//! LaTeX-OCR (pix2tex) on ONNX Runtime (ADR-015): image of one formula line → LaTeX.
//!
//! The steps follow the reference implementation (`RapidLaTeXOCR` 0.0.9) so results match it:
//! 1. ink on white, cropped to the ink, padded to multiples of 32, fit within 672 × 192;
//! 2. the resizer model picks the width (steps of 32), up to 10 rounds;
//! 3. grey, normalised (mean 0.7931, std 0.1738), one channel → encoder;
//! 4. greedy decoding from BOS (1) to EOS (2), the whole sequence each step (the exported decoder
//!    has no cache), capped by tokens, time and repeats;
//! 5. tokens joined, `Ġ` → space, then the reference whitespace clean-up (core).

use std::path::Path;
use std::time::{Duration, Instant};

use ep_document_core::intel::formula::{FormulaModel, Gray, clean_latex};
use ort::session::Session;
use ort::value::Tensor;
use parking_lot::Mutex;

use crate::{Runtime, err};

const MAX_W: u32 = 672;
const MAX_H: u32 = 192;
const MIN_W: u32 = 32;
const MIN_H: u32 = 32;
const BOS: i64 = 1;
const EOS: i64 = 2;
const MAX_TOKENS: usize = 512;
/// A line that takes longer is cut off (spike S5: one wide crop looped for 18 s).
const MAX_TIME: Duration = Duration::from_secs(5);
/// The same token this many times in a row is a loop.
const MAX_REPEAT: usize = 12;

/// The loaded formula recogniser.
pub struct LatexOcr {
    resizer: Mutex<Session>,
    encoder: Mutex<Session>,
    decoder: Mutex<Session>,
    vocab: Vec<String>,
}

impl std::fmt::Debug for LatexOcr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LatexOcr")
            .field("vocab", &self.vocab.len())
            .finish_non_exhaustive()
    }
}

/// Token strings by id, from a Hugging Face `tokenizer.json` (BPE vocabulary).
fn vocab(json: &str) -> Result<Vec<String>, String> {
    let v: serde_json::Value = serde_json::from_str(json).map_err(err)?;
    let map = v
        .pointer("/model/vocab")
        .and_then(serde_json::Value::as_object)
        .ok_or("tokenizer.json has no vocabulary")?;
    let mut out = vec![String::new(); map.len()];
    for (tok, id) in map {
        let id = id
            .as_u64()
            .and_then(|i| usize::try_from(i).ok())
            .ok_or("bad token id")?;
        if let Some(slot) = out.get_mut(id) {
            slot.clone_from(tok);
        }
    }
    Ok(out)
}

/// Ink on white: the image stretched to 0–255, inverted when the ink is light, cropped to the
/// ink, padded (white) to multiples of 32. `None` when there is no ink.
fn pad(g: &Gray) -> Option<Gray> {
    let (lo, hi) = g
        .data
        .iter()
        .fold((255u8, 0u8), |(lo, hi), &p| (lo.min(p), hi.max(p)));
    if hi <= lo {
        return None;
    }
    let span = f32::from(hi - lo);
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // 0..=255
    let mut data: Vec<u8> = g
        .data
        .iter()
        .map(|&p| (f32::from(p - lo) / span * 255.0) as u8)
        .collect();
    #[allow(clippy::cast_precision_loss)]
    let mean = data.iter().map(|&p| f64::from(p)).sum::<f64>() / data.len().max(1) as f64;
    let ink: Vec<bool> = if mean > 128.0 {
        data.iter().map(|&p| p < 128).collect()
    } else {
        data.iter_mut()
            .map(|p| {
                let was = *p > 128;
                *p = 255 - *p;
                was
            })
            .collect()
    };
    let (w, h) = (g.width, g.height);
    let (mut x0, mut y0, mut x1, mut y1) = (w, h, 0, 0);
    for y in 0..h {
        for x in 0..w {
            if ink[(y * w + x) as usize] {
                x0 = x0.min(x);
                y0 = y0.min(y);
                x1 = x1.max(x + 1);
                y1 = y1.max(y + 1);
            }
        }
    }
    if x1 <= x0 || y1 <= y0 {
        return None;
    }
    let (cw, ch) = (x1 - x0, y1 - y0);
    let (pw, ph) = (cw.div_ceil(32) * 32, ch.div_ceil(32) * 32);
    let mut out = vec![255u8; (pw * ph) as usize];
    for y in 0..ch {
        let src = ((y0 + y) * w + x0) as usize;
        let dst = (y * pw) as usize;
        out[dst..dst + cw as usize].copy_from_slice(&data[src..src + cw as usize]);
    }
    Some(Gray {
        width: pw,
        height: ph,
        data: out,
    })
}

