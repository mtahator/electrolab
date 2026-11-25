//! Objects on the page through the bus (P1.M5.UND.3–6, 8). The paper is a small written one behind
//! the Document engine's command names and the model is a recording, so these always run.
#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use std::sync::Arc;
use std::time::Duration;

use ep_bus::{Bus, CommandError, Envelope, Permission, Recv, Spec, Stability, Subscription};
use ep_inference::{Inference, Replay};
use ep_types::{BlobRef, Hash, MediaType};
use ep_understanding::Understanding;
use ep_understanding::contract::{
    Basis, CellRole, ExplainStatus, ExplainedKind, FiguresSeen, ObjectExplanation, TableAnswer,
    TableReason, TableStatus, UnitsResult,
};
use parking_lot::Mutex;
use serde_json::{Value, json};

/// Page 1: a paragraph, Figure 1 with its caption, the equation. Page 2: where they are mentioned.
const PAGE_1: &str = "The flow was studied in a duct of 40 mm.\n\
Re Nu inlet outlet\n\
Figure 1. Nusselt number against Reynolds number.\n\
The heat flow is written as\n\
Q = m c dT\n\
where Q is the heat flow (W), m is the mass flow (kg/s), c denotes the specific heat (J/(kg K)) and dT is the temperature rise (K).";
const PAGE_2: &str = "As Fig. 1 shows, the Nusselt number grows with 0.8 power at the NVC.\n\
Fig. 12 is another matter.\n\
In Table 1 the model of this paper has the lowest error of 0.31 K.\n\
The heat flow of Eq. (1) is used in every run. Here the symbol w stands for the wetted perimeter.\n\
The wall has a non-stationary vortex core (NVC) behind it.";

const STEP: f32 = 5.0;

/// Boxes: characters 5 wide, lines 10 high.
fn text(page: u32, t: &str) -> Value {
    let (mut x, mut y) = (0.0, 0.0);
    let boxes: Vec<Value> = t
        .chars()
        .map(|c| {
            if c == '\n' {
                (x, y) = (0.0, y + 10.0);
                return Value::Null;
            }
            x += STEP;
            json!([x - STEP, y, x, y + 10.0])
        })
        .collect();
    json!({ "page": page, "text": t, "boxes": boxes, "words": [], "lines": [], "unknown": 0 })
}

fn line(page: u32, n: u8, lines: u8) -> Value {
    let y = f32::from(n) * 10.0;
    json!({ "page": page, "x0": 0.0, "y0": y, "x1": 2000.0, "y1": y + f32::from(lines) * 10.0 })
}

fn object(id: &str, kind: &str, rect: &Value, more: &Value) -> Value {
    let mut o = json!({ "id": id, "kind": kind, "rect": rect, "score": 1.0, "order": 0 });
    for (k, v) in more.as_object().unwrap() {
        o[k] = v.clone();
    }
    o
}

fn figure() -> Value {
    object(
        "p1-figure-1",
        "figure",
        &line(1, 1, 1),
        &json!({ "label": "Figure 1", "number": "1", "caption_rect": line(1, 2, 1),
            "text": "Figure 1. Nusselt number against Reynolds number." }),
    )
}

fn objects(page: u32) -> Value {
    let mut all = vec![];
    if page == 1 {
        all.push(object("p1-text-1", "text", &line(1, 0, 1), &json!({})));
        all.push(figure());
        all.push(object(
            "p1-caption-1",
            "caption",
            &line(1, 2, 1),
            &json!({}),
        ));
        all.push(object(
            "p1-equation-1",
            "equation",
            &line(1, 4, 1),
            &json!({ "label": "(1)", "number": "1" }),
        ));
        // Nothing in it, no caption, no number.
        all.push(object("p1-figure-9", "figure", &line(1, 90, 1), &json!({})));
        all.push(object(
            "p1-table-1",
            "table",
            &line(1, 91, 1),
            &json!({ "label": "Table 1", "number": "1", "text": "Table 1. Errors of the models." }),
        ));
    }
    json!({ "page": page, "objects": all, "detected": true })
}

