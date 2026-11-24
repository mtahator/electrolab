//! PDF reading commands (P1.M2.DOC.1): info, text, render, thumbnails, outline, links.
//!
//! Every position is in **page space** (ADR-003): PDF points, origin at the top-left of the
//! visible page, y down, page rotation applied.

use ep_types::{BlobRef, PageRect};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Input naming one stored PDF.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct FileRequest {
    /// The stored PDF.
    pub file: BlobRef,
}

/// Size of one page in points, rotation applied.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct PageSize {
    /// Width.
    pub width: f32,
    /// Height.
    pub height: f32,
}

/// Output of `document.info`: everything the viewer needs before drawing anything.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct DocInfo {
    /// Number of pages.
    pub pages: u32,
    /// Size of every page, in order (so the scroll height never jumps).
    pub sizes: Vec<PageSize>,
    /// The file declares encryption.
    pub encrypted: bool,
}

/// Input naming one page.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct PageRequest {
    /// The stored PDF.
    pub file: BlobRef,
    /// 1-based page number.
    pub page: u32,
    /// Nobody is waiting: pages that are looked at go first.
    #[serde(default)]
    pub background: bool,
}

/// A run of characters (a word or a line) and its box.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct TextSpan {
    /// First character index in `PageText::text` (in characters, not bytes).
    pub start: u32,
    /// One past the last character.
    pub end: u32,
    /// Box around the run.
    pub rect: PageRect,
}

/// Output of `document.text`: the page's characters with their boxes, grouped into words and
/// lines (layout level 0; reading order across columns comes with the layout model in M4).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct PageText {
    /// 1-based page.
    pub page: u32,
    /// The characters, in content order. Line ends are `\n`; a soft hyphen is U+00AD.
    pub text: String,
    /// One box per character (`[x0, y0, x1, y1]`), or `None` for generated characters such as
    /// line breaks.
    pub boxes: Vec<Option<[f32; 4]>>,
    /// Words.
    pub words: Vec<TextSpan>,
    /// Lines.
    pub lines: Vec<TextSpan>,
    /// Characters the font could not map to text (U+FFFD, private use); never quote these.
    pub unknown: u32,
}

/// Input of `document.render`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct RenderRequest {
    /// The stored PDF.
    pub file: BlobRef,
    /// 1-based page.
    pub page: u32,
    /// Width of the image in device pixels (height follows the page's shape).
    pub width_px: u32,
    /// Page colours for dark papers: text and lines take `foreground`, the page `background`;
    /// colour figures keep their colours. `None`: the page as printed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub colors: Option<PageColors>,
    /// Background work (previews of pages ahead): waits behind what the user is looking at.
    #[serde(default)]
    pub background: bool,
    /// Draw the annotations stored in the file too (printing an annotated copy). The viewer
    /// draws annotations itself, so pages are drawn without them by default.
    #[serde(default)]
    pub annotations: bool,
}

/// Page colours (`#rrggbb`) for drawing a page on a dark paper.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct PageColors {
    /// The page.
    pub background: String,
    /// Text and lines.
    pub foreground: String,
}

/// A rendered page image (PNG in the render cache).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Rendered {
    /// 1-based page.
    pub page: u32,
    /// The PNG.
    pub image: BlobRef,
    /// Width in pixels.
    pub width_px: u32,
    /// Height in pixels.
    pub height_px: u32,
}

/// Input of `document.thumbnails`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ThumbnailsRequest {
    /// The stored PDF.
    pub file: BlobRef,
    /// Pages to render (empty: all).
    #[serde(default)]
    pub pages: Vec<u32>,
    /// Width of each thumbnail in device pixels.
    pub width_px: u32,
}

/// A place a link or bookmark points to.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[non_exhaustive]
pub enum LinkTarget {
    /// A page in the same document; `top` is the y to show at the top of the view.
    Page {
        /// 1-based page.
        page: u32,
        /// Page-space y, when the destination gives one.
        #[serde(default)]
        top: Option<f32>,
    },
    /// A web address (`http`, `https` or `mailto` only; others are reported as `unsupported`).
    Url {
        /// The address.
        url: String,
    },
    /// Anything else (launch actions, scripts, other files). Never followed.
    Unsupported,
}

/// One entry of the document's outline (bookmarks).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct OutlineItem {
    /// Title.
    pub title: String,
    /// Where it goes, if anywhere.
    #[serde(default)]
    pub target: Option<LinkTarget>,
    /// Sub-entries.
    #[serde(default)]
    pub children: Vec<OutlineItem>,
}

/// A link area on a page.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Link {
    /// The clickable area.
    pub rect: PageRect,
    /// Where it goes.
    pub target: LinkTarget,
}

/// Error kinds of the PDF commands (in addition to `not_found` and `unreadable`).
pub mod pdf_errors {
    /// The PDF needs a password.
    pub const PASSWORD_REQUIRED: &str = "password_required";
    /// Page number out of range.
    pub const BAD_PAGE: &str = "bad_page";
    /// The PDF backend did not answer in time (a pathological file).
    pub const TIMEOUT: &str = "timeout";
}
