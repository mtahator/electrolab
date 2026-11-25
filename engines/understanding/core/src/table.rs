//! An answer from a table (P1.M5.UND.6): which cells and which operation come from a plan, the
//! cells are picked, the number computed and the sentence written here.

use std::sync::LazyLock;

use regex::Regex;

use crate::units;

/// Most rows and columns of a table that is asked.
pub const MOST_ROWS: usize = 120;
/// Most columns.
pub const MOST_COLUMNS: usize = 24;
/// Name for every row or column in a plan.
pub const ALL: &str = "all";
/// Most cells that are listed.
const MOST_LISTED: usize = 12;
/// Most rows that are named.
const MOST_TOLD: usize = 30;

/// A number as it is printed in a cell.
#[derive(Debug, Clone, PartialEq)]
pub struct Number {
    /// Its value.
    pub value: f64,
    /// Digits after the decimal point as printed.
    pub decimals: usize,
    /// Unit printed in the cell.
    pub unit: Option<String>,
}

/// The number in a cell: "0.209◦C", "4.80 ± 0.18", "1.2 × 10−4", "1 000", "31.54%", "1,234.5†".
#[must_use]
pub fn number(text: &str) -> Option<Number> {
    static TIMES: LazyLock<Regex> = LazyLock::new(|| {
        crate::re(
            r"^([-+]?\d+(?:\.\d+)?)\s*[×x·\u{22c5}]\s*10\s*\^?\s*\{?([-+]?\s?\d{1,2})\}?\s*(.*)$",
        )
    });
    static GROUPED: LazyLock<Regex> =
        LazyLock::new(|| crate::re(r"^[-+]?\d{1,3}(?:,\d{3})+(?:\D|$)"));
    static POWER: LazyLock<Regex> = LazyLock::new(|| crate::re(r"^10\s?\^?(-\d{1,2})$"));
    static PLAIN: LazyLock<Regex> = LazyLock::new(|| {
        crate::re(r"^([-+]?(?:\d{1,3}(?: \d{3})+|\d+)?(?:\.\d+)?(?:[eE][-+]?\d+)?)\s*(.*)$")
    });
    let t = text.trim().replace(['\u{2212}', '\u{2013}'], "-");
    let t = t.split(['±', '\u{b1}']).next().unwrap_or("").trim();
    let t = t
        .trim_start_matches(['(', '['])
        .trim_end_matches(['*', '†', '‡', ')', ']', '∗'])
        .trim();
    let tail = |s: &str| -> Option<Option<String>> {
        let s = s.trim();
        if s.is_empty() {
            return Some(None);
        }
        // What follows a number is a unit, or it is no number at all ("2 layers").
        units::parse(s).map(|_| Some(s.to_string()))
    };
    let decimals = |s: &str| {
        s.split(['e', 'E'])
            .next()
            .and_then(|m| m.split_once('.'))
            .map_or(0, |(_, d)| d.len())
    };
    if let Some(c) = TIMES.captures(t) {
        let m: f64 = c[1].parse().ok()?;
        let e: i32 = c[2].replace(' ', "").parse().ok()?;
        let d = i32::try_from(decimals(&c[1])).ok()? - e;
        return Some(Number {
            value: m * 10f64.powi(e),
            decimals: usize::try_from(d.max(0)).unwrap_or(0),
            unit: tail(&c[3])?,
        });
    }
    if let Some(c) = POWER.captures(t) {
        let e: i32 = c[1].parse().ok()?;
        return Some(Number {
            value: 10f64.powi(e),
            decimals: usize::try_from(-e).unwrap_or(0),
            unit: None,
        });
    }
    // "1,234,567.8" groups by three; a comma otherwise is not read as a number.
    let t = if GROUPED.is_match(t) {
        t.replace(',', "")
    } else {
        t.to_string()
    };
    let c = PLAIN.captures(&t)?;
    let digits = c[1].replace(' ', "");
    if !digits.chars().any(|c| c.is_ascii_digit()) {
        return None;
    }
    Some(Number {
        value: digits.parse().ok()?,
        decimals: decimals(&digits),
        unit: tail(&c[2])?,
    })
}

/// A table to be asked: cells by rows, with how many of the first rows are headings.
#[derive(Debug, Clone, Default)]
pub struct Grid {
    /// Cells; every row has the same number.
    pub cells: Vec<Vec<String>>,
    /// Heading rows.
    pub header_rows: usize,
    /// Unit of every column, when printed in its heading.
    pub units: Vec<Option<String>>,
}

