//! Dateimenü am Logo, Liste „Zuletzt geöffnet“, Tastenkürzel und die Nachfrage
//! „Änderungen speichern?“ im eigenen UI (E17).
//!
//! Alles hier ist Zustand und Zeichnen ohne Fenster; `main.rs` führt die
//! Befehle aus.

use crate::document::Document;
use crate::scene::Scene;
use sk_model::Model;
use sk_paint::{Canvas, Path, Rgba};
use sk_platform::{Key, Modifiers};
use sk_ui::theme::Theme;
use sk_ui::widgets::{self, ButtonState, Fonts, Rect};
use std::path::{Path as FsPath, PathBuf};

/// Befehle aus Menü, Titelleiste und Tastenkürzeln.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Command {
    New,
    Open,
    /// Eintrag der Liste „Zuletzt geöffnet“ (0 = neueste).
    OpenRecent(usize),
    Save,
    SaveAs,
    Close,
    Quit,
    Undo,
    Redo,
    OpenMenu,
    ClearRecent,
    /// Einstellungsfenster (E5).
    Settings,
    /// Bauteilkatalog (K3).
    Catalog,
    /// Fenster „Baustoffe …“ (Paket 5).
    Materials,
    /// Auswahl löschen (Entf, Paket „Löschen“).
    Delete,
    /// Liste „Sicherungen …“ (F-13).
    Backups,
    /// Sicherung Nummer `i` der zuletzt gezeigten Liste öffnen.
    OpenBackup(usize),
}

/// Antwort der Nachfrage „Änderungen speichern?“.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SaveAnswer {
    Save,
    Discard,
    Cancel,
}

/// Höchstens so viele Dateien in „Zuletzt geöffnet“.
pub const RECENT_MAX: usize = 8;

/// Zuletzt geöffnete oder gespeicherte Dateien, neueste oben.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Recent {
    files: Vec<PathBuf>,
}

impl Recent {
    /// Trägt eine Datei oben ein; steht sie schon in der Liste, rückt sie nach oben.
    pub fn push(&mut self, p: PathBuf) {
        let key = |p: &FsPath| p.to_string_lossy().to_lowercase();
        let k = key(&p);
        self.files.retain(|f| key(f) != k);
        self.files.insert(0, p);
        self.files.truncate(RECENT_MAX);
    }

    pub fn remove(&mut self, i: usize) {
        if i < self.files.len() {
            self.files.remove(i);
        }
    }

    pub fn clear(&mut self) {
        self.files.clear();
    }

    pub fn get(&self, i: usize) -> Option<&PathBuf> {
        self.files.get(i)
    }

    pub fn paths(&self) -> &[PathBuf] {
        &self.files
    }
}

/// Eine Zeile des Menüs.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MenuItem {
    pub label: String,
    /// Kürzel rechtsbündig („Strg+N“), „▸“ für das Untermenü.
    pub shortcut: String,
    /// Zweite Zeile im Untermenü: Ordner bzw. „nicht gefunden“.
    pub detail: String,
    pub enabled: bool,
    pub separator: bool,
    pub command: Option<Command>,
    /// Unumkehrbar Großes in `ui.danger` („Gebäude löschen …“).
    pub danger: bool,
}

fn item(label: &str, shortcut: &str, command: Command, enabled: bool) -> MenuItem {
    MenuItem {
        label: label.into(),
        shortcut: shortcut.into(),
        enabled,
        command: Some(command),
        ..MenuItem::default()
    }
}

pub(crate) fn separator() -> MenuItem {
    MenuItem {
        separator: true,
        ..MenuItem::default()
    }
}

/// Zeile „Zuletzt geöffnet“ im Hauptmenü.
const RECENT_ROW: usize = 2;

/// Abstände im Menü (dip): Rand oben und unten, Trennlinie, Zeile einer
/// Datei im Untermenü (zwei Zeilen), Text links und rechts.
pub(crate) const PAD: f32 = 4.0;
pub(crate) const SEP_H: f32 = 9.0;
const FILE_ROW: f32 = 44.0;
const INSET: f32 = 12.0;

/// Ordner höchstens so viele Zeichen (Untermenü 320 dip).
const FOLDER_CHARS: usize = 44;

