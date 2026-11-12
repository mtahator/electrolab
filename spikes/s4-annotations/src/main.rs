//! Spike P0.S4 — standard PDF annotations that other readers accept. Throw-away code.
//!
//!   s4-annotations support <pdfium-dir>                       which subtypes PDFium itself can create
//!   s4-annotations write   <pdfium-dir> <in.pdf> <out.pdf>     our writer: all kinds, incremental update (lopdf)
//!   s4-annotations read    <pdfium-dir> <file.pdf>             PDFium reads every annotation back (JSON)
//!   s4-annotations visible <pdfium-dir> <file.pdf> <page>      pixels changed inside each annotation when rendered
//!
//! PDFium finds word boxes on page 1 so the markup lands on real text. Coordinates are PDF user
//! space (points, y up) — the same space annotation /Rect and /QuadPoints use.

use std::path::Path;

use lopdf::{Dictionary, IncrementalDocument, Object, ObjectId, Stream, StringFormat, dictionary};
use pdfium_render::prelude::*;
use serde_json::json;

fn pdfium(dir: &str) -> anyhow::Result<Pdfium> {
    Ok(Pdfium::new(Pdfium::bind_to_library(Pdfium::pdfium_platform_library_name_at_path(dir))?))
}

fn main() -> anyhow::Result<()> {
    let a: Vec<String> = std::env::args().skip(1).collect();
    match a.first().map(String::as_str) {
        Some("support") => support(&pdfium(&a[1])?),
        Some("write") => write(&pdfium(&a[1])?, &a[2], &a[3]),
        Some("read") => read(&pdfium(&a[1])?, &a[2]),
        Some("visible") => visible(&pdfium(&a[1])?, &a[2], a[3].parse()?),
        _ => anyhow::bail!("usage: support | write | read | visible (see source)"),
    }
}

// ---- 1. what PDFium can create --------------------------------------------------------------
fn support(p: &Pdfium) -> anyhow::Result<()> {
    let kinds = [(1, "Text (note)"), (3, "FreeText"), (4, "Line"), (5, "Square"), (6, "Circle"), (9, "Highlight"),
                 (10, "Underline"), (11, "Squiggly"), (12, "StrikeOut"), (13, "Stamp"), (15, "Ink"), (16, "Popup")];
    let out: Vec<_> = kinds
        .iter()
        .map(|(k, n)| json!({ "subtype": n, "pdfium_can_create": p.bindings().is_true(unsafe { p.bindings().FPDFAnnot_IsSupportedSubtype(*k) }) }))
        .collect();
    println!("{}", serde_json::to_string_pretty(&out)?);
    Ok(())
}

// ---- 2. our writer ---------------------------------------------------------------------------
#[derive(Clone, Copy, Debug)]
struct Box4 { l: f32, b: f32, r: f32, t: f32 }

/// Words (≥ 6 letters) on page 1 with their tight boxes, from PDFium.
fn words(p: &Pdfium, path: &str) -> anyhow::Result<(Vec<(String, Box4)>, f32, f32)> {
    let doc = p.load_pdf_from_file(path, None)?;
    let page = doc.pages().get(0)?;
    let (w, h) = (page.width().value, page.height().value);
    let text = page.text()?;
    let mut out = Vec::new();
    let (mut cur, mut bx): (String, Option<Box4>) = (String::new(), None);
    for ch in text.chars().iter() {
        let c = char::from_u32(ch.unicode_value()).unwrap_or(' ');
        if c.is_alphabetic() {
            if let Ok(r) = ch.tight_bounds() {
                let nb = Box4 { l: r.left().value, b: r.bottom().value, r: r.right().value, t: r.top().value };
                bx = Some(match bx { None => nb, Some(o) => Box4 { l: o.l.min(nb.l), b: o.b.min(nb.b), r: o.r.max(nb.r), t: o.t.max(nb.t) } });
                cur.push(c);
                continue;
            }
        }
        if cur.chars().count() >= 6 {
            if let Some(b) = bx { out.push((cur.clone(), b)); }
        }
        cur.clear();
        bx = None;
    }
    Ok((out, w, h))
}

fn s(text: &str) -> Object {
    if text.is_ascii() {
        Object::String(text.as_bytes().to_vec(), StringFormat::Literal)
    } else {
        // PDF text string: UTF-16BE with byte-order mark
        let mut b = vec![0xFE, 0xFF];
        for u in text.encode_utf16() { b.extend_from_slice(&u.to_be_bytes()); }
        Object::String(b, StringFormat::Hexadecimal)
    }
}
fn r(v: f32) -> Object { Object::Real(v) }
fn arr(v: &[f32]) -> Object { Object::Array(v.iter().map(|x| r(*x)).collect()) }

