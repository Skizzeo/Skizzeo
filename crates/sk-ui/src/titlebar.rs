//! Eigene Titelleiste: links Menüknopf mit Logo, Rückgängig und
//! Wiederherstellen (E17), rechts die drei Fensterknöpfe (Maße wie Windows 11).

use crate::{logo, theme::Theme};
use sk_paint::{font::Font, Canvas, Path, Rgba};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Button {
    Minimize,
    Maximize,
    Close,
    /// Linke Gruppe (E17): Dateimenü am Logo, Rückgängig, Wiederherstellen.
    Menu,
    Undo,
    Redo,
}

/// Linke Knopfgruppe (dip): Menü 46, Abstand 8, Rückgängig und
/// Wiederherstellen je 40.
const MENU_W: f32 = 46.0;
const LEFT_GAP: f32 = 8.0;
const HISTORY_W: f32 = 40.0;

#[derive(Clone, Debug)]
pub struct TitleBar {
    /// Bildschirmskalierung (1,0 = 96 dpi).
    pub scale: f32,
    pub maximized: bool,
    pub active: bool,
    pub hover: Option<Button>,
    pub pressed: Option<Button>,
    /// Mittig gezeigter Text, z. B. Dateiname.
    pub caption: String,
    /// Es gibt etwas zum Rückgängigmachen bzw. Wiederherstellen; sonst ist
    /// der Knopf ausgegraut und ohne Hover.
    pub undo_enabled: bool,
    pub redo_enabled: bool,
    /// Dateimenü offen: Menüknopf gedrückt.
    pub menu_open: bool,
}

impl TitleBar {
    pub fn new(scale: f32) -> TitleBar {
        TitleBar {
            scale,
            maximized: false,
            active: true,
            hover: None,
            pressed: None,
            caption: String::new(),
            undo_enabled: false,
            redo_enabled: false,
            menu_open: false,
        }
    }

    /// Linke und rechte Kante eines Knopfes der linken Gruppe (ganze Pixel).
    fn left_span(&self, b: Button) -> Option<(f32, f32)> {
        let s = self.scale;
        let menu = (MENU_W * s).round();
        let undo = (menu + LEFT_GAP * s).round();
        let redo = undo + (HISTORY_W * s).round();
        let end = (redo + HISTORY_W * s).round();
        match b {
            Button::Menu => Some((0.0, menu)),
            Button::Undo => Some((undo, redo)),
            Button::Redo => Some((redo, end)),
            _ => None,
        }
    }

    /// Breite der linken Gruppe (Pixel); dort ist die Leiste keine Ziehfläche.
    pub fn left_width(&self) -> u32 {
        self.left_span(Button::Redo).map_or(0, |s| s.1 as u32)
    }

    /// Knopf der linken Gruppe unter `(x, y)`.
    pub fn left_button_at(&self, x: f64, y: f64) -> Option<Button> {
        if y < 0.0 || y >= self.height() as f64 {
            return None;
        }
        [Button::Menu, Button::Undo, Button::Redo]
            .into_iter()
            .find(|&b| {
                self.left_span(b)
                    .is_some_and(|(a, z)| x >= a as f64 && x < z as f64)
            })
    }

    /// Ausgegraut: nichts zum Rückgängigmachen bzw. Wiederherstellen.
    pub fn is_disabled(&self, b: Button) -> bool {
        match b {
            Button::Undo => !self.undo_enabled,
            Button::Redo => !self.redo_enabled,
            _ => false,
        }
    }

    pub fn height(&self) -> u32 {
        (32.0 * self.scale).round() as u32
    }

    pub fn button_width(&self) -> u32 {
        (46.0 * self.scale).round() as u32
    }

    pub fn buttons_width(&self) -> u32 {
        self.button_width() * 3
    }

    /// Knopf unter `(x, y)`: linke Gruppe oder Fensterknöpfe.
    pub fn button_at(&self, x: f64, y: f64, width: u32) -> Option<Button> {
        if y < 0.0 || y >= self.height() as f64 {
            return None;
        }
        if let Some(b) = self.left_button_at(x, y) {
            return Some(b);
        }
        let bw = self.button_width() as f64;
        let right = width as f64;
        match ((right - x) / bw).floor() as i64 {
            0 => Some(Button::Close),
            1 => Some(Button::Maximize),
            2 => Some(Button::Minimize),
            _ => None,
        }
    }

    pub fn paint(&self, t: &Theme, font: Option<&Font>, width: u32) -> Canvas {
        let h = self.height();
        let mut c = Canvas::new(width as usize, h as usize);
        c.clear(t.title.bg);

        let s = self.scale;
        // Text mittig; nur wenn er zwischen die Knopfgruppen passt
        if let (Some(f), false) = (font, self.caption.is_empty()) {
            let px = (12.0 * s).round();
            let tw = f.width(&self.caption, px);
            let x = ((width as f32 - tw) * 0.5).round();
            let left = self.left_width() as f32 + 8.0 * s;
            let right = width as f32 - self.buttons_width() as f32 - 8.0 * s;
            if x >= left && x + tw <= right {
                let y = ((h as f32 + f.cap_height(px)) * 0.5).round();
                let col = if self.active {
                    t.title.glyph
                } else {
                    t.title.glyph_inactive
                };
                f.draw(&mut c, &self.caption, px, x, y, col);
            }
        }

        for b in [
            Button::Menu,
            Button::Undo,
            Button::Redo,
            Button::Minimize,
            Button::Maximize,
            Button::Close,
        ] {
            self.paint_button_at(t, &mut c, b, self.button_x(b, width));
        }
        c
    }

