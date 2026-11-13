//! Evidence: where a statement comes from, and how sure we are (Draft 2.2 §13.1, §15.1).

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{PageRect, PaperId};

/// An exact passage in a paper: the unit of evidence.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Span {
    /// The paper.
    pub source: PaperId,
    /// 1-based page.
    pub page: u32,
    /// Where on the page, when known.
    #[serde(default)]
    pub bbox: Option<PageRect>,
    /// The exact words.
    pub quote: String,
}

/// Whether something is in the paper.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[non_exhaustive]
pub enum Status {
    /// Written in the paper (quote verified verbatim).
    Stated {
        /// The passage.
        span: Span,
    },
    /// Not written, but concluded — with the reason and what it rests on.
    Inferred {
        /// Why.
        reason: String,
        /// Passages it rests on.
        from: Vec<Span>,
    },
    /// Not in the paper.
    Missing,
}

/// How much something matters for a result.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Importance {
    /// Changes the result.
    Critical,
    /// Affects the result.
    Major,
    /// Detail.
    Minor,
}

/// A confidence between 0 and 1 (from an independent judge, never self-reported).
#[derive(Clone, Copy, Debug, PartialEq, PartialOrd, Serialize, Deserialize, JsonSchema)]
#[serde(transparent)]
pub struct Confidence(f32);

impl Confidence {
    /// Clamp into 0–1 (NaN becomes 0).
    #[must_use]
    pub fn new(v: f32) -> Self {
        Self(if v.is_nan() { 0.0 } else { v.clamp(0.0, 1.0) })
    }

    /// The value.
    #[must_use]
    pub const fn get(&self) -> f32 {
        self.0
    }
}

/// A unit of measurement, as written (`m/s`, `K`, `kg m^-3`). Dimensional analysis lives in
/// the Validation engine; here the unit is kept, never dropped.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(transparent)]
pub struct Unit(pub String);

/// A number with its unit and optional uncertainty.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Quantity {
    /// Value.
    pub value: f64,
    /// Unit (empty for dimensionless).
    pub unit: Unit,
    /// Standard uncertainty, same unit.
    #[serde(default)]
    pub uncertainty: Option<f64>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn confidence_is_clamped() {
        assert!((Confidence::new(1.7).get() - 1.0).abs() < f32::EPSILON);
        assert!(Confidence::new(-0.2).get().abs() < f32::EPSILON);
        assert!(Confidence::new(f32::NAN).get().abs() < f32::EPSILON);
    }

    #[test]
    fn status_is_tagged() {
        let j = serde_json::to_value(Status::Missing).expect("json");
        assert_eq!(j["kind"], "missing");
    }
}
