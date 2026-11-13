//! Fakes of every port, for tests of engines and hosts.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use futures::future::BoxFuture;
use parking_lot::Mutex;

use crate::{
    Accelerators, ChildProcess, FileDialog, FileWatcher, Filter, KeyStore, LocalIpc, Notifier,
    OsError, ProcessSpawner, ProcessSpec, Result, Sandbox, ShellOpen, Stream, Updater,
};

/// App folders under one root (e.g. a temporary folder).
pub use crate::RootDirs as FakeDirs;

/// Secrets in memory.
#[derive(Debug, Default)]
pub struct MemoryKeyStore(Mutex<HashMap<String, String>>);

impl KeyStore for MemoryKeyStore {
    fn get(&self, name: &str) -> Result<Option<String>> {
        Ok(self.0.lock().get(name).cloned())
    }
    fn set(&self, name: &str, secret: &str) -> Result<()> {
        self.0.lock().insert(name.into(), secret.into());
        Ok(())
    }
    fn delete(&self, name: &str) -> Result<()> {
        self.0.lock().remove(name);
        Ok(())
    }
}

/// Records what the code asked the OS to do.
#[derive(Debug, Default)]
pub struct Recorder(pub Mutex<Vec<String>>);

impl Recorder {
    fn note(&self, s: String) {
        self.0.lock().push(s);
    }
    /// Everything recorded so far.
    #[must_use]
    pub fn calls(&self) -> Vec<String> {
        self.0.lock().clone()
    }
}

impl ShellOpen for Recorder {
    fn open_path(&self, path: &Path) -> Result<()> {
        self.note(format!("open {}", path.display()));
        Ok(())
    }
    fn reveal(&self, path: &Path) -> Result<()> {
        self.note(format!("reveal {}", path.display()));
        Ok(())
    }
    fn open_url(&self, url: &str) -> Result<()> {
        if !(url.starts_with("https://") || url.starts_with("http://")) {
            return Err(OsError::Unsupported("only http(s) addresses"));
        }
        self.note(format!("url {url}"));
        Ok(())
    }
}

impl Notifier for Recorder {
    fn notify(&self, title: &str, body: &str) -> Result<()> {
        self.note(format!("notify {title}: {body}"));
        Ok(())
    }
}

impl Accelerators for Recorder {
    fn register(&self, id: &str, shortcut: &str) -> Result<()> {
        self.note(format!("accel {id}={shortcut}"));
        Ok(())
    }
    fn unregister(&self, id: &str) {
        self.note(format!("unaccel {id}"));
    }
}

impl FileWatcher for Recorder {
    fn watch(&self, folder: &Path, _tx: tokio::sync::mpsc::UnboundedSender<PathBuf>) -> Result<()> {
        self.note(format!("watch {}", folder.display()));
        Ok(())
    }
    fn unwatch(&self, folder: &Path) {
        self.note(format!("unwatch {}", folder.display()));
    }
}

impl Updater for Recorder {
    fn check(&self) -> BoxFuture<'_, Result<Option<String>>> {
        Box::pin(async { Ok(None) })
    }
    fn install(&self) -> BoxFuture<'_, Result<()>> {
        Box::pin(async { Err(OsError::Unsupported("no update in tests")) })
    }
}

/// Dialogs that answer with prepared paths (or "cancelled" when empty).
#[derive(Debug, Default)]
pub struct ScriptedDialog(pub Mutex<Vec<PathBuf>>);

impl FileDialog for ScriptedDialog {
    fn pick_files<'a>(
        &'a self,
        _title: &'a str,
        _filters: &'a [Filter<'a>],
    ) -> BoxFuture<'a, Result<Vec<PathBuf>>> {
        let v = std::mem::take(&mut *self.0.lock());
        Box::pin(async move {
            if v.is_empty() {
                Err(OsError::Cancelled)
            } else {
                Ok(v)
            }
        })
    }
    fn save_file<'a>(
        &'a self,
        _title: &'a str,
        _suggested: &'a str,
        _filters: &'a [Filter<'a>],
    ) -> BoxFuture<'a, Result<PathBuf>> {
        let v = self.0.lock().pop();
        Box::pin(async move { v.ok_or(OsError::Cancelled) })
    }
}

/// A child that "runs" until killed.
#[derive(Debug)]
pub struct FakeChild {
    id: u32,
    exited: Option<i32>,
}

impl ChildProcess for FakeChild {
    fn id(&self) -> u32 {
        self.id
    }
    fn try_wait(&mut self) -> Result<Option<i32>> {
        Ok(self.exited)
    }
    fn kill(&mut self) -> Result<()> {
        self.exited = Some(1);
        Ok(())
    }
}

/// Spawner that records specs and returns fake children.
#[derive(Debug, Default)]
pub struct FakeSpawner(pub Mutex<Vec<ProcessSpec>>);

impl ProcessSpawner for FakeSpawner {
    fn spawn(&self, spec: &ProcessSpec) -> Result<Box<dyn ChildProcess>> {
        let mut v = self.0.lock();
        v.push(spec.clone());
        Ok(Box::new(FakeChild {
            id: u32::try_from(v.len()).unwrap_or(u32::MAX),
            exited: None,
        }))
    }
}

impl Sandbox for FakeSpawner {
    fn run(&self, _spec: &ProcessSpec) -> Result<Box<dyn ChildProcess>> {
        Err(OsError::Unsupported("sandbox arrives in Phase 2"))
    }
}

/// In-memory IPC: `accept(name)` and `connect(name)` meet through a shared table.
#[derive(Clone, Debug, Default)]
pub struct MemoryIpc(
    Arc<Mutex<HashMap<String, tokio::sync::oneshot::Sender<tokio::io::DuplexStream>>>>,
);

impl LocalIpc for MemoryIpc {
    fn accept<'a>(&'a self, name: &'a str) -> BoxFuture<'a, Result<Box<dyn Stream>>> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.0.lock().insert(name.into(), tx);
        Box::pin(async move {
            rx.await
                .map(|s| Box::new(s) as Box<dyn Stream>)
                .map_err(|_| OsError::Unsupported("listener dropped"))
        })
    }
    fn connect<'a>(
        &'a self,
        name: &'a str,
        timeout_ms: u64,
    ) -> BoxFuture<'a, Result<Box<dyn Stream>>> {
        Box::pin(async move {
            let deadline =
                tokio::time::Instant::now() + std::time::Duration::from_millis(timeout_ms);
            loop {
                if let Some(tx) = self.0.lock().remove(name) {
                    let (a, b) = tokio::io::duplex(1 << 16);
                    tx.send(b)
                        .map_err(|_| OsError::Unsupported("listener gone"))?;
                    return Ok(Box::new(a) as Box<dyn Stream>);
                }
                if tokio::time::Instant::now() >= deadline {
                    return Err(
                        std::io::Error::new(std::io::ErrorKind::TimedOut, "no listener").into(),
                    );
                }
                tokio::time::sleep(std::time::Duration::from_millis(1)).await;
            }
        })
    }
}
