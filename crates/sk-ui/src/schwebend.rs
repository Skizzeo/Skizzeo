//! Schwebende Großansicht über einem Fenster (Jörn 11.10., zuerst die große
//! Druckvorschau des LV): Das Fenster unter der Titelleiste wird abgedunkelt,
//! oben steht eine dunkle Leiste mit Titel, Hinweis und „Schließen ×“,
//! darunter zeigt der Aufrufer seinen Inhalt groß. Esc, ein Klick daneben
//! oder „Schließen“ schließt; ein Klick auf den Inhalt schließt nicht.
//!
//! Der Baustein kennt nur Rahmen, Leiste und Klick. Was groß gezeigt wird,
//! wie es rollt und was Esc sonst noch tut, bleibt beim Aufrufer:
//!
//! ```ignore
//! let r = Schwebend::neu((0.0, oben, w, h - oben), scale, rechts, fonts);
//! r.abdunkeln(c);
//! /* Inhalt ab r.inhalt_oben() zeichnen */
//! r.leiste(c, fonts, ("Seite 1 von 3", inhalt_x), schwebend::HINWEIS, hot_zu);
//! match r.klick(x, y, inhalt_trifft) { Klick::Daneben | Klick::Zu => zu(), Klick::Inhalt => {} }
//! ```
//! Die Leiste wird nach dem Inhalt gezeichnet und deckt, was nach oben
//! gerollt ist.

use crate::widgets::Fonts;
use sk_paint::{Canvas, Path, Rgba};

/// x, y, Breite, Höhe (px).
pub type Rect = (f32, f32, f32, f32);

/// Höhe der Leiste (dip).
pub const LEISTE: f32 = 44.0;
/// Rand links und rechts der Leiste (dip).
pub const RAND: f32 = 16.0;
/// Hinweis rechts neben dem Titel, wenn Platz ist.
pub const HINWEIS: &str = "Esc oder Klick daneben schließt";
const ZU: &str = "Schließen";
/// Abdunkeln und Leiste, in jedem Schema dunkel; Schrift darauf hell.
pub const DUNKEL: Rgba = Rgba(0, 0, 0, 170);
pub const LEISTE_BG: Rgba = Rgba(34, 34, 36, 255);
const HELL: Rgba = Rgba(255, 255, 255, 235);
const HELL_DIM: Rgba = Rgba(255, 255, 255, 150);

fn inside((rx, ry, rw, rh): Rect, x: f32, y: f32) -> bool {
    x >= rx && x < rx + rw && y >= ry && y < ry + rh
}

/// Was ein Klick in der Großansicht trifft.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Klick {
    /// „Schließen ×“: schließt.
    Zu,
    /// Der gezeigte Inhalt: schließt nicht.
    Inhalt,
    /// Leiste oder Fläche daneben: schließt.
    Daneben,
}

/// Rahmen der Großansicht (px).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Schwebend {
    /// Abgedunkelte Fläche: das Fenster unter seiner Titelleiste.
    pub flaeche: Rect,
    pub scale: f32,
    /// „Schließen ×“ in der Leiste.
    pub zu: Rect,
}

impl Schwebend {
    /// Rahmen über `flaeche`; „Schließen“ endet rechts bei `rechts` (px),
    /// höchstens `RAND` vor dem rechten Rand der Fläche.
    pub fn neu(flaeche: Rect, scale: f32, rechts: f32, fonts: &Fonts) -> Schwebend {
        let s = scale;
        let (fx, fy, fw, _) = flaeche;
        let bold = fonts.bold.as_ref().or(fonts.regular.as_ref());
        let zw = bold.map_or(60.0 * s, |f| f.width(ZU, 11.0 * s)) + 40.0 * s;
        let zh = 26.0 * s;
        let r = rechts.min(fx + fw - RAND * s);
        Schwebend {
            flaeche,
            scale,
            zu: (r - zw, fy + (LEISTE * s - zh) * 0.5, zw, zh),
        }
    }

    /// Oberkante des Inhalts unter der Leiste (px).
    pub fn inhalt_oben(&self) -> f32 {
        self.flaeche.1 + LEISTE * self.scale
    }

    /// Was bei (x, y) liegt; `inhalt`: der Aufrufer zeigt dort etwas.
    pub fn klick(&self, x: f32, y: f32, inhalt: bool) -> Klick {
        if inside(self.zu, x, y) {
            Klick::Zu
        } else if inhalt && y >= self.inhalt_oben() {
            Klick::Inhalt
        } else {
            Klick::Daneben
        }
    }

