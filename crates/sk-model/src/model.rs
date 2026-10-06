//! Das Gebäudemodell als Datenbank: Bibliothek, Geschosse, Bauteile, Wandzüge.
//!
//! Bauteile verweisen nur über Kennungen aufeinander, nie über Vec-Plätze.
//! Geometrie wird aus der Parametrik abgeleitet ([`Model::chain`]) und nie
//! gespeichert. Jede Änderung erhöht die Revision.

use crate::attr::{
    self, Attributes, Display, Fill, FillId, LineType, LineTypeId, Pen, PenId, Surface, SurfaceId,
};
use crate::element::{
    Category, Element, ElementId, ElementKind, PropSet, PropValue, RunId, Storey, StoreyId, Wall,
    WallRun,
};
use crate::guid::{Guid, GuidGen};
use crate::id::Arena;
use crate::join::{self, Join, JoinEnd, JoinKind};
use crate::library::{
    material_key, LayerFunction, LayerSet, LayerSetId, MatCategory, Material, MaterialId,
    MaterialLayer,
};
use crate::solid::material;
use crate::txn::{Change, Direction, Key, Open, Touched, Txn};
use crate::wall::{clean_points, cross2, segment_count, EndCut, Layer, RefSide, WallChain};
use sk_math::{vec3, Vec3};

/// Voreinstellungen für neue Bauteile.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Defaults {
    pub storey: StoreyId,
    pub exterior_wall: LayerSetId,
    pub interior_wall: LayerSetId,
}

/// Das Projekt (IFC: IfcProject).
#[derive(Clone, Debug, PartialEq)]
pub struct Project {
    pub guid: Guid,
    pub name: String,
}

/// Fehler beim Umbenennen eines Bauteils.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NumberError {
    Empty,
    Taken(ElementId),
    NoElement,
}

#[derive(Clone, Debug)]
pub struct Model {
    project: Project,
    /// Stifte, Schraffuren, Oberflächen und Bauteildarstellung.
    attr: Attributes,
    materials: Arena<Material>,
    layer_sets: Arena<LayerSet>,
    storeys: Arena<Storey>,
    elements: Arena<Element>,
    runs: Arena<WallRun>,
    /// Anschlüsse zwischen Wandzügen, aus den Punkten abgeleitet (B5a).
    joins: Vec<Join>,
    defaults: Defaults,
    /// Zuletzt vergebene laufende Nummer je Kategorie (wird nie zurückgesetzt).
    numbers: [u32; Category::ALL.len()],
    revision: u64,
    guids: GuidGen,
    /// Offener Schritt für Rückgängig ([`Model::begin`]).
    txn: Option<Open>,
    /// Jede Änderung muss in einem Schritt liegen (in der App; in Tests nicht).
    strict: bool,
}

/// Merkt den Stand eines Datensatzes vor seiner ersten Änderung im offenen
/// Schritt; `new`: eben angelegt, vorher gab es ihn nicht.
macro_rules! note {
    ($self:ident, $variant:ident, new $id:expr) => {
        note!(@push $self, $variant, $id, |_| None)
    };
    ($self:ident, $variant:ident, $arena:expr, $id:expr) => {
        note!(@push $self, $variant, $id, |id| $arena.get(id).cloned())
    };
    (@push $self:ident, $variant:ident, $id:expr, $old:expr) => {{
        let id = $id;
        match $self.txn.as_mut() {
            Some(t) => {
                if t.noted.insert(Key::$variant(id)) {
                    #[allow(clippy::redundant_closure_call)]
                    let old = ($old)(id);
                    t.changes.push(Change::$variant { id, old, new: None });
                }
            }
            None => debug_assert!(!$self.strict, "Änderung ohne Schritt"),
        }
    }};
}

impl Default for Model {
    fn default() -> Model {
        Model::new()
    }
}

impl Model {
    /// Leeres Modell mit Startbibliothek und Erdgeschoss.
    pub fn new() -> Model {
        Model::standard(GuidGen::from_time())
    }

    /// Wie [`Model::new`], aber mit festem Startwert für die Guids (Tests).
    pub fn with_seed(seed: u64) -> Model {
        Model::standard(GuidGen::with_seed(seed))
    }

    fn standard(mut guids: GuidGen) -> Model {
        let (mut attr, st) = attr::defaults(&mut guids);
        let mut materials = Arena::new();
        let mut mat = |name: &str, category, priority, density, cut_fill, color, cut_color| {
            let surface = attr.add_surface(Surface {
                guid: guids.next_guid(),
                name: name.into(),
                color,
                cut_color,
            });
            materials.insert(Material {
                guid: guids.next_guid(),
                name: name.into(),
                category,
                priority,
                density,
                lambda: None,
                cut_fill,
                cut_fg: st.hatch_pen,
                cut_bg: st.background,
                surface,
            })
        };
        use MatCategory as C;
        let aerated = mat(
            "Gasbeton",
            C::Masonry,
            800,
            350.0,
            st.masonry,
            [238, 237, 232],
            [176, 177, 174],
        );
        let insulation = mat(
            "Dämmung (WDVS)",
            C::Insulation,
            300,
            20.0,
            st.insulation,
            [244, 239, 220],
            [232, 196, 92],
        );
        mat(
            "Stahlbeton",
            C::Concrete,
            900,
            2500.0,
            st.masonry,
            [214, 214, 210],
            [150, 150, 148],
        );
        mat(
            "Putz",
            C::Plaster,
            100,
            1400.0,
            st.empty,
            [240, 238, 232],
            [200, 198, 192],
        );
        let mut layer_sets = Arena::new();
        let exterior_wall = layer_sets.insert(LayerSet {
            guid: guids.next_guid(),
            name: "AW 31,5 Gasbeton + WDVS".into(),
            layers: vec![
                MaterialLayer {
                    material: insulation,
                    thickness: 140.0,
                    function: LayerFunction::Insulation,
                    core: false,
                },
                MaterialLayer {
                    material: aerated,
                    thickness: 175.0,
                    function: LayerFunction::Structure,
                    core: true,
                },
            ],
        });
        let mut storeys = Arena::new();
        let storey = storeys.insert(Storey {
            guid: guids.next_guid(),
            name: "EG".into(),
            elevation: 0.0,
            height: 3500.0,
        });
        let project = Project {
            guid: guids.next_guid(),
            name: "Projekt".into(),
        };
        // Nach der Projekt-Guid angelegt, damit die älteren Guids gleich bleiben
        let interior_wall = layer_sets.insert(interior_set(guids.next_guid(), aerated));
        Model {
            project,
            attr,
            materials,
            layer_sets,
            storeys,
            elements: Arena::new(),
            runs: Arena::new(),
            joins: Vec::new(),
            defaults: Defaults {
                storey,
                exterior_wall,
                interior_wall,
            },
            numbers: [0; Category::ALL.len()],
            revision: 0,
            guids,
            txn: None,
            strict: false,
        }
    }

