//! Automatic model search through the bus (contract 0.2.0), on small made-up GGUF files.

#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    clippy::items_after_statements
)]

use std::path::Path;
use std::sync::Arc;

use ep_bus::{Bus, CommandError, Envelope};
use ep_inference::Inference;
use ep_inference::contract::{ModelKind, Models};
use serde_json::{Value, json};

fn text(s: &str) -> Vec<u8> {
    let mut b = (s.len() as u64).to_le_bytes().to_vec();
    b.extend(s.as_bytes());
    b
}

/// A GGUF header: architecture, name, and a chat template when `chat`.
fn gguf(path: &Path, arch: &str, name: &str, chat: bool) {
    let mut pairs = vec![
        ("general.architecture", arch),
        ("general.name", name),
        ("general.license", "apache-2.0"),
    ];
    if chat {
        pairs.push(("tokenizer.chat_template", "{{ messages }}"));
    }
    let mut b = b"GGUF".to_vec();
    b.extend(3u32.to_le_bytes());
    b.extend(0u64.to_le_bytes());
    b.extend((pairs.len() as u64 + 1).to_le_bytes());
    b.extend(text("general.file_type"));
    b.extend(4u32.to_le_bytes());
    b.extend(15u32.to_le_bytes());
    for (k, v) in pairs {
        b.extend(text(k));
        b.extend(8u32.to_le_bytes());
        b.extend(text(v));
    }
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, b).unwrap();
}

/// A folder as people have it: models in sub-folders, parts, other formats, broken files.
fn library(dir: &Path) {
    gguf(
        &dir.join("text/My-Model-7B.gguf"),
        "llama",
        "My Model 7B",
        true,
    );
    gguf(&dir.join("text/small/tiny.gguf"), "qwen3", "Tiny", true);
    gguf(
        &dir.join("text/Qwen3-4B-Instruct-2507-Q4_K_M.gguf"),
        "qwen3",
        "Qwen3 4B",
        true,
    );
    gguf(
        &dir.join("big/huge-00001-of-00003.gguf"),
        "llama",
        "Huge",
        true,
    );
    gguf(
        &dir.join("big/huge-00002-of-00003.gguf"),
        "llama",
        "Huge",
        true,
    );
    gguf(&dir.join("vision/mmproj-x.gguf"), "clip", "X", false);
    gguf(&dir.join("embed/e.gguf"), "nomic-bert", "Embedder", false);
    gguf(&dir.join("ocr/o.gguf"), "deepseek2-ocr", "Reader", true);
    gguf(&dir.join(".hidden/secret.gguf"), "llama", "Hidden", true);
    std::fs::write(dir.join("text/broken.gguf"), b"GGUF but not really").unwrap();
    std::fs::create_dir_all(dir.join("hf")).unwrap();
    std::fs::write(dir.join("hf/model.safetensors"), b"x").unwrap();
}

fn engine(choices: Option<&Path>) -> Arc<Bus> {
    let bus = Arc::new(Bus::default());
    let spawner = Arc::new(ep_os::fake::FakeSpawner::default());
    Inference::new(
        vec![],
        vec![],
        spawner,
        None,
        choices.map(Path::to_path_buf),
    )
    .register(&bus)
    .unwrap();
    bus
}

async fn call(bus: &Bus, c: &str, i: Value) -> Result<Models, CommandError> {
    bus.call(Envelope::new(c, i))
        .await
        .map(|v| serde_json::from_value(v).unwrap())
}

fn ids(m: &Models, kind: ModelKind) -> Vec<&str> {
    let mut v: Vec<&str> = m
        .models
        .iter()
        .filter(|x| x.kind == kind)
        .map(|x| x.id.as_str())
        .collect();
    v.sort_unstable();
    v
}

#[tokio::test]
async fn models_in_sub_folders_are_found_by_what_they_are() {
    let dir = tempfile::tempdir().unwrap();
    library(dir.path());
    let bus = engine(None);
    let m = call(&bus, "inference.add_folder", json!({ "path": dir.path() }))
        .await
        .unwrap();
    assert_eq!(
        ids(&m, ModelKind::Chat),
        [
            "huge-00001-of-00003",
            "my-model-7b",
            "qwen3-4b-instruct-2507",
            "tiny"
        ]
    );
    assert_eq!(ids(&m, ModelKind::VisionPart), ["mmproj-x"]);
    assert_eq!(ids(&m, ModelKind::Embedding), ["e"]);
    assert_eq!(
        ids(&m, ModelKind::Other),
        ["o"],
        "a page reader does not answer questions"
    );
    let f = &m.folders[0];
    assert!(f.own && !f.missing && !f.partial);
    assert_eq!((f.models, f.other_formats), (7, 1));
    let tiny = m.models.iter().find(|x| x.id == "tiny").unwrap();
    assert_eq!(
        (
            tiny.name.as_str(),
            tiny.quantisation.as_deref(),
            tiny.licence.as_str()
        ),
        ("Tiny, Q4_K_M", Some("Q4_K_M"), "apache-2.0")
    );
    assert_eq!(
        m.models.iter().filter(|x| x.recommended).count(),
        1,
        "one suggestion"
    );
}

