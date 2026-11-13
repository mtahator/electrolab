//! Integration tests of the Windows adapters on a real Windows system.
#![cfg(windows)]

use std::sync::Arc;
use std::time::{Duration, Instant};

use ep_os::{AppDirs, KeyStore, LocalIpc, ProcessSpawner, ProcessSpec, ShellOpen};
use ep_os_windows::{CredentialStore, JobSpawner, NamedPipes, WinDirs, WinShell};

fn alive(pid: u32) -> bool {
    let out = std::process::Command::new("tasklist")
        .args(["/FI", &format!("PID eq {pid}"), "/NH", "/FO", "CSV"])
        .output()
        .expect("tasklist");
    String::from_utf8_lossy(&out.stdout).contains(&format!("\"{pid}\""))
}

#[test]
fn credential_store_round_trip() {
    let ks = CredentialStore::new("ElectroPaper-test");
    let name = format!("unit-{}", std::process::id());
    assert_eq!(ks.get(&name).expect("get"), None);
    ks.set(&name, "sk-test-كلید").expect("set");
    assert_eq!(ks.get(&name).expect("get").as_deref(), Some("sk-test-كلید"));
    ks.delete(&name).expect("delete");
    ks.delete(&name).expect("delete twice is fine");
    assert_eq!(ks.get(&name).expect("get"), None);
}

#[test]
fn job_object_kills_children_when_the_spawner_goes() {
    let spawner = JobSpawner::new().expect("job");
    let spec = ProcessSpec {
        program: "ping.exe".into(),
        args: vec!["-n".into(), "60".into(), "127.0.0.1".into()],
        ..ProcessSpec::default()
    };
    let mut child = spawner.spawn(&spec).expect("spawn");
    let pid = child.id();
    assert!(alive(pid));
    assert_eq!(child.try_wait().expect("wait"), None);
    drop(spawner); // closing the job handle ends the child, as an app crash would
    let t = Instant::now();
    while alive(pid) && t.elapsed() < Duration::from_secs(5) {
        std::thread::sleep(Duration::from_millis(50));
    }
    assert!(!alive(pid), "child {pid} outlived the job");
}

#[test]
fn child_output_goes_to_the_log_file() {
    let d = tempfile::tempdir().expect("tmp");
    let log = d.path().join("child.log");
    let spawner = JobSpawner::new().expect("job");
    let spec = ProcessSpec {
        program: "cmd.exe".into(),
        args: vec!["/c".into(), "echo electropaper".into()],
        log_file: Some(log.clone()),
        ..ProcessSpec::default()
    };
    let mut child = spawner.spawn(&spec).expect("spawn");
    let t = Instant::now();
    while child.try_wait().expect("wait").is_none() && t.elapsed() < Duration::from_secs(5) {
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(
        std::fs::read_to_string(&log)
            .expect("log")
            .contains("electropaper")
    );
}

#[test]
fn dirs_are_per_user_and_shell_refuses_non_web_urls() {
    let d = WinDirs::new("ElectroPaper-test").expect("dirs");
    assert!(d.data().is_absolute() && d.logs().starts_with(d.cache()));
    assert!(WinShell.open_url("file:///C:/Windows/notepad.exe").is_err());
    assert!(
        WinShell
            .open_path(std::path::Path::new(r"C:\definitely\missing.pdf"))
            .is_err()
    );
}

#[tokio::test]
async fn bus_over_a_named_pipe_passes_conformance() {
    let bus = Arc::new(ep_bus::Bus::default());
    ep_bus::testing::register_fixture_commands(&bus).expect("register");
    let name = format!("electropaper-test-{}-{}", std::process::id(), uuid_like());
    let server = {
        let name = name.clone();
        tokio::spawn(async move {
            let s = NamedPipes.accept(&name).await.expect("accept");
            let (rd, wr) = tokio::io::split(s);
            ep_bus::ipc::serve(bus, rd, wr).await
        })
    };
    let c = NamedPipes.connect(&name, 2000).await.expect("connect");
    let (rd, wr) = tokio::io::split(c);
    let client = ep_bus::ipc::IpcClient::new(rd, wr);
    ep_bus::testing::run_conformance(&client)
        .await
        .expect("conformance over a named pipe");
    // A second listener with the same name is refused while the first exists.
    assert!(NamedPipes.accept(&name).await.is_err());
    drop(client);
    let _ = server.await;
}

fn uuid_like() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos())
}
