//! Geländeflächen (Gelände G3b, plan-gelaendemodul.md „G3 Feinplan“): ein
//! ebener Baustein des Planungsgeländes im Modell (mm, wie Wände), nicht in
//! Landeswerten. Er gehört zum Entwurf und bleibt beim Verschieben des
//! Geländes am Haus (E8). Umriss, Höhe (eben, Gefälle oder bis drei Ecken),
//! je Kante eine Böschung 1:n bis ans Urgelände oder senkrecht, Belag Grün
//! oder befestigt. Mengen daraus rechnet `qto_earth` über
//! [`sk_gelaende::planung`].
//!
//! Dazu die Werte des Geländebaus am Projekt ([`SiteSoil`]): Dicke der
//! Andeckung und Faktor Eigenboden fest → eingebaut (VOB-Prüfung G3, B4
//! und B5).

use super::{Model, SurveyFrame};
use sk_gelaende::planung::{oberflaeche, schraffen, Baustein, Herkunft, Kante, Teil};
use sk_math::{vec3, Vec3};
use std::sync::{Arc, Mutex};

/// Abstand der Böschungsschraffen im Grundriss (mm).
pub const PATCH_HATCH: f64 = 1000.0;

/// Vorgabe der Böschung 1:n am Rand einer Geländefläche.
pub const PATCH_SLOPE: f64 = 1.5;
/// Bereich von n; 0 heißt senkrecht (Stützmauer ab G4).
pub const PATCH_SLOPE_RANGE: (f64, f64) = (0.5, 20.0);
/// Größtes Gefälle einer Fläche in Prozent.
pub const PATCH_FALL_MAX: f64 = 100.0;
/// Größter Betrag einer Eckenlage im Modell (mm, 100 km).
pub const PATCH_XY_MAX: f64 = 1e8;
/// Größter Betrag einer Höhe über ±0,00 an jeder Ecke (mm).
pub const PATCH_Z_MAX: f64 = 100_000.0;
/// Bereich einer Höhe in m ü. NHN an jeder Ecke.
pub const PATCH_NHN: (f64, f64) = (-500.0, 9_000.0);
/// Größter Betrag der Richtung des Gefälles (Grad).
pub const PATCH_DIR_MAX: f64 = 360.0;

/// Höhe einer Geländefläche, so wie sie eingegeben wurde: in mm über ±0,00
/// oder, mit [`TerrainPatch::abs`], in m ü. NHN.
#[derive(Clone, Debug, PartialEq)]
pub enum PatchHeight {
    /// Eine Höhe überall.
    Flat(f64),
    /// Höhe `z` an der ersten Ecke, fällt mit `fall` Prozent in Richtung
    /// `dir` (Grad gegen den Uhrzeigersinn ab der Modell-x-Achse).
    Fall { z: f64, dir: f64, fall: f64 },
    /// Höhen an bis zu drei Ecken (Nummer im Umriss, Höhe): eine Ecke eben,
    /// zwei geneigt längs ihrer Verbindung und quer dazu waagerecht, drei
    /// die Ebene durch sie.
    Corners(Vec<(usize, f64)>),
}

/// Eine Geländefläche.
#[derive(Clone, Debug, PartialEq)]
pub struct TerrainPatch {
    /// Umriss im Modell (mm), Umlaufsinn beliebig.
    pub outline: Vec<[f64; 2]>,
    pub height: PatchHeight,
    /// Höhen in m ü. NHN statt in mm über ±0,00.
    pub abs: bool,
    /// n der Böschung 1:n: leer heißt [`PATCH_SLOPE`] ringsum, ein Wert
    /// ringsum, sonst je Kante (Kante i von Ecke i nach i + 1); 0 heißt
    /// senkrecht.
    pub slopes: Vec<f64>,
    /// Befestigte Fläche (Q = P, kein Andecken; Aufbau ab G4).
    pub paved: bool,
}

impl TerrainPatch {
    /// Eine ebene Grünfläche in Höhe `z` (mm über ±0,00), Böschung nach
    /// Vorgabe.
    pub fn flat(outline: Vec<[f64; 2]>, z: f64) -> TerrainPatch {
        TerrainPatch {
            outline,
            height: PatchHeight::Flat(z),
            abs: false,
            slopes: Vec::new(),
            paved: false,
        }
    }

