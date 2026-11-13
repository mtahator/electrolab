//! lopdf adapter: object-level PDF access in pure Rust (no native library).
//!
//! Used for the Info dictionary now; for writing annotations incrementally in M3 (ADR-006).
//! Text and rendering come from PDFium (ADR-003), not from here.
#![deny(missing_docs)]

use std::path::Path;

use ep_document_core::metadata::RawMetadata;
use lopdf::Object;

/// Why a file could not be read.
#[derive(Debug)]
pub struct ReadError(pub String);

impl std::fmt::Display for ReadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

fn text(o: &Object) -> Option<String> {
    match o {
        Object::String(bytes, _) => Some(decode_pdf_string(bytes)),
        Object::Name(n) => Some(String::from_utf8_lossy(n).into_owned()),
        _ => None,
    }
}

/// PDF text strings: UTF-16BE with a BOM, UTF-8 with a BOM (PDF 2.0), else PDFDocEncoding
/// (treated as Latin-1, which matches it for the characters that matter here).
fn decode_pdf_string(b: &[u8]) -> String {
    if let Some(rest) = b.strip_prefix(&[0xFE, 0xFF]) {
        let units: Vec<u16> = rest
            .chunks_exact(2)
            .map(|c| u16::from_be_bytes([c[0], c[1]]))
            .collect();
        return String::from_utf16_lossy(&units);
    }
    if let Some(rest) = b.strip_prefix(&[0xEF, 0xBB, 0xBF]) {
        return String::from_utf8_lossy(rest).into_owned();
    }
    b.iter().map(|&c| char::from(c)).collect()
}

/// Read the Info dictionary and page count without loading the whole document.
///
/// # Errors
/// The file cannot be read or is not a PDF lopdf understands.
pub fn read_metadata(path: &Path) -> Result<RawMetadata, ReadError> {
    let m = lopdf::Document::load_metadata(path).map_err(|e| ReadError(e.to_string()))?;
    let mut custom: Vec<(String, String)> = m
        .custom
        .iter()
        .filter_map(|(k, v)| Some((String::from_utf8_lossy(k).into_owned(), text(v)?)))
        .collect();
    custom.sort();
    Ok(RawMetadata {
        title: m.title,
        author: m.author,
        subject: m.subject,
        keywords: m.keywords,
        custom,
        pages: m.page_count,
        encrypted: m.encrypted,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_string_encodings() {
        assert_eq!(
            decode_pdf_string(&[0xFE, 0xFF, 0x06, 0x34, 0x06, 0x28]),
            "شب"
        );
        assert_eq!(decode_pdf_string(b"plain"), "plain");
        assert_eq!(decode_pdf_string(&[0xEF, 0xBB, 0xBF, 0xC3, 0xA9]), "é");
    }
}
