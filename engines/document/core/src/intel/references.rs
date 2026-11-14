//! The reference list (P1.M4.TAB.3): found after a "References" heading, split into entries by
//! their printed labels ("[12]", "12.") or by hanging indent, and parsed for authors, year, title,
//! DOI, arXiv id and web address.

use std::sync::LazyLock;

use ep_document_contract::Reference;
use ep_types::PageRect;
use regex::Regex;

use super::Line;

fn re(s: &str) -> Regex {
    Regex::new(s).unwrap_or_else(|_| unreachable!())
}

static HEADING: LazyLock<Regex> = LazyLock::new(|| {
    re(
        r"(?i)^(?:\d+\.?\s+|[IVX]+\.\s+)?(references|bibliography|literature cited|works cited|reference list)$",
    )
});
static END: LazyLock<Regex> =
    LazyLock::new(|| re(r"^(?:Appendix\b|APPENDIX\b|Supplementary|[A-H]\.?\s+\p{Lu}[\w ]{2,60}$)"));
static BRACKET: LazyLock<Regex> = LazyLock::new(|| re(r"^\[([^\]]{1,12})\]\s*"));
static NUMBERED: LazyLock<Regex> = LazyLock::new(|| re(r"^(\d{1,3})\.\s+"));
static YEAR: LazyLock<Regex> = LazyLock::new(|| re(r"\b((?:19|20)\d{2})[a-z]?\b"));
static DOI: LazyLock<Regex> = LazyLock::new(|| re(r"(?i)\b(10\.\d{4,9}/[^\s,;]+)"));
static ARXIV: LazyLock<Regex> =
    LazyLock::new(|| re(r"(?i)arxiv[:\s]*(?:abs/)?(\d{4}\.\d{4,5}(?:v\d+)?)"));
