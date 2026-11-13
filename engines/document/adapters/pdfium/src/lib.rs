//! `PDFium` adapter of the [`PdfBackend`] port (ADR-003, P1.M2.DOC.2).
//!
//! One thread owns `PDFium`; every request travels over a channel and waits with a timeout, so a
//! pathological file returns `Timeout` instead of freezing the caller. Open documents stay in a
//! small least-recently-used cache on that thread. No `PDFium` handle ever leaves it.
#![deny(missing_docs)]

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::Duration;

use ep_document_core::pdf::{
    BackendError, Bitmap, PageGeom, PdfBackend, RawBookmark, RawChar, RawDest, RawLink,
};
use pdfium_render::prelude::*;

/// Open documents kept on the `PDFium` thread.
const OPEN_DOCS: usize = 4;
/// Longest wait for one request.
const TIMEOUT: Duration = Duration::from_secs(30);

type Job = Box<dyn FnOnce(&mut State) + Send>;

struct State {
    pdfium: &'static Pdfium,
    docs: VecDeque<(PathBuf, PdfDocument<'static>)>,
}

fn internal(e: &PdfiumError) -> BackendError {
    match e {
        PdfiumError::PdfiumLibraryInternalError(PdfiumInternalError::PasswordError) => {
            BackendError::PasswordRequired
        }
        other => BackendError::Unreadable(format!("{other:?}")),
    }
}

impl State {
    /// The open document for `path`, opening it (and evicting the oldest) if needed.
    fn doc(&mut self, path: &Path) -> Result<&PdfDocument<'static>, BackendError> {
        if let Some(i) = self.docs.iter().position(|(p, _)| p == path) {
            let entry = self
                .docs
                .remove(i)
                .ok_or(BackendError::Unreadable("cache".into()))?;
            self.docs.push_front(entry);
        } else {
            let d = self
                .pdfium
                .load_pdf_from_file(path, None)
                .map_err(|e| internal(&e))?;
            if self.docs.len() >= OPEN_DOCS {
                self.docs.pop_back();
            }
            self.docs.push_front((path.to_path_buf(), d));
        }
        self.docs
            .front()
            .map(|(_, d)| d)
            .ok_or(BackendError::Unreadable("cache".into()))
    }
}

fn page<'a>(doc: &'a PdfDocument<'static>, index: u32) -> Result<PdfPage<'a>, BackendError> {
    let n = doc.pages().len();
    let i = i32::try_from(index).map_err(|_| BackendError::BadPage(index))?;
    if i >= n {
        return Err(BackendError::BadPage(index));
    }
    doc.pages().get(i).map_err(|e| internal(&e))
}

fn rect(r: &PdfRect) -> [f32; 4] {
    [
        r.left().value,
        r.bottom().value,
        r.right().value,
        r.top().value,
    ]
}

fn geom(p: &PdfPage<'_>) -> PageGeom {
    let rotation = match p.rotation() {
        Ok(PdfPageRenderRotation::Degrees90) => 90,
        Ok(PdfPageRenderRotation::Degrees180) => 180,
        Ok(PdfPageRenderRotation::Degrees270) => 270,
        _ => 0,
    };
    let bounds = p
        .boundaries()
        .crop()
        .or_else(|_| p.boundaries().media())
        .map(|b| rect(&b.bounds));
    let crop = bounds.unwrap_or([0.0, 0.0, p.width().value, p.height().value]);
    PageGeom { rotation, crop }
}

fn dest(d: &PdfDestination<'_>) -> RawDest {
    let Ok(index) = d.page_index() else {
        return RawDest::Other;
    };
    let y = match d.view_settings() {
        Ok(
            PdfDestinationViewSettings::SpecificCoordinatesAndZoom(_, y, _)
            | PdfDestinationViewSettings::FitPageHorizontallyToWindow(y),
        ) => y.map(|v| v.value),
        Ok(PdfDestinationViewSettings::FitPageToRectangle(r)) => Some(r.top().value),
        _ => None,
    };
    u32::try_from(index).map_or(RawDest::Other, |index| RawDest::Page { index, y })
}

fn action_dest(a: &PdfAction<'_>) -> RawDest {
    if let Some(u) = a.as_uri_action() {
        return u.uri().map_or(RawDest::Other, RawDest::Uri);
    }
    if let Some(l) = a.as_local_destination_action() {
        return l.destination().map_or(RawDest::Other, |d| dest(&d));
    }
    RawDest::Other
}

fn bookmarks(first: Option<PdfBookmark<'_>>, depth: usize) -> Vec<RawBookmark> {
    let mut out = Vec::new();
    let Some(first) = first else { return out };
    // Damaged outlines can loop; stop at a sane size and depth.
    for b in first.iter_siblings().take(2000) {
        let dest = b
            .destination()
            .map(|d| dest(&d))
            .or_else(|| b.action().map(|a| action_dest(&a)));
        let children = if depth < 16 {
            bookmarks(b.first_child(), depth + 1)
        } else {
            Vec::new()
        };
        out.push(RawBookmark {
            title: b.title().unwrap_or_default(),
            dest,
            children,
        });
    }
    out
}

