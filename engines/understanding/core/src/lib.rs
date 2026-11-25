//! Core of the Understanding engine: what code can know about a table, an equation or an answer
//! (P1.M5.UND.4–6). No I/O, no model.
//!
//! - [`abbreviations`]: what the paper says an abbreviation stands for.
//! - [`table`]: an answer from a table's cells, computed from a plan.
//! - [`symbols`]: the symbols of a formula and what the paper says they mean.
//! - [`units`]: units as they are printed.
//! - [`formula`]: whether the units of a formula agree.
//! - [`numbers`]: numbers in an answer that stand nowhere in what it was made from.
//! - [`written`]: formulas of an answer as plain text.
#![deny(missing_docs)]
#![allow(clippy::many_single_char_names)] // places, units and numbers of formulas

pub mod abbreviations;
pub mod formula;
mod latex;
pub mod numbers;
pub mod symbols;
pub mod table;
pub mod units;
pub mod written;

/// A pattern written in this crate.
fn re(pattern: &str) -> regex::Regex {
    regex::Regex::new(pattern).unwrap_or_else(|_| unreachable!("a pattern of this crate"))
}