    /// Linke Kante eines Knopfes (ganze Pixel).
    fn button_x(&self, b: Button, width: u32) -> f32 {
        if let Some((a, _)) = self.left_span(b) {
            return a;
        }
        let i = match b {
            Button::Minimize => 3,
            Button::Maximize => 2,
            _ => 1,
        };
        width as f32 - (self.button_width() * i) as f32
    }

    /// Breite eines Knopfes (Pixel).
    fn width_of(&self, b: Button) -> f32 {
        match self.left_span(b) {
            Some((a, z)) => z - a,
            None => self.button_width() as f32,
        }
    }

    /// Zeichnet einen Knopf auf dem Bild von [`TitleBar::paint`] neu (gleiche
    /// Breite), pixelgleich zum vollen Neuzeichnen. Liefert seine Spalten `(x, w)`.
    pub fn repaint_button(
        &self,
        t: &Theme,
        c: &mut Canvas,
        b: Button,
        width: u32,
    ) -> (usize, usize) {
        let x = self.button_x(b, width);
        let bw = self.width_of(b);
        c.fill_rect(x, 0.0, bw, self.height() as f32, t.title.bg);
        self.paint_button_at(t, c, b, x);
        (x.max(0.0) as usize, bw as usize)
    }

    fn paint_button_at(&self, t: &Theme, c: &mut Canvas, b: Button, x: f32) {
        let col = &t.title;
        let h = self.height();
        let bw = self.width_of(b);
        let disabled = self.is_disabled(b);
        let state = if disabled {
            0
        } else if (self.pressed == Some(b) && self.hover == Some(b))
            || (b == Button::Menu && self.menu_open)
        {
            2
        } else if self.hover == Some(b) {
            1
        } else {
            0
        };
        let bg = match (b, state) {
            (_, 0) => col.bg,
            (Button::Close, 1) => col.close_hover,
            (Button::Close, _) => col.close_pressed,
            (_, 1) => col.hover,
            _ => col.pressed,
        };
        if state > 0 {
            c.fill_rect(x, 0.0, bw, h as f32, bg);
        }
        let glyph = if b == Button::Close && state > 0 {
            col.close_glyph_hover
        } else if self.active && !disabled {
            col.glyph
        } else {
            col.glyph_inactive
        };
        match b {
            Button::Menu => self.paint_menu_glyph(t, c, x, bw),
            Button::Undo | Button::Redo => {
                self.paint_history_glyph(c, b == Button::Redo, x + bw * 0.5, glyph)
            }
            _ => self.paint_glyph(c, b, x + bw * 0.5, h as f32 * 0.5, glyph, bg),
        }
    }

    /// Logo wie bisher, darunter ein kleiner Pfeil ▾ (5 dip, Strich 1 dip).
    fn paint_menu_glyph(&self, t: &Theme, c: &mut Canvas, x: f32, bw: f32) {
        let s = self.scale;
        let h = self.height() as f32;
        let logo_h = (14.0 * s).round();
        let logo_w = logo_h * logo::WIDTH / logo::HEIGHT;
        let lx = (x + (bw - logo_w) * 0.5).round();
        let ly = ((h - logo_h) * 0.5 - 2.0 * s).round();
        c.fill(&logo::path_at(lx, ly, logo_h), t.title.logo);
        let (cx, ty) = (lx + logo_w * 0.5, ly + logo_h + 3.0 * s);
        let (d, sw) = (2.5 * s, s.max(1.0));
        let mut p = Path::new();
        p.segment((cx - d, ty), (cx, ty + d), sw);
        p.segment((cx, ty + d), (cx + d, ty), sw);
        c.fill(&p, t.title.logo);
    }

