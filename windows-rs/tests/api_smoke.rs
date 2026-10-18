//! WebApi contract smoke test: drives the loopback UI bridge end-to-end on the
//! host platform (linux/mac/windows) and verifies the exact JSON contract that
//! shared/app.js expects (same shapes as the Android bridge + Windows events).

use std::io::{Read, Write};
use std::net::TcpStream;
use std::sync::atomic::Ordering;
use std::sync::{mpsc, Arc, Mutex};
use std::time::Duration;

#[path = "../src/uri.rs"]
mod uri;
#[path = "../src/tls.rs"]
mod tls;
#[path = "../src/ws.rs"]
mod ws;
#[path = "../src/proto.rs"]
mod proto;
#[path = "../src/fetch.rs"]
mod fetch;
#[path = "../src/seed.rs"]
mod seed;
#[path = "../src/store.rs"]
mod store;
#[path = "../src/sysproxy.rs"]
mod sysproxy;
#[path = "../src/engine.rs"]
mod engine;
#[path = "../src/webapi.rs"]
mod webapi;

fn http(port: u16, raw: &str, body: &[u8]) -> (u16, String) {
    let mut s = TcpStream::connect(("127.0.0.1", port)).expect("connect");
    s.set_read_timeout(Some(Duration::from_secs(20))).ok();
    s.write_all(raw.as_bytes()).unwrap();
    s.write_all(body).unwrap();
    let mut buf = Vec::new();
    let _ = s.read_to_end(&mut buf);
    let text = String::from_utf8_lossy(&buf).to_string();
    let status: u16 = text
        .split_whitespace()
        .nth(1)
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);
    let body_start = text
        .find("\r\n\r\n")
        .map(|p| text[p + 4..].to_string())
        .unwrap_or_default();
    (status, body_start)
}

fn get(port: u16, path: &str) -> (u16, String) {
    let raw = format!("GET {} HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n", path);
    http(port, &raw, b"")
}

fn post(port: u16, path: &str, json: &str) -> (u16, String) {
    let raw = format!(
        "POST {} HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n",
        path,
        json.len()
    );
    http(port, &raw, json.as_bytes())
}

