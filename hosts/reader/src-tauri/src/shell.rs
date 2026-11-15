//! Window commands that hand things to the operating system (P1.M2.VW.6, P1.M2.SH.2, P1.M2.SH.5):
//! open a link in the browser, open or print a paper with the user's usual app, and PDFs given
//! to the app on the command line or by a second launch (double-click in Explorer).

use std::path::{Path, PathBuf};

use ep_types::Hash;
use tauri::{Emitter, Manager, State};

use crate::AppState;

/// PDF files among command-line arguments (the first argument is the program).
pub fn pdf_args(args: &[String]) -> Vec<String> {
    args.iter()
        .skip(1)
        .filter(|a| {
            Path::new(a)
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("pdf"))
                && Path::new(a).is_file()
        })
        .cloned()
        .collect()
}

fn paper_path(state: &AppState, hash: &str) -> Result<PathBuf, String> {
    let hash: Hash = hash.parse().map_err(|_| "not a file hash".to_string())?;
    state
        .workspace
        .blob_path(&hash)
        .ok_or_else(|| "the paper is not in the open library".to_string())
}

/// Open a web or mail address in the user's browser or mail app (`http`, `https`, `mailto`).
#[tauri::command]
#[allow(clippy::needless_pass_by_value)] // Tauri passes managed state by value
pub fn open_external(state: State<'_, AppState>, url: String) -> Result<(), String> {
    if !ep_os::is_safe_url(&url) {
        return Err(format!(
            "not opened: only http, https and mailto links are allowed ({url})"
        ));
    }
    state.shell.open_url(&url).map_err(|e| e.to_string())
}

/// Open the paper's managed copy in the user's usual PDF app.
#[tauri::command]
#[allow(clippy::needless_pass_by_value)]
pub fn open_paper_externally(state: State<'_, AppState>, hash: String) -> Result<(), String> {
    state
        .shell
        .open_path(&paper_path(&state, &hash)?)
        .map_err(|e| e.to_string())
}

/// Print the paper with the user's usual PDF app.
#[tauri::command]
#[allow(clippy::needless_pass_by_value)]
pub fn print_paper(state: State<'_, AppState>, hash: String) -> Result<(), String> {
    state
        .shell
        .print(&paper_path(&state, &hash)?)
        .map_err(|e| e.to_string())
}

/// PDFs waiting to be opened (given at start-up); taking them empties the list.
#[tauri::command]
#[allow(clippy::needless_pass_by_value)]
pub fn take_pending_files(state: State<'_, AppState>) -> Vec<String> {
    std::mem::take(&mut *state.pending.lock())
}

/// A second launch (e.g. double-clicking a PDF while the app runs): bring the window forward
/// and pass its PDFs to the window.
pub fn on_second_instance(app: &tauri::AppHandle, args: &[String]) {
    let files = pdf_args(args);
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
    if !files.is_empty() {
        let _ = app.emit("open-files", files);
    }
}

/// Where downloads wait until they are added to the library (emptied at start-up).
pub fn downloads_dir() -> PathBuf {
    std::env::temp_dir().join("electropaper-downloads")
}

/// Download a PDF from a web address the user gave (P1.M2.SH.7) with the system `curl`
/// (http and https only, at most 200 MB, 2 minutes). Returns the downloaded file's path; the
/// window then adds it to the library with the address kept as its source.
#[tauri::command]
pub async fn download_pdf(url: String) -> Result<String, String> {
    let url = url.trim().to_string();
    let lower = url.to_ascii_lowercase();
    if !(lower.starts_with("https://") || lower.starts_with("http://"))
        || url.contains(char::is_whitespace)
    {
        return Err("only http and https addresses can be opened".into());
    }
    let dir = downloads_dir();
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    let file = dir.join(format!("{nanos}.pdf"));
    let target = file.clone();
    let out = tauri::async_runtime::spawn_blocking(move || {
        let mut cmd = std::process::Command::new("curl.exe");
        cmd.args([
            "--location",
            "--fail",
            "--silent",
            "--show-error",
            "--proto",
            "=https,http",
            "--max-redirs",
            "10",
            "--max-filesize",
            "209715200",
            "--max-time",
            "120",
            "--user-agent",
            "ElectroPaper-Reader",
            "--output",
        ])
        .arg(&target)
        .arg(&url)
        .stdin(std::process::Stdio::null());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
        }
        cmd.output()
    })
    .await
    .map_err(|e| e.to_string())?
    .map_err(|e| format!("the download tool could not start: {e}"))?;
    if !out.status.success() {
        let _ = std::fs::remove_file(&file);
        let msg = String::from_utf8_lossy(&out.stderr).trim().to_string();
        return Err(if msg.is_empty() {
            "the download failed".into()
        } else {
            msg
        });
    }
    Ok(file.display().to_string())
}

/// Remove downloads left from an earlier run.
pub fn clean_downloads() {
    if let Ok(entries) = std::fs::read_dir(downloads_dir()) {
        for e in entries.flatten() {
            let _ = std::fs::remove_file(e.path());
        }
    }
}

/// Write text the user chose to save (a table as CSV, notes as Markdown) to the path they picked
/// in the save dialog. Only text formats are written.
#[tauri::command]
#[allow(clippy::needless_pass_by_value)] // Tauri passes command arguments by value
pub fn save_text_file(path: String, contents: String) -> Result<(), String> {
    let p = std::path::Path::new(&path);
    let ext = p
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase);
    if !matches!(ext.as_deref(), Some("csv" | "md" | "txt" | "bib" | "typ")) {
        return Err("only .csv, .md, .txt, .bib and .typ files can be saved here".into());
    }
    std::fs::write(p, contents).map_err(|e| e.to_string())
}
