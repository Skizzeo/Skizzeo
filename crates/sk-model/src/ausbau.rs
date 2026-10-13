//! Ausbau des Raumbuchs (raumbuch/plan.md §4.2, Schritt RB1): eingebaute
//! Baustoffe und Werks-Ausbautypen für Boden, Wand, Decke und Sockel.
//!
//! Wie Dachterrasse und Flachdach haben Baustoffe und Typen feste Guids und
//! werden erst angelegt, wenn sie gebraucht werden ([`Model::ensure_finish_type`]):
//! Ein Projekt ohne Ausbau schreibt dieselben Bytes wie vorher. Die
//! Vorbelegung ist Praxis, keine Norm (Jörn 11.10. 17:03: Estrich 14 cm,
//! Belag 2 bzw. 1,5 cm, Innenputz 1,5 cm Gips- oder Kalkzementputz mit
//! Anstrich oder Tapete).

use super::*;
use crate::proctex::{Palette, Pattern};

/// Gewerk des eingebauten Ausbaubaustoffs mit der Guid `g` (`None`: keiner).
pub(super) fn finish_trade(g: Guid) -> Option<TradeId> {
    let m = FINISH_MATERIALS.iter().find(|m| m.guid == g)?;
    trade::start_id(m.trade)
}

/// Ein eingebauter Baustoff des Ausbaus mit seiner Darstellung.
struct FinishMaterial {
    guid: Guid,
    name: &'static str,
    category: MatCategory,
    priority: u16,
    density: f64,
    lambda: Option<f64>,
    color: [u8; 3],
    cut_color: [u8; 3],
    /// ATV-Nummer des Gewerks.
    trade: &'static str,
    /// Feste Guid der Oberfläche; ihr Werksmuster steht in [`finish_pattern`].
    surface: Guid,
}

pub const GYPSUM_PLASTER_GUID: Guid = Guid(0x225cc949e110870cf5079e92438a9394);
pub const LIME_PLASTER_GUID: Guid = Guid(0x8d332d09d4c0f4179a5c6b51add6e6a1);
pub const SCREED_MAT_GUID: Guid = Guid(0xaed99449deb2fef9fec089795aaf9a36);
pub const IMPACT_INSULATION_GUID: Guid = Guid(0x79d989a97d3718eacca325e03e2e26c8);
pub const FLOOR_INSULATION_GUID: Guid = Guid(0xcc48c6d387da9c3c6d6c1bdb8344e2ea);
pub const SLAB_SEAL_GUID: Guid = Guid(0x8e3177a26da1cbb3efbdfb57b3bc44f1);
pub const PORCELAIN_GUID: Guid = Guid(0x7d41118ae25cb29dcc2357afb583c437);
pub const WALL_TILE_GUID: Guid = Guid(0x3012a1685e7adecd8916d32513977b73);
pub const TILE_SEAL_GUID: Guid = Guid(0xe25a9e08ae4a4a1fb8f01ce8bff3a258);
pub const PARQUET_GUID: Guid = Guid(0xe20498098c4de54896ee6476645dbb3c);
pub const WOODCHIP_GUID: Guid = Guid(0x43030e65343b147cd3a4d8f507ae0ae6);
pub const PAINT_GUID: Guid = Guid(0x2fb4399ad1081a1a7ddddac1de1c1e7e);
pub const SKIRTING_MAT_GUID: Guid = Guid(0x58354fa37e9658353fa5dcfee08998c8);

const GYPSUM_SURFACE: Guid = Guid(0x9a747baa782f5bab282653297fa1fc89);
const LIME_SURFACE: Guid = Guid(0xbf1624afdac6e9c34776c3f6bd8a3be5);
const SCREED_SURFACE: Guid = Guid(0xd0a6a8ef055935dbb2e81d414e273afd);
const IMPACT_SURFACE: Guid = Guid(0x90ac2d5e1a245af479e6c7cd1457ba68);
const FLOOR_INSULATION_SURFACE: Guid = Guid(0x8ccfe351a6c45225695265c3c91c5cf4);
const SLAB_SEAL_SURFACE: Guid = Guid(0x4adae4c7cbd49b535d382cef8e6ee35a);
const PORCELAIN_SURFACE: Guid = Guid(0xa918efe9bf6f62da5bc642da3132c43e);
const WALL_TILE_SURFACE: Guid = Guid(0xdf12fb2dede8a0d6d1c06da9dfa5fc5a);
const TILE_SEAL_SURFACE: Guid = Guid(0x687f2f9a80178e59e24ffbf9ce44e16b);
const PARQUET_SURFACE: Guid = Guid(0xf9430d80051bcd9d9136e623c6bac867);
const WOODCHIP_SURFACE: Guid = Guid(0x26b708b22d7d88bdf1b82df6c468041e);
const PAINT_SURFACE: Guid = Guid(0xc6438dc4f82abb495e57ea50a3ba4d13);
const SKIRTING_SURFACE: Guid = Guid(0x3098c1a2bd0868199668b513664b1e40);

