//! Windows-Anbindung über selbst deklarierte Win32-, WGL- und DWM-Funktionen.
//!
//! Das Fenster hat keine Windows-Titelleiste: `WM_NCCALCSIZE` macht die ganze
//! Fensterfläche zur Zeichenfläche, `WM_NCHITTEST` meldet dem System, wo die
//! eigene Titelleiste und die Ränder zum Größeziehen liegen.

#![allow(non_snake_case, non_camel_case_types, clippy::upper_case_acronyms)]

use crate::layout::{Action, Rect, Show, Windows as Layout};
use crate::{
    CaptionArea, Config, Cursor, Event, FileFilter, Key, Modifiers, MouseButton, WindowCommand,
    WindowId,
};
use std::cell::{Cell, RefCell};
use std::ffi::{c_char, c_void};
use std::path::PathBuf;
use std::ptr::{null, null_mut};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicUsize, Ordering};
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
#[derive(Default)]
struct MONITORINFO {
    cbSize: u32,
    rcMonitor: RECT,
    rcWork: RECT,
    dwFlags: u32,
}

#[repr(C)]
#[derive(Default)]
struct WINDOWPLACEMENT {
    length: u32,
    flags: u32,
    showCmd: u32,
    ptMinPosition: POINT,
    ptMaxPosition: POINT,
    rcNormalPosition: RECT,
}

#[repr(C)]
#[derive(Default)]
struct BITMAPINFOHEADER {
    biSize: u32,
    biWidth: i32,
    biHeight: i32,
    biPlanes: u16,
    biBitCount: u16,
    biCompression: u32,
    biSizeImage: u32,
    biXPelsPerMeter: i32,
    biYPelsPerMeter: i32,
    biClrUsed: u32,
    biClrImportant: u32,
}

#[repr(C)]
struct BITMAPINFO {
    bmiHeader: BITMAPINFOHEADER,
    bmiColors: [u8; 4],
}

type MONITORENUMPROC = unsafe extern "system" fn(HANDLE, HDC, *mut RECT, LPARAM) -> BOOL;

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
    fn GetLocalTime(t: *mut SYSTEMTIME);
    fn GlobalAlloc(flags: u32, bytes: usize) -> HANDLE;
    fn GlobalLock(h: HANDLE) -> *mut c_void;
    fn GlobalUnlock(h: HANDLE) -> BOOL;
    fn GlobalFree(h: HANDLE) -> HANDLE;
}

#[repr(C)]
#[derive(Default)]
struct SYSTEMTIME {
    year: u16,
    month: u16,
    day_of_week: u16,
    day: u16,
    hour: u16,
    minute: u16,
    second: u16,
    milliseconds: u16,
}

/// Ortszeit (Stunde, Minute).
pub fn local_time() -> (u8, u8) {
    let mut t = SYSTEMTIME::default();
    unsafe { GetLocalTime(&mut t) };
    (t.hour as u8, t.minute as u8)
}

pub fn local_date_time() -> (u16, u8, u8, u8, u8) {
    let mut t = SYSTEMTIME::default();
    unsafe { GetLocalTime(&mut t) };
    (
        t.year,
        t.month as u8,
        t.day as u8,
        t.hour as u8,
        t.minute as u8,
    )
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
    fn ReleaseDC(h: HWND, dc: HDC) -> i32;
    fn GetWindowRect(h: HWND, r: *mut RECT) -> BOOL;
    fn IsIconic(h: HWND) -> BOOL;
    fn IsWindowVisible(h: HWND) -> BOOL;
    fn SetForegroundWindow(h: HWND) -> BOOL;
    fn GetWindowPlacement(h: HWND, p: *mut WINDOWPLACEMENT) -> BOOL;
    fn SetWindowPlacement(h: HWND, p: *const WINDOWPLACEMENT) -> BOOL;
    fn MonitorFromWindow(h: HWND, flags: u32) -> HANDLE;
    fn GetMonitorInfoW(m: HANDLE, mi: *mut MONITORINFO) -> BOOL;
    fn EnumDisplayMonitors(dc: HDC, clip: *const RECT, f: MONITORENUMPROC, l: LPARAM) -> BOOL;
    fn LoadCursorW(inst: HINSTANCE, name: *const u16) -> HCURSOR;
    fn SetCursor(c: HCURSOR) -> HCURSOR;
    fn GetCursorPos(p: *mut POINT) -> BOOL;
    fn WindowFromPoint(p: POINT) -> HWND;
    fn GetCapture() -> HWND;
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
    fn OpenClipboard(h: HWND) -> BOOL;
    fn CloseClipboard() -> BOOL;
    fn EmptyClipboard() -> BOOL;
    fn GetClipboardData(format: u32) -> HANDLE;
    fn SetClipboardData(format: u32, h: HANDLE) -> HANDLE;
}

/// Unicode-Text in der Zwischenablage.
const CF_UNICODETEXT: u32 = 13;
const GMEM_MOVEABLE: u32 = 0x0002;

/// Text aus der Zwischenablage (`None`, wenn keiner da ist).
pub fn clipboard_text() -> Option<String> {
    unsafe {
        if OpenClipboard(null_mut()) == 0 {
            return None;
        }
        let h = GetClipboardData(CF_UNICODETEXT);
        let mut out = None;
        if !h.is_null() {
            let p = GlobalLock(h) as *const u16;
            if !p.is_null() {
                let mut n = 0;
                while *p.add(n) != 0 {
                    n += 1;
                }
                out = Some(String::from_utf16_lossy(std::slice::from_raw_parts(p, n)));
                GlobalUnlock(h);
            }
        }
        CloseClipboard();
        out
    }
}

