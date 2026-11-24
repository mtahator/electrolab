//! The in-process bus: registry + middleware chain (P1.M1.BUS.1/3/5/7).
//!
//! Middleware order is part of the contract and tested:
//! lookup → decode input → **policy** → result cache → deadline + cancellation → span → handler →
//! **provenance event** (for anything that is not a plain read) → finished event on the hub.

use std::collections::{BTreeMap, HashMap};
use std::future::Future;
use std::sync::Arc;
use std::time::{Duration, Instant};

use ep_events::{Event, EventLog};
use ep_policy::{Decision, Operation, Request, Settings};
use ep_types::Hash;
use futures::future::BoxFuture;
use parking_lot::{Mutex, RwLock};
use schemars::JsonSchema;
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use tracing::Instrument;
use uuid::Uuid;

use crate::cache::{ResultCache, cache_key};
use crate::command::{CommandError, CommandInfo, Envelope, Permission, Stability};
use crate::cx::{Clock, Cx, IdSource, RandomIds, SystemClock};
use crate::hub::EventHub;

type Handler =
    Arc<dyn Fn(Cx, Value) -> BoxFuture<'static, Result<Value, CommandError>> + Send + Sync>;

struct Entry {
    info: CommandInfo,
    handler: Handler,
    /// Implementation version, for commands whose results are cached.
    cached: Option<String>,
}

/// Registration problems (programming errors found at start-up).
#[derive(Debug, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum RegisterError {
    /// The name is not `engine.verb` in lowercase snake case.
    #[error("command name `{0}` is not `engine.verb` (lowercase, digits, `_`)")]
    BadName(String),
    /// Registered twice.
    #[error("command `{0}` is already registered")]
    Duplicate(String),
}

/// What to declare when registering a command.
#[derive(Clone, Debug)]
pub struct Spec {
    /// `engine.verb`.
    pub name: &'static str,
    /// One line for humans and agents.
    pub summary: &'static str,
    /// Contract stability.
    pub stability: Stability,
    /// Policy declaration.
    pub permission: Permission,
}

/// Largest part of an input that is kept in the log as it is.
const LOGGED: usize = 8192;

fn valid_name(n: &str) -> bool {
    let mut parts = n.split('.');
    let ok = |p: &str| {
        !p.is_empty()
            && p.starts_with(|c: char| c.is_ascii_lowercase())
            && p.chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
    };
    matches!((parts.next(), parts.next(), parts.next()), (Some(a), Some(b), None) if ok(a) && ok(b))
}

fn schema_of<T: JsonSchema>() -> Value {
    serde_json::to_value(schemars::schema_for!(T)).unwrap_or(Value::Null)
}

/// The command bus.
pub struct Bus {
    commands: RwLock<BTreeMap<String, Entry>>,
    inflight: Mutex<HashMap<Uuid, CancellationToken>>,
    settings: RwLock<Settings>,
    clock: Arc<dyn Clock>,
    ids: Arc<dyn IdSource>,
    provenance: RwLock<Option<Arc<EventLog>>>,
    results: RwLock<Option<Arc<ResultCache>>>,
    hub: EventHub,
}

impl std::fmt::Debug for Bus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Bus")
            .field("commands", &self.commands.read().keys().collect::<Vec<_>>())
            .finish_non_exhaustive()
    }
}

impl Default for Bus {
    fn default() -> Self {
        Self::new(Arc::new(SystemClock), Arc::new(RandomIds))
    }
}

impl Bus {
    /// A bus with the given clock and id source.
    #[must_use]
    pub fn new(clock: Arc<dyn Clock>, ids: Arc<dyn IdSource>) -> Self {
        Self {
            commands: RwLock::new(BTreeMap::new()),
            inflight: Mutex::new(HashMap::new()),
            settings: RwLock::new(Settings::default()),
            clock,
            ids,
            provenance: RwLock::new(None),
            results: RwLock::new(None),
            hub: EventHub::new(1024),
        }
    }

