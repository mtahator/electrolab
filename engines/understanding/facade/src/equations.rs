//! Equations (P1.M5.UND.4): the symbols with the meanings the paper gives them, and whether the
//! units agree. Both by code; the model is asked only for what code did not find, and what it says
//! is kept only when it stands in the paper.

use ep_bus::{CommandError, Cx};
use ep_document_contract::{Equation, PageText};
use ep_types::BlobRef;
use ep_understanding_contract::{
    Basis, ExplainRegion, Symbol, UnitsCheck, UnitsResult, errors, events,
};
use ep_understanding_core::symbols::{self, Definition};
use ep_understanding_core::{formula, units};
use serde_json::{Map, json};

use super::objects::{Known, Target, beside, quote, span};
use super::prompts::prompts;
use super::{Understanding, around, failed, locate, squash};

/// Text around an equation.
const AROUND: usize = 600;
/// Most symbols the model is asked for, and the text it is shown on each side.
const ASKED: usize = 12;
const SHOWN: usize = 3000;
/// Most pages of a list of symbols.
const LISTS: usize = 3;

/// Pages read together, one text.
struct Pages {
    text: String,
    /// Where each begins (characters).
    parts: Vec<(usize, PageText)>,
}

impl Pages {
    fn new(pages: Vec<PageText>) -> Self {
        let (mut text, mut parts, mut at) = (String::new(), vec![], 0);
        for p in pages {
            let n = p.text.chars().count() + 1;
            text.push_str(&p.text);
            text.push('\n');
            parts.push((at, p));
            at += n;
        }
        Self { text, parts }
    }

    /// Page and top of a character.
    fn place(&self, at: usize) -> Option<(u32, f32)> {
        let (start, p) = self.parts.iter().rev().find(|(s, _)| *s <= at)?;
        let y = p.boxes.iter().skip(at - start).flatten().next()?[1];
        Some((p.page, y))
    }

    /// Where a rectangle of a page begins.
    fn near(&self, rect: &ep_types::PageRect) -> usize {
        self.parts
            .iter()
            .find(|(_, p)| p.page == rect.page)
            .and_then(|(s, p)| Some(s + span(p, rect)?.0))
            .unwrap_or(0)
    }
}

fn row(symbol: String, shown: String, d: Option<&Definition>, pages: &Pages) -> Symbol {
    let place = d.and_then(|d| pages.place(d.at));
    Symbol {
        symbol,
        shown,
        meaning: d.map(|d| d.meaning.clone()),
        unit: d.and_then(|d| d.unit.clone()),
        page: place.map(|p| p.0),
        y: place.map(|p| p.1),
        quote: d.map(|d| squash(&d.quote)),
    }
}

/// The units check of a formula whose symbols are `rows` (in the order of `all`).
fn check(latex: &str, all: &[symbols::Symbol], rows: &[Symbol]) -> UnitsCheck {
    let unit = |s: &symbols::Symbol| {
        let i = all.iter().position(|o| o == s)?;
        units::parse(rows.get(i)?.unit.as_deref()?)
    };
    let c = formula::check(latex, &unit);
    UnitsCheck {
        result: match c.agree {
            Some(true) => UnitsResult::Consistent,
            Some(false) => UnitsResult::Inconsistent,
            None => UnitsResult::Unknown,
        },
        sides: c.sides,
        missing: c.missing,
        unread: c.unread,
    }
}

impl Understanding {
    fn publish_symbols(&self, cx: &Cx, id: Option<&str>, rows: &[Symbol]) {
        if let (Some(id), Ok(bus)) = (id, self.bus()) {
            bus.events().publish(
                events::SYMBOLS,
                json!({ "id": id, "symbols": rows }),
                cx.clock.now_ms(),
            );
        }
    }

    /// The pages where symbols are explained: the equation's, its neighbours, lists of symbols.
    async fn symbol_pages(&self, cx: &Cx, file: &BlobRef, page: u32) -> Pages {
        let lists = ["Nomenclature", "List of symbols"].map(String::from);
        let mut wanted = vec![page, page + 1];
        if page > 1 {
            wanted.insert(0, page - 1);
        }
        let mut found: Vec<u32> = self
            .hits(cx, file, &lists)
            .await
            .iter()
            .map(|h| h.page)
            .collect();
        found.sort_unstable();
        found.dedup();
        found.truncate(LISTS);
        for p in found {
            if !wanted.contains(&p) {
                wanted.push(p);
            }
        }
        let mut pages = vec![];
        for p in wanted {
            let t = self.page(cx, file, p).await;
            if !t.text.is_empty() {
                pages.push(t);
            }
        }
        Pages::new(pages)
    }

