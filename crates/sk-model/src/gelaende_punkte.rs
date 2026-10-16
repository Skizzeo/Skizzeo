//! Eigene Höhenpunkte (I4, gelaende/analyse-interaktives-gelaende.md §4.3,
//! §4.3a): Punkte setzen, in Höhe und Lage verschieben, kopieren und
//! löschen. Gerechnet wird im Grundriss (mm, Höhe über ±0,00); ins System
//! des Geländes kommen die Werte auf mm gerundet.
//!
//! ±0,00 bleibt, wo es ist: Bei der Höhenlage „über Gelände“ hängt es am
//! höchsten Punkt am Plattenrand. Ändert ein Punkt diesen, gleicht der
//! Abstand ±0,00 über Gelände des Bezugsgebäudes das aus; das Haus bleibt
//! stehen, das Gelände ändert sich. Ein Punkt wirkt nur auf die Dreiecke,
//! die an ihm hängen (§5 J6 A).
//!
//! Eigene Punkte tragen `hand`; Punkte des Vermessers werden beim Löschen
//! nur aus dem Gelände genommen (`off`), so bleibt das Aufmaß vollständig.

use super::gelaende_eben::rund;
use super::*;

/// Kürzel der Nummern eigener Punkte, wie beim angenommenen Gelände.
const NR: &str = "H";

/// Ein Höhenpunkt im Grundriss.
#[derive(Clone, Debug, PartialEq)]
pub struct TerrainSpot {
    /// Index in [`Terrain::points`].
    pub i: usize,
    /// Lage (mm) und Höhe über ±0,00 (mm).
    pub p: [f64; 3],
    /// Eigener Punkt (sonst vom Vermesser).
    pub hand: bool,
}

impl Model {
    /// Die Punkte im Gelände (ohne die herausgenommenen), im Grundriss;
    /// leer ohne Gelände, Lage oder Höhe von ±0,00.
    pub fn terrain_spots(&self) -> Vec<TerrainSpot> {
        let (Some(t), Some(f), Some(h0)) =
            (self.terrain(), self.survey_frame(), self.terrain_nhn())
        else {
            return Vec::new();
        };
        t.points
            .iter()
            .enumerate()
            .filter(|(_, p)| !p.off)
            .map(|(i, p)| {
                let q = f.to_model(p.e, p.n);
                TerrainSpot {
                    i,
                    p: [q[0], q[1], (p.h - h0) * 1000.0],
                    hand: p.hand,
                }
            })
            .collect()
    }

    /// Punkte verschieben (Index, neue Lage und Höhe in mm; im offenen
    /// Schritt). `false`, wenn sich nichts ändert, ein Index fehlt, ein
    /// Messpunkt dabei ist oder zwei Punkte auf eine Stelle fielen.
    /// Messpunkte bleiben, wie der Vermesser sie gemessen hat; sonst ginge
    /// eine geänderte Höhe ohne Vermerk in die Mengen (VOB H-234c, bis zur
    /// Sperre in I5).
    pub fn move_terrain_points(&mut self, neu: &[(usize, [f64; 3])]) -> bool {
        let Some((mut t, f, h0)) = self.punkte_rahmen() else {
            return false;
        };
        for &(i, p) in neu {
            let Some(q) = t.points.get_mut(i).filter(|q| q.hand) else {
                return false;
            };
            let (e, n) = f.to_survey(p[0], p[1]);
            let (e, n, h) = (rund(e), rund(n), rund(h0 + p[2] / 1000.0));
            // nur was sich bewegt, wird gerundet
            if (q.e - e).abs() >= 5e-4 || (q.n - n).abs() >= 5e-4 {
                q.e = e;
                q.n = n;
            }
            if (q.h - h).abs() >= 5e-4 {
                q.h = h;
            }
        }
        if doppelt(&t) {
            return false;
        }
        self.punkte_schreiben(t)
    }

    /// Eigene Punkte setzen (Lage und Höhe in mm, im offenen Schritt). Hat
    /// das Gelände noch keine Punkte, kommen zuerst die Ecken der ebenen
    /// Platte dazu (I1), so dass nichts springt. Gibt die Indizes der neuen
    /// Punkte zurück; leer, wenn keiner ging (Stelle schon belegt).
    pub fn add_terrain_points(&mut self, neu: &[[f64; 3]]) -> Vec<usize> {
        if neu.is_empty() {
            return Vec::new();
        }
        if self.terrain().is_none_or(|t| t.points.is_empty()) {
            let lage: Vec<[f64; 2]> = neu.iter().map(|p| [p[0], p[1]]).collect();
            self.seed_flat_terrain(&lage);
        }
        let Some((mut t, f, h0)) = self.punkte_rahmen() else {
            return Vec::new();
        };
        let nr = naechste_nummer(&t);
        let start = t.points.len();
        for (k, p) in (nr..).zip(neu) {
            let (e, n) = f.to_survey(p[0], p[1]);
            t.points.push(TerrainPoint {
                nr: format!("{NR}{k}"),
                e: rund(e),
                n: rund(n),
                h: rund(h0 + p[2] / 1000.0),
                code: String::new(),
                off: false,
                hand: true,
            });
        }
        if doppelt(&t) || !self.punkte_schreiben(t) {
            return Vec::new();
        }
        (start..start + neu.len()).collect()
    }

