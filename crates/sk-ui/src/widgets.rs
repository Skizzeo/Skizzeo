//! Bausteine der Paneele: Schriften, Paneelfläche, Knöpfe, Text.
//! Gestaltung nach Jörns Vorlage (dunkles Paneel, gelber Akzent).

use crate::theme::panel as col;
use sk_paint::{font::Font, Canvas, Path, Rgba};

/// Rechteck in Pixeln.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl Rect {
    pub const fn new(x: f32, y: f32, w: f32, h: f32) -> Rect {
        Rect { x, y, w, h }
    }

    pub fn contains(&self, x: f64, y: f64) -> bool {
        let (x, y) = (x as f32, y as f32);
        x >= self.x && x < self.x + self.w && y >= self.y && y < self.y + self.h
    }
}

/// Schriften aus dem System (eigener TrueType-Leser). Fehlt eine, bleibt Text weg.
pub struct Fonts {
    pub regular: Option<Font>,
    pub bold: Option<Font>,
}

impl Fonts {
    pub fn system() -> Fonts {
        Fonts {
            regular: Font::system(&["segoeui.ttf", "arial.ttf", "tahoma.ttf"]),
            bold: Font::system(&["seguisb.ttf", "segoeuib.ttf", "arialbd.ttf", "tahomabd.ttf"]),
        }
    }
}

/// Paneelfläche mit weichem Schatten und feinem Rand. `r` ist die Fläche ohne Schatten.
///
/// Schatten und Rand werden nur als Ringe gefüllt: Ihr Inneres wird ohnehin von
/// der deckenden Fläche darüber verdeckt. Das Loch liegt [`HIDDEN_INSET`] Pixel
/// innerhalb dieser Fläche, damit die geglätteten Kanten genau gleich aussehen.
pub fn panel(c: &mut Canvas, r: Rect, s: f32) {
    let rad = col::CORNER_RADIUS * s;
    let b = s.round().max(1.0);
    // Deckend überdeckte Bereiche: unter dem Rand bzw. unter der Füllung
    let hole = |p: &mut Path, x: f32, y: f32, w: f32, h: f32, r: f32| {
        let i = HIDDEN_INSET;
        if w > 2.0 * i && h > 2.0 * i {
            p.rounded_rect_hole(x + i, y + i, w - 2.0 * i, h - 2.0 * i, (r - i).max(0.0));
        }
    };
    for i in 1..=4 {
        let d = i as f32 * 2.0 * s;
        let mut p = Path::new();
        p.rounded_rect(r.x - d * 0.5, r.y - d * 0.25, r.w + d, r.h + d, rad + d);
        hole(&mut p, r.x, r.y, r.w, r.h, rad);
        c.fill(&p, Rgba(0, 0, 0, 14));
    }
    let (ix, iy, iw, ih, ir) = (r.x + b, r.y + b, r.w - 2.0 * b, r.h - 2.0 * b, rad - b);
    let mut p = Path::new();
    p.rounded_rect(r.x, r.y, r.w, r.h, rad);
    hole(&mut p, ix, iy, iw, ih, ir);
    c.fill(&p, col::BORDER);
    let mut p = Path::new();
    p.rounded_rect(ix, iy, iw, ih, ir);
    c.fill(&p, col::BACKGROUND);
}

/// Abstand des ausgesparten Lochs vom Rand der deckenden Fläche darüber (Pixel).
const HIDDEN_INSET: f32 = 2.0;

#[derive(Clone, Copy, Debug, Default)]
pub struct ButtonState {
    pub hover: bool,
    pub pressed: bool,
    /// Eingeschaltet / ausgewählt: gelb gefüllt.
    pub active: bool,
}

