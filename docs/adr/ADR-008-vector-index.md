# ADR-008: Exact vector search behind the `VectorIndex` port; no LanceDB in T1

Status: **accepted**
Date: 2026-11-13
Plan reference: Draft 2.2 §14.3 (`VectorIndex` port), §17 (search < 50 ms over 100k passages), §21 (LanceDB proposed); task P0.S6

## Context
The Knowledge engine needs vector search for hybrid retrieval. The plan proposed LanceDB, with lighter options to compare against the < 80 MB installer budget.

## Decision
1. **T1 uses exact brute-force search in pure Rust** (parallel dot products over a contiguous vector array): 6 ms over 100k × 384 vectors, recall 1.0, no index, 0.3 MB, no extra dependencies.
2. Vectors stored as **f16 or memory-mapped** to keep memory near 77 MB per 100k passages (measure in P1.M6.KNO.3).
3. The `VectorIndex` port stays; an **approximate adapter** (usearch f32, or hnsw_rs to stay pure Rust) is added only when a store passes ~1M vectors (team server, T2).
4. **LanceDB is not adopted** for the desktop: 589 crates and a `protoc` build-tool dependency for capabilities a desktop index of tens of thousands of passages does not use. Revisit for the team server.

## Alternatives considered
| Option | Result in S6 | Why not (now) |
|---|---|---|
| LanceDB | does not build without `protoc`; 589 crates | weight and build complexity |
| usearch | 0.18 ms, recall 0.99; C++ | not needed at T1 scale; C++ breaks local cross-checks |
| hnsw_rs | 0.53 ms, recall 1.0; 35 s index build | index build and storage for no user-visible gain at T1 scale |
| sqlite-vec | 43–211 ms | misses the budget |
| int8 quantisation | recall 0.74 | loses evidence |

## Consequences
- Simplest possible code path; deterministic results; no index migrations.
- Memory grows linearly with passages; above ~1M vectors an approximate adapter is required.
- `plans/technical-structure.md` §11 updated: vector search = exact scan in T1.

## Evidence
`spikes/s6-vector-index/RESULTS.md`.
