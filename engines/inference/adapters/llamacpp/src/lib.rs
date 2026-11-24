//! `LlamaCppSidecar`: the local model runtime (ADR-005). A pinned `llama-server` build runs as
//! a child process bound to 127.0.0.1 on a free port with a per-launch key; the engine talks to
//! its OpenAI-compatible API (streamed chat, prefill by a trailing assistant message, JSON
//! Schema). The sidecar is treated as hostile-until-proven: its output is parsed defensively
//! and size-limited, and it is restarted once when it stops answering.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use ep_inference_contract::Role;
use ep_inference_core::{
    BackendError, Completion, InferenceBackend, IsCancelled, OnToken, Prepared,
};
use futures::future::BoxFuture;
use serde_json::{Value, json};
use std::sync::Arc;

use ep_os::{ChildProcess, ProcessSpawner, ProcessSpec};
use tokio::sync::Mutex;

pub mod devices;
pub mod http;

/// How long a model may take to load.
const START_LIMIT: Duration = Duration::from_secs(75);
/// How long the server may write nothing before it counts as stuck (reading a long text on a slow
/// processor takes a while; a stuck graphics driver never ends).
const STALL: Duration = Duration::from_secs(150);
/// Memory for texts read before (the server's own limit is 8 GB).
const PROMPT_CACHE_MIB: &str = "512";

/// What to run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    /// `llama-server` executable.
    pub exe: PathBuf,
    /// GGUF model file.
    pub model: PathBuf,
    /// Model id reported in answers.
    pub model_id: String,
    /// Context length.
    pub context: u32,
    /// Layers on the GPU (0 for the CPU build); llama.cpp chooses when missing (large models).
    pub gpu_layers: Option<u32>,
    /// Graphics card (`-dev`), when more than one is there.
    pub device: Option<String>,
    /// 8-bit cache: half the memory, as good (a model that cannot do it is started without).
    pub compact: bool,
    /// Backend name for route reasons (`cuda`, `vulkan`, `cpu`).
    pub backend: String,
    /// The server's own log (otherwise discarded).
    pub log_file: Option<PathBuf>,
}

struct Running {
    child: Box<dyn ChildProcess>,
    port: u16,
    key: String,
}

/// The sidecar. Started on first use (or by [`LlamaCppSidecar::start`]).
pub struct LlamaCppSidecar {
    config: Config,
    spawner: Arc<dyn ProcessSpawner>,
    running: Mutex<Option<Running>>,
}

impl LlamaCppSidecar {
    /// A sidecar for `config` (not started). `spawner` ties the child to the app's lifetime
    /// (a kill-on-close Job Object on Windows, ADR-009).
    #[must_use]
    pub fn new(config: Config, spawner: Arc<dyn ProcessSpawner>) -> Self {
        Self {
            config,
            spawner,
            running: Mutex::new(None),
        }
    }

    /// The configuration.
    #[must_use]
    pub fn config(&self) -> &Config {
        &self.config
    }

    /// Start it (if not running) and wait until it answers. Returns the time taken.
    ///
    /// # Errors
    /// The build or model is missing, or it did not become ready.
    pub async fn start(&self) -> Result<Duration, BackendError> {
        let mut g = self.running.lock().await;
        self.ensure(&mut g).await
    }

    /// Stop it.
    pub async fn stop(&self) {
        if let Some(mut r) = self.running.lock().await.take() {
            let _ = r.child.kill();
        }
    }

