//! `understanding.suggest_folders` through the bus, with recorded model replies (Replay).

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use std::sync::Arc;

use ep_bus::{Bus, CommandError, Envelope};
use ep_inference::{Inference, Replay};
use ep_types::{BlobRef, Hash, MediaType};
use ep_understanding::Understanding;
use ep_understanding::contract::FolderSuggestion;
use serde_json::{Value, json};

fn bus(replay: Replay) -> Arc<Bus> {
    let bus = Arc::new(Bus::default());
    Inference::with_backend(Arc::new(replay))
        .register(&bus)
        .unwrap();
    Understanding::new().register(&bus).unwrap();
    bus
}

fn papers(titles: &[&str]) -> Vec<Value> {
    titles
        .iter()
        .enumerate()
        .map(|(i, t)| {
            let file = BlobRef {
                hash: Hash::of(t.as_bytes()),
                size: 1,
                media_type: MediaType::pdf(),
            };
            json!({ "id": format!("p{i}"), "title": t, "file": file })
        })
        .collect()
}

async fn suggest(bus: &Bus, input: Value) -> Result<FolderSuggestion, CommandError> {
    bus.call(Envelope::new("understanding.suggest_folders", input))
        .await
        .map(|v| serde_json::from_value(v).unwrap())
}

#[tokio::test]
async fn papers_are_grouped_and_existing_folders_are_used() {
    let replay = Replay::new()
        .on("sort.names", r#"{"folders":["Wind energy","invoices"]}"#)
        .on(
            "sort.assign",
            r#"{"1":"Wind energy","2":"Invoices","3":"Other","4":"Wind energy"}"#,
        );
    let b = bus(replay.clone());
    let r = suggest(
        &b,
        json!({ "papers": papers(&["Offshore wind", "Invoice 12", "A poem", "Wind farms"]), "folders": [{ "id": "f1", "name": "Invoices" }] }),
    )
    .await
    .unwrap();
    let got: Vec<(&str, Option<&str>, Vec<&str>)> = r
        .folders
        .iter()
        .map(|f| {
            (
                f.name.as_str(),
                f.existing.as_deref(),
                f.papers.iter().map(String::as_str).collect(),
            )
        })
        .collect();
    assert_eq!(
        got,
        [
            ("Wind energy", None, vec!["p0", "p3"]),
            ("Invoices", Some("f1"), vec!["p1"])
        ]
    );
    assert_eq!(r.unsure, ["p2"]);
    // Titles are data, never instructions.
    for p in replay.seen() {
        assert!(!p.messages[0].content.contains("Offshore"));
        assert!(p.messages[1].content.contains("--- DATA"));
    }
}

#[tokio::test]
async fn every_paper_is_in_exactly_one_place() {
    let titles: Vec<String> = (0..60).map(|i| format!("Paper {i}")).collect();
    let refs: Vec<&str> = titles.iter().map(String::as_str).collect();
    let replay = Replay::new()
        .on("sort.names", r#"{"folders":["A","B"]}"#)
        .on_with("sort.assign", None, |p| {
            let props = p.schema.as_ref().unwrap()["properties"]
                .as_object()
                .unwrap()
                .clone();
            let v: serde_json::Map<String, Value> = props
                .keys()
                .map(|k| {
                    (
                        k.clone(),
                        json!(if k.parse::<u32>().unwrap() % 2 == 0 {
                            "A"
                        } else {
                            "B"
                        }),
                    )
                })
                .collect();
            Value::Object(v).to_string()
        });
    let r = suggest(
        &bus(replay.clone()),
        json!({ "papers": papers(&refs), "folders": [] }),
    )
    .await
    .unwrap();
    let mut all: Vec<&String> = r
        .folders
        .iter()
        .flat_map(|f| &f.papers)
        .chain(&r.unsure)
        .collect();
    assert_eq!(all.len(), 60);
    all.sort();
    all.dedup();
    assert_eq!(all.len(), 60, "none twice");
    assert_eq!(
        replay
            .seen()
            .iter()
            .filter(|p| p.task == "sort.assign")
            .count(),
        3,
        "in parts"
    );
}

#[tokio::test]
async fn a_useless_answer_sorts_nothing() {
    let r = suggest(
        &bus(Replay::new().on("sort.names", r#"{"folders":["Other"," "]}"#)),
        json!({ "papers": papers(&["X"]), "folders": [] }),
    )
    .await
    .unwrap();
    assert_eq!((r.folders.len(), r.unsure.len()), (0, 1));
    let e = suggest(&bus(Replay::new()), json!({ "papers": [], "folders": [] }))
        .await
        .unwrap_err();
    assert!(
        matches!(&e, CommandError::Failed { kind, .. } if kind == "nothing_to_sort"),
        "{e:?}"
    );
}

#[tokio::test]
async fn cancelling_a_sorting_cancels_the_model() {
    let b = bus(Replay::new().on("sort.names", &"word ".repeat(2_000_000)));
    let env = Envelope::new(
        "understanding.suggest_folders",
        json!({ "papers": papers(&["X"]), "folders": [] }),
    );
    let id = env.id;
    let c = b.clone();
    let h = tokio::spawn(async move { c.call(env).await });
    tokio::time::sleep(std::time::Duration::from_millis(60)).await;
    assert!(b.cancel(id));
    assert!(matches!(h.await.unwrap(), Err(CommandError::Cancelled)));
}
