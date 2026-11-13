"""Test set for spike S6: 100k clustered unit vectors (dim 384), 200 queries, exact top-10.

Clustered (1,000 centres) rather than uniform: real embeddings of a paper collection form topic
clusters, and uniform high-dimensional data is unrealistically hard for approximate indexes.
Files (little-endian): vectors.f32, queries.f32 (row-major float32), truth.u32 (200×10 ids),
meta.json. Deterministic (seed 7).
usage: python -I make_data.py <out-dir>
"""
import json
import sys
from pathlib import Path

import numpy as np

N, D, Q, K, C = 100_000, 384, 200, 10, 1_000
out = Path(sys.argv[1])
out.mkdir(parents=True, exist_ok=True)
rng = np.random.default_rng(7)
centres = rng.standard_normal((C, D)).astype(np.float32)
assign = rng.integers(0, C, N)
vecs = centres[assign] + 0.35 * rng.standard_normal((N, D)).astype(np.float32)
vecs /= np.linalg.norm(vecs, axis=1, keepdims=True)
qa = rng.integers(0, C, Q)
queries = centres[qa] + 0.45 * rng.standard_normal((Q, D)).astype(np.float32)
queries /= np.linalg.norm(queries, axis=1, keepdims=True)
truth = np.argsort(-(queries @ vecs.T), axis=1)[:, :K].astype(np.uint32)
vecs.tofile(out / "vectors.f32")
queries.tofile(out / "queries.f32")
truth.tofile(out / "truth.u32")
(out / "meta.json").write_text(json.dumps({"n": N, "dim": D, "queries": Q, "k": K, "clusters": C, "seed": 7}))
print(f"wrote {N}x{D} vectors ({vecs.nbytes / 1e6:.0f} MB), {Q} queries, exact top-{K}")
