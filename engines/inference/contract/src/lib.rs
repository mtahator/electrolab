//! Contract of the Inference engine — one interface to the language models.
//!
//! See `engines/inference/CONTRACT.md` (Draft 2.2 §14.1, ADR-005).
#![deny(missing_docs)]

use ep_types::BlobRef;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Contract version; experimental until a second host uses it (Draft 2.2 §8.3).
pub const VERSION: &str = "0.5.0";

/// Command names.
pub mod commands {
    /// Generate an answer (streamed when the request asks for it).
    pub const COMPLETE: &str = "inference.complete";
    /// Models the engine knows and the runtime that will run them.
    pub const MODELS: &str = "inference.models";
    /// Search the model folders again.
    pub const SEARCH_MODELS: &str = "inference.search_models";
    /// Add a folder with models.
    pub const ADD_FOLDER: &str = "inference.add_folder";
    /// Forget a folder.
    pub const REMOVE_FOLDER: &str = "inference.remove_folder";
    /// Choose the order of the models, and whether one is kept ready.
    pub const CHOOSE: &str = "inference.choose";
    /// Read pictures with a model on this computer.
    pub const VISION_READ: &str = "inference.vision_read";
    /// State of the runtime.
    pub const STATUS: &str = "inference.status";
    /// Start the runtime with a model.
    pub const LOAD: &str = "inference.load";
    /// Make a model ready, when the user wants one kept ready.
    pub const WARM: &str = "inference.warm";
    /// Stop the runtime.
    pub const UNLOAD: &str = "inference.unload";
}

/// Event names.
pub mod events {
    /// The next piece of a streamed answer: `{ id, text }`.
    pub const TOKEN: &str = "inference.token";
    /// A model is being loaded: `{ model }`.
    pub const MODEL_LOADING: &str = "model.loading";
    /// A model was loaded: `{ model, backend, ms }`.
    pub const MODEL_LOADED: &str = "model.loaded";
    /// Another model than the first choice is used: `{ wanted, used, reason }`.
    pub const MODEL_FALLBACK: &str = "model.fallback";
    /// The runtime stopped: `{ model }`.
    pub const MODEL_UNLOADED: &str = "model.unloaded";
    /// Folders, choices or models changed: `{}`.
    pub const MODELS_CHANGED: &str = "inference.models_changed";
    /// A picture was read: `{ id, done, total, text }`.
    pub const VISION: &str = "inference.vision";
    /// Where a request went and why: `{ task, backend, model, reason }`.
    pub const ROUTE_DECIDED: &str = "route.decided";
}

/// Error kinds (`CommandError::Failed { kind }`).
pub mod errors {
    /// A folder was expected.
    pub const NOT_A_FOLDER: &str = "not_a_folder";
    /// A file could not be read or written.
    pub const IO: &str = "io";
    /// No model file was found.
    pub const NO_MODEL: &str = "no_model";
    /// No llama.cpp build was found.
    pub const NO_RUNTIME: &str = "no_runtime";
    /// The runtime did not start.
    pub const START_FAILED: &str = "start_failed";
    /// The runtime stopped answering.
    pub const RUNTIME_FAILED: &str = "runtime_failed";
    /// The answer did not meet the schema after one retry.
    pub const BAD_OUTPUT: &str = "bad_output";
    /// Pictures are only looked at with consent.
    pub const CONSENT_NEEDED: &str = "consent_needed";
    /// No model that looks at pictures was found.
    pub const NO_VISION_MODEL: &str = "no_vision_model";
    /// Local-only data and no local backend.
    pub const REMOTE_NOT_ALLOWED: &str = "remote_not_allowed";
}

/// Who says a message.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
#[schemars(rename = "LlmRole")]
pub enum Role {
    /// Instructions.
    System,
    /// The question.
    User,
    /// The model.
    Assistant,
}

/// One message of a conversation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[schemars(rename = "LlmMessage")]
pub struct Message {
    /// Who says it.
    pub role: Role,
    /// What is said.
    pub content: String,
}

impl Message {
    /// A system message.
    pub fn system(s: impl Into<String>) -> Self {
        Self {
            role: Role::System,
            content: s.into(),
        }
    }
    /// A user message.
    pub fn user(s: impl Into<String>) -> Self {
        Self {
            role: Role::User,
            content: s.into(),
        }
    }
}

/// Where the data of a request may go.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
#[schemars(rename = "LlmSensitivity")]
pub enum Sensitivity {
    /// Only to a model on this machine.
    #[default]
    LocalOnly,
    /// Anywhere the policy allows.
    Any,
}

