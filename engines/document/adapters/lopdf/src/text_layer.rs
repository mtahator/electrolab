//! Invisible text layers for scanned pages (P1.M4.OCR.2): an incremental update that adds, per
//! page, a content stream drawing the recognised words in render mode 3 (invisible) with
//! Helvetica. The page's own content is wrapped in `q … Q` first so its graphics state cannot
//! move the text. The original bytes stay as a prefix.

use std::io::Write;
use std::path::Path;

use lopdf::{Dictionary, Document, IncrementalDocument, Object, ObjectId, Stream};

use crate::ReadError;

fn err(e: impl std::fmt::Display) -> ReadError {
    ReadError(e.to_string())
}

/// The page's resources, inherited ones included, as one dictionary.
fn resources(doc: &Document, page: ObjectId) -> Dictionary {
    let mut cur = Some(page);
    let mut depth = 0;
    while let Some(id) = cur {
        let Ok(d) = doc.get_object(id).and_then(Object::as_dict) else {
            break;
        };
        if let Ok(r) = d.get(b"Resources")
            && let Ok((_, Object::Dictionary(rd))) = doc.dereference(r)
        {
            return rd.clone();
        }
        cur = d.get(b"Parent").ok().and_then(|p| p.as_reference().ok());
        depth += 1;
        if depth > 32 {
            break;
        }
    }
    Dictionary::new()
}

/// Add the content streams `layers` (`(page index, content)`) to `input`, writing to `out`.
/// `font` is the resource name the content uses for Helvetica.
///
/// # Errors
/// The file cannot be read (or is encrypted), a page does not exist, or writing fails.
pub fn write_text_layer(
    input: &Path,
    layers: &[(u32, String)],
    font: &str,
    out: &mut impl Write,
) -> Result<u32, ReadError> {
    let mut inc = IncrementalDocument::load(input).map_err(err)?;
    if inc.get_prev_documents().is_encrypted() {
        return Err(ReadError("the PDF is encrypted".into()));
    }
    let pages = inc.get_prev_documents().get_pages();
    let mut f = Dictionary::new();
    f.set("Type", Object::Name(b"Font".to_vec()));
    f.set("Subtype", Object::Name(b"Type1".to_vec()));
    f.set("BaseFont", Object::Name(b"Helvetica".to_vec()));
    f.set("Encoding", Object::Name(b"WinAnsiEncoding".to_vec()));
    let font_id = inc.new_document.add_object(f);
    let save = inc
        .new_document
        .add_object(Stream::new(Dictionary::new(), b"q\n".to_vec()));
    let restore = inc
        .new_document
        .add_object(Stream::new(Dictionary::new(), b"\nQ\n".to_vec()));
    let mut written = 0;
    for (index, content) in layers {
        let n = index + 1;
        let page = *pages
            .get(&n)
            .ok_or_else(|| ReadError(format!("page {n} does not exist")))?;
        let (contents, mut res) = {
            let prev = inc.get_prev_documents();
            let d = prev
                .get_object(page)
                .and_then(Object::as_dict)
                .map_err(err)?;
            let contents: Vec<Object> = match d.get(b"Contents") {
                Ok(Object::Array(a)) => a.clone(),
                Ok(o @ Object::Reference(_)) => vec![o.clone()],
                _ => Vec::new(),
            };
            (contents, resources(prev, page))
        };
        let layer = inc
            .new_document
            .add_object(Stream::new(Dictionary::new(), content.clone().into_bytes()));
        let mut fonts = match res.get(b"Font") {
            Ok(Object::Dictionary(d)) => d.clone(),
            Ok(Object::Reference(r)) => inc
                .get_prev_documents()
                .get_object(*r)
                .and_then(Object::as_dict)
                .cloned()
                .unwrap_or_default(),
            _ => Dictionary::new(),
        };
        fonts.set(font.as_bytes().to_vec(), Object::Reference(font_id));
        res.set("Font", Object::Dictionary(fonts));
        let mut all = vec![Object::Reference(save)];
        all.extend(contents);
        all.push(Object::Reference(restore));
        all.push(Object::Reference(layer));
        inc.opt_clone_object_to_new_document(page).map_err(err)?;
        if let Ok(Object::Dictionary(p)) = inc.new_document.get_object_mut(page) {
            p.set("Contents", Object::Array(all));
            p.set("Resources", Object::Dictionary(res));
        }
        written += 1;
    }
    inc.save_to(out).map_err(err)?;
    Ok(written)
}
