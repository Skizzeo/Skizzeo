//! Szene mit dem Testkörper (Würfel 3 × 3 × 3 m). Einheit: Millimeter.

use sk_math::{ray_triangle, vec3, Vec3};
use sk_render::MeshData;

pub struct Scene {
    triangles: Vec<[Vec3; 3]>,
    normals: Vec<Vec3>,
    edges: Vec<[Vec3; 2]>,
}

impl Scene {
    pub fn test_body() -> Scene {
        let s = 3000.0;
        let p = |x: f64, y: f64, z: f64| vec3(x * s, y * s, z * s);
        // Eckpunkte gegen den Uhrzeigersinn von außen gesehen
        let quads = [
            [p(0., 0., 0.), p(0., 1., 0.), p(1., 1., 0.), p(1., 0., 0.)], // unten
            [p(0., 0., 1.), p(1., 0., 1.), p(1., 1., 1.), p(0., 1., 1.)], // oben
            [p(0., 0., 0.), p(1., 0., 0.), p(1., 0., 1.), p(0., 0., 1.)], // vorne (-Y)
            [p(1., 1., 0.), p(0., 1., 0.), p(0., 1., 1.), p(1., 1., 1.)], // hinten (+Y)
            [p(0., 1., 0.), p(0., 0., 0.), p(0., 0., 1.), p(0., 1., 1.)], // links (-X)
            [p(1., 0., 0.), p(1., 1., 0.), p(1., 1., 1.), p(1., 0., 1.)], // rechts (+X)
        ];
        let mut triangles = Vec::new();
        let mut normals = Vec::new();
        for q in quads {
            let n = (q[1] - q[0]).cross(q[2] - q[0]).normalized();
            triangles.push([q[0], q[1], q[2]]);
            triangles.push([q[0], q[2], q[3]]);
            normals.push(n);
            normals.push(n);
        }
        let mut edges = Vec::new();
        for i in 0..4 {
            let a = [(0., 0.), (1., 0.), (1., 1.), (0., 1.)][i];
            let b = [(0., 0.), (1., 0.), (1., 1.), (0., 1.)][(i + 1) % 4];
            edges.push([p(a.0, a.1, 0.), p(b.0, b.1, 0.)]);
            edges.push([p(a.0, a.1, 1.), p(b.0, b.1, 1.)]);
            edges.push([p(a.0, a.1, 0.), p(a.0, a.1, 1.)]);
        }
        Scene {
            triangles,
            normals,
            edges,
        }
    }

    pub fn mesh(&self) -> MeshData {
        let mut m = MeshData::default();
        for (t, n) in self.triangles.iter().zip(&self.normals) {
            for v in t {
                let (p, n) = (v.to_f32(), n.to_f32());
                m.faces.push([p[0], p[1], p[2], n[0], n[1], n[2]]);
            }
        }
        m.edges = self.edges.iter().map(|[a, b]| [a.to_f32(), b.to_f32()]).collect();
        m
    }

    /// Nächster Treffer eines Strahls mit dem Modell.
    pub fn raycast(&self, origin: Vec3, dir: Vec3) -> Option<f64> {
        self.triangles
            .iter()
            .filter_map(|t| ray_triangle(origin, dir, t[0], t[1], t[2]))
            .min_by(f64::total_cmp)
    }

    pub fn center(&self) -> Vec3 {
        vec3(1500.0, 1500.0, 1500.0)
    }
}
