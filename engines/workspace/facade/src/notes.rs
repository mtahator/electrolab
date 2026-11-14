//! Paper notes (P1.MD.NOTE.1): one notebook per paper, saved as the user types. Every save is a
//! new version; the history keeps one version per minute of editing (the newest of that minute),
//! up to `HISTORY` versions per paper, so earlier text can always be restored.

use std::sync::Arc;

use ep_bus::{CommandError, Cx};
use ep_store::rusqlite::{self, OptionalExtension, params};
use ep_types::PaperId;
use ep_workspace_contract::{
    MAX_NOTES_CHARS, NotesVersion, PaperInput, PaperNotes, RestoreNotes, SaveNotes, errors, events,
};
use serde_json::json;

use super::{Workspace, sql_err};

/// Versions kept per paper.
const HISTORY: i64 = 200;
/// Saves closer together than this replace the newest history entry instead of adding one.
const COALESCE_MS: i64 = 60_000;

fn preview(text: &str) -> String {
    let flat: String = text.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut out: String = flat.chars().take(80).collect();
    if flat.chars().count() > 80 {
        out.push('…');
    }
    out
}

fn current(c: &rusqlite::Connection, paper: PaperId) -> rusqlite::Result<PaperNotes> {
    Ok(c.query_row(
        "SELECT text, version, modified_ms FROM paper_note WHERE paper = ?1",
        [paper.to_string()],
        |r| {
            Ok(PaperNotes {
                paper,
                text: r.get(0)?,
                version: u64::try_from(r.get::<_, i64>(1)?).unwrap_or(0),
                modified_ms: Some(r.get(2)?),
            })
        },
    )
    .optional()?
    .unwrap_or(PaperNotes {
        paper,
        text: String::new(),
        version: 0,
        modified_ms: None,
    }))
}

enum Fail {
    Sql(rusqlite::Error),
    Conflict(u64),
    Missing,
}

impl From<rusqlite::Error> for Fail {
    fn from(e: rusqlite::Error) -> Self {
        Self::Sql(e)
    }
}

