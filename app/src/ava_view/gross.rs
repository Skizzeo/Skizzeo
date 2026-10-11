//! Große Druckvorschau (Jörn 11.10.): Ein Klick auf das Blatt der
//! Druckvorschau legt die Seiten schwebend über das ganze Fenster, so
//! breit wie das Fenster (höchstens `BREIT_MAX`), untereinander zum Rollen.
//! Esc, ein Klick daneben oder „Schließen“ schließt sie; die kleine
//! Vorschau steht danach auf der zuletzt gezeigten Seite.

use super::*;
use sk_paint::pdf::A4;

/// Leiste oben mit „Seite x von y“ und „Schließen“ (dip).
const LEISTE: f32 = 44.0;
/// Rand links und rechts, Luft zwischen den Seiten (dip).
const RAND: f32 = 16.0;
/// Breiteste Seite (dip), etwa anderthalbfache Papiergröße.
const BREIT_MAX: f32 = 1200.0;
const ZU: &str = "Schließen";
/// Abdunkeln des Fensters und Fläche der Leiste, in jedem Schema dunkel.
const DUNKEL: Rgba = Rgba(0, 0, 0, 170);
const LEISTE_BG: Rgba = Rgba(34, 34, 36, 255);
const HELL: Rgba = Rgba(255, 255, 255, 235);
const HELL_DIM: Rgba = Rgba(255, 255, 255, 150);

/// Lage der großen Vorschau (px).
pub(super) struct GrossLage {
    /// Oberkante unter der Titelleiste des Fensters.
    pub(super) oben: f32,
    pub(super) zu: Rect,
    /// Linke Kante der Seiten und Pixel je Punkt.
    pub(super) x: f32,
    pub(super) k: f32,
    /// Oberkante der ersten Seite ungerollt; Abstand von Seite zu Seite.
    pub(super) y0: f32,
    pub(super) schritt: f32,
    pub(super) seiten: usize,
    /// Rollstand, auf `0..=max` begrenzt.
    pub(super) scroll: f32,
    pub(super) max: f32,
}

impl GrossLage {
    /// Fläche der Seite `i` (px).
    pub(super) fn seite(&self, i: usize) -> Rect {
        let y = self.y0 - self.scroll + i as f32 * self.schritt;
        (self.x, y, A4.0 * self.k, A4.1 * self.k)
    }

    /// Die gezeigte Seite (ab 0): die in der Mitte des Sichtbaren, bei
    /// kleinen Seiten die oben; `h` ist die Höhe des Fensters.
    pub(super) fn jetzt(&self, h: f32) -> usize {
        let m = self.scroll + ((h - self.y0) * 0.5).min(self.schritt * 0.5);
        ((m / self.schritt).max(0.0) as usize).min(self.seiten.saturating_sub(1))
    }
}

impl AvaView {
    pub fn gross_offen(&self) -> bool {
        self.gross.is_some() && self.ansicht == Ansicht::Blatt
    }

    pub(super) fn gross_lage(&self, fonts: &Fonts) -> GrossLage {
        let mut l = self.gross_seiten(self.seiten(fonts));
        let s = self.scale;
        let bold = fonts.bold.as_ref().or(fonts.regular.as_ref());
        let zw = bold.map_or(60.0 * s, |f| f.width(ZU, 11.0 * s)) + 40.0 * s;
        let zh = 26.0 * s;
        let r = (l.x + A4.0 * l.k).min(self.w as f32 - RAND * s);
        l.zu = (r - zw, l.oben + (LEISTE * s - zh) * 0.5, zw, zh);
        l
    }

    /// Lage der `seiten` Seiten, ohne „Schließen“.
    fn gross_seiten(&self, seiten: usize) -> GrossLage {
        let s = self.scale;
        let (w, h) = (self.w as f32, self.h as f32);
        let oben = ((self.top - crate::cards::HEIGHT) * s).max(0.0);
        let pw = (w - 2.0 * RAND * s).min(BREIT_MAX * s).max(40.0 * s);
        let k = pw / A4.0;
        let y0 = oben + (LEISTE + RAND * 0.5) * s;
        let schritt = A4.1 * k + RAND * s;
        let inhalt = seiten as f32 * schritt + RAND * 0.5 * s;
        let max = (inhalt - (h - y0)).max(0.0);
        GrossLage {
            oben,
            zu: (0.0, 0.0, 0.0, 0.0),
            x: (w - pw) * 0.5,
            k,
            y0,
            schritt,
            seiten,
            scroll: self.gross.unwrap_or(0.0).clamp(0.0, max),
            max,
        }
    }

