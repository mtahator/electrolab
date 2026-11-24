//! `ElectroPaper` Reader — the Tauri host (P1.M1.HST.1).
//!
//! Builds the engines on one bus, exposes the bus to the window through two Tauri commands
//! (`bus_call`, `bus_cancel`) limited by `allowed-commands.json`, forwards hub events to the
//! window, and serves stored files through the `ep` scheme. No engine knows about Tauri.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod blob;
mod models;
mod shell;

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

struct AppState {
    bus: Arc<Bus>,
    document: Arc<Document>,
    /// Where downloaded page models are kept.
    models_dir: PathBuf,
    /// Every folder with page models (bundled first).
    model_dirs: Vec<PathBuf>,
    allowed: HashSet<String>,
    workspace: Arc<Workspace>,
    shell: Arc<dyn ep_os::ShellOpen>,
    /// PDFs given on the command line, until the window takes them.
    pending: parking_lot::Mutex<Vec<String>>,
}

struct WorkspaceBlobs(Arc<Workspace>);

impl BlobAccess for WorkspaceBlobs {
    fn path_of(&self, hash: &Hash) -> Option<PathBuf> {
        self.0.blob_path(hash)
    }
    fn put(
        &self,
        bytes: &[u8],
        media_type: ep_types::MediaType,
    ) -> std::io::Result<ep_types::BlobRef> {
        self.0.store_blob(bytes, media_type)
    }
    fn project(&self) -> Option<(String, PathBuf)> {
        self.0
            .project_folder()
            .map(|(id, dir)| (id.to_string(), dir))
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
    id: Option<Uuid>,
) -> Result<Value, CommandError> {
    if !state.allowed.contains(&command) {
        return Err(CommandError::Denied {
            rule: "reader.allowed_commands".into(),
            reason: format!("{command} is not available in this window"),
        });
    }
    let mut env = Envelope::new(command, input);
    // The window may choose the id, so that it can cancel the call (`bus_cancel`).
    if let Some(id) = id {
        env.id = id;
    }
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
    Ok(Arc::new(ep_os_windows::WinDirs::new(ep_config::PRODUCT)?))
}

#[cfg(not(windows))]
fn os_dirs() -> anyhow::Result<Arc<dyn ep_os::AppDirs>> {
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_default();
    Ok(Arc::new(ep_os::RootDirs(home.join(".electropaper"))))
}

/// Starts helper processes tied to the app's lifetime (ADR-009).
#[cfg(windows)]
fn spawner() -> anyhow::Result<Arc<dyn ep_os::ProcessSpawner>> {
    Ok(Arc::new(ep_os_windows::JobSpawner::new()?))
}

#[cfg(not(windows))]
#[allow(clippy::unnecessary_wraps)] // same signature on every system
fn spawner() -> anyhow::Result<Arc<dyn ep_os::ProcessSpawner>> {
    Ok(Arc::new(ep_os::fake::FakeSpawner::default()))
}

#[allow(clippy::too_many_lines)] // one place wires the whole app
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
    let renders = dirs.cache().join("renders");
    let dirs_for_models = dirs.clone();
    let workspace = Workspace::new(dirs);
    workspace.register(&bus)?;
    let pdfium = ep_document::find_pdfium().and_then(|d| match ep_document::start_pdfium(&d) {
        Ok(b) => Some(b),
        Err(e) => {
            tracing::error!(%e, "PDFium could not be loaded");
            None
        }
    });
    let document = Document::new(
        Arc::new(WorkspaceBlobs(workspace.clone())),
        pdfium,
        Some(&renders),
    )?;
    // New annotations carry the signed-in user's name as their author (`/T` in saved PDFs).
    if let Some(name) = std::env::var_os("USERNAME").or_else(|| std::env::var_os("USER")) {
        document.set_author(&name.to_string_lossy());
    }
    document.register(&bus)?;
    // Page models (layout) load when present; preparation works from the text alone otherwise.
    // Bundled models (text recognition) first, then the ones the user downloaded.
    let models_dir = models::models_dir(dirs_for_models.as_ref());
    let model_dirs: Vec<PathBuf> = ep_document::bundled_models_dir()
        .into_iter()
        .chain([models_dir.clone()])
        .collect();
    if let Err(e) = document.load_page_models(&model_dirs) {
        tracing::warn!(%e, "page models could not be loaded");
    }

    // Language models (M5, ADR-005): GGUF files in the app's `models/llm` folder (and
    // `EP_LLM_DIR`), run by the llama.cpp build next to the app or in `.natives` (development).
    let llm_dirs: Vec<PathBuf> = std::env::var_os("EP_LLM_DIR")
        .map(PathBuf::from)
        .into_iter()
        .chain([models_dir.join("llm")])
        .collect();
    let inference = ep_inference::Inference::new(
        llm_dirs,
        ep_inference::runtime_dirs(),
        spawner()?,
        Some(dirs_for_models.logs().join("llama-server.log")),
        Some(models_dir.join("llm.json")),
    );
    for dir in models::take_old_llm_folders(&models_dir) {
        if let Err(e) = inference.add_folder(&dir) {
            tracing::warn!(%e, "a model folder could not be taken over");
        }
    }
    inference.register(&bus)?;
    ep_knowledge::Knowledge::new().register(&bus)?;
    ep_understanding::Understanding::new().register(&bus)?;

    #[cfg(windows)]
    let os_shell: Arc<dyn ep_os::ShellOpen> = Arc::new(ep_os_windows::WinShell);
    #[cfg(not(windows))]
    let os_shell: Arc<dyn ep_os::ShellOpen> = Arc::new(ep_os::fake::Recorder::default());
    shell::clean_downloads();
    let startup_files = shell::pdf_args(&std::env::args().collect::<Vec<_>>());

    let protocol_ws = workspace.clone();
    let protocol_doc = document.clone();
    let events_bus = bus.clone();
    let mut builder = tauri::Builder::default();
    // One window per user; a run with its own data folder (tests, EP_DATA_DIR) is separate, so
    // it can run next to the user's own Reader.
    if std::env::var_os("EP_DATA_DIR").is_none() {
        builder = builder.plugin(tauri_plugin_single_instance::init(|app, args, _cwd| {
            shell::on_second_instance(app, &args);
        }));
    }
    builder
        .plugin(tauri_plugin_dialog::init())
        .manage(AppState {
            bus,
            document: document.clone(),
            models_dir,
            model_dirs,
            allowed: allowed_commands()?,
            workspace: workspace.clone(),
            shell: os_shell,
            pending: parking_lot::Mutex::new(startup_files),
        })
        .register_asynchronous_uri_scheme_protocol("ep", move |_ctx, request, responder| {
            let (ws, doc) = (protocol_ws.clone(), protocol_doc.clone());
            std::thread::spawn(move || responder.respond(blob::serve(&ws, &doc, &request)));
        })
        .invoke_handler(tauri::generate_handler![
            bus_call,
            bus_cancel,
            shell::open_external,
            shell::open_paper_externally,
            shell::print_paper,
            shell::take_pending_files,
            shell::download_pdf,
            models::fetch_model,
            models::remove_model,
            models::take_install_choice,
            shell::save_text_file
        ])
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
