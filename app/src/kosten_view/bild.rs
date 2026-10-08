//! Klickpunkte im Reiter Kosten für die Ist-Bilder (nur Tests).

use super::*;

impl KostenView {
    /// Mitte des EP der ersten Zeile, deren Text `teil` enthält.
    pub(crate) fn ep_mitte(&self, t: &Theme, teil: &str) -> Option<(f64, f64)> {
        let i = self.zeilen.iter().position(|z| z.text.contains(teil))?;
        let (_, y, h) = self.sichtbar().into_iter().find(|(j, _, _)| *j == i)?;
        let (x0, cw) = self.content_x(t);
        Some(((col_ep(x0, cw) - 20.0) as f64, (y + h * 0.5) as f64))
    }

    /// Mitte von „Bauleistung wählen …“ der ersten grauen Zeile, deren
    /// Text `teil` enthält und für die es etwas zu wählen gibt.
    pub(crate) fn waehlen_mitte(&self, t: &Theme, fonts: &Fonts, teil: &str) -> Option<(f64, f64)> {
        let i = self.zeilen.iter().position(|z| {
            z.text.contains(teil) && z.ohne.is_some_and(|j| self.waehlbar.contains(&j))
        })?;
        let (_, y, h) = self.sichtbar().into_iter().find(|(j, _, _)| *j == i)?;
        let (x, y, w, h) = self.waehlen_rect(t, fonts, y, h);
        Some(((x + w * 0.5) as f64, (y + h * 0.5) as f64))
    }

    /// Mitte von „übernehmen“ in der Abgleichzeile.
    pub(crate) fn uebernehmen_mitte(&self, t: &Theme, fonts: &Fonts) -> Option<(f64, f64)> {
        let (x, y, w, h) = self.abgleich_lage(t, fonts)?.uebernehmen;
        Some(((x + w * 0.5) as f64, (y + h * 0.5) as f64))
    }

    /// Mitte von „Unterschiede ansehen“ in der Abgleichzeile (KA-3a5).
    pub(crate) fn ansehen_mitte(&self, t: &Theme, fonts: &Fonts) -> Option<(f64, f64)> {
        let (x, y, w, h) = self.abgleich_lage(t, fonts)?.ansehen;
        Some(((x + w * 0.5) as f64, (y + h * 0.5) as f64))
    }

    /// Mitte von Teil `nr` des Segments „je m² | je m³ | je Stück“ im
    /// offenen Preisblatt (KA-3a7).
    pub(crate) fn je_mitte(&mut self, t: &Theme, fonts: &Fonts, nr: usize) -> Option<(f64, f64)> {
        self.lege_preis(t);
        self.preis.as_ref()?.je_mitte(fonts, nr)
    }
}
