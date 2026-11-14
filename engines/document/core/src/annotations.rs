//! Annotation rules (P1.M3.MOD.1–4, P1.M3.PDF.1–2; ADR-006): validation, edits and their
//! inverses (undo/redo is a stack of inverse changes), import of annotations found in a PDF and
//! the plan for writing them back as standard PDF annotations with appearance streams.
//!
//! Pure: no I/O, clock or randomness — time comes in as `now_ms`. The lopdf adapter reads raw
//! dictionaries into [`RawAnnot`] and turns a [`WriteSpec`] into PDF objects.

// Coordinate algebra and PDF operators read best with short names; one match per annotation
// kind keeps each kind's writing rules together.
#![allow(clippy::many_single_char_names, clippy::too_many_lines)]

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fmt::Write as _;

use ep_document_contract::{Annotation, AnnotationKind, AnnotationOp, Category};
use ep_types::{PagePoint, PageQuad, PageRect};

use crate::pdf::PageGeom;

/// Longest comment accepted (characters).
pub const MAX_BODY: usize = 100_000;
/// Most points in one annotation (all ink strokes together).
pub const MAX_POINTS: usize = 20_000;

/// One change: the annotation before and after (`None` = absent). Undo applies `before`.
#[derive(Clone, Debug, PartialEq)]
pub struct Change {
    /// Annotation id.
    pub id: String,
    /// State before the change.
    pub before: Option<Annotation>,
    /// State after the change.
    pub after: Option<Annotation>,
}

/// Why an edit was refused.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EditError {
    /// The annotation is not valid.
    Invalid(String),
    /// Add of an existing id, or change/remove of a missing one.
    Conflict(String),
}

impl std::fmt::Display for EditError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Invalid(m) | Self::Conflict(m) => f.write_str(m),
        }
    }
}

fn finite(v: f32) -> bool {
    v.is_finite() && v.abs() < 1.0e6
}

fn point_ok(p: PagePoint) -> bool {
    finite(p.x) && finite(p.y)
}

/// Parse `#rrggbb` into 0–1 components.
#[must_use]
pub fn parse_color(c: &str) -> Option<[f32; 3]> {
    let h = c.strip_prefix('#')?;
    if h.len() != 6 || !h.is_ascii() {
        return None;
    }
    let comp = |i: usize| u8::from_str_radix(&h[i..i + 2], 16).ok();
    Some([
        f32::from(comp(0)?) / 255.0,
        f32::from(comp(2)?) / 255.0,
        f32::from(comp(4)?) / 255.0,
    ])
}

/// `#rrggbb` from 0–1 components (gray when fewer than three are given).
#[must_use]
pub fn color_hex(c: &[f32]) -> String {
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // clamped to 0–255
    let b = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    match c {
        [r, g, bl, ..] => format!("#{:02x}{:02x}{:02x}", b(*r), b(*g), b(*bl)),
        [g] | [g, _] => format!("#{0:02x}{0:02x}{0:02x}", b(*g)),
        [] => "#ffd400".into(),
    }
}

fn bounds_of(page: u32, pts: impl Iterator<Item = PagePoint>, pad: f32) -> Option<PageRect> {
    let mut r: Option<PageRect> = None;
    for p in pts {
        let q = PageRect {
            page,
            x0: p.x,
            y0: p.y,
            x1: p.x,
            y1: p.y,
        };
        r = Some(r.map_or(q, |o| o.union(&q)));
    }
    r.map(|r| PageRect {
        x0: r.x0 - pad,
        y0: r.y0 - pad,
        x1: r.x1 + pad,
        y1: r.y1 + pad,
        ..r
    })
}

/// Check an annotation and normalise it: page numbers everywhere, bounds recomputed from the
/// geometry, values clamped. `pages` is the paper's page count.
///
/// # Errors
/// [`EditError::Invalid`] with the reason.
pub fn normalize(mut a: Annotation, pages: u32) -> Result<Annotation, EditError> {
    let bad = |m: &str| Err(EditError::Invalid(format!("annotation {}: {m}", a.id)));
    if a.id.is_empty() || a.id.len() > 200 {
        return bad("the id must be 1–200 characters");
    }
    if a.page == 0 || a.page > pages {
        return bad(&format!("page {} does not exist", a.page));
    }
    if parse_color(&a.color).is_none() {
        return bad("the colour must be #rrggbb");
    }
    if a.body.chars().count() > MAX_BODY {
        return bad("the comment is too long");
    }
    a.color = a.color.to_ascii_lowercase();
    a.opacity = if a.opacity.is_finite() {
        a.opacity.clamp(0.05, 1.0)
    } else {
        1.0
    };
    a.width = if a.width.is_finite() {
        a.width.clamp(0.25, 50.0)
    } else {
        1.0
    };
    a.font_size = a.font_size.map(|s| {
        if s.is_finite() {
            s.clamp(4.0, 144.0)
        } else {
            12.0
        }
    });
    let page = a.page;
    let rect_ok = |r: &PageRect| {
        point_ok(PagePoint { x: r.x0, y: r.y0 }) && point_ok(PagePoint { x: r.x1, y: r.y1 })
    };
    if !rect_ok(&a.rect) {
        return bad("the box is not a finite rectangle");
    }
    a.rect = PageRect {
        page,
        x0: a.rect.x0.min(a.rect.x1),
        y0: a.rect.y0.min(a.rect.y1),
        x1: a.rect.x0.max(a.rect.x1),
        y1: a.rect.y0.max(a.rect.y1),
    };
    let points: usize = a.paths.iter().map(Vec::len).sum::<usize>() + a.quads.len() * 4;
    if points > MAX_POINTS {
        return bad("too many points");
    }
    if a.parent.is_some() && a.kind != AnnotationKind::Note {
        return bad("only notes can be replies");
    }
    match a.kind {
        k if k.is_text_markup() => {
            if a.quads.is_empty() {
                return bad("text mark-up needs at least one quad");
            }
            for q in &mut a.quads {
                q.page = page;
                if !q.p.iter().all(|p| point_ok(*p)) {
                    return bad("a quad is not finite");
                }
            }
            if let Some(r) = bounds_of(page, a.quads.iter().flat_map(|q| q.p), 0.0) {
                a.rect = r;
            }
            a.paths.clear();
            a.line = None;
        }
        AnnotationKind::Ink => {
            a.paths.retain(|p| !p.is_empty());
            if a.paths.is_empty() {
                return bad("ink needs at least one stroke");
            }
            if !a.paths.iter().flatten().all(|p| point_ok(*p)) {
                return bad("a point is not finite");
            }
            if let Some(r) = bounds_of(page, a.paths.iter().flatten().copied(), a.width / 2.0 + 1.0)
            {
                a.rect = r;
            }
            a.quads.clear();
            a.line = None;
        }
        AnnotationKind::Line | AnnotationKind::Arrow => {
            let Some(l) = a.line else {
                return bad("a line needs a start and an end");
            };
            if !l.iter().all(|p| point_ok(*p)) {
                return bad("a point is not finite");
            }
            let pad = a.width / 2.0
                + if a.kind == AnnotationKind::Arrow {
                    3.0 + a.width * 3.0
                } else {
                    1.0
                };
            if let Some(r) = bounds_of(page, l.into_iter(), pad) {
                a.rect = r;
            }
            a.quads.clear();
            a.paths.clear();
        }
        AnnotationKind::Note => {
            // The icon: a fixed 20 × 20 point square at the top-left corner given.
            a.rect = PageRect {
                page,
                x0: a.rect.x0,
                y0: a.rect.y0,
                x1: a.rect.x0 + 20.0,
                y1: a.rect.y0 + 20.0,
            };
            a.quads.clear();
            a.paths.clear();
            a.line = None;
        }
        _ => {
            if a.rect.width() < 1.0 || a.rect.height() < 1.0 {
                return bad("the box is too small");
            }
            a.quads.clear();
            a.paths.clear();
            a.line = None;
        }
    }
    Ok(a)
}

