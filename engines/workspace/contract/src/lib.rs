//! Contract of the Workspace engine — libraries, projects and the papers in them.
//!
//! See `engines/workspace/CONTRACT.md`. Paths are host-local strings: commands run on the
//! user's own machine, and project files store only project-relative paths.
#![deny(missing_docs)]

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

pub use ep_types::{BlobRef, Hash, MediaType, PaperId, ProjectId};

/// Contract version; experimental until a second host uses it (Draft 2.2 §8.3).
pub const VERSION: &str = "0.1.0";

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
    /// Added, ms since the Unix epoch.
    pub added_ms: i64,
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
}

/// Events published on the hub.
pub mod events {
    /// A library or project was opened. Data: `ProjectInfo`.
    pub const OPENED: &str = "workspace.opened";
    /// A paper was added. Data: `Paper`.
    pub const PAPER_ADDED: &str = "workspace.paper_added";
    /// A paper's metadata changed. Data: `Paper`.
    pub const PAPER_CHANGED: &str = "workspace.paper_changed";
}
