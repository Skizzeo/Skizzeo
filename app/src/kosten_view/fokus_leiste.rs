//! Reiter Kosten isoliert auf die Auswahl (Jörn 10.10., [`crate::fokus`]):
//! Leiste über der Liste mit Bauteilen, Zahl der Positionen und ihrem
//! Anteil, rechts „Alle Positionen“.

use super::{euro, Art, KostenView, Rect, HEAD};
use crate::fokus::{self, Fokus};
use sk_cost::Cent;
use sk_model::{ElementId, Model};
use sk_paint::Canvas;
use sk_ui::theme::Theme;
use sk_ui::widgets::Fonts;
use std::collections::HashSet;

/// Tooltip an der Leiste (Review 10.10.): wie die Summe entsteht.
pub(super) const TIP: &str = "Summe der Anteile, je Position auf den Cent gerundet. \
Erdarbeiten und Baustelleneinrichtung zählen an jedem Teil der Gründung voll.";

impl KostenView {
    /// Isolieren auf `auswahl` bzw. mit `None` wieder alles zeigen. Beim
    /// Isolieren steht die Liste oben, danach wieder wo sie vorher stand.
    pub(super) fn fokus_setzen(&mut self, auswahl: Option<Vec<ElementId>>) {
        if self.fokus_auswahl == auswahl {
            return;
        }
        match (&self.fokus_auswahl, &auswahl) {
            (None, Some(_)) => {
                self.scroll_vorher = Some(self.scroll);
                self.scroll = 0.0;
            }
            (Some(_), Some(_)) => self.scroll = 0.0,
            (_, None) => self.scroll = self.scroll_vorher.take().unwrap_or(0.0),
        }
        self.fokus_auswahl = auswahl;
        self.fokus_wunsch += 1;
    }

    /// Den Fokus aus Auswahl und Modell bestimmen; `true`, wenn er sich
    /// ändert (dann baut `sync` die Zeilen neu).
    pub(super) fn fokus_sync(&mut self, m: &Model) -> bool {
        let von = (self.fokus_wunsch, m.revision());
        if self.fokus_von == Some(von) {
            return false;
        }
        self.fokus_von = Some(von);
        let neu = self.fokus_auswahl.as_deref().map(|a| Fokus::neu(m, a));
        if neu == self.fokus {
            return false;
        }
        self.fokus = neu;
        self.fokus_stand += 1;
        true
    }

    /// Höhe der Leiste im Kopf (dip).
    pub(super) fn fokus_h(&self) -> f32 {
        if self.fokus.is_some() {
            fokus::H
        } else {
            0.0
        }
    }

    /// „Verknüpft mit FU-001 · 6 Positionen · 3.456,78 €“: Positionen der
    /// Zeilen und die Summe ihrer Anteile.
    pub(super) fn fokus_leiste_text(&self) -> String {
        let Some(f) = &self.fokus else {
            return String::new();
        };
        let n = self
            .zeilen
            .iter()
            .filter(|z| matches!(z.art, Art::Position { .. }))
            .filter_map(|z| z.pos.map(|p| p.0))
            .collect::<HashSet<usize>>()
            .len();
        let summe: Option<Cent> = self
            .zeilen
            .iter()
            .filter(|z| z.art == Art::Gruppe && z.ebene == 0)
            .filter_map(|z| z.betrag)
            .fold(None, |acc, c| Some(acc.unwrap_or(Cent(0)) + c));
        let zusatz = summe.map_or_else(String::new, |c| format!("{} €", euro(c)));
        f.text(n, &zusatz)
    }

    /// Text der Leiste (Tests, Ist-Bilder).
    #[cfg(test)]
    pub fn fokus_text(&self) -> Option<&str> {
        self.fokus.as_ref().map(|_| self.fokus_text.as_str())
    }

    /// Pille der Leiste (px) zwischen Schaltern und Spaltenköpfen.
    pub(super) fn fokus_pille(&self, t: &Theme) -> Option<Rect> {
        self.fokus.as_ref()?;
        let s = self.scale;
        let (x0, cw) = self.content_x(t);
        let oben = HEAD - 34.0 + self.ab() + self.zeile2() + 8.0;
        Some(fokus::pille(x0, self.top_px() + oben * s, cw, s))
    }

    /// „Alle Positionen“ (px).
    pub(super) fn fokus_verweis(&self, t: &Theme, fonts: &Fonts) -> Option<Rect> {
        let p = self.fokus_pille(t)?;
        Some(fokus::verweis_rect(fonts, p, self.scale))
    }

    pub(super) fn paint_fokus(&self, c: &mut Canvas, t: &Theme, fonts: &Fonts) {
        let Some(p) = self.fokus_pille(t) else {
            return;
        };
        let hot = self.hot == Some(super::Hot::Alle);
        fokus::paint(c, t, fonts, p, &self.fokus_text, hot, self.scale);
    }
}