/// Apply `ops` to `set` as one edit. On error nothing changes. Removing an annotation also
/// removes its replies. Returns the changes, in order, for the history.
///
/// # Errors
/// An op is invalid or conflicts with the current state.
pub fn apply(
    set: &mut BTreeMap<String, Annotation>,
    ops: Vec<AnnotationOp>,
    pages: u32,
    now_ms: i64,
) -> Result<Vec<Change>, EditError> {
    let mut work = set.clone();
    let mut changes = Vec::new();
    for op in ops {
        match op {
            AnnotationOp::Add { annotation } => {
                if work.contains_key(&annotation.id) {
                    return Err(EditError::Conflict(format!(
                        "annotation {} already exists",
                        annotation.id
                    )));
                }
                let mut a = normalize(annotation, pages)?;
                a.created_ms = if a.created_ms > 0 {
                    a.created_ms
                } else {
                    now_ms
                };
                a.modified_ms = now_ms;
                a.pristine = false;
                check_parent(&work, &a)?;
                changes.push(Change {
                    id: a.id.clone(),
                    before: None,
                    after: Some(a.clone()),
                });
                work.insert(a.id.clone(), a);
            }
            AnnotationOp::Change { annotation } => {
                let Some(old) = work.get(&annotation.id).cloned() else {
                    return Err(EditError::Conflict(format!(
                        "annotation {} does not exist",
                        annotation.id
                    )));
                };
                let mut a = normalize(annotation, pages)?;
                a.created_ms = old.created_ms;
                a.modified_ms = now_ms;
                a.pristine = false;
                check_parent(&work, &a)?;
                changes.push(Change {
                    id: a.id.clone(),
                    before: Some(old),
                    after: Some(a.clone()),
                });
                work.insert(a.id.clone(), a);
            }
            AnnotationOp::Remove { id } => {
                if !work.contains_key(&id) {
                    return Err(EditError::Conflict(format!(
                        "annotation {id} does not exist"
                    )));
                }
                for victim in thread(&work, &id) {
                    if let Some(old) = work.remove(&victim) {
                        changes.push(Change {
                            id: victim,
                            before: Some(old),
                            after: None,
                        });
                    }
                }
            }
        }
    }
    *set = work;
    Ok(changes)
}

fn check_parent(set: &BTreeMap<String, Annotation>, a: &Annotation) -> Result<(), EditError> {
    let Some(p) = &a.parent else { return Ok(()) };
    if p == &a.id {
        return Err(EditError::Invalid("a note cannot answer itself".into()));
    }
    match set.get(p) {
        Some(parent) if parent.page == a.page => Ok(()),
        Some(_) => Err(EditError::Invalid(
            "a reply must be on its note's page".into(),
        )),
        None => Err(EditError::Invalid(format!("note {p} does not exist"))),
    }
}

/// An annotation and every reply under it, replies last (deepest first).
fn thread(set: &BTreeMap<String, Annotation>, id: &str) -> Vec<String> {
    let mut out = vec![id.to_string()];
    let mut i = 0;
    while i < out.len() {
        let cur = out[i].clone();
        let children: Vec<String> = set
            .values()
            .filter(|a| a.parent.as_deref() == Some(&cur))
            .map(|a| a.id.clone())
            .filter(|c| !out.contains(c))
            .collect();
        out.extend(children);
        i += 1;
    }
    out.reverse();
    out
}

/// Undo: apply the `before` of each change, last change first. Restored annotations are never
/// pristine (the file may hold a later version of them).
pub fn revert(set: &mut BTreeMap<String, Annotation>, changes: &[Change]) {
    for c in changes.iter().rev() {
        put(set, &c.id, c.before.clone());
    }
}

/// Redo: apply the `after` of each change, in order.
pub fn replay(set: &mut BTreeMap<String, Annotation>, changes: &[Change]) {
    for c in changes {
        put(set, &c.id, c.after.clone());
    }
}

fn put(set: &mut BTreeMap<String, Annotation>, id: &str, a: Option<Annotation>) {
    match a {
        Some(mut a) => {
            a.pristine = false;
            set.insert(id.to_string(), a);
        }
        None => {
            set.remove(id);
        }
    }
}

/// Annotations in display order: page, then creation time, then id.
#[must_use]
pub fn ordered(set: &BTreeMap<String, Annotation>) -> Vec<Annotation> {
    let mut v: Vec<Annotation> = set.values().cloned().collect();
    v.sort_by(|a, b| (a.page, a.created_ms, &a.id).cmp(&(b.page, b.created_ms, &b.id)));
    v
}

// ------------------------------------------------------------------------------- import ---

/// An annotation dictionary as the adapter reads it (PDF user space, unrotated).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RawAnnot {
    /// 0-based page index.
    pub page_index: u32,
    /// `/NM`, or `pdf-<object>-<generation>` when there is none.
    pub key: String,
    /// `/Subtype` name.
    pub subtype: String,
    /// `/Rect`.
    pub rect: [f32; 4],
    /// `/QuadPoints` (8 numbers per quad).
    pub quads: Vec<f32>,
    /// `/InkList`.
    pub ink: Vec<Vec<f32>>,
    /// `/L`.
    pub line: Option<[f32; 4]>,
    /// `/LE` ends with an arrowhead (either end).
    pub arrow: bool,
    /// `/C`.
    pub color: Vec<f32>,
    /// `/CA`.
    pub opacity: Option<f32>,
    /// `/BS /W` (or `/Border` width).
    pub border: Option<f32>,
    /// `/Contents`.
    pub contents: Option<String>,
    /// `/T`.
    pub author: Option<String>,
    /// `/Subj`.
    pub subject: Option<String>,
    /// `/CreationDate`.
    pub created: Option<String>,
    /// `/M`.
    pub modified: Option<String>,
    /// `/IRT`: the key of the annotation this answers.
    pub irt: Option<String>,
    /// `/StateModel`.
    pub state_model: Option<String>,
    /// `/State`.
    pub state: Option<String>,
    /// Font size from `/DA` (`… 12 Tf …`).
    pub font_size: Option<f32>,
    /// `/F` has the hidden bit.
    pub hidden: bool,
}