    /// Setzt ein Modell aus geladenen Tabellen zusammen ([`crate::szo`]).
    /// Nummernzähler je Kategorie stehen auf der höchsten vorhandenen Nummer.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn from_parts(
        project: Project,
        attr: Attributes,
        materials: Arena<Material>,
        layer_sets: Arena<LayerSet>,
        storeys: Arena<Storey>,
        elements: Arena<Element>,
        runs: Arena<WallRun>,
        defaults: Defaults,
        guids: GuidGen,
    ) -> Model {
        let mut numbers = [0; Category::ALL.len()];
        for (_, e) in elements.iter() {
            let n = e
                .number
                .strip_prefix(e.category.prefix())
                .and_then(|r| r.strip_prefix('-'))
                .and_then(|r| r.parse::<u32>().ok());
            if let Some(n) = n {
                let c = &mut numbers[e.category.index()];
                *c = (*c).max(n);
            }
        }
        let mut m = Model {
            project,
            attr,
            materials,
            layer_sets,
            storeys,
            elements,
            runs,
            joins: Vec::new(),
            defaults,
            numbers,
            revision: 0,
            guids,
            txn: None,
            strict: false,
        };
        m.joins = m.detect_all();
        m
    }

    pub fn project(&self) -> &Project {
        &self.project
    }

    /// Steigt bei jeder Änderung.
    pub fn revision(&self) -> u64 {
        self.revision
    }

    fn touch(&mut self) {
        self.revision += 1;
    }

    /// Neue, noch nie vergebene Guid.
    pub fn new_guid(&mut self) -> Guid {
        self.guids.next_guid()
    }

    pub fn defaults(&self) -> &Defaults {
        &self.defaults
    }

    // --- Darstellung ------------------------------------------------------

    pub fn attr(&self) -> &Attributes {
        &self.attr
    }

    pub fn add_pen(&mut self, p: Pen) -> PenId {
        self.touch();
        let id = self.attr.add_pen(p);
        note!(self, Pen, new id);
        id
    }

    pub fn add_line_type(&mut self, l: LineType) -> LineTypeId {
        self.touch();
        let id = self.attr.add_line_type(l);
        note!(self, LineType, new id);
        id
    }

    pub fn add_fill(&mut self, f: Fill) -> FillId {
        self.touch();
        let id = self.attr.add_fill(f);
        note!(self, Fill, new id);
        id
    }

    pub fn add_surface(&mut self, s: Surface) -> SurfaceId {
        self.touch();
        let id = self.attr.add_surface(s);
        note!(self, Surface, new id);
        id
    }

    pub fn set_pen(&mut self, id: PenId, p: Pen) -> bool {
        note!(self, Pen, self.attr.pens(), id);
        let ok = self.attr.set_pen(id, p);
        self.revision += ok as u64;
        ok
    }

    pub fn set_fill(&mut self, id: FillId, f: Fill) -> bool {
        note!(self, Fill, self.attr.fills(), id);
        let ok = self.attr.set_fill(id, f);
        self.revision += ok as u64;
        ok
    }

    pub fn set_surface(&mut self, id: SurfaceId, s: Surface) -> bool {
        note!(self, Surface, self.attr.surfaces(), id);
        let ok = self.attr.set_surface(id, s);
        self.revision += ok as u64;
        ok
    }

    pub fn set_display(&mut self, d: Display) {
        match self.txn.as_mut() {
            Some(t) => {
                if t.noted.insert(Key::Display) {
                    t.changes.push(Change::Display {
                        old: self.attr.display().clone(),
                        new: d.clone(),
                    });
                }
            }
            None => debug_assert!(!self.strict, "Änderung ohne Schritt"),
        }
        self.touch();
        self.attr.set_display(d);
    }

    // --- Bibliothek -------------------------------------------------------

    pub fn materials(&self) -> &Arena<Material> {
        &self.materials
    }

    pub fn material(&self, id: MaterialId) -> Option<&Material> {
        self.materials.get(id)
    }

    /// Baustoff zu einem Darstellungsschlüssel aus einem Körper (Schnittbit egal).
    pub fn material_by_key(&self, key: u16) -> Option<&Material> {
        let key = key & !material::CUT;
        if key == material::PLAIN {
            return None;
        }
        self.materials.at_index(key as u32 - 1).map(|(_, m)| m)
    }

    pub fn add_material(&mut self, m: Material) -> MaterialId {
        self.touch();
        let id = self.materials.insert(m);
        note!(self, Material, new id);
        id
    }

    pub fn layer_sets(&self) -> &Arena<LayerSet> {
        &self.layer_sets
    }

    pub fn layer_set(&self, id: LayerSetId) -> Option<&LayerSet> {
        self.layer_sets.get(id)
    }

    pub fn add_layer_set(&mut self, s: LayerSet) -> LayerSetId {
        self.touch();
        let id = self.layer_sets.insert(s);
        note!(self, LayerSet, new id);
        id
    }

    /// Ändert einen Aufbau; alle Bauteile dieses Typs folgen.
    pub fn set_layer_set(&mut self, id: LayerSetId, s: LayerSet) -> bool {
        if self.layer_sets.contains(id) {
            note!(self, LayerSet, self.layer_sets, id);
        }
        match self.layer_sets.get_mut(id) {
            Some(old) => {
                *old = s;
                // Andere Dicken: Anschlüsse neu erkennen
                self.joins = self.detect_all();
                self.touch();
                true
            }
            None => false,
        }
    }

    /// Schichten eines Aufbaus für die Geometrie (Darstellungsschlüssel statt Kennung).
    pub fn wall_layers(&self, id: LayerSetId) -> Vec<Layer> {
        self.layer_set(id).map_or_else(Vec::new, |s| {
            s.layers
                .iter()
                .map(|l| Layer {
                    thickness: l.thickness,
                    material: material_key(l.material),
                    core: l.core,
                })
                .collect()
        })
    }

    // --- Geschosse --------------------------------------------------------

    pub fn storeys(&self) -> &Arena<Storey> {
        &self.storeys
    }

    pub fn storey(&self, id: StoreyId) -> Option<&Storey> {
        self.storeys.get(id)
    }

    // --- Bauteile ---------------------------------------------------------

    pub fn elements(&self) -> &Arena<Element> {
        &self.elements
    }

    pub fn element(&self, id: ElementId) -> Option<&Element> {
        self.elements.get(id)
    }

    pub fn element_by_number(&self, number: &str) -> Option<ElementId> {
        self.elements
            .iter()
            .find(|(_, e)| e.number == number)
            .map(|(id, _)| id)
    }

    /// Nächste freie Nummer der Kategorie, z. B. „AW-004“. Nummern werden nie
    /// wiederverwendet, auch nicht nach Löschen oder Rückgängig.
    fn next_number(&mut self, category: Category) -> String {
        loop {
            let n = &mut self.numbers[category.index()];
            *n += 1;
            let s = format!("{}-{:03}", category.prefix(), n);
            if self.element_by_number(&s).is_none() {
                return s;
            }
        }
    }

    /// Benennt ein Bauteil um; die Nummer muss im Modell eindeutig sein.
    pub fn set_number(&mut self, id: ElementId, number: &str) -> Result<(), NumberError> {
        let number = number.trim();
        if number.is_empty() {
            return Err(NumberError::Empty);
        }
        match self.element_by_number(number) {
            Some(other) if other == id => return Ok(()),
            Some(other) => return Err(NumberError::Taken(other)),
            None => {}
        }
        if !self.elements.contains(id) {
            return Err(NumberError::NoElement);
        }
        note!(self, Element, self.elements, id);
        let e = self.elements.get_mut(id).ok_or(NumberError::NoElement)?;
        e.number = number.to_string();
        self.touch();
        Ok(())
    }

    /// Setzt eine freie Eigenschaft; `None` entfernt sie.
    pub fn set_prop(&mut self, id: ElementId, key: &str, value: Option<PropValue>) -> bool {
        if !self.elements.contains(id) {
            return false;
        }
        note!(self, Element, self.elements, id);
        let Some(e) = self.elements.get_mut(id) else {
            return false;
        };
        match value {
            Some(v) => {
                e.props.insert(key.to_string(), v);
            }
            None => {
                e.props.remove(key);
            }
        }
        self.touch();
        true
    }

    fn new_wall(&mut self, run: RunId, seg: usize, template: &Element) -> ElementId {
        let guid = self.new_guid();
        let number = self.next_number(template.category);
        let id = self.elements.insert(Element {
            guid,
            number,
            kind: ElementKind::Wall(Wall {
                run,
                seg: seg as u32,
            }),
            ..template.clone()
        });
        note!(self, Element, new id);
        id
    }

    // --- Wandzüge ---------------------------------------------------------

    pub fn runs(&self) -> &Arena<WallRun> {
        &self.runs
    }

    pub fn run(&self, id: RunId) -> Option<&WallRun> {
        self.runs.get(id)
    }

    /// Legt einen Wandzug an, mit einem Wand-Bauteil je Segment. `None`, wenn
    /// die Punkte kein Segment ergeben.
    pub fn add_wall_run(
        &mut self,
        points: &[Vec3],
        closed: bool,
        ref_side: RefSide,
        height: f64,
        layer_set: LayerSetId,
        category: Category,
    ) -> Option<RunId> {
        let pts = clean_points(points, closed);
        let closed = closed && pts.len() >= 3;
        let count = segment_count(pts.len(), closed);
        if count == 0 || !self.layer_sets.contains(layer_set) {
            return None;
        }
        let storey = self.defaults.storey;
        let guid = self.new_guid();
        let run = self.runs.insert(WallRun {
            guid,
            points: pts,
            closed,
            ref_side,
            height,
            storey,
            segments: Vec::new(),
        });
        note!(self, Run, new run);
        let template = Element {
            guid: Guid(0),
            number: String::new(),
            category,
            storey,
            layer_set: Some(layer_set),
            kind: ElementKind::Wall(Wall { run, seg: 0 }),
            props: PropSet::new(),
        };
        let segments = (0..count)
            .map(|k| self.new_wall(run, k, &template))
            .collect();
        self.runs.get_mut(run)?.segments = segments;
        self.update_joins(&[run]);
        self.touch();
        Some(run)
    }

    /// Setzt neue Eckpunkte eines Wandzugs. Bleibt die Segmentzahl gleich, behält
    /// jede Wand ihren Platz (Verschieben). Ändert sie sich, behalten die Wände
    /// ihre Kennung nach Lage (Regel 11): ein geteiltes Segment gibt sie an seinen
    /// längeren Teil weiter, ein weggefallenes Segment nimmt genau seine mit.
    ///
    /// Angeschlossene Züge werden im selben Schritt mitgeführt (B5a). Liefert
    /// alle Züge, deren Körper sich dadurch ändern kann, `None` bei ungültigen
    /// Punkten.
    pub fn set_run_points(&mut self, id: RunId, points: &[Vec3]) -> Option<Vec<RunId>> {
        if !self.set_points(id, points) {
            return None;
        }
        let moved = self.follow(id);
        let mut out = moved.clone();
        let partners = |m: &Model, out: &mut Vec<RunId>| {
            for r in &moved {
                for p in m.joined_runs(*r) {
                    if !out.contains(&p) {
                        out.push(p);
                    }
                }
            }
        };
        partners(self, &mut out);
        self.update_joins(&moved);
        partners(self, &mut out);
        Some(out)
    }

    /// [`Model::set_run_points`] ohne Mitführen und ohne Anschlüsse.
    fn set_points(&mut self, id: RunId, points: &[Vec3]) -> bool {
        let Some(run) = self.runs.get(id) else {
            return false;
        };
        let pts = clean_points(points, run.closed);
        let closed = run.closed && pts.len() >= 3;
        let count = segment_count(pts.len(), closed);
        if count == 0 {
            return false;
        }
        let old = run.segments.clone();
        note!(self, Run, self.runs, id);
        let matched = if old.len() == count {
            (0..count).map(Some).collect()
        } else {
            match_segments(
                &segment_lines(&run.points, run.closed),
                &segment_lines(&pts, closed),
            )
        };
        let template = old.first().and_then(|e| self.elements.get(*e)).cloned();
        let mut kept = vec![false; old.len()];
        let mut segments = Vec::with_capacity(count);
        for (k, m) in matched.into_iter().enumerate() {
            match (m, &template) {
                (Some(o), _) => {
                    kept[o] = true;
                    let e = old[o];
                    if self.segment_of(e).map(|s| s.1) != Some(k) {
                        note!(self, Element, self.elements, e);
                    }
                    if let Some(ElementKind::Wall(w)) =
                        self.elements.get_mut(e).map(|el| &mut el.kind)
                    {
                        w.seg = k as u32;
                    }
                    segments.push(e);
                }
                (None, Some(t)) => segments.push(self.new_wall(id, k, t)),
                (None, None) => return false,
            }
        }
        for (e, keep) in old.into_iter().zip(kept) {
            if !keep {
                note!(self, Element, self.elements, e);
                self.elements.remove(e);
            }
        }
        let Some(run) = self.runs.get_mut(id) else {
            return false;
        };
        run.points = pts;
        run.closed = closed;
        run.segments = segments;
        self.touch();
        true
    }

    /// Entfernt einen Wandzug mit allen seinen Wänden.
    pub fn remove_run(&mut self, id: RunId) -> bool {
        if !self.runs.contains(id) {
            return false;
        }
        note!(self, Run, self.runs, id);
        let Some(run) = self.runs.remove(id) else {
            return false;
        };
        for e in run.segments {
            note!(self, Element, self.elements, e);
            self.elements.remove(e);
        }
        self.update_joins(&[id]);
        self.touch();
        true
    }

    /// Wandzug und Segment einer Wand.
    pub fn segment_of(&self, wall: ElementId) -> Option<(RunId, usize)> {
        match self.element(wall)?.kind {
            ElementKind::Wall(w) => Some((w.run, w.seg as usize)),
        }
    }

    /// Wand zu Segment `seg` eines Wandzugs.
    pub fn wall_at(&self, run: RunId, seg: usize) -> Option<ElementId> {
        self.run(run)?.segments.get(seg).copied()
    }

    /// Geometrie eines Wandzugs mit den Schichten seines Aufbaus und seinen
    /// Anschlüssen an andere Züge. Haben die Wände eines Zuges verschiedene
    /// Aufbauten, gilt der des ersten Segments.
    pub fn chain(&self, id: RunId) -> Option<WallChain> {
        let mut c = self.base_chain(id)?;
        let prio = |k: u16| self.material_by_key(k).map_or(0, |m| m.priority);
        for j in &self.joins {
            if j.a_run == id {
                let cut = match (j.kind, j.b_end) {
                    (JoinKind::L, Some(eb)) => self
                        .base_chain(j.b_run)
                        .and_then(|b| join::l_miter(&c, j.a_end, &b, eb))
                        .map(EndCut::Miter),
                    _ => self
                        .segment_of(j.b)
                        .zip(self.base_chain(j.b_run))
                        .and_then(|((_, s), h)| join::t_cut(&c, j.a_end, &h, s, &prio))
                        .map(|x| EndCut::Layers(x.0)),
                };
                if let Some(cut) = cut {
                    c.joints.ends[j.a_end.index()] = cut;
                }
            }
            if j.b_run == id && j.kind == JoinKind::T {
                let gaps = self
                    .segment_of(j.b)
                    .zip(self.base_chain(j.a_run))
                    .and_then(|((_, s), a)| join::t_cut(&a, j.a_end, &c, s, &prio));
                if let Some((_, g)) = gaps {
                    c.joints.gaps.extend(g);
                }
            }
        }
        Some(c)
    }

    /// Geometrie eines Wandzugs ohne Anschlüsse.
    fn base_chain(&self, id: RunId) -> Option<WallChain> {
        let run = self.run(id)?;
        let set = run
            .segments
            .first()
            .and_then(|e| self.element(*e))
            .and_then(|e| e.layer_set)?;
        Some(WallChain {
            points: run.points.clone(),
            closed: run.closed,
            ref_side: run.ref_side,
            layers: self.wall_layers(set),
            height: run.height,
            joints: Default::default(),
        })
    }

    /// Alle Wandzüge als Geometrie.
    pub fn chains(&self) -> impl Iterator<Item = (RunId, WallChain)> + '_ {
        self.runs
            .ids()
            .filter_map(|id| self.chain(id).map(|c| (id, c)))
    }

    // --- Anschlüsse (B5a) -------------------------------------------------

    /// Anschlüsse zwischen Wandzügen, aus den Punkten abgeleitet.
    pub fn joins(&self) -> &[Join] {
        &self.joins
    }

    /// Züge, die über einen Anschluss mit `id` verbunden sind.
    pub fn joined_runs(&self, id: RunId) -> Vec<RunId> {
        let mut out = Vec::new();
        for j in &self.joins {
            let other = match (j.a_run == id, j.b_run == id) {
                (true, false) => j.b_run,
                (false, true) => j.a_run,
                _ => continue,
            };
            if !out.contains(&other) {
                out.push(other);
            }
        }
        out
    }

    /// Zug ohne Anschlüsse, mit Grundriss je Segment.
    fn base(&self, run: RunId) -> Option<Base> {
        let chain = self.base_chain(run)?;
        let foot = join::footprints(&chain);
        let mut lo = vec3(f64::INFINITY, f64::INFINITY, 0.0);
        let mut hi = -lo;
        for p in foot.iter().flatten() {
            lo = vec3(lo.x.min(p.x), lo.y.min(p.y), 0.0);
            hi = vec3(hi.x.max(p.x), hi.y.max(p.y), 0.0);
        }
        Some(Base {
            run,
            chain,
            segments: self.run(run)?.segments.clone(),
            foot,
            lo,
            hi,
        })
    }

    /// Grober Rahmen je Zug, den sein Grundriss samt Fangabstand nicht
    /// verlässt: Eckpunkte plus Reichweite der Gehrungen. Billiger Vortest.
    fn rough_boxes(&self) -> Vec<(RunId, Vec3, Vec3)> {
        self.runs
            .iter()
            .map(|(id, r)| {
                let t: f64 = r
                    .segments
                    .first()
                    .and_then(|e| self.element(*e))
                    .and_then(|e| e.layer_set)
                    .and_then(|s| self.layer_set(s))
                    .map_or(0.0, |s| s.layers.iter().map(|l| l.thickness).sum());
                let m = t * 9.0 + join::SNAP;
                let mut lo = vec3(f64::INFINITY, f64::INFINITY, 0.0);
                let mut hi = -lo;
                for q in &r.points {
                    lo = vec3(lo.x.min(q.x), lo.y.min(q.y), 0.0);
                    hi = vec3(hi.x.max(q.x), hi.y.max(q.y), 0.0);
                }
                (id, lo - vec3(m, m, 0.0), hi + vec3(m, m, 0.0))
            })
            .collect()
    }

    /// Freies Ende eines offenen Zuges (gespeicherte Punkte sind bereinigt).
    fn end_point(r: &WallRun, e: JoinEnd) -> Option<Vec3> {
        if r.closed || r.points.len() < 2 {
            return None;
        }
        match e {
            JoinEnd::Start => r.points.first().copied(),
            JoinEnd::End => r.points.last().copied(),
        }
    }

    /// Anschluss des freien Endes `e` von Zug `run`, frisch aus den Punkten.
    fn detect_end(&self, run: RunId, e: JoinEnd, boxes: &[(RunId, Vec3, Vec3)]) -> Option<Join> {
        let p = Model::end_point(self.runs.get(run)?, e)?;
        let a = self.base(run)?;
        let cands: Vec<Base> = boxes
            .iter()
            .filter(|(id, lo, hi)| *id != run && inside(p, *lo, *hi))
            .filter_map(|(id, _, _)| self.base(*id))
            .collect();
        Model::detect(&a, e, &cands)
    }

    /// Erkennt den Anschluss des freien Endes `e` von `a`: L, wenn ein freies
    /// Ende eines anderen Zuges höchstens [`join::SNAP`] entfernt liegt, sonst
    /// T an das nächste nicht parallele Segment, in dessen Grundriss oder
    /// höchstens [`join::SNAP`] vor dessen Fläche das Ende liegt.
    fn detect(a: &Base, e: JoinEnd, all: &[Base]) -> Option<Join> {
        let (p, da) = a.chain.end_frame(e.index())?;
        let elem = |b: &Base, e: JoinEnd| match e {
            JoinEnd::Start => b.segments.first().copied(),
            JoinEnd::End => b.segments.last().copied(),
        };
        let others = || all.iter().filter(|b| b.run != a.run);
        let mut l: Option<(f64, &Base, JoinEnd, Vec3, Vec3)> = None;
        for b in others() {
            for eb in JoinEnd::BOTH {
                let Some((q, db)) = b.chain.end_frame(eb.index()) else {
                    continue;
                };
                let d = (q - p).length();
                if d <= join::SNAP && l.as_ref().is_none_or(|x| d < x.0) {
                    l = Some((d, b, eb, q, db));
                }
            }
        }
        if let Some((_, b, eb, q, db)) = l {
            return Some(Join {
                a: elem(a, e)?,
                a_end: e,
                b: elem(b, eb)?,
                b_end: Some(eb),
                kind: JoinKind::L,
                a_run: a.run,
                b_run: b.run,
                anchor: crate::wall::Line2 { p: q, d: db },
            });
        }
        let mut t: Option<(f64, f64, &Base, usize)> = None;
        for b in others() {
            if p.x < b.lo.x - join::SNAP
                || p.y < b.lo.y - join::SNAP
                || p.x > b.hi.x + join::SNAP
                || p.y > b.hi.y + join::SNAP
            {
                continue;
            }
            for (k, f) in b.foot.iter().enumerate() {
                let d = join::quad_distance(f, p);
                let Some((_, db)) = b.chain.segment_frame(k) else {
                    continue;
                };
                let sin = join::sin_between(da, db);
                if d > join::SNAP || sin < join::MIN_SIN {
                    continue;
                }
                let better = t
                    .as_ref()
                    .is_none_or(|x| d < x.0 - 1e-6 || (d <= x.0 + 1e-6 && sin > x.1));
                if better {
                    t = Some((d, sin, b, k));
                }
            }
        }
        let (_, _, b, k) = t?;
        Some(Join {
            a: elem(a, e)?,
            a_end: e,
            b: *b.segments.get(k)?,
            b_end: None,
            kind: JoinKind::T,
            a_run: a.run,
            b_run: b.run,
            anchor: join::facing_face(&a.chain, e, &b.chain, k)?,
        })
    }

    /// Alle Anschlüsse frisch aus den Punkten.
    fn detect_all(&self) -> Vec<Join> {
        let boxes = self.rough_boxes();
        let mut out: Vec<Join> = self
            .runs
            .ids()
            .flat_map(|r| JoinEnd::BOTH.map(|e| self.detect_end(r, e, &boxes)))
            .flatten()
            .collect();
        sort_joins(&mut out);
        out
    }

    /// Erkennt die Anschlüsse neu, die sich durch Änderungen an `runs` ändern
    /// können: freie Enden dieser Züge, Enden, die an ihnen hängen, und freie
    /// Enden anderer Züge in ihrer Nähe.
    fn update_joins(&mut self, runs: &[RunId]) {
        let boxes = self.rough_boxes();
        let changed: Vec<&(RunId, Vec3, Vec3)> =
            boxes.iter().filter(|b| runs.contains(&b.0)).collect();
        let mut ends: Vec<(RunId, JoinEnd)> = Vec::new();
        for (id, r) in self.runs.iter() {
            for e in JoinEnd::BOTH {
                let Some(p) = Model::end_point(r, e) else {
                    continue;
                };
                if runs.contains(&id) || changed.iter().any(|c| inside(p, c.1, c.2)) {
                    ends.push((id, e));
                }
            }
        }
        for j in &self.joins {
            if runs.contains(&j.a_run) || runs.contains(&j.b_run) {
                ends.push((j.a_run, j.a_end));
            }
        }
        ends.sort_by_key(|(r, e)| (r.index(), *e));
        ends.dedup();
        let fresh: Vec<Join> = ends
            .iter()
            .filter_map(|&(r, e)| self.detect_end(r, e, &boxes))
            .collect();
        self.joins.retain(|j| !ends.contains(&(j.a_run, j.a_end)));
        self.joins.extend(fresh);
        sort_joins(&mut self.joins);
    }

    /// Führt die Züge mit, die an `root` hängen, und weiter die an diesen
    /// (B5a, Abschnitt 3): ein T-Ende entlang seiner Richtung auf die neue
    /// zugewandte Wirtsfläche, ein L-Ende auf den neuen Endpunkt des Partners.
    /// Liefert `root` und alle bewegten Züge.
    fn follow(&mut self, root: RunId) -> Vec<RunId> {
        let mut moved = vec![root];
        let mut k = 0;
        while k < moved.len() {
            let r = moved[k];
            k += 1;
            let deps: Vec<Join> = self
                .joins
                .iter()
                .filter(|j| j.b_run == r && j.a_run != root)
                .cloned()
                .collect();
            for j in deps {
                let Some(p) = self.follow_target(&j) else {
                    continue;
                };
                if self.set_end_point(j.a_run, j.a_end, p) && !moved.contains(&j.a_run) {
                    moved.push(j.a_run);
                }
            }
        }
        moved
    }

    /// Neue Lage des Endes `j.a_end`, wenn sich der Partner seit dem Erkennen
    /// bewegt hat.
    fn follow_target(&self, j: &Join) -> Option<Vec3> {
        let a = self.base_chain(j.a_run)?;
        let (p, da) = a.end_frame(j.a_end.index())?;
        match j.kind {
            JoinKind::L => {
                let (q, _) = self.base_chain(j.b_run)?.end_frame(j.b_end?.index())?;
                ((q - j.anchor.p).length() > 1e-6).then_some(q)
            }
            JoinKind::T => {
                let (run, seg) = self.segment_of(j.b)?;
                let face = join::facing_face(&a, j.a_end, &self.base_chain(run)?, seg)?;
                let (o, d) = (j.anchor, face.d);
                let same = cross2(d, o.d).abs() < 1e-9
                    && d.dot(o.d) > 0.0
                    && cross2(o.p - face.p, d).abs() < 1e-6;
                if same {
                    return None;
                }
                face.meet(p, da)
            }
        }
    }

    /// Legt das freie Ende `e` eines offenen Zuges auf `p`.
    fn set_end_point(&mut self, run: RunId, e: JoinEnd, p: Vec3) -> bool {
        let Some(r) = self.runs.get(run).filter(|r| !r.closed) else {
            return false;
        };
        let mut pts = r.points.clone();
        let i = match e {
            JoinEnd::Start => 0,
            JoinEnd::End => pts.len() - 1,
        };
        if (pts[i] - vec3(p.x, p.y, pts[i].z)).length() < 1e-9 {
            return false;
        }
        pts[i] = vec3(p.x, p.y, pts[i].z);
        self.set_points(run, &pts)
    }

    // --- Rückgängig -------------------------------------------------------

    /// Verlangt ab jetzt für jede Änderung einen offenen Schritt (App).
    pub fn require_steps(&mut self) {
        self.strict = true;
    }

    /// Öffnet einen Schritt. Es darf keiner offen sein ([`Model::commit`] vorher).
    pub fn begin(&mut self, label: &'static str) {
        debug_assert!(self.txn.is_none(), "Schritt schon offen");
        self.txn = Some(Open {
            label,
            changes: Vec::new(),
            noted: Default::default(),
        });
    }

    pub fn in_step(&self) -> bool {
        self.txn.is_some()
    }

    /// Schließt den offenen Schritt. `None`, wenn er nichts geändert hat.
    pub fn commit(&mut self) -> Option<Txn> {
        let open = self.txn.take()?;
        let mut changes = open.changes;
        for c in &mut changes {
            self.fill_new(c);
        }
        changes.retain(|c| !c.is_noop());
        (!changes.is_empty()).then_some(Txn {
            label: open.label,
            changes,
        })
    }

    /// Verwirft den offenen Schritt und stellt den Stand bei [`Model::begin`]
    /// wieder her (Esc beim Ziehen).
    pub fn rollback(&mut self) -> Touched {
        match self.commit() {
            Some(t) => self.apply(&t, Direction::Undo),
            None => Touched::default(),
        }
    }

    /// Trägt den heutigen Stand als „nachher“ ein.
    fn fill_new(&self, c: &mut Change) {
        match c {
            Change::Run { id, new, .. } => *new = self.runs.get(*id).cloned(),
            Change::Element { id, new, .. } => *new = self.elements.get(*id).cloned(),
            Change::LayerSet { id, new, .. } => *new = self.layer_sets.get(*id).cloned(),
            Change::Material { id, new, .. } => *new = self.materials.get(*id).cloned(),
            Change::Storey { id, new, .. } => *new = self.storeys.get(*id).cloned(),
            Change::Pen { id, new, .. } => *new = self.attr.pen(*id).cloned(),
            Change::LineType { id, new, .. } => *new = self.attr.line_type(*id).cloned(),
            Change::Fill { id, new, .. } => *new = self.attr.fill(*id).cloned(),
            Change::Surface { id, new, .. } => *new = self.attr.surface(*id).cloned(),
            Change::Display { new, .. } => *new = self.attr.display().clone(),
        }
    }

    /// Macht einen Schritt rückgängig oder wiederholt ihn. Kennungen bleiben
    /// erhalten; Guid-Erzeuger und Nummernzähler laufen weiter, die Revision steigt.
    pub fn apply(&mut self, t: &Txn, dir: Direction) -> Touched {
        debug_assert!(self.txn.is_none(), "Rückgängig in einem offenen Schritt");
        let mut touched = Touched::default();
        let mut apply_one = |m: &mut Model, c: &Change| match c {
            Change::Run { id, old, new } => {
                m.runs.set(*id, pick(dir, old, new));
                touched.run(*id);
            }
            Change::Element { id, old, new } => {
                for e in [old, new].into_iter().flatten() {
                    match e.kind {
                        ElementKind::Wall(w) => touched.run(w.run),
                    }
                }
                m.elements.set(*id, pick(dir, old, new));
            }
            Change::LayerSet { id, old, new } => {
                m.layer_sets.set(*id, pick(dir, old, new));
                touched.library = true;
            }
            Change::Material { id, old, new } => {
                m.materials.set(*id, pick(dir, old, new));
                touched.library = true;
            }
            Change::Storey { id, old, new } => {
                m.storeys.set(*id, pick(dir, old, new));
                touched.library = true;
            }
            Change::Pen { id, old, new } => {
                m.attr.put_pen(*id, pick(dir, old, new));
                touched.attr = true;
            }
            Change::LineType { id, old, new } => {
                m.attr.put_line_type(*id, pick(dir, old, new));
                touched.attr = true;
            }
            Change::Fill { id, old, new } => {
                m.attr.put_fill(*id, pick(dir, old, new));
                touched.attr = true;
            }
            Change::Surface { id, old, new } => {
                m.attr.put_surface(*id, pick(dir, old, new));
                touched.attr = true;
            }
            Change::Display { old, new } => {
                m.attr.put_display(pick(dir, old, new));
                touched.attr = true;
            }
        };
        // Rückwärts in umgekehrter Reihenfolge: ein Platz wird erst frei, dann neu belegt
        match dir {
            Direction::Undo => t.changes.iter().rev().for_each(|c| apply_one(self, c)),
            Direction::Redo => t.changes.iter().for_each(|c| apply_one(self, c)),
        }
        if touched.attr {
            self.attr.bump();
        }
        if touched.library {
            self.joins = self.detect_all();
        } else {
            let runs = touched.runs.clone();
            for r in &runs {
                self.joined_runs(*r)
                    .into_iter()
                    .for_each(|p| touched.run(p));
            }
            self.update_joins(&runs);
            for r in &runs {
                self.joined_runs(*r)
                    .into_iter()
                    .for_each(|p| touched.run(p));
            }
        }
        self.touch();
        touched
    }

    // --- Prüfung ----------------------------------------------------------

    /// Prüft die Strukturregeln des BIM-Konzepts (Abschnitt 8) und liefert
    /// die Verstöße als Text.
    pub fn check(&self) -> Vec<String> {
        let mut out = Vec::new();
        let mut guids = Vec::new();
        let mut numbers: Vec<&str> = Vec::new();
        for (id, e) in self.elements.iter() {
            guids.push(e.guid);
            numbers.push(&e.number);
            if e.number.is_empty() {
                out.push(format!("{id:?}: keine Nummer"));
            }
            if !self.storeys.contains(e.storey) {
                out.push(format!("{}: Geschoss fehlt", e.number));
            }
            if let Some(s) = e.layer_set {
                if !self.layer_sets.contains(s) {
                    out.push(format!("{}: Aufbau fehlt", e.number));
                }
            }
            match e.kind {
                ElementKind::Wall(w) => {
                    if self.wall_at(w.run, w.seg as usize) != Some(id) {
                        out.push(format!("{}: nicht im Wandzug eingetragen", e.number));
                    }
                }
            }
        }
        for (id, r) in self.runs.iter() {
            guids.push(r.guid);
            let count = segment_count(r.points.len(), r.closed);
            if r.segments.len() != count {
                out.push(format!(
                    "Wandzug {id:?}: {} Wände für {count} Segmente",
                    r.segments.len()
                ));
            }
            if r.height <= 0.0 || !r.height.is_finite() {
                out.push(format!("Wandzug {id:?}: Höhe {} ungültig", r.height));
            }
            let n = r.points.len();
            for k in 0..count {
                let (p, q) = (r.points[k], r.points[(k + 1) % n]);
                if (q - p).length() < 1.0 {
                    out.push(format!("Wandzug {id:?}: Segment {} ohne Länge", k + 1));
                }
            }
            for e in &r.segments {
                if self.segment_of(*e).map(|s| s.0) != Some(id) {
                    out.push(format!(
                        "Wandzug {id:?}: Wand {e:?} fehlt oder gehört woandershin"
                    ));
                }
            }
        }
        for (_, set) in self.layer_sets.iter() {
            if set.layers.is_empty() {
                out.push(format!("Aufbau {}: keine Schichten", set.name));
            }
            if set
                .layers
                .iter()
                .any(|l| l.thickness <= 0.0 || !l.thickness.is_finite())
            {
                out.push(format!("Aufbau {}: Schichtdicke ungültig", set.name));
            }
        }
        for (_, m) in self.materials.iter() {
            let a = &self.attr;
            if a.fill(m.cut_fill).is_none()
                || a.pen(m.cut_fg).is_none()
                || a.pen(m.cut_bg).is_none()
                || a.surface(m.surface).is_none()
            {
                out.push(format!(
                    "Baustoff {}: Verweis auf fehlendes Attribut",
                    m.name
                ));
            }
        }
        out.extend(self.attr.check());
        guids.extend(self.attr.guids());
        guids.extend(self.materials.iter().map(|(_, m)| m.guid));
        guids.extend(self.layer_sets.iter().map(|(_, s)| s.guid));
        guids.extend(self.storeys.iter().map(|(_, s)| s.guid));
        guids.push(self.project.guid);
        let n = guids.len();
        guids.sort();
        guids.dedup();
        if guids.len() != n {
            out.push("Guid doppelt vergeben".into());
        }
        let n = numbers.len();
        numbers.sort();
        numbers.dedup();
        if numbers.len() != n {
            out.push("Bauteilnummer doppelt vergeben".into());
        }
        for (_, s) in self.layer_sets.iter() {
            for l in &s.layers {
                if !self.materials.contains(l.material) {
                    out.push(format!("Aufbau {}: Baustoff fehlt", s.name));
                }
            }
        }
        for j in &self.joins {
            if self.segment_of(j.a).map(|s| s.0) != Some(j.a_run)
                || self.segment_of(j.b).map(|s| s.0) != Some(j.b_run)
            {
                out.push(format!("Anschluss {:?}: Wand fehlt", j.a));
            }
        }
        // Regel 13: Sichtbares hängt nur an den Punkten, nicht am Verlauf
        let fresh = self.detect_all();
        if fresh.len() != self.joins.len()
            || !fresh.iter().all(|f| self.joins.iter().any(|j| j.same(f)))
        {
            out.push("Anschlüsse passen nicht zu den Punkten".into());
        }
        out
    }
}

