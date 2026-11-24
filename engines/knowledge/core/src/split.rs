//! A document as sections: by its headings where it has them, else by pages; not too long, not too
//! short, not too many.

use ep_knowledge_contract::Role;

use crate::{head, squash};

/// Longest section given to a model (characters), shortest kept on its own, and most sections.
pub const LONG: usize = 4200;
const SHORT: usize = 180;
/// With many headings, sections shorter than this are joined.
const SHORT_OF_MANY: usize = 600;
const MANY: usize = 30;
const MOST: usize = 80;
const BY_PAGES: usize = 3500;

/// A heading of the document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Heading {
    pub title: String,
    pub level: u32,
    /// 1-based page.
    pub page: u32,
}

/// A piece of the document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Piece {
    pub title: String,
    pub level: u32,
    /// `(page, text)`, in order.
    pub parts: Vec<(u32, String)>,
}

impl Piece {
    /// Characters of text.
    #[must_use]
    pub fn len(&self) -> usize {
        self.parts.iter().map(|(_, t)| t.chars().count()).sum()
    }
    /// No text.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
    /// All its text.
    #[must_use]
    pub fn text(&self) -> String {
        self.parts
            .iter()
            .map(|(_, t)| t.as_str())
            .collect::<Vec<_>>()
            .join(" ")
    }
    /// First and last page.
    #[must_use]
    pub fn pages(&self) -> (u32, u32) {
        (
            self.parts.first().map_or(1, |p| p.0),
            self.parts.last().map_or(1, |p| p.0),
        )
    }
    /// Its sentences with their page.
    #[must_use]
    pub fn sentences(&self) -> Vec<(u32, String)> {
        self.parts
            .iter()
            .flat_map(|(p, t)| sentences(t).into_iter().map(|s| (*p, s)))
            .collect()
    }
}

/// Sentences of a text. Not cut after "Fig.", "e.g.", initials or inside numbers.
#[must_use]
pub fn sentences(text: &str) -> Vec<String> {
    const NOT_END: [&str; 16] = [
        "fig", "figs", "eq", "eqs", "ref", "refs", "sec", "e.g", "i.e", "et al", "al", "vs", "cf",
        "no", "dr", "approx",
    ];
    let t = squash(text);
    let chars: Vec<char> = t.chars().collect();
    let (mut out, mut start) = (vec![], 0);
    for i in 0..chars.len() {
        if !matches!(chars[i], '.' | '!' | '?') {
            continue;
        }
        let next_ok = chars.get(i + 1).is_none_or(|c| c.is_whitespace())
            && chars.get(i + 2).is_none_or(|c| {
                c.is_uppercase() || c.is_ascii_digit() || *c == '(' || !c.is_ascii()
            });
        let before: String = chars[start..i].iter().collect();
        let last = before
            .rsplit([' ', '('])
            .next()
            .unwrap_or("")
            .to_lowercase();
        let short = last.chars().count() == 1 && last.chars().all(char::is_alphabetic);
        if next_ok && !short && !NOT_END.contains(&last.as_str()) {
            let s: String = chars[start..=i].iter().collect();
            if !s.trim().is_empty() {
                out.push(s.trim().to_string());
            }
            start = i + 1;
        }
    }
    let rest: String = chars[start..].iter().collect();
    if !rest.trim().is_empty() {
        out.push(rest.trim().to_string());
    }
    out
}

/// What a heading says its section does.
#[must_use]
pub fn role_of(title: &str) -> Option<Role> {
    let t = title.to_lowercase();
    let has = |w: &[&str]| w.iter().any(|x| t.contains(x));
    Some(
        if has(&["reference", "bibliograph", "works cited", "further reading"]) {
            Role::References
        } else if has(&["appendix", "annex", "supplement"]) {
            Role::Appendix
        } else if has(&["introduction", "overview", "motivation"]) {
            Role::Introduction
        } else if has(&["abstract", "summary", "conclusion", "recommendation"]) {
            Role::Conclusion
        } else if has(&["literature", "related work", "background", "theory"]) {
            Role::Background
        } else if has(&[
            "method",
            "model",
            "approach",
            "experimental setup",
            "procedure",
        ]) {
            Role::Method
        } else if has(&["result", "findings", "evaluation"]) {
            Role::Results
        } else if has(&["discussion", "limitation"]) {
            Role::Discussion
        } else if has(&["warranty", "licen", "terms", "legal", "copyright"]) {
            Role::Legal
        } else if has(&[
            "safety",
            "instruction",
            "getting started",
            "installation",
            "troubleshoot",
            "cleaning",
        ]) {
            Role::Instructions
        } else if has(&[
            "acknowledg",
            "contents",
            "declaration",
            "nomenclature",
            "list of",
        ]) {
            Role::Administrative
        } else {
            return None;
        },
    )
}