/// Legt Text in die Zwischenablage.
pub fn set_clipboard_text(text: &str) {
    let w = wide(text);
    unsafe {
        if OpenClipboard(null_mut()) == 0 {
            return;
        }
        EmptyClipboard();
        let h = GlobalAlloc(GMEM_MOVEABLE, w.len() * 2);
        if !h.is_null() {
            let p = GlobalLock(h) as *mut u16;
            if p.is_null() {
                GlobalFree(h);
            } else {
                std::ptr::copy_nonoverlapping(w.as_ptr(), p, w.len());
                GlobalUnlock(h);
                if SetClipboardData(CF_UNICODETEXT, h).is_null() {
                    GlobalFree(h);
                }
            }
        }
        CloseClipboard();
    }
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
    fn SetDIBitsToDevice(
        dc: HDC,
        x: i32,
        y: i32,
        w: u32,
        h: u32,
        sx: i32,
        sy: i32,
        start: u32,
        lines: u32,
        bits: *const c_void,
        bmi: *const BITMAPINFO,
        usage: u32,
    ) -> i32;
    fn GdiFlush() -> BOOL;
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
const WM_WINDOWPOSCHANGED: u32 = 0x0047;
const WM_MOVING: u32 = 0x0216;
const WM_ACTIVATE: u32 = 0x0006;
const WM_PAINT: u32 = 0x000F;
const WM_CLOSE: u32 = 0x0010;
const WM_ERASEBKGND: u32 = 0x0014;
const WM_SETCURSOR: u32 = 0x0020;
const WM_GETMINMAXINFO: u32 = 0x0024;
const WM_SETICON: u32 = 0x0080;
const WM_NCCALCSIZE: u32 = 0x0083;
const WM_NCHITTEST: u32 = 0x0084;
const WM_NCACTIVATE: u32 = 0x0086;
const WM_SYSCOMMAND: u32 = 0x0112;
const WM_KEYDOWN: u32 = 0x0100;
const WM_KEYUP: u32 = 0x0101;
const WM_CHAR: u32 = 0x0102;
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
/// Der Zeichenthread hat einen anderen Mauszeiger gewählt.
const WM_APP_CURSOR: u32 = 0x8002;
/// Mengenfenster öffnen bzw. schließen (an das Hauptfenster gesandt).
const WM_APP_OPEN_QUANTITY: u32 = 0x8003;
const WM_APP_CLOSE_QUANTITY: u32 = 0x8004;
/// Fenster wieder nach vorn (nach dem Maximieren des anderen).
const WM_APP_RAISE: u32 = 0x8005;
/// Vom System maximiert (Aero Snap, Win+↑) bei angedocktem Mengenfenster:
/// stattdessen den Rest des Bildschirms füllen.
const WM_APP_FILL: u32 = 0x8006;

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
const SW_SHOWMAXIMIZED: i32 = 3;
const SW_SHOWNOACTIVATE: i32 = 4;
const SW_SHOWMINNOACTIVE: i32 = 7;
const SW_RESTORE: i32 = 9;
const WPF_RESTORETOMAXIMIZED: u32 = 0x0002;
const MONITOR_DEFAULTTONEAREST: u32 = 2;
const DIB_RGB_COLORS: u32 = 0;
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
const MB_ICONINFORMATION: u32 = 0x40;
const OFN_OVERWRITEPROMPT: u32 = 0x0000_0002;
const OFN_HIDEREADONLY: u32 = 0x0000_0004;
const OFN_NOCHANGEDIR: u32 = 0x0000_0008;
const OFN_PATHMUSTEXIST: u32 = 0x0000_0800;
const OFN_FILEMUSTEXIST: u32 = 0x0000_1000;
const OFN_EXPLORER: u32 = 0x0008_0000;
const COINIT_APARTMENTTHREADED: u32 = 0x2;
const IDC_ARROW: usize = 32512;
const IDC_IBEAM: usize = 32513;
const IDC_SIZENS: usize = 32645;
const IDC_HAND: usize = 32649;

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

/// Zustand, den Fensterprozedur (Hauptthread) und Zeichenthread je Fenster teilen.
struct Shared {
    /// Fenstergriff (0: Fenster gibt es nicht).
    hwnd: AtomicUsize,
    width: AtomicU32,
    height: AtomicU32,
    dpi: AtomicU32,
    caption_height: AtomicU32,
    buttons_width: AtomicU32,
    left_width: AtomicU32,
    /// Der Nutzer zieht gerade an Rand oder Titelleiste (Windows-Größeziehschleife).
    sizing: AtomicBool,
    /// Gewählter Mauszeiger (Index in [`Windows::cursors`]).
    cursor: AtomicU32,
    /// Größe des zuletzt gezeigten Bildes (0, 0: noch keines).
    presented: Mutex<(u32, u32)>,
    presented_cv: Condvar,
    /// Titel für ein Fenster, das erst noch angelegt wird.
    title: Mutex<String>,
}

/// Höchstens so lange wartet das Fenster beim Größeziehen auf ein passendes Bild.
const RESIZE_WAIT: Duration = Duration::from_millis(50);

impl Shared {
    fn new() -> Arc<Shared> {
        Arc::new(Shared {
            hwnd: AtomicUsize::new(0),
            width: AtomicU32::new(0),
            height: AtomicU32::new(0),
            dpi: AtomicU32::new(96),
            caption_height: AtomicU32::new(0),
            buttons_width: AtomicU32::new(0),
            left_width: AtomicU32::new(0),
            sizing: AtomicBool::new(false),
            cursor: AtomicU32::new(0),
            presented: Mutex::new((0, 0)),
            presented_cv: Condvar::new(),
            title: Mutex::new(String::new()),
        })
    }

    fn hwnd(&self) -> HWND {
        self.hwnd.load(Ordering::Relaxed) as HWND
    }

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

    fn mark_presented(&self, w: u32, h: u32) {
        if let Ok(mut p) = self.presented.lock() {
            *p = (w, h);
        }
        self.presented_cv.notify_all();
    }
}

/// Ein Fenster aus Sicht des Hauptthreads.
struct WndState {
    id: WindowId,
    shared: Arc<Shared>,
    tracking_leave: bool,
    buttons_down: u32,
}

/// Beide Fenster und was der Hauptthread zum Anlegen des Mengenfensters braucht.
struct Windows {
    tx: Sender<(WindowId, Event)>,
    /// Hauptfenster, dann (falls offen) das Mengenfenster.
    list: Vec<WndState>,
    /// Geteilt mit dem Zeichenthread, auch solange das Fenster zu ist.
    quantity: Arc<Shared>,
    /// Regeln für das Zusammenspiel (auch der Zeichenthread liest sie).
    layout: Arc<Mutex<Layout>>,
    /// Systemzeiger in der Reihenfolge von [`Cursor`], einmal geladen.
    cursors: [HCURSOR; 4],
    inst: usize,
    icon: Option<fn(u32) -> Vec<u8>>,
    /// Das Hauptfenster wird zerstört: das Mengenfenster geht mit, ohne Regeln.
    closing: bool,
}

thread_local! {
    static WINDOWS: RefCell<Option<Windows>> = const { RefCell::new(None) };
}

/// Greift kurz auf die Fenster zu. Darin keine Systemaufrufe: Sie können
/// Nachrichten verschachtelt zurück in die Fensterprozedur schicken.
fn with<R>(f: impl FnOnce(&mut Windows) -> R) -> Option<R> {
    WINDOWS.with(|w| w.borrow_mut().as_mut().map(f))
}

/// Kennung und geteilter Zustand von `hwnd`. Ein Fenster, das gerade angelegt
/// wird, meldet sich mit seiner ersten Nachricht an.
fn state(hwnd: HWND) -> Option<(WindowId, Arc<Shared>)> {
    with(|w| {
        let h = hwnd as usize;
        let i = w
            .list
            .iter()
            .position(|s| s.shared.hwnd.load(Ordering::Relaxed) == h)
            .or_else(|| {
                w.list
                    .iter()
                    .position(|s| s.shared.hwnd.load(Ordering::Relaxed) == 0)
            })?;
        let s = &w.list[i];
        s.shared.hwnd.store(h, Ordering::Relaxed);
        Some((s.id, s.shared.clone()))
    })
    .flatten()
}

fn id_of(hwnd: HWND) -> Option<WindowId> {
    state(hwnd).map(|s| s.0)
}

/// Griff eines offenen Fensters.
fn hwnd_of(id: WindowId) -> Option<HWND> {
    with(|w| {
        w.list
            .iter()
            .find(|s| s.id == id)
            .map(|s| s.shared.hwnd())
            .filter(|h| !h.is_null())
    })
    .flatten()
}

fn send(hwnd: HWND, e: Event) {
    let Some(id) = id_of(hwnd) else {
        return;
    };
    with(|w| {
        let _ = w.tx.send((id, e));
    });
}

/// Regeln befragen: Sperre nur für die Rechnung, die Taten danach ausführen.
fn ask(f: impl FnOnce(&mut Layout) -> Vec<Action>) -> Vec<Action> {
    let Some(layout) = with(|w| w.layout.clone()) else {
        return Vec::new();
    };
    let actions = match layout.lock() {
        Ok(mut g) => f(&mut g),
        Err(e) => f(&mut e.into_inner()),
    };
    actions
}

/// Eine Frage an die Regeln, die nichts ändert.
fn peek<R: Default>(f: impl FnOnce(&Layout) -> R) -> R {
    let Some(layout) = with(|w| w.layout.clone()) else {
        return R::default();
    };
    let r = match layout.lock() {
        Ok(g) => f(&g),
        Err(e) => f(&e.into_inner()),
    };
    r
}

fn main_filled() -> bool {
    peek(|l| l.main_filled())
}

fn fills_on_maximize(work: Rect) -> bool {
    peek(|l| l.fills_on_maximize(work))
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
        ReleaseDC(hwnd, dc);
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

fn to_rect(r: RECT) -> Rect {
    (r.left, r.top, r.right - r.left, r.bottom - r.top)
}

/// Sichtbare Lage eines Fensters. Maximiert ragt es um die unsichtbare
/// Rahmenbreite über den Bildschirm hinaus; die zählt nicht.
unsafe fn window_rect(hwnd: HWND) -> Rect {
    let mut r = RECT::default();
    GetWindowRect(hwnd, &mut r);
    if IsZoomed(hwnd) != 0 {
        let (fx, fy) = frame_thickness(dpi_of(hwnd));
        r.left += fx;
        r.right -= fx;
        r.top += fy;
        r.bottom -= fy;
    }
    to_rect(r)
}

/// Arbeitsbereich (ohne Taskleiste) des Bildschirms, auf dem `hwnd` liegt.
unsafe fn work_area(hwnd: HWND) -> Rect {
    let mon = MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST);
    let mut mi = MONITORINFO {
        cbSize: std::mem::size_of::<MONITORINFO>() as u32,
        ..Default::default()
    };
    GetMonitorInfoW(mon, &mut mi);
    to_rect(mi.rcWork)
}

unsafe extern "system" fn collect_monitor(mon: HANDLE, _: HDC, _: *mut RECT, lp: LPARAM) -> BOOL {
    let out = &mut *(lp as *mut Vec<Rect>);
    let mut mi = MONITORINFO {
        cbSize: std::mem::size_of::<MONITORINFO>() as u32,
        ..Default::default()
    };
    if GetMonitorInfoW(mon, &mut mi) != 0 {
        out.push(to_rect(mi.rcWork));
    }
    1
}

/// Arbeitsbereiche aller Bildschirme.
pub fn monitors() -> Vec<Rect> {
    let mut out: Vec<Rect> = Vec::new();
    unsafe {
        EnumDisplayMonitors(
            null_mut(),
            null(),
            collect_monitor,
            &mut out as *mut Vec<Rect> as LPARAM,
        );
    }
    out
}

unsafe fn set_rect(hwnd: HWND, r: Rect) {
    SetWindowPos(
        hwnd,
        null_mut(),
        r.0,
        r.1,
        r.2,
        r.3,
        SWP_NOZORDER | SWP_NOACTIVATE,
    );
}

/// Maximiert `hwnd`; Wiederherstellen bringt es nach `r` (Bildschirmpunkte).
unsafe fn maximize_at(hwnd: HWND, r: Rect) {
    let mut wp = WINDOWPLACEMENT {
        length: std::mem::size_of::<WINDOWPLACEMENT>() as u32,
        ..Default::default()
    };
    GetWindowPlacement(hwnd, &mut wp);
    // rcNormalPosition zählt ab dem Arbeitsbereich, nicht ab dem Bildschirm
    let mon = MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST);
    let mut mi = MONITORINFO {
        cbSize: std::mem::size_of::<MONITORINFO>() as u32,
        ..Default::default()
    };
    GetMonitorInfoW(mon, &mut mi);
    let (dx, dy) = (
        mi.rcWork.left - mi.rcMonitor.left,
        mi.rcWork.top - mi.rcMonitor.top,
    );
    wp.rcNormalPosition = RECT {
        left: r.0 - dx,
        top: r.1 - dy,
        right: r.0 + r.2 - dx,
        bottom: r.1 + r.3 - dy,
    };
    wp.showCmd = SW_SHOWMAXIMIZED as u32;
    wp.flags = 0;
    SetWindowPlacement(hwnd, &wp);
}

