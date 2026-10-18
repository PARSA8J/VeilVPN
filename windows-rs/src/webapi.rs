//! Local UI bridge: a tiny HTTP server on 127.0.0.1 that
//!  1) serves the embedded Liquid Glass web UI (shared/index.html + app.js + i18n.js),
//!  2) implements the JSON API the UI already speaks (same contract as the Android bridge),
//!  3) pushes live events (status / servers) to the UI via a long-poll endpoint.
//!
//! Security: loopback-only bind, random port, random per-launch token required on
//! every request (?k=), Host header must be 127.0.0.1 (anti DNS-rebinding).

use crate::engine::{Cmd, Phase, Shared};
use crate::store;
use std::collections::VecDeque;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::atomic::{AtomicBool, AtomicU16, AtomicU64, Ordering};
use std::sync::{mpsc, Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

const INDEX_HTML: &str = include_str!("../../shared/index.html");
const APP_JS: &str = include_str!("../../shared/app.js");
const I18N_JS: &str = include_str!("../../shared/i18n.js");

pub struct WebApi {
    pub shared: Arc<Shared>,
    pub cfg: Arc<Mutex<store::Config>>,
    engine_tx: Mutex<mpsc::Sender<Cmd>>,
    token: String,
    port_cell: AtomicU16,
    events: Mutex<VecDeque<serde_json::Value>>,
    ev_cv: Condvar,
    last_phase_fp: Mutex<String>,
    pushed_gen: AtomicU64,
    auto_done: AtomicBool,
    auto_enabled: AtomicBool,
}

fn http_res(status: &str, ctype: &str, body: Vec<u8>) -> Vec<u8> {
    let head = format!(
        "HTTP/1.1 {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n",
        status,
        ctype,
        body.len()
    );
    let mut out = head.into_bytes();
    out.extend_from_slice(&body);
    out
}

fn json_res(status: &str, v: &serde_json::Value) -> Vec<u8> {
    http_res(status, "application/json", v.to_string().into_bytes())
}

impl WebApi {
    pub fn spawn(
        shared: Arc<Shared>,
        cfg: Arc<Mutex<store::Config>>,
        engine_tx: mpsc::Sender<Cmd>,
    ) -> Arc<Self> {
        let token: String = (0..32).map(|_| format!("{:x}", rand::random::<u8>() % 16)).collect();
        let auto = cfg.lock().unwrap().auto_connect;
        let api = Arc::new(Self {
            shared,
            cfg,
            engine_tx: Mutex::new(engine_tx),
            token,
            port_cell: AtomicU16::new(0),
            events: Mutex::new(VecDeque::new()),
            ev_cv: Condvar::new(),
            last_phase_fp: Mutex::new(String::new()),
            pushed_gen: AtomicU64::new(u64::MAX), // force initial servers push
            auto_done: AtomicBool::new(false),
            auto_enabled: AtomicBool::new(auto),
        });
        let api2 = api.clone();
        std::thread::Builder::new()
            .name("veil-webapi".into())
            .spawn(move || server_loop(api2))
            .expect("spawn webapi thread");
        api
    }

    pub fn port(&self) -> u16 {
        self.port_cell.load(Ordering::SeqCst)
    }

    pub fn token(&self) -> &str {
        &self.token
    }

    fn send_cmd(&self, cmd: Cmd) -> bool {
        self.engine_tx.lock().unwrap().send(cmd).is_ok()
    }

    // ---------------- events ----------------
    fn push_event(&self, ev: serde_json::Value) {
        let mut q = self.events.lock().unwrap();
        if q.len() > 16 {
            q.pop_front();
        }
        q.push_back(ev);
        self.ev_cv.notify_all();
    }

    pub fn push_status_event(&self) {
        let ev = serde_json::json!({ "kind": "status", "state": self.ui_status() });
        self.push_event(ev);
    }

    pub fn push_servers_event(&self) {
        let ev = serde_json::json!({ "kind": "servers", "servers": self.servers_json() });
        self.push_event(ev);
    }

    /// Called on every engine repaint (WM_APP_REPAINT): diff + push events.
    pub fn notify_repaint(self: &Arc<Self>) {
        // status diff
        let st = self.shared.status();
        let fp = format!(
            "{:?}|{}|{}|{}",
            st.phase,
            st.active.as_ref().map(|s| s.key()).unwrap_or_default(),
            st.ip.clone().unwrap_or_default(),
            st.last_error.clone().unwrap_or_default()
        );
        let changed = {
            let mut g = self.last_phase_fp.lock().unwrap();
            let changed = *g != fp;
            *g = fp;
            changed
        };
        if changed {
            self.push_status_event();
        }
        // servers diff
        let gen = self.shared.servers_gen.load(Ordering::SeqCst);
        if self.pushed_gen.swap(gen, Ordering::SeqCst) != gen {
            self.push_servers_event();
            self.maybe_auto_connect();
        }
    }

    // ---------------- status mapping ----------------
    fn ui_state(&self, p: &Phase) -> &'static str {
        match p {
            Phase::Idle => "disconnected",
            Phase::Connecting => "connecting",
            Phase::Connected => "connected",
            Phase::Disconnecting => "disconnecting",
            Phase::Error(_) => "disconnected",
        }
    }

    /// find the "sN" id of a server by key
    fn server_id(&self, key: &str) -> Option<String> {
        let servers = self.shared.servers.lock().unwrap();
        for (i, s) in servers.iter().enumerate() {
            if s.key() == key {
                return Some(format!("s{}", i));
            }
        }
        None
    }

    fn server_json_for(&self, s: &crate::uri::Server) -> serde_json::Value {
        serde_json::json!({
            "id": self.server_id(&s.key()).unwrap_or_default(),
            "cc": if s.cc.is_empty() { "XX".to_string() } else { s.cc.clone() },
            "name": s.remark,
        })
    }

    fn ui_status(&self) -> serde_json::Value {
        let st = self.shared.status();
        let state = self.ui_state(&st.phase);
        let err = match &st.phase {
            Phase::Error(e) => Some(e.clone()),
            _ => st.last_error.clone().filter(|_| state == "disconnected"),
        };
        let server = st.active.as_ref().map(|s| self.server_json_for(s));
        serde_json::json!({
            "state": state,
            "server": server,
            "error": err,
        })
    }

    fn servers_json(&self) -> serde_json::Value {
        let servers = self.shared.servers.lock().unwrap();
        let pings = self.shared.pings.lock().unwrap();
        let st = self.shared.status();
        let active_key = if st.phase == Phase::Connected {
            st.active.as_ref().map(|s| s.key())
        } else {
            None
        };
        let arr: Vec<serde_json::Value> = servers
            .iter()
            .enumerate()
            .map(|(i, s)| {
                let ping = pings.get(i).copied().unwrap_or(u32::MAX);
                let mut o = serde_json::json!({
                    "id": format!("s{}", i),
                    "p": s.proto,
                    "cc": if s.cc.is_empty() { "XX".to_string() } else { s.cc.clone() },
                    "name": s.remark,
                    "ping": if ping == u32::MAX { serde_json::Value::Null } else { serde_json::json!(ping) },
                });
                if active_key.as_deref() == Some(s.key().as_str()) {
                    o["ok"] = serde_json::json!(true); // probe-verified live server
                }
                o
            })
            .collect();
        serde_json::Value::Array(arr)
    }

    fn state_json(&self) -> serde_json::Value {
        let cfg = self.cfg.lock().unwrap();
        let st = self.shared.status();
        let state = self.ui_state(&st.phase);
        let server = st.active.as_ref().map(|s| self.server_json_for(s));
        serde_json::json!({
            "status": state,
            "platform": "windows",
            "version": env!("CARGO_PKG_VERSION"),
            "features": { "auto": self.auto_enabled.load(Ordering::SeqCst) },
            "lang": cfg.lang,
            "server": server,
        })
    }

    // ---------------- actions ----------------
    fn parse_id(id: &str) -> Option<usize> {
        id.strip_prefix('s')?.parse::<usize>().ok()
    }

    fn action_connect(&self, p: &serde_json::Value) -> serde_json::Value {
        let id = p.get("id").and_then(|v| v.as_str()).unwrap_or("");
        let Some(idx) = Self::parse_id(id) else {
            return serde_json::json!({ "ok": false, "error": "bad id" });
        };
        let server = {
            let servers = self.shared.servers.lock().unwrap();
            servers.get(idx).cloned()
        };
        let Some(server) = server else {
            return serde_json::json!({ "ok": false, "error": "no such server" });
        };
        let sj = self.server_json_for(&server);
        if self.send_cmd(Cmd::Connect { server }) {
            serde_json::json!({ "ok": true, "async": true, "server": sj })
        } else {
            serde_json::json!({ "ok": false, "error": "engine stopped" })
        }
    }

    fn action_ping_batch(&self, p: &serde_json::Value) -> serde_json::Value {
        let ids = p.get("ids").and_then(|v| v.as_array()).cloned().unwrap_or_default();
        let servers = self.shared.servers.lock().unwrap();
        let pings = self.shared.pings.lock().unwrap();
        let mut map = serde_json::Map::new();
        for idv in ids {
            let Some(id) = idv.as_str() else { continue };
            let Some(idx) = Self::parse_id(id) else { continue };
            if idx < servers.len() {
                let ms = pings.get(idx).copied().unwrap_or(u32::MAX);
                if ms != u32::MAX {
                    map.insert(id.to_string(), serde_json::json!(ms));
                }
            }
        }
        serde_json::json!({ "pings": map })
    }

    fn action_stats(&self) -> serde_json::Value {
        let st = self.shared.status();
        let connected = st.phase == Phase::Connected;
        serde_json::json!({
            "ping": st.latency_ms,
            "down": if connected { serde_json::json!(self.shared.down.load(Ordering::Relaxed)) } else { serde_json::Value::Null },
            "up":   if connected { serde_json::json!(self.shared.up.load(Ordering::Relaxed)) } else { serde_json::Value::Null },
        })
    }

    fn action_get_ip(&self) -> serde_json::Value {
        let st = self.shared.status();
        match (&st.ip, &st.ip_cc) {
            (Some(ip), cc) => serde_json::json!({ "ip": ip, "cc": cc.clone().unwrap_or_default() }),
            (None, _) => serde_json::json!({}),
        }
    }

    fn maybe_auto_connect(self: &Arc<Self>) {
        if self.auto_done.swap(true, Ordering::SeqCst) {
            return;
        }
        if !self.auto_enabled.load(Ordering::SeqCst) {
            return;
        }
        {
            let st = self.shared.status();
            if st.phase != Phase::Idle {
                return;
            }
        }
        let api = self.clone();
        std::thread::Builder::new()
            .name("veil-autoconn".into())
            .spawn(move || {
                // wait for the first ping results (up to 8 s), then pick the fastest real node
                let mut best: Option<usize> = None;
                for _ in 0..27 {
                    {
                        let pings = api.shared.pings.lock().unwrap();
                        let mut fastest = u32::MAX;
                        let mut idx = None;
                        for (i, ms) in pings.iter().enumerate() {
                            if *ms < fastest {
                                fastest = *ms;
                                idx = Some(i);
                            }
                        }
                        if let Some(i) = idx {
                            best = Some(i);
                        }
                    }
                    if best.is_some() {
                        break;
                    }
                    std::thread::sleep(Duration::from_millis(300));
                }
                let Some(i) = best else { return };
                let server = {
                    let servers = api.shared.servers.lock().unwrap();
                    servers.get(i).cloned()
                };
                if let Some(server) = server {
                    let _ = api.send_cmd(Cmd::Connect { server });
                }
            })
            .ok();
    }
}

