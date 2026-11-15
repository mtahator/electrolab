//! `document.search` core: fast, notation-aware word search with context (pure, no I/O).
//! Design: `docs/design/document-search.md`.

mod index;
mod normalize;
mod words;

use std::collections::{BTreeMap, HashMap};

use ep_document_contract::{
    ContextOptions, ContextUnit, Hit, MatchKind, MatchMode, MatchOptions, PageRect, PaperRef,
    SearchError, SearchRequest, SearchResult, Sort, TermCount,
};

pub use index::{Heading, PageChars, SearchIndex};
use normalize::{Sep, greek_alias, key_of};
use words::{bounded_distance, max_distance, similarity};

const MAX_TERM_CHARS: usize = 200;

/// Search one or more prepared papers. `papers` pairs each paper with its index; papers in
/// `req.papers` without an index give `PaperNotPrepared`.
///
/// # Errors
/// `EmptyQuery`, `TermTooLong`, `InvalidOption`, `PaperNotPrepared` (see the contract).
pub fn search(
    papers: &[(PaperRef, &SearchIndex)],
    req: &SearchRequest,
) -> Result<SearchResult, SearchError> {
    validate(req)?;
    let mut found: Vec<Found> = Vec::new();
    for want in &req.papers {
        let (_, ix) = papers
            .iter()
            .find(|(p, _)| p == want)
            .ok_or_else(|| SearchError::PaperNotPrepared(want.clone()))?;
        for (ti, term) in req.terms.iter().enumerate() {
            for m in find_term(ix, &term.text, &req.matching) {
                if in_scope(ix, &m, req) {
                    found.push(Found {
                        paper: want.clone(),
                        term: ti,
                        m,
                        ix,
                    });
                }
            }
        }
    }
    Ok(assemble(found, req))
}

fn validate(req: &SearchRequest) -> Result<(), SearchError> {
    if req.terms.iter().all(|t| t.text.trim().is_empty()) {
        return Err(SearchError::EmptyQuery);
    }
    if let Some(i) = req
        .terms
        .iter()
        .position(|t| t.text.chars().count() > MAX_TERM_CHARS)
    {
        return Err(SearchError::TermTooLong(i));
    }
    let m = &req.matching;
    if !(0.0..=1.0).contains(&m.similarity) {
        return Err(SearchError::InvalidOption(
            "similarity must be between 0 and 1".into(),
        ));
    }
    if m.phrase_slop > 5 {
        return Err(SearchError::InvalidOption(
            "phrase_slop must be at most 5".into(),
        ));
    }
    if req.context.before > 200 || req.context.after > 200 {
        return Err(SearchError::InvalidOption(
            "context before/after must be at most 200 words".into(),
        ));
    }
    Ok(())
}

/// A match inside one paper: tokens `[first, last]`.
#[derive(Clone, Copy, Debug)]
struct Match {
    first: usize,
    last: usize,
    kind: MatchKind,
    similarity: f32,
}

struct Found<'a> {
    paper: PaperRef,
    term: usize,
    m: Match,
    ix: &'a SearchIndex,
}

// ---------------------------------------------------------------- matching ----------------

/// A query word, tokenised and folded like the paper's words.
struct QWord {
    key: String,
    chars: Vec<char>,
    raw: String,
}

fn query_words(text: &str) -> Vec<QWord> {
    let pages = [PageChars {
        page: 1,
        chars: text.chars().map(|c| (c, None)).collect(),
    }];
    let ix = SearchIndex::build(&pages, Vec::new());
    ix.tokens
        .iter()
        .map(|t| {
            let e = &ix.vocab[t.vocab as usize];
            QWord {
                key: e.key.clone(),
                chars: e.chars.clone(),
                raw: t.raw.clone(),
            }
        })
        .collect()
}