/// Führt die Taten der Regeln aus. `from`: das Fenster, dessen Meldung sie
/// ausgelöst hat (wird nach einem Maximieren des anderen wieder vorn).
unsafe fn run_actions(actions: Vec<Action>, from: Option<HWND>) {
    let mut actions = actions.into_iter().peekable();
    while let Some(a) = actions.next() {
        match a {
            Action::Create(WindowId::Quantity, r) => create_quantity(r),
            Action::Create(WindowId::Main, _) | Action::AskSave => {}
            // Alte Lage und Maximieren in einem Schritt (sonst kurz sichtbar)
            Action::Place(id, r) if actions.peek() == Some(&Action::Show(id, Show::Maximize)) => {
                actions.next();
                if let Some(h) = hwnd_of(id) {
                    maximize_at(h, r);
                }
            }
            Action::Place(id, r) => {
                if let Some(h) = hwnd_of(id) {
                    if IsZoomed(h) != 0 || IsIconic(h) != 0 {
                        ShowWindow(h, SW_RESTORE);
                    }
                    set_rect(h, r);
                }
            }
            Action::Show(id, how) => {
                let Some(h) = hwnd_of(id) else { continue };
                match how {
                    Show::Minimize => {
                        ShowWindow(h, SW_SHOWMINNOACTIVE);
                    }
                    Show::RestoreNoActivate => {
                        // Vor dem Minimieren maximiert: so zurück, dann den
                        // Auslöser wieder nach vorn
                        let mut wp = WINDOWPLACEMENT {
                            length: std::mem::size_of::<WINDOWPLACEMENT>() as u32,
                            ..Default::default()
                        };
                        GetWindowPlacement(h, &mut wp);
                        if wp.flags & WPF_RESTORETOMAXIMIZED != 0 {
                            ShowWindow(h, SW_SHOWMAXIMIZED);
                            if let Some(f) = from {
                                PostMessageW(f, WM_APP_RAISE, 0, 0);
                            }
                        } else {
                            ShowWindow(h, SW_SHOWNOACTIVATE);
                        }
                    }
                    Show::Maximize => {
                        ShowWindow(h, SW_SHOWMAXIMIZED);
                    }
                }
            }
            Action::Front(id) => {
                if let Some(h) = hwnd_of(id) {
                    if IsIconic(h) != 0 {
                        ShowWindow(h, SW_RESTORE);
                    }
                    SetForegroundWindow(h);
                }
            }
            Action::Close(WindowId::Quantity) => {
                if let Some(h) = hwnd_of(WindowId::Quantity) {
                    DestroyWindow(h);
                }
            }
            // Das Hauptfenster schließt die App (Ende des Zeichenthreads)
            Action::Close(WindowId::Main) => {}
        }
    }
}

