//! Objects of a page (P1.M4.OBJ.1–2) from layout detections and the page text: duplicates
//! removed, captions paired with their figure or table ("Figure 3" → label, number, caption
//! text), equation numbers read from the text beside each formula, headings numbered, and
//! everything put in reading order by recursive XY-cut (columns before rows).
//!
//! Without detections (no layout model), the text alone still gives captions, headings and
//! numbered equations.

use std::sync::LazyLock;

use ep_document_contract::{DocObject, ObjectKind, PageText};
use ep_types::PageRect;
use regex::Regex;

use super::headings::Heading;
use super::{Line, iou, overlap, text_in};

/// Labels of the Docling layout models (Heron, Egret), in output order.
pub const DOCLING_LABELS: [&str; 17] = [
    "Caption",
    "Footnote",
    "Formula",
    "List-item",
    "Page-footer",
    "Page-header",
    "Picture",
    "Section-header",
    "Table",
    "Text",
    "Title",
    "Document Index",
    "Code",
    "Checkbox-Selected",
    "Checkbox-Unselected",
    "Form",
    "Key-Value Region",
];

/// One detection of the layout model, already in page space.
#[derive(Clone, Debug, PartialEq)]
pub struct Detection {
    /// Index into `DOCLING_LABELS`.
    pub class: usize,
    /// Confidence 0–1.
    pub score: f32,
    /// Box.
    pub rect: PageRect,
}

/// Detections below this confidence are dropped.
pub const MIN_SCORE: f32 = 0.45;

fn kind_of(class: usize) -> Option<ObjectKind> {
    Some(match DOCLING_LABELS.get(class)? {
        &"Caption" => ObjectKind::Caption,
        &"Footnote" => ObjectKind::Footnote,
        &"Formula" => ObjectKind::Equation,
        &"List-item" => ObjectKind::ListItem,
        &"Page-footer" | &"Page-header" => ObjectKind::PageFurniture,
        &"Picture" => ObjectKind::Figure,
        &"Section-header" => ObjectKind::Heading,
        &"Table" => ObjectKind::Table,
        &"Text" | &"Document Index" => ObjectKind::Text,
        &"Title" => ObjectKind::Title,
        &"Code" => ObjectKind::Code,
        _ => return None,
    })
}

/// A caption's label with its full printed number: "Figure 2.3", "Table A.1", "Figure S2",
/// "Fig. 4-2", "Table IV" (review R7), then the rest of the line.
static CAPTION: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"^(Fig\.?|Figure|FIGURE|Table|TABLE|Tab\.|شکل|جدول)\s*((?:[A-Z]|S)?[0-9\u{6f0}-\u{6f9}]+(?:[.\-][0-9\u{6f0}-\u{6f9}]+)*|[A-Z]\.[0-9]+|[IVX]+)\b(.*)$",
    )
    .unwrap_or_else(|_| unreachable!())
});
static EQ_NUMBER: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^\((\d{1,3}(?:\.\d{1,3}){0,2}[a-z]?)\)$").unwrap_or_else(|_| unreachable!())
});

/// A caption label in text: (is a table, "Table 2", "2").
fn caption_label(text: &str) -> Option<(bool, String, String)> {
    let c = CAPTION.captures(text.trim())?;
    // A caption is a label followed by caption text ("Table 1.1 Comparison of …", "Figure 2:
    // …"), not a sentence that starts with a label ("Table 1.1 provides …", "… see" / "Table
    // 1.4."), and not a continuation ("Table 1.7 (Continued)").
    let rest = c[3].trim_start();
    let after_mark = rest
        .strip_prefix([':', '.', '|', '—', '–', '-'])
        .map(str::trim_start);
    let caption = match after_mark {
        Some(t) => t.chars().next().is_some_and(char::is_alphanumeric),
        None => rest.chars().next().is_some_and(|ch| {
            ch.is_uppercase() || ch == '(' && !rest.to_lowercase().starts_with("(cont")
        }),
    };
    if !caption || rest.to_lowercase().starts_with("(cont") {
        return None;
    }
    let word = &c[1];
    let table = word.starts_with('T') || word.starts_with('t') || word == "جدول";
    let num = super::ascii_digits(&c[2]);
    let printed = if table {
        format!("Table {num}")
    } else {
        format!("Figure {num}")
    };
    Some((table, printed, num))
}

