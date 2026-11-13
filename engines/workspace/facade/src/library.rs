//! Library commands (P1.M2.WSP.1–3): collections, tags, search, reading state, bookmarks and
//! reading statistics.

use std::fmt::Write as _;
use std::sync::Arc;

use ep_bus::{CommandError, Cx};
use ep_store::rusqlite::{self, OptionalExtension, params, params_from_iter};
use ep_types::{CollectionId, Location, ObjectId, PaperId, ZoomSpec};
use ep_workspace_contract::{
    AddBookmark, Bookmark, BookmarkRef, Collection, CollectionPapers, CollectionRef,
    CreateCollection, ListPapers, Paper, PaperRef, PaperSort, ReadingState, ReadingStats,
    RecordReading, RenameCollection, SaveReadingState, SetTags, TagCount, ViewMode, errors,
};
use ep_workspace_core::{MAX_READING_REPORT_S, clean_name, clean_tags, like_patterns};
use serde_json::json;

use super::{PAPER_SELECT, Workspace, row_to_paper, sql_err};

fn not_found(what: impl std::fmt::Display) -> CommandError {
    CommandError::failed(errors::NOT_FOUND, what)
}

fn row_to_collection(r: &rusqlite::Row<'_>) -> rusqlite::Result<Collection> {
    let id: String = r.get("id")?;
    Ok(Collection {
        id: id.parse().map_err(|e: ep_types::IdParseError| {
            rusqlite::Error::FromSqlConversionFailure(
                0,
                rusqlite::types::Type::Text,
                e.to_string().into(),
            )
        })?,
        name: r.get("name")?,
        papers: u64::try_from(r.get::<_, i64>("papers")?).unwrap_or(0),
        created_ms: r.get("created_ms")?,
    })
}

const COLLECTION_SELECT: &str = "SELECT c.*, (SELECT count(*) FROM collection_paper cp WHERE cp.collection = c.id) AS papers FROM collection c";

fn view_mode_str(v: ViewMode) -> &'static str {
    match v {
        ViewMode::Continuous => "continuous",
        ViewMode::Single => "single",
        ViewMode::TwoPage => "two_page",
    }
}

fn view_mode_from(s: &str) -> ViewMode {
    match s {
        "single" => ViewMode::Single,
        "two_page" => ViewMode::TwoPage,
        _ => ViewMode::Continuous,
    }
}

fn is_unique_violation(e: &rusqlite::Error) -> bool {
    matches!(e, rusqlite::Error::SqliteFailure(f, _) if f.extended_code == 2067 || f.extended_code == 1555)
}

// Handlers are `async` because the bus calls them as futures; most are single short SQLite calls.
#[allow(clippy::unused_async, clippy::unused_async_trait_impl)]
impl Workspace {
    pub(crate) async fn list_papers(
        self: Arc<Self>,
        _cx: Cx,
        i: ListPapers,
    ) -> Result<Vec<Paper>, CommandError> {
        let o = self.require()?;
        let mut sql = format!("{PAPER_SELECT} WHERE 1 = 1");
        let mut args: Vec<String> = Vec::new();
        if let Some(c) = i.collection {
            sql.push_str(" AND p.id IN (SELECT paper FROM collection_paper WHERE collection = ?)");
            args.push(c.to_string());
        }
        if let Some(t) = i.tag.as_deref().map(str::trim).filter(|t| !t.is_empty()) {
            sql.push_str(" AND p.id IN (SELECT paper FROM paper_tag WHERE tag = ? COLLATE NOCASE)");
            args.push(t.to_string());
        }
        for pat in like_patterns(i.query.as_deref().unwrap_or_default()) {
            sql.push_str(" AND lower(coalesce(p.title, '') || ' ' || p.authors || ' ' || coalesce(p.doi, '') || ' ' || p.original_name) LIKE ? ESCAPE '!'");
            args.push(pat);
        }
        sql.push_str(match i.sort {
            PaperSort::Added => " ORDER BY p.added_ms DESC, p.id",
            PaperSort::Opened => {
                " ORDER BY p.opened_ms IS NULL, p.opened_ms DESC, p.added_ms DESC, p.id"
            }
            PaperSort::Title => " ORDER BY lower(coalesce(p.title, p.original_name)), p.id",
        });
        let limit = i.limit.unwrap_or(200).min(10_000);
        let _ = write!(sql, " LIMIT {limit} OFFSET {}", i.offset);
        o.db.read(|c| {
            let mut s = c.prepare(&sql)?;
            s.query_map(params_from_iter(args.iter()), row_to_paper)?
                .collect::<rusqlite::Result<Vec<_>>>()
        })
        .map_err(sql_err)
    }

