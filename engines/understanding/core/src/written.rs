//! Answers as plain text: a model writes some formulas in LaTeX, and the reader sees letters.

use crate::latex::{FONTS, accent, greek};

/// The longest formula inside a sentence.
const LONGEST: usize = 200;

/// The text with its LaTeX (`$…$`, `\(…\)`, `\[…\]`) written as plain text; everything else as it is.
#[must_use]
pub fn written(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::new();
    let mut i = 0;
    while i < chars.len() {
        if let Some((from, to, next)) = formula(&chars, i) {
            out.push_str(plain(&chars[from..to]).trim());
            i = next;
        } else if let Some((c, next)) = letter(&chars, i) {
            out.push(c);
            i = next;
        } else {
            out.push(chars[i]);
            i += 1;
        }
    }
    out
}

/// As [`written`], of a text that still grows: a formula that has begun is not shown before its end.
#[must_use]
pub fn written_so_far(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if let Some((_, _, next)) = formula(&chars, i) {
            i = next;
        } else if opens(&chars, i).is_some() && chars.len() - i <= LONGEST || chars[i..] == ['\\'] {
            return written(&chars[..i].iter().collect::<String>())
                .trim_end()
                .to_string();
        } else {
            i += 1;
        }
    }
    written(text)
}

/// Whether a formula opens at `i`: where its inside begins and what closes it.
fn opens(c: &[char], i: usize) -> Option<(usize, &'static [char])> {
    match (c.get(i)?, c.get(i + 1)) {
        ('\\', Some('(')) => Some((i + 2, &['\\', ')'])),
        ('\\', Some('[')) => Some((i + 2, &['\\', ']'])),
        ('$', Some('$')) => Some((i + 2, &['$', '$'])),
        // "$5" is an amount.
        ('$', Some(d)) if d.is_ascii_digit() => None,
        ('$', _) if i == 0 || c[i - 1] != '\\' => Some((i + 1, &['$'])),
        _ => None,
    }
}

/// The formula that opens at `i`: its inside, and where the text goes on.
fn formula(c: &[char], i: usize) -> Option<(usize, usize, usize)> {
    let (from, close) = opens(c, i)?;
    let last = c.len().min(from + LONGEST);
    let to = (from..last).find(|k| c[*k..].starts_with(close))?;
    (to > from).then_some((from, to, to + close.len()))
}

/// A Greek letter written as a command outside a formula ("the \lambda of").
fn letter(c: &[char], i: usize) -> Option<(char, usize)> {
    if c[i] != '\\' {
        return None;
    }
    let (name, next) = command(c, i);
    Some((greek(&name)?, next))
}

/// The name of the command at `i`, and where it ends.
fn command(c: &[char], i: usize) -> (String, usize) {
    let name: String = c[i + 1..]
        .iter()
        .take_while(|c| c.is_ascii_alphabetic())
        .collect();
    let end = i + 1 + name.len();
    if name.is_empty() {
        let one = c.get(i + 1).map(ToString::to_string).unwrap_or_default();
        return (one, (i + 2).min(c.len()));
    }
    (name, end)
}

/// The one character of a command.
fn sign(name: &str) -> Option<char> {
    if let Some(letter) = greek(name) {
        return Some(letter);
    }
    Some(match name {
        "pi" | "varpi" => 'π',
        "cdot" => '·',
        "times" => '×',
        "div" => '÷',
        "leq" | "le" => '≤',
        "geq" | "ge" => '≥',
        "neq" | "ne" => '≠',
        "approx" | "simeq" => '≈',
        "sim" => '∼',
        "propto" => '∝',
        "equiv" => '≡',
        "pm" => '±',
        "ll" => '≪',
        "gg" => '≫',
        "infty" => '∞',
        "partial" => '∂',
        "nabla" => '∇',
        "sum" => '∑',
        "prod" => '∏',
        "int" => '∫',
        "in" => '∈',
        "to" | "rightarrow" => '→',
        "ldots" | "cdots" | "dots" => '…',
        "circ" => '°',
        "prime" => '′',
        "ast" => '*',
        "langle" => '⟨',
        "rangle" => '⟩',
        "quad" | "qquad" => ' ',
        _ => return None,
    })
}

/// What stands at `i`: a group, a command with its group, or one character; where it goes on.
fn piece(c: &[char], i: usize) -> (String, usize) {
    match c.get(i) {
        Some('{') => {
            let end = closing(c, i);
            (plain(&c[i + 1..end]), (end + 1).min(c.len()))
        }
        Some('\\') => {
            let (name, next) = command(c, i);
            told(c, &name, next)
        }
        Some(ch) => (ch.to_string(), i + 1),
        None => (String::new(), i),
    }
}

/// Where the group that opens at `i` closes (the end, when it does not).
fn closing(c: &[char], i: usize) -> usize {
    let mut depth = 0;
    for (k, ch) in c.iter().enumerate().skip(i) {
        match ch {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return k;
                }
            }
            _ => {}
        }
    }
    c.len()
}

