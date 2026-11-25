//! Abbreviations and what the paper itself says they stand for (P1.M5.UND.1): "precessing vortex
//! core (PVC)", "PVC (precessing vortex core)". The letters must fit the words (Schwartz and
//! Hearst 2003), so a wrong expansion is never made up.

use crate::numbers;

/// The abbreviation a selection is, without the "s" of a plural ("`PINNs`" → "PINN"); `None` for
/// anything else.
#[must_use]
pub fn abbreviation(selection: &str) -> Option<&str> {
    let s = selection
        .trim()
        .trim_matches(|c: char| !c.is_alphanumeric());
    let s = match s.strip_suffix('s') {
        Some(one) if one.ends_with(|c: char| c.is_uppercase()) => one,
        _ => s,
    };
    let (mut letters, mut upper) = (0, 0);
    for c in s.chars() {
        if c.is_alphabetic() {
            letters += 1;
            upper += usize::from(c.is_uppercase());
        } else if !(c.is_numeric() || c == '-') {
            return None;
        }
    }
    let begins = s.starts_with(char::is_alphabetic);
    (begins && (2..=10).contains(&s.chars().count()) && upper >= 2 && 2 * upper >= letters)
        .then_some(s)
}

/// The abbreviations of a text, each once, in the order they stand; at most `most`.
#[must_use]
pub fn all(text: &str, most: usize) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for w in text.split(|c: char| !(c.is_alphanumeric() || c == '-')) {
        if out.len() == most {
            break;
        }
        if let Some(a) = abbreviation(w)
            && !out.iter().any(|o| o == a)
        {
            out.push(a.to_string());
        }
    }
    out
}

/// An answer in which the abbreviations stand for what the paper says (`known`: abbreviation and
/// words): other words that spell one are replaced; a sentence that says it stands for something
/// else is left out. How many were left out, and whether anything changed.
#[must_use]
pub fn faithful(answer: &str, known: &[(String, String)]) -> (String, u32, bool) {
    let (mut kept, mut left, mut changed) = (String::new(), 0, false);
    for s in numbers::sentences(answer) {
        if let Some(t) = known
            .iter()
            .try_fold(s.to_string(), |s, (abbr, words)| expanded(&s, abbr, words))
        {
            changed |= t != s;
            kept.push_str(&t);
        } else {
            left += 1;
            changed = true;
        }
    }
    (kept.trim().to_string(), left, changed)
}

/// A sentence in which `abbr` is `words`; none when it says something else that cannot be replaced.
fn expanded(sentence: &str, abbr: &str, words: &str) -> Option<String> {
    let mut s = sentence.to_string();
    for at in places(sentence, abbr).into_iter().rev() {
        let (before, rest) = (&sentence[..at], &sentence[at + abbr.len()..]);
        let after = rest.strip_prefix('s').unwrap_or(rest);
        if let Some(b) = before.strip_suffix('(')
            && after.starts_with(')')
        {
            // "stimulated scattering (SS)".
            match words_for(abbr, b) {
                Some(w) if says(&w, words) || says(words, &w) => {}
                Some(w) => {
                    let b = b.trim_end();
                    s.replace_range(b.strip_suffix(w.as_str())?.len()..b.len(), words);
                }
                None if says(sentence, words) => {}
                None => return None,
            }
        } else if let Some(inside) = after
            .trim_start()
            .strip_prefix('(')
            .and_then(|r| r.split_once(')'))
            .map(|(i, _)| i)
        {
            // "SS (stimulated scattering)".
            if words_for(abbr, inside).is_some_and(|w| w.len() == inside.trim().len())
                && !says(inside, words)
                && !says(words, inside)
            {
                let from = sentence.len() - after.len() + after.find('(')? + 1;
                s.replace_range(from..from + inside.len(), words);
            }
        } else if after.trim_start().starts_with("stands for") && !says(sentence, words) {
            return None;
        }
    }
    Some(s)
}

/// Where `abbr` stands as a word.
fn places(text: &str, abbr: &str) -> Vec<usize> {
    let word = |c: char| c.is_alphanumeric() || c == '-';
    text.match_indices(abbr)
        .map(|(i, _)| i)
        .filter(|i| {
            let after = &text[i + abbr.len()..];
            !text[..*i].ends_with(word)
                && !after.strip_prefix('s').unwrap_or(after).starts_with(word)
        })
        .collect()
}

