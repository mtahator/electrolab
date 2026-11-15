//! Equations as LaTeX (P1.M4.EQ.1, ADR-015): the formula-model port and everything around the
//! model that is pure — the grey crop, splitting a block into lines, cleaning and joining the
//! LaTeX. The model itself (LaTeX-OCR on ONNX Runtime) lives in the ONNX adapter.

use std::sync::LazyLock;

use regex::Regex;

use crate::pdf::Bitmap;

/// A grey image, one byte per pixel, rows top to bottom (0 black, 255 white).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Gray {
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// `width * height` bytes.
    pub data: Vec<u8>,
}

impl Gray {
    /// The part `x0..x1, y0..y1` of a drawn page, in grey.
    #[allow(clippy::many_single_char_names)] // corners and colour channels
    #[must_use]
    pub fn from_bitmap(b: &Bitmap, x0: u32, y0: u32, x1: u32, y1: u32) -> Self {
        let x1 = x1.min(b.width).max(x0.min(b.width));
        let y1 = y1.min(b.height).max(y0.min(b.height));
        let (x0, y0) = (x0.min(x1), y0.min(y1));
        let (w, h) = (x1 - x0, y1 - y0);
        let mut data = Vec::with_capacity((w * h) as usize);
        for y in y0..y1 {
            for x in x0..x1 {
                let i = ((y * b.width + x) * 4) as usize;
                let (r, g, bl) = (
                    u32::from(b.rgba[i]),
                    u32::from(b.rgba[i + 1]),
                    u32::from(b.rgba[i + 2]),
                );
                data.push(u8::try_from((r * 299 + g * 587 + bl * 114) / 1000).unwrap_or(255));
            }
        }
        Self {
            width: w,
            height: h,
            data,
        }
    }

    /// Rows `y0..y1`.
    #[must_use]
    pub fn rows(&self, y0: u32, y1: u32) -> Self {
        let (y0, y1) = (y0.min(self.height), y1.min(self.height));
        Self {
            width: self.width,
            height: y1.saturating_sub(y0),
            data: self.data[(y0 * self.width) as usize..(y1 * self.width) as usize].to_vec(),
        }
    }
}

/// Port: a formula recogniser (image of one formula line → LaTeX).
pub trait FormulaModel: Send + Sync {
    /// Short id, part of cache keys ("latex-ocr").
    fn id(&self) -> &'static str;
    /// LaTeX of the formula in `line` (one line; dark ink on white).
    ///
    /// # Errors
    /// The model failed.
    fn latex(&self, line: &Gray) -> Result<String, String>;
}

/// Rows with ink: a row is blank when no pixel is darker than `ink`.
fn ink_rows(g: &Gray, ink: u8) -> Vec<bool> {
    (0..g.height)
        .map(|y| {
            let row = &g.data[(y * g.width) as usize..((y + 1) * g.width) as usize];
            row.iter().any(|&p| p < ink)
        })
        .collect()
}

/// Runs of `true` in `v`, as `start..end`.
fn runs(v: &[bool]) -> Vec<(u32, u32)> {
    let mut out = Vec::new();
    let mut start: Option<u32> = None;
    for (i, &on) in v.iter().enumerate() {
        let i = u32::try_from(i).unwrap_or(u32::MAX);
        match (on, start) {
            (true, None) => start = Some(i),
            (false, Some(s)) => {
                out.push((s, i));
                start = None;
            }
            _ => {}
        }
    }
    if let Some(s) = start {
        out.push((s, u32::try_from(v.len()).unwrap_or(u32::MAX)));
    }
    out
}

/// The formula without a printed equation number at its right end: a narrow island of ink
/// ("(12)") set apart by a wide gap is cut off, together with the gap. `em_px` is the height of
/// a text line in pixels.
#[must_use]
pub fn drop_number(g: &Gray, em_px: u32) -> Gray {
    let cols: Vec<bool> = (0..g.width)
        .map(|x| (0..g.height).any(|y| g.data[(y * g.width + x) as usize] < 160))
        .collect();
    let r = runs(&cols);
    if r.len() < 2 {
        return g.clone();
    }
    // The last island: runs closer than half an em belong together ("(", "1", "2", ")").
    let mut start = r.len() - 1;
    while start > 0 && r[start].0 - r[start - 1].1 < em_px / 2 {
        start -= 1;
    }
    if start == 0 {
        return g.clone();
    }
    let island = (r[start].0, r[r.len() - 1].1);
    let gap = island.0 - r[start - 1].1;
    if island.1 - island.0 > em_px * 3 || gap < em_px * 6 / 5 {
        return g.clone();
    }
    let keep = r[start - 1].1;
    let mut data = Vec::with_capacity((keep * g.height) as usize);
    for y in 0..g.height {
        let row = (y * g.width) as usize;
        data.extend_from_slice(&g.data[row..row + keep as usize]);
    }
    Gray {
        width: keep,
        height: g.height,
        data,
    }
}

