//! Host-runnable UI harness: seed + engine + webapi, no window.
//! Serves the real shared/ UI at http://127.0.0.1:<port>/?k=<token>
//! so the app can be driven end-to-end in a real browser (dev/testing only).
//!
//! Run: cargo run --example hostui   (prints the URL, Ctrl-C to stop)

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

use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};

fn main() {
    let shared = engine::Shared::new();
    let seed_nodes = seed::parse_seed();
    let n = seed_nodes.len();
    *shared.pings.lock().unwrap() = vec![u32::MAX; n];
    *shared.servers.lock().unwrap() = seed_nodes;
    shared.servers_gen.fetch_add(1, Ordering::SeqCst);
    eprintln!("seed servers: {}", n);

    let mut engine = engine::Engine::start(shared.clone());
    let cfg = Arc::new(Mutex::new(store::Config::default()));
    let api = webapi::WebApi::spawn(shared.clone(), cfg, engine.tx.clone());

    // same boot sequence as main.rs (minus window plumbing)
    let _ = engine.tx.send(engine::Cmd::PingAll);
    api.push_status_event();
    api.push_servers_event();

    let url = format!("http://127.0.0.1:{}/?k={}", api.port(), api.token());
    println!("URL={}", url);
    println!("HOSTUI_READY");
    std::thread::sleep(std::time::Duration::from_secs(600));
    let _ = engine;
}