/// Remove lower-scored duplicates of the same kind (`IoU` > 0.6).
fn dedupe(mut dets: Vec<Detection>) -> Vec<Detection> {
    dets.sort_by(|a, b| b.score.total_cmp(&a.score));
    let mut out: Vec<Detection> = Vec::new();
    for d in dets {
        if d.score < MIN_SCORE {
            continue;
        }
        if out
            .iter()
            .any(|o| o.class == d.class && iou(&o.rect, &d.rect) > 0.6)
        {
            continue;
        }
        out.push(d);
    }
    out
}

/// Recursive XY-cut: split at the widest vertical gap (columns), else at the widest horizontal
/// gap (rows); returns indexes in reading order.
fn xy_cut(rects: &[PageRect], idx: Vec<usize>, depth: u32) -> Vec<usize> {
    if idx.len() <= 1 || depth > 32 {
        let mut v = idx;
        v.sort_by(|&a, &b| {
            rects[a]
                .y0
                .total_cmp(&rects[b].y0)
                .then(rects[a].x0.total_cmp(&rects[b].x0))
        });
        return v;
    }
    // Find gaps along one axis: intervals covered by no rect.
    let cut = |vertical: bool| -> Option<f32> {
        let mut spans: Vec<(f32, f32)> = idx
            .iter()
            .map(|&i| {
                if vertical {
                    (rects[i].x0, rects[i].x1)
                } else {
                    (rects[i].y0, rects[i].y1)
                }
            })
            .collect();
        spans.sort_by(|a, b| a.0.total_cmp(&b.0));
        let mut end = spans[0].1;
        let mut best: Option<(f32, f32)> = None;
        for (a, b) in spans.into_iter().skip(1) {
            if a > end + 2.0 {
                let width = a - end;
                if best.is_none_or(|(w, _)| width > w) {
                    best = Some((width, f32::midpoint(end, a)));
                }
            }
            end = end.max(b);
        }
        best.map(|(_, at)| at)
    };
    // Columns first: a two-column page reads down one column, then the next. Only where no
    // column gap runs through the region (a full-width title or figure) does a row cut come first.
    let (first, second): (Vec<usize>, Vec<usize>) = if let Some(x) = cut(true) {
        idx.iter()
            .partition(|&&i| f32::midpoint(rects[i].x0, rects[i].x1) < x)
    } else if let Some(y) = cut(false) {
        idx.iter()
            .partition(|&&i| f32::midpoint(rects[i].y0, rects[i].y1) < y)
    } else {
        let mut v = idx;
        v.sort_by(|&a, &b| {
            rects[a]
                .y0
                .total_cmp(&rects[b].y0)
                .then(rects[a].x0.total_cmp(&rects[b].x0))
        });
        return v;
    };
    if first.is_empty() || second.is_empty() {
        let mut v: Vec<usize> = first.into_iter().chain(second).collect();
        v.sort_by(|&a, &b| rects[a].y0.total_cmp(&rects[b].y0));
        return v;
    }
    let mut out = xy_cut(rects, first, depth + 1);
    out.extend(xy_cut(rects, second, depth + 1));
    out
}