/// Generation limits.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, JsonSchema)]
#[schemars(rename = "LlmLimits")]
pub struct Limits {
    /// Most tokens to generate.
    pub max_tokens: u32,
    /// Sampling temperature (0 = always the most likely token).
    #[serde(default)]
    pub temperature: f32,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_tokens: 512,
            temperature: 0.0,
        }
    }
}

/// Ask for the answer to be streamed as `inference.token` events.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[schemars(rename = "LlmStream")]
pub struct Stream {
    /// Id carried by every event of this answer.
    pub id: String,
}

/// A request to a language model.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct LlmRequest {
    /// The job (`explain.draft`, …); routing and model profiles key on it.
    pub task: String,
    /// The conversation: instructions and question.
    pub messages: Vec<Message>,
    /// Source text (paper passages), sent in its own labelled data section, never as instructions.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<String>,
    /// JSON Schema the answer must follow.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub schema: Option<serde_json::Value>,
    /// Start of the answer; the model continues it, and the answer includes it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prefill: Option<String>,
    /// Where the data may go.
    #[serde(default)]
    pub sensitivity: Sensitivity,
    /// Generation limits.
    #[serde(default)]
    pub limits: Limits,
    /// Nobody is waiting: what a person asked goes first.
    #[serde(default)]
    pub background: bool,
    /// Stream the answer as events.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stream: Option<Stream>,
}

impl LlmRequest {
    /// A request for `task` with these messages and default limits.
    #[must_use]
    pub fn new(task: impl Into<String>, messages: Vec<Message>) -> Self {
        Self {
            task: task.into(),
            messages,
            data: None,
            schema: None,
            prefill: None,
            sensitivity: Sensitivity::LocalOnly,
            limits: Limits::default(),
            background: false,
            stream: None,
        }
    }
}

/// Token counts and times of one answer.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[schemars(rename = "LlmUsage")]
pub struct Usage {
    /// Tokens read.
    pub prompt_tokens: u32,
    /// Tokens written.
    pub completion_tokens: u32,
    /// Time to the first token.
    pub first_token_ms: u64,
    /// Time to the end.
    pub total_ms: u64,
}

/// An answer.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct LlmResponse {
    /// The text (with the prefill); JSON text when a schema was given.
    pub content: String,
    /// Model that answered.
    pub model: String,
    /// Why this backend and model were chosen.
    pub route_reason: String,
    /// Counts and times.
    pub usage: Usage,
}

/// Whether a model can be used.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
#[schemars(rename = "LlmModelState")]
pub enum ModelState {
    /// Its file is here.
    Ready,
    /// Running now.
    Loaded,
}

/// What a model file can be used for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
#[schemars(rename = "LlmModelKind")]
pub enum ModelKind {
    /// Writes answers.
    Chat,
    /// Makes vectors for search.
    Embedding,
    /// The picture part of another model.
    VisionPart,
    /// Something else.
    Other,
}

/// A local model that was found.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[schemars(rename = "LlmModel")]
pub struct ModelInfo {
    /// Stable id.
    pub id: String,
    /// Name for people.
    pub name: String,
    /// File name.
    pub file: String,
    /// Full path.
    pub path: String,
    /// Bytes.
    pub size: u64,
    /// Licence of the weights, when the file says so.
    pub licence: String,
    /// What it can be used for.
    pub kind: ModelKind,
    /// Architecture named in the file.
    pub architecture: String,
    /// Quantisation (`Q4_K_M`), when known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quantisation: Option<String>,
    /// Whether it is running.
    pub state: ModelState,
    /// The model suggested for this machine.
    pub recommended: bool,
    /// Context length used.
    pub context: u32,
    /// Memory it needs on the graphics card, in bytes (about).
    pub needs: u64,
    /// Why it could not be started the last time.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub problem: Option<String>,
}

/// A folder that is searched for models.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[schemars(rename = "LlmFolder")]
pub struct Folder {
    /// Where.
    pub path: String,
    /// Models found in it (and not already in an earlier folder).
    pub models: u32,
    /// Model files in formats that cannot be used.
    pub other_formats: u32,
    /// It is not there.
    pub missing: bool,
    /// The search stopped at its limit.
    pub partial: bool,
    /// Added by the user (can be removed).
    pub own: bool,
}

