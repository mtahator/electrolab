//! Repository automation for `ElectroPaper Lab`.
//!
//! ```text
//! cargo xtask arch-check      dependency rules + OS isolation (technical-structure.md §3.3)
//! cargo xtask set-name "<N>" rename the product everywhere (source: product.toml)
//! cargo xtask gen             JSON Schemas (spec/schemas) and TypeScript bindings (ui/bindings)
//! cargo xtask schema-check    committed schemas and bindings match the generated ones
//! cargo xtask fetch-natives [name...]  download pinned native libraries and verify their hashes
//! cargo xtask fetch-fixtures  download pinned fixture papers and verify their hashes
//! ```

mod arch;
mod download;
mod fixtures;
mod layout;
mod natives;
mod os_scan;
mod product;
mod schema;
mod ts;

use std::path::{Path, PathBuf};
use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let root = repo_root();
    let result = match args.first().map(String::as_str) {
        Some("arch-check") => arch_check(&root),
        Some("set-name") => match args.get(1) {
            Some(n) => product::set_name(&root, n),
            None => Err(anyhow::anyhow!("usage: cargo xtask set-name \"<Name>\"")),
        },
        Some("gen") => schema::generate(&root),
        Some("schema-check") => schema::check(&root),
        Some("fetch-natives") => natives::fetch(&root, &args[1..]),
        Some("fetch-fixtures") => fixtures::fetch(&root),
        _ => {
            eprintln!(
                "usage: cargo xtask <arch-check | gen | schema-check | set-name | fetch-natives | fetch-fixtures>"
            );
            return ExitCode::from(2);
        }
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("error: {err:#}");
            ExitCode::FAILURE
        }
    }
}

fn arch_check(root: &Path) -> anyhow::Result<()> {
    let metadata = cargo_metadata::MetadataCommand::new()
        .manifest_path(root.join("Cargo.toml"))
        .no_deps()
        .exec()?;
    let mut violations = arch::check(&metadata, root);
    violations.extend(os_scan::scan(root)?);
    violations.extend(product::check(root)?);
    if violations.is_empty() {
        println!("arch-check: ok ({} crates)", metadata.packages.len());
        return Ok(());
    }
    for v in &violations {
        eprintln!("  - {v}");
    }
    anyhow::bail!("arch-check: {} violation(s)", violations.len())
}

/// The workspace root: the parent of this crate's directory.
fn repo_root() -> PathBuf {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest_dir
        .parent()
        .map_or(manifest_dir.clone(), Path::to_path_buf)
}
