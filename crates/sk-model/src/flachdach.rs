//! Flachdach (Jörn 10.10., planung/flachdach/plan-heute.md D1–D4): die
//! Ebene „Flachdach“ über dem obersten Geschoss, die Aufkantung als
//! gekoppelter Zug im Außenwandtyp darunter, der waagerechte Dachaufbau und
//! das Attikablech auf der Krone der Aufkantung.

use super::*;
use crate::gefaelle::{self, Drainage, SlopeField};
use crate::roof::FlatRoof;

/// Werkstyp „Flachdach 21,5“ (DA-21,5) und seine eingebauten Baustoffe:
/// feste Guids, angelegt, sobald das erste Flachdach sie braucht (ältere
/// Dateien bleiben bytegleich).
pub const ROOF_TYPE_GUID: Guid = Guid(0x23a600a96432444383537a73429b10b9);
pub const ROOF_SEAL_GUID: Guid = Guid(0x98ba84e68f884dc8a9afb9e5d7d744d6);
pub const ROOF_INSULATION_GUID: Guid = Guid(0x58eea8c1e6de400c8a943c7bae55718c);
pub const ROOF_VAPOUR_GUID: Guid = Guid(0x22943253428c444fbbaa503ccbaa312e);
/// Eigener Baustoff des Dämmstreifens auf der Krone (W9 an #60), mit
/// fester Oberfläche: auch beim Lesen angelegt, träfe eine Guid aus der
/// Folge sonst eine schon in der Datei vergebene.
pub const ROOF_CROWN_GUID: Guid = Guid(0x50741868ca4a4f23b98ac302f995e825);
pub const ROOF_CROWN_NAME: &str = "Dämmung EPS 035 DAA dh, Attikakrone";
const ROOF_CROWN_SURFACE_GUID: Guid = Guid(0xa7efe20a51034f36a754161b1fb7b3de);
/// Aufbau des Werkstyps von oben nach unten (Plan Flachdach P10):
/// Abdichtung Bitumen 2-lagig, Dämmung EPS 035 DAA dh, Dampfsperre.
const ROOF_BUILD_UP: [(f64, LayerFunction); 3] = [
    (10.0, LayerFunction::Membrane),
    (200.0, LayerFunction::Insulation),
    (5.0, LayerFunction::Membrane),
];
/// Mindestens so hoch steht die Aufkantung über der Dachhaut (mm,
/// Flachdachrichtlinie: Anschlusshöhe am Dachrand 10 cm).
pub const MIN_ROOF_EDGE: f64 = 100.0;

/// Mindestwärmeschutz eines Daches über Aufenthaltsräumen (m²K/W, DIN
/// 4108-2 Tabelle 3): gilt auch an der dünnsten Stelle der Gefälledämmung.
pub const MIN_ROOF_R: f64 = 1.2;
/// Guid der Ebene Flachdach: die des Gebäudes mit diesem Muster verknüpft.
const ROOF_LEVEL_SALT: u128 = 0x4644_0000_0000_4000_8000_0000_0000_4644;

/// Erste Schicht innen vom (letzten) Kern: Ab hier hat die Aufkantung keine
/// Schicht (Innenputz, Bekleidung; Jörn 10.10.: zum Dach hin ist außen, die
/// Abdichtung läuft bis zur Anschlusshöhe hoch). Ohne Kern entfällt nichts.
pub(crate) fn parapet_inner_start(layers: &[MaterialLayer]) -> usize {
    layers
        .iter()
        .rposition(|l| l.core)
        .map_or(layers.len(), |i| i + 1)
}

impl Model {
    /// Schichten des Zugs `run` im Typ `set` für seine Geometrie. In der
    /// Aufkantung bleiben die Schichten innen vom Kern ohne Körper (wie eine
    /// Luftschicht): Sie behalten ihre Dicke, damit Bezugslinie und
    /// Außenfläche zur Wand darunter passen; den Raum nimmt der Dachaufbau.
    pub(super) fn run_layers(&self, run: RunId, set: LayerSetId) -> Vec<Layer> {
        let mut layers = self.wall_layers(set);
        if let Some(from) = self.parapet_inner_layers(run) {
            for l in layers.iter_mut().skip(from) {
                l.air = true;
            }
        }
        layers
    }

    /// Erste weggelassene Schicht der Aufkantung `run` (siehe
    /// [`parapet_inner_start`]); `None`, wenn `run` keine Aufkantung ist
    /// oder nichts wegfällt.
    pub(crate) fn parapet_inner_layers(&self, run: RunId) -> Option<usize> {
        if self.category_of(run) != Some(Category::Parapet) {
            return None;
        }
        let set = self
            .run(run)?
            .segments
            .first()
            .and_then(|e| self.element(*e))?;
        let layers = &self.layer_set(set.layer_set?)?.layers;
        let from = parapet_inner_start(layers);
        (from < layers.len()).then_some(from)
    }

    /// Dicke der weggelassenen Schichten der Aufkantung `run` (mm).
    fn parapet_inner_thickness(&self, run: RunId) -> f64 {
        let Some(from) = self.parapet_inner_layers(run) else {
            return 0.0;
        };
        self.run(run)
            .and_then(|r| r.segments.first())
            .and_then(|e| self.element(*e)?.layer_set)
            .and_then(|t| self.layer_set(t))
            .map_or(0.0, |s| s.layers[from..].iter().map(|l| l.thickness).sum())
    }
}

impl Model {
    /// Ebene Flachdach des Gebäudes `b` (`None`: der Vorlage), falls es eine
    /// hat.
    pub fn roof_level_of(&self, b: Option<BuildingId>) -> Option<StoreyId> {
        self.levels_in(b)
            .into_iter()
            .find(|id| self.storey(*id).is_some_and(|s| s.kind == LevelKind::Roof))
    }

    /// Ebene Flachdach des Gebäudes, zu dem das Geschoss `id` gehört.
    pub fn roof_level(&self, id: StoreyId) -> Option<StoreyId> {
        self.roof_level_of(self.storey(id)?.building)
    }

    /// Ist `wall` eine Aufkantung des Flachdachs?
    pub fn is_parapet(&self, wall: ElementId) -> bool {
        self.element(wall)
            .is_some_and(|e| e.category == Category::Parapet)
    }

    /// Aufkantungen auf der Ebene Flachdach `fd` (D2): über jedem
    /// geschlossenen Außenwandzug des Geschosses darunter, der noch keinen
    /// Zug darüber hat, ein gekoppelter Zug im selben Wandtyp. Alle Schichten
    /// laufen weiter; der Kern steht auf der Decke wie ein Geschoss höher.
    fn add_parapets(&mut self, fd: StoreyId) {
        let Some(below) = self.level_below(fd) else {
            return;
        };
        let roots: Vec<RunId> = self
            .runs
            .iter()
            .filter(|(_, r)| r.storey == below)
            .map(|(id, _)| id)
            .filter(|id| self.needs_floor(*id) && self.runs_above(*id).is_empty())
            .collect();
        for r in roots {
            self.stack_run(r, fd);
        }
    }

    /// Schaltet die Ebene Flachdach (Jörn 10.10.) über dem obersten
    /// Geschoss des Gebäudes ein oder aus, zu dem das Geschoss `id` gehört:
    /// Band „Flachdach“/„FD“ von OK Rohdecke darunter, [`ROOF_UPSTAND`]
    /// hoch. Ausschalten nimmt die Ebene samt allem, was auf ihr steht.
    /// `false` ohne Geschoss oder ohne Geschoss über der Gründung.
    pub fn set_flat_roof(&mut self, id: StoreyId, on: bool) -> bool {
        let Some(b) = self.storey(id).map(|s| s.building) else {
            return false;
        };
        match (on, self.roof_level_of(b)) {
            (true, Some(_)) | (false, None) => true,
            (true, None) => {
                let Some(top) = self
                    .levels_in(b)
                    .last()
                    .and_then(|t| self.storey(*t))
                    .filter(|s| s.kind == LevelKind::Storey)
                    .map(|s| s.top())
                else {
                    return false;
                };
                // Guid aus der des Gebäudes: Ein- und Ausschalten (etwa im
                // Gebäude-Dialog, P8) verbraucht keine aus dem Erzeuger
                let of = b
                    .and_then(|b| self.building(b))
                    .map_or(self.project.guid, |x| x.guid);
                let st = Storey {
                    guid: Guid(of.0 ^ ROOF_LEVEL_SALT),
                    building: b,
                    name: "Flachdach".into(),
                    short: "FD".into(),
                    kind: LevelKind::Roof,
                    elevation: top,
                    height: ROOF_UPSTAND,
                    embed: None,
                };
                let fd = self.storeys.insert(st);
                note!(self, Storey, new fd);
                self.add_parapets(fd);
                self.touch();
                true
            }
            (false, Some(fd)) => {
                let runs: Vec<RunId> = self
                    .runs
                    .iter()
                    .filter(|(_, r)| r.storey == fd)
                    .map(|(r, _)| r)
                    .collect();
                for r in runs {
                    self.remove_run(r);
                }
                // Dachaufbau und Blech gehören zur Ebene
                let derived: Vec<ElementId> = self
                    .elements
                    .iter()
                    .filter(|(_, e)| e.storey == fd)
                    .map(|(id, _)| id)
                    .collect();
                for id in derived {
                    note!(self, Element, self.elements, id);
                    self.elements.remove(id);
                }
                note!(self, Storey, self.storeys, fd);
                self.storeys.remove(fd);
                self.touch();
                true
            }
        }
    }
}

impl Model {
    // --- Dachaufbau und Attikablech (D3, D4) ------------------------------

    /// Zug der Aufkantung auf dem Zug `run`, falls es einen gibt.
    pub fn parapet_above(&self, run: RunId) -> Option<RunId> {
        self.runs_above(run)
            .into_iter()
            .find(|r| self.category_of(*r) == Some(Category::Parapet))
    }

    /// Decken unter einem Flachdach: über ihrem Zug steht eine Aufkantung.
    /// Nach Nummer.
    pub fn roof_floors(&self) -> Vec<ElementId> {
        self.roof_floors_in(None)
    }

    fn roof_floors_in(&self, scope: Option<&[RunId]>) -> Vec<ElementId> {
        let mut out: Vec<(String, ElementId)> = self
            .elements
            .iter()
            .filter_map(|(id, e)| match e.kind {
                ElementKind::Floor(f)
                    if in_scope(scope, f.run) && self.parapet_above(f.run).is_some() =>
                {
                    Some((e.number.clone(), id))
                }
                _ => None,
            })
            .collect();
        out.sort_by(|a, b| a.0.cmp(&b.0));
        out.into_iter().map(|x| x.1).collect()
    }

    /// Liegt auf der Decke `floor` ein Flachdach? Dann gehört ihr
    /// Attikablech dem Flachdach, nicht einer Dachterrasse.
    pub fn is_roof_floor(&self, floor: ElementId) -> bool {
        matches!(self.element(floor).map(|e| &e.kind),
            Some(ElementKind::Floor(f)) if self.parapet_above(f.run).is_some())
    }

    /// Gehört das Blech `coping` auf der Decke `floor` dem Flachdach? Ja,
    /// solange über der Decke eine Aufkantung steht oder es auf der Ebene
    /// Flachdach liegt.
    pub fn flat_roof_coping(&self, coping: ElementId, floor: ElementId) -> bool {
        self.is_roof_floor(floor)
            || self
                .element(coping)
                .and_then(|e| self.storey(e.storey))
                .is_some_and(|s| s.kind == LevelKind::Roof)
    }

    /// Dachaufbau auf der Decke `floor`.
    pub fn flat_roof_of(&self, floor: ElementId) -> Option<ElementId> {
        self.elements
            .iter()
            .find(|(_, e)| matches!(e.kind, ElementKind::Roof { floor: f, .. } if f == floor))
            .map(|(id, _)| id)
    }

