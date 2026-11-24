//! Inference engine facade: the commands of `engines/inference/CONTRACT.md` on the bus.
//!
//! It finds the local models and the llama.cpp builds, chooses one (ADR-005), starts the
//! sidecar on first use, streams answers as `inference.token` events, and checks answers
//! against their schema with one retry. Tests replace the backend with `Replay`.

mod catalog;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Weak};
use std::time::Instant;

use catalog::{Catalog, Choices, Found};
use ep_bus::{Bus, CommandError, Cx, Permission, RegisterError, Spec, Stability};
use ep_inference_adapter_llamacpp::{Config, LlamaCppSidecar};
pub use ep_inference_contract as contract;
use ep_inference_contract::{
    Assign, Backend, Folder, FolderPath, LlmRequest, LlmResponse, LoadRequest, ModelInfo,
    ModelKind, ModelState, Models, RunState, Runtime, Sensitivity, Status, Task, Usage,
    commands as cmd, errors, events,
};
pub use ep_inference_core::Replay;
pub use ep_inference_core::{
    BackendError, Completion, InferenceBackend, IsCancelled, OnToken, Prepared,
};
use ep_inference_core::{prepare, schema};
use ep_os::ProcessSpawner;
use parking_lot::{Mutex, RwLock};
use serde_json::json;

/// Every kind of work a model can be chosen for.
const DEFAULT: &str = "default";
const TASKS: [&str; 4] = [DEFAULT, "explain", "check", "sort"];

/// The kind of work of a request's task.
fn task_of(task: &str) -> &'static str {
    match task {
        "explain.draft" => "explain",
        "explain.check" => "check",
        "sort.names" | "sort.assign" => "sort",
        _ => DEFAULT,
    }
}

/// The engine.
pub struct Inference {
    /// Folders that are always searched; the user's own ones are in `choices`.
    base_dirs: Vec<PathBuf>,
    choices: RwLock<Choices>,
    choices_file: Option<PathBuf>,
    catalog: RwLock<Option<Arc<Catalog>>>,
    headers: Mutex<catalog::Cache>,
    problems: Mutex<HashMap<String, String>>,
    /// One answer at a time: another model may have to be loaded for the next one.
    turn: tokio::sync::Mutex<()>,
    runtime_dirs: Vec<PathBuf>,
    spawner: Option<Arc<dyn ProcessSpawner>>,
    log_file: Option<PathBuf>,
    /// A fixed backend (tests, `Replay`); otherwise the sidecar.
    fixed: Option<Arc<dyn InferenceBackend>>,
    sidecar: RwLock<Option<Arc<LlamaCppSidecar>>>,
    status: Mutex<Status>,
    /// One start at a time.
    starting: tokio::sync::Mutex<()>,
    bus: RwLock<Weak<Bus>>,
}

fn failed(kind: &str, message: impl Into<String>) -> CommandError {
    CommandError::Failed {
        kind: kind.into(),
        message: message.into(),
    }
}

fn from_backend(e: BackendError) -> CommandError {
    match e {
        BackendError::NoModel(m) => {
            failed(errors::NO_MODEL, format!("no language model found ({m})"))
        }
        BackendError::NoRuntime(m) => failed(
            errors::NO_RUNTIME,
            format!("no llama.cpp build found ({m})"),
        ),
        BackendError::StartFailed(m) => failed(errors::START_FAILED, m),
        BackendError::Failed(m) => failed(errors::RUNTIME_FAILED, m),
        BackendError::Cancelled => CommandError::Cancelled,
    }
}

/// The prefill and what the model wrote after it, with one space where both bring one.
fn join(prefill: &str, text: &str) -> String {
    let text = if prefill.ends_with(' ') {
        text.trim_start_matches(' ')
    } else {
        text
    };
    format!("{prefill}{text}")
}

fn exe_name() -> &'static str {
    if cfg!(windows) {
        "llama-server.exe"
    } else {
        "llama-server"
    }
}

