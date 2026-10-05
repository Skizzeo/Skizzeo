//! Selbst geschriebene OpenGL-3.3-Anbindung: nur die Funktionen, die Skizzeo braucht.

#![allow(non_snake_case, clippy::missing_safety_doc, clippy::too_many_arguments)]

use std::ffi::c_void;

pub type GLenum = u32;
pub type GLuint = u32;
pub type GLint = i32;
pub type GLsizei = i32;
pub type GLfloat = f32;
pub type GLboolean = u8;
pub type GLbitfield = u32;
pub type GLsizeiptr = isize;
pub type GLchar = i8;

pub const COLOR_BUFFER_BIT: GLbitfield = 0x4000;
pub const DEPTH_BUFFER_BIT: GLbitfield = 0x0100;
pub const DEPTH_TEST: GLenum = 0x0B71;
pub const BLEND: GLenum = 0x0BE2;
pub const CULL_FACE: GLenum = 0x0B44;
pub const SCISSOR_TEST: GLenum = 0x0C11;
pub const POLYGON_OFFSET_FILL: GLenum = 0x8037;
pub const MULTISAMPLE: GLenum = 0x809D;
pub const FRAMEBUFFER_SRGB: GLenum = 0x8DB9;
pub const LESS: GLenum = 0x0201;
pub const LEQUAL: GLenum = 0x0203;
pub const ALWAYS: GLenum = 0x0207;
pub const ONE: GLenum = 1;
pub const ONE_MINUS_SRC_ALPHA: GLenum = 0x0303;
pub const TRIANGLES: GLenum = 0x0004;
pub const ARRAY_BUFFER: GLenum = 0x8892;
pub const STATIC_DRAW: GLenum = 0x88E4;
pub const DYNAMIC_DRAW: GLenum = 0x88E8;
pub const FLOAT: GLenum = 0x1406;
pub const UNSIGNED_BYTE: GLenum = 0x1401;
pub const FALSE: GLboolean = 0;
pub const TRUE: GLboolean = 1;
pub const VERTEX_SHADER: GLenum = 0x8B31;
pub const FRAGMENT_SHADER: GLenum = 0x8B30;
pub const COMPILE_STATUS: GLenum = 0x8B81;
pub const LINK_STATUS: GLenum = 0x8B82;
pub const INFO_LOG_LENGTH: GLenum = 0x8B84;
pub const FRAMEBUFFER: GLenum = 0x8D40;
pub const READ_FRAMEBUFFER: GLenum = 0x8CA8;
pub const DRAW_FRAMEBUFFER: GLenum = 0x8CA9;
pub const RENDERBUFFER: GLenum = 0x8D41;
pub const RGBA8: GLenum = 0x8058;
pub const RGBA: GLenum = 0x1908;
pub const DEPTH_COMPONENT24: GLenum = 0x81A6;
pub const COLOR_ATTACHMENT0: GLenum = 0x8CE0;
pub const DEPTH_ATTACHMENT: GLenum = 0x8D00;
pub const FRAMEBUFFER_COMPLETE: GLenum = 0x8CD5;
pub const NEAREST: GLint = 0x2600;
pub const LINEAR: GLint = 0x2601;
pub const CLAMP_TO_EDGE: GLint = 0x812F;
pub const TEXTURE_2D: GLenum = 0x0DE1;
pub const TEXTURE0: GLenum = 0x84C0;
pub const TEXTURE_MIN_FILTER: GLenum = 0x2801;
pub const TEXTURE_MAG_FILTER: GLenum = 0x2800;
pub const TEXTURE_WRAP_S: GLenum = 0x2802;
pub const TEXTURE_WRAP_T: GLenum = 0x2803;
pub const PACK_ALIGNMENT: GLenum = 0x0D05;
pub const UNPACK_ALIGNMENT: GLenum = 0x0CF5;
pub const MAX_SAMPLES: GLenum = 0x8D57;
pub const VERSION: GLenum = 0x1F02;
pub const RENDERER: GLenum = 0x1F01;
pub const BACK: GLenum = 0x0405;

macro_rules! gl_api {
    ($( fn $name:ident ( $($arg:ident : $ty:ty),* ) $(-> $ret:ty)? ; )*) => {
        /// Tabelle der geladenen OpenGL-Funktionen.
        pub struct Gl {
            $( $name: unsafe extern "system" fn($($ty),*) $(-> $ret)?, )*
        }

        impl Gl {
            /// Lädt alle Funktionen über `get` (Name endet mit `\0`).
            pub fn load(mut get: impl FnMut(&str) -> *const c_void) -> Result<Gl, String> {
                Ok(Gl {
                    $( $name: {
                        let p = get(concat!(stringify!($name), "\0"));
                        if p.is_null() {
                            return Err(format!(
                                "Der Grafiktreiber kennt die OpenGL-Funktion {} nicht.",
                                stringify!($name)
                            ));
                        }
                        // SAFETY: Zeiger kommt vom Treiber und hat diese Signatur laut OpenGL-Spezifikation.
                        unsafe {
                            std::mem::transmute::<*const c_void, unsafe extern "system" fn($($ty),*) $(-> $ret)?>(p)
                        }
                    }, )*
                })
            }

            $(
                #[inline]
                pub unsafe fn $name(&self, $($arg: $ty),*) $(-> $ret)? {
                    (self.$name)($($arg),*)
                }
            )*
        }
    };
}

