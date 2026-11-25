//! Folders in the library and changes of several papers at once (contract 0.5.0), through the bus.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use std::sync::Arc;

use ep_bus::{Bus, CommandError, Envelope};
use ep_workspace::Workspace;
use ep_workspace_contract::{AppliedFolders, Collection, Paper};
use serde_json::{Value, json};

struct Lib {
    bus: Arc<Bus>,
    dir: tempfile::TempDir,
}

async fn lib(papers: usize) -> (Lib, Vec<String>) {
    let dir = tempfile::tempdir().unwrap();
    let bus = Arc::new(Bus::default());
    Workspace::new(Arc::new(ep_os::RootDirs(dir.path().join("data"))))
        .register(&bus)
        .unwrap();
    let l = Lib { bus, dir };
    l.ok("workspace.open_default_library", Value::Null).await;
    let mut ids = vec![];
    for i in 0..papers {
        let p = l.dir.path().join(format!("{i}.pdf"));
        std::fs::write(&p, format!("%PDF-1.7\n% paper {i}\n%%EOF")).unwrap();
        let v = l.ok("workspace.add_file", json!({ "path": p })).await;
        ids.push(v["paper"]["id"].as_str().unwrap().to_string());
    }
    (l, ids)
}

impl Lib {
    async fn call(&self, c: &str, i: Value) -> Result<Value, CommandError> {
        self.bus.call(Envelope::new(c, i)).await
    }
    async fn ok(&self, c: &str, i: Value) -> Value {
        self.call(c, i)
            .await
            .unwrap_or_else(|e| panic!("{c}: {e:?}"))
    }
    async fn folder(&self, name: &str, parent: Option<&str>) -> String {
        let v = self
            .ok(
                "workspace.create_collection",
                json!({ "name": name, "parent": parent }),
            )
            .await;
        v["id"].as_str().unwrap().to_string()
    }
    async fn folders(&self) -> Vec<Collection> {
        serde_json::from_value(self.ok("workspace.list_collections", Value::Null).await).unwrap()
    }
    async fn papers(&self, filter: Value) -> Vec<String> {
        let mut f = json!({ "offset": 0, "sort": "title" });
        f.as_object_mut()
            .unwrap()
            .extend(filter.as_object().unwrap().clone());
        let v: Vec<Paper> =
            serde_json::from_value(self.ok("workspace.list_papers", f).await).unwrap();
        v.iter().map(|p| p.id.to_string()).collect()
    }
    async fn apply(&self, groups: Value) -> AppliedFolders {
        serde_json::from_value(
            self.ok("workspace.apply_folders", json!({ "groups": groups }))
                .await,
        )
        .unwrap()
    }
}

fn kind(r: Result<Value, CommandError>) -> String {
    match r.unwrap_err() {
        CommandError::Failed { kind, .. } => kind,
        CommandError::InvalidInput { .. } => "invalid".into(),
        e => format!("{e:?}"),
    }
}

#[tokio::test]
async fn folders_can_be_inside_folders() {
    let (l, _) = lib(0).await;
    let a = l.folder("Energy", None).await;
    let b = l.folder("Wind", Some(&a)).await;
    let c = l.folder("Offshore", Some(&b)).await;
    // The same name is fine in another place, not in the same one.
    l.folder("Wind", None).await;
    assert_eq!(
        kind(
            l.call(
                "workspace.create_collection",
                json!({ "name": "wind", "parent": a })
            )
            .await
        ),
        "exists"
    );
    let all = l.folders().await;
    let parent = |id: &str| {
        all.iter()
            .find(|x| x.id.to_string() == id)
            .unwrap()
            .parent
            .map(|p| p.to_string())
    };
    assert_eq!(
        (parent(&a), parent(&b), parent(&c)),
        (None, Some(a.clone()), Some(b.clone()))
    );
    // Never into itself or into a folder of its own.
    for into in [&a, &b, &c] {
        assert_eq!(
            kind(
                l.call(
                    "workspace.move_collection",
                    json!({ "id": a, "parent": into })
                )
                .await
            ),
            "invalid"
        );
    }
    l.ok("workspace.move_collection", json!({ "id": c })).await;
    assert_eq!(
        l.folders()
            .await
            .iter()
            .find(|x| x.id.to_string() == c)
            .unwrap()
            .parent,
        None
    );
}

#[tokio::test]
async fn several_papers_move_at_once_and_back() {
    let (l, p) = lib(5).await;
    let a = l.folder("A", None).await;
    let done = l
        .apply(json!([{ "papers": [p[0], p[1], p[2]], "collection": a }]))
        .await;
    assert_eq!(l.papers(json!({ "collection": a })).await.len(), 3);
    assert_eq!(l.papers(json!({ "unfiled": true })).await.len(), 2);
    // Moving is moving: the paper is in the new folder only.
    let second = l
        .apply(json!([{ "papers": [p[0]], "new": { "name": "B", "parent": a } }]))
        .await;
    let b = second.created[0].id.to_string();
    assert_eq!(l.papers(json!({ "collection": a })).await.len(), 2);
    assert_eq!(l.papers(json!({ "collection": b })).await, [p[0].clone()]);
    l.ok(
        "workspace.undo_folders",
        serde_json::to_value(&second).unwrap(),
    )
    .await;
    assert_eq!(l.papers(json!({ "collection": a })).await.len(), 3);
    assert_eq!(
        l.folders().await.len(),
        1,
        "the folder made for it is gone again"
    );
    l.ok(
        "workspace.undo_folders",
        serde_json::to_value(&done).unwrap(),
    )
    .await;
    assert_eq!(l.papers(json!({ "unfiled": true })).await.len(), 5);
    // Out of all folders.
    l.apply(json!([{ "papers": [p[0]], "collection": a }]))
        .await;
    l.apply(json!([{ "papers": [p[0]] }])).await;
    assert_eq!(l.papers(json!({ "unfiled": true })).await.len(), 5);
}

