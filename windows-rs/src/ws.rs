//! Minimal async WebSocket client (RFC 6455 subset) over any tokio stream.
//! Used for vless+ws transport.

use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use sha1::{Sha1, Digest};
use base64::Engine;
use std::pin::Pin;
use std::task::{Context, Poll};

pub trait AsyncRW: AsyncRead + AsyncWrite + Unpin + Send {}
impl<T: AsyncRead + AsyncWrite + Unpin + Send> AsyncRW for T {}

pub struct WsStream<S: AsyncRW> {
    inner: S,
    rx_buf: Vec<u8>,   // decoded payload pending
    rx_frag: Vec<u8>,  // partial frame bytes
    closed: bool,
    masked_tx: bool,
    pending_pong: bool,
}

fn ws_key() -> String {
    let mut k = [0u8; 16];
    rand::RngCore::fill_bytes(&mut rand::thread_rng(), &mut k);
    base64::engine::general_purpose::STANDARD.encode(k)
}

fn sha1_b64(data: &[u8]) -> String {
    let mut h = Sha1::new();
    h.update(data);
    base64::engine::general_purpose::STANDARD.encode(h.finalize())
}

/// Perform handshake and return a stream whose read/write carry *payload* bytes.
pub async fn connect<S: AsyncRW>(
    mut stream: S,
    host: &str,       // Host header value (may include port)
    path: &str,       // e.g. "/ws?ed=2048"
    extra_headers: &[(&str, String)],
) -> std::io::Result<WsStream<S>> {
    let key = ws_key();
    let path = if path.starts_with('/') { path.to_string() } else { format!("/{}", path) };
    let mut req = format!(
        "GET {} HTTP/1.1\r\nHost: {}\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Key: {}\r\nSec-WebSocket-Version: 13\r\n",
        path, host, key
    );
    for (k, v) in extra_headers {
        req.push_str(&format!("{}: {}\r\n", k, v));
    }
    req.push_str("\r\n");
    eprintln!("WS-DBG: sending req {} bytes", req.len());
    stream.write_all(req.as_bytes()).await?;
    stream.flush().await?;

    // read until \r\n\r\n
    let mut head = Vec::with_capacity(512);
    let mut buf = [0u8; 512];
    loop {
        let n = stream.read(&mut buf).await?;
        if n == 0 {
            return Err(std::io::Error::new(std::io::ErrorKind::UnexpectedEof, "ws: eof during handshake"));
        }
        head.extend_from_slice(&buf[..n]);
        if let Some(pos) = find_head_end(&head) {
            // validate
            let text = String::from_utf8_lossy(&head[..pos]);
            let mut lines = text.split("\r\n");
            let status = lines.next().unwrap_or("");
            if !status.contains(" 101 ") {
                return Err(std::io::Error::new(std::io::ErrorKind::ConnectionAborted, format!("ws: bad status: {}", status)));
            }
            let mut accept_ok = false;
            for l in lines {
                if let Some((k, v)) = l.split_once(':') {
                    if k.trim().eq_ignore_ascii_case("sec-websocket-accept") {
                        let want = sha1_b64(format!("{}258EAFA5-E914-47DA-95CA-C5AB0DC85B11", key).as_bytes());
                        if v.trim() == want {
                            accept_ok = true;
                        }
                    }
                }
            }
            if !accept_ok {
                return Err(std::io::Error::new(std::io::ErrorKind::ConnectionAborted, "ws: bad accept key"));
            }
            let rest = head[pos + 4..].to_vec();
            return Ok(WsStream {
                inner: stream,
                rx_buf: Vec::new(),
                rx_frag: rest,
                closed: false,
                masked_tx: true,
                pending_pong: false,
            });
        }
        if head.len() > 16 * 1024 {
            return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "ws: header too large"));
        }
    }
}

fn find_head_end(v: &[u8]) -> Option<usize> {
    v.windows(4).position(|w| w == b"\r\n\r\n")
}

impl<S: AsyncRW> WsStream<S> {
    fn make_frame(&self, data: &[u8], opcode: u8, fin: bool) -> Vec<u8> {
        let mut f = Vec::with_capacity(data.len() + 14);
        f.push(0x80 | opcode); // FIN + opcode
        let len = data.len();
        let mask_bit = if self.masked_tx { 0x80u8 } else { 0u8 };
        if len < 126 {
            f.push(mask_bit | len as u8);
        } else if len <= u16::MAX as usize {
            f.push(mask_bit | 126);
            f.extend_from_slice(&(len as u16).to_be_bytes());
        } else {
            f.push(mask_bit | 127);
            f.extend_from_slice(&(len as u64).to_be_bytes());
        }
        if self.masked_tx {
            let mut mask = [0u8; 4];
            rand::RngCore::fill_bytes(&mut rand::thread_rng(), &mut mask);
            f.extend_from_slice(&mask);
            for (i, b) in data.iter().enumerate() {
                f.push(b ^ mask[i % 4]);
            }
        } else {
            f.extend_from_slice(data);
        }
        let _ = fin;
        f
    }

    async fn send_control(&mut self, opcode: u8) -> std::io::Result<()> {
        let frame = self.make_frame(&[], opcode, true);
        self.inner.write_all(&frame).await?;
        self.inner.flush().await
    }