/// Resampling filters, as the reference's image library (Pillow) has them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Filter {
    Bilinear,
    Lanczos,
}

impl Filter {
    fn support(self) -> f64 {
        match self {
            Self::Bilinear => 1.0,
            Self::Lanczos => 3.0,
        }
    }

    fn weight(self, x: f64) -> f64 {
        let sinc = |x: f64| {
            if x == 0.0 {
                1.0
            } else {
                let px = x * std::f64::consts::PI;
                px.sin() / px
            }
        };
        match self {
            Self::Bilinear => (1.0 - x.abs()).max(0.0),
            Self::Lanczos if x.abs() < 3.0 => sinc(x) * sinc(x / 3.0),
            Self::Lanczos => 0.0,
        }
    }
}

/// Fixed-point precision of Pillow's 8-bit resampling.
const PRECISION_BITS: u32 = 32 - 8 - 2;

/// Pillow's coefficients for one axis: for each output pixel, the first input pixel and the
/// fixed-point weights (`precompute_coeffs` + `normalize_coeffs_8bpc`), so results match it.
#[allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)]
fn coeffs(in_size: u32, out_size: u32, f: Filter) -> Vec<(usize, Vec<i64>)> {
    let scale = f64::from(in_size) / f64::from(out_size);
    let filterscale = scale.max(1.0);
    let support = f.support() * filterscale;
    (0..out_size)
        .map(|xx| {
            let center = (f64::from(xx) + 0.5) * scale;
            let ss = 1.0 / filterscale;
            let xmin = ((center - support + 0.5) as i64).max(0);
            let xmax = ((center + support + 0.5) as i64).min(i64::from(in_size)) - xmin;
            let k: Vec<f64> = (0..xmax.max(0))
                .map(|x| f.weight(((x + xmin) as f64 - center + 0.5) * ss))
                .collect();
            let ww: f64 = k.iter().sum();
            let k = k
                .iter()
                .map(|&w| {
                    let w = if ww == 0.0 { w } else { w / ww };
                    let v = w * f64::from(1u32 << PRECISION_BITS);
                    (if v < 0.0 { v - 0.5 } else { v + 0.5 }) as i64
                })
                .collect();
            (xmin as usize, k)
        })
        .collect()
}

fn clip8(ss: i64) -> u8 {
    u8::try_from((ss >> PRECISION_BITS).clamp(0, 255)).unwrap_or(255)
}

/// Resize as Pillow does (`Image.resize` with BILINEAR or LANCZOS on an 8-bit grey image):
/// horizontal pass, then vertical, each rounded to 8 bits.
#[allow(clippy::many_single_char_names)] // image, size, filter and weights, as in Pillow
fn resize_with(g: &Gray, w: u32, h: u32, f: Filter) -> Gray {
    let (w, h) = (w.max(1), h.max(1));
    let mut cur = g.clone();
    if w != cur.width {
        let k = coeffs(cur.width, w, f);
        let mut data = Vec::with_capacity((w * cur.height) as usize);
        for y in 0..cur.height {
            let row = &cur.data[(y * cur.width) as usize..((y + 1) * cur.width) as usize];
            for (xmin, ks) in &k {
                let mut ss: i64 = 1 << (PRECISION_BITS - 1);
                for (i, kv) in ks.iter().enumerate() {
                    ss += i64::from(row[xmin + i]) * kv;
                }
                data.push(clip8(ss));
            }
        }
        cur = Gray {
            width: w,
            height: cur.height,
            data,
        };
    }
    if h != cur.height {
        let k = coeffs(cur.height, h, f);
        let mut data = Vec::with_capacity((cur.width * h) as usize);
        for (ymin, ks) in &k {
            for x in 0..cur.width {
                let mut ss: i64 = 1 << (PRECISION_BITS - 1);
                for (i, kv) in ks.iter().enumerate() {
                    let y = u32::try_from(ymin + i).unwrap_or(0);
                    ss += i64::from(cur.data[(y * cur.width + x) as usize]) * kv;
                }
                data.push(clip8(ss));
            }
        }
        cur = Gray {
            width: cur.width,
            height: h,
            data,
        };
    }
    cur
}