    /// Store results of cached commands here (the app cache folder).
    pub fn set_result_cache(&self, cache: Option<Arc<ResultCache>>) {
        *self.results.write() = cache;
    }

    /// Record provenance of every non-read command into this log (the open library's).
    pub fn set_provenance_log(&self, log: Option<Arc<EventLog>>) {
        *self.provenance.write() = log;
    }

    /// Replace the policy settings (consents, approvals from the user's settings).
    pub fn set_policy_settings(&self, s: Settings) {
        *self.settings.write() = s;
    }

    /// The event hub.
    #[must_use]
    pub fn events(&self) -> &EventHub {
        &self.hub
    }

    /// Register a typed command. Input and output schemas come from the types.
    ///
    /// # Errors
    /// Bad name or duplicate.
    pub fn register<I, O, F, Fut>(&self, spec: Spec, f: F) -> Result<(), RegisterError>
    where
        I: DeserializeOwned + JsonSchema + Send + 'static,
        O: Serialize + JsonSchema + 'static,
        F: Fn(Cx, I) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<O, CommandError>> + Send + 'static,
    {
        self.insert::<I, O, F, Fut>(spec, false, None, f)
    }

    /// Register a deterministic, expensive command whose results are cached (ADR-011).
    /// Bump `implementation` whenever the output for the same input may change.
    ///
    /// # Errors
    /// Bad name or duplicate.
    pub fn register_cached<I, O, F, Fut>(
        &self,
        spec: Spec,
        implementation: &str,
        f: F,
    ) -> Result<(), RegisterError>
    where
        I: DeserializeOwned + JsonSchema + Send + 'static,
        O: Serialize + JsonSchema + 'static,
        F: Fn(Cx, I) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<O, CommandError>> + Send + 'static,
    {
        self.insert::<I, O, F, Fut>(spec, false, Some(implementation.to_string()), f)
    }

    /// Register a command that sends progress items through [`Cx::progress`].
    ///
    /// # Errors
    /// Bad name or duplicate.
    pub fn register_streaming<I, O, F, Fut>(&self, spec: Spec, f: F) -> Result<(), RegisterError>
    where
        I: DeserializeOwned + JsonSchema + Send + 'static,
        O: Serialize + JsonSchema + 'static,
        F: Fn(Cx, I) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<O, CommandError>> + Send + 'static,
    {
        self.insert::<I, O, F, Fut>(spec, true, None, f)
    }

    fn insert<I, O, F, Fut>(
        &self,
        spec: Spec,
        streaming: bool,
        cached: Option<String>,
        f: F,
    ) -> Result<(), RegisterError>
    where
        I: DeserializeOwned + JsonSchema + Send + 'static,
        O: Serialize + JsonSchema + 'static,
        F: Fn(Cx, I) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<O, CommandError>> + Send + 'static,
    {
        if !valid_name(spec.name) {
            return Err(RegisterError::BadName(spec.name.into()));
        }
        let mut cmds = self.commands.write();
        if cmds.contains_key(spec.name) {
            return Err(RegisterError::Duplicate(spec.name.into()));
        }
        let f = Arc::new(f);
        let handler: Handler = Arc::new(move |cx, input| {
            let f = f.clone();
            Box::pin(async move {
                let input: I =
                    serde_json::from_value(input).map_err(|e| CommandError::InvalidInput {
                        message: e.to_string(),
                    })?;
                let out = f(cx, input).await?;
                serde_json::to_value(out).map_err(|e| CommandError::failed("output", e))
            })
        });
        let info = CommandInfo {
            name: spec.name.into(),
            summary: spec.summary.into(),
            stability: spec.stability,
            permission: spec.permission,
            streaming,
            input_schema: schema_of::<I>(),
            output_schema: schema_of::<O>(),
        };
        cmds.insert(
            spec.name.into(),
            Entry {
                info,
                handler,
                cached,
            },
        );
        Ok(())
    }

