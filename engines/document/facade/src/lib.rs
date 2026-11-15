//! Document engine facade: `document.*` commands on the bus.
//!
//! The engine reads stored files through the [`BlobAccess`] port, which the host implements
//! with the open library's blob store (the Document engine does not depend on Workspace).
//! PDF work goes through the [`PdfBackend`] port (`PDFium`, ADR-003); rendered images go to a
//! disposable render cache (content-addressed, in the app cache folder).
#![deny(missing_docs)]

use std::collections::{HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Weak};

use ep_blob::BlobStore;
use ep_bus::cache::{ResultCache, cache_key};
use ep_bus::{Bus, CommandError, Cx, Permission, RegisterError, Spec, Stability};
use ep_document_contract::{
    AnnotationOp, ApplyAnnotations, DocInfo, FileRequest, Link, MetadataRequest, OutlineItem,
    PageColors, PageRequest, PageSize, PageText, PaperRef, PdfMetadata, RenderRequest, Rendered,
    SearchRequest, SearchResult, ThumbnailsRequest, commands, errors, pdf_errors,
};
use ep_document_core::pdf::{BackendError, PageGeom, PdfBackend};
use ep_document_core::search::{PageChars, SearchIndex};
use ep_types::{BlobRef, Hash, MediaType};
use parking_lot::{Mutex, RwLock};
use serde_json::json;

mod annotations;
mod intel;
mod models;
pub use annotations::DATABASE as ANNOTATION_DATABASE;
pub use models::{ModelSpec, bundled_models_dir, find_onnxruntime, group_of};

pub use ep_document_core::pdf::PdfBackend as Backend;

/// Port: the open library's stored files and folder (the host implements it with Workspace).
pub trait BlobAccess: Send + Sync {
    /// Path of the blob, if it is available.
    fn path_of(&self, hash: &Hash) -> Option<PathBuf>;

    /// Store new file content (a saved PDF) and return its reference.
    ///
    /// # Errors
    /// Nothing is open, or the file cannot be written.
    fn put(&self, bytes: &[u8], media_type: MediaType) -> std::io::Result<BlobRef> {
        let _ = (bytes, media_type);
        Err(std::io::Error::other("storing files is not available"))
    }

    /// Id and folder of the open library or project, where the engine keeps its own database
    /// (`document.sqlite`, annotations). `None` when nothing is open.
    fn project(&self) -> Option<(String, PathBuf)> {
        None
    }
}

/// Version of everything that shapes rendered images; bump to invalidate cached renders.
const RENDER_IMPL: &str = "pdfium-8125/png-2-no-annots";
/// Search indexes kept in memory.
const INDEXES: usize = 8;

/// Cached page geometry of one file: hash, pages, encrypted.
type GeomEntry = (Hash, Arc<Vec<PageGeom>>, bool);
type SizeEntry = (Hash, Arc<Vec<PageSize>>, bool);

/// The Document engine.
pub struct Document {
    blobs: Arc<dyn BlobAccess>,
    pdf: Option<Arc<dyn PdfBackend>>,
    renders: Option<(BlobStore, ResultCache)>,
    geoms: Mutex<VecDeque<GeomEntry>>,
    /// Page sizes read without loading pages (opening a paper needs only these).
    sizes: Mutex<VecDeque<SizeEntry>>,
    /// Geometry of single pages read before the whole paper's (the pages being shown).
    page_geoms: Mutex<VecDeque<(Hash, u32, PageGeom)>>,
    indexes: Mutex<VecDeque<(Hash, Arc<SearchIndex>)>>,
    /// The open project's annotation database, by project id.
    annotation_db: Mutex<Option<(String, Arc<ep_store::Store>)>>,
    /// For hub events.
    bus: Mutex<Weak<Bus>>,
    /// Author written on new annotations (`/T`).
    author: RwLock<String>,
    /// Paper-wide analyses (headings, references) of recent files.
    analyses: Mutex<VecDeque<(Hash, Arc<intel::Analysis>)>>,
    /// Files being prepared now.
    preparing: Mutex<HashSet<Hash>>,
    /// Page models.
    models: RwLock<intel::Models>,
    /// OCR engine for scanned pages.
    ocr: RwLock<Option<Arc<dyn ep_document_core::intel::ocr::OcrEngine>>>,
}