/// Every vocabulary entry a query word may match, with how and how well.
fn candidates(ix: &SearchIndex, q: &QWord, o: &MatchOptions) -> HashMap<u32, (MatchKind, f32)> {
    let mut out: HashMap<u32, (MatchKind, f32)> = HashMap::new();
    let mut add = |id: u32, kind: MatchKind, sim: f32| {
        let e = out.entry(id).or_insert((kind, sim));
        if (kind, -sim) < (e.0, -e.1) {
            *e = (kind, sim);
        }
    };
    if let Some(&id) = ix.by_key.get(&q.key) {
        add(id, MatchKind::Exact, 1.0);
    }
    if o.notation
        && let Some(alias) = greek_alias(&q.key).and_then(|a| ix.by_key.get(a))
    {
        add(*alias, MatchKind::Notation, 1.0);
    }
    if matches!(o.mode, MatchMode::WordForms | MatchMode::Fuzzy)
        && let Some(ids) = ix.by_stem.get(&words::stem(&q.key))
    {
        for &id in ids {
            add(id, MatchKind::WordForm, 1.0);
        }
    }
    let scan = o.prefix
        || !o.whole_word
        || (o.mode == MatchMode::Fuzzy && q.chars.len() >= o.min_fuzzy_length);
    if scan {
        let max_d = max_distance(q.chars.len(), o.similarity);
        for (i, e) in ix.vocab.iter().enumerate() {
            let id = u32::try_from(i).unwrap_or(u32::MAX);
            if o.prefix && e.key.len() > q.key.len() && e.key.starts_with(&q.key) {
                add(id, MatchKind::Prefix, 1.0);
            } else if !o.whole_word && e.key.len() > q.key.len() && e.key.contains(&q.key) {
                add(id, MatchKind::Substring, 1.0);
            }
            if o.mode == MatchMode::Fuzzy
                && q.chars.len() >= o.min_fuzzy_length
                && e.chars.len() >= o.min_fuzzy_length
                && let Some(d) = bounded_distance(&q.chars, &e.chars, max_d)
                && d > 0
            {
                add(
                    id,
                    MatchKind::Fuzzy,
                    similarity(d, q.chars.len(), e.chars.len()),
                );
            }
        }
    }
    out
}

fn worse(a: (MatchKind, f32), b: (MatchKind, f32)) -> (MatchKind, f32) {
    (a.0.max(b.0), a.1.min(b.1))
}

fn find_term(ix: &SearchIndex, text: &str, o: &MatchOptions) -> Vec<Match> {
    let q = query_words(text);
    if q.is_empty() {
        return Vec::new();
    }
    let cands: Vec<HashMap<u32, (MatchKind, f32)>> =
        q.iter().map(|w| candidates(ix, w, o)).collect();
    let mut by_span: BTreeMap<(usize, usize), (MatchKind, f32)> = BTreeMap::new();
    let symbol = is_symbol_like(&q);

    // 1. word by word (single words and phrases, with slop)
    for (&id, &ks0) in &cands[0] {
        for &p in &ix.vocab[id as usize].positions {
            let p = p as usize;
            if let Some((last, ks)) = follow(ix, &cands, 1, p, ks0, o.phrase_slop, symbol) {
                keep(&mut by_span, p, last, ks);
            }
        }
    }

    // 2. notation: symbol-like words glued together (`C_p` = `Cp` = `C p` = `C_{p}`)
    if o.notation && is_symbol_like(&q) {
        let compact: String = q.iter().map(|w| w.key.as_str()).collect();
        for n in 1..=compact.chars().count().min(4) {
            let head: String = compact.chars().take(n).collect();
            if let Some(&id) = ix.by_key.get(&head) {
                for &p in &ix.vocab[id as usize].positions {
                    if let Some(last) = glue(ix, p as usize, &compact) {
                        let exact_shape = last - p as usize + 1 == q.len();
                        let kind = if exact_shape && by_span.contains_key(&(p as usize, last)) {
                            None
                        } else {
                            Some(MatchKind::Notation)
                        };
                        if let Some(k) = kind {
                            keep(&mut by_span, p as usize, last, (k, 1.0));
                        }
                    }
                }
            }
        }
    }

    by_span
        .into_iter()
        .filter(|((first, last), (kind, _))| case_ok(ix, *first, *last, &q, *kind, o))
        .map(|((first, last), (kind, similarity))| Match {
            first,
            last,
            kind,
            similarity,
        })
        .collect()
}

