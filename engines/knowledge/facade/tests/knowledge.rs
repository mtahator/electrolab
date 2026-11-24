//! Digests and asking the library through the bus: real documents (Document, Workspace), recorded
//! model replies (Replay); one live check with a real model when `EP_LLM_DIR` is set.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use ep_bus::{Bus, CommandError, Envelope};
use ep_document::{BlobAccess, Document};
use ep_inference::{Inference, Prepared, Replay};
use ep_knowledge::Knowledge;
use ep_knowledge::contract::{Digest, DigestInfo, DigestStatus, FindResult, Kind, Level, Source};
use ep_types::{BlobRef, Hash, MediaType};
use ep_workspace::Workspace;
use serde_json::{Value, json};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..")
}

struct Blobs(Arc<Workspace>);
impl BlobAccess for Blobs {
    fn path_of(&self, hash: &Hash) -> Option<PathBuf> {
        self.0.blob_path(hash)
    }
    fn put(&self, bytes: &[u8], media_type: MediaType) -> std::io::Result<BlobRef> {
        self.0.store_blob(bytes, media_type)
    }
}

struct Lib {
    bus: Arc<Bus>,
    ids: Vec<String>,
    dir: Arc<tempfile::TempDir>,
    ws: Arc<Workspace>,
    knowledge: Arc<Knowledge>,
}

static PDF: std::sync::OnceLock<Arc<dyn ep_document::Backend>> = std::sync::OnceLock::new();

/// A library with these fixtures; `None` when `PDFium` is missing.
async fn lib(files: &[&str], inference: Arc<Inference>) -> Option<Lib> {
    let natives = root().join(".natives/pdfium/bin");
    if !natives.exists() || files.iter().any(|f| !root().join(f).exists()) {
        eprintln!("skipped: PDFium or a fixture is missing");
        return None;
    }
    PDF.get_or_init(|| ep_document::start_pdfium(&natives).unwrap());
    let dir = Arc::new(tempfile::tempdir().unwrap());
    let ws = Workspace::new(Arc::new(ep_os::RootDirs(dir.path().join("data"))));
    let mut l = engines(dir, ws, &inference, "cache");
    l.ok("workspace.open_default_library", Value::Null).await;
    for f in files {
        let v = l
            .ok("workspace.add_file", json!({ "path": root().join(f) }))
            .await;
        l.ids.push(v["paper"]["id"].as_str().unwrap().to_string());
    }
    Some(l)
}

/// The engines on a library.
fn engines(
    dir: Arc<tempfile::TempDir>,
    ws: Arc<Workspace>,
    inference: &Arc<Inference>,
    cache: &str,
) -> Lib {
    let bus = Arc::new(Bus::default());
    ws.register(&bus).unwrap();
    let pdf = PDF.get().unwrap().clone();
    Document::new(
        Arc::new(Blobs(ws.clone())),
        Some(pdf),
        Some(&dir.path().join(cache)),
    )
    .unwrap()
    .register(&bus)
    .unwrap();
    inference.register(&bus).unwrap();
    let knowledge = Knowledge::new();
    knowledge.register(&bus).unwrap();
    Lib {
        bus,
        ids: vec![],
        dir,
        ws,
        knowledge,
    }
}

impl Lib {
    /// The same library, opened again by new engines.
    fn again(&self, inference: &Arc<Inference>) -> Lib {
        let mut l = engines(self.dir.clone(), self.ws.clone(), inference, "cache-2");
        l.ids.clone_from(&self.ids);
        l
    }
    async fn call(&self, c: &str, i: Value) -> Result<Value, CommandError> {
        self.bus.call(Envelope::new(c, i)).await
    }
    async fn ok(&self, c: &str, i: Value) -> Value {
        self.call(c, i)
            .await
            .unwrap_or_else(|e| panic!("{c}: {e:?}"))
    }
    async fn text(&self, n: usize) -> String {
        let p = self
            .ok("workspace.get_paper", json!({ "id": self.ids[n] }))
            .await;
        let info = self.ok("document.info", json!({ "file": p["file"] })).await;
        let mut all = String::new();
        for page in 1..=info["pages"].as_u64().unwrap() {
            let t = self
                .ok("document.text", json!({ "file": p["file"], "page": page }))
                .await;
            all.push_str(t["text"].as_str().unwrap());
            all.push(' ');
        }
        all.split_whitespace().collect::<Vec<_>>().join(" ")
    }
}

