//! Contract of the Document engine — the only part other engines and hosts may use.
//!
//! So far: `document.search` (experimental). See `engines/document/CONTRACT.md` and
//! `docs/design/document-search.md`.
#![deny(missing_docs)]

use serde::{Deserialize, Serialize};

mod annotations;
mod pdf;
pub use annotations::*;
pub use pdf::*;

pub use ep_types::PageRect;

/// Contract version (semantic); experimental until used by a second engine or host (Draft 2.2 §8.3).
pub const VERSION: &str = "0.3.0";

/// A paper the caller wants searched. Placeholder for `Ref<Paper>` (Phase 1 M1.TYP).
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
pub struct PaperRef(pub String);

// ---------------------------------------------------------------- document.search ---------

/// Input of `document.search`.
#[derive(Clone, Debug, Serialize, Deserialize, schemars::JsonSchema)]
pub struct SearchRequest {
    /// Papers to search (one, several, or a whole collection).
    pub papers: Vec<PaperRef>,
    /// What to look for; each term is searched and counted separately.
    pub terms: Vec<Term>,
    /// How strict a match is.
    #[serde(default, rename = "match")]
    pub matching: MatchOptions,
    /// What is returned around each hit.
    #[serde(default)]
    pub context: ContextOptions,
    /// Which pages and sections are searched.
    #[serde(default)]
    pub scope: Scope,
    /// Caps on the size of the result.
    #[serde(default)]
    pub limits: Limits,
    /// Order of hits.
    #[serde(default)]
    pub sort: Sort,
}

/// One thing to look for: a word, a symbol (`C_p`) or a phrase (`heat capacity`).
#[derive(Clone, Debug, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Term {
    /// The text as the caller writes it.
    pub text: String,
    /// Optional caller's name for the term, returned with each hit.
    #[serde(default)]
    pub label: Option<String>,
}

/// How a word in the query may match a word in the paper.
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum MatchMode {
    /// The written words after normalisation (case, accents, Unicode forms).
    Exact,
    /// Also plural and simple English endings (equation/equations, solve/solved/solving).
    #[default]
    WordForms,
    /// Also misspellings and OCR errors, down to `similarity`.
    Fuzzy,
}

/// Match options. Defaults suit both people and the language model.
// Independent on/off switches that a caller (often the language model) sets by name in JSON;
// separate bools read better there than enums.
#[allow(clippy::struct_excessive_bools)]
#[derive(Clone, Debug, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(default)]
pub struct MatchOptions {
    /// Strictness.
    pub mode: MatchMode,
    /// For `Fuzzy`: minimum similarity per word, 0.0–1.0 (1 − edit distance ÷ longer length).
    pub similarity: f32,
    /// Words shorter than this (in letters) always match exactly, even in `Fuzzy` mode.
    pub min_fuzzy_length: usize,
    /// Symbols and their written variants match: `Cp`, `C_p`, `C_{p}`, `C p`, `Cₚ`, `𝐶𝑝`;
    /// `x²` = `x^2`; `α` = `alpha` = `\alpha`.
    pub notation: bool,
    /// Distinguish upper and lower case (`Re` vs `re`).
    pub case_sensitive: bool,
    /// Match whole words only; false also finds the term inside longer words.
    pub whole_word: bool,
    /// The term may be the start of a longer word (`therm` → `thermal`).
    pub prefix: bool,
    /// For phrases: number of other words allowed between the phrase's words.
    pub phrase_slop: usize,
    /// Treat accented and unaccented letters as equal (`Schrödinger` = `Schrodinger`).
    pub fold_accents: bool,
}

impl Default for MatchOptions {
    fn default() -> Self {
        Self {
            mode: MatchMode::WordForms,
            similarity: 0.8,
            min_fuzzy_length: 4,
            notation: true,
            case_sensitive: false,
            whole_word: true,
            prefix: false,
            phrase_slop: 0,
            fold_accents: true,
        }
    }
}

/// The unit of context returned around a hit.
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum ContextUnit {
    /// `before` words before and `after` words after.
    #[default]
    Words,
    /// The whole sentence the hit is in.
    Sentence,
    /// No context, only the match.
    None,
}

