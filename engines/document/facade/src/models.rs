//! Page models the engine can use (ADR-007, ADR-013): pinned files kept as `<folder>/<file>`.
//! The text recognition models (PP-OCRv5, about 13 MB) ship with the app, so scanned pages work
//! offline from the first start; the layout model can be downloaded later by the host, with hash
//! verification. Hosts give the engine every folder to look in (bundled first).

use std::path::{Path, PathBuf};

use ep_document_contract::ModelInfo;

/// One pinned model file.
#[derive(Clone, Copy, Debug)]
pub struct ModelSpec {
    /// Id.
    pub id: &'static str,
    /// Folder name (several files of one model share it).
    pub folder: &'static str,
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
    /// Shipped with the app (never downloaded at run time).
    pub bundled: bool,
}

/// The layout model: Docling Egret-medium (ONNX export pinned by commit; spike S5).
pub const LAYOUT: ModelSpec = ModelSpec {
    id: "egret-medium",
    folder: "egret-medium",
    purpose: "Finds figures, tables, equations and headings on a page",
    file: "model.onnx",
    url: "https://huggingface.co/ningpp/docling-layout-egret-medium-ONNX/resolve/06b27d85c5a004eb024ca82f78e094fb7bf47e47/model.onnx",
    sha256: "e64ebaa061f6f8b4c049fa109590876532a67b77a4f6c00508a7def24844757b",
    size: 78_481_216,
    licence: "Apache-2.0",
    bundled: false,
};

/// PP-OCRv5 mobile text detector.
pub const OCR_DET: ModelSpec = ModelSpec {
    id: "ppocrv5-det",
    folder: "ppocrv5-det",
    purpose: "Finds lines of text on scanned pages",
    file: "inference.onnx",
    url: "https://huggingface.co/PaddlePaddle/PP-OCRv5_mobile_det_onnx/resolve/e6f4fa85f00e168c862bc462aebca69eef9b3d3d/inference.onnx",
    sha256: "a431985659dc921974177a95adcfbb90fd9e51989a5e04d70d0b75f597b6e61d",
    size: 4_826_518,
    licence: "Apache-2.0",
    bundled: true,
};

/// PP-OCRv5 mobile recogniser for Latin script with math symbols.
pub const OCR_REC: ModelSpec = ModelSpec {
    id: "ppocrv5-rec-en",
    folder: "ppocrv5-rec-en",
    purpose: "Reads the text of each line on scanned pages",
    file: "inference.onnx",
    url: "https://huggingface.co/PaddlePaddle/en_PP-OCRv5_mobile_rec_onnx/resolve/3fafbc3b5dcf93dd72add9f48368be8a3a2cd33b/inference.onnx",
    sha256: "b5f833dfc5d0eb71da397b4efa06ebeee9b431b690a47d6af40d77d8eabc557f",
    size: 7_848_423,
    licence: "Apache-2.0",
    bundled: true,
};

/// The recogniser's character list.
pub const OCR_DICT: ModelSpec = ModelSpec {
    id: "ppocrv5-rec-en-dict",
    folder: "ppocrv5-rec-en",
    purpose: "Characters the recogniser knows",
    file: "inference.yml",
    url: "https://huggingface.co/PaddlePaddle/en_PP-OCRv5_mobile_rec_onnx/resolve/3fafbc3b5dcf93dd72add9f48368be8a3a2cd33b/inference.yml",
    sha256: "27e91d0582f40168aa218303c76e184bc78fa7a5d105aad0cfbad8458b441067",
    size: 3_964,
    licence: "Apache-2.0",
    bundled: true,
};

/// Every model file the engine knows.
pub const ALL: &[ModelSpec] = &[LAYOUT, OCR_DET, OCR_REC, OCR_DICT];

impl ModelSpec {
    /// Where the file lives under `dir`.
    #[must_use]
    pub fn path(&self, dir: &Path) -> PathBuf {
        dir.join(self.folder).join(self.file)
    }

    /// The first of `dirs` holding the file with the right size (the hash is checked when the
    /// file is fetched or bundled).
    #[must_use]
    pub fn find(&self, dirs: &[PathBuf]) -> Option<PathBuf> {
        dirs.iter()
            .map(|d| self.path(d))
            .find(|p| std::fs::metadata(p).is_ok_and(|m| m.len() == self.size))
    }

    /// Whether the file is present in one of `dirs`.
    #[must_use]
    pub fn present(&self, dirs: &[PathBuf]) -> bool {
        self.find(dirs).is_some()
    }

    /// For `document.models`.
    #[must_use]
    pub fn info(&self, dirs: &[PathBuf]) -> ModelInfo {
        ModelInfo {
            id: self.id.into(),
            purpose: self.purpose.into(),
            size: self.size,
            present: self.present(dirs),
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

/// Models shipped with the app: `models/` next to the executable (installed app), or the
/// repository's `.natives/models` above it (development builds).
#[must_use]
pub fn bundled_models_dir() -> Option<PathBuf> {
    let exe_dir = std::env::current_exe().ok()?.parent()?.to_path_buf();
    let next = exe_dir.join("models");
    if next.is_dir() {
        return Some(next);
    }
    exe_dir
        .ancestors()
        .map(|a| a.join(".natives").join("models"))
        .find(|d| d.is_dir())
}