static URL: LazyLock<Regex> = LazyLock::new(|| re(r"https?://[^\s]+"));
static QUOTED: LazyLock<Regex> = LazyLock::new(|| re(r#"[“"]([^”"]{8,})[”"]"#));
static SURNAME_FIRST: LazyLock<Regex> = LazyLock::new(|| re(r"^\p{Lu}[\p{L}'’\-]+,\s+\p{Lu}"));

/// Where the reference list starts: the line after its heading.
fn start(pages: &[Vec<Line>]) -> Option<(usize, usize)> {
    let mut found = None;
    for (pi, lines) in pages.iter().enumerate() {
        for (li, l) in lines.iter().enumerate() {
            if HEADING.is_match(l.text.trim_end_matches(':').trim()) {
                // The last such heading wins (a table of contents may name it earlier).
                found = Some((pi, li + 1));
            }
        }
    }
    found
}

/// Lines of the reference list, in order, until the appendix.
fn list_lines(pages: &[Vec<Line>]) -> Vec<Line> {
    let Some((p0, l0)) = start(pages) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for (pi, lines) in pages.iter().enumerate().skip(p0) {
        let from = if pi == p0 { l0 } else { 0 };
        for l in &lines[from.min(lines.len())..] {
            if END.is_match(&l.text) && !out.is_empty() {
                return out;
            }
            out.push(l.clone());
        }
    }
    out
}

/// How entries begin.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Style {
    Bracket,
    Numbered,
    Hanging,
}

fn style(lines: &[Line]) -> Style {
    let n = lines.len().max(1);
    let bracket = lines.iter().filter(|l| BRACKET.is_match(&l.text)).count();
    let numbered = lines.iter().filter(|l| NUMBERED.is_match(&l.text)).count();
    if bracket * 6 >= n {
        Style::Bracket
    } else if numbered * 6 >= n {
        Style::Numbered
    } else {
        Style::Hanging
    }
}

/// Whether a line starts a new entry.
fn starts(style: Style, l: &Line, prev: Option<&Line>, left: f32) -> bool {
    match style {
        Style::Bracket => BRACKET.is_match(&l.text),
        Style::Numbered => NUMBERED.is_match(&l.text),
        Style::Hanging => {
            let Some(p) = prev else { return true };
            // A new column or page, or back at the left edge after an indented line.
            if l.page != p.page || l.rect.y0 < p.rect.y0 - 1.0 {
                return SURNAME_FIRST.is_match(&l.text) || l.rect.x0 <= left + 2.0;
            }
            let outdent = l.rect.x0 < p.rect.x0 - 3.0;
            let at_left = (l.rect.x0 - left).abs() < 2.5;
            outdent
                || (at_left && p.text.trim_end().ends_with('.') && SURNAME_FIRST.is_match(&l.text))
        }
    }
}

/// Parse the reference list of a paper. `pages` are the lines of every page.
#[must_use]
pub fn references(pages: &[Vec<Line>]) -> Vec<Reference> {
    let lines = list_lines(pages);
    if lines.is_empty() {
        return Vec::new();
    }
    let st = style(&lines);
    let mut groups: Vec<Vec<Line>> = Vec::new();
    for (i, l) in lines.iter().enumerate() {
        // Left edge of the column this line is in (lines in the same column share it).
        let left = lines
            .iter()
            .filter(|o| o.page == l.page && (o.rect.x0 - l.rect.x0).abs() < 40.0)
            .map(|o| o.rect.x0)
            .fold(f32::INFINITY, f32::min);
        let prev = if i > 0 { lines.get(i - 1) } else { None };
        if groups.is_empty() || starts(st, l, prev, left) {
            groups.push(vec![l.clone()]);
        } else if let Some(g) = groups.last_mut() {
            g.push(l.clone());
        }
    }
    groups
        .into_iter()
        .filter(|g| g.iter().map(|l| l.text.len()).sum::<usize>() > 15)
        .enumerate()
        .map(|(i, g)| entry(u32::try_from(i).unwrap_or(u32::MAX), &g, st))
        .collect()
}

fn join(lines: &[Line]) -> String {
    let mut out = String::new();
    for l in lines {
        let t = l.text.trim();
        if out.ends_with('\u{ad}')
            || out.ends_with('-') && !out.ends_with(" -") && t.starts_with(char::is_lowercase)
        {
            out.pop();
        } else if !out.is_empty() {
            out.push(' ');
        }
        out.push_str(t);
    }
    out.replace('\u{ad}', "")
}

fn entry(index: u32, g: &[Line], st: Style) -> Reference {
    let mut text = join(g);
    let mut label = None;
    if st == Style::Bracket
        && let Some(c) = BRACKET.captures(&text)
    {
        label = Some(c[1].trim().to_string());
        text = text[c[0].len()..].to_string();
    } else if st == Style::Numbered
        && let Some(c) = NUMBERED.captures(&text)
    {
        label = Some(c[1].to_string());
        text = text[c[0].len()..].to_string();
    }
    let first = &g[0];
    let rect = g
        .iter()
        .filter(|l| l.page == first.page)
        .fold(first.rect, |r, l| r.union(&l.rect));
    let year = YEAR.captures(&text).map(|c| c[1].to_string());
    let doi = DOI
        .captures(&text)
        .map(|c| c[1].trim_end_matches(['.', ')']).to_lowercase());
    let arxiv = ARXIV.captures(&text).map(|c| c[1].to_string());
    let url = URL
        .find(&text)
        .map(|m| m.as_str().trim_end_matches(['.', ')', ',']).to_string());
    let (authors, title) = authors_title(&text);
    Reference {
        index,
        label,
        text,
        authors,
        year,
        title,
        doi,
        arxiv,
        url,
        page: first.page,
        rect: PageRect {
            page: first.page,
            ..rect
        },
    }
}

/// Authors and title, by the two common shapes:
/// "A. Smith, B. Lee, Title of the paper, Journal 3 (2020)" and
/// "Smith, A., & Lee, B. (2020). Title of the paper. Journal".
fn authors_title(text: &str) -> (Vec<String>, Option<String>) {
    // Author–year: authors before "(2020)", title after it.
    static PAREN_YEAR: LazyLock<Regex> =
        LazyLock::new(|| re(r"\(\s*((?:19|20)\d{2})[a-z]?\s*\)\.?"));
    // Only when everything before the year reads like a list of names (no long segment).
    if let Some(m) = PAREN_YEAR.find(text)
        && m.start() > 3
        && text[..m.start()]
            .split(',')
            .all(|seg| seg.split_whitespace().count() <= 4)
    {
        let authors = split_authors(text[..m.start()].trim().trim_end_matches(','));
        let rest = text[m.end()..].trim();
        let title = rest
            .split(". ")
            .next()
            .map(|t| t.trim().trim_end_matches('.').to_string())
            .filter(|t| t.len() > 3);
        return (authors, title);
    }
    // Comma style: authors are the leading segments that look like names; the title is the first
    // longer segment (quoted titles win).
    let quoted = QUOTED
        .captures(text)
        .map(|c| c[1].trim().trim_end_matches(',').to_string());
    let mut authors = Vec::new();
    let mut title = None;
    for seg in text
        .split([',', '.'])
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        let words = seg.split_whitespace().count();
        let namey = words <= 4
            && seg.split_whitespace().all(|w| {
                w.starts_with(|c: char| c.is_uppercase())
                    || matches!(w, "and" | "&" | "van" | "de" | "der" | "von" | "et" | "al")
            });
        if title.is_none() && namey && quoted.as_ref().is_none_or(|q| !q.contains(seg)) {
            let seg = seg
                .trim_start_matches("and ")
                .trim_start_matches("& ")
                .to_string();
            if !seg.is_empty() {
                authors.push(seg);
            }
            continue;
        }
        if words >= 3 {
            title = Some(seg.to_string());
            break;
        }
    }
    // Initials split by '.' became separate segments: rejoin "A" "Smith" → "A Smith".
    let mut joined: Vec<String> = Vec::new();
    for a in authors {
        if let Some(last) = joined.last_mut()
            && last.split_whitespace().all(|w| w.chars().count() == 1)
        {
            last.push(' ');
            last.push_str(&a);
        } else {
            joined.push(a);
        }
    }
    (joined, quoted.or(title))
}

