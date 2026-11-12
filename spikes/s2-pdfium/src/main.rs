//! Spike P0.S2 — PDFium text and geometry.
//!
//! usage: s2-pdfium <pdfium-dir> <out-dir> <pdf>...
//!
//! Per paper writes, into <out-dir>:
//!   <stem>.pdfium.txt          page texts, separated by form feed
//!   <stem>.pdfium.chars.json   per page: width, height, chars [u, origin x, origin y, l, b, r, t]
//!                              (PDF user space, points, y up — the same space as PDF.js transforms)
//! and prints one JSON summary line per paper to stdout.
//!
//! Throw-away code: unwrap-free but not product quality.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::time::Instant;

use pdfium_render::prelude::*;
use serde_json::json;

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    anyhow::ensure!(args.len() >= 3, "usage: s2-pdfium <pdfium-dir> <out-dir> <pdf>...");
    let lib = Pdfium::pdfium_platform_library_name_at_path(&args[0]);
    let pdfium = Pdfium::new(Pdfium::bind_to_library(lib)?);
    let out = PathBuf::from(&args[1]);
    std::fs::create_dir_all(&out)?;
    for pdf in &args[2..] {
        match run(&pdfium, Path::new(pdf), &out) {
            Ok(summary) => println!("{summary}"),
            Err(e) => println!("{}", json!({ "file": pdf, "error": format!("{e:#}") })),
        }
    }
    Ok(())
}

fn run(pdfium: &Pdfium, pdf: &Path, out: &Path) -> anyhow::Result<serde_json::Value> {
    let stem = pdf.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();

    let t = Instant::now();
    let doc = pdfium.load_pdf_from_file(pdf, None)?;
    let pages = doc.pages();
    let n = pages.len();
    let open_ms = ms(t);

    // Pass 1: text only (what search indexing and chat need).
    let t = Instant::now();
    let mut text = String::new();
    let (mut load_s, mut slowest_page) = (0.0_f64, (0.0_f64, 0));
    for i in 0..n {
        let tl = Instant::now();
        let page = pages.get(i)?; // parses the page content stream (figures included)
        let l = tl.elapsed().as_secs_f64();
        load_s += l;
        if l > slowest_page.0 {
            slowest_page = (l, i + 1);
        }
        text.push_str(&page.text()?.all());
        text.push('\u{c}');
    }
    let text_ms = ms(t);
    let load_ms = (load_s * 100_000.0).round() / 100.0;

    // Pass 2: text with per-character geometry (what selection, highlights and evidence need).
    let t = Instant::now();
    let mut page_json = Vec::with_capacity(usize::try_from(n).unwrap_or(0));
    let (mut chars_total, mut no_bounds, mut replacement, mut private_use, mut ligatures) = (0, 0, 0, 0, 0);
    let (mut rotated, mut surrogate_pairs, mut unpaired) = (0, 0, 0);
    for page in pages.iter() {
        if !matches!(page.rotation(), Ok(PdfPageRenderRotation::None)) {
            rotated += 1;
        }
        let page_text = page.text()?;
        let mut chars = Vec::new();
        // FPDFText_GetUnicode returns UTF-16 code units: characters outside the BMP
        // (e.g. mathematical italic 𝐻, U+1D43B) arrive as a high + low surrogate pair
        // on two consecutive text "chars". Pair them; keep the first unit's geometry.
        let mut high: Option<u32> = None;
        for ch in page_text.chars().iter() {
            let v = ch.unicode_value();
            let u = match (high.take(), v) {
                (None, 0xD800..=0xDBFF) => {
                    high = Some(v);
                    continue;
                }
                (Some(h), 0xDC00..=0xDFFF) => {
                    surrogate_pairs += 1;
                    char::from_u32(0x10000 + ((h - 0xD800) << 10) + (v - 0xDC00))
                }
                _ => char::from_u32(v),
            };
            let Some(u) = u else {
                unpaired += 1;
                continue;
            };
            chars_total += 1;
            match u {
                '\u{FFFD}' => replacement += 1,
                '\u{E000}'..='\u{F8FF}' => private_use += 1,
                '\u{FB00}'..='\u{FB06}' => ligatures += 1,
                _ => {}
            }
            let (Ok((ox, oy)), Ok(lb)) = (ch.origin(), ch.loose_bounds()) else {
                no_bounds += 1;
                continue;
            };
            chars.push(json!([
                u.to_string(),
                r2(ox.value), r2(oy.value),
                r2(lb.left().value), r2(lb.bottom().value), r2(lb.right().value), r2(lb.top().value)
            ]));
        }
        page_json.push(json!({ "w": r2(page.width().value), "h": r2(page.height().value), "chars": chars }));
    }
    let chars_ms = ms(t);

    let bookmarks = doc.bookmarks().iter().count();
    let mut links = 0usize;
    for page in pages.iter() {
        links += usize::try_from(page.links().len()).unwrap_or(0);
    }

    std::fs::write(out.join(format!("{stem}.pdfium.txt")), &text)?;
    std::fs::write(out.join(format!("{stem}.pdfium.chars.json")), serde_json::to_vec(&page_json)?)?;

    let mut note = String::new();
    if rotated > 0 {
        let _ = write!(note, "{rotated} rotated page(s); ");
    }
    Ok(json!({
        "file": stem, "pages": n, "open_ms": open_ms,
        "text_ms": text_ms, "chars_ms": chars_ms,
        "page_load_ms": load_ms, "page_load_share": r2(load_ms / text_ms.max(0.001)),
        "slowest_page": slowest_page.1, "slowest_page_load_ms": r2(slowest_page.0 * 1000.0),
        "text_ms_per_100p": r2(text_ms / f64::from(n.max(1)) * 100.0),
        "chars_ms_per_100p": r2(chars_ms / f64::from(n.max(1)) * 100.0),
        "chars": chars_total, "chars_without_bounds": no_bounds,
        "replacement_chars": replacement, "private_use_chars": private_use, "ligature_chars": ligatures,
        "surrogate_pairs": surrogate_pairs, "unpaired_units": unpaired,
        "bookmarks": bookmarks, "links": links, "note": note,
    }))
}

fn ms(t: Instant) -> f64 {
    (t.elapsed().as_secs_f64() * 1000.0 * 100.0).round() / 100.0
}

fn r2(v: impl Into<f64>) -> f64 {
    (v.into() * 100.0).round() / 100.0
}
