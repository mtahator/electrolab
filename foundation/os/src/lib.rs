//! OS ports (Draft 2.2 §6.4, P1.M1.OS.1): every OS service the app uses, as a trait.
//!
//! Engines and hosts depend on these traits; only hosts pick an adapter (`ep-os-windows`
//! today, macOS and Linux in Phase 3). Every port has a fake in [`fake`] for tests.
//!
//! Ports implemented by the host's UI toolkit rather than an OS crate: [`FileDialog`],
//! [`Notifier`], [`Accelerators`] and [`Updater`] (Tauri plugins), wrapped behind these traits.
#![deny(missing_docs)]

pub mod fake;
pub mod paths;

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use futures::future::BoxFuture;
use tokio::io::{AsyncRead, AsyncWrite};

/// Errors from any OS adapter.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum OsError {
    /// Disk or system call error.
    #[error("{0}")]
    Io(#[from] std::io::Error),
    /// The service is not available on this system.
    #[error("not supported here: {0}")]
    Unsupported(&'static str),
    /// The user cancelled a dialog.
    #[error("cancelled by the user")]
    Cancelled,
}

/// Result alias.
pub type Result<T> = std::result::Result<T, OsError>;

/// Per-user folders of the app.
pub trait AppDirs: Send + Sync {
    /// Durable user data (the default library lives here).
    fn data(&self) -> PathBuf;
    /// Settings.
    fn config(&self) -> PathBuf;
    /// Disposable caches (rendered pages, model outputs that can be recomputed).
    fn cache(&self) -> PathBuf;
    /// Log files.
    fn logs(&self) -> PathBuf;
}

/// All app folders under one root: `data/`, `config/`, `cache/`, `logs/`. Used for portable
/// installs, tests, and the `EP_DATA_DIR` override of the hosts.
#[derive(Clone, Debug)]
pub struct RootDirs(pub PathBuf);

impl AppDirs for RootDirs {
    fn data(&self) -> PathBuf {
        self.0.join("data")
    }
    fn config(&self) -> PathBuf {
        self.0.join("config")
    }
    fn cache(&self) -> PathBuf {
        self.0.join("cache")
    }
    fn logs(&self) -> PathBuf {
        self.0.join("logs")
    }
}

/// Secrets (API keys, tokens) in the OS key store — never in files or settings.
pub trait KeyStore: Send + Sync {
    /// Read a secret.
    ///
    /// # Errors
    /// The store cannot be reached.
    fn get(&self, name: &str) -> Result<Option<String>>;
    /// Store or replace a secret.
    ///
    /// # Errors
    /// The store cannot be reached.
    fn set(&self, name: &str, secret: &str) -> Result<()>;
    /// Remove a secret (no error if absent).
    ///
    /// # Errors
    /// The store cannot be reached.
    fn delete(&self, name: &str) -> Result<()>;
}

/// A program to start.
#[derive(Clone, Debug, Default)]
pub struct ProcessSpec {
    /// Executable.
    pub program: PathBuf,
    /// Arguments.
    pub args: Vec<OsString>,
    /// Working folder.
    pub cwd: Option<PathBuf>,
    /// Extra environment variables.
    pub env: Vec<(OsString, OsString)>,
    /// Append the child's stdout and stderr to this file (otherwise discarded).
    pub log_file: Option<PathBuf>,
}

/// A running child process. It never outlives the app (ADR-009).
pub trait ChildProcess: Send {
    /// OS process id.
    fn id(&self) -> u32;
    /// Exit code if it has exited.
    ///
    /// # Errors
    /// The OS cannot be asked.
    fn try_wait(&mut self) -> Result<Option<i32>>;
    /// Stop it now.
    ///
    /// # Errors
    /// The OS refused.
    fn kill(&mut self) -> Result<()>;
}

/// Starts helper processes (model server, page-model worker) bound to the app's lifetime:
/// if the app exits in any way, its children exit too. No handles are inherited.
pub trait ProcessSpawner: Send + Sync {
    /// Start a process.
    ///
    /// # Errors
    /// The program cannot be started.
    fn spawn(&self, spec: &ProcessSpec) -> Result<Box<dyn ChildProcess>>;
}

/// Hand things to the user's other apps.
pub trait ShellOpen: Send + Sync {
    /// Open a file with its usual app.
    ///
    /// # Errors
    /// The shell refused.
    fn open_path(&self, path: &Path) -> Result<()>;
    /// Show a file selected in the file manager.
    ///
    /// # Errors
    /// The shell refused.
    fn reveal(&self, path: &Path) -> Result<()>;
    /// Open a web or mail address (`http`, `https`, `mailto` only).
    ///
    /// # Errors
    /// Another scheme, or the shell refused.
    fn open_url(&self, url: &str) -> Result<()>;
    /// Print a file with its usual app (the system print dialog or default printer).
    ///
    /// # Errors
    /// The file is missing or the shell refused.
    fn print(&self, path: &Path) -> Result<()>;
}

/// Whether a link may be handed to the shell: `http`, `https` or `mailto` only.
#[must_use]
pub fn is_safe_url(url: &str) -> bool {
    let u = url.trim().to_ascii_lowercase();
    (u.starts_with("https://") || u.starts_with("http://") || u.starts_with("mailto:"))
        && !u.contains(char::is_whitespace)
}

/// A file-type filter for dialogs, e.g. `("PDF", &["pdf"])`.
pub type Filter<'a> = (&'a str, &'a [&'a str]);

