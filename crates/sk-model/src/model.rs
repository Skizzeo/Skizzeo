//! Das Gebäudemodell als Datenbank: Bibliothek, Geschosse, Bauteile, Wandzüge.
//!
//! Bauteile verweisen nur über Kennungen aufeinander, nie über Vec-Plätze.
//! Geometrie wird aus der Parametrik abgeleitet ([`Model::chain`]) und nie
//! gespeichert. Jede Änderung erhöht die Revision.

use crate::attr::{
    self, Attributes, Display, Fill, FillId, LineType, LineTypeId, Pen, PenId, Surface, SurfaceId,
};
use crate::element::{
    Category, Element, ElementId, ElementKind, PropSet, RunId, Storey, StoreyId, Wall, WallRun,
};
use crate::guid::{Guid, GuidGen};
use crate::id::Arena;
use crate::library::{
    material_key, LayerFunction, LayerSet, LayerSetId, MatCategory, Material, MaterialId,
    MaterialLayer,
};
use crate::solid::material;
use crate::wall::{clean_points, segment_count, Layer, RefSide, WallChain};
use sk_math::{vec3, Vec3};

/// Voreinstellungen für neue Bauteile.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Defaults {
    pub storey: StoreyId,
    pub exterior_wall: LayerSetId,
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
    /// Stifte, Schraffuren, Oberflächen und Bauteildarstellung.
    attr: Attributes,
    materials: Arena<Material>,
    layer_sets: Arena<LayerSet>,
    storeys: Arena<Storey>,
    elements: Arena<Element>,
    runs: Arena<WallRun>,
    defaults: Defaults,
    /// Zuletzt vergebene laufende Nummer je Kategorie (wird nie zurückgesetzt).
    numbers: [u32; Category::ALL.len()],
    revision: u64,
    guids: GuidGen,
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
        Model {
            attr,
            materials,
            layer_sets,
            storeys,
            elements: Arena::new(),
            runs: Arena::new(),
            defaults: Defaults {
                storey,
                exterior_wall,
            },
            numbers: [0; Category::ALL.len()],
            revision: 0,
            guids,
        }
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
        self.attr.add_pen(p)
    }

    pub fn add_line_type(&mut self, l: LineType) -> LineTypeId {
        self.touch();
        self.attr.add_line_type(l)
    }

    pub fn add_fill(&mut self, f: Fill) -> FillId {
        self.touch();
        self.attr.add_fill(f)
    }

    pub fn add_surface(&mut self, s: Surface) -> SurfaceId {
        self.touch();
        self.attr.add_surface(s)
    }

    pub fn set_pen(&mut self, id: PenId, p: Pen) -> bool {
        let ok = self.attr.set_pen(id, p);
        self.revision += ok as u64;
        ok
    }

    pub fn set_fill(&mut self, id: FillId, f: Fill) -> bool {
        let ok = self.attr.set_fill(id, f);
        self.revision += ok as u64;
        ok
    }

    pub fn set_surface(&mut self, id: SurfaceId, s: Surface) -> bool {
        let ok = self.attr.set_surface(id, s);
        self.revision += ok as u64;
        ok
    }

    pub fn set_display(&mut self, d: Display) {
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
        self.materials.insert(m)
    }

    pub fn layer_sets(&self) -> &Arena<LayerSet> {
        &self.layer_sets
    }

    pub fn layer_set(&self, id: LayerSetId) -> Option<&LayerSet> {
        self.layer_sets.get(id)
    }

    pub fn add_layer_set(&mut self, s: LayerSet) -> LayerSetId {
        self.touch();
        self.layer_sets.insert(s)
    }

    /// Ändert einen Aufbau; alle Bauteile dieses Typs folgen.
    pub fn set_layer_set(&mut self, id: LayerSetId, s: LayerSet) -> bool {
        match self.layer_sets.get_mut(id) {
            Some(old) => {
                *old = s;
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
        let e = self.elements.get_mut(id).ok_or(NumberError::NoElement)?;
        e.number = number.to_string();
        self.touch();
        Ok(())
    }

    fn new_wall(&mut self, run: RunId, seg: usize, template: &Element) -> ElementId {
        let guid = self.new_guid();
        let number = self.next_number(template.category);
        self.elements.insert(Element {
            guid,
            number,
            kind: ElementKind::Wall(Wall {
                run,
                seg: seg as u32,
            }),
            ..template.clone()
        })
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
        self.touch();
        Some(run)
    }

    /// Setzt neue Eckpunkte eines Wandzugs. Bleibt die Segmentzahl gleich, behält
    /// jede Wand ihren Platz (Verschieben). Ändert sie sich, behalten die Wände
    /// ihre Kennung nach Lage (Regel 11): ein geteiltes Segment gibt sie an seinen
    /// längeren Teil weiter, ein weggefallenes Segment nimmt genau seine mit.
    pub fn set_run_points(&mut self, id: RunId, points: &[Vec3]) -> bool {
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
        let Some(run) = self.runs.remove(id) else {
            return false;
        };
        for e in run.segments {
            self.elements.remove(e);
        }
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

    /// Geometrie eines Wandzugs mit den Schichten seines Aufbaus.
    /// Haben die Wände eines Zuges verschiedene Aufbauten, gilt bis zu den
    /// Wandanschlüssen (Paket B5) der des ersten Segments.
    pub fn chain(&self, id: RunId) -> Option<WallChain> {
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
        })
    }

    /// Alle Wandzüge als Geometrie.
    pub fn chains(&self) -> impl Iterator<Item = (RunId, WallChain)> + '_ {
        self.runs
            .ids()
            .filter_map(|id| self.chain(id).map(|c| (id, c)))
    }

    // --- Rückgängig -------------------------------------------------------

    /// Setzt das Modell auf einen früheren Stand zurück (Rückgängig, Abbruch).
    /// Guid-Erzeuger und Nummernzähler laufen weiter, damit nichts doppelt
    /// vergeben wird; die Revision steigt.
    pub fn restore(&mut self, mut earlier: Model) {
        // Kennungen aus dem verworfenen Stand dürfen nicht wiederkehren
        earlier.materials.keep_generations(&self.materials);
        earlier.layer_sets.keep_generations(&self.layer_sets);
        earlier.storeys.keep_generations(&self.storeys);
        earlier.elements.keep_generations(&self.elements);
        earlier.runs.keep_generations(&self.runs);
        earlier.attr.replace(&self.attr);
        let guids = self.guids.clone();
        let revision = self.revision + 1;
        let mut numbers = self.numbers;
        for (n, e) in numbers.iter_mut().zip(earlier.numbers) {
            *n = (*n).max(e);
        }
        *self = earlier;
        self.guids = guids;
        self.numbers = numbers;
        self.revision = revision;
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
            for e in &r.segments {
                if self.segment_of(*e).map(|s| s.0) != Some(id) {
                    out.push(format!(
                        "Wandzug {id:?}: Wand {e:?} fehlt oder gehört woandershin"
                    ));
                }
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
        out
    }
}

