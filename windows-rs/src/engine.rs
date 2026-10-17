//! Connection engine: tokio runtime thread, local SOCKS5/HTTP proxy listener,
//! failover across candidates, server fetching + TCP ping, IP check.

use crate::proto::{self, Addr};
use crate::uri::{self, Server};
use std::io;
use std::sync::atomic::{AtomicBool, AtomicU16, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub enum Phase {
    Idle,
    Connecting,
    Connected,
    Disconnecting,
    Error(String),
}

#[derive(Clone)]
pub struct Status {
    pub phase: Phase,
    pub active: Option<Server>,
    pub since: Option<std::time::SystemTime>,
    pub latency_ms: Option<u32>,
    pub ip: Option<String>,
    pub last_error: Option<String>,
}

impl Default for Status {
    fn default() -> Self {
        Self { phase: Phase::Idle, active: None, since: None, latency_ms: None, ip: None, last_error: None }
    }
}

pub struct Shared {
    pub status: Mutex<Status>,
    pub servers: Mutex<Vec<Server>>,
    pub pings: Mutex<Vec<u32>>, // parallel to servers, u32::MAX = fail
    pub pinging: AtomicBool,
    pub fetching: AtomicBool,
    pub proxy_port: AtomicU16,
    pub ctx: Mutex<Option<egui::Context>>,
}

impl Shared {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            status: Mutex::new(Status::default()),
            servers: Mutex::new(Vec::new()),
            pings: Mutex::new(Vec::new()),
            pinging: AtomicBool::new(false),
            fetching: AtomicBool::new(false),
            proxy_port: AtomicU16::new(0),
            ctx: Mutex::new(None),
        })
    }
    pub fn repaint(&self) {
        if let Some(c) = self.ctx.lock().unwrap().as_ref() {
            c.request_repaint();
        }
    }
    pub fn set_status(&self, s: Status) {
        *self.status.lock().unwrap() = s;
        self.repaint();
    }
    pub fn status(&self) -> Status {
        self.status.lock().unwrap().clone()
    }
}

pub enum Cmd {
    Connect { server: Server },
    Disconnect,
    FetchSources { urls: Vec<String>, custom: Option<String> },
    PingAll,
}

pub struct Engine {
    pub tx: std::sync::mpsc::Sender<Cmd>,
    pub shared: Arc<Shared>,
    stop_flag: Arc<AtomicBool>,
    handle: Option<std::thread::JoinHandle<()>>,
}

impl Engine {
    pub fn start(shared: Arc<Shared>) -> Self {
        let (tx, rx) = std::sync::mpsc::channel::<Cmd>();
        let stop = Arc::new(AtomicBool::new(false));
        let stop2 = stop.clone();
        let shared2 = shared.clone();
        let handle = std::thread::Builder::new()
            .name("veil-engine".into())
            .spawn(move || {
                let rt = tokio::runtime::Builder::new_multi_thread()
                    .enable_all()
                    .worker_threads(4)
                    .build()
                    .expect("tokio runtime");
                rt.block_on(async move {
                    engine_loop(shared2, rx, stop2).await;
                });
            })
            .expect("spawn engine thread");
        Self { tx, shared, stop_flag: stop, handle: Some(handle) }
    }

