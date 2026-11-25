//! Inference engine facade: the commands of `engines/inference/CONTRACT.md` on the bus.
//!
//! It finds the local models and the llama.cpp builds, chooses one that fits the free memory in the
//! user's order (`select`), starts the sidecar and keeps it running, streams answers as `inference.token` events, and checks answers
//! against their schema with one retry. Tests replace the backend with `Replay`.

mod catalog;
mod select;
mod vision;

pub use vision::Pictures;

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Weak};
use std::time::{Duration, Instant};

use catalog::{Catalog, Choices, Found};
use ep_bus::{Bus, CommandError, Cx, Permission, RegisterError, Spec, Stability};
use ep_inference_adapter_llamacpp::{Config, LlamaCppSidecar, devices};
pub use ep_inference_contract as contract;
use ep_inference_contract::{
    Backend, Choose, Fallback, Folder, FolderPath, LlmRequest, LlmResponse, LoadRequest, Memory,
    ModelInfo, ModelKind, ModelState, Models, RunState, Runtime, Sensitivity, Status, Usage,
    VisionRead, commands as cmd, errors, events,
};
pub use ep_inference_core::Replay;
pub use ep_inference_core::{
    BackendError, Completion, InferenceBackend, IsCancelled, OnToken, Prepared,
};
use ep_inference_core::{prepare, schema};
use ep_os::{Machine, ProcessSpawner};
use parking_lot::{Mutex, RwLock};
use serde_json::json;

/// After this long another look is taken at a first choice that could not be used.
const LOOK_AGAIN: Duration = Duration::from_secs(600);
/// Fewer tokens a second than this are too slow (twice in a row, in answers of some length).
const SLOW: f64 = 3.0;
const SLOW_LENGTH: u32 = 40;

