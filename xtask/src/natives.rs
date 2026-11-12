//! Download pinned prebuilt native libraries and verify their hashes.
//!
//! Reads `natives.toml`. Each entry with `url` and `sha256` is downloaded to
//! `.natives/downloads/`, verified, and (if `extract = true`) unpacked with the system
//! `tar` into `.natives/<name>/`. Already verified downloads are not fetched again.

use std::fs;
use std::io::Read;
use std::path::Path;
use std::process::Command;

use anyhow::Context;
use serde::Deserialize;
use sha2::{Digest, Sha256};

#[derive(Deserialize)]
struct Manifest {
    #[serde(default)]
    native: Vec<Native>,
}

#[derive(Deserialize)]
struct Native {
    name: String,
    version: String,
    os: String,
    #[serde(default)]
    url: String,
    #[serde(default)]
    sha256: String,
    #[serde(default)]
    extract: bool,
}

pub fn fetch(root: &Path) -> anyhow::Result<()> {
    let manifest: Manifest = toml::from_str(
        &fs::read_to_string(root.join("natives.toml")).context("reading natives.toml")?,
    )
    .context("parsing natives.toml")?;
    let pinned: Vec<&Native> = manifest
        .native
        .iter()
        .filter(|n| !n.url.is_empty() && !n.sha256.is_empty())
        .collect();
    if pinned.is_empty() {
        println!("fetch-natives: nothing pinned yet (filled in by spikes P0.S2, S3, S5)");
        return Ok(());
    }
    let downloads = root.join(".natives").join("downloads");
    fs::create_dir_all(&downloads)?;
    for n in pinned {
        let file_name = n.url.rsplit('/').next().unwrap_or(&n.name);
        let file = downloads.join(file_name);
        if !(file.exists() && sha256_of(&file)? == n.sha256.to_lowercase()) {
            println!("fetching {} {} ({})", n.name, n.version, n.os);
            download(&n.url, &file)?;
            let got = sha256_of(&file)?;
            if got != n.sha256.to_lowercase() {
                fs::remove_file(&file).ok();
                anyhow::bail!(
                    "{}: hash mismatch (expected {}, got {got})",
                    n.name,
                    n.sha256
                );
            }
        }
        if n.extract {
            let dest = root.join(".natives").join(&n.name);
            fs::create_dir_all(&dest)?;
            // Arguments as a vector, never a shell string (§6.4 rule 7).
            let status = Command::new("tar")
                .arg("-xf")
                .arg(&file)
                .arg("-C")
                .arg(&dest)
                .status()?;
            anyhow::ensure!(status.success(), "{}: extraction failed", n.name);
        }
        println!("ok  {} {}", n.name, n.version);
    }
    Ok(())
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

fn sha256_of(file: &Path) -> anyhow::Result<String> {
    let mut f = fs::File::open(file)?;
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
