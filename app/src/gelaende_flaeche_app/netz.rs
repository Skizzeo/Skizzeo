//! Hilfsnetz in 3D (Jörns §4.3a in gelaende/analyse-interaktives-gelaende.md):
//! In 3D ist das Gelände glatt schattiert, man sieht nicht, wo die
//! Höhenpunkte sitzen. Bei offener Leiste „Gelände“ liegen darum die Kanten
//! des Dreiecksnetzes fein auf dem Rasen, die vollen Meter kräftig
//! ([`crate::gelaende_netz::koerper_mit_netz`]), und jeder Höhenpunkt steht
//! als Nadel da: Messpunkte grau, eigene blau, gewählte orange. Der Knopf
//! „Netz“ oder die Taste N schaltet es aus; es bleibt aus, bis man es
//! wieder einschaltet.

use crate::freischaltung::Modul;
use crate::ui::{GelaendeKnopf, ViewKind};
use crate::App;
use sk_platform::{Key, Modifiers};
use sk_render::Helper;
use sk_ui::theme::Theme;

/// Länge der Nadel über dem Punkt (mm).
const NADEL: f32 = 600.0;
/// Größe des Punkts am Fuß der Nadel (Bildpunkte bei 96 dpi).
const PUNKT_PX: f32 = 6.0;
/// Bis so viele Punkte steht jeder als Nadel da, darüber nur die eigenen
/// und die gewählten (wie die Koten im Grundriss).
const NADELN: usize = 2_000;
/// Messpunkt des Vermessers und eigener Punkt.
const GRAU: [f32; 4] = [0.45, 0.45, 0.45, 1.0];
const BLAU: [f32; 4] = [0.18, 0.42, 0.85, 1.0];

impl App {
    /// Ist das Hilfsnetz an? Bei offener Leiste mit dem Modul, solange es
    /// nicht ausgeschaltet ist und das Gelände zu sehen ist.
    pub(crate) fn netz_an(&self) -> bool {
        let z = &self.platte.leiste;
        z.offen && !z.netz_aus && self.ui.frei.frei(Modul::Gelaende) && !self.scene.terrain_hidden()
    }

    /// Taste N schaltet das Netz wie der Knopf, bei offener Leiste mit dem
    /// Modul und einem Gelände mit Punkten.
    pub(crate) fn netz_taste(&mut self, key: Key, mods: Modifiers) -> bool {
        let hat_punkte = self
            .scene
            .model()
            .terrain()
            .is_some_and(|t| !t.points.is_empty());
        if key != Key::Char('N')
            || mods.ctrl
            || mods.alt
            || mods.shift
            || !self.platte.leiste.offen
            || !self.ui.frei.frei(Modul::Gelaende)
            || !hat_punkte
        {
            return false;
        }
        self.gelaende_klick(GelaendeKnopf::Netz);
        true
    }

    /// Die Höhenpunkte als Nadeln in 3D, wenn das Netz an ist; im Haus
    /// keine.
    pub(crate) fn netz_marken(&self, scale: f32, theme: &Theme) -> Vec<Helper> {
        if self.ui.view != ViewKind::Persp || !self.netz_an() {
            return Vec::new();
        }
        let spots = self.spots();
        let haeuser = self.scene.model().terrain_patches(0.0);
        let im_haus = |p: [f64; 3]| {
            haeuser
                .iter()
                .any(|(b, _)| (b[0]..=b[2]).contains(&p[0]) && (b[1]..=b[3]).contains(&p[1]))
        };
        let gewaehlt = &self.platte.punkte.gewaehlt;
        let alle = spots.len() <= NADELN;
        let mut out = Vec::new();
        for (nr, s) in spots.iter() {
            let wahl = gewaehlt.contains(nr);
            if !(alle || s.hand || wahl) || im_haus(s.p) {
                continue;
            }
            let farbe = if wahl {
                theme.interact.select
            } else if s.hand {
                BLAU
            } else {
                GRAU
            };
            let a = s.p.map(|v| v as f32);
            let b = [a[0], a[1], a[2] + NADEL];
            let strich = |a: [f32; 3], b: [f32; 3], px: f32, round: bool| Helper {
                a,
                b,
                color: farbe,
                width: px * scale,
                dash: 0.0,
                pattern: sk_render::SOLID,
                occlude: true,
                round,
            };
            out.push(strich(a, b, 1.5, false));
            out.push(strich(a, a, PUNKT_PX, true));
        }
        out
    }
}