    /// Lässt sich die Fläche bauen? Mindestens drei Ecken, Fläche größer
    /// null, Werte in ihren Bereichen ([`PATCH_XY_MAX`], [`PATCH_FALL_MAX`],
    /// [`PATCH_DIR_MAX`]); die Höhe an jeder Ecke bis [`PATCH_Z_MAX`] über
    /// oder unter ±0,00 bzw. in [`PATCH_NHN`] (Review #173 B2).
    pub fn valid(&self) -> bool {
        let n = self.outline.len();
        let ecken = n >= 3
            && self
                .outline
                .iter()
                .flatten()
                .all(|v| v.abs() <= PATCH_XY_MAX)
            && flaeche(&self.outline).abs() > 1.0
            // Schleife oder Rückläufer: Fläche, Gefälle und Mengen sinnlos (Geometrie #173)
            && sk_math::polygon::is_simple(
                &self
                    .outline
                    .iter()
                    .map(|q| sk_math::vec3(q[0], q[1], 0.0))
                    .collect::<Vec<_>>(),
            );
        let z_ok = |z: f64| {
            if self.abs {
                (PATCH_NHN.0..=PATCH_NHN.1).contains(&z)
            } else {
                z.abs() <= PATCH_Z_MAX
            }
        };
        let hoehe = match &self.height {
            PatchHeight::Flat(z) => z_ok(*z),
            PatchHeight::Fall { z, dir, fall } => {
                z_ok(*z) && dir.abs() <= PATCH_DIR_MAX && fall.abs() <= PATCH_FALL_MAX
            }
            PatchHeight::Corners(c) => {
                (1..=3).contains(&c.len())
                    && c.iter().all(|(i, z)| *i < n && z_ok(*z))
                    && (1..c.len()).all(|k| c[..k].iter().all(|e| e.0 != c[k].0))
            }
        };
        // auch an den übrigen Ecken, sonst trüge ein großes Gefälle weit
        let hoehe = hoehe && ecken && {
            let e = self.plane();
            self.outline
                .iter()
                .all(|p| z_ok(e[0] + e[1] * p[0] + e[2] * p[1]))
        };
        let (lo, hi) = PATCH_SLOPE_RANGE;
        let boeschung = matches!(self.slopes.len(), 0 | 1) || self.slopes.len() == n;
        let boeschung = boeschung
            && self
                .slopes
                .iter()
                .all(|s| *s == 0.0 || (*s >= lo && *s <= hi));
        ecken && hoehe && boeschung
    }

    /// n der Böschung an Kante `i`; `None` senkrecht.
    pub fn slope(&self, i: usize) -> Option<f64> {
        let n = match self.slopes.as_slice() {
            [] => PATCH_SLOPE,
            [s] => *s,
            v => v.get(i).copied().unwrap_or(PATCH_SLOPE),
        };
        (n > 0.0).then_some(n)
    }

    /// Ebene der Fläche `z = a + b·x + c·y` im Modell (x, y in mm, z in der
    /// Einheit der Höhe: mm bzw. mit `abs` m).
    pub fn plane(&self) -> [f64; 3] {
        // Steigung je mm Lage in der Einheit der Höhe
        let je_mm = if self.abs { 1e-3 } else { 1.0 };
        let p = |i: usize| self.outline.get(i).copied().unwrap_or([0.0, 0.0]);
        let durch = |z: f64, q: [f64; 2], b: f64, c: f64| [z - b * q[0] - c * q[1], b, c];
        match &self.height {
            PatchHeight::Flat(z) => [*z, 0.0, 0.0],
            PatchHeight::Fall { z, dir, fall } => {
                let (s, c) = dir.to_radians().sin_cos();
                let k = -fall / 100.0 * je_mm;
                durch(*z, p(0), k * c, k * s)
            }
            PatchHeight::Corners(e) => match e.as_slice() {
                [] => [0.0, 0.0, 0.0],
                [(_, z)] => [*z, 0.0, 0.0],
                [(i, z0), (j, z1)] => zwei(p(*i), *z0, p(*j), *z1),
                [(i, z0), (j, z1), (k, z2), ..] => {
                    let (a, b, c) = (p(*i), p(*j), p(*k));
                    let (u, v) = ([b[0] - a[0], b[1] - a[1]], [c[0] - a[0], c[1] - a[1]]);
                    let det = u[0] * v[1] - u[1] * v[0];
                    if det.abs() < 1e-6 * (u[0].hypot(u[1]) * v[0].hypot(v[1])).max(1.0) {
                        // auf einer Linie: wie zwei Ecken, die weitesten
                        let (q, zq) = if u[0].hypot(u[1]) >= v[0].hypot(v[1]) {
                            (b, *z1)
                        } else {
                            (c, *z2)
                        };
                        return zwei(a, *z0, q, zq);
                    }
                    let (du, dv) = (z1 - z0, z2 - z0);
                    let gb = (du * v[1] - dv * u[1]) / det;
                    let gc = (dv * u[0] - du * v[0]) / det;
                    durch(*z0, a, gb, gc)
                }
            },
        }
    }