/// What surrounds each hit.
#[derive(Clone, Debug, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(default)]
pub struct ContextOptions {
    /// Context unit.
    pub unit: ContextUnit,
    /// Words before the hit (for `Words`).
    pub before: usize,
    /// Words after the hit (for `Words`).
    pub after: usize,
    /// Hits closer than this many words share one snippet.
    pub merge_within: usize,
    /// Let context continue onto the previous or next page.
    pub cross_pages: bool,
}

impl Default for ContextOptions {
    fn default() -> Self {
        Self {
            unit: ContextUnit::Words,
            before: 12,
            after: 12,
            merge_within: 0,
            cross_pages: false,
        }
    }
}

/// An inclusive range of 1-based page numbers.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct PageRange {
    /// First page.
    pub first: u32,
    /// Last page.
    pub last: u32,
}

/// Which parts of the papers are searched.
#[derive(Clone, Debug, Default, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(default)]
pub struct Scope {
    /// Only these pages (empty = all).
    pub pages: Vec<PageRange>,
    /// Only sections whose heading contains one of these (case-insensitive; empty = all).
    pub include_sections: Vec<String>,
    /// Skip sections whose heading contains one of these (e.g. `References`).
    pub exclude_sections: Vec<String>,
}

/// Caps on the size of a result. Counts always include every occurrence.
#[derive(Clone, Debug, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(default)]
pub struct Limits {
    /// Most hits returned in total.
    pub max_hits: usize,
    /// Most hits returned per term.
    pub max_hits_per_term: usize,
    /// Most characters of context returned in total (keeps a model's context small).
    pub max_context_chars: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_hits: 200,
            max_hits_per_term: 50,
            max_context_chars: 20_000,
        }
    }
}

/// Order of hits.
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum Sort {
    /// Paper, page and position in the text.
    #[default]
    Position,
    /// Best match first (exact before fuzzy), then position.
    Score,
}

/// How a hit matched its term.
#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum MatchKind {
    /// Same words after normalisation.
    Exact,
    /// A written variant of the same symbol (`C_p` for `Cp`).
    Notation,
    /// Another form of the word (plural, -ed, -ing).
    WordForm,
    /// The term is the start of the word.
    Prefix,
    /// The term is inside the word.
    Substring,
    /// Similar spelling, within `similarity`.
    Fuzzy,
}

/// One occurrence of a term.
#[derive(Clone, Debug, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Hit {
    /// Index of the term in the request.
    pub term: usize,
    /// The term's label, if the caller gave one.
    pub label: Option<String>,
    /// The paper.
    pub paper: PaperRef,
    /// 1-based page number.
    pub page: u32,
    /// The text exactly as written in the paper.
    pub matched: String,
    /// Context before the match (plain text).
    pub before: String,
    /// Context after the match (plain text).
    pub after: String,
    /// How it matched.
    pub kind: MatchKind,
    /// 1.0 for exact; the fuzzy similarity otherwise.
    pub similarity: f32,
    /// Boxes of the matched characters, one per line (for highlighting).
    pub rects: Vec<PageRect>,
    /// Heading of the section the hit is in, when known.
    pub section: Option<String>,
    /// Character offsets `[start, end)` of the match in the paper's text.
    pub char_range: (usize, usize),
    /// Other hits folded into this snippet by `merge_within`.
    pub merged: usize,
}

/// Every occurrence of one term, whether or not it was returned as a hit.
#[derive(Clone, Debug, Default, Serialize, Deserialize, schemars::JsonSchema)]
pub struct TermCount {
    /// Index of the term in the request.
    pub term: usize,
    /// Total occurrences.
    pub total: usize,
    /// `(page, occurrences)`, pages in order.
    pub per_page: Vec<(u32, usize)>,
    /// `(kind, occurrences)`.
    pub per_kind: Vec<(MatchKind, usize)>,
}