/// Was die Maus im Menü trifft.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Hit {
    Main(usize),
    Sub(usize),
    /// Im Menü, aber auf keiner Zeile (Rand, Trennlinie).
    Inside,
    Outside,
}

/// Das Dateimenü: offen, markierte Zeile, Untermenü „Zuletzt geöffnet“.
#[derive(Clone, Debug, Default)]
pub struct FileMenu {
    open: bool,
    sel: Option<usize>,
    sub_open: bool,
    sub_sel: Option<usize>,
    /// Gedrückt im Menü (der Befehl folgt beim Loslassen).
    pressed: bool,
}

impl FileMenu {
    pub fn is_open(&self) -> bool {
        self.open
    }

    #[cfg(test)]
    pub fn sub_open(&self) -> bool {
        self.open && self.sub_open
    }

    /// Öffnet das Menü mit markierter erster Zeile.
    pub fn open(&mut self) {
        *self = FileMenu {
            open: true,
            sel: Some(0),
            ..FileMenu::default()
        };
    }

    pub fn close(&mut self) {
        *self = FileMenu::default();
    }

    /// Klick neben das Menü: zu, ohne Wirkung.
    #[cfg(test)]
    pub fn click_outside(&mut self) {
        self.close();
    }

    /// Zeilen des Hauptmenüs. `save`: „Speichern“ ist möglich.
    pub fn items(&self, save: bool, _recent: &Recent) -> Vec<MenuItem> {
        vec![
            item("Neu", "Strg+N", Command::New, true),
            item("Öffnen …", "Strg+O", Command::Open, true),
            MenuItem {
                label: "Zuletzt geöffnet".into(),
                shortcut: "▸".into(),
                enabled: true,
                ..MenuItem::default()
            },
            item("Sicherungen …", "", Command::Backups, true),
            separator(),
            item("Speichern", "Strg+S", Command::Save, save),
            item(
                "Speichern unter …",
                "Strg+Umschalt+S",
                Command::SaveAs,
                true,
            ),
            separator(),
            item("Einstellungen …", "Strg+Komma", Command::Settings, true),
            item("Bauteilkatalog …", "", Command::Catalog, true),
            item("Baustoffe …", "", Command::Materials, true),
            separator(),
            item("Schließen", "Strg+W", Command::Close, true),
            item("Beenden", "Alt+F4", Command::Quit, true),
        ]
    }

    /// Zeilen des Untermenüs: die Dateien, dann „Liste leeren“; leer nur
    /// „Keine Dateien“ (ausgegraut).
    pub fn sub_items(&self, recent: &Recent) -> Vec<MenuItem> {
        if recent.paths().is_empty() {
            return vec![MenuItem {
                label: "Keine Dateien".into(),
                ..MenuItem::default()
            }];
        }
        let mut v: Vec<MenuItem> = recent
            .paths()
            .iter()
            .enumerate()
            .map(|(i, p)| {
                let found = p.is_file();
                MenuItem {
                    label: file_name(p),
                    detail: if found {
                        short_folder(p.parent().unwrap_or(FsPath::new("")), FOLDER_CHARS)
                    } else {
                        "nicht gefunden".into()
                    },
                    enabled: found,
                    command: Some(Command::OpenRecent(i)),
                    ..MenuItem::default()
                }
            })
            .collect();
        v.push(item("Liste leeren", "", Command::ClearRecent, true));
        v
    }

    /// Taste im offenen Menü: Pfeile hoch und runter, rechts öffnet
    /// „Zuletzt geöffnet“, links schließt es, Enter führt aus, Esc schließt.
    pub fn key(&mut self, k: Key, save: bool, recent: &Recent) -> Option<Command> {
        if !self.open {
            return None;
        }
        let items = self.items(save, recent);
        let sub = self.sub_items(recent);
        match k {
            Key::Other(0x28) | Key::Other(0x26) => {
                let down = k == Key::Other(0x28);
                if self.sub_open {
                    self.sub_sel = step(&sub, self.sub_sel, down);
                } else {
                    self.sel = step(&items, self.sel, down);
                }
            }
            Key::Right if !self.sub_open && self.sel == Some(RECENT_ROW) => self.open_sub(&sub),
            Key::Left if self.sub_open => self.sub_open = false,
            Key::Escape if self.sub_open => self.sub_open = false,
            Key::Escape => self.close(),
            Key::Enter => {
                if self.sub_open {
                    let c = self
                        .sub_sel
                        .and_then(|i| sub.get(i))
                        .filter(|i| i.enabled)
                        .and_then(|i| i.command);
                    if c.is_some() {
                        self.close();
                    }
                    return c;
                }
                match self.sel {
                    Some(RECENT_ROW) => self.open_sub(&sub),
                    Some(i) => {
                        let c = items.get(i).filter(|i| i.enabled).and_then(|i| i.command);
                        if c.is_some() {
                            self.close();
                        }
                        return c;
                    }
                    None => {}
                }
            }
            _ => {}
        }
        None
    }