impl std::fmt::Debug for Document {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Document")
            .field("pdf", &self.pdf.is_some())
            .finish_non_exhaustive()
    }
}

/// Start the `PDFium` backend from the folder holding its library.
///
/// # Errors
/// The library cannot be loaded.
pub fn start_pdfium(dir: &Path) -> Result<Arc<dyn PdfBackend>, String> {
    Ok(Arc::new(ep_document_adapter_pdfium::PdfiumBackend::start(
        dir,
    )?))
}

/// Where the `PDFium` library is: `EP_PDFIUM_DIR`, next to the executable (installed app), or the
/// repository's `.natives/pdfium/bin` above it (development builds).
#[must_use]
pub fn find_pdfium() -> Option<PathBuf> {
    let has_lib = |d: &Path| {
        ["pdfium.dll", "libpdfium.so", "libpdfium.dylib"]
            .iter()
            .any(|n| d.join(n).exists())
    };
    if let Some(d) = std::env::var_os("EP_PDFIUM_DIR").map(PathBuf::from) {
        return has_lib(&d).then_some(d);
    }
    let exe_dir = std::env::current_exe().ok()?.parent()?.to_path_buf();
    if has_lib(&exe_dir) {
        return Some(exe_dir);
    }
    exe_dir
        .ancestors()
        .map(|a| a.join(".natives").join("pdfium").join("bin"))
        .find(|d| has_lib(d))
}

fn backend_err(e: BackendError) -> CommandError {
    let kind = match e {
        BackendError::Unreadable(_) => errors::UNREADABLE,
        BackendError::PasswordRequired => pdf_errors::PASSWORD_REQUIRED,
        BackendError::BadPage(_) => pdf_errors::BAD_PAGE,
        BackendError::Timeout => pdf_errors::TIMEOUT,
    };
    CommandError::failed(kind, e)
}

async fn blocking<T: Send + 'static>(
    f: impl FnOnce() -> Result<T, CommandError> + Send + 'static,
) -> Result<T, CommandError> {
    tokio::task::spawn_blocking(f)
        .await
        .map_err(|e| CommandError::failed(errors::UNREADABLE, e))?
}

fn png(width: u32, height: u32, rgba: &[u8]) -> Result<Vec<u8>, CommandError> {
    let mut out = Vec::with_capacity(rgba.len() / 4);
    let mut enc = png::Encoder::new(&mut out, width, height);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    enc.set_compression(png::Compression::Fast);
    let mut w = enc
        .write_header()
        .map_err(|e| CommandError::failed("render", e))?;
    w.write_image_data(rgba)
        .map_err(|e| CommandError::failed("render", e))?;
    w.finish().map_err(|e| CommandError::failed("render", e))?;
    Ok(out)
}

impl Document {
    /// An engine reading files through `blobs`. Without `pdf`, only `document.metadata` works;
    /// without `render_cache`, `document.render` and `document.thumbnails` are unavailable.
    ///
    /// # Errors
    /// The render cache folder cannot be created.
    pub fn new(
        blobs: Arc<dyn BlobAccess>,
        pdf: Option<Arc<dyn PdfBackend>>,
        render_cache: Option<&Path>,
    ) -> std::io::Result<Arc<Self>> {
        let renders = match render_cache {
            Some(dir) => Some((
                BlobStore::open(dir.join("images")).map_err(std::io::Error::other)?,
                ResultCache::open(dir.join("index"))?,
            )),
            None => None,
        };
        Ok(Arc::new(Self {
            blobs,
            pdf,
            renders,
            geoms: Mutex::new(VecDeque::new()),
            sizes: Mutex::new(VecDeque::new()),
            page_geoms: Mutex::new(VecDeque::new()),
            indexes: Mutex::new(VecDeque::new()),
            annotation_db: Mutex::new(None),
            bus: Mutex::new(Weak::new()),
            author: RwLock::new(String::new()),
            analyses: Mutex::new(VecDeque::new()),
            preparing: Mutex::new(HashSet::new()),
            models: RwLock::new(intel::Models::default()),
            ocr: RwLock::new(None),
        }))
    }

    /// The OCR engine for scanned pages (the host chooses one for its system).
    pub fn set_ocr(&self, engine: Arc<dyn ep_document_core::intel::ocr::OcrEngine>) {
        *self.ocr.write() = Some(engine);
    }

