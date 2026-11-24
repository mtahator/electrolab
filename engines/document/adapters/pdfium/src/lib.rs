//! `PDFium` adapter of the [`PdfBackend`] port (ADR-003, P1.M2.DOC.2).
//!
//! One thread owns `PDFium`; every request travels over a channel and waits with a timeout, so a
//! pathological file returns `Timeout` instead of freezing the caller. Open documents stay in a
//! small least-recently-used cache on that thread. No `PDFium` handle ever leaves it.
#![deny(missing_docs)]

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Condvar, Mutex, mpsc};
use std::time::Duration;

use ep_document_core::pdf::{
    BackendError, Bitmap, PageGeom, PageSize, PdfBackend, RawBookmark, RawChar, RawDest, RawLink,
};
use pdfium_render::prelude::*;

/// Open documents kept on the `PDFium` thread.
const OPEN_DOCS: usize = 4;
/// Longest wait for one request.
const TIMEOUT: Duration = Duration::from_secs(30);
/// Background work waits behind interactive work, so it gets much longer.
const BACKGROUND_TIMEOUT: Duration = Duration::from_secs(600);

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

fn encrypted(doc: &PdfDocument<'static>) -> bool {
    doc.permissions()
        .security_handler_revision()
        .is_ok_and(|r| r != PdfSecurityHandlerRevision::Unprotected)
}

fn rect(r: &PdfRect) -> [f32; 4] {
    [
        r.left().value,
        r.bottom().value,
        r.right().value,
        r.top().value,
    ]
}

/// The shown size, turned back to the page's own directions.
fn unturned(p: &PdfPage<'_>, rotation: u16) -> [f32; 4] {
    let (w, h) = (p.width().value, p.height().value);
    if rotation % 180 == 90 {
        [0.0, 0.0, h, w]
    } else {
        [0.0, 0.0, w, h]
    }
}

fn geom(p: &PdfPage<'_>) -> PageGeom {
    let rotation = match p.rotation() {
        Ok(PdfPageRenderRotation::Degrees90) => 90,
        Ok(PdfPageRenderRotation::Degrees180) => 180,
        Ok(PdfPageRenderRotation::Degrees270) => 270,
        _ => 0,
    };
    // PDFium's own page box: crop box and media box, inherited ones too, before turning. (Boxes
    // read one by one miss the inherited ones, and the page's width and height are turned.)
    let crop = p
        .boundaries()
        .bounding()
        .map_or_else(|_| unturned(p, rotation), |b| rect(&b.bounds));
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

/// The worker's queue: requests for what the user is looking at (interactive) go before
/// background work; within a lane, first come first served.
#[derive(Default)]
struct Lanes {
    interactive: VecDeque<Job>,
    background: VecDeque<Job>,
}

type Queue = Arc<(Mutex<Lanes>, Condvar)>;

/// The `PDFium` backend.
pub struct PdfiumBackend {
    queue: Queue,
}

impl std::fmt::Debug for PdfiumBackend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PdfiumBackend").finish_non_exhaustive()
    }
}

impl PdfiumBackend {
    /// Load `pdfium.dll` (or the platform's library) from `dir` and start the owning thread.
    /// `PDFium` can be started once per process (a second start crashes it), so later calls share
    /// the first worker.
    ///
    /// # Errors
    /// The library cannot be loaded.
    pub fn start(dir: &Path) -> Result<Self, String> {
        static WORKER: std::sync::Mutex<Option<Queue>> = std::sync::Mutex::new(None);
        let mut worker = WORKER.lock().map_err(|e| e.to_string())?;
        if let Some(q) = worker.as_ref() {
            return Ok(Self { queue: q.clone() });
        }
        let started = Self::spawn(dir)?;
        *worker = Some(started.queue.clone());
        Ok(started)
    }

