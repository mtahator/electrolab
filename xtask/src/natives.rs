//! Download pinned prebuilt native libraries and verify their hashes.
//!
//! Reads `natives.toml`. Each entry with `url` and `sha256` is downloaded to
//! `.natives/downloads/`, verified, and (if `extract = true`) unpacked with the system
//! `tar` into `.natives/<name>/`. Already verified downloads are not fetched again.

use std::fs;
use std::path::Path;
use std::process::Command;

use anyhow::Context;
use serde::Deserialize;

use crate::download;

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
        println!("fetch-natives: nothing pinned yet");
        return Ok(());
    }
    let downloads = root.join(".natives").join("downloads");
    for n in pinned {
        let file_name = n.url.rsplit('/').next().unwrap_or(&n.name);
        let file = downloads.join(file_name);
        let outcome = download::ensure(&n.url, &n.sha256, &file)
            .with_context(|| format!("{} {} ({})", n.name, n.version, n.os))?;
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
        println!("ok  {} {} ({outcome:?})", n.name, n.version);
    }
    Ok(())
}
