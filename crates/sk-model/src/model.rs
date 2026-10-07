//! Das Gebäudemodell als Datenbank: Bibliothek, Geschosse, Bauteile, Wandzüge.
//!
//! Bauteile verweisen nur über Kennungen aufeinander, nie über Vec-Plätze.
//! Geometrie wird aus der Parametrik abgeleitet ([`Model::chain`]) und nie
//! gespeichert. Jede Änderung erhöht die Revision.

use crate::attr::{
    self, AttrRef, AttrUser, Attributes, Display, Fill, FillId, LineType, LineTypeId, Pen, PenId,
    Surface, SurfaceId,
};
use crate::element::{
    Building, BuildingId, Category, Coupling, Element, ElementId, ElementKind, Floor, GroundSlab,
    LevelEdge, LevelKind, LevelRef, PropSet, PropValue, RunId, Soffit, Storey, StoreyId,
    StripFooting, Wall, WallRun,
};
use crate::floor::{FloorError, FloorParams, FloorSlab, SoffitParams, StripParams};
use crate::foundation::{FootingShape, Foundation, FoundationError, FoundationParams};
use crate::guid::{Guid, GuidGen};
use crate::id::Arena;
use crate::join::{self, Join, JoinEnd, JoinKind};
use crate::library::{
    material_key, Bearing, LayerFunction, LayerSet, LayerSetId, MatCategory, Material,
    MaterialDisplay, MaterialId, MaterialLayer, TypeCategory,
};
use crate::solid::material;
use crate::txn::{Change, Direction, Key, Open, Touched, Txn};
use crate::wall::{
    clean_points, cross2, segment_count, EndCut, Layer, Overhang, RefSide, WallChain,
};
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

/// Lage und Blickrichtung eines Schnitts (A quer, B längs). Ansichtszustand:
/// kein Rückgängig-Schritt, ändert die Revision nicht, steht aber in der
/// Datei.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Cut {
    /// Lage der Schnittebene quer zur Linie (A: y, B: x, mm); `None`, solange
    /// der Schnitt nie gezeigt wurde.
    pub pos: Option<f64>,
    /// Blick gespiegelt (A: nach −y statt +y, B: nach −x statt +x).
    pub flip: bool,
}

/// Namen der Schnitte in der Reihenfolge ihrer Kennung.
pub const CUT_NAMES: [&str; 2] = ["A", "B"];

#[derive(Clone, Debug)]
pub struct Model {
    project: Project,
    /// Schnitte A und B.
    cuts: [Cut; 2],
    /// Stifte, Schraffuren, Oberflächen und Bauteildarstellung.
    attr: Attributes,
    materials: Arena<Material>,
    layer_sets: Arena<LayerSet>,
    buildings: Arena<Building>,
    storeys: Arena<Storey>,
    elements: Arena<Element>,
    runs: Arena<WallRun>,
    /// Anschlüsse zwischen Wandzügen, aus den Punkten abgeleitet (B5a).
    joins: Vec<Join>,
    defaults: Defaults,
    /// Zuletzt vergebene laufende Nummer je Kategorie (wird nie zurückgesetzt).
    numbers: [u32; Category::ALL.len()],
    /// Zuletzt vergebene Gebäudenummer (GB-02 → 2); nie neu vergeben, nur
    /// ein verworfener Schritt nimmt sie zurück.
    building_number: u32,
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

#[path = "delete.rs"]
mod delete;
pub use delete::{refusal_lines, refusal_text, Deleted, Refusal};

impl Default for Model {
    fn default() -> Model {
        Model::new()
    }
}

impl Model {
    /// Leeres Modell mit Startbibliothek und Erdgeschoss. Die Bibliothek
    /// (Attribute, Baustoffe, Bauteiltypen) hat in jedem neuen Projekt
    /// dieselben Guids, damit der Firmenkatalog sie wiedererkennt (K2);
    /// Projekt und Geschosse bekommen neue.
    pub fn new() -> Model {
        let mut m = Model::standard(GuidGen::with_seed(LIBRARY_SEED));
        let mut g = GuidGen::from_time();
        m.project.guid = g.next_guid();
        for id in m.storeys.ids().collect::<Vec<_>>() {
            if let Some(s) = m.storeys.get_mut(id) {
                s.guid = g.next_guid();
            }
        }
        m.guids = g;
        // Standard-Außenwand neuer Projekte: AW-36 (F5); Tests mit festem
        // Startwert behalten AW-31,5
        if let Some(id) = m.type_by_guid(ETICS_TYPE_GUID) {
            m.defaults.exterior_wall = id;
        }
        m
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
        let concrete = mat(
            "Stahlbeton",
            C::Concrete,
            900,
            2500.0,
            st.masonry,
            [214, 214, 210],
            [150, 150, 148],
        );
        let plaster = mat(
            "Putz",
            C::Plaster,
            100,
            1400.0,
            st.empty,
            [240, 238, 232],
            [200, 198, 192],
        );
        let mut layer_sets = Arena::new();
        // Werkstypen mit festen Guids (K1); der Erzeuger läuft weiter, damit
        // die übrigen Guids gleich bleiben
        let _ = guids.next_guid();
        let exterior_wall = layer_sets.insert(LayerSet {
            guid: EXTERIOR_TYPE_GUID,
            name: "AW 31,5 Gasbeton + WDVS".into(),
            code: "AW-31,5".into(),
            category: TypeCategory::ExteriorWall,
            props: PropSet::new(),
            note: String::new(),
            changed: 1,
            bearing: Bearing::Core,
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
            building: None,
            name: "Erdgeschoss".into(),
            short: "EG".into(),
            kind: LevelKind::Storey,
            elevation: 0.0,
            height: STOREY_HEIGHT,
        });
        let project = Project {
            guid: guids.next_guid(),
            name: "Projekt".into(),
        };
        // Nach der Projekt-Guid angelegt, damit die älteren Guids gleich bleiben
        let _ = guids.next_guid();
        let interior_wall = layer_sets.insert(interior_set(INTERIOR_TYPE_GUID, aerated));
        // Stahlbeton: Diagonale, jede zweite Linie gestrichelt (E15)
        let cross = attr.add_fill(Fill {
            guid: guids.next_guid(),
            name: "Stahlbeton".into(),
            kind: attr::FillKind::Lines(attr::concrete_lines()),
            space: attr::FillSpace::Paper,
        });
        if let Some(m) = materials.get_mut(concrete) {
            m.cut_fill = cross;
        }
        // Gründung und Obergeschoss (B11), zuletzt angelegt, damit die
        // älteren Guids gleich bleiben
        storeys.insert(Storey {
            guid: guids.next_guid(),
            building: None,
            name: "Gründung".into(),
            short: "GR".into(),
            kind: LevelKind::Foundation,
            elevation: -FOUNDATION_DEPTH,
            height: FOUNDATION_DEPTH,
        });
        storeys.insert(Storey {
            guid: guids.next_guid(),
            building: None,
            name: "Obergeschoss".into(),
            short: "OG".into(),
            kind: LevelKind::Storey,
            elevation: STOREY_HEIGHT,
            height: UPPER_HEIGHT,
        });
        // Linientypen des Startsatzes (E4), zuletzt angelegt, damit die
        // älteren Guids gleich bleiben; die Schnittlinie A–A wird Strichpunkt
        for (name, pattern) in attr::standard_line_types().into_iter().skip(1) {
            let id = attr.add_line_type(LineType {
                guid: guids.next_guid(),
                name: name.into(),
                pattern,
            });
            if name == attr::SECTION_LINE_TYPE {
                let mut d = attr.display().clone();
                d.section_line.line_type = id;
                attr.set_display(d);
            }
        }
        // K4: λ-Vorbelegungen (BIM) an den älteren Baustoffen
        for (id, lambda) in [
            (aerated, 0.09),
            (insulation, 0.035),
            (concrete, 2.3),
            (plaster, 0.87),
        ] {
            if let Some(m) = materials.get_mut(id) {
                m.lambda = Some(lambda);
            }
        }
        // K4: Jörns Wandtypen mit ihren Baustoffen, zuletzt angelegt, damit
        // die älteren Guids gleich bleiben
        let mut mat =
            |name: &str, category, priority, density, lambda, cut_fill, color, cut_color| {
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
                    lambda,
                    cut_fill,
                    cut_fg: st.hatch_pen,
                    cut_bg: st.background,
                    surface,
                })
            };
        let facing = mat(
            "Verblender (Vormauerziegel)",
            C::Masonry,
            700,
            1800.0,
            Some(0.68),
            st.masonry,
            [168, 74, 52],
            [140, 62, 46],
        );
        let cavity = mat(
            "Kerndämmung (Mineralwolle)",
            C::Insulation,
            300,
            30.0,
            Some(0.035),
            st.insulation,
            [236, 226, 170],
            [222, 200, 110],
        );
        // Luft hat keinen Körper; Schraffur leer, die Oberfläche wird nie gezeigt
        let air = mat(
            "Luft",
            C::Air,
            0,
            1.2,
            None,
            st.empty,
            [255, 255, 255],
            [255, 255, 255],
        );
        let layer = |material, thickness, function, core| MaterialLayer {
            material,
            thickness,
            function,
            core,
        };
        use LayerFunction as F;
        let wall_type = |guid, name: &str, code: &str, category, layers| LayerSet {
            guid,
            name: name.into(),
            code: code.into(),
            category,
            props: PropSet::new(),
            note: String::new(),
            changed: 1,
            bearing: Bearing::Core,
            layers,
        };
        layer_sets.insert(wall_type(
            ETICS_TYPE_GUID,
            "AW mit WDVS 36",
            "AW-36",
            TypeCategory::ExteriorWall,
            vec![
                layer(insulation, 120.0, F::Insulation, false),
                layer(aerated, 240.0, F::Structure, true),
            ],
        ));
        layer_sets.insert(wall_type(
            CAVITY_TYPE_GUID,
            "AW mehrschalig 49",
            "AW-49",
            TypeCategory::ExteriorWall,
            vec![
                layer(facing, 115.0, F::Finish, false),
                layer(air, 60.0, F::AirGap, false),
                layer(cavity, 140.0, F::Insulation, false),
                layer(aerated, 175.0, F::Structure, true),
            ],
        ));
        for (guid, name, code, d) in [
            (INTERIOR_115_TYPE_GUID, "IW 11,5 Gasbeton", "IW-11,5", 115.0),
            (INTERIOR_240_TYPE_GUID, "IW 24 Gasbeton", "IW-24", 240.0),
        ] {
            layer_sets.insert(wall_type(
                guid,
                name,
                code,
                TypeCategory::InteriorWall,
                vec![layer(aerated, d, F::Structure, true)],
            ));
        }
        // K5: monolithische Wand, Decke 24 cm aufgelegt, davor Randdämmstreifen
        let edge = mat(
            "Randdämmung",
            C::Insulation,
            300,
            30.0,
            Some(0.035),
            st.insulation,
            [236, 230, 196],
            [226, 204, 120],
        );
        let mut mono = wall_type(
            MONO_TYPE_GUID,
            "AW monolithisch 36,5",
            "AW-36,5",
            TypeCategory::ExteriorWall,
            vec![layer(aerated, 365.0, F::Structure, true)],
        );
        mono.bearing = Bearing::Depth {
            depth: 240.0,
            strip: edge,
        };
        layer_sets.insert(mono);
        Model {
            project,
            attr,
            materials,
            layer_sets,
            buildings: Arena::new(),
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
            building_number: 0,
            revision: 0,
            guids,
            txn: None,
            strict: false,
            cuts: Default::default(),
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
        buildings: Arena<Building>,
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
        let building_number = buildings
            .iter()
            .filter_map(|(_, b)| building_index(&b.number))
            .max()
            .unwrap_or(0);
        let mut m = Model {
            project,
            attr,
            materials,
            layer_sets,
            buildings,
            storeys,
            elements,
            runs,
            joins: Vec::new(),
            defaults,
            numbers,
            building_number,
            revision: 0,
            guids,
            txn: None,
            strict: false,
            cuts: Default::default(),
        };
        m.joins = m.detect_all();
        m
    }

    pub fn project(&self) -> &Project {
        &self.project
    }

    /// Schnitte A und B (Ansichtszustand, siehe [`Cut`]).
    pub fn cuts(&self) -> &[Cut; 2] {
        &self.cuts
    }

    /// Lage oder Blickrichtung eines Schnitts ändern: ohne Schritt und ohne
    /// neue Revision, wie Kamera und Zoom.
    pub fn set_cut(&mut self, i: usize, cut: Cut) {
        if let Some(c) = self.cuts.get_mut(i) {
            *c = cut;
        }
    }

    /// Steigt bei jeder Änderung.
    pub fn revision(&self) -> u64 {
        self.revision
    }

    fn touch(&mut self) {
        self.revision += 1;
    }

    /// Setzt die Revision nach einem verworfenen Schritt auf den Stand davor
    /// zurück (Einstellungsfenster: „Abbrechen“ lässt das Projekt
    /// ungeändert). Nur für Schritte, die allein Attribute ändern.
    pub fn restore_revision(&mut self, rev: u64) {
        debug_assert!(self.txn.is_none());
        self.revision = rev;
    }

    /// Neue, noch nie vergebene Guid.
    pub fn new_guid(&mut self) -> Guid {
        self.guids.next_guid()
    }