struct Writer<'a> { inc: &'a mut IncrementalDocument, page: ObjectId, annots: Vec<Object>, n: u32, log: Vec<serde_json::Value> }

impl Writer<'_> {
    fn form(&mut self, rect: [f32; 4], resources: Dictionary, content: String) -> ObjectId {
        let d = dictionary! { "Type" => "XObject", "Subtype" => "Form", "BBox" => arr(&rect), "Resources" => resources };
        self.inc.new_document.add_object(Stream::new(d, content.into_bytes()))
    }

    /// Common markup keys; returns the new annotation's id.
    fn add(&mut self, subtype: &str, rect: [f32; 4], mut extra: Dictionary, contents: &str, category: &str, ap: Option<ObjectId>) -> ObjectId {
        self.n += 1;
        let nm = format!("ep-{}-{}", subtype.to_lowercase(), self.n);
        let mut d = dictionary! {
            "Type" => "Annot", "Subtype" => subtype, "Rect" => arr(&rect), "P" => self.page,
            "NM" => s(&nm), "T" => s("ElectroPaper spike"), "M" => s("D:20261112120000Z"),
            "CreationDate" => s("D:20261112120000Z"), "F" => 4, "Contents" => s(contents),
        };
        if !category.is_empty() { d.set("Subj", s(category)); }
        if let Some(ap) = ap { d.set("AP", dictionary! { "N" => ap }); }
        for (k, v) in extra.iter_mut() { d.set(k.clone(), v.clone()); }
        let id = self.inc.new_document.add_object(d);
        self.annots.push(id.into());
        self.log.push(json!({ "kind": subtype, "nm": nm, "rect": rect, "category": category, "contents": contents }));
        id
    }
}

fn quad(b: Box4) -> [f32; 8] {
    // Acrobat order: upper-left, upper-right, lower-left, lower-right
    [b.l, b.t, b.r, b.t, b.l, b.b, b.r, b.b]
}
fn pad(b: Box4, p: f32) -> [f32; 4] { [b.l - p, b.b - p, b.r + p, b.t + p] }

