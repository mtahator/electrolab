//! Speed of `document.search` on the fixture papers, with PDFium's real characters and boxes.
//!
//! Input: spike S2's per-paper character files (`spikes/s2-pdfium/out/*.pdfium.chars.json`:
//! per page `w`, `h`, `chars: [u, origin x, origin y, left, bottom, right, top]` in PDF space).
//! usage: cargo run -p ep-document-core --example search_bench --release -- <s2-out-dir>

// A measuring tool, not library code: stopping loudly on a bad input file is the right behaviour.
#![allow(
    clippy::expect_used,
    clippy::cast_possible_truncation,
    clippy::too_many_lines,
    clippy::doc_markdown
)]

use std::path::Path;
use std::time::Instant;

use ep_document_contract::{
    ContextOptions, Limits, MatchMode, MatchOptions, PageRect, PaperRef, Scope, SearchRequest,
    Sort, Term,
};
use ep_document_core::search::{PageChars, SearchIndex, search};

fn load(path: &Path) -> Vec<PageChars> {
    let pages: serde_json::Value =
        serde_json::from_slice(&std::fs::read(path).expect("read")).expect("json");
    pages
        .as_array()
        .expect("array")
        .iter()
        .enumerate()
        .map(|(i, p)| {
            let h = p["h"].as_f64().unwrap_or(792.0) as f32;
            let page = u32::try_from(i + 1).expect("page");
            let chars = p["chars"]
                .as_array()
                .expect("chars")
                .iter()
                .map(|c| {
                    let ch = c[0].as_str().and_then(|s| s.chars().next()).unwrap_or(' ');
                    let f = |k: usize| c[k].as_f64().unwrap_or(0.0) as f32;
                    // PDF space (y up) → page space (y down, top-left origin), ADR-003
                    let r = PageRect {
                        page,
                        x0: f(3),
                        y0: h - f(6),
                        x1: f(5),
                        y1: h - f(4),
                    };
                    (ch, (r.x1 > r.x0).then_some(r))
                })
                .collect();
            PageChars { page, chars }
        })
        .collect()
}

fn request(papers: &[PaperRef], terms: &[&str], matching: MatchOptions) -> SearchRequest {
    SearchRequest {
        near_page: None,
        stream: None,
        papers: papers.to_vec(),
        terms: terms
            .iter()
            .map(|t| Term {
                text: (*t).into(),
                label: None,
            })
            .collect(),
        matching,
        context: ContextOptions {
            before: 25,
            after: 25,
            ..ContextOptions::default()
        },
        scope: Scope::default(),
        limits: Limits::default(),
        sort: Sort::Position,
    }
}

fn median_us(mut f: impl FnMut() -> usize, runs: usize) -> (f64, usize) {
    let mut t = Vec::with_capacity(runs);
    let mut n = 0;
    for _ in 0..runs {
        let s = Instant::now();
        n = f();
        t.push(s.elapsed().as_secs_f64() * 1e6);
    }
    t.sort_by(f64::total_cmp);
    (t[runs / 2], n)
}

fn main() {
    let dir = std::env::args().nth(1).expect("s2 out dir");
    let mut files: Vec<_> = std::fs::read_dir(&dir)
        .expect("dir")
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.to_string_lossy().ends_with(".pdfium.chars.json"))
        .collect();
    files.sort();

    let mut papers = Vec::new();
    for f in &files {
        let name = f
            .file_name()
            .expect("name")
            .to_string_lossy()
            .replace(".pdfium.chars.json", "");
        let pages = load(f);
        let t = Instant::now();
        let ix = SearchIndex::build(&pages, Vec::new());
        let build_ms = t.elapsed().as_secs_f64() * 1000.0;
        println!(
            "{}",
            serde_json::json!({ "paper": name, "pages": pages.len(), "words": ix.word_count(),
            "vocabulary": ix.vocabulary_size(), "index_build_ms": (build_ms * 100.0).round() / 100.0 })
        );
        papers.push((PaperRef(name), ix));
    }

    let queries: [(&str, &[&str], MatchOptions); 7] = [
        (
            "exact word",
            &["network"],
            MatchOptions {
                mode: MatchMode::Exact,
                ..MatchOptions::default()
            },
        ),
        (
            "word forms (default)",
            &["equation"],
            MatchOptions::default(),
        ),
        ("notation symbol", &["C_p"], MatchOptions::default()),
        ("phrase", &["neural network"], MatchOptions::default()),
        (
            "several terms",
            &["loss", "boundary condition", "Reynolds"],
            MatchOptions::default(),
        ),
        (
            "fuzzy 0.8",
            &["Navier"],
            MatchOptions {
                mode: MatchMode::Fuzzy,
                ..MatchOptions::default()
            },
        ),
        (
            "prefix",
            &["therm"],
            MatchOptions {
                prefix: true,
                ..MatchOptions::default()
            },
        ),
    ];
    let refs: Vec<(PaperRef, &SearchIndex)> =
        papers.iter().map(|(p, ix)| (p.clone(), ix)).collect();
    let biggest = papers
        .iter()
        .max_by_key(|(_, ix)| ix.word_count())
        .expect("papers");
    let all: Vec<PaperRef> = papers.iter().map(|(p, _)| p.clone()).collect();
    for (label, terms, m) in &queries {
        let one = request(std::slice::from_ref(&biggest.0), terms, m.clone());
        let (us_one, hits_one) = median_us(
            || {
                search(&refs, &one)
                    .expect("search")
                    .counts
                    .iter()
                    .map(|c| c.total)
                    .sum()
            },
            50,
        );
        let coll = request(&all, terms, m.clone());
        let (us_all, hits_all) = median_us(
            || {
                search(&refs, &coll)
                    .expect("search")
                    .counts
                    .iter()
                    .map(|c| c.total)
                    .sum()
            },
            20,
        );
        println!(
            "{}",
            serde_json::json!({ "query": label, "terms": terms,
            "largest_paper": biggest.0 .0, "largest_paper_us": us_one.round(), "largest_paper_occurrences": hits_one,
            "all_papers": papers.len(), "all_papers_us": us_all.round(), "all_papers_occurrences": hits_all })
        );
    }

    // show one result the way the language model would receive it
    let demo = request(&all, &["C_p"], MatchOptions::default());
    let r = search(&refs, &demo).expect("search");
    if let Some(h) = r.hits.first() {
        println!(
            "{}",
            serde_json::json!({ "example_hit": { "paper": h.paper.0, "page": h.page, "matched": h.matched,
            "kind": h.kind, "before": h.before, "after": h.after, "rects": h.rects.len() } })
        );
    }
}
