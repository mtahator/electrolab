//! Workspace conformance suite (P1.M1.WSP.1-2): the contract's promises, through the bus.
#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)] // tests and their helpers stop loudly

use std::path::{Path, PathBuf};
use std::sync::Arc;

use ep_bus::{Bus, CommandError, Envelope, FakeClock, SeqIds, Transport};
use ep_os::fake::FakeDirs;
use ep_types::Hash;
use ep_workspace::Workspace;
use ep_workspace_contract::{
    Added, Classification, FileKind, Paper, ProjectInfo, commands as cmd, errors,
};
use serde_json::{Value, json};

fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../fixtures/papers/fawiki-artificial-neural-network.pdf")
}

fn setup(root: &Path) -> (Arc<Bus>, Arc<Workspace>) {
    let bus = Arc::new(Bus::new(
        Arc::new(FakeClock::at(1_700_000_000_000)),
        Arc::new(SeqIds::default()),
    ));
    let ws = Workspace::new(Arc::new(FakeDirs(root.join("app"))));
    ws.register(&bus).expect("register");
    (bus, ws)
}

async fn call<T: serde::de::DeserializeOwned>(
    bus: &Bus,
    name: &str,
    input: Value,
) -> Result<T, CommandError> {
    let v = Transport::call(bus, Envelope::new(name, input)).await?;
    Ok(serde_json::from_value(v).expect("output matches contract"))
}

fn kind(e: &CommandError) -> &str {
    match e {
        CommandError::Failed { kind, .. } => kind,
        _ => "other",
    }
}

#[tokio::test]
async fn create_add_close_reopen_is_identical() {
    let d = tempfile::tempdir().expect("tmp");
    let folder = d.path().join("کتابخانه آزمایشی");
    let original = d.path().join("my paper.pdf");
    std::fs::copy(fixture(), &original).expect("copy fixture");
    let before = Hash::of(&std::fs::read(&original).expect("read"));

    let (bus, ws) = setup(d.path());
    let info: ProjectInfo = call(
        &bus,
        cmd::CREATE_PROJECT,
        json!({ "folder": folder, "name": "Test library", "kind": "library" }),
    )
    .await
    .expect("create");
    assert_eq!((info.papers, info.format), (0, 1));
    let added: Added = call(&bus, cmd::ADD_FILE, json!({ "path": original }))
        .await
        .expect("add");
    assert!(!added.already_present);
    assert_eq!(added.paper.original_name, "my paper.pdf");
    assert_eq!(added.paper.file.hash, before);
    assert!(ws.blob_path(&before).is_some());
    // The original is untouched.
    assert_eq!(Hash::of(&std::fs::read(&original).expect("read")), before);
    // Same content again: nothing new.
    let again: Added = call(&bus, cmd::ADD_FILE, json!({ "path": original }))
        .await
        .expect("add again");
    assert!(again.already_present);
    assert_eq!(again.paper.id, added.paper.id);
    let listed: Vec<Paper> = call(&bus, cmd::LIST_PAPERS, json!({})).await.expect("list");
    drop((bus, ws));

    // Reopen from scratch: same project, same papers.
    let (bus, _ws) = setup(d.path());
    let reopened: ProjectInfo = call(&bus, cmd::OPEN_PROJECT, json!({ "folder": folder }))
        .await
        .expect("open");
    assert_eq!(reopened, ProjectInfo { papers: 1, ..info });
    let relisted: Vec<Paper> = call(&bus, cmd::LIST_PAPERS, json!({})).await.expect("list");
    assert_eq!(relisted, listed);
}

