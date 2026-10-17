//! Protocol clients: VLESS / Trojan / Shadowsocks-AEAD.
//! Each `dial_*` returns a boxed async stream proxying raw target payload.

use base64::Engine;
use crate::tls;
use crate::uri::Server;
use crate::ws::WsStream;
use aes_gcm::aead::{AeadInPlace, KeyInit};
use aes_gcm::{Aes128Gcm, Aes256Gcm, Nonce};
use hkdf::Hkdf;
use sha1::Sha1;
use sha2::{Digest, Sha256};
use std::pin::Pin;
use std::task::Poll;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio_rustls::client::TlsStream;

pub type BoxStream = Pin<Box<dyn AsyncReadWrite>>;
pub trait AsyncReadWrite: AsyncRead + AsyncWrite + Unpin + Send {}
impl<T: AsyncRead + AsyncWrite + Unpin + Send> AsyncReadWrite for T {}

#[derive(Debug, Clone)]
pub struct Addr {
    pub host: String,
    pub port: u16,
}

impl Addr {
    pub fn new(host: String, port: u16) -> Self {
        Self { host, port }
    }
    fn atyp_and_addr(&self) -> (u8, Vec<u8>) {
        use std::net::IpAddr;
        if let Ok(ip) = self.host.parse::<IpAddr>() {
            match ip {
                IpAddr::V4(v4) => (0x01, v4.octets().to_vec()),
                IpAddr::V6(v6) => (0x04, v6.octets().to_vec()),
            }
        } else {
            let mut v = vec![self.host.len() as u8];
            v.extend_from_slice(self.host.as_bytes());
            (0x02, v)
        }
    }
}

pub async fn tcp_connect(host: &str, port: u16, timeout_ms: u64) -> std::io::Result<TcpStream> {
    let addr = format!("{}:{}", host, port);
    let fut = TcpStream::connect(addr);
    let s = tokio::time::timeout(std::time::Duration::from_millis(timeout_ms), fut)
        .await
        .map_err(|_| std::io::Error::new(std::io::ErrorKind::TimedOut, "tcp connect timeout"))??;
    s.set_nodelay(true).ok();
    Ok(s)
}

fn vless_uuid_bytes(uuid: &str) -> std::io::Result<[u8; 16]> {
    let clean: String = uuid.trim().to_lowercase().replace('-', "");
    if clean.len() == 32 {
        let mut out = [0u8; 16];
        for i in 0..16 {
            out[i] = u8::from_str_radix(&clean[i * 2..i * 2 + 2], 16)
                .map_err(|_| std::io::Error::new(std::io::ErrorKind::InvalidData, "bad vless uuid hex"))?;
        }
        return Ok(out);
    }
    if let Ok(b) = base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(uuid.trim()) {
        if b.len() == 16 {
            let mut out = [0u8; 16];
            out.copy_from_slice(&b);
            return Ok(out);
        }
    }
    Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "bad vless uuid"))
}

/// Establish an upstream connection to `dst` through `srv`.
pub async fn dial(srv: &Server, dst: &Addr) -> std::io::Result<BoxStream> {
    match srv.proto.as_str() {
        "vless" => dial_vless(srv, dst).await,
        "trojan" => dial_trojan(srv, dst).await,
        "ss" => dial_ss(srv, dst).await,
        p => Err(std::io::Error::new(std::io::ErrorKind::Unsupported, format!("protocol {} not supported", p))),
    }
}

async fn tls_wrap(srv: &Server, tcp: TcpStream) -> std::io::Result<TlsStream<TcpStream>> {
    let sni_host = if srv.sni.is_empty() { srv.host.clone() } else { srv.sni.clone() };
    let connector = if srv.allow_insecure { tls::connector_insecure() } else { tls::connector_verified() };
    let name = tls::sni_name(&sni_host);
    connector.connect(name, tcp).await
}

/* ------------------------------- VLESS -------------------------------- */

