//! Fetch and verify the fixture papers listed in `fixtures/papers.toml` (task P0.C.1).
//!
//! Entries with a `url` are downloaded into `.fixtures/papers/` (not in Git: about 100 MB).
//! Entries with `committed = true` are already in `fixtures/papers/`; only their hash is checked.
//! Every entry must carry a licence; downloads pause between requests to stay polite.

use std::fs;
use std::path::Path;
use std::thread::sleep;
use std::time::Duration;

use anyhow::Context;
use serde::Deserialize;

use crate::download::{self, Outcome};

#[derive(Deserialize)]
struct Manifest {
    paper: Vec<Paper>,
}

#[derive(Deserialize)]
struct Paper {
    file: String,
    #[serde(default)]
    url: Option<String>,
    sha256: String,
    licence: String,
    #[serde(default)]
    committed: bool,
}

pub fn fetch(root: &Path) -> anyhow::Result<()> {
    let manifest: Manifest = toml::from_str(
        &fs::read_to_string(root.join("fixtures").join("papers.toml"))
            .context("reading fixtures/papers.toml")?,
    )
    .context("parsing fixtures/papers.toml")?;
    let cache = root.join(".fixtures").join("papers");
    let (mut cached, mut downloaded) = (0, 0);
    for p in &manifest.paper {
        anyhow::ensure!(
            !p.licence.trim().is_empty(),
            "{}: no licence recorded",
            p.file
        );
        anyhow::ensure!(
            !p.file.contains(['/', '\\']),
            "{}: file must be a plain name",
            p.file
        );
        if p.committed {
            let path = root.join("fixtures").join("papers").join(&p.file);
            let got = download::sha256_of(&path)?;
            anyhow::ensure!(
                got == p.sha256.to_lowercase(),
                "{}: committed file changed (expected {}, got {got})",
                p.file,
                p.sha256
            );
            cached += 1;
            continue;
        }
        let url = p
            .url
            .as_deref()
            .with_context(|| format!("{}: needs `url` or `committed = true`", p.file))?;
        match download::ensure(url, &p.sha256, &cache.join(&p.file))? {
            Outcome::Cached => cached += 1,
            Outcome::Downloaded => {
                downloaded += 1;
                println!("fetched {}", p.file);
                sleep(Duration::from_secs(3)); // arXiv asks for at least 3 s between requests
            }
        }
    }
    println!(
        "fetch-fixtures: {} papers ok ({downloaded} downloaded, {cached} already present)",
        manifest.paper.len()
    );
    Ok(())
}
