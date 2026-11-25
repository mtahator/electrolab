//! `understanding.explain_region` and its kin (P1.M5.UND.3–5): what is on the page is explained from
//! what the paper says about it. What code knows comes first, then a streamed draft, the check, and
//! the checks by code: abbreviations stand for what the paper says, and a sentence with a number from
//! nowhere is left out.

use std::time::Instant;

use ep_bus::{CommandError, Cx};
use ep_document_contract::{
    DocObject, Hit, ObjectKind, ObjectRef, PageObjects, PageText, RegionHit,
};
use ep_types::PageRect;
use ep_understanding_contract::{
    Basis, Evidence, ExplainRegion, ExplainStatus, ExplainTimings, ExplainedKind, FigureSeen,
    ObjectExplanation, Symbol, UnitsCheck, errors,
};
use ep_understanding_core::{abbreviations, numbers};
use serde_json::json;

use super::draft::Draft;
use super::prompts::prompts;
use super::{
    PASSAGE_CHARS, Understanding, around, failed, head, ms, numbered, squash, tables, tail,
};

/// Most places that mention it.
const MENTIONS: usize = 4;
/// Text around a paragraph or an equation.
const AROUND: usize = 600;
/// Words inside a figure, and the cells of a table, as the model sees them.
const WORDS: usize = 300;
const CELLS: usize = 2500;
/// Most abbreviations looked up.
const ABBREVIATIONS: usize = 12;
/// The longest paragraph.
const PARAGRAPH: usize = 3000;

/// What is explained.
pub(crate) struct Target {
    pub(crate) kind: ExplainedKind,
    pub(crate) rect: PageRect,
    pub(crate) object: Option<DocObject>,
}

impl Target {
    /// Its printed label.
    fn label(&self) -> Option<String> {
        self.object.as_ref().and_then(|o| o.label.clone())
    }

    fn reference(&self) -> Option<ObjectRef> {
        self.object.as_ref().map(|o| ObjectRef {
            page: self.rect.page,
            id: o.id.clone(),
        })
    }
}

/// What is known about it before the model is asked.
#[derive(Default)]
pub(crate) struct Known {
    /// What the model is given, part by part.
    pub(crate) data: Vec<String>,
    pub(crate) evidence: Vec<Evidence>,
    pub(crate) based_on: Vec<Basis>,
    pub(crate) symbols: Vec<Symbol>,
    pub(crate) units: Option<UnitsCheck>,
    pub(crate) latex: Option<String>,
    pub(crate) seen: Option<FigureSeen>,
}

impl Known {
    pub(crate) fn add(&mut self, basis: Basis, what: &str, text: &str) {
        if !text.trim().is_empty() {
            self.data.push(format!("{what}: {text}"));
            if !self.based_on.contains(&basis) {
                self.based_on.push(basis);
            }
        }
    }
}

/// The characters of a page whose middle lies in `rect`, as the range from the first to the last.
pub(crate) fn span(page: &PageText, rect: &PageRect) -> Option<(usize, usize)> {
    let inside = |b: &[f32; 4]| {
        let (x, y) = (f32::midpoint(b[0], b[2]), f32::midpoint(b[1], b[3]));
        x >= rect.x0 && x <= rect.x1 && y >= rect.y0 && y <= rect.y1
    };
    let mut it = page
        .boxes
        .iter()
        .enumerate()
        .filter(|(_, b)| b.as_ref().is_some_and(inside))
        .map(|(i, _)| i);
    let first = it.next()?;
    Some((first, it.next_back().unwrap_or(first) + 1))
}

/// The text inside `rect` (a range may run through a neighbouring column; this does not).
pub(crate) fn inside(page: &PageText, rect: &PageRect) -> String {
    let mut out = String::new();
    for (c, b) in page.text.chars().zip(&page.boxes) {
        match b {
            Some(b) => {
                let (x, y) = (f32::midpoint(b[0], b[2]), f32::midpoint(b[1], b[3]));
                if x >= rect.x0 && x <= rect.x1 && y >= rect.y0 && y <= rect.y1 {
                    out.push(c);
                } else if !out.ends_with(' ') {
                    out.push(' ');
                }
            }
            None if !out.is_empty() => out.push(c),
            None => {}
        }
    }
    squash(&out)
}

