//! Shared types used by every engine.
//!
//! Only what is already needed lives here; Phase 1 task M1.TYP adds ids, references, evidence
//! spans and quantities (technical-structure.md §7.4, Draft 2.2 §13.1).
#![deny(missing_docs)]

use serde::{Deserialize, Serialize};

/// A rectangle on one page, in **page space**: PDF points (1/72 inch), origin at the top-left
/// of the page's visible box, y growing downward, page rotation already applied (ADR-003).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct PageRect {
    /// 1-based page number.
    pub page: u32,
    /// Left edge.
    pub x0: f32,
    /// Top edge.
    pub y0: f32,
    /// Right edge.
    pub x1: f32,
    /// Bottom edge.
    pub y1: f32,
}

impl PageRect {
    /// The smallest rectangle containing both (on the page of `self`).
    #[must_use]
    pub fn union(&self, other: &Self) -> Self {
        Self {
            page: self.page,
            x0: self.x0.min(other.x0),
            y0: self.y0.min(other.y0),
            x1: self.x1.max(other.x1),
            y1: self.y1.max(other.y1),
        }
    }

    /// Height of the rectangle in points.
    #[must_use]
    pub fn height(&self) -> f32 {
        self.y1 - self.y0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn union_covers_both() {
        let a = PageRect {
            page: 1,
            x0: 10.0,
            y0: 10.0,
            x1: 20.0,
            y1: 20.0,
        };
        let b = PageRect {
            page: 1,
            x0: 15.0,
            y0: 5.0,
            x1: 30.0,
            y1: 12.0,
        };
        assert_eq!(
            a.union(&b),
            PageRect {
                page: 1,
                x0: 10.0,
                y0: 5.0,
                x1: 30.0,
                y1: 20.0
            }
        );
    }
}
