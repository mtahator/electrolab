//! Word forms (a light English stemmer) and bounded edit distance (fuzzy matching).

/// Reduce an English word to a stem for `WordForms` matching: plural and simple verb endings.
/// Deliberately light — it must never merge unrelated words. Non-ASCII words are returned as-is.
#[must_use]
pub fn stem(word: &str) -> String {
    let s = stem_endings(word);
    if s.is_ascii() && s.len() >= 5 && s.ends_with('e') && !s.ends_with("ee") {
        return s[..s.len() - 1].to_string();
    }
    s
}

fn stem_endings(word: &str) -> String {
    if !word.is_ascii() || word.len() < 4 || word.bytes().any(|b| b.is_ascii_digit()) {
        return word.to_string();
    }
    let w = word;
    let strip = |n: usize| w[..w.len() - n].to_string();
    if w.ends_with("ies") && w.len() > 4 {
        return format!("{}y", &w[..w.len() - 3]);
    }
    if w.ends_with("ied") && w.len() > 4 {
        return format!("{}y", &w[..w.len() - 3]);
    }
    if w.ends_with("sses") {
        return strip(2);
    }
    if w.ends_with("ing") && w.len() >= 6 {
        return undouble(&strip(3));
    }
    if w.ends_with("ed") && w.len() >= 5 {
        return undouble(&strip(2));
    }
    if (w.ends_with("ches") || w.ends_with("shes") || w.ends_with("xes") || w.ends_with("zes"))
        && w.len() > 4
    {
        return strip(2);
    }
    if w.ends_with('s') && !w.ends_with("ss") && !w.ends_with("us") && !w.ends_with("is") {
        return strip(1);
    }
    w.to_string()
}

/// `stopped` → `stopp` → `stop`; keeps `ll`, `ss`, `ff` (`called` → `call`).
fn undouble(s: &str) -> String {
    let b = s.as_bytes();
    if b.len() >= 3 {
        let (x, y) = (b[b.len() - 1], b[b.len() - 2]);
        if x == y && !matches!(x, b'l' | b's' | b'f' | b'z') {
            return s[..s.len() - 1].to_string();
        }
    }
    s.to_string()
}

/// Levenshtein distance if it is at most `max`, else `None`. Stops early once every value in
/// the current row exceeds `max`, so comparing a word with a whole vocabulary stays cheap.
#[must_use]
pub fn bounded_distance(a: &[char], b: &[char], max: usize) -> Option<usize> {
    if a.len().abs_diff(b.len()) > max {
        return None;
    }
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    let mut cur = vec![0; b.len() + 1];
    for (i, ca) in a.iter().enumerate() {
        cur[0] = i + 1;
        let mut row_min = cur[0];
        for (j, cb) in b.iter().enumerate() {
            let cost = usize::from(ca != cb);
            cur[j + 1] = (prev[j] + cost).min(prev[j + 1] + 1).min(cur[j] + 1);
            row_min = row_min.min(cur[j + 1]);
        }
        if row_min > max {
            return None;
        }
        std::mem::swap(&mut prev, &mut cur);
    }
    let d = prev[b.len()];
    (d <= max).then_some(d)
}

/// Similarity 0.0–1.0 from an edit distance: 1 − distance ÷ longer length.
#[must_use]
pub fn similarity(distance: usize, a_len: usize, b_len: usize) -> f32 {
    let longest = a_len.max(b_len).max(1);
    #[allow(clippy::cast_precision_loss)]
    let s = 1.0 - distance as f32 / longest as f32;
    s
}

/// Largest edit distance that still reaches `min_similarity` for a word of `len` letters.
#[must_use]
pub fn max_distance(len: usize, min_similarity: f32) -> usize {
    #[allow(
        clippy::cast_precision_loss,
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss
    )]
    // small tolerance: (1 − 0.8) × 10 is 1.9999999 in f32, which must allow 2 edits
    let d = ((1.0 - min_similarity.clamp(0.0, 1.0)) * len as f32 + 1e-4).floor() as usize;
    d
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chars(s: &str) -> Vec<char> {
        s.chars().collect()
    }

    #[test]
    fn stems_plurals_and_verb_forms() {
        assert_eq!(stem("equations"), stem("equation"));
        assert_eq!(stem("solving"), "solv");
        assert_eq!(stem("solve"), "solv");
        assert_eq!(stem("phases"), stem("phase"));
        assert_eq!(stem("solved"), "solv");
        assert_eq!(stem("studies"), stem("study"));
        assert_eq!(stem("stopped"), "stop");
        assert_eq!(stem("boxes"), "box");
        assert_eq!(stem("class"), "class");
        assert_eq!(stem("analysis"), "analysis");
        assert_eq!(stem("net"), "net");
    }

    #[test]
    fn bounded_distance_basics() {
        assert_eq!(
            bounded_distance(&chars("navier"), &chars("navier"), 1),
            Some(0)
        );
        assert_eq!(
            bounded_distance(&chars("navier"), &chars("naiver"), 2),
            Some(2)
        );
        assert_eq!(
            bounded_distance(&chars("stokes"), &chars("strokes"), 1),
            Some(1)
        );
        assert_eq!(bounded_distance(&chars("network"), &chars("not"), 2), None);
        assert_eq!(
            bounded_distance(&chars("modulation"), &chars("modulat1on"), 1),
            Some(1)
        );
    }

    #[test]
    fn similarity_and_budget() {
        assert!((similarity(1, 10, 10) - 0.9).abs() < 1e-6);
        assert_eq!(max_distance(10, 0.8), 2);
        assert_eq!(max_distance(4, 0.8), 0);
        assert_eq!(max_distance(5, 1.0), 0);
    }
}
