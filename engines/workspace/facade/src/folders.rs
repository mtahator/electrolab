//! Folders and changes of several papers at once (contract 0.5.0). Each is one transaction: all of
//! it, or nothing.

use std::sync::Arc;

use ep_bus::{CommandError, Cx};
use ep_store::rusqlite::{self, OptionalExtension, Transaction, params};
use ep_types::{CollectionId, PaperId};
use ep_workspace_contract::{
    AppliedFolders, ApplyFolders, Collection, MoveCollection, PaperIds, Placement, errors, events,
};
use ep_workspace_core::clean_name;
use serde_json::json;

use super::library::{COLLECTION_SELECT, is_unique_violation, not_found, row_to_collection};
use super::{Workspace, sql_err};

/// Most papers changed at once.
const MOST: usize = 10_000;
/// Deepest nesting of folders.
const DEPTH: usize = 12;

/// The folders above `id`, nearest first (stops at the nesting limit).
fn above(t: &Transaction<'_>, id: CollectionId) -> rusqlite::Result<Vec<String>> {
    let mut out = vec![];
    let mut at = id.to_string();
    while out.len() <= DEPTH {
        let parent: Option<Option<String>> = t
            .query_row("SELECT parent FROM collection WHERE id = ?1", [&at], |r| {
                r.get(0)
            })
            .optional()?;
        match parent.flatten() {
            Some(p) => {
                out.push(p.clone());
                at = p;
            }
            None => break,
        }
    }
    Ok(out)
}

fn exists(t: &Transaction<'_>, id: CollectionId) -> rusqlite::Result<bool> {
    t.query_row(
        "SELECT EXISTS(SELECT 1 FROM collection WHERE id = ?1)",
        [id.to_string()],
        |r| r.get(0),
    )
}

