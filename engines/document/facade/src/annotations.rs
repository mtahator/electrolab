//! Annotation commands (P1.M3.MOD.2–4, P1.M3.PDF.1–3; ADR-006).
//!
//! The Document engine keeps annotations in its own database, `document.sqlite`, inside the open
//! library or project folder (ADR-010, amendment 1). Every edit is stored with its inverse, so
//! undo and redo survive restarts. "Save into PDF" writes a new file (incremental update) into
//! the library's blob store; the host then points the paper at it (`workspace.set_paper_file`).

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::sync::Arc;

use ep_bus::{CommandError, Cx};
use ep_document_contract::{
    Annotated, Annotation, AnnotationSet, AnnotationsOf, ApplyAnnotations, PaperFile,
    annotation_errors as aerr, annotation_events, errors,
};
use ep_document_core::annotations::{self as core, Change, EditError};
use ep_store::rusqlite::{self, OptionalExtension, params};
use ep_store::{Migration, Store};
use ep_types::{MediaType, PaperId};
use serde_json::json;

use crate::{Document, blocking};

/// File name of the engine's database inside the project folder.
pub const DATABASE: &str = "document.sqlite";

/// Edits kept per paper (older ones can no longer be undone).
const HISTORY: i64 = 1000;

const MIGRATIONS: &[Migration] = &[Migration {
    version: 1,
    name: "annotations",
    sql: "
CREATE TABLE annotation (
    paper TEXT NOT NULL, id TEXT NOT NULL, page INTEGER NOT NULL,
    data TEXT NOT NULL, PRIMARY KEY (paper, id)) WITHOUT ROWID;
-- One row per change; an edit (step) is undone or redone as a whole.
CREATE TABLE annotation_edit (
    paper TEXT NOT NULL, step INTEGER NOT NULL, seq INTEGER NOT NULL, id TEXT NOT NULL,
    before TEXT, after TEXT, undone INTEGER NOT NULL DEFAULT 0, at_ms INTEGER NOT NULL,
    PRIMARY KEY (paper, step, seq)) WITHOUT ROWID;
-- Every id ever used for the paper: on save, file objects with these ids are ours to replace.
CREATE TABLE annotation_seen (paper TEXT NOT NULL, id TEXT NOT NULL,
    PRIMARY KEY (paper, id)) WITHOUT ROWID;
-- Files whose annotations are already in the model (imported, or written by a save).
CREATE TABLE annotation_file (paper TEXT NOT NULL, hash TEXT NOT NULL, at_ms INTEGER NOT NULL,
    PRIMARY KEY (paper, hash)) WITHOUT ROWID;
CREATE TABLE annotation_paper (paper TEXT PRIMARY KEY, unsaved INTEGER NOT NULL DEFAULT 0)
    WITHOUT ROWID;
",
}];

fn sql(e: rusqlite::Error) -> CommandError {
    CommandError::failed(errors::UNREADABLE, e)
}

/// A command error that SQLite errors convert into (for `Store::write` closures).
struct Fail(CommandError);

impl From<rusqlite::Error> for Fail {
    fn from(e: rusqlite::Error) -> Self {
        Self(sql(e))
    }
}

impl From<CommandError> for Fail {
    fn from(e: CommandError) -> Self {
        Self(e)
    }
}

fn edit_err(e: EditError) -> CommandError {
    match e {
        EditError::Invalid(m) => CommandError::failed(aerr::INVALID, m),
        EditError::Conflict(m) => CommandError::failed(aerr::CONFLICT, m),
    }
}

fn to_json(a: &Annotation) -> String {
    serde_json::to_string(a).unwrap_or_default()
}

fn from_json(s: &str) -> Option<Annotation> {
    serde_json::from_str(s).ok()
}

/// Open the database of a project folder.
///
/// # Errors
/// The file cannot be opened or was written by a newer app.
pub fn open_store(folder: &Path) -> Result<Store, CommandError> {
    Store::open(folder.join(DATABASE), MIGRATIONS)
        .map_err(|e| CommandError::failed(aerr::NO_PROJECT_OPEN, e))
}

