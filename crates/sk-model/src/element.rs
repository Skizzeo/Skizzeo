//! Bauteile: gemeinsamer Kopf, Kategorien und Parametrik je Art.

use crate::guid::Guid;
use crate::id::Id;
use crate::kinds::spec;
use crate::library::{LayerSetId, MaterialId};
use crate::wall::RefSide;
use sk_math::Vec3;
use std::collections::BTreeMap;

pub type ElementId = Id<Element>;
pub type RunId = Id<WallRun>;
pub type StoreyId = Id<Storey>;
pub type BuildingId = Id<Building>;

/// Gebäude (IFC: IfcBuilding): ein Wohnhaus oder Nebengebäude mit eigenen
/// Geschossen (B12). Speichert keine Dialogwerte; die Bänder stehen in den
/// Geschossen.
#[derive(Clone, Debug, PartialEq)]
pub struct Building {
    pub guid: Guid,
    /// „Gebäude 1“.
    pub name: String,
    /// „GB-01“, im Modell eindeutig.
    pub number: String,
    /// Versatz OK Sohlplatte (±0,00) über OK Gelände an diesem Gebäude, mm;
    /// positiv sitzt das Gebäude höher (Gelände Thema 1). Ändert sich nur
    /// über [`crate::Model::set_terrain_offset_of`]; die Geländehöhe fragt
    /// man mit [`crate::Model::terrain_z_at`] ab.
    pub terrain: f64,
}

/// Bauteilkategorie: bestimmt Nummernpräfix, IFC-Klasse und DIN-276-Kostengruppe
/// (Tabelle in [`crate::kinds`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Category {
    ExteriorWall,
    InteriorWall,
    /// Geschossdecke (heute nur die Erdgeschossdecke, B10).
    Floor,
    GroundSlab,
    Roof,
    Window,
    Door,
    Opening,
    Space,
    /// Frostschürze: umlaufendes Streifenfundament unter der Sohlplatte.
    StripFooting,
    /// Randdämmstreifen vor dem Deckenauflager einer Außenwand (K5).
    EdgeInsulation,
    /// Untersichtdämmung unter einer auskragenden Decke (OG Phase 2, G7 K4).
    SoffitInsulation,
    /// Dachterrasse auf einer Decke, über der das Geschoss zurückspringt
    /// (BIM Regel 41).
    RoofTerrace,
    /// Attikablech am Rand der Dachterrasse (BIM Regel 46).
    Coping,
    /// Perimeterdämmung vollflächig unter der Sohlplatte (Gelände Thema 4).
    PerimeterInsulation,
    /// Erweiterungsbauteil aus einer .szb (Vertrag 0.5). Name, Präfix,
    /// IFC-Klasse und Kostengruppe kommen aus seiner Definition; deshalb
    /// nicht in [`Category::ALL`], das die Arten des Lieferumfangs nennt.
    Extension,
}

impl Category {
    /// Die Arten des Lieferumfangs (ohne [`Category::Extension`]).
    pub const ALL: [Category; 15] = [
        Category::ExteriorWall,
        Category::InteriorWall,
        Category::Floor,
        Category::GroundSlab,
        Category::Roof,
        Category::Window,
        Category::Door,
        Category::Opening,
        Category::Space,
        Category::StripFooting,
        Category::EdgeInsulation,
        Category::SoffitInsulation,
        Category::RoofTerrace,
        Category::Coping,
        Category::PerimeterInsulation,
    ];

    /// Platz in [`Category::ALL`].
    pub fn index(self) -> usize {
        self as usize
    }

    /// Name, z. B. „Außenwand“ ([`crate::kinds`]).
    pub fn name(self) -> &'static str {
        spec(self).name
    }

    /// Präfix der Bauteilnummer, z. B. „AW“ für AW-001.
    pub fn prefix(self) -> &'static str {
        spec(self).prefix
    }

    /// IFC-Klasse mit vordefiniertem Typ, falls nötig.
    pub fn ifc_class(self) -> &'static str {
        spec(self).ifc
    }

    /// Kostengruppe nach DIN 276 (Räume und Öffnungen haben keine).
    pub fn din276(self) -> Option<u16> {
        spec(self).kg
    }

    /// IFC-Eigenschaft IsExternal.
    pub fn is_external(self) -> bool {
        spec(self).external
    }
}

/// Freier Eigenschaftswert.
#[derive(Clone, Debug, PartialEq)]
pub enum PropValue {
    Text(String),
    Number(f64),
    Bool(bool),
}

