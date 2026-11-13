//! Shared by every S6 variant: load the test set, time queries, score recall@10, print JSON.
#![allow(dead_code)]

use std::path::Path;
use std::time::Instant;

pub const DIM: usize = 384;
pub const K: usize = 10;

pub struct Data {
    pub vectors: Vec<f32>,
    pub queries: Vec<f32>,
    pub truth: Vec<u32>,
}

impl Data {
    pub fn n(&self) -> usize { self.vectors.len() / DIM }
    pub fn vector(&self, i: usize) -> &[f32] { &self.vectors[i * DIM..(i + 1) * DIM] }
    pub fn query(&self, i: usize) -> &[f32] { &self.queries[i * DIM..(i + 1) * DIM] }
    pub fn nq(&self) -> usize { self.queries.len() / DIM }
}

fn f32s(p: &Path) -> Vec<f32> {
    std::fs::read(p).expect("read").chunks_exact(4).map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]])).collect()
}

pub fn load(dir: &str) -> Data {
    let d = Path::new(dir);
    let truth = std::fs::read(d.join("truth.u32")).expect("read").chunks_exact(4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]])).collect();
    Data { vectors: f32s(&d.join("vectors.f32")), queries: f32s(&d.join("queries.f32")), truth }
}

/// Run `search` for every query; report build time, latency percentiles and recall@10 as JSON.
pub fn report(name: &str, data: &Data, build_s: f64, extra: serde_json::Value, mut search: impl FnMut(&[f32]) -> Vec<u64>) {
    for i in 0..5 { let _ = search(data.query(i)); } // warm-up
    let mut ms = Vec::new();
    let mut hits = 0usize;
    for i in 0..data.nq() {
        let t = Instant::now();
        let got = search(data.query(i));
        ms.push(t.elapsed().as_secs_f64() * 1000.0);
        let truth = &data.truth[i * K..(i + 1) * K];
        hits += got.iter().take(K).filter(|g| truth.contains(&(**g as u32))).count();
    }
    ms.sort_by(|a, b| a.total_cmp(b));
    let q = |p: f64| (ms[((ms.len() as f64 - 1.0) * p) as usize] * 1000.0).round() / 1000.0;
    let exe = std::env::current_exe().ok().and_then(|p| std::fs::metadata(p).ok()).map(|m| m.len());
    println!("{}", serde_json::json!({
        "variant": name, "n": data.n(), "dim": DIM, "build_s": (build_s * 100.0).round() / 100.0,
        "query_ms_p50": q(0.5), "query_ms_p95": q(0.95), "query_ms_max": q(1.0),
        "recall_at_10": (hits as f64 / (data.nq() * K) as f64 * 1000.0).round() / 1000.0,
        "exe_bytes": exe, "extra": extra,
    }));
}
