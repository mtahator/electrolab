//! Section headings from the text (P1.M4.OBJ.4): the generated outline for PDFs without
//! bookmarks. A heading is a short line that is numbered like a section ("3", "4.2", "II.",
//! "A.") or is one of the usual unnumbered section names, at least as tall as body text, and
//! whose numbers keep going up through the paper.

use std::sync::LazyLock;

use ep_document_contract::{LinkTarget, OutlineItem};
use regex::Regex;

use super::Line;

/// A heading found in the text.
#[derive(Clone, Debug, PartialEq)]
pub struct Heading {
    /// "4.2", "II", "A", or empty for unnumbered names.
    pub number: String,
    /// Heading text without the number.
    pub title: String,
    /// 1 for top level.
    pub depth: u32,
    /// Where it is.
    pub line: Line,
}

static NUMBERED: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^(\d{1,2}(?:\.\d{1,2}){0,3})\.?\s+(\p{Lu}[^\n]{1,90})$")
        .unwrap_or_else(|_| unreachable!())
});
static ROMAN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^([IVX]{1,5})\.\s+(\p{Lu}[^\n]{1,90})$").unwrap_or_else(|_| unreachable!())
});
static LETTER: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^(?:Appendix\s+)?([A-H](?:\.\d{1,2}){0,2})[.:]?\s+(\p{Lu}[^\n]{1,90})$")
        .unwrap_or_else(|_| unreachable!())
});
static NAMED: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)^(abstract|introduction|related work|background|preliminaries|method(?:s|ology)?|experiments?|results(?: and discussion)?|discussion|conclusions?|summary|acknowledge?ments?|references|bibliography|appendix(?: [A-Z])?|supplementary material)$")
        .unwrap_or_else(|_| unreachable!())
});

fn roman_value(s: &str) -> u32 {
    let v = |c| match c {
        'I' => 1,
        'V' => 5,
        'X' => 10,
        _ => 0,
    };
    let cs: Vec<u32> = s.chars().map(v).collect();
    let mut total = 0i64;
    for (i, &c) in cs.iter().enumerate() {
        if cs.get(i + 1).is_some_and(|&n| n > c) {
            total -= i64::from(c);
        } else {
            total += i64::from(c);
        }
    }
    u32::try_from(total).unwrap_or(0)
}

/// Candidate heading in one line, if it looks like one.
fn candidate(line: &Line, body: f32) -> Option<Heading> {
    let t = line.text.trim_end_matches(['.', ':']).trim();
    let words = t.split_whitespace().count();
    if t.chars().count() > 90 || words > 14 || words == 0 {
        return None;
    }
    // Headings are never smaller than body text.
    if body > 0.0 && line.height() < body * 0.92 {
        return None;
    }
    let bad_end = |s: &str| s.ends_with(',') || s.ends_with(';') || s.contains("  ");
    if let Some(c) = NUMBERED.captures(&line.text) {
        let (num, title) = (c[1].to_string(), c[2].trim().to_string());
        // "2020 IEEE …", "1 Introduction" ok; "3 m/s were …" (prose) not: few words, no verb-ish end.
        let top: u32 = num
            .split('.')
            .next()
            .and_then(|n| n.parse().ok())
            .unwrap_or(99);
        if top == 0
            || top > 30
            || bad_end(&title)
            || title.ends_with('.') && title.split_whitespace().count() > 8
        {
            return None;
        }
        let depth = u32::try_from(num.split('.').count()).unwrap_or(1);
        return Some(Heading {
            number: num,
            title,
            depth,
            line: line.clone(),
        });
    }
    if let Some(c) = ROMAN.captures(&line.text) {
        if roman_value(&c[1]) == 0 || bad_end(&c[2]) {
            return None;
        }
        return Some(Heading {
            number: c[1].to_string(),
            title: c[2].trim().to_string(),
            depth: 1,
            line: line.clone(),
        });
    }
    if NAMED.is_match(t) {
        return Some(Heading {
            number: String::new(),
            title: t.to_string(),
            depth: 1,
            line: line.clone(),
        });
    }
    if let Some(c) = LETTER.captures(&line.text) {
        // Only clearly appendix-like: "A Hyperparameters", "Appendix B …", "A.1 Data".
        if line.text.starts_with("Appendix") || c[1].contains('.') || words <= 6 {
            let depth = u32::try_from(c[1].split('.').count()).unwrap_or(1);
            return Some(Heading {
                number: c[1].to_string(),
                title: c[2].trim().to_string(),
                depth,
                line: line.clone(),
            });
        }
    }
    None
}

