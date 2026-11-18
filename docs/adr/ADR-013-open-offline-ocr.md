# ADR-013: One open-source OCR engine everywhere — PP-OCRv5 on ONNX Runtime, shipped with the app

Status: **accepted**
Date: 2026-11-15
Amends: ADR-007 decision 2 (two OCR adapters) and decision 5 (every model downloaded on first use)
Plan reference: Draft 2.2 §14.2 (`Ocr` port), §28.3 (Scanned pages); Phase 1 P1.M4.OCR.1–2

## Context
ADR-007 planned two OCR adapters: Windows' built-in OCR for the page in view and PP-OCRv5 in the background. The founder asked for (1) the **same results on every device and system**, (2) an **open-source engine that is good on scientific papers**, and (3) OCR that is **local and works with no network**.

The Windows engine fails (1) and (2). It is closed. It behaves differently between Windows versions and needs installed language packs. It has no macOS or Linux version, so each system would give different text for the same scan.

## Decision
1. **One OCR engine on every system: PP-OCRv5 mobile** (PaddleOCR, Apache-2.0), run through ONNX Runtime (`ort`, the same runtime as the layout model, ADR-005/007).
   - **Detector:** `PP-OCRv5_mobile_det`, 4.8 MB.
   - **Recogniser:** `en_PP-OCRv5_mobile_rec`, 7.8 MB; Latin script with Greek letters and math symbols.
   - Both are pinned by Hugging Face commit and SHA-256 in `engines/document/facade/src/models.rs`.
2. **The OCR models ship inside the installer** (`models/ppocrv5-det`, `models/ppocrv5-rec-en`, about 13 MB). Scanned pages therefore work offline from the first start, with no download. `cargo xtask fetch-models` puts them in `.natives/models` for development and CI.
3. **Our own pre- and post-processing lives in the adapter** (`ep-document-adapter-onnx`, `ppocr.rs`), so it is identical everywhere:
   - **Detection:** the page is rendered by PDFium at about 216 dpi and limited to 1280 px on the long side. Text regions are taken as connected components of the probability map (mean ≥ 0.6), then grown with the DB "unclip" rule (ratio 1.6).
   - **Recognition:** batches of 8 crops sorted by width, 48 px high, greedy CTC decoding with the model's own dictionary.
   - **Reading order:** lines are put in reading order by the same XY-cut as page objects, so a two-column page reads down each column.
4. **The Windows OCR adapter is removed.** The `Ocr` port stays, so another engine can be added later if it is open and measured better.
5. **Results are made durable.**
   - Recognised text is cached per page, keyed by the engine id. Search, selection, copy, mentions and references then work on scanned pages like on any other page.
   - `document.ocr` writes the text into a new PDF as an invisible layer (render mode 3, Helvetica). The layer is added as an incremental update, so the original bytes stay as a prefix and the original file is never changed.
6. **The layout model (Egret-medium, 78 MB) is still an optional download** (ADR-007). Without it, the text rules (headings, references, mentions, citations, tables from text) still work offline, and the Reader offers the download in the status line. Bundling it as well is a founder decision about installer size.

## Measured (this laptop, 8 cores, CPU only)
| What | Result |
|---|---|
| Word agreement with the PDF's own text (scanned copy of a fixture page, 739 words) | F1 **0.999** |
| Text layer read back by PDFium | F1 **1.000** against the OCR text |
| Time per scanned page | about **11 s** (detection 0.4–0.5 s, recognition 10–11 s for ~100 lines) |
| Threads for recognition | 4 is fastest. 8 threads take 11.6 s and 16 threads take 31 s. |

The recognition time was checked against ONNX Runtime from Python on the same model and input sizes: 847 ms per batch of eight 800 px-wide lines, the same as our adapter. The time therefore comes from the model on this CPU, not from our code.

Consequence for the product: OCR runs as a **background preparation stage**. The page in view is done first, and progress is shown in the status line. Reading is never blocked. A 20-page scan takes about 4 minutes on the minimum hardware.

## Alternatives considered
| Option | Why not |
|---|---|
| Windows built-in OCR (ADR-007) | Closed; different results per Windows version and installed language packs; no macOS/Linux; weaker on tables and symbols |
| Tesseract | Lower accuracy on dense two-column scientific layouts and math symbols; a native C++ build per system |
| Nougat (Meta) | Code MIT, but the **weights are CC-BY-NC** (non-commercial), so it cannot be shipped. It is an autoregressive model: very slow on CPU, and it can fall into repetition loops. |
| GOT-OCR 2.0 | Weights published as Apache-2.0. 580 M parameters with an autoregressive decoder: far too slow for CPU page OCR, and there is no official ONNX export. It may be worth a measured spike later as an optional high-accuracy mode for formulas and tables on a GPU. |
| MinerU | A Python/PyTorch pipeline that cannot be embedded in the Rust engines. Its OCR is PaddleOCR anyway. The licence changed in 2026 to a custom "MinerU Open Source License" (Apache-2.0 plus attribution and scale terms); earlier versions were AGPL with CC-BY-NC-SA models. Not adopted as a dependency. |
| PDF-Extract-Kit | **AGPL-3.0** (YOLO-based layout, PyMuPDF), which is incompatible with our licence |
| OXR-1.0 (SoMark, 2026) | A whole-page vision-language parser (text, tables, LaTeX) that needs a 24–48 GB NVIDIA GPU on Linux, has no ONNX or CPU build, and has a USD 2 M funding/revenue limit for free commercial use. Not a replacement; a possible optional server adapter — see [ADR-016](ADR-016-oxr-page-parser.md) |
| PP-OCRv5 server models | More accurate on hard scans but several times slower on CPU; may become an optional "accurate" setting |

## Consequences
- The same text comes out for the same scan on Windows, macOS and Linux; only speed differs.
- The installer grows by about 13 MB for the OCR models, plus ONNX Runtime (shared with the layout model).
- **Open — speed:** about 11 s per page. Ways to try:
  - narrower recognition batches (crops split at wide gaps);
  - a GPU execution provider when the language model is idle;
  - PP-OCRv5 updates.
- **Open — Persian and other scripts:** Phase 3 (English-only until then). The port takes a recogniser per script.
- **Formulas** in scans are recognised as plain text. LaTeX comes from the formula model (P1.M4.EQ.1). The candidates are UniMERNet and PP-FormulaNet, both Apache-2.0.

## Evidence
`engines/document/facade/tests/intel.rs` → `ocr_makes_a_scanned_page_searchable` (builds an image-only copy of a fixture page and checks OCR, search, preparation stages and the written text layer).