/// Memory of the graphics card.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[schemars(rename = "LlmMemory")]
pub struct Memory {
    /// Name of the card.
    pub device: String,
    /// Bytes.
    pub total: u64,
    /// Bytes that were free when a model was last loaded.
    pub free: u64,
}

/// Why another model than the first choice is used.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
#[schemars(rename = "LlmFallback")]
pub enum Fallback {
    /// It does not fit in the memory that is free.
    NotEnoughMemory,
    /// It did not start.
    StartFailed,
    /// It answered too slowly.
    TooSlow,
}

/// Input of `inference.add_folder` and `inference.remove_folder`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[schemars(rename = "LlmFolderPath")]
pub struct FolderPath {
    /// Where.
    pub path: String,
}

/// Input of `inference.choose`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[schemars(rename = "LlmChoose")]
pub struct Choose {
    /// Model ids, first choice first; empty: automatic again; missing: as it is.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub order: Option<Vec<String>>,
    /// Load a model when the app starts and keep it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub keep_ready: Option<bool>,
}

/// The llama.cpp build in use.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
#[schemars(rename = "LlmBackend")]
pub enum Backend {
    /// NVIDIA.
    Cuda,
    /// Any GPU with Vulkan.
    Vulkan,
    /// The processor.
    Cpu,
    /// No build found.
    None,
}

/// The runtime that will run the models.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[schemars(rename = "LlmRuntime")]
pub struct Runtime {
    /// Which build.
    pub backend: Backend,
    /// Its folder, when found.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub build: Option<String>,
}

/// Models and runtime.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[schemars(rename = "LlmModels")]
pub struct Models {
    /// The models that were found.
    pub models: Vec<ModelInfo>,
    /// The folders searched.
    pub folders: Vec<Folder>,
    /// Models in the order they are tried.
    pub order: Vec<String>,
    /// The user chose that order (otherwise it is automatic).
    pub own_order: bool,
    /// A model is loaded when the app starts and stays loaded.
    pub keep_ready: bool,
    /// Memory of the graphics card, when it was measured.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub memory: Option<Memory>,
    /// The runtime.
    pub runtime: Runtime,
    /// The model that looks at pictures, when there is one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vision: Option<VisionModel>,
}

/// The model that looks at pictures.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[schemars(rename = "LlmVisionModel")]
pub struct VisionModel {
    /// Stable id.
    pub id: String,
    /// Name for people.
    pub name: String,
}

/// A picture and what is asked about it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[schemars(rename = "LlmPicture")]
pub struct Picture {
    /// The picture (PNG or JPEG).
    pub image: BlobRef,
    /// What is asked.
    pub question: String,
    /// Words of the paper about it (its caption), sent as data.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<String>,
}

/// Input of `inference.vision_read`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct VisionRead {
    /// The pictures, in the order they are read.
    pub pictures: Vec<Picture>,
    /// The user agreed to these pictures being read.
    pub consent: bool,
    /// Nobody is waiting: what a person asked goes first.
    #[serde(default)]
    pub background: bool,
    /// Stream the texts, and say what is done, with this id.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stream: Option<Stream>,
}

/// What was read in the pictures.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct VisionAnswer {
    /// One text per picture.
    pub texts: Vec<String>,
    /// Model that read them.
    pub model: String,
    /// Time for all of them.
    pub total_ms: u64,
}

/// Runtime state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
#[schemars(rename = "LlmRunState")]
pub enum RunState {
    /// Not running.
    Stopped,
    /// Loading a model.
    Starting,
    /// Answering.
    Ready,
    /// Could not start or stopped answering.
    Failed,
}

/// State of the runtime.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[schemars(rename = "LlmStatus")]
pub struct Status {
    /// State.
    pub state: RunState,
    /// Model loaded or loading.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// Build in use.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub backend: Option<Backend>,
    /// Why it failed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// The first choice, when another model is used.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wanted: Option<String>,
    /// Why the first choice is not used.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fallback: Option<Fallback>,
    /// How long the last start took.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ready_ms: Option<u64>,
}

/// Input of `inference.load`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[schemars(rename = "LlmLoad")]
pub struct LoadRequest {
    /// Model id; the recommended one when missing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_defaults_are_local_only() {
        let r: LlmRequest = serde_json::from_value(serde_json::json!({
            "task": "t", "messages": [{ "role": "user", "content": "hi" }]
        }))
        .unwrap();
        assert_eq!(r.sensitivity, Sensitivity::LocalOnly);
        assert_eq!(r.limits.max_tokens, 512);
    }
}