/// Sort key of a numbered heading: (kind, numbers) so "2.1" < "2.10" < "3".
fn key(h: &Heading) -> Option<Vec<u32>> {
    if h.number.is_empty() {
        return None;
    }
    if h.number.chars().all(|c| matches!(c, 'I' | 'V' | 'X')) {
        return Some(vec![roman_value(&h.number)]);
    }
    let mut out = Vec::new();
    for part in h.number.split('.') {
        if let Ok(n) = part.parse() {
            out.push(n);
        } else if let Some(c) = part.chars().next() {
            out.push(1000 + (c as u32 - 'A' as u32));
        }
    }
    Some(out)
}

/// Headings of a paper, in order. `pages` are the lines of every page.
#[must_use]
pub fn headings(pages: &[Vec<Line>]) -> Vec<Heading> {
    let body = super::body_height(pages);
    let mut cands: Vec<Heading> = Vec::new();
    for lines in pages {
        for (i, l) in lines.iter().enumerate() {
            let Some(mut h) = candidate(l, body) else {
                continue;
            };
            // A heading broken over two lines ("… Physics In-" / "formed Neural Networks").
            if let Some(next) = lines.get(i + 1)
                && (l.text.ends_with('\u{ad}') || l.text.ends_with('-'))
                && next.rect.y0 > l.rect.y0
                && next.rect.y0 - l.rect.y1 < l.height()
                && (next.rect.x0 - l.rect.x0).abs() < 40.0
            {
                h.title = format!(
                    "{}{}",
                    h.title.trim_end_matches(['\u{ad}', '-']),
                    next.text.trim()
                );
                h.line.rect = h.line.rect.union(&next.rect);
            }
            cands.push(h);
        }
    }
    // Numbers must keep going up: drop candidates that would step back (list items, table rows,
    // references "1. Smith …" after the reference heading).
    let mut out: Vec<Heading> = Vec::new();
    let mut last: Option<Vec<u32>> = None;
    let mut in_refs = false;
    for h in cands {
        if in_refs
            && !h.number.starts_with(|c: char| c.is_ascii_uppercase())
            && !h.title.to_lowercase().starts_with("appendix")
        {
            continue;
        }
        // Lettered sections ("A Hyperparameters") are appendices: only after the body's sections.
        let lettered = h.number.starts_with(|c: char| c.is_ascii_uppercase())
            && !h.number.chars().all(|c| matches!(c, 'I' | 'V' | 'X'));
        if lettered && !in_refs && last.is_none() {
            continue;
        }
        if let Some(k) = key(&h) {
            if last.as_ref().is_some_and(|l| k <= *l) {
                continue;
            }
            // A jump of more than one at the top level is a list, not a section.
            if let Some(l) = &last
                && k.len() == 1
                && l.first()
                    .is_some_and(|f| *f < 1000 && k[0] < 1000 && k[0] > f + 2)
            {
                continue;
            }
            last = Some(k);
        }
        let lower = h.title.to_lowercase();
        if lower == "references" || lower == "bibliography" {
            in_refs = true;
        }
        // The same unnumbered name twice (running headers) counts once.
        if h.number.is_empty()
            && out
                .iter()
                .any(|o| o.number.is_empty() && o.title.eq_ignore_ascii_case(&h.title))
        {
            continue;
        }
        out.push(h);
    }
    out
}