/// What `abbr` stands for, when it is defined where it stands: `before` and `after` are the words
/// around it.
#[must_use]
pub fn defined(abbr: &str, before: &str, after: &str) -> Option<String> {
    let before = before.trim_end();
    if let Some(words) = before.strip_suffix(['(', '[']) {
        // "precessing vortex core (PVC)", "(PVCs)", "(PVC, see below)".
        let after = after.strip_prefix('s').unwrap_or(after).trim_start();
        return after
            .starts_with([')', ']', ',', ';'])
            .then(|| words_for(abbr, words))
            .flatten();
    }
    // "PVC (precessing vortex core)".
    let after = after.strip_prefix('s').unwrap_or(after).trim_start();
    let inside = after.strip_prefix('(')?.split(')').next()?.trim();
    let whole = words_for(abbr, inside)?;
    (whole.len() == inside.len()).then_some(whole)
}

/// The last words of `text` whose letters spell `abbr`: every letter in order, the first one at
/// the beginning of a word.
fn words_for(abbr: &str, text: &str) -> Option<String> {
    let short: Vec<char> = abbr
        .chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect();
    let text = text.trim_end();
    let long: Vec<char> = text.chars().collect();
    let begins = |i: usize| i == 0 || !long[i - 1].is_alphanumeric();
    let mut at = long.len();
    for (n, c) in short.iter().enumerate().rev() {
        loop {
            at = at.checked_sub(1)?;
            // A definition stands in one sentence.
            if matches!(long[at], '.' | ';' | '(' | ')' | ':') {
                return None;
            }
            if long[at].to_lowercase().eq([*c]) && (n > 0 || begins(at)) {
                break;
            }
        }
    }
    let found: String = long[at..].iter().collect();
    let words = found
        .split(|c: char| c.is_whitespace() || c == '-')
        .filter(|w| !w.is_empty())
        .count();
    let most = (short.len() + 5).min(2 * short.len());
    let squashed = found.split_whitespace().collect::<Vec<_>>().join(" ");
    // Words are longer than their letters, and more than one; most letters begin a word.
    let first = squashed
        .split(|c: char| !c.is_alphanumeric())
        .filter_map(|w| w.chars().next())
        .flat_map(char::to_lowercase)
        .filter(|c| short.contains(c))
        .count();
    (words >= 2 && words <= most && 2 * first >= short.len() && squashed.len() > short.len() + 2)
        .then_some(squashed)
}

/// The best written of the same words: a line that ends in "physics-" gives "physicsinformed" at one
/// place and "physics-informed" at another. `first` is the one that counts.
#[must_use]
pub fn best_written(first: &str, others: &[String]) -> String {
    let parts = |s: &str| {
        s.split(|c: char| !c.is_alphanumeric())
            .filter(|w| !w.is_empty())
            .count()
    };
    others
        .iter()
        .filter(|o| says(o, first) && says(first, o))
        .fold(
            first,
            |best, o| {
                if parts(o) > parts(best) { o } else { best }
            },
        )
        .to_string()
}

