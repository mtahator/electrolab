//! Document intelligence on real papers (P1.M4): preparation, objects with the layout model,
//! contents, mentions and citation markers linked to their targets, references, region lookup,
//! tables and equations. Needs `PDFium` and the fixtures; the layout part also needs ONNX Runtime
//! (`cargo xtask fetch-natives onnxruntime`) and the model in `EP_MODELS_DIR` — skipped with a note
//! when missing.
#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    clippy::too_many_lines,
    clippy::cast_precision_loss
)]

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use ep_bus::{Bus, CommandError, Envelope};
use ep_document::{BlobAccess, Document};
use ep_document_contract::{
    CitationMarker, Contents, Mention, MentionKind, ObjectKind, PageObjects, Preparation,
    Reference, RegionHit, StageStatus, Table,
};
use ep_types::{BlobRef, Hash, MediaType};
use serde_json::{Value, json};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..")
}

struct OneFile(PathBuf);
impl BlobAccess for OneFile {
    fn path_of(&self, _hash: &Hash) -> Option<PathBuf> {
        Some(self.0.clone())
    }
}

fn pdfium() -> Option<Arc<dyn ep_document::Backend>> {
    static P: std::sync::OnceLock<Option<Arc<dyn ep_document::Backend>>> =
        std::sync::OnceLock::new();
    P.get_or_init(|| {
        let dir = root().join(".natives/pdfium/bin");
        dir.exists()
            .then(|| ep_document::start_pdfium(&dir).expect("pdfium"))
    })
    .clone()
}

/// The engine on one fixture paper; with the layout model when `models` is true and it is there.
fn setup(name: &str, models: bool, cache: &Path) -> Option<(Arc<Bus>, BlobRef, bool)> {
    let file = root().join(".fixtures/papers").join(name);
    let Some(pdf) = pdfium() else {
        eprintln!("skipped: PDFium not fetched");
        return None;
    };
    if !file.exists() {
        eprintln!("skipped: fixture {name} missing");
        return None;
    }
    let doc = Document::new(Arc::new(OneFile(file.clone())), Some(pdf), Some(cache)).unwrap();
    let mut with_model = false;
    if models {
        match std::env::var_os("EP_MODELS_DIR") {
            Some(dir) => {
                doc.load_page_models(Path::new(&dir)).expect("models load");
                with_model = Document::model_specs()
                    .iter()
                    .all(|m| m.present(Some(Path::new(&dir))));
            }
            None => eprintln!("note: EP_MODELS_DIR not set; layout model parts skipped"),
        }
    }
    let bus = Arc::new(Bus::default());
    doc.register(&bus).unwrap();
    let bytes = std::fs::read(&file).unwrap();
    Some((
        bus,
        BlobRef {
            hash: Hash::of(&bytes),
            size: bytes.len() as u64,
            media_type: MediaType::pdf(),
        },
        with_model,
    ))
}

async fn call<T: serde::de::DeserializeOwned>(
    bus: &Bus,
    name: &str,
    input: Value,
) -> Result<T, CommandError> {
    let v = bus.call(Envelope::new(name, input)).await?;
    Ok(serde_json::from_value(v).expect("output shape"))
}

#[tokio::test]
async fn text_rules_on_a_real_paper() {
    let tmp = tempfile::tempdir().unwrap();
    let Some((bus, file, _)) = setup("arxiv-2111.12066v2.pdf", false, tmp.path()) else {
        return;
    };
    let refs: Vec<Reference> = call(&bus, "document.references", json!({ "file": file }))
        .await
        .unwrap();
    eprintln!("references: {}", refs.len());
    for r in refs.iter().take(3) {
        eprintln!(
            "  [{:?}] {:?} | {:?} | {:?}",
            r.label, r.authors, r.year, r.title
        );
    }
    assert!(refs.len() >= 20, "{} references", refs.len());
    let with_year = refs.iter().filter(|r| r.year.is_some()).count();
    assert!(
        with_year * 10 >= refs.len() * 8,
        "years found in {with_year}/{}",
        refs.len()
    );

    let contents: Contents = call(&bus, "document.contents", json!({ "file": file }))
        .await
        .unwrap();
    let titles: Vec<&str> = contents.sections.iter().map(|s| s.title.as_str()).collect();
    eprintln!("sections: {titles:?}");
    assert!(
        titles.iter().any(|t| t.contains("Introduction")),
        "{titles:?}"
    );
    eprintln!(
        "figures {} tables {} equations {}",
        contents.figures.len(),
        contents.tables.len(),
        contents.equations.len()
    );

    let mut markers = 0;
    let mut linked = 0;
    let mut mentions_total = 0;
    let mut mentions_linked = 0;
    for page in 1..=6 {
        let m: Vec<CitationMarker> = call(
            &bus,
            "document.citation_markers",
            json!({ "file": file, "page": page }),
        )
        .await
        .unwrap();
        markers += m.len();
        linked += m.iter().filter(|x| !x.entries.is_empty()).count();
        let ms: Vec<Mention> = call(
            &bus,
            "document.mentions",
            json!({ "file": file, "page": page }),
        )
        .await
        .unwrap();
        mentions_total += ms.len();
        mentions_linked += ms.iter().filter(|x| x.target.is_some()).count();
        for x in ms.iter().filter(|x| x.target.is_none()).take(3) {
            eprintln!("  unlinked mention p{page}: {:?} {:?}", x.kind, x.text);
        }
    }
    eprintln!(
        "citation markers {markers} (linked {linked}); mentions {mentions_total} (linked {mentions_linked})"
    );
    assert!(
        markers >= 10 && linked == markers,
        "markers {markers} linked {linked}"
    );
    assert!(mentions_total >= 3, "mentions {mentions_total}");
}

