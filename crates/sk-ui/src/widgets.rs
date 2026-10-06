//! Bausteine der Paneele: Schriften, Paneelfläche, Knöpfe, Text.
//! Gestaltung nach Jörns Vorlage (dunkles Paneel, gelber Akzent).

use crate::theme::Theme;
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
pub fn panel(c: &mut Canvas, r: Rect, s: f32, t: &Theme) {
    let rad = t.size.corner_radius * s;
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
        c.fill(&p, t.ui.shadow);
    }
    let (ix, iy, iw, ih, ir) = (r.x + b, r.y + b, r.w - 2.0 * b, r.h - 2.0 * b, rad - b);
    let mut p = Path::new();
    p.rounded_rect(r.x, r.y, r.w, r.h, rad);
    hole(&mut p, ix, iy, iw, ih, ir);
    c.fill(&p, t.ui.border);
    let mut p = Path::new();
    p.rounded_rect(ix, iy, iw, ih, ir);
    c.fill(&p, t.ui.bg);
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
pub fn button(
    c: &mut Canvas,
    fonts: &Fonts,
    r: Rect,
    label: &str,
    st: ButtonState,
    s: f32,
    t: &Theme,
) {
    let u = &t.ui;
    let rad = 6.0 * s;
    let b = s.round().max(1.0);
    let (fill, border, text) = if st.active {
        let f = if st.hover { u.accent_hover } else { u.accent };
        (f, f, u.on_accent)
    } else if st.pressed {
        (u.pressed, u.border, u.text)
    } else if st.hover {
        (u.hover, u.border, u.text)
    } else {
        (u.bg, u.border, u.text)
    };
    let mut p = Path::new();
    p.rounded_rect(r.x, r.y, r.w, r.h, rad);
    c.fill(&p, border);
    let mut p = Path::new();
    p.rounded_rect(r.x + b, r.y + b, r.w - 2.0 * b, r.h - 2.0 * b, rad - b);
    c.fill(&p, fill);
    if let Some(f) = fonts.bold.as_ref().or(fonts.regular.as_ref()) {
        let px = t.size.font * s;
        let x = r.x + (r.w - f.width(label, px)) * 0.5;
        let y = r.y + (r.h + f.cap_height(px)) * 0.5;
        f.draw(c, label, px, x.round(), y.round(), text);
    }
}

/// Zustand eines Zahlenfelds beim Zeichnen.
#[derive(Clone, Copy, Debug, Default)]
pub struct FieldState<'a> {
    /// Zahl, wie sie im Feld steht (beim Eingeben der getippte Text).
    pub text: &'a str,
    /// Einheit rechtsbündig hinter der Zahl, z. B. „cm“.
    pub unit: &'a str,
    pub hover: bool,
    /// Eingabemodus: Rahmen in `field_focus`, Schreibmarke und Markierung.
    pub focus: bool,
    pub invalid: bool,
    /// Schreibmarke als Byte-Stelle in `text`.
    pub caret: Option<usize>,
    /// Markierter Bereich (Byte-Stellen, von < bis).
    pub select: Option<(usize, usize)>,
}

/// Zahlenfeld: Grund, Rahmen, Zahl rechtsbündig vor der Einheit.
pub fn field(c: &mut Canvas, fonts: &Fonts, r: Rect, st: &FieldState, s: f32, t: &Theme) {
    let u = &t.ui;
    let rad = 4.0 * s;
    let b = s.round().max(1.0);
    let border = if st.invalid {
        u.field_invalid
    } else if st.focus {
        u.field_focus
    } else {
        u.field_border
    };
    let fill = if st.hover && !st.focus {
        u.field_hover
    } else {
        u.field
    };
    let mut p = Path::new();
    p.rounded_rect(r.x, r.y, r.w, r.h, rad);
    c.fill(&p, border);
    let mut p = Path::new();
    p.rounded_rect(r.x + b, r.y + b, r.w - 2.0 * b, r.h - 2.0 * b, rad - b);
    c.fill(&p, fill);
    let Some(f) = fonts.regular.as_ref() else {
        return;
    };
    let px = t.size.font_small * s;
    let pad = t.size.field_pad * s;
    let unit_w = f.width(st.unit, px);
    let unit_x = r.x + r.w - pad - unit_w;
    let num_x = unit_x - 4.0 * s - f.width(st.text, px);
    let base = (r.y + (r.h + f.cap_height(px)) * 0.5).round();
    let at = |i: usize| num_x + f.width(&st.text[..i.min(st.text.len())], px);
    if let Some((a, z)) = st.select.filter(|(a, z)| a < z) {
        let (x0, x1) = (at(a), at(z));
        c.fill_rect(x0, r.y + 4.0 * s, x1 - x0, r.h - 8.0 * s, u.text_select);
    }
    f.draw(c, st.text, px, num_x.round(), base, u.field_text);
    f.draw(c, st.unit, px, unit_x.round(), base, u.field_unit);
    if let Some(i) = st.caret.filter(|_| st.focus) {
        c.fill_rect(at(i).round(), r.y + 5.0 * s, b, r.h - 10.0 * s, u.caret);
    }
}

/// Text mit Grundlinie bei `y`.
pub fn text(c: &mut Canvas, font: Option<&Font>, t: &str, px: f32, x: f32, y: f32, color: Rgba) {
    if let Some(f) = font {
        f.draw(c, t, px, x.round(), y.round(), color);
    }
}

/// Feine waagerechte Trennlinie.
pub fn separator(c: &mut Canvas, x: f32, y: f32, w: f32, s: f32, t: &Theme) {
    c.fill_rect(x, y.round(), w, s.round().max(1.0), t.ui.border);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Paneel wie vor der Beschleunigung: sechs Flächen über das ganze Paneel.
    fn panel_reference(c: &mut Canvas, r: Rect, s: f32, t: &Theme) {
        let rad = t.size.corner_radius * s;
        for i in 1..=4 {
            let d = i as f32 * 2.0 * s;
            let mut p = Path::new();
            p.rounded_rect(r.x - d * 0.5, r.y - d * 0.25, r.w + d, r.h + d, rad + d);
            c.fill(&p, Rgba(0, 0, 0, 14));
        }
        let mut p = Path::new();
        p.rounded_rect(r.x, r.y, r.w, r.h, rad);
        c.fill(&p, Rgba::rgb(56, 65, 76));
        let b = s.round().max(1.0);
        let mut p = Path::new();
        p.rounded_rect(r.x + b, r.y + b, r.w - 2.0 * b, r.h - 2.0 * b, rad - b);
        c.fill(&p, Rgba::rgb(31, 37, 45));
    }

    fn paint(s: f32, f: fn(&mut Canvas, Rect, f32, &Theme)) -> Vec<u8> {
        let m = (10.0 * s).round();
        let (w, h) = (196.0 * s, 640.0 * s);
        let mut c = Canvas::new((w + 2.0 * m) as usize, (h + 2.0 * m) as usize);
        f(&mut c, Rect::new(m, m, w, h), s, &Theme::dark());
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
            let t = |f: fn(&mut Canvas, Rect, f32, &Theme)| {
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
