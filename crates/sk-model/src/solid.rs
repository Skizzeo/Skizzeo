//! Einfacher Körper aus Dreiecken und sichtbaren Kanten (Darstellung und Treffertest).

use sk_math::{ray_triangle, vec3, Vec3};

/// Baustoff einer Fläche als Darstellungsschlüssel (siehe [`crate::material_key`]).
/// Farbe und Schraffur stehen in der Baustoffbibliothek des Modells.
pub mod material {
    /// Ohne Baustoff.
    pub const PLAIN: u16 = 0;
    /// Zusatzbit für Schnittflächen (Grundriss, Schnitt).
    pub const CUT: u16 = 0x8000;
}

/// Kantenarten für die Strichstärke in Zeichnungen.
pub mod edge_kind {
    /// Sichtkante (Ansicht).
    pub const VIEW: u8 = 0;
    /// Schnittkontur.
    pub const CUT: u8 = 1;
    /// Feine Linie, z. B. Fuge zwischen zwei Schichten.
    pub const FINE: u8 = 2;
    /// Schnittkontur einer nicht tragenden Schicht (z. B. Dämmung), mitteldick.
    pub const CUT_LAYER: u8 = 3;
}

#[derive(Clone, Copy, Debug)]
pub struct Edge {
    pub a: Vec3,
    pub b: Vec3,
    pub kind: u8,
}

#[derive(Clone, Copy, Debug)]
pub struct Tri {
    pub p: [Vec3; 3],
    /// Flächennormale.
    pub n: Vec3,
    pub mat: u16,
    /// Musterkoordinaten (für Schraffuren): u längs, v quer zur Schicht (0..1).
    pub uv: [[f64; 2]; 3],
    /// Teil des Körpers, zu dem das Dreieck gehört (bei Wänden: Segment im Zug).
    pub elem: u32,
}

#[derive(Clone, Debug, Default)]
pub struct Solid {
    pub triangles: Vec<Tri>,
    /// Sichtbare Kanten (Umrisse der Flächen, keine inneren Fugen).
    pub edges: Vec<Edge>,
    /// Baustoff für die nächsten Flächen.
    pub mat: u16,
    /// Art der nächsten Kanten.
    pub edge_kind: u8,
    /// Teil (Segment) für die nächsten Flächen.
    pub elem: u32,
}

impl Solid {
    /// Viereck (eben, konvex) als zwei Dreiecke.
    pub fn quad(&mut self, a: Vec3, b: Vec3, c: Vec3, d: Vec3, normal: Vec3) {
        self.quad_uv([a, b, c, d], normal, [[0.0; 2]; 4]);
    }

    /// Viereck mit Musterkoordinaten je Ecke.
    pub fn quad_uv(&mut self, p: [Vec3; 4], normal: Vec3, uv: [[f64; 2]; 4]) {
        let (mat, elem) = (self.mat, self.elem);
        self.triangles.push(Tri {
            p: [p[0], p[1], p[2]],
            n: normal,
            mat,
            uv: [uv[0], uv[1], uv[2]],
            elem,
        });
        self.triangles.push(Tri {
            p: [p[0], p[2], p[3]],
            n: normal,
            mat,
            uv: [uv[0], uv[2], uv[3]],
            elem,
        });
    }

    pub fn edge(&mut self, a: Vec3, b: Vec3) {
        if (a - b).length() > 1e-6 {
            self.edges.push(Edge {
                a,
                b,
                kind: self.edge_kind,
            });
        }
    }

    pub fn append(&mut self, other: &Solid) {
        self.triangles.extend_from_slice(&other.triangles);
        self.edges.extend_from_slice(&other.edges);
    }

    pub fn raycast(&self, origin: Vec3, dir: Vec3) -> Option<f64> {
        self.raycast_elem(origin, dir).map(|h| h.0)
    }

    /// Nächster Treffer: Abstand und Teil ([`Tri::elem`]) des getroffenen Dreiecks.
    pub fn raycast_elem(&self, origin: Vec3, dir: Vec3) -> Option<(f64, u32)> {
        self.triangles
            .iter()
            .filter_map(|t| ray_triangle(origin, dir, t.p[0], t.p[1], t.p[2]).map(|d| (d, t.elem)))
            .min_by(|a, b| a.0.total_cmp(&b.0))
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
            let mut poly: Vec<(Vec3, [f64; 2])> = Vec::with_capacity(4);
            for k in 0..3 {
                let (a, b) = (t.p[k], t.p[(k + 1) % 3]);
                let (ua, ub) = (t.uv[k], t.uv[(k + 1) % 3]);
                let (da, db) = (side(a), side(b));
                if da <= 0.0 {
                    poly.push((a, ua));
                }
                if (da < 0.0) != (db < 0.0) && (da - db).abs() > 1e-12 {
                    let f = da / (da - db);
                    let uv = [ua[0] + (ub[0] - ua[0]) * f, ua[1] + (ub[1] - ua[1]) * f];
                    poly.push((a + (b - a) * f, uv));
                }
            }
            for k in 1..poly.len().saturating_sub(1) {
                out.triangles.push(Tri {
                    p: [poly[0].0, poly[k].0, poly[k + 1].0],
                    n: t.n,
                    mat: t.mat,
                    uv: [poly[0].1, poly[k].1, poly[k + 1].1],
                    elem: t.elem,
                });
            }
        }
        for e in &self.edges {
            let (a, b) = (e.a, e.b);
            let (da, db) = (side(a), side(b));
            out.edge_kind = e.kind;
            match (da <= 0.0, db <= 0.0) {
                (true, true) => out.edges.push(*e),
                (false, false) => {}
                _ => {
                    let x = a + (b - a) * (da / (da - db));
                    out.edge(if da <= 0.0 { a } else { b }, x);
                }
            }
        }
        out.edge_kind = edge_kind::VIEW;
        out
    }
}