gl_api! {
    fn glGetString(name: GLenum) -> *const u8;
    fn glGetIntegerv(name: GLenum, data: *mut GLint);
    fn glGetError() -> GLenum;
    fn glViewport(x: GLint, y: GLint, w: GLsizei, h: GLsizei);
    fn glScissor(x: GLint, y: GLint, w: GLsizei, h: GLsizei);
    fn glClearColor(r: GLfloat, g: GLfloat, b: GLfloat, a: GLfloat);
    fn glClearDepth(d: f64);
    fn glClear(mask: GLbitfield);
    fn glEnable(cap: GLenum);
    fn glDisable(cap: GLenum);
    fn glDepthFunc(f: GLenum);
    fn glDepthMask(m: GLboolean);
    fn glBlendFunc(s: GLenum, d: GLenum);
    fn glPolygonOffset(factor: GLfloat, units: GLfloat);
    fn glPixelStorei(p: GLenum, v: GLint);
    fn glReadBuffer(m: GLenum);
    fn glReadPixels(x: GLint, y: GLint, w: GLsizei, h: GLsizei, f: GLenum, t: GLenum, d: *mut c_void);
    fn glDrawArrays(mode: GLenum, first: GLint, count: GLsizei);

    fn glCreateShader(kind: GLenum) -> GLuint;
    fn glShaderSource(s: GLuint, n: GLsizei, src: *const *const GLchar, len: *const GLint);
    fn glCompileShader(s: GLuint);
    fn glGetShaderiv(s: GLuint, p: GLenum, v: *mut GLint);
    fn glGetShaderInfoLog(s: GLuint, max: GLsizei, len: *mut GLsizei, log: *mut GLchar);
    fn glDeleteShader(s: GLuint);
    fn glCreateProgram() -> GLuint;
    fn glAttachShader(p: GLuint, s: GLuint);
    fn glLinkProgram(p: GLuint);
    fn glGetProgramiv(p: GLuint, n: GLenum, v: *mut GLint);
    fn glGetProgramInfoLog(p: GLuint, max: GLsizei, len: *mut GLsizei, log: *mut GLchar);
    fn glUseProgram(p: GLuint);
    fn glGetUniformLocation(p: GLuint, name: *const GLchar) -> GLint;
    fn glUniform1i(l: GLint, v: GLint);
    fn glUniform1f(l: GLint, v: GLfloat);
    fn glUniform2f(l: GLint, a: GLfloat, b: GLfloat);
    fn glUniform3f(l: GLint, a: GLfloat, b: GLfloat, c: GLfloat);
    fn glUniform4f(l: GLint, a: GLfloat, b: GLfloat, c: GLfloat, d: GLfloat);
    fn glUniform1fv(l: GLint, n: GLsizei, v: *const GLfloat);
    fn glUniform3fv(l: GLint, n: GLsizei, v: *const GLfloat);
    fn glUniformMatrix4fv(l: GLint, n: GLsizei, transpose: GLboolean, v: *const GLfloat);

    fn glGenVertexArrays(n: GLsizei, out: *mut GLuint);
    fn glBindVertexArray(v: GLuint);
    fn glGenBuffers(n: GLsizei, out: *mut GLuint);
    fn glBindBuffer(t: GLenum, b: GLuint);
    fn glBufferData(t: GLenum, size: GLsizeiptr, data: *const c_void, usage: GLenum);
    fn glVertexAttribPointer(i: GLuint, n: GLint, t: GLenum, norm: GLboolean, stride: GLsizei, off: *const c_void);
    fn glEnableVertexAttribArray(i: GLuint);

    fn glGenFramebuffers(n: GLsizei, out: *mut GLuint);
    fn glDeleteFramebuffers(n: GLsizei, f: *const GLuint);
    fn glBindFramebuffer(t: GLenum, f: GLuint);
    fn glCheckFramebufferStatus(t: GLenum) -> GLenum;
    fn glFramebufferRenderbuffer(t: GLenum, a: GLenum, rt: GLenum, rb: GLuint);
    fn glGenRenderbuffers(n: GLsizei, out: *mut GLuint);
    fn glDeleteRenderbuffers(n: GLsizei, r: *const GLuint);
    fn glBindRenderbuffer(t: GLenum, r: GLuint);
    fn glRenderbufferStorageMultisample(t: GLenum, samples: GLsizei, f: GLenum, w: GLsizei, h: GLsizei);
    fn glBlitFramebuffer(sx0: GLint, sy0: GLint, sx1: GLint, sy1: GLint, dx0: GLint, dy0: GLint, dx1: GLint, dy1: GLint, mask: GLbitfield, filter: GLenum);

    fn glGenTextures(n: GLsizei, out: *mut GLuint);
    fn glBindTexture(t: GLenum, tex: GLuint);
    fn glActiveTexture(t: GLenum);
    fn glTexParameteri(t: GLenum, p: GLenum, v: GLint);
    fn glTexImage2D(t: GLenum, level: GLint, ifmt: GLint, w: GLsizei, h: GLsizei, border: GLint, f: GLenum, ty: GLenum, d: *const c_void);
}
