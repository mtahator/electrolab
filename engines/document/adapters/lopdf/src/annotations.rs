//! Annotations in PDF files (P1.M3.MOD.4, P1.M3.PDF.1–3; ADR-006).
//!
//! Reading: every page's `/Annots` into [`RawAnnot`] (user space, as written).
//! Writing: an **incremental update** — the original bytes stay as a prefix; new annotation
//! objects, their appearance streams and a rewritten `/Annots` array per page are appended.
//! Objects the Reader does not manage (links, form fields, stamps, other tools' marks the user
//! has not touched) stay referenced exactly as they were.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::io::Write;
use std::path::Path;

use ep_document_core::annotations::{PdfValue, RawAnnot, WritePlan};
use lopdf::{Dictionary, Document, IncrementalDocument, Object, ObjectId, Stream, StringFormat};

use crate::{ReadError, decode_pdf_string};

fn err(e: impl std::fmt::Display) -> ReadError {
    ReadError(e.to_string())
}

fn resolve<'a>(doc: &'a Document, o: &'a Object) -> Option<&'a Object> {
    doc.dereference(o).ok().map(|(_, o)| o)
}

fn text(doc: &Document, d: &Dictionary, key: &[u8]) -> Option<String> {
    match resolve(doc, d.get(key).ok()?)? {
        Object::String(b, _) => Some(decode_pdf_string(b)),
        Object::Name(n) => Some(String::from_utf8_lossy(n).into_owned()),
        _ => None,
    }
}

fn num(doc: &Document, o: &Object) -> Option<f32> {
    resolve(doc, o)?.as_float().ok().filter(|v| v.is_finite())
}

fn numbers(doc: &Document, d: &Dictionary, key: &[u8]) -> Vec<f32> {
    d.get(key)
        .ok()
        .and_then(|o| resolve(doc, o))
        .and_then(|o| o.as_array().ok())
        .map(|a| a.iter().filter_map(|x| num(doc, x)).collect())
        .unwrap_or_default()
}

/// The annotation references of a page (inline or referenced `/Annots` array).
fn annots_of(doc: &Document, page: &Dictionary) -> Vec<Object> {
    match page.get(b"Annots") {
        Ok(Object::Array(v)) => v.clone(),
        Ok(Object::Reference(id)) => doc
            .get_object(*id)
            .ok()
            .and_then(|o| o.as_array().ok())
            .cloned()
            .unwrap_or_default(),
        _ => Vec::new(),
    }
}

/// The key of an annotation: `/NM`, else `pdf-<object>-<generation>`.
fn key_of(doc: &Document, id: Option<ObjectId>, d: &Dictionary) -> Option<String> {
    text(doc, d, b"NM")
        .filter(|s| !s.trim().is_empty())
        .or_else(|| id.map(|(n, g)| format!("pdf-{n}-{g}")))
}

fn font_size(da: &str) -> Option<f32> {
    let toks: Vec<&str> = da.split_whitespace().collect();
    toks.windows(2)
        .find(|w| w[1] == "Tf")
        .and_then(|w| w[0].parse().ok())
        .filter(|v: &f32| v.is_finite() && *v > 0.0)
}

fn load(path: &Path) -> Result<Document, ReadError> {
    let doc = Document::load(path).map_err(err)?;
    if doc.is_encrypted() {
        return Err(ReadError("the PDF is encrypted".into()));
    }
    Ok(doc)
}

/// Read every annotation of every page.
///
/// # Errors
/// The file cannot be read, is encrypted, or is not a PDF lopdf understands.
pub fn read_annotations(path: &Path) -> Result<Vec<RawAnnot>, ReadError> {
    let doc = load(path)?;
    Ok(read_doc(&doc))
}

