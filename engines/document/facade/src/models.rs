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

const LATEX_OCR_LICENCE: &str = "MIT (LaTeX-OCR weights), Apache-2.0 (ONNX build)";

/// The formula recogniser's encoder; it stands for the whole model (ADR-015).
pub const FORMULA_ENCODER: ModelSpec = ModelSpec {
    id: "latex-ocr",
    folder: "latex-ocr",
    purpose: "Shows equations as LaTeX",
    file: "encoder.onnx",
    url: const_format_url::ENCODER,
    sha256: "01bf5dc25539ca0cd5b1bd29296ea495977a6ba5f629dc4178277809d26e5e7d",
    size: 89_008_136,
    licence: LATEX_OCR_LICENCE,
    bundled: false,
};

/// The formula recogniser's image resizer.
pub const FORMULA_RESIZER: ModelSpec = ModelSpec {
    id: "latex-ocr-resizer",
    folder: "latex-ocr",
    purpose: "Picks the size a formula image is read at",
    file: "image_resizer.onnx",
    url: const_format_url::RESIZER,
    sha256: "e0b075c39700f64d50400f39c8fc186bbb3b5d84d31864008313f376603aca9d",
    size: 38_967_751,
    licence: LATEX_OCR_LICENCE,
    bundled: false,
};

/// The formula recogniser's decoder.
pub const FORMULA_DECODER: ModelSpec = ModelSpec {
    id: "latex-ocr-decoder",
    folder: "latex-ocr",
    purpose: "Writes the LaTeX of a formula",
    file: "decoder.onnx",
    url: const_format_url::DECODER,
    sha256: "bd695497bf1b22279b7626f5916c79226e1e244c84355f8da7edfd2d921d0072",
    size: 50_952_726,
    licence: LATEX_OCR_LICENCE,
    bundled: false,
};

/// The formula recogniser's tokens.
pub const FORMULA_TOKENIZER: ModelSpec = ModelSpec {
    id: "latex-ocr-tokenizer",
    folder: "latex-ocr",
    purpose: "LaTeX pieces the decoder writes",
    file: "tokenizer.json",
    url: const_format_url::TOKENIZER,
    sha256: "1dc27b18d6a518d0d5ff3f4bb7bd98521fe80ad39e5b2a246d4109f1bb9d5019",
    size: 24_174,
    licence: LATEX_OCR_LICENCE,
    bundled: false,
};

/// Download addresses of the formula files (one pinned release).
mod const_format_url {
    // One pinned GitHub release of RapidLaTeXOCR; every file is checked against its SHA-256.
    pub const ENCODER: &str =
        "https://github.com/RapidAI/RapidLaTeXOCR/releases/download/v0.0.0/encoder.onnx";
    pub const RESIZER: &str =
        "https://github.com/RapidAI/RapidLaTeXOCR/releases/download/v0.0.0/image_resizer.onnx";
    pub const DECODER: &str =
        "https://github.com/RapidAI/RapidLaTeXOCR/releases/download/v0.0.0/decoder.onnx";
    pub const TOKENIZER: &str =
        "https://github.com/RapidAI/RapidLaTeXOCR/releases/download/v0.0.0/tokenizer.json";
}

/// Every model file the engine knows.
pub const ALL: &[ModelSpec] = &[
    LAYOUT,
    OCR_DET,
    OCR_REC,
    OCR_DICT,
    FORMULA_ENCODER,
    FORMULA_RESIZER,
    FORMULA_DECODER,
    FORMULA_TOKENIZER,
];

/// The files of the model `id` belongs to (all files that share its folder).
#[must_use]
pub fn group_of(id: &str) -> Vec<ModelSpec> {
    let Some(folder) = ALL.iter().find(|m| m.id == id).map(|m| m.folder) else {
        return Vec::new();
    };
    ALL.iter().filter(|m| m.folder == folder).copied().collect()
}

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
            bundled: self.bundled,
            group: self.folder.into(),
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