#[tokio::test]
async fn a_sorting_is_applied_whole_or_not_at_all() {
    let (l, p) = lib(4).await;
    let gone = "col_018f0000000070008000000000000000";
    let r = l
        .call(
            "workspace.apply_folders",
            json!({ "groups": [
                { "papers": [p[0], p[1]], "new": { "name": "New" } },
                { "papers": [p[2]], "collection": gone }
            ] }),
        )
        .await;
    assert!(r.is_err());
    assert_eq!(l.folders().await, [], "nothing was made");
    assert_eq!(l.papers(json!({ "unfiled": true })).await.len(), 4);
    // An existing folder of the same name in the same place is used, not doubled.
    l.folder("Reports", None).await;
    let done = l
        .apply(json!([{ "papers": [p[0]], "new": { "name": "reports" } }]))
        .await;
    assert_eq!((done.created.len(), l.folders().await.len()), (0, 1));
    assert_eq!(
        kind(
            l.call(
                "workspace.apply_folders",
                json!({ "groups": [{ "papers": [], "new": { "name": "  " } }] })
            )
            .await
        ),
        "invalid"
    );
}

#[tokio::test]
async fn several_papers_are_removed_and_come_back_with_their_folder() {
    let (l, p) = lib(3).await;
    let a = l.folder("A", None).await;
    l.apply(json!([{ "papers": p, "collection": a }])).await;
    l.ok("workspace.remove_papers", json!({ "ids": [p[0], p[1]] }))
        .await;
    assert_eq!(l.papers(json!({})).await, [p[2].clone()]);
    assert_eq!(
        l.folders().await[0].papers,
        1,
        "removed papers are not counted"
    );
    l.ok("workspace.restore_papers", json!({ "ids": [p[0], p[1]] }))
        .await;
    assert_eq!(l.papers(json!({ "collection": a })).await.len(), 3);
}

#[tokio::test]
async fn deleting_a_folder_keeps_its_papers() {
    let (l, p) = lib(2).await;
    let a = l.folder("A", None).await;
    let b = l.folder("B", Some(&a)).await;
    l.apply(json!([{ "papers": [p[0]], "collection": a }, { "papers": [p[1]], "collection": b }]))
        .await;
    let mut env = Envelope::new("workspace.delete_collection", json!({ "id": a }));
    if let Err(CommandError::NeedsApproval { rule, .. }) = l.bus.call(env.clone()).await {
        env = Envelope::new("workspace.delete_collection", json!({ "id": a }));
        env.approved = [rule].into();
        l.bus.call(env).await.unwrap();
    }
    assert_eq!(l.folders().await, [], "the folders in it go with it");
    assert_eq!(
        l.papers(json!({ "unfiled": true })).await.len(),
        2,
        "no paper is lost"
    );
}

#[tokio::test]
async fn a_digest_is_kept_with_its_paper_and_goes_with_it() {
    let (l, p) = lib(2).await;
    l.ok(
        "workspace.save_figures",
        json!({ "paper": p[0], "data": { "seen": [] } }),
    )
    .await;
    let figures = l.ok("workspace.get_figures", json!({})).await;
    assert_eq!(figures[0]["paper"], json!(p[0]), "apart from the digests");
    for id in &p {
        l.ok(
            "workspace.save_digest",
            json!({ "paper": id, "data": { "summary": id } }),
        )
        .await;
    }
    let all = l.ok("workspace.get_digests", json!({})).await;
    assert_eq!(all.as_array().unwrap().len(), 2);
    let one = l
        .ok("workspace.get_digests", json!({ "papers": [p[0]] }))
        .await;
    assert_eq!(one[0]["data"]["summary"], json!(p[0]));
    l.ok("workspace.remove_papers", json!({ "ids": [p[0]] }))
        .await;
    // The library is opened again: removed papers are purged.
    let bus = Arc::new(Bus::default());
    Workspace::new(Arc::new(ep_os::RootDirs(l.dir.path().join("data"))))
        .register(&bus)
        .unwrap();
    let call = async |c: &str, i: Value| bus.call(Envelope::new(c, i)).await.unwrap();
    call("workspace.open_default_library", Value::Null).await;
    let left = call("workspace.get_digests", json!({})).await;
    assert_eq!(left.as_array().unwrap().len(), 1);
    assert_eq!(left[0]["paper"], json!(p[1]));
    let figures = call("workspace.get_figures", json!({})).await;
    assert_eq!(figures, json!([]), "gone with the paper");
    let r = l
        .call(
            "workspace.save_digest",
            json!({ "paper": "01a20e02-906d-7097-a00e-0ce6cdb6b13d", "data": {} }),
        )
        .await;
    assert_eq!(kind(r), "not_found");
}
