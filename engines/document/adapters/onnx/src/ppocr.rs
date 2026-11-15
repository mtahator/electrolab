//! PP-OCRv5 (Apache-2.0) on ONNX Runtime: one open-source OCR on every system (ADR-013).
//!
//! Detection (DB): the page at most 1280 px on its long side → text probability map → threshold
//! → connected regions → boxes grown by the DB "unclip" rule. Recognition (CTC): each line cropped,
//! 48 px high, lines of similar width batched together, greedy decoding with the model's
//! character list (which includes the math symbols of scientific text: ±, ∓, ≤, ∠, ·, …).
//! Pre- and post-processing follow spike S5 (`spikes/s5-page-models/ocr.py`), measured at
//! 93–95 % word agreement on scientific text pages.

use std::path::Path;

use ep_document_core::intel::ocr::{OcrEngine, OcrLine, OcrWord};
use ep_document_core::pdf::Bitmap;
use ort::session::Session;
use ort::value::Tensor;
use parking_lot::Mutex;

use crate::{Runtime, err};

const MAX_SIDE: usize = 1280;
const THRESH: f32 = 0.3;
const BOX_THRESH: f32 = 0.6;
const UNCLIP: f32 = 1.6;
const REC_H: usize = 48;
const BATCH: usize = 8;

/// The PP-OCR detector and recogniser.
pub struct PpOcr {
    det: Mutex<Session>,
    rec: Mutex<Session>,
    chars: Vec<String>,
}

impl std::fmt::Debug for PpOcr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PpOcr")
            .field("chars", &self.chars.len())
            .finish_non_exhaustive()
    }
}

/// The character list from a PP-OCR `inference.yml` (`PostProcess.character_dict`), with the CTC
/// blank first and a space last, as the model expects.
#[must_use]
pub fn charset(yml: &str) -> Vec<String> {
    let mut out = vec![String::new()];
    let mut inside = false;
    for line in yml.lines() {
        let t = line.trim_end();
        if t.trim_start().starts_with("character_dict:") {
            inside = true;
            continue;
        }
        if !inside {
            continue;
        }
        let Some(item) = t.trim_start().strip_prefix("- ") else {
            if t.trim_start().starts_with('-') {
                continue;
            }
            break;
        };
        // YAML scalars: 'quoted' (with '' for a quote) or plain.
        let v = if let Some(q) = item.strip_prefix('\'').and_then(|s| s.strip_suffix('\'')) {
            q.replace("''", "'")
        } else if let Some(q) = item.strip_prefix('"').and_then(|s| s.strip_suffix('"')) {
            q.replace("\\\\", "\\").replace("\\\"", "\"")
        } else {
            item.to_string()
        };
        out.push(v);
    }
    out.push(" ".into());
    out
}

/// An RGB image, row-major, 3 bytes per pixel.
#[derive(Clone)]
struct Rgb {
    w: usize,
    h: usize,
    px: Vec<u8>,
}

impl Rgb {
    fn from_bitmap(b: &Bitmap) -> Self {
        let (w, h) = (b.width as usize, b.height as usize);
        let mut px = Vec::with_capacity(w * h * 3);
        for p in b.rgba.as_chunks::<4>().0 {
            px.extend_from_slice(&p[..3]);
        }
        Self { w, h, px }
    }