/// Without the layout model: the area of the figure or table a caption belongs to. A figure is
/// the space above its caption up to the nearest line of running text (or the top of the text
/// area); a table is the rows below its caption up to a clear gap. The width is the column's.
/// `None` when there is no such space (the caption stands alone).
#[must_use]
pub fn caption_area(lines: &[Line], cap: &PageRect, table: bool) -> Option<PageRect> {
    let body: Vec<&Line> = lines
        .iter()
        .filter(|l| {
            l.text.chars().count() > 50 && overlap(l.rect.x0, l.rect.x1, cap.x0, cap.x1) > 0.0
        })
        .collect();
    let (left, right) = if body.is_empty() {
        (cap.x0, cap.x1)
    } else {
        (
            body.iter().map(|l| l.rect.x0).fold(cap.x0, f32::min),
            body.iter().map(|l| l.rect.x1).fold(cap.x1, f32::max),
        )
    };
    let text_top = lines.iter().map(|l| l.rect.y0).fold(f32::MAX, f32::min);
    let h = cap.height().max(6.0);
    // Running text: a long line spanning most of the column.
    let running = |l: &Line| l.text.chars().count() > 50 && l.rect.width() > 0.8 * (right - left);
    let area = if table {
        // Rows below the caption until a gap of more than two lines or running text.
        let mut bottom = cap.y1;
        let mut below: Vec<&Line> = lines
            .iter()
            .filter(|l| {
                l.rect.y0 >= cap.y1 - 1.0 && overlap(l.rect.x0, l.rect.x1, left, right) > 0.0
            })
            .collect();
        below.sort_by(|a, b| a.rect.y0.total_cmp(&b.rect.y0));
        for l in below {
            if l.rect.y0 - bottom > 2.5 * h || (running(l) && l.rect.y0 - bottom > 0.8 * h) {
                break;
            }
            bottom = bottom.max(l.rect.y1);
        }
        PageRect {
            page: cap.page,
            x0: left,
            y0: cap.y1 + 1.0,
            x1: right,
            y1: bottom,
        }
    } else {
        // Up to the nearest running text above, or the top of the text area.
        let top = lines
            .iter()
            .filter(|l| {
                l.rect.y1 <= cap.y0 - 1.0
                    && overlap(l.rect.x0, l.rect.x1, left, right) > 0.0
                    && running(l)
            })
            .map(|l| l.rect.y1)
            .fold(text_top, f32::max);
        PageRect {
            page: cap.page,
            x0: left,
            y0: top + 2.0,
            x1: right,
            y1: cap.y0 - 1.0,
        }
    };
    (area.height() > 2.0 * h).then_some(area)
}

/// Math-font text made safe to show or search: private-use glyphs (big brackets) and control
/// characters dropped, spaces collapsed; empty when no letter or digit is left.
#[must_use]
pub fn clean_math_text(t: &str) -> String {
    let kept: String = t
        .chars()
        .map(|c| {
            if c.is_control() || ('\u{e000}'..='\u{f8ff}').contains(&c) {
                ' '
            } else {
                c
            }
        })
        .collect();
    let out = kept.split_whitespace().collect::<Vec<_>>().join(" ");
    if out.chars().any(char::is_alphanumeric) {
        out
    } else {
        String::new()
    }
}

static PROSE_REF: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\b(see|in|from|of|by|and|eqs?\.?|equations?|using|with)\s*\(\d")
        .unwrap_or_else(|_| unreachable!())
});

/// Whether text reads like a sentence (prose that merely ends in "(3)").
fn prose(t: &str) -> bool {
    let words = t
        .split_whitespace()
        .filter(|w| {
            w.chars().count() >= 3
                && w.chars()
                    .all(|c| c.is_alphabetic() || matches!(c, ',' | '.' | ';' | '-'))
        })
        .count();
    words >= 4 || PROSE_REF.is_match(t)
}

