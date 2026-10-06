//! Windows-Anbindung über selbst deklarierte Win32-, WGL- und DWM-Funktionen.
//!
//! Das Fenster hat keine Windows-Titelleiste: `WM_NCCALCSIZE` macht die ganze
//! Fensterfläche zur Zeichenfläche, `WM_NCHITTEST` meldet dem System, wo die
//! eigene Titelleiste und die Ränder zum Größeziehen liegen.

#![allow(non_snake_case, non_camel_case_types, clippy::upper_case_acronyms)]

use crate::{
    CaptionArea, Config, Event, FileFilter, Key, Modifiers, MouseButton, SaveAnswer, WindowCommand,
};
use std::cell::{Cell, RefCell};
use std::ffi::{c_char, c_void};
use std::path::PathBuf;
use std::ptr::{null, null_mut};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

type HANDLE = *mut c_void;
type HWND = HANDLE;
type HDC = HANDLE;
type HGLRC = HANDLE;
type HINSTANCE = HANDLE;
type HMODULE = HANDLE;
type HICON = HANDLE;
type HCURSOR = HANDLE;
type BOOL = i32;
type WPARAM = usize;
type LPARAM = isize;
type LRESULT = isize;
type WNDPROC = unsafe extern "system" fn(HWND, u32, WPARAM, LPARAM) -> LRESULT;

