//! Append-only event log (P1.M1.STO.3; Draft 2.2 §9.5).
//!
//! One log per library or project. Records are JSON lines in numbered segment files
//! (`seg-000001.jsonl`, …). Each record carries the hash of the one before it, so any edit or
//! loss in the middle is detectable. A crash can only leave a torn *last* line; opening the log
//! cuts it off. Publishers never wait for readers: replay reads the files.
#![deny(missing_docs)]

use std::fs::{self, File, OpenOptions};
use std::io::{self, BufRead, BufReader, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use ep_types::{Actor, Hash};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};

/// Start a new segment after this many bytes.
const SEGMENT_BYTES: u64 = 8 << 20;

/// What happened.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Event {
    /// When, in milliseconds since the Unix epoch (from the `Cx` clock).
    pub at_ms: i64,
    /// Who.
    pub actor: Actor,
    /// Dotted kind, e.g. `workspace.file_added`.
    pub kind: String,
    /// Kind-specific payload.
    pub data: serde_json::Value,
}

/// An event as stored: numbered and chained.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Record {
    /// 1-based sequence number, no gaps.
    pub seq: u64,
    /// Hash of the previous record (all zero for the first).
    pub prev: Hash,
    /// Hash of this record (`prev`, `seq` and the event).
    pub hash: Hash,
    /// The event.
    pub event: Event,
}

