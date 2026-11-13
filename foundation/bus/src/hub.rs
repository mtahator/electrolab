//! Events: publish/subscribe (P1.M1.BUS.4).
//!
//! Publishing never waits. Each subscriber has a bounded buffer; a subscriber that falls more
//! than the buffer behind loses the oldest events and is told how many it missed
//! ([`Recv::Missed`]) — it then re-reads state instead of trusting the stream.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::sync::broadcast;

/// An event on the hub.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BusEvent {
    /// Dotted kind, e.g. `workspace.file_added`.
    pub kind: String,
    /// Payload.
    pub data: Value,
    /// Milliseconds since the Unix epoch.
    pub at_ms: i64,
}

/// Publish/subscribe hub.
#[derive(Debug)]
pub struct EventHub {
    tx: broadcast::Sender<BusEvent>,
}

/// What a subscriber receives.
#[derive(Debug, PartialEq)]
pub enum Recv {
    /// The next event.
    Event(BusEvent),
    /// This many events were dropped because the subscriber was too slow.
    Missed(u64),
    /// The hub is gone.
    Closed,
}

/// A subscription.
#[derive(Debug)]
pub struct Subscription {
    rx: broadcast::Receiver<BusEvent>,
    prefix: String,
}

impl Subscription {
    /// Wait for the next matching event.
    pub async fn recv(&mut self) -> Recv {
        loop {
            match self.rx.recv().await {
                Ok(e) if e.kind.starts_with(&self.prefix) => return Recv::Event(e),
                Ok(_) => {}
                Err(broadcast::error::RecvError::Lagged(n)) => return Recv::Missed(n),
                Err(broadcast::error::RecvError::Closed) => return Recv::Closed,
            }
        }
    }
}

impl EventHub {
    /// A hub where each subscriber buffers up to `capacity` events.
    #[must_use]
    pub fn new(capacity: usize) -> Self {
        Self {
            tx: broadcast::channel(capacity.max(1)).0,
        }
    }

    /// Publish; never blocks, never fails (no subscribers is fine).
    pub fn publish(&self, kind: &str, data: Value, at_ms: i64) {
        let _ = self.tx.send(BusEvent {
            kind: kind.into(),
            data,
            at_ms,
        });
    }

    /// Subscribe to events whose kind starts with `prefix` (`""` for all).
    #[must_use]
    pub fn subscribe(&self, prefix: &str) -> Subscription {
        Subscription {
            rx: self.tx.subscribe(),
            prefix: prefix.into(),
        }
    }
}
