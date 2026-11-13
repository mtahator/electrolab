//! Generated contract surfaces (Draft 2.2 §8.6, P1.M1.GEN.1/3).
//!
//! `gen` asks the `ep` CLI for every registered command's input and output JSON Schema
//! (`spec/schemas/`), then writes TypeScript from them (`ui/bindings/src/`).
//! `check` regenerates and asks Git whether anything changed: a difference means a contract
//! changed without the committed schema (and its version) being updated.

use std::path::Path;
use std::process::Command;

const GENERATED: &[&str] = &[
    "spec/schemas",
    "ui/bindings/src/types.ts",
    "ui/bindings/src/commands.ts",
];

pub fn generate(root: &Path) -> anyhow::Result<()> {
    let dir = root.join("spec").join("schemas");
    if dir.exists() {
        // Start clean so a removed command also disappears from the committed schemas.
        for e in std::fs::read_dir(&dir)? {
            let p = e?.path();
            if p.extension().is_some_and(|x| x == "json") {
                std::fs::remove_file(p)?;
            }
        }
    }
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".into());
    let status = Command::new(cargo)
        .args(["run", "--quiet", "-p", "ep-cli", "--", "schemas"])
        .arg(&dir)
        .current_dir(root)
        .status()?;
    if !status.success() {
        anyhow::bail!("`ep schemas` failed");
    }
    crate::ts::generate(&dir, &root.join("ui").join("bindings").join("src"))?;
    println!("gen: schemas in spec/schemas, TypeScript in ui/bindings/src");
    Ok(())
}

pub fn check(root: &Path) -> anyhow::Result<()> {
    generate(root)?;
    let output = Command::new("git")
        .args(["status", "--porcelain", "--"])
        .args(GENERATED)
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
            "schema-check: generated schemas or bindings differ from the committed ones:\n{changed}\
             review the contract change, bump its version and commit the regenerated files (§8.4)"
        )
    }
}
