//! The golden Explain set (P0.C.2, `fixtures/golden/explain.toml`) with the real models: the fixture
//! papers, the layout model (`EP_MODELS_DIR`) and a language model (`EP_LLM_DIR`).
//!
//! `cargo test -p ep-understanding --test golden -- --ignored --nocapture`
//! With `EP_GOLDEN_KIND=table` only one kind runs, with `EP_GOLDEN_PAPER=2310` one paper.
//! `what_is_there` lists what the fixture papers hold, for writing more cases.
#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    clippy::print_stdout
)]

use std::collections::{BTreeMap, HashMap};
use std::fmt::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use ep_bus::{Bus, Envelope};
use ep_document::{BlobAccess, Document};
use ep_inference::Inference;
use ep_types::{BlobRef, Hash, MediaType};
use ep_understanding::Understanding;
use parking_lot::Mutex;
use serde::Deserialize;
use serde_json::{Value, json};

#[derive(Deserialize)]
struct Golden {
    /// Least share of each kind that must be right.
    least: BTreeMap<String, f64>,
    case: Vec<Case>,
}

/// One question with what a right answer holds.
#[derive(Deserialize)]
struct Case {
    /// span, paragraph, figure, table, equation, ask.
    kind: String,
    paper: String,
    page: u32,
    /// Selected words, or the printed label ("Figure 3"), number or id of an object.
    #[serde(rename = "at")]
    place: String,
    #[serde(default)]
    question: String,
    /// Each must be there; "a|b" is one or the other.
    expect: Vec<String>,
}

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..")
}

#[derive(Default)]
struct Files(Mutex<HashMap<Hash, PathBuf>>);

impl BlobAccess for Files {
    fn path_of(&self, hash: &Hash) -> Option<PathBuf> {
        self.0.lock().get(hash).cloned()
    }
    fn put(&self, _bytes: &[u8], _media_type: MediaType) -> std::io::Result<BlobRef> {
        Err(std::io::Error::other("read-only in this test"))
    }
}

struct Spawner;
struct Child(std::process::Child);

impl Drop for Child {
    fn drop(&mut self) {
        let _ = self.0.kill();
    }
}

impl ep_os::ChildProcess for Child {
    fn id(&self) -> u32 {
        self.0.id()
    }
    fn try_wait(&mut self) -> ep_os::Result<Option<i32>> {
        Ok(self.0.try_wait()?.map(|s| s.code().unwrap_or(-1)))
    }
    fn kill(&mut self) -> ep_os::Result<()> {
        Ok(self.0.kill()?)
    }
}

impl ep_os::ProcessSpawner for Spawner {
    fn spawn(&self, spec: &ep_os::ProcessSpec) -> ep_os::Result<Box<dyn ep_os::ChildProcess>> {
        let child = std::process::Command::new(&spec.program)
            .args(&spec.args)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()?;
        Ok(Box::new(Child(child)))
    }
}

struct World {
    bus: Arc<Bus>,
    files: Arc<Files>,
    _cache: tempfile::TempDir,
}

/// The engines with the real models; a note when something is missing.
fn world() -> Option<World> {
    let pdfium = root().join(".natives/pdfium/bin");
    let dir = |name: &str| std::env::var_os(name).map(PathBuf::from);
    let (Some(models), Some(llm)) = (dir("EP_MODELS_DIR"), dir("EP_LLM_DIR")) else {
        println!("skipped: EP_MODELS_DIR or EP_LLM_DIR is not set");
        return None;
    };
    let cache = tempfile::tempdir().unwrap();
    let files = Arc::new(Files::default());
    let bus = Arc::new(Bus::default());
    let document = Document::new(
        files.clone(),
        Some(ep_document::start_pdfium(&pdfium).unwrap()),
        Some(cache.path()),
    )
    .unwrap();
    document.load_page_models(&[models]).unwrap();
    document.register(&bus).unwrap();
    Inference::new(
        vec![llm],
        vec![root().join(".natives")],
        Arc::new(Spawner),
        None,
        None,
    )
    .register(&bus)
    .unwrap();
    Understanding::new().register(&bus).unwrap();
    Some(World {
        bus,
        files,
        _cache: cache,
    })
}

impl World {
    fn open(&self, paper: &str) -> Option<BlobRef> {
        let path = root().join(".fixtures/papers").join(paper);
        let bytes = std::fs::read(&path).ok()?;
        let hash = Hash::of(&bytes);
        self.files.0.lock().insert(hash, path);
        Some(BlobRef {
            hash,
            size: bytes.len() as u64,
            media_type: MediaType::pdf(),
        })
    }

    async fn call(&self, name: &str, input: Value) -> Result<Value, String> {
        self.bus
            .call(Envelope::new(name, input))
            .await
            .map_err(|e| e.to_string())
    }

