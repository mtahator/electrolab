//! Pinned downloads: fetch once over HTTPS, verify SHA-256, reuse while the hash still matches.

use std::fs;
use std::io::Read;
use std::path::Path;
use std::process::Command;

use anyhow::Context;
use sha2::{Digest, Sha256};

/// What [`ensure`] did.
#[derive(Debug, PartialEq, Eq)]
pub enum Outcome {
    /// The file was already present with the right hash.
    Cached,
    /// The file was downloaded and verified.
    Downloaded,
}

/// Make sure `dest` exists with SHA-256 `sha256`, downloading it from `url` if needed.
/// A file with the wrong hash is never left at `dest`.
pub fn ensure(url: &str, sha256: &str, dest: &Path) -> anyhow::Result<Outcome> {
    let want = sha256.to_lowercase();
    if dest.exists() && sha256_of(dest)? == want {
        return Ok(Outcome::Cached);
    }
    if let Some(dir) = dest.parent() {
        fs::create_dir_all(dir)?;
    }
    download(url, dest)?;
    let got = sha256_of(dest)?;
    if got != want {
        fs::remove_file(dest).ok();
        anyhow::bail!(
            "{}: hash mismatch (expected {want}, got {got})",
            dest.display()
        );
    }
    Ok(Outcome::Downloaded)
}

fn download(url: &str, to: &Path) -> anyhow::Result<()> {
    anyhow::ensure!(
        url.starts_with("https://"),
        "only https downloads are allowed: {url}"
    );
    // The system `curl` (Windows 10+, macOS, Linux) keeps C code such as `ring` out of
    // this tool, so the workspace still cross-checks for Linux and macOS from Windows.
    // Arguments as a vector, never a shell string (§6.4 rule 7).
    let partial = to.with_extension("partial");
    let status = Command::new("curl")
        .args([
            "--fail",
            "--location",
            "--silent",
            "--show-error",
            "--proto",
            "=https",
        ])
        .args([
            "--retry",
            "3",
            "--user-agent",
            "electropaper-xtask",
            "--output",
        ])
        .arg(&partial)
        .arg(url)
        .status()
        .context("running curl")?;
    if !status.success() {
        fs::remove_file(&partial).ok();
        anyhow::bail!("downloading {url} failed ({status})");
    }
    fs::rename(&partial, to)?;
    Ok(())
}

/// SHA-256 of a file, as lowercase hex, streamed (large files never load into memory).
pub fn sha256_of(file: &Path) -> anyhow::Result<String> {
    let mut f = fs::File::open(file).with_context(|| format!("opening {}", file.display()))?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 1 << 16];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}