    /// The name written as the author of new annotations (the host passes the user's name).
    pub fn set_author(&self, name: &str) {
        name.trim().clone_into(&mut self.author.write());
    }

    /// The image store of the render cache (hosts serve it to the viewer).
    #[must_use]
    pub fn render_path(&self, hash: &Hash) -> Option<PathBuf> {
        let (store, _) = self.renders.as_ref()?;
        let p = store.path_of(hash);
        p.exists().then_some(p)
    }

    fn path(&self, file: &BlobRef) -> Result<PathBuf, CommandError> {
        self.blobs.path_of(&file.hash).ok_or_else(|| {
            CommandError::failed(
                errors::NOT_FOUND,
                format!("file {} is not available", file.hash),
            )
        })
    }

    fn backend(&self) -> Result<Arc<dyn PdfBackend>, CommandError> {
        self.pdf.clone().ok_or_else(|| {
            CommandError::failed(
                errors::UNREADABLE,
                "the PDF backend (PDFium) is not available",
            )
        })
    }

    /// Page geometry of a file, cached by content hash.
    fn geoms(&self, hash: Hash, path: &Path) -> Result<(Arc<Vec<PageGeom>>, bool), CommandError> {
        if let Some((_, g, e)) = self.geoms.lock().iter().find(|(h, ..)| *h == hash) {
            return Ok((g.clone(), *e));
        }
        let (g, enc) = self.backend()?.info(path).map_err(backend_err)?;
        let g = Arc::new(g);
        let mut cache = self.geoms.lock();
        if cache.len() >= 32 {
            cache.pop_back();
        }
        cache.push_front((hash, g.clone(), enc));
        Ok((g, enc))
    }

    fn page_index(geoms: &[PageGeom], page: u32) -> Result<u32, CommandError> {
        Self::page_index_of(geoms.len(), page)
    }

    fn page_index_of(pages: usize, page: u32) -> Result<u32, CommandError> {
        if page == 0 || page as usize > pages {
            return Err(CommandError::failed(
                pdf_errors::BAD_PAGE,
                format!("page {page} does not exist (the PDF has {pages})"),
            ));
        }
        Ok(page - 1)
    }

    /// Page sizes and encryption, without loading any page (fast even for long papers).
    fn sizes(&self, hash: Hash, path: &Path) -> Result<(Arc<Vec<PageSize>>, bool), CommandError> {
        if let Some((_, g, e)) = self.geoms.lock().iter().find(|(h, ..)| *h == hash) {
            return Ok((Arc::new(g.iter().map(PageGeom::size).collect()), *e));
        }
        if let Some((_, s, e)) = self.sizes.lock().iter().find(|(h, ..)| *h == hash) {
            return Ok((s.clone(), *e));
        }
        let (s, enc) = self.backend()?.sizes(path).map_err(backend_err)?;
        let s = Arc::new(s);
        let mut cache = self.sizes.lock();
        if cache.len() >= 32 {
            cache.pop_back();
        }
        cache.push_front((hash, s.clone(), enc));
        Ok((s, enc))
    }

    /// Geometry of one page (0-based `idx`), loading only that page unless the whole paper's
    /// geometry is known.
    fn geom_of(&self, hash: Hash, path: &Path, idx: u32) -> Result<PageGeom, CommandError> {
        if let Some((_, g, _)) = self.geoms.lock().iter().find(|(h, ..)| *h == hash)
            && let Some(x) = g.get(idx as usize)
        {
            return Ok(*x);
        }
        if let Some((.., g)) = self
            .page_geoms
            .lock()
            .iter()
            .find(|(h, i, _)| *h == hash && *i == idx)
        {
            return Ok(*g);
        }
        let g = self.backend()?.page_geom(path, idx).map_err(backend_err)?;
        let mut cache = self.page_geoms.lock();
        if cache.len() >= 256 {
            cache.pop_back();
        }
        cache.push_front((hash, idx, g));
        Ok(g)
    }