    /// Bilinear resize to `nw` × `nh`, then `f(channel value 0–1, channel)` into CHW floats.
    #[allow(
        clippy::cast_precision_loss,
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss
    )]
    fn to_chw(
        &self,
        nw: usize,
        nh: usize,
        f: impl Fn(f32, usize) -> f32,
        out: &mut [f32],
        stride_w: usize,
    ) {
        let sx = self.w as f32 / nw as f32;
        let sy = self.h as f32 / nh as f32;
        for oy in 0..nh {
            let fy = ((oy as f32 + 0.5) * sy - 0.5).clamp(0.0, (self.h - 1) as f32);
            let (y0, ty) = (fy as usize, fy.fract());
            let y1 = (y0 + 1).min(self.h - 1);
            for ox in 0..nw {
                let fx = ((ox as f32 + 0.5) * sx - 0.5).clamp(0.0, (self.w - 1) as f32);
                let (x0, tx) = (fx as usize, fx.fract());
                let x1 = (x0 + 1).min(self.w - 1);
                for c in 0..3 {
                    let p = |x: usize, y: usize| f32::from(self.px[(y * self.w + x) * 3 + c]);
                    let v = (p(x0, y0) * (1.0 - tx) + p(x1, y0) * tx) * (1.0 - ty)
                        + (p(x0, y1) * (1.0 - tx) + p(x1, y1) * tx) * ty;
                    out[c * nh * stride_w + oy * stride_w + ox] = f(v / 255.0, c);
                }
            }
        }
    }

    fn crop(&self, x0: usize, y0: usize, x1: usize, y1: usize) -> Self {
        let (x1, y1) = (x1.min(self.w), y1.min(self.h));
        let (w, h) = (x1.saturating_sub(x0).max(1), y1.saturating_sub(y0).max(1));
        let mut px = Vec::with_capacity(w * h * 3);
        for y in y0..y0 + h {
            let row = (y.min(self.h - 1) * self.w + x0) * 3;
            px.extend_from_slice(&self.px[row..(row + w * 3).min(self.px.len())]);
        }
        px.resize(w * h * 3, 255);
        Self { w, h, px }
    }

    /// Rotated 90° counter-clockwise (vertical text such as axis labels).
    fn rot90(&self) -> Self {
        let (w, h) = (self.h, self.w);
        let mut px = vec![0u8; w * h * 3];
        for y in 0..self.h {
            for x in 0..self.w {
                let (nx, ny) = (y, self.w - 1 - x);
                px[(ny * w + nx) * 3..(ny * w + nx) * 3 + 3]
                    .copy_from_slice(&self.px[(y * self.w + x) * 3..(y * self.w + x) * 3 + 3]);
            }
        }
        Self { w, h, px }
    }
}

/// Text boxes from a probability map (`w` × `h`): connected regions above the threshold whose
/// mean probability is high enough, grown by the unclip rule. Boxes in map pixels.
#[must_use]
#[allow(clippy::cast_precision_loss, clippy::many_single_char_names)] // pixel coordinates
pub fn boxes(prob: &[f32], w: usize, h: usize) -> Vec<[f32; 4]> {
    let mut seen = vec![false; w * h];
    let mut out = Vec::new();
    let mut stack = Vec::new();
    for start in 0..w * h {
        if seen[start] || prob[start] <= THRESH {
            continue;
        }
        let (mut x0, mut y0, mut x1, mut y1) = (usize::MAX, usize::MAX, 0, 0);
        let (mut sum, mut n) = (0.0f32, 0usize);
        seen[start] = true;
        stack.push(start);
        while let Some(i) = stack.pop() {
            let (x, y) = (i % w, i / w);
            x0 = x0.min(x);
            y0 = y0.min(y);
            x1 = x1.max(x);
            y1 = y1.max(y);
            sum += prob[i];
            n += 1;
            let mut visit = |j: usize| {
                if !seen[j] && prob[j] > THRESH {
                    seen[j] = true;
                    stack.push(j);
                }
            };
            if x > 0 {
                visit(i - 1);
            }
            if x + 1 < w {
                visit(i + 1);
            }
            if y > 0 {
                visit(i - w);
            }
            if y + 1 < h {
                visit(i + w);
            }
        }
        if n < 6 || sum / (n as f32) < BOX_THRESH {
            continue;
        }
        let (bw, bh) = ((x1 - x0 + 1) as f32, (y1 - y0 + 1) as f32);
        let d = bw * bh * UNCLIP / (2.0 * (bw + bh)).max(1e-6);
        out.push([
            x0 as f32 - d,
            y0 as f32 - d,
            x1 as f32 + 1.0 + d,
            y1 as f32 + 1.0 + d,
        ]);
    }
    out
}

