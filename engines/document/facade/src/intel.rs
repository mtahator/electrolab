//! Document intelligence commands (P1.M4): background preparation, objects, region lookup,
//! contents, mentions, citation markers, references, equations and tables.
//!
//! Results are derived data: per-page objects go to the result cache (keyed by file, page, rules
//! version and layout model), so preparation resumes where it stopped and costs nothing twice.
//! Paper-wide analysis (headings, reference list) is recomputed from the text and kept in memory.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};

use ep_bus::cache::cache_key;
use ep_bus::{CommandError, Cx};
use ep_document_contract::{
    CitationMarker, Contents, DocObject, Equation, EquationRequest, FileOnly, Mention, MentionKind,
    ModelInfo, ObjectKind, ObjectRef, ObjectsRequest, PageObjects, PageText, Preparation,
    PrepareRequest, Reference, RegionHit, RegionRequest, Stage, StageState, StageStatus, Table,
    TableRequest, errors, intel_errors, intel_events,
};
use ep_document_core::intel::headings::{self, Heading};
use ep_document_core::intel::{
    self as core, LayoutModel, Line, layout, mentions, references, tables,
};
use ep_document_core::pdf::{Bitmap, PageGeom};
use ep_types::{BlobRef, Hash, MediaType, PageRect};
use serde_json::json;

use crate::models::{self, LAYOUT};
use crate::{Document, backend_err, blocking, png};

/// Version of the rules that shape objects; bump to recompute cached objects.
const INTEL_IMPL: &str = "intel-1";
/// Papers whose analysis stays in memory.
const ANALYSES: usize = 4;
/// Width the page is rendered at for the layout model.
const LAYOUT_RENDER_W: u32 = 800;
/// A page with fewer characters than this has no usable text layer.
const MIN_CHARS: usize = 20;

/// The paper-wide analysis: every page's text and lines, headings, reference list.
pub(crate) struct Analysis {
    pages: Vec<PageText>,
    lines: Vec<Vec<Line>>,
    heads: Vec<Heading>,
    refs: Vec<Reference>,
    image_only: Vec<u32>,
}

/// Page-model state of the engine.
#[derive(Default)]
pub(crate) struct Models {
    dir: Option<PathBuf>,
    layout: Option<Arc<dyn LayoutModel>>,
}

static RUNTIME: OnceLock<Result<ep_document_adapter_onnx::Runtime, String>> = OnceLock::new();

fn no_object(what: impl std::fmt::Display) -> CommandError {
    CommandError::failed(intel_errors::NO_OBJECT, what)
}

fn crop(bmp: &Bitmap, x0: u32, y0: u32, x1: u32, y1: u32) -> Bitmap {
    let (x1, y1) = (x1.min(bmp.width), y1.min(bmp.height));
    let (w, h) = (x1.saturating_sub(x0).max(1), y1.saturating_sub(y0).max(1));
    let mut rgba = Vec::with_capacity((w * h * 4) as usize);
    for y in y0..y0 + h {
        let row = ((y.min(bmp.height - 1) * bmp.width + x0) * 4) as usize;
        let end = (row + (w * 4) as usize).min(bmp.rgba.len());
        rgba.extend_from_slice(&bmp.rgba[row..end]);
    }
    rgba.resize((w * h * 4) as usize, 255);
    Bitmap {
        width: w,
        height: h,
        rgba,
    }
}

impl Document {
    /// Where page models are kept, and load the layout model when it is there. Hosts call this
    /// at start-up and again after a model was downloaded.
    ///
    /// # Errors
    /// ONNX Runtime or a present model could not be loaded (the engine keeps working without).
    pub fn load_page_models(&self, dir: &Path) -> Result<(), String> {
        let mut m = self.models.write();
        m.dir = Some(dir.to_path_buf());
        if !LAYOUT.present(Some(dir)) {
            m.layout = None;
            return Ok(());
        }
        let runtime = RUNTIME
            .get_or_init(|| {
                let lib = models::find_onnxruntime()
                    .ok_or_else(|| "ONNX Runtime is not installed".to_string())?;
                ep_document_adapter_onnx::Runtime::load(&lib)
            })
            .as_ref()
            .map_err(Clone::clone)?;
        let layout = runtime.layout(&LAYOUT.path(dir))?;
        m.layout = Some(Arc::new(layout));
        Ok(())
    }

