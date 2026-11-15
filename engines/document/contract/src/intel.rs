//! Document intelligence (P1.M4): background preparation and what it finds — objects on the page
//! (figures, tables, equations, headings…), the generated outline and jump lists, mentions of
//! figures and tables in the text, citation markers and the reference list, OCR of scanned pages,
//! equations as LaTeX and tables as cells.
//!
//! Deterministic first: the PDF's own text, outline and printed labels; the layout, OCR and
//! formula models (ADR-007) fill what the text cannot give. Positions are in page space.

use ep_types::{BlobRef, PagePoint, PageQuad, PageRect};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::OutlineItem;

// ------------------------------------------------------------------------- preparation ----

/// A stage of background preparation, in the order they run.
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum Stage {
    /// Characters of every page; marks pages without a usable text layer.
    Text,
    /// Section headings (the generated outline).
    Outline,
    /// Figures, tables, equations and the rest, by the layout model.
    Objects,
    /// Text for pages that have none (scanned).
    Ocr,
    /// Reference list, citation markers and mentions of figures, tables and equations.
    Links,
}

/// How a stage stands for one file.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum StageStatus {
    /// Not started.
    Pending,
    /// Running now.
    Running,
    /// Finished.
    Done,
    /// Not possible here (e.g. the layout model is not downloaded); `note` says why.
    Skipped,
    /// Failed; `note` says why. Other stages still run.
    Failed,
}

/// One stage's state.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct StageState {
    /// Which stage.
    pub stage: Stage,
    /// Where it stands.
    pub status: StageStatus,
    /// Pages done.
    pub done: u32,
    /// Pages to do.
    pub total: u32,
    /// Why it was skipped or failed.
    #[serde(default)]
    pub note: Option<String>,
}

/// Input of `document.prepare`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct PrepareRequest {
    /// The file.
    pub file: BlobRef,
    /// Pages to do first (the ones in view).
    #[serde(default)]
    pub first_pages: Vec<u32>,
}

/// Output of `document.prepare` (and `document.preparation`): every stage's state. The call
/// streams a `StageState` each time a stage moves.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Preparation {
    /// The file.
    pub file: BlobRef,
    /// Stages in order.
    pub stages: Vec<StageState>,
    /// Pages without a usable text layer (scanned), 1-based.
    pub image_only_pages: Vec<u32>,
}

// ----------------------------------------------------------------------------- objects ----

/// What an object on the page is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ObjectKind {
    /// A figure, picture or chart.
    Figure,
    /// A table.
    Table,
    /// A displayed equation.
    Equation,
    /// A section heading.
    Heading,
    /// The paper's title.
    Title,
    /// A figure or table caption.
    Caption,
    /// A paragraph of body text.
    Text,
    /// A list item.
    ListItem,
    /// A footnote.
    Footnote,
    /// Running header or footer.
    PageFurniture,
    /// Program code or an algorithm.
    Code,
}

/// An object on a page.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct DocObject {
    /// Stable within the file: `p<page>-<kind>-<n>`.
    pub id: String,
    /// What it is.
    pub kind: ObjectKind,
    /// Where it is.
    pub rect: PageRect,
    /// Printed label: "Figure 3", "Table 2", "(7)", "4.2".
    #[serde(default)]
    pub label: Option<String>,
    /// The number alone: "3", "2", "7", "4.2".
    #[serde(default)]
    pub number: Option<String>,
    /// Figures and tables: the caption text; headings: the heading text.
    #[serde(default)]
    pub text: Option<String>,
    /// The caption's box.
    #[serde(default)]
    pub caption_rect: Option<PageRect>,
    /// Detector confidence 0–1 (1 when found from the text alone).
    pub score: f32,
    /// Position in reading order on the page (0 = first).
    pub order: u32,
}

/// Objects of one page.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct PageObjects {
    /// 1-based page.
    pub page: u32,
    /// In reading order.
    pub objects: Vec<DocObject>,
    /// The layout model ran on this page (otherwise only text-derived objects are listed).
    pub detected: bool,
}

/// Input naming one page of a file.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ObjectsRequest {
    /// The file.
    pub file: BlobRef,
    /// 1-based page.
    pub page: u32,
}

/// Input of `document.region`: a point (click) or a rectangle (drag) on a page.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct RegionRequest {
    /// The file.
    pub file: BlobRef,
    /// 1-based page.
    pub page: u32,
    /// A clicked point.
    #[serde(default)]
    pub point: Option<PagePoint>,
    /// A dragged rectangle.
    #[serde(default)]
    pub rect: Option<PageRect>,
    /// Width of the cropped image in pixels (default 800).
    #[serde(default)]
    pub width_px: Option<u32>,
}

/// Output of `document.region`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct RegionHit {
    /// The object under the point or best covering the rectangle, if any.
    #[serde(default)]
    pub object: Option<DocObject>,
    /// The region (the object's box, or the dragged rectangle).
    pub rect: PageRect,
    /// The region as a PNG.
    pub image: BlobRef,
    /// Text inside the region.
    pub text: String,
}

/// Output of `document.contents`: what the paper contains, for navigation.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Contents {
    /// Section headings found in the text (used when the PDF has no bookmarks).
    pub sections: Vec<OutlineItem>,
    /// Figures in order.
    pub figures: Vec<DocObject>,
    /// Tables in order.
    pub tables: Vec<DocObject>,
    /// Numbered equations in order.
    pub equations: Vec<DocObject>,
}

/// Input naming a file.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct FileOnly {
    /// The file.
    pub file: BlobRef,
}

