//! What a model wrote, against its source: numbers, words, terms and tags. Code decides.

use std::collections::{HashMap, HashSet};

use crate::{content, head, is_stop, squash, stem, words};

/// Share of a summary's content words that must stand in its source.
pub const GROUNDED: f32 = 0.6;

/// Numbers of a text in one spelling: `1,234.50` → `1234.5`, `53%` → `53`.
#[must_use]
#[allow(clippy::many_single_char_names)] // a small scanner
pub fn numbers(s: &str) -> HashSet<String> {
    let c: Vec<char> = s.chars().collect();
    let (mut out, mut i) = (HashSet::new(), 0);
    while i < c.len() {
        if !c[i].is_ascii_digit() {
            i += 1;
            continue;
        }
        let start = i;
        while i < c.len()
            && (c[i].is_ascii_digit()
                || (matches!(c[i], '.' | ',') && c.get(i + 1).is_some_and(char::is_ascii_digit)))
        {
            i += 1;
        }
        // A digit inside a word (CO2, E5, H2O) is part of the word, not a number.
        if start > 0 && c[start - 1].is_alphabetic() {
            continue;
        }
        let t: String = c[start..i].iter().filter(|x| **x != ',').collect();
        let t = if t.contains('.') {
            t.trim_end_matches('0').trim_end_matches('.').to_string()
        } else {
            t
        };
        let z = t.trim_start_matches('0');
        out.insert(if z.is_empty() || z.starts_with('.') {
            format!("0{z}")
        } else {
            z.to_string()
        });
    }
    out
}

/// Numbers of `said` that do not stand in `source`.
#[must_use]
pub fn wrong_numbers(said: &str, source: &str) -> Vec<String> {
    let have = numbers(source);
    let mut v: Vec<String> = numbers(said)
        .into_iter()
        .filter(|n| !have.contains(n))
        .collect();
    v.sort();
    v
}

