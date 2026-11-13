//! `Cx`: everything a command handler may use for side effects (Draft 2.2 §13.1, P1.M1.TYP.4).
//!
//! Handlers take time, ids, cancellation and progress from here, never from globals, so tests
//! can run them with a fake clock and deterministic ids.

use std::sync::Arc;
use std::sync::atomic::{AtomicI64, AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use ep_types::Actor;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

/// Source of the current time.
pub trait Clock: Send + Sync + std::fmt::Debug {
    /// Milliseconds since the Unix epoch.
    fn now_ms(&self) -> i64;
}

/// The real clock.
#[derive(Debug, Default)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now_ms(&self) -> i64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
    }
}

/// A clock tests can set and advance.
#[derive(Debug, Default)]
pub struct FakeClock(AtomicI64);

impl FakeClock {
    /// Start at `ms`.
    #[must_use]
    pub fn at(ms: i64) -> Self {
        Self(AtomicI64::new(ms))
    }

    /// Move forward.
    pub fn advance(&self, ms: i64) {
        self.0.fetch_add(ms, Ordering::SeqCst);
    }
}

impl Clock for FakeClock {
    fn now_ms(&self) -> i64 {
        self.0.load(Ordering::SeqCst)
    }
}

/// Source of new ids.
pub trait IdSource: Send + Sync + std::fmt::Debug {
    /// A fresh UUID.
    fn next(&self) -> Uuid;
}

/// Time-ordered random ids (UUID v7).
#[derive(Debug, Default)]
pub struct RandomIds;

impl IdSource for RandomIds {
    fn next(&self) -> Uuid {
        Uuid::now_v7()
    }
}

/// Ids 1, 2, 3, … for tests.
#[derive(Debug, Default)]
pub struct SeqIds(AtomicU64);

impl IdSource for SeqIds {
    fn next(&self) -> Uuid {
        Uuid::from_u128(u128::from(self.0.fetch_add(1, Ordering::SeqCst) + 1))
    }
}

/// Context of one command call.
#[derive(Clone, Debug)]
pub struct Cx {
    /// Who asked.
    pub actor: Actor,
    /// Correlation id of the call (the envelope id).
    pub correlation: Uuid,
    /// Cancelled when the caller cancels or the deadline passes.
    pub cancel: CancellationToken,
    /// Time.
    pub clock: Arc<dyn Clock>,
    /// New ids.
    pub ids: Arc<dyn IdSource>,
    progress: Option<mpsc::UnboundedSender<serde_json::Value>>,
}

impl Cx {
    /// A context (built by the bus; tests may build one directly).
    #[must_use]
    pub fn new(
        actor: Actor,
        correlation: Uuid,
        clock: Arc<dyn Clock>,
        ids: Arc<dyn IdSource>,
    ) -> Self {
        Self {
            actor,
            correlation,
            cancel: CancellationToken::new(),
            clock,
            ids,
            progress: None,
        }
    }

    /// A test context: user actor, clock at 0, ids 1, 2, 3, ….
    #[must_use]
    pub fn test() -> Self {
        Self::new(
            Actor::User,
            Uuid::nil(),
            Arc::new(FakeClock::at(0)),
            Arc::new(SeqIds::default()),
        )
    }

    pub(crate) fn with_progress(mut self, tx: mpsc::UnboundedSender<serde_json::Value>) -> Self {
        self.progress = Some(tx);
        self
    }

    /// Send a partial result to a streaming caller (ignored when nobody listens).
    pub fn progress(&self, item: impl serde::Serialize) {
        if let (Some(tx), Ok(v)) = (&self.progress, serde_json::to_value(item)) {
            let _ = tx.send(v);
        }
    }

    /// Whether the call was cancelled; long loops check this between steps.
    #[must_use]
    pub fn is_cancelled(&self) -> bool {
        self.cancel.is_cancelled()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cx_is_deterministic() {
        let a = Cx::test();
        let b = Cx::test();
        assert_eq!(a.ids.next(), b.ids.next());
        assert_eq!(a.ids.next(), Uuid::from_u128(2));
        assert_eq!(a.clock.now_ms(), 0);
    }
}