/// Parse a PDF date (`D:YYYYMMDDHHmmSSOHH'mm`) into ms since the Unix epoch.
#[must_use]
pub fn parse_pdf_date(s: &str) -> Option<i64> {
    let s = s.strip_prefix("D:").unwrap_or(s);
    let digits: String = s.chars().take_while(char::is_ascii_digit).collect();
    if digits.len() < 4 {
        return None;
    }
    let num = |a: usize, b: usize, def: i64| -> i64 {
        digits.get(a..b).and_then(|t| t.parse().ok()).unwrap_or(def)
    };
    let (y, mo, d) = (num(0, 4, 1970), num(4, 6, 1), num(6, 8, 1));
    let (h, mi, se) = (num(8, 10, 0), num(10, 12, 0), num(12, 14, 0));
    if !(1..=12).contains(&mo) || !(1..=31).contains(&d) || h > 23 || mi > 59 || se > 60 {
        return None;
    }
    // Offset after the digits: Z, +HH'mm or -HH'mm.
    let rest = &s[digits.len()..];
    let mut offset_min = 0i64;
    if let Some(sign) = rest.chars().next().filter(|c| *c == '+' || *c == '-') {
        let nums: String = rest[1..].chars().filter(char::is_ascii_digit).collect();
        let oh: i64 = nums.get(0..2).and_then(|t| t.parse().ok()).unwrap_or(0);
        let om: i64 = nums.get(2..4).and_then(|t| t.parse().ok()).unwrap_or(0);
        offset_min = (oh * 60 + om) * if sign == '-' { -1 } else { 1 };
    }
    let days = days_from_civil(y, mo, d);
    Some(((days * 86_400 + h * 3600 + mi * 60 + se) - offset_min * 60) * 1000)
}

/// Days since 1970-01-01 (proleptic Gregorian).
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// A PDF date for ms since the Unix epoch (UTC).
#[must_use]
pub fn pdf_date(ms: i64) -> String {
    let secs = ms.div_euclid(1000);
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    // Inverse of days_from_civil.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!(
        "D:{y:04}{m:02}{d:02}{:02}{:02}{:02}Z",
        rem / 3600,
        rem % 3600 / 60,
        rem % 60
    )
}

fn kind_of(raw: &RawAnnot) -> Option<AnnotationKind> {
    Some(match raw.subtype.as_str() {
        "Highlight" => AnnotationKind::Highlight,
        "Underline" => AnnotationKind::Underline,
        "StrikeOut" => AnnotationKind::StrikeOut,
        "Squiggly" => AnnotationKind::Squiggly,
        "Text" => AnnotationKind::Note,
        "FreeText" => AnnotationKind::FreeText,
        "Ink" => AnnotationKind::Ink,
        "Square" => AnnotationKind::Rectangle,
        "Circle" => AnnotationKind::Ellipse,
        "Line" if raw.arrow => AnnotationKind::Arrow,
        "Line" => AnnotationKind::Line,
        _ => return None,
    })
}

fn is_state(raw: &RawAnnot) -> bool {
    raw.subtype == "Text" && raw.irt.is_some() && raw.state.is_some()
}

/// Turn the annotations found in a PDF into the model. Kinds the Reader does not edit (links,
/// form fields, stamps, popups, …) are skipped here and kept untouched in the file on save;
/// state annotations become `resolved` on their note. `geoms` are the file's pages.
#[must_use]
pub fn import(raws: &[RawAnnot], geoms: &[PageGeom], now_ms: i64) -> Vec<Annotation> {
    let mut out: Vec<Annotation> = Vec::new();
    for raw in raws {
        let Some(kind) = kind_of(raw) else { continue };
        if is_state(raw) || raw.hidden {
            continue;
        }
        let Some(geom) = geoms.get(raw.page_index as usize) else {
            continue;
        };
        let page = raw.page_index + 1;
        let pt = |x: f32, y: f32| {
            let (px, py) = geom.to_page(x, y);
            PagePoint { x: px, y: py }
        };
        let quads: Vec<PageQuad> = raw
            .quads
            .as_chunks::<8>()
            .0
            .iter()
            .map(|q| PageQuad {
                page,
                p: [
                    pt(q[0], q[1]),
                    pt(q[2], q[3]),
                    pt(q[4], q[5]),
                    pt(q[6], q[7]),
                ],
            })
            .collect();
        let paths: Vec<Vec<PagePoint>> = raw
            .ink
            .iter()
            .map(|s| {
                s.as_chunks::<2>()
                    .0
                    .iter()
                    .map(|p| pt(p[0], p[1]))
                    .collect()
            })
            .collect();
        let line = raw.line.map(|l| [pt(l[0], l[1]), pt(l[2], l[3])]);
        let rect = geom.rect(page, raw.rect);
        let created = raw
            .created
            .as_deref()
            .and_then(parse_pdf_date)
            .or_else(|| raw.modified.as_deref().and_then(parse_pdf_date))
            .unwrap_or(now_ms);
        let category = raw.subject.as_deref().and_then(Category::from_label);
        let default_color = match kind {
            AnnotationKind::Note => "#ffd400",
            k if k.is_text_markup() => "#ffd400",
            _ => "#e03131",
        };
        let a = Annotation {
            id: raw.key.clone(),
            page,
            kind,
            rect,
            // Some writers omit /QuadPoints: use the box.
            quads: if kind.is_text_markup() && quads.is_empty() {
                vec![PageQuad::from_rect(&rect)]
            } else {
                quads
            },
            paths,
            line,
            color: if raw.color.is_empty() {
                default_color.into()
            } else {
                color_hex(&raw.color)
            },
            opacity: raw.opacity.unwrap_or(1.0),
            width: raw.border.unwrap_or(1.0),
            font_size: (kind == AnnotationKind::FreeText).then(|| raw.font_size.unwrap_or(12.0)),
            category,
            body: raw.contents.clone().unwrap_or_default(),
            text: None,
            author: raw.author.clone().filter(|s| !s.trim().is_empty()),
            parent: raw.irt.clone().filter(|_| kind == AnnotationKind::Note),
            resolved: false,
            created_ms: created,
            modified_ms: raw
                .modified
                .as_deref()
                .and_then(parse_pdf_date)
                .unwrap_or(created),
            pristine: true,
        };
        if let Ok(a) = normalize(a, u32::try_from(geoms.len()).unwrap_or(u32::MAX)) {
            out.push(a);
        }
    }
    // Replies must point at a note we imported.
    let ids: BTreeSet<String> = out.iter().map(|a| a.id.clone()).collect();
    for a in &mut out {
        if a.parent.as_ref().is_some_and(|p| !ids.contains(p)) {
            a.parent = None;
        }
    }
    // The latest review state of a thread decides `resolved`.
    let mut states: HashMap<&str, (i64, bool)> = HashMap::new();
    for raw in raws.iter().filter(|r| is_state(r)) {
        let (Some(parent), Some(state)) = (raw.irt.as_deref(), raw.state.as_deref()) else {
            continue;
        };
        let at = raw
            .modified
            .as_deref()
            .or(raw.created.as_deref())
            .and_then(parse_pdf_date)
            .unwrap_or(0);
        let done = matches!(state, "Completed" | "Accepted");
        let e = states.entry(parent).or_insert((at, done));
        if at >= e.0 {
            *e = (at, done);
        }
    }
    for a in &mut out {
        if let Some((_, done)) = states.get(a.id.as_str()) {
            a.resolved = *done;
        }
    }
    out
}

