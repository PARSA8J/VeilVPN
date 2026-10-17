//! End-to-end protocol tests: real local mock servers, full client dial + relay roundtrip.

use base64::Engine;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

// Binary crate: pull in internals via #[path] modules (single crate, no lib needed).
#[path = "../src/uri.rs"]
mod uri;
#[path = "../src/tls.rs"]
mod tls;
#[path = "../src/ws.rs"]
mod ws;
#[path = "../src/proto.rs"]
mod proto;

use proto::{Addr, AsyncReadWrite};
use uri::Server;

fn srv(proto_name: &str, port: u16) -> Server {
    let mut s = if proto_name == "ss" {
        uri::parse_uri(&format!("ss://YWVzLTI1Ni1nY206aHVudGVyMkB3aWxsLWJlLW92ZXJ3cml0dGVuOjE=#T")).unwrap_or_default()
    } else {
        uri::parse_uri(&format!("{}://test@127.0.0.1:{}#T", proto_name, port)).unwrap()
    };
    s.host = "127.0.0.1".into();
    s.port = port;
    s.uuid = "a7ba5f2c-0e4d-4a4f-9d1c-3f2b6c7d8e9f".into();
    s.password = "hunter2".into();
    s.method = "aes-256-gcm".into();
    s
}

fn srv_insecure(proto_name: &str, port: u16, sni: &str) -> Server {
    let mut s = srv(proto_name, port);
    s.security = "tls".into();
    s.sni = sni.into();
    s.allow_insecure = true;
    s
}

async fn roundtrip(st: &mut (dyn AsyncReadWrite + Unpin)) {
    let payload = b"The quick brown fox jumps over the lazy dog 0123456789";
    st.write_all(payload).await.expect("write");
    let mut buf = vec![0u8; payload.len()];
    tokio::time::timeout(std::time::Duration::from_secs(5), st.read_exact(&mut buf))
        .await
        .expect("timeout")
        .expect("read");
    assert_eq!(&buf, payload);
    // second roundtrip through the same stream (state continuity, nonce counters)
    st.write_all(b"second!").await.unwrap();
    let mut b2 = [0u8; 7];
    tokio::time::timeout(std::time::Duration::from_secs(5), st.read_exact(&mut b2))
        .await
        .expect("timeout2")
        .unwrap();
    assert_eq!(&b2, b"second!");
}

/* ------------------------------ VLESS mock ------------------------------ */

async fn mock_vless(l: TcpListener) {
    loop {
        let (mut s, _) = match l.accept().await {
            Ok(x) => x,
            Err(_) => return,
        };
        tokio::spawn(async move {
            let mut hdr = [0u8; 20]; // ver(1) uuid(16) addons(1) cmd(1) atyp(1)
            if s.read_exact(&mut hdr).await.is_err() {
                return;
            }
            match hdr[19] {
                0x01 => {
                    let mut b = [0u8; 4];
                    let _ = s.read_exact(&mut b).await;
                }
                0x02 => {
                    let mut l1 = [0u8; 1];
                    let _ = s.read_exact(&mut l1).await;
                    let mut b = vec![0u8; l1[0] as usize];
                    let _ = s.read_exact(&mut b).await;
                }
                0x04 => {
                    let mut b = [0u8; 16];
                    let _ = s.read_exact(&mut b).await;
                }
                _ => return,
            }
            let mut pb = [0u8; 2];
            if s.read_exact(&mut pb).await.is_err() {
                return;
            }
            if s.write_all(&[0x00, 0x00]).await.is_err() {
                return;
            }
            let (mut r, mut w) = tokio::io::split(s);
            let _ = tokio::io::copy(&mut r, &mut w).await;
        });
    }
}

/* --------------------------- VLESS+WS mock ------------------------------ */

fn ws_accept(key: &str) -> String {
    use sha1::{Digest, Sha1};
    let mut h = Sha1::new();
    h.update(key.as_bytes());
    h.update(b"258EAFA5-E914-47DA-95CA-C5AB0DC85B11");
    base64::engine::general_purpose::STANDARD.encode(h.finalize())
}

