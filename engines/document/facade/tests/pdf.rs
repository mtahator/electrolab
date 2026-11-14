//! `document.*` PDF commands through the bus, on fixture papers (needs `PDFium`:
//! `cargo xtask fetch-natives`; skipped with a note when missing).
#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)] // tests and their helpers stop loudly

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use ep_bus::{Bus, CommandError, Envelope};
use ep_document::{BlobAccess, Document};
use ep_document_contract::{
    DocInfo, Link, LinkTarget, OutlineItem, PageText, Rendered, SearchResult,
};
use ep_types::{BlobRef, Hash, MediaType};
use serde_json::{Value, json};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..")
}

/// Every hash maps to one fixture file.
struct OneFile(PathBuf);
impl BlobAccess for OneFile {
    fn path_of(&self, _hash: &Hash) -> Option<PathBuf> {
        Some(self.0.clone())
    }
}

fn setup(file: &Path, cache: &Path) -> Option<(Arc<Bus>, BlobRef)> {
    let dir = root().join(".natives/pdfium/bin");
    if !dir.exists() {
        eprintln!("skipped: PDFium not fetched");
        return None;
    }
    let bus = Arc::new(Bus::default());
    let pdf = ep_document::start_pdfium(&dir).expect("pdfium");
    Document::new(
        Arc::new(OneFile(file.to_path_buf())),
        Some(pdf),
        Some(cache),
    )
    .expect("engine")
    .register(&bus)
    .expect("register");
    let bytes = std::fs::read(file).unwrap();
    Some((
        bus,
        BlobRef {
            hash: Hash::of(&bytes),
            size: bytes.len() as u64,
            media_type: MediaType::pdf(),
        },
    ))
}

async fn call<T: serde::de::DeserializeOwned>(
    bus: &Bus,
    name: &str,
    input: Value,
) -> Result<T, CommandError> {
    Ok(serde_json::from_value(bus.call(Envelope::new(name, input)).await?).expect("contract"))
}

#[tokio::test]
async fn reading_commands_on_the_wikipedia_fixture() {
    let d = tempfile::tempdir().unwrap();
    let file = root().join("fixtures/papers/fawiki-artificial-neural-network.pdf");
    let Some((bus, f)) = setup(&file, d.path()) else {
        return;
    };

    let info: DocInfo = call(&bus, "document.info", json!({ "file": f }))
        .await
        .unwrap();
    assert_eq!(info.pages as usize, info.sizes.len());
    assert!(info.sizes[0].width > 100.0);

    let text: PageText = call(&bus, "document.text", json!({ "file": f, "page": 1 }))
        .await
        .unwrap();
    assert!(text.words.len() > 50);
    assert_eq!(text.boxes.len(), text.text.chars().count());

    let t = Instant::now();
    let r1: Rendered = call(
        &bus,
        "document.render",
        json!({ "file": f, "page": 1, "width_px": 800 }),
    )
    .await
    .unwrap();
    let cold = t.elapsed();
    let t = Instant::now();
    let r2: Rendered = call(
        &bus,
        "document.render",
        json!({ "file": f, "page": 1, "width_px": 800 }),
    )
    .await
    .unwrap();
    let warm = t.elapsed();
    assert_eq!(r1, r2);
    assert_eq!(
        (r1.width_px, r1.image.media_type.0.as_str()),
        (800, "image/png")
    );
    eprintln!("render 800 px: cold {cold:?}, warm {warm:?}");
    assert!(warm < cold, "second render must come from the cache");

    let thumbs: Vec<Rendered> = call(
        &bus,
        "document.thumbnails",
        json!({ "file": f, "pages": [1, 2], "width_px": 120 }),
    )
    .await
    .unwrap();
    assert_eq!(thumbs.iter().map(|t| t.page).collect::<Vec<_>>(), [1, 2]);

    let links: Vec<Link> = call(&bus, "document.links", json!({ "file": f, "page": 1 }))
        .await
        .unwrap();
    assert!(links.iter().all(|l| !matches!(&l.target, LinkTarget::Url { url } if !(url.starts_with("http") || url.starts_with("mailto:")))));

    let e = call::<PageText>(&bus, "document.text", json!({ "file": f, "page": 99 }))
        .await
        .unwrap_err();
    assert!(matches!(e, CommandError::Failed { ref kind, .. } if kind == "bad_page"));
}

#[tokio::test]
async fn outline_and_search_on_a_paper_with_bookmarks() {
    let file = root().join(".fixtures/papers/arxiv-2111.12066v2.pdf");
    if !file.exists() {
        eprintln!("skipped: fixtures not fetched");
        return;
    }
    let d = tempfile::tempdir().unwrap();
    let Some((bus, f)) = setup(&file, d.path()) else {
        return;
    };
    let outline: Vec<OutlineItem> = call(&bus, "document.outline", json!({ "file": f }))
        .await
        .unwrap();
    assert!(outline.len() > 1, "outline: {outline:?}");
    assert!(
        outline
            .iter()
            .any(|o| matches!(o.target, Some(LinkTarget::Page { .. })))
    );

    let t = Instant::now();
    let r: SearchResult = call(
        &bus,
        "document.search",
        json!({ "papers": [f.hash], "terms": [{ "text": "thermal" }] }),
    )
    .await
    .unwrap();
    eprintln!("first search (builds the index): {:?}", t.elapsed());
    assert!(r.counts[0].total > 3);
    let hit = &r.hits[0];
    assert!(!hit.rects.is_empty() && hit.page >= 1);
    let t = Instant::now();
    let _: SearchResult = call(
        &bus,
        "document.search",
        json!({ "papers": [f.hash], "terms": [{ "text": "neural network" }] }),
    )
    .await
    .unwrap();
    eprintln!("second search (index cached): {:?}", t.elapsed());
}