/// Displayed, numbered formulas found from the text alone (no layout model): a line ending in a
/// printed number "(n)" whose formula is set apart from the column's text (indented), together
/// with the displayed lines right above it. Prose ending in "(3)" is not an equation, and a
/// lone number takes the formula lines above it (or is no equation).
#[must_use]
pub fn equation_blocks(lines: &[Line]) -> Vec<PageRect> {
    // The column a box sits in: the span of the long (body text) lines that overlap it.
    let column = |r: &PageRect| -> (f32, f32) {
        let body: Vec<&Line> = lines
            .iter()
            .filter(|b| {
                b.text.chars().count() > 40 && overlap(b.rect.x0, b.rect.x1, r.x0, r.x1) > 0.0
            })
            .collect();
        if body.is_empty() {
            return (r.x0 - 13.0, r.x1 + 5.0);
        }
        let left = body.iter().map(|b| b.rect.x0).fold(f32::MAX, f32::min);
        let right = body.iter().map(|b| b.rect.x1).fold(f32::MIN, f32::max);
        (left, right)
    };
    let indented = |l: &Line| l.rect.x0 > column(&l.rect).0 + 12.0;
    // A row that starts at the column's left edge is running text (PDFium splits a sentence at
    // its subscripts, so a piece of it can look indented on its own).
    let text_row = |l: &Line| {
        let (left, _) = column(&l.rect);
        lines.iter().any(|o| {
            o.page == l.page
                && o.rect.x0 < left + 12.0
                && overlap(o.rect.y0, o.rect.y1, l.rect.y0, l.rect.y1)
                    > 0.5 * l.height().min(o.height())
                && overlap(o.rect.x0, o.rect.x1, left, column(&l.rect).1) > 0.0
        })
    };
    let displayed = |l: &Line| {
        let (left, right) = column(&l.rect);
        l.rect.x0 > left + 12.0 && l.rect.x1 < right - 4.0 && !prose(&l.text) && !text_row(l)
    };
    let mut out: Vec<PageRect> = Vec::new();
    for l in lines {
        let mut words: Vec<&str> = l.text.split_whitespace().collect();
        let Some(last) = words.pop() else { continue };
        if !EQ_NUMBER.is_match(last) {
            continue;
        }
        let body = words.join(" ");
        let has_body = body.chars().any(char::is_alphanumeric);
        if has_body && (prose(&body) || !indented(l)) {
            continue;
        }
        // Pieces of the same displayed formula: on the same row (PDFium splits a formula's row
        // at its gaps) or just above, close together, displayed, and not another numbered line.
        let mut rect = l.rect;
        let mut taken = vec![false; lines.len()];
        loop {
            let mut grew = false;
            for (k, a) in lines.iter().enumerate() {
                if taken[k] || a.page != l.page || std::ptr::eq(a, l) {
                    continue;
                }
                let h = l.height().max(a.height());
                // Close to the block on any side: rows of a formula sit above, beside and below
                // its number.
                let vgap = (a.rect.y0 - rect.y1).max(rect.y0 - a.rect.y1);
                let near = vgap < 1.6 * h
                    && overlap(a.rect.x0, a.rect.x1, rect.x0 - 60.0, rect.x1 + 60.0) > 0.0;
                let same_column = (column(&a.rect).0 - column(&l.rect).0).abs() < 20.0;
                if near
                    && same_column
                    && !EQ_NUMBER.is_match(a.text.split_whitespace().last().unwrap_or(""))
                    && displayed(a)
                    && rect.union(&a.rect).height() < 12.0 * h
                {
                    rect = rect.union(&a.rect);
                    taken[k] = true;
                    grew = true;
                }
            }
            if !grew {
                break;
            }
        }
        // A lone number with no formula above it is not an equation.
        if !has_body && rect == l.rect {
            continue;
        }
        if out.iter().any(|o| {
            overlap(o.y0, o.y1, rect.y0, rect.y1) > 0.0
                && overlap(o.x0, o.x1, rect.x0, rect.x1) > 0.0
        }) {
            continue;
        }
        out.push(rect);
    }
    out
}

/// Equation number printed beside a formula: a line "(7)" in the same band, right of or at the
/// end of the formula.
fn equation_number(lines: &[Line], r: &PageRect) -> Option<(String, PageRect)> {
    lines
        .iter()
        .filter(|l| {
            let band = overlap(l.rect.y0, l.rect.y1, r.y0 - 2.0, r.y1 + 2.0);
            band > 0.5 * l.height() && l.rect.x1 >= f32::midpoint(r.x0, r.x1)
        })
        .find_map(|l| {
            // The number may be the line's last token ("x = y (7)").
            let last = l.text.split_whitespace().last()?;
            EQ_NUMBER.captures(last).map(|c| (c[1].to_string(), l.rect))
        })
}