/// Whether an NVIDIA driver is installed (the CUDA build needs it).
fn has_nvidia() -> bool {
    if cfg!(windows) {
        std::env::var_os("SystemRoot")
            .map(PathBuf::from)
            .is_some_and(|r| r.join("System32").join("nvcuda.dll").is_file())
    } else {
        Path::new("/proc/driver/nvidia").exists()
    }
}

fn has_vulkan() -> bool {
    if cfg!(windows) {
        std::env::var_os("SystemRoot")
            .map(PathBuf::from)
            .is_some_and(|r| r.join("System32").join("vulkan-1.dll").is_file())
    } else {
        false
    }
}

/// Folders that may hold the llama.cpp builds: next to the program, else `.natives` above it
/// (development).
#[must_use]
pub fn runtime_dirs() -> Vec<PathBuf> {
    let Some(dir) = std::env::current_exe()
        .ok()
        .and_then(|e| e.parent().map(PathBuf::from))
    else {
        return vec![];
    };
    let natives = dir
        .ancestors()
        .map(|a| a.join(".natives"))
        .find(|d| d.is_dir());
    std::iter::once(dir.clone()).chain(natives).collect()
}

impl Inference {
    /// An engine that looks for GGUF models in `model_dirs` and llama.cpp builds
    /// (`llama-cpp-cuda`, `llama-cpp-vulkan`, `llama-cpp-cpu`) in `runtime_dirs`; the server is
    /// started by `spawner` (tied to the app's lifetime) and logs to `log_file`. The user's folders
    /// and the model of each task are kept in `choices_file`.
    #[must_use]
    pub fn new(
        model_dirs: Vec<PathBuf>,
        runtime_dirs: Vec<PathBuf>,
        spawner: Arc<dyn ProcessSpawner>,
        log_file: Option<PathBuf>,
        choices_file: Option<PathBuf>,
    ) -> Arc<Self> {
        Arc::new(Self {
            base_dirs: model_dirs,
            choices: RwLock::new(Choices::read(choices_file.as_deref())),
            choices_file,
            catalog: RwLock::new(None),
            headers: Mutex::default(),
            problems: Mutex::default(),
            turn: tokio::sync::Mutex::new(()),
            runtime_dirs,
            spawner: Some(spawner),
            log_file,
            fixed: None,
            sidecar: RwLock::new(None),
            status: Mutex::new(Status {
                state: RunState::Stopped,
                model: None,
                backend: None,
                error: None,
                ready_ms: None,
            }),
            starting: tokio::sync::Mutex::new(()),
            bus: RwLock::new(Weak::new()),
        })
    }

    /// An engine that always uses `backend` (tests: `Replay`).
    #[must_use]
    pub fn with_backend(backend: Arc<dyn InferenceBackend>) -> Arc<Self> {
        Arc::new(Self {
            base_dirs: vec![],
            choices: RwLock::default(),
            choices_file: None,
            catalog: RwLock::new(None),
            headers: Mutex::default(),
            problems: Mutex::default(),
            turn: tokio::sync::Mutex::new(()),
            runtime_dirs: vec![],
            spawner: None,
            log_file: None,
            fixed: Some(backend),
            sidecar: RwLock::new(None),
            status: Mutex::new(Status {
                state: RunState::Ready,
                model: Some("replay".into()),
                backend: None,
                error: None,
                ready_ms: None,
            }),
            starting: tokio::sync::Mutex::new(()),
            bus: RwLock::new(Weak::new()),
        })
    }

    /// Also look for models in `dir`, from now on.
    ///
    /// # Errors
    /// It is not a folder, or the choice could not be kept.
    pub fn add_folder(&self, dir: &Path) -> Result<(), CommandError> {
        if !dir.is_dir() {
            return Err(failed(
                errors::NOT_A_FOLDER,
                format!("{} is not a folder", dir.display()),
            ));
        }
        self.change(|c| {
            if !c.folders.iter().any(|d| d == dir) {
                c.folders.push(dir.to_path_buf());
            }
        })?;
        *self.catalog.write() = None;
        Ok(())
    }