/// A command and what belongs to it.
fn told(c: &[char], name: &str, at: usize) -> (String, usize) {
    let at_group = |mut i: usize| {
        while c.get(i).is_some_and(|c| c.is_whitespace()) {
            i += 1;
        }
        i
    };
    if let Some(s) = sign(name) {
        return (s.to_string(), at);
    }
    if let Some(mark) = accent(name) {
        let (inside, next) = piece(c, at_group(at));
        let mut it = inside.chars();
        let mut out: String = it.next().into_iter().collect();
        out.push(mark);
        out.extend(it);
        return (out, next);
    }
    if FONTS.contains(&name) {
        let i = at_group(at);
        return match c.get(i) {
            // "{\rm d}" has no group of its own.
            Some('{') => piece(c, i),
            _ => (String::new(), i),
        };
    }
    match name {
        "frac" | "dfrac" | "tfrac" => {
            let (above, next) = piece(c, at_group(at));
            let (below, next) = piece(c, at_group(next));
            (format!("{}/{}", one(&above), one(&below)), next)
        }
        "sqrt" => {
            let (inside, next) = piece(c, at_group(at));
            (format!("√{}", one(&inside)), next)
        }
        "left" | "right" => {
            // "\left." opens nothing that is seen.
            let next = if c.get(at) == Some(&'.') { at + 1 } else { at };
            (String::new(), next)
        }
        "big" | "Big" | "bigg" | "Bigg" | "bigl" | "bigr" | "displaystyle" | "textstyle"
        | "limits" | "nolimits" | "!" => (String::new(), at),
        "," | ";" | ":" | " " | ">" | "thinspace" | "\\" => (" ".into(), at),
        "begin" | "end" => (String::new(), piece(c, at_group(at)).1),
        other => (other.to_string(), at),
    }
}

/// In brackets when it is more than a name or a number.
fn one(s: &str) -> String {
    let s = s.trim();
    let simple = s
        .chars()
        .all(|c| c.is_alphanumeric() || ".∞*′°".contains(c) || accented(c));
    if simple {
        s.to_string()
    } else {
        format!("({s})")
    }
}

fn accented(c: char) -> bool {
    ('\u{300}'..='\u{36f}').contains(&c) || c == '\u{20d7}'
}

/// The inside of a formula as plain text.
fn plain(c: &[char]) -> String {
    let mut out = String::new();
    let mut i = 0;
    while i < c.len() {
        match c[i] {
            '_' | '^' => {
                let mut k = i + 1;
                while c.get(k).is_some_and(|c| c.is_whitespace()) {
                    k += 1;
                }
                let (p, next) = piece(c, k);
                let p = one(&p);
                // A degree and a prime stand high as they are.
                if c[i] == '_' || !matches!(p.as_str(), "°" | "′" | "") {
                    out.push(c[i]);
                }
                out.push_str(&p);
                i = next.max(i + 1);
            }
            '}' | '&' => i += 1,
            w if w.is_whitespace() || w == '~' => {
                if !out.ends_with(' ') {
                    out.push(' ');
                }
                i += 1;
            }
            _ => {
                let (p, next) = piece(c, i);
                out.push_str(&p);
                i = next.max(i + 1);
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formulas_are_written_as_letters() {
        for (latex, plain) in [
            (
                r"the field $ Q(z, t) $ at \( \lambda_i \).",
                "the field Q(z, t) at λ_i.",
            ),
            (r"\( x_i^{\text{obs}} \) is seen", "x_i^obs is seen"),
            (
                r"$\dot{m} c_{p} (T_{\mathrm{out}} - T_{in})$",
                "m\u{307} c_p (T_out - T_in)",
            ),
            (r"\[ \frac{\partial u}{\partial t} \]", "(∂ u)/(∂ t)"),
            (r"$x_{i,j} = 10^{-3}$", "x_(i,j) = 10^(-3)"),
            (r"$\sqrt{2 k} \leq \sin x$", "√(2 k) ≤ sin x"),
            (r"0.2 $^\circ$C, $\text{for all } x$", "0.2 °C, for all x"),
            (r"the \lambda of it", "the λ of it"),
            (r"$\left( a \right)^{2}$", "( a )^2"),
            (r"$$E = mc^2$$", "E = mc^2"),
        ] {
            assert_eq!(written(latex), plain, "{latex}");
        }
    }

    #[test]
    fn other_text_stays_as_it_is() {
        for t in [
            "It costs $5 and $7.",
            "C:\\new folder",
            "a_b^c {x}",
            "a $ alone",
            "",
            "\\",
        ] {
            assert_eq!(written(t), t);
        }
    }

    #[test]
    fn broken_formulas_lose_nothing_and_end() {
        for t in [
            "$\\frac{a$",
            "$_$",
            "$x^$",
            "\\(\\\\)",
            "$\\hat$ x",
            "$}{$",
            "$\\sqrt$",
            "$ $",
        ] {
            let _ = written(t);
            let _ = written_so_far(t);
        }
        assert_eq!(written(r"$\frac{a$"), "a/");
    }

    #[test]
    fn a_formula_that_has_begun_waits_for_its_end() {
        assert_eq!(written_so_far(r"the field $ Q(z"), "the field");
        assert_eq!(
            written_so_far(r"the field $ Q(z, t) $ is"),
            "the field Q(z, t) is"
        );
        assert_eq!(written_so_far(r"the field \"), "the field");
        assert_eq!(written_so_far("It costs $5"), "It costs $5");
        assert_eq!(written_so_far("as it is "), "as it is ");
    }
}
