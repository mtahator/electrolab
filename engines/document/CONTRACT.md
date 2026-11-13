# Document engine — contract

Version: 0.2.0      Stability: experimental      Tier: T1
Crates: `ep-document-contract` (types), `ep-document-core` (pure logic), `ep-document-adapter-pdfium`, `ep-document-adapter-lopdf`, `ep-document` (facade)

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

### Planned (Phase 1, contracts written before code)
`document.extract`, `document.region`, `document.annotate`, `document.ocr`, `document.objects`, `document.equation`, `document.table`, `document.annotations`, `document.references` (Draft 2.2 §14.2, §28.6).

## Events
- `document.extracted { paper, extractor_version }` — after preparation; the search index is built in the same stage.

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
- 0.2.0 (2026-11-13): `info`, `text`, `render`, `thumbnails`, `outline`, `links` with the PDFium adapter; `search` on the bus (index built from PDFium text on first use).
- 0.1.0 (2026-11-13): `document.search` contract and core; `document.metadata` with the lopdf adapter and the facade.
