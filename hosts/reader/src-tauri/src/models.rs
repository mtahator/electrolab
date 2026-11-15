//! Page models (ADR-007): kept in the app's `models` folder (or `EP_MODELS_DIR`), downloaded on
//! the user's request from their pinned address, resumable, checked against their SHA-256 before
//! use. The Document engine loads them when they are present.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::Duration;

use ep_document::Document;
use serde_json::json;
use sha2::{Digest, Sha256};
use tauri::{AppHandle, Emitter, State};

use crate::AppState;

/// Where models live.
pub fn models_dir(dirs: &dyn ep_os::AppDirs) -> PathBuf {
    std::env::var_os("EP_MODELS_DIR").map_or_else(|| dirs.data().join("models"), PathBuf::from)
}

fn sha256_of(path: &Path) -> std::io::Result<String> {
    let mut f = std::fs::File::open(path)?;
    let mut h = Sha256::new();
    let mut buf = vec![0u8; 1 << 20];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
    }
    let mut out = String::with_capacity(64);
    for b in h.finalize() {
        use std::fmt::Write as _;
        let _ = write!(out, "{b:02x}");
    }
    Ok(out)
}

/// Download a page model (the user asked for it), verify it and load it.
#[tauri::command]
pub async fn fetch_model(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
) -> Result<(), String> {
    let spec = *Document::model_specs()
        .iter()
        .find(|m| m.id == id)
        .ok_or_else(|| format!("unknown model {id}"))?;
    let dir = state.models_dir.clone();
    let dest = spec.path(&dir);
    let part = dest.with_extension("part");
    std::fs::create_dir_all(dest.parent().unwrap_or(&dir)).map_err(|e| e.to_string())?;
    let mut cmd = std::process::Command::new("curl.exe");
    cmd.args([
        "--location",
        "--fail",
        "--silent",
        "--show-error",
        "--proto",
        "=https",
        "--max-redirs",
        "10",
        "--retry",
        "3",
        "--continue-at",
        "-",
        "--user-agent",
        "ElectroPaper-Reader",
        "--output",
    ])
    .arg(&part)
    .arg(spec.url)
    .stdin(std::process::Stdio::null())
    .stdout(std::process::Stdio::null())
    .stderr(std::process::Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    let mut child = cmd
        .spawn()
        .map_err(|e| format!("the download tool could not start: {e}"))?;
    // Progress from the size of the partial file.
    let status = loop {
        if let Some(s) = child.try_wait().map_err(|e| e.to_string())? {
            break s;
        }
        let received = std::fs::metadata(&part).map_or(0, |m| m.len());
        let _ = app.emit(
            "model-progress",
            json!({ "id": spec.id, "received": received, "total": spec.size }),
        );
        tokio::time::sleep(Duration::from_millis(400)).await;
    };
    if !status.success() {
        let mut msg = String::new();
        if let Some(mut e) = child.stderr.take() {
            let _ = e.read_to_string(&mut msg);
        }
        return Err(if msg.trim().is_empty() {
            "the download failed".into()
        } else {
            msg.trim().to_string()
        });
    }
    let p = part.clone();
    let hash = tauri::async_runtime::spawn_blocking(move || sha256_of(&p))
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())?;
    if hash != spec.sha256 {
        let _ = std::fs::remove_file(&part);
        return Err(format!(
            "the downloaded {} did not match its expected checksum and was removed",
            spec.id
        ));
    }
    std::fs::rename(&part, &dest).map_err(|e| e.to_string())?;
    let _ = app.emit(
        "model-progress",
        json!({ "id": spec.id, "received": spec.size, "total": spec.size, "done": true }),
    );
    state.document.load_page_models(&state.model_dirs)?;
    Ok(())
}

/// Remove a downloaded page model (Settings). Models shipped with the app cannot be removed.
#[tauri::command]
pub fn remove_model(state: State<'_, AppState>, id: String) -> Result<(), String> {
    let spec = *Document::model_specs()
        .iter()
        .find(|m| m.id == id)
        .ok_or_else(|| format!("unknown model {id}"))?;
    if spec.bundled {
        return Err(format!("{id} is part of the app and cannot be removed"));
    }
    let path = spec.path(&state.models_dir);
    if path.exists() {
        std::fs::remove_file(&path).map_err(|e| e.to_string())?;
    }
    let _ = std::fs::remove_dir(path.parent().unwrap_or(&state.models_dir));
    state.document.load_page_models(&state.model_dirs)?;
    Ok(())
}

/// Name of the flag the installer leaves next to the app when the user chose, while installing,
/// to get the layout model (the download itself runs in the app, verified like any other).
const INSTALL_CHOICE: &str = "get-layout-model";

/// Whether the user asked for the layout model while installing; asked once (the flag is removed).
#[tauri::command]
pub fn take_install_choice() -> bool {
    let Some(dir) = std::env::current_exe()
        .ok()
        .and_then(|e| e.parent().map(Path::to_path_buf))
    else {
        return false;
    };
    let flag = dir.join(INSTALL_CHOICE);
    flag.exists() && std::fs::remove_file(&flag).is_ok()
}
