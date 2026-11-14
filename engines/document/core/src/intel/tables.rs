//! Tables from the words inside a table box (P1.M4.TAB.1–2): rows by vertical position, columns
//! by the gaps that run through every row, header rows, units and numeric values, CSV.

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
    // Columns: gaps in the union of word extents across all rows.
    let mut spans: Vec<(f32, f32)> = ws.iter().map(|w| (w.rect.x0, w.rect.x1)).collect();
    spans.sort_by(|a, b| a.0.total_cmp(&b.0));
    // Column gaps are wider than the space between words (about a quarter of the line height).
    let gap = (h * 0.8).max(4.0);
    let mut cols: Vec<(f32, f32)> = Vec::new();
    for (a, b) in spans {
        match cols.last_mut() {
            Some(c) if a <= c.1 + gap => c.1 = c.1.max(b),
            _ => cols.push((a, b)),
        }
    }
    if cols.is_empty() {
        cols.push((object.rect.x0, object.rect.x1));
    }
    let col_of = |w: &Word| {
        let cx = f32::midpoint(w.rect.x0, w.rect.x1);
        cols.iter()
            .position(|c| cx >= c.0 - gap && cx <= c.1 + gap)
            .unwrap_or(0)
    };
    let mut grid: Vec<Vec<Cell>> = Vec::new();
    for mut r in rows {
        r.sort_by(|a, b| a.rect.x0.total_cmp(&b.rect.x0));
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
        for w in &r {
            let cell = &mut cells[col_of(w)];
            if !cell.text.is_empty() {
                cell.text.push(' ');
            }
            cell.text.push_str(w.text.trim());
            cell.rect.y0 = cell.rect.y0.min(w.rect.y0);
            cell.rect.y1 = cell.rect.y1.max(w.rect.y1);
        }
        let (y0, y1) = r.iter().fold((f32::MAX, f32::MIN), |(a, b), w| {
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
    let header_rows = if grid.iter().any(&numeric) {
        u32::try_from(grid.iter().take_while(|r| !numeric(r)).count().min(3)).unwrap_or(0)
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
        let obj = DocObject {
            id: "p4-table-1".into(),
            kind: ObjectKind::Table,
            rect: PageRect {
                page: 4,
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
        };
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
