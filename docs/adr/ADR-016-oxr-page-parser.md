# ADR-016: OXR-1.0 (SoMark) — not a replacement for the Reader's OCR; an optional GPU/server page parser later

Status: **accepted** (evaluation) · OXR adapter **deferred** (Phase 2 server edition, gated on the conditions below)
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
1. **OXR does not replace PP-OCRv5 in the Reader.** It breaks the binding rules of ADR-013:
   - **Same on every device, offline:** it needs a 24–48 GB NVIDIA GPU, Linux and vLLM. The minimum laptop has a 4 GB GPU and Windows, and many users have no NVIDIA GPU at all.
   - **Embeddable:** there is no ONNX or CPU build, so it cannot run inside our Rust engines through ONNX Runtime like the other page models.
   - **Licence fit:** the funding and revenue limit would require a commercial licence once the company passes USD 2 M in funding or revenue. That is a business dependency we do not take on for a core feature.
2. **Where OXR fits: an optional page-parser adapter**, behind the ports we already have, for machines that can run it — the **server edition** (Phase 2) or a workstation with a large GPU:
   - **One pass, three ports:** one OXR call per page answers the `LayoutModel` (objects with boxes), the `Ocr` (text of scanned pages) and the `FormulaModel` (LaTeX of formulas) at once. It also gives tables as HTML/Markdown for `document.table`.
   - **A sidecar adapter:** OXR runs as its own process (vLLM server, as ADR-005 does for the language model) and the Document engine talks to it through an adapter crate (`ep-document-adapter-oxr`, HTTP to a local or LAN service). Nothing in the core changes; results are cached per file and model version, like every page model.
   - **Never required, always optional:** the Reader keeps working the same without it (PP-OCRv5, Egret layout, LaTeX-OCR). Settings → Page models lists it as "Page parser (server)", with an address to connect to.
3. **Before an OXR adapter is built**, all of these must hold:
   - a measured spike on our golden sets (layout, equations, tables, scanned pages) shows a clear gain over the current page models;
   - the licence question is settled in writing with SoMark for our case (desktop app, server edition, company size), or the company stays under both limits and the attribution and no-competing-API terms are met;
   - the server edition (Phase 2) is being built.
4. **What we take from OXR now:**
   - **Its element list** (13 kinds, including chemistry and code) is a good target for our `ObjectKind`. Chemical equations and structures are added to the object-kind backlog.
   - **Its benchmarks** (olmOCR-Bench, OmniDocBench) become external references next to our golden sets when page models are compared.

## Alternatives considered
| Option | Why not now |
|---|---|
| Replace PP-OCRv5 with OXR everywhere | Needs a 24–48 GB NVIDIA GPU, Linux and vLLM; no CPU/ONNX path; breaks offline-on-every-device (ADR-013); licence limit |
| Convert OXR to ONNX ourselves | A 1.3–2 B parameter autoregressive model on CPU would take many seconds per page on the minimum laptop (LaTeX-OCR, about 30 times smaller, already takes 1–3 s per formula); the licence still applies |
| Use SoMark's hosted API | Sends users' papers to an outside service, against local-first (Draft 2.2 §6); needs network |
| Other 2026 VLM parsers (PaddleOCR-VL-1.5, LightOnOCR-2-1B, GLM-OCR) | Same class of model: GPU-sized, to be compared in the same spike when the server edition starts |

## Consequences
- No change to the Reader now; ADR-013 and ADR-015 stand.
- The Phase 2 plan gains a task: a page-parser spike (OXR and peers on our golden sets, licence settled) and, if it passes, the `ep-document-adapter-oxr` sidecar adapter.
- `ObjectKind` backlog: chemical equation, chemical structure, code block.

## Sources
- OXR-1.0 model card: https://huggingface.co/SoMarkAI/OXR-1.0
- OXR licence: https://huggingface.co/SoMarkAI/OXR-1.0/blob/main/LICENSE
- Release coverage: https://letsdatascience.com/news/somark-open-sources-13b-oxr-10-document-parser-with-benchmar-903f39c6
