//! Contract of the Understanding engine — explaining and understanding a paper.
//!
//! See `engines/understanding/CONTRACT.md` (Draft 2.2 §14.5, §28.6).
#![deny(missing_docs)]

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

pub use ep_document_contract::ObjectRef;
pub use ep_types::{BlobRef, PageRect};

/// Contract version; experimental until a second host uses it (Draft 2.2 §8.3).
pub const VERSION: &str = "0.4.0";

/// Command names.
pub mod commands {
    /// Explain a selection: a fast draft, then a version checked against the paper.
    pub const EXPLAIN_SPAN: &str = "understanding.explain_span";
    /// Explain what is at a place on a page: a paragraph, an equation, a figure or a table.
    pub const EXPLAIN_REGION: &str = "understanding.explain_region";
    /// Explain an equation.
    pub const EXPLAIN_EQUATION: &str = "understanding.explain_equation";
    /// Explain a figure.
    pub const EXPLAIN_FIGURE: &str = "understanding.explain_figure";
    /// Answer a question from a table's cells.
    pub const TABLE_ANSWER: &str = "understanding.table_answer";
    /// Look at the pictures of a paper (with consent) and keep what was seen.
    pub const LOOK_AT_FIGURES: &str = "understanding.look_at_figures";
    /// What was seen in a paper's pictures.
    pub const FIGURES_SEEN: &str = "understanding.figures_seen";
    /// Suggest folders for papers (nothing is moved).
    pub const SUGGEST_FOLDERS: &str = "understanding.suggest_folders";
}

/// Event names.
pub mod events {
    /// The answer so far: `{ id, phase: draft | checking | check, text }`.
    pub const EXPLAINING: &str = "understanding.explaining";
    /// The symbols of an equation as they are found: `{ id, symbols: [Symbol] }`.
    pub const SYMBOLS: &str = "understanding.symbols";
    /// Pictures looked at: `{ id, done, total, label, text }` (the text of one picture grows).
    pub const LOOKING: &str = "understanding.looking";
    /// Progress of a sorting: `{ id, step: reading | thinking | sorting, done, total }`.
    pub const SORTING: &str = "understanding.sorting";
}

/// Error kinds besides those of `inference.complete`.
pub mod errors {
    /// Nothing was selected.
    pub const EMPTY_SELECTION: &str = "empty_selection";
    /// Nothing is at this place.
    pub const NO_OBJECT: &str = "no_object";
    /// The object is no table, or holds no cells.
    pub const NOT_A_TABLE: &str = "not_a_table";
    /// The table is too large to be asked.
    pub const TOO_LARGE: &str = "too_large";
    /// No question.
    pub const EMPTY_QUESTION: &str = "empty_question";
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
    /// How deep.
    #[serde(default)]
    pub depth: Depth,
    /// Stream the answer.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stream: Option<Stream>,
}

/// How deep an explanation goes.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
#[schemars(rename = "ExplainDepth")]
pub enum Depth {
    /// In plain words.
    Simpler,
    /// As usual.
    #[default]
    Normal,
    /// With the details.
    Deeper,
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

/// Input of `understanding.explain_region`, `explain_equation` and `explain_figure`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ExplainRegion {
    /// The paper's file.
    pub file: BlobRef,
    /// 1-based page.
    pub page: u32,
    /// The object, as returned by `document.objects`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub object: Option<String>,
    /// Or a place: the object it covers best, or the text in it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rect: Option<PageRect>,
    /// The paper in the library (what was seen in its figures is kept with it).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub paper: Option<String>,
    /// How deep.
    #[serde(default)]
    pub depth: Depth,
    /// Stream the answer.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stream: Option<Stream>,
}

/// What was explained.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExplainedKind {
    /// Text.
    Paragraph,
    /// A displayed equation.
    Equation,
    /// A figure.
    Figure,
    /// A table.
    Table,
}

/// What an explanation was made from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Basis {
    /// The caption.
    Caption,
    /// The text itself, or the text around it.
    Text,
    /// The places that mention it.
    Mentions,
    /// The words inside the figure.
    Words,
    /// The cells of the table.
    Cells,
    /// The formula.
    Formula,
    /// What was seen in the picture.
    Picture,
}

/// A symbol of an equation and what the paper says it means.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Symbol {
    /// LaTeX of the symbol, or the characters printed.
    pub symbol: String,
    /// As plain text ("`T_in`").
    pub shown: String,
    /// Its meaning, word for word from the paper; missing when the paper does not say it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub meaning: Option<String>,
    /// Its unit, when the paper prints one with the meaning.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unit: Option<String>,
    /// 1-based page where the meaning stands.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub page: Option<u32>,
    /// Top of that place (points).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub y: Option<f32>,
    /// The sentence it stands in.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quote: Option<String>,
}

