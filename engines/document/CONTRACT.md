# Document engine — contract

Version: 0.4.0      Stability: experimental      Tier: T1
Crates: `ep-document-contract` (types), `ep-document-core` (pure logic), `ep-document-adapter-pdfium`, `ep-document-adapter-lopdf`, `ep-document-adapter-onnx`, `ep-document` (facade)

## Purpose
Read documents into structured, positioned content — and find things in them — for every host (Reader, Lab, CLI, Python, MCP).

## Commands

### document.search   (experimental, T1) — implemented in the core
Fast, deterministic, notation-aware word search with context. Full design: [`docs/design/document-search.md`](../../docs/design/document-search.md).

- **Input:** `SearchRequest { papers, terms: [Term { text, label? }], match: MatchOptions, context: ContextOptions, scope: Scope, limits: Limits, sort }`
  - `MatchOptions { mode: exact | word_forms (default) | fuzzy, similarity: 0.8, min_fuzzy_length: 4, notation: true, case_sensitive: false, whole_word: true, prefix: false, phrase_slop: 0, fold_accents: true }`
  - `ContextOptions { unit: words (default) | sentence | none, before: 12, after: 12, merge_within: 0, cross_pages: false }`
  - `Scope { pages: [PageRange], include_sections, exclude_sections }`
  - `Limits { max_hits: 200, max_hits_per_term: 50, max_context_chars: 20000 }`
  - `sort: position (default) | score`
- **Output:** `SearchResult { hits: [Hit], counts: [TermCount], truncated, elapsed_us }`
  - `Hit { term, label, paper, page, matched, before, after, kind: exact | notation | word_form | prefix | substring | fuzzy, similarity, rects: [PageRect], section, char_range, merged }`
  - `TermCount { term, total, per_page, per_kind }`
- **Errors:** `PaperNotPrepared(paper)` — prepare first; `EmptyQuery`; `TermTooLong(term)`; `InvalidOption(text)`.
- **Permission:** read.
- **Example:** `terms: ["C_p"]` on a paper containing `Cp`, `C_p`, `C_{p}`, `𝐶ₚ` and `C p` → five hits, kinds `exact`/`notation`, each with page, boxes and 12 words either side; "C. P. Burgess" is not a hit.