// -------------------------------------------------------------------------------- write ---

/// A PDF value, independent of the PDF library.
#[derive(Clone, Debug, PartialEq)]
pub enum PdfValue {
    /// A number.
    Num(f32),
    /// An integer.
    Int(i64),
    /// A boolean.
    Bool(bool),
    /// A name (without the slash).
    Name(String),
    /// A text string (written as UTF-16BE when not ASCII).
    Text(String),
    /// An array.
    Array(Vec<PdfValue>),
    /// A dictionary.
    Dict(Vec<(String, PdfValue)>),
    /// A reference to the annotation object with this key (resolved by the writer).
    AnnotRef(String),
    /// A reference to this annotation's own page.
    PageRef,
}

/// A form `XObject` used as an appearance.
#[derive(Clone, Debug, PartialEq)]
pub struct Appearance {
    /// `/BBox` in user space (equal to `/Rect`, so content uses page coordinates).
    pub bbox: [f32; 4],
    /// `/Resources`.
    pub resources: Vec<(String, PdfValue)>,
    /// Content stream.
    pub content: String,
}

/// One annotation object to append.
#[derive(Clone, Debug, PartialEq)]
pub struct WriteSpec {
    /// 0-based page index.
    pub page_index: u32,
    /// Key of this object (`/NM`); replies refer to it.
    pub key: String,
    /// Dictionary entries (without `/AP`, `/P`, `/Popup`).
    pub dict: Vec<(String, PdfValue)>,
    /// Normal appearance.
    pub appearance: Option<Appearance>,
    /// Add a closed `/Popup` (notes), at this user-space rectangle.
    pub popup: Option<[f32; 4]>,
}

/// What a save writes: new objects, and which annotations already in the file stay.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct WritePlan {
    /// Annotations to append.
    pub specs: Vec<WriteSpec>,
    /// Keys of annotations whose existing object stays as it is.
    pub keep: BTreeSet<String>,
    /// Keys the Reader manages (every id it has ever seen for this paper). Existing objects
    /// with these keys are dropped unless in `keep`; any other existing annotation stays.
    pub managed: BTreeSet<String>,
}

fn num(v: f32) -> PdfValue {
    PdfValue::Num((v * 1000.0).round() / 1000.0)
}

fn nums(v: &[f32]) -> PdfValue {
    PdfValue::Array(v.iter().map(|x| num(*x)).collect())
}

fn f(v: f32) -> String {
    let r = (v * 1000.0).round() / 1000.0;
    let mut s = format!("{r}");
    if s == "-0" {
        s = "0".into();
    }
    s
}

/// Helvetica glyph widths (per 1000 em) for ASCII 32–126.
const HELV: [u16; 95] = [
    278, 278, 355, 556, 556, 889, 667, 191, 333, 333, 389, 584, 278, 333, 278, 278, 556, 556, 556,
    556, 556, 556, 556, 556, 556, 556, 278, 278, 584, 584, 584, 556, 1015, 667, 667, 722, 722, 667,
    611, 778, 722, 278, 500, 667, 556, 833, 722, 778, 667, 778, 722, 667, 611, 722, 667, 944, 667,
    667, 611, 278, 278, 278, 469, 556, 333, 556, 556, 500, 556, 556, 278, 556, 556, 222, 222, 500,
    222, 833, 556, 556, 556, 556, 333, 500, 278, 556, 500, 722, 500, 500, 500, 334, 260, 334, 584,
];

fn helv_width(s: &str, size: f32) -> f32 {
    s.chars()
        .map(|c| {
            let i = (c as usize).wrapping_sub(32);
            f32::from(*HELV.get(i).unwrap_or(&556))
        })
        .sum::<f32>()
        * size
        / 1000.0
}

/// Break text into lines no wider than `width` (Helvetica metrics).
#[must_use]
pub fn wrap(text: &str, size: f32, width: f32) -> Vec<String> {
    let mut lines = Vec::new();
    for para in text.split('\n') {
        let mut cur = String::new();
        for word in para.split(' ') {
            let cand = if cur.is_empty() {
                word.to_string()
            } else {
                format!("{cur} {word}")
            };
            if helv_width(&cand, size) <= width || cur.is_empty() {
                cur = cand;
            } else {
                lines.push(std::mem::take(&mut cur));
                cur = word.to_string();
            }
        }
        lines.push(cur);
    }
    lines
}

/// A literal string for a content stream (Latin-1, escaped).
fn pdf_literal(s: &str) -> String {
    let mut out = String::from("(");
    for c in s.chars() {
        match c {
            '(' | ')' | '\\' => {
                out.push('\\');
                out.push(c);
            }
            c if (c as u32) < 0x20 => {}
            c if (c as u32) < 0x7f => out.push(c),
            c if (0xa0..=0xff).contains(&(c as u32)) => {
                let _ = write!(out, "\\{:03o}", c as u32);
            }
            _ => out.push('?'),
        }
    }
    out.push(')');
    out
}

/// Text Helvetica (`WinAnsiEncoding`) can draw.
fn latin1(s: &str) -> bool {
    s.chars()
        .all(|c| c == '\n' || (' '..='~').contains(&c) || ('\u{a0}'..='\u{ff}').contains(&c))
}

/// Build the write plan for a paper's annotations on a file with pages `geoms`. Replies of a
/// note that is rewritten are rewritten too, so their `/IRT` points at the new object.
#[must_use]
pub fn plan(
    annotations: &[Annotation],
    geoms: &[PageGeom],
    managed: &BTreeSet<String>,
    author: &str,
) -> WritePlan {
    let by_id: HashMap<&str, &Annotation> =
        annotations.iter().map(|a| (a.id.as_str(), a)).collect();
    // An annotation stays as it is when it and every ancestor are pristine.
    let pristine = |a: &Annotation| {
        let mut cur = Some(a);
        let mut depth = 0;
        while let Some(x) = cur {
            if !x.pristine || depth > 64 {
                return false;
            }
            cur = x.parent.as_deref().and_then(|p| by_id.get(p).copied());
            depth += 1;
        }
        true
    };
    let mut out = WritePlan {
        managed: managed.clone(),
        ..WritePlan::default()
    };
    out.managed.extend(annotations.iter().map(|a| a.id.clone()));
    for a in annotations {
        let Some(geom) = geoms.get(a.page.saturating_sub(1) as usize) else {
            continue;
        };
        if pristine(a) {
            out.keep.insert(a.id.clone());
            continue;
        }
        out.specs.extend(spec(a, geom, author));
    }
    out
}