/// The text just before and after `rect` on its page.
pub(crate) fn beside(page: &PageText, rect: &PageRect, most: usize) -> (String, String) {
    let Some((a, b)) = span(page, rect) else {
        return (String::new(), String::new());
    };
    let before: String = page.text.chars().take(a).collect();
    let after: String = page.text.chars().skip(b).collect();
    (squash(tail(&before, most)), squash(head(&after, most)))
}

/// At most [`PASSAGE_CHARS`] characters from the start of a range, as a quote.
pub(crate) fn quote(page: &PageText, range: (usize, usize), y: f32) -> Option<Evidence> {
    let chars: Vec<char> = page.text.chars().collect();
    let end = range.1.min(range.0 + PASSAGE_CHARS).min(chars.len());
    let at = (range.0.min(end), end);
    let mut quote = around(&chars, at, 0);
    if end < range.1 && chars.get(end).is_some_and(|c| !c.is_whitespace()) {
        // Whole words.
        quote = quote
            .rsplit_once(' ')
            .map_or(quote.clone(), |(q, _)| q.to_string());
    }
    (!quote.is_empty()).then_some(Evidence {
        page: page.page,
        y,
        quote,
    })
}

/// How a figure, table or equation is mentioned.
fn names(kind: ExplainedKind, number: &str) -> Vec<String> {
    let n = number;
    match kind {
        ExplainedKind::Figure => ["Figure", "Fig.", "Fig", "Figures", "Figs."]
            .iter()
            .map(|w| format!("{w} {n}"))
            .collect(),
        ExplainedKind::Table => ["Table", "Tables", "Tab."]
            .iter()
            .map(|w| format!("{w} {n}"))
            .collect(),
        ExplainedKind::Equation => ["Eq.", "Equation", "Eqs.", "Equations", "Eqn."]
            .iter()
            .flat_map(|w| [format!("{w} ({n})"), format!("{w} {n}")])
            .collect(),
        ExplainedKind::Paragraph => vec![],
    }
}

/// Whether the number ends here ("Fig. 3", not "Fig. 31" or "Fig. 3.1").
fn ends(after: &str) -> bool {
    let mut c = after.chars();
    match c.next() {
        Some(d) if d.is_ascii_digit() => false,
        Some('.') => !c.next().is_some_and(|d| d.is_ascii_digit()),
        _ => true,
    }
}

impl Understanding {
    /// What the request is about.
    async fn target(&self, cx: &Cx, i: &ExplainRegion) -> Result<Target, CommandError> {
        let nothing = || failed(errors::NO_OBJECT, "nothing is here");
        let here: PageObjects = self
            .call(
                cx,
                "document.objects",
                json!({ "file": i.file, "page": i.page }),
            )
            .await?;
        let (object, rect) = match (&i.object, &i.rect) {
            (Some(id), _) => {
                let o = here.objects.iter().find(|o| o.id == *id);
                let o = o.cloned().ok_or_else(nothing)?;
                let r = o.rect;
                (Some(o), r)
            }
            (None, Some(r)) => {
                let input = json!({ "file": i.file, "page": i.page, "rect": r, "width_px": 64 });
                let hit: RegionHit = self.call(cx, "document.region", input).await?;
                (hit.object, hit.rect)
            }
            (None, None) => return Err(nothing()),
        };
        let kind = |k: ObjectKind| match k {
            ObjectKind::Figure => ExplainedKind::Figure,
            ObjectKind::Table => ExplainedKind::Table,
            ObjectKind::Equation => ExplainedKind::Equation,
            _ => ExplainedKind::Paragraph,
        };
        // A caption stands for what it is the caption of.
        if let Some(c) = object.as_ref().filter(|o| o.kind == ObjectKind::Caption)
            && let Some(o) = here.objects.iter().find(|o| {
                kind(o.kind) != ExplainedKind::Paragraph
                    && o.caption_rect.is_some_and(|r| {
                        let (x, y) = (
                            f32::midpoint(c.rect.x0, c.rect.x1),
                            f32::midpoint(c.rect.y0, c.rect.y1),
                        );
                        x >= r.x0 && x <= r.x1 && y >= r.y0 && y <= r.y1
                    })
            })
        {
            return Ok(Target {
                kind: kind(o.kind),
                rect: o.rect,
                object: Some(o.clone()),
            });
        }
        Ok(Target {
            kind: object
                .as_ref()
                .map_or(ExplainedKind::Paragraph, |o| kind(o.kind)),
            rect: PageRect {
                page: i.page,
                ..rect
            },
            object,
        })
    }