    pub fn shutdown(&mut self) {
        self.stop_flag.store(true, Ordering::SeqCst);
        let _ = self.tx.send(Cmd::Disconnect);
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}

impl Drop for Engine {
    fn drop(&mut self) {
        self.shutdown();
    }
}

static SOURCES: &[&str] = &[
    "https://raw.githubusercontent.com/Epodonios/v2ray-configs/main/All_Providers_Configs.txt",
    "https://cdn.jsdelivr.net/gh/Epodonios/v2ray-configs@main/All_Providers_Configs.txt",
    "https://raw.githubusercontent.com/mahdibland/ShadowsocksAggregator/master/Eternity",
    "https://cdn.jsdelivr.net/gh/mahdibland/ShadowsocksAggregator@master/Eternity",
    "https://raw.githubusercontent.com/barry-far/V2ray-Configs/main/Splitted-By-Protocol/vless.txt",
    "https://cdn.jsdelivr.net/gh/barry-far/V2ray-Configs@main/Splitted-By-Protocol/vless.txt",
    "https://raw.githubusercontent.com/barry-far/V2ray-Configs/main/Splitted-By-Protocol/trojan.txt",
];

async fn engine_loop(shared: Arc<Shared>, rx: std::sync::mpsc::Receiver<Cmd>, stop: Arc<AtomicBool>) {
    // local proxy listener
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("bind local proxy");
    let port = listener.local_addr().map(|a| a.port()).unwrap_or(0);
    shared.proxy_port.store(port, Ordering::SeqCst);
    let uplink: Arc<Mutex<Option<Arc<Server>>>> = Arc::new(Mutex::new(None));

    // listener task
    {
        let shared = shared.clone();
        let uplink = uplink.clone();
        let stop_l = stop.clone();
        tokio::spawn(async move {
            loop {
                match listener.accept().await {
                    Ok((sock, _)) => {
                        let uplink = uplink.clone();
                        let shared = shared.clone();
                        tokio::spawn(async move {
                            let _ = handle_local(sock, uplink).await;
                        });
                    }
                    Err(_) => tokio::time::sleep(Duration::from_millis(50)).await,
                }
                if stop_l.load(Ordering::SeqCst) {
                    break;
                }
            }
        });
    }

    while let Ok(cmd) = rx.recv() {
        if stop.load(Ordering::SeqCst) {
            break;
        }
        match cmd {
            Cmd::Connect { server } => {
                connect_with_failover(&shared, &uplink, server).await;
            }
            Cmd::Disconnect => {
                *uplink.lock().unwrap() = None;
                let mut st = shared.status();
                st.phase = Phase::Idle;
                st.active = None;
                st.since = None;
                st.ip = None;
                st.latency_ms = None;
                shared.set_status(st);
                crate::sysproxy::disable();
            }
            Cmd::FetchSources { urls, custom } => {
                fetch_sources(&shared, &urls, custom).await;
            }
            Cmd::PingAll => {
                ping_all(&shared).await;
            }
        }
    }
}

fn build_candidates(servers: &[Server], pings: &[u32], selected: &Server, n: usize) -> Vec<Server> {
    let mut out = vec![selected.clone()];
    let mut idx: Vec<usize> = (0..servers.len()).collect();
    idx.sort_by_key(|&i| pings.get(i).copied().unwrap_or(u32::MAX));
    for i in idx {
        if out.len() >= n {
            break;
        }
        let s = &servers[i];
        if s.key() != selected.key() && s.proto != "vmess" {
            out.push(s.clone());
        }
    }
    out
}

async fn connect_with_failover(shared: &Arc<Shared>, uplink: &Arc<Mutex<Option<Arc<Server>>>>, selected: Server) {
    let servers = shared.servers.lock().unwrap().clone();
    let pings = shared.pings.lock().unwrap().clone();
    let candidates = build_candidates(&servers, &pings, &selected, 6);

    let mut st = shared.status();
    st.phase = Phase::Connecting;
    st.active = Some(selected.clone());
    st.last_error = None;
    shared.set_status(st);

    // probe: connect through each candidate to ipify over plain HTTP (port 80)
    let probe = Addr::new("api.ipify.org".into(), 80);
    for cand in &candidates {
        match try_probe(cand, &probe).await {
            Ok(ip) => {
                *uplink.lock().unwrap() = Some(Arc::new(cand.clone()));
                let mut st = shared.status();
                st.phase = Phase::Connected;
                st.active = Some(cand.clone());
                st.since = Some(std::time::SystemTime::now());
                st.ip = Some(ip);
                // latency: measured in try_probe via connect time
                st.latency_ms = None;
                shared.set_status(st);
                crate::sysproxy::enable(shared.proxy_port.load(Ordering::SeqCst));
                return;
            }
            Err(e) => {
                let mut st = shared.status();
                st.last_error = Some(format!("{}: {}", cand.host, e));
                shared.set_status(st);
            }
        }
    }
    let mut st = shared.status();
    st.phase = Phase::Error("all candidates failed".into());
    shared.set_status(st);
}

async fn try_probe(srv: &Server, dst: &Addr) -> io::Result<String> {
    let t0 = std::time::Instant::now();
    let mut s = proto::dial(srv, dst).await?;
    let req = format!("GET / HTTP/1.1\r\nHost: {}\r\nUser-Agent: veil\r\nConnection: close\r\n\r\n", dst.host);
    s.write_all(req.as_bytes()).await?;
    let mut buf = Vec::new();
    let mut tmp = [0u8; 2048];
    let deadline = tokio::time::Instant::now() + Duration::from_millis(7000);
    loop {
        let left = deadline.saturating_duration_since(tokio::time::Instant::now());
        if left.is_zero() {
            return Err(io::Error::new(io::ErrorKind::TimedOut, "probe timeout"));
        }
        match tokio::time::timeout(left, s.read(&mut tmp)).await {
            Ok(Ok(0)) => break,
            Ok(Ok(n)) => {
                buf.extend_from_slice(&tmp[..n]);
                if let Ok(text) = std::str::from_utf8(&buf) {
                    if let Some(pos) = text.find("\r\n\r\n") {
                        let ip = text[pos + 4..].trim().to_string();
                        if !ip.is_empty() {
                            return Ok(ip);
                        }
                    }
                }
                if buf.len() > 8192 {
                    break;
                }
            }
            Ok(Err(e)) => return Err(e),
            Err(_) => return Err(io::Error::new(io::ErrorKind::TimedOut, "probe timeout")),
        }
    }
    let text = String::from_utf8_lossy(&buf);
    let ip = text
        .split("\r\n\r\n")
        .nth(1)
        .map(|b| b.trim().to_string())
        .unwrap_or_default();
    if ip.is_empty() || !ip.chars().next().map(|c| c.is_ascii_digit() || c == ':').unwrap_or(false) {
        let _ = t0.elapsed();
        return Err(io::Error::new(io::ErrorKind::InvalidData, "bad probe response"));
    }
    Ok(ip)
}

async fn handle_local(
    mut sock: tokio::net::TcpStream,
    uplink: Arc<Mutex<Option<Arc<Server>>>>,
) -> io::Result<()> {
    sock.set_nodelay(true).ok();
    let mut first = [0u8; 1];
    let n = sock.read(&mut first).await?;
    if n == 0 {
        return Ok(());
    }
    if first[0] == 0x05 {
        socks5_flow(sock, first[0], uplink).await
    } else {
        http_connect_flow(sock, first[0], uplink).await
    }
}

async fn socks5_flow(
    mut sock: tokio::net::TcpStream,
    ver: u8,
    uplink: Arc<Mutex<Option<Arc<Server>>>>,
) -> io::Result<()> {
    // greeting: first byte (version) already consumed by caller
    let mut nmethods = [0u8; 1];
    sock.read_exact(&mut nmethods).await?;
    let mut methods = vec![0u8; nmethods[0] as usize];
    sock.read_exact(&mut methods).await?;
    sock.write_all(&[ver, 0x00]).await?; // no auth

    // request
    let mut req = [0u8; 4];
    sock.read_exact(&mut req).await?;
    let cmd = req[1];
    let atyp = req[3];
    let host = match atyp {
        0x01 => {
            let mut b = [0u8; 4];
            sock.read_exact(&mut b).await?;
            std::net::Ipv4Addr::from(b).to_string()
        }
        0x03 => {
            let mut l = [0u8; 1];
            sock.read_exact(&mut l).await?;
            let mut b = vec![0u8; l[0] as usize];
            sock.read_exact(&mut b).await?;
            String::from_utf8_lossy(&b).into_owned()
        }
        0x04 => {
            let mut b = [0u8; 16];
            sock.read_exact(&mut b).await?;
            std::net::Ipv6Addr::from(b).to_string()
        }
        _ => return Err(io::Error::new(io::ErrorKind::InvalidData, "bad atyp")),
    };
    let mut pb = [0u8; 2];
    sock.read_exact(&mut pb).await?;
    let port = u16::from_be_bytes(pb);

    if cmd != 0x01 {
        sock.write_all(&[ver, 0x07, 0x00, 0x01, 0, 0, 0, 0, 0, 0]).await?;
        return Ok(());
    }
    // success reply
    sock.write_all(&[ver, 0x00, 0x00, 0x01, 0, 0, 0, 0, 0, 0]).await?;

    relay(sock, host, port, uplink).await
}

async fn http_connect_flow(
    mut sock: tokio::net::TcpStream,
    first: u8,
    uplink: Arc<Mutex<Option<Arc<Server>>>>,
) -> io::Result<()> {
    // accumulate request head
    let mut head = vec![first];
    let mut buf = [0u8; 1024];
    loop {
        if head.windows(4).any(|w| w == b"\r\n\r\n") {
            break;
        }
        let n = sock.read(&mut buf).await?;
        if n == 0 {
            return Ok(());
        }
        head.extend_from_slice(&buf[..n]);
        if head.len() > 16 * 1024 {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "http head too large"));
        }
    }
    let text = String::from_utf8_lossy(&head);
    let line1 = text.lines().next().unwrap_or("");
    let mut parts = line1.split_whitespace();
    let method = parts.next().unwrap_or("").to_ascii_uppercase();
    let target = parts.next().unwrap_or("").to_string();
    if method != "CONNECT" {
        // absolute-URI plain HTTP is not supported in v1 (browsers use CONNECT for everything relevant via system proxy)
        let body = "HTTP/1.1 405 Method Not Allowed\r\nContent-Length: 0\r\n\r\n";
        sock.write_all(body.as_bytes()).await?;
        return Ok(());
    }
    let (host, port) = match target.rsplit_once(':') {
        Some((h, p)) => (h.trim_matches(|c| c == '[' || c == ']').to_string(), p.parse().unwrap_or(80u16)),
        None => (target.clone(), 80),
    };
    sock.write_all(b"HTTP/1.1 200 Connection established\r\n\r\n").await?;
    relay(sock, host, port, uplink).await
}

