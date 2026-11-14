//! Annotations through the bus (P1.M3.MOD.2–4, P1.M3.PDF.1–4): edit, undo/redo, save into the
//! PDF as an incremental update, re-import without duplicates, other tools' annotations kept,
//! rotated pages, several save cycles, and a restart. Needs `PDFium` and the fixture papers
//! (`cargo xtask fetch-natives`, `cargo xtask fixtures`); skipped with a note when missing.
#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::items_after_statements,
    clippy::used_underscore_binding,
    clippy::too_many_lines
)] // tests stop loudly and read top to bottom

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use ep_blob::BlobStore;
use ep_bus::{Bus, CommandError, Envelope};
use ep_document::{BlobAccess, Document};
use ep_document_contract::{Annotated, Annotation, AnnotationKind, AnnotationSet, Category};
use ep_types::{BlobRef, Hash, MediaType, PagePoint, PageQuad, PageRect, PaperId};
use serde_json::{Value, json};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..")
}

/// A library folder: blobs plus the folder the engine keeps its database in.
struct Library {
    dir: PathBuf,
    blobs: BlobStore,
}

impl BlobAccess for Library {
    fn path_of(&self, hash: &Hash) -> Option<PathBuf> {
        let p = self.blobs.path_of(hash);
        p.exists().then_some(p)
    }
    fn put(&self, bytes: &[u8], media_type: MediaType) -> std::io::Result<BlobRef> {
        self.blobs
            .put(bytes, media_type)
            .map_err(std::io::Error::other)
    }
    fn project(&self) -> Option<(String, PathBuf)> {
        Some(("library-1".into(), self.dir.clone()))
    }
}

struct Env {
    bus: Arc<Bus>,
    lib: Arc<Library>,
    _tmp: tempfile::TempDir,
}

fn pdfium_dir() -> Option<PathBuf> {
    let dir = root().join(".natives/pdfium/bin");
    if dir.exists() {
        Some(dir)
    } else {
        eprintln!("skipped: PDFium not fetched");
        None
    }
}

fn fixture(name: &str) -> Option<PathBuf> {
    let p = root().join(".fixtures/papers").join(name);
    if p.exists() {
        Some(p)
    } else {
        eprintln!("skipped: fixture {name} missing");
        None
    }
}

/// One `PDFium` per process (it cannot be loaded twice).
fn pdfium() -> Arc<dyn ep_document::Backend> {
    static PDFIUM: std::sync::OnceLock<Arc<dyn ep_document::Backend>> = std::sync::OnceLock::new();
    PDFIUM
        .get_or_init(|| ep_document::start_pdfium(&pdfium_dir().unwrap()).expect("pdfium"))
        .clone()
}

fn engine(dir: &Path) -> (Arc<Bus>, Arc<Library>) {
    let lib = Arc::new(Library {
        dir: dir.to_path_buf(),
        blobs: BlobStore::open(dir.join("blobs")).unwrap(),
    });
    let bus = Arc::new(Bus::default());
    let pdf = pdfium();
    let doc = Document::new(lib.clone(), Some(pdf), Some(&dir.join("cache"))).unwrap();
    doc.set_author("Tester");
    doc.register(&bus).unwrap();
    (bus, lib)
}

fn setup() -> Option<Env> {
    pdfium_dir()?;
    let tmp = tempfile::tempdir().unwrap();
    let (bus, lib) = engine(tmp.path());
    Some(Env {
        bus,
        lib,
        _tmp: tmp,
    })
}

async fn call<T: serde::de::DeserializeOwned>(
    bus: &Bus,
    name: &str,
    input: Value,
) -> Result<T, CommandError> {
    let v = bus.call(Envelope::new(name, input)).await?;
    Ok(serde_json::from_value(v).expect("output shape"))
}

