//! Szene: das Gebäudemodell mit Rückgängig-Verlauf und den daraus abgeleiteten
//! Körpern. Einheit: Millimeter.
//!
//! Körper werden je Wandzug zwischengespeichert. Eine Änderung markiert nur den
//! betroffenen Wandzug; [`Scene::rebuild_dirty`] rechnet nur ihn neu. Das Netz
//! einer Ansicht entsteht aus den gespeicherten Körpern, beim Ziehen getrennt in
//! ein ruhendes Netz ohne den gezogenen Zug und ein Live-Netz nur für ihn.

use crate::ui::ViewKind;
use sk_math::{vec3, Vec3};
use sk_model::{edge_kind, material, Category, ElementId, Hatch, Model, RunId, Solid, WallChain};
use sk_paint::Rgba;
use sk_render::{pattern, MeshData};
use sk_ui::theme;

/// Schnitthöhe des Grundrisses über dem Boden (mm).
pub const PLAN_CUT: f64 = 1000.0;

type Aabb = (Vec3, Vec3);
/// Schnittebene: Punkt und Normale zum Betrachter.
type Plane = (Vec3, Vec3);

/// Abgeleitete Daten eines Wandzugs.
struct RunCache {
    id: RunId,
    chain: WallChain,
    /// Körper für 3D und Ansichten.
    solid: Solid,
    /// Körper waagerecht geschnitten (Grundriss), erst bei Bedarf berechnet.
    plan: Option<Solid>,
    /// Senkrechter Schnitt für die zuletzt gefragte Ebene.
    section: Option<(Plane, Solid)>,
    bounds: Option<Aabb>,
    /// Äußerer Wandfuß je Segment (Gummiband).
    foot: Vec<(Vec3, Vec3)>,
    /// Umschließender Quader des Wandfußes (Vortest beim Greifen).
    foot_bounds: Option<Aabb>,
    /// Wie oft dieser Zug berechnet wurde (für Tests und Messung).
    builds: u32,
}

impl RunCache {
    fn new(id: RunId, chain: WallChain, builds: u32) -> RunCache {
        let solid = chain.solid();
        let foot = chain.outer_foot();
        let foot_bounds = foot
            .iter()
            .flat_map(|&(a, b)| [a, b])
            .fold(None, |acc, p| union(acc, Some((p, p))));
        RunCache {
            id,
            plan: None,
            bounds: solid.bounds(),
            foot,
            foot_bounds,
            solid,
            chain,
            section: None,
            builds,
        }
    }

    fn view_solid(&mut self, view: ViewKind, section: Option<Plane>) -> Option<&Solid> {
        match (view, section) {
            (ViewKind::Plan, _) => {
                let chain = &self.chain;
                Some(
                    self.plan
                        .get_or_insert_with(|| chain.solid_cut_at(PLAN_CUT)),
                )
            }
            (ViewKind::Section, Some(pl)) => {
                if self.section.as_ref().is_none_or(|(p, _)| *p != pl) {
                    let (p0, n) = pl;
                    let mut s = self.solid.clipped(p0, n);
                    s.append(&self.chain.section_caps(p0, n));
                    self.section = Some((pl, s));
                }
                self.section.as_ref().map(|(_, s)| s)
            }
            (ViewKind::Section, None) => None,
            _ => Some(&self.solid),
        }
    }
}

pub struct Scene {
    model: Model,
    /// Frühere Stände für „Rückgängig“.
    undo: Vec<Model>,
    /// Rückgängig gemachte Stände für „Wiederholen“.
    redo: Vec<Model>,
    /// Zwischenspeicher je Wandzug, über den Arena-Platz der [`RunId`].
    cache: Vec<Option<RunCache>>,
    /// Wandzüge, die neu berechnet werden müssen.
    dirty: Vec<RunId>,
    all_dirty: bool,
    bounds: Option<Aabb>,
}

fn union(a: Option<Aabb>, b: Option<Aabb>) -> Option<Aabb> {
    match (a, b) {
        (Some((l0, h0)), Some((l1, h1))) => Some((
            vec3(l0.x.min(l1.x), l0.y.min(l1.y), l0.z.min(l1.z)),
            vec3(h0.x.max(h1.x), h0.y.max(h1.y), h0.z.max(h1.z)),
        )),
        (a, None) => a,
        (None, b) => b,
    }
}

