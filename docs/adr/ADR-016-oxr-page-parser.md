# ADR-016: OXR-1.0 (SoMark) — rejected; the Reader keeps its small on-device page models

Status: **rejected** (founder decision, 2026-11-18) — no OXR adapter is planned, also not for a server edition
Date: 2026-11-18
Relates to: ADR-007 (page models), ADR-013 (OCR: PP-OCRv5 everywhere, shipped with the app), ADR-015 (formulas: LaTeX-OCR)
Plan reference: Draft 2.2 §14.2 (`LayoutModel`, `Ocr`, `FormulaModel` ports)

## Context
The founder asked whether **OXR-1.0**, an open document parser published by SoMark on 20 October 2026, could be used instead of our OCR, and how it would fit our structure.

What OXR is, from its model card and licence (Hugging Face `SoMarkAI/OXR-1.0`, read 2026-11-18):

| Property | OXR-1.0 |
|---|---|
| **Model** | about 1.3 B parameters (metadata: 2 B, BF16), pretrained from scratch, a vision-language parser |
| **What it does** | a whole page in one pass: text, reading order, and 13 element kinds with boxes — formulas as LaTeX, chemistry (mhchem, SMILES), tables as HTML/Markdown, code, captions, headers and footers |
| **Pipeline** | two models: a lightweight layout model (`layout_analysis/`, a D-FINE detector by its module name) and OXR; the layout model is replaceable |
| **Reported quality** | first among open parsers on olmOCR-Bench (87.84), OmniDocBench and DocParsingBench (self-reported; an independent re-test was requested) |
| **Runs on** | Linux, NVIDIA GPU with BF16; **24 GB GPU to start, ≥ 48 GB recommended**; vLLM with an out-of-tree plugin, PyTorch for the layout service; Docker recommended |
| **Not available** | no ONNX, no GGUF/llama.cpp, no CPU path, no Windows support stated |
| **Languages** | Chinese and English shown; Persian or Arabic not mentioned |
| **Licence** | "OXR Open Source License": Apache-2.0 **plus** terms that take precedence (summarised below) |

The licence's additional terms:
- **Size limit:** commercial use is free only while the company and its affiliates have **under USD 2 M in total funding and under USD 2 M in annual revenue**; above either, a separate commercial licence is needed before (or to continue) commercial use.
- **No competing hosted service:** no hosted API that competes with SoMark's parsing or OCR API, paid or free, whatever the company's size.
- **Attribution for online services:** online services must show "SoMark OXR" prominently.
- **Termination:** rights end automatically when a term is broken.

## Decision
1. **OXR is not used**, neither in the Reader nor as a server add-on. The founder's reason: a reading tool must not depend on a 24–48 GB GPU model to read papers. In addition:
   - **Same on every device, offline (ADR-013):** OXR needs a 24–48 GB NVIDIA GPU, Linux and vLLM; the minimum laptop has a 4 GB GPU and Windows.
   - **Embeddable:** there is no ONNX or CPU build, so it cannot run inside the engines through ONNX Runtime like the other page models.
   - **Licence:** free commercial use only under USD 2 M funding and USD 2 M revenue; above that, a licence from SoMark would be needed for a core feature.
2. **The Reader keeps its small on-device page models:**
   - PP-OCRv5 for scanned pages (ADR-013);
   - Egret-medium for layout (ADR-007);
   - LaTeX-OCR for formulas (ADR-015).
   All run on the minimum laptop's CPU, offline, the same everywhere.
3. **The better path later is a refinement loop** around small models, not one large model. A small vision model checks and completes what the small page models produce, starting with equations:
   - LaTeX-OCR reads a formula;
   - the LaTeX is rendered and compared with the crop of the page;
   - a small vision-language model (on-device class, a few GB at most) is shown the crop and the rendered result and corrects the LaTeX;
   - the loop stops when the rendering matches the crop, or after a few rounds.
   It is described as a future option in ADR-015 and is to be measured on the golden equation set before it is built.
4. **What we keep from the evaluation:**
   - **Element list:** OXR's 13 element kinds (including chemical equations, chemical structures and code blocks) remain a useful target for `ObjectKind`; the three missing kinds are on the backlog.
   - **Benchmarks:** olmOCR-Bench and OmniDocBench are external references next to our golden sets when page models are compared.

## Alternatives considered
| Option | Why not now |
|---|---|
| Replace PP-OCRv5 with OXR everywhere, or run it as a server add-on | Needs a 24–48 GB NVIDIA GPU, Linux and vLLM; no CPU/ONNX path; breaks offline-on-every-device (ADR-013); licence limit |
| Convert OXR to ONNX ourselves | A 1.3–2 B parameter autoregressive model on CPU would take many seconds per page on the minimum laptop (LaTeX-OCR, about 30 times smaller, already takes 1–3 s per formula); the licence still applies |
| Use SoMark's hosted API | Sends users' papers to an outside service, against local-first (Draft 2.2 §6); needs network |
| Other 2026 VLM parsers (PaddleOCR-VL-1.5, LightOnOCR-2-1B, GLM-OCR) | Same class of model: GPU-sized, to be compared in the same spike when the server edition starts |

## Consequences
- **No change to the Reader:** ADR-013 and ADR-015 stand.
- **No OXR adapter**, no GPU-server page parser in the plans.
- **Future work:** the formula refinement loop (ADR-015, "Later"), measured on the golden equation set first.
- **Backlog:** `ObjectKind` gains chemical equation, chemical structure and code block.

## Sources
- OXR-1.0 model card: https://huggingface.co/SoMarkAI/OXR-1.0
- OXR licence: https://huggingface.co/SoMarkAI/OXR-1.0/blob/main/LICENSE
- Release coverage: https://letsdatascience.com/news/somark-open-sources-13b-oxr-10-document-parser-with-benchmar-903f39c6
