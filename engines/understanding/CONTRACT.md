# Understanding engine — contract

Version: 0.4.0      Stability: experimental      Tier: T1
Crates: `ep-understanding-contract` (types), `ep-understanding-core` (pure: tables, symbols, units, numbers), `ep-understanding` (facade)

## Purpose
Explain and understand a paper (Draft 2.2 §14.5). Phase 1 builds the Reader commands of §28.6 first; the method graph follows in Stage 3. First slice (P1.M5.UND.1–2): `explain_span`. Second slice (P1.M5.UND.3–6, 8): objects on the page.

## Commands

| Command | Input → Output | Permission |
|---|---|---|
| `understanding.explain_span` | `ExplainRequest` → `Explanation` (streamed when `stream` is set) | read |
| `understanding.explain_region` | `ExplainRegion` → `ObjectExplanation` (paragraph, equation, figure or table: what is there decides) | read |
| `understanding.explain_equation` | `ExplainRegion` with `object` → `ObjectExplanation` | read |
| `understanding.explain_figure` | `ExplainRegion` with `object` → `ObjectExplanation` | read |
| `understanding.table_answer` | `TableQuestion` → `TableAnswer` | read |
| `understanding.look_at_figures` | `LookAtFigures` → `FiguresSeen` (kept with the paper) | create |
| `understanding.figures_seen` | `{ paper, file }` → `FiguresSeen` (what is kept; no model) | read |

`ExplainRequest { file: BlobRef, page, selection, before?, after?, depth?: simpler | normal | deeper, stream?: { id } }`
- `selection` is the selected text (a word, term, symbol or sentence); `before` / `after` are up to 500 characters of the text around it on the page. When they are missing, the engine reads them from the page.

`Explanation { selection, text, status: unchecked | checked | corrected, evidence: [Evidence { page, y, quote }], model, timings: { first_token_ms, draft_ms, check_ms, total_ms } }`

Error kinds: those of `inference.complete` (`no_model`, `no_runtime`, …), plus `empty_selection`.

### understanding.suggest_folders (founder request, 2026-11-24)
`SuggestFolders { papers: [{ id, title, file, about? }], folders: [{ id, name }], stream?: { id } }` → `FolderSuggestion { folders: [{ name, existing?, papers: [id] }], unsure: [id], model, total_ms }` (permission: read)
- **It only suggests.** Nothing is moved; the window applies what the person approved (`workspace.apply_folders`), with undo.
- The model is asked twice, both with an enforced schema: which folders (short names, about one for every three papers and at most 8; existing ones where they fit), then which of them for each paper, or "Other". The second schema allows only those names, once per paper: **every paper is in exactly one folder or in `unsure`**.
- It sees the title and the first 220 characters of the first page, in the data section. At most 300 papers, 25 per question.
- Progress: `understanding.sorting { id, step: reading | thinking | sorting, done, total }`. Cancelling stops the model. Error kinds: `nothing_to_sort`, `too_many`, and those of `inference.complete`.
- Model: the one chosen for the task `sort`.

### Objects on the page (P1.M5.UND.3–6, 8; 2026-11-25)

`ExplainRegion { file, page, object?: id, rect?, paper?, depth?, stream? }` — `object` is an id of `document.objects`; without one, the object that `rect` covers best; without that, the text inside `rect` is a paragraph.
`ObjectExplanation { kind: paragraph | equation | figure | table, page, object?, label?, text, status, evidence, symbols: [Symbol], units?: UnitsCheck, latex?, seen?: FigureSeen, based_on: [caption | text | mentions | words | cells | formula | picture], left_out, model, timings }`