/// Schatten, runde Ecken und Programmsymbol für ein eigenes Fenster.
unsafe fn decorate(hwnd: HWND, inst: HINSTANCE, icon: Option<fn(u32) -> Vec<u8>>, dpi: u32) {
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
        SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE | SWP_FRAMECHANGED,
    );
    if let Some(icon) = icon {
        for (kind, metric_id) in [(1usize, SM_CXICON), (0usize, SM_CXSMICON)] {
            let size = metric(metric_id, dpi).max(16);
            let h = make_icon(inst, size, &icon(size as u32));
            if !h.is_null() {
                SendMessageW(hwnd, WM_SETICON, kind, h as isize);
            }
        }
    }
}

/// Legt das Mengenfenster an `r` an und zeigt es, ohne es zu aktivieren.
unsafe fn create_quantity(r: Rect) {
    if hwnd_of(WindowId::Quantity).is_some() {
        return;
    }
    let Some((shared, inst, icon)) = with(|w| {
        w.quantity.hwnd.store(0, Ordering::Relaxed);
        w.quantity.mark_presented(0, 0);
        w.list.push(WndState {
            id: WindowId::Quantity,
            shared: w.quantity.clone(),
            tracking_leave: false,
            buttons_down: 0,
        });
        (w.quantity.clone(), w.inst, w.icon)
    }) else {
        return;
    };
    let title = wide(&shared.title.lock().map(|t| t.clone()).unwrap_or_default());
    let class = wide(QUANTITY_CLASS);
    let hwnd = CreateWindowExW(
        0,
        class.as_ptr(),
        title.as_ptr(),
        WS_OVERLAPPEDWINDOW,
        r.0,
        r.1,
        r.2,
        r.3,
        null_mut(),
        null_mut(),
        inst as HINSTANCE,
        null_mut(),
    );
    if hwnd.is_null() {
        with(|w| w.list.retain(|s| s.id != WindowId::Quantity));
        if let Some(l) = with(|w| w.layout.clone()) {
            if let Ok(mut l) = l.lock() {
                l.quantity_gone();
            }
        }
        return;
    }
    shared.hwnd.store(hwnd as usize, Ordering::Relaxed);
    let dpi = dpi_of(hwnd);
    shared.dpi.store(dpi, Ordering::Relaxed);
    decorate(hwnd, inst as HINSTANCE, icon, dpi);
    let mut rc = RECT::default();
    GetClientRect(hwnd, &mut rc);
    shared.width.store(rc.right as u32, Ordering::Relaxed);
    shared.height.store(rc.bottom as u32, Ordering::Relaxed);
    send(hwnd, Event::ScaleChanged(dpi as f32 / 96.0));
    send(
        hwnd,
        Event::Resized {
            width: rc.right as u32,
            height: rc.bottom as u32,
        },
    );
    ShowWindow(hwnd, SW_SHOWNOACTIVATE);
    // Gleich hinter das Hauptfenster: beide zusammen vor anderen Programmen
    if let Some(main) = hwnd_of(WindowId::Main) {
        SetWindowPos(
            hwnd,
            main,
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
        );
    }
}