/// Die eingebauten Baustoffe des Ausbaus. Estrich und seine Dämmung legt
/// der Estrichleger, die Verbundabdichtung der Fliesenleger, die
/// Abdichtung der Sohlplatte der Abdichter (DIN 18533).
const FINISH_MATERIALS: [FinishMaterial; 13] = [
    FinishMaterial {
        guid: GYPSUM_PLASTER_GUID,
        name: "Gipsputz",
        category: MatCategory::Plaster,
        priority: 100,
        density: 1200.0,
        lambda: Some(0.51),
        color: [244, 243, 240],
        cut_color: [214, 212, 206],
        trade: "18350",
        surface: GYPSUM_SURFACE,
    },
    FinishMaterial {
        guid: LIME_PLASTER_GUID,
        name: "Kalkzementputz",
        category: MatCategory::Plaster,
        priority: 100,
        density: 1800.0,
        lambda: Some(1.0),
        color: [236, 234, 228],
        cut_color: [206, 203, 196],
        trade: "18350",
        surface: LIME_SURFACE,
    },
    FinishMaterial {
        guid: SCREED_MAT_GUID,
        name: "Zementestrich CT-C25-F4",
        category: MatCategory::Screed,
        priority: 400,
        density: 2000.0,
        lambda: Some(1.4),
        color: [190, 188, 182],
        cut_color: [176, 174, 168],
        trade: "18353",
        surface: SCREED_SURFACE,
    },
    FinishMaterial {
        guid: IMPACT_INSULATION_GUID,
        name: "Trittschalldämmung EPS-T",
        category: MatCategory::Insulation,
        priority: 300,
        density: 15.0,
        lambda: Some(0.040),
        color: [244, 244, 236],
        cut_color: [232, 224, 180],
        trade: "18353",
        surface: IMPACT_SURFACE,
    },
    FinishMaterial {
        guid: FLOOR_INSULATION_GUID,
        name: "Wärmedämmung EPS 035 DEO",
        category: MatCategory::Insulation,
        priority: 300,
        density: 20.0,
        lambda: Some(0.035),
        color: [240, 242, 246],
        cut_color: [214, 222, 236],
        trade: "18353",
        surface: FLOOR_INSULATION_SURFACE,
    },
    FinishMaterial {
        guid: SLAB_SEAL_GUID,
        name: "Abdichtung Bitumenbahn, Sohlplatte",
        category: MatCategory::Membrane,
        priority: 200,
        density: 1100.0,
        lambda: Some(0.17),
        color: [62, 62, 64],
        cut_color: [72, 72, 74],
        trade: "18336",
        surface: SLAB_SEAL_SURFACE,
    },
    FinishMaterial {
        guid: PORCELAIN_GUID,
        name: "Feinsteinzeug",
        category: MatCategory::Ceramic,
        priority: 500,
        density: 2300.0,
        lambda: Some(1.3),
        color: [176, 174, 170],
        cut_color: [160, 158, 154],
        trade: "18352",
        surface: PORCELAIN_SURFACE,
    },
    FinishMaterial {
        guid: WALL_TILE_GUID,
        name: "Wandfliese",
        category: MatCategory::Ceramic,
        priority: 500,
        density: 2000.0,
        lambda: Some(1.0),
        color: [244, 244, 242],
        cut_color: [220, 220, 216],
        trade: "18352",
        surface: WALL_TILE_SURFACE,
    },
    FinishMaterial {
        guid: TILE_SEAL_GUID,
        name: "Verbundabdichtung",
        category: MatCategory::Membrane,
        priority: 200,
        density: 1400.0,
        lambda: None,
        color: [120, 140, 170],
        cut_color: [120, 140, 170],
        trade: "18352",
        surface: TILE_SEAL_SURFACE,
    },
    FinishMaterial {
        guid: PARQUET_GUID,
        name: "Parkett Eiche",
        category: MatCategory::Flooring,
        priority: 500,
        density: 700.0,
        lambda: Some(0.18),
        color: [196, 160, 112],
        cut_color: [180, 146, 100],
        trade: "18356",
        surface: PARQUET_SURFACE,
    },
    FinishMaterial {
        guid: WOODCHIP_GUID,
        name: "Raufaser",
        category: MatCategory::Coating,
        priority: 0,
        density: 300.0,
        lambda: None,
        color: [248, 247, 244],
        cut_color: [248, 247, 244],
        trade: "18366",
        surface: WOODCHIP_SURFACE,
    },
    FinishMaterial {
        guid: PAINT_GUID,
        name: "Dispersionsanstrich",
        category: MatCategory::Coating,
        priority: 0,
        density: 1000.0,
        lambda: None,
        color: [250, 250, 248],
        cut_color: [250, 250, 248],
        trade: "18363",
        surface: PAINT_SURFACE,
    },
    FinishMaterial {
        guid: SKIRTING_MAT_GUID,
        name: "Sockelleiste Eiche",
        category: MatCategory::Timber,
        priority: 500,
        density: 700.0,
        lambda: Some(0.18),
        color: [186, 150, 104],
        cut_color: [170, 136, 94],
        trade: "18356",
        surface: SKIRTING_SURFACE,
    },
];

