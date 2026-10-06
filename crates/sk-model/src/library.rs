//! Bibliothek: Baustoffe und Aufbauten (Bauteiltypen).
//!
//! Bauteile verweisen auf einen Aufbau, statt ihre Schichten zu kopieren.
//! Ändert sich der Aufbau, ändern sich alle Bauteile dieses Typs.

use crate::attr::{FillId, PenId, SurfaceId};
use crate::element::{Category, PropSet};
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
    /// Ruhende Luftschicht: zählt zur Dicke, hat aber keinen Körper (K4).
    Air,
}

impl MatCategory {
    pub const ALL: [MatCategory; 6] = [
        MatCategory::Masonry,
        MatCategory::Concrete,
        MatCategory::Insulation,
        MatCategory::Plaster,
        MatCategory::Timber,
        MatCategory::Air,
    ];

    pub fn name(self) -> &'static str {
        match self {
            MatCategory::Masonry => "Mauerwerk",
            MatCategory::Concrete => "Beton",
            MatCategory::Insulation => "Dämmung",
            MatCategory::Plaster => "Putz",
            MatCategory::Timber => "Holz",
            MatCategory::Air => "Luft",
        }
    }
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
    /// Schraffur der Schnittfläche in der Bauzeichnung.
    pub cut_fill: FillId,
    /// Stift der Schraffurlinien.
    pub cut_fg: PenId,
    /// Stift des Grundes unter der Schraffur (Füllfarbe in der Zeichnung).
    pub cut_bg: PenId,
    /// Oberfläche in 3D.
    pub surface: SurfaceId,
}

/// Die Darstellungsverweise eines Baustoffs: Schnittstelle zwischen BIM und
/// Darstellung (E6, Teil C). Nur sie ändert das Einstellungsfenster.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MaterialDisplay {
    pub cut_fill: FillId,
    pub cut_fg: PenId,
    pub cut_bg: PenId,
    pub surface: SurfaceId,
}

impl Material {
    pub fn display(&self) -> MaterialDisplay {
        MaterialDisplay {
            cut_fill: self.cut_fill,
            cut_fg: self.cut_fg,
            cut_bg: self.cut_bg,
            surface: self.surface,
        }
    }
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

/// Wie eine Geschossdecke auf dem Typ aufliegt (K5).
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub enum Bearing {
    /// Tasche über den ganzen Kern (Vorgabe).
    #[default]
    Core,
    /// Die Decke liegt `depth` (mm, ab Innenseite) auf; außen davor ein
    /// Randdämmstreifen aus `strip`. Die Tasche bleibt über die ganze Wand.
    Depth { depth: f64, strip: MaterialId },
}

/// Art eines Bauteiltyps: für welche Bauteile er taugt (K1). Weitere Arten
/// (Decke, Dach …) nur auf Jörns Vorgabe.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum TypeCategory {
    ExteriorWall,
    InteriorWall,
}

impl TypeCategory {
    pub const ALL: [TypeCategory; 2] = [TypeCategory::ExteriorWall, TypeCategory::InteriorWall];

    /// Typart, die Bauteile der Kategorie brauchen; `None`: ohne Typ.
    pub fn of(c: Category) -> Option<TypeCategory> {
        match c {
            Category::ExteriorWall => Some(TypeCategory::ExteriorWall),
            Category::InteriorWall => Some(TypeCategory::InteriorWall),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            TypeCategory::ExteriorWall => "Außenwand",
            TypeCategory::InteriorWall => "Innenwand",
        }
    }

    /// Präfix des Kurzzeichens, z. B. „AW“ für AW-1.
    pub fn prefix(self) -> &'static str {
        match self {
            TypeCategory::ExteriorWall => "AW",
            TypeCategory::InteriorWall => "IW",
        }
    }
}

/// Kurzzeichen aus Typart und Dicke in cm, z. B. „AW-31,5“, „IW-17,5“.
pub fn type_code(category: TypeCategory, thickness: f64) -> String {
    let cm = thickness.round() / 10.0;
    let t = if cm.fract() == 0.0 {
        format!("{cm:.0}")
    } else {
        format!("{cm:.1}").replace('.', ",")
    };
    format!("{}-{t}", category.prefix())
}

/// Feste Merkmale, die der Katalog immer zeigt (IFC `Pset_WallCommon`
/// FireRating, AcousticRating; Hersteller aus
/// `Pset_ManufacturerTypeInformation`). Weitere Schlüssel sind frei.
pub const TYPE_PROPS: [&str; 4] = ["Brandschutz", "Schallschutz", "Hersteller", "Bemerkung"];