fn new_paper() -> PaperId {
    use std::sync::atomic::{AtomicU64, Ordering};
    static N: AtomicU64 = AtomicU64::new(1);
    PaperId::from_uuid(uuid::Uuid::from_u128(u128::from(
        N.fetch_add(1, Ordering::SeqCst),
    )))
}

fn rect(page: u32, x0: f32, y0: f32, x1: f32, y1: f32) -> PageRect {
    PageRect {
        page,
        x0,
        y0,
        x1,
        y1,
    }
}

fn ann(id: &str, page: u32, kind: AnnotationKind) -> Annotation {
    let r = rect(page, 100.0, 120.0, 220.0, 134.0);
    Annotation {
        id: id.into(),
        page,
        kind,
        rect: r,
        quads: if kind.is_text_markup() {
            vec![PageQuad::from_rect(&r)]
        } else {
            vec![]
        },
        paths: if kind == AnnotationKind::Ink {
            vec![
                (0..20)
                    .map(|i| PagePoint {
                        x: 300.0 + i as f32 * 4.0,
                        y: 400.0 + (i % 3) as f32 * 5.0,
                    })
                    .collect(),
            ]
        } else {
            vec![]
        },
        line: matches!(kind, AnnotationKind::Line | AnnotationKind::Arrow).then_some([
            PagePoint { x: 80.0, y: 300.0 },
            PagePoint { x: 200.0, y: 260.0 },
        ]),
        color: "#ffd400".into(),
        opacity: 1.0,
        width: 1.5,
        font_size: (kind == AnnotationKind::FreeText).then_some(11.0),
        category: Some(Category::Question),
        body: if kind == AnnotationKind::FreeText {
            "Check this assumption".into()
        } else {
            "note text".into()
        },
        text: None,
        author: None,
        parent: None,
        resolved: false,
        created_ms: 0,
        modified_ms: 0,
        pristine: false,
    }
}

const KINDS: [AnnotationKind; 11] = [
    AnnotationKind::Highlight,
    AnnotationKind::Underline,
    AnnotationKind::StrikeOut,
    AnnotationKind::Squiggly,
    AnnotationKind::Note,
    AnnotationKind::FreeText,
    AnnotationKind::Ink,
    AnnotationKind::Rectangle,
    AnnotationKind::Ellipse,
    AnnotationKind::Line,
    AnnotationKind::Arrow,
];

fn add(path: &Path, lib: &Library) -> BlobRef {
    lib.blobs.put_file(path, MediaType::pdf()).unwrap()
}

async fn list(bus: &Bus, paper: PaperId, file: &BlobRef) -> AnnotationSet {
    call(
        bus,
        "document.annotations",
        json!({ "paper": paper, "file": file }),
    )
    .await
    .unwrap()
}

async fn apply(bus: &Bus, paper: PaperId, ops: Value) -> Result<AnnotationSet, CommandError> {
    call(
        bus,
        "document.annotations_apply",
        json!({ "paper": paper, "ops": ops }),
    )
    .await
}

