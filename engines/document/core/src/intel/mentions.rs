//! Mentions and citation markers in the text (P1.M4.LNK.1–2), deterministic:
//! "Fig. 3", "Figure 3(b)", "Figs. 2–4", "Figures 2 and 5", "Table 2", "Eq. (5)", "Eqs. (5)–(7)",
//! "Section 4.2", "شکل ۳"; "[12]", "[3–5, 9]", "(Smith et al., 2020)", "Smith and Lee (2019)".
//! A caption's own label ("Figure 3:" starting a line) is not a mention.

use std::sync::LazyLock;

use ep_document_contract::{CitationMarker, Mention, MentionKind, PageText, Reference};
use regex::Regex;

use super::{ascii_digits, quads};

fn re(s: &str) -> Regex {
    Regex::new(s).unwrap_or_else(|_| unreachable!())
}

const NUM: &str = r"\(?[0-9\u{6f0}-\u{6f9}]{1,3}(?:\.[0-9]{1,2}){0,2}\)?";

static MENTION: LazyLock<Regex> = LazyLock::new(|| {
    re(&format!(
        r"(?i)\b(figs?\.|figures?|tables?|tabs?\.|eqs?\.|equations?|sec(?:tion)?s?\.?|§)\s?({NUM})((?:\([a-z]\)|[a-z]\b)?(?:\s*(?:[-–—]|to|,|and|&)\s*{NUM}(?:\([a-z]\)|[a-z]\b)?)*)|(شکل|جدول|معادله)\s*({NUM})"
    ))
});
static NUMS: LazyLock<Regex> =
    LazyLock::new(|| re(r"[0-9\u{6f0}-\u{6f9}]{1,3}(?:\.[0-9]{1,2}){0,2}"));
static RANGE: LazyLock<Regex> = LazyLock::new(|| re(r"[-–—]|\bto\b"));

fn kind_of(word: &str) -> Option<MentionKind> {
    let w = word.to_lowercase();
    Some(if w.starts_with("fig") || w == "شکل" {
        MentionKind::Figure
    } else if w.starts_with("tab") || w == "جدول" {
        MentionKind::Table
    } else if w.starts_with("eq") || w == "معادله" {
        MentionKind::Equation
    } else if w.starts_with("sec") || w == "§" {
        MentionKind::Section
    } else {
        return None;
    })
}

/// Numbers in a mention, ranges of whole numbers expanded ("2–4" → 2, 3, 4).
fn numbers(first: &str, rest: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let all = format!("{first}{rest}");
    let found: Vec<(usize, usize, String)> = NUMS
        .find_iter(&all)
        .map(|m| (m.start(), m.end(), ascii_digits(m.as_str())))
        .collect();
    for (i, (pos, _, n)) in found.iter().enumerate() {
        if i > 0 {
            let between = &all[found[i - 1].1..*pos];
            if RANGE.is_match(between)
                && let (Ok(a), Ok(b)) = (found[i - 1].2.parse::<u32>(), n.parse::<u32>())
                && b > a
                && b - a <= 20
            {
                for k in a + 1..b {
                    out.push(k.to_string());
                }
            }
        }
        if !out.contains(n) {
            out.push(n.clone());
        }
    }
    out
}

/// Byte offset → character index in `s`.
fn char_index(s: &str, byte: usize) -> u32 {
    u32::try_from(s[..byte].chars().count()).unwrap_or(u32::MAX)
}

/// Mentions on a page (targets are filled in by the caller, which knows the objects).
#[must_use]
pub fn mentions(pt: &PageText) -> Vec<Mention> {
    let text = &pt.text;
    let mut out = Vec::new();
    for c in MENTION.captures_iter(text) {
        let Some(m) = c.get(0) else { continue };
        let (word, first, rest) = match (c.get(1), c.get(2), c.get(4), c.get(5)) {
            (Some(w), Some(n), _, _) => {
                (w.as_str(), n.as_str(), c.get(3).map_or("", |r| r.as_str()))
            }
            (_, _, Some(w), Some(n)) => (w.as_str(), n.as_str(), ""),
            _ => continue,
        };
        let Some(kind) = kind_of(word) else { continue };
        // A caption's own label: at the start of a line and followed by ':' or '.'.
        let line_start = text[..m.start()].rfind('\n').map_or(0, |i| i + 1);
        let after = text[m.end()..].chars().next().unwrap_or(' ');
        if text[line_start..m.start()].trim().is_empty() && matches!(after, ':' | '.' | '|') {
            continue;
        }
        // "Section" needs a dotted or small number; plain words like "table 1000" are skipped.
        let nums = numbers(first, rest);
        if nums.is_empty() {
            continue;
        }
        let matched = m.as_str().trim_end_matches([',', ' ']);
        let start = char_index(text, m.start());
        let end = start + u32::try_from(matched.chars().count()).unwrap_or(0);
        out.push(Mention {
            kind,
            text: matched.replace('\n', " "),
            numbers: nums,
            quads: quads(pt, start, end),
            target: None,
        });
    }
    out
}

