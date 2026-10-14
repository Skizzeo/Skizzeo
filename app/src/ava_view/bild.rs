//! Klickpunkte im Blatt AVA für die Ist-Bilder (nur Tests).

use super::*;

impl AvaView {
    /// Mitte der ersten Position, deren Kurztext `teil` enthält; liegt sie
    /// nicht ganz im Fenster, wird zuerst zu ihr gerollt.
    pub(crate) fn position_mitte(&mut self, t: &Theme, teil: &str) -> Option<(f64, f64)> {
        let i = self
            .zeilen
            .iter()
            .position(|z| z.art == Art::Position && z.text.contains(teil))?;
        let ganz = |v: &Self| {
            let (oben, unten) = v.liste_y();
            let (_, y, h) = v.sichtbar().into_iter().find(|(j, _, _)| *j == i)?;
            (y >= oben && y + h <= unten).then_some((y, h))
        };
        if ganz(self).is_none() {
            self.springe(&Sprung::Oz(self.zeilen[i].oz.clone()));
        }
        let (y, h) = ganz(self)?;
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

    /// Mitte der Lupe auf dem kleinen Blatt der Druckvorschau.
    pub(crate) fn lupe_mitte(&self, t: &Theme, fonts: &Fonts) -> (f64, f64) {
        let (x, y, w, h) = self.lupe_rect(t, fonts);
        ((x + 0.5 * w) as f64, (y + 0.5 * h) as f64)
    }

    /// Erste Position mit Bauteilen auf Seite `seite`: ihre Mitte im
    /// kleinen Blatt (gilt, wenn diese Seite gezeigt wird) und ihre Bauteile.
    pub(crate) fn stelle_auf(
        &self,
        t: &Theme,
        fonts: &Fonts,
        seite: usize,
    ) -> Option<((f64, f64), Vec<ElementId>)> {
        let b = self.blatt(fonts)?;
        let i = (0..b.stellen.len())
            .find(|&i| b.stellen[i].seite == seite && !self.stelle_ids(i).is_empty())?;
        let (bx, by, k) = self.blatt_lage(t, fonts);
        let (fx, fy, fw, fh) = b.stellen[i].flaeche;
        let m = (
            (bx + (fx + fw * 0.5) * k) as f64,
            (by + (fy + fh * 0.5) * k) as f64,
        );
        Some((m, self.stelle_ids(i).to_vec()))
    }

    /// Zeigt das Blatt die Druckvorschau?
    pub(crate) fn in_druckvorschau(&self) -> bool {
        self.ansicht == Ansicht::Blatt
    }

    /// Rollt die große Druckvorschau um `px`.
    pub(crate) fn gross_rollen_um(&mut self, px: f32) {
        if let Some(y) = self.gross.as_mut() {
            *y += px;
        }
    }

    /// Mitte der Leiste „LV ▾“ im schmalen Fenster.
    pub(crate) fn leiste_mitte(&self, t: &Theme, fonts: &Fonts) -> Option<(f64, f64)> {
        self.baum_als_leiste().then_some(())?;
        let ((x, y, w, h), _) = self.leiste_rect(t, fonts.bold.as_ref());
        Some(((x + w * 0.5) as f64, (y + h * 0.5) as f64))
    }
}
