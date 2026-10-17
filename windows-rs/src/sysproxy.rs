//! Windows system proxy switch (registry + WinINet refresh).
//! Non-windows: no-op stubs (for host tests).

#[cfg(windows)]
mod imp {
    use windows_sys::Win32::Foundation::ERROR_SUCCESS;
    use windows_sys::Win32::Networking::WinInet::{
        InternetSetOptionW, INTERNET_OPTION_REFRESH, INTERNET_OPTION_SETTINGS_CHANGED,
    };
    use windows_sys::Win32::System::Registry::{
        RegCloseKey, RegOpenKeyExW, RegQueryValueExW, RegSetValueExW, HKEY, HKEY_CURRENT_USER,
        KEY_QUERY_VALUE, KEY_SET_VALUE, REG_DWORD, REG_SZ,
    };

    // ---- native error box (last-resort startup failure report) ----
    #[link(name = "user32")]
    extern "system" {
        fn MessageBoxW(hwnd: *mut core::ffi::c_void, text: *const u16, caption: *const u16, utype: u32) -> i32;
    }

    pub fn error_box(title: &str, text: &str) {
        unsafe {
            let t: Vec<u16> = title.encode_utf16().chain([0]).collect();
            let b: Vec<u16> = text.encode_utf16().chain([0]).collect();
            MessageBoxW(std::ptr::null_mut(), b.as_ptr(), t.as_ptr(), 0x10); // MB_ICONERROR
        }
    }

    // ---- startup diagnostics log: %APPDATA%\Mehrveil\log.txt ----
    fn boot_ms() -> u128 {
        static BOOT: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();
        BOOT.get_or_init(std::time::Instant::now).elapsed().as_millis()
    }

