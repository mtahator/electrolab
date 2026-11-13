//! The per-paper search index: original characters, normalised word tokens with their spans,
//! and a vocabulary with positions (an inverted index). Built once per paper, during preparation.

use std::collections::HashMap;

use ep_types::PageRect;

use super::normalize::{Class, Sep, class, fold};
use super::words::stem;

/// One page of extracted text, as the `PDFium` adapter delivers it: characters in reading order,
/// each with its box when it has one (spaces and line breaks often have none).
#[derive(Clone, Debug, Default)]
pub struct PageChars {
    /// 1-based page number.
    pub page: u32,
    /// Characters with their boxes (page space, ADR-003).
    pub chars: Vec<(char, Option<PageRect>)>,
}

/// A section heading at a position in the paper's text (from the outline or layout).
#[derive(Clone, Debug)]
pub struct Heading {
    /// Character offset in the paper's text where the section starts.
    pub at: usize,
    /// The heading's text.
    pub title: String,
}

/// A word in the paper.
#[derive(Clone, Debug)]
pub struct Token {
    /// `[start, end)` in the paper's original characters.
    pub start: usize,
    pub end: usize,
    /// 1-based page.
    pub page: u32,
    /// Vocabulary id of the key form.
    pub vocab: u32,
    /// Raw form (case and accents kept), for case- or accent-sensitive matching.
    pub raw: String,
    /// What separated this word from the previous one.
    pub sep: Sep,
    /// Index into `headings` of the section this word is in.
    pub section: Option<u32>,
}

/// A distinct word key.
#[derive(Clone, Debug)]
pub struct Entry {
    pub key: String,
    pub chars: Vec<char>,
    /// Token indices where it occurs, in order.
    pub positions: Vec<u32>,
}

/// Everything needed to search one paper.
#[derive(Clone, Debug, Default)]
pub struct SearchIndex {
    pub(super) text: Vec<char>,
    pub(super) boxes: Vec<Option<PageRect>>,
    pub(super) page_of: Vec<u32>,
    pub(super) tokens: Vec<Token>,
    pub(super) vocab: Vec<Entry>,
    pub(super) by_key: HashMap<String, u32>,
    pub(super) by_stem: HashMap<String, Vec<u32>>,
    pub(super) headings: Vec<Heading>,
    /// Token index where each page starts, in page order: `(page, first token)`.
    pub(super) page_starts: Vec<(u32, u32)>,
}

impl SearchIndex {
    /// Build the index for one paper. Pure: no I/O; cost grows linearly with the text.
    #[must_use]
    pub fn build(pages: &[PageChars], headings: Vec<Heading>) -> Self {
        let mut ix = Self {
            headings,
            ..Self::default()
        };
        let mut key = Vec::new();
        let mut raw = Vec::new();
        let mut word_start: Option<usize> = None;
        let mut sep = Sep::None;
        let mut joining = false;
        for p in pages {
            ix.page_starts
                .push((p.page, u32::try_from(ix.tokens.len()).unwrap_or(u32::MAX)));
            for &(c, b) in &p.chars {
                let at = ix.text.len();
                ix.text.push(c);
                ix.boxes.push(b);
                ix.page_of.push(p.page);
                match class(c) {
                    Class::Word => {
                        joining = false;
                        if word_start.is_none() {
                            word_start = Some(at);
                        }
                        fold(c, &mut key, &mut raw);
                    }
                    Class::Skip => {}
                    Class::Join => joining = word_start.is_some(),
                    Class::Sep(s) => {
                        // after a soft hyphen the line break (and indentation) does not end the word
                        if joining && matches!(s, Sep::Break | Sep::Space) {
                            continue;
                        }
                        joining = false;
                        if let Some(start) = word_start.take() {
                            ix.push_token(start, at, p.page, &mut key, &mut raw, sep);
                            sep = s;
                        } else if sep_rank(s) > sep_rank(sep) || sep == Sep::None {
                            sep = s;
                        }
                    }
                }
            }
            if let Some(start) = word_start.take() {
                let end = ix.text.len();
                ix.push_token(start, end, p.page, &mut key, &mut raw, sep);
            }
            sep = Sep::Break;
            joining = false;
        }
        ix.assign_sections();
        ix
    }

