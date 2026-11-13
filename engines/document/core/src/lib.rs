//! Document engine core: pure functions over typed data (Draft 2.2 §7.1).
//! No I/O, no clock, no randomness, no network — adapters do that; the facade wires them.

pub mod search;
