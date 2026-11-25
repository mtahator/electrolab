//! What a paper says its symbols mean (P1.M5.UND.4): "where ρ is the density, u the velocity and p
//! the pressure", lists of symbols, "the heat transfer coefficient h". Found by code, taken word for
//! word.

use std::sync::LazyLock;

use regex::Regex;

pub use crate::latex::{Symbol, symbols as from_latex};
use crate::{latex, numbers, units};

/// Where a paper says what a symbol means.
#[derive(Debug, Clone, PartialEq)]
pub struct Definition {
    /// Letters and digits of the symbol.
    pub key: String,
    /// As it is printed.
    pub printed: String,
    /// It has a mark over it.
    pub accent: bool,
    /// The meaning, word for word (white space collapsed).
    pub meaning: String,
    /// Its unit, when one is printed with it.
    pub unit: Option<String>,
    /// The sentence.
    pub quote: String,
    /// Where the symbol stands in the text (characters).
    pub at: usize,
    /// It is said ("ρ is the density"), not only named ("the density ρ").
    pub said: bool,
}

/// A symbol as it is printed: a letter with marks, and a subscript close to it.
const S: &str = r"[\u{300}-\u{308}\u{20d7}ˆ]?[A-Za-z\u{391}-\u{3c9}\u{2206}ℓ][\u{300}-\u{308}\u{20d7}ˆ′'*]{0,2}(?:_?\{?[A-Za-z0-9\u{3b1}-\u{3c9}∞,]{1,9}\}?|[ ][A-Za-z0-9\u{3b1}-\u{3c9}]{1,4})??";
const VERB: &str = r"(?:is|are|denotes?|denoting|represents?|representing|stands? for|means|refers? to|indicates?|describes?|being)";
const ARTICLE: &str = r"(?:the|a|an|its|their|our)";

/// Words that begin no meaning ("where T is given by …").
const NO_MEANING: [&str; 34] = [
    "given",
    "defined",
    "used",
    "set",
    "chosen",
    "computed",
    "obtained",
    "calculated",
    "not",
    "also",
    "then",
    "now",
    "known",
    "unknown",
    "large",
    "small",
    "equal",
    "called",
    "assumed",
    "fixed",
    "positive",
    "negative",
    "true",
    "shown",
    "described",
    "said",
    "determined",
    "taken",
    "evaluated",
    "considered",
    "estimated",
    "only",
    "to",
    "in",
];

/// A letter of the mathematical alphabets.
fn math(c: char) -> bool {
    ('\u{1d400}'..='\u{1d7ff}').contains(&c)
}

/// The text in one spelling, with the place every byte came from.
struct Norm {
    text: String,
    from: Vec<usize>,
    chars: Vec<char>,
}

impl Norm {
    fn new(text: &str) -> Self {
        use unicode_normalization::UnicodeNormalization;
        let chars: Vec<char> = text.chars().collect();
        let mut out = String::new();
        let mut from = Vec::new();
        let mut push = |c: char, i: usize, out: &mut String| {
            out.push(c);
            from.extend(std::iter::repeat_n(i, c.len_utf8()));
        };
        let mut i = 0;
        while i < chars.len() {
            let c = chars[i];
            // A word cut at the end of a line.
            if c == '-' && i > 0 && chars[i - 1].is_lowercase() {
                let mut k = i + 1;
                while chars.get(k).is_some_and(|c| c.is_whitespace()) {
                    k += 1;
                }
                if k > i + 1
                    && chars[i + 1..k].contains(&'\n')
                    && chars.get(k).is_some_and(|c| c.is_lowercase())
                {
                    i = k;
                    continue;
                }
            }
            match c {
                '\u{2d9}' => push('\u{307}', i, &mut out),
                '\u{af}' | '\u{2c9}' => push('\u{304}', i, &mut out),
                '\u{2dc}' => push('\u{303}', i, &mut out),
                '\n' => push('\n', i, &mut out),
                c if c.is_whitespace() => push(' ', i, &mut out),
                c => {
                    for n in c.to_string().nfkd() {
                        push(n, i, &mut out);
                    }
                }
            }
            i += 1;
        }
        Norm {
            text: out,
            from,
            chars,
        }
    }