    /// Every registered command, sorted by name.
    #[must_use]
    pub fn commands(&self) -> Vec<CommandInfo> {
        self.commands
            .read()
            .values()
            .map(|e| e.info.clone())
            .collect()
    }

    /// Cancel a running call. Returns whether it was running.
    pub fn cancel(&self, id: Uuid) -> bool {
        self.inflight.lock().get(&id).is_some_and(|t| {
            t.cancel();
            true
        })
    }

    /// Call a command.
    ///
    /// # Errors
    /// See [`CommandError`].
    pub async fn call(&self, env: Envelope) -> Result<Value, CommandError> {
        self.dispatch(env, None).await
    }

    /// Call a command, receiving its progress items on `tx`.
    ///
    /// # Errors
    /// See [`CommandError`].
    pub async fn call_with_progress(
        &self,
        env: Envelope,
        tx: mpsc::UnboundedSender<Value>,
    ) -> Result<Value, CommandError> {
        self.dispatch(env, Some(tx)).await
    }

    async fn dispatch(
        &self,
        env: Envelope,
        progress: Option<mpsc::UnboundedSender<Value>>,
    ) -> Result<Value, CommandError> {
        // 1. lookup
        let (handler, info, cached) = {
            let cmds = self.commands.read();
            let e = cmds
                .get(&env.command)
                .ok_or_else(|| CommandError::UnknownCommand {
                    command: env.command.clone(),
                })?;
            (e.handler.clone(), e.info.clone(), e.cached.clone())
        };
        // 2. policy, before anything runs
        let req = Request {
            operation: info.permission.operation,
            data: info.permission.data.clone(),
            destination: info.permission.destination,
            medium: ep_policy::Medium::Text,
        };
        match ep_policy::decide(&req, &self.settings.read()) {
            Decision::Allow { .. } => {}
            Decision::NeedsApproval { rule, .. } if env.approved.contains(&rule) => {}
            Decision::NeedsApproval { rule, reason } => {
                return Err(CommandError::NeedsApproval { rule, reason });
            }
            Decision::Deny { rule, reason } => return Err(CommandError::Denied { rule, reason }),
        }
        // 3. result cache: an identical earlier call answers immediately
        let cache = cached.and_then(|imp| {
            self.results
                .read()
                .clone()
                .map(|c| (c, cache_key(&env.command, &imp, &env.input)))
        });
        if let Some((c, key)) = &cache
            && let Some(v) = c.get(key)
        {
            self.hub.publish("command.finished", json!({ "command": env.command, "id": env.id, "outcome": "cached", "elapsed_ms": 0 }), self.clock.now_ms());
            return Ok(v);
        }
        // 4. cancellation + deadline
        let mut cx = Cx::new(
            env.actor.clone(),
            env.id,
            self.clock.clone(),
            self.ids.clone(),
        );
        if let Some(tx) = progress {
            cx = cx.with_progress(tx);
        }
        let token = cx.cancel.clone();
        self.inflight.lock().insert(env.id, token.clone());
        let deadline = env.deadline_ms.map(Duration::from_millis);
        let started = Instant::now();
        // 5. span + handler
        let span = tracing::info_span!("command", name = %env.command, id = %env.id);
        let run = handler(cx, env.input.clone()).instrument(span);
        let result = tokio::select! {
            r = run => r,
            () = token.cancelled() => Err(CommandError::Cancelled),
            () = sleep_opt(deadline) => {
                token.cancel();
                Err(CommandError::DeadlineExceeded { deadline_ms: env.deadline_ms.unwrap_or(0) })
            }
        };
        self.inflight.lock().remove(&env.id);
        if let (Some((c, key)), Ok(v)) = (&cache, &result)
            && let Err(e) = c.put(key, v)
        {
            tracing::warn!(command = %env.command, %e, "result could not be cached");
        }
        // 6. provenance, after the handler
        let outcome = match &result {
            Ok(_) => json!("ok"),
            Err(e) => serde_json::to_value(e).unwrap_or(Value::Null),
        };
        let elapsed_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
        if info.permission.operation != Operation::Read {
            let log = self.provenance.read().clone();
            if let Some(log) = log {
                let event = Event {
                    at_ms: self.clock.now_ms(),
                    actor: env.actor.clone(),
                    kind: "command.completed".into(),
                    data: json!({ "command": env.command, "id": env.id, "input": logged(&env.input), "outcome": outcome, "elapsed_ms": elapsed_ms }),
                };
                // fsync happens on a blocking thread so the runtime keeps going.
                let appended = tokio::task::spawn_blocking(move || log.append(event)).await;
                if !matches!(appended, Ok(Ok(_))) {
                    tracing::error!(command = %env.command, "provenance event could not be written");
                }
            }
        }
        // 7. finished event (publisher never waits)
        self.hub.publish("command.finished", json!({ "command": env.command, "id": env.id, "outcome": outcome, "elapsed_ms": elapsed_ms }), self.clock.now_ms());
        result
    }
}

