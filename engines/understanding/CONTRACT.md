# Understanding engine — contract

Version: 0.1.0      Stability: experimental      Tier: T1
Crates: `ep-understanding-contract` (types), `ep-understanding` (facade)

## Purpose
Explain and understand a paper (Draft 2.2 §14.5). Phase 1 builds the Reader commands of §28.6 first; the method graph follows in Stage 3. First slice (P1.M5.UND.1–2): `explain_span`.

## Commands

| Command | Input → Output | Permission |
|---|---|---|
| `understanding.explain_span` | `ExplainRequest` → `Explanation` (streamed when `stream` is set) | read |

`ExplainRequest { file: BlobRef, page, selection, before?, after?, stream?: { id } }`
- `selection` is the selected text (a word, term, symbol or sentence); `before` / `after` are up to 500 characters of the text around it on the page. When they are missing, the engine reads them from the page.

`Explanation { selection, text, status: unchecked | checked | corrected, evidence: [Evidence { page, y, quote }], model, timings: { first_token_ms, draft_ms, check_ms, total_ms } }`

Error kinds: those of `inference.complete` (`no_model`, `no_runtime`, …), plus `empty_selection`.

## How it works (spike S3, ADR-005)
1. **Draft:** the model explains the selection in 2–3 sentences from the nearby text only, starting with the selection itself (prefill), at most 150 tokens. It streams as `understanding.explaining { id, phase: "draft", text }`.
2. **Passages (deterministic, during the draft):** `document.search` finds where the selection appears in the paper (exact words, with the 40 words around each hit, which is where an abbreviation's long form stands); definitions first ("… (PINN)", "PINN = …"), then other pages; at most 6 passages of 400 characters are the evidence.
3. **Check:** the model is given the draft and the passages and answers "OK" or a corrected explanation. "OK" first → `checked`; a corrected text of ≥ 8 words → `corrected`, which replaces the draft and is sent as `phase: "check"`; anything else, or no passages → `unchecked` (the draft is kept).
4. The passages are sent in the request's `data` section, never as instructions; the nearby text is data too. The selection (at most 300 characters) is quoted inside the question, because the question is about it.

## Events (hub)
- `understanding.explaining` `{ id, phase: draft | checking | check, text }` — the text so far of the answer shown: the draft while it streams (`draft`), the finished draft while it is checked (`checking`), the corrected text if there is one (`check`).

## Uses (contracts only)
- `inference.complete`, `document.search`, `document.text`.

## Guarantees
1. The paper's text is never sent as instructions (Inference guarantee 2).
2. The status is honest: `checked` only when the model said OK against at least one passage; `corrected` only when the corrected text replaced the draft.
3. Evidence quotes are verbatim: they are cut from the page's text at the hit (white space collapsed, whole words).
4. Explaining never changes the paper or its preparation.

## Budget (design language §17, plan P1.M5.UND.2)
- First token < 500 ms (GPU, model loaded); checked answer < 2 s (S3 baseline with Qwen3-4B: 2.8 s — the draft is capped at 150 tokens and the passage search runs during the draft).

## Quality
- Conformance with Replay recordings (`engines/understanding/facade/tests/explain.rs`): draft → checked, draft → corrected, no passages → unchecked, evidence verbatim, data section.
- Golden Explain set (P0.C.2) with the real model when present: ≥ the S3 result (98% correct after the check with Qwen3-4B).

## Changelog
- 0.1.0 (2026-11-21): `explain_span` (P1.M5.UND.1–2).
