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

static CAPTION: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"^(Fig\.?|Figure|FIGURE|Table|TABLE|Tab\.|شکل|جدول)\s*([0-9\u{6f0}-\u{6f9}]+|[IVX]+)\b",
    )
    .unwrap_or_else(|_| unreachable!())
});
static EQ_NUMBER: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^\((\d{1,3}(?:\.\d{1,3}){0,2}[a-z]?)\)$").unwrap_or_else(|_| unreachable!())
});

/// A caption label in text: (is a table, "Table 2", "2").
fn caption_label(text: &str) -> Option<(bool, String, String)> {
    let c = CAPTION.captures(text.trim())?;
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
            if caption_label(&l.text).is_some() {
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
            }
            let last = l.text.split_whitespace().last().unwrap_or("");
            if EQ_NUMBER.is_match(last) && l.rect.x1 > 0.0 {
                // The equation spans its line(s) up to the number.
                objs.push(DocObject {
                    id: String::new(),
                    kind: ObjectKind::Equation,
                    rect: l.rect,
                    label: None,
                    number: None,
                    text: None,
                    caption_rect: None,
                    score: 1.0,
                    order: 0,
                });
            }
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

    // Text of every object.
    for o in &mut objs {
        let t = text_in(pt, &o.rect);
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
    fn text_only_finds_captions_and_equations() {
        let p = page(
            3,
            &[
                ("Figure 4: A caption.", 72.0, 100.0, 10.0),
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