/// May a phrase continue into token `k` across the separator before it?
/// Symbols (`C_p`) join only like notation does; phrases cross spaces, line breaks and hyphens,
/// never sentence punctuation (`C. P. Burgess` is not `C_p`; `heat. Capacity` is not a phrase).
fn step_ok(ix: &SearchIndex, k: usize, symbol: bool) -> bool {
    let t = &ix.tokens[k];
    if symbol {
        let len = |i: usize| ix.vocab[ix.tokens[i].vocab as usize].chars.len();
        return match t.sep {
            Sep::Underscore | Sep::Caret | Sep::Brace | Sep::None => true,
            Sep::Space => len(k) <= 2 && len(k - 1) <= 2,
            Sep::Break | Sep::Other | Sep::Hyphen => false,
        };
    }
    !matches!(t.sep, Sep::Other)
}

/// Record a match span, keeping the best kind for a span found several ways.
fn keep(
    map: &mut BTreeMap<(usize, usize), (MatchKind, f32)>,
    first: usize,
    last: usize,
    ks: (MatchKind, f32),
) {
    let e = map.entry((first, last)).or_insert(ks);
    if (ks.0, -ks.1) < (e.0, -e.1) {
        *e = ks;
    }
}

/// Continue a phrase match from token `p` with query word `qi`, allowing `slop` skipped tokens.
fn follow(
    ix: &SearchIndex,
    cands: &[HashMap<u32, (MatchKind, f32)>],
    qi: usize,
    p: usize,
    ks: (MatchKind, f32),
    slop: usize,
    symbol: bool,
) -> Option<(usize, (MatchKind, f32))> {
    if qi == cands.len() {
        return Some((p, ks));
    }
    for skip in 0..=slop {
        let next = p + 1 + skip;
        let t = ix.tokens.get(next)?;
        if !(p + 1..=next).all(|k| step_ok(ix, k, symbol)) {
            return None;
        }
        if let Some(&k) = cands[qi].get(&t.vocab)
            && let Some(r) = follow(ix, cands, qi + 1, next, worse(ks, k), slop, symbol)
        {
            return Some(r);
        }
    }
    None
}

fn is_symbol_like(q: &[QWord]) -> bool {
    let total: usize = q.iter().map(|w| w.chars.len()).sum();
    q.len() <= 3 && total <= 8 && (q.len() > 1 || total <= 4)
}

/// Glue up to 3 consecutive short tokens from `p` and check they spell `compact`. Tokens may be
/// joined by `_ ^ { }` or nothing, or by a space when both sides are 1–2 characters (a subscript
/// printed as a separate glyph).
fn glue(ix: &SearchIndex, p: usize, compact: &str) -> Option<usize> {
    let mut acc = String::new();
    for (k, t) in ix.tokens.iter().enumerate().skip(p).take(3) {
        let key = &ix.vocab[t.vocab as usize].key;
        if k > p {
            let prev_len = ix.vocab[ix.tokens[k - 1].vocab as usize].chars.len();
            let ok = match t.sep {
                Sep::Underscore | Sep::Caret | Sep::Brace | Sep::None => key.chars().count() <= 4,
                Sep::Space => key.chars().count() <= 2 && prev_len <= 2,
                Sep::Break | Sep::Other | Sep::Hyphen => false,
            };
            if !ok {
                return None;
            }
        }
        acc.push_str(key);
        if acc == compact {
            return Some(k);
        }
        if !compact.starts_with(&acc) {
            return None;
        }
    }
    None
}

/// Case- or accent-sensitive options: compare raw forms for exact-like matches.
fn case_ok(
    ix: &SearchIndex,
    first: usize,
    last: usize,
    q: &[QWord],
    kind: MatchKind,
    o: &MatchOptions,
) -> bool {
    let sensitive = o.case_sensitive || !o.fold_accents;
    if !sensitive || !matches!(kind, MatchKind::Exact | MatchKind::Notation) {
        return true;
    }
    let doc: String = ix.tokens[first..=last]
        .iter()
        .map(|t| normalize::form(&t.raw, o.case_sensitive, o.fold_accents))
        .collect();
    let qs: String = q
        .iter()
        .map(|w| normalize::form(&w.raw, o.case_sensitive, o.fold_accents))
        .collect();
    doc == qs
}

