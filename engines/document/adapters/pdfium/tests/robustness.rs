//! Malformed and unusual PDFs (P1.M2.DOC.8): typed errors, no panic, no hang. The derived files
//! are made on the fly from the committed fixture (nothing binary is added to the repository).
#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)] // tests and their helpers stop loudly

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use ep_document_adapter_pdfium::PdfiumBackend;
use ep_document_core::pdf::{BackendError, PdfBackend, page_text};
use lopdf::encryption::{EncryptionState, EncryptionVersion, Permissions};
use lopdf::{Document, Object, dictionary};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../..")
}

fn fixture() -> PathBuf {
    root().join("fixtures/papers/fawiki-artificial-neural-network.pdf")
}

fn backend() -> Option<PdfiumBackend> {
    let dir = root().join(".natives/pdfium/bin");
    if !dir.exists() {
        eprintln!("skipped: PDFium not fetched (cargo xtask fetch-natives pdfium)");
        return None;
    }
    Some(PdfiumBackend::start(&dir).expect("load pdfium"))
}

/// Run `f` and require it to finish quickly (no hang).
fn quick<T>(f: impl FnOnce() -> T) -> T {
    let t = Instant::now();
    let r = f();
    assert!(
        t.elapsed() < Duration::from_secs(10),
        "took {:?}",
        t.elapsed()
    );
    r
}

#[test]
fn damaged_and_empty_files_give_typed_errors() {
    let Some(b) = backend() else { return };
    let d = tempfile::tempdir().unwrap();
    let bytes = std::fs::read(fixture()).unwrap();
    let cases: [(&str, Vec<u8>); 4] = [
        ("empty.pdf", Vec::new()),
        (
            "garbage.pdf",
            b"%PDF-1.7\n\x00\x01\x02 not really a pdf \xff\xfe".to_vec(),
        ),
        ("truncated.pdf", bytes[..bytes.len() / 2].to_vec()),
        ("no-header.pdf", bytes[1024..].to_vec()),
    ];
    for (name, data) in cases {
        let p = d.path().join(name);
        std::fs::write(&p, data).unwrap();
        // PDFium repairs some damage; either it opens, or the error is typed. It never panics.
        match quick(|| b.info(&p)) {
            Ok((geoms, _)) => {
                for i in 0..u32::try_from(geoms.len()).unwrap() {
                    let _ = quick(|| b.chars(&p, i));
                }
            }
            Err(BackendError::Unreadable(_)) => {}
            Err(other) => panic!("{name}: unexpected {other:?}"),
        }
    }
}

#[test]
fn password_protected_pdf_asks_for_the_password() {
    let Some(b) = backend() else { return };
    let d = tempfile::tempdir().unwrap();
    let mut doc = Document::load(fixture()).unwrap();
    if doc.trailer.get(b"ID").is_err() {
        let id = Object::String(
            b"0123456789abcdef".to_vec(),
            lopdf::StringFormat::Hexadecimal,
        );
        doc.trailer.set("ID", Object::Array(vec![id.clone(), id]));
    }
    let state = EncryptionState::try_from(EncryptionVersion::V2 {
        document: &doc,
        owner_password: "owner",
        user_password: "secret",
        key_length: 128,
        permissions: Permissions::all(),
    })
    .unwrap();
    doc.encrypt(&state).unwrap();
    let p = d.path().join("locked.pdf");
    doc.save(&p).unwrap();
    assert_eq!(quick(|| b.info(&p)), Err(BackendError::PasswordRequired));
}

#[test]
fn zero_page_document() {
    let Some(b) = backend() else { return };
    let d = tempfile::tempdir().unwrap();
    let mut doc = Document::with_version("1.7");
    let pages = doc.add_object(
        dictionary! { "Type" => "Pages", "Kids" => Vec::<Object>::new(), "Count" => 0 },
    );
    let catalog = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages });
    doc.trailer.set("Root", catalog);
    let p = d.path().join("zero.pdf");
    doc.save(&p).unwrap();
    match quick(|| b.info(&p)) {
        Ok((geoms, _)) => {
            assert_eq!(geoms.len(), 0);
            assert_eq!(b.chars(&p, 0), Err(BackendError::BadPage(0)));
            assert_eq!(b.render(&p, 0, 100).err(), Some(BackendError::BadPage(0)));
        }
        Err(e) => assert!(matches!(e, BackendError::Unreadable(_)), "{e:?}"),
    }
}

/// Copy the fixture with page 1 changed by `edit`.
fn edited(d: &Path, name: &str, edit: impl FnOnce(&mut lopdf::Dictionary)) -> PathBuf {
    let mut doc = Document::load(fixture()).unwrap();
    let first = *doc.get_pages().get(&1).unwrap();
    edit(doc.get_object_mut(first).unwrap().as_dict_mut().unwrap());
    let p = d.join(name);
    doc.save(&p).unwrap();
    p
}

