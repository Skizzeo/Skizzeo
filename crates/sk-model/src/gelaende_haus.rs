//! „Gelände ans Haus“ (Gelände G3, plan-gelaendemodul.md „G3 Feinplan“):
//! ein Streifen um den Sockel eines Gebäudes, Vorgabe 1,00 m breit, OK
//! 15 cm unter OK Sohle (±0,00) und 2 % Gefälle vom Haus weg, außen die
//! Böschung bis ans Urgelände. Je Wand eine geneigte Teilfläche; an den
//! Ecken treffen sich die Ebenen zweier Wände auf Gehrung (Grat oder
//! Kehle), der Außenrand liegt waagerecht.
//!
//! Der Streifen ist eine Geländefläche ([`TerrainPatch`] mit
//! [`HouseStrip`]): Mengen, Darstellung und Datei gehen wie bei jeder
//! Fläche. Er folgt dem Haus: Jeder Schritt, der den Außenumriss ändert,
//! zieht ihn im selben Schritt mit ([`Model::sync_house_strips`]), wie die
//! Randdämmstreifen den Wänden folgen; ohne das Haus entfällt er.

use super::{BuildingId, Guid, Model, PatchHeight, TerrainPatch, PATCH_Z_MAX};
use sk_math::vec3;

/// Vorgaben des Streifens: Breite (mm), OK am Haus über ±0,00 (mm),
/// Gefälle vom Haus weg (%).
pub const STRIP_WIDTH: f64 = 1000.0;
pub const STRIP_Z: f64 = -150.0;
pub const STRIP_FALL: f64 = 2.0;
/// Bereiche von Breite (mm) und Gefälle (%).
pub const STRIP_WIDTH_RANGE: (f64, f64) = (200.0, 10_000.0);
pub const STRIP_FALL_RANGE: (f64, f64) = (0.0, 20.0);

/// Unter diesem Abstand (mm) gelten zwei Ecken des Hausumrisses als eine.
const DOPPELT: f64 = 1.0;

/// Ein Streifen am Haus: das Gebäude, dessen Außenumriss (gegen den
/// Uhrzeigersinn, wie [`Model::plate_outlines`]) er zuletzt gefolgt ist,
/// Breite und Gefälle. Der Außenrand steht in [`TerrainPatch::outline`],
/// Ecke für Ecke zu den Ecken des Hauses; die Höhe am Haus ist die eine
/// Höhe der Fläche ([`PatchHeight::Flat`], mm über ±0,00).
#[derive(Clone, Debug, PartialEq)]
pub struct HouseStrip {
    pub building: Guid,
    pub house: Vec<[f64; 2]>,
    /// Breite (mm).
    pub width: f64,
    /// Gefälle vom Haus weg (%).
    pub fall: f64,
}

impl HouseStrip {
    /// Außenrand des Streifens um das Haus `house` (gegen den
    /// Uhrzeigersinn), Ecke `i` außen zu Ecke `i` des Hauses: jede Kante um
    /// `width` nach außen versetzt, die Ecken im Schnitt der Nachbarn.
    /// `None`, wenn er sich so nicht bauen lässt (eine Kante kehrt sich um,
    /// etwa an einer Einbuchtung schmaler als zwei Streifen, oder der Rand
    /// kreuzt sich).
    pub fn aussen(house: &[[f64; 2]], width: f64) -> Option<Vec<[f64; 2]>> {
        let n = house.len();
        if n < 3 || !(width.is_finite() && width > 0.0) {
            return None;
        }
        let h: Vec<_> = house.iter().map(|p| vec3(p[0], p[1], 0.0)).collect();
        if sk_math::polygon::signed_area(&h) <= 0.0 || !sk_math::polygon::is_simple(&h) {
            return None;
        }
        // links ist innen: nach außen mit negativem Abstand
        let o = sk_math::flaeche::versatz(&h, &vec![-width; n])?;
        if o.len() != n || !sk_math::polygon::is_simple(&o) {
            return None;
        }
        Some(o.iter().map(|p| [p.x, p.y]).collect())
    }

