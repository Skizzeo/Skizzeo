//! Wandzug: Polygonzug auf dem Boden, aus dem Wände mit Dicke und Höhe entstehen.
//!
//! Die gezeichnete Linie ist die Bezugslinie der Wand. Die Bezugsseite sagt,
//! welche Wandfläche auf dieser Linie liegt, in Zeichenrichtung gesehen.
//! Wird im Uhrzeigersinn gezeichnet, liegt links außen: Bezugsseite `Left`
//! bedeutet dann „außen“, der Wandkörper wächst nach rechts ins Gebäude.

use crate::Solid;
use sk_math::{vec3, Vec3};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RefSide {
    /// Linke Wandfläche auf der Linie (bei Uhrzeigersinn: außen).
    Left,
    /// Rechte Wandfläche auf der Linie (bei Uhrzeigersinn: innen).
    Right,
    /// Wandachse auf der Linie.
    Center,
}

impl RefSide {
    /// Reihenfolge beim Umschalten mit Tab.
    pub fn next(self) -> RefSide {
        match self {
            RefSide::Left => RefSide::Right,
            RefSide::Right => RefSide::Center,
            RefSide::Center => RefSide::Left,
        }
    }

    /// Bereich des Wandkörpers quer zur Linie (positiv = rechts der Zeichenrichtung).
    fn span(self, t: f64) -> (f64, f64) {
        match self {
            RefSide::Left => (0.0, t),
            RefSide::Right => (-t, 0.0),
            RefSide::Center => (-t * 0.5, t * 0.5),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct WallChain {
    /// Eckpunkte der Bezugslinie (z wird ignoriert, Wände stehen auf z = 0).
    pub points: Vec<Vec3>,
    pub closed: bool,
    pub ref_side: RefSide,
    pub thickness: f64,
    pub height: f64,
}

/// Größter Abstand einer Gehrungsecke vom Linienpunkt, in Wanddicken.
const MITER_LIMIT: f64 = 8.0;

fn flat(p: Vec3) -> Vec3 {
    vec3(p.x, p.y, 0.0)
}

/// Rechte Normale einer Richtung in der Ebene.
fn right_of(d: Vec3) -> Vec3 {
    vec3(d.y, -d.x, 0.0)
}

fn cross2(a: Vec3, b: Vec3) -> f64 {
    a.x * b.y - a.y * b.x
}

impl WallChain {
    /// Punkte ohne doppelte Nachbarn (und ohne Schlusspunkt gleich Startpunkt).
    pub fn clean_points(&self) -> Vec<Vec3> {
        let mut pts: Vec<Vec3> = Vec::new();
        for p in &self.points {
            let p = flat(*p);
            if pts.last().is_none_or(|q| (p - *q).length() > 1.0) {
                pts.push(p);
            }
        }
        if self.closed && pts.len() > 2 && (pts[0] - pts[pts.len() - 1]).length() <= 1.0 {
            pts.pop();
        }
        pts
    }

    /// Bereinigte Punkte, ob wirklich geschlossen, und Richtung jedes Segments.
    fn layout(&self) -> Option<(Vec<Vec3>, bool, Vec<Vec3>)> {
        let pts = self.clean_points();
        if pts.len() < 2 {
            return None;
        }
        let closed = self.closed && pts.len() >= 3;
        let n = pts.len();
        let m = if closed { n } else { n - 1 };
        let dirs = (0..m)
            .map(|i| (pts[(i + 1) % n] - pts[i]).normalized())
            .collect();
        Some((pts, closed, dirs))
    }

    /// Anzahl der Wandsegmente.
    pub fn segment_count(&self) -> usize {
        self.layout().map_or(0, |(_, _, dirs)| dirs.len())
    }

    /// Normale eines Segments in der Ebene (rechts der Zeichenrichtung).
    pub fn segment_normal(&self, i: usize) -> Option<Vec3> {
        let (_, _, dirs) = self.layout()?;
        dirs.get(i).map(|d| right_of(*d))
    }

    /// Lage der Außenfläche quer zur Bezugslinie (positiv = rechts der Zeichenrichtung).
    ///
    /// Geschlossen: die Seite, die vom umschlossenen Bereich weg zeigt.
    /// Offen: die Fläche auf der Bezugsseite (Mitte zählt als links).
    pub fn outer_offset(&self) -> f64 {
        let (lo, hi) = self.ref_side.span(self.thickness);
        match self.layout() {
            Some((pts, true, _)) => {
                let n = pts.len();
                let area: f64 = (0..n).map(|i| cross2(pts[i], pts[(i + 1) % n])).sum();
                // Fläche > 0: gegen den Uhrzeigersinn, außen liegt rechts
                if area > 0.0 {
                    hi
                } else {
                    lo
                }
            }
            _ => match self.ref_side {
                RefSide::Right => hi,
                RefSide::Left | RefSide::Center => lo,
            },
        }
    }

    /// Eckpunkte der zur Bezugslinie parallelen Wandfläche im Abstand `off`
    /// (je Punkt der bereinigten Linie einer, mit Gehrung an den Ecken).
    pub fn face_corners(&self, off: f64) -> Vec<Vec3> {
        let Some((pts, closed, dirs)) = self.layout() else {
            return Vec::new();
        };
        let (n, m) = (pts.len(), dirs.len());
        let corner = |j: usize| -> Vec3 {
            let has_prev = closed || j > 0;
            let has_next = closed || j < n - 1;
            match (has_prev, has_next) {
                (true, true) => {
                    let (d0, d1) = (dirs[(j + m - 1) % m], dirs[j % m]);
                    let (n0, n1) = (right_of(d0), right_of(d1));
                    let c = cross2(d0, d1);
                    let base = pts[j];
                    if c.abs() < 1e-9 {
                        return base + n1 * off;
                    }
                    // Schnitt der Linien base + n0*off + d0*s und base + n1*off + d1*u
                    let p0 = base + n0 * off;
                    let p1 = base + n1 * off;
                    let s_par = cross2(p1 - p0, d1) / c;
                    let x = p0 + d0 * s_par;
                    if (x - base).length() > MITER_LIMIT * self.thickness.max(1.0) {
                        base + n1 * off
                    } else {
                        x
                    }
                }
                (false, _) => pts[0] + right_of(dirs[0]) * off,
                (_, false) => pts[n - 1] + right_of(dirs[m - 1]) * off,
            }
        };
        (0..n).map(corner).collect()
    }

    /// Verschiebt Segment `i` um `d` entlang seiner Normalen. Die Nachbarsegmente
    /// behalten ihre Richtung: Die gemeinsamen Punkte gleiten auf deren Linien.
    /// `None`, wenn dabei ein Segment verschwinden oder sich umkehren würde.
    pub fn with_segment_moved(&self, i: usize, d: f64) -> Option<WallChain> {
        let (pts, closed, dirs) = self.layout()?;
        let (n, m) = (pts.len(), dirs.len());
        if i >= m {
            return None;
        }
        let shift = right_of(dirs[i]) * d;
        let (a, b) = (i, (i + 1) % n);
        // Schnitt der verschobenen Linie mit der Linie des Nachbarn durch `pts[k]`
        let slide = |k: usize, nd: Vec3| -> Vec3 {
            let c = cross2(nd, dirs[i]);
            if c.abs() < 1e-9 {
                return pts[k] + shift;
            }
            // pts[k] + nd*t liegt auf pts[k] + shift + dirs[i]*u
            let t = cross2(shift, dirs[i]) / c;
            pts[k] + nd * t
        };
        let mut out = pts.clone();
        out[a] = if closed || a > 0 {
            slide(a, dirs[(i + m - 1) % m])
        } else {
            pts[a] + shift
        };
        out[b] = if closed || b < n - 1 {
            slide(b, dirs[(i + 1) % m])
        } else {
            pts[b] + shift
        };
        // Kein Segment darf verschwinden oder die Richtung wechseln
        for k in 0..m {
            let v = out[(k + 1) % n] - out[k];
            if v.length() < 1.0 || v.dot(dirs[k]) <= 0.0 {
                return None;
            }
        }
        Some(WallChain {
            points: out,
            closed,
            ..self.clone()
        })
    }

    /// Wandkörper mit Gehrungen an den Ecken.
    pub fn solid(&self) -> Solid {
        let mut s = Solid::default();
        let Some((pts, closed, dirs)) = self.layout() else {
            return s;
        };
        let (n, m) = (pts.len(), dirs.len());
        let (oa, ob) = self.ref_side.span(self.thickness);
        let ca = self.face_corners(oa);
        let cb = self.face_corners(ob);

        let h = self.height;
        let up = vec3(0.0, 0.0, h);

        for i in 0..m {
            let j = (i + 1) % n;
            let (a0, a1, b0, b1) = (ca[i], ca[j], cb[i], cb[j]);
            let nr = right_of(dirs[i]);
            s.quad(a0, a1, b1, b0, vec3(0.0, 0.0, -1.0));
            s.quad(a0 + up, b0 + up, b1 + up, a1 + up, vec3(0.0, 0.0, 1.0));
            s.quad(a0, a0 + up, a1 + up, a1, -nr);
            s.quad(b0, b1, b1 + up, b0 + up, nr);
            for (p, q) in [(a0, a1), (b0, b1)] {
                s.edge(p, q);
                s.edge(p + up, q + up);
            }
        }
        if !closed {
            let (d0, d1) = (dirs[0], dirs[m - 1]);
            let (a0, b0, a1, b1) = (ca[0], cb[0], ca[n - 1], cb[n - 1]);
            s.quad(a0, b0, b0 + up, a0 + up, -d0);
            s.quad(a1, a1 + up, b1 + up, b1, d1);
            for (p, q) in [(a0, b0), (a1, b1)] {
                s.edge(p, q);
                s.edge(p + up, q + up);
            }
        }
        // Senkrechte Kanten an Ecken (nicht dort, wo die Wand gerade weiterläuft)
        for j in 0..n {
            let straight = if closed || (j > 0 && j < n - 1) {
                let (d0, d1) = (dirs[(j + m - 1) % m], dirs[j % m]);
                cross2(d0, d1).abs() < 1e-9 && d0.dot(d1) > 0.0
            } else {
                false
            };
            if !straight {
                s.edge(ca[j], ca[j] + up);
                s.edge(cb[j], cb[j] + up);
            }
        }
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn square_cw(side: f64) -> Vec<Vec3> {
        // Im Uhrzeigersinn von oben gesehen (Z nach oben)
        vec![
            vec3(0.0, 0.0, 0.0),
            vec3(0.0, side, 0.0),
            vec3(side, side, 0.0),
            vec3(side, 0.0, 0.0),
        ]
    }

    fn bbox(s: &Solid) -> (Vec3, Vec3) {
        let mut lo = vec3(f64::MAX, f64::MAX, f64::MAX);
        let mut hi = -lo;
        for (t, _) in &s.triangles {
            for p in t {
                lo = vec3(lo.x.min(p.x), lo.y.min(p.y), lo.z.min(p.z));
                hi = vec3(hi.x.max(p.x), hi.y.max(p.y), hi.z.max(p.z));
            }
        }
        (lo, hi)
    }

    fn chain(points: Vec<Vec3>, closed: bool, ref_side: RefSide) -> WallChain {
        WallChain {
            points,
            closed,
            ref_side,
            thickness: 400.0,
            height: 3500.0,
        }
    }

    #[test]
    fn aussen_im_uhrzeigersinn_waechst_nach_innen() {
        let s = chain(square_cw(5000.0), true, RefSide::Left).solid();
        let (lo, hi) = bbox(&s);
        assert!(
            (lo.x - 0.0).abs() < 1e-6 && (hi.x - 5000.0).abs() < 1e-6,
            "{lo:?} {hi:?}"
        );
        assert!((hi.z - 3500.0).abs() < 1e-6);
        // Innenecke liegt 400 mm innen
        let inner = vec3(400.0, 400.0, 0.0);
        assert!(s
            .triangles
            .iter()
            .any(|(t, _)| t.iter().any(|p| (*p - inner).length() < 1e-6)));
    }

    #[test]
    fn innen_waechst_nach_aussen() {
        let s = chain(square_cw(5000.0), true, RefSide::Right).solid();
        let (lo, hi) = bbox(&s);
        assert!(
            (lo.x + 400.0).abs() < 1e-6 && (hi.x - 5400.0).abs() < 1e-6,
            "{lo:?} {hi:?}"
        );
    }

    #[test]
    fn mitte_waechst_beidseitig() {
        let s = chain(square_cw(5000.0), true, RefSide::Center).solid();
        let (lo, hi) = bbox(&s);
        assert!(
            (lo.y + 200.0).abs() < 1e-6 && (hi.y - 5200.0).abs() < 1e-6,
            "{lo:?} {hi:?}"
        );
    }

    #[test]
    fn offener_zug_hat_stirnflaechen() {
        let s = chain(
            vec![vec3(0.0, 0.0, 0.0), vec3(3000.0, 0.0, 0.0)],
            false,
            RefSide::Left,
        )
        .solid();
        // unten, oben, zwei Seiten, zwei Stirnseiten = 6 Vierecke
        assert_eq!(s.triangles.len(), 12);
        assert_eq!(s.edges.len(), 12);
    }

    #[test]
    fn geschlossenes_viereck_hat_keine_stirnflaechen() {
        let s = chain(square_cw(5000.0), true, RefSide::Left).solid();
        assert_eq!(s.triangles.len(), 4 * 8);
        // je Seite 4 waagerechte, je Ecke 2 senkrechte Kanten
        assert_eq!(s.edges.len(), 4 * 4 + 4 * 2);
    }
}

#[cfg(test)]
mod verschieben {
    use super::*;

    fn rechteck() -> WallChain {
        WallChain {
            // im Uhrzeigersinn: links unten, links oben, rechts oben, rechts unten
            points: vec![
                vec3(0.0, 0.0, 0.0),
                vec3(0.0, 4000.0, 0.0),
                vec3(5000.0, 4000.0, 0.0),
                vec3(5000.0, 0.0, 0.0),
            ],
            closed: true,
            ref_side: RefSide::Left,
            thickness: 400.0,
            height: 3500.0,
        }
    }

    fn near(a: Vec3, b: Vec3) -> bool {
        (a - b).length() < 1e-6
    }

    #[test]
    fn aussenseite_im_uhrzeigersinn_ist_links() {
        let w = rechteck();
        assert_eq!(w.outer_offset(), 0.0);
        let mut ccw = w.clone();
        ccw.points.reverse();
        assert_eq!(ccw.outer_offset(), 400.0);
        // Gegen den Uhrzeigersinn liegt die Bezugslinie innen, die Wand wächst nach außen
        let c = ccw.face_corners(ccw.outer_offset());
        assert!(
            c.iter().any(|p| near(*p, vec3(5400.0, 4400.0, 0.0))),
            "{c:?}"
        );
    }

    #[test]
    fn segment_wandert_nachbarn_bleiben_richtungstreu() {
        let w = rechteck();
        // Segment 1 (oben, von links nach rechts) nach außen (+y) schieben:
        // Normale rechts der Zeichenrichtung zeigt nach -y, also d negativ
        let n = w.segment_normal(1).unwrap();
        assert!(near(n, vec3(0.0, -1.0, 0.0)));
        let v = w.with_segment_moved(1, -1000.0).unwrap();
        assert!(near(v.points[1], vec3(0.0, 5000.0, 0.0)));
        assert!(near(v.points[2], vec3(5000.0, 5000.0, 0.0)));
        assert!(near(v.points[0], w.points[0]) && near(v.points[3], w.points[3]));
    }

    #[test]
    fn schraege_nachbarn_gleiten_auf_ihrer_linie() {
        let w = WallChain {
            points: vec![
                vec3(0.0, 0.0, 0.0),
                vec3(1000.0, 3000.0, 0.0),
                vec3(4000.0, 3000.0, 0.0),
                vec3(5000.0, 0.0, 0.0),
            ],
            ..rechteck()
        };
        let v = w.with_segment_moved(1, 300.0).unwrap();
        // Oberes Segment liegt jetzt bei y = 2700, Nachbarn behalten ihre Richtung
        for (k, prev) in [(1usize, 0usize), (2, 3)] {
            assert!((v.points[k].y - 2700.0).abs() < 1e-6);
            let a = (w.points[k] - w.points[prev]).normalized();
            let b = (v.points[k] - v.points[prev]).normalized();
            assert!(near(a, b), "{a:?} {b:?}");
        }
    }

    #[test]
    fn offenes_ende_wandert_parallel() {
        let w = WallChain {
            points: vec![vec3(0.0, 0.0, 0.0), vec3(3000.0, 0.0, 0.0)],
            closed: false,
            ..rechteck()
        };
        let v = w.with_segment_moved(0, 500.0).unwrap();
        assert!(near(v.points[0], vec3(0.0, -500.0, 0.0)));
        assert!(near(v.points[1], vec3(3000.0, -500.0, 0.0)));
    }

    #[test]
    fn zu_weit_verschoben_wird_abgelehnt() {
        let w = rechteck();
        // Oberes Segment über das untere hinaus nach innen schieben
        assert!(w.with_segment_moved(1, 4500.0).is_none());
    }
}

#[cfg(test)]
mod richtung {
    use super::*;

    #[test]
    fn links_bezug_waechst_nach_rechts() {
        let w = WallChain {
            points: vec![vec3(0.0, 0.0, 0.0), vec3(0.0, 4000.0, 0.0)],
            closed: false,
            ref_side: RefSide::Left,
            thickness: 400.0,
            height: 3500.0,
        };
        let s = w.solid();
        let xs: Vec<f64> = s
            .triangles
            .iter()
            .flat_map(|(t, _)| t.iter().map(|p| p.x))
            .collect();
        let (lo, hi) = (
            xs.iter().cloned().fold(f64::MAX, f64::min),
            xs.iter().cloned().fold(f64::MIN, f64::max),
        );
        assert!(lo.abs() < 1e-9 && (hi - 400.0).abs() < 1e-9, "{lo} {hi}");
    }
}
