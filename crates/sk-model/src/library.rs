//! Bibliothek: Baustoffe und Aufbauten (Bauteiltypen).
//!
//! Bauteile verweisen auf einen Aufbau, statt ihre Schichten zu kopieren.
//! Ändert sich der Aufbau, ändern sich alle Bauteile dieses Typs.

use crate::attr::{FillId, PenId, SurfaceId};
use crate::element::{Category, PropSet};
use crate::guid::Guid;
use crate::id::Id;
use crate::solid::material;
use crate::trade::TradeId;

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
    /// Blech (Attikablech, Dachterrasse D3).
    Metal,
}

impl MatCategory {
    pub const ALL: [MatCategory; 7] = [
        MatCategory::Masonry,
        MatCategory::Concrete,
        MatCategory::Insulation,
        MatCategory::Plaster,
        MatCategory::Timber,
        MatCategory::Air,
        MatCategory::Metal,
    ];

    pub fn name(self) -> &'static str {
        match self {
            MatCategory::Masonry => "Mauerwerk",
            MatCategory::Concrete => "Beton",
            MatCategory::Insulation => "Dämmung",
            MatCategory::Plaster => "Putz",
            MatCategory::Timber => "Holz",
            MatCategory::Air => "Luft",
            MatCategory::Metal => "Metall",
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
    /// Gewerk, das der Baustoff für seine Schichten vorschlägt (Paket 1a).
    pub trade: Option<TradeId>,
    /// Kennwerte (Paket 5): feste Schlüssel aus [`crate::MAT_PROPS`] und
    /// eigene; Rohdichte und λ stehen in ihren Feldern.
    pub props: PropSet,
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
    /// Baustoff ohne λ, Gewerk und Kennwerte; weitere Angaben über die
    /// Builder (wie [`MaterialLayer::new`]).
    pub fn new(
        guid: Guid,
        name: impl Into<String>,
        category: MatCategory,
        priority: u16,
        density: f64,
        d: MaterialDisplay,
    ) -> Self {
        Material {
            guid,
            name: name.into(),
            category,
            priority,
            density,
            lambda: None,
            cut_fill: d.cut_fill,
            cut_fg: d.cut_fg,
            cut_bg: d.cut_bg,
            surface: d.surface,
            trade: None,
            props: PropSet::new(),
        }
    }

    /// Wärmeleitfähigkeit in W/(mK).
    pub fn lambda(mut self, lambda: Option<f64>) -> Self {
        self.lambda = lambda;
        self
    }

    /// Gewerk, das der Baustoff vorschlägt.
    pub fn trade(mut self, trade: Option<TradeId>) -> Self {
        self.trade = trade;
        self
    }

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
    /// Gewerk abweichend vom Baustoff (Paket 1a, [`crate::Model::layer_trade`]).
    pub trade: Option<TradeId>,
    /// Kostengruppe abweichend von der Tabelle ([`crate::Model::layer_kg`]).
    pub kg: Option<u16>,
}

impl MaterialLayer {
    /// Schicht außerhalb des Kerns; weitere Angaben über die Builder
    /// (R4a, bim/paket-r4-deckenschichten.md §1.1).
    pub fn new(material: MaterialId, thickness: f64, function: LayerFunction) -> Self {
        MaterialLayer {
            material,
            thickness,
            function,
            core: false,
            trade: None,
            kg: None,
        }
    }

    /// Gewerk abweichend vom Baustoff.
    pub fn trade(mut self, trade: Option<TradeId>) -> Self {
        self.trade = trade;
        self
    }

    /// Gehört zum tragenden Kern.
    pub fn core(mut self) -> Self {
        self.core = true;
        self
    }

