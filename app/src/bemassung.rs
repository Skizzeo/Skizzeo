//! Bemaßung im Grundriss und in 3D (Konzept `bemassung/konzept.md`): die
//! Außenmaßketten aus [`sk_plan`], flach auf der Geschossebene. Im
//! Grundriss die Ebene des aktiven Geschosses, in 3D die des gewählten
//! Bauteils (sonst die des aktiven), in jedem Gebäude, das auf ihr ein
//! Geschoss hat. Linien, Hilfslinien, Kreise und Schrägstriche sind
//! Hilfslinien mit der Strichstärke des Stils, Schrift, Punkte und Pfeile
//! sind Tinte ([`sk_render::Ink`]).
//!
//! Gezeigt wird nur, was das Zahnrad der Ansicht erlaubt ([`Model::mass_sicht`]);
//! „Maßbezugspunkte zeigen“ setzt an jeden Bezugspunkt einen Punkt mit
//! gestrichelter Linie zur Kette, in der Spurlinienfarbe.

use crate::scene::Scene;
use crate::ui::ViewKind;
use sk_model::bemassung::Stil;
use sk_model::{ElementId, LevelKind, Model, StoreyId};
use sk_paint::font::Font;
use sk_plan::aussen::{aussenmasse, Aussenmasse};
use sk_plan::bild::{bild, Bild, Strichart};
use sk_plan::schrift::Schrift;
use sk_render::{Helper, Ink, SOLID};
use sk_ui::theme::Theme;
use std::collections::HashMap;
use std::rc::Rc;

#[cfg(test)]
#[path = "bemassung_istbilder.rs"]
mod istbilder;

/// Wählbare Schriften im Reiter „Bemaßung“ und ihre Dateien im
/// Schriftenordner (ohne Datei: Ausweichschrift Arial, dann die Schrift der
/// Oberfläche).
pub const SCHRIFTEN: [(&str, &[&str]); 4] = [
    ("Segoe UI", &["segoeui.ttf"]),
    ("Arial", &["arial.ttf"]),
    ("Calibri", &["calibri.ttf"]),
    ("ISOCPEUR", &["isocpeur.ttf", "ISOCPEUR.TTF"]),
];

/// Punkt am Bezugspunkt (dip) und Strichlänge der Bezugslinie (dip).
const BEZUG_PUNKT: f32 = 6.0;
const BEZUG_STRICH: f32 = 4.0;

/// Gerechnete Ketten der Geschosse einer Ebene, je mit ihrem Bild.
pub struct Stand {
    pub geschosse: Vec<(Aussenmasse, Bild)>,
}

/// Bemaßung der App: gemerkte Schriften und das zuletzt gerechnete Bild.
#[derive(Default)]
pub struct Massketten {
    schriften: HashMap<String, Option<Rc<Schrift>>>,
    /// (Revision des Modells, Geschosse, Stil) und Stand dazu; der Stil
    /// zählt mit, weil „Abbrechen“ im Einstellungsfenster die Revision
    /// zurückstellt.
    stand: Option<((u64, Vec<StoreyId>, Stil), Rc<Stand>)>,
}

/// Ansicht mit Bemaßung (Stufe 1: Grundriss und 3D).
pub fn ansicht(v: ViewKind) -> bool {
    matches!(v, ViewKind::Plan | ViewKind::Persp)
}

/// Geschosse, deren Ketten die Ansicht `v` zeigt: die Ebene des aktiven
/// Geschosses, in 3D die des ersten gewählten Bauteils, in jedem Gebäude.
/// Die Gründung hat keine.
pub fn geschosse(scene: &Scene, v: ViewKind, gewaehlt: &[ElementId]) -> Vec<StoreyId> {
    let m = scene.model();
    let normal = |st: StoreyId| {
        m.storey(st)
            .is_some_and(|s| s.kind != LevelKind::Foundation)
    };
    let aus_wahl = (v == ViewKind::Persp)
        .then(|| {
            gewaehlt
                .iter()
                .filter_map(|&id| m.element(id).map(|e| e.storey))
                .find(|&st| normal(st))
        })
        .flatten();
    let Some(z) = aus_wahl
        .or(Some(scene.active_storey()))
        .filter(|&st| normal(st))
        .and_then(|st| m.storey(st))
        .map(|s| s.elevation)
    else {
        return Vec::new();
    };
    m.storeys()
        .iter()
        .filter(|(id, s)| normal(*id) && (s.elevation - z).abs() < 1.0)
        .map(|(id, _)| id)
        .collect()
}

/// Schrift zum Namen `name` aus dem Schriftenordner des Systems; fehlt sie,
/// Arial, dann die Schrift der Oberfläche (`ui`).
pub fn schrift_laden(name: &str, ui: Option<&Font>) -> Option<Schrift> {
    let eigene: Vec<String> = SCHRIFTEN
        .iter()
        .find(|(n, _)| n.eq_ignore_ascii_case(name))
        .map(|(_, f)| f.iter().map(|s| s.to_string()).collect())
        .unwrap_or_else(|| vec![format!("{}.ttf", name.replace(' ', "").to_lowercase())]);
    let mut dateien: Vec<&str> = eigene.iter().map(String::as_str).collect();
    dateien.push("arial.ttf");
    Font::system(&dateien)
        .or_else(|| ui.and_then(|f| Font::parse(f.data().to_vec())))
        .map(Schrift::new)
}