    pub fn log_line(msg: &str) {
        let p = crate::store::data_dir().join("log.txt");
        // keep the file bounded: drop oldest half above 32 KB
        if let Ok(meta) = std::fs::metadata(&p) {
            if meta.len() > 32768 {
                if let Ok(old) = std::fs::read_to_string(&p) {
                    let cut = old.len() / 2;
                    let tail = old.split_off(cut);
                    let _ = std::fs::write(&p, tail);
                }
            }
        }
        let line = format!("[{:>6}ms] {}\n", boot_ms(), msg);
        if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&p) {
            use std::io::Write;
            let _ = f.write_all(line.as_bytes());
        }
    }

    pub fn read_log_tail() -> String {
        let p = crate::store::data_dir().join("log.txt");
        match std::fs::read_to_string(&p) {
            Ok(s) => {
                let n = s.chars().count();
                if n > 2200 {
                    format!("(last part of {})\n{}", p.display(), s.chars().skip(n - 2200).collect::<String>())
                } else {
                    format!("{}\n", s)
                }
            }
            Err(e) => format!("No log yet ({}).\nPath: {}", e, p.display()),
        }
    }

    const SUBKEY_STR: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Internet Settings";

    fn to_wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    fn wide_to_string(w: &[u16]) -> String {
        let end = w.iter().position(|&c| c == 0).unwrap_or(w.len());
        String::from_utf16_lossy(&w[..end])
    }

    fn open_settings() -> (HKEY, bool) {
        let sub = to_wide(SUBKEY_STR);
        let mut hk: HKEY = std::ptr::null_mut();
        let rc = unsafe {
            RegOpenKeyExW(HKEY_CURRENT_USER, sub.as_ptr(), 0, KEY_SET_VALUE | KEY_QUERY_VALUE, &mut hk)
        };
        if rc == ERROR_SUCCESS {
            (hk, true)
        } else {
            (std::ptr::null_mut(), false)
        }
    }

    fn set_str(hk: HKEY, name: &str, val: &str) {
        let wname = to_wide(name);
        let wval = to_wide(val);
        unsafe {
            RegSetValueExW(
                hk,
                wname.as_ptr(),
                0,
                REG_SZ,
                wval.as_ptr() as *const u8,
                (wval.len() * 2) as u32,
            );
        }
    }

    fn set_dword(hk: HKEY, name: &str, val: u32) {
        let wname = to_wide(name);
        unsafe {
            RegSetValueExW(hk, wname.as_ptr(), 0, REG_DWORD, &val as *const u32 as *const u8, 4);
        }
    }

    fn get_str(hk: HKEY, name: &str) -> Option<String> {
        let wname = to_wide(name);
        let mut buf = [0u16; 512];
        let mut size = (buf.len() * 2) as u32;
        let mut typ = 0u32;
        let rc = unsafe {
            RegQueryValueExW(
                hk,
                wname.as_ptr(),
                std::ptr::null(),
                &mut typ,
                buf.as_mut_ptr() as *mut u8,
                &mut size,
            )
        };
        if rc == ERROR_SUCCESS && typ == REG_SZ {
            Some(wide_to_string(&buf))
        } else {
            None
        }
    }

    fn get_dword(hk: HKEY, name: &str) -> Option<u32> {
        let wname = to_wide(name);
        let mut val = 0u32;
        let mut size = 4u32;
        let mut typ = 0u32;
        let rc = unsafe {
            RegQueryValueExW(
                hk,
                wname.as_ptr(),
                std::ptr::null(),
                &mut typ,
                &mut val as *mut u32 as *mut u8,
                &mut size,
            )
        };
        if rc == ERROR_SUCCESS {
            Some(val)
        } else {
            None
        }
    }

    fn refresh() {
        unsafe {
            InternetSetOptionW(std::ptr::null_mut(), INTERNET_OPTION_SETTINGS_CHANGED, std::ptr::null_mut(), 0);
            InternetSetOptionW(std::ptr::null_mut(), INTERNET_OPTION_REFRESH, std::ptr::null_mut(), 0);
        }
    }

    pub fn enable(port: u16) {
        let (hk, ok) = open_settings();
        if !ok {
            return;
        }
        // save previous state once (marker: ProxyVeilSaved)
        if get_dword(hk, "ProxyVeilSaved").unwrap_or(0) == 0 {
            if let Some(v) = get_dword(hk, "ProxyEnable") {
                set_dword(hk, "ProxyVeilPrevEnable", v);
            }
            if let Some(v) = get_str(hk, "ProxyServer") {
                set_str(hk, "ProxyVeilPrevServer", &v);
            }
            if let Some(v) = get_str(hk, "ProxyOverride") {
                set_str(hk, "ProxyVeilPrevOverride", &v);
            }
            set_dword(hk, "ProxyVeilSaved", 1);
        }
        set_dword(hk, "ProxyEnable", 1);
        set_str(hk, "ProxyServer", &format!("127.0.0.1:{}", port));
        set_str(hk, "ProxyOverride", "<local>;localhost;127.0.0.1;*.local;192.168.*;10.*;172.16.*");
        unsafe {
            RegCloseKey(hk);
        }
        refresh();
    }

    pub fn disable() {
        let (hk, ok) = open_settings();
        if !ok {
            return;
        }
        if get_dword(hk, "ProxyVeilSaved").unwrap_or(0) == 1 {
            let prev_en = get_dword(hk, "ProxyVeilPrevEnable").unwrap_or(0);
            set_dword(hk, "ProxyEnable", prev_en);
            if let Some(v) = get_str(hk, "ProxyVeilPrevServer") {
                if !v.is_empty() {
                    set_str(hk, "ProxyServer", &v);
                }
            }
            if let Some(v) = get_str(hk, "ProxyVeilPrevOverride") {
                if !v.is_empty() {
                    set_str(hk, "ProxyOverride", &v);
                }
            }
            set_dword(hk, "ProxyVeilSaved", 0);
        } else {
            set_dword(hk, "ProxyEnable", 0);
        }
        unsafe {
            RegCloseKey(hk);
        }
        refresh();
    }
}

#[cfg(not(windows))]
mod imp {
    pub fn enable(_port: u16) {}
    pub fn disable() {}
    pub fn error_box(_title: &str, _text: &str) {}
    pub fn log_line(_msg: &str) {}
    pub fn read_log_tail() -> String {
        String::new()
    }
}

pub fn enable(port: u16) {
    imp::enable(port);
}
pub fn disable() {
    imp::disable();
}
pub fn error_box(title: &str, text: &str) {
    imp::error_box(title, text);
}
pub fn log_line(msg: &str) {
    imp::log_line(msg);
}
pub fn read_log_tail() -> String {
    imp::read_log_tail()
}