    /// Change what the user chose, and keep it.
    fn change(&self, f: impl FnOnce(&mut Choices)) -> Result<(), CommandError> {
        let mut c = self.choices.write();
        let mut next = c.clone();
        f(&mut next);
        next.write(self.choices_file.as_deref())
            .map_err(|e| failed(errors::IO, e.to_string()))?;
        *c = next;
        Ok(())
    }

    fn dirs(&self) -> Vec<PathBuf> {
        let own = self.choices.read().folders.clone();
        self.base_dirs.iter().cloned().chain(own).collect()
    }

    /// The models found; the folders are searched when that was not done yet (blocking).
    fn catalog(&self) -> Arc<Catalog> {
        let have = self.catalog.read().clone();
        if let Some(c) = have {
            return c;
        }
        let c = Arc::new(catalog::scan(&self.dirs(), &mut self.headers.lock()));
        *self.catalog.write() = Some(c.clone());
        c
    }

    /// As [`Self::catalog`], without holding up other work; `again` searches anew.
    async fn searched(self: &Arc<Self>, again: bool) -> Arc<Catalog> {
        if again {
            *self.catalog.write() = None;
        }
        let me = self.clone();
        tokio::task::spawn_blocking(move || me.catalog())
            .await
            .unwrap_or_default()
    }

    /// The build that will run: CUDA when an NVIDIA driver is present, else Vulkan, else CPU
    /// (ADR-005; CUDA is chosen over Vulkan when both are installed, since it is only
    /// installed on NVIDIA machines).
    #[must_use]
    pub fn runtime(&self) -> (Backend, Option<PathBuf>) {
        let find = |name: &str| {
            self.runtime_dirs
                .iter()
                .map(|d| d.join(name).join(exe_name()))
                .find(|p| p.is_file())
        };
        if has_nvidia()
            && let Some(p) = find("llama-cpp-cuda")
        {
            return (Backend::Cuda, Some(p));
        }
        if has_vulkan()
            && let Some(p) = find("llama-cpp-vulkan")
        {
            return (Backend::Vulkan, Some(p));
        }
        // The Vulkan package holds the CPU code too (no layers on the GPU then).
        match find("llama-cpp-cpu").or_else(|| find("llama-cpp-vulkan")) {
            Some(p) => (Backend::Cpu, Some(p)),
            None => (Backend::None, None),
        }
    }

    /// The model for a request's task, and why (contract: tasks).
    fn chosen(&self, c: &Catalog, backend: Backend, task: &str) -> Option<(Found, &'static str)> {
        let choices = self.choices.read();
        let kind = task_of(task);
        let find = |id: &String| {
            c.models
                .iter()
                .find(|m| m.id == *id && m.kind == ModelKind::Chat)
                .cloned()
        };
        let wanted = choices
            .tasks
            .get(kind)
            .map(|id| (id, "chosen for this task"));
        let default = choices
            .tasks
            .get(DEFAULT)
            .map(|id| (id, "chosen by the user"));
        for (id, why) in [wanted, default].into_iter().flatten() {
            if let Some(m) = find(id) {
                return Some((m, why));
            }
        }
        let gone = wanted.is_some() || default.is_some();
        let m = catalog::suggested(&c.models, backend)?.clone();
        Some((
            m,
            if gone {
                "suggested; the chosen model is not there"
            } else {
                "suggested"
            },
        ))
    }