// ------------------------------------------------------------------- mentions and refs ----

/// What a mention points at.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum MentionKind {
    /// "Fig. 3", "Figure 3(b)", "Figs. 2–4".
    Figure,
    /// "Table 2".
    Table,
    /// "Eq. (5)", "Equation 5", "(5)" after "in".
    Equation,
    /// "Section 4.2", "Sec. 4".
    Section,
}

/// A mention of a figure, table, equation or section in the text.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Mention {
    /// What kind.
    pub kind: MentionKind,
    /// The text as printed ("Fig. 3(b)").
    pub text: String,
    /// Numbers referred to ("3"; "2", "3", "4" for a range).
    pub numbers: Vec<String>,
    /// Where it is (one quad per line piece).
    pub quads: Vec<PageQuad>,
    /// The object it refers to (the first number's), when found: `(page, object id)`.
    #[serde(default)]
    pub target: Option<ObjectRef>,
}

/// A reference to an object in the same file.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ObjectRef {
    /// 1-based page.
    pub page: u32,
    /// The object's id.
    pub id: String,
}

/// An in-text citation marker ("[12]", "[3–5, 9]", "(Smith et al., 2020)").
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct CitationMarker {
    /// The text as printed.
    pub text: String,
    /// Where it is.
    pub quads: Vec<PageQuad>,
    /// Indexes into the reference list (0-based), in order.
    pub entries: Vec<u32>,
}

/// One entry of the reference list.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Reference {
    /// 0-based position in the list.
    pub index: u32,
    /// Printed label: "12" for "[12]", or `None` for author–year lists.
    #[serde(default)]
    pub label: Option<String>,
    /// The whole entry, line breaks joined.
    pub text: String,
    /// Authors as printed (surnames first when the entry gives them that way).
    #[serde(default)]
    pub authors: Vec<String>,
    /// Year.
    #[serde(default)]
    pub year: Option<String>,
    /// Title (best guess).
    #[serde(default)]
    pub title: Option<String>,
    /// DOI, lowercase.
    #[serde(default)]
    pub doi: Option<String>,
    /// arXiv identifier.
    #[serde(default)]
    pub arxiv: Option<String>,
    /// Web address in the entry.
    #[serde(default)]
    pub url: Option<String>,
    /// 1-based page of the entry's first line.
    pub page: u32,
    /// The entry's box on that page.
    pub rect: PageRect,
}

// --------------------------------------------------------------------- equations, tables ----

/// Input of `document.equation`: an equation object, or a printed number.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct EquationRequest {
    /// The file.
    pub file: BlobRef,
    /// The object, as returned by `document.objects`.
    #[serde(default)]
    pub object: Option<ObjectRef>,
    /// Or the printed number ("12", "2.3").
    #[serde(default)]
    pub number: Option<String>,
}

/// Output of `document.equation`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Equation {
    /// The equation object.
    pub object: DocObject,
    /// 1-based page.
    pub page: u32,
    /// LaTeX from the formula model, when it is available and the conversion succeeded.
    #[serde(default)]
    pub latex: Option<String>,
    /// Why there is no LaTeX.
    #[serde(default)]
    pub note: Option<String>,
    /// The equation as a PNG.
    pub image: BlobRef,
    /// Text of the equation from the PDF (often partial for math).
    pub text: String,
}

/// Input of `document.table`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct TableRequest {
    /// The file.
    pub file: BlobRef,
    /// The table object.
    pub object: ObjectRef,
}

/// One table cell.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Cell {
    /// Text.
    pub text: String,
    /// The number in it, when the cell is numeric ("1.2e-3", "45 %", "−0.7 ± 0.1" → −0.7).
    #[serde(default)]
    pub value: Option<f64>,
    /// Box on the page.
    pub rect: PageRect,
}

/// Output of `document.table`: cells in rows and columns, from the words inside the table box.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Table {
    /// The table object.
    pub object: DocObject,
    /// 1-based page.
    pub page: u32,
    /// Rows of cells (every row has the same number of columns).
    pub rows: Vec<Vec<Cell>>,
    /// How many leading rows are headers.
    pub header_rows: u32,
    /// Units found in header cells, per column ("K", "W/m²", "%"), when printed.
    pub units: Vec<Option<String>>,
    /// The table as CSV (RFC 4180).
    pub csv: String,
}

/// Output of `document.ocr`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Ocred {
    /// The new file: the original followed by an invisible text layer on scanned pages.
    pub file: BlobRef,
    /// Pages that received text.
    pub pages: Vec<u32>,
    /// Words written.
    pub words: u32,
}

/// Page models the engine can use, and whether each is present.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ModelInfo {
    /// Id ("egret-medium", "ppocrv5-det", …).
    pub id: String,
    /// What it does.
    pub purpose: String,
    /// Download size in bytes.
    pub size: u64,
    /// Present and verified.
    pub present: bool,
    /// Licence.
    pub licence: String,
    /// Shipped with the app (cannot be removed).
    pub bundled: bool,
}

/// Error kinds of the intelligence commands.
pub mod intel_errors {
    /// A model this needs is not downloaded (`document.models` lists them).
    pub const MODEL_MISSING: &str = "model_missing";
    /// No such object, equation number or table.
    pub const NO_OBJECT: &str = "no_object";
}

/// Hub events.
pub mod intel_events {
    /// A stage moved: `{ file, stage: StageState }`.
    pub const PREPARING: &str = "document.preparing";
    /// Every stage finished: `Preparation`.
    pub const PREPARED: &str = "document.prepared";
}
