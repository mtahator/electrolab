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
    fn put(&self, bytes: &[u8], media_type: MediaType) -> std::io::Result<BlobRef> {
        // Tests keep written files next to the source.
        let hash = Hash::of(bytes);
        std::fs::write(self.0.with_extension(format!("{hash}.pdf")), bytes)?;
        Ok(BlobRef {
            hash,
            size: bytes.len() as u64,
            media_type,
        })
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
    setup_file(root().join(".fixtures/papers").join(name), models, cache)
}

#[allow(clippy::needless_pass_by_value)]
fn setup_file(file: PathBuf, models: bool, cache: &Path) -> Option<(Arc<Bus>, BlobRef, bool)> {
    let name = file.display().to_string();
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
                let dirs = vec![PathBuf::from(dir)];
                doc.load_page_models(&dirs).expect("models load");
                with_model = ep_document::ModelSpec::present(&Document::model_specs()[0], &dirs);
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

/// An image-only ("scanned") copy of page `page` of `src`: the page rendered, put back as a
/// picture, no text layer.
fn scanned_copy(src: &Path, page: u32, out: &Path) {
    use lopdf::{Object, Stream, dictionary};
    let pdf = pdfium().unwrap();
    let (geoms, _) = pdf.info(src).unwrap();
    let size = geoms[(page - 1) as usize].size();
    let bmp = pdf.render(src, page - 1, 1700).unwrap();
    let rgb: Vec<u8> = bmp
        .rgba
        .as_chunks::<4>()
        .0
        .iter()
        .flat_map(|p| [p[0], p[1], p[2]])
        .collect();
    let mut doc = lopdf::Document::with_version("1.7");
    let img = doc.add_object(Stream::new(
        dictionary! { "Type" => "XObject", "Subtype" => "Image", "Width" => i64::from(bmp.width), "Height" => i64::from(bmp.height), "ColorSpace" => "DeviceRGB", "BitsPerComponent" => 8 },
        rgb,
    ));
    let content = format!("q {} 0 0 {} 0 0 cm /Im0 Do Q", size.width, size.height);
    let contents = doc.add_object(Stream::new(dictionary! {}, content.into_bytes()));
    let pages_id = doc.new_object_id();
    let leaf = doc.add_object(dictionary! {
        "Type" => "Page", "Parent" => pages_id, "MediaBox" => vec![0.into(), 0.into(), Object::Real(size.width), Object::Real(size.height)],
        "Contents" => contents, "Resources" => dictionary! { "XObject" => dictionary! { "Im0" => img } },
    });
    doc.objects.insert(
        pages_id,
        Object::Dictionary(
            dictionary! { "Type" => "Pages", "Kids" => vec![leaf.into()], "Count" => 1 },
        ),
    );
    let catalog = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
    doc.trailer.set("Root", catalog);
    doc.save(out).unwrap();
}

fn words(s: &str) -> std::collections::HashMap<String, usize> {
    let mut m = std::collections::HashMap::new();
    for w in s
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| w.len() > 1)
    {
        *m.entry(w.to_lowercase()).or_insert(0) += 1;
    }
    m
}

/// Word-multiset F1 of two texts.
fn f1(a: &str, b: &str) -> f32 {
    let (wa, wb) = (words(a), words(b));
    let common: usize = wa
        .iter()
        .map(|(k, v)| (*v).min(*wb.get(k).unwrap_or(&0)))
        .sum();
    let (na, nb): (usize, usize) = (wa.values().sum(), wb.values().sum());
    if common == 0 {
        0.0
    } else {
        2.0 * common as f32 / (na + nb) as f32
    }
}

