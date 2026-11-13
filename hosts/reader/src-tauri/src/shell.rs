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