    /// Typ des Dachaufbaus `roof`: der am Bauteil oder der Werkstyp
    /// „Flachdach 21,5“, sobald es ihn im Projekt gibt.
    pub fn flat_roof_type(&self, roof: ElementId) -> Option<LayerSetId> {
        self.roof_type_or_default(self.element(roof)?.layer_set)
    }

    fn roof_type_or_default(&self, chosen: Option<LayerSetId>) -> Option<LayerSetId> {
        chosen
            .filter(|id| {
                self.layer_set(*id)
                    .is_some_and(|x| x.category == TypeCategory::FlatRoof)
            })
            .or_else(|| self.type_by_guid(ROOF_TYPE_GUID))
    }

    /// Schichten des Dachaufbaus `roof`, von oben nach unten.
    pub fn flat_roof_layers(&self, roof: ElementId) -> Vec<MaterialLayer> {
        self.flat_roof_type(roof)
            .and_then(|t| self.layer_set(t))
            .map_or_else(Vec::new, |t| t.layers.clone())
    }

    /// Werkstyp „Flachdach 21,5“ (DA-21,5); fehlt er, wird er samt
    /// Baustoffen angelegt. Alle Schichten legt der Dachdecker.
    fn ensure_roof_type(&mut self) -> Option<LayerSetId> {
        if let Some(id) = self.type_by_guid(ROOF_TYPE_GUID) {
            return Some(id);
        }
        let seal = self.builtin_material(
            ROOF_SEAL_GUID,
            "Abdichtung Bitumen 2-lagig",
            MatCategory::Membrane,
            200,
            1100.0,
            Some(0.17),
            [70, 72, 76],
            [40, 42, 46],
            trade::roofing(),
        )?;
        let insulation = self.builtin_material(
            ROOF_INSULATION_GUID,
            "Dämmung EPS 035 DAA dh",
            MatCategory::Insulation,
            300,
            20.0,
            Some(0.035),
            [236, 232, 214],
            [214, 208, 184],
            trade::roofing(),
        )?;
        let vapour = self.builtin_material(
            ROOF_VAPOUR_GUID,
            "Dampfsperre Bitumen-Alu",
            MatCategory::Membrane,
            200,
            1100.0,
            Some(0.17),
            [120, 124, 130],
            [90, 94, 100],
            trade::roofing(),
        )?;
        let mats = [seal, insulation, vapour];
        let layers = ROOF_BUILD_UP
            .iter()
            .zip(mats)
            .map(|(&(d, f), m)| MaterialLayer::new(m, d, f).trade(trade::roofing()))
            .collect();
        let code = self.free_code("DA-21,5");
        self.add_layer_set(LayerSet {
            guid: ROOF_TYPE_GUID,
            name: "Flachdach 21,5".into(),
            code,
            category: TypeCategory::FlatRoof,
            layers,
            props: PropSet::new(),
            note: String::new(),
            changed: 1,
            bearing: Bearing::Core,
            height: None,
        })
    }

    /// Setzt den Typ des Dachaufbaus `roof` (`None`: Werkstyp); er muss die
    /// Typart Flachdach haben.
    pub fn set_flat_roof_type(&mut self, roof: ElementId, t: Option<LayerSetId>) -> bool {
        if t.is_some_and(|t| {
            self.layer_set(t)
                .is_none_or(|x| x.category != TypeCategory::FlatRoof)
        }) {
            return false;
        }
        let Some(e) = self.element(roof) else {
            return false;
        };
        if !matches!(e.kind, ElementKind::Roof { .. }) {
            return false;
        }
        let t = t.or_else(|| self.type_by_guid(ROOF_TYPE_GUID));
        if e.layer_set == t {
            return true;
        }
        note!(self, Element, self.elements, roof);
        if let Some(e) = self.elements.get_mut(roof) {
            e.layer_set = t;
        }
        self.touch();
        true
    }

    /// Gefälle und Abläufe des Dachaufbaus auf der Decke `floor`.
    pub fn drainage_of(&self, floor: ElementId) -> Option<&Drainage> {
        self.drainage_of_roof(self.flat_roof_of(floor)?)
    }

    /// Gefälle und Abläufe des Dachaufbaus `roof`.
    pub fn drainage_of_roof(&self, roof: ElementId) -> Option<&Drainage> {
        match &self.element(roof)?.kind {
            ElementKind::Roof { drainage, .. } => Some(drainage),
            _ => None,
        }
    }

    /// Setzt Gefälle und Abläufe des Dachaufbaus `roof` (Gefälledämmung,
    /// G2). Gefälle 0 bis 100 %, Abläufe auf z = 0.
    pub fn set_roof_drainage(&mut self, roof: ElementId, d: Drainage) -> bool {
        let ok = d.slope.is_finite()
            && (0.0..=100.0).contains(&d.slope)
            && d.drains.iter().all(|p| p.x.is_finite() && p.y.is_finite());
        let Some(ElementKind::Roof { drainage, .. }) = self.element(roof).map(|e| &e.kind) else {
            return false;
        };
        if !ok {
            return false;
        }
        if *drainage == d {
            return true;
        }
        let d = Drainage {
            drains: d.drains.iter().map(|p| vec3(p.x, p.y, 0.0)).collect(),
            ..d
        };
        note!(self, Element, self.elements, roof);
        if let Some(ElementKind::Roof { drainage, .. }) =
            self.elements.get_mut(roof).map(|e| &mut e.kind)
        {
            *drainage = d;
        }
        self.touch();
        true
    }

    /// Gefälle des Dachaufbaus `roof` in Prozent (0 = waagerecht). Hat das
    /// Dach noch keine Abläufe, schlägt Skizzeo sie vor.
    pub fn set_roof_slope(&mut self, roof: ElementId, slope: f64) -> bool {
        let Some(ElementKind::Roof { floor, drainage }) = self.element(roof).map(|e| &e.kind)
        else {
            return false;
        };
        let mut d = Drainage {
            slope,
            ..drainage.clone()
        };
        if d.on() && d.drains.is_empty() {
            d.drains = self.proposed_drains(*floor);
        }
        self.set_roof_drainage(roof, d)
    }

    /// Ersetzt die Abläufe des Dachaufbaus `roof` durch den Vorschlag.
    pub fn propose_roof_drains(&mut self, roof: ElementId) -> bool {
        let Some(ElementKind::Roof { floor, drainage }) = self.element(roof).map(|e| &e.kind)
        else {
            return false;
        };
        let drains = self.proposed_drains(*floor);
        if drains.is_empty() {
            return false;
        }
        let d = Drainage {
            drains,
            ..drainage.clone()
        };
        self.set_roof_drainage(roof, d)
    }

    /// Ein weiterer Ablauf am Dachaufbau `roof` mit Gefälle (Jörn 11.10.),
    /// dort an der Aufkantung, wo er Fließweg und Einzugsfläche am meisten
    /// entlastet. `false`, wenn das Dach kein Gefälle hat oder kein Platz
    /// mehr frei ist.
    pub fn add_roof_drain(&mut self, roof: ElementId) -> bool {
        let Some(ElementKind::Roof { floor, drainage }) = self.element(roof).map(|e| &e.kind)
        else {
            return false;
        };
        let Some(r) = self.flat_roof_over(*floor).filter(|_| drainage.on()) else {
            return false;
        };
        let lim = gefaelle::Limits::default();
        let Some(p) = gefaelle::next_drain(&r.outline, &drainage.drains, &lim) else {
            return false;
        };
        let mut d = drainage.clone();
        d.drains.push(p);
        self.set_roof_drainage(roof, d)
    }

    /// Löscht den Ablauf `k` am Dachaufbau `roof` (Jörn 11.10.). Den
    /// letzten nicht: Ein Dach mit Gefälle braucht einen Ablauf.
    pub fn remove_roof_drain(&mut self, roof: ElementId, k: usize) -> bool {
        let Some(mut d) = self.drainage_of_roof(roof).cloned() else {
            return false;
        };
        if k >= d.drains.len() || d.drains.len() < 2 {
            return false;
        }
        d.drains.remove(k);
        // sein Regenfallrohr geht mit, die übrigen behalten ihr Bauteil
        self.drop_downpipe(roof, k);
        self.set_roof_drainage(roof, d)
    }

    fn proposed_drains(&self, floor: ElementId) -> Vec<Vec3> {
        self.flat_roof_over(floor).map_or_else(Vec::new, |r| {
            gefaelle::propose_drains(&r.outline, &gefaelle::Limits::default())
        })
    }

    /// Geometrie von Dachaufbau und Attikablech über dem Zug der
    /// Aufkantung `ak`; `None`, wenn `ak` keine Aufkantung auf einer Decke
    /// ist.
    pub fn flat_roof(&self, ak: RunId) -> Option<FlatRoof> {
        self.flat_roof_on(ak, &self.base_chain(ak)?)
    }

    /// [`Model::flat_roof`] zum schon gebauten Zug `chain` der Aufkantung.
    pub fn flat_roof_on(&self, ak: RunId, chain: &WallChain) -> Option<FlatRoof> {
        if self.category_of(ak) != Some(Category::Parapet) || !chain.closed {
            return None;
        }
        let floor = self.floor_of(self.run_below(ak)?)?;
        let ElementKind::Floor(f) = self.element(floor)?.kind else {
            return None;
        };
        let ins = |m: MaterialId| {
            self.material(m)
                .is_some_and(|x| x.category == MatCategory::Insulation)
        };
        let chosen = self
            .flat_roof_of(floor)
            .and_then(|r| self.element(r)?.layer_set);
        let layers: Vec<(f64, u16, bool)> = match self
            .roof_type_or_default(chosen)
            .and_then(|t| self.layer_set(t))
        {
            Some(set) => set
                .layers
                .iter()
                .filter(|l| l.function != LayerFunction::AirGap)
                .map(|l| (l.thickness, material_key(l.material), ins(l.material)))
                .collect(),
            // im offenen Schritt, bis der Abgleich den Werkstyp anlegt
            None => ROOF_BUILD_UP
                .iter()
                .map(|&(d, f)| (d, material::PLAIN, f == LayerFunction::Insulation))
                .collect(),
        };
        // Innenfläche am Kern: die Schichten innen davon hat die Aufkantung nicht
        let drop = self.parapet_inner_thickness(ak);
        let (inner, outer) = (chain.inner_offset(), chain.outer_offset());
        let dir = (outer - inner).signum();
        let inner = inner + dir * drop;
        // Ohne Fugenpunkte und ohne Rückläufer an kurzen Eckstücken (#61)
        let clean = |p: Vec<Vec3>| {
            use sk_math::polygon;
            polygon::to_ccw(&polygon::simplified(&crate::terrace::without_spikes(p)))
        };
        let outline = clean(chain.face_corners(inner));
        // Attikadämmung innen (Jörn 10.10. 16:07): Innenfläche um ihre Dicke
        // weiter zum Dach hin, aus dem Umriss selbst (die Wandflächen kappen
        // spitze Ecken jenseits ihrer Gehrungsgrenze)
        let ins = crate::model::inner_of(&f.terrace);
        let (inset, inner_depth) = if ins > 0.0 {
            inset_depth(&outline, ins)
        } else {
            (outline.clone(), 0.0)
        };
        // Gefälle: der Keil liegt in der obersten Dämmschicht (Konzept F3)
        let tapered = layers.iter().position(|l| l.2);
        let slope = tapered
            .and(self.drainage_of(floor))
            .filter(|d| d.on())
            .and_then(|d| SlopeField::compute(&outline, &d.drains, d.slope));
        let mut r = FlatRoof {
            tapered: slope.as_ref().and(tapered),
            slope,
            outline,
            base: self.level_z(f.top)?,
            layers,
            ring: clean(chain.face_corners(chain.outer_offset())),
            width: chain.thickness() - drop,
            crown: chain.top(),
            coping_mat: self
                .coping_material_of(&f.terrace)
                .map_or(material::PLAIN, material_key),
            inner: ins,
            inner_depth,
            inset,
            cap: crate::model::crown_of(&f.terrace),
            ins_mat: self
                .flat_roof_insulation(floor)
                .map_or(material::PLAIN, material_key),
            cap_mat: self
                .flat_roof_crown_material(floor)
                .map_or(material::PLAIN, material_key),
            downpipes: Vec::new(),
        };
        r.downpipes = self.layout_downpipes(floor, &r);
        Some(r)
    }

