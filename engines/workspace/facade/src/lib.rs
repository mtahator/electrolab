//! Workspace engine facade: the `workspace.*` commands, composed with the blob store, database,
//! event log and OS ports.
//!
//! One library or project is open at a time. Its folder (ADR-010):
//! `electropaper-project.toml`, `blobs/`, `workspace.sqlite`, `events/`.
#![deny(missing_docs)]

use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Weak};

use ep_blob::BlobStore;
use ep_bus::{Bus, CommandError, Cx, Permission, RegisterError, Spec, Stability};
use ep_events::{Event, EventLog};
use ep_os::AppDirs;
use ep_store::rusqlite::{self, OptionalExtension, params};
use ep_store::{Migration, Store};
use ep_types::{BlobRef, Hash, MediaType, PaperId, ProjectId};
use ep_workspace_contract::{
    Added, Classification, CreateProject, FileKind, FilePath, GetPaper, OpenProject, Paper,
    ProjectInfo, ProjectKind, SetPaperMetadata, commands as cmd, errors, events,
};
use ep_workspace_core::{
    BLOBS_DIR, DATABASE, EVENTS_DIR, MANIFEST, Manifest, classify, clean_name,
};
use parking_lot::RwLock;

mod library;
use serde_json::json;

/// An open library or project.
#[derive(Debug)]
pub struct Opened {
    root: PathBuf,
    manifest: Manifest,
    /// Managed copies of papers.
    pub blobs: BlobStore,
    db: Store,
    /// The project's event log (provenance and domain events).
    pub events: Arc<EventLog>,
}

/// The Workspace engine.
pub struct Workspace {
    dirs: Arc<dyn AppDirs>,
    open: RwLock<Option<Arc<Opened>>>,
    bus: RwLock<Weak<Bus>>,
}

impl std::fmt::Debug for Workspace {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Workspace")
            .field("open", &self.open.read().as_ref().map(|o| o.root.clone()))
            .finish_non_exhaustive()
    }
}

fn io_err(e: impl std::fmt::Display) -> CommandError {
    CommandError::failed(errors::IO, e)
}

fn sql_err(e: rusqlite::Error) -> CommandError {
    io_err(e)
}

fn migrations() -> Vec<Migration> {
    ep_workspace_core::MIGRATIONS
        .iter()
        .map(|&(version, name, sql)| Migration { version, name, sql })
        .collect()
}

/// Columns of a paper row: the table plus its tags (joined with the unit separator U+001F).
const PAPER_SELECT: &str = "SELECT p.*, (SELECT group_concat(t.tag, char(31)) FROM paper_tag t WHERE t.paper = p.id) AS tags FROM paper p";

fn row_to_paper(r: &rusqlite::Row<'_>) -> rusqlite::Result<Paper> {
    let id: String = r.get("id")?;
    let hash: String = r.get("hash")?;
    let authors: String = r.get("authors")?;
    let bad = |i: usize, e: String| {
        rusqlite::Error::FromSqlConversionFailure(i, rusqlite::types::Type::Text, e.into())
    };
    Ok(Paper {
        id: id
            .parse()
            .map_err(|e: ep_types::IdParseError| bad(0, e.to_string()))?,
        file: BlobRef {
            hash: hash
                .parse()
                .map_err(|e: ep_types::HashParseError| bad(1, e.to_string()))?,
            size: u64::try_from(r.get::<_, i64>("size")?).unwrap_or(0),
            media_type: MediaType(r.get("media_type")?),
        },
        original_name: r.get("original_name")?,
        original_path: r.get("original_path")?,
        title: r.get("title")?,
        authors: serde_json::from_str(&authors).unwrap_or_default(),
        doi: r.get("doi")?,
        pages: r
            .get::<_, Option<i64>>("pages")?
            .and_then(|p| u32::try_from(p).ok()),
        metadata_source: r.get("metadata_source")?,
        tags: r
            .get::<_, Option<String>>("tags")?
            .map(|t| t.split('\u{1f}').map(str::to_string).collect())
            .unwrap_or_default(),
        added_ms: r.get("added_ms")?,
        opened_ms: r.get("opened_ms")?,
    })
}