/// What is done with the cells.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Op {
    /// One cell, or a few.
    Value,
    /// The cells, one after the other.
    List,
    /// The largest.
    Largest,
    /// The smallest.
    Smallest,
    /// Their average.
    Average,
    /// Their sum.
    Sum,
    /// How many rows.
    Count,
    /// The first less the second.
    Difference,
    /// The first divided by the second.
    Ratio,
    /// From the first to the second, in percent of the first.
    Change,
    /// The table does not hold the answer.
    None,
}

impl Op {
    /// Every operation with its name in a plan.
    pub const NAMES: [(&'static str, Op); 11] = [
        ("value", Op::Value),
        ("list", Op::List),
        ("largest", Op::Largest),
        ("smallest", Op::Smallest),
        ("average", Op::Average),
        ("sum", Op::Sum),
        ("count", Op::Count),
        ("difference", Op::Difference),
        ("ratio", Op::Ratio),
        ("change", Op::Change),
        ("none", Op::None),
    ];

    /// By its name.
    #[must_use]
    pub fn named(name: &str) -> Option<Op> {
        Self::NAMES
            .iter()
            .find(|(n, _)| *n == name)
            .map(|(_, o)| *o)
    }

    /// Its name.
    #[must_use]
    pub fn name(self) -> &'static str {
        Self::NAMES
            .iter()
            .find(|(_, o)| *o == self)
            .map_or("none", |(n, _)| n)
    }
}

/// How a condition is tested.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Test {
    /// The same text or number.
    Equals,
    /// The text is in it.
    Contains,
    /// The number is larger.
    Above,
    /// The number is smaller.
    Below,
}

impl Test {
    /// Every test with its name in a plan.
    pub const NAMES: [(&'static str, Test); 4] = [
        ("equals", Test::Equals),
        ("contains", Test::Contains),
        ("above", Test::Above),
        ("below", Test::Below),
    ];
}

/// A condition on rows.
#[derive(Debug, Clone, PartialEq)]
pub struct Where {
    /// Column.
    pub column: usize,
    /// Test.
    pub test: Test,
    /// Against this.
    pub value: String,
}

/// Which cells and which operation. Columns and rows are indexes (rows without the headings), in
/// the order they were named; empty means all.
#[derive(Debug, Clone, PartialEq)]
pub struct Plan {
    /// Operation.
    pub op: Op,
    /// Columns.
    pub columns: Vec<usize>,
    /// Rows.
    pub rows: Vec<usize>,
    /// Only rows for which these hold.
    pub conditions: Vec<Where>,
}

/// Why there is no answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refusal {
    /// The table does not hold it.
    NotInTable,
    /// Cells that are not numbers.
    NotNumbers,
    /// Different units.
    DifferentUnits,
    /// Two cells are needed.
    NotTwo,
}

/// A cell that was used: its row in the table (headings counted) and its column.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Used {
    /// Row, headings counted.
    pub row: usize,
    /// Column.
    pub col: usize,
    /// It is the answer itself.
    pub answer: bool,
}

/// An answer from a table.
#[derive(Debug, Clone, PartialEq)]
pub struct Answer {
    /// The sentence.
    pub text: String,
    /// Cells.
    pub cells: Vec<Used>,
    /// The number, when it is one.
    pub value: Option<f64>,
    /// Its unit.
    pub unit: Option<String>,
}