    fn collection(&self, id: CollectionId) -> Result<Collection, CommandError> {
        self.require()?
            .db
            .read(|c| {
                c.query_row(
                    &format!("{COLLECTION_SELECT} WHERE c.id = ?1"),
                    [id.to_string()],
                    row_to_collection,
                )
                .optional()
            })
            .map_err(sql_err)?
            .ok_or_else(|| not_found(format!("collection {id}")))
    }

    pub(crate) async fn create_collection(
        self: Arc<Self>,
        cx: Cx,
        i: CreateCollection,
    ) -> Result<Collection, CommandError> {
        let o = self.require()?;
        let name = clean_name(&i.name).ok_or_else(|| CommandError::InvalidInput {
            message: "collection name is empty or too long".into(),
        })?;
        let id = CollectionId::from_uuid(cx.ids.next());
        let now = cx.clock.now_ms();
        o.db.write(|t| {
            t.execute(
                "INSERT INTO collection (id, name, created_ms) VALUES (?1, ?2, ?3)",
                params![id.to_string(), name, now],
            )
        })
        .map_err(|e| {
            if is_unique_violation(&e) {
                CommandError::failed(
                    "exists",
                    format!("a collection named {name:?} already exists"),
                )
            } else {
                sql_err(e)
            }
        })?;
        o.record(
            &cx,
            "workspace.collection_created",
            json!({ "collection": id, "name": name }),
        );
        self.collection(id)
    }

    pub(crate) async fn rename_collection(
        self: Arc<Self>,
        cx: Cx,
        i: RenameCollection,
    ) -> Result<Collection, CommandError> {
        let o = self.require()?;
        let name = clean_name(&i.name).ok_or_else(|| CommandError::InvalidInput {
            message: "collection name is empty or too long".into(),
        })?;
        let n =
            o.db.write(|t| {
                t.execute(
                    "UPDATE collection SET name = ?2 WHERE id = ?1",
                    params![i.id.to_string(), name],
                )
            })
            .map_err(|e| {
                if is_unique_violation(&e) {
                    CommandError::failed(
                        "exists",
                        format!("a collection named {name:?} already exists"),
                    )
                } else {
                    sql_err(e)
                }
            })?;
        if n == 0 {
            return Err(not_found(format!("collection {}", i.id)));
        }
        o.record(
            &cx,
            "workspace.collection_renamed",
            json!({ "collection": i.id, "name": name }),
        );
        self.collection(i.id)
    }

    pub(crate) async fn delete_collection(
        self: Arc<Self>,
        cx: Cx,
        i: CollectionRef,
    ) -> Result<(), CommandError> {
        let o = self.require()?;
        let n =
            o.db.write(|t| t.execute("DELETE FROM collection WHERE id = ?1", [i.id.to_string()]))
                .map_err(sql_err)?;
        if n == 0 {
            return Err(not_found(format!("collection {}", i.id)));
        }
        o.record(
            &cx,
            "workspace.collection_deleted",
            json!({ "collection": i.id }),
        );
        Ok(())
    }

    pub(crate) async fn list_collections(
        self: Arc<Self>,
        _cx: Cx,
        (): (),
    ) -> Result<Vec<Collection>, CommandError> {
        let o = self.require()?;
        o.db.read(|c| {
            let mut s = c.prepare(&format!("{COLLECTION_SELECT} ORDER BY lower(c.name)"))?;
            s.query_map([], row_to_collection)?
                .collect::<rusqlite::Result<Vec<_>>>()
        })
        .map_err(sql_err)
    }

    pub(crate) async fn add_to_collection(
        self: Arc<Self>,
        cx: Cx,
        i: CollectionPapers,
    ) -> Result<Collection, CommandError> {
        let o = self.require()?;
        self.collection(i.collection)?;
        let now = cx.clock.now_ms();
        o.db.write(|t| {
            for p in &i.papers {
                let exists: bool = t.query_row("SELECT EXISTS(SELECT 1 FROM paper WHERE id = ?1)", [p.to_string()], |r| r.get(0))?;
                if exists {
                    t.execute("INSERT OR IGNORE INTO collection_paper (collection, paper, added_ms) VALUES (?1, ?2, ?3)", params![i.collection.to_string(), p.to_string(), now])?;
                }
            }
            Ok::<_, rusqlite::Error>(())
        })
        .map_err(sql_err)?;
        o.record(
            &cx,
            "workspace.collection_changed",
            json!({ "collection": i.collection, "added": i.papers }),
        );
        self.collection(i.collection)
    }

    pub(crate) async fn remove_from_collection(
        self: Arc<Self>,
        cx: Cx,
        i: CollectionPapers,
    ) -> Result<Collection, CommandError> {
        let o = self.require()?;
        o.db.write(|t| {
            for p in &i.papers {
                t.execute(
                    "DELETE FROM collection_paper WHERE collection = ?1 AND paper = ?2",
                    params![i.collection.to_string(), p.to_string()],
                )?;
            }
            Ok::<_, rusqlite::Error>(())
        })
        .map_err(sql_err)?;
        o.record(
            &cx,
            "workspace.collection_changed",
            json!({ "collection": i.collection, "removed": i.papers }),
        );
        self.collection(i.collection)
    }

