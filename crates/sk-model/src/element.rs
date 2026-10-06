//! Bauteile: gemeinsamer Kopf, Kategorien und Parametrik je Art.

use crate::guid::Guid;
use crate::id::Id;
use crate::library::LayerSetId;
use crate::wall::RefSide;
use sk_math::Vec3;
use std::collections::BTreeMap;

pub type ElementId = Id<Element>;
pub type RunId = Id<WallRun>;
pub type StoreyId = Id<Storey>;

/// Bauteilkategorie: bestimmt Nummernpräfix, IFC-Klasse und DIN-276-Kostengruppe.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Category {
    ExteriorWall,
    InteriorWall,
    Slab,
    BaseSlab,
    Roof,
    Window,
    Door,
    Opening,
    Space,
}

impl Category {
    pub const ALL: [Category; 9] = [
        Category::ExteriorWall,
        Category::InteriorWall,
        Category::Slab,
        Category::BaseSlab,
        Category::Roof,
        Category::Window,
        Category::Door,
        Category::Opening,
        Category::Space,
    ];

    /// Platz in [`Category::ALL`].
    pub fn index(self) -> usize {
        self as usize
    }

    pub fn name(self) -> &'static str {
        match self {
            Category::ExteriorWall => "Außenwand",
            Category::InteriorWall => "Innenwand",
            Category::Slab => "Decke",
            Category::BaseSlab => "Bodenplatte",
            Category::Roof => "Dach",
            Category::Window => "Fenster",
            Category::Door => "Tür",
            Category::Opening => "Öffnung",
            Category::Space => "Raum",
        }
    }

    /// Präfix der Bauteilnummer, z. B. „AW“ für AW-001.
    pub fn prefix(self) -> &'static str {
        match self {
            Category::ExteriorWall => "AW",
            Category::InteriorWall => "IW",
            Category::Slab => "DE",
            Category::BaseSlab => "BP",
            Category::Roof => "DA",
            Category::Window => "FE",
            Category::Door => "TU",
            Category::Opening => "OE",
            Category::Space => "R",
        }
    }

    /// IFC-Klasse mit vordefiniertem Typ, falls nötig.
    pub fn ifc_class(self) -> &'static str {
        match self {
            Category::ExteriorWall | Category::InteriorWall => "IfcWall",
            Category::Slab => "IfcSlab.FLOOR",
            Category::BaseSlab => "IfcSlab.BASESLAB",
            Category::Roof => "IfcRoof",
            Category::Window => "IfcWindow",
            Category::Door => "IfcDoor",
            Category::Opening => "IfcOpeningElement",
            Category::Space => "IfcSpace",
        }
    }

    /// Kostengruppe nach DIN 276 (Räume und Öffnungen haben keine).
    pub fn din276(self) -> Option<u16> {
        match self {
            Category::ExteriorWall | Category::Window => Some(330),
            Category::InteriorWall | Category::Door => Some(340),
            Category::Slab => Some(350),
            Category::BaseSlab => Some(320),
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
    pub kind: ElementKind,
    pub props: PropSet,
}

/// Parametrik je Bauteilart.
#[derive(Clone, Debug, PartialEq)]
pub enum ElementKind {
    Wall(Wall),
}

/// Wand = ein gerades Segment eines Wandzugs (IFC: IfcWall).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Wall {
    pub run: RunId,
    /// Segment im Wandzug: von Punkt `seg` zu Punkt `seg + 1`.
    pub seg: u32,
}

/// Wandzug: die Eingabe, aus der die Wand-Bauteile entstehen.
#[derive(Clone, Debug, PartialEq)]
pub struct WallRun {
    pub guid: Guid,
    /// Eckpunkte der Bezugslinie (bereinigt, ohne doppelte Punkte).
    pub points: Vec<Vec3>,
    pub closed: bool,
    pub ref_side: RefSide,
    pub height: f64,
    pub storey: StoreyId,
    /// Je Segment ein Bauteil vom Typ Wand, in Segmentreihenfolge.
    pub segments: Vec<ElementId>,
}

/// Geschoss (IFC: IfcBuildingStorey).
#[derive(Clone, Debug, PartialEq)]
pub struct Storey {
    pub guid: Guid,
    pub name: String,
    /// Höhe der Geschossebene (OK Rohfußboden) in mm.
    pub elevation: f64,
    /// Geschosshöhe in mm.
    pub height: f64,
}
