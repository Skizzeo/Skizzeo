//! Betriebssystem-Schicht: Fenster, Eingabe und OpenGL-Kontext.
//!
//! Keine fremden Crates. Die Systemfunktionen werden in den Untermodulen
//! selbst deklariert (`extern "system"`) und direkt aufgerufen.
//!
//! Aufbau: Der Hauptthread verarbeitet die Fensternachrichten des Systems und
//! schickt sie als [`Event`] über einen Kanal an den Zeichenthread. Dieser
//! besitzt den OpenGL-Kontext. So zeichnet die App auch weiter, während das
//! System das Fenster beim Ziehen oder Vergrößern festhält.

use std::ffi::c_void;
use std::sync::mpsc::{Receiver, RecvTimeoutError, TryRecvError};
use std::time::Duration;

#[cfg(windows)]
mod win32;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Modifiers {
    pub shift: bool,
    pub ctrl: bool,
    pub alt: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MouseButton {
    Left,
    Middle,
    Right,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Event {
    /// Schließen angefordert (Alt+F4, Taskleiste, eigener Schließen-Knopf).
    CloseRequested,
    /// Neue Größe der Zeichenfläche in Pixeln.
    Resized { width: u32, height: u32 },
    /// Bildschirmskalierung geändert (1,0 = 96 dpi).
    ScaleChanged(f32),
    Maximized(bool),
    Focus(bool),
    Redraw,
    MouseMove { x: f64, y: f64, mods: Modifiers },
    MouseDown { button: MouseButton, x: f64, y: f64, mods: Modifiers },
    MouseUp { button: MouseButton, x: f64, y: f64, mods: Modifiers },
    MouseLeave,
    /// Mausrad in Rasten (positiv = vom Benutzer weg gedreht).
    Wheel { delta: f64, x: f64, y: f64, mods: Modifiers },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WindowCommand {
    Minimize,
    ToggleMaximize,
    Close,
}

/// Bereiche der eigenen Titelleiste in Pixeln, damit das System Ziehen,
/// Doppelklick und Aero-Snap richtig behandelt.
#[derive(Clone, Copy, Debug, Default)]
pub struct CaptionArea {
    pub height: u32,
    /// Breite der Knopfgruppe am rechten Rand (gehört der App, nicht dem Ziehen).
    pub buttons_width: u32,
}

pub struct Config {
    pub title: String,
    pub width: u32,
    pub height: u32,
    /// Liefert das Programmsymbol als RGBA8 (nicht vormultipliziert) für eine Kantenlänge.
    pub icon: Option<fn(u32) -> Vec<u8>>,
}

/// Zugriff des Zeichenthreads auf Fenster und GPU.
pub struct Surface {
    events: Receiver<Event>,
    inner: SurfaceImpl,
}

#[cfg(windows)]
type SurfaceImpl = win32::Surface;
#[cfg(not(windows))]
type SurfaceImpl = ();

impl Surface {
    /// Wartet blockierend auf das nächste Ereignis. `None`, wenn das Fenster weg ist.
    pub fn wait_event(&self) -> Option<Event> {
        self.events.recv().ok()
    }

    pub fn wait_event_timeout(&self, d: Duration) -> Option<Event> {
        match self.events.recv_timeout(d) {
            Ok(e) => Some(e),
            Err(RecvTimeoutError::Timeout) | Err(RecvTimeoutError::Disconnected) => None,
        }
    }

    pub fn poll_event(&self) -> Option<Event> {
        match self.events.try_recv() {
            Ok(e) => Some(e),
            Err(TryRecvError::Empty) | Err(TryRecvError::Disconnected) => None,
        }
    }

    #[cfg(windows)]
    pub fn size(&self) -> (u32, u32) {
        self.inner.size()
    }

    #[cfg(windows)]
    pub fn scale(&self) -> f32 {
        self.inner.scale()
    }

    #[cfg(windows)]
    pub fn swap_buffers(&self) {
        self.inner.swap_buffers()
    }

    /// Adresse einer OpenGL-Funktion; `name` muss mit `\0` enden.
    #[cfg(windows)]
    pub fn gl_proc(&self, name: &str) -> *const c_void {
        self.inner.gl_proc(name)
    }

    #[cfg(windows)]
    pub fn command(&self, c: WindowCommand) {
        self.inner.command(c)
    }

    #[cfg(windows)]
    pub fn set_caption_area(&self, a: CaptionArea) {
        self.inner.set_caption_area(a)
    }

    #[cfg(not(windows))]
    pub fn size(&self) -> (u32, u32) {
        (0, 0)
    }
    #[cfg(not(windows))]
    pub fn scale(&self) -> f32 {
        1.0
    }
    #[cfg(not(windows))]
    pub fn swap_buffers(&self) {}
    #[cfg(not(windows))]
    pub fn gl_proc(&self, _name: &str) -> *const c_void {
        std::ptr::null()
    }
    #[cfg(not(windows))]
    pub fn command(&self, _c: WindowCommand) {}
    #[cfg(not(windows))]
    pub fn set_caption_area(&self, _a: CaptionArea) {}
}

/// Öffnet das Fenster und führt `app` im Zeichenthread aus, bis sie zurückkehrt.
pub fn run<F>(config: Config, app: F) -> Result<(), String>
where
    F: FnOnce(Surface) -> Result<(), String> + Send + 'static,
{
    #[cfg(windows)]
    {
        win32::run(config, move |events, inner| app(Surface { events, inner }))
    }
    #[cfg(not(windows))]
    {
        let _ = (config, app);
        Err("Skizzeo läuft derzeit nur unter Windows.".into())
    }
}

/// Zeigt eine Fehlermeldung an (unter Windows als Meldungsfenster).
pub fn show_error(msg: &str) {
    #[cfg(windows)]
    win32::message_box("Skizzeo", msg);
    #[cfg(not(windows))]
    eprintln!("Skizzeo: {msg}");
}
