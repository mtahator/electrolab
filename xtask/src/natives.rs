//! Download pinned prebuilt native libraries and verify their hashes.
//!
//! Reads `natives.toml`. Each entry with `url` and `sha256` is downloaded to
//! `.natives/downloads/`, verified, and (if `extract = true`) unpacked with the system
//! `tar` into `.natives/<name>/`. Already verified downloads are not fetched again.
//!
//! An entry may list `files` (`"<path under .natives/<name>>" = "<path inside the archive>"`):
//! then the archive is unpacked aside and only those files are copied into place. This keeps
//! the paths the build uses when a library has to come from a different package (ONNX Runtime
//! from NuGet since its GitHub releases went offline, 2026-11-18).

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
    #[serde(default)]
    files: std::collections::BTreeMap<String, String>,
}

/// Fetch every pinned native library, or only those whose name is in `only` (when not empty).
pub fn fetch(root: &Path, only: &[String]) -> anyhow::Result<()> {
    let manifest: Manifest = toml::from_str(
        &fs::read_to_string(root.join("natives.toml")).context("reading natives.toml")?,
    )
    .context("parsing natives.toml")?;
    let pinned: Vec<&Native> = manifest
        .native
        .iter()
        .filter(|n| !n.url.is_empty() && !n.sha256.is_empty())
        .filter(|n| only.is_empty() || only.iter().any(|o| o == &n.name))
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
            // With a file list, unpack aside and copy only the listed files into place.
            let unpack = if n.files.is_empty() {
                dest.clone()
            } else {
                downloads.join(format!("{}-{}-unpacked", n.name, n.version))
            };
            fs::create_dir_all(&unpack)?;
            // Arguments as a vector, never a shell string (§6.4 rule 7).
            let status = Command::new("tar")
                .arg("-xf")
                .arg(&file)
                .arg("-C")
                .arg(&unpack)
                .status()?;
            anyhow::ensure!(status.success(), "{}: extraction failed", n.name);
            for (to, from) in &n.files {
                let src = unpack.join(from);
                let target = dest.join(to);
                if let Some(dir) = target.parent() {
                    fs::create_dir_all(dir)?;
                }
                fs::copy(&src, &target)
                    .with_context(|| format!("{}: {} is not in the package", n.name, from))?;
            }
        }
        println!("ok  {} {} ({outcome:?})", n.name, n.version);
    }
    Ok(())
}