/// Name und Art der eingebauten Baustoffe des Ausbaus, für
/// [`crate::model::BUILTIN_MATERIALS`].
pub(super) const fn finish_material(i: usize) -> (Guid, &'static str, MatCategory) {
    let m = &FINISH_MATERIALS[i];
    (m.guid, m.name, m.category)
}

/// Werksmuster der Oberfläche eines Ausbau-Baustoffs (Texturgenerator):
/// Parkett als Dielen im Halbverband, Feinsteinzeug 60 × 60, Wandfliese
/// 30 × 60, Raufaser und Putze als feines Korn. Estrich, Dämmung,
/// Abdichtung, Anstrich und Sockelleiste zeigen ihre Farbe.
pub(crate) fn finish_pattern(g: Guid) -> Option<Pattern> {
    let tiles = |len, wid, joint, half, joint_rgb, palette: Palette, spread, seed| Pattern::Tiles {
        len,
        wid,
        joint,
        half,
        joint_rgb,
        palette,
        spread,
        seed,
    };
    let plaster = |grain, spread, seed| Pattern::Plaster {
        grain,
        spread,
        seed,
    };
    Some(match g {
        GYPSUM_SURFACE => plaster(0.5, 2.0, 5),
        LIME_SURFACE => plaster(1.0, 3.0, 6),
        WOODCHIP_SURFACE => plaster(0.8, 4.0, 7),
        PORCELAIN_SURFACE => tiles(
            600.0,
            600.0,
            3.0,
            false,
            [140, 138, 134],
            [
                ([176, 174, 170], 50.0),
                ([168, 166, 162], 30.0),
                ([184, 182, 178], 20.0),
            ],
            3.0,
            41,
        ),
        WALL_TILE_SURFACE => tiles(
            600.0,
            300.0,
            2.0,
            false,
            [206, 206, 204],
            [
                ([244, 244, 242], 70.0),
                ([238, 238, 236], 30.0),
                ([0, 0, 0], 0.0),
            ],
            2.0,
            43,
        ),
        PARQUET_SURFACE => tiles(
            1200.0,
            140.0,
            2.0,
            true,
            [150, 118, 80],
            [
                ([196, 160, 112], 40.0),
                ([184, 148, 100], 35.0),
                ([206, 172, 124], 25.0),
            ],
            8.0,
            47,
        ),
        _ => return None,
    })
}

/// Ein Werks-Ausbautyp (raumbuch/plan.md §4.2): Schichten als Baustoff,
/// Dicke in mm und Aufgabe, von oben nach unten bzw. vom Raum zum Rohbau.
pub struct FinishType {
    pub guid: Guid,
    pub name: &'static str,
    pub code: &'static str,
    pub category: TypeCategory,
    pub layers: &'static [(Guid, f64, LayerFunction)],
    /// Höhe der Sockelleiste (mm).
    pub height: Option<f64>,
}

pub const PARQUET_TYPE_GUID: Guid = Guid(0xf4d88a7b269313ae7b4ca83b09ee4e08);
pub const PORCELAIN_TYPE_GUID: Guid = Guid(0xcad1226a57ac8f71d5dd23d3a89116a3);
pub const WET_FLOOR_TYPE_GUID: Guid = Guid(0x397bcbc5d15f9ec775fc44352761110b);
pub const SCREED_FLOOR_TYPE_GUID: Guid = Guid(0xa78f96b612169de0a5a56bbd96bb7f3c);
pub const SCREED_SLAB_TYPE_GUID: Guid = Guid(0x979e89354c9d7a3e2f0240dc79f94f56);
pub const WOODCHIP_WALL_TYPE_GUID: Guid = Guid(0x12e202e700e828cb08004dc80dc201dd);
pub const GYPSUM_WALL_TYPE_GUID: Guid = Guid(0xcc76c1a99829ee772a34452cc3414b47);
pub const LIME_WALL_TYPE_GUID: Guid = Guid(0x13e3309f030c9e093531bc65b7e413a6);
pub const WALL_TILES_TYPE_GUID: Guid = Guid(0x1dc80968239ad440d027c1f971b71b89);
pub const CEILING_TYPE_GUID: Guid = Guid(0xd3c6c8efc9d30a9f2d578b344a697221);
pub const OAK_SKIRTING_TYPE_GUID: Guid = Guid(0xcc5c189f4c92fd11c6021ab2e3408300);
pub const TILE_SKIRTING_TYPE_GUID: Guid = Guid(0xc7871507c4d6ca1de2482077061095ea);

use LayerFunction as F;

