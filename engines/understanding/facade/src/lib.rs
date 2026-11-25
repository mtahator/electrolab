//! Understanding engine facade: `understanding.explain_span` (P1.M5.UND.2).
//!
//! Objects on the page: `objects` (P1.M5.UND.3–5). A fast draft from the nearby text, streamed; meanwhile the paper is searched for the
//! selection; then the model checks the draft against those passages and either agrees or
//! corrects it (spike S3: 83% → 98% correct with Qwen3-4B). The engine uses the Document and
//! Inference engines through their contracts only, over the bus.

mod draft;
mod equations;
mod figures;
mod objects;
mod prompts;
mod sorting;
mod tables;

use std::collections::hash_map::Entry;
use std::sync::{Arc, Weak};
use std::time::Instant;

use ep_bus::{Bus, CommandError, Cx, Envelope, Permission, RegisterError, Spec, Stability};
use ep_document_contract::{Hit, PageText, SearchResult};
use ep_types::BlobRef;
pub use ep_understanding_contract as contract;
use ep_understanding_contract::{
    Evidence, ExplainRegion, ExplainRequest, ExplainStatus, ExplainTimings, ExplainedKind,
    Explanation, FiguresSeenRequest, LookAtFigures, SuggestFolders, TableQuestion, commands as cmd,
    errors, events,
};
use ep_understanding_core::{abbreviations, numbers};
use parking_lot::RwLock;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};

use draft::Draft;
use prompts::prompts;

const NEARBY: usize = 500;
const PASSAGES: usize = 6;
const PASSAGE_CHARS: usize = 400;

/// The engine.
#[derive(Default)]
pub struct Understanding {
    bus: RwLock<Weak<Bus>>,
    /// What was seen in figures is kept by one at a time.
    keeping: tokio::sync::Mutex<()>,
}

fn failed(kind: &str, message: impl Into<String>) -> CommandError {
    CommandError::Failed {
        kind: kind.into(),
        message: message.into(),
    }
}

fn ms(t: Instant) -> u64 {
    u64::try_from(t.elapsed().as_millis()).unwrap_or(u64::MAX)
}

