# Spike P0.S6 — Vector index: size, build cost and speed

**Question (plans/phase-0):** is LanceDB worth its weight in a < 80 MB installer, against lighter options, for 100k passages in < 50 ms?
**Throw-away code.** Lessons go into ADR-008 and P1.M6.KNO.3.

| File | Does |
|---|---|
| `make_data.py` | 100k clustered unit vectors (384-d), 200 queries, exact top-10 (written to a scratch folder) |
| `common.rs` | Shared by all variants: load data, time queries, recall@10, JSON |
| `flat/`, `hnsw/`, `usearch/`, `sqlitevec/`, `lancedb/` | One small crate per variant (each its own workspace) |
| `build-all.ps1` | Clean release build of each variant, timed, with crate count |

```
uv run --no-project --with numpy python -I spikes/s6-vector-index/make_data.py <data-dir>
pwsh spikes/s6-vector-index/build-all.ps1
spikes/s6-vector-index/<variant>/target/release/s6-<variant>.exe <data-dir> [f32|i8]
```

Results: [RESULTS.md](RESULTS.md).
