//! Knowledge engine core (pure): cutting a document into sections, checking what a model wrote
//! against its source, cleaning tags, and ranking papers and places by words.

pub mod check;
pub mod rank;
pub mod split;

/// Collapse runs of white space.
#[must_use]
pub fn squash(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// At most `n` characters.
#[must_use]
pub fn head(s: &str, n: usize) -> &str {
    s.char_indices().nth(n).map_or(s, |(i, _)| &s[..i])
}

/// English words that say nothing about the subject, and the words people ask with.
const STOP: &str = " a about above after again against all also am an and any are as at be because been before being below between both but by can could did different do does doing down during each few find for from further get give had has have having he her here hers him his how i if in into is it its itself just like look looking make me more most my near no nor not now of off on once one only or other our out over own paper papers pdf per place please same search see she should show so some such than that the their them then there these they this those through to too under until up us use used using various very want was we were what when where which while who why will with would you your document documents article page pages section chapter figure figures fig table tables chart charts graph graphs plot plots diagram equation equations ";

/// Whether a word says nothing about the subject.
#[must_use]
pub fn is_stop(w: &str) -> bool {
    w.len() < 2 && !w.chars().all(char::is_alphanumeric) || STOP.contains(&format!(" {w} "))
}

/// A word without the simplest English endings, for comparing.
#[must_use]
pub fn stem(w: &str) -> String {
    let w = w.to_lowercase();
    // Carbon dioxide and its equivalent are named for each other.
    if matches!(w.as_str(), "co2e" | "co2eq" | "co₂" | "co₂e") {
        return "co2".into();
    }
    if !w.is_ascii() || w.len() < 5 {
        return w;
    }
    for (end, to) in [
        ("ies", "y"),
        ("sses", "ss"),
        ("ing", ""),
        ("edly", ""),
        ("ed", ""),
        ("es", ""),
        ("ly", ""),
        ("s", ""),
    ] {
        if let Some(b) = w.strip_suffix(end)
            && b.len() >= 4
            && !(end == "s" && b.ends_with('s'))
        {
            let b = format!("{b}{to}");
            return b.strip_suffix('e').map_or(b.clone(), str::to_string);
        }
    }
    w.strip_suffix('e').map_or(w.clone(), str::to_string)
}

/// Words of a text, lower case.
pub fn words(s: &str) -> impl Iterator<Item = String> + '_ {
    s.split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(str::to_lowercase)
}

/// Words that carry meaning, reduced for comparing.
pub fn content(s: &str) -> impl Iterator<Item = String> + '_ {
    words(s).filter(|w| !is_stop(w)).map(|w| stem(&w))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn word_forms_meet() {
        for (a, b) in [
            ("savings", "saving"),
            ("technologies", "technology"),
            ("emissions", "emission"),
            ("compared", "compare"),
            ("charts", "chart"),
        ] {
            assert_eq!(stem(a), stem(b), "{a} {b}");
        }
        assert_ne!(stem("gas"), stem("ga"));
        assert!(is_stop("the") && !is_stop("npv"));
    }
}