/// Die Werks-Ausbautypen. Parkett 20 und Estrichaufbau 140 ergeben den
/// Fußbodenaufbau 160 mm; OK Fertigfußboden liegt im Geschoss überall
/// gleich hoch (Jörn 11.10. 17:39), unter dünnerem Belag gleicht im Raum
/// die Dämmung unter dem Estrich aus (Raumbuch-Rechnung, RB3).
pub const FINISH_TYPES: [FinishType; 12] = [
    FinishType {
        guid: PARQUET_TYPE_GUID,
        name: "Parkett Eiche 20",
        code: "BB-PA",
        category: TypeCategory::FloorFinish,
        layers: &[(PARQUET_GUID, 20.0, F::Finish)],
        height: None,
    },
    FinishType {
        guid: PORCELAIN_TYPE_GUID,
        name: "Feinsteinzeug 60 × 60",
        code: "BB-FS",
        category: TypeCategory::FloorFinish,
        // im Dünnbett, Kleber in der Dicke
        layers: &[(PORCELAIN_GUID, 15.0, F::Finish)],
        height: None,
    },
    FinishType {
        guid: WET_FLOOR_TYPE_GUID,
        name: "Feinsteinzeug mit Verbundabdichtung",
        code: "BB-FA",
        category: TypeCategory::FloorFinish,
        // Bad und Gäste-WC: DIN 18534, W1-I
        layers: &[
            (PORCELAIN_GUID, 13.0, F::Finish),
            (TILE_SEAL_GUID, 2.0, F::Membrane),
        ],
        height: None,
    },
    FinishType {
        guid: SCREED_FLOOR_TYPE_GUID,
        name: "Estrich 14 auf Decke",
        code: "ES-DE",
        category: TypeCategory::Screed,
        layers: &[
            (SCREED_MAT_GUID, 65.0, F::Finish),
            (IMPACT_INSULATION_GUID, 30.0, F::Insulation),
            (FLOOR_INSULATION_GUID, 45.0, F::Insulation),
        ],
        height: None,
    },
    FinishType {
        guid: SCREED_SLAB_TYPE_GUID,
        name: "Estrich 14 auf Sohlplatte",
        code: "ES-SP",
        category: TypeCategory::Screed,
        layers: &[
            (SCREED_MAT_GUID, 65.0, F::Finish),
            (IMPACT_INSULATION_GUID, 30.0, F::Insulation),
            (FLOOR_INSULATION_GUID, 40.0, F::Insulation),
            (SLAB_SEAL_GUID, 5.0, F::Membrane),
        ],
        height: None,
    },
    FinishType {
        guid: WOODCHIP_WALL_TYPE_GUID,
        name: "Gipsputz mit Raufaser und Anstrich",
        code: "WB-RF",
        category: TypeCategory::WallFinish,
        layers: &[
            (PAINT_GUID, 0.0, F::Finish),
            (WOODCHIP_GUID, 0.0, F::Finish),
            (GYPSUM_PLASTER_GUID, 15.0, F::Finish),
        ],
        height: None,
    },
    FinishType {
        guid: GYPSUM_WALL_TYPE_GUID,
        name: "Gipsputz mit Anstrich",
        code: "WB-GP",
        category: TypeCategory::WallFinish,
        layers: &[
            (PAINT_GUID, 0.0, F::Finish),
            (GYPSUM_PLASTER_GUID, 15.0, F::Finish),
        ],
        height: None,
    },
    FinishType {
        guid: LIME_WALL_TYPE_GUID,
        name: "Kalkzementputz mit Anstrich",
        code: "WB-KZ",
        category: TypeCategory::WallFinish,
        layers: &[
            (PAINT_GUID, 0.0, F::Finish),
            (LIME_PLASTER_GUID, 15.0, F::Finish),
        ],
        height: None,
    },
    FinishType {
        guid: WALL_TILES_TYPE_GUID,
        name: "Wandfliesen 30 × 60",
        code: "WF-1",
        category: TypeCategory::WallTiles,
        // im Dünnbett auf dem Kalkzementputz, Kleber in der Dicke
        layers: &[(WALL_TILE_GUID, 10.0, F::Finish)],
        height: None,
    },
    FinishType {
        guid: CEILING_TYPE_GUID,
        name: "Deckenputz Gips mit Anstrich",
        code: "DB-GP",
        category: TypeCategory::CeilingFinish,
        layers: &[
            (PAINT_GUID, 0.0, F::Finish),
            (GYPSUM_PLASTER_GUID, 10.0, F::Finish),
        ],
        height: None,
    },
    FinishType {
        guid: OAK_SKIRTING_TYPE_GUID,
        name: "Sockelleiste Eiche 60 × 15",
        code: "SL-HO",
        category: TypeCategory::Skirting,
        layers: &[(SKIRTING_MAT_GUID, 15.0, F::Finish)],
        height: Some(60.0),
    },
    FinishType {
        guid: TILE_SKIRTING_TYPE_GUID,
        name: "Fliesensockel 60",
        code: "SL-FL",
        category: TypeCategory::Skirting,
        layers: &[(PORCELAIN_GUID, 10.0, F::Finish)],
        height: Some(60.0),
    },
];

impl Model {
    /// Eingebauter Baustoff des Ausbaus mit der Guid `g`; fehlt er, wird er
    /// samt Oberfläche angelegt, mit Werksmuster an ihrer festen Guid
    /// ([`crate::proctex::factory_for`]). `None` bei fremder Guid.
    pub fn ensure_finish_material(&mut self, g: Guid) -> Option<MaterialId> {
        if let Some(id) = self.material_by_guid(g) {
            return Some(id);
        }
        let m = FINISH_MATERIALS.iter().find(|m| m.guid == g)?;
        self.builtin_material_with(
            m.guid,
            m.name,
            m.category,
            m.priority,
            m.density,
            m.lambda,
            m.color,
            m.cut_color,
            trade::start_id(m.trade),
            None,
            Some(m.surface),
        )
    }