    /// Baustoff der Attikadämmung am Flachdach über der Decke `floor`: die
    /// (oberste) Dämmschicht seines Aufbaus, sonst die des Werkstyps.
    pub fn flat_roof_insulation(&self, floor: ElementId) -> Option<MaterialId> {
        let chosen = self
            .flat_roof_of(floor)
            .and_then(|r| self.element(r)?.layer_set);
        self.roof_type_or_default(chosen)
            .and_then(|t| self.layer_set(t))
            .and_then(|t| {
                t.layers
                    .iter()
                    .find(|l| l.function == LayerFunction::Insulation)
                    .map(|l| l.material)
            })
            .or_else(|| self.material_by_guid(ROOF_INSULATION_GUID))
    }

    /// Baustoff des Dämmstreifens auf der Krone am Flachdach über der Decke
    /// `floor` (W9 an #60): mit der Werksdämmung im Aufbau der eigene
    /// Baustoff „Dämmung EPS 035 DAA dh, Attikakrone“, damit das LV Krone
    /// und innen am Baustoff trennt; mit einer anderen Dämmung im Aufbau
    /// diese, wie innen (die Optik bleibt). Fehlt der eigene Baustoff noch,
    /// die Dämmung innen.
    pub fn flat_roof_crown_material(&self, floor: ElementId) -> Option<MaterialId> {
        let ins = self.flat_roof_insulation(floor)?;
        if self
            .material(ins)
            .is_some_and(|m| m.guid == ROOF_INSULATION_GUID)
        {
            return self.material_by_guid(ROOF_CROWN_GUID).or(Some(ins));
        }
        Some(ins)
    }

    /// Eigener Baustoff der Kronendämmung am Flachdach: Darstellung, Kennwerte
    /// und Gewerk wie „Dämmung EPS 035 DAA dh“, Oberfläche mit fester Guid.
    /// Fehlt er, wird er angelegt; ohne die Werksdämmung keiner.
    pub(crate) fn ensure_roof_crown_material(&mut self) -> Option<MaterialId> {
        if let Some(id) = self.material_by_guid(ROOF_CROWN_GUID) {
            return Some(id);
        }
        let eps = self
            .material(self.material_by_guid(ROOF_INSULATION_GUID)?)?
            .clone();
        let surface = self.attr.surface(eps.surface)?.clone();
        let id = self.builtin_material_with(
            ROOF_CROWN_GUID,
            ROOF_CROWN_NAME,
            eps.category,
            eps.priority,
            eps.density,
            eps.lambda,
            surface.color,
            surface.cut_color,
            eps.trade,
            Some(eps.cut_fill),
            Some(ROOF_CROWN_SURFACE_GUID),
        )?;
        // Stifte und Kennwerte wie die Werksdämmung (Optik bleibt gleich)
        if let Some(m) = self.materials.get_mut(id) {
            (m.cut_fg, m.cut_bg, m.props) = (eps.cut_fg, eps.cut_bg, eps.props);
        }
        Some(id)
    }

    /// Geometrie des Flachdachs auf der Decke `floor`.
    pub fn flat_roof_over(&self, floor: ElementId) -> Option<FlatRoof> {
        self.flat_roof(self.parapet_above(self.run_of(floor)?)?)
    }

    /// Legt fehlende Dachaufbauten und Attikableche des Flachdachs an und
    /// entfernt überzählige (wie [`Model::sync_terraces`]): je Decke unter
    /// einer Aufkantung genau eins von jeder Art, auf der Ebene Flachdach.
    /// Bleibt die Aufkantung, bleiben Guid und Nummer. Beim ersten Mal
    /// kommen Werkstyp und Baustoffe dazu. Liefert die Zahl der angelegten
    /// und entfernten Bauteile.
    fn sync_flat_roofs(&mut self) -> (usize, usize) {
        self.sync_flat_roofs_in(None)
    }

    pub(super) fn sync_flat_roofs_in(&mut self, scope: Option<&[RunId]>) -> (usize, usize) {
        let want: Vec<(ElementId, StoreyId, u16)> = self
            .roof_floors_in(scope)
            .into_iter()
            .filter_map(|floor| {
                let ElementKind::Floor(f) = self.element(floor)?.kind else {
                    return None;
                };
                let ak = self.run(self.parapet_above(f.run)?)?.storey;
                Some((floor, ak, self.element(floor)?.seq))
            })
            .collect();
        if !want.is_empty() {
            self.ensure_roof_type();
            // Kronendämmung mit eigenem Baustoff (W9 an #60), auch beim
            // Lesen; ohne Dämmstreifen bleibt die Datei bytegleich
            if want.iter().any(|w| {
                matches!(self.element(w.0).map(|e| &e.kind),
                    Some(ElementKind::Floor(x)) if crate::model::crown_of(&x.terrace) > 0.0)
            }) {
                self.ensure_roof_crown_material();
            }
            if want.iter().any(|w| {
                matches!(self.element(w.0).map(|e| &e.kind),
                    Some(ElementKind::Floor(x)) if x.terrace.coping_mat.is_none())
            }) {
                self.ensure_coping_material();
            }
        }
        // Dachaufbauten (alle) und Bleche auf Decken unter einer Aufkantung
        // oder auf der Ebene Flachdach; die übrigen Bleche gehören der
        // Dachterrasse
        let have: Vec<(ElementId, ElementId, bool)> = self
            .elements
            .iter()
            .filter_map(|(id, e)| match e.kind {
                ElementKind::Roof { floor, .. } => Some((id, floor, true)),
                ElementKind::Coping { floor } if self.flat_roof_coping(id, floor) => {
                    Some((id, floor, false))
                }
                _ => None,
            })
            .filter(|&(_, floor, _)| self.floor_in_scope(floor, scope))
            .collect();
        let (mut added, mut removed) = (0, 0);
        let mut roofs: Vec<(ElementId, ElementId)> = Vec::new();
        let mut copings: Vec<ElementId> = Vec::new();
        for (id, floor, da) in have {
            let storey = want.iter().find(|w| w.0 == floor).map(|w| w.1);
            let kept = if da {
                roofs.iter().any(|x| x.0 == floor)
            } else {
                copings.contains(&floor)
            };
            let right = storey.is_some_and(|s| self.element(id).is_some_and(|e| e.storey == s));
            if right && !kept {
                if da {
                    roofs.push((floor, id));
                } else {
                    copings.push(floor);
                }
            } else {
                note!(self, Element, self.elements, id);
                self.elements.remove(id);
                self.touch();
                removed += 1;
            }
        }
        for &(floor, storey, seq) in &want {
            if !roofs.iter().any(|x| x.0 == floor) {
                // Neues Flachdach mit Gefälle nach Flachdachrichtlinie und
                // vorgeschlagenen Abläufen
                let drainage = Drainage {
                    slope: gefaelle::DEFAULT_SLOPE,
                    drains: self.proposed_drains(floor),
                };
                let kind = ElementKind::Roof { floor, drainage };
                let id = self.new_element(Category::Roof, storey, seq, kind);
                roofs.push((floor, id));
                self.touch();
                added += 1;
            }
            if !copings.contains(&floor) {
                self.new_element(Category::Coping, storey, seq, ElementKind::Coping { floor });
                self.touch();
                added += 1;
            }
        }
        // Ohne gültige Wahl gilt der Werkstyp, und er steht am Bauteil
        for (_, roof) in roofs {
            let t = self.flat_roof_type(roof);
            if self.element(roof).is_some_and(|e| e.layer_set != t) {
                note!(self, Element, self.elements, roof);
                if let Some(e) = self.elements.get_mut(roof) {
                    e.layer_set = t;
                }
                self.touch();
            }
        }
        // Regenfallrohre je Ablauf (Jörn 11.10.), mit eigenem Hinweis beim
        // Lesen
        self.sync_downpipes(scope);
        (added, removed)
    }

    /// Abgeleitete Merkmale von Dachaufbau und Blech des Flachdachs:
    /// Anschlusshöhe über der Dachhaut; Abwicklung und Zuschnitt des Blechs
    /// (auf die nächste Handelsbreite aufgerundet, fürs LV).
    pub(super) fn flat_roof_props(&self, id: ElementId) -> Vec<(String, PropValue)> {
        let Some(e) = self.element(id) else {
            return Vec::new();
        };
        match e.kind {
            ElementKind::Roof { floor, .. } => self
                .flat_roof_over(floor)
                .map(|r| vec![("Anschlusshöhe".into(), PropValue::Number(r.upstand()))])
                .unwrap_or_default(),
            ElementKind::Coping { floor } if self.flat_roof_coping(id, floor) => self
                .flat_roof_over(floor)
                .map(|r| {
                    let g = r.coping_girth();
                    vec![
                        ("Abwicklung".into(), PropValue::Number(g)),
                        (
                            "Zuschnitt".into(),
                            PropValue::Number(crate::terrace::coping_cut_width(g)),
                        ),
                    ]
                })
                .unwrap_or_default(),
            _ => Vec::new(),
        }
    }

    /// Hinweis zum Flachdach auf der Decke `floor` (Plan Flachdach P13):
    /// Die Aufkantung steht weniger als [`MIN_ROOF_EDGE`] über der Dachhaut.
    pub fn flat_roof_hint(&self, floor: ElementId) -> Option<String> {
        let r = self.flat_roof_over(floor)?;
        (r.upstand() < MIN_ROOF_EDGE - 1e-6).then(|| {
            format!(
                "Anschlusshöhe {} cm über der Dachhaut, Flachdachrichtlinie mindestens {} cm",
                crate::library::cm_de(r.upstand().max(0.0)),
                crate::library::cm_de(MIN_ROOF_EDGE)
            )
        })
    }

    /// Hinweis zur Attikadämmung innen auf der Decke `floor`: Ist der
    /// Umriss stellenweise schmaler als ihre doppelte Dicke, rückt die ganze
    /// Innenfläche weniger weit ein (Geometrie PR #43).
    pub fn flat_roof_inner_hint(&self, floor: ElementId) -> Option<String> {
        let r = self.flat_roof_over(floor)?;
        (r.inner > 0.0 && r.inner_depth < r.inner - 0.5).then(|| {
            format!(
                "Attikadämmung innen nur {} mm statt {} mm: Die Dachfläche ist stellenweise schmaler als die doppelte Dicke",
                r.inner_depth,
                r.inner
            )
        })
    }

    /// Wärmeschutz des Flachdachs `roof` mit der Geometrie `r` (G6): R0 an
    /// der dünnsten Stelle (m²K/W, Rsi 0,10 und Rse 0,04, Wärmestrom nach
    /// oben, samt Rohdecke, wenn ihr Typ λ hat) und λ der Gefälleschicht.
    /// `None`, wenn einer Schicht des Dachtyps λ fehlt.
    pub fn flat_roof_resistance(&self, roof: ElementId, r: &FlatRoof) -> Option<(f64, f64)> {
        let mut r0 = 0.10 + 0.04;
        let mut taper = None;
        let layers = self.flat_roof_layers(roof);
        let mut k = 0;
        for l in &layers {
            if l.function == LayerFunction::AirGap {
                r0 += 0.18;
                continue;
            }
            let lambda = self.material(l.material)?.lambda.filter(|v| *v > 0.0)?;
            if r.tapered == Some(k) {
                taper = Some(lambda);
            }
            k += 1;
            r0 += l.thickness / 1000.0 / lambda;
        }
        // Rohdecke darunter, soweit ihr Typ λ kennt
        let floor = match self.element(roof).map(|e| &e.kind) {
            Some(ElementKind::Roof { floor, .. }) => Some(*floor),
            _ => None,
        };
        let slab = floor
            .and_then(|f| self.element(f)?.layer_set)
            .and_then(|t| self.layer_set(t));
        if let Some(t) = slab {
            let rs: Option<f64> = t
                .layers
                .iter()
                .filter(|l| l.function != LayerFunction::AirGap)
                .map(|l| {
                    let lambda = self.material(l.material)?.lambda.filter(|v| *v > 0.0)?;
                    Some(l.thickness / 1000.0 / lambda)
                })
                .sum();
            r0 += rs.unwrap_or(0.0);
        }
        Some((r0, taper.unwrap_or(0.0)))
    }

