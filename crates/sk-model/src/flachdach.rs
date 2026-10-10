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
        let inner = inner + (outer - inner).signum() * drop;
        let outline = sk_math::polygon::to_ccw(&chain.face_corners(inner));
        // Gefälle: der Keil liegt in der obersten Dämmschicht (Konzept F3)
        let tapered = layers.iter().position(|l| l.2);
        let slope = tapered
            .and(self.drainage_of(floor))
            .filter(|d| d.on())
            .and_then(|d| SlopeField::compute(&outline, &d.drains, d.slope));
        Some(FlatRoof {
            tapered: slope.as_ref().and(tapered),
            slope,
            outline,
            base: self.level_z(f.top)?,
            layers,
            ring: sk_math::polygon::to_ccw(&chain.face_corners(chain.outer_offset())),
            width: chain.thickness() - drop,
            crown: chain.top(),
            coping_mat: self
                .coping_material_of(&f.terrace)
                .map_or(material::PLAIN, material_key),
        })
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
                let kind = ElementKind::Roof {
                    floor,
                    drainage: Default::default(),
                };
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
        let (added, removed) = self.sync_flat_roofs();
        self.strict = strict;
        let mut out = Vec::new();
        if added > 0 {
            out.push("Flachdach bzw. Attikablech ergänzt".to_string());
        }
        if removed > 0 {
            out.push("Flachdach bzw. Attikablech ohne Aufkantung entfernt".to_string());
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::txn::Direction;
    use sk_math::vec3;

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
        assert!(refusal_text(&m, w, &m.can_delete(w).unwrap_err()).contains("Geschossverwaltung"));
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
        let soll = vorher.replacen("\n[building]", " next=DA:1,AB:1,AK:4\n[building]", 1);
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
    }

    /// Dachaufbau und Blech des Flachdachs auf der OG-Decke unter `og`.
    fn da_ab(m: &Model, og: RunId) -> (ElementId, ElementId) {
        let floor = m.floor_of(og).unwrap();
        (m.flat_roof_of(floor).unwrap(), m.coping_of(floor).unwrap())
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
        assert!((c.girth - 455.0).abs() < 1e-9);
        let p = m.props_of(ab);
        assert_eq!(p.get("Zuschnitt"), Some(&PropValue::Number(500.0)));
        assert_eq!(p.get("Abwicklung"), Some(&PropValue::Number(455.0)));
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
        assert_eq!(auto("roof.drains"), Some(1.0));
        assert_eq!(auto("roof.overflows"), Some(1.0));
        // Abschalten nimmt beide mit, Rückgängig bringt sie mit Nummer zurück
        let num = e.number.clone();
        let aus = schritt(&mut m, |m| m.set_flat_roof(og_storey, false));
        assert!(m.flat_roof_of(m.floor_of(og).unwrap()).is_none());
        assert!(m.coping_of(m.floor_of(og).unwrap()).is_none());
        assert!(m.check().is_empty(), "{:?}", m.check());
        m.apply(&aus, Direction::Undo);
        assert_eq!(m.element(da_ab(&m, og).0).unwrap().number, num);
        assert!(m.check().is_empty(), "{:?}", m.check());
        // Vor dem Einschalten: auch Werkstyp und Baustoffe wieder weg
        m.apply(&tx, Direction::Undo);
        assert!(m.type_by_guid(ROOF_TYPE_GUID).is_none());
        let nachher = crate::szo::write(&m);
        assert_eq!(nachher.replace(" next=DA:1,AB:1,AK:4", ""), vorher);
    }

    /// D3: Die Dämmdicke ändert den Typ (Paneel „Aufbau“), Grenzen 4–40 cm;
    /// unter 10 cm Anschlusshöhe gibt es einen Hinweis (P13).
    #[test]
    fn daemmdicke_und_hinweis() {
        let (mut m, og) = haus(EXTERIOR_TYPE_GUID);
        let og_storey = m.run(og).unwrap().storey;
        schritt(&mut m, |m| m.set_flat_roof(og_storey, true));
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

    /// D4: Abwicklung = Wanddicke + 40 + 50 + 50, Zuschnitt aufgerundet:
    /// AW-49 630 → 667, AW-36,5 505 → 625.
    #[test]
    fn blech_zuschnitt() {
        for (typ, g, w) in [
            (CAVITY_TYPE_GUID, 630.0, 667.0),
            (MONO_TYPE_GUID, 505.0, 625.0),
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