    /// Lässt sich der Werks-Ausbautyp `t` anlegen? Nicht, wenn das Büro einen
    /// seiner Werksbaustoffe umgestellt hat (z. B. den Anstrich auf eine
    /// andere Art): Dann entstünde ein ungültiger Typ.
    pub(super) fn finish_type_creatable(&self, t: &FinishType) -> bool {
        let art = |g: Guid| match self.material_by_guid(g) {
            Some(id) => self.material(id).map(|m| m.category),
            None => FINISH_MATERIALS
                .iter()
                .find(|m| m.guid == g)
                .map(|m| m.category),
        };
        !t.layers.iter().any(|&(mat, thickness, _)| {
            art(mat).is_none_or(|c| thickness == 0.0 && c != MatCategory::Coating)
        })
    }

    /// Werks-Ausbautyp mit der Guid `g` ([`FINISH_TYPES`]); fehlt er, wird
    /// er samt Baustoffen angelegt. Ein belegtes Kurzzeichen weicht auf das
    /// nächste freie aus. `None` bei fremder Guid.
    pub fn ensure_finish_type(&mut self, g: Guid) -> Option<LayerSetId> {
        if let Some(id) = self.type_by_guid(g) {
            return Some(id);
        }
        let t = FINISH_TYPES.iter().find(|t| t.guid == g)?;
        // Erst prüfen, dann anlegen: kein ungültiger Typ, kein verwaister
        // Baustoff
        if !self.finish_type_creatable(t) {
            return None;
        }
        let mut layers = Vec::new();
        for &(mat, thickness, function) in t.layers {
            let material = self.ensure_finish_material(mat)?;
            layers.push(MaterialLayer::new(material, thickness, function));
        }
        let set = LayerSet {
            guid: t.guid,
            name: t.name.into(),
            code: self.free_code(t.code),
            category: t.category,
            layers,
            props: PropSet::new(),
            note: String::new(),
            changed: 1,
            bearing: Bearing::Core,
            height: t.height,
        };
        self.add_layer_set(set)
    }