/// Greedy CTC decoding of one line: best class per step, repeats and blanks removed.
#[must_use]
pub fn ctc(probs: &[f32], steps: usize, classes: usize, chars: &[String]) -> String {
    let mut out = String::new();
    let mut prev = 0usize;
    for t in 0..steps {
        let row = &probs[t * classes..(t + 1) * classes];
        let best = row
            .iter()
            .enumerate()
            .fold(
                (0, f32::NEG_INFINITY),
                |b, (i, &v)| if v > b.1 { (i, v) } else { b },
            )
            .0;
        if best != prev
            && best != 0
            && let Some(c) = chars.get(best)
        {
            out.push_str(c);
        }
        prev = best;
    }
    out
}

impl PpOcr {
    /// Load the detector, recogniser and character list.
    ///
    /// # Errors
    /// A model file cannot be loaded or read.
    pub fn load(rt: &Runtime, det: &Path, rec: &Path, dict: &Path) -> Result<Self, String> {
        let chars = charset(&std::fs::read_to_string(dict).map_err(err)?);
        Ok(Self {
            det: Mutex::new(rt.session(det)?),
            rec: Mutex::new(rt.session(rec)?),
            chars,
        })
    }

    #[allow(
        clippy::cast_precision_loss,
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss
    )]
    fn detect(&self, img: &Rgb) -> Result<Vec<[f32; 4]>, String> {
        let s = (MAX_SIDE as f32 / img.w.max(img.h) as f32).min(1.0);
        let round32 = |v: f32| (((v / 32.0).round() as usize) * 32).max(32);
        let (nw, nh) = (round32(img.w as f32 * s), round32(img.h as f32 * s));
        let mut x = vec![0.0f32; 3 * nw * nh];
        let mean = [0.485, 0.456, 0.406];
        let std = [0.229, 0.224, 0.225];
        img.to_chw(nw, nh, |v, c| (v - mean[c]) / std[c], &mut x, nw);
        let input = Tensor::from_array(([1usize, 3, nh, nw], x)).map_err(err)?;
        let mut session = self.det.lock();
        let outputs = session.run(ort::inputs!["x" => input]).map_err(err)?;
        let (_, prob) = outputs[0].try_extract_tensor::<f32>().map_err(err)?;
        let (fx, fy) = (img.w as f32 / nw as f32, img.h as f32 / nh as f32);
        let mut bs: Vec<[f32; 4]> = boxes(prob, nw, nh)
            .into_iter()
            .map(|b| {
                [
                    (b[0] * fx).max(0.0),
                    (b[1] * fy).max(0.0),
                    (b[2] * fx).min(img.w as f32),
                    (b[3] * fy).min(img.h as f32),
                ]
            })
            .collect();
        // Reading order: rows top to bottom, then left to right.
        bs.sort_by(|a, b| {
            let ra = (f32::midpoint(a[1], a[3]) / 12.0).round();
            let rb = (f32::midpoint(b[1], b[3]) / 12.0).round();
            ra.total_cmp(&rb).then(a[0].total_cmp(&b[0]))
        });
        Ok(bs)
    }

    #[allow(
        clippy::cast_precision_loss,
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss
    )]
    fn recognise(&self, crops: &[Rgb]) -> Result<Vec<String>, String> {
        let mut order: Vec<usize> = (0..crops.len()).collect();
        // Lines of similar width together: padding to the widest line costs most.
        order.sort_by(|&a, &b| {
            (crops[a].w as f32 / crops[a].h as f32)
                .total_cmp(&(crops[b].w as f32 / crops[b].h as f32))
        });
        let mut texts = vec![String::new(); crops.len()];
        let mut session = self.rec.lock();
        for ids in order.chunks(BATCH) {
            let widths: Vec<usize> = ids
                .iter()
                .map(|&k| ((REC_H as f32 * crops[k].w as f32 / crops[k].h as f32) as usize).max(16))
                .collect();
            let wmax = widths.iter().copied().max().unwrap_or(16).min(3200);
            let mut x = vec![0.0f32; ids.len() * 3 * REC_H * wmax];
            for (k, (&id, &cw)) in ids.iter().zip(&widths).enumerate() {
                let slot = &mut x[k * 3 * REC_H * wmax..(k + 1) * 3 * REC_H * wmax];
                crops[id].to_chw(cw.min(wmax), REC_H, |v, _| (v - 0.5) / 0.5, slot, wmax);
            }
            let input = Tensor::from_array(([ids.len(), 3, REC_H, wmax], x)).map_err(err)?;
            let outputs = session.run(ort::inputs!["x" => input]).map_err(err)?;
            let (shape, probs) = outputs[0].try_extract_tensor::<f32>().map_err(err)?;
            let steps = usize::try_from(shape[1]).unwrap_or(0);
            let classes = usize::try_from(shape[2]).unwrap_or(1);
            for (k, &id) in ids.iter().enumerate() {
                texts[id] = ctc(
                    &probs[k * steps * classes..(k + 1) * steps * classes],
                    steps,
                    classes,
                    &self.chars,
                );
            }
        }
        Ok(texts)
    }
}

