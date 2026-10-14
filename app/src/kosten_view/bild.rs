//! Klickpunkte im Reiter Kosten für die Ist-Bilder (nur Tests).

use super::*;

impl KostenView {
    /// Mitte des EP der ersten Zeile, deren Text `teil` enthält; liegt sie
    /// nicht ganz im Fenster, wird zuerst zu ihr gerollt.
    pub(crate) fn ep_mitte(&mut self, t: &Theme, teil: &str) -> Option<(f64, f64)> {
        let (_, y, h) = self.zeige(|z| z.text.contains(teil))?;
        let (x0, cw) = self.content_x(t);
        Some(((col_ep(x0, cw) - 20.0) as f64, (y + h * 0.5) as f64))
    }

    /// Mitte von „Bauleistung wählen …“ der ersten grauen Zeile, deren
    /// Text `teil` enthält und für die es etwas zu wählen gibt; wie
    /// [`Self::ep_mitte`] zuerst zu ihr gerollt.
    pub(crate) fn waehlen_mitte(
        &mut self,
        t: &Theme,
        fonts: &Fonts,
        teil: &str,
    ) -> Option<(f64, f64)> {
        let waehlbar = self.waehlbar.clone();
        let (_, y, h) =
            self.zeige(|z| z.text.contains(teil) && z.ohne.is_some_and(|j| waehlbar.contains(&j)))?;
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

    /// Erste Zeile, die `f` erfüllt, mit Oberkante und Höhe (px); liegt sie
    /// nicht ganz in der Liste, wird zuerst zu ihr gerollt.
    fn zeige(&mut self, f: impl Fn(&Zeile) -> bool) -> Option<(usize, f32, f32)> {
        let i = self.zeilen.iter().position(&f)?;
        let ganz = |v: &Self| {
            let (top, bottom) = (v.list_top(), v.list_bottom());
            let (_, y, h) = v.sichtbar().into_iter().find(|(j, _, _)| *j == i)?;
            (y >= top && y + h <= bottom).then_some((i, y, h))
        };
        if ganz(self).is_none() {
            self.springe(f);
        }
        ganz(self)
    }
}
