//! Ein Gelände von Anfang an (I1, gelaende/analyse-interaktives-gelaende.md
//! §4.1): Ohne Aufmaß liegt um jedes Haus die ebene Platte in Höhe OK
//! Gelände. Braucht ein Werkzeug des Geländemoduls ein Netz (die erste
//! Geländefläche), legt Skizzeo daraus ein Gelände mit Punkten von Hand an:
//! die Ecken der Platten in ihrer Höhe, so dass nichts springt (§5 J4 B).
//! Bis dahin steht in der Datei nichts (J3 B: alte Dateien bleiben
//! bytegleich).

use super::*;

/// Erste Nummer der Punkte von Hand; die übrigen zählen weiter („H2“ …).
const NR: &str = "H";

impl Terrain {
    /// Das Gelände hat Punkte, und alle sind von Hand gesetzt: Es ist
    /// angenommen, nicht vermessen. Mengen nennen das im Ansatz.
    pub fn assumed(&self) -> bool {
        !self.points.is_empty() && self.points.iter().all(|p| p.hand)
    }
}

impl Model {
    /// Ein Gelände ohne Punkte (keins oder nur ein Grundstück aus dem
    /// Lageplan) bekommt Punkte von Hand an den Ecken der Platten um die
    /// Häuser, je Haus in seiner Höhe OK Gelände, dazu um `extra` (mm, etwa
    /// die Ecken einer neuen Geländefläche) und die vorhandenen Flächen ein
    /// Rechteck im Abstand der Platte in der Höhe des Bezugsgebäudes (im
    /// offenen Schritt). Ohne
    /// Gelände entsteht eins im örtlichen System, Nullpunkt im Modell.
    /// `false`, wenn schon Punkte da sind oder es keine Ecken gibt.
    pub fn seed_flat_terrain(&mut self, extra: &[[f64; 2]]) -> bool {
        if self.terrain().is_some_and(|t| !t.points.is_empty()) {
            return false;
        }
        let s = self.plate_settings();
        let platten = self.terrain_patches(s.margin);
        let mut ecken: Vec<([f64; 2], f64)> = Vec::new();
        for (r, z) in &platten {
            for p in [[r[0], r[1]], [r[2], r[1]], [r[2], r[3]], [r[0], r[3]]] {
                ecken.push((p, *z));
            }
        }
        // die Geländeflächen, die schon da sind, liegen mit darauf
        let flaechen = self.site_patches().iter().flat_map(|f| f.outline.iter());
        let extra: Vec<[f64; 2]> = extra.iter().chain(flaechen).copied().collect();
        if !extra.is_empty() {
            let e = f64::INFINITY;
            let mut k = [e, e, -e, -e];
            for p in &extra {
                k = [
                    k[0].min(p[0]),
                    k[1].min(p[1]),
                    k[2].max(p[0]),
                    k[3].max(p[1]),
                ];
            }
            let m = s.margin;
            let z = self.terrain_z_of(self.level_building());
            let r = [k[0] - m, k[1] - m, k[2] + m, k[3] + m];
            for p in [[r[0], r[1]], [r[2], r[1]], [r[2], r[3]], [r[0], r[3]]] {
                // in einer Platte gilt deren Höhe
                let drin = platten
                    .iter()
                    .any(|(q, _)| (q[0]..=q[2]).contains(&p[0]) && (q[1]..=q[3]).contains(&p[1]));
                if !drin {
                    ecken.push((p, z));
                }
            }
        }
        let mut gesehen = std::collections::HashSet::new();
        ecken.retain(|(p, _)| gesehen.insert(p.map(f64::to_bits)));
        if ecken.len() < 3 {
            return false;
        }
        let loc = *self.location();
        let mut t = match self.terrain() {
            Some(t) => t.clone(),
            None => {
                let mut t = Terrain::new(Crs::Local);
                t.local = Some(LocalFrame {
                    e0: 0.0,
                    n0: 0.0,
                    rot: 0.0,
                });
                if loc.nhn.is_some() {
                    t.href = HeightRef::Abs;
                }
                t
            }
        };
        let Some(f) = self.survey_frame_of(&t) else {
            return false;
        };
        // ±0,00 bleibt, wo es ist: fest in NHN, sonst bei 0
        let h0 = loc.nhn.unwrap_or(0.0);
        let rund = |v: f64| (v * 1000.0).round() / 1000.0;
        for (i, (p, z)) in ecken.into_iter().enumerate() {
            let (e, n) = f.to_survey(p[0], p[1]);
            t.points.push(TerrainPoint {
                nr: format!("{NR}{}", i + 1),
                e: rund(e),
                n: rund(n),
                h: rund(h0 + z / 1000.0),
                code: String::new(),
                off: false,
                hand: true,
            });
        }
        self.set_terrain(Some(t))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sk_math::vec3;

    /// Haus 10 × 8 m, OK Gelände 50 cm unter ±0,00.
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

    /// Die Ecken der Platte in ihrer Höhe, von Hand; das Gelände liegt dort,
    /// wo die Platte lag, und ±0,00 bleibt. Ein zweites Mal ändert nichts.
    #[test]
    fn ecken_der_platte() {
        let mut m = haus();
        let z = m.terrain_z();
        let rand = m.plate_settings().margin;
        m.begin("Fläche");
        assert!(m.seed_flat_terrain(&[]));
        assert!(!m.seed_flat_terrain(&[]), "schon Punkte");
        m.commit();
        let t = m.terrain().unwrap();
        assert!(t.assumed());
        assert_eq!(t.crs, Crs::Local);
        assert_eq!(t.points.len(), 4);
        assert_eq!(t.points[0].nr, "H1");
        assert!(t.points.iter().all(|p| p.h == -0.5), "{:?}", t.points);
        let e: Vec<f64> = t.points.iter().map(|p| p.e).collect();
        assert_eq!(
            e,
            [
                -rand / 1000.0,
                10.0 + rand / 1000.0,
                10.0 + rand / 1000.0,
                -rand / 1000.0
            ]
        );
        assert_eq!(m.terrain_nhn(), Some(0.0));
        for (x, y) in [(5000.0, 4000.0), (-1000.0, -1000.0), (11000.0, 9000.0)] {
            assert!((m.terrain_z_at(None, x, y) - z).abs() < 1e-6, "{x} {y}");
        }
        let mut m = haus();
        m.begin("Fläche");
        assert!(m.seed_flat_terrain(&[]));
        m.rollback();
        assert!(m.terrain().is_none(), "im Schritt");
    }

    /// Die Erdmengen der Gründung bleiben, wenn das Gelände angelegt wird:
    /// gleiche Werte über das Netz wie über die Ebene; der Ansatz sagt, dass
    /// das Gelände angenommen ist.
    #[test]
    fn mengen_bleiben() {
        let mut m = haus();
        let vorher = crate::qto::schedule(&m).auto;
        m.begin("Fläche");
        assert!(m.seed_flat_terrain(&[]));
        m.commit();
        let nachher = crate::qto::schedule(&m).auto;
        let erde = |v: &[crate::qto::AutoMenge]| -> Vec<(String, f64)> {
            let mut e: Vec<(String, f64)> = v
                .iter()
                .filter(|a| a.key.starts_with("earth."))
                .map(|a| (a.key.to_string(), a.value))
                .collect();
            e.sort_by(|a, b| a.0.cmp(&b.0));
            e
        };
        let (a, b) = (erde(&vorher), erde(&nachher));
        assert!(!a.is_empty());
        assert_eq!(a.len(), b.len(), "{a:?}\n{b:?}");
        for (x, y) in a.iter().zip(&b) {
            assert_eq!(x.0, y.0);
            // mm³: unter 1 dm³
            assert!((x.1 - y.1).abs() < 1e6, "{} {} {}", x.0, x.1, y.1);
        }
        let netz: Vec<&crate::qto::AutoMenge> = nachher
            .iter()
            .filter(|a| a.formula.contains(crate::qto_earth::DREIECKSPRISMEN))
            .collect();
        assert!(!netz.is_empty());
        assert!(netz
            .iter()
            .all(|a| a.formula.contains(crate::qto_earth::ANGENOMMEN)));
        assert_eq!(crate::qto_earth::netz_herkunft(&m), None);
        assert_eq!(crate::qto_earth::netz_angenommen(&m).as_deref(), Some("2"));
    }

    /// Eine Fläche neben dem Haus bekommt ihr Rechteck dazu; Ecken in der
    /// Platte fallen weg. Mit NHN in der Lage stehen die Höhen in NHN.
    #[test]
    fn flaeche_neben_dem_haus() {
        let mut m = haus();
        let mut l = *m.location();
        l.nhn = Some(52.3);
        m.begin("Lage");
        m.put_location(l);
        m.commit();
        let rand = m.plate_settings().margin;
        m.begin("Fläche");
        assert!(m.seed_flat_terrain(&[[2000.0, 2000.0], [30000.0, 4000.0]]));
        m.commit();
        let t = m.terrain().unwrap();
        assert_eq!(t.href, HeightRef::Abs);
        // zwei Ecken des Zusatzrechtecks liegen in der Platte
        assert_eq!(t.points.len(), 6);
        assert!(t.points.iter().all(|p| (p.h - 51.8).abs() < 1e-9));
        assert!(t.points.iter().any(|p| p.e == 30.0 + rand / 1000.0));
        assert_eq!(m.terrain_nhn(), Some(52.3));
    }

    /// Nur ein Grundstück aus dem Lageplan (UTM): Die Punkte kommen in
    /// dessen System dazu, Grenze und Lage bleiben.
    #[test]
    fn zum_grundstueck() {
        let mut m = haus();
        let mut t = Terrain::new(Crs::Local);
        t.local = Some(LocalFrame {
            e0: 1000.0,
            n0: 2000.0,
            rot: 90.0,
        });
        t.parcels = vec![vec![(990.0, 1990.0), (1010.0, 1990.0), (1010.0, 2010.0)]];
        m.begin("Lageplan");
        assert!(m.set_terrain(Some(t)));
        m.commit();
        m.begin("Fläche");
        assert!(m.seed_flat_terrain(&[]));
        m.commit();
        let t = m.terrain().unwrap();
        assert_eq!(t.parcels.len(), 1);
        assert_eq!(t.local.unwrap().e0, 1000.0);
        // Modell-x zeigt nach Nord: Ecke (−rand, −rand) liegt östlich
        let rand = m.plate_settings().margin / 1000.0;
        assert!(
            (t.points[0].e - (1000.0 + rand)).abs() < 1e-9,
            "{:?}",
            t.points[0]
        );
        assert!(
            (t.points[0].n - (2000.0 - rand)).abs() < 1e-9,
            "{:?}",
            t.points[0]
        );
        assert!(t.assumed());
    }

    /// Ohne Haus und ohne Fläche gibt es nichts anzulegen; mit vermessenen
    /// Punkten ist das Gelände nicht angenommen.
    #[test]
    fn nichts_anzulegen() {
        let mut m = Model::with_seed(1);
        m.begin("Fläche");
        assert!(!m.seed_flat_terrain(&[]));
        m.commit();
        assert!(m.terrain().is_none());
        let mut t = Terrain::new(Crs::Local);
        assert!(!t.assumed(), "ohne Punkte");
        t.points.push(TerrainPoint {
            nr: "1".into(),
            e: 0.0,
            n: 0.0,
            h: 1.0,
            code: String::new(),
            off: false,
            hand: false,
        });
        assert!(!t.assumed());
    }
}