fn write(p: &Pdfium, input: &str, output: &str) -> anyhow::Result<()> {
    let (ws, w, h) = words(p, input)?;
    anyhow::ensure!(ws.len() > 120, "page 1 has too little text");
    let (w_hl, w_ul, w_so, w_note) = (ws[30].clone(), ws[60].clone(), ws[90].clone(), ws[110].clone());

    let mut inc = IncrementalDocument::load(input)?;
    let page = *inc.get_prev_documents().get_pages().get(&1).ok_or_else(|| anyhow::anyhow!("no page 1"))?;
    let prev = inc.get_prev_documents();
    let existing: Vec<Object> = match prev.get_object(page)?.as_dict()?.get(b"Annots") {
        Ok(Object::Array(v)) => v.clone(),
        Ok(Object::Reference(id)) => prev.get_object(*id)?.as_array()?.clone(),
        _ => Vec::new(),
    };
    inc.opt_clone_object_to_new_document(page)?;
    let mut wr = Writer { inc: &mut inc, page, annots: existing, n: 0, log: Vec::new() };

    // 1. Highlight (category: definition) — multiply blend so text stays readable
    let q = quad(w_hl.1);
    let rect = pad(w_hl.1, 1.0);
    let gs = dictionary! { "ExtGState" => dictionary! { "GS0" => dictionary! { "Type" => "ExtGState", "BM" => "Multiply", "CA" => 1.0, "ca" => 1.0 } } };
    let ap = wr.form(rect, gs, format!("/GS0 gs 1 0.9 0.2 rg {} {} m {} {} l {} {} l {} {} l h f", q[0], q[1], q[2], q[3], q[6], q[7], q[4], q[5]));
    wr.add("Highlight", rect, dictionary! { "QuadPoints" => arr(&q), "C" => arr(&[1.0, 0.9, 0.2]), "CA" => 1.0 }, &format!("Definition: {}", w_hl.0), "definition", Some(ap));

    // 2. Underline (category: method)
    let q = quad(w_ul.1);
    let rect = pad(w_ul.1, 2.0);
    let ap = wr.form(rect, Dictionary::new(), format!("0 0.45 0.85 RG 1.2 w {} {} m {} {} l S", q[4], q[5] - 1.0, q[6], q[7] - 1.0));
    wr.add("Underline", rect, dictionary! { "QuadPoints" => arr(&q), "C" => arr(&[0.0, 0.45, 0.85]) }, "", "method", Some(ap));

    // 3. StrikeOut (category: to check)
    let q = quad(w_so.1);
    let rect = pad(w_so.1, 1.0);
    let mid = (q[1] + q[5]) / 2.0;
    let ap = wr.form(rect, Dictionary::new(), format!("0.85 0.1 0.1 RG 1 w {} {} m {} {} l S", q[4], mid, q[6], mid));
    wr.add("StrikeOut", rect, dictionary! { "QuadPoints" => arr(&q), "C" => arr(&[0.85, 0.1, 0.1]) }, "", "to check", Some(ap));

    // 4. Note with a reply and a resolved state (category: question); Persian text tests Unicode strings
    let nx = (w_note.1.r + 4.0).min(w - 24.0);
    let nrect = [nx, w_note.1.t - 18.0, nx + 18.0, w_note.1.t];
    let ap = wr.form(nrect, Dictionary::new(), format!(
        "1 0.85 0.2 rg 0.45 0.3 0 RG 0.8 w {} {} 17 17 re B 0.45 0.3 0 RG {} {} m {} {} l {} {} m {} {} l S",
        nrect[0] + 0.5, nrect[1] + 0.5, nrect[0] + 4.0, nrect[1] + 12.0, nrect[0] + 14.0, nrect[1] + 12.0, nrect[0] + 4.0, nrect[1] + 7.0, nrect[0] + 11.0, nrect[1] + 7.0));
    let note = wr.add("Text", nrect, dictionary! { "Name" => "Comment", "Open" => false, "C" => arr(&[1.0, 0.85, 0.2]) },
        "Is this term defined earlier? — آیا این اصطلاح قبلاً تعریف شده است؟", "question", Some(ap));
    let popup = wr.inc.new_document.add_object(dictionary! {
        "Type" => "Annot", "Subtype" => "Popup", "Rect" => arr(&[nx - 200.0, nrect[1] - 120.0, nx, nrect[1]]), "Parent" => note, "Open" => false, "F" => 28 });
    wr.annots.push(popup.into());
    if let Ok(Object::Dictionary(d)) = wr.inc.new_document.get_object_mut(note) { d.set("Popup", popup); }
    wr.add("Text", nrect, dictionary! { "IRT" => note, "RT" => "R", "Name" => "Comment" }, "Yes: defined in Section II.", "", None);
    wr.add("Text", nrect, dictionary! { "IRT" => note, "StateModel" => s("Review"), "State" => s("Completed"), "Name" => "Comment" },
        "ElectroPaper spike set the status to Completed", "", None);

    // 5. FreeText (text on page)
    let frect = [40.0, 40.0, 230.0, 62.0];
    let font = dictionary! { "Font" => dictionary! { "Helv" => dictionary! { "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Helvetica", "Encoding" => "WinAnsiEncoding" } } };
    let ap = wr.form(frect, font, format!(
        "1 1 0.88 rg 0.3 0.3 0.3 RG 0.6 w {} {} {} {} re B BT /Helv 10 Tf 0 0 0 rg {} {} Td (Check this assumption on page 3) Tj ET",
        frect[0] + 0.3, frect[1] + 0.3, frect[2] - frect[0] - 0.6, frect[3] - frect[1] - 0.6, frect[0] + 5.0, frect[1] + 7.0));
    wr.add("FreeText", frect, dictionary! { "DA" => s("/Helv 10 Tf 0 0 0 rg"), "Q" => 0, "BS" => dictionary! { "W" => 0.6 } },
        "Check this assumption on page 3", "to check", Some(ap));

    // 6. Ink (a hand-drawn tick near the bottom margin)
    let pts: Vec<f32> = (0..30).flat_map(|i| { let x = 260.0 + i as f32 * 3.0; let y = 50.0 + (i as f32 * 0.5).sin() * 8.0; [x, y] }).collect();
    let irect = [256.0, 38.0, 354.0, 62.0];
    let path: String = pts.chunks(2).enumerate().map(|(i, c)| format!("{} {} {} ", c[0], c[1], if i == 0 { "m" } else { "l" })).collect();
    let ap = wr.form(irect, Dictionary::new(), format!("0.85 0 0 RG 1.5 w 1 J 1 j {path}S"));
    wr.add("Ink", irect, dictionary! { "InkList" => Object::Array(vec![arr(&pts)]), "C" => arr(&[0.85, 0.0, 0.0]), "BS" => dictionary! { "W" => 1.5 } },
        "", "", Some(ap));

    // 7. Square (around the title block)
    let srect = [w * 0.1, h - 140.0, w * 0.9, h - 60.0];
    let ap = wr.form(srect, Dictionary::new(), format!("0.9 0.2 0.2 RG 1.5 w {} {} {} {} re S", srect[0] + 0.75, srect[1] + 0.75, srect[2] - srect[0] - 1.5, srect[3] - srect[1] - 1.5));
    wr.add("Square", srect, dictionary! { "C" => arr(&[0.9, 0.2, 0.2]), "BS" => dictionary! { "W" => 1.5 } }, "Title block", "result", Some(ap));

    // 8. Line with an arrowhead, pointing at the highlighted word
    let (x2, y2) = (w_hl.1.l - 2.0, (w_hl.1.b + w_hl.1.t) / 2.0);
    let (x1, y1) = ((x2 - 60.0).max(10.0), y2 - 30.0);
    let lrect = [x1.min(x2) - 6.0, y1.min(y2) - 6.0, x1.max(x2) + 6.0, y1.max(y2) + 6.0];
    let (dx, dy) = (x2 - x1, y2 - y1);
    let len = (dx * dx + dy * dy).sqrt();
    let (ux, uy) = (dx / len, dy / len);
    let (ax, ay) = (x2 - 8.0 * ux + 4.0 * uy, y2 - 8.0 * uy - 4.0 * ux);
    let (bx, by) = (x2 - 8.0 * ux - 4.0 * uy, y2 - 8.0 * uy + 4.0 * ux);
    let ap = wr.form(lrect, Dictionary::new(), format!("0 0.4 0.9 RG 1.5 w 1 J {x1} {y1} m {x2} {y2} l S {ax} {ay} m {x2} {y2} l {bx} {by} l S"));
    wr.add("Line", lrect, dictionary! { "L" => arr(&[x1, y1, x2, y2]), "LE" => Object::Array(vec!["None".into(), "OpenArrow".into()]),
        "C" => arr(&[0.0, 0.4, 0.9]), "BS" => dictionary! { "W" => 1.5 } }, "See this word", "", Some(ap));

    let Writer { annots, log, .. } = wr;
    if let Ok(Object::Dictionary(d)) = inc.new_document.get_object_mut(page) { d.set("Annots", Object::Array(annots)); }
    inc.save(output)?;
    println!("{}", serde_json::to_string_pretty(&json!({ "written": log, "words": [w_hl.0, w_ul.0, w_so.0, w_note.0] }))?);
    Ok(())
}