async fn dial_vless(srv: &Server, dst: &Addr) -> std::io::Result<BoxStream> {
    let tcp = tcp_connect(&srv.host, srv.port, 8000).await?;
    let uid = vless_uuid_bytes(&srv.uuid)?;

    let inner: Box<dyn AsyncReadWrite> = if srv.security == "tls" {
        Box::new(tls_wrap(srv, tcp).await?)
    } else {
        Box::new(tcp)
    };

    if srv.net == "ws" {
        let mut ws: Box<dyn AsyncReadWrite> = ws_over(srv, inner).await?;
        write_vless_header(&mut ws, &uid, dst).await?;
        Ok(Box::into_pin(ws))
    } else {
        let mut s: Box<dyn AsyncReadWrite> = inner;
        write_vless_header(&mut s, &uid, dst).await?;
        Ok(Box::into_pin(s))
    }
}

async fn ws_over(srv: &Server, boxed: Box<dyn AsyncReadWrite>) -> std::io::Result<Box<dyn AsyncReadWrite>> {
    let ws_host = if srv.ws_host.is_empty() { srv.host.clone() } else { srv.ws_host.clone() };
    let extra: Vec<(&str, String)> = if srv.ws_host.is_empty() {
        vec![]
    } else {
        vec![("Host", srv.ws_host.clone())]
    };
    let ws: WsStream<Box<dyn AsyncReadWrite>> = crate::ws::connect(boxed, &ws_host, &srv.ws_path, &extra).await?;
    Ok(Box::new(ws))
}

async fn write_vless_header(s: &mut dyn AsyncReadWrite, uid: &[u8; 16], dst: &Addr) -> std::io::Result<()> {
    let mut req: Vec<u8> = Vec::with_capacity(64);
    req.push(0); // version
    req.extend_from_slice(uid);
    req.push(0); // addons length
    req.push(1); // cmd: tcp
    let (atyp, addr) = dst.atyp_and_addr();
    req.push(atyp);
    req.extend_from_slice(&addr);
    req.extend_from_slice(&dst.port.to_be_bytes());
    eprintln!("P-DBG: header write {} bytes", req.len());
    s.write_all(&req).await?;
    let mut resp = [0u8; 2];
    s.read_exact(&mut resp).await?;
    if resp[0] != 0 {
        return Err(std::io::Error::new(std::io::ErrorKind::ConnectionAborted, format!("vless: version {}", resp[0])));
    }
    let addon_len = resp[1] as usize;
    if addon_len > 0 {
        let mut junk = vec![0u8; addon_len];
        s.read_exact(&mut junk).await?;
    }
    Ok(())
}

/* ------------------------------- TROJAN -------------------------------- */

async fn dial_trojan(srv: &Server, dst: &Addr) -> std::io::Result<BoxStream> {
    let tcp = tcp_connect(&srv.host, srv.port, 8000).await?;
    let tls_s = tls_wrap(srv, tcp).await?;
    let mut s: Box<dyn AsyncReadWrite> = Box::new(tls_s);

    let mut hasher = Sha256::new();
    hasher.update(srv.uuid.as_bytes());
    let hex: String = hasher.finalize().iter().map(|b| format!("{:02x}", b)).collect();

    let mut head: Vec<u8> = Vec::with_capacity(80);
    head.extend_from_slice(hex.as_bytes());
    head.extend_from_slice(b"\r\n");
    let (atyp, addr) = dst.atyp_and_addr();
    head.push(atyp);
    head.extend_from_slice(&addr);
    head.extend_from_slice(&dst.port.to_be_bytes());
    head.extend_from_slice(b"\r\n");
    s.write_all(&head).await?;
    Ok(Box::into_pin(s))
}

/* --------------------------- SHADOWSOCKS ------------------------------- */

enum Cipher {
    Aes128(Aes128Gcm),
    Aes256(Aes256Gcm),
}

impl Cipher {
    fn seal(&self, counter: u64, buf: &mut [u8]) -> [u8; 16] {
        let n = nonce_from_counter(counter);
        let tag = match self {
            Cipher::Aes128(c) => c.encrypt_in_place_detached(Nonce::from_slice(&n), b"", buf),
            Cipher::Aes256(c) => c.encrypt_in_place_detached(Nonce::from_slice(&n), b"", buf),
        };
        let t = tag.expect("aead seal");
        let mut out = [0u8; 16];
        out.copy_from_slice(t.as_slice());
        out
    }
    fn open(&self, counter: u64, buf: &mut [u8], tag: &[u8]) -> std::io::Result<()> {
        let n = nonce_from_counter(counter);
        let res = match self {
            Cipher::Aes128(c) => c.decrypt_in_place_detached(Nonce::from_slice(&n), b"", buf, tag.into()),
            Cipher::Aes256(c) => c.decrypt_in_place_detached(Nonce::from_slice(&n), b"", buf, tag.into()),
        };
        res.map_err(|_| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("ss: aead open failed (ctr={}, blen={}, tlen={})", counter, buf.len(), tag.len()),
            )
        })
    }
}