async fn relay(
    mut sock: tokio::net::TcpStream,
    host: String,
    port: u16,
    uplink: Arc<Mutex<Option<Arc<Server>>>>,
) -> io::Result<()> {
    let srv = { uplink.lock().unwrap().clone() };
    let Some(srv) = srv else {
        return Err(io::Error::new(io::ErrorKind::NotConnected, "not connected"));
    };
    let dst = Addr::new(host, port);
    let mut upstream = proto::dial(&srv, &dst).await?;
    let _ = tokio::io::copy_bidirectional(&mut sock, &mut upstream).await;
    let _ = sock.shutdown().await;
    Ok(())
}

async fn fetch_sources(shared: &Arc<Shared>, urls: &[String], custom: Option<String>) {
    shared.fetching.store(true, Ordering::SeqCst);
    shared.repaint();
    let mut text_all = String::new();
    let mut list: Vec<String> = urls.to_vec();
    if list.is_empty() {
        list.extend(SOURCES.iter().map(|s| s.to_string()));
    }
    // custom source first (highest priority)
    if let Some(c) = &custom {
        if !c.trim().is_empty() {
            list.insert(0, c.trim().to_string());
        }
    }
    for url in &list {
        match fetch_url(url).await {
            Ok(body) => {
                text_all.push_str(&body);
                text_all.push('\n');
                if text_all.len() > 6 * 1024 * 1024 {
                    break;
                }
            }
            Err(_) => continue,
        }
    }
    let mut servers: Vec<Server> = uri::parse_lines(&text_all)
        .into_iter()
        .filter(|s| s.proto == "vless" || s.proto == "trojan" || (s.proto == "ss" && proto::method_supported(&s.method)))
        .collect();
    servers = uri::dedupe(servers);
    // cap list size for UI
    servers.truncate(400);
    let n = servers.len();
    *shared.pings.lock().unwrap() = vec![u32::MAX; n];
    *shared.servers.lock().unwrap() = servers;
    shared.fetching.store(false, Ordering::SeqCst);
    shared.repaint();
    let _ = n;
    // kick a ping round
    ping_all(shared).await;
}

