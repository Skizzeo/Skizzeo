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

use super::{BuildingId, Guid, Model, PatchHeight, RunId, TerrainPatch, PATCH_Z_MAX};
use sk_math::vec3;
use std::collections::HashSet;

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

/// Was der Abgleich beim Schließen eines Schritts über einen Streifen zu
/// melden hat ([`Model::house_strip_note`]); `serial` zählt die Meldungen,
/// damit die App jede nur einmal zeigt.
#[derive(Clone, Debug, PartialEq)]
pub struct StripNote {
    pub serial: u64,
    /// Gebäude des Streifens.
    pub building: Guid,
    /// `Some(b)`: schmaler geworden, jetzt `b` mm breit; `None`: folgt dem
    /// Haus nicht, weil nicht einmal der schmalste Streifen passt.
    pub width: Option<f64>,
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

    /// Die größte Breite bis `bis` (auf 1 cm, mindestens
    /// [`STRIP_WIDTH_RANGE`]`.0`), mit der sich der Streifen um `house`
    /// legen lässt, samt Außenrand; `None`, wenn nicht einmal die kleinste
    /// passt. Eine Einbuchtung schmaler als zwei Streifen begrenzt sie
    /// (Bedienbarkeit zu #226).
    pub fn breiteste(house: &[[f64; 2]], bis: f64) -> Option<(f64, Vec<[f64; 2]>)> {
        if let Some(o) = Self::aussen(house, bis) {
            return Some((bis, o));
        }
        // in cm halbieren: passt `lo`, passt `hi` nicht
        let (mut lo, mut hi) = ((STRIP_WIDTH_RANGE.0 / 10.0).ceil(), (bis / 10.0).floor());
        let mut o = Self::aussen(house, lo * 10.0)?;
        while hi - lo > 1.0 {
            let mitte = ((lo + hi) / 2.0).floor();
            match Self::aussen(house, mitte * 10.0) {
                Some(x) => (lo, o) = (mitte, x),
                None => hi = mitte,
            }
        }
        Some((lo * 10.0, o))
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
        // nur die Züge dieses Gebäudes (Review A-226a: alle Umrisse aller
        // Häuser sprengten bei 1000 Häusern das Loslassen)
        self.lesend(|| {
            self.runs
                .iter()
                .filter(|(_, r)| self.building_of(r.storey) == Some(b))
                .filter_map(|(id, _)| self.plate_outline(id))
                .max_by(|r, s| sk_math::polygon::area(r).total_cmp(&sk_math::polygon::area(s)))
        })
        .map(|r| bereinigt(&r))
    }