fn load(c: &rusqlite::Connection, paper: &str) -> rusqlite::Result<BTreeMap<String, Annotation>> {
    let mut st = c.prepare_cached("SELECT data FROM annotation WHERE paper = ?1")?;
    let rows = st.query_map([paper], |r| r.get::<_, String>(0))?;
    let mut out = BTreeMap::new();
    for r in rows {
        if let Some(a) = from_json(&r?) {
            out.insert(a.id.clone(), a);
        }
    }
    Ok(out)
}

fn put(
    c: &rusqlite::Connection,
    paper: &str,
    id: &str,
    a: Option<&Annotation>,
) -> rusqlite::Result<()> {
    match a {
        Some(a) => {
            c.execute(
                "INSERT INTO annotation (paper, id, page, data) VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT (paper, id) DO UPDATE SET page = excluded.page, data = excluded.data",
                params![paper, id, a.page, to_json(a)],
            )?;
            c.execute(
                "INSERT OR IGNORE INTO annotation_seen (paper, id) VALUES (?1, ?2)",
                params![paper, id],
            )?;
        }
        None => {
            c.execute(
                "DELETE FROM annotation WHERE paper = ?1 AND id = ?2",
                params![paper, id],
            )?;
        }
    }
    Ok(())
}

fn set_unsaved(c: &rusqlite::Connection, paper: &str, unsaved: bool) -> rusqlite::Result<()> {
    c.execute(
        "INSERT INTO annotation_paper (paper, unsaved) VALUES (?1, ?2)
         ON CONFLICT (paper) DO UPDATE SET unsaved = excluded.unsaved",
        params![paper, i64::from(unsaved)],
    )?;
    Ok(())
}

fn state(c: &rusqlite::Connection, paper: PaperId) -> rusqlite::Result<AnnotationSet> {
    let p = paper.to_string();
    let set = load(c, &p)?;
    let can_undo: bool = c.query_row(
        "SELECT EXISTS (SELECT 1 FROM annotation_edit WHERE paper = ?1 AND undone = 0)",
        [&p],
        |r| r.get(0),
    )?;
    let can_redo: bool = c.query_row(
        "SELECT EXISTS (SELECT 1 FROM annotation_edit WHERE paper = ?1 AND undone = 1)",
        [&p],
        |r| r.get(0),
    )?;
    let unsaved: bool = c
        .query_row(
            "SELECT unsaved FROM annotation_paper WHERE paper = ?1",
            [&p],
            |r| r.get(0),
        )
        .optional()?
        .unwrap_or(false);
    Ok(AnnotationSet {
        paper,
        annotations: core::ordered(&set),
        can_undo,
        can_redo,
        unsaved,
    })
}

fn changes_of(c: &rusqlite::Connection, paper: &str, step: i64) -> rusqlite::Result<Vec<Change>> {
    let mut st = c.prepare_cached(
        "SELECT id, before, after FROM annotation_edit WHERE paper = ?1 AND step = ?2 ORDER BY seq",
    )?;
    let rows = st.query_map(params![paper, step], |r| {
        Ok(Change {
            id: r.get(0)?,
            before: r
                .get::<_, Option<String>>(1)?
                .as_deref()
                .and_then(from_json),
            after: r
                .get::<_, Option<String>>(2)?
                .as_deref()
                .and_then(from_json),
        })
    })?;
    rows.collect()
}

impl Document {
    /// The database of the open project (opened on first use, reopened when the project changes).
    fn annotation_store(&self) -> Result<Arc<Store>, CommandError> {
        let (id, folder) = self.blobs.project().ok_or_else(|| {
            CommandError::failed(aerr::NO_PROJECT_OPEN, "no library or project is open")
        })?;
        let mut cur = self.annotation_db.lock();
        if let Some((open_id, store)) = cur.as_ref()
            && *open_id == id
        {
            return Ok(store.clone());
        }
        let store = Arc::new(open_store(&folder)?);
        *cur = Some((id, store.clone()));
        Ok(store)
    }

    fn changed(&self, cx: &Cx, paper: PaperId) {
        if let Some(bus) = self.bus.lock().upgrade() {
            bus.events().publish(
                annotation_events::CHANGED,
                json!({ "paper": paper }),
                cx.clock.now_ms(),
            );
        }
    }