#[repr(C)]
struct WNDCLASSEXW {
    cbSize: u32,
    style: u32,
    lpfnWndProc: WNDPROC,
    cbClsExtra: i32,
    cbWndExtra: i32,
    hInstance: HINSTANCE,
    hIcon: HICON,
    hCursor: HCURSOR,
    hbrBackground: HANDLE,
    lpszMenuName: *const u16,
    lpszClassName: *const u16,
    hIconSm: HICON,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct POINT {
    x: i32,
    y: i32,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct RECT {
    left: i32,
    top: i32,
    right: i32,
    bottom: i32,
}

#[repr(C)]
struct MSG {
    hwnd: HWND,
    message: u32,
    wParam: WPARAM,
    lParam: LPARAM,
    time: u32,
    pt: POINT,
    lPrivate: u32,
}

#[repr(C)]
struct PIXELFORMATDESCRIPTOR {
    nSize: u16,
    nVersion: u16,
    dwFlags: u32,
    iPixelType: u8,
    cColorBits: u8,
    cRedBits: u8,
    cRedShift: u8,
    cGreenBits: u8,
    cGreenShift: u8,
    cBlueBits: u8,
    cBlueShift: u8,
    cAlphaBits: u8,
    cAlphaShift: u8,
    cAccumBits: u8,
    cAccumRedBits: u8,
    cAccumGreenBits: u8,
    cAccumBlueBits: u8,
    cAccumAlphaBits: u8,
    cDepthBits: u8,
    cStencilBits: u8,
    cAuxBuffers: u8,
    iLayerType: u8,
    bReserved: u8,
    dwLayerMask: u32,
    dwVisibleMask: u32,
    dwDamageMask: u32,
}

#[repr(C)]
struct MARGINS {
    left: i32,
    right: i32,
    top: i32,
    bottom: i32,
}

#[repr(C)]
struct TRACKMOUSEEVENT {
    cbSize: u32,
    dwFlags: u32,
    hwndTrack: HWND,
    dwHoverTime: u32,
}

#[repr(C)]
struct MINMAXINFO {
    ptReserved: POINT,
    ptMaxSize: POINT,
    ptMaxPosition: POINT,
    ptMinTrackSize: POINT,
    ptMaxTrackSize: POINT,
}

#[repr(C)]
struct OPENFILENAMEW {
    lStructSize: u32,
    hwndOwner: HWND,
    hInstance: HINSTANCE,
    lpstrFilter: *const u16,
    lpstrCustomFilter: *mut u16,
    nMaxCustFilter: u32,
    nFilterIndex: u32,
    lpstrFile: *mut u16,
    nMaxFile: u32,
    lpstrFileTitle: *mut u16,
    nMaxFileTitle: u32,
    lpstrInitialDir: *const u16,
    lpstrTitle: *const u16,
    Flags: u32,
    nFileOffset: u16,
    nFileExtension: u16,
    lpstrDefExt: *const u16,
    lCustData: LPARAM,
    lpfnHook: *const c_void,
    lpTemplateName: *const u16,
    pvReserved: *mut c_void,
    dwReserved: u32,
    FlagsEx: u32,
}

#[link(name = "kernel32")]
extern "system" {
    fn GetModuleHandleW(name: *const u16) -> HMODULE;
    fn LoadLibraryA(name: *const c_char) -> HMODULE;
    fn GetProcAddress(m: HMODULE, name: *const c_char) -> *const c_void;
}

#[link(name = "user32")]
extern "system" {
    fn RegisterClassExW(c: *const WNDCLASSEXW) -> u16;
    fn CreateWindowExW(
        ex: u32,
        class: *const u16,
        title: *const u16,
        style: u32,
        x: i32,
        y: i32,
        w: i32,
        h: i32,
        parent: HWND,
        menu: HANDLE,
        inst: HINSTANCE,
        param: *mut c_void,
    ) -> HWND;
    fn DefWindowProcW(h: HWND, m: u32, w: WPARAM, l: LPARAM) -> LRESULT;
    fn GetMessageW(msg: *mut MSG, h: HWND, min: u32, max: u32) -> BOOL;
    fn TranslateMessage(msg: *const MSG) -> BOOL;
    fn DispatchMessageW(msg: *const MSG) -> LRESULT;
    fn PostMessageW(h: HWND, m: u32, w: WPARAM, l: LPARAM) -> BOOL;
    fn SendMessageW(h: HWND, m: u32, w: WPARAM, l: LPARAM) -> LRESULT;
    fn PostQuitMessage(code: i32);
    fn DestroyWindow(h: HWND) -> BOOL;
    fn ShowWindow(h: HWND, cmd: i32) -> BOOL;
    fn GetDC(h: HWND) -> HDC;
    fn LoadCursorW(inst: HINSTANCE, name: *const u16) -> HCURSOR;
    fn GetClientRect(h: HWND, r: *mut RECT) -> BOOL;
    fn ScreenToClient(h: HWND, p: *mut POINT) -> BOOL;
    fn SetCapture(h: HWND) -> HWND;
    fn ReleaseCapture() -> BOOL;
    fn GetKeyState(vk: i32) -> i16;
    fn IsZoomed(h: HWND) -> BOOL;
    fn GetSystemMetrics(i: i32) -> i32;
    fn SetWindowPos(h: HWND, after: HWND, x: i32, y: i32, w: i32, hh: i32, flags: u32) -> BOOL;
    fn TrackMouseEvent(t: *mut TRACKMOUSEEVENT) -> BOOL;
    fn MessageBoxW(h: HWND, text: *const u16, caption: *const u16, kind: u32) -> i32;
    fn SetWindowTextW(h: HWND, text: *const u16) -> BOOL;
    fn CreateIcon(
        inst: HINSTANCE,
        w: i32,
        h: i32,
        planes: u8,
        bits: u8,
        and: *const u8,
        xor: *const u8,
    ) -> HICON;
    fn SetProcessDPIAware() -> BOOL;
}

#[link(name = "comdlg32")]
extern "system" {
    fn GetOpenFileNameW(o: *mut OPENFILENAMEW) -> BOOL;
    fn GetSaveFileNameW(o: *mut OPENFILENAMEW) -> BOOL;
}

#[link(name = "ole32")]
extern "system" {
    fn CoInitializeEx(reserved: *mut c_void, coinit: u32) -> i32;
}

#[link(name = "gdi32")]
extern "system" {
    fn ChoosePixelFormat(dc: HDC, pfd: *const PIXELFORMATDESCRIPTOR) -> i32;
    fn SetPixelFormat(dc: HDC, f: i32, pfd: *const PIXELFORMATDESCRIPTOR) -> BOOL;
    fn SwapBuffers(dc: HDC) -> BOOL;
    fn GetDeviceCaps(dc: HDC, i: i32) -> i32;
}

#[link(name = "opengl32")]
extern "system" {
    fn wglCreateContext(dc: HDC) -> HGLRC;
    fn wglMakeCurrent(dc: HDC, rc: HGLRC) -> BOOL;
    fn wglDeleteContext(rc: HGLRC) -> BOOL;
    fn wglGetProcAddress(name: *const c_char) -> *const c_void;
}

#[link(name = "dwmapi")]
extern "system" {
    fn DwmExtendFrameIntoClientArea(h: HWND, m: *const MARGINS) -> i32;
    fn DwmSetWindowAttribute(h: HWND, attr: u32, v: *const c_void, size: u32) -> i32;
    fn DwmFlush() -> i32;
}

const WM_DESTROY: u32 = 0x0002;
const WM_SIZE: u32 = 0x0005;
const WM_ACTIVATE: u32 = 0x0006;
const WM_PAINT: u32 = 0x000F;
const WM_CLOSE: u32 = 0x0010;
const WM_ERASEBKGND: u32 = 0x0014;
const WM_GETMINMAXINFO: u32 = 0x0024;
const WM_SETICON: u32 = 0x0080;
const WM_NCCALCSIZE: u32 = 0x0083;
const WM_NCHITTEST: u32 = 0x0084;
const WM_NCACTIVATE: u32 = 0x0086;
const WM_SYSCOMMAND: u32 = 0x0112;
const WM_KEYDOWN: u32 = 0x0100;
const WM_KEYUP: u32 = 0x0101;
const WM_SYSKEYDOWN: u32 = 0x0104;
const WM_SYSKEYUP: u32 = 0x0105;
const WM_MOUSEMOVE: u32 = 0x0200;
const WM_LBUTTONDOWN: u32 = 0x0201;
const WM_LBUTTONUP: u32 = 0x0202;
const WM_RBUTTONDOWN: u32 = 0x0204;
const WM_RBUTTONUP: u32 = 0x0205;
const WM_MBUTTONDOWN: u32 = 0x0207;
const WM_MBUTTONUP: u32 = 0x0208;
const WM_MOUSEWHEEL: u32 = 0x020A;
const WM_MOUSELEAVE: u32 = 0x02A3;
const WM_ENTERSIZEMOVE: u32 = 0x0231;
const WM_EXITSIZEMOVE: u32 = 0x0232;
const WM_DPICHANGED: u32 = 0x02E0;
const WM_APP_QUIT: u32 = 0x8001;

const SC_MINIMIZE: usize = 0xF020;
const SC_MAXIMIZE: usize = 0xF030;
const SC_RESTORE: usize = 0xF120;
const SC_CLOSE: usize = 0xF060;

const HTCLIENT: isize = 1;
const HTCAPTION: isize = 2;
const HTLEFT: isize = 10;
const HTRIGHT: isize = 11;
const HTTOP: isize = 12;
const HTTOPLEFT: isize = 13;
const HTTOPRIGHT: isize = 14;
const HTBOTTOM: isize = 15;
const HTBOTTOMLEFT: isize = 16;
const HTBOTTOMRIGHT: isize = 17;

const CS_VREDRAW: u32 = 0x0001;
const CS_HREDRAW: u32 = 0x0002;
const CS_OWNDC: u32 = 0x0020;
const WS_OVERLAPPEDWINDOW: u32 = 0x00CF_0000;
const CW_USEDEFAULT: i32 = 0x8000_0000_u32 as i32;
const SW_SHOWNORMAL: i32 = 1;
const SWP_NOSIZE: u32 = 0x0001;
const SWP_NOMOVE: u32 = 0x0002;
const SWP_NOZORDER: u32 = 0x0004;
const SWP_NOACTIVATE: u32 = 0x0010;
const SWP_FRAMECHANGED: u32 = 0x0020;
const SIZE_MINIMIZED: usize = 1;
const SIZE_MAXIMIZED: usize = 2;
const TME_LEAVE: u32 = 0x0002;
const SM_CXSCREEN: i32 = 0;
const SM_CYSCREEN: i32 = 1;
const SM_CXFRAME: i32 = 32;
const SM_CYFRAME: i32 = 33;
const SM_CXICON: i32 = 11;
const SM_CXSMICON: i32 = 49;
const SM_CXPADDEDBORDER: i32 = 92;
const LOGPIXELSX: i32 = 88;
const VK_SHIFT: i32 = 0x10;
const VK_CONTROL: i32 = 0x11;
const VK_MENU: i32 = 0x12;
const MB_ICONERROR: u32 = 0x10;
const MB_ICONWARNING: u32 = 0x30;
const MB_ICONINFORMATION: u32 = 0x40;
const MB_YESNOCANCEL: u32 = 0x3;
const IDYES: i32 = 6;
const IDNO: i32 = 7;
const OFN_OVERWRITEPROMPT: u32 = 0x0000_0002;
const OFN_HIDEREADONLY: u32 = 0x0000_0004;
const OFN_NOCHANGEDIR: u32 = 0x0000_0008;
const OFN_PATHMUSTEXIST: u32 = 0x0000_0800;
const OFN_FILEMUSTEXIST: u32 = 0x0000_1000;
const OFN_EXPLORER: u32 = 0x0008_0000;
const COINIT_APARTMENTTHREADED: u32 = 0x2;
const IDC_ARROW: usize = 32512;

const PFD_DOUBLEBUFFER: u32 = 0x0001;
const PFD_DRAW_TO_WINDOW: u32 = 0x0004;
const PFD_SUPPORT_OPENGL: u32 = 0x0020;

const WGL_CONTEXT_MAJOR_VERSION_ARB: i32 = 0x2091;
const WGL_CONTEXT_MINOR_VERSION_ARB: i32 = 0x2092;
const WGL_CONTEXT_PROFILE_MASK_ARB: i32 = 0x9126;
const WGL_CONTEXT_CORE_PROFILE_BIT_ARB: i32 = 0x0001;

const DWMWA_WINDOW_CORNER_PREFERENCE: u32 = 33;
const DWMWCP_ROUND: u32 = 2;

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

pub fn message_box(title: &str, text: &str) {
    let (t, m) = (wide(title), wide(text));
    unsafe {
        MessageBoxW(null_mut(), m.as_ptr(), t.as_ptr(), MB_ICONERROR);
    }
}

/// Zustand, den Fensterprozedur (Hauptthread) und Zeichenthread teilen.
struct Shared {
    width: AtomicU32,
    height: AtomicU32,
    dpi: AtomicU32,
    caption_height: AtomicU32,
    buttons_width: AtomicU32,
    /// Der Nutzer zieht gerade an Rand oder Titelleiste (Windows-Größeziehschleife).
    sizing: AtomicBool,
    /// Größe des zuletzt gezeigten Bildes (0, 0: noch keines).
    presented: Mutex<(u32, u32)>,
    presented_cv: Condvar,
}

/// Höchstens so lange wartet das Fenster beim Größeziehen auf ein passendes Bild.
const RESIZE_WAIT: Duration = Duration::from_millis(50);

impl Shared {
    /// Wartet, bis der Zeichenthread ein Bild in der Größe `w` × `h` gezeigt hat.
    /// Sonst zeigt der Fenstermanager während des Größeziehens kurz ein altes,
    /// verzerrtes oder leeres Bild: Das Fenster flackert.
    fn wait_presented(&self, w: u32, h: u32) {
        // Minimiert (0 × 0) wird nichts gezeichnet, also nicht warten
        if w == 0 || h == 0 {
            return;
        }
        let deadline = Instant::now() + RESIZE_WAIT;
        let Ok(mut p) = self.presented.lock() else {
            return;
        };
        // Vor dem ersten Bild gibt es nichts abzuwarten
        while *p != (0, 0) && *p != (w, h) {
            let now = Instant::now();
            if now >= deadline {
                return;
            }
            match self.presented_cv.wait_timeout(p, deadline - now) {
                Ok((g, _)) => p = g,
                Err(_) => return,
            }
        }
    }
}

struct WndState {
    tx: Sender<Event>,
    shared: Arc<Shared>,
    tracking_leave: bool,
    buttons_down: u32,
}

thread_local! {
    static STATE: RefCell<Option<WndState>> = const { RefCell::new(None) };
}

fn send(e: Event) {
    STATE.with(|s| {
        if let Some(s) = s.borrow().as_ref() {
            let _ = s.tx.send(e);
        }
    });
}

fn mods() -> Modifiers {
    unsafe {
        Modifiers {
            shift: GetKeyState(VK_SHIFT) < 0,
            ctrl: GetKeyState(VK_CONTROL) < 0,
            alt: GetKeyState(VK_MENU) < 0,
        }
    }
}

fn lparam_xy(l: LPARAM) -> (i32, i32) {
    (
        (l & 0xFFFF) as u16 as i16 as i32,
        ((l >> 16) & 0xFFFF) as u16 as i16 as i32,
    )
}

fn dpi_of(hwnd: HWND) -> u32 {
    type GetDpiForWindow = unsafe extern "system" fn(HWND) -> u32;
    unsafe {
        let user32 = LoadLibraryA(c"user32.dll".as_ptr());
        let f = GetProcAddress(user32, c"GetDpiForWindow".as_ptr());
        if !f.is_null() {
            let f: GetDpiForWindow = std::mem::transmute(f);
            let d = f(hwnd);
            if d > 0 {
                return d;
            }
        }
        let dc = GetDC(hwnd);
        let d = GetDeviceCaps(dc, LOGPIXELSX);
        if d > 0 {
            d as u32
        } else {
            96
        }
    }
}

fn frame_thickness(dpi: u32) -> (i32, i32) {
    let pad = metric(SM_CXPADDEDBORDER, dpi);
    (metric(SM_CXFRAME, dpi) + pad, metric(SM_CYFRAME, dpi) + pad)
}

/// Systemmaß für eine Auflösung; ältere Windows-Versionen ohne
/// `GetSystemMetricsForDpi` bekommen das Maß des Hauptbildschirms.
fn metric(i: i32, dpi: u32) -> i32 {
    type ForDpi = unsafe extern "system" fn(i32, u32) -> i32;
    unsafe {
        let user32 = LoadLibraryA(c"user32.dll".as_ptr());
        let f = GetProcAddress(user32, c"GetSystemMetricsForDpi".as_ptr());
        if f.is_null() {
            GetSystemMetrics(i)
        } else {
            std::mem::transmute::<*const c_void, ForDpi>(f)(i, dpi)
        }
    }
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_NCCALCSIZE if wp == 1 => {
            // Ganze Fensterfläche als Zeichenfläche. Maximiert ragt das Fenster
            // um die (unsichtbare) Rahmenbreite über den Bildschirm hinaus.
            if IsZoomed(hwnd) != 0 {
                let r = &mut *(lp as *mut RECT);
                let (fx, fy) = frame_thickness(dpi_of(hwnd));
                r.left += fx;
                r.right -= fx;
                r.top += fy;
                r.bottom -= fy;
            }
            0
        }
        WM_NCHITTEST => {
            let (sx, sy) = lparam_xy(lp);
            let mut p = POINT { x: sx, y: sy };
            ScreenToClient(hwnd, &mut p);
            let mut rc = RECT::default();
            GetClientRect(hwnd, &mut rc);
            let (w, h) = (rc.right, rc.bottom);
            let (cap_h, btn_w, dpi) = STATE.with(|s| {
                s.borrow().as_ref().map_or((0, 0, 96), |s| {
                    (
                        s.shared.caption_height.load(Ordering::Relaxed) as i32,
                        s.shared.buttons_width.load(Ordering::Relaxed) as i32,
                        s.shared.dpi.load(Ordering::Relaxed),
                    )
                })
            });
            let b = (6 * dpi / 96) as i32;
            let over_buttons = p.x >= w - btn_w && p.y < cap_h;
            if IsZoomed(hwnd) == 0 {
                let left = p.x < b;
                let right = p.x >= w - b;
                let top = p.y < b && !over_buttons;
                let bottom = p.y >= h - b;
                match (top, bottom, left, right) {
                    (true, _, true, _) => return HTTOPLEFT,
                    (true, _, _, true) => return HTTOPRIGHT,
                    (_, true, true, _) => return HTBOTTOMLEFT,
                    (_, true, _, true) => return HTBOTTOMRIGHT,
                    (true, ..) => return HTTOP,
                    (_, true, ..) => return HTBOTTOM,
                    (_, _, true, _) => return HTLEFT,
                    (_, _, _, true) => return HTRIGHT,
                    _ => {}
                }
            }
            if p.y < cap_h && !over_buttons {
                HTCAPTION
            } else {
                HTCLIENT
            }
        }
        WM_NCACTIVATE => {
            // -1 verhindert, dass Windows einen eigenen Rahmen zeichnet.
            DefWindowProcW(hwnd, msg, wp, -1)
        }
        WM_ACTIVATE => {
            send(Event::Focus(wp & 0xFFFF != 0));
            0
        }
        WM_SIZE => {
            if wp != SIZE_MINIMIZED {
                let (w, h) = ((lp & 0xFFFF) as u32, ((lp >> 16) & 0xFFFF) as u32);
                STATE.with(|s| {
                    if let Some(s) = s.borrow().as_ref() {
                        s.shared.width.store(w, Ordering::Relaxed);
                        s.shared.height.store(h, Ordering::Relaxed);
                    }
                });
                send(Event::Maximized(wp == SIZE_MAXIMIZED));
                send(Event::Resized {
                    width: w,
                    height: h,
                });
                // Erst weiter, wenn das Bild in neuer Größe da ist (gegen Flackern)
                let shared = STATE.with(|s| s.borrow().as_ref().map(|s| s.shared.clone()));
                if let Some(sh) = shared {
                    sh.wait_presented(w, h);
                }
            }
            0
        }
        WM_DPICHANGED => {
            let dpi = (wp & 0xFFFF) as u32;
            STATE.with(|s| {
                if let Some(s) = s.borrow().as_ref() {
                    s.shared.dpi.store(dpi, Ordering::Relaxed);
                }
            });
            let r = &*(lp as *const RECT);
            SetWindowPos(
                hwnd,
                null_mut(),
                r.left,
                r.top,
                r.right - r.left,
                r.bottom - r.top,
                SWP_NOZORDER | SWP_NOACTIVATE,
            );
            send(Event::ScaleChanged(dpi as f32 / 96.0));
            0
        }
        WM_GETMINMAXINFO => {
            let mm = &mut *(lp as *mut MINMAXINFO);
            mm.ptMinTrackSize = POINT { x: 640, y: 400 };
            0
        }
        WM_ENTERSIZEMOVE | WM_EXITSIZEMOVE => {
            STATE.with(|s| {
                if let Some(s) = s.borrow().as_ref() {
                    s.shared
                        .sizing
                        .store(msg == WM_ENTERSIZEMOVE, Ordering::Relaxed);
                }
            });
            // Nach dem Ziehen einmal neu zeichnen, wieder mit Bildsynchronisation
            send(Event::Redraw);
            0
        }
        WM_ERASEBKGND => 1,
        WM_PAINT => {
            send(Event::Redraw);
            DefWindowProcW(hwnd, msg, wp, lp)
        }
        WM_MOUSEMOVE => {
            let (x, y) = lparam_xy(lp);
            let start_tracking = STATE.with(|s| {
                s.borrow_mut().as_mut().is_some_and(|s| {
                    let start = !s.tracking_leave;
                    s.tracking_leave = true;
                    start
                })
            });
            if start_tracking {
                let mut t = TRACKMOUSEEVENT {
                    cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32,
                    dwFlags: TME_LEAVE,
                    hwndTrack: hwnd,
                    dwHoverTime: 0,
                };
                TrackMouseEvent(&mut t);
            }
            send(Event::MouseMove {
                x: x as f64,
                y: y as f64,
                mods: mods(),
            });
            0
        }
        WM_MOUSELEAVE => {
            STATE.with(|s| {
                if let Some(s) = s.borrow_mut().as_mut() {
                    s.tracking_leave = false;
                }
            });
            send(Event::MouseLeave);
            0
        }
        WM_LBUTTONDOWN | WM_MBUTTONDOWN | WM_RBUTTONDOWN | WM_LBUTTONUP | WM_MBUTTONUP
        | WM_RBUTTONUP => {
            let (x, y) = lparam_xy(lp);
            let button = match msg {
                WM_LBUTTONDOWN | WM_LBUTTONUP => MouseButton::Left,
                WM_MBUTTONDOWN | WM_MBUTTONUP => MouseButton::Middle,
                _ => MouseButton::Right,
            };
            let down = matches!(msg, WM_LBUTTONDOWN | WM_MBUTTONDOWN | WM_RBUTTONDOWN);
            let still_down = STATE.with(|s| {
                s.borrow_mut().as_mut().map_or(0, |s| {
                    if down {
                        s.buttons_down += 1;
                    } else {
                        s.buttons_down = s.buttons_down.saturating_sub(1);
                    }
                    s.buttons_down
                })
            });
            if down {
                SetCapture(hwnd);
            } else if still_down == 0 {
                ReleaseCapture();
            }
            let (x, y, m) = (x as f64, y as f64, mods());
            send(if down {
                Event::MouseDown {
                    button,
                    x,
                    y,
                    mods: m,
                }
            } else {
                Event::MouseUp {
                    button,
                    x,
                    y,
                    mods: m,
                }
            });
            0
        }
        WM_MOUSEWHEEL => {
            let delta = ((wp >> 16) & 0xFFFF) as u16 as i16 as f64 / 120.0;
            let (sx, sy) = lparam_xy(lp);
            let mut p = POINT { x: sx, y: sy };
            ScreenToClient(hwnd, &mut p);
            send(Event::Wheel {
                delta,
                x: p.x as f64,
                y: p.y as f64,
                mods: mods(),
            });
            0
        }
        WM_KEYDOWN | WM_KEYUP | WM_SYSKEYDOWN | WM_SYSKEYUP => {
            let down = matches!(msg, WM_KEYDOWN | WM_SYSKEYDOWN);
            let repeat = down && (lp >> 30) & 1 == 1;
            let key = match wp as u32 {
                0x09 => Key::Tab,
                0x1B => Key::Escape,
                0x0D => Key::Enter,
                0x08 => Key::Backspace,
                0x2E => Key::Delete,
                0x10 => Key::Shift,
                0x11 => Key::Control,
                0x12 => Key::Alt,
                c @ (0x30..=0x39 | 0x41..=0x5A) => Key::Char(c as u8 as char),
                c => Key::Other(c),
            };
            send(Event::Key {
                key,
                down,
                repeat,
                mods: mods(),
            });
            // Alt+F4 und das Systemmenü weiter vom System behandeln lassen
            if matches!(msg, WM_SYSKEYDOWN | WM_SYSKEYUP) {
                return DefWindowProcW(hwnd, msg, wp, lp);
            }
            0
        }
        // Vom Nutzer (Alt+F4, Taskleiste, eigener Knopf): die App darf nachfragen
        WM_SYSCOMMAND if wp & 0xFFF0 == SC_CLOSE => {
            send(Event::CloseRequested { ask: true });
            0
        }
        WM_CLOSE => {
            send(Event::CloseRequested { ask: false });
            0
        }
        WM_APP_QUIT => {
            DestroyWindow(hwnd);
            0
        }
        WM_DESTROY => {
            PostQuitMessage(0);
            0
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}

/// Teil der Oberfläche, der im Zeichenthread lebt.
pub struct Surface {
    hwnd: usize,
    hdc: usize,
    opengl32: usize,
    shared: Arc<Shared>,
    /// `wglSwapIntervalEXT`, falls vorhanden.
    swap_interval: Cell<usize>,
    /// Zuletzt gesetztes Bildintervall (1 = auf den Bildwechsel warten).
    interval: Cell<i32>,
    /// Größe des vorigen Bildes.
    last_size: Cell<(u32, u32)>,
    /// COM ist im Zeichenthread eingerichtet (für die Dateidialoge).
    com: Cell<bool>,
}

// Fenster- und Gerätekontext-Handles dürfen zwischen Threads weitergereicht werden.
unsafe impl Send for Surface {}

impl Surface {
    pub fn size(&self) -> (u32, u32) {
        (
            self.shared.width.load(Ordering::Relaxed),
            self.shared.height.load(Ordering::Relaxed),
        )
    }

    pub fn scale(&self) -> f32 {
        self.shared.dpi.load(Ordering::Relaxed) as f32 / 96.0
    }

    /// Zeigt das gezeichnete Bild; `w` × `h` ist die Größe, für die es gezeichnet wurde.
    ///
    /// Beim Größeziehen muss jedes Bild in genau dem Bildwechsel erscheinen, in
    /// dem der Fenstermanager auch den neuen Rahmen zeigt; sonst sieht man für
    /// einen Wechsel den alten Inhalt im neuen Rahmen, und alles, was am rechten
    /// Rand oder in der Mitte hängt, zittert hin und her. Deshalb dann: nicht auf
    /// den Bildwechsel warten (das schiebt das Bild um einen Wechsel nach hinten),
    /// sondern sofort abgeben und bis zum Zusammensetzen durch den
    /// Fenstermanager warten. Erst danach darf die Größeziehschleife weiter.
    pub fn swap_buffers(&self, w: u32, h: u32) {
        let resized = self.shared.sizing.load(Ordering::Relaxed) && self.last_size.get() != (w, h);
        self.last_size.set((w, h));
        self.set_interval(if resized { 0 } else { 1 });
        unsafe {
            SwapBuffers(self.hdc as HDC);
            if resized {
                DwmFlush();
            }
        }
        if let Ok(mut p) = self.shared.presented.lock() {
            *p = (w, h);
        }
        self.shared.presented_cv.notify_all();
    }

    fn set_interval(&self, i: i32) {
        type SwapInterval = unsafe extern "system" fn(i32) -> BOOL;
        let f = self.swap_interval.get();
        if f == 0 || self.interval.get() == i {
            return;
        }
        unsafe {
            let si: SwapInterval = std::mem::transmute(f as *const c_void);
            si(i);
        }
        self.interval.set(i);
    }

    pub fn gl_proc(&self, name: &str) -> *const c_void {
        debug_assert!(name.ends_with('\0'));
        let c = name.as_ptr() as *const c_char;
        unsafe {
            let p = wglGetProcAddress(c);
            match p as isize {
                // OpenGL-1.1-Funktionen liefert nur opengl32.dll selbst.
                -1..=3 => GetProcAddress(self.opengl32 as HMODULE, c),
                _ => p,
            }
        }
    }

    pub fn command(&self, c: WindowCommand) {
        let hwnd = self.hwnd as HWND;
        unsafe {
            match c {
                WindowCommand::Minimize => {
                    PostMessageW(hwnd, WM_SYSCOMMAND, SC_MINIMIZE, 0);
                }
                WindowCommand::ToggleMaximize => {
                    let sc = if IsZoomed(hwnd) != 0 {
                        SC_RESTORE
                    } else {
                        SC_MAXIMIZE
                    };
                    PostMessageW(hwnd, WM_SYSCOMMAND, sc, 0);
                }
                WindowCommand::Close => {
                    PostMessageW(hwnd, WM_SYSCOMMAND, SC_CLOSE, 0);
                }
            }
        }
    }

    pub fn set_title(&self, title: &str) {
        let t = wide(title);
        unsafe {
            SetWindowTextW(self.hwnd as HWND, t.as_ptr());
        }
    }

    /// Dateidialog des Systems. Läuft im Zeichenthread; das Fenster ist so lange
    /// gesperrt, der Hauptthread verarbeitet weiter seine Nachrichten.
    pub fn file_dialog(
        &self,
        save: bool,
        title: &str,
        filters: &[FileFilter],
        default_ext: &str,
        suggested: &str,
    ) -> Option<PathBuf> {
        if !self.com.get() {
            unsafe {
                CoInitializeEx(null_mut(), COINIT_APARTMENTTHREADED);
            }
            self.com.set(true);
        }
        // Bezeichnung\0Muster\0…\0\0
        let mut filter: Vec<u16> = Vec::new();
        for (name, pattern) in filters {
            filter.extend(name.encode_utf16().chain([0]));
            filter.extend(pattern.encode_utf16().chain([0]));
        }
        filter.push(0);
        let mut file = vec![0u16; 4096];
        for (i, c) in suggested.encode_utf16().take(file.len() - 1).enumerate() {
            file[i] = c;
        }
        let title = wide(title);
        let ext = wide(default_ext);
        let mut o = OPENFILENAMEW {
            lStructSize: std::mem::size_of::<OPENFILENAMEW>() as u32,
            hwndOwner: self.hwnd as HWND,
            hInstance: null_mut(),
            lpstrFilter: filter.as_ptr(),
            lpstrCustomFilter: null_mut(),
            nMaxCustFilter: 0,
            nFilterIndex: 1,
            lpstrFile: file.as_mut_ptr(),
            nMaxFile: file.len() as u32,
            lpstrFileTitle: null_mut(),
            nMaxFileTitle: 0,
            lpstrInitialDir: null(),
            lpstrTitle: title.as_ptr(),
            Flags: OFN_EXPLORER
                | OFN_HIDEREADONLY
                | OFN_NOCHANGEDIR
                | OFN_PATHMUSTEXIST
                | if save {
                    OFN_OVERWRITEPROMPT
                } else {
                    OFN_FILEMUSTEXIST
                },
            nFileOffset: 0,
            nFileExtension: 0,
            lpstrDefExt: if default_ext.is_empty() {
                null()
            } else {
                ext.as_ptr()
            },
            lCustData: 0,
            lpfnHook: null(),
            lpTemplateName: null(),
            pvReserved: null_mut(),
            dwReserved: 0,
            FlagsEx: 0,
        };
        let ok = unsafe {
            if save {
                GetSaveFileNameW(&mut o)
            } else {
                GetOpenFileNameW(&mut o)
            }
        };
        if ok == 0 {
            return None;
        }
        let len = file.iter().position(|&c| c == 0).unwrap_or(file.len());
        Some(PathBuf::from(String::from_utf16_lossy(&file[..len])))
    }

    pub fn ask_save(&self, question: &str) -> SaveAnswer {
        let (t, m) = (wide("Skizzeo"), wide(question));
        let r = unsafe {
            MessageBoxW(
                self.hwnd as HWND,
                m.as_ptr(),
                t.as_ptr(),
                MB_YESNOCANCEL | MB_ICONWARNING,
            )
        };
        match r {
            IDYES => SaveAnswer::Save,
            IDNO => SaveAnswer::Discard,
            _ => SaveAnswer::Cancel,
        }
    }

    pub fn message(&self, text: &str, error: bool) {
        let (t, m) = (wide("Skizzeo"), wide(text));
        let icon = if error {
            MB_ICONERROR
        } else {
            MB_ICONINFORMATION
        };
        unsafe {
            MessageBoxW(self.hwnd as HWND, m.as_ptr(), t.as_ptr(), icon);
        }
    }

    pub fn set_caption_area(&self, a: CaptionArea) {
        self.shared
            .caption_height
            .store(a.height, Ordering::Relaxed);
        self.shared
            .buttons_width
            .store(a.buttons_width, Ordering::Relaxed);
    }

    /// Legt im aufrufenden Thread einen OpenGL-3.3-Core-Kontext an.
    fn make_gl_context(&self) -> Result<(), String> {
        type CreateCtx = unsafe extern "system" fn(HDC, HGLRC, *const i32) -> HGLRC;
        type SwapInterval = unsafe extern "system" fn(i32) -> BOOL;
        let hdc = self.hdc as HDC;
        unsafe {
            let tmp = wglCreateContext(hdc);
            if tmp.is_null() || wglMakeCurrent(hdc, tmp) == 0 {
                return Err("OpenGL konnte nicht gestartet werden (Grafiktreiber prüfen).".into());
            }
            let f = wglGetProcAddress(c"wglCreateContextAttribsARB".as_ptr());
            if f.is_null() {
                return Err("Der Grafiktreiber bietet kein OpenGL 3.3.".into());
            }
            let create: CreateCtx = std::mem::transmute(f);
            let attribs = [
                WGL_CONTEXT_MAJOR_VERSION_ARB,
                3,
                WGL_CONTEXT_MINOR_VERSION_ARB,
                3,
                WGL_CONTEXT_PROFILE_MASK_ARB,
                WGL_CONTEXT_CORE_PROFILE_BIT_ARB,
                0,
            ];
            let ctx = create(hdc, null_mut(), attribs.as_ptr());
            if ctx.is_null() {
                return Err("Der Grafiktreiber bietet kein OpenGL 3.3 Core.".into());
            }
            wglMakeCurrent(hdc, ctx);
            wglDeleteContext(tmp);
            let si = wglGetProcAddress(c"wglSwapIntervalEXT".as_ptr());
            if !si.is_null() {
                self.swap_interval.set(si as usize);
                let si: SwapInterval = std::mem::transmute(si);
                si(1);
            }
        }
        Ok(())
    }
}

fn set_dpi_awareness() {
    type SetCtx = unsafe extern "system" fn(isize) -> BOOL;
    unsafe {
        let user32 = LoadLibraryA(c"user32.dll".as_ptr());
        let f = GetProcAddress(user32, c"SetProcessDpiAwarenessContext".as_ptr());
        // -4 = DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2
        if f.is_null() || (std::mem::transmute::<*const c_void, SetCtx>(f))(-4) == 0 {
            SetProcessDPIAware();
        }
    }
}

fn make_icon(inst: HINSTANCE, size: i32, rgba: &[u8]) -> HICON {
    let mut bgra = Vec::with_capacity(rgba.len());
    for p in rgba.chunks(4) {
        bgra.extend_from_slice(&[p[2], p[1], p[0], p[3]]);
    }
    let mask_row = ((size + 15) / 16 * 2) as usize;
    let mask = vec![0u8; mask_row * size as usize];
    unsafe { CreateIcon(inst, size, size, 1, 32, mask.as_ptr(), bgra.as_ptr()) }
}

pub fn run<F>(config: Config, app: F) -> Result<(), String>
where
    F: FnOnce(Receiver<Event>, Surface) -> Result<(), String> + Send + 'static,
{
    set_dpi_awareness();
    unsafe {
        let inst = GetModuleHandleW(null());
        let class = wide("SkizzeoFenster");
        let wc = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            style: CS_OWNDC | CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: wndproc,
            cbClsExtra: 0,
            cbWndExtra: 0,
            hInstance: inst,
            hIcon: null_mut(),
            hCursor: LoadCursorW(null_mut(), IDC_ARROW as *const u16),
            hbrBackground: null_mut(),
            lpszMenuName: null(),
            lpszClassName: class.as_ptr(),
            hIconSm: null_mut(),
        };
        if RegisterClassExW(&wc) == 0 {
            return Err("Fensterklasse konnte nicht angelegt werden.".into());
        }

        let (tx, rx) = channel();
        let shared = Arc::new(Shared {
            width: AtomicU32::new(0),
            height: AtomicU32::new(0),
            dpi: AtomicU32::new(96),
            caption_height: AtomicU32::new(0),
            buttons_width: AtomicU32::new(0),
            sizing: AtomicBool::new(false),
            presented: Mutex::new((0, 0)),
            presented_cv: Condvar::new(),
        });
        STATE.with(|s| {
            *s.borrow_mut() = Some(WndState {
                tx,
                shared: shared.clone(),
                tracking_leave: false,
                buttons_down: 0,
            })
        });

        let title = wide(&config.title);
        let hwnd = CreateWindowExW(
            0,
            class.as_ptr(),
            title.as_ptr(),
            WS_OVERLAPPEDWINDOW,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            null_mut(),
            null_mut(),
            inst,
            null_mut(),
        );
        if hwnd.is_null() {
            return Err("Fenster konnte nicht geöffnet werden.".into());
        }
        let dpi = dpi_of(hwnd);
        shared.dpi.store(dpi, Ordering::Relaxed);
        let s = dpi as f32 / 96.0;
        // Auf dem Hauptbildschirm mittig, höchstens 90 % seiner Größe
        let (sw, sh) = (GetSystemMetrics(SM_CXSCREEN), GetSystemMetrics(SM_CYSCREEN));
        let w = ((config.width as f32 * s) as i32).min(sw * 9 / 10);
        let h = ((config.height as f32 * s) as i32).min(sh * 9 / 10);
        SetWindowPos(
            hwnd,
            null_mut(),
            (sw - w) / 2,
            (sh - h) / 2,
            w,
            h,
            SWP_NOZORDER | SWP_NOACTIVATE,
        );

        // Schatten und (unter Windows 11) runde Ecken vom Fenstermanager behalten.
        let m = MARGINS {
            left: 0,
            right: 0,
            top: 1,
            bottom: 0,
        };
        DwmExtendFrameIntoClientArea(hwnd, &m);
        let corner = DWMWCP_ROUND;
        DwmSetWindowAttribute(
            hwnd,
            DWMWA_WINDOW_CORNER_PREFERENCE,
            &corner as *const u32 as *const c_void,
            4,
        );
        SetWindowPos(
            hwnd,
            null_mut(),
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_FRAMECHANGED,
        );

        if let Some(icon) = config.icon {
            for (kind, metric_id) in [(1usize, SM_CXICON), (0usize, SM_CXSMICON)] {
                let size = metric(metric_id, dpi).max(16);
                let h = make_icon(inst, size, &icon(size as u32));
                if !h.is_null() {
                    SendMessageW(hwnd, WM_SETICON, kind, h as isize);
                }
            }
        }

        let hdc = GetDC(hwnd);
        let pfd = PIXELFORMATDESCRIPTOR {
            nSize: std::mem::size_of::<PIXELFORMATDESCRIPTOR>() as u16,
            nVersion: 1,
            dwFlags: PFD_DRAW_TO_WINDOW | PFD_SUPPORT_OPENGL | PFD_DOUBLEBUFFER,
            iPixelType: 0,
            cColorBits: 32,
            cRedBits: 0,
            cRedShift: 0,
            cGreenBits: 0,
            cGreenShift: 0,
            cBlueBits: 0,
            cBlueShift: 0,
            cAlphaBits: 8,
            cAlphaShift: 0,
            cAccumBits: 0,
            cAccumRedBits: 0,
            cAccumGreenBits: 0,
            cAccumBlueBits: 0,
            cAccumAlphaBits: 0,
            cDepthBits: 24,
            cStencilBits: 8,
            cAuxBuffers: 0,
            iLayerType: 0,
            bReserved: 0,
            dwLayerMask: 0,
            dwVisibleMask: 0,
            dwDamageMask: 0,
        };
        let fmt = ChoosePixelFormat(hdc, &pfd);
        if fmt == 0 || SetPixelFormat(hdc, fmt, &pfd) == 0 {
            return Err("Kein passendes OpenGL-Pixelformat gefunden.".into());
        }

        let mut rc = RECT::default();
        GetClientRect(hwnd, &mut rc);
        shared.width.store(rc.right as u32, Ordering::Relaxed);
        shared.height.store(rc.bottom as u32, Ordering::Relaxed);

        let surface = Surface {
            hwnd: hwnd as usize,
            hdc: hdc as usize,
            opengl32: LoadLibraryA(c"opengl32.dll".as_ptr()) as usize,
            shared,
            swap_interval: Cell::new(0),
            interval: Cell::new(1),
            last_size: Cell::new((0, 0)),
            com: Cell::new(false),
        };
        let hwnd_val = hwnd as usize;
        let (err_tx, err_rx) = channel::<String>();
        let render = std::thread::Builder::new()
            .name("zeichnen".into())
            .spawn(move || {
                let result = surface.make_gl_context().and_then(|_| app(rx, surface));
                if let Err(e) = result {
                    let _ = err_tx.send(e);
                }
                PostMessageW(hwnd_val as HWND, WM_APP_QUIT, 0, 0);
            })
            .map_err(|e| e.to_string())?;

        ShowWindow(hwnd, SW_SHOWNORMAL);

        let mut msg: MSG = std::mem::zeroed();
        while GetMessageW(&mut msg, null_mut(), 0, 0) > 0 {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
        STATE.with(|s| *s.borrow_mut() = None); // Kanal schließen
        let _ = render.join();
        match err_rx.try_recv() {
            Ok(e) => Err(e),
            Err(_) => Ok(()),
        }
    }
}
