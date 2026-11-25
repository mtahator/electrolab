//! Units as they are printed: "W/(m²·K)", "J kg−1 K−1", "mm2/s", "◦C", "%".

use std::sync::LazyLock;

use regex::Regex;

/// Exponents of metre, kilogram, second, ampere, kelvin, mole and candela.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Dims(pub [f64; 7]);

impl Dims {
    /// No dimension.
    pub const NONE: Dims = Dims([0.0; 7]);

    /// Multiplied.
    #[must_use]
    pub fn times(self, o: Dims) -> Dims {
        let mut d = self;
        for (a, b) in d.0.iter_mut().zip(o.0) {
            *a += b;
        }
        d
    }

    /// Raised.
    #[must_use]
    pub fn power(self, p: f64) -> Dims {
        Dims(self.0.map(|x| x * p))
    }

    /// The same, up to rounding.
    #[must_use]
    pub fn same(self, o: Dims) -> bool {
        self.0.iter().zip(o.0).all(|(a, b)| (a - b).abs() < 1e-6)
    }

    /// In base units: "kg·m⁻¹·s⁻²"; "1" without a dimension.
    #[must_use]
    pub fn shown(self) -> String {
        const NAMES: [&str; 7] = ["m", "kg", "s", "A", "K", "mol", "cd"];
        const ORDER: [usize; 7] = [1, 0, 2, 3, 4, 5, 6];
        let mut parts: Vec<String> = Vec::new();
        for i in ORDER {
            let e = self.0[i];
            if e.abs() < 1e-6 {
                continue;
            }
            parts.push(if (e - 1.0).abs() < 1e-6 {
                NAMES[i].into()
            } else {
                format!("{}{}", NAMES[i], raised(e))
            });
        }
        if parts.is_empty() {
            "1".into()
        } else {
            parts.join("·")
        }
    }
}

fn raised(e: f64) -> String {
    let s = if (e - e.round()).abs() < 1e-6 {
        format!("{e:.0}")
    } else {
        format!("{e:.2}")
    };
    s.chars()
        .map(|c| match c {
            '-' => '⁻',
            '.' => '·',
            '0' => '⁰',
            '1' => '¹',
            '2' => '²',
            '3' => '³',
            '4' => '⁴',
            '5' => '⁵',
            '6' => '⁶',
            '7' => '⁷',
            '8' => '⁸',
            _ => '⁹',
        })
        .collect()
}

/// Units with a name: exponents, and whether a prefix may stand before it.
const NAMED: [(&str, [i8; 7], bool); 52] = [
    ("m", [1, 0, 0, 0, 0, 0, 0], true),
    ("g", [0, 1, 0, 0, 0, 0, 0], true),
    ("t", [0, 1, 0, 0, 0, 0, 0], false),
    ("s", [0, 0, 1, 0, 0, 0, 0], true),
    ("sec", [0, 0, 1, 0, 0, 0, 0], false),
    ("min", [0, 0, 1, 0, 0, 0, 0], false),
    ("h", [0, 0, 1, 0, 0, 0, 0], false),
    ("hr", [0, 0, 1, 0, 0, 0, 0], false),
    ("d", [0, 0, 1, 0, 0, 0, 0], false),
    ("day", [0, 0, 1, 0, 0, 0, 0], false),
    ("days", [0, 0, 1, 0, 0, 0, 0], false),
    ("year", [0, 0, 1, 0, 0, 0, 0], false),
    ("years", [0, 0, 1, 0, 0, 0, 0], false),
    ("A", [0, 0, 0, 1, 0, 0, 0], true),
    ("K", [0, 0, 0, 0, 1, 0, 0], true),
    ("°C", [0, 0, 0, 0, 1, 0, 0], false),
    ("°F", [0, 0, 0, 0, 1, 0, 0], false),
    ("mol", [0, 0, 0, 0, 0, 1, 0], true),
    ("cd", [0, 0, 0, 0, 0, 0, 1], true),
    ("N", [1, 1, -2, 0, 0, 0, 0], true),
    ("Pa", [-1, 1, -2, 0, 0, 0, 0], true),
    ("bar", [-1, 1, -2, 0, 0, 0, 0], true),
    ("atm", [-1, 1, -2, 0, 0, 0, 0], false),
    ("psi", [-1, 1, -2, 0, 0, 0, 0], false),
    ("J", [2, 1, -2, 0, 0, 0, 0], true),
    ("cal", [2, 1, -2, 0, 0, 0, 0], true),
    ("eV", [2, 1, -2, 0, 0, 0, 0], true),
    ("Wh", [2, 1, -2, 0, 0, 0, 0], true),
    ("W", [2, 1, -3, 0, 0, 0, 0], true),
    ("V", [2, 1, -3, -1, 0, 0, 0], true),
    ("C", [0, 0, 1, 1, 0, 0, 0], true),
    ("F", [-2, -1, 4, 2, 0, 0, 0], true),
    ("Ω", [2, 1, -3, -2, 0, 0, 0], true),
    ("ohm", [2, 1, -3, -2, 0, 0, 0], false),
    ("S", [-2, -1, 3, 2, 0, 0, 0], true),
    ("H", [2, 1, -2, -2, 0, 0, 0], true),
    ("T", [0, 1, -2, -1, 0, 0, 0], true),
    ("Wb", [2, 1, -2, -1, 0, 0, 0], true),
    ("Hz", [0, 0, -1, 0, 0, 0, 0], true),
    ("rpm", [0, 0, -1, 0, 0, 0, 0], false),
    ("L", [3, 0, 0, 0, 0, 0, 0], true),
    ("l", [3, 0, 0, 0, 0, 0, 0], true),
    ("ha", [2, 0, 0, 0, 0, 0, 0], false),
    ("P", [-1, 1, -1, 0, 0, 0, 0], true),
    ("rad", [0, 0, 0, 0, 0, 0, 0], true),
    ("sr", [0, 0, 0, 0, 0, 0, 0], false),
    ("°", [0, 0, 0, 0, 0, 0, 0], false),
    ("deg", [0, 0, 0, 0, 0, 0, 0], false),
    ("%", [0, 0, 0, 0, 0, 0, 0], false),
    ("ppm", [0, 0, 0, 0, 0, 0, 0], false),
    ("dB", [0, 0, 0, 0, 0, 0, 0], false),
    ("M", [-3, 0, 0, 0, 0, 1, 0], true),
];
const PREFIXES: [&str; 17] = [
    "k", "M", "G", "T", "P", "h", "da", "d", "c", "m", "µ", "μ", "n", "p", "f", "a", "E",
];