/// Zug ohne Anschlüsse, zum Erkennen der Anschlüsse.
struct Base {
    run: RunId,
    chain: WallChain,
    segments: Vec<ElementId>,
    foot: Vec<[Vec3; 4]>,
    /// Umschließendes Rechteck des Grundrisses.
    lo: Vec3,
    hi: Vec3,
}

/// Liegt `p` im Rechteck `lo`..`hi` (Grundriss)?
fn inside(p: Vec3, lo: Vec3, hi: Vec3) -> bool {
    p.x >= lo.x && p.y >= lo.y && p.x <= hi.x && p.y <= hi.y
}

/// Feste Reihenfolge: nach anschließendem Zug und Ende.
fn sort_joins(j: &mut [Join]) {
    j.sort_by_key(|j| (j.a_run.index(), j.a_end));
}

/// Aufbau „IW 17,5 Gasbeton“: eine tragende Schicht aus `material`.
pub(crate) fn interior_set(guid: Guid, material: MaterialId) -> LayerSet {
    LayerSet {
        guid,
        name: "IW 17,5 Gasbeton".into(),
        layers: vec![MaterialLayer {
            material,
            thickness: 175.0,
            function: LayerFunction::Structure,
            core: true,
        }],
    }
}

/// Segmente eines Zuges als (Anfang, Ende).
/// Stand vorher (Rückgängig) bzw. nachher (Wiederholen).
fn pick<T: Clone>(dir: Direction, old: &T, new: &T) -> T {
    match dir {
        Direction::Undo => old.clone(),
        Direction::Redo => new.clone(),
    }
}

