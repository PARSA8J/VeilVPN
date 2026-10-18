#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
//! Mehrveil — native Windows client (Rust core + WebView2 UI). See nothing. Be invisible.
//!
//! Architecture:
//!   • Rust core: engine (SOCKS5/HTTP local proxy + failover), proto (vless/trojan/ss),
//!     subscription fetching, real TCP pings, system-proxy switch, diagnostics log.
//!   • UI: system WebView2 (Chromium/DirectComposition) hosting the shared Liquid Glass
//!     web app over a loopback HTTP bridge — no GPU rasterization stack of our own,
//!     immune to the OpenGL/wgpu white-screen bugs of the previous egui versions.

mod engine;
mod fetch;
mod proto;
mod seed;
mod store;
mod sysproxy;
mod tls;
mod uri;
mod webapi;
#[cfg(windows)]
mod winui;
mod ws;

#[cfg(windows)]
fn single_instance_ok() -> bool {
    use windows_sys::Win32::Foundation::{GetLastError, ERROR_ALREADY_EXISTS};
    use windows_sys::Win32::System::Threading::CreateMutexW;
    let name: Vec<u16> = "Global\\Mehrveil.SingleInstance"
        .encode_utf16()
        .chain([0])
        .collect();
    unsafe {
        let h = CreateMutexW(std::ptr::null(), 0, name.as_ptr());
        if h.is_null() {
            return true; // couldn't create — allow anyway
        }
        GetLastError() != ERROR_ALREADY_EXISTS
    }
}

#[cfg(not(windows))]
fn single_instance_ok() -> bool {
    true
}

fn install_panic_hook() {
    std::panic::set_hook(Box::new(|info| {
        let payload = if let Some(s) = info.payload().downcast_ref::<&str>() {
            (*s).to_string()
        } else if let Some(s) = info.payload().downcast_ref::<String>() {
            s.clone()
        } else {
            "unknown panic".to_string()
        };
        let loc = info
            .location()
            .map(|l| format!("{}:{}", l.file(), l.line()))
            .unwrap_or_else(|| "?".into());
        let msg = format!("{}\nat {}", payload, loc);
        sysproxy::log_line(&format!("PANIC: {}", msg));
        sysproxy::error_box(
            "Mehrveil",
            &format!(
                "An internal error occurred and the app had to stop.\n\n{}\n\nA diagnostic log was saved to:\n%APPDATA%\\Mehrveil\\log.txt",
                msg
            ),
        );
    }));
}

#[cfg(windows)]
fn run_windows() -> Result<(), Box<dyn std::error::Error>> {
    use std::sync::atomic::Ordering;
    use std::sync::{Arc, Mutex};
    use windows::Win32::System::Com::{CoInitializeEx, COINIT_APARTMENTTHREADED};

    unsafe {
        winui::set_dpi_aware();
        CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok()?;
    }

    // WebView2 Evergreen runtime is a system component (like Edge). Detect it
    // up-front and give an actionable message instead of a broken window.
    let Some(wv_version) = winui::check_runtime() else {
        sysproxy::log_line("webview2 runtime NOT found");
        sysproxy::error_box(
            "Mehrveil",
            "Mehrveil needs the free \"WebView2 Runtime\" component from Microsoft\n(it is already built into Windows 11 and most Windows 10 systems).\n\nIt is missing on this PC.\n\n1) Install it from:\n     https://go.microsoft.com/fwlink/p/?LinkId=2124703\n2) Run Mehrveil again.\n",
        );
        return Ok(());
    };
    sysproxy::log_line(&format!("webview2 runtime: {}", wv_version));

    let hwnd = unsafe { winui::create_window() }?;

    // config + shared state
    let cfg = Arc::new(Mutex::new(store::Config::load()));
    let shared = engine::Shared::new();

    // seed servers are real and always available — load them before anything else
    {
        let seed_nodes = seed::parse_seed();
        let n = seed_nodes.len();
        *shared.pings.lock().unwrap() = vec![u32::MAX; n];
        *shared.servers.lock().unwrap() = seed_nodes;
        shared.servers_gen.fetch_add(1, Ordering::SeqCst);
        sysproxy::log_line(&format!("seed servers loaded: {}", n));
    }

    // engine (local proxy + tunnel) and the loopback UI bridge
    let mut engine = engine::Engine::start(shared.clone());
    let api = webapi::WebApi::spawn(shared.clone(), cfg.clone(), engine.tx.clone());
    *shared.hwnd.lock().unwrap() = Some(hwnd.0 as isize);

    winui::set_repaint_cb(Box::new({
        let api = api.clone();
        move || api.notify_repaint()
    }));

    // WebView2 UI — port is bound synchronously in spawn(), so it is real by
    // now; the wait below is a belt-and-braces guard so we can NEVER navigate
    // to a port-0 URL (Chromium refuses it: ERR_UNSAFE_PORT).
    let mut port = api.port();
    if port == 0 {
        for _ in 0..40 {
            std::thread::sleep(std::time::Duration::from_millis(50));
            port = api.port();
            if port != 0 {
                break;
            }
        }
    }
    if port == 0 {
        sysproxy::log_line("webapi: no listener after wait — aborting UI startup");
        sysproxy::error_box(
            "Mehrveil",
            "Failed to start the local UI server.\n\nPlease reinstall Mehrveil and try again.\n\nA diagnostic log was saved to:\n%APPDATA%\\Mehrveil\\log.txt",
        );
        sysproxy::disable();
        engine.abandon();
        return Ok(());
    }
    let base = format!("http://127.0.0.1:{}", port);
    let url = format!("{}/?k={}", base, api.token());
    if let Err(e) = unsafe { winui::create_webview(hwnd, &base, api.token(), &url) } {
        sysproxy::log_line(&format!("webview init failed: {}", e));
        sysproxy::error_box(
            "Mehrveil",
            &format!(
                "Failed to start the user interface (WebView2).\n\n{}\n\nA diagnostic log was saved to:\n%APPDATA%\\Mehrveil\\log.txt",
                e
            ),
        );
        sysproxy::disable();
        engine.abandon();
        return Ok(());
    }
    sysproxy::log_line(&format!("webview ready ({}), ui at {}", wv_version, base));

    unsafe { winui::show(hwnd) };
    unsafe { winui::run_message_loop() };

    // ----- exit: restore the user's proxy settings, stop the tunnel -----
    shared.hwnd.lock().unwrap().take();
    sysproxy::disable(); // guarded: only if WE enabled it
    engine.abandon();
    winui::cleanup();
    sysproxy::log_line("exit clean");
    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    install_panic_hook();
    sysproxy::log_line(&format!(
        "==== Mehrveil v{} boot (args: {:?}) ====",
        env!("CARGO_PKG_VERSION"),
        std::env::args().skip(1).collect::<Vec<_>>()
    ));

    // --diag: print the startup log in a message box, no window.
    if std::env::args().any(|a| a == "--diag") {
        sysproxy::log_line("diag mode requested");
        sysproxy::error_box("Mehrveil — Diagnostics", &sysproxy::read_log_tail());
        return Ok(());
    }

    if !single_instance_ok() {
        sysproxy::log_line("single-instance: another instance is running");
        sysproxy::error_box(
            "Mehrveil",
            "Mehrveil is already running.\n\nClose the other Mehrveil window (or end \"Mehrveil.exe\" in Task Manager) and try again.",
        );
        return Ok(());
    }

    #[cfg(windows)]
    {
        return run_windows();
    }
    #[cfg(not(windows))]
    {
        sysproxy::log_line("non-windows host: nothing to do");
        Ok(())
    }
}