    /// Gleiche Schicht aus einem anderen Baustoff.
    pub fn with_material(mut self, material: MaterialId) -> Self {
        self.material = material;
        self
    }
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

/// Art eines Bauteiltyps: für welche Bauteile er taugt (K1). Wandarten
/// haben Werkstypen und einen Standardtyp; Decke, Sohlplatte und
/// Frostschürze kommen ohne Typ aus (gedachter Einschicht-Aufbau,
/// [`crate::Model::element_layers`]) und können einen haben (R4). Die
/// Dachterrasse hat immer einen, ohne Kern (Werkstyp „Dachterrasse 14“).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum TypeCategory {
    ExteriorWall,
    InteriorWall,
    Floor,
    GroundSlab,
    StripFooting,
    RoofTerrace,
}

impl TypeCategory {
    pub const ALL: [TypeCategory; 6] = [
        TypeCategory::ExteriorWall,
        TypeCategory::InteriorWall,
        TypeCategory::Floor,
        TypeCategory::GroundSlab,
        TypeCategory::StripFooting,
        TypeCategory::RoofTerrace,
    ];

    /// Wandarten: nur sie haben Werkstypen, Standardtyp, Werkzeug und einen
    /// Platz im Katalogfenster.
    pub const WALLS: [TypeCategory; 2] = [TypeCategory::ExteriorWall, TypeCategory::InteriorWall];

    /// Typart, die zu Bauteilen der Kategorie passt; `None`: ohne Typ.
    pub fn of(c: Category) -> Option<TypeCategory> {
        crate::kinds::spec(c).type_category
    }

    /// Wandart (Schichten von außen nach innen).
    pub fn is_wall(self) -> bool {
        Self::WALLS.contains(&self)
    }

    /// Waagerechte Art (Schichten von oben nach unten) mit genau einer
    /// Kernschicht, deren Dicke am Bauteil steht (Regel 38).
    pub fn variable_core(self) -> bool {
        !self.is_wall() && self != TypeCategory::RoofTerrace
    }

    /// Kategorie der Bauteile dieser Typart.
    pub fn category(self) -> Category {
        match self {
            TypeCategory::ExteriorWall => Category::ExteriorWall,
            TypeCategory::InteriorWall => Category::InteriorWall,
            TypeCategory::Floor => Category::Floor,
            TypeCategory::GroundSlab => Category::GroundSlab,
            TypeCategory::StripFooting => Category::StripFooting,
            TypeCategory::RoofTerrace => Category::RoofTerrace,
        }
    }

    pub fn name(self) -> &'static str {
        crate::kinds::spec(self.category()).name
    }

    /// Präfix des Kurzzeichens, z. B. „AW“ für AW-1.
    pub fn prefix(self) -> &'static str {
        crate::kinds::spec(self.category()).prefix
    }
}

/// Dicken von Belag und Dämmung der Dachterrasse (mm, Steckbrief DT §2).
pub const TERRACE_FINISH: (f64, f64) = (20.0, 150.0);
pub const TERRACE_INSULATION: (f64, f64) = (40.0, 300.0);

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

/// Bauteiltyp (IFC: IfcWallType bzw. IfcSlabType + IfcMaterialLayerSet).
/// Schichten bei Wänden von außen nach innen, bei waagerechten Typen von
/// oben nach unten. Bauteile verweisen darauf und kopieren nie Schichten.
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

/// Kerndicke, ab der eine Innenwand trägt (Regel 100), in mm.
pub const BEARING_CORE_MIN: f64 = 175.0;

