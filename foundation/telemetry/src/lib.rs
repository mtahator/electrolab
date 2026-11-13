//! Tracing setup (P1.M1.STO.5). Engines only use `tracing` macros; hosts call [`init`] once.
//!
//! Development: readable spans on stderr. Release: JSON lines appended to a log file in
//! the app's log folder (rotation is done by the host on start). Filter with `EP_LOG`, e.g.
//! `EP_LOG=info,ep_bus=debug`.
#![deny(missing_docs)]

use std::fs::OpenOptions;
use std::io;
use std::path::Path;
use std::sync::Mutex;

pub use tracing;
use tracing_subscriber::EnvFilter;
use tracing_subscriber::fmt::format::FmtSpan;

/// Where logs go.
#[derive(Clone, Copy, Debug)]
pub enum Output<'a> {
    /// Human-readable lines on stderr, with span close timings.
    Console,
    /// JSON lines appended to this file.
    JsonFile(&'a Path),
}

/// Install the global subscriber. Calling it twice is harmless (the second call returns
/// `Ok(false)`).
///
/// # Errors
/// The log file cannot be opened.
pub fn init(output: Output<'_>, default_filter: &str) -> io::Result<bool> {
    let filter =
        EnvFilter::try_from_env("EP_LOG").unwrap_or_else(|_| EnvFilter::new(default_filter));
    let installed = match output {
        Output::Console => tracing_subscriber::fmt()
            .with_env_filter(filter)
            .with_span_events(FmtSpan::CLOSE)
            .with_writer(io::stderr)
            .try_init(),
        Output::JsonFile(path) => {
            if let Some(dir) = path.parent() {
                std::fs::create_dir_all(dir)?;
            }
            let file = OpenOptions::new().create(true).append(true).open(path)?;
            tracing_subscriber::fmt()
                .json()
                .with_env_filter(filter)
                .with_span_events(FmtSpan::CLOSE)
                .with_writer(Mutex::new(file))
                .try_init()
        }
    };
    Ok(installed.is_ok())
}