#[tokio::test]
async fn layout_model_objects_region_table() {
    let tmp = tempfile::tempdir().unwrap();
    let Some((bus, file, with_model)) = setup("arxiv-2111.12066v2.pdf", true, tmp.path()) else {
        return;
    };
    if !with_model {
        eprintln!("skipped: layout model not available");
        return;
    }
    let t = Instant::now();
    let prep: Preparation = call(
        &bus,
        "document.prepare",
        json!({ "file": file, "first_pages": [3] }),
    )
    .await
    .unwrap();
    let secs = t.elapsed().as_secs_f32();
    let objects = prep
        .stages
        .iter()
        .find(|s| s.stage == ep_document_contract::Stage::Objects)
        .unwrap();
    eprintln!(
        "prepared {} pages in {secs:.1} s ({:.0} ms/page)",
        objects.total,
        1000.0 * secs / objects.total as f32
    );
    assert_eq!(objects.status, StageStatus::Done);

    let contents: Contents = call(&bus, "document.contents", json!({ "file": file }))
        .await
        .unwrap();
    eprintln!(
        "figures {:?}\ntables {:?}\nequations {:?}",
        contents
            .figures
            .iter()
            .map(|o| o.label.clone().unwrap_or_default())
            .collect::<Vec<_>>(),
        contents
            .tables
            .iter()
            .map(|o| o.label.clone().unwrap_or_default())
            .collect::<Vec<_>>(),
        contents
            .equations
            .iter()
            .map(|o| o.label.clone().unwrap_or_default())
            .collect::<Vec<_>>()
    );
    assert!(contents.figures.len() >= 3);
    assert!(contents.equations.len() >= 3);

    // Mentions now point at detected figures.
    let mut linked_to_figure = 0;
    for page in 1..=8 {
        let ms: Vec<Mention> = call(
            &bus,
            "document.mentions",
            json!({ "file": file, "page": page }),
        )
        .await
        .unwrap();
        linked_to_figure += ms
            .iter()
            .filter(|m| {
                m.kind == MentionKind::Figure
                    && m.target.as_ref().is_some_and(|t| t.id.contains("figure"))
            })
            .count();
    }
    eprintln!("figure mentions linked to detected figures: {linked_to_figure}");
    assert!(linked_to_figure >= 2);

    // Region: the centre of the first figure is that figure; a crop comes back.
    let fig = &contents.figures[0];
    let (cx, cy) = (
        f32::midpoint(fig.rect.x0, fig.rect.x1),
        f32::midpoint(fig.rect.y0, fig.rect.y1),
    );
    let t = Instant::now();
    let hit: RegionHit = call(
        &bus,
        "document.region",
        json!({ "file": file, "page": fig.rect.page, "point": { "x": cx, "y": cy } }),
    )
    .await
    .unwrap();
    eprintln!("region in {} ms", t.elapsed().as_millis());
    assert_eq!(
        hit.object.as_ref().map(|o| o.id.clone()),
        Some(fig.id.clone())
    );
    assert!(hit.image.size > 1000);

    // A table as cells.
    if let Some(tab) = contents.tables.first() {
        let table: Table = call(
            &bus,
            "document.table",
            json!({ "file": file, "object": { "page": tab.rect.page, "id": tab.id } }),
        )
        .await
        .unwrap();
        eprintln!(
            "{} rows × {} cols, header rows {}\n{}",
            table.rows.len(),
            table.rows.first().map_or(0, Vec::len),
            table.header_rows,
            table.csv.lines().take(4).collect::<Vec<_>>().join("\n")
        );
        assert!(table.rows.len() >= 2);
    }
    let objs: PageObjects = call(&bus, "document.objects", json!({ "file": file, "page": 1 }))
        .await
        .unwrap();
    assert!(
        objs.detected
            && objs
                .objects
                .iter()
                .any(|o| o.kind == ObjectKind::Title || o.kind == ObjectKind::Heading)
    );
}

#[tokio::test]
#[ignore = "debugging aid"]
async fn print_heading_like_lines() {
    let tmp = tempfile::tempdir().unwrap();
    let Some((bus, file, _)) = setup("arxiv-2111.12066v2.pdf", false, tmp.path()) else {
        return;
    };
    for page in 2..=5 {
        let pt: ep_document_contract::PageText =
            call(&bus, "document.text", json!({ "file": file, "page": page }))
                .await
                .unwrap();
        let chars: Vec<char> = pt.text.chars().collect();
        for l in &pt.lines {
            let t: String = chars[l.start as usize..l.end as usize].iter().collect();
            if t.trim().starts_with(|c: char| c.is_ascii_digit()) && t.len() < 80 {
                eprintln!("p{page} h={:.1} {:?}", l.rect.y1 - l.rect.y0, t.trim());
            }
        }
    }
}
