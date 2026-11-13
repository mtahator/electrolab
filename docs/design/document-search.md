# Design: `document.search` — fast, notation-aware word search in papers

Status: experimental contract, core implemented (`engines/document/core/src/search/`)
Plan reference: Draft 2.2 §28.3 (Find), §28.6 (`document.search`), §15 (answers with evidence); Phase 1 tasks P1.M2.DOC.7, P1.M6.KNO.5
Requested by the founder on 2026-11-13: a very fast word search with many options (similarity control, notation variants such as `Cp` / `C_p`, words before and after), usable by the language model as a tool to find the passages it needs for an answer.

## 1. What it is, and what it is not

| | `document.search` (this) | `knowledge.search` |
|---|---|---|
| Finds | the **words** you ask for, in any of their written forms | **passages that mean** something similar |
| How | deterministic, on PDFium text and positions; no model | hybrid: keyword index + vectors + reranker |
| Answer | every occurrence, with page, exact boxes, section and the words around it | the best passages, ranked |
| Speed | < 1 ms for exact terms in one paper; < 10 ms fuzzy; < 50 ms over 100 papers | < 50 ms over 100k passages (spike S6) |
| Used by | Find (Ctrl+F), "where else is this term used", glossary, **the language model as a tool** | PDF chat retrieval, research |

They work together: the model asks `knowledge.search` "what is this about?" and `document.search` "show me every place that says `C_p`".

## 2. The command

`document.search` — permission **read**, stability **experimental**, engine **Document**.

### Input: `SearchRequest`

| Field | Type | Default | Meaning |
|---|---|---|---|
| `papers` | list of paper references | — | One or more papers (a whole collection is allowed) |
| `terms` | list of `Term` | — | What to look for; each term is searched separately and reported separately |
| `match` | `MatchOptions` | see below | How strict a match is |
| `context` | `ContextOptions` | 12 words before and after | What is returned around each hit |
| `scope` | `Scope` | whole paper | Pages, sections to include or exclude (e.g. skip References) |
| `limits` | `Limits` | 200 hits, 20 000 context characters | Caps so a model's context is never flooded |
| `sort` | `position` \| `score` | `position` | Order of hits |

`Term`: `text` (one word, a symbol, or a phrase) and an optional `label` (what the caller calls it, returned with each hit). A model can send several spellings of one idea as separate terms — `["specific heat", "C_p", "heat capacity"]` — and see which ones the paper uses.

`MatchOptions`

