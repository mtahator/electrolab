//! PDF backend port and the pure rules around it (P1.M2.DOC.2–3, 5–6).
//!
//! The adapter (`PDFium`) returns raw values in PDF user space (points, y up, unrotated). Here they
//! become page space (ADR-003: top-left origin, y down, rotation and crop box applied), and
//! characters become words and lines.

use std::path::Path;

use ep_document_contract::{LinkTarget, OutlineItem, PageSize, PageText, TextSpan};
use ep_types::PageRect;

/// How a page's user space maps to what is shown.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PageGeom {
    /// Clockwise rotation in degrees: 0, 90, 180 or 270.
    pub rotation: u16,
    /// Crop box in user space: `[x0, y0, x1, y1]`.
    pub crop: [f32; 4],
}

impl PageGeom {
    fn wh(&self) -> (f32, f32) {
        (
            (self.crop[2] - self.crop[0]).abs(),
            (self.crop[3] - self.crop[1]).abs(),
        )
    }

    /// Shown size, rotation applied.
    #[must_use]
    pub fn size(&self) -> PageSize {
        let (w, h) = self.wh();
        if self.rotation % 180 == 90 {
            PageSize {
                width: h,
                height: w,
            }
        } else {
            PageSize {
                width: w,
                height: h,
            }
        }
    }

    /// A user-space point in page space.
    #[must_use]
    #[allow(clippy::many_single_char_names)] // coordinate algebra reads best with short names
    pub fn to_page(&self, x: f32, y: f32) -> (f32, f32) {
        let (w, h) = self.wh();
        let (u, v) = (
            x - self.crop[0].min(self.crop[2]),
            y - self.crop[1].min(self.crop[3]),
        );
        match self.rotation {
            90 => (v, u),
            180 => (w - u, v),
            270 => (h - v, w - u),
            _ => (u, h - v),
        }
    }

    /// A page-space point back in user space (inverse of [`PageGeom::to_page`]).
    #[must_use]
    #[allow(clippy::many_single_char_names)]
    pub fn to_user(&self, px: f32, py: f32) -> (f32, f32) {
        let (w, h) = self.wh();
        let (u, v) = match self.rotation {
            90 => (py, px),
            180 => (w - px, py),
            270 => (w - py, h - px),
            _ => (px, h - py),
        };
        (
            u + self.crop[0].min(self.crop[2]),
            v + self.crop[1].min(self.crop[3]),
        )
    }

    /// A user-space rectangle (`left, bottom, right, top`) in page space.
    #[must_use]
    pub fn rect(&self, page: u32, r: [f32; 4]) -> PageRect {
        let (ax, ay) = self.to_page(r[0], r[1]);
        let (bx, by) = self.to_page(r[2], r[3]);
        PageRect {
            page,
            x0: ax.min(bx),
            y0: ay.min(by),
            x1: ax.max(bx),
            y1: ay.max(by),
        }
    }
}

/// One character as the backend reports it: a UTF-16 code unit and its box in user space.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RawChar {
    /// UTF-16 code unit (surrogates arrive as two consecutive chars).
    pub unit: u32,
    /// `[left, bottom, right, top]` in user space, if the character has a box.
    pub bounds: Option<[f32; 4]>,
}

/// A destination as the backend reports it.
#[derive(Clone, Debug, PartialEq)]
pub enum RawDest {
    /// 0-based page index and an optional user-space y for the top of the view.
    Page {
        /// 0-based page.
        index: u32,
        /// User-space y.
        y: Option<f32>,
    },
    /// A URI action.
    Uri(String),
    /// Anything else.
    Other,
}

/// A bookmark as the backend reports it.
#[derive(Clone, Debug, PartialEq)]
pub struct RawBookmark {
    /// Title.
    pub title: String,
    /// Destination.
    pub dest: Option<RawDest>,
    /// Children.
    pub children: Vec<RawBookmark>,
}

/// A link annotation as the backend reports it.
#[derive(Clone, Debug, PartialEq)]
pub struct RawLink {
    /// `[left, bottom, right, top]` in user space.
    pub rect: [f32; 4],
    /// Destination.
    pub dest: RawDest,
}

/// A rendered page, 8-bit RGBA, rows top to bottom.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Bitmap {
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// `width * height * 4` bytes.
    pub rgba: Vec<u8>,
}