    /// The places that mention it, nearest pages first; not its own caption.
    pub(crate) async fn mentions(&self, cx: &Cx, i: &ExplainRegion, t: &Target, known: &mut Known) {
        let Some(number) = t.object.as_ref().and_then(|o| o.number.clone()) else {
            return;
        };
        let caption = t.object.as_ref().and_then(|o| o.caption_rect);
        let own = |h: &Hit| {
            h.page == t.rect.page
                && h.rects.first().is_some_and(|r| {
                    let (x, y) = (f32::midpoint(r.x0, r.x1), f32::midpoint(r.y0, r.y1));
                    caption.is_some_and(|c| {
                        x >= c.x0 - 1.0 && x <= c.x1 + 1.0 && y >= c.y0 - 1.0 && y <= c.y1 + 1.0
                    })
                })
        };
        let mut hits = self.hits(cx, &i.file, &names(t.kind, &number)).await;
        hits.retain(|h| ends(&h.after) && !own(h));
        hits.sort_by_key(|h| (h.page.abs_diff(t.rect.page), h.page));
        let found = self.quotes(cx, &i.file, hits, MENTIONS).await;
        known.add(
            Basis::Mentions,
            "Places of the paper that mention it",
            &format!("\n{}", numbered(&found)),
        );
        if found.is_empty() {
            return;
        }
        known.evidence.extend(found);
    }

    /// Its caption, as the data and as a quote.
    fn caption(page: &PageText, t: &Target, known: &mut Known) {
        let Some(o) = &t.object else { return };
        let text = o.text.as_deref().map(squash).unwrap_or_default();
        known.add(Basis::Caption, "Caption", head(&text, 1200));
        if let Some(r) = &o.caption_rect
            && let Some(at) = span(page, r)
            && let Some(e) = quote(page, at, r.y0)
        {
            known.evidence.push(e);
        }
    }

    /// What is known about a figure, a table or a paragraph.
    async fn known(&self, cx: &Cx, i: &ExplainRegion, t: &Target) -> Known {
        let mut known = Known::default();
        let page = self.page(cx, &i.file, t.rect.page).await;
        match t.kind {
            ExplainedKind::Paragraph => {
                let text = inside(&page, &t.rect);
                known.add(Basis::Text, "The passage", head(&text, PARAGRAPH));
                if text.is_empty() {
                    return known;
                }
                let (before, after) = beside(&page, &t.rect, AROUND);
                known.add(Basis::Text, "Text just before it", &before);
                known.add(Basis::Text, "Text just after it", &after);
                known
                    .evidence
                    .extend(span(&page, &t.rect).and_then(|at| quote(&page, at, t.rect.y0)));
            }
            ExplainedKind::Table => {
                Self::caption(&page, t, &mut known);
                if let Some(o) = t.reference()
                    && let Ok(table) = self.table(cx, &i.file, &o).await
                {
                    let g = tables::grid(&table);
                    if g.cells.len() > 1 {
                        known.add(
                            Basis::Cells,
                            "The table",
                            &format!("\n{}", head(&g.outline(), CELLS)),
                        );
                    }
                }
                self.mentions(cx, i, t, &mut known).await;
            }
            ExplainedKind::Figure => {
                Self::caption(&page, t, &mut known);
                self.mentions(cx, i, t, &mut known).await;
                let words = inside(&page, &t.rect);
                known.add(Basis::Words, "Words inside it", head(&words, WORDS));
                known.seen = self.seen(cx, i, t).await;
                if let Some(s) = known.seen.clone() {
                    known.add(Basis::Picture, "What was seen in the picture", &s.text);
                }
            }
            ExplainedKind::Equation => {}
        }
        known
    }

