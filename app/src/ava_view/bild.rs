//! Klickpunkte im Blatt AVA für die Ist-Bilder (nur Tests).

use super::*;

impl AvaView {
    /// Mitte der ersten Position, deren Kurztext `teil` enthält.
    pub(crate) fn position_mitte(&self, t: &Theme, teil: &str) -> Option<(f64, f64)> {
        let i = self
            .zeilen
            .iter()
            .position(|z| z.art == Art::Position && z.text.contains(teil))?;
        let (_, y, h) = self.sichtbar().into_iter().find(|(j, _, _)| *j == i)?;
        let (tx, _) = self.tabelle_x(t);
        Some(((tx + 40.0) as f64, (y + h * 0.5) as f64))
    }

    /// Mitte des ersten Baumknotens, auf den `pred` passt.
    pub(crate) fn knoten_mitte(
        &self,
        t: &Theme,
        pred: impl Fn(&Knoten) -> bool,
    ) -> Option<(f64, f64)> {
        let (_, (x, y, w, h)) = self
            .baum_lage(t)
            .into_iter()
            .find(|(i, _)| pred(&self.baum[*i]))?;
        Some(((x + w * 0.5) as f64, (y + h * 0.5) as f64))
    }

    /// Mitte von „Für Anfrage (leer)“ (`0`) oder „Mit Preisen“ (`1`).
    pub(crate) fn schalter_mitte(&self, t: &Theme, fonts: &Fonts, i: usize) -> (f64, f64) {
        let (_, segs) = self.schalter(t, fonts);
        let (_, (x, y, w, h)) = segs[i];
        ((x + w * 0.5) as f64, (y + h * 0.5) as f64)
    }

    /// Mitte von „Kopf und Vorbemerkungen“, „Bauherr fehlt“ oder „Mehr“.
    pub(crate) fn kopf_mitte(&self, t: &Theme, fonts: &Fonts, was: &str) -> Option<(f64, f64)> {
        let ziel = match was {
            "Mehr" => Hot::Mehr,
            "Bauherr fehlt" | "Aufsteller fehlt" => Hot::BauherrFehlt,
            _ => Hot::Kopf,
        };
        let (x0, cw) = self.content_x(t);
        let y = self.top_px() + (SWITCH_TOP + self.kopfzeile_dy() + SWITCH_H * 0.5) * self.scale;
        let mut x = x0;
        while x < x0 + cw {
            if self.hit(t, fonts, x as f64, y as f64) == Some(ziel) {
                return Some((x as f64 + 4.0, y as f64));
            }
            x += 2.0;
        }
        None
    }

    /// Druckvorschau auf Seite `seite` (ab 0).
    pub(crate) fn zeige_druckvorschau(&mut self, seite: usize) {
        self.zeige(Ansicht::Blatt);
        self.seite = seite;
    }

    /// Mitte der Leiste „LV ▾“ im schmalen Fenster.
    pub(crate) fn leiste_mitte(&self, t: &Theme, fonts: &Fonts) -> Option<(f64, f64)> {
        self.baum_als_leiste().then_some(())?;
        let ((x, y, w, h), _) = self.leiste_rect(t, fonts.bold.as_ref());
        Some(((x + w * 0.5) as f64, (y + h * 0.5) as f64))
    }
}
