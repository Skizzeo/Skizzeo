//! Szene: Testkörper (Würfel 3 × 3 × 3 m) und gezeichnete Wände. Einheit: Millimeter.

use sk_math::{vec3, Vec3};
use sk_model::{Solid, WallChain};
use sk_render::MeshData;

pub struct Scene {
    pub walls: Vec<WallChain>,
    /// Rückgängig gemachte Wände für „Wiederholen“.
    redo: Vec<WallChain>,
    cube: Solid,
    solid: Solid,
}

impl Scene {
    pub fn new() -> Scene {
        let mut s = Scene {
            walls: Vec::new(),
            redo: Vec::new(),
            cube: test_cube(),
            solid: Solid::default(),
        };
        s.rebuild();
        s
    }

    fn rebuild(&mut self) {
        let mut solid = self.cube.clone();
        for w in &self.walls {
            solid.append(&w.solid());
        }
        self.solid = solid;
    }

    pub fn add_wall(&mut self, w: WallChain) {
        self.walls.push(w);
        self.redo.clear();
        self.rebuild();
    }

    pub fn undo(&mut self) -> bool {
        match self.walls.pop() {
            Some(w) => {
                self.redo.push(w);
                self.rebuild();
                true
            }
            None => false,
        }
    }

    pub fn redo(&mut self) -> bool {
        match self.redo.pop() {
            Some(w) => {
                self.walls.push(w);
                self.rebuild();
                true
            }
            None => false,
        }
    }

    pub fn mesh(&self) -> MeshData {
        mesh_of(&self.solid)
    }

    /// Nächster Treffer eines Strahls mit dem Modell.
    pub fn raycast(&self, origin: Vec3, dir: Vec3) -> Option<f64> {
        self.solid.raycast(origin, dir)
    }

    /// Mitte des umschließenden Quaders aller Flächen.
    pub fn center(&self) -> Option<Vec3> {
        let mut pts = self.solid.triangles.iter().flat_map(|(t, _)| t.iter());
        let first = *pts.next()?;
        let (mut lo, mut hi) = (first, first);
        for p in pts {
            lo = vec3(lo.x.min(p.x), lo.y.min(p.y), lo.z.min(p.z));
            hi = vec3(hi.x.max(p.x), hi.y.max(p.y), hi.z.max(p.z));
        }
        Some((lo + hi) * 0.5)
    }
}

pub fn mesh_of(s: &Solid) -> MeshData {
    let mut m = MeshData::default();
    for (t, n) in &s.triangles {
        let n = n.to_f32();
        for v in t {
            let p = v.to_f32();
            m.faces.push([p[0], p[1], p[2], n[0], n[1], n[2]]);
        }
    }
    m.edges = s
        .edges
        .iter()
        .map(|[a, b]| [a.to_f32(), b.to_f32()])
        .collect();
    m
}

fn test_cube() -> Solid {
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
    let mut solid = Solid::default();
    for q in quads {
        let n = (q[1] - q[0]).cross(q[2] - q[0]).normalized();
        solid.quad(q[0], q[1], q[2], q[3], n);
    }
    for i in 0..4 {
        let a = [(0., 0.), (1., 0.), (1., 1.), (0., 1.)][i];
        let b = [(0., 0.), (1., 0.), (1., 1.), (0., 1.)][(i + 1) % 4];
        solid.edge(p(a.0, a.1, 0.), p(b.0, b.1, 0.));
        solid.edge(p(a.0, a.1, 1.), p(b.0, b.1, 1.));
        solid.edge(p(a.0, a.1, 0.), p(a.0, a.1, 1.));
    }
    solid
}