/// Why the backend failed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BackendError {
    /// Not a readable PDF (damaged, unsupported).
    Unreadable(String),
    /// The PDF needs a password.
    PasswordRequired,
    /// Page index out of range.
    BadPage(u32),
    /// The backend did not answer in time.
    Timeout,
}

impl std::fmt::Display for BackendError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unreadable(m) => write!(f, "not a readable PDF: {m}"),
            Self::PasswordRequired => f.write_str("the PDF needs a password"),
            Self::BadPage(p) => write!(f, "page {p} does not exist"),
            Self::Timeout => f.write_str("the PDF took too long to process"),
        }
    }
}

/// The PDF backend port (`PDFium` in production). Pages are 0-based here, 1-based in the contract.
pub trait PdfBackend: Send + Sync {
    /// Geometry of every page, and whether the file declares encryption.
    ///
    /// # Errors
    /// See [`BackendError`].
    fn info(&self, file: &Path) -> Result<(Vec<PageGeom>, bool), BackendError>;
    /// Characters of one page.
    ///
    /// # Errors
    /// See [`BackendError`].
    fn chars(&self, file: &Path, index: u32) -> Result<Vec<RawChar>, BackendError>;
    /// Render one page at `width_px` pixels wide.
    ///
    /// # Errors
    /// See [`BackendError`].
    fn render(&self, file: &Path, index: u32, width_px: u32) -> Result<Bitmap, BackendError>;
    /// The bookmarks.
    ///
    /// # Errors
    /// See [`BackendError`].
    fn outline(&self, file: &Path) -> Result<Vec<RawBookmark>, BackendError>;
    /// Link annotations of one page.
    ///
    /// # Errors
    /// See [`BackendError`].
    fn links(&self, file: &Path, index: u32) -> Result<Vec<RawLink>, BackendError>;
}

/// Pair UTF-16 surrogates (spike S2: math alphanumerics such as 𝐻 arrive as two chars), keep
/// the first unit's box, and map `PDFium`'s `\x02` soft-hyphen marker to U+00AD.
#[must_use]
pub fn decode_chars(raw: &[RawChar]) -> Vec<(char, Option<[f32; 4]>)> {
    let mut out = Vec::with_capacity(raw.len());
    let mut high: Option<(u32, Option<[f32; 4]>)> = None;
    for rc in raw {
        let v = rc.unit;
        let decoded = match (high.take(), v) {
            (None, 0xD800..=0xDBFF) => {
                high = Some((v, rc.bounds));
                continue;
            }
            (Some((h, b)), 0xDC00..=0xDFFF) => {
                char::from_u32(0x10000 + ((h - 0xD800) << 10) + (v - 0xDC00))
                    .map(|c| (c, b.or(rc.bounds)))
            }
            (_, 2) => Some(('\u{AD}', rc.bounds)),
            _ => char::from_u32(v).map(|c| (c, rc.bounds)),
        };
        if let Some(d) = decoded {
            out.push(d);
        }
    }
    out
}

fn is_unknown(c: char) -> bool {
    matches!(c, '\u{FFFD}' | '\u{E000}'..='\u{F8FF}')
}

fn union(a: Option<[f32; 4]>, b: [f32; 4]) -> [f32; 4] {
    match a {
        None => b,
        Some(a) => [
            a[0].min(b[0]),
            a[1].min(b[1]),
            a[2].max(b[2]),
            a[3].max(b[3]),
        ],
    }
}

fn span(page: u32, start: usize, end: usize, r: [f32; 4]) -> TextSpan {
    TextSpan {
        start: u32::try_from(start).unwrap_or(u32::MAX),
        end: u32::try_from(end).unwrap_or(u32::MAX),
        rect: PageRect {
            page,
            x0: r[0],
            y0: r[1],
            x1: r[2],
            y1: r[3],
        },
    }
}