    fn push_token(
        &mut self,
        start: usize,
        end: usize,
        page: u32,
        key: &mut Vec<char>,
        raw: &mut Vec<char>,
        sep: Sep,
    ) {
        if key.is_empty() {
            raw.clear();
            return;
        }
        let k: String = key.iter().collect();
        let id = if let Some(&id) = self.by_key.get(&k) {
            id
        } else {
            let id = u32::try_from(self.vocab.len()).unwrap_or(u32::MAX);
            let st = stem(&k);
            self.by_stem.entry(st).or_default().push(id);
            self.vocab.push(Entry {
                key: k.clone(),
                chars: key.clone(),
                positions: Vec::new(),
            });
            self.by_key.insert(k, id);
            id
        };
        let t = u32::try_from(self.tokens.len()).unwrap_or(u32::MAX);
        self.vocab[id as usize].positions.push(t);
        self.tokens.push(Token {
            start,
            end,
            page,
            vocab: id,
            raw: raw.iter().collect(),
            sep,
            section: None,
        });
        key.clear();
        raw.clear();
    }

    fn assign_sections(&mut self) {
        if self.headings.is_empty() {
            return;
        }
        self.headings.sort_by_key(|h| h.at);
        let mut h = 0usize;
        for t in &mut self.tokens {
            while h + 1 < self.headings.len() && self.headings[h + 1].at <= t.start {
                h += 1;
            }
            if self.headings[h].at <= t.start {
                t.section = u32::try_from(h).ok();
            }
        }
    }

    /// Number of words in the paper.
    #[must_use]
    pub fn word_count(&self) -> usize {
        self.tokens.len()
    }

    /// Number of distinct word keys.
    #[must_use]
    pub fn vocabulary_size(&self) -> usize {
        self.vocab.len()
    }

    /// The first token of `page` and the first token after it.
    pub(super) fn page_token_range(&self, page: u32) -> (usize, usize) {
        let i = self
            .page_starts
            .iter()
            .position(|(p, _)| *p == page)
            .unwrap_or(0);
        let start = self.page_starts.get(i).map_or(0, |x| x.1 as usize);
        let end = self
            .page_starts
            .get(i + 1)
            .map_or(self.tokens.len(), |x| x.1 as usize);
        (start, end)
    }
}

/// Which separator wins when several stand between two words (a line break beats a space).
fn sep_rank(s: Sep) -> u8 {
    match s {
        Sep::None => 0,
        Sep::Space => 1,
        Sep::Underscore | Sep::Caret | Sep::Brace | Sep::Hyphen => 2,
        Sep::Other => 3,
        Sep::Break => 4,
    }
}

#[cfg(test)]
pub(super) mod tests {
    use super::*;

    /// Build an index from plain strings, one per page, with synthetic boxes (one per character).
    pub fn index_of(pages: &[&str]) -> SearchIndex {
        let pages: Vec<PageChars> = pages
            .iter()
            .enumerate()
            .map(|(i, s)| {
                let page = u32::try_from(i + 1).unwrap_or(1);
                let mut x = 0.0_f32;
                let chars = s
                    .chars()
                    .map(|c| {
                        x += 5.0;
                        (
                            c,
                            (!c.is_whitespace()).then_some(PageRect {
                                page,
                                x0: x,
                                y0: 10.0,
                                x1: x + 5.0,
                                y1: 20.0,
                            }),
                        )
                    })
                    .collect();
                PageChars { page, chars }
            })
            .collect();
        SearchIndex::build(&pages, Vec::new())
    }

    #[test]
    fn tokens_map_back_to_original_text() {
        let ix = index_of(&["The ﬁnite C_p of 𝐶ₚ, propa\u{2}\ngating"]);
        let words: Vec<String> = ix
            .tokens
            .iter()
            .map(|t| ix.text[t.start..t.end].iter().collect())
            .collect();
        assert_eq!(
            words,
            ["The", "ﬁnite", "C", "p", "of", "𝐶ₚ", "propa\u{2}\ngating"]
        );
        let keys: Vec<&str> = ix
            .tokens
            .iter()
            .map(|t| ix.vocab[t.vocab as usize].key.as_str())
            .collect();
        assert_eq!(keys, ["the", "finite", "c", "p", "of", "cp", "propagating"]);
        assert_eq!(ix.tokens[3].sep, Sep::Underscore);
    }

    #[test]
    fn persian_word_with_zwnj_is_one_token() {
        let ix = index_of(&["شبکه\u{200C}های عصبی"]);
        assert_eq!(ix.word_count(), 2);
    }
}