/// Knopf „Mengenermittlung“ (vom Zeichenthread über `WM_APP_OPEN_QUANTITY`).
unsafe fn open_quantity(main: HWND) {
    let r = window_rect(main);
    let zoomed = IsZoomed(main) != 0;
    let work = work_area(main);
    let scale = dpi_of(main) as f32 / 96.0;
    let actions = ask(|l| l.open_quantity(r, zoomed, work, scale));
    run_actions(actions, Some(main));
}

/// Setzt den gewählten Zeiger des Fensters.
unsafe fn apply_cursor(hwnd: HWND) {
    let Some((_, shared)) = state(hwnd) else {
        return;
    };
    let i = shared.cursor.load(Ordering::Relaxed) as usize;
    if let Some(c) = with(|w| w.cursors[i.min(3)]) {
        SetCursor(c);
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
            let Some((id, sh)) = state(hwnd) else {
                return DefWindowProcW(hwnd, msg, wp, lp);
            };
            let cap_h = sh.caption_height.load(Ordering::Relaxed) as i32;
            let btn_w = sh.buttons_width.load(Ordering::Relaxed) as i32;
            let left_w = sh.left_width.load(Ordering::Relaxed) as i32;
            let dpi = sh.dpi.load(Ordering::Relaxed);
            // Angedockt ändert am Mengenfenster nur der rechte Rand die Breite
            let docked = id == WindowId::Quantity
                && with(|w| w.layout.lock().map(|l| l.docked()).unwrap_or(false)).unwrap_or(false);
            let b = (6 * dpi / 96) as i32;
            // Knopfgruppen rechts und links (Menü, Rückgängig, E17) gehören der App
            let over_buttons = (p.x >= w - btn_w || p.x < left_w) && p.y < cap_h;
            if IsZoomed(hwnd) == 0 {
                let left = p.x < b && !docked;
                let right = p.x >= w - b;
                let top = p.y < b && !over_buttons && !docked;
                let bottom = p.y >= h - b && !docked;
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
            let active = wp & 0xFFFF != 0;
            send(hwnd, Event::Focus(active));
            // Das andere Fenster gleich dahinter: beide zusammen vor anderen
            // Programmen (Alt+Tab, Klick in die Taskleiste)
            let minimized = (wp >> 16) & 0xFFFF != 0;
            if active && !minimized {
                let other = match id_of(hwnd) {
                    Some(WindowId::Main) => hwnd_of(WindowId::Quantity),
                    Some(WindowId::Quantity) => hwnd_of(WindowId::Main),
                    None => None,
                };
                if let Some(o) = other.filter(|&o| IsIconic(o) == 0 && IsWindowVisible(o) != 0) {
                    SetWindowPos(
                        o,
                        hwnd,
                        0,
                        0,
                        0,
                        0,
                        SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
                    );
                }
            }
            0
        }
        WM_SIZE => {
            let Some((id, sh)) = state(hwnd) else {
                return 0;
            };
            if wp == SIZE_MINIMIZED {
                let actions = ask(|l| l.minimized(id));
                run_actions(actions, Some(hwnd));
                return 0;
            }
            let (w, h) = ((lp & 0xFFFF) as u32, ((lp >> 16) & 0xFFFF) as u32);
            sh.width.store(w, Ordering::Relaxed);
            sh.height.store(h, Ordering::Relaxed);
            // Neben dem angedockten Mengenfenster gefüllt: zeigt sich wie maximiert
            let filled = id == WindowId::Main && main_filled();
            send(hwnd, Event::Maximized(wp == SIZE_MAXIMIZED || filled));
            send(
                hwnd,
                Event::Resized {
                    width: w,
                    height: h,
                },
            );
            // Erst weiter, wenn das Bild in neuer Größe da ist (gegen Flackern)
            sh.wait_presented(w, h);
            let mut actions = ask(|l| l.restored(id));
            if wp == SIZE_MAXIMIZED {
                actions.extend(ask(|l| l.maximized(id)));
                let work = work_area(hwnd);
                if id == WindowId::Main && fills_on_maximize(work) {
                    PostMessageW(hwnd, WM_APP_FILL, 0, 0);
                }
            }
            run_actions(actions, Some(hwnd));
            0
        }
        WM_WINDOWPOSCHANGED => {
            // Erzeugt WM_SIZE und WM_MOVE
            let r = DefWindowProcW(hwnd, msg, wp, lp);
            if IsIconic(hwnd) != 0 {
                return r;
            }
            let actions = match id_of(hwnd) {
                // Maximiert wandert das Mengenfenster nicht mit
                Some(WindowId::Main) if IsZoomed(hwnd) == 0 => {
                    let m = window_rect(hwnd);
                    let filled = main_filled();
                    let a = ask(|l| l.main_moved(m));
                    // Weggezogen: nicht mehr „maximiert“
                    if filled && !main_filled() {
                        send(hwnd, Event::Maximized(false));
                    }
                    a
                }
                Some(WindowId::Quantity) if IsZoomed(hwnd) == 0 => {
                    let q = window_rect(hwnd);
                    ask(|l| l.quantity_resized(q))
                }
                _ => Vec::new(),
            };
            run_actions(actions, Some(hwnd));
            r
        }
        WM_MOVING => {
            // Mengenfenster an der Titelleiste gezogen: Lösen und Einrasten
            if id_of(hwnd) == Some(WindowId::Quantity) {
                if let Some(main) = hwnd_of(WindowId::Main) {
                    let m = (IsZoomed(main) == 0).then(|| window_rect(main));
                    let rc = &mut *(lp as *mut RECT);
                    let proposed = to_rect(*rc);
                    let actions = ask(|l| {
                        if let Some(m) = m {
                            l.note_main(m);
                        }
                        l.quantity_moved(proposed)
                    });
                    for a in actions {
                        if let Action::Place(WindowId::Quantity, r) = a {
                            *rc = RECT {
                                left: r.0,
                                top: r.1,
                                right: r.0 + r.2,
                                bottom: r.1 + r.3,
                            };
                        }
                    }
                }
            }
            1
        }
        WM_DPICHANGED => {
            let dpi = (wp & 0xFFFF) as u32;
            if let Some((_, sh)) = state(hwnd) {
                sh.dpi.store(dpi, Ordering::Relaxed);
            }
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
            send(hwnd, Event::ScaleChanged(dpi as f32 / 96.0));
            0
        }
        WM_GETMINMAXINFO => {
            let mm = &mut *(lp as *mut MINMAXINFO);
            mm.ptMinTrackSize = match id_of(hwnd) {
                Some(WindowId::Quantity) => POINT { x: 320, y: 240 },
                _ => POINT { x: 640, y: 400 },
            };
            0
        }
        WM_ENTERSIZEMOVE | WM_EXITSIZEMOVE => {
            if let Some((_, sh)) = state(hwnd) {
                sh.sizing.store(msg == WM_ENTERSIZEMOVE, Ordering::Relaxed);
            }
            // Nach dem Ziehen einmal neu zeichnen, wieder mit Bildsynchronisation
            send(hwnd, Event::Redraw);
            // Eingerastet: Höhe und Oberkante vom Hauptfenster übernehmen
            if msg == WM_EXITSIZEMOVE && id_of(hwnd) == Some(WindowId::Quantity) {
                if let Some(main) = hwnd_of(WindowId::Main).filter(|&m| IsZoomed(m) == 0) {
                    let m = window_rect(main);
                    let actions = ask(|l| l.main_moved(m));
                    run_actions(actions, Some(hwnd));
                }
            }
            0
        }
        // Nur über der Zeichenfläche; Rand und Titelleiste behandelt das System
        WM_SETCURSOR if lp & 0xFFFF == HTCLIENT => {
            apply_cursor(hwnd);
            1
        }
        WM_APP_CURSOR => {
            // Sofort tauschen, wenn die Maus über der Zeichenfläche steht oder
            // gefangen ist (beim Ziehen kommt kein WM_SETCURSOR)
            let mut p = POINT::default();
            GetCursorPos(&mut p);
            let captured = GetCapture() == hwnd;
            let over = WindowFromPoint(p) == hwnd && {
                let l = ((p.x as u16 as u32) | ((p.y as u16 as u32) << 16)) as LPARAM;
                SendMessageW(hwnd, WM_NCHITTEST, 0, l) == HTCLIENT
            };
            if captured || over {
                apply_cursor(hwnd);
            }
            0
        }
        WM_ERASEBKGND => 1,
        WM_PAINT => {
            send(hwnd, Event::Redraw);
            DefWindowProcW(hwnd, msg, wp, lp)
        }
        WM_MOUSEMOVE => {
            let (x, y) = lparam_xy(lp);
            let id = id_of(hwnd);
            let start_tracking = with(|w| {
                w.list
                    .iter_mut()
                    .find(|s| Some(s.id) == id)
                    .is_some_and(|s| {
                        let start = !s.tracking_leave;
                        s.tracking_leave = true;
                        start
                    })
            })
            .unwrap_or(false);
            if start_tracking {
                let mut t = TRACKMOUSEEVENT {
                    cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32,
                    dwFlags: TME_LEAVE,
                    hwndTrack: hwnd,
                    dwHoverTime: 0,
                };
                TrackMouseEvent(&mut t);
            }
            send(
                hwnd,
                Event::MouseMove {
                    x: x as f64,
                    y: y as f64,
                    mods: mods(),
                },
            );
            0
        }
        WM_MOUSELEAVE => {
            let id = id_of(hwnd);
            with(|w| {
                if let Some(s) = w.list.iter_mut().find(|s| Some(s.id) == id) {
                    s.tracking_leave = false;
                }
            });
            send(hwnd, Event::MouseLeave);
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
            let id = id_of(hwnd);
            let still_down = with(|w| {
                w.list.iter_mut().find(|s| Some(s.id) == id).map_or(0, |s| {
                    if down {
                        s.buttons_down += 1;
                    } else {
                        s.buttons_down = s.buttons_down.saturating_sub(1);
                    }
                    s.buttons_down
                })
            })
            .unwrap_or(0);
            if down {
                SetCapture(hwnd);
            } else if still_down == 0 {
                ReleaseCapture();
            }
            let (x, y, m) = (x as f64, y as f64, mods());
            send(
                hwnd,
                if down {
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
                },
            );
            0
        }
        WM_MOUSEWHEEL => {
            let delta = ((wp >> 16) & 0xFFFF) as u16 as i16 as f64 / 120.0;
            let (sx, sy) = lparam_xy(lp);
            let mut p = POINT { x: sx, y: sy };
            ScreenToClient(hwnd, &mut p);
            send(
                hwnd,
                Event::Wheel {
                    delta,
                    x: p.x as f64,
                    y: p.y as f64,
                    mods: mods(),
                },
            );
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
                0x25 => Key::Left,
                0x27 => Key::Right,
                0x24 => Key::Home,
                0x23 => Key::End,
                c @ (0x30..=0x39 | 0x41..=0x5A) => Key::Char(c as u8 as char),
                // Ziffernblock
                c @ 0x60..=0x69 => Key::Char((b'0' + (c - 0x60) as u8) as char),
                0x6E | 0xBC => Key::Char(','),
                0xBE => Key::Char('.'),
                0x6D | 0xBD => Key::Char('-'),
                c => Key::Other(c),
            };
            send(
                hwnd,
                Event::Key {
                    key,
                    down,
                    repeat,
                    mods: mods(),
                },
            );
            // Alt allein und F10 öffnen das Dateimenü der App (E17), nicht das
            // Systemmenü; Alt+F4 und Alt+Leertaste behandelt weiter das System
            if matches!(msg, WM_SYSKEYDOWN | WM_SYSKEYUP) && !matches!(wp as u32, 0x12 | 0x79) {
                return DefWindowProcW(hwnd, msg, wp, lp);
            }
            0
        }
        // Getippte Zeichen (mit Umlauten und Großschreibung) für Textfelder;
        // Steuerzeichen (Strg+Buchstabe, Rücktaste, Enter) kommen als Taste
        WM_CHAR => {
            if let Some(c) = char::from_u32(wp as u32).filter(|c| !c.is_control()) {
                send(hwnd, Event::Text(c));
            }
            0
        }
        // Maximieren neben dem angedockten Mengenfenster: Rest des Bildschirms
        WM_SYSCOMMAND if wp & 0xFFF0 == SC_MAXIMIZE && id_of(hwnd) == Some(WindowId::Main) => {
            let (m, work) = (window_rect(hwnd), work_area(hwnd));
            let actions = ask(|l| l.maximize_main(m, work));
            if actions.is_empty() {
                return DefWindowProcW(hwnd, msg, wp, lp);
            }
            run_actions(actions, Some(hwnd));
            0
        }
        WM_APP_FILL => {
            if IsZoomed(hwnd) != 0 {
                ShowWindow(hwnd, SW_RESTORE);
            }
            if !main_filled() {
                let (m, work) = (window_rect(hwnd), work_area(hwnd));
                let actions = ask(|l| l.maximize_main(m, work));
                run_actions(actions, Some(hwnd));
            }
            0
        }
        // Vom Nutzer (Alt+F4, Taskleiste, eigener Knopf): die App darf nachfragen
        WM_SYSCOMMAND if wp & 0xFFF0 == SC_CLOSE => {
            send(hwnd, Event::CloseRequested { ask: true });
            0
        }
        WM_CLOSE => {
            send(hwnd, Event::CloseRequested { ask: false });
            0
        }
        WM_APP_OPEN_QUANTITY => {
            open_quantity(hwnd);
            0
        }
        WM_APP_CLOSE_QUANTITY => {
            let actions = ask(|l| l.close_requested(WindowId::Quantity, false));
            run_actions(actions, None);
            0
        }
        WM_APP_RAISE => {
            SetForegroundWindow(hwnd);
            0
        }
        WM_APP_QUIT => {
            DestroyWindow(hwnd);
            0
        }
        WM_DESTROY => {
            match id_of(hwnd) {
                Some(WindowId::Quantity) => {
                    let closing = with(|w| {
                        w.list.retain(|s| s.id != WindowId::Quantity);
                        w.quantity.hwnd.store(0, Ordering::Relaxed);
                        w.closing
                    })
                    .unwrap_or(true);
                    // Nicht über die Regeln geschlossen (etwa vom System): vergessen
                    if !closing {
                        if let Some(l) = with(|w| w.layout.clone()) {
                            if let Ok(mut l) = l.lock() {
                                l.quantity_gone();
                            }
                        }
                    }
                }
                _ => {
                    // Das Hauptfenster nimmt das Mengenfenster mit; offen bleibt gemerkt
                    with(|w| w.closing = true);
                    if let Some(q) = hwnd_of(WindowId::Quantity) {
                        DestroyWindow(q);
                    }
                    PostQuitMessage(0);
                }
            }
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
    /// Mengenfenster (Griff 0, solange es nicht offen ist).
    quantity: Arc<Shared>,
    /// Zeilen für die GDI-Kopie ins Mengenfenster (BGRA).
    quantity_buf: RefCell<Vec<u8>>,
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

fn command_to(hwnd: HWND, c: WindowCommand) {
    if hwnd.is_null() {
        return;
    }
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
            WindowCommand::Activate => {
                if IsIconic(hwnd) != 0 {
                    PostMessageW(hwnd, WM_SYSCOMMAND, SC_RESTORE, 0);
                }
                PostMessageW(hwnd, WM_APP_RAISE, 0, 0);
            }
        }
    }
}

