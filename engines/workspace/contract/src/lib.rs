//! Contract of the Workspace engine — libraries, projects and the papers in them.
//!
//! See `engines/workspace/CONTRACT.md`. Paths are host-local strings: commands run on the
//! user's own machine, and project files store only project-relative paths.
#![deny(missing_docs)]

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

pub use ep_types::{BlobRef, CollectionId, Hash, MediaType, PaperId, ProjectId};

mod library;
mod notes;
pub use library::*;
pub use notes::*;

/// Contract version; experimental until a second host uses it (Draft 2.2 §8.3).
pub const VERSION: &str = "0.4.0";

/// Project folder format written by this version (ADR-010).
pub const PROJECT_FORMAT: u32 = 1;

/// Command names.
pub mod commands {
    /// Create a library or project folder.
    pub const CREATE_PROJECT: &str = "workspace.create_project";
    /// Open an existing library or project folder.
    pub const OPEN_PROJECT: &str = "workspace.open_project";
    /// Open (creating on first run) the user's default library.
    pub const OPEN_DEFAULT_LIBRARY: &str = "workspace.open_default_library";
    /// The open library or project.
    pub const CURRENT: &str = "workspace.current";
    /// What kind of file is this.
    pub const CLASSIFY: &str = "workspace.classify";
    /// Add a file as a managed copy.
    pub const ADD_FILE: &str = "workspace.add_file";
    /// One paper.
    pub const GET_PAPER: &str = "workspace.get_paper";
    /// Papers, newest first.
    pub const LIST_PAPERS: &str = "workspace.list_papers";
    /// Record metadata read from the paper.
    pub const SET_PAPER_METADATA: &str = "workspace.set_paper_metadata";
    /// Point a paper at a new version of its managed copy (the previous one is kept).
    pub const SET_PAPER_FILE: &str = "workspace.set_paper_file";
    /// Make a collection.
    pub const CREATE_COLLECTION: &str = "workspace.create_collection";
    /// Rename a collection.
    pub const RENAME_COLLECTION: &str = "workspace.rename_collection";
    /// Delete a collection (its papers stay in the library).
    pub const DELETE_COLLECTION: &str = "workspace.delete_collection";
    /// Collections, by name.
    pub const LIST_COLLECTIONS: &str = "workspace.list_collections";
    /// Put papers in a collection.
    pub const ADD_TO_COLLECTION: &str = "workspace.add_to_collection";
    /// Take papers out of a collection.
    pub const REMOVE_FROM_COLLECTION: &str = "workspace.remove_from_collection";
    /// Replace a paper's tags.
    pub const SET_TAGS: &str = "workspace.set_tags";
    /// Every tag in use, with counts.
    pub const LIST_TAGS: &str = "workspace.list_tags";
    /// Where the reader was in a paper.
    pub const GET_READING_STATE: &str = "workspace.get_reading_state";
    /// Remember where the reader is.
    pub const SAVE_READING_STATE: &str = "workspace.save_reading_state";
    /// Add a named place.
    pub const ADD_BOOKMARK: &str = "workspace.add_bookmark";
    /// Remove a named place.
    pub const REMOVE_BOOKMARK: &str = "workspace.remove_bookmark";
    /// Count active reading time on a page.
    pub const RECORD_READING: &str = "workspace.record_reading";
    /// Reading time and pages read.
    pub const READING_STATS: &str = "workspace.reading_stats";
    /// The notebook of a paper.
    pub const GET_NOTES: &str = "workspace.get_notes";
    /// Save the notebook of a paper.
    pub const SAVE_NOTES: &str = "workspace.save_notes";
    /// Earlier versions of a paper's notebook.
    pub const NOTES_HISTORY: &str = "workspace.notes_history";
    /// Bring back an earlier version of a paper's notebook.
    pub const RESTORE_NOTES: &str = "workspace.restore_notes";
}

/// Library (the user's collection) or project (a focused piece of work).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ProjectKind {
    /// The user's library.
    Library,
    /// A research project.
    Project,
}

/// Input of `workspace.create_project`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct CreateProject {
    /// Folder to create (must not exist, or be empty).
    pub folder: String,
    /// Display name.
    pub name: String,
    /// Library or project.
    pub kind: ProjectKind,
}

/// Input of `workspace.open_project`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct OpenProject {
    /// The project folder.
    pub folder: String,
}

/// An open library or project.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ProjectInfo {
    /// Stable id (kept when the folder moves).
    pub id: ProjectId,
    /// Display name.
    pub name: String,
    /// Library or project.
    pub kind: ProjectKind,
    /// Where it is on this machine.
    pub folder: String,
    /// Folder format version.
    pub format: u32,
    /// Created, ms since the Unix epoch.
    pub created_ms: i64,
    /// Number of papers.
    pub papers: u64,
}