    /// Dunkelt die Fläche ab; vor dem Inhalt.
    pub fn abdunkeln(&self, c: &mut Canvas) {
        let (x, y, w, h) = self.flaeche;
        c.fill_rect(x, y, w, h, DUNKEL);
    }

    /// Leiste mit `titel` ab `titel_x` (px), `hinweis` daneben, wenn Platz
    /// ist, und „Schließen ×“ (`hot`: unter der Maus); nach dem Inhalt.
    pub fn leiste(
        &self,
        c: &mut Canvas,
        fonts: &Fonts,
        (titel, titel_x): (&str, f32),
        hinweis: &str,
        hot: bool,
    ) {
        let s = self.scale;
        let (fx, fy, fw, _) = self.flaeche;
        c.fill_rect(fx, fy, fw, LEISTE * s, LEISTE_BG);
        let Some(regular) = fonts.regular.as_ref() else {
            return;
        };
        let fett = fonts.bold.as_ref().unwrap_or(regular);
        let px = 11.0 * s;
        let mitte = fy + LEISTE * s * 0.5;
        let base = mitte + regular.cap_height(px) * 0.5;
        let tx = titel_x.max(fx + RAND * s);
        fett.draw(c, titel, px, tx, base, HELL);
        let hx = tx + fett.width(titel, px) + 16.0 * s;
        if !hinweis.is_empty() && hx + regular.width(hinweis, px) < self.zu.0 - 12.0 * s {
            regular.draw(c, hinweis, px, hx, base, HELL_DIM);
        }
        let (zx, zy, zw, zh) = self.zu;
        let bg = Rgba(255, 255, 255, if hot { 70 } else { 36 });
        let mut p = Path::new();
        p.rounded_rect(zx, zy, zw, zh, zh * 0.5);
        c.fill(&p, bg);
        fett.draw(c, ZU, px, zx + 12.0 * s, base, HELL);
        let (mx, my, d) = (zx + zw - 16.0 * s, mitte, 3.5 * s);
        let mut p = Path::new();
        p.segment((mx - d, my - d), (mx + d, my + d), 1.5 * s);
        p.segment((mx - d, my + d), (mx + d, my - d), 1.5 * s);
        c.fill(&p, HELL);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn leer() -> Fonts {
        Fonts {
            regular: None,
            bold: None,
            italic: None,
        }
    }

    /// „Schließen“ rechts in der Leiste, nie über den Rand; Klicks: Knopf,
    /// Inhalt unter der Leiste, alles andere daneben.
    #[test]
    fn rahmen_und_klick() {
        for scale in [1.0_f32, 1.5] {
            let r = Schwebend::neu((0.0, 32.0, 800.0, 900.0), scale, 2000.0, &leer());
            let (zx, zy, zw, zh) = r.zu;
            assert!(zx + zw <= 800.0 - RAND * scale + 0.01);
            assert!(zy >= 32.0 && zy + zh <= r.inhalt_oben());
            let mitte = (zx + zw * 0.5, zy + zh * 0.5);
            assert_eq!(r.klick(mitte.0, mitte.1, true), Klick::Zu);
            let unten = r.inhalt_oben() + 10.0;
            assert_eq!(r.klick(400.0, unten, true), Klick::Inhalt);
            assert_eq!(r.klick(400.0, unten, false), Klick::Daneben);
            // In der Leiste neben dem Knopf schließt es, auch über Inhalt
            assert_eq!(r.klick(100.0, 40.0, true), Klick::Daneben);
        }
        // Rechts bündig mit dem Inhalt, wenn der schmaler ist
        let r = Schwebend::neu((0.0, 0.0, 800.0, 600.0), 1.0, 500.0, &leer());
        assert!((r.zu.0 + r.zu.2 - 500.0).abs() < 0.01);
    }

    #[test]
    fn zeichnet_ohne_schrift() {
        let r = Schwebend::neu((0.0, 10.0, 200.0, 100.0), 1.0, 200.0, &leer());
        let mut c = Canvas::new(200, 110);
        r.abdunkeln(&mut c);
        r.leiste(&mut c, &leer(), ("Titel", 0.0), HINWEIS, true);
        let px = c.to_rgba8();
        let i = (12 * 200 + 2) * 4;
        assert_eq!(&px[i..i + 3], &[34, 34, 36], "Leiste");
    }
}