    fn open_sub(&mut self, sub: &[MenuItem]) {
        self.sub_open = true;
        self.sub_sel = sub.iter().position(|i| i.enabled);
    }

    /// Lage des Hauptmenüs und des Untermenüs im Fenster (Pixel, ohne Schatten).
    fn layout(&self, g: &Geo, save: bool, recent: &Recent) -> Layout {
        let s = g.scale;
        let items = self.items(save, recent);
        let mut rows = Vec::new();
        let mut y = g.top + PAD * s;
        for it in &items {
            let h = if it.separator { SEP_H } else { g.row } * s;
            rows.push((y, h));
            y += h;
        }
        let main = Rect::new(0.0, g.top, g.w * s, y + PAD * s - g.top);
        let sub_items = self.sub_items(recent);
        let sx = main.w - 2.0 * s;
        let sy = rows[RECENT_ROW].0 - PAD * s;
        let mut sub_rows = Vec::new();
        let mut y = sy + PAD * s;
        for (i, it) in sub_items.iter().enumerate() {
            // Über „Liste leeren“ eine Trennlinie
            if i > 0 && it.command == Some(Command::ClearRecent) {
                y += SEP_H * s;
            }
            let file = matches!(it.command, Some(Command::OpenRecent(_)));
            let h = if file { FILE_ROW } else { g.row } * s;
            sub_rows.push((y, h));
            y += h;
        }
        let sub = Rect::new(sx, sy, g.sub_w * s, y + PAD * s - sy);
        Layout {
            main,
            rows,
            sub,
            sub_rows,
        }
    }

    fn hit(&self, g: &Geo, save: bool, recent: &Recent, x: f64, y: f64) -> Hit {
        let l = self.layout(g, save, recent);
        let within = |r: &Rect, rows: &[(f32, f32)]| {
            r.contains(x, y).then(|| {
                rows.iter()
                    .position(|&(ry, rh)| y >= ry as f64 && y < (ry + rh) as f64)
            })
        };
        if self.sub_open {
            if let Some(i) = within(&l.sub, &l.sub_rows) {
                return i.map_or(Hit::Inside, Hit::Sub);
            }
        }
        match within(&l.main, &l.rows) {
            Some(Some(i)) => Hit::Main(i),
            Some(None) => Hit::Inside,
            None => Hit::Outside,
        }
    }

    /// Maus bewegt: markiert die Zeile darunter; über „Zuletzt geöffnet“ öffnet
    /// sich das Untermenü. `true`, wenn neu zu zeichnen ist.
    pub fn mouse_move(&mut self, g: &Geo, save: bool, recent: &Recent, x: f64, y: f64) -> bool {
        if !self.open {
            return false;
        }
        let before = (self.sel, self.sub_open, self.sub_sel);
        let items = self.items(save, recent);
        match self.hit(g, save, recent, x, y) {
            Hit::Main(i) => {
                let it = &items[i];
                self.sel = (!it.separator && it.enabled).then_some(i);
                if i == RECENT_ROW {
                    if !self.sub_open {
                        self.sub_open = true;
                        self.sub_sel = None;
                    }
                } else if !it.separator {
                    self.sub_open = false;
                }
            }
            Hit::Sub(i) => {
                self.sel = Some(RECENT_ROW);
                self.sub_sel = Some(i);
            }
            Hit::Inside | Hit::Outside => {
                if self.sub_open {
                    self.sub_sel = None;
                }
            }
        }
        before != (self.sel, self.sub_open, self.sub_sel)
    }