/// Event log errors.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum EventError {
    /// Disk error.
    #[error("event log I/O: {0}")]
    Io(#[from] io::Error),
    /// Serialisation error.
    #[error("event log JSON: {0}")]
    Json(#[from] serde_json::Error),
    /// The chain is broken at this sequence number (tampering or disk damage).
    #[error("event log chain broken at record {0}")]
    Broken(u64),
}

/// Result alias.
pub type Result<T> = std::result::Result<T, EventError>;

fn record_hash(prev: &Hash, seq: u64, event: &Event) -> Result<Hash> {
    let mut h = blake3::Hasher::new();
    h.update(prev.as_bytes());
    h.update(&seq.to_le_bytes());
    h.update(&serde_json::to_vec(event)?);
    Ok(Hash::from_bytes(*h.finalize().as_bytes()))
}

#[derive(Debug)]
struct Tail {
    file: File,
    segment: u32,
    len: u64,
    seq: u64,
    last: Hash,
}

/// An open event log.
#[derive(Debug)]
pub struct EventLog {
    dir: PathBuf,
    tail: Mutex<Tail>,
}

fn segment_path(dir: &Path, n: u32) -> PathBuf {
    dir.join(format!("seg-{n:06}.jsonl"))
}

fn segments(dir: &Path) -> Result<Vec<u32>> {
    let mut v: Vec<u32> = fs::read_dir(dir)?
        .filter_map(|e| {
            e.ok()?
                .file_name()
                .to_str()?
                .strip_prefix("seg-")?
                .strip_suffix(".jsonl")?
                .parse()
                .ok()
        })
        .collect();
    v.sort_unstable();
    Ok(v)
}

impl EventLog {
    /// Open or create a log in `dir`, cutting off a torn last line left by a crash.
    ///
    /// # Errors
    /// Disk error, or a broken chain inside the last segment.
    pub fn open(dir: impl Into<PathBuf>) -> Result<Self> {
        let dir = dir.into();
        fs::create_dir_all(&dir)?;
        let segs = segments(&dir)?;
        let segment = segs.last().copied().unwrap_or(1);
        let path = segment_path(&dir, segment);
        let file_len = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(false)
            .open(&path)?
            .metadata()?
            .len();
        // Scan the last segment: keep every complete, valid, chained line.
        let (mut seq, mut last, mut good_len) = (0u64, Hash::from_bytes([0; 32]), 0u64);
        let mut first = true;
        let mut reader = BufReader::new(File::open(&path)?);
        let mut line = String::new();
        loop {
            line.clear();
            let n = reader.read_line(&mut line)?;
            if n == 0 || !line.ends_with('\n') {
                break;
            }
            let Ok(rec) = serde_json::from_str::<Record>(line.trim_end()) else {
                break;
            };
            if first {
                // The first record of a segment links to the previous segment; trust its prev.
                (seq, last, first) = (rec.seq - 1, rec.prev, false);
            }
            if rec.seq != seq + 1
                || rec.prev != last
                || record_hash(&rec.prev, rec.seq, &rec.event)? != rec.hash
            {
                if good_len + n as u64 == file_len {
                    break; // a bad last line is a torn write
                }
                return Err(EventError::Broken(seq + 1));
            }
            (seq, last) = (rec.seq, rec.hash);
            good_len += n as u64;
        }
        drop(reader);
        if file_len != good_len {
            // Append handles cannot truncate on Windows: use a plain write handle.
            let f = OpenOptions::new().write(true).open(&path)?;
            f.set_len(good_len)?;
            f.sync_all()?;
        }
        let file = OpenOptions::new().append(true).open(&path)?;
        Ok(Self {
            dir,
            tail: Mutex::new(Tail {
                file,
                segment,
                len: good_len,
                seq,
                last,
            }),
        })
    }

    /// Append one event and flush it to disk. Returns its sequence number.
    ///
    /// # Errors
    /// Disk or serialisation error.
    pub fn append(&self, event: Event) -> Result<u64> {
        let mut t = self.tail.lock();
        if t.len >= SEGMENT_BYTES {
            t.segment += 1;
            t.file = OpenOptions::new()
                .read(true)
                .append(true)
                .create(true)
                .open(segment_path(&self.dir, t.segment))?;
            t.len = 0;
        }
        let seq = t.seq + 1;
        let hash = record_hash(&t.last, seq, &event)?;
        let rec = Record {
            seq,
            prev: t.last,
            hash,
            event,
        };
        let mut line = serde_json::to_vec(&rec)?;
        line.push(b'\n');
        t.file.write_all(&line)?;
        t.file.sync_data()?;
        t.len += line.len() as u64;
        (t.seq, t.last) = (seq, hash);
        Ok(seq)
    }

    /// Sequence number of the last record (0 when empty).
    #[must_use]
    pub fn last_seq(&self) -> u64 {
        self.tail.lock().seq
    }

    /// Call `f` for every record with `seq >= from`, oldest first, checking the chain.
    ///
    /// # Errors
    /// Disk error, or [`EventError::Broken`] where the chain does not hold.
    pub fn replay(&self, from: u64, mut f: impl FnMut(&Record)) -> Result<()> {
        let end = self.last_seq();
        let mut expect: Option<(u64, Hash)> = None;
        for s in segments(&self.dir)? {
            let mut file = File::open(segment_path(&self.dir, s))?;
            file.seek(SeekFrom::Start(0))?;
            for line in BufReader::new(file).lines() {
                let rec: Record = serde_json::from_str(&line?)?;
                if rec.seq > end {
                    return Ok(());
                }
                let ok = expect.is_none_or(|(seq, h)| rec.seq == seq + 1 && rec.prev == h)
                    && record_hash(&rec.prev, rec.seq, &rec.event)? == rec.hash;
                if !ok {
                    return Err(EventError::Broken(rec.seq));
                }
                expect = Some((rec.seq, rec.hash));
                if rec.seq >= from {
                    f(&rec);
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ev(i: i64) -> Event {
        Event {
            at_ms: i,
            actor: Actor::User,
            kind: "test.happened".into(),
            data: serde_json::json!({ "i": i }),
        }
    }

    #[test]
    fn append_replay_reopen() {
        let d = tempfile::tempdir().expect("tmp");
        {
            let log = EventLog::open(d.path()).expect("open");
            for i in 1..=5 {
                assert_eq!(
                    log.append(ev(i)).expect("append"),
                    u64::try_from(i).expect("u64")
                );
            }
        }
        let log = EventLog::open(d.path()).expect("reopen");
        assert_eq!(log.last_seq(), 5);
        assert_eq!(log.append(ev(6)).expect("append"), 6);
        let mut seen = vec![];
        log.replay(3, |r| seen.push(r.event.at_ms)).expect("replay");
        assert_eq!(seen, [3, 4, 5, 6]);
    }

    #[test]
    fn crash_in_the_middle_of_a_write_recovers() {
        let d = tempfile::tempdir().expect("tmp");
        {
            let log = EventLog::open(d.path()).expect("open");
            log.append(ev(1)).expect("a");
            log.append(ev(2)).expect("b");
        }
        // Simulate a crash half-way through writing record 3.
        let p = segment_path(d.path(), 1);
        let mut f = OpenOptions::new().append(true).open(&p).expect("f");
        f.write_all(br#"{"seq":3,"prev":"00"#).expect("torn");
        drop(f);
        let log = EventLog::open(d.path()).expect("recover");
        assert_eq!(log.last_seq(), 2);
        assert_eq!(log.append(ev(3)).expect("c"), 3);
        let mut n = 0;
        log.replay(1, |_| n += 1).expect("replay");
        assert_eq!(n, 3);
    }

    #[test]
    fn tampering_is_detected() {
        let d = tempfile::tempdir().expect("tmp");
        let log = EventLog::open(d.path()).expect("open");
        for i in 1..=3 {
            log.append(ev(i)).expect("append");
        }
        let p = segment_path(d.path(), 1);
        let text = fs::read_to_string(&p)
            .expect("read")
            .replace(r#""i":2"#, r#""i":9"#);
        fs::write(&p, text).expect("write");
        assert!(matches!(log.replay(1, |_| {}), Err(EventError::Broken(2))));
        drop(log);
        assert!(matches!(
            EventLog::open(d.path()),
            Err(EventError::Broken(2))
        ));
    }
}