/// Input of `workspace.classify` and `workspace.add_file`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct FilePath {
    /// A file on this machine.
    pub path: String,
    /// `workspace.add_file` only: the web address the file was downloaded from (kept with the
    /// paper; the temporary download path is then not recorded).
    #[serde(default)]
    pub source_url: Option<String>,
}

/// What a file is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum FileKind {
    /// A PDF (by content, not by name).
    Pdf,
    /// Anything else (not accepted in Phase 1).
    Unsupported,
}

/// Output of `workspace.classify`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Classification {
    /// Kind.
    pub kind: FileKind,
    /// Size in bytes.
    pub size: u64,
}

/// A paper in a library or project.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Paper {
    /// Id.
    pub id: PaperId,
    /// The managed copy.
    pub file: BlobRef,
    /// File name when added.
    pub original_name: String,
    /// Where it came from on this machine, if added from a file.
    #[serde(default)]
    pub original_path: Option<String>,
    /// The web address it was downloaded from, if any.
    #[serde(default)]
    pub source_url: Option<String>,
    /// Title, when known.
    #[serde(default)]
    pub title: Option<String>,
    /// Authors in order.
    #[serde(default)]
    pub authors: Vec<String>,
    /// DOI (lowercase, without `https://doi.org/`).
    #[serde(default)]
    pub doi: Option<String>,
    /// Number of pages, when known.
    #[serde(default)]
    pub pages: Option<u32>,
    /// Where the metadata came from (e.g. `pdf-info`), when set.
    #[serde(default)]
    pub metadata_source: Option<String>,
    /// Tags.
    #[serde(default)]
    pub tags: Vec<String>,
    /// Added, ms since the Unix epoch.
    pub added_ms: i64,
    /// Last opened, ms since the Unix epoch.
    #[serde(default)]
    pub opened_ms: Option<i64>,
}

/// Output of `workspace.add_file`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Added {
    /// The paper.
    pub paper: Paper,
    /// The same content was already in the library (nothing new was stored).
    pub already_present: bool,
}

/// Input of `workspace.get_paper`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct GetPaper {
    /// Which paper.
    pub id: PaperId,
}

/// Input of `workspace.list_papers`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct ListPapers {
    /// At most this many (default 200).
    pub limit: Option<u32>,
    /// Skip this many.
    pub offset: u32,
    /// Only papers in this collection.
    pub collection: Option<CollectionId>,
    /// Only papers with this tag.
    pub tag: Option<String>,
    /// Only papers whose title, authors, DOI or file name contain these words.
    pub query: Option<String>,
    /// Order.
    pub sort: PaperSort,
}

/// A tag and how many papers carry it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct TagCount {
    /// Tag.
    pub tag: String,
    /// Papers.
    pub papers: u64,
}

/// Input of `workspace.set_paper_metadata`. Fields left `None` are unchanged.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct SetPaperMetadata {
    /// Which paper.
    pub id: PaperId,
    /// Title.
    #[serde(default)]
    pub title: Option<String>,
    /// Authors.
    #[serde(default)]
    pub authors: Option<Vec<String>>,
    /// DOI.
    #[serde(default)]
    pub doi: Option<String>,
    /// Page count.
    #[serde(default)]
    pub pages: Option<u32>,
    /// Where it came from.
    pub source: String,
}

/// Input of `workspace.set_paper_file`: a new version of the paper's managed copy, already in
/// the library's blob store (for example a PDF with annotations saved into it).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct SetPaperFile {
    /// Which paper.
    pub id: PaperId,
    /// The new file (must be a stored PDF).
    pub file: BlobRef,
}

/// Error kinds reported in `CommandError::Failed { kind }`.
pub mod errors {
    /// No library or project is open.
    pub const NO_PROJECT: &str = "no_project_open";
    /// No such paper or folder.
    pub const NOT_FOUND: &str = "not_found";
    /// Not a supported file.
    pub const UNSUPPORTED_FILE: &str = "unsupported_file";
    /// The folder is not empty / already a project.
    pub const FOLDER_IN_USE: &str = "folder_in_use";
    /// The folder was written by a newer version or is damaged.
    pub const BAD_FORMAT: &str = "bad_format";
    /// Disk or database failure.
    pub const IO: &str = "io";
    /// The notebook changed since the version the edit started from.
    pub const CONFLICT: &str = "conflict";
}

/// Events published on the hub.
pub mod events {
    /// A paper's notebook was saved. Data: `{ paper, version }`.
    pub const NOTES_CHANGED: &str = "workspace.notes_changed";
    /// A library or project was opened. Data: `ProjectInfo`.
    pub const OPENED: &str = "workspace.opened";
    /// A paper was added. Data: `Paper`.
    pub const PAPER_ADDED: &str = "workspace.paper_added";
    /// A paper's metadata changed. Data: `Paper`.
    pub const PAPER_CHANGED: &str = "workspace.paper_changed";
}
