//! Große Druckvorschau (Jörn 11.10.): Die Lupe auf dem Blatt der
//! Druckvorschau legt die Seiten schwebend über das ganze Fenster, so
//! breit wie das Fenster (höchstens `BREIT_MAX`), untereinander zum Rollen.
//! Esc, ein Klick daneben oder „Schließen“ schließt sie; die kleine
//! Vorschau steht danach auf der zuletzt gezeigten Seite. Rahmen, Leiste
//! und Klick kommen aus dem gemeinsamen Baustein [`sk_ui::schwebend`].

use super::*;
use sk_paint::pdf::A4;
use sk_ui::schwebend::{self, Klick, Schwebend, LEISTE, RAND};

/// Breiteste Seite (dip), etwa anderthalbfache Papiergröße.
const BREIT_MAX: f32 = 1200.0;

/// Lage der großen Vorschau (px).
pub(super) struct GrossLage {
    /// Oberkante unter der Titelleiste des Fensters.
    pub(super) oben: f32,
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

    /// Lage der Seiten und der Rahmen mit „Schließen“ rechts über ihnen.
    pub(super) fn gross_lage(&self, fonts: &Fonts) -> (GrossLage, Schwebend) {
        let l = self.gross_seiten(self.seiten(fonts));
        let (w, h) = (self.w as f32, self.h as f32);
        let flaeche = (0.0, l.oben, w, h - l.oben);
        let rechts = l.x + A4.0 * l.k;
        let r = Schwebend::neu(flaeche, self.scale, rechts, fonts);
        (l, r)
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
            .is_some_and(|h| matches!(h, Hot::GrossZu | Hot::GrossSeite | Hot::Stelle(_)))
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
        let (l, r) = self.gross_lage(fonts);
        let seite = (0..l.seiten).find(|&i| inside(l.seite(i), x, y));
        match r.klick(x, y, seite.is_some()) {
            Klick::Zu => Some(Hot::GrossZu),
            // Eine Position wählt ihre Bauteile (Jörn 12.10.)
            Klick::Inhalt => {
                let stelle = seite.zip(self.blatt(fonts)).and_then(|(i, b)| {
                    let (sx, sy, ..) = l.seite(i);
                    self.stelle_bei(&b, i, (sx, sy, l.k), (x, y))
                });
                Some(stelle.map_or(Hot::GrossSeite, Hot::Stelle))
            }
            Klick::Daneben => None,
        }
    }

    /// Zeichnet die große Vorschau über das ganze Fenster (nach den Karten,
    /// siehe [`crate::quantity::QuantityWindow`]).
    pub fn paint_gross(&self, c: &mut Canvas, t: &Theme, fonts: &Fonts) {
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
        let h = self.h as f32;
        let (l, r) = self.gross_lage(fonts);
        r.abdunkeln(c);
        let sicht = r.inhalt_oben();
        for (i, seite) in b.seiten.iter().enumerate() {
            let (x, y, pw, ph) = l.seite(i);
            if y + ph < sicht || y > h {
                continue;
            }
            c.fill_rect(x + 2.0 * s, y + 3.0 * s, pw, ph, Rgba(0, 0, 0, 60));
            c.fill_rect(x, y, pw, ph, Rgba(255, 255, 255, 255));
            self.stellen_malen(c, t, &b, i, (x, y, l.k));
            sk_paint::pdf::zeichnen(c, seite, (x, y), l.k, regular, fett);
        }
        let titel = format!("Seite {} von {}", l.jetzt(h) + 1, l.seiten.max(1));
        // Lang, wenn er passt, sonst wie bisher
        let lang = format!(
            "Klick auf eine Position zeigt sie im Modell · Mausrad blättert · {}",
            schwebend::HINWEIS
        );
        let px = 11.0 * self.scale;
        let platz = r.zu.0 - l.x.max(RAND * self.scale) - 16.0 * self.scale;
        let titel_w = fett.width(&titel, px) + 16.0 * self.scale;
        let hinweis = if titel_w + regular.width(&lang, px) < platz {
            lang
        } else {
            format!("Mausrad blättert · {}", schwebend::HINWEIS)
        };
        let hot = self.hot == Some(Hot::GrossZu);
        r.leiste(c, fonts, (&titel, l.x), &hinweis, hot);
    }
}