async fn fetch_url(url: &str) -> io::Result<String> {
    crate::fetch::http_get(url, 8 * 1024 * 1024).await
}

async fn ping_all(shared: &Arc<Shared>) {
    if shared.pinging.swap(true, Ordering::SeqCst) {
        return;
    }
    let servers = shared.servers.lock().unwrap().clone();
    let mut results = vec![u32::MAX; servers.len()];

    // concurrent batches of 24
    for chunk in servers.chunks(24) {
        let mut futs = Vec::new();
        for (i, s) in chunk.iter().enumerate() {
            let host = s.host.clone();
            let port = s.port;
            futs.push(tokio::spawn(async move {
                let t0 = std::time::Instant::now();
                match proto::tcp_connect(&host, port, 1500).await {
                    Ok(_) => Some(t0.elapsed().as_millis() as u32),
                    Err(_) => None,
                }
            }));
            let _ = i;
        }
        for (j, f) in futs.into_iter().enumerate() {
            let base = chunk_start(&servers, chunk);
            if let Ok(Some(ms)) = f.await {
                if base + j < results.len() {
                    results[base + j] = ms;
                }
            }
        }
    }
    *shared.pings.lock().unwrap() = results;
    shared.pinging.store(false, Ordering::SeqCst);
    shared.repaint();
}

fn chunk_start(all: &[Server], chunk: &[Server]) -> usize {
    if chunk.is_empty() || all.is_empty() {
        return 0;
    }
    let first = &chunk[0];
    all.iter().position(|s| s.key() == first.key()).unwrap_or(0)
}