    /// Höhe über ±0,00 (mm) an `(x, y)`; `h0` ist ±0,00 in m ü. NHN, nur
    /// mit `abs` gebraucht.
    pub fn z_at(&self, x: f64, y: f64, h0: f64) -> f64 {
        let [a, b, c] = self.plane();
        let z = a + b * x + c * y;
        if self.abs {
            (z - h0) * 1000.0
        } else {
            z
        }
    }

    /// Die Fläche als Baustein im Netz des Aufmaßes: Lage in m relativ zu
    /// `bezug` (Landeswert des Netznullpunkts), Höhen in m ü. NHN; `h0` ist
    /// ±0,00 in m ü. NHN. `None`, wenn sie sich nicht bauen lässt.
    pub fn baustein(&self, frame: &SurveyFrame, h0: f64, bezug: (f64, f64)) -> Option<Baustein> {
        if !self.valid() {
            return None;
        }
        let im_netz = |p: [f64; 2]| {
            let (e, n) = frame.to_survey(p[0], p[1]);
            [e - bezug.0, n - bezug.1]
        };
        let hoehe = |p: [f64; 2]| h0 + self.z_at(p[0], p[1], h0) / 1000.0;
        // Ebene im Netz durch drei Punkte (die Lage ist längentreu)
        let o = self.outline[0];
        let [q0, q1, q2] = [o, [o[0] + 1000.0, o[1]], [o[0], o[1] + 1000.0]];
        let [m0, m1, m2] = [q0, q1, q2].map(im_netz);
        let [z0, z1, z2] = [q0, q1, q2].map(hoehe);
        let (u, v) = (
            [m1[0] - m0[0], m1[1] - m0[1]],
            [m2[0] - m0[0], m2[1] - m0[1]],
        );
        let det = u[0] * v[1] - u[1] * v[0];
        let gb = ((z1 - z0) * v[1] - (z2 - z0) * u[1]) / det;
        let gc = ((z2 - z0) * u[0] - (z1 - z0) * v[0]) / det;
        let ebene = [z0 - gb * m0[0] - gc * m0[1], gb, gc];
        let ring: Vec<[f64; 2]> = self.outline.iter().map(|&p| im_netz(p)).collect();
        let n = ring.len();
        let rand = (0..n)
            .map(|i| {
                let (a, b) = (ring[i], ring[(i + 1) % n]);
                let z = |p: [f64; 2]| ebene[0] + ebene[1] * p[0] + ebene[2] * p[1];
                Kante {
                    a: [a[0], a[1], z(a)],
                    b: [b[0], b[1], z(b)],
                    neigung: self.slope(i),
                }
            })
            .collect();
        Some(Baustein {
            teile: vec![Teil { ring, ebene }],
            rand,
        })
    }
}

/// Ebene durch zwei Punkte, quer zu ihrer Verbindung waagerecht.
fn zwei(a: [f64; 2], za: f64, b: [f64; 2], zb: f64) -> [f64; 3] {
    let d = [b[0] - a[0], b[1] - a[1]];
    let l2 = d[0] * d[0] + d[1] * d[1];
    if l2 <= 0.0 {
        return [za, 0.0, 0.0];
    }
    let k = (zb - za) / l2;
    let (gb, gc) = (k * d[0], k * d[1]);
    [za - gb * a[0] - gc * a[1], gb, gc]
}

/// Fläche mit Vorzeichen (gegen den Uhrzeigersinn positiv).
fn flaeche(r: &[[f64; 2]]) -> f64 {
    let n = r.len();
    (0..n)
        .map(|i| {
            let (a, b) = (r[i], r[(i + 1) % n]);
            a[0] * b[1] - a[1] * b[0]
        })
        .sum::<f64>()
        / 2.0
}