    /// Whether the child has ended. A broken connection can be seen a moment before the end of
    /// the process, so it is given a moment.
    async fn has_ended(&self) -> bool {
        for _ in 0..10 {
            if !self.is_running().await {
                return true;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        false
    }

    /// Whether the child is alive.
    pub async fn is_running(&self) -> bool {
        let mut g = self.running.lock().await;
        match g.as_mut() {
            Some(r) => matches!(r.child.try_wait(), Ok(None)),
            None => false,
        }
    }

    async fn ensure(&self, g: &mut Option<Running>) -> Result<Duration, BackendError> {
        if let Some(r) = g.as_mut() {
            if matches!(r.child.try_wait(), Ok(None)) {
                return Ok(Duration::ZERO);
            }
            *g = None;
        }
        let c = &self.config;
        if !c.exe.is_file() {
            return Err(BackendError::NoRuntime(c.exe.display().to_string()));
        }
        if !c.model.is_file() {
            return Err(BackendError::NoModel(c.model.display().to_string()));
        }
        let started = Instant::now();
        let port = free_port().map_err(|e| BackendError::StartFailed(e.to_string()))?;
        let key = uuid::Uuid::new_v4().simple().to_string();
        let spec = self.spec(port, &key);
        let mut child = self
            .spawner
            .spawn(&spec)
            .map_err(|e| BackendError::StartFailed(e.to_string()))?;
        loop {
            if let Ok(Some(code)) = child.try_wait() {
                return Err(BackendError::StartFailed(format!(
                    "llama-server exited (code {code})"
                )));
            }
            if started.elapsed() > START_LIMIT {
                let _ = child.kill();
                return Err(BackendError::StartFailed(
                    "the model took too long to load".into(),
                ));
            }
            if let Ok(r) = http::request(port, "GET", "/health", &key, None).await
                && r.status == 200
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(150)).await;
        }
        *g = Some(Running { child, port, key });
        Ok(started.elapsed())
    }

    /// How the server is started: this machine only, its own key (contract: sidecar rule 1).
    fn spec(&self, port: u16, key: &str) -> ProcessSpec {
        let c = &self.config;
        ProcessSpec {
            program: c.exe.clone(),
            args: [
                "-m".into(),
                c.model.clone().into_os_string(),
                "--host".into(),
                "127.0.0.1".into(),
                "--port".into(),
                port.to_string().into(),
                "--api-key".into(),
                key.into(),
                "-c".into(),
                c.context.to_string().into(),
                "-np".into(),
                "1".into(),
                "--no-webui".into(),
                "-cram".into(),
                PROMPT_CACHE_MIB.into(),
            ]
            .into_iter()
            .chain(
                c.gpu_layers
                    .into_iter()
                    .flat_map(|n| ["-ngl".into(), n.to_string().into()]),
            )
            .chain(
                c.device
                    .iter()
                    .flat_map(|d| ["-dev".into(), d.clone().into()]),
            )
            .chain(
                ["-fa", "on", "-ctk", "q8_0", "-ctv", "q8_0"]
                    .into_iter()
                    .filter(|_| c.compact)
                    .map(Into::into),
            )
            .collect(),
            cwd: c.exe.parent().map(Path::to_path_buf),
            env: vec![],
            log_file: c.log_file.clone(),
        }
    }

    fn body(&self, req: &Prepared) -> Value {
        let messages: Vec<Value> = req
            .messages
            .iter()
            .map(|m| {
                let role = match m.role {
                    Role::System => "system",
                    Role::User => "user",
                    Role::Assistant => "assistant",
                };
                json!({ "role": role, "content": m.content })
            })
            .collect();
        let mut body = json!({
            "model": self.config.model_id,
            "messages": messages,
            "max_tokens": req.max_tokens,
            "temperature": req.temperature,
            "stream": true,
            "stream_options": { "include_usage": true },
        });
        if let Some(schema) = &req.schema {
            // The answer starts at once: without this a model may "think" first, which the schema
            // does not hold and which uses up the limit (founder report, 2026-11-24).
            body["reasoning_format"] = json!("none");
            body["response_format"] = json!({
                "type": "json_schema",
                "json_schema": { "name": "answer", "schema": schema, "strict": true }
            });
        }
        body
    }

    async fn generate(
        &self,
        req: &Prepared,
        on_token: OnToken<'_>,
        cancelled: IsCancelled<'_>,
    ) -> Result<Completion, BackendError> {
        let (port, key) = {
            let mut g = self.running.lock().await;
            self.ensure(&mut g).await?;
            let r = g
                .as_ref()
                .ok_or_else(|| BackendError::Failed("not running".into()))?;
            (r.port, r.key.clone())
        };
        let body =
            serde_json::to_vec(&self.body(req)).map_err(|e| BackendError::Failed(e.to_string()))?;
        let mut resp = http::request(port, "POST", "/v1/chat/completions", &key, Some(&body))
            .await
            .map_err(|e| BackendError::Failed(e.to_string()))?;
        if resp.status != 200 {
            let status = resp.status;
            let text = resp.text().await.unwrap_or_default();
            return Err(BackendError::Failed(format!(
                "HTTP {status}: {}",
                text.chars().take(300).collect::<String>()
            )));
        }
        let mut out = Answer::default();
        // Bytes, not text: a letter may be split between two reads.
        let mut pending: Vec<u8> = Vec::new();
        let mut heard = Instant::now();
        loop {
            if cancelled() {
                // Dropping the connection stops the slot in llama-server.
                return Err(BackendError::Cancelled);
            }
            let next = tokio::time::timeout(Duration::from_millis(100), resp.next()).await;
            let bytes = match next {
                Err(_) if heard.elapsed() > STALL => {
                    // A stuck server is ended, so that it can be started again.
                    if let Some(r) = self.running.lock().await.as_mut() {
                        let _ = r.child.kill();
                    }
                    return Err(BackendError::Failed("the model stopped answering".into()));
                }
                Err(_) => continue,
                Ok(Err(e)) => return Err(BackendError::Failed(e.to_string())),
                Ok(Ok(None)) => break,
                Ok(Ok(Some(b))) => b,
            };
            heard = Instant::now();
            pending.extend(bytes);
            if take_lines(&mut pending, &mut out, on_token) {
                break;
            }
        }
        sse_line(String::from_utf8_lossy(&pending).trim(), &mut out, on_token);
        match out.error.take() {
            Some(e) => Err(BackendError::Failed(e)),
            None => Ok(out.done),
        }
    }
}

/// What was read of an answer.
#[derive(Debug, Default)]
struct Answer {
    done: Completion,
    /// The server said what went wrong (after it had begun to answer).
    error: Option<String>,
}

/// Read the whole lines of `pending`, keeping the rest; true at `[DONE]`.
fn take_lines(pending: &mut Vec<u8>, out: &mut Answer, on_token: OnToken<'_>) -> bool {
    while let Some(i) = pending.iter().position(|b| *b == b'\n') {
        let line: Vec<u8> = pending.drain(..=i).collect();
        if sse_line(String::from_utf8_lossy(&line).trim(), out, on_token) {
            return true;
        }
    }
    false
}

/// Read one server-sent-events line; true at `[DONE]`. Broken lines are skipped.
fn sse_line(line: &str, out: &mut Answer, on_token: OnToken<'_>) -> bool {
    let Some(data) = line.strip_prefix("data:") else {
        return false;
    };
    let data = data.trim();
    if data == "[DONE]" {
        return true;
    }
    let Ok(ev) = serde_json::from_str::<Value>(data) else {
        return false;
    };
    if let Some(e) = ev.get("error").filter(|e| !e.is_null()) {
        let said = e.get("message").and_then(Value::as_str).unwrap_or("error");
        out.error = Some(said.chars().take(300).collect());
        return true;
    }
    let out = &mut out.done;
    if ev
        .pointer("/choices/0/finish_reason")
        .and_then(Value::as_str)
        == Some("length")
    {
        out.cut = true;
    }
    if let Some(piece) = ev
        .pointer("/choices/0/delta/content")
        .and_then(Value::as_str)
        .filter(|p| !p.is_empty())
    {
        on_token(piece);
        out.text.push_str(piece);
    }
    if let Some(u) = ev.get("usage").filter(|u| u.is_object()) {
        let n = |k: &str| {
            u.get(k)
                .and_then(Value::as_u64)
                .and_then(|v| u32::try_from(v).ok())
                .unwrap_or(0)
        };
        out.prompt_tokens = n("prompt_tokens");
        out.completion_tokens = n("completion_tokens");
    }
    false
}

fn free_port() -> std::io::Result<u16> {
    let l = std::net::TcpListener::bind(("127.0.0.1", 0))?;
    Ok(l.local_addr()?.port())
}

impl InferenceBackend for LlamaCppSidecar {
    fn name(&self) -> String {
        format!("llama.cpp/{}", self.config.backend)
    }
    fn is_local(&self) -> bool {
        true
    }
    fn model(&self) -> Option<String> {
        Some(self.config.model_id.clone())
    }
    fn complete<'a>(
        &'a self,
        req: &'a Prepared,
        on_token: OnToken<'a>,
        cancelled: IsCancelled<'a>,
    ) -> BoxFuture<'a, Result<Completion, BackendError>> {
        Box::pin(async move {
            match self.generate(req, on_token, cancelled).await {
                // Restart once when it stopped answering (contract: sidecar rule 3).
                Err(BackendError::Failed(e)) if self.has_ended().await => {
                    tracing::warn!(%e, "llama-server stopped; restarting once");
                    self.generate(req, on_token, cancelled).await
                }
                r => r,
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use std::io::{Read, Write};
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

    use super::*;

    #[test]
    fn sse_lines_are_read_defensively() {
        let mut c = Answer::default();
        let got = parking_lot::Mutex::new(String::new());
        let on = |t: &str| got.lock().push_str(t);
        let mut feed = |l: &str| sse_line(l, &mut c, &on);
        assert!(!feed(r#"data: {"choices":[{"delta":{"content":"Hel"}}]}"#));
        assert!(!feed("data: {broken"));
        assert!(!feed(": keep-alive"));
        assert!(!feed(
            r#"data: {"choices":[{"delta":{"content":"lo"}}],"usage":{"prompt_tokens":7,"completion_tokens":2}}"#
        ));
        assert!(!feed(
            r#"data: {"choices":[{"delta":{},"finish_reason":"length"}]}"#
        ));
        assert!(feed("data: [DONE]"));
        let e = r#"data: {"error":{"message":"too long"}}"#;
        assert!(feed(e));
        assert_eq!(c.error.as_deref(), Some("too long"));
        let c = c.done;
        assert!(c.cut);
        assert_eq!(c.text, "Hello");
        assert_eq!(*got.lock(), "Hello");
        assert_eq!((c.prompt_tokens, c.completion_tokens), (7, 2));
    }

    #[test]
    fn letters_split_between_reads_stay_whole() {
        let line = "data: {\"choices\":[{\"delta\":{\"content\":\"∇·u = α\"}}]}\n".as_bytes();
        let (mut pending, mut c) = (Vec::new(), Answer::default());
        for b in line {
            pending.push(*b);
            assert!(!take_lines(&mut pending, &mut c, &|_| {}));
        }
        assert_eq!(c.done.text, "∇·u = α");
    }

    /// Starts a tiny server instead of a process: healthy, checks the key, answers one request per
    /// life; `die_first` ends the first life in the middle of its answer.
    struct FakeServer {
        starts: Arc<AtomicUsize>,
        die_first: bool,
        specs: parking_lot::Mutex<Vec<ProcessSpec>>,
    }
    struct FakeChild(Arc<AtomicBool>);
    impl ChildProcess for FakeChild {
        fn id(&self) -> u32 {
            1
        }
        fn try_wait(&mut self) -> ep_os::Result<Option<i32>> {
            Ok(self.0.load(Ordering::SeqCst).then_some(1))
        }
        fn kill(&mut self) -> ep_os::Result<()> {
            self.0.store(true, Ordering::SeqCst);
            Ok(())
        }
    }
    /// A whole request, body included: closing with unread bytes would reset the connection.
    fn read_request(s: &mut std::net::TcpStream) -> String {
        let (mut all, mut buf) = (Vec::new(), [0u8; 4096]);
        loop {
            let n = s.read(&mut buf).unwrap();
            all.extend_from_slice(&buf[..n]);
            let text = String::from_utf8_lossy(&all).into_owned();
            let Some((head, body)) = text.split_once("\r\n\r\n") else {
                assert_ne!(n, 0, "closed before the headers");
                continue;
            };
            let length = head
                .lines()
                .find_map(|l| l.strip_prefix("Content-Length: "))
                .map_or(0, |v| v.parse().unwrap());
            if body.len() >= length || n == 0 {
                return text;
            }
        }
    }
    fn arg(spec: &ProcessSpec, name: &str) -> String {
        let i = spec.args.iter().position(|a| a == name).unwrap();
        spec.args[i + 1].to_string_lossy().into_owned()
    }
    impl ProcessSpawner for FakeServer {
        fn spawn(&self, spec: &ProcessSpec) -> ep_os::Result<Box<dyn ChildProcess>> {
            let life = self.starts.fetch_add(1, Ordering::SeqCst);
            self.specs.lock().push(spec.clone());
            let dead = Arc::new(AtomicBool::new(false));
            let (stop, die) = (dead.clone(), self.die_first && life == 0);
            let key = format!("Bearer {}", arg(spec, "--api-key"));
            let listener = std::net::TcpListener::bind(format!(
                "{}:{}",
                arg(spec, "--host"),
                arg(spec, "--port")
            ))
            .unwrap();
            std::thread::spawn(move || {
                for s in listener.incoming() {
                    let mut s = s.unwrap();
                    let head = read_request(&mut s);
                    if !head.contains(&key) {
                        let _ = s.write_all(b"HTTP/1.1 401 No\r\nContent-Length: 0\r\n\r\n");
                    } else if head.starts_with("GET /health") {
                        let _ = s.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nok");
                    } else if die {
                        drop(s);
                        std::thread::sleep(Duration::from_millis(120));
                        stop.store(true, Ordering::SeqCst);
                        return;
                    } else {
                        let body = "data: {\"choices\":[{\"delta\":{\"content\":\"hi\"}}]}\n\ndata: [DONE]\n\n";
                        let _ = write!(
                            s,
                            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n{body}",
                            body.len()
                        );
                    }
                }
            });
            Ok(Box::new(FakeChild(dead)))
        }
    }

    /// A sidecar on the fake server; the program and model only have to be files.
    fn sidecar(die_first: bool) -> (LlamaCppSidecar, Arc<FakeServer>) {
        let file = std::env::current_exe().unwrap();
        let server = Arc::new(FakeServer {
            starts: Arc::default(),
            die_first,
            specs: parking_lot::Mutex::default(),
        });
        let config = Config {
            exe: file.clone(),
            model: file,
            model_id: "m".into(),
            context: 4096,
            gpu_layers: Some(0),
            device: None,
            compact: true,
            backend: "cpu".into(),
            log_file: None,
        };
        (LlamaCppSidecar::new(config, server.clone()), server)
    }

    fn question() -> Prepared {
        Prepared {
            task: "t".into(),
            messages: vec![],
            schema: None,
            max_tokens: 8,
            temperature: 0.0,
        }
    }

    #[tokio::test]
    async fn the_sidecar_is_loopback_only_with_its_own_key() {
        let (s, server) = sidecar(false);
        let c = s.complete(&question(), &|_| {}, &|| false).await.unwrap();
        assert_eq!(c.text, "hi");
        s.stop().await;
        let _ = s.start().await.unwrap();
        let specs = server.specs.lock().clone();
        assert_eq!(specs.len(), 2);
        for spec in &specs {
            assert_eq!(arg(spec, "--host"), "127.0.0.1");
            assert_eq!(arg(spec, "--api-key").len(), 32);
        }
        assert_ne!(
            arg(&specs[0], "--api-key"),
            arg(&specs[1], "--api-key"),
            "a new key every start"
        );
    }

    #[tokio::test]
    async fn a_dead_sidecar_is_restarted_once() {
        let (s, server) = sidecar(true);
        let c = s.complete(&question(), &|_| {}, &|| false).await.unwrap();
        assert_eq!(c.text, "hi");
        assert_eq!(server.starts.load(Ordering::SeqCst), 2);
    }
}