/// Freie Eigenschaften (Name → Wert).
pub type PropSet = BTreeMap<String, PropValue>;

/// Gemeinsamer Kopf aller Bauteile.
#[derive(Clone, Debug, PartialEq)]
pub struct Element {
    pub guid: Guid,
    /// Bauteilnummer für Menschen, z. B. „AW-001“, im Modell eindeutig.
    pub number: String,
    pub category: Category,
    /// Geschoss (IFC: IfcRelContainedInSpatialStructure).
    pub storey: StoreyId,
    /// Aufbau, geteilt mit allen Bauteilen desselben Typs.
    pub layer_set: Option<LayerSetId>,
    /// Bauabschnitt in der Reihenfolge der Ausführung (Frostschürze 1,
    /// Sohlplatte 2, Wände 3). Heute nur gespeichert und angezeigt.
    pub seq: u16,
    pub kind: ElementKind,
    pub props: PropSet,
    /// Gesperrt (Paket 4 §2.2): sichtbar und wählbar, aber nicht änderbar.
    /// Nur Bauteile ohne Quelle tragen es; abgeleitete folgen ihrer Quelle
    /// ([`crate::Model::is_locked`]).
    pub locked: bool,
}

/// Parametrik je Bauteilart.
#[derive(Clone, Debug, PartialEq)]
pub enum ElementKind {
    Wall(Wall),
    GroundSlab(GroundSlab),
    StripFooting(StripFooting),
    Floor(Floor),
    /// Randdämmstreifen: nur Verweise auf Wand und Decke; Maße, Baustoff
    /// und Lage folgen aus Wandtyp und Decke (Regel 4).
    EdgeStrip {
        wall: ElementId,
        floor: ElementId,
    },
    /// Untersichtdämmung: nur der Verweis auf die auskragende Decke; Umriss
    /// aus dem Vorsprung darüber, Dicke und Baustoff aus [`Floor::soffit`].
    SoffitInsulation {
        floor: ElementId,
    },
    /// Dachterrasse: nur der Verweis auf die Decke; Umriss aus dem
    /// Rücksprung darüber, Aufbau aus [`Terrace::build_up`] (BIM §3).
    RoofTerrace {
        floor: ElementId,
    },
    /// Attikablech: Verweis auf dieselbe Decke (keine Ladereihenfolge),
    /// Pfad auf OK Attika, Baustoff aus [`Terrace::coping_mat`].
    Coping {
        floor: ElementId,
    },
    /// Erweiterungsbauteil: Verweis auf die Definition und die Werte dieses
    /// Exemplars ([`crate::erweiterung`]).
    Ext(crate::erweiterung::ExtPart),
    /// Perimeterdämmung: nur der Verweis auf die Sohlplatte; Umriss aus der
    /// Platte, Dicke aus [`GroundSlab::insulation`], Baustoff XPS.
    PerimeterInsulation {
        slab: ElementId,
    },
}

/// Dachterrasse einer Decke (BIM §3). Jede Decke trägt die Werte, auch ohne
/// Rücksprung, damit sie erhalten bleiben, wenn einer entsteht.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Terrace {
    /// Aufbau; `None`: Werkstyp „Dachterrasse 14“.
    pub build_up: Option<LayerSetId>,
    /// Attika über OK Belag, mm (0 … 300).
    pub upstand: f64,
    /// Baustoff des Attikablechs; `None`: Titanzink 0,7.
    pub coping_mat: Option<MaterialId>,
}

impl Default for Terrace {
    fn default() -> Self {
        Terrace {
            build_up: None,
            upstand: crate::model::TERRACE_UPSTAND,
            coping_mat: None,
        }
    }
}

/// Untersichtdämmung einer Decke (G7 K4). Jede Decke trägt den Wert, auch
/// ohne Vorsprung, damit er erhalten bleibt, wenn einer entsteht.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Soffit {
    /// Dicke ab UK Decke nach unten, mm (40 … 300).
    pub thickness: f64,
    /// `None`: Baustoff der äußersten Dämmschicht der Wand darüber, bei
    /// einschaligen Typen der des Randdämmstreifens.
    pub material: Option<MaterialId>,
    /// Bekleidung unter der Dämmung (W3), mm: 0 keine, sonst 10 … 80
    /// (Platte und Traglattung symbolisch in einer Schicht).
    pub cladding: f64,
    /// `None`: eingebauter Baustoff „Bekleidung Faserzement“.
    pub cladding_material: Option<MaterialId>,
    /// Überstand des abgefangenen Verblenders unter UK Bekleidung, mm
    /// (20 … 40).
    pub drip: f64,
    /// Achsabstand der Grundlattung, mm (nur Mengen, keine Geometrie).
    pub batten: f64,
    /// Achsabstand der Traglattung quer dazu, mm; 0 ohne.
    pub counter: f64,
}

