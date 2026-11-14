//! Library features (P1.M2.WSP.1–3) through the bus: collections, tags, search, reading state,
//! bookmarks and reading statistics.
#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    clippy::many_single_char_names,
    clippy::too_many_lines,
    clippy::used_underscore_binding
)] // scenario tests read top to bottom

use std::path::Path;
use std::sync::Arc;

use ep_bus::{Bus, CommandError, Envelope, FakeClock, SeqIds};
use ep_os::fake::FakeDirs;
use ep_workspace::Workspace;
use ep_workspace_contract::{
    Added, Bookmark, Collection, Paper, ProjectInfo, ReadingState, ReadingStats, TagCount, ViewMode,
};
use serde_json::{Value, json};

struct Env {
    bus: Arc<Bus>,
    clock: Arc<FakeClock>,
    _ws: Arc<Workspace>,
}

fn env(root: &Path) -> Env {
    let clock = Arc::new(FakeClock::at(1_000_000));
    let bus = Arc::new(Bus::new(clock.clone(), Arc::new(SeqIds::default())));
    let ws = Workspace::new(Arc::new(FakeDirs(root.join("app"))));
    ws.register(&bus).unwrap();
    Env {
        bus,
        clock,
        _ws: ws,
    }
}

async fn call<T: serde::de::DeserializeOwned>(
    bus: &Bus,
    name: &str,
    input: Value,
) -> Result<T, CommandError> {
    Ok(serde_json::from_value(bus.call(Envelope::new(name, input)).await?).expect("contract"))
}

async fn add(e: &Env, dir: &Path, name: &str, title: &str, authors: &[&str]) -> Paper {
    let f = dir.join(name);
    std::fs::write(&f, format!("%PDF-1.4 {name}")).unwrap();
    let a: Added = call(&e.bus, "workspace.add_file", json!({ "path": f }))
        .await
        .unwrap();
    call(
        &e.bus,
        "workspace.set_paper_metadata",
        json!({ "id": a.paper.id, "title": title, "authors": authors, "source": "test" }),
    )
    .await
    .unwrap()
}

#[tokio::test]
async fn collections_tags_and_search() {
    let d = tempfile::tempdir().unwrap();
    let e = env(d.path());
    let _: ProjectInfo = call(&e.bus, "workspace.open_default_library", json!(null))
        .await
        .unwrap();
    let a = add(
        &e,
        d.path(),
        "a.pdf",
        "Physics-informed neural networks",
        &["Ada Lovelace"],
    )
    .await;
    let b = add(
        &e,
        d.path(),
        "b.pdf",
        "Large-eddy simulation",
        &["Alan Turing"],
    )
    .await;

    let c: Collection = call(
        &e.bus,
        "workspace.create_collection",
        json!({ "name": "Thesis" }),
    )
    .await
    .unwrap();
    let dup = call::<Collection>(
        &e.bus,
        "workspace.create_collection",
        json!({ "name": "thesis" }),
    )
    .await
    .unwrap_err();
    assert!(matches!(dup, CommandError::Failed { ref kind, .. } if kind == "exists"));
    let c: Collection = call(
        &e.bus,
        "workspace.add_to_collection",
        json!({ "collection": c.id, "papers": [a.id, a.id] }),
    )
    .await
    .unwrap();
    assert_eq!(c.papers, 1);
    let inside: Vec<Paper> = call(
        &e.bus,
        "workspace.list_papers",
        json!({ "collection": c.id }),
    )
    .await
    .unwrap();
    assert_eq!(inside.iter().map(|p| p.id).collect::<Vec<_>>(), [a.id]);
    let c: Collection = call(
        &e.bus,
        "workspace.rename_collection",
        json!({ "id": c.id, "name": "PhD thesis" }),
    )
    .await
    .unwrap();
    assert_eq!(c.name, "PhD thesis");

    let p: Paper = call(
        &e.bus,
        "workspace.set_tags",
        json!({ "paper": b.id, "tags": [" CFD ", "cfd", "turbulence"] }),
    )
    .await
    .unwrap();
    assert_eq!(p.tags, ["CFD", "turbulence"]);
    let tags: Vec<TagCount> = call(&e.bus, "workspace.list_tags", json!(null))
        .await
        .unwrap();
    assert_eq!(tags.len(), 2);
    let tagged: Vec<Paper> = call(&e.bus, "workspace.list_papers", json!({ "tag": "cfd" }))
        .await
        .unwrap();
    assert_eq!(tagged.iter().map(|p| p.id).collect::<Vec<_>>(), [b.id]);

    let found: Vec<Paper> = call(
        &e.bus,
        "workspace.list_papers",
        json!({ "query": "NEURAL lovelace" }),
    )
    .await
    .unwrap();
    assert_eq!(found.iter().map(|p| p.id).collect::<Vec<_>>(), [a.id]);
    let none: Vec<Paper> = call(&e.bus, "workspace.list_papers", json!({ "query": "100%" }))
        .await
        .unwrap();
    assert_eq!(none, Vec::<Paper>::new(), "% must be matched literally");
    let by_title: Vec<Paper> = call(&e.bus, "workspace.list_papers", json!({ "sort": "title" }))
        .await
        .unwrap();
    assert_eq!(
        by_title.iter().map(|p| p.id).collect::<Vec<_>>(),
        [b.id, a.id]
    );

    // Deleting a collection is destructive: the policy asks first; the papers stay.
    let ask = call::<()>(&e.bus, "workspace.delete_collection", json!({ "id": c.id }))
        .await
        .unwrap_err();
    let CommandError::NeedsApproval { rule, .. } = ask else {
        panic!("expected approval, got {ask:?}")
    };
    let mut env2 = Envelope::new("workspace.delete_collection", json!({ "id": c.id }));
    env2.approved.insert(rule);
    e.bus.call(env2).await.unwrap();
    let cols: Vec<Collection> = call(&e.bus, "workspace.list_collections", json!(null))
        .await
        .unwrap();
    assert_eq!(cols, Vec::<Collection>::new());
    let all: Vec<Paper> = call(&e.bus, "workspace.list_papers", json!({}))
        .await
        .unwrap();
    assert_eq!(all.len(), 2);
}

