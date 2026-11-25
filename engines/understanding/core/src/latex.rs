//! LaTeX of one formula as tokens, and the symbols in it.

/// A piece of LaTeX. White space is left out: formula models print it everywhere.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Tok {
    /// `\name` (without the backslash).
    Cmd(String),
    /// A character.
    Ch(char),
    /// `{`.
    Open,
    /// `}`.
    Close,
    /// `^`.
    Sup,
    /// `_`.
    Sub,
}

pub(crate) fn tokens(latex: &str) -> Vec<Tok> {
    let mut out = Vec::new();
    let mut it = latex.chars().peekable();
    while let Some(c) = it.next() {
        out.push(match c {
            '\\' => {
                let mut name = String::new();
                while let Some(n) = it.peek().copied().filter(char::is_ascii_alphabetic) {
                    name.push(n);
                    it.next();
                }
                if name.is_empty() {
                    match it.next() {
                        // Spaces of a formula.
                        Some(',' | ';' | '!' | ' ' | ':' | '>') | None => continue,
                        Some(n) => name.push(n),
                    }
                }
                match name.as_str() {
                    "left" | "right" | "big" | "Big" | "bigg" | "Bigg" | "bigl" | "bigr"
                    | "displaystyle" | "textstyle" | "limits" | "nolimits" | "thinspace" => {
                        // "\left." opens nothing that is seen.
                        if (name == "left" || name == "right") && it.peek() == Some(&'.') {
                            it.next();
                        }
                        continue;
                    }
                    _ => Tok::Cmd(name),
                }
            }
            '{' => Tok::Open,
            '}' => Tok::Close,
            '^' => Tok::Sup,
            '_' => Tok::Sub,
            c if c.is_whitespace() || c == '~' => continue,
            c => Tok::Ch(c),
        });
    }
    out
}

/// As LaTeX again.
pub(crate) fn tex(t: &[Tok]) -> String {
    let mut s = String::new();
    for (i, k) in t.iter().enumerate() {
        match k {
            Tok::Cmd(n) => {
                s.push('\\');
                s.push_str(n);
                if matches!(t.get(i + 1), Some(Tok::Ch(c)) if c.is_alphanumeric()) {
                    s.push(' ');
                }
            }
            Tok::Ch(c) => s.push(*c),
            Tok::Open => s.push('{'),
            Tok::Close => s.push('}'),
            Tok::Sup => s.push('^'),
            Tok::Sub => s.push('_'),
        }
    }
    s
}

/// What stands at `i`: the inside of a group or one token, and where it goes on.
pub(crate) fn group(t: &[Tok], i: usize) -> Option<(&[Tok], usize)> {
    if t.get(i)? != &Tok::Open {
        return Some((&t[i..=i], i + 1));
    }
    let mut depth = 0;
    for (k, tok) in t.iter().enumerate().skip(i) {
        match tok {
            Tok::Open => depth += 1,
            Tok::Close => {
                depth -= 1;
                if depth == 0 {
                    return Some((&t[i + 1..k], k + 1));
                }
            }
            _ => {}
        }
    }
    None
}

pub(crate) fn greek(name: &str) -> Option<char> {
    Some(match name {
        "alpha" => 'α',
        "beta" => 'β',
        "gamma" => 'γ',
        "delta" => 'δ',
        "epsilon" | "varepsilon" => 'ε',
        "zeta" => 'ζ',
        "eta" => 'η',
        "theta" | "vartheta" => 'θ',
        "iota" => 'ι',
        "kappa" | "varkappa" => 'κ',
        "lambda" => 'λ',
        "mu" => 'μ',
        "nu" => 'ν',
        "xi" => 'ξ',
        "rho" | "varrho" => 'ρ',
        "sigma" | "varsigma" => 'σ',
        "tau" => 'τ',
        "upsilon" => 'υ',
        "phi" | "varphi" => 'φ',
        "chi" => 'χ',
        "psi" => 'ψ',
        "omega" => 'ω',
        "Gamma" => 'Γ',
        "Delta" => 'Δ',
        "Theta" => 'Θ',
        "Lambda" => 'Λ',
        "Xi" => 'Ξ',
        "Pi" => 'Π',
        "Sigma" => 'Σ',
        "Upsilon" => 'Υ',
        "Phi" => 'Φ',
        "Psi" => 'Ψ',
        "Omega" => 'Ω',
        "ell" => 'ℓ',
        "hbar" => 'ħ',
        _ => return None,
    })
}