#[allow(clippy::too_many_lines)] // one pass per rule, in order: detect, text, captions, numbers, order
/// Objects of one page. `detections` from the layout model (may be empty); `lines` and `pt`
/// are the page's text; `heads` the document's headings (for heading numbers).
#[must_use]
pub fn objects(
    page: u32,
    detections: Vec<Detection>,
    pt: &PageText,
    lines: &[Line],
    heads: &[Heading],
) -> Vec<DocObject> {
    let dets = dedupe(detections);
    let detected = !dets.is_empty();
    let mut objs: Vec<DocObject> = dets
        .iter()
        .filter_map(|d| {
            let kind = kind_of(d.class)?;
            Some(DocObject {
                id: String::new(),
                kind,
                rect: PageRect { page, ..d.rect },
                label: None,
                number: None,
                text: None,
                caption_rect: None,
                score: d.score,
                order: 0,
            })
        })
        .collect();

    // Without a model: captions, headings and numbered equations from the text alone.
    if !detected {
        for l in lines {
            if let Some((table, _, _)) = caption_label(&l.text) {
                objs.push(DocObject {
                    id: String::new(),
                    kind: ObjectKind::Caption,
                    rect: l.rect,
                    label: None,
                    number: None,
                    text: None,
                    caption_rect: None,
                    score: 1.0,
                    order: 0,
                });
                // The figure or table itself, from the space around its caption.
                if let Some(area) = caption_area(lines, &l.rect, table) {
                    objs.push(DocObject {
                        id: String::new(),
                        kind: if table {
                            ObjectKind::Table
                        } else {
                            ObjectKind::Figure
                        },
                        rect: area,
                        label: None,
                        number: None,
                        text: None,
                        caption_rect: None,
                        score: 0.5,
                        order: 0,
                    });
                }
            }
        }
        for rect in equation_blocks(lines) {
            objs.push(DocObject {
                id: String::new(),
                kind: ObjectKind::Equation,
                rect,
                label: None,
                number: None,
                text: None,
                caption_rect: None,
                score: 1.0,
                order: 0,
            });
        }
        for h in heads.iter().filter(|h| h.line.page == page) {
            objs.push(DocObject {
                id: String::new(),
                kind: ObjectKind::Heading,
                rect: h.line.rect,
                label: None,
                number: None,
                text: None,
                caption_rect: None,
                score: 1.0,
                order: 0,
            });
        }
    }

    // Text of every object. Equation text is math-font text: cleaned, and dropped when nothing
    // readable is left (it is never shown as the equation; the picture or LaTeX is).
    for o in &mut objs {
        let t = text_in(pt, &o.rect);
        let t = if o.kind == ObjectKind::Equation {
            clean_math_text(&t)
        } else {
            t
        };
        if !t.is_empty() {
            o.text = Some(t);
        }
    }

    // Captions → their figure or table.
    let captions: Vec<(usize, bool, String, String)> = objs
        .iter()
        .enumerate()
        .filter(|(_, o)| o.kind == ObjectKind::Caption)
        .filter_map(|(i, o)| caption_label(o.text.as_deref()?).map(|(t, l, n)| (i, t, l, n)))
        .collect();
    for (ci, is_table, label, number) in captions {
        let cap = objs[ci].rect;
        let want = if is_table {
            ObjectKind::Table
        } else {
            ObjectKind::Figure
        };
        let best = objs
            .iter()
            .enumerate()
            .filter(|(_, o)| o.kind == want && o.label.is_none())
            .filter(|(_, o)| {
                overlap(o.rect.x0, o.rect.x1, cap.x0, cap.x1)
                    > 0.3 * cap.width().min(o.rect.width())
            })
            .map(|(i, o)| {
                let gap = if o.rect.y1 <= cap.y0 {
                    cap.y0 - o.rect.y1
                } else if cap.y1 <= o.rect.y0 {
                    o.rect.y0 - cap.y1
                } else {
                    0.0
                };
                (i, gap)
            })
            .filter(|(_, g)| *g < 80.0)
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(i, _)| i);
        objs[ci].label = Some(label.clone());
        objs[ci].number = Some(number.clone());
        if let Some(t) = best {
            let text = objs[ci].text.clone();
            let o = &mut objs[t];
            o.label = Some(label);
            o.number = Some(number);
            o.text = text;
            o.caption_rect = Some(cap);
        }
    }

    // Equation numbers.
    for o in objs.iter_mut().filter(|o| o.kind == ObjectKind::Equation) {
        if let Some((n, nr)) = equation_number(lines, &o.rect) {
            o.label = Some(format!("({n})"));
            o.number = Some(n);
            // Detected boxes often leave the number out; the object covers both.
            if detected {
                o.rect = o.rect.union(&nr);
            }
        }
    }

    // Headings: the number from the text.
    for o in objs.iter_mut().filter(|o| o.kind == ObjectKind::Heading) {
        let same = |h: &&Heading| {
            let centre = ep_types::PagePoint {
                x: f32::midpoint(h.line.rect.x0, h.line.rect.x1),
                y: f32::midpoint(h.line.rect.y0, h.line.rect.y1),
            };
            h.line.page == page && (iou(&h.line.rect, &o.rect) > 0.2 || o.rect.contains(centre))
        };
        if let Some(h) = heads.iter().find(same)
            && !h.number.is_empty()
        {
            o.number = Some(h.number.clone());
            o.label = Some(h.number.clone());
        }
    }

    // Reading order and ids.
    let rects: Vec<PageRect> = objs.iter().map(|o| o.rect).collect();
    let order = xy_cut(&rects, (0..objs.len()).collect(), 0);
    let mut out: Vec<DocObject> = order.into_iter().map(|i| objs[i].clone()).collect();
    let mut counts = std::collections::HashMap::new();
    for (n, o) in out.iter_mut().enumerate() {
        o.order = u32::try_from(n).unwrap_or(u32::MAX);
        let k = counts.entry(o.kind).or_insert(0u32);
        *k += 1;
        let kind = serde_kind(o.kind);
        o.id = format!("p{page}-{kind}-{k}");
    }
    out
}