fn read_doc(doc: &Document) -> Vec<RawAnnot> {
    let mut out = Vec::new();
    for (n, page_id) in doc.get_pages() {
        let Ok(page) = doc.get_object(page_id).and_then(Object::as_dict) else {
            continue;
        };
        for r in annots_of(doc, page) {
            let Ok((id, obj)) = doc.dereference(&r) else {
                continue;
            };
            let Ok(d) = obj.as_dict() else { continue };
            let Some(key) = key_of(doc, id, d) else {
                continue;
            };
            let subtype = text(doc, d, b"Subtype").unwrap_or_default();
            let rect = numbers(doc, d, b"Rect");
            if rect.len() != 4 {
                continue;
            }
            let ink = d
                .get(b"InkList")
                .ok()
                .and_then(|o| resolve(doc, o))
                .and_then(|o| o.as_array().ok())
                .map(|a| {
                    a.iter()
                        .filter_map(|s| resolve(doc, s)?.as_array().ok())
                        .map(|s| s.iter().filter_map(|x| num(doc, x)).collect())
                        .collect()
                })
                .unwrap_or_default();
            let line = numbers(doc, d, b"L");
            let le = d
                .get(b"LE")
                .ok()
                .and_then(|o| resolve(doc, o))
                .and_then(|o| o.as_array().ok())
                .is_some_and(|a| {
                    a.iter()
                        .filter_map(|x| x.as_name().ok())
                        .any(|n| n.ends_with(b"Arrow"))
                });
            let border = d
                .get(b"BS")
                .ok()
                .and_then(|o| resolve(doc, o))
                .and_then(|o| o.as_dict().ok())
                .and_then(|bs| bs.get(b"W").ok())
                .and_then(|w| num(doc, w))
                .or_else(|| numbers(doc, d, b"Border").get(2).copied());
            let irt = d
                .get(b"IRT")
                .ok()
                .and_then(|o| doc.dereference(o).ok())
                .and_then(|(pid, o)| key_of(doc, pid, o.as_dict().ok()?));
            let flags = d
                .get(b"F")
                .ok()
                .and_then(|o| resolve(doc, o)?.as_i64().ok())
                .unwrap_or(0);
            out.push(RawAnnot {
                page_index: n - 1,
                key,
                subtype,
                rect: [rect[0], rect[1], rect[2], rect[3]],
                quads: numbers(doc, d, b"QuadPoints"),
                ink,
                line: (line.len() == 4).then(|| [line[0], line[1], line[2], line[3]]),
                arrow: le,
                color: numbers(doc, d, b"C"),
                opacity: d.get(b"CA").ok().and_then(|o| num(doc, o)),
                border,
                contents: text(doc, d, b"Contents"),
                author: text(doc, d, b"T"),
                subject: text(doc, d, b"Subj"),
                created: text(doc, d, b"CreationDate"),
                modified: text(doc, d, b"M"),
                irt,
                state_model: text(doc, d, b"StateModel"),
                state: text(doc, d, b"State"),
                font_size: text(doc, d, b"DA").as_deref().and_then(font_size),
                hidden: flags & 2 != 0,
            });
        }
    }
    out
}

fn pdf_text(s: &str) -> Object {
    if s.is_ascii() {
        Object::String(s.as_bytes().to_vec(), StringFormat::Literal)
    } else {
        // PDF text string: UTF-16BE with a byte-order mark.
        let mut b = vec![0xFE, 0xFF];
        for u in s.encode_utf16() {
            b.extend_from_slice(&u.to_be_bytes());
        }
        Object::String(b, StringFormat::Hexadecimal)
    }
}

fn to_object(v: &PdfValue, refs: &HashMap<String, ObjectId>, page: ObjectId) -> Object {
    match v {
        PdfValue::Num(n) => Object::Real(*n),
        PdfValue::Int(i) => Object::Integer(*i),
        PdfValue::Bool(b) => Object::Boolean(*b),
        PdfValue::Name(n) => Object::Name(n.as_bytes().to_vec()),
        PdfValue::Text(t) => pdf_text(t),
        PdfValue::Array(a) => Object::Array(a.iter().map(|x| to_object(x, refs, page)).collect()),
        PdfValue::Dict(d) => Object::Dictionary(to_dict(d, refs, page)),
        PdfValue::AnnotRef(k) => refs
            .get(k)
            .map_or(Object::Null, |id| Object::Reference(*id)),
        PdfValue::PageRef => Object::Reference(page),
    }
}

