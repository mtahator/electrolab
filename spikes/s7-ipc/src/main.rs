//! Spike P0.S7 — the command/event bus between processes over a Windows named pipe. Throw-away code.
//!
//!   s7-ipc                 run every measurement (parent role), print one JSON report
//!   s7-ipc child <pipe>    engine role (spawned by the parent)
//!   s7-ipc orphan <job|nojob>  helper: spawn a child, print its pid, exit abruptly
//!
//! Wire format: 4-byte little-endian length + JSON envelope (Draft 2.2 §9.1, simplified).

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::net::windows::named_pipe::{ClientOptions, ServerOptions};
use tokio::sync::mpsc;

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq)]
enum Kind { Request, Response, Event, Done, Cancel }

#[derive(Serialize, Deserialize, Debug, Clone)]
struct Envelope { id: u64, correlation: u64, kind: Kind, command: String, body: Value }

async fn read_frame<R: AsyncRead + Unpin>(r: &mut R) -> std::io::Result<Envelope> {
    let n = r.read_u32_le().await? as usize;
    let mut buf = vec![0u8; n];
    r.read_exact(&mut buf).await?;
    serde_json::from_slice(&buf).map_err(std::io::Error::other)
}

async fn write_frame<W: AsyncWrite + Unpin>(w: &mut W, e: &Envelope) -> std::io::Result<()> {
    let b = serde_json::to_vec(e).map_err(std::io::Error::other)?;
    let mut frame = Vec::with_capacity(b.len() + 4);
    frame.extend_from_slice(&(b.len() as u32).to_le_bytes());
    frame.extend_from_slice(&b);
    w.write_all(&frame).await
}

// ---------------------------------------------------------------- child (engine) ---------
async fn child(pipe: String) {
    let client = loop {
        match ClientOptions::new().open(&pipe) {
            Ok(c) => break c,
            Err(_) => tokio::time::sleep(Duration::from_millis(1)).await,
        }
    };
    let (mut rd, mut wr) = tokio::io::split(client);
    let (tx, mut rx) = mpsc::unbounded_channel::<Envelope>();
    tokio::spawn(async move { while let Some(e) = rx.recv().await { if write_frame(&mut wr, &e).await.is_err() { break; } } });
    let cancels: Arc<Mutex<HashMap<u64, (Arc<AtomicBool>, Arc<tokio::sync::Notify>)>>> = Arc::default();
    while let Ok(req) = read_frame(&mut rd).await {
        let tx = tx.clone();
        match (req.kind, req.command.as_str()) {
            (Kind::Cancel, _) => { if let Some((f, n)) = cancels.lock().unwrap().get(&req.id) { f.store(true, Ordering::SeqCst); n.notify_one(); } }
            (_, "echo") => { let _ = tx.send(Envelope { kind: Kind::Response, ..req }); }
            (_, "stream") => {
                tokio::spawn(async move {
                    let n = req.body["n"].as_u64().unwrap_or(0);
                    for i in 0..n {
                        let _ = tx.send(Envelope { id: req.id, correlation: req.correlation, kind: Kind::Event, command: req.command.clone(), body: json!({ "token": format!("tok{i} ") }) });
                    }
                    let _ = tx.send(Envelope { id: req.id, correlation: req.correlation, kind: Kind::Done, command: req.command.clone(), body: json!({}) });
                });
            }
            (_, "work") => {
                let flag = Arc::new(AtomicBool::new(false));
                let note = Arc::new(tokio::sync::Notify::new());
                cancels.lock().unwrap().insert(req.id, (flag.clone(), note.clone()));
                tokio::spawn(async move {
                    // Wake on the cancel itself: a polling loop is only as fast as the timer
                    // (Windows' default sleep granularity is ~15.6 ms).
                    let mut step = 0u64;
                    while !flag.load(Ordering::SeqCst) {
                        step += 1;
                        let _ = tx.send(Envelope { id: req.id, correlation: req.correlation, kind: Kind::Event, command: "work".into(), body: json!({ "step": step }) });
                        tokio::select! { _ = tokio::time::sleep(Duration::from_millis(1)) => {}, _ = note.notified() => {} }
                    }
                    let _ = tx.send(Envelope { id: req.id, correlation: req.correlation, kind: Kind::Done, command: "work".into(), body: json!({ "cancelled": true, "steps": step }) });
                });
            }
            (_, "crash") => std::process::abort(),
            _ => {}
        }
    }
}

// ---------------------------------------------------------------- parent (bus side) ------
struct Link {
    tx: mpsc::UnboundedSender<Envelope>,
    pending: Arc<Mutex<HashMap<u64, mpsc::UnboundedSender<Envelope>>>>,
    closed: tokio::sync::watch::Receiver<Option<Instant>>,
    child: tokio::process::Child,
    _job: Job,
}

static NEXT: AtomicU64 = AtomicU64::new(1);

