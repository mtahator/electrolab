//! Whether the units of a formula agree (P1.M5.UND.4). The LaTeX is read as a formula and the units
//! of its symbols are carried through it; what is not known stays unknown, and "they do not agree"
//! is only said about parts that are known completely.

use crate::latex::{FUNCTIONS, Symbol, Tok, accent, differential, group, symbol, tokens};
use crate::units::Dims;

/// The result.
#[derive(Debug, Clone, PartialEq)]
pub struct Check {
    /// `Some(true)`: every unit is known and they agree; `Some(false)`: known parts do not agree.
    pub agree: Option<bool>,
    /// Units of the sides, in base units (unknown ones left out).
    pub sides: Vec<String>,
    /// Symbols without a unit.
    pub missing: Vec<String>,
    /// The formula could not be read.
    pub unread: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Value {
    Known(Dims),
    /// Nothing, infinity: fits everywhere.
    Any,
    Unknown,
}

impl Value {
    fn map(self, f: impl FnOnce(Dims) -> Dims) -> Value {
        match self {
            Value::Known(d) => Value::Known(f(d)),
            v => v,
        }
    }

    fn times(self, o: Value) -> Value {
        match (self, o) {
            (Value::Known(a), Value::Known(b)) => Value::Known(a.times(b)),
            (Value::Any, _) | (_, Value::Any) => Value::Any,
            _ => Value::Unknown,
        }
    }
}

const ONE: Value = Value::Known(Dims::NONE);

/// Not a formula that can be read.
struct Unread;

struct Reader<'a> {
    unit: &'a dyn Fn(&Symbol) -> Option<Dims>,
    missing: Vec<String>,
    /// Values that must agree.
    compared: Vec<Vec<Dims>>,
    unknown: bool,
    /// Something was read one of several ways.
    guessed: bool,
    depth: u32,
}

/// Check the units of a formula; `unit` gives the dimension of a symbol.
#[must_use]
pub fn check(latex: &str, unit: &dyn Fn(&Symbol) -> Option<Dims>) -> Check {
    let t = tokens(latex);
    let mut r = Reader {
        unit,
        missing: vec![],
        compared: vec![],
        unknown: false,
        guessed: false,
        depth: 0,
    };
    let mut sides: Vec<String> = vec![];
    let mut related = false;
    for part in parts(&t) {
        match r.relation(part) {
            Ok(values) => {
                related |= values.len() > 1;
                if values.len() > 1 && sides.is_empty() {
                    sides = values
                        .iter()
                        .filter_map(|v| match v {
                            Value::Known(d) => Some(d.shown()),
                            _ => None,
                        })
                        .collect();
                }
                r.compare(&values);
            }
            Err(Unread) => {
                return Check {
                    agree: None,
                    sides: vec![],
                    missing: vec![],
                    unread: true,
                };
            }
        }
    }
    let differs = r.compared.iter().any(|c| c.iter().any(|d| !d.same(c[0])));
    let agree = if differs {
        // What was guessed is not held against the formula.
        (!r.guessed).then_some(false)
    } else {
        (related && !r.unknown).then_some(true)
    };
    Check {
        agree,
        sides,
        missing: r.missing,
        unread: false,
    }
}