#[tokio::test]
async fn ocr_makes_a_scanned_page_searchable() {
    let tmp = tempfile::tempdir().unwrap();
    let models = root().join(".natives/models");
    let src = root().join(".fixtures/papers/arxiv-2111.12066v2.pdf");
    if pdfium().is_none()
        || !src.exists()
        || !models.join("ppocrv5-rec-en/inference.onnx").exists()
        || ep_document::find_onnxruntime().is_none()
    {
        eprintln!(
            "skipped: PDFium, fixtures, ONNX Runtime or OCR models missing (cargo xtask fetch-models)"
        );
        return;
    }
    let scanned = tmp.path().join("scanned.pdf");
    scanned_copy(&src, 2, &scanned);
    // The end-to-end check of the app (e2e/intel.mjs) opens the same scanned file.
    if let Some(keep) = std::env::var_os("EP_SCANNED_OUT") {
        std::fs::copy(&scanned, keep).unwrap();
    }
    let doc = Document::new(
        Arc::new(OneFile(scanned.clone())),
        pdfium(),
        Some(&tmp.path().join("cache")),
    )
    .unwrap();
    doc.load_page_models(&[models]).unwrap();
    let bus = Arc::new(Bus::default());
    doc.register(&bus).unwrap();
    let bytes = std::fs::read(&scanned).unwrap();
    let file = BlobRef {
        hash: Hash::of(&bytes),
        size: bytes.len() as u64,
        media_type: MediaType::pdf(),
    };

    let before: ep_document_contract::PageText =
        call(&bus, "document.text", json!({ "file": file, "page": 1 }))
            .await
            .unwrap();
    assert!(before.text.trim().is_empty(), "the copy has no text layer");
    let t = Instant::now();
    let prep: Preparation = call(&bus, "document.prepare", json!({ "file": file }))
        .await
        .unwrap();
    let secs = t.elapsed().as_secs_f32();
    assert_eq!(prep.image_only_pages, vec![1]);
    let ocr = prep
        .stages
        .iter()
        .find(|s| s.stage == ep_document_contract::Stage::Ocr)
        .unwrap();
    assert_eq!(ocr.status, StageStatus::Done, "{ocr:?}");

    // The recognised text against the original page's real text.
    let original = {
        let d = Document::new(Arc::new(OneFile(src.clone())), pdfium(), None).unwrap();
        let b = Arc::new(Bus::default());
        d.register(&b).unwrap();
        let ob = std::fs::read(&src).unwrap();
        let of = BlobRef {
            hash: Hash::of(&ob),
            size: ob.len() as u64,
            media_type: MediaType::pdf(),
        };
        call::<ep_document_contract::PageText>(
            &b,
            "document.text",
            json!({ "file": of, "page": 2 }),
        )
        .await
        .unwrap()
    };
    let after: ep_document_contract::PageText =
        call(&bus, "document.text", json!({ "file": file, "page": 1 }))
            .await
            .unwrap();
    let score = f1(&after.text, &original.text);
    eprintln!(
        "OCR of one page: {secs:.1} s, word F1 {score:.3}, {} words\n{}",
        after.words.len(),
        after.text.chars().take(300).collect::<String>()
    );
    assert!(score > 0.85, "word agreement {score}");

    // Search finds a word on the scanned page.
    let found: ep_document_contract::SearchResult = call(
        &bus,
        "document.search",
        json!({ "papers": [file.hash.to_string()], "terms": [{ "text": "controllers" }] }),
    )
    .await
    .unwrap();
    assert!(!found.hits.is_empty(), "search finds recognised words");

    // The text layer written into the PDF is read back by PDFium itself.
    let ocred: ep_document_contract::Ocred = call(&bus, "document.ocr", json!({ "file": file }))
        .await
        .unwrap_or_else(|e| panic!("{e:?}"));
    assert_eq!(ocred.pages, vec![1]);
    let written = scanned.with_extension(format!("{}.pdf", ocred.file.hash));
    assert!(
        std::fs::read(&written).unwrap().starts_with(&bytes),
        "incremental: the scan is kept as it was"
    );
    let d = Document::new(Arc::new(OneFile(written.clone())), pdfium(), None).unwrap();
    let b = Arc::new(Bus::default());
    d.register(&b).unwrap();
    let layer: ep_document_contract::PageText = call(
        &b,
        "document.text",
        json!({ "file": ocred.file, "page": 1 }),
    )
    .await
    .unwrap();
    let back = f1(&layer.text, &after.text);
    eprintln!("text layer read back by PDFium: F1 {back:.3} against the OCR text");
    assert!(back > 0.9, "{back}");
}

fn titles(items: &[ep_document_contract::OutlineItem], out: &mut Vec<String>) {
    for i in items {
        out.push(i.title.clone());
        titles(&i.children, out);
    }
}

/// What the text rules find in one document (no layout model).
struct Found {
    sections: Vec<String>,
    references: usize,
    markers: usize,
    mentions: Vec<String>,
    stages_ok: bool,
}