async fn mock_vless_ws(l: TcpListener) {
    loop {
        let (mut s, _) = match l.accept().await {
            Ok(x) => x,
            Err(_) => return,
        };
        tokio::spawn(async move {
            let mut head = Vec::new();
            let mut buf = [0u8; 1024];
            loop {
                let n = match s.read(&mut buf).await {
                    Ok(0) => return,
                    Ok(n) => n,
                    Err(_) => return,
                };
                head.extend_from_slice(&buf[..n]);
                if head.windows(4).any(|w| w == b"\r\n\r\n") {
                    break;
                }
            }
            let text = String::from_utf8_lossy(&head);
            let key = text
                .lines()
                .find_map(|l| l.strip_prefix("Sec-WebSocket-Key:"))
                .map(|v| v.trim().to_string())
                .unwrap_or_default();
            let resp = format!(
                "HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Accept: {}\r\n\r\n",
                ws_accept(&key)
            );
            if s.write_all(resp.as_bytes()).await.is_err() {
                return;
            }
            // vless-over-ws server: first frame = header (discard), reply [0,0]; then echo frames
            {
                let mut h2 = [0u8; 2];
                if s.read_exact(&mut h2).await.is_err() {
                    return;
                }
                let masked = h2[1] & 0x80 != 0;
                let mut len = (h2[1] & 0x7F) as usize;
                if len == 126 {
                    let mut e = [0u8; 2];
                    if s.read_exact(&mut e).await.is_err() {
                        return;
                    }
                    len = u16::from_be_bytes(e) as usize;
                } else if len == 127 {
                    let mut e = [0u8; 8];
                    if s.read_exact(&mut e).await.is_err() {
                        return;
                    }
                    len = u64::from_be_bytes(e) as usize;
                }
                if masked {
                    let mut m = [0u8; 4];
                    if s.read_exact(&mut m).await.is_err() {
                        return;
                    }
                }
                let mut payload = vec![0u8; len];
                if s.read_exact(&mut payload).await.is_err() {
                    return;
                }
                let reply = [0x82u8, 0x02, 0x00, 0x00];
                if s.write_all(&reply).await.is_err() {
                    return;
                }
            }
            // frame echo server: read masked frames, reply unmasked binary
            loop {
                let mut h2 = [0u8; 2];
                if s.read_exact(&mut h2).await.is_err() {
                    return;
                }
                let opcode = h2[0] & 0x0F;
                let masked = h2[1] & 0x80 != 0;
                let mut len = (h2[1] & 0x7F) as usize;
                if len == 126 {
                    let mut e = [0u8; 2];
                    if s.read_exact(&mut e).await.is_err() {
                        return;
                    }
                    len = u16::from_be_bytes(e) as usize;
                } else if len == 127 {
                    let mut e = [0u8; 8];
                    if s.read_exact(&mut e).await.is_err() {
                        return;
                    }
                    len = u64::from_be_bytes(e) as usize;
                }
                let mask = if masked {
                    let mut m = [0u8; 4];
                    if s.read_exact(&mut m).await.is_err() {
                        return;
                    }
                    m
                } else {
                    [0u8; 4]
                };
                let mut payload = vec![0u8; len];
                if s.read_exact(&mut payload).await.is_err() {
                    return;
                }
                if masked {
                    for (i, b) in payload.iter_mut().enumerate() {
                        *b ^= mask[i % 4];
                    }
                }
                match opcode {
                    0x1 | 0x2 => {
                        let mut frame = vec![0x82u8];
                        if len < 126 {
                            frame.push(len as u8);
                        } else {
                            frame.push(126);
                            frame.extend_from_slice(&(len as u16).to_be_bytes());
                        }
                        frame.extend_from_slice(&payload);
                        if s.write_all(&frame).await.is_err() {
                            return;
                        }
                    }
                    0x8 => return,
                    _ => {}
                }
            }
        });
    }
}

/* ----------------------------- Trojan mock ------------------------------ */