/// Werte des Geländebaus am Projekt (VOB-Prüfung G3): Dicke der Andeckung
/// (mm, senkrecht zur Fläche, B5) und Faktor Eigenboden fest → eingebaut
/// für die Massenbilanz (B4, nie in den Mengen der Positionen).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SiteSoil {
    pub spread: f64,
    pub reuse: f64,
}

impl Default for SiteSoil {
    fn default() -> SiteSoil {
        SiteSoil {
            spread: 200.0,
            reuse: 1.0,
        }
    }
}

impl SiteSoil {
    /// Schlüssel an `[sitesoil]`, Bezeichnung und erlaubter Bereich, in der
    /// Reihenfolge der Felder.
    pub const FELDER: [(&'static str, &'static str, f64, f64); 2] = [
        ("spread", "Oberboden andecken", 0.0, 1000.0),
        ("reuse", "Eigenboden fest zu eingebaut", 0.8, 1.2),
    ];

    pub fn werte(&self) -> [f64; 2] {
        [self.spread, self.reuse]
    }

    /// Aus Werten in der Reihenfolge von [`SiteSoil::FELDER`]; `None`, wenn
    /// einer außerhalb seines Bereichs liegt.
    pub fn aus_werten(w: [f64; 2]) -> Option<SiteSoil> {
        let ok = w
            .iter()
            .zip(SiteSoil::FELDER)
            .all(|(v, f)| v.is_finite() && *v >= f.2 && *v <= f.3);
        ok.then_some(SiteSoil {
            spread: w[0],
            reuse: w[1],
        })
    }
}

/// Das Planungsgelände für die Darstellung (G3c) im Modell (mm, z zu
/// ±0,00): die Dreiecke des Urgeländes, die es ersetzt, und die Dreiecke an
/// ihrer Stelle ([`sk_gelaende::planung::oberflaeche`]), dazu die
/// Böschungsschraffen und die Umrisse der Flächen auf ihrer Höhe.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PlannedTerrain {
    /// Nummern in [`Model::terrain_triangles`], aufsteigend.
    pub replaced: Vec<u32>,
    /// Gegen den Uhrzeigersinn, wie das Urgelände.
    pub triangles: Vec<[Vec3; 3]>,
    /// Je Dreieck: auf einer befestigten Fläche.
    pub paved: Vec<bool>,
    /// Böschungsschraffen vom Kopf zum Fuß, je Stück; `true` lang.
    pub hatch: Vec<([Vec3; 2], bool)>,
    /// Umriss jeder Fläche, die sich bauen lässt, auf ihrer Höhe.
    pub outlines: Vec<Vec<Vec3>>,
    /// Senkrechte Wände, wo die Höhe am Rand einer Fläche springt (Kante
    /// ohne Böschung); die Normale zeigt zur tieferen Seite.
    pub walls: Vec<[Vec3; 3]>,
}

/// Das zuletzt gerechnete Planungsgelände mit Netz, Lage und Flächen, aus
/// denen es folgt.
struct Geplant {
    netz: Arc<crate::model::TerrainMesh>,
    lage: [u64; 4],
    flaechen: Vec<TerrainPatch>,
    plan: PlannedTerrain,
}

/// Platz für das zuletzt gerechnete Planungsgelände, geteilt über die
/// Kopien des Modells (Review #183 a): Das Gelände rechnet ein eigener
/// Faden auf einer Kopie; ein Schritt, der Flächen, Aufmaß und ±0,00 lässt,
/// rechnet es nicht neu, auch wenn die Mengen es nicht gemerkt haben.
#[derive(Clone, Default)]
pub(crate) struct PlanMerker(Arc<Mutex<Option<Arc<Geplant>>>>);

impl std::fmt::Debug for PlanMerker {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Planmerker")
    }
}

impl PlanMerker {
    fn alt(&self) -> Option<Arc<Geplant>> {
        self.0.lock().ok().and_then(|g| g.clone())
    }

    fn setzen(&self, g: Option<Geplant>) {
        if let Ok(mut x) = self.0.lock() {
            *x = g.map(Arc::new);
        }
    }
}