impl Link {
    async fn spawn(tag: &str) -> Link {
        let pipe = format!(r"\\.\pipe\ep-s7-{}-{}-{}", std::process::id(), tag, NEXT.fetch_add(1, Ordering::SeqCst));
        let server = ServerOptions::new().first_pipe_instance(true).create(&pipe).expect("pipe");
        let child = tokio::process::Command::new(std::env::current_exe().expect("exe")).args(["child", &pipe]).kill_on_drop(true).spawn().expect("spawn");
        let job = Job::new();
        job.assign(child.raw_handle().expect("handle"));
        server.connect().await.expect("connect");
        let (mut rd, mut wr) = tokio::io::split(server);
        let (tx, mut rx) = mpsc::unbounded_channel::<Envelope>();
        tokio::spawn(async move { while let Some(e) = rx.recv().await { if write_frame(&mut wr, &e).await.is_err() { break; } } });
        let pending: Arc<Mutex<HashMap<u64, mpsc::UnboundedSender<Envelope>>>> = Arc::default();
        let (closed_tx, closed) = tokio::sync::watch::channel(None);
        let p2 = pending.clone();
        tokio::spawn(async move {
            while let Ok(e) = read_frame(&mut rd).await {
                let sink = p2.lock().unwrap().get(&e.id).cloned();
                if let Some(s) = sink { let _ = s.send(e); }
            }
            let _ = closed_tx.send(Some(Instant::now())); // EOF or error: the child is gone
        });
        Link { tx, pending, closed, child, _job: job }
    }

    fn request(&self, command: &str, body: Value) -> (u64, mpsc::UnboundedReceiver<Envelope>) {
        let id = NEXT.fetch_add(1, Ordering::SeqCst);
        let (s, r) = mpsc::unbounded_channel();
        self.pending.lock().unwrap().insert(id, s);
        let _ = self.tx.send(Envelope { id, correlation: id, kind: Kind::Request, command: command.into(), body });
        (id, r)
    }

    async fn call(&self, command: &str, body: Value) -> Envelope {
        let (id, mut r) = self.request(command, body);
        let e = r.recv().await.expect("reply");
        self.pending.lock().unwrap().remove(&id);
        e
    }
}

// Windows Job Object with "kill on close": children die with the parent, however it exits.
struct Job(windows_sys::Win32::Foundation::HANDLE);
unsafe impl Send for Job {}
unsafe impl Sync for Job {}
impl Job {
    fn new() -> Job {
        use windows_sys::Win32::System::JobObjects::*;
        unsafe {
            let h = CreateJobObjectW(std::ptr::null(), std::ptr::null());
            let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
            info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            SetInformationJobObject(h, JobObjectExtendedLimitInformation, &info as *const _ as *const _, std::mem::size_of_val(&info) as u32);
            Job(h)
        }
    }
    fn assign(&self, process: std::os::windows::io::RawHandle) {
        unsafe { windows_sys::Win32::System::JobObjects::AssignProcessToJobObject(self.0, process as _); }
    }
}

fn alive(pid: u32) -> bool {
    use windows_sys::Win32::System::Threading::*;
    unsafe {
        let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if h.is_null() { return false; }
        let mut code = 0u32;
        GetExitCodeProcess(h, &mut code);
        windows_sys::Win32::Foundation::CloseHandle(h);
        code == 259 // STILL_ACTIVE
    }
}

fn pct(mut v: Vec<f64>, p: f64) -> f64 {
    v.sort_by(|a, b| a.total_cmp(b));
    (v[((v.len() as f64 - 1.0) * p) as usize] * 1000.0).round() / 1000.0
}