    fn spawn(dir: &Path) -> Result<Self, String> {
        let queue: Queue = Arc::new((Mutex::new(Lanes::default()), Condvar::new()));
        let worker_queue = queue.clone();
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
                let (lock, ready) = &*worker_queue;
                loop {
                    let job = {
                        let mut lanes = lock
                            .lock()
                            .unwrap_or_else(std::sync::PoisonError::into_inner);
                        loop {
                            if let Some(j) = lanes.interactive.pop_front() {
                                break j;
                            }
                            if let Some(j) = lanes.background.pop_front() {
                                break j;
                            }
                            lanes = ready
                                .wait(lanes)
                                .unwrap_or_else(std::sync::PoisonError::into_inner);
                        }
                    };
                    job(&mut state);
                }
            })
            .map_err(|e| e.to_string())?;
        ready_rx.recv().map_err(|e| e.to_string())??;
        Ok(Self { queue })
    }

    fn run<T: Send + 'static>(
        &self,
        f: impl FnOnce(&mut State) -> Result<T, BackendError> + Send + 'static,
    ) -> Result<T, BackendError> {
        let (rtx, rrx) = mpsc::channel();
        let job: Job = Box::new(move |s| {
            let _ = rtx.send(f(s));
        });
        let background = ep_document_core::pdf::is_background();
        {
            let (lock, ready) = &*self.queue;
            let mut lanes = lock
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if background {
                lanes.background.push_back(job);
            } else {
                lanes.interactive.push_back(job);
            }
            ready.notify_one();
        }
        // Background work may wait behind a lot of interactive work; it is not timed out.
        let limit = if background {
            BACKGROUND_TIMEOUT
        } else {
            TIMEOUT
        };
        rrx.recv_timeout(limit).map_err(|_| BackendError::Timeout)?
    }
}

impl PdfiumBackend {
    fn draw(
        &self,
        file: &Path,
        index: u32,
        width_px: u32,
        annotations: bool,
    ) -> Result<Bitmap, BackendError> {
        let file = file.to_path_buf();
        self.run(move |s| {
            let doc = s.doc(&file)?;
            let p = page(doc, index)?;
            let w = i32::try_from(width_px.clamp(16, 8192)).unwrap_or(1024);
            let config = PdfRenderConfig::new()
                .set_target_width(w)
                .set_maximum_height(16384)
                // Annotations are drawn by the viewer from the annotation store (ADR-006), so
                // the page image leaves them out unless asked (printing); form fields stay.
                .render_annotations(annotations)
                .render_form_data(true);
            let bmp = p.render_with_config(&config).map_err(|e| internal(&e))?;
            Ok(Bitmap {
                width: u32::try_from(bmp.width()).unwrap_or(0),
                height: u32::try_from(bmp.height()).unwrap_or(0),
                rgba: bmp.as_rgba_bytes(),
            })
        })
    }
}

impl PdfBackend for PdfiumBackend {
    fn info(&self, file: &Path) -> Result<(Vec<PageGeom>, bool), BackendError> {
        // Loading a page parses its content (about 20 ms on a heavy page), so the pages are read
        // a few at a time: drawing and text jobs for the page in view run in between.
        const CHUNK: u32 = 4;
        let f = file.to_path_buf();
        let (n, encrypted) = self.run(move |s| {
            let doc = s.doc(&f)?;
            Ok((
                u32::try_from(doc.pages().len()).unwrap_or(0),
                encrypted(doc),
            ))
        })?;
        let mut geoms = Vec::with_capacity(n as usize);
        let mut start = 0;
        while start < n {
            let end = (start + CHUNK).min(n);
            let f = file.to_path_buf();
            let part = self.run(move |s| {
                let doc = s.doc(&f)?;
                (start..end)
                    .map(|i| page(doc, i).map(|p| geom(&p)))
                    .collect::<Result<Vec<_>, _>>()
            })?;
            geoms.extend(part);
            start = end;
        }
        Ok((geoms, encrypted))
    }

    fn sizes(&self, file: &Path) -> Result<(Vec<PageSize>, bool), BackendError> {
        let file = file.to_path_buf();
        self.run(move |s| {
            let doc = s.doc(&file)?;
            // PDFium reads sizes from the page dictionaries without parsing any content.
            let sizes = doc
                .pages()
                .page_sizes()
                .map_err(|e| internal(&e))?
                .iter()
                .map(|r| PageSize {
                    width: r.width().value,
                    height: r.height().value,
                })
                .collect();
            Ok((sizes, encrypted(doc)))
        })
    }

    fn page_geom(&self, file: &Path, index: u32) -> Result<PageGeom, BackendError> {
        let file = file.to_path_buf();
        self.run(move |s| {
            let doc = s.doc(&file)?;
            page(doc, index).map(|p| geom(&p))
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
        self.draw(file, index, width_px, false)
    }

    fn render_annotated(
        &self,
        file: &Path,
        index: u32,
        width_px: u32,
    ) -> Result<Bitmap, BackendError> {
        self.draw(file, index, width_px, true)
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
