//! Connection engine: tokio runtime thread, local SOCKS5/HTTP proxy listener,
//! failover across candidates, server fetching + TCP ping, IP check.

use crate::proto::{self, Addr};
use crate::uri::{self, Server};
use std::io;
use std::sync::atomic::{AtomicBool, AtomicU16, AtomicU64, Ordering};
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
    pub ip_cc: Option<String>,
    pub last_error: Option<String>,
}

impl Default for Status {
    fn default() -> Self {
        Self { phase: Phase::Idle, active: None, since: None, latency_ms: None, ip: None, ip_cc: None, last_error: None }
    }
}

pub struct Shared {
    pub status: Mutex<Status>,
    pub servers: Mutex<Vec<Server>>,
    pub pings: Mutex<Vec<u32>>, // parallel to servers, u32::MAX = fail
    pub pinging: AtomicBool,
    pub fetching: AtomicBool,
    pub proxy_port: AtomicU16,
    pub hwnd: Mutex<Option<isize>>, // main window (Win32) for repaint pings
    pub up: AtomicU64,   // bytes sent through the tunnel this session
    pub down: AtomicU64, // bytes received through the tunnel this session
    pub servers_gen: AtomicU64, // bumped whenever the server list is replaced
    // v1.6.11: honest ping progress for the UI (pingBatch reads these)
    pub ping_done: AtomicBool,     // full TCP-ping pass finished for the current list
    pub ping_measured: AtomicU64,  // results final so far (ok or fail)
    pub ping_total: AtomicU64,     // servers in the pass
    // v1.6.11: bumped on Disconnect/new Connect to cancel in-flight connect attempts
    pub connect_gen: AtomicU64,
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
            hwnd: Mutex::new(None),
            up: AtomicU64::new(0),
            down: AtomicU64::new(0),
            servers_gen: AtomicU64::new(0),
            ping_done: AtomicBool::new(false),
            ping_measured: AtomicU64::new(0),
            ping_total: AtomicU64::new(0),
            connect_gen: AtomicU64::new(0),
        })
    }
    pub fn repaint(&self) {
        if let Some(h) = self.hwnd.lock().unwrap().as_ref() {
            crate::sysproxy::post_repaint(*h);
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

    /// Fire-and-forget stop for app exit: never blocks the UI thread on join.
    pub fn abandon(&mut self) {
        self.stop_flag.store(true, Ordering::SeqCst);
        let _ = self.tx.send(Cmd::Disconnect);
        self.handle.take();
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
        let shared_l = shared.clone();
        let uplink = uplink.clone();
        let stop_l = stop.clone();
        tokio::spawn(async move {
            loop {
                match listener.accept().await {
                    Ok((sock, _)) => {
                        let uplink = uplink.clone();
                        let shared = shared_l.clone();
                        tokio::spawn(async move {
                            let _ = handle_local(sock, uplink, shared).await;
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
                // v1.6.11: run OFF the command loop — probing up to 24 candidates
                // must never freeze Disconnect/Connect handling. connect_gen marks
                // this attempt; a later Disconnect/Connect invalidates it.
                let my_gen = shared.connect_gen.fetch_add(1, Ordering::SeqCst) + 1;
                let shared2 = shared.clone();
                let uplink2 = uplink.clone();
                tokio::spawn(async move {
                    connect_with_failover(&shared2, &uplink2, server, my_gen).await;
                });
            }
            Cmd::Disconnect => {
                shared.connect_gen.fetch_add(1, Ordering::SeqCst); // cancel in-flight attempt
                *uplink.lock().unwrap() = None;
                let mut st = shared.status();
                st.phase = Phase::Idle;
                st.active = None;
                st.since = None;
                st.ip = None;
                st.ip_cc = None;
                st.latency_ms = None;
                shared.set_status(st);
                crate::sysproxy::disable();
            }
            Cmd::FetchSources { urls, custom } => {
                // v1.6.10: run OFF the command loop — subscription fetching can
                // crawl (blocked/slow URLs); Connect/Disconnect must stay live.
                let shared2 = shared.clone();
                tokio::spawn(async move {
                    fetch_sources(&shared2, &urls, custom).await;
                });
            }
            Cmd::PingAll => {
                let shared2 = shared.clone();
                tokio::spawn(async move {
                    ping_all(&shared2).await;
                });
            }
        }
    }
}

fn build_candidates(servers: &[Server], pings: &[u32], selected: &Server, n: usize) -> Vec<Server> {
    let mut out = vec![selected.clone()];
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    seen.insert(format!("{}|{}|{}", selected.proto, selected.host, selected.port));
    // measured-ok fastest first, then unmeasured/failed endpoints (a failed TCP
    // ping does not always mean a dead node — DNS or DPI can interfere), then
    // dedupe by endpoint so one working host is not probed five times.
    let mut idx: Vec<usize> = (0..servers.len())
        .filter(|&i| servers[i].proto != "vmess")
        .collect();
    idx.sort_by_key(|&i| pings.get(i).copied().unwrap_or(u32::MAX));
    for i in idx {
        if out.len() >= n {
            break;
        }
        let s = &servers[i];
        let k = format!("{}|{}|{}", s.proto, s.host, s.port);
        if seen.insert(k) {
            out.push(s.clone());
        }
    }
    out
}

async fn connect_with_failover(
    shared: &Arc<Shared>,
    uplink: &Arc<Mutex<Option<Arc<Server>>>>,
    selected: Server,
    gen: u64,
) {
    let servers = shared.servers.lock().unwrap().clone();
    let pings = shared.pings.lock().unwrap().clone();
    // v1.6.11: 24 candidates (was 6) — public lists are mostly dead nodes, so
    // breadth matters. Selected server always gets the first attempt.
    let candidates = build_candidates(&servers, &pings, &selected, 24);

    let mut st = shared.status();
    st.phase = Phase::Connecting;
    st.active = Some(selected.clone());
    st.last_error = None;
    shared.set_status(st);

    // probe: connect through each candidate to Cloudflare's connectivity check (port 80).
    // api.ipify.org rate-limits (429) and many proxy exits 400 it; generate_204 is stable
    // and reachable from Iran.
    let probe = Addr::new("cp.cloudflare.com".into(), 80);
    let mut tried = 0usize;
    let mut last_err = String::from("no candidates");
    // v1.6.11: probe in parallel waves of 6 — first success wins. Serial probing
    // of dead nodes wasted up to a minute; waves cut that to seconds.
    for wave in candidates.chunks(6) {
        if gen != shared.connect_gen.load(Ordering::SeqCst) {
            return; // cancelled by Disconnect or a newer Connect
        }
        let mut futs = Vec::new();
        for cand in wave {
            let c = cand.clone();
            let p = probe.clone();
            futs.push(tokio::spawn(async move {
                let r = try_probe(&c, &p).await;
                (c, r)
            }));
        }
        let mut won: Option<(Server, String)> = None;
        for f in futs {
            if let Ok((cand, r)) = f.await {
                match r {
                    Ok(ms) => {
                        if won.is_none() {
                            won = Some((cand, ms));
                        }
                    }
                    Err(e) => last_err = format!("{}: {}", cand.host, e),
                }
            }
        }
        tried += wave.len();
        if let Some((cand, ip)) = won {
            if gen != shared.connect_gen.load(Ordering::SeqCst) {
                return; // cancelled while probing
            }
            *uplink.lock().unwrap() = Some(Arc::new(cand.clone()));
            // "ok|<ms>" from try_probe — store the measured latency, no fake IP
            let ms = ip.split('|').nth(1).and_then(|v| v.parse::<u32>().ok());
            shared.up.store(0, Ordering::SeqCst);
            shared.down.store(0, Ordering::SeqCst);
            let mut st = shared.status();
            st.phase = Phase::Connected;
            st.active = Some(cand.clone());
            st.since = Some(std::time::SystemTime::now());
            st.ip = None;
            st.ip_cc = None;
            st.latency_ms = ms;
            st.last_error = None;
            shared.set_status(st);
            crate::sysproxy::enable(shared.proxy_port.load(Ordering::SeqCst));
            // real IP check through the tunnel (best effort, never blocks)
            {
                let shared = shared.clone();
                let up = uplink.clone();
                tokio::spawn(async move {
                    let srv_opt = up.lock().unwrap().clone();
                    if let Some(srv) = srv_opt {
                        ip_check_through_tunnel(&shared, &srv).await;
                    }
                });
            }
            return;
        }
        let mut st = shared.status();
        st.last_error = Some(last_err.clone());
        shared.set_status(st);
    }
    let mut st = shared.status();
    st.phase = Phase::Error(format!(
        "{} of {} candidates failed — last: {}",
        tried, candidates.len(), last_err
    ));
    shared.set_status(st);
}

/// v1.6.11: hard 6s outer bound per candidate (dial 8s inside proto::dial would
/// let one dead node stall a whole wave). Every probe attempt is bounded.
async fn try_probe(srv: &Server, dst: &Addr) -> io::Result<String> {
    match tokio::time::timeout(Duration::from_secs(6), try_probe_inner(srv, dst)).await {
        Ok(r) => r,
        Err(_) => Err(io::Error::new(io::ErrorKind::TimedOut, "probe timeout")),
    }
}

async fn try_probe_inner(srv: &Server, dst: &Addr) -> io::Result<String> {
    let t0 = std::time::Instant::now();
    let mut s = proto::dial(srv, dst).await?;
    let req = format!(
        "GET /generate_204 HTTP/1.1\r\nHost: {}\r\nUser-Agent: Mozilla/5.0\r\nConnection: close\r\n\r\n",
        dst.host
    );
    s.write_all(req.as_bytes()).await?;
    let mut buf = Vec::new();
    let mut tmp = [0u8; 2048];
    // v1.6.11: 6s outer bound covers the dial too (see try_probe callers);
    // keep the read deadline below it so the error names the real cause.
    let deadline = tokio::time::Instant::now() + Duration::from_millis(5000);
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
                    if let Some(pos) = text.find("\r\n") {
                        let status = &text[..pos];
                        if status.starts_with("HTTP/") {
                            if status.contains(" 204") || status.contains(" 200")
                                || status.contains(" 301") || status.contains(" 302") {
                                let ms = t0.elapsed().as_millis() as u32;
                                return Ok(format!("ok|{}", ms));
                            }
                            return Err(io::Error::new(io::ErrorKind::InvalidData, "bad probe status"));
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
    let head = text.split("\r\n").next().unwrap_or("");
    if head.starts_with("HTTP/") && (head.contains(" 204") || head.contains(" 200")) {
        let ms = t0.elapsed().as_millis() as u32;
        return Ok(format!("ok|{}", ms));
    }
    Err(io::Error::new(io::ErrorKind::InvalidData, "bad probe response"))
}

async fn handle_local(
    mut sock: tokio::net::TcpStream,
    uplink: Arc<Mutex<Option<Arc<Server>>>>,
    shared: Arc<Shared>,
) -> io::Result<()> {
    sock.set_nodelay(true).ok();
    let mut first = [0u8; 1];
    let n = sock.read(&mut first).await?;
    if n == 0 {
        return Ok(());
    }
    if first[0] == 0x05 {
        socks5_flow(sock, first[0], uplink, shared).await
    } else {
        http_connect_flow(sock, first[0], uplink, shared).await
    }
}

async fn socks5_flow(
    mut sock: tokio::net::TcpStream,
    ver: u8,
    uplink: Arc<Mutex<Option<Arc<Server>>>>,
    shared: Arc<Shared>,
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

    relay(sock, host, port, uplink, shared).await
}

async fn http_connect_flow(
    mut sock: tokio::net::TcpStream,
    first: u8,
    uplink: Arc<Mutex<Option<Arc<Server>>>>,
    shared: Arc<Shared>,
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
    relay(sock, host, port, uplink, shared).await
}

async fn relay(
    sock: tokio::net::TcpStream,
    host: String,
    port: u16,
    uplink: Arc<Mutex<Option<Arc<Server>>>>,
    shared: Arc<Shared>,
) -> io::Result<()> {
    let srv = { uplink.lock().unwrap().clone() };
    let Some(srv) = srv else {
        return Err(io::Error::new(io::ErrorKind::NotConnected, "not connected"));
    };
    let dst = Addr::new(host, port);
    let upstream = proto::dial(&srv, &dst).await?;
    relay_counted(sock, upstream, shared).await;
    Ok(())
}

/// Bidirectional relay with real byte accounting (per-direction totals).
/// shared.up += local→remote bytes, shared.down += remote→local bytes.
async fn relay_counted(sock: tokio::net::TcpStream, upstream: proto::BoxStream, shared: Arc<Shared>) {
    let (mut s_read, mut s_write) = tokio::io::split(sock);
    let (mut u_read, mut u_write) = tokio::io::split(upstream);
    let a = async {
        let mut buf = [0u8; 16 * 1024];
        loop {
            match s_read.read(&mut buf).await {
                Ok(0) => break,
                Ok(n) => {
                    shared.up.fetch_add(n as u64, Ordering::Relaxed);
                    if u_write.write_all(&buf[..n]).await.is_err() {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
        let _ = u_write.flush().await;
    };
    let b = async {
        let mut buf = [0u8; 16 * 1024];
        loop {
            match u_read.read(&mut buf).await {
                Ok(0) => break,
                Ok(n) => {
                    shared.down.fetch_add(n as u64, Ordering::Relaxed);
                    if s_write.write_all(&buf[..n]).await.is_err() {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
        let _ = s_write.flush().await;
    };
    tokio::join!(a, b);
}

/// Real public-IP check THROUGH the tunnel: cloudflare /cdn-cgi/trace over TLS
/// dialed via the connected server. Best-effort; stores ip + country code.
async fn ip_check_through_tunnel(shared: &Arc<Shared>, srv: &Server) {
    let host = "www.cloudflare.com";
    let dst = Addr::new(host.to_string(), 443);
    let Ok(tcp) = proto::dial(srv, &dst).await else { return };
    let connector = crate::tls::connector_verified();
    let Ok(mut tls) = connector.connect(crate::tls::sni_name(host), tcp).await else { return };
    let req = format!(
        "GET /cdn-cgi/trace HTTP/1.1\r\nHost: {}\r\nUser-Agent: Mozilla/5.0\r\nConnection: close\r\n\r\n",
        host
    );
    if tls.write_all(req.as_bytes()).await.is_err() {
        return;
    }
    let mut raw = Vec::new();
    let mut buf = [0u8; 4096];
    let deadline = tokio::time::Instant::now() + Duration::from_millis(8000);
    loop {
        let left = deadline.saturating_duration_since(tokio::time::Instant::now());
        if left.is_zero() {
            break;
        }
        match tokio::time::timeout(left, tls.read(&mut buf)).await {
            Ok(Ok(0)) | Err(_) => break,
            Ok(Ok(n)) => {
                raw.extend_from_slice(&buf[..n]);
                if raw.len() > 64 * 1024 {
                    break;
                }
            }
            Ok(Err(_)) => break,
        }
    }
    let text = String::from_utf8_lossy(&raw);
    let mut ip = None;
    let mut cc = None;
    for line in text.split('\n') {
        if let Some(v) = line.strip_prefix("ip=") {
            ip = Some(v.trim().to_string());
        } else if let Some(v) = line.strip_prefix("loc=") {
            cc = Some(v.trim().to_string());
        }
    }
    if let Some(ip) = ip {
        let mut st = shared.status();
        // only report if still connected to the same server
        if st.phase == Phase::Connected {
            st.ip = Some(ip);
            st.ip_cc = cc;
            shared.set_status(st);
        }
    }
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
    let parsed: Vec<Server> = uri::parse_lines(&text_all);
    let mut servers: Vec<Server> = parsed
        .iter()
        .filter(|s| {
            (s.proto == "vless"
                || s.proto == "trojan"
                || (s.proto == "ss" && proto::method_supported(&s.method)))
                && transport_supported(s)
        })
        .cloned()
        .collect();
    let dropped = parsed.len() - servers.len();
    if dropped > 0 {
        crate::sysproxy::log_line(&format!(
            "fetch: dropped {} configs with unsupported transports (reality/grpc/kcp/...)",
            dropped
        ));
    }
    servers = uri::dedupe(servers);
    // cap list size for UI
    servers.truncate(400);
    let n = servers.len();
    *shared.pings.lock().unwrap() = vec![u32::MAX; n];
    *shared.servers.lock().unwrap() = servers;
    shared.servers_gen.fetch_add(1, Ordering::SeqCst);
    shared.fetching.store(false, Ordering::SeqCst);
    shared.repaint();
    let _ = n;
    // kick a ping round
    ping_all(shared).await;
}

/// v1.6.11: honest list — keep only transports this build can actually dial.
/// REALITY/gRPC/KCP/QUIC/H2 configs TCP-ping fine but can never connect; they
/// used to rank as "fast" candidates and guarantee connect failures.
fn transport_supported(s: &Server) -> bool {
    match s.proto.as_str() {
        "vless" => {
            matches!(s.security.as_str(), "none" | "tls")
                && matches!(s.net.as_str(), "tcp" | "ws")
        }
        "trojan" => {
            matches!(s.security.as_str(), "tls" | "none")
                && matches!(s.net.as_str(), "tcp" | "ws")
        }
        "ss" => true,
        _ => false,
    }
}

async fn fetch_url(url: &str) -> io::Result<String> {
    // overall cap per source: a blocked/hanging URL must never stall the UI
    match tokio::time::timeout(Duration::from_secs(8), crate::fetch::http_get(url, 8 * 1024 * 1024)).await {
        Ok(r) => r,
        Err(_) => Err(io::Error::new(io::ErrorKind::TimedOut, "source fetch timeout")),
    }
}

async fn ping_all(shared: &Arc<Shared>) {
    if shared.pinging.swap(true, Ordering::SeqCst) {
        return;
    }
    let servers = shared.servers.lock().unwrap().clone();
    let gen_at_start = shared.servers_gen.load(Ordering::SeqCst);
    shared.ping_done.store(false, Ordering::SeqCst);
    shared.ping_total.store(servers.len() as u64, Ordering::SeqCst);
    shared.ping_measured.store(0, Ordering::SeqCst);
    {
        let mut p = shared.pings.lock().unwrap();
        p.clear();
        p.resize(servers.len(), u32::MAX);
    }

    // v1.6.11: results merge INCREMENTALLY (per chunk) so the UI sees pings
    // trickle in while the pass is running — before, every result landed only
    // after the whole multi-pass loop, and the UI marked everything dead.
    // 3s timeout (was 1.5s) for high-latency / DPI-throttled networks.
    const CHUNK: usize = 32;
    const TIMEOUT_MS: u64 = 3000;
    for chunk in servers.chunks(CHUNK) {
        if shared.servers_gen.load(Ordering::SeqCst) != gen_at_start {
            break; // list was replaced; the new fetch starts a fresh pass
        }
        let base = chunk_start(&servers, chunk);
        let mut futs = Vec::new();
        for s in chunk {
            let host = s.host.clone();
            let port = s.port;
            futs.push(tokio::spawn(async move {
                let t0 = std::time::Instant::now();
                match proto::tcp_connect(&host, port, TIMEOUT_MS).await {
                    Ok(_) => Some(t0.elapsed().as_millis() as u32),
                    Err(_) => None,
                }
            }));
        }
        for (j, f) in futs.into_iter().enumerate() {
            let idx = base + j;
            if idx >= servers.len() {
                continue;
            }
            let ms: Option<u32> = f.await.ok().flatten();
            let mut p = shared.pings.lock().unwrap();
            if idx < p.len() {
                p[idx] = ms.unwrap_or(u32::MAX);
            }
        }
        shared.ping_measured.fetch_add(chunk.len() as u64, Ordering::SeqCst);
        shared.repaint();
    }
    if shared.servers_gen.load(Ordering::SeqCst) == gen_at_start {
        shared.ping_done.store(true, Ordering::SeqCst);
    }
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