/// Where a heading's words stand in a page (first byte, byte after), comparing letters and digits
/// only.
fn find_heading(page: &str, title: &str) -> Option<(usize, usize)> {
    let fold = |c: char| c.to_lowercase().next().unwrap_or(c);
    let want: Vec<char> = title
        .chars()
        .filter(|c| c.is_alphanumeric())
        .map(fold)
        .collect();
    if want.is_empty() {
        return None;
    }
    let letters: Vec<(usize, char)> = page
        .char_indices()
        .filter(|(_, c)| c.is_alphanumeric())
        .map(|(i, c)| (i, fold(c)))
        .collect();
    letters
        .windows(want.len())
        .find(|w| w.iter().map(|(_, c)| c).eq(want.iter()))
        .map(|w| {
            let last = w[w.len() - 1];
            (w[0].0, last.0 + last.1.len_utf8())
        })
}

/// Roles of pieces in order: by their heading, else by the heading above, else by their text.
#[must_use]
pub fn roles(pieces: &[Piece]) -> Vec<Option<Role>> {
    let mut above: Vec<(u32, Option<Role>)> = vec![];
    pieces
        .iter()
        .map(|p| {
            while above.last().is_some_and(|(l, _)| *l >= p.level) {
                above.pop();
            }
            let own = looks_like_references(&p.text())
                .then_some(Role::References)
                .or_else(|| role_of(&p.title));
            let role = own.or_else(|| above.iter().rev().find_map(|(_, r)| *r));
            above.push((p.level, role));
            role
        })
        .collect()
}

/// A list of references without a heading: many web addresses, DOIs and years for its length.
fn looks_like_references(text: &str) -> bool {
    let n = text.split_whitespace().count();
    let marks = [
        "http",
        "doi",
        "Available at",
        "Accessed",
        "et al",
        "pp.",
        "vol.",
    ]
    .iter()
    .map(|m| text.matches(m).count())
    .sum::<usize>();
    n > 60 && marks * 18 > n
}

/// Cut a document (text of each page, in order) into pieces.
#[must_use]
pub fn split(pages: &[String], headings: &[Heading]) -> Vec<Piece> {
    let mut heads: Vec<Heading> = headings
        .iter()
        .filter(|h| h.page >= 1 && h.page as usize <= pages.len() && !h.title.trim().is_empty())
        .cloned()
        .collect();
    // Too many: the deepest ones are left out first.
    while heads.len() > MOST {
        let deepest = heads.iter().map(|h| h.level).max().unwrap_or(1);
        if deepest <= 1 {
            break;
        }
        heads.retain(|h| h.level < deepest);
    }
    let pieces = if heads.len() < 2 {
        by_pages(pages)
    } else {
        by_headings(pages, &heads)
    };
    tidy(pieces)
}

fn by_pages(pages: &[String]) -> Vec<Piece> {
    let mut out: Vec<Piece> = vec![];
    let total: usize = pages.iter().map(|p| p.chars().count()).sum();
    let size = BY_PAGES.max(total / MOST + 1);
    for (i, t) in pages.iter().enumerate() {
        let part = (u32::try_from(i + 1).unwrap_or(u32::MAX), squash(t));
        match out.last_mut() {
            Some(p) if p.len() + part.1.chars().count() <= size => p.parts.push(part),
            _ => out.push(Piece {
                title: String::new(),
                level: 1,
                parts: vec![part],
            }),
        }
    }
    for p in &mut out {
        let (a, b) = p.pages();
        p.title = if a == b {
            format!("Page {a}")
        } else {
            format!("Pages {a}–{b}")
        };
    }
    out
}

