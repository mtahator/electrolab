//! Contract of the Knowledge engine — digests of papers and asking the library.
//!
//! See `engines/knowledge/CONTRACT.md`.
#![deny(missing_docs)]

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

pub use ep_types::{BlobRef, PaperId};

/// Contract version; experimental until a second host uses it (Draft 2.2 §8.3).
pub const VERSION: &str = "0.1.0";

/// Version of the digest format; older digests are made again.
pub const DIGEST_VERSION: u32 = 1;

/// Command names.
pub mod commands {
    /// Make (or finish) the digest of a paper.
    pub const DIGEST_PAPER: &str = "knowledge.digest_paper";
    /// What digests there are.
    pub const DIGESTS: &str = "knowledge.digests";
    /// The digest of a paper.
    pub const GET_DIGEST: &str = "knowledge.get_digest";
    /// Make digests of the library in the background.
    pub const DIGEST_LIBRARY: &str = "knowledge.digest_library";
    /// Stop making digests.
    pub const DIGEST_STOP: &str = "knowledge.digest_stop";
    /// State of making digests.
    pub const DIGEST_STATUS: &str = "knowledge.digest_status";
    /// Ask the library.
    pub const FIND: &str = "knowledge.find";
}

/// Event names.
pub mod events {
    /// State of making digests: `DigestStatus`.
    pub const DIGESTING: &str = "knowledge.digesting";
    /// A digest was made: `{ paper, level }`.
    pub const DIGESTED: &str = "knowledge.digested";
    /// A search so far: `{ id, step, result }`.
    pub const FINDING: &str = "knowledge.finding";
}

/// Error kinds besides those of the engines used.
pub mod errors {
    /// The question holds no word to look for.
    pub const NOTHING_TO_FIND: &str = "nothing_to_find";
    /// The paper has no text.
    pub const NO_TEXT: &str = "no_text";
}

/// How much a digest holds.
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
#[schemars(rename = "DigestLevel")]
pub enum Level {
    /// Headings, captions and telling words (no model).
    #[default]
    Basic,
    /// Written summaries too.
    Full,
}

/// Where a section's summary comes from.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
#[schemars(rename = "SummarySource")]
pub enum Source {
    /// Written by the model and checked against the section.
    Written,
    /// The section's own sentences.
    Quoted,
    /// There is none.
    #[default]
    None,
}

/// What a section does.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
#[allow(missing_docs)] // the names say it
#[schemars(rename = "SectionRole")]
pub enum Role {
    Introduction,
    Background,
    Method,
    Data,
    Results,
    Discussion,
    Conclusion,
    Instructions,
    Legal,
    Administrative,
    Appendix,
    References,
    #[default]
    Other,
}

/// A sentence of the paper.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Quote {
    /// 1-based page.
    pub page: u32,
    /// Word for word.
    pub text: String,
}

/// The digest of one section.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct SectionDigest {
    /// Stable within the digest (`s12`).
    pub id: String,
    /// Heading, or the pages.
    pub title: String,
    /// Depth of the heading, from 1.
    pub level: u32,
    /// First and last page (1-based).
    pub pages: (u32, u32),
    /// What it does.
    #[serde(default)]
    pub role: Role,
    /// 1–3 sentences (empty when there is none).
    #[serde(default)]
    pub summary: String,
    /// Subjects, cleaned.
    #[serde(default)]
    pub tags: Vec<String>,
    /// Important words, as they stand in the section.
    #[serde(default)]
    pub terms: Vec<String>,
    /// Sentences that carry the summary.
    #[serde(default)]
    pub support: Vec<Quote>,
    /// Where the summary comes from.
    #[serde(default)]
    pub source: Source,
}

/// Kind of a labelled object or of a place.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
#[schemars(rename = "PlaceKind")]
pub enum Kind {
    /// A figure or chart.
    Figure,
    /// A table.
    Table,
    /// An equation.
    Equation,
    /// A section.
    Section,
    /// Running text.
    #[default]
    Text,
}

/// A labelled figure, table or equation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[schemars(rename = "DigestObject")]
pub struct ObjectDigest {
    /// Kind.
    pub kind: Kind,
    /// Label as printed (`Figure 2.3`).
    pub label: String,
    /// 1-based page.
    pub page: u32,
    /// Top on its page (points).
    pub y: f32,
    /// Id in the Document engine.
    pub object: String,
    /// Its area on the page in points (left, top, right, bottom).
    #[serde(default)]
    pub rect: Option<[f32; 4]>,
    /// Caption or text of it.
    #[serde(default)]
    pub caption: String,
}

/// The digest of a paper.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Digest {
    /// Format.
    pub version: u32,
    /// Paper id.
    pub paper: String,
    /// Pages of the document.
    pub pages: u32,
    /// How much it holds.
    pub level: Level,
    /// Every section that can have a summary was looked at.
    #[serde(default)]
    pub complete: bool,
    /// Title.
    pub title: String,
    /// What the document is (`research paper`, `report`, …).
    #[serde(default)]
    pub kind: String,
    /// 3–5 sentences (empty when basic).
    #[serde(default)]
    pub summary: String,
    /// Subjects, cleaned.
    #[serde(default)]
    pub topics: Vec<String>,
    /// Questions it can answer.
    #[serde(default)]
    pub questions: Vec<String>,
    /// Sections in order.
    pub sections: Vec<SectionDigest>,
    /// Labelled objects in order.
    pub objects: Vec<ObjectDigest>,
    /// Model that wrote it.
    #[serde(default)]
    pub model: Option<String>,
    /// When, ms since the Unix epoch.
    pub made_ms: i64,
}