fn nonce_from_counter(counter: u64) -> [u8; 12] {
    let mut n = [0u8; 12];
    n[4..12].copy_from_slice(&counter.to_be_bytes());
    n
}

struct SsAead {
    c: Cipher,
    counter: u64,
}

impl SsAead {
    fn new(c: Cipher) -> Self {
        Self { c, counter: 0 }
    }
    fn seal(&mut self, plain: &[u8]) -> Vec<u8> {
        let mut buf = plain.to_vec();
        let tag = self.c.seal(self.counter, &mut buf);
        self.counter += 1;
        let mut out = buf;
        out.extend_from_slice(&tag);
        out
    }
    /// input: ciphertext||tag ; output plain
    fn open(&mut self, ct: &[u8]) -> std::io::Result<Vec<u8>> {
        if ct.len() < 16 {
            return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "ss: short block"));
        }
        let (body, t) = ct.split_at(ct.len() - 16);
        let mut buf = body.to_vec();
        self.c.open(self.counter, &mut buf, t)?;
        self.counter += 1;
        Ok(buf)
    }
}

const SS_MAX_CHUNK: usize = 0x3F00;

pub struct SsStream {
    inner: Box<dyn AsyncReadWrite>,
    enc: SsAead,
    dec: SsAead,
    first_write_pending: bool,
    target: Vec<u8>,
    rx_raw: Vec<u8>,
    rx_plain: Vec<u8>,
    rx_eof: bool,
    rx_pending: Option<usize>,
}

impl SsStream {
    async fn new(mut inner: Box<dyn AsyncReadWrite>, srv: &Server, dst: &Addr) -> std::io::Result<Self> {
        let key_len: usize = match srv.method.as_str() {
            "aes-128-gcm" => 16,
            "aes-256-gcm" => 32,
            m => {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::Unsupported,
                    format!("ss method {} unsupported (use aes-*-gcm)", m),
                ))
            }
        };
        let mut salt = vec![0u8; key_len];
        rand::RngCore::fill_bytes(&mut rand::thread_rng(), &mut salt);
        let h = Hkdf::<Sha1>::new(Some(salt.as_slice()), srv.password.as_bytes());
        let mut subkey = vec![0u8; key_len];
        h.expand(b"ss-subkey", &mut subkey)
            .map_err(|_| std::io::Error::new(std::io::ErrorKind::InvalidData, "hkdf expand"))?;
        let cipher = match key_len {
            16 => Cipher::Aes128(Aes128Gcm::new_from_slice(&subkey).expect("key16")),
            _ => Cipher::Aes256(Aes256Gcm::new_from_slice(&subkey).expect("key32")),
        };
        // salt goes on the wire FIRST, before any chunk
        inner.write_all(&salt).await?;
        let (atyp, addr) = dst.atyp_and_addr();
        let mut target = Vec::with_capacity(addr.len() + 3);
        target.push(atyp);
        target.extend_from_slice(&addr);
        target.extend_from_slice(&dst.port.to_be_bytes());
        Ok(Self {
            inner,
            enc: SsAead::new(cipher),
            dec: SsAead::new(match key_len {
                16 => Cipher::Aes128(Aes128Gcm::new_from_slice(&subkey).expect("key16")),
                _ => Cipher::Aes256(Aes256Gcm::new_from_slice(&subkey).expect("key32")),
            }),
            first_write_pending: true,
            target,
            rx_raw: Vec::new(),
            rx_plain: Vec::new(),
            rx_eof: false,
            rx_pending: None,
        })
    }

    fn chunk_frame(&mut self, data: &[u8]) -> Vec<u8> {
        // [E(len2)]+[tag] [E(data)]+[tag]
        let mut lenb = (data.len() as u16).to_be_bytes().to_vec();
        let mut f = self.enc.seal(&lenb);
        f.extend(self.enc.seal(data));
        let _ = &mut lenb;
        f
    }
}

