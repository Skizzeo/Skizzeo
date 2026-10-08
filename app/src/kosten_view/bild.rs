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
        let (_, _, _, _, (x, y, w, h), _) = self.abgleich_lage(t, fonts)?;
        Some(((x + w * 0.5) as f64, (y + h * 0.5) as f64))
    }
}
