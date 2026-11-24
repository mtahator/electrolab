//! Contract of the Understanding engine — explaining and understanding a paper.
//!
//! See `engines/understanding/CONTRACT.md` (Draft 2.2 §14.5, §28.6).
#![deny(missing_docs)]

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

pub use ep_types::BlobRef;

/// Contract version; experimental until a second host uses it (Draft 2.2 §8.3).
pub const VERSION: &str = "0.3.0";

/// Command names.
pub mod commands {
    /// Explain a selection: a fast draft, then a version checked against the paper.
    pub const EXPLAIN_SPAN: &str = "understanding.explain_span";
    /// Suggest folders for papers (nothing is moved).
    pub const SUGGEST_FOLDERS: &str = "understanding.suggest_folders";
}

/// Event names.
pub mod events {
    /// The answer so far: `{ id, phase: draft | checking | check, text }`.
    pub const EXPLAINING: &str = "understanding.explaining";
    /// Progress of a sorting: `{ id, step: reading | thinking | sorting, done, total }`.
    pub const SORTING: &str = "understanding.sorting";
}

/// Error kinds besides those of `inference.complete`.
pub mod errors {
    /// Nothing was selected.
    pub const EMPTY_SELECTION: &str = "empty_selection";
    /// No paper to sort.
    pub const NOTHING_TO_SORT: &str = "nothing_to_sort";
    /// Too many papers at once.
    pub const TOO_MANY: &str = "too_many";
}

/// Ask for the answer to be streamed as `understanding.explaining` events.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[schemars(rename = "ExplainStream")]
pub struct Stream {
    /// Id carried by every event of this answer.
    pub id: String,
}

/// Input of `understanding.explain_span`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ExplainRequest {
    /// The paper's file.
    pub file: BlobRef,
    /// 1-based page of the selection.
    pub page: u32,
    /// The selected text.
    pub selection: String,
    /// Up to 500 characters before the selection (read from the page when missing).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub before: Option<String>,
    /// Up to 500 characters after the selection (read from the page when missing).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub after: Option<String>,
    /// Stream the answer.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stream: Option<Stream>,
}

/// How far an explanation was checked.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExplainStatus {
    /// Not checked (no passages, or the check gave no clear answer).
    Unchecked,
    /// The check agreed with the draft.
    Checked,
    /// The check corrected the draft; the text is the correction.
    Corrected,
}

/// A passage of the paper the check used.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Evidence {
    /// 1-based page.
    pub page: u32,
    /// Top of the passage on its page (points).
    pub y: f32,
    /// Verbatim text of the paper.
    pub quote: String,
}

/// Times of one explanation, from the request.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ExplainTimings {
    /// First draft token.
    pub first_token_ms: u64,
    /// Draft done.
    pub draft_ms: u64,
    /// The check alone.
    pub check_ms: u64,
    /// Everything.
    pub total_ms: u64,
}

/// Output of `understanding.explain_span`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Explanation {
    /// The selection explained.
    pub selection: String,
    /// The explanation shown.
    pub text: String,
    /// How far it was checked.
    pub status: ExplainStatus,
    /// Passages the check used.
    pub evidence: Vec<Evidence>,
    /// Model that answered.
    pub model: String,
    /// Times.
    pub timings: ExplainTimings,
}

/// A paper to sort.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct SortPaper {
    /// Id, returned in the suggestion.
    pub id: String,
    /// Title or file name.
    pub title: String,
    /// Its file (the first page is read when nothing is said about it).
    pub file: BlobRef,
    /// What it is about (its summary and topics), when that is known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub about: Option<String>,
}

/// A folder that is there.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct SortFolder {
    /// Id, returned in the suggestion.
    pub id: String,
    /// Name.
    pub name: String,
}

/// Input of `understanding.suggest_folders`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct SuggestFolders {
    /// Papers.
    pub papers: Vec<SortPaper>,
    /// Folders that may be used again.
    #[serde(default)]
    pub folders: Vec<SortFolder>,
    /// Send progress.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stream: Option<Stream>,
}

/// A suggested folder.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct SuggestedFolder {
    /// Name.
    pub name: String,
    /// The folder that is there with this name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub existing: Option<String>,
    /// Papers (ids).
    pub papers: Vec<String>,
}

/// Output of `understanding.suggest_folders`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct FolderSuggestion {
    /// Folders with their papers.
    pub folders: Vec<SuggestedFolder>,
    /// Papers that fit nowhere (they stay where they are).
    pub unsure: Vec<String>,
    /// Model that answered.
    pub model: String,
    /// Time taken.
    pub total_ms: u64,
}
