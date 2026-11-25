//! Tables from the words inside a table box (P1.M4.TAB.1–2): rows by vertical position, columns
//! by the gaps that run through (almost) every row, header rows, units and numeric values, CSV.
//!
//! A heading over several columns, or one long entry, crosses a gap that the other rows have; so a
//! few rows may cross it. Words of one entry stand close together; two numbers next to each other
//! are two entries even when they are close.

use std::sync::LazyLock;

use ep_document_contract::{Cell, DocObject, PageText, Table};
use ep_types::PageRect;
use regex::Regex;

use super::median;

/// A word with its box.
#[derive(Clone, Debug)]
struct Word {
    text: String,
    rect: PageRect,
}

fn words_in(pt: &PageText, r: &PageRect) -> Vec<Word> {
    let chars: Vec<char> = pt.text.chars().collect();
    pt.words
        .iter()
        .filter(|w| {
            let (cx, cy) = (
                f32::midpoint(w.rect.x0, w.rect.x1),
                f32::midpoint(w.rect.y0, w.rect.y1),
            );
            cx >= r.x0 && cx <= r.x1 && cy >= r.y0 && cy <= r.y1
        })
        .map(|w| Word {
            text: chars[w.start as usize..(w.end as usize).min(chars.len())]
                .iter()
                .collect::<String>()
                .replace('\u{ad}', ""),
            rect: w.rect,
        })
        .filter(|w| !w.text.trim().is_empty())
        .collect()
}

/// The number in a cell: "1.2e-3", "45 %", "−0.7 ± 0.1" → −0.7, "1,234.5" → 1234.5.
#[must_use]
pub fn value(text: &str) -> Option<f64> {
    static NUM: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"^[(\[]?([-+]?\d+(?:\.\d+)?(?:[eE][-+]?\d+)?)\s*%?[)\]]?$")
            .unwrap_or_else(|_| unreachable!())
    });
    let t = text
        .trim()
        .replace(['\u{2212}', '\u{2013}'], "-")
        .replace(',', "")
        .split(['±', '\u{00b1}'])
        .next()
        .unwrap_or("")
        .trim()
        .trim_end_matches(['*', '†', '‡'])
        .to_string();
    NUM.captures(&t).and_then(|c| c[1].parse().ok())
}

fn unit(header: &str) -> Option<String> {
    static U: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"[(\[]([^()\[\]]{1,12})[)\]]\s*$").unwrap_or_else(|_| unreachable!())
    });
    U.captures(header.trim())
        .map(|c| c[1].trim().to_string())
        .filter(|u| !u.chars().all(char::is_numeric))
}

fn csv_field(s: &str) -> String {
    if s.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

/// Words of a row that belong together: first and last word.
fn phrases(row: &[Word], h: f32) -> Vec<(usize, usize)> {
    let mut out: Vec<(usize, usize)> = Vec::new();
    for (i, w) in row.iter().enumerate() {
        if let Some(p) = out.last_mut() {
            let last = &row[p.1];
            let both = value(&last.text).is_some() && value(&w.text).is_some();
            // Between two numbers even a narrow gap is one between columns.
            let near = if both { h * 0.18 } else { h * 0.72 };
            if w.rect.x0 - last.rect.x1 < near {
                p.1 = i;
                continue;
            }
        }
        out.push((i, i));
    }
    out
}

/// The middle of every gap between columns, from left to right.
fn separators(rows: &[Vec<Word>], h: f32) -> Vec<(f32, f32)> {
    type Row<'a> = (&'a Vec<Word>, Vec<(usize, usize)>);
    let many: Vec<Row> = rows
        .iter()
        .map(|r| (r, phrases(r, h)))
        .filter(|(_, p)| p.len() > 1)
        .collect();
    let crossing = match many.len() {
        0..=3 => 0,
        4..=6 => 1,
        n => n * 15 / 100,
    };
    let span = |r: &[Word], p: (usize, usize)| (r[p.0].rect.x0, r[p.1].rect.x1);
    let mut xs: Vec<f32> = many
        .iter()
        .flat_map(|(r, p)| {
            p.iter().flat_map(|p| {
                let (a, b) = span(r, *p);
                [a, b]
            })
        })
        .collect();
    xs.sort_by(f32::total_cmp);
    xs.dedup();
    let mut free: Vec<(f32, f32)> = Vec::new();
    let mut open = false;
    for w in xs.windows(2) {
        let m = f32::midpoint(w[0], w[1]);
        let covered = many
            .iter()
            .filter(|(r, p)| {
                p.iter().any(|p| {
                    let (a, b) = span(r, *p);
                    a < m && m < b
                })
            })
            .count();
        if covered <= crossing {
            match free.last_mut() {
                Some(f) if open => f.1 = w[1],
                _ => free.push((w[0], w[1])),
            }
        }
        open = covered <= crossing;
    }
    free.into_iter()
        .filter(|(a, b)| {
            let m = f32::midpoint(*a, *b);
            let (mut gaps, mut numbers) = (Vec::new(), 0);
            for (r, p) in &many {
                let left = p.iter().rfind(|p| r[p.1].rect.x1 <= m);
                let right = p.iter().find(|p| r[p.0].rect.x0 >= m);
                let crossed = p.iter().any(|p| {
                    let (a, b) = span(r, *p);
                    a < m && m < b
                });
                let (Some(l), Some(q), false) = (left, right, crossed) else {
                    continue;
                };
                gaps.push(r[q.0].rect.x0 - r[l.1].rect.x1);
                numbers +=
                    usize::from(value(&r[l.1].text).is_some() && value(&r[q.0].text).is_some());
            }
            if gaps.is_empty() || (many.len() > 2 && gaps.len() < 2) {
                return false;
            }
            let wide = if numbers * 2 >= gaps.len() {
                h * 0.18
            } else {
                h * 0.6
            };
            b - a >= h * 0.18 && median(gaps) >= wide
        })
        .collect()
}