    /// Maustaste gedrückt. `false`: daneben, das Menü ist zu (der Klick
    /// bewirkt sonst nichts).
    pub fn press(&mut self, g: &Geo, save: bool, recent: &Recent, x: f64, y: f64) -> bool {
        if self.hit(g, save, recent, x, y) == Hit::Outside {
            self.close();
            return false;
        }
        self.pressed = true;
        true
    }

    /// Maustaste losgelassen: Befehl der Zeile. Eine fehlende Datei liefert
    /// ihren Platz, damit sie aus der Liste fällt.
    pub fn release(
        &mut self,
        g: &Geo,
        save: bool,
        recent: &Recent,
        x: f64,
        y: f64,
    ) -> Option<Command> {
        if !std::mem::take(&mut self.pressed) {
            return None;
        }
        let c = match self.hit(g, save, recent, x, y) {
            Hit::Main(i) => self
                .items(save, recent)
                .get(i)
                .filter(|i| i.enabled)
                .and_then(|i| i.command),
            Hit::Sub(i) => self
                .sub_items(recent)
                .get(i)
                .filter(|i| i.enabled || matches!(i.command, Some(Command::OpenRecent(_))))
                .and_then(|i| i.command),
            _ => None,
        };
        if c.is_some() {
            self.close();
        }
        c
    }

    /// Bild des Menüs samt Untermenü und Schatten, mit seiner Lage im Fenster.
    pub fn paint(
        &self,
        t: &Theme,
        fonts: &Fonts,
        g: &Geo,
        save: bool,
        recent: &Recent,
    ) -> (Canvas, i32, i32) {
        let s = g.scale;
        let l = self.layout(g, save, recent);
        let m = (t.size.panel_shadow * s).round();
        let right = if self.sub_open {
            l.sub.x + l.sub.w
        } else {
            l.main.w
        };
        let bottom = if self.sub_open {
            (l.main.y + l.main.h).max(l.sub.y + l.sub.h)
        } else {
            l.main.y + l.main.h
        };
        let (ox, oy) = (l.main.x - m, l.main.y - m);
        let mut c = Canvas::new(
            (right - ox + m).ceil() as usize,
            (bottom - oy + m).ceil() as usize,
        );
        let at = |r: Rect| Rect::new(r.x - ox, r.y - oy, r.w, r.h);
        let items = self.items(save, recent);
        widgets::panel_filled(&mut c, at(l.main), s, t, t.ui.menu_bg);
        for (i, (it, &(y, h))) in items.iter().zip(&l.rows).enumerate() {
            let row = at(Rect::new(l.main.x, y, l.main.w, h));
            let hot = self.sel == Some(i) && it.enabled;
            paint_row(&mut c, t, fonts, s, row, it, hot);
        }
        if self.sub_open {
            let sub = self.sub_items(recent);
            widgets::panel_filled(&mut c, at(l.sub), s, t, t.ui.menu_bg);
            for (i, (it, &(y, h))) in sub.iter().zip(&l.sub_rows).enumerate() {
                if it.command == Some(Command::ClearRecent) && i > 0 {
                    let x = l.sub.x - ox + 8.0 * s;
                    let ly = y - oy - SEP_H * s * 0.5;
                    widgets::separator(&mut c, x, ly, l.sub.w - 16.0 * s, s, t);
                }
                let row = at(Rect::new(l.sub.x, y, l.sub.w, h));
                let hot = self.sub_sel == Some(i)
                    && (it.enabled || matches!(it.command, Some(Command::OpenRecent(_))));
                paint_row(&mut c, t, fonts, s, row, it, hot);
            }
        }
        (c, ox as i32, oy as i32)
    }
}

/// Lage im Fenster und Maße aus dem Schema.
pub struct Geo {
    pub scale: f32,
    /// Oberkante (Pixel): Unterkante der Titelleiste.
    pub top: f32,
    pub row: f32,
    pub w: f32,
    pub sub_w: f32,
}

impl Geo {
    pub fn new(t: &Theme, scale: f32, top: f32) -> Geo {
        Geo {
            scale,
            top,
            row: t.size.menu_row,
            w: t.size.menu_w,
            sub_w: t.size.menu_sub_w,
        }
    }
}