/// Collapse runs of white space, for comparing text from different extractions.
fn squash(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn tail(s: &str, n: usize) -> &str {
    let start = s
        .char_indices()
        .rev()
        .nth(n.saturating_sub(1))
        .map_or(0, |(i, _)| i);
    &s[start..]
}

fn head(s: &str, n: usize) -> &str {
    s.char_indices().nth(n).map_or(s, |(i, _)| &s[..i])
}

/// Where a hit stands in its page, found by its letters and digits (spaces, line ends and
/// hyphens differ between the search's text and the page's).
fn locate(page: &[char], before: &str, matched: &str, after: &str) -> Option<(usize, usize)> {
    const NEAR: usize = 24;
    let fold = |s: &str| -> Vec<char> { s.chars().filter(|c| c.is_alphanumeric()).collect() };
    let letters: Vec<usize> = (0..page.len())
        .filter(|i| page[*i].is_alphanumeric())
        .collect();
    let (b, m, a) = (fold(before), fold(matched), fold(after));
    let (b, a) = (&b[b.len().saturating_sub(NEAR)..], &a[..a.len().min(NEAR)]);
    if m.is_empty() {
        return None;
    }
    let find = |needle: &[char]| {
        letters
            .windows(needle.len())
            .position(|w| w.iter().zip(needle).all(|(i, c)| page[*i] == *c))
    };
    let whole: Vec<char> = b.iter().chain(&m).chain(a).copied().collect();
    let start = find(&whole).map(|i| i + b.len()).or_else(|| find(&m))?;
    Some((letters[start], letters[start + m.len() - 1] + 1))
}

/// At most `max` characters of the page around `range`, cut at word boundaries, white space
/// collapsed.
fn around(chars: &[char], range: (usize, usize), max: usize) -> String {
    let (start, end) = (range.0.min(chars.len()), range.1.min(chars.len()));
    let side = max.saturating_sub(end - start) / 2;
    let (mut a, mut b) = (start.saturating_sub(side), (end + side).min(chars.len()));
    while a > 0 && a < start && !chars[a - 1].is_whitespace() {
        a += 1;
    }
    while b < chars.len() && b > end && !chars[b].is_whitespace() {
        b -= 1;
    }
    squash(&chars[a..b].iter().collect::<String>())
}

/// Quotes as the model sees them.
fn numbered(quotes: &[Evidence]) -> String {
    quotes
        .iter()
        .enumerate()
        .map(|(n, e)| format!("[{}] {}", n + 1, e.quote))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Whether two quotes of a page are mostly the same passage (the middle of one is in the other).
fn overlap(a: &str, b: &str) -> bool {
    let middle = |s: &str| {
        let c: Vec<char> = s.chars().collect();
        c[c.len() / 4..c.len() - c.len() / 4]
            .iter()
            .collect::<String>()
    };
    a.contains(&middle(b)) || b.contains(&middle(a))
}

impl Understanding {
    /// A new engine.
    #[must_use]
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    fn bus(&self) -> Result<Arc<Bus>, CommandError> {
        self.bus
            .read()
            .upgrade()
            .ok_or_else(|| failed("io", "the bus is gone"))
    }

    /// Call another engine's command; cancelling `cx` cancels it too.
    async fn call<T: DeserializeOwned>(
        &self,
        cx: &Cx,
        command: &str,
        input: Value,
    ) -> Result<T, CommandError> {
        let bus = self.bus()?;
        let env = Envelope::new(command, input);
        let id = env.id;
        let v = tokio::select! {
            r = bus.call(env) => r?,
            () = cx.cancel.cancelled() => {
                bus.cancel(id);
                return Err(CommandError::Cancelled);
            }
        };
        serde_json::from_value(v).map_err(|e| failed("bad_output", format!("{command}: {e}")))
    }

    fn publish(&self, cx: &Cx, id: &str, phase: &str, text: &str) {
        if let Ok(bus) = self.bus() {
            bus.events().publish(
                events::EXPLAINING,
                json!({ "id": id, "phase": phase, "text": text }),
                cx.clock.now_ms(),
            );
        }
    }

    /// The text around the selection on its page.
    async fn nearby(&self, cx: &Cx, i: &ExplainRequest) -> (String, String) {
        if i.before.is_some() || i.after.is_some() {
            return (
                i.before.clone().unwrap_or_default(),
                i.after.clone().unwrap_or_default(),
            );
        }
        let Ok(t) = self
            .call::<PageText>(
                cx,
                "document.text",
                json!({ "file": i.file, "page": i.page }),
            )
            .await
        else {
            return (String::new(), String::new());
        };
        match t.text.find(i.selection.trim()) {
            Some(at) => {
                let end = at + i.selection.trim().len();
                (
                    tail(&t.text[..at], NEARBY).to_string(),
                    head(&t.text[end..], NEARBY).to_string(),
                )
            }
            None => (String::new(), head(&t.text, NEARBY).to_string()),
        }
    }

    /// Passages of the paper with the selection; definitions ("… (PINN)", "PINN = …") first,
    /// other pages before the selection's own spot.
    async fn passages(&self, cx: &Cx, i: &ExplainRequest) -> Vec<Evidence> {
        let defines = |h: &Hit| {
            h.after.trim_start().starts_with(')')
                || h.before.trim_end().ends_with('(')
                || h.after.trim_start().starts_with('=')
                || h.after.trim_start().starts_with(" is")
        };
        let mut hits = self
            .hits(cx, &i.file, &[i.selection.trim().to_string()])
            .await;
        hits.sort_by_key(|h| (!defines(h), h.page == i.page, h.page));
        self.quotes(cx, &i.file, hits, PASSAGES).await
    }

    /// What the paper itself says abbreviations stand for: the last place at or before `page` that
    /// defines one, else the first one after it. Those it does not define are not there.
    async fn stands_for(
        &self,
        cx: &Cx,
        file: &BlobRef,
        page: u32,
        abbrs: &[String],
    ) -> Vec<(String, String)> {
        if abbrs.is_empty() {
            return vec![];
        }
        let terms: Vec<String> = abbrs
            .iter()
            .flat_map(|a| [format!("({a}"), format!("{a} (")])
            .collect();
        let mut hits = self.hits(cx, file, &terms).await;
        hits.sort_by_key(|h| (h.page > page, h.page.abs_diff(page)));
        abbrs
            .iter()
            .enumerate()
            .filter_map(|(n, abbr)| {
                let found: Vec<String> = hits
                    .iter()
                    .filter(|h| h.term / 2 == n)
                    .filter_map(|h| {
                        let around = format!("{}{}{}", h.before, h.matched, h.after);
                        let at = h.before.len() + h.matched.find(abbr.as_str())?;
                        abbreviations::defined(abbr, &around[..at], &around[at + abbr.len()..])
                    })
                    .collect();
                let words = abbreviations::best_written(found.first()?, &found);
                Some((abbr.clone(), words))
            })
            .collect()
    }

    /// Where the paper has one of the `terms`.
    async fn hits(&self, cx: &Cx, file: &BlobRef, terms: &[String]) -> Vec<Hit> {
        let most = 40 * terms.len().max(1);
        let terms: Vec<Value> = terms.iter().map(|t| json!({ "text": t })).collect();
        let req = json!({
            "papers": [file.hash.to_string()],
            "terms": terms,
            "context": { "unit": "words", "before": 40, "after": 40 },
            "limits": { "max_hits": most, "max_hits_per_term": 40 },
            "matching": { "mode": "exact" },
        });
        self.call::<SearchResult>(cx, "document.search", req)
            .await
            .map(|r| r.hits)
            .unwrap_or_default()
    }

    /// The text of a page; empty when it cannot be read.
    async fn page(&self, cx: &Cx, file: &BlobRef, page: u32) -> PageText {
        self.call(cx, "document.text", json!({ "file": file, "page": page }))
            .await
            .unwrap_or(PageText {
                page,
                text: String::new(),
                boxes: vec![],
                words: vec![],
                lines: vec![],
                unknown: 0,
            })
    }

    /// At most `most` quotes of `hits`, in their order, each place once. Quotes are kept only when
    /// they are verbatim in the page text (guarantee 3).
    async fn quotes(&self, cx: &Cx, file: &BlobRef, hits: Vec<Hit>, most: usize) -> Vec<Evidence> {
        let mut out: Vec<Evidence> = vec![];
        let mut pages: std::collections::HashMap<u32, Vec<char>> = std::collections::HashMap::new();
        for h in hits {
            if out.len() >= most {
                break;
            }
            let chars = match pages.entry(h.page) {
                Entry::Occupied(e) => e.into_mut(),
                Entry::Vacant(e) => {
                    let page = self.page(cx, file, h.page).await;
                    e.insert(page.text.chars().collect())
                }
            };
            let Some(at) = locate(chars, &h.before, &h.matched, &h.after) else {
                continue;
            };
            let quote = around(chars, at, PASSAGE_CHARS);
            if quote.is_empty()
                || out
                    .iter()
                    .any(|e| e.page == h.page && overlap(&e.quote, &quote))
            {
                continue;
            }
            out.push(Evidence {
                page: h.page,
                y: h.rects.first().map_or(0.0, |r| r.y0),
                quote,
            });
        }
        out
    }

    /// `understanding.explain_span`.
    ///
    /// # Errors
    /// An empty selection, or the errors of `inference.complete`.
    pub async fn explain_span(
        &self,
        cx: &Cx,
        i: ExplainRequest,
    ) -> Result<Explanation, CommandError> {
        let started = Instant::now();
        let sel = i.selection.trim().to_string();
        if sel.is_empty() {
            return Err(failed(
                errors::EMPTY_SELECTION,
                "select a word or a sentence first",
            ));
        }
        let sel = head(&sel, 300).to_string();
        let id = i.stream.as_ref().map(|s| s.id.clone());
        let (before, after) = self.nearby(cx, &i).await;
        // An abbreviation stands for what the paper says, not for what the model thinks of first.
        let abbr: Vec<String> = abbreviations::abbreviation(&sel)
            .map(String::from)
            .into_iter()
            .collect();
        let stands_for = self.stands_for(cx, &i.file, i.page, &abbr).await.pop();
        let begin = match &stands_for {
            Some((abbr, words)) => format!("{abbr} stands for {words}"),
            None => format!("{sel} "),
        };

        // The draft streams while the paper is searched.
        let (words, level) = (&prompts().span, prompts().level(i.depth));
        let (draft, passages) = tokio::join!(
            self.draft(
                cx,
                Draft {
                    task: "explain.draft",
                    id: id.as_deref(),
                    system: level.told(&words.draft),
                    question: format!("Explain only these words: \"{sel}\""),
                    data: format!(
                        "Text just before them: {before}\n\nText just after them: {after}"
                    ),
                    prefill: Some(begin),
                    tokens: level.draft,
                }
            ),
            self.passages(cx, &i)
        );
        let draft = draft?;
        let draft_ms = ms(started);
        let first_token_ms =
            draft_ms.saturating_sub(draft.usage.total_ms) + draft.usage.first_token_ms;
        let mut text = numbers::whole(&draft.content);
        if let Some(id) = &id {
            let phase = if passages.is_empty() {
                "draft"
            } else {
                "checking"
            };
            self.publish(cx, id, phase, &text);
        }

        let mut status = ExplainStatus::Unchecked;
        let mut check_ms = 0;
        if !passages.is_empty() {
            let t = Instant::now();
            let (said, corrected) = self
                .check(
                    cx,
                    "explain.check",
                    level.told(&words.check),
                    format!("Term: \"{sel}\"\n\nExplanation: {text}"),
                    format!("Passages from the PDF:\n{}", numbered(&passages)),
                    level.check,
                )
                .await?;
            check_ms = ms(t);
            status = said;
            let keeps = |c: &String| {
                stands_for
                    .as_ref()
                    .is_none_or(|(_, w)| abbreviations::says(c, w))
            };
            if corrected.as_ref().is_some_and(|c| !keeps(c)) {
                status = ExplainStatus::Unchecked;
            } else if let Some(c) = corrected {
                text = c;
                if let Some(id) = &id {
                    self.publish(cx, id, "check", &text);
                }
            }
        }
        Ok(Explanation {
            selection: sel,
            text,
            status,
            evidence: passages,
            model: draft.model,
            timings: ExplainTimings {
                first_token_ms,
                draft_ms,
                check_ms,
                total_ms: ms(started),
            },
        })
    }

    /// Register the commands on `bus`.
    ///
    /// # Errors
    /// Duplicate names.
    #[allow(clippy::too_many_lines)] // a list
    pub fn register(self: &Arc<Self>, bus: &Arc<Bus>) -> Result<(), RegisterError> {
        *self.bus.write() = Arc::downgrade(bus);
        let me = self.clone();
        bus.register(
            Spec {
                name: cmd::EXPLAIN_SPAN,
                summary: "Explain a selection: a fast draft, then a version checked against the paper",
                stability: Stability::Experimental,
                permission: Permission::read(),
            },
            move |cx, i: ExplainRequest| {
                let me = me.clone();
                async move { me.explain_span(&cx, i).await }
            },
        )?;
        let me = self.clone();
        bus.register(
            Spec {
                name: cmd::SUGGEST_FOLDERS,
                summary: "Suggest folders for papers; nothing is moved",
                stability: Stability::Experimental,
                permission: Permission::read(),
            },
            move |cx, i: SuggestFolders| {
                let me = me.clone();
                async move { me.suggest_folders(&cx, i).await }
            },
        )?;
        let me = self.clone();
        bus.register(
            Spec {
                name: cmd::TABLE_ANSWER,
                summary: "Answer a question from a table's cells; the number is computed",
                stability: Stability::Experimental,
                permission: Permission::read(),
            },
            move |cx, i: TableQuestion| {
                let me = me.clone();
                async move { me.table_answer(&cx, i).await }
            },
        )?;
        let me = self.clone();
        bus.register(
            Spec {
                name: cmd::EXPLAIN_REGION,
                summary: "Explain what is at a place: a paragraph, an equation, a figure or a table",
                stability: Stability::Experimental,
                permission: Permission::read(),
            },
            move |cx, i: ExplainRegion| {
                let me = me.clone();
                async move { me.explain_region(&cx, i, None).await }
            },
        )?;
        let me = self.clone();
        bus.register(
            Spec {
                name: cmd::EXPLAIN_EQUATION,
                summary: "Explain an equation: meaning, symbols, units",
                stability: Stability::Experimental,
                permission: Permission::read(),
            },
            move |cx, i: ExplainRegion| {
                let me = me.clone();
                async move {
                    me.explain_region(&cx, i, Some(ExplainedKind::Equation))
                        .await
                }
            },
        )?;
        let me = self.clone();
        bus.register(
            Spec {
                name: cmd::EXPLAIN_FIGURE,
                summary: "Explain a figure from what the paper says about it",
                stability: Stability::Experimental,
                permission: Permission::read(),
            },
            move |cx, i: ExplainRegion| {
                let me = me.clone();
                async move { me.explain_region(&cx, i, Some(ExplainedKind::Figure)).await }
            },
        )?;
        let me = self.clone();
        bus.register(
            Spec {
                name: cmd::LOOK_AT_FIGURES,
                summary: "Look at the pictures of a paper, when the user agrees, and keep what was seen",
                stability: Stability::Experimental,
                permission: Permission::create(),
            },
            move |cx, i: LookAtFigures| {
                let me = me.clone();
                async move { me.look_at_figures(&cx, i).await }
            },
        )?;
        let me = self.clone();
        bus.register(
            Spec {
                name: cmd::FIGURES_SEEN,
                summary: "What was seen in a paper's pictures",
                stability: Stability::Experimental,
                permission: Permission::read(),
            },
            move |cx, i: FiguresSeenRequest| {
                let me = me.clone();
                async move { me.figures_seen(&cx, i).await }
            },
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quotes_are_cut_from_the_page_at_words() {
        let page: Vec<char> = "alpha beta\ngamma delta epsilon".chars().collect();
        let at = page.iter().collect::<String>().find("gamma").unwrap();
        assert_eq!(around(&page, (at, at + 5), 18), "beta gamma delta");
        assert_eq!(
            around(&page, (at, at + 5), 400),
            "alpha beta gamma delta epsilon"
        );
        assert_eq!(
            around(&page, (500, 600), 40),
            "gamma delta epsilon",
            "a range past the end is harmless"
        );
        let hit = locate(&page, "alpha be-", "ta gam", "ma delta").unwrap();
        assert_eq!(page[hit.0..hit.1].iter().collect::<String>(), "ta\ngam");
        assert_eq!(locate(&page, "", "zeta", ""), None);
        assert!(overlap(
            "one two three four five",
            "two three four five six"
        ));
        assert!(!overlap("one two three four", "seven eight nine ten"));
    }

    #[test]
    fn head_and_tail_count_characters() {
        assert_eq!(head("αβγδ", 2), "αβ");
        assert_eq!(tail("αβγδ", 2), "γδ");
        assert_eq!(tail("ab", 5), "ab");
    }
}