impl Opened {
    fn info(&self) -> Result<ProjectInfo, CommandError> {
        let papers: i64 = self
            .db
            .read(|c| c.query_row("SELECT count(*) FROM paper", [], |r| r.get(0)))
            .map_err(sql_err)?;
        Ok(ProjectInfo {
            id: self.manifest.id,
            name: self.manifest.name.clone(),
            kind: self.manifest.kind,
            folder: self.root.display().to_string(),
            format: self.manifest.format,
            created_ms: self.manifest.created_ms,
            papers: u64::try_from(papers).unwrap_or(0),
        })
    }

    fn paper(&self, id: PaperId) -> Result<Option<Paper>, CommandError> {
        self.db
            .read(|c| {
                c.query_row(
                    &format!("{PAPER_SELECT} WHERE p.id = ?1"),
                    [id.to_string()],
                    row_to_paper,
                )
                .optional()
            })
            .map_err(sql_err)
    }

    fn paper_by_hash(&self, hash: &Hash) -> Result<Option<Paper>, CommandError> {
        self.db
            .read(|c| {
                c.query_row(
                    &format!("{PAPER_SELECT} WHERE p.hash = ?1"),
                    [hash.to_hex()],
                    row_to_paper,
                )
                .optional()
            })
            .map_err(sql_err)
    }

    fn record(&self, cx: &Cx, kind: &str, data: serde_json::Value) {
        let e = Event {
            at_ms: cx.clock.now_ms(),
            actor: cx.actor.clone(),
            kind: kind.into(),
            data,
        };
        if let Err(err) = self.events.append(e) {
            tracing::error!(%err, kind, "workspace event could not be written");
        }
    }
}

fn open_folder(root: &Path) -> Result<Opened, CommandError> {
    let text = fs::read_to_string(root.join(MANIFEST)).map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            CommandError::failed(
                errors::NOT_FOUND,
                format!(
                    "{} is not an ElectroPaper library or project",
                    root.display()
                ),
            )
        } else {
            io_err(e)
        }
    })?;
    let manifest =
        Manifest::parse(&text).map_err(|e| CommandError::failed(errors::BAD_FORMAT, e))?;
    let blobs = BlobStore::open(root.join(BLOBS_DIR)).map_err(io_err)?;
    let db = Store::open(root.join(DATABASE), &migrations()).map_err(|e| match e {
        ep_store::StoreError::TooNew { .. } => CommandError::failed(errors::BAD_FORMAT, e),
        other => io_err(other),
    })?;
    let events = Arc::new(EventLog::open(root.join(EVENTS_DIR)).map_err(io_err)?);
    Ok(Opened {
        root: root.to_path_buf(),
        manifest,
        blobs,
        db,
        events,
    })
}

fn create_folder(root: &Path, manifest: &Manifest) -> Result<(), CommandError> {
    if root.join(MANIFEST).exists() {
        return Err(CommandError::failed(
            errors::FOLDER_IN_USE,
            format!("{} is already a library or project", root.display()),
        ));
    }
    if root.exists() && fs::read_dir(root).map_err(io_err)?.next().is_some() {
        return Err(CommandError::failed(
            errors::FOLDER_IN_USE,
            format!("{} is not empty", root.display()),
        ));
    }
    fs::create_dir_all(root).map_err(io_err)?;
    // Manifest last-but-atomic: written to a temporary name, then renamed.
    let tmp = root.join(format!("{MANIFEST}.tmp"));
    fs::write(&tmp, manifest.to_toml()).map_err(io_err)?;
    fs::rename(&tmp, root.join(MANIFEST)).map_err(io_err)
}

async fn blocking<T: Send + 'static>(
    f: impl FnOnce() -> Result<T, CommandError> + Send + 'static,
) -> Result<T, CommandError> {
    tokio::task::spawn_blocking(f).await.map_err(io_err)?
}

// Handlers are `async` because the bus calls them as futures; quick ones (single SQLite reads)
// do not need to await anything.
#[allow(clippy::unused_async, clippy::unused_async_trait_impl)]
impl Workspace {
    /// A workspace with nothing open.
    #[must_use]
    pub fn new(dirs: Arc<dyn AppDirs>) -> Arc<Self> {
        Arc::new(Self {
            dirs,
            open: RwLock::new(None),
            bus: RwLock::new(Weak::new()),
        })
    }