/// Words with their column, and how many columns there are. Neighbouring columns that are never
/// filled in the same row are one (entries of one column that stand apart).
fn join_columns(
    rows: Vec<Vec<Word>>,
    cuts: &[(f32, f32)],
    h: f32,
) -> (Vec<Vec<(usize, Word)>>, usize) {
    let column = |x: f32| {
        cuts.iter()
            .filter(|(a, b)| x > f32::midpoint(*a, *b))
            .count()
    };
    let mut out: Vec<Vec<(usize, Word)>> = rows
        .into_iter()
        .map(|r| {
            let mut at = vec![0; r.len()];
            for (first, last) in phrases(&r, h) {
                let gaps: Vec<f32> = (first..last)
                    .map(|i| r[i + 1].rect.x0 - r[i].rect.x1)
                    .collect();
                let usual = median(gaps.clone());
                let mut start = first;
                // A phrase over a gap stays whole (a heading of several columns), unless it is
                // itself wider apart there.
                for i in first..last {
                    let (a, b) = (r[i].rect.x1, r[i + 1].rect.x0);
                    let here = cuts.iter().any(|(x0, x1)| a < *x1 && b > *x0);
                    if here && gaps.len() > 1 && b - a > usual * 1.3 {
                        at[start..=i].fill(column(begin(&r, start, i, h)));
                        start = i + 1;
                    }
                }
                at[start..=last].fill(column(begin(&r, start, last, h)));
            }
            at.into_iter().zip(r).collect()
        })
        .collect();
    let mut n = cuts.len() + 1;
    let mut c = 0;
    while c + 1 < n {
        let filled = |row: &[(usize, Word)], c: usize| row.iter().any(|(at, _)| *at == c);
        let both = out.iter().any(|r| filled(r, c) && filled(r, c + 1));
        let first = out.iter().filter(|r| filled(r, c)).count();
        let second = out.iter().filter(|r| filled(r, c + 1)).count();
        if both && first > 0 && second > 0 {
            c += 1;
            continue;
        }
        for (at, _) in out.iter_mut().flatten() {
            if *at > c {
                *at -= 1;
            }
        }
        n -= 1;
    }
    (out, n)
}

/// Where a phrase stands: the middle of it, or of its beginning when it is long.
fn begin(r: &[Word], first: usize, last: usize, h: f32) -> f32 {
    let (a, b) = (r[first].rect.x0, r[last].rect.x1);
    f32::midpoint(a, b.min(a + h * 4.0))
}