    /// Ebene der Teilfläche an Wand `i` (von Ecke `i` nach `i + 1`):
    /// `z = a + b·x + c·y` in mm über ±0,00, `z` am Haus, fällt mit
    /// [`HouseStrip::fall`] quer zur Wand nach außen.
    pub fn ebene(&self, i: usize, z: f64) -> [f64; 3] {
        let n = self.house.len();
        let (a, b) = (self.house[i % n], self.house[(i + 1) % n]);
        let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
        let l = dx.hypot(dy).max(1e-9);
        // nach außen: rechts der Kante gegen den Uhrzeigersinn
        let (nx, ny) = (dy / l, -dx / l);
        let k = self.fall / 100.0;
        [z + k * (a[0] * nx + a[1] * ny), -k * nx, -k * ny]
    }

    /// Je Wand das Viereck zwischen Haus und Außenrand `aussen` mit seiner
    /// Ebene ([`HouseStrip::ebene`]).
    pub fn teile(&self, aussen: &[[f64; 2]], z: f64) -> Vec<(Vec<[f64; 2]>, [f64; 3])> {
        let n = self.house.len();
        if aussen.len() != n {
            return Vec::new();
        }
        (0..n)
            .map(|i| {
                let j = (i + 1) % n;
                let ring = vec![self.house[i], self.house[j], aussen[j], aussen[i]];
                (ring, self.ebene(i, z))
            })
            .collect()
    }

    /// Lässt sich der Streifen mit dem Außenrand `aussen` und der Höhe am
    /// Haus `z` bauen? Werte in ihren Bereichen, der Rand passt zum Haus,
    /// außen höchstens [`PATCH_Z_MAX`] unter ±0,00.
    pub(crate) fn gueltig(&self, aussen: &[[f64; 2]], z: f64) -> bool {
        let (w0, w1) = STRIP_WIDTH_RANGE;
        let (f0, f1) = STRIP_FALL_RANGE;
        let werte = (w0..=w1).contains(&self.width)
            && (f0..=f1).contains(&self.fall)
            && z.abs() <= PATCH_Z_MAX
            && (z - self.fall / 100.0 * self.width).abs() <= PATCH_Z_MAX;
        werte && Self::aussen(&self.house, self.width).is_some_and(|o| nah(&o, aussen))
    }
}

/// Liegen die Ringe Ecke für Ecke höchstens 1 mm auseinander? (Der
/// Außenrand kommt aus der Datei in gerundeten Zahlen zurück.)
fn nah(a: &[[f64; 2]], b: &[[f64; 2]]) -> bool {
    a.len() == b.len()
        && a.iter()
            .zip(b)
            .all(|(p, q)| (p[0] - q[0]).hypot(p[1] - q[1]) <= DOPPELT)
}

/// Der Hausumriss ohne doppelte Ecken, gegen den Uhrzeigersinn.
fn bereinigt(ring: &[sk_math::Vec3]) -> Vec<[f64; 2]> {
    let mut out: Vec<[f64; 2]> = Vec::with_capacity(ring.len());
    for p in sk_math::polygon::to_ccw(ring) {
        if out
            .last()
            .is_none_or(|q| (p.x - q[0]).hypot(p.y - q[1]) > DOPPELT)
        {
            out.push([p.x, p.y]);
        }
    }
    while out.len() > 1 && {
        let (a, b) = (out[0], out[out.len() - 1]);
        (a[0] - b[0]).hypot(a[1] - b[1]) <= DOPPELT
    } {
        out.pop();
    }
    out
}

impl Model {
    /// Außenumriss des Gebäudes `b`, um den ein Streifen läuft: von seinen
    /// Umrissen ([`Model::plate_outlines`]) der größte, ohne doppelte Ecken,
    /// gegen den Uhrzeigersinn; `None` ohne Gründung.
    pub fn house_ring(&self, b: BuildingId) -> Option<Vec<[f64; 2]>> {
        self.plate_outlines()
            .into_iter()
            .filter(|(x, _)| *x == b)
            .map(|(_, r)| r)
            .max_by(|r, s| sk_math::polygon::area(r).total_cmp(&sk_math::polygon::area(s)))
            .map(|r| bereinigt(&r))
    }

