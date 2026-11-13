//! Result cache (P1.M1.STO.7, ADR-011).
//!
//! Expensive, deterministic commands (extraction, OCR, page models) are registered with an
//! *implementation version*. Their results are stored under
//! `BLAKE3(command ‖ implementation ‖ canonical JSON input)`. Inputs refer to files by content
//! hash (`BlobRef`), so a changed file is a different key; a new implementation version is a
//! different key too — old entries are simply never asked for again.

use std::fs;
use std::io;
use std::path::PathBuf;

use ep_types::Hash;
use serde_json::Value;

/// Cache key of one call. `serde_json` keeps object keys sorted, so equal inputs serialise
/// identically.
#[must_use]
pub fn cache_key(command: &str, implementation: &str, input: &Value) -> Hash {
    let mut h = blake3::Hasher::new();
    for part in [command.as_bytes(), b"\0", implementation.as_bytes(), b"\0"] {
        h.update(part);
    }
    h.update(input.to_string().as_bytes());
    Hash::from_bytes(*h.finalize().as_bytes())
}

/// Results on disk, one JSON file per key (in the app cache folder: disposable).
#[derive(Debug)]
pub struct ResultCache {
    dir: PathBuf,
}

impl ResultCache {
    /// A cache in `dir` (created if needed).
    ///
    /// # Errors
    /// The folder cannot be created.
    pub fn open(dir: impl Into<PathBuf>) -> io::Result<Self> {
        let dir = dir.into();
        fs::create_dir_all(&dir)?;
        Ok(Self { dir })
    }

    fn path(&self, key: &Hash) -> PathBuf {
        let h = key.to_hex();
        self.dir.join(&h[..2]).join(format!("{h}.json"))
    }

    /// A stored result, if any (unreadable entries count as misses).
    #[must_use]
    pub fn get(&self, key: &Hash) -> Option<Value> {
        let text = fs::read_to_string(self.path(key)).ok()?;
        serde_json::from_str(&text).ok()
    }

    /// Store a result (atomic: temporary file, then rename).
    ///
    /// # Errors
    /// Disk error.
    pub fn put(&self, key: &Hash, value: &Value) -> io::Result<()> {
        let path = self.path(key);
        if let Some(d) = path.parent() {
            fs::create_dir_all(d)?;
        }
        let tmp = path.with_extension(format!("{}.tmp", std::process::id()));
        fs::write(&tmp, value.to_string())?;
        fs::rename(&tmp, &path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn keys_depend_on_everything_that_matters() {
        let a = cache_key("document.extract", "1", &json!({ "b": 1, "a": 2 }));
        assert_eq!(
            a,
            cache_key("document.extract", "1", &json!({ "a": 2, "b": 1 }))
        );
        assert_ne!(
            a,
            cache_key("document.extract", "2", &json!({ "a": 2, "b": 1 }))
        );
        assert_ne!(
            a,
            cache_key("document.render", "1", &json!({ "a": 2, "b": 1 }))
        );
        assert_ne!(
            a,
            cache_key("document.extract", "1", &json!({ "a": 3, "b": 1 }))
        );
    }
}