### document.metadata   (experimental, T1)
- **Input:** `{ file: BlobRef }` (the file is read through the host's `BlobAccess` port).
- **Output:** `PdfMetadata { title?, authors, doi?, pages, subject?, encrypted }` from the Info dictionary, cleaned: junk titles ("Microsoft Word - …", file names, template defaults) become `null`, authors are split, DOIs are found in custom keys, subject, keywords or title.
- **Errors:** `not_found`, `unreadable`.
- **Permission:** read.
- **Note:** many arXiv PDFs carry no Info metadata; first-page text (PDFium, M2) fills those gaps. Snapshot over the fixture papers: `engines/document/facade/tests/metadata-snapshot.json`.

### Reading commands (experimental, T1) — P1.M2.DOC.1–6
All positions are in page space (points, top-left origin, rotation applied; ADR-003). Pages are 1-based. Errors: `not_found`, `unreadable`, `password_required`, `bad_page`, `timeout`. Permission: read.

| Command | Input → Output |
|---|---|
| `document.info` | `{ file }` → `DocInfo { pages, sizes: [PageSize], encrypted }` (every page size before anything is drawn) |
| `document.text` | `{ file, page }` → `PageText { page, text, boxes, words, lines, unknown }` (layout level 0; `\n` line ends, U+00AD soft hyphens) |
| `document.render` | `{ file, page, width_px }` → `Rendered { page, image: BlobRef (PNG), width_px, height_px }` (render cache, keyed by file, page, width and renderer version) |
| `document.thumbnails` | `{ file, pages?, width_px ≤ 512 }` → `[Rendered]`, streaming one item per page |
| `document.outline` | `{ file }` → `[OutlineItem { title, target?, children }]` (the PDF's bookmarks; generated outlines come in M4) |
| `document.links` | `{ file, page }` → `[Link { rect, target }]`; `target` is `page { page, top? }`, `url { url }` (http, https, mailto only) or `unsupported` (launch, script, other files: never followed) |

The PDF backend is the `PdfBackend` port; the `PDFium` adapter owns one thread, answers through a channel with a 30 s timeout, and keeps four documents open (ADR-003). Hosts find `PDFium` with `ep_document::find_pdfium()` (`EP_PDFIUM_DIR`, next to the executable, or `.natives/pdfium/bin`).

### Annotations (experimental, T1) — P1.M3, ADR-006
The store is the truth; a PDF is an export. Annotations belong to a **paper** (`PaperId`), so they survive "Save into PDF". They are kept in the engine's own database, `document.sqlite`, in the open library folder (ADR-010 amendment 1), reached through the host's `BlobAccess::project()` port. Positions are in page space.

| Command | Input → Output | Permission |
|---|---|---|
| `document.annotations` | `{ paper, file }` → `AnnotationSet` (the first time a file is seen for the paper, the annotations already in it are imported; ids the paper has used before are never imported again) | create |
| `document.annotations_apply` | `{ paper, ops: [add { annotation } \| change { annotation } \| remove { id }] }` → `AnnotationSet` (one undoable edit; removing a note removes its replies) | create |
| `document.annotations_undo` / `_redo` | `{ paper }` → `AnnotationSet` | create |
| `document.annotate` | `{ paper, file }` → `Annotated { file, written, kept }`: a new file in the library's blob store = the given file's bytes + an incremental update with every annotation | create |

`Annotation { id, page, kind, rect, quads, paths, line?, color "#rrggbb", opacity, width, font_size?, category?, body, text?, author?, parent?, resolved, created_ms, modified_ms, pristine }`; kinds `highlight | underline | strike_out | squiggly | note | free_text | ink | rectangle | ellipse | line | arrow`; categories `definition | method | result | question | to_check`.
`AnnotationSet { paper, annotations, can_undo, can_redo, unsaved }` (`unsaved`: changes not yet written into the PDF).

Writing (ADR-006): standard subtypes with appearance streams; our id in `/NM`, category in `/Subj`, author in `/T`; notes get a closed `/Popup`; replies are `/Text` with `/IRT`; a resolved thread adds a `Review`/`Completed` state annotation. Unchanged imported annotations keep their original object; changed or deleted ones are dropped from `/Annots` (their popups and review states with them); kinds the Reader does not edit are never touched. Free text that is not Latin-1 has no appearance yet (P1.M3.PDF.5).

Errors: `no_project_open`, `invalid_annotation`, `conflict`, `write_failed`, `not_found`, `unreadable` (encrypted PDFs cannot be annotated yet).
Event: `document.annotations_changed { paper }` after every change.

Rendering: `document.render` and `document.thumbnails` leave annotations out of page images (the viewer draws them from the store).

### Document intelligence (experimental, T1) — P1.M4, ADR-007, ADR-013
A paper is **prepared** in the background: text → outline → objects → OCR → index → equations → tables → references. Every result is cached by file hash and implementation version, so preparing again costs nothing. Deterministic rules (the PDF's text, printed labels) come first; models fill what text cannot give. Pages are 1-based; positions in page space.

| Command | Input → Output | Permission |
|---|---|---|
| `document.prepare` | `PrepareRequest { file, first_pages }` → `Preparation { file, stages, image_only_pages }`, streaming; `first_pages` (the page in view) go first | read |
| `document.preparation` | `{ file }` → `Preparation` (state now, without starting work) | read |
| `document.objects` | `{ file, page }` → `PageObjects` with `DocObject { id, kind, rect, label, number, text, caption_rect, score, order }`; kinds figure, table, equation, heading, paragraph, caption, list, … | read |
| `document.region` | `RegionRequest { file, page, point or rect, width_px }` → `RegionHit { object?, rect, image (PNG blob), text }` — a lookup on prepared objects | read |
| `document.contents` | `{ file }` → `Contents { sections (generated outline), figures, tables, equations }` | read |
| `document.mentions` | `{ file, page }` → `[Mention { kind, text, numbers, quads, target? }]` ("Fig. 3(b)", "Figs. 2–4", "Table 2", "Eq. (5)", "Section 4.2") | read |
| `document.citation_markers` | `{ file, page }` → `[CitationMarker { text, quads, entries }]` ("[3–5, 9]", "(Smith et al., 2020)", "Smith and Lee [4]") | read |
| `document.references` | `{ file }` → `[Reference { index, label, text, authors, year, title, doi?, arxiv?, url?, page, rect }]` | read |
| `document.equation` | `EquationRequest { file, id or number }` → `Equation { object, page, latex?, note?, image, text }` (`latex` is empty until the formula model is installed; `note` says why) | read |
| `document.table` | `TableRequest { file, id }` → `Table { rows: [[Cell]], header_rows, units, csv }` (numbers parsed) | read |
| `document.ocr` | `{ file }` → `Ocred { file, pages, words }`: a **new** PDF = the original bytes + an incremental update with an invisible text layer on every scanned page (ADR-013); the original is never changed | create |
| `document.models` | `{}` → `[ModelInfo { id, purpose, size, present, licence }]` | read |

Errors: `not_found`, `unreadable`, `bad_page`, `model_missing` (a model the command needs is not installed), `no_object` (nothing at that place, or no scanned page with text), `write_failed`.
Events: `document.preparing { file, stage }` while a stage runs; `document.prepared { file }` when preparation ends.

**Scanned pages.** Pages with almost no text of their own are recognised with PP-OCRv5 (ONNX Runtime, the same on every system, models shipped with the app). The recognised text is cached per page and returned by `document.text`, and used by search and every rule above, so a scanned paper works like any other. About 11 s per page on the minimum hardware, in the background, page in view first.

**Models.** Pinned in `facade/src/models.rs` by folder, file, size and SHA-256. Hosts pass every folder to look in (bundled `models/` next to the executable first, then the user's data folder) to `Document::load_page_models`. The OCR models (13 MB) are bundled; the layout model (Egret-medium, 78 MB) is downloaded on request by the host, and without it objects come from text rules only.

### Planned (Phase 1, contracts written before code)
`document.extract` (Draft 2.2 §14.2, §28.6); LaTeX for `document.equation` (formula model, P1.M4.EQ.1).

## Events
- `document.preparing`, `document.prepared` — see Document intelligence.
- `document.extracted { paper, extractor_version }` — planned with `document.extract`.

## Uses (contracts only)
- none (Document is a base engine, Draft 2.2 §6.3).

## Guarantees (document.search)
1. Deterministic for the same request, paper and extractor version.
2. `matched` is exactly the paper's characters at `char_range`; `rects` are those characters' boxes (no model involved).
3. `counts` include every occurrence, even when `limits` cut the hits (`truncated = true`).
4. Total context never exceeds `limits.max_context_chars`.
5. Text in the paper is data and never changes how the search runs.

## Quality
- Conformance cases (`engines/document/core/src/search/mod.rs` tests): notation variants, Greek names, word forms, fuzzy thresholds, short words never fuzzy, phrases and slop, punctuation never joins symbols or phrases, prefix/substring/case, context by words and sentence, limits and counts, merging, Persian letter forms, exact character ranges, errors.

## Budget (measured 2026-11-13, 20 fixture papers, release build, this laptop)
| | Measured | Budget |
|---|---|---|
| Index build per paper | 1.4–39 ms (286 pages: 37 ms) | part of preparation |
| One query, largest paper (79k words) | 20–181 µs | < 1 ms exact, < 10 ms fuzzy |
| One query, all 20 papers (~350k words) | 0.14–2.7 ms | < 50 ms |

Raw numbers: `engines/document/core/benches-results/search-2026-11-13.jsonl`; rerun with `cargo run -p ep-document-core --example search_bench --release -- spikes/s2-pdfium/out`.

## Assets
- none yet (the Greek-name table and stemming rules are code; they move to assets if they become language-specific lists).

## Changelog
- 0.3.0 (2026-11-14): annotations — `annotations`, `annotations_apply`, `annotations_undo`, `annotations_redo`, `annotate` (P1.M3); page images no longer include annotations.
- 0.2.0 (2026-11-13): `info`, `text`, `render`, `thumbnails`, `outline`, `links` with the PDFium adapter; `search` on the bus (index built from PDFium text on first use).
- 0.1.0 (2026-11-13): `document.search` contract and core; `document.metadata` with the lopdf adapter and the facade.
