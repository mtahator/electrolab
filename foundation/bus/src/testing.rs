//! Transport conformance suite (P1.M1.BUS.6): every transport — in-process, named pipe — must
//! pass the same checks. Register [`register_fixture_commands`] on the bus behind the
//! transport, then call [`run_conformance`] with the transport.

use std::time::{Duration, Instant};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio::sync::mpsc;

use crate::{Bus, CommandError, Envelope, Permission, Spec, Stability, Transport, call_typed};

/// Input of `test.echo` and `test.count`.
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct Echo {
    /// Text.
    pub text: String,
    /// Number.
    pub n: u32,
}

fn spec(name: &'static str, permission: Permission) -> Spec {
    Spec {
        name,
        summary: "conformance fixture",
        stability: Stability::Experimental,
        permission,
    }
}

/// Register the fixture commands the suite calls.
///
/// # Errors
/// Already registered.
pub fn register_fixture_commands(bus: &Bus) -> Result<(), crate::RegisterError> {
    bus.register(
        spec("test.echo", Permission::read()),
        |_cx, i: Echo| async move { Ok::<_, CommandError>(i.text.repeat(i.n as usize)) },
    )?;
    bus.register(
        spec("test.fail", Permission::read()),
        |_cx, (): ()| async move { Err::<(), _>(CommandError::failed("broken", "always fails")) },
    )?;
    bus.register(
        spec("test.sleep", Permission::read()),
        |cx, ms: u64| async move {
            let end = Instant::now() + Duration::from_millis(ms);
            while Instant::now() < end {
                if cx.is_cancelled() {
                    return Err(CommandError::Cancelled);
                }
                tokio::time::sleep(Duration::from_millis(1)).await;
            }
            Ok(ms)
        },
    )?;
    bus.register_streaming(
        spec("test.count", Permission::read()),
        |cx, i: Echo| async move {
            for k in 0..i.n {
                cx.progress(k);
            }
            Ok::<_, CommandError>(i.n)
        },
    )?;
    Ok(())
}

/// A failed conformance check.
#[derive(Debug)]
pub struct Failure(pub String);

fn check(ok: bool, what: &str) -> Result<(), Failure> {
    if ok {
        Ok(())
    } else {
        Err(Failure(what.into()))
    }
}

/// Run every check against `t`.
///
/// # Errors
/// The first check that fails.
pub async fn run_conformance(t: &(dyn Transport + Sync)) -> Result<(), Failure> {
    // Typed round trip.
    let r: Result<String, _> = call_typed(
        t,
        "test.echo",
        &Echo {
            text: "ab".into(),
            n: 3,
        },
    )
    .await;
    check(r.as_deref() == Ok("ababab"), "echo round trip")?;
    // Errors are reported the same way.
    let r = t.call(Envelope::new("test.nope", json!(null))).await;
    check(
        matches!(r, Err(CommandError::UnknownCommand { .. })),
        "unknown command",
    )?;
    let r = t
        .call(Envelope::new("test.echo", json!({ "text": 1 })))
        .await;
    check(
        matches!(r, Err(CommandError::InvalidInput { .. })),
        "invalid input",
    )?;
    let r = t.call(Envelope::new("test.fail", json!(null))).await;
    check(
        matches!(r, Err(CommandError::Failed { ref kind, .. }) if kind == "broken"),
        "handler failure",
    )?;
    // Deadline.
    let mut env = Envelope::new("test.sleep", json!(5000));
    env.deadline_ms = Some(30);
    let t0 = Instant::now();
    let r = t.call(env).await;
    check(
        matches!(r, Err(CommandError::DeadlineExceeded { deadline_ms: 30 })),
        "deadline error",
    )?;
    check(
        t0.elapsed() < Duration::from_millis(500),
        "deadline is prompt",
    )?;
    // Cancellation within 50 ms.
    let env = Envelope::new("test.sleep", json!(5000));
    let id = env.id;
    let call = t.call(env);
    let cancel = async {
        tokio::time::sleep(Duration::from_millis(50)).await;
        let at = Instant::now();
        t.cancel(id).await;
        at
    };
    let (r, at) = tokio::join!(call, cancel);
    check(matches!(r, Err(CommandError::Cancelled)), "cancelled error")?;
    check(
        at.elapsed() < Duration::from_millis(50),
        "cancel stops the command within 50 ms",
    )?;
    // Streaming.
    let (tx, mut rx) = mpsc::unbounded_channel();
    let r = t
        .call_with_progress(
            Envelope::new("test.count", json!({ "text": "", "n": 5 })),
            tx,
        )
        .await;
    check(r == Ok(json!(5)), "streaming result")?;
    let mut items = vec![];
    while let Ok(v) = rx.try_recv() {
        items.push(v);
    }
    check(
        items == (0..5).map(|k| json!(k)).collect::<Vec<_>>(),
        "streaming items in order",
    )?;
    // Listing.
    let list = t.commands().await.map_err(|e| Failure(e.to_string()))?;
    check(
        list.iter().any(|c| c.name == "test.count" && c.streaming),
        "commands listed",
    )?;
    Ok(())
}
