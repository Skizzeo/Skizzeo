//! Druckvorschau mit dem Modell verknüpft (Jörn 12.10.): Ein Klick auf eine
//! Position des Blatts, klein oder groß, wählt ihre Bauteile wie ein Klick
//! in der Tabelle; Auswahl und Hover im Modell markieren die Positionen im
//! Blatt, und eine neue Auswahl von außen blättert zu ihrer ersten. Die
//! Bauteile je Position sind dieselben wie in der Tabelle ([`lv_zeilen_mit`]
//! ohne Isolieren). Groß zeigt das Blatt nun die Lupe oben rechts oder ein
//! Klick neben die Positionen.

use super::*;
use crate::lv_blatt::{Blatt, Stelle};
use sk_paint::pdf::A4;
use std::collections::HashSet;

/// Lupe oben rechts auf dem kleinen Blatt (dip).
const LUPE: f32 = 26.0;

impl AvaView {
    /// Bauteile der Position mit dieser OZ.
    fn ids_fuer(&self, oz: &str) -> &[ElementId] {
        self.blatt_ids
            .iter()
            .find(|(o, _)| o == oz)
            .map_or(&[], |(_, v)| v.as_slice())
    }

    /// Bauteile der Position `i` des zuletzt gebauten Blatts.
    pub(super) fn stelle_ids(&self, i: usize) -> &[ElementId] {
        let oz = self
            .blatt_cache
            .borrow()
            .as_ref()
            .and_then(|(_, b)| b.stellen.get(i).map(|s| s.oz.clone()));
        oz.map_or(&[], |oz| self.ids_fuer(&oz))
    }

    /// Position mit Bauteilen unter (x, y) auf Seite `seite`; das Blatt
    /// liegt bei (bx, by) mit `k` Pixeln je Punkt.
    pub(super) fn stelle_bei(
        &self,
        b: &Blatt,
        seite: usize,
        (bx, by, k): (f32, f32, f32),
        (x, y): (f32, f32),
    ) -> Option<usize> {
        let (px, py) = ((x - bx) / k, (y - by) / k);
        b.stellen.iter().position(|s| {
            s.seite == seite && inside(s.flaeche, px, py) && !self.ids_fuer(&s.oz).is_empty()
        })
    }

    /// Lupe oben rechts auf dem kleinen Blatt (px).
    pub(super) fn lupe_rect(&self, t: &Theme, fonts: &Fonts) -> Rect {
        let (bx, by, k) = self.blatt_lage(t, fonts);
        let s = self.scale;
        let d = LUPE * s;
        (bx + A4.0 * k - d - 6.0 * s, by + 6.0 * s, d, d)
    }

    /// Klick auf eine Position: ihre Bauteile sind gewählt, wie ein Klick
    /// in der Tabelle (isoliert nicht).
    pub(super) fn stelle_waehlen(&mut self, i: usize, p: &mut Picking) -> Option<ListOut> {
        let ids = self.stelle_ids(i).to_vec();
        if ids.is_empty() {
            return None;
        }
        p.selected = ids;
        self.selected = p.selected.clone();
        self.blatt_gewaehlt = self
            .blatt_cache
            .borrow()
            .as_ref()
            .and_then(|(_, b)| b.stellen.get(i).map(|s| s.oz.clone()));
        Some(ListOut::Picking { selection: true })
    }

    /// Gehört die Position zur Auswahl? Nach einem Klick in der Vorschau
    /// nur die angeklickte; zu einer Auswahl von außen die Positionen, auf
    /// die die Tabelle isoliert (ohne Baustelleneinrichtung, mit der
    /// Gründung); sonst jede mit einem gewählten Bauteil.
    fn stelle_gewaehlt(&self, st: &Stelle, sel: &HashSet<ElementId>) -> bool {
        if sel.is_empty() {
            return false;
        }
        if let Some(oz) = &self.blatt_gewaehlt {
            return st.oz == *oz;
        }
        if self
            .fokus
            .as_ref()
            .is_some_and(|f| f.auswahl == self.selected)
        {
            return self.blatt_fokus.contains(&st.oz);
        }
        self.ids_fuer(&st.oz).iter().any(|e| sel.contains(e))
    }

