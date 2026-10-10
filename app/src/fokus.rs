//! Auswahl im Modell → verknüpfte Positionen in Kosten und AVA (Jörn
//! 10.10.): Was im Modell, im Mengenblatt oder im anderen Blatt gewählt
//! wird, zeigen Kosten und AVA isoliert mit den Positionen, die daran
//! hängen, auch den abgeleiteten wie den Erdarbeiten am Fundament
//! ([`sk_model::qto::gruendung_von`]). Eine Leiste über der Liste nennt die
//! Auswahl und führt mit „Alle Positionen“ zurück zur ganzen Liste.
//!
//! Hier steht nur, was beide Blätter gleich brauchen: welche Ansatzzeilen
//! zur Auswahl gehören, die Bauteile einer Ansatzzeile und die Leiste.

use sk_cost::rechnung::{Ansatz, OhneZeile};
use sk_model::element::ElementKind;
use sk_model::{ElementId, Model};
use sk_paint::{Canvas, Path};
use sk_ui::theme::Theme;
use sk_ui::widgets::Fonts;

/// Höhe der Leiste samt Luft (dip), um die die Liste nach unten rückt.
pub const H: f32 = 32.0;
/// Höhe der Pille (dip).
const PILLE_H: f32 = 24.0;
/// Verweis rechts in der Leiste.
pub const ALLE: &str = "Alle Positionen";
/// So viele Bauteilnummern nennt die Leiste, dann „und n weitere“.
const NAMEN: usize = 3;

type Rect = (f32, f32, f32, f32);

/// Die Auswahl, auf die ein Blatt isoliert ist.
#[derive(Clone, Debug, PartialEq)]
pub struct Fokus {
    pub auswahl: Vec<ElementId>,
    /// Sohlplatten, deren Automatikmengen aus der Auswahl folgen.
    platten: Vec<ElementId>,
    /// „FU-001“, „AW-001, AW-002, AW-003 und 2 weitere“.
    pub name: String,
}

impl Fokus {
    pub fn neu(m: &Model, auswahl: &[ElementId]) -> Fokus {
        let mut platten = Vec::new();
        for &e in auswahl {
            for g in sk_model::qto::gruendung_von(m, e) {
                let platte = m
                    .element(g)
                    .is_some_and(|x| matches!(x.kind, ElementKind::GroundSlab(_)));
                if platte && !platten.contains(&g) {
                    platten.push(g);
                }
            }
        }
        Fokus {
            auswahl: auswahl.to_vec(),
            platten,
            name: namen(m, auswahl),
        }
    }

    /// Die Ansatzzeile hängt an der Auswahl: am gewählten Bauteil selbst
    /// oder, als Automatikmenge der Gründung, an deren Sohlplatte.
    pub fn trifft(&self, a: &Ansatz) -> bool {
        self.auswahl.contains(&a.element)
            || (a.formel.is_some() && self.platten.contains(&a.element))
    }

    /// Zeile ohne Bauleistung eines gewählten Bauteils.
    pub fn trifft_ohne(&self, o: &OhneZeile) -> bool {
        self.auswahl.contains(&o.element)
    }

    /// Text der Leiste: „Verknüpft mit FU-001 · 6 Positionen“ bzw. „Mit
    /// FU-001 ist hier keine Position verknüpft.“
    pub fn text(&self, positionen: usize, zusatz: &str) -> String {
        if positionen == 0 {
            return format!("Mit {} ist hier keine Position verknüpft.", self.name);
        }
        let n = if positionen == 1 {
            "1 Position".to_string()
        } else {
            format!("{positionen} Positionen")
        };
        if zusatz.is_empty() {
            format!("Verknüpft mit {} · {n}", self.name)
        } else {
            format!("Verknüpft mit {} · {n} · {zusatz}", self.name)
        }
    }
}

/// Bauteile, an denen eine Ansatzzeile in der Geometrie hängt: das
/// Bauteil, bei einer Automatikmenge der Gründung die ganze Gründung (ein
/// Klick auf „Baugrube ausheben“ zeigt Platte und Frostschürze).
pub fn bauteile(m: &Model, a: &Ansatz) -> Vec<ElementId> {
    if a.formel.is_some() {
        let g = sk_model::qto::gruendung_von(m, a.element);
        if !g.is_empty() {
            return g;
        }
    }
    vec![a.element]
}

/// Bauteilnummern der Auswahl, die ersten drei und „und n weitere“.
fn namen(m: &Model, auswahl: &[ElementId]) -> String {
    let mut v: Vec<String> = Vec::new();
    for e in auswahl.iter().filter_map(|id| m.element(*id)) {
        let n = if e.number.is_empty() {
            sk_model::kinds::spec(e.category).name.to_string()
        } else {
            e.number.clone()
        };
        if !v.contains(&n) {
            v.push(n);
        }
    }
    match v.len() {
        0 => "der Auswahl".into(),
        n if n <= NAMEN => v.join(", "),
        n => format!("{} und {} weitere", v[..NAMEN].join(", "), n - NAMEN),
    }
}

/// Pille der Leiste (px) in der Zeile ab `y` über die Breite `w`.
pub fn pille(x0: f32, y: f32, w: f32, s: f32) -> Rect {
    (x0, y, w, PILLE_H * s)
}

/// „Alle Positionen“ rechts in der Pille (px).
pub fn verweis_rect(fonts: &Fonts, pille: Rect, s: f32) -> Rect {
    let px = 11.0 * s;
    let tw = fonts
        .bold
        .as_ref()
        .or(fonts.regular.as_ref())
        .map_or(100.0 * s, |f| f.width(ALLE, px));
    let (x, y, w, h) = pille;
    (x + w - tw - 12.0 * s, y, tw, h)
}

/// Leiste zeichnen: Band in der Farbe der Auswahl mit Akzentstrich, links
/// der Text, rechts „Alle Positionen“ (`hot`: unter der Maus).
pub fn paint(c: &mut Canvas, t: &Theme, fonts: &Fonts, pille: Rect, text: &str, hot: bool, s: f32) {
    let u = &t.ui;
    let (x, y, w, h) = pille;
    let mut p = Path::new();
    p.rounded_rect(x, y, w, h, t.size.corner_radius * s);
    c.fill(&p, u.sheet_select);
    c.fill_rect(x, y + 4.0 * s, 3.0 * s, h - 8.0 * s, u.accent);
    let px = 11.0 * s;
    let (vx, ..) = verweis_rect(fonts, pille, s);
    if let Some(f) = fonts.regular.as_ref() {
        let room = (vx - x - 28.0 * s).max(0.0);
        let text = sk_ui::widgets::ellipsize(Some(f), text, px, room);
        f.draw(
            c,
            &text,
            px,
            x + 12.0 * s,
            y + (h + f.cap_height(px)) * 0.5,
            u.sheet_text,
        );
    }
    if let Some(f) = fonts.bold.as_ref().or(fonts.regular.as_ref()) {
        let col = crate::cards::verweis(u, hot);
        f.draw(c, ALLE, px, vx, y + (h + f.cap_height(px)) * 0.5, col);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_der_leiste() {
        let f = Fokus {
            auswahl: Vec::new(),
            platten: Vec::new(),
            name: "FU-001".into(),
        };
        assert_eq!(f.text(1, ""), "Verknüpft mit FU-001 · 1 Position");
        assert_eq!(
            f.text(6, "3.456,78 €"),
            "Verknüpft mit FU-001 · 6 Positionen · 3.456,78 €"
        );
        assert_eq!(
            f.text(0, "x"),
            "Mit FU-001 ist hier keine Position verknüpft."
        );
    }
}