fn in_scope(ix: &SearchIndex, m: &Match, req: &SearchRequest) -> bool {
    let t = &ix.tokens[m.first];
    let s = &req.scope;
    if !s.pages.is_empty() && !s.pages.iter().any(|r| (r.first..=r.last).contains(&t.page)) {
        return false;
    }
    let title = t
        .section
        .and_then(|h| ix.headings.get(h as usize))
        .map(|h| h.title.to_lowercase());
    let has = |list: &[String]| {
        title
            .as_ref()
            .is_some_and(|tt| list.iter().any(|x| tt.contains(&x.to_lowercase())))
    };
    if !s.include_sections.is_empty() && !has(&s.include_sections) {
        return false;
    }
    s.exclude_sections.is_empty() || !has(&s.exclude_sections)
}

// ---------------------------------------------------------------- results -----------------

fn assemble(mut found: Vec<Found>, req: &SearchRequest) -> SearchResult {
    let mut counts: Vec<TermCount> = (0..req.terms.len())
        .map(|term| TermCount {
            term,
            ..TermCount::default()
        })
        .collect();
    for f in &found {
        let c = &mut counts[f.term];
        c.total += 1;
        let page = f.ix.tokens[f.m.first].page;
        match c.per_page.iter_mut().find(|(p, _)| *p == page) {
            Some(x) => x.1 += 1,
            None => c.per_page.push((page, 1)),
        }
        match c.per_kind.iter_mut().find(|(k, _)| *k == f.m.kind) {
            Some(x) => x.1 += 1,
            None => c.per_kind.push((f.m.kind, 1)),
        }
    }
    for c in &mut counts {
        c.per_page.sort_unstable();
        c.per_kind.sort_unstable();
    }
    let order_of = |p: &PaperRef| req.papers.iter().position(|x| x == p).unwrap_or(0);
    found.sort_by_key(|a| (order_of(&a.paper), a.m.first, a.term));
    let found = merge_close(found, req.context.merge_within);
    let mut found = found;
    if req.sort == Sort::Score {
        found.sort_by(|a, b| {
            (a.0.m.kind, -a.0.m.similarity)
                .partial_cmp(&(b.0.m.kind, -b.0.m.similarity))
                .unwrap_or(std::cmp::Ordering::Equal)
        });
    }

    let mut result = SearchResult {
        counts,
        ..SearchResult::default()
    };
    let mut per_term = vec![0usize; req.terms.len()];
    let mut chars = 0usize;
    for (f, merged_last, merged) in found {
        if result.hits.len() >= req.limits.max_hits
            || per_term[f.term] >= req.limits.max_hits_per_term
        {
            result.truncated = true;
            continue;
        }
        let hit = make_hit(&f, merged_last, merged, req);
        let size = hit.before.len() + hit.matched.len() + hit.after.len();
        if chars + size > req.limits.max_context_chars {
            result.truncated = true;
            continue;
        }
        chars += size;
        per_term[f.term] += 1;
        result.hits.push(hit);
    }
    result
}

/// Fold hits of the same term closer than `within` words into the first one.
/// Returns `(hit, last token covered by the snippet, number folded in)`.
fn merge_close(found: Vec<Found>, within: usize) -> Vec<(Found, usize, usize)> {
    let mut out: Vec<(Found, usize, usize)> = Vec::new();
    for f in found {
        if within > 0
            && let Some(prev) = out
                .iter_mut()
                .rev()
                .find(|(p, _, _)| p.term == f.term && p.paper == f.paper)
            && f.m.first <= prev.1 + within
            && f.m.first > prev.0.m.last
        {
            prev.1 = prev.1.max(f.m.last);
            prev.2 += 1;
            continue;
        }
        let last = f.m.last;
        out.push((f, last, 0));
    }
    out
}

fn make_hit(f: &Found, snippet_last: usize, merged: usize, req: &SearchRequest) -> Hit {
    let ix = f.ix;
    let (a, b) = (ix.tokens[f.m.first].start, ix.tokens[f.m.last].end);
    let page = ix.tokens[f.m.first].page;
    let (before, after) = context(ix, f.m.first, snippet_last, page, &req.context);
    let section = ix.tokens[f.m.first]
        .section
        .and_then(|h| ix.headings.get(h as usize))
        .map(|h| h.title.clone());
    Hit {
        term: f.term,
        label: req.terms[f.term].label.clone(),
        paper: f.paper.clone(),
        page,
        matched: ix.text[a..b].iter().collect(),
        before,
        after,
        kind: f.m.kind,
        similarity: f.m.similarity,
        rects: rects(ix, a, b),
        section,
        char_range: (a, b),
        merged,
    }
}

