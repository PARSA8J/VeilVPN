//! Win32 window host + WebView2 (system Chromium) UI shell.
//!
//! Why WebView2: it renders through DirectComposition (same as Edge), completely
//! bypassing the OpenGL/wgpu presentation paths that white-screened on some GPUs.
//! The UI is the shared Liquid Glass web app served over a loopback HTTP bridge.

use std::sync::mpsc;
use std::sync::Mutex;

use webview2_com::{
    AddScriptToExecuteOnDocumentCreatedCompletedHandler,
    CreateCoreWebView2ControllerCompletedHandler, CreateCoreWebView2EnvironmentCompletedHandler,
    Microsoft::Web::WebView2::Win32::*, take_pwstr,
};
use windows::{
    core::*,
    Win32::{
        Foundation::{
            COLORREF, E_FAIL, E_POINTER, HINSTANCE, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM,
        },
        Graphics::Gdi::CreateSolidBrush,
        System::{
            LibraryLoader,
        },
        UI::{
            HiDpi,
            Input::KeyboardAndMouse::SetFocus,
            WindowsAndMessaging::{
                CreateIconFromResourceEx, CreateWindowExW, DefWindowProcW, DestroyWindow,
                DispatchMessageW, GetClientRect, GetMessageW, GetSystemMetrics, LoadCursorW,
                PostQuitMessage, RegisterClassW, SendMessageW, ShowWindow, TranslateMessage,
                CW_USEDEFAULT, HCURSOR, HICON, ICON_BIG, ICON_SMALL, IDC_ARROW, LR_DEFAULTSIZE,
                MINMAXINFO, MSG, SM_CXSCREEN, SM_CYSCREEN, SW_SHOW, WINDOW_EX_STYLE, WNDCLASSW,
                WS_OVERLAPPEDWINDOW, WM_CLOSE, WM_DESTROY, WM_GETMINMAXINFO, WM_SETICON, WM_SIZE,
            },
        },
    },
};

/// COM interface wrappers are not Send/Sync in windows 0.62; the WebView2
/// controller lives on the UI thread and is only touched from the wndproc.
struct SendIface<T>(T);
unsafe impl<T> Send for SendIface<T> {}
unsafe impl<T> Sync for SendIface<T> {}

const WM_APP_REPAINT: u32 = 0x8001; // must match sysproxy::WM_APP_REPAINT
const MIN_W: i32 = 360;
const MIN_H: i32 = 560;

static CONTROLLER: Mutex<Option<SendIface<ICoreWebView2Controller>>> = Mutex::new(None);
static REPAINT_CB: Mutex<Option<Box<dyn Fn() + Send>>> = Mutex::new(None);

pub fn set_repaint_cb(f: Box<dyn Fn() + Send>) {
    *REPAINT_CB.lock().unwrap() = Some(f);
}

fn controller() -> Option<ICoreWebView2Controller> {
    CONTROLLER.lock().unwrap().as_ref().map(|w| w.0.clone())
}

/// Returns the WebView2 Evergreen runtime version, or None when missing.
pub fn check_runtime() -> Option<String> {
    unsafe {
        let mut ver = PWSTR::null();
        let hr = GetAvailableCoreWebView2BrowserVersionString(PCWSTR::null(), &mut ver);
        if hr.is_err() {
            return None;
        }
        let s = take_pwstr(ver);
        if s.trim().is_empty() {
            None
        } else {
            Some(s)
        }
    }
}

fn load_app_icon() -> Option<HICON> {
    let png = crate::seed::ICON_PNG;
    let h = unsafe { CreateIconFromResourceEx(png, true, 0x00030000, 0, 0, LR_DEFAULTSIZE) };
    h.ok()
}

extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    unsafe {
        match msg {
            WM_SIZE => {
                if let Some(c) = controller() {
                    let mut r = RECT::default();
                    let _ = GetClientRect(hwnd, &mut r);
                    let _ = c.SetBounds(r);
                }
                LRESULT(0)
            }
            WM_GETMINMAXINFO => {
                let mmi = lparam.0 as *mut MINMAXINFO;
                if !mmi.is_null() {
                    (*mmi).ptMinTrackSize = POINT { x: MIN_W, y: MIN_H };
                }
                DefWindowProcW(hwnd, msg, wparam, lparam)
            }
            WM_APP_REPAINT => {
                if let Some(f) = REPAINT_CB.lock().unwrap().as_ref() {
                    f();
                }
                LRESULT(0)
            }
            WM_CLOSE => {
                let _ = DestroyWindow(hwnd);
                LRESULT(0)
            }
            WM_DESTROY => {
                PostQuitMessage(0);
                LRESULT(0)
            }
            _ => DefWindowProcW(hwnd, msg, wparam, lparam),
        }
    }
}

fn to_pcw(s: &str) -> (Vec<u16>, PCWSTR) {
    let v: Vec<u16> = s.encode_utf16().chain([0]).collect();
    let p = PCWSTR(v.as_ptr());
    (v, p)
}

/// Create the (initially hidden) main window with a light matching background
/// brush so the pre-WebView fill blends with the app's pastel Liquid Glass page.
pub unsafe fn create_window() -> Result<HWND> {
    let hinstance: HINSTANCE = {
        let hm = LibraryLoader::GetModuleHandleW(None)?;
        HINSTANCE(hm.0)
    };
    let (class_buf, class_name) = to_pcw("MehrveilWnd");
    let (title_buf, title) = to_pcw("Mehrveil");
    let icon = load_app_icon().unwrap_or_default();
    let wc = WNDCLASSW {
        lpfnWndProc: Some(wndproc),
        hInstance: hinstance,
        hIcon: icon,
        hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(),
        // COLORREF = 0x00BBGGRR; RGB(240,241,246) pastel matches the web bg
        hbrBackground: CreateSolidBrush(COLORREF(0x00F6F1F0)),
        lpszClassName: class_name,
        ..Default::default()
    };
    RegisterClassW(&wc);

    let (w, h) = (430, 724);
    let sw = GetSystemMetrics(SM_CXSCREEN).max(w);
    let sh = GetSystemMetrics(SM_CYSCREEN).max(h);
    let x = if sw > w { (sw - w) / 2 } else { CW_USEDEFAULT };
    let y = if sh > h { (sh - h) / 2 } else { CW_USEDEFAULT };

    let hwnd = CreateWindowExW(
        WINDOW_EX_STYLE::default(),
        class_name,
        title,
        WS_OVERLAPPEDWINDOW, // no WS_VISIBLE: we show it after the UI is ready
        x,
        y,
        w,
        h,
        None,
        None,
        Some(hinstance),
        None,
    )?;

    if !icon.is_invalid() {
        let _ = SendMessageW(hwnd, WM_SETICON, Some(WPARAM(ICON_BIG as usize)), Some(LPARAM(icon.0 as isize)));
        let _ = SendMessageW(hwnd, WM_SETICON, Some(WPARAM(ICON_SMALL as usize)), Some(LPARAM(icon.0 as isize)));
    }
    let _ = (class_buf, title_buf); // keep alive while the class/window exists
    Ok(hwnd)
}

pub unsafe fn show(hwnd: HWND) {
    let _ = ShowWindow(hwnd, SW_SHOW);
    let _ = SetFocus(Some(hwnd));
}

