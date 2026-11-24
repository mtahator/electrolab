//! Workspace engine core: pure rules for the project folder format, file classification and the
//! database schema. No I/O — the facade reads and writes.
#![deny(missing_docs)]

use ep_workspace_contract::{FileKind, PROJECT_FORMAT, ProjectId, ProjectKind};
use serde::{Deserialize, Serialize};

/// Name of the manifest file at the root of every library or project folder (ADR-010).
pub const MANIFEST: &str = "electropaper-project.toml";
/// Content-addressed files.
pub const BLOBS_DIR: &str = "blobs";
/// Event log segments.
pub const EVENTS_DIR: &str = "events";
/// The database.
pub const DATABASE: &str = "workspace.sqlite";

/// The manifest: identifies the folder as a library or project and its format version.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Manifest {
    /// Folder format version.
    pub format: u32,
    /// Stable id.
    pub id: ProjectId,
    /// Library or project.
    pub kind: ProjectKind,
    /// Display name.
    pub name: String,
    /// Created, ms since the Unix epoch.
    pub created_ms: i64,
}

/// Why a manifest cannot be used.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FormatError {
    /// Not valid manifest TOML.
    Invalid(String),
    /// Written by a newer app.
    TooNew {
        /// Version in the folder.
        found: u32,
    },
}

impl std::fmt::Display for FormatError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Invalid(m) => write!(f, "project manifest is not valid: {m}"),
            Self::TooNew { found } => write!(
                f,
                "project folder format {found} is newer than this app supports ({PROJECT_FORMAT}); update the app"
            ),
        }
    }
}

impl Manifest {
    /// A new manifest at the current format.
    #[must_use]
    pub fn new(id: ProjectId, kind: ProjectKind, name: &str, created_ms: i64) -> Self {
        Self {
            format: PROJECT_FORMAT,
            id,
            kind,
            name: name.trim().to_string(),
            created_ms,
        }
    }

    /// Parse and check the version.
    ///
    /// # Errors
    /// Invalid TOML or a newer format.
    pub fn parse(text: &str) -> Result<Self, FormatError> {
        let m: Self = toml::from_str(text).map_err(|e| FormatError::Invalid(e.to_string()))?;
        if m.format > PROJECT_FORMAT {
            return Err(FormatError::TooNew { found: m.format });
        }
        if m.format == 0 {
            return Err(FormatError::Invalid("format 0".into()));
        }
        Ok(m)
    }

    /// TOML text (stable key order, a comment for people who open the file).
    #[must_use]
    pub fn to_toml(&self) -> String {
        let body = toml::to_string(self).unwrap_or_default();
        format!("# ElectroPaper library/project folder. Do not edit by hand.\n{body}")
    }
}

/// Classify a file by its first bytes (never by its name).
#[must_use]
pub fn classify(head: &[u8]) -> FileKind {
    // The PDF header must appear in the first 1024 bytes (PDF 32000-1, Annex H.3, note 3).
    let window = &head[..head.len().min(1024)];
    if window.windows(5).any(|w| w == b"%PDF-") {
        FileKind::Pdf
    } else {
        FileKind::Unsupported
    }
}

/// Database migrations, in order (each engine owns its tables).
pub const MIGRATIONS: &[(u32, &str, &str)] = &[
    (
        1,
        "papers",
        "CREATE TABLE paper (
        id              TEXT PRIMARY KEY,
        hash            TEXT NOT NULL UNIQUE,
        size            INTEGER NOT NULL,
        media_type      TEXT NOT NULL,
        original_name   TEXT NOT NULL,
        original_path   TEXT,
        title           TEXT,
        authors         TEXT NOT NULL DEFAULT '[]',
        doi             TEXT,
        pages           INTEGER,
        metadata_source TEXT,
        added_ms        INTEGER NOT NULL
    );
    CREATE INDEX paper_added ON paper (added_ms DESC);",
    ),
    (
        2,
        "library: collections, tags, reading state, bookmarks, reading time",
        "ALTER TABLE paper ADD COLUMN opened_ms INTEGER;
    CREATE TABLE collection (
        id          TEXT PRIMARY KEY,
        name        TEXT NOT NULL,
        created_ms  INTEGER NOT NULL
    );
    CREATE UNIQUE INDEX collection_name ON collection (name COLLATE NOCASE);
    CREATE TABLE collection_paper (
        collection  TEXT NOT NULL REFERENCES collection(id) ON DELETE CASCADE,
        paper       TEXT NOT NULL REFERENCES paper(id) ON DELETE CASCADE,
        added_ms    INTEGER NOT NULL,
        PRIMARY KEY (collection, paper)
    );
    CREATE TABLE paper_tag (
        paper       TEXT NOT NULL REFERENCES paper(id) ON DELETE CASCADE,
        tag         TEXT NOT NULL,
        PRIMARY KEY (paper, tag)
    );
    CREATE INDEX paper_tag_tag ON paper_tag (tag COLLATE NOCASE);
    CREATE TABLE reading_state (
        paper       TEXT PRIMARY KEY REFERENCES paper(id) ON DELETE CASCADE,
        location    TEXT,
        zoom        TEXT,
        view_mode   TEXT NOT NULL DEFAULT 'continuous',
        updated_ms  INTEGER NOT NULL
    );
    CREATE TABLE bookmark (
        id          TEXT PRIMARY KEY,
        paper       TEXT NOT NULL REFERENCES paper(id) ON DELETE CASCADE,
        name        TEXT NOT NULL,
        location    TEXT NOT NULL,
        created_ms  INTEGER NOT NULL
    );
    CREATE INDEX bookmark_paper ON bookmark (paper, created_ms);
    CREATE TABLE reading_time (
        paper       TEXT NOT NULL REFERENCES paper(id) ON DELETE CASCADE,
        page        INTEGER NOT NULL,
        seconds     INTEGER NOT NULL,
        last_ms     INTEGER NOT NULL,
        PRIMARY KEY (paper, page)
    );",
    ),
    (
        3,
        "papers downloaded from the web keep their address",
        "ALTER TABLE paper ADD COLUMN source_url TEXT;",
    ),
    (
        4,
        "earlier versions of a paper's managed copy",
        "CREATE TABLE paper_version (
            paper TEXT NOT NULL REFERENCES paper(id) ON DELETE CASCADE,
            hash TEXT NOT NULL, size INTEGER NOT NULL, replaced_ms INTEGER NOT NULL,
            PRIMARY KEY (paper, hash));",
    ),
    (
        5,
        "paper notes and their history",
        "CREATE TABLE paper_note (
            paper TEXT PRIMARY KEY REFERENCES paper(id) ON DELETE CASCADE,
            text TEXT NOT NULL, version INTEGER NOT NULL, modified_ms INTEGER NOT NULL);
         CREATE TABLE paper_note_version (
            paper TEXT NOT NULL REFERENCES paper(id) ON DELETE CASCADE,
            version INTEGER NOT NULL, text TEXT NOT NULL, saved_ms INTEGER NOT NULL,
            PRIMARY KEY (paper, version));",
    ),
    (
        6,
        "removing papers with undo (design language 17.10)",
        "ALTER TABLE paper ADD COLUMN removed_ms INTEGER;",
    ),
    (
        7,
        "folders: collections inside collections",
        "ALTER TABLE collection ADD COLUMN parent TEXT REFERENCES collection(id) ON DELETE CASCADE;
    DROP INDEX collection_name;
    CREATE UNIQUE INDEX collection_name ON collection (coalesce(parent, ''), name COLLATE NOCASE);
    CREATE INDEX collection_parent ON collection (parent);
    CREATE INDEX collection_paper_paper ON collection_paper (paper);",
    ),
];