    /// Bogen über 270° mit Spitze links (Rückgängig, gegen den
    /// Uhrzeigersinn), für Wiederherstellen gespiegelt; 14 dip, Strich 1,5 dip.
    fn paint_history_glyph(&self, c: &mut Canvas, mirror: bool, cx: f32, fg: Rgba) {
        let s = self.scale;
        let cy = self.height() as f32 * 0.5 + 1.0 * s;
        let (r, w) = (5.0 * s, 1.5 * s);
        let m = if mirror { -1.0 } else { 1.0 };
        // Winkel mathematisch (y nach oben): von links unten über unten,
        // rechts und oben bis links oben
        let (a0, a1) = (225f32.to_radians(), 495f32.to_radians());
        let pt = |a: f32, rad: f32| (cx + m * rad * a.cos(), cy - rad * a.sin());
        let n = 40;
        let mut p = Path::new();
        let ang = |i: usize| a0 + (a1 - a0) * i as f32 / n as f32;
        let (x, y) = pt(ang(0), r + w * 0.5);
        p.move_to(x, y);
        for i in 1..=n {
            let (x, y) = pt(ang(i), r + w * 0.5);
            p.line_to(x, y);
        }
        for i in (0..=n).rev() {
            let (x, y) = pt(ang(i), r - w * 0.5);
            p.line_to(x, y);
        }
        p.close();
        c.fill(&p, fg);
        // Spitze am Ende (links oben), in Laufrichtung
        let end = a1;
        let (ex, ey) = pt(end, r);
        let (tx, ty) = (-end.sin() * m, -end.cos());
        let (nx, ny) = (end.cos() * m, -end.sin());
        let len = 3.5 * s;
        let mut q = Path::new();
        q.move_to(ex + tx * len, ey + ty * len)
            .line_to(ex + nx * len, ey + ny * len)
            .line_to(ex - nx * len, ey - ny * len)
            .close();
        c.fill(&q, fg);
    }

    fn paint_glyph(&self, c: &mut Canvas, b: Button, cx: f32, cy: f32, fg: Rgba, bg: Rgba) {
        let s = self.scale;
        let sw = s.round().max(1.0); // Strichstärke 1 dip, pixelgenau
        let g = (10.0 * s).round(); // Glyphengröße 10 dip
                                    // Auf das Pixelraster legen, damit waagerechte/senkrechte Striche scharf sind.
        let x0 = (cx - g * 0.5).round();
        let y0 = (cy - g * 0.5).round();
        let r = 1.0 * s;
        match b {
            Button::Minimize => {
                c.fill_rect(x0, (cy - sw * 0.5).round(), g, sw, fg);
            }
            Button::Maximize if !self.maximized => {
                outline(c, x0, y0, g, g, r, sw, fg);
            }
            Button::Maximize => {
                let d = (2.0 * s).round();
                let f = g - d;
                outline(c, x0 + d, y0, f, f, r, sw, fg);
                let mut p = Path::new();
                p.rounded_rect(x0, y0 + d, f, f, r);
                c.fill(&p, bg);
                outline(c, x0, y0 + d, f, f, r, sw, fg);
            }
            Button::Menu | Button::Undo | Button::Redo => {}
            Button::Close => {
                let mut p = Path::new();
                let k = sw * 0.35; // Strichenden leicht einrücken
                p.segment((x0 + k, y0 + k), (x0 + g - k, y0 + g - k), sw * 1.05);
                c.fill(&p, fg);
                let mut q = Path::new();
                q.segment((x0 + g - k, y0 + k), (x0 + k, y0 + g - k), sw * 1.05);
                c.fill(&q, fg);
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn outline(c: &mut Canvas, x: f32, y: f32, w: f32, h: f32, r: f32, sw: f32, fg: Rgba) {
    let mut p = Path::new();
    p.rounded_rect(x, y, w, h, r);
    p.rounded_rect_hole(
        x + sw,
        y + sw,
        w - 2.0 * sw,
        h - 2.0 * sw,
        (r - sw).max(0.0),
    );
    c.fill(&p, fg);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn knopf_trefferpruefung() {
        let t = TitleBar::new(1.0);
        assert_eq!(t.button_at(999.0, 5.0, 1000), Some(Button::Close));
        assert_eq!(
            t.button_at(1000.0 - 47.0, 5.0, 1000),
            Some(Button::Maximize)
        );
        assert_eq!(
            t.button_at(1000.0 - 93.0, 5.0, 1000),
            Some(Button::Minimize)
        );
        assert_eq!(t.button_at(500.0, 5.0, 1000), None);
        assert_eq!(t.button_at(999.0, 40.0, 1000), None);
    }

    #[test]
    fn einzelner_knopf_gleicht_der_ganzen_leiste() {
        let states = |b| [(None, None), (Some(b), None), (Some(b), Some(b))];
        for scale in [1.0f32, 1.25, 1.5, 1.75, 2.0] {
            for maximized in [false, true] {
                for b in [
                    Button::Minimize,
                    Button::Maximize,
                    Button::Close,
                    Button::Menu,
                    Button::Undo,
                    Button::Redo,
                ] {
                    for (from, to) in states(b).into_iter().zip(states(b).into_iter().rev()) {
                        let mut t = TitleBar::new(scale);
                        t.maximized = maximized;
                        t.undo_enabled = true;
                        t.redo_enabled = maximized;
                        let width = 1003;
                        (t.hover, t.pressed) = from;
                        let th = Theme::dark();
                        let mut c = t.paint(&th, None, width);
                        (t.hover, t.pressed) = to;
                        t.repaint_button(&th, &mut c, b, width);
                        let full = t.paint(&th, None, width).to_premul_rgba8();
                        assert!(
                            c.to_premul_rgba8() == full,
                            "{scale} {b:?} {from:?} -> {to:?}"
                        );
                    }
                }
            }
        }
    }
}