fn split_authors(s: &str) -> Vec<String> {
    // "Smith, A., Lee, B., & Kim, C." → ["Smith, A.", "Lee, B.", "Kim, C."]
    let s = s
        .replace(", &", ",")
        .replace(" & ", ", ")
        .replace(", and ", ", ")
        .replace(" and ", ", ");
    let parts: Vec<&str> = s
        .split(", ")
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .collect();
    let mut out: Vec<String> = Vec::new();
    let mut i = 0;
    while i < parts.len() {
        let p = parts[i];
        let next_is_initials = parts.get(i + 1).is_some_and(|n| {
            n.chars().filter(|c| c.is_alphabetic()).count() <= 4 && n.contains('.')
        });
        if next_is_initials {
            out.push(format!("{p}, {}", parts[i + 1]));
            i += 2;
        } else {
            out.push(p.to_string());
            i += 1;
        }
    }
    out
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::super::{lines, testing::page};
    use super::*;

    #[test]
    fn bracketed_list_with_wrapped_lines() {
        let p = page(
            9,
            &[
                ("5 Conclusion", 72.0, 60.0, 11.0),
                ("We conclude things.", 72.0, 80.0, 10.0),
                ("References", 72.0, 120.0, 11.0),
                (
                    "[1] A. Smith, B. Lee, Physics informed neural networks for control,",
                    72.0,
                    140.0,
                    10.0,
                ),
                (
                    "Applied Energy 300 (2021) 117. doi:10.1016/j.apenergy.2021.117.",
                    84.0,
                    152.0,
                    10.0,
                ),
                (
                    "[2] C. Doe, A second paper about buildings, arXiv:2111.12066v2 (2022).",
                    72.0,
                    166.0,
                    10.0,
                ),
            ],
        );
        let r = references(&[lines(&p)]);
        assert_eq!(r.len(), 2);
        assert_eq!(r[0].label.as_deref(), Some("1"));
        assert_eq!(r[0].year.as_deref(), Some("2021"));
        assert_eq!(r[0].doi.as_deref(), Some("10.1016/j.apenergy.2021.117"));
        assert_eq!(r[0].authors, vec!["A Smith", "B Lee"]);
        assert_eq!(
            r[0].title.as_deref(),
            Some("Physics informed neural networks for control")
        );
        assert_eq!(r[1].arxiv.as_deref(), Some("2111.12066v2"));
        assert_eq!(r[1].page, 9);
    }

    #[test]
    fn author_year_list_with_hanging_indent() {
        let p = page(
            3,
            &[
                ("References", 72.0, 100.0, 11.0),
                (
                    "Smith, A., & Lee, B. (2020). Learning thermal models of buildings. Energy and",
                    72.0,
                    120.0,
                    10.0,
                ),
                ("Buildings, 12, 1–10.", 84.0, 132.0, 10.0),
                (
                    "Zhang, C. (2019). Another title here. Journal of Things.",
                    72.0,
                    146.0,
                    10.0,
                ),
            ],
        );
        let r = references(&[lines(&p)]);
        assert_eq!(r.len(), 2, "{r:#?}");
        assert_eq!(r[0].authors, vec!["Smith, A.", "Lee, B."]);
        assert_eq!(
            r[0].title.as_deref(),
            Some("Learning thermal models of buildings")
        );
        assert_eq!(r[1].year.as_deref(), Some("2019"));
        assert!(r[0].label.is_none());
    }

    #[test]
    fn no_heading_no_references() {
        let p = page(1, &[("Just text", 72.0, 100.0, 10.0)]);
        assert_eq!(
            references(&[lines(&p)]),
            [] as [ep_document_contract::Reference; 0]
        );
    }
}
