//! Digests kept with the papers (made by the Knowledge engine; this engine only keeps them).

use ep_types::PaperId;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Largest digest kept (bytes of JSON).
pub const MAX_DIGEST: usize = 4 << 20;

/// A digest of a paper. Output of `workspace.get_digests`, input of `workspace.save_digest`
/// (and, for what was seen in its figures, of `get_figures` and `save_figures`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct PaperDigest {
    /// The paper.
    pub paper: PaperId,
    /// The digest, as the engine that made it wrote it.
    pub data: serde_json::Value,
}

/// Input of `workspace.get_digests` and `workspace.get_figures`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct GetDigests {
    /// Which papers; all of the library when missing.
    #[serde(default)]
    pub papers: Option<Vec<PaperId>>,
}