// ---------------- HTTP server ----------------

fn server_loop(api: Arc<WebApi>) {
    let listener = match TcpListener::bind("127.0.0.1:0") {
        Ok(l) => l,
        Err(e) => {
            crate::sysproxy::log_line(&format!("webapi: bind failed: {}", e));
            crate::sysproxy::error_box("Mehrveil", &format!("Failed to start the local UI server.\n{}", e));
            return;
        }
    };
    let port = listener.local_addr().map(|a| a.port()).unwrap_or(0);
    api.port_cell.store(port, Ordering::SeqCst);
    crate::sysproxy::log_line(&format!("webapi: listening on 127.0.0.1:{} (token ok)", port));

    for conn in listener.incoming() {
        let Ok(stream) = conn else { continue };
        let api = api.clone();
        std::thread::Builder::new()
            .name("veil-http".into())
            .spawn(move || {
                let _ = handle_conn(stream, &api);
            })
            .ok();
    }
}

struct Req {
    method: String,
    path: String,
    body: Vec<u8>,
}

fn handle_conn(mut stream: std::net::TcpStream, api: &Arc<WebApi>) -> std::io::Result<()> {
    stream.set_read_timeout(Some(Duration::from_secs(35))).ok();
    stream.set_write_timeout(Some(Duration::from_secs(10))).ok();

    // --- read head ---
    let mut buf: Vec<u8> = Vec::with_capacity(4096);
    let mut tmp = [0u8; 4096];
    let head_end = loop {
        if let Some(p) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
            break p;
        }
        if buf.len() > 32 * 1024 {
            return Ok(());
        }
        let n = stream.read(&mut tmp)?;
        if n == 0 {
            return Ok(());
        }
        buf.extend_from_slice(&tmp[..n]);
    };
    let head = String::from_utf8_lossy(&buf[..head_end]).to_string();
    let mut lines = head.split("\r\n");
    let line1 = lines.next().unwrap_or("");
    let mut parts = line1.split_whitespace();
    let method = parts.next().unwrap_or("").to_ascii_uppercase();
    let target = parts.next().unwrap_or("").to_string();
    let mut host_ok = false;
    for l in lines {
        let Some((k, v)) = l.split_once(':') else { continue };
        if k.trim().eq_ignore_ascii_case("host") {
            host_ok = v.trim().starts_with("127.0.0.1") || v.trim().starts_with("localhost");
        }
    }
    if !host_ok {
        let _ = stream.write_all(&json_res("403 Forbidden", &serde_json::json!({"error":"bad host"})));
        return Ok(());
    }
    let (path, query) = match target.split_once('?') {
        Some((p, q)) => (p.to_string(), q.to_string()),
        None => (target.clone(), String::new()),
    };

    // --- token check ---
    let mut token_ok = false;
    for kv in query.split('&') {
        if let Some(v) = kv.strip_prefix("k=") {
            token_ok = v == api.token;
        }
    }
    if !token_ok {
        let _ = stream.write_all(&json_res("403 Forbidden", &serde_json::json!({"error":"bad token"})));
        return Ok(());
    }

    // --- body (Content-Length) ---
    let mut body: Vec<u8> = Vec::new();
    let clen = head
        .to_ascii_lowercase()
        .split("\r\n")
        .find_map(|l| l.strip_prefix("content-length:"))
        .and_then(|v| v.trim().parse::<usize>().ok())
        .unwrap_or(0)
        .min(1024 * 1024);
    if clen > 0 {
        let already = buf.len().saturating_sub(head_end + 4);
        if already > 0 {
            let take = already.min(clen);
            body.extend_from_slice(&buf[head_end + 4..head_end + 4 + take]);
        }
        while body.len() < clen {
            let n = stream.read(&mut tmp)?;
            if n == 0 {
                break;
            }
            let take = n.min(clen - body.len());
            body.extend_from_slice(&tmp[..take]);
        }
    }

    let req = Req { method, path, body };

    // ---------------- routes ----------------
    let resp = match (req.method.as_str(), req.path.as_str()) {
        ("GET", "/") | ("GET", "/index.html") => {
            http_res("200 OK", "text/html; charset=utf-8", INDEX_HTML.as_bytes().to_vec())
        }
        ("GET", "/app.js") => http_res("200 OK", "application/javascript; charset=utf-8", APP_JS.as_bytes().to_vec()),
        ("GET", "/i18n.js") => http_res("200 OK", "application/javascript; charset=utf-8", I18N_JS.as_bytes().to_vec()),
        ("GET", "/favicon.ico") => http_res("200 OK", "image/png", crate::seed::ICON_PNG.to_vec()),
        ("GET", "/api/events") => route_events(api),
        ("POST", p) if p.starts_with("/api/") => {
            let action = &p["/api/".len()..];
            let payload: serde_json::Value =
                serde_json::from_slice(&req.body).unwrap_or(serde_json::json!({}));
            route_action(api, action, &payload)
        }
        _ => json_res("404 Not Found", &serde_json::json!({"error":"not found"})),
    };
    stream.write_all(&resp)?;
    stream.flush().ok();
    Ok(())
}