/// Write `text` as the next version (shared by save and restore).
fn write(
    t: &rusqlite::Transaction<'_>,
    paper: PaperId,
    text: &str,
    base: Option<u64>,
    now: i64,
) -> Result<PaperNotes, Fail> {
    let p = paper.to_string();
    let exists: bool = t.query_row(
        "SELECT EXISTS (SELECT 1 FROM paper WHERE id = ?1)",
        [&p],
        |r| r.get(0),
    )?;
    if !exists {
        return Err(Fail::Missing);
    }
    let cur = current(t, paper)?;
    if let Some(b) = base
        && b != cur.version
    {
        return Err(Fail::Conflict(cur.version));
    }
    if cur.version > 0 && cur.text == text {
        return Ok(cur);
    }
    let version = i64::try_from(cur.version + 1).unwrap_or(i64::MAX);
    t.execute(
        "INSERT INTO paper_note (paper, text, version, modified_ms) VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT (paper) DO UPDATE SET text = excluded.text, version = excluded.version, modified_ms = excluded.modified_ms",
        params![p, text, version, now],
    )?;
    // History: within a minute of the newest entry, that entry is replaced.
    let newest: Option<(i64, i64)> = t
        .query_row(
            "SELECT version, saved_ms FROM paper_note_version WHERE paper = ?1 ORDER BY version DESC LIMIT 1",
            [&p],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;
    // The replaced entry keeps its start time, so continuous typing still leaves one entry
    // per minute.
    let mut saved_at = now;
    if let Some((v, at)) = newest
        && now - at < COALESCE_MS
        && v > 1
    {
        t.execute(
            "DELETE FROM paper_note_version WHERE paper = ?1 AND version = ?2",
            params![p, v],
        )?;
        saved_at = at;
    }
    t.execute(
        "INSERT INTO paper_note_version (paper, version, text, saved_ms) VALUES (?1, ?2, ?3, ?4)",
        params![p, version, text, saved_at],
    )?;
    t.execute(
        "DELETE FROM paper_note_version WHERE paper = ?1 AND version NOT IN
         (SELECT version FROM paper_note_version WHERE paper = ?1 ORDER BY version DESC LIMIT ?2)",
        params![p, HISTORY],
    )?;
    Ok(PaperNotes {
        paper,
        text: text.to_string(),
        version: u64::try_from(version).unwrap_or(0),
        modified_ms: Some(now),
    })
}

fn fail(e: Fail, paper: PaperId) -> CommandError {
    match e {
        Fail::Sql(e) => sql_err(e),
        Fail::Conflict(v) => CommandError::failed(
            errors::CONFLICT,
            format!("the notes of {paper} changed (now version {v}); reload them"),
        ),
        Fail::Missing => CommandError::failed(errors::NOT_FOUND, format!("paper {paper}")),
    }
}

// Handlers are `async` because the bus calls them as futures; each is one short transaction.
#[allow(clippy::unused_async, clippy::unused_async_trait_impl)]
impl Workspace {
    pub(crate) async fn get_notes(
        self: Arc<Self>,
        _cx: Cx,
        i: PaperInput,
    ) -> Result<PaperNotes, CommandError> {
        self.require()?
            .db
            .read(|c| current(c, i.paper))
            .map_err(sql_err)
    }

    pub(crate) async fn save_notes(
        self: Arc<Self>,
        cx: Cx,
        i: SaveNotes,
    ) -> Result<PaperNotes, CommandError> {
        if i.text.chars().count() > MAX_NOTES_CHARS {
            return Err(CommandError::InvalidInput {
                message: format!("notes are limited to {MAX_NOTES_CHARS} characters"),
            });
        }
        let o = self.require()?;
        let now = cx.clock.now_ms();
        let before = o.db.read(|c| current(c, i.paper)).map_err(sql_err)?;
        let notes =
            o.db.write(|t| write(t, i.paper, &i.text, i.base_version, now))
                .map_err(|e| fail(e, i.paper))?;
        if notes.version != before.version {
            self.notes_changed(&cx, &notes);
        }
        Ok(notes)
    }

    pub(crate) async fn notes_history(
        self: Arc<Self>,
        _cx: Cx,
        i: PaperInput,
    ) -> Result<Vec<NotesVersion>, CommandError> {
        self.require()?
            .db
            .read(|c| {
                let mut st = c.prepare(
                    "SELECT version, saved_ms, text FROM paper_note_version WHERE paper = ?1 ORDER BY version DESC",
                )?;
                st.query_map([i.paper.to_string()], |r| {
                    let text: String = r.get(2)?;
                    Ok(NotesVersion {
                        version: u64::try_from(r.get::<_, i64>(0)?).unwrap_or(0),
                        saved_ms: r.get(1)?,
                        chars: u64::try_from(text.chars().count()).unwrap_or(0),
                        preview: preview(&text),
                    })
                })?
                .collect::<rusqlite::Result<Vec<_>>>()
            })
            .map_err(sql_err)
    }

    pub(crate) async fn restore_notes(
        self: Arc<Self>,
        cx: Cx,
        i: RestoreNotes,
    ) -> Result<PaperNotes, CommandError> {
        let o = self.require()?;
        let now = cx.clock.now_ms();
        let old: Option<String> =
            o.db.read(|c| {
                c.query_row(
                    "SELECT text FROM paper_note_version WHERE paper = ?1 AND version = ?2",
                    params![i.paper.to_string(), i64::try_from(i.version).unwrap_or(-1)],
                    |r| r.get(0),
                )
                .optional()
            })
            .map_err(sql_err)?;
        let text = old.ok_or_else(|| {
            CommandError::failed(
                errors::NOT_FOUND,
                format!("version {} of the notes", i.version),
            )
        })?;
        // A restore is its own history entry, never merged into the one before.
        let notes = o
            .db
            .write(|t| {
                t.execute(
                    "UPDATE paper_note_version SET saved_ms = saved_ms - ?2 WHERE paper = ?1 AND version = (SELECT max(version) FROM paper_note_version WHERE paper = ?1)",
                    params![i.paper.to_string(), COALESCE_MS],
                )?;
                write(t, i.paper, &text, None, now)
            })
            .map_err(|e| fail(e, i.paper))?;
        self.notes_changed(&cx, &notes);
        Ok(notes)
    }

    fn notes_changed(&self, cx: &Cx, notes: &PaperNotes) {
        if let Some(bus) = self.bus.read().upgrade() {
            bus.events().publish(
                events::NOTES_CHANGED,
                json!({ "paper": notes.paper, "version": notes.version }),
                cx.clock.now_ms(),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::preview;

    #[test]
    fn preview_is_short_and_flat() {
        assert_eq!(preview("# Title\n\nsome   text"), "# Title some text");
        assert!(preview(&"word ".repeat(40)).ends_with('…'));
    }
}
