//! Bus behaviour: middleware order, policy, provenance, events, overhead.

use std::sync::Arc;
use std::time::{Duration, Instant};

use ep_bus::testing::{register_fixture_commands, run_conformance};
use ep_bus::{
    Bus, CommandError, Envelope, FakeClock, Permission, Recv, RegisterError, SeqIds, Spec,
    Stability,
};
use ep_events::EventLog;
use ep_policy::{DataClass, Destination, Operation};
use parking_lot::Mutex;
use serde_json::json;

fn bus() -> Bus {
    Bus::new(Arc::new(FakeClock::at(1_000)), Arc::new(SeqIds::default()))
}

fn spec(name: &'static str, permission: Permission) -> Spec {
    Spec {
        name,
        summary: "test",
        stability: Stability::Stable,
        permission,
    }
}

#[tokio::test]
async fn in_process_transport_passes_conformance() {
    let b = bus();
    register_fixture_commands(&b).expect("register");
    run_conformance(&b).await.expect("conformance");
}

#[tokio::test]
async fn policy_runs_before_handler_and_provenance_after() {
    let d = tempfile::tempdir().expect("tmp");
    let log = Arc::new(EventLog::open(d.path()).expect("log"));
    let b = bus();
    b.set_provenance_log(Some(log.clone()));
    let ran = Arc::new(Mutex::new(Vec::<&str>::new()));
    let r1 = ran.clone();
    let external = Permission {
        operation: Operation::ExternalApi,
        data: [DataClass::UnpublishedIdea].into(),
        destination: Destination::External,
    };
    b.register(spec("t.send", external), move |_cx, (): ()| {
        let r1 = r1.clone();
        async move {
            r1.lock().push("handler");
            Ok::<_, CommandError>(())
        }
    })
    .expect("register");
    let local_only = Permission {
        operation: Operation::ExternalApi,
        data: [DataClass::LocalOnly].into(),
        destination: Destination::External,
    };
    b.register(spec("t.leak", local_only), |_cx, (): ()| async {
        Ok::<_, CommandError>(())
    })
    .expect("register");

    // Needs approval: handler never runs, nothing recorded.
    let r = b.call(Envelope::new("t.send", json!(null))).await;
    let Err(CommandError::NeedsApproval { rule, .. }) = r else {
        panic!("expected approval, got {r:?}")
    };
    assert!(ran.lock().is_empty());
    assert_eq!(log.last_seq(), 0);
    // Denied outright.
    assert!(matches!(
        b.call(Envelope::new("t.leak", json!(null))).await,
        Err(CommandError::Denied { .. })
    ));
    // Approved by the user: runs, then provenance is written.
    let mut env = Envelope::new("t.send", json!(null));
    env.approved.insert(rule);
    b.call(env).await.expect("approved call");
    assert_eq!(*ran.lock(), ["handler"]);
    let mut kinds = vec![];
    log.replay(1, |r| {
        kinds.push((
            r.event.kind.clone(),
            r.event.data["command"].clone(),
            r.event.at_ms,
        ))
    })
    .expect("replay");
    assert_eq!(
        kinds,
        [("command.completed".to_string(), json!("t.send"), 1_000)]
    );
}

#[tokio::test]
async fn reads_are_not_recorded_and_finished_events_are_published() {
    let d = tempfile::tempdir().expect("tmp");
    let log = Arc::new(EventLog::open(d.path()).expect("log"));
    let b = bus();
    b.set_provenance_log(Some(log.clone()));
    register_fixture_commands(&b).expect("register");
    let mut sub = b.events().subscribe("command.");
    b.call(Envelope::new("test.echo", json!({ "text": "x", "n": 1 })))
        .await
        .expect("call");
    assert_eq!(log.last_seq(), 0);
    let Recv::Event(e) = sub.recv().await else {
        panic!("no event")
    };
    assert_eq!(e.data["command"], "test.echo");
    assert_eq!(e.data["outcome"], "ok");
}

#[tokio::test]
async fn slow_subscriber_is_told_what_it_missed() {
    let b = bus();
    let mut sub = b.events().subscribe("");
    for i in 0..1100 {
        b.events().publish("x.tick", json!(i), 0); // never blocks
    }
    assert_eq!(sub.recv().await, Recv::Missed(76));
    let Recv::Event(e) = sub.recv().await else {
        panic!("event")
    };
    assert_eq!(e.data, json!(76));
}

#[test]
fn registration_rules() {
    let b = bus();
    let ok = |_cx, (): ()| async { Ok::<_, CommandError>(()) };
    assert_eq!(
        b.register(spec("Bad.Name", Permission::read()), ok),
        Err(RegisterError::BadName("Bad.Name".into()))
    );
    assert_eq!(
        b.register(spec("noverb", Permission::read()), ok),
        Err(RegisterError::BadName("noverb".into()))
    );
    b.register(spec("workspace.add_file", Permission::create()), ok)
        .expect("ok");
    assert_eq!(
        b.register(spec("workspace.add_file", Permission::read()), ok),
        Err(RegisterError::Duplicate("workspace.add_file".into()))
    );
    let info = &b.commands()[0];
    assert_eq!(info.input_schema["type"], "null");
}

#[tokio::test]
async fn overhead_is_well_under_a_millisecond() {
    let b = bus();
    register_fixture_commands(&b).expect("register");
    let n = 2000u32;
    let t0 = Instant::now();
    for _ in 0..n {
        b.call(Envelope::new("test.echo", json!({ "text": "a", "n": 1 })))
            .await
            .expect("call");
    }
    let per = t0.elapsed() / n;
    eprintln!("bus overhead: {per:?} per call");
    assert!(per < Duration::from_millis(1), "{per:?}");
}

#[tokio::test]
async fn stream_transport_passes_the_same_conformance() {
    let b = Arc::new(bus());
    register_fixture_commands(&b).expect("register");
    let (client_end, server_end) = tokio::io::duplex(1 << 16);
    let (srd, swr) = tokio::io::split(server_end);
    tokio::spawn(ep_bus::ipc::serve(b, srd, swr));
    let (crd, cwr) = tokio::io::split(client_end);
    let client = ep_bus::ipc::IpcClient::new(crd, cwr);
    run_conformance(&client)
        .await
        .expect("conformance over a stream");
}

#[tokio::test]
async fn closed_connection_fails_waiting_calls() {
    let (client_end, server_end) = tokio::io::duplex(1 << 16);
    let (crd, cwr) = tokio::io::split(client_end);
    let client = ep_bus::ipc::IpcClient::new(crd, cwr);
    let call = client_call(&client);
    drop(server_end);
    assert!(matches!(call.await, Err(CommandError::Transport { .. })));
}

async fn client_call(c: &ep_bus::ipc::IpcClient) -> Result<serde_json::Value, CommandError> {
    ep_bus::Transport::call(
        c,
        Envelope::new("test.echo", json!({ "text": "a", "n": 1 })),
    )
    .await
}

#[tokio::test]
async fn dropping_the_client_closes_the_connection() {
    let b = Arc::new(bus());
    let (client_end, server_end) = tokio::io::duplex(1 << 16);
    let (srd, swr) = tokio::io::split(server_end);
    let server = tokio::spawn(ep_bus::ipc::serve(b, srd, swr));
    let (crd, cwr) = tokio::io::split(client_end);
    drop(ep_bus::ipc::IpcClient::new(crd, cwr));
    let r = tokio::time::timeout(Duration::from_secs(2), server).await;
    assert!(
        matches!(r, Ok(Ok(Ok(())))),
        "server did not see the close: {r:?}"
    );
}