    /// Where a symbol in mathematical letters begins whose subscript is cut by spaces ("𝑣𝑢 𝑝").
    fn begin_of(&self, at: usize) -> usize {
        let Some(&first) = self.from.get(at) else {
            return at;
        };
        let is = |i: usize| self.chars.get(i).is_some_and(|c| math(*c));
        let mut i = first;
        if !is(i) {
            return at;
        }
        while i > 1 && (is(i - 1) || (self.chars[i - 1] == ' ' && is(i - 2))) {
            i -= 1;
        }
        // Two or three letters with a space between; more is a formula.
        if first - i > 6 {
            return at;
        }
        self.from.partition_point(|f| *f < i)
    }

    /// The text as it is printed, between two bytes; words cut at line ends joined.
    fn printed(&self, a: usize, b: usize) -> String {
        let start = self.from.get(a).copied().unwrap_or(self.chars.len());
        let end = if b == 0 {
            start
        } else {
            self.from.get(b - 1).map_or(self.chars.len(), |i| i + 1)
        };
        let raw: String = self
            .chars
            .get(start..end.max(start))
            .unwrap_or(&[])
            .iter()
            .collect();
        join(&raw)
    }
}

/// One line: white space collapsed, words cut at line ends joined.
fn join(raw: &str) -> String {
    static CUT: LazyLock<Regex> = LazyLock::new(|| crate::re(r"(\p{Ll})-\s*\n\s*(\p{Ll})"));
    CUT.replace_all(raw, "$1$2")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// Short words that look like a symbol.
const WORDS: [&str; 34] = [
    "of", "in", "to", "by", "on", "at", "for", "and", "or", "as", "with", "from", "per", "is",
    "are", "be", "we", "it", "an", "a", "the", "that", "than", "all", "if", "so", "out", "up",
    "via", "but", "not", "no", "into", "this",
];

/// Whether this is a symbol rather than a word.
fn symbolic(s: &str) -> bool {
    let s = s.trim();
    if s.split(' ').any(|w| WORDS.contains(&w)) {
        return false;
    }
    let long = s.chars().filter(|c| c.is_alphabetic()).count() > 2;
    !long || s.chars().any(|c| !c.is_ascii_lowercase() && c != ' ')
}

/// Where a meaning that begins at `a` ends.
fn end_of(t: &str, a: usize) -> usize {
    static NEXT: LazyLock<Regex> = LazyLock::new(|| {
        crate::re(&format!(
            r"^\s+(?:and\s+|while\s+)?(?P<s>{S})\s+(?:{VERB}\s|{ARTICLE}\s)"
        ))
    });
    static STOP: LazyLock<Regex> = LazyLock::new(|| {
        crate::re(
            r"^\s+(?:which|that|where|whereas|while|respectively|such as|defined|given by|and (?:is|are|can|has)|is|are|can|will|was|were|has|have)\b",
        )
    });
    let rest = &t[a..];
    let mut words = 0;
    let mut depth = 0i32;
    let mut blank = false;
    for (i, c) in rest.char_indices() {
        match c {
            '(' | '[' => depth += 1,
            ')' | ']' => {
                depth -= 1;
                if depth < 0 {
                    return a + i;
                }
            }
            ',' | ';' | ':' if depth == 0 => return a + i,
            '.' if depth == 0 && rest[i + 1..].chars().next().is_none_or(char::is_whitespace) => {
                return a + i;
            }
            c if c.is_whitespace() && depth <= 0 => {
                words += u32::from(!blank);
                blank = true;
                if words > 16 || i > 160 {
                    return a + i;
                }
                let after = &rest[i..];
                let next = NEXT
                    .captures(after)
                    .and_then(|c| c.name("s"))
                    .is_some_and(|s| symbolic(s.as_str()));
                if after.starts_with("\n\n") || STOP.is_match(after) || next {
                    return a + i;
                }
                continue;
            }
            // A bracket that never closes.
            _ if depth > 0 && i > 200 => return a + i,
            _ => {}
        }
        blank = false;
    }
    t.len()
}

/// A unit that follows the meaning after a comma ("the density, kg/m3").
fn unit_after(t: &str, end: usize) -> Option<String> {
    static NEXT: LazyLock<Regex> = LazyLock::new(|| {
        crate::re(
            r"^\s*,?\s*[(\[]?\s*([^,;()\[\]]{1,16}?(?:\([^()]{1,8}\))?)\s*(?:[)\],;]|\.\s|\n|$)",
        )
    });
    let u = NEXT.captures(&t[end..])?[1].to_string();
    let long = u.chars().count() > 1 || u == "K";
    (long && u.chars().any(char::is_alphabetic) && units::parse(&u).is_some()).then_some(u)
}

/// The sentence around a place, cut to a readable length.
fn sentence(n: &Norm, a: usize, b: usize) -> String {
    let before = n.text[..a]
        .rfind(". ")
        .or_else(|| n.text[..a].rfind(".\n"))
        .map_or(0, |i| i + 2);
    let after = n.text[b..]
        .find(". ")
        .or_else(|| n.text[b..].find(".\n"))
        .map_or(n.text.len(), |i| b + i + 1);
    // A list of symbols has no sentences: its line then.
    let near = |at: usize, back: bool| {
        if back {
            n.text[..at].rfind('\n').map_or(0, |i| i + 1)
        } else {
            n.text[at..].find('\n').map_or(n.text.len(), |i| at + i)
        }
    };
    let start = if a - before > 220 {
        near(a, true)
    } else {
        before
    };
    let end = if after - b > 220 {
        near(b, false)
    } else {
        after
    };
    n.printed(start, end)
}

struct Found<'a> {
    n: &'a Norm,
    out: Vec<Definition>,
}