/// Ein Zahlenwert der Bekleidung an [`Soffit`] (W3).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CladdingValue {
    Thickness,
    Drip,
    Batten,
    Counter,
}

impl CladdingValue {
    pub fn of(self, s: &Soffit) -> f64 {
        match self {
            CladdingValue::Thickness => s.cladding,
            CladdingValue::Drip => s.drip,
            CladdingValue::Batten => s.batten,
            CladdingValue::Counter => s.counter,
        }
    }

    pub(crate) fn of_mut(self, s: &mut Soffit) -> &mut f64 {
        match self {
            CladdingValue::Thickness => &mut s.cladding,
            CladdingValue::Drip => &mut s.drip,
            CladdingValue::Batten => &mut s.batten,
            CladdingValue::Counter => &mut s.counter,
        }
    }
}

impl Default for Soffit {
    fn default() -> Soffit {
        use crate::model::{BATTEN, COUNTER, SOFFIT_CLADDING, SOFFIT_DRIP, SOFFIT_THICKNESS};
        Soffit {
            thickness: SOFFIT_THICKNESS,
            material: None,
            cladding: SOFFIT_CLADDING,
            cladding_material: None,
            drip: SOFFIT_DRIP,
            batten: BATTEN,
            counter: COUNTER,
        }
    }
}

/// Geschossdecke über einem geschlossenen Außenwandzug (IFC: IfcSlab FLOOR).
/// Ihr Umriss ist abgeleitet: Außenseite der tragenden Schicht des Zuges; die
/// Decke liegt in einer Auflagertasche über die ganze Kerndicke.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Floor {
    pub run: RunId,
    pub material: MaterialId,
    /// Dicke in mm, von der Oberkante nach unten.
    pub thickness: f64,
    /// Oberkante: OK Erdgeschoss (B11).
    pub top: LevelRef,
    /// Untersichtdämmung unter dem auskragenden Streifen, wo das Geschoss
    /// darüber vorspringt (G7 K4).
    pub soffit: Soffit,
    /// Dachterrasse, Attika und Attikablech, wo das Geschoss darüber
    /// zurückspringt (BIM paket-dachterrasse).
    pub terrace: Terrace,
}

impl ElementKind {
    /// Baustoff am Bauteil selbst: bei Decke, Sohlplatte und Frostschürze
    /// der Baustoff der Kernschicht. Wände tragen ihn im Typ.
    pub fn material(&self) -> Option<MaterialId> {
        match self {
            ElementKind::Floor(f) => Some(f.material),
            ElementKind::GroundSlab(s) => Some(s.material),
            ElementKind::StripFooting(f) => Some(f.material),
            _ => None,
        }
    }

    /// Dicke der Kernschicht, die am Bauteil steht (Regel 38): Decke und
    /// Sohlplatte die Dicke, Frostschürze die Breite.
    pub fn core_thickness(&self) -> Option<f64> {
        match self {
            ElementKind::Floor(f) => Some(f.thickness),
            ElementKind::GroundSlab(s) => Some(s.thickness),
            ElementKind::StripFooting(f) => Some(f.width),
            _ => None,
        }
    }
}

/// Sohlplatte unter einem geschlossenen Außenwandzug (IFC: IfcSlab BASESLAB).
/// Ihr Umriss ist abgeleitet: Außenfläche des Zuges, um `recess` nach innen.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GroundSlab {
    pub run: RunId,
    pub material: MaterialId,
    /// Oberkante: UK Erdgeschoss (±0,00).
    pub top: LevelRef,
    /// Dicke in mm, von der Oberkante (z = 0) nach unten.
    pub thickness: f64,
    /// Sockelrücksprung in mm: 0 (bündig) oder mindestens 20.
    pub recess: f64,
    /// Perimeterdämmung vollflächig unter der Platte, mm; 0 = keine. Sie
    /// hebt die Platte um ihre Dicke über UK Gründung (Gelände Thema 4).
    pub insulation: f64,
}

