//! A small HTTP/1.1 client for the sidecar on 127.0.0.1 only: one request per connection
//! (`Connection: close`), plain or chunked bodies, read in a cancel-safe way (raw bytes into a
//! buffer, then decoded), with a size limit. No TLS, no redirects, no proxies: it talks to the
//! child process the engine started, nothing else.

use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

/// Largest body accepted (guideline: parse the sidecar's output defensively).
pub const MAX_BODY: usize = 4 << 20;

const CRLF: &[u8] = b"\r\n";

/// Where the chunked decoder is.
#[derive(Clone, Copy)]
enum Chunk {
    /// Reading a size line.
    Size,
    /// Bytes left in the chunk.
    Data(usize),
    /// The line end after a chunk.
    End,
}

/// An open response.
pub struct Response {
    /// Status code.
    pub status: u16,
    stream: TcpStream,
    raw: Vec<u8>,
    chunked: bool,
    remaining: Option<usize>,
    chunk: Chunk,
    done: bool,
    total: usize,
}

/// Send a request to `127.0.0.1:port`.
///
/// # Errors
/// Connection or protocol errors.
pub async fn request(
    port: u16,
    method: &str,
    path: &str,
    key: &str,
    body: Option<&[u8]>,
) -> std::io::Result<Response> {
    let mut stream = tokio::time::timeout(
        Duration::from_secs(5),
        TcpStream::connect(("127.0.0.1", port)),
    )
    .await
    .map_err(|_| std::io::Error::new(std::io::ErrorKind::TimedOut, "connect"))??;
    stream.set_nodelay(true)?;
    let mut head = format!(
        "{method} {path} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nAuthorization: Bearer {key}\r\nConnection: close\r\nAccept: */*\r\n"
    );
    if let Some(b) = body {
        use std::fmt::Write as _;
        let _ = write!(
            head,
            "Content-Type: application/json\r\nContent-Length: {}\r\n",
            b.len()
        );
    }
    head.push_str("\r\n");
    stream.write_all(head.as_bytes()).await?;
    if let Some(b) = body {
        stream.write_all(b).await?;
    }
    stream.flush().await?;

    // Read until the end of the headers.
    let mut raw = Vec::with_capacity(4096);
    let end = loop {
        if let Some(i) = find(&raw, b"\r\n\r\n") {
            break i;
        }
        if raw.len() > 64 * 1024 {
            return Err(bad("headers too long"));
        }
        let mut buf = [0u8; 4096];
        let n = stream.read(&mut buf).await?;
        if n == 0 {
            return Err(bad("closed before the headers"));
        }
        raw.extend_from_slice(&buf[..n]);
    };
    let headers = String::from_utf8_lossy(&raw[..end]).to_string();
    raw.drain(..end + 4);
    let mut lines = headers.lines();
    let status = lines
        .next()
        .and_then(|l| l.split_whitespace().nth(1))
        .and_then(|s| s.parse().ok())
        .ok_or_else(|| bad("no status line"))?;
    let (mut chunked, mut remaining) = (false, None);
    for l in lines {
        let Some((k, v)) = l.split_once(':') else {
            continue;
        };
        let (k, v) = (k.trim().to_ascii_lowercase(), v.trim());
        if k == "transfer-encoding" && v.to_ascii_lowercase().contains("chunked") {
            chunked = true;
        } else if k == "content-length" {
            remaining = v.parse().ok();
        }
    }
    Ok(Response {
        status,
        stream,
        raw,
        chunked,
        remaining,
        chunk: Chunk::Size,
        done: false,
        total: 0,
    })
}

fn bad(m: &str) -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::InvalidData, m.to_string())
}

fn find(h: &[u8], n: &[u8]) -> Option<usize> {
    h.windows(n.len()).position(|w| w == n)
}

impl Response {
    /// Read more raw bytes. Cancel-safe: a dropped call loses nothing.
    async fn fill(&mut self) -> std::io::Result<bool> {
        let mut buf = [0u8; 8192];
        let n = self.stream.read(&mut buf).await?;
        self.raw.extend_from_slice(&buf[..n]);
        Ok(n > 0)
    }