    pub(crate) async fn set_tags(
        self: Arc<Self>,
        cx: Cx,
        i: SetTags,
    ) -> Result<Paper, CommandError> {
        let o = self.require()?;
        o.paper(i.paper)?
            .ok_or_else(|| not_found(format!("paper {}", i.paper)))?;
        let tags = clean_tags(&i.tags);
        o.db.write(|t| {
            t.execute(
                "DELETE FROM paper_tag WHERE paper = ?1",
                [i.paper.to_string()],
            )?;
            for tag in &tags {
                t.execute(
                    "INSERT INTO paper_tag (paper, tag) VALUES (?1, ?2)",
                    params![i.paper.to_string(), tag],
                )?;
            }
            Ok::<_, rusqlite::Error>(())
        })
        .map_err(sql_err)?;
        o.record(
            &cx,
            "workspace.tags_set",
            json!({ "paper": i.paper, "tags": tags }),
        );
        let paper = o
            .paper(i.paper)?
            .ok_or_else(|| not_found(format!("paper {}", i.paper)))?;
        self.publish(&cx, ep_workspace_contract::events::PAPER_CHANGED, &paper);
        Ok(paper)
    }

    pub(crate) async fn list_tags(
        self: Arc<Self>,
        _cx: Cx,
        (): (),
    ) -> Result<Vec<TagCount>, CommandError> {
        let o = self.require()?;
        o.db.read(|c| {
            let mut s = c.prepare("SELECT tag, count(*) AS n FROM paper_tag GROUP BY tag COLLATE NOCASE ORDER BY lower(tag)")?;
            s.query_map([], |r| Ok(TagCount { tag: r.get(0)?, papers: u64::try_from(r.get::<_, i64>(1)?).unwrap_or(0) }))?.collect::<rusqlite::Result<Vec<_>>>()
        })
        .map_err(sql_err)
    }