impl Found<'_> {
    fn push(&mut self, s: (usize, usize), m: (usize, usize), said: bool) {
        let s = (self.n.begin_of(s.0), s.1);
        let printed = self.n.printed(s.0, s.1);
        let meaning = self.n.printed(m.0, m.1);
        let meaning = meaning.trim_matches([' ', ',', ';', ':']).to_string();
        let letters = meaning.chars().filter(|c| c.is_alphabetic()).count();
        // A formula is no meaning: words are printed in ordinary letters.
        let word = meaning
            .split(|c: char| !c.is_alphabetic() || math(c))
            .any(|w| w.chars().count() > 2);
        let first = meaning.split(' ').next().unwrap_or("").to_lowercase();
        if letters < 3 || !word || NO_MEANING.contains(&first.as_str()) {
            return;
        }
        let key = latex::key(&printed);
        if key.is_empty() {
            return;
        }
        let unit = units::in_meaning(&meaning).or_else(|| unit_after(&self.n.text, m.1));
        self.out.push(Definition {
            key,
            accent: self.n.text[s.0..s.1]
                .chars()
                .any(|c| ('\u{300}'..='\u{308}').contains(&c) || c == '\u{20d7}' || c == 'ˆ'),
            printed,
            meaning,
            unit,
            quote: sentence(self.n, s.0, m.1),
            at: self.n.from.get(s.0).copied().unwrap_or(0),
            said,
        });
    }
}

