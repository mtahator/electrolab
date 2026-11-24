//! Windows adapters of the `ep-os` ports (P1.M1.OS.2).
//!
//! This is a boundary crate: the only crate allowed Win32 calls. Each `unsafe` block is
//! allowed one item at a time and carries a `SAFETY:` comment.
#![cfg(windows)]
#![deny(missing_docs)]

mod dirs;
mod ipc;
mod keystore;
mod machine;
mod process;
mod shell;

pub use dirs::WinDirs;
pub use ipc::NamedPipes;
pub use keystore::CredentialStore;
pub use machine::WinMachine;
pub use process::JobSpawner;
pub use shell::WinShell;

/// UTF-16, NUL-terminated, for Win32 `W` functions.
fn wide(s: &std::ffi::OsStr) -> Vec<u16> {
    use std::os::windows::ffi::OsStrExt;
    s.encode_wide().chain(std::iter::once(0)).collect()
}