/// The `PDFium` backend.
#[derive(Debug)]
pub struct PdfiumBackend {
    tx: mpsc::Sender<Job>,
}

impl PdfiumBackend {
    /// Load `pdfium.dll` (or the platform's library) from `dir` and start the owning thread.
    ///
    /// # Errors
    /// The library cannot be loaded.
    pub fn start(dir: &Path) -> Result<Self, String> {
        let (tx, rx) = mpsc::channel::<Job>();
        let (ready_tx, ready_rx) = mpsc::channel::<Result<(), String>>();
        let lib = Pdfium::pdfium_platform_library_name_at_path(dir);
        std::thread::Builder::new()
            .name("pdfium".into())
            .spawn(move || {
                let bindings = match Pdfium::bind_to_library(&lib) {
                    Ok(b) => b,
                    Err(e) => {
                        let _ = ready_tx.send(Err(format!("cannot load {}: {e:?}", lib.display())));
                        return;
                    }
                };
                // One PDFium per process, alive until the process ends.
                let pdfium: &'static Pdfium = Box::leak(Box::new(Pdfium::new(bindings)));
                let _ = ready_tx.send(Ok(()));
                let mut state = State {
                    pdfium,
                    docs: VecDeque::new(),
                };
                while let Ok(job) = rx.recv() {
                    job(&mut state);
                }
            })
            .map_err(|e| e.to_string())?;
        ready_rx.recv().map_err(|e| e.to_string())??;
        Ok(Self { tx })
    }

    fn run<T: Send + 'static>(
        &self,
        f: impl FnOnce(&mut State) -> Result<T, BackendError> + Send + 'static,
    ) -> Result<T, BackendError> {
        let (rtx, rrx) = mpsc::channel();
        let job: Job = Box::new(move |s| {
            let _ = rtx.send(f(s));
        });
        self.tx
            .send(job)
            .map_err(|_| BackendError::Unreadable("PDF backend stopped".into()))?;
        rrx.recv_timeout(TIMEOUT)
            .map_err(|_| BackendError::Timeout)?
    }
}

impl PdfBackend for PdfiumBackend {
    fn info(&self, file: &Path) -> Result<(Vec<PageGeom>, bool), BackendError> {
        let file = file.to_path_buf();
        self.run(move |s| {
            let doc = s.doc(&file)?;
            let encrypted = doc
                .permissions()
                .security_handler_revision()
                .is_ok_and(|r| r != PdfSecurityHandlerRevision::Unprotected);
            let mut geoms = Vec::new();
            for p in doc.pages().iter() {
                geoms.push(geom(&p));
            }
            Ok((geoms, encrypted))
        })
    }

    fn chars(&self, file: &Path, index: u32) -> Result<Vec<RawChar>, BackendError> {
        let file = file.to_path_buf();
        self.run(move |s| {
            let doc = s.doc(&file)?;
            let p = page(doc, index)?;
            let text = p.text().map_err(|e| internal(&e))?;
            Ok(text
                .chars()
                .iter()
                .map(|c| {
                    let bounds = c
                        .loose_bounds()
                        .ok()
                        .map(|r| rect(&r))
                        .filter(|b| b[2] > b[0] || b[3] > b[1]);
                    RawChar {
                        unit: c.unicode_value(),
                        bounds,
                    }
                })
                .collect())
        })
    }

    fn render(&self, file: &Path, index: u32, width_px: u32) -> Result<Bitmap, BackendError> {
        let file = file.to_path_buf();
        self.run(move |s| {
            let doc = s.doc(&file)?;
            let p = page(doc, index)?;
            let w = i32::try_from(width_px.clamp(16, 8192)).unwrap_or(1024);
            let config = PdfRenderConfig::new()
                .set_target_width(w)
                .set_maximum_height(16384)
                .render_form_data(true);
            let bmp = p.render_with_config(&config).map_err(|e| internal(&e))?;
            Ok(Bitmap {
                width: u32::try_from(bmp.width()).unwrap_or(0),
                height: u32::try_from(bmp.height()).unwrap_or(0),
                rgba: bmp.as_rgba_bytes(),
            })
        })
    }

    fn outline(&self, file: &Path) -> Result<Vec<RawBookmark>, BackendError> {
        let file = file.to_path_buf();
        self.run(move |s| {
            let doc = s.doc(&file)?;
            Ok(bookmarks(doc.bookmarks().root(), 0))
        })
    }

    fn links(&self, file: &Path, index: u32) -> Result<Vec<RawLink>, BackendError> {
        let file = file.to_path_buf();
        self.run(move |s| {
            let doc = s.doc(&file)?;
            let p = page(doc, index)?;
            Ok(p.links()
                .iter()
                .filter_map(|l| {
                    let r = rect(&l.rect().ok()?);
                    let d = l
                        .destination()
                        .map(|d| dest(&d))
                        .or_else(|| l.action().map(|a| action_dest(&a)))
                        .unwrap_or(RawDest::Other);
                    Some(RawLink { rect: r, dest: d })
                })
                .collect())
        })
    }
}