    /// Models, folders, tasks and the runtime.
    fn overview(&self, c: &Catalog) -> Models {
        let (backend, build) = self.runtime();
        let suggested = catalog::suggested(&c.models, backend).map(|m| m.id.clone());
        // Cloned first: a second lock in the same statement would wait for itself.
        let status = self.status.lock().clone();
        let loaded = status.model.filter(|_| status.state == RunState::Ready);
        let problems = self.problems.lock().clone();
        let choices = self.choices.read().clone();
        let count = |n: usize| u32::try_from(n).unwrap_or(u32::MAX);
        Models {
            models: c
                .models
                .iter()
                .map(|m| ModelInfo {
                    id: m.id.clone(),
                    name: m.name.clone(),
                    file: m
                        .path
                        .file_name()
                        .map(|f| f.to_string_lossy().into_owned())
                        .unwrap_or_default(),
                    path: m.path.display().to_string(),
                    size: m.size,
                    licence: m.licence.clone(),
                    kind: m.kind,
                    architecture: m.architecture.clone(),
                    quantisation: m.quantisation.clone(),
                    state: if loaded.as_ref() == Some(&m.id) {
                        ModelState::Loaded
                    } else {
                        ModelState::Ready
                    },
                    recommended: suggested.as_ref() == Some(&m.id),
                    context: m.context,
                    problem: problems.get(&m.id).cloned(),
                })
                .collect(),
            folders: self
                .dirs()
                .into_iter()
                .filter_map(|d| {
                    let own = choices.folders.contains(&d);
                    let f = c.folders.get(&d).cloned().unwrap_or_default();
                    // The app's own folder is only worth a line when it holds something.
                    (own || f.models > 0).then(|| Folder {
                        path: d.display().to_string(),
                        models: count(f.models),
                        other_formats: count(f.other_formats),
                        missing: f.missing,
                        partial: f.partial,
                        own,
                    })
                })
                .collect(),
            tasks: TASKS
                .iter()
                .map(|t| Task {
                    id: (*t).to_string(),
                    model: choices.tasks.get(*t).cloned(),
                })
                .collect(),
            runtime: Runtime {
                backend,
                build: build.and_then(|b| b.parent().map(|p| p.display().to_string())),
            },
        }
    }

    /// Choose the model of a task (`None`: the default again).
    async fn assign(self: &Arc<Self>, i: Assign) -> Result<Models, CommandError> {
        if !TASKS.contains(&i.task.as_str()) {
            return Err(CommandError::InvalidInput {
                message: format!("unknown task {:?}", i.task),
            });
        }
        let c = self.searched(false).await;
        if let Some(id) = &i.model
            && !c
                .models
                .iter()
                .any(|m| m.id == *id && m.kind == ModelKind::Chat)
        {
            return Err(failed(
                errors::NO_MODEL,
                format!("{id} is not a model that writes answers"),
            ));
        }
        self.change(|choices| {
            match i.model {
                Some(id) => choices.tasks.insert(i.task, id),
                None => choices.tasks.remove(&i.task),
            };
        })?;
        Ok(self.overview(&c))
    }

    fn publish(&self, kind: &str, data: serde_json::Value, cx: &Cx) {
        if let Some(bus) = self.bus.read().upgrade() {
            bus.events().publish(kind, data, cx.clock.now_ms());
        }
    }