#[tokio::test]
async fn edit_undo_save_and_reimport() {
    let (Some(env), Some(src)) = (setup(), fixture("arxiv-2111.12066v2.pdf")) else {
        return;
    };
    let original = std::fs::read(&src).unwrap();
    let paper = new_paper();
    let file = add(&src, &env.lib);
    let set = list(&env.bus, paper, &file).await;
    assert!(set.annotations.is_empty() && !set.can_undo && !set.unsaved);

    // One edit with every kind, then a reply and a resolved thread.
    let ops: Vec<Value> = KINDS
        .iter()
        .enumerate()
        .map(|(i, k)| json!({ "op": "add", "annotation": ann(&format!("ep-{i}"), 1 + (i as u32 % 2), *k) }))
        .collect();
    let set = apply(&env.bus, paper, json!(ops)).await.unwrap();
    assert_eq!(set.annotations.len(), 11);
    assert!(set.can_undo && set.unsaved);
    assert!(
        set.annotations
            .iter()
            .all(|a| a.author.as_deref() == Some("Tester"))
    );
    let mut reply = ann("ep-reply", 1, AnnotationKind::Note);
    reply.parent = Some("ep-4".into());
    reply.body = "An answer — with a non-ASCII dash".into();
    apply(
        &env.bus,
        paper,
        json!([{ "op": "add", "annotation": reply }]),
    )
    .await
    .unwrap();
    let mut note = set
        .annotations
        .iter()
        .find(|a| a.id == "ep-4")
        .unwrap()
        .clone();
    note.resolved = true;
    apply(
        &env.bus,
        paper,
        json!([{ "op": "change", "annotation": note }]),
    )
    .await
    .unwrap();

    // Undo twice, redo twice.
    let u1: AnnotationSet = call(
        &env.bus,
        "document.annotations_undo",
        json!({ "paper": paper }),
    )
    .await
    .unwrap();
    assert!(
        !u1.annotations
            .iter()
            .find(|a| a.id == "ep-4")
            .unwrap()
            .resolved
    );
    let u2: AnnotationSet = call(
        &env.bus,
        "document.annotations_undo",
        json!({ "paper": paper }),
    )
    .await
    .unwrap();
    assert_eq!(u2.annotations.len(), 11);
    assert!(u2.can_redo);
    call::<AnnotationSet>(
        &env.bus,
        "document.annotations_redo",
        json!({ "paper": paper }),
    )
    .await
    .unwrap();
    let r2: AnnotationSet = call(
        &env.bus,
        "document.annotations_redo",
        json!({ "paper": paper }),
    )
    .await
    .unwrap();
    assert_eq!(r2.annotations.len(), 12);
    assert!(!r2.can_redo);

    // Errors: duplicate id, unknown parent, empty mark-up.
    let e = apply(
        &env.bus,
        paper,
        json!([{ "op": "add", "annotation": ann("ep-0", 1, AnnotationKind::Note) }]),
    )
    .await;
    assert!(
        matches!(e, Err(CommandError::Failed { ref kind, .. }) if kind == "conflict"),
        "{e:?}"
    );
    let mut orphan = ann("ep-x", 1, AnnotationKind::Note);
    orphan.parent = Some("nope".into());
    let e = apply(
        &env.bus,
        paper,
        json!([{ "op": "add", "annotation": orphan }]),
    )
    .await;
    assert!(
        matches!(e, Err(CommandError::Failed { ref kind, .. }) if kind == "invalid_annotation"),
        "{e:?}"
    );

    // Save into the PDF.
    let t = Instant::now();
    let saved: Annotated = call(
        &env.bus,
        "document.annotate",
        json!({ "paper": paper, "file": file }),
    )
    .await
    .unwrap();
    let save_ms = t.elapsed().as_millis();
    eprintln!(
        "save into PDF: {save_ms} ms, written {}, kept {}",
        saved.written, saved.kept
    );
    assert_eq!(saved.written, 13, "12 annotations + 1 review state");
    let new_bytes = std::fs::read(env.lib.blobs.path_of(&saved.file.hash)).unwrap();
    assert!(
        new_bytes.starts_with(&original),
        "incremental: the original bytes are a prefix"
    );
    assert_eq!(
        std::fs::read(&src).unwrap(),
        original,
        "the source file is untouched"
    );
    assert!(
        env.lib.blobs.path_of(&file.hash).exists(),
        "the previous version is kept"
    );

    // What lopdf reads back from the new file.
    let raws =
        ep_document_adapter_lopdf::read_annotations(&env.lib.blobs.path_of(&saved.file.hash))
            .unwrap();
    for i in 0..11 {
        assert_eq!(
            raws.iter().filter(|r| r.key == format!("ep-{i}")).count(),
            1,
            "ep-{i} once"
        );
    }
    let reply_raw = raws.iter().find(|r| r.key == "ep-reply").unwrap();
    assert_eq!(reply_raw.irt.as_deref(), Some("ep-4"));
    assert!(
        raws.iter()
            .any(|r| r.irt.as_deref() == Some("ep-4") && r.state.as_deref() == Some("Completed"))
    );

    // PDFium opens the new file.
    let info: Value = call(&env.bus, "document.info", json!({ "file": saved.file }))
        .await
        .unwrap();
    assert!(info["pages"].as_u64().unwrap() > 1);

    // The new file adds nothing on import; the model is saved.
    let after = list(&env.bus, paper, &saved.file).await;
    assert_eq!(after.annotations.len(), 12);
    assert!(!after.unsaved);
    assert!(after.annotations.iter().all(|a| a.pristine));

    // Second cycle: delete one, change one, save. Unchanged ones are kept, not rewritten.
    let mut changed = after
        .annotations
        .iter()
        .find(|a| a.id == "ep-7")
        .unwrap()
        .clone();
    changed.color = "#1971c2".into();
    apply(
        &env.bus,
        paper,
        json!([{ "op": "remove", "id": "ep-0" }, { "op": "change", "annotation": changed }]),
    )
    .await
    .unwrap();
    let saved2: Annotated = call(
        &env.bus,
        "document.annotate",
        json!({ "paper": paper, "file": saved.file }),
    )
    .await
    .unwrap();
    assert_eq!(saved2.written, 1);
    assert_eq!(saved2.kept, 10, "11 unchanged minus the removed one");
    let raws =
        ep_document_adapter_lopdf::read_annotations(&env.lib.blobs.path_of(&saved2.file.hash))
            .unwrap();
    assert!(
        !raws.iter().any(|r| r.key == "ep-0"),
        "removed from /Annots"
    );
    let ep7: Vec<_> = raws.iter().filter(|r| r.key == "ep-7").collect();
    assert_eq!(ep7.len(), 1);
    assert!((ep7[0].color[2] - 0.761).abs() < 0.01, "the new colour");
    // Deleted ids are not imported again from an older file version.
    let back = list(&env.bus, paper, &file).await;
    assert_eq!(back.annotations.len(), 11);
    assert!(save_ms < 2000, "save took {save_ms} ms");
}

