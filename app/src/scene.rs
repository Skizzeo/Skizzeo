//! Szene: gezeichnete Wände mit Rückgängig-Verlauf. Einheit: Millimeter.

use sk_math::{vec3, Vec3};
use sk_model::{Solid, WallChain};
use sk_render::MeshData;

pub struct Scene {
    pub walls: Vec<WallChain>,
    /// Frühere Stände für „Rückgängig“.
    undo: Vec<Vec<WallChain>>,
    /// Rückgängig gemachte Stände für „Wiederholen“.
    redo: Vec<Vec<WallChain>>,
    solid: Solid,
}

impl Scene {
    pub fn new() -> Scene {
        Scene {
            walls: Vec::new(),
            undo: Vec::new(),
            redo: Vec::new(),
            solid: Solid::default(),
        }
    }

    fn rebuild(&mut self) {
        let mut solid = Solid::default();
        for w in &self.walls {
            solid.append(&w.solid());
        }
        self.solid = solid;
    }

    /// Merkt den Stand `before` als Schritt für „Rückgängig“.
    pub fn record(&mut self, before: Vec<WallChain>) {
        if before != self.walls {
            self.undo.push(before);
            self.redo.clear();
        }
    }

    pub fn add_wall(&mut self, w: WallChain) {
        let before = self.walls.clone();
        self.walls.push(w);
        self.record(before);
        self.rebuild();
    }

    /// Ersetzt eine Wand ohne Verlaufseintrag (für Live-Änderungen beim Ziehen).
    pub fn set_wall(&mut self, i: usize, w: WallChain) {
        self.walls[i] = w;
        self.rebuild();
    }

    /// Setzt alle Wände ohne Verlaufseintrag (Abbruch einer Live-Änderung).
    pub fn set_walls(&mut self, walls: Vec<WallChain>) {
        self.walls = walls;
        self.rebuild();
    }

    pub fn undo(&mut self) -> bool {
        match self.undo.pop() {
            Some(prev) => {
                self.redo.push(std::mem::replace(&mut self.walls, prev));
                self.rebuild();
                true
            }
            None => false,
        }
    }

    pub fn redo(&mut self) -> bool {
        match self.redo.pop() {
            Some(next) => {
                self.undo.push(std::mem::replace(&mut self.walls, next));
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