/// Create the WebView2 environment + controller bound to `hwnd`, inject the
/// bridge marker, navigate to the local UI server, size and show the surface.
pub unsafe fn create_webview(hwnd: HWND, base_url: &str, token: &str, url: &str) -> Result<()> {
    // user data folder must be writable (Program Files is not)
    let user_data = crate::store::data_dir().join("WebView2");
    let ud_h = HSTRING::from(user_data.as_os_str());

    let environment = {
        let ud_h = ud_h.clone();
        let (tx, rx) = mpsc::channel();
        CreateCoreWebView2EnvironmentCompletedHandler::wait_for_async_operation(
            Box::new(move |handler| {
                CreateCoreWebView2EnvironmentWithOptions(
                    PCWSTR::null(),
                    PCWSTR(ud_h.as_ptr()),
                    None::<&ICoreWebView2EnvironmentOptions>,
                    &handler,
                )?;
                Ok(())
            }),
            Box::new(move |error_code, environment| {
                error_code?;
                tx.send(environment.ok_or_else(|| Error::from(E_POINTER)))
                    .map_err(|_| Error::from(E_FAIL))?;
                Ok(())
            }),
        )
        .map_err(map_wv_err)?;
        rx.recv().map_err(|_| Error::from(E_FAIL))??
    };

    let controller = {
        let (tx, rx) = mpsc::channel();
        CreateCoreWebView2ControllerCompletedHandler::wait_for_async_operation(
            Box::new(move |handler| {
                environment.CreateCoreWebView2Controller(hwnd, &handler)?;
                Ok(())
            }),
            Box::new(move |error_code, controller| {
                error_code?;
                tx.send(controller.ok_or_else(|| Error::from(E_POINTER)))
                    .map_err(|_| Error::from(E_FAIL))?;
                Ok(())
            }),
        )
        .map_err(map_wv_err)?;
        rx.recv().map_err(|_| Error::from(E_FAIL))??
    };

    // size to the current client area
    let mut client = RECT::default();
    let _ = GetClientRect(hwnd, &mut client);
    controller.SetBounds(client)?;

    let webview = controller.CoreWebView2()?;

    // harden the surface a bit (best-effort: older runtimes may reject some)
    let settings = webview.Settings()?;
    let _ = settings.SetAreDevToolsEnabled(false);
    let _ = settings.SetAreDefaultContextMenusEnabled(false);
    let _ = settings.SetIsStatusBarEnabled(false);
    let _ = settings.SetIsZoomControlEnabled(false);
    let _ = settings.SetIsBuiltInErrorPageEnabled(true);

    // bridge marker, available before any page script runs
    let init = format!("window.__VEIL_WIN__ = {{ base: '{}/', k: '{}' }};", base_url, token);
    let init_h = HSTRING::from(init);
    let webview_init = webview.clone();
    AddScriptToExecuteOnDocumentCreatedCompletedHandler::wait_for_async_operation(
        Box::new(move |handler| {
            webview_init.AddScriptToExecuteOnDocumentCreated(PCWSTR(init_h.as_ptr()), &handler)?;
            Ok(())
        }),
        Box::new(|error_code, _id| error_code),
    )
    .map_err(map_wv_err)?;

    let url_h = HSTRING::from(url);
    webview.Navigate(PCWSTR(url_h.as_ptr()))?;
    controller.SetIsVisible(true)?;

    *CONTROLLER.lock().unwrap() = Some(SendIface(controller));
    Ok(())
}

fn map_wv_err(e: webview2_com::Error) -> windows::core::Error {
    windows::core::Error::new(windows::core::HRESULT(-1), e.to_string())
}

/// Standard Win32 message loop. Returns after WM_QUIT.
pub unsafe fn run_message_loop() {
    let mut msg = MSG::default();
    loop {
        let r = GetMessageW(&mut msg, None, 0, 0);
        if r.0 <= 0 {
            break;
        }
        let _ = TranslateMessage(&msg);
        let _ = DispatchMessageW(&msg);
    }
}

pub fn cleanup() {
    if let Some(w) = CONTROLLER.lock().unwrap().take() {
        unsafe {
            let _ = w.0.Close();
        }
    }
    *REPAINT_CB.lock().unwrap() = None;
}

/// Best-effort per-monitor DPI awareness (Win10 1703+; ignore failures).
pub unsafe fn set_dpi_aware() {
    let _ = HiDpi::SetProcessDpiAwarenessContext(HiDpi::DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
}

#[allow(dead_code)]
fn _unused_shims(h: HCURSOR) -> HCURSOR {
    h
}
