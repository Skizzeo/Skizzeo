//! Szene: das Gebäudemodell mit Rückgängig-Verlauf und dem daraus abgeleiteten
//! Körper für Darstellung und Treffertest. Einheit: Millimeter.

use crate::ui::ViewKind;
use sk_math::Vec3;
use sk_model::{edge_kind, material, Category, Hatch, Model, RunId, Solid, WallChain};
use sk_paint::Rgba;
use sk_render::{pattern, MeshData};
use sk_ui::theme;

/// Schnitthöhe des Grundrisses über dem Boden (mm).
pub const PLAN_CUT: f64 = 1000.0;

pub struct Scene {
    pub model: Model,
    /// Frühere Stände für „Rückgängig“.
    undo: Vec<Model>,
    /// Rückgängig gemachte Stände für „Wiederholen“.
    redo: Vec<Model>,
    solid: Solid,
}

impl Scene {
    pub fn new() -> Scene {
        Scene::with_model(Model::new())
    }

    pub fn with_model(model: Model) -> Scene {
        let mut s = Scene {
            model,
            undo: Vec::new(),
            redo: Vec::new(),
            solid: Solid::default(),
        };
        s.rebuild();
        s
    }

    fn rebuild(&mut self) {
        let mut solid = Solid::default();
        for (_, c) in self.model.chains() {
            solid.append(&c.solid());
        }
        self.solid = solid;
    }

    /// Geometrie aller Wandzüge.
    pub fn chains(&self) -> impl Iterator<Item = (RunId, WallChain)> + '_ {
        self.model.chains()
    }

    pub fn chain(&self, run: RunId) -> Option<WallChain> {
        self.model.chain(run)
    }

    /// Merkt den Stand `before` als Schritt für „Rückgängig“, falls sich seitdem
    /// etwas geändert hat.
    pub fn record(&mut self, before: Model) {
        if before.revision() != self.model.revision() {
            self.undo.push(before);
            self.redo.clear();
        }
    }

    /// Legt die Außenwand an, die `w` beschreibt (Punkte, Bezugsseite, Höhe),
    /// mit dem voreingestellten Außenwand-Aufbau.
    pub fn add_wall(&mut self, w: &WallChain) -> Option<RunId> {
        let before = self.model.clone();
        let set = self.model.defaults().exterior_wall;
        let run = self.model.add_wall_run(
            &w.points,
            w.closed,
            w.ref_side,
            w.height,
            set,
            Category::ExteriorWall,
        );
        self.record(before);
        self.rebuild();
        run
    }

    /// Neue Eckpunkte eines Wandzugs ohne Verlaufseintrag (Live-Änderung beim Ziehen).
    pub fn set_run_points(&mut self, run: RunId, points: &[Vec3]) {
        if self.model.set_run_points(run, points) {
            self.rebuild();
        }
    }

    /// Setzt das Modell ohne Verlaufseintrag zurück (Abbruch einer Live-Änderung).
    pub fn restore(&mut self, before: Model) {
        self.model.restore(before);
        self.rebuild();
    }

    pub fn undo(&mut self) -> bool {
        match self.undo.pop() {
            Some(prev) => {
                self.redo.push(self.model.clone());
                self.model.restore(prev);
                self.rebuild();
                true
            }
            None => false,
        }
    }

    pub fn redo(&mut self) -> bool {
        match self.redo.pop() {
            Some(next) => {
                self.undo.push(self.model.clone());
                self.model.restore(next);
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
        let mesh_of = |s: &Solid| mesh_with(s, drawing, &self.model);
        match (view, section) {
            (ViewKind::Plan, _) => {
                let mut s = Solid::default();
                for (_, w) in self.chains() {
                    s.append(&w.solid_cut_at(PLAN_CUT));
                }
                mesh_of(&s)
            }
            (ViewKind::Section, Some((p0, n))) => {
                let mut s = self.solid.clipped(p0, n);
                for (_, w) in self.chains() {
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

/// Darstellungsfarbe eines Baustoffs aus der Bibliothek.
fn color_of(model: &Model, mat: u16) -> [f32; 3] {
    let cut = mat & material::CUT != 0;
    match model.material_by_key(mat) {
        Some(m) => {
            let [r, g, b] = if cut { m.cut_color } else { m.color };
            rgb(Rgba::rgb(r, g, b))
        }
        None => rgb(theme::FACE),
    }
}

/// Netz für die 3D-Ansicht.
pub fn mesh_of(s: &Solid, model: &Model) -> MeshData {
    mesh_with(s, false, model)
}

/// Netz für die 3D-Ansicht oder als Bauzeichnung (weiße Flächen, Schraffuren in
/// Schnittflächen, Strichstärken nach Kantenart).
pub fn mesh_with(s: &Solid, drawing: bool, model: &Model) -> MeshData {
    use theme::drawing as d;
    let mut m = MeshData::default();
    for t in &s.triangles {
        let n = t.n.to_f32();
        let (c, pat) = if drawing {
            let hatch = model.material_by_key(t.mat).map(|m| m.hatch);
            let pat = match (t.mat & material::CUT != 0, hatch) {
                (true, Some(Hatch::Diagonal)) => pattern::DIAGONAL,
                (true, Some(Hatch::Zigzag)) => pattern::ZIGZAG,
                _ => pattern::NONE,
            };
            (rgb(d::FILL), pat)
        } else {
            (color_of(model, t.mat), pattern::NONE)
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
                (true, edge_kind::CUT_LAYER) => d::LAYER_CUT_WIDTH,
                (true, _) => d::VIEW_WIDTH,
                (false, _) => 1.0,
            };
            ([e.a.to_f32(), e.b.to_f32()], w)
        })
        .collect();
    m
}
