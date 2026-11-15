//! ONNX Runtime adapter for the Document engine's page models (ADR-007, ADR-013): the layout
//! model (Docling Egret-medium, D-FINE) and PP-OCRv5 text recognition; formulas join later.
//!
//! ONNX Runtime is loaded at run time from a pinned build (`natives.toml`), one environment per
//! process; each model has one shared session with 4 intra-op threads, so preparation leaves
//! the other cores to the interface and the language model.
#![deny(missing_docs)]

use std::path::Path;

use ep_document_core::intel::{LayoutModel, RawDetection};
use ep_document_core::pdf::Bitmap;
use ort::environment::Environment;
use ort::session::Session;
use ort::value::Tensor;
use parking_lot::Mutex;

mod ppocr;
pub use ppocr::PpOcr;

/// Square input size of the Docling layout models.
const SIZE: usize = 640;
/// Intra-op threads per model (half of an 8-core laptop).
const THREADS: usize = 4;

pub(crate) fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

/// ONNX Runtime, loaded once.
pub struct Runtime {
    env: Environment,
}

impl std::fmt::Debug for Runtime {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Runtime").finish_non_exhaustive()
    }
}

impl Runtime {
    /// Load the ONNX Runtime library (`onnxruntime.dll` / `libonnxruntime.so` / `.dylib`).
    ///
    /// # Errors
    /// The library cannot be loaded or has an unsupported version.
    pub fn load(library: &Path) -> Result<Self, String> {
        let env = ort::init_from(library)
            .map_err(err)?
            .with_name("electropaper")
            .build()
            .map_err(err)?;
        Ok(Self { env })
    }

    pub(crate) fn session(&self, model: &Path) -> Result<Session, String> {
        // More threads made OCR slower on an 8-core laptop (measured: 4 → 11.5 s, 16 → 31 s).
        let threads = THREADS;
        Session::builder(&self.env)
            .map_err(err)?
            .with_optimization_level(ort::session::builder::GraphOptimizationLevel::Level3)
            .map_err(err)?
            .with_intra_threads(threads)
            .map_err(err)?
            .with_inter_threads(1)
            .map_err(err)?
            .commit_from_file(model)
            .map_err(err)
    }

    /// PP-OCRv5: detector, recogniser and the recogniser's `inference.yml` (character list).
    ///
    /// # Errors
    /// A model file cannot be loaded.
    pub fn ppocr(&self, det: &Path, rec: &Path, dict: &Path) -> Result<PpOcr, String> {
        PpOcr::load(self, det, rec, dict)
    }

    /// The Docling layout model at `model` (Egret-medium ONNX: `pixel_values` in, logits and
    /// boxes out).
    ///
    /// # Errors
    /// The model file cannot be loaded.
    pub fn layout(&self, model: &Path) -> Result<DoclingLayout, String> {
        Ok(DoclingLayout {
            session: Mutex::new(self.session(model)?),
        })
    }
}

/// A Docling layout model (RT-DETR family, Hugging Face export).
pub struct DoclingLayout {
    session: Mutex<Session>,
}

impl std::fmt::Debug for DoclingLayout {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DoclingLayout").finish_non_exhaustive()
    }
}

/// RGBA page → 640 × 640 RGB, bilinear, scaled to 0–1, channel-first (the model's input).
#[must_use]
#[allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)] // pixel arithmetic
pub fn preprocess(bmp: &Bitmap) -> Vec<f32> {
    let (w, h) = (bmp.width as usize, bmp.height as usize);
    let mut out = vec![0.0f32; 3 * SIZE * SIZE];
    if w == 0 || h == 0 || bmp.rgba.len() < w * h * 4 {
        return out;
    }
    let sx = w as f32 / SIZE as f32;
    let sy = h as f32 / SIZE as f32;
    for oy in 0..SIZE {
        let fy = ((oy as f32 + 0.5) * sy - 0.5).clamp(0.0, (h - 1) as f32);
        let (y0, ty) = (fy.floor() as usize, fy - fy.floor());
        let y1 = (y0 + 1).min(h - 1);
        for ox in 0..SIZE {
            let fx = ((ox as f32 + 0.5) * sx - 0.5).clamp(0.0, (w - 1) as f32);
            let (x0, tx) = (fx.floor() as usize, fx - fx.floor());
            let x1 = (x0 + 1).min(w - 1);
            for c in 0..3 {
                let p = |x: usize, y: usize| f32::from(bmp.rgba[(y * w + x) * 4 + c]);
                let top = p(x0, y0) * (1.0 - tx) + p(x1, y0) * tx;
                let bottom = p(x0, y1) * (1.0 - tx) + p(x1, y1) * tx;
                out[c * SIZE * SIZE + oy * SIZE + ox] = (top * (1.0 - ty) + bottom * ty) / 255.0;
            }
        }
    }
    out
}