fn serde_kind(k: ObjectKind) -> &'static str {
    match k {
        ObjectKind::Figure => "figure",
        ObjectKind::Table => "table",
        ObjectKind::Equation => "equation",
        ObjectKind::Heading => "heading",
        ObjectKind::Title => "title",
        ObjectKind::Caption => "caption",
        ObjectKind::Text => "text",
        ObjectKind::ListItem => "list",
        ObjectKind::Footnote => "footnote",
        ObjectKind::PageFurniture => "furniture",
        ObjectKind::Code => "code",
    }
}

/// Reading order of boxes (indexes), by the same XY-cut as objects: columns before rows.
#[must_use]
pub fn reading_order(rects: &[PageRect]) -> Vec<usize> {
    xy_cut(rects, (0..rects.len()).collect(), 0)
}

/// The object a point falls in: the smallest object containing it (figures, tables and
/// equations before text).
#[must_use]
pub fn at_point(objs: &[DocObject], x: f32, y: f32) -> Option<&DocObject> {
    let rank = |k: ObjectKind| match k {
        ObjectKind::Figure | ObjectKind::Table | ObjectKind::Equation => 0,
        ObjectKind::Caption | ObjectKind::Heading | ObjectKind::Title => 1,
        _ => 2,
    };
    objs.iter()
        .filter(|o| o.rect.contains(ep_types::PagePoint { x, y }))
        .min_by(|a, b| {
            rank(a.kind).cmp(&rank(b.kind)).then(
                (a.rect.width() * a.rect.height()).total_cmp(&(b.rect.width() * b.rect.height())),
            )
        })
}

