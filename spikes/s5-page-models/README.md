# Spike P0.S5 — Page understanding models on CPU

**Question (plans/phase-0, work package B):** for a 20-page paper on CPU, can layout objects, reading order, OCR of image-only pages and formulas-to-LaTeX be prepared in the background — text, outline and objects inside 10 s (formulas may continue after) — with licences we can ship?
**Fall-back named in the plan:** lighter models; formulas prepared lazily; a Python worker as a bridge.

**Throw-away code.** Lessons go into ADR-007 and Phase 1, M4.

## Files

| File | Does |
|---|---|
| `models.json`, `fetch-models.ps1` | Pinned model files (Hugging Face commit + SHA-256), downloaded to `D:\local LLM\LLM-Models\page-models` |
| `inspect_models.py` | Input/output signatures of every ONNX model |
| `quantize.py` | int8 copy of Heron (tested and rejected: 10× slower) |
| `layout.py`, `run-layout.ps1` | Layout models on 9 fixture papers; truth from the paper itself: captions ("Figure N", "Table N") and equation numbers found in PDFium's characters (spike S2 output) |
| `ocr.py` | PP-OCRv5 mobile (detector + recogniser) on pages rendered as images; scored against NACA's own text layer and the correct Persian text |
| `winocr.ps1`, `winocr_score.py` | Windows' built-in OCR on the same pages |
| `formula.py` | Formulas found by the layout model, cropped at 4×, converted by LaTeX-OCR (pix2tex ONNX); crops saved for checking |
| `summarise.py` | `out/report.md` |

## Reproduce

```
cargo xtask fetch-fixtures            # plus spike S2's output (spikes/s2-pdfium/out)
pwsh spikes/s5-page-models/fetch-models.ps1
pwsh spikes/s5-page-models/run-layout.ps1
uv run --no-project --python 3.13 --with onnxruntime --with numpy --with pypdfium2 --with pillow --with opencv-python-headless --with pyyaml python -I -X utf8 spikes/s5-page-models/ocr.py en 4 <naca reference txt> out/ocr-en-naca.json <naca pdf> 2 5
uv run --no-project --python 3.12 --with rapid_latex_ocr==0.0.9 --with requests --with pypdfium2 --with pillow python -I -X utf8 spikes/s5-page-models/formula.py egret out 16 <pdf> <first> <n> ...
python spikes/s5-page-models/summarise.py
```

Results: [RESULTS.md](RESULTS.md).
