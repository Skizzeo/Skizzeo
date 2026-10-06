//! Betriebssystem-Schicht: Fenster, Eingabe und OpenGL-Kontext.
//!
//! Keine fremden Crates. Die Systemfunktionen werden in den Untermodulen
//! selbst deklariert (`extern "system"`) und direkt aufgerufen.
//!
//! Aufbau: Der Hauptthread verarbeitet die Fensternachrichten des Systems und
//! schickt sie als [`Event`] über einen Kanal an den Zeichenthread. Dieser
//! besitzt den OpenGL-Kontext. So zeichnet die App auch weiter, während das
//! System das Fenster beim Ziehen oder Vergrößern festhält.
//!
//! Neben dem Hauptfenster kann es das Mengenfenster geben
//! ([`WindowId::Quantity`], F2). Beide haben keinen Besitzer und je einen
//! Taskleisteneintrag; die Schicht hält sie nach den Regeln in [`layout`]
//! zusammen (Andocken, Teilen, gemeinsam minimieren).

use std::ffi::c_void;
use std::path::PathBuf;
use std::sync::mpsc::{Receiver, RecvTimeoutError, TryRecvError};
use std::sync::{Arc, Mutex};
use std::time::Duration;

pub mod layout;
#[cfg(windows)]
mod win32;