#[tokio::test]
async fn survives_restart() {
    let (Some(_), Some(src)) = (pdfium_dir(), fixture("arxiv-2311.07440v2.pdf")) else {
        return;
    };
    let tmp = tempfile::tempdir().unwrap();
    let paper = new_paper();
    let file = {
        let (bus, lib) = engine(tmp.path());
        let file = add(&src, &lib);
        list(&bus, paper, &file).await;
        apply(
            &bus,
            paper,
            json!([{ "op": "add", "annotation": ann("ep-a", 1, AnnotationKind::Highlight) }]),
        )
        .await
        .unwrap();
        apply(
            &bus,
            paper,
            json!([{ "op": "add", "annotation": ann("ep-b", 1, AnnotationKind::Note) }]),
        )
        .await
        .unwrap();
        file
    };
    let (bus, _lib) = engine(tmp.path());
    let set = list(&bus, paper, &file).await;
    assert_eq!(set.annotations.len(), 2);
    assert!(set.can_undo && set.unsaved);
    let set: AnnotationSet = call(&bus, "document.annotations_undo", json!({ "paper": paper }))
        .await
        .unwrap();
    assert_eq!(set.annotations.len(), 1, "undo history survives a restart");
}

/// A copy of `src` with annotations written the way other tools do: no `/NM` on some, a popup,
/// a reply, a review state, a link and a stamp (a kind the Reader does not edit); page 1 rotated.
fn foreign_copy(src: &Path, out: &Path, rotate: bool) {
    use lopdf::{Object, StringFormat, dictionary};
    let mut doc = lopdf::Document::load(src).unwrap();
    let page = *doc.get_pages().get(&1).unwrap();
    let s = |t: &str| Object::String(t.as_bytes().to_vec(), StringFormat::Literal);
    let hl = doc.add_object(dictionary! {
        "Type" => "Annot", "Subtype" => "Highlight", "Rect" => vec![100.into(), 600.into(), 200.into(), 612.into()],
        "QuadPoints" => vec![100.into(), 612.into(), 200.into(), 612.into(), 100.into(), 600.into(), 200.into(), 600.into()],
        "C" => vec![Object::Real(0.0), Object::Real(1.0), Object::Real(0.0)], "T" => s("Other tool"),
        "Contents" => s("highlight made elsewhere"), "Subj" => s("Result"), "P" => page,
    });
    let note = doc.add_object(dictionary! {
        "Type" => "Annot", "Subtype" => "Text", "Rect" => vec![300.into(), 600.into(), 318.into(), 618.into()],
        "NM" => s("acro-note-1"), "T" => s("Other tool"), "Contents" => s("A question"), "P" => page,
        "M" => s("D:20250101120000Z"),
    });
    let popup = doc.add_object(dictionary! {
        "Type" => "Annot", "Subtype" => "Popup", "Rect" => vec![318.into(), 500.into(), 500.into(), 618.into()], "Parent" => note,
    });
    let reply = doc.add_object(dictionary! {
        "Type" => "Annot", "Subtype" => "Text", "Rect" => vec![300.into(), 600.into(), 318.into(), 618.into()],
        "IRT" => note, "T" => s("Someone"), "Contents" => s("An answer"), "P" => page,
    });
    let state = doc.add_object(dictionary! {
        "Type" => "Annot", "Subtype" => "Text", "Rect" => vec![300.into(), 600.into(), 318.into(), 618.into()],
        "IRT" => note, "StateModel" => s("Review"), "State" => s("Completed"), "M" => s("D:20250102120000Z"), "P" => page,
    });
    let stamp = doc.add_object(dictionary! {
        "Type" => "Annot", "Subtype" => "Stamp", "Rect" => vec![50.into(), 50.into(), 150.into(), 100.into()], "Name" => "Approved", "P" => page,
    });
    let link = doc.add_object(dictionary! {
        "Type" => "Annot", "Subtype" => "Link", "Rect" => vec![50.into(), 700.into(), 150.into(), 712.into()],
        "A" => dictionary! { "S" => "URI", "URI" => s("https://example.org") },
    });
    if let Ok(Object::Dictionary(p)) = doc.get_object_mut(page) {
        p.set(
            "Annots",
            vec![
                hl.into(),
                note.into(),
                popup.into(),
                reply.into(),
                state.into(),
                stamp.into(),
                link.into(),
            ],
        );
        if rotate {
            p.set("Rotate", 90);
        }
    }
    doc.save(out).unwrap();
}