    /// Register the `document.*` commands.
    ///
    /// # Errors
    /// A name is already registered.
    #[allow(clippy::too_many_lines)] // one line per command
    pub fn register(self: &Arc<Self>, bus: &Arc<Bus>) -> Result<(), RegisterError> {
        *self.bus.lock() = Arc::downgrade(bus);
        macro_rules! reg {
            ($name:expr, $summary:expr, $method:ident) => {{ reg!($name, $summary, $method, Permission::read()) }};
            ($name:expr, $summary:expr, $method:ident, $perm:expr) => {{
                let d = self.clone();
                let spec = Spec {
                    name: $name,
                    summary: $summary,
                    stability: Stability::Experimental,
                    permission: $perm,
                };
                bus.register(spec, move |cx: Cx, i| {
                    let d = d.clone();
                    async move { d.$method(cx, i).await }
                })?;
            }};
        }
        reg!(
            commands::METADATA,
            "Title, authors, DOI and page count from a stored PDF",
            metadata
        );
        reg!(
            commands::INFO,
            "Page count and the size of every page",
            info
        );
        reg!(
            commands::TEXT,
            "Characters, words and lines of a page, with boxes in page space",
            text
        );
        reg!(commands::RENDER, "A page as a PNG image (cached)", render);
        reg!(
            commands::THUMBNAILS,
            "Small images of pages (cached)",
            thumbnails
        );
        reg!(commands::OUTLINE, "The PDF's bookmarks", outline);
        reg!(
            commands::LINKS,
            "Link areas of a page and where they go",
            links
        );
        reg!(
            commands::SEARCH,
            "Notation-aware word search with context in one or more papers",
            search
        );
        reg!(
            commands::ANNOTATIONS,
            "The annotations of a paper (imports those already in its PDF the first time)",
            annotations,
            Permission::create()
        );
        reg!(
            commands::ANNOTATIONS_APPLY,
            "Add, change or remove annotations as one undoable edit",
            apply_annotations,
            Permission::create()
        );
        reg!(
            commands::ANNOTATIONS_UNDO,
            "Undo the last annotation edit of a paper",
            annotations_undo,
            Permission::create()
        );
        reg!(
            commands::ANNOTATIONS_REDO,
            "Redo the last undone annotation edit of a paper",
            annotations_redo,
            Permission::create()
        );
        reg!(
            commands::PREPARATION,
            "How far preparation of a file has come",
            preparation
        );
        reg!(
            commands::OBJECTS,
            "Objects on a page: figures, tables, equations, headings",
            objects
        );
        reg!(
            commands::REGION,
            "The object at a point or in a rectangle, with an image of it",
            region
        );
        reg!(
            commands::CONTENTS,
            "Generated outline and lists of figures, tables and equations",
            contents
        );
        reg!(
            commands::MENTIONS,
            "Mentions of figures, tables, equations and sections on a page",
            mentions
        );
        reg!(
            commands::CITATION_MARKERS,
            "Citation markers on a page, linked to the reference list",
            citation_markers
        );
        reg!(commands::REFERENCES, "The reference list", references);
        reg!(
            commands::EQUATION,
            "An equation as an image, with LaTeX when the formula model is installed",
            equation
        );
        reg!(commands::TABLE, "A table as rows of cells, with CSV", table);
        reg!(
            commands::MODELS,
            "Page models and whether they are present",
            models
        );
        reg!(
            commands::OCR,
            "Write the recognised text of scanned pages into a new version of the PDF",
            ocr,
            Permission::create()
        );
        {
            let d = self.clone();
            let spec = Spec {
                name: commands::PREPARE,
                summary: "Prepare a file in the background (text, outline, objects, links); streams progress",
                stability: Stability::Experimental,
                permission: Permission::read(),
            };
            bus.register_streaming(spec, move |cx: Cx, i| {
                let d = d.clone();
                async move { d.prepare(cx, i).await }
            })?;
        }
        reg!(
            commands::ANNOTATE,
            "Write a paper's annotations into a new version of its PDF",
            annotate,
            Permission::create()
        );
        Ok(())
    }

    async fn apply_annotations(
        self: Arc<Self>,
        cx: Cx,
        mut i: ApplyAnnotations,
    ) -> Result<ep_document_contract::AnnotationSet, CommandError> {
        let author = self.author.read().clone();
        if !author.is_empty() {
            for op in &mut i.ops {
                if let AnnotationOp::Add { annotation } = op
                    && annotation.author.is_none()
                {
                    annotation.author = Some(author.clone());
                }
            }
        }
        self.annotations_apply(cx, i).await
    }