    /// Markiert die Positionen der Seite `seite` unter dem Text: gewählte
    /// im Akzent mit Strich, die unter der Maus bzw. im Modell überfahrenen
    /// grau. Das Papier bleibt weiß, auch im dunklen Schema.
    pub(super) fn stellen_malen(
        &self,
        c: &mut Canvas,
        t: &Theme,
        b: &Blatt,
        seite: usize,
        (x, y, k): (f32, f32, f32),
    ) {
        let s = self.scale;
        let a = t.ui.accent;
        let sel: HashSet<ElementId> = self.selected.iter().copied().collect();
        let hov: HashSet<ElementId> = self.hover.iter().copied().collect();
        let maus = matches!(self.hot, Some(Hot::Stelle(_)));
        for (i, st) in b.stellen.iter().enumerate() {
            if st.seite != seite {
                continue;
            }
            let ids = self.ids_fuer(&st.oz);
            if ids.is_empty() {
                continue;
            }
            let (fx, fy, fw, fh) = st.flaeche;
            let r = (x + fx * k, y + fy * k, fw * k, fh * k);
            let schwebt = if maus {
                self.hot == Some(Hot::Stelle(i))
            } else {
                ids.iter().any(|e| hov.contains(e))
            };
            if self.stelle_gewaehlt(st, &sel) {
                c.fill_rect(r.0, r.1, r.2, r.3, Rgba(a.0, a.1, a.2, 52));
                c.fill_rect(r.0, r.1, 3.0 * s, r.3, a);
            } else if schwebt {
                c.fill_rect(r.0, r.1, r.2, r.3, Rgba(0, 0, 0, 20));
            }
        }
    }

    /// Lupe: dunkler Kreis mit hellem Glas und Griff.
    pub(super) fn paint_lupe(&self, c: &mut Canvas, t: &Theme, fonts: &Fonts) {
        let s = self.scale;
        let (x, y, d, _) = self.lupe_rect(t, fonts);
        let hot = self.hot == Some(Hot::Lupe);
        let mut p = Path::new();
        p.rounded_rect(x, y, d, d, d * 0.5);
        c.fill(
            &p,
            if hot {
                t.ui.accent
            } else {
                Rgba(40, 40, 44, 200)
            },
        );
        let hell = if hot {
            t.ui.on_accent
        } else {
            Rgba(255, 255, 255, 255)
        };
        let (mx, my, r, b) = (x + d * 0.44, y + d * 0.44, 5.5 * s, 1.6 * s);
        let mut p = Path::new();
        p.rounded_rect(mx - r, my - r, 2.0 * r, 2.0 * r, r);
        p.rounded_rect_hole(mx - r + b, my - r + b, 2.0 * (r - b), 2.0 * (r - b), r - b);
        let g = r * 0.7;
        p.segment(
            (mx + g, my + g),
            (mx + g + 4.5 * s, my + g + 4.5 * s),
            2.0 * s,
        );
        c.fill(&p, hell);
    }

    /// Nach einer Auswahl von außen: zur ersten Position mit gewählten
    /// Bauteilen, wenn die gezeigte Seite (groß: das Sichtbare) keine hat.
    /// `true`, wenn sich das Bild ändert.
    pub fn blatt_folgen(&mut self, fonts: &Fonts) -> bool {
        if !std::mem::take(&mut self.blatt_folgt) || self.ansicht != Ansicht::Blatt {
            return false;
        }
        let Some(b) = self.blatt(fonts) else {
            return false;
        };
        let sel: HashSet<ElementId> = self.selected.iter().copied().collect();
        let treffer: Vec<&Stelle> = b
            .stellen
            .iter()
            .filter(|st| self.stelle_gewaehlt(st, &sel))
            .collect();
        let Some(erste) = treffer.first() else {
            return false;
        };
        if self.gross_offen() {
            let (l, r) = self.gross_lage(fonts);
            let (oben, h) = (r.inhalt_oben(), self.h as f32);
            let sichtbar = |st: &&Stelle| {
                let y = l.seite(st.seite).1 + st.flaeche.1 * l.k;
                y >= oben && y + st.flaeche.3 * l.k <= h
            };
            if treffer.iter().any(sichtbar) {
                return false;
            }
            // Die Position ein Viertel unter der Oberkante des Sichtbaren
            let ziel = l.y0 + erste.seite as f32 * l.schritt + erste.flaeche.1 * l.k
                - (oben + (h - oben) * 0.25);
            self.gross = Some(ziel.clamp(0.0, l.max));
            return true;
        }
        if treffer.iter().any(|st| st.seite == self.seite_jetzt(fonts)) {
            return false;
        }
        self.seite = erste.seite;
        true
    }
}