/// Whether the units of a formula agree.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum UnitsResult {
    /// Every unit is known and they agree.
    Consistent,
    /// Every unit needed is known and they do not agree.
    Inconsistent,
    /// Not enough is known, or the formula could not be read.
    Unknown,
}

/// The units check of an equation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct UnitsCheck {
    /// The result.
    pub result: UnitsResult,
    /// Units of the parts compared, in base units, in the order they stand (unknown ones left out).
    pub sides: Vec<String>,
    /// Symbols whose unit is not known.
    pub missing: Vec<String>,
    /// The formula could not be read.
    pub unread: bool,
}

/// What was seen in one figure.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct FigureSeen {
    /// The figure.
    pub object: ObjectRef,
    /// Its printed label.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// What was seen.
    pub text: String,
    /// Model that looked.
    pub model: String,
    /// When (ms since 1970).
    pub at_ms: u64,
}

/// Output of `understanding.explain_region`, `explain_equation` and `explain_figure`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ObjectExplanation {
    /// What it is.
    pub kind: ExplainedKind,
    /// 1-based page.
    pub page: u32,
    /// The object, when it is one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub object: Option<String>,
    /// Its printed label.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// The explanation.
    pub text: String,
    /// How far it was checked.
    pub status: ExplainStatus,
    /// Places of the paper that were used.
    pub evidence: Vec<Evidence>,
    /// Equations: the symbols.
    #[serde(default)]
    pub symbols: Vec<Symbol>,
    /// Equations: the units check.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub units: Option<UnitsCheck>,
    /// Equations: the LaTeX, when the formula model read it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub latex: Option<String>,
    /// Figures: what was seen in the picture.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seen: Option<FigureSeen>,
    /// What the explanation was made from.
    pub based_on: Vec<Basis>,
    /// Sentences left out because they named numbers that stand nowhere.
    pub left_out: u32,
    /// Model that answered.
    pub model: String,
    /// Times.
    pub timings: ExplainTimings,
}

/// Input of `understanding.table_answer`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct TableQuestion {
    /// The paper's file.
    pub file: BlobRef,
    /// The table.
    pub object: ObjectRef,
    /// The question.
    pub question: String,
}

/// Whether a cell is the answer or was used for it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CellRole {
    /// The answer stands in it.
    Answer,
    /// It was used.
    Used,
}

/// A cell of the table that was used.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct CellRef {
    /// Row, 0-based, headings counted (as in `document.table`).
    pub row: u32,
    /// Column, 0-based.
    pub col: u32,
    /// What stands in it.
    pub text: String,
    /// Its box on the page.
    pub rect: PageRect,
    /// Answer or used.
    pub role: CellRole,
}

/// Whether a table answered.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TableStatus {
    /// Taken or computed from the cells.
    Computed,
    /// The table does not give it (`reason` says why).
    NotInTable,
}

/// Why a table does not answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TableReason {
    /// It does not hold it.
    NotInTable,
    /// The cells are no numbers.
    NotNumbers,
    /// The cells have different units.
    DifferentUnits,
    /// Comparing needs exactly two cells.
    NotTwo,
}

/// Output of `understanding.table_answer`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct TableAnswer {
    /// The question.
    pub question: String,
    /// The answer, written by code; empty when there is none.
    pub text: String,
    /// What was done: value, list, largest, smallest, average, sum, count, difference, ratio,
    /// change or none.
    pub op: String,
    /// The cells.
    pub cells: Vec<CellRef>,
    /// The number, when the answer is one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<f64>,
    /// Its unit.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unit: Option<String>,
    /// Whether there is an answer.
    pub status: TableStatus,
    /// Why not.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<TableReason>,
    /// Model that understood the question.
    pub model: String,
    /// Time taken.
    pub total_ms: u64,
}

/// Input of `understanding.look_at_figures`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct LookAtFigures {
    /// The paper in the library.
    pub paper: String,
    /// Its file.
    pub file: BlobRef,
    /// Which figures; without them every one that was not looked at yet.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub objects: Option<Vec<ObjectRef>>,
    /// The person agreed.
    #[serde(default)]
    pub consent: bool,
    /// Send progress.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stream: Option<Stream>,
}

/// Input of `understanding.figures_seen`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct FiguresSeenRequest {
    /// The paper in the library.
    pub paper: String,
    /// Its file.
    pub file: BlobRef,
}

/// Output of `understanding.look_at_figures` and `figures_seen`: what is kept for this file.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct FiguresSeen {
    /// Figures in the order of the paper.
    pub figures: Vec<FigureSeen>,
    /// Figures of the paper.
    pub total: u32,
    /// Model that looked now.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// Time taken.
    pub total_ms: u64,
}