/// Native file dialogs.
pub trait FileDialog: Send + Sync {
    /// Pick existing files.
    fn pick_files<'a>(
        &'a self,
        title: &'a str,
        filters: &'a [Filter<'a>],
    ) -> BoxFuture<'a, Result<Vec<PathBuf>>>;
    /// Choose where to save.
    fn save_file<'a>(
        &'a self,
        title: &'a str,
        suggested: &'a str,
        filters: &'a [Filter<'a>],
    ) -> BoxFuture<'a, Result<PathBuf>>;
}

/// Watches folders for changes (library folders, Phase 1 M2).
pub trait FileWatcher: Send + Sync {
    /// Start watching; changed paths are sent to `tx`.
    ///
    /// # Errors
    /// The folder cannot be watched.
    fn watch(&self, folder: &Path, tx: tokio::sync::mpsc::UnboundedSender<PathBuf>) -> Result<()>;
    /// Stop watching.
    fn unwatch(&self, folder: &Path);
}

/// Desktop notifications.
pub trait Notifier: Send + Sync {
    /// Show a notification.
    ///
    /// # Errors
    /// Notifications are blocked or unavailable.
    fn notify(&self, title: &str, body: &str) -> Result<()>;
}

/// Global keyboard shortcuts.
pub trait Accelerators: Send + Sync {
    /// Register a shortcut such as `Ctrl+Shift+E`; `id` is reported when pressed.
    ///
    /// # Errors
    /// Taken by another app or invalid.
    fn register(&self, id: &str, shortcut: &str) -> Result<()>;
    /// Remove a shortcut.
    fn unregister(&self, id: &str);
}

/// App updates.
pub trait Updater: Send + Sync {
    /// Look for an update; returns its version.
    fn check(&self) -> BoxFuture<'_, Result<Option<String>>>;
    /// Download, verify the signature and install on next start.
    fn install(&self) -> BoxFuture<'_, Result<()>>;
}

/// Isolated execution for user code (declared in Phase 1, implemented in Phase 2).
pub trait Sandbox: Send + Sync {
    /// Run a process inside the sandbox.
    ///
    /// # Errors
    /// Not available yet, or the sandbox refused.
    fn run(&self, spec: &ProcessSpec) -> Result<Box<dyn ChildProcess>>;
}

/// A connected local byte stream (one side of an IPC link).
pub trait Stream: AsyncRead + AsyncWrite + Send + Unpin {}
impl<T: AsyncRead + AsyncWrite + Send + Unpin> Stream for T {}

/// Local inter-process links (named pipes on Windows, Unix sockets elsewhere), reachable only
/// by the current user on this machine.
pub trait LocalIpc: Send + Sync {
    /// Listen on `name` and accept one connection.
    fn accept<'a>(&'a self, name: &'a str) -> BoxFuture<'a, Result<Box<dyn Stream>>>;
    /// Connect to `name`, waiting up to `timeout_ms` for the listener.
    fn connect<'a>(
        &'a self,
        name: &'a str,
        timeout_ms: u64,
    ) -> BoxFuture<'a, Result<Box<dyn Stream>>>;
}