/// What a text says its symbols mean. Words that happen to look like a symbol are in it too: the
/// symbols looked for decide.
#[must_use]
pub fn definitions(text: &str) -> Vec<Definition> {
    static SAID: LazyLock<Regex> = LazyLock::new(|| {
        crate::re(&format!(
            r"(?:^|[\s(,;])(?P<s>{S})\s*(?:,\s*)?\s(?P<v>{VERB})\s+(?P<a>{ARTICLE}\s+)?(?P<m>\S)"
        ))
    });
    static BOTH: LazyLock<Regex> = LazyLock::new(|| {
        crate::re(&format!(
            r"(?:^|[\s(,;])(?P<l>(?:{S},\s+){{0,5}}{S},?\s+and\s+{S})\s+(?:are|denote|represent|stand for|mean|indicate|refer to)\s+(?P<m>[^.;]{{3,220}})"
        ))
    });
    static NEXT: LazyLock<Regex> = LazyLock::new(|| {
        crate::re(&format!(
            r"^\s*[,;]?\s*(?:and\s+)?(?P<s>{S})\s+(?:{VERB}\s+)?(?P<a>{ARTICLE})\s+(?P<m>\S)"
        ))
    });
    static ITEM: LazyLock<Regex> = LazyLock::new(|| {
        crate::re(&format!(
            r"(?m)(?:^|[,;]\s*|where\s+)(?P<s>{S})\s*(?:[:=]|\s[\u{{2013}}\u{{2014}}-])\s+(?P<m>[a-z][a-z-]{{2,}})"
        ))
    });
    static NAMED: LazyLock<Regex> = LazyLock::new(|| {
        crate::re(&format!(
            r"\b(?:the|a|an|of|and|with)\s+(?P<m>(?:[a-z-]+\s+){{0,3}}[a-z-]{{4,}})\s+(?P<s>{S})(?P<e>[,.;:)\]]|\s+(?:is|are|and|at|in|of|for|as|to|with|from|by|can|was)\b|\s+\()"
        ))
    });
    let n = Norm::new(text);
    let t = n.text.as_str();
    let mut f = Found { n: &n, out: vec![] };
    let mut taken: Vec<(usize, usize)> = vec![];

    // "a, b and c are the x, y and z, respectively"
    for c in BOTH.captures_iter(t) {
        let (Some(l), Some(m)) = (c.name("l"), c.name("m")) else {
            continue;
        };
        let names = pieces(l.as_str(), l.start());
        let said = m.as_str();
        taken.push((l.start(), l.end()));
        if let Some(k) = said.find("respectively") {
            let body = said[..k].trim_end().trim_end_matches(',');
            let means = pieces(body, m.start());
            if means.len() == names.len() {
                for (s, mean) in names.iter().zip(&means) {
                    f.push(*s, *mean, true);
                }
            }
        } else {
            let end = end_of(t, m.start());
            for s in &names {
                f.push(*s, (m.start(), end), true);
            }
        }
    }
    // "ρ is the density", and what goes on after it: ", u the velocity and p the pressure"
    for c in SAID.captures_iter(t) {
        let (Some(s), Some(v), Some(m)) = (c.name("s"), c.name("v"), c.name("m")) else {
            continue;
        };
        if taken.iter().any(|(a, b)| s.start() < *b && s.end() > *a) {
            continue;
        }
        let begin = c.name("a").map_or(m.start(), |a| a.start());
        // "T is large" is no meaning: without an article only after "where", or with a verb of naming.
        let before = t[..s.start()].trim_end().to_lowercase();
        let led = ["where", "here,", "with", "and", ",", "which"]
            .iter()
            .any(|w| before.ends_with(w));
        if c.name("a").is_none() && matches!(v.as_str(), "is" | "are" | "being") && !led {
            continue;
        }
        let mut end = end_of(t, begin);
        f.push((s.start(), s.end()), (begin, end), true);
        for _ in 0..24 {
            let Some(k) = NEXT.captures(&t[end..]) else {
                break;
            };
            let (Some(s), Some(a)) = (k.name("s"), k.name("a")) else {
                break;
            };
            if !symbolic(s.as_str()) {
                break;
            }
            let from = end;
            let begin = from + a.start();
            end = end_of(t, begin);
            f.push((from + s.start(), from + s.end()), (begin, end), true);
        }
    }
    // "k: thermal conductivity (W/m K)", "k = thermal conductivity"
    for c in ITEM.captures_iter(t) {
        if let (Some(s), Some(m)) = (c.name("s"), c.name("m")) {
            let end = end_of(t, m.start());
            // "x = and so on": an item has a name of some length.
            f.push((s.start(), s.end()), (m.start(), end), true);
        }
    }
    listed(&n, &mut f);
    // "the heat transfer coefficient h,"
    for c in NAMED.captures_iter(t) {
        if let (Some(s), Some(m)) = (c.name("s"), c.name("m")) {
            let key = latex::key(s.as_str());
            // Longer names look like words here.
            if key.chars().count() <= 3 {
                f.push((s.start(), s.end()), (m.start(), m.end()), false);
            }
        }
    }
    f.out
}

