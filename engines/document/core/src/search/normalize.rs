//! Character normalisation for search: every original character maps to zero or more search
//! characters, so a position in the search text always maps back to the paper's text.
//!
//! Folding applied to the *key* form (used for lookups):
//! - Unicode compatibility decomposition (NFKD): ligatures `ﬁ`→`fi`, math alphanumerics `𝐶`→`C`,
//!   subscripts/superscripts `ₚ`→`p`, `²`→`2`, Arabic presentation forms → base letters;
//! - combining marks removed (accents, Arabic short vowels);
//! - lower case;
//! - Persian/Arabic letters unified (`ي`→`ی`, `ك`→`ک`, `ى`→`ی`), tatweel and zero-width non-joiner removed;
//! - Arabic-Indic and Persian digits → ASCII digits.
//!
//! The *raw* form keeps case and accents (decomposed), for case- or accent-sensitive matching.

use unicode_normalization::char::{decompose_compatible, is_combining_mark};

/// What a character is for tokenisation after normalisation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Class {
    /// Part of a word: letter or digit.
    Word,
    /// Breaks words; remembered as the separator before the next word.
    Sep(Sep),
    /// Dropped without breaking the word (zero-width non-joiner, tatweel).
    Skip,
    /// `PDFium`'s soft-hyphen marker `\x02` or U+00AD: the word continues after the line break.
    Join,
}

/// Separator between two words, as far as notation matching cares.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sep {
    /// Start of the text.
    None,
    /// Space or tab.
    Space,
    /// Line or paragraph break.
    Break,
    /// `_` (subscript in TeX-like writing).
    Underscore,
    /// `^` (superscript).
    Caret,
    /// `{` or `}`.
    Brace,
    /// Hyphen, dash or slash (`Navier-Stokes`, `input/output`).
    Hyphen,
    /// Anything else (punctuation, symbols).
    Other,
}

/// Classify an original character.
#[must_use]
pub fn class(c: char) -> Class {
    match c {
        '\u{200C}' | '\u{200D}' | '\u{0640}' | '\u{FEFF}' => Class::Skip,
        '\u{0002}' | '\u{00AD}' => Class::Join,
        ' ' | '\t' | '\u{00A0}' | '\u{2009}' | '\u{202F}' => Class::Sep(Sep::Space),
        '\r' | '\n' | '\u{000C}' => Class::Sep(Sep::Break),
        '_' => Class::Sep(Sep::Underscore),
        '^' => Class::Sep(Sep::Caret),
        '{' | '}' => Class::Sep(Sep::Brace),
        '-' | '/' | '\u{2010}'..='\u{2015}' | '\u{2212}' => Class::Sep(Sep::Hyphen),
        c if c.is_alphanumeric() || is_combining_mark(c) => Class::Word,
        // compatibility forms that decompose to letters/digits (e.g. ₚ, ², ①) are words too
        c if decomposes_to_alnum(c) => Class::Word,
        _ => Class::Sep(Sep::Other),
    }
}

fn decomposes_to_alnum(c: char) -> bool {
    let mut any = false;
    decompose_compatible(c, |d| any |= d.is_alphanumeric());
    any
}

fn unify(c: char) -> Option<char> {
    Some(match c {
        '\u{064A}' | '\u{0649}' => '\u{06CC}', // Arabic yeh / alef maksura → Persian yeh
        '\u{0643}' => '\u{06A9}',              // Arabic kaf → Persian keheh
        '\u{0660}'..='\u{0669}' => char::from_u32('0' as u32 + (c as u32 - 0x0660))?,
        '\u{06F0}'..='\u{06F9}' => char::from_u32('0' as u32 + (c as u32 - 0x06F0))?,
        '\u{2010}'..='\u{2015}' | '\u{2212}' => '-',
        _ => c,
    })
}

