//! Inference engine conformance through the bus (Replay), plus one live check with the real
//! llama.cpp sidecar when `EP_LLM_DIR` names a folder with a known GGUF model.
#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    clippy::items_after_statements
)]

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use ep_bus::{Bus, CommandError, Envelope};
use ep_inference::Inference;
use ep_inference::Replay;
use ep_inference::contract::{LlmResponse, Models, Status};
use serde_json::{Value, json};

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..")
}

async fn call<T: serde::de::DeserializeOwned>(
    bus: &Bus,
    c: &str,
    i: Value,
) -> Result<T, CommandError> {
    bus.call(Envelope::new(c, i))
        .await
        .map(|v| serde_json::from_value(v).unwrap())
}

fn bus_with(r: Replay) -> Arc<Bus> {
    let bus = Arc::new(Bus::default());
    Inference::with_backend(Arc::new(r)).register(&bus).unwrap();
    bus
}

#[tokio::test]
async fn streams_tokens_with_prefill_and_data_section() {
    let replay = Replay::new().on(
        "explain.draft",
        "stands for physics-informed neural network.",
    );
    let bus = bus_with(replay.clone());
    let mut sub = bus.events().subscribe("inference.token");
    let r: LlmResponse = call(&bus, "inference.complete", json!({
        "task": "explain.draft",
        "messages": [{ "role": "system", "content": "Explain." }, { "role": "user", "content": "PINN" }],
        "data": "A PINN is trained with the PDE as a loss.",
        "prefill": "PINN ",
        "stream": { "id": "s1" }
    }))
    .await
    .unwrap();
    assert_eq!(
        r.content,
        "PINN stands for physics-informed neural network."
    );
    assert!(r.route_reason.contains("replay") && r.route_reason.contains("local-only"));
    let mut streamed = String::new();
    while let Ok(Some(ep_bus::Recv::Event(e))) =
        tokio::time::timeout(std::time::Duration::from_millis(50), sub.recv())
            .await
            .map(Some)
    {
        assert_eq!(e.data["id"], "s1");
        streamed.push_str(e.data["text"].as_str().unwrap());
    }
    assert_eq!(streamed, "stands for physics-informed neural network.");
    let seen = replay.seen();
    let user = &seen[0].messages[1].content;
    assert!(
        user.starts_with("PINN\n\n--- DATA"),
        "data follows the question: {user}"
    );
    assert_eq!(seen[0].messages.last().unwrap().content, "PINN ");
}

#[tokio::test]
async fn one_space_between_prefill_and_answer() {
    for said in [" is heat.", "is heat."] {
        let bus = bus_with(Replay::new().on("t", said));
        let input = json!({ "task": "t", "messages": [{ "role": "user", "content": "q" }], "prefill": "It " });
        let r: LlmResponse = call(&bus, "inference.complete", input).await.unwrap();
        assert_eq!(r.content, "It is heat.");
    }
}

#[tokio::test]
async fn schema_is_retried_once_then_fails() {
    let schema = json!({ "type": "object", "properties": { "ok": { "type": "boolean" } }, "required": ["ok"], "additionalProperties": false });
    let n = Arc::new(AtomicUsize::new(0));
    let m = n.clone();
    let replay = Replay::new().on_with("t", None, move |_| {
        if m.fetch_add(1, Ordering::SeqCst) == 0 {
            "{\"ok\": \"yes\"}".into()
        } else {
            "{\"ok\": true}".into()
        }
    });
    let bus = bus_with(replay);
    let r: LlmResponse = call(
        &bus,
        "inference.complete",
        json!({
            "task": "t", "messages": [{ "role": "user", "content": "q" }], "schema": schema
        }),
    )
    .await
    .unwrap();
    assert_eq!(r.content, "{\"ok\": true}");
    assert_eq!(n.load(Ordering::SeqCst), 2);

    let bus = bus_with(Replay::new().on("t", "not json"));
    let e = call::<LlmResponse>(
        &bus,
        "inference.complete",
        json!({
            "task": "t", "messages": [{ "role": "user", "content": "q" }], "schema": schema
        }),
    )
    .await
    .unwrap_err();
    assert!(
        matches!(&e, CommandError::Failed { kind, .. } if kind == "bad_output"),
        "{e:?}"
    );
}