/// Knopf mit zentrierter Beschriftung.
pub fn button(c: &mut Canvas, fonts: &Fonts, r: Rect, label: &str, st: ButtonState, s: f32) {
    let rad = 6.0 * s;
    let b = s.round().max(1.0);
    let (fill, border, text) = if st.active {
        let f = if st.hover {
            col::ACCENT_HOVER
        } else {
            col::ACCENT
        };
        (f, f, col::ON_ACCENT)
    } else if st.pressed {
        (col::BUTTON_PRESSED, col::BORDER, col::TEXT)
    } else if st.hover {
        (col::BUTTON_HOVER, col::BORDER, col::TEXT)
    } else {
        (col::BACKGROUND, col::BORDER, col::TEXT)
    };
    let mut p = Path::new();
    p.rounded_rect(r.x, r.y, r.w, r.h, rad);
    c.fill(&p, border);
    let mut p = Path::new();
    p.rounded_rect(r.x + b, r.y + b, r.w - 2.0 * b, r.h - 2.0 * b, rad - b);
    c.fill(&p, fill);
    if let Some(f) = fonts.bold.as_ref().or(fonts.regular.as_ref()) {
        let px = 14.0 * s;
        let x = r.x + (r.w - f.width(label, px)) * 0.5;
        let y = r.y + (r.h + f.cap_height(px)) * 0.5;
        f.draw(c, label, px, x.round(), y.round(), text);
    }
}

/// Text mit Grundlinie bei `y`.
pub fn text(c: &mut Canvas, font: Option<&Font>, t: &str, px: f32, x: f32, y: f32, color: Rgba) {
    if let Some(f) = font {
        f.draw(c, t, px, x.round(), y.round(), color);
    }
}

/// Feine waagerechte Trennlinie.
pub fn separator(c: &mut Canvas, x: f32, y: f32, w: f32, s: f32) {
    c.fill_rect(x, y.round(), w, s.round().max(1.0), col::BORDER);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Paneel wie vor der Beschleunigung: sechs Flächen über das ganze Paneel.
    fn panel_reference(c: &mut Canvas, r: Rect, s: f32) {
        let rad = col::CORNER_RADIUS * s;
        for i in 1..=4 {
            let d = i as f32 * 2.0 * s;
            let mut p = Path::new();
            p.rounded_rect(r.x - d * 0.5, r.y - d * 0.25, r.w + d, r.h + d, rad + d);
            c.fill(&p, Rgba(0, 0, 0, 14));
        }
        let mut p = Path::new();
        p.rounded_rect(r.x, r.y, r.w, r.h, rad);
        c.fill(&p, col::BORDER);
        let b = s.round().max(1.0);
        let mut p = Path::new();
        p.rounded_rect(r.x + b, r.y + b, r.w - 2.0 * b, r.h - 2.0 * b, rad - b);
        c.fill(&p, col::BACKGROUND);
    }

    fn paint(s: f32, f: fn(&mut Canvas, Rect, f32)) -> Vec<u8> {
        let m = (10.0 * s).round();
        let (w, h) = (196.0 * s, 640.0 * s);
        let mut c = Canvas::new((w + 2.0 * m) as usize, (h + 2.0 * m) as usize);
        f(&mut c, Rect::new(m, m, w, h), s);
        c.to_premul_rgba8()
    }

    #[test]
    fn paneel_bild_bleibt_gleich() {
        for s in [1.0, 1.25, 1.5, 2.0] {
            let (a, b) = (paint(s, panel), paint(s, panel_reference));
            let diff = a.iter().zip(&b).map(|(x, y)| x.abs_diff(*y)).max();
            assert_eq!(diff, Some(0), "Skalierung {s}");
        }
    }

    /// `cargo test --release -p sk-ui paneel_zeit -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn paneel_zeit() {
        for s in [1.0f32, 1.5, 2.0] {
            let t = |f: fn(&mut Canvas, Rect, f32)| {
                let n = 20;
                let start = std::time::Instant::now();
                for _ in 0..n {
                    std::hint::black_box(paint(s, f));
                }
                start.elapsed().as_secs_f64() * 1000.0 / n as f64
            };
            println!(
                "Skalierung {s}: vorher {:.2} ms, jetzt {:.2} ms",
                t(panel_reference),
                t(panel)
            );
        }
    }
}
