//! Content-addressed file store (Draft 2.2 §9.4, P1.M1.STO.1).
//!
//! Every file is stored once, under its BLAKE3 hash: `<root>/<aa>/<bb>/<64-hex>`.
//! Writes stream through the hasher into a temporary file in the same volume, are flushed to
//! disk, then renamed into place — a half-written blob never appears under its final name.
//! Large files are never loaded into memory.
#![deny(missing_docs)]

use std::fs::{self, File, OpenOptions};
use std::io::{self, BufReader, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use ep_types::{BlobRef, Hash, MediaType};

/// Size of the copy buffer (1 MiB: large enough for fast disks, small enough for memory).
const CHUNK: usize = 1 << 20;

/// Blob store errors.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum BlobError {
    /// No blob with this hash.
    #[error("blob {0} not found")]
    NotFound(Hash),
    /// The stored bytes no longer match their hash.
    #[error("blob {0} is corrupt")]
    Corrupt(Hash),
    /// A range outside the file.
    #[error("range {start}..{end} is outside blob {hash} of {size} bytes")]
    BadRange {
        /// Blob.
        hash: Hash,
        /// Requested start.
        start: u64,
        /// Requested end (exclusive).
        end: u64,
        /// Actual size.
        size: u64,
    },
    /// Disk error.
    #[error("blob store I/O: {0}")]
    Io(#[from] io::Error),
}

/// Result alias.
pub type Result<T> = std::result::Result<T, BlobError>;

/// A content-addressed store rooted at one folder.
#[derive(Debug)]
pub struct BlobStore {
    root: PathBuf,
    tmp_counter: AtomicU64,
}

impl BlobStore {
    /// Open (creating if needed) a store at `root`. Leftover temporary files from a crash are
    /// removed.
    ///
    /// # Errors
    /// The folder cannot be created or read.
    pub fn open(root: impl Into<PathBuf>) -> Result<Self> {
        let root = root.into();
        let tmp = root.join("tmp");
        fs::create_dir_all(&tmp)?;
        for e in fs::read_dir(&tmp)? {
            let _ = fs::remove_file(e?.path());
        }
        Ok(Self {
            root,
            tmp_counter: AtomicU64::new(0),
        })
    }

    /// The folder this store lives in.
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Where a blob with this hash lives (whether or not it exists).
    #[must_use]
    pub fn path_of(&self, hash: &Hash) -> PathBuf {
        let h = hash.to_hex();
        self.root.join(&h[..2]).join(&h[2..4]).join(h)
    }

    /// Store the bytes read from `src`, hashing as they stream. Storing the same content twice
    /// is cheap and returns the same reference.
    ///
    /// # Errors
    /// Reading `src` or writing the store fails.
    pub fn put(&self, mut src: impl Read, media_type: MediaType) -> Result<BlobRef> {
        let n = self.tmp_counter.fetch_add(1, Ordering::Relaxed);
        let tmp_path = self
            .root
            .join("tmp")
            .join(format!("{}-{n}.part", std::process::id()));
        let guard = TempGuard(&tmp_path);
        let mut tmp = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&tmp_path)?;
        let mut hasher = blake3::Hasher::new();
        let mut buf = vec![0u8; CHUNK];
        let mut size = 0u64;
        loop {
            let k = match src.read(&mut buf) {
                Ok(0) => break,
                Ok(k) => k,
                Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                Err(e) => return Err(e.into()),
            };
            hasher.update(&buf[..k]);
            tmp.write_all(&buf[..k])?;
            size += k as u64;
        }
        tmp.sync_all()?;
        drop(tmp);
        let hash = Hash::from_bytes(*hasher.finalize().as_bytes());
        let dest = self.path_of(&hash);
        if dest.exists() {
            // Same content already stored; the temporary copy is dropped by the guard.
            return Ok(BlobRef {
                hash,
                size,
                media_type,
            });
        }
        if let Some(dir) = dest.parent() {
            fs::create_dir_all(dir)?;
        }
        match fs::rename(&tmp_path, &dest) {
            Ok(()) => {}
            // Another writer won the race with identical bytes.
            Err(_) if dest.exists() => {}
            Err(e) => return Err(e.into()),
        }
        drop(guard);
        Ok(BlobRef {
            hash,
            size,
            media_type,
        })
    }

    /// Store a copy of a file on disk; the original is only read.
    ///
    /// # Errors
    /// See [`BlobStore::put`].
    pub fn put_file(&self, path: &Path, media_type: MediaType) -> Result<BlobRef> {
        self.put(
            BufReader::with_capacity(CHUNK, File::open(path)?),
            media_type,
        )
    }

    /// Size of a stored blob, or `None` if absent.
    ///
    /// # Errors
    /// Disk error other than "not found".
    pub fn stat(&self, hash: &Hash) -> Result<Option<u64>> {
        match fs::metadata(self.path_of(hash)) {
            Ok(m) => Ok(Some(m.len())),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    /// Open a blob for streaming reads.
    ///
    /// # Errors
    /// [`BlobError::NotFound`] or a disk error.
    pub fn open_read(&self, hash: &Hash) -> Result<File> {
        File::open(self.path_of(hash)).map_err(|e| {
            if e.kind() == io::ErrorKind::NotFound {
                BlobError::NotFound(*hash)
            } else {
                e.into()
            }
        })
    }

    /// Read bytes `start..end` (used by the `ep://blob` protocol for HTTP range requests).
    ///
    /// # Errors
    /// [`BlobError::BadRange`] if the range is empty-inverted or past the end.
    pub fn read_range(&self, hash: &Hash, start: u64, end: u64) -> Result<Vec<u8>> {
        let mut f = self.open_read(hash)?;
        let size = f.metadata()?.len();
        if start > end || end > size {
            return Err(BlobError::BadRange {
                hash: *hash,
                start,
                end,
                size,
            });
        }
        let len = usize::try_from(end - start).map_err(|_| BlobError::BadRange {
            hash: *hash,
            start,
            end,
            size,
        })?;
        let mut out = vec![0u8; len];
        f.seek(SeekFrom::Start(start))?;
        f.read_exact(&mut out)?;
        Ok(out)
    }

    /// Re-hash a stored blob and compare.
    ///
    /// # Errors
    /// [`BlobError::Corrupt`] if the bytes changed; [`BlobError::NotFound`] if absent.
    pub fn verify(&self, hash: &Hash) -> Result<()> {
        let mut f = BufReader::with_capacity(CHUNK, self.open_read(hash)?);
        let mut hasher = blake3::Hasher::new();
        io::copy(&mut f, &mut hasher)?;
        if hasher.finalize().as_bytes() == hash.as_bytes() {
            Ok(())
        } else {
            Err(BlobError::Corrupt(*hash))
        }
    }
}

/// Removes the temporary file unless it was renamed away.
struct TempGuard<'a>(&'a Path);

impl Drop for TempGuard<'_> {
    fn drop(&mut self) {
        let _ = fs::remove_file(self.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn store() -> (tempfile::TempDir, BlobStore) {
        let d = tempfile::tempdir().expect("tmp");
        let s = BlobStore::open(d.path().join("blobs")).expect("open");
        (d, s)
    }

    #[test]
    fn put_get_dedupe_and_no_temp_left() {
        let (_d, s) = store();
        let a = s.put(&b"hello paper"[..], MediaType::pdf()).expect("put");
        let b = s
            .put(&b"hello paper"[..], MediaType::pdf())
            .expect("put again");
        assert_eq!(a, b);
        assert_eq!(a.hash, Hash::of(b"hello paper"));
        assert_eq!(s.stat(&a.hash).expect("stat"), Some(11));
        assert_eq!(s.read_range(&a.hash, 6, 11).expect("range"), b"paper");
        s.verify(&a.hash).expect("verify");
        assert_eq!(fs::read_dir(s.root().join("tmp")).expect("tmp").count(), 0);
    }

    #[test]
    fn corrupt_file_is_detected() {
        let (_d, s) = store();
        let r = s
            .put(&b"original bytes"[..], MediaType::pdf())
            .expect("put");
        fs::write(s.path_of(&r.hash), b"tampered bytes").expect("tamper");
        assert!(matches!(s.verify(&r.hash), Err(BlobError::Corrupt(_))));
    }

    #[test]
    fn missing_and_bad_range() {
        let (_d, s) = store();
        assert!(matches!(
            s.verify(&Hash::of(b"none")),
            Err(BlobError::NotFound(_))
        ));
        assert_eq!(s.stat(&Hash::of(b"none")).expect("stat"), None);
        let r = s.put(&b"abc"[..], MediaType::pdf()).expect("put");
        assert!(matches!(
            s.read_range(&r.hash, 2, 9),
            Err(BlobError::BadRange { .. })
        ));
    }

    #[test]
    fn put_file_leaves_original_untouched() {
        let (d, s) = store();
        let src = d.path().join("paper.pdf");
        fs::write(&src, b"%PDF-1.7 test").expect("write");
        let before = fs::read(&src).expect("read");
        let r = s.put_file(&src, MediaType::pdf()).expect("put");
        assert_eq!(fs::read(&src).expect("read"), before);
        assert_eq!(fs::read(s.path_of(&r.hash)).expect("read"), before);
    }

    /// A reader that produces `n` bytes without holding them, to prove streaming.
    struct Pattern(u64);
    impl Read for Pattern {
        fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            let k = usize::try_from(self.0.min(buf.len() as u64)).unwrap_or(buf.len());
            for (i, b) in buf[..k].iter_mut().enumerate() {
                *b = u8::try_from(i % 251).unwrap_or(0);
            }
            self.0 -= k as u64;
            Ok(k)
        }
    }

    #[test]
    fn large_stream_is_hashed_in_chunks() {
        let (_d, s) = store();
        let n = 64 * 1024 * 1024 + 17; // 64 MiB: enough to cross many chunks quickly
        let r = s.put(Pattern(n), MediaType::pdf()).expect("put");
        assert_eq!(r.size, n);
        s.verify(&r.hash).expect("verify");
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(32))]
        #[test]
        fn any_bytes_round_trip(data in proptest::collection::vec(any::<u8>(), 0..4096)) {
            let (_d, s) = store();
            let r = s.put(&data[..], MediaType::pdf()).expect("put");
            prop_assert_eq!(r.hash, Hash::of(&data));
            let len = data.len() as u64;
            prop_assert_eq!(s.read_range(&r.hash, 0, len).expect("range"), data);
        }
    }
}