Every kind: **what is known by code first** (symbols, places), then a streamed draft, then the check of `explain_span` against the same text, then the **checks by code**: abbreviations of the object are given to the model with what the paper says they stand for; other words that spell one are replaced by the paper's, a sentence that says it stands for something else is left out; a sentence with a number that does not stand in what the model was given is left out (`left_out` counts both; then the status is `corrected`, or `unchecked` when the check did not answer). Evidence: the places used (caption, mentions, paragraphs), verbatim. Formulas a model writes as LaTeX in its sentences are shown as plain text (`λ_i`, `x_i^obs`), also while they stream (a formula appears when it is complete); this holds for every answer of the engine.
- A caption stands for its figure or table. Mentions are found with `document.search` ("Figure 3", "Fig. 3"); the object's own caption is not one of them.
- A figure or table the paper says nothing about gets an empty `text`, without asking the model.
- A sentence cut off at the end of the answer is left out (also in `explain_span`). A correction that talks about the draft instead of the thing, or that says less than half as many sentences, is not used (`unchecked`).

- **Paragraph:** its text, with what stands just before and after.
- **Table:** caption, cells, the places that mention it, and an outline made by code (how many rows, the smallest and largest of each column of numbers).
- **Figure:** caption, the places that mention it, the words inside it and, when the picture was looked at before, what was seen. No change of model. `based_on` says what was used; the window says in words when the picture itself was not looked at.
- **Equation:**
  - *Symbols:* every symbol of the LaTeX (`T_{in}`, `\dot{m}`, `\alpha`; not functions, operators or numbers). Its meaning is searched in the paper by code ("where T is …", "T: …", a list of symbols), nearest place first; what is left is asked of the model with an enforced schema, and kept **only when it stands in the text given, word for word**. `Symbol { symbol, shown, meaning?, unit?, page?, y?, quote? }`; one without a meaning is listed without one.
  - *Units:* `UnitsCheck { result: consistent | inconsistent | unknown, sides: [unit], missing: [symbol], unread }`. Code only: the LaTeX is read as a formula and the units of the symbols are carried through it. `consistent` only when every term is known; a formula that cannot be read is `unknown` with `unread: true`.
  - Without LaTeX (no formula model) the symbols are those defined nearby that occur in the printed equation, also in mathematical letters with spaced subscripts; the units are `unknown` with `unread: true`. A formula is never the meaning of a symbol.

`understanding.table_answer`: `TableQuestion { file, object: { page, id }, question }` → `TableAnswer { question, text, op, cells: [CellRef { row, col, text, rect, role: answer | used }], value?, unit?, status: computed | not_in_table, reason?, model, total_ms }`
- The model only fills a plan (enforced schema): `op` one of value, list, largest, smallest, average, sum, count, difference, ratio, change, none; `columns` and `rows` from **the table's own names**; `where` conditions on columns. The cells are chosen, the number computed and the sentence written **by code**. Where the question says a name of the table itself, the plan is narrowed to it. The sentence says what was done ("Smallest of RMSE (K) over 6 rows: 0.31 K, for PINN").
- Refused with a reason in words: the table does not hold it (`none`, nothing selected), cells that are not numbers, different units added up.
- At most 120 rows and 24 columns (`too_large`).

`understanding.look_at_figures`: `LookAtFigures { paper, file, objects?: [{ page, id }], consent, stream? }` → `FiguresSeen { figures: [FigureSeen { object: { page, id }, label?, text, model, at_ms }], total, model?, total_ms }` (`total` = the figures of the paper)
- Without `consent: true` it fails with `consent_needed`. Without `objects`: every figure of the paper that was not looked at yet.
- Kept piece by piece with the paper (`workspace.save_figures`), so an early end loses nothing; what is kept for another version of the file or another place is not used.
- Progress: `understanding.looking { id, done, total, label, text }` (`text` = what is read so far of this picture).

Error kinds besides those above: `no_object`, `not_a_table`, `too_large`, `empty_question`, `consent_needed`, `no_vision_model`.

## How it works (spike S3, ADR-005)
1. **Draft:** the model explains the selection in 2–3 sentences from the nearby text only, starting with the selection itself (prefill), at most 150 tokens. It streams as `understanding.explaining { id, phase: "draft", text }`.
   **An abbreviation** begins with what the paper itself says it stands for ("precessing vortex core (PVC)", "PVC (precessing vortex core)"; the letters must fit the words; the last definition at or before the page counts), found by code before the draft. A correction that does not say those words is not used (`unchecked`).