/// The numbered sentences a request shows.
fn sentences(p: &Prepared) -> Vec<String> {
    p.messages[1]
        .content
        .lines()
        .filter_map(|l| {
            l.split_once(". ")
                .filter(|(n, _)| n.parse::<u32>().is_ok())
                .map(|(_, t)| t.to_string())
        })
        .collect()
}

fn card(summary: &str, p: &Prepared) -> String {
    let word = sentences(p)[0]
        .split_whitespace()
        .find(|w| w.len() > 4 && w.chars().all(char::is_alphanumeric))
        .unwrap_or("x")
        .to_string();
    json!({ "summary": summary, "tags": ["Coffee Grinders", "coffee grinder", "a b c d e"], "terms": [word, "not in the text at all"], "role": "instructions", "support": [1, 1] }).to_string()
}

fn paper_card() -> &'static str {
    r#"{"summary":"The burr coffee grinder grinds coffee.","topics":["Coffee Grinders"],"kind":"manual","questions":["How is the grinder cleaned","Why"]}"#
}

const MANUAL: &str = "fixtures/documents/product-manual.pdf";
const REPORT: &str = "fixtures/documents/report-business.pdf";

#[tokio::test]
async fn summaries_are_checked_against_their_section() {
    // First a sentence of the section itself; then words from nowhere; then a wrong number.
    let n = Arc::new(AtomicUsize::new(0));
    let m = n.clone();
    let replay = Replay::new()
        .on_with("digest.section", None, move |p| {
            match m.fetch_add(1, Ordering::SeqCst) {
                1 => card("Penguins migrate across frozen oceans in winter.", p),
                2 => card(
                    &format!("{} It weighs 987654 kilograms.", sentences(p)[0]),
                    p,
                ),
                _ => card(&sentences(p)[0], p),
            }
        })
        .on("digest.paper", paper_card());
    let Some(l) = lib(&[MANUAL], Inference::with_backend(Arc::new(replay.clone()))).await else {
        return;
    };
    let d: Digest = serde_json::from_value(
        l.ok("knowledge.digest_paper", json!({ "paper": l.ids[0] }))
            .await,
    )
    .unwrap();
    assert!(
        d.complete && d.level == Level::Full && d.sections.len() >= 3,
        "{d:?}"
    );
    assert_eq!(d.sections[0].source, Source::Written);
    assert_eq!(
        d.sections[1].source,
        Source::Quoted,
        "words from nowhere are not used"
    );
    assert!(!d.sections[1].summary.contains("Penguins"));
    assert!(
        d.sections.iter().all(|s| !s.summary.contains("987654")),
        "no number from nowhere"
    );
    let asked = replay
        .seen()
        .into_iter()
        .filter(|p| p.messages[1].content.contains("987654"))
        .count();
    assert!(asked >= 1, "asked again with the wrong number named");
    // Quotes and terms stand in the document, tags are clean.
    let text = l.text(0).await;
    let squash = |s: &str| s.split_whitespace().collect::<Vec<_>>().join(" ");
    for s in &d.sections {
        for q in &s.support {
            assert!(text.contains(&squash(&q.text)), "{q:?}");
        }
        assert!(!s.terms.iter().any(|t| t.contains("not in the text")));
        assert!(s.tags.iter().all(|t| t == "coffee grinder"), "{:?}", s.tags);
        assert!(s.pages.0 >= 1 && s.pages.1 <= d.pages);
    }
    assert_eq!(d.topics, ["coffee grinder"]);
    assert_eq!(d.questions, ["How is the grinder cleaned?"]);
    assert!(
        d.objects.iter().any(|o| o.kind == Kind::Table),
        "{:?}",
        d.objects
    );
    // The text of papers is data, never instructions; digests wait for people.
    for p in replay.seen() {
        assert!(p.messages[1].content.contains("--- DATA"));
    }
    // Kept with the paper, as a file.
    let kept = walk(l.dir.path())
        .into_iter()
        .find(|p| p.ends_with(format!("digests/{}.json", l.ids[0])));
    assert!(
        std::fs::read_to_string(kept.expect("a file"))
            .unwrap()
            .contains("\"sections\"")
    );
}

