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

    /// Wandkörper mit Gehrungen an den Ecken.
    pub fn solid(&self) -> Solid {
        let mut s = Solid::default();
        let pts = self.clean_points();
        let closed = self.closed && pts.len() >= 3;
        if pts.len() < 2 {
            return s;
        }
        let n = pts.len();
        let m = if closed { n } else { n - 1 };
        let dirs: Vec<Vec3> = (0..m)
            .map(|i| (pts[(i + 1) % n] - pts[i]).normalized())
            .collect();
        let (oa, ob) = self.ref_side.span(self.thickness);

        // Eckpunkt der versetzten Linie an Punkt j
        let corner = |j: usize, off: f64| -> Vec3 {
            let has_prev = closed || j > 0;
            let has_next = closed || j < n - 1;
            let next = j % m.max(1);
            let prev = (j + m - 1) % m;
            match (has_prev, has_next) {
                (true, true) => {
                    let (d0, d1) = (dirs[prev], dirs[next]);
                    let (n0, n1) = (right_of(d0), right_of(d1));
                    let c = cross2(d0, d1);
                    let base = pts[j % n];
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

        let h = self.height;
        let up = vec3(0.0, 0.0, h);
        let ca: Vec<Vec3> = (0..n).map(|j| corner(j, oa)).collect();
        let cb: Vec<Vec3> = (0..n).map(|j| corner(j, ob)).collect();

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