#[tokio::test]
async fn other_tools_annotations_are_imported_and_kept() {
    let (Some(env), Some(src)) = (setup(), fixture("arxiv-2311.07440v2.pdf")) else {
        return;
    };
    let derived = env._tmp.path().join("foreign.pdf");
    foreign_copy(&src, &derived, true);
    let paper = new_paper();
    let file = add(&derived, &env.lib);
    let set = list(&env.bus, paper, &file).await;
    // Highlight, note and reply: popup, review state, stamp and link are not editable kinds.
    assert_eq!(set.annotations.len(), 3, "{:#?}", set.annotations);
    let note = set
        .annotations
        .iter()
        .find(|a| a.id == "acro-note-1")
        .unwrap();
    assert!(note.resolved, "the review state marks the thread resolved");
    let reply = set
        .annotations
        .iter()
        .find(|a| a.body == "An answer")
        .unwrap();
    assert_eq!(reply.parent.as_deref(), Some("acro-note-1"));
    let hl = set
        .annotations
        .iter()
        .find(|a| a.kind == AnnotationKind::Highlight)
        .unwrap();
    assert!(hl.id.starts_with("pdf-"));
    assert_eq!(hl.category, Some(Category::Result));
    assert_eq!(hl.color, "#00ff00");
    assert!(set.annotations.iter().all(|a| a.pristine));
    // Opening again imports nothing new.
    assert_eq!(list(&env.bus, paper, &file).await.annotations.len(), 3);

    // Add a highlight on the rotated page; change nothing else; save.
    let mine = ann("ep-mine", 1, AnnotationKind::Highlight);
    apply(
        &env.bus,
        paper,
        json!([{ "op": "add", "annotation": mine.clone() }]),
    )
    .await
    .unwrap();
    let saved: Annotated = call(
        &env.bus,
        "document.annotate",
        json!({ "paper": paper, "file": file }),
    )
    .await
    .unwrap();
    assert_eq!(saved.written, 1);
    assert_eq!(saved.kept, 3);
    let raws =
        ep_document_adapter_lopdf::read_annotations(&env.lib.blobs.path_of(&saved.file.hash))
            .unwrap();
    for sub in ["Popup", "Stamp", "Link"] {
        assert!(
            raws.iter().any(|r| r.subtype == sub),
            "{sub} kept untouched"
        );
    }
    assert_eq!(
        raws.iter().filter(|r| r.state.is_some()).count(),
        1,
        "the review state stays"
    );

    // Round trip on the rotated page: a fresh library imports our highlight where it was drawn.
    let tmp2 = tempfile::tempdir().unwrap();
    let (bus2, lib2) = engine(tmp2.path());
    let copy = env.lib.blobs.path_of(&saved.file.hash);
    let f2 = add(&copy, &lib2);
    let set2 = list(&bus2, new_paper(), &f2).await;
    let back = set2.annotations.iter().find(|a| a.id == "ep-mine").unwrap();
    for (p, q) in back.quads[0].p.iter().zip(&mine.quads[0].p) {
        assert!(
            (p.x - q.x).abs() < 0.05 && (p.y - q.y).abs() < 0.05,
            "{p:?} vs {q:?}"
        );
    }

    // Resolve-state change and deleting the foreign note: its popup, reply and state go too.
    apply(
        &env.bus,
        paper,
        json!([{ "op": "remove", "id": "acro-note-1" }]),
    )
    .await
    .unwrap();
    let saved3: Annotated = call(
        &env.bus,
        "document.annotate",
        json!({ "paper": paper, "file": saved.file }),
    )
    .await
    .unwrap();
    let raws =
        ep_document_adapter_lopdf::read_annotations(&env.lib.blobs.path_of(&saved3.file.hash))
            .unwrap();
    assert!(!raws.iter().any(|r| r.key == "acro-note-1"));
    assert!(
        !raws.iter().any(|r| r.subtype == "Popup"),
        "the orphan popup is dropped"
    );
    assert!(
        !raws.iter().any(|r| r.state.is_some()),
        "the review state of a deleted note is dropped"
    );
    assert!(raws.iter().any(|r| r.subtype == "Stamp"));
}

#[tokio::test]
async fn without_a_project_the_commands_say_so() {
    if pdfium_dir().is_none() {
        return;
    }
    struct Nothing;
    impl BlobAccess for Nothing {
        fn path_of(&self, _: &Hash) -> Option<PathBuf> {
            None
        }
    }
    let bus = Arc::new(Bus::default());
    Document::new(Arc::new(Nothing), Some(pdfium()), None)
        .unwrap()
        .register(&bus)
        .unwrap();
    let e = call::<AnnotationSet>(
        &bus,
        "document.annotations_undo",
        json!({ "paper": new_paper() }),
    )
    .await;
    assert!(
        matches!(e, Err(CommandError::Failed { ref kind, .. }) if kind == "no_project_open"),
        "{e:?}"
    );
}
