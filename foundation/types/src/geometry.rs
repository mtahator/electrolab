//! Page geometry: one coordinate system for everything that crosses a boundary (ADR-003).
//!
//! **Page space:** PDF points (1/72 inch), origin at the top-left of the page's visible box,
//! y growing downward, page rotation already applied. Adapters convert to and from PDF native
//! space; screen pixels exist only inside the UI viewer.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::PaperId;

/// A point on a page.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct PagePoint {
    /// Distance from the left edge.
    pub x: f32,
    /// Distance from the top edge.
    pub y: f32,
}

/// A rectangle on one page.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
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

    /// Height in points.
    #[must_use]
    pub fn height(&self) -> f32 {
        self.y1 - self.y0
    }

    /// Width in points.
    #[must_use]
    pub fn width(&self) -> f32 {
        self.x1 - self.x0
    }

    /// Whether the point lies inside (edges included).
    #[must_use]
    pub fn contains(&self, p: PagePoint) -> bool {
        p.x >= self.x0 && p.x <= self.x1 && p.y >= self.y0 && p.y <= self.y1
    }

    /// Convert from PDF native space (y up, origin bottom-left of a page of height `page_height`).
    #[must_use]
    pub fn from_pdf(
        page: u32,
        page_height: f32,
        left: f32,
        bottom: f32,
        right: f32,
        top: f32,
    ) -> Self {
        Self {
            page,
            x0: left,
            y0: page_height - top,
            x1: right,
            y1: page_height - bottom,
        }
    }

    /// Convert to PDF native space: `(left, bottom, right, top)`.
    #[must_use]
    pub fn to_pdf(&self, page_height: f32) -> (f32, f32, f32, f32) {
        (
            self.x0,
            page_height - self.y1,
            self.x1,
            page_height - self.y0,
        )
    }
}

/// Four corners of text that may be rotated or skewed (upper-left, upper-right, lower-left,
/// lower-right — the order PDF `/QuadPoints` use in practice).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct PageQuad {
    /// 1-based page number.
    pub page: u32,
    /// Corners.
    pub p: [PagePoint; 4],
}

impl PageQuad {
    /// The axis-aligned quad of a rectangle.
    #[must_use]
    pub fn from_rect(r: &PageRect) -> Self {
        let pt = |x, y| PagePoint { x, y };
        Self {
            page: r.page,
            p: [
                pt(r.x0, r.y0),
                pt(r.x1, r.y0),
                pt(r.x0, r.y1),
                pt(r.x1, r.y1),
            ],
        }
    }

    /// Smallest rectangle around the quad.
    #[must_use]
    pub fn bounds(&self) -> PageRect {
        let xs = self.p.map(|q| q.x);
        let ys = self.p.map(|q| q.y);
        let min = |v: [f32; 4]| v.into_iter().fold(f32::INFINITY, f32::min);
        let max = |v: [f32; 4]| v.into_iter().fold(f32::NEG_INFINITY, f32::max);
        PageRect {
            page: self.page,
            x0: min(xs),
            y0: min(ys),
            x1: max(xs),
            y1: max(ys),
        }
    }
}

/// How far to zoom when going to a location.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[non_exhaustive]
pub enum ZoomSpec {
    /// Page width fills the view.
    FitWidth,
    /// Whole page visible.
    FitPage,
    /// A scale factor (1.0 = 100 %).
    Scale {
        /// Factor.
        factor: f32,
    },
}

/// "Go there": the single way to point at a place in a paper (links, history, search hits,
/// citations in answers, bookmarks).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Location {
    /// The paper.
    pub paper: PaperId,
    /// 1-based page.
    pub page: u32,
    /// Area to show and mark, if any.
    #[serde(default)]
    pub target: Option<PageRect>,
    /// Zoom to apply, if any.
    #[serde(default)]
    pub zoom: Option<ZoomSpec>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

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

    #[test]
    fn quad_bounds() {
        let r = PageRect {
            page: 2,
            x0: 1.0,
            y0: 2.0,
            x1: 3.0,
            y1: 4.0,
        };
        assert_eq!(PageQuad::from_rect(&r).bounds(), r);
    }

    proptest! {
        #[test]
        fn pdf_space_round_trip(h in 100.0f32..2000.0, l in 0.0f32..500.0, w in 0.1f32..500.0, b in 0.0f32..90.0, t in 0.1f32..9.0) {
            let (left, right, bottom, top) = (l, l + w, b, b + t);
            let r = PageRect::from_pdf(1, h, left, bottom, right, top);
            let (l2, b2, r2, t2) = r.to_pdf(h);
            prop_assert!((l2 - left).abs() < 1e-3 && (b2 - bottom).abs() < 1e-2 && (r2 - right).abs() < 1e-3 && (t2 - top).abs() < 1e-2);
            prop_assert!(r.y0 <= r.y1);
        }
    }
}