/// The pieces of "a, b and c", with their places.
fn pieces(list: &str, at: usize) -> Vec<(usize, usize)> {
    static CUT: LazyLock<Regex> = LazyLock::new(|| crate::re(r",\s+and\s+|,\s+|\s+and\s+"));
    let mut out = vec![];
    let mut start = 0;
    for m in CUT.find_iter(list) {
        out.push((at + start, at + m.start()));
        start = m.end();
    }
    out.push((at + start, at + list.len()));
    out
}

/// The lines of a list of symbols ("Nomenclature").
fn listed(n: &Norm, f: &mut Found<'_>) {
    static HEADING: LazyLock<Regex> = LazyLock::new(|| {
        crate::re(
            r"(?mi)^\s*(?:\d+\.?\s*)?(?:nomenclature|list of symbols|symbols|notations?|latin symbols|greek symbols|greek letters)\s*$",
        )
    });
    static LINE: LazyLock<Regex> = LazyLock::new(|| {
        crate::re(&format!(
            r"^\s*(?P<s>{S})\s*[,:=]?\s+(?P<m>[A-Za-z][A-Za-z-]{{3,}}[^\n]*)$"
        ))
    });
    let t = n.text.as_str();
    for h in HEADING.find_iter(t) {
        let mut at = h.end();
        let mut misses = 0;
        for line in t[h.end()..].split_inclusive('\n').take(120) {
            let body = line.trim_end_matches('\n');
            if let Some(c) = LINE.captures(body)
                && let (Some(s), Some(m)) = (c.name("s"), c.name("m"))
            {
                misses = 0;
                let end = at + m.start() + m.as_str().trim_end().len();
                f.push((at + s.start(), at + s.end()), (at + m.start(), end), true);
            } else if !body.trim().is_empty() {
                misses += 1;
            }
            at += line.len();
            if misses > 12 {
                break;
            }
        }
    }
}

/// Whether a symbol has a star or a prime ("Q*", "x′"): it is another one than without it.
fn starred(s: &str) -> bool {
    s.contains(['*', '\u{2217}', '\u{2032}', '\'', '\u{2019}', '\u{2020}'])
}

/// The definition of a symbol: one that is said before one that is only named, then the nearest to
/// `near`. A mark over the symbol that the text lost is forgiven unless `all` has the same letters
/// with and without it.
#[must_use]
pub fn meaning<'a>(
    symbol: &Symbol,
    all: &[Symbol],
    definitions: &'a [Definition],
    near: usize,
) -> Option<&'a Definition> {
    let twins = all.iter().filter(|o| o.key == symbol.key).count() > 1;
    definitions
        .iter()
        .filter(|d| d.key == symbol.key)
        .filter(|d| d.accent == symbol.accent || (symbol.accent && !twins))
        .filter(|d| {
            let star = starred(&symbol.shown);
            starred(&d.printed) == star || (star && !twins)
        })
        .min_by_key(|d| {
            let marks = d.accent != symbol.accent || starred(&d.printed) != starred(&symbol.shown);
            (!d.said, marks, d.at.abs_diff(near))
        })
}

/// The defined symbols that stand in the printed text of an equation, each once.
#[must_use]
pub fn in_text<'a>(
    equation: &str,
    definitions: &'a [Definition],
    near: usize,
) -> Vec<&'a Definition> {
    let words = words(equation);
    let spaceless: String = words.concat();
    let mut out: Vec<&Definition> = vec![];
    for d in definitions.iter().filter(|d| d.said) {
        let one = d.key.chars().count() == 1;
        let stands = words.contains(&d.key)
            || (one
                && words.iter().any(|w| {
                    w.chars().count() <= 3
                        && w.contains(d.key.as_str())
                        && !latex::FUNCTIONS.contains(&w.to_lowercase().as_str())
                }))
            || (!one && d.key.chars().count() <= 6 && spaceless.contains(&d.key));
        // "it is …": no equation has the word; its letters may stand side by side.
        let word = !one && WORDS.contains(&d.printed.to_lowercase().as_str());
        if !stands || word || (starred(&d.printed) && !starred(equation)) {
            continue;
        }
        match out.iter_mut().find(|o| {
            o.key == d.key && o.accent == d.accent && starred(&o.printed) == starred(&d.printed)
        }) {
            Some(o) if d.at.abs_diff(near) < o.at.abs_diff(near) => *o = d,
            Some(_) => {}
            None => out.push(d),
        }
    }
    out.sort_by_key(|d| spaceless.find(&d.key));
    out
}