fn squash(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Which of the names the question says, as whole words; a name inside a longer one that is said
/// ("FNO" in "PITT FNO") does not count.
fn said(names: &[String], question: &str) -> Vec<usize> {
    let words = |s: &str| -> Vec<String> {
        s.to_lowercase()
            .split(|c: char| !c.is_alphanumeric())
            .filter(|w| !w.is_empty())
            .map(str::to_string)
            .collect()
    };
    let asked = words(question);
    let names: Vec<Vec<String>> = names.iter().map(|n| words(n)).collect();
    let is_in = |small: &[String], big: &[String]| {
        !small.is_empty() && big.windows(small.len()).any(|w| w == small)
    };
    let there: Vec<usize> = (0..names.len())
        .filter(|i| is_in(&names[*i], &asked))
        .collect();
    there
        .iter()
        .copied()
        .filter(|i| {
            !there
                .iter()
                .any(|j| names[*j].len() > names[*i].len() && is_in(&names[*i], &names[*j]))
        })
        .collect()
}

impl Grid {
    fn columns(&self) -> usize {
        self.cells.first().map_or(0, Vec::len)
    }

    fn body(&self) -> &[Vec<String>] {
        self.cells.get(self.header_rows..).unwrap_or(&[])
    }

    /// Whether the table is small enough to be asked.
    #[must_use]
    pub fn fits(&self) -> bool {
        self.cells.len() <= MOST_ROWS && self.columns() <= MOST_COLUMNS
    }

    /// Names of the columns: their headings, several rows joined, a heading over several columns
    /// given to all of them.
    #[must_use]
    pub fn column_names(&self) -> Vec<String> {
        let heads = &self.cells[..self.header_rows.min(self.cells.len())];
        let last = heads.len().saturating_sub(1);
        (0..self.columns())
            .map(|c| {
                let mut parts: Vec<String> = Vec::new();
                for (r, row) in heads.iter().enumerate() {
                    let own = squash(&row[c]);
                    let text =
                        if own.is_empty() && r < last && c > 0 && !heads[last][c].trim().is_empty()
                        {
                            let left = (1..c).rev().find(|k| !row[*k].trim().is_empty());
                            let right = (c + 1..row.len()).find(|k| !row[*k].trim().is_empty());
                            left.or(right).map(|k| squash(&row[k])).unwrap_or_default()
                        } else {
                            own
                        };
                    if !text.is_empty() && !parts.contains(&text) {
                        parts.push(text);
                    }
                }
                if parts.is_empty() {
                    format!("column {}", c + 1)
                } else {
                    parts.join(" ")
                }
            })
            .collect()
    }

    /// How many of the first columns name the rows.
    fn label_columns(&self) -> usize {
        let body = self.body();
        let numeric = |c: usize| {
            let filled: Vec<_> = body.iter().filter(|r| !r[c].trim().is_empty()).collect();
            filled.iter().filter(|r| number(&r[c]).is_some()).count() * 2 >= filled.len().max(1)
        };
        (0..self.columns())
            .take_while(|c| !numeric(*c))
            .count()
            .clamp(1, 2)
            .min(self.columns())
    }

    /// Names of the rows below the headings; an empty first cell has the name above it.
    #[must_use]
    pub fn row_names(&self) -> Vec<String> {
        let labels = self.label_columns();
        let mut above = String::new();
        self.body()
            .iter()
            .enumerate()
            .map(|(i, r)| {
                let first = squash(&r[0]);
                let mut name = if first.is_empty() {
                    above.clone()
                } else {
                    first
                };
                above.clone_from(&name);
                for c in r.iter().take(labels).skip(1) {
                    let t = squash(c);
                    if !t.is_empty() && number(&t).is_none() && t.chars().count() <= 40 {
                        name = format!("{name} {t}").trim().to_string();
                    }
                }
                if name.is_empty() {
                    format!("row {}", i + 1)
                } else {
                    name
                }
            })
            .collect()
    }

    fn unit_of(&self, row: usize, col: usize, n: &Number) -> Option<String> {
        if n.unit.is_some() {
            return n.unit.clone();
        }
        if let Some(u) = self
            .units
            .get(col)
            .cloned()
            .flatten()
            .filter(|u| units::parse(u).is_some())
        {
            return Some(u);
        }
        let names = self.column_names();
        units::in_heading(&names[col])
            .or_else(|| units::in_heading(&self.row_names()[row]))
            .or_else(|| units::in_heading(&self.body()[row][0]))
    }

    fn holds(row: &[String], w: &Where) -> bool {
        let Some(cell) = row.get(w.column) else {
            return false;
        };
        let (a, b) = (squash(cell).to_lowercase(), squash(&w.value).to_lowercase());
        let numbers = || Some((number(cell)?.value, number(&w.value)?.value));
        match w.test {
            Test::Equals => {
                a == b || numbers().is_some_and(|(x, y)| (x - y).abs() <= 1e-9 * y.abs().max(1.0))
            }
            Test::Contains => a.contains(&b),
            Test::Above => numbers().is_some_and(|(x, y)| x > y),
            Test::Below => numbers().is_some_and(|(x, y)| x < y),
        }
    }

    /// The table for somebody who tells about it: what its columns and rows are, and what is
    /// smallest and largest, worked out here (a small model mixes up the cells of a table). Cells
    /// are given only where a row is one name with its value, or words.
    #[must_use]
    pub fn outline(&self) -> String {
        let (labels, cols, names) = (self.label_columns(), self.column_names(), self.row_names());
        let data = &cols[labels.min(cols.len())..];
        let mut out = vec![format!("Columns: {}", data.join(" | "))];
        let body = self.body();
        let (mut filled, mut words) = (0, 0);
        for cell in body.iter().flat_map(|r| &r[labels.min(r.len())..]) {
            if !cell.trim().is_empty() {
                filled += 1;
                words += usize::from(number(cell).is_none());
            }
        }
        if data.len() <= 1 || words * 2 > filled {
            out.extend(body.iter().take(MOST_TOLD).zip(&names).map(|(r, name)| {
                let cells: Vec<String> =
                    r[labels.min(r.len())..].iter().map(|c| squash(c)).collect();
                format!("{name}: {}", cells.join(" | "))
            }));
            return out.join("\n");
        }
        let told: Vec<&str> = names.iter().take(MOST_TOLD).map(String::as_str).collect();
        out.push(format!("Rows: {}", told.join(" | ")));
        let mut facts: Vec<String> = (labels..cols.len())
            .filter_map(|c| {
                let cells = (0..body.len()).map(|r| (r, c));
                self.extremes(&cols[c], cells, |(r, _)| &names[r])
            })
            .collect();
        if facts.is_empty() {
            facts = (0..body.len())
                .filter_map(|r| {
                    let cells = (labels..cols.len()).map(|c| (r, c));
                    self.extremes(&names[r], cells, |(_, c)| &cols[c])
                })
                .collect();
        }
        out.extend(facts.into_iter().take(10));
        out.join("\n")
    }

    /// "Smallest Error: 0.31 K (GRU); largest Error: 0.87 K (LSTM)." of at least three numbers.
    fn extremes<'a>(
        &self,
        what: &str,
        cells: impl Iterator<Item = (usize, usize)>,
        name: impl Fn((usize, usize)) -> &'a String,
    ) -> Option<String> {
        let (mut read, mut filled) = (vec![], 0);
        for at in cells {
            let text = &self.body()[at.0][at.1];
            filled += usize::from(!text.trim().is_empty());
            if let Some(n) = number(text) {
                let unit = self.unit_of(at.0, at.1, &n);
                read.push((at, n.value, unit));
            }
        }
        if read.len() < 3 || read.len() * 4 < filled * 3 {
            return None;
        }
        let unit = read[0].2.clone();
        if !read
            .iter()
            .all(|(_, _, u)| same_unit(u.as_deref(), unit.as_deref()))
        {
            return None;
        }
        let low = read.iter().min_by(|a, b| a.1.total_cmp(&b.1))?.0;
        let high = read.iter().max_by(|a, b| a.1.total_cmp(&b.1))?.0;
        let say = |at: (usize, usize)| {
            let cell = squash(&self.body()[at.0][at.1]);
            format!("{cell} ({})", name(at))
        };
        Some(format!(
            "Smallest {what}: {}; largest {what}: {}.",
            say(low),
            say(high)
        ))
    }

    /// A plan that reads whole rows or columns, narrowed to those whose names the question says.
    #[must_use]
    pub fn narrowed(&self, mut plan: Plan, question: &str) -> Plan {
        if plan.op != Op::Value {
            return plan;
        }
        let rows = self.row_names();
        if plan.rows.len() != 1 && rows.len() > 1 {
            let said = said(&rows, question);
            if !said.is_empty() {
                plan.rows = said;
            }
        }
        let (labels, cols) = (self.label_columns(), self.column_names());
        if plan.columns.len() != 1 && cols.len() > labels + 1 {
            let said = said(&cols[labels..], question);
            if !said.is_empty() {
                plan.columns = said.into_iter().map(|c| c + labels).collect();
            }
        }
        plan
    }

    /// The answer a plan leads to.
    ///
    /// # Errors
    /// Why there is none.
    #[allow(clippy::too_many_lines)] // one operation after the other
    pub fn answer(&self, plan: &Plan) -> Result<Answer, Refusal> {
        let body = self.body();
        let (cols, names) = (self.column_names(), self.row_names());
        let all_rows = plan.rows.is_empty();
        let rows: Vec<usize> = if all_rows {
            (0..body.len()).collect()
        } else {
            plan.rows.clone()
        }
        .into_iter()
        .filter(|r| {
            body.get(*r)
                .is_some_and(|row| plan.conditions.iter().all(|w| Self::holds(row, w)))
        })
        .collect();
        if plan.op == Op::None || rows.is_empty() {
            return Err(Refusal::NotInTable);
        }
        let head = self.header_rows;
        let condition = plan
            .conditions
            .iter()
            .map(|w| {
                let how = match w.test {
                    Test::Equals => "is",
                    Test::Contains => "contains",
                    Test::Above => "is above",
                    Test::Below => "is below",
                };
                format!(
                    "{} {how} {}",
                    cols.get(w.column).map_or("", String::as_str),
                    w.value
                )
            })
            .collect::<Vec<_>>()
            .join(" and ");
        let where_ = if condition.is_empty() {
            String::new()
        } else {
            format!(" where {condition}")
        };
        if plan.op == Op::Count {
            let col = plan.columns.first().copied().unwrap_or(0);
            return Ok(Answer {
                text: format!(
                    "{} {}{where_}.",
                    rows.len(),
                    if rows.len() == 1 { "row" } else { "rows" }
                ),
                cells: rows
                    .iter()
                    .map(|r| Used {
                        row: r + head,
                        col,
                        answer: false,
                    })
                    .collect(),
                #[allow(clippy::cast_precision_loss)]
                value: Some(rows.len() as f64),
                unit: None,
            });
        }
        let labels = self.label_columns();
        let mut columns: Vec<usize> = if plan.columns.is_empty() {
            (labels..cols.len()).collect()
        } else {
            plan.columns
                .iter()
                .copied()
                .filter(|c| *c < cols.len())
                .collect()
        };
        // A row's name is said with its cells already.
        if columns.iter().any(|c| *c >= labels) {
            columns.retain(|c| *c >= labels);
        }
        let mut cells: Vec<(usize, usize)> = rows
            .iter()
            .flat_map(|r| columns.iter().map(move |c| (*r, *c)))
            .filter(|(r, c)| !body[*r][*c].trim().is_empty())
            .collect();
        if cells.is_empty() {
            return Err(Refusal::NotInTable);
        }
        let at = |(r, c): (usize, usize)| {
            let (row, col) = (&names[r], &cols[c]);
            if c == 0 || col.starts_with("column ") {
                row.clone()
            } else {
                format!("{row}, {col}")
            }
        };
        let shown = |(r, c): (usize, usize)| {
            let text = squash(&body[r][c]);
            match number(&text) {
                Some(n) if n.unit.is_none() => match self.unit_of(r, c, &n) {
                    Some(u) => with_unit(&text, &u),
                    None => text,
                },
                _ => text,
            }
        };
        if matches!(plan.op, Op::Value | Op::List) {
            let more = cells.len().saturating_sub(MOST_LISTED);
            let text = if cells.len() == 1 {
                format!("{}: {}.", at(cells[0]), shown(cells[0]))
            } else {
                let list: Vec<String> = cells
                    .iter()
                    .take(MOST_LISTED)
                    .map(|k| format!("{}: {}", at(*k), shown(*k)))
                    .collect();
                let and = if more > 0 {
                    format!("; and {more} more")
                } else {
                    String::new()
                };
                format!("{}{and}.", list.join("; "))
            };
            let one = cells.len() == 1;
            let n = if one {
                number(&body[cells[0].0][cells[0].1])
            } else {
                None
            };
            return Ok(Answer {
                text,
                unit: n
                    .as_ref()
                    .and_then(|n| self.unit_of(cells[0].0, cells[0].1, n)),
                value: n.map(|n| n.value),
                cells: cells
                    .iter()
                    .map(|(r, c)| Used {
                        row: r + head,
                        col: *c,
                        answer: one,
                    })
                    .collect(),
            });
        }

        // Everything else computes: numbers, in one unit.
        let mut ns: Vec<Number> = Vec::new();
        cells.retain(|(r, c)| {
            let n = number(&body[*r][*c]);
            // A cell that says "-" or "n/a" is left out of a range, not of a pair.
            let keep = n.is_some();
            ns.extend(n);
            keep
        });
        if cells.is_empty() {
            return Err(Refusal::NotNumbers);
        }
        let us: Vec<Option<String>> = cells
            .iter()
            .zip(&ns)
            .map(|((r, c), n)| self.unit_of(*r, *c, n))
            .collect();
        let unit = us[0].clone();
        if us.iter().any(|u| !same_unit(u.as_deref(), unit.as_deref())) {
            return Err(Refusal::DifferentUnits);
        }
        let with = |v: f64, d: usize, u: Option<&str>| {
            let s = plain(v, d);
            u.map_or(s.clone(), |u| with_unit(&s, u))
        };
        let decimals = ns.iter().map(|n| n.decimals).max().unwrap_or(0);
        let many = if rows.len() > 1 && columns.len() == 1 {
            format!("{} rows", cells.len())
        } else if rows.len() == 1 && columns.len() > 1 {
            format!("{} columns", cells.len())
        } else {
            format!("{} cells", cells.len())
        };
        let what = if columns.len() == 1 {
            cols[columns[0]].clone()
        } else if rows.len() == 1 {
            names[rows[0]].clone()
        } else {
            "the table".to_string()
        };
        let values: Vec<f64> = ns.iter().map(|n| n.value).collect();
        let used = |answer: Option<usize>| -> Vec<Used> {
            cells
                .iter()
                .enumerate()
                .map(|(i, (r, c))| Used {
                    row: r + head,
                    col: *c,
                    answer: Some(i) == answer,
                })
                .collect()
        };
        #[allow(clippy::cast_precision_loss)]
        let count = values.len() as f64;
        match plan.op {
            Op::Largest | Op::Smallest => {
                let best = (0..values.len())
                    .reduce(|a, b| {
                        let better = if plan.op == Op::Largest {
                            values[b] > values[a]
                        } else {
                            values[b] < values[a]
                        };
                        if better { b } else { a }
                    })
                    .unwrap_or(0);
                // The same value more than once is said.
                #[allow(clippy::float_cmp)]
                let ties: Vec<usize> = (0..values.len())
                    .filter(|i| values[*i] == values[best])
                    .collect();
                let place = |i: usize| {
                    let (r, c) = cells[i];
                    if columns.len() == 1 {
                        names[r].clone()
                    } else if rows.len() == 1 {
                        cols[c].clone()
                    } else {
                        at(cells[i])
                    }
                };
                let places: Vec<String> = ties.iter().take(4).map(|i| place(*i)).collect();
                let name = if plan.op == Op::Largest {
                    "Largest"
                } else {
                    "Smallest"
                };
                Ok(Answer {
                    text: format!(
                        "{name} of {what} over {many}{where_}: {}, for {}.",
                        shown(cells[best]),
                        places.join(" and ")
                    ),
                    cells: used(None)
                        .into_iter()
                        .enumerate()
                        .map(|(i, u)| Used {
                            answer: ties.contains(&i),
                            ..u
                        })
                        .collect(),
                    value: Some(values[best]),
                    unit,
                })
            }
            Op::Average | Op::Sum => {
                let sum: f64 = values.iter().sum();
                let (name, v, d) = if plan.op == Op::Sum {
                    ("Sum", sum, decimals)
                } else {
                    ("Average", sum / count, decimals + 1)
                };
                Ok(Answer {
                    text: format!(
                        "{name} of {what} over {many}{where_}: {}.",
                        with(v, d, unit.as_deref())
                    ),
                    cells: used(None),
                    value: Some(v),
                    unit,
                })
            }
            Op::Difference | Op::Ratio | Op::Change => {
                if values.len() != 2 {
                    return Err(Refusal::NotTwo);
                }
                let (a, b) = (values[0], values[1]);
                let (x, y) = (shown(cells[0]), shown(cells[1]));
                let (p, q) = (at(cells[0]), at(cells[1]));
                let (text, value, unit) = match plan.op {
                    Op::Difference => (
                        format!(
                            "{p} less {q}: {x} − {y} = {}.",
                            with(a - b, decimals, unit.as_deref())
                        ),
                        a - b,
                        unit,
                    ),
                    Op::Ratio if b != 0.0 => (
                        format!("{p} over {q}: {x} / {y} = {}.", significant(a / b)),
                        a / b,
                        None,
                    ),
                    Op::Change if a != 0.0 => {
                        let v = (b - a) / a.abs() * 100.0;
                        let sign = if v > 0.0 { "+" } else { "" };
                        (
                            format!("From {p} to {q}: {x} → {y}, {sign}{} %.", significant(v)),
                            v,
                            Some("%".to_string()),
                        )
                    }
                    _ => return Err(Refusal::NotNumbers),
                };
                Ok(Answer {
                    text,
                    cells: used(None),
                    value: Some(value),
                    unit,
                })
            }
            Op::Value | Op::List | Op::Count | Op::None => Err(Refusal::NotInTable),
        }
    }
}