/// Frostschürze unter dem Rand einer Sohlplatte (IFC: IfcFooting STRIP_FOOTING),
/// außen bündig mit der Platte.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StripFooting {
    pub slab: ElementId,
    pub material: MaterialId,
    /// Breite in mm.
    pub width: f64,
    /// Unterkante: UK Gründung. Die Tiefe ab UK Sohlplatte ist abgeleitet.
    pub base: LevelRef,
}

/// Wand = ein gerades Segment eines Wandzugs (IFC: IfcWall).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Wall {
    pub run: RunId,
    /// Segment im Wandzug: von Punkt `seg` zu Punkt `seg + 1`.
    pub seg: u32,
    /// Kopplung an die Wand im Geschoss darunter (gestapelte Außenwand, B12).
    pub coupling: Option<Coupling>,
}

/// Stapelbezug eines Wandsegments auf das Segment im Geschoss direkt
/// darunter: seine Bezugslinie ist die des Partners plus `offset` nach
/// außen. Gekoppelt (`linked`) geht es mit, wenn man den Partner zieht;
/// gelöst bleibt es stehen und nur `offset` wird nachgeführt (OG Phase 2).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Coupling {
    pub below: ElementId,
    /// mm quer zur Wand, + = nach außen; immer der wahre Abstand.
    pub offset: f64,
    /// Kette zu: folgt dem Partner.
    pub linked: bool,
}

/// Wandzug: die Eingabe, aus der die Wand-Bauteile entstehen.
#[derive(Clone, Debug, PartialEq)]
pub struct WallRun {
    pub guid: Guid,
    /// Eckpunkte der Bezugslinie (bereinigt, ohne doppelte Punkte).
    pub points: Vec<Vec3>,
    pub closed: bool,
    pub ref_side: RefSide,
    /// Wandfuß: UK des Geschosses (B11).
    pub base: LevelRef,
    /// Wandkrone: OK des Geschosses (B12); die Höhe ist abgeleitet.
    pub top: LevelRef,
    pub storey: StoreyId,
    /// Je Segment ein Bauteil vom Typ Wand, in Segmentreihenfolge.
    pub segments: Vec<ElementId>,
}

/// Art eines Geschossbands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LevelKind {
    /// Gründung: UK Frostschürze bis OK Sohlplatte (±0,00).
    Foundation,
    Storey,
}

/// Geschoss als Band mit Unter- und Oberkante (IFC: IfcBuildingStorey,
/// Elevation = UK). Die Bänder liegen lückenlos übereinander.
#[derive(Clone, Debug, PartialEq)]
pub struct Storey {
    pub guid: Guid,
    /// Gebäude, zu dem das Geschoss gehört; `None` nur für die Vorlage, solange
    /// es noch kein Gebäude gibt (das erste Gebäude übernimmt sie).
    pub building: Option<BuildingId>,
    /// „Gründung“, „Erdgeschoss“, „Obergeschoss“.
    pub name: String,
    /// „GR“, „EG“, „OG“.
    pub short: String,
    pub kind: LevelKind,
    /// Unterkante in mm, absolut zu ±0,00.
    pub elevation: f64,
    /// Geschosshöhe in mm; OK = `elevation + height`.
    pub height: f64,
    /// Nur Gründung (Gelände Thema 1): gewollte Einbindetiefe (OK Gelände
    /// bis UK, mm), wenn die Schürze sie gerade nicht einhält, weil sie an
    /// ihr Mindestmaß stößt. Folgt das Gelände zurück, gilt wieder sie.
    /// `None`: die tatsächliche Einbindetiefe ist die gewollte.
    pub embed: Option<f64>,
}

impl Storey {
    pub fn top(&self) -> f64 {
        self.elevation + self.height
    }
}

/// Kante eines Geschossbands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LevelEdge {
    Bottom,
    Top,
}

/// Höhenbezug eines Bauteils: Kante eines Geschosses plus Versatz (mm).
/// Bauteile speichern keine absoluten Höhen (Prüfregel 14).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LevelRef {
    pub storey: StoreyId,
    pub edge: LevelEdge,
    pub offset: f64,
}

impl LevelRef {
    pub fn bottom(storey: StoreyId) -> LevelRef {
        LevelRef {
            storey,
            edge: LevelEdge::Bottom,
            offset: 0.0,
        }
    }

    pub fn top(storey: StoreyId) -> LevelRef {
        LevelRef {
            storey,
            edge: LevelEdge::Top,
            offset: 0.0,
        }
    }
}