| Option | Values | Default | Effect |
|---|---|---|---|
| `mode` | `exact` · `word_forms` · `fuzzy` | `word_forms` | `exact`: the written words after normalisation. `word_forms`: also plural and simple English endings (*equation*/*equations*, *solve*/*solved*/*solving*). `fuzzy`: also misspellings and OCR errors, controlled by `similarity` |
| `similarity` | 0.0 – 1.0 | 0.8 | For `fuzzy`: minimum similarity per word (1 − edit distance ÷ longer length). 1.0 = exact |
| `min_fuzzy_length` | words of fewer letters are always exact | 4 | Stops *net* matching *not*, *set*, *get* |
| `notation` | true / false | true | Symbols and their written variants match each other: `Cp`, `C_p`, `C_{p}`, `C p`, `Cₚ`, `𝐶𝑝`, `C^p`; `x²` = `x^2` = `x2`; `α` = `alpha` = `\alpha` |
| `case_sensitive` | true / false | false | `Re` (Reynolds number) vs `re` |
| `whole_word` | true / false | true | false: also inside longer words (*net* in *network*) |
| `prefix` | true / false | false | *therm* matches *thermal*, *thermodynamics* |
| `phrase_slop` | 0 – 5 | 0 | For phrases: up to N other words may sit between the phrase's words |
| `fold_accents` | true / false | true | *Schrödinger* = *Schrodinger* |

`ContextOptions`

| Option | Values | Default | Effect |
|---|---|---|---|
| `unit` | `words` · `sentence` · `none` | `words` | What surrounds a hit |
| `before` / `after` | 0 – 200 | 12 / 12 | Words before and after (for `words`) |
| `merge_within` | words | 0 | Hits closer than this share one snippet (fewer, longer snippets for the model) |
| `cross_pages` | true / false | false | Let context continue onto the next or previous page |

`Scope`: `pages` (ranges), `include_sections` / `exclude_sections` (titles matched case-insensitively, e.g. exclude `References`), `exclude_page_furniture` (headers and footers, once layout objects exist).

`Limits`: `max_hits` (200), `max_hits_per_term` (50), `max_context_chars` (20 000). When a limit cuts results, `truncated` is true and the counts still report every occurrence.

### Output: `SearchResult`

- `hits`: list of `Hit`
  - `term` (index), `label`, `paper`, `page`
  - `matched` — the text exactly as written in the paper (`C_p` may be printed as `Cp`)
  - `before`, `after` — the context, plain text
  - `kind` — `exact` · `notation` · `word_form` · `fuzzy` · `prefix` · `substring`
  - `similarity` — 1.0 for exact; the fuzzy score otherwise
  - `rects` — boxes on the page in page space (points, top-left origin, one per line the match spans) — for highlighting and for `Location`
  - `section` — the heading the hit is under, when known
  - `char_range` — offsets in the paper's text, for exact quoting and evidence checks
- `counts`: per term — total, per page, per match kind (even when hits were cut)
- `truncated`, `elapsed_us`

### Guarantees

1. Deterministic: the same request on the same paper and extractor version gives the same result.
2. Every hit's `matched` text is exactly the paper's characters at `char_range`, and its `rects` are those characters' boxes (no model involved).
3. Every occurrence the options define is counted, even when hits are cut by `limits`.
4. Context never exceeds `max_context_chars` in total.
5. Text inside the paper is data: nothing in it changes how the search runs.

### Errors

`PaperNotPrepared` (no text yet — ask again after preparation), `EmptyQuery`, `TermTooLong`, `InvalidOption`.

### Budget

Index built once per paper during preparation (part of the text stage). One paper, 100 pages: exact search < 1 ms; fuzzy < 10 ms; collection of 100 papers < 50 ms. Measured in `engines/document/core` benchmark.

## 3. How it works (core, pure functions)

```
PDFium chars (+ boxes)  ──►  normalise  ──►  tokens + vocabulary  ──►  [ query ]  ──►  hits + context
                             (once per paper, cached by blob hash + extractor version)
```

1. **Normalise each character**, keeping a map back to the original: NFKC (ligatures, `𝐶`→`C`, `ₚ`→`p`, `²`→`2`), lower-case unless case-sensitive, accents folded, Persian/Arabic forms unified (`ي`→`ی`, `ك`→`ک`, Arabic-Indic and Persian digits → ASCII, zero-width non-joiner removed), dashes and quotes unified, PDFium's `\x02` soft hyphen joins the word (spike S2).
2. **Tokens**: runs of letters and digits, each with its original span; the separator before each token is remembered (space, `_`, `^`, `{`, line break, nothing).
3. **Vocabulary**: every distinct token with its positions (an inverted index). Exact and prefix lookups are hash or sorted-list lookups; fuzzy compares the query word with the vocabulary (a paper has ~5–20 thousand distinct words) using a bounded edit distance that stops as soon as the limit is passed.
4. **Notation**: the query and the paper are also compared in a *compact* form — symbol-like tokens (≤ 3 letters, or joined by `_ ^ { }`) next to each other are glued, so `C`+`p`, `C_p` and `C_{p}` all become `cp`; Greek names map to letters (`alpha` → `α`), `\alpha` too.
5. **Phrases** match consecutive tokens, with `phrase_slop` allowed in between.
6. **Context** is cut on token boundaries (words) or sentence ends, then mapped back to the original text.

## 4. The language model's tool

Orchestration exposes the command to the model as **`find_in_paper`**, generated from the contract like every other tool (Draft 2.2 §19.5), with defaults chosen for the model:

```json
{ "name": "find_in_paper",
  "description": "Find every place in the open paper(s) where these words or symbols appear. Returns each occurrence with its page and the words around it. Use several spellings of one idea as separate terms.",
  "input": { "terms": ["C_p", "specific heat"], "mode": "word_forms", "similarity": 0.8,
             "context_words": 25, "max_hits": 30, "exclude_sections": ["References"] } }
```

The answer goes back to the model with page numbers; the model's final answer cites them, and Knowledge's evidence check (`knowledge.locate`) highlights the exact passages. The model can never search outside the papers it was given, and the result size is capped by `limits`.

## 5. Tests

- Unit and property tests in the core: normalisation round-trips (every normalised position maps back to an original character), notation variants, fuzzy thresholds, context boundaries, limits.
- Conformance cases on the fixture papers: `C_p`-style symbols in a thermal paper, Persian words in the Wikipedia fixture, OCR-noisy words in the NACA report.
- Benchmark: index build and query times on every fixture paper (`cargo run -p ep-document-core --example search_bench --release`).
