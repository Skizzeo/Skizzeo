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
}
