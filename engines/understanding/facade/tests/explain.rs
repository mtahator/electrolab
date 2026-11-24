//! `understanding.explain_span` through the bus, on a real paper (Document engine) with recorded
//! model replies (Replay): draft → corrected, draft → checked, no passages → unchecked.
#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    clippy::items_after_statements
)]

use std::path::{Path, PathBuf};
use std::sync::Arc;

use ep_bus::{Bus, Envelope};
use ep_document::{BlobAccess, Document};
use ep_inference::{Inference, Replay};
use ep_types::{BlobRef, Hash, MediaType};
use ep_understanding::Understanding;
use ep_understanding::contract::{ExplainStatus, Explanation};
use serde_json::json;

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..")
}

struct OneFile(PathBuf);
impl BlobAccess for OneFile {
    fn path_of(&self, _hash: &Hash) -> Option<PathBuf> {
        Some(self.0.clone())
    }
    fn put(&self, _bytes: &[u8], _media_type: MediaType) -> std::io::Result<BlobRef> {
        Err(std::io::Error::other("read-only in this test"))
    }
}

/// The engines on one paper with `replay` as the model; `None` when `PDFium` or the paper is missing.
fn setup(replay: Replay) -> Option<(Arc<Bus>, BlobRef)> {
    let src = root().join(".fixtures/papers/arxiv-2111.12066v2.pdf");
    let dir = root().join(".natives/pdfium/bin");
    if !src.exists() || !dir.exists() {
        eprintln!("skipped: PDFium or the fixture paper is missing");
        return None;
    }
    static P: std::sync::OnceLock<Arc<dyn ep_document::Backend>> = std::sync::OnceLock::new();
    let pdf = P
        .get_or_init(|| ep_document::start_pdfium(&dir).unwrap())
        .clone();
    let bus = Arc::new(Bus::default());
    Document::new(Arc::new(OneFile(src.clone())), Some(pdf), None)
        .unwrap()
        .register(&bus)
        .unwrap();
    Inference::with_backend(Arc::new(replay))
        .register(&bus)
        .unwrap();
    Understanding::new().register(&bus).unwrap();
    let bytes = std::fs::read(&src).unwrap();
    Some((
        bus,
        BlobRef {
            hash: Hash::of(&bytes),
            size: bytes.len() as u64,
            media_type: MediaType::pdf(),
        },
    ))
}

async fn explain(bus: &Bus, file: &BlobRef, selection: &str, page: u32) -> Explanation {
    let v = bus
        .call(Envelope::new(
            "understanding.explain_span",
            json!({ "file": file, "page": page, "selection": selection, "stream": { "id": "e1" } }),
        ))
        .await
        .unwrap_or_else(|e| panic!("{e:?}"));
    serde_json::from_value(v).unwrap()
}

#[tokio::test]
async fn a_wrong_draft_is_corrected_from_the_paper() {
    let replay = Replay::new()
        .on("explain.draft", "stands for Model-Driven Planning, a way to plan with models.")
        .on_containing(
            "explain.check",
            "Markov Decision Process",
            "MDP stands for Markov Decision Process, the formal model of the agent's decisions used for reinforcement learning here.",
        );
    let Some((bus, file)) = setup(replay.clone()) else {
        return;
    };
    let mut sub = bus.events().subscribe("understanding.explaining");
    let r = explain(&bus, &file, "MDP", 5).await;
    assert_eq!(r.status, ExplainStatus::Corrected, "{r:?}");
    assert!(r.text.starts_with("MDP stands for Markov Decision Process"));
    assert!(
        r.evidence
            .iter()
            .any(|e| e.quote.contains("Markov Decision Process")),
        "{:?}",
        r.evidence
    );
    assert!(r.evidence.len() <= 6);
    // The passages went in the data section, not into the instructions.
    let check = replay
        .seen()
        .into_iter()
        .find(|p| p.task == "explain.check")
        .unwrap();
    assert!(
        !check.messages[0].content.contains("Markov"),
        "instructions stay clean"
    );
    assert!(check.messages[1].content.contains("--- DATA"));
    // The draft streamed, then the correction replaced it.
    let mut phases = vec![];
    while let Ok(ep_bus::Recv::Event(e)) =
        tokio::time::timeout(std::time::Duration::from_millis(50), sub.recv()).await
    {
        phases.push(e.data["phase"].as_str().unwrap().to_string());
    }
    assert_eq!(phases.first().map(String::as_str), Some("draft"));
    assert_eq!(phases.last().map(String::as_str), Some("check"));
}