/// Build the page's text (layout level 0): characters in page space, words split at spaces and
/// line ends, lines split at line ends or where the baseline jumps.
#[must_use]
pub fn page_text(page: u32, geom: &PageGeom, raw: &[RawChar]) -> PageText {
    let chars = decode_chars(raw);
    let mut text = String::with_capacity(chars.len());
    let mut boxes = Vec::with_capacity(chars.len());
    let mut unknown = 0u32;
    for (c, b) in &chars {
        // PDFium writes line ends as "\r\n"; keep one '\n'.
        if *c == '\r' {
            continue;
        }
        if is_unknown(*c) {
            unknown += 1;
        }
        text.push(*c);
        boxes.push(b.map(|r| {
            let pr = geom.rect(page, r);
            [pr.x0, pr.y0, pr.x1, pr.y1]
        }));
    }
    let chars: Vec<char> = text.chars().collect();
    let (mut words, mut lines) = (Vec::new(), Vec::new());
    let (mut w_start, mut w_box): (Option<usize>, Option<[f32; 4]>) = (None, None);
    let (mut l_start, mut l_box): (Option<usize>, Option<[f32; 4]>) = (None, None);
    for (i, c) in chars.iter().enumerate() {
        let b = boxes[i];
        let breaks_line = *c == '\n'
            || matches!((l_box, b), (Some(lb), Some(cb)) if cb[1] > lb[3] || cb[3] < lb[1]);
        if breaks_line {
            if let (Some(s), Some(r)) = (w_start.take(), w_box.take()) {
                words.push(span(page, s, i, r));
            }
            if let (Some(s), Some(r)) = (l_start.take(), l_box.take()) {
                lines.push(span(page, s, i, r));
            }
        }
        if c.is_whitespace() {
            if let (Some(s), Some(r)) = (w_start.take(), w_box.take()) {
                words.push(span(page, s, i, r));
            }
            continue;
        }
        if let Some(r) = b {
            w_start.get_or_insert(i);
            w_box = Some(union(w_box, r));
            l_start.get_or_insert(i);
            l_box = Some(union(l_box, r));
        }
    }
    let n = chars.len();
    if let (Some(s), Some(r)) = (w_start, w_box) {
        words.push(span(page, s, n, r));
    }
    if let (Some(s), Some(r)) = (l_start, l_box) {
        lines.push(span(page, s, n, r));
    }
    PageText {
        page,
        text,
        boxes,
        words,
        lines,
        unknown,
    }
}

/// A raw destination as a contract target (page numbers 1-based, y in page space).
#[must_use]
pub fn target(dest: &RawDest, geoms: &[PageGeom]) -> LinkTarget {
    match dest {
        RawDest::Page { index, y } => match geoms.get(*index as usize) {
            Some(g) => LinkTarget::Page {
                page: index + 1,
                top: y.map(|y| g.to_page(g.crop[0], y).1.max(0.0)),
            },
            None => LinkTarget::Unsupported,
        },
        RawDest::Uri(u) => {
            let lower = u.trim().to_ascii_lowercase();
            if lower.starts_with("https://")
                || lower.starts_with("http://")
                || lower.starts_with("mailto:")
            {
                LinkTarget::Url {
                    url: u.trim().to_string(),
                }
            } else {
                LinkTarget::Unsupported
            }
        }
        RawDest::Other => LinkTarget::Unsupported,
    }
}

