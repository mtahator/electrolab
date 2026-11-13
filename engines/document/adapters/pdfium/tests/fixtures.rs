//! The PDFium adapter on the fixture papers. Needs `cargo xtask fetch-natives` (PDFium) and,
//! for the full set, `cargo xtask fetch-fixtures`; missing pieces are skipped with a note.
#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)] // tests and their helpers stop loudly

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use ep_document_adapter_pdfium::PdfiumBackend;
use ep_document_core::pdf::{BackendError, PdfBackend, outline, page_text, target};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../..")
}

fn backend() -> Option<PdfiumBackend> {
    let dir = root().join(".natives/pdfium/bin");
    if !dir.exists() {
        eprintln!("skipped: PDFium not fetched (cargo xtask fetch-natives)");
        return None;
    }
    Some(PdfiumBackend::start(&dir).expect("load pdfium"))
}

fn papers() -> Vec<PathBuf> {
    let mut v = vec![root().join("fixtures/papers/fawiki-artificial-neural-network.pdf")];
    if let Ok(rd) = std::fs::read_dir(root().join(".fixtures/papers")) {
        let mut more: Vec<PathBuf> = rd
            .filter_map(|e| Some(e.ok()?.path()))
            .filter(|p| p.extension().is_some_and(|x| x == "pdf"))
            .collect();
        more.sort();
        v.extend(more);
    }
    v
}

#[test]
fn every_fixture_reads() {
    let Some(b) = backend() else { return };
    for paper in papers() {
        let t = Instant::now();
        let (geoms, _enc) = b
            .info(&paper)
            .unwrap_or_else(|e| panic!("{}: {e}", paper.display()));
        assert!(!geoms.is_empty(), "{}", paper.display());
        let text = page_text(1, &geoms[0], &b.chars(&paper, 0).expect("chars"));
        assert_eq!(text.boxes.len(), text.text.chars().count());
        let size = geoms[0].size();
        for w in &text.words {
            assert!(
                w.rect.x0 >= -2.0
                    && w.rect.x1 <= size.width + 2.0
                    && w.rect.y0 >= -2.0
                    && w.rect.y1 <= size.height + 2.0,
                "{}: word box outside the page: {:?}",
                paper.display(),
                w.rect
            );
        }
        let bmp = b.render(&paper, 0, 400).expect("render");
        assert_eq!(bmp.width, 400);
        assert_eq!(bmp.rgba.len(), (bmp.width * bmp.height * 4) as usize);
        let ol = outline(&b.outline(&paper).expect("outline"), &geoms);
        let mut links = 0;
        for i in 0..u32::try_from(geoms.len()).unwrap() {
            for l in b.links(&paper, i).expect("links") {
                links += 1;
                let _ = target(&l.dest, &geoms);
            }
        }
        eprintln!(
            "{:>40}  pages {:>3}  words p1 {:>4}  outline {:>3}  links {:>4}  {:?}",
            paper.file_name().unwrap().to_string_lossy(),
            geoms.len(),
            text.words.len(),
            ol.len(),
            links,
            t.elapsed()
        );
    }
}

#[test]
fn bad_input_is_typed_not_a_crash() {
    let Some(b) = backend() else { return };
    let paper = &papers()[0];
    assert_eq!(b.chars(paper, 10_000), Err(BackendError::BadPage(10_000)));
    let d = std::env::temp_dir().join(format!("ep-bad-{}.pdf", std::process::id()));
    std::fs::write(&d, b"%PDF-1.7\nthis is not a pdf").unwrap();
    assert!(matches!(b.info(&d), Err(BackendError::Unreadable(_))));
    let _ = std::fs::remove_file(d);
}

#[test]
fn hundred_mixed_requests_from_threads() {
    let Some(b) = backend() else { return };
    let b = Arc::new(b);
    let all = papers();
    let hs: Vec<_> = (0..10)
        .map(|t| {
            let (b, all) = (b.clone(), all.clone());
            std::thread::spawn(move || {
                for k in 0..10 {
                    let p = &all[(t + k) % all.len()];
                    match k % 4 {
                        0 => drop(b.info(p).expect("info")),
                        1 => drop(b.chars(p, 0).expect("chars")),
                        2 => drop(b.render(p, 0, 200).expect("render")),
                        _ => drop(b.outline(p).expect("outline")),
                    }
                }
            })
        })
        .collect();
    for h in hs {
        h.join().expect("thread");
    }
}
