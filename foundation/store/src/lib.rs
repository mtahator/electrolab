//! SQLite store (P1.M1.STO.2): numbered migrations, one writer, a small pool of readers.
//!
//! Rules (phase-1 plan, "Best practices — foundation"): one writer; short transactions; WAL so
//! readers never block the writer; every migration is tested from every earlier version.
//! Each engine owns its tables and passes its own migration list.
#![deny(missing_docs)]

use std::path::{Path, PathBuf};
use std::time::Duration;

use parking_lot::{Condvar, Mutex};
use rusqlite::{Connection, OpenFlags, Transaction, TransactionBehavior};

pub use rusqlite;

/// One schema step. Versions start at 1 and have no gaps.
#[derive(Clone, Copy, Debug)]
pub struct Migration {
    /// Version reached after this step.
    pub version: u32,
    /// Short name for logs.
    pub name: &'static str,
    /// SQL run inside one transaction.
    pub sql: &'static str,
}

/// Store errors.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum StoreError {
    /// SQLite error.
    #[error("sqlite: {0}")]
    Sqlite(#[from] rusqlite::Error),
    /// Migration list is not 1, 2, 3, … in order.
    #[error(
        "migration list is not numbered 1..n without gaps (found {found} at position {position})"
    )]
    BadMigrations {
        /// Position in the list (0-based).
        position: usize,
        /// Version found there.
        found: u32,
    },
    /// The database was written by a newer app.
    #[error("database is at version {db}, newer than this app knows ({known})")]
    TooNew {
        /// Version in the file.
        db: u32,
        /// Highest version this build knows.
        known: u32,
    },
}

/// Result alias.
pub type Result<T> = std::result::Result<T, StoreError>;

/// A database file with one writer connection and a pool of readers.
#[derive(Debug)]
pub struct Store {
    path: PathBuf,
    writer: Mutex<Connection>,
    readers: Mutex<Vec<Connection>>,
    reader_free: Condvar,
}

const READERS: usize = 4;

fn configure(c: &Connection) -> rusqlite::Result<()> {
    c.busy_timeout(Duration::from_secs(5))?;
    c.pragma_update(None, "foreign_keys", "ON")?;
    Ok(())
}

impl Store {
    /// Open or create the database and bring it to the newest migration.
    ///
    /// # Errors
    /// Bad migration list, a newer database, or SQLite failure.
    pub fn open(path: impl AsRef<Path>, migrations: &[Migration]) -> Result<Self> {
        let path = path.as_ref().to_path_buf();
        let writer = Connection::open(&path)?;
        writer.pragma_update(None, "journal_mode", "WAL")?;
        writer.pragma_update(None, "synchronous", "NORMAL")?;
        configure(&writer)?;
        migrate(&writer, migrations)?;
        let mut readers = Vec::with_capacity(READERS);
        for _ in 0..READERS {
            let r = Connection::open_with_flags(
                &path,
                OpenFlags::SQLITE_OPEN_READ_ONLY
                    | OpenFlags::SQLITE_OPEN_NO_MUTEX
                    | OpenFlags::SQLITE_OPEN_URI,
            )?;
            configure(&r)?;
            readers.push(r);
        }
        Ok(Self {
            path,
            writer: Mutex::new(writer),
            readers: Mutex::new(readers),
            reader_free: Condvar::new(),
        })
    }

    /// The database file.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Current schema version.
    ///
    /// # Errors
    /// SQLite failure.
    pub fn version(&self) -> Result<u32> {
        Ok(user_version(&self.writer.lock())?)
    }

