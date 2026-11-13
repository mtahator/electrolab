//! Named pipes for local IPC (ADR-009).
//!
//! Pipes are local only (`reject_remote_clients`), one instance per name
//! (`first_pipe_instance`, so nobody can pre-create a pipe with our name), and named with a
//! random part chosen by the host and passed to the child, so other processes cannot guess it.

use std::time::Duration;

use ep_os::{LocalIpc, OsError, Result, Stream};
use futures::future::BoxFuture;
use tokio::net::windows::named_pipe::{ClientOptions, ServerOptions};

/// Windows named pipes under `\\.\pipe\`.
#[derive(Clone, Copy, Debug, Default)]
pub struct NamedPipes;

fn full(name: &str) -> String {
    format!(r"\\.\pipe\{name}")
}

const ERROR_PIPE_BUSY: i32 = 231;

impl LocalIpc for NamedPipes {
    fn accept<'a>(&'a self, name: &'a str) -> BoxFuture<'a, Result<Box<dyn Stream>>> {
        Box::pin(async move {
            let server = ServerOptions::new()
                .first_pipe_instance(true)
                .reject_remote_clients(true)
                .create(full(name))?;
            server.connect().await?;
            Ok(Box::new(server) as Box<dyn Stream>)
        })
    }

    fn connect<'a>(
        &'a self,
        name: &'a str,
        timeout_ms: u64,
    ) -> BoxFuture<'a, Result<Box<dyn Stream>>> {
        Box::pin(async move {
            let deadline = tokio::time::Instant::now() + Duration::from_millis(timeout_ms);
            loop {
                match ClientOptions::new().open(full(name)) {
                    Ok(c) => return Ok(Box::new(c) as Box<dyn Stream>),
                    Err(e)
                        if e.kind() == std::io::ErrorKind::NotFound
                            || e.raw_os_error() == Some(ERROR_PIPE_BUSY) =>
                    {
                        if tokio::time::Instant::now() >= deadline {
                            return Err(OsError::Io(std::io::Error::new(
                                std::io::ErrorKind::TimedOut,
                                format!("no listener on pipe {name}"),
                            )));
                        }
                        tokio::time::sleep(Duration::from_millis(2)).await;
                    }
                    Err(e) => return Err(e.into()),
                }
            }
        })
    }
}