pub(crate) const FONTS: [&str; 18] = [
    "mathrm",
    "mathbf",
    "boldsymbol",
    "bm",
    "mathcal",
    "mathit",
    "mathsf",
    "mathbb",
    "mathfrak",
    "mathscr",
    "text",
    "textrm",
    "textit",
    "textbf",
    "operatorname",
    "rm",
    "bf",
    "pmb",
];

/// The mark an accent puts over its symbol.
pub(crate) fn accent(name: &str) -> Option<char> {
    Some(match name {
        "dot" => '\u{307}',
        "ddot" => '\u{308}',
        "hat" | "widehat" => '\u{302}',
        "bar" | "overline" => '\u{304}',
        "tilde" | "widetilde" => '\u{303}',
        "vec" | "overrightarrow" => '\u{20d7}',
        _ => return None,
    })
}

/// Names of functions and operators, which are no symbols.
pub(crate) const FUNCTIONS: [&str; 34] = [
    "sin", "cos", "tan", "cot", "sec", "csc", "sinh", "cosh", "tanh", "arcsin", "arccos", "arctan",
    "exp", "log", "ln", "lg", "max", "min", "sup", "inf", "arg", "argmin", "argmax", "lim", "det",
    "dim", "tr", "diag", "sgn", "sign", "relu", "softmax", "erf", "mod",
];

/// A symbol of a formula.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Symbol {
    /// As LaTeX.
    pub latex: String,
    /// As plain text ("`T_in`", "ṁ").
    pub shown: String,
    /// Letters and digits alone, for comparing with printed text.
    pub key: String,
    /// Whether it has a mark over it.
    pub accent: bool,
    /// Its mark and base without subscript, when it has a mark.
    pub(crate) mark: Option<char>,
}

fn letters(t: &[Tok]) -> Option<String> {
    let mut s = String::new();
    let mut i = 0;
    while i < t.len() {
        match &t[i] {
            Tok::Ch(c) if c.is_alphanumeric() || "-,.*+′'()/".contains(*c) => s.push(*c),
            Tok::Cmd(n) if greek(n).is_some() => s.extend(greek(n)),
            Tok::Cmd(n) if n == "prime" => s.push('′'),
            Tok::Cmd(n) if n == "infty" => s.push('∞'),
            Tok::Cmd(n) if n == "ast" => s.push('*'),
            Tok::Cmd(n) if FONTS.contains(&n.as_str()) || accent(n).is_some() => {}
            Tok::Open | Tok::Close => {}
            _ => return None,
        }
        i += 1;
    }
    (!s.is_empty()).then_some(s)
}

/// Letters and digits of a printed symbol.
pub(crate) fn key(s: &str) -> String {
    use unicode_normalization::UnicodeNormalization;
    s.nfkd()
        .filter(|c| c.is_alphanumeric())
        .map(|c| match c {
            '\u{2206}' => 'Δ',
            c => c,
        })
        .collect()
}