    /// The model specs (hosts download from these).
    #[must_use]
    pub fn model_specs() -> &'static [models::ModelSpec] {
        models::ALL
    }

    fn layout_model(&self) -> Option<Arc<dyn LayoutModel>> {
        self.models.read().layout.clone()
    }

    /// The paper-wide analysis of a file (cached in memory).
    fn analysis(&self, file: &BlobRef, path: &Path) -> Result<Arc<Analysis>, CommandError> {
        if let Some((_, a)) = self.analyses.lock().iter().find(|(h, _)| *h == file.hash) {
            return Ok(a.clone());
        }
        let (g, _) = self.geoms(file.hash, path)?;
        let backend = self.backend()?;
        let mut pages = Vec::with_capacity(g.len());
        for (i, geom) in g.iter().enumerate() {
            let idx = u32::try_from(i).unwrap_or(u32::MAX);
            let raw = backend.chars(path, idx).map_err(backend_err)?;
            pages.push(ep_document_core::pdf::page_text(idx + 1, geom, &raw));
        }
        let lines: Vec<Vec<Line>> = pages.iter().map(core::lines).collect();
        let heads = headings::headings(&lines);
        let refs = references::references(&lines);
        let image_only = pages
            .iter()
            .filter(|p| p.text.chars().filter(|c| !c.is_whitespace()).count() < MIN_CHARS)
            .map(|p| p.page)
            .collect();
        let a = Arc::new(Analysis {
            pages,
            lines,
            heads,
            refs,
            image_only,
        });
        let mut cache = self.analyses.lock();
        if cache.len() >= ANALYSES {
            cache.pop_back();
        }
        cache.push_front((file.hash, a.clone()));
        Ok(a)
    }

    fn objects_key(file: &BlobRef, page: u32, model: &str) -> Hash {
        cache_key(
            "document.objects",
            INTEL_IMPL,
            &json!({ "file": file.hash, "page": page, "model": model }),
        )
    }

    fn cached_objects(&self, file: &BlobRef, page: u32, model: &str) -> Option<PageObjects> {
        let (_, index) = self.renders.as_ref()?;
        index
            .get(&Self::objects_key(file, page, model))
            .and_then(|v| serde_json::from_value(v).ok())
    }

    /// Objects of a page: cached, else computed (with the layout model when `use_model` and it
    /// is loaded; from the text alone otherwise).
    fn page_objects(
        &self,
        file: &BlobRef,
        path: &Path,
        page: u32,
        use_model: bool,
    ) -> Result<PageObjects, CommandError> {
        let model = if use_model { self.layout_model() } else { None };
        let model_id = if model.is_some() { LAYOUT.id } else { "none" };
        if let Some(o) = self.cached_objects(file, page, model_id) {
            return Ok(o);
        }
        // A page done with the model earlier is better than text alone.
        if model.is_none()
            && let Some(o) = self.cached_objects(file, page, LAYOUT.id)
        {
            return Ok(o);
        }
        let a = self.analysis(file, path)?;
        let (g, _) = self.geoms(file.hash, path)?;
        let idx = Self::page_index(&g, page)?;
        let pt = &a.pages[idx as usize];
        let lines = &a.lines[idx as usize];
        let dets = match &model {
            Some(m) => {
                let size = g[idx as usize].size();
                let bmp = self
                    .backend()?
                    .render(path, idx, LAYOUT_RENDER_W)
                    .map_err(backend_err)?;
                let raw = m
                    .detect(&bmp)
                    .map_err(|e| CommandError::failed("model", e))?;
                core::to_page(page, size.width, size.height, &raw)
            }
            None => Vec::new(),
        };
        let objects = layout::objects(page, dets, pt, lines, &a.heads);
        let out = PageObjects {
            page,
            objects,
            detected: model.is_some(),
        };
        if let Some((_, index)) = self.renders.as_ref() {
            let _ = index.put(
                &Self::objects_key(file, page, model_id),
                &serde_json::to_value(&out).unwrap_or_default(),
            );
        }
        Ok(out)
    }

    fn preparation_state(&self, file: &BlobRef, a: Option<&Analysis>, pages: u32) -> Preparation {
        let has_model = self.layout_model().is_some();
        let done_objects = (1..=pages)
            .filter(|&p| self.cached_objects(file, p, LAYOUT.id).is_some())
            .count();
        let all_done = u32::try_from(done_objects).is_ok_and(|d| d == pages);
        let done = |b: bool| {
            if b {
                StageStatus::Done
            } else {
                StageStatus::Pending
            }
        };
        let st = |stage, status, done: usize, note: Option<&str>| StageState {
            stage,
            status,
            done: u32::try_from(done).unwrap_or(0),
            total: pages,
            note: note.map(str::to_string),
        };
        let analysed = a.is_some();
        let image_only = a.map(|a| a.image_only.clone()).unwrap_or_default();
        Preparation {
            file: file.clone(),
            stages: vec![
                st(
                    Stage::Text,
                    done(analysed),
                    if analysed { pages as usize } else { 0 },
                    None,
                ),
                st(
                    Stage::Outline,
                    done(analysed),
                    if analysed { pages as usize } else { 0 },
                    None,
                ),
                if has_model || done_objects > 0 {
                    st(Stage::Objects, done(all_done), done_objects, None)
                } else {
                    st(
                        Stage::Objects,
                        StageStatus::Skipped,
                        0,
                        Some("The layout model is not downloaded."),
                    )
                },
                if image_only.is_empty() {
                    st(Stage::Ocr, done(analysed), 0, None)
                } else {
                    st(
                        Stage::Ocr,
                        StageStatus::Skipped,
                        0,
                        Some("Text recognition for scanned pages arrives in a later update."),
                    )
                },
                st(
                    Stage::Links,
                    done(analysed),
                    if analysed { pages as usize } else { 0 },
                    None,
                ),
            ],
            image_only_pages: image_only,
        }
    }

    pub(crate) async fn prepare(
        self: Arc<Self>,
        cx: Cx,
        i: PrepareRequest,
    ) -> Result<Preparation, CommandError> {
        let path = self.path(&i.file)?;
        let me = self.clone();
        blocking(move || {
            let (g, _) = me.geoms(i.file.hash, &path)?;
            let pages = u32::try_from(g.len()).unwrap_or(0);
            // One run per file; a second caller gets the state so far.
            if !me.preparing.lock().insert(i.file.hash) {
                let a = me
                    .analyses
                    .lock()
                    .iter()
                    .find(|(h, _)| *h == i.file.hash)
                    .map(|(_, a)| a.clone());
                return Ok(me.preparation_state(&i.file, a.as_deref(), pages));
            }
            let result = me.run_preparation(&cx, &i, &path, pages);
            me.preparing.lock().remove(&i.file.hash);
            result
        })
        .await
    }

    fn report(&self, cx: &Cx, file: &BlobRef, s: &StageState) {
        cx.progress(s);
        if let Some(bus) = self.bus.lock().upgrade() {
            bus.events().publish(
                intel_events::PREPARING,
                json!({ "file": file, "stage": s }),
                cx.clock.now_ms(),
            );
        }
    }

    fn run_preparation(
        &self,
        cx: &Cx,
        i: &PrepareRequest,
        path: &Path,
        pages: u32,
    ) -> Result<Preparation, CommandError> {
        let state = |s: Stage, status: StageStatus, done: u32| StageState {
            stage: s,
            status,
            done,
            total: pages,
            note: None,
        };
        self.report(cx, &i.file, &state(Stage::Text, StageStatus::Running, 0));
        let a = self.analysis(&i.file, path)?;
        self.report(cx, &i.file, &state(Stage::Text, StageStatus::Done, pages));
        self.report(
            cx,
            &i.file,
            &state(Stage::Outline, StageStatus::Done, pages),
        );
        // Objects: pages in view first, then the rest; each page is stored as it finishes.
        if self.layout_model().is_some() {
            let mut order: Vec<u32> = i
                .first_pages
                .iter()
                .copied()
                .filter(|p| *p >= 1 && *p <= pages)
                .collect();
            order.extend((1..=pages).filter(|p| !i.first_pages.contains(p)));
            let mut done = u32::try_from(
                (1..=pages)
                    .filter(|&p| self.cached_objects(&i.file, p, LAYOUT.id).is_some())
                    .count(),
            )
            .unwrap_or(0);
            for p in order {
                if cx.is_cancelled() {
                    return Err(CommandError::Cancelled);
                }
                if self.cached_objects(&i.file, p, LAYOUT.id).is_some() {
                    continue;
                }
                if let Err(e) = self.page_objects(&i.file, path, p, true) {
                    tracing::warn!(page = p, error = %e, "layout failed on a page");
                }
                done += 1;
                if done.is_multiple_of(2) || done == pages {
                    self.report(
                        cx,
                        &i.file,
                        &state(Stage::Objects, StageStatus::Running, done),
                    );
                }
            }
        }
        let out = self.preparation_state(&i.file, Some(&a), pages);
        if let Some(bus) = self.bus.lock().upgrade() {
            bus.events().publish(
                intel_events::PREPARED,
                serde_json::to_value(&out).unwrap_or_default(),
                cx.clock.now_ms(),
            );
        }
        Ok(out)
    }

    pub(crate) async fn preparation(
        self: Arc<Self>,
        _cx: Cx,
        i: FileOnly,
    ) -> Result<Preparation, CommandError> {
        let path = self.path(&i.file)?;
        let me = self.clone();
        blocking(move || {
            let (g, _) = me.geoms(i.file.hash, &path)?;
            let a = me
                .analyses
                .lock()
                .iter()
                .find(|(h, _)| *h == i.file.hash)
                .map(|(_, a)| a.clone());
            Ok(me.preparation_state(&i.file, a.as_deref(), u32::try_from(g.len()).unwrap_or(0)))
        })
        .await
    }

    pub(crate) async fn objects(
        self: Arc<Self>,
        _cx: Cx,
        i: ObjectsRequest,
    ) -> Result<PageObjects, CommandError> {
        let path = self.path(&i.file)?;
        let me = self.clone();
        blocking(move || me.page_objects(&i.file, &path, i.page, true)).await
    }

    /// Objects of every page: what preparation stored, text alone for the rest (fast).
    fn all_objects(&self, file: &BlobRef, path: &Path) -> Result<Vec<PageObjects>, CommandError> {
        let (g, _) = self.geoms(file.hash, path)?;
        (1..=u32::try_from(g.len()).unwrap_or(0))
            .map(|p| self.page_objects(file, path, p, false))
            .collect()
    }

    pub(crate) async fn contents(
        self: Arc<Self>,
        _cx: Cx,
        i: FileOnly,
    ) -> Result<Contents, CommandError> {
        let path = self.path(&i.file)?;
        let me = self.clone();
        blocking(move || {
            let a = me.analysis(&i.file, &path)?;
            let all = me.all_objects(&i.file, &path)?;
            let pick = |k: ObjectKind| -> Vec<DocObject> {
                let mut v: Vec<DocObject> = all
                    .iter()
                    .flat_map(|p| p.objects.iter())
                    .filter(|o| o.kind == k && o.label.is_some())
                    .cloned()
                    .collect();
                // Without the model, a caption stands for its figure or table.
                if v.is_empty() && matches!(k, ObjectKind::Figure | ObjectKind::Table) {
                    let prefix = if k == ObjectKind::Figure {
                        "Figure"
                    } else {
                        "Table"
                    };
                    v = all
                        .iter()
                        .flat_map(|p| p.objects.iter())
                        .filter(|o| {
                            o.kind == ObjectKind::Caption
                                && o.label.as_deref().is_some_and(|l| l.starts_with(prefix))
                        })
                        .cloned()
                        .collect();
                }
                v
            };
            Ok(Contents {
                sections: headings::outline(&a.heads),
                figures: pick(ObjectKind::Figure),
                tables: pick(ObjectKind::Table),
                equations: pick(ObjectKind::Equation),
            })
        })
        .await
    }

    pub(crate) async fn references(
        self: Arc<Self>,
        _cx: Cx,
        i: FileOnly,
    ) -> Result<Vec<Reference>, CommandError> {
        let path = self.path(&i.file)?;
        let me = self.clone();
        blocking(move || Ok(me.analysis(&i.file, &path)?.refs.clone())).await
    }

    pub(crate) async fn mentions(
        self: Arc<Self>,
        _cx: Cx,
        i: ObjectsRequest,
    ) -> Result<Vec<Mention>, CommandError> {
        let path = self.path(&i.file)?;
        let me = self.clone();
        blocking(move || {
            let a = me.analysis(&i.file, &path)?;
            let pt = a
                .pages
                .get(i.page.saturating_sub(1) as usize)
                .ok_or_else(|| CommandError::failed("bad_page", format!("page {}", i.page)))?;
            let mut found = mentions::mentions(pt);
            if found.is_empty() {
                return Ok(found);
            }
            // Targets: labelled objects of the whole paper, by kind and number.
            let all = me.all_objects(&i.file, &path)?;
            let mut targets: HashMap<(u8, String), ObjectRef> = HashMap::new();
            for po in &all {
                for o in &po.objects {
                    let Some(n) = o.number.clone() else { continue };
                    let k = match o.kind {
                        ObjectKind::Figure => 0,
                        ObjectKind::Table => 1,
                        ObjectKind::Equation => 2,
                        ObjectKind::Heading => 3,
                        ObjectKind::Caption
                            if o.label.as_deref().is_some_and(|l| l.starts_with("Figure")) =>
                        {
                            10
                        }
                        ObjectKind::Caption => 11,
                        _ => continue,
                    };
                    targets.entry((k, n)).or_insert(ObjectRef {
                        page: po.page,
                        id: o.id.clone(),
                    });
                }
            }
            for m in &mut found {
                let (k, fallback) = match m.kind {
                    MentionKind::Figure => (0, Some(10)),
                    MentionKind::Table => (1, Some(11)),
                    MentionKind::Equation => (2, None),
                    MentionKind::Section => (3, None),
                };
                let n = m.numbers.first().cloned().unwrap_or_default();
                m.target = targets
                    .get(&(k, n.clone()))
                    .or_else(|| fallback.and_then(|f| targets.get(&(f, n))))
                    .cloned();
            }
            Ok(found)
        })
        .await
    }

    pub(crate) async fn citation_markers(
        self: Arc<Self>,
        _cx: Cx,
        i: ObjectsRequest,
    ) -> Result<Vec<CitationMarker>, CommandError> {
        let path = self.path(&i.file)?;
        let me = self.clone();
        blocking(move || {
            let a = me.analysis(&i.file, &path)?;
            let pt = a
                .pages
                .get(i.page.saturating_sub(1) as usize)
                .ok_or_else(|| CommandError::failed("bad_page", format!("page {}", i.page)))?;
            Ok(mentions::citation_markers(pt, &a.refs))
        })
        .await
    }

    /// A region of a page as a PNG in the render cache, about `width_px` wide.
    #[allow(clippy::cast_precision_loss, clippy::many_single_char_names)] // pixel sizes are small; geometry names
    fn region_image(
        &self,
        path: &Path,
        g: &[PageGeom],
        page: u32,
        r: &PageRect,
        width_px: u32,
    ) -> Result<BlobRef, CommandError> {
        let (store, _) = self.renders.as_ref().ok_or_else(|| {
            CommandError::failed(errors::UNREADABLE, "no render cache configured")
        })?;
        let idx = Self::page_index(g, page)?;
        let size = g[idx as usize].size();
        let scale = (width_px.clamp(64, 2400) as f32 / r.width().max(1.0))
            .min(8192.0 / size.width.max(1.0));
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // positive, bounded
        let page_w = (size.width * scale).round() as u32;
        let bmp = self
            .backend()?
            .render(path, idx, page_w)
            .map_err(backend_err)?;
        let s = bmp.width as f32 / size.width.max(1.0);
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let px = |v: f32| (v * s).max(0.0).round() as u32;
        let part = crop(&bmp, px(r.x0), px(r.y0), px(r.x1), px(r.y1));
        let bytes = png(part.width, part.height, &part.rgba)?;
        store
            .put(&bytes[..], MediaType("image/png".into()))
            .map_err(|e| CommandError::failed("render", e))
    }

    #[allow(clippy::many_single_char_names)] // i, g, p, o, r: request, geometry, point, object, rect
    pub(crate) async fn region(
        self: Arc<Self>,
        _cx: Cx,
        i: RegionRequest,
    ) -> Result<RegionHit, CommandError> {
        let path = self.path(&i.file)?;
        let me = self.clone();
        blocking(move || {
            let (g, _) = me.geoms(i.file.hash, &path)?;
            let objs = me.page_objects(&i.file, &path, i.page, true)?;
            let (object, rect) = match (&i.point, &i.rect) {
                (Some(p), _) => {
                    let o = layout::at_point(&objs.objects, p.x, p.y)
                        .cloned()
                        .ok_or_else(|| no_object("nothing here"))?;
                    let r = o.rect;
                    (Some(o), r)
                }
                (None, Some(r)) => {
                    let o = layout::best_cover(&objs.objects, r).cloned();
                    // Snap to the object when the drag nearly covers it.
                    let rect = o.as_ref().map_or(*r, |o| o.rect);
                    (
                        o,
                        PageRect {
                            page: i.page,
                            ..rect
                        },
                    )
                }
                (None, None) => {
                    return Err(CommandError::InvalidInput {
                        message: "give a point or a rectangle".into(),
                    });
                }
            };
            let a = me.analysis(&i.file, &path)?;
            let text = core::text_in(&a.pages[(i.page - 1) as usize], &rect);
            let image = me.region_image(&path, &g, i.page, &rect, i.width_px.unwrap_or(800))?;
            Ok(RegionHit {
                object,
                rect,
                image,
                text,
            })
        })
        .await
    }

    fn find_object(
        &self,
        file: &BlobRef,
        path: &Path,
        r: &ObjectRef,
    ) -> Result<DocObject, CommandError> {
        self.page_objects(file, path, r.page, true)?
            .objects
            .into_iter()
            .find(|o| o.id == r.id)
            .ok_or_else(|| no_object(format!("object {} on page {}", r.id, r.page)))
    }

    pub(crate) async fn equation(
        self: Arc<Self>,
        _cx: Cx,
        i: EquationRequest,
    ) -> Result<Equation, CommandError> {
        let path = self.path(&i.file)?;
        let me = self.clone();
        blocking(move || {
            let (g, _) = me.geoms(i.file.hash, &path)?;
            let object = match (&i.object, &i.number) {
                (Some(r), _) => me.find_object(&i.file, &path, r)?,
                (None, Some(n)) => {
                    let n = n.trim().trim_matches(['(', ')']);
                    me.all_objects(&i.file, &path)?
                        .into_iter()
                        .flat_map(|p| p.objects)
                        .find(|o| o.kind == ObjectKind::Equation && o.number.as_deref() == Some(n))
                        .ok_or_else(|| no_object(format!("equation ({n})")))?
                }
                (None, None) => return Err(CommandError::InvalidInput { message: "give an object or a number".into() }),
            };
            let page = object.rect.page;
            let image = me.region_image(&path, &g, page, &object.rect, 900)?;
            let a = me.analysis(&i.file, &path)?;
            let text = core::text_in(&a.pages[(page - 1) as usize], &object.rect);
            Ok(Equation {
                object,
                page,
                latex: None,
                note: Some("Converting equations to LaTeX needs the formula model, which is not installed yet.".into()),
                image,
                text,
            })
        })
        .await
    }

    pub(crate) async fn table(
        self: Arc<Self>,
        _cx: Cx,
        i: TableRequest,
    ) -> Result<Table, CommandError> {
        let path = self.path(&i.file)?;
        let me = self.clone();
        blocking(move || {
            let object = me.find_object(&i.file, &path, &i.object)?;
            if object.kind != ObjectKind::Table {
                return Err(no_object(format!("{} is not a table", object.id)));
            }
            let a = me.analysis(&i.file, &path)?;
            Ok(tables::table(
                &a.pages[(object.rect.page - 1) as usize],
                &object,
            ))
        })
        .await
    }

    #[allow(clippy::unused_async, clippy::unused_async_trait_impl)] // bus handlers are async
    pub(crate) async fn models(
        self: Arc<Self>,
        _cx: Cx,
        _i: (),
    ) -> Result<Vec<ModelInfo>, CommandError> {
        let dir = self.models.read().dir.clone();
        Ok(models::ALL.iter().map(|m| m.info(dir.as_deref())).collect())
    }
}