impl AsyncRead for SsStream {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        let this = self.get_mut();
        loop {
            if !this.rx_plain.is_empty() {
                let n = this.rx_plain.len().min(buf.remaining());
                let src: Vec<u8> = this.rx_plain.drain(..n).collect();
                buf.put_slice(&src);
                return Poll::Ready(Ok(()));
            }
            if this.rx_eof && this.rx_raw.is_empty() {
                return Poll::Ready(Ok(()));
            }
            // try decode one chunk from rx_raw (len block consumed immediately, pending tracks payload)
            if this.rx_pending.is_none() && this.rx_raw.len() >= 18 {
                let lenb = this.dec.open(&this.rx_raw[..18])?;
                this.rx_raw.drain(..18);
                let need = u16::from_be_bytes([lenb[0], lenb[1]]) as usize;
                if need > 0 {
                    this.rx_pending = Some(need);
                }
            }
            if let Some(need) = this.rx_pending {
                if this.rx_raw.len() >= need + 16 {
                    let payload = this.dec.open(&this.rx_raw[..need + 16])?;
                    this.rx_raw.drain(..need + 16);
                    this.rx_pending = None;
                    this.rx_plain.extend_from_slice(&payload);
                    continue;
                }
            } else if this.rx_pending.is_none() && this.rx_raw.is_empty() && this.rx_eof {
                return Poll::Ready(Ok(()));
            }
            if this.rx_eof {
                return Poll::Ready(Err(std::io::Error::new(std::io::ErrorKind::UnexpectedEof, "ss: truncated chunk")));
            }
            // pull more ciphertext
            let mut tmp = [0u8; 16 * 1024];
            let mut rb = tokio::io::ReadBuf::new(&mut tmp);
            match Pin::new(&mut this.inner).poll_read(cx, &mut rb) {
                Poll::Ready(Ok(())) => {
                    let n = rb.filled().len();
                    if n == 0 {
                        this.rx_eof = true;
                    } else {
                        this.rx_raw.extend_from_slice(rb.filled());
                    }
                }
                Poll::Ready(Err(e)) => return Poll::Ready(Err(e)),
                Poll::Pending => return Poll::Pending,
            }
        }
    }
}

impl AsyncWrite for SsStream {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        data: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        // First write carries the target address as its own chunk.
        if self.first_write_pending {
            let target = self.target.clone();
            let mut f = self.chunk_frame(&target);
            match Pin::new(&mut self.inner).poll_write(cx, &f) {
                Poll::Ready(Ok(_)) => self.first_write_pending = false,
                Poll::Ready(Err(e)) => return Poll::Ready(Err(e)),
                Poll::Pending => return Poll::Pending,
            }
            let _ = &mut f;
        }
        let chunk = data.len().min(SS_MAX_CHUNK);
        let frame = self.chunk_frame(&data[..chunk]);
        match Pin::new(&mut self.inner).poll_write(cx, &frame) {
            Poll::Ready(Ok(_)) => Poll::Ready(Ok(chunk)),
            Poll::Ready(Err(e)) => Poll::Ready(Err(e)),
            Poll::Pending => Poll::Pending,
        }
    }
    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut std::task::Context<'_>) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.inner).poll_flush(cx)
    }
    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut std::task::Context<'_>) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.inner).poll_shutdown(cx)
    }
}

async fn dial_ss(srv: &Server, dst: &Addr) -> std::io::Result<BoxStream> {
    let tcp = tcp_connect(&srv.host, srv.port, 8000).await?;
    let inner: Box<dyn AsyncReadWrite> = Box::new(tcp);
    let s = SsStream::new(inner, srv, dst).await?;
    Ok(Box::into_pin(Box::new(s)))
}

/* ------------------------------ helpers -------------------------------- */

pub fn method_supported(m: &str) -> bool {
    matches!(m, "aes-128-gcm" | "aes-256-gcm")
}