impl Massketten {
    /// Schrift des Stils, einmal geladen.
    fn schrift(&mut self, name: &str, ui: Option<&Font>) -> Option<Rc<Schrift>> {
        self.schriften
            .entry(name.to_string())
            .or_insert_with(|| schrift_laden(name, ui).map(Rc::new))
            .clone()
    }

    /// Ketten und Bilder der Geschosse `st`, gerechnet nur bei neuer
    /// Revision.
    pub fn stand(&mut self, m: &Model, st: Vec<StoreyId>, ui: Option<&Font>) -> Option<Rc<Stand>> {
        let key = (m.revision(), st, m.massstil().clone());
        if let Some((k, s)) = &self.stand {
            if *k == key {
                return Some(s.clone());
            }
        }
        let schrift = self.schrift(&key.2.schrift, ui)?;
        let geschosse = key
            .1
            .iter()
            .filter_map(|&st| aussenmasse(m, st))
            .map(|a| {
                let b = bild(&a, m.massstil(), &schrift);
                (a, b)
            })
            .collect();
        let s = Rc::new(Stand { geschosse });
        self.stand = Some((key, s.clone()));
        Some(s)
    }

    /// Hilfslinien und Tinte der Bemaßung für die Ansicht `v` (leer, wenn
    /// das Zahnrad sie ausblendet oder es nichts zu bemaßen gibt).
    pub fn zeichnen(
        &mut self,
        scene: &Scene,
        v: ViewKind,
        gewaehlt: &[ElementId],
        ui: Option<&Font>,
        scale: f32,
        theme: &Theme,
    ) -> (Vec<Helper>, Vec<Ink>) {
        let m = scene.model();
        let (zeigen, bezug) = scene.mass_sicht(v);
        if !ansicht(v) || !zeigen {
            return (Vec::new(), Vec::new());
        }
        let st = geschosse(scene, v, gewaehlt);
        let Some(stand) = self.stand(m, st, ui) else {
            return (Vec::new(), Vec::new());
        };
        let occlude = v == ViewKind::Persp;
        let (mut out, mut ink) = (Vec::new(), Vec::new());
        for (_, b) in &stand.geschosse {
            out.extend(linien(b, m.massstil(), scale, theme, occlude));
            ink.extend(tinte(b, m.massstil(), occlude));
            if bezug {
                out.extend(bezugspunkte(b, scale, theme, occlude));
            }
        }
        (out, ink)
    }
}

/// Farbe des Stils als RGBA.
pub fn farbe(s: &Stil) -> [f32; 4] {
    let [r, g, b] = s.farbe;
    [
        f32::from(r) / 255.0,
        f32::from(g) / 255.0,
        f32::from(b) / 255.0,
        1.0,
    ]
}

/// Striche des Bildes als Hilfslinien: Strichstärke aus dem Stil (mm auf
/// dem Papier) wie bei allen Stiften.
pub fn linien(b: &Bild, s: &Stil, scale: f32, theme: &Theme, occlude: bool) -> Vec<Helper> {
    let px = theme.px_per_mm * scale;
    let color = farbe(s);
    b.striche
        .iter()
        .map(|st| {
            let mm = match st.art {
                Strichart::Masslinie => s.linie,
                Strichart::Hilfslinie => s.hilfslinie,
                Strichart::Symbol => s.symbol_strich,
            };
            Helper {
                a: st.a.to_f32(),
                b: st.b.to_f32(),
                color,
                width: (mm as f32 * px).max(1.0),
                dash: 0.0,
                pattern: SOLID,
                occlude,
                round: st.art == Strichart::Symbol,
            }
        })
        .collect()
}

/// Gefüllte Dreiecke des Bildes in der Farbe des Stils.
pub fn tinte(b: &Bild, s: &Stil, occlude: bool) -> Vec<Ink> {
    let color = farbe(s);
    b.tinte
        .iter()
        .map(|d| Ink {
            p: d.map(|p| p.to_f32()),
            color,
            occlude,
        })
        .collect()
}

/// Maßbezugspunkte: Punkt am Bauteil, gestrichelte Linie zur Kette.
pub fn bezugspunkte(b: &Bild, scale: f32, theme: &Theme, occlude: bool) -> Vec<Helper> {
    let color = theme.interact.track;
    let mut out = Vec::new();
    for r in &b.bezuege {
        out.push(Helper {
            a: r.am_bauteil.to_f32(),
            b: r.an_kette.to_f32(),
            color,
            width: scale,
            dash: BEZUG_STRICH * scale,
            pattern: SOLID,
            occlude,
            round: false,
        });
        out.push(Helper {
            a: r.am_bauteil.to_f32(),
            b: r.am_bauteil.to_f32(),
            color,
            width: BEZUG_PUNKT * scale,
            dash: 0.0,
            pattern: SOLID,
            occlude: false,
            round: true,
        });
    }
    out
}