fn set_caption_area_of(sh: &Shared, a: CaptionArea) {
    sh.caption_height.store(a.height, Ordering::Relaxed);
    sh.buttons_width.store(a.buttons_width, Ordering::Relaxed);
    sh.left_width.store(a.left_width, Ordering::Relaxed);
}

fn set_cursor_of(sh: &Shared, c: Cursor) {
    let i = cursor_index(c);
    let hwnd = sh.hwnd();
    if sh.cursor.swap(i, Ordering::Relaxed) != i && !hwnd.is_null() {
        unsafe {
            PostMessageW(hwnd, WM_APP_CURSOR, 0, 0);
        }
    }
}

fn set_title_of(hwnd: HWND, title: &str) {
    if hwnd.is_null() {
        return;
    }
    let t = wide(title);
    unsafe {
        SetWindowTextW(hwnd, t.as_ptr());
    }
}

fn cursor_index(c: Cursor) -> u32 {
    match c {
        Cursor::Arrow => 0,
        Cursor::SizeNS => 1,
        Cursor::Hand => 2,
        Cursor::IBeam => 3,
    }
}

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
        self.shared.mark_presented(w, h);
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
        command_to(self.hwnd as HWND, c)
    }

    pub fn set_title(&self, title: &str) {
        set_title_of(self.hwnd as HWND, title)
    }

    pub fn set_cursor(&self, c: Cursor) {
        set_cursor_of(&self.shared, c)
    }

    pub fn set_caption_area(&self, a: CaptionArea) {
        set_caption_area_of(&self.shared, a)
    }

    pub fn open_quantity(&self, title: &str) {
        if let Ok(mut t) = self.quantity.title.lock() {
            *t = title.to_string();
        }
        unsafe {
            PostMessageW(self.hwnd as HWND, WM_APP_OPEN_QUANTITY, 0, 0);
        }
    }

    pub fn close_quantity(&self) {
        unsafe {
            PostMessageW(self.hwnd as HWND, WM_APP_CLOSE_QUANTITY, 0, 0);
        }
    }

    /// Kopiert das Bild per GDI ins Mengenfenster (kein zweiter GL-Kontext:
    /// zwei Fenster mit Bildsynchronisation würden die Bildrate halbieren).
    pub fn present_quantity(&self, w: u32, h: u32, rgba: &[u8], rows: Option<(u32, u32)>) {
        let hwnd = self.quantity.hwnd();
        if hwnd.is_null() || w == 0 || h == 0 || rgba.len() < (w * h * 4) as usize {
            return;
        }
        let (y0, y1) = rows.map_or((0, h), |(a, b)| (a.min(h), b.min(h)));
        if y1 <= y0 {
            return;
        }
        let mut buf = self.quantity_buf.borrow_mut();
        let src = &rgba[(y0 * w * 4) as usize..(y1 * w * 4) as usize];
        buf.clear();
        buf.extend(src.chunks_exact(4).flat_map(|p| [p[2], p[1], p[0], 255]));
        let bmi = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: w as i32,
                // Negativ: Zeilen von oben nach unten
                biHeight: -((y1 - y0) as i32),
                biPlanes: 1,
                biBitCount: 32,
                ..Default::default()
            },
            bmiColors: [0; 4],
        };
        unsafe {
            let dc = GetDC(hwnd);
            if dc.is_null() {
                return;
            }
            SetDIBitsToDevice(
                dc,
                0,
                y0 as i32,
                w,
                y1 - y0,
                0,
                0,
                0,
                y1 - y0,
                buf.as_ptr() as *const c_void,
                &bmi,
                DIB_RGB_COLORS,
            );
            GdiFlush();
            ReleaseDC(hwnd, dc);
        }
        if rows.is_none() {
            self.quantity.mark_presented(w, h);
        }
    }

    pub fn quantity_size(&self) -> (u32, u32) {
        (
            self.quantity.width.load(Ordering::Relaxed),
            self.quantity.height.load(Ordering::Relaxed),
        )
    }

    pub fn quantity_scale(&self) -> f32 {
        self.quantity.dpi.load(Ordering::Relaxed) as f32 / 96.0
    }

    pub fn quantity_command(&self, c: WindowCommand) {
        command_to(self.quantity.hwnd(), c)
    }

    pub fn set_quantity_caption_area(&self, a: CaptionArea) {
        set_caption_area_of(&self.quantity, a)
    }

    pub fn set_quantity_cursor(&self, c: Cursor) {
        set_cursor_of(&self.quantity, c)
    }

    pub fn set_quantity_title(&self, title: &str) {
        if let Ok(mut t) = self.quantity.title.lock() {
            *t = title.to_string();
        }
        set_title_of(self.quantity.hwnd(), title)
    }

    pub fn monitors(&self) -> Vec<Rect> {
        monitors()
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

/// Fensterklassen: Hauptfenster mit eigenem Gerätekontext (OpenGL),
/// Mengenfenster ohne (GDI-Kopie aus dem Zeichenthread).
const MAIN_CLASS: &str = "SkizzeoFenster";
const QUANTITY_CLASS: &str = "SkizzeoMengenfenster";

unsafe fn register_class(inst: HINSTANCE, name: &str, style: u32) -> bool {
    let class = wide(name);
    let wc = WNDCLASSEXW {
        cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
        style,
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
    RegisterClassExW(&wc) != 0
}

pub fn run<F>(config: Config, layout: Arc<Mutex<Layout>>, app: F) -> Result<(), String>
where
    F: FnOnce(Receiver<(WindowId, Event)>, Surface) -> Result<(), String> + Send + 'static,
{
    set_dpi_awareness();
    unsafe {
        let inst = GetModuleHandleW(null());
        if !register_class(inst, MAIN_CLASS, CS_OWNDC | CS_HREDRAW | CS_VREDRAW)
            || !register_class(inst, QUANTITY_CLASS, CS_HREDRAW | CS_VREDRAW)
        {
            return Err("Fensterklasse konnte nicht angelegt werden.".into());
        }

        let (tx, rx) = channel();
        let shared = Shared::new();
        let quantity = Shared::new();
        WINDOWS.with(|s| {
            *s.borrow_mut() = Some(Windows {
                tx,
                list: vec![WndState {
                    id: WindowId::Main,
                    shared: shared.clone(),
                    tracking_leave: false,
                    buttons_down: 0,
                }],
                quantity: quantity.clone(),
                layout,
                cursors: [IDC_ARROW, IDC_SIZENS, IDC_HAND, IDC_IBEAM]
                    .map(|c| LoadCursorW(null_mut(), c as *const u16)),
                inst: inst as usize,
                icon: config.icon,
                closing: false,
            })
        });

        let class = wide(MAIN_CLASS);
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
        shared.hwnd.store(hwnd as usize, Ordering::Relaxed);
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
        decorate(hwnd, inst, config.icon, dpi);

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
            quantity,
            quantity_buf: RefCell::new(Vec::new()),
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
        WINDOWS.with(|s| *s.borrow_mut() = None); // Kanal schließen
        let _ = render.join();
        match err_rx.try_recv() {
            Ok(e) => Err(e),
            Err(_) => Ok(()),
        }
    }
}
