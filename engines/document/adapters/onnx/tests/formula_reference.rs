//! The formula recogniser against the reference implementation (ADR-015): spike S5 saved 16
//! formula crops of `arxiv-2111.12066v2` with the LaTeX that `rapid_latex_ocr` 0.0.9 wrote for
//! them (`spikes/s5-page-models/out/`). Our pipeline must give the same LaTeX for the same
//! images. Needs ONNX Runtime and the model in `EP_MODELS_DIR` (`<dir>/latex-ocr/…`); skipped
//! with a note otherwise.
#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use std::path::{Path, PathBuf};

use ep_document_core::intel::formula::{FormulaModel, Gray};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../..")
}

fn onnxruntime() -> Option<PathBuf> {
    let base = root().join(".natives/onnxruntime");
    std::fs::read_dir(base).ok()?.flatten().find_map(|e| {
        let p = e.path().join("lib/onnxruntime.dll");
        p.exists().then_some(p)
    })
}

fn gray_png(path: &Path) -> Gray {
    let dec = png::Decoder::new(std::io::BufReader::new(std::fs::File::open(path).unwrap()));
    let mut r = dec.read_info().unwrap();
    let mut buf = vec![0; r.output_buffer_size().unwrap()];
    let info = r.next_frame(&mut buf).unwrap();
    let ch = info.color_type.samples();
    let data = buf[..info.buffer_size()]
        .chunks(ch)
        .map(|p| match ch {
            1 | 2 => p[0],
            _ => u8::try_from(
                (u32::from(p[0]) * 299 + u32::from(p[1]) * 587 + u32::from(p[2]) * 114) / 1000,
            )
            .unwrap(),
        })
        .collect();
    Gray {
        width: info.width,
        height: info.height,
        data,
    }
}

#[test]
fn same_latex_as_the_reference_on_the_spike_crops() {
    let (Some(lib), Some(models)) = (onnxruntime(), std::env::var_os("EP_MODELS_DIR")) else {
        eprintln!("skipped: ONNX Runtime or EP_MODELS_DIR missing");
        return;
    };
    let dir = PathBuf::from(models).join("latex-ocr");
    if !dir.join("encoder.onnx").exists() {
        eprintln!("skipped: the formula model is not in EP_MODELS_DIR");
        return;
    }
    let out = root().join("spikes/s5-page-models/out");
    let Ok(text) = std::fs::read_to_string(out.join("formulas.json")) else {
        eprintln!("skipped: spike S5 output missing");
        return;
    };
    let rt = ep_document_adapter_onnx::Runtime::load(&lib).unwrap();
    let m = rt
        .latex_ocr(
            &dir.join("image_resizer.onnx"),
            &dir.join("encoder.onnx"),
            &dir.join("decoder.onnx"),
            &dir.join("tokenizer.json"),
        )
        .unwrap();
    let v: serde_json::Value = serde_json::from_str(&text).unwrap();
    let mut same = 0;
    let mut total = 0;
    for f in v["formulas"].as_array().unwrap() {
        let want = f["latex"].as_str().unwrap();
        // The reference stopped long outputs at 512 tokens; ours stops loops and slow lines
        // earlier (5 s), so only formulas it read in under 4 s are compared.
        if f["ms"].as_u64().unwrap_or(0) > 4000 {
            continue;
        }
        let img = gray_png(&out.join("formulas").join(f["image"].as_str().unwrap()));
        let t = std::time::Instant::now();
        let got = m.latex(&img).unwrap();
        total += 1;
        if got == want {
            same += 1;
        } else {
            eprintln!("DIFF {}\n  ref: {want}\n  got: {got}", f["image"]);
        }
        eprintln!("{} {:.2}s", f["image"], t.elapsed().as_secs_f32());
    }
    eprintln!("same LaTeX as the reference: {same}/{total}");
    // Measured 7–8 of 9 the same. The others are the reference's own sampling noise (it samples
    // at a tiny temperature: it wrote "patss" where it had written "pathes" earlier in the same
    // formula, as ours does) and a looping block our repeat stop ends earlier, by design.
    assert!(
        total > 0 && same * 4 >= total * 3,
        "{same}/{total} the same"
    );
}
