//! Bauteile: gemeinsamer Kopf, Kategorien und Parametrik je Art.

use crate::guid::Guid;
use crate::id::Id;
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
}

/// Bauteilkategorie: bestimmt Nummernpräfix, IFC-Klasse und DIN-276-Kostengruppe.
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
}

impl Category {
    pub const ALL: [Category; 10] = [
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
    ];

    /// Platz in [`Category::ALL`].
    pub fn index(self) -> usize {
        self as usize
    }

    pub fn name(self) -> &'static str {
        match self {
            Category::ExteriorWall => "Außenwand",
            Category::InteriorWall => "Innenwand",
            Category::Floor => "Geschossdecke",
            Category::GroundSlab => "Sohlplatte",
            Category::Roof => "Dach",
            Category::Window => "Fenster",
            Category::Door => "Tür",
            Category::Opening => "Öffnung",
            Category::Space => "Raum",
            Category::StripFooting => "Frostschürze",
        }
    }

    /// Präfix der Bauteilnummer, z. B. „AW“ für AW-001.
    pub fn prefix(self) -> &'static str {
        match self {
            Category::ExteriorWall => "AW",
            Category::InteriorWall => "IW",
            Category::Floor => "DE",
            Category::GroundSlab => "SP",
            Category::Roof => "DA",
            Category::Window => "FE",
            Category::Door => "TU",
            Category::Opening => "OE",
            Category::Space => "R",
            Category::StripFooting => "FS",
        }
    }

    /// IFC-Klasse mit vordefiniertem Typ, falls nötig.
    pub fn ifc_class(self) -> &'static str {
        match self {
            Category::ExteriorWall | Category::InteriorWall => "IfcWall",
            Category::Floor => "IfcSlab.FLOOR",
            Category::GroundSlab => "IfcSlab.BASESLAB",
            Category::Roof => "IfcRoof",
            Category::Window => "IfcWindow",
            Category::Door => "IfcDoor",
            Category::Opening => "IfcOpeningElement",
            Category::Space => "IfcSpace",
            Category::StripFooting => "IfcFooting.STRIP_FOOTING",
        }
    }

    /// Kostengruppe nach DIN 276 (Räume und Öffnungen haben keine).
    pub fn din276(self) -> Option<u16> {
        match self {
            Category::ExteriorWall | Category::Window => Some(330),
            Category::InteriorWall | Category::Door => Some(340),
            Category::Floor => Some(350),
            Category::GroundSlab | Category::StripFooting => Some(322),
            Category::Roof => Some(360),
            Category::Opening | Category::Space => None,
        }
    }

    /// IFC-Eigenschaft IsExternal.
    pub fn is_external(self) -> bool {
        matches!(self, Category::ExteriorWall)
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
}

/// Parametrik je Bauteilart.
#[derive(Clone, Debug, PartialEq)]
pub enum ElementKind {
    Wall(Wall),
    GroundSlab(GroundSlab),
    StripFooting(StripFooting),
    Floor(Floor),
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

/// Kopplung eines Wandsegments an das Segment im Geschoss direkt darunter:
/// seine Bezugslinie ist die des Partners plus `offset` nach außen. Zieht
/// man den Partner, geht das Segment im selben Schritt mit.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Coupling {
    pub below: ElementId,
    /// mm quer zur Wand, + = nach außen; in Phase 1 immer 0.
    pub offset: f64,
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