fn walk(dir: &Path) -> Vec<PathBuf> {
    let mut out = vec![];
    for e in std::fs::read_dir(dir).unwrap().flatten() {
        if e.path().is_dir() {
            out.extend(walk(&e.path()));
        } else {
            out.push(e.path());
        }
    }
    out
}

#[tokio::test]
async fn a_digest_goes_on_where_it_stopped() {
    let n = Arc::new(AtomicUsize::new(0));
    let m = n.clone();
    // The third section's answer fails as a dead runtime would (no recording for its task).
    let replay = Replay::new()
        .on_with("digest.section", None, move |p| {
            if m.fetch_add(1, Ordering::SeqCst) == 2 {
                "{}".into()
            } else {
                card(&sentences(p)[0], p)
            }
        })
        .on("digest.paper", paper_card());
    let Some(l) = lib(&[MANUAL], Inference::with_backend(Arc::new(replay.clone()))).await else {
        return;
    };
    let d: Digest = serde_json::from_value(
        l.ok("knowledge.digest_paper", json!({ "paper": l.ids[0] }))
            .await,
    )
    .unwrap();
    let none = d
        .sections
        .iter()
        .filter(|s| s.source == Source::None)
        .count();
    assert!(
        d.complete && none <= 1,
        "one unreadable answer does not lose the others"
    );
    let before = replay.seen().len();
    l.ok("knowledge.digest_paper", json!({ "paper": l.ids[0] }))
        .await;
    assert_eq!(
        replay.seen().len(),
        before,
        "a finished digest is not made again"
    );
    l.ok(
        "knowledge.digest_paper",
        json!({ "paper": l.ids[0], "force": true }),
    )
    .await;
    assert!(replay.seen().len() > before);
}

