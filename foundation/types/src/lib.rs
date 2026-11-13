//! Shared types used by every engine (Draft 2.2 §13.1, technical-structure.md §7.4).
//!
//! Small, stable, dependency-light: ids, content hashes and references, page geometry,
//! evidence, actors. Everything is serialisable and has a JSON Schema.
#![deny(missing_docs)]

mod evidence;
mod geometry;
mod hash;
mod ids;

pub use evidence::{Confidence, Importance, Quantity, Span, Status, Unit};
pub use geometry::{Location, PagePoint, PageQuad, PageRect, ZoomSpec};
pub use hash::{BlobRef, Hash, HashParseError, MediaType};
pub use ids::{AnnotationId, ChatId, CollectionId, IdParseError, JobId, ObjectId, PaperId, ProjectId, Ref};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Who caused an action — recorded on every command envelope and provenance event.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[non_exhaustive]
pub enum Actor {
    /// The person using the app.
    User,
    /// An agent or model acting for the user, by name.
    Agent {
        /// Agent or tool name.
        name: String,
    },
    /// The app itself (background preparation, migrations).
    System,
}