/// Seconds counted from one `record_reading` report at most (the UI reports every few seconds;
/// a larger number means the report is wrong or the user was away).
pub const MAX_READING_REPORT_S: u32 = 300;

/// Clean tags: trimmed, inner spaces collapsed, at most 64 characters, no duplicates ignoring
/// case (first spelling wins), empty ones dropped. Order is kept.
#[must_use]
pub fn clean_tags(tags: &[String]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for t in tags {
        let t: String = t
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .chars()
            .filter(|c| !c.is_control())
            .take(64)
            .collect();
        if !t.is_empty() && !out.iter().any(|o| o.to_lowercase() == t.to_lowercase()) {
            out.push(t);
        }
    }
    out
}

/// Words of a library search, lowercase, as SQL `LIKE` patterns (`%word%`). `%`, `_` and `!` in
/// the words are escaped with `!`, so queries use `LIKE ? ESCAPE '!'`.
#[must_use]
pub fn like_patterns(query: &str) -> Vec<String> {
    query
        .split_whitespace()
        .take(8)
        .map(|w| {
            format!(
                "%{}%",
                w.to_lowercase()
                    .replace('!', "!!")
                    .replace('%', "!%")
                    .replace('_', "!_")
            )
        })
        .collect()
}

/// Clean a display name for a new library or project.
#[must_use]
pub fn clean_name(name: &str) -> Option<String> {
    let n: String = name
        .chars()
        .filter(|c| !c.is_control())
        .collect::<String>()
        .trim()
        .to_string();
    (!n.is_empty() && n.chars().count() <= 200).then_some(n)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manifest_round_trip_and_versions() {
        let m = Manifest::new(
            ProjectId::from_uuid(uuid::Uuid::from_u128(7)),
            ProjectKind::Library,
            " My library ",
            5,
        );
        let text = m.to_toml();
        assert!(text.starts_with('#'));
        assert_eq!(Manifest::parse(&text), Ok(m.clone()));
        let newer = text.replace("format = 1", "format = 9");
        assert_eq!(
            Manifest::parse(&newer),
            Err(FormatError::TooNew { found: 9 })
        );
        assert!(matches!(
            Manifest::parse("format = 1"),
            Err(FormatError::Invalid(_))
        ));
    }

    #[test]
    fn classifies_by_content() {
        assert_eq!(classify(b"%PDF-1.7\n..."), FileKind::Pdf);
        assert_eq!(classify(b"\xEF\xBB\xBF junk then %PDF-1.4"), FileKind::Pdf);
        assert_eq!(classify(b"PK\x03\x04 a zip"), FileKind::Unsupported);
        let mut late = vec![b' '; 2000];
        late.extend_from_slice(b"%PDF-1.4");
        assert_eq!(classify(&late), FileKind::Unsupported);
    }

    #[test]
    fn names() {
        assert_eq!(
            clean_name("  کتابخانه من \u{7}"),
            Some("کتابخانه من".into())
        );
        assert_eq!(clean_name("   "), None);
    }
}

#[cfg(test)]
mod library_tests {
    use super::*;

    #[test]
    fn tags_are_cleaned() {
        let t = clean_tags(&[
            "  PINN ".into(),
            "pinn".into(),
            "heat  transfer".into(),
            String::new(),
        ]);
        assert_eq!(t, ["PINN", "heat transfer"]);
    }

    #[test]
    fn like_patterns_escape() {
        assert_eq!(like_patterns("Neural 100%_x"), ["%neural%", "%100!%!_x%"]);
    }
}