#[tokio::test]
async fn unknown_task_and_empty_request_fail() {
    let bus = bus_with(Replay::new());
    let e = call::<LlmResponse>(
        &bus,
        "inference.complete",
        json!({ "task": "x", "messages": [{ "role": "user", "content": "q" }] }),
    )
    .await
    .unwrap_err();
    assert!(matches!(e, CommandError::Failed { .. }));
    let e = call::<LlmResponse>(
        &bus,
        "inference.complete",
        json!({ "task": "x", "messages": [] }),
    )
    .await
    .unwrap_err();
    assert!(matches!(e, CommandError::InvalidInput { .. }));
}

#[tokio::test]
async fn cancel_stops_the_answer() {
    let long = "word ".repeat(5000);
    let bus = bus_with(Replay::new().on("t", &long));
    let env = Envelope::new(
        "inference.complete",
        json!({ "task": "t", "messages": [{ "role": "user", "content": "q" }] }),
    );
    let id = env.id;
    let b = bus.clone();
    let h = tokio::spawn(async move { b.call(env).await });
    tokio::task::yield_now().await;
    tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    assert!(bus.cancel(id));
    let r = h.await.unwrap();
    assert!(matches!(r, Err(CommandError::Cancelled)), "{r:?}");
}

#[tokio::test]
async fn without_a_model_it_says_so() {
    let bus = Arc::new(Bus::default());
    let spawner = Arc::new(ep_os::fake::FakeSpawner::default());
    Inference::new(vec![], vec![], spawner, None, None)
        .register(&bus)
        .unwrap();
    let m: Models = call(&bus, "inference.models", Value::Null).await.unwrap();
    assert_eq!(m.models, []);
    let e = call::<Status>(&bus, "inference.load", Value::Null)
        .await
        .unwrap_err();
    assert!(
        matches!(&e, CommandError::Failed { kind, .. } if kind == "no_runtime" || kind == "no_model"),
        "{e:?}"
    );
}

/// A backend that is not on this machine; it must never be asked in these tests.
struct Remote;
impl ep_inference::InferenceBackend for Remote {
    fn name(&self) -> String {
        "remote".into()
    }
    fn is_local(&self) -> bool {
        false
    }
    fn model(&self) -> Option<String> {
        None
    }
    fn complete<'a>(
        &'a self,
        _: &'a ep_inference::Prepared,
        _: ep_inference::OnToken<'a>,
        _: ep_inference::IsCancelled<'a>,
    ) -> futures::future::BoxFuture<'a, Result<ep_inference::Completion, ep_inference::BackendError>>
    {
        unreachable!("local-only text reached a remote backend")
    }
}

#[tokio::test]
async fn local_only_never_goes_to_a_remote_backend() {
    let bus = Arc::new(Bus::default());
    Inference::with_backend(Arc::new(Remote))
        .register(&bus)
        .unwrap();
    // `sensitivity` left out means local-only.
    let e = call::<LlmResponse>(
        &bus,
        "inference.complete",
        json!({ "task": "t", "messages": [{ "role": "user", "content": "q" }] }),
    )
    .await
    .unwrap_err();
    assert!(
        matches!(&e, CommandError::Failed { kind, .. } if kind == "remote_not_allowed"),
        "{e:?}"
    );
}

#[tokio::test]
async fn models_and_status_answer_while_a_model_runs() {
    let bus = bus_with(Replay::new());
    let both = async {
        let m: Models = call(&bus, "inference.models", Value::Null).await.unwrap();
        let s: Status = call(&bus, "inference.status", Value::Null).await.unwrap();
        (m, s)
    };
    let (_, s) = tokio::time::timeout(std::time::Duration::from_secs(5), both)
        .await
        .expect("hangs");
    assert_eq!(s.model.as_deref(), Some("replay"));
}

/// The GPU holds one model at a time: the live checks take turns.
static GPU: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// Starts a real process for the live check; it ends when the test drops it.
struct StdSpawner;
struct StdChild(std::process::Child);
impl Drop for StdChild {
    fn drop(&mut self) {
        let _ = self.0.kill();
    }
}
impl ep_os::ChildProcess for StdChild {
    fn id(&self) -> u32 {
        self.0.id()
    }
    fn try_wait(&mut self) -> ep_os::Result<Option<i32>> {
        Ok(self.0.try_wait()?.map(|s| s.code().unwrap_or(-1)))
    }
    fn kill(&mut self) -> ep_os::Result<()> {
        Ok(self.0.kill()?)
    }
}
impl ep_os::ProcessSpawner for StdSpawner {
    fn spawn(&self, spec: &ep_os::ProcessSpec) -> ep_os::Result<Box<dyn ep_os::ChildProcess>> {
        let child = std::process::Command::new(&spec.program)
            .args(&spec.args)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()?;
        Ok(Box::new(StdChild(child)))
    }
}

