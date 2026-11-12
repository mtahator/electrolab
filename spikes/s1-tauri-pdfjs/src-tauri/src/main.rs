//! Spike P0.S1 — Tauri + PDF.js on WebView2. Throw-away code.
//!
//! * `ep` protocol (`http://ep.localhost/file?path=…` on Windows) serves PDF bytes with
//!   HTTP range support, like the product's `ep://blob/<hash>` (technical-structure.md §7.5).
//!   Only files under the fixture folders are served.
//! * `doc_info` returns every page size from PDFium without loading pages, so the viewer
//!   can lay out the whole document before PDF.js has parsed anything.
//! * Bench mode: `s1-tauri-pdfjs --bench <open|scroll> --pdf <path> --out <file.json>`
//!   runs one measurement in the page, writes the JSON, and exits.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

use pdfium_render::prelude::*;
use serde::Serialize;
use tauri::http::{Request, Response, StatusCode};
use tauri::{AppHandle, Manager, State};

static REQUESTS: AtomicU64 = AtomicU64::new(0);
static BYTES: AtomicU64 = AtomicU64::new(0);

fn repo_root() -> PathBuf {
    // spikes/s1-tauri-pdfjs/src-tauri -> repository root
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..")
}

fn allowed(path: &Path) -> Option<PathBuf> {
    let p = path.canonicalize().ok()?;
    let roots = [repo_root().join(".fixtures/papers"), repo_root().join("fixtures/papers")];
    roots
        .iter()
        .filter_map(|r| r.canonicalize().ok())
        .any(|r| p.starts_with(&r))
        .then_some(p)
}

struct Pdf(Mutex<Pdfium>);

#[derive(Default)]
struct Bench(Option<serde_json::Value>, Option<PathBuf>);

#[derive(Serialize)]
struct DocInfo {
    bytes: u64,
    pages: Vec<[f32; 2]>,
    pdfium_ms: f64,
}

#[tauri::command]
fn doc_info(pdf: State<'_, Pdf>, path: String) -> Result<DocInfo, String> {
    let path = allowed(Path::new(&path)).ok_or("path not allowed")?;
    let t = Instant::now();
    let pdfium = pdf.0.lock().map_err(|e| e.to_string())?;
    let doc = pdfium.load_pdf_from_file(&path, None).map_err(|e| e.to_string())?;
    let pages = doc
        .pages()
        .page_sizes()
        .map_err(|e| e.to_string())?
        .iter()
        .map(|r| [r.width().value, r.height().value])
        .collect();
    let bytes = std::fs::metadata(&path).map_err(|e| e.to_string())?.len();
    Ok(DocInfo { bytes, pages, pdfium_ms: t.elapsed().as_secs_f64() * 1000.0 })
}

#[tauri::command]
fn list_fixtures() -> Vec<String> {
    let mut out = Vec::new();
    for dir in [repo_root().join(".fixtures/papers"), repo_root().join("fixtures/papers")] {
        if let Ok(rd) = std::fs::read_dir(dir) {
            for e in rd.flatten() {
                let p = e.path();
                if p.extension().is_some_and(|x| x == "pdf") {
                    if let Some(p) = allowed(&p) {
                        out.push(p.to_string_lossy().into_owned());
                    }
                }
            }
        }
    }
    out.sort();
    out
}

#[tauri::command]
fn protocol_stats(reset: bool) -> [u64; 2] {
    let s = [REQUESTS.load(Ordering::Relaxed), BYTES.load(Ordering::Relaxed)];
    if reset {
        REQUESTS.store(0, Ordering::Relaxed);
        BYTES.store(0, Ordering::Relaxed);
    }
    s
}

#[tauri::command]
fn bench_config(bench: State<'_, Mutex<Bench>>) -> Option<serde_json::Value> {
    bench.lock().ok().and_then(|b| b.0.clone())
}

/// Write `<out>.targets.json` without exiting: lets an outside script send real mouse input.
#[tauri::command]
fn note(bench: State<'_, Mutex<Bench>>, value: serde_json::Value) -> Result<(), String> {
    let out = bench.lock().map_err(|e| e.to_string())?.1.clone().ok_or("no --out")?;
    std::fs::write(out.with_extension("targets.json"), value.to_string()).map_err(|e| e.to_string())
}

