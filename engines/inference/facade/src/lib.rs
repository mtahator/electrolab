//! Inference engine facade: the commands of `engines/inference/CONTRACT.md` on the bus.
//!
//! It finds the local models and the llama.cpp builds, chooses one (ADR-005), starts the
//! sidecar on first use, streams answers as `inference.token` events, and checks answers
//! against their schema with one retry. Tests replace the backend with `Replay`.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Weak};
use std::time::Instant;

use ep_bus::{Bus, CommandError, Cx, Permission, RegisterError, Spec, Stability};
use ep_inference_adapter_llamacpp::{Config, LlamaCppSidecar};
pub use ep_inference_contract as contract;
use ep_inference_contract::{
    Backend, LlmRequest, LlmResponse, LoadRequest, ModelInfo, ModelState, Models, RunState,
    Runtime, Sensitivity, Status, Usage, commands as cmd, errors, events,
};
pub use ep_inference_core::Replay;
pub use ep_inference_core::{
    BackendError, Completion, InferenceBackend, IsCancelled, OnToken, Prepared,
};
use ep_inference_core::{prepare, schema};
use ep_os::ProcessSpawner;
use parking_lot::{Mutex, RwLock};
use serde_json::json;

/// A model the engine knows (ADR-005).
struct Known {
    id: &'static str,
    name: &'static str,
    file: &'static str,
    licence: &'static str,
    context: u32,
    /// Fits a 4 GB GPU; the default there.
    gpu: bool,
}

const KNOWN: &[Known] = &[
    Known {
        id: "qwen3-4b-instruct-2507",
        name: "Qwen3 4B Instruct (2507), Q4_K_M",
        file: "Qwen3-4B-Instruct-2507-Q4_K_M.gguf",
        licence: "Apache-2.0",
        context: 8192,
        gpu: true,
    },
    Known {
        id: "lfm2.5-1.2b-instruct",
        name: "LFM2.5 1.2B Instruct, Q8_0",
        file: "LFM2.5-1.2B-Instruct-Q8_0.gguf",
        licence: "LFM Open License 1.0",
        context: 8192,
        gpu: false,
    },
];

