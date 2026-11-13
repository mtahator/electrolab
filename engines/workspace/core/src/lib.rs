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
pub const MIGRATIONS: &[(u32, &str, &str)] = &[(
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
)];

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