    /// Punkte löschen (im offenen Schritt): eigene fallen weg, Punkte des
    /// Vermessers kommen aus dem Gelände. Es bleiben mindestens drei.
    /// `false`, wenn nichts zu löschen ist oder weniger als drei blieben.
    pub fn remove_terrain_points(&mut self, idx: &[usize]) -> bool {
        let Some((mut t, ..)) = self.punkte_rahmen() else {
            return false;
        };
        let weg: std::collections::BTreeSet<usize> = idx
            .iter()
            .copied()
            .filter(|&i| t.points.get(i).is_some_and(|p| !p.off))
            .collect();
        let bleiben = t.points.iter().filter(|p| !p.off).count() - weg.len();
        if weg.is_empty() || bleiben < 3 {
            return false;
        }
        // Bruchkanten nennen Punkte beim Namen: ein eigener Punkt darin
        // bleibt in der Datei und geht nur aus dem Gelände
        let in_kante = |nr: &str| t.breaks.iter().any(|b| b.pts.iter().any(|q| q == nr));
        for &i in weg.iter().rev() {
            if t.points[i].hand && !in_kante(&t.points[i].nr) {
                t.points.remove(i);
            } else {
                t.points[i].off = true;
            }
        }
        self.punkte_schreiben(t)
    }

    /// Gelände, System und Höhe von ±0,00 zum Ändern der Punkte.
    fn punkte_rahmen(&self) -> Option<(Terrain, SurveyFrame, f64)> {
        let t = self.terrain()?;
        Some((t.clone(), self.survey_frame_of(t)?, self.terrain_nhn()?))
    }

    /// Geänderte Punkte schreiben; ±0,00 bleibt (siehe Kopf), die
    /// Gründungen behalten ihre Einbindetiefe.
    fn punkte_schreiben(&mut self, t: Terrain) -> bool {
        let vorher = self.foundation_terrain();
        let vor = self.terrain_nhn();
        let haus = self.level_building();
        if !self.put_survey(Some(t)) {
            return false;
        }
        let ueber = self.terrain().is_some_and(|t| t.href == HeightRef::High);
        if let (true, Some(v), Some(b), Some(n)) = (ueber, vor, haus, self.terrain_nhn()) {
            let d = (v - n) * 1000.0;
            let off = self.terrain_offset_of(Some(b)) + d;
            if d.abs() > 1e-6 && off.abs() <= MAX_TERRAIN_OFFSET + 1e-9 {
                self.put_terrain(b, (off * 1e6).round() / 1e6);
                self.terrain_nhn_merken();
            }
        }
        self.resettle_foundations(vorher);
        true
    }
}

/// Zwei Punkte im Gelände auf derselben Stelle (unter 1 mm).
fn doppelt(t: &Terrain) -> bool {
    let mut seen = std::collections::HashSet::new();
    !t.points
        .iter()
        .filter(|p| !p.off)
        .all(|p| seen.insert(((p.e * 1000.0).round() as i64, (p.n * 1000.0).round() as i64)))
}