/// The engine.
pub struct Inference {
    model_dirs: RwLock<Vec<PathBuf>>,
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
    /// started by `spawner` (tied to the app's lifetime) and logs to `log_file`.
    #[must_use]
    pub fn new(
        model_dirs: Vec<PathBuf>,
        runtime_dirs: Vec<PathBuf>,
        spawner: Arc<dyn ProcessSpawner>,
        log_file: Option<PathBuf>,
    ) -> Arc<Self> {
        Arc::new(Self {
            model_dirs: RwLock::new(model_dirs),
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
            model_dirs: RwLock::new(vec![]),
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

    /// Also look for models in `dir`.
    pub fn add_model_dir(&self, dir: PathBuf) {
        let mut dirs = self.model_dirs.write();
        if !dirs.contains(&dir) {
            dirs.push(dir);
        }
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

    fn file_of(&self, k: &Known) -> Option<PathBuf> {
        self.model_dirs
            .read()
            .iter()
            .map(|d| d.join(k.file))
            .find(|p| p.is_file())
    }

    fn recommended(&self, backend: Backend) -> Option<&'static Known> {
        let gpu = matches!(backend, Backend::Cuda | Backend::Vulkan);
        KNOWN
            .iter()
            .filter(|k| self.file_of(k).is_some())
            .find(|k| k.gpu == gpu)
            .or_else(|| KNOWN.iter().find(|k| self.file_of(k).is_some()))
    }

    /// Models and runtime.
    #[must_use]
    pub fn models(&self) -> Models {
        let (backend, build) = self.runtime();
        let rec = self.recommended(backend).map(|k| k.id);
        // Cloned first: a second lock in the same statement would wait for itself.
        let status = self.status.lock().clone();
        let loaded = status.model.filter(|_| status.state == RunState::Ready);
        let models = KNOWN
            .iter()
            .map(|k| {
                let file = self.file_of(k);
                ModelInfo {
                    id: k.id.into(),
                    name: k.name.into(),
                    file: k.file.into(),
                    size: file
                        .as_ref()
                        .and_then(|f| f.metadata().ok())
                        .map_or(0, |m| m.len()),
                    licence: k.licence.into(),
                    state: match (&file, loaded.as_deref() == Some(k.id)) {
                        (_, true) => ModelState::Loaded,
                        (Some(_), false) => ModelState::Ready,
                        (None, false) => ModelState::Missing,
                    },
                    recommended: rec == Some(k.id),
                    context: k.context,
                }
            })
            .collect();
        Models {
            models,
            runtime: Runtime {
                backend,
                build: build.and_then(|b| b.parent().map(|p| p.display().to_string())),
            },
        }
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
    pub async fn load(&self, cx: &Cx, model: Option<&str>) -> Result<Status, CommandError> {
        if self.fixed.is_some() {
            return Ok(self.status.lock().clone());
        }
        let _one = self.starting.lock().await;
        let (backend, exe) = self.runtime();
        let exe = exe.ok_or_else(|| failed(errors::NO_RUNTIME, "no llama.cpp build found"))?;
        let k = match model {
            Some(id) => {
                KNOWN
                    .iter()
                    .find(|k| k.id == id)
                    .ok_or_else(|| CommandError::InvalidInput {
                        message: format!("unknown model {id:?}"),
                    })?
            }
            None => self.recommended(backend).ok_or_else(|| {
                failed(
                    errors::NO_MODEL,
                    "no language model found; download one in Settings",
                )
            })?,
        };
        let file = self
            .file_of(k)
            .ok_or_else(|| failed(errors::NO_MODEL, format!("{} is not downloaded", k.name)))?;
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
            model: Some(k.id.into()),
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
                model_id: k.id.into(),
                context: k.context,
                gpu_layers: if backend == Backend::Cpu { 0 } else { 99 },
                backend: format!("{backend:?}").to_lowercase(),
                log_file: self.log_file.clone(),
            },
            spawner,
        ));
        match sidecar.start().await {
            Ok(took) => {
                let ms = u64::try_from(took.as_millis()).unwrap_or(u64::MAX);
                *self.sidecar.write() = Some(sidecar.clone());
                let st = Status {
                    state: RunState::Ready,
                    model: Some(k.id.into()),
                    backend: Some(backend),
                    error: None,
                    ready_ms: Some(ms),
                };
                *self.status.lock() = st.clone();
                self.publish(
                    events::MODEL_LOADED,
                    json!({ "model": k.id, "backend": backend, "ms": ms }),
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
                *self.status.lock() = Status {
                    state: RunState::Failed,
                    model: Some(k.id.into()),
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

    async fn backend(&self, cx: &Cx) -> Result<Arc<dyn InferenceBackend>, CommandError> {
        if let Some(b) = &self.fixed {
            return Ok(b.clone());
        }
        let running = self.sidecar.read().clone();
        if let Some(s) = running {
            return Ok(s);
        }
        self.load(cx, None).await?;
        self.sidecar
            .read()
            .clone()
            .map(|s| s as Arc<dyn InferenceBackend>)
            .ok_or_else(|| failed(errors::START_FAILED, "the model runtime did not start"))
    }

    /// Generate an answer.
    ///
    /// # Errors
    /// See the contract's error kinds.
    pub async fn complete(&self, cx: &Cx, req: LlmRequest) -> Result<LlmResponse, CommandError> {
        if req.messages.is_empty() {
            return Err(CommandError::InvalidInput {
                message: "no messages".into(),
            });
        }
        let backend = self.backend(cx).await?;
        // Guarantee 4: local-only data never leaves the machine.
        if req.sensitivity == Sensitivity::LocalOnly && !backend.is_local() {
            return Err(failed(
                errors::REMOTE_NOT_ALLOWED,
                "this text may only go to a model on this computer",
            ));
        }
        let model = backend.model().unwrap_or_default();
        let reason = format!(
            "{}: {} ({})",
            req.task,
            backend.name(),
            if req.sensitivity == Sensitivity::LocalOnly {
                "local-only data"
            } else {
                "local model available"
            }
        );
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
            let content = format!("{prefill}{}", c.text);
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
    pub fn register(self: &Arc<Self>, bus: &Arc<Bus>) -> Result<(), RegisterError> {
        *self.bus.write() = Arc::downgrade(bus);
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
                async move { Ok(me.models()) }
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