    pub(crate) async fn annotations(
        self: Arc<Self>,
        cx: Cx,
        i: PaperFile,
    ) -> Result<AnnotationSet, CommandError> {
        let path = self.path(&i.file)?;
        let store = self.annotation_store()?;
        let me = self.clone();
        let now = cx.clock.now_ms();
        blocking(move || {
            let p = i.paper.to_string();
            let hash = i.file.hash.to_string();
            let known: bool = store
                .read(|c| {
                    c.query_row(
                        "SELECT EXISTS (SELECT 1 FROM annotation_file WHERE paper = ?1 AND hash = ?2)",
                        params![p, hash],
                        |r| r.get(0),
                    )
                })
                .map_err(sql)?;
            if !known {
                // First time this file is seen for the paper: import what is already in it.
                let (g, _) = me.geoms(i.file.hash, &path)?;
                let found = match ep_document_adapter_lopdf::read_annotations(&path) {
                    Ok(raws) => core::import(&raws, &g, now),
                    Err(e) => {
                        tracing::warn!(%e, "annotations in the PDF could not be read");
                        Vec::new()
                    }
                };
                store
                    .write(|c| {
                        let existing = load(c, &p)?;
                        let seen: BTreeSet<String> = {
                            let mut st = c.prepare_cached(
                                "SELECT id FROM annotation_seen WHERE paper = ?1",
                            )?;
                            st.query_map([&p], |r| r.get(0))?
                                .collect::<rusqlite::Result<_>>()?
                        };
                        for a in &found {
                            // Ids we know (ours, or deleted by the user) are not imported again.
                            if !existing.contains_key(&a.id) && !seen.contains(&a.id) {
                                put(c, &p, &a.id, Some(a))?;
                            }
                        }
                        c.execute(
                            "INSERT OR IGNORE INTO annotation_file (paper, hash, at_ms) VALUES (?1, ?2, ?3)",
                            params![p, hash, now],
                        )?;
                        Ok::<_, rusqlite::Error>(())
                    })
                    .map_err(sql)?;
            }
            store.read(|c| state(c, i.paper)).map_err(sql)
        })
        .await
    }

    pub(crate) async fn annotations_apply(
        self: Arc<Self>,
        cx: Cx,
        i: ApplyAnnotations,
    ) -> Result<AnnotationSet, CommandError> {
        let store = self.annotation_store()?;
        let now = cx.clock.now_ms();
        let out = blocking(move || {
            let p = i.paper.to_string();
            store
                .write(|c| -> Result<AnnotationSet, Fail> {
                    let mut set = load(c, &p)?;
                    // Page bounds come from the annotations' own pages: the viewer only offers
                    // existing pages, and the PDF writer refuses pages that do not exist.
                    let changes = core::apply(&mut set, i.ops, u32::MAX, now).map_err(edit_err)?;
                    if changes.is_empty() {
                        return Ok(state(c, i.paper)?);
                    }
                    // A new edit discards whatever could have been redone.
                    c.execute(
                        "DELETE FROM annotation_edit WHERE paper = ?1 AND undone = 1",
                        [&p],
                    )?;
                    let step: i64 = c.query_row(
                        "SELECT coalesce(max(step), 0) + 1 FROM annotation_edit WHERE paper = ?1",
                        [&p],
                        |r| r.get(0),
                    )?;
                    for (seq, ch) in changes.iter().enumerate() {
                        put(c, &p, &ch.id, ch.after.as_ref())?;
                        c.execute(
                        "INSERT INTO annotation_edit (paper, step, seq, id, before, after, at_ms)
                         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                        params![
                            p,
                            step,
                            i64::try_from(seq).unwrap_or(i64::MAX),
                            ch.id,
                            ch.before.as_ref().map(to_json),
                            ch.after.as_ref().map(to_json),
                            now
                        ],
                    )
                    ?;
                    }
                    c.execute(
                        "DELETE FROM annotation_edit WHERE paper = ?1 AND step <= ?2",
                        params![p, step - HISTORY],
                    )?;
                    set_unsaved(c, &p, true)?;
                    Ok(state(c, i.paper)?)
                })
                .map_err(|f| f.0)
        })
        .await?;
        self.changed(&cx, out.paper);
        Ok(out)
    }

    pub(crate) async fn annotations_undo(
        self: Arc<Self>,
        cx: Cx,
        i: AnnotationsOf,
    ) -> Result<AnnotationSet, CommandError> {
        self.history(cx, i.paper, true).await
    }