    /// Regel des Ausbaus mit den Baustoffen des Projekts
    /// ([`LayerSet::coat_problem`]): eine Schicht ohne Dicke ist eine
    /// Beschichtung. Wie das Deckenauflager: [`Model::add_layer_set`] und
    /// [`Model::set_layer_set`] lehnen ab, ein Typ aus Datei oder
    /// Firmenkatalog kommt mit und wird gemeldet.
    pub fn finish_problems(&self, set: &LayerSet) -> Option<String> {
        set.coat_problem(|id| self.material(id).map(|m| m.category))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::txn::Direction;
    use crate::{szo, GuidGen};

    /// Projekt mit allen Werks-Ausbautypen, in einem Schritt angelegt.
    fn mit_ausbau() -> (Model, Txn) {
        let mut m = Model::with_seed(71);
        m.require_steps();
        m.begin("Ausbau");
        for t in &FINISH_TYPES {
            assert!(m.ensure_finish_type(t.guid).is_some(), "{}", t.name);
        }
        let tx = m.commit().expect("Schritt");
        (m, tx)
    }

    /// Jeder Werks-Ausbautyp entsteht samt Baustoffen, hält die Typregeln
    /// und das Modell bleibt ohne Befund; zweimal anlegen ändert nichts.
    #[test]
    fn werkstypen_anlegen() {
        let (mut m, _) = mit_ausbau();
        assert!(m.check().is_empty(), "{:?}", m.check());
        for t in &FINISH_TYPES {
            let id = m.type_by_guid(t.guid).unwrap();
            let set = m.layer_set(id).unwrap();
            assert_eq!(set.code, t.code);
            assert_eq!(set.category, t.category);
            assert!(set.category.is_finish());
            assert!(set.problems().is_empty(), "{}", t.name);
            assert_eq!(set.height, t.height);
        }
        let n = (
            m.layer_sets().len(),
            m.materials().len(),
            m.attr().surfaces().len(),
        );
        m.begin("noch einmal");
        for t in &FINISH_TYPES {
            m.ensure_finish_type(t.guid);
        }
        assert!(m.commit().is_none(), "nichts geändert");
        assert_eq!(
            n,
            (
                m.layer_sets().len(),
                m.materials().len(),
                m.attr().surfaces().len()
            )
        );
    }

    /// Fußbodenaufbau 160 mm aus Parkett 20 und Estrichaufbau 140; die
    /// Werte aus Jörns Nachricht 17:03 (Estrich 14, Belag 2 bzw. 1,5,
    /// Innenputz 1,5).
    #[test]
    fn dicken_nach_vorgabe() {
        let (m, _) = mit_ausbau();
        let dicke = |g| m.layer_set(m.type_by_guid(g).unwrap()).unwrap().thickness();
        assert_eq!(
            dicke(PARQUET_TYPE_GUID) + dicke(SCREED_FLOOR_TYPE_GUID),
            160.0
        );
        assert_eq!(dicke(SCREED_SLAB_TYPE_GUID), 140.0);
        assert_eq!(dicke(PORCELAIN_TYPE_GUID), 15.0);
        assert_eq!(dicke(WET_FLOOR_TYPE_GUID), 15.0);
        for g in [
            WOODCHIP_WALL_TYPE_GUID,
            GYPSUM_WALL_TYPE_GUID,
            LIME_WALL_TYPE_GUID,
        ] {
            assert_eq!(dicke(g), 15.0);
        }
        assert_eq!(dicke(CEILING_TYPE_GUID), 10.0);
    }

    /// Gewerk je Baustoff nach VOB/C; Beschichtungen ohne Dicke, alle
    /// anderen Schichten mit; Werksmuster gültig und an der Oberfläche.
    #[test]
    fn baustoffe_gewerke_muster() {
        let (m, _) = mit_ausbau();
        for f in &FINISH_MATERIALS {
            let id = m.material_by_guid(f.guid).unwrap();
            let x = m.material(id).unwrap();
            assert_eq!(x.name, f.name);
            assert_eq!(x.trade, trade::start_id(f.trade), "{}", f.name);
            assert!(x.trade.is_some(), "{}", f.name);
            let s = m.attr().surface(x.surface).unwrap();
            assert_eq!(s.guid, f.surface);
            assert_eq!(s.pattern, finish_pattern(f.surface), "{}", f.name);
            if let Some(p) = &s.pattern {
                assert!(crate::proctex::validate(p).is_ok(), "{}", f.name);
            }
        }
        for t in &FINISH_TYPES {
            for &(g, d, _) in t.layers {
                let coating = FINISH_MATERIALS
                    .iter()
                    .find(|f| f.guid == g)
                    .unwrap()
                    .category
                    == MatCategory::Coating;
                assert_eq!(d == 0.0, coating, "{}", t.name);
            }
        }
        let parkett = m.material_by_guid(PARQUET_GUID).unwrap();
        assert_eq!(
            m.material(parkett).unwrap().trade,
            trade::start_id("18356"),
            "Parkett 18356"
        );
    }

    /// Feste Guids, eindeutige Namen und Kurzzeichen; der Werksbestand
    /// kennt alle eingebauten Baustoffe des Ausbaus (`mat=` der Kosten).
    #[test]
    fn guids_eindeutig() {
        let mut g: Vec<Guid> = FINISH_MATERIALS
            .iter()
            .flat_map(|f| [f.guid, f.surface])
            .chain(FINISH_TYPES.iter().map(|t| t.guid))
            .collect();
        let n = g.len();
        g.sort();
        g.dedup();
        assert_eq!(g.len(), n);
        let werk = Model::new();
        for x in &g {
            assert!(werk.material_by_guid(*x).is_none());
            assert!(werk.type_by_guid(*x).is_none());
        }
        let mut codes: Vec<&str> = FINISH_TYPES.iter().map(|t| t.code).collect();
        codes.sort();
        codes.dedup();
        assert_eq!(codes.len(), FINISH_TYPES.len());
        for f in &FINISH_MATERIALS {
            assert!(
                BUILTIN_MATERIALS.contains(&(f.guid, f.name, f.category)),
                "{}",
                f.name
            );
        }
    }

    /// Speichern und Öffnen ergibt dieselben Bytes; Typart, Höhe der
    /// Sockelleiste, Schichten ohne Dicke und Werksmuster kommen zurück.
    #[test]
    fn rundlauf() {
        let (m, _) = mit_ausbau();
        let text = szo::write(&m);
        assert!(text.contains(" cat=skirting changed=1 h=60 "));
        assert!(text.contains(" cat=floorfinish "));
        assert!(text.contains(" cat=coating "));
        let l = szo::read(&text, GuidGen::with_seed(5)).unwrap();
        assert!(l.hints.is_empty(), "{:?}", l.hints);
        assert_eq!(szo::write(&l.model), text);
        let back = &l.model;
        for t in &FINISH_TYPES {
            let a = m.layer_set(m.type_by_guid(t.guid).unwrap()).unwrap();
            let b = back.layer_set(back.type_by_guid(t.guid).unwrap()).unwrap();
            assert_eq!(a.height, b.height);
            assert_eq!(a.category, b.category);
            assert_eq!(a.layers.len(), b.layers.len());
        }
        assert!(back.check().is_empty(), "{:?}", back.check());
    }

    /// Ohne Ausbau bleibt die Datei, wie sie war: nichts angelegt, nichts
    /// geschrieben.
    #[test]
    fn ohne_ausbau_bytegleich() {
        let m = Model::with_seed(71);
        let text = szo::write(&m);
        for w in ["floorfinish", "screed", "skirting", "coating", "ceramic"] {
            assert!(!text.contains(w), "{w}");
        }
        assert!(text
            .lines()
            .filter(|l| l.starts_with("[layerset]"))
            .all(|l| !l.contains(" h=")));
    }

    /// Rückgängig nimmt Typen, Baustoffe und Oberflächen wieder weg.
    #[test]
    fn rueckgaengig() {
        let vorher = szo::write(&Model::with_seed(71));
        let (mut m, tx) = mit_ausbau();
        m.apply(&tx, Direction::Undo);
        assert_eq!(szo::write(&m), vorher);
        m.apply(&tx, Direction::Redo);
        assert!(m.type_by_guid(TILE_SKIRTING_TYPE_GUID).is_some());
        assert!(m.check().is_empty());
    }

    /// Neue Typen, Baustoffe und Raumvorlagen tragen keine Wand: Rückgängig
    /// und Wiederholen rechnen keinen Zug neu (Integration zu #116 und
    /// #112). Ein geänderter Typ dagegen trifft alle Züge, die ihn nutzen.
    #[test]
    fn neue_typen_ruehren_keine_wand_an() {
        let mut m = Model::with_seed(72);
        m.require_steps();
        m.begin("Ausbau");
        assert!(m.ensure_space_type(crate::model::BATH_SPACE_GUID).is_some());
        let tx = m.commit().expect("Schritt");
        for dir in [Direction::Undo, Direction::Redo] {
            let t = m.apply(&tx, dir);
            assert!(!t.library && t.runs.is_empty(), "{dir:?}");
        }
        let aw = m.defaults().exterior_wall;
        let mut typ = m.layer_set(aw).unwrap().clone();
        typ.layers[0].thickness += 10.0;
        m.begin("Typ geändert");
        assert!(m.set_layer_set(aw, typ));
        let tx = m.commit().expect("Schritt");
        assert!(m.apply(&tx, Direction::Undo).library);
    }

    /// Ein belegtes Kurzzeichen weicht aus; eine fremde Guid legt nichts an.
    #[test]
    fn kurzzeichen_belegt() {
        let mut m = Model::with_seed(71);
        let id = m.ensure_finish_type(PARQUET_TYPE_GUID).unwrap();
        let mut t = m.layer_set(id).unwrap().clone();
        t.guid = Guid(0x7a11);
        m.add_layer_set(LayerSet {
            code: "BB-FS".into(),
            ..t
        })
        .unwrap();
        let fs = m.ensure_finish_type(PORCELAIN_TYPE_GUID).unwrap();
        assert_eq!(m.layer_set(fs).unwrap().code, "BB-FS-2");
        assert!(m.ensure_finish_type(Guid(0x1234)).is_none());
        assert!(m.ensure_finish_material(Guid(0x1234)).is_none());
    }

    /// Hat das Büro den Anstrich auf eine andere Art umgestellt, entsteht
    /// kein Typ mit einer Putzschicht ohne Dicke.
    #[test]
    fn umgestellter_anstrich() {
        let mut m = Model::with_seed(71);
        let a = m.ensure_finish_material(PAINT_GUID).unwrap();
        let x = Material {
            category: MatCategory::Plaster,
            ..m.material(a).unwrap().clone()
        };
        assert!(m.set_material(a, x));
        let n = m.materials().len();
        assert!(m.ensure_finish_type(GYPSUM_WALL_TYPE_GUID).is_none());
        assert_eq!(m.materials().len(), n, "kein verwaister Baustoff");
        assert!(m.check().is_empty(), "{:?}", m.check());
    }

    /// Die Höhe der Sockelleiste gehört zum Inhalt des Typs: Ändern wirkt,
    /// der Abgleich mit dem Firmenkatalog sieht sie, Übernehmen holt sie
    /// (Review #80, Befund 1).
    #[test]
    fn sockelhoehe_aendern_und_abgleichen() {
        use crate::catalog::{compare, import_type, Library, TypeState};
        let mut m = Model::with_seed(71);
        let sl = m.ensure_finish_type(OAK_SKIRTING_TYPE_GUID).unwrap();
        let lib = Library::from_model(&m);
        let alt = m.layer_set(sl).unwrap().clone();
        let hoch = LayerSet {
            height: Some(100.0),
            ..alt.clone()
        };
        assert!(m.set_layer_set(sl, hoch));
        let neu = m.layer_set(sl).unwrap();
        assert_eq!(neu.height, Some(100.0));
        assert_eq!(neu.changed, alt.changed + 1);
        let stand = |m: &Model| {
            compare(m, &lib)
                .into_iter()
                .find(|x| x.0 == OAK_SKIRTING_TYPE_GUID)
                .map(|x| x.1)
        };
        assert_eq!(stand(&m), Some(TypeState::Differs));
        assert_eq!(import_type(&mut m, &lib, OAK_SKIRTING_TYPE_GUID), Some(sl));
        assert_eq!(m.layer_set(sl).unwrap().height, Some(60.0));
        assert_eq!(stand(&m), Some(TypeState::Same));
    }

    /// „Ohne Dicke nur Beschichtung“ gilt beim Anlegen und Ändern; aus dem
    /// Firmenkatalog kommt ein solcher Typ wie ein ungültiges
    /// Deckenauflager mit und wird gemeldet (Review #80, Befund 2).
    #[test]
    fn beschichtungsregel_beim_anlegen() {
        use crate::catalog::{import_type, read_szk, write_szk, Library};
        let mut m = Model::with_seed(71);
        let wb = m.ensure_finish_type(GYPSUM_WALL_TYPE_GUID).unwrap();
        let gut = m.layer_set(wb).unwrap().clone();
        let mut putz_ohne_dicke = gut.clone();
        putz_ohne_dicke.layers[1].thickness = 0.0;
        assert!(m.finish_problems(&putz_ohne_dicke).is_some());
        assert!(!m.set_layer_set(wb, putz_ohne_dicke.clone()));
        assert_eq!(m.layer_set(wb).unwrap().layers, gut.layers);
        let neu = LayerSet {
            guid: Guid(0x5151),
            code: "WB-X".into(),
            ..putz_ohne_dicke
        };
        assert!(m.add_layer_set(neu).is_none());
        assert!(m.check().is_empty(), "{:?}", m.check());
        // Firmenkatalog mit Gipsputz 0 mm: lesbar, gemeldet, übernommen
        let mut lib = Library::from_model(&m);
        let id = lib.type_by_guid(GYPSUM_WALL_TYPE_GUID).unwrap();
        lib.types.get_mut(id).unwrap().layers[1].thickness = 0.0;
        let lib = read_szk(&write_szk(&lib)).expect("lesbar");
        assert_eq!(lib.invalid_finishes(), vec!["WB-GP".to_string()]);
        assert_eq!(import_type(&mut m, &lib, GYPSUM_WALL_TYPE_GUID), Some(wb));
        assert!(m
            .check()
            .iter()
            .any(|p| p.contains("ohne Dicke nur eine Beschichtung")));
    }

    /// Wand- und Deckenbekleidung dürfen nur Beschichtung sein (Anstrich
    /// auf Sichtbeton); Estrich, Belag, Fliesen und Sockelleiste brauchen
    /// eine Dicke (Review #80, Hinweis 4).
    #[test]
    fn dicke_je_art() {
        let mut m = Model::with_seed(71);
        let anstrich = m.ensure_finish_material(PAINT_GUID).unwrap();
        for (g, nur_anstrich) in [
            (GYPSUM_WALL_TYPE_GUID, true),
            (CEILING_TYPE_GUID, true),
            (SCREED_FLOOR_TYPE_GUID, false),
            (PARQUET_TYPE_GUID, false),
            (WALL_TILES_TYPE_GUID, false),
            (OAK_SKIRTING_TYPE_GUID, false),
        ] {
            let id = m.ensure_finish_type(g).unwrap();
            let x = LayerSet {
                layers: vec![MaterialLayer::new(anstrich, 0.0, LayerFunction::Finish)],
                ..m.layer_set(id).unwrap().clone()
            };
            let p = x.problems();
            let dicke = p.iter().any(|p| p.contains("braucht eine Dicke"));
            assert_eq!(dicke, !nur_anstrich, "{}: {p:?}", x.code);
            assert!(m.finish_problems(&x).is_none());
            assert_eq!(m.set_layer_set(id, x), nur_anstrich);
        }
        assert!(m.check().is_empty(), "{:?}", m.check());
    }

    /// Regeln: ohne Dicke nur Beschichtung; Ausbau ohne Kern; Höhe nur und
    /// immer an der Sockelleiste, aus genau einer Schicht.
    #[test]
    fn regeln() {
        let mut m = Model::with_seed(71);
        let wb = m.ensure_finish_type(GYPSUM_WALL_TYPE_GUID).unwrap();
        let sl = m.ensure_finish_type(OAK_SKIRTING_TYPE_GUID).unwrap();
        let wand = m.layer_set(wb).unwrap().clone();
        let mut x = wand.clone();
        x.layers[1].thickness = 0.0;
        assert!(
            x.problems().is_empty(),
            "Ausbau darf Schichten ohne Dicke haben"
        );
        assert!(m.finish_problems(&x).is_some(), "aber nur Beschichtungen");
        assert!(m.finish_problems(&wand).is_none());
        let mut x = wand.clone();
        x.layers[1].core = true;
        assert!(x.problems().iter().any(|p| p.contains("ohne Kernschicht")));
        let mut x = wand.clone();
        x.height = Some(60.0);
        assert!(x
            .problems()
            .iter()
            .any(|p| p.contains("nur bei Sockelleisten")));
        let sockel = m.layer_set(sl).unwrap().clone();
        assert!(sockel.problems().is_empty());
        for h in [None, Some(10.0), Some(400.0)] {
            let x = LayerSet {
                height: h,
                ..sockel.clone()
            };
            assert!(
                x.problems().iter().any(|p| p.contains("Höhe 2 bis 30 cm")),
                "{h:?}"
            );
        }
        let mut x = sockel.clone();
        x.layers.push(x.layers[0]);
        assert!(x.problems().iter().any(|p| p.contains("aus einer Schicht")));
        // Wandtypen bleiben, wie sie waren: keine Schicht ohne Dicke
        let aw = m.layer_set(m.defaults().exterior_wall).unwrap().clone();
        let mut x = aw.clone();
        x.layers[0].thickness = 0.0;
        assert!(x
            .problems()
            .iter()
            .any(|p| p.contains("Schichtdicke ungültig")));
    }
}