#[tokio::test]
async fn reading_state_survives_reopening_and_stats_count_time() {
    let d = tempfile::tempdir().unwrap();
    let folder = d.path().join("lib");
    let paper;
    {
        let e = env(d.path());
        let _: ProjectInfo = call(
            &e.bus,
            "workspace.create_project",
            json!({ "folder": folder, "name": "L", "kind": "library" }),
        )
        .await
        .unwrap();
        paper = add(&e, d.path(), "a.pdf", "A paper", &[]).await;
        let other = add(&e, d.path(), "b.pdf", "B paper", &[]).await;
        let loc = json!({ "paper": paper.id, "page": 7, "target": { "page": 7, "x0": 0.0, "y0": 120.5, "x1": 10.0, "y1": 130.0 } });
        call::<()>(&e.bus, "workspace.save_reading_state", json!({ "paper": paper.id, "location": loc, "zoom": { "kind": "scale", "factor": 1.5 }, "view_mode": "two_page" }))
            .await
            .unwrap();
        let bm: Bookmark = call(
            &e.bus,
            "workspace.add_bookmark",
            json!({ "name": "  Results  ", "location": { "paper": paper.id, "page": 9 } }),
        )
        .await
        .unwrap();
        assert_eq!(bm.name, "Results");
        let unnamed: Bookmark = call(
            &e.bus,
            "workspace.add_bookmark",
            json!({ "name": "", "location": { "paper": paper.id, "page": 3 } }),
        )
        .await
        .unwrap();
        assert_eq!(unnamed.name, "Page 3");
        call::<()>(
            &e.bus,
            "workspace.remove_bookmark",
            json!({ "id": unnamed.id }),
        )
        .await
        .unwrap();

        // Recently opened comes first.
        e.clock.advance(1_000);
        call::<()>(
            &e.bus,
            "workspace.save_reading_state",
            json!({ "paper": other.id }),
        )
        .await
        .unwrap();
        let recent: Vec<Paper> = call(&e.bus, "workspace.list_papers", json!({ "sort": "opened" }))
            .await
            .unwrap();
        assert_eq!(
            recent.iter().map(|p| p.id).collect::<Vec<_>>(),
            [other.id, paper.id]
        );

        // Reading time: reports are capped; pages are counted once.
        let _: ReadingStats = call(
            &e.bus,
            "workspace.record_reading",
            json!({ "paper": paper.id, "page": 1, "seconds": 30 }),
        )
        .await
        .unwrap();
        let _: ReadingStats = call(
            &e.bus,
            "workspace.record_reading",
            json!({ "paper": paper.id, "page": 1, "seconds": 10 }),
        )
        .await
        .unwrap();
        e.clock.advance(5_000);
        let s: ReadingStats = call(
            &e.bus,
            "workspace.record_reading",
            json!({ "paper": paper.id, "page": 2, "seconds": 100_000 }),
        )
        .await
        .unwrap();
        assert_eq!(
            (s.seconds, s.pages_read, s.last_ms),
            (340, 2, Some(1_006_000))
        );
    }
    // A new process opens the same library: the place, zoom, layout and bookmarks are back.
    let e = env(d.path());
    let _: ProjectInfo = call(
        &e.bus,
        "workspace.open_project",
        json!({ "folder": folder }),
    )
    .await
    .unwrap();
    let st: ReadingState = call(
        &e.bus,
        "workspace.get_reading_state",
        json!({ "paper": paper.id }),
    )
    .await
    .unwrap();
    assert_eq!(
        st.location.map(|l| (l.page, l.target.map(|t| t.y0))),
        Some((7, Some(120.5)))
    );
    assert_eq!(st.view_mode, ViewMode::TwoPage);
    assert_eq!(
        st.bookmarks
            .iter()
            .map(|b| b.name.as_str())
            .collect::<Vec<_>>(),
        ["Results"]
    );
    assert_eq!(st.opened_ms, Some(1_000_000));
    let s: ReadingStats = call(
        &e.bus,
        "workspace.reading_stats",
        json!({ "paper": paper.id }),
    )
    .await
    .unwrap();
    assert_eq!(s.seconds, 340);
}

