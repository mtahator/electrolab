//! Document intelligence, pure logic (P1.M4): lines from page text, section headings, mentions,
//! citation markers, the reference list, tables from words, and objects from layout detections.
//! Deterministic first — the PDF's own text and printed labels — models only fill gaps.

pub mod headings;
pub mod layout;
pub mod mentions;
pub mod references;
pub mod tables;

use ep_document_contract::PageText;
use ep_types::{PageQuad, PageRect};

/// One visual line of a page.
#[derive(Clone, Debug, PartialEq)]
pub struct Line {
    /// 1-based page.
    pub page: u32,
    /// Text, trimmed, soft hyphens kept.
    pub text: String,
    /// Box.
    pub rect: PageRect,
    /// First character index in the page text.
    pub start: u32,
    /// One past the last character.
    pub end: u32,
}

impl Line {
    /// Line height in points.
    #[must_use]
    pub fn height(&self) -> f32 {
        self.rect.y1 - self.rect.y0
    }
}

/// The lines of a page, in the order the text gives them.
#[must_use]
pub fn lines(pt: &PageText) -> Vec<Line> {
    let chars: Vec<char> = pt.text.chars().collect();
    pt.lines
        .iter()
        .filter_map(|l| {
            let s = l.start as usize;
            let e = (l.end as usize).min(chars.len());
            if s >= e {
                return None;
            }
            let text: String = chars[s..e].iter().collect::<String>().trim().to_string();
            (!text.is_empty()).then_some(Line {
                page: pt.page,
                text,
                rect: l.rect,
                start: l.start,
                end: l.end,
            })
        })
        .collect()
}

/// Median of a list (0 when empty).
#[must_use]
pub fn median(mut v: Vec<f32>) -> f32 {
    if v.is_empty() {
        return 0.0;
    }
    v.sort_by(f32::total_cmp);
    v[v.len() / 2]
}

/// The usual line height of body text across pages.
#[must_use]
pub fn body_height(pages: &[Vec<Line>]) -> f32 {
    median(
        pages
            .iter()
            .flatten()
            .filter(|l| l.text.chars().count() > 30)
            .map(Line::height)
            .collect(),
    )
}

/// Characters `start..end` of a page as one quad per line piece, from the character boxes.
#[must_use]
#[allow(clippy::many_single_char_names)] // box corners
pub fn quads(pt: &PageText, start: u32, end: u32) -> Vec<PageQuad> {
    let chars: Vec<char> = pt.text.chars().collect();
    let mut out = Vec::new();
    for line in &pt.lines {
        let s = start.max(line.start) as usize;
        let e = end.min(line.end) as usize;
        if s >= e {
            continue;
        }
        let mut r: Option<[f32; 4]> = None;
        for i in s..e.min(pt.boxes.len()) {
            let Some(b) = pt.boxes[i] else { continue };
            if chars.get(i).is_some_and(|c| c.is_whitespace()) {
                continue;
            }
            r = Some(match r {
                None => b,
                Some(o) => [
                    o[0].min(b[0]),
                    o[1].min(b[1]),
                    o[2].max(b[2]),
                    o[3].max(b[3]),
                ],
            });
        }
        if let Some([x0, y0, x1, y1]) = r {
            out.push(PageQuad::from_rect(&PageRect {
                page: pt.page,
                x0,
                y0,
                x1,
                y1,
            }));
        }
    }
    out
}

