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
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

/// Länge der Nadel über dem Punkt (mm).
const NADEL: f32 = 600.0;
/// Größe des Punkts am Fuß der Nadel (Bildpunkte bei 96 dpi).
const PUNKT_PX: f32 = 6.0;
/// Bis so viele Punkte steht jeder als Nadel da, darüber nur die eigenen
/// und die gewählten (wie die Koten im Grundriss).
const NADELN: usize = 2_000;
/// Messpunkt des Vermessers und eigener Punkt.
pub(super) const GRAU: [f32; 4] = [0.45, 0.45, 0.45, 1.0];
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

/// Die Häuser als Quader (Grundriss aus
/// [`sk_model::Model::terrain_patches`], Höhe aus dem Quader der Szene),
/// einmal je Stand des Modells (Review #237 B-237a), mit einem Raster für
/// die Frage, ob ein Punkt unter einem Haus liegt.
#[derive(Debug, Default)]
pub(crate) struct Haeuser {
    pub(crate) stand: u64,
    pub(crate) quader: Vec<(Vec3, Vec3)>,
    zelle: f64,
    raster: HashMap<(i64, i64), Vec<u32>>,
}

impl Haeuser {
    pub(crate) fn neu(stand: u64, quader: Vec<(Vec3, Vec3)>) -> Haeuser {
        // Zelle etwa ein Haus groß, damit jedes in wenigen Zellen steht
        let n = quader.len().max(1) as f64;
        let flaeche: f64 = quader
            .iter()
            .map(|(lo, hi)| (hi.x - lo.x) * (hi.y - lo.y))
            .sum();
        let zelle = (flaeche / n).sqrt().max(1000.0);
        let mut raster: HashMap<(i64, i64), Vec<u32>> = HashMap::new();
        let z = |v: f64| (v / zelle).floor() as i64;
        for (k, (lo, hi)) in quader.iter().enumerate() {
            for i in z(lo.x)..=z(hi.x) {
                for j in z(lo.y)..=z(hi.y) {
                    raster.entry((i, j)).or_default().push(k as u32);
                }
            }
        }
        Haeuser {
            stand,
            quader,
            zelle,
            raster,
        }
    }

    /// Liegt der Punkt im Grundriss eines Hauses?
    pub(crate) fn unter(&self, p: [f64; 3]) -> bool {
        let k = (
            (p[0] / self.zelle).floor() as i64,
            (p[1] / self.zelle).floor() as i64,
        );
        self.raster.get(&k).is_some_and(|v| {
            v.iter().any(|&k| {
                let (lo, hi) = self.quader[k as usize];
                (lo.x..=hi.x).contains(&p[0]) && (lo.y..=hi.y).contains(&p[1])
            })
        })
    }
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

    /// Die Punkte, die in 3D als Nadel stehen: keine im Haus, bei sehr
    /// vielen nur die eigenen und die gewählten. Was hinter einem Haus
    /// steht, zeichnet die Verdeckung blass; greifen lässt es sich nicht
    /// ([`App::dahinter`], Bedienbarkeit zu #235).
    pub(super) fn nadeln<'a>(
        &self,
        spots: &'a [(String, TerrainSpot)],
    ) -> Vec<&'a (String, TerrainSpot)> {
        let haeuser = self.haeuser();
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
                (alle || s.hand || gewaehlt.contains(nr.as_str())) && !haeuser.unter(s.p)
            })
            .collect()
    }

    /// Steht der Punkt von der Kamera aus hinter einem Haus? Nur für die
    /// Punkte unter der Maus gefragt, nicht je Bild (Review #237 B-237a).
    pub(super) fn dahinter(&self, p: [f64; 3]) -> bool {
        let (vw, vh, _) = self.view_size();
        let p = vec3(p[0], p[1], p[2]);
        let Some(q) = self.cam.project(p, vw, vh) else {
            return true;
        };
        let (o, d) = self.cam.ray(q.0, q.1, vw, vh);
        let ende = o + d * ((p - o).dot(d) / d.dot(d));
        self.haeuser()
            .quader
            .iter()
            .any(|&(lo, hi)| verdeckt(o, ende, lo, hi))
    }

    /// Die Häuser zum Stand des Modells: gemerkt ([`App::haeuser_merken`]),
    /// sonst gerechnet.
    pub(super) fn haeuser(&self) -> Arc<Haeuser> {
        let stand = self.scene.model().revision();
        match &self.platte.punkte.haeuser {
            Some(h) if h.stand == stand => h.clone(),
            _ => Arc::new(self.haeuser_rechnen()),
        }
    }

    /// Die Häuser zum jetzigen Stand merken.
    pub(super) fn haeuser_merken(&mut self) {
        let stand = self.scene.model().revision();
        if self
            .platte
            .punkte
            .haeuser
            .as_ref()
            .is_none_or(|h| h.stand != stand)
        {
            self.platte.punkte.haeuser = Some(Arc::new(self.haeuser_rechnen()));
        }
    }

    fn haeuser_rechnen(&self) -> Haeuser {
        let stand = self.scene.model().revision();
        let Some((lo, hi)) = self.scene.bounds() else {
            return Haeuser::neu(stand, Vec::new());
        };
        let quader = self
            .scene
            .model()
            .terrain_patches(0.0)
            .iter()
            .map(|(b, _)| (vec3(b[0], b[1], lo.z), vec3(b[2], b[3], hi.z)))
            .collect();
        Haeuser::neu(stand, quader)
    }

    /// Punkte und Häuser zum Stand des Modells merken, solange die Nadeln
    /// stehen; einmal je Bild vor dem Zeichnen (Review #237 B-237a).
    pub(crate) fn nadeln_merken(&mut self) {
        if self.ui.view == ViewKind::Persp && self.netz_an() {
            self.spots_merken();
            self.haeuser_merken();
        }
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

    /// Unter welchem Haus ein Punkt liegt, sagt das Raster wie der Gang
    /// über alle Häuser (Review #237 B-237a).
    #[test]
    fn unter_dem_haus_im_raster() {
        let mut quader = Vec::new();
        for i in 0..40 {
            let (x, y) = ((i % 8) as f64 * 15000.0, (i / 8) as f64 * 15000.0);
            quader.push((vec3(x, y, 0.0), vec3(x + 10000.0, y + 8000.0, 9000.0)));
        }
        let h = Haeuser::neu(1, quader.clone());
        for k in 0..2000 {
            let p = [
                (k * 7919 % 130_000) as f64 - 5000.0,
                (k * 104_729 % 85_000) as f64 - 5000.0,
                0.0,
            ];
            let soll = quader
                .iter()
                .any(|(lo, hi)| (lo.x..=hi.x).contains(&p[0]) && (lo.y..=hi.y).contains(&p[1]));
            assert_eq!(h.unter(p), soll, "{p:?}");
        }
        assert!(h.unter([10000.0, 8000.0, 0.0]), "auf der Ecke");
        assert!(!Haeuser::default().unter([0.0, 0.0, 0.0]));
    }
}