    fn bookmarks(&self, paper: PaperId) -> Result<Vec<Bookmark>, CommandError> {
        let o = self.require()?;
        o.db.read(|c| {
            let mut s = c.prepare("SELECT id, name, location, created_ms FROM bookmark WHERE paper = ?1 ORDER BY created_ms, id")?;
            s.query_map([paper.to_string()], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?, r.get::<_, i64>(3)?)))?
                .collect::<rusqlite::Result<Vec<_>>>()
        })
        .map_err(sql_err)
        .map(|rows| {
            rows.into_iter()
                .filter_map(|(id, name, loc, created_ms)| Some(Bookmark { id: id.parse().ok()?, name, location: serde_json::from_str(&loc).ok()?, created_ms }))
                .collect()
        })
    }

    pub(crate) async fn get_reading_state(
        self: Arc<Self>,
        _cx: Cx,
        i: PaperRef,
    ) -> Result<ReadingState, CommandError> {
        let o = self.require()?;
        let paper = o
            .paper(i.paper)?
            .ok_or_else(|| not_found(format!("paper {}", i.paper)))?;
        let row =
            o.db.read(|c| {
                c.query_row(
                    "SELECT location, zoom, view_mode FROM reading_state WHERE paper = ?1",
                    [i.paper.to_string()],
                    |r| {
                        Ok((
                            r.get::<_, Option<String>>(0)?,
                            r.get::<_, Option<String>>(1)?,
                            r.get::<_, String>(2)?,
                        ))
                    },
                )
                .optional()
            })
            .map_err(sql_err)?;
        let (location, zoom, view_mode) = match row {
            Some((loc, zoom_json, mode)) => (
                loc.and_then(|text| serde_json::from_str::<Location>(&text).ok()),
                zoom_json.and_then(|text| serde_json::from_str::<ZoomSpec>(&text).ok()),
                view_mode_from(&mode),
            ),
            None => (None, None, ViewMode::default()),
        };
        Ok(ReadingState {
            paper: i.paper,
            location,
            zoom,
            view_mode,
            bookmarks: self.bookmarks(i.paper)?,
            opened_ms: paper.opened_ms,
        })
    }

    pub(crate) async fn save_reading_state(
        self: Arc<Self>,
        cx: Cx,
        i: SaveReadingState,
    ) -> Result<(), CommandError> {
        let o = self.require()?;
        let now = cx.clock.now_ms();
        let loc = i
            .location
            .as_ref()
            .map(|l| serde_json::to_string(l).unwrap_or_default());
        let zoom = i
            .zoom
            .as_ref()
            .map(|z| serde_json::to_string(z).unwrap_or_default());
        let n = o
            .db
            .write(|t| {
                let n = t.execute("UPDATE paper SET opened_ms = ?2 WHERE id = ?1", params![i.paper.to_string(), now])?;
                if n > 0 {
                    t.execute(
                        "INSERT INTO reading_state (paper, location, zoom, view_mode, updated_ms) VALUES (?1, ?2, ?3, ?4, ?5)
                         ON CONFLICT(paper) DO UPDATE SET location = excluded.location, zoom = excluded.zoom, view_mode = excluded.view_mode, updated_ms = excluded.updated_ms",
                        params![i.paper.to_string(), loc, zoom, view_mode_str(i.view_mode), now],
                    )?;
                }
                Ok::<_, rusqlite::Error>(n)
            })
            .map_err(sql_err)?;
        if n == 0 {
            return Err(not_found(format!("paper {}", i.paper)));
        }
        Ok(())
    }

    pub(crate) async fn add_bookmark(
        self: Arc<Self>,
        cx: Cx,
        i: AddBookmark,
    ) -> Result<Bookmark, CommandError> {
        let o = self.require()?;
        let paper = i.location.paper;
        o.paper(paper)?
            .ok_or_else(|| not_found(format!("paper {paper}")))?;
        let name: String = i
            .name
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .chars()
            .take(200)
            .collect();
        let name = if name.is_empty() {
            format!("Page {}", i.location.page)
        } else {
            name
        };
        let b = Bookmark {
            id: ObjectId::from_uuid(cx.ids.next()),
            name,
            location: i.location,
            created_ms: cx.clock.now_ms(),
        };
        let loc = serde_json::to_string(&b.location).unwrap_or_default();
        o.db.write(|t| t.execute("INSERT INTO bookmark (id, paper, name, location, created_ms) VALUES (?1, ?2, ?3, ?4, ?5)", params![b.id.to_string(), paper.to_string(), b.name, loc, b.created_ms]))
            .map_err(sql_err)?;
        o.record(
            &cx,
            "workspace.bookmark_added",
            json!({ "paper": paper, "bookmark": b.id }),
        );
        Ok(b)
    }

    pub(crate) async fn remove_bookmark(
        self: Arc<Self>,
        cx: Cx,
        i: BookmarkRef,
    ) -> Result<(), CommandError> {
        let o = self.require()?;
        let n =
            o.db.write(|t| t.execute("DELETE FROM bookmark WHERE id = ?1", [i.id.to_string()]))
                .map_err(sql_err)?;
        if n == 0 {
            return Err(not_found(format!("bookmark {}", i.id)));
        }
        o.record(
            &cx,
            "workspace.bookmark_removed",
            json!({ "bookmark": i.id }),
        );
        Ok(())
    }

    pub(crate) async fn record_reading(
        self: Arc<Self>,
        cx: Cx,
        i: RecordReading,
    ) -> Result<ReadingStats, CommandError> {
        let o = self.require()?;
        if i.page == 0 {
            return Err(CommandError::InvalidInput {
                message: "pages start at 1".into(),
            });
        }
        let now = cx.clock.now_ms();
        let secs = i64::from(i.seconds.min(MAX_READING_REPORT_S));
        let n = o
            .db
            .write(|t| {
                let exists: bool = t.query_row("SELECT EXISTS(SELECT 1 FROM paper WHERE id = ?1)", [i.paper.to_string()], |r| r.get(0))?;
                if exists {
                    t.execute(
                        "INSERT INTO reading_time (paper, page, seconds, last_ms) VALUES (?1, ?2, ?3, ?4)
                         ON CONFLICT(paper, page) DO UPDATE SET seconds = seconds + excluded.seconds, last_ms = excluded.last_ms",
                        params![i.paper.to_string(), i.page, secs, now],
                    )?;
                }
                Ok::<_, rusqlite::Error>(exists)
            })
            .map_err(sql_err)?;
        if !n {
            return Err(not_found(format!("paper {}", i.paper)));
        }
        self.stats(i.paper)
    }

    fn stats(&self, paper: PaperId) -> Result<ReadingStats, CommandError> {
        let o = self.require()?;
        o.db.read(|c| {
            c.query_row("SELECT coalesce(sum(seconds), 0), count(*), max(last_ms) FROM reading_time WHERE paper = ?1", [paper.to_string()], |r| {
                Ok(ReadingStats { seconds: u64::try_from(r.get::<_, i64>(0)?).unwrap_or(0), pages_read: u32::try_from(r.get::<_, i64>(1)?).unwrap_or(0), last_ms: r.get(2)? })
            })
        })
        .map_err(sql_err)
    }

    pub(crate) async fn reading_stats(
        self: Arc<Self>,
        _cx: Cx,
        i: PaperRef,
    ) -> Result<ReadingStats, CommandError> {
        self.stats(i.paper)
    }
}