/// Output of `document.search`.
#[derive(Clone, Debug, Default, Serialize, Deserialize, schemars::JsonSchema)]
pub struct SearchResult {
    /// The hits, in the requested order.
    pub hits: Vec<Hit>,
    /// Per-term counts (every occurrence, even when hits were cut).
    pub counts: Vec<TermCount>,
    /// True when a limit cut the hits.
    pub truncated: bool,
    /// Time spent, in microseconds (filled in by the facade, not the pure core).
    pub elapsed_us: u64,
}

/// Errors of `document.search`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[non_exhaustive]
pub enum SearchError {
    /// The paper has no extracted text yet; ask again after preparation.
    PaperNotPrepared(PaperRef),
    /// No term, or only empty terms.
    EmptyQuery,
    /// A term longer than 200 characters.
    TermTooLong(usize),
    /// An option is out of range.
    InvalidOption(String),
}

impl std::fmt::Display for SearchError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::PaperNotPrepared(p) => write!(f, "paper {} has no extracted text yet", p.0),
            Self::EmptyQuery => write!(f, "nothing to search for"),
            Self::TermTooLong(i) => write!(f, "term {i} is longer than 200 characters"),
            Self::InvalidOption(o) => write!(f, "invalid option: {o}"),
        }
    }
}

impl std::error::Error for SearchError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn minimal_json_request_uses_defaults() {
        let req: SearchRequest =
            serde_json::from_str(r#"{"papers":["p1"],"terms":[{"text":"C_p"}]}"#).expect("parse");
        assert_eq!(req.matching.mode, MatchMode::WordForms);
        assert!(req.matching.notation);
        assert_eq!(req.context.before, 12);
        assert_eq!(req.limits.max_hits, 200);
    }
}

// ---------------------------------------------------------------- document.metadata -------

/// Command names.
pub mod commands {
    /// Read title, authors, DOI and page count from a stored PDF.
    pub const METADATA: &str = "document.metadata";
    /// Page count and page sizes.
    pub const INFO: &str = "document.info";
    /// Characters, words and lines of a page, with boxes.
    pub const TEXT: &str = "document.text";
    /// A page as an image.
    pub const RENDER: &str = "document.render";
    /// Small page images.
    pub const THUMBNAILS: &str = "document.thumbnails";
    /// The PDF's bookmarks.
    pub const OUTLINE: &str = "document.outline";
    /// Link areas of a page.
    pub const LINKS: &str = "document.links";
    /// Notation-aware word search with context.
    pub const SEARCH: &str = "document.search";
    /// The annotations of a paper (imports those already in its PDF the first time).
    pub const ANNOTATIONS: &str = "document.annotations";
    /// Add, change or remove annotations as one undoable edit.
    pub const ANNOTATIONS_APPLY: &str = "document.annotations_apply";
    /// Undo the last annotation edit of a paper.
    pub const ANNOTATIONS_UNDO: &str = "document.annotations_undo";
    /// Redo the last undone annotation edit of a paper.
    pub const ANNOTATIONS_REDO: &str = "document.annotations_redo";
    /// Write a paper's annotations into a new version of its PDF (incremental update).
    pub const ANNOTATE: &str = "document.annotate";
}

/// Input of `document.metadata`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct MetadataRequest {
    /// The stored PDF.
    pub file: ep_types::BlobRef,
}

/// Output of `document.metadata`: what the file says about itself, cleaned. Fields are `None`
/// rather than guessed when the file's values look like junk.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct PdfMetadata {
    /// Title.
    pub title: Option<String>,
    /// Authors in order.
    pub authors: Vec<String>,
    /// DOI, lowercase.
    pub doi: Option<String>,
    /// Number of pages.
    pub pages: u32,
    /// Subject line (often journal and year).
    pub subject: Option<String>,
    /// The file is encrypted (fields may be missing).
    pub encrypted: bool,
}

/// Error kinds reported in `CommandError::Failed { kind }` by the Document engine.
pub mod errors {
    /// The blob is not available.
    pub const NOT_FOUND: &str = "not_found";
    /// The file is not a readable PDF.
    pub const UNREADABLE: &str = "unreadable";
}