    async fn metadata(
        self: Arc<Self>,
        _cx: Cx,
        i: MetadataRequest,
    ) -> Result<PdfMetadata, CommandError> {
        let path = self.path(&i.file)?;
        blocking(move || {
            let raw = ep_document_adapter_lopdf::read_metadata(&path)
                .map_err(|e| CommandError::failed(errors::UNREADABLE, e))?;
            Ok(ep_document_core::metadata::clean(&raw))
        })
        .await
    }

    async fn info(self: Arc<Self>, _cx: Cx, i: FileRequest) -> Result<DocInfo, CommandError> {
        let path = self.path(&i.file)?;
        let me = self.clone();
        blocking(move || {
            let (sizes, encrypted) = me.sizes(i.file.hash, &path)?;
            Ok(DocInfo {
                pages: u32::try_from(sizes.len()).unwrap_or(u32::MAX),
                sizes: sizes.to_vec(),
                encrypted,
            })
        })
        .await
    }

    async fn text(self: Arc<Self>, _cx: Cx, i: PageRequest) -> Result<PageText, CommandError> {
        let path = self.path(&i.file)?;
        let me = self.clone();
        blocking(move || {
            let (sizes, _) = me.sizes(i.file.hash, &path)?;
            let idx = Self::page_index_of(sizes.len(), i.page)?;
            let geom = me.geom_of(i.file.hash, &path, idx)?;
            me.page_text_of(&i.file.hash, &path, &geom, idx)
        })
        .await
    }

    fn render_one(
        &self,
        file: &BlobRef,
        path: &Path,
        page: u32,
        width_px: u32,
        colors: Option<&PageColors>,
    ) -> Result<Rendered, CommandError> {
        let (store, index) = self.renders.as_ref().ok_or_else(|| {
            CommandError::failed(errors::UNREADABLE, "no render cache configured")
        })?;
        let width_px = width_px.clamp(16, 8192);
        let key = cache_key(
            commands::RENDER,
            RENDER_IMPL,
            &match colors {
                // Without colours, the key is the one cached drawings already have.
                None => json!({ "file": file.hash, "page": page, "width_px": width_px }),
                Some(c) => {
                    json!({ "file": file.hash, "page": page, "width_px": width_px, "colors": c })
                }
            },
        );
        if let Some(r) = index
            .get(&key)
            .and_then(|v| serde_json::from_value::<Rendered>(v).ok())
            && store.stat(&r.image.hash).ok().flatten().is_some()
        {
            return Ok(r);
        }
        let (sizes, _) = self.sizes(file.hash, path)?;
        let idx = Self::page_index_of(sizes.len(), page)?;
        let mut bmp = self
            .backend()?
            .render(path, idx, width_px)
            .map_err(backend_err)?;
        if let Some(c) = colors {
            let bad = || CommandError::failed("invalid_colors", "colors must be #rrggbb");
            let bg = ep_document_core::pdf::hex_color(&c.background).ok_or_else(bad)?;
            let fg = ep_document_core::pdf::hex_color(&c.foreground).ok_or_else(bad)?;
            ep_document_core::pdf::recolor(&mut bmp.rgba, bg, fg);
        }
        let bytes = png(bmp.width, bmp.height, &bmp.rgba)?;
        let image = store
            .put(&bytes[..], MediaType("image/png".into()))
            .map_err(|e| CommandError::failed("render", e))?;
        let r = Rendered {
            page,
            image,
            width_px: bmp.width,
            height_px: bmp.height,
        };
        let _ = index.put(&key, &serde_json::to_value(&r).unwrap_or_default());
        Ok(r)
    }

    async fn render(self: Arc<Self>, _cx: Cx, i: RenderRequest) -> Result<Rendered, CommandError> {
        let path = self.path(&i.file)?;
        let me = self.clone();
        blocking(move || me.render_one(&i.file, &path, i.page, i.width_px, i.colors.as_ref())).await
    }

    async fn thumbnails(
        self: Arc<Self>,
        cx: Cx,
        i: ThumbnailsRequest,
    ) -> Result<Vec<Rendered>, CommandError> {
        let path = self.path(&i.file)?;
        let me = self.clone();
        blocking(move || {
            let (g, _) = me.geoms(i.file.hash, &path)?;
            let pages: Vec<u32> = if i.pages.is_empty() {
                (1..=u32::try_from(g.len()).unwrap_or(0)).collect()
            } else {
                i.pages.clone()
            };
            let mut out = Vec::with_capacity(pages.len());
            for p in pages {
                if cx.is_cancelled() {
                    return Err(CommandError::Cancelled);
                }
                let r = me.render_one(&i.file, &path, p, i.width_px.min(512), None)?;
                cx.progress(&r);
                out.push(r);
            }
            Ok(out)
        })
        .await
    }