fn placement(t: &Transaction<'_>, paper: PaperId) -> rusqlite::Result<Placement> {
    let mut s = t.prepare_cached("SELECT collection FROM collection_paper WHERE paper = ?1")?;
    let collections = s
        .query_map([paper.to_string()], |r| r.get::<_, String>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?
        .iter()
        .filter_map(|c| c.parse().ok())
        .collect();
    Ok(Placement { paper, collections })
}

fn put(
    t: &Transaction<'_>,
    paper: PaperId,
    folder: CollectionId,
    now: i64,
) -> rusqlite::Result<()> {
    t.execute(
        "INSERT OR IGNORE INTO collection_paper (collection, paper, added_ms)
         SELECT ?1, id, ?3 FROM paper WHERE id = ?2",
        params![folder.to_string(), paper.to_string(), now],
    )?;
    Ok(())
}

fn refuse(message: impl Into<String>) -> CommandError {
    CommandError::InvalidInput {
        message: message.into(),
    }
}

fn too_many(n: usize) -> Result<(), CommandError> {
    if n > MOST {
        return Err(refuse(format!("{n} papers at once (at most {MOST})")));
    }
    Ok(())
}

#[allow(clippy::unused_async, clippy::unused_async_trait_impl)] // the bus calls handlers as futures
impl Workspace {
    pub(crate) async fn move_collection(
        self: Arc<Self>,
        cx: Cx,
        i: MoveCollection,
    ) -> Result<Collection, CommandError> {
        let o = self.require()?;
        let id = i.id.to_string();
        let ok =
            o.db.write(|t| {
                if let Some(p) = i.parent {
                    // Not into itself or one of its own folders, not too deep, only into what is there.
                    let up = above(t, p)?;
                    if p == i.id || up.contains(&id) || up.len() + 1 >= DEPTH || !exists(t, p)? {
                        return Ok(None);
                    }
                }
                t.execute(
                    "UPDATE collection SET parent = ?2 WHERE id = ?1",
                    params![id, i.parent.map(|p| p.to_string())],
                )
                .map(Some)
            })
            .map_err(|e| {
                if is_unique_violation(&e) {
                    CommandError::failed("exists", "a folder of this name is already there")
                } else {
                    sql_err(e)
                }
            })?;
        match ok {
            None => return Err(refuse("a folder cannot go there")),
            Some(0) => return Err(not_found(format!("collection {}", i.id))),
            Some(_) => {}
        }
        o.record(
            &cx,
            "workspace.collection_moved",
            json!({ "collection": i.id, "parent": i.parent }),
        );
        self.collection(i.id)
    }

    /// Put groups of papers into folders, making the folders that are new. A paper is then in that
    /// folder only.
    pub(crate) async fn apply_folders(
        self: Arc<Self>,
        cx: Cx,
        i: ApplyFolders,
    ) -> Result<AppliedFolders, CommandError> {
        let o = self.require()?;
        too_many(i.groups.iter().map(|g| g.papers.len()).sum())?;
        let mut names = vec![];
        for g in &i.groups {
            names.push(match &g.new {
                Some(n) => Some(
                    clean_name(&n.name)
                        .ok_or_else(|| refuse("a folder name is empty or too long"))?,
                ),
                None => None,
            });
        }
        let now = cx.clock.now_ms();
        let ids: Vec<CollectionId> = names
            .iter()
            .map(|_| CollectionId::from_uuid(cx.ids.next()))
            .collect();
        let done = o
            .db
            .write(|t| {
                let (mut created, mut before) = (vec![], vec![]);
                for ((g, name), new_id) in i.groups.iter().zip(&names).zip(&ids) {
                    let folder = match (g.collection, &g.new, name) {
                        (Some(c), ..) => {
                            if !exists(t, c)? {
                                return Ok(None);
                            }
                            Some(c)
                        }
                        (None, Some(n), Some(name)) => {
                            if let Some(p) = n.parent
                                && (!exists(t, p)? || above(t, p)?.len() + 1 >= DEPTH)
                            {
                                return Ok(None);
                            }
                            let parent = n.parent.map(|p| p.to_string());
                            let have: Option<String> = t
                                .query_row(
                                    "SELECT id FROM collection WHERE name = ?1 COLLATE NOCASE AND coalesce(parent, '') = coalesce(?2, '')",
                                    params![name, parent],
                                    |r| r.get(0),
                                )
                                .optional()?;
                            if let Some(id) = have.and_then(|h| h.parse().ok()) {
                                Some(id)
                            } else {
                                t.execute(
                                    "INSERT INTO collection (id, name, parent, created_ms) VALUES (?1, ?2, ?3, ?4)",
                                    params![new_id.to_string(), name, parent, now],
                                )?;
                                created.push(*new_id);
                                Some(*new_id)
                            }
                        }
                        _ => None,
                    };
                    for p in &g.papers {
                        before.push(placement(t, *p)?);
                        t.execute("DELETE FROM collection_paper WHERE paper = ?1", [p.to_string()])?;
                        if let Some(f) = folder {
                            put(t, *p, f, now)?;
                        }
                    }
                }
                Ok(Some((created, before)))
            })
            .map_err(sql_err)?;
        let Some((created, before)) = done else {
            return Err(not_found("a folder named here"));
        };
        o.record(
            &cx,
            "workspace.folders_applied",
            json!({ "created": created, "papers": before.len() }),
        );
        let created = created
            .into_iter()
            .map(|c| self.collection(c))
            .collect::<Result<_, _>>()?;
        Ok(AppliedFolders { created, before })
    }

    /// Put the papers back where they were, and take away the folders that were made when nothing
    /// else is in them.
    pub(crate) async fn undo_folders(
        self: Arc<Self>,
        cx: Cx,
        i: AppliedFolders,
    ) -> Result<(), CommandError> {
        let o = self.require()?;
        too_many(i.before.len())?;
        let now = cx.clock.now_ms();
        o.db.write(|t| {
            // Backwards: a paper named twice ends where it was first.
            for b in i.before.iter().rev() {
                t.execute(
                    "DELETE FROM collection_paper WHERE paper = ?1",
                    [b.paper.to_string()],
                )?;
                for c in &b.collections {
                    if exists(t, *c)? {
                        put(t, b.paper, *c, now)?;
                    }
                }
            }
            for c in i.created.iter().rev() {
                t.execute(
                    "DELETE FROM collection WHERE id = ?1
                     AND NOT EXISTS (SELECT 1 FROM collection_paper WHERE collection = ?1)
                     AND NOT EXISTS (SELECT 1 FROM collection WHERE parent = ?1)",
                    [c.id.to_string()],
                )?;
            }
            Ok(())
        })
        .map_err(sql_err)?;
        o.record(
            &cx,
            "workspace.folders_undone",
            json!({ "papers": i.before.len() }),
        );
        Ok(())
    }

    pub(crate) async fn remove_papers(
        self: Arc<Self>,
        cx: Cx,
        i: PaperIds,
    ) -> Result<(), CommandError> {
        let o = self.require()?;
        too_many(i.ids.len())?;
        let now = cx.clock.now_ms();
        let gone =
            o.db.write(|t| {
                let mut gone = vec![];
                for p in &i.ids {
                    let n = t.execute(
                        "UPDATE paper SET removed_ms = ?2 WHERE id = ?1 AND removed_ms IS NULL",
                        params![p.to_string(), now],
                    )?;
                    if n > 0 {
                        gone.push(*p);
                    }
                }
                Ok(gone)
            })
            .map_err(sql_err)?;
        if gone.is_empty() {
            return Err(CommandError::failed(
                errors::NOT_FOUND,
                "none of these papers is in the library",
            ));
        }
        for p in gone {
            o.record(&cx, events::PAPER_REMOVED, json!({ "paper": p }));
            if let Some(bus) = self.bus.read().upgrade() {
                bus.events()
                    .publish(events::PAPER_REMOVED, json!({ "paper": p }), now);
            }
        }
        Ok(())
    }

    pub(crate) async fn restore_papers(
        self: Arc<Self>,
        cx: Cx,
        i: PaperIds,
    ) -> Result<(), CommandError> {
        let o = self.require()?;
        too_many(i.ids.len())?;
        o.db.write(|t| {
            for p in &i.ids {
                t.execute(
                    "UPDATE paper SET removed_ms = NULL WHERE id = ?1",
                    [p.to_string()],
                )?;
            }
            Ok(())
        })
        .map_err(sql_err)?;
        o.record(&cx, "workspace.papers_restored", json!({ "papers": i.ids }));
        Ok(())
    }

    /// Every folder, by name.
    pub(crate) fn all_folders(&self) -> Result<Vec<Collection>, CommandError> {
        self.require()?
            .db
            .read(|c| {
                let mut s = c.prepare(&format!("{COLLECTION_SELECT} ORDER BY lower(c.name)"))?;
                s.query_map([], row_to_collection)?
                    .collect::<rusqlite::Result<Vec<_>>>()
            })
            .map_err(sql_err)
    }
}
