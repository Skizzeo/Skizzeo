//! OpenGL ohne Fenster (nur Linux, für Prüfbilder der App): ein
//! 3.3-Kontext über GLX auf einem Pbuffer, z. B. unter Xvfb mit Mesa. Alles
//! über `dlopen`, ohne fremden Code; die App selbst läuft nur unter Windows.

use crate::gl::Gl;
use std::ffi::{c_char, c_int, c_void, CStr};

extern "C" {
    fn dlopen(name: *const c_char, flags: c_int) -> *mut c_void;
    fn dlsym(lib: *mut c_void, name: *const c_char) -> *mut c_void;
}

const RTLD_NOW: c_int = 2;

// GLX
const GLX_DOUBLEBUFFER: c_int = 5;
const GLX_RED_SIZE: c_int = 8;
const GLX_GREEN_SIZE: c_int = 9;
const GLX_BLUE_SIZE: c_int = 10;
const GLX_ALPHA_SIZE: c_int = 11;
const GLX_DEPTH_SIZE: c_int = 12;
const GLX_DRAWABLE_TYPE: c_int = 0x8010;
const GLX_RENDER_TYPE: c_int = 0x8011;
const GLX_PBUFFER_BIT: c_int = 0x4;
const GLX_RGBA_BIT: c_int = 0x1;
const GLX_PBUFFER_HEIGHT: c_int = 0x8040;
const GLX_PBUFFER_WIDTH: c_int = 0x8041;
const GLX_CONTEXT_MAJOR_VERSION_ARB: c_int = 0x2091;
const GLX_CONTEXT_MINOR_VERSION_ARB: c_int = 0x2092;
const GLX_CONTEXT_PROFILE_MASK_ARB: c_int = 0x9126;
const GLX_CONTEXT_CORE_PROFILE_BIT_ARB: c_int = 0x1;

type Display = c_void;
type FbConfig = *mut c_void;

/// Ein laufender Kontext; bleibt gültig, solange der Wert lebt (der
/// Kontext wird am Prozessende freigegeben).
pub struct Kontext {
    pub gl: Gl,
}

unsafe fn sym<T>(lib: *mut c_void, name: &CStr) -> Option<T> {
    let p = dlsym(lib, name.as_ptr());
    (!p.is_null()).then(|| std::mem::transmute_copy::<*mut c_void, T>(&p))
}

/// Kontext mit einem Pbuffer `w` × `h`; `None` ohne X-Server (`DISPLAY`)
/// oder ohne GLX.
pub fn kontext(w: i32, h: i32) -> Option<Kontext> {
    unsafe {
        let x11 = dlopen(c"libX11.so.6".as_ptr(), RTLD_NOW);
        let libgl = dlopen(c"libGL.so.1".as_ptr(), RTLD_NOW);
        if x11.is_null() || libgl.is_null() {
            return None;
        }
        let open: unsafe extern "C" fn(*const c_char) -> *mut Display = sym(x11, c"XOpenDisplay")?;
        let screen: unsafe extern "C" fn(*mut Display) -> c_int = sym(x11, c"XDefaultScreen")?;
        let choose: unsafe extern "C" fn(
            *mut Display,
            c_int,
            *const c_int,
            *mut c_int,
        ) -> *mut FbConfig = sym(libgl, c"glXChooseFBConfig")?;
        let proc_addr: unsafe extern "C" fn(*const u8) -> *mut c_void =
            sym(libgl, c"glXGetProcAddressARB")?;
        let pbuffer: unsafe extern "C" fn(*mut Display, FbConfig, *const c_int) -> usize =
            sym(libgl, c"glXCreatePbuffer")?;
        let current: unsafe extern "C" fn(*mut Display, usize, usize, *mut c_void) -> c_int =
            sym(libgl, c"glXMakeContextCurrent")?;
        let p = proc_addr(c"glXCreateContextAttribsARB".as_ptr() as *const u8);
        if p.is_null() {
            return None;
        }
        let create: unsafe extern "C" fn(
            *mut Display,
            FbConfig,
            *mut c_void,
            c_int,
            *const c_int,
        ) -> *mut c_void = std::mem::transmute::<*mut c_void, _>(p);

        let dpy = open(std::ptr::null());
        if dpy.is_null() {
            return None;
        }
        let attr = [
            GLX_DRAWABLE_TYPE,
            GLX_PBUFFER_BIT,
            GLX_RENDER_TYPE,
            GLX_RGBA_BIT,
            GLX_RED_SIZE,
            8,
            GLX_GREEN_SIZE,
            8,
            GLX_BLUE_SIZE,
            8,
            GLX_ALPHA_SIZE,
            8,
            GLX_DEPTH_SIZE,
            24,
            GLX_DOUBLEBUFFER,
            1,
            0,
        ];
        let mut n = 0;
        let cfgs = choose(dpy, screen(dpy), attr.as_ptr(), &mut n);
        if cfgs.is_null() || n < 1 {
            return None;
        }
        let cfg = *cfgs;
        let ctx_attr = [
            GLX_CONTEXT_MAJOR_VERSION_ARB,
            3,
            GLX_CONTEXT_MINOR_VERSION_ARB,
            3,
            GLX_CONTEXT_PROFILE_MASK_ARB,
            GLX_CONTEXT_CORE_PROFILE_BIT_ARB,
            0,
        ];
        let ctx = create(dpy, cfg, std::ptr::null_mut(), 1, ctx_attr.as_ptr());
        if ctx.is_null() {
            return None;
        }
        let pb_attr = [GLX_PBUFFER_WIDTH, w, GLX_PBUFFER_HEIGHT, h, 0];
        let pb = pbuffer(dpy, cfg, pb_attr.as_ptr());
        if pb == 0 || current(dpy, pb, pb, ctx) == 0 {
            return None;
        }
        let gl = Gl::load(|name| proc_addr(name.as_ptr()) as *const c_void).ok()?;
        Some(Kontext { gl })
    }
}
