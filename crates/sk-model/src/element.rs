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
    /// Oberkante in mm über dem Wandfuß. Beim Anlegen ⅔ der Wandhöhe, danach
    /// fest (bis zur Ebenenverwaltung).
    pub top: f64,
}

/// Sohlplatte unter einem geschlossenen Außenwandzug (IFC: IfcSlab BASESLAB).
/// Ihr Umriss ist abgeleitet: Außenfläche des Zuges, um `recess` nach innen.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GroundSlab {
    pub run: RunId,
    pub material: MaterialId,
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
    /// Tiefe in mm ab Unterkante Sohlplatte.
    pub depth: f64,
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