    /// Start the runtime with `model` (or the recommended one).
    ///
    /// # Errors
    /// No build, no model, or it did not start.
    #[allow(clippy::too_many_lines)] // one sequence: choose, stop the old, start, warm up
    pub async fn load(
        self: &Arc<Self>,
        cx: &Cx,
        model: Option<&str>,
    ) -> Result<Status, CommandError> {
        if self.fixed.is_some() {
            return Ok(self.status.lock().clone());
        }
        let _one = self.starting.lock().await;
        let (backend, exe) = self.runtime();
        let exe = exe.ok_or_else(|| failed(errors::NO_RUNTIME, "no llama.cpp build found"))?;
        let c = self.searched(false).await;
        let found = match model {
            Some(id) => c
                .models
                .iter()
                .find(|m| m.id == id && m.kind == ModelKind::Chat)
                .cloned()
                .ok_or_else(|| CommandError::InvalidInput {
                    message: format!("unknown model {id:?}"),
                })?,
            None => self
                .chosen(&c, backend, DEFAULT)
                .map(|(m, _)| m)
                .ok_or_else(|| {
                    failed(
                        errors::NO_MODEL,
                        "no language model found; choose the folder that has one in Settings",
                    )
                })?,
        };
        let (id, file) = (found.id.clone(), found.path.clone());
        let current = self.sidecar.read().clone();
        if let Some(s) = current
            && s.config().model == file
            && s.is_running().await
        {
            return Ok(self.status.lock().clone());
        }
        let old = self.sidecar.write().take();
        if let Some(old) = old {
            old.stop().await;
            self.publish(
                events::MODEL_UNLOADED,
                json!({ "model": old.config().model_id }),
                cx,
            );
        }
        *self.status.lock() = Status {
            state: RunState::Starting,
            model: Some(id.clone()),
            backend: Some(backend),
            error: None,
            ready_ms: None,
        };
        let spawner = self
            .spawner
            .clone()
            .ok_or_else(|| failed(errors::NO_RUNTIME, "no process starter"))?;
        let sidecar = Arc::new(LlamaCppSidecar::new(
            Config {
                exe,
                model: file,
                model_id: id.clone(),
                context: found.context,
                // Small models run whole on the GPU; for large ones llama.cpp decides what fits.
                gpu_layers: match backend {
                    Backend::Cpu => Some(0),
                    _ if found.size <= catalog::FITS_GPU => Some(99),
                    _ => None,
                },
                backend: format!("{backend:?}").to_lowercase(),
                log_file: self.log_file.clone(),
            },
            spawner,
        ));
        match sidecar.start().await {
            Ok(took) => {
                let ms = u64::try_from(took.as_millis()).unwrap_or(u64::MAX);
                *self.sidecar.write() = Some(sidecar.clone());
                self.problems.lock().remove(&id);
                let st = Status {
                    state: RunState::Ready,
                    model: Some(id.clone()),
                    backend: Some(backend),
                    error: None,
                    ready_ms: Some(ms),
                };
                *self.status.lock() = st.clone();
                self.publish(
                    events::MODEL_LOADED,
                    json!({ "model": id, "backend": backend, "ms": ms }),
                    cx,
                );
                // Warm-up in the background (Vulkan's one-time pause, ADR-005 decision 6).
                tokio::spawn(async move {
                    let p = prepare(&LlmRequest::new(
                        "warmup",
                        vec![ep_inference_contract::Message::user("Say OK.")],
                    ));
                    let mut p = p;
                    p.max_tokens = 4;
                    let _ = sidecar.complete(&p, &|_| {}, &|| false).await;
                });
                Ok(st)
            }
            Err(e) => {
                let err = from_backend(e);
                self.problems.lock().insert(id.clone(), err.to_string());
                *self.status.lock() = Status {
                    state: RunState::Failed,
                    model: Some(id.clone()),
                    backend: Some(backend),
                    error: Some(err.to_string()),
                    ready_ms: None,
                };
                Err(err)
            }
        }
    }

    /// Stop the runtime.
    pub async fn unload(&self, cx: &Cx) -> Status {
        let old = self.sidecar.write().take();
        if let Some(old) = old {
            old.stop().await;
            self.publish(
                events::MODEL_UNLOADED,
                json!({ "model": old.config().model_id }),
                cx,
            );
        }
        if self.fixed.is_none() {
            *self.status.lock() = Status {
                state: RunState::Stopped,
                model: None,
                backend: None,
                error: None,
                ready_ms: None,
            };
        }
        self.status.lock().clone()
    }

    /// The backend with the model for `task`, started when needed, and why that model.
    async fn backend(
        self: &Arc<Self>,
        cx: &Cx,
        task: &str,
    ) -> Result<(Arc<dyn InferenceBackend>, &'static str), CommandError> {
        if let Some(b) = &self.fixed {
            return Ok((b.clone(), "fixed"));
        }
        let c = self.searched(false).await;
        let (found, why) = self.chosen(&c, self.runtime().0, task).ok_or_else(|| {
            failed(
                errors::NO_MODEL,
                "no language model found; choose the folder that has one in Settings",
            )
        })?;
        self.load(cx, Some(&found.id)).await?;
        let running = self.sidecar.read().clone();
        running
            .map(|s| (s as Arc<dyn InferenceBackend>, why))
            .ok_or_else(|| failed(errors::START_FAILED, "the model runtime did not start"))
    }