fn same_unit(a: Option<&str>, b: Option<&str>) -> bool {
    let clean = |u: &str| u.replace([' ', '\u{25e6}'], "").replace('º', "°");
    match (a, b) {
        (Some(a), Some(b)) => clean(a) == clean(b),
        (None, None) => true,
        _ => false,
    }
}

fn with_unit(number: &str, unit: &str) -> String {
    format!("{number} {}", unit.trim().replace('\u{25e6}', "°"))
}

/// With this many digits after the point, "−" for a minus.
fn plain(v: f64, decimals: usize) -> String {
    let s = format!("{:.*}", decimals.min(12), v);
    let zero = s
        .trim_start_matches('-')
        .chars()
        .all(|c| c == '0' || c == '.');
    if zero {
        s.trim_start_matches('-').to_string()
    } else {
        s.replace('-', "−")
    }
}

/// Three digits that count.
fn significant(v: f64) -> String {
    if v == 0.0 || !v.is_finite() {
        return "0".into();
    }
    #[allow(clippy::cast_possible_truncation)] // a logarithm
    let left = v.abs().log10().floor() as i32;
    plain(v, usize::try_from((2 - left).max(0)).unwrap_or(0))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::float_cmp)]
mod tests {
    use super::*;

    fn grid(rows: &[&str], header_rows: usize) -> Grid {
        let cells: Vec<Vec<String>> = rows
            .iter()
            .map(|r| r.split('|').map(|c| c.trim().to_string()).collect())
            .collect();
        Grid {
            units: vec![None; cells[0].len()],
            cells,
            header_rows,
        }
    }
    fn plan(op: Op, columns: &[usize], rows: &[usize]) -> Plan {
        Plan {
            op,
            columns: columns.to_vec(),
            rows: rows.to_vec(),
            conditions: vec![],
        }
    }
    fn models() -> Grid {
        let mut g = grid(
            &[
                "Model|Error (K)|Time [s]|Note",
                "PINN|0.42|12|-",
                "LSTM|0.87|9.5|fast",
                "GRU|0.31|n/a|-",
                "MLP|0.31|20|old",
            ],
            1,
        );
        g.units = vec![None, Some("K".into()), Some("s".into()), None];
        g
    }