#[tokio::test]
async fn new_models_appear_and_the_same_name_is_told_apart() {
    let dir = tempfile::tempdir().unwrap();
    gguf(&dir.path().join("a/m.gguf"), "llama", "M", true);
    let bus = engine(None);
    call(&bus, "inference.add_folder", json!({ "path": dir.path() }))
        .await
        .unwrap();
    gguf(&dir.path().join("b/m.gguf"), "llama", "M", true);
    let m = call(&bus, "inference.search_models", Value::Null)
        .await
        .unwrap();
    assert_eq!(ids(&m, ModelKind::Chat), ["m", "m@b"]);
    // The folder and a folder in it: no model twice.
    let m = call(
        &bus,
        "inference.add_folder",
        json!({ "path": dir.path().join("a") }),
    )
    .await
    .unwrap();
    assert_eq!(ids(&m, ModelKind::Chat), ["m", "m@b"]);
}

#[tokio::test]
async fn a_task_keeps_its_model_after_a_restart() {
    let dir = tempfile::tempdir().unwrap();
    library(dir.path());
    let choices = dir.path().join("choices/llm.json");
    let bus = engine(Some(&choices));
    call(&bus, "inference.add_folder", json!({ "path": dir.path() }))
        .await
        .unwrap();
    call(
        &bus,
        "inference.assign",
        json!({ "task": "check", "model": "tiny" }),
    )
    .await
    .unwrap();
    let task = |m: &Models, id: &str| m.tasks.iter().find(|t| t.id == id).unwrap().model.clone();
    let again = engine(Some(&choices));
    let m = call(&again, "inference.models", Value::Null).await.unwrap();
    assert_eq!(task(&m, "check").as_deref(), Some("tiny"));
    assert_eq!(task(&m, "explain"), None);
    assert_eq!(m.folders.len(), 1, "the folder is remembered");
    let m = call(&again, "inference.assign", json!({ "task": "check" }))
        .await
        .unwrap();
    assert_eq!(task(&m, "check"), None, "back to the default");
}

#[tokio::test]
async fn what_cannot_work_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    library(dir.path());
    let bus = engine(None);
    call(&bus, "inference.add_folder", json!({ "path": dir.path() }))
        .await
        .unwrap();
    let kind = |r: Result<Models, CommandError>| match r.unwrap_err() {
        CommandError::Failed { kind, .. } => kind,
        e => format!("{e:?}").chars().take(12).collect(),
    };
    assert_eq!(
        kind(
            call(
                &bus,
                "inference.assign",
                json!({ "task": "explain", "model": "mmproj-x" })
            )
            .await
        ),
        "no_model"
    );
    assert_eq!(
        kind(
            call(
                &bus,
                "inference.assign",
                json!({ "task": "explain", "model": "nope" })
            )
            .await
        ),
        "no_model"
    );
    assert_eq!(
        kind(
            call(
                &bus,
                "inference.assign",
                json!({ "task": "cook", "model": "tiny" })
            )
            .await
        ),
        "InvalidInput"
    );
    let file = dir.path().join("text/tiny.txt");
    std::fs::write(&file, b"x").unwrap();
    assert_eq!(
        kind(call(&bus, "inference.add_folder", json!({ "path": file })).await),
        "not_a_folder"
    );
}

#[tokio::test]
async fn forgetting_a_folder_touches_nothing_on_disk() {
    let dir = tempfile::tempdir().unwrap();
    library(dir.path());
    let bus = engine(None);
    call(&bus, "inference.add_folder", json!({ "path": dir.path() }))
        .await
        .unwrap();
    let m = call(
        &bus,
        "inference.remove_folder",
        json!({ "path": dir.path() }),
    )
    .await
    .unwrap();
    assert_eq!((m.models.len(), m.folders.len()), (0, 0));
    assert!(dir.path().join("text/My-Model-7B.gguf").is_file());
}

#[tokio::test]
async fn a_folder_that_went_away_is_said_so() {
    let dir = tempfile::tempdir().unwrap();
    let gone = dir.path().join("usb");
    gguf(&gone.join("m.gguf"), "llama", "M", true);
    let bus = engine(None);
    call(&bus, "inference.add_folder", json!({ "path": gone }))
        .await
        .unwrap();
    std::fs::remove_dir_all(&gone).unwrap();
    let m = call(&bus, "inference.search_models", Value::Null)
        .await
        .unwrap();
    assert!(m.folders[0].missing && m.models.is_empty());
}