    /// Decode what is buffered into body bytes, without reading.
    fn decode(&mut self, out: &mut Vec<u8>) -> std::io::Result<()> {
        if self.done {
            return Ok(());
        }
        if !self.chunked {
            let take = self
                .remaining
                .map_or(self.raw.len(), |r| r.min(self.raw.len()));
            out.extend(self.raw.drain(..take));
            if let Some(r) = &mut self.remaining {
                *r -= take;
                if *r == 0 {
                    self.done = true;
                }
            }
            return Ok(());
        }
        loop {
            match self.chunk {
                Chunk::Size => {
                    let Some(i) = find(&self.raw, CRLF) else {
                        return Ok(());
                    };
                    let line = String::from_utf8_lossy(&self.raw[..i]).to_string();
                    let size =
                        usize::from_str_radix(line.split(';').next().unwrap_or("").trim(), 16)
                            .map_err(|_| bad(&format!("bad chunk size {line:?}")))?;
                    self.raw.drain(..i + 2);
                    if size == 0 {
                        self.done = true;
                        return Ok(());
                    }
                    self.chunk = Chunk::Data(size);
                }
                Chunk::Data(left) => {
                    let take = left.min(self.raw.len());
                    out.extend(self.raw.drain(..take));
                    if take < left {
                        self.chunk = Chunk::Data(left - take);
                        return Ok(());
                    }
                    self.chunk = Chunk::End;
                }
                Chunk::End => {
                    if self.raw.len() < 2 {
                        return Ok(());
                    }
                    if !self.raw.starts_with(CRLF) {
                        return Err(bad("no line end after a chunk"));
                    }
                    self.raw.drain(..2);
                    self.chunk = Chunk::Size;
                }
            }
        }
    }

    /// The next body bytes; `None` at the end.
    ///
    /// # Errors
    /// Connection errors, a broken body, or a body over [`MAX_BODY`].
    pub async fn next(&mut self) -> std::io::Result<Option<Vec<u8>>> {
        loop {
            let mut out = Vec::new();
            self.decode(&mut out)?;
            if !out.is_empty() {
                self.total += out.len();
                if self.total > MAX_BODY {
                    return Err(bad("answer too long"));
                }
                return Ok(Some(out));
            }
            if self.done {
                return Ok(None);
            }
            if !self.fill().await? {
                return Ok(None);
            }
        }
    }

    /// The whole body as text.
    ///
    /// # Errors
    /// As [`Response::next`].
    pub async fn text(mut self) -> std::io::Result<String> {
        let mut all = Vec::new();
        while let Some(b) = self.next().await? {
            all.extend(b);
        }
        Ok(String::from_utf8_lossy(&all).into_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::net::TcpListener;

    async fn serve(reply: &'static [&'static [u8]]) -> u16 {
        let l = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = l.local_addr().unwrap().port();
        tokio::spawn(async move {
            let (mut s, _) = l.accept().await.unwrap();
            let mut buf = [0u8; 4096];
            let _ = s.read(&mut buf).await;
            for part in reply {
                s.write_all(part).await.unwrap();
                s.flush().await.unwrap();
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        });
        port
    }

    #[tokio::test]
    async fn chunked_body_split_anywhere() {
        let port = serve(&[
            b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n5\r\nhel",
            b"lo\r",
            b"\n6\r\n world\r\n0\r\n\r\n",
        ])
        .await;
        let r = request(port, "GET", "/", "k", None).await.unwrap();
        assert_eq!(r.status, 200);
        assert_eq!(r.text().await.unwrap(), "hello world");
    }

    #[tokio::test]
    async fn content_length_body() {
        let port = serve(&[b"HTTP/1.1 503 Busy\r\nContent-Length: 4\r\n\r\nbusy"]).await;
        let r = request(port, "GET", "/health", "k", None).await.unwrap();
        assert_eq!(r.status, 503);
        assert_eq!(r.text().await.unwrap(), "busy");
    }
}