    #[test]
    fn numbers_as_they_are_printed() {
        for (t, v, u) in [
            ("0.209◦C", 0.209, Some("◦C")),
            ("31.54%", 31.54, Some("%")),
            ("1528.5s", 1528.5, Some("s")),
            ("4.80 ± 0.18", 4.8, None),
            ("1.2 × 10−4", 0.00012, None),
            ("6.217 × 104", 62170.0, None),
            ("49 695", 49695.0, None),
            ("1,234.5†", 1234.5, None),
            ("−0.7", -0.7, None),
            ("1.22E-03", 0.00122, None),
            ("10−7", 1e-7, None),
            (".5", 0.5, None),
        ] {
            let n = number(t).unwrap_or_else(|| panic!("{t}"));
            assert!(
                (n.value - v).abs() <= 1e-12 * v.abs().max(1.0),
                "{t}: {}",
                n.value
            );
            assert_eq!(n.unit.as_deref(), u, "{t}");
        }
        for t in [
            "PINN",
            "-",
            "",
            "5 layers*40 neurons",
            "0.0007 0.0006",
            "Case C",
            "[17]x",
            "2.4Mio",
            "1,2",
        ] {
            assert_eq!(number(t), None, "{t}");
        }
    }

    #[test]
    fn the_extreme_is_found_and_named() {
        let a = models().answer(&plan(Op::Smallest, &[1], &[])).unwrap();
        assert_eq!(
            a.text,
            "Smallest of Error (K) over 4 rows: 0.31 K, for GRU and MLP."
        );
        assert_eq!(
            a.cells
                .iter()
                .filter(|c| c.answer)
                .map(|c| (c.row, c.col))
                .collect::<Vec<_>>(),
            [(3, 1), (4, 1)]
        );
        let a = models().answer(&plan(Op::Largest, &[2], &[])).unwrap();
        assert_eq!(
            a.text, "Largest of Time [s] over 3 rows: 20 s, for MLP.",
            "n/a is left out"
        );
        assert_eq!((a.value, a.unit.as_deref()), (Some(20.0), Some("s")));
    }