    /// Öffnet die große Vorschau auf der gezeigten Seite.
    pub(super) fn gross_oeffnen(&mut self, fonts: &Fonts) {
        let l = self.gross_seiten(self.seiten(fonts));
        let y = self.seite_jetzt(fonts) as f32 * l.schritt;
        self.gross = Some(y.min(l.max));
        self.hot = None;
    }

    /// Seiten des zuletzt gebauten Blatts (Rad und Esc kennen die Schrift
    /// nicht; gebaut ist es, seit die große Vorschau offen ist).
    fn seiten_gebaut(&self) -> usize {
        self.blatt_cache
            .borrow()
            .as_ref()
            .map_or(1, |(_, b)| b.seiten.len().max(1))
    }

    /// Schließt sie; die kleine Vorschau zeigt die Seite aus ihrer Mitte.
    /// `true`, wenn sie offen war.
    pub fn gross_schliessen(&mut self) -> bool {
        if !self.gross_offen() {
            self.gross = None;
            return false;
        }
        self.seite = self.gross_seiten(self.seiten_gebaut()).jetzt(self.h as f32);
        self.gross = None;
        if self
            .hot
            .is_some_and(|h| matches!(h, Hot::GrossZu | Hot::GrossSeite))
        {
            self.hot = None;
        }
        true
    }

    pub(super) fn gross_rollen(&mut self, delta: f64, t: &Theme) -> Option<ListOut> {
        self.gross?;
        let l = self.gross_seiten(self.seiten_gebaut());
        let vorher = l.scroll;
        let neu = (vorher - delta as f32 * 4.0 * t.size.qto_row * self.scale).clamp(0.0, l.max);
        self.gross = Some(neu);
        (neu != vorher).then_some(ListOut::Repaint)
    }

    pub(super) fn gross_hit(&self, fonts: &Fonts, x: f32, y: f32) -> Option<Hot> {
        let l = self.gross_lage(fonts);
        if inside(l.zu, x, y) {
            return Some(Hot::GrossZu);
        }
        if y < l.oben + LEISTE * self.scale {
            return None;
        }
        (0..l.seiten)
            .any(|i| inside(l.seite(i), x, y))
            .then_some(Hot::GrossSeite)
    }

    /// Zeichnet die große Vorschau über das ganze Fenster (nach den Karten,
    /// siehe [`crate::quantity::QuantityWindow`]).
    pub fn paint_gross(&self, c: &mut Canvas, fonts: &Fonts) {
        if !self.gross_offen() {
            return;
        }
        let Some(regular) = fonts.regular.as_ref() else {
            return;
        };
        let fett = fonts.bold.as_ref().unwrap_or(regular);
        let Some(b) = self.blatt(fonts) else {
            return;
        };
        let s = self.scale;
        let (w, h) = (self.w as f32, self.h as f32);
        let l = self.gross_lage(fonts);
        c.fill_rect(0.0, l.oben, w, h - l.oben, DUNKEL);
        let sicht = l.oben + LEISTE * s;
        for (i, seite) in b.seiten.iter().enumerate() {
            let (x, y, pw, ph) = l.seite(i);
            if y + ph < sicht || y > h {
                continue;
            }
            c.fill_rect(x + 2.0 * s, y + 3.0 * s, pw, ph, Rgba(0, 0, 0, 60));
            c.fill_rect(x, y, pw, ph, Rgba(255, 255, 255, 255));
            sk_paint::pdf::zeichnen(c, seite, (x, y), l.k, regular, fett);
        }
        // Die Leiste deckt Seiten, die nach oben gerollt sind
        c.fill_rect(0.0, l.oben, w, LEISTE * s, LEISTE_BG);
        let px = 11.0 * s;
        let mitte = l.oben + LEISTE * s * 0.5;
        let base = mitte + regular.cap_height(px) * 0.5;
        let text = format!("Seite {} von {}", l.jetzt(h) + 1, l.seiten.max(1));
        let tx = l.x.max(RAND * s);
        fett.draw(c, &text, px, tx, base, HELL);
        let hinweis = "Mausrad blättert · Esc oder Klick daneben schließt";
        let hx = tx + fett.width(&text, px) + 16.0 * s;
        if hx + regular.width(hinweis, px) < l.zu.0 - 12.0 * s {
            regular.draw(c, hinweis, px, hx, base, HELL_DIM);
        }
        // „Schließen ×“
        let (zx, zy, zw, zh) = l.zu;
        let bg = if self.hot == Some(Hot::GrossZu) {
            Rgba(255, 255, 255, 70)
        } else {
            Rgba(255, 255, 255, 36)
        };
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