/// The engine.
pub struct Inference {
    /// Folders that are always searched; the user's own ones are in `choices`.
    base_dirs: Vec<PathBuf>,
    choices: RwLock<Choices>,
    choices_file: Option<PathBuf>,
    catalog: RwLock<Option<Arc<Catalog>>>,
    headers: Mutex<catalog::Cache>,
    problems: Mutex<HashMap<String, String>>,
    /// Models left out until the app starts again or the order changes, and why.
    out: Mutex<HashMap<String, Fallback>>,
    /// Answers in a row that were too slow.
    slow: std::sync::atomic::AtomicU32,
    /// When the order was last gone through.
    looked: Mutex<Option<Instant>>,
    memory: Mutex<Option<Memory>>,
    /// One answer at a time: another model may have to be loaded for the next one.
    turn: tokio::sync::Mutex<()>,
    /// People waiting for an answer; background work lets them go first.
    people: std::sync::atomic::AtomicUsize,
    /// Batches of pictures under way; other background work waits for them.
    looking: std::sync::atomic::AtomicUsize,
    pictures: RwLock<Option<Pictures>>,
    runtime_dirs: Vec<PathBuf>,
    spawner: Option<Arc<dyn ProcessSpawner>>,
    log_file: Option<PathBuf>,
    /// What other programs use.
    machine: RwLock<Option<Arc<dyn Machine>>>,
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
    /// and the order of the models are kept in `choices_file`.
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
            out: Mutex::default(),
            slow: std::sync::atomic::AtomicU32::new(0),
            looked: Mutex::new(None),
            memory: Mutex::new(None),
            turn: tokio::sync::Mutex::new(()),
            people: std::sync::atomic::AtomicUsize::new(0),
            looking: std::sync::atomic::AtomicUsize::new(0),
            pictures: RwLock::new(None),
            runtime_dirs,
            spawner: Some(spawner),
            log_file,
            machine: RwLock::new(None),
            fixed: None,
            sidecar: RwLock::new(None),
            status: Mutex::new(Status {
                state: RunState::Stopped,
                model: None,
                backend: None,
                error: None,
                wanted: None,
                fallback: None,
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
            out: Mutex::default(),
            slow: std::sync::atomic::AtomicU32::new(0),
            looked: Mutex::new(None),
            memory: Mutex::new(None),
            turn: tokio::sync::Mutex::new(()),
            people: std::sync::atomic::AtomicUsize::new(0),
            looking: std::sync::atomic::AtomicUsize::new(0),
            pictures: RwLock::new(None),
            runtime_dirs: vec![],
            spawner: None,
            log_file: None,
            machine: RwLock::new(None),
            fixed: Some(backend),
            sidecar: RwLock::new(None),
            status: Mutex::new(Status {
                state: RunState::Ready,
                model: Some("replay".into()),
                backend: None,
                error: None,
                wanted: None,
                fallback: None,
                ready_ms: None,
            }),
            starting: tokio::sync::Mutex::new(()),
            bus: RwLock::new(Weak::new()),
        })
    }

    /// Ask `machine` how much of the graphics memory other programs use.
    pub fn use_machine(&self, machine: Arc<dyn Machine>) {
        *self.machine.write() = Some(machine);
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
        // Kept as a whole path: the app may be started from anywhere the next time.
        let dir = std::path::absolute(dir).unwrap_or_else(|_| dir.to_path_buf());
        self.change(|c| {
            if !c.folders.contains(&dir) {
                c.folders.push(dir);
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

    /// The models in the order they are tried.
    fn order(&self, c: &Catalog, backend: Backend) -> Vec<Found> {
        select::order(&self.choices.read().order, &c.models, backend)
    }

    /// Models, folders, their order and the runtime.
    fn overview(&self, c: &Catalog) -> Models {
        let (backend, build) = self.runtime();
        let suggested = catalog::suggested(&c.models, backend).map(|m| m.id.clone());
        // Cloned first: a second lock in the same statement would wait for itself.
        let status = self.status.lock().clone();
        let loaded = status.model.filter(|_| status.state == RunState::Ready);
        let problems = self.problems.lock().clone();
        let choices = self.choices.read().clone();
        let order = self.order(c, backend).into_iter().map(|m| m.id).collect();
        let memory = self.memory.lock().clone();
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
                    needs: select::need(m, m.context),
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
            order,
            own_order: !choices.order.is_empty(),
            keep_ready: !choices.on_demand,
            memory,
            runtime: Runtime {
                backend,
                build: build.and_then(|b| b.parent().map(|p| p.display().to_string())),
            },
            vision: self.vision_model(c),
        }
    }

    /// Choose the order of the models, and whether one is kept ready.
    async fn choose(self: &Arc<Self>, cx: &Cx, i: Choose) -> Result<Models, CommandError> {
        let c = self.searched(false).await;
        if let Some(order) = &i.order {
            let chat = |id: &String| {
                c.models
                    .iter()
                    .any(|m| m.id == *id && m.kind == ModelKind::Chat)
            };
            if let Some(id) = order.iter().find(|id| !chat(id)) {
                return Err(failed(
                    errors::NO_MODEL,
                    format!("{id} is not a model that writes answers"),
                ));
            }
        }
        let reorder = i.order.is_some();
        self.change(|choices| {
            if let Some(order) = i.order {
                let mut seen = HashSet::new();
                choices.order = order
                    .into_iter()
                    .filter(|x| seen.insert(x.clone()))
                    .collect();
            }
            if let Some(k) = i.keep_ready {
                choices.on_demand = !k;
            }
        })?;
        if reorder {
            self.out.lock().clear();
            *self.looked.lock() = None;
        }
        // The new first choice is made ready when nobody is waiting for an answer.
        let running = self.sidecar.read().is_some();
        if self.fixed.is_none() && (running || !self.choices.read().on_demand) {
            let (me, cx) = (self.clone(), cx.clone());
            tokio::spawn(async move {
                if let Ok(_turn) = me.take_turn(&cx, true).await {
                    let _ = me.load(&cx, None).await;
                }
            });
        }
        Ok(self.overview(&c))
    }

    fn publish(&self, kind: &str, data: serde_json::Value, cx: &Cx) {
        if let Some(bus) = self.bus.read().upgrade() {
            bus.events().publish(kind, data, cx.clock.now_ms());
        }
    }

    /// Free memory of the graphics card, measured now (it takes about a second, and may not answer).
    async fn measure(&self, exe: &Path) -> Option<(Memory, String)> {
        let spawner = self.spawner.clone()?;
        let file = self.log_file.as_ref()?.with_file_name("llama-devices.log");
        let all = devices::ask(exe, &spawner, &file).await;
        let d = devices::best(&all)?;
        // Windows promises every program nearly the whole card; what the others use counts.
        let machine = self.machine.read().clone();
        let used = machine.and_then(|m| m.graphics_used()).unwrap_or(0);
        let m = Memory {
            device: d.name.clone(),
            total: d.total,
            free: d.free.min(d.total.saturating_sub(used)),
        };
        *self.memory.lock() = Some(m.clone());
        Some((m, d.id.clone()))
    }

    /// Start the runtime with `model`, or with the first model of the order that can be used now.
    ///
    /// # Errors
    /// No build, no model, or none could be started.
    #[allow(clippy::too_many_lines)] // one sequence: choose, stop the old, start, next when it fails
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
        let order = match model {
            Some(id) => vec![
                c.models
                    .iter()
                    .find(|m| m.id == id && m.kind == ModelKind::Chat)
                    .cloned()
                    .ok_or_else(|| CommandError::InvalidInput {
                        message: format!("unknown model {id:?}"),
                    })?,
            ],
            None => self.order(&c, backend),
        };
        let Some(first) = order.first().cloned() else {
            return Err(failed(
                errors::NO_MODEL,
                "no language model found; choose the folder that has one in Settings",
            ));
        };
        *self.looked.lock() = Some(Instant::now());
        let current = self.sidecar.read().clone();
        let current = match current {
            Some(s) if s.is_running().await => Some(s),
            _ => None,
        };
        if let Some(s) = &current
            && s.config().model == first.path
            && s.config().mmproj.is_none()
        {
            return Ok(self.status.lock().clone());
        }
        let mut skip: HashSet<String> = if model.is_some() {
            HashSet::new()
        } else {
            self.out.lock().keys().cloned().collect()
        };
        // A second choice that runs is only given up when the first one fits in its place.
        if let Some(s) = &current
            && model.is_none()
            && s.config().mmproj.is_none()
            && let Some(at) = order.iter().position(|m| m.path == s.config().model)
        {
            let free = self.measure(&exe).await.map(|(m, _)| m.free);
            let room = free.map(|f| f + select::need(&order[at], s.config().context));
            let better = room.is_some_and(|room| {
                order[..at].iter().any(|m| {
                    !skip.contains(&m.id) && select::need(m, select::SHORT_CONTEXT) <= room
                })
            });
            if !better {
                return Ok(self.status.lock().clone());
            }
        }
        self.stop_current(cx).await;
        let mut last = None;
        let mut fallback = self.out.lock().get(&first.id).copied();
        loop {
            // The memory of the model that was just ended comes free a moment later.
            let late = current.is_some() && !skip.contains(&first.id);
            let measured = if backend == Backend::Cpu {
                None
            } else {
                select::settled(
                    || self.measure(&exe),
                    |(m, _)| {
                        !late
                            || select::need(&first, select::SHORT_CONTEXT.min(first.context))
                                <= m.free
                    },
                )
                .await
            };
            let free = measured.as_ref().map(|(m, _)| m.free);
            let Some(pick) = select::pick(&order, free, &skip) else {
                break;
            };
            if pick.model.id != first.id {
                let need = select::need(&first, select::SHORT_CONTEXT.min(first.context)) >> 20;
                tracing::info!(first = %first.id, used = %pick.model.id, free = ?free.map(|f| f >> 20), need, "not the first choice");
            }
            if pick.model.id != first.id && fallback.is_none() {
                fallback = Some(Fallback::NotEnoughMemory);
            }
            match self
                .start(
                    cx,
                    &pick,
                    &first,
                    fallback,
                    (backend, exe.clone()),
                    measured.map(|m| m.1),
                )
                .await
            {
                Ok(st) => return Ok(st),
                Err(e) => {
                    if pick.model.id == first.id {
                        fallback = Some(Fallback::StartFailed);
                    }
                    self.out
                        .lock()
                        .insert(pick.model.id.clone(), Fallback::StartFailed);
                    skip.insert(pick.model.id.clone());
                    tracing::warn!(model = %pick.model.id, %e, "the model did not start; the next one is tried");
                    last = Some(e);
                }
            }
        }
        // Asked for again, all of them are tried again.
        self.out.lock().clear();
        Err(last.unwrap_or_else(|| failed(errors::START_FAILED, "no model could be started")))
    }

    async fn stop_current(&self, cx: &Cx) {
        let old = self.sidecar.write().take();
        if let Some(old) = old {
            old.stop().await;
            self.publish(
                events::MODEL_UNLOADED,
                json!({ "model": old.config().model_id }),
                cx,
            );
        }
    }

    /// Start one model.
    async fn start(
        self: &Arc<Self>,
        cx: &Cx,
        pick: &select::Pick,
        first: &Found,
        fallback: Option<Fallback>,
        (backend, exe): (Backend, PathBuf),
        device: Option<String>,
    ) -> Result<Status, CommandError> {
        let m = &pick.model;
        let second = m.id != first.id;
        let status = |state, error, ready_ms| Status {
            state,
            model: Some(m.id.clone()),
            backend: Some(backend),
            error,
            wanted: second.then(|| first.id.clone()),
            fallback: fallback.filter(|_| second),
            ready_ms,
        };
        *self.status.lock() = status(RunState::Starting, None, None);
        self.publish(events::MODEL_LOADING, json!({ "model": m.id }), cx);
        let spawner = self
            .spawner
            .clone()
            .ok_or_else(|| failed(errors::NO_RUNTIME, "no process starter"))?;
        let mut r = Err(BackendError::StartFailed("not started".into()));
        // A model that cannot do the small cache is started without it.
        for compact in [true, false] {
            let sidecar = Arc::new(LlamaCppSidecar::new(
                Config {
                    exe: exe.clone(),
                    model: m.path.clone(),
                    mmproj: None,
                    mmproj_on_cpu: false,
                    model_id: m.id.clone(),
                    context: pick.context,
                    gpu_layers: match backend {
                        Backend::Cpu => Some(0),
                        _ if pick.whole => Some(99),
                        _ => None,
                    },
                    device: device.clone(),
                    compact,
                    backend: format!("{backend:?}").to_lowercase(),
                    log_file: self.log_file.clone(),
                },
                spawner.clone(),
            ));
            let began = Instant::now();
            r = sidecar.start().await.map(|took| (sidecar, took));
            // Only an early end can come from the options.
            if r.is_ok() || began.elapsed() > Duration::from_secs(20) {
                break;
            }
        }
        match r {
            Ok((sidecar, took)) => {
                let ms = u64::try_from(took.as_millis()).unwrap_or(u64::MAX);
                *self.sidecar.write() = Some(sidecar.clone());
                self.problems.lock().remove(&m.id);
                self.slow.store(0, std::sync::atomic::Ordering::SeqCst);
                let st = status(RunState::Ready, None, Some(ms));
                *self.status.lock() = st.clone();
                self.publish(
                    events::MODEL_LOADED,
                    json!({ "model": m.id, "backend": backend, "ms": ms }),
                    cx,
                );
                if let (Some(wanted), Some(reason)) = (&st.wanted, st.fallback) {
                    self.publish(
                        events::MODEL_FALLBACK,
                        json!({ "wanted": wanted, "used": m.id, "reason": reason }),
                        cx,
                    );
                }
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
                self.problems.lock().insert(m.id.clone(), err.to_string());
                *self.status.lock() = status(RunState::Failed, Some(err.to_string()), None);
                Err(err)
            }
        }
    }

    /// Make a model ready, when the user wants one kept (called when the app starts).
    pub async fn keep_ready(self: &Arc<Self>, cx: &Cx) {
        if self.fixed.is_some() || self.choices.read().on_demand {
            return;
        }
        if let Ok(_turn) = self.take_turn(cx, true).await
            && let Err(e) = self.load(cx, None).await
        {
            tracing::info!(%e, "no model was made ready");
        }
    }

    /// Stop the runtime.
    pub async fn unload(&self, cx: &Cx) -> Status {
        self.stop_current(cx).await;
        if self.fixed.is_none() {
            *self.status.lock() = Status {
                state: RunState::Stopped,
                model: None,
                backend: None,
                error: None,
                wanted: None,
                fallback: None,
                ready_ms: None,
            };
        }
        self.status.lock().clone()
    }

    /// The backend with the model to use, started when needed, and why that model. A model that runs
    /// stays: any model of the order answers, only background work looks for a better one.
    async fn backend(
        self: &Arc<Self>,
        cx: &Cx,
        background: bool,
    ) -> Result<(Arc<dyn InferenceBackend>, String), CommandError> {
        if let Some(b) = &self.fixed {
            return Ok((b.clone(), "fixed".into()));
        }
        let c = self.searched(false).await;
        let order = self.order(&c, self.runtime().0);
        let running = self.sidecar.read().clone();
        let at = match &running {
            // The one that looks at pictures does not answer questions.
            Some(s) if s.config().mmproj.is_none() && s.is_running().await => {
                order.iter().position(|m| m.path == s.config().model)
            }
            _ => None,
        };
        let look_again = background
            && at.is_some_and(|at| at > 0)
            && self.looked.lock().is_none_or(|t| t.elapsed() > LOOK_AGAIN);
        if at.is_none() || look_again {
            self.load(cx, None).await?;
        }
        let running = self.sidecar.read().clone();
        let s = running
            .ok_or_else(|| failed(errors::START_FAILED, "the model runtime did not start"))?;
        let why = match self.status.lock().fallback {
            None => "first choice".to_string(),
            Some(Fallback::NotEnoughMemory) => "the first choice does not fit in memory".into(),
            Some(Fallback::StartFailed) => "the first choice did not start".into(),
            Some(Fallback::TooSlow) => "the first choice was too slow".into(),
        };
        Ok((s as Arc<dyn InferenceBackend>, why))
    }

    /// Watch the speed: a model that is too slow twice in a row gives way to the next one.
    async fn watch(self: &Arc<Self>, cx: &Cx, model: &str, tokens: u32, ms: u64) {
        use std::sync::atomic::Ordering;
        if self.fixed.is_some() || tokens < SLOW_LENGTH || ms == 0 {
            return;
        }
        #[allow(clippy::cast_precision_loss)] // milliseconds
        let speed = f64::from(tokens) * 1000.0 / ms as f64;
        if speed >= SLOW {
            self.slow.store(0, Ordering::SeqCst);
            return;
        }
        if self.slow.fetch_add(1, Ordering::SeqCst) < 1 {
            return;
        }
        let c = self.searched(false).await;
        let out: HashSet<String> = self.out.lock().keys().cloned().collect();
        let next = self
            .order(&c, self.runtime().0)
            .iter()
            .any(|m| m.id != model && !out.contains(&m.id));
        if next {
            tracing::warn!(model, speed, "too slow; the next model is used");
            self.out.lock().insert(model.to_string(), Fallback::TooSlow);
            self.problems.lock().insert(
                model.to_string(),
                format!("too slow here ({speed:.1} words a second)"),
            );
            self.stop_current(cx).await;
        }
    }

    /// One answer at a time; a background answer also waits until nobody is waiting.
    async fn take_turn(
        &self,
        cx: &Cx,
        background: bool,
    ) -> Result<tokio::sync::MutexGuard<'_, ()>, CommandError> {
        use std::sync::atomic::Ordering;
        struct Waiting<'a>(&'a std::sync::atomic::AtomicUsize);
        impl Drop for Waiting<'_> {
            fn drop(&mut self) {
                self.0.fetch_sub(1, Ordering::SeqCst);
            }
        }
        let _person = (!background).then(|| {
            self.people.fetch_add(1, Ordering::SeqCst);
            Waiting(&self.people)
        });
        loop {
            let turn = tokio::select! {
                t = self.turn.lock() => t,
                () = cx.cancel.cancelled() => return Err(CommandError::Cancelled),
            };
            if !background || self.people.load(Ordering::SeqCst) == 0 {
                return Ok(turn);
            }
            drop(turn);
            tokio::select! {
                () = tokio::time::sleep(std::time::Duration::from_millis(40)) => {}
                () = cx.cancel.cancelled() => return Err(CommandError::Cancelled),
            }
        }
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
        // A background answer gives way when a person asks, and is begun again afterwards.
        loop {
            while req.background && self.looking.load(std::sync::atomic::Ordering::SeqCst) > 0 {
                tokio::select! {
                    () = tokio::time::sleep(Duration::from_millis(200)) => {}
                    () = cx.cancel.cancelled() => return Err(CommandError::Cancelled),
                }
            }
            let turn = self.take_turn(cx, req.background).await?;
            let r = self.answer(cx, &req).await;
            drop(turn);
            let gave_way = req.background && matches!(r, Err(CommandError::Cancelled));
            if !gave_way || cx.is_cancelled() {
                return r;
            }
        }
    }

    #[allow(clippy::too_many_lines)] // one answer: where, how, checked
    async fn answer(
        self: &Arc<Self>,
        cx: &Cx,
        req: &LlmRequest,
    ) -> Result<LlmResponse, CommandError> {
        let (backend, why) = self.backend(cx, req.background).await?;
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

        let mut prepared = prepare(req);
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
        let background = req.background;
        let cancelled = || {
            let person = self.people.load(std::sync::atomic::Ordering::SeqCst) > 0;
            cancel.is_cancelled() || (background && person)
        };
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
                // Once more, not the same way: with more room when it was cut off, else less rigid.
                if c.cut {
                    last_err = "the answer was cut off".into();
                    prepared.max_tokens = prepared.max_tokens.saturating_mul(2);
                } else {
                    last_err = e;
                    prepared.temperature = prepared.temperature.max(0.3);
                }
                continue;
            }
            let total = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
            let begun = first.lock().unwrap_or_default();
            self.watch(cx, &model, c.completion_tokens, total.saturating_sub(begun))
                .await;
            return Ok(LlmResponse {
                content,
                model,
                route_reason: reason,
                usage: Usage {
                    prompt_tokens: c.prompt_tokens,
                    completion_tokens: c.completion_tokens,
                    first_token_ms: begun,
                    total_ms: total,
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
            make(
                cmd::CHOOSE,
                "Choose the order of the language models, and whether one is kept ready",
            ),
            move |cx, i: Choose| {
                let me = me.clone();
                async move {
                    let m = me.choose(&cx, i).await?;
                    me.publish(events::MODELS_CHANGED, json!({}), &cx);
                    Ok(m)
                }
            },
        )?;
        let me = self.clone();
        bus.register(
            make(
                cmd::VISION_READ,
                "Read pictures with a model on this computer, when the user agrees",
            ),
            move |cx, i: VisionRead| {
                let me = me.clone();
                async move { me.vision_read(&cx, i).await }
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
            spec(
                cmd::WARM,
                "Make a model ready, when the user wants one kept ready",
            ),
            move |cx, (): ()| {
                let me = me.clone();
                async move {
                    me.keep_ready(&cx).await;
                    Ok(me.status.lock().clone())
                }
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