fn clean(chars: &[char]) -> String {
    let mut s = String::with_capacity(chars.len());
    let mut space = false;
    for &c in chars {
        // soft hyphens, and control codes PDFium emits for some math-font glyphs, are not readable text
        if c == '\u{00AD}' || (c.is_control() && !c.is_whitespace()) {
            continue;
        }
        if c.is_whitespace() {
            space = true;
        } else {
            if space && !s.is_empty() {
                s.push(' ');
            }
            space = false;
            s.push(c);
        }
    }
    s
}

fn context(
    ix: &SearchIndex,
    first: usize,
    last: usize,
    page: u32,
    opts: &ContextOptions,
) -> (String, String) {
    let (m_start, m_end) = (ix.tokens[first].start, ix.tokens[last].end);
    match opts.unit {
        ContextUnit::Words => {
            let (lo, hi) = if opts.cross_pages {
                (0, ix.tokens.len())
            } else {
                ix.page_token_range(page)
            };
            let bt = first.saturating_sub(opts.before).max(lo);
            let at = (last + opts.after).min(hi.saturating_sub(1)).max(last);
            let before = if bt < first {
                clean(&ix.text[ix.tokens[bt].start..m_start])
            } else {
                String::new()
            };
            let after = if at > last {
                clean(&ix.text[m_end..ix.tokens[at].end])
            } else {
                String::new()
            };
            (before, after)
        }
        ContextUnit::Sentence => {
            let end_of = |i: usize| {
                matches!(ix.text[i], '.' | '!' | '?' | '؟' | '۔')
                    && ix.text.get(i + 1).is_none_or(|c| c.is_whitespace())
            };
            let mut s = m_start;
            while s > 0 && m_start - s < 600 && ix.page_of[s - 1] == page && !end_of(s - 1) {
                s -= 1;
            }
            let mut e = m_end;
            while e < ix.text.len() && e - m_end < 600 && ix.page_of[e] == page && !end_of(e) {
                e += 1;
            }
            let e = (e + 1).min(ix.text.len());
            (clean(&ix.text[s..m_start]), clean(&ix.text[m_end..e]))
        }
        // `None`, and any unit a later contract version adds: no context
        _ => (String::new(), String::new()),
    }
}

/// Boxes of the matched characters, merged into one rectangle per line.
fn rects(ix: &SearchIndex, a: usize, b: usize) -> Vec<PageRect> {
    let mut out: Vec<PageRect> = Vec::new();
    for r in ix.boxes[a..b].iter().flatten() {
        match out.last_mut() {
            Some(last) if last.page == r.page && same_line(last, r) => *last = last.union(r),
            _ => out.push(*r),
        }
    }
    out
}

fn same_line(a: &PageRect, b: &PageRect) -> bool {
    let (ca, cb) = (f32::midpoint(a.y0, a.y1), f32::midpoint(b.y0, b.y1));
    (ca - cb).abs() < a.height().max(b.height()) * 0.6 && b.x0 + 2.0 >= a.x0
}

/// Query words for a term as the index sees them (exposed for the tool layer and tests).
#[must_use]
pub fn normalised_term(text: &str) -> Vec<String> {
    query_words(text).into_iter().map(|w| w.key).collect()
}

/// Fold a string the way the index folds words (exposed for tests and callers).
#[must_use]
pub fn fold_key(text: &str) -> String {
    key_of(text)
}

#[cfg(test)]
mod tests {
    use super::index::tests::index_of;
    use super::*;
    use ep_document_contract::{Limits, Scope, Term};

    fn req(terms: &[&str], m: MatchOptions) -> SearchRequest {
        SearchRequest {
            near_page: None,
            stream: None,
            papers: vec![PaperRef("p".into())],
            terms: terms
                .iter()
                .map(|t| Term {
                    text: (*t).into(),
                    label: None,
                })
                .collect(),
            matching: m,
            context: ContextOptions::default(),
            scope: Scope::default(),
            limits: Limits::default(),
            sort: Sort::Position,
        }
    }

    fn run(pages: &[&str], terms: &[&str], m: MatchOptions) -> SearchResult {
        let ix = index_of(pages);
        search(&[(PaperRef("p".into()), &ix)], &req(terms, m)).expect("search")
    }

    fn matched(r: &SearchResult) -> Vec<String> {
        r.hits.iter().map(|h| h.matched.clone()).collect()
    }