    /// The open library or project, if any.
    #[must_use]
    pub fn opened(&self) -> Option<Arc<Opened>> {
        self.open.read().clone()
    }

    /// Path of a stored file in the open project (for the `ep://blob` protocol and the
    /// Document engine). `None` if nothing is open or the blob is absent.
    #[must_use]
    pub fn blob_path(&self, hash: &Hash) -> Option<PathBuf> {
        let o = self.opened()?;
        let p = o.blobs.path_of(hash);
        p.exists().then_some(p)
    }

    fn require(&self) -> Result<Arc<Opened>, CommandError> {
        self.opened().ok_or_else(|| {
            CommandError::failed(errors::NO_PROJECT, "no library or project is open")
        })
    }

    fn activate(&self, cx: &Cx, opened: Opened) -> Result<ProjectInfo, CommandError> {
        let opened = Arc::new(opened);
        let info = opened.info()?;
        opened.record(
            cx,
            events::OPENED,
            json!({ "id": info.id, "format": info.format }),
        );
        if let Some(bus) = self.bus.read().upgrade() {
            bus.set_provenance_log(Some(opened.events.clone()));
            bus.events().publish(
                events::OPENED,
                serde_json::to_value(&info).unwrap_or_default(),
                cx.clock.now_ms(),
            );
        }
        *self.open.write() = Some(opened);
        Ok(info)
    }

    fn publish(&self, cx: &Cx, kind: &str, paper: &Paper) {
        if let Some(bus) = self.bus.read().upgrade() {
            bus.events().publish(
                kind,
                serde_json::to_value(paper).unwrap_or_default(),
                cx.clock.now_ms(),
            );
        }
    }

    /// Register every `workspace.*` command on `bus`.
    ///
    /// # Errors
    /// A name is already registered.
    #[allow(clippy::too_many_lines)] // a table of command registrations
    pub fn register(self: &Arc<Self>, bus: &Arc<Bus>) -> Result<(), RegisterError> {
        *self.bus.write() = Arc::downgrade(bus);
        macro_rules! reg {
            ($name:expr, $summary:expr, $perm:expr, $method:ident) => {{
                let w = self.clone();
                let spec = Spec {
                    name: $name,
                    summary: $summary,
                    stability: Stability::Experimental,
                    permission: $perm,
                };
                bus.register(spec, move |cx, i| {
                    let w = w.clone();
                    async move { w.$method(cx, i).await }
                })?;
            }};
        }
        reg!(
            cmd::CREATE_PROJECT,
            "Create a library or project folder and open it",
            Permission::create(),
            create_project
        );
        reg!(
            cmd::OPEN_PROJECT,
            "Open a library or project folder",
            Permission::read(),
            open_project
        );
        reg!(
            cmd::OPEN_DEFAULT_LIBRARY,
            "Open the default library, creating it on first run",
            Permission::create(),
            open_default_library
        );
        reg!(
            cmd::CURRENT,
            "The open library or project, if any",
            Permission::read(),
            current
        );
        reg!(
            cmd::CLASSIFY,
            "Tell what kind of file this is (by content)",
            Permission::read(),
            classify_file
        );
        reg!(
            cmd::ADD_FILE,
            "Add a PDF as a managed copy (the original is never changed)",
            Permission::create(),
            add_file
        );
        reg!(cmd::GET_PAPER, "One paper", Permission::read(), get_paper);
        reg!(
            cmd::LIST_PAPERS,
            "Papers: filter by collection, tag or words; sort by added, opened or title",
            Permission::read(),
            list_papers
        );
        reg!(
            cmd::SET_PAPER_METADATA,
            "Record title, authors, DOI or page count for a paper",
            Permission::create(),
            set_paper_metadata
        );
        reg!(
            cmd::CREATE_COLLECTION,
            "Make a collection",
            Permission::create(),
            create_collection
        );
        reg!(
            cmd::RENAME_COLLECTION,
            "Rename a collection",
            Permission::create(),
            rename_collection
        );
        reg!(
            cmd::DELETE_COLLECTION,
            "Delete a collection (its papers stay in the library)",
            Permission::destructive(),
            delete_collection
        );
        reg!(
            cmd::LIST_COLLECTIONS,
            "Collections, by name",
            Permission::read(),
            list_collections
        );
        reg!(
            cmd::ADD_TO_COLLECTION,
            "Put papers in a collection",
            Permission::create(),
            add_to_collection
        );
        reg!(
            cmd::REMOVE_FROM_COLLECTION,
            "Take papers out of a collection (they stay in the library)",
            Permission::create(),
            remove_from_collection
        );
        reg!(
            cmd::SET_TAGS,
            "Replace a paper's tags",
            Permission::create(),
            set_tags
        );
        reg!(
            cmd::LIST_TAGS,
            "Every tag in use, with counts",
            Permission::read(),
            list_tags
        );
        reg!(
            cmd::GET_READING_STATE,
            "Where the reader was in a paper, its zoom, layout and bookmarks",
            Permission::read(),
            get_reading_state
        );
        reg!(
            cmd::SAVE_READING_STATE,
            "Remember where the reader is in a paper",
            Permission::create(),
            save_reading_state
        );
        reg!(
            cmd::ADD_BOOKMARK,
            "Add a named place in a paper",
            Permission::create(),
            add_bookmark
        );
        reg!(
            cmd::REMOVE_BOOKMARK,
            "Remove a named place",
            Permission::create(),
            remove_bookmark
        );
        reg!(
            cmd::RECORD_READING,
            "Count active reading time on a visible page",
            Permission::create(),
            record_reading
        );
        reg!(
            cmd::READING_STATS,
            "Reading time and pages read for a paper",
            Permission::read(),
            reading_stats
        );
        Ok(())
    }

