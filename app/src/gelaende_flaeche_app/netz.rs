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
use sk_math::{vec3, Vec3};
use sk_model::TerrainSpot;
use sk_platform::{Key, Modifiers};
use sk_render::Helper;
use sk_ui::theme::Theme;
use std::collections::HashSet;

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

/// Schneidet die Strecke `a`–`b` den Quader `lo`–`hi`, bevor sie `b`
/// erreicht? (Strahlverfahren nach Platten.)
pub(crate) fn verdeckt(a: Vec3, b: Vec3, lo: Vec3, hi: Vec3) -> bool {
    let d = b - a;
    let (mut t0, mut t1): (f64, f64) = (0.0, 1.0 - 1e-6);
    for (o, d, lo, hi) in [
        (a.x, d.x, lo.x, hi.x),
        (a.y, d.y, lo.y, hi.y),
        (a.z, d.z, lo.z, hi.z),
    ] {
        if d.abs() < 1e-12 {
            if o < lo || o > hi {
                return false;
            }
            continue;
        }
        let (u, v) = ((lo - o) / d, (hi - o) / d);
        t0 = t0.max(u.min(v));
        t1 = t1.min(u.max(v));
        if t0 > t1 {
            return false;
        }
    }
    true
}

/// Ein Höhenpunkt in 3D: Punkt und, mit `stab`, die Nadel darüber; grau
/// der Messpunkt, blau der eigene, sonst in der Farbe `wahl`.
pub(crate) fn nadel(
    out: &mut Vec<Helper>,
    s: &TerrainSpot,
    wahl: Option<[f32; 4]>,
    stab: bool,
    scale: f32,
) {
    let farbe = wahl.unwrap_or(if s.hand { BLAU } else { GRAU });
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
    if stab {
        out.push(strich(a, b, 1.5, false));
    }
    out.push(strich(a, a, PUNKT_PX, true));
}

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

    /// Die Punkte, die in 3D als Nadel stehen und greifbar sind: keine im
    /// Haus und keine hinter einem Haus (Bedienbarkeit zu #235), bei sehr
    /// vielen nur die eigenen und die gewählten.
    pub(super) fn nadeln<'a>(
        &self,
        spots: &'a [(String, TerrainSpot)],
    ) -> Vec<&'a (String, TerrainSpot)> {
        let quader = self.haus_quader();
        let im_haus = |p: [f64; 3]| {
            quader
                .iter()
                .any(|(lo, hi)| (lo.x..=hi.x).contains(&p[0]) && (lo.y..=hi.y).contains(&p[1]))
        };
        let (vw, vh, _) = self.view_size();
        let dahinter = |p: [f64; 3]| {
            let p = vec3(p[0], p[1], p[2]);
            let Some(q) = self.cam.project(p, vw, vh) else {
                return true;
            };
            let (o, d) = self.cam.ray(q.0, q.1, vw, vh);
            let ende = o + d * ((p - o).dot(d) / d.dot(d));
            quader.iter().any(|&(lo, hi)| verdeckt(o, ende, lo, hi))
        };
        let gewaehlt: HashSet<&str> = self
            .platte
            .punkte
            .gewaehlt
            .iter()
            .map(|s| s.as_str())
            .collect();
        let alle = spots.len() <= NADELN;
        spots
            .iter()
            .filter(|(nr, s)| {
                (alle || s.hand || gewaehlt.contains(nr.as_str()))
                    && !im_haus(s.p)
                    && !dahinter(s.p)
            })
            .collect()
    }

    /// Die Häuser als Quader: Grundriss aus [`sk_model::Model::terrain_patches`],
    /// Höhe aus dem Quader der Szene.
    fn haus_quader(&self) -> Vec<(Vec3, Vec3)> {
        let Some((lo, hi)) = self.scene.bounds() else {
            return Vec::new();
        };
        self.scene
            .model()
            .terrain_patches(0.0)
            .iter()
            .map(|(b, _)| (vec3(b[0], b[1], lo.z), vec3(b[2], b[3], hi.z)))
            .collect()
    }

    /// Die Höhenpunkte als Nadeln in 3D, wenn das Netz an ist.
    pub(crate) fn netz_marken(&self, scale: f32, theme: &Theme) -> Vec<Helper> {
        if self.ui.view != ViewKind::Persp || !self.netz_an() {
            return Vec::new();
        }
        let spots = self.spots();
        let gewaehlt = &self.platte.punkte.gewaehlt;
        let mut out = Vec::new();
        for (nr, s) in self.nadeln(&spots) {
            let wahl = gewaehlt.contains(nr);
            // am ersten gewählten steht statt der Nadel der Höhenpfeil
            let erster = gewaehlt.first() == Some(nr);
            nadel(
                &mut out,
                s,
                wahl.then_some(theme.interact.select),
                !erster,
                scale,
            );
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Ein Haus 10 × 10 × 8 m: der Punkt dahinter ist verdeckt, der davor
    /// und der daneben nicht.
    #[test]
    fn hinter_dem_haus() {
        let (lo, hi) = (vec3(0.0, 0.0, -500.0), vec3(10000.0, 10000.0, 8000.0));
        let auge = vec3(-20000.0, 5000.0, 6000.0);
        assert!(verdeckt(auge, vec3(15000.0, 5000.0, 0.0), lo, hi));
        assert!(!verdeckt(auge, vec3(-2000.0, 5000.0, 0.0), lo, hi));
        assert!(!verdeckt(auge, vec3(15000.0, 14000.0, 0.0), lo, hi));
        // von hoch oben über das Haus hinweg
        let oben = vec3(-20000.0, 5000.0, 60000.0);
        assert!(!verdeckt(oben, vec3(25000.0, 5000.0, 0.0), lo, hi));
        assert!(
            verdeckt(oben, vec3(14000.0, 5000.0, 0.0), lo, hi),
            "über die Dachkante"
        );
    }
}
