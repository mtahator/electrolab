//! Open files and web addresses with the user's apps; reveal files in Explorer.

use std::ffi::OsStr;
use std::os::windows::process::CommandExt;
use std::path::Path;

use ep_os::{OsError, Result, ShellOpen};
use windows_sys::Win32::UI::Shell::ShellExecuteW;
use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

use crate::wide;

/// The Windows shell.
#[derive(Clone, Copy, Debug, Default)]
pub struct WinShell;

#[allow(unsafe_code)]
fn shell_open(target: &OsStr) -> Result<()> {
    let verb = wide(OsStr::new("open"));
    let file = wide(target);
    // SAFETY: both strings are NUL-terminated UTF-16 buffers that outlive the call; the other
    // pointer arguments are allowed to be null.
    let r = unsafe {
        ShellExecuteW(
            std::ptr::null_mut(),
            verb.as_ptr(),
            file.as_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            SW_SHOWNORMAL,
        )
    };
    // Per the Win32 contract, values above 32 mean success.
    if r as usize > 32 {
        Ok(())
    } else {
        Err(std::io::Error::last_os_error().into())
    }
}

impl ShellOpen for WinShell {
    fn open_path(&self, path: &Path) -> Result<()> {
        if !path.exists() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                path.display().to_string(),
            )
            .into());
        }
        shell_open(path.as_os_str())
    }

    fn reveal(&self, path: &Path) -> Result<()> {
        // Explorer parses its own command line; the path is quoted (paths cannot contain `"`).
        std::process::Command::new("explorer.exe")
            .raw_arg(format!("/select,\"{}\"", path.display()))
            .spawn()?;
        Ok(())
    }

    fn open_url(&self, url: &str) -> Result<()> {
        if !(url.starts_with("https://") || url.starts_with("http://")) {
            return Err(OsError::Unsupported("only http(s) addresses"));
        }
        shell_open(OsStr::new(url))
    }
}