// ---- 3. read back with PDFium ----------------------------------------------------------------
fn read(p: &Pdfium, path: &str) -> anyhow::Result<()> {
    let doc = p.load_pdf_from_file(path, None)?;
    let mut out = Vec::new();
    for (i, page) in doc.pages().iter().enumerate() {
        for a in page.annotations().iter() {
            let b = a.bounds().ok();
            out.push(json!({ "page": i + 1, "type": format!("{:?}", a.annotation_type()), "nm": a.name(), "author": a.creator(),
                "contents": a.contents(), "rect": b.map(|b| [b.left().value, b.bottom().value, b.right().value, b.top().value]) }));
        }
    }
    println!("{}", serde_json::to_string_pretty(&out)?);
    Ok(())
}

// ---- 4. are they visible? render with and without annotations, count changed pixels per annotation
fn visible(p: &Pdfium, path: &str, page_no: i32) -> anyhow::Result<()> {
    let doc = p.load_pdf_from_file(path, None)?;
    let page = doc.pages().get(page_no - 1)?;
    let scale = 2.0;
    let render = |annots: bool| -> anyhow::Result<(Vec<u8>, usize, usize)> {
        let bmp = page.render_with_config(&PdfRenderConfig::new().scale_page_by_factor(scale).render_annotations(annots).render_form_data(annots))?;
        Ok((bmp.as_rgba_bytes(), bmp.width() as usize, bmp.height() as usize))
    };
    let (with, wd, ht) = render(true)?;
    let (without, _, _) = render(false)?;
    let ph = page.height().value;
    let mut out = Vec::new();
    for a in page.annotations().iter() {
        let Ok(b) = a.bounds() else { continue };
        let (x0, x1) = ((b.left().value * scale) as usize, ((b.right().value * scale) as usize).min(wd));
        let (y0, y1) = (((ph - b.top().value) * scale).max(0.0) as usize, (((ph - b.bottom().value) * scale) as usize).min(ht));
        let mut changed = 0usize;
        for y in y0..y1 { for x in x0..x1 {
            let i = (y * wd + x) * 4;
            if (0..3).any(|c| with[i + c].abs_diff(without[i + c]) > 24) { changed += 1; }
        } }
        out.push(json!({ "type": format!("{:?}", a.annotation_type()), "nm": a.name(), "changed_pixels": changed, "area_px": (x1.saturating_sub(x0)) * (y1.saturating_sub(y0)) }));
    }
    println!("{}", serde_json::to_string_pretty(&out)?);
    let _ = Path::new(path);
    Ok(())
}