impl Model {
    /// Das Planungsgelände der Geländeflächen über dem Urgelände; `None`
    /// ohne Fläche, Aufmaß, Lage oder Netz.
    pub fn planned_terrain(&self) -> Option<PlannedTerrain> {
        let merker = self.planmerker();
        let (Some(t), Some(f), Some(h0)) =
            (self.terrain(), self.survey_frame(), self.terrain_nhn())
        else {
            merker.setzen(None);
            return None;
        };
        if self.site_patches().is_empty() {
            merker.setzen(None);
            return None;
        }
        let mesh = t.mesh();
        let lage = [f.e0, f.n0, f.g, h0].map(f64::to_bits);
        if let Some(g) = merker.alt().filter(|g| {
            Arc::ptr_eq(&g.netz, &mesh) && g.lage == lage && g.flaechen == self.site_patches()
        }) {
            return Some(g.plan.clone());
        }
        let plan = self.planungsgelaende(&f, h0, &mesh);
        merker.setzen(Some(Geplant {
            netz: mesh,
            lage,
            flaechen: self.site_patches().to_vec(),
            plan: plan.clone(),
        }));
        Some(plan)
    }

    fn planungsgelaende(
        &self,
        f: &SurveyFrame,
        h0: f64,
        mesh: &Arc<crate::model::TerrainMesh>,
    ) -> PlannedTerrain {
        let plaene = crate::qto_earth::planungen(self, f, h0, mesh);
        let gilt: Vec<(usize, &sk_gelaende::planung::Planung)> = plaene
            .iter()
            .enumerate()
            .filter_map(|(i, p)| p.as_ref().map(|p| (i, p)))
            .collect();
        let refs: Vec<_> = gilt.iter().map(|x| x.1).collect();
        let o = oberflaeche(&mesh.abfrage, &refs);
        let s = schraffen(&o, &refs, PATCH_HATCH / 1000.0);
        let im_modell = |q: [f64; 3]| {
            let [x, y] = f.to_model(q[0] + mesh.bezug.0, q[1] + mesh.bezug.1);
            vec3(x, y, (q[2] - h0) * 1000.0)
        };
        let patches = self.site_patches();
        PlannedTerrain {
            replaced: o.ersetzt.clone(),
            triangles: o.dreiecke.iter().map(|(d, _)| d.map(im_modell)).collect(),
            paved: o
                .dreiecke
                .iter()
                .map(|(_, h)| matches!(h, Herkunft::Flaeche(k) if patches[gilt[*k].0].paved))
                .collect(),
            hatch: s
                .iter()
                .map(|(q, lang)| (q.map(im_modell), *lang))
                .collect(),
            outlines: gilt
                .iter()
                .map(|&(i, _)| {
                    let p = &patches[i];
                    p.outline
                        .iter()
                        .map(|q| vec3(q[0], q[1], p.z_at(q[0], q[1], h0)))
                        .collect()
                })
                .collect(),
            walls: o.waende.iter().map(|d| d.map(im_modell)).collect(),
        }
    }

    /// Die Geländeflächen in der Reihenfolge, in der sie gesetzt wurden
    /// (bei Überlappung gilt die spätere).
    pub fn site_patches(&self) -> &[TerrainPatch] {
        &self.project.site_patches
    }

    /// Neue Geländefläche (im offenen Schritt); ihre Nummer, `None`, wenn
    /// sie sich nicht bauen lässt.
    pub fn add_site_patch(&mut self, p: TerrainPatch) -> Option<usize> {
        if !p.valid() {
            return None;
        }
        self.note_project();
        self.project.site_patches.push(p);
        self.touch_height();
        Some(self.project.site_patches.len() - 1)
    }

    /// Geländefläche `i` ändern (im offenen Schritt). Gleiche Werte, eine
    /// Fläche, die sich nicht bauen lässt, oder keine Fläche `i` ändern
    /// nichts.
    pub fn set_site_patch(&mut self, i: usize, p: TerrainPatch) -> bool {
        if !p.valid() || self.project.site_patches.get(i).is_none_or(|q| *q == p) {
            return false;
        }
        self.note_project();
        self.project.site_patches[i] = p;
        self.touch_height();
        true
    }

    /// Geländefläche `i` entfernen (im offenen Schritt).
    pub fn remove_site_patch(&mut self, i: usize) -> bool {
        if i >= self.project.site_patches.len() {
            return false;
        }
        self.note_project();
        self.project.site_patches.remove(i);
        self.touch_height();
        true
    }

    /// Werte des Geländebaus.
    pub fn site_soil(&self) -> SiteSoil {
        self.project.site_soil
    }