/// Headings as an outline tree (depth from the numbering).
#[must_use]
pub fn outline(heads: &[Heading]) -> Vec<OutlineItem> {
    fn item(h: &Heading) -> OutlineItem {
        let title = if h.number.is_empty() {
            h.title.clone()
        } else {
            format!("{} {}", h.number, h.title)
        };
        OutlineItem {
            title,
            target: Some(LinkTarget::Page {
                page: h.line.page,
                top: Some((h.line.rect.y0 - 6.0).max(0.0)),
            }),
            children: Vec::new(),
        }
    }
    let mut root: Vec<OutlineItem> = Vec::new();
    for h in heads {
        let mut level = &mut root;
        for _ in 1..h.depth {
            if level.is_empty() {
                break;
            }
            let last = level.len() - 1;
            level = &mut level[last].children;
        }
        level.push(item(h));
    }
    root
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::super::{lines, testing::page};
    use super::*;

    #[test]
    fn finds_numbered_and_named_headings_in_order() {
        let p1 = page(
            1,
            &[
                ("A Title of the Paper", 100.0, 60.0, 16.0),
                ("Abstract", 72.0, 120.0, 11.0),
                (
                    "This paper presents a long abstract sentence that goes on and on for a while.",
                    72.0,
                    135.0,
                    10.0,
                ),
                ("1 Introduction", 72.0, 200.0, 11.0),
                (
                    "Physics informed neural networks are used here for a reason that is long.",
                    72.0,
                    215.0,
                    10.0,
                ),
                ("2 Related Work", 72.0, 300.0, 11.0),
                ("2.1 Building Control", 72.0, 330.0, 10.5),
                (
                    "1. a list item that is not a heading at all, really not.",
                    72.0,
                    360.0,
                    10.0,
                ),
                ("3 m/s", 72.0, 380.0, 9.0),
            ],
        );
        let p2 = page(
            2,
            &[
                ("3 Method", 72.0, 60.0, 11.0),
                ("References", 72.0, 400.0, 11.0),
                (
                    "1. Smith, J. A paper title. Journal, 2020.",
                    72.0,
                    420.0,
                    10.0,
                ),
                ("A Hyperparameters", 72.0, 600.0, 11.0),
            ],
        );
        let ls = vec![lines(&p1), lines(&p2)];
        let h = headings(&ls);
        let names: Vec<String> = h
            .iter()
            .map(|h| format!("{} {}", h.number, h.title).trim().to_string())
            .collect();
        assert_eq!(
            names,
            vec![
                "Abstract",
                "1 Introduction",
                "2 Related Work",
                "2.1 Building Control",
                "3 Method",
                "References",
                "A Hyperparameters"
            ]
        );
        let o = outline(&h);
        assert_eq!(o.len(), 6, "2.1 nests under 2");
        assert_eq!(o[2].children.len(), 1);
    }

    #[test]
    fn wrapped_heading_is_joined() {
        let p = page(
            1,
            &[
                ("1 Introduction", 72.0, 60.0, 11.0),
                ("A Hyperparameters for Physics In\u{ad}", 72.0, 100.0, 11.0),
                ("formed Neural Networks", 72.0, 113.0, 11.0),
            ],
        );
        let h = headings(&[lines(&p)]);
        assert_eq!(
            h[1].title,
            "Hyperparameters for Physics Informed Neural Networks"
        );
    }

    #[test]
    fn roman_numerals() {
        assert_eq!(roman_value("IV"), 4);
        assert_eq!(roman_value("XII"), 12);
        let p = page(
            1,
            &[
                ("I. INTRODUCTION", 72.0, 100.0, 11.0),
                ("II. RELATED WORK", 72.0, 300.0, 11.0),
            ],
        );
        let h = headings(&[lines(&p)]);
        assert_eq!(h.len(), 2);
        assert_eq!(h[1].number, "II");
    }
}