    /// What is known about an equation, and the symbols without a meaning (indexes of its rows).
    pub(crate) async fn equation(
        &self,
        cx: &Cx,
        req: &ExplainRegion,
        target: &Target,
        id: Option<&str>,
    ) -> Result<(Known, Vec<usize>), CommandError> {
        let mut known = Known::default();
        let Some(o) = &target.object else {
            return Err(failed(errors::NO_OBJECT, "no equation is here"));
        };
        let input = json!({ "file": req.file, "object": { "page": target.rect.page, "id": o.id.as_str() } });
        let eq: Equation = self.call(cx, "document.equation", input).await?;
        let pages = self.symbol_pages(cx, &req.file, target.rect.page).await;
        let defs = symbols::definitions(&pages.text);
        let near = pages.near(&target.rect);
        let mut missing = vec![];
        if let Some(latex) = &eq.latex {
            let all = symbols::from_latex(latex);
            for (n, s) in all.iter().enumerate() {
                let d = symbols::meaning(s, &all, &defs, near);
                if d.is_none() {
                    missing.push(n);
                }
                known
                    .symbols
                    .push(row(s.latex.clone(), s.shown.clone(), d, &pages));
            }
            known.units = Some(check(latex, &all, &known.symbols));
            known.add(Basis::Formula, "The equation (LaTeX)", latex);
            known.latex = Some(latex.clone());
        } else {
            known.symbols = symbols::in_text(&eq.text, &defs, near)
                .into_iter()
                .map(|d| row(d.printed.clone(), d.printed.clone(), Some(d), &pages))
                .collect();
            known.units = Some(UnitsCheck {
                result: UnitsResult::Unknown,
                sides: vec![],
                missing: vec![],
                unread: true,
            });
            known.add(
                Basis::Formula,
                "The equation, as far as its text could be read",
                &squash(&eq.text),
            );
        }
        self.publish_symbols(cx, id, &known.symbols);
        let said: Vec<String> = known
            .symbols
            .iter()
            .filter_map(|s| Some(format!("{}: {}", s.shown, s.meaning.as_ref()?)))
            .collect();
        known.add(Basis::Text, "Its symbols", &said.join("; "));
        if let Some((_, p)) = pages.parts.iter().find(|(_, p)| p.page == target.rect.page) {
            let (before, after) = beside(p, &target.rect, AROUND);
            known.add(Basis::Text, "Text just before it", &before);
            known.add(Basis::Text, "Text just after it", &after);
            if let Some((a, b)) = span(p, &target.rect) {
                let n = p.text.chars().count();
                let y = |at: usize| {
                    let b = p.boxes.iter().skip(at).flatten().next();
                    b.map_or(target.rect.y0, |b| b[1])
                };
                let from = a.saturating_sub(240);
                known.evidence.extend(
                    [(from, a), (b, (b + 300).min(n))]
                        .into_iter()
                        .filter(|(s, e)| e > s)
                        .filter_map(|r| quote(p, whole_words(p, r), y(r.0))),
                );
            }
        }
        self.mentions(cx, req, target, &mut known).await;
        Ok((known, missing))
    }

    /// Ask the model for the symbols code found no meaning for; what it says is kept when it
    /// stands in the paper (guarantee 6).
    pub(crate) async fn ask_symbols(
        &self,
        cx: &Cx,
        req: &ExplainRegion,
        target: &Target,
        known: &mut Known,
        missing: Vec<usize>,
        id: Option<&str>,
    ) -> Result<(), CommandError> {
        let Some(latex) = known.latex.clone() else {
            return Ok(());
        };
        let all = symbols::from_latex(&latex);
        let pages = self.symbol_pages(cx, &req.file, target.rect.page).await;
        let chars: Vec<char> = pages.text.chars().collect();
        let at = pages.near(&target.rect);
        let shown = around(&chars, (at, at), 2 * SHOWN);
        let asked: Vec<usize> = missing.into_iter().take(ASKED).collect();
        let mut fields = Map::new();
        for n in &asked {
            let field = json!({ "type": "string", "maxLength": 90 });
            fields.insert(all[*n].shown.clone(), field);
        }
        let names: Vec<String> = fields.keys().cloned().collect();
        let schema = json!({
            "type": "object", "properties": fields, "required": names,
            "additionalProperties": false
        });
        let said = self
            .ask(
                cx,
                "explain.symbols",
                prompts().symbols(),
                format!("Symbols: {}", names.join(", ")),
                format!("Text of the paper:\n{shown}"),
                schema,
                40 + 30 * u32::try_from(asked.len()).unwrap_or(0),
            )
            .await;
        let said = match said {
            Ok((v, _)) => v,
            Err(CommandError::Cancelled) => return Err(CommandError::Cancelled),
            // The explanation stands without them.
            Err(_) => return Ok(()),
        };
        let mut found = false;
        for n in asked {
            let s = &all[n];
            let meaning = squash(said[s.shown.as_str()].as_str().unwrap_or_default());
            let meaning = meaning.trim_matches([' ', '.', ',', ';', ':']);
            let Some(sentence) = symbols::stands(&pages.text, s, meaning) else {
                continue;
            };
            let place = pages.parts.iter().find_map(|(_, p)| {
                let c: Vec<char> = p.text.chars().collect();
                let (a, _) = locate(&c, "", &sentence, "")?;
                let y = p.boxes.iter().skip(a).flatten().next()?[1];
                Some((p.page, y))
            });
            let Some((page, y)) = place else { continue };
            found = true;
            let r = &mut known.symbols[n];
            r.unit = units::in_meaning(meaning);
            r.meaning = Some(meaning.to_string());
            r.quote = Some(sentence);
            (r.page, r.y) = (Some(page), Some(y));
        }
        if found {
            known.units = Some(check(&latex, &all, &known.symbols));
            self.publish_symbols(cx, id, &known.symbols);
        }
        Ok(())
    }
}

/// A range without the pieces of words at its ends.
fn whole_words(p: &PageText, (mut a, mut b): (usize, usize)) -> (usize, usize) {
    let c: Vec<char> = p.text.chars().collect();
    while a > 0 && a < b && !c[a - 1].is_whitespace() {
        a += 1;
    }
    while b < c.len() && b > a && !c[b].is_whitespace() {
        b -= 1;
    }
    (a, b)
}