fn named(name: &str) -> Option<Dims> {
    let find = |n: &str, prefixed: bool| {
        NAMED
            .iter()
            .find(|(u, _, p)| *u == n && (!prefixed || *p))
            .map(|(_, d, _)| Dims(d.map(f64::from)))
    };
    find(name, false).or_else(|| {
        PREFIXES.iter().find_map(|p| {
            name.strip_prefix(p)
                .filter(|r| !r.is_empty())
                .and_then(|r| find(r, true))
        })
    })
}

/// What the characters of a unit are in one spelling.
fn clean(text: &str) -> String {
    text.chars()
        .map(|c| match c {
            '\u{2212}' | '\u{2013}' | '⁻' => '-',
            '\u{25e6}' | 'º' | '˚' | '\u{2218}' => '°',
            '·' | '\u{22c5}' | '∙' | '*' | '.' | '\u{a0}' | '\u{2009}' | '×' => ' ',
            '²' => '2',
            '³' => '3',
            '¹' => '1',
            '⁴' => '4',
            '\u{3a9}' | '\u{2126}' => 'Ω',
            '℃' => 'ℂ',
            c => c,
        })
        .collect::<String>()
        .replace('ℂ', "°C")
        .replace("° C", "°C")
        .replace("° F", "°F")
}

/// The dimension of a unit, or nothing when this is not a unit. After the first "/" everything
/// divides ("W/m2K" is W/(m²·K)).
#[must_use]
pub fn parse(text: &str) -> Option<Dims> {
    static PART: LazyLock<Regex> = LazyLock::new(|| {
        crate::re(r"^(%|°[CF]?|[A-Za-zµμΩ]+)\s?\^?\s?\{?(-?\s?\d+(?:/\d+|\.\d+)?)?\}?")
    });
    let t = clean(text.trim());
    let t = t.trim();
    if ["-", "–", "1", "dimensionless", "unitless", "none"].contains(&t) {
        return Some(Dims::NONE);
    }
    if t.is_empty() || t.chars().count() > 24 {
        return None;
    }
    let mut out = Dims::NONE;
    for (i, side) in t.split('/').enumerate() {
        let mut rest = side
            .trim()
            .trim_start_matches('(')
            .trim_end_matches(')')
            .trim();
        if rest == "1" && i == 0 {
            continue;
        }
        if rest.is_empty() {
            return None;
        }
        while !rest.is_empty() {
            let c = PART.captures(rest)?;
            let whole = c.get(0)?.end();
            let e: f64 = match c.get(2) {
                Some(m) => {
                    let s = m.as_str().replace(' ', "");
                    match s.split_once('/') {
                        Some((a, b)) => a.parse::<f64>().ok()? / b.parse::<f64>().ok()?,
                        None => s.parse().ok()?,
                    }
                }
                None => 1.0,
            };
            let d = several(&c[1])?.power(e);
            out = out.times(if i == 0 { d } else { d.power(-1.0) });
            rest = rest[whole..].trim_start();
        }
    }
    Some(out)
}