/// Bauteiltyp (IFC: IfcWallType + IfcMaterialLayerSet). Schichten von außen
/// nach innen. Bauteile verweisen darauf und kopieren nie Schichten.
#[derive(Clone, Debug, PartialEq)]
pub struct LayerSet {
    /// Ändert sich nie, auch nicht beim Übernehmen oder Zurückspeichern
    /// (Regel 19).
    pub guid: Guid,
    pub name: String,
    /// Kurzzeichen, im Projekt eindeutig und nicht leer, z. B. „AW-1“.
    pub code: String,
    pub category: TypeCategory,
    pub layers: Vec<MaterialLayer>,
    /// Merkmale des Typs; ein gleichnamiges Merkmal am Bauteil überschreibt
    /// sie ([`crate::Model::props_of`]).
    pub props: PropSet,
    /// Beschreibung für den Katalog.
    pub note: String,
    /// Änderungsstand, +1 bei jeder Änderung; nur zur Anzeige im Abgleich.
    pub changed: u32,
    /// Deckenauflager (K5).
    pub bearing: Bearing,
}

impl LayerSet {
    pub fn thickness(&self) -> f64 {
        self.layers.iter().map(|l| l.thickness).sum()
    }

    /// Dicke des Randdämmstreifens vor dem Deckenauflager (Wanddicke minus
    /// Auflagertiefe); `None` bei Tasche über den ganzen Kern.
    pub fn strip_width(&self) -> Option<f64> {
        match self.bearing {
            Bearing::Core => None,
            Bearing::Depth { depth, .. } => Some(self.thickness() - depth),
        }
    }

    /// Baustoff des Randdämmstreifens.
    pub fn strip_material(&self) -> Option<MaterialId> {
        match self.bearing {
            Bearing::Core => None,
            Bearing::Depth { strip, .. } => Some(strip),
        }
    }

    /// Tragend (IFC LoadBearing): es gibt eine tragende Kernschicht.
    pub fn load_bearing(&self) -> bool {
        self.layers
            .iter()
            .any(|l| l.core && l.function == LayerFunction::Structure)
    }

    /// Außenbauteil (IFC IsExternal), aus der Typart.
    pub fn is_external(&self) -> bool {
        self.category == TypeCategory::ExteriorWall
    }

    /// Verstöße gegen die Regeln eines Typs: mindestens eine Schicht, jede
    /// dicker als 0, Kernschichten zusammenhängend, Kurzzeichen nicht leer;
    /// Luftschichten nach Regel 20.
    pub fn problems(&self) -> Vec<String> {
        let mut out = Vec::new();
        let who = if self.code.is_empty() {
            self.name.as_str()
        } else {
            self.code.as_str()
        };
        if self.code.trim().is_empty() {
            out.push(format!("Typ {}: Kurzzeichen fehlt", self.name));
        }
        if self.layers.is_empty() {
            out.push(format!("Typ {who}: keine Schichten"));
        }
        if self
            .layers
            .iter()
            .any(|l| l.thickness <= 0.0 || !l.thickness.is_finite())
        {
            out.push(format!("Typ {who}: Schichtdicke ungültig"));
        }
        let core: Vec<usize> = (0..self.layers.len())
            .filter(|&i| self.layers[i].core)
            .collect();
        if core.windows(2).any(|w| w[1] != w[0] + 1) {
            out.push(format!("Typ {who}: Kernschichten nicht zusammenhängend"));
        }
        // Regel 20: Luftschicht nie Kern, nie am Rand, nie zweimal nacheinander
        let air = |l: &MaterialLayer| l.function == LayerFunction::AirGap;
        let n = self.layers.len();
        if self.layers.iter().any(|l| air(l) && l.core) {
            out.push(format!("Typ {who}: Luftschicht als Kern"));
        }
        if n > 0 && (air(&self.layers[0]) || air(&self.layers[n - 1])) {
            out.push(format!("Typ {who}: Luftschicht am Rand des Aufbaus"));
        }
        if self.layers.windows(2).any(|w| air(&w[0]) && air(&w[1])) {
            out.push(format!("Typ {who}: zwei Luftschichten hintereinander"));
        }
        // Regel 21: das Auflager liegt im Kern und ist kürzer als die Wand
        if let Bearing::Depth { depth, .. } = self.bearing {
            let first = self.layers.iter().position(|l| l.core).unwrap_or(0);
            let band: f64 = self.layers[first.min(n)..]
                .iter()
                .map(|l| l.thickness)
                .sum();
            if !(depth.is_finite() && depth > 0.0 && depth < band.min(self.thickness()) - 1e-9) {
                out.push(format!("Typ {who}: Deckenauflager ungültig"));
            }
        }
        out
    }
}

/// Darstellungsschlüssel eines Baustoffs in Körpern ([`crate::Tri::mat`]):
/// Platz in der Bibliothek plus 1; 0 heißt „ohne Baustoff“.
pub fn material_key(id: MaterialId) -> u16 {
    debug_assert!(id.index() + 1 < material::CUT as u32);
    (id.index() + 1) as u16
}