    /// Ein neuer Streifen um das Gebäude `b` nach den Vorgaben; `None` ohne
    /// Gründung oder wenn er sich um diesen Umriss nicht bauen lässt.
    pub fn house_strip(&self, b: BuildingId) -> Option<TerrainPatch> {
        let building = self.building(b)?.guid;
        let house = self.house_ring(b)?;
        let outline = HouseStrip::aussen(&house, STRIP_WIDTH)?;
        let p = TerrainPatch {
            outline,
            height: PatchHeight::Flat(STRIP_Z),
            abs: false,
            slopes: Vec::new(),
            paved: false,
            house: Some(HouseStrip {
                building,
                house,
                width: STRIP_WIDTH,
                fall: STRIP_FALL,
            }),
        };
        p.valid().then_some(p)
    }

    /// Nummer des Streifens am Gebäude `b`, wenn es einen gibt.
    pub fn house_strip_of(&self, b: BuildingId) -> Option<usize> {
        let g = self.building(b)?.guid;
        self.site_patches()
            .iter()
            .position(|p| p.house.as_ref().is_some_and(|s| s.building == g))
    }

    /// Die Streifen folgen ihren Häusern (beim Schließen eines Schritts):
    /// Hat sich der Außenumriss geändert, wird der Streifen neu gelegt;
    /// lässt er sich um den neuen Umriss nicht bauen, bleibt er, wie er
    /// war, und die Mengen melden ihn. Ohne Haus (Gebäude oder Gründung
    /// fort) entfällt er.
    pub(crate) fn sync_house_strips(&mut self) {
        if self.site_patches().iter().all(|p| p.house.is_none()) {
            return;
        }
        let mut neu = self.site_patches().to_vec();
        let mut weg = Vec::new();
        for (i, p) in neu.iter_mut().enumerate() {
            let Some(s) = p.house.as_mut() else {
                continue;
            };
            let b = self
                .buildings()
                .iter()
                .find(|(_, x)| x.guid == s.building)
                .map(|(id, _)| id);
            let Some(ring) = b.and_then(|b| self.house_ring(b)) else {
                weg.push(i);
                continue;
            };
            if ring == s.house {
                continue;
            }
            if let Some(o) = HouseStrip::aussen(&ring, s.width) {
                // Böschung je Kante nur, solange es dieselben Kanten sind
                if p.slopes.len() > 1 && o.len() != p.slopes.len() {
                    p.slopes = vec![p.slopes[0]];
                }
                s.house = ring;
                p.outline = o;
            }
        }
        for &i in weg.iter().rev() {
            neu.remove(i);
        }
        if neu == self.project.site_patches {
            return;
        }
        self.note_project();
        // rohe Zeilen hinter einem entfallenen Streifen rücken mit
        for &i in weg.iter().rev() {
            for (k, _) in &mut self.project.site_patches_raw {
                if *k > i {
                    *k -= 1;
                }
            }
        }
        self.project.site_patches = neu;
        self.touch_height();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Terrain;
    use crate::txn::Direction;

    fn nah(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-6
    }

    fn rechteck(x0: f64, y0: f64, x1: f64, y1: f64) -> Vec<[f64; 2]> {
        vec![[x0, y0], [x1, y0], [x1, y1], [x0, y1]]
    }

    fn streifen(house: Vec<[f64; 2]>) -> HouseStrip {
        HouseStrip {
            building: Guid(1),
            house,
            width: STRIP_WIDTH,
            fall: STRIP_FALL,
        }
    }

    /// Rechteck 10 × 8 m: Außenrand 1 m weiter, am Haus 15 cm unter
    /// ±0,00, außen 2 cm tiefer; an jeder Ecke treffen sich die Ebenen der
    /// beiden Wände (Gehrung), auch an der Kehle eines L.
    #[test]
    fn rand_und_ebenen() {
        let h = rechteck(0.0, 0.0, 10_000.0, 8_000.0);
        let o = HouseStrip::aussen(&h, STRIP_WIDTH).unwrap();
        let soll = rechteck(-1_000.0, -1_000.0, 11_000.0, 9_000.0);
        assert!(nah_ring(&o, &soll), "{o:?}");
        let l = vec![
            [0.0, 0.0],
            [10_000.0, 0.0],
            [10_000.0, 4_000.0],
            [4_000.0, 4_000.0],
            [4_000.0, 8_000.0],
            [0.0, 8_000.0],
        ];
        for h in [h, l] {
            let s = streifen(h.clone());
            let o = HouseStrip::aussen(&h, s.width).unwrap();
            let teile = s.teile(&o, STRIP_Z);
            assert_eq!(teile.len(), h.len());
            let z = |e: &[f64; 3], p: [f64; 2]| e[0] + e[1] * p[0] + e[2] * p[1];
            for (r, e) in &teile {
                assert!(nah(z(e, r[0]), -150.0) && nah(z(e, r[1]), -150.0));
                assert!(nah(z(e, r[2]), -170.0) && nah(z(e, r[3]), -170.0));
            }
            for i in 0..h.len() {
                let vor = &teile[(i + h.len() - 1) % h.len()].1;
                let e = &teile[i].1;
                for p in [h[i], o[i]] {
                    assert!(nah(z(vor, p), z(e, p)), "Ecke {i}");
                }
            }
        }
    }

    fn nah_ring(a: &[[f64; 2]], b: &[[f64; 2]]) -> bool {
        a.len() == b.len()
            && a.iter()
                .zip(b)
                .all(|(p, q)| (p[0] - q[0]).hypot(p[1] - q[1]) < 1e-6)
    }

    /// Eine Einbuchtung schmaler als zwei Streifen: kein Streifen; ein
    /// schmalerer geht.
    #[test]
    fn einbuchtung() {
        let u = vec![
            [0.0, 0.0],
            [10_000.0, 0.0],
            [10_000.0, 8_000.0],
            [5_750.0, 8_000.0],
            [5_750.0, 3_000.0],
            [4_250.0, 3_000.0],
            [4_250.0, 8_000.0],
            [0.0, 8_000.0],
        ];
        assert!(HouseStrip::aussen(&u, 1_000.0).is_none());
        assert!(HouseStrip::aussen(&u, 500.0).is_some());
        // im Uhrzeigersinn gilt nicht (der Umriss kommt gegen ihn)
        let mut cw = rechteck(0.0, 0.0, 10_000.0, 8_000.0);
        cw.reverse();
        assert!(HouseStrip::aussen(&cw, 1_000.0).is_none());
    }

    fn haus() -> (Model, BuildingId, crate::RunId) {
        let mut m = Model::with_seed(7);
        let b = m.add_building(1);
        let pts = [
            vec3(7000.0, 6000.0, 0.0),
            vec3(7000.0, 14000.0, 0.0),
            vec3(17000.0, 14000.0, 0.0),
            vec3(17000.0, 6000.0, 0.0),
        ];
        let run = m.build_from_polygon(b, &pts).unwrap();
        let a = sk_gelaende::lesen::lesen(b"1 0 0 10\n2 60 0 10\n3 60 60 10\n4 0 60 10\n").unwrap();
        m.begin("Aufmaß");
        assert!(m.import_terrain(Terrain::from_survey(&a, "eben.txt", "2026-10-14", 32)));
        assert!(m.set_terrain_nhn(9.5));
        m.commit();
        (m, b, run)
    }

    /// Am Haus auf ebenem Gelände (±0,00 50 cm über Gelände): Der Streifen
    /// liegt um das Haus, im Netz waagerecht am Außenrand, hat Auftrag und
    /// eine Höhenzahl in einer Teilfläche, kommt aus der Datei gleich
    /// zurück und folgt dem Haus, wenn sich die Wände ändern; ohne Haus
    /// entfällt er. Rückgängig holt jeden Stand zurück.
    #[test]
    fn folgt_dem_haus() {
        let (mut m, b, run) = haus();
        let p = m.house_strip(b).unwrap();
        let ring = m.house_ring(b).unwrap();
        assert_eq!(p.house.as_ref().unwrap().house, ring);
        let links = ring.iter().map(|q| q[0]).fold(f64::MAX, f64::min);
        assert!(p.contains([links - 500.0, 10_000.0]));
        assert!(!p.contains([links - 1_500.0, 10_000.0]));
        assert!(!p.contains([12_000.0, 10_000.0]), "im Haus");
        let breite = |r: &[[f64; 2]]| r.iter().map(|q| q[0]).fold(f64::MIN, f64::max);
        assert!(nah(breite(&p.outline), breite(&ring) + 1_000.0));
        m.begin("am Haus");
        assert_eq!(m.add_site_patch(p.clone()), Some(0));
        let neu = m.commit().unwrap();
        assert_eq!(m.house_strip_of(b), Some(0));
        // im Netz: vier Wände, der Rand waagerecht 17 cm unter ±0,00
        let (f, h0) = (m.survey_frame().unwrap(), m.terrain_nhn().unwrap());
        let t = m.terrain().unwrap().mesh();
        let bs = p.baustein(&f, h0, t.bezug).unwrap();
        assert_eq!(bs.teile.len(), 4);
        assert!(bs
            .rand
            .iter()
            .all(|k| nah(k.a[2], h0 - 0.17) && nah(k.b[2], h0 - 0.17)));
        let g = m.planned_terrain().unwrap();
        let q = g.strip_marks[0].unwrap();
        assert!((-170.0..=-150.0).contains(&q.z) && !p.contains([12_000.0, 10_000.0]));
        assert!(p.contains([q.x, q.y]));
        let auf: f64 = crate::qto::schedule(&m)
            .auto
            .iter()
            .filter(|r| {
                r.key == crate::qto_earth::EMBANK || r.key == crate::qto_earth::EMBANK_SUPPLY
            })
            .map(|r| r.value / 1e9)
            .sum();
        assert!(auf > 0.0, "{auf}");
        // Datei hin und zurück
        let t = crate::szo::write(&m);
        assert!(t.contains("[tpatch]") && t.contains(" bau="), "{t}");
        let back = crate::szo::read(&t, crate::GuidGen::with_seed(9)).unwrap();
        assert_eq!(back.model.site_patches(), m.site_patches());
        assert_eq!(crate::szo::write(&back.model), t);
        // Haus 2 m breiter: der Streifen zieht im selben Schritt mit
        let pts = [
            vec3(7000.0, 6000.0, 0.0),
            vec3(7000.0, 14000.0, 0.0),
            vec3(19000.0, 14000.0, 0.0),
            vec3(19000.0, 6000.0, 0.0),
        ];
        m.begin("Wand");
        assert!(m.set_run_points(run, &pts).is_some());
        let wand = m.commit().unwrap();
        let s = &m.site_patches()[0];
        let r2 = m.house_ring(b).unwrap();
        assert_eq!(s.house.as_ref().unwrap().house, r2);
        assert!(nah(breite(&s.outline), breite(&r2) + 1_000.0));
        assert!(breite(&r2) > breite(&ring) + 1_900.0);
        m.apply(&wand, Direction::Undo);
        assert_eq!(m.site_patches()[0], p);
        m.apply(&wand, Direction::Redo);
        assert_eq!(m.site_patches()[0].house.as_ref().unwrap().house, r2);
        // ohne Haus kein Streifen
        m.begin("weg");
        assert!(m.remove_building(b));
        let weg = m.commit().unwrap();
        assert!(m.site_patches().is_empty());
        m.apply(&weg, Direction::Undo);
        assert_eq!(m.site_patches().len(), 1);
        m.apply(&neu, Direction::Undo);
        assert!(m.site_patches().is_empty() || m.site_patches()[0].house.is_some());
    }
}