fn words(text: &str) -> Vec<String> {
    text.split(|c: char| !c.is_alphanumeric())
        .map(latex::key)
        .filter(|w| !w.is_empty())
        .collect()
}

/// Whether a meaning somebody else found (a model) stands in the text word for word, in a sentence
/// that has the symbol; the sentence.
#[must_use]
pub fn stands(text: &str, symbol: &Symbol, meaning: &str) -> Option<String> {
    let flat = join(text);
    let wanted = join(meaning);
    if wanted.chars().filter(|c| c.is_alphabetic()).count() < 4 {
        return None;
    }
    numbers::sentences(&flat)
        .into_iter()
        .find(|s| {
            s.contains(&wanted) && {
                let w = words(s);
                w.contains(&symbol.key)
                    || (symbol.key.chars().count() > 1
                        && w.windows(2).any(|p| p.concat() == symbol.key))
            }
        })
        .map(|s| s.trim().to_string())
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn found(text: &str) -> Vec<(String, String)> {
        definitions(text)
            .into_iter()
            .map(|d| (d.key, d.meaning))
            .collect()
    }
    fn has(text: &str, key: &str, meaning: &str) {
        let f = found(text);
        assert!(
            f.iter().any(|(k, m)| k == key && m == meaning),
            "{key} {meaning}: {f:?}"
        );
    }

    #[test]
    fn a_where_sentence_gives_every_symbol() {
        let t = "where ρ is the density, u the velocity vector, and p the pressure. Next.";
        has(t, "ρ", "the density");
        has(t, "u", "the velocity vector");
        has(t, "p", "the pressure");
        let t = "Here, Tin and Tout denote the inlet and outlet temperature, respectively, and cp is the specific heat\ncapac-\nity of water (J/kgK).";
        has(t, "cp", "the specific heat capacity of water (J/kgK)");
        let d = definitions(t);
        assert_eq!(
            d.iter().find(|d| d.key == "cp").unwrap().unit.as_deref(),
            Some("J/kgK")
        );
        let t = "where λ1 and λ2 are the weights of the data and physics loss. The";
        has(t, "λ1", "the weights of the data and physics loss");
        has(
            "where T and h are the temperature and enthalpy, respectively.",
            "h",
            "enthalpy",
        );
    }

    #[test]
    fn other_ways_to_name_a_symbol() {
        has(
            "in which 𝛼 denotes the thermal diffusivity in m2/s.",
            "α",
            "the thermal diffusivity in m2/s",
        );
        has(
            "with the heat transfer coefficient h, the area",
            "h",
            "the heat transfer coefficient",
        );
        has(
            "where k: thermal conductivity, W/(m K); x",
            "k",
            "thermal conductivity",
        );
        let d = definitions("where k: thermal conductivity, W/(m K); x");
        assert_eq!(d[0].unit.as_deref(), Some("W/(m K)"));
        has(
            "The mass flow m˙ is the flow of water.",
            "m",
            "the flow of water",
        );
        assert!(definitions("The mass flow m˙ is the flow of water.")[0].accent);
        has(
            "where T in is the supply temperature.",
            "Tin",
            "the supply temperature",
        );
    }

    #[test]
    fn symbols_in_mathematical_letters() {
        let t = "where 𝑣𝑢 𝑝 is the velocity before the shock and 𝑝𝑖 is the product 𝑛𝑖 𝑘.";
        has(t, "vup", "the velocity before the shock");
        assert!(!found(t).iter().any(|(k, _)| k == "p"), "{:?}", found(t));
        let t = "where 𝛿𝑖 𝑗 is the Kronecker delta.";
        has(t, "δij", "the Kronecker delta");
        let t = "so that 𝑝𝑖 = 𝑛𝑖 𝑘 and 𝜇 is 𝑚𝑒 𝑛𝑒 + 𝑚𝑖.";
        assert_eq!(found(t), [], "a formula is no meaning");
    }

    #[test]
    fn a_list_of_symbols_is_read_by_lines() {
        let t = "Nomenclature\nA area (m2)\ncp specific heat capacity [J/(kg K)]\nη efficiency\n\nIntroduction\nThe house";
        has(t, "A", "area (m2)");
        has(t, "η", "efficiency");
        let d = definitions(t);
        assert_eq!(
            d.iter().find(|d| d.key == "cp").unwrap().unit.as_deref(),
            Some("J/(kg K)")
        );
    }

    #[test]
    fn a_property_is_no_meaning() {
        for t in [
            "When T is large, the flow stops.",
            "where T is given by the law.",
            "so that u is equal to zero.",
        ] {
            assert!(
                !found(t).iter().any(|(k, _)| k == "T" || k == "u"),
                "{t}: {:?}",
                found(t)
            );
        }
    }

    #[test]
    fn the_nearest_definition_that_is_said_is_taken() {
        let t = "the time T, and later T is the temperature. Much later, T denotes the total thickness of it.";
        let d = definitions(t);
        let s = from_latex("T");
        let m = |near: usize| meaning(&s[0], &s, &d, near).unwrap().meaning.clone();
        assert_eq!(m(0), "the temperature");
        assert_eq!(m(t.len()), "the total thickness of it");
    }

    #[test]
    fn a_mark_is_not_taken_for_its_letter() {
        let d = definitions("where m˙ is the mass flow rate and m is the mass.");
        let both = from_latex(r"\dot{m} m");
        assert_eq!(
            meaning(&both[0], &both, &d, 0).unwrap().meaning,
            "the mass flow rate"
        );
        assert_eq!(meaning(&both[1], &both, &d, 0).unwrap().meaning, "the mass");
        // The mark was lost in the text: taken only when there is one such symbol.
        let d = definitions("where m is the mass flow rate.");
        let one = from_latex(r"\dot{m}");
        assert!(meaning(&one[0], &one, &d, 0).is_some());
        assert!(meaning(&both[0], &both, &d, 0).is_none());
    }

    #[test]
    fn symbols_of_a_printed_equation() {
        let d = definitions(
            "where ρ is the density, u the velocity, n the number of nodes and q the heat flux.",
        );
        let keys: Vec<&str> = in_text("ρ ∂u ∂t = − sin(x)", &d, 0)
            .iter()
            .map(|d| d.key.as_str())
            .collect();
        assert_eq!(keys, ["ρ", "u"], "the n of sin is none, q is not there");
    }

    #[test]
    fn a_word_is_no_symbol_of_an_equation() {
        let d = definitions("where it is the better way and a is the speed of sound.");
        let keys: Vec<&str> = in_text("u_i t = a", &d, 0)
            .iter()
            .map(|d| d.key.as_str())
            .collect();
        assert_eq!(keys, ["a"]);
    }

    #[test]
    fn a_star_makes_another_symbol() {
        let d = definitions("where Q* is the conjugate of it and z is the distance.");
        let shown = |e: &str| -> Vec<String> {
            in_text(e, &d, 0)
                .iter()
                .map(|d| d.printed.clone())
                .collect()
        };
        assert_eq!(shown("iQ + z = 0"), ["z"]);
        assert_eq!(shown("iQ* + z = 0"), ["Q*", "z"]);
        let plain = from_latex("Q");
        assert!(meaning(&plain[0], &plain, &d, 0).is_none());
        let star = from_latex("Q^{*}");
        assert!(meaning(&star[0], &star, &d, 0).is_some());
    }

    #[test]
    fn what_a_model_found_must_stand_there() {
        let s = from_latex("U");
        let t = "We use the overall coefficient U of the wall. The loss is small.";
        assert!(stands(t, &s[0], "overall coefficient").is_some());
        assert!(stands(t, &s[0], "heat transfer coefficient").is_none());
        assert!(
            stands(t, &s[0], "The loss").is_none(),
            "not where the symbol is"
        );
    }
}
