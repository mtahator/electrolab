//! `ElectroPaper` Reader — the Tauri host (P1.M1.HST.1).
//!
//! Builds the engines on one bus, exposes the bus to the window through two Tauri commands
//! (`bus_call`, `bus_cancel`) limited by `allowed-commands.json`, forwards hub events to the
//! window, and serves stored files through the `ep` scheme. No engine knows about Tauri.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod blob;

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;

use ep_bus::{Bus, CommandError, Envelope};
use ep_document::{BlobAccess, Document};
use ep_types::Hash;
use ep_workspace::Workspace;
use serde_json::Value;
use tauri::{Emitter, Manager, State};
use uuid::Uuid;

/// Product name for per-user folders (final name pending).
const APP_NAME: &str = "ElectroPaper";

struct AppState {
    bus: Arc<Bus>,
    allowed: HashSet<String>,
}

struct WorkspaceBlobs(Arc<Workspace>);

impl BlobAccess for WorkspaceBlobs {
    fn path_of(&self, hash: &Hash) -> Option<PathBuf> {
        self.0.blob_path(hash)
    }
}

/// Call a bus command from the window.
#[tauri::command]
async fn bus_call(
    state: State<'_, AppState>,
    command: String,
    input: Value,
    deadline_ms: Option<u64>,
    approved: Vec<String>,
) -> Result<Value, CommandError> {
    if !state.allowed.contains(&command) {
        return Err(CommandError::Denied {
            rule: "reader.allowed_commands".into(),
            reason: format!("{command} is not available in this window"),
        });
    }
    let mut env = Envelope::new(command, input);
    env.deadline_ms = deadline_ms;
    env.approved = approved.into_iter().collect();
    state.bus.call(env).await
}

/// Cancel a running call.
#[tauri::command]
#[allow(clippy::needless_pass_by_value)] // Tauri passes managed state by value
fn bus_cancel(state: State<'_, AppState>, id: Uuid) -> bool {
    state.bus.cancel(id)
}

fn allowed_commands() -> anyhow::Result<HashSet<String>> {
    let v: Value = serde_json::from_str(include_str!("../allowed-commands.json"))?;
    Ok(v["main"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|c| c.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default())
}

/// App folders: `EP_DATA_DIR` (portable install, tests) or the user's standard folders.
fn dirs() -> anyhow::Result<Arc<dyn ep_os::AppDirs>> {
    match std::env::var_os("EP_DATA_DIR") {
        Some(root) => Ok(Arc::new(ep_os::RootDirs(PathBuf::from(root)))),
        None => os_dirs(),
    }
}

#[cfg(windows)]
fn os_dirs() -> anyhow::Result<Arc<dyn ep_os::AppDirs>> {
    Ok(Arc::new(ep_os_windows::WinDirs::new(APP_NAME)?))
}

#[cfg(not(windows))]
fn os_dirs() -> anyhow::Result<Arc<dyn ep_os::AppDirs>> {
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_default();
    Ok(Arc::new(ep_os::RootDirs(home.join(".electropaper"))))
}

fn main() -> anyhow::Result<()> {
    let dirs = dirs()?;
    let log_file = dirs.logs().join("reader.jsonl");
    let output = if cfg!(debug_assertions) {
        ep_telemetry::Output::Console
    } else {
        ep_telemetry::Output::JsonFile(&log_file)
    };
    let _ = ep_telemetry::init(output, "info");

    let bus = Arc::new(Bus::default());
    let workspace = Workspace::new(dirs);
    workspace.register(&bus)?;
    Document::new(Arc::new(WorkspaceBlobs(workspace.clone()))).register(&bus)?;

    let protocol_ws = workspace.clone();
    let events_bus = bus.clone();
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(AppState {
            bus,
            allowed: allowed_commands()?,
        })
        .register_asynchronous_uri_scheme_protocol("ep", move |_ctx, request, responder| {
            let ws = protocol_ws.clone();
            std::thread::spawn(move || responder.respond(blob::serve(&ws, &request)));
        })
        .invoke_handler(tauri::generate_handler![bus_call, bus_cancel])
        .setup(move |app| {
            // Hub events go to the window; the publisher never waits for the UI.
            let handle = app.handle().clone();
            let mut sub = events_bus.events().subscribe("");
            tauri::async_runtime::spawn(async move {
                loop {
                    match sub.recv().await {
                        ep_bus::Recv::Event(e) => {
                            let _ = handle.emit("bus-event", &e);
                        }
                        ep_bus::Recv::Missed(n) => {
                            let _ = handle.emit(
                                "bus-event",
                                serde_json::json!({ "kind": "bus.missed", "data": n, "at_ms": 0 }),
                            );
                        }
                        ep_bus::Recv::Closed => break,
                    }
                }
            });
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.set_focus();
            }
            Ok(())
        })
        .run(tauri::generate_context!())?;
    Ok(())
}
