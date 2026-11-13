//! Envelopes, command specifications and errors: the data that crosses every transport.

use std::collections::BTreeSet;

use ep_policy::{DataClass, Destination, Operation};
use ep_types::Actor;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// One command call, as sent by any host (UI, CLI, MCP, IPC).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Envelope {
    /// Correlation id; also used to cancel the call.
    pub id: Uuid,
    /// Command name, `engine.verb`.
    pub command: String,
    /// Who asks.
    pub actor: Actor,
    /// Input, as JSON matching the command's input schema.
    #[serde(default)]
    pub input: serde_json::Value,
    /// Give up after this many milliseconds.
    #[serde(default)]
    pub deadline_ms: Option<u64>,
    /// Policy rules the user approved for this call (set only by hosts after asking).
    #[serde(default)]
    pub approved: BTreeSet<String>,
}

impl Envelope {
    /// A user call with a fresh id.
    #[must_use]
    pub fn new(command: impl Into<String>, input: serde_json::Value) -> Self {
        Self {
            id: Uuid::now_v7(),
            command: command.into(),
            actor: Actor::User,
            input,
            deadline_ms: None,
            approved: BTreeSet::new(),
        }
    }
}

/// How settled a command's contract is (Draft 2.2 §8.3).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Stability {
    /// May change without notice.
    Experimental,
    /// Changes only with a version bump and a migration note.
    Stable,
}

/// What the policy needs to know about a command, declared once at registration.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Permission {
    /// Permission level.
    pub operation: Operation,
    /// Data classes the command may touch.
    pub data: BTreeSet<DataClass>,
    /// Where data goes.
    pub destination: Destination,
}

impl Permission {
    /// Reads local data only.
    #[must_use]
    pub fn read() -> Self {
        Self {
            operation: Operation::Read,
            data: BTreeSet::new(),
            destination: Destination::Device,
        }
    }

    /// Creates local data.
    #[must_use]
    pub fn create() -> Self {
        Self {
            operation: Operation::Create,
            ..Self::read()
        }
    }

    /// Deletes or overwrites.
    #[must_use]
    pub fn destructive() -> Self {
        Self {
            operation: Operation::Destructive,
            ..Self::read()
        }
    }
}

/// Public description of a command (listed by `ep commands`, MCP, generated bindings).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct CommandInfo {
    /// `engine.verb`.
    pub name: String,
    /// One line for humans and agents.
    pub summary: String,
    /// Contract stability.
    pub stability: Stability,
    /// Policy declaration.
    pub permission: Permission,
    /// Whether it streams progress items.
    pub streaming: bool,
    /// JSON Schema of the input.
    pub input_schema: serde_json::Value,
    /// JSON Schema of the output.
    pub output_schema: serde_json::Value,
}

/// Why a call failed. Serialisable so every transport reports errors the same way.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema, thiserror::Error)]
#[serde(tag = "code", rename_all = "snake_case")]
#[non_exhaustive]
pub enum CommandError {
    /// No command with this name.
    #[error("unknown command `{command}`")]
    UnknownCommand {
        /// Name asked for.
        command: String,
    },
    /// Input does not match the schema.
    #[error("invalid input: {message}")]
    InvalidInput {
        /// Details.
        message: String,
    },
    /// The policy forbids it.
    #[error("denied by policy rule {rule}: {reason}")]
    Denied {
        /// Rule cited.
        rule: String,
        /// Why.
        reason: String,
    },
    /// The user must approve first; resend with `rule` in `approved`.
    #[error("needs approval ({rule}): {reason}")]
    NeedsApproval {
        /// Rule cited.
        rule: String,
        /// What to tell the user.
        reason: String,
    },
    /// Cancelled by the caller.
    #[error("cancelled")]
    Cancelled,
    /// The deadline passed.
    #[error("deadline of {deadline_ms} ms exceeded")]
    DeadlineExceeded {
        /// Deadline.
        deadline_ms: u64,
    },
    /// The command could not do its work.
    #[error("{kind}: {message}")]
    Failed {
        /// Short machine-readable kind, e.g. `not_found`, `io`, `corrupt`.
        kind: String,
        /// Human-readable message.
        message: String,
    },
    /// The transport broke (IPC peer gone, frame too large).
    #[error("transport: {message}")]
    Transport {
        /// Details.
        message: String,
    },
}

impl CommandError {
    /// A handler failure.
    #[must_use]
    pub fn failed(kind: &str, message: impl std::fmt::Display) -> Self {
        Self::Failed {
            kind: kind.into(),
            message: message.to_string(),
        }
    }
}