/// The object a dragged rectangle best covers (`IoU` over 0.5), for snapping.
#[must_use]
pub fn best_cover<'a>(objs: &'a [DocObject], r: &PageRect) -> Option<&'a DocObject> {
    objs.iter()
        .map(|o| (o, iou(&o.rect, r)))
        .filter(|(_, v)| *v > 0.5)
        .max_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(o, _)| o)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::super::{lines as page_lines, testing::page};
    use super::*;

    fn det(class: &str, score: f32, x0: f32, y0: f32, x1: f32, y1: f32) -> Detection {
        Detection {
            class: DOCLING_LABELS.iter().position(|l| *l == class).unwrap(),
            score,
            rect: PageRect {
                page: 1,
                x0,
                y0,
                x1,
                y1,
            },
        }
    }

    #[test]
    fn without_the_model_a_caption_brings_its_figure_or_table() {
        let p = page(
            3,
            &[
                (
                    "This is running text across the whole column, long enough to count as such.",
                    72.0,
                    100.0,
                    10.0,
                ),
                ("x axis label", 150.0, 250.0, 8.0),
                (
                    "Figure 2.3 Urbanisation levels compared.",
                    72.0,
                    300.0,
                    10.0,
                ),
                ("Table 1.4 Comparison between schemes.", 72.0, 400.0, 10.0),
                ("Bus  12  40", 72.0, 414.0, 9.0),
                ("Train  3  10", 72.0, 426.0, 9.0),
                (
                    "This is running text again after the table, long enough to count as such.",
                    72.0,
                    520.0,
                    10.0,
                ),
            ],
        );
        let o = objects(3, vec![], &p, &page_lines(&p), &[]);
        let fig = o
            .iter()
            .find(|x| x.kind == ObjectKind::Figure)
            .expect("figure");
        assert_eq!(fig.label.as_deref(), Some("Figure 2.3"));
        assert!(
            fig.rect.y0 > 105.0 && fig.rect.y1 < 300.0 && fig.rect.height() > 150.0,
            "{:?}",
            fig.rect
        );
        let tab = o
            .iter()
            .find(|x| x.kind == ObjectKind::Table)
            .expect("table");
        assert_eq!(tab.label.as_deref(), Some("Table 1.4"));
        assert!(
            tab.rect.y0 > 405.0 && tab.rect.y1 >= 435.0 && tab.rect.y1 < 520.0,
            "{:?}",
            tab.rect
        );
    }

    #[test]
    fn labels_keep_the_printed_number_and_sentences_are_not_captions() {
        let label = |t: &str| caption_label(t).map(|(_, l, _)| l);
        assert_eq!(
            label("Figure 2.3 Urbanisation levels compared"),
            Some("Figure 2.3".into())
        );
        assert_eq!(
            label("Table 1.1 Comparison of electric vehicles"),
            Some("Table 1.1".into())
        );
        assert_eq!(label("Fig. 4: The setup."), Some("Figure 4".into()));
        assert_eq!(label("Table A.1 Hyperparameters"), Some("Table A.1".into()));
        assert_eq!(label("Figure S2. Extra results"), Some("Figure S2".into()));
        assert_eq!(label("TABLE IV Results"), Some("Table IV".into()));
        // Sentences and continuations are not captions.
        assert_eq!(label("Table 1.1 provides a detailed comparison"), None);
        assert_eq!(label("Table 1.6 was created to compare urban"), None);
        assert_eq!(label("Table 1.4."), None);
        assert_eq!(label("Table 1.7 (Continued)"), None);
    }

    #[test]
    fn captions_pair_numbers_attach_and_order_is_by_columns() {
        let p = page(
            1,
            &[
                (
                    "Left column text that is long enough to be body text here.",
                    50.0,
                    100.0,
                    10.0,
                ),
                ("Figure 2: Results of the model.", 50.0, 330.0, 10.0),
                (
                    "Right column text that is long enough to be body text too.",
                    320.0,
                    100.0,
                    10.0,
                ),
                ("Table 1. Errors of every model.", 320.0, 200.0, 10.0),
                ("(7)", 520.0, 420.0, 10.0),
            ],
        );
        let ls = page_lines(&p);
        let dets = vec![
            det("Text", 0.9, 50.0, 100.0, 290.0, 110.0),
            det("Picture", 0.95, 50.0, 150.0, 290.0, 320.0),
            det("Picture", 0.5, 52.0, 152.0, 288.0, 318.0),
            det("Caption", 0.9, 50.0, 330.0, 290.0, 340.0),
            det("Text", 0.9, 320.0, 100.0, 560.0, 110.0),
            det("Caption", 0.9, 320.0, 200.0, 560.0, 210.0),
            det("Table", 0.9, 320.0, 215.0, 560.0, 300.0),
            det("Formula", 0.8, 320.0, 415.0, 500.0, 435.0),
            det("Text", 0.3, 0.0, 0.0, 10.0, 10.0),
        ];
        let o = objects(1, dets, &p, &ls, &[]);
        assert_eq!(o.len(), 7, "duplicate and low-score removed: {o:#?}");
        let fig = o.iter().find(|x| x.kind == ObjectKind::Figure).unwrap();
        assert_eq!(fig.label.as_deref(), Some("Figure 2"));
        assert_eq!(
            fig.text.as_deref(),
            Some("Figure 2: Results of the model."),
            "words keep their spaces"
        );
        let tab = o.iter().find(|x| x.kind == ObjectKind::Table).unwrap();
        assert_eq!(tab.number.as_deref(), Some("1"));
        let eq = o.iter().find(|x| x.kind == ObjectKind::Equation).unwrap();
        assert_eq!(eq.label.as_deref(), Some("(7)"));
        assert!(
            eq.rect.x1 > 520.0,
            "the number is inside the equation's box"
        );
        // Left column first, then the right one.
        let kinds: Vec<&str> = o.iter().map(|x| x.id.as_str()).collect();
        assert_eq!(kinds[0], "p1-text-1");
        assert!(
            o.iter().position(|x| x.kind == ObjectKind::Figure)
                < o.iter().position(|x| x.kind == ObjectKind::Table)
        );
        assert_eq!(at_point(&o, 100.0, 200.0).unwrap().kind, ObjectKind::Figure);
        assert_eq!(
            best_cover(
                &o,
                &PageRect {
                    page: 1,
                    x0: 315.0,
                    y0: 210.0,
                    x1: 565.0,
                    y1: 305.0
                }
            )
            .unwrap()
            .kind,
            ObjectKind::Table
        );
    }

    #[test]
    fn text_only_equations_are_formula_blocks_not_prose() {
        // A column of body text from x=72 to about 300; a displayed formula with its number on
        // the line below; a sentence ending in "(3)"; a lone number with nothing above it.
        let p = page(
            2,
            &[
                (
                    "This is a long line of body text that sets the column width here.",
                    72.0,
                    100.0,
                    10.0,
                ),
                ("x_{i+1} = f(x_i, u_i)", 130.0, 130.0, 10.0),
                (", (2)", 240.0, 143.0, 10.0),
                (
                    "The model is trained as described in Section 2 and (3)",
                    72.0,
                    180.0,
                    10.0,
                ),
                (
                    "Another long line of body text that also spans the whole column.",
                    72.0,
                    300.0,
                    10.0,
                ),
                ("(4)", 280.0, 400.0, 10.0),
            ],
        );
        let blocks = equation_blocks(&page_lines(&p));
        assert_eq!(blocks.len(), 1, "{blocks:?}");
        assert!(blocks[0].y0 < 131.0 && blocks[0].y1 > 143.0, "{blocks:?}");
        assert_eq!(clean_math_text("x\u{10} = \u{f8ee}y\u{11}"), "x = y");
        assert_eq!(clean_math_text(" , "), "");
    }

    #[test]
    fn text_only_finds_captions_and_equations() {
        let p = page(
            3,
            &[
                ("Figure 4: A caption.", 72.0, 100.0, 10.0),
                (
                    "A long line of body text that sets where the column starts and ends.",
                    72.0,
                    150.0,
                    10.0,
                ),
                ("a = b + c (12)", 200.0, 200.0, 10.0),
            ],
        );
        let o = objects(3, vec![], &p, &page_lines(&p), &[]);
        assert!(
            o.iter()
                .any(|x| x.kind == ObjectKind::Caption && x.label.as_deref() == Some("Figure 4"))
        );
        assert!(
            o.iter()
                .any(|x| x.kind == ObjectKind::Equation && x.number.as_deref() == Some("12"))
        );
    }
}
