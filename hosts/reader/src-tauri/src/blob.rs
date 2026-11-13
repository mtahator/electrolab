//! `ep` URI scheme: `ep://localhost/blob/<hash>` (`http://ep.localhost/blob/<hash>` on Windows)
//! serves a stored file of the open library, with HTTP range requests so PDF.js loads only the
//! parts it needs (P1.M1.HST.2). Only blobs of the open library are served; nothing else on disk.

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};

use ep_types::Hash;
use ep_workspace::Workspace;
use tauri::http::{Request, Response, StatusCode};

fn cors(b: tauri::http::response::Builder) -> tauri::http::response::Builder {
    b.header("Access-Control-Allow-Origin", "*")
        .header("Access-Control-Allow-Headers", "Range")
        .header(
            "Access-Control-Expose-Headers",
            "Content-Range, Content-Length, Accept-Ranges",
        )
}

fn fail(code: StatusCode, msg: &str) -> Response<Vec<u8>> {
    cors(Response::builder().status(code))
        .body(msg.as_bytes().to_vec())
        .unwrap_or_default()
}

/// Parse `bytes=a-b`, `bytes=a-` or `bytes=-n` against a file of `len` bytes.
pub fn parse_range(header: &str, len: u64) -> Option<(u64, u64)> {
    let spec = header.strip_prefix("bytes=")?.split(',').next()?.trim();
    let (a, b) = spec.split_once('-')?;
    let (start, end) = if a.is_empty() {
        let n: u64 = b.parse().ok()?;
        (len.checked_sub(n.min(len))?, len.checked_sub(1)?)
    } else {
        let start: u64 = a.parse().ok()?;
        let end = if b.is_empty() {
            len.checked_sub(1)?
        } else {
            b.parse::<u64>().ok()?.min(len.checked_sub(1)?)
        };
        (start, end)
    };
    (start <= end && end < len).then_some((start, end))
}

pub fn serve(ws: &Workspace, req: &Request<Vec<u8>>) -> Response<Vec<u8>> {
    if req.method() == "OPTIONS" {
        return cors(Response::builder().status(StatusCode::NO_CONTENT))
            .body(Vec::new())
            .unwrap_or_default();
    }
    let path = req.uri().path();
    let Some(hash) = path
        .strip_prefix("/blob/")
        .and_then(|h| h.parse::<Hash>().ok())
    else {
        return fail(StatusCode::BAD_REQUEST, "expected /blob/<hash>");
    };
    let Some(file_path) = ws.blob_path(&hash) else {
        return fail(StatusCode::NOT_FOUND, "not in the open library");
    };
    let Ok(mut file) = File::open(&file_path) else {
        return fail(StatusCode::NOT_FOUND, "missing");
    };
    let len = file.metadata().map_or(0, |m| m.len());
    let range = req.headers().get("range").and_then(|v| v.to_str().ok());
    let (start, end, status) = match range.map(|r| parse_range(r, len)) {
        Some(Some((a, b))) => (a, b, StatusCode::PARTIAL_CONTENT),
        Some(None) => {
            return cors(Response::builder().status(StatusCode::RANGE_NOT_SATISFIABLE))
                .header("Content-Range", format!("bytes */{len}"))
                .body(Vec::new())
                .unwrap_or_default();
        }
        None => (0, len.saturating_sub(1), StatusCode::OK),
    };
    let size = if len == 0 { 0 } else { end + 1 - start };
    let mut body = vec![0u8; usize::try_from(size).unwrap_or(0)];
    if file.seek(SeekFrom::Start(start)).is_err() || file.read_exact(&mut body).is_err() {
        return fail(StatusCode::INTERNAL_SERVER_ERROR, "read");
    }
    let mut b = cors(Response::builder().status(status))
        .header("Content-Type", "application/pdf")
        .header("Accept-Ranges", "bytes")
        .header("Cache-Control", "private, max-age=31536000, immutable")
        .header("Content-Length", body.len().to_string());
    if status == StatusCode::PARTIAL_CONTENT {
        b = b.header("Content-Range", format!("bytes {start}-{end}/{len}"));
    }
    b.body(body).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ranges() {
        assert_eq!(parse_range("bytes=0-99", 1000), Some((0, 99)));
        assert_eq!(parse_range("bytes=900-", 1000), Some((900, 999)));
        assert_eq!(parse_range("bytes=-100", 1000), Some((900, 999)));
        assert_eq!(parse_range("bytes=990-2000", 1000), Some((990, 999)));
        assert_eq!(parse_range("bytes=1000-", 1000), None);
        assert_eq!(parse_range("bytes=5-1", 1000), None);
        assert_eq!(parse_range("items=0-1", 1000), None);
    }
}