/// Model output → detections: sigmoid class scores, best class per query, `cx, cy, w, h`
/// boxes (fractions) → corners. Low scores are kept for the caller to filter.
#[must_use]
pub fn postprocess(
    logits: &[f32],
    boxes: &[f32],
    classes: usize,
    min_score: f32,
) -> Vec<RawDetection> {
    let queries = logits.len().checked_div(classes).unwrap_or(0);
    let mut out = Vec::new();
    for q in 0..queries.min(boxes.len() / 4) {
        let row = &logits[q * classes..(q + 1) * classes];
        let (class, logit) =
            row.iter().enumerate().fold(
                (0, f32::NEG_INFINITY),
                |b, (i, &v)| if v > b.1 { (i, v) } else { b },
            );
        let score = 1.0 / (1.0 + (-logit).exp());
        if score < min_score {
            continue;
        }
        let b = &boxes[q * 4..q * 4 + 4];
        let (cx, cy, bw, bh) = (b[0], b[1], b[2], b[3]);
        out.push((
            class,
            score,
            [cx - bw / 2.0, cy - bh / 2.0, cx + bw / 2.0, cy + bh / 2.0],
        ));
    }
    out
}

impl LayoutModel for DoclingLayout {
    fn detect(&self, page: &Bitmap) -> Result<Vec<RawDetection>, String> {
        let input = Tensor::from_array(([1usize, 3, SIZE, SIZE], preprocess(page))).map_err(err)?;
        let mut session = self.session.lock();
        let outputs = session
            .run(ort::inputs!["pixel_values" => input])
            .map_err(err)?;
        let (lshape, logits) = outputs[0].try_extract_tensor::<f32>().map_err(err)?;
        let (_, boxes) = outputs[1].try_extract_tensor::<f32>().map_err(err)?;
        let classes = lshape
            .last()
            .copied()
            .and_then(|c| usize::try_from(c).ok())
            .unwrap_or(17);
        Ok(postprocess(logits, boxes, classes, 0.3))
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::float_cmp)]
mod tests {
    use super::*;

    #[test]
    fn preprocess_shapes_and_scales() {
        let bmp = Bitmap {
            width: 2,
            height: 2,
            rgba: vec![
                255, 0, 0, 255, 255, 0, 0, 255, 0, 0, 255, 255, 0, 0, 255, 255,
            ],
        };
        let v = preprocess(&bmp);
        assert_eq!(v.len(), 3 * SIZE * SIZE);
        assert_eq!(v[0], 1.0, "top-left red");
        assert_eq!(
            v[2 * SIZE * SIZE + (SIZE - 1) * SIZE],
            1.0,
            "bottom-left blue"
        );
    }

    #[test]
    fn postprocess_picks_best_class() {
        let logits = vec![-5.0, 3.0, -5.0, -5.0, -5.0, -5.0];
        let boxes = vec![0.5, 0.5, 0.2, 0.4, 0.1, 0.1, 0.1, 0.1];
        let d = postprocess(&logits, &boxes, 3, 0.5);
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].0, 1);
        assert!((d[0].2[0] - 0.4).abs() < 1e-6 && (d[0].2[3] - 0.7).abs() < 1e-6);
    }
}
