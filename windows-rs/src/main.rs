#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
//! Mehrveil — native Windows client (Rust + egui). See nothing. Be invisible.

mod app;
mod engine;
mod fetch;
mod i18n;
mod proto;
mod store;
mod sysproxy;
mod theme;
mod tls;
mod uri;
mod ws;

use eframe::Renderer;

fn load_icon() -> egui::IconData {
    let img = image::load_from_memory(i18n::ICON_PNG).expect("icon png").into_rgba8();
    let (w, h) = img.dimensions();
    egui::IconData { width: w, height: h, rgba: img.into_raw() }
}

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

/// Opaque window (transparent+acrylic broke presentation on some GPUs).
/// Window starts HIDDEN: we paint a few frames first, then show it —
/// this kills the classic "white window until first present/resize" bug
/// on Intel/VM/RDP graphics stacks. Liquid-glass look is painted by egui.
fn make_options() -> eframe::NativeOptions {
    eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Mehrveil")
            .with_inner_size([400.0, 680.0])
            .with_min_inner_size([360.0, 580.0])
            .with_icon(load_icon())
            .with_visible(false)
            .with_decorations(false),
        ..Default::default()
    }
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

fn main() -> eframe::Result<()> {
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

    // Renderer fallback chain, most-compatible first:
    //  1) wgpu (DirectX 12 / Vulkan, software WARP fallback) — immune to the
    //     OpenGL "white window" presentation bug seen on Intel HD / VM / RDP.
    //  2) glow (OpenGL) — for old systems without DX12/Vulkan (e.g. Windows 7).
    // If a renderer fails to create its context, run_native returns Err and we
    // try the next one; only if both fail we show the error box.
    let mut last_err: Option<eframe::Error> = None;
    for r in [Renderer::Wgpu, Renderer::Glow] {
        sysproxy::log_line(&format!("renderer: trying {:?}", r));
        let opts = make_options();
        let res = eframe::run_native(
            "Mehrveil",
            opts,
            Box::new(|cc| {
                match &cc.wgpu_render_state {
                    Some(rs) => {
                        let info = rs.adapter.get_info();
                        sysproxy::log_line(&format!(
                            "wgpu adapter: backend={:?} device_type={:?} name=\"{}\" driver=\"{}\"",
                            info.backend, info.device_type, info.name, info.driver
                        ));
                    }
                    None => sysproxy::log_line("render state: glow (no wgpu adapter info)"),
                }
                Ok(Box::new(app::VeilApp::new(cc)))
            }),
        );
        match res {
            ok @ Ok(_) => {
                sysproxy::log_line("run_native returned normally (window closed)");
                return ok;
            }
            Err(e) => {
                sysproxy::log_line(&format!("renderer {:?} failed: {}", r, e));
                last_err = Some(e);
            }
        }
    }

    let Some(e) = last_err else { return Ok(()) };
    sysproxy::error_box(
        "Mehrveil",
        &format!(
            "Failed to start the graphics engine.\n\n{}\n\nPlease update your GPU/graphics driver (get the latest driver from your laptop maker or intel.com) and try again.\n\nA diagnostic log was saved to:\n%APPDATA%\\Mehrveil\\log.txt",
            e
        ),
    );
    Err(e)
}