/// Scale down to fit within 672 × 192, then pad (white, top left) to at least 32 × 32.
#[allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)]
fn minmax(g: &Gray) -> Gray {
    let ratio = (f64::from(g.width) / f64::from(MAX_W)).max(f64::from(g.height) / f64::from(MAX_H));
    let g = if ratio > 1.0 {
        resize_with(
            g,
            ((f64::from(g.width) / ratio).floor() as u32).max(1),
            ((f64::from(g.height) / ratio).floor() as u32).max(1),
            Filter::Bilinear,
        )
    } else {
        g.clone()
    };
    let (w, h) = (g.width.max(MIN_W), g.height.max(MIN_H));
    if (w, h) == (g.width, g.height) {
        return g;
    }
    let mut data = vec![255u8; (w * h) as usize];
    for y in 0..g.height {
        let src = (y * g.width) as usize;
        let dst = (y * w) as usize;
        data[dst..dst + g.width as usize].copy_from_slice(&g.data[src..src + g.width as usize]);
    }
    Gray {
        width: w,
        height: h,
        data,
    }
}

/// The model input: one channel, normalised.
fn tensor(g: &Gray) -> Result<Tensor<f32>, String> {
    let (mean, std) = (0.7931 * 255.0, 0.1738 * 255.0);
    let x: Vec<f32> = g
        .data
        .iter()
        .map(|&p| (f32::from(p) - mean) / std)
        .collect();
    Tensor::from_array(([1usize, 1, g.height as usize, g.width as usize], x)).map_err(err)
}

impl LatexOcr {
    pub(crate) fn load(
        rt: &Runtime,
        resizer: &Path,
        encoder: &Path,
        decoder: &Path,
        tokenizer: &Path,
    ) -> Result<Self, String> {
        Ok(Self {
            resizer: Mutex::new(rt.session(resizer)?),
            encoder: Mutex::new(rt.session(encoder)?),
            decoder: Mutex::new(rt.session(decoder)?),
            vocab: vocab(&std::fs::read_to_string(tokenizer).map_err(err)?)?,
        })
    }

    /// The image at the width the resizer model prefers (reference: `loop_image_resizer`).
    #[allow(clippy::many_single_char_names)] // ratio, width, height and the image, as in the reference
    fn sized(&self, line: &Gray) -> Result<Option<Gray>, String> {
        let Some(first) = pad(line) else {
            return Ok(None);
        };
        let input = minmax(&first);
        let (mut r, mut w, mut h) = (1.0f32, input.width, input.height);
        let mut last = None;
        for _ in 0..10 {
            #[allow(
                clippy::cast_precision_loss,
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss
            )]
            {
                h = ((h as f32) * r) as u32;
            }
            // The reference grows with BILINEAR and shrinks (or keeps) with LANCZOS.
            let filter = if r > 1.0 {
                Filter::Bilinear
            } else {
                Filter::Lanczos
            };
            let Some(p) = pad(&minmax(&resize_with(&input, w, h, filter))) else {
                break;
            };
            let mut s = self.resizer.lock();
            let out = s.run(ort::inputs!["input" => tensor(&p)?]).map_err(err)?;
            let (_, scores) = out[0].try_extract_tensor::<f32>().map_err(err)?;
            let best = scores
                .iter()
                .enumerate()
                .max_by(|a, b| a.1.total_cmp(b.1))
                .map_or(0, |(i, _)| i);
            let new_w = u32::try_from(best + 1).unwrap_or(1) * 32;
            let done = new_w == p.width;
            #[allow(clippy::cast_precision_loss)]
            {
                r = new_w as f32 / p.width as f32;
            }
            w = new_w;
            last = Some(p);
            if done {
                break;
            }
        }
        Ok(last)
    }

    fn decode(&self, ids: &[i64]) -> String {
        let mut s = String::new();
        for &id in ids {
            if id == EOS {
                break;
            }
            let tok = usize::try_from(id).ok().and_then(|i| self.vocab.get(i));
            match tok.map(String::as_str) {
                None | Some("[BOS]" | "[PAD]" | "[EOS]") => {}
                Some(t) => s.push_str(t),
            }
        }
        clean_latex(&s.replace('Ġ', " "))
    }
}