    pub(crate) async fn annotations_redo(
        self: Arc<Self>,
        cx: Cx,
        i: AnnotationsOf,
    ) -> Result<AnnotationSet, CommandError> {
        self.history(cx, i.paper, false).await
    }

    async fn history(
        self: Arc<Self>,
        cx: Cx,
        paper: PaperId,
        undo: bool,
    ) -> Result<AnnotationSet, CommandError> {
        let store = self.annotation_store()?;
        let (out, moved) = blocking(move || {
            let p = paper.to_string();
            store
                .write(|c| {
                    let step: Option<i64> = c.query_row(
                        if undo {
                            "SELECT max(step) FROM annotation_edit WHERE paper = ?1 AND undone = 0"
                        } else {
                            "SELECT min(step) FROM annotation_edit WHERE paper = ?1 AND undone = 1"
                        },
                        [&p],
                        |r| r.get(0),
                    )?;
                    let Some(step) = step else {
                        return Ok((state(c, paper)?, false));
                    };
                    let changes = changes_of(c, &p, step)?;
                    let mut set = load(c, &p)?;
                    if undo {
                        core::revert(&mut set, &changes);
                    } else {
                        core::replay(&mut set, &changes);
                    }
                    for ch in &changes {
                        put(c, &p, &ch.id, set.get(&ch.id))?;
                    }
                    c.execute(
                        "UPDATE annotation_edit SET undone = ?3 WHERE paper = ?1 AND step = ?2",
                        params![p, step, i64::from(undo)],
                    )?;
                    set_unsaved(c, &p, true)?;
                    Ok::<_, rusqlite::Error>((state(c, paper)?, true))
                })
                .map_err(sql)
        })
        .await?;
        if moved {
            self.changed(&cx, paper);
        }
        Ok(out)
    }

    pub(crate) async fn annotate(
        self: Arc<Self>,
        cx: Cx,
        i: PaperFile,
    ) -> Result<Annotated, CommandError> {
        let path = self.path(&i.file)?;
        let store = self.annotation_store()?;
        let me = self.clone();
        let now = cx.clock.now_ms();
        let author = {
            let a = self.author.read().clone();
            if a.is_empty() {
                "Reader".to_string()
            } else {
                a
            }
        };
        let out = blocking(move || {
            let p = i.paper.to_string();
            let (set, managed) = store
                .read(|c| {
                    let set = load(c, &p)?;
                    let mut st =
                        c.prepare_cached("SELECT id FROM annotation_seen WHERE paper = ?1")?;
                    let seen: BTreeSet<String> = st
                        .query_map([&p], |r| r.get(0))?
                        .collect::<rusqlite::Result<_>>()?;
                    Ok::<_, rusqlite::Error>((set, seen))
                })
                .map_err(sql)?;
            let (g, _) = me.geoms(i.file.hash, &path)?;
            let list = core::ordered(&set);
            if let Some(a) = list.iter().find(|a| a.page as usize > g.len()) {
                return Err(CommandError::failed(
                    aerr::INVALID,
                    format!("annotation {} is on page {}, which the PDF does not have", a.id, a.page),
                ));
            }
            let plan = core::plan(&list, &g, &managed, &author);
            let mut bytes = Vec::new();
            let report = ep_document_adapter_lopdf::write_annotations(&path, &plan, &mut bytes)
                .map_err(|e| CommandError::failed(aerr::WRITE_FAILED, e))?;
            let file = me
                .blobs
                .put(&bytes, MediaType::pdf())
                .map_err(|e| CommandError::failed(aerr::WRITE_FAILED, e))?;
            // The new file holds exactly the model: everything is pristine and saved.
            store
                .write(|c| {
                    for mut a in list {
                        a.pristine = true;
                        put(c, &p, &a.id.clone(), Some(&a))?;
                    }
                    c.execute(
                        "INSERT OR IGNORE INTO annotation_file (paper, hash, at_ms) VALUES (?1, ?2, ?3)",
                        params![p, file.hash.to_string(), now],
                    )?;
                    set_unsaved(c, &p, false)?;
                    Ok::<_, rusqlite::Error>(())
                })
                .map_err(sql)?;
            Ok(Annotated {
                file,
                written: report.written,
                kept: report.kept,
            })
        })
        .await?;
        self.changed(&cx, i.paper);
        Ok(out)
    }
}
