//! Id newtypes — mixing ids of different kinds is a compile error — and versioned references.

use std::fmt;
use std::marker::PhantomData;
use std::str::FromStr;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::Hash;

/// A malformed id string.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IdParseError(pub String);

impl fmt::Display for IdParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "not a valid id: {}", self.0)
    }
}

impl std::error::Error for IdParseError {}

macro_rules! id_type {
    ($(#[$doc:meta])* $name:ident) => {
        $(#[$doc])*
        ///
        /// Serialised as the UUID's canonical hyphenated string.
        #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema)]
        #[serde(transparent)]
        pub struct $name(Uuid);

        impl $name {
            /// Wrap an existing UUID (ids are created through `Cx`'s id source, not here,
            /// so that tests are deterministic).
            #[must_use]
            pub const fn from_uuid(u: Uuid) -> Self {
                Self(u)
            }

            /// The underlying UUID.
            #[must_use]
            pub const fn uuid(&self) -> Uuid {
                self.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                self.0.fmt(f)
            }
        }

        impl FromStr for $name {
            type Err = IdParseError;
            fn from_str(s: &str) -> Result<Self, Self::Err> {
                Uuid::parse_str(s).map(Self).map_err(|_| IdParseError(s.to_string()))
            }
        }
    };
}

id_type!(
    /// A research object of any kind.
    ObjectId
);
id_type!(
    /// A paper (a PDF in the library or a project).
    PaperId
);
id_type!(
    /// A library or project.
    ProjectId
);
id_type!(
    /// A highlight, note, drawing or other annotation.
    AnnotationId
);
id_type!(
    /// A collection of papers.
    CollectionId
);
id_type!(
    /// A PDF chat.
    ChatId
);
id_type!(
    /// A background job (preparation, OCR, indexing).
    JobId
);

/// A reference to one version of a research object of kind `T`, pinned by content hash.
#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(bound = "")]
#[schemars(bound = "")]
pub struct Ref<T> {
    /// The object.
    pub id: ObjectId,
    /// Its version (1, 2, …).
    pub version: u32,
    /// Hash of that version's content.
    pub hash: Hash,
    #[serde(skip)]
    #[schemars(skip)]
    _kind: PhantomData<fn() -> T>,
}

impl<T> Ref<T> {
    /// Make a reference.
    #[must_use]
    pub const fn new(id: ObjectId, version: u32, hash: Hash) -> Self {
        Self { id, version, hash, _kind: PhantomData }
    }
}

// Manual impls: derives would wrongly require `T: Clone/PartialEq/Debug`.
impl<T> fmt::Debug for Ref<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Ref").field("id", &self.id).field("version", &self.version).field("hash", &self.hash).finish()
    }
}
impl<T> Clone for Ref<T> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<T> Copy for Ref<T> {}
impl<T> PartialEq for Ref<T> {
    fn eq(&self, o: &Self) -> bool {
        self.id == o.id && self.version == o.version && self.hash == o.hash
    }
}
impl<T> Eq for Ref<T> {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_round_trip_and_differ_by_type() {
        let u = Uuid::from_u128(42);
        let p = PaperId::from_uuid(u);
        let s = p.to_string();
        assert_eq!(s.parse::<PaperId>(), Ok(p));
        assert_eq!(serde_json::to_string(&p).expect("json"), format!("\"{s}\""));
        assert!("nope".parse::<PaperId>().is_err());
    }

    #[test]
    fn ref_serialises_without_kind() {
        struct Paper;
        let r: Ref<Paper> = Ref::new(ObjectId::from_uuid(Uuid::from_u128(1)), 3, Hash::of(b"x"));
        let j = serde_json::to_value(r).expect("json");
        assert_eq!(j["version"], 3);
        assert!(j.get("_kind").is_none());
        let back: Ref<Paper> = serde_json::from_value(j).expect("back");
        assert_eq!(back, r);
    }
}