/// Trifft der Strahl den Quader (Slab-Test)?
fn ray_hits_box(o: Vec3, d: Vec3, (lo, hi): Aabb) -> bool {
    let (mut t0, mut t1) = (0.0f64, f64::INFINITY);
    for (o, d, lo, hi) in [
        (o.x, d.x, lo.x, hi.x),
        (o.y, d.y, lo.y, hi.y),
        (o.z, d.z, lo.z, hi.z),
    ] {
        if d.abs() < 1e-12 {
            if o < lo || o > hi {
                return false;
            }
            continue;
        }
        let (a, b) = ((lo - o) / d, (hi - o) / d);
        t0 = t0.max(a.min(b));
        t1 = t1.min(a.max(b));
        if t0 > t1 {
            return false;
        }
    }
    true
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
            cache: Vec::new(),
            dirty: Vec::new(),
            all_dirty: true,
            bounds: None,
        };
        s.rebuild_dirty();
        s
    }

    pub fn model(&self) -> &Model {
        &self.model
    }

    /// Rechnet die markierten Wandzüge neu und entfernt gelöschte.
    fn rebuild_dirty(&mut self) {
        if self.all_dirty {
            self.all_dirty = false;
            self.dirty.clear();
            let builds: Vec<u32> = self
                .cache
                .iter()
                .map(|c| c.as_ref().map_or(0, |c| c.builds))
                .collect();
            self.cache.clear();
            let ids: Vec<RunId> = self.model.runs().ids().collect();
            for id in ids {
                let b = builds.get(id.index() as usize).copied().unwrap_or(0);
                self.store(id, b);
            }
        } else {
            for id in std::mem::take(&mut self.dirty) {
                let b = self.cached(id).map_or(0, |c| c.builds);
                self.store(id, b);
            }
        }
        self.bounds = self
            .cache
            .iter()
            .flatten()
            .fold(None, |acc, c| union(acc, c.bounds));
    }

    /// Berechnet einen Wandzug neu (oder entfernt ihn, wenn es ihn nicht mehr gibt).
    fn store(&mut self, id: RunId, builds: u32) {
        let slot = id.index() as usize;
        if self.cache.len() <= slot {
            self.cache.resize_with(slot + 1, || None);
        }
        match self.model.chain(id) {
            Some(c) => self.cache[slot] = Some(RunCache::new(id, c, builds + 1)),
            // Gelöscht: Platz nur räumen, wenn kein neuerer Zug darin steht
            None => {
                if self.cache[slot].as_ref().is_some_and(|c| c.id == id) {
                    self.cache[slot] = None;
                }
            }
        }
    }

    fn mark(&mut self, id: RunId) {
        if !self.dirty.contains(&id) {
            self.dirty.push(id);
        }
    }

    fn mark_all(&mut self) {
        self.all_dirty = true;
    }

    fn cached(&self, id: RunId) -> Option<&RunCache> {
        self.cache
            .get(id.index() as usize)?
            .as_ref()
            .filter(|c| c.id == id)
    }

    /// Wie oft ein Wandzug berechnet wurde.
    #[cfg(test)]
    pub fn build_count(&self, id: RunId) -> u32 {
        self.cached(id).map_or(0, |c| c.builds)
    }

    pub fn chain(&self, run: RunId) -> Option<&WallChain> {
        self.cached(run).map(|c| &c.chain)
    }

    /// Äußerer Wandfuß eines Wandzugs, je Segment (Anfang, Ende).
    pub fn foot(&self, run: RunId) -> Option<&[(Vec3, Vec3)]> {
        self.cached(run).map(|c| c.foot.as_slice())
    }

    /// Äußerer Wandfuß aller Wandzüge: umschließender Quader und je Segment
    /// (Anfang, Ende).
    pub fn feet(&self) -> impl Iterator<Item = (RunId, Option<Aabb>, &[(Vec3, Vec3)])> + '_ {
        self.cache
            .iter()
            .flatten()
            .map(|c| (c.id, c.foot_bounds, c.foot.as_slice()))
    }

    /// Merkt den Stand `before` als Schritt für „Rückgängig“, falls sich seitdem
    /// etwas geändert hat.
    pub fn record(&mut self, before: Model) {
        if before.revision() != self.model.revision() {
            self.undo.push(before);
            self.redo.clear();
        }
    }

    /// Stand für einen späteren [`Scene::record`] oder [`Scene::restore`].
    pub fn snapshot(&self) -> Model {
        self.model.clone()
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
        if let Some(id) = run {
            self.mark(id);
        }
        self.rebuild_dirty();
        run
    }

    /// Neue Eckpunkte eines Wandzugs ohne Verlaufseintrag (Live-Änderung beim
    /// Ziehen). Nur dieser Wandzug wird neu berechnet.
    pub fn set_run_points(&mut self, run: RunId, points: &[Vec3]) {
        if self.model.set_run_points(run, points) {
            self.mark(run);
            self.rebuild_dirty();
        }
    }

    /// Setzt das Modell ohne Verlaufseintrag zurück (Abbruch einer Live-Änderung).
    pub fn restore(&mut self, before: Model) {
        self.model.restore(before);
        self.mark_all();
        self.rebuild_dirty();
    }

    pub fn undo(&mut self) -> bool {
        match self.undo.pop() {
            Some(prev) => {
                self.redo.push(self.model.clone());
                self.model.restore(prev);
                self.mark_all();
                self.rebuild_dirty();
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
                self.mark_all();
                self.rebuild_dirty();
                true
            }
            None => false,
        }
    }

    /// Netz einer Ansicht aus allen Wandzügen außer `except` (der gerade gezogene).
    /// Beim Schnitt: Ebene `section` (Punkt, Normale zum Betrachter), alles davor
    /// wird weggeschnitten.
    pub fn mesh(
        &mut self,
        view: ViewKind,
        section: Option<Plane>,
        except: Option<RunId>,
    ) -> MeshData {
        let drawing = view != ViewKind::Persp;
        let mut m = MeshData::default();
        let model = &self.model;
        for c in self.cache.iter_mut().flatten() {
            if Some(c.id) == except {
                continue;
            }
            if let Some(s) = c.view_solid(view, section) {
                mesh_into(&mut m, s, drawing, model);
            }
        }
        m
    }

    /// Netz eines einzelnen Wandzugs (Live-Netz beim Ziehen).
    pub fn mesh_run(&mut self, view: ViewKind, section: Option<Plane>, run: RunId) -> MeshData {
        let drawing = view != ViewKind::Persp;
        let mut m = MeshData::default();
        let model = &self.model;
        if let Some(Some(c)) = self.cache.get_mut(run.index() as usize) {
            if c.id == run {
                if let Some(s) = c.view_solid(view, section) {
                    mesh_into(&mut m, s, drawing, model);
                }
            }
        }
        m
    }

    /// Umschließender Quader des Modells.
    pub fn bounds(&self) -> Option<Aabb> {
        self.bounds
    }

    /// Nächster Treffer eines Strahls: Abstand und getroffene Wand.
    pub fn raycast(&self, origin: Vec3, dir: Vec3) -> Option<(f64, ElementId)> {
        let mut best: Option<(f64, RunId, u32)> = None;
        for c in self.cache.iter().flatten() {
            if !c.bounds.is_some_and(|b| ray_hits_box(origin, dir, b)) {
                continue;
            }
            if let Some((t, seg)) = c.solid.raycast_elem(origin, dir) {
                if best.is_none_or(|b| t < b.0) {
                    best = Some((t, c.id, seg));
                }
            }
        }
        let (t, run, seg) = best?;
        Some((t, self.model.wall_at(run, seg as usize)?))
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
    let mut m = MeshData::default();
    mesh_into(&mut m, s, drawing, model);
    m
}

