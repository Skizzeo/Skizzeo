//! Eigene Titelleiste mit Logo und den drei Fensterknöpfen (Maße wie Windows 11).

use crate::{logo, theme::titlebar as col};
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

    pub fn paint(&self, width: u32) -> Canvas {
        let h = self.height();
        let mut c = Canvas::new(width as usize, h as usize);
        c.clear(col::BACKGROUND);

        let s = self.scale;
        let logo_h = (14.0 * s).round();
        let logo_y = ((h as f32 - logo_h) * 0.5).round();
        c.fill(
            &logo::path_at((12.0 * s).round(), logo_y, logo_h),
            col::LOGO,
        );

        let bw = self.button_width() as f32;
        for (i, b) in [Button::Minimize, Button::Maximize, Button::Close]
            .into_iter()
            .enumerate()
        {
            let x = width as f32 - bw * (3 - i) as f32;
            let state = if self.pressed == Some(b) && self.hover == Some(b) {
                2
            } else if self.hover == Some(b) {
                1
            } else {
                0
            };
            let bg = match (b, state) {
                (_, 0) => col::BACKGROUND,
                (Button::Close, 1) => col::CLOSE_HOVER,
                (Button::Close, _) => col::CLOSE_PRESSED,
                (_, 1) => col::HOVER,
                _ => col::PRESSED,
            };
            if state > 0 {
                c.fill_rect(x, 0.0, bw, h as f32, bg);
            }
            let glyph = if b == Button::Close && state > 0 {
                col::CLOSE_GLYPH_HOVER
            } else if self.active {
                col::GLYPH
            } else {
                col::GLYPH_INACTIVE
            };
            self.paint_glyph(&mut c, b, x + bw * 0.5, h as f32 * 0.5, glyph, bg);
        }
        c
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
}