/// Bookmarks as outline items.
#[must_use]
pub fn outline(raw: &[RawBookmark], geoms: &[PageGeom]) -> Vec<OutlineItem> {
    raw.iter()
        .map(|b| OutlineItem {
            title: b.title.split_whitespace().collect::<Vec<_>>().join(" "),
            target: b.dest.as_ref().map(|d| target(d, geoms)),
            children: outline(&b.children, geoms),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    const A4: PageGeom = PageGeom {
        rotation: 0,
        crop: [0.0, 0.0, 595.0, 842.0],
    };

    #[test]
    fn rotation_maps_corners() {
        // Unrotated: user (0, 842) is the top-left.
        assert_eq!(A4.to_page(0.0, 842.0), (0.0, 0.0));
        let r90 = PageGeom { rotation: 90, ..A4 };
        assert_eq!(
            r90.size(),
            PageSize {
                width: 842.0,
                height: 595.0
            }
        );
        assert_eq!(r90.to_page(0.0, 842.0), (842.0, 0.0)); // old top-left -> top-right
        assert_eq!(r90.to_page(0.0, 0.0), (0.0, 0.0));
        let r180 = PageGeom {
            rotation: 180,
            ..A4
        };
        assert_eq!(r180.to_page(0.0, 0.0), (595.0, 0.0));
        let r270 = PageGeom {
            rotation: 270,
            ..A4
        };
        assert_eq!(r270.to_page(0.0, 0.0), (842.0, 595.0));
    }

    proptest! {
        #[test]
        fn to_user_inverts_to_page(rot in prop::sample::select(vec![0u16, 90, 180, 270]), x0 in -50.0f32..50.0, y0 in -50.0f32..50.0,
                                   w in 100.0f32..1000.0, h in 100.0f32..1000.0, fx in 0.0f32..1.0, fy in 0.0f32..1.0) {
            let g = PageGeom { rotation: rot, crop: [x0, y0, x0 + w, y0 + h] };
            let (x, y) = (x0 + fx * w, y0 + fy * h);
            let (px, py) = g.to_page(x, y);
            let s = g.size();
            prop_assert!(px >= -0.01 && py >= -0.01 && px <= s.width + 0.01 && py <= s.height + 0.01);
            let (ux, uy) = g.to_user(px, py);
            prop_assert!((ux - x).abs() < 0.01 && (uy - y).abs() < 0.01);
        }
    }

    fn rc(c: char, x: f32, baseline: f32) -> RawChar {
        RawChar {
            unit: c as u32,
            bounds: Some([x, baseline, x + 5.0, baseline + 8.0]),
        }
    }

    #[test]
    fn surrogates_soft_hyphen_words_lines() {
        // "𝐻 is" then a line break, "propa\x02" + "\r\n" + "gating"
        let mut raw = vec![
            RawChar {
                unit: 0xD835,
                bounds: Some([10.0, 800.0, 16.0, 808.0]),
            },
            RawChar {
                unit: 0xDC3B,
                bounds: None,
            },
            RawChar {
                unit: ' ' as u32,
                bounds: None,
            },
            rc('i', 20.0, 800.0),
            rc('s', 25.0, 800.0),
            RawChar {
                unit: '\r' as u32,
                bounds: None,
            },
            RawChar {
                unit: '\n' as u32,
                bounds: None,
            },
        ];
        let mut x = 10.0;
        for c in "propa".chars() {
            raw.push(rc(c, x, 780.0));
            x += 5.0;
        }
        raw.push(RawChar {
            unit: 2,
            bounds: Some([35.0, 780.0, 38.0, 788.0]),
        });
        raw.push(RawChar {
            unit: '\r' as u32,
            bounds: None,
        });
        raw.push(RawChar {
            unit: '\n' as u32,
            bounds: None,
        });
        let mut x = 10.0;
        for c in "gating".chars() {
            raw.push(rc(c, x, 760.0));
            x += 5.0;
        }
        let t = page_text(1, &A4, &raw);
        assert_eq!(t.text, "𝐻 is\npropa\u{AD}\ngating");
        assert_eq!(t.boxes.len(), t.text.chars().count());
        let words: Vec<String> = t
            .words
            .iter()
            .map(|w| {
                t.text
                    .chars()
                    .skip(w.start as usize)
                    .take((w.end - w.start) as usize)
                    .collect()
            })
            .collect();
        assert_eq!(words, ["𝐻", "is", "propa\u{AD}", "gating"]);
        assert_eq!(t.lines.len(), 3);
        // Page space: y grows downward; the first line is above the second.
        assert!(t.lines[0].rect.y1 <= t.lines[1].rect.y0 + 0.01);
        assert_eq!(t.unknown, 0);
    }

    #[test]
    fn unknown_glyphs_are_counted_and_links_are_filtered() {
        let t = page_text(
            1,
            &A4,
            &[rc('\u{FFFD}', 1.0, 1.0), rc('\u{E001}', 7.0, 1.0)],
        );
        assert_eq!(t.unknown, 2);
        let geoms = [A4, A4];
        assert_eq!(
            target(
                &RawDest::Page {
                    index: 1,
                    y: Some(842.0)
                },
                &geoms
            ),
            LinkTarget::Page {
                page: 2,
                top: Some(0.0)
            }
        );
        assert_eq!(
            target(&RawDest::Page { index: 5, y: None }, &geoms),
            LinkTarget::Unsupported
        );
        assert_eq!(
            target(&RawDest::Uri("javascript:alert(1)".into()), &geoms),
            LinkTarget::Unsupported
        );
        assert_eq!(
            target(&RawDest::Uri(" https://doi.org/10.1/x ".into()), &geoms),
            LinkTarget::Url {
                url: "https://doi.org/10.1/x".into()
            }
        );
    }
}
