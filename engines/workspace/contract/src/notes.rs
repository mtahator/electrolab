//! Paper notes (P1.MD.NOTE.1, guideline §10.2): one notebook per paper, Markdown text, saved as
//! the user types, with a version history so earlier text can always be brought back.

use ep_types::PaperId;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Longest notebook accepted (characters).
pub const MAX_NOTES_CHARS: usize = 2_000_000;

/// The notebook of a paper.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct PaperNotes {
    /// The paper.
    pub paper: PaperId,
    /// Markdown text. Location chips are links of the form `[p. 4](loc:4/120)` (page / y in
    /// page space), optionally `loc:4/120/<annotation id>`.
    pub text: String,
    /// Increases with every save; 0 when nothing was written yet.
    pub version: u64,
    /// Last saved, ms since the Unix epoch.
    #[serde(default)]
    pub modified_ms: Option<i64>,
}

/// Input of `workspace.save_notes`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct SaveNotes {
    /// The paper.
    pub paper: PaperId,
    /// The whole text.
    pub text: String,
    /// The version the text was edited from. When given and the notebook has moved on (another
    /// window saved in between), the save is refused with `conflict` and nothing changes.
    #[serde(default)]
    pub base_version: Option<u64>,
}

/// One point in a notebook's history.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct NotesVersion {
    /// Version number.
    pub version: u64,
    /// When it was saved.
    pub saved_ms: i64,
    /// Length in characters.
    pub chars: u64,
    /// The first words, for the history list.
    pub preview: String,
}

/// Input of `workspace.restore_notes`: bring back an earlier version (saved as a new version, so
/// restoring can itself be undone by restoring again).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct RestoreNotes {
    /// The paper.
    pub paper: PaperId,
    /// The version to bring back.
    pub version: u64,
}