/// The same words, whatever the case, the hyphens or the number ("networks" and "Network").
#[must_use]
pub fn says(text: &str, words: &str) -> bool {
    let fold = |s: &str| -> String {
        s.chars()
            .filter(|c| c.is_alphanumeric())
            .flat_map(char::to_lowercase)
            .collect()
    };
    let words = fold(words);
    fold(text).contains(words.strip_suffix('s').unwrap_or(&words))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn what_is_an_abbreviation() {
        for (s, a) in [
            ("PVC", "PVC"),
            (" PINNs ", "PINN"),
            ("(LES)", "LES"),
            ("SCPINN", "SCPINN"),
            ("UMAP-2", "UMAP-2"),
            ("DeepONet", ""),
            ("policy", ""),
            ("MDP model", ""),
            ("A", ""),
            ("3D", ""),
        ] {
            assert_eq!(abbreviation(s).unwrap_or_default(), a, "{s}");
        }
    }

    #[test]
    fn the_words_before_the_brackets() {
        for (abbr, before, words) in [
            (
                "PVC",
                "forms a precessing vortex core (",
                "precessing vortex core",
            ),
            (
                "SCPINN",
                "we propose the strongly-constrained physics-informed neural network (",
                "strongly-constrained physics-informed neural network",
            ),
            (
                "MDP",
                "modelled as a Markov Decision  Process (",
                "Markov Decision Process",
            ),
            (
                "FFT",
                "with the fast Fourier transform (",
                "fast Fourier transform",
            ),
            (
                "CRM",
                "the NASA Common Research Model (",
                "Common Research Model",
            ),
        ] {
            assert_eq!(
                defined(abbr, before, ") of").as_deref(),
                Some(words),
                "{abbr}"
            );
        }
        assert_eq!(
            defined("PINN", "physics-informed neural networks (", "s) are").as_deref(),
            Some("physics-informed neural networks")
        );
    }

    #[test]
    fn the_words_in_the_brackets() {
        assert_eq!(
            defined("LES", "compared with ", " (large eddy simulation) and").as_deref(),
            Some("large eddy simulation")
        );
        assert_eq!(
            defined("LES", "the ", " (see the large eddy simulation)"),
            None
        );
        assert_eq!(defined("LES", "the ", " (Smith et al., 2020)"), None);
    }

    #[test]
    fn abbreviations_of_a_text() {
        assert_eq!(
            all("The SS and SFS (see PINNs, SS) of Fig. 3 in 2D", 9),
            ["SS", "SFS", "PINN"]
        );
        assert_eq!(all("AB CD EF", 2), ["AB", "CD"]);
    }

    #[test]
    fn an_answer_says_what_the_paper_says() {
        let known = [
            ("SS".to_string(), "self-steepening".to_string()),
            (
                "SFS".to_string(),
                "Raman soliton self-frequency shift".to_string(),
            ),
            ("PVC".to_string(), "precessing vortex core".to_string()),
        ];
        let right = "It has self-steepening (SS). The SS grows (see Fig. 3).";
        assert_eq!(faithful(right, &known), (right.to_string(), 0, false));
        assert_eq!(
            faithful("It has stimulated scattering (SS) and loss.", &known),
            ("It has self-steepening (SS) and loss.".to_string(), 0, true)
        );
        assert_eq!(
            faithful("SS (stimulated scattering) grows.", &known).0,
            "SS (self-steepening) grows."
        );
        assert_eq!(
            faithful(
                "It falls. It has stimulated four-wave mixing (SFS).",
                &known
            ),
            ("It falls.".to_string(), 1, true),
            "letters that do not fit cannot be replaced"
        );
        assert_eq!(
            faithful("PVC stands for a strong vortex.", &known),
            (String::new(), 1, true)
        );
        for same in [
            "The Precessing Vortex Cores (PVCs) are strong.",
            "PVC (see Fig. 2) is strong.",
            "The CLASS (see above) is SS.",
            "PVC stands for precessing vortex core.",
        ] {
            assert_eq!(faithful(same, &known), (same.to_string(), 0, false));
        }
    }

    #[test]
    fn nothing_is_made_up() {
        assert_eq!(defined("PVC", "the pressure was low (", ") and"), None);
        assert_eq!(defined("PVC", "see the flow. Core (", ")"), None);
        assert_eq!(defined("PVC", "the PVC is", " strong"), None);
        assert_eq!(defined("SI", "units are used (", ")"), None);
    }

    #[test]
    fn a_word_broken_at_a_line_is_written_whole() {
        let seen = [
            "physicsinformed neural networks".to_string(),
            "physics-informed neural networks".to_string(),
            "partially informed neural networks".to_string(),
        ];
        assert_eq!(best_written(&seen[0], &seen), seen[1]);
        assert_eq!(best_written(&seen[2], &seen), seen[2]);
        assert_eq!(best_written("Gaussian process", &[]), "Gaussian process");
    }

    #[test]
    fn the_same_words_in_another_form() {
        assert!(says(
            "FNO stands for Fourier Neural Operator.",
            "Fourier neural operators"
        ));
        assert!(!says(
            "FNO stands for a network.",
            "Fourier neural operators"
        ));
    }
}
