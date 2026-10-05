//! Einfacher Körper aus Dreiecken und sichtbaren Kanten (Darstellung und Treffertest).

use sk_math::{ray_triangle, Vec3};

#[derive(Clone, Debug, Default)]
pub struct Solid {
    /// Dreiecke mit Flächennormale.
    pub triangles: Vec<([Vec3; 3], Vec3)>,
    /// Sichtbare Kanten (Umrisse der Flächen, keine inneren Fugen).
    pub edges: Vec<[Vec3; 2]>,
}

impl Solid {
    /// Viereck (eben, konvex) als zwei Dreiecke.
    pub fn quad(&mut self, a: Vec3, b: Vec3, c: Vec3, d: Vec3, normal: Vec3) {
        self.triangles.push(([a, b, c], normal));
        self.triangles.push(([a, c, d], normal));
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
            .filter_map(|(t, _)| ray_triangle(origin, dir, t[0], t[1], t[2]))
            .min_by(f64::total_cmp)
    }

    pub fn is_empty(&self) -> bool {
        self.triangles.is_empty()
    }
}