    /// `understanding.explain_region`, and with `must` `explain_equation` and `explain_figure`.
    ///
    /// # Errors
    /// Nothing is there, it is something else, or the errors of `inference.complete`.
    #[allow(clippy::too_many_lines)] // one flow: what is known, draft, check, abbreviations, numbers
    pub async fn explain_region(
        &self,
        cx: &Cx,
        i: ExplainRegion,
        must: Option<ExplainedKind>,
    ) -> Result<ObjectExplanation, CommandError> {
        let started = Instant::now();
        let t = self.target(cx, &i).await?;
        if must.is_some_and(|k| k != t.kind) {
            return Err(failed(errors::NO_OBJECT, "this is something else"));
        }
        let id = i.stream.as_ref().map(|s| s.id.clone());
        let (mut known, missing) = if t.kind == ExplainedKind::Equation {
            self.equation(cx, &i, &t, id.as_deref()).await?
        } else {
            (self.known(cx, &i, &t).await, vec![])
        };
        let mut out = ObjectExplanation {
            kind: t.kind,
            page: t.rect.page,
            object: t.object.as_ref().map(|o| o.id.clone()),
            label: t.label(),
            text: String::new(),
            status: ExplainStatus::Unchecked,
            evidence: vec![],
            symbols: vec![],
            units: None,
            latex: None,
            seen: None,
            based_on: vec![],
            left_out: 0,
            model: String::new(),
            timings: ExplainTimings::default(),
        };
        if known.data.is_empty() {
            if t.kind == ExplainedKind::Paragraph {
                return Err(failed(errors::NO_OBJECT, "no text is here"));
            }
            // The paper says nothing about it: nothing is made up.
            out.timings.total_ms = ms(started);
            return Ok(out);
        }

        // An abbreviation stands for what the paper says, not for what the model thinks of first.
        let abbrs = abbreviations::all(&known.data.join("\n"), ABBREVIATIONS);
        let stand_for = self.stands_for(cx, &i.file, t.rect.page, &abbrs).await;
        if !stand_for.is_empty() {
            let list: Vec<String> = stand_for.iter().map(|(a, w)| format!("{a}: {w}")).collect();
            known.data.push(format!(
                "What its abbreviations stand for: {}",
                list.join("; ")
            ));
        }

        let (name, label) = (kind_name(t.kind), t.label());
        let what = match (t.kind, &label) {
            (ExplainedKind::Equation, Some(l)) => format!("Equation {l}"),
            (_, Some(l)) => l.clone(),
            (k, None) => format!("this {}", kind_name(k)),
        };
        let prefill = match (t.kind, &label) {
            (ExplainedKind::Figure, Some(l)) => Some(format!("{l} shows")),
            (ExplainedKind::Table, Some(l)) => Some(format!("{l} gives")),
            (ExplainedKind::Equation, _) => Some("This equation".to_string()),
            _ => None,
        };
        let (words, level) = (prompts(), prompts().level(i.depth));
        let data = known.data.join("\n\n");
        let question = format!("Explain {what}.");
        let draft = self
            .draft(
                cx,
                Draft {
                    task: &format!("explain.{name}"),
                    id: id.as_deref(),
                    system: level.told(words.object_draft(t.kind)),
                    question: question.clone(),
                    data: data.clone(),
                    prefill,
                    tokens: level.draft,
                },
            )
            .await?;
        out.timings.draft_ms = ms(started);
        out.timings.first_token_ms =
            out.timings.draft_ms.saturating_sub(draft.usage.total_ms) + draft.usage.first_token_ms;
        out.model = draft.model;
        out.text = numbers::whole(&draft.content);
        if let Some(id) = &id {
            self.publish(cx, id, "checking", &out.text);
        }
        let checking = Instant::now();
        let (status, corrected) = self
            .check(
                cx,
                &format!("explain.{name}.check"),
                level.told(words.object_check()),
                format!("Explanation of {what}: {}", out.text),
                data.clone(),
                level.check,
            )
            .await?;
        out.status = status;
        let before = out.text.clone();
        match corrected {
            // A single remark is no new explanation.
            Some(c) if 2 * numbers::sentences(&c).len() < numbers::sentences(&out.text).len() => {
                out.status = ExplainStatus::Unchecked;
            }
            Some(c) => out.text = c,
            None => {}
        }
        let (kept, left, changed) = abbreviations::faithful(&out.text, &stand_for);
        let (kept, unknown) = numbers::supported(&kept, &[&data, &question]);
        if changed || unknown > 0 {
            out.text = kept;
            out.left_out = left + unknown;
            if out.status == ExplainStatus::Checked {
                out.status = ExplainStatus::Corrected;
            }
        }
        out.timings.check_ms = ms(checking);
        if let Some(id) = id.as_ref().filter(|_| out.text != before) {
            self.publish(cx, id, "check", &out.text);
        }
        if !missing.is_empty() {
            self.ask_symbols(cx, &i, &t, &mut known, missing, id.as_deref())
                .await?;
        }
        out.evidence = known.evidence;
        out.symbols = known.symbols;
        out.units = known.units;
        out.latex = known.latex;
        out.seen = known.seen;
        out.based_on = known.based_on;
        out.timings.total_ms = ms(started);
        Ok(out)
    }
}