#[tokio::test]
async fn an_agreeing_check_marks_the_draft_checked() {
    let replay = Replay::new()
        .on(
            "explain.draft",
            "stands for Markov Decision Process, the model of sequential decisions.",
        )
        .on("explain.check", "OK");
    let Some((bus, file)) = setup(replay) else {
        return;
    };
    let r = explain(&bus, &file, "MDP", 5).await;
    assert_eq!(r.status, ExplainStatus::Checked);
    assert_eq!(
        r.text,
        "MDP stands for Markov Decision Process, the model of sequential decisions."
    );
}

#[tokio::test]
async fn no_passages_leaves_it_unchecked() {
    // No check recording: the check must not even be asked.
    let replay = Replay::new().on("explain.draft", "is not a word this paper uses.");
    let Some((bus, file)) = setup(replay.clone()) else {
        return;
    };
    let r = explain(&bus, &file, "Zqxjv", 1).await;
    assert_eq!(r.status, ExplainStatus::Unchecked);
    assert_eq!(r.evidence, []);
    assert!(replay.seen().iter().all(|p| p.task != "explain.check"));
}

#[tokio::test]
async fn a_vague_check_leaves_it_unchecked() {
    let replay = Replay::new()
        .on("explain.draft", "stands for Markov Decision Process.")
        .on("explain.check", "I am not sure.");
    let Some((bus, file)) = setup(replay) else {
        return;
    };
    let r = explain(&bus, &file, "MDP", 5).await;
    assert_eq!(r.status, ExplainStatus::Unchecked);
    assert_eq!(
        r.text, "MDP stands for Markov Decision Process.",
        "the draft is kept"
    );
}

#[tokio::test]
async fn quotes_are_verbatim() {
    let replay = Replay::new()
        .on("explain.draft", "is a thing.")
        .on("explain.check", "OK");
    let Some((bus, file)) = setup(replay) else {
        return;
    };
    let squash = |s: &str| s.split_whitespace().collect::<Vec<_>>().join(" ");
    for (word, page) in [("MDP", 5), ("policy", 12), ("linear programs", 2)] {
        let r = explain(&bus, &file, word, page).await;
        assert!(!r.evidence.is_empty(), "{word}");
        for e in &r.evidence {
            let t = bus
                .call(Envelope::new(
                    "document.text",
                    json!({ "file": file, "page": e.page }),
                ))
                .await
                .unwrap();
            assert!(
                squash(t["text"].as_str().unwrap()).contains(&e.quote),
                "{word}: {e:?}"
            );
            assert!(e.quote.chars().count() <= 400);
            assert!(
                e.quote.to_lowercase().contains(&word[..3].to_lowercase()),
                "{word}: {}",
                e.quote
            );
        }
    }
}

#[tokio::test]
async fn cancelling_explain_cancels_the_model() {
    let long = "word ".repeat(2_000_000);
    let replay = Replay::new().on("explain.draft", &long);
    let Some((bus, file)) = setup(replay) else {
        return;
    };
    let mut tokens = bus.events().subscribe("inference.token");
    let env = Envelope::new(
        "understanding.explain_span",
        json!({ "file": file, "page": 5, "selection": "MDP", "stream": { "id": "c" } }),
    );
    let id = env.id;
    let b = bus.clone();
    let h = tokio::spawn(async move { b.call(env).await });
    assert!(
        matches!(tokens.recv().await, ep_bus::Recv::Event(_)),
        "the answer streams"
    );
    assert!(bus.cancel(id));
    assert!(matches!(
        h.await.unwrap(),
        Err(ep_bus::CommandError::Cancelled)
    ));
    // The model is quiet: after what was already sent, nothing new comes.
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;
    let mut later = bus.events().subscribe("inference.token");
    assert!(
        tokio::time::timeout(std::time::Duration::from_millis(300), later.recv())
            .await
            .is_err(),
        "the model still writes"
    );
}

#[tokio::test]
async fn empty_selection_is_refused() {
    let Some((bus, file)) = setup(Replay::new()) else {
        return;
    };
    let e = bus
        .call(Envelope::new(
            "understanding.explain_span",
            json!({ "file": file, "page": 1, "selection": "  " }),
        ))
        .await
        .unwrap_err();
    assert!(
        matches!(&e, ep_bus::CommandError::Failed { kind, .. } if kind == "empty_selection"),
        "{e:?}"
    );
}