    #[test]
    fn sums_keep_the_printed_precision() {
        let g = models();
        assert_eq!(
            g.answer(&plan(Op::Sum, &[1], &[])).unwrap().text,
            "Sum of Error (K) over 4 rows: 1.91 K."
        );
        assert_eq!(
            g.answer(&plan(Op::Average, &[1], &[0, 1])).unwrap().text,
            "Average of Error (K) over 2 rows: 0.645 K."
        );
    }

    #[test]
    fn two_cells_are_compared_in_the_order_named() {
        let g = models();
        assert_eq!(
            g.answer(&plan(Op::Difference, &[1], &[1, 0])).unwrap().text,
            "LSTM, Error (K) less PINN, Error (K): 0.87 K − 0.42 K = 0.45 K."
        );
        assert_eq!(
            g.answer(&plan(Op::Ratio, &[1], &[1, 0])).unwrap().text,
            "LSTM, Error (K) over PINN, Error (K): 0.87 K / 0.42 K = 2.07."
        );
        assert_eq!(
            g.answer(&plan(Op::Change, &[1], &[1, 0])).unwrap().text,
            "From LSTM, Error (K) to PINN, Error (K): 0.87 K → 0.42 K, −51.7 %."
        );
        assert_eq!(
            g.answer(&plan(Op::Difference, &[1], &[0, 1, 2])),
            Err(Refusal::NotTwo)
        );
    }