#[tokio::test]
async fn downloaded_files_keep_their_address() {
    let d = tempfile::tempdir().unwrap();
    let e = env(d.path());
    let _: ProjectInfo = call(&e.bus, "workspace.open_default_library", json!(null))
        .await
        .unwrap();
    let tmp = d.path().join("1794000000.pdf");
    std::fs::write(&tmp, "%PDF-1.4 downloaded").unwrap();
    let a: Added = call(
        &e.bus,
        "workspace.add_file",
        json!({ "path": tmp, "source_url": "https://arxiv.org/pdf/2111.12066v2?download=1" }),
    )
    .await
    .unwrap();
    assert_eq!(
        a.paper.source_url.as_deref(),
        Some("https://arxiv.org/pdf/2111.12066v2?download=1")
    );
    assert_eq!(a.paper.original_name, "2111.12066v2.pdf");
    assert_eq!(a.paper.original_path, None);
    // Unsafe addresses are not stored.
    let tmp2 = d.path().join("x.pdf");
    std::fs::write(&tmp2, "%PDF-1.4 other").unwrap();
    let b: Added = call(
        &e.bus,
        "workspace.add_file",
        json!({ "path": tmp2, "source_url": "javascript:alert(1)" }),
    )
    .await
    .unwrap();
    assert_eq!(b.paper.source_url, None);
}

#[tokio::test]
async fn a_saved_version_replaces_the_managed_copy() {
    let d = tempfile::tempdir().unwrap();
    let e = env(d.path());
    let _: ProjectInfo = call(&e.bus, "workspace.open_default_library", json!(null))
        .await
        .unwrap();
    let p = add(&e, d.path(), "v1.pdf", "A paper", &[]).await;
    // A new version stored in the library (as `document.annotate` does).
    let v2 = d.path().join("v2.pdf");
    std::fs::write(&v2, "%PDF-1.4 v1.pdf\n% annotations appended").unwrap();
    let ws = &e._ws;
    let file = ws
        .opened()
        .unwrap()
        .blobs
        .put_file(&v2, ep_types::MediaType::pdf())
        .unwrap();
    let changed: Paper = call(
        &e.bus,
        "workspace.set_paper_file",
        json!({ "id": p.id, "file": file }),
    )
    .await
    .unwrap_or_else(|e| panic!("{e:?}"));
    assert_eq!(changed.file.hash, file.hash);
    assert_ne!(changed.file.hash, p.file.hash);
    assert_eq!(changed.title.as_deref(), Some("A paper"));
    let missing = call::<Paper>(
        &e.bus,
        "workspace.set_paper_file",
        json!({ "id": p.id, "file": { "hash": "0".repeat(64), "size": 1, "media_type": "application/pdf" } }),
    )
    .await;
    assert!(matches!(missing, Err(CommandError::Failed { ref kind, .. }) if kind == "not_found"));
}

