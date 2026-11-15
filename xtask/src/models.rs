//! Page models (ADR-007, ADR-013): download the pinned files the Document engine knows into
//! `.natives/models/<folder>/<file>`, verifying each SHA-256. Bundled ones (PP-OCR) are packed
//! into the installer from there; `--all` also fetches the optional ones (the layout model).

use std::path::Path;

use crate::download;

/// Fetch the bundled models, or every model with `--all`.
pub fn fetch(root: &Path, args: &[String]) -> anyhow::Result<()> {
    let all = args.iter().any(|a| a == "--all");
    let dir = root.join(".natives").join("models");
    for m in ep_document::Document::model_specs()
        .iter()
        .filter(|m| all || m.bundled)
    {
        let outcome = download::ensure(m.url, m.sha256, &m.path(&dir))?;
        println!("ok  {} {} ({outcome:?})", m.id, m.file);
    }
    Ok(())
}