/// The formulas of one line: cut at commas outside brackets and at wide spaces; a number in
/// brackets at the end is left out.
fn parts(t: &[Tok]) -> Vec<&[Tok]> {
    let mut out = vec![];
    let (mut depth, mut start) = (0i32, 0);
    for (i, k) in t.iter().enumerate() {
        match k {
            Tok::Open | Tok::Ch('(' | '[') => depth += 1,
            Tok::Close | Tok::Ch(')' | ']') => depth -= 1,
            Tok::Cmd(n) if n == "{" || n == "lbrace" || n == "langle" => depth += 1,
            Tok::Cmd(n) if n == "}" || n == "rbrace" || n == "rangle" => depth -= 1,
            Tok::Ch(',' | ';') if depth == 0 => {
                out.push(&t[start..i]);
                start = i + 1;
            }
            Tok::Cmd(n) if depth == 0 && (n == "quad" || n == "qquad") => {
                out.push(&t[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }
    let mut last = &t[start..];
    while let Some((Tok::Ch('.'), rest)) = last.split_last() {
        last = rest;
    }
    out.push(last);
    out.retain(|p| !p.is_empty());
    out
}

fn relation(t: &Tok) -> bool {
    match t {
        Tok::Ch('=' | '<' | '>' | '≈' | '≤' | '≥' | '≡') => true,
        Tok::Cmd(n) => matches!(
            n.as_str(),
            "approx"
                | "simeq"
                | "equiv"
                | "leq"
                | "le"
                | "geq"
                | "ge"
                | "ll"
                | "gg"
                | "triangleq"
                | "cong"
                | "neq"
                | "leqslant"
                | "geqslant"
                | "lt"
                | "gt"
        ),
        _ => false,
    }
}

/// How a bracket is closed.
fn closing(t: &Tok) -> Option<fn(&Tok) -> bool> {
    match t {
        Tok::Ch('(' | '[') => Some(|k| matches!(k, Tok::Ch(')' | ']'))),
        Tok::Ch('|') => Some(|k| matches!(k, Tok::Ch('|'))),
        Tok::Cmd(n) => match n.as_str() {
            "{" | "lbrace" => Some(|k| matches!(k, Tok::Cmd(n) if n == "}" || n == "rbrace")),
            "|" | "Vert" => Some(|k| matches!(k, Tok::Cmd(n) if n == "|" || n == "Vert")),
            "vert" => Some(|k| matches!(k, Tok::Cmd(n) if n == "vert")),
            "langle" => Some(|k| matches!(k, Tok::Cmd(n) if n == "rangle")),
            _ => None,
        },
        _ => None,
    }
}

/// The inside of the bracket at `i`, and where the formula goes on.
fn bracket(t: &[Tok], i: usize) -> Option<(&[Tok], usize)> {
    let closes = closing(t.get(i)?)?;
    let same = closes(&t[i]);
    let mut depth = 0;
    let mut k = i + 1;
    while k < t.len() {
        if closes(&t[k]) && depth == 0 {
            return Some((&t[i + 1..k], k + 1));
        }
        if closes(&t[k]) {
            depth -= 1;
        } else if !same && closing(&t[k]).is_some() && !matches!(t[k], Tok::Ch('|')) {
            depth += 1;
        }
        k += 1;
    }
    None
}

/// A plain number: "2", "-1", "0.8", "1/2", "\frac{1}{2}".
fn plain(t: &[Tok]) -> Option<f64> {
    if let [Tok::Cmd(n), ..] = t
        && matches!(n.as_str(), "frac" | "dfrac" | "tfrac")
    {
        let (a, i) = group(t, 1)?;
        let (b, end) = group(t, i)?;
        return (end == t.len()).then_some(plain(a)? / plain(b)?);
    }
    let s: String = t
        .iter()
        .map(|k| match k {
            Tok::Ch('\u{2212}') => Some('-'),
            Tok::Ch(c) if c.is_ascii_digit() || "+-./".contains(*c) => Some(*c),
            _ => None,
        })
        .collect::<Option<String>>()?;
    match s.split_once('/') {
        Some((a, b)) => Some(a.parse::<f64>().ok()? / b.parse::<f64>().ok()?),
        None => s.parse().ok(),
    }
}

impl Reader<'_> {
    fn compare(&mut self, values: &[Value]) {
        let known: Vec<Dims> = values
            .iter()
            .filter_map(|v| match v {
                Value::Known(d) => Some(*d),
                _ => None,
            })
            .collect();
        if known.len() > 1 {
            self.compared.push(known);
        }
    }

    fn unknown(&mut self) -> Value {
        self.unknown = true;
        Value::Unknown
    }

    /// The sides of a formula.
    fn relation(&mut self, t: &[Tok]) -> Result<Vec<Value>, Unread> {
        let mut out = vec![];
        let (mut depth, mut start) = (0i32, 0);
        for (i, k) in t.iter().enumerate() {
            match k {
                Tok::Open | Tok::Ch('(' | '[') => depth += 1,
                Tok::Close | Tok::Ch(')' | ']') => depth -= 1,
                k if depth == 0 && relation(k) => {
                    if i == start {
                        return Err(Unread);
                    }
                    out.push(self.sum(&t[start..i])?);
                    start = i + 1;
                }
                _ => {}
            }
        }
        if start >= t.len() {
            return Err(Unread);
        }
        // "x = 1" alone proves nothing; a side that is only a list of words is no formula.
        out.push(self.sum(&t[start..])?);
        Ok(out)
    }

    /// Terms that are added.
    fn sum(&mut self, t: &[Tok]) -> Result<Value, Unread> {
        self.depth += 1;
        if self.depth > 40 {
            return Err(Unread);
        }
        let mut values = vec![];
        let mut i = 0;
        let mut signed = false;
        while i < t.len() {
            match &t[i] {
                Tok::Ch('+' | '-' | '\u{2212}') => {
                    i += 1;
                    signed = true;
                }
                Tok::Cmd(n) if n == "pm" || n == "mp" => {
                    i += 1;
                    signed = true;
                }
                _ => {
                    let (v, next, number) = self.term(t, i)?;
                    // A number that is added to something may have a unit nobody prints.
                    if number && (signed || next < t.len()) {
                        self.guessed = true;
                    }
                    values.push(v);
                    i = next;
                }
            }
        }
        self.depth -= 1;
        if values.is_empty() {
            return Err(Unread);
        }
        self.compare(&values);
        Ok(values
            .iter()
            .find(|v| matches!(v, Value::Known(_)))
            .or_else(|| values.iter().find(|v| matches!(v, Value::Unknown)))
            .copied()
            .unwrap_or(Value::Any))
    }

    /// Factors that are multiplied; whether it is only a number.
    fn term(&mut self, t: &[Tok], mut i: usize) -> Result<(Value, usize, bool), Unread> {
        let mut v = ONE;
        let mut divide = false;
        let mut number = true;
        let mut first = true;
        while i < t.len() {
            match &t[i] {
                Tok::Ch('+' | '-' | '\u{2212}') if !first && !divide => break,
                Tok::Cmd(n) if n == "pm" || n == "mp" => break,
                Tok::Ch('*' | '·' | '×') => i += 1,
                Tok::Ch('/') => {
                    divide = true;
                    i += 1;
                }
                Tok::Cmd(n) if matches!(n.as_str(), "cdot" | "times" | "ast" | "odot" | "circ") => {
                    i += 1;
                }
                Tok::Cmd(n) if n == "div" => {
                    divide = true;
                    i += 1;
                }
                _ => {
                    let (f, next, plain) = self.factor(t, i)?;
                    number &= plain;
                    v = v.times(if divide { f.map(|d| d.power(-1.0)) } else { f });
                    divide = false;
                    i = next;
                }
            }
            first = false;
        }
        if first || divide {
            return Err(Unread);
        }
        Ok((v, i, number))
    }

    /// One factor with its power.
    fn factor(&mut self, t: &[Tok], i: usize) -> Result<(Value, usize, bool), Unread> {
        let (mut v, mut i, number) = self.atom(t, i)?;
        loop {
            match t.get(i) {
                Some(Tok::Sub) => i = group(t, i + 1).ok_or(Unread)?.1,
                Some(Tok::Ch('!' | '\'' | '′')) => i += 1,
                Some(Tok::Sup) => {
                    let (g, next) = group(t, i + 1).ok_or(Unread)?;
                    i = next;
                    // The transpose and other names change no unit.
                    let name = matches!(g, [Tok::Cmd(n), ..] if matches!(n.as_str(), "top" | "intercal" | "prime" | "ast" | "dagger" | "circ" | "mathrm" | "text" | "star"))
                        || matches!(g, [Tok::Ch('T' | '*' | '+' | '′' | '\'')]);
                    if name {
                        continue;
                    }
                    v = match (v, plain(g)) {
                        (Value::Known(d), Some(p)) => Value::Known(d.power(p)),
                        (Value::Known(d), None) if d.same(Dims::NONE) => v,
                        (Value::Any, _) => Value::Any,
                        _ => self.unknown(),
                    };
                }
                _ => return Ok((v, i, number)),
            }
        }
    }

    fn lookup(&mut self, s: &Symbol) -> Value {
        if let Some(d) = (self.unit)(s) {
            return Value::Known(d);
        }
        if !self.missing.contains(&s.shown) {
            self.missing.push(s.shown.clone());
        }
        self.unknown()
    }

    #[allow(clippy::too_many_lines)] // one case after the other
    fn atom(&mut self, t: &[Tok], i: usize) -> Result<(Value, usize, bool), Unread> {
        // Numbers.
        if matches!(&t[i], Tok::Ch(c) if c.is_ascii_digit()) {
            let mut k = i;
            let mut s = String::new();
            while let Some(Tok::Ch(c)) = t.get(k)
                && (c.is_ascii_digit()
                    || (*c == '.'
                        && matches!(t.get(k + 1), Some(Tok::Ch(d)) if d.is_ascii_digit())))
            {
                s.push(*c);
                k += 1;
            }
            // A measured constant may carry a unit.
            self.guessed |= s.contains('.');
            let zero = s.parse::<f64>().is_ok_and(|v| v == 0.0);
            return Ok((if zero { Value::Any } else { ONE }, k, s != "1"));
        }
        if let Some((inside, next)) = bracket(t, i) {
            let mut v = None;
            for part in parts(inside) {
                let w = self.sum(part)?;
                v.get_or_insert(w);
            }
            return Ok((v.ok_or(Unread)?, next, false));
        }
        // "d" and "∂" change no unit: what follows them does.
        if let Some(next) = differential(t, i)
            && t[i] != Tok::Ch('d')
        {
            let next = match t.get(next) {
                Some(Tok::Sup) => group(t, next + 1).ok_or(Unread)?.1,
                _ => next,
            };
            return Ok((ONE, next, false));
        }
        if t[i] == Tok::Ch('e') && t.get(i + 1) == Some(&Tok::Sup) {
            let (_, next) = group(t, i + 2).ok_or(Unread)?;
            return Ok((ONE, next, false));
        }
        if let Tok::Cmd(n) = &t[i] {
            match n.as_str() {
                "frac" | "dfrac" | "tfrac" | "cfrac" => {
                    let (a, k) = group(t, i + 1).ok_or(Unread)?;
                    let (b, next) = group(t, k).ok_or(Unread)?;
                    let derivative =
                        a.first() == Some(&Tok::Ch('d')) && b.first() == Some(&Tok::Ch('d'));
                    let (top, bottom) = if derivative {
                        (self.derived(a)?, self.derived(b)?)
                    } else {
                        (self.sum(a)?, self.sum(b)?)
                    };
                    return Ok((top.times(bottom.map(|d| d.power(-1.0))), next, false));
                }
                "sqrt" => {
                    let mut k = i + 1;
                    let mut root = 2.0;
                    if t.get(k) == Some(&Tok::Ch('[')) {
                        let (inside, next) = bracket(t, k).ok_or(Unread)?;
                        root = plain(inside).ok_or(Unread)?;
                        k = next;
                    }
                    let (a, next) = group(t, k).ok_or(Unread)?;
                    let v = self.sum(a)?;
                    return Ok((v.map(|d| d.power(1.0 / root)), next, false));
                }
                "sum" | "int" | "oint" | "iint" | "iiint" => {
                    let mut k = i + 1;
                    while matches!(t.get(k), Some(Tok::Sub | Tok::Sup)) {
                        k = group(t, k + 1).ok_or(Unread)?.1;
                    }
                    // What is added up, to the end of the term.
                    let rest = &t[k..];
                    if n != "sum" {
                        let (v, used) = self.integrand(rest)?;
                        return Ok((v, k + used, false));
                    }
                    return Ok((ONE, k, false));
                }
                "nabla" => {
                    let inverse = Dims([1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0]).power(-1.0);
                    return Ok((Value::Known(inverse), i + 1, false));
                }
                "Delta" if !matches!(t.get(i + 1), None | Some(Tok::Sub | Tok::Sup)) => {
                    // A difference; it could be a Laplacian.
                    self.guessed = true;
                    return Ok((ONE, i + 1, false));
                }
                "pi" => return Ok((ONE, i + 1, false)),
                "infty" => return Ok((Value::Any, i + 1, false)),
                "mid" | "in" | "to" | "rightarrow" | "Rightarrow" | "mapsto" | "propto" | "sim"
                | "forall" | "exists" | "begin" | "end" | "\\" | "underbrace" | "overbrace"
                | "cup" | "cap" | "subset" | "prod" | "leftarrow" | "substack" | "dots"
                | "ldots" | "cdots" => return Err(Unread),
                _ if FUNCTIONS.contains(&n.as_str()) => return self.function(t, i + 1),
                _ => {}
            }
        }
        // A function written in upright letters.
        if let Tok::Cmd(n) = &t[i]
            && matches!(n.as_str(), "mathrm" | "operatorname" | "text" | "rm")
            && let Some((g, next)) = group(t, i + 1)
        {
            let word: String = g
                .iter()
                .filter_map(|k| match k {
                    Tok::Ch(c) => Some(*c),
                    _ => None,
                })
                .collect();
            if FUNCTIONS.contains(&word.to_lowercase().as_str()) {
                return self.function(t, next);
            }
        }
        let (s, next) = symbol(t, i).ok_or(Unread)?;
        let v = self.valued(&s);
        // "u(x, t)" is u; "k (a + b)" is a product; "h(T)" could be both.
        if let Some(Tok::Ch('(')) = t.get(next)
            && let Some((inside, after)) = bracket(t, next)
        {
            let plain = inside.iter().all(|k| {
                matches!(k, Tok::Ch(c) if "txyzrsθ,".contains(*c))
                    || matches!(k, Tok::Cmd(n) if n == "theta" || n == "cdot" || n == "tau")
            });
            let listed = inside.contains(&Tok::Ch(','));
            if plain || listed {
                return Ok((v, after, false));
            }
            if symbol(inside, 0).is_some_and(|(_, end)| end == inside.len()) {
                self.guessed = true;
            }
        }
        Ok((v, next, false))
    }

    /// The unit of a symbol; one with a dot and no unit of its own has that of its base in a second.
    fn valued(&mut self, s: &Symbol) -> Value {
        if (self.unit)(s).is_none()
            && let Some(m) = s.mark
        {
            let bare = Symbol {
                accent: false,
                mark: None,
                ..s.clone()
            };
            if let Some(d) = (self.unit)(&bare) {
                self.guessed = true;
                let rate = if accent("dot") == Some(m) { -1.0 } else { 0.0 };
                return Value::Known(d.times(Dims([0.0, 0.0, rate, 0.0, 0.0, 0.0, 0.0])));
            }
        }
        self.lookup(s)
    }

    /// A function has no unit, whatever is in it.
    fn function(&mut self, t: &[Tok], mut i: usize) -> Result<(Value, usize, bool), Unread> {
        while matches!(t.get(i), Some(Tok::Sub | Tok::Sup)) {
            i = group(t, i + 1).ok_or(Unread)?.1;
        }
        if i >= t.len() {
            return Err(Unread);
        }
        // Without brackets its argument is the next factor.
        let next = match bracket(t, i) {
            Some((inside, next)) => {
                for part in parts(inside) {
                    self.sum(part)?;
                }
                next
            }
            None => self.factor(t, i)?.1,
        };
        Ok((ONE, next, false))
    }

    /// The top or bottom of a derivative: every d changes no unit.
    fn derived(&mut self, t: &[Tok]) -> Result<Value, Unread> {
        let mut rest: Vec<Tok> = vec![];
        let mut i = 0;
        while i < t.len() {
            if let Some(next) = differential(t, i).or((t[i] == Tok::Ch('d')).then_some(i + 1)) {
                i = match t.get(next) {
                    Some(Tok::Sup) => group(t, next + 1).ok_or(Unread)?.1,
                    _ => next,
                };
                continue;
            }
            rest.push(t[i].clone());
            i += 1;
        }
        if rest.is_empty() {
            return Ok(ONE);
        }
        self.sum(&rest)
    }

    /// What is integrated, with the "dx" after it; how far it goes.
    fn integrand(&mut self, t: &[Tok]) -> Result<(Value, usize), Unread> {
        let mut depth = 0i32;
        let mut end = t.len();
        for (i, k) in t.iter().enumerate() {
            match k {
                Tok::Open | Tok::Ch('(' | '[') => depth += 1,
                Tok::Close | Tok::Ch(')' | ']') => depth -= 1,
                k if depth == 0 && relation(k) => {
                    end = i;
                    break;
                }
                Tok::Ch('+' | '-' | '\u{2212}') if depth == 0 && i > 0 => {
                    end = i;
                    break;
                }
                _ => {}
            }
        }
        // Taken to the end of the term, without its d's.
        let v = self.derived(&t[..end])?;
        Ok((v, end))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::units;

    fn run(latex: &str, known: &[(&str, &str)]) -> Check {
        check(latex, &|s: &Symbol| {
            known
                .iter()
                .find(|(n, _)| *n == s.shown)
                .and_then(|(_, u)| units::parse(u))
        })
    }

    const HEAT: [(&str, &str); 8] = [
        ("Q", "W"),
        ("m\u{307}", "kg/s"),
        ("c_p", "J/(kg K)"),
        ("T_in", "°C"),
        ("T_out", "K"),
        ("h", "W/(m2 K)"),
        ("A", "m2"),
        ("m", "kg"),
    ];

    #[test]
    fn units_that_agree() {
        let c = run(r"Q = \dot{m} c_{p} \left( T_{in} - T_{out} \right)", &HEAT);
        assert_eq!(
            (c.agree, c.sides),
            (
                Some(true),
                vec!["kg·m²·s⁻³".to_string(), "kg·m²·s⁻³".into()]
            )
        );
        let u = [
            ("ρ", "kg/m3"),
            ("u", "m/s"),
            ("t", "s"),
            ("x", "m"),
            ("μ", "Pa s"),
            ("p", "Pa"),
        ];
        let c = run(
            r"\rho \left( \frac { \partial u } { \partial t } + u \frac{\partial u}{\partial x} \right) = - \frac{\partial p}{\partial x} + \mu \frac{\partial^{2} u}{\partial x^{2}}",
            &u,
        );
        assert_eq!(c.agree, Some(true), "{c:?}");
        let c = run(r"\rho \frac{d u}{d t} = - \nabla p + \mu \nabla^{2} u", &u);
        assert_eq!(c.agree, Some(true), "{c:?}");
        let c = run(
            r"E = \int_{0}^{t} Q \, d t",
            &[("E", "kWh"), ("Q", "W"), ("t", "s")],
        );
        assert_eq!(c.agree, Some(true), "{c:?}");
        let c = run(
            r"v = \sqrt{2 g H}",
            &[("v", "m/s"), ("g", "m/s2"), ("H", "m")],
        );
        assert_eq!(c.agree, Some(true), "{c:?}");
    }

    #[test]
    fn units_that_do_not_agree() {
        let c = run(r"Q = h A T_{in} + m", &HEAT);
        assert_eq!(c.agree, Some(false), "{c:?}");
        let c = run(r"Q = h \left( T_{in} - T_{out} \right)", &HEAT);
        assert_eq!(c.agree, Some(false));
        assert_eq!(c.sides, ["kg·m²·s⁻³", "kg·s⁻³"]);
    }

    #[test]
    fn what_is_not_known_is_not_judged() {
        let c = run(r"Q = U A T_{in}", &HEAT);
        assert_eq!((c.agree, c.missing), (None, vec!["U".to_string()]));
        // A dot that may mean a rate, a measured constant, a difference: never "they disagree".
        for f in [
            r"Q = \dot{T}_{in}",
            r"h = 5.7 + 3.8 A",
            r"Q = h A \Delta m",
            r"h = 12 + A",
        ] {
            assert_eq!(run(f, &HEAT).agree, None, "{f}");
        }
        // A power that is no number.
        assert_eq!(run(r"Q = A^{n}", &HEAT).agree, None);
        assert_eq!(run(r"Q", &HEAT).agree, None, "nothing to compare");
    }

    #[test]
    fn what_cannot_be_read_is_said() {
        for f in [
            r"\begin{aligned} a &= b \end{aligned}",
            r"x \in \Omega",
            r"f : X \to Y",
            r"= Q",
            r"Q = \frac{a}",
        ] {
            assert!(run(f, &HEAT).unread, "{f}");
        }
        assert!(!run(r"Q = h A T_{in} .", &HEAT).unread);
    }

    #[test]
    fn functions_numbers_and_zero_fit() {
        let k = [
            ("y", "-"),
            ("t", "s"),
            ("ω", "rad/s"),
            ("a", "m"),
            ("b", "m"),
        ];
        assert_eq!(
            run(r"y = \sin(\omega t) + e^{-2} + 1", &k).agree,
            Some(true)
        );
        assert_eq!(run(r"a - b = 0", &k).agree, Some(true));
        assert_eq!(run(r"a(t) = b(t) + 2 a", &k).agree, Some(true));
        assert_eq!(
            run(r"a = \frac{1}{2} b, \quad y = a", &k).agree,
            Some(false)
        );
    }
}