#[tokio::test]
async fn paper_notes_save_history_restore_and_restart() {
    use ep_workspace_contract::{NotesVersion, PaperNotes};
    let d = tempfile::tempdir().unwrap();
    let e = env(d.path());
    let _: ProjectInfo = call(&e.bus, "workspace.open_default_library", json!(null))
        .await
        .unwrap();
    let p = add(&e, d.path(), "n.pdf", "Notes paper", &[]).await;

    let empty: PaperNotes = call(&e.bus, "workspace.get_notes", json!({ "paper": p.id }))
        .await
        .unwrap();
    assert_eq!((empty.version, empty.text.as_str()), (0, ""));

    // Typing: saves a second apart are one history entry; a minute later starts a new one.
    let mut v = 0;
    for (i, text) in [
        "# Notes",
        "# Notes\n\nFirst idea",
        "# Notes\n\nFirst idea, refined",
    ]
    .iter()
    .enumerate()
    {
        if i > 0 {
            e.clock.advance(1_000);
        }
        let n: PaperNotes = call(
            &e.bus,
            "workspace.save_notes",
            json!({ "paper": p.id, "text": text, "base_version": v }),
        )
        .await
        .unwrap();
        v = n.version;
    }
    assert_eq!(v, 3);
    e.clock.advance(120_000);
    let n: PaperNotes = call(
        &e.bus,
        "workspace.save_notes",
        json!({ "paper": p.id, "text": "Rewritten", "base_version": v }),
    )
    .await
    .unwrap();
    assert_eq!(n.version, 4);
    let hist: Vec<NotesVersion> = call(&e.bus, "workspace.notes_history", json!({ "paper": p.id }))
        .await
        .unwrap();
    assert_eq!(
        hist.iter().map(|h| h.version).collect::<Vec<_>>(),
        vec![4, 3, 1],
        "{hist:?}"
    );

    // A save from an old version is refused and changes nothing.
    let stale = call::<PaperNotes>(
        &e.bus,
        "workspace.save_notes",
        json!({ "paper": p.id, "text": "lost?", "base_version": 2 }),
    )
    .await;
    assert!(
        matches!(stale, Err(CommandError::Failed { ref kind, .. }) if kind == "conflict"),
        "{stale:?}"
    );
    // Saving the same text is not a new version.
    let same: PaperNotes = call(
        &e.bus,
        "workspace.save_notes",
        json!({ "paper": p.id, "text": "Rewritten" }),
    )
    .await
    .unwrap();
    assert_eq!(same.version, 4);

    // Restore version 3: a new version with the old text, itself in the history.
    let r: PaperNotes = call(
        &e.bus,
        "workspace.restore_notes",
        json!({ "paper": p.id, "version": 3 }),
    )
    .await
    .unwrap();
    assert_eq!(
        (r.version, r.text.as_str()),
        (5, "# Notes\n\nFirst idea, refined")
    );
    let hist: Vec<NotesVersion> = call(&e.bus, "workspace.notes_history", json!({ "paper": p.id }))
        .await
        .unwrap();
    assert_eq!(hist[0].version, 5);
    assert_eq!(hist[1].version, 4, "the version before the restore is kept");
    assert!(hist[0].preview.starts_with("# Notes First idea"));

    // After a restart the notebook is there.
    drop(e);
    let e2 = env(d.path());
    let _: ProjectInfo = call(&e2.bus, "workspace.open_default_library", json!(null))
        .await
        .unwrap();
    let back: PaperNotes = call(&e2.bus, "workspace.get_notes", json!({ "paper": p.id }))
        .await
        .unwrap();
    assert_eq!(back.version, 5);
    let missing = call::<PaperNotes>(
        &e2.bus,
        "workspace.save_notes",
        json!({ "paper": "00000000-0000-0000-0000-000000000009", "text": "x" }),
    )
    .await;
    assert!(matches!(missing, Err(CommandError::Failed { ref kind, .. }) if kind == "not_found"));
}