fn spec(a: &Annotation, g: &PageGeom, author: &str) -> Vec<WriteSpec> {
    let u = |p: PagePoint| g.to_user(p.x, p.y);
    let user_rect = |r: &PageRect| {
        let (ax, ay) = g.to_user(r.x0, r.y0);
        let (bx, by) = g.to_user(r.x1, r.y1);
        [ax.min(bx), ay.min(by), ax.max(bx), ay.max(by)]
    };
    let color = parse_color(&a.color).unwrap_or([1.0, 0.83, 0.0]);
    let rgb = format!("{} {} {}", f(color[0]), f(color[1]), f(color[2]));
    let mut rect = user_rect(&a.rect);
    let w = a.width;
    let mut d: Vec<(String, PdfValue)> = vec![
        ("Type".into(), PdfValue::Name("Annot".into())),
        ("NM".into(), PdfValue::Text(a.id.clone())),
        ("F".into(), PdfValue::Int(4)),
        ("C".into(), nums(&color)),
        (
            "T".into(),
            PdfValue::Text(a.author.clone().unwrap_or_else(|| author.to_string())),
        ),
        (
            "CreationDate".into(),
            PdfValue::Text(pdf_date(a.created_ms)),
        ),
        ("M".into(), PdfValue::Text(pdf_date(a.modified_ms))),
    ];
    if !a.body.is_empty() {
        d.push(("Contents".into(), PdfValue::Text(a.body.clone())));
    }
    if let Some(c) = a.category {
        d.push(("Subj".into(), PdfValue::Text(c.label().into())));
    }
    if a.opacity < 1.0 {
        d.push(("CA".into(), num(a.opacity)));
    }
    let gs = |blend: &str| -> Vec<(String, PdfValue)> {
        vec![(
            "ExtGState".into(),
            PdfValue::Dict(vec![(
                "G0".into(),
                PdfValue::Dict(vec![
                    ("Type".into(), PdfValue::Name("ExtGState".into())),
                    ("BM".into(), PdfValue::Name(blend.into())),
                    ("CA".into(), num(a.opacity)),
                    ("ca".into(), num(a.opacity)),
                ]),
            )]),
        )]
    };
    let border = |d: &mut Vec<(String, PdfValue)>| {
        d.push((
            "BS".into(),
            PdfValue::Dict(vec![
                ("W".into(), num(w)),
                ("S".into(), PdfValue::Name("S".into())),
            ]),
        ));
    };
    let mut content = String::new();
    let mut resources = Vec::new();
    let mut popup = None;
    let subtype = match a.kind {
        k if k.is_text_markup() => {
            let mut qp = Vec::with_capacity(a.quads.len() * 8);
            for q in &a.quads {
                // /QuadPoints order used in practice: upper-left, upper-right, lower-left, lower-right.
                for p in q.p {
                    let (x, y) = u(p);
                    qp.push(x);
                    qp.push(y);
                }
            }
            d.push(("QuadPoints".into(), nums(&qp)));
            for q in qp.as_chunks::<8>().0 {
                let (ul, ur, ll, lr) = ((q[0], q[1]), (q[2], q[3]), (q[4], q[5]), (q[6], q[7]));
                match k {
                    AnnotationKind::Highlight => {
                        let _ = write!(
                            content,
                            "{} {} m {} {} l {} {} l {} {} l h f ",
                            f(ul.0),
                            f(ul.1),
                            f(ur.0),
                            f(ur.1),
                            f(lr.0),
                            f(lr.1),
                            f(ll.0),
                            f(ll.1)
                        );
                    }
                    AnnotationKind::Underline => {
                        let t = (((ul.0 - ll.0).powi(2) + (ul.1 - ll.1).powi(2)).sqrt() / 14.0)
                            .max(0.6);
                        let _ = write!(
                            content,
                            "{} w {} {} m {} {} l S ",
                            f(t),
                            f(ll.0),
                            f(ll.1 + t),
                            f(lr.0),
                            f(lr.1 + t)
                        );
                    }
                    AnnotationKind::StrikeOut => {
                        let t = (((ul.0 - ll.0).powi(2) + (ul.1 - ll.1).powi(2)).sqrt() / 14.0)
                            .max(0.6);
                        let _ = write!(
                            content,
                            "{} w {} {} m {} {} l S ",
                            f(t),
                            f(f32::midpoint(ul.0, ll.0)),
                            f(f32::midpoint(ul.1, ll.1)),
                            f(f32::midpoint(ur.0, lr.0)),
                            f(f32::midpoint(ur.1, lr.1))
                        );
                    }
                    _ => {
                        // Squiggly: a zigzag along the bottom edge.
                        let len = ((lr.0 - ll.0).powi(2) + (lr.1 - ll.1).powi(2))
                            .sqrt()
                            .max(0.1);
                        let (dx, dy) = ((lr.0 - ll.0) / len, (lr.1 - ll.1) / len);
                        let amp = (((ul.0 - ll.0).powi(2) + (ul.1 - ll.1).powi(2)).sqrt() / 12.0)
                            .max(0.8);
                        let step = amp * 2.0;
                        let _ = write!(content, "{} w {} {} m ", f(amp / 2.0), f(ll.0), f(ll.1));
                        let mut s = step;
                        let mut up = true;
                        while s <= len {
                            let off = if up { amp } else { 0.0 };
                            let _ = write!(
                                content,
                                "{} {} l ",
                                f(ll.0 + dx * s - dy * off),
                                f(ll.1 + dy * s + dx * off)
                            );
                            s += step;
                            up = !up;
                        }
                        content.push_str("S ");
                    }
                }
            }
            if k == AnnotationKind::Highlight {
                resources = gs("Multiply");
                content = format!("/G0 gs {rgb} rg {content}");
            } else {
                content = format!("{rgb} RG {content}");
                if a.opacity < 1.0 {
                    resources = gs("Normal");
                    content = format!("/G0 gs {content}");
                }
            }
            rect = pad4(rect, 1.0);
            match k {
                AnnotationKind::Highlight => "Highlight",
                AnnotationKind::Underline => "Underline",
                AnnotationKind::StrikeOut => "StrikeOut",
                _ => "Squiggly",
            }
        }
        AnnotationKind::Note => {
            d.push(("Name".into(), PdfValue::Name("Comment".into())));
            d.push(("Open".into(), PdfValue::Bool(false)));
            if let Some(p) = &a.parent {
                d.push(("IRT".into(), PdfValue::AnnotRef(p.clone())));
                d.push(("RT".into(), PdfValue::Name("R".into())));
            } else {
                // The icon: a speech bubble.
                let [x0, y0, x1, y1] = rect;
                let (wd, ht) = (x1 - x0, y1 - y0);
                let _ = write!(
                    content,
                    "{rgb} rg 0.25 0.25 0.25 RG 0.8 w {} {} m {} {} l {} {} l {} {} l {} {} l {} {} l h B \
                     0.25 0.25 0.25 RG 0.8 w {} {} m {} {} l {} {} m {} {} l S",
                    f(x0 + 1.0),
                    f(y1 - 1.0),
                    f(x1 - 1.0),
                    f(y1 - 1.0),
                    f(x1 - 1.0),
                    f(y0 + ht * 0.3),
                    f(x0 + wd * 0.45),
                    f(y0 + ht * 0.3),
                    f(x0 + wd * 0.25),
                    f(y0 + 1.0),
                    f(x0 + 1.0),
                    f(y0 + ht * 0.3),
                    f(x0 + wd * 0.2),
                    f(y0 + ht * 0.72),
                    f(x1 - wd * 0.2),
                    f(y0 + ht * 0.72),
                    f(x0 + wd * 0.2),
                    f(y0 + ht * 0.52),
                    f(x1 - wd * 0.35),
                    f(y0 + ht * 0.52)
                );
                popup = Some([rect[2], rect[3] - 120.0, rect[2] + 200.0, rect[3]]);
            }
            "Text"
        }
        AnnotationKind::FreeText => {
            let size = a.font_size.unwrap_or(12.0);
            d.push((
                "DA".into(),
                PdfValue::Text(format!("/Helv {} Tf {rgb} rg", f(size))),
            ));
            d.push(("Q".into(), PdfValue::Int(0)));
            d.push((
                "BS".into(),
                PdfValue::Dict(vec![("W".into(), PdfValue::Int(0))]),
            ));
            // Appearance only for text Helvetica can draw; other viewers build their own from
            // /DA and /Contents otherwise (embedded Unicode font: P1.M3.PDF.5).
            if latin1(&a.body) {
                resources = vec![(
                    "Font".into(),
                    PdfValue::Dict(vec![(
                        "Helv".into(),
                        PdfValue::Dict(vec![
                            ("Type".into(), PdfValue::Name("Font".into())),
                            ("Subtype".into(), PdfValue::Name("Type1".into())),
                            ("BaseFont".into(), PdfValue::Name("Helvetica".into())),
                            ("Encoding".into(), PdfValue::Name("WinAnsiEncoding".into())),
                        ]),
                    )]),
                )];
                // The box in page space keeps its upright size even on rotated pages.
                let pw = a.rect.width() - 4.0;
                let lines = wrap(&a.body, size, pw.max(size));
                let lead = size * 1.2;
                let (ox, oy) = u(PagePoint {
                    x: a.rect.x0 + 2.0,
                    y: a.rect.y0 + 2.0 + size,
                });
                let rot = match g.rotation {
                    90 => "0 1 -1 0",
                    180 => "-1 0 0 -1",
                    270 => "0 -1 1 0",
                    _ => "1 0 0 1",
                };
                let _ = write!(
                    content,
                    "BT /Helv {} Tf {rgb} rg {rot} {} {} Tm {} TL ",
                    f(size),
                    f(ox),
                    f(oy),
                    f(lead)
                );
                for (i, line) in lines.iter().enumerate() {
                    if i > 0 {
                        content.push_str("T* ");
                    }
                    let _ = write!(content, "{} Tj ", pdf_literal(line));
                }
                content.push_str("ET");
            }
            "FreeText"
        }
        AnnotationKind::Ink => {
            let list: Vec<PdfValue> = a
                .paths
                .iter()
                .map(|s| {
                    let mut v = Vec::with_capacity(s.len() * 2);
                    for p in s {
                        let (x, y) = u(*p);
                        v.push(x);
                        v.push(y);
                    }
                    nums(&v)
                })
                .collect();
            d.push(("InkList".into(), PdfValue::Array(list)));
            border(&mut d);
            let _ = write!(content, "{rgb} RG {} w 1 J 1 j ", f(w));
            for s in &a.paths {
                for (i, p) in s.iter().enumerate() {
                    let (x, y) = u(*p);
                    let _ = write!(
                        content,
                        "{} {} {} ",
                        f(x),
                        f(y),
                        if i == 0 { "m" } else { "l" }
                    );
                }
                if s.len() == 1 {
                    let (x, y) = u(s[0]);
                    let _ = write!(content, "{} {} l ", f(x + 0.01), f(y));
                }
                content.push_str("S ");
            }
            "Ink"
        }
        AnnotationKind::Rectangle | AnnotationKind::Ellipse => {
            border(&mut d);
            let [x0, y0, x1, y1] = rect;
            let h = w / 2.0;
            let _ = write!(content, "{rgb} RG {} w ", f(w));
            if a.kind == AnnotationKind::Rectangle {
                let _ = write!(
                    content,
                    "{} {} {} {} re S",
                    f(x0 + h),
                    f(y0 + h),
                    f(x1 - x0 - w),
                    f(y1 - y0 - w)
                );
            } else {
                // Four Bézier arcs.
                let (cx, cy) = (f32::midpoint(x0, x1), f32::midpoint(y0, y1));
                let (rx, ry) = (
                    ((x1 - x0) / 2.0 - h).max(0.1),
                    ((y1 - y0) / 2.0 - h).max(0.1),
                );
                let k = 0.552_284_8;
                let _ = write!(
                    content,
                    "{} {} m {} {} {} {} {} {} c {} {} {} {} {} {} c {} {} {} {} {} {} c {} {} {} {} {} {} c S",
                    f(cx + rx),
                    f(cy),
                    f(cx + rx),
                    f(cy + ry * k),
                    f(cx + rx * k),
                    f(cy + ry),
                    f(cx),
                    f(cy + ry),
                    f(cx - rx * k),
                    f(cy + ry),
                    f(cx - rx),
                    f(cy + ry * k),
                    f(cx - rx),
                    f(cy),
                    f(cx - rx),
                    f(cy - ry * k),
                    f(cx - rx * k),
                    f(cy - ry),
                    f(cx),
                    f(cy - ry),
                    f(cx + rx * k),
                    f(cy - ry),
                    f(cx + rx),
                    f(cy - ry * k),
                    f(cx + rx),
                    f(cy)
                );
            }
            if a.kind == AnnotationKind::Rectangle {
                "Square"
            } else {
                "Circle"
            }
        }
        AnnotationKind::Line | AnnotationKind::Arrow => {
            let [s, e] = a.line.unwrap_or([
                PagePoint {
                    x: a.rect.x0,
                    y: a.rect.y0,
                },
                PagePoint {
                    x: a.rect.x1,
                    y: a.rect.y1,
                },
            ]);
            let ((x1, y1), (x2, y2)) = (u(s), u(e));
            d.push(("L".into(), nums(&[x1, y1, x2, y2])));
            border(&mut d);
            let arrow = a.kind == AnnotationKind::Arrow;
            d.push((
                "LE".into(),
                PdfValue::Array(vec![
                    PdfValue::Name("None".into()),
                    PdfValue::Name(if arrow { "OpenArrow" } else { "None" }.into()),
                ]),
            ));
            let _ = write!(
                content,
                "{rgb} RG {} w 1 J 1 j {} {} m {} {} l S",
                f(w),
                f(x1),
                f(y1),
                f(x2),
                f(y2)
            );
            if arrow {
                let (dx, dy) = (x2 - x1, y2 - y1);
                let len = (dx * dx + dy * dy).sqrt().max(0.01);
                let (ux, uy) = (dx / len, dy / len);
                let (head, half) = (3.0 + w * 3.0, 1.5 + w * 1.5);
                let (ax, ay) = (x2 - head * ux + half * uy, y2 - head * uy - half * ux);
                let (bx, by) = (x2 - head * ux - half * uy, y2 - head * uy + half * ux);
                let _ = write!(
                    content,
                    " {} {} m {} {} l {} {} l S",
                    f(ax),
                    f(ay),
                    f(x2),
                    f(y2),
                    f(bx),
                    f(by)
                );
            }
            "Line"
        }
        _ => "Text",
    };
    d.push(("Subtype".into(), PdfValue::Name(subtype.into())));
    d.push(("Rect".into(), nums(&rect)));
    let appearance = (!content.is_empty()).then_some(Appearance {
        bbox: rect,
        resources,
        content,
    });
    let mut specs = vec![WriteSpec {
        page_index: a.page - 1,
        key: a.id.clone(),
        dict: d,
        appearance,
        popup,
    }];
    // A resolved thread: a state annotation answering the note (ADR-006 §3).
    if a.kind == AnnotationKind::Note && a.parent.is_none() && a.resolved {
        specs.push(WriteSpec {
            page_index: a.page - 1,
            key: format!("{}-state", a.id),
            dict: vec![
                ("Type".into(), PdfValue::Name("Annot".into())),
                ("Subtype".into(), PdfValue::Name("Text".into())),
                ("Rect".into(), nums(&rect)),
                ("NM".into(), PdfValue::Text(format!("{}-state", a.id))),
                ("F".into(), PdfValue::Int(4 | 2)),
                ("IRT".into(), PdfValue::AnnotRef(a.id.clone())),
                ("StateModel".into(), PdfValue::Text("Review".into())),
                ("State".into(), PdfValue::Text("Completed".into())),
                (
                    "T".into(),
                    PdfValue::Text(a.author.clone().unwrap_or_else(|| author.to_string())),
                ),
                ("Contents".into(), PdfValue::Text("Completed".into())),
                ("M".into(), PdfValue::Text(pdf_date(a.modified_ms))),
            ],
            appearance: None,
            popup: None,
        });
    }
    specs
}