async fn measure() -> Value {
    let mut out = serde_json::Map::new();
    let t = Instant::now();
    let link = Link::spawn("main").await;
    let _ = link.call("echo", json!({})).await;
    out.insert("spawn_connect_first_reply_ms".into(), json!(t.elapsed().as_secs_f64() * 1000.0));

    // 1. round trip, small command (200-byte body), sequential
    let body = json!({ "paper": "x".repeat(180) });
    for _ in 0..200 { let _ = link.call("echo", body.clone()).await; }
    let mut ms = Vec::new();
    for _ in 0..10_000 { let t = Instant::now(); let _ = link.call("echo", body.clone()).await; ms.push(t.elapsed().as_secs_f64() * 1000.0); }
    out.insert("roundtrip_ms".into(), json!({ "p50": pct(ms.clone(), 0.5), "p99": pct(ms.clone(), 0.99), "max": pct(ms, 1.0), "n": 10000 }));

    // 2. many in flight: 64 concurrent requests, 20k total
    let t = Instant::now();
    let link = Arc::new(link);
    let mut tasks = Vec::new();
    for _ in 0..64 { let l = link.clone(); let b = body.clone(); tasks.push(tokio::spawn(async move { for _ in 0..(20_000 / 64) { let _ = l.call("echo", b.clone()).await; } })); }
    for t in tasks { t.await.expect("task"); }
    out.insert("concurrent_64_msgs_per_s".into(), json!((20_000.0 / 64.0 * 64.0 / t.elapsed().as_secs_f64()).round()));

    // 3. streaming: 1000 token events
    let t = Instant::now();
    let (_, mut r) = link.request("stream", json!({ "n": 1000 }));
    let mut first = None; let mut events = 0;
    while let Some(e) = r.recv().await { if e.kind == Kind::Event { events += 1; first.get_or_insert(t.elapsed()); } else { break; } }
    out.insert("stream_1000_events".into(), json!({ "first_event_ms": first.map(|d| d.as_secs_f64() * 1000.0), "total_ms": t.elapsed().as_secs_f64() * 1000.0, "events": events }));

    // 4. cancel a busy command
    let (id, mut r) = link.request("work", json!({}));
    let started = Instant::now();
    while started.elapsed() < Duration::from_millis(100) { let _ = r.recv().await; }
    let t = Instant::now();
    let _ = link.tx.send(Envelope { id, correlation: id, kind: Kind::Cancel, command: "work".into(), body: json!({}) });
    loop { let e = r.recv().await.expect("ack"); if e.kind == Kind::Done { break; } }
    out.insert("cancel_to_ack_ms".into(), json!(t.elapsed().as_secs_f64() * 1000.0));

    // 5. large messages (the plan sends big data as BlobRef; this shows why)
    for mb in [1usize, 8] {
        let big = json!({ "data": "x".repeat(mb * 1_000_000) });
        let t = Instant::now();
        let _ = link.call("echo", big).await;
        out.insert(format!("echo_{mb}mb_ms"), json!(t.elapsed().as_secs_f64() * 1000.0));
    }

    // 6. crash: child aborts; detect, restart, first good reply
    let mut link = Arc::try_unwrap(link).ok().expect("unique");
    let mut closed = link.closed.clone();
    let t = Instant::now();
    let _ = link.request("crash", json!({}));
    while closed.borrow().is_none() { closed.changed().await.expect("watch"); }
    let detect = t.elapsed();
    let _ = link.child.wait().await;
    let t2 = Instant::now();
    link = Link::spawn("restart").await;
    let ok = link.call("echo", json!({ "after": "restart" })).await.body["after"] == "restart";
    out.insert("crash".into(), json!({ "detect_ms": detect.as_secs_f64() * 1000.0, "restart_to_first_reply_ms": t2.elapsed().as_secs_f64() * 1000.0, "reply_correct": ok }));

    // 7. killed from outside (TerminateProcess): detect
    let mut closed = link.closed.clone();
    let t = Instant::now();
    link.child.start_kill().expect("kill");
    while closed.borrow().is_none() { closed.changed().await.expect("watch"); }
    out.insert("killed_detect_ms".into(), json!(t.elapsed().as_secs_f64() * 1000.0));

    // 8. parent dies abruptly: is the child left running? with and without the Job Object
    for mode in ["job", "nojob"] {
        // The helper reports the child's pid through a file, not a pipe: on Windows a child inherits
        // *every* inheritable handle of its parent, so a captured stdout pipe would leak into the hung
        // grandchild and keep us waiting forever (found the hard way in this spike).
        let pid_file = std::env::temp_dir().join(format!("ep-s7-orphan-{mode}.pid"));
        let _ = std::fs::remove_file(&pid_file);
        let _ = std::process::Command::new(std::env::current_exe().expect("exe")).args(["orphan", mode, &pid_file.to_string_lossy()])
            .stdin(std::process::Stdio::null()).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null())
            .status().expect("orphan");
        let pid: u32 = std::fs::read_to_string(&pid_file).unwrap_or_default().trim().parse().unwrap_or(0);
        tokio::time::sleep(Duration::from_millis(500)).await;
        let left = alive(pid);
        out.insert(format!("child_alive_after_parent_exit_{mode}"), json!(left));
        if left { let _ = std::process::Command::new("taskkill").args(["/PID", &pid.to_string(), "/F"]).output(); }
    }
    Value::Object(out)
}

async fn orphan(mode: &str, pid_file: &str) {
    let pipe = format!(r"\\.\pipe\ep-s7-orphan-{}", std::process::id());
    let server = ServerOptions::new().first_pipe_instance(true).create(&pipe).expect("pipe");
    let child = std::process::Command::new(std::env::current_exe().expect("exe")).args(["hang", &pipe])
        // no inherited stdio: an inherited stdout would keep the measuring parent waiting on a hung child
        .stdin(std::process::Stdio::null()).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null())
        .spawn().expect("spawn");
    let job = Job::new();
    if mode == "job" { job.assign(std::os::windows::io::AsRawHandle::as_raw_handle(&child)); }
    server.connect().await.expect("connect");
    std::fs::write(pid_file, child.id().to_string()).expect("pid file");
    std::process::exit(0); // abrupt: no cleanup, no drop — the OS closes our handles
}

#[tokio::main]
async fn main() {
    let a: Vec<String> = std::env::args().collect();
    match a.get(1).map(String::as_str) {
        Some("child") => child(a[2].clone()).await,
        Some("hang") => { let _ = ClientOptions::new().open(&a[2]); loop { tokio::time::sleep(Duration::from_secs(3600)).await; } }
        Some("orphan") => orphan(&a[2], &a[3]).await,
        _ => println!("{}", serde_json::to_string_pretty(&measure().await).expect("json")),
    }
}
