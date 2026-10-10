//! Blatt AVA isoliert auf die Auswahl (Jörn 10.10., [`crate::fokus`]):
//! Leiste über der Tabelle mit Bauteilen, Zahl der Positionen im gezeigten
//! Los und den übrigen Losen, rechts „Alle Positionen“. Zeigt das gezeigte
//! Los nichts, aber ein anderes, wechselt das Blatt dorthin.

use super::{Ansicht, AvaView, Rect};
use crate::fokus::{self, Fokus};
use crate::scene::Scene;
use sk_cost::lv::{Lv, LvPosition, LvWahl};
use sk_model::{ElementId, Guid};
use sk_paint::Canvas;
use sk_ui::theme::Theme;
use sk_ui::widgets::Fonts;
use std::rc::Rc;

/// Lose (Guid, Nr., Name) wie im Baum.
type Lose = [(Guid, String, String)];

impl AvaView {
    /// Isolieren auf `auswahl` bzw. mit `None` wieder alles zeigen.
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

    /// Fokus aus Auswahl und Modell; beim neuen Fokus zur Tabelle und zum
    /// ersten Los mit verknüpften Positionen, wenn das gezeigte keine hat.
    /// `true`, wenn sich etwas ändert.
    pub(super) fn fokus_sync(
        &mut self,
        s: &mut Scene,
        firma: Option<(&sk_model::Library, u64)>,
        lose: &Lose,
        wahl: &dyn Fn(Guid) -> LvWahl,
    ) -> bool {
        let von = (self.fokus_wunsch, s.model().revision());
        if self.fokus_von == Some(von) {
            return false;
        }
        let neu_gewuenscht = self.fokus_von.is_none_or(|v| v.0 != von.0);
        self.fokus_von = Some(von);
        let neu = self
            .fokus_auswahl
            .as_deref()
            .map(|a| Fokus::neu(s.model(), a));
        if neu == self.fokus {
            return false;
        }
        self.fokus = neu;
        self.fokus_stand += 1;
        let Some(f) = self.fokus.clone().filter(|_| neu_gewuenscht) else {
            return true;
        };
        if matches!(self.ansicht, Ansicht::Zusammenstellung | Ansicht::Pruefen) {
            self.zeige(Ansicht::Lv);
        }
        let zahl = |s: &mut Scene, g: Guid| {
            let lv = s.lv(firma, &self.leiste.umfang, &wahl(g));
            let b = s.kostenblatt(firma, &self.leiste.umfang);
            verknuepft(&lv, &b, &f).len()
        };
        let hier = self.los.map_or(0, |g| zahl(s, g));
        if hier == 0 {
            if let Some(g) = lose.iter().map(|l| l.0).find(|g| zahl(s, *g) > 0) {
                self.waehle_los(g);
                if !self.offen.contains(&g) {
                    self.offen.push(g);
                }
            }
        }
        true
    }

    /// Höhe der Leiste über der Tabelle (dip); nur in der Ansicht LV.
    pub(super) fn fokus_h(&self) -> f32 {
        if self.fokus.is_some() && self.ansicht == Ansicht::Lv {
            fokus::H
        } else {
            0.0
        }
    }

    /// „Verknüpft mit FU-001 · 4 Positionen · LV Rohbau“ und die übrigen
    /// Lose mit verknüpften Positionen („weitere in LV Ausbau“).
    pub(super) fn fokus_leiste_text(
        &self,
        s: &mut Scene,
        firma: Option<(&sk_model::Library, u64)>,
        lose: &Lose,
        wahl: &dyn Fn(Guid) -> LvWahl,
        lv: &Rc<Lv>,
    ) -> String {
        let Some(f) = &self.fokus else {
            return String::new();
        };
        let b = s.kostenblatt(firma, &self.leiste.umfang);
        let hier = verknuepft(lv, &b, f).len();
        let mut text = f.text(hier, &format!("LV {}", lv.kopf.los));
        let andere: Vec<String> = lose
            .iter()
            .filter(|l| Some(l.0) != self.los)
            .filter_map(|l| {
                let x = s.lv(firma, &self.leiste.umfang, &wahl(l.0));
                (!verknuepft(&x, &b, f).is_empty()).then(|| format!("LV {}", l.2))
            })
            .collect();
        if !andere.is_empty() {
            let sep = if hier == 0 { " " } else { " · " };
            text = format!(
                "{}{sep}weitere in {}",
                text.trim_end_matches('.'),
                andere.join(", ")
            );
        }
        text
    }

    /// Text der Leiste (Tests).
    #[cfg(test)]
    pub fn fokus_text(&self) -> Option<&str> {
        self.fokus.as_ref().map(|_| self.fokus_text.as_str())
    }

    /// Pille der Leiste (px) über der Linie vor Baum und Tabelle.
    fn fokus_pille(&self, t: &Theme) -> Option<Rect> {
        if self.fokus_h() == 0.0 {
            return None;
        }
        let s = self.scale;
        let (x0, cw) = self.content_x(t);
        let y = self.body_top() - (fokus::H + 6.0) * s;
        Some(fokus::pille(x0, y, cw, s))
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

/// Positionen des LV, die an der Auswahl hängen (über die Zeilen des
/// Kostenblatts hinter ihnen, je Geschoss).
pub(super) fn verknuepft<'a>(
    lv: &'a Lv,
    b: &sk_cost::Kostenblatt,
    f: &Fokus,
) -> Vec<&'a LvPosition> {
    lv.titel
        .iter()
        .flat_map(|t| &t.positionen)
        .filter(|p| {
            p.blatt
                .iter()
                .filter_map(|&i| b.positionen.get(i))
                .flat_map(|x| &x.ansatz)
                .any(|a| {
                    f.trifft(a)
                        && p.ansatz
                            .iter()
                            .any(|z| z.geschoss == a.geschoss && z.elemente.contains(&a.element))
                })
        })
        .collect()
}
