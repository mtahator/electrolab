//! The bus over a byte stream (P1.M1.BUS.6, ADR-009).
//!
//! Frames are a 4-byte little-endian length followed by one JSON [`Wire`] message. The
//! protocol works over any `AsyncRead`/`AsyncWrite` pair; the OS adapter supplies the stream
//! (a Windows named pipe today). Large data never travels here — only `BlobRef`s.

use std::collections::HashMap;
use std::sync::Arc;

use futures::future::BoxFuture;
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::sync::{mpsc, oneshot};
use uuid::Uuid;

use crate::{Bus, CommandError, CommandInfo, Envelope, Transport};

/// Largest accepted frame (64 MiB); larger messages must use the blob store.
pub const MAX_FRAME: usize = 64 << 20;

/// One message on the wire.
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "t", rename_all = "snake_case")]
pub enum Wire {
    /// Client → server: run a command.
    Call {
        /// The call.
        env: Envelope,
        /// Send progress frames.
        stream: bool,
    },
    /// Client → server: cancel a call.
    Cancel {
        /// Call id.
        id: Uuid,
    },
    /// Client → server: list commands.
    List {
        /// Request id.
        id: Uuid,
    },
    /// Server → client: a progress item.
    Progress {
        /// Call id.
        id: Uuid,
        /// Item.
        item: Value,
    },
    /// Server → client: the outcome of a call.
    Done {
        /// Call id.
        id: Uuid,
        /// Output or error.
        result: Result<Value, CommandError>,
    },
    /// Server → client: the command list.
    Commands {
        /// Request id.
        id: Uuid,
        /// Commands.
        list: Vec<CommandInfo>,
    },
}

/// Read one frame. `Ok(None)` at a clean end of stream.
///
/// # Errors
/// I/O error, oversize frame or bad JSON.
pub async fn read_frame<R: AsyncRead + Unpin>(r: &mut R) -> std::io::Result<Option<Wire>> {
    let n = match r.read_u32_le().await {
        Ok(n) => n as usize,
        Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(e) => return Err(e),
    };
    if n > MAX_FRAME {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("frame of {n} bytes exceeds {MAX_FRAME}"),
        ));
    }
    let mut buf = vec![0u8; n];
    r.read_exact(&mut buf).await?;
    serde_json::from_slice(&buf)
        .map(Some)
        .map_err(std::io::Error::other)
}

/// Write one frame.
///
/// # Errors
/// I/O error or oversize message.
pub async fn write_frame<W: AsyncWrite + Unpin>(w: &mut W, msg: &Wire) -> std::io::Result<()> {
    let body = serde_json::to_vec(msg).map_err(std::io::Error::other)?;
    let len = u32::try_from(body.len())
        .ok()
        .filter(|&n| n as usize <= MAX_FRAME)
        .ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "message too large; use a BlobRef",
            )
        })?;
    let mut frame = Vec::with_capacity(body.len() + 4);
    frame.extend_from_slice(&len.to_le_bytes());
    frame.extend_from_slice(&body);
    w.write_all(&frame).await?;
    w.flush().await
}

fn writer_task<W: AsyncWrite + Unpin + Send + 'static>(mut w: W) -> mpsc::UnboundedSender<Wire> {
    let (tx, mut rx) = mpsc::unbounded_channel::<Wire>();
    tokio::spawn(async move {
        while let Some(m) = rx.recv().await {
            if write_frame(&mut w, &m).await.is_err() {
                break;
            }
        }
    });
    tx
}

/// Serve `bus` on one connection until the peer closes it.
///
/// # Errors
/// I/O error on the read side (a clean close returns `Ok`).
pub async fn serve<R, W>(bus: Arc<Bus>, mut rd: R, wr: W) -> std::io::Result<()>
where
    R: AsyncRead + Unpin,
    W: AsyncWrite + Unpin + Send + 'static,
{
    let out = writer_task(wr);
    while let Some(msg) = read_frame(&mut rd).await? {
        match msg {
            Wire::Call { env, stream } => {
                let (bus, out) = (bus.clone(), out.clone());
                tokio::spawn(async move {
                    let id = env.id;
                    let result = if stream {
                        let (ptx, mut prx) = mpsc::unbounded_channel();
                        let fwd_out = out.clone();
                        let fwd = tokio::spawn(async move {
                            while let Some(item) = prx.recv().await {
                                let _ = fwd_out.send(Wire::Progress { id, item });
                            }
                        });
                        let r = bus.call_with_progress(env, ptx).await;
                        let _ = fwd.await; // all progress frames go out before Done
                        r
                    } else {
                        bus.call(env).await
                    };
                    let _ = out.send(Wire::Done { id, result });
                });
            }
            Wire::Cancel { id } => {
                bus.cancel(id);
            }
            Wire::List { id } => {
                let _ = out.send(Wire::Commands {
                    id,
                    list: bus.commands(),
                });
            }
            Wire::Progress { .. } | Wire::Done { .. } | Wire::Commands { .. } => {}
        }
    }
    Ok(())
}