/// The symbol at `i`, with its subscript and the marks that belong to its name, and where the
/// formula goes on.
pub(crate) fn symbol(t: &[Tok], i: usize) -> Option<(Symbol, usize)> {
    let (base, mark, mut at) = base(t, i)?;
    let mut sub = String::new();
    let mut label = String::new();
    loop {
        match t.get(at) {
            Some(Tok::Sub) if sub.is_empty() => {
                let (g, next) = group(t, at + 1)?;
                sub = letters(g)?;
                at = next;
            }
            Some(Tok::Ch(c)) if "'′".contains(*c) => {
                label.push('′');
                at += 1;
            }
            Some(Tok::Sup) => {
                let (g, next) = group(t, at + 1)?;
                // A name above ("obs", "*", "(n)", "′") belongs to the symbol; a power does not.
                let named = match g {
                    [Tok::Cmd(n)] => matches!(n.as_str(), "prime" | "ast" | "dagger" | "star"),
                    [Tok::Ch('*' | '′' | '\'' | '+')] | [Tok::Ch('('), .., Tok::Ch(')')] => true,
                    [Tok::Cmd(n), ..] if matches!(n.as_str(), "mathrm" | "text" | "rm") => {
                        // T in roman letters is the transpose.
                        letters(g).is_some_and(|s| s.chars().count() > 1)
                    }
                    g => {
                        g.len() > 2
                            && g.iter()
                                .all(|k| matches!(k, Tok::Ch(c) if c.is_alphabetic()))
                    }
                };
                if !named || !label.is_empty() {
                    break;
                }
                label = letters(g).unwrap_or_else(|| "*".into());
                at = next;
            }
            _ => break,
        }
    }
    let mut shown = base.clone();
    shown.extend(mark);
    if !sub.is_empty() {
        shown.push('_');
        shown.push_str(&sub);
    }
    if !label.is_empty() && label != "′" {
        shown.push('^');
    }
    shown.push_str(&label);
    Some((
        Symbol {
            latex: tex(&t[i..at]),
            key: key(&format!("{base}{sub}{label}")),
            shown,
            accent: mark.is_some(),
            mark,
        },
        at,
    ))
}

fn base(t: &[Tok], i: usize) -> Option<(String, Option<char>, usize)> {
    match t.get(i)? {
        Tok::Ch(c) if c.is_alphabetic() => Some((c.to_string(), None, i + 1)),
        Tok::Cmd(n) => {
            if let Some(g) = greek(n) {
                return Some((g.to_string(), None, i + 1));
            }
            if let Some(m) = accent(n) {
                let (g, next) = group(t, i + 1)?;
                let (b, _, end) = base(g, 0)?;
                return (end == g.len()).then_some((b, Some(m), next));
            }
            if FONTS.contains(&n.as_str()) {
                let (g, next) = group(t, i + 1)?;
                let word: String = g
                    .iter()
                    .map(|k| match k {
                        Tok::Ch(c) if c.is_alphabetic() => Some(*c),
                        _ => None,
                    })
                    .collect::<Option<String>>()
                    .unwrap_or_default();
                if word.chars().count() > 1 {
                    let function = FUNCTIONS.contains(&word.to_lowercase().as_str());
                    return (!function && word.chars().count() <= 8).then_some((word, None, next));
                }
                let (b, m, end) = base(g, 0)?;
                return (end == g.len()).then_some((b, m, next));
            }
            None
        }
        Tok::Open => {
            let (g, next) = group(t, i)?;
            let (b, m, end) = base(g, 0)?;
            (end == g.len()).then_some((b, m, next))
        }
        _ => None,
    }
}

/// Whether a letter d (or ∂) at `i` is the one of a derivative or of an integral: what it
/// differentiates comes right after it.
pub(crate) fn differential(t: &[Tok], i: usize) -> Option<usize> {
    match t.get(i)? {
        Tok::Cmd(n) if n == "partial" => Some(i + 1),
        Tok::Ch('∂') => Some(i + 1),
        Tok::Ch('d') => {
            let next = t.get(i + 1)?;
            let symbol = matches!(next, Tok::Ch(c) if c.is_alphabetic())
                || matches!(next, Tok::Cmd(n) if greek(n).is_some() || accent(n).is_some() || FONTS.contains(&n.as_str()))
                || matches!(next, Tok::Sup);
            symbol.then_some(i + 1)
        }
        Tok::Cmd(n) if n == "mathrm" || n == "rm" || n == "text" => {
            let (g, next) = group(t, i + 1)?;
            (g == [Tok::Ch('d')]).then_some(next)
        }
        _ => None,
    }
}

