//! Content hashes (BLAKE3) and references to stored files.

use std::fmt;
use std::str::FromStr;

use schemars::JsonSchema;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// A 32-byte BLAKE3 content hash, written as 64 lowercase hex characters.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Hash([u8; 32]);

/// A malformed hash string.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HashParseError(pub String);

impl fmt::Display for HashParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "not a 64-character hex hash: {}", self.0)
    }
}

impl std::error::Error for HashParseError {}

impl Hash {
    /// Wrap raw hash bytes.
    #[must_use]
    pub const fn from_bytes(b: [u8; 32]) -> Self {
        Self(b)
    }

    /// The raw bytes.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    /// Hash a byte slice (convenience for small data and tests; large files are hashed
    /// incrementally by the blob store).
    #[must_use]
    pub fn of(data: &[u8]) -> Self {
        Self(*blake3::hash(data).as_bytes())
    }

    /// Lowercase hex form.
    #[must_use]
    pub fn to_hex(&self) -> String {
        hex::encode(self.0)
    }
}

impl fmt::Display for Hash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_hex())
    }
}

impl fmt::Debug for Hash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Hash({})", &self.to_hex()[..12])
    }
}

impl FromStr for Hash {
    type Err = HashParseError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut b = [0u8; 32];
        if s.len() != 64 || hex::decode_to_slice(s, &mut b).is_err() || s.chars().any(|c| c.is_ascii_uppercase()) {
            return Err(HashParseError(s.to_string()));
        }
        Ok(Self(b))
    }
}

impl Serialize for Hash {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.to_hex())
    }
}

impl<'de> Deserialize<'de> for Hash {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        s.parse().map_err(serde::de::Error::custom)
    }
}

impl JsonSchema for Hash {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "Hash".into()
    }
    fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({ "type": "string", "pattern": "^[0-9a-f]{64}$", "description": "BLAKE3 content hash (hex)" })
    }
}

/// A media type such as `application/pdf`.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(transparent)]
pub struct MediaType(pub String);

impl MediaType {
    /// `application/pdf`.
    #[must_use]
    pub fn pdf() -> Self {
        Self("application/pdf".into())
    }
}

/// A file in the content-addressed store: messages carry these, never large bytes (§9.4).
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
pub struct BlobRef {
    /// Content hash.
    pub hash: Hash,
    /// Size in bytes.
    pub size: u64,
    /// What the bytes are.
    pub media_type: MediaType,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_round_trip_and_strictness() {
        let h = Hash::of(b"electropaper");
        assert_eq!(h.to_hex().parse::<Hash>(), Ok(h));
        assert!(h.to_hex().to_uppercase().parse::<Hash>().is_err());
        assert!("abc".parse::<Hash>().is_err());
        let j = serde_json::to_string(&h).expect("json");
        assert_eq!(serde_json::from_str::<Hash>(&j).expect("back"), h);
    }

    #[test]
    fn known_blake3_value() {
        // BLAKE3 of the empty input (reference value from the BLAKE3 specification)
        assert_eq!(Hash::of(b"").to_hex(), "af1349b9f5f9a1a6a0404dea36dcc9499bcb25c9adc112b7cc9a93cae41f3262");
    }
}