/// Regel 100, eine Quelle für tragend (IFC LoadBearing, Deckenauflager) und
/// die Kostengruppe 341/342: eine tragende Kernschicht und (keine
/// Innenwand oder Kern ≥ 175 mm). IW-11,5 trägt damit nicht, IW-24 schon.
pub fn bears(layers: &[MaterialLayer], interior: bool) -> bool {
    let structure = layers
        .iter()
        .any(|l| l.core && l.function == LayerFunction::Structure);
    let core: f64 = layers.iter().filter(|l| l.core).map(|l| l.thickness).sum();
    structure && (!interior || core >= BEARING_CORE_MIN - 1e-6)
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

    /// Tragend (IFC LoadBearing, Auflager der Decke) nach Regel 100: siehe
    /// [`bears`].
    pub fn load_bearing(&self) -> bool {
        bears(&self.layers, self.category == TypeCategory::InteriorWall)
    }

    /// Außenbauteil (IFC IsExternal), aus der Typart.
    pub fn is_external(&self) -> bool {
        self.category == TypeCategory::ExteriorWall
    }

    /// Verstöße gegen die Regeln eines Typs: mindestens eine Schicht, jede
    /// dicker als 0, Kernschichten zusammenhängend, Kurzzeichen nicht leer;
    /// Luftschichten nach Regel 20; waagerechte Typen mit genau einer
    /// Kernschicht (Regel 38).
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
        // Regel 38: die Kernschicht trägt die Dicke des Bauteils
        if self.category.variable_core() && core.len() != 1 {
            out.push(format!("Typ {who}: braucht genau eine Kernschicht"));
        }
        // Regel 39: die Dachterrasse liegt auf der Rohdecke, ohne Kern; Belag
        // und Dämmung in den Grenzen des Paneels (Steckbrief DT §2)
        if self.category == TypeCategory::RoofTerrace {
            if !core.is_empty() {
                out.push(format!("Typ {who}: Dachterrasse ohne Kernschicht"));
            }
            for l in &self.layers {
                let range = match l.function {
                    LayerFunction::Finish => Some(TERRACE_FINISH),
                    LayerFunction::Insulation => Some(TERRACE_INSULATION),
                    _ => None,
                };
                if let Some((lo, hi)) = range.filter(|r| !(r.0..=r.1).contains(&l.thickness)) {
                    out.push(format!(
                        "Typ {who}: Schicht {} bis {} cm",
                        cm_de(lo),
                        cm_de(hi)
                    ));
                }
            }
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
        out
    }

    /// Bereich der Auflagertiefe für „fest, Rest Randstreifen“ (mm ab
    /// Innenseite, Regel 21): mindestens die inneren Nicht-Kern-Schichten
    /// plus 10 cm, höchstens Wanddicke minus 2 cm. `None`: nicht möglich
    /// (keine Außenwand, Schicht vor dem Kern oder Bereich leer).
    pub fn bearing_range(&self) -> Option<(f64, f64)> {
        if self.category != TypeCategory::ExteriorWall || !self.layers.first()?.core {
            return None;
        }
        let last = self.layers.iter().rposition(|l| l.core)?;
        let inner: f64 = self.layers[last + 1..].iter().map(|l| l.thickness).sum();
        let (lo, hi) = (inner + 100.0, self.thickness() - 20.0);
        (lo <= hi + 1e-9).then_some((lo, hi))
    }

    /// Regel 21 ohne den Baustoff des Streifens ([`crate::Model::bearing_problem`]
    /// prüft auch ihn): Auflagertiefe im Bereich [`LayerSet::bearing_range`].
    pub fn bearing_problem(&self) -> Option<String> {
        let Bearing::Depth { depth, .. } = self.bearing else {
            return None;
        };
        let who = if self.code.is_empty() {
            self.name.as_str()
        } else {
            self.code.as_str()
        };
        match self.bearing_range() {
            None => Some(format!(
                "Typ {who}: Deckenauflager nur bei Außenwänden ohne Schichten vor der tragenden Schicht"
            )),
            Some((lo, hi)) if !(depth >= lo - 1e-6 && depth <= hi + 1e-6) => Some(format!(
                "Typ {who}: Auflagertiefe {} bis {} cm",
                cm_de(lo),
                cm_de(hi)
            )),
            _ => None,
        }
    }
}

/// Darstellungsschlüssel eines Baustoffs in Körpern ([`crate::Tri::mat`]):
/// Platz in der Bibliothek plus 1; 0 heißt „ohne Baustoff“.
pub fn material_key(id: MaterialId) -> u16 {
    debug_assert!(id.index() + 1 < material::CUT as u32);
    (id.index() + 1) as u16
}

/// Zentimeter mit Komma, ohne Einheit („24“, „40,5“).
fn cm_de(mm: f64) -> String {
    let cm = (mm / 5.0).round() * 0.5;
    if cm.fract() == 0.0 {
        format!("{cm:.0}")
    } else {
        format!("{cm:.1}").replace('.', ",")
    }
}
