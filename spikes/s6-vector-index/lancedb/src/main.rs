//! S6 variant: LanceDB (the plan's choice), embedded, on disk. Mode `flat` (no index) or `ivfpq`.
//! usage: s6-lancedb <data-dir> <table-dir> <flat|ivfpq>
#[path = "../../common.rs"]
mod common;
use common::*;

use std::sync::Arc;
use std::time::Instant;

use futures::TryStreamExt;
use lancedb::arrow::arrow_array::types::Float32Type;
use lancedb::arrow::arrow_array::{Array, FixedSizeListArray, Int32Array, RecordBatch, RecordBatchIterator};
use lancedb::arrow::arrow_schema::{DataType, Field, Schema};
use lancedb::index::Index;
use lancedb::index::vector::IvfPqIndexBuilder;
use lancedb::query::{ExecutableQuery, QueryBase};
use lancedb::{DistanceType, connect};

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let (data_dir, table_dir, mode) = (a[1].clone(), a[2].clone(), a.get(3).cloned().unwrap_or_else(|| "flat".into()));
    let data = load(&data_dir);
    let rt = tokio::runtime::Runtime::new().expect("runtime");
    let _ = std::fs::remove_dir_all(&table_dir);
    let t = Instant::now();
    let table = rt.block_on(async {
        let db = connect(&table_dir).execute().await.expect("connect");
        let schema = Arc::new(Schema::new(vec![
            Field::new("id", DataType::Int32, false),
            Field::new("vector", DataType::FixedSizeList(Arc::new(Field::new("item", DataType::Float32, true)), DIM as i32), true),
        ]));
        let batch = RecordBatch::try_new(schema.clone(), vec![
            Arc::new(Int32Array::from_iter_values(0..data.n() as i32)),
            Arc::new(FixedSizeListArray::from_iter_primitive::<Float32Type, _, _>(
                (0..data.n()).map(|i| Some(data.vector(i).iter().map(|x| Some(*x)).collect::<Vec<_>>())), DIM as i32)),
        ]).expect("batch");
        let reader = RecordBatchIterator::new(vec![Ok(batch)], schema);
        let table = db.create_table("v", Box::new(reader)).execute().await.expect("create");
        if mode == "ivfpq" {
            table.create_index(&["vector"], Index::IvfPq(IvfPqIndexBuilder::default()
                .distance_type(DistanceType::Dot).num_partitions(256).num_sub_vectors(48)))
                .execute().await.expect("index");
        }
        table
    });
    let build = t.elapsed().as_secs_f64();
    let disk: u64 = walk(std::path::Path::new(&table_dir));
    report(&format!("lancedb ({mode})"), &data, build, serde_json::json!({ "disk_bytes": disk }), |q| {
        rt.block_on(async {
            let mut s = table.vector_search(q).expect("search").distance_type(DistanceType::Dot).limit(K);
            if mode == "ivfpq" { s = s.nprobes(24).expect("nprobes").refine_factor(5); }
            let batches: Vec<RecordBatch> = s.execute().await.expect("exec").try_collect().await.expect("collect");
            batches.iter().flat_map(|b| {
                let ids = b.column_by_name("id").expect("id").as_any().downcast_ref::<Int32Array>().expect("i32").clone();
                (0..ids.len()).map(move |i| ids.value(i) as u64).collect::<Vec<_>>()
            }).collect()
        })
    });
}

fn walk(p: &std::path::Path) -> u64 {
    std::fs::read_dir(p).map(|rd| rd.flatten().map(|e| {
        let m = e.metadata().expect("meta");
        if m.is_dir() { walk(&e.path()) } else { m.len() }
    }).sum()).unwrap_or(0)
}