struct Layout {
    main: Rect,
    rows: Vec<(f32, f32)>,
    sub: Rect,
    sub_rows: Vec<(f32, f32)>,
}

/// Nächste wählbare Zeile (Trennlinien und Ausgegrautes übersprungen).
fn step(items: &[MenuItem], from: Option<usize>, down: bool) -> Option<usize> {
    let n = items.len();
    let ok = |i: usize| !items[i].separator && items[i].enabled;
    if n == 0 || !(0..n).any(ok) {
        return None;
    }
    let mut i = match from {
        Some(i) => i,
        None if down => n - 1,
        None => 0,
    };
    for _ in 0..n {
        i = if down { (i + 1) % n } else { (i + n - 1) % n };
        if ok(i) {
            return Some(i);
        }
    }
    from
}

/// Eine Zeile zeichnen: Grund unter der Maus, Text, Kürzel bzw. Pfeil.
pub(crate) fn paint_row(
    c: &mut Canvas,
    t: &Theme,
    fonts: &Fonts,
    s: f32,
    r: Rect,
    it: &MenuItem,
    hot: bool,
) {
    let u = &t.ui;
    if it.separator {
        let y = r.y + r.h * 0.5;
        widgets::separator(c, r.x + 8.0 * s, y, r.w - 16.0 * s, s, t);
        return;
    }
    let found = it.enabled || matches!(it.command, Some(Command::OpenRecent(_)));
    if hot {
        let mut p = Path::new();
        p.rounded_rect(
            r.x + 4.0 * s,
            r.y + s,
            r.w - 8.0 * s,
            r.h - 2.0 * s,
            4.0 * s,
        );
        c.fill(&p, u.hover);
    }
    let (fg, dim) = if it.enabled && it.danger {
        (u.danger, u.text_dim)
    } else if it.enabled {
        (u.text, u.text_dim)
    } else {
        (u.text_disabled, u.text_disabled)
    };
    let px = t.size.font * s;
    let small = t.size.font_small * s;
    let x = r.x + INSET * s;
    let regular = fonts.regular.as_ref();
    let cap = |px: f32| regular.map_or(px * 0.7, |f| f.cap_height(px));
    let file = matches!(it.command, Some(Command::OpenRecent(_)));
    if file && found {
        // Dateiname fett, darunter der Ordner
        let bold = fonts.bold.as_ref().or(regular);
        let base = r.y + 6.0 * s + cap(px);
        widgets::text(c, bold, &it.label, px, x, base, fg);
        let base2 = base + 6.0 * s + cap(small);
        widgets::text(c, regular, &it.detail, small, x, base2, dim);
        return;
    }
    let base = r.y + (r.h + cap(px)) * 0.5;
    widgets::text(c, regular, &it.label, px, x, base, fg);
    if !it.detail.is_empty() && file {
        // Fehlende Datei: „nicht gefunden“ rechts
        let w = regular.map_or(0.0, |f| f.width(&it.detail, small));
        let rx = r.x + r.w - INSET * s - w;
        widgets::text(c, regular, &it.detail, small, rx, base, dim);
    }
    if it.shortcut == "▸" {
        let (cx, cy, d) = (r.x + r.w - INSET * s - 3.0 * s, r.y + r.h * 0.5, 3.5 * s);
        let mut p = Path::new();
        p.move_to(cx - d * 0.6, cy - d)
            .line_to(cx + d * 0.6, cy)
            .line_to(cx - d * 0.6, cy + d)
            .close();
        c.fill(&p, dim);
    } else if !it.shortcut.is_empty() {
        let w = regular.map_or(0.0, |f| f.width(&it.shortcut, small));
        let rx = r.x + r.w - INSET * s - w;
        widgets::text(c, regular, &it.shortcut, small, rx, base, dim);
    }
}

fn file_name(p: &FsPath) -> String {
    let s = p.to_string_lossy();
    s.rsplit(['\\', '/']).next().unwrap_or(&s).to_string()
}

