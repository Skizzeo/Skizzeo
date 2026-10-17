//! Ein Gelände von Anfang an (I1, gelaende/analyse-interaktives-gelaende.md
//! §4.1): Ohne Aufmaß liegt um jedes Haus die ebene Platte in Höhe OK
//! Gelände. Braucht ein Werkzeug des Geländemoduls ein Netz (die erste
//! Geländefläche), legt Skizzeo daraus ein Gelände mit Punkten von Hand an:
//! die Ecken der Platten in ihrer Höhe, so dass nichts springt (§5 J4 B).
//! Bis dahin steht in der Datei nichts (J3 B: alte Dateien bleiben
//! bytegleich).
//!
//! Die Grenze geht auch ohne Lageplan: von Hand gezeichnet (I2,
//! [`Model::add_parcel`]) liegt sie wie ein Flurstück im Gelände. Ein
//! angenommenes Gelände reicht dann bis über sie hinaus (I6, Jörn 17.10.):
//! Die ebene Platte wächst bis zur Grenze und um den Plattenabstand
//! darüber, die Umgebung außerhalb zeigt sie ganz, blass oder gar nicht.

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
        let in_platte = |q: &[f64; 4], p: [f64; 2]| {
            (q[0]..=q[2]).contains(&p[0]) && (q[1]..=q[3]).contains(&p[1])
        };
        // je Ecke ihre Höhe und ihre Platte; eine Ecke in oder auf einer
        // früheren Platte fällt weg, dort gilt die erste (wie bei
        // `terrain_patches`)
        let mut ecken: Vec<([f64; 2], f64, Option<usize>)> = Vec::new();
        for (i, (r, z)) in platten.iter().enumerate() {
            for p in [[r[0], r[1]], [r[2], r[1]], [r[2], r[3]], [r[0], r[3]]] {
                if !platten[..i].iter().any(|(q, _)| in_platte(q, p)) {
                    ecken.push((p, *z, Some(i)));
                }
            }
        }
        // die Geländeflächen, die schon da sind, und das Grundstück liegen
        // mit darauf (I2)
        let flaechen = self.site_patches().iter().flat_map(|f| f.outline.iter());
        let grenze: Vec<[f64; 2]> = match self.terrain() {
            Some(t) => match self.survey_frame_of(t) {
                Some(f) => t
                    .boundary_rings()
                    .iter()
                    .flatten()
                    .map(|&(e, n)| f.to_model(e, n))
                    .collect(),
                None => Vec::new(),
            },
            None => Vec::new(),
        };
        let extra: Vec<[f64; 2]> = extra
            .iter()
            .chain(flaechen)
            .chain(&grenze)
            .copied()
            .collect();
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
                if !platten.iter().any(|(q, _)| in_platte(q, p)) {
                    ecken.push((p, z, None));
                }
            }
        }
        let mut gesehen = std::collections::HashSet::new();
        ecken.retain(|(p, ..)| gesehen.insert(p.map(f64::to_bits)));
        if ecken.len() < 3 {
            return false;
        }
        let mut t = self.terrain().cloned().unwrap_or_else(|| self.oertlich());
        let Some(f) = self.survey_frame_of(&t) else {
            return false;
        };
        // ±0,00 bleibt, wo es ist: fest in NHN, sonst bei 0
        let h0 = self.location().nhn.unwrap_or(0.0);
        // Die Ränder ganzer Platten werden Bruchkanten: Sonst verbindet das
        // Netz quer durch eine Platte mit der Ecke eines anderen Hauses in
        // anderer Höhe, und es springt doch (J4 B; Geometrie, Routine
        // 16.10. 19:03). Platten mit Ecken in einer früheren bekommen
        // keine, die eine frühere berühren: Ihr Rand kreuzte deren.
        let ganz = |i: usize| {
            let r = platten[i].0;
            !platten[..i]
                .iter()
                .any(|(q, _)| r[0] <= q[2] && q[0] <= r[2] && r[1] <= q[3] && q[1] <= r[3])
        };
        let mut ringe: Vec<Vec<String>> = vec![Vec::new(); platten.len()];
        for (i, (p, z, platte)) in ecken.iter().enumerate() {
            let nr = format!("{NR}{}", i + 1);
            if let Some(k) = platte.filter(|&k| ganz(k)) {
                ringe[k].push(nr.clone());
            }
            let (e, n) = f.to_survey(p[0], p[1]);
            t.points.push(TerrainPoint {
                nr,
                e: rund(e),
                n: rund(n),
                h: rund(h0 + z / 1000.0),
                code: String::new(),
                off: false,
                hand: true,
                gemessen: None,
            });
        }
        t.breaks.extend(
            ringe
                .into_iter()
                .filter(|r| r.len() == 4)
                .map(|pts| TerrainLine { pts, closed: true }),
        );
        self.set_terrain(Some(t))
    }

    /// Eine von Hand gezeichnete Grenze (I2): `ring` sind ihre Ecken im
    /// Grundriss (mm). Sie kommt als Flurstück zu den Grenzen des Geländes,
    /// in dessen System auf mm gerundet; ohne Gelände entsteht eins im
    /// örtlichen System wie bei [`Model::seed_flat_terrain`]. Das Gelände
    /// reicht danach bis über die Grenze ([`Model::grenze_abdecken`], im
    /// offenen Schritt). `false` unter drei Ecken, ohne Fläche oder ohne
    /// Lage des Systems.
    pub fn add_parcel(&mut self, ring: &[[f64; 2]]) -> bool {
        if !super::gelaende_grenze::parcel_ring_ok(ring) {
            return false;
        }
        let mut t = self.terrain().cloned().unwrap_or_else(|| self.oertlich());
        let Some(f) = self.survey_frame_of(&t) else {
            return false;
        };
        let r: Vec<(f64, f64)> = ring
            .iter()
            .map(|p| {
                let (e, n) = f.to_survey(p[0], p[1]);
                (rund(e), rund(n))
            })
            .collect();
        t.parcels.push(r);
        let neu = self.set_terrain(Some(t));
        if neu {
            self.grenze_abdecken();
        }
        neu
    }

    /// Nach einer gezeichneten oder geänderten Grenze (I6, Jörn 17.10.):
    /// Ein angenommenes Gelände reicht bis um den Plattenabstand über alle
    /// Grenzen hinaus. Ohne Punkte tut es das schon (die App zeigt dann das
    /// Rechteck um die Grenzen eben); mit Punkten von Hand kommen die Ecken
    /// dieses Rechtecks dazu, die das Netz noch nicht deckt, in der Höhe,
    /// die das Netz an der Platte des Bezugsgebäudes zeigt (Kosten H-251a:
    /// so wie bei Grenze vor Fläche). Der bisherige Rand des Netzes wird
    /// Bruchkante: Innen bleibt jedes Dreieck, wie es war, auch zwischen
    /// Häusern in verschiedener Höhe (VOB B-251a). Ein Aufmaß des
    /// Vermessers bleibt, wie es ist. Ohne Lageplan zeigt das Gelände die
    /// Umgebung außerhalb der Grenze ganz, bis man „blass“ oder „nicht“
    /// wählt.
    pub(super) fn grenze_abdecken(&mut self) {
        let Some(t) = self.terrain() else {
            return;
        };
        let gezeichnet = t.underlay.is_none() && t.show.is_none();
        if t.assumed() {
            let ecken: Vec<[f64; 2]> = self.parcel_rings().into_iter().flatten().collect();
            if ecken.is_empty() {
                return;
            }
            let e = f64::INFINITY;
            let mut k = [e, e, -e, -e];
            for p in &ecken {
                k = [
                    k[0].min(p[0]),
                    k[1].min(p[1]),
                    k[2].max(p[0]),
                    k[3].max(p[1]),
                ];
            }
            let m = self.plate_settings().margin;
            let r = [k[0] - m, k[1] - m, k[2] + m, k[3] + m];
            let z = self.hoehe_an_der_platte();
            // das Netz deckt die konvexe Hülle seiner Punkte
            let spots = self.terrain_spots();
            let punkte: Vec<Vec3> = spots.iter().map(|s| vec3(s.p[0], s.p[1], 0.0)).collect();
            let huelle = crate::qto_site::convex_hull(&punkte);
            let neu: Vec<[f64; 3]> = [[r[0], r[1]], [r[2], r[1]], [r[2], r[3]], [r[0], r[3]]]
                .into_iter()
                .filter(|p| !in_huelle(&huelle, *p))
                .map(|p| [p[0], p[1], z])
                .collect();
            if !neu.is_empty() {
                let rand: Vec<String> = huelle_ring(&huelle, &spots)
                    .into_iter()
                    .filter_map(|i| t.points.get(i).map(|p| p.nr.clone()))
                    .collect();
                self.add_terrain_points(&neu);
                self.rand_als_bruchkante(rand);
            }
        } else if !t.points.is_empty() {
            return;
        }
        if gezeichnet {
            self.set_terrain_show(Show::All);
        }
    }

    /// OK Gelände (mm), wie das Netz es in der Mitte des Bezugsgebäudes
    /// zeigt, also auf seiner Platte; ohne Netz dort sein Versatz.
    fn hoehe_an_der_platte(&self) -> f64 {
        let b = self.level_building();
        let e = f64::INFINITY;
        let mut k = [e, e, -e, -e];
        for (_, r) in self.runs.iter() {
            if b.is_none() || self.building_of(r.storey) != b {
                continue;
            }
            for p in &r.points {
                k = [k[0].min(p.x), k[1].min(p.y), k[2].max(p.x), k[3].max(p.y)];
            }
        }
        (k[0] <= k[2])
            .then(|| self.terrain_z_survey((k[0] + k[2]) / 2.0, (k[1] + k[3]) / 2.0))
            .flatten()
            .unwrap_or_else(|| self.terrain_z_of(b))
    }

    /// Der Ring `rand` (Nummern der Punkte) als geschlossene Bruchkante,
    /// wenn er nicht schon eine ist (die Platte eines einzelnen Hauses).
    fn rand_als_bruchkante(&mut self, rand: Vec<String>) {
        if rand.len() < 3 {
            return;
        }
        let Some(mut t) = self.terrain().cloned() else {
            return;
        };
        let sortiert = |v: &[String]| {
            let mut v = v.to_vec();
            v.sort();
            v
        };
        let s = sortiert(&rand);
        if t.breaks.iter().any(|b| b.closed && sortiert(&b.pts) == s) {
            return;
        }
        t.breaks.push(TerrainLine {
            pts: rand,
            closed: true,
        });
        self.set_terrain(Some(t));
    }

    /// Die Grenzen eines Geländes ohne Lageplan, also von Hand gezeichnet
    /// (I2), im Grundriss (mm).
    pub(super) fn gezeichnete_grenzen(&self) -> Vec<Vec<[f64; 2]>> {
        let Some(t) = self.terrain().filter(|t| t.underlay.is_none()) else {
            return Vec::new();
        };
        let Some(f) = self.survey_frame_of(t) else {
            return Vec::new();
        };
        t.parcels
            .iter()
            .map(|r| r.iter().map(|&(e, n)| f.to_model(e, n)).collect())
            .collect()
    }

    /// Ein neues Aufmaß ohne eigene Grenze übernimmt die gezeichneten
    /// `grenzen` (mm) dort, wo sie im Grundriss lagen, in seinem System;
    /// `t` hat seinen Platz schon. Sonst bliebe die Grenze, die man vor dem
    /// Vermessen gezeichnet hat, nicht stehen.
    pub(super) fn grenzen_mitnehmen(&self, t: &mut Terrain, grenzen: &[Vec<[f64; 2]>]) {
        if grenzen.is_empty() || !t.boundary_rings().is_empty() {
            return;
        }
        let Some(f) = self.survey_frame_of(t) else {
            return;
        };
        t.parcels = grenzen
            .iter()
            .map(|r| {
                r.iter()
                    .map(|p| {
                        let (e, n) = f.to_survey(p[0], p[1]);
                        (rund(e), rund(n))
                    })
                    .collect()
            })
            .collect();
    }

    /// Ein leeres Gelände im örtlichen System, Nullpunkt im Modell, Nord
    /// nach oben; mit fester Höhe in der Lage in NHN.
    fn oertlich(&self) -> Terrain {
        let mut t = Terrain::new(Crs::Local);
        t.local = Some(LocalFrame {
            e0: 0.0,
            n0: 0.0,
            rot: 0.0,
        });
        if self.location().nhn.is_some() {
            t.href = HeightRef::Abs;
        }
        t
    }
}

