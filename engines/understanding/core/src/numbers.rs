//! Numbers in an answer must stand in what it was made from (guarantee 5).

use std::collections::HashSet;
use std::sync::LazyLock;

use regex::Regex;

static NUMBER: LazyLock<Regex> =
    LazyLock::new(|| crate::re(r"\d+(?:[.,]\d+)*(?:[eE][-+\u{2212}]?\d+)?"));

/// Numbers of a text, in one spelling ("1,234.50" and "1234.5" are the same). Digits that belong
/// to a name ("L2", "CO2") are no numbers.
fn numbers(text: &str) -> Vec<String> {
    NUMBER
        .find_iter(text)
        .filter(|m| {
            !text[..m.start()]
                .chars()
                .next_back()
                .is_some_and(|c| c.is_alphabetic() || c == '_')
        })
        .map(|m| spelling(m.as_str()))
        .collect()
}

fn spelling(n: &str) -> String {
    let n = n.replace('\u{2212}', "-").to_lowercase();
    let (n, exp) = n.split_once('e').map_or((n.as_str(), ""), |(a, b)| (a, b));
    // A comma before exactly three digits groups them; otherwise it is a decimal comma.
    let parts: Vec<&str> = n.split(',').collect();
    let n = if parts.len() > 1
        && parts[1..]
            .iter()
            .all(|p| p.split('.').next().map_or(0, str::len) == 3)
    {
        parts.concat()
    } else {
        n.replace(',', ".")
    };
    let n = if n.contains('.') {
        n.trim_end_matches('0').trim_end_matches('.').to_string()
    } else {
        n
    };
    let n = n.trim_start_matches('0');
    let n = if n.is_empty() || n.starts_with('.') {
        format!("0{n}")
    } else {
        n.to_string()
    };
    if exp.is_empty() {
        n
    } else {
        format!("{n}e{}", exp.trim_start_matches('+'))
    }
}

/// Abbreviations that end with a full stop inside a sentence.
const INSIDE: [&str; 16] = [
    "fig.", "figs.", "eq.", "eqs.", "e.g.", "i.e.", "al.", "ref.", "refs.", "sec.", "tab.", "vs.",
    "no.", "approx.", "cf.", "resp.",
];

/// The sentences of a text (joined, they are the text).
#[must_use]
pub fn sentences(text: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut start = 0;
    let bytes: Vec<(usize, char)> = text.char_indices().collect();
    for (k, (i, c)) in bytes.iter().enumerate() {
        if !matches!(c, '.' | '!' | '?') {
            continue;
        }
        let Some((_, next)) = bytes.get(k + 1) else {
            continue;
        };
        if !next.is_whitespace() {
            continue;
        }
        let so_far = &text[start..=*i];
        let word = so_far
            .split_whitespace()
            .next_back()
            .unwrap_or("")
            .to_lowercase();
        let word = word.trim_start_matches(['(', '[']);
        let after = bytes[k + 1..]
            .iter()
            .find(|(_, c)| !c.is_whitespace())
            .map(|(_, c)| *c);
        if INSIDE.contains(&word) || after.is_some_and(char::is_lowercase) {
            continue;
        }
        let end = i + c.len_utf8();
        out.push(&text[start..end]);
        start = end;
    }
    if !text[start..].trim().is_empty() {
        out.push(&text[start..]);
    }
    out
}

/// Without a last sentence that was cut off; as it is when there is no other.
#[must_use]
pub fn whole(text: &str) -> String {
    let t = text.trim();
    let mut all = sentences(t);
    let cut = !t.ends_with(['.', '!', '?', '"', ')', ']', '\u{201d}']);
    if cut && all.len() > 1 {
        all.pop();
    }
    all.concat().trim().to_string()
}

/// An answer without the sentences that name a number which stands in no source, and how many
/// were left out.
#[must_use]
pub fn supported(answer: &str, sources: &[&str]) -> (String, u32) {
    let known: HashSet<String> = sources.iter().flat_map(|s| numbers(s)).collect();
    let (mut kept, mut left) = (String::new(), 0);
    for s in sentences(answer) {
        if numbers(s).iter().all(|n| known.contains(n)) {
            kept.push_str(s);
        } else {
            left += 1;
        }
    }
    (kept.trim().to_string(), left)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_number_from_nowhere_takes_its_sentence_with_it() {
        let src = "The RMSE is 0.310 K for 1,200 samples (Fig. 3).";
        let (t, left) = supported(
            "The error is 0.31 K. It was halved by 47 %. There were 1200 samples in Fig. 3.",
            &[src],
        );
        assert_eq!(t, "The error is 0.31 K. There were 1200 samples in Fig. 3.");
        assert_eq!(left, 1);
    }

    #[test]
    fn a_sentence_that_was_cut_off_is_left_out() {
        assert_eq!(whole("It grows. It shows the"), "It grows.");
        assert_eq!(whole(" It shows the "), "It shows the", "there is no other");
        assert_eq!(whole("It grows (see Fig. 3)"), "It grows (see Fig. 3)");
    }

    #[test]
    fn digits_of_names_are_not_numbers() {
        assert_eq!(supported("The L2 error of CO2 and x_1 falls.", &[""]).1, 0);
        assert_eq!(supported("It falls to 31K.", &["31 K"]).1, 0);
        assert_eq!(supported("It falls to 31K.", &["3.1 K"]).1, 1);
    }

    #[test]
    fn spellings_of_one_number_are_the_same() {
        for (a, b) in [
            ("0.50", ".5"),
            ("1,234", "1234"),
            ("0,25", "0.25"),
            ("1e+3", "1E3"),
            ("10", "10"),
        ] {
            assert_eq!(
                numbers(&format!("x {a}"))[0].trim_start_matches('0'),
                numbers(&format!("x 0{b}"))[0].trim_start_matches('0'),
                "{a} {b}"
            );
        }
        assert_ne!(spelling("10"), spelling("1"));
    }

    #[test]
    fn sentences_join_to_the_text() {
        let t =
            "As in Fig. 3, it grows. E.g. at 2.5 m it is small! Why? Because (cf. Eq. 4) it is.";
        let s = sentences(t);
        assert_eq!(s.concat(), t);
        assert_eq!(s.len(), 4, "{s:?}");
    }
}
