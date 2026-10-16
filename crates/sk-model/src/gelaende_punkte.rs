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
//! Verschiebt man einen Messpunkt (das Schloss dazu ist in der App, I5),
//! merkt er sich die gemessenen Werte in `gemessen`; zurück auf sie (unter
//! 0,5 mm) oder [`Model::reset_terrain_points`] löscht den Vermerk.

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
    /// Ein Messpunkt, der nicht mehr an seiner gemessenen Stelle steht.
    pub geaendert: bool,
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
                    geaendert: p.gemessen.is_some(),
                }
            })
            .collect()
    }

    /// Punkte verschieben (Index, neue Lage und Höhe in mm; im offenen
    /// Schritt). `false`, wenn sich nichts ändert, ein Index fehlt oder zwei
    /// Punkte auf eine Stelle fielen. Ein Messpunkt merkt sich beim ersten
    /// Mal die gemessenen Werte (VOB-Spruch zu I5: das Modell, nicht die
    /// App); kommt er auf sie zurück, entfällt der Vermerk. Ob er sich
    /// ziehen lässt, entscheidet das Schloss in der App.
    pub fn move_terrain_points(&mut self, neu: &[(usize, [f64; 3])]) -> bool {
        let Some((mut t, f, h0)) = self.punkte_rahmen() else {
            return false;
        };
        let mut seitlich = Vec::new();
        for &(i, p) in neu {
            let Some(q) = t.points.get_mut(i) else {
                return false;
            };
            let vorher = [q.e, q.n, q.h];
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
            if !q.hand {
                let g = q.gemessen.unwrap_or(vorher);
                let zurueck = (0..3).all(|k| ([q.e, q.n, q.h][k] - g[k]).abs() < 5e-4);
                // zurück auf der gemessenen Stelle gelten die Werte des
                // Vermessers wieder ungerundet
                if zurueck {
                    [q.e, q.n, q.h] = g;
                }
                q.gemessen = (!zurueck).then_some(g);
            }
            if [q.e, q.n] != [vorher[0], vorher[1]] {
                seitlich.push(i);
            }
        }
        if doppelt_bei(&t, &seitlich) {
            return false;
        }
        let geaendert: Vec<usize> = neu.iter().map(|x| x.0).collect();
        self.punkte_schreiben(t, Some(&geaendert))
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
                gemessen: None,
            });
        }
        let neu: Vec<usize> = (start..t.points.len()).collect();
        if doppelt_bei(&t, &neu) || !self.punkte_schreiben(t, None) {
            return Vec::new();
        }
        neu
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
        self.punkte_schreiben(t, None)
    }

    /// Geänderte Messpunkte auf die gemessenen Werte zurück (im offenen
    /// Schritt); `false`, wenn keiner der Indizes geändert ist.
    pub fn reset_terrain_points(&mut self, idx: &[usize]) -> bool {
        let Some((mut t, ..)) = self.punkte_rahmen() else {
            return false;
        };
        let (mut neu, mut seitlich) = (Vec::new(), Vec::new());
        for &i in idx {
            if let Some(q) = t.points.get_mut(i) {
                if let Some([e, n, h]) = q.gemessen.take() {
                    if [q.e, q.n] != [e, n] {
                        seitlich.push(i);
                    }
                    (q.e, q.n, q.h) = (e, n, h);
                    neu.push(i);
                }
            }
        }
        if neu.is_empty() || doppelt_bei(&t, &seitlich) {
            return false;
        }
        self.punkte_schreiben(t, Some(&neu))
    }

    /// Gelände, System und Höhe von ±0,00 zum Ändern der Punkte.
    fn punkte_rahmen(&self) -> Option<(Terrain, SurveyFrame, f64)> {
        let t = self.terrain()?;
        Some((t.clone(), self.survey_frame_of(t)?, self.terrain_nhn()?))
    }

    /// Geänderte Punkte schreiben; ±0,00 bleibt (siehe Kopf), die
    /// Gründungen behalten ihre Einbindetiefe. Mit `geaendert` hat sich
    /// nur Lage und Höhe dieser Punkte geändert
    /// ([`Model::put_survey_punkte`]).
    fn punkte_schreiben(&mut self, t: Terrain, geaendert: Option<&[usize]>) -> bool {
        let vorher = self.foundation_terrain();
        let vor = self.terrain_nhn();
        let haus = self.level_building();
        let neu = match geaendert {
            Some(g) => self.put_survey_punkte(t, g),
            None => self.put_survey(Some(t)),
        };
        if !neu {
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

/// Ein Punkt aus `bewegt` auf derselben Stelle (unter 1 mm) wie ein
/// anderer im Gelände; abgewählte zählen nicht. Nur die bewegten gegen
/// alle, ohne Gang über alle Paare (Review #234 B-234a).
fn doppelt_bei(t: &Terrain, bewegt: &[usize]) -> bool {
    use std::collections::HashSet;
    let stelle = |p: &TerrainPoint| ((p.e * 1000.0).round() as i64, (p.n * 1000.0).round() as i64);
    let mut stellen = HashSet::new();
    let mut eigen = HashSet::new();
    for &i in bewegt {
        let p = &t.points[i];
        if !p.off && eigen.insert(i) && !stellen.insert(stelle(p)) {
            return true;
        }
    }
    let Some(&s0) = stellen.iter().next() else {
        return false;
    };
    let (lo, hi) = stellen.iter().fold((s0, s0), |(lo, hi), &(x, y)| {
        ((lo.0.min(x), lo.1.min(y)), (hi.0.max(x), hi.1.max(y)))
    });
    t.points.iter().enumerate().any(|(j, q)| {
        let s = stelle(q);
        !q.off
            && (lo.0..=hi.0).contains(&s.0)
            && (lo.1..=hi.1).contains(&s.1)
            && !eigen.contains(&j)
            && stellen.contains(&s)
    })
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

    /// Ein Messpunkt merkt sich beim Verschieben die gemessenen Werte
    /// (I5); zurück auf sie oder „gemessen zurück“ löscht den Vermerk. Die
    /// Vorbemerkung nennt alte und neue Höhe und den Versatz, Ansatz und
    /// Befund zählen ihn (VOB-Spruch zu I5); die Datei trägt ihn ungerundet.
    #[test]
    fn messpunkt_mit_schloss() {
        let mut m = haus();
        m.begin("Höhenpunkt");
        assert!(m.seed_flat_terrain(&[]));
        m.commit();
        let mut t = m.terrain().unwrap().clone();
        for p in &mut t.points {
            p.hand = false;
        }
        t.points[1].h += 0.0004321;
        t.src = "aufmass.csv".into();
        m.begin("Aufmaß");
        assert!(m.set_terrain(Some(t)));
        m.commit();
        let alt = m.terrain().unwrap().points[1].clone();
        let p = m.terrain_spots().into_iter().find(|s| s.i == 1).unwrap().p;
        m.begin("Messpunkt");
        assert!(m.move_terrain_points(&[(1, [p[0] + 350.0, p[1], p[2] - 242.0])]));
        m.commit();
        let q = &m.terrain().unwrap().points[1];
        assert_eq!(q.gemessen, Some([alt.e, alt.n, alt.h]), "ungerundet");
        assert!(m.terrain_spots().iter().any(|s| s.i == 1 && s.geaendert));
        assert_eq!(crate::qto_earth::netz_geaendert(&m), Some((0, 1, 0)));
        let h = crate::qto_earth::netz_herkunft(&m).unwrap();
        let soll = format!(
            ", davon 1 Messpunkt geändert ({} von {} auf {} m, Lage um 0,35 m versetzt)",
            alt.nr,
            format!("{:.3}", alt.h).replace('.', ","),
            format!("{:.3}", q.h).replace('.', ",")
        );
        assert!(h.ends_with(&soll), "{h}\n{soll}");
        let satz = crate::qto_earth::hand_geaendert((0, 1, 0));
        assert!(satz.contains("(0 ergänzt, 1 geändert, 0 nicht verwendet)"));
        let auto = crate::qto::schedule(&m).auto;
        assert!(auto
            .iter()
            .filter(|a| a.formula.contains(crate::qto_earth::DREIECKSPRISMEN))
            .all(|a| a.formula.contains("davon 1 Messpunkt geändert")
                && !a.formula.contains("von Hand")));
        // die Datei trägt die gemessenen Werte und liest sie wieder
        let text = crate::szo::write(&m);
        let neu = crate::szo::read(&text, crate::GuidGen::with_seed(9))
            .unwrap()
            .model;
        assert_eq!(neu.terrain().unwrap().points[1].gemessen, q.gemessen);
        // eine Kopie ist ein eigener Punkt ohne Vermerk
        m.begin("Kopie");
        let k = m.add_terrain_points(&[[p[0] + 2000.0, p[1], p[2]]])[0];
        m.commit();
        assert_eq!(m.terrain().unwrap().points[k].gemessen, None);
        // zurück auf die gemessene Stelle löscht den Vermerk
        m.begin("Zurück");
        assert!(m.move_terrain_points(&[(1, p)]));
        m.commit();
        let q = &m.terrain().unwrap().points[1];
        assert_eq!((q.e, q.n, q.h, q.gemessen), (alt.e, alt.n, alt.h, None));
        // „gemessen zurück“ holt die ungerundeten Werte
        m.begin("Messpunkt");
        assert!(m.move_terrain_points(&[(1, [p[0], p[1], p[2] + 500.0])]));
        m.commit();
        m.begin("Gemessen zurück");
        assert!(m.reset_terrain_points(&[1, 0]));
        m.commit();
        let q = &m.terrain().unwrap().points[1];
        assert_eq!((q.e, q.n, q.h, q.gemessen), (alt.e, alt.n, alt.h, None));
        m.begin("Nichts");
        assert!(!m.reset_terrain_points(&[1]), "nichts mehr zurückzuholen");
        m.rollback();
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
                gemessen: None,
            });
        }
        assert_eq!(naechste_nummer(&t), 4);
    }
}