fn by_headings(pages: &[String], heads: &[Heading]) -> Vec<Piece> {
    // Start of each heading: page and byte.
    let mut starts: Vec<(usize, usize, usize, &Heading)> = vec![];
    for h in heads {
        let p = h.page as usize - 1;
        let (at, end) = find_heading(&pages[p], &h.title).unwrap_or((0, 0));
        starts.push((p, at, end, h));
    }
    starts.sort_by_key(|(p, at, ..)| (*p, *at));
    let mut out = vec![];
    let cut = |from: (usize, usize), to: (usize, usize)| -> Vec<(u32, String)> {
        let last = to.0.min(pages.len() - 1);
        (from.0..=last)
            .filter_map(|p| {
                let a = if p == from.0 { from.1 } else { 0 };
                let b = if p == to.0 { to.1 } else { pages[p].len() };
                let t = squash(pages[p].get(a..b.max(a)).unwrap_or(""));
                (!t.is_empty()).then(|| (u32::try_from(p + 1).unwrap_or(u32::MAX), t))
            })
            .collect()
    };
    let first = (starts[0].0, starts[0].1);
    let before = cut((0, 0), first);
    if !before.is_empty() {
        out.push(Piece {
            title: "Start of the document".into(),
            level: 1,
            parts: before,
        });
    }
    for (i, (p, _, end, h)) in starts.iter().enumerate() {
        let to = starts.get(i + 1).map_or((pages.len(), 0), |n| (n.0, n.1));
        out.push(Piece {
            title: squash(&h.title),
            level: h.level.max(1),
            parts: cut((*p, *end), to),
        });
    }
    out
}

/// Join short pieces to the next one, cut long ones, drop empty ones.
fn tidy(pieces: Vec<Piece>) -> Vec<Piece> {
    let mut joined: Vec<Piece> = vec![];
    let mut carry: Option<Piece> = None;
    let short = if pieces.len() > MANY {
        SHORT_OF_MANY
    } else {
        SHORT
    };
    for mut p in pieces {
        if let Some(c) = carry.take() {
            let mut parts = c.parts;
            parts.append(&mut p.parts);
            p = Piece {
                title: format!("{} · {}", c.title, p.title),
                level: c.level.min(p.level),
                parts,
            };
        }
        if p.len() < short && role_of(&p.title) != Some(Role::References) {
            carry = Some(p);
        } else {
            joined.push(p);
        }
    }
    if let Some(c) = carry {
        match joined.last_mut() {
            Some(l) => l.parts.extend(c.parts),
            None => joined.push(c),
        }
    }
    let mut out = vec![];
    for p in joined.into_iter().filter(|p| !p.is_empty()) {
        if p.len() <= LONG || role_of(&p.title) == Some(Role::References) {
            out.push(p);
            continue;
        }
        let mut part = Piece {
            title: p.title.clone(),
            level: p.level,
            parts: vec![],
        };
        let mut n = 1;
        for (page, s) in p.sentences() {
            if !part.is_empty() && part.len() + s.chars().count() > LONG {
                n += 1;
                let next = Piece {
                    title: format!("{} (part {n})", p.title),
                    level: p.level,
                    parts: vec![],
                };
                out.push(std::mem::replace(&mut part, next));
            }
            let s = head(&s, LONG).to_string();
            match part.parts.last_mut() {
                Some((pg, t)) if *pg == page => {
                    t.push(' ');
                    t.push_str(&s);
                }
                _ => part.parts.push((page, s)),
            }
        }
        out.push(part);
    }
    out.truncate(MOST * 2);
    out
}