/// Ordner in der Mitte gekürzt auf höchstens `max` Zeichen:
/// „C:\Projekte\…\Haus“. Laufwerk und erster Ordner bleiben, hinten so viele
/// Ordner wie passen.
pub fn short_folder(p: &FsPath, max: usize) -> String {
    let full = p.to_string_lossy().to_string();
    if full.chars().count() <= max {
        return full;
    }
    let sep = if full.contains('\\') { '\\' } else { '/' };
    let parts: Vec<&str> = full.split(['\\', '/']).collect();
    if parts.len() < 3 {
        let tail: String = full.chars().rev().take(max.saturating_sub(1)).collect();
        return format!("…{}", tail.chars().rev().collect::<String>());
    }
    let head = format!("{}{sep}{}{sep}", parts[0], parts[1]);
    let len = |s: &str| s.chars().count();
    let mut tail = format!("{sep}{}", parts[parts.len() - 1]);
    for part in parts[2..parts.len() - 1].iter().rev() {
        let more = format!("{sep}{part}{tail}");
        if len(&head) + 1 + len(&more) > max {
            break;
        }
        tail = more;
    }
    if len(&head) + 1 + len(&tail) <= max {
        format!("{head}…{tail}")
    } else {
        let first = format!("{}{sep}", parts[0]);
        format!("{first}…{tail}")
    }
}

/// Hinweis beim Darüberfahren über Rückgängig (`redo = false`) bzw.
/// Wiederherstellen; `None`, wenn es nichts gibt (Knopf ausgegraut).
pub fn history_hint(s: &Scene, redo: bool) -> Option<String> {
    if redo {
        s.redo_label()
            .map(|l| format!("Wiederherstellen: {l} (Strg+Y)"))
    } else {
        s.undo_label().map(|l| format!("Rückgängig: {l} (Strg+Z)"))
    }
}

/// „Speichern“ ist möglich: noch ohne Datei oder mit Änderungen.
pub fn save_enabled(doc: &Document, model: &Model) -> bool {
    doc.path.is_none() || doc.is_dirty(model)
}

/// Text der Nachfrage: Frage mit dem Namen ohne Endung und, bei einer Datei,
/// der zweite Satz mit der Uhrzeit des letzten Speicherns.
pub fn save_question(doc: &Document, saved: Option<(u8, u8)>) -> (String, Option<String>) {
    let name = doc.name();
    let stem = name
        .strip_suffix(".szo")
        .or_else(|| name.strip_suffix(".SZO"))
        .unwrap_or(&name);
    let q = format!("Änderungen an „{stem}“ speichern?");
    let detail =
        doc.path.as_ref().and(saved).map(|(h, m)| {
            format!("Ohne Speichern gehen die Änderungen seit {h:02}:{m:02} verloren.")
        });
    (q, detail)
}

/// Tastenkürzel des Hauptfensters. Alt allein öffnet das Menü beim
/// Loslassen, wenn dazwischen keine andere Taste kam.
#[derive(Clone, Debug, Default)]
pub struct Shortcuts {
    alt_alone: bool,
}

impl Shortcuts {
    /// `free`: kein Werkzeug in Eingabe, nichts gezogen.
    pub fn key(&mut self, key: Key, down: bool, mods: Modifiers, free: bool) -> Option<Command> {
        if key == Key::Alt {
            if down {
                if !mods.ctrl && !mods.shift {
                    self.alt_alone = true;
                }
                return None;
            }
            let fire = std::mem::take(&mut self.alt_alone) && free;
            return fire.then_some(Command::OpenMenu);
        }
        if down {
            self.alt_alone = false;
        }
        if !down || !free {
            return None;
        }
        match (key, mods.ctrl, mods.shift, mods.alt) {
            (Key::Char('N'), true, false, false) => Some(Command::New),
            (Key::Char('O'), true, false, false) => Some(Command::Open),
            (Key::Char('S'), true, false, false) => Some(Command::Save),
            (Key::Char('S'), true, true, false) => Some(Command::SaveAs),
            (Key::Char('W'), true, false, false) => Some(Command::Close),
            (Key::Char('Z'), true, false, false) => Some(Command::Undo),
            (Key::Char('Y'), true, false, false) | (Key::Char('Z'), true, true, false) => {
                Some(Command::Redo)
            }
            (Key::Other(0x79), false, false, _) => Some(Command::OpenMenu),
            (Key::Delete, false, false, false) => Some(Command::Delete),
            // Komma-Taste (VK_OEM_COMMA, als Zeichen oder als Code)
            (Key::Char(',') | Key::Other(0xBC), true, false, false) => Some(Command::Settings),
            _ => None,
        }
    }