    #[test]
    fn a_question_that_names_its_cell_gets_that_cell() {
        let g = grid(
            &[
                "|MLP|PhysReg MLP|PhysNet",
                "Tr|0.209|0.197|0.226",
                "Tm|1.413|0.385|0.436",
            ],
            1,
        );
        let text = |cols: &[usize], rows: &[usize], q: &str| {
            let p = g.narrowed(plan(Op::Value, cols, rows), q);
            g.answer(&p).unwrap().text
        };
        assert_eq!(
            text(&[0, 1, 2, 3], &[0, 1], "Value of Tr for PhysNet?"),
            "Tr, PhysNet: 0.226."
        );
        assert_eq!(
            text(&[], &[1], "Tm for the PhysReg MLP"),
            "Tm, PhysReg MLP: 0.385.",
            "not the shorter name inside it"
        );
        assert_eq!(
            text(&[0, 3], &[0], "the error?"),
            "Tr, PhysNet: 0.226.",
            "a row's name is not repeated"
        );
        let p = plan(Op::Largest, &[], &[]);
        assert_eq!(g.narrowed(p.clone(), "MLP"), p, "only what is read");
    }

    #[test]
    fn what_is_told_about_a_table_is_worked_out_not_copied() {
        let said = models().outline();
        assert!(said.contains("Columns: Error (K) | Time [s]"), "{said}");
        assert!(said.contains("Rows: PINN | LSTM"), "{said}");
        assert!(said.contains("largest Error (K): 0.87"), "{said}");
        assert!(!said.contains("0.42 |"), "no raw cells: {said}");
        let side = grid(
            &[
                "|MLP|PhysReg|PhysNet",
                "Tr|0.209|0.197|0.226",
                "Tm|1.4|0.38|0.43",
            ],
            1,
        )
        .outline();
        assert!(
            side.contains("Smallest Tr: 0.197 (PhysReg); largest Tr: 0.226 (PhysNet)."),
            "{side}"
        );
        let pairs = grid(
            &[
                "Parameter|Value",
                "Optimizer|Adam",
                "Batch|2048",
                "Layers|2",
            ],
            1,
        )
        .outline();
        assert!(
            pairs.ends_with("Optimizer: Adam\nBatch: 2048\nLayers: 2"),
            "{pairs}"
        );
    }