/// The lines of a displayed formula block, as row ranges. A block shorter than about two and a
/// half lines stays whole (fractions, sums with limits and matrices have gaps inside them); a
/// taller one is cut at blank gaps of at least half a line, and pieces thinner than a line (a
/// sum's limits, a fraction's part) join their nearest neighbour.
#[must_use]
pub fn split_lines(g: &Gray, line_height_px: u32) -> Vec<(u32, u32)> {
    let rows = ink_rows(g, 160);
    let ink = runs(&rows);
    let (Some(&(first, _)), Some(&(_, last))) = (ink.first(), ink.last()) else {
        return Vec::new();
    };
    if last - first < line_height_px * 5 / 2 {
        return vec![(first, last)];
    }
    let gap = (line_height_px / 2).max(2);
    let mut lines: Vec<(u32, u32)> = Vec::new();
    for run in ink {
        match lines.last_mut() {
            Some(l) if run.0 - l.1 < gap => l.1 = run.1,
            _ => lines.push(run),
        }
    }
    // Thin pieces join the nearer neighbour, until none is left.
    while lines.len() > 1 {
        let Some(thin) = lines
            .iter()
            .position(|l| l.1 - l.0 < line_height_px * 7 / 10)
        else {
            break;
        };
        let up = (thin > 0).then(|| lines[thin].0 - lines[thin - 1].1);
        let down = lines.get(thin + 1).map(|n| n.0 - lines[thin].1);
        let into = match (up, down) {
            (Some(above), Some(below)) if below < above => thin + 1,
            (None, _) => thin + 1,
            _ => thin - 1,
        };
        let (top, bottom) = (thin.min(into), thin.max(into));
        lines[top].1 = lines[bottom].1;
        lines.remove(bottom);
    }
    lines
}

static TEXT_CMD: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(\\(?:operatorname|mathrm|text|mathbf)\s?\*? \{.*?\})")
        .unwrap_or_else(|_| unreachable!())
});
static NL_NL: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"([\W_\d])\s+([\W_\d])").unwrap_or_else(|_| unreachable!()));
static NL_L: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"([\W_\d])\s+([a-zA-Z])").unwrap_or_else(|_| unreachable!()));
static L_NL: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"([a-zA-Z])\s+([\W_\d])").unwrap_or_else(|_| unreachable!()));

/// The model's LaTeX with the spaces LaTeX does not need removed (as the reference does): inside
/// `\mathrm{…}` and the like all spaces go; between a non-letter and anything, and a letter and a
/// non-letter, spaces go; "\ " (a forced space) stays.
#[must_use]
pub fn clean_latex(s: &str) -> String {
    let mut s = TEXT_CMD
        .replace_all(s, |c: &regex::Captures<'_>| c[1].replace(' ', ""))
        .into_owned();
    loop {
        let protect = s.replace("\\ ", "\u{1}");
        let next = NL_NL.replace_all(&protect, "$1$2");
        let next = NL_L.replace_all(&next, "$1$2");
        let next = L_NL.replace_all(&next, "$1$2").replace('\u{1}', "\\ ");
        if next == s {
            break;
        }
        s = next;
    }
    s.trim().to_string()
}

/// LaTeX of several lines of one displayed formula, as one block.
#[must_use]
pub fn join_lines(lines: &[String]) -> String {
    let lines: Vec<&str> = lines
        .iter()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty())
        .collect();
    match lines.len() {
        0 => String::new(),
        1 => lines[0].to_string(),
        _ => format!(
            "\\begin{{aligned}}\n{}\n\\end{{aligned}}",
            lines.join(" \\\\\n")
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn block(lines: &[(u32, u32)], height: u32) -> Gray {
        let width = 40;
        let mut data = vec![255u8; (width * height) as usize];
        for &(a, b) in lines {
            for y in a..b {
                for x in 5..30 {
                    data[(y * width + x) as usize] = 0;
                }
            }
        }
        Gray {
            width,
            height,
            data,
        }
    }

    #[test]
    fn one_line_stays_whole_and_blocks_split() {
        // A fraction: two runs with a small gap, about one line tall → whole.
        assert_eq!(
            split_lines(&block(&[(2, 10), (12, 20)], 24), 20),
            vec![(2, 20)]
        );
        // Three lines with clear gaps.
        let g = block(&[(2, 18), (30, 46), (58, 74)], 80);
        assert_eq!(split_lines(&g, 18), vec![(2, 18), (30, 46), (58, 74)]);
        assert_eq!(split_lines(&block(&[], 10), 10), Vec::<(u32, u32)>::new());
    }

    #[test]
    fn the_equation_number_is_cut_off() {
        // Formula ink at x 0..60, number "(1)" at x 120..135, line height 10.
        let (w, h) = (140u32, 12u32);
        let mut data = vec![255u8; (w * h) as usize];
        for y in 2..10 {
            for x in (0..60).chain(120..124).chain(126..130).chain(132..135) {
                data[(y * w + x) as usize] = 0;
            }
        }
        let g = Gray {
            width: w,
            height: h,
            data,
        };
        assert_eq!(drop_number(&g, 10).width, 60);
        // Nothing to cut when there is no number.
        let mut plain = vec![255u8; (70 * h) as usize];
        for y in 2..10u32 {
            for x in 0..60u32 {
                plain[(y * 70 + x) as usize] = 0;
            }
        }
        let g = Gray {
            width: 70,
            height: h,
            data: plain,
        };
        assert_eq!(drop_number(&g, 10).width, 70);
    }

    #[test]
    fn latex_is_cleaned_and_joined() {
        assert_eq!(
            clean_latex("x _ { i + 1 } = f ( x _ { i } )"),
            "x_{i+1}=f(x_{i})"
        );
        assert_eq!(clean_latex("\\mathrm { L o s s } = a"), "\\mathrm{Loss}=a");
        assert_eq!(clean_latex("a \\ b"), "a\\ b");
        assert_eq!(join_lines(&["a=b".into()]), "a=b");
        assert_eq!(
            join_lines(&["a=b".into(), "c=d".into()]),
            "\\begin{aligned}\na=b \\\\\nc=d\n\\end{aligned}"
        );
    }
}
