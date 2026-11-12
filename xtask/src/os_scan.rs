//! Rule 1 of Draft 2.2 §6.4: OS-specific code lives only in adapters.
//!
//! Scans Rust sources under `foundation/` and `engines/` and reports OS-specific
//! constructs outside adapter crates (`engines/*/adapters/**`, `foundation/os-*`).

use std::fs;
use std::path::{Path, PathBuf};

/// Text patterns that mark OS-specific code.
const PATTERNS: &[&str] = &[
    "cfg(target_os",
    "cfg(windows",
    "cfg(unix",
    "cfg(target_family",
    "cfg(not(windows",
    "cfg(not(unix",
    "std::os::windows",
    "std::os::unix",
    "windows_sys::",
    "windows::Win32",
];

/// Return one violation per offending line.
pub fn scan(root: &Path) -> anyhow::Result<Vec<String>> {
    let mut out = Vec::new();
    for top in ["foundation", "engines"] {
        let dir = root.join(top);
        if dir.is_dir() {
            walk(&dir, &mut |file| {
                if is_rust(file) && !is_adapter_path(file, root) {
                    out.extend(scan_text(&display(file, root), &fs::read_to_string(file)?));
                }
                Ok(())
            })?;
        }
    }
    Ok(out)
}

/// Find OS-specific constructs in one file's text. Comment lines are ignored.
pub fn scan_text(name: &str, text: &str) -> Vec<String> {
    text.lines()
        .enumerate()
        .filter(|(_, line)| !line.trim_start().starts_with("//"))
        .filter_map(|(i, line)| {
            let compact: String = line.chars().filter(|c| !c.is_whitespace()).collect();
            PATTERNS
                .iter()
                .find(|p| compact.contains(&p.replace(' ', "")))
                .map(|p| {
                    format!(
                        "{name}:{}: OS-specific `{p}` outside an adapter (§6.4 rule 1)",
                        i + 1
                    )
                })
        })
        .collect()
}

/// Adapter locations where OS-specific code is allowed.
fn is_adapter_path(file: &Path, root: &Path) -> bool {
    let rel = file.strip_prefix(root).unwrap_or(file);
    let parts: Vec<String> = rel
        .components()
        .map(|c| c.as_os_str().to_string_lossy().to_lowercase())
        .collect();
    match parts.as_slice() {
        [top, part, ..] if top == "foundation" => part.starts_with("os-"),
        [top, _, role, ..] if top == "engines" => role == "adapters",
        _ => false,
    }
}

fn is_rust(file: &Path) -> bool {
    file.extension().is_some_and(|e| e == "rs")
}

fn display(file: &Path, root: &Path) -> String {
    file.strip_prefix(root)
        .unwrap_or(file)
        .to_string_lossy()
        .replace('\\', "/")
}

fn walk(dir: &Path, f: &mut dyn FnMut(&Path) -> anyhow::Result<()>) -> anyhow::Result<()> {
    let mut entries: Vec<PathBuf> = fs::read_dir(dir)?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .collect();
    entries.sort();
    for path in entries {
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        if path.is_dir() {
            if name != "target" && !name.starts_with('.') {
                walk(&path, f)?;
            }
        } else {
            f(&path)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_cfg_and_std_os() {
        let text =
            "fn a() {}\n#[cfg(target_os = \"windows\")]\nuse std::os::windows::ffi::OsStrExt;\n";
        assert_eq!(scan_text("x.rs", text).len(), 2);
    }

    #[test]
    fn ignores_comments_and_clean_code() {
        let text = "// #[cfg(windows)] is only allowed in adapters\nfn a() {}\n";
        assert_eq!(scan_text("x.rs", text), Vec::<String>::new());
    }

    #[test]
    fn spacing_does_not_hide_a_cfg() {
        assert_eq!(scan_text("x.rs", "#[cfg( windows )]").len(), 1);
    }

    #[test]
    fn adapter_paths() {
        let root = Path::new("/r");
        assert!(is_adapter_path(
            &root.join("foundation/os-windows/src/lib.rs"),
            root
        ));
        assert!(is_adapter_path(
            &root.join("engines/document/adapters/pdfium/src/lib.rs"),
            root
        ));
        assert!(!is_adapter_path(
            &root.join("engines/document/core/src/lib.rs"),
            root
        ));
        assert!(!is_adapter_path(
            &root.join("foundation/os/src/lib.rs"),
            root
        ));
    }
}