#[tokio::test]
async fn the_library_is_digested_in_the_background_and_can_be_stopped() {
    let slow = Replay::new()
        .on_with("digest.section", None, |p| {
            card(&format!("{} {}", sentences(p)[0], "word ".repeat(3000)), p)
        })
        .on("digest.paper", paper_card());
    let Some(l) = lib(&[MANUAL, REPORT], Inference::with_backend(Arc::new(slow))).await else {
        return;
    };
    let status = async || -> DigestStatus {
        serde_json::from_value(l.ok("knowledge.digest_status", Value::Null).await).unwrap()
    };
    l.ok("knowledge.digest_library", json!({ "full": true }))
        .await;
    let infos = async || -> Vec<DigestInfo> {
        serde_json::from_value(l.ok("knowledge.digests", json!({})).await).unwrap()
    };
    // The basic ones come first, for every paper.
    for _ in 0..400 {
        if infos().await.iter().all(|i| i.level.is_some()) {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    }
    assert!(
        infos().await.iter().all(|i| i.level.is_some()),
        "{:?}",
        infos().await
    );
    l.ok("knowledge.digest_stop", Value::Null).await;
    for _ in 0..200 {
        if !status().await.running {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    }
    let s = status().await;
    assert!(!s.running && s.error.is_none(), "{s:?}");
}

#[tokio::test]
async fn papers_asked_for_come_next_and_only_they_are_written() {
    let replay = Replay::new()
        .on_with("digest.section", None, |p| card(&sentences(p)[0], p))
        .on("digest.paper", paper_card());
    let Some(l) = lib(&[MANUAL, REPORT], Inference::with_backend(Arc::new(replay))).await else {
        return;
    };
    let mut sub = l.bus.events().subscribe("knowledge.digested");
    l.ok(
        "knowledge.digest_library",
        json!({ "full": false, "papers": [l.ids[1]] }),
    )
    .await;
    for _ in 0..800 {
        let s: DigestStatus =
            serde_json::from_value(l.ok("knowledge.digest_status", Value::Null).await).unwrap();
        if !s.running {
            assert!(s.next.is_empty() && s.error.is_none(), "{s:?}");
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    }
    let infos: Vec<DigestInfo> =
        serde_json::from_value(l.ok("knowledge.digests", json!({})).await).unwrap();
    let level = |id: &str| infos.iter().find(|i| i.paper == id).unwrap().level;
    assert_eq!(level(&l.ids[1]), Some(Level::Full));
    assert_eq!(level(&l.ids[0]), Some(Level::Basic));
    // It could be read while it grew.
    let mut grew = 0;
    while let Ok(ep_bus::Recv::Event(e)) =
        tokio::time::timeout(std::time::Duration::from_millis(50), sub.recv()).await
    {
        grew += u32::from(e.data["paper"] == json!(l.ids[1]) && e.data["level"] == "full");
    }
    assert!(grew >= 2, "{grew}");
}

/// Wait until making digests has ended.
async fn ended(l: &Lib) -> DigestStatus {
    for _ in 0..1200 {
        let s: DigestStatus =
            serde_json::from_value(l.ok("knowledge.digest_status", Value::Null).await).unwrap();
        if !s.running {
            return s;
        }
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    }
    panic!("still running")
}

async fn infos(l: &Lib) -> Vec<DigestInfo> {
    serde_json::from_value(l.ok("knowledge.digests", json!({})).await).unwrap()
}

#[tokio::test]
async fn every_paper_is_understood_by_itself_and_nothing_is_made_twice() {
    let replay = Replay::new()
        .on_with("digest.section", None, |p| card(&sentences(p)[0], p))
        .on("digest.paper", paper_card());
    let Some(l) = lib(
        &[MANUAL, REPORT],
        Inference::with_backend(Arc::new(replay.clone())),
    )
    .await
    else {
        return;
    };
    let auto = json!({ "full": true, "auto": true });
    l.ok("knowledge.digest_library", auto.clone()).await;
    let s = ended(&l).await;
    assert_eq!((s.done, s.total, s.error), (2, 2, None));
    assert!(infos(&l).await.iter().all(|i| i.ready && i.complete));
    let asked = replay.seen().len();
    l.ok("knowledge.digest_library", auto).await;
    ended(&l).await;
    assert_eq!(replay.seen().len(), asked, "it is ready already");
}

#[tokio::test]
async fn a_paper_closed_too_early_is_gone_on_with_the_next_time() {
    let slow = Replay::new()
        .on_with("digest.section", None, |p| {
            card(&format!("{} {}", sentences(p)[0], "word ".repeat(1500)), p)
        })
        .on("digest.paper", paper_card());
    let Some(l) = lib(&[MANUAL], Inference::with_backend(Arc::new(slow.clone()))).await else {
        return;
    };
    let mut sub = l.bus.events().subscribe("knowledge.digested");
    l.ok(
        "knowledge.digest_library",
        json!({ "full": false, "papers": [l.ids[0]], "auto": true }),
    )
    .await;
    // Closed after the first section was read.
    while let Ok(ep_bus::Recv::Event(e)) =
        tokio::time::timeout(std::time::Duration::from_secs(20), sub.recv()).await
    {
        if e.data["level"] == "full" {
            break;
        }
    }
    l.ok("knowledge.digest_stop", Value::Null).await;
    ended(&l).await;
    let i = infos(&l).await;
    assert!(!i[0].ready && i[0].level == Some(Level::Full), "{i:?}");
    let sections = slow
        .seen()
        .iter()
        .filter(|p| p.task == "digest.section")
        .count();
    // The person stopped it: nothing starts by itself until they say so.
    let s: DigestStatus = serde_json::from_value(
        l.ok(
            "knowledge.digest_library",
            json!({ "full": true, "auto": true }),
        )
        .await,
    )
    .unwrap();
    assert!(!s.running && s.stopped);
    // The next time the app starts (a new engine on the same library), it goes on by itself.
    let quick = Replay::new()
        .on_with("digest.section", None, |p| card(&sentences(p)[0], p))
        .on("digest.paper", paper_card());
    let again = l.again(&Inference::with_backend(Arc::new(quick.clone())));
    again
        .ok(
            "knowledge.digest_library",
            json!({ "full": true, "auto": true }),
        )
        .await;
    ended(&again).await;
    assert!(infos(&again).await[0].ready);
    let rest = quick
        .seen()
        .iter()
        .filter(|p| p.task == "digest.section")
        .count();
    let all = {
        let d: Digest = serde_json::from_value(
            again
                .ok("knowledge.get_digest", json!({ "paper": l.ids[0] }))
                .await,
        )
        .unwrap();
        d.sections.len()
    };
    assert!(
        rest < all,
        "what was made is not made again: {sections} + {rest} of {all}"
    );
}

#[tokio::test]
async fn on_battery_only_what_a_person_asks_is_made() {
    let replay = Replay::new()
        .on_with("digest.section", None, |p| card(&sentences(p)[0], p))
        .on("digest.paper", paper_card());
    let Some(l) = lib(&[MANUAL, REPORT], Inference::with_backend(Arc::new(replay))).await else {
        return;
    };
    let machine = Arc::new(ep_os::fake::FakeMachine::default());
    *machine.battery.lock() = Some(true);
    l.knowledge.use_machine(machine.clone());
    l.ok(
        "knowledge.digest_library",
        json!({ "full": true, "auto": true }),
    )
    .await;
    let status = async || -> DigestStatus {
        serde_json::from_value(l.ok("knowledge.digest_status", Value::Null).await).unwrap()
    };
    for _ in 0..200 {
        if status().await.waiting.is_some() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    }
    assert_eq!(
        status().await.waiting,
        Some(ep_knowledge::contract::Waiting::Battery)
    );
    assert!(
        infos(&l)
            .await
            .iter()
            .all(|i| i.level == Some(Level::Basic)),
        "words can be found"
    );
    // Asked for: made at once.
    l.ok(
        "knowledge.digest_library",
        json!({ "full": false, "papers": [l.ids[1]] }),
    )
    .await;
    for _ in 0..600 {
        if infos(&l).await.iter().any(|i| i.ready) {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    }
    let i = infos(&l).await;
    assert!(i.iter().all(|i| i.ready == (i.paper == l.ids[1])), "{i:?}");
    // Plugged in: the rest.
    *machine.battery.lock() = Some(false);
    ended(&l).await;
    assert!(infos(&l).await.iter().all(|i| i.ready));
}

#[tokio::test]
async fn without_a_model_it_says_so_and_goes_on_when_there_is_one() {
    let Some(l) = lib(
        &[MANUAL],
        Inference::new(
            vec![],
            vec![],
            Arc::new(ep_os::fake::FakeSpawner::default()),
            None,
            None,
        ),
    )
    .await
    else {
        return;
    };
    l.ok(
        "knowledge.digest_library",
        json!({ "full": true, "auto": true }),
    )
    .await;
    let s = ended(&l).await;
    assert!(
        matches!(s.error.as_deref(), Some("no_model" | "no_runtime")),
        "{s:?}"
    );
    let i = infos(&l).await;
    assert!(
        !i[0].ready && i[0].problem.is_none() && i[0].level == Some(Level::Basic),
        "{i:?}"
    );
}

#[tokio::test]
async fn asking_finds_the_paper_and_the_place() {
    let replay = Replay::new()
        .on(
            "find.understand",
            r#"{"looking_for":"table","things":[{"thing":"motor power","names":["watt"]},{"thing":"technical data","names":[]}]}"#,
        )
        .on(
            "find.papers",
            r#"{"papers":[{"n":1,"why":"its technical data","fit":"yes"},{"n":1,"why":"again","fit":"maybe"}]}"#,
        )
        .on_with("find.nearest", None, |p| {
            // The second and the third place are it.
            let keys = p.schema.as_ref().unwrap()["properties"]
                .as_object()
                .unwrap()
                .clone();
            let v: serde_json::Map<String, Value> = keys
                .keys()
                .map(|k| (k.clone(), json!(if k == "2" || k == "3" { 3 } else { 0 })))
                .collect();
            Value::Object(v).to_string()
        });
    let Some(l) = lib(
        &[REPORT, MANUAL],
        Inference::with_backend(Arc::new(replay.clone())),
    )
    .await
    else {
        return;
    };
    l.ok("knowledge.digest_library", json!({ "full": false }))
        .await;
    for _ in 0..400 {
        let s: DigestStatus =
            serde_json::from_value(l.ok("knowledge.digest_status", Value::Null).await).unwrap();
        if !s.running {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    }
    let mut sub = l.bus.events().subscribe("knowledge.finding");
    let r: FindResult = serde_json::from_value(l.ok("knowledge.find", json!({ "question": "table with the motor power of the coffee grinder", "stream": { "id": "f" } })).await).unwrap();
    assert_eq!(r.looking_for, Kind::Table);
    assert_eq!(r.related.len(), 1, "{:?}", r.related);
    assert_eq!(
        (r.related[0].paper.as_str(), r.related[0].why.as_deref()),
        (l.ids[1].as_str(), Some("its technical data"))
    );
    assert!(
        r.places.len() >= 2 && r.places[0].nearest && r.places[1].nearest,
        "{:?}",
        r.places
    );
    assert_eq!(r.places[0].paper, l.ids[1]);
    assert_eq!((r.digests.basic, r.digests.total), (2, 2));
    // Every quote stands in its paper, word for word.
    let texts = [l.text(0).await, l.text(1).await];
    let fold = |s: &str| {
        s.chars()
            .filter(|c| c.is_alphanumeric())
            .collect::<String>()
    };
    for p in &r.places {
        let n = l.ids.iter().position(|i| *i == p.paper).unwrap();
        assert!(fold(&texts[n]).contains(&fold(&p.quote)), "{p:?}");
    }
    // Something is seen at once, step by step.
    let mut steps = vec![];
    while let Ok(ep_bus::Recv::Event(e)) =
        tokio::time::timeout(std::time::Duration::from_millis(50), sub.recv()).await
    {
        if steps.is_empty() {
            let first: FindResult = serde_json::from_value(e.data["result"].clone()).unwrap();
            assert!(
                !first.used_model && !first.places.is_empty(),
                "words at once: {first:?}"
            );
        }
        steps.push(e.data["step"].as_str().unwrap().to_string());
    }
    assert_eq!(
        (
            steps[0].as_str(),
            steps[1].as_str(),
            steps.last().unwrap().as_str()
        ),
        ("understanding", "related", "done")
    );
    for p in replay.seen().iter().filter(|p| p.task != "find.understand") {
        assert!(
            p.messages[1].content.contains("--- DATA"),
            "titles and quotes are data"
        );
    }
}

#[tokio::test]
async fn finding_works_without_a_model() {
    let Some(l) = lib(
        &[REPORT, MANUAL],
        Inference::with_backend(Arc::new(Replay::new())),
    )
    .await
    else {
        return;
    };
    let r: FindResult = serde_json::from_value(
        l.ok(
            "knowledge.find",
            json!({ "question": "Where is the hopper of the grinder?" }),
        )
        .await,
    )
    .unwrap();
    assert!(!r.used_model && r.model.is_none());
    assert!(
        r.places.iter().any(|p| p.paper == l.ids[1]),
        "{:?}",
        r.places
    );
    assert!(r.places.iter().all(|p| !p.nearest));
    let e = l
        .call("knowledge.find", json!({ "question": "the and of" }))
        .await
        .unwrap_err();
    assert!(
        matches!(&e, CommandError::Failed { kind, .. } if kind == "nothing_to_find"),
        "{e:?}"
    );
}

/// With a real model: the digests of a paper hold, and the example of the founder finds its figure.
#[tokio::test]
#[allow(clippy::too_many_lines)] // one long look at the real model
async fn live_digest_and_find() {
    let (Some(llm), Some(pdf)) = (
        std::env::var_os("EP_LLM_DIR"),
        std::env::var_os("EP_FIND_PDF"),
    ) else {
        eprintln!("skipped: EP_LLM_DIR and EP_FIND_PDF not set");
        return;
    };
    let spawner: Arc<dyn ep_os::ProcessSpawner> = Arc::new(std_spawner::StdSpawner);
    let engine = Inference::new(
        vec![llm.into()],
        vec![root().join(".natives")],
        spawner,
        None,
        None,
    );
    let own = PathBuf::from(pdf);
    let Some(l) = lib(
        &[
            own.to_str().unwrap(),
            MANUAL,
            ".fixtures/papers/arxiv-2111.12066v2.pdf",
        ],
        engine,
    )
    .await
    else {
        return;
    };
    let t = std::time::Instant::now();
    // Made once, kept between runs while the search is worked on.
    let keep = std::env::var_os("EP_DIGEST_JSON").map(PathBuf::from);
    if let Some(text) = keep.as_ref().and_then(|k| std::fs::read_to_string(k).ok()) {
        let mut v: Value = serde_json::from_str(&text).unwrap();
        v["paper"] = json!(l.ids[0]);
        l.ok(
            "workspace.save_digest",
            json!({ "paper": l.ids[0], "data": v }),
        )
        .await;
    }
    let d: Digest = serde_json::from_value(
        l.ok("knowledge.digest_paper", json!({ "paper": l.ids[0] }))
            .await,
    )
    .unwrap();
    if let Some(k) = keep {
        std::fs::write(k, serde_json::to_string_pretty(&d).unwrap()).unwrap();
    }
    let written = d
        .sections
        .iter()
        .filter(|s| s.source == Source::Written)
        .count();
    let quoted = d
        .sections
        .iter()
        .filter(|s| s.source == Source::Quoted)
        .count();
    eprintln!(
        "digest: {:.0} s, {} sections, {written} written, {quoted} quoted\nkind {} topics {:?}\n{}\nquestions {:?}",
        t.elapsed().as_secs_f32(),
        d.sections.len(),
        d.kind,
        d.topics,
        d.summary,
        d.questions
    );
    for s in &d.sections {
        eprintln!(
            "  [{:?} p.{}-{}] {} — {} | {:?} | {:?}",
            s.role,
            s.pages.0,
            s.pages.1,
            s.title,
            s.summary.chars().take(150).collect::<String>(),
            s.tags,
            s.terms
        );
    }
    assert!(written * 10 >= (written + quoted) * 7, "most cards hold");
    l.ok("knowledge.digest_library", json!({ "full": false }))
        .await;
    tokio::time::sleep(std::time::Duration::from_secs(6)).await;
    for q in [
        "chart of carbon saving against NPV for the different technologies",
        "نمودار سیو کربن و npv برای تکنولوژی های مختلف",
    ] {
        let r: FindResult =
            serde_json::from_value(l.ok("knowledge.find", json!({ "question": q })).await).unwrap();
        eprintln!(
            "\n{q}\n  {} ms, for {:?}, terms {:?}\n  related {:?}",
            r.total_ms, r.looking_for, r.terms, r.related
        );
        for p in r.places.iter().take(6) {
            eprintln!(
                "  {} p.{} {:?} {} | {}",
                if p.nearest { "*" } else { " " },
                p.page,
                p.label,
                p.paper == l.ids[0],
                p.quote.chars().take(110).collect::<String>()
            );
        }
        assert_eq!(r.related[0].paper, l.ids[0]);
        assert_eq!(r.places[0].paper, l.ids[0]);
        assert!(r.related[0].why.is_some(), "the model says why");
        let near: Vec<&str> = r
            .places
            .iter()
            .take(4)
            .filter_map(|p| p.label.as_deref())
            .collect();
        assert!(
            near.contains(&"Figure 6") || near.contains(&"Figure 11"),
            "{near:?}"
        );
    }
}

mod std_spawner {
    /// Starts a real process for the live check; it ends when the test drops it.
    pub struct StdSpawner;
    struct StdChild(std::process::Child);
    impl Drop for StdChild {
        fn drop(&mut self) {
            let _ = self.0.kill();
        }
    }
    impl ep_os::ChildProcess for StdChild {
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
    impl ep_os::ProcessSpawner for StdSpawner {
        fn spawn(&self, spec: &ep_os::ProcessSpec) -> ep_os::Result<Box<dyn ep_os::ChildProcess>> {
            let child = std::process::Command::new(&spec.program)
                .args(&spec.args)
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .spawn()?;
            Ok(Box::new(StdChild(child)))
        }
    }
}
