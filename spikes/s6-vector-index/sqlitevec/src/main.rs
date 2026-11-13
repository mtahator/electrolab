//! S6 variant: sqlite-vec (vec0 virtual table in SQLite — exact scan inside the database).
#[path = "../../common.rs"]
mod common;
use common::*;
use rusqlite::{Connection, ffi::sqlite3_auto_extension};
use std::time::Instant;

fn bytes(v: &[f32]) -> Vec<u8> { v.iter().flat_map(|x| x.to_le_bytes()).collect() }

fn main() {
    let data = load(&std::env::args().nth(1).expect("data dir"));
    unsafe {
        #[allow(clippy::missing_transmute_annotations)]
        sqlite3_auto_extension(Some(std::mem::transmute(sqlite_vec::sqlite3_vec_init as *const ())));
    }
    let db = Connection::open_in_memory().expect("db");
    let t = Instant::now();
    db.execute_batch(&format!("CREATE VIRTUAL TABLE v USING vec0(embedding float[{DIM}] distance_metric=cosine)")).expect("create");
    db.execute_batch("BEGIN").expect("begin");
    {
        let mut ins = db.prepare("INSERT INTO v(rowid, embedding) VALUES (?, ?)").expect("prep");
        for i in 0..data.n() { ins.execute(rusqlite::params![i as i64, bytes(data.vector(i))]).expect("insert"); }
    }
    db.execute_batch("COMMIT").expect("commit");
    let build = t.elapsed().as_secs_f64();
    let mut q = db.prepare(&format!("SELECT rowid FROM v WHERE embedding MATCH ? AND k = {K} ORDER BY distance")).expect("prep");
    report("sqlite-vec (vec0, exact)", &data, build, serde_json::json!({}), |v| {
        q.query_map([bytes(v)], |r| r.get::<_, i64>(0)).expect("query").map(|r| r.expect("row") as u64).collect()
    });
}