/// Hängt das Netz eines Körpers an `m` an.
fn mesh_into(m: &mut MeshData, s: &Solid, drawing: bool, model: &Model) {
    use theme::drawing as d;
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
    m.edges.extend(s.edges.iter().map(|e| {
        let w = match (drawing, e.kind) {
            (_, edge_kind::FINE) => d::FINE_WIDTH,
            (true, edge_kind::CUT) => d::CUT_WIDTH,
            (true, edge_kind::CUT_LAYER) => d::LAYER_CUT_WIDTH,
            (true, _) => d::VIEW_WIDTH,
            (false, _) => 1.0,
        };
        ([e.a.to_f32(), e.b.to_f32()], w)
    }));
}

#[cfg(test)]
mod tests {
    use super::*;
    use sk_model::RefSide;

    fn rechteck(x: f64) -> WallChain {
        WallChain {
            points: vec![
                vec3(x, 0.0, 0.0),
                vec3(x, 4000.0, 0.0),
                vec3(x + 5000.0, 4000.0, 0.0),
                vec3(x + 5000.0, 0.0, 0.0),
            ],
            closed: true,
            ref_side: RefSide::Left,
            layers: Vec::new(),
            height: 3500.0,
        }
    }

    fn same(a: &MeshData, b: &MeshData) -> bool {
        a.faces == b.faces && a.edges == b.edges
    }

