//! S6 variant: HNSW graph index, pure Rust (hnsw_rs).
#[path = "../../common.rs"]
mod common;
use common::*;
use hnsw_rs::prelude::*;
use std::time::Instant;

fn main() {
    let data = load(&std::env::args().nth(1).expect("data dir"));
    let t = Instant::now();
    let hnsw: Hnsw<f32, DistDot> = Hnsw::new(16, data.n(), 16, 200, DistDot {});
    let items: Vec<(&[f32], usize)> = (0..data.n()).map(|i| (data.vector(i), i)).collect();
    hnsw.parallel_insert_slice(&items);
    let build = t.elapsed().as_secs_f64();
    report("hnsw_rs (M=16, ef_c=200, ef_s=64)", &data, build, serde_json::json!({}), |q| {
        hnsw.search(q, K, 64).into_iter().map(|n| n.d_id as u64).collect()
    });
}