fn pad4(r: [f32; 4], p: f32) -> [f32; 4] {
    [r[0] - p, r[1] - p, r[2] + p, r[3] + p]
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::cast_precision_loss
)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn geom() -> PageGeom {
        PageGeom {
            rotation: 0,
            crop: [0.0, 0.0, 600.0, 800.0],
        }
    }

    fn base(id: &str, kind: AnnotationKind) -> Annotation {
        let rect = PageRect {
            page: 1,
            x0: 100.0,
            y0: 100.0,
            x1: 200.0,
            y1: 140.0,
        };
        Annotation {
            id: id.into(),
            page: 1,
            kind,
            rect,
            quads: if kind.is_text_markup() {
                vec![PageQuad::from_rect(&rect)]
            } else {
                vec![]
            },
            paths: if kind == AnnotationKind::Ink {
                vec![vec![
                    PagePoint { x: 10.0, y: 10.0 },
                    PagePoint { x: 30.0, y: 40.0 },
                ]]
            } else {
                vec![]
            },
            line: matches!(kind, AnnotationKind::Line | AnnotationKind::Arrow).then_some([
                PagePoint { x: 10.0, y: 10.0 },
                PagePoint { x: 90.0, y: 50.0 },
            ]),
            color: "#FFD400".into(),
            opacity: 1.0,
            width: 1.5,
            font_size: (kind == AnnotationKind::FreeText).then_some(11.0),
            category: Some(Category::Method),
            body: "a comment".into(),
            text: None,
            author: None,
            parent: None,
            resolved: false,
            created_ms: 0,
            modified_ms: 0,
            pristine: false,
        }
    }

    const KINDS: [AnnotationKind; 11] = [
        AnnotationKind::Highlight,
        AnnotationKind::Underline,
        AnnotationKind::StrikeOut,
        AnnotationKind::Squiggly,
        AnnotationKind::Note,
        AnnotationKind::FreeText,
        AnnotationKind::Ink,
        AnnotationKind::Rectangle,
        AnnotationKind::Ellipse,
        AnnotationKind::Line,
        AnnotationKind::Arrow,
    ];

    #[test]
    fn normalize_recomputes_bounds_and_checks() {
        let a = normalize(base("a", AnnotationKind::Ink), 3).unwrap();
        assert!(a.rect.x0 < 10.0 && a.rect.y1 > 40.0);
        assert_eq!(a.color, "#ffd400");
        let mut bad = base("b", AnnotationKind::Highlight);
        bad.quads.clear();
        assert!(normalize(bad, 3).is_err());
        let mut bad = base("c", AnnotationKind::Note);
        bad.page = 4;
        assert!(normalize(bad, 3).is_err());
        let mut bad = base("d", AnnotationKind::Note);
        bad.color = "red".into();
        assert!(normalize(bad, 3).is_err());
    }

    #[test]
    fn remove_takes_replies_and_undo_restores() {
        let mut set = BTreeMap::new();
        let note = base("n", AnnotationKind::Note);
        let mut reply = base("r", AnnotationKind::Note);
        reply.parent = Some("n".into());
        let c1 = apply(
            &mut set,
            vec![
                AnnotationOp::Add { annotation: note },
                AnnotationOp::Add { annotation: reply },
            ],
            2,
            5,
        )
        .unwrap();
        assert_eq!(set.len(), 2);
        let before = set.clone();
        let c2 = apply(
            &mut set,
            vec![AnnotationOp::Remove { id: "n".into() }],
            2,
            6,
        )
        .unwrap();
        assert!(set.is_empty());
        assert_eq!(c2.len(), 2);
        revert(&mut set, &c2);
        assert_eq!(set, before);
        revert(&mut set, &c1);
        assert!(set.is_empty());
        replay(&mut set, &c1);
        assert_eq!(set, before);
    }

    #[test]
    fn failed_edit_changes_nothing() {
        let mut set = BTreeMap::new();
        apply(
            &mut set,
            vec![AnnotationOp::Add {
                annotation: base("a", AnnotationKind::Note),
            }],
            1,
            1,
        )
        .unwrap();
        let before = set.clone();
        let r = apply(
            &mut set,
            vec![
                AnnotationOp::Remove { id: "a".into() },
                AnnotationOp::Change {
                    annotation: base("zz", AnnotationKind::Note),
                },
            ],
            1,
            2,
        );
        assert!(matches!(r, Err(EditError::Conflict(_))));
        assert_eq!(set, before);
    }

    fn op_strategy() -> impl Strategy<Value = (u8, u8, u8)> {
        (0u8..3, 0u8..6, 0u8..11)
    }

    proptest! {
        /// P1.M3.MOD.2: any sequence of edits followed by a full undo gives the original state.
        #[test]
        fn full_undo_restores_original(seq in proptest::collection::vec(op_strategy(), 1..40), seed in 0usize..4) {
            let mut set = BTreeMap::new();
            for i in 0..seed {
                apply(&mut set, vec![AnnotationOp::Add { annotation: base(&format!("s{i}"), KINDS[i % 11]) }], 3, 1).unwrap();
            }
            let original = set.clone();
            let mut history: Vec<Vec<Change>> = Vec::new();
            for (n, (op, slot, kind)) in seq.into_iter().enumerate() {
                let id = format!("x{slot}");
                let mut a = base(&id, KINDS[kind as usize]);
                a.body = format!("edit {n}");
                let ops = match op {
                    0 => vec![AnnotationOp::Add { annotation: a }],
                    1 => vec![AnnotationOp::Change { annotation: a }],
                    _ => vec![AnnotationOp::Remove { id }],
                };
                if let Ok(ch) = apply(&mut set, ops, 3, i64::try_from(n).unwrap() + 10) {
                    history.push(ch);
                }
            }
            let end = set.clone();
            for ch in history.iter().rev() {
                revert(&mut set, ch);
            }
            prop_assert_eq!(&set, &original.iter().map(|(k, v)| (k.clone(), Annotation { pristine: false, ..v.clone() })).collect::<BTreeMap<_, _>>());
            for ch in &history {
                replay(&mut set, ch);
            }
            prop_assert_eq!(set, end);
        }
    }

    #[test]
    fn dates_round_trip() {
        assert_eq!(parse_pdf_date("D:19700101000000Z"), Some(0));
        assert_eq!(
            parse_pdf_date("D:20261112120000+01'00"),
            parse_pdf_date("D:20261112110000Z")
        );
        let ms = 1_763_000_000_000;
        assert_eq!(parse_pdf_date(&pdf_date(ms)), Some(ms));
        assert_eq!(parse_pdf_date("garbage"), None);
    }

    #[test]
    fn every_kind_has_a_spec_and_round_trips_through_import() {
        let g = [geom()];
        for (i, k) in KINDS.iter().enumerate() {
            let mut a = normalize(base(&format!("ep-{i}"), *k), 1).unwrap();
            a.resolved = *k == AnnotationKind::Note;
            let specs = spec(&a, &g[0], "Reader");
            assert!(!specs.is_empty(), "{k:?}");
            assert!(specs[0].appearance.is_some(), "{k:?} has an appearance");
            // Read the spec back as the adapter would.
            let raws: Vec<RawAnnot> = specs.iter().map(raw_from_spec).collect();
            let back = import(&raws, &g, 0);
            assert_eq!(back.len(), 1, "{k:?}");
            let b = &back[0];
            assert_eq!(b.kind, *k);
            assert_eq!(b.id, a.id);
            assert_eq!(b.color, a.color);
            assert_eq!(b.category, a.category);
            assert_eq!(b.body, a.body);
            assert_eq!(b.resolved, a.resolved);
            for (p, q) in b.quads.iter().zip(&a.quads) {
                for (x, y) in p.p.iter().zip(&q.p) {
                    assert!((x.x - y.x).abs() < 0.01 && (x.y - y.y).abs() < 0.01);
                }
            }
        }
    }

    #[test]
    fn rotated_pages_round_trip() {
        for rotation in [90, 180, 270] {
            let g = [PageGeom {
                rotation,
                crop: [10.0, 20.0, 610.0, 820.0],
            }];
            let a = normalize(base("ep-r", AnnotationKind::Highlight), 1).unwrap();
            let specs = spec(&a, &g[0], "Reader");
            let back = import(&[raw_from_spec(&specs[0])], &g, 0);
            let (p, q) = (&back[0].quads[0], &a.quads[0]);
            for (x, y) in p.p.iter().zip(&q.p) {
                assert!(
                    (x.x - y.x).abs() < 0.01 && (x.y - y.y).abs() < 0.01,
                    "rotation {rotation}"
                );
            }
        }
    }

    #[test]
    fn plan_keeps_pristine_and_rewrites_replies_of_changed_notes() {
        let g = [geom()];
        let mut n = normalize(base("n", AnnotationKind::Note), 1).unwrap();
        let mut r = normalize(base("r", AnnotationKind::Note), 1).unwrap();
        r.parent = Some("n".into());
        let mut h = normalize(base("h", AnnotationKind::Highlight), 1).unwrap();
        n.pristine = false;
        r.pristine = true;
        h.pristine = true;
        let managed: BTreeSet<String> = ["gone".to_string()].into();
        let p = plan(&[n, r, h], &g, &managed, "Reader");
        assert!(p.keep.contains("h"));
        assert!(
            !p.keep.contains("r"),
            "a reply of a rewritten note is rewritten"
        );
        assert!(p.managed.contains("gone") && p.managed.contains("n"));
        assert_eq!(p.specs.len(), 2);
    }

    #[test]
    fn wrap_breaks_long_text() {
        let lines = wrap("one two three four five six seven", 12.0, 60.0);
        assert!(lines.len() > 2);
        assert!(
            lines
                .iter()
                .all(|l| helv_width(l, 12.0) <= 60.0 || !l.contains(' '))
        );
        assert_eq!(pdf_literal("a(b)\\é"), "(a\\(b\\)\\\\\\351)");
    }

    /// What the adapter would read back from a written spec.
    fn raw_from_spec(s: &WriteSpec) -> RawAnnot {
        let get = |k: &str| s.dict.iter().find(|(n, _)| n == k).map(|(_, v)| v.clone());
        let nums = |v: Option<PdfValue>| -> Vec<f32> {
            match v {
                Some(PdfValue::Array(a)) => a
                    .into_iter()
                    .filter_map(|x| match x {
                        PdfValue::Num(n) => Some(n),
                        PdfValue::Int(i) => Some(i as f32),
                        _ => None,
                    })
                    .collect(),
                _ => vec![],
            }
        };
        let text = |v: Option<PdfValue>| match v {
            Some(PdfValue::Text(t) | PdfValue::Name(t)) => Some(t),
            _ => None,
        };
        let rect = nums(get("Rect"));
        let line = nums(get("L"));
        RawAnnot {
            page_index: s.page_index,
            key: s.key.clone(),
            subtype: text(get("Subtype")).unwrap_or_default(),
            rect: [rect[0], rect[1], rect[2], rect[3]],
            quads: nums(get("QuadPoints")),
            ink: match get("InkList") {
                Some(PdfValue::Array(a)) => a.into_iter().map(|x| nums(Some(x))).collect(),
                _ => vec![],
            },
            line: (line.len() == 4).then(|| [line[0], line[1], line[2], line[3]]),
            arrow: matches!(get("LE"), Some(PdfValue::Array(v)) if v.contains(&PdfValue::Name("OpenArrow".into()))),
            color: nums(get("C")),
            opacity: None,
            border: match get("BS") {
                Some(PdfValue::Dict(d)) => {
                    d.iter().find(|(k, _)| k == "W").and_then(|(_, v)| match v {
                        PdfValue::Num(n) => Some(*n),
                        _ => None,
                    })
                }
                _ => None,
            },
            contents: text(get("Contents")),
            author: text(get("T")),
            subject: text(get("Subj")),
            created: text(get("CreationDate")),
            modified: text(get("M")),
            irt: match get("IRT") {
                Some(PdfValue::AnnotRef(k)) => Some(k),
                _ => None,
            },
            state_model: text(get("StateModel")),
            state: text(get("State")),
            font_size: s.dict.iter().any(|(k, _)| k == "DA").then_some(11.0),
            hidden: false,
        }
    }
}
