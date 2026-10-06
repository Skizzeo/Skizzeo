//! Bibliothek: Baustoffe und Aufbauten (Bauteiltypen).
//!
//! Bauteile verweisen auf einen Aufbau, statt ihre Schichten zu kopieren.
//! Ändert sich der Aufbau, ändern sich alle Bauteile dieses Typs.

use crate::guid::Guid;
use crate::id::Id;
use crate::solid::material;

pub type MaterialId = Id<Material>;
pub type LayerSetId = Id<LayerSet>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MatCategory {
    Masonry,
    Concrete,
    Insulation,
    Plaster,
    Timber,
}

impl MatCategory {
    pub fn name(self) -> &'static str {
        match self {
            MatCategory::Masonry => "Mauerwerk",
            MatCategory::Concrete => "Beton",
            MatCategory::Insulation => "Dämmung",
            MatCategory::Plaster => "Putz",
            MatCategory::Timber => "Holz",
        }
    }
}

/// Schraffur einer geschnittenen Fläche in der Bauzeichnung.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Hatch {
    None,
    /// Schräge Linien (Mauerwerk).
    Diagonal,
    /// Zickzack (harte Dämmung).
    Zigzag,
}

/// Baustoff (IFC: IfcMaterial).
#[derive(Clone, Debug, PartialEq)]
pub struct Material {
    pub guid: Guid,
    pub name: String,
    pub category: MatCategory,
    /// Verschnittpriorität 0..999: die höhere läuft durch.
    pub priority: u16,
    /// Rohdichte in kg/m³.
    pub density: f64,
    /// Wärmeleitfähigkeit in W/(mK), später für den U-Wert.
    pub lambda: Option<f64>,
    pub hatch: Hatch,
    /// Farbe der Oberfläche in 3D.
    pub color: [u8; 3],
    /// Farbe einer Schnittfläche in 3D.
    pub cut_color: [u8; 3],
}

/// Aufgabe einer Schicht im Aufbau.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LayerFunction {
    Structure,
    Insulation,
    Finish,
    Membrane,
    AirGap,
}

/// Eine Schicht im Aufbau (IFC: IfcMaterialLayer).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MaterialLayer {
    pub material: MaterialId,
    pub thickness: f64,
    pub function: LayerFunction,
    /// Gehört zum tragenden Kern.
    pub core: bool,
}

/// Aufbau = Bauteiltyp (IFC: IfcMaterialLayerSet). Schichten von außen nach innen.
#[derive(Clone, Debug, PartialEq)]
pub struct LayerSet {
    pub guid: Guid,
    pub name: String,
    pub layers: Vec<MaterialLayer>,
}

impl LayerSet {
    pub fn thickness(&self) -> f64 {
        self.layers.iter().map(|l| l.thickness).sum()
    }
}

/// Darstellungsschlüssel eines Baustoffs in Körpern ([`crate::Tri::mat`]):
/// Platz in der Bibliothek plus 1; 0 heißt „ohne Baustoff“.
pub fn material_key(id: MaterialId) -> u16 {
    debug_assert!(id.index() + 1 < material::CUT as u32);
    (id.index() + 1) as u16
}