static BRACKETS: LazyLock<Regex> =
    LazyLock::new(|| re(r"\[(\s*\d{1,3}(?:\s*[-–,;]\s*\d{1,3})*\s*)\]"));
static AUTHOR_YEAR: LazyLock<Regex> = LazyLock::new(|| {
    re(
        r"\(((?:see\s+|e\.g\.,?\s+)?\p{Lu}[\p{L}'’\-]+(?:\s+(?:et\s+al\.?|and|&)\s*(?:\p{Lu}[\p{L}'’\-]+)?)?,?\s+(?:19|20)\d{2}[a-z]?(?:\s*;\s*\p{Lu}[\p{L}'’\-]+(?:\s+(?:et\s+al\.?|and|&)\s*(?:\p{Lu}[\p{L}'’\-]+)?)?,?\s+(?:19|20)\d{2}[a-z]?)*)\)",
    )
});
static NARRATIVE: LazyLock<Regex> = LazyLock::new(|| {
    re(
        r"\b(\p{Lu}[\p{L}'’\-]+(?:\s+(?:et\s+al\.?|and|&)\s*(?:\p{Lu}[\p{L}'’\-]+)?)?)\s+\(((?:19|20)\d{2}[a-z]?)\)",
    )
});

/// Expand "3–5, 9" into [3, 4, 5, 9].
fn bracket_numbers(s: &str) -> Vec<u32> {
    let mut out = Vec::new();
    for part in s.split([',', ';']) {
        let bounds: Vec<u32> = part
            .split(['-', '–'])
            .filter_map(|x| x.trim().parse().ok())
            .collect();
        match bounds.as_slice() {
            [a] => out.push(*a),
            [a, b] if b >= a && b - a <= 50 => out.extend(*a..=*b),
            _ => {}
        }
    }
    out
}

/// Entries whose first author's surname and year match.
fn by_author_year(refs: &[Reference], surname: &str, year: &str) -> Option<u32> {
    let s = surname.to_lowercase();
    let y = year.trim_end_matches(|c: char| c.is_ascii_lowercase());
    refs.iter()
        .find(|r| {
            r.year.as_deref() == Some(y)
                && (r
                    .authors
                    .first()
                    .is_some_and(|a| a.to_lowercase().contains(&s))
                    || r.text.to_lowercase().starts_with(&s))
        })
        .map(|r| r.index)
}

/// Citation markers on a page, linked to the reference list.
#[must_use]
pub fn citation_markers(pt: &PageText, refs: &[Reference]) -> Vec<CitationMarker> {
    let text = &pt.text;
    let mut out = Vec::new();
    let mut push = |start_b: usize, end_b: usize, entries: Vec<u32>| {
        if entries.is_empty() {
            return;
        }
        let start = char_index(text, start_b);
        let end = char_index(text, end_b);
        out.push(CitationMarker {
            text: text[start_b..end_b].replace('\n', " "),
            quads: quads(pt, start, end),
            entries,
        });
    };
    let labelled = refs.iter().any(|r| r.label.is_some());
    for c in BRACKETS.captures_iter(text) {
        let Some(m) = c.get(0) else { continue };
        let entries: Vec<u32> = bracket_numbers(&c[1])
            .into_iter()
            .filter_map(|n| {
                let label = n.to_string();
                if labelled {
                    refs.iter()
                        .find(|r| r.label.as_deref() == Some(label.as_str()))
                        .map(|r| r.index)
                } else {
                    // Without labels, [n] is the n-th entry.
                    (n >= 1 && (n as usize) <= refs.len()).then_some(n - 1)
                }
            })
            .collect();
        push(m.start(), m.end(), entries);
    }
    for c in AUTHOR_YEAR.captures_iter(text) {
        let Some(m) = c.get(0) else { continue };
        let entries: Vec<u32> = c[1]
            .split(';')
            .filter_map(|one| {
                let one = one
                    .trim()
                    .trim_start_matches("see ")
                    .trim_start_matches("e.g.,")
                    .trim_start_matches("e.g.")
                    .trim();
                let surname = one.split([' ', ',']).next()?;
                let year = YEAR_IN.find(one)?.as_str();
                by_author_year(refs, surname, year)
            })
            .collect();
        push(m.start(), m.end(), entries);
    }
    for c in NARRATIVE.captures_iter(text) {
        let Some(m) = c.get(0) else { continue };
        let surname = c[1].split_whitespace().next().unwrap_or("");
        if let Some(i) = by_author_year(refs, surname, &c[2]) {
            push(m.start(), m.end(), vec![i]);
        }
    }
    out
}

