//! Annotations (P1.M3.MOD.1–4, ADR-006).
//!
//! The annotation store is the truth; a PDF is an export target. Every position is in **page
//! space** (ADR-003). Annotations belong to a paper (not to one file version), so they survive
//! "Save into PDF", which gives the paper a new managed copy.

use ep_types::{BlobRef, PagePoint, PageQuad, PageRect, PaperId};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// What an annotation is. Each maps to one standard PDF subtype (ADR-006).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum AnnotationKind {
    /// Text mark-up: `/Highlight`.
    Highlight,
    /// Text mark-up: `/Underline`.
    Underline,
    /// Text mark-up: `/StrikeOut`.
    StrikeOut,
    /// Text mark-up: `/Squiggly`.
    Squiggly,
    /// A note (sticky note) at a point: `/Text`. Replies are notes with a `parent`.
    Note,
    /// Text drawn on the page: `/FreeText`.
    FreeText,
    /// Free-hand drawing: `/Ink`.
    Ink,
    /// `/Square`.
    Rectangle,
    /// `/Circle`.
    Ellipse,
    /// `/Line`.
    Line,
    /// `/Line` with an arrowhead at the end.
    Arrow,
}

impl AnnotationKind {
    /// Kinds drawn over text from the text's quads.
    #[must_use]
    pub fn is_text_markup(self) -> bool {
        matches!(
            self,
            Self::Highlight | Self::Underline | Self::StrikeOut | Self::Squiggly
        )
    }
}

/// What a mark means to the reader (Draft 2.2 §28.3). Written to `/Subj`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Category {
    /// A definition.
    Definition,
    /// A method.
    Method,
    /// A result.
    Result,
    /// A question.
    Question,
    /// Something to check.
    ToCheck,
}

impl Category {
    /// The `/Subj` text written to PDFs.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Definition => "Definition",
            Self::Method => "Method",
            Self::Result => "Result",
            Self::Question => "Question",
            Self::ToCheck => "To check",
        }
    }

    /// Read a `/Subj` text back (ignoring case and spacing).
    #[must_use]
    pub fn from_label(s: &str) -> Option<Self> {
        let k: String = s
            .chars()
            .filter(|c| c.is_alphanumeric())
            .flat_map(char::to_lowercase)
            .collect();
        Some(match k.as_str() {
            "definition" => Self::Definition,
            "method" => Self::Method,
            "result" => Self::Result,
            "question" => Self::Question,
            "tocheck" => Self::ToCheck,
            _ => return None,
        })
    }
}

/// One annotation.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Annotation {
    /// Stable id: `ep-…` for ours, the PDF's `/NM` (or `pdf-<object>-<generation>`) for imported
    /// ones. Written to `/NM`, so re-importing updates instead of duplicating.
    pub id: String,
    /// 1-based page number.
    pub page: u32,
    /// What it is.
    pub kind: AnnotationKind,
    /// Bounds on the page. For notes, the icon; for free text and shapes, the box; for mark-up,
    /// ink and lines, the area they cover (recomputed by the engine).
    pub rect: PageRect,
    /// Text mark-up: one quad per line piece, from the engine's character boxes.
    #[serde(default)]
    pub quads: Vec<PageQuad>,
    /// Ink: strokes, each a list of points.
    #[serde(default)]
    pub paths: Vec<Vec<PagePoint>>,
    /// Line and arrow: start and end.
    #[serde(default)]
    pub line: Option<[PagePoint; 2]>,
    /// Colour as `#rrggbb`.
    pub color: String,
    /// Opacity 0–1.
    #[serde(default = "one")]
    pub opacity: f32,
    /// Stroke width in points (ink, shapes, lines, underline).
    #[serde(default = "one")]
    pub width: f32,
    /// Free text: font size in points.
    #[serde(default)]
    pub font_size: Option<f32>,
    /// What the mark means.
    #[serde(default)]
    pub category: Option<Category>,
    /// The comment (note text, free text, or a comment on a mark).
    #[serde(default)]
    pub body: String,
    /// Text mark-up: the marked text (for the sidebar, search and citations).
    #[serde(default)]
    pub text: Option<String>,
    /// Who made it.
    #[serde(default)]
    pub author: Option<String>,
    /// Replies: the note this answers.
    #[serde(default)]
    pub parent: Option<String>,
    /// The thread is resolved (written as a `Review`/`Completed` state annotation).
    #[serde(default)]
    pub resolved: bool,
    /// Created, ms since the Unix epoch.
    pub created_ms: i64,
    /// Last changed, ms since the Unix epoch.
    pub modified_ms: i64,
    /// Imported from the PDF and not changed since: saving keeps the file's own object.
    #[serde(default)]
    pub pristine: bool,
}

fn one() -> f32 {
    1.0
}

/// Input naming a paper and its current managed copy.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct PaperFile {
    /// The paper (annotations belong to it).
    pub paper: PaperId,
    /// Its current file (annotations already in it are imported once).
    pub file: BlobRef,
}

/// One change in an edit.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum AnnotationOp {
    /// Add a new annotation (its id must be new).
    Add {
        /// The annotation.
        annotation: Annotation,
    },
    /// Replace an annotation with the same id.
    Change {
        /// The new version.
        annotation: Annotation,
    },
    /// Remove an annotation and its replies.
    Remove {
        /// Its id.
        id: String,
    },
}

/// Input of `document.annotations_apply`: changes applied together, undone together.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ApplyAnnotations {
    /// The paper.
    pub paper: PaperId,
    /// Changes, in order.
    pub ops: Vec<AnnotationOp>,
}

/// Input naming a paper.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct AnnotationsOf {
    /// The paper.
    pub paper: PaperId,
}

/// The annotations of a paper and the state of its history.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct AnnotationSet {
    /// The paper.
    pub paper: PaperId,
    /// Every annotation, by page then creation time.
    pub annotations: Vec<Annotation>,
    /// An edit can be undone.
    pub can_undo: bool,
    /// An undone edit can be redone.
    pub can_redo: bool,
    /// There are changes not yet saved into the PDF.
    pub unsaved: bool,
}

/// Output of `document.annotate`: the new version of the file, with every annotation written.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Annotated {
    /// The new managed copy (the previous bytes followed by an incremental update).
    pub file: BlobRef,
    /// Annotations written as new objects.
    pub written: u32,
    /// Annotations already in the file and unchanged (kept as they were).
    pub kept: u32,
}

/// Error kinds of the annotation commands.
pub mod annotation_errors {
    /// No library or project is open (annotations are stored in it).
    pub const NO_PROJECT_OPEN: &str = "no_project_open";
    /// An annotation is not valid (bad page, empty mark-up, unknown parent, …).
    pub const INVALID: &str = "invalid_annotation";
    /// `add` with an id that exists, or `change`/`remove` of one that does not.
    pub const CONFLICT: &str = "conflict";
    /// The PDF cannot be written (damaged structure lopdf cannot follow).
    pub const WRITE_FAILED: &str = "write_failed";
}

/// Hub events.
pub mod annotation_events {
    /// The annotations of a paper changed: `{ paper }`.
    pub const CHANGED: &str = "document.annotations_changed";
}
