//! Scanned pages (P1.M4.OCR.1–2): the OCR port, OCR words turned into page text (so search,
//! selection and copy work like on any page), and the invisible text layer written into a PDF.

use std::fmt::Write as _;

use ep_document_contract::{PageText, TextSpan};
use ep_types::PageRect;

use crate::pdf::{Bitmap, PageGeom};

/// One recognised word: text and box as fractions of the image (`x0, y0, x1, y1`, top-left).
#[derive(Clone, Debug, PartialEq)]
pub struct OcrWord {
    /// Text.
    pub text: String,
    /// Box as fractions of the image.
    pub rect: [f32; 4],
}

/// One recognised line, words in reading order.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct OcrLine {
    /// Words.
    pub words: Vec<OcrWord>,
}

/// Port: an OCR engine (ADR-013: PP-OCRv5 on ONNX Runtime, the same on every system).
pub trait OcrEngine: Send + Sync {
    /// Short id, part of cache keys ("ppocr").
    fn id(&self) -> &'static str;
    /// Recognise the text of a rendered page.
    ///
    /// # Errors
    /// The engine failed.
    fn recognize(&self, page: &Bitmap) -> Result<Vec<OcrLine>, String>;
}

/// OCR lines as page text for a page of `width` × `height` points: characters with boxes (each
/// word's box shared evenly by its letters), words, lines, `\n` between lines.
#[must_use]
#[allow(clippy::cast_precision_loss)] // letters per word
pub fn page_text(page: u32, width: f32, height: f32, lines: &[OcrLine]) -> PageText {
    let mut text = String::new();
    let mut boxes = Vec::new();
    let mut words = Vec::new();
    let mut spans = Vec::new();
    let mut n: u32 = 0;
    let rect = |r: [f32; 4]| PageRect {
        page,
        x0: r[0] * width,
        y0: r[1] * height,
        x1: r[2] * width,
        y1: r[3] * height,
    };
    // Lines in reading order: down one column, then the next.
    let lines: Vec<&OcrLine> = lines.iter().filter(|l| !l.words.is_empty()).collect();
    let line_boxes: Vec<PageRect> = lines
        .iter()
        .map(|l| {
            l.words
                .iter()
                .map(|w| rect(w.rect))
                .reduce(|a, b| a.union(&b))
                .unwrap_or(PageRect {
                    page,
                    x0: 0.0,
                    y0: 0.0,
                    x1: 0.0,
                    y1: 0.0,
                })
        })
        .collect();
    let ordered: Vec<&OcrLine> = super::layout::reading_order(&line_boxes)
        .into_iter()
        .map(|i| lines[i])
        .collect();
    for line in ordered {
        let line_start = n;
        let mut line_rect: Option<PageRect> = None;
        for (wi, w) in line.words.iter().enumerate() {
            if wi > 0 {
                text.push(' ');
                boxes.push(None);
                n += 1;
            }
            let r = rect(w.rect);
            let chars: Vec<char> = w.text.chars().collect();
            let k = chars.len().max(1) as f32;
            let step = (r.x1 - r.x0) / k;
            let start = n;
            for (ci, c) in chars.iter().enumerate() {
                let x0 = r.x0 + step * ci as f32;
                text.push(*c);
                boxes.push(Some([x0, r.y0, x0 + step, r.y1]));
                n += 1;
            }
            words.push(TextSpan {
                start,
                end: n,
                rect: r,
            });
            line_rect = Some(line_rect.map_or(r, |l| l.union(&r)));
        }
        if let Some(r) = line_rect {
            spans.push(TextSpan {
                start: line_start,
                end: n,
                rect: r,
            });
        }
        text.push('\n');
        boxes.push(None);
        n += 1;
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

/// Helvetica glyph widths (per 1000 em) for ASCII 32–126 (as in the annotation writer).
const HELV: [u16; 95] = [
    278, 278, 355, 556, 556, 889, 667, 191, 333, 333, 389, 584, 278, 333, 278, 278, 556, 556, 556,
    556, 556, 556, 556, 556, 556, 556, 278, 278, 584, 584, 584, 556, 1015, 667, 667, 722, 722, 667,
    611, 778, 722, 278, 500, 667, 556, 833, 722, 778, 667, 778, 722, 667, 611, 722, 667, 944, 667,
    667, 611, 278, 278, 278, 469, 556, 333, 556, 556, 500, 556, 556, 278, 556, 556, 222, 222, 500,
    222, 833, 556, 556, 556, 556, 333, 500, 278, 556, 500, 722, 500, 500, 500, 334, 260, 334, 584,
];

/// Latin-1 text for a `WinAnsiEncoding` string; other characters become '?'.
fn latin1(s: &str) -> Vec<u8> {
    s.chars()
        .map(|c| {
            if (' '..='~').contains(&c) || ('\u{a0}'..='\u{ff}').contains(&c) {
                c as u8
            } else {
                b'?'
            }
        })
        .collect()
}

fn escape(bytes: &[u8]) -> String {
    let mut out = String::from("(");
    for &b in bytes {
        match b {
            b'(' | b')' | b'\\' => {
                out.push('\\');
                out.push(char::from(b));
            }
            0x20..=0x7e => out.push(char::from(b)),
            _ => {
                let _ = write!(out, "\\{b:03o}");
            }
        }
    }
    out.push(')');
    out
}

fn f(v: f32) -> String {
    let r = (v * 100.0).round() / 100.0;
    if r == 0.0 { "0".into() } else { format!("{r}") }
}

/// Content stream drawing every word invisibly (`3 Tr`) in `/<font>` (Helvetica, `WinAnsi`),
/// stretched to its box, so the page becomes searchable and selectable in any PDF reader.
#[must_use]
#[allow(clippy::many_single_char_names)] // text matrix a b c d, position x y
pub fn text_layer(geom: &PageGeom, pt: &PageText, font: &str) -> String {
    let chars: Vec<char> = pt.text.chars().collect();
    let mut out = String::from("q BT 3 Tr ");
    let (a, b, c, d) = match geom.rotation {
        90 => (0.0, 1.0, -1.0, 0.0),
        180 => (-1.0, 0.0, 0.0, -1.0),
        270 => (0.0, -1.0, 1.0, 0.0),
        _ => (1.0, 0.0, 0.0, 1.0),
    };
    for w in &pt.words {
        let s: String = chars[w.start as usize..(w.end as usize).min(chars.len())]
            .iter()
            .collect();
        let bytes = latin1(&s);
        if bytes.is_empty() {
            continue;
        }
        let r = &w.rect;
        let size = (r.y1 - r.y0).max(1.0) * 0.9;
        let natural: f32 = bytes
            .iter()
            .map(|&ch| f32::from(*HELV.get(usize::from(ch).wrapping_sub(32)).unwrap_or(&556)))
            .sum::<f32>()
            * size
            / 1000.0;
        let scale = if natural > 0.0 {
            (r.x1 - r.x0) / natural * 100.0
        } else {
            100.0
        };
        // Baseline near the bottom of the box, in user space.
        let (x, y) = geom.to_user(r.x0, r.y1 - (r.y1 - r.y0) * 0.18);
        let _ = write!(
            out,
            "/{font} {} Tf {} Tz {} {} {} {} {} {} Tm {} Tj ",
            f(size),
            f(scale.clamp(1.0, 1000.0)),
            f(a),
            f(b),
            f(c),
            f(d),
            f(x),
            f(y),
            escape(&bytes)
        );
    }
    out.push_str("ET Q");
    out
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn lines() -> Vec<OcrLine> {
        vec![
            OcrLine {
                words: vec![
                    OcrWord {
                        text: "Thermal".into(),
                        rect: [0.1, 0.1, 0.3, 0.12],
                    },
                    OcrWord {
                        text: "model".into(),
                        rect: [0.32, 0.1, 0.45, 0.12],
                    },
                ],
            },
            OcrLine {
                words: vec![OcrWord {
                    text: "(x)".into(),
                    rect: [0.1, 0.2, 0.15, 0.22],
                }],
            },
        ]
    }

    #[test]
    fn ocr_words_become_page_text() {
        let pt = page_text(2, 600.0, 800.0, &lines());
        assert_eq!(pt.text, "Thermal model\n(x)\n");
        assert_eq!(pt.words.len(), 3);
        assert_eq!(pt.lines.len(), 2);
        assert_eq!(pt.boxes.len(), pt.text.chars().count());
        let b = pt.boxes[0].unwrap();
        assert!((b[0] - 60.0).abs() < 0.01 && (b[1] - 80.0).abs() < 0.01);
        assert!((pt.words[1].rect.x1 - 270.0).abs() < 0.01);
    }

    #[test]
    fn invisible_text_layer() {
        let pt = page_text(1, 600.0, 800.0, &lines());
        let geom = PageGeom {
            rotation: 0,
            crop: [0.0, 0.0, 600.0, 800.0],
        };
        let s = text_layer(&geom, &pt, "EPocr");
        assert!(s.starts_with("q BT 3 Tr /EPocr "));
        assert!(s.contains("(Thermal) Tj") && s.contains("(\\(x\\)) Tj"));
        assert!(s.ends_with("ET Q"));
    }
}