    async fn create_project(
        self: Arc<Self>,
        cx: Cx,
        i: CreateProject,
    ) -> Result<ProjectInfo, CommandError> {
        let name = clean_name(&i.name).ok_or_else(|| CommandError::InvalidInput {
            message: "name is empty or too long".into(),
        })?;
        let manifest = Manifest::new(
            ProjectId::from_uuid(cx.ids.next()),
            i.kind,
            &name,
            cx.clock.now_ms(),
        );
        let root = PathBuf::from(&i.folder);
        let opened = blocking(move || {
            create_folder(&root, &manifest)?;
            open_folder(&root)
        })
        .await?;
        self.activate(&cx, opened)
    }

    async fn open_project(
        self: Arc<Self>,
        cx: Cx,
        i: OpenProject,
    ) -> Result<ProjectInfo, CommandError> {
        let root = PathBuf::from(&i.folder);
        let opened = blocking(move || open_folder(&root)).await?;
        self.activate(&cx, opened)
    }

    async fn open_default_library(
        self: Arc<Self>,
        cx: Cx,
        (): (),
    ) -> Result<ProjectInfo, CommandError> {
        let root = self.dirs.data().join("Library");
        let manifest = Manifest::new(
            ProjectId::from_uuid(cx.ids.next()),
            ProjectKind::Library,
            "My Library",
            cx.clock.now_ms(),
        );
        let opened = blocking(move || {
            if !root.join(MANIFEST).exists() {
                create_folder(&root, &manifest)?;
            }
            open_folder(&root)
        })
        .await?;
        self.activate(&cx, opened)
    }

    async fn current(
        self: Arc<Self>,
        _cx: Cx,
        (): (),
    ) -> Result<Option<ProjectInfo>, CommandError> {
        self.opened().map(|o| o.info()).transpose()
    }

    async fn classify_file(
        self: Arc<Self>,
        _cx: Cx,
        i: FilePath,
    ) -> Result<Classification, CommandError> {
        blocking(move || {
            let (head, size) = read_head(Path::new(&i.path))?;
            Ok(Classification {
                kind: classify(&head),
                size,
            })
        })
        .await
    }

    async fn add_file(self: Arc<Self>, cx: Cx, i: FilePath) -> Result<Added, CommandError> {
        let o = self.require()?;
        let id = PaperId::from_uuid(cx.ids.next());
        let now = cx.clock.now_ms();
        let o2 = o.clone();
        let added = blocking(move || store_file(&o2, &PathBuf::from(&i.path), id, now)).await?;
        if !added.already_present {
            o.record(&cx, events::PAPER_ADDED, json!({ "paper": added.paper.id, "hash": added.paper.file.hash, "size": added.paper.file.size }));
            self.publish(&cx, events::PAPER_ADDED, &added.paper);
        }
        Ok(added)
    }

