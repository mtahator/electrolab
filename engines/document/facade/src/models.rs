//! Page models the engine can use (ADR-007): pinned files, downloaded on first use by the host
//! with hash verification, kept in the app's `models` folder as `<id>/<file>`.

use std::path::{Path, PathBuf};

use ep_document_contract::ModelInfo;

/// One pinned model file.
#[derive(Clone, Copy, Debug)]
pub struct ModelSpec {
    /// Id (also the folder name).
    pub id: &'static str,
    /// What it does.
    pub purpose: &'static str,
    /// File name inside the folder.
    pub file: &'static str,
    /// Download address (a fixed revision).
    pub url: &'static str,
    /// SHA-256 of the file.
    pub sha256: &'static str,
    /// Size in bytes.
    pub size: u64,
    /// Licence.
    pub licence: &'static str,
}

/// The layout model: Docling Egret-medium (ONNX export pinned by commit; spike S5).
pub const LAYOUT: ModelSpec = ModelSpec {
    id: "egret-medium",
    purpose: "Finds figures, tables, equations and headings on a page",
    file: "model.onnx",
    url: "https://huggingface.co/ningpp/docling-layout-egret-medium-ONNX/resolve/06b27d85c5a004eb024ca82f78e094fb7bf47e47/model.onnx",
    sha256: "e64ebaa061f6f8b4c049fa109590876532a67b77a4f6c00508a7def24844757b",
    size: 78_481_216,
    licence: "Apache-2.0",
};

/// Every model the engine knows.
pub const ALL: &[ModelSpec] = &[LAYOUT];

impl ModelSpec {
    /// Where the file lives under `dir`.
    #[must_use]
    pub fn path(&self, dir: &Path) -> PathBuf {
        dir.join(self.id).join(self.file)
    }

    /// Whether the file is present with the right size (the hash is checked when downloaded).
    #[must_use]
    pub fn present(&self, dir: Option<&Path>) -> bool {
        dir.and_then(|d| std::fs::metadata(self.path(d)).ok())
            .is_some_and(|m| m.len() == self.size)
    }

    /// For `document.models`.
    #[must_use]
    pub fn info(&self, dir: Option<&Path>) -> ModelInfo {
        ModelInfo {
            id: self.id.into(),
            purpose: self.purpose.into(),
            size: self.size,
            present: self.present(dir),
            licence: self.licence.into(),
        }
    }
}

/// Where ONNX Runtime is: `EP_ONNXRUNTIME`, next to the executable, or the repository's
/// `.natives/onnxruntime/*/lib` above it (development builds).
#[must_use]
pub fn find_onnxruntime() -> Option<PathBuf> {
    let names = [
        "onnxruntime.dll",
        "libonnxruntime.so",
        "libonnxruntime.dylib",
    ];
    if let Some(p) = std::env::var_os("EP_ONNXRUNTIME").map(PathBuf::from) {
        return p.exists().then_some(p);
    }
    let exe_dir = std::env::current_exe().ok()?.parent()?.to_path_buf();
    if let Some(p) = names.iter().map(|n| exe_dir.join(n)).find(|p| p.exists()) {
        return Some(p);
    }
    for a in exe_dir.ancestors() {
        let root = a.join(".natives").join("onnxruntime");
        let Ok(entries) = std::fs::read_dir(&root) else {
            continue;
        };
        for e in entries.flatten() {
            if let Some(p) = names
                .iter()
                .map(|n| e.path().join("lib").join(n))
                .find(|p| p.exists())
            {
                return Some(p);
            }
        }
    }
    None
}