impl FormulaModel for LatexOcr {
    fn id(&self) -> &'static str {
        "latex-ocr"
    }

    fn latex(&self, line: &Gray) -> Result<String, String> {
        let started = Instant::now();
        let Some(img) = self.sized(line)? else {
            return Ok(String::new());
        };
        let (cshape, context) = {
            let mut s = self.encoder.lock();
            let out = s.run(ort::inputs!["input" => tensor(&img)?]).map_err(err)?;
            let (shape, v) = out[0].try_extract_tensor::<f32>().map_err(err)?;
            let shape: Vec<usize> = shape
                .iter()
                .map(|&d| usize::try_from(d).unwrap_or(0))
                .collect();
            (shape, v.to_vec())
        };
        let mut ids: Vec<i64> = vec![BOS];
        let mut dec = self.decoder.lock();
        while ids.len() <= MAX_TOKENS && started.elapsed() < MAX_TIME {
            let n = ids.len();
            let x = Tensor::from_array(([1usize, n], ids.clone())).map_err(err)?;
            let mask = Tensor::from_array(([1usize, n], vec![true; n])).map_err(err)?;
            let ctx = Tensor::from_array((cshape.clone(), context.clone())).map_err(err)?;
            let out = dec
                .run(ort::inputs!["x" => x, "mask" => mask, "context" => ctx])
                .map_err(err)?;
            let (shape, logits) = out[0].try_extract_tensor::<f32>().map_err(err)?;
            let classes = usize::try_from(shape[2]).unwrap_or(1);
            let last = &logits[(n - 1) * classes..n * classes];
            let next = last
                .iter()
                .enumerate()
                .max_by(|a, b| a.1.total_cmp(b.1))
                .map_or(EOS, |(i, _)| i64::try_from(i).unwrap_or(EOS));
            ids.push(next);
            if next == EOS {
                break;
            }
            if ids.len() > MAX_REPEAT && ids[ids.len() - MAX_REPEAT..].iter().all(|&t| t == next) {
                // A loop: keep what came before it.
                ids.truncate(ids.len() - MAX_REPEAT + 1);
                break;
            }
        }
        Ok(self.decode(&ids[1..]))
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn ink_is_cropped_and_padded_to_32() {
        // 50 × 20, white, with a dark bar at x 10..20, y 5..9.
        let mut data = vec![250u8; 1000];
        for y in 5..9 {
            for x in 10..20 {
                data[y * 50 + x] = 10;
            }
        }
        let p = pad(&Gray {
            width: 50,
            height: 20,
            data,
        })
        .unwrap();
        assert_eq!((p.width, p.height), (32, 32));
        assert_eq!(p.data[0], 0, "ink at the top left after cropping");
        assert_eq!(p.data[(4 * 32) as usize], 255, "white padding");
        assert!(
            pad(&Gray {
                width: 4,
                height: 4,
                data: vec![200; 16]
            })
            .is_none()
        );
    }

    #[test]
    fn big_images_fit_and_small_ones_are_padded() {
        let g = Gray {
            width: 1344,
            height: 96,
            data: vec![255; 1344 * 96],
        };
        let m = minmax(&g);
        assert_eq!((m.width, m.height), (672, 48));
        let s = minmax(&Gray {
            width: 20,
            height: 10,
            data: vec![0; 200],
        });
        assert_eq!((s.width, s.height), (32, 32));
    }

    #[test]
    fn vocabulary_by_id() {
        let v =
            vocab(r#"{"model":{"vocab":{"[PAD]":0,"[BOS]":1,"[EOS]":2,"x":3,"Ġ=":4}}}"#).unwrap();
        assert_eq!(v[3], "x");
        assert_eq!(v[4], "Ġ=");
    }
}
