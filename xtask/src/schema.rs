//! Committed JSON Schemas must match what the code generates (Draft 2.2 §8.6).
//!
//! Generators write into `spec/schemas/`. This check regenerates and then asks Git whether
//! anything under `spec/` changed. A difference means a contract changed without the
//! committed schema (and its version) being updated.
//!
//! Until the first contract crate exists (Phase 1, M1.GEN.3) there is nothing to generate,
//! so the check only verifies that `spec/` is clean.

use std::path::Path;
use std::process::Command;

pub fn check(root: &Path) -> anyhow::Result<()> {
    // Phase 1 adds: `cargo run -p ep-schema-gen -- spec/schemas` here.
    let output = Command::new("git")
        .args(["status", "--porcelain", "--", "spec"])
        .current_dir(root)
        .output()?;
    if !output.status.success() {
        anyhow::bail!(
            "git status failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let changed = String::from_utf8_lossy(&output.stdout);
    if changed.trim().is_empty() {
        println!("schema-check: ok");
        Ok(())
    } else {
        anyhow::bail!(
            "schema-check: generated schemas differ from the committed ones:\n{changed}\
             bump the contract version and commit the new schema (§8.4)"
        )
    }
}