#[tauri::command]
fn report(app: AppHandle, bench: State<'_, Mutex<Bench>>, result: serde_json::Value) -> Result<(), String> {
    let out = bench.lock().map_err(|e| e.to_string())?.1.clone();
    if let Some(out) = out {
        std::fs::write(&out, serde_json::to_vec_pretty(&result).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
        app.exit(0);
    } else {
        println!("{result}");
    }
    Ok(())
}

/// GET /file?path=<percent-encoded absolute path>, with optional `Range: bytes=a-b`.
fn serve(req: &Request<Vec<u8>>) -> Response<Vec<u8>> {
    let cors = |b: tauri::http::response::Builder| {
        b.header("Access-Control-Allow-Origin", "*")
            .header("Access-Control-Allow-Headers", "Range")
            .header("Access-Control-Expose-Headers", "Content-Range, Content-Length, Accept-Ranges")
    };
    let fail = |code: StatusCode, msg: &str| {
        cors(Response::builder().status(code)).body(msg.as_bytes().to_vec()).unwrap_or_default()
    };
    if req.method() == "OPTIONS" {
        return cors(Response::builder().status(StatusCode::NO_CONTENT)).body(Vec::new()).unwrap_or_default();
    }
    let query = req.uri().query().unwrap_or("");
    let Some(raw) = query.strip_prefix("path=") else { return fail(StatusCode::BAD_REQUEST, "path") };
    let Some(path) = allowed(Path::new(&percent_decode(raw))) else {
        return fail(StatusCode::FORBIDDEN, "not allowed");
    };
    let Ok(mut file) = File::open(&path) else { return fail(StatusCode::NOT_FOUND, "missing") };
    let len = file.metadata().map(|m| m.len()).unwrap_or(0);
    let range = req
        .headers()
        .get("range")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("bytes="))
        .and_then(|v| v.split_once('-'))
        .and_then(|(a, b)| {
            let a: u64 = a.parse().ok()?;
            let b: u64 = if b.is_empty() { len.saturating_sub(1) } else { b.parse().ok()? };
            (a <= b && b < len).then_some((a, b))
        });
    let (start, end, status) = match range {
        Some((a, b)) => (a, b, StatusCode::PARTIAL_CONTENT),
        None => (0, len.saturating_sub(1), StatusCode::OK),
    };
    let mut body = vec![0u8; usize::try_from(end + 1 - start).unwrap_or(0)];
    if file.seek(SeekFrom::Start(start)).is_err() || file.read_exact(&mut body).is_err() {
        return fail(StatusCode::INTERNAL_SERVER_ERROR, "read");
    }
    REQUESTS.fetch_add(1, Ordering::Relaxed);
    BYTES.fetch_add(body.len() as u64, Ordering::Relaxed);
    let mut b = cors(Response::builder().status(status))
        .header("Content-Type", "application/pdf")
        .header("Accept-Ranges", "bytes")
        .header("Content-Length", body.len().to_string());
    if status == StatusCode::PARTIAL_CONTENT {
        b = b.header("Content-Range", format!("bytes {start}-{end}/{len}"));
    }
    b.body(body).unwrap_or_default()
}

fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Ok(v) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(if bytes[i] == b'+' { b' ' } else { bytes[i] });
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn bench_from_args() -> Bench {
    let args: Vec<String> = std::env::args().collect();
    let get = |k: &str| args.iter().position(|a| a == k).and_then(|i| args.get(i + 1)).cloned();
    match (get("--bench"), get("--pdf"), get("--out")) {
        (Some(mode), Some(pdf), Some(out)) => Bench(
            Some(serde_json::json!({ "mode": mode, "pdf": pdf, "pages": get("--pages") })),
            Some(PathBuf::from(out)),
        ),
        _ => Bench::default(),
    }
}

fn main() -> anyhow::Result<()> {
    let lib = Pdfium::pdfium_platform_library_name_at_path(&repo_root().join(".natives/pdfium/bin"));
    let pdfium = Pdfium::new(Pdfium::bind_to_library(lib)?);
    tauri::Builder::default()
        .manage(Pdf(Mutex::new(pdfium)))
        .manage(Mutex::new(bench_from_args()))
        .register_asynchronous_uri_scheme_protocol("ep", |_ctx, request, responder| {
            std::thread::spawn(move || responder.respond(serve(&request)));
        })
        .invoke_handler(tauri::generate_handler![
            doc_info,
            list_fixtures,
            protocol_stats,
            bench_config,
            note,
            report
        ])
        .setup(|app| {
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.set_focus();
            }
            Ok(())
        })
        .run(tauri::generate_context!())?;
    Ok(())
}