/// Segmente eines Zuges als (Anfang, Ende).
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
        assert!(m.set_run_points(r, &moved.points));
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
        assert!(m2.set_run_points(r2, &pts[..2]));
        assert_eq!(m2.run(r2).unwrap().segments, segs[..1]);
        assert!(m2.element(segs[1]).is_none());
        // Und wieder mehr: neue Wand mit neuer Nummer
        assert!(m2.set_run_points(r2, &pts));
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
        assert!(m.set_run_points(r, &q));
        let t = m.run(r).unwrap().segments.clone();
        assert_eq!(t.len(), 5);
        assert_eq!((t[0], t[3], t[4]), (s[0], s[2], s[3]));
        assert_ne!(t[1], s[1]);
        assert_eq!(t[2], s[1], "der längere Teil behält die Kennung");
        assert_eq!(m.element(t[1]).unwrap().number, "AW-005");
        // Den Punkt wieder entfernen: das kurze Stück verschwindet mit seiner Kennung
        assert!(m.set_run_points(r, &p));
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
        assert!(m.set_run_points(r2, &o[1..]));
        assert_eq!(m.run(r2).unwrap().segments, s2[1..]);
        assert!(m.element(s2[0]).is_none());
        assert!(m.check().is_empty(), "{:?}", m.check());
    }

    #[test]
    fn rueckgaengig_vergibt_nichts_doppelt() {
        let mut m = Model::with_seed(5);
        let empty = m.clone();
        let r = rechteck(&mut m);
        let guid = m.run(r).unwrap().guid;
        let rev = m.revision();
        m.restore(empty);
        assert!(m.runs().is_empty() && m.elements().is_empty());
        assert!(m.revision() > rev);
        let r = rechteck(&mut m);
        assert_ne!(m.run(r).unwrap().guid, guid);
        let first = m.run(r).unwrap().segments[0];
        assert_eq!(m.element(first).unwrap().number, "AW-005");
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