    /// Decode frames from rx_frag into rx_buf payload; handle ping/pong/close.
    fn decode_frames(&mut self) -> std::io::Result<bool> {
        // returns Ok(true) if any data payload decoded
        let mut any_data = false;
        loop {
            if self.rx_frag.len() < 2 {
                return Ok(any_data);
            }
            let b0 = self.rx_frag[0];
            let b1 = self.rx_frag[1];
            let opcode = b0 & 0x0F;
            let masked = b1 & 0x80 != 0;
            let mut len = (b1 & 0x7F) as usize;
            let mut off = 2usize;
            if len == 126 {
                if self.rx_frag.len() < 4 {
                    return Ok(any_data);
                }
                len = u16::from_be_bytes([self.rx_frag[2], self.rx_frag[3]]) as usize;
                off = 4;
            } else if len == 127 {
                if self.rx_frag.len() < 10 {
                    return Ok(any_data);
                }
                let mut a = [0u8; 8];
                a.copy_from_slice(&self.rx_frag[2..10]);
                len = u64::from_be_bytes(a) as usize;
                off = 10;
            }
            let mask_len = if masked { 4 } else { 0 };
            if self.rx_frag.len() < off + mask_len + len {
                return Ok(any_data);
            }
            let mask = if masked {
                let m = [self.rx_frag[off], self.rx_frag[off + 1], self.rx_frag[off + 2], self.rx_frag[off + 3]];
                off += 4;
                m
            } else {
                [0u8; 4]
            };
            let payload: Vec<u8> = self.rx_frag[off..off + len]
                .iter()
                .enumerate()
                .map(|(i, b)| b ^ mask[i % 4])
                .collect();
            self.rx_frag.drain(..off + len);
            match opcode {
                0x1 | 0x2 => {
                    self.rx_buf.extend_from_slice(&payload);
                    any_data = true;
                }
                0x9 => {
                    // ping -> queue pong immediately (send later via task? we send synchronously is impossible in sync fn)
                    self.rx_buf.extend_from_slice(&[]); // no data
                    self.pending_pong = true;
                }
                0x8 => {
                    self.closed = true;
                    return Ok(any_data);
                }
                _ => {}
            }
        }
    }
}

impl<S: AsyncRW> AsyncRead for WsStream<S> {
    fn poll_read(mut self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &mut tokio::io::ReadBuf<'_>) -> Poll<std::io::Result<()>> {
        if self.rx_buf.is_empty() && !self.rx_frag.is_empty() {
            match self.decode_frames() {
                Ok(_) => {}
                Err(e) => return Poll::Ready(Err(e)),
            }
        }
        if !self.rx_buf.is_empty() {
            let n = self.rx_buf.len().min(buf.remaining());
            let src: Vec<u8> = self.rx_buf.drain(..n).collect();
            buf.put_slice(&src);
            return Poll::Ready(Ok(()));
        }
        if self.closed {
            return Poll::Ready(Ok(())); // EOF
        }
        // read more from inner
        let mut tmp = [0u8; 16 * 1024];
        let mut rb = tokio::io::ReadBuf::new(&mut tmp);
        match Pin::new(&mut self.inner).poll_read(cx, &mut rb) {
            Poll::Ready(Ok(())) => {
                let filled = rb.filled().len();
                if filled == 0 {
                    return Poll::Ready(Ok(())); // EOF
                }
                self.rx_frag.extend_from_slice(rb.filled());
                match self.decode_frames() {
                    Ok(_) => {}
                    Err(e) => return Poll::Ready(Err(e)),
                }
                if !self.rx_buf.is_empty() {
                    let n = self.rx_buf.len().min(buf.remaining());
                    let src: Vec<u8> = self.rx_buf.drain(..n).collect();
                    buf.put_slice(&src);
                    return Poll::Ready(Ok(()));
                }
                Poll::Pending
            }
            Poll::Ready(Err(e)) => Poll::Ready(Err(e)),
            Poll::Pending => Poll::Pending,
        }
    }
}

impl<S: AsyncRW> AsyncWrite for WsStream<S> {
    fn poll_write(mut self: Pin<&mut Self>, cx: &mut Context<'_>, data: &[u8]) -> Poll<std::io::Result<usize>> {
        // answer pending ping with pong first
        if self.pending_pong {
            let pong = self.make_frame(&[], 0xA, true);
            match Pin::new(&mut self.inner).poll_write(cx, &pong) {
                Poll::Ready(Ok(_)) => self.pending_pong = false,
                Poll::Ready(Err(e)) => return Poll::Ready(Err(e)),
                Poll::Pending => return Poll::Pending,
            }
        }
        // chunk into frames <= 64KB to be gentle
        let chunk = data.len().min(64 * 1024);
        let frame = self.make_frame(&data[..chunk], 0x2, true);
        match Pin::new(&mut self.inner).poll_write(cx, &frame) {
            Poll::Ready(Ok(_n)) => Poll::Ready(Ok(chunk)),
            Poll::Ready(Err(e)) => Poll::Ready(Err(e)),
            Poll::Pending => Poll::Pending,
        }
    }
    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.inner).poll_flush(cx)
    }
    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.inner).poll_shutdown(cx)
    }
}