/// Liegt `p` in der konvexen Hülle `h` (gegen den Uhrzeigersinn), auf dem
/// Rand eingeschlossen (1 mm)?
fn in_huelle(h: &[Vec3], p: [f64; 2]) -> bool {
    let n = h.len();
    n >= 3
        && (0..n).all(|i| {
            let (a, b) = (h[i], h[(i + 1) % n]);
            let l = (b.x - a.x).hypot(b.y - a.y);
            (b.x - a.x) * (p[1] - a.y) - (b.y - a.y) * (p[0] - a.x) >= -l
        })
}

/// Der Rand der Hülle `h` als Ring von Punkten (Indizes in
/// [`Terrain::points`]): ihre Ecken und die Punkte, die auf ihren Kanten
/// liegen (bis 0,5 mm), damit die Bruchkante keinen Punkt überspringt.
fn huelle_ring(h: &[Vec3], spots: &[TerrainSpot]) -> Vec<usize> {
    let n = h.len();
    if n < 3 {
        return Vec::new();
    }
    let mut ring: Vec<usize> = Vec::new();
    let mut lage: Vec<[f64; 2]> = Vec::new();
    for k in 0..n {
        let (a, b) = (h[k], h[(k + 1) % n]);
        let (dx, dy) = (b.x - a.x, b.y - a.y);
        let l2 = dx * dx + dy * dy;
        if l2 < 1e-12 {
            continue;
        }
        let mut auf: Vec<(f64, usize, [f64; 2])> = spots
            .iter()
            .filter_map(|s| {
                let (px, py) = (s.p[0] - a.x, s.p[1] - a.y);
                let t = (px * dx + py * dy) / l2;
                let quer = (px * dy - py * dx).abs() / l2.sqrt();
                ((0.0..1.0).contains(&t) && quer <= 0.5).then_some((t, s.i, [s.p[0], s.p[1]]))
            })
            .collect();
        auf.sort_by(|x, y| x.0.total_cmp(&y.0));
        for (_, i, p) in auf {
            // je Stelle ein Punkt
            if !lage.iter().any(|q| (q[0] - p[0]).hypot(q[1] - p[1]) <= 0.5) {
                ring.push(i);
                lage.push(p);
            }
        }
    }
    ring
}

