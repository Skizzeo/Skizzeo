//! Grenze mit Griffen ändern (I3, gelaende/analyse-interaktives-gelaende.md
//! §4.2): Ecken verschieben, einfügen und löschen, eine Kante parallel
//! versetzen. Das gilt für die Flurstücke des Geländes (`[tparcel]`), ob
//! gezeichnet oder aus dem Lageplan (§5 J7 B); Grenzen aus Punkten eines
//! Aufmaßes ändern sich mit ihren Punkten.
//!
//! Gerechnet wird im Grundriss (mm). Zurück ins System des Geländes geht
//! nur, was sich bewegt hat, auf mm gerundet; die übrigen Ecken behalten
//! ihre Werte aus dem Lageplan genau.

use super::gelaende_eben::{flaeche, rund};
use super::*;

/// Kleinste Fläche einer Grenze (mm², 1 m²).
const MIN_FLAECHE: f64 = 1e6;

impl Model {
    /// Die Flurstücke im Grundriss (mm), je Ring die Ecken; leer ohne
    /// Gelände oder ohne Lage des Systems.
    pub fn parcel_rings(&self) -> Vec<Vec<[f64; 2]>> {
        let Some(t) = self.terrain() else {
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

    /// Flurstück `i` bekommt die Ecken `ring` (mm, im offenen Schritt).
    /// Ecken, die auf einer alten Ecke liegen, behalten deren Werte; die
    /// übrigen kommen auf mm gerundet ins System. `false`, wenn es `i`
    /// nicht gibt, sich nichts ändert oder die Grenze keine einfache
    /// Fläche von mindestens 1 m² mehr wäre.
    pub fn set_parcel_ring(&mut self, i: usize, ring: &[[f64; 2]]) -> bool {
        if !parcel_ring_ok(ring) {
            return false;
        }
        let Some(t) = self.terrain() else {
            return false;
        };
        let Some(f) = self.survey_frame_of(t) else {
            return false;
        };
        let Some(alt) = t.parcels.get(i) else {
            return false;
        };
        let im_plan: Vec<([f64; 2], (f64, f64))> = alt
            .iter()
            .map(|&(e, n)| (f.to_model(e, n), (e, n)))
            .collect();
        let neu: Vec<(f64, f64)> = ring
            .iter()
            .map(|p| {
                // eine Ecke, die nicht bewegt wurde, bleibt genau
                im_plan
                    .iter()
                    .find(|(q, _)| (q[0] - p[0]).abs() < 1e-6 && (q[1] - p[1]).abs() < 1e-6)
                    .map_or_else(
                        || {
                            let (e, n) = f.to_survey(p[0], p[1]);
                            (rund(e), rund(n))
                        },
                        |(_, s)| *s,
                    )
            })
            .collect();
        if neu == *alt {
            return false;
        }
        let vorher: Vec<[f64; 2]> = im_plan.iter().map(|q| q.0).collect();
        let mut t = t.clone();
        t.parcels[i] = neu;
        // die Punkte an der Grenze ziehen mit (U-264a, B-264a), in einer
        // Änderung (B-264e)
        self.grenze_bauen(t, ring, Some(&vorher))
    }
}

/// Eine Grenze, die sich als Fläche zeigen und rechnen lässt: drei Ecken,
/// mindestens 1 m², ohne Schleife.
pub fn parcel_ring_ok(ring: &[[f64; 2]]) -> bool {
    ring.len() >= 3
        && flaeche(ring) >= MIN_FLAECHE
        && sk_math::polygon::is_simple(
            &ring
                .iter()
                .map(|q| sk_math::vec3(q[0], q[1], 0.0))
                .collect::<Vec<_>>(),
        )
}

/// `ring` mit der Kante `k` (von Ecke `k` zu `k + 1`) um `s` (mm) parallel
/// versetzt, nach links von ihrer Richtung positiv; die Nachbarkanten
/// behalten ihre Richtung (§4.2). Ist eine Nachbarkante gleich gerichtet,
/// rückt ihre Ecke quer mit. `None` bei einer Kante ohne Länge.
pub fn offset_ring_edge(ring: &[[f64; 2]], k: usize, s: f64) -> Option<Vec<[f64; 2]>> {
    let n = ring.len();
    if n < 3 || k >= n {
        return None;
    }
    let (a, b) = (ring[k], ring[(k + 1) % n]);
    let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
    let l = dx.hypot(dy);
    if l < 1e-9 {
        return None;
    }
    let nrm = [-dy / l * s, dx / l * s];
    let a2 = [a[0] + nrm[0], a[1] + nrm[1]];
    let b2 = [b[0] + nrm[0], b[1] + nrm[1]];
    let d = [dx, dy];
    // die Ecke `p` wandert auf ihrer Nachbarkante (Richtung `u`) bis zur
    // versetzten Kante; parallel: quer mit
    let auf = |p: [f64; 2], u: [f64; 2], quer: [f64; 2]| -> [f64; 2] {
        let det = u[0] * d[1] - u[1] * d[0];
        if det.abs() < 1e-9 * u[0].hypot(u[1]) * l {
            return quer;
        }
        // p + t·u liegt auf a2 + r·d
        let t = ((a2[0] - p[0]) * d[1] - (a2[1] - p[1]) * d[0]) / det;
        [p[0] + t * u[0], p[1] + t * u[1]]
    };
    let vor = ring[(k + n - 1) % n];
    let nach = ring[(k + 2) % n];
    let mut r = ring.to_vec();
    r[k] = auf(a, [a[0] - vor[0], a[1] - vor[1]], a2);
    r[(k + 1) % n] = auf(b, [nach[0] - b[0], nach[1] - b[1]], b2);
    Some(r)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Haus und ein gezeichnetes Rechteck 20 × 10 m.
    fn mit_grenze() -> Model {
        let mut m = Model::with_seed(2);
        m.begin("Grenze");
        assert!(m.add_parcel(&[
            [0.0, 0.0],
            [20000.0, 0.0],
            [20000.0, 10000.0],
            [0.0, 10000.0]
        ]));
        m.commit();
        m
    }

    /// Ecke verschieben: Nur sie ändert sich im System; eine Schleife
    /// oder zu wenig Fläche ändert nichts.
    #[test]
    fn ecke_verschieben() {
        let mut m = mit_grenze();
        let mut r = m.parcel_rings()[0].clone();
        r[2] = [22000.0, 12000.0];
        m.begin("Grenze: Ecke verschoben");
        assert!(m.set_parcel_ring(0, &r));
        assert!(!m.set_parcel_ring(0, &r), "nichts neu");
        // Schleife
        let mut x = r.clone();
        x.swap(1, 2);
        assert!(!m.set_parcel_ring(0, &x));
        assert!(!m.set_parcel_ring(0, &r[..2]));
        assert!(!m.set_parcel_ring(1, &r), "kein zweites Flurstück");
        m.commit();
        let p = &m.terrain().unwrap().parcels[0];
        assert_eq!(p[2], (22.0, 12.0));
        assert_eq!(p[0], (0.0, 0.0));
    }

    /// Unbewegte Ecken behalten ihre Werte aus dem Lageplan genau, auch
    /// unter mm und im gedrehten System.
    #[test]
    fn lageplan_bleibt_genau() {
        let mut m = Model::with_seed(2);
        let mut t = Terrain::new(Crs::Local);
        t.local = Some(LocalFrame {
            e0: 1000.0,
            n0: 2000.0,
            rot: 33.0,
        });
        t.parcels = vec![vec![
            (1000.1234, 2000.5678),
            (1030.9876, 2001.4321),
            (1029.5555, 2020.4444),
        ]];
        m.begin("Lageplan");
        assert!(m.set_terrain(Some(t)));
        m.commit();
        let mut r = m.parcel_rings()[0].clone();
        r.insert(1, [r[0][0] + 5000.0, r[0][1] - 3000.0]);
        m.begin("Grenze: Ecke eingefügt");
        assert!(m.set_parcel_ring(0, &r));
        m.commit();
        let p = &m.terrain().unwrap().parcels[0];
        assert_eq!(p.len(), 4);
        assert_eq!(p[0], (1000.1234, 2000.5678));
        assert_eq!(p[2], (1030.9876, 2001.4321));
        assert_eq!(p[3], (1029.5555, 2020.4444));
        assert_eq!(p[1].0, rund(p[1].0));
    }

    /// Kante parallel: Die Nachbarkanten behalten die Richtung, beim
    /// Rechteck wächst es um den Versatz; schräge Nachbarn verlängern sich.
    #[test]
    fn kante_versetzen() {
        let r = [
            [0.0, 0.0],
            [20000.0, 0.0],
            [20000.0, 10000.0],
            [0.0, 10000.0],
        ];
        // obere Kante (2 → 3) zeigt nach −x, links ist −y: −1000 hebt sie
        let v = offset_ring_edge(&r, 2, -1000.0).unwrap();
        assert_eq!(v[2], [20000.0, 11000.0]);
        assert_eq!(v[3], [0.0, 11000.0]);
        assert_eq!((v[0], v[1]), (r[0], r[1]));
        // schräger Nachbar: Ecke wandert auf ihm
        let t = [[0.0, 0.0], [10000.0, 0.0], [5000.0, 10000.0]];
        let v = offset_ring_edge(&t, 0, 2000.0).unwrap();
        assert!((v[0][1] - 2000.0).abs() < 1e-9 && (v[1][1] - 2000.0).abs() < 1e-9);
        assert!((v[0][0] - 1000.0).abs() < 1e-9, "{:?}", v[0]);
        assert!((v[1][0] - 9000.0).abs() < 1e-9, "{:?}", v[1]);
        assert!(offset_ring_edge(&[[0.0, 0.0], [0.0, 0.0], [1.0, 1.0]], 0, 5.0).is_none());
        // gleich gerichteter Nachbar (Ecke auf einer Geraden): quer mit
        let g = [[0.0, 0.0], [5000.0, 0.0], [10000.0, 0.0], [5000.0, 8000.0]];
        let v = offset_ring_edge(&g, 1, 500.0).unwrap();
        assert_eq!(v[1], [5000.0, 500.0]);
        let mut m = mit_grenze();
        let v = offset_ring_edge(&m.parcel_rings()[0], 2, -1000.0).unwrap();
        m.begin("Grenze: Kante verschoben");
        assert!(m.set_parcel_ring(0, &v));
        m.commit();
        assert!((m.terrain().unwrap().parcel_area() - 220.0).abs() < 1e-9);
    }
}