    async fn get_paper(self: Arc<Self>, _cx: Cx, i: GetPaper) -> Result<Paper, CommandError> {
        self.require()?
            .paper(i.id)?
            .ok_or_else(|| CommandError::failed(errors::NOT_FOUND, format!("paper {}", i.id)))
    }

    async fn set_paper_metadata(
        self: Arc<Self>,
        cx: Cx,
        i: SetPaperMetadata,
    ) -> Result<Paper, CommandError> {
        let o = self.require()?;
        let authors = i
            .authors
            .as_ref()
            .map(|a| serde_json::to_string(a).unwrap_or_else(|_| "[]".into()));
        let doi = i.doi.as_ref().map(|d| d.trim().to_lowercase());
        let changed = o
            .db
            .write(|t| {
                t.execute(
                    "UPDATE paper SET title = coalesce(?2, title), authors = coalesce(?3, authors), doi = coalesce(?4, doi), pages = coalesce(?5, pages), metadata_source = ?6 WHERE id = ?1",
                    params![i.id.to_string(), i.title, authors, doi, i.pages, i.source],
                )
            })
            .map_err(sql_err)?;
        let missing = || CommandError::failed(errors::NOT_FOUND, format!("paper {}", i.id));
        if changed == 0 {
            return Err(missing());
        }
        let paper = o.paper(i.id)?.ok_or_else(missing)?;
        o.record(
            &cx,
            events::PAPER_CHANGED,
            json!({ "paper": i.id, "source": i.source }),
        );
        self.publish(&cx, events::PAPER_CHANGED, &paper);
        Ok(paper)
    }
}

/// First 1024 bytes and the size of a file.
fn read_head(path: &Path) -> Result<(Vec<u8>, u64), CommandError> {
    let mut f = fs::File::open(path)
        .map_err(|e| CommandError::failed(errors::NOT_FOUND, format!("{}: {e}", path.display())))?;
    let size = f.metadata().map_err(io_err)?.len();
    let mut head = Vec::with_capacity(1024);
    f.by_ref()
        .take(1024)
        .read_to_end(&mut head)
        .map_err(io_err)?;
    Ok((head, size))
}

/// Copy a PDF into the blob store and record it (or return the paper with the same bytes).
fn store_file(o: &Opened, path: &Path, id: PaperId, now: i64) -> Result<Added, CommandError> {
    let (head, _) = read_head(path)?;
    if classify(&head) != FileKind::Pdf {
        return Err(CommandError::failed(
            errors::UNSUPPORTED_FILE,
            format!("{} is not a PDF", path.display()),
        ));
    }
    let blob = o.blobs.put_file(path, MediaType::pdf()).map_err(io_err)?;
    if let Some(existing) = o.paper_by_hash(&blob.hash)? {
        return Ok(Added {
            paper: existing,
            already_present: true,
        });
    }
    let name = path.file_name().map_or_else(
        || "paper.pdf".to_string(),
        |n| n.to_string_lossy().into_owned(),
    );
    let original = ep_os::paths::canonical(path).unwrap_or_else(|_| path.to_path_buf());
    let paper = Paper {
        id,
        file: blob,
        original_name: name,
        original_path: Some(original.display().to_string()),
        title: None,
        authors: vec![],
        doi: None,
        pages: None,
        metadata_source: None,
        tags: vec![],
        added_ms: now,
        opened_ms: None,
    };
    o.db.write(|t| {
        t.execute(
            "INSERT INTO paper (id, hash, size, media_type, original_name, original_path, added_ms) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                paper.id.to_string(),
                paper.file.hash.to_hex(),
                i64::try_from(paper.file.size).unwrap_or(i64::MAX),
                paper.file.media_type.0,
                paper.original_name,
                paper.original_path,
                paper.added_ms
            ],
        )
    })
    .map_err(sql_err)?;
    Ok(Added {
        paper,
        already_present: false,
    })
}