async fn mock_trojan_tls(l: TcpListener) {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let ck = rcgen::generate_simple_self_signed(vec!["mock.tj".to_string()]).expect("rcgen");
    let cert_der = ck.cert.der().clone();
    let key_der = rustls::pki_types::PrivateKeyDer::Pkcs8(rustls::pki_types::PrivatePkcs8KeyDer::from(
        ck.key_pair.serialize_der(),
    ));
    let cfg = rustls::ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(vec![cert_der], key_der)
        .expect("server cert");
    let acceptor = tokio_rustls::TlsAcceptor::from(std::sync::Arc::new(cfg));
    loop {
        let (tcp, _) = match l.accept().await {
            Ok(x) => x,
            Err(_) => return,
        };
        let acceptor = acceptor.clone();
        tokio::spawn(async move {
            let mut s = match acceptor.accept(tcp).await {
                Ok(s) => s,
                Err(_) => return,
            };
            // head: hex(64) CRLF atyp addr port CRLF
            let mut head = vec![0u8; 64];
            if s.read_exact(&mut head).await.is_err() {
                return;
            }
            let mut crlf = [0u8; 2];
            if s.read_exact(&mut crlf).await.is_err() {
                return;
            }
            let mut atyp = [0u8; 1];
            if s.read_exact(&mut atyp).await.is_err() {
                return;
            }
            match atyp[0] {
                0x01 => {
                    let mut b = [0u8; 4];
                    let _ = s.read_exact(&mut b).await;
                }
                0x02 => {
                    let mut l1 = [0u8; 1];
                    let _ = s.read_exact(&mut l1).await;
                    let mut b = vec![0u8; l1[0] as usize];
                    let _ = s.read_exact(&mut b).await;
                }
                0x04 => {
                    let mut b = [0u8; 16];
                    let _ = s.read_exact(&mut b).await;
                }
                _ => return,
            }
            let mut pb = [0u8; 2];
            if s.read_exact(&mut pb).await.is_err() {
                return;
            }
            let mut c2 = [0u8; 2];
            if s.read_exact(&mut c2).await.is_err() {
                return;
            }
            let (mut r, mut w) = tokio::io::split(s);
            let _ = tokio::io::copy(&mut r, &mut w).await;
        });
    }
}

/* ------------------------------- SS mock -------------------------------- */

use aes_gcm::aead::{AeadInPlace, KeyInit};
use aes_gcm::{Aes256Gcm, Nonce};
use hkdf::Hkdf;
use sha1::Sha1;

struct MockSs {
    c: Aes256Gcm,
    counter: u64,
}

impl MockSs {
    fn new(pass: &[u8], salt: &[u8]) -> Self {
        let h = Hkdf::<Sha1>::new(Some(salt), pass);
        let mut subkey = [0u8; 32];
        h.expand(b"ss-subkey", &mut subkey).unwrap();
        Self { c: Aes256Gcm::new_from_slice(&subkey).unwrap(), counter: 0 }
    }
    fn open(&mut self, ct: &[u8]) -> Vec<u8> {
        let (body, tag) = ct.split_at(ct.len() - 16);
        let mut buf = body.to_vec();
        let mut n = [0u8; 12];
        n[4..12].copy_from_slice(&self.counter.to_be_bytes());
        self.c.decrypt_in_place_detached(Nonce::from_slice(&n), b"", &mut buf, tag.into()).expect("mock open");
        self.counter += 1;
        buf
    }
    fn seal(&mut self, pt: &[u8]) -> Vec<u8> {
        let mut buf = pt.to_vec();
        let mut n = [0u8; 12];
        n[4..12].copy_from_slice(&self.counter.to_be_bytes());
        let tag = self.c.encrypt_in_place_detached(Nonce::from_slice(&n), b"", &mut buf).expect("mock seal");
        self.counter += 1;
        buf.extend_from_slice(tag.as_slice());
        buf
    }
}

