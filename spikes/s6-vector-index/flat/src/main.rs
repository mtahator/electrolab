//! S6 variant: no index — exact brute-force scan, pure Rust, all cores (rayon).
#[path = "../../common.rs"]
mod common;
use common::*;
use rayon::prelude::*;
use std::time::Instant;

fn dot(a: &[f32], b: &[f32]) -> f32 {
    // 8 independent accumulators so the compiler vectorises the loop
    let mut acc = [0.0f32; 8];
    for (x, y) in a.chunks_exact(8).zip(b.chunks_exact(8)) {
        for j in 0..8 { acc[j] += x[j] * y[j]; }
    }
    acc.iter().sum()
}

fn main() {
    let data = load(&std::env::args().nth(1).expect("data dir"));
    let t = Instant::now();
    let n = data.n();
    let build = t.elapsed().as_secs_f64();
    report("flat (exact, pure Rust)", &data, build, serde_json::json!({ "threads": rayon::current_num_threads() }), |q| {
        let mut scored: Vec<(f32, u64)> = (0..n).into_par_iter().with_min_len(4096)
            .map(|i| (dot(q, data.vector(i)), i as u64)).collect();
        scored.select_nth_unstable_by(K, |a, b| b.0.total_cmp(&a.0));
        scored.truncate(K);
        scored.sort_by(|a, b| b.0.total_cmp(&a.0));
        scored.into_iter().map(|(_, i)| i).collect()
    });
}