    /// U-Wert des Flachdachs `roof` (W/(m²K)): ohne Gefälle 1/R, mit
    /// Gefälle das Flächenmittel nach DIN EN ISO 6946 Anhang C.
    pub fn flat_roof_u(&self, roof: ElementId, r: &FlatRoof) -> Option<f64> {
        let (r0, lambda) = self.flat_roof_resistance(roof, r)?;
        match &r.slope {
            Some(g) if lambda > 0.0 => Some(g.u_value(r0, lambda)),
            _ => Some(1.0 / r0),
        }
    }

    /// Hinweise zum Gefälle des Flachdachs auf der Decke `floor` (G6):
    /// Gefälle unter 2 %, Einzugsfläche je Ablauf über 150 m², Fließweg
    /// über 8 m und Mindestwärmeschutz an der dünnsten Stelle.
    pub fn flat_roof_slope_hints(&self, floor: ElementId) -> Vec<String> {
        let mut out = Vec::new();
        let (Some(roof), Some(r)) = (self.flat_roof_of(floor), self.flat_roof_over(floor)) else {
            return out;
        };
        let Some(g) = &r.slope else {
            return out;
        };
        let de = |v: f64, n: usize| format!("{v:.n$}").replace('.', ",");
        let pct = g.slope * 100.0;
        if pct < 2.0 - 1e-9 {
            out.push(format!(
                "Gefälle {} %, Flachdachrichtlinie mindestens 2 % (darunter Sonderkonstruktion)",
                de(pct, 1)
            ));
        }
        let limits = gefaelle::Limits::default();
        for (i, a) in g.catchments().iter().enumerate() {
            if *a > limits.max_area + 1e-6 {
                out.push(format!(
                    "Ablauf {} entwässert {} m², geplant höchstens {} m² je Ablauf",
                    i + 1,
                    de(a / 1e6, 1),
                    de(limits.max_area / 1e6, 0)
                ));
            }
        }
        let weg = g.wedge_max() / g.slope;
        if weg > limits.max_path + 1e-6 {
            out.push(format!(
                "Längster Fließweg {} m, geplant höchstens {} m: Keil {} cm dick",
                de(weg / 1000.0, 1),
                de(limits.max_path / 1000.0, 0),
                de(g.wedge_max() / 10.0, 1)
            ));
        }
        if let Some((r0, _)) = self.flat_roof_resistance(roof, &r) {
            if r0 < MIN_ROOF_R - 1e-9 {
                out.push(format!(
                    "Wärmeschutz am Ablauf R = {} m²K/W, DIN 4108-2 mindestens {}",
                    de(r0, 2),
                    de(MIN_ROOF_R, 1)
                ));
            }
        }
        out
    }

    /// Regel 48 für Dachaufbau oder Blech `id` des Flachdachs auf der Decke
    /// `floor`: genau dann, wenn darüber eine Aufkantung steht, auf deren
    /// Ebene; der Dachaufbau mit einem Typ der Art Flachdach.
    pub(super) fn check_flat_roof(&self, id: ElementId, floor: ElementId) -> Vec<String> {
        let Some(e) = self.element(id) else {
            return Vec::new();
        };
        let mut out = Vec::new();
        let ak = match self.element(floor).map(|f| &f.kind) {
            Some(ElementKind::Floor(f)) => self.parapet_above(f.run),
            _ => {
                out.push(format!("{}: Decke fehlt", e.number));
                return out;
            }
        };
        match ak.and_then(|r| self.run(r)) {
            None => out.push(format!("{}: keine Aufkantung über der Decke", e.number)),
            Some(r) if r.storey != e.storey => {
                out.push(format!("{}: nicht auf der Ebene Flachdach", e.number))
            }
            Some(_) => {}
        }
        if matches!(e.kind, ElementKind::Roof { .. })
            && (e.layer_set.is_none() || e.layer_set != self.flat_roof_type(id))
        {
            out.push(format!("{}: Typ des Flachdachs ungültig", e.number));
        }
        out
    }

    /// Nach dem Laden: Dachaufbauten und Bleche passend zu den
    /// Aufkantungen; liefert Hinweise, wenn welche ergänzt oder entfernt
    /// wurden.
    pub(crate) fn complete_flat_roofs(&mut self) -> Vec<String> {
        let strict = std::mem::replace(&mut self.strict, false);
        let roofs = |m: &Self| -> Vec<ElementId> {
            m.elements
                .iter()
                .filter(|(_, e)| matches!(e.kind, ElementKind::Roof { .. }))
                .map(|(id, _)| id)
                .collect()
        };
        let had = roofs(self);
        let rf = self.downpipe_ids();
        let (added, removed) = self.sync_flat_roofs();
        self.strict = strict;
        // Ein beim Laden ergänztes Dach bleibt eben wie in der Datei; die
        // 2 % gelten nur für neu angelegte Dächer
        for r in roofs(self).into_iter().filter(|r| !had.contains(r)) {
            if let Some(ElementKind::Roof { drainage, .. }) =
                self.elements.get_mut(r).map(|e| &mut e.kind)
            {
                *drainage = Drainage::default();
            }
        }
        let mut out = Vec::new();
        if added > 0 {
            out.push("Flachdach bzw. Attikablech ergänzt".to_string());
        }
        if removed > 0 {
            out.push("Flachdach bzw. Attikablech ohne Aufkantung entfernt".to_string());
        }
        let now = self.downpipe_ids();
        if now.iter().any(|d| !rf.contains(d)) {
            out.push("Regenfallrohre an den Abläufen ergänzt".to_string());
        }
        if rf.iter().any(|d| !now.contains(d)) {
            out.push("Regenfallrohre ohne Ablauf entfernt".to_string());
        }
        out
    }
}

/// Umriss `outline` (gegen den Uhrzeigersinn) um `d` nach innen versetzt,
/// Ecke für Ecke: Schnitt der versetzten Nachbarkanten, ohne Gehrungsgrenze.
/// Eine kurze Kante zwischen spitzen Ecken (kürzer als d · (cot α/2 +
/// cot β/2)) schrumpft auf einen Punkt: Ihre beiden Ecken liegen im Schnitt
/// der versetzten Nachbarkanten, die dort wie überall um `d` versetzt
/// bleiben. Nur wo das nicht aufgeht (Streifen schmaler als 2 d), rücken die
/// Ecken weniger weit ein und die Dämmung ist dünner als `d`. Gilt nur ein
/// einfacher Ring, dessen Ecken im Umriss liegen; schließt ein Zahn oder
/// Finger schmaler als 2 d auch so nicht (Review PR #43), rückt die ganze
/// Innenfläche so weit ein, wie es noch aufgeht. Liefert dazu die Dicke,
/// um die sie tatsächlich einrückt: `d`, sonst weniger (ganze mm).
fn inset_depth(outline: &[Vec3], d: f64) -> (Vec<Vec3>, f64) {
    let at = |d: f64| {
        inset_lines(outline, d)
            .filter(|v| inset_valid(outline, v))
            .or_else(|| Some(inset_clamped(outline, d)).filter(|v| inset_valid(outline, v)))
    };
    if let Some(v) = at(d) {
        return (v, d);
    }
    let (mut lo, mut hi) = (0.0, d);
    for _ in 0..30 {
        let m = (lo + hi) / 2.0;
        if at(m).is_some() {
            lo = m;
        } else {
            hi = m;
        }
    }
    // auf ganze mm abgerundet, damit Menge und Hinweis glatte Werte nennen
    [lo.floor(), lo]
        .into_iter()
        .find_map(|t| at(t).map(|v| (v, t)))
        .unwrap_or_else(|| (outline.to_vec(), 0.0))
}

/// Ist `pts` (Innenfläche zu `outline`, Ecke für Ecke) ein einfacher Ring,
/// dessen Ecken alle im Umriss liegen? Zusammengefallene Ecken einer
/// weggefallenen Kante zählen einmal.
fn inset_valid(outline: &[Vec3], pts: &[Vec3]) -> bool {
    let mut ring: Vec<Vec3> = Vec::with_capacity(pts.len());
    for p in pts {
        if ring.last().is_none_or(|q| (*p - *q).length() > 1e-6) {
            ring.push(*p);
        }
    }
    while ring.len() > 1 && (ring[0] - ring[ring.len() - 1]).length() <= 1e-6 {
        ring.pop();
    }
    let inside = |p: Vec3| {
        let n = outline.len();
        let mut c = false;
        for i in 0..n {
            let (a, b) = (outline[i], outline[(i + 1) % n]);
            if (a.y > p.y) != (b.y > p.y) && p.x < a.x + (p.y - a.y) * (b.x - a.x) / (b.y - a.y) {
                c = !c;
            }
        }
        c
    };
    sk_math::polygon::is_simple(&ring) && ring.iter().all(|p| inside(*p))
}

/// [`inset_depth`] über die versetzten Kanten: Kanten, die sich umkehrten,
/// fallen weg (die zuerst verschwindende zuerst), ihre Ecken liegen im
/// Schnitt der versetzten Nachbarn; `None`, wenn dabei weniger als drei
/// Kanten bleiben oder zwei Nachbarn parallel laufen.
fn inset_lines(outline: &[Vec3], d: f64) -> Option<Vec<Vec3>> {
    let n = outline.len();
    let dirs: Vec<Option<Vec3>> = (0..n)
        .map(|i| {
            let (a, b) = (outline[i], outline[(i + 1) % n]);
            let v = vec3(b.x - a.x, b.y - a.y, 0.0);
            (v.length() > 1e-9).then(|| v.normalized())
        })
        .collect();
    let mut alive: Vec<bool> = dirs.iter().map(Option::is_some).collect();
    // Ecke `i` liegt zwischen Kante i − 1 und Kante i
    let point = |alive: &[bool], i: usize| -> Option<Vec3> {
        let p = (1..=n).map(|k| (i + n - k) % n).find(|&k| alive[k])?;
        let q = (0..n).map(|k| (i + k) % n).find(|&k| alive[k])?;
        let (d0, d1) = (dirs[p]?, dirs[q]?);
        let (a, b) = (
            outline[p] + vec3(-d0.y, d0.x, 0.0) * d,
            outline[q] + vec3(-d1.y, d1.x, 0.0) * d,
        );
        let cr = d0.x * d1.y - d0.y * d1.x;
        if cr.abs() < 1e-9 {
            // gerade weiter: Lot von der Ecke; parallel gegenläufig: nicht lösbar
            return (d0.dot(d1) > 0.0).then(|| outline[i] + vec3(-d1.y, d1.x, 0.0) * d);
        }
        let w = b - a;
        Some(a + d0 * ((w.x * d1.y - w.y * d1.x) / cr))
    };
    loop {
        let pts: Vec<Vec3> = (0..n).map(|i| point(&alive, i)).collect::<Option<_>>()?;
        // umgekehrte Kante mit dem stärksten Überschlag je Länge zuerst
        let worst = (0..n)
            .filter(|&k| alive[k])
            .map(|k| {
                let e = dirs[k].unwrap_or_default();
                let l = (outline[(k + 1) % n] - outline[k]).length();
                (k, (pts[(k + 1) % n] - pts[k]).dot(e) / l)
            })
            .filter(|&(_, r)| r < -1e-9)
            .min_by(|x, y| x.1.total_cmp(&y.1));
        match worst {
            None => return Some(pts),
            Some((k, _)) => alive[k] = false,
        }
        if alive.iter().filter(|a| **a).count() < 3 {
            return None;
        }
    }
}

