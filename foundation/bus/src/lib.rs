//! Command bus (Draft 2.2 §8, §9; P1.M1.BUS).
//!
//! Every action in the app is a named command with typed input and output, called through a
//! [`Transport`] — in-process ([`Bus`]) or over IPC — so policy, provenance, budgets and
//! cancellation always apply, and the UI, CLI, MCP host and agents all use the same surface.
//!
//! Commands are registered with [`Bus::register`]: a generic function whose types give the
//! schemas, so a wrong handler signature is a compile error (this replaces the proc-macro
//! planned in P1.M1.BUS.2: the same guarantees with less machinery).
#![deny(missing_docs)]

mod bus;
pub mod cache;
mod command;
mod cx;
mod hub;
pub mod ipc;
pub mod testing;

pub use bus::{Bus, RegisterError, Spec, Transport, call_typed};
pub use command::{CommandError, CommandInfo, Envelope, Permission, Stability};
pub use cx::{Clock, Cx, FakeClock, IdSource, RandomIds, SeqIds, SystemClock};
pub use hub::{BusEvent, EventHub, Recv, Subscription};