    /// Generate an answer.
    ///
    /// # Errors
    /// See the contract's error kinds.
    pub async fn complete(
        self: &Arc<Self>,
        cx: &Cx,
        req: LlmRequest,
    ) -> Result<LlmResponse, CommandError> {
        if req.messages.is_empty() {
            return Err(CommandError::InvalidInput {
                message: "no messages".into(),
            });
        }
        let _turn = tokio::select! {
            t = self.turn.lock() => t,
            () = cx.cancel.cancelled() => return Err(CommandError::Cancelled),
        };
        let (backend, why) = self.backend(cx, &req.task).await?;
        // Guarantee 4: local-only data never leaves the machine.
        if req.sensitivity == Sensitivity::LocalOnly && !backend.is_local() {
            return Err(failed(
                errors::REMOTE_NOT_ALLOWED,
                "this text may only go to a model on this computer",
            ));
        }
        let model = backend.model().unwrap_or_default();
        let data = if req.sensitivity == Sensitivity::LocalOnly {
            "local-only data"
        } else {
            "local model available"
        };
        let reason = format!("{}: {} ({why}; {data})", req.task, backend.name());
        self.publish(events::ROUTE_DECIDED, json!({ "task": req.task, "backend": backend.name(), "model": model, "reason": reason }), cx);

        let prepared = prepare(&req);
        let prefill = req.prefill.clone().unwrap_or_default();
        let started = Instant::now();
        let first: Mutex<Option<u64>> = Mutex::new(None);
        let stream_id = req.stream.as_ref().map(|s| s.id.clone());
        let bus = self.bus.read().clone();
        let clock = cx.clock.clone();
        let on_token = |t: &str| {
            let mut f = first.lock();
            if f.is_none() {
                *f = Some(u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX));
            }
            drop(f);
            if let (Some(id), Some(bus)) = (&stream_id, bus.upgrade()) {
                bus.events().publish(
                    events::TOKEN,
                    json!({ "id": id, "text": t }),
                    clock.now_ms(),
                );
            }
        };
        let cancel = cx.cancel.clone();
        let cancelled = move || cancel.is_cancelled();
        let attempts = if req.schema.is_some() { 2 } else { 1 };
        let mut last_err = String::new();
        for _ in 0..attempts {
            let c = backend
                .complete(&prepared, &on_token, &cancelled)
                .await
                .map_err(from_backend)?;
            let content = join(&prefill, &c.text);
            if let Some(s) = &req.schema
                && let Err(e) = schema::parse_checked(&content, s)
            {
                tracing::warn!(task = %req.task, %e, "answer does not meet its schema; retrying once");
                last_err = e;
                continue;
            }
            return Ok(LlmResponse {
                content,
                model,
                route_reason: reason,
                usage: Usage {
                    prompt_tokens: c.prompt_tokens,
                    completion_tokens: c.completion_tokens,
                    first_token_ms: first.lock().unwrap_or_default(),
                    total_ms: u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX),
                },
            });
        }
        Err(failed(
            errors::BAD_OUTPUT,
            format!("the answer did not follow its schema: {last_err}"),
        ))
    }

    /// Register the commands on `bus`.
    ///
    /// # Errors
    /// Duplicate names.
    #[allow(clippy::too_many_lines)] // one entry per command
    pub fn register(self: &Arc<Self>, bus: &Arc<Bus>) -> Result<(), RegisterError> {
        *self.bus.write() = Arc::downgrade(bus);
        let make = |name, summary| Spec {
            name,
            summary,
            stability: Stability::Experimental,
            permission: Permission::create(),
        };
        let spec = |name, summary| Spec {
            name,
            summary,
            stability: Stability::Experimental,
            permission: Permission::read(),
        };
        let me = self.clone();
        bus.register(
            spec(
                cmd::COMPLETE,
                "Generate an answer with a language model (streamed as inference.token)",
            ),
            move |cx, i: LlmRequest| {
                let me = me.clone();
                async move { me.complete(&cx, i).await }
            },
        )?;
        let me = self.clone();
        bus.register(
            spec(
                cmd::MODELS,
                "Language models and the runtime that runs them",
            ),
            move |_cx, (): ()| {
                let me = me.clone();
                async move {
                    let c = me.searched(false).await;
                    Ok(me.overview(&c))
                }
            },
        )?;
        let me = self.clone();
        bus.register(
            spec(cmd::SEARCH_MODELS, "Search the model folders again"),
            move |_cx, (): ()| {
                let me = me.clone();
                async move {
                    let c = me.searched(true).await;
                    Ok(me.overview(&c))
                }
            },
        )?;
        let me = self.clone();
        bus.register(
            make(
                cmd::ADD_FOLDER,
                "Search a folder for language models, from now on",
            ),
            move |cx, i: FolderPath| {
                let me = me.clone();
                async move {
                    me.add_folder(Path::new(&i.path))?;
                    let c = me.searched(true).await;
                    me.publish(events::MODELS_CHANGED, json!({}), &cx);
                    Ok(me.overview(&c))
                }
            },
        )?;
        let me = self.clone();
        bus.register(
            make(
                cmd::REMOVE_FOLDER,
                "Stop searching a folder for language models",
            ),
            move |cx, i: FolderPath| {
                let me = me.clone();
                async move {
                    me.change(|c| c.folders.retain(|d| d.as_path() != Path::new(&i.path)))?;
                    let c = me.searched(true).await;
                    me.publish(events::MODELS_CHANGED, json!({}), &cx);
                    Ok(me.overview(&c))
                }
            },
        )?;
        let me = self.clone();
        bus.register(
            make(cmd::ASSIGN, "Choose the language model of a task"),
            move |cx, i: Assign| {
                let me = me.clone();
                async move {
                    let m = me.assign(i).await?;
                    me.publish(events::MODELS_CHANGED, json!({}), &cx);
                    Ok(m)
                }
            },
        )?;
        let me = self.clone();
        bus.register(
            spec(cmd::STATUS, "State of the language model runtime"),
            move |_cx, (): ()| {
                let me = me.clone();
                async move { Ok(me.status.lock().clone()) }
            },
        )?;
        let me = self.clone();
        bus.register(
            spec(cmd::LOAD, "Start the language model runtime with a model"),
            move |cx, i: Option<LoadRequest>| {
                let me = me.clone();
                async move { me.load(&cx, i.and_then(|i| i.model).as_deref()).await }
            },
        )?;
        let me = self.clone();
        bus.register(
            spec(cmd::UNLOAD, "Stop the language model runtime"),
            move |cx, (): ()| {
                let me = me.clone();
                async move { Ok(me.unload(&cx).await) }
            },
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    #[test]
    fn a_chosen_model_that_is_gone_falls_back_and_says_so() {
        let e = Inference::new(
            vec![],
            vec![],
            Arc::new(ep_os::fake::FakeSpawner::default()),
            None,
            None,
        );
        let found = |id: &str| Found {
            id: id.into(),
            name: id.into(),
            path: PathBuf::from(id),
            size: 1,
            licence: String::new(),
            kind: ModelKind::Chat,
            architecture: "llama".into(),
            quantisation: None,
            context: 4096,
        };
        let c = Catalog {
            models: vec![found("a"), found("b")],
            ..Catalog::default()
        };
        let pick = |task: &str| {
            e.chosen(&c, Backend::Cpu, task)
                .map(|(m, why)| (m.id, why))
                .unwrap()
        };
        assert_eq!(pick("explain.draft").1, "suggested");
        e.change(|x| {
            x.tasks.insert("default".into(), "b".into());
            x.tasks.insert("check".into(), "a".into());
        })
        .unwrap();
        assert_eq!(pick("explain.draft"), ("b".into(), "chosen by the user"));
        assert_eq!(pick("explain.check"), ("a".into(), "chosen for this task"));
        e.change(|x| {
            x.tasks.insert("default".into(), "gone".into());
            x.tasks.remove("check");
        })
        .unwrap();
        assert_eq!(
            pick("explain.check").1,
            "suggested; the chosen model is not there"
        );
    }
}
