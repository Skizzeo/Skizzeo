//! Eigene Titelleiste mit Logo und den drei Fensterknöpfen (Maße wie Windows 11).

use crate::{logo, theme::Theme};
use sk_paint::{Canvas, Path, Rgba};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Button {
    Minimize,
    Maximize,
    Close,
}

#[derive(Clone, Debug)]
pub struct TitleBar {
    /// Bildschirmskalierung (1,0 = 96 dpi).
    pub scale: f32,
    pub maximized: bool,
    pub active: bool,
    pub hover: Option<Button>,
    pub pressed: Option<Button>,
}

impl TitleBar {
    pub fn new(scale: f32) -> TitleBar {
        TitleBar {
            scale,
            maximized: false,
            active: true,
            hover: None,
            pressed: None,
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

    pub fn button_at(&self, x: f64, y: f64, width: u32) -> Option<Button> {
        if y < 0.0 || y >= self.height() as f64 {
            return None;
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

    pub fn paint(&self, t: &Theme, width: u32) -> Canvas {
        let h = self.height();
        let mut c = Canvas::new(width as usize, h as usize);
        c.clear(t.title.bg);

        let s = self.scale;
        let logo_h = (14.0 * s).round();
        let logo_y = ((h as f32 - logo_h) * 0.5).round();
        c.fill(
            &logo::path_at((12.0 * s).round(), logo_y, logo_h),
            t.title.logo,
        );

        for b in [Button::Minimize, Button::Maximize, Button::Close] {
            self.paint_button_at(t, &mut c, b, self.button_x(b, width));
        }
        c
    }

    /// Linke Kante eines Knopfes (ganze Pixel).
    fn button_x(&self, b: Button, width: u32) -> f32 {
        let i = match b {
            Button::Minimize => 3,
            Button::Maximize => 2,
            Button::Close => 1,
        };
        width as f32 - (self.button_width() * i) as f32
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
        let bw = self.button_width() as f32;
        c.fill_rect(x, 0.0, bw, self.height() as f32, t.title.bg);
        self.paint_button_at(t, c, b, x);
        (x.max(0.0) as usize, self.button_width() as usize)
    }

    fn paint_button_at(&self, t: &Theme, c: &mut Canvas, b: Button, x: f32) {
        let col = &t.title;
        let h = self.height();
        let bw = self.button_width() as f32;
        let state = if self.pressed == Some(b) && self.hover == Some(b) {
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
        } else if self.active {
            col.glyph
        } else {
            col.glyph_inactive
        };
        self.paint_glyph(c, b, x + bw * 0.5, h as f32 * 0.5, glyph, bg);
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
                for b in [Button::Minimize, Button::Maximize, Button::Close] {
                    for (from, to) in states(b).into_iter().zip(states(b).into_iter().rev()) {
                        let mut t = TitleBar::new(scale);
                        t.maximized = maximized;
                        let width = 1003;
                        (t.hover, t.pressed) = from;
                        let th = Theme::dark();
                        let mut c = t.paint(&th, width);
                        (t.hover, t.pressed) = to;
                        t.repaint_button(&th, &mut c, b, width);
                        let full = t.paint(&th, width).to_premul_rgba8();
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