    async fn outline(
        self: Arc<Self>,
        _cx: Cx,
        i: FileRequest,
    ) -> Result<Vec<OutlineItem>, CommandError> {
        let path = self.path(&i.file)?;
        let me = self.clone();
        blocking(move || {
            let (g, _) = me.geoms(i.file.hash, &path)?;
            let raw = me.backend()?.outline(&path).map_err(backend_err)?;
            Ok(ep_document_core::pdf::outline(&raw, &g))
        })
        .await
    }

    async fn links(self: Arc<Self>, _cx: Cx, i: PageRequest) -> Result<Vec<Link>, CommandError> {
        let path = self.path(&i.file)?;
        let me = self.clone();
        blocking(move || {
            let (sizes, _) = me.sizes(i.file.hash, &path)?;
            let idx = Self::page_index_of(sizes.len(), i.page)?;
            let geom = me.geom_of(i.file.hash, &path, idx)?;
            let raw = me.backend()?.links(&path, idx).map_err(backend_err)?;
            let n = u32::try_from(sizes.len()).unwrap_or(0);
            Ok(raw
                .iter()
                .map(|l| Link {
                    rect: geom.rect(i.page, l.rect),
                    target: ep_document_core::pdf::target_with(&l.dest, |t| {
                        (t < n)
                            .then(|| me.geom_of(i.file.hash, &path, t).ok())
                            .flatten()
                    }),
                })
                .collect())
        })
        .await
    }

    /// The search index of a file, built from every page's text on first use.
    fn index(&self, hash: Hash, cx: &Cx) -> Result<Arc<SearchIndex>, CommandError> {
        if let Some((_, ix)) = self.indexes.lock().iter().find(|(h, _)| *h == hash) {
            return Ok(ix.clone());
        }
        let path = self.blobs.path_of(&hash).ok_or_else(|| {
            CommandError::failed(errors::NOT_FOUND, format!("paper {hash} is not available"))
        })?;
        let (g, _) = self.geoms(hash, &path)?;
        let backend = self.backend()?;
        let mut pages = Vec::with_capacity(g.len());
        for (i, geom) in g.iter().enumerate() {
            if cx.is_cancelled() {
                return Err(CommandError::Cancelled);
            }
            let idx = u32::try_from(i).unwrap_or(u32::MAX);
            let _ = (geom, &backend);
            let t = self.page_text_of(&hash, &path, &g[idx as usize], idx)?;
            let chars = t
                .text
                .chars()
                .zip(t.boxes)
                .map(|(c, b)| {
                    (
                        c,
                        b.map(|r| ep_types::PageRect {
                            page: idx + 1,
                            x0: r[0],
                            y0: r[1],
                            x1: r[2],
                            y1: r[3],
                        }),
                    )
                })
                .collect();
            pages.push(PageChars {
                page: idx + 1,
                chars,
            });
        }
        let ix = Arc::new(SearchIndex::build(&pages, Vec::new()));
        let mut cache = self.indexes.lock();
        if cache.len() >= INDEXES {
            cache.pop_back();
        }
        cache.push_front((hash, ix.clone()));
        Ok(ix)
    }

    async fn search(
        self: Arc<Self>,
        cx: Cx,
        i: SearchRequest,
    ) -> Result<SearchResult, CommandError> {
        let me = self.clone();
        blocking(move || {
            let mut indexes = Vec::with_capacity(i.papers.len());
            for p in &i.papers {
                let hash: Hash = p.0.parse().map_err(|_| CommandError::InvalidInput {
                    message: format!("paper {:?} is not a file hash", p.0),
                })?;
                indexes.push((PaperRef(p.0.clone()), me.index(hash, &cx)?));
            }
            let refs: Vec<(PaperRef, &SearchIndex)> = indexes
                .iter()
                .map(|(p, ix)| (p.clone(), ix.as_ref()))
                .collect();
            ep_document_core::search::search(&refs, &i)
                .map_err(|e| CommandError::failed("search", format!("{e:?}")))
        })
        .await
    }
}