/// The symbols of a formula, each once, in the order they stand.
#[must_use]
pub fn symbols(latex: &str) -> Vec<Symbol> {
    let t = tokens(latex);
    // A plain d is a differential only in a formula that has a derivative or an integral.
    let derives = t.iter().enumerate().any(|(i, k)| {
        matches!(k, Tok::Cmd(n) if n == "int" || n == "oint" || n == "iint" || n == "iiint")
            || (matches!(k, Tok::Cmd(n) if n == "frac" || n == "dfrac")
                && group(&t, i + 1).is_some_and(|(g, _)| matches!(g.first(), Some(Tok::Ch('d')))))
    });
    let mut out: Vec<Symbol> = Vec::new();
    let mut i = 0;
    while i < t.len() {
        // Names of environments and colours are no symbols.
        if matches!(&t[i], Tok::Cmd(n) if n == "begin" || n == "end" || n == "label" || n == "tag")
        {
            i = group(&t, i + 1).map_or(i + 1, |(_, next)| next);
            continue;
        }
        if let Some(next) = differential(&t, i)
            && (derives || t[i] != Tok::Ch('d'))
        {
            i = next;
            continue;
        }
        // e^{…} is the exponential.
        if t[i] == Tok::Ch('e') && t.get(i + 1) == Some(&Tok::Sup) {
            i += 2;
            continue;
        }
        match symbol(&t, i) {
            Some((s, next)) => {
                // A group that is only a letter is seen when it is entered, too.
                if !out.iter().any(|o| o.shown == s.shown) && out.len() < 32 {
                    out.push(s);
                }
                i = next;
            }
            None => i += 1,
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shown(latex: &str) -> Vec<String> {
        symbols(latex).into_iter().map(|s| s.shown).collect()
    }

    #[test]
    fn symbols_are_found_with_their_subscripts_and_marks() {
        assert_eq!(
            shown(r"Q = \dot{m} c_{p} \left( T_{in} - T_{\mathrm{out}} \right)"),
            ["Q", "m\u{307}", "c_p", "T_in", "T_out"]
        );
        assert_eq!(
            shown(r"\rho \frac { \partial u } { \partial t } = \mu \nabla ^ { 2 } u"),
            ["ρ", "u", "t", "μ"]
        );
        assert_eq!(
            shown(r"h = \mathrm { N u } \, k / D_{h}"),
            ["h", "Nu", "k", "D_h"]
        );
    }

    #[test]
    fn operators_functions_and_numbers_are_no_symbols() {
        assert_eq!(
            shown(r"y = e^{-2x} + \sin(\omega t) + \exp(z)"),
            ["y", "x", "ω", "t", "z"]
        );
        assert_eq!(
            shown(r"\frac{dT}{dt} = - k T"),
            ["T", "t", "k"],
            "the d of a derivative"
        );
        assert_eq!(
            shown(r"V = \pi d^2"),
            ["V", "d"],
            "a d alone is a symbol, π is none"
        );
        assert_eq!(shown(r"\begin{aligned} a &= b \end{aligned}"), ["a", "b"]);
    }

    #[test]
    fn names_above_belong_to_the_symbol_and_powers_do_not() {
        assert_eq!(
            shown(r"x^{*} + y^2 + z^{obs} + w'"),
            ["x^*", "y", "z^obs", "w′"]
        );
        assert_eq!(symbols(r"T_{in}^{2}")[0].key, "Tin");
        assert_eq!(symbols(r"T_{in}^{2}")[0].latex, "T_{in}");
    }
}