/// The input of a command as it is kept in the log: large parts by size and hash only.
fn logged(input: &Value) -> Value {
    let Value::Object(fields) = input else {
        return input.clone();
    };
    let short = |v: &Value| -> Value {
        let text = v.to_string();
        if text.len() <= LOGGED {
            return v.clone();
        }
        json!({ "bytes": text.len(), "hash": Hash::of(text.as_bytes()).to_string() })
    };
    Value::Object(fields.iter().map(|(k, v)| (k.clone(), short(v))).collect())
}

async fn sleep_opt(d: Option<Duration>) {
    match d {
        Some(d) => tokio::time::sleep(d).await,
        None => std::future::pending().await,
    }
}

/// Anything that carries envelopes to a bus: in-process, named pipe, (later) network.
pub trait Transport: Send + Sync {
    /// Call a command.
    fn call(&self, env: Envelope) -> BoxFuture<'_, Result<Value, CommandError>>;
    /// Call a command with progress items sent to `tx`.
    fn call_with_progress(
        &self,
        env: Envelope,
        tx: mpsc::UnboundedSender<Value>,
    ) -> BoxFuture<'_, Result<Value, CommandError>>;
    /// Cancel a running call.
    fn cancel(&self, id: Uuid) -> BoxFuture<'_, ()>;
    /// List commands.
    fn commands(&self) -> BoxFuture<'_, Result<Vec<CommandInfo>, CommandError>>;
}

impl Transport for Bus {
    fn call(&self, env: Envelope) -> BoxFuture<'_, Result<Value, CommandError>> {
        Box::pin(Self::call(self, env))
    }
    fn call_with_progress(
        &self,
        env: Envelope,
        tx: mpsc::UnboundedSender<Value>,
    ) -> BoxFuture<'_, Result<Value, CommandError>> {
        Box::pin(Self::call_with_progress(self, env, tx))
    }
    fn cancel(&self, id: Uuid) -> BoxFuture<'_, ()> {
        Self::cancel(self, id);
        Box::pin(async {})
    }
    fn commands(&self) -> BoxFuture<'_, Result<Vec<CommandInfo>, CommandError>> {
        let c = Self::commands(self);
        Box::pin(async move { Ok(c) })
    }
}

/// Typed call through any transport.
///
/// # Errors
/// The call's error, or `InvalidInput`/`Failed` when (de)serialising.
pub async fn call_typed<I: Serialize, O: DeserializeOwned>(
    t: &(impl Transport + ?Sized),
    command: &str,
    input: &I,
) -> Result<O, CommandError> {
    let input = serde_json::to_value(input).map_err(|e| CommandError::InvalidInput {
        message: e.to_string(),
    })?;
    let out = t.call(Envelope::new(command, input)).await?;
    serde_json::from_value(out).map_err(|e| CommandError::failed("output", e))
}