fn to_dict(
    d: &[(String, PdfValue)],
    refs: &HashMap<String, ObjectId>,
    page: ObjectId,
) -> Dictionary {
    let mut out = Dictionary::new();
    for (k, v) in d {
        out.set(k.as_bytes().to_vec(), to_object(v, refs, page));
    }
    out
}

/// What [`write_annotations`] did.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WriteReport {
    /// New annotation objects (state annotations included).
    pub written: u32,
    /// Existing annotations kept because they are unchanged.
    pub kept: u32,
    /// Existing annotations dropped from `/Annots` (changed or deleted in the Reader).
    pub dropped: u32,
}

#[allow(clippy::too_many_lines)] // one pass: collect, decide, append, rewrite pages
/// Write `plan` as an incremental update of `input` into `out`.
///
/// # Errors
/// The file cannot be read (or is encrypted), a page in the plan does not exist, or writing fails.
pub fn write_annotations(
    input: &Path,
    plan: &WritePlan,
    out: &mut impl Write,
) -> Result<WriteReport, ReadError> {
    let mut inc = IncrementalDocument::load(input).map_err(err)?;
    if inc.get_prev_documents().is_encrypted() {
        return Err(ReadError("the PDF is encrypted".into()));
    }
    let pages = inc.get_prev_documents().get_pages();
    let mut report = WriteReport::default();

    // Pages touched: any page with a new object, or with an existing annotation we manage.
    let mut touched: BTreeMap<u32, ObjectId> = BTreeMap::new();
    for s in &plan.specs {
        let n = s.page_index + 1;
        let id = *pages
            .get(&n)
            .ok_or_else(|| ReadError(format!("page {n} does not exist")))?;
        touched.insert(n, id);
    }
    // Existing annotations: which to keep on each page, and the object of every key.
    let mut refs: HashMap<String, ObjectId> = HashMap::new();
    let mut kept_lists: BTreeMap<u32, Vec<Object>> = BTreeMap::new();
    {
        let prev = inc.get_prev_documents();
        for (&n, &page_id) in &pages {
            let Ok(page) = prev.get_object(page_id).and_then(Object::as_dict) else {
                continue;
            };
            let list = annots_of(prev, page);
            let info: Vec<(Object, Option<ObjectId>, Option<&Dictionary>)> = list
                .iter()
                .map(|r| match prev.dereference(r) {
                    Ok((id, o)) => (r.clone(), id, o.as_dict().ok()),
                    Err(_) => (r.clone(), None, None),
                })
                .collect();
            // Keys of the annotations on this page that are dropped.
            let mut dropped_ids: BTreeSet<ObjectId> = BTreeSet::new();
            let mut changed = false;
            for (_, id, d) in &info {
                let (Some(d), Some(id)) = (d, id) else {
                    continue;
                };
                if text(prev, d, b"Subtype").as_deref() == Some("Popup") {
                    continue;
                }
                let Some(key) = key_of(prev, Some(*id), d) else {
                    continue;
                };
                refs.insert(key.clone(), *id);
                let is_state = d.has(b"StateModel") || d.has(b"State");
                let drop = if is_state {
                    // Review states of a thread we manage are rewritten from the model.
                    let parent = d
                        .get(b"IRT")
                        .ok()
                        .and_then(|o| prev.dereference(o).ok())
                        .and_then(|(pid, o)| key_of(prev, pid, o.as_dict().ok()?));
                    parent.is_some_and(|p| plan.managed.contains(&p) && !plan.keep.contains(&p))
                } else {
                    plan.managed.contains(&key) && !plan.keep.contains(&key)
                };
                if drop {
                    dropped_ids.insert(*id);
                    changed = true;
                }
            }
            if !changed && !touched.contains_key(&n) {
                continue;
            }
            let mut keep = Vec::new();
            for (r, id, d) in &info {
                let dropped = id.is_some_and(|i| dropped_ids.contains(&i));
                let orphan_popup = d.is_some_and(|d| {
                    text(prev, d, b"Subtype").as_deref() == Some("Popup")
                        && d.get(b"Parent")
                            .ok()
                            .and_then(|p| p.as_reference().ok())
                            .is_some_and(|p| dropped_ids.contains(&p))
                });
                if dropped || orphan_popup {
                    report.dropped += u32::from(dropped);
                } else {
                    if d.is_some_and(|d| {
                        key_of(prev, *id, d).is_some_and(|k| plan.keep.contains(&k))
                    }) {
                        report.kept += 1;
                    }
                    keep.push(r.clone());
                }
            }
            touched.insert(n, page_id);
            kept_lists.insert(n, keep);
        }
    }

    // New objects: first reserve ids so replies can point at notes written in the same save.
    let mut new_ids: Vec<ObjectId> = Vec::with_capacity(plan.specs.len());
    for s in &plan.specs {
        let id = inc.new_document.new_object_id();
        refs.insert(s.key.clone(), id);
        new_ids.push(id);
    }
    let mut added: BTreeMap<u32, Vec<Object>> = BTreeMap::new();
    for (s, id) in plan.specs.iter().zip(&new_ids) {
        let n = s.page_index + 1;
        let page = touched[&n];
        let mut d = to_dict(&s.dict, &refs, page);
        d.set("P", Object::Reference(page));
        if let Some(ap) = &s.appearance {
            let mut form = Dictionary::new();
            form.set("Type", Object::Name(b"XObject".to_vec()));
            form.set("Subtype", Object::Name(b"Form".to_vec()));
            form.set(
                "BBox",
                Object::Array(ap.bbox.iter().map(|v| Object::Real(*v)).collect()),
            );
            form.set(
                "Resources",
                Object::Dictionary(to_dict(&ap.resources, &refs, page)),
            );
            let stream = Stream::new(form, ap.content.clone().into_bytes());
            let ap_id = inc.new_document.add_object(stream);
            let mut apd = Dictionary::new();
            apd.set("N", Object::Reference(ap_id));
            d.set("AP", Object::Dictionary(apd));
        }
        let list = added.entry(n).or_default();
        if let Some(pr) = s.popup {
            let mut p = Dictionary::new();
            p.set("Type", Object::Name(b"Annot".to_vec()));
            p.set("Subtype", Object::Name(b"Popup".to_vec()));
            p.set(
                "Rect",
                Object::Array(pr.iter().map(|v| Object::Real(*v)).collect()),
            );
            p.set("Parent", Object::Reference(*id));
            p.set("Open", Object::Boolean(false));
            p.set("F", Object::Integer(28));
            let pid = inc.new_document.add_object(p);
            d.set("Popup", Object::Reference(pid));
            list.push(Object::Reference(*id));
            list.push(Object::Reference(pid));
        } else {
            list.push(Object::Reference(*id));
        }
        inc.new_document.set_object(*id, d);
        report.written += 1;
    }

    // Each touched page gets a new revision with an inline /Annots array.
    for (n, page_id) in &touched {
        let mut list = kept_lists.remove(n).unwrap_or_else(|| {
            let prev = inc.get_prev_documents();
            prev.get_object(*page_id)
                .and_then(Object::as_dict)
                .map(|p| annots_of(prev, p))
                .unwrap_or_default()
        });
        list.extend(added.remove(n).unwrap_or_default());
        inc.opt_clone_object_to_new_document(*page_id)
            .map_err(err)?;
        if let Ok(Object::Dictionary(p)) = inc.new_document.get_object_mut(*page_id) {
            if list.is_empty() {
                p.remove(b"Annots");
            } else {
                p.set("Annots", Object::Array(list));
            }
        }
    }
    inc.save_to(out).map_err(err)?;
    Ok(report)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn font_size_from_da() {
        assert_eq!(font_size("/Helv 11 Tf 0 0 0 rg"), Some(11.0));
        assert_eq!(font_size("0 g"), None);
    }
}