pub use layout::{Rect, WindowId, Windows};

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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Key {
    Tab,
    Escape,
    Enter,
    Backspace,
    Delete,
    Shift,
    Control,
    Alt,
    Left,
    Right,
    Home,
    End,
    /// Buchstaben- und Zifferntasten als Großbuchstabe bzw. Ziffer (auch vom
    /// Ziffernblock), dazu `,` `.` `-` für Zahleneingaben.
    Char(char),
    /// Sonstige Taste mit dem Code des Betriebssystems.
    Other(u32),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Event {
    /// Schließen angefordert. `ask`: vom Nutzer (Alt+F4, Taskleiste, eigener
    /// Schließen-Knopf), dann darf nach ungespeicherten Änderungen gefragt
    /// werden. Sonst kam es von außen (z. B. `taskkill`) und schließt sofort.
    CloseRequested {
        ask: bool,
    },
    /// Neue Größe der Zeichenfläche in Pixeln.
    Resized {
        width: u32,
        height: u32,
    },
    /// Bildschirmskalierung geändert (1,0 = 96 dpi).
    ScaleChanged(f32),
    Maximized(bool),
    Focus(bool),
    Redraw,
    MouseMove {
        x: f64,
        y: f64,
        mods: Modifiers,
    },
    MouseDown {
        button: MouseButton,
        x: f64,
        y: f64,
        mods: Modifiers,
    },
    MouseUp {
        button: MouseButton,
        x: f64,
        y: f64,
        mods: Modifiers,
    },
    MouseLeave,
    /// Taste gedrückt (`down`, auch bei automatischer Wiederholung) oder losgelassen.
    Key {
        key: Key,
        down: bool,
        repeat: bool,
        mods: Modifiers,
    },
    /// Getipptes Zeichen (nach Tastaturbelegung, mit Umschalt), für Textfelder.
    Text(char),
    /// Mausrad in Rasten (positiv = vom Benutzer weg gedreht).
    Wheel {
        delta: f64,
        x: f64,
        y: f64,
        mods: Modifiers,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WindowCommand {
    Minimize,
    ToggleMaximize,
    Close,
}

/// Mauszeiger über der Zeichenfläche.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Cursor {
    #[default]
    Arrow,
    /// Senkrecht verschieben (Ebene ziehen).
    SizeNS,
    /// Klickbar (Maßzahl, Kote, Geschossname).
    Hand,
    /// Texteingabe.
    IBeam,
}

/// Bereiche der eigenen Titelleiste in Pixeln, damit das System Ziehen,
/// Doppelklick und Aero-Snap richtig behandelt.
#[derive(Clone, Copy, Debug, Default)]
pub struct CaptionArea {
    pub height: u32,
    /// Breite der Knopfgruppe am rechten Rand (gehört der App, nicht dem Ziehen).
    pub buttons_width: u32,
    /// Breite der Knopfgruppe am linken Rand (Menü, Rückgängig, E17).
    pub left_width: u32,
}

/// Dateityp im Dateidialog: Bezeichnung und Muster, z. B. `("Skizzeo-Projekt", "*.szo")`.
pub type FileFilter<'a> = (&'a str, &'a str);

pub struct Config {
    pub title: String,
    pub width: u32,
    pub height: u32,
    /// Liefert das Programmsymbol als RGBA8 (nicht vormultipliziert) für eine Kantenlänge.
    pub icon: Option<fn(u32) -> Vec<u8>>,
}

/// Zugriff des Zeichenthreads auf Fenster und GPU.
pub struct Surface {
    events: Receiver<(WindowId, Event)>,
    layout: Arc<Mutex<Windows>>,
    #[cfg_attr(not(windows), allow(dead_code))]
    inner: SurfaceImpl,
}

#[cfg(windows)]
type SurfaceImpl = win32::Surface;
#[cfg(not(windows))]
type SurfaceImpl = ();

impl Surface {
    /// Wartet blockierend auf das nächste Ereignis. `None`, wenn das Fenster weg ist.
    pub fn wait_event(&self) -> Option<(WindowId, Event)> {
        self.events.recv().ok()
    }

    /// Wartet höchstens `d` auf das nächste Ereignis: `Some(None)` nach Ablauf,
    /// `None`, wenn das Fenster weg ist.
    pub fn wait_event_timeout(&self, d: Duration) -> Option<Option<(WindowId, Event)>> {
        match self.events.recv_timeout(d) {
            Ok(e) => Some(Some(e)),
            Err(RecvTimeoutError::Timeout) => Some(None),
            Err(RecvTimeoutError::Disconnected) => None,
        }
    }

    pub fn poll_event(&self) -> Option<(WindowId, Event)> {
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

    /// Zeigt das gezeichnete Bild; `w` × `h` ist die Größe, für die es gezeichnet wurde.
    #[cfg(windows)]
    pub fn swap_buffers(&self, w: u32, h: u32) {
        self.inner.swap_buffers(w, h)
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

    /// Mauszeiger über der Zeichenfläche; kostet nichts, wenn er schon gilt.
    #[cfg(windows)]
    pub fn set_cursor(&self, c: Cursor) {
        self.inner.set_cursor(c)
    }

    /// Fenstertitel für Taskleiste und Alt+Tab.
    #[cfg(windows)]
    pub fn set_title(&self, title: &str) {
        self.inner.set_title(title)
    }

    /// Systemdialog „Öffnen“. `None`, wenn abgebrochen.
    #[cfg(windows)]
    pub fn open_dialog(&self, title: &str, filters: &[FileFilter]) -> Option<PathBuf> {
        self.inner.file_dialog(false, title, filters, "", "")
    }

    /// Systemdialog „Speichern unter“ mit Vorschlag `suggested` und Endung
    /// `default_ext` (ohne Punkt), fragt vor dem Überschreiben.
    #[cfg(windows)]
    pub fn save_dialog(
        &self,
        title: &str,
        filters: &[FileFilter],
        default_ext: &str,
        suggested: &str,
    ) -> Option<PathBuf> {
        self.inner
            .file_dialog(true, title, filters, default_ext, suggested)
    }

    /// Meldung mit OK-Knopf; `error` wählt das Fehlersymbol statt des Hinweises.
    #[cfg(windows)]
    pub fn message(&self, text: &str, error: bool) {
        self.inner.message(text, error)
    }

    /// Öffnet das Mengenfenster nach den Regeln in [`layout`] (angedockt, an
    /// seinem gemerkten Platz oder bei maximiertem Hauptfenster geteilt). Ist
    /// es schon offen, kommt es nach vorn. Seine Ereignisse tragen
    /// [`WindowId::Quantity`]; zuerst kommen `ScaleChanged` und `Resized`.
    #[cfg(windows)]
    pub fn open_quantity(&self, title: &str) {
        self.inner.open_quantity(title)
    }

    /// Schließt das Mengenfenster; ein vorher maximiertes Hauptfenster wird
    /// wieder maximiert.
    #[cfg(windows)]
    pub fn close_quantity(&self) {
        self.inner.close_quantity()
    }

    /// Zeigt ein Bild im Mengenfenster: `rgba` vormultipliziert und deckend,
    /// `w` × `h`. `rows`: nur diese Zeilen (von, bis) neu kopieren.
    #[cfg(windows)]
    pub fn present_quantity(&self, w: u32, h: u32, rgba: &[u8], rows: Option<(u32, u32)>) {
        self.inner.present_quantity(w, h, rgba, rows)
    }

    #[cfg(windows)]
    pub fn quantity_size(&self) -> (u32, u32) {
        self.inner.quantity_size()
    }

    #[cfg(windows)]
    pub fn quantity_scale(&self) -> f32 {
        self.inner.quantity_scale()
    }

    #[cfg(windows)]
    pub fn quantity_command(&self, c: WindowCommand) {
        self.inner.quantity_command(c)
    }

    #[cfg(windows)]
    pub fn set_quantity_caption_area(&self, a: CaptionArea) {
        self.inner.set_quantity_caption_area(a)
    }

    #[cfg(windows)]
    pub fn set_quantity_cursor(&self, c: Cursor) {
        self.inner.set_quantity_cursor(c)
    }

    #[cfg(windows)]
    pub fn set_quantity_title(&self, title: &str) {
        self.inner.set_quantity_title(title)
    }

    /// Arbeitsbereiche aller Bildschirme (ohne Taskleiste).
    #[cfg(windows)]
    pub fn monitors(&self) -> Vec<Rect> {
        self.inner.monitors()
    }

    /// Zustand der beiden Fenster zueinander (zum Merken in den Einstellungen
    /// und zum Setzen beim Start).
    pub fn layout(&self) -> std::sync::MutexGuard<'_, Windows> {
        match self.layout.lock() {
            Ok(g) => g,
            Err(e) => e.into_inner(),
        }
    }

    #[cfg(not(windows))]
    pub fn open_quantity(&self, _title: &str) {}
    #[cfg(not(windows))]
    pub fn close_quantity(&self) {}
    #[cfg(not(windows))]
    pub fn present_quantity(&self, _w: u32, _h: u32, _rgba: &[u8], _rows: Option<(u32, u32)>) {}
    #[cfg(not(windows))]
    pub fn quantity_size(&self) -> (u32, u32) {
        (0, 0)
    }
    #[cfg(not(windows))]
    pub fn quantity_scale(&self) -> f32 {
        1.0
    }
    #[cfg(not(windows))]
    pub fn quantity_command(&self, _c: WindowCommand) {}
    #[cfg(not(windows))]
    pub fn set_quantity_caption_area(&self, _a: CaptionArea) {}
    #[cfg(not(windows))]
    pub fn set_quantity_cursor(&self, _c: Cursor) {}
    #[cfg(not(windows))]
    pub fn set_quantity_title(&self, _title: &str) {}
    #[cfg(not(windows))]
    pub fn monitors(&self) -> Vec<Rect> {
        Vec::new()
    }

    #[cfg(not(windows))]
    pub fn set_cursor(&self, _c: Cursor) {}
    #[cfg(not(windows))]
    pub fn set_title(&self, _title: &str) {}
    #[cfg(not(windows))]
    pub fn open_dialog(&self, _title: &str, _filters: &[FileFilter]) -> Option<PathBuf> {
        None
    }
    #[cfg(not(windows))]
    pub fn save_dialog(
        &self,
        _title: &str,
        _filters: &[FileFilter],
        _default_ext: &str,
        _suggested: &str,
    ) -> Option<PathBuf> {
        None
    }
    #[cfg(not(windows))]
    pub fn message(&self, text: &str, _error: bool) {
        eprintln!("Skizzeo: {text}");
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
    pub fn swap_buffers(&self, _w: u32, _h: u32) {}
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
        let layout = Arc::new(Mutex::new(Windows::new(layout::WIDTH_DIP)));
        win32::run(config, layout.clone(), move |events, inner| {
            app(Surface {
                events,
                layout,
                inner,
            })
        })
    }
    #[cfg(not(windows))]
    {
        let _ = (config, app);
        Err("Skizzeo läuft derzeit nur unter Windows.".into())
    }
}

/// Ortszeit (Stunde, Minute); außerhalb von Windows UTC.
pub fn local_time() -> (u8, u8) {
    #[cfg(windows)]
    {
        win32::local_time()
    }
    #[cfg(not(windows))]
    {
        let secs = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs());
        (((secs / 3600) % 24) as u8, ((secs / 60) % 60) as u8)
    }
}

/// Text aus der Zwischenablage. Außerhalb von Windows eine Ablage nur für
/// diesen Prozess (Tests).
pub fn clipboard_text() -> Option<String> {
    #[cfg(windows)]
    {
        win32::clipboard_text()
    }
    #[cfg(not(windows))]
    {
        CLIPBOARD.lock().ok().and_then(|c| c.clone())
    }
}

/// Legt Text in die Zwischenablage.
pub fn set_clipboard_text(text: &str) {
    #[cfg(windows)]
    win32::set_clipboard_text(text);
    #[cfg(not(windows))]
    if let Ok(mut c) = CLIPBOARD.lock() {
        *c = Some(text.to_string());
    }
}

#[cfg(not(windows))]
static CLIPBOARD: std::sync::Mutex<Option<String>> = std::sync::Mutex::new(None);

/// Zeigt eine Fehlermeldung an (unter Windows als Meldungsfenster).
pub fn show_error(msg: &str) {
    #[cfg(windows)]
    win32::message_box("Skizzeo", msg);
    #[cfg(not(windows))]
    eprintln!("Skizzeo: {msg}");
}