    /// Werte des Geländebaus setzen (im offenen Schritt); außerhalb von
    /// [`SiteSoil::FELDER`] und gleiche Werte ändern nichts. Eine Zeile,
    /// die nicht galt, fällt damit weg.
    pub fn set_site_soil(&mut self, s: SiteSoil) -> bool {
        if s == self.project.site_soil || SiteSoil::aus_werten(s.werte()).is_none() {
            return false;
        }
        self.note_project();
        self.project.site_soil = s;
        self.project.site_soil_raw = None;
        self.touch_height();
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn quadrat() -> Vec<[f64; 2]> {
        vec![
            [0.0, 0.0],
            [10_000.0, 0.0],
            [10_000.0, 8_000.0],
            [0.0, 8_000.0],
        ]
    }

    #[test]
    fn ebenen() {
        let mut p = TerrainPatch::flat(quadrat(), -150.0);
        assert!(p.valid());
        assert_eq!(p.z_at(3_000.0, 4_000.0, 0.0), -150.0);
        // 2 % nach +x fallend, Höhe an der ersten Ecke
        p.height = PatchHeight::Fall {
            z: 0.0,
            dir: 0.0,
            fall: 2.0,
        };
        assert!((p.z_at(10_000.0, 5_000.0, 0.0) + 200.0).abs() < 1e-9);
        // drei Ecken
        p.height = PatchHeight::Corners(vec![(0, 0.0), (1, -100.0), (3, 80.0)]);
        assert!((p.z_at(10_000.0, 8_000.0, 0.0) + 20.0).abs() < 1e-9);
        // zwei Ecken: längs geneigt, quer waagerecht
        p.height = PatchHeight::Corners(vec![(0, 0.0), (1, -100.0)]);
        assert!((p.z_at(5_000.0, 8_000.0, 0.0) + 50.0).abs() < 1e-9);
        // absolut in m ü. NHN, ±0,00 auf 24,00
        p.abs = true;
        p.height = PatchHeight::Fall {
            z: 24.5,
            dir: 90.0,
            fall: 10.0,
        };
        assert!((p.z_at(0.0, 1_000.0, 24.0) - 400.0).abs() < 1e-6);
    }

    #[test]
    fn umriss_einfach() {
        // beide Umlaufrichtungen gelten
        let mut p = TerrainPatch::flat(quadrat(), 0.0);
        assert!(p.valid());
        p.outline.reverse();
        assert!(p.valid());
        // schiefe Schleife: Umlauffläche 20 m², aber kein Umriss
        p.outline = vec![
            [0.0, 0.0],
            [10_000.0, 6_000.0],
            [10_000.0, 0.0],
            [0.0, 10_000.0],
        ];
        assert!(flaeche(&p.outline).abs() > 1.0);
        assert!(!p.valid());
        // Rückläufer auf einer Kante
        p.outline = vec![
            [0.0, 0.0],
            [10_000.0, 0.0],
            [10_000.0, 10_000.0],
            [10_000.0, 4_000.0],
            [10_000.0, 10_000.0],
            [0.0, 10_000.0],
        ];
        assert!(!p.valid());
    }

    #[test]
    fn gueltig() {
        let mut p = TerrainPatch::flat(quadrat(), 0.0);
        p.slopes = vec![1.5, 0.0, 2.0, 1.0];
        assert!(p.valid());
        assert_eq!(p.slope(1), None);
        assert_eq!(p.slope(2), Some(2.0));
        p.slopes = vec![1.5, 2.0];
        assert!(!p.valid());
        p.slopes = vec![0.2];
        assert!(!p.valid());
        p.slopes.clear();
        p.height = PatchHeight::Corners(vec![(0, 0.0), (0, 1.0)]);
        assert!(!p.valid());
        p.height = PatchHeight::Corners(vec![(4, 0.0)]);
        assert!(!p.valid());
        p.height = PatchHeight::Flat(f64::NAN);
        assert!(!p.valid());
        // Wertebereiche (Review #173 B2): sonst inf oder Milliarden
        for z in [1e300, -1e300, 1e12, PATCH_Z_MAX + 1.0] {
            p.height = PatchHeight::Flat(z);
            assert!(!p.valid(), "{z}");
        }
        p.height = PatchHeight::Flat(-PATCH_Z_MAX);
        assert!(p.valid());
        p.abs = true;
        p.height = PatchHeight::Flat(24.35);
        assert!(p.valid());
        p.height = PatchHeight::Flat(24_350.0);
        assert!(!p.valid());
        p.abs = false;
        p.height = PatchHeight::Fall {
            z: 0.0,
            dir: 1e9,
            fall: 2.0,
        };
        assert!(!p.valid());
        // 100 % über 100 m Ecke: am Ende 100 m tiefer, gerade noch
        let lang = TerrainPatch {
            height: PatchHeight::Fall {
                z: 0.0,
                dir: 0.0,
                fall: 100.0,
            },
            ..TerrainPatch::flat(
                vec![
                    [0.0, 0.0],
                    [100_000.0, 0.0],
                    [100_000.0, 1_000.0],
                    [0.0, 1_000.0],
                ],
                0.0,
            )
        };
        assert!(lang.valid());
        let zu_lang = TerrainPatch {
            outline: vec![
                [0.0, 0.0],
                [100_001.0, 0.0],
                [100_001.0, 1_000.0],
                [0.0, 1_000.0],
            ],
            ..lang.clone()
        };
        assert!(!zu_lang.valid());
        p.height = PatchHeight::Flat(0.0);
        p.outline[1][0] = 2e8;
        assert!(!p.valid());
        p.outline = quadrat();
        let linie = TerrainPatch::flat(vec![[0.0, 0.0], [1.0, 0.0], [2.0, 0.0]], 0.0);
        assert!(!linie.valid());
    }

    /// Im Netz gedreht und verschoben: Ecken und Ebene passen zu den Höhen
    /// im Modell.
    #[test]
    fn baustein_im_netz() {
        let mut p = TerrainPatch::flat(quadrat(), 0.0);
        p.height = PatchHeight::Fall {
            z: 300.0,
            dir: 30.0,
            fall: 5.0,
        };
        p.slopes = vec![1.5, 0.0, 1.5, 2.0];
        let frame = SurveyFrame {
            e0: 467_100.0,
            n0: 5_878_700.0,
            g: 0.4,
        };
        let (h0, bezug) = (24.0, (467_000.0, 5_878_600.0));
        let b = p.baustein(&frame, h0, bezug).unwrap();
        assert_eq!(b.rand.len(), 4);
        assert_eq!(b.rand[1].neigung, None);
        assert_eq!(b.rand[3].neigung, Some(2.0));
        for (k, q) in p.outline.iter().enumerate() {
            let (e, n) = frame.to_survey(q[0], q[1]);
            let a = b.rand[k].a;
            assert!((a[0] - (e - bezug.0)).abs() < 1e-9 && (a[1] - (n - bezug.1)).abs() < 1e-9);
            let soll = h0 + p.z_at(q[0], q[1], h0) / 1000.0;
            assert!((a[2] - soll).abs() < 1e-9, "{} {soll}", a[2]);
            let t = &b.teile[0].ebene;
            assert!((t[0] + t[1] * a[0] + t[2] * a[1] - soll).abs() < 1e-9);
        }
    }

    /// Auf ebenem Aufmaß (10,00 m ü. NHN, ±0,00 auf 9,50) eine Fläche 1 m
    /// über dem Gelände: Das Planungsgelände ersetzt die Dreiecke darunter,
    /// deckt sie genau, liegt zwischen Gelände und Fläche und hat
    /// Schraffen auf der Böschung.
    #[test]
    fn planungsgelaende_im_modell() {
        use crate::model::Terrain;
        let mut m = Model::with_seed(7);
        let a = sk_gelaende::lesen::lesen(b"1 0 0 10\n2 60 0 10\n3 60 60 10\n4 0 60 10\n").unwrap();
        m.begin("Aufmaß");
        assert!(m.import_terrain(Terrain::from_survey(&a, "eben.txt", "2026-10-14", 32)));
        assert!(m.set_terrain_nhn(9.5));
        m.commit();
        assert_eq!(m.planned_terrain(), None, "ohne Fläche");
        let r = vec![
            [-5_000.0, -4_000.0],
            [5_000.0, -4_000.0],
            [5_000.0, 4_000.0],
            [-5_000.0, 4_000.0],
        ];
        m.begin("Fläche");
        let mut p = TerrainPatch::flat(r, 1500.0);
        p.paved = true;
        assert!(m.add_site_patch(p).is_some());
        m.commit();
        // in einer Kopie gerechnet, im Modell gemerkt (Review #183 a)
        let kopie = m.clone();
        assert!(m.planmerker().alt().is_none());
        let g = kopie.planned_terrain().unwrap();
        assert!(m.planmerker().alt().is_some());
        assert_eq!(m.planned_terrain().as_ref(), Some(&g));
        assert!(!g.replaced.is_empty());
        assert!(g.walls.is_empty(), "mit Böschung keine Wand");
        assert_eq!(g.triangles.len(), g.paved.len());
        let flaeche = |d: &[Vec3; 3]| ((d[1] - d[0]).cross(d[2] - d[0])).z / 2.0;
        let alt = m.terrain_triangles();
        let weg: f64 = g.replaced.iter().map(|&i| flaeche(&alt[i as usize])).sum();
        let neu: f64 = g.triangles.iter().map(flaeche).sum();
        assert!((weg - neu).abs() < 1.0, "{weg} {neu}");
        assert!(g.triangles.iter().all(|d| flaeche(d) > 0.0));
        for (d, befestigt) in g.triangles.iter().zip(&g.paved) {
            let z: Vec<f64> = d.iter().map(|q| q.z).collect();
            assert!(
                z.iter().all(|z| (500.0 - 1e-6..=1500.0 + 1e-6).contains(z)),
                "{z:?}"
            );
            let auf = z.iter().all(|z| (z - 1500.0).abs() < 1e-6);
            let mitte = (d[0] + d[1] + d[2]) * (1.0 / 3.0);
            let innen =
                (-5_000.0..=5_000.0).contains(&mitte.x) && (-4_000.0..=4_000.0).contains(&mitte.y);
            assert_eq!(*befestigt, innen, "{d:?}");
            assert!(!innen || auf);
        }
        // Böschung 1:1,5 bis 1,5 m vor der Kante
        let x_max = g
            .triangles
            .iter()
            .filter(|d| d.iter().any(|q| q.z > 500.0 + 1e-6))
            .flatten()
            .map(|q| q.x)
            .fold(0.0, f64::max);
        assert!((x_max - 6_500.0).abs() < 1e-6, "{x_max}");
        assert!(!g.hatch.is_empty());
        for ([a, b], _) in &g.hatch {
            for q in [a, b] {
                assert!((500.0 - 1e-6..=1500.0 + 1e-6).contains(&q.z), "{q:?}");
            }
        }
        assert_eq!(g.outlines.len(), 1);
        assert!(g.outlines[0].iter().all(|q| q.z == 1500.0));
        // senkrecht: neu gerechnet, Wände 1,00 m hoch rings um 10 × 8 m
        m.begin("senkrecht");
        let mut p = m.site_patches()[0].clone();
        p.slopes = vec![0.0];
        assert!(m.set_site_patch(0, p));
        m.commit();
        let s = m.planned_terrain().unwrap();
        assert_ne!(s, g);
        let wand: f64 = s
            .walls
            .iter()
            .map(|d| (d[1] - d[0]).cross(d[2] - d[0]).length() / 2.0)
            .sum();
        assert!((wand - 36_000.0 * 1_000.0).abs() < 1.0, "{wand}");
        // ohne Flächen nichts gemerkt
        m.begin("weg");
        assert!(m.remove_site_patch(0));
        m.commit();
        assert_eq!(m.planned_terrain(), None);
        assert!(m.planmerker().alt().is_none());
    }

    #[test]
    fn im_schritt_und_zurueck() {
        use crate::txn::Direction;
        let mut m = Model::with_seed(3);
        m.begin("Fläche");
        assert_eq!(
            m.add_site_patch(TerrainPatch::flat(quadrat(), -150.0)),
            Some(0)
        );
        let neu = m.commit().unwrap();
        assert_eq!(m.site_patches().len(), 1);
        m.begin("Fläche");
        let mut p = m.site_patches()[0].clone();
        assert!(!m.set_site_patch(0, p.clone()));
        p.paved = true;
        assert!(m.set_site_patch(0, p));
        assert!(!m.set_site_patch(1, TerrainPatch::flat(quadrat(), 0.0)));
        let belag = m.commit().unwrap();
        assert!(m.site_patches()[0].paved);
        m.apply(&belag, Direction::Undo);
        assert!(!m.site_patches()[0].paved);
        m.apply(&neu, Direction::Undo);
        assert!(m.site_patches().is_empty());
        m.apply(&neu, Direction::Redo);
        assert_eq!(m.site_patches().len(), 1);
        m.begin("Boden");
        assert!(!m.set_site_soil(SiteSoil {
            spread: 200.0,
            reuse: 1.5
        }));
        assert!(m.set_site_soil(SiteSoil {
            spread: 150.0,
            reuse: 0.9
        }));
        m.commit();
        assert_eq!(m.site_soil().spread, 150.0);
    }
}