impl OcrEngine for PpOcr {
    fn id(&self) -> &'static str {
        "ppocr"
    }

    #[allow(
        clippy::cast_precision_loss,
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss
    )]
    fn recognize(&self, page: &Bitmap) -> Result<Vec<OcrLine>, String> {
        let img = Rgb::from_bitmap(page);
        if img.w == 0 || img.h == 0 {
            return Ok(Vec::new());
        }
        let bs = self.detect(&img)?;
        let crops: Vec<Rgb> = bs
            .iter()
            .map(|b| {
                let c = img.crop(
                    b[0] as usize,
                    b[1] as usize,
                    b[2].ceil() as usize,
                    b[3].ceil() as usize,
                );
                if c.h as f32 > 1.5 * c.w as f32 {
                    c.rot90()
                } else {
                    c
                }
            })
            .collect();
        let texts = self.recognise(&crops)?;
        let (fw, fh) = (img.w as f32, img.h as f32);
        // Each detected box is a line; its words share the box by character count.
        Ok(bs
            .iter()
            .zip(texts)
            .filter(|(_, t)| !t.trim().is_empty())
            .map(|(b, t)| {
                let total = t.chars().count().max(1) as f32;
                let mut x = b[0];
                let step = (b[2] - b[0]) / total;
                let mut words = Vec::new();
                for (i, part) in t.split(' ').enumerate() {
                    let n = part.chars().count() as f32;
                    if i > 0 {
                        x += step;
                    }
                    if !part.is_empty() {
                        words.push(OcrWord {
                            text: part.to_string(),
                            rect: [x / fw, b[1] / fh, (x + n * step) / fw, b[3] / fh],
                        });
                    }
                    x += n * step;
                }
                OcrLine { words }
            })
            .collect())
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn charset_reads_quoted_and_plain_items() {
        let yml = "PostProcess:\n  name: CTCLabelDecode\n  character_dict:\n  - '0'\n  - A\n  - ''''\n  - ±\nOther: 1\n";
        let c = charset(yml);
        assert_eq!(c, vec!["", "0", "A", "'", "±", " "]);
    }

    #[test]
    fn regions_and_unclip() {
        let (w, h) = (20, 10);
        let mut prob = vec![0.0f32; w * h];
        for y in 3..6 {
            for x in 2..12 {
                prob[y * w + x] = 0.9;
            }
        }
        prob[9 * w + 19] = 0.95; // a lone pixel is dropped
        let b = boxes(&prob, w, h);
        assert_eq!(b.len(), 1);
        assert!(b[0][0] < 2.0 && b[0][2] > 12.0 && b[0][1] < 3.0 && b[0][3] > 6.0);
    }

    #[test]
    fn ctc_decoding() {
        let chars: Vec<String> = ["", "a", "b", " "]
            .iter()
            .map(|s| (*s).to_string())
            .collect();
        // a a _ b b _ a
        let seq = [1, 1, 0, 2, 2, 0, 1];
        let mut probs = vec![0.0f32; seq.len() * 4];
        for (t, &c) in seq.iter().enumerate() {
            probs[t * 4 + c] = 1.0;
        }
        assert_eq!(ctc(&probs, seq.len(), 4, &chars), "aba");
    }
}