    #[test]
    fn conditions_choose_rows() {
        let p = Plan {
            op: Op::Count,
            columns: vec![],
            rows: vec![],
            conditions: vec![Where {
                column: 1,
                test: Test::Below,
                value: "0.5".into(),
            }],
        };
        let a = models().answer(&p).unwrap();
        assert_eq!(a.text, "3 rows where Error (K) is below 0.5.");
        assert_eq!(a.value, Some(3.0));
        let p = Plan {
            op: Op::Value,
            columns: vec![2],
            conditions: vec![Where {
                column: 0,
                test: Test::Equals,
                value: "lstm".into(),
            }],
            rows: vec![],
        };
        assert_eq!(models().answer(&p).unwrap().text, "LSTM, Time [s]: 9.5 s.");
    }

    #[test]
    fn what_cannot_be_computed_is_refused() {
        let g = models();
        assert_eq!(
            g.answer(&plan(Op::None, &[], &[])),
            Err(Refusal::NotInTable)
        );
        assert_eq!(
            g.answer(&plan(Op::Sum, &[3], &[])),
            Err(Refusal::NotNumbers)
        );
        assert_eq!(
            g.answer(&plan(Op::Sum, &[1, 2], &[])),
            Err(Refusal::DifferentUnits)
        );
        assert_eq!(
            g.answer(&plan(Op::Value, &[1], &[99])),
            Err(Refusal::NotInTable)
        );
    }

    #[test]
    fn a_table_on_its_side_works_too() {
        let g = grid(
            &[
                "Layer|1|2|3",
                "L2|31.54%|21.98%|6.05%",
                "Time|1528.5s|4885.1s|8192.6s",
            ],
            1,
        );
        let a = g.answer(&plan(Op::Smallest, &[], &[0])).unwrap();
        assert_eq!(a.text, "Smallest of L2 over 3 columns: 6.05%, for 3.");
    }

    #[test]
    fn headings_over_columns_and_repeated_names() {
        let g = grid(
            &[
                "Model||Burgers|Advection",
                "|nu = 0.1|nu = 0.01|beta = 0.1",
                "U-Net|0.15|0.22|0.09",
                "|0.06|0.17|0.02",
            ],
            2,
        );
        assert_eq!(
            g.column_names(),
            [
                "Model",
                "Burgers nu = 0.1",
                "Burgers nu = 0.01",
                "Advection beta = 0.1"
            ]
        );
        assert_eq!(g.row_names(), ["U-Net", "U-Net"]);
        assert!(g.fits());
        let wide = Grid {
            cells: vec![vec![String::new(); 30]],
            ..Grid::default()
        };
        assert!(!wide.fits());
    }
}