2. **Passages (deterministic, during the draft):** `document.search` finds where the selection appears in the paper (exact words, with the 40 words around each hit, which is where an abbreviation's long form stands); definitions first ("… (PINN)", "PINN = …"), then other pages; at most 6 passages of 400 characters are the evidence.
3. **Check:** the model is given the draft and the passages and answers "OK" or a corrected explanation. "OK" first → `checked`; a corrected text of ≥ 8 words → `corrected`, which replaces the draft and is sent as `phase: "check"`; anything else, or no passages → `unchecked` (the draft is kept).
4. The passages are sent in the request's `data` section, never as instructions; the nearby text is data too. The selection (at most 300 characters) is quoted inside the question, because the question is about it.

## Events (hub)
- `understanding.explaining` `{ id, phase: draft | checking | check, text }` — the text so far of the answer shown: the draft while it streams (`draft`), the finished draft while it is checked (`checking`), the corrected text if there is one (`check`).

- `understanding.symbols` `{ id, symbols }` — the symbols of an equation, as they are found (sent again when more is known).
- `understanding.looking` `{ id, done, total, label, text }` — pictures looked at, and the text of the one being read as it grows.

## Uses (contracts only)
- `inference.complete`, `inference.vision_read`, `document.search`, `document.text`, `document.objects`, `document.contents`, `document.region`, `document.equation`, `document.table`, `workspace.save_figures`, `workspace.get_figures`.

## Guarantees
1. The paper's text is never sent as instructions (Inference guarantee 2).
2. The status is honest: `checked` only when the model said OK against at least one passage; `corrected` only when the corrected text replaced the draft.
3. Evidence quotes are verbatim: they are cut from the page's text at the hit (white space collapsed, whole words).
4. Explaining never changes the paper or its preparation.
5. A number in the explanation of an object stands in what the model was given.
6. A meaning or unit of a symbol stands in the paper, at the place named.
7. A table's answer is computed by code from the cells it names.
8. A picture is looked at only with consent, on this computer.

## Budget (design language §17, plan P1.M5.UND.2)
- First token < 500 ms (GPU, model loaded); checked answer < 2 s (S3 baseline with Qwen3-4B: 2.8 s — the draft is capped at 150 tokens and the passage search runs during the draft).

- Objects: what code knows (symbols with their meanings, what a figure is based on) < 300 ms; figure explanation from what is kept < 1.5 s to the first full sentence (model loaded); table answer < 2 s.

## Quality
- Prompts, prefills and limits are an asset (`engines/understanding/facade/assets/explain.toml`), with a version; the depth (`simpler`, `deeper`) is in it, not in code.
- Core tests: table operations on written tables, symbols and definitions, abbreviations, formulas as plain text, units algebra, numbers check. Facade tests with Replay (`objects.rs`).
- Golden set (`fixtures/golden/explain.toml`), run with the real model: `cargo test -p ep-understanding --release --test golden -- --ignored --nocapture`. Measured 2026-11-25 (Qwen3-4B Q4_K_M on 4 GB): passages 78/78 (2.9 s each), figures 14/14 (3.4 s), tables 8/8 (4.5 s), equations 9/9 (5.5 s), table questions 16/16 (3.6 s). The times are above the budget: the check is a second answer.
- Conformance with Replay recordings (`engines/understanding/facade/tests/explain.rs`): draft → checked, draft → corrected, no passages → unchecked, evidence verbatim, data section.
- Golden Explain set (P0.C.2) with the real model when present: ≥ the S3 result (98% correct after the check with Qwen3-4B).

## Changelog
- 0.4.0 (2026-11-25): objects (`explain_region`, `explain_equation`, `explain_figure`, `table_answer`, `look_at_figures`, `figures_seen`), depth, prompts as an asset; `explain_span` begins an abbreviation with the paper's own words; golden set.
- 0.3.1 (2026-11-24): any number of papers fits one question; a part that cannot be read stays where it is instead of failing the whole.
- 0.3.0 (2026-11-24): `about` of a paper (its summary) is used instead of its first page.
- 0.2.0 (2026-11-24): `suggest_folders`.
- 0.1.0 (2026-11-21): `explain_span` (P1.M5.UND.1–2).