/// Auf mm.
pub(super) fn rund(v: f64) -> f64 {
    (v * 1000.0).round() / 1000.0
}

/// Inhalt eines Umrisses (mm²).
pub(super) fn flaeche(ring: &[[f64; 2]]) -> f64 {
    let n = ring.len();
    let s: f64 = (0..n)
        .map(|i| {
            let (a, b) = (ring[i], ring[(i + 1) % n]);
            a[0] * b[1] - b[0] * a[1]
        })
        .sum();
    s.abs() / 2.0
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
        let erde = |v: &[crate::qto::AutoMenge]| -> Vec<(String, &str, f64)> {
            let mut e: Vec<(String, &str, f64)> = v
                .iter()
                .filter(|a| a.key.starts_with("earth."))
                .map(|a| (a.key.to_string(), a.unit, a.value))
                .collect();
            e.sort_by(|a, b| a.0.cmp(&b.0));
            e
        };
        let (a, b) = (erde(&vorher), erde(&nachher));
        assert!(!a.is_empty());
        assert_eq!(a.len(), b.len(), "{a:?}\n{b:?}");
        for (x, y) in a.iter().zip(&b) {
            assert_eq!((&x.0, x.1), (&y.0, y.1));
            // je Einheit (VOB-Hinweis c zu #229): unter 1 dm³ bzw. 1 dm²
            let tol = match x.1 {
                "m3" => 1e6,
                "m2" => 1e4,
                "m" => 1e2,
                _ => 1e-3,
            };
            assert!((x.2 - y.2).abs() < tol, "{} {} {} {}", x.0, x.1, x.2, y.2);
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

    /// Langes Haus und Schuppen 1 m tiefer, die Platten 6 m auseinander:
    /// Jede Platte bleibt in ihrer Höhe, auch wo die Ecke des anderen
    /// Hauses im Umkreis ihrer Diagonale liegt (ohne Bruchkanten bis
    /// 0,35 m im Haus), und die Erdmengen bleiben (Geometrie, Routine
    /// 16.10. 19:03).
    #[test]
    fn zwei_haeuser_springen_nicht() {
        let mut m = Model::with_seed(3);
        let neu = |m: &mut Model, x0: f64, y0: f64, w: f64, h: f64, versatz: f64| {
            let b = m.add_building(1);
            let pts = [
                vec3(x0, y0, 0.0),
                vec3(x0 + w, y0, 0.0),
                vec3(x0 + w, y0 + h, 0.0),
                vec3(x0, y0 + h, 0.0),
            ];
            m.build_from_polygon(b, &pts).unwrap();
            m.begin("Gelände");
            assert!(m.set_terrain_offset_of(b, versatz));
            m.commit();
            b
        };
        let haus = neu(&mut m, 0.0, 0.0, 30000.0, 6000.0, 500.0);
        let schuppen = neu(&mut m, 13000.0, 18000.0, 4000.0, 3000.0, 1500.0);
        let platten = m.terrain_patches(m.plate_settings().margin);
        let vorher = crate::qto::schedule(&m).auto;
        m.begin("Fläche");
        assert!(m.seed_flat_terrain(&[]));
        m.commit();
        assert_eq!(m.terrain().unwrap().breaks.len(), 2);
        for (r, z) in &platten {
            let b = if *z == m.terrain_z_of(Some(haus)) {
                haus
            } else {
                schuppen
            };
            for i in 0..=20 {
                for j in 0..=20 {
                    let x = r[0] + (r[2] - r[0]) * i as f64 / 20.0;
                    let y = r[1] + (r[3] - r[1]) * j as f64 / 20.0;
                    let d = m.terrain_z_at(Some(b), x, y) - z;
                    assert!(d.abs() < 1e-6, "{x} {y}: {d}");
                }
            }
        }
        let nachher = crate::qto::schedule(&m).auto;
        let erde = |v: &[crate::qto::AutoMenge]| -> Vec<(String, f64)> {
            let mut e: Vec<(String, f64)> = v
                .iter()
                .filter(|a| a.key.starts_with("earth.") && a.unit == "m3")
                .map(|a| (a.key.to_string(), a.value))
                .collect();
            e.sort_by(|a, b| a.0.cmp(&b.0));
            e
        };
        let (a, b) = (erde(&vorher), erde(&nachher));
        assert!(!a.is_empty());
        assert_eq!(a.len(), b.len(), "{a:?}\n{b:?}");
        for (x, y) in a.iter().zip(&b) {
            assert!((x.1 - y.1).abs() < 1e6, "{} {} {}", x.0, x.1, y.1);
        }
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

    /// Eine gezeichnete Grenze ohne Gelände: ein örtliches Gelände nur mit
    /// ihr, ohne Punkte, die Platte bleibt eben. Eine zweite kommt dazu;
    /// zu wenig Ecken oder keine Fläche ändern nichts.
    #[test]
    fn grenze_gezeichnet() {
        let mut m = haus();
        let z = m.terrain_z();
        let ring = [
            [-5000.0, -4000.0],
            [20000.0, -4000.0],
            [20000.0, 16000.0],
            [-5000.0, 16000.0],
        ];
        m.begin("Grenze");
        assert!(!m.add_parcel(&ring[..2]));
        assert!(!m.add_parcel(&[[0.0, 0.0], [5000.0, 0.0], [10000.0, 0.0]]));
        assert!(m.terrain().is_none());
        assert!(m.add_parcel(&ring));
        m.commit();
        let t = m.terrain().unwrap();
        assert_eq!(t.crs, Crs::Local);
        assert!(t.points.is_empty() && !t.assumed());
        // die Umgebung um die gezeichnete Grenze ganz (I6)
        assert_eq!(t.show(), Show::All);
        assert_eq!(
            t.parcels,
            vec![vec![(-5.0, -4.0), (20.0, -4.0), (20.0, 16.0), (-5.0, 16.0)]]
        );
        assert!((t.parcel_area() - 500.0).abs() < 1e-9);
        let b = m.level_building();
        assert_eq!(m.terrain_z(), z);
        // ohne Punkte die ebene Platte am Haus
        assert_eq!(m.terrain_z_at(b, 5000.0, 4000.0), z);
        m.begin("Grenze");
        assert!(m.add_parcel(&[[30000.0, 0.0], [40000.0, 0.0], [40000.0, 10000.0]]));
        m.commit();
        assert_eq!(m.terrain().unwrap().parcels.len(), 2);
        m.begin("Grenze");
        assert!(m.add_parcel(&[[0.0, 0.0], [2000.0, 0.0], [0.0, 2000.0]]));
        m.rollback();
        assert_eq!(m.terrain().unwrap().parcels.len(), 2, "im Schritt");
    }

    /// Steht schon ein angenommenes Gelände (Punkte von Hand um das Haus),
    /// reicht es nach einer größeren Grenze bis um den Plattenabstand über
    /// sie hinaus, eben in der Höhe des Hauses (I6, Jörn 17.10.). Eine
    /// gezogene Grenze nimmt es mit; „nicht“ bleibt gewählt.
    #[test]
    fn platte_waechst_bis_zur_grenze() {
        let mut m = haus();
        let z = m.terrain_z();
        m.begin("Fläche");
        assert!(m.seed_flat_terrain(&[]));
        m.commit();
        let ring = [
            [-20000.0, -15000.0],
            [35000.0, -15000.0],
            [35000.0, 30000.0],
            [-20000.0, 30000.0],
        ];
        let rand = m.plate_settings().margin;
        // Die Höhenabfrage hält sich außen an den Netzrand, darum zählt die
        // Ausdehnung der Punkte.
        let bis = |m: &Model| {
            let s = m.terrain_spots();
            let x = s.iter().map(|p| p.p[0]);
            let y = s.iter().map(|p| p.p[1]);
            [
                x.clone().fold(f64::MAX, f64::min),
                y.clone().fold(f64::MAX, f64::min),
                x.fold(f64::MIN, f64::max),
                y.fold(f64::MIN, f64::max),
            ]
        };
        let n0 = m.terrain().unwrap().points.len();
        assert!(bis(&m)[0] > -20000.0, "vorher {:?}", bis(&m));
        m.begin("Grenze");
        assert!(m.add_parcel(&ring));
        m.commit();
        let t = m.terrain().unwrap();
        assert!(t.assumed());
        assert_eq!(t.show(), Show::All);
        for p in ring {
            for q in [[p[0] - rand * 0.99, p[1]], [p[0] + rand * 0.99, p[1]], p] {
                let h = m.terrain_z_survey(q[0], q[1]);
                assert!(h.is_some_and(|h| (h - z).abs() < 1.0), "{q:?} {h:?}");
            }
        }
        assert!(t.points.len() > n0);
        let b = bis(&m);
        assert!(
            b[0] <= -20000.0 - rand * 0.99 && b[1] <= -15000.0 - rand * 0.99,
            "{b:?}"
        );
        assert!(
            b[2] >= 35000.0 + rand * 0.99 && b[3] >= 30000.0 + rand * 0.99,
            "{b:?}"
        );
        // am Haus bleibt es, wo es war
        assert!((m.terrain_z_at(m.level_building(), 5000.0, -1000.0) - z).abs() < 1.0);
        let n = m.terrain().unwrap().points.len();
        // „nicht“ gewählt, dann eine Ecke weit hinaus gezogen
        m.begin("Zuschnitt");
        assert!(m.set_terrain_show(Show::Parcel));
        m.commit();
        let mut weiter = ring;
        weiter[2] = [60000.0, 50000.0];
        m.begin("Ecke");
        assert!(m.set_parcel_ring(0, &weiter));
        m.commit();
        let t = m.terrain().unwrap();
        assert_eq!(t.show(), Show::Parcel, "die Wahl bleibt");
        assert!(t.points.len() > n);
        let b = bis(&m);
        assert!(
            b[2] >= 60000.0 + rand * 0.99 && b[3] >= 50000.0 + rand * 0.99,
            "{b:?}"
        );
        let h = m.terrain_z_survey(60000.0 + rand * 0.99, 50000.0 + rand * 0.99);
        assert!(h.is_some_and(|h| (h - z).abs() < 1.0), "{h:?}");
        // innen genügt: nichts kommt dazu
        let n = t.points.len();
        let mut innen = weiter;
        innen[0] = [-19000.0, -14000.0];
        m.begin("Ecke");
        assert!(m.set_parcel_ring(0, &innen));
        m.commit();
        assert_eq!(m.terrain().unwrap().points.len(), n);
    }

    /// Zwei Häuser in verschiedener Höhe, dann eine Grenze dicht um sie
    /// (VOB B-251a): Innerhalb des bisherigen Netzrands bleibt jede Höhe,
    /// die neuen Ecken liegen so hoch, wie das Netz an der Platte des
    /// Bezugsgebäudes steht (Kosten H-251a), und ein zweites Mal kommt
    /// keine Bruchkante doppelt dazu.
    #[test]
    fn grenze_aendert_innen_nichts() {
        let mut m = Model::with_seed(3);
        let neu = |m: &mut Model, x0: f64, w: f64, versatz: f64| {
            let b = m.add_building(1);
            let pts = [
                vec3(x0, 0.0, 0.0),
                vec3(x0 + w, 0.0, 0.0),
                vec3(x0 + w, 10000.0, 0.0),
                vec3(x0, 10000.0, 0.0),
            ];
            m.build_from_polygon(b, &pts).unwrap();
            m.begin("Gelände");
            assert!(m.set_terrain_offset_of(b, versatz));
            m.commit();
        };
        neu(&mut m, 0.0, 8000.0, 500.0);
        neu(&mut m, 11000.0, 8000.0, 1500.0);
        m.begin("Fläche");
        assert!(m.seed_flat_terrain(&[]));
        m.commit();
        let spots = m.terrain_spots();
        let x = |f: fn(f64, f64) -> f64, i: usize, a: f64| spots.iter().map(|s| s.p[i]).fold(a, f);
        let k = [
            x(f64::min, 0, f64::MAX),
            x(f64::min, 1, f64::MAX),
            x(f64::max, 0, f64::MIN),
            x(f64::max, 1, f64::MIN),
        ];
        // ein Raster im bisherigen Netz, 1 mm vom Rand
        let mut raster = Vec::new();
        for i in 0..=40 {
            for j in 0..=40 {
                let q = [
                    k[0] + 1.0 + (k[2] - k[0] - 2.0) * f64::from(i) / 40.0,
                    k[1] + 1.0 + (k[3] - k[1] - 2.0) * f64::from(j) / 40.0,
                ];
                raster.push((q, m.terrain_z_survey(q[0], q[1]).unwrap()));
            }
        }
        let platte = m.hoehe_an_der_platte();
        let n = m.terrain().unwrap().points.len();
        let brueche = m.terrain().unwrap().breaks.len();
        let ring = [
            [k[0] - 500.0, k[1] - 500.0],
            [k[2] + 500.0, k[1] - 500.0],
            [k[2] + 500.0, k[3] + 500.0],
            [k[0] - 500.0, k[3] + 500.0],
        ];
        m.begin("Grenze");
        assert!(m.add_parcel(&ring));
        m.commit();
        let t = m.terrain().unwrap();
        assert_eq!(t.points.len(), n + 4);
        assert_eq!(t.breaks.len(), brueche + 1, "der alte Rand");
        for (q, z) in &raster {
            let h = m.terrain_z_survey(q[0], q[1]).unwrap();
            assert!((h - z).abs() < 1e-6, "{q:?}: {z} → {h}");
        }
        for s in m.terrain_spots().iter().skip(n) {
            assert!((s.p[2] - platte).abs() < 1.0, "{:?} statt {platte}", s.p);
        }
        // weiter gezogen: der Rand von eben kommt dazu, keiner doppelt
        let mut weiter = ring;
        weiter[2] = [k[2] + 20000.0, k[3] + 20000.0];
        m.begin("Ecke");
        assert!(m.set_parcel_ring(0, &weiter));
        m.commit();
        let t = m.terrain().unwrap();
        assert_eq!(t.breaks.len(), brueche + 2);
        let mut ringe: Vec<Vec<String>> = t
            .breaks
            .iter()
            .map(|b| {
                let mut v = b.pts.clone();
                v.sort();
                v
            })
            .collect();
        ringe.sort();
        ringe.dedup();
        assert_eq!(ringe.len(), t.breaks.len());
        for (q, z) in &raster {
            let h = m.terrain_z_survey(q[0], q[1]).unwrap();
            assert!((h - z).abs() < 1e-6, "weiter {q:?}: {z} → {h}");
        }
    }

    /// Im System eines Lageplans (gedreht) kommt die Grenze in dessen
    /// Werten dazu; die erste Fläche legt die Punkte dann bis über das
    /// Grundstück an.
    #[test]
    fn grenze_im_system_und_gelaende_darueber() {
        let mut m = haus();
        let mut t = Terrain::new(Crs::Local);
        t.local = Some(LocalFrame {
            e0: 1000.0,
            n0: 2000.0,
            rot: 90.0,
        });
        m.begin("Lageplan");
        assert!(m.set_terrain(Some(t)));
        m.commit();
        let ring = [
            [-10000.0, -10000.0],
            [30000.0, -10000.0],
            [30000.0, 20000.0],
        ];
        m.begin("Grenze");
        assert!(m.add_parcel(&ring));
        m.commit();
        let p = &m.terrain().unwrap().parcels[0];
        // Modell-x zeigt nach Nord
        assert_eq!(p[0], (1010.0, 1990.0));
        assert_eq!(p[1], (1010.0, 2030.0));
        m.begin("Fläche");
        assert!(m.seed_flat_terrain(&[]));
        m.commit();
        let t = m.terrain().unwrap();
        let f = m.survey_frame_of(t).unwrap();
        let rand = m.plate_settings().margin;
        let xs: Vec<[f64; 2]> = t.points.iter().map(|q| f.to_model(q.e, q.n)).collect();
        let max_x = xs.iter().map(|q| q[0]).fold(f64::MIN, f64::max);
        assert!((max_x - (30000.0 + rand)).abs() < 1e-3, "{xs:?}");
        assert_eq!(t.parcels.len(), 1, "die Grenze bleibt");
    }

    /// Das Aufmaß kommt nach der gezeichneten Grenze: Sie bleibt im
    /// Grundriss, wo sie war, jetzt im System des Aufmaßes. Bringt das
    /// Aufmaß eine eigene Grenze mit, gilt seine.
    #[test]
    fn grenze_bleibt_beim_aufmass() {
        let ring = [
            [-5000.0, -4000.0],
            [20000.0, -4000.0],
            [20000.0, 16000.0],
            [-5000.0, 16000.0],
        ];
        let aufmass = |grenze: bool| {
            let mut t = Terrain::new(Crs::Local);
            for (i, (e, n)) in [
                (100.0, 200.0),
                (160.0, 200.0),
                (160.0, 250.0),
                (100.0, 250.0),
            ]
            .into_iter()
            .enumerate()
            {
                t.points.push(TerrainPoint {
                    nr: format!("{}", i + 1),
                    e,
                    n,
                    h: 30.0,
                    code: String::new(),
                    off: false,
                    hand: false,
                    gemessen: None,
                });
            }
            if grenze {
                t.parcels = vec![vec![(110.0, 210.0), (150.0, 210.0), (150.0, 240.0)]];
            }
            t
        };
        for grenze in [false, true] {
            let mut m = haus();
            m.begin("Grenze");
            assert!(m.add_parcel(&ring));
            m.commit();
            m.begin("Aufmaß");
            assert!(m.import_terrain(aufmass(grenze)));
            m.commit();
            let t = m.terrain().unwrap();
            assert_eq!(t.parcels.len(), 1);
            let f = m.survey_frame_of(t).unwrap();
            let im_plan: Vec<[f64; 2]> = t.parcels[0]
                .iter()
                .map(|&(e, n)| f.to_model(e, n))
                .collect();
            if grenze {
                assert_eq!(t.parcels[0].len(), 3, "die Grenze des Aufmaßes");
            } else {
                for (a, b) in im_plan.iter().zip(&ring) {
                    assert!(
                        (a[0] - b[0]).abs() < 1.0 && (a[1] - b[1]).abs() < 1.0,
                        "{a:?} {b:?}"
                    );
                }
            }
        }
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
            gemessen: None,
        });
        assert!(!t.assumed());
    }
}