    /// Eine Maustaste zählt wie eine andere Taste (Alt beim Ziehen).
    pub fn cancel_alt(&mut self) {
        self.alt_alone = false;
    }
}

/// Die Nachfrage „Änderungen speichern?“ als modales Fenster.
#[derive(Clone, Debug, Default)]
pub struct SaveDialog {
    pub question: String,
    pub detail: Option<String>,
    /// Knopf, den Enter auslöst (0 = Speichern).
    focus: usize,
    hover: Option<usize>,
    pressed: Option<usize>,
}

const SAVE_BUTTONS: [&str; 3] = ["Speichern", "Nicht speichern", "Abbrechen"];
const SAVE_ANSWERS: [SaveAnswer; 3] = [SaveAnswer::Save, SaveAnswer::Discard, SaveAnswer::Cancel];
/// Breite des Fensters und der Knöpfe (dip).
const SAVE_W: f32 = 440.0;
const SAVE_BUTTON_W: [f32; 3] = [112.0, 136.0, 104.0];

impl SaveDialog {
    pub fn new(question: String, detail: Option<String>) -> SaveDialog {
        SaveDialog {
            question,
            detail,
            ..SaveDialog::default()
        }
    }

    #[cfg(test)]
    pub fn buttons(&self) -> [&'static str; 3] {
        SAVE_BUTTONS
    }

    #[cfg(test)]
    pub fn default_button(&self) -> usize {
        0
    }

    /// Enter löst den Knopf mit dem Fokus aus, Esc bricht ab, Tab und die
    /// Pfeile wandern.
    pub fn key(&mut self, k: Key) -> Option<SaveAnswer> {
        match k {
            Key::Enter => Some(SAVE_ANSWERS[self.focus]),
            Key::Escape => Some(SaveAnswer::Cancel),
            Key::Tab | Key::Right => {
                self.focus = (self.focus + 1) % 3;
                None
            }
            Key::Left => {
                self.focus = (self.focus + 2) % 3;
                None
            }
            _ => None,
        }
    }

    /// Größe (Pixel) ohne Schatten.
    fn size(&self, s: f32) -> (f32, f32) {
        let h = if self.detail.is_some() { 140.0 } else { 110.0 };
        ((SAVE_W * s).round(), (h * s).round())
    }

    /// Lage im Fenster: mittig unterhalb der Titelleiste.
    pub fn rect(&self, s: f32, win_w: u32, win_h: u32, top: u32) -> Rect {
        let (w, h) = self.size(s);
        let x = ((win_w as f32 - w) * 0.5).round().max(0.0);
        let y = (top as f32 + (win_h as f32 - top as f32 - h) * 0.45)
            .round()
            .max(top as f32);
        Rect::new(x, y, w, h)
    }

    /// Knöpfe im Fenster (relativ zur Fläche), rechtsbündig.
    fn button_rects(&self, s: f32) -> [Rect; 3] {
        let (w, h) = self.size(s);
        let (bh, gap, pad) = (30.0 * s, 8.0 * s, 20.0 * s);
        let y = h - pad - bh;
        let mut x = w - pad;
        let mut out = [Rect::new(0.0, 0.0, 0.0, 0.0); 3];
        for i in (0..3).rev() {
            let bw = SAVE_BUTTON_W[i] * s;
            x -= bw;
            out[i] = Rect::new(x.round(), y.round(), bw.round(), bh.round());
            x -= gap;
        }
        out
    }

    fn button_at(&self, r: Rect, s: f32, x: f64, y: f64) -> Option<usize> {
        let (lx, ly) = (x - r.x as f64, y - r.y as f64);
        self.button_rects(s).iter().position(|b| b.contains(lx, ly))
    }

    /// Maus bewegt; `true`, wenn neu zu zeichnen ist.
    pub fn mouse_move(&mut self, r: Rect, s: f32, x: f64, y: f64) -> bool {
        let h = self.button_at(r, s, x, y);
        let changed = h != self.hover;
        self.hover = h;
        changed
    }