/// A table from the words inside `object.rect`.
#[must_use]
#[allow(clippy::too_many_lines)] // rows, columns, cells, headers, units, CSV in one pass
pub fn table(pt: &PageText, object: &DocObject) -> Table {
    let ws = words_in(pt, &object.rect);
    // Rows: words whose vertical centres are close.
    let h = median(ws.iter().map(|w| w.rect.height()).collect()).max(4.0);
    let mut sorted = ws.clone();
    sorted.sort_by(|a, b| {
        f32::midpoint(a.rect.y0, a.rect.y1).total_cmp(&f32::midpoint(b.rect.y0, b.rect.y1))
    });
    let mut rows: Vec<Vec<Word>> = Vec::new();
    for w in sorted {
        let cy = f32::midpoint(w.rect.y0, w.rect.y1);
        match rows.last_mut() {
            Some(r)
                if (cy
                    - median(
                        r.iter()
                            .map(|x| f32::midpoint(x.rect.y0, x.rect.y1))
                            .collect(),
                    ))
                .abs()
                    < h * 0.55 =>
            {
                r.push(w);
            }
            _ => rows.push(vec![w]),
        }
    }
    for r in &mut rows {
        r.sort_by(|a, b| a.rect.x0.total_cmp(&b.rect.x0));
    }
    let cuts = separators(&rows, h);
    let (rows, n) = join_columns(rows, &cuts, h);
    let cols: Vec<(f32, f32)> = (0..n)
        .map(|c| {
            let own = rows.iter().flatten().filter(|(at, _)| *at == c);
            let (a, b) = own.fold((f32::MAX, f32::MIN), |(a, b), (_, w)| {
                (a.min(w.rect.x0), b.max(w.rect.x1))
            });
            if a <= b {
                (a, b)
            } else {
                (object.rect.x0, object.rect.x1)
            }
        })
        .collect();
    let mut grid: Vec<Vec<Cell>> = Vec::new();
    for r in rows {
        let mut cells: Vec<Cell> = cols
            .iter()
            .map(|c| Cell {
                text: String::new(),
                value: None,
                rect: PageRect {
                    page: object.rect.page,
                    x0: c.0,
                    x1: c.1,
                    y0: f32::MAX,
                    y1: f32::MIN,
                },
            })
            .collect();
        for (at, w) in &r {
            let cell = &mut cells[*at];
            if !cell.text.is_empty() {
                cell.text.push(' ');
            }
            cell.text.push_str(w.text.trim());
            cell.rect.y0 = cell.rect.y0.min(w.rect.y0);
            cell.rect.y1 = cell.rect.y1.max(w.rect.y1);
        }
        let (y0, y1) = r.iter().fold((f32::MAX, f32::MIN), |(a, b), (_, w)| {
            (a.min(w.rect.y0), b.max(w.rect.y1))
        });
        for c in &mut cells {
            if c.text.is_empty() {
                c.rect.y0 = y0;
                c.rect.y1 = y1;
            }
            c.value = value(&c.text);
        }
        grid.push(cells);
    }
    // Header rows: leading rows where most filled cells are not numbers, before numeric rows.
    let numeric = |row: &Vec<Cell>| {
        let filled: Vec<&Cell> = row.iter().filter(|c| !c.text.is_empty()).collect();
        !filled.is_empty()
            && filled.iter().filter(|c| c.value.is_some()).count() * 2 >= filled.len()
    };
    // A heading without gaps needs no second row, unless that row has no name of its own
    // (the rest of the heading) or holds units.
    let whole = grid
        .first()
        .is_some_and(|r| r.iter().all(|c| !c.text.is_empty()));
    let data = |row: &Vec<Cell>| {
        whole
            && row
                .first()
                .is_some_and(|c| !c.text.is_empty() && !c.text.starts_with(['(', '[']))
    };
    let header_rows = if grid.iter().any(&numeric) {
        let rows = grid
            .iter()
            .enumerate()
            .take_while(|(i, r)| !numeric(r) && (*i == 0 || !data(r)))
            .count();
        u32::try_from(rows.min(3)).unwrap_or(0)
    } else {
        u32::from(!grid.is_empty())
    };
    let units = (0..cols.len())
        .map(|ci| {
            grid.iter()
                .take(header_rows as usize)
                .find_map(|r| unit(&r[ci].text))
        })
        .collect();
    let csv = grid
        .iter()
        .map(|r| {
            r.iter()
                .map(|c| csv_field(&c.text))
                .collect::<Vec<_>>()
                .join(",")
        })
        .collect::<Vec<_>>()
        .join("\r\n");
    Table {
        object: object.clone(),
        page: object.rect.page,
        rows: grid,
        header_rows,
        units,
        csv,
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::float_cmp)]
mod tests {
    use super::super::testing::page;
    use super::*;
    use ep_document_contract::ObjectKind;