    /// The answer to a case.
    async fn answer(&self, c: &Case, file: &BlobRef) -> Result<Value, String> {
        if c.kind == "span" {
            return self
                .call(
                    "understanding.explain_span",
                    json!({ "file": file, "page": c.page, "selection": c.place }),
                )
                .await;
        }
        let here = self
            .call("document.objects", json!({ "file": file, "page": c.page }))
            .await?;
        let kind = match c.kind.as_str() {
            "ask" => "table",
            "paragraph" => "text",
            k => k,
        };
        let mut found = None;
        for o in here["objects"].as_array().unwrap() {
            if o["kind"] != kind {
                continue;
            }
            if o["label"] == c.place.as_str()
                || o["number"] == c.place.as_str()
                || o["id"] == c.place.as_str()
            {
                found = Some(o["id"].clone());
            }
        }
        let id = found.ok_or_else(|| {
            let there: Vec<String> = here["objects"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|o| o["kind"] != "text")
                .map(|o| format!("{} {}", o["id"], o["label"]))
                .collect();
            format!("{} is not on the page: {there:?}", c.place)
        })?;
        if c.kind == "ask" {
            return self
                .call(
                    "understanding.table_answer",
                    json!({ "file": file, "object": { "page": c.page, "id": id }, "question": c.question }),
                )
                .await;
        }
        self.call(
            "understanding.explain_region",
            json!({ "file": file, "page": c.page, "object": id }),
        )
        .await
    }
}

/// Letters and digits in one case, so line ends and hyphens do not matter.
fn fold(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

fn holds(said: &str, expect: &str) -> bool {
    let said = fold(said);
    expect.split('|').any(|e| said.contains(&fold(e)))
}

#[tokio::test]
#[ignore = "needs the real models; a quality check, not a test of the code"]
async fn the_golden_set() {
    let Some(w) = world() else { return };
    let text = std::fs::read_to_string(root().join("fixtures/golden/explain.toml")).unwrap();
    let golden: Golden = toml::from_str(&text).unwrap();
    let only = std::env::var("EP_GOLDEN_KIND").ok();
    let paper = std::env::var("EP_GOLDEN_PAPER").ok();
    let mut score: BTreeMap<&str, (u32, u32, u64)> = BTreeMap::new();
    for c in &golden.case {
        if only.as_ref().is_some_and(|k| *k != c.kind)
            || paper.as_ref().is_some_and(|p| !c.paper.contains(p))
        {
            continue;
        }
        let Some(file) = w.open(&c.paper) else {
            println!("skipped: {} is not there", c.paper);
            continue;
        };
        let name = format!("{} {} p.{} {}", c.kind, c.paper, c.page, c.place);
        let sum = score.entry(c.kind.as_str()).or_default();
        sum.1 += 1;
        let (said, wrong) = match w.answer(c, &file).await {
            Ok(v) => {
                sum.2 += v["timings"]["total_ms"]
                    .as_u64()
                    .or(v["total_ms"].as_u64())
                    .unwrap_or(0);
                let mut said = v["text"].as_str().unwrap_or_default().to_string();
                for x in v["symbols"].as_array().into_iter().flatten() {
                    let meaning = x["meaning"].as_str().unwrap_or("?");
                    let _ = write!(said, " [{}: {meaning}]", x["shown"]);
                }
                if c.kind == "ask" && v["status"] != "computed" {
                    said = format!("none ({})", v["reason"]);
                }
                let wrong: Vec<&String> = c.expect.iter().filter(|e| !holds(&said, e)).collect();
                let how = format!(
                    "{} left out {}",
                    v["status"].as_str().unwrap_or(""),
                    v["left_out"]
                );
                (format!("{how}: {said}"), format!("{wrong:?}"))
            }
            Err(e) => (e, "error".into()),
        };
        let right = wrong == "[]";
        sum.0 += u32::from(right);
        println!(
            "{} {name} {}\n     {said}",
            if right { "ok  " } else { "MISS" },
            if right { "" } else { &wrong }
        );
    }
    let mut short = vec![];
    for (kind, (right, all, ms)) in score {
        let share = f64::from(right) / f64::from(all);
        println!("{kind}: {right}/{all}, {} ms each", ms / u64::from(all));
        if share < golden.least.get(kind).copied().unwrap_or(0.0) {
            short.push(kind);
        }
    }
    assert!(short.is_empty(), "below what was measured: {short:?}");
}

#[tokio::test]
#[ignore = "lists what the fixture papers hold"]
async fn what_is_there() {
    let Some(w) = world() else { return };
    let mut papers: Vec<_> = std::fs::read_dir(root().join(".fixtures/papers"))
        .unwrap()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    papers.sort();
    for p in papers {
        let Some(file) = w.open(&p) else { continue };
        let c = w
            .call("document.contents", json!({ "file": file }))
            .await
            .unwrap();
        for kind in ["figures", "tables", "equations"] {
            for o in c[kind].as_array().unwrap() {
                let text: String = o["text"]
                    .as_str()
                    .unwrap_or_default()
                    .chars()
                    .take(230)
                    .collect();
                println!(
                    "{p} p.{} {} {}: {text}",
                    o["rect"]["page"], o["id"], o["label"]
                );
                if kind == "tables" {
                    let object = json!({ "page": o["rect"]["page"], "id": o["id"] });
                    let table = w
                        .call("document.table", json!({ "file": file, "object": object }))
                        .await;
                    println!("{}", table.map_or_else(|e| e, |t| t["csv"].to_string()));
                }
            }
        }
    }
}
