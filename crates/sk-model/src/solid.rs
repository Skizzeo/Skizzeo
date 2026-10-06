//! Einfacher Körper aus Dreiecken und sichtbaren Kanten (Darstellung und Treffertest).

use sk_math::{ray_triangle, vec3, Vec3};

/// Baustoffe. Die App ordnet ihnen Farben zu.
pub mod material {
    pub const PLAIN: u16 = 0;
    pub const AERATED_CONCRETE: u16 = 1;
    pub const INSULATION: u16 = 2;
    /// Zusatzbit für Schnittflächen (Grundriss, Schnitt).
    pub const CUT: u16 = 0x100;
}

#[derive(Clone, Copy, Debug)]
pub struct Tri {
    pub p: [Vec3; 3],
    /// Flächennormale.
    pub n: Vec3,
    pub mat: u16,
}

#[derive(Clone, Debug, Default)]
pub struct Solid {
    pub triangles: Vec<Tri>,
    /// Sichtbare Kanten (Umrisse der Flächen, keine inneren Fugen).
    pub edges: Vec<[Vec3; 2]>,
    /// Baustoff für die nächsten Flächen.
    pub mat: u16,
}

impl Solid {
    /// Viereck (eben, konvex) als zwei Dreiecke.
    pub fn quad(&mut self, a: Vec3, b: Vec3, c: Vec3, d: Vec3, normal: Vec3) {
        let mat = self.mat;
        self.triangles.push(Tri {
            p: [a, b, c],
            n: normal,
            mat,
        });
        self.triangles.push(Tri {
            p: [a, c, d],
            n: normal,
            mat,
        });
    }

    pub fn edge(&mut self, a: Vec3, b: Vec3) {
        if (a - b).length() > 1e-6 {
            self.edges.push([a, b]);
        }
    }

    pub fn append(&mut self, other: &Solid) {
        self.triangles.extend_from_slice(&other.triangles);
        self.edges.extend_from_slice(&other.edges);
    }

    pub fn raycast(&self, origin: Vec3, dir: Vec3) -> Option<f64> {
        self.triangles
            .iter()
            .filter_map(|t| ray_triangle(origin, dir, t.p[0], t.p[1], t.p[2]))
            .min_by(f64::total_cmp)
    }

    pub fn is_empty(&self) -> bool {
        self.triangles.is_empty()
    }

    /// Kleinster umschließender Quader (min, max).
    pub fn bounds(&self) -> Option<(Vec3, Vec3)> {
        let mut pts = self.triangles.iter().flat_map(|t| t.p.iter());
        let first = *pts.next()?;
        let (mut lo, mut hi) = (first, first);
        for p in pts {
            lo = vec3(lo.x.min(p.x), lo.y.min(p.y), lo.z.min(p.z));
            hi = vec3(hi.x.max(p.x), hi.y.max(p.y), hi.z.max(p.z));
        }
        Some((lo, hi))
    }

    /// Teil des Körpers hinter der Ebene durch `p0` mit Normale `n`
    /// (behalten wird, was auf der Gegenseite von `n` liegt). Ohne Deckel.
    pub fn clipped(&self, p0: Vec3, n: Vec3) -> Solid {
        let side = |p: Vec3| (p - p0).dot(n);
        let mut out = Solid {
            mat: self.mat,
            ..Solid::default()
        };
        for t in &self.triangles {
            let mut poly: Vec<Vec3> = Vec::with_capacity(4);
            for k in 0..3 {
                let (a, b) = (t.p[k], t.p[(k + 1) % 3]);
                let (da, db) = (side(a), side(b));
                if da <= 0.0 {
                    poly.push(a);
                }
                if (da < 0.0) != (db < 0.0) && (da - db).abs() > 1e-12 {
                    poly.push(a + (b - a) * (da / (da - db)));
                }
            }
            for k in 1..poly.len().saturating_sub(1) {
                out.triangles.push(Tri {
                    p: [poly[0], poly[k], poly[k + 1]],
                    n: t.n,
                    mat: t.mat,
                });
            }
        }
        for &[a, b] in &self.edges {
            let (da, db) = (side(a), side(b));
            match (da <= 0.0, db <= 0.0) {
                (true, true) => out.edges.push([a, b]),
                (false, false) => {}
                _ => {
                    let x = a + (b - a) * (da / (da - db));
                    out.edge(if da <= 0.0 { a } else { b }, x);
                }
            }
        }
        out
    }
}
