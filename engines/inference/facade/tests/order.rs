//! One model, kept ready, and the next one when it cannot be used (contract 0.4.0), with made-up
//! servers instead of llama.cpp.

#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    clippy::many_single_char_names
)]

use std::io::{Read, Write};
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::time::Duration;

use ep_bus::{Bus, CommandError, Envelope};
use ep_inference::Inference;
use ep_inference::contract::{
    Backend, Fallback, LlmResponse, Models, RunState, Status, VisionAnswer,
};
use ep_os::{ChildProcess, ProcessSpawner, ProcessSpec};
use serde_json::{Value, json};

/// Servers that answer "hi"; models with `broken` in their name end at once.
#[derive(Default)]
struct Servers {
    started: parking_lot::Mutex<Vec<String>>,
    /// Free memory said by `--list-devices`, MiB (0: it says nothing).
    free: AtomicU64,
    /// How long an answer takes.
    slow_ms: AtomicU64,
    answers: Arc<AtomicUsize>,
}

struct Child(Arc<AtomicBool>);
impl ChildProcess for Child {
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

fn arg(spec: &ProcessSpec, name: &str) -> String {
    let i = spec.args.iter().position(|a| a == name).unwrap();
    spec.args[i + 1].to_string_lossy().into_owned()
}

fn read_request(s: &mut std::net::TcpStream) -> String {
    let (mut all, mut buf) = (Vec::new(), [0u8; 4096]);
    loop {
        let n = s.read(&mut buf).unwrap_or(0);
        all.extend_from_slice(&buf[..n]);
        let text = String::from_utf8_lossy(&all).into_owned();
        if let Some((head, body)) = text.split_once("\r\n\r\n") {
            let length = head
                .lines()
                .find_map(|l| l.strip_prefix("Content-Length: "))
                .map_or(0, |v| v.parse().unwrap());
            if body.len() >= length {
                return text;
            }
        }
        if n == 0 {
            return text;
        }
    }
}

impl ProcessSpawner for Servers {
    fn spawn(&self, spec: &ProcessSpec) -> ep_os::Result<Box<dyn ChildProcess>> {
        if spec.args.iter().any(|a| a == "--list-devices") {
            let free = self.free.load(Ordering::SeqCst);
            if free > 0 {
                let said =
                    format!("Available devices:\n  GPU0: Test card (4096 MiB, {free} MiB free)\n");
                std::fs::write(spec.log_file.as_ref().unwrap(), said)?;
            }
            return Ok(Box::new(Child(Arc::new(AtomicBool::new(true)))));
        }
        let model = arg(spec, "-m");
        let name = Path::new(&model)
            .file_stem()
            .unwrap()
            .to_string_lossy()
            .into_owned();
        self.started.lock().push(name.clone());
        let dead = Arc::new(AtomicBool::new(name.contains("broken")));
        if name.contains("broken") {
            return Ok(Box::new(Child(dead)));
        }
        let listener = std::net::TcpListener::bind(format!("127.0.0.1:{}", arg(spec, "--port")))?;
        let (slow, stop) = (self.slow_ms.load(Ordering::SeqCst), dead.clone());
        let answers = self.answers.clone();
        std::thread::spawn(move || {
            for s in listener.incoming() {
                let Ok(mut s) = s else { return };
                if stop.load(Ordering::SeqCst) {
                    return;
                }
                let answers = answers.clone();
                std::thread::spawn(move || {
                    let head = read_request(&mut s);
                    if head.starts_with("GET") {
                        let _ = s.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nok");
                        return;
                    }
                    let warmup = head.contains("Say OK.");
                    let _ = s.write_all(b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n");
                    let mut piece = |t: &str| {
                        let line = format!("data: {t}\n\n");
                        s.write_all(format!("{:x}\r\n{line}\r\n", line.len()).as_bytes())
                            .is_ok()
                            && s.flush().is_ok()
                    };
                    piece(r#"{"choices":[{"delta":{"content":"h"}}]}"#);
                    for _ in 0..(if warmup { 0 } else { slow / 20 }) {
                        std::thread::sleep(Duration::from_millis(20));
                        // The other side went away: the answer was stopped.
                        if !piece(r#"{"choices":[{"delta":{"content":""}}]}"#) {
                            return;
                        }
                    }
                    piece(
                        r#"{"choices":[{"delta":{"content":"i"}}],"usage":{"prompt_tokens":1,"completion_tokens":60}}"#,
                    );
                    piece("[DONE]");
                    let _ = s.write_all(b"0\r\n\r\n");
                    if !warmup {
                        answers.fetch_add(1, Ordering::SeqCst);
                    }
                });
            }
        });
        Ok(Box::new(Child(dead)))
    }
}

/// A GGUF header that says "writes answers" and needs about `mib` for its cache; as many KiB long.
fn gguf(path: &Path, mib: u32) {
    let text = |s: &str| {
        let mut b = (s.len() as u64).to_le_bytes().to_vec();
        b.extend(s.as_bytes());
        b
    };
    let mut b = b"GGUF".to_vec();
    b.extend(3u32.to_le_bytes());
    b.extend(0u64.to_le_bytes());
    b.extend(6u64.to_le_bytes());
    for (k, v) in [
        (
            "general.architecture",
            if path.to_string_lossy().contains("mmproj") {
                "clip"
            } else {
                "llama"
            },
        ),
        ("tokenizer.chat_template", "x"),
    ] {
        b.extend(text(k));
        b.extend(8u32.to_le_bytes());
        b.extend(text(v));
    }
    // A layer needs 8.5 MiB at 8192 tokens.
    for (k, v) in [
        ("llama.block_count", mib * 2 / 17),
        ("llama.attention.head_count", 8),
        ("llama.attention.head_count_kv", 8),
        ("llama.attention.key_length", 64),
    ] {
        b.extend(text(k));
        b.extend(4u32.to_le_bytes());
        b.extend(v.to_le_bytes());
    }
    b.resize(b.len().max((mib as usize) << 10), 0);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, b).unwrap();
}

struct App {
    bus: Arc<Bus>,
    servers: Arc<Servers>,
    dir: tempfile::TempDir,
    engine: Arc<Inference>,
}

fn app(models: &[(&str, u32)]) -> App {
    let dir = tempfile::tempdir().unwrap();
    for (name, mib) in models {
        gguf(&dir.path().join(format!("models/{name}.gguf")), *mib);
    }
    let exe = if cfg!(windows) {
        "llama-server.exe"
    } else {
        "llama-server"
    };
    for b in ["llama-cpp-vulkan", "llama-cpp-cpu"] {
        let d = dir.path().join("rt").join(b);
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(d.join(exe), b"x").unwrap();
    }
    let servers = Arc::new(Servers::default());
    let engine = Inference::new(
        vec![dir.path().join("models")],
        vec![dir.path().join("rt")],
        servers.clone(),
        Some(dir.path().join("llama.log")),
        Some(dir.path().join("llm.json")),
    );
    let bus = Arc::new(Bus::default());
    engine.register(&bus).unwrap();
    App {
        bus,
        servers,
        dir,
        engine,
    }
}

impl App {
    async fn call<T: serde::de::DeserializeOwned>(
        &self,
        c: &str,
        i: Value,
    ) -> Result<T, CommandError> {
        self.bus
            .call(Envelope::new(c, i))
            .await
            .map(|v| serde_json::from_value(v).unwrap())
    }
    async fn order(&self, ids: &[&str]) -> Models {
        // Nothing is loaded by choosing.
        self.call(
            "inference.choose",
            json!({ "order": ids, "keep_ready": false }),
        )
        .await
        .unwrap()
    }
    async fn ask(&self, task: &str) -> LlmResponse {
        self.call(
            "inference.complete",
            json!({ "task": task, "messages": [{ "role": "user", "content": "q" }] }),
        )
        .await
        .unwrap()
    }
    async fn status(&self) -> Status {
        self.call("inference.status", Value::Null).await.unwrap()
    }
    fn started(&self) -> Vec<String> {
        self.servers.started.lock().clone()
    }
    fn on_gpu(&self) -> bool {
        self.engine.runtime().0 != Backend::Cpu
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn one_model_answers_every_task_and_stays_loaded() {
    let a = app(&[("first", 900), ("second", 500)]);
    a.order(&["first", "second"]).await;
    for task in [
        "explain.draft",
        "find.understand",
        "sort.names",
        "digest.section",
    ] {
        assert_eq!(a.ask(task).await.model, "first");
    }
    assert_eq!(a.started(), ["first"], "loaded once");
    assert_eq!(a.status().await.state, RunState::Ready);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_model_that_does_not_start_gives_way_to_the_next() {
    let a = app(&[("broken", 900), ("second", 500)]);
    a.order(&["broken", "second"]).await;
    let mut sub = a.bus.events().subscribe("model.fallback");
    let r = a.ask("explain.draft").await;
    assert_eq!(r.model, "second");
    assert!(
        r.route_reason.contains("did not start"),
        "{}",
        r.route_reason
    );
    let s = a.status().await;
    assert_eq!(
        (s.wanted.as_deref(), s.fallback),
        (Some("broken"), Some(Fallback::StartFailed))
    );
    let Ok(ep_bus::Recv::Event(e)) = tokio::time::timeout(Duration::from_secs(1), sub.recv()).await
    else {
        panic!("nothing was said")
    };
    assert_eq!(
        (e.data["wanted"].as_str(), e.data["used"].as_str()),
        (Some("broken"), Some("second"))
    );
    // It is not tried again for every answer; its problem is shown.
    let tries = a.started().iter().filter(|x| *x == "broken").count();
    a.ask("explain.draft").await;
    assert_eq!(a.started().iter().filter(|x| *x == "broken").count(), tries);
    let m: Models = a.call("inference.models", Value::Null).await.unwrap();
    assert!(
        m.models
            .iter()
            .find(|x| x.id == "broken")
            .unwrap()
            .problem
            .is_some()
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_full_memory_means_the_next_model() {
    let a = app(&[("large", 2000), ("small", 400)]);
    if !a.on_gpu() {
        return;
    }
    a.order(&["large", "small"]).await;
    a.servers.free.store(1200, Ordering::SeqCst);
    let r = a.ask("explain.draft").await;
    assert_eq!(r.model, "small");
    assert!(r.route_reason.contains("memory"), "{}", r.route_reason);
    assert_eq!(a.started(), ["small"], "the large one was not even tried");
    assert_eq!(a.status().await.fallback, Some(Fallback::NotEnoughMemory));
    let m: Models = a.call("inference.models", Value::Null).await.unwrap();
    assert_eq!(m.memory.unwrap().free >> 20, 1200);
    assert_eq!(
        (m.order, m.own_order),
        (vec!["large".to_string(), "small".into()], true)
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn what_other_programs_use_counts() {
    let a = app(&[("large", 2000), ("small", 400)]);
    if !a.on_gpu() {
        return;
    }
    let machine = Arc::new(ep_os::fake::FakeMachine::default());
    *machine.used.lock() = Some(3000 << 20);
    a.engine.use_machine(machine.clone());
    a.order(&["large", "small"]).await;
    // The card says that nearly all is free, as Windows does.
    a.servers.free.store(4000, Ordering::SeqCst);
    assert_eq!(a.ask("explain.draft").await.model, "small");
    // It is free again: the first choice, as soon as the order is gone through again.
    *machine.used.lock() = Some(0);
    a.order(&["large", "small"]).await;
    for _ in 0..80 {
        if a.status().await.model.as_deref() == Some("large") {
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    assert_eq!(a.ask("explain.draft").await.model, "large");
    assert_eq!(a.status().await.fallback, None);
}

#[tokio::test(flavor = "multi_thread")]
async fn the_order_is_kept_and_can_be_automatic_again() {
    let a = app(&[("large", 2000), ("small", 400)]);
    let m: Models = a.call("inference.models", Value::Null).await.unwrap();
    assert_eq!(
        (m.order.clone(), m.own_order, m.keep_ready),
        (vec!["large".into(), "small".into()], false, true)
    );
    a.order(&["small", "large", "small"]).await;
    let again = Inference::new(
        vec![a.dir.path().join("models")],
        vec![],
        a.servers.clone(),
        None,
        Some(a.dir.path().join("llm.json")),
    );
    let bus = Arc::new(Bus::default());
    again.register(&bus).unwrap();
    let m: Models = serde_json::from_value(
        bus.call(Envelope::new("inference.models", Value::Null))
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(
        (m.order, m.own_order, m.keep_ready),
        (vec!["small".to_string(), "large".into()], true, false)
    );
    let m = a.order(&[]).await;
    assert!(!m.own_order && m.order == ["large", "small"]);
    let e = a
        .call::<Models>("inference.choose", json!({ "order": ["nope"] }))
        .await
        .unwrap_err();
    assert!(
        matches!(&e, CommandError::Failed { kind, .. } if kind == "no_model"),
        "{e:?}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_model_is_made_ready_before_it_is_asked() {
    let a = app(&[("first", 900)]);
    let _: Status = a.call("inference.warm", Value::Null).await.unwrap();
    assert_eq!(a.status().await.state, RunState::Ready);
    assert_eq!(a.started(), ["first"]);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_background_answer_gives_way_to_a_person_and_is_begun_again() {
    let a = app(&[("first", 900)]);
    a.order(&["first"]).await;
    a.ask("warm").await;
    a.servers.slow_ms.store(1500, Ordering::SeqCst);
    let _: Status = a.call("inference.unload", Value::Null).await.unwrap();
    let bus = a.bus.clone();
    let input = json!({ "task": "digest.section", "background": true, "messages": [{ "role": "user", "content": "q" }] });
    let background =
        tokio::spawn(async move { bus.call(Envelope::new("inference.complete", input)).await });
    tokio::time::sleep(Duration::from_millis(900)).await;
    a.ask("explain.draft").await;
    assert!(!background.is_finished(), "the person came first");
    assert!(background.await.unwrap().is_ok(), "and nothing was lost");
    // Warm, the person, the background: the one that was stopped did not end.
    assert_eq!(a.servers.answers.load(Ordering::SeqCst), 3);
}

/// A picture in a folder of the test, and how the engine finds it.
fn picture(a: &App) -> Value {
    let (file, bytes) = (a.dir.path().join("p.png"), b"not really a picture");
    std::fs::write(&file, bytes).unwrap();
    a.engine.use_pictures(Arc::new(move |_| Some(file.clone())));
    json!({
        "image": { "hash": "00".repeat(32), "size": bytes.len(), "media_type": "image/png" },
        "question": "What is it?"
    })
}

#[tokio::test(flavor = "multi_thread")]
async fn pictures_are_read_with_consent_by_the_model_that_can() {
    let a = app(&[("first", 900)]);
    a.order(&["first"]).await;
    let p = picture(&a);
    let read = async |pictures: Value, consent| {
        a.call::<VisionAnswer>(
            "inference.vision_read",
            json!({ "pictures": pictures, "consent": consent }),
        )
        .await
    };
    let kind = |r: Result<VisionAnswer, CommandError>| match r.unwrap_err() {
        CommandError::Failed { kind, .. } => kind,
        e => e.to_string(),
    };
    assert_eq!(kind(read(json!([p]), false).await), "consent_needed");
    assert_eq!(kind(read(json!([p]), true).await), "no_vision_model");
    assert!(matches!(
        read(json!([]), true).await,
        Err(CommandError::InvalidInput { .. })
    ));
    assert_eq!(a.started().len(), 0);

    let a = app(&[
        ("first", 900),
        ("v/see-vl-q4", 300),
        ("v/mmproj-see-vl", 100),
    ]);
    a.order(&["first", "see-vl-q4"]).await;
    let m: Models = a.call("inference.models", Value::Null).await.unwrap();
    assert_eq!(m.vision.unwrap().id, "see-vl-q4");
    a.ask("explain.draft").await;
    let mut sub = a.bus.events().subscribe("inference.vision");
    let r: VisionAnswer = a
        .call(
            "inference.vision_read",
            json!({ "pictures": [picture(&a), picture(&a)], "consent": true,
                "background": true, "stream": { "id": "f" } }),
        )
        .await
        .unwrap();
    assert_eq!((r.texts.len(), r.model.as_str()), (2, "see-vl-q4"));
    for done in 1..=2 {
        let Ok(ep_bus::Recv::Event(e)) =
            tokio::time::timeout(Duration::from_secs(1), sub.recv()).await
        else {
            panic!("nothing was said")
        };
        assert_eq!(
            (&e.data["done"], &e.data["text"]),
            (&json!(done), &json!("hi"))
        );
    }
    // Afterwards questions are answered by the answering model, though the other is in the order.
    a.call::<Models>("inference.choose", json!({ "keep_ready": true }))
        .await
        .unwrap();
    assert_eq!(a.ask("explain.draft").await.model, "first");
    assert_eq!(a.started(), ["first", "see-vl-q4", "first"]);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_person_goes_before_the_pictures_and_digests_after_them() {
    let a = app(&[
        ("first", 900),
        ("v/see-vl-q4", 300),
        ("v/mmproj-see-vl", 100),
    ]);
    a.order(&["first"]).await;
    a.servers.slow_ms.store(700, Ordering::SeqCst);
    let p = picture(&a);
    let (bus, pictures) = (a.bus.clone(), json!([p, p]));
    let batch = tokio::spawn(async move {
        bus.call(Envelope::new(
            "inference.vision_read",
            json!({ "pictures": pictures, "consent": true, "background": true }),
        ))
        .await
        .unwrap();
        std::time::Instant::now()
    });
    while a.started().is_empty() {
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    tokio::time::sleep(Duration::from_millis(300)).await;
    let bus = a.bus.clone();
    let digest = tokio::spawn(async move {
        bus.call(Envelope::new(
            "inference.complete",
            json!({ "task": "digest.section", "background": true,
                "messages": [{ "role": "user", "content": "q" }] }),
        ))
        .await
        .unwrap();
        std::time::Instant::now()
    });
    assert_eq!(a.ask("explain.draft").await.model, "first");
    let asked = std::time::Instant::now();
    let (batch, digest) = (batch.await.unwrap(), digest.await.unwrap());
    assert!(asked < batch, "the person did not wait for the pictures");
    assert!(batch < digest, "the digest waited for them");
    assert_eq!(a.started(), ["see-vl-q4", "first", "see-vl-q4", "first"]);
}