/// Rückfall für [`inset_depth`]: Einrücktiefe je Ecke geklemmt, bis keine
/// versetzte Kante gegen ihre Kante läuft.
fn inset_clamped(outline: &[Vec3], d: f64) -> Vec<Vec3> {
    let n = outline.len();
    let dir = |a: Vec3, b: Vec3| {
        let v = vec3(b.x - a.x, b.y - a.y, 0.0);
        (v.length() > 1e-9).then(|| v.normalized())
    };
    let left = |v: Vec3| vec3(-v.y, v.x, 0.0);
    // Weg der Ecke je mm Dicke
    let miter: Vec<Vec3> = (0..n)
        .map(|i| {
            let (a, b, c) = (outline[(i + n - 1) % n], outline[i], outline[(i + 1) % n]);
            match (dir(a, b), dir(b, c)) {
                (Some(d0), Some(d1)) => {
                    let cr = d0.x * d1.y - d0.y * d1.x;
                    if cr.abs() < 1e-9 {
                        return left(d1);
                    }
                    let w = left(d1) - left(d0);
                    left(d0) + d0 * ((w.x * d1.y - w.y * d1.x) / cr)
                }
                (Some(v), None) | (None, Some(v)) => left(v),
                (None, None) => vec3(0.0, 0.0, 0.0),
            }
        })
        .collect();
    // Einrücktiefe je Ecke: höchstens so weit, dass keine versetzte Kante
    // gegen ihre Kante läuft (Länge l + b·t[j] − a·t[i] ≥ 0)
    let mut t = vec![d; n];
    for _ in 0..2 * n {
        let mut changed = false;
        for i in 0..n {
            let j = (i + 1) % n;
            let Some(e) = dir(outline[i], outline[j]) else {
                continue;
            };
            let l = (outline[j] - outline[i]).dot(e);
            let (a, b) = (miter[i].dot(e), miter[j].dot(e));
            let len = |t: &[f64]| l + b * t[j] - a * t[i];
            if len(&t) >= -1e-9 {
                continue;
            }
            changed = true;
            if a > 0.0 && b < 0.0 {
                let s = l / (a - b);
                t[i] = t[i].min(s);
                t[j] = t[j].min(s);
            }
            if len(&t) < -1e-9 {
                if a > 0.0 {
                    t[i] = ((l + b * t[j]) / a).max(0.0);
                } else {
                    t[j] = ((l - a * t[i]) / -b).max(0.0);
                }
            }
        }
        if !changed {
            break;
        }
    }
    (0..n).map(|i| outline[i] + miter[i] * t[i]).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::txn::Direction;
    use sk_math::vec3;

    fn inset_of(outline: &[Vec3], d: f64) -> Vec<Vec3> {
        inset_depth(outline, d).0
    }

    /// Prüfhaus 10 × 8 m mit EG und OG im Wandtyp `typ` (Guid).
    fn haus(typ: Guid) -> (Model, RunId) {
        let mut m = Model::with_seed(12);
        let b = m.add_building(2);
        let pts = [
            vec3(0.0, 0.0, 0.0),
            vec3(0.0, 8000.0, 0.0),
            vec3(10000.0, 8000.0, 0.0),
            vec3(10000.0, 0.0, 0.0),
        ];
        let set = m.type_by_guid(typ).unwrap();
        let eg = m.ground_of(Some(b)).unwrap();
        let eg = m
            .add_wall_run(&pts, true, RefSide::Left, eg, set, Category::ExteriorWall)
            .unwrap();
        let og = m.runs_above(eg)[0];
        (m, og)
    }

    fn schritt(m: &mut Model, f: impl FnOnce(&mut Model) -> bool) -> Txn {
        m.begin("Flachdach");
        assert!(f(m));
        m.commit().expect("Schritt ändert etwas")
    }

    fn ak_runs(m: &Model) -> Vec<RunId> {
        m.runs()
            .iter()
            .filter(|(id, _)| m.category_of(*id) == Some(Category::Parapet))
            .map(|(id, _)| id)
            .collect()
    }

    /// D1/D2: Flachdach an und aus, rückgängig als ein Schritt.
    #[test]
    fn ebene_flachdach_mit_aufkantung() {
        let (mut m, og) = haus(EXTERIOR_TYPE_GUID);
        let og_storey = m.run(og).unwrap().storey;
        let vorher = crate::szo::write(&m);
        assert!(m.roof_level(og_storey).is_none());
        let t = schritt(&mut m, |m| m.set_flat_roof(og_storey, true));
        let fd = m.roof_level(og_storey).expect("FD");
        let st = m.storey(fd).unwrap().clone();
        assert_eq!((st.name.as_str(), st.short.as_str()), ("Flachdach", "FD"));
        assert_eq!(st.kind, LevelKind::Roof);
        assert_eq!(st.elevation, m.storey(og_storey).unwrap().top());
        assert_eq!(st.height, ROOF_UPSTAND);
        assert_eq!(m.level_above(og_storey), Some(fd));
        // Aufkantung: gekoppelt an die OG-Wand, gleicher Typ, keine Decke
        let ak = ak_runs(&m);
        assert_eq!(ak.len(), 1);
        assert_eq!(m.runs_above(og), ak);
        let r = m.run(ak[0]).unwrap().clone();
        assert_eq!(r.storey, fd);
        assert!(r.closed && r.segments.len() == 4);
        let og_set = m.element(m.wall_at(og, 0).unwrap()).unwrap().layer_set;
        for w in &r.segments {
            let e = m.element(*w).unwrap();
            assert!(e.number.starts_with("AK-"), "{}", e.number);
            assert_eq!(e.layer_set, og_set);
            assert_eq!(m.stack_offset(*w), Some((0.0, true)));
        }
        assert!(m.floor_of(ak[0]).is_none());
        assert!(m.check().is_empty(), "{:?}", m.check());
        // Höhe der Ebene = Höhe der Aufkantung
        let c = m.chain(ak[0]).unwrap();
        assert_eq!((c.base, c.height), (st.elevation, ROOF_UPSTAND));
        schritt(&mut m, |m| m.set_storey_height(fd, 800.0));
        assert_eq!(m.chain(ak[0]).unwrap().height, 800.0);
        m.begin("zu hoch");
        assert!(!m.set_storey_height(fd, MAX_ROOF_UPSTAND + 1.0));
        assert!(!m.set_storey_height(fd, MIN_ROOF_UPSTAND - 1.0));
        assert!(m.commit().is_none());
        // Aufkantung bleibt bündig und gekoppelt, löschen abgelehnt
        let w = r.segments[0];
        m.begin("ziehen");
        assert!(m.set_offset(w, 300.0).is_none());
        assert!(!m.set_linked(w, false));
        assert!(m.commit().is_none());
        assert!(matches!(m.can_delete(w), Err(Refusal::Derived { .. })));
        assert!(refusal_text(&m, w, &m.can_delete(w).unwrap_err()).contains("Paneel Geschosse"));
        // Aus: Ebene und Aufkantung weg; rückgängig bringt beides
        let aus = schritt(&mut m, |m| m.set_flat_roof(og_storey, false));
        assert!(m.roof_level(og_storey).is_none() && ak_runs(&m).is_empty());
        m.apply(&aus, Direction::Undo);
        assert_eq!(ak_runs(&m), ak);
        assert_eq!(m.roof_level(og_storey), Some(fd));
        m.apply(&aus, Direction::Redo);
        assert!(ak_runs(&m).is_empty());
        m.apply(&aus, Direction::Undo);
        // Rückgängig bis vor das Flachdach: die Datei ist wie vorher, nur
        // die Nummern der Aufkantung bleiben vergeben
        m.apply(&t, Direction::Undo);
        let nachher = crate::szo::write(&m);
        let soll = vorher.replacen("\n[building]", " next=DA:1,AB:1,AK:4,RF:4\n[building]", 1);
        assert!(nachher == soll, "Datei nach Rückgängig anders");
    }

    /// D1: Datei mit Flachdach (`kind=roof`, `cat=parapet`) liest sich
    /// gleich zurück; ohne Flachdach steht keins der Wörter darin.
    #[test]
    fn datei_mit_flachdach() {
        let (mut m, og) = haus(EXTERIOR_TYPE_GUID);
        let og_storey = m.run(og).unwrap().storey;
        let ohne = crate::szo::write(&m);
        assert!(!ohne.contains("kind=roof") && !ohne.contains("cat=parapet"));
        schritt(&mut m, |m| m.set_flat_roof(og_storey, true));
        let text = crate::szo::write(&m);
        assert!(text.contains("kind=roof"), "{text}");
        assert!(text.contains("cat=parapet"), "{text}");
        assert!(text.contains("\n[roof] "), "{text}");
        assert!(text.contains("cat=roof"), "{text}");
        let back = crate::szo::read(&text, GuidGen::with_seed(5)).unwrap();
        assert!(back.hints.is_empty(), "{:?}", back.hints);
        assert_eq!(crate::szo::write(&back.model), text);
        assert!(back.model.check().is_empty());
        // Datei von vor dem Dachaufbau (ohne [roof]): das ergänzte Dach
        // bleibt eben, ohne Abläufe
        let alt: String = text
            .lines()
            .filter(|l| !l.starts_with("[roof] ") && !l.starts_with("[downpipe] "))
            .map(|l| format!("{l}\n"))
            .collect();
        let back = crate::szo::read(&alt, GuidGen::with_seed(5)).unwrap();
        assert!(
            back.hints.iter().any(|h| h.contains("ergänzt")),
            "{:?}",
            back.hints
        );
        let da = (back.model.elements.iter())
            .find(|(_, e)| matches!(e.kind, ElementKind::Roof { .. }))
            .unwrap()
            .0;
        assert_eq!(back.model.drainage_of_roof(da), Some(&Drainage::default()));
    }

    /// Abläufe hinzufügen und löschen (Jörn 11.10.): je ein Schritt im
    /// Verlauf; der letzte Ablauf bleibt.
    #[test]
    fn ablauf_hinzufuegen_und_loeschen() {
        let (mut m, og) = haus(EXTERIOR_TYPE_GUID);
        let og_storey = m.run(og).unwrap().storey;
        schritt(&mut m, |m| m.set_flat_roof(og_storey, true));
        let floor = m.floor_of(og).unwrap();
        let da = m.flat_roof_of(floor).unwrap();
        let vorher = m.drainage_of_roof(da).unwrap().clone();
        let n = vorher.drains.len();
        assert!(n >= 2, "{vorher:?}");
        let dazu = schritt(&mut m, |m| m.add_roof_drain(da));
        let mit = m.drainage_of_roof(da).unwrap().clone();
        assert_eq!(mit.drains.len(), n + 1);
        assert_eq!(mit.drains[..n], vorher.drains[..]);
        assert!(m.flat_roof_over(floor).unwrap().slope.is_some());
        let weg = schritt(&mut m, |m| m.remove_roof_drain(da, 0));
        assert_eq!(m.drainage_of_roof(da).unwrap().drains, mit.drains[1..]);
        m.apply(&weg, Direction::Undo);
        m.apply(&dazu, Direction::Undo);
        assert_eq!(m.drainage_of_roof(da), Some(&vorher));
        // bis auf einen löschen, den letzten nicht
        for _ in 1..n {
            schritt(&mut m, |m| m.remove_roof_drain(da, 0));
        }
        m.begin("letzter");
        assert!(!m.remove_roof_drain(da, 0));
        assert!(!m.remove_roof_drain(da, 5));
        m.rollback();
        assert_eq!(m.drainage_of_roof(da).unwrap().drains.len(), 1);
        // ohne Gefälle kein weiterer
        schritt(&mut m, |m| m.set_roof_slope(da, 0.0));
        m.begin("eben");
        assert!(!m.add_roof_drain(da));
        m.rollback();
    }

    /// Regel 114: Werte außerhalb des Bereichs an Flachdach und Decke
    /// sperren das Projekt nicht; es öffnet mit je einem Hinweis, und die
    /// Vorgaben gelten und werden gespeichert.
    #[test]
    fn datei_mit_werten_ausserhalb() {
        let (mut m, og) = haus(EXTERIOR_TYPE_GUID);
        let og_storey = m.run(og).unwrap().storey;
        schritt(&mut m, |m| m.set_flat_roof(og_storey, true));
        let text = crate::szo::write(&m);
        let feld = |l: &str, k: &str| {
            l.split(' ')
                .find_map(|t| t.strip_prefix(&format!("{k}=")))
                .map(str::to_string)
        };
        let dach = text.lines().find(|l| l.starts_with("[roof] ")).unwrap();
        let decke = feld(dach, "floor").unwrap();
        let kaputt: String = text
            .lines()
            .map(|l| {
                if l.starts_with("[roof] ") {
                    let l: Vec<&str> = l
                        .split(' ')
                        .filter(|t| !t.starts_with("slope=") && !t.starts_with("drains="))
                        .collect();
                    format!("{} slope=150 drains=\"x\"\n", l.join(" "))
                } else if l.starts_with("[floor] ") && feld(l, "guid") == Some(decke.clone()) {
                    format!("{l} crown=-5 inner=99999\n")
                } else {
                    format!("{l}\n")
                }
            })
            .collect();
        let back = crate::szo::read(&kaputt, GuidGen::with_seed(5)).unwrap();
        // ohne lesbare Abläufe gelten die vorgeschlagenen, die
        // Regenfallrohre bleiben (Review W12 an #68)
        assert_eq!(back.hints.len(), 4, "{:?}", back.hints);
        let m = back.model;
        let (da, floor) = m
            .elements
            .iter()
            .find_map(|(id, e)| match e.kind {
                ElementKind::Roof { floor, .. } => Some((id, floor)),
                _ => None,
            })
            .unwrap();
        assert_eq!(m.drainage_of_roof(da), Some(&Drainage::default()));
        let ElementKind::Floor(f) = &m.element(floor).unwrap().kind else {
            panic!("keine Decke");
        };
        assert_eq!(f.terrace.crown, crate::model::TERRACE_CROWN);
        assert_eq!(f.terrace.inner, crate::model::ROOF_INNER);
        let neu = crate::szo::write(&m);
        for k in ["slope=", "drains=", "crown=", "inner="] {
            assert!(!neu.contains(k), "{k} in {neu}");
        }
    }

    /// Attikadämmung innen an einer spitzen Ecke (20°): die Innenfläche
    /// liegt überall genau um die Dicke vor der Aufkantung, auch wo die
    /// Wandflächen jenseits ihrer Gehrungsgrenze kappen würden.
    #[test]
    fn attikadaemmung_innen_spitze_ecke() {
        let a = 20f64.to_radians();
        let pts = [
            vec3(0.0, 0.0, 0.0),
            vec3(14000.0 * a.cos(), 14000.0 * a.sin(), 0.0),
            vec3(14000.0, 0.0, 0.0),
        ];
        let mut m = Model::with_seed(12);
        let b = m.add_building(2);
        let set = m.type_by_guid(EXTERIOR_TYPE_GUID).unwrap();
        let eg = m.ground_of(Some(b)).unwrap();
        let eg = m
            .add_wall_run(&pts, true, RefSide::Left, eg, set, Category::ExteriorWall)
            .unwrap();
        let og = m.runs_above(eg)[0];
        let st = m.run(og).unwrap().storey;
        schritt(&mut m, |m| m.set_flat_roof(st, true));
        let floor = m.floor_of(og).unwrap();
        for d in [20.0, 60.0, 200.0] {
            schritt(&mut m, |m| m.set_floor_inner(floor, d));
            let r = m.flat_roof_over(floor).unwrap();
            let (o, i) = (&r.outline, &r.inset);
            assert_eq!(o.len(), i.len());
            let n = o.len();
            for k in 0..n {
                let e = (o[(k + 1) % n] - o[k]).normalized();
                let nrm = vec3(-e.y, e.x, 0.0);
                for p in [i[k], i[(k + 1) % n]] {
                    assert!(((p - o[k]).dot(nrm) - d).abs() < 1e-6, "{d}: {p:?}");
                }
            }
        }
    }

    /// An einer kurzen Kante zwischen spitzen Ecken (Geometrie, Review
    /// PR #42): Kante 600 mm zwischen einer Ecke von 120° und einem Zahn von
    /// 30°; 200 mm Dämmung würden 0,86 m Kante verbrauchen. Die versetzte
    /// Kante schrumpft auf einen Punkt statt sich umzukehren, und das Band
    /// zwischen Umriss und Innenfläche hat genau die Fläche Umriss −
    /// Innenfläche, ohne Überschlag.
    #[test]
    fn attikadaemmung_innen_kurze_kante() {
        let (a, b) = (30f64.to_radians(), vec3(600.0, 0.0, 0.0));
        let e = 120f64.to_radians();
        let outline = [
            vec3(0.0, 0.0, 0.0),
            b,
            b + vec3(-a.cos(), a.sin(), 0.0) * 700.0,
            vec3(-500.0, 3500.0, 0.0),
            vec3(e.cos(), e.sin(), 0.0) * 3000.0,
        ];
        assert!(sk_math::polygon::signed_area(&outline) > 0.0);
        for d in [20.0, 60.0] {
            let inset = inset_of(&outline, d);
            let n = outline.len();
            let mut band = 0.0;
            for k in 0..n {
                let j = (k + 1) % n;
                let e = outline[j] - outline[k];
                assert!(
                    (inset[j] - inset[k]).dot(e) >= -1e-6,
                    "{d}: Kante {k} umgekehrt"
                );
                band +=
                    sk_math::polygon::signed_area(&[outline[k], outline[j], inset[j], inset[k]]);
            }
            let soll =
                sk_math::polygon::signed_area(&outline) - sk_math::polygon::signed_area(&inset);
            assert!(
                (band - soll).abs() < 1e-6 * soll,
                "{d}: {band} statt {soll}"
            );
            // die langen Kanten bleiben um die volle Dicke versetzt
            for k in [2, 3] {
                let e = (outline[k + 1] - outline[k]).normalized();
                let nrm = vec3(-e.y, e.x, 0.0);
                assert!(
                    ((inset[k + 1] - outline[k]).dot(nrm) - d).abs() < 1e-6,
                    "{d}"
                );
            }
        }
        // Giebelspitze: die kurze Kante oben (100 mm) fällt bei 200 mm weg
        // (Punkt), die Nachbarkanten bleiben auf ganzer Länge um 200 versetzt
        let giebel = [
            vec3(0.0, 0.0, 0.0),
            vec3(4000.0, 0.0, 0.0),
            vec3(4000.0, 1000.0, 0.0),
            vec3(2050.0, 5000.0, 0.0),
            vec3(1950.0, 5000.0, 0.0),
            vec3(0.0, 1000.0, 0.0),
        ];
        let inset = inset_of(&giebel, 200.0);
        assert!((inset[4] - inset[3]).length() < 1e-6, "{inset:?}");
        for k in 0..giebel.len() {
            let j = (k + 1) % giebel.len();
            if k == 3 {
                continue;
            }
            let e = (giebel[j] - giebel[k]).normalized();
            let nrm = vec3(-e.y, e.x, 0.0);
            for p in [inset[k], inset[j]] {
                assert!(
                    ((p - giebel[k]).dot(nrm) - 200.0).abs() < 1e-6,
                    "{k}: {p:?}"
                );
            }
        }
        // ab etwa 140 mm schließt sich der Zahn zwischen Kante 4 und 1: bei
        // 200 mm läge der Schnitt der Nachbarn 27 mm jenseits von Kante 0, die
        // Klemmung jenseits von Kante 4 (Review PR #43); die Innenfläche
        // bleibt im Umriss und rückt weniger weit ein
        assert!(!inset_valid(
            &outline,
            &inset_lines(&outline, 200.0).unwrap()
        ));
        assert!(inset_valid(&outline, &inset_of(&outline, 200.0)));
        // Streifen schmaler als 2 d: Rückfall, nichts kehrt sich um
        let streifen = [
            vec3(0.0, 0.0, 0.0),
            vec3(5000.0, 0.0, 0.0),
            vec3(5000.0, 300.0, 0.0),
            vec3(0.0, 300.0, 0.0),
        ];
        let i = inset_of(&streifen, 200.0);
        for k in 0..4 {
            let e = streifen[(k + 1) % 4] - streifen[k];
            assert!((i[(k + 1) % 4] - i[k]).dot(e) >= -1e-6, "{i:?}");
        }
        // Finger 3 m lang, an der Wurzel 300, an der Spitze 310 breit: die
        // versetzten Seiten schnitten sich erst 27 m jenseits der Spitze
        // (Review PR #43); die Innenfläche bleibt im Umriss
        let finger = [
            vec3(0.0, 0.0, 0.0),
            vec3(6000.0, 0.0, 0.0),
            vec3(6000.0, 3000.0, 0.0),
            vec3(3150.0, 3000.0, 0.0),
            vec3(3155.0, 6000.0, 0.0),
            vec3(2845.0, 6000.0, 0.0),
            vec3(2850.0, 3000.0, 0.0),
            vec3(0.0, 3000.0, 0.0),
        ];
        assert!(inset_lines(&finger, 200.0).is_some_and(|v| v.iter().any(|p| p.y > 20_000.0)));
        let (i, t) = inset_depth(&finger, 200.0);
        assert!(inset_valid(&finger, &i), "{i:?}");
        // die ganze Innenfläche rückt weniger weit ein, auf ganze mm
        assert!(t < 200.0 && t > 100.0 && t.fract() == 0.0, "{t}");
        assert_eq!(inset_depth(&finger, 60.0).1, 60.0);
        assert_eq!(inset_depth(&outline, 60.0).1, 60.0);
        assert!(inset_depth(&outline, 200.0).1 < 200.0);
    }

    /// Flachdach mit einem Finger, innen 300 mm breit: 200 mm Attikadämmung
    /// innen gehen dort nicht auf. Die Menge nennt die tatsächliche Dicke,
    /// und das Paneel weist darauf hin; 60 mm gehen auf, ohne Hinweis.
    #[test]
    fn attikadaemmung_innen_engstelle() {
        let pts = [
            vec3(0.0, 0.0, 0.0),
            vec3(10000.0, 0.0, 0.0),
            vec3(10000.0, 6000.0, 0.0),
            vec3(5150.0, 6000.0, 0.0),
            vec3(5150.0, 9000.0, 0.0),
            vec3(4850.0, 9000.0, 0.0),
            vec3(4850.0, 6000.0, 0.0),
            vec3(0.0, 6000.0, 0.0),
        ];
        let mut m = Model::with_seed(12);
        let b = m.add_building(2);
        let set = m.type_by_guid(EXTERIOR_TYPE_GUID).unwrap();
        let eg = m.ground_of(Some(b)).unwrap();
        let eg = m
            .add_wall_run(&pts, true, RefSide::Left, eg, set, Category::ExteriorWall)
            .unwrap();
        let og = m.runs_above(eg)[0];
        let st = m.run(og).unwrap().storey;
        schritt(&mut m, |m| m.set_flat_roof(st, true));
        let floor = m.floor_of(og).unwrap();
        let (roof, blech) = da_ab(&m, og);
        // Breite des Fingers innen an der Aufkantung
        let r = m.flat_roof_over(floor).unwrap();
        let xs: Vec<f64> = r
            .outline
            .iter()
            .filter(|p| p.y > 7000.0)
            .map(|p| p.x)
            .collect();
        let breite = xs.iter().copied().fold(f64::MIN, f64::max)
            - xs.iter().copied().fold(f64::MAX, f64::min);
        assert!(breite > 120.0 && breite < 400.0, "{breite}");
        let hinweis = |m: &Model| {
            m.warnings(roof)
                .into_iter()
                .find(|w| w.starts_with("Attikadämmung innen"))
        };
        for (d, eng) in [(200.0, true), (60.0, false)] {
            schritt(&mut m, |m| m.set_floor_inner(floor, d));
            let r = m.flat_roof_over(floor).unwrap();
            assert_eq!(r.inner, d);
            assert_eq!(r.inner_depth < d, eng, "{d}: {}", r.inner_depth);
            let w = hinweis(&m);
            assert_eq!(w.is_some(), eng, "{d}: {w:?}");
            if let Some(w) = w {
                assert!(
                    w.contains(&format!("nur {} mm statt 200 mm", r.inner_depth)),
                    "{w}"
                );
            }
            let q = crate::qto::coping_qto(&m, blech).unwrap();
            assert_eq!(q.insulation[0].thickness, r.inner_depth);
            // Schichtzeile (Bauteilliste, Kosten) mit derselben Dicke
            // (Review PR #47)
            let s = crate::qto::schedule(&m);
            let zeile = s
                .layer_rows(&m)
                .into_iter()
                .find(|(_, l)| l.element == blech && l.layer == 1)
                .unwrap()
                .1;
            assert_eq!(zeile.thickness, r.inner_depth);
            // das Blech deckt Aufkantung und die tatsächliche Dämmung, der
            // innere Schenkel liegt an ihr an (Geometrie PR #47)
            let (i, o) = r.coping_legs();
            let girth = crate::terrace::coping_girth_legs(r.width + r.inner_depth, i, o);
            assert!((q.girth - girth).abs() < 1e-9, "{d}: {} {girth}", q.girth);
        }
    }

    /// Bauteilsumme des Attikablechs mit der Dämmung darunter (BIM R1 B1):
    /// Volumen im Mengenfenster = Summe seiner Schichtzeilen, das Blech
    /// selbst (Schicht 0) zählt nur einmal.
    #[test]
    fn attikablech_summe_mit_daemmung() {
        let (mut m, og) = haus(EXTERIOR_TYPE_GUID);
        let st = m.run(og).unwrap().storey;
        schritt(&mut m, |m| m.set_flat_roof(st, true));
        let (_, blech) = da_ab(&m, og);
        let q = crate::qto::coping_qto(&m, blech).unwrap();
        let daemmung: f64 = q.insulation.iter().map(|i| i.volume).sum();
        assert!(daemmung > 0.0);
        let s = crate::qto::schedule(&m);
        let row = s
            .buildings
            .iter()
            .flat_map(|b| &b.storeys)
            .flat_map(|st| &st.groups)
            .flat_map(|g| &g.rows)
            .find(|r| r.element == blech)
            .unwrap();
        let v = row.q.as_ref().unwrap().volume();
        assert!((v - (q.volume + daemmung)).abs() < 1e-6, "{v}");
        let zeilen: Vec<_> = s
            .layer_rows(&m)
            .into_iter()
            .filter(|(_, l)| l.element == blech)
            .map(|(_, l)| l)
            .collect();
        let blechzeile = zeilen.iter().find(|l| l.layer == 0).unwrap();
        assert!((blechzeile.volume - q.volume).abs() < 1e-6);
        let summe: f64 = zeilen.iter().map(|l| l.volume).sum();
        assert!((summe - v).abs() < 1e-6, "{summe} {v}");
    }

    /// W9 an #60: Die Kronendämmung am Flachdach trägt einen eigenen
    /// Baustoff, innen bleibt die Dämmung des Aufbaus; Darstellung wie die
    /// Werksdämmung, Mengen unverändert. Ältere Dateien ohne ihn bekommen
    /// ihn beim Lesen (nur mit Dämmstreifen auf der Krone, sonst bytegleich).
    #[test]
    fn kronendaemmung_eigener_baustoff() {
        let (mut m, og) = haus(EXTERIOR_TYPE_GUID);
        let st = m.run(og).unwrap().storey;
        schritt(&mut m, |m| m.set_flat_roof(st, true));
        let (_, blech) = da_ab(&m, og);
        let guid = |m: &Model, id: MaterialId| m.material(id).unwrap().guid;
        let l = m.element_layers(blech);
        assert_eq!(l.len(), 3);
        assert_eq!(guid(&m, l[1].material), ROOF_INSULATION_GUID);
        assert_eq!(guid(&m, l[2].material), ROOF_CROWN_GUID);
        let q = crate::qto::coping_qto(&m, blech).unwrap();
        assert_eq!(q.insulation.len(), 2);
        assert_eq!(q.insulation[0].material, l[1].material);
        assert_eq!(q.insulation[1].material, l[2].material);
        // Optik wie die Werksdämmung
        let (eps, krone) = (
            m.material(l[1].material).unwrap(),
            m.material(l[2].material).unwrap(),
        );
        assert_eq!(krone.name, ROOF_CROWN_NAME);
        assert_eq!(
            (
                krone.cut_fill,
                krone.cut_fg,
                krone.cut_bg,
                krone.density,
                krone.lambda,
                krone.trade
            ),
            (
                eps.cut_fill,
                eps.cut_fg,
                eps.cut_bg,
                eps.density,
                eps.lambda,
                eps.trade
            )
        );
        let (se, sk) = (
            m.attr.surface(eps.surface).unwrap(),
            m.attr.surface(krone.surface).unwrap(),
        );
        assert_eq!((se.color, se.cut_color), (sk.color, sk.cut_color));
        assert!(!m.can_remove_material(l[2].material));
        // Datei: mit ihm bytegleich; ältere ohne ihn bekommen ihn beim
        // Lesen und rechnen dieselben Mengen
        let text = crate::szo::write(&m);
        let alt: String = text
            .lines()
            .filter(|z| !z.contains("Attikakrone"))
            .map(|z| format!("{z}\n"))
            .collect();
        assert_ne!(alt, text);
        for t in [&text, &alt] {
            let r = crate::szo::read(t, GuidGen::with_seed(7)).unwrap().model;
            assert_eq!(crate::szo::write(&r), text);
            let g = m.element(blech).unwrap().guid;
            let b = r.elements().iter().find(|(_, e)| e.guid == g).unwrap().0;
            let k = crate::qto::coping_qto(&r, b).unwrap();
            for (x, y) in k.insulation.iter().zip(&q.insulation) {
                assert_eq!(
                    (x.thickness, x.area, x.volume),
                    (y.thickness, y.area, y.volume)
                );
            }
        }
        // ohne Dämmstreifen auf der Krone: ältere Datei bleibt bytegleich
        let floor = m.floor_of(og).unwrap();
        schritt(&mut m, |m| m.set_floor_crown(floor, 0.0));
        assert_eq!(m.element_layers(blech).len(), 2);
        let ohne: String = crate::szo::write(&m)
            .lines()
            .filter(|z| !z.contains("Attikakrone"))
            .map(|z| format!("{z}\n"))
            .collect();
        let r = crate::szo::read(&ohne, GuidGen::with_seed(7))
            .unwrap()
            .model;
        assert!(r.material_by_guid(ROOF_CROWN_GUID).is_none());
        assert_eq!(crate::szo::write(&r), ohne);
    }

    /// Attikadämmung innen mit Gefälle: Menge exakt (gegen feine Summe)
    /// und gleich, wenn der Umriss einen Punkt in der Flucht dazubekommt
    /// (Teilen einer Wand, Review Geometrie #61).
    #[test]
    fn attikadaemmung_innen_mit_keil_exakt() {
        let (mut m, og) = haus(EXTERIOR_TYPE_GUID);
        let st = m.run(og).unwrap().storey;
        schritt(&mut m, |m| m.set_flat_roof(st, true));
        let floor = m.floor_of(og).unwrap();
        let r = m.flat_roof_over(floor).unwrap();
        assert!(r.slope.is_some());
        let (face, vol) = r.inner_qto();
        // feine Summe: Mittelpunktregel auf 400 × 400 je Stück
        let (fein_a, fein_v) = r.inner_qto_fein(400);
        assert!((face - fein_a).abs() < face * 1e-4, "{face} {fein_a}");
        assert!((vol - fein_v).abs() < vol * 1e-4, "{vol} {fein_v}");
        // Punkt in der Flucht der Nordwand im EG (der Stapel folgt)
        let eg = m.run_below(og).unwrap();
        let mut pts = m.run(eg).unwrap().points.clone();
        pts.insert(2, vec3(3000.0, 8000.0, 0.0));
        schritt(&mut m, |m| m.set_run_points(eg, &pts).is_some());
        let r2 = m.flat_roof_over(floor).unwrap();
        // Mit PR #61 fällt der Fugenpunkt aus dem Umriss (simplified); die
        // Menge muss in beiden Fällen gleich bleiben
        assert!(r2.outline.len() >= r.outline.len());
        let (face2, vol2) = r2.inner_qto();
        assert!((face2 - face).abs() < 1e-3, "{face} {face2}");
        assert!((vol2 - vol).abs() < 1e-1, "{vol} {vol2}");
    }

    /// Dachaufbau und Blech des Flachdachs auf der OG-Decke unter `og`.
    fn da_ab(m: &Model, og: RunId) -> (ElementId, ElementId) {
        let floor = m.floor_of(og).unwrap();
        (m.flat_roof_of(floor).unwrap(), m.coping_of(floor).unwrap())
    }

    /// Neue Flachdächer haben 2 % Gefälle (Jörn 11.10.); Tests der
    /// waagerechten Geometrie stellen es als Schritt auf 0.
    fn waagerecht(m: &mut Model, og: RunId) -> Txn {
        let (da, _) = da_ab(m, og);
        schritt(m, |m| m.set_roof_slope(da, 0.0))
    }

    /// D3, D4: Mit der Ebene entstehen Dachaufbau DA (Werkstyp „Flachdach
    /// 21,5“) und Attikablech AB auf der Ebene FD; Richtwerte des Plans für
    /// AW-31,5 im Prüfhaus 10 × 8 m.
    #[test]
    fn dachaufbau_und_blech() {
        let (mut m, og) = haus(EXTERIOR_TYPE_GUID);
        let og_storey = m.run(og).unwrap().storey;
        let vorher = crate::szo::write(&m);
        assert!(m.flat_roof_of(m.floor_of(og).unwrap()).is_none());
        let tx = schritt(&mut m, |m| m.set_flat_roof(og_storey, true));
        let fd = m.roof_level(og_storey).unwrap();
        let (da, ab) = da_ab(&m, og);
        // Neu mit 2 % Gefälle und vorgeschlagenen Abläufen (Jörn 11.10.)
        let floor = m.floor_of(og).unwrap();
        let neu = m.drainage_of_roof(da).unwrap().clone();
        assert_eq!(neu.slope, crate::gefaelle::DEFAULT_SLOPE);
        assert!(!neu.drains.is_empty());
        assert_eq!(neu.drains, m.proposed_drains(floor));
        let g = m.flat_roof_over(floor).unwrap().slope.unwrap();
        assert!(g.wedge_max() > 0.0);
        let auto = |m: &Model, k: &str| {
            crate::qto::schedule(m)
                .auto
                .iter()
                .find(|a| a.key == k && a.element == da)
                .map(|a| a.value)
        };
        assert_eq!(auto(&m, "roof.drains"), Some(neu.drains.len() as f64));
        assert!(auto(&m, "roof.taper").is_some_and(|v| v > 0.0));
        let flach = waagerecht(&mut m, og);
        let (e, b) = (m.element(da).unwrap(), m.element(ab).unwrap());
        assert_eq!((e.category, e.storey), (Category::Roof, fd));
        assert_eq!((b.category, b.storey), (Category::Coping, fd));
        assert!(e.number.starts_with("DA-"), "{}", e.number);
        let t = m.layer_set(m.flat_roof_type(da).unwrap()).unwrap();
        assert_eq!(
            (t.name.as_str(), t.code.as_str(), t.category),
            ("Flachdach 21,5", "DA-21,5", TypeCategory::FlatRoof)
        );
        let d: Vec<f64> = t.layers.iter().map(|l| l.thickness).collect();
        assert_eq!(d, [10.0, 200.0, 5.0]);
        assert!(t.problems().is_empty(), "{:?}", t.problems());
        assert!(m.check().is_empty(), "{:?}", m.check());
        // Mengen: Fläche innen, Dämmung, Anschluss, Blech
        let q = crate::qto::flat_roof_qto(&m, da).unwrap();
        assert!((q.area / 1e6 - 69.0569).abs() < 1e-4, "{}", q.area / 1e6);
        assert!((q.insulation_volume / 1e9 - 13.81138).abs() < 1e-4);
        let r = m.flat_roof_over(m.floor_of(og).unwrap()).unwrap();
        assert!((r.edge_length() - 33_480.0).abs() < 1e-6);
        assert_eq!(r.corners(), 4);
        assert!((r.upstand() - 285.0).abs() < 1e-6, "{}", r.upstand());
        assert!(m.warnings(da).is_empty());
        let c = crate::qto::coping_qto(&m, ab).unwrap();
        assert!((c.length - 36_000.0).abs() < 1e-6, "{}", c.length);
        // Blech über Aufkantung und Attikadämmung innen 6 cm (Jörn 10.10.
        // 16:07), auf dem Streifen 4 cm: Schenkel je 40 länger,
        // 315 + 60 + 40 + 90 + 90
        assert!((c.girth - 595.0).abs() < 1e-9, "{}", c.girth);
        let p = m.props_of(ab);
        assert_eq!(p.get("Zuschnitt"), Some(&PropValue::Number(625.0)));
        assert_eq!(p.get("Abwicklung"), Some(&PropValue::Number(595.0)));
        // innen: Innenfläche 33,00 m × 0,285 m bis OK Aufkantung, Band
        // (33,48 + 33,00) / 2 × 0,06 m × 0,285 m; Krone: (36,00 + 33,00) / 2
        // × 0,375 m, 4 cm dick
        let r4 = |x: f64| (x * 1e4).round() / 1e4;
        let dd: Vec<(f64, f64, f64)> = c
            .insulation
            .iter()
            .map(|k| (k.thickness, r4(k.area / 1e6), r4(k.volume / 1e9)))
            .collect();
        assert_eq!(dd, [(60.0, 9.405, 0.5684), (40.0, 12.9375, 0.5175)]);
        // Automatikmengen am Dachaufbau
        let s = crate::qto::schedule(&m);
        let auto = |k: &str| {
            s.auto
                .iter()
                .find(|a| a.key == k && a.element == da)
                .map(|a| a.value)
        };
        assert_eq!(auto("roof.edge"), Some(r.edge_length()));
        assert_eq!(auto("roof.corners"), Some(4.0));
        // ohne Gefälle die gespeicherten Abläufe, je mit Regenfallrohr
        // (Review W12 an #68), nicht mehr nach Fläche
        assert_eq!(auto("roof.drains"), Some(4.0));
        assert_eq!(auto("roof.overflows"), Some(4.0));
        // Abschalten nimmt beide mit, Rückgängig bringt sie mit Nummer zurück
        let num = e.number.clone();
        let aus = schritt(&mut m, |m| m.set_flat_roof(og_storey, false));
        assert!(m.flat_roof_of(m.floor_of(og).unwrap()).is_none());
        assert!(m.coping_of(m.floor_of(og).unwrap()).is_none());
        assert!(m.check().is_empty(), "{:?}", m.check());
        m.apply(&aus, Direction::Undo);
        assert_eq!(m.element(da_ab(&m, og).0).unwrap().number, num);
        assert!(m.check().is_empty(), "{:?}", m.check());
        m.apply(&flach, Direction::Undo);
        assert_eq!(m.drainage_of_roof(da), Some(&neu));
        // Vor dem Einschalten: auch Werkstyp und Baustoffe wieder weg
        m.apply(&tx, Direction::Undo);
        assert!(m.type_by_guid(ROOF_TYPE_GUID).is_none());
        let nachher = crate::szo::write(&m);
        assert_eq!(nachher.replace(" next=DA:1,AB:1,AK:4,RF:4", ""), vorher);
    }

    /// D3: Die Dämmdicke ändert den Typ (Paneel „Aufbau“), Grenzen 4–40 cm;
    /// unter 10 cm Anschlusshöhe gibt es einen Hinweis (P13).
    #[test]
    fn daemmdicke_und_hinweis() {
        let (mut m, og) = haus(EXTERIOR_TYPE_GUID);
        let og_storey = m.run(og).unwrap().storey;
        schritt(&mut m, |m| m.set_flat_roof(og_storey, true));
        waagerecht(&mut m, og);
        let (da, _) = da_ab(&m, og);
        let t = m.flat_roof_type(da).unwrap();
        let mut set = m.layer_set(t).unwrap().clone();
        set.layers[1].thickness = 400.0;
        schritt(&mut m, |m| m.set_layer_set(t, set.clone()));
        let r = m.flat_roof_over(m.floor_of(og).unwrap()).unwrap();
        assert!((r.upstand() - 85.0).abs() < 1e-6);
        assert_eq!(m.warnings(da).len(), 1, "{:?}", m.warnings(da));
        set.layers[1].thickness = 410.0;
        assert_eq!(set.problems(), ["Typ DA-21,5: Dämmung 4 bis 40 cm"]);
    }

    /// D1–D4: Wird das OG höher, rückt die Ebene mit; Dachaufbau und Blech
    /// folgen über die Höhenbezüge, die Aufkantungshöhe bleibt.
    #[test]
    fn og_hoeher_flachdach_folgt() {
        let (mut m, og) = haus(EXTERIOR_TYPE_GUID);
        let og_storey = m.run(og).unwrap().storey;
        schritt(&mut m, |m| m.set_flat_roof(og_storey, true));
        schritt(&mut m, |m| m.set_storey_height(og_storey, 3000.0));
        let fd = m.roof_level(og_storey).unwrap();
        assert_eq!(m.storey(fd).unwrap().elevation, 5855.0);
        let r = m.flat_roof_over(m.floor_of(og).unwrap()).unwrap();
        assert_eq!((r.base, r.crown), (5855.0, 6355.0));
        assert!(m.check().is_empty(), "{:?}", m.check());
    }

    /// D4: Abwicklung = Wanddicke + 60 (Attikadämmung innen, Jörn 10.10.
    /// 16:07) + 40 + 90 + 90 (Schenkel über dem Streifen 4 cm je 40
    /// länger), Zuschnitt aufgerundet: AW-49 770 → 1000, AW-36,5 645 → 667.
    #[test]
    fn blech_zuschnitt() {
        for (typ, g, w) in [
            (CAVITY_TYPE_GUID, 770.0, 1000.0),
            (MONO_TYPE_GUID, 645.0, 667.0),
        ] {
            let (mut m, og) = haus(typ);
            let og_storey = m.run(og).unwrap().storey;
            schritt(&mut m, |m| m.set_flat_roof(og_storey, true));
            let (_, ab) = da_ab(&m, og);
            let c = crate::qto::coping_qto(&m, ab).unwrap();
            assert!((c.girth - g).abs() < 1e-9, "{}", c.girth);
            assert_eq!(m.props_of(ab).get("Zuschnitt"), Some(&PropValue::Number(w)));
            assert!(m.check().is_empty(), "{:?}", m.check());
        }
    }

    /// D2 (Koordination 10.10.): Innenputz der Außenwand läuft in der
    /// Aufkantung nicht weiter. Kein Körper, keine Menge; Dachaufbau und
    /// Blech enden am Kern wie ohne Putz.
    #[test]
    fn aufkantung_ohne_innenputz() {
        let (mut ohne, og0) = haus(EXTERIOR_TYPE_GUID);
        let s0 = ohne.run(og0).unwrap().storey;
        schritt(&mut ohne, |m| m.set_flat_roof(s0, true));
        let ref_roof = ohne.flat_roof(ak_runs(&ohne)[0]).unwrap();

        let (mut m, og) = haus(EXTERIOR_TYPE_GUID);
        let set = m.type_by_guid(EXTERIOR_TYPE_GUID).unwrap();
        let putz = m
            .materials()
            .iter()
            .find(|(_, x)| x.name == "Putz")
            .map(|(id, _)| id)
            .unwrap();
        let mut t = m.layer_set(set).unwrap().clone();
        t.layers
            .push(MaterialLayer::new(putz, 15.0, LayerFunction::Finish));
        schritt(&mut m, |m| m.set_layer_set(set, t));
        let og_storey = m.run(og).unwrap().storey;
        schritt(&mut m, |m| m.set_flat_roof(og_storey, true));
        let ak = ak_runs(&m)[0];

        // OG mit Putz, Aufkantung ohne
        assert!(!m.chain(og).unwrap().layers[2].air);
        assert!(m.chain(ak).unwrap().layers[2].air);
        assert_eq!(m.parapet_inner_layers(ak), Some(2));
        assert_eq!(m.parapet_inner_layers(og), None);
        let og_q = crate::qto::run_qto(&m, og);
        assert!(og_q.iter().all(|q| q.layers[2].volume > 0.0));
        let ak_q = crate::qto::run_qto(&m, ak);
        let ak0 = crate::qto::run_qto(&ohne, ak_runs(&ohne)[0]);
        for (q, q0) in ak_q.iter().zip(&ak0) {
            assert_eq!(q.layers[2].thickness, 0.0);
            assert_eq!(q.layers[2].volume, 0.0);
            assert_eq!(q.layers[2].inner_area, 0.0);
            assert_eq!(q.width, 315.0);
            assert!(
                (q.volume - q0.volume).abs() < 1.0,
                "{} {}",
                q.volume,
                q0.volume
            );
            assert!((q.side_inner - q0.side_inner).abs() < 1.0);
            assert!((q.footprint - q0.footprint).abs() < 1.0);
        }
        // keine Zeile Putz an der Aufkantung im Mengenfenster
        let rows = crate::qto::schedule(&m).layer_rows(&m);
        let ak_walls = &m.run(ak).unwrap().segments;
        let ak_rows: Vec<_> = rows
            .iter()
            .filter(|(_, r)| ak_walls.contains(&r.element))
            .collect();
        assert_eq!(ak_rows.len(), 2 * ak_walls.len());
        assert!(ak_rows.iter().all(|(_, r)| r.material != putz));
        assert!(rows.iter().any(|(_, r)| r.material == putz));

        let roof = m.flat_roof(ak).unwrap();
        assert!((roof.area() - ref_roof.area()).abs() < 1.0);
        assert_eq!(roof.width, 315.0);
        assert_eq!(roof.coping_girth(), ref_roof.coping_girth());
        assert!(m.check().is_empty(), "{:?}", m.check());
    }

    /// D2: Ein Gebäude, das nach dem Einschalten in der Vorlage entsteht,
    /// bekommt die Aufkantung gleich mit; im OG gezeichnet auch.
    #[test]
    fn vorlage_mit_flachdach() {
        let mut m = Model::with_seed(3);
        let eg = m.defaults().storey;
        assert!(m.set_flat_roof(eg, true));
        let b = m.add_building(2);
        let fd = m.roof_level_of(Some(b)).expect("FD gehört zum Gebäude");
        let levels = m.levels_in(Some(b));
        assert_eq!(levels.last(), Some(&fd));
        assert_eq!(levels.len(), 4);
        let og = levels[2];
        assert_eq!(m.storey(fd).unwrap().elevation, m.storey(og).unwrap().top());
        let pts = [
            vec3(0.0, 0.0, 0.0),
            vec3(0.0, 8000.0, 0.0),
            vec3(10000.0, 8000.0, 0.0),
            vec3(10000.0, 0.0, 0.0),
        ];
        let eg = m.build_from_polygon(b, &pts).unwrap();
        let og_run = m.runs_above(eg)[0];
        assert_eq!(m.runs_above(og_run), ak_runs(&m));
        assert!(m.check().is_empty(), "{:?}", m.check());
        // Auf der Ebene Flachdach lässt sich nichts zeichnen
        let set = m.defaults().exterior_wall;
        let off: Vec<Vec3> = pts.iter().map(|p| *p + vec3(20000.0, 0.0, 0.0)).collect();
        assert!(m
            .add_wall_run(&off, true, RefSide::Left, fd, set, Category::ExteriorWall)
            .is_none());
    }
}
