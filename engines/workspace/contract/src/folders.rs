//! Folders in the library (founder request, 2026-11-24): collections inside each other, and changes
//! of several papers at once, each with its undo.

use ep_types::{CollectionId, PaperId};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::Collection;

/// Input of `workspace.move_collection`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct MoveCollection {
    /// Which folder.
    pub id: CollectionId,
    /// The folder it goes into; the top of the library when missing.
    #[serde(default)]
    pub parent: Option<CollectionId>,
}

/// A folder to make.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct NewFolder {
    /// Name (an existing folder of this name in the same place is used).
    pub name: String,
    /// Where; the top of the library when missing.
    #[serde(default)]
    pub parent: Option<CollectionId>,
}

/// Papers that go together into one folder.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct FolderGroup {
    /// The papers.
    pub papers: Vec<PaperId>,
    /// An existing folder.
    #[serde(default)]
    pub collection: Option<CollectionId>,
    /// A new folder. Neither this nor `collection`: out of all folders.
    #[serde(default)]
    pub new: Option<NewFolder>,
}

/// Input of `workspace.apply_folders`: all of it, or nothing.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ApplyFolders {
    /// Groups.
    pub groups: Vec<FolderGroup>,
}

/// The folders a paper was in.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Placement {
    /// Paper.
    pub paper: PaperId,
    /// Its folders.
    pub collections: Vec<CollectionId>,
}

/// Output of `workspace.apply_folders`, and input of `workspace.undo_folders`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct AppliedFolders {
    /// Folders that were made.
    pub created: Vec<Collection>,
    /// Where the papers were before.
    pub before: Vec<Placement>,
}

/// Input of `workspace.remove_papers` and `workspace.restore_papers`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct PaperIds {
    /// The papers.
    pub ids: Vec<PaperId>,
}