fn kind_name(kind: ExplainedKind) -> &'static str {
    match kind {
        ExplainedKind::Paragraph => "paragraph",
        ExplainedKind::Equation => "equation",
        ExplainedKind::Figure => "figure",
        ExplainedKind::Table => "table",
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    /// Two lines of ten characters, 10 points high and 10 wide each.
    fn page() -> PageText {
        let text = "first line\nsecond one";
        let mut boxes = vec![];
        let (mut x, mut y) = (0.0, 0.0);
        for c in text.chars() {
            if c == '\n' {
                boxes.push(None);
                (x, y) = (0.0, 10.0);
            } else {
                boxes.push(Some([x, y, x + 10.0, y + 10.0]));
                x += 10.0;
            }
        }
        PageText {
            page: 1,
            text: text.into(),
            boxes,
            words: vec![],
            lines: vec![],
            unknown: 0,
        }
    }

    fn rect(x0: f32, y0: f32, x1: f32, y1: f32) -> PageRect {
        PageRect {
            page: 1,
            x0,
            y0,
            x1,
            y1,
        }
    }

    #[test]
    fn text_is_read_inside_and_beside_a_place() {
        let p = page();
        assert_eq!(inside(&p, &rect(0.0, 0.0, 50.0, 20.0)), "first secon");
        assert_eq!(span(&p, &rect(60.0, 0.0, 99.0, 9.0)), Some((6, 10)));
        assert_eq!(span(&p, &rect(500.0, 0.0, 600.0, 9.0)), None);
        assert_eq!(
            beside(&p, &rect(0.0, 10.0, 60.0, 20.0), 4),
            ("ine".into(), "one".into())
        );
        let q = quote(&p, (0, 8), 3.0).unwrap();
        assert_eq!((q.quote.as_str(), q.y), ("first li", 3.0));
    }

    #[test]
    fn a_number_ends_where_it_ends() {
        assert!(ends("(b) shows") && ends(". Then") && ends(""));
        assert!(!ends("1 shows") && !ends(".2 shows"));
        assert!(names(ExplainedKind::Equation, "7").contains(&"Eq. (7)".to_string()));
    }
}