async fn mock_ss(l: TcpListener) {
    loop {
        let (mut s, _) = match l.accept().await {
            Ok(x) => x,
            Err(_) => return,
        };
        tokio::spawn(async move {
            let mut salt = [0u8; 32];
            if s.read_exact(&mut salt).await.is_err() {
                return;
            }
            let mut srv = MockSs::new(b"hunter2", &salt);
            // first chunk = target header; skip it
            let mut lb = [0u8; 18];
            if s.read_exact(&mut lb).await.is_err() {
                return;
            }
            let need = {
                let d = srv.open(&lb);
                u16::from_be_bytes([d[0], d[1]])
            };
            let mut payload = vec![0u8; need as usize + 16];
            if s.read_exact(&mut payload).await.is_err() {
                return;
            }
            let _ = srv.open(&payload);
            // mock reply cipher reuses the same subkey (client dec expects it)
            let mut reply = MockSs::new(b"hunter2", &salt);
            loop {
                let mut lb = [0u8; 18];
                if s.read_exact(&mut lb).await.is_err() {
                    return;
                }
                let need = {
                    let d = srv.open(&lb);
                    u16::from_be_bytes([d[0], d[1]])
                };
                let mut payload = vec![0u8; need as usize + 16];
                if s.read_exact(&mut payload).await.is_err() {
                    return;
                }
                let data = srv.open(&payload);
                let mut frame = reply.seal(&(data.len() as u16).to_be_bytes());
                frame.extend(reply.seal(&data));
                if s.write_all(&frame).await.is_err() {
                    return;
                }
            }
        });
    }
}

/* -------------------------------- tests --------------------------------- */

#[tokio::test]
async fn vless_plain_roundtrip() {
    let l = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = l.local_addr().unwrap().port();
    tokio::spawn(mock_vless(l));
    let mut st = proto::dial(&srv("vless", port), &Addr::new("echo.test".into(), 80)).await.unwrap();
    roundtrip(&mut *st).await;
}

#[tokio::test]
async fn vless_ws_roundtrip() {
    let l = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = l.local_addr().unwrap().port();
    tokio::spawn(mock_vless_ws(l));
    let mut s = srv("vless", port);
    s.net = "ws".into();
    s.ws_path = "/ws".into();
    let dst = Addr::new("echo.test".into(), 80);
    let dial_fut = proto::dial(&s, &dst);
    let mut st = tokio::time::timeout(std::time::Duration::from_secs(5), dial_fut)
        .await
        .expect("dial ws timeout")
        .unwrap();
    eprintln!("WS DIAL OK");
    tokio::time::timeout(std::time::Duration::from_secs(5), st.write_all(b"ping!")).await.expect("ws write timeout").unwrap();
    eprintln!("WS WRITE OK");
    let mut b = [0u8; 5];
    tokio::time::timeout(std::time::Duration::from_secs(5), st.read_exact(&mut b)).await.expect("ws read timeout").unwrap();
    eprintln!("WS READ OK: {}", String::from_utf8_lossy(&b));
    roundtrip(&mut *st).await;
}

#[tokio::test]
async fn trojan_tls_roundtrip() {
    let l = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = l.local_addr().unwrap().port();
    tokio::spawn(mock_trojan_tls(l));
    let mut st = proto::dial(&srv_insecure("trojan", port, "mock.tj"), &Addr::new("echo.test".into(), 80)).await.unwrap();
    roundtrip(&mut *st).await;
}

#[tokio::test]
async fn ss_aead_roundtrip() {
    let l = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = l.local_addr().unwrap().port();
    tokio::spawn(mock_ss(l));
    let mut st = proto::dial(&srv("ss", port), &Addr::new("echo.test".into(), 80)).await.unwrap();
    roundtrip(&mut *st).await;
    // multi-block (> 0x3F00 chunk size)
    let big: Vec<u8> = (0..100_000u32).map(|i| (i % 251) as u8).collect();
    st.write_all(&big).await.unwrap();
    let mut got = vec![0u8; big.len()];
    tokio::time::timeout(std::time::Duration::from_secs(10), st.read_exact(&mut got))
        .await
        .expect("big timeout")
        .unwrap();
    assert_eq!(got, big);
}