    #[test]
    fn values() {
        assert_eq!(value("1.2e-3"), Some(0.0012));
        assert_eq!(value("45 %"), Some(45.0));
        assert_eq!(value("\u{2212}0.7 ± 0.1"), Some(-0.7));
        assert_eq!(value("1,234.5"), Some(1234.5));
        assert_eq!(value("(12)"), Some(12.0));
        assert_eq!(value("PINN"), None);
    }

    fn read(lines: &[(&str, f32)]) -> Vec<Vec<String>> {
        let lines: Vec<(&str, f32, f32, f32)> =
            lines.iter().map(|(t, y)| (*t, 100.0, *y, 10.0)).collect();
        let p = page(1, &lines);
        let obj = DocObject {
            id: "p1-table-1".into(),
            kind: ObjectKind::Table,
            rect: PageRect {
                page: 1,
                x0: 90.0,
                y0: 95.0,
                x1: 600.0,
                y1: 400.0,
            },
            label: None,
            number: None,
            text: None,
            caption_rect: None,
            score: 1.0,
            order: 0,
        };
        table(&p, &obj)
            .rows
            .iter()
            .map(|r| r.iter().map(|c| c.text.clone()).collect())
            .collect()
    }

    #[test]
    fn a_heading_over_columns_does_not_join_them() {
        let t = read(&[
            ("Model      Burgers with two values      Advection", 100.0),
            ("           nu=0.1       nu=0.01        beta=0.1", 114.0),
            ("U-Net      0.1548       0.2222         0.0897", 128.0),
            ("DeepONet   0.0687       0.1758         0.0194", 142.0),
            ("FNO        0.0171       0.0492         0.0125", 156.0),
            ("ResNet     0.0383       0.0843         0.0413", 170.0),
            ("Other      0.0061       0.0180         0.0120", 184.0),
        ]);
        assert_eq!(t[2], ["U-Net", "0.1548", "0.2222", "0.0897"], "{t:?}");
        assert!(
            t[0].contains(&"Burgers with two values".to_string()),
            "{t:?}"
        );
    }

    #[test]
    fn entries_of_one_column_that_stand_apart_are_one_column() {
        let t = read(&[
            ("Parameter         Value    Description", 100.0),
            ("layers        2            Number of layers", 114.0),
            ("width         256          Size of a layer", 128.0),
            ("rate          0.0003       Learning rate", 142.0),
        ]);
        assert!(t.iter().all(|r| r.len() == 3), "{t:?}");
        assert_eq!(t[2], ["width", "256", "Size of a layer"]);
    }

    /// A table where the lines of a page stand.
    fn object(page: u32) -> DocObject {
        DocObject {
            id: format!("p{page}-table-1"),
            kind: ObjectKind::Table,
            rect: PageRect {
                page,
                x0: 90.0,
                y0: 95.0,
                x1: 400.0,
                y1: 145.0,
            },
            label: Some("Table 1".into()),
            number: Some("1".into()),
            text: None,
            caption_rect: None,
            score: 1.0,
            order: 0,
        }
    }

    #[test]
    fn a_row_with_words_below_a_whole_heading_is_not_a_heading() {
        let words = page(
            1,
            &[
                ("Parameter         Value", 100.0, 100.0, 10.0),
                ("Optimizer         Adam", 100.0, 114.0, 10.0),
                ("Learning rate     0.001", 100.0, 128.0, 10.0),
                ("Batch size        2048", 100.0, 142.0, 10.0),
            ],
        );
        let t = table(&words, &object(1));
        assert_eq!(t.header_rows, 1, "{}", t.csv);
    }

    #[test]
    fn grid_header_units_csv() {
        let p = page(
            4,
            &[
                ("Model      Error (K)     Time [s]", 100.0, 100.0, 10.0),
                ("PINN       0.42          12", 100.0, 114.0, 10.0),
                ("LSTM, v2   0.87          9.5", 100.0, 128.0, 10.0),
            ],
        );
        let obj = object(4);
        let t = table(&p, &obj);
        assert_eq!(t.rows.len(), 3);
        assert_eq!(
            t.rows[0].len(),
            3,
            "{:?}",
            t.rows[0].iter().map(|c| &c.text).collect::<Vec<_>>()
        );
        assert_eq!(t.header_rows, 1);
        assert_eq!(t.rows[1][1].value, Some(0.42));
        assert_eq!(t.units, vec![None, Some("K".into()), Some("s".into())]);
        assert!(t.csv.contains("\"LSTM, v2\",0.87,9.5"));
    }
}
