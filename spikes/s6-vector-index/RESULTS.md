# Spike P0.S6 — Results: is LanceDB worth its weight?

Date: 2026-11-13 · 100,000 unit vectors, 384 dimensions, 1,000 clusters (`make_data.py`, seed 7), 200 queries, exact top-10 as truth · Ryzen 7 5800H, release builds · raw output `out/bench.jsonl`, `build-times.jsonl`

## Verdict: **no index needed at T1 scale** — an exact brute-force scan in pure Rust meets the budget 8× over. LanceDB is not adopted.

| Variant | Query p50 / p95 | Recall@10 | Index build | Program size | Clean build | Crates |
|---|---|---|---|---|---|---|
| **Exact scan, pure Rust (rayon)** | **6.0 / 7.0 ms** | **1.00** | none | **0.3 MB** | 62 s | **12** |
| hnsw_rs (HNSW, pure Rust) | 0.53 / 0.69 ms | 1.00 | 35 s | 0.5 MB | 66 s | 82 |
| usearch, f32 (HNSW, C++) | 0.18 / 0.26 ms | 0.99 | 55 s | 2.9 MB | 189 s | 21 |
| usearch, int8 | 0.09 / 0.14 ms | **0.74** | 22 s | 2.9 MB | (same) | 21 |
| sqlite-vec (vec0, exact) | 43 / 60 ms (max 211) | 1.00 | 0.9 s | 2.0 MB | 39 s | 16 |
| LanceDB 0.41 | not measured | — | — | — | **fails without `protoc`** (Protocol Buffers compiler) | **589** |

Budget (Draft 2.2 §17): hybrid search over 100k passages < 50 ms.

## Findings

1. **Exact is fast enough and never wrong.** 6 ms for 100k × 384 on all cores; a typical project (100 papers × ~300 passages = 30k) takes ~2 ms. Exact search has no recall loss and no index to build, store, version or rebuild.
2. **Memory is the real cost of the scan:** 100k × 384 × 4 bytes = 154 MB as f32. Store vectors as f16 (77 MB) or memory-map the vector file so the OS pages it; measure in P1.M6.KNO.3.
3. **Approximate indexes start paying above ~1M vectors** (a team server, T2): usearch f32 is the fastest with near-perfect recall but brings C++ (breaks the local Linux/macOS cross-check, see CONTRIBUTING.md); hnsw_rs is pure Rust and exact enough. Both fit behind the existing `VectorIndex` port.
4. **int8 quantisation lost a quarter of the right answers** — not acceptable for evidence retrieval.
5. **sqlite-vec misses the budget** (exact scan inside SQLite, 43–211 ms).
6. **LanceDB is heavy for what we need:** 589 crates and an external `protoc` build tool before the first line runs. Its strengths (large on-disk datasets, versioned tables) matter for a server, not for a desktop index of tens of thousands of passages. Measuring its query speed would not change the decision; it can be revisited for the team server.

## Not covered
- Real embeddings (synthetic clustered vectors are a reasonable proxy for speed; recall of an exact scan is 1.0 by definition).
- Keyword half of hybrid search (tantivy) — measured in P1.M6.KNO.3.