    /// Eigene Guid-Folge für eine Arbeitskopie (K3: Bauteilkatalog): was
    /// sie vergibt und ins Modell zurückkommt, vergibt das Modell nie
    /// noch einmal.
    pub fn fork_guids(&mut self) {
        self.guids = GuidGen::from_time();
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

    /// Löscht einen unbenutzten Stift ([`Model::attr_users`] leer). `false`,
    /// wenn er verwendet wird oder fehlt; dann bleibt alles, wie es ist.
    pub fn remove_pen(&mut self, id: PenId) -> bool {
        if self.attr.pen(id).is_none() || !self.attr_users(AttrRef::Pen(id)).is_empty() {
            return false;
        }
        note!(self, Pen, self.attr.pens(), id);
        self.touch();
        self.attr.remove_pen(id).is_some()
    }

    /// Nummer für einen neuen Stift: die höchste vergebene + 1. Nummern
    /// rücken nie nach und werden nicht wieder vergeben (solange der Stift mit
    /// der höchsten Nummer lebt).
    pub fn next_pen_number(&self) -> u16 {
        let max = self.attr.pens().iter().map(|(_, p)| p.number).max();
        max.map_or(1, |n| n.saturating_add(1))
    }

    /// Wer das Attribut verwendet: Stellen der Darstellung (Stift und
    /// Linientyp) und Baustoffe (Schnittmuster, Schraffur, Grund, Oberfläche).
    pub fn attr_users(&self, r: AttrRef) -> Vec<AttrUser> {
        let mut out = Vec::new();
        for (label, st) in attr::display_slots(self.attr.display()) {
            let hit = match r {
                AttrRef::Pen(p) => st.pen == p,
                AttrRef::LineType(l) => st.line_type == l,
                _ => false,
            };
            if hit {
                out.push(AttrUser::Display(label));
            }
        }
        for (id, m) in self.materials.iter() {
            let roles: [(&'static str, bool); 4] = [
                ("Schnittmuster", r == AttrRef::Fill(m.cut_fill)),
                ("Schraffur", r == AttrRef::Pen(m.cut_fg)),
                ("Grund", r == AttrRef::Pen(m.cut_bg)),
                ("Oberfläche", r == AttrRef::Surface(m.surface)),
            ];
            for (role, hit) in roles {
                if hit {
                    out.push(AttrUser::Material {
                        id,
                        name: m.name.clone(),
                        role,
                    });
                }
            }
        }
        out
    }

    /// Ändert einen Linientyp; `false` (und nichts geändert) bei toter Kennung.
    pub fn set_line_type(&mut self, id: LineTypeId, l: LineType) -> bool {
        if self.attr.line_type(id).is_none() {
            return false;
        }
        note!(self, LineType, self.attr.line_types(), id);
        let ok = self.attr.set_line_type(id, l);
        self.revision += ok as u64;
        ok
    }

    /// Löscht einen unbenutzten Linientyp (wie [`Model::remove_pen`]).
    pub fn remove_line_type(&mut self, id: LineTypeId) -> bool {
        if self.attr.line_type(id).is_none() || !self.attr_users(AttrRef::LineType(id)).is_empty() {
            return false;
        }
        note!(self, LineType, self.attr.line_types(), id);
        self.touch();
        self.attr.remove_line_type(id).is_some()
    }

    /// Löscht eine Schraffur, auf die kein Baustoff verweist.
    pub fn remove_fill(&mut self, id: FillId) -> bool {
        if self.attr.fill(id).is_none() || !self.attr_users(AttrRef::Fill(id)).is_empty() {
            return false;
        }
        note!(self, Fill, self.attr.fills(), id);
        self.touch();
        self.attr.remove_fill(id).is_some()
    }

    /// Löscht eine Oberfläche, auf die kein Baustoff verweist.
    pub fn remove_surface(&mut self, id: SurfaceId) -> bool {
        if self.attr.surface(id).is_none() || !self.attr_users(AttrRef::Surface(id)).is_empty() {
            return false;
        }
        note!(self, Surface, self.attr.surfaces(), id);
        self.touch();
        self.attr.remove_surface(id).is_some()
    }

    /// Ändert die Darstellungsverweise eines Baustoffs (Schraffur, Stift
    /// Schraffur, Stift Grund, Oberfläche). Die übrigen Felder gehören BIM und
    /// bleiben unerreichbar. `false` (nichts geändert), wenn der Baustoff oder
    /// eines der Ziele fehlt.
    pub fn set_material_display(&mut self, id: MaterialId, d: MaterialDisplay) -> bool {
        let a = &self.attr;
        let alive = a.fill(d.cut_fill).is_some()
            && a.pen(d.cut_fg).is_some()
            && a.pen(d.cut_bg).is_some()
            && a.surface(d.surface).is_some();
        if !alive || self.materials.get(id).is_none() {
            return false;
        }
        note!(self, Material, self.materials, id);
        if let Some(m) = self.materials.get_mut(id) {
            (m.cut_fill, m.cut_fg, m.cut_bg, m.surface) =
                (d.cut_fill, d.cut_fg, d.cut_bg, d.surface);
        }
        self.touch();
        // Nur die Zeichentabelle ändert sich, keine Körper
        self.attr.bump();
        true
    }

    /// Ergänzt in Dateien vor E4 die Linientypen des Startsatzes und setzt die
    /// Schnittlinie A–A auf Strichpunkt, wenn sie auf die Volllinie zeigt (so
    /// sieht sie aus wie bisher). Ohne Rückgängig-Schritt, gleich nach dem
    /// Lesen; liefert einen Hinweis, wenn etwas ergänzt wurde.
    pub(crate) fn complete_line_types(&mut self) -> Vec<String> {
        let mut added = false;
        for (name, pattern) in attr::standard_line_types() {
            if self.attr.line_types().iter().any(|(_, l)| l.name == name) {
                continue;
            }
            let guid = self.new_guid();
            let id = self.attr.add_line_type(LineType {
                guid,
                name: name.into(),
                pattern,
            });
            added = true;
            let solid = self
                .attr
                .line_type(self.attr.display().section_line.line_type);
            if name == attr::SECTION_LINE_TYPE && solid.is_some_and(|l| l.pattern.is_empty()) {
                let mut d = self.attr.display().clone();
                d.section_line.line_type = id;
                self.attr.set_display(d);
            }
        }
        if added {
            vec!["Linientypen ergänzt".to_string()]
        } else {
            Vec::new()
        }
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

    /// Wird der Baustoff benutzt: in einer Schicht, als Randdämmstreifen
    /// eines Typs (Regel 15) oder von Sohlplatte, Schürze, Decke?
    pub fn material_used(&self, id: MaterialId) -> bool {
        let in_types = self.layer_sets.iter().any(|(_, t)| {
            t.layers.iter().any(|l| l.material == id) || t.strip_material() == Some(id)
        });
        let in_elements = self.elements.iter().any(|(_, e)| match &e.kind {
            ElementKind::Floor(f) => f.material == id || f.soffit.material == Some(id),
            ElementKind::GroundSlab(g) => g.material == id,
            ElementKind::StripFooting(f) => f.material == id,
            ElementKind::Wall(_)
            | ElementKind::EdgeStrip { .. }
            | ElementKind::SoffitInsulation { .. } => false,
        });
        in_types || in_elements
    }

    /// Löscht einen unbenutzten Baustoff ([`Model::material_used`]). `false`,
    /// wenn er benutzt wird oder fehlt; dann bleibt alles, wie es ist.
    pub fn remove_material(&mut self, id: MaterialId) -> bool {
        if !self.materials.contains(id) || self.material_used(id) {
            return false;
        }
        note!(self, Material, self.materials, id);
        self.touch();
        self.materials.remove(id).is_some()
    }

    pub fn layer_sets(&self) -> &Arena<LayerSet> {
        &self.layer_sets
    }

    pub fn layer_set(&self, id: LayerSetId) -> Option<&LayerSet> {
        self.layer_sets.get(id)
    }

    /// Legt einen Bauteiltyp an. `None`, wenn das Kurzzeichen leer oder
    /// schon vergeben ist, die Guid schon existiert oder der Typ gegen die
    /// Typregeln verstößt ([`LayerSet::problems`]).
    pub fn add_layer_set(&mut self, s: LayerSet) -> Option<LayerSetId> {
        self.insert_layer_set(s, true)
    }

    /// Wie [`Model::add_layer_set`], ein ungültiges Deckenauflager (Regel 21)
    /// bleibt aber stehen und wird wie „ganze tragende Schicht“ gebaut, wie
    /// beim Laden einer `.szo` (Übernahme aus dem Firmenkatalog).
    pub(crate) fn adopt_layer_set(&mut self, s: LayerSet) -> Option<LayerSetId> {
        self.insert_layer_set(s, false)
    }

    fn insert_layer_set(&mut self, s: LayerSet, strict: bool) -> Option<LayerSetId> {
        let taken = self
            .layer_sets
            .iter()
            .any(|(_, t)| t.guid == s.guid || t.code == s.code);
        if taken
            || !s.problems().is_empty()
            || (strict && self.bearing_problem(&s).is_some())
            || s.layers
                .iter()
                .map(|l| l.material)
                .chain(s.strip_material())
                .any(|m| !self.materials.contains(m))
        {
            return None;
        }
        self.touch();
        let id = self.layer_sets.insert(s);
        note!(self, LayerSet, new id);
        Some(id)
    }

    /// Ändert einen Bauteiltyp; alle Bauteile dieses Typs folgen. Die Guid
    /// bleibt (Regel 19), das Kurzzeichen bleibt eindeutig, die Typart
    /// ändert sich nur bei unbenutzten Typen, die nicht Standard sind.
    /// `changed` zählt eins hoch (auch beim Übernehmen aus dem
    /// Firmenkatalog: es ist der Stand im Projekt). Gleicher Inhalt ändert
    /// nichts und gilt als Erfolg; `false`: abgelehnt, nichts geändert.
    pub fn set_layer_set(&mut self, id: LayerSetId, s: LayerSet) -> bool {
        self.replace_layer_set(id, s, true)
    }

    /// Wie [`Model::set_layer_set`] mit ungültigem Deckenauflager wie
    /// [`Model::adopt_layer_set`].
    pub(crate) fn adopt_set_layer_set(&mut self, id: LayerSetId, s: LayerSet) -> bool {
        self.replace_layer_set(id, s, false)
    }

    fn replace_layer_set(&mut self, id: LayerSetId, mut s: LayerSet, strict: bool) -> bool {
        let Some(old) = self.layer_sets.get(id) else {
            return false;
        };
        let code_taken = self
            .layer_sets
            .iter()
            .any(|(other, t)| other != id && t.code == s.code);
        let category_locked = s.category != old.category
            && (self.is_default_type(id) || !self.type_users(id).is_empty());
        if s.guid != old.guid
            || code_taken
            || category_locked
            || !s.problems().is_empty()
            || (strict && self.bearing_problem(&s).is_some())
            || s.layers
                .iter()
                .map(|l| l.material)
                .chain(s.strip_material())
                .any(|m| !self.materials.contains(m))
        {
            return false;
        }
        if same_type(old, &s) {
            return true;
        }
        s.changed = old.changed + 1;
        note!(self, LayerSet, self.layer_sets, id);
        if let Some(t) = self.layer_sets.get_mut(id) {
            *t = s;
        }
        // Andere Dicken: Anschlüsse neu erkennen
        self.joins = self.detect_all();
        self.touch();
        true
    }

    /// Bauteile, die den Typ benutzen (Wände).
    pub fn type_users(&self, id: LayerSetId) -> Vec<ElementId> {
        self.elements
            .iter()
            .filter(|(_, e)| e.layer_set == Some(id))
            .map(|(i, _)| i)
            .collect()
    }

    /// Typ zu einem Kurzzeichen.
    pub fn type_by_code(&self, code: &str) -> Option<LayerSetId> {
        self.layer_sets
            .iter()
            .find(|(_, t)| t.code == code)
            .map(|(id, _)| id)
    }

    /// Typ zu einer Guid.
    pub fn type_by_guid(&self, g: Guid) -> Option<LayerSetId> {
        self.layer_sets
            .iter()
            .find(|(_, t)| t.guid == g)
            .map(|(id, _)| id)
    }

    /// `code`, wenn frei, sonst das nächste freie „code-2“, „code-3“ …
    pub fn free_code(&self, code: &str) -> String {
        free_code(code, |c| self.type_by_code(c).is_some())
    }

    /// Ist der Typ Standard für neue Wände seiner Art? Standardtypen lassen
    /// sich nicht löschen (Regel 18).
    pub fn is_default_type(&self, id: LayerSetId) -> bool {
        self.defaults.exterior_wall == id || self.defaults.interior_wall == id
    }

    /// Standardtyp für neue Wände der Art.
    pub fn default_type(&self, cat: TypeCategory) -> LayerSetId {
        match cat {
            TypeCategory::ExteriorWall => self.defaults.exterior_wall,
            TypeCategory::InteriorWall => self.defaults.interior_wall,
        }
    }

    /// Regel 21: „fest, Rest Randstreifen“ nur an Außenwänden, deren erste
    /// Schicht Kern ist, Tiefe im Bereich ([`LayerSet::bearing_range`]),
    /// Streifen aus Dämmung. Ein ungültiges Auflager bleibt gespeichert und
    /// wird wie „ganze tragende Schicht“ gebaut.
    pub fn bearing_problem(&self, t: &LayerSet) -> Option<String> {
        let Bearing::Depth { strip, .. } = t.bearing else {
            return None;
        };
        if let Some(p) = t.bearing_problem() {
            return Some(p);
        }
        let insulation = self
            .material(strip)
            .is_some_and(|m| m.category == MatCategory::Insulation);
        (!insulation).then(|| {
            let who = if t.code.is_empty() { &t.name } else { &t.code };
            format!("Typ {who}: Randdämmstreifen nur aus Dämmung")
        })
    }

    /// Kopie eines Typs: neue Guid, Name „… (Kopie)“, nächstes freies
    /// Kurzzeichen „…-2“, Stand 1.
    pub fn duplicate_type(&mut self, id: LayerSetId) -> Option<LayerSetId> {
        let t = self.layer_set(id)?.clone();
        let guid = self.new_guid();
        // Das eigene Kurzzeichen ist belegt: „…-2“, „…-3“ …
        let code = self.free_code(&t.code);
        self.add_layer_set(LayerSet {
            guid,
            name: format!("{} (Kopie)", t.name),
            code,
            changed: 1,
            ..t
        })
    }

    /// Löscht einen Typ, den kein Bauteil benutzt und der nicht Standard ist
    /// (Regel 18). `Err(n)`: n Bauteile benutzen ihn; `Err(0)`: Standardtyp
    /// oder kein Typ.
    pub fn remove_type(&mut self, id: LayerSetId) -> Result<(), usize> {
        let users = self.type_users(id).len();
        if users > 0 {
            return Err(users);
        }
        if self.is_default_type(id) || !self.layer_sets.contains(id) {
            return Err(0);
        }
        note!(self, LayerSet, self.layer_sets, id);
        self.layer_sets.remove(id);
        self.touch();
        Ok(())
    }

    /// Setzt den Standardtyp für neue Wände der Art; `false`, wenn der Typ
    /// fehlt oder eine andere Art hat.
    pub fn set_default_type(&mut self, cat: TypeCategory, id: LayerSetId) -> bool {
        if self.layer_set(id).is_none_or(|t| t.category != cat) {
            return false;
        }
        let mut d = self.defaults;
        match cat {
            TypeCategory::ExteriorWall => d.exterior_wall = id,
            TypeCategory::InteriorWall => d.interior_wall = id,
        }
        if d == self.defaults {
            return true;
        }
        match self.txn.as_mut() {
            Some(t) => {
                if t.noted.insert(Key::Defaults) {
                    t.changes.push(Change::Defaults {
                        old: self.defaults,
                        new: d,
                    });
                }
            }
            None => debug_assert!(!self.strict, "Änderung ohne Schritt"),
        }
        self.defaults = d;
        self.touch();
        true
    }

    /// Wechselt den Typ eines Wandzugs. Alle Segmente bekommen ihn, dazu
    /// die gekoppelten Züge darunter und darüber, damit die Schale fugenlos
    /// bleibt. Bei Außenwänden bleibt die Außenseite stehen und die Wand
    /// wächst nach innen: Liegt die Außenseite nicht auf der Bezugslinie,
    /// wandert die Linie um die Dickenänderung. Decke, Gründung und
    /// Anschlüsse ziehen nach. `false` (nichts geändert), wenn der Typ fehlt,
    /// nicht zur Wand passt oder die Linie sich nicht verschieben lässt.
    pub fn set_run_type(&mut self, run: RunId, id: LayerSetId) -> bool {
        let Some(t) = self.layer_set(id) else {
            return false;
        };
        let cat = t.category;
        let mut root = run;
        for _ in 0..64 {
            match self.run_below(root) {
                Some(b) if b != root => root = b,
                _ => break,
            }
        }
        let mut stack = vec![root];
        stack.extend(self.stack_above(root));
        let walls: Vec<ElementId> = stack
            .iter()
            .filter_map(|r| self.run(*r))
            .flat_map(|r| r.segments.iter().copied())
            .collect();
        if !stack.contains(&run)
            || walls.iter().any(|w| {
                self.element(*w)
                    .is_none_or(|e| TypeCategory::of(e.category) != Some(cat))
            })
        {
            return false;
        }
        if walls
            .iter()
            .all(|w| self.element(*w).is_some_and(|e| e.layer_set == Some(id)))
        {
            return true;
        }
        // Neue Bezugslinie vorab, damit bei einem Fehler nichts geändert ist
        let mut points = None;
        if cat == TypeCategory::ExteriorWall {
            let Some(old) = self.base_chain(root) else {
                return false;
            };
            let new = WallChain {
                layers: self.wall_layers(id),
                ..old.clone()
            };
            let d = (old.outer_offset() - new.outer_offset()) * old.outward_sign();
            if d.abs() > 1e-9 {
                let offsets = vec![d; old.segment_count()];
                match old.with_segment_offsets(&offsets) {
                    Some(c) => points = Some(c.points),
                    None => return false,
                }
            }
        }
        for w in walls {
            note!(self, Element, self.elements, w);
            if let Some(e) = self.elements.get_mut(w) {
                e.layer_set = Some(id);
            }
        }
        match points {
            Some(p) => {
                self.set_run_points(root, &p);
            }
            None => self.joins = self.detect_all(),
        }
        self.touch();
        true
    }

    /// Entfernt einen Typ ohne Rückgängig und ohne Prüfung (nur beim
    /// Anlegen eines Projekts aus dem Firmenkatalog, [`crate::catalog`]).
    pub(crate) fn forget_type(&mut self, id: LayerSetId) {
        debug_assert!(self.type_users(id).is_empty() && self.txn.is_none());
        self.layer_sets.remove(id);
    }

    /// Merkmale eines Bauteils: die seines Typs, überlagert von den eigenen
    /// mit gleichem Schlüssel (IFC-Regel). Schichten und Dicken lassen sich
    /// am Bauteil nicht überschreiben.
    pub fn props_of(&self, el: ElementId) -> PropSet {
        let Some(e) = self.element(el) else {
            return PropSet::new();
        };
        let mut p = e
            .layer_set
            .and_then(|s| self.layer_set(s))
            .map_or_else(PropSet::new, |t| t.props.clone());
        p.extend(e.props.iter().map(|(k, v)| (k.clone(), v.clone())));
        p
    }

    /// U-Wert eines Typs in W/(m²K), nur zur Anzeige ([`Model::u_value_of`]).
    pub fn u_value(&self, id: LayerSetId) -> Option<f64> {
        self.u_value_of(self.layer_set(id)?)
    }

    /// U = 1 / (0,13 + Σ d/λ + 0,18 je Luftschicht + 0,04), nur bei
    /// Außenwänden; `None`, wenn einer Schicht λ fehlt.
    pub fn u_value_of(&self, s: &LayerSet) -> Option<f64> {
        if !s.is_external() || s.layers.is_empty() {
            return None;
        }
        let mut r = 0.13 + 0.04;
        for l in &s.layers {
            if l.function == LayerFunction::AirGap {
                r += 0.18;
                continue;
            }
            let lambda = self.material(l.material)?.lambda?;
            if lambda <= 0.0 {
                return None;
            }
            r += l.thickness / 1000.0 / lambda;
        }
        Some(1.0 / r)
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
                    air: l.function == LayerFunction::AirGap,
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

    // --- Gebäude (B12) ----------------------------------------------------

    /// Tests: OK EG auf `h` (Wandhöhe im EG), liefert das EG.
    #[cfg(test)]
    pub(crate) fn eg_at(&mut self, h: f64) -> StoreyId {
        let eg = self.defaults.storey;
        if self.storey(eg).is_some_and(|s| s.top() != h) {
            assert!(self.set_storey_top(eg, h), "OK EG {h}");
        }
        eg
    }

    pub fn buildings(&self) -> &Arena<Building> {
        &self.buildings
    }

    pub fn building(&self, id: BuildingId) -> Option<&Building> {
        self.buildings.get(id)
    }

    /// Gebäude eines Geschosses (`None`: Vorlage ohne Gebäude).
    pub fn building_of(&self, storey: StoreyId) -> Option<BuildingId> {
        self.storey(storey)?.building
    }

    /// Gebäude eines Bauteils (über sein Geschoss).
    pub fn building_of_element(&self, e: ElementId) -> Option<BuildingId> {
        self.building_of(self.element(e)?.storey)
    }

    /// Legt ein Gebäude mit Gründung, EG und `storeys − 1` Obergeschossen an
    /// (Vorgaben: EG 2,855, OG 2,98 = lichte Höhe 2,76 + Decke 0,22). Das
    /// erste Gebäude übernimmt die Vorlage (die Geschosse, die das Paneel
    /// vorher zeigt), jedes weitere bekommt eigene Geschosse.
    pub fn add_building(&mut self, storeys: u8) -> BuildingId {
        let storeys = storeys.max(1) as usize;
        let template = self.levels_in(None);
        // Nummern werden nie neu vergeben, auch nicht nach dem Löschen
        let mut k = self.building_number;
        let number = loop {
            k += 1;
            let s = format!("GB-{k:02}");
            if self.buildings.iter().all(|(_, b)| b.number != s) {
                break s;
            }
        };
        self.building_number = k;
        let guid = self.new_guid();
        let b = self.buildings.insert(Building {
            guid,
            name: format!("Gebäude {k}"),
            number,
        });
        note!(self, Building, new b);
        let mut levels = if template.is_empty() {
            let gr = self.insert_storey(Storey {
                guid: Guid(0),
                building: Some(b),
                name: "Gründung".into(),
                short: "GR".into(),
                kind: LevelKind::Foundation,
                elevation: -FOUNDATION_DEPTH,
                height: FOUNDATION_DEPTH,
            });
            let eg = self.insert_storey(Storey {
                guid: Guid(0),
                building: Some(b),
                name: "Erdgeschoss".into(),
                short: "EG".into(),
                kind: LevelKind::Storey,
                elevation: 0.0,
                height: STOREY_HEIGHT,
            });
            vec![gr, eg]
        } else {
            for &id in &template {
                note!(self, Storey, self.storeys, id);
                if let Some(st) = self.storeys.get_mut(id) {
                    st.building = Some(b);
                }
            }
            template
        };
        // Obergeschosse: überzählige der Vorlage weg, fehlende obendrauf
        while levels.len() > storeys + 1 {
            if let Some(top) = levels.pop() {
                note!(self, Storey, self.storeys, top);
                self.storeys.remove(top);
            }
        }
        while levels.len() < storeys + 1 {
            let z = levels
                .last()
                .and_then(|id| self.storey(*id))
                .map_or(0.0, |s| s.top());
            let id = self.insert_storey(Storey {
                guid: Guid(0),
                building: Some(b),
                name: String::new(),
                short: String::new(),
                kind: LevelKind::Storey,
                elevation: z,
                height: UPPER_HEIGHT,
            });
            levels.push(id);
        }
        let uppers = levels.len().saturating_sub(2);
        for (i, &id) in levels.iter().skip(2).enumerate() {
            let (name, short) = match uppers {
                1 => ("Obergeschoss".to_string(), "OG".to_string()),
                _ => (format!("{}. Obergeschoss", i + 1), format!("{}. OG", i + 1)),
            };
            if self
                .storey(id)
                .is_some_and(|s| s.name != name || s.short != short)
            {
                note!(self, Storey, self.storeys, id);
                if let Some(st) = self.storeys.get_mut(id) {
                    st.name = name;
                    st.short = short;
                }
            }
        }
        self.touch();
        b
    }

    /// Legt ein Geschoss an (neue Guid).
    fn insert_storey(&mut self, mut st: Storey) -> StoreyId {
        st.guid = self.new_guid();
        let id = self.storeys.insert(st);
        note!(self, Storey, new id);
        id
    }

    /// Ein Geschoss ohne Gebäude (Vorlage) wird mit seiner Vorlage zum ersten
    /// Gebäude: Bauteile gehören immer zu einem Gebäude.
    fn ensure_building(&mut self, storey: StoreyId) {
        if self.storey(storey).is_some_and(|s| s.building.is_none()) {
            let up = self
                .levels_in(None)
                .iter()
                .filter(|id| {
                    self.storey(**id)
                        .is_some_and(|s| s.kind != LevelKind::Foundation)
                })
                .count();
            self.add_building(up.max(1) as u8);
        }
    }

    /// Erdgeschoss des Gebäudes `b` (unterstes Geschoss über der Gründung).
    pub fn ground_of(&self, b: Option<BuildingId>) -> Option<StoreyId> {
        self.levels_in(b).into_iter().find(|id| {
            self.storey(*id)
                .is_some_and(|s| s.kind != LevelKind::Foundation)
        })
    }

    /// Erdgeschoss des Gebäudes, zu dem das Geschoss `id` gehört.
    pub fn ground_storey(&self, id: StoreyId) -> Option<StoreyId> {
        self.ground_of(self.storey(id)?.building)
    }

    /// Geschoss für das Wandsegment, das bei `p` beginnt, wenn im Geschoss
    /// `level` gezeichnet wird: Liegt `p` im EG-Außenpolygon eines anderen
    /// Gebäudes, das Geschoss auf gleicher Lage dort (B12).
    pub fn storey_at(&self, p: Vec3, level: StoreyId) -> StoreyId {
        let own = self.building_of(level);
        let hit = self.runs.iter().find_map(|(id, r)| {
            let b = self.building_of(r.storey);
            (b != own
                && r.closed
                && self.ground_storey(r.storey) == Some(r.storey)
                && self.category_of(id) == Some(Category::ExteriorWall)
                && contains(&r.points, p))
            .then_some(b)
        });
        let Some(b) = hit else {
            return level;
        };
        let rank = |g: Option<BuildingId>| -> Vec<StoreyId> {
            self.levels_in(g)
                .into_iter()
                .filter(|id| {
                    self.storey(*id)
                        .is_some_and(|s| s.kind != LevelKind::Foundation)
                })
                .collect()
        };
        let i = rank(own).iter().position(|x| *x == level).unwrap_or(0);
        let there = rank(b);
        there.get(i).or(there.last()).copied().unwrap_or(level)
    }

    /// Bauabschnitt (Wände, Decke) eines Geschosses: EG 3/4, OG 5/6, …
    fn storey_seq(&self, storey: StoreyId) -> (u16, u16) {
        let i = self
            .group_levels(storey)
            .into_iter()
            .filter(|id| {
                self.storey(*id)
                    .is_some_and(|s| s.kind != LevelKind::Foundation)
            })
            .position(|id| id == storey)
            .unwrap_or(0) as u16;
        (WALL_SEQ + 2 * i, FLOOR_SEQ + 2 * i)
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
                coupling: None,
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

    /// Legt einen Wandzug im Geschoss `storey` an, mit einem Wand-Bauteil je
    /// Segment; die Wände reichen von UK bis OK des Geschosses. Ein
    /// geschlossener Außenwandzug im EG erzeugt das ganze Gebäude (B12): in
    /// jedem Geschoss darüber einen gekoppelten Zug mit Decke. Gehört das
    /// Geschoss noch zu keinem Gebäude, entsteht das erste. `None`, wenn die
    /// Punkte kein Segment ergeben.
    pub fn add_wall_run(
        &mut self,
        points: &[Vec3],
        closed: bool,
        ref_side: RefSide,
        storey: StoreyId,
        layer_set: LayerSetId,
        category: Category,
    ) -> Option<RunId> {
        let flat: Vec<Vec3> = points.iter().map(|p| vec3(p.x, p.y, 0.0)).collect();
        let pts = clean_points(&flat, closed);
        let closed = closed && pts.len() >= 3;
        let count = segment_count(pts.len(), closed);
        let fits = self
            .layer_set(layer_set)
            .is_some_and(|t| TypeCategory::of(category) == Some(t.category));
        if count == 0 || !fits || self.storey(storey).is_none() {
            return None;
        }
        self.ensure_building(storey);
        let guid = self.new_guid();
        let run = self.runs.insert(WallRun {
            guid,
            points: pts,
            closed,
            ref_side,
            base: LevelRef::bottom(storey),
            top: LevelRef::top(storey),
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
            seq: self.storey_seq(storey).0,
            kind: ElementKind::Wall(Wall {
                run,
                seg: 0,
                coupling: None,
            }),
            props: PropSet::new(),
        };
        let segments = (0..count)
            .map(|k| self.new_wall(run, k, &template))
            .collect();
        self.runs.get_mut(run)?.segments = segments;
        self.sync_parts(run);
        self.update_joins(&[run]);
        if self.needs_foundation(run) {
            let (mut below, mut at) = (run, storey);
            while let Some(up) = self.level_above(at) {
                let Some(r) = self.stack_run(below, up) else {
                    break;
                };
                (below, at) = (r, up);
            }
        }
        self.touch();
        Some(run)
    }

    /// Gebäude aus einem Polygon (B12): geschlossener Außenwandzug im EG des
    /// Gebäudes `b` mit dem voreingestellten Aufbau, darüber alle Geschosse.
    pub fn build_from_polygon(&mut self, b: BuildingId, points: &[Vec3]) -> Option<RunId> {
        let eg = self.ground_of(Some(b))?;
        let set = self.defaults.exterior_wall;
        self.add_wall_run(points, true, RefSide::Left, eg, set, Category::ExteriorWall)
    }

    /// Zug im Geschoss `storey` über dem Zug `below`: gleiche Punkte (Versatz
    /// 0), gleiche Bezugsseite, je Segment eine Wand gleicher Art, an die
    /// Wand darunter gekoppelt; mit Decke an OK des Geschosses.
    fn stack_run(&mut self, below: RunId, storey: StoreyId) -> Option<RunId> {
        let r = self.run(below)?.clone();
        let guid = self.new_guid();
        let run = self.runs.insert(WallRun {
            guid,
            points: r.points.clone(),
            closed: r.closed,
            ref_side: r.ref_side,
            base: LevelRef::bottom(storey),
            top: LevelRef::top(storey),
            storey,
            segments: Vec::new(),
        });
        note!(self, Run, new run);
        let seq = self.storey_seq(storey).0;
        let mut segments = Vec::with_capacity(r.segments.len());
        for (k, &w) in r.segments.iter().enumerate() {
            let Some(t) = self.element(w).cloned() else {
                continue;
            };
            let id = self.new_wall(
                run,
                k,
                &Element {
                    storey,
                    seq,
                    props: PropSet::new(),
                    ..t
                },
            );
            if let Some(ElementKind::Wall(x)) = self.elements.get_mut(id).map(|e| &mut e.kind) {
                x.coupling = Some(Coupling {
                    below: w,
                    offset: 0.0,
                    linked: true,
                });
            }
            segments.push(id);
        }
        self.runs.get_mut(run)?.segments = segments;
        self.sync_parts(run);
        self.update_joins(&[run]);
        Some(run)
    }

    /// Züge, deren Wände an Wände des Zuges `below` gekoppelt sind.
    pub fn runs_above(&self, below: RunId) -> Vec<RunId> {
        let Some(segs) = self.run(below).map(|r| &r.segments) else {
            return Vec::new();
        };
        let mut out = Vec::new();
        for (_, e) in self.elements.iter() {
            if let ElementKind::Wall(Wall {
                run,
                coupling: Some(c),
                ..
            }) = e.kind
            {
                if segs.contains(&c.below) && !out.contains(&run) {
                    out.push(run);
                }
            }
        }
        out
    }

    /// Alle Züge, die über `run` gestapelt sind (Geschoss für Geschoss).
    pub fn stack_above(&self, run: RunId) -> Vec<RunId> {
        let mut out: Vec<RunId> = Vec::new();
        let mut k = 0;
        let mut at = vec![run];
        while k < at.len() {
            for up in self.runs_above(at[k]) {
                if !at.contains(&up) {
                    at.push(up);
                    out.push(up);
                }
            }
            k += 1;
        }
        out
    }

    /// Ist der Zug an einen Zug darunter gekoppelt (gestapelte Außenwand)?
    pub fn is_coupled(&self, run: RunId) -> bool {
        self.run(run).is_some_and(|r| {
            r.segments.iter().any(|e| {
                matches!(
                    self.element(*e).map(|x| &x.kind),
                    Some(ElementKind::Wall(Wall {
                        coupling: Some(_),
                        ..
                    }))
                )
            })
        })
    }

    /// Zug darunter, an den `run` gekoppelt ist.
    pub fn run_below(&self, run: RunId) -> Option<RunId> {
        self.run(run)?
            .segments
            .iter()
            .find_map(|e| match self.element(*e)?.kind {
                ElementKind::Wall(Wall {
                    coupling: Some(c), ..
                }) => self.segment_of(c.below).map(|s| s.0),
                _ => None,
            })
    }

    /// Führt die gekoppelten Züge über `root` mit (Linie = Partnerlinie +
    /// Versatz, Ecken neu geschnitten), Geschoss für Geschoss nach oben, und
    /// ordnet die Kopplungen neu zu, wenn sich die Segmentzahl geändert hat.
    /// Liefert die bewegten Züge.
    fn carry_stack(&mut self, root: RunId) -> Vec<RunId> {
        let mut out = Vec::new();
        let mut queue = vec![root];
        let mut k = 0;
        while k < queue.len() {
            let below = queue[k];
            k += 1;
            for up in self.runs_above(below) {
                if queue.contains(&up) {
                    continue;
                }
                self.follow_below(up, below);
                queue.push(up);
                out.push(up);
            }
        }
        out
    }

    /// Versatz je Segment des Zuges `up` gegenüber dem Zug darunter mit der
    /// Geometrie `lower` und den Wänden `lsegs`, und welche Segmente gelöst
    /// sind (ohne Kopplung, OG Phase 2). Gekoppelte Segmente behalten ihren
    /// gespeicherten Versatz; gelöste behalten ihre Linie, ihr Versatz wird
    /// aus der eigenen Lage gegen `lower` gemessen (G7 K1). Hat sich die
    /// Segmentzahl geändert, siehe [`Model::recount_offsets`].
    fn stack_offsets(
        &self,
        up: RunId,
        lower: &WallChain,
        lsegs: &[ElementId],
    ) -> (Vec<f64>, Vec<bool>) {
        let usegs = self.run(up).map(|r| r.segments.clone()).unwrap_or_default();
        let coupling = |w: ElementId| match self.element(w).map(|e| &e.kind) {
            Some(ElementKind::Wall(x)) => x.coupling,
            _ => None,
        };
        if usegs.len() != lsegs.len() {
            return self.recount_offsets(&usegs, lower, lsegs);
        }
        let free: Vec<bool> = usegs
            .iter()
            .map(|w| coupling(*w).is_some_and(|c| !c.linked))
            .collect();
        let measured = if free.iter().any(|f| *f) {
            self.base_chain(up)
                .and_then(|c| c.segment_offsets_from(lower))
        } else {
            None
        };
        let offsets = lsegs
            .iter()
            .enumerate()
            .map(|(k, e)| {
                if free[k] {
                    return measured
                        .as_ref()
                        .and_then(|m| m.get(k).copied())
                        .unwrap_or(0.0);
                }
                usegs
                    .iter()
                    .find_map(|w| coupling(*w).filter(|c| c.below == *e).map(|c| c.offset))
                    .unwrap_or(0.0)
            })
            .collect();
        (offsets, free)
    }

    /// Versatz und Zustand je Segment von `lower`, nachdem sich seine
    /// Segmentzahl geändert hat (Regel 28): Ein Segment, dessen Wand schon
    /// einen Partner oben hat, behält dessen Versatz und Zustand; ein neues
    /// Teilstück erbt beides vom geraden Nachbarn, aus dem es geteilt wurde.
    /// Sonst bündig und gekoppelt.
    fn recount_offsets(
        &self,
        usegs: &[ElementId],
        lower: &WallChain,
        lsegs: &[ElementId],
    ) -> (Vec<f64>, Vec<bool>) {
        let partner = |e: ElementId| {
            usegs
                .iter()
                .find_map(|w| match self.element(*w).map(|x| &x.kind) {
                    Some(ElementKind::Wall(Wall {
                        coupling: Some(c), ..
                    })) if c.below == e => Some((c.offset, !c.linked)),
                    _ => None,
                })
        };
        let n = lsegs.len();
        let normal = |k: usize| lower.segment_normal(k);
        let straight = |a: usize, b: usize| match (normal(a), normal(b)) {
            (Some(x), Some(y)) => x.dot(y) > 1.0 - 1e-9,
            _ => false,
        };
        let closed = lower.closed;
        let (mut offsets, mut free) = (vec![0.0; n], vec![false; n]);
        for k in 0..n {
            let own = partner(lsegs[k]);
            let near = || {
                let prev = (k > 0 || closed).then(|| (k + n - 1) % n);
                let next = (k + 1 < n || closed).then(|| (k + 1) % n);
                [prev, next]
                    .into_iter()
                    .flatten()
                    .filter(|&j| j != k && straight(j, k))
                    .find_map(|j| partner(lsegs[j]))
            };
            if let Some((o, f)) = own.or_else(near) {
                offsets[k] = o;
                free[k] = f;
            }
        }
        (offsets, free)
    }

    /// Lassen sich alle über `id` gestapelten Züge auf die neuen Punkte
    /// `points` von `id` legen (G7 K2)? Nein, wenn ein Segment verschwinden
    /// oder kippen würde oder sich ein Umriss selbst schneidet; dann bleibt
    /// das Gummiband stehen statt eine Decke zu verlieren.
    fn stack_fits(&self, id: RunId, points: &[Vec3]) -> bool {
        let Some(mut lower) = self.base_chain(id) else {
            return true;
        };
        lower.points = points.to_vec();
        let Some(lsegs) = self.run(id).map(|r| r.segments.clone()) else {
            return true;
        };
        // Andere Segmentzahl: die Kopplungen werden neu zugeordnet (alles bündig)
        if lower.segment_count() != lsegs.len() {
            return true;
        }
        for up in self.runs_above(id) {
            let (offsets, _) = self.stack_offsets(up, &lower, &lsegs);
            match lower.with_segment_offsets(&offsets) {
                Some(c) if room_inside(&c) && self.stack_fits(up, &c.points) => {}
                _ => return false,
            }
        }
        true
    }

    /// Stapelbezug einer gestapelten Wand: (Versatz in mm, + außen;
    /// gekoppelt). `None`: keine gestapelte Wand (EG, Innenwand).
    pub fn stack_offset(&self, wall: ElementId) -> Option<(f64, bool)> {
        match self.element(wall)?.kind {
            ElementKind::Wall(Wall {
                coupling: Some(c), ..
            }) => Some((c.offset, c.linked)),
            _ => None,
        }
    }

    /// Wand im Geschoss darunter, auf der `wall` steht.
    pub fn wall_below(&self, wall: ElementId) -> Option<ElementId> {
        match self.element(wall)?.kind {
            ElementKind::Wall(Wall {
                coupling: Some(c), ..
            }) => Some(c.below),
            _ => None,
        }
    }

    /// Fuß der Kette: von `wall` über gekoppelte Glieder nach unten bis zum
    /// ersten gelösten Glied oder zum EG. Ziehen ohne Strg an einer
    /// gekoppelten OG-Wand zieht diese Wand (OG Phase 2).
    pub fn chain_foot(&self, wall: ElementId) -> ElementId {
        let mut at = wall;
        while let Some((_, true)) = self.stack_offset(at) {
            match self.wall_below(at) {
                Some(b) if b != wall => at = b,
                _ => break,
            }
        }
        at
    }

    /// Kette am Segment öffnen bzw. schließen. Die Lage bleibt, der Versatz
    /// auch (OG-16). `false` bei einer Wand ohne Partner darunter.
    pub fn set_linked(&mut self, wall: ElementId, linked: bool) -> bool {
        let Some((_, now)) = self.stack_offset(wall) else {
            return false;
        };
        if now == linked {
            return true;
        }
        note!(self, Element, self.elements, wall);
        if let Some(ElementKind::Wall(Wall {
            coupling: Some(c), ..
        })) = self.elements.get_mut(wall).map(|e| &mut e.kind)
        {
            c.linked = linked;
        }
        if let Some((run, _)) = self.segment_of(wall) {
            self.sync_parts(run);
            self.update_joins(&[run]);
        }
        true
    }

    /// Versatz der gestapelten Wand `wall` auf `offset` (mm, + außen) setzen;
    /// unter 20 mm rastet er auf 0 ein (Regel 31). Nur diese Wand und was
    /// über ihr steht bewegt sich, der Zustand der Kette bleibt. `None`, wenn
    /// der Stapel dabei ungültig würde (Segment verschwindet, Umriss kreuzt
    /// sich, kein Raum, ein Erker über nur einen Teil der Wand).
    pub fn set_offset(&mut self, wall: ElementId, offset: f64) -> Option<Vec<RunId>> {
        let offset = if offset.abs() < MIN_OFFSET {
            0.0
        } else {
            offset
        };
        let (run, seg) = self.segment_of(wall)?;
        let (old, linked) = self.stack_offset(wall)?;
        let below = self.run_below(run)?;
        let lower = self.base_chain(below)?;
        let lsegs = self.run(below)?.segments.clone();
        let usegs = self.run(run)?.segments.clone();
        if usegs.len() != lsegs.len() || !offset.is_finite() {
            return None;
        }
        let mut offsets: Vec<f64> = usegs
            .iter()
            .map(|w| self.stack_offset(*w).map_or(0.0, |o| o.0))
            .collect();
        offsets[seg] = offset;
        let c = lower.with_segment_offsets(&offsets)?;
        if !room_inside(&c) {
            return None;
        }
        if (old - offset).abs() < 1e-9 {
            return Some(Vec::new());
        }
        let out = self.set_run_points(run, &c.points)?;
        note!(self, Element, self.elements, wall);
        if let Some(ElementKind::Wall(Wall {
            coupling: Some(cp), ..
        })) = self.elements.get_mut(wall).map(|e| &mut e.kind)
        {
            *cp = Coupling {
                below: cp.below,
                offset,
                linked,
            };
        }
        self.sync_parts(run);
        self.update_joins(&[run]);
        Some(out)
    }

    /// Versatz um `d` ändern ([`Model::set_offset`]).
    pub fn move_segment(&mut self, wall: ElementId, d: f64) -> Option<Vec<RunId>> {
        let (old, _) = self.stack_offset(wall)?;
        self.set_offset(wall, old + d)
    }

    /// „Bündig setzen“: Versatz 0 und gekoppelt.
    pub fn set_flush(&mut self, wall: ElementId) -> bool {
        self.set_offset(wall, 0.0).is_some() && self.set_linked(wall, true)
    }

    /// Legt den Zug `up` auf den Zug `below` (mit den Versätzen seiner Wände)
    /// und koppelt Wand k an Wand k darunter; gelöste Wände bleiben gelöst
    /// und stehen still.
    fn follow_below(&mut self, up: RunId, below: RunId) {
        let (Some(lower), Some(lsegs)) = (
            self.base_chain(below),
            self.run(below).map(|r| r.segments.clone()),
        ) else {
            return;
        };
        let (mut offsets, free) = self.stack_offsets(up, &lower, &lsegs);
        // Gelöste Wand, die durch das Ziehen darunter fast bündig steht: rastet
        // wie beim Ziehen am Fuß unter 20 mm auf 0 ein (Regel 31, Review 1t T1)
        for (d, f) in offsets.iter_mut().zip(&free) {
            if *f && d.abs() < MIN_OFFSET {
                *d = 0.0;
            }
        }
        let recount = self
            .run(up)
            .is_some_and(|r| r.segments.len() != lsegs.len());
        let pts = match lower.with_segment_offsets(&offsets) {
            Some(c) => c.points,
            // Neue Segmentzahl: bündig neu auflegen, die Kopplungen folgen
            None if recount => lower.points.clone(),
            // Ungültig (siehe stack_fits): Zug bleibt liegen, statt bündig
            // zu springen
            None => return,
        };
        let same = self
            .run(up)
            .is_some_and(|r| r.points == pts && r.segments.len() == lsegs.len());
        if !same && !self.set_points(up, &pts) {
            return;
        }
        let segs = self.run(up).map(|r| r.segments.clone()).unwrap_or_default();
        for (k, w) in segs.into_iter().enumerate() {
            let Some(&b) = lsegs.get(k) else {
                continue;
            };
            // Gelöst: Versatz nachgeführt, die Kette bleibt offen
            let want = Coupling {
                below: b,
                offset: offsets[k],
                linked: free.get(k) != Some(&true),
            };
            let now = match self.element(w).map(|e| &e.kind) {
                Some(ElementKind::Wall(x)) => x.coupling,
                _ => continue,
            };
            if now != Some(want) {
                note!(self, Element, self.elements, w);
                if let Some(ElementKind::Wall(x)) = self.elements.get_mut(w).map(|e| &mut e.kind) {
                    x.coupling = Some(want);
                }
            }
        }
    }

    /// Gelöste Segmente eines gestapelten Zuges, der selbst bewegt wurde:
    /// ihr Versatz ist wieder die wahre Lage gegen den Zug darunter.
    fn remeasure(&mut self, id: RunId) {
        let Some(below) = self.run_below(id) else {
            return;
        };
        let (Some(lower), Some(lsegs)) = (
            self.base_chain(below),
            self.run(below).map(|r| r.segments.clone()),
        ) else {
            return;
        };
        let (offsets, free) = self.stack_offsets(id, &lower, &lsegs);
        let segs = self.run(id).map(|r| r.segments.clone()).unwrap_or_default();
        if segs.len() != lsegs.len() {
            return;
        }
        for (k, w) in segs.into_iter().enumerate() {
            if !free[k] {
                continue;
            }
            let now = match self.element(w).map(|e| &e.kind) {
                Some(ElementKind::Wall(x)) => x.coupling,
                _ => continue,
            };
            if let Some(c) = now.filter(|c| c.offset != offsets[k]) {
                note!(self, Element, self.elements, w);
                if let Some(ElementKind::Wall(x)) = self.elements.get_mut(w).map(|e| &mut e.kind) {
                    x.coupling = Some(Coupling {
                        offset: offsets[k],
                        ..c
                    });
                }
            }
        }
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
        let flat: Vec<Vec3> = points.iter().map(|p| vec3(p.x, p.y, 0.0)).collect();
        // Punkte der geschlossenen Züge vorher: Deckenumriss alt gegen neu (U3)
        let before: Vec<(RunId, Vec<Vec3>)> = self
            .runs
            .iter()
            .filter(|(_, r)| r.closed)
            .map(|(id, r)| (id, r.points.clone()))
            .collect();
        if !self.stack_fits(id, &flat) || !self.set_points(id, &flat) {
            return None;
        }
        self.remeasure(id);
        let mut moved = Vec::new();
        let mut roots = vec![id];
        roots.extend(self.carry_stack(id));
        for r in roots {
            for x in self.follow(r) {
                if !moved.contains(&x) {
                    moved.push(x);
                }
            }
        }
        for r in &moved {
            self.sync_parts(*r);
        }
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
        // Der Versatz zwischen den Geschossen formt Decke und Außenschichten
        // darunter (G7 K4) und den Fuß darüber (K3)
        for r in &moved {
            for p in self.run_below(*r).into_iter().chain(self.runs_above(*r)) {
                if !out.contains(&p) {
                    out.push(p);
                }
            }
        }
        // Innenwände, die unter eine mitbewegte Decke kommen oder sie verlassen
        // (Deckenband)
        for r in &moved {
            let old = before.iter().find(|(id, _)| id == r).map(|(_, p)| &p[..]);
            for i in self.runs_under_moved_floor(*r, old) {
                if !out.contains(&i) {
                    out.push(i);
                }
            }
        }
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

    /// Entfernt einen Wandzug mit allen seinen Wänden und den Zügen, die
    /// darüber an ihn gekoppelt sind.
    pub fn remove_run(&mut self, id: RunId) -> bool {
        if !self.runs.contains(id) {
            return false;
        }
        for up in self.runs_above(id) {
            self.remove_run(up);
        }
        note!(self, Run, self.runs, id);
        let Some(run) = self.runs.remove(id) else {
            return false;
        };
        for e in run.segments {
            note!(self, Element, self.elements, e);
            self.elements.remove(e);
        }
        self.sync_parts(id);
        self.update_joins(&[id]);
        self.touch();
        true
    }

    /// Wandzug und Segment einer Wand.
    pub fn segment_of(&self, wall: ElementId) -> Option<(RunId, usize)> {
        match self.element(wall)?.kind {
            ElementKind::Wall(w) => Some((w.run, w.seg as usize)),
            _ => None,
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
        self.chain_and_floor(id).map(|(c, _)| c)
    }

    /// [`Model::chain`] und die Decke über dem Zug ([`Model::floor`]) in
    /// einem Gang: der Deckenumriss entsteht nur einmal.
    #[allow(clippy::type_complexity)]
    pub fn chain_and_floor(
        &self,
        id: RunId,
    ) -> Option<(WallChain, Option<Result<FloorSlab, FloorError>>)> {
        let mut c = self.base_chain(id)?;
        let floor = self.floor_of_chain(id, &c);
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
        c.joints.seamless = matches!(&floor, Some(Ok(f)) if f.strip.is_some());
        c.joints.strip_below = self
            .run_below(id)
            .is_some_and(|b| self.strip_params(b).is_some());
        if c.joints.strip_below {
            c.joints.strip_open = self.open_segments(id);
        }
        if let Some(Ok(f)) = &floor {
            c.joints.overhang = self.overhang_offsets(id).map(|offsets| {
                let (b, t) = f.band();
                Overhang {
                    offsets,
                    from: f.soffit_band().map_or(b, |s| s.0),
                    to: t,
                }
            });
        }
        c.joints.slab_band = match &floor {
            Some(Ok(f)) => Some(f.band()),
            _ if self.category_of(id) == Some(Category::InteriorWall) => self
                .run(id)
                .and_then(|r| self.floor_over(r.storey, &c))
                .map(|f| f.band()),
            _ => None,
        };
        Some((c, floor))
    }

    /// Kategorie der Wände eines Zuges (die des ersten Segments).
    fn category_of(&self, run: RunId) -> Option<Category> {
        let first = *self.run(run)?.segments.first()?;
        Some(self.element(first)?.category)
    }

    /// Decke im Geschoss `storey`, unter der der Zug `c` steht: eine
    /// Segmentmitte liegt in ihrem Umriss.
    fn floor_over(&self, storey: StoreyId, c: &WallChain) -> Option<FloorSlab> {
        let mids = mids(&c.points, c.closed);
        let (lo, hi) = bounds2(&mids);
        for (id, r) in self.runs.iter() {
            // Vortest: geschlossen und Rechteck der Zugpunkte überdeckt die
            // Segmentmitten, bevor der Umriss entsteht
            if !r.closed || r.storey != storey || !overlap(bounds2(&r.points), (lo, hi)) {
                continue;
            }
            let (rlo, rhi) = bounds2(&r.points);
            if !mids.iter().any(|m| inside(*m, rlo, rhi)) {
                continue;
            }
            if let Some(Ok(slab)) = self.floor(id) {
                if mids.iter().any(|m| contains(&slab.outline, *m)) {
                    return Some(slab);
                }
            }
        }
        None
    }

    /// Innenwandzüge, die die Decke über dem Zug `run` unterbricht.
    pub fn runs_under_floor(&self, run: RunId) -> Vec<RunId> {
        let Some(r) = self.run(run).filter(|r| r.closed) else {
            return Vec::new();
        };
        let (lo, hi) = bounds2(&r.points);
        // Vortest am Rechteck, bevor der Umriss entsteht
        let near: Vec<(RunId, Vec<Vec3>)> = self
            .runs
            .iter()
            .filter(|(id, x)| {
                *id != run && x.storey == r.storey && overlap(bounds2(&x.points), (lo, hi))
            })
            .filter(|(id, _)| self.category_of(*id) == Some(Category::InteriorWall))
            .map(|(id, x)| (id, mids(&x.points, x.closed)))
            .filter(|(_, m)| m.iter().any(|p| inside(*p, lo, hi)))
            .collect();
        if near.is_empty() {
            return Vec::new();
        }
        let Some(Ok(slab)) = self.floor(run) else {
            return Vec::new();
        };
        near.into_iter()
            .filter(|(_, m)| m.iter().any(|p| contains(&slab.outline, *p)))
            .map(|(r, _)| r)
            .collect()
    }

    /// Innenwandzüge, deren Deckenband sich ändert, weil die Decke über `run`
    /// von den Punkten `old` auf die jetzigen gewandert ist: Eine Segmentmitte
    /// liegt jetzt im Umriss und vorher in keinem oder umgekehrt. Das Band
    /// selbst (UK, OK) hängt nicht vom Grundriss ab; Innenwände, die drunter
    /// bleiben, ändern sich nicht (U3). Ohne alten Stand oder ohne gültige
    /// Decke vorher und nachher wie [`Model::runs_under_floor`].
    fn runs_under_moved_floor(&self, run: RunId, old: Option<&[Vec3]>) -> Vec<RunId> {
        let Some(r) = self.run(run).filter(|r| r.closed) else {
            return Vec::new();
        };
        let Some(old) = old else {
            return self.runs_under_floor(run);
        };
        if old == &r.points[..] {
            return Vec::new();
        }
        let before = self.base_chain(run).and_then(|mut c| {
            c.points = old.to_vec();
            self.floor_of_chain(run, &c)
        });
        let (Some(Ok(before)), Some(Ok(after))) = (before, self.floor(run)) else {
            return self.runs_under_floor(run);
        };
        let (alo, ahi) = bounds2(&before.outline);
        let (blo, bhi) = bounds2(&after.outline);
        let (lo, hi) = (
            vec3(alo.x.min(blo.x), alo.y.min(blo.y), 0.0),
            vec3(ahi.x.max(bhi.x), ahi.y.max(bhi.y), 0.0),
        );
        self.runs
            .iter()
            .filter(|(id, x)| {
                *id != run && x.storey == r.storey && overlap(bounds2(&x.points), (lo, hi))
            })
            .filter(|(id, _)| self.category_of(*id) == Some(Category::InteriorWall))
            .filter(|(_, x)| {
                let m = mids(&x.points, x.closed);
                let under = |o: &[Vec3]| m.iter().any(|p| contains(o, *p));
                under(&before.outline) != under(&after.outline)
            })
            .map(|(id, _)| id)
            .collect()
    }

    /// Geometrie eines Wandzugs ohne Anschlüsse.
    fn base_chain(&self, id: RunId) -> Option<WallChain> {
        let run = self.run(id)?;
        let set = run
            .segments
            .first()
            .and_then(|e| self.element(*e))
            .and_then(|e| e.layer_set)?;
        let base = self.level_z(run.base).unwrap_or(0.0);
        Some(WallChain {
            points: run.points.clone(),
            closed: run.closed,
            ref_side: run.ref_side,
            layers: self.wall_layers(set),
            base,
            height: self.level_z(run.top).unwrap_or(base) - base,
            joints: Default::default(),
        })
    }

    /// Alle Wandzüge als Geometrie.
    pub fn chains(&self) -> impl Iterator<Item = (RunId, WallChain)> + '_ {
        self.runs
            .ids()
            .filter_map(|id| self.chain(id).map(|c| (id, c)))
    }

    // --- Gründung (B9) ----------------------------------------------------

    /// Braucht der Zug eine Decke? Ein lebender, geschlossener Außenwandzug.
    fn needs_floor(&self, run: RunId) -> bool {
        self.run(run).is_some_and(|r| {
            r.closed
                && r.segments
                    .first()
                    .and_then(|e| self.element(*e))
                    .is_some_and(|e| e.category == Category::ExteriorWall)
        })
    }

    /// Braucht der Zug eine Gründung? Ein geschlossener Außenwandzug im EG
    /// seines Gebäudes (B12: Gründung nur unter dem EG).
    fn needs_foundation(&self, run: RunId) -> bool {
        self.needs_floor(run)
            && self
                .run(run)
                .is_some_and(|r| self.ground_storey(r.storey) == Some(r.storey))
    }

    /// Sohlplatten unter einem Wandzug (richtig: höchstens eine).
    fn slabs_of(&self, run: RunId) -> Vec<ElementId> {
        self.elements
            .iter()
            .filter(|(_, e)| matches!(e.kind, ElementKind::GroundSlab(s) if s.run == run))
            .map(|(id, _)| id)
            .collect()
    }

    /// Frostschürzen unter einer Sohlplatte (richtig: genau eine).
    fn footings_of(&self, slab: ElementId) -> Vec<ElementId> {
        self.elements
            .iter()
            .filter(|(_, e)| matches!(e.kind, ElementKind::StripFooting(f) if f.slab == slab))
            .map(|(id, _)| id)
            .collect()
    }

    /// Sohlplatte und Frostschürze unter einem Wandzug.
    pub fn foundation_of(&self, run: RunId) -> Option<(ElementId, Option<ElementId>)> {
        let slab = *self.slabs_of(run).first()?;
        Some((slab, self.footings_of(slab).first().copied()))
    }

    /// Wandzug, zu dem ein Bauteil gehört: Wand, Sohlplatte darunter oder
    /// Frostschürze unter deren Platte.
    pub fn run_of(&self, e: ElementId) -> Option<RunId> {
        match self.element(e)?.kind {
            ElementKind::Wall(w) => Some(w.run),
            ElementKind::GroundSlab(s) => Some(s.run),
            ElementKind::StripFooting(f) => self.run_of(f.slab),
            ElementKind::Floor(f) => Some(f.run),
            ElementKind::EdgeStrip { wall, .. } => self.run_of(wall),
            ElementKind::SoffitInsulation { floor } => self.run_of(floor),
        }
    }

    /// Paare (Wand, Decke), auf denen ein Randdämmstreifen liegen muss (K5):
    /// jede Wand eines Zuges unter seiner Decke, deren Typ das Auflager
    /// `Depth` hat. In Nummernreihenfolge: Gebäude, Geschoss, Segment.
    pub fn edge_strip_pairs(&self) -> Vec<(ElementId, ElementId)> {
        let mut floors: Vec<(String, f64, ElementId, RunId)> = self
            .elements
            .iter()
            .filter_map(|(id, e)| match e.kind {
                ElementKind::Floor(f) => {
                    let r = self.run(f.run)?;
                    let st = self.storey(r.storey)?;
                    let b = st
                        .building
                        .and_then(|b| self.building(b))
                        .map_or(String::new(), |b| b.number.clone());
                    Some((b, st.elevation, id, f.run))
                }
                _ => None,
            })
            .collect();
        floors.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.total_cmp(&b.1)));
        let mut out = Vec::new();
        for (_, _, floor, run) in floors {
            let Some(r) = self.run(run) else { continue };
            for &wall in &r.segments {
                let depth = self
                    .element(wall)
                    .and_then(|e| e.layer_set)
                    .and_then(|t| self.layer_set(t))
                    .is_some_and(|t| {
                        matches!(t.bearing, Bearing::Depth { .. })
                            && self.bearing_problem(t).is_none()
                    });
                if depth {
                    out.push((wall, floor));
                }
            }
        }
        out
    }

    /// Randdämmstreifen eines Paares (Wand, Decke).
    pub fn edge_strip_of(&self, wall: ElementId, floor: ElementId) -> Option<ElementId> {
        self.elements
            .iter()
            .find(|(_, e)| e.kind == ElementKind::EdgeStrip { wall, floor })
            .map(|(id, _)| id)
    }

    /// Nach dem Laden: Randdämmstreifen passend zu den Typen. Ein ungültiges
    /// Auflager (Regel 21) bleibt im Typ stehen, seine Streifen fallen weg,
    /// damit Decke und Streifen nicht doppelt zählen.
    /// Andere Abweichungen (Regel 22) bleiben stehen und meldet `check()`.
    pub(crate) fn complete_edge_strips(&mut self) {
        let invalid: Vec<ElementId> = self
            .elements
            .iter()
            .filter(|(_, e)| match e.kind {
                ElementKind::EdgeStrip { wall, .. } => self
                    .element(wall)
                    .and_then(|w| w.layer_set)
                    .and_then(|t| self.layer_set(t))
                    .is_some_and(|t| self.bearing_problem(t).is_some()),
                _ => false,
            })
            .map(|(id, _)| id)
            .collect();
        for id in invalid {
            self.elements.remove(id);
            self.touch();
        }
    }

    /// Legt fehlende Randdämmstreifen an und entfernt überzählige, im offenen
    /// Schritt. Bleibt das Paar (Wand, Decke), bleibt der Streifen mit Guid
    /// und Nummer.
    fn sync_edge_strips(&mut self) {
        let want = self.edge_strip_pairs();
        let have: Vec<(ElementId, (ElementId, ElementId))> = self
            .elements
            .iter()
            .filter_map(|(id, e)| match e.kind {
                ElementKind::EdgeStrip { wall, floor } => Some((id, (wall, floor))),
                _ => None,
            })
            .collect();
        let mut kept = Vec::with_capacity(have.len());
        for (id, pair) in have {
            if want.contains(&pair) && !kept.contains(&pair) {
                kept.push(pair);
            } else {
                note!(self, Element, self.elements, id);
                self.elements.remove(id);
                self.touch();
            }
        }
        for (wall, floor) in want {
            if kept.contains(&(wall, floor)) {
                continue;
            }
            let (Some(storey), Some(seq)) = (
                self.element(wall).map(|e| e.storey),
                self.element(floor).map(|e| e.seq),
            ) else {
                continue;
            };
            self.new_element(
                Category::EdgeInsulation,
                storey,
                seq,
                ElementKind::EdgeStrip { wall, floor },
            );
            self.touch();
        }
    }

    /// Bauteil hinter einem Teil des Körpers eines Wandzugs (Treffer beim
    /// Klicken): Segmentnummer, [`SLAB_PART`] oder [`FOOTING_PART`].
    pub fn part_of(&self, run: RunId, part: u32) -> Option<ElementId> {
        match part {
            SLAB_PART => self.foundation_of(run).map(|f| f.0),
            FOOTING_PART => self.foundation_of(run).and_then(|f| f.1),
            FLOOR_PART => self.floor_of(run),
            SOFFIT_PART => self.soffit_of(self.floor_of(run)?),
            p if (STRIP_PART..STRIP_PART + MAX_STRIPS).contains(&p) => {
                let wall = self.wall_at(run, (p - STRIP_PART) as usize)?;
                self.edge_strip_of(wall, self.floor_of(run)?)
            }
            seg => self.wall_at(run, seg as usize),
        }
    }

    fn new_element(
        &mut self,
        category: Category,
        storey: StoreyId,
        seq: u16,
        kind: ElementKind,
    ) -> ElementId {
        let guid = self.new_guid();
        let number = self.next_number(category);
        let id = self.elements.insert(Element {
            guid,
            number,
            category,
            storey,
            layer_set: None,
            seq,
            kind,
            props: PropSet::new(),
        });
        note!(self, Element, new id);
        id
    }

    /// Baustoff neuer Gründungen: Stahlbeton (erster Beton der Bibliothek).
    fn concrete(&self) -> Option<MaterialId> {
        self.materials
            .iter()
            .find(|(_, m)| m.category == MatCategory::Concrete)
            .or_else(|| self.materials.iter().next())
            .map(|(id, _)| id)
    }

    /// Jeder geschlossene Außenwandzug hat genau eine Sohlplatte mit einer
    /// Frostschürze; ist der Zug offen oder weg, verschwinden beide. Im selben
    /// Schritt wie die Änderung am Zug.
    fn sync_foundation(&mut self, run: RunId) {
        let slabs = self.slabs_of(run);
        if !self.needs_foundation(run) {
            for slab in slabs {
                for f in self.footings_of(slab) {
                    note!(self, Element, self.elements, f);
                    self.elements.remove(f);
                }
                note!(self, Element, self.elements, slab);
                self.elements.remove(slab);
            }
            return;
        }
        let (Some(storey), Some(material)) = (self.run(run).map(|r| r.storey), self.concrete())
        else {
            return;
        };
        let slab = match slabs.first() {
            Some(s) => *s,
            None => self.new_element(
                Category::GroundSlab,
                storey,
                SLAB_SEQ,
                ElementKind::GroundSlab(GroundSlab {
                    run,
                    material,
                    top: LevelRef::bottom(storey),
                    thickness: SLAB_THICKNESS,
                    recess: 0.0,
                }),
            ),
        };
        if self.footings_of(slab).is_empty() {
            let Some(gr) = self.foundation_level_of(storey) else {
                return;
            };
            self.new_element(
                Category::StripFooting,
                storey,
                FOOTING_SEQ,
                ElementKind::StripFooting(StripFooting {
                    slab,
                    material,
                    width: 350.0,
                    base: LevelRef::bottom(gr),
                }),
            );
        }
    }

    /// Gründung und Erdgeschossdecke eines Zuges abgleichen.
    fn sync_parts(&mut self, run: RunId) {
        self.sync_foundation(run);
        self.sync_floor(run);
    }

    /// Ergänzt in Dateien vor B9 die Stahlbeton-Schraffur (E15) und die
    /// Gründung unter jedem geschlossenen Außenwandzug. Ohne Rückgängig-Schritt,
    /// gleich nach dem Lesen; liefert Hinweise auf das Ergänzte.
    pub(crate) fn complete_pre_b9(&mut self) -> Vec<String> {
        let mut hints = Vec::new();
        if !self
            .attr
            .fills()
            .iter()
            .any(|(_, f)| f.name == "Stahlbeton")
        {
            let guid = self.new_guid();
            let cross = self.attr.add_fill(crate::attr::Fill {
                guid,
                name: "Stahlbeton".into(),
                kind: crate::attr::FillKind::Lines(crate::attr::concrete_lines()),
                space: crate::attr::FillSpace::Paper,
            });
            let ids: Vec<MaterialId> = self.materials.ids().collect();
            for id in ids {
                if let Some(m) = self
                    .materials
                    .get_mut(id)
                    .filter(|m| m.name == "Stahlbeton")
                {
                    m.cut_fill = cross;
                }
            }
            hints.push("Schraffur „Stahlbeton“ ergänzt".to_string());
        }
        let missing: Vec<RunId> = self
            .runs
            .ids()
            .filter(|r| self.needs_foundation(*r) && self.slabs_of(*r).is_empty())
            .collect();
        for r in &missing {
            self.sync_foundation(*r);
        }
        if !missing.is_empty() {
            hints.push(match missing.len() {
                1 => "Gründung unter dem geschlossenen Außenwandzug ergänzt".to_string(),
                n => format!("Gründung unter {n} geschlossenen Außenwandzügen ergänzt"),
            });
        }
        hints
    }

    /// Geometrie der Gründung unter einem Wandzug; `None` ohne Sohlplatte,
    /// `Err` wenn kein Körper entstehen kann.
    pub fn foundation(&self, run: RunId) -> Option<Result<Foundation, FoundationError>> {
        let (slab, footing) = self.foundation_of(run)?;
        let ElementKind::GroundSlab(s) = self.element(slab)?.kind else {
            return None;
        };
        let ElementKind::StripFooting(f) = self.element(footing?)?.kind else {
            return None;
        };
        if s.recess > 0.0 && s.recess < MIN_RECESS {
            return Some(Err(FoundationError::RecessTooSmall));
        }
        let chain = self.base_chain(run)?;
        let slab_bottom = self.level_z(s.top)? - s.thickness;
        let p = FoundationParams {
            recess: s.recess,
            slab_thickness: s.thickness,
            footing_width: f.width,
            footing_depth: slab_bottom - self.level_z(f.base)?,
            slab_mat: material_key(s.material),
            footing_mat: material_key(f.material),
        };
        Some(Foundation::from_chain(&chain, &p))
    }

    /// Setzt den Sockelrücksprung einer Sohlplatte (mm). Erlaubt sind 0
    /// (bündig) und Werte ab 20 mm; 1 bis 19 mm werden abgelehnt.
    pub fn set_slab_recess(&mut self, slab: ElementId, recess: f64) -> bool {
        if !(recess == 0.0 || recess >= MIN_RECESS) || !recess.is_finite() {
            return false;
        }
        self.edit_slab(slab, |s| s.recess = recess)
    }

    /// Setzt die Dicke einer Sohlplatte (mm, von oben nach unten).
    /// Grenze: Schürze bleibt mindestens 10 cm tief (UK Gründung steht).
    pub fn set_slab_thickness(&mut self, slab: ElementId, thickness: f64) -> bool {
        let bottom = self
            .element(slab)
            .and_then(|e| self.foundation_level_of(e.storey))
            .and_then(|g| self.storey(g))
            .map_or(f64::MIN, |g| g.elevation);
        thickness > 0.0
            && thickness.is_finite()
            && thickness <= -bottom - MIN_FOOTING
            && self.edit_slab(slab, |s| s.thickness = thickness)
    }

    fn edit_slab(&mut self, slab: ElementId, f: impl FnOnce(&mut GroundSlab)) -> bool {
        if !matches!(
            self.element(slab).map(|e| &e.kind),
            Some(ElementKind::GroundSlab(_))
        ) {
            return false;
        }
        note!(self, Element, self.elements, slab);
        if let Some(ElementKind::GroundSlab(s)) = self.elements.get_mut(slab).map(|e| &mut e.kind) {
            f(s);
        }
        self.touch();
        true
    }

    /// Setzt die Breite einer Frostschürze (mm), 30 bis 45 cm (Jörn 17:34:
    /// die Schürze schützt vor Frost, tragend ist die Platte). Alte Dateien
    /// behalten einen Wert außerhalb, bis er geändert wird.
    pub fn set_footing_width(&mut self, footing: ElementId, width: f64) -> bool {
        let (lo, hi) = FOOTING_WIDTH;
        if !(width >= lo - 1e-9 && width <= hi + 1e-9)
            || !matches!(
                self.element(footing).map(|e| &e.kind),
                Some(ElementKind::StripFooting(_))
            )
        {
            return false;
        }
        note!(self, Element, self.elements, footing);
        if let Some(ElementKind::StripFooting(f)) =
            self.elements.get_mut(footing).map(|e| &mut e.kind)
        {
            f.width = width;
        }
        self.touch();
        true
    }

    /// Tiefe einer Frostschürze ab UK Sohlplatte (mm), abgeleitet aus UK
    /// Gründung.
    pub fn footing_depth(&self, footing: ElementId) -> Option<f64> {
        let ElementKind::StripFooting(f) = self.element(footing)?.kind else {
            return None;
        };
        let ElementKind::GroundSlab(s) = self.element(f.slab)?.kind else {
            return None;
        };
        Some(self.level_z(s.top)? - s.thickness - self.level_z(f.base)?)
    }

    /// Schürzentiefe als Zahl: verschiebt UK Gründung (B11), für alle
    /// Schürzen. Ein Wert außerhalb der Grenzen wird abgelehnt.
    pub fn set_footing_depth(&mut self, footing: ElementId, depth: f64) -> bool {
        let (Some(d), true) = (self.footing_depth(footing), depth.is_finite()) else {
            return false;
        };
        let Some(ElementKind::StripFooting(f)) = self.element(footing).map(|e| e.kind.clone())
        else {
            return false;
        };
        let gr = f.base.storey;
        let Some(z) = self.storey(gr).map(|g| g.elevation) else {
            return false;
        };
        self.set_foundation_bottom_of(gr, z - (depth - d))
    }

    // --- Erdgeschossdecke (B10) -------------------------------------------

    /// Decken über einem Wandzug (richtig: höchstens eine).
    fn floors_of(&self, run: RunId) -> Vec<ElementId> {
        self.elements
            .iter()
            .filter(|(_, e)| matches!(e.kind, ElementKind::Floor(f) if f.run == run))
            .map(|(id, _)| id)
            .collect()
    }

    /// Erdgeschossdecke über einem geschlossenen Außenwandzug.
    pub fn floor_of(&self, run: RunId) -> Option<ElementId> {
        self.floors_of(run).first().copied()
    }

    /// Jeder geschlossene Außenwandzug hat genau eine Erdgeschossdecke, ihre
    /// Oberkante wird beim Anlegen aus der Wandhöhe vorbelegt und dann
    /// gespeichert. Ist der Zug offen oder weg, verschwindet sie.
    fn sync_floor(&mut self, run: RunId) {
        let floors = self.floors_of(run);
        if !self.needs_floor(run) {
            for f in floors {
                note!(self, Element, self.elements, f);
                self.elements.remove(f);
            }
            return;
        }
        if !floors.is_empty() {
            return;
        }
        let (Some(r), Some(material)) = (self.run(run), self.concrete()) else {
            return;
        };
        let storey = r.storey;
        let seq = self.storey_seq(storey).1;
        self.new_element(
            Category::Floor,
            storey,
            seq,
            ElementKind::Floor(Floor {
                run,
                material,
                thickness: FLOOR_THICKNESS,
                top: LevelRef::top(storey),
                soffit: Soffit {
                    thickness: SOFFIT_THICKNESS,
                    material: None,
                },
            }),
        );
    }

    /// Ergänzt in Dateien vor B10 die Erdgeschossdecke über jedem
    /// geschlossenen Außenwandzug. Ohne Rückgängig-Schritt; liefert Hinweise.
    pub(crate) fn complete_pre_b10(&mut self) -> Vec<String> {
        let missing: Vec<RunId> = self
            .runs
            .ids()
            .filter(|r| self.needs_floor(*r) && self.floors_of(*r).is_empty())
            .collect();
        for r in &missing {
            self.sync_floor(*r);
        }
        let eg = missing.iter().all(|r| self.needs_foundation(*r));
        match (missing.len(), eg) {
            (0, _) => Vec::new(),
            (1, true) => vec!["Erdgeschossdecke ergänzt".to_string()],
            (n, true) => vec![format!("Erdgeschossdecke über {n} Außenwandzügen ergänzt")],
            (1, false) => vec!["Geschossdecke ergänzt".to_string()],
            (n, false) => vec![format!("Geschossdecken über {n} Außenwandzügen ergänzt")],
        }
    }

    /// Stellt Dateien vor B12 auf Gebäude um: alle Geschosse gehören zu
    /// „Gebäude 1“, das OG wird 2,98 hoch (lichte Höhe 2,76), und über jedem
    /// geschlossenen EG-Außenwandzug entsteht der gekoppelte OG-Zug mit Decke.
    /// Ohne Rückgängig-Schritt; liefert den Hinweis.
    pub(crate) fn complete_pre_b12(&mut self) -> Vec<String> {
        if !self.buildings.is_empty() || self.levels_in(None).is_empty() {
            return Vec::new();
        }
        let guid = self.new_guid();
        let b = self.buildings.insert(Building {
            guid,
            name: "Gebäude 1".into(),
            number: "GB-01".into(),
        });
        self.building_number = self.building_number.max(1);
        let levels = self.levels_in(None);
        let ground = self.ground_of(None);
        let mut z = None;
        for id in levels {
            let Some(st) = self.storeys.get_mut(id) else {
                continue;
            };
            st.building = Some(b);
            if let Some(z) = z {
                st.elevation = z;
                st.height = UPPER_HEIGHT;
            }
            if Some(id) == ground || z.is_some() {
                z = Some(st.top());
            }
        }
        let roots: Vec<RunId> = self
            .runs
            .ids()
            .filter(|r| self.needs_foundation(*r) && self.runs_above(*r).is_empty())
            .collect();
        for r in roots {
            let (mut below, mut at) = match self.run(r) {
                Some(x) => (r, x.storey),
                None => continue,
            };
            while let Some(up) = self.level_above(at) {
                let Some(n) = self.stack_run(below, up) else {
                    break;
                };
                (below, at) = (n, up);
            }
        }
        self.touch();
        vec!["Datei auf Gebäude umgestellt, Obergeschoss ergänzt".to_string()]
    }

    /// Geometrie der Decke über einem Wandzug; `None` ohne Decke, `Err` wenn
    /// kein Körper entstehen kann.
    pub fn floor(&self, run: RunId) -> Option<Result<FloorSlab, FloorError>> {
        self.floor_of_chain(run, &self.base_chain(run)?)
    }

    /// [`Model::floor`] zum schon gebauten Zug `chain` (ohne Anschlüsse).
    fn floor_of_chain(
        &self,
        run: RunId,
        chain: &WallChain,
    ) -> Option<Result<FloorSlab, FloorError>> {
        let id = self.floor_of(run)?;
        let ElementKind::Floor(f) = self.element(id)?.kind else {
            return None;
        };
        let p = FloorParams {
            top: self.level_z(f.top)?,
            thickness: f.thickness,
            mat: material_key(f.material),
        };
        let ext = self
            .overhang_offsets(run)
            .and_then(|o| Some((chain.with_segment_offsets(&o)?, o)));
        let soffit = SoffitParams {
            // Bis 1 m über dem Wandfuß (wie die lichte Höhe, G4)
            thickness: f
                .soffit
                .thickness
                .clamp(MIN_SOFFIT, MAX_SOFFIT)
                .min((p.top - p.thickness - chain.base - MIN_CLEAR).max(0.0)),
            mat: f
                .soffit
                .material
                .or_else(|| self.soffit_material(run))
                .map_or(material::PLAIN, material_key),
        };
        let over = ext.as_ref().map(|(c, o)| (c, &o[..], soffit));
        let mut slab = FloorSlab::from_chain_over(chain, &p, self.strip_params(run), over);
        if let Ok(f) = &mut slab {
            if f.strip.is_some_and(|sp| sp.covered) {
                f.strip_covered = self.covered_segments(run);
            }
        }
        Some(slab)
    }

    /// Versatz nach außen je Segment des Zuges darüber gegenüber `run`, wie
    /// er steht (gekoppelt oder gelöst). `None` ohne Zug darüber oder wenn
    /// die Züge nicht zusammenpassen.
    fn offsets_above(&self, run: RunId) -> Option<Vec<f64>> {
        let up = *self.runs_above(run).first()?;
        self.base_chain(up)?
            .segment_offsets_from(&self.base_chain(run)?)
    }

    /// Vorsprung des Zuges darüber je Segment (≥ 0), wenn mindestens ein
    /// Segment vorspringt (G7 K4): Dort wachsen Decke, Untersichtdämmung und
    /// die Außenschichten mit.
    fn overhang_offsets(&self, run: RunId) -> Option<Vec<f64>> {
        let o: Vec<f64> = self
            .offsets_above(run)?
            .into_iter()
            .map(|d| if d >= MIN_OFFSET { d } else { 0.0 })
            .collect();
        o.iter().any(|d| *d > 0.0).then_some(o)
    }

    /// Je Segment des Zuges: springt es gegenüber dem Zug darunter zurück
    /// (G7 K3)? Dort steht der Fuß neben dem Randdämmstreifen und zeichnet
    /// im Schnitt seine Linie. Bündig oder vorspringend steht er auf dem
    /// Streifen, der mit der Wand darüber wandert (K4).
    fn open_segments(&self, run: RunId) -> Vec<bool> {
        let n = self.run(run).map_or(0, |r| r.segments.len());
        let Some(below) = self.run_below(run) else {
            return vec![true; n];
        };
        match self.offsets_above(below) {
            Some(o) if o.len() == n => o.iter().map(|d| *d <= -MIN_OFFSET).collect(),
            _ => vec![true; n],
        }
    }

    /// Je Segment des Zuges: steht die Wand darüber bündig oder vorspringend
    /// auf dem Randdämmstreifen (G7 K3, K4)? Springt sie zurück, deckt sie ihn
    /// nicht ganz.
    fn covered_segments(&self, run: RunId) -> Vec<bool> {
        let n = self.run(run).map_or(0, |r| r.segments.len());
        match self.offsets_above(run) {
            Some(o) if o.len() == n => o.iter().map(|d| *d > -MIN_OFFSET).collect(),
            _ => vec![false; n],
        }
    }

    /// Baustoff der Untersichtdämmung unter der Decke `floor`: die eigene
    /// Wahl oder [`Model::soffit_material`].
    pub fn soffit_material_of(&self, floor: ElementId) -> Option<MaterialId> {
        let ElementKind::Floor(f) = self.element(floor)?.kind else {
            return None;
        };
        f.soffit.material.or_else(|| self.soffit_material(f.run))
    }

    /// Baustoff der Untersichtdämmung ohne eigene Wahl (K4, BIM): die
    /// äußerste Dämmschicht der Wand darüber; einschalig der Baustoff des
    /// Randdämmstreifens; sonst der erste Dämmstoff im Projekt.
    fn soffit_material(&self, run: RunId) -> Option<MaterialId> {
        let set_of = |r: RunId| {
            self.run(r)?
                .segments
                .first()
                .and_then(|w| self.element(*w))
                .and_then(|e| e.layer_set)
                .and_then(|t| self.layer_set(t))
        };
        let insul = |m: MaterialId| {
            self.material(m)
                .is_some_and(|x| x.category == MatCategory::Insulation)
        };
        let above = self.runs_above(run).first().and_then(|u| set_of(*u));
        let outer = |t: &LayerSet| {
            t.layers
                .iter()
                .take_while(|l| !l.core)
                .map(|l| l.material)
                .find(|m| insul(*m))
        };
        above
            .and_then(outer)
            .or_else(|| set_of(run).and_then(outer))
            .or_else(|| above.and_then(|t| t.strip_material()))
            .or_else(|| set_of(run).and_then(|t| t.strip_material()))
            .or_else(|| {
                self.materials
                    .iter()
                    .find(|(_, m)| m.category == MatCategory::Insulation)
                    .map(|(id, _)| id)
            })
    }

    /// Setzt die Dicke der Untersichtdämmung einer Decke (mm,
    /// [`MIN_SOFFIT`] … [`MAX_SOFFIT`]).
    pub fn set_floor_soffit(&mut self, floor: ElementId, thickness: f64) -> bool {
        if !matches!(
            self.element(floor).map(|e| &e.kind),
            Some(ElementKind::Floor(_))
        ) || !(MIN_SOFFIT..=MAX_SOFFIT).contains(&thickness)
        {
            return false;
        }
        note!(self, Element, self.elements, floor);
        if let Some(ElementKind::Floor(f)) = self.elements.get_mut(floor).map(|e| &mut e.kind) {
            f.soffit.thickness = thickness;
        }
        self.touch();
        true
    }

    /// Setzt den Baustoff der Untersichtdämmung (`None`: wie die Wand darüber).
    pub fn set_floor_soffit_material(&mut self, floor: ElementId, m: Option<MaterialId>) -> bool {
        if !matches!(
            self.element(floor).map(|e| &e.kind),
            Some(ElementKind::Floor(_))
        ) || m.is_some_and(|m| self.material(m).is_none())
        {
            return false;
        }
        note!(self, Element, self.elements, floor);
        if let Some(ElementKind::Floor(f)) = self.elements.get_mut(floor).map(|e| &mut e.kind) {
            f.soffit.material = m;
        }
        self.touch();
        true
    }

    /// Decken, unter denen eine Untersichtdämmung liegen muss (Regel 35):
    /// die Decke kragt unter einem Vorsprung darüber aus.
    pub fn soffit_floors(&self) -> Vec<ElementId> {
        let mut out: Vec<(String, ElementId)> = self
            .elements
            .iter()
            .filter_map(|(id, e)| match e.kind {
                ElementKind::Floor(f) => match self.floor(f.run) {
                    Some(Ok(s)) if !s.soffits.is_empty() => Some((e.number.clone(), id)),
                    _ => None,
                },
                _ => None,
            })
            .collect();
        out.sort_by(|a, b| a.0.cmp(&b.0));
        out.into_iter().map(|x| x.1).collect()
    }

    /// Untersichtdämmung unter einer Decke.
    pub fn soffit_of(&self, floor: ElementId) -> Option<ElementId> {
        self.elements
            .iter()
            .find(|(_, e)| e.kind == ElementKind::SoffitInsulation { floor })
            .map(|(id, _)| id)
    }

    /// Legt fehlende Untersichtdämmungen an und entfernt überzählige (im
    /// offenen Schritt oder nach dem Laden). Bleibt die Decke auskragend,
    /// bleibt die Dämmung mit Guid und Nummer; entsteht sie neu, bekommt sie
    /// eine neue (Regel 25).
    fn sync_soffits(&mut self) {
        let want = self.soffit_floors();
        let have: Vec<(ElementId, ElementId)> = self
            .elements
            .iter()
            .filter_map(|(id, e)| match e.kind {
                ElementKind::SoffitInsulation { floor } => Some((id, floor)),
                _ => None,
            })
            .collect();
        let mut kept = Vec::with_capacity(have.len());
        for (id, floor) in have {
            if want.contains(&floor) && !kept.contains(&floor) {
                kept.push(floor);
            } else {
                note!(self, Element, self.elements, id);
                self.elements.remove(id);
                self.touch();
            }
        }
        for floor in want {
            if kept.contains(&floor) {
                continue;
            }
            let Some((storey, seq)) = self.element(floor).map(|e| (e.storey, e.seq)) else {
                continue;
            };
            self.new_element(
                Category::SoffitInsulation,
                storey,
                seq,
                ElementKind::SoffitInsulation { floor },
            );
            self.touch();
        }
    }

    /// Nach dem Laden: Untersichtdämmungen passend zu den Vorsprüngen.
    pub(crate) fn complete_soffits(&mut self) {
        let strict = std::mem::replace(&mut self.strict, false);
        self.sync_soffits();
        self.strict = strict;
    }

    /// Randdämmstreifen der Decke über dem Zug: aus dem Typ seiner Wände (K5).
    fn strip_params(&self, run: RunId) -> Option<StripParams> {
        let t = self
            .run(run)?
            .segments
            .first()
            .and_then(|w| self.element(*w))
            .and_then(|e| e.layer_set)
            .and_then(|t| self.layer_set(t))?;
        // Ungültiges Auflager (Regel 21): wie „ganze tragende Schicht“
        if self.bearing_problem(t).is_some() {
            return None;
        }
        let core = t.layers.iter().find(|l| l.core).or(t.layers.first())?;
        Some(StripParams {
            width: t.strip_width()?,
            mat: material_key(t.strip_material()?),
            face: material_key(core.material),
            covered: !self.runs_above(run).is_empty(),
        })
    }

    /// Setzt die Dicke einer Decke (mm, von der Oberkante nach unten).
    /// Grenze: lichte Höhe darunter mindestens 1,00 m (OK bleibt stehen).
    pub fn set_floor_thickness(&mut self, floor: ElementId, thickness: f64) -> bool {
        let Some(ElementKind::Floor(f)) = self.element(floor).map(|e| &e.kind) else {
            return false;
        };
        let room = self.level_z(f.top).unwrap_or(0.0)
            - self.storey(f.top.storey).map_or(0.0, |s| s.elevation);
        if !(thickness > 0.0 && thickness.is_finite() && thickness <= room - MIN_CLEAR) {
            return false;
        }
        note!(self, Element, self.elements, floor);
        if let Some(ElementKind::Floor(f)) = self.elements.get_mut(floor).map(|e| &mut e.kind) {
            f.thickness = thickness;
        }
        self.touch();
        true
    }

    /// Hinweise, die keinen Fehler darstellen: Der Körper entsteht, aber etwas
    /// ist ungewöhnlich (Paneel „Eigenschaften“).
    pub fn warnings(&self, e: ElementId) -> Vec<String> {
        let mut out = Vec::new();
        // Ungültiges Auflager aus einer Datei (Regel 21): gebaut wie „ganze
        // tragende Schicht“
        if self
            .element(e)
            .filter(|x| matches!(x.kind, ElementKind::Wall(_)))
            .and_then(|x| x.layer_set)
            .and_then(|t| self.layer_set(t))
            .is_some_and(|t| self.bearing_problem(t).is_some())
        {
            out.push("Deckenauflager des Typs ungültig".into());
        }
        let Some(run) = self.run_of(e) else {
            return out;
        };
        if matches!(
            self.element(e).map(|x| &x.kind),
            Some(ElementKind::Floor(_))
        ) {
            return out;
        }
        let Some((slab, _)) = self.foundation_of(run) else {
            return out;
        };
        if self
            .element(e)
            .is_some_and(|x| matches!(x.kind, ElementKind::Wall(_)))
        {
            return out;
        }
        let Some(&ElementKind::GroundSlab(s)) = self.element(slab).map(|x| &x.kind) else {
            return out;
        };
        // Schichten außen vor dem tragenden Kern (bei AW 31,5 die Dämmung)
        let outside: f64 = self
            .run(run)
            .and_then(|r| r.segments.first())
            .and_then(|w| self.element(*w))
            .and_then(|w| w.layer_set)
            .and_then(|id| self.layer_set(id))
            .map_or(0.0, |set| {
                set.layers
                    .iter()
                    .take_while(|l| !l.core)
                    .map(|l| l.thickness)
                    .sum()
            });
        if s.recess > outside && outside > 0.0 {
            // Rücksprung größer als die Schichten vor dem Kern
            out.push("Kern steht teils neben Platte".into());
        }
        if let Some(Ok(f)) = self.foundation(run) {
            if matches!(f.footing, FootingShape::Full(_)) {
                out.push("Schürze füllt die Platte ganz".into());
            }
        }
        out
    }

    // --- Geschossbänder (B11) ----------------------------------------------

    /// Absolute Höhe eines Höhenbezugs (mm), `None` ohne das Geschoss.
    pub fn level_z(&self, r: LevelRef) -> Option<f64> {
        let s = self.storey(r.storey)?;
        Some(
            match r.edge {
                LevelEdge::Bottom => s.elevation,
                LevelEdge::Top => s.top(),
            } + r.offset,
        )
    }

    /// Geschosse des Gebäudes `b` von unten nach oben (`None`: die Vorlage).
    pub fn levels_in(&self, b: Option<BuildingId>) -> Vec<StoreyId> {
        let mut v: Vec<(StoreyId, f64)> = self
            .storeys
            .iter()
            .filter(|(_, s)| s.building == b)
            .map(|(id, s)| (id, s.elevation))
            .collect();
        v.sort_by(|a, b| a.1.total_cmp(&b.1));
        v.into_iter().map(|(id, _)| id).collect()
    }

    /// Geschosse des Gebäudes, zu dem `id` gehört, von unten nach oben.
    pub fn group_levels(&self, id: StoreyId) -> Vec<StoreyId> {
        match self.storey(id) {
            Some(s) => self.levels_in(s.building),
            None => Vec::new(),
        }
    }

    /// Geschosse des ersten Gebäudes (bzw. der Vorlage) von unten nach oben.
    pub fn levels(&self) -> Vec<StoreyId> {
        self.group_levels(self.defaults.storey)
    }

    /// Das Gründungsband des ersten Gebäudes (bzw. der Vorlage).
    pub fn foundation_level(&self) -> Option<StoreyId> {
        self.foundation_level_of(self.defaults.storey)
    }

    /// Das Gründungsband des Gebäudes, zu dem `id` gehört.
    pub fn foundation_level_of(&self, id: StoreyId) -> Option<StoreyId> {
        self.group_levels(id).into_iter().find(|g| {
            self.storey(*g)
                .is_some_and(|s| s.kind == LevelKind::Foundation)
        })
    }

    /// Geschoss über `id` im selben Gebäude.
    pub fn level_above(&self, id: StoreyId) -> Option<StoreyId> {
        let l = self.group_levels(id);
        let i = l.iter().position(|x| *x == id)?;
        l.get(i + 1).copied()
    }

    /// Geschoss unter `id` im selben Gebäude (beim EG die Gründung).
    pub fn level_below(&self, id: StoreyId) -> Option<StoreyId> {
        let l = self.group_levels(id);
        let i = l.iter().position(|x| *x == id)?;
        l.get(i.checked_sub(1)?).copied()
    }

    /// Dickste Decke an der OK des Geschosses `id` (bestimmt dessen lichte
    /// Höhe), ohne Decke der Standardwert.
    fn floor_thickness_of(&self, id: StoreyId) -> f64 {
        self.elements
            .iter()
            .filter_map(|(_, e)| match e.kind {
                ElementKind::Floor(f) if f.top.storey == id => Some(f.thickness),
                _ => None,
            })
            .reduce(f64::max)
            .unwrap_or(FLOOR_THICKNESS)
    }

    /// Dickste Sohlplatte im Gebäude der Gründung `gr`, ohne Platte der
    /// Standardwert.
    pub fn max_slab_thickness(&self, gr: StoreyId) -> f64 {
        let b = self.building_of(gr);
        self.elements
            .iter()
            .filter_map(|(_, e)| match e.kind {
                ElementKind::GroundSlab(s) if self.building_of(s.top.storey) == b => {
                    Some(s.thickness)
                }
                _ => None,
            })
            .reduce(f64::max)
            .unwrap_or(SLAB_THICKNESS)
    }

    /// Lichte Höhe eines Geschosses: Geschosshöhe minus Deckendicke.
    pub fn clear_height(&self, id: StoreyId) -> f64 {
        self.storey(id)
            .map_or(0.0, |s| s.height - self.floor_thickness_of(id))
    }

    /// Erlaubter Bereich der Oberkante eines Geschosses; `None`, wenn sie
    /// fest liegt (Gründung: ±0,00). Die lichte Höhe bleibt mindestens
    /// 1,00 m; nach oben frei, die Wände gehen mit (B12).
    pub fn storey_top_range(&self, id: StoreyId) -> Option<(f64, f64)> {
        let s = self.storey(id)?;
        match s.kind {
            LevelKind::Foundation => None,
            _ => Some((
                s.elevation + self.floor_thickness_of(id) + MIN_CLEAR,
                f64::INFINITY,
            )),
        }
    }

    /// Erlaubter Bereich der Unterkante der Gründung `gr`.
    pub fn foundation_bottom_range_of(&self, gr: StoreyId) -> (f64, f64) {
        let eg = self
            .ground_storey(gr)
            .and_then(|e| self.storey(e))
            .map_or(0.0, |s| s.elevation);
        (
            eg - MAX_FOUNDATION,
            eg - self.max_slab_thickness(gr) - MIN_FOOTING,
        )
    }

    /// Erlaubter Bereich der Unterkante der Gründung des ersten Gebäudes.
    pub fn foundation_bottom_range(&self) -> (f64, f64) {
        match self.foundation_level() {
            Some(gr) => self.foundation_bottom_range_of(gr),
            None => (-MAX_FOUNDATION, -SLAB_THICKNESS - MIN_FOOTING),
        }
    }

    /// Oberkante auf `z` geklemmt (Ziehen).
    pub fn clamp_storey_top(&self, id: StoreyId, z: f64) -> Option<f64> {
        let (lo, hi) = self.storey_top_range(id)?;
        Some(z.min(hi).max(lo))
    }

    /// Verschiebt die Oberkante ohne Prüfung; die Bänder darüber (im selben
    /// Gebäude) wandern mit.
    fn move_storey_top(&mut self, id: StoreyId, z: f64) {
        let Some(old) = self.storey(id).map(|s| s.top()) else {
            return;
        };
        let delta = z - old;
        if delta == 0.0 {
            return;
        }
        let levels = self.group_levels(id);
        let Some(i) = levels.iter().position(|x| *x == id) else {
            return;
        };
        note!(self, Storey, self.storeys, id);
        if let Some(s) = self.storeys.get_mut(id) {
            s.height += delta;
        }
        for &above in &levels[i + 1..] {
            note!(self, Storey, self.storeys, above);
            if let Some(s) = self.storeys.get_mut(above) {
                s.elevation += delta;
            }
        }
        self.touch();
    }

    /// Oberkante eines Geschosses als Zahl (mm). Außerhalb der Grenzen
    /// abgelehnt; ±0,00 liegt fest.
    pub fn set_storey_top(&mut self, id: StoreyId, z: f64) -> bool {
        match self.storey_top_range(id) {
            Some((lo, hi)) if z.is_finite() && z >= lo - 1e-9 && z <= hi + 1e-9 => {
                self.move_storey_top(id, z);
                true
            }
            _ => false,
        }
    }

    /// Oberkante beim Ziehen: an den Grenzen geklemmt.
    pub fn drag_storey_top(&mut self, id: StoreyId, z: f64) -> bool {
        match self.clamp_storey_top(id, z) {
            Some(z) if z.is_finite() => {
                self.move_storey_top(id, z);
                true
            }
            _ => false,
        }
    }

    /// Geschosshöhe eines Geschosses ohne Grenzprüfung (Vorgaben des Dialogs
    /// „Gebäude erstellen“, die der Aufrufer prüft); die Geschosse darüber
    /// rücken mit.
    pub fn plan_storey_height(&mut self, id: StoreyId, h: f64) {
        if let Some(e) = self.storey(id).map(|s| s.elevation) {
            if h.is_finite() && h > 0.0 {
                self.move_storey_top(id, e + h);
            }
        }
    }

    /// Geschosshöhe als Zahl; bei der Gründung die Gründungstiefe.
    pub fn set_storey_height(&mut self, id: StoreyId, h: f64) -> bool {
        let Some(s) = self.storey(id) else {
            return false;
        };
        match s.kind {
            LevelKind::Foundation => self.set_foundation_depth_of(id, h),
            _ => {
                let z = s.elevation + h;
                self.set_storey_top(id, z)
            }
        }
    }

    /// Lichte Höhe als Zahl: OK = UK + lichte Höhe + Deckendicke.
    pub fn set_clear_height(&mut self, id: StoreyId, h: f64) -> bool {
        let Some(e) = self.storey(id).map(|s| s.elevation) else {
            return false;
        };
        let z = e + h + self.floor_thickness_of(id);
        self.set_storey_top(id, z)
    }

    /// Unterkante der Gründung `gr` ohne Prüfung.
    fn move_foundation_bottom(&mut self, gr: StoreyId, z: f64) {
        let Some(top) = self.storey(gr).map(|s| s.top()) else {
            return;
        };
        if self.storey(gr).is_some_and(|s| s.elevation == z) {
            return;
        }
        note!(self, Storey, self.storeys, gr);
        if let Some(s) = self.storeys.get_mut(gr) {
            s.elevation = z;
            s.height = top - z;
        }
        self.touch();
    }

    /// Unterkante der Gründung `gr` als Zahl (mm); außerhalb abgelehnt.
    pub fn set_foundation_bottom_of(&mut self, gr: StoreyId, z: f64) -> bool {
        let (lo, hi) = self.foundation_bottom_range_of(gr);
        if !(z.is_finite() && z >= lo - 1e-9 && z <= hi + 1e-9) {
            return false;
        }
        self.move_foundation_bottom(gr, z);
        true
    }

    /// Unterkante der Gründung des ersten Gebäudes als Zahl (mm).
    pub fn set_foundation_bottom(&mut self, z: f64) -> bool {
        self.foundation_level()
            .is_some_and(|gr| self.set_foundation_bottom_of(gr, z))
    }

    /// Unterkante der Gründung `gr` beim Ziehen, geklemmt.
    pub fn drag_foundation_bottom_of(&mut self, gr: StoreyId, z: f64) -> bool {
        if !z.is_finite() || self.storey(gr).is_none() {
            return false;
        }
        let (lo, hi) = self.foundation_bottom_range_of(gr);
        self.move_foundation_bottom(gr, z.min(hi).max(lo));
        true
    }

    /// Unterkante der Gründung des ersten Gebäudes beim Ziehen, geklemmt.
    pub fn drag_foundation_bottom(&mut self, z: f64) -> bool {
        self.foundation_level()
            .is_some_and(|gr| self.drag_foundation_bottom_of(gr, z))
    }

    /// Gründungstiefe der Gründung `gr` als Zahl: UK Gründung = −Tiefe.
    pub fn set_foundation_depth_of(&mut self, gr: StoreyId, h: f64) -> bool {
        let eg = self
            .ground_storey(gr)
            .and_then(|e| self.storey(e))
            .map_or(0.0, |s| s.elevation);
        self.set_foundation_bottom_of(gr, eg - h)
    }

    /// Gründungstiefe des ersten Gebäudes als Zahl.
    pub fn set_foundation_depth(&mut self, h: f64) -> bool {
        self.foundation_level()
            .is_some_and(|gr| self.set_foundation_depth_of(gr, h))
    }

    /// Prüfregeln der Geschossbänder (B11, B12): je Gebäude eine Gründung
    /// zuunterst und ein EG bei ±0,00, lückenlos.
    fn check_levels(&self) -> Vec<String> {
        let mut out = Vec::new();
        let mut groups: Vec<Option<BuildingId>> =
            self.buildings.ids().map(Some).collect::<Vec<_>>();
        if self.storeys.iter().any(|(_, s)| s.building.is_none()) {
            if !self.buildings.is_empty() {
                out.push("Geschosse: Vorlage ohne Gebäude neben einem Gebäude".into());
            }
            groups.push(None);
        }
        for (_, s) in self.storeys.iter() {
            if s.building.is_some_and(|b| !self.buildings.contains(b)) {
                out.push(format!("Geschoss {}: Gebäude fehlt", s.short));
            }
        }
        for g in groups {
            let name = g
                .and_then(|b| self.building(b))
                .map_or("Vorlage".to_string(), |b| b.number.clone());
            let levels = self.levels_in(g);
            let found = levels
                .iter()
                .filter(|id| {
                    self.storey(**id)
                        .is_some_and(|s| s.kind == LevelKind::Foundation)
                })
                .count();
            if found != 1 || levels.len() < 2 {
                out.push(format!(
                    "Geschosse {name}: nicht genau eine Gründung und mindestens ein EG"
                ));
            }
            let first = levels.first().and_then(|id| self.storey(*id));
            if first.is_some_and(|s| s.kind != LevelKind::Foundation) {
                out.push(format!(
                    "Geschosse {name}: Gründung ist nicht das unterste Band"
                ));
            }
            for w in levels.windows(2) {
                let (a, b) = (self.storey(w[0]), self.storey(w[1]));
                if let (Some(a), Some(b)) = (a, b) {
                    if (a.top() - b.elevation).abs() > 1e-6 {
                        out.push(format!(
                            "Geschosse {} und {} nicht lückenlos",
                            a.short, b.short
                        ));
                    }
                }
            }
            if self
                .ground_of(g)
                .and_then(|e| self.storey(e))
                .is_none_or(|s| s.elevation != 0.0)
            {
                out.push(format!("Geschosse {name}: UK EG liegt nicht bei ±0,00"));
            }
        }
        for (_, s) in self.storeys.iter() {
            if !(s.height > 0.0 && s.height.is_finite()) {
                out.push(format!("Geschoss {}: Höhe {} ungültig", s.short, s.height));
            }
        }
        let mut refs: Vec<LevelRef> = self
            .runs
            .iter()
            .flat_map(|(_, r)| [r.base, r.top])
            .collect();
        for (_, e) in self.elements.iter() {
            match e.kind {
                ElementKind::GroundSlab(s) => refs.push(s.top),
                ElementKind::StripFooting(f) => refs.push(f.base),
                ElementKind::Floor(f) => refs.push(f.top),
                ElementKind::Wall(_)
                | ElementKind::EdgeStrip { .. }
                | ElementKind::SoffitInsulation { .. } => {}
            }
        }
        if refs.iter().any(|r| !self.storeys.contains(r.storey)) {
            out.push("Höhenbezug auf ein fehlendes Geschoss".into());
        }
        out
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
            ends: JoinEnd::BOTH.map(|e| chain.end_frame(e.index())),
            frames: chain.segment_frames(),
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
    fn detect_end(
        &self,
        run: RunId,
        e: JoinEnd,
        boxes: &[(RunId, Vec3, Vec3)],
        bases: &mut Bases,
    ) -> Option<Join> {
        let r = self.runs.get(run)?;
        let p = Model::end_point(r, e)?;
        let storey = r.storey;
        // Anschlüsse nur innerhalb eines Geschosses (B12)
        let ids: Vec<RunId> = boxes
            .iter()
            .filter(|(id, lo, hi)| {
                *id != run
                    && inside(p, *lo, *hi)
                    && self.runs.get(*id).is_some_and(|x| x.storey == storey)
            })
            .map(|(id, _, _)| *id)
            .collect();
        for &id in ids.iter().chain([&run]) {
            bases.fill(self, id);
        }
        let a = bases.get(run)?;
        let cands: Vec<&Base> = ids.iter().filter_map(|id| bases.get(*id)).collect();
        Model::detect(a, e, &cands)
    }

    /// Erkennt den Anschluss des freien Endes `e` von `a`: L, wenn ein freies
    /// Ende eines anderen Zuges höchstens [`join::SNAP`] entfernt liegt, sonst
    /// T an das nächste nicht parallele Segment, in dessen Grundriss oder
    /// höchstens [`join::SNAP`] vor dessen Fläche das Ende liegt.
    fn detect(a: &Base, e: JoinEnd, all: &[&Base]) -> Option<Join> {
        let (p, da) = a.ends[e.index()]?;
        let elem = |b: &Base, e: JoinEnd| match e {
            JoinEnd::Start => b.segments.first().copied(),
            JoinEnd::End => b.segments.last().copied(),
        };
        let others = || all.iter().filter(|b| b.run != a.run);
        let mut l: Option<(f64, &Base, JoinEnd, Vec3, Vec3)> = None;
        for b in others() {
            for eb in JoinEnd::BOTH {
                let Some((q, db)) = b.ends[eb.index()] else {
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
                let Some(&(_, db)) = b.frames.get(k) else {
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
        let mut bases = Bases::default();
        let mut out: Vec<Join> = self
            .runs
            .ids()
            .flat_map(|r| JoinEnd::BOTH.map(|e| (r, e)))
            .filter_map(|(r, e)| self.detect_end(r, e, &boxes, &mut bases))
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
        let mut bases = Bases::default();
        let fresh: Vec<Join> = ends
            .iter()
            .filter_map(|&(r, e)| self.detect_end(r, e, &boxes, &mut bases))
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

    /// Gegenstück zu [`Model::require_steps`]: Änderungen auch ohne Schritt,
    /// z. B. an einer Kopie des App-Modells zum Ausprobieren (Tests).
    pub fn allow_unstepped(&mut self) {
        self.strict = false;
    }

    /// Öffnet einen Schritt. Es darf keiner offen sein ([`Model::commit`] vorher).
    pub fn begin(&mut self, label: &'static str) {
        debug_assert!(self.txn.is_none(), "Schritt schon offen");
        self.txn = Some(Open {
            building_number: self.building_number,
            label,
            changes: Vec::new(),
            noted: Default::default(),
        });
    }

    pub fn in_step(&self) -> bool {
        self.txn.is_some()
    }

    /// Schließt den offenen Schritt. `None`, wenn er nichts geändert hat.
    /// Randdämmstreifen folgen Wänden, Decken und Typen im selben Schritt.
    pub fn commit(&mut self) -> Option<Txn> {
        if self.txn.is_some() {
            self.sync_edge_strips();
            self.sync_soffits();
        }
        self.close()
    }

    fn close(&mut self) -> Option<Txn> {
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
        if let Some(open) = &self.txn {
            self.building_number = open.building_number;
        }
        match self.close() {
            Some(t) => self.apply(&t, Direction::Undo),
            None => Touched::default(),
        }
    }

    /// Wandzüge, die der offene Schritt bisher berührt, samt Schürzen-,
    /// Decken- und Anschlusspartnern wie bei [`Model::apply`]: für eine
    /// Live-Vorschau, die nur diese Züge neu rechnet.
    pub fn step_touched(&self) -> Vec<RunId> {
        let mut t = Touched::default();
        let Some(open) = &self.txn else {
            return Vec::new();
        };
        let (mut footings, mut floors, mut strips) = (Vec::new(), Vec::new(), Vec::new());
        for c in &open.changes {
            match c {
                Change::Run { id, .. } => t.run(*id),
                Change::Element { id, old, .. } => {
                    for e in [old.as_ref(), self.elements.get(*id)].into_iter().flatten() {
                        match e.kind {
                            ElementKind::Wall(w) => t.run(w.run),
                            ElementKind::GroundSlab(s) => t.run(s.run),
                            ElementKind::StripFooting(f) => footings.push(f.slab),
                            ElementKind::Floor(f) => {
                                t.run(f.run);
                                floors.push(f.run);
                            }
                            ElementKind::EdgeStrip { wall, .. } => strips.push(wall),
                            ElementKind::SoffitInsulation { floor } => strips.push(floor),
                        }
                    }
                }
                _ => {}
            }
        }
        for slab in footings.into_iter().chain(strips) {
            if let Some(r) = self.run_of(slab) {
                t.run(r);
            }
        }
        for r in floors {
            self.runs_under_floor(r).into_iter().for_each(|i| t.run(i));
        }
        self.stack_partners(&mut t);
        let runs = t.runs.clone();
        for r in runs {
            self.joined_runs(r).into_iter().for_each(|p| t.run(p));
        }
        t.runs
    }

    /// Ergänzt die Züge darunter und darüber: Der Versatz zwischen ihnen
    /// formt Decke und Außenschichten unten (G7 K4) und den Fuß oben (K3).
    fn stack_partners(&self, t: &mut Touched) {
        let runs = t.runs.clone();
        for r in runs {
            if self.run(r).is_none() {
                continue;
            }
            self.run_below(r).into_iter().for_each(|b| t.run(b));
            self.runs_above(r).into_iter().for_each(|u| t.run(u));
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
            Change::Building { id, new, .. } => *new = self.buildings.get(*id).cloned(),
            Change::Pen { id, new, .. } => *new = self.attr.pen(*id).cloned(),
            Change::LineType { id, new, .. } => *new = self.attr.line_type(*id).cloned(),
            Change::Fill { id, new, .. } => *new = self.attr.fill(*id).cloned(),
            Change::Surface { id, new, .. } => *new = self.attr.surface(*id).cloned(),
            Change::Display { new, .. } => *new = self.attr.display().clone(),
            Change::Defaults { new, .. } => *new = self.defaults,
        }
    }

    /// Macht einen Schritt rückgängig oder wiederholt ihn. Kennungen bleiben
    /// erhalten; Guid-Erzeuger und Nummernzähler laufen weiter, die Revision steigt.
    pub fn apply(&mut self, t: &Txn, dir: Direction) -> Touched {
        debug_assert!(self.txn.is_none(), "Rückgängig in einem offenen Schritt");
        let mut touched = Touched::default();
        let mut footings = Vec::new();
        let mut floors = Vec::new();
        let mut apply_one = |m: &mut Model, c: &Change| match c {
            Change::Run { id, old, new } => {
                m.runs.set(*id, pick(dir, old, new));
                touched.run(*id);
            }
            Change::Element { id, old, new } => {
                for e in [old, new].into_iter().flatten() {
                    match e.kind {
                        ElementKind::Wall(w) => touched.run(w.run),
                        ElementKind::GroundSlab(s) => touched.run(s.run),
                        // Wand oder Platte stehen ggf. selbst im Schritt
                        ElementKind::StripFooting(f) => footings.push(f.slab),
                        ElementKind::EdgeStrip { wall, .. } => footings.push(wall),
                        ElementKind::SoffitInsulation { floor } => footings.push(floor),
                        ElementKind::Floor(f) => {
                            touched.run(f.run);
                            floors.push(f.run);
                        }
                    }
                }
                m.elements.set(*id, pick(dir, old, new));
            }
            Change::LayerSet { id, old, new } => {
                m.layer_sets.set(*id, pick(dir, old, new));
                touched.library = true;
            }
            Change::Material { id, old, new } => {
                // Nur Darstellungsverweise geändert: allein die Zeichentabelle
                if bim_equal(old.as_ref(), new.as_ref()) {
                    touched.attr = true;
                } else {
                    touched.library = true;
                }
                m.materials.set(*id, pick(dir, old, new));
            }
            Change::Storey { id, old, new } => {
                m.storeys.set(*id, pick(dir, old, new));
                touched.library = true;
            }
            Change::Building { id, old, new } => {
                m.buildings.set(*id, pick(dir, old, new));
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
            Change::Defaults { old, new } => m.defaults = pick(dir, old, new),
        };
        // Rückwärts in umgekehrter Reihenfolge: ein Platz wird erst frei, dann neu belegt
        match dir {
            Direction::Undo => t.changes.iter().rev().for_each(|c| apply_one(self, c)),
            Direction::Redo => t.changes.iter().for_each(|c| apply_one(self, c)),
        }
        // Schürze geändert: ihr Zug ist der ihrer Platte (eine gelöschte Platte
        // steht selbst im Schritt)
        for slab in footings {
            if let Some(r) = self.run_of(slab) {
                touched.run(r);
            }
        }
        // Decke geändert: auch die Innenwände darunter (Deckenband)
        for r in floors {
            self.runs_under_floor(r)
                .into_iter()
                .for_each(|i| touched.run(i));
        }
        self.stack_partners(&mut touched);
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

    /// Prüfregel Kopplung (B12): Partner lebt, steht im Geschoss direkt
    /// darunter im selben Gebäude, und das Segment liegt parallel auf
    /// Partnerlinie + Versatz (Toleranz 0,01 mm).
    fn check_coupling(&self, id: ElementId, number: &str, w: Wall, c: Coupling) -> Vec<String> {
        let mut out = Vec::new();
        let (Some(me), Some(partner)) = (self.element(id), self.element(c.below)) else {
            out.push(format!("{number}: gekoppelte Wand darunter fehlt"));
            return out;
        };
        let ElementKind::Wall(pw) = partner.kind else {
            out.push(format!(
                "{number}: gekoppelt an ein Bauteil, das keine Wand ist"
            ));
            return out;
        };
        if self.level_below(me.storey) != Some(partner.storey) {
            out.push(format!(
                "{number}: gekoppelte Wand {} steht nicht im Geschoss darunter",
                partner.number
            ));
            return out;
        }
        let offsets = self
            .base_chain(w.run)
            .zip(self.base_chain(pw.run))
            .and_then(|(a, b)| {
                let k = self.segment_of(c.below)?.1;
                let la = a.segment_frame(w.seg as usize)?;
                let lb = b.segment_frame(k)?;
                // parallel, gleiche Richtung, Abstand quer zur Wand nach außen
                let par = cross2(la.1, lb.1).abs() < 1e-9 && la.1.dot(lb.1) > 0.0;
                let n = vec3(lb.1.y, -lb.1.x, 0.0);
                par.then(|| (la.0 - lb.0).dot(n) * b.outward_sign())
            });
        match offsets {
            Some(d) if (d - c.offset).abs() <= 0.01 => {}
            Some(d) => out.push(format!(
                "{number}: liegt {d:.2} mm statt {:.2} mm neben {}",
                c.offset, partner.number
            )),
            None => out.push(format!("{number}: nicht parallel zu {}", partner.number)),
        }
        out
    }

    /// Prüft die Strukturregeln des BIM-Konzepts (Abschnitt 8) und liefert
    /// die Verstöße als Text.
    pub fn check(&self) -> Vec<String> {
        let mut out = Vec::new();
        let strips_wanted = self.edge_strip_pairs();
        let soffits_wanted = self.soffit_floors();
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
                match self.layer_set(s) {
                    None => out.push(format!("{}: Aufbau fehlt", e.number)),
                    // Regel 16: der Typ passt zur Wand
                    Some(t) if TypeCategory::of(e.category) != Some(t.category) => {
                        out.push(format!(
                            "{}: Typ {} passt nicht zur {}",
                            e.number,
                            t.code,
                            e.category.name()
                        ))
                    }
                    Some(_) => {}
                }
            } else if TypeCategory::of(e.category).is_some() {
                out.push(format!("{}: kein Bauteiltyp", e.number));
            }
            match e.kind {
                ElementKind::Wall(w) => {
                    if self.wall_at(w.run, w.seg as usize) != Some(id) {
                        out.push(format!("{}: nicht im Wandzug eingetragen", e.number));
                    }
                    if let Some(c) = w.coupling {
                        out.extend(self.check_coupling(id, &e.number, w, c));
                    }
                }
                ElementKind::GroundSlab(s) => {
                    if !self.needs_foundation(s.run) {
                        out.push(format!(
                            "{}: kein geschlossener Außenwandzug im EG darüber",
                            e.number
                        ));
                    }
                    if self.footings_of(id).len() != 1 {
                        out.push(format!("{}: nicht genau eine Frostschürze", e.number));
                    }
                    if s.recess > 0.0 && s.recess < MIN_RECESS {
                        out.push(format!(
                            "{}: Sockelrücksprung {} mm (0 oder ab {MIN_RECESS} mm)",
                            e.number, s.recess
                        ));
                    }
                    if let Some(Err(err)) = self.foundation(s.run) {
                        out.push(format!("{}: keine Gründung, {}", e.number, explain(err)));
                    }
                    if !self.materials.contains(s.material) {
                        out.push(format!("{}: Baustoff fehlt", e.number));
                    }
                }
                ElementKind::StripFooting(f) => {
                    if self.footing_depth(id).is_none_or(|d| d <= 0.0) {
                        out.push(format!("{}: Tiefe nicht größer als 0", e.number));
                    }
                    if !matches!(
                        self.element(f.slab).map(|x| &x.kind),
                        Some(ElementKind::GroundSlab(_))
                    ) {
                        out.push(format!("{}: Sohlplatte fehlt", e.number));
                    }
                    if !self.materials.contains(f.material) {
                        out.push(format!("{}: Baustoff fehlt", e.number));
                    }
                }
                ElementKind::Floor(f) => {
                    if !self.needs_floor(f.run) {
                        out.push(format!(
                            "{}: kein geschlossener Außenwandzug darunter",
                            e.number
                        ));
                    }
                    if !(f.thickness > 0.0 && f.thickness.is_finite()) {
                        out.push(format!("{}: Dicke {} ungültig", e.number, f.thickness));
                    }
                    let crown = self
                        .run(f.run)
                        .and_then(|r| self.level_z(r.top))
                        .unwrap_or(0.0);
                    if self
                        .run(f.run)
                        .is_some_and(|r| f.top != LevelRef::top(r.storey))
                    {
                        out.push(format!("{}: nicht an der OK des Geschosses", e.number));
                    }
                    match self.level_z(f.top) {
                        Some(top) if top - f.thickness > 0.0 && top <= crown + 1e-6 => {}
                        Some(top) => out.push(format!(
                            "{}: Lage OK {} / UK {} außerhalb der Wand",
                            e.number,
                            top,
                            top - f.thickness
                        )),
                        None => out.push(format!("{}: Geschoss der Oberkante fehlt", e.number)),
                    }
                    if let Some(Err(err)) = self.floor(f.run) {
                        out.push(format!("{}: keine Decke, {}", e.number, explain_floor(err)));
                    }
                    if !self.materials.contains(f.material) {
                        out.push(format!("{}: Baustoff fehlt", e.number));
                    }
                    // Regel 35: Dicke der Untersichtdämmung je Decke
                    if !(MIN_SOFFIT..=MAX_SOFFIT).contains(&f.soffit.thickness) {
                        out.push(format!(
                            "{}: Untersichtdämmung {} mm (erlaubt {MIN_SOFFIT}…{MAX_SOFFIT} mm)",
                            e.number, f.soffit.thickness
                        ));
                    }
                    if f.soffit
                        .material
                        .is_some_and(|m| !self.materials.contains(m))
                    {
                        out.push(format!(
                            "{}: Baustoff der Untersichtdämmung fehlt",
                            e.number
                        ));
                    }
                }
                ElementKind::SoffitInsulation { floor } => {
                    // Regel 35: genau dann, wenn die Decke auskragt
                    if !matches!(
                        self.element(floor).map(|f| &f.kind),
                        Some(ElementKind::Floor(_))
                    ) {
                        out.push(format!("{}: Decke fehlt", e.number));
                    } else if !soffits_wanted.contains(&floor) {
                        out.push(format!("{}: Decke kragt nicht aus", e.number));
                    }
                }
                ElementKind::EdgeStrip { wall, floor } => {
                    // Regel 22: Wand und Decke gibt es, das Paar braucht ihn
                    let wall_e = self.element(wall);
                    if !matches!(wall_e.map(|w| &w.kind), Some(ElementKind::Wall(_))) {
                        out.push(format!("{}: Wand fehlt", e.number));
                    } else if wall_e.is_some_and(|w| w.storey != e.storey) {
                        out.push(format!("{}: nicht im Geschoss seiner Wand", e.number));
                    }
                    if !matches!(
                        self.element(floor).map(|f| &f.kind),
                        Some(ElementKind::Floor(_))
                    ) {
                        out.push(format!("{}: Decke fehlt", e.number));
                    }
                    if !strips_wanted.contains(&(wall, floor)) {
                        out.push(format!(
                            "{}: Wand und Decke brauchen keinen Randdämmstreifen",
                            e.number
                        ));
                    }
                }
            }
        }
        // Regel 35: unter jeder auskragenden Decke genau eine Untersichtdämmung
        for floor in &soffits_wanted {
            let n = self
                .elements
                .iter()
                .filter(|(_, e)| e.kind == ElementKind::SoffitInsulation { floor: *floor })
                .count();
            if n != 1 {
                let f = self.element(*floor).map_or("?", |f| f.number.as_str());
                out.push(format!("{f}: {n} Untersichtdämmungen statt einer"));
            }
        }
        // Regel 22: zu jedem Paar genau ein Streifen
        for (wall, floor) in &strips_wanted {
            let n = self
                .elements
                .iter()
                .filter(|(_, e)| {
                    e.kind
                        == ElementKind::EdgeStrip {
                            wall: *wall,
                            floor: *floor,
                        }
                })
                .count();
            if n != 1 {
                let w = self.element(*wall).map_or("?", |w| w.number.as_str());
                out.push(format!("{w}: {n} Randdämmstreifen statt einem"));
            }
        }
        for (id, _) in self.runs.iter() {
            if self.needs_foundation(id) && self.slabs_of(id).len() != 1 {
                out.push(format!("Wandzug {id:?}: nicht genau eine Sohlplatte"));
            }
            if self.needs_floor(id) && self.floors_of(id).len() != 1 {
                out.push(format!("Wandzug {id:?}: nicht genau eine Decke"));
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
            if r.base != LevelRef::bottom(r.storey) || r.top != LevelRef::top(r.storey) {
                out.push(format!(
                    "Wandzug {id:?}: Fuß/Krone nicht an UK/OK des eigenen Geschosses"
                ));
            }
            if self
                .storey(r.storey)
                .is_some_and(|s| s.building.is_none_or(|b| !self.buildings.contains(b)))
            {
                out.push(format!("Wandzug {id:?}: Geschoss ohne Gebäude"));
            }
            let n = r.points.len();
            for k in 0..count {
                let (p, q) = (r.points[k], r.points[(k + 1) % n]);
                if (q - p).length() < 1.0 {
                    out.push(format!("Wandzug {id:?}: Segment {} ohne Länge", k + 1));
                }
            }
            if let Some(lower) = self.run_below(id).and_then(|b| self.run(b)) {
                // Regel 28: gleiche Segmentzahl, Segment k über Segment k
                if lower.segments.len() != r.segments.len() {
                    out.push(format!(
                        "Wandzug {id:?}: {} Segmente über {} im Geschoss darunter",
                        r.segments.len(),
                        lower.segments.len()
                    ));
                } else {
                    for (k, w) in r.segments.iter().enumerate() {
                        let Some(e) = self.element(*w) else { continue };
                        match e.kind {
                            ElementKind::Wall(Wall {
                                coupling: Some(c), ..
                            }) => {
                                if c.below != lower.segments[k] {
                                    out.push(format!(
                                        "{}: steht nicht über Segment {} darunter",
                                        e.number,
                                        k + 1
                                    ));
                                }
                            }
                            // Gelöst heißt `linked = false`, der Bezug bleibt
                            ElementKind::Wall(_) => {
                                out.push(format!("{}: ohne Stapelbezug", e.number));
                            }
                            _ => {}
                        }
                    }
                    // Regel 30: jedes Segment mindestens min(Wanddicke,
                    // Länge des Partners) lang
                    let (n, nl) = (r.points.len(), lower.points.len());
                    for (k, w) in r.segments.iter().enumerate() {
                        let Some(e) = self.element(*w) else { continue };
                        let len = (r.points[(k + 1) % n] - r.points[k]).length();
                        let partner = (lower.points[(k + 1) % nl] - lower.points[k]).length();
                        let dicke = e
                            .layer_set
                            .and_then(|t| self.layer_sets.get(t))
                            .map_or(0.0, |t| t.thickness());
                        let min = dicke.min(partner);
                        if len >= 1.0 && len + 0.5 < min {
                            out.push(format!(
                                "{}: {:.0} mm lang, kürzer als {:.0} mm",
                                e.number, len, min
                            ));
                        }
                    }
                }
                // Regel 30: der Umriss oben schneidet sich nicht selbst
                if r.closed && !sk_math::polygon::is_simple(&r.points) {
                    out.push(format!("Wandzug {id:?}: Umriss schneidet sich selbst"));
                }
            }
            for e in &r.segments {
                if self.segment_of(*e).map(|s| s.0) != Some(id) {
                    out.push(format!(
                        "Wandzug {id:?}: Wand {e:?} fehlt oder gehört woandershin"
                    ));
                }
            }
            // Regel 16: ein Zug hat genau einen Typ
            let mut sets = r
                .segments
                .iter()
                .filter_map(|e| self.element(*e))
                .map(|e| e.layer_set);
            if let Some(first) = sets.next() {
                if sets.any(|s| s != first) {
                    out.push(format!("Wandzug {id:?}: Wände mit verschiedenen Typen"));
                }
            }
        }
        out.extend(self.check_levels());
        out.extend(self.check_delete());
        let mut codes: Vec<&str> = Vec::new();
        for (_, set) in self.layer_sets.iter() {
            out.extend(set.problems());
            out.extend(self.bearing_problem(set));
            codes.push(&set.code);
        }
        // Regel 17: Kurzzeichen eindeutig
        codes.sort_unstable();
        for w in codes
            .windows(2)
            .filter(|w| w[0] == w[1] && !w[0].is_empty())
        {
            out.push(format!("Kurzzeichen {} doppelt vergeben", w[0]));
        }
        // Regel 18: die Standardtypen leben und haben ihre Art
        for cat in TypeCategory::ALL {
            match self.layer_set(self.default_type(cat)) {
                Some(t) if t.category == cat => {}
                Some(t) => out.push(format!(
                    "Standardtyp {}: {} ist kein {}typ",
                    cat.name(),
                    t.code,
                    cat.name()
                )),
                None => out.push(format!("Standardtyp {} fehlt", cat.name())),
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
        guids.extend(self.buildings.iter().map(|(_, b)| b.guid));
        guids.push(self.project.guid);
        let mut bn: Vec<&str> = self
            .buildings
            .iter()
            .map(|(_, b)| b.number.as_str())
            .collect();
        let n = bn.len();
        bn.sort();
        bn.dedup();
        if bn.len() != n {
            out.push("Gebäudenummer doppelt vergeben".into());
        }
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
            if self.run(j.a_run).map(|r| r.storey) != self.run(j.b_run).map(|r| r.storey) {
                out.push(format!("Anschluss {:?}: über zwei Geschosse", j.a));
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

/// Guids der Werkstypen „AW 31,5 Gasbeton + WDVS“ und „IW 17,5 Gasbeton“:
/// in jedem Projekt und Firmenkatalog derselbe Typ (K1, Regel 19).
pub const EXTERIOR_TYPE_GUID: Guid = Guid(0xbf19d5c9cf9241c9b5a191d5bdac9382);
pub const INTERIOR_TYPE_GUID: Guid = Guid(0xe3753d4ddf2d435299910b99a65cfba2);
/// Werkstypen aus K4 (Jörn 06.10. 16:38), in jedem Projekt gleich.
pub const ETICS_TYPE_GUID: Guid = Guid(0x72147f93f58782c40c7064abc28678f7);
pub const CAVITY_TYPE_GUID: Guid = Guid(0xab62c0bdc94bb6740b98633edb84415c);
pub const INTERIOR_115_TYPE_GUID: Guid = Guid(0x2f8dff421c184680660acedac925c20e);
pub const INTERIOR_240_TYPE_GUID: Guid = Guid(0x399dbdcd165ab68f996acfde24d981df);
/// Werkstyp aus K5: AW monolithisch 36,5 mit Randdämmstreifen.
pub const MONO_TYPE_GUID: Guid = Guid(0x9c03de8332f6e5a1d90474f347dbbf7e);
/// Laufende Zahl einer Gebäudenummer: „GB-02“ → 2.
pub fn building_index(number: &str) -> Option<u32> {
    number.strip_prefix("GB-")?.parse().ok()
}

/// Art eines Werkstyps nach seiner festen Guid.
pub(crate) fn werk_category(g: Guid) -> Option<TypeCategory> {
    match g {
        EXTERIOR_TYPE_GUID | ETICS_TYPE_GUID | CAVITY_TYPE_GUID | MONO_TYPE_GUID => {
            Some(TypeCategory::ExteriorWall)
        }
        INTERIOR_TYPE_GUID | INTERIOR_115_TYPE_GUID | INTERIOR_240_TYPE_GUID => {
            Some(TypeCategory::InteriorWall)
        }
        _ => None,
    }
}

/// Startwert der Guids der Startbibliothek in [`Model::new`]: Stifte,
/// Schraffuren, Oberflächen und Baustoffe sind in jedem neuen Projekt gleich.
const LIBRARY_SEED: u64 = 0x534b_4b41_5441_4c47;

/// Bauabschnitt der Wände (nach Frostschürze 1 und Sohlplatte 2).
pub const WALL_SEQ: u16 = 3;
const SLAB_SEQ: u16 = 2;
const FOOTING_SEQ: u16 = 1;
/// Bauabschnitt der Erdgeschossdecke (nach den Wänden).
const FLOOR_SEQ: u16 = 4;
/// Standardwerte der Geschossbänder (B11, mm): Geschosshöhe EG und OG,
/// Gründungstiefe, Deckendicke.
pub(crate) const STOREY_HEIGHT: f64 = 2855.0;
/// Geschosshöhe OG (Jörn 10:13): lichte Höhe 2,635 + Decke 0,22.
pub(crate) const UPPER_HEIGHT: f64 = 2855.0;
pub(crate) const FOUNDATION_DEPTH: f64 = 800.0;
pub const FLOOR_THICKNESS: f64 = 220.0;
/// Untersichtdämmung unter einem Vorsprung (Jörn 04:03, G7 K4, BIM Regel
/// 35): 12 cm, einstellbar von 4 bis 30 cm, nie 0.
pub const SOFFIT_THICKNESS: f64 = 120.0;
pub const MIN_SOFFIT: f64 = 40.0;
pub const MAX_SOFFIT: f64 = 300.0;
/// Sohlplatte neuer Gebäude (Jörn 10:13): 22 cm, die Frostschürze reicht
/// darunter bis UK Gründung −0,80 (58 cm).
pub const SLAB_THICKNESS: f64 = 220.0;
/// Grenzen (G4): lichte Höhe jedes Geschosses, Schürzentiefe, größte
/// Gründungstiefe.
pub const MIN_CLEAR: f64 = 1000.0;
pub const MIN_FOOTING: f64 = 100.0;
pub const MAX_FOUNDATION: f64 = 10000.0;
/// Kleinster Sockelrücksprung außer 0 (mm).
pub const MIN_RECESS: f64 = 20.0;

/// Breite der Frostschürze (mm): 30 bis 45 cm, unabhängig von der Wand
/// (Jörn 06.10. 17:34).
pub const FOOTING_WIDTH: (f64, f64) = (300.0, 450.0);
/// Teil des Körpers eines Wandzugs: Sohlplatte bzw. Frostschürze (statt Segment).
pub const SLAB_PART: u32 = u32::MAX - 1;
pub const FOOTING_PART: u32 = u32::MAX - 2;
/// Teil des Körpers eines Wandzugs: Erdgeschossdecke darüber.
pub const FLOOR_PART: u32 = u32::MAX - 3;
/// Teil des Körpers eines Wandzugs: Randdämmstreifen auf Segment `k` ist
/// `STRIP_PART + k` (K5).
pub const STRIP_PART: u32 = u32::MAX - 3 - MAX_STRIPS;
const MAX_STRIPS: u32 = 1 << 16;
/// Teil des Körpers eines Wandzugs: Untersichtdämmung unter seiner Decke (G7 K4).
pub const SOFFIT_PART: u32 = STRIP_PART - 1;

/// Warum keine Decke entsteht, als Satz.
/// Zwei Stände eines Baustoffs unterscheiden sich höchstens in den
/// Darstellungsverweisen (beide vorhanden).
fn bim_equal(a: Option<&Material>, b: Option<&Material>) -> bool {
    match (a, b) {
        (Some(a), Some(b)) => {
            let same = Material {
                cut_fill: a.cut_fill,
                cut_fg: a.cut_fg,
                cut_bg: a.cut_bg,
                surface: a.surface,
                ..b.clone()
            };
            same == *a
        }
        _ => false,
    }
}

pub(crate) fn explain_floor(e: FloorError) -> &'static str {
    match e {
        FloorError::NotClosed => "der Wandzug ist nicht geschlossen",
        FloorError::NoCore => "der Wandaufbau hat keine tragende Schicht",
        FloorError::NotSimple => "der Umriss überschneidet sich",
        FloorError::BadThickness => "die Deckendicke muss größer als 0 sein",
        FloorError::BelowWallFoot => "die Unterkante liegt auf oder unter dem Wandfuß",
        FloorError::AboveWallTop => "die Oberkante liegt über der Wandkrone",
    }
}

/// Umschließendes Rechteck von Punkten (Grundriss).
fn bounds2(pts: &[Vec3]) -> (Vec3, Vec3) {
    pts.iter().fold(
        (vec3(f64::MAX, f64::MAX, 0.0), vec3(f64::MIN, f64::MIN, 0.0)),
        |(lo, hi), p| {
            (
                vec3(lo.x.min(p.x), lo.y.min(p.y), 0.0),
                vec3(hi.x.max(p.x), hi.y.max(p.y), 0.0),
            )
        },
    )
}

/// Überschneiden sich zwei Rechtecke (Grundriss)?
fn overlap((alo, ahi): (Vec3, Vec3), (blo, bhi): (Vec3, Vec3)) -> bool {
    alo.x <= bhi.x && blo.x <= ahi.x && alo.y <= bhi.y && blo.y <= ahi.y
}

/// Segmentmitten eines Zuges.
fn mids(pts: &[Vec3], closed: bool) -> Vec<Vec3> {
    segment_lines(pts, closed)
        .into_iter()
        .map(|(a, b)| (a + b) * 0.5)
        .collect()
}

/// Liegt `p` im Vieleck `poly` (Grundriss, Strahltest)?
fn contains(poly: &[Vec3], p: Vec3) -> bool {
    let n = poly.len();
    let mut inside = false;
    for i in 0..n {
        let (a, b) = (poly[i], poly[(i + 1) % n]);
        if (a.y > p.y) != (b.y > p.y) && p.x < a.x + (p.y - a.y) / (b.y - a.y) * (b.x - a.x) {
            inside = !inside;
        }
    }
    inside
}

/// Warum keine Gründung entsteht, als Satz.
pub(crate) fn explain(e: FoundationError) -> &'static str {
    match e {
        FoundationError::NotClosed => "der Wandzug ist nicht geschlossen",
        FoundationError::NotSimple => "der Umriss überschneidet sich",
        FoundationError::RecessTooLarge => "der Rücksprung ist nicht kleiner als die Wanddicke",
        FoundationError::RecessTooSmall => "der Rücksprung liegt zwischen 0 und 20 mm",
        FoundationError::BadSize => "ein Maß ist nicht größer als 0",
    }
}

/// Zug ohne Anschlüsse, zum Erkennen der Anschlüsse.
struct Base {
    run: RunId,
    /// Freie Enden und Segmente: Punkt und Richtung, einmal berechnet (U3).
    ends: [Option<(Vec3, Vec3)>; 2],
    frames: Vec<(Vec3, Vec3)>,
    chain: WallChain,
    segments: Vec<ElementId>,
    foot: Vec<[Vec3; 4]>,
    /// Umschließendes Rechteck des Grundrisses.
    lo: Vec3,
    hi: Vec3,
}

/// Grundrisse der Züge, je Zug einmal je Erkennungslauf berechnet (U3): Ein
/// großer Außenzug ist Kandidat für fast jedes Innenwandende und kostete sonst
/// je Ende einen ganzen Grundriss. Die Züge ändern sich während eines Laufs
/// nicht.
#[derive(Default)]
struct Bases(Vec<Option<Base>>);

impl Bases {
    fn fill(&mut self, m: &Model, id: RunId) {
        let slot = id.index() as usize;
        if self.0.len() <= slot {
            self.0.resize_with(slot + 1, || None);
        }
        if self.0[slot].as_ref().is_none_or(|b| b.run != id) {
            self.0[slot] = m.base(id);
        }
    }

    fn get(&self, id: RunId) -> Option<&Base> {
        self.0
            .get(id.index() as usize)?
            .as_ref()
            .filter(|b| b.run == id)
    }
}

/// Liegt `p` im Rechteck `lo`..`hi` (Grundriss)?
fn inside(p: Vec3, lo: Vec3, hi: Vec3) -> bool {
    p.x >= lo.x && p.y >= lo.y && p.x <= hi.x && p.y <= hi.y
}

/// Feste Reihenfolge: nach anschließendem Zug und Ende.
fn sort_joins(j: &mut [Join]) {
    j.sort_by_key(|j| (j.a_run.index(), j.a_end));
}

/// Gleicher Inhalt zweier Typen; der Änderungsstand zählt nicht mit.
pub(crate) fn same_type(a: &LayerSet, b: &LayerSet) -> bool {
    a.guid == b.guid
        && a.name == b.name
        && a.code == b.code
        && a.category == b.category
        && a.layers == b.layers
        && a.props == b.props
        && a.note == b.note
        && a.bearing == b.bearing
}

/// `code`, wenn `taken` es nicht kennt, sonst „code-2“, „code-3“ …
pub(crate) fn free_code(code: &str, taken: impl Fn(&str) -> bool) -> String {
    if !taken(code) {
        return code.to_string();
    }
    (2..)
        .map(|n| format!("{code}-{n}"))
        .find(|c| !taken(c))
        .unwrap_or_default()
}

/// Aufbau „IW 17,5 Gasbeton“: eine tragende Schicht aus `material`.
pub(crate) fn interior_set(guid: Guid, material: MaterialId) -> LayerSet {
    LayerSet {
        guid,
        name: "IW 17,5 Gasbeton".into(),
        code: "IW-17,5".into(),
        category: TypeCategory::InteriorWall,
        props: PropSet::new(),
        note: String::new(),
        changed: 1,
        bearing: Bearing::Core,
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

/// Kleinster Versatz einer gestapelten Wand; darunter rastet er auf 0 ein
/// (Regel 31, wie der Sockelrücksprung). Erst ab ihm zählt ein Segment als
/// Vor- oder Rücksprung: Deckenstreifen, Untersichtdämmung und Abfangung
/// entstehen nicht für Reste darunter, etwa aus alten Dateien (Review 1t T1).
pub const MIN_OFFSET: f64 = 20.0;

/// Bleibt innerhalb der Wand eines geschlossenen Zuges ein Raum (Innenfläche
/// mit dem robusten Versatz)? Sonst entstünde keine Decke (G7 K2).
fn room_inside(c: &WallChain) -> bool {
    !c.closed || sk_math::polygon::inset(&c.face_corners(c.outer_offset()), c.thickness()).is_ok()
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
        let eg = m.eg_at(3500.0);
        m.add_wall_run(&pts, true, RefSide::Left, eg, set, Category::ExteriorWall)
            .unwrap()
    }

    /// Gebäude mit `n` Geschossen aus dem Rechteck 10 × 8 m (B12).
    fn gebaeude(n: u8) -> (Model, Vec<RunId>) {
        let mut m = Model::with_seed(12);
        let b = m.add_building(n);
        let pts = [
            vec3(0.0, 0.0, 0.0),
            vec3(0.0, 8000.0, 0.0),
            vec3(10000.0, 8000.0, 0.0),
            vec3(10000.0, 0.0, 0.0),
        ];
        let eg = m.build_from_polygon(b, &pts).unwrap();
        let mut runs = vec![eg];
        while let Some(up) = runs.last().and_then(|r| m.runs_above(*r).first().copied()) {
            runs.push(up);
        }
        (m, runs)
    }

    fn nummern(m: &Model, prefix: &str) -> Vec<String> {
        let mut v: Vec<String> = m
            .elements()
            .iter()
            .map(|(_, e)| e.number.clone())
            .filter(|n| n.starts_with(prefix))
            .collect();
        v.sort();
        v
    }

    /// U3: Beim Verschieben der Außenwand kommen nur die Innenwände mit, die
    /// unter die Decke kommen oder sie verlassen; die anderen behalten ihr
    /// Deckenband.
    #[test]
    fn verschieben_nimmt_nur_innenwaende_mit_neuem_deckenband() {
        let (mut m, runs) = gebaeude(2);
        let eg = runs[0];
        let storey = m.run(eg).unwrap().storey;
        let set = m.defaults().interior_wall;
        let innen = |m: &mut Model, x: f64| {
            let pts = [vec3(x, 3000.0, 0.0), vec3(x, 5000.0, 0.0)];
            m.add_wall_run(
                &pts,
                false,
                RefSide::Center,
                storey,
                set,
                Category::InteriorWall,
            )
            .unwrap()
        };
        let nah = innen(&mut m, 1000.0);
        let fern = innen(&mut m, 6000.0);
        assert_eq!(m.runs_under_floor(eg), [nah, fern]);
        m.begin("Wand verschieben");
        let pts = [
            vec3(2000.0, 0.0, 0.0),
            vec3(2000.0, 8000.0, 0.0),
            vec3(10000.0, 8000.0, 0.0),
            vec3(10000.0, 0.0, 0.0),
        ];
        let out = m.set_run_points(eg, &pts).unwrap();
        assert!(out.contains(&nah), "verlässt die Decke: {out:?}");
        assert!(!out.contains(&fern), "bleibt drunter: {out:?}");
        assert_eq!(m.runs_under_floor(eg), [fern]);
        // Zurück: die nahe kommt wieder unter die Decke
        let back = m.chain(eg).unwrap().points;
        let mut alt = back.clone();
        for p in alt.iter_mut().take(2) {
            p.x = 0.0;
        }
        let out = m.set_run_points(eg, &alt).unwrap();
        assert!(out.contains(&nah) && !out.contains(&fern), "{out:?}");
        m.commit();
        assert!(m.check().is_empty(), "{:?}", m.check());
    }

    /// BIM Regeln 28 und 30 als eigene Befunde der Prüfung.
    #[test]
    fn pruefung_stapel_regeln_28_und_30() {
        let (mut m, runs) = gebaeude(2);
        let (eg, og) = (runs[0], runs[1]);
        assert!(m.check().is_empty(), "{:?}", m.check());
        // Regel 28: Segment 1 oben zeigt auf Segment 2 darunter
        let w = m.wall_at(og, 0).unwrap();
        let falsch = m.wall_at(eg, 1).unwrap();
        if let Some(ElementKind::Wall(x)) = m.elements.get_mut(w).map(|e| &mut e.kind) {
            x.coupling = x.coupling.map(|c| Coupling { below: falsch, ..c });
        }
        assert!(
            m.check()
                .iter()
                .any(|t| t.contains("steht nicht über Segment 1")),
            "{:?}",
            m.check()
        );
        // Regel 30: Umriss oben in Achterform
        let (mut m, runs) = gebaeude(2);
        let og = runs[1];
        let acht = [
            vec3(0.0, 0.0, 0.0),
            vec3(10000.0, 8000.0, 0.0),
            vec3(10000.0, 0.0, 0.0),
            vec3(0.0, 8000.0, 0.0),
        ];
        m.runs.get_mut(og).unwrap().points = acht.to_vec();
        assert!(
            m.check()
                .iter()
                .any(|t| t.contains("Umriss schneidet sich selbst")),
            "{:?}",
            m.check()
        );
        // Regel 28: ein Segment ganz ohne Bezug
        let (mut m, runs) = gebaeude(2);
        let w = m.wall_at(runs[1], 2).unwrap();
        if let Some(ElementKind::Wall(x)) = m.elements.get_mut(w).map(|e| &mut e.kind) {
            x.coupling = None;
        }
        assert!(
            m.check().iter().any(|t| t.contains("ohne Stapelbezug")),
            "{:?}",
            m.check()
        );
        // Regel 30: Segment kürzer als min(Wanddicke, Partnerlänge)
        let (mut m, runs) = gebaeude(2);
        let og = runs[1];
        let p = m.run(og).unwrap().points.clone();
        // Ecke 1 so weit vorziehen, dass Segment 1 nur 100 mm lang ist
        let d = (p[1] - p[0]).normalized();
        m.runs.get_mut(og).unwrap().points[1] = p[0] + d * 100.0;
        assert!(
            m.check().iter().any(|t| t.contains("100 mm lang")),
            "{:?}",
            m.check()
        );
    }

    #[test]
    fn gebaeude_mit_einem_geschoss() {
        let (m, runs) = gebaeude(1);
        assert_eq!(runs.len(), 1);
        let b = m.buildings().ids().next().unwrap();
        let shorts: Vec<String> = m
            .levels_in(Some(b))
            .iter()
            .map(|id| m.storey(*id).unwrap().short.clone())
            .collect();
        assert_eq!(shorts, ["GR", "EG"]);
        assert_eq!(nummern(&m, "AW-").len(), 4);
        assert_eq!(nummern(&m, "DE-"), ["DE-001"]);
        assert!(m.check().is_empty(), "{:?}", m.check());
    }

    #[test]
    fn gebaeude_mit_drei_geschossen() {
        let (m, runs) = gebaeude(3);
        assert_eq!(runs.len(), 3);
        let b = m.buildings().ids().next().unwrap();
        let bands: Vec<(String, f64, f64)> = m
            .levels_in(Some(b))
            .iter()
            .map(|id| {
                let st = m.storey(*id).unwrap();
                (st.short.clone(), st.elevation, st.top())
            })
            .collect();
        assert_eq!(
            bands,
            [
                ("GR".to_string(), -800.0, 0.0),
                ("EG".to_string(), 0.0, 2855.0),
                ("1. OG".to_string(), 2855.0, 5710.0),
                ("2. OG".to_string(), 5710.0, 8565.0),
            ]
        );
        assert_eq!(nummern(&m, "AW-").len(), 12);
        assert_eq!(nummern(&m, "DE-"), ["DE-001", "DE-002", "DE-003"]);
        let top = m.floor(runs[2]).unwrap().unwrap();
        let q = crate::qto::floor_qto_of(&top);
        assert!((q.volume / 1e9 - 16.508448).abs() < 1e-6, "{}", q.volume);
        let de = m.floors_of(runs[2]);
        assert_eq!(m.element(de[0]).unwrap().number, "DE-003");
        // Wände der 2. OG an die des 1. OG gekoppelt
        let w = m.wall_at(runs[2], 0).unwrap();
        let below = match m.element(w).map(|e| &e.kind) {
            Some(ElementKind::Wall(x)) => x.coupling.unwrap().below,
            _ => unreachable!(),
        };
        assert_eq!(Some(below), m.wall_at(runs[1], 0));
        assert!(m.check().is_empty(), "{:?}", m.check());
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
        let eg = m2.eg_at(3000.0);
        let r2 = m2
            .add_wall_run(&pts, false, RefSide::Left, eg, set, Category::ExteriorWall)
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
        // AW-005…008 sind die gestapelten OG-Wände (B12)
        assert_eq!(m.element(t[1]).unwrap().number, "AW-009");
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
        let eg = m.eg_at(3000.0);
        let r2 = m
            .add_wall_run(&o, false, RefSide::Left, eg, set, Category::ExteriorWall)
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
        // EG-Zug und gekoppelter OG-Zug
        assert_eq!(touched.runs.len(), 2);
        assert!(touched.runs.contains(&r));
        assert!(m.runs().is_empty() && m.elements().is_empty());
        assert_eq!(state(&m), empty);
        assert!(m.revision() > rev);
        m.begin("Wand");
        let r2 = rechteck(&mut m);
        m.commit();
        assert_ne!(r2, r);
        assert_ne!(m.run(r2).unwrap().guid, guid);
        let first = m.run(r2).unwrap().segments[0];
        assert_eq!(m.element(first).unwrap().number, "AW-009");
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
        // Zug, Wände, Sohlplatte, Frostschürze und Erdgeschossdecke, dazu
        // der gekoppelte OG-Zug mit Wänden und OG-Decke
        assert_eq!(t.changes.len(), 2 * (1 + walls.len()) + 4);
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
        // EG-Zug und der mitgeführte OG-Zug
        assert_eq!(t.changes.len(), 2, "{:?}", t.changes);
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
        assert_eq!(touched.runs.len(), 2, "EG- und OG-Zug");
        assert!(touched.runs.contains(&r));
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

    /// E5/BIM: Stift löschen nur unbenutzt, als Schritt mit Rückgängig;
    /// neue Nummer = höchste + 1; Verwender aus Darstellung und Baustoffen.
    #[test]
    fn stift_loeschen_nur_unbenutzt() {
        let mut m = Model::with_seed(7);
        let pen = |m: &Model, n: u16| m.attr().pens().iter().find(|p| p.1.number == n).unwrap().0;
        let strong = pen(&m, 3);
        let users = m.attr_users(AttrRef::Pen(strong));
        let labels: Vec<String> = users.iter().map(|u| u.label()).collect();
        assert_eq!(labels, ["Schnittkante in Zeichnungen", "Gelände"]);
        let hatch = pen(&m, 4);
        assert!(m.attr_users(AttrRef::Pen(hatch)).iter().all(|u| matches!(
            u,
            AttrUser::Material {
                role: "Schraffur",
                ..
            }
        )));
        assert_eq!(m.next_pen_number(), 10);
        m.begin("Stift");
        assert!(!m.remove_pen(strong), "verwendet");
        let p = Pen {
            guid: m.new_guid(),
            number: m.next_pen_number(),
            name: "Stift 10".into(),
            color: [0, 0, 0],
            width_mm: 0.25,
        };
        let id = m.add_pen(p);
        let t = m.commit().unwrap();
        assert_eq!(m.attr().pens().len(), 10);
        assert!(m.check().is_empty(), "{:?}", m.check());
        m.begin("Löschen");
        assert!(m.remove_pen(id));
        let del = m.commit().unwrap();
        assert!(m.attr().pen(id).is_none());
        assert_eq!(m.next_pen_number(), 10);
        m.apply(&del, Direction::Undo);
        assert_eq!(m.attr().pen(id).unwrap().number, 10);
        m.apply(&t, Direction::Undo);
        assert!(m.attr().pen(id).is_none());
        assert!(m.check().is_empty());
        // Doppelte Nummer verletzt die Prüfregel
        m.begin("Doppelt");
        let mut dup = m.attr().pen(strong).unwrap().clone();
        dup.guid = m.new_guid();
        m.add_pen(dup);
        m.commit();
        assert!(m.check().iter().any(|e| e.contains("Stiftnummer")));
    }
}

/// OG Phase 2 im Kern (G7 K1–K3): gelöste und versetzte OG-Segmente.
#[cfg(test)]
mod og_phase2 {
    use super::*;
    use crate::qto::{run_qto, soffit_qto, WallQto};
    use crate::solid::Solid;

    fn gebaeude() -> (Model, RunId, RunId) {
        let mut m = Model::with_seed(71);
        let b = m.add_building(2);
        let pts = [
            vec3(0.0, 0.0, 0.0),
            vec3(0.0, 8000.0, 0.0),
            vec3(10000.0, 8000.0, 0.0),
            vec3(10000.0, 0.0, 0.0),
        ];
        let eg = m.build_from_polygon(b, &pts).unwrap();
        let og = m.runs_above(eg)[0];
        (m, eg, og)
    }

    /// Kopplung setzen; `None` löst die Kette (der Stapelbezug bleibt).
    fn set_coupling(m: &mut Model, w: ElementId, c: Option<Coupling>) {
        if let Some(ElementKind::Wall(x)) = m.elements.get_mut(w).map(|e| &mut e.kind) {
            match (c, x.coupling.as_mut()) {
                (Some(c), _) => x.coupling = Some(c),
                (None, Some(now)) => now.linked = false,
                (None, None) => {}
            }
        }
    }

    /// Zieht Segment `seg` um `out` nach außen (negativ: nach innen).
    fn drag(m: &mut Model, run: RunId, seg: usize, out: f64) -> bool {
        let c = m.chain(run).unwrap();
        let c = c.with_segment_moved(seg, c.outward_sign() * out).unwrap();
        m.set_run_points(run, &c.points).is_some()
    }

    fn ys(m: &Model, run: RunId) -> (f64, f64) {
        let p = &m.run(run).unwrap().points;
        (p[1].y, p[2].y)
    }

    #[test]
    fn geloestes_segment_bleibt_stehen() {
        let (mut m, eg, og) = gebaeude();
        // OG-Wand y = 8 lösen und 0,30 m nach außen ziehen (Normale zeigt nach −y)
        let w = m.wall_at(og, 1).unwrap();
        set_coupling(&mut m, w, None);
        assert!(drag(&mut m, og, 1, 300.0));
        assert_eq!(ys(&m, og), (8300.0, 8300.0));
        // EG-Wand y = 8 um 1 m nach außen: das gelöste OG-Segment bleibt stehen
        assert!(drag(&mut m, eg, 1, 1000.0));
        assert_eq!(ys(&m, eg), (9000.0, 9000.0));
        assert_eq!(ys(&m, og), (8300.0, 8300.0));
        // EG-Westwand 0,5 m nach außen: das gekoppelte OG-Segment folgt
        assert!(drag(&mut m, eg, 0, 500.0));
        let (ex, ox) = (
            m.run(eg).unwrap().points[0].x,
            m.run(og).unwrap().points[0].x,
        );
        assert!(
            (ex + 500.0).abs() < 1e-6 && (ox + 500.0).abs() < 1e-6,
            "{ex} {ox}"
        );
        assert_eq!(ys(&m, og), (8300.0, 8300.0));
        assert!(m.check().is_empty(), "{:?}", m.check());
        // Bleibt gelöst, die übrigen bleiben gekoppelt
        let coupling = |k: usize| match m.element(m.wall_at(og, k).unwrap()).map(|e| &e.kind) {
            Some(ElementKind::Wall(x)) => x.coupling,
            _ => None,
        };
        assert!(coupling(1).is_some_and(|c| !c.linked));
        assert!((0..4)
            .filter(|k| *k != 1)
            .all(|k| coupling(k).is_some_and(|c| c.linked)));
    }

    #[test]
    fn ungueltiger_stapel_klemmt_das_gummiband() {
        let (mut m, eg, og) = gebaeude();
        // OG-Wand y = 8 gekoppelt mit −7,0 m (steht bei y = 1 m, OG 1 m tief)
        let w = m.wall_at(og, 1).unwrap();
        let below = m.wall_at(eg, 1).unwrap();
        set_coupling(
            &mut m,
            w,
            Some(Coupling {
                below,
                offset: -7000.0,
                linked: true,
            }),
        );
        m.carry_stack(eg);
        assert!(m.check().is_empty(), "{:?}", m.check());
        assert_eq!(ys(&m, og), (1000.0, 1000.0));
        // EG-Südwand 0,6 m nach innen: im OG bliebe zwischen den Wänden
        // (2 × 31,5 cm) kein Raum, die Decke fiele weg → abgelehnt
        let before = m.run(eg).unwrap().points.clone();
        assert!(!drag(&mut m, eg, 3, -600.0));
        assert_eq!(m.run(eg).unwrap().points, before);
        assert_eq!(ys(&m, og), (1000.0, 1000.0));
        // 0,3 m nach innen geht noch (OG 0,7 m tief)
        assert!(drag(&mut m, eg, 3, -300.0));
        assert!(m.check().is_empty(), "{:?}", m.check());
    }

    #[test]
    fn streifen_offen_wo_das_og_versetzt_steht() {
        let (mut m, eg, og) = gebaeude();
        let w = m.wall_at(og, 1).unwrap();
        let below = m.wall_at(eg, 1).unwrap();
        let stelle = |m: &mut Model, offset: f64| {
            set_coupling(
                m,
                w,
                Some(Coupling {
                    below,
                    offset,
                    linked: true,
                }),
            );
            m.carry_stack(eg);
        };
        // Rücksprung: der Fuß steht neben dem Streifen (K3)
        stelle(&mut m, -300.0);
        assert_eq!(m.covered_segments(eg), vec![true, false, true, true]);
        assert_eq!(m.open_segments(og), vec![false, true, false, false]);
        // gelöst bleibt die Lage und damit der offene Streifen
        set_coupling(&mut m, w, None);
        assert_eq!(m.covered_segments(eg), vec![true, false, true, true]);
        assert_eq!(m.open_segments(og), vec![false, true, false, false]);
        // Vorsprung: der Streifen wandert mit der Deckenstirn nach außen und
        // liegt wieder unter der Wand (K4)
        stelle(&mut m, 300.0);
        assert_eq!(m.covered_segments(eg), vec![true; 4]);
        assert_eq!(m.open_segments(og), vec![false; 4]);
    }

    /// Gebäude mit der Nordwand im OG um `out` nach außen (gekoppelt).
    fn mit_versatz(out: f64) -> (Model, RunId, RunId) {
        let (mut m, eg, og) = gebaeude();
        let w = m.wall_at(og, 1).unwrap();
        let below = m.wall_at(eg, 1).unwrap();
        set_coupling(
            &mut m,
            w,
            Some(Coupling {
                below,
                offset: out,
                linked: true,
            }),
        );
        m.carry_stack(eg);
        m.sync_soffits();
        assert!(m.check().is_empty(), "{:?}", m.check());
        (m, eg, og)
    }

    fn near(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-3
    }

    #[test]
    fn eg_faengt_geloestes_og_unter_2_cm() {
        // Review 1t T1: OG-Nordwand gelöst bei +300, EG-Nordfuß 290 nach außen
        let (mut m, eg, og) = gebaeude();
        let w = m.wall_at(og, 1).unwrap();
        set_coupling(&mut m, w, None);
        assert!(drag(&mut m, og, 1, 300.0));
        assert!(drag(&mut m, eg, 1, 290.0));
        // Gemessen wären 10 mm: das Segment rastet bündig ein und bleibt gelöst
        assert_eq!(m.stack_offset(w), Some((0.0, false)));
        assert_eq!(ys(&m, og), ys(&m, eg));
        assert!(m.chain(eg).unwrap().joints.overhang.is_none());
        assert!(m.soffit_floors().is_empty());
        assert!(m.check().is_empty(), "{:?}", m.check());
        // Ab 20 mm bleibt der Versatz stehen
        let (mut m, eg, og) = gebaeude();
        let w = m.wall_at(og, 1).unwrap();
        set_coupling(&mut m, w, None);
        assert!(drag(&mut m, og, 1, 300.0));
        assert!(drag(&mut m, eg, 1, 280.0));
        assert!(m.stack_offset(w).is_some_and(|(d, l)| near(d, 20.0) && !l));
        assert!(m.chain(eg).unwrap().joints.overhang.is_some());
    }

    #[test]
    fn rest_unter_2_cm_ist_kein_vorsprung() {
        // Alte Datei: 10 mm Versatz gespeichert. Decke, UD und Außenschichten
        // bleiben wie bündig.
        let (m, eg, _) = mit_versatz(10.0);
        assert!(m.chain(eg).unwrap().joints.overhang.is_none());
        assert!(m.soffit_floors().is_empty());
        let f = m.floor(eg).unwrap().unwrap();
        assert!(near(f.area(), 9720.0 * 7720.0), "{}", f.area());
        // Zurückgesetzt um 10 mm: der Randstreifen gilt als gedeckt
        let (m, eg, og) = mit_versatz(-10.0);
        assert!(m.covered_segments(eg).iter().all(|c| *c));
        assert!(m.open_segments(og).iter().all(|o| !*o));
    }

    #[test]
    fn vorsprung_verlaengert_decke_untersicht_und_daemmung() {
        let (m, eg, _) = mit_versatz(300.0);
        // Decke bis Außenseite Kern OG: y = 8,30 − 0,14
        let f = m.floor(eg).unwrap().unwrap();
        let ymax = f.outline.iter().map(|p| p.y).fold(f64::MIN, f64::max);
        assert!(near(ymax, 8160.0), "{ymax}");
        assert!(near(f.area(), 9720.0 * 8020.0), "{}", f.area());
        // Untersichtdämmung 12 cm unter dem auskragenden Streifen, zwischen
        // den Kernen von EG und OG
        let sp = f.soffit.unwrap();
        assert!(near(sp.thickness, 120.0));
        assert_eq!(f.soffit_band(), Some((2515.0, 2635.0)));
        assert!(near(f.soffit_area(), 9720.0 * 300.0), "{}", f.soffit_area());
        // als eigenes Bauteil UD unter DE-001 (BIM Regel 35)
        let fl = m.floors_of(eg)[0];
        let ud = m.soffit_of(fl).unwrap();
        assert_eq!(m.element(ud).unwrap().number, "UD-001");
        let q = soffit_qto(&m, ud).unwrap();
        assert!(near(q.area, 9720.0 * 300.0));
        assert!(near(q.volume, 9720.0 * 300.0 * 120.0));
        assert!(m.can_delete(ud).is_err());
        // Dämmung der EG-Wände ab UK Untersichtdämmung in der Lage des OG
        let c = m.chain(eg).unwrap();
        let o = c.joints.overhang.clone().unwrap();
        assert_eq!(o.offsets, vec![0.0, 300.0, 0.0, 0.0]);
        assert_eq!((o.from, o.to), (2515.0, 2855.0));
        assert_eq!(
            c.layer_parts(0),
            vec![(0.0, 2515.0, false), (2515.0, 2855.0, true)]
        );
        // Kern: Tasche wie bisher
        assert_eq!(c.layer_parts(1), vec![(0.0, 2635.0, false)]);
        // Mengen (BIM): die herabgezogenen Abschnitte zählen zur OG-Wand.
        // EG-Nordwand: Dämmung nur bis UK Untersicht; EG-Westwand bleibt ganz
        let (w, o) = (run_qto(&m, eg), run_qto(&m, og_of(&m, eg)));
        let nord = (10000.0 + 9720.0) * 0.5 * 140.0;
        let west = (8000.0 + 7720.0) * 0.5 * 140.0;
        assert!(near(w[1].layers[0].volume, nord * 2515.0));
        assert!(near(w[0].layers[0].volume, west * 2855.0));
        assert!(near(w[0].layers[0].side_area, 8000.0 * 2855.0));
        assert!(near(w[1].layers[1].pocket, w[1].layers[1].area * 220.0));
        // OG-Nordwand: dazu der ganze Abschnitt (gleich groß, 0,34 m hoch);
        // OG-Westwand: das Eckstück über dem Vorsprung (0,30 × 0,14 × 0,34)
        let og_west = (8300.0 + 8020.0) * 0.5 * 140.0;
        assert!(near(o[1].layers[0].volume, nord * 2855.0 + nord * 340.0));
        assert!(near(
            o[0].layers[0].volume,
            og_west * 2855.0 + 300.0 * 140.0 * 340.0
        ));
        assert!(near(
            o[0].layers[0].side_area,
            8300.0 * 2855.0 + 300.0 * 340.0
        ));
        let sum = |q: &[WallQto]| q.iter().map(|x| x.layers[0].volume).sum::<f64>();
        assert!(near(
            sum(&w),
            nord * 2515.0 + nord * 2855.0 + 2.0 * west * 2855.0
        ));
        // Ohne Verblender keine Abfangung
        assert!(o.iter().all(|x| x.facing_support == 0.0));
    }

    fn og_of(m: &Model, eg: RunId) -> RunId {
        m.runs_above(eg)[0]
    }

    #[test]
    fn verblender_wird_abgefangen() {
        let (mut m, eg, og) = gebaeude();
        // AW-49 (Verblender 11,5, Luft 6, Dämmung 14, Kern 17,5) auf alle Wände
        let t = m
            .layer_sets
            .iter()
            .find(|(_, x)| x.guid == CAVITY_TYPE_GUID)
            .map(|(id, _)| id)
            .unwrap();
        for r in [eg, og] {
            for w in m.run(r).unwrap().segments.clone() {
                m.elements.get_mut(w).unwrap().layer_set = Some(t);
            }
        }
        let w = m.wall_at(og, 1).unwrap();
        let below = m.wall_at(eg, 1).unwrap();
        set_coupling(
            &mut m,
            w,
            Some(Coupling {
                below,
                offset: 300.0,
                linked: true,
            }),
        );
        m.carry_stack(eg);
        m.sync_soffits();
        assert!(m.check().is_empty(), "{:?}", m.check());
        let o = run_qto(&m, og);
        // Nordwand: Achse des Verblenders über die ganze Länge (10,00 − 0,115);
        // Seitenwände: je das Eckstück 0,30 m
        assert!(
            near(o[1].facing_support, 10000.0 - 115.0),
            "{}",
            o[1].facing_support
        );
        assert!(near(o[0].facing_support, 300.0), "{}", o[0].facing_support);
        assert!(near(o[2].facing_support, 300.0));
        assert!(near(o[3].facing_support, 0.0));
        // Untersicht aus der Dämmschicht der OG-Wand (Kerndämmung)
        let f = m.floor(eg).unwrap().unwrap();
        let ins = m.layer_set(t).unwrap().layers[2].material;
        assert_eq!(f.soffit.unwrap().mat, material_key(ins));
    }

    #[test]
    fn putz_und_duenne_schalen_werden_nicht_abgefangen() {
        // Vorsatzschale aus Putz (z. B. WDVS mit Oberputz) oder unter 7 cm:
        // keine Abfangung; Mauerwerk ab 7 cm schon (BIM, Review 1t T5)
        let plaster = |m: &Model| {
            m.materials
                .iter()
                .find(|(_, x)| x.category == MatCategory::Plaster)
                .map(|(id, _)| id)
                .unwrap()
        };
        for (putz, dicke, soll) in [(true, 115.0, 0.0), (false, 60.0, 0.0), (false, 70.0, 300.0)] {
            let (mut m, eg, og) = gebaeude();
            let t = m
                .layer_sets
                .iter()
                .find(|(_, x)| x.guid == CAVITY_TYPE_GUID)
                .map(|(id, _)| id)
                .unwrap();
            let p = plaster(&m);
            let l = &mut m.layer_sets.get_mut(t).unwrap().layers[0];
            if putz {
                l.material = p;
            }
            l.thickness = dicke;
            for r in [eg, og] {
                for w in m.run(r).unwrap().segments.clone() {
                    m.elements.get_mut(w).unwrap().layer_set = Some(t);
                }
            }
            let w = m.wall_at(og, 1).unwrap();
            let below = m.wall_at(eg, 1).unwrap();
            set_coupling(
                &mut m,
                w,
                Some(Coupling {
                    below,
                    offset: 300.0,
                    linked: true,
                }),
            );
            m.carry_stack(eg);
            m.sync_soffits();
            let o = run_qto(&m, og);
            assert!(
                near(o[0].facing_support, soll),
                "{putz} {dicke}: {}",
                o[0].facing_support
            );
        }
    }

    /// Kanten auf der Höhe `z`, die auf der Geraden x = `x` liegen: Länge.
    fn kante_bei_x(s: &Solid, z: f64, x: f64) -> f64 {
        s.edges
            .iter()
            .filter(|e| near(e.a.z, z) && near(e.b.z, z) && near(e.a.x, x) && near(e.b.x, x))
            .map(|e| (e.b - e.a).length())
            .sum()
    }

    #[test]
    fn vorsprung_ohne_naht_auf_den_buendigen_seiten() {
        let (m, eg, _) = mit_versatz(300.0);
        let c = m.chain(eg).unwrap();
        let s = c.solid();
        // Westwand außen: unter dem Vorsprung nur das herabgezogene Stück
        // (y 8,00 … 8,30), sonst läuft die Dämmung ohne Naht durch
        assert!(
            near(kante_bei_x(&s, 2515.0, 0.0), 300.0),
            "{}",
            kante_bei_x(&s, 2515.0, 0.0)
        );
        // Nordseite: Unterkante der herabgezogenen Dämmung bei y = 8,30 und
        // EG-Dämmung unter der Untersicht bei y = 8,00
        let north = |y: f64| -> f64 {
            s.edges
                .iter()
                .filter(|e| near(e.a.z, 2515.0) && near(e.b.z, 2515.0))
                .filter(|e| near(e.a.y, y) && near(e.b.y, y))
                .map(|e| (e.b - e.a).length())
                .sum()
        };
        assert!(north(8300.0) >= 10000.0 - 1e-3, "{}", north(8300.0));
        assert!(north(8000.0) >= 10000.0 - 1e-3, "{}", north(8000.0));
        // Schnitt durch die Westwand (Ebene y = 4 m): ohne Querlinie bei UK
        // Untersicht; durch die Nordwand (x = 5 m) mit Untersicht und Stufe
        let cw = c.section_caps(vec3(0.0, 4000.0, 0.0), vec3(0.0, 1.0, 0.0));
        assert!(near(kante_bei_x(&cw, 2515.0, 0.0), 0.0));
        assert!(cw
            .edges
            .iter()
            .all(|e| !(near(e.a.z, 2515.0) && near(e.b.z, 2515.0))));
        let cn = c.section_caps(vec3(5000.0, 0.0, 0.0), vec3(1.0, 0.0, 0.0));
        let at = |z: f64| {
            cn.edges
                .iter()
                .filter(|e| near(e.a.z, z) && near(e.b.z, z))
                .count()
        };
        assert!(at(2515.0) >= 2, "{}", at(2515.0));
        let f = m.floor(eg).unwrap().unwrap();
        let fc = f.soffit_section_caps(vec3(5000.0, 0.0, 0.0), vec3(1.0, 0.0, 0.0));
        let soffit = fc
            .triangles
            .iter()
            .filter(|t| t.p.iter().all(|p| p.z <= 2635.0 + 1e-6))
            .count();
        assert!(soffit > 0);
        // Dämmschraffur längs der Schicht (Jörn 05:41): v quer über die Dicke
        // (unten 0, oben 1), u läuft waagerecht
        for t in &fc.triangles {
            for (p, uv) in t.p.iter().zip(t.uv) {
                let v = if near(p.z, 2515.0) { 0.0 } else { 1.0 };
                assert!(near(uv[1], v), "{p:?} {uv:?}");
            }
        }
    }

    #[test]
    fn ruecksprung_laesst_decke_und_daemmung_stehen() {
        let (bm, beg, _) = mit_versatz(0.0);
        let (m, eg, og) = mit_versatz(-300.0);
        let (f, bf) = (
            m.floor(eg).unwrap().unwrap(),
            bm.floor(beg).unwrap().unwrap(),
        );
        assert_eq!(f.outline, bf.outline);
        assert!(f.soffit.is_none() && f.soffits.is_empty());
        assert!(m.chain(eg).unwrap().joints.overhang.is_none());
        assert_eq!(run_qto(&m, eg), run_qto(&bm, beg));
        // Das OG steht auf der Decke; dort gibt es nichts herabzuziehen
        let o = m.chain(og).unwrap();
        assert!(o.joints.overhang.is_none());
    }

    #[test]
    fn untersicht_einstellbar_und_rueckgaengig_als_ganzes() {
        let (mut m, eg, og) = gebaeude();
        let flaeche = m.floor(eg).unwrap().unwrap().area();
        // OG-Nordwand lösen und 0,30 m vorziehen: ein Schritt
        let w = m.wall_at(og, 1).unwrap();
        m.begin("Vorziehen");
        note!(m, Element, m.elements, w);
        set_coupling(&mut m, w, None);
        let out = {
            let c = m.chain(og).unwrap();
            let c = c.with_segment_moved(1, c.outward_sign() * 300.0).unwrap();
            m.set_run_points(og, &c.points).unwrap()
        };
        // Die EG-Wand darunter ändert ihren Körper mit
        assert!(out.contains(&eg), "{out:?}");
        let t = m.commit().unwrap();
        assert!(m.chain(eg).unwrap().joints.overhang.is_some());
        assert!(m.floor(eg).unwrap().unwrap().area() > flaeche + 1.0);
        // UD entsteht im selben Schritt
        let fl = m.floors_of(eg)[0];
        let ud = m.soffit_of(fl).unwrap();
        assert!(m.check().is_empty(), "{:?}", m.check());
        // Dicke 40 … 300 mm, nie 0 (Regel 35); 20 cm: die Schichten reichen tiefer
        m.begin("Untersicht");
        assert!(!m.set_floor_soffit(fl, 0.0));
        assert!(!m.set_floor_soffit(fl, MIN_SOFFIT - 1.0));
        assert!(!m.set_floor_soffit(fl, MAX_SOFFIT + 1.0));
        assert!(m.set_floor_soffit(fl, 200.0));
        let t2 = m.commit().unwrap();
        assert_eq!(m.chain(eg).unwrap().joints.overhang.unwrap().from, 2435.0);
        assert_eq!(m.soffit_of(fl), Some(ud), "bleibt mit Nummer");
        let touched = m.apply(&t2, Direction::Undo);
        assert!(touched.runs.contains(&eg));
        assert_eq!(m.chain(eg).unwrap().joints.overhang.unwrap().from, 2515.0);
        // Rückgängig: alles wieder bündig, Decke wie vorher, keine UD
        let touched = m.apply(&t, Direction::Undo);
        assert!(touched.runs.contains(&eg), "{:?}", touched.runs);
        assert!(m.chain(eg).unwrap().joints.overhang.is_none());
        assert!(near(m.floor(eg).unwrap().unwrap().area(), flaeche));
        assert!(m.soffit_of(fl).is_none());
        assert!(m.check().is_empty(), "{:?}", m.check());
        m.apply(&t, Direction::Redo);
        assert!(m.chain(eg).unwrap().joints.overhang.is_some());
        assert_eq!(m.soffit_of(fl), Some(ud));
    }

    #[test]
    fn untersicht_dicke_in_der_datei() {
        let (mut m, eg, _) = mit_versatz(300.0);
        let fl = m.floors_of(eg)[0];
        m.allow_unstepped();
        assert!(m.set_floor_soffit(fl, 160.0));
        let text = crate::szo::write(&m);
        assert!(text.contains("soffit=160"), "{text}");
        assert!(
            text.contains("[soffit]") || text.contains("soffit guid"),
            "{text}"
        );
        let back = crate::szo::read(&text, crate::GuidGen::with_seed(3))
            .unwrap()
            .model;
        let eg2 = back
            .runs
            .ids()
            .find(|r| back.run_below(*r).is_none() && !back.runs_above(*r).is_empty())
            .unwrap();
        assert_eq!(
            back.floor(eg2).unwrap().unwrap().soffit.unwrap().thickness,
            160.0
        );
        assert!(back.check().is_empty(), "{:?}", back.check());
        let ud = |b: &Model| {
            b.elements()
                .iter()
                .find(|(_, e)| e.category == Category::SoffitInsulation)
                .map(|(_, e)| (e.guid, e.number.clone()))
        };
        assert_eq!(ud(&back), ud(&m), "Guid und Nummer bleiben");
        // Ohne Schlüssel (Datei vor K4): 12 cm; Standarddicke schreibt keinen
        let old = text.replace(" soffit=160", "");
        let back = crate::szo::read(&old, crate::GuidGen::with_seed(3))
            .unwrap()
            .model;
        let eg3 = back
            .runs
            .ids()
            .find(|r| back.run_below(*r).is_none() && !back.runs_above(*r).is_empty())
            .unwrap();
        assert_eq!(
            back.floor(eg3).unwrap().unwrap().soffit.unwrap().thickness,
            120.0
        );
    }
    /// Lösen, Versatz, Einrasten unter 2 cm, wieder koppeln (Versatz bleibt),
    /// bündig setzen; gekoppelt zieht der Fuß der Kette (OG Phase 2).
    #[test]
    fn koppeln_versatz_buendig() {
        let (mut m, eg, og) = gebaeude();
        let w = m.wall_at(og, 1).unwrap();
        let p = m.wall_at(eg, 1).unwrap();
        assert_eq!(m.stack_offset(p), None, "EG hat keinen Partner");
        assert_eq!(m.chain_foot(w), p);
        assert!(m.set_linked(w, false));
        assert_eq!(m.chain_foot(w), w);
        assert!(m.move_segment(w, -300.0).is_some());
        assert_eq!(m.stack_offset(w), Some((-300.0, false)));
        assert_eq!(ys(&m, og), (7700.0, 7700.0));
        assert!(m.check().is_empty(), "{:?}", m.check());
        // EG +1,00: gelöstes OG bleibt, Versatz −1300
        assert!(drag(&mut m, eg, 1, 1000.0));
        assert_eq!(m.stack_offset(w), Some((-1300.0, false)));
        assert_eq!(ys(&m, og), (7700.0, 7700.0));
        // Koppeln: Versatz bleibt, EG −1,00 nimmt das OG mit
        assert!(m.set_linked(w, true));
        assert!(drag(&mut m, eg, 1, -1000.0));
        assert_eq!(m.stack_offset(w), Some((-1300.0, true)));
        assert_eq!(ys(&m, og), (6700.0, 6700.0));
        // 1 cm rastet auf 0
        assert!(m.set_offset(w, 10.0).is_some());
        assert_eq!(m.stack_offset(w), Some((0.0, true)));
        assert!(m.move_segment(w, 300.0).is_some());
        assert!(m.set_flush(w));
        assert_eq!(m.stack_offset(w), Some((0.0, true)));
        assert_eq!(m.run(og).unwrap().points, m.run(eg).unwrap().points);
        assert!(m.check().is_empty(), "{:?}", m.check());
    }

    /// Gelöste Segmente schreiben `link=0`, sonst bleibt die Datei gleich;
    /// `link=2` wird abgelehnt.
    #[test]
    fn datei_link() {
        let (mut m, _, og) = gebaeude();
        let plain = crate::szo::write(&m);
        assert!(!plain.contains("link="));
        let w = m.wall_at(og, 1).unwrap();
        m.set_linked(w, false);
        let text = crate::szo::write(&m);
        assert_eq!(text.matches(" link=0").count(), 1);
        let back = crate::szo::read(&text, GuidGen::with_seed(5))
            .unwrap()
            .model;
        assert_eq!(crate::szo::write(&back), text);
        let bad = text.replace(" link=0", " link=2");
        let e = crate::szo::read(&bad, GuidGen::with_seed(5)).err().unwrap();
        assert!(format!("{e:?}").contains("link"), "{e:?}");
    }
}