fn route_events(api: &Arc<WebApi>) -> Vec<u8> {
    // long-poll: deliver the next queued event, or wait up to 15 s
    let deadline = Instant::now() + Duration::from_secs(15);
    let mut q = api.events.lock().unwrap();
    while q.is_empty() {
        let now = Instant::now();
        if now >= deadline {
            break;
        }
        let (g, _) = api.ev_cv.wait_timeout(q, deadline - now).unwrap();
        q = g;
    }
    let ev = q.pop_front().unwrap_or(serde_json::json!({ "kind": "none" }));
    json_res("200 OK", &ev)
}

fn route_action(api: &Arc<WebApi>, action: &str, p: &serde_json::Value) -> Vec<u8> {
    let resp = match action {
        "getState" => api.state_json(),
        "connect" => api.action_connect(p),
        "disconnect" => {
            let _ = api.send_cmd(Cmd::Disconnect);
            serde_json::json!({ "ok": true })
        }
        "fetchServers" => {
            let custom = {
                let cfg = api.cfg.lock().unwrap();
                cfg.custom_source.trim().to_string()
            };
            let custom = if custom.is_empty() { None } else { Some(custom) };
            let _ = api.send_cmd(Cmd::FetchSources { urls: vec![], custom });
            serde_json::json!({ "ok": true, "async": true })
        }
        "pingBatch" => api.action_ping_batch(p),
        "stats" => api.action_stats(),
        "getIpInfo" => api.action_get_ip(),
        "setLang" => {
            let lang = p.get("lang").and_then(|v| v.as_str()).unwrap_or("en").to_string();
            {
                let mut cfg = api.cfg.lock().unwrap();
                cfg.lang = lang;
                cfg.save();
            }
            serde_json::json!({ "ok": true })
        }
        "setSource" => {
            let url = p.get("url").and_then(|v| v.as_str()).unwrap_or("").to_string();
            {
                let mut cfg = api.cfg.lock().unwrap();
                cfg.custom_source = url;
                cfg.save();
            }
            serde_json::json!({ "ok": true })
        }
        "setFeature" => {
            let key = p.get("key").and_then(|v| v.as_str()).unwrap_or("");
            let on = p.get("on").and_then(|v| v.as_bool()).unwrap_or(false);
            if key == "auto" {
                api.auto_enabled.store(on, Ordering::SeqCst);
                {
                    let mut cfg = api.cfg.lock().unwrap();
                    cfg.auto_connect = on;
                    cfg.save();
                }
                serde_json::json!({ "ok": true })
            } else {
                // honest: not implemented on this platform
                serde_json::json!({ "ok": false, "error": "not supported on windows" })
            }
        }
        _ => serde_json::json!({ "ok": false, "error": format!("unknown action: {}", action) }),
    };
    json_res("200 OK", &resp)
}