/// Without the sentences that hold a number the source does not have.
#[must_use]
pub fn drop_wrong(said: &str, source: &str) -> String {
    crate::split::sentences(said)
        .into_iter()
        .filter(|s| wrong_numbers(s, source).is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

/// A summary without the list numbers and the heading a model copied into it.
#[must_use]
pub fn clean_summary(said: &str, title: &str) -> String {
    let s = crate::split::sentences(said)
        .into_iter()
        .filter(|s| !s.trim_end_matches('.').chars().all(|c| c.is_ascii_digit()))
        .collect::<Vec<_>>()
        .join(" ");
    let rest = s
        .strip_prefix(title.trim())
        .filter(|_| !title.trim().is_empty())
        .map(str::trim_start);
    match rest {
        // The heading, then a new sentence.
        Some(r) if r.starts_with(char::is_uppercase) => r.to_string(),
        _ => s,
    }
}

/// Share of the content words of `said` that stand in `source`.
#[must_use]
#[allow(clippy::cast_precision_loss)] // counts of words
pub fn grounded(said: &str, source: &str) -> f32 {
    let have: HashSet<String> = content(source).collect();
    let all: Vec<String> = content(said).collect();
    if all.is_empty() {
        return 0.0;
    }
    all.iter().filter(|w| have.contains(*w)).count() as f32 / all.len() as f32
}

/// Letters and digits only, lower case, with single spaces.
fn fold(s: &str) -> String {
    words(s).collect::<Vec<_>>().join(" ")
}

/// Terms that stand in the source, word for word; no doubles.
#[must_use]
pub fn keep_terms(terms: &[String], source: &str, most: usize) -> Vec<String> {
    let have = format!(" {} ", fold(source));
    let mut seen = HashSet::new();
    terms
        .iter()
        .map(|t| squash(t))
        .filter(|t| (2..=60).contains(&t.chars().count()))
        .filter(|t| {
            let f = fold(t);
            !f.is_empty() && !f.split(' ').all(is_stop) && have.contains(&format!(" {f} "))
        })
        .filter(|t| seen.insert(t.to_lowercase()))
        .take(most)
        .collect()
}

/// Tags in one form: lower case, 1–3 words, singular where that is safe, no doubles.
#[must_use]
pub fn clean_tags(tags: &[String], most: usize) -> Vec<String> {
    let mut seen = HashSet::new();
    tags.iter()
        .filter_map(|t| {
            let w: Vec<String> = t
                .split(|c: char| !(c.is_alphanumeric() || c == '-'))
                .filter(|w| !w.is_empty())
                .map(str::to_lowercase)
                .collect();
            if w.is_empty() || w.len() > 3 || w.iter().all(|x| is_stop(x)) {
                return None;
            }
            let last = w.len() - 1;
            let w: Vec<String> = w
                .into_iter()
                .enumerate()
                .map(|(i, x)| if i == last { singular(&x) } else { x })
                .collect();
            let t = w.join(" ");
            (t.chars().count() <= 40).then_some(t)
        })
        .filter(|t| seen.insert(t.split(' ').map(stem).collect::<Vec<_>>().join(" ")))
        .take(most)
        .collect()
}

fn singular(w: &str) -> String {
    if !w.is_ascii()
        || w.len() < 5
        || w.ends_with("ss")
        || w.ends_with("us")
        || w.ends_with("is")
        || w.ends_with("ics")
    {
        return w.to_string();
    }
    if let Some(b) = w.strip_suffix("ies") {
        return format!("{b}y");
    }
    w.strip_suffix('s').unwrap_or(w).to_string()
}

/// The most telling words of a text: frequent in it, and not everyday words.
#[must_use]
pub fn telling(text: &str, most: usize) -> Vec<String> {
    let mut count: HashMap<String, (usize, String)> = HashMap::new();
    for w in words(text)
        .filter(|w| w.chars().count() > 3 && !is_stop(w) && !w.chars().all(|c| c.is_ascii_digit()))
    {
        let e = count.entry(stem(&w)).or_insert((0, w));
        e.0 += 1;
    }
    let mut v: Vec<(usize, String)> = count.into_values().filter(|(n, _)| *n > 1).collect();
    v.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
    v.into_iter().take(most).map(|(_, w)| w).collect()
}

/// A question of a reader: one line, ends with a question mark.
#[must_use]
pub fn clean_question(q: &str) -> Option<String> {
    let q = squash(q);
    let q = head(&q, 140)
        .trim_end_matches(['.', ' ', '?', '؟', '？'])
        .to_string();
    (q.split(' ').count() >= 3).then(|| format!("{q}?"))
}

#[cfg(test)]
mod tests {
    use super::*;

    const SOURCE: &str = "Wood-chip CHP of 4 MWe has an NPV of £41.36 million and saves 7,250 t of CO2 per year (47.0%). Simple payback is 3.40 years.";

    #[test]
    fn wrong_numbers_are_seen_and_dropped() {
        assert_eq!(
            wrong_numbers("NPV is 41.36 million; it saves 7250 t or 47 %.", SOURCE),
            [""; 0]
        );
        assert_eq!(
            wrong_numbers("NPV is 42.36 million in 2030.", SOURCE),
            ["2030", "42.36"]
        );
        assert_eq!(
            drop_wrong("It saves 7,250 t of CO2. The NPV is 99 million.", SOURCE),
            "It saves 7,250 t of CO2."
        );
        assert!(numbers("CO2 and E5 in 007").contains("7") && !numbers("CO2").contains("2"));
    }

    #[test]
    fn list_numbers_and_headings_are_not_part_of_a_summary() {
        assert_eq!(
            clean_summary("1. The load is 5 MW. 2. It peaks in winter.", "Load"),
            "The load is 5 MW. It peaks in winter."
        );
        assert_eq!(
            clean_summary(
                "Economic indicators NPV is the main one.",
                "Economic indicators"
            ),
            "NPV is the main one."
        );
        assert_eq!(
            clean_summary("Load grows by 12.5 MW.", "Load"),
            "Load grows by 12.5 MW."
        );
    }

    #[test]
    fn a_made_up_summary_is_not_grounded() {
        assert!(
            grounded(
                "The wood-chip CHP saves CO2 and has a short payback.",
                SOURCE
            ) >= GROUNDED
        );
        assert!(
            grounded(
                "Penguins migrate across frozen oceans during winter.",
                SOURCE
            ) < 0.2
        );
        assert!(grounded("", SOURCE) < GROUNDED);
    }

    #[test]
    fn terms_stand_in_the_text() {
        let t = [
            "NPV",
            "npv",
            "wood-chip CHP",
            "solar farm",
            "the",
            "simple payback",
        ]
        .map(String::from);
        assert_eq!(
            keep_terms(&t, SOURCE, 8),
            ["NPV", "wood-chip CHP", "simple payback"]
        );
    }

    #[test]
    fn tags_are_clean_and_not_doubled() {
        let t = [
            "Renewable Energies",
            "renewable energy",
            "CO2 emissions!",
            "a very long tag of words",
            "the",
            "Physics",
        ]
        .map(String::from);
        assert_eq!(
            clean_tags(&t, 6),
            ["renewable energy", "co2 emission", "physics"]
        );
    }

    #[test]
    fn telling_words_are_the_frequent_ones() {
        assert_eq!(
            telling(
                "turbine blades and turbines with a blade, a turbine and nothing else",
                2
            ),
            ["turbine", "blades"]
        );
        assert_eq!(
            clean_question("what is the NPV of wood chips"),
            Some("what is the NPV of wood chips?".into())
        );
        assert_eq!(clean_question("Why?"), None);
    }
}
