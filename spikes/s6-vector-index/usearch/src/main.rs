//! S6 variant: usearch HNSW (C++ core), f32 and int8 storage.
#[path = "../../common.rs"]
mod common;
use common::*;
use std::time::Instant;
use usearch::{Index, IndexOptions, MetricKind, ScalarKind};

fn main() {
    let data = load(&std::env::args().nth(1).expect("data dir"));
    let quant = match std::env::args().nth(2).as_deref() { Some("i8") => ScalarKind::I8, _ => ScalarKind::F32 };
    let t = Instant::now();
    let opts = IndexOptions { dimensions: DIM, metric: MetricKind::IP, quantization: quant, connectivity: 16,
        expansion_add: 128, expansion_search: 64, multi: false };
    let index = Index::new(&opts).expect("index");
    index.reserve(data.n()).expect("reserve");
    for i in 0..data.n() { index.add(i as u64, data.vector(i)).expect("add"); }
    let build = t.elapsed().as_secs_f64();
    report(&format!("usearch ({quant:?}, M=16)"), &data, build, serde_json::json!({ "memory_bytes": index.memory_usage() }), |q| {
        index.search(q, K).expect("search").keys
    });
}