/// The real sidecar: start, stream, JSON schema (spike S3 budgets: ready < 5 s once cached).
#[tokio::test]
async fn live_llamacpp_sidecar() {
    let Some(dir) = std::env::var_os("EP_LLM_DIR").map(PathBuf::from) else {
        eprintln!("skipped: EP_LLM_DIR not set");
        return;
    };
    let spawner: Arc<dyn ep_os::ProcessSpawner> = Arc::new(StdSpawner);
    let _gpu = GPU.lock().await;
    let engine = Inference::new(
        vec![dir],
        vec![root().join(".natives")],
        spawner,
        None,
        None,
    );
    let bus = Arc::new(Bus::default());
    engine.register(&bus).unwrap();
    let m: Models = call(&bus, "inference.models", Value::Null).await.unwrap();
    eprintln!(
        "runtime {:?}; models {:?}",
        m.runtime,
        m.models
            .iter()
            .map(|m| (&m.id, m.state))
            .collect::<Vec<_>>()
    );
    if m.runtime.backend == ep_inference::contract::Backend::None {
        eprintln!("skipped: no llama.cpp build in .natives");
        return;
    }
    let st: Status = call(&bus, "inference.load", Value::Null).await.unwrap();
    eprintln!(
        "loaded {:?} on {:?} in {:?} ms",
        st.model, st.backend, st.ready_ms
    );
    let r: LlmResponse = call(&bus, "inference.complete", json!({
        "task": "test", "messages": [{ "role": "user", "content": "What does CPU stand for? One short sentence." }],
        "prefill": "CPU stands for", "limits": { "max_tokens": 40 }
    }))
    .await
    .unwrap();
    eprintln!("{r:?}");
    assert!(r.content.starts_with("CPU stands for"));
    assert!(
        r.content.to_lowercase().contains("processing"),
        "{}",
        r.content
    );
    let schema = json!({ "type": "object", "properties": { "answer": { "type": "integer" } }, "required": ["answer"], "additionalProperties": false });
    let r: LlmResponse = call(&bus, "inference.complete", json!({
        "task": "test", "messages": [{ "role": "user", "content": "What is 6 times 7? Reply as JSON." }],
        "schema": schema, "limits": { "max_tokens": 30 }
    }))
    .await
    .unwrap();
    let v: Value = serde_json::from_str(&r.content).unwrap();
    assert_eq!(v["answer"], 42, "{}", r.content);
    let _: Status = call(&bus, "inference.unload", Value::Null).await.unwrap();
}

/// Two models, chosen for two tasks, take turns (needs two chat models under `EP_LLM_DIR`).
#[tokio::test]
async fn live_each_task_uses_its_model() {
    let Some(dir) = std::env::var_os("EP_LLM_DIR").map(PathBuf::from) else {
        eprintln!("skipped: EP_LLM_DIR not set");
        return;
    };
    let _gpu = GPU.lock().await;
    let engine = Inference::new(
        vec![dir],
        vec![root().join(".natives")],
        Arc::new(StdSpawner),
        None,
        None,
    );
    let bus = Arc::new(Bus::default());
    engine.register(&bus).unwrap();
    let m: Models = call(&bus, "inference.models", Value::Null).await.unwrap();
    let want = ["qwen3-4b-instruct-2507", "lfm2.5-1.2b-instruct"];
    if !want.iter().all(|w| m.models.iter().any(|x| x.id == *w)) {
        eprintln!("skipped: the two models are not there");
        return;
    }
    let _: Models = call(
        &bus,
        "inference.assign",
        json!({ "task": "check", "model": want[1] }),
    )
    .await
    .unwrap();
    for (task, model) in [
        ("explain.draft", want[0]),
        ("explain.check", want[1]),
        ("explain.draft", want[0]),
    ] {
        let r: LlmResponse = call(&bus, "inference.complete", json!({ "task": task, "messages": [{ "role": "user", "content": "Say OK." }], "limits": { "max_tokens": 5 } })).await.unwrap();
        eprintln!("{task}: {} | {}", r.model, r.route_reason);
        assert_eq!(r.model, model);
    }
    let _: Status = call(&bus, "inference.unload", Value::Null).await.unwrap();
}