    pub fn press(&mut self, r: Rect, s: f32, x: f64, y: f64) {
        self.pressed = self.button_at(r, s, x, y);
    }

    pub fn release(&mut self, r: Rect, s: f32, x: f64, y: f64) -> Option<SaveAnswer> {
        let p = self.pressed.take()?;
        (self.button_at(r, s, x, y) == Some(p)).then_some(SAVE_ANSWERS[p])
    }

    /// Bild samt Schatten; Lage links oben = `rect` minus Schatten.
    pub fn paint(&self, t: &Theme, fonts: &Fonts, s: f32) -> Canvas {
        let (w, h) = self.size(s);
        let m = (t.size.panel_shadow * s).round();
        let mut c = Canvas::new((w + 2.0 * m) as usize, (h + 2.0 * m) as usize);
        widgets::panel(&mut c, Rect::new(m, m, w, h), s, t);
        let (regular, bold) = (fonts.regular.as_ref(), fonts.bold.as_ref());
        let x = m + 20.0 * s;
        let px = t.size.font_title * s;
        let cap = |px: f32| regular.map_or(px * 0.7, |f| f.cap_height(px));
        let base = m + 20.0 * s + cap(px);
        widgets::text(
            &mut c,
            bold.or(regular),
            &self.question,
            px,
            x,
            base,
            t.ui.text,
        );
        if let Some(d) = &self.detail {
            let px2 = t.size.font * s;
            let b2 = base + 14.0 * s + cap(px2);
            widgets::text(&mut c, regular, d, px2, x, b2, t.ui.text_dim);
        }
        for (i, b) in self.button_rects(s).iter().enumerate() {
            let st = ButtonState {
                hover: self.hover == Some(i),
                pressed: self.pressed == Some(i) && self.hover == Some(i),
                active: self.focus == i,
                disabled: false,
            };
            let r = Rect::new(b.x + m, b.y + m, b.w, b.h);
            widgets::button(&mut c, fonts, r, SAVE_BUTTONS[i], st, s, t);
        }
        c
    }
}

/// Abdunkeln hinter einem modalen Fenster als vormultiplizierte Farbe.
pub fn scrim_premul(k: Rgba) -> [u8; 4] {
    let a = k.3 as u32;
    let pm = |v: u8| ((v as u32 * a + 127) / 255) as u8;
    [pm(k.0), pm(k.1), pm(k.2), k.3]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn menue_trifft_zeilen_und_untermenue() {
        let t = Theme::dark();
        let g = Geo::new(&t, 1.0, 32.0);
        let r = Recent::default();
        let mut m = FileMenu::default();
        m.open();
        // Erste Zeile: 32 + 4 … 32 + 34
        assert!(m.mouse_move(&g, true, &r, 20.0, 50.0) || m.sel == Some(0));
        assert_eq!(m.sel, Some(0));
        // „Zuletzt geöffnet“ öffnet das Untermenü
        m.mouse_move(&g, true, &r, 20.0, 36.0 + 60.0 + 10.0);
        assert!(m.sub_open());
        assert!(m.press(&g, true, &r, 20.0, 50.0));
        assert_eq!(m.release(&g, true, &r, 20.0, 50.0), Some(Command::New));
        assert!(!m.is_open());
        m.open();
        assert!(!m.press(&g, true, &r, 600.0, 400.0), "daneben schließt");
        assert!(!m.is_open());
    }

    #[test]
    fn nachfrage_knoepfe_mit_der_maus() {
        let d = SaveDialog::new("Änderungen an „Haus“ speichern?".into(), None);
        let r = d.rect(1.0, 1280, 800, 32);
        let b = d.button_rects(1.0);
        let mut d = d;
        let (x, y) = ((r.x + b[1].x + 5.0) as f64, (r.y + b[1].y + 5.0) as f64);
        assert!(d.mouse_move(r, 1.0, x, y));
        d.press(r, 1.0, x, y);
        assert_eq!(d.release(r, 1.0, x, y), Some(SaveAnswer::Discard));
        let c = d.paint(
            &Theme::dark(),
            &Fonts {
                regular: None,
                bold: None,
                italic: None,
            },
            1.0,
        );
        assert!(c.width > 400);
    }
}