/// Pages without their running heads and feet: a first or last line that stands, but for its
/// numbers, on at least three pages and on a third of them.
#[must_use]
pub fn without_running(pages: &[String]) -> Vec<String> {
    let key = |l: &str| -> String { l.chars().filter(|c| c.is_alphabetic()).collect() };
    let edges = |p: &str| -> Vec<usize> {
        let full: Vec<usize> = p
            .lines()
            .enumerate()
            .filter(|(_, l)| !l.trim().is_empty())
            .map(|(i, _)| i)
            .collect();
        let mut e: Vec<usize> = full
            .iter()
            .take(2)
            .chain(full.iter().rev().take(2))
            .copied()
            .collect();
        e.sort_unstable();
        e.dedup();
        e
    };
    let mut count: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    for p in pages {
        let lines: Vec<&str> = p.lines().collect();
        let here: std::collections::HashSet<String> = edges(p)
            .into_iter()
            .map(|i| key(lines[i]))
            .filter(|k| k.chars().count() > 3)
            .collect();
        for k in here {
            *count.entry(k).or_insert(0) += 1;
        }
    }
    let least = 3.max(pages.len().div_ceil(3));
    pages
        .iter()
        .map(|p| {
            let edge = edges(p);
            p.lines()
                .enumerate()
                .filter(|(i, l)| {
                    !(edge.contains(i) && count.get(&key(l)).is_some_and(|n| *n >= least))
                })
                .map(|(_, l)| l)
                .collect::<Vec<_>>()
                .join("\n")
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn h(title: &str, level: u32, page: u32) -> Heading {
        Heading {
            title: title.into(),
            level,
            page,
        }
    }

    #[test]
    fn running_heads_and_feet_are_left_out() {
        let words = ["burrs", "hopper", "timer", "dial"];
        let pages: Vec<String> = words
            .iter()
            .enumerate()
            .map(|(n, w)| format!("Coffee Grinder\nAbout the {w}\nIt has {n} parts.\nClean the {w}.\nUser Manual — Page {n}"))
            .collect();
        let p = without_running(&pages);
        assert!(
            p.iter()
                .all(|x| !x.contains("Manual") && !x.contains("Coffee Grinder")),
            "{p:?}"
        );
        assert_eq!(p[2], "About the timer\nIt has 2 parts.\nClean the timer.");
        // Two pages say nothing about what is repeated.
        assert_eq!(without_running(&pages[..2]), pages[..2]);
    }

    #[test]
    fn sections_follow_the_headings_with_their_pages() {
        let long = "This sentence fills the section with words. ".repeat(12);
        let pages = vec![
            format!("My Title 1 Intro-\nduction {long}"),
            format!("{long} 2 Methods {long}"),
            format!("{long} References [1] Someone."),
        ];
        let p = split(
            &pages,
            &[
                h("1 Introduction", 1, 1),
                h("2 Methods", 1, 2),
                h("References", 1, 3),
            ],
        );
        let seen: Vec<(&str, (u32, u32))> =
            p.iter().map(|x| (x.title.as_str(), x.pages())).collect();
        assert_eq!(
            seen,
            [
                ("Start of the document · 1 Introduction", (1, 2)),
                ("2 Methods", (2, 3)),
                ("References", (3, 3))
            ]
        );
        assert!(
            p[1].text().starts_with("This sentence") && !p[1].text().contains("[1]"),
            "the heading is not part of the text"
        );
    }

    #[test]
    fn without_headings_pages_are_taken_together() {
        let pages: Vec<String> = (0..10)
            .map(|i| format!("Page text {i}. ").repeat(60))
            .collect();
        let p = split(&pages, &[]);
        assert!(p.len() > 1 && p.iter().all(|x| x.len() <= LONG));
        assert_eq!(p[0].title, "Pages 1–4");
        assert_eq!(p.last().unwrap().pages().1, 10, "nothing is left out");
    }

    #[test]
    fn long_sections_are_cut_at_sentences_and_many_headings_are_joined() {
        let pages = vec!["A short sentence here. ".repeat(900)];
        let p = split(&pages, &[h("A short", 1, 1), h("zzz", 1, 1)]);
        assert!(p.len() > 3 && p.iter().all(|x| x.len() <= LONG));
        assert!(p.iter().all(|x| x.text().ends_with('.')));
        let pages: Vec<String> = (0..300)
            .map(|i| format!("Head {i} {}", "word and more words. ".repeat(30)))
            .collect();
        let heads: Vec<Heading> = (0..300u32)
            .map(|i| h(&format!("Head {i}"), if i % 10 == 0 { 1 } else { 2 }, i + 1))
            .collect();
        assert!(split(&pages, &heads).len() <= MOST * 2);
    }

    #[test]
    fn sentences_are_not_cut_in_abbreviations_and_numbers() {
        let s = sentences(
            "See Fig. 3 for the NPV of 3.5 million. Wood, e.g. chips, is cheap! J. Smith agrees. Is it?",
        );
        assert_eq!(s.len(), 4, "{s:?}");
        assert!(s[0].ends_with("3.5 million."));
    }

    #[test]
    fn a_section_without_a_telling_heading_takes_the_role_above_it() {
        let piece = |title: &str, level, text: &str| Piece {
            title: title.into(),
            level,
            parts: vec![(1, text.into())],
        };
        let refs =
            "Smith, J. (2020) Wind. Available at: https://x.org (Accessed: 1 May). ".repeat(12);
        let r = roles(&[
            piece("4 Model(s)", 1, "a"),
            piece("4.2 Roof PV", 2, "b"),
            piece("5 Wood", 1, "c"),
            piece("Recommendations (part 2)", 1, &refs),
        ]);
        assert_eq!(
            r,
            [
                Some(Role::Method),
                Some(Role::Method),
                None,
                Some(Role::References)
            ]
        );
    }

    #[test]
    fn headings_say_what_a_section_does() {
        assert_eq!(role_of("3.2 Results and discussion"), Some(Role::Results));
        assert_eq!(role_of("Bibliography"), Some(Role::References));
        assert_eq!(role_of("Wood chips"), None);
    }
}
