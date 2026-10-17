#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
//! Mehrveil — native Windows client (Rust + egui). See nothing. Be invisible.

mod app;
mod engine;
mod fetch;
mod i18n;
mod license;
mod proto;
mod store;
mod sysproxy;
mod theme;
mod tls;
mod uri;
mod ws;

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

fn main() -> eframe::Result<()> {
    if !single_instance_ok() {
        return Ok(());
    }
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Mehrveil")
            .with_inner_size([400.0, 680.0])
            .with_min_inner_size([360.0, 580.0])
            .with_icon(load_icon())
            .with_transparent(true)
            .with_decorations(false),
        ..Default::default()
    };
    eframe::run_native("Mehrveil", options, Box::new(|cc| Ok(Box::new(app::VeilApp::new(cc)))))
}
