//! Library features (P1.M2.WSP.1–3): collections, tags, reading state, bookmarks, statistics.

use ep_types::{CollectionId, Location, ObjectId, PaperId, ZoomSpec};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// A named group of papers.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Collection {
    /// Id.
    pub id: CollectionId,
    /// Name.
    pub name: String,
    /// The folder it is in; the top of the library when missing.
    #[serde(default)]
    pub parent: Option<CollectionId>,
    /// Number of papers in it.
    pub papers: u64,
    /// Created, ms since the Unix epoch.
    pub created_ms: i64,
}

/// Input of `workspace.create_collection`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct CreateCollection {
    /// Name.
    pub name: String,
    /// The folder it goes into.
    #[serde(default)]
    pub parent: Option<CollectionId>,
}

/// Input of `workspace.rename_collection`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct RenameCollection {
    /// Which collection.
    pub id: CollectionId,
    /// New name.
    pub name: String,
}

/// Input naming one collection.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct CollectionRef {
    /// Which collection.
    pub id: CollectionId,
}

/// Input of `workspace.add_to_collection` and `workspace.remove_from_collection`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct CollectionPapers {
    /// Which collection.
    pub collection: CollectionId,
    /// Papers to add or remove.
    pub papers: Vec<PaperId>,
}

/// Input of `workspace.set_tags`: replaces the paper's tags.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct SetTags {
    /// Which paper.
    pub paper: PaperId,
    /// Tags (trimmed, case-insensitively unique).
    pub tags: Vec<String>,
}

/// Order of `workspace.list_papers`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PaperSort {
    /// Newest added first.
    #[default]
    Added,
    /// Most recently opened first (never opened last).
    Opened,
    /// By title (untitled by file name).
    Title,
}

/// How pages are laid out.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ViewMode {
    /// One column of pages, scrolling continuously.
    #[default]
    Continuous,
    /// One page at a time.
    Single,
    /// Two pages side by side.
    TwoPage,
}

/// A named place in a paper.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Bookmark {
    /// Id.
    pub id: ObjectId,
    /// Name.
    pub name: String,
    /// Where.
    pub location: Location,
    /// Created, ms since the Unix epoch.
    pub created_ms: i64,
}

/// Where the reader was in a paper, and how it was shown.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ReadingState {
    /// The paper.
    pub paper: PaperId,
    /// Last place.
    #[serde(default)]
    pub location: Option<Location>,
    /// Zoom.
    #[serde(default)]
    pub zoom: Option<ZoomSpec>,
    /// Layout.
    #[serde(default)]
    pub view_mode: ViewMode,
    /// Named places.
    #[serde(default)]
    pub bookmarks: Vec<Bookmark>,
    /// Last opened, ms since the Unix epoch (`None` if never opened).
    #[serde(default)]
    pub opened_ms: Option<i64>,
}

/// Input of `workspace.save_reading_state` (also marks the paper as opened now).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct SaveReadingState {
    /// The paper.
    pub paper: PaperId,
    /// Current place.
    #[serde(default)]
    pub location: Option<Location>,
    /// Zoom.
    #[serde(default)]
    pub zoom: Option<ZoomSpec>,
    /// Layout.
    #[serde(default)]
    pub view_mode: ViewMode,
}

/// Input naming one paper.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct PaperInput {
    /// The paper.
    pub paper: PaperId,
}

/// Input of `workspace.add_bookmark`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct AddBookmark {
    /// Name.
    pub name: String,
    /// Where (its `paper` names the paper).
    pub location: Location,
}

/// Input of `workspace.remove_bookmark`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct BookmarkRef {
    /// Which bookmark.
    pub id: ObjectId,
}

/// Input of `workspace.record_reading`: the UI reports time on a visible page while the window
/// is active, in short intervals.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct RecordReading {
    /// The paper.
    pub paper: PaperId,
    /// The page that was visible.
    pub page: u32,
    /// Seconds of active reading in this interval (at most 300 counted).
    pub seconds: u32,
}

/// Reading statistics of a paper.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ReadingStats {
    /// Total seconds of active reading.
    pub seconds: u64,
    /// Number of different pages seen.
    pub pages_read: u32,
    /// Last reading, ms since the Unix epoch.
    #[serde(default)]
    pub last_ms: Option<i64>,
}