#[test]
fn webapi_contract() {
    let shared = engine::Shared::new();

    // seed servers (same as the real app boot)
    let seed_nodes = seed::parse_seed();
    assert!(!seed_nodes.is_empty(), "seed must be non-empty");
    let n = seed_nodes.len();
    *shared.pings.lock().unwrap() = vec![u32::MAX; n];
    *shared.servers.lock().unwrap() = seed_nodes;
    shared.servers_gen.fetch_add(1, Ordering::SeqCst);

    // fake engine loop: mirrors Cmd handling that matters for the UI contract
    let (tx, rx) = mpsc::channel::<engine::Cmd>();
    let shared_h = shared.clone();
    std::thread::spawn(move || {
        while let Ok(cmd) = rx.recv() {
            match cmd {
                engine::Cmd::Connect { server } => {
                    let mut st = shared_h.status();
                    st.phase = engine::Phase::Connected;
                    st.active = Some(server);
                    st.since = Some(std::time::SystemTime::now());
                    st.latency_ms = Some(42);
                    shared_h.set_status(st);
                }
                engine::Cmd::Disconnect => {
                    let mut st = shared_h.status();
                    st.phase = engine::Phase::Idle;
                    st.active = None;
                    shared_h.set_status(st);
                }
                engine::Cmd::FetchSources { .. } => {
                    shared_h.servers_gen.fetch_add(1, Ordering::SeqCst);
                    shared_h.repaint();
                }
                engine::Cmd::PingAll => {}
            }
        }
    });

    let cfg = Arc::new(Mutex::new(store::Config::default()));
    let api = webapi::WebApi::spawn(shared.clone(), cfg, tx);
    // REGRESSION (v1.6.9): the port must be bound synchronously before spawn()
    // returns. v1.6.8 read port 0 here and navigated WebView2 to
    // http://127.0.0.1:0 → Chromium ERR_UNSAFE_PORT. No waiting allowed.
    assert_ne!(api.port(), 0, "port must be real immediately after spawn()");
    let port = api.port();
    let k = api.token().to_string();

    // --- security: bad token / bad host ---
    let (st, _) = get(port, &format!("/?k=wrong{}", k));
    assert_eq!(st, 403, "bad token must be rejected");
    let raw = format!("GET /?k={} HTTP/1.1\r\nHost: evil.example.com\r\n\r\n", k);
    let (st, _) = http(port, &raw, b"");
    assert_eq!(st, 403, "bad host must be rejected");

    // --- static assets ---
    let (st, body) = get(port, &format!("/?k={}", k));
    assert_eq!(st, 200);
    assert!(body.contains("app.js"), "index must reference app.js");
    let (st, body) = get(port, &format!("/app.js?k={}", k));
    assert_eq!(st, 200);
    assert!(body.contains("__VEIL_WIN__"), "app.js must read the bridge marker");
    let (st, _) = get(port, &format!("/i18n.js?k={}", k));
    assert_eq!(st, 200);

    // --- getState (boot contract) ---
    let (st, body) = post(port, &format!("/api/getState?k={}", k), "{}");
    assert_eq!(st, 200);
    let g: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!(g["platform"], "windows");
    assert_eq!(g["status"], "disconnected");
    assert_eq!(g["version"], "1.6.10");
    assert!(g["features"]["auto"].is_boolean());

    // --- servers arrive via the events queue (gen bump + repaint) ---
    api.notify_repaint();
    let (st, body) = get(port, &format!("/api/events?k={}", k));
    assert_eq!(st, 200);
    let ev: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert!(ev["kind"] == "servers" || ev["kind"] == "status", "event kind ok");
    if ev["kind"] == "servers" {
        let list = ev["servers"].as_array().expect("servers array");
        assert!(!list.is_empty());
        assert!(list[0]["id"].as_str().unwrap_or("").starts_with('s'), "string ids");
        assert!(list[0]["cc"].is_string());
        assert!(list[0]["p"].is_string());
    }

    // --- pingBatch with real pings ---
    *shared.pings.lock().unwrap() = vec![24, 31, u32::MAX, 88];
    let (st, body) = post(
        port,
        &format!("/api/pingBatch?k={}", k),
        r#"{"ids":["s0","s1","s2","s3"]}"#,
    );
    assert_eq!(st, 200);
    let r: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!(r["pings"]["s0"], 24);
    assert_eq!(r["pings"]["s1"], 31);
    assert!(r["pings"]["s2"].is_null(), "failed ping must be omitted");

    // --- connect → async ack, then status event ---
    let (st, body) = post(port, &format!("/api/connect?k={}", k), r#"{"id":"s0"}"#);
    assert_eq!(st, 200);
    let r: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!(r["ok"], true);
    assert_eq!(r["async"], true);
    assert_eq!(r["server"]["id"], "s0");
    std::thread::sleep(Duration::from_millis(200)); // allow status event to queue
    api.notify_repaint(); // engine repaint → status diff
    // drain events until status=connected
    let mut got_connected = false;
    for _ in 0..5 {
        let (st, body) = get(port, &format!("/api/events?k={}", k));
        assert_eq!(st, 200);
        let ev: serde_json::Value = serde_json::from_str(&body).unwrap();
        if ev["kind"] == "status" && ev["state"]["state"] == "connected" {
            got_connected = true;
            break;
        }
        if ev["kind"] == "none" {
            break;
        }
    }
    assert!(got_connected, "must receive a connected status event");

    // --- connected: state + stats + ok badge + ip info ---
    let (_, body) = post(port, &format!("/api/getState?k={}", k), "{}");
    let g: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!(g["status"], "connected");
    assert_eq!(g["server"]["id"], "s0");
    let (_, body) = post(port, &format!("/api/stats?k={}", k), "{}");
    let r: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!(r["ping"], 42);
    assert!(r["down"].is_u64() && r["up"].is_u64());
    shared.down.store(1000, Ordering::Relaxed);
    shared.up.store(500, Ordering::Relaxed);
    let (_, body) = post(port, &format!("/api/stats?k={}", k), "{}");
    let r: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!(r["down"], 1000);
    assert_eq!(r["up"], 500);
    let (_, body) = post(port, &format!("/api/getIpInfo?k={}", k), "{}");
    let _: serde_json::Value = serde_json::from_str(&body).unwrap(); // shape valid ({} or {ip,cc})

    // --- disconnect ---
    let (_, body) = post(port, &format!("/api/disconnect?k={}", k), "{}");
    let r: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!(r["ok"], true);
    std::thread::sleep(Duration::from_millis(150));
    api.notify_repaint();
    let (_, body) = post(port, &format!("/api/getState?k={}", k), "{}");
    let g: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!(g["status"], "disconnected");

    // --- setLang / setSource / setFeature(auto) persist via config ---
    let (_, body) = post(port, &format!("/api/setLang?k={}", k), r#"{"lang":"fa"}"#);
    let r: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!(r["ok"], true);
    let (_, body) = post(port, &format!("/api/setFeature?k={}", k), r#"{"key":"auto","on":true}"#);
    let r: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!(r["ok"], true);
    let (_, body) = post(port, &format!("/api/setFeature?k={}", k), r#"{"key":"kill","on":true}"#);
    let r: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!(r["ok"], false, "unsupported features must answer honestly");
    let (_, body) = post(port, &format!("/api/getState?k={}", k), "{}");
    let g: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!(g["lang"], "fa");
    assert_eq!(g["features"]["auto"], true);

    // --- fetchServers is async (servers arrive later via events) ---
    let (_, body) = post(port, &format!("/api/fetchServers?k={}", k), "{}");
    let r: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!(r["async"], true);

    // --- unknown action ---
    let (_, body) = post(port, &format!("/api/nope?k={}", k), "{}");
    let r: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!(r["ok"], false);

    let _ = std::fs::remove_file("mehrveil-data/config.json");
}