/// Die nächste freie Nummer nach „H“.
fn naechste_nummer(t: &Terrain) -> u64 {
    t.points
        .iter()
        .filter_map(|p| p.nr.strip_prefix(NR)?.parse::<u64>().ok())
        .max()
        .map_or(1, |n| n + 1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use sk_math::vec3;

    /// Gleich bis auf Rechenreste (unter 1 µm).
    fn nah(a: [f64; 3], b: [f64; 3]) -> bool {
        a.iter().zip(b).all(|(x, y)| (x - y).abs() < 1e-3)
    }

    /// Haus 10 × 8 m, OK Gelände 50 cm unter ±0,00, Gelände angenommen.
    fn haus() -> Model {
        let mut m = Model::with_seed(3);
        let b = m.add_building(1);
        let pts = [
            vec3(0.0, 0.0, 0.0),
            vec3(10000.0, 0.0, 0.0),
            vec3(10000.0, 8000.0, 0.0),
            vec3(0.0, 8000.0, 0.0),
        ];
        m.build_from_polygon(b, &pts).unwrap();
        m.begin("Gelände");
        assert!(m.set_terrain_offset_of(b, 500.0));
        m.commit();
        m
    }

    /// Ein Punkt neben dem Haus: ohne Punkte kommen die Plattenecken und
    /// das Rechteck um den Punkt dazu (I1), der neue Punkt liegt in seiner
    /// Höhe, ±0,00 und das Haus bleiben.
    #[test]
    fn punkt_setzen() {
        let mut m = haus();
        let b = m.level_building();
        m.begin("Höhenpunkt");
        let i = m.add_terrain_points(&[[15000.0, 4000.0, -200.0]]);
        m.commit();
        let t = m.terrain().unwrap();
        let n = t.points.len();
        assert_eq!(n, 7, "vier Plattenecken, zwei Ecken außerhalb, der Punkt");
        assert_eq!(i, [n - 1]);
        assert_eq!(t.points[n - 1].nr, "H7");
        assert!(t.points[n - 1].hand && t.assumed());
        let s = m.terrain_spots();
        assert_eq!(s.len(), n);
        assert!(nah(s[n - 1].p, [15000.0, 4000.0, -200.0]), "{:?}", s[n - 1]);
        assert!(m.terrain_nhn().unwrap().abs() < 1e-6);
        assert!((m.terrain_z_at(b, 15000.0, 4000.0) + 200.0).abs() < 1e-6);
        // dieselbe Stelle noch einmal geht nicht
        m.begin("Höhenpunkt");
        assert!(m.add_terrain_points(&[[15000.0, 4000.0, 0.0]]).is_empty());
        m.rollback();
    }

    /// Eine Plattenecke heben: Bei „über Gelände“ bliebe ±0,00 sonst am
    /// höchsten Punkt hängen; der Abstand ±0,00 über Gelände gleicht aus.
    #[test]
    fn hoehe_aendern_haus_bleibt() {
        let mut m = haus();
        let b = m.level_building();
        m.begin("Höhenpunkt");
        assert!(m.seed_flat_terrain(&[]));
        m.commit();
        let s = m.terrain_spots();
        let p = s[0].p;
        assert!((p[2] + 500.0).abs() < 1e-3);
        // im Schritt zurück: alles wie vorher
        m.begin("Höhe");
        assert!(m.move_terrain_points(&[(0, [p[0], p[1], -100.0])]));
        m.rollback();
        assert!((m.terrain_spots()[0].p[2] + 500.0).abs() < 1e-3);
        assert!((m.terrain_offset_of(b) - 500.0).abs() < 1e-6);
        m.begin("Höhe");
        assert!(m.move_terrain_points(&[(0, [p[0], p[1], -100.0])]));
        m.commit();
        assert!(m.terrain_nhn().unwrap().abs() < 1e-6, "±0,00 bleibt");
        let s = m.terrain_spots();
        assert!((s[0].p[2] + 100.0).abs() < 1e-3);
        assert!((s[1].p[2] + 500.0).abs() < 1e-3, "die übrigen bleiben");
        // am Rand der Gründung liegt das Gelände jetzt höher, der Abstand
        // ±0,00 über Gelände wird kleiner
        let v = m.terrain_offset_of(b);
        assert!(v > 100.0 && v < 500.0, "{v}");
        // unverändert ist kein Schritt
        m.begin("Höhe");
        assert!(!m.move_terrain_points(&[(0, s[0].p)]));
        m.rollback();
    }

    /// In der Lage verschieben: Höhe bleibt, auf einen anderen Punkt geht
    /// nicht.
    #[test]
    fn lage_aendern() {
        let mut m = haus();
        m.begin("Höhenpunkt");
        let i = m.add_terrain_points(&[[15000.0, 4000.0, -200.0]])[0];
        m.commit();
        m.begin("Lage");
        assert!(m.move_terrain_points(&[(i, [16000.0, 5000.0, -200.0])]));
        m.commit();
        assert!(nah(m.terrain_spots()[i].p, [16000.0, 5000.0, -200.0]));
        let ecke = m.terrain_spots()[0].p;
        m.begin("Lage");
        assert!(!m.move_terrain_points(&[(i, ecke)]));
        m.rollback();
    }

    /// Löschen: eigene fallen weg, die des Vermessers kommen aus dem
    /// Gelände; drei bleiben.
    #[test]
    fn punkte_loeschen() {
        let mut m = haus();
        m.begin("Höhenpunkt");
        let i = m.add_terrain_points(&[[15000.0, 4000.0, -200.0]])[0];
        m.commit();
        let mut t = m.terrain().unwrap().clone();
        let n = t.points.len();
        t.points[1].hand = false;
        m.begin("Aufmaß");
        assert!(m.set_terrain(Some(t)));
        m.commit();
        let p = m.terrain_spots().into_iter().find(|s| s.i == 1).unwrap().p;
        m.begin("Messpunkt");
        assert!(!m.move_terrain_points(&[(1, [p[0], p[1], p[2] + 100.0])]));
        m.rollback();
        m.begin("Löschen");
        assert!(m.remove_terrain_points(&[i, 1]));
        m.commit();
        let t = m.terrain().unwrap();
        assert_eq!(t.points.len(), n - 1);
        assert!(t.points[1].off && !t.points[1].hand);
        assert_eq!(m.terrain_spots().len(), n - 2);
        m.begin("Löschen");
        let alle: Vec<usize> = m.terrain_spots().iter().map(|s| s.i).collect();
        assert!(!m.remove_terrain_points(&alle[2..]), "drei bleiben");
        assert!(m.remove_terrain_points(&alle[3..]));
        m.rollback();
    }

    /// Ein Aufmaß mit eigenen Punkten bleibt vermessen; Ansatz und
    /// Vorbemerkung sagen, wie viele von Hand ergänzt sind.
    #[test]
    fn mengen_nennen_eigene_punkte() {
        let mut m = haus();
        m.begin("Höhenpunkt");
        assert!(m.seed_flat_terrain(&[]));
        m.commit();
        let mut t = m.terrain().unwrap().clone();
        for p in &mut t.points {
            p.hand = false;
        }
        t.src = "aufmass.csv".into();
        m.begin("Aufmaß");
        assert!(m.set_terrain(Some(t)));
        m.commit();
        let h = crate::qto_earth::netz_herkunft(&m).unwrap();
        assert!(!h.contains("von Hand"), "{h}");
        m.begin("Höhenpunkt");
        assert_eq!(m.add_terrain_points(&[[15000.0, 4000.0, -200.0]]).len(), 1);
        m.commit();
        assert!(!m.terrain().unwrap().assumed());
        let h = crate::qto_earth::netz_herkunft(&m).unwrap();
        assert!(h.ends_with(", davon 1 Punkt von Hand ergänzt (H5)"), "{h}");
        let auto = crate::qto::schedule(&m).auto;
        let netz: Vec<_> = auto
            .iter()
            .filter(|a| a.formula.contains(crate::qto_earth::DREIECKSPRISMEN))
            .collect();
        assert!(!netz.is_empty());
        assert!(netz
            .iter()
            .all(|a| a.formula.contains("davon 1 Punkt von Hand ergänzt")));
    }

    /// Ein eigener Punkt in einer Bruchkante wird beim Löschen nur
    /// ausgeschaltet; er ist kein Messpunkt und zählt nicht als „nicht
    /// verwendet“ (VOB H-234b).
    #[test]
    fn eigener_punkt_in_bruchkante() {
        let mut m = haus();
        m.begin("Höhenpunkt");
        assert!(m.seed_flat_terrain(&[]));
        m.commit();
        let mut t = m.terrain().unwrap().clone();
        for p in &mut t.points {
            p.hand = false;
        }
        t.src = "aufmass.csv".into();
        m.begin("Aufmaß");
        assert!(m.set_terrain(Some(t)));
        m.commit();
        m.begin("Höhenpunkt");
        let i = m.add_terrain_points(&[[15000.0, 4000.0, -200.0]])[0];
        m.commit();
        let mut t = m.terrain().unwrap().clone();
        let kante = vec![t.points[0].nr.clone(), t.points[i].nr.clone()];
        t.breaks.push(crate::TerrainLine {
            pts: kante,
            closed: false,
        });
        m.begin("Kante");
        assert!(m.set_terrain(Some(t)));
        m.commit();
        m.begin("Löschen");
        assert!(m.remove_terrain_points(&[i]));
        m.commit();
        let p = &m.terrain().unwrap().points[i];
        assert!(p.off && p.hand);
        assert_eq!(crate::qto_earth::netz_geaendert(&m), None);
        let h = crate::qto_earth::netz_herkunft(&m).unwrap();
        assert!(
            !h.contains("nicht verwendet") && !h.contains("von Hand"),
            "{h}"
        );
    }

    /// Nummern zählen hinter der höchsten weiter.
    #[test]
    fn nummern() {
        let mut t = Terrain::new(Crs::Local);
        assert_eq!(naechste_nummer(&t), 1);
        for nr in ["H3", "17", "Hx"] {
            t.points.push(TerrainPoint {
                nr: nr.into(),
                e: 0.0,
                n: 0.0,
                h: 0.0,
                code: String::new(),
                off: false,
                hand: true,
            });
        }
        assert_eq!(naechste_nummer(&t), 4);
    }
}