    #[test]
    fn notation_variants_all_match() {
        let page = "Cp is the heat capacity; C_p again, C_{p} here, 𝐶ₚ there, C p printed apart, c_p lower. Not CPU.";
        for q in ["Cp", "C_p", "C_{p}", "cp", "C p"] {
            let r = run(&[page], &[q], MatchOptions::default());
            assert_eq!(r.counts[0].total, 6, "query {q}: {:?}", matched(&r));
        }
    }

    #[test]
    fn punctuation_is_not_a_symbol_or_phrase_join() {
        let page = "see C. P. Burgess; heat. Capacity matters; the Navier-Stokes and Navier Stokes equations; C_p";
        assert_eq!(
            matched(&run(&[page], &["C_p"], MatchOptions::default())),
            ["C_p"]
        );
        assert_eq!(
            run(&[page], &["heat capacity"], MatchOptions::default()).counts[0].total,
            0
        );
        assert_eq!(
            run(&[page], &["Navier Stokes"], MatchOptions::default()).counts[0].total,
            2
        );
    }

    #[test]
    fn notation_off_keeps_written_form() {
        let m = MatchOptions {
            notation: false,
            ..MatchOptions::default()
        };
        let r = run(&["Cp and C_p"], &["Cp"], m);
        assert_eq!(matched(&r), ["Cp"]);
    }

    #[test]
    fn greek_names_and_letters() {
        let r = run(
            &["the weight α and alpha and \\alpha"],
            &["alpha"],
            MatchOptions::default(),
        );
        assert_eq!(r.counts[0].total, 3);
    }

    #[test]
    fn word_forms_and_exact() {
        let page = "One equation, two equations, solved by solving.";
        let r = run(&[page], &["equation", "solve"], MatchOptions::default());
        assert_eq!((r.counts[0].total, r.counts[1].total), (2, 2));
        let exact = MatchOptions {
            mode: MatchMode::Exact,
            ..MatchOptions::default()
        };
        let r = run(&[page], &["equation"], exact);
        assert_eq!(r.counts[0].total, 1);
    }

    #[test]
    fn fuzzy_finds_ocr_errors_within_similarity() {
        let page = "the Navier-Stokes equations; the Naiver-Stokes typo; modulat1on noise";
        let m = MatchOptions {
            mode: MatchMode::Fuzzy,
            similarity: 0.6,
            ..MatchOptions::default()
        };
        let r = run(&[page], &["Navier", "modulation"], m);
        assert_eq!(r.counts[0].total, 2);
        assert_eq!(r.counts[1].total, 1);
        assert!(
            r.hits
                .iter()
                .any(|h| h.kind == MatchKind::Fuzzy && h.similarity < 1.0)
        );
        let strict = MatchOptions {
            mode: MatchMode::Fuzzy,
            similarity: 0.95,
            ..MatchOptions::default()
        };
        assert_eq!(run(&[page], &["Navier"], strict).counts[0].total, 1);
    }

    #[test]
    fn short_words_are_never_fuzzy() {
        let m = MatchOptions {
            mode: MatchMode::Fuzzy,
            similarity: 0.5,
            ..MatchOptions::default()
        };
        let r = run(&["net not set get"], &["net"], m);
        assert_eq!(matched(&r), ["net"]);
    }

    #[test]
    fn phrases_with_and_without_slop() {
        let page = "heat capacity, heat transfer capacity, specific heat";
        assert_eq!(
            run(&[page], &["heat capacity"], MatchOptions::default()).counts[0].total,
            1
        );
        let slop = MatchOptions {
            phrase_slop: 1,
            ..MatchOptions::default()
        };
        assert_eq!(run(&[page], &["heat capacity"], slop).counts[0].total, 2);
    }

    #[test]
    fn prefix_substring_and_case() {
        let page = "Thermal and thermodynamic network networks Re re";
        let p = MatchOptions {
            prefix: true,
            ..MatchOptions::default()
        };
        assert_eq!(run(&[page], &["therm"], p).counts[0].total, 2);
        let s = MatchOptions {
            whole_word: false,
            ..MatchOptions::default()
        };
        assert_eq!(run(&[page], &["work"], s).counts[0].total, 2);
        let c = MatchOptions {
            case_sensitive: true,
            ..MatchOptions::default()
        };
        assert_eq!(matched(&run(&[page], &["Re"], c)), ["Re"]);
        assert_eq!(
            run(&[page], &["Re"], MatchOptions::default()).counts[0].total,
            2
        );
    }