async fn found(name: &str) -> Option<Found> {
    let tmp = tempfile::tempdir().unwrap();
    let (bus, file, _) = setup_file(
        root().join("fixtures/documents").join(name),
        false,
        tmp.path(),
    )?;
    let info: ep_document_contract::DocInfo = call(&bus, "document.info", json!({ "file": file }))
        .await
        .unwrap();
    let prep: Preparation = call(
        &bus,
        "document.prepare",
        json!({ "file": file, "first_pages": [1] }),
    )
    .await
    .unwrap();
    let contents: Contents = call(&bus, "document.contents", json!({ "file": file }))
        .await
        .unwrap();
    let refs: Vec<Reference> = call(&bus, "document.references", json!({ "file": file }))
        .await
        .unwrap();
    let mut markers = 0;
    let mut mentions = Vec::new();
    for page in 1..=info.pages {
        let m: Vec<CitationMarker> = call(
            &bus,
            "document.citation_markers",
            json!({ "file": file, "page": page }),
        )
        .await
        .unwrap();
        markers += m.len();
        let ms: Vec<Mention> = call(
            &bus,
            "document.mentions",
            json!({ "file": file, "page": page }),
        )
        .await
        .unwrap();
        mentions.extend(ms.into_iter().map(|m| m.text));
    }
    let mut sections = Vec::new();
    titles(&contents.sections, &mut sections);
    let stages_ok = prep.stages.iter().all(|s| s.status != StageStatus::Failed);
    eprintln!(
        "{name}: {} pages · sections {sections:?} · {} figures, {} tables, {} equations · {} references · {markers} citation markers · mentions {mentions:?} · stages {:?}",
        info.pages,
        contents.figures.len(),
        contents.tables.len(),
        contents.equations.len(),
        refs.len(),
        prep.stages
            .iter()
            .map(|s| format!("{:?}={:?}", s.stage, s.status))
            .collect::<Vec<_>>()
    );
    Some(Found {
        sections,
        references: refs.len(),
        markers,
        mentions,
        stages_ok,
    })
}

/// Documents that are not research papers (fixtures/documents): manuals, posters, slides,
/// brochures, reports, forms. Nothing fails, nothing academic is invented (no reference list,
/// no citation markers where there are none), and the structure that is there is found.
#[tokio::test]
async fn non_academic_documents() {
    if pdfium().is_none() {
        eprintln!("skipped: PDFium not fetched");
        return;
    }
    let names = [
        "product-manual.pdf",
        "poster-research.pdf",
        "poster-event.pdf",
        "slides.pdf",
        "brochure.pdf",
        "report-business.pdf",
        "form-irs-w4-2024.pdf",
    ];
    let mut all = Vec::new();
    for n in names {
        all.push((n, found(n).await.expect("document opens")));
    }
    for (n, f) in &all {
        assert!(f.stages_ok, "{n}: a preparation stage failed");
        if *n != "poster-research.pdf" {
            assert_eq!(f.references, 0, "{n}: invented a reference list");
            assert_eq!(f.markers, 0, "{n}: invented citation markers");
        }
    }
    let get = |n: &str| &all.iter().find(|(m, _)| *m == n).unwrap().1;
    // The manual's numbered sections, not its numbered steps.
    let manual = get("product-manual.pdf");
    for want in [
        "Safety instructions",
        "Getting started",
        "Troubleshooting",
        "Technical data",
    ] {
        assert!(
            manual.sections.iter().any(|s| s.contains(want)),
            "manual: missing section {want}: {:?}",
            manual.sections
        );
    }
    assert!(
        !manual
            .sections
            .iter()
            .any(|s| s.contains("Place the grinder")),
        "manual: a step became a section: {:?}",
        manual.sections
    );
    assert!(
        manual.mentions.iter().any(|m| m.contains("Section 4"))
            && manual.mentions.iter().any(|m| m.contains("Table 1")),
        "manual mentions: {:?}",
        manual.mentions
    );
    // Slides: one entry per slide title. Brochure and report: their unnumbered headings; an
    // address is not a section.
    let slides = get("slides.pdf");
    assert!(
        slides.sections.len() >= 6 && slides.sections.iter().any(|s| s == "Roadmap for Q3"),
        "slides: {:?}",
        slides.sections
    );
    let brochure = get("brochure.pdf");
    assert!(
        brochure.sections.iter().any(|s| s == "Plot fees")
            && brochure
                .sections
                .iter()
                .any(|s| s == "Greenway Community Garden")
            && !brochure.sections.iter().any(|s| s.starts_with("12 ")),
        "brochure: {:?}",
        brochure.sections
    );
    let report = get("report-business.pdf");
    assert!(
        report.sections.iter().any(|s| s == "Key figures"),
        "report: {:?}",
        report.sections
    );
    // The research poster keeps its references and markers.
    let poster = get("poster-research.pdf");
    assert_eq!(poster.references, 2, "poster references");
    assert!(poster.markers >= 1, "poster markers");
}