/// Text of characters whose box centre lies inside `rect`, in text order, lines joined by spaces.
#[must_use]
pub fn text_in(pt: &PageText, rect: &PageRect) -> String {
    let mut out = String::new();
    let mut last_line: Option<usize> = None;
    let chars: Vec<char> = pt.text.chars().collect();
    for (li, line) in pt.lines.iter().enumerate() {
        let (s, e) = (line.start as usize, (line.end as usize).min(chars.len()));
        for (i, &ch) in chars.iter().enumerate().take(e).skip(s) {
            let Some(b) = pt.boxes.get(i).copied().flatten() else {
                continue;
            };
            let (cx, cy) = (f32::midpoint(b[0], b[2]), f32::midpoint(b[1], b[3]));
            if cx < rect.x0 || cx > rect.x1 || cy < rect.y0 || cy > rect.y1 {
                continue;
            }
            if last_line.is_some_and(|l| l != li) && !out.ends_with(' ') {
                if out.ends_with('\u{ad}') {
                    out.pop();
                } else {
                    out.push(' ');
                }
            }
            last_line = Some(li);
            out.push(ch);
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// ASCII digits for Persian and Arabic-Indic digits.
#[must_use]
pub fn ascii_digits(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            '\u{6f0}'..='\u{6f9}' => char::from_digit(c as u32 - 0x6f0, 10).unwrap_or(c),
            '\u{660}'..='\u{669}' => char::from_digit(c as u32 - 0x660, 10).unwrap_or(c),
            _ => c,
        })
        .collect()
}

/// Intersection over union of two rectangles.
#[must_use]
pub fn iou(a: &PageRect, b: &PageRect) -> f32 {
    let ix = (a.x1.min(b.x1) - a.x0.max(b.x0)).max(0.0);
    let iy = (a.y1.min(b.y1) - a.y0.max(b.y0)).max(0.0);
    let inter = ix * iy;
    let union = a.width() * a.height() + b.width() * b.height() - inter;
    if union <= 0.0 { 0.0 } else { inter / union }
}

/// Overlap of two intervals.
#[must_use]
pub fn overlap(a0: f32, a1: f32, b0: f32, b1: f32) -> f32 {
    (a1.min(b1) - a0.max(b0)).max(0.0)
}

/// One raw detection from a layout model: class index, score, box as fractions of the image
/// (`x0, y0, x1, y1` in 0–1, origin top-left).
pub type RawDetection = (usize, f32, [f32; 4]);

/// Port: a page layout model (ADR-007). The adapter owns the model session and its threads.
pub trait LayoutModel: Send + Sync {
    /// Detections on one rendered page.
    ///
    /// # Errors
    /// The model failed to run.
    fn detect(&self, page: &crate::pdf::Bitmap) -> Result<Vec<RawDetection>, String>;
}

/// Raw detections in page space for a page of `width` × `height` points.
#[must_use]
pub fn to_page(page: u32, width: f32, height: f32, raw: &[RawDetection]) -> Vec<layout::Detection> {
    raw.iter()
        .map(|&(class, score, [x0, y0, x1, y1])| layout::Detection {
            class,
            score,
            rect: PageRect {
                page,
                x0: (x0 * width).clamp(0.0, width),
                y0: (y0 * height).clamp(0.0, height),
                x1: (x1 * width).clamp(0.0, width),
                y1: (y1 * height).clamp(0.0, height),
            },
        })
        .collect()
}

#[cfg(test)]
pub(crate) mod testing {
    use ep_document_contract::{PageText, TextSpan};
    use ep_types::PageRect;

    /// A page from lines of text: each line at `(x, y)`, 6 pt per character, `h` tall.
    pub fn page(page: u32, lines: &[(&str, f32, f32, f32)]) -> PageText {
        let mut text = String::new();
        let mut boxes = Vec::new();
        let mut spans = Vec::new();
        let mut words = Vec::new();
        for (t, x, y, h) in lines {
            let start = u32::try_from(text.chars().count()).unwrap_or(0);
            let mut wstart: Option<u32> = None;
            for (i, c) in t.chars().enumerate() {
                let idx = start + u32::try_from(i).unwrap_or(0);
                #[allow(clippy::cast_precision_loss)]
                let x0 = x + 6.0 * i as f32;
                boxes.push(Some([x0, *y, x0 + 5.0, y + h]));
                text.push(c);
                if c == ' ' {
                    if let Some(ws) = wstart.take() {
                        words.push(span(page, ws, idx, &boxes));
                    }
                } else if wstart.is_none() {
                    wstart = Some(idx);
                }
            }
            let end = u32::try_from(text.chars().count()).unwrap_or(0);
            if let Some(ws) = wstart {
                words.push(span(page, ws, end, &boxes));
            }
            #[allow(clippy::cast_precision_loss)]
            let x1 = x + 6.0 * t.chars().count() as f32;
            spans.push(TextSpan {
                start,
                end,
                rect: PageRect {
                    page,
                    x0: *x,
                    y0: *y,
                    x1,
                    y1: y + h,
                },
            });
            text.push('\n');
            boxes.push(None);
        }
        PageText {
            page,
            text,
            boxes,
            words,
            lines: spans,
            unknown: 0,
        }
    }

    fn span(page: u32, s: u32, e: u32, boxes: &[Option<[f32; 4]>]) -> TextSpan {
        let bs: Vec<[f32; 4]> = boxes[s as usize..e as usize]
            .iter()
            .flatten()
            .copied()
            .collect();
        let x0 = bs.iter().map(|b| b[0]).fold(f32::INFINITY, f32::min);
        let y0 = bs.iter().map(|b| b[1]).fold(f32::INFINITY, f32::min);
        let x1 = bs.iter().map(|b| b[2]).fold(f32::NEG_INFINITY, f32::max);
        let y1 = bs.iter().map(|b| b[3]).fold(f32::NEG_INFINITY, f32::max);
        TextSpan {
            start: s,
            end: e,
            rect: PageRect {
                page,
                x0,
                y0,
                x1,
                y1,
            },
        }
    }
}