    #[test]
    fn context_words_sentence_and_pages() {
        let p1 = "One two three four five target six seven eight nine ten. Next sentence here.";
        let ix = index_of(&[p1, "page two starts target end"]);
        let mut r = req(&["target"], MatchOptions::default());
        r.context.before = 2;
        r.context.after = 3;
        let res = search(&[(PaperRef("p".into()), &ix)], &r).expect("search");
        assert_eq!(res.hits[0].before, "four five");
        assert_eq!(res.hits[0].after, "six seven eight");
        assert_eq!(res.hits[1].page, 2);
        assert_eq!(res.hits[1].before, "two starts");
        r.context.unit = ContextUnit::Sentence;
        let res = search(&[(PaperRef("p".into()), &ix)], &r).expect("search");
        assert_eq!(res.hits[0].after, "six seven eight nine ten.");
        assert_eq!(res.hits[0].before, "One two three four five");
    }

    #[test]
    fn limits_cut_hits_but_not_counts() {
        let page = "x a x b x c x d x";
        let ix = index_of(&[page]);
        let mut r = req(&["x"], MatchOptions::default());
        r.limits.max_hits = 2;
        let res = search(&[(PaperRef("p".into()), &ix)], &r).expect("search");
        assert_eq!(res.hits.len(), 2);
        assert_eq!(res.counts[0].total, 5);
        assert!(res.truncated);
    }

    #[test]
    fn merge_within_folds_nearby_hits() {
        let ix = index_of(&["x a x b c d e f g h x"]);
        let mut r = req(&["x"], MatchOptions::default());
        r.context.merge_within = 3;
        let res = search(&[(PaperRef("p".into()), &ix)], &r).expect("search");
        assert_eq!(res.hits.len(), 2);
        assert_eq!(res.hits[0].merged, 1);
    }

    #[test]
    fn persian_words_match_across_letter_forms() {
        let page = "شبكه\u{200C}هاي عصبي مصنوعي";
        let r = run(&[page], &["شبکه‌های"], MatchOptions::default());
        assert_eq!(r.counts[0].total, 1);
        assert_eq!(
            run(&[page], &["عصبی"], MatchOptions::default()).counts[0].total,
            1
        );
    }

    #[test]
    fn hits_point_at_exact_characters() {
        let page = "the C_p value";
        let ix = index_of(&[page]);
        let res = search(
            &[(PaperRef("p".into()), &ix)],
            &req(&["Cp"], MatchOptions::default()),
        )
        .expect("search");
        let (a, b) = res.hits[0].char_range;
        assert_eq!(page.chars().skip(a).take(b - a).collect::<String>(), "C_p");
        assert_eq!(res.hits[0].matched, "C_p");
        assert_eq!(res.hits[0].rects.len(), 1);
    }

    #[test]
    fn context_drops_control_codes_but_match_stays_exact() {
        let page = "a \u{12}max\u{1a} value b";
        let ix = index_of(&[page]);
        let res = search(
            &[(PaperRef("p".into()), &ix)],
            &req(&["value"], MatchOptions::default()),
        )
        .expect("search");
        assert_eq!(res.hits[0].before, "a max");
        assert_eq!(res.hits[0].matched, "value");
    }

    #[test]
    fn errors() {
        let ix = index_of(&["x"]);
        assert_eq!(
            search(
                &[(PaperRef("p".into()), &ix)],
                &req(&["  "], MatchOptions::default())
            )
            .err(),
            Some(SearchError::EmptyQuery)
        );
        let bad = MatchOptions {
            similarity: 1.5,
            ..MatchOptions::default()
        };
        assert!(matches!(
            search(&[(PaperRef("p".into()), &ix)], &req(&["x"], bad)),
            Err(SearchError::InvalidOption(_))
        ));
        let mut r = req(&["x"], MatchOptions::default());
        r.papers = vec![PaperRef("missing".into())];
        assert!(matches!(
            search(&[(PaperRef("p".into()), &ix)], &r),
            Err(SearchError::PaperNotPrepared(_))
        ));
    }
}
