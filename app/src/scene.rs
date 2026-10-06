//! Szene: gezeichnete Wände mit Rückgängig-Verlauf. Einheit: Millimeter.

use crate::ui::ViewKind;
use sk_math::Vec3;
use sk_model::{edge_kind, material, Solid, WallChain};
use sk_paint::Rgba;
use sk_render::{pattern, MeshData};
use sk_ui::theme;

/// Schnitthöhe des Grundrisses über dem Boden (mm).
pub const PLAN_CUT: f64 = 1000.0;

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

    /// Darstellung für eine Ansicht. Beim Schnitt: Ebene durch `section` (Punkt, Normale
    /// zum Betrachter), alles davor wird weggeschnitten.
    pub fn mesh(&self, view: ViewKind, section: Option<(Vec3, Vec3)>) -> MeshData {
        let drawing = view != ViewKind::Persp;
        let mesh_of = |s: &Solid| mesh_with(s, drawing);
        match (view, section) {
            (ViewKind::Plan, _) => {
                let mut s = Solid::default();
                for w in &self.walls {
                    s.append(&w.solid_cut_at(PLAN_CUT));
                }
                mesh_of(&s)
            }
            (ViewKind::Section, Some((p0, n))) => {
                let mut s = self.solid.clipped(p0, n);
                for w in &self.walls {
                    s.append(&w.section_caps(p0, n));
                }
                mesh_of(&s)
            }
            _ => mesh_of(&self.solid),
        }
    }

    /// Umschließender Quader des Modells.
    pub fn bounds(&self) -> Option<(Vec3, Vec3)> {
        self.solid.bounds()
    }

    /// Nächster Treffer eines Strahls mit dem Modell.
    pub fn raycast(&self, origin: Vec3, dir: Vec3) -> Option<f64> {
        self.solid.raycast(origin, dir)
    }

    /// Mitte des umschließenden Quaders aller Flächen.
    pub fn center(&self) -> Option<Vec3> {
        self.bounds().map(|(lo, hi)| (lo + hi) * 0.5)
    }
}

fn rgb(c: Rgba) -> [f32; 3] {
    [c.0 as f32 / 255.0, c.1 as f32 / 255.0, c.2 as f32 / 255.0]
}

/// Darstellungsfarbe eines Baustoffs.
fn color_of(mat: u16) -> [f32; 3] {
    use theme::material as m;
    let cut = mat & material::CUT != 0;
    rgb(match (mat & !material::CUT, cut) {
        (material::AERATED_CONCRETE, false) => m::AERATED_CONCRETE,
        (material::AERATED_CONCRETE, true) => m::AERATED_CONCRETE_CUT,
        (material::INSULATION, false) => m::INSULATION,
        (material::INSULATION, true) => m::INSULATION_CUT,
        _ => theme::FACE,
    })
}

/// Netz für die 3D-Ansicht.
pub fn mesh_of(s: &Solid) -> MeshData {
    mesh_with(s, false)
}

/// Netz für die 3D-Ansicht oder als Bauzeichnung (weiße Flächen, Schraffuren in
/// Schnittflächen, Strichstärken nach Kantenart).
pub fn mesh_with(s: &Solid, drawing: bool) -> MeshData {
    use theme::drawing as d;
    let mut m = MeshData::default();
    for t in &s.triangles {
        let n = t.n.to_f32();
        let (c, pat) = if drawing {
            let pat = match (t.mat & material::CUT != 0, t.mat & !material::CUT) {
                (true, material::AERATED_CONCRETE) => pattern::DIAGONAL,
                (true, material::INSULATION) => pattern::ZIGZAG,
                _ => pattern::NONE,
            };
            (rgb(d::FILL), pat)
        } else {
            (color_of(t.mat), pattern::NONE)
        };
        for (v, uv) in t.p.iter().zip(t.uv) {
            let p = v.to_f32();
            m.faces.push([
                p[0],
                p[1],
                p[2],
                n[0],
                n[1],
                n[2],
                c[0],
                c[1],
                c[2],
                pat,
                uv[0] as f32,
                uv[1] as f32,
            ]);
        }
    }
    m.edges = s
        .edges
        .iter()
        .map(|e| {
            let w = match (drawing, e.kind) {
                (_, edge_kind::FINE) => d::FINE_WIDTH,
                (true, edge_kind::CUT) => d::CUT_WIDTH,
                (true, _) => d::VIEW_WIDTH,
                (false, _) => 1.0,
            };
            ([e.a.to_f32(), e.b.to_f32()], w)
        })
        .collect();
    m
}