    /// Run `f` in one short write transaction (committed if `f` returns `Ok`).
    ///
    /// # Errors
    /// Whatever `f` returns, or SQLite failure.
    pub fn write<T, E: From<rusqlite::Error>>(
        &self,
        f: impl FnOnce(&Transaction<'_>) -> std::result::Result<T, E>,
    ) -> std::result::Result<T, E> {
        let mut c = self.writer.lock();
        let tx = c.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let out = f(&tx)?;
        tx.commit()?;
        Ok(out)
    }

    /// Run `f` on a pooled read-only connection (waits if all are in use).
    ///
    /// # Errors
    /// Whatever `f` returns.
    pub fn read<T, E>(
        &self,
        f: impl FnOnce(&Connection) -> std::result::Result<T, E>,
    ) -> std::result::Result<T, E> {
        let conn = {
            let mut pool = self.readers.lock();
            loop {
                if let Some(c) = pool.pop() {
                    break c;
                }
                self.reader_free.wait(&mut pool);
            }
        };
        let out = f(&conn);
        self.readers.lock().push(conn);
        self.reader_free.notify_one();
        out
    }
}

fn user_version(c: &Connection) -> rusqlite::Result<u32> {
    c.query_row("PRAGMA user_version", [], |r| r.get(0))
}

/// Apply every migration newer than the file's version, each in its own transaction.
fn migrate(c: &Connection, migrations: &[Migration]) -> Result<()> {
    for (i, m) in migrations.iter().enumerate() {
        if usize::try_from(m.version).ok() != Some(i + 1) {
            return Err(StoreError::BadMigrations {
                position: i,
                found: m.version,
            });
        }
    }
    let known = u32::try_from(migrations.len()).unwrap_or(u32::MAX);
    let db = user_version(c)?;
    if db > known {
        return Err(StoreError::TooNew { db, known });
    }
    for m in migrations.iter().skip(db as usize) {
        c.execute_batch(&format!(
            "BEGIN IMMEDIATE;\n{}\nPRAGMA user_version = {};\nCOMMIT;",
            m.sql, m.version
        ))
        .inspect_err(|_| {
            let _ = c.execute_batch("ROLLBACK;");
        })?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const M: &[Migration] = &[
        Migration {
            version: 1,
            name: "papers",
            sql: "CREATE TABLE paper (id TEXT PRIMARY KEY, title TEXT);",
        },
        Migration {
            version: 2,
            name: "paper hash",
            sql: "ALTER TABLE paper ADD COLUMN hash TEXT NOT NULL DEFAULT '';",
        },
        Migration {
            version: 3,
            name: "notes",
            sql: "CREATE TABLE note (id INTEGER PRIMARY KEY, paper TEXT NOT NULL REFERENCES paper(id), body TEXT);",
        },
    ];

    #[test]
    fn migrates_up_from_every_earlier_version() {
        for start in 0..=M.len() {
            let d = tempfile::tempdir().expect("tmp");
            let p = d.path().join("db.sqlite");
            {
                let s = Store::open(&p, &M[..start]).expect("open old");
                if start >= 1 {
                    s.write(|t| {
                        t.execute("INSERT INTO paper (id, title) VALUES ('a', 'A')", [])
                            .map(|_| ())
                    })
                    .expect("insert");
                }
            }
            let s = Store::open(&p, M).expect("open new");
            assert_eq!(s.version().expect("v"), 3);
            let n: i64 = s
                .read(|c| c.query_row("SELECT count(*) FROM paper", [], |r| r.get(0)))
                .expect("count");
            assert_eq!(n, i64::from(start >= 1));
        }
    }

    #[test]
    fn newer_database_is_refused_and_bad_lists_rejected() {
        let d = tempfile::tempdir().expect("tmp");
        let p = d.path().join("db.sqlite");
        drop(Store::open(&p, M).expect("open"));
        assert!(matches!(
            Store::open(&p, &M[..2]),
            Err(StoreError::TooNew { db: 3, known: 2 })
        ));
        let gap = [M[0], M[2]];
        assert!(matches!(
            Store::open(d.path().join("x.sqlite"), &gap),
            Err(StoreError::BadMigrations {
                position: 1,
                found: 3
            })
        ));
    }

    #[test]
    fn failed_write_rolls_back_and_readers_see_commits() {
        let d = tempfile::tempdir().expect("tmp");
        let s = Store::open(d.path().join("db.sqlite"), M).expect("open");
        let r: std::result::Result<(), rusqlite::Error> = s.write(|t| {
            t.execute("INSERT INTO paper (id) VALUES ('x')", [])?;
            t.execute(
                "INSERT INTO note (paper, body) VALUES ('missing', 'fk fails')",
                [],
            )?;
            Ok(())
        });
        assert!(r.is_err());
        let count = |s: &Store| {
            s.read(|c| c.query_row("SELECT count(*) FROM paper", [], |r| r.get::<_, i64>(0)))
                .expect("count")
        };
        assert_eq!(count(&s), 0);
        s.write(|t| {
            t.execute("INSERT INTO paper (id) VALUES ('y')", [])
                .map(|_| ())
        })
        .expect("insert");
        assert_eq!(count(&s), 1);
    }

    #[test]
    fn readers_work_in_parallel_threads() {
        let d = tempfile::tempdir().expect("tmp");
        let s = std::sync::Arc::new(Store::open(d.path().join("db.sqlite"), M).expect("open"));
        let hs: Vec<_> = (0..16)
            .map(|_| {
                let s = s.clone();
                std::thread::spawn(move || {
                    s.read(|c| c.query_row("SELECT 1", [], |r| r.get::<_, i64>(0)))
                        .expect("read")
                })
            })
            .collect();
        for h in hs {
            assert_eq!(h.join().expect("join"), 1);
        }
    }
}