    /// Ein neuer Streifen um das Gebäude `b` nach den Vorgaben; `None` ohne
    /// Gründung oder wenn er sich um diesen Umriss nicht bauen lässt.
    pub fn house_strip(&self, b: BuildingId) -> Option<TerrainPatch> {
        let building = self.building(b)?.guid;
        let house = self.house_ring(b)?;
        let (width, outline) = HouseStrip::breiteste(&house, STRIP_WIDTH)?;
        let p = TerrainPatch {
            outline,
            height: PatchHeight::Flat(STRIP_Z),
            abs: false,
            slopes: Vec::new(),
            paved: false,
            house: Some(HouseStrip {
                building,
                house,
                width,
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

    /// Was der letzte Abgleich der Streifen gemeldet hat (siehe
    /// [`StripNote`]).
    pub fn house_strip_note(&self) -> Option<&StripNote> {
        self.strip_note.as_ref()
    }

    /// Die Streifen folgen ihren Häusern (beim Schließen eines Schritts):
    /// Hat sich der Außenumriss geändert, wird der Streifen neu gelegt,
    /// wenn nötig schmaler (mit Meldung); passt nicht einmal der schmalste,
    /// bleibt er, wie er war, und meldet das. Ohne Haus (Gebäude oder
    /// Gründung fort) entfällt er. Nur Streifen an Gebäuden, deren Züge der
    /// Schritt berührt (`scope` wie [`Model::sync_scope`]); im Zweifel
    /// (`None`, ein Zug fort) alle (Review A-226a).
    pub(crate) fn sync_house_strips(&mut self, scope: Option<&[RunId]>) {
        if self.site_patches().iter().all(|p| p.house.is_none()) {
            return;
        }
        let beruehrt: Option<HashSet<BuildingId>> = scope.and_then(|runs| {
            runs.iter()
                .map(|r| self.runs.get(*r).and_then(|r| self.building_of(r.storey)))
                .collect()
        });
        let mut neu = self.site_patches().to_vec();
        let mut weg = Vec::new();
        let mut meldung = None;
        for (i, p) in neu.iter_mut().enumerate() {
            let Some(s) = p.house.as_mut() else {
                continue;
            };
            let b = self
                .buildings()
                .iter()
                .find(|(_, x)| x.guid == s.building)
                .map(|(id, _)| id);
            if let (Some(b), Some(nur)) = (b, &beruehrt) {
                if !nur.contains(&b) {
                    continue;
                }
            }
            let Some(ring) = b.and_then(|b| self.house_ring(b)) else {
                weg.push(i);
                continue;
            };
            if ring == s.house {
                continue;
            }
            match HouseStrip::breiteste(&ring, s.width) {
                Some((w, o)) => {
                    // Böschung je Kante nur, solange es dieselben Kanten sind
                    if p.slopes.len() > 1 && o.len() != p.slopes.len() {
                        p.slopes = vec![p.slopes[0]];
                    }
                    if w < s.width {
                        meldung = Some((s.building, Some(w)));
                    }
                    (s.house, s.width, p.outline) = (ring, w, o);
                }
                None => meldung = Some((s.building, None)),
            }
        }
        if let Some((building, width)) = meldung {
            let serial = self.strip_note.as_ref().map_or(1, |n| n.serial + 1);
            self.strip_note = Some(StripNote {
                serial,
                building,
                width,
            });
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
    /// schmalerer geht, der breiteste auf 1 cm (Bedienbarkeit zu #226).
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
        let (w, o) = HouseStrip::breiteste(&u, 1_000.0).unwrap();
        assert!((500.0..750.0).contains(&w) && w % 10.0 == 0.0, "{w}");
        assert_eq!(HouseStrip::aussen(&u, w), Some(o));
        assert!(HouseStrip::aussen(&u, w + 10.0).is_none());
        assert_eq!(HouseStrip::breiteste(&u, 400.0).unwrap().0, 400.0);
        // schmaler als 2 × 20 cm: gar keiner
        let mut eng = u.clone();
        (eng[3][0], eng[6][0]) = (5_150.0, 4_850.0);
        (eng[4][0], eng[5][0]) = (5_150.0, 4_850.0);
        assert!(HouseStrip::breiteste(&eng, 1_000.0).is_none());
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

    /// Am Haus auf ebenem Gelände (±0,00 50 cm unter Gelände): Der Streifen
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
        let auf = aufschuettung(&m);
        assert!(auf > 0.0, "{auf}");
        // Regelfall: Gelände unter ±0,00, der Streifen wird angeschüttet
        // (Prüfung VOB zu #226, c)
        m.begin("Höhe");
        assert!(m.set_terrain_nhn(10.5));
        let hoch = m.commit().unwrap();
        assert!(aufschuettung(&m) > auf + 1.0, "{}", aufschuettung(&m));
        m.apply(&hoch, Direction::Undo);
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
        // eine Nische von 1,50 m: der Streifen folgt schmaler und meldet
        // es (Bedienbarkeit zu #226)
        assert_eq!(m.house_strip_note(), None);
        let nische = [
            vec3(7000.0, 6000.0, 0.0),
            vec3(7000.0, 14000.0, 0.0),
            vec3(12250.0, 14000.0, 0.0),
            vec3(12250.0, 9000.0, 0.0),
            vec3(13750.0, 9000.0, 0.0),
            vec3(13750.0, 14000.0, 0.0),
            vec3(19000.0, 14000.0, 0.0),
            vec3(19000.0, 6000.0, 0.0),
        ];
        m.begin("Nische");
        assert!(m.set_run_points(run, &nische).is_some());
        let schritt = m.commit().unwrap();
        let s = m.site_patches()[0].house.clone().unwrap();
        assert_eq!(s.house, m.house_ring(b).unwrap());
        assert!(
            s.width < 750.0 && s.width >= STRIP_WIDTH_RANGE.0,
            "{}",
            s.width
        );
        assert!(m.site_patches()[0].valid());
        let n = m.house_strip_note().unwrap().clone();
        assert_eq!((n.serial, n.width), (1, Some(s.width)));
        assert_eq!(n.building, s.building);
        m.apply(&schritt, Direction::Undo);
        assert_eq!(m.site_patches()[0].house.as_ref().unwrap().width, 1_000.0);
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

    /// Auftrag der Böschung samt Lieferung (m³).
    fn aufschuettung(m: &Model) -> f64 {
        crate::qto::schedule(m)
            .auto
            .iter()
            .filter(|r| {
                r.key == crate::qto_earth::EMBANK || r.key == crate::qto_earth::EMBANK_SUPPLY
            })
            .map(|r| r.value / 1e9)
            .sum()
    }

    /// `n` Häuser 10 × 8 m im Raster zu 15 m, je eins ein Gebäude, mit
    /// ihren Wandzügen.
    fn viele(n: usize) -> (Model, Vec<BuildingId>, Vec<RunId>) {
        let mut m = Model::with_seed(5);
        let (mut gb, mut runs) = (Vec::new(), Vec::new());
        for i in 0..n {
            let b = m.add_building(1);
            runs.push(m.build_from_polygon(b, &rechteck_um(i, 0.0)).unwrap());
            gb.push(b);
        }
        (m, gb, runs)
    }

    fn rechteck_um(i: usize, dx: f64) -> [sk_math::Vec3; 4] {
        let (ox, oy) = ((i % 64) as f64 * 15000.0, (i / 64) as f64 * 15000.0);
        let p = |x: f64, y: f64| vec3(ox + x, oy + y, 0.0);
        [
            p(0.0, 0.0),
            p(0.0, 8000.0),
            p(10000.0 + dx, 8000.0),
            p(10000.0 + dx, 0.0),
        ]
    }

    /// Bestwert des Abgleichs der Streifen (ms) über `k` Wandzüge an Haus
    /// `haus`, im Schritt gemessen wie beim Loslassen.
    fn zuege(m: &mut Model, runs: &[RunId], haus: usize, k: usize) -> f64 {
        let mut best = f64::INFINITY;
        for j in 0..k {
            m.begin("Wand");
            let dx = if j % 2 == 0 { 200.0 } else { 0.0 };
            assert!(m
                .set_run_points(runs[haus], &rechteck_um(haus, dx))
                .is_some());
            let scope = m.sync_scope();
            let t = std::time::Instant::now();
            m.sync_house_strips(scope.as_deref());
            best = best.min(t.elapsed().as_secs_f64() * 1000.0);
            m.commit();
        }
        best
    }

    /// Leistung (Review A-226a): 1000 Häuser, ein Streifen an Haus 0. Ein
    /// Wandzug an einem anderen Haus rechnet keinen Umriss, einer am Haus
    /// des Streifens nur die Züge dieses Hauses; der Streifen folgt.
    #[test]
    fn tempo_viele_haeuser() {
        let (mut m, gb, runs) = viele(1000);
        let p = m.house_strip(gb[0]).unwrap();
        m.begin("am Haus");
        m.add_site_patch(p.clone());
        m.commit();
        let anderes = zuege(&mut m, &runs, 500, 5);
        assert_eq!(m.site_patches()[0], p, "fremdes Haus");
        let eigenes = zuege(&mut m, &runs, 0, 5);
        let s = m.site_patches()[0].house.clone().unwrap();
        assert_eq!(s.house, m.house_ring(gb[0]).unwrap());
        let (a, e) = if cfg!(debug_assertions) {
            (20.0, 80.0)
        } else {
            (0.5, 2.0)
        };
        assert!(anderes < a && eigenes < e, "{anderes:.2} / {eigenes:.2} ms");
    }

    /// Wie [`tempo_viele_haeuser`] bei 1000 und 4000 Häusern, mit Ausgabe.
    #[test]
    #[ignore = "Messung im Release"]
    fn messen() {
        for n in [1000, 4000] {
            let (mut m, gb, runs) = viele(n);
            let p = m.house_strip(gb[0]).unwrap();
            m.begin("am Haus");
            m.add_site_patch(p);
            m.commit();
            let anderes = zuege(&mut m, &runs, n / 2, 7);
            let eigenes = zuege(&mut m, &runs, 0, 7);
            println!(
                "{n} Häuser: fremdes Haus {anderes:.3} ms, Haus des Streifens {eigenes:.3} ms"
            );
        }
    }
}