fn segment_lines(pts: &[Vec3], closed: bool) -> Vec<(Vec3, Vec3)> {
    let n = pts.len();
    (0..segment_count(n, closed))
        .map(|k| (pts[k], pts[(k + 1) % n]))
        .collect()
}

/// Ordnet jedem neuen Segment höchstens ein altes zu, mit dem es auf derselben
/// Linie in gleicher Richtung liegt; bei mehreren Kandidaten bekommt die
/// längste Überdeckung den Vorrang.
fn match_segments(old: &[(Vec3, Vec3)], new: &[(Vec3, Vec3)]) -> Vec<Option<usize>> {
    let mut pairs: Vec<(f64, usize, usize)> = Vec::new();
    for (i, &(a, b)) in new.iter().enumerate() {
        for (k, &(c, d)) in old.iter().enumerate() {
            let len = (d - c).length();
            if len < 1e-9 || (b - a).length() < 1e-9 {
                continue;
            }
            let dir = (d - c) * (1.0 / len);
            let dn = (b - a).normalized();
            let side = vec3(-dir.y, dir.x, 0.0);
            let parallel = (dn.x * dir.y - dn.y * dir.x).abs() < 1e-6 && dn.dot(dir) > 0.0;
            if !parallel || (a - c).dot(side).abs() > 1.0 {
                continue;
            }
            let (s0, s1) = ((a - c).dot(dir), (b - c).dot(dir));
            let overlap = s1.min(len) - s0.max(0.0);
            if overlap > 1.0 {
                pairs.push((overlap, i, k));
            }
        }
    }
    pairs.sort_by(|x, y| y.0.total_cmp(&x.0));
    let mut out = vec![None; new.len()];
    let mut used = vec![false; old.len()];
    for (_, i, k) in pairs {
        if out[i].is_none() && !used[k] {
            out[i] = Some(k);
            used[k] = true;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use sk_math::vec3;

    fn rechteck(m: &mut Model) -> RunId {
        let pts = [
            vec3(0.0, 0.0, 0.0),
            vec3(0.0, 6000.0, 0.0),
            vec3(8000.0, 6000.0, 0.0),
            vec3(8000.0, 0.0, 0.0),
        ];
        let set = m.defaults().exterior_wall;
        m.add_wall_run(
            &pts,
            true,
            RefSide::Left,
            3500.0,
            set,
            Category::ExteriorWall,
        )
        .unwrap()
    }

    #[test]
    fn je_segment_eine_wand_mit_nummer() {
        let mut m = Model::with_seed(1);
        let r = rechteck(&mut m);
        let run = m.run(r).unwrap();
        assert_eq!(run.segments.len(), 4);
        let nums: Vec<&str> = run
            .segments
            .iter()
            .map(|e| m.element(*e).unwrap().number.as_str())
            .collect();
        assert_eq!(nums, ["AW-001", "AW-002", "AW-003", "AW-004"]);
        for (k, e) in run.segments.iter().enumerate() {
            assert_eq!(m.segment_of(*e), Some((r, k)));
            let el = m.element(*e).unwrap();
            assert_eq!(el.category.din276(), Some(330));
            assert_eq!(el.layer_set, Some(m.defaults().exterior_wall));
        }
        assert!(m.check().is_empty(), "{:?}", m.check());
    }

    #[test]
    fn aufbau_wird_referenziert() {
        let mut m = Model::with_seed(2);
        let r = rechteck(&mut m);
        let c = m.chain(r).unwrap();
        assert!((c.thickness() - 315.0).abs() < 1e-9);
        assert!(c.layers[1].core && !c.layers[0].core);
        // Schichtdicke im Aufbau ändern: die Wand folgt ohne eigene Kopie
        let id = m.defaults().exterior_wall;
        let mut s = m.layer_set(id).unwrap().clone();
        s.layers[0].thickness = 160.0;
        assert!(m.set_layer_set(id, s));
        assert!((m.chain(r).unwrap().thickness() - 335.0).abs() < 1e-9);
        // Darstellungsschlüssel führen zurück zum Baustoff
        let key = c.layers[1].material;
        assert_eq!(m.material_by_key(key).unwrap().name, "Gasbeton");
        let fill = m.material_by_key(key | material::CUT).unwrap().cut_fill;
        assert_eq!(m.attr().fill(fill).unwrap().name, "Mauerwerk");
        assert!(m.material_by_key(material::PLAIN).is_none());
    }

    #[test]
    fn verschieben_behaelt_kennungen() {
        let mut m = Model::with_seed(3);
        let r = rechteck(&mut m);
        let before = m.run(r).unwrap().segments.clone();
        let rev = m.revision();
        let moved = m.chain(r).unwrap().with_segment_moved(1, 500.0).unwrap();
        assert!(m.set_run_points(r, &moved.points).is_some());
        assert_eq!(m.run(r).unwrap().segments, before);
        assert!(m.revision() > rev);
        // Offener Zug mit weniger Punkten: überzählige Wände verschwinden
        let mut m2 = Model::with_seed(4);
        let set = m2.defaults().exterior_wall;
        let pts = [
            vec3(0.0, 0.0, 0.0),
            vec3(0.0, 5000.0, 0.0),
            vec3(5000.0, 5000.0, 0.0),
        ];
        let r2 = m2
            .add_wall_run(
                &pts,
                false,
                RefSide::Left,
                3000.0,
                set,
                Category::ExteriorWall,
            )
            .unwrap();
        let segs = m2.run(r2).unwrap().segments.clone();
        assert!(m2.set_run_points(r2, &pts[..2]).is_some());
        assert_eq!(m2.run(r2).unwrap().segments, segs[..1]);
        assert!(m2.element(segs[1]).is_none());
        // Und wieder mehr: neue Wand mit neuer Nummer
        assert!(m2.set_run_points(r2, &pts).is_some());
        let e = m2.run(r2).unwrap().segments[1];
        assert_ne!(e, segs[1]);
        assert_eq!(m2.element(e).unwrap().number, "AW-003");
        assert!(m2.check().is_empty(), "{:?}", m2.check());
    }

    #[test]
    fn kennung_nach_lage() {
        let mut m = Model::with_seed(7);
        let r = rechteck(&mut m);
        let s = m.run(r).unwrap().segments.clone();
        let p = m.run(r).unwrap().points.clone();
        // Punkt in Segment 1 einfügen (oben, 0..8000 bei y = 6000), längerer Teil rechts
        let mut q = p.clone();
        q.insert(2, vec3(3000.0, 6000.0, 0.0));
        assert!(m.set_run_points(r, &q).is_some());
        let t = m.run(r).unwrap().segments.clone();
        assert_eq!(t.len(), 5);
        assert_eq!((t[0], t[3], t[4]), (s[0], s[2], s[3]));
        assert_ne!(t[1], s[1]);
        assert_eq!(t[2], s[1], "der längere Teil behält die Kennung");
        assert_eq!(m.element(t[1]).unwrap().number, "AW-005");
        // Den Punkt wieder entfernen: das kurze Stück verschwindet mit seiner Kennung
        assert!(m.set_run_points(r, &p).is_some());
        assert_eq!(m.run(r).unwrap().segments, s);
        assert!(m.element(t[1]).is_none());
        for (k, e) in s.iter().enumerate() {
            assert_eq!(m.segment_of(*e), Some((r, k)));
        }
        // Erstes Segment eines offenen Zuges entfernen: genau seine Kennung geht
        let set = m.defaults().exterior_wall;
        let o = [
            vec3(0.0, 0.0, 0.0),
            vec3(0.0, 5000.0, 0.0),
            vec3(5000.0, 5000.0, 0.0),
            vec3(5000.0, 0.0, 0.0),
        ];
        let r2 = m
            .add_wall_run(
                &o,
                false,
                RefSide::Left,
                3000.0,
                set,
                Category::ExteriorWall,
            )
            .unwrap();
        let s2 = m.run(r2).unwrap().segments.clone();
        assert!(m.set_run_points(r2, &o[1..]).is_some());
        assert_eq!(m.run(r2).unwrap().segments, s2[1..]);
        assert!(m.element(s2[0]).is_none());
        assert!(m.check().is_empty(), "{:?}", m.check());
    }

    /// Parametrik ohne Revision und Zähler, zum Vergleichen zweier Stände.
    fn state(m: &Model) -> String {
        format!(
            "{:?}{:?}{:?}{:?}{:?}{:?}",
            m.runs.iter().collect::<Vec<_>>(),
            m.elements.iter().collect::<Vec<_>>(),
            m.layer_sets.iter().collect::<Vec<_>>(),
            m.materials.iter().collect::<Vec<_>>(),
            m.attr.pens().iter().collect::<Vec<_>>(),
            m.attr.display(),
        )
    }

    #[test]
    fn rueckgaengig_vergibt_nichts_doppelt() {
        let mut m = Model::with_seed(5);
        m.require_steps();
        let empty = state(&m);
        m.begin("Wand");
        let r = rechteck(&mut m);
        let t = m.commit().unwrap();
        let guid = m.run(r).unwrap().guid;
        let rev = m.revision();
        let touched = m.apply(&t, Direction::Undo);
        assert_eq!(touched.runs, vec![r]);
        assert!(m.runs().is_empty() && m.elements().is_empty());
        assert_eq!(state(&m), empty);
        assert!(m.revision() > rev);
        m.begin("Wand");
        let r2 = rechteck(&mut m);
        m.commit();
        assert_ne!(r2, r);
        assert_ne!(m.run(r2).unwrap().guid, guid);
        let first = m.run(r2).unwrap().segments[0];
        assert_eq!(m.element(first).unwrap().number, "AW-005");
    }

    #[test]
    fn anlegen_rueckgaengig_wiederholen_behaelt_kennungen() {
        let mut m = Model::with_seed(7);
        m.require_steps();
        m.begin("Wand");
        let r = rechteck(&mut m);
        let t = m.commit().unwrap();
        let walls = m.run(r).unwrap().segments.clone();
        let numbers: Vec<String> = walls
            .iter()
            .map(|e| m.element(*e).unwrap().number.clone())
            .collect();
        let after = state(&m);
        m.apply(&t, Direction::Undo);
        assert!(m.run(r).is_none());
        m.apply(&t, Direction::Redo);
        assert_eq!(state(&m), after);
        assert_eq!(m.run(r).unwrap().segments, walls);
        for (e, n) in walls.iter().zip(&numbers) {
            assert_eq!(&m.element(*e).unwrap().number, n);
        }
        assert_eq!(numbers[0], "AW-001");
        assert!(m.check().is_empty(), "{:?}", m.check());
    }

    #[test]
    fn loeschen_rueckgaengig_bringt_alte_kennungen() {
        let mut m = Model::with_seed(8);
        m.require_steps();
        m.begin("Wand");
        let r = rechteck(&mut m);
        m.commit();
        let walls = m.run(r).unwrap().segments.clone();
        let guids: Vec<Guid> = walls.iter().map(|e| m.element(*e).unwrap().guid).collect();
        let before = state(&m);
        m.begin("Löschen");
        assert!(m.remove_run(r));
        let t = m.commit().unwrap();
        assert_eq!(t.changes.len(), 1 + walls.len());
        m.apply(&t, Direction::Undo);
        assert_eq!(state(&m), before);
        for (e, g) in walls.iter().zip(&guids) {
            assert_eq!(m.element(*e).unwrap().guid, *g);
        }
        assert!(m.check().is_empty());
        // Wiederholen löscht, ein neuer Zug bekommt keine früher vergebene Kennung
        m.apply(&t, Direction::Redo);
        m.begin("Wand");
        let r2 = rechteck(&mut m);
        m.commit();
        assert_ne!(r2, r);
        for e in &m.run(r2).unwrap().segments {
            assert!(!walls.contains(e));
        }
    }

    #[test]
    fn ziehen_ergibt_einen_eintrag() {
        let mut m = Model::with_seed(9);
        m.require_steps();
        m.begin("Wand");
        let r = rechteck(&mut m);
        m.commit();
        let start = m.run(r).unwrap().points.clone();
        let before = state(&m);
        m.begin("Verschieben");
        for i in 1..=100 {
            let mut p = start.clone();
            p[2].x += i as f64 * 10.0;
            p[3].x += i as f64 * 10.0;
            assert!(m.set_run_points(r, &p).is_some());
        }
        let t = m.commit().unwrap();
        assert_eq!(t.changes.len(), 1, "{:?}", t.changes);
        assert!(matches!(t.changes[0], Change::Run { id, .. } if id == r));
        m.apply(&t, Direction::Undo);
        assert_eq!(m.run(r).unwrap().points, start);
        assert_eq!(state(&m), before);
    }

    #[test]
    fn abbrechen_stellt_den_stand_beim_greifen_her() {
        let mut m = Model::with_seed(10);
        m.require_steps();
        m.begin("Wand");
        let r = rechteck(&mut m);
        m.commit();
        let before = state(&m);
        let start = m.run(r).unwrap().points.clone();
        m.begin("Verschieben");
        // Auch ein Teilen und Zusammenführen von Segmenten
        let mut p = start.clone();
        p.insert(1, vec3(0.0, 3000.0, 0.0));
        p[2].x += 500.0;
        assert!(m.set_run_points(r, &p).is_some());
        assert!(m.set_run_points(r, &start[..3]).is_some());
        let touched = m.rollback();
        assert_eq!(touched.runs, vec![r]);
        assert!(!m.in_step());
        assert_eq!(state(&m), before);
        assert!(m.check().is_empty());
    }

    #[test]
    fn umbenennen_eindeutig() {
        let mut m = Model::with_seed(6);
        let r = rechteck(&mut m);
        let s = m.run(r).unwrap().segments.clone();
        assert_eq!(m.set_number(s[0], "AW-Nord"), Ok(()));
        assert_eq!(m.element_by_number("AW-Nord"), Some(s[0]));
        assert_eq!(m.set_number(s[1], "AW-Nord"), Err(NumberError::Taken(s[0])));
        assert_eq!(m.set_number(s[1], "  "), Err(NumberError::Empty));
        assert!(m.remove_run(r));
        assert!(m.elements().is_empty());
        assert!(m.check().is_empty());
    }
}