    #[test]
    fn nur_der_geaenderte_zug_wird_neu_berechnet() {
        let mut s = Scene::with_model(Model::with_seed(1));
        let a = s.add_wall(&rechteck(0.0)).unwrap();
        let b = s.add_wall(&rechteck(10000.0)).unwrap();
        assert_eq!((s.build_count(a), s.build_count(b)), (1, 1));
        let before = s.snapshot();
        // Oberes Segment 50 cm nach außen
        let moved = s.chain(a).unwrap().with_segment_moved(1, -500.0).unwrap();
        s.set_run_points(a, &moved.points);
        s.record(before);
        assert_eq!((s.build_count(a), s.build_count(b)), (2, 1));
        // Gesamtquader folgt der Änderung
        assert!((s.bounds().unwrap().1.y - 4500.0).abs() < 1e-6);
        // Nach Rückgängig stimmt alles mit einem vollständigen Neuaufbau überein
        assert!(s.undo());
        let mut full = Scene::with_model(s.model().clone());
        for v in [ViewKind::Persp, ViewKind::Plan] {
            assert!(same(&s.mesh(v, None, None), &full.mesh(v, None, None)));
        }
        let pl = Some((vec3(0.0, 2000.0, 0.0), vec3(0.0, -1.0, 0.0)));
        assert!(same(
            &s.mesh(ViewKind::Section, pl, None),
            &full.mesh(ViewKind::Section, pl, None)
        ));
        assert!(s.model().check().is_empty());
    }

    #[test]
    fn nach_rueckgaengig_bekommt_eine_neue_wand_eine_neue_kennung() {
        let mut s = Scene::with_model(Model::with_seed(4));
        let a = s.add_wall(&rechteck(0.0)).unwrap();
        let wa = s.model().wall_at(a, 0).unwrap();
        assert!(s.undo());
        let b = s.add_wall(&rechteck(0.0)).unwrap();
        assert_ne!(a, b);
        // Die alte Wandkennung zeigt nicht auf die neue Wand
        assert!(s.model().element(wa).is_none());
        assert!(s.model().check().is_empty());
        // Wiederholen geht nach einer neuen Wand nicht mehr, Rückgängig schon
        assert!(!s.redo());
        assert!(s.undo());
        assert!(s.model().run(b).is_none());
    }

    #[test]
    fn ruhendes_und_live_netz_ergeben_das_ganze() {
        let mut s = Scene::with_model(Model::with_seed(2));
        let a = s.add_wall(&rechteck(0.0)).unwrap();
        s.add_wall(&rechteck(10000.0)).unwrap();
        let whole = s.mesh(ViewKind::Persp, None, None);
        let rest = s.mesh(ViewKind::Persp, None, Some(a));
        let live = s.mesh_run(ViewKind::Persp, None, a);
        assert_eq!(whole.faces.len(), rest.faces.len() + live.faces.len());
        assert_eq!(whole.edges.len(), rest.edges.len() + live.edges.len());
    }

    #[test]
    fn strahl_liefert_die_wand() {
        let mut s = Scene::with_model(Model::with_seed(3));
        s.add_wall(&rechteck(10000.0)).unwrap();
        let a = s.add_wall(&rechteck(0.0)).unwrap();
        // Segment 2 läuft bei x = 5000 von y = 4000 nach 0; von außen (+x) treffen
        let hit = s.raycast(vec3(9000.0, 2000.0, 1000.0), vec3(-1.0, 0.0, 0.0));
        let (t, wall) = hit.unwrap();
        assert!((t - 4000.0).abs() < 1e-6, "{t}");
        assert_eq!(Some(wall), s.model().wall_at(a, 2));
        assert_eq!(s.model().element(wall).unwrap().number, "AW-007");
        // Am Modell vorbei: kein Treffer
        assert!(s
            .raycast(vec3(0.0, -9000.0, 5000.0), vec3(0.0, 0.0, 1.0))
            .is_none());
    }
}