/// One unit, or two written together ("mK" is millikelvin, "kgK" kg·K, "Nm" N·m).
fn several(name: &str) -> Option<Dims> {
    named(name).or_else(|| {
        let cut: Vec<usize> = name.char_indices().map(|(i, _)| i).skip(1).collect();
        cut.into_iter()
            .rev()
            .find_map(|i| Some(named(&name[..i])?.times(named(&name[i..])?)))
    })
}

/// The unit at the end of a heading: "Error (K)", "Time [s]", "ν, mm2 /s".
#[must_use]
pub fn in_heading(heading: &str) -> Option<String> {
    static BRACKET: LazyLock<Regex> =
        LazyLock::new(|| crate::re(r"[(\[]\s*([^()\[\]]{1,14}(?:\([^()]{1,8}\))?)\s*[)\]]\s*$"));
    static COMMA: LazyLock<Regex> = LazyLock::new(|| crate::re(r",\s*(?:in )?([^,]{1,12})$"));
    let h = heading.trim();
    BRACKET
        .captures(h)
        .or_else(|| COMMA.captures(h))
        .map(|c| c[1].trim().to_string())
        .filter(|u| {
            u.chars()
                .any(|c| c.is_alphabetic() || c == '%' || c == '°' || c == '\u{25e6}')
                && parse(u).is_some()
        })
}

/// The unit in a meaning: "thermal conductivity (W/mK)", "the velocity in m/s", "[kg m−3]".
#[must_use]
pub fn in_meaning(meaning: &str) -> Option<String> {
    static BRACKET: LazyLock<Regex> = LazyLock::new(|| {
        crate::re(r"[(\[]\s*(?:in )?([^()\[\]]{1,18}(?:\([^()]{1,10}\))?)\s*[)\]]")
    });
    static IN: LazyLock<Regex> = LazyLock::new(|| {
        crate::re(r"\bin ((?:[°◦]\s?C|[A-Za-zµΩ%°/·²³0-9−\-]+(?: [A-Za-zµ/·²³0-9−\-]+){0,3}))")
    });
    let word = |u: &str| {
        u.chars()
            .any(|c| c.is_alphabetic() || "%°\u{25e6}".contains(c))
            && parse(u).is_some()
    };
    if let Some(u) = BRACKET
        .captures_iter(meaning)
        .map(|c| c[1].trim().to_string())
        .find(|u| word(u))
    {
        return Some(u);
    }
    IN.captures_iter(meaning).find_map(|c| {
        // "in m/s at the inlet": as many words as make a unit; a single letter word ("a") is none.
        let words: Vec<&str> = c[1].split(' ').collect();
        (1..=words.len())
            .rev()
            .map(|n| words[..n].join(" "))
            .find(|u| word(u) && u != "a" && u != "A")
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn units_as_papers_print_them() {
        let same = |a: &str, b: &str| assert!(parse(a).unwrap().same(parse(b).unwrap()), "{a} {b}");
        same("W/m2K", "W m−2 K−1");
        same("W/(m²·K)", "kg s-3 K-1");
        same("J kg−1 K−1", "kJ/kgK");
        same("N", "kg m/s2");
        same("Pa", "N/m^2");
        same("mm2 /s", "m^2 s^{-1}");
        same("kWh", "J");
        same("◦C", "K");
        same("%", "-");
        same("Nm", "J");
        same("1/s", "Hz");
        assert_eq!(parse("W/m2").unwrap().shown(), "kg·s⁻³");
        assert_eq!(parse("m/s").unwrap().shown(), "m·s⁻¹");
    }

    #[test]
    fn words_are_no_units() {
        for t in [
            "layers",
            "Ours",
            "a word",
            "17",
            "et al.",
            "∂t",
            "",
            "x",
            "seconds of time and more words",
        ] {
            assert!(parse(t).is_none(), "{t}");
        }
    }

    #[test]
    fn units_are_found_in_headings_and_meanings() {
        assert_eq!(in_heading("Frequency [Hz]").as_deref(), Some("Hz"));
        assert_eq!(in_heading("ν, mm2 /s").as_deref(), Some("mm2 /s"));
        assert_eq!(in_heading("p rel. error, %").as_deref(), Some("%"));
        assert_eq!(in_heading("Experiment [17]"), None);
        assert_eq!(in_heading("LSTM, v2"), None);
        assert_eq!(
            in_meaning("the thermal conductivity (W/(m K))").as_deref(),
            Some("W/(m K)")
        );
        assert_eq!(
            in_meaning("the velocity in m/s at the inlet").as_deref(),
            Some("m/s")
        );
        assert_eq!(
            in_meaning("the density [kg m−3]").as_deref(),
            Some("kg m−3")
        );
        assert_eq!(in_meaning("a point in a room (see below)"), None);
    }
}