#[tokio::test]
async fn metadata_updates_and_events() {
    let d = tempfile::tempdir().expect("tmp");
    let (bus, ws) = setup(d.path());
    let mut sub = bus.events().subscribe("workspace.paper");
    let _: ProjectInfo = call(&bus, cmd::OPEN_DEFAULT_LIBRARY, json!(null))
        .await
        .expect("default library");
    assert!(
        d.path()
            .join("app/data/Library/electropaper-project.toml")
            .exists()
    );
    let added: Added = call(&bus, cmd::ADD_FILE, json!({ "path": fixture() }))
        .await
        .expect("add");
    let id = added.paper.id;
    let p: Paper = call(&bus, cmd::SET_PAPER_METADATA, json!({ "id": id, "title": "Neural network", "authors": ["A. Writer"], "doi": " 10.1000/XYZ ", "pages": 12, "source": "pdf-info" })).await.expect("set");
    assert_eq!(
        (p.title.as_deref(), p.doi.as_deref(), p.pages),
        (Some("Neural network"), Some("10.1000/xyz"), Some(12))
    );
    // Partial update keeps other fields.
    let p: Paper = call(
        &bus,
        cmd::SET_PAPER_METADATA,
        json!({ "id": id, "pages": 13, "source": "user" }),
    )
    .await
    .expect("set");
    assert_eq!(
        (p.title.as_deref(), p.authors.len(), p.pages),
        (Some("Neural network"), 1, Some(13))
    );
    let got: Paper = call(&bus, cmd::GET_PAPER, json!({ "id": id }))
        .await
        .expect("get");
    assert_eq!(got, p);
    let mut kinds = vec![];
    for _ in 0..3 {
        kinds.push(match sub.recv().await {
            ep_bus::Recv::Event(e) => e.kind,
            other => format!("{other:?}"),
        });
    }
    assert_eq!(
        kinds,
        [
            "workspace.paper_added",
            "workspace.paper_changed",
            "workspace.paper_changed"
        ]
    );
    // Provenance of every non-read command went into the library's log.
    let mut seen = vec![];
    ws.opened()
        .expect("open")
        .events
        .replay(1, |r| seen.push(r.event.kind.clone()))
        .expect("replay");
    assert!(
        seen.contains(&"command.completed".to_string())
            && seen.contains(&"workspace.paper_added".to_string())
    );
}

#[tokio::test]
async fn errors_follow_the_contract() {
    let d = tempfile::tempdir().expect("tmp");
    let (bus, _ws) = setup(d.path());
    let e = call::<Added>(&bus, cmd::ADD_FILE, json!({ "path": fixture() }))
        .await
        .expect_err("nothing open");
    assert_eq!(kind(&e), errors::NO_PROJECT);
    let none: Option<ProjectInfo> = call(&bus, cmd::CURRENT, json!(null))
        .await
        .expect("current");
    assert_eq!(none, None);

    let folder = d.path().join("lib");
    let _: ProjectInfo = call(
        &bus,
        cmd::CREATE_PROJECT,
        json!({ "folder": folder, "name": "L", "kind": "library" }),
    )
    .await
    .expect("create");
    let e = call::<ProjectInfo>(
        &bus,
        cmd::CREATE_PROJECT,
        json!({ "folder": folder, "name": "L", "kind": "library" }),
    )
    .await
    .expect_err("in use");
    assert_eq!(kind(&e), errors::FOLDER_IN_USE);
    let e = call::<ProjectInfo>(
        &bus,
        cmd::CREATE_PROJECT,
        json!({ "folder": d.path().join("x"), "name": "  ", "kind": "project" }),
    )
    .await
    .expect_err("bad name");
    assert!(matches!(e, CommandError::InvalidInput { .. }));

    let text = d.path().join("notes.txt");
    std::fs::write(&text, "not a pdf").expect("write");
    let c: Classification = call(&bus, cmd::CLASSIFY, json!({ "path": text }))
        .await
        .expect("classify");
    assert_eq!(c.kind, FileKind::Unsupported);
    let e = call::<Added>(&bus, cmd::ADD_FILE, json!({ "path": text }))
        .await
        .expect_err("unsupported");
    assert_eq!(kind(&e), errors::UNSUPPORTED_FILE);
    let e = call::<Paper>(
        &bus,
        cmd::GET_PAPER,
        json!({ "id": "00000000-0000-0000-0000-000000000099" }),
    )
    .await
    .expect_err("missing");
    assert_eq!(kind(&e), errors::NOT_FOUND);

    // A folder from a newer app is refused, not damaged.
    let manifest = folder.join("electropaper-project.toml");
    let text = std::fs::read_to_string(&manifest)
        .expect("read")
        .replace("format = 1", "format = 2");
    std::fs::write(&manifest, &text).expect("write");
    let e = call::<ProjectInfo>(&bus, cmd::OPEN_PROJECT, json!({ "folder": folder }))
        .await
        .expect_err("newer");
    assert_eq!(kind(&e), errors::BAD_FORMAT);
    assert_eq!(std::fs::read_to_string(&manifest).expect("read"), text);
}
