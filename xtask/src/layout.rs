//! Where a crate sits in the architecture, derived from its folder.
//!
//! The folder is the source of truth (technical-structure.md §2), not the crate name,
//! so a wrongly named crate cannot slip past the rules.

use std::path::Path;

/// The architectural place of one workspace crate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Place {
    /// `foundation/<part>/`
    Foundation { part: String },
    /// `engines/<engine>/<role>/`
    Engine { engine: String, role: Role },
    /// `hosts/<host>/`
    Host,
    /// `xtask/`, `spikes/…`, anything else: not checked.
    Tooling,
}

/// The role of a crate inside an engine (§7).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    Contract,
    Core,
    Adapter,
    Facade,
    Cli,
}

/// Classify a crate from its manifest directory.
pub fn place_of(manifest_dir: &Path, root: &Path) -> Place {
    let rel = manifest_dir.strip_prefix(root).unwrap_or(manifest_dir);
    let parts: Vec<String> = rel
        .components()
        .map(|c| c.as_os_str().to_string_lossy().to_lowercase())
        .collect();
    match parts.as_slice() {
        [top, part, ..] if top == "foundation" => Place::Foundation { part: part.clone() },
        [top, engine, rest @ ..] if top == "engines" => {
            let role = match rest.first().map(String::as_str) {
                Some("contract") => Role::Contract,
                Some("core") => Role::Core,
                Some("adapters") => Role::Adapter,
                Some("cli") => Role::Cli,
                _ => Role::Facade,
            };
            Place::Engine {
                engine: engine.clone(),
                role,
            }
        }
        [top, ..] if top == "hosts" => Place::Host,
        _ => Place::Tooling,
    }
}

/// Engines each engine may use, through contracts only (Draft 2.2 §6.3).
/// `None` means the folder is not a known engine.
pub fn allowed_engines(engine: &str) -> Option<&'static [&'static str]> {
    const ALL: &[&str] = &[
        "inference",
        "document",
        "knowledge",
        "workspace",
        "understanding",
        "realization",
        "feasibility",
        "execution",
        "validation",
        "provenance-memory",
        "publishing",
    ];
    Some(match engine {
        "inference" | "document" | "workspace" | "provenance-memory" => &[],
        "knowledge" => &["inference", "document", "workspace"],
        "execution" => &["workspace"],
        "feasibility" => &["execution"],
        "validation" => &["execution", "inference"],
        "understanding" => &["document", "knowledge", "inference"],
        "realization" => &["understanding", "feasibility", "knowledge", "inference"],
        "publishing" => &["provenance-memory", "validation", "workspace", "document"],
        "orchestration" => ALL,
        _ => return None,
    })
}

/// Foundation parts a contract or a pure core may use. The rest of foundation does I/O or holds state.
pub const PURE_FOUNDATION: &[&str] = &["types", "policy"];

/// Foundation parts that hold OS adapters (`ep-os-windows`, later `ep-os-macos`, `ep-os-linux`).
pub fn is_os_adapter_part(part: &str) -> bool {
    part.starts_with("os-")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn places_from_folders() {
        let root = Path::new("/repo");
        let p = |s: &str| place_of(&root.join(s), root);
        assert_eq!(
            p("foundation/types"),
            Place::Foundation {
                part: "types".into()
            }
        );
        assert_eq!(
            p("engines/document/adapters/pdfium"),
            Place::Engine {
                engine: "document".into(),
                role: Role::Adapter
            }
        );
        assert_eq!(
            p("engines/document/facade"),
            Place::Engine {
                engine: "document".into(),
                role: Role::Facade
            }
        );
        assert_eq!(p("hosts/reader/src-tauri"), Place::Host);
        assert_eq!(p("xtask"), Place::Tooling);
    }
}