enum Pending {
    Call {
        done: oneshot::Sender<Result<Value, CommandError>>,
        progress: Option<mpsc::UnboundedSender<Value>>,
    },
    List(oneshot::Sender<Vec<CommandInfo>>),
}

/// Client side of a connection; a [`Transport`].
pub struct IpcClient {
    out: mpsc::UnboundedSender<Wire>,
    pending: Arc<Mutex<HashMap<Uuid, Pending>>>,
    reader: tokio::task::JoinHandle<()>,
}

impl Drop for IpcClient {
    /// Closing the client closes the connection: the reader task owns the read half, and the
    /// writer task ends when `out` is dropped, so both halves of the stream are released.
    fn drop(&mut self) {
        self.reader.abort();
    }
}

impl std::fmt::Debug for IpcClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("IpcClient")
            .field("pending", &self.pending.lock().len())
            .finish_non_exhaustive()
    }
}

fn gone() -> CommandError {
    CommandError::Transport {
        message: "connection closed".into(),
    }
}

impl IpcClient {
    /// Start a client on a connected stream.
    pub fn new<R, W>(mut rd: R, wr: W) -> Self
    where
        R: AsyncRead + Unpin + Send + 'static,
        W: AsyncWrite + Unpin + Send + 'static,
    {
        let out = writer_task(wr);
        let pending: Arc<Mutex<HashMap<Uuid, Pending>>> = Arc::default();
        let p = pending.clone();
        let reader = tokio::spawn(async move {
            while let Ok(Some(msg)) = read_frame(&mut rd).await {
                match msg {
                    Wire::Progress { id, item } => {
                        if let Some(Pending::Call {
                            progress: Some(tx), ..
                        }) = p.lock().get(&id)
                        {
                            let _ = tx.send(item);
                        }
                    }
                    Wire::Done { id, result } => {
                        if let Some(Pending::Call { done, .. }) = p.lock().remove(&id) {
                            let _ = done.send(result);
                        }
                    }
                    Wire::Commands { id, list } => {
                        if let Some(Pending::List(done)) = p.lock().remove(&id) {
                            let _ = done.send(list);
                        }
                    }
                    Wire::Call { .. } | Wire::Cancel { .. } | Wire::List { .. } => {}
                }
            }
            // Peer gone: every waiting call fails now (senders dropped).
            p.lock().clear();
        });
        Self {
            out,
            pending,
            reader,
        }
    }

    async fn call_inner(
        &self,
        env: Envelope,
        progress: Option<mpsc::UnboundedSender<Value>>,
    ) -> Result<Value, CommandError> {
        let (done, rx) = oneshot::channel();
        let stream = progress.is_some();
        self.pending
            .lock()
            .insert(env.id, Pending::Call { done, progress });
        self.out
            .send(Wire::Call { env, stream })
            .map_err(|_| gone())?;
        rx.await.map_err(|_| gone())?
    }
}

impl Transport for IpcClient {
    fn call(&self, env: Envelope) -> BoxFuture<'_, Result<Value, CommandError>> {
        Box::pin(self.call_inner(env, None))
    }
    fn call_with_progress(
        &self,
        env: Envelope,
        tx: mpsc::UnboundedSender<Value>,
    ) -> BoxFuture<'_, Result<Value, CommandError>> {
        Box::pin(self.call_inner(env, Some(tx)))
    }
    fn cancel(&self, id: Uuid) -> BoxFuture<'_, ()> {
        let _ = self.out.send(Wire::Cancel { id });
        Box::pin(async {})
    }
    fn commands(&self) -> BoxFuture<'_, Result<Vec<CommandInfo>, CommandError>> {
        Box::pin(async move {
            let id = Uuid::now_v7();
            let (done, rx) = oneshot::channel();
            self.pending.lock().insert(id, Pending::List(done));
            self.out.send(Wire::List { id }).map_err(|_| gone())?;
            rx.await.map_err(|_| gone())
        })
    }
}
