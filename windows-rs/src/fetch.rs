//! Minimal async HTTPS GET (rustls/ring, HTTP/1.1, Connection: close).
//! Enough for subscription fetches — no heavy HTTP client dependency.

use std::io;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

fn parse_url(url: &str) -> io::Result<(String, u16, String)> {
    let rest = url.strip_prefix("https://").ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "only https"))?;
    let (hostport, path) = match rest.find('/') {
        Some(i) => (&rest[..i], &rest[i..]),
        None => (rest, "/"),
    };
    let (host, port) = match hostport.rsplit_once(':') {
        Some((h, p)) => (h.to_string(), p.parse().unwrap_or(443)),
        None => (hostport.to_string(), 443),
    };
    Ok((host, port, path.to_string()))
}

fn dechunk(body: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(body.len());
    let mut i = 0usize;
    while i < body.len() {
        let line_end = match body[i..].windows(2).position(|w| w == b"\r\n") {
            Some(p) => i + p,
            None => break,
        };
        let size_str = String::from_utf8_lossy(&body[i..line_end]);
        let size = usize::from_str_radix(size_str.trim().split(';').next().unwrap_or("0"), 16).unwrap_or(0);
        if size == 0 {
            break;
        }
        let start = line_end + 2;
        let end = (start + size).min(body.len());
        out.extend_from_slice(&body[start..end]);
        i = end + 2; // skip trailing CRLF
    }
    out
}

pub async fn http_get(url: &str, max_bytes: usize) -> io::Result<String> {
    let (host, port, path) = parse_url(url)?;
    let tcp_fut = TcpStream::connect((host.as_str(), port));
    let tcp = tokio::time::timeout(std::time::Duration::from_millis(8000), tcp_fut)
        .await
        .map_err(|_| io::Error::new(io::ErrorKind::TimedOut, "connect timeout"))??;
    tcp.set_nodelay(true).ok();

    let connector = crate::tls::connector_verified();
    let name = crate::tls::sni_name(&host);
    let tls = tokio::time::timeout(std::time::Duration::from_millis(8000), connector.connect(name, tcp))
        .await
        .map_err(|_| io::Error::new(io::ErrorKind::TimedOut, "tls timeout"))??;

    let mut s: Box<dyn crate::proto::AsyncReadWrite> = Box::new(tls);
    let req = format!(
        "GET {} HTTP/1.1\r\nHost: {}\r\nUser-Agent: veilvpn/1.6\r\nAccept: */*\r\nConnection: close\r\n\r\n",
        path, host
    );
    s.write_all(req.as_bytes()).await?;

    let mut raw = Vec::with_capacity(64 * 1024);
    let mut buf = [0u8; 16 * 1024];
    loop {
        let n = tokio::time::timeout(std::time::Duration::from_millis(12000), s.read(&mut buf)).await??;
        if n == 0 {
            break;
        }
        raw.extend_from_slice(&buf[..n]);
        if raw.len() > max_bytes {
            break;
        }
    }
    // split headers/body
    let pos = raw
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "bad http response"))?;
    let head = String::from_utf8_lossy(&raw[..pos]).to_ascii_lowercase();
    let body = &raw[pos + 4..];
    let chunked = head.contains("transfer-encoding: chunked");
    let out = if chunked { dechunk(body) } else { body.to_vec() };
    Ok(String::from_utf8_lossy(&out).into_owned())
}