/// What there is for a paper.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct DigestInfo {
    /// Paper id.
    pub paper: String,
    /// How much; missing when there is none.
    #[serde(default)]
    pub level: Option<Level>,
    /// Finished.
    pub complete: bool,
    /// Summary.
    pub summary: String,
    /// Subjects.
    pub topics: Vec<String>,
    /// What the document is.
    pub kind: String,
    /// Number of sections.
    pub sections: u32,
    /// Model.
    #[serde(default)]
    pub model: Option<String>,
    /// When.
    pub made_ms: i64,
}

/// Input naming a paper.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct DigestRequest {
    /// Paper id.
    pub paper: PaperId,
    /// Make it again.
    #[serde(default)]
    pub force: bool,
}

/// Input naming papers (all of the library when missing).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[schemars(rename = "DigestPapers")]
pub struct Papers {
    /// Paper ids.
    #[serde(default)]
    pub papers: Option<Vec<PaperId>>,
}

/// Input of `knowledge.digest_library`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct DigestLibrary {
    /// Written summaries too (needs a model).
    #[serde(default)]
    pub full: bool,
    /// Written summaries of these papers, next in the row (also while it works).
    #[serde(default)]
    pub papers: Option<Vec<PaperId>>,
    /// Make those of `papers` anew.
    #[serde(default)]
    pub force: bool,
}

/// State of making digests.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct DigestStatus {
    /// Working.
    pub running: bool,
    /// Written summaries are being made.
    pub full: bool,
    /// Papers done.
    pub done: u32,
    /// Papers.
    pub total: u32,
    /// Id of the paper at work.
    #[serde(default)]
    pub paper: Option<String>,
    /// Papers waiting because they were asked for.
    #[serde(default)]
    pub next: Vec<String>,
    /// Title of the paper at work.
    #[serde(default)]
    pub current: Option<String>,
    /// Section at work (`12 / 30`).
    #[serde(default)]
    pub step: Option<String>,
    /// Why it stopped.
    #[serde(default)]
    pub error: Option<String>,
}

/// Ask for progress as events.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[schemars(rename = "FindStream")]
pub struct Stream {
    /// Id carried by every event.
    pub id: String,
}

/// Input of `knowledge.find`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct FindRequest {
    /// A question, or a description of what is looked for.
    pub question: String,
    /// Only in these papers.
    #[serde(default)]
    pub papers: Option<Vec<PaperId>>,
    /// Send progress.
    #[serde(default)]
    pub stream: Option<Stream>,
}

/// A paper in which it may be.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[schemars(rename = "RelatedPaper")]
pub struct Related {
    /// Paper id.
    pub paper: String,
    /// Why, in a few words (from the model).
    #[serde(default)]
    pub why: Option<String>,
    /// Score by words.
    pub score: f32,
}

/// A place where it may be.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Place {
    /// Paper id.
    pub paper: String,
    /// 1-based page.
    pub page: u32,
    /// Top on its page (points).
    pub y: f32,
    /// Kind.
    pub kind: Kind,
    /// Label as printed.
    #[serde(default)]
    pub label: Option<String>,
    /// Id in the Document engine.
    #[serde(default)]
    pub object: Option<String>,
    /// Area of the object on its page in points (left, top, right, bottom).
    #[serde(default)]
    pub rect: Option<[f32; 4]>,
    /// Words of the paper.
    pub quote: String,
    /// Which words met here.
    pub terms: Vec<String>,
    /// Section it is in.
    #[serde(default)]
    pub section: Option<String>,
    /// Score by words.
    pub score: f32,
    /// Chosen as nearest.
    #[serde(default)]
    pub nearest: bool,
}

/// Hits in a paper that is not among the related ones.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct OtherHits {
    /// Paper id.
    pub paper: String,
    /// Hits.
    pub hits: u32,
}

/// Which digests a search could use.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct DigestCount {
    /// With written summaries.
    pub full: u32,
    /// Basic.
    pub basic: u32,
    /// Papers asked.
    pub total: u32,
}

/// Output of `knowledge.find`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct FindResult {
    /// The question.
    pub question: String,
    /// What is looked for.
    pub looking_for: Kind,
    /// Words searched.
    pub terms: Vec<String>,
    /// Papers it can be in, best first.
    pub related: Vec<Related>,
    /// Places, nearest first.
    pub places: Vec<Place>,
    /// Hits elsewhere.
    pub others: Vec<OtherHits>,
    /// A model helped.
    pub used_model: bool,
    /// Which.
    #[serde(default)]
    pub model: Option<String>,
    /// Digests used.
    pub digests: DigestCount,
    /// Time taken.
    pub total_ms: u64,
}