/// Two ways to a page's size, and the drawing: one answer, for every kind of page box.
#[test]
fn quick_sizes_and_full_geometry_agree() {
    let Some(b) = backend() else { return };
    let dir = root().join("fixtures/documents");
    let mut pages = 0;
    for e in std::fs::read_dir(dir).unwrap().flatten() {
        let path = e.path();
        if path.extension().is_none_or(|x| x != "pdf") || path.ends_with("book-800.pdf") {
            continue;
        }
        let name = path.display().to_string();
        let (quick, _) = b.sizes(&path).unwrap();
        let (full, _) = b.info(&path).unwrap();
        for (i, (q, g)) in quick.iter().zip(&full).enumerate() {
            let f = g.size();
            assert!(
                (q.width - f.width).abs() < 0.1 && (q.height - f.height).abs() < 0.1,
                "{name} page {}: quick {q:?}, full {f:?}",
                i + 1
            );
            let index = u32::try_from(i).unwrap();
            let img = b.render(&path, index, 400).unwrap();
            let shape = f64::from(img.height) / f64::from(img.width);
            let want = f64::from(f.height) / f64::from(f.width);
            assert!(
                (shape - want).abs() < 0.02,
                "{name} page {}: drawing {shape}, page {want}",
                i + 1
            );
            // Words inside the page (an older fixture really draws some outside its page).
            if !path.ends_with("mixed-pages.pdf") {
                pages += 1;
                continue;
            }
            for w in page_text(index + 1, g, &b.chars(&path, index).unwrap()).words {
                assert!(
                    w.rect.x0 >= -1.0
                        && w.rect.x1 <= f.width + 1.0
                        && w.rect.y0 >= -1.0
                        && w.rect.y1 <= f.height + 1.0,
                    "{name} page {}: word {:?} outside {f:?}",
                    i + 1,
                    w.rect
                );
            }
            pages += 1;
        }
    }
    assert!(pages > 15, "only {pages} pages checked");
}

#[test]
fn rotated_page_geometry_matches_the_rendering() {
    let Some(b) = backend() else { return };
    let d = tempfile::tempdir().unwrap();
    let plain = fixture();
    let rotated = edited(d.path(), "rotated.pdf", |page| page.set("Rotate", 90));
    let (g0, _) = b.info(&plain).unwrap();
    let (g1, _) = b.info(&rotated).unwrap();
    assert_eq!(g1[0].rotation, 90);
    let (s0, s1) = (g0[0].size(), g1[0].size());
    assert!(
        (s0.width - s1.height).abs() < 0.5 && (s0.height - s1.width).abs() < 0.5,
        "{s0:?} vs {s1:?}"
    );
    // PDFium applies /Rotate when rendering: the image has the rotated shape.
    let img = b.render(&rotated, 0, 800).unwrap();
    let ratio = f64::from(img.height) / f64::from(img.width);
    let want = f64::from(s1.height) / f64::from(s1.width);
    assert!(
        (ratio - want).abs() < 0.01,
        "image {}x{} vs page {s1:?}",
        img.width,
        img.height
    );
    // Word boxes are in the rotated page space: inside the page, and the text runs downward.
    let t = page_text(1, &g1[0], &b.chars(&rotated, 0).unwrap());
    for w in &t.words {
        assert!(
            w.rect.x0 >= -1.0
                && w.rect.x1 <= s1.width + 1.0
                && w.rect.y0 >= -1.0
                && w.rect.y1 <= s1.height + 1.0,
            "{:?}",
            w.rect
        );
    }
    let first = &t.words[0].rect;
    assert!(
        first.y1 - first.y0 > first.x1 - first.x0 || t.words[0].end - t.words[0].start <= 1,
        "rotated words are tall: {first:?}"
    );
}

#[test]
fn huge_page_renders_within_limits() {
    let Some(b) = backend() else { return };
    let d = tempfile::tempdir().unwrap();
    let huge = edited(d.path(), "huge.pdf", |page| {
        page.set(
            "MediaBox",
            vec![0.into(), 0.into(), 14_400.into(), 14_400.into()],
        );
        page.remove(b"CropBox");
    });
    let (g, _) = b.info(&huge).unwrap();
    assert!((g[0].size().width - 14_400.0).abs() < 1.0);
    let img = quick(|| b.render(&huge, 0, 1200)).unwrap();
    assert_eq!((img.width, img.height), (1200, 1200));
    let _ = quick(|| b.chars(&huge, 0)).unwrap();
}