/// Push the search characters of one original character: `key` form (folded, lower case) and
/// `raw` form (decomposed, case and accents kept).
pub fn fold(c: char, key: &mut Vec<char>, raw: &mut Vec<char>) {
    decompose_compatible(c, |d| {
        if is_combining_mark(d) {
            raw.push(d);
            return;
        }
        let u = unify(d).unwrap_or(d);
        raw.push(u);
        for l in u.to_lowercase() {
            key.push(l);
        }
    });
}

/// Fold a whole string to its key form (used for query words).
#[must_use]
pub fn key_of(s: &str) -> String {
    let (mut k, mut r) = (Vec::new(), Vec::new());
    for c in s.chars() {
        fold(c, &mut k, &mut r);
    }
    k.into_iter().collect()
}

/// The comparison form of a raw string under the case / accent options.
#[must_use]
pub fn form(raw: &str, case_sensitive: bool, fold_accents: bool) -> String {
    raw.chars()
        .filter(|c| !(fold_accents && is_combining_mark(*c)))
        .flat_map(|c| {
            if case_sensitive {
                vec![c]
            } else {
                c.to_lowercase().collect()
            }
        })
        .collect()
}

/// Greek letter for its English name (`alpha` → `α`), and back, for notation matching.
#[must_use]
pub fn greek_alias(key: &str) -> Option<&'static str> {
    const PAIRS: [(&str, &str); 24] = [
        ("alpha", "α"),
        ("beta", "β"),
        ("gamma", "γ"),
        ("delta", "δ"),
        ("epsilon", "ε"),
        ("zeta", "ζ"),
        ("eta", "η"),
        ("theta", "θ"),
        ("iota", "ι"),
        ("kappa", "κ"),
        ("lambda", "λ"),
        ("mu", "μ"),
        ("nu", "ν"),
        ("xi", "ξ"),
        ("omicron", "ο"),
        ("pi", "π"),
        ("rho", "ρ"),
        ("sigma", "σ"),
        ("tau", "τ"),
        ("upsilon", "υ"),
        ("phi", "φ"),
        ("chi", "χ"),
        ("psi", "ψ"),
        ("omega", "ω"),
    ];
    let alt = match key {
        "ϵ" => "ε",
        "ϑ" => "θ",
        "ϕ" => "φ",
        "ς" => "σ",
        _ => key,
    };
    PAIRS.iter().find_map(|(n, l)| {
        if *n == alt {
            Some(*l)
        } else if *l == alt {
            Some(*n)
        } else {
            None
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn folds_math_letters_subscripts_and_ligatures() {
        assert_eq!(key_of("𝐶ₚ"), "cp");
        assert_eq!(key_of("x²"), "x2");
        assert_eq!(key_of("ﬁnite"), "finite");
        assert_eq!(key_of("Schrödinger"), "schrodinger");
    }

    #[test]
    fn unifies_persian_and_arabic_forms() {
        assert_eq!(key_of("شبكه"), key_of("شبکه"));
        assert_eq!(key_of("ي"), key_of("ی"));
        assert_eq!(key_of("۱۲۳"), "123");
        assert_eq!(key_of("ﺷﺒﻜﻪ"), key_of("شبکه")); // presentation forms
    }

    #[test]
    fn classes() {
        assert_eq!(class('a'), Class::Word);
        assert_eq!(class('ₚ'), Class::Word);
        assert_eq!(class('_'), Class::Sep(Sep::Underscore));
        assert_eq!(class('\u{200C}'), Class::Skip);
        assert_eq!(class('\u{0002}'), Class::Join);
        assert_eq!(class('('), Class::Sep(Sep::Other));
    }

    #[test]
    fn greek_names_both_ways() {
        assert_eq!(greek_alias("alpha"), Some("α"));
        assert_eq!(greek_alias("α"), Some("alpha"));
        assert_eq!(greek_alias("ϕ"), Some("phi"));
        assert_eq!(greek_alias("word"), None);
    }
}