fn table() -> Value {
    let cell = |t: &str| json!({ "text": t, "rect": line(1, 91, 1) });
    let row = |r: [&str; 2]| r.iter().map(|t| cell(t)).collect::<Vec<_>>();
    json!({
        "object": objects(1)["objects"][5], "page": 1, "header_rows": 1, "csv": "",
        "units": [null, "K"],
        "rows": [row(["Model", "RMSE (K)"]), row(["LSTM", "0.52"]), row(["PINN", "0.31"]), row(["Mean", "n/a"])],
    })
}

/// Every place of a page that has one of the terms.
fn search(input: &Value) -> Value {
    let mut hits = vec![];
    for (page, t) in [(1, PAGE_1), (2, PAGE_2)] {
        for (n, term) in input["terms"].as_array().unwrap().iter().enumerate() {
            let term = term["text"].as_str().unwrap();
            for (at, _) in t.match_indices(term) {
                let y = u8::try_from(t[..at].matches('\n').count()).unwrap();
                let x = f32::from(
                    u16::try_from(at - t[..at].rfind('\n').map_or(0, |i| i + 1)).unwrap(),
                );
                let mut r = line(page, y, 1);
                (r["x0"], r["x1"]) = (json!(x * STEP), json!((x + 4.0) * STEP));
                hits.push(json!({ "term": n, "label": null, "paper": "", "page": page, "matched": term,
                    "before": t[..at], "after": t[at + term.len()..], "kind": "exact", "similarity": 1.0,
                    "rects": [r], "section": null, "char_range": [0, 0], "merged": 0 }));
            }
        }
    }
    json!({ "hits": hits, "counts": [], "truncated": false, "elapsed_us": 0 })
}

type Kept = Arc<Mutex<Vec<Value>>>;

struct World {
    bus: Arc<Bus>,
    replay: Replay,
    /// What the workspace keeps.
    kept: Kept,
}

fn on(
    bus: &Arc<Bus>,
    name: &'static str,
    f: impl Fn(Value) -> Result<Value, CommandError> + Send + Sync + 'static,
) {
    let f = Arc::new(f);
    bus.register(
        Spec {
            name,
            summary: "",
            stability: Stability::Experimental,
            permission: Permission::read(),
        },
        move |_cx, i: Value| {
            let f = f.clone();
            async move { f(i) }
        },
    )
    .unwrap();
}

fn failed(kind: &str) -> CommandError {
    CommandError::Failed {
        kind: kind.into(),
        message: String::new(),
    }
}

fn file() -> BlobRef {
    BlobRef {
        hash: Hash::of(b"paper"),
        size: 5,
        media_type: MediaType::pdf(),
    }
}

/// The paper; the equation is read as `latex`.
fn paper(latex: Option<&str>) -> (Arc<Bus>, Kept) {
    let bus = Arc::new(Bus::default());
    on(&bus, "document.text", |i| match i["page"].as_u64() {
        Some(1) => Ok(text(1, PAGE_1)),
        Some(2) => Ok(text(2, PAGE_2)),
        _ => Err(failed("bad_page")),
    });
    on(&bus, "document.objects", |i| {
        Ok(objects(u32::try_from(i["page"].as_u64().unwrap()).unwrap()))
    });
    on(&bus, "document.search", |i| Ok(search(&i)));
    on(&bus, "document.contents", |_| {
        Ok(
            json!({ "sections": [], "figures": [figure(), objects(1)["objects"][4]], "tables": [], "equations": [] }),
        )
    });
    on(&bus, "document.table", |i| {
        if i["object"]["id"] == "p1-table-1" {
            Ok(table())
        } else {
            Err(failed("no_object"))
        }
    });
    let image = json!({ "hash": Hash::of(b"picture"), "size": 7, "media_type": "image/png" });
    let i2 = image.clone();
    on(&bus, "document.region", move |i| {
        Ok(json!({ "rect": i["rect"], "image": image, "text": "" }))
    });
    let latex = latex.map(String::from);
    on(&bus, "document.equation", move |_| {
        Ok(
            json!({ "object": objects(1)["objects"][3], "page": 1, "latex": latex, "image": i2, "text": "Q = m c dT" }),
        )
    });
    let kept: Kept = Arc::default();
    let k = kept.clone();
    on(&bus, "workspace.get_figures", move |i| {
        Ok(k.lock()
            .iter()
            .filter(|v| v["paper"] == i["papers"][0])
            .cloned()
            .collect())
    });
    let k = kept.clone();
    on(&bus, "workspace.save_figures", move |i| {
        k.lock().retain(|v| v["paper"] != i["paper"]);
        k.lock().push(i);
        Ok(Value::Null)
    });
    (bus, kept)
}

