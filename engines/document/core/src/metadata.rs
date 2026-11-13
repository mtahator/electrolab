//! Cleaning metadata read from a PDF (P1.M1.WSP.3).
//!
//! PDF Info fields are often junk ("Microsoft Word - draft3.docx", "untitled", a LaTeX file
//! name). These rules keep what looks real and drop the rest; a missing value is better than a
//! wrong one, because later stages (`PDFium` first-page text in M2, references in M6) fill gaps.

use ep_document_contract::PdfMetadata;

/// Raw values as found in the file.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RawMetadata {
    /// `/Title`.
    pub title: Option<String>,
    /// `/Author`.
    pub author: Option<String>,
    /// `/Subject`.
    pub subject: Option<String>,
    /// `/Keywords`.
    pub keywords: Option<String>,
    /// Custom Info keys and values (e.g. `doi`, `arXivID`).
    pub custom: Vec<(String, String)>,
    /// Page count.
    pub pages: u32,
    /// Declares encryption.
    pub encrypted: bool,
}

fn tidy(s: &str) -> String {
    s.chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// A title worth showing, or `None`.
#[must_use]
pub fn clean_title(raw: &str) -> Option<String> {
    let t = tidy(raw);
    let lower = t.to_lowercase();
    let junk_prefix = [
        "microsoft word - ",
        "microsoft powerpoint - ",
        "untitled",
        "title",
        "document",
        "slide 1",
        "doi:",
        "arxiv:",
        "overleaf example",
        "latex template",
        "paper title",
        "manuscript",
    ];
    let junk_suffix = [
        ".pdf", ".doc", ".docx", ".tex", ".dvi", ".ps", ".indd", ".rtf", ".odt",
    ];
    let too_short = t.chars().filter(|c| c.is_alphabetic()).count() < 4;
    let looks_like_id = !t.contains(' ') && t.chars().any(|c| c.is_ascii_digit());
    if t.is_empty()
        || too_short
        || looks_like_id
        || junk_prefix.iter().any(|p| lower.starts_with(p))
        || junk_suffix.iter().any(|s| lower.ends_with(s))
    {
        return None;
    }
    Some(t)
}

/// Authors in order. Splits on `;`, ` and `, `&`, and on commas between full names (but not
/// "Surname, Initials").
#[must_use]
pub fn split_authors(raw: &str) -> Vec<String> {
    let t = tidy(raw);
    let lower = t.to_lowercase();
    if t.is_empty() || ["unknown", "anonymous", "user", "admin", "author"].contains(&lower.as_str())
    {
        return vec![];
    }
    let mut parts: Vec<String> = vec![t.clone()];
    for sep in [";", " and ", " & ", " AND "] {
        parts = parts
            .iter()
            .flat_map(|p| p.split(sep).map(str::to_string))
            .collect();
    }
    // Commas: split only when every piece is a full name ("Ada Lovelace, Alan Turing"), not
    // "Lovelace, A." which is one person.
    parts = parts
        .into_iter()
        .flat_map(|p| {
            let pieces: Vec<&str> = p
                .split(',')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .collect();
            let full_names = pieces.len() > 1
                && pieces
                    .iter()
                    .all(|s| s.split_whitespace().count() >= 2 && !s.ends_with('.'));
            if full_names {
                pieces.into_iter().map(str::to_string).collect()
            } else {
                vec![p]
            }
        })
        .collect();
    parts
        .into_iter()
        .map(|p| tidy(&p))
        .filter(|p| p.chars().any(char::is_alphabetic))
        .collect()
}

/// The first DOI in `text`, lowercase, without a resolver prefix.
#[must_use]
pub fn find_doi(text: &str) -> Option<String> {
    let lower = text.to_lowercase();
    let bytes = lower.as_bytes();
    let mut start = 0;
    while let Some(off) = lower[start..].find("10.") {
        let i = start + off;
        start = i + 3;
        // "10." must not be the tail of a longer number.
        if i > 0 && bytes[i - 1].is_ascii_digit() {
            continue;
        }
        let rest = &lower[i + 3..];
        let digits = rest.bytes().take_while(u8::is_ascii_digit).count();
        if !(4..=9).contains(&digits) || rest.as_bytes().get(digits) != Some(&b'/') {
            continue;
        }
        let suffix: String = rest[digits + 1..]
            .chars()
            .take_while(|c| !c.is_whitespace() && !matches!(c, '"' | '<' | '>' | '{' | '}'))
            .collect();
        let mut suffix = suffix.as_str();
        // Drop trailing punctuation; a `)` only when it closes nothing inside the DOI.
        loop {
            let unbalanced = |s: &str| s.matches(')').count() > s.matches('(').count();
            match suffix.chars().last() {
                Some('.' | ',' | ';' | ']' | ':') => suffix = &suffix[..suffix.len() - 1],
                Some(')') if unbalanced(suffix) => suffix = &suffix[..suffix.len() - 1],
                _ => break,
            }
        }
        if !suffix.is_empty() {
            return Some(format!("10.{}/{}", &rest[..digits], suffix));
        }
    }
    None
}

/// Clean raw metadata into the contract's form.
#[must_use]
pub fn clean(raw: &RawMetadata) -> PdfMetadata {
    let doi_keys = ["doi", "prism:doi", "dc:identifier", "identifier"];
    let doi = raw
        .custom
        .iter()
        .filter(|(k, _)| doi_keys.contains(&k.to_lowercase().as_str()))
        .find_map(|(_, v)| find_doi(v))
        .or_else(|| {
            [&raw.subject, &raw.keywords, &raw.title]
                .into_iter()
                .flatten()
                .find_map(|s| find_doi(s))
        });
    PdfMetadata {
        title: raw.title.as_deref().and_then(clean_title),
        authors: raw.author.as_deref().map(split_authors).unwrap_or_default(),
        doi,
        pages: raw.pages,
        subject: raw.subject.as_deref().map(tidy).filter(|s| !s.is_empty()),
        encrypted: raw.encrypted,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn titles() {
        assert_eq!(
            clean_title("  Attention Is\nAll You Need "),
            Some("Attention Is All You Need".into())
        );
        for junk in [
            "Microsoft Word - final.docx",
            "Overleaf Example",
            "untitled",
            "main.tex",
            "paper.pdf",
            "2106.03711",
            "",
            "ab",
        ] {
            assert_eq!(clean_title(junk), None, "{junk}");
        }
        assert_eq!(
            clean_title("شبکه عصبی مصنوعی"),
            Some("شبکه عصبی مصنوعی".into())
        );
    }

    #[test]
    fn authors() {
        assert_eq!(
            split_authors("Ada Lovelace; Alan Turing"),
            ["Ada Lovelace", "Alan Turing"]
        );
        assert_eq!(
            split_authors("Ada Lovelace, Alan Turing and Grace Hopper"),
            ["Ada Lovelace", "Alan Turing", "Grace Hopper"]
        );
        assert_eq!(split_authors("Lovelace, A."), ["Lovelace, A."]);
        assert_eq!(split_authors("Unknown"), Vec::<String>::new());
    }

    #[test]
    fn dois() {
        assert_eq!(
            find_doi("doi:10.1371/journal.pone.0232683."),
            Some("10.1371/journal.pone.0232683".into())
        );
        assert_eq!(
            find_doi("https://doi.org/10.48550/arXiv.2106.03711"),
            Some("10.48550/arxiv.2106.03711".into())
        );
        assert_eq!(find_doi("version 210.1234/x"), None);
        assert_eq!(find_doi("10.12/x"), None);
        assert_eq!(
            find_doi("[see 10.1016/0021-9991(79)90145-1]."),
            Some("10.1016/0021-9991(79)90145-1".into())
        );
        assert_eq!(
            find_doi("(Smith 2020, 10.1000/abc)."),
            Some("10.1000/abc".into())
        );
    }

    #[test]
    fn doi_from_custom_key_wins() {
        let raw = RawMetadata {
            subject: Some("see 10.1000/other".into()),
            custom: vec![("DOI".into(), "10.5555/right".into())],
            pages: 3,
            ..RawMetadata::default()
        };
        assert_eq!(clean(&raw).doi.as_deref(), Some("10.5555/right"));
    }
}