static YEAR_IN: LazyLock<Regex> = LazyLock::new(|| re(r"(?:19|20)\d{2}[a-z]?"));

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::super::testing::page;
    use super::*;
    use ep_types::PageRect;

    #[test]
    fn mentions_of_all_kinds() {
        let p = page(
            2,
            &[
                ("Figure 3: The caption itself.", 72.0, 300.0, 10.0),
                (
                    "As Fig. 3(b) and Figs. 2–4 show, Table 2 and Eq. (5) agree; see Section 4.2.",
                    72.0,
                    100.0,
                    10.0,
                ),
                (
                    "Equations (6) to (8) follow, as in Tables 1 and 3.",
                    72.0,
                    120.0,
                    10.0,
                ),
                ("شکل ۳ را ببینید", 72.0, 140.0, 10.0),
            ],
        );
        let m = mentions(&p);
        let got: Vec<(MentionKind, Vec<String>)> =
            m.iter().map(|x| (x.kind, x.numbers.clone())).collect();
        assert_eq!(
            got,
            vec![
                (MentionKind::Figure, vec!["3".into()]),
                (
                    MentionKind::Figure,
                    vec!["2".into(), "3".into(), "4".into()]
                ),
                (MentionKind::Table, vec!["2".into()]),
                (MentionKind::Equation, vec!["5".into()]),
                (MentionKind::Section, vec!["4.2".into()]),
                (
                    MentionKind::Equation,
                    vec!["6".into(), "7".into(), "8".into()]
                ),
                (MentionKind::Table, vec!["1".into(), "3".into()]),
                (MentionKind::Figure, vec!["3".into()]),
            ]
        );
        assert!(m.iter().all(|x| !x.quads.is_empty()));
        assert!(m[0].text.starts_with("Fig. 3"));
    }

    fn reference(index: u32, label: Option<&str>, author: &str, year: &str) -> Reference {
        Reference {
            index,
            label: label.map(str::to_string),
            text: format!("{author}, X. ({year}). A title."),
            authors: vec![format!("{author}, X.")],
            year: Some(year.into()),
            title: None,
            doi: None,
            arxiv: None,
            url: None,
            page: 9,
            rect: PageRect {
                page: 9,
                x0: 0.0,
                y0: 0.0,
                x1: 1.0,
                y1: 1.0,
            },
        }
    }

    #[test]
    fn numeric_and_author_year_markers() {
        let refs: Vec<Reference> = (0..10)
            .map(|i| reference(i, Some(&(i + 1).to_string()), "Smith", "2020"))
            .collect();
        let p = page(1, &[("as shown [3–5, 9] and in [2].", 72.0, 100.0, 10.0)]);
        let m = citation_markers(&p, &refs);
        assert_eq!(m.len(), 2);
        assert_eq!(m[0].entries, vec![2, 3, 4, 8]);
        assert_eq!(m[1].entries, vec![1]);

        let refs = vec![
            reference(0, None, "Smith", "2020"),
            reference(1, None, "Lee", "2019"),
        ];
        let p = page(
            1,
            &[(
                "This holds (Smith et al., 2020; Lee and Kim, 2019) and Lee (2019) agrees.",
                72.0,
                100.0,
                10.0,
            )],
        );
        let m = citation_markers(&p, &refs);
        assert_eq!(m.len(), 2, "{m:#?}");
        assert_eq!(m[0].entries, vec![0, 1]);
        assert_eq!(m[1].entries, vec![1]);
    }
}