fn world(latex: Option<&str>, replay: Replay) -> World {
    let (bus, kept) = paper(latex);
    Inference::with_backend(Arc::new(replay.clone()))
        .register(&bus)
        .unwrap();
    Understanding::new().register(&bus).unwrap();
    World { bus, replay, kept }
}

impl World {
    async fn call(&self, name: &str, more: Value) -> Result<Value, CommandError> {
        let mut input = json!({ "file": file() });
        for (k, v) in more.as_object().unwrap() {
            input[k] = v.clone();
        }
        self.bus.call(Envelope::new(name, input)).await
    }

    async fn explain(&self, object: &str) -> ObjectExplanation {
        self.explain_with(object, json!({})).await
    }

    async fn explain_with(&self, object: &str, more: Value) -> ObjectExplanation {
        let mut input = json!({ "page": 1, "object": object, "stream": { "id": "x" } });
        for (k, v) in more.as_object().unwrap() {
            input[k] = v.clone();
        }
        let v = self.call("understanding.explain_region", input).await;
        serde_json::from_value(v.unwrap_or_else(|e| panic!("{e:?}"))).unwrap()
    }

    /// Everything the model was sent for a task.
    fn sent(&self, task: &str) -> String {
        let seen = self.replay.seen();
        let p = seen.iter().find(|p| p.task == task).expect(task);
        p.messages
            .iter()
            .map(|m| m.content.as_str())
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn asked(&self, task: &str) -> bool {
        self.replay.seen().iter().any(|p| p.task == task)
    }
}

fn kind(e: CommandError) -> String {
    match e {
        CommandError::Failed { kind, .. } => kind,
        e => format!("{e:?}"),
    }
}

async fn events(sub: &mut Subscription) -> Vec<Value> {
    let mut out = vec![];
    while let Ok(Recv::Event(e)) = tokio::time::timeout(Duration::from_millis(60), sub.recv()).await
    {
        out.push(e.data);
    }
    out
}

/// Every quote stands on its page.
fn verbatim(r: &ObjectExplanation) {
    let squash = |s: &str| s.split_whitespace().collect::<Vec<_>>().join(" ");
    for e in &r.evidence {
        let page = if e.page == 1 { PAGE_1 } else { PAGE_2 };
        assert!(squash(page).contains(&e.quote), "{e:?}");
    }
}

#[tokio::test]
async fn a_figure_is_explained_from_what_the_paper_says() {
    let w = world(
        None,
        Replay::new()
            .on(
                "explain.figure",
                " the Nusselt number, which grows with Re to the power of 0.8.",
            )
            .on("explain.figure.check", "OK"),
    );
    let mut sub = w.bus.events().subscribe("understanding.explaining");
    let r = w.explain("p1-figure-1").await;
    assert_eq!(
        (r.kind, r.status),
        (ExplainedKind::Figure, ExplainStatus::Checked)
    );
    assert!(
        r.text.starts_with("Figure 1 shows the Nusselt number"),
        "{}",
        r.text
    );
    assert_eq!(
        r.based_on,
        [Basis::Caption, Basis::Mentions, Basis::Words],
        "the picture was not looked at"
    );
    assert_eq!((r.left_out, r.seen.is_none()), (0, true));
    let said = w.sent("explain.figure");
    assert!(said.contains("As Fig. 1 shows") && said.contains("Re Nu inlet outlet"));
    assert!(!said.contains("Places of the paper that mention it: \n[1] Figure 1."));
    assert_eq!(r.evidence.len(), 2, "caption and mention: {:?}", r.evidence);
    assert_eq!((r.evidence[0].y, r.evidence[1].page), (20.0, 2));
    verbatim(&r);
    let phases: Vec<Value> = events(&mut sub)
        .await
        .iter()
        .map(|e| e["phase"].clone())
        .collect();
    assert_eq!(phases.first(), Some(&json!("draft")), "it streams");
    assert_eq!(phases.last(), Some(&json!("checking")));

    // Its caption and its place mean the figure; the paper's words are never instructions.
    let c = w.explain("p1-caption-1").await;
    assert_eq!(c.object.as_deref(), Some("p1-figure-1"));
    for p in w.replay.seen() {
        assert!(!p.messages[0].content.contains("Nusselt"));
        assert!(p.messages[1].content.contains("--- DATA"));
    }
}

#[tokio::test]
async fn a_number_from_nowhere_is_left_out() {
    let w = world(
        None,
        Replay::new()
            .on(
                "explain.figure",
                " the Nusselt number. It grows with the 0.8 power. It is 47 % better.",
            )
            .on("explain.figure.check", "OK"),
    );
    let mut sub = w.bus.events().subscribe("understanding.explaining");
    let r = w.explain("p1-figure-1").await;
    assert_eq!(
        r.text,
        "Figure 1 shows the Nusselt number. It grows with the 0.8 power."
    );
    assert_eq!((r.left_out, r.status), (1, ExplainStatus::Corrected));
    let last = events(&mut sub).await.pop().unwrap();
    assert_eq!(
        (&last["phase"], &last["text"]),
        (&json!("check"), &json!(r.text))
    );
}

#[tokio::test]
async fn abbreviations_stand_for_what_the_paper_says() {
    let w = world(
        None,
        Replay::new()
            .on(
                "explain.figure",
                " the wake (NVC). The nasal vent code (NVC) falls. NVC grows with Re.",
            )
            .on("explain.figure.check", "OK"),
    );
    let r = w.explain("p1-figure-1").await;
    assert!(
        w.sent("explain.figure")
            .contains("What its abbreviations stand for: NVC: non-stationary vortex core")
    );
    assert_eq!(
        r.text,
        "The non-stationary vortex core (NVC) falls. NVC grows with Re."
    );
    assert_eq!((r.left_out, r.status), (1, ExplainStatus::Corrected));
}

#[tokio::test]
async fn formulas_are_shown_as_letters() {
    let w = world(
        None,
        Replay::new()
            .on(
                "explain.figure",
                r" the Nusselt number $ \mathrm{Nu}_{x} $ against \( \lambda_i \).",
            )
            .on("explain.figure.check", "OK"),
    );
    let mut sub = w.bus.events().subscribe("understanding.explaining");
    let r = w.explain("p1-figure-1").await;
    assert_eq!(
        r.text,
        "Figure 1 shows the Nusselt number Nu_x against λ_i."
    );
    for e in events(&mut sub).await {
        let t = e["text"].as_str().unwrap();
        assert!(!t.contains('$') && !t.contains('\\'), "{t}");
    }
}

#[tokio::test]
async fn the_check_corrects_and_a_vague_check_leaves_it_unchecked() {
    let right = "Figure 1 shows how the Nusselt number grows with the Reynolds number.";
    let w = world(
        None,
        Replay::new()
            .on("explain.figure", " a cat.")
            .on("explain.figure.check", right),
    );
    let r = w.explain("p1-figure-1").await;
    assert_eq!(
        (r.status, r.text.as_str()),
        (ExplainStatus::Corrected, right)
    );
    let w = world(
        None,
        Replay::new()
            .on("explain.figure", " a thing. There are 99 of them.")
            .on("explain.figure.check", "Hm."),
    );
    let r = w.explain("p1-figure-1").await;
    assert_eq!((r.status, r.left_out), (ExplainStatus::Unchecked, 1));
    assert_eq!(r.text, "Figure 1 shows a thing.");

    // A single remark about a longer explanation is none.
    let w = world(
        None,
        Replay::new()
            .on("explain.figure", " a thing. It grows. It is for pipes.")
            .on(
                "explain.figure.check",
                "The second of the two is for tubes and not for pipes.",
            ),
    );
    let r = w.explain("p1-figure-1").await;
    assert_eq!(r.status, ExplainStatus::Unchecked);
    assert!(r.text.ends_with("It is for pipes."), "{}", r.text);
}

#[tokio::test]
async fn nothing_is_made_up_where_the_paper_says_nothing() {
    // No recording: asking the model would fail.
    let w = world(None, Replay::new());
    let r = w.explain("p1-figure-9").await;
    assert_eq!((r.text.as_str(), r.based_on.len()), ("", 0));
    for (cmd, input) in [
        (
            "explain_region",
            json!({ "page": 1, "object": "p1-gone-1" }),
        ),
        ("explain_region", json!({ "page": 1 })),
        (
            "explain_region",
            json!({ "page": 1, "rect": line(1, 60, 1) }),
        ),
        (
            "explain_figure",
            json!({ "page": 1, "object": "p1-table-1" }),
        ),
        (
            "explain_equation",
            json!({ "page": 1, "object": "p1-figure-1" }),
        ),
    ] {
        let e = w
            .call(&format!("understanding.{cmd}"), input.clone())
            .await
            .unwrap_err();
        assert_eq!(kind(e), "no_object", "{cmd} {input}");
    }
}

#[tokio::test]
async fn what_was_seen_in_the_picture_is_used_when_it_is_this_figure() {
    let replay = || {
        Replay::new()
            .on("explain.figure", " a rising curve.")
            .on("explain.figure.check", "OK")
    };
    let keep = |rect: Value, hash: Hash| {
        json!({ "paper": "P", "data": { "file": hash, "figures": [{ "rect": rect,
            "object": { "page": 1, "id": "p1-figure-1" }, "text": "A rising curve.", "model": "eyes", "at_ms": 1 }] } })
    };
    for (kept, used) in [
        (keep(line(1, 1, 1), file().hash), true),
        (keep(line(1, 7, 1), file().hash), false),
        (keep(line(1, 1, 1), Hash::of(b"another")), false),
    ] {
        let w = world(None, replay());
        w.kept.lock().push(kept);
        let r = w.explain_with("p1-figure-1", json!({ "paper": "P" })).await;
        assert_eq!(r.seen.is_some(), used);
        assert_eq!(r.based_on.contains(&Basis::Picture), used);
        assert_eq!(
            w.sent("explain.figure").contains("A rising curve."),
            used,
            "the model is given it"
        );
    }
}

#[tokio::test]
async fn a_table_and_a_paragraph_are_explained() {
    let w = world(
        None,
        Replay::new()
            .on(
                "explain.table",
                " the errors; the lowest is 0.31 K and the highest 0.52.",
            )
            .on("explain.table.check", "OK")
            .on("explain.paragraph", "The duct is 40 mm wide.")
            .on("explain.paragraph.check", "OK"),
    );
    let r = w.explain("p1-table-1").await;
    assert_eq!(
        (r.kind, r.status, r.left_out),
        (ExplainedKind::Table, ExplainStatus::Checked, 0)
    );
    assert_eq!(
        r.based_on,
        [Basis::Caption, Basis::Cells, Basis::Mentions],
        "{r:?}"
    );
    verbatim(&r);
    let r = w.explain("p1-text-1").await;
    assert_eq!(
        (r.kind, r.text.as_str()),
        (ExplainedKind::Paragraph, "The duct is 40 mm wide.")
    );
    let said = w.sent("explain.paragraph");
    assert!(said.contains("The passage: The flow was studied") && said.contains("Text just after"));
    verbatim(&r);
    // A dragged place is a paragraph too.
    let v = w
        .call(
            "understanding.explain_region",
            json!({ "page": 1, "rect": line(1, 0, 1) }),
        )
        .await
        .unwrap();
    assert_eq!(
        (&v["kind"], v.get("object")),
        (&json!("paragraph"), None::<&Value>)
    );
}

#[tokio::test]
async fn an_equation_has_its_symbols_and_its_units() {
    let w = world(
        Some(r"Q = m c dT"),
        Replay::new()
            .on(
                "explain.equation",
                " gives the heat flow from the mass flow and the temperature rise.",
            )
            .on("explain.equation.check", "OK")
            .on("explain.symbols", "{}"),
    );
    let mut sub = w.bus.events().subscribe("understanding.symbols");
    let r = w.explain("p1-equation-1").await;
    assert_eq!(
        (r.kind, r.status),
        (ExplainedKind::Equation, ExplainStatus::Checked)
    );
    assert!(r.text.starts_with("This equation gives the heat flow"));
    let of = |s: &str| r.symbols.iter().find(|x| x.shown == s).expect(s);
    assert_eq!(of("Q").meaning.as_deref(), Some("the heat flow (W)"));
    assert_eq!(of("m").unit.as_deref(), Some("kg/s"));
    assert_eq!((of("c").page, of("c").y), (Some(1), Some(50.0)));
    let first = &events(&mut sub).await[0];
    assert_eq!(
        (&first["id"], first["symbols"].as_array().unwrap().len()),
        (&json!("x"), r.symbols.len()),
        "the symbols come before the explanation"
    );
    assert!(
        r.based_on.contains(&Basis::Formula) && r.based_on.contains(&Basis::Mentions),
        "{:?}",
        r.based_on
    );
    verbatim(&r);
}

#[tokio::test]
async fn units_that_agree_and_units_that_do_not() {
    for (latex, result) in [
        (r"Q = m c \frac{Q}{m c}", UnitsResult::Consistent),
        (r"Q = m c", UnitsResult::Inconsistent),
        (r"Q = m c z", UnitsResult::Unknown),
    ] {
        let w = world(
            Some(latex),
            Replay::new()
                .on("explain.equation", " is an equation.")
                .on("explain.equation.check", "OK")
                .on("explain.symbols", r#"{"z":""}"#),
        );
        let u = w.explain("p1-equation-1").await.units.unwrap();
        assert_eq!(u.result, result, "{latex}: {u:?}");
        assert_eq!(u.missing.len(), usize::from(latex.contains('z')), "{u:?}");
    }
}

#[tokio::test]
async fn a_meaning_from_the_model_is_kept_only_when_it_stands_in_the_paper() {
    for (said, kept) in [
        ("the wetted perimeter", true),
        ("the width of the channel", false),
        ("", false),
    ] {
        let w = world(
            Some(r"Q = w z"),
            Replay::new()
                .on("explain.equation", " is an equation.")
                .on("explain.equation.check", "OK")
                .on_with("explain.symbols", None, {
                    let said = said.to_string();
                    move |_| json!({ "w": said, "z": "" }).to_string()
                }),
        );
        let r = w.explain("p1-equation-1").await;
        let s = r.symbols.iter().find(|s| s.shown == "w").unwrap();
        // Code finds what stands there before the model is asked.
        if s.meaning.is_some() && !w.sent("explain.symbols").contains("Symbols: w") {
            assert_eq!(s.meaning.as_deref(), Some("the wetted perimeter"));
            continue;
        }
        assert_eq!(s.meaning.is_some(), kept, "{said}");
        assert_eq!(s.page, kept.then_some(2));
    }
    // Without an answer the explanation stands.
    let w = world(
        Some(r"Q = w z"),
        Replay::new()
            .on("explain.equation", " is an equation.")
            .on("explain.equation.check", "OK"),
    );
    assert_eq!(
        w.explain("p1-equation-1").await.text,
        "This equation is an equation."
    );
}

#[tokio::test]
async fn an_equation_that_was_not_read_is_explained_from_the_text() {
    let w = world(
        None,
        Replay::new()
            .on("explain.equation", " gives the heat flow.")
            .on("explain.equation.check", "OK"),
    );
    let r = w.explain("p1-equation-1").await;
    assert_eq!(r.latex, None);
    let u = r.units.unwrap();
    assert_eq!((u.result, u.unread), (UnitsResult::Unknown, true));
    assert!(
        r.symbols
            .iter()
            .any(|s| s.shown == "Q" && s.meaning.is_some()),
        "{:?}",
        r.symbols
    );
    assert!(!w.asked("explain.symbols"));
}

#[tokio::test]
async fn simpler_and_deeper_add_to_what_the_model_is_told() {
    let w = world(
        None,
        Replay::new()
            .on("explain.figure", " a thing.")
            .on("explain.figure.check", "OK")
            .on("explain.draft", "is a duct.")
            .on("explain.check", "OK"),
    );
    for depth in ["normal", "simpler", "deeper"] {
        w.explain_with("p1-figure-1", json!({ "depth": depth }))
            .await;
        w.call(
            "understanding.explain_span",
            json!({ "page": 1, "selection": "duct", "depth": depth }),
        )
        .await
        .unwrap();
    }
    for task in ["explain.figure", "explain.draft", "explain.check"] {
        let told: Vec<(String, u32)> = w
            .replay
            .seen()
            .into_iter()
            .filter(|p| p.task == task)
            .map(|p| (p.messages[0].content.clone(), p.max_tokens))
            .collect();
        let [normal, simpler, deeper] = &told[..] else {
            panic!("{task}: {}", told.len())
        };
        let words = normal.0.trim_end();
        assert!(simpler.0.len() > words.len() && deeper.0.len() > words.len());
        assert!(deeper.1 > normal.1, "{task}: a longer answer fits");
    }
}

#[tokio::test]
async fn a_table_answers_with_its_cells() {
    let plan = |op: &str, columns: Value| {
        json!({ "operation": op, "columns": columns, "rows": ["all"], "conditions": [] })
            .to_string()
    };
    let w = world(
        None,
        Replay::new()
            .on_containing(
                "table.plan",
                "lowest",
                &plan("smallest", json!(["c2: RMSE (K)"])),
            )
            .on_containing(
                "table.plan",
                "Add the names",
                &plan("sum", json!(["c1: Model"])),
            )
            .on_containing("table.plan", "weather", &plan("none", json!(["all"]))),
    );
    let ask = async |q: &str, id: &str| {
        w.call(
            "understanding.table_answer",
            json!({ "object": { "page": 1, "id": id }, "question": q }),
        )
        .await
        .map(|v| serde_json::from_value::<TableAnswer>(v).unwrap())
    };
    let a = ask("Which has the lowest error?", "p1-table-1")
        .await
        .unwrap();
    assert_eq!((a.status, a.value), (TableStatus::Computed, Some(0.31)));
    assert!(a.text.contains("0.31") && a.text.contains("PINN"), "{a:?}");
    let answer: Vec<_> = a
        .cells
        .iter()
        .filter(|c| c.role == CellRole::Answer)
        .collect();
    assert_eq!((answer.len(), answer[0].row, answer[0].col), (1, 2, 1));
    let a = ask("Add the names", "p1-table-1").await.unwrap();
    assert_eq!(a.reason, Some(TableReason::NotNumbers));
    let a = ask("How is the weather?", "p1-table-1").await.unwrap();
    assert_eq!(
        (a.status, a.reason, a.text.as_str()),
        (TableStatus::NotInTable, Some(TableReason::NotInTable), "")
    );
    let e = ask("Which?", "p1-figure-1").await.unwrap_err();
    assert_eq!(kind(e), "not_a_table");
    let e = ask("  ", "p1-table-1").await.unwrap_err();
    assert_eq!(kind(e), "empty_question");
}

/// A paper with a model that looks: it says `texts` one by one, then fails when there are too few.
fn eyes(texts: &'static [&'static str]) -> World {
    let (bus, kept) = paper(None);
    on(&bus, "inference.models", |_| {
        Ok(
            json!({ "models": [], "order": [], "runtimes": [], "dirs": [], "keep_ready": false,
            "vision": { "id": "eyes", "name": "eyes", "path": "", "mmproj": "", "size": 1 } }),
        )
    });
    let hub = bus.clone();
    bus.register(
        Spec {
            name: "inference.vision_read",
            summary: "",
            stability: Stability::Experimental,
            permission: Permission::read(),
        },
        move |_cx, i: Value| {
            let hub = hub.clone();
            async move {
                assert_eq!(i["consent"], true);
                let total = i["pictures"].as_array().unwrap().len();
                let id = &i["stream"]["id"];
                for (n, t) in texts.iter().take(total).enumerate() {
                    let read = json!({ "id": id, "done": n + 1, "total": total, "text": t });
                    let token = json!({ "id": id, "text": t });
                    hub.events().publish("inference.token", token, 0);
                    hub.events().publish("inference.vision", read, 0);
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
                if texts.len() < total {
                    return Err(failed("backend_failed"));
                }
                Ok(json!({ "texts": texts[..total], "model": "eyes", "total_ms": 1 }))
            }
        },
    )
    .unwrap();
    Understanding::new().register(&bus).unwrap();
    World {
        bus,
        replay: Replay::new(),
        kept,
    }
}

impl World {
    async fn look(&self, more: Value) -> Result<FiguresSeen, CommandError> {
        let mut input = json!({ "paper": "P", "consent": true, "stream": { "id": "L" } });
        for (k, v) in more.as_object().unwrap() {
            input[k] = v.clone();
        }
        self.call("understanding.look_at_figures", input)
            .await
            .map(|v| serde_json::from_value(v).unwrap())
    }

    async fn seen(&self) -> FiguresSeen {
        let v = self
            .call("understanding.figures_seen", json!({ "paper": "P" }))
            .await;
        serde_json::from_value(v.unwrap()).unwrap()
    }
}

#[tokio::test]
async fn pictures_are_looked_at_with_consent_and_kept() {
    let w = eyes(&["A rising curve.", "A pump."]);
    let e = w.look(json!({ "consent": false })).await.unwrap_err();
    assert_eq!(kind(e), "consent_needed");
    assert_eq!((w.seen().await.figures.len(), w.seen().await.total), (0, 2));

    let one = json!({ "objects": [{ "page": 1, "id": "p1-figure-1" }] });
    let mut sub = w.bus.events().subscribe("understanding.looking");
    let s = w.look(one).await.unwrap();
    assert_eq!((s.figures.len(), s.model.as_deref()), (1, Some("eyes")));
    assert_eq!(s.figures[0].text, "A rising curve.");
    assert_eq!(s.figures[0].label.as_deref(), Some("Figure 1"));
    let said = events(&mut sub).await;
    assert_eq!(
        (&said[0]["label"], &said[0]["total"]),
        (&json!("Figure 1"), &json!(1))
    );
    assert!(
        said.iter().any(|e| e["text"] == "A rising curve."),
        "one picture streams: {said:?}"
    );
    assert_eq!(said.last().unwrap()["done"], 1);

    // The rest: only what was not looked at yet, and then nothing.
    let s = w.look(json!({})).await.unwrap();
    assert_eq!(s.figures.len(), 2);
    let s = w.look(json!({})).await.unwrap();
    assert_eq!((s.figures.len(), s.model), (2, None));
    let e = w
        .look(json!({ "objects": [{ "page": 1, "id": "nothing" }] }))
        .await
        .unwrap_err();
    assert_eq!(kind(e), "no_object");
}

#[tokio::test]
async fn an_early_end_loses_nothing() {
    let w = eyes(&["A rising curve."]);
    let e = w.look(json!({})).await.unwrap_err();
    assert_eq!(kind(e), "backend_failed");
    let s = w.seen().await;
    assert_eq!((s.figures.len(), s.total), (1, 2));
    assert_eq!(s.figures[0].model, "eyes");
}
