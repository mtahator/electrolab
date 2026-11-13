# ADR-011: Result cache keyed by command, implementation version and input

Status: **accepted**
Date: 2026-11-13
Plan reference: Draft 2.2 §9.4 (content addressing), §17 (budgets); task P1.M1.STO.7

## Context
Preparation work (text extraction, page models, OCR, AI outlines) is expensive and deterministic for a given file and implementation. Reopening a paper must not redo it, and an improved implementation must not keep serving old results.

## Decision
1. Commands opt in with `Bus::register_cached(spec, implementation, handler)`. `implementation` is a version string bumped whenever the output for the same input may change (new model, new rules).
2. Key = `BLAKE3(command ‖ 0 ‖ implementation ‖ 0 ‖ canonical JSON input)`. Inputs refer to files by content hash (`BlobRef`), so changed files are new keys; object keys are sorted, so equal inputs give equal keys.
3. The cache step runs **after policy** and before the handler: a denied call is never answered from the cache.
4. Values are JSON files in the app cache folder (`<cache>/<aa>/<key>.json`), written atomically. The cache is disposable: deleting it only costs time.
5. Old implementation versions are never looked up again; a size-bounded sweep removes them (later, with the cache settings page).
6. Model-generated results are cached only with the model id inside `implementation`, so changing the model changes the key.

## Alternatives considered
| Option | Why not |
|---|---|
| Cache inside each engine | Every engine would re-solve keys, invalidation and storage |
| SQLite table for results | Large values bloat the database; files are simpler to sweep and inspect |
| Time-based expiry | Results do not go stale with time, only with implementation or input changes |

## Consequences
- A second identical call returns immediately (tested in `foundation/bus/tests/bus.rs`).
- Contract changes must bump the implementation string; review checks it with the schema diff.

## Evidence
`foundation/bus/src/cache.rs`, `foundation/bus/tests/bus.rs` (`second_identical_call_is_a_cache_hit`).
