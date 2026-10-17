//! Das Gebäudemodell als Datenbank: Bibliothek, Geschosse, Bauteile, Wandzüge.
//!
//! Bauteile verweisen nur über Kennungen aufeinander, nie über Vec-Plätze.
//! Geometrie wird aus der Parametrik abgeleitet ([`Model::chain`]) und nie
//! gespeichert. Jede Änderung erhöht die Revision.

use crate::attr::{
    self, AttrRef, AttrUser, Attributes, Display, Fill, FillId, LineType, LineTypeId, Pen, PenId,
    Surface, SurfaceId,
};
use crate::element::{
    Building, BuildingId, Category, CladdingValue, Coupling, Element, ElementId, ElementKind,
    Floor, GroundSlab, LevelEdge, LevelKind, LevelRef, PropSet, PropValue, RunId, Soffit, Storey,
    StoreyId, StripFooting, Terrace, Wall, WallRun,
};
use crate::floor::{FloorError, FloorParams, FloorSlab, SoffitParams, StripParams, TerraceParams};
use crate::foundation::{FootingShape, Foundation, FoundationError, FoundationParams};
use crate::guid::{Guid, GuidGen};
use crate::id::Arena;
use crate::join::{self, Join, JoinEnd, JoinKind};
use crate::library::{
    material_key, Bearing, LayerFunction, LayerSet, LayerSetId, MatCategory, Material,
    MaterialDisplay, MaterialId, MaterialLayer, TypeCategory,
};
use crate::solid::material;
use crate::trade::{self, Trade, TradeId};
use crate::txn::{Change, Direction, Key, Open, Touched, Txn};
use crate::wall::{
    clean_points, cross2, segment_count, Attika, Drip, EndCut, FacingFoot, Layer, Overhang,
    RefSide, WallChain,
};
use sk_math::{vec3, Vec3};
use std::collections::BTreeMap;

/// Voreinstellungen für neue Bauteile.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Defaults {
    pub storey: StoreyId,
    pub exterior_wall: LayerSetId,
    pub interior_wall: LayerSetId,
}

/// Das Projekt (IFC: IfcProject) mit seinen Projektdaten (BIM §3.12a,
/// Regel 110); ein leeres Feld ist nicht gesetzt.
#[derive(Clone, Debug, PartialEq)]
pub struct Project {
    pub guid: Guid,
    pub name: String,
    /// Bauvorhaben (Bezeichnung), Bauherr, Planung bzw. Aufsteller.
    pub site: String,
    pub client: String,
    pub author: String,
    /// Projektart, Bauort, Projektnummer, Anschriften; mehrzeilige Felder
    /// mit `\n`.
    pub kind: String,
    pub place: String,
    pub number: String,
    pub client_addr: String,
    pub author_addr: String,
    /// Die Projektdaten stehen in `[projectinfo]`; sonst stehen `site`,
    /// `client` und `author` an `[project]` wie vor Regel 110.
    pub info: bool,
    /// `site`, `client`, `author` an `[project]` einer Datei, die auch
    /// `[projectinfo]` hat: gelten nicht und bleiben bytegleich, bis die
    /// erste Änderung sie entfernt (Regel 110).
    pub legacy: [String; 3],
    /// Bodenkennwerte für die Erdarbeiten; kein Projektdatum, ändert sich
    /// nur über [`Model::set_soil`].
    pub soil: crate::qto_earth::Boden,
    /// Die Bodenkennwerte stehen in `[soil]` (Regel 113); sonst noch an
    /// `[project]` wie davor.
    pub soil_info: bool,
    /// Bodenkennwerte an `[project]` (vor Regel 113), in der Reihenfolge von
    /// [`crate::qto_earth::Boden::FELDER`]: bleiben bytegleich stehen, bis
    /// die erste Änderung sie nach `[soil]` umzieht.
    pub soil_legacy: [Option<f64>; 5],
    /// Dicke und Abstand der Geländeplatte (Gelände G0); in der Datei nur,
    /// wo sie von der Vorgabe abweichen.
    pub terrain_plate: PlateSettings,
    /// Höhenaufmaß (Gelände G1); ändert sich nur über [`Model::set_terrain`].
    pub terrain: Option<std::sync::Arc<Terrain>>,
    /// `[terrainplate]`, deren Werte nicht galten: bleibt bytegleich, bis
    /// neue Werte sie ersetzen (Regel 72).
    pub terrain_plate_raw: Option<String>,
    /// Geländeflächen (Gelände G3b); ändern sich nur über
    /// [`Model::add_site_patch`] und die Geschwister.
    pub site_patches: Vec<TerrainPatch>,
    /// `[tpatch]`-Zeilen, die nicht galten: bleiben roh stehen (Regel 72),
    /// je mit der Zahl der gültigen Flächen davor. So bleibt die
    /// Reihenfolge der Datei und mit ihr die Zuordnung fremder Schlüssel.
    pub site_patches_raw: Vec<(usize, String)>,
    /// Andeckung und Eigenboden-Faktor (Gelände G3b); ändern sich nur über
    /// [`Model::set_site_soil`].
    pub site_soil: SiteSoil,
    /// `[sitesoil]`, deren Werte nicht galten: bleibt bytegleich, bis neue
    /// Werte sie ersetzen (Regel 72).
    pub site_soil_raw: Option<String>,
    /// Jedes weitere `[sitesoil]` nach dem ersten: gilt nicht, bleibt roh
    /// hinter ihm stehen (Regel 72, 116, wie `[terrain]` in #178).
    pub site_soil_doppelt: Vec<String>,
}

impl Project {
    /// Ein Projekt ohne Projektdaten.
    pub fn new(guid: Guid, name: &str) -> Project {
        Project {
            guid,
            name: name.to_string(),
            site: String::new(),
            client: String::new(),
            author: String::new(),
            kind: String::new(),
            place: String::new(),
            number: String::new(),
            client_addr: String::new(),
            author_addr: String::new(),
            info: false,
            legacy: Default::default(),
            soil: Default::default(),
            soil_info: false,
            soil_legacy: [None; 5],
            terrain_plate: PlateSettings::default(),
            terrain: None,
            terrain_plate_raw: None,
            site_patches: Vec::new(),
            site_patches_raw: Vec::new(),
            site_soil: SiteSoil::default(),
            site_soil_raw: None,
            site_soil_doppelt: Vec::new(),
        }
    }

    /// Die acht Projektdaten in der Reihenfolge von `[projectinfo]`:
    /// Schlüssel und Wert.
    pub fn fields(&self) -> [(&'static str, &str); 8] {
        [
            ("kind", &self.kind),
            ("projno", &self.number),
            ("site", &self.site),
            ("place", &self.place),
            ("client", &self.client),
            ("clientaddr", &self.client_addr),
            ("author", &self.author),
            ("authoraddr", &self.author_addr),
        ]
    }

    /// Sind alle Projektdaten leer?
    pub fn is_blank(&self) -> bool {
        self.fields().iter().all(|(_, v)| v.is_empty())
    }
}

/// Fehler beim Umbenennen eines Bauteils.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NumberError {
    Empty,
    Taken(ElementId),
    /// Die Nummer trägt eine `[opening]`-Zeile der Datei, die nicht zählt
    /// (Regel 72, Review #86 B3), oder eine solche `[space]`-Zeile.
    Reserved,
    NoElement,
}

/// Wo ein Baustoff steckt ([`Model::material_uses`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Use {
    /// Typ mit der Zahl seiner Bauteile.
    Type(LayerSetId, usize),
    /// Bauteil ohne Typ.
    Element(ElementId),
}

/// Grundlagen der Erdarbeiten einer Gründung ([`Model::ground_basis`]): alle
/// Höhen in mm relativ zu ±0,00, Flächen in mm², Längen in mm.
#[derive(Clone, Debug, PartialEq)]
pub struct GroundBasis {
    /// Außenwandzug, Sohlplatte, Frostschürze, Gebäude.
    pub run: RunId,
    pub slab: ElementId,
    pub footing: ElementId,
    pub building: Option<BuildingId>,
    /// OK Gelände; mit Höhenaufmaß das Flächenmittel unter der Platte (die
    /// Ebene der Erdmengen in G1).
    pub terrain_z: f64,
    /// Mit Höhenaufmaß die Höhe von ±0,00 in m ü. NHN, sonst `None`.
    pub terrain_nhn: Option<f64>,
    /// OK und UK Sohlplatte.
    pub slab_top_z: f64,
    pub slab_bottom_z: f64,
    /// Dicke der Perimeterdämmung (0 = keine) und ihre Unterkante (ohne
    /// Dämmung UK Platte). Die Dämmung liegt innerhalb der Schürze, OK
    /// Schürze ist UK Platte.
    pub insulation: f64,
    pub insulation_bottom_z: f64,
    /// UK Frostschürze = UK Gründung, Breite der Schürze.
    pub footing_bottom_z: f64,
    pub footing_width: f64,
    /// OK Gelände bis UK Gründung; mit Höhenaufmaß ab dem tiefsten
    /// Geländepunkt am Plattenrand.
    pub embedment: f64,
    /// Plattenumriss gegen den Uhrzeigersinn auf z = 0.
    pub outline: Vec<Vec3>,
    pub slab_area: f64,
    pub slab_perimeter: f64,
    /// Ring der Schürze im Grundriss und Länge ihrer Mittellinie.
    pub footing_area: f64,
    pub footing_axis_length: f64,
}

/// Lage und Blickrichtung eines Schnitts (A quer, B längs). Ansichtszustand:
/// kein Rückgängig-Schritt, ändert die Revision nicht, steht aber in der
/// Datei.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Cut {
    /// Lage der Schnittebene quer zur Linie (A: y, B: x, mm); `None`, solange
    /// der Schnitt nie gezeigt wurde.
    pub pos: Option<f64>,
    /// Blick gespiegelt (A: nach −y statt +y, B: nach −x statt +x).
    pub flip: bool,
}

/// Namen der Schnitte in der Reihenfolge ihrer Kennung.
pub const CUT_NAMES: [&str; 2] = ["A", "B"];

#[derive(Clone, Debug)]
pub struct Model {
    project: Project,
    /// Lage und Nordrichtung (`[location]`, Sonnenstand S1).
    location: Location,
    /// `[location]`-Zeile der Datei, die nicht als Lage zählt, roh (Befund A
    /// der Abnahme S1, Review 3br).
    location_raw: Option<String>,
    /// Fußpunkt des Nordpfeils (Sonnenstand S2).
    north_foot: Option<location::Foot>,
    /// Datum und Uhrzeit des Sonnenstands-Systems (S4); Ansichtszustand
    /// wie `cuts`.
    sun: Option<location::Sun>,
    /// `[sun]`-Zeile der Datei, die nicht zählt, roh.
    sun_raw: Option<String>,
    /// Schatten der vier Ansichten (S7), eigene Wahl des Projekts;
    /// Ansichtszustand wie `cuts`. Dazu unlesbare Zeilen roh.
    view_shade: [Option<location::ViewShade>; 4],
    view_shade_raw: Vec<String>,
    /// Ansichten: Teile unter dem Gelände gestrichelt statt ausgeblendet
    /// (S11), Ansichtszustand wie `view_shade`, dazu unlesbare Zeilen roh.
    view_below: [bool; 4],
    view_below_raw: Vec<String>,
    /// Schalter „Ausbau innen“ je Ansicht (Raumbuch RB4), Ansichtszustand wie
    /// `view_below`, dazu unlesbare Zeilen roh.
    view_finish: [(bool, bool); 3],
    view_finish_raw: Vec<String>,
    /// Zeilen `[spacefinish]`, die nicht zählen (RB6, roh).
    space_finish_raw: Vec<raum_ausbau::RohZeile>,
    /// Eigene Schnitthöhe des Grundrisses ([`crate::hoehen`]), dazu eine
    /// unlesbare Zeile roh.
    pub(crate) plan_cut: Option<f64>,
    pub(crate) plan_cut_raw: Option<String>,
    /// `[opening]`-Zeilen der Datei, die nicht zählen (Regel 72).
    pub(crate) opening_raw: Vec<oeffnung_model::RawOpening>,
    /// `[space]`-Zeilen der Datei, die nicht zählen (Regel 72).
    pub(crate) space_raw: Vec<raum::RawSpace>,
    /// Schnitte A und B.
    cuts: [Cut; 2],
    /// Zuletzt gezeigter Schnitt (Kennung, A = 0); Ansichtszustand wie `cuts`.
    active_cut: usize,
    /// Ausgeblendetes und Isoliertes (Paket 3); Ansichtszustand wie `cuts`.
    pub(crate) visibility: crate::view::Visibility,
    /// Schnelloption je Ansicht (Zahnrad, Jörn 12.10.); Ansichtszustand wie
    /// `view_below`.
    pub(crate) view_hide: crate::view::ViewHide,
    /// Reihenfolge, in der Bauteile gesperrt wurden (für die `[lock]`-Zeilen,
    /// damit ein Rundlauf die Reihenfolge der Datei behält).
    pub(crate) lock_order: Vec<Guid>,
    /// Stifte, Schraffuren, Oberflächen und Bauteildarstellung.
    attr: Attributes,
    materials: Arena<Material>,
    layer_sets: Arena<LayerSet>,
    /// Gewerke nach Reihe (Paket 1a): Startbestand, Firmenanpassungen aus
    /// der Datei.
    trades: Vec<Trade>,
    buildings: Arena<Building>,
    storeys: Erfasst<Storey>,
    elements: Erfasst<Element>,
    runs: Erfasst<WallRun>,
    /// Anschlüsse zwischen Wandzügen, aus den Punkten abgeleitet (B5a).
    joins: JoinListe,
    defaults: Defaults,
    /// Zuletzt vergebene laufende Nummer je Kategorie (wird nie zurückgesetzt).
    numbers: [u32; Category::ALL.len()],
    /// Zuletzt vergebene Gebäudenummer (GB-02 → 2); nie neu vergeben, nur
    /// ein verworfener Schritt nimmt sie zurück.
    building_number: u32,
    revision: u64,
    guids: GuidGen,
    /// Offener Schritt für Rückgängig ([`Model::begin`]).
    txn: Option<Open>,
    /// Jede Änderung muss in einem Schritt liegen (in der App; in Tests nicht).
    strict: bool,
    /// Was der letzte Abgleich der Streifen am Haus melden will
    /// ([`Model::house_strip_note`]); nicht in der Datei, nicht im Verlauf.
    strip_note: Option<StripNote>,
    /// Was eine neuere Fassung in die Datei geschrieben hat und dieser Leser
    /// nicht kennt (F-17, F-17b): bleibt beim Speichern bytegleich.
    pub(crate) foreign: crate::catalog::Foreign,
    /// Zeilen der Erweiterungsabschnitte (KA-0b), gedeutet von `sk-cost`.
    pub(crate) ext: crate::ext::ExtStore,
    /// Steigt bei jeder Änderung im Erweiterungsspeicher.
    ext_revision: u64,
    /// Stand der Räume und die zuletzt geänderten ([`Model::raum_revision`]).
    raum_log: raum::RaumLog,
    /// Netze nach Punktänderungen im Faden ([`Model::netz_im_faden`]). Nur
    /// die Szene der App schaltet es ein; sk-cost, CLI und Tests haben es
    /// immer aus. Es wird nicht gespeichert, zählt nicht zur Revision,
    /// Rückgängig stellt es nicht zurück, und ein Klon hat es aus.
    netz_faden: gelaende_aufmass::NetzImFaden,
    /// Gebäude der letzten Schritte ([`Model::gebaeude_seit`]).
    bau_log: bau_log::BauLog,
    /// Definitionen der Erweiterungsbauteile, nach `key` (E3).
    ext_defs: Vec<crate::erweiterung::ExtDef>,
    /// Zuletzt vergebene Nummer je Präfix einer Erweiterung.
    ext_numbers: BTreeMap<String, u32>,
    /// Unlesbare `[extdef]`- und `[extpart]`-Zeilen der Datei, roh.
    pub(crate) ext_raw: Vec<String>,
    /// Raumvorlagen des Raumkatalogs (Raumbuch RB2).
    space_types: Vec<raumvorlage::SpaceType>,
    /// Stil und Versätze der Maßketten ([`crate::bemassung`]).
    bemassung: crate::bemassung::Bemassung,
    /// Bemaßung je Ansicht am Zahnrad; Ansichtszustand.
    mass_sicht: crate::bemassung::MassSicht,
    /// Verzeichnis für einen lesenden Gang ([`Model::lesend`]).
    merker: verzeichnis::Merker,
    /// Das Verzeichnis zwischen den Gängen, nachgeführt beim nächsten.
    halter: verzeichnis::Halter,
    /// Was aus dem Höhenaufmaß folgt, je Modellstand gemerkt.
    lagemerker: gelaende_aufmass::Lagemerker,
    /// Was die Geländeflächen ergeben, zu den Werten, aus denen es folgt.
    flaechenmerker: crate::qto_earth::FlaechenMerker,
    /// Das Planungsgelände für die Darstellung, über Kopien geteilt.
    planmerker: gelaende_flaeche::PlanMerker,
}

/// Merkt den Stand eines Datensatzes vor seiner ersten Änderung im offenen
/// Schritt; `new`: eben angelegt, vorher gab es ihn nicht.
macro_rules! note {
    ($self:ident, $variant:ident, new $id:expr) => {
        note!(@push $self, $variant, $id, |_| None)
    };
    ($self:ident, $variant:ident, $arena:expr, $id:expr) => {
        note!(@push $self, $variant, $id, |id| $arena.get(id).cloned())
    };
    (@push $self:ident, $variant:ident, $id:expr, $old:expr) => {{
        let id = $id;
        match $self.txn.as_mut() {
            Some(t) => {
                if t.noted.insert(Key::$variant(id)) {
                    #[allow(clippy::redundant_closure_call)]
                    let old = ($old)(id);
                    t.changes.push(Change::$variant { id, old, new: None });
                }
            }
            None => debug_assert!(!$self.strict, "Änderung ohne Schritt"),
        }
    }};
}

#[path = "delete.rs"]
mod delete;
#[path = "model_bibliothek.rs"]
mod model_bibliothek;
#[path = "model_darstellung.rs"]
mod model_darstellung;
#[path = "model_gebaeude.rs"]
mod model_gebaeude;
#[path = "model_pruefung.rs"]
mod model_pruefung;
#[path = "verzeichnis.rs"]
mod verzeichnis;
pub use delete::{refusal_lines, refusal_text, Deleted, Refusal};
use verzeichnis::{Erfasst, JoinListe};

#[path = "lock.rs"]
mod lock;

#[path = "erweiterung_model.rs"]
mod erweiterung_model;
pub use erweiterung_model::{ExtError, EXT_SEQ};
pub use lock::{edit_blocked, Locked};

#[path = "location.rs"]
mod location;

#[path = "ansicht_ausbau.rs"]
mod ansicht_ausbau;
pub use ansicht_ausbau::FINISH_VIEWS;

#[path = "fallrohr_model.rs"]
mod fallrohr_model;
#[path = "flachdach.rs"]
mod flachdach;
pub use flachdach::{
    MIN_ROOF_EDGE, ROOF_INSULATION_GUID, ROOF_SEAL_GUID, ROOF_TYPE_GUID, ROOF_VAPOUR_GUID,
};
pub use location::{
    im_bereich, Foot, Location, ShadeLight, Sun, Utm, ViewShade, FOOT_MAX, NHN, SHADE_VIEWS, UTM_E,
    UTM_N, UTM_ZONEN,
};

#[path = "anschluesse.rs"]
mod anschluesse;

#[path = "dach.rs"]
mod dach;

#[cfg(test)]
#[path = "abgleich_tempo.rs"]
mod abgleich_tempo;

#[path = "decke.rs"]
mod decke;

#[path = "gelaende.rs"]
mod gelaende;
pub use gelaende::GelaendeZug;

#[path = "gelaende_platte.rs"]
mod gelaende_platte;
pub use gelaende_platte::{PlateInput, PlateSettings, TerrainPlate, PLATE_MARGIN, PLATE_THICK};

#[path = "gelaende_aufmass.rs"]
mod gelaende_aufmass;
#[cfg(test)]
#[path = "gelaende_aufmass_budget.rs"]
mod gelaende_aufmass_budget;
pub use gelaende_aufmass::{
    Crs, HeightRef, HeightSystem, LocalFrame, NetzAuftrag, NetzFertig, Show, SurveyFrame, Terrain,
    TerrainFolgen, TerrainLine, TerrainMesh, TerrainPoint, Trim,
};

#[path = "gelaende_eben.rs"]
mod gelaende_eben;

#[path = "gelaende_grenze.rs"]
mod gelaende_grenze;
pub use gelaende_grenze::{offset_ring_edge, parcel_ring_ok};

#[path = "gelaende_punkte.rs"]
mod gelaende_punkte;
pub use gelaende_punkte::TerrainSpot;

#[path = "gelaende_flaeche.rs"]
mod gelaende_flaeche;
pub use gelaende_flaeche::{
    PatchHeight, PlannedTerrain, SiteSoil, TerrainPatch, PATCH_FALL_MAX, PATCH_HATCH, PATCH_SLOPE,
    PATCH_SLOPE_RANGE, PATCH_XY_MAX, PATCH_Z_MAX,
};

#[path = "gelaende_haus.rs"]
mod gelaende_haus;
pub use gelaende_haus::{
    HouseStrip, StripNote, STRIP_FALL, STRIP_FALL_RANGE, STRIP_WIDTH, STRIP_WIDTH_RANGE, STRIP_Z,
};

#[path = "gelaende_lageplan.rs"]
mod gelaende_lageplan;
pub use gelaende_lageplan::{affine_apply, Affine, SitePlan, Underlay, UnderlayKind};

#[path = "geschossbaender.rs"]
mod geschossbaender;

#[path = "gruendung.rs"]
mod gruendung;

#[path = "bemassung_model.rs"]
mod bemassung_model;
pub use bemassung_model::FLOOR_COVER;

#[path = "oeffnung_model.rs"]
mod oeffnung_model;
pub use oeffnung_model::OpeningFault;
pub(crate) use oeffnung_model::{OpeningGrab, RawOpening};

#[path = "oeffnung_schloss.rs"]
mod oeffnung_schloss;

#[path = "stapel.rs"]
mod stapel;

#[path = "ausbau.rs"]
mod ausbau;
pub(crate) use ausbau::finish_pattern;
pub use ausbau::{
    FinishType, CEILING_TYPE_GUID, FINISH_TYPES, FLOOR_INSULATION_GUID, GYPSUM_PLASTER_GUID,
    GYPSUM_WALL_TYPE_GUID, IMPACT_INSULATION_GUID, LIME_PLASTER_GUID, LIME_WALL_TYPE_GUID,
    OAK_SKIRTING_TYPE_GUID, PAINT_GUID, PARQUET_GUID, PARQUET_TYPE_GUID, PORCELAIN_GUID,
    PORCELAIN_TYPE_GUID, SCREED_FLOOR_TYPE_GUID, SCREED_MAT_GUID, SCREED_SLAB_TYPE_GUID,
    SKIRTING_MAT_GUID, SLAB_SEAL_GUID, TILE_SEAL_GUID, TILE_SKIRTING_TYPE_GUID,
    WALL_TILES_TYPE_GUID, WALL_TILE_GUID, WET_FLOOR_TYPE_GUID, WOODCHIP_GUID,
    WOODCHIP_WALL_TYPE_GUID,
};
#[path = "raumvorlage.rs"]
mod raumvorlage;
pub use raumvorlage::{
    space_template_rank, SpaceGroup, SpaceTemplate, SpaceType, WallTiles, BATH_SPACE_GUID,
    BEDROOM_SPACE_GUID, CHILD_SPACE_GUID, DEFAULT_SPACE_GUID, DINING_SPACE_GUID, GUEST_SPACE_GUID,
    HALL_SPACE_GUID, KITCHEN_SPACE_GUID, LIVING_SPACE_GUID, SPACE_TEMPLATES, STORAGE_SPACE_GUID,
    STUDY_SPACE_GUID, TECHNICAL_SPACE_GUID, TILES_DEFAULT, TILES_HEIGHT, UTILITY_SPACE_GUID,
    WC_SPACE_GUID, WOFLV,
};
#[path = "teilen.rs"]
mod teilen;
pub use teilen::FugeError;

#[path = "fang.rs"]
mod fang;
pub use fang::{Fangziele, Wandende, Wandflaeche};

#[path = "ecke.rs"]
mod ecke;
pub use ecke::{EckeFehler, Vereint, ZugPunkte};
#[path = "bau_log.rs"]
mod bau_log;
#[path = "raum.rs"]
mod raum;
pub use bau_log::BauStand;
pub(crate) use raum::RawSpace;
pub use raum::{FreieRaumnamen, Space, SPACE_SEQ};
#[path = "raum_ausbau.rs"]
mod raum_ausbau;
pub(crate) use raum_ausbau::RohZeile;
pub use raum_ausbau::{ausbau_befund, AusbauAbweichung, AusbauStelle, AusbauWahl, WandSeite};

impl Default for Model {
    fn default() -> Model {
        Model::new()
    }
}

impl Model {
    /// Leeres Modell mit Startbibliothek und Erdgeschoss. Die Bibliothek
    /// (Attribute, Baustoffe, Bauteiltypen) hat in jedem neuen Projekt
    /// dieselben Guids, damit der Firmenkatalog sie wiedererkennt (K2);
    /// Projekt und Geschosse bekommen neue.
    pub fn new() -> Model {
        let mut m = Model::standard(GuidGen::with_seed(LIBRARY_SEED));
        let mut g = GuidGen::from_time();
        m.project.guid = g.next_guid();
        for id in m.storeys.ids().collect::<Vec<_>>() {
            if let Some(s) = m.storeys.get_mut(id) {
                s.guid = g.next_guid();
            }
        }
        m.guids = g;
        // Standard-Außenwand neuer Projekte: AW-36 (F5); Tests mit festem
        // Startwert behalten AW-31,5
        if let Some(id) = m.type_by_guid(ETICS_TYPE_GUID) {
            m.defaults.exterior_wall = id;
        }
        m
    }

    /// Wie [`Model::new`], aber mit festem Startwert für die Guids (Tests).
    pub fn with_seed(seed: u64) -> Model {
        Model::standard(GuidGen::with_seed(seed))
    }

    fn standard(mut guids: GuidGen) -> Model {
        let (mut attr, st) = attr::defaults(&mut guids);
        let mut materials = Arena::new();
        let mut mat = |name: &str, category, priority, density, cut_fill, color, cut_color| {
            // Werks-Oberflächen mit Werksmuster: feste Guid (Regel 60)
            let next = guids.next_guid();
            let guid = crate::proctex::factory_guid(name).unwrap_or(next);
            let surface = attr.add_surface(Surface {
                guid,
                name: name.into(),
                color,
                cut_color,
                pattern: crate::proctex::factory_for(guid),
            });
            let d = MaterialDisplay {
                cut_fill,
                cut_fg: st.hatch_pen,
                cut_bg: st.background,
                surface,
            };
            materials.insert(Material::new(
                guids.next_guid(),
                name,
                category,
                priority,
                density,
                d,
            ))
        };
        use MatCategory as C;
        let aerated = mat(
            "Porenbeton",
            C::Masonry,
            800,
            350.0,
            st.masonry,
            [238, 237, 232],
            [176, 177, 174],
        );
        let insulation = mat(
            "Dämmung (WDVS)",
            C::Insulation,
            300,
            20.0,
            st.insulation,
            [244, 239, 220],
            [232, 196, 92],
        );
        let concrete = mat(
            "Stahlbeton",
            C::Concrete,
            900,
            2500.0,
            st.masonry,
            // Jörns Referenz „Sichtbeton mittelgrau“ (Paket 6 §1.1)
            [142, 142, 141],
            [150, 150, 148],
        );
        let plaster = mat(
            "Putz",
            C::Plaster,
            100,
            1400.0,
            st.empty,
            // Jörns Referenz „Reibeputz weiß“ (Paket 6 §1.1)
            [236, 236, 237],
            [200, 198, 192],
        );
        let mut layer_sets = Arena::new();
        // Werkstypen mit festen Guids (K1); der Erzeuger läuft weiter, damit
        // die übrigen Guids gleich bleiben
        let _ = guids.next_guid();
        let exterior_wall = layer_sets.insert(LayerSet {
            guid: EXTERIOR_TYPE_GUID,
            name: "AW 31,5 Porenbeton + WDVS".into(),
            code: "AW-31,5".into(),
            category: TypeCategory::ExteriorWall,
            props: PropSet::new(),
            note: String::new(),
            changed: 1,
            bearing: Bearing::Core,
            layers: vec![
                MaterialLayer::new(insulation, 140.0, LayerFunction::Insulation),
                MaterialLayer::new(aerated, 175.0, LayerFunction::Structure).core(),
            ],
            height: None,
        });
        let mut storeys = Arena::new();
        let storey = storeys.insert(Storey {
            guid: guids.next_guid(),
            building: None,
            name: "Erdgeschoss".into(),
            short: "EG".into(),
            kind: LevelKind::Storey,
            elevation: 0.0,
            height: STOREY_HEIGHT,
            embed: None,
            lines: Default::default(),
        });
        let project = Project::new(guids.next_guid(), "Projekt");
        // Nach der Projekt-Guid angelegt, damit die älteren Guids gleich bleiben
        let _ = guids.next_guid();
        let interior_wall = layer_sets.insert(interior_set(INTERIOR_TYPE_GUID, aerated));
        // Stahlbeton: Diagonale, jede zweite Linie gestrichelt (E15)
        let cross = attr.add_fill(Fill {
            guid: guids.next_guid(),
            name: "Stahlbeton".into(),
            kind: attr::FillKind::Lines(attr::concrete_lines()),
            space: attr::FillSpace::Paper,
        });
        if let Some(m) = materials.get_mut(concrete) {
            m.cut_fill = cross;
        }
        // Gründung und Obergeschoss (B11), zuletzt angelegt, damit die
        // älteren Guids gleich bleiben
        storeys.insert(Storey {
            guid: guids.next_guid(),
            building: None,
            name: "Gründung".into(),
            short: "GR".into(),
            kind: LevelKind::Foundation,
            elevation: -FOUNDATION_DEPTH,
            height: FOUNDATION_DEPTH,
            embed: None,
            lines: Default::default(),
        });
        storeys.insert(Storey {
            guid: guids.next_guid(),
            building: None,
            name: "Obergeschoss".into(),
            short: "OG".into(),
            kind: LevelKind::Storey,
            elevation: STOREY_HEIGHT,
            height: UPPER_HEIGHT,
            embed: None,
            lines: Default::default(),
        });
        // Linientypen des Startsatzes (E4), zuletzt angelegt, damit die
        // älteren Guids gleich bleiben; die Schnittlinie A–A wird Strichpunkt
        for (name, pattern) in attr::standard_line_types().into_iter().skip(1) {
            let id = attr.add_line_type(LineType {
                guid: guids.next_guid(),
                name: name.into(),
                pattern,
            });
            if name == attr::SECTION_LINE_TYPE {
                let mut d = attr.display().clone();
                d.section_line.line_type = id;
                attr.set_display(d);
            }
        }
        // K4: λ-Vorbelegungen (BIM) an den älteren Baustoffen
        for (id, lambda) in [
            (aerated, 0.09),
            (insulation, 0.035),
            (concrete, 2.3),
            (plaster, 0.87),
        ] {
            if let Some(m) = materials.get_mut(id) {
                m.lambda = Some(lambda);
            }
        }
        // K4: Jörns Wandtypen mit ihren Baustoffen, zuletzt angelegt, damit
        // die älteren Guids gleich bleiben
        let mut mat =
            |name: &str, category, priority, density, lambda, cut_fill, color, cut_color| {
                let next = guids.next_guid();
                let guid = crate::proctex::factory_guid(name).unwrap_or(next);
                let surface = attr.add_surface(Surface {
                    guid,
                    name: name.into(),
                    color,
                    cut_color,
                    pattern: crate::proctex::factory_for(guid),
                });
                let d = MaterialDisplay {
                    cut_fill,
                    cut_fg: st.hatch_pen,
                    cut_bg: st.background,
                    surface,
                };
                materials.insert(
                    Material::new(guids.next_guid(), name, category, priority, density, d)
                        .lambda(lambda),
                )
            };
        let facing = mat(
            "Verblender (Vormauerziegel)",
            C::Masonry,
            700,
            1800.0,
            Some(0.68),
            st.masonry,
            // Paket 6 (Darstellung §3.5): Ansichtsfläche weiß (Fugen in der
            // Ansicht auf Papier), in 3D zeigen die Steine ihre Farben
            [255, 255, 255],
            [150, 80, 62],
        );
        let cavity = mat(
            "Kerndämmung (Mineralwolle)",
            C::Insulation,
            300,
            30.0,
            Some(0.035),
            st.insulation,
            [236, 226, 170],
            [222, 200, 110],
        );
        // Luft hat keinen Körper; Schraffur leer, die Oberfläche wird nie gezeigt
        let air = mat(
            "Luft",
            C::Air,
            0,
            1.2,
            None,
            st.empty,
            [255, 255, 255],
            [255, 255, 255],
        );
        let layer = |material, thickness, function, core| {
            let l = MaterialLayer::new(material, thickness, function);
            if core {
                l.core()
            } else {
                l
            }
        };
        use LayerFunction as F;
        let wall_type = |guid, name: &str, code: &str, category, layers| LayerSet {
            guid,
            name: name.into(),
            code: code.into(),
            category,
            props: PropSet::new(),
            note: String::new(),
            changed: 1,
            bearing: Bearing::Core,
            layers,
            height: None,
        };
        layer_sets.insert(wall_type(
            ETICS_TYPE_GUID,
            "AW mit WDVS 36",
            "AW-36",
            TypeCategory::ExteriorWall,
            vec![
                layer(insulation, 120.0, F::Insulation, false),
                layer(aerated, 240.0, F::Structure, true),
            ],
        ));
        layer_sets.insert(wall_type(
            CAVITY_TYPE_GUID,
            "AW mehrschalig 49",
            "AW-49",
            TypeCategory::ExteriorWall,
            vec![
                layer(facing, 115.0, F::Finish, false),
                layer(air, 60.0, F::AirGap, false),
                layer(cavity, 140.0, F::Insulation, false),
                layer(aerated, 175.0, F::Structure, true),
            ],
        ));
        for (guid, name, code, d) in [
            (
                INTERIOR_115_TYPE_GUID,
                "IW 11,5 Porenbeton",
                "IW-11,5",
                115.0,
            ),
            (INTERIOR_240_TYPE_GUID, "IW 24 Porenbeton", "IW-24", 240.0),
        ] {
            layer_sets.insert(wall_type(
                guid,
                name,
                code,
                TypeCategory::InteriorWall,
                vec![layer(aerated, d, F::Structure, true)],
            ));
        }
        // K5: monolithische Wand, Decke 24 cm aufgelegt, davor Randdämmstreifen
        let edge = mat(
            "Randdämmung",
            C::Insulation,
            300,
            30.0,
            Some(0.035),
            st.insulation,
            [236, 230, 196],
            [226, 204, 120],
        );
        let mut mono = wall_type(
            MONO_TYPE_GUID,
            "AW monolithisch 36,5",
            "AW-36,5",
            TypeCategory::ExteriorWall,
            vec![layer(aerated, 365.0, F::Structure, true)],
        );
        mono.bearing = Bearing::Depth {
            depth: 240.0,
            strip: edge,
        };
        layer_sets.insert(mono);
        // Paket 6: Stift „Ansichtsmuster“ für die Fugen in Ansichten, mit
        // fester Guid, damit die übrigen Guids gleich bleiben
        let pen = attr::pattern_pen(attr::PATTERN_PEN_GUID, |_| false);
        let pen = attr.add_pen(pen);
        let mut d = attr.display().clone();
        d.pattern.pen = pen;
        attr.set_display(d);
        // Einbauelement (Integration F20): Linientyp „Strichlinie fein“,
        // ebenfalls mit fester Guid; über der Schnittebene (G7b) zunächst
        // ebenso
        let lt = attr.add_line_type(attr::element_line_type());
        let mut d = attr.display().clone();
        d.element.line_type = lt;
        d.above.line_type = lt;
        attr.set_display(d);
        // Paket 1a: Gewerk je Startbaustoff
        let ids: Vec<MaterialId> = materials.ids().collect();
        for id in ids {
            if let Some(x) = materials.get_mut(id) {
                x.trade = trade::for_material(&x.name, x.category);
            }
        }
        Model {
            project,
            location: Location::default(),
            location_raw: None,
            north_foot: None,
            sun: None,
            sun_raw: None,
            view_shade: [None; 4],
            view_shade_raw: Vec::new(),
            view_below: [false; 4],
            view_below_raw: Vec::new(),
            view_hide: Default::default(),
            view_finish: [(false, false); 3],
            view_finish_raw: Vec::new(),
            space_finish_raw: Vec::new(),
            plan_cut: None,
            plan_cut_raw: None,
            opening_raw: Vec::new(),
            space_raw: Vec::new(),
            attr,
            materials,
            layer_sets,
            trades: trade::start_trades(),
            buildings: Arena::new(),
            storeys: Erfasst::new(storeys),
            elements: Default::default(),
            runs: Default::default(),
            joins: Default::default(),
            defaults: Defaults {
                storey,
                exterior_wall,
                interior_wall,
            },
            numbers: [0; Category::ALL.len()],
            building_number: 0,
            revision: 0,
            guids,
            txn: None,
            strict: false,
            strip_note: None,
            cuts: Default::default(),
            active_cut: 0,
            visibility: Default::default(),
            lock_order: Vec::new(),
            foreign: Default::default(),
            ext: Default::default(),
            ext_revision: 0,
            raum_log: Default::default(),
            netz_faden: Default::default(),
            bau_log: Default::default(),
            merker: Default::default(),
            halter: Default::default(),
            lagemerker: Default::default(),
            flaechenmerker: Default::default(),
            planmerker: Default::default(),
            ext_defs: Vec::new(),
            ext_numbers: BTreeMap::new(),
            ext_raw: Vec::new(),
            space_types: Vec::new(),
            bemassung: Default::default(),
            mass_sicht: Default::default(),
        }
    }

    /// Setzt ein Modell aus geladenen Tabellen zusammen ([`crate::szo`]).
    /// Nummernzähler je Kategorie stehen auf der höchsten vorhandenen Nummer.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn from_parts(
        project: Project,
        attr: Attributes,
        materials: Arena<Material>,
        layer_sets: Arena<LayerSet>,
        buildings: Arena<Building>,
        storeys: Arena<Storey>,
        elements: Arena<Element>,
        runs: Arena<WallRun>,
        defaults: Defaults,
        guids: GuidGen,
    ) -> Model {
        let mut numbers = [0; Category::ALL.len()];
        for (_, e) in elements.iter() {
            if e.category == Category::Extension {
                continue;
            }
            let n = e
                .number
                .strip_prefix(e.category.prefix())
                .and_then(|r| r.strip_prefix('-'))
                .and_then(|r| r.parse::<u32>().ok());
            if let Some(n) = n {
                let c = &mut numbers[e.category.index()];
                *c = (*c).max(n);
            }
        }
        let building_number = buildings
            .iter()
            .filter_map(|(_, b)| building_index(&b.number))
            .max()
            .unwrap_or(0);
        let mut m = Model {
            project,
            location: Location::default(),
            location_raw: None,
            north_foot: None,
            sun: None,
            sun_raw: None,
            view_shade: [None; 4],
            view_shade_raw: Vec::new(),
            view_below: [false; 4],
            view_below_raw: Vec::new(),
            view_hide: Default::default(),
            view_finish: [(false, false); 3],
            view_finish_raw: Vec::new(),
            space_finish_raw: Vec::new(),
            plan_cut: None,
            plan_cut_raw: None,
            opening_raw: Vec::new(),
            space_raw: Vec::new(),
            attr,
            materials,
            layer_sets,
            trades: trade::start_trades(),
            buildings,
            storeys: Erfasst::new(storeys),
            elements: Erfasst::new(elements),
            runs: Erfasst::new(runs),
            joins: Default::default(),
            defaults,
            numbers,
            building_number,
            revision: 0,
            guids,
            txn: None,
            strict: false,
            strip_note: None,
            cuts: Default::default(),
            active_cut: 0,
            visibility: Default::default(),
            lock_order: Vec::new(),
            foreign: Default::default(),
            ext: Default::default(),
            ext_revision: 0,
            raum_log: Default::default(),
            netz_faden: Default::default(),
            bau_log: Default::default(),
            merker: Default::default(),
            halter: Default::default(),
            lagemerker: Default::default(),
            flaechenmerker: Default::default(),
            planmerker: Default::default(),
            ext_defs: Vec::new(),
            ext_numbers: BTreeMap::new(),
            ext_raw: Vec::new(),
            space_types: Vec::new(),
            bemassung: Default::default(),
            mass_sicht: Default::default(),
        };
        m.joins.setzen(m.detect_all());
        m
    }

    pub fn project(&self) -> &Project {
        &self.project
    }

    /// Projektdaten ändern; Guid und Name bleiben. Die erste Änderung zieht
    /// die Daten nach `[projectinfo]` um und nimmt `site`, `client`,
    /// `author` von `[project]` (Regel 110), im selben Schritt. Gleiche
    /// Daten ändern nichts.
    pub fn set_project(&mut self, p: Project) -> bool {
        if p.fields() == self.project.fields() {
            return false;
        }
        let p = Project {
            guid: self.project.guid,
            name: self.project.name.clone(),
            info: true,
            legacy: Default::default(),
            soil: self.project.soil,
            soil_info: self.project.soil_info,
            soil_legacy: self.project.soil_legacy,
            terrain_plate: self.project.terrain_plate,
            terrain_plate_raw: self.project.terrain_plate_raw.clone(),
            site_patches: self.project.site_patches.clone(),
            site_patches_raw: self.project.site_patches_raw.clone(),
            site_soil: self.project.site_soil,
            site_soil_raw: self.project.site_soil_raw.clone(),
            site_soil_doppelt: self.project.site_soil_doppelt.clone(),
            ..p
        };
        match self.txn.as_mut() {
            Some(t) => {
                if t.noted.insert(Key::Project) {
                    t.changes.push(Change::Project {
                        old: Box::new(self.project.clone()),
                        new: Box::new(self.project.clone()),
                    });
                }
            }
            None => debug_assert!(!self.strict, "Änderung ohne Schritt"),
        }
        self.project = p;
        self.touch();
        true
    }

    /// Projektdaten eines neuen Projekts (Maske bei „Neu“): ohne
    /// Rückgängig-Schritt, es gibt davor nichts, wohin man zurück könnte.
    /// Nur außerhalb eines Schritts.
    pub fn init_project(&mut self, p: Project) {
        debug_assert!(self.txn.is_none(), "Anfangswerte im Schritt");
        // Bodenkennwerte gelten auch ohne Projektdaten
        if crate::qto_earth::Boden::aus_werten(p.soil.werte()).is_some()
            && p.soil != self.project.soil
        {
            self.project.soil = p.soil;
            self.project.soil_info = true;
            self.project.soil_legacy = [None; 5];
            self.touch();
        }
        if p.fields() == self.project.fields() {
            return;
        }
        self.project = Project {
            guid: self.project.guid,
            name: self.project.name.clone(),
            info: true,
            legacy: Default::default(),
            soil: self.project.soil,
            soil_info: self.project.soil_info,
            soil_legacy: self.project.soil_legacy,
            terrain_plate: self.project.terrain_plate,
            terrain_plate_raw: self.project.terrain_plate_raw.clone(),
            site_patches: self.project.site_patches.clone(),
            site_patches_raw: self.project.site_patches_raw.clone(),
            site_soil: self.project.site_soil,
            site_soil_raw: self.project.site_soil_raw.clone(),
            site_soil_doppelt: self.project.site_soil_doppelt.clone(),
            ..p
        };
        self.touch();
    }

    /// Zeilen eines Erweiterungsabschnitts (KA-0b) in Dateireihenfolge.
    pub fn ext<'a>(&'a self, section: &'a str) -> impl Iterator<Item = &'a crate::ExtRec> + 'a {
        self.ext.section(section)
    }

    /// Der ganze Erweiterungsspeicher (zum Lesen und als Arbeitskopie).
    pub fn ext_store(&self) -> &crate::ExtStore {
        &self.ext
    }

    /// Ungültige `svc=` an Schichten, die beim Speichern noch roh
    /// zurückgeschrieben werden (A311, Regel 72): Typ, Schicht (ab 0) und
    /// Wert. Ändert man die Schicht, fällt der Wert weg (F-17b); Rückgängig
    /// bringt ihn wieder. Für Befund 99 in `sk-cost`.
    pub fn raw_svc(&self) -> Vec<crate::RawSvc> {
        crate::szo::raw_svc(self)
    }

    /// Steigt bei jeder Änderung im Erweiterungsspeicher (auch Rückgängig).
    pub fn ext_revision(&self) -> u64 {
        self.ext_revision
    }

    /// Reihenfolge der Erweiterungsabschnitte beim Schreiben; fehlende kommen
    /// dazu, vorhandene bleiben.
    #[doc(hidden)]
    pub fn ext_declare(&mut self, sections: &[&str]) {
        self.ext.declare(sections);
    }

    /// Ersetzt die erste Zeile mit Kennung `id` an Ort und Stelle; gibt es
    /// keine, kommt die neue vor die erste Zeile mit Kennung `before`, sonst
    /// ans Ende des Abschnitts. Nur `sk-cost` ruft das auf (Bausteingrenze
    /// §4.1), im offenen Schritt.
    #[doc(hidden)]
    pub fn ext_put(&mut self, section: &str, id: &str, line: String, before: Option<&str>) {
        let new = Some(line.clone());
        let (at, old) = self.ext.put(section, id, line, before);
        self.note_ext(section, id, at, old, new);
    }

    /// Hängt `line` ans Ende des Abschnitts, ohne eine Zeile mit derselben
    /// Kennung zu ersetzen (doppelte Kennung bleibt, Regel 74). Nur
    /// `sk-cost`, im offenen Schritt.
    #[doc(hidden)]
    pub fn ext_append(&mut self, section: &str, line: String) {
        let id = crate::ext::rec_id(&line).unwrap_or_default();
        let new = Some(line.clone());
        let at = self.ext.append(section, line);
        self.note_ext(section, &id, at, None, new);
    }

    /// Entfernt die erste Zeile mit Kennung `id` (nur `sk-cost`).
    #[doc(hidden)]
    pub fn ext_remove(&mut self, section: &str, id: &str) {
        if let Some((at, old)) = self.ext.remove(section, id) {
            self.note_ext(section, id, at, Some(old), None);
        }
    }

    fn note_ext(
        &mut self,
        section: &str,
        id: &str,
        at: usize,
        old: Option<String>,
        new: Option<String>,
    ) {
        if old == new {
            return;
        }
        match self.txn.as_mut() {
            Some(t) => t.changes.push(Change::Ext {
                section: section.to_string(),
                id: id.to_string(),
                at,
                old,
                new,
            }),
            None => debug_assert!(!self.strict, "Änderung ohne Schritt"),
        }
        self.ext_revision += 1;
        self.touch();
    }

    /// Schnitte A und B (Ansichtszustand, siehe [`Cut`]).
    pub fn cuts(&self) -> &[Cut; 2] {
        &self.cuts
    }

    /// Lage oder Blickrichtung eines Schnitts ändern: ohne Schritt und ohne
    /// neue Revision, wie Kamera und Zoom.
    pub fn set_cut(&mut self, i: usize, cut: Cut) {
        if let Some(c) = self.cuts.get_mut(i) {
            *c = cut;
        }
    }

    /// Zuletzt gezeigter Schnitt (A = 0).
    pub fn active_cut(&self) -> usize {
        self.active_cut
    }

    /// Gezeigten Schnitt merken (Ansichtszustand wie [`Model::set_cut`]).
    pub fn set_active_cut(&mut self, i: usize) {
        if i < self.cuts.len() {
            self.active_cut = i;
        }
    }

    /// Der zuletzt gerechnete Stand der Geländeflächen (Review #173 B1).
    pub(crate) fn flaechenmerker(&self) -> &crate::qto_earth::FlaechenMerker {
        &self.flaechenmerker
    }

    pub(crate) fn planmerker(&self) -> &gelaende_flaeche::PlanMerker {
        &self.planmerker
    }

    /// Steigt bei jeder Änderung.
    pub fn revision(&self) -> u64 {
        self.revision
    }

    fn touch(&mut self) {
        self.revision += 1;
        if self.txn.is_none() {
            self.raum_log.unbekannt();
            self.bau_log.unbekannt();
        }
        self.lagemerker.weiter();
    }

    /// Wie `touch` für Änderungen, von denen die Lage im Aufmaß nicht
    /// abhängt: Geschosshöhen, Höhe und Einbindung einer Gründung, Versatz
    /// und feste Höhe von ±0,00 (Review #123 W45 B1 und Nachprüfung). So
    /// bleibt sie beim Ziehen im Geschossmanager gemerkt.
    fn touch_height(&mut self) {
        self.revision += 1;
        if self.txn.is_none() {
            self.raum_log.unbekannt();
            self.bau_log.unbekannt();
        }
    }

    /// Setzt die Revision nach einem verworfenen Schritt auf den Stand davor
    /// zurück (Einstellungsfenster: „Abbrechen“ lässt das Projekt
    /// ungeändert). Nur für Schritte, die allein Attribute ändern.
    pub fn restore_revision(&mut self, rev: u64) {
        debug_assert!(self.txn.is_none());
        self.revision = rev;
        self.lagemerker.vergessen();
        self.bau_log.unbekannt();
    }

    /// Neue, noch nie vergebene Guid.
    pub fn new_guid(&mut self) -> Guid {
        self.guids.next_guid()
    }

    /// Eigene Guid-Folge für eine Arbeitskopie (K3: Bauteilkatalog): was
    /// sie vergibt und ins Modell zurückkommt, vergibt das Modell nie
    /// noch einmal.
    pub fn fork_guids(&mut self) {
        self.guids = GuidGen::from_time();
    }

    pub fn defaults(&self) -> &Defaults {
        &self.defaults
    }

    // --- Wandzüge ---------------------------------------------------------

    pub fn runs(&self) -> &Arena<WallRun> {
        &self.runs
    }

    pub fn run(&self, id: RunId) -> Option<&WallRun> {
        self.runs.get(id)
    }

    /// Legt einen Wandzug im Geschoss `storey` an, mit einem Wand-Bauteil je
    /// Segment; die Wände reichen von UK bis OK des Geschosses. Ein
    /// geschlossener Außenwandzug im EG erzeugt das ganze Gebäude (B12): in
    /// jedem Geschoss darüber einen gekoppelten Zug mit Decke. Gehört das
    /// Geschoss noch zu keinem Gebäude, entsteht das erste. `None`, wenn die
    /// Punkte kein Segment ergeben.
    pub fn add_wall_run(
        &mut self,
        points: &[Vec3],
        closed: bool,
        ref_side: RefSide,
        storey: StoreyId,
        layer_set: LayerSetId,
        category: Category,
    ) -> Option<RunId> {
        let flat: Vec<Vec3> = points.iter().map(|p| vec3(p.x, p.y, 0.0)).collect();
        let pts = clean_points(&flat, closed);
        let closed = closed && pts.len() >= 3;
        let count = segment_count(pts.len(), closed);
        let fits = self
            .layer_set(layer_set)
            .is_some_and(|t| TypeCategory::of(category) == Some(t.category));
        // Auf der Ebene Flachdach stehen nur die Aufkantungen (abgeleitet)
        let roof = self
            .storey(storey)
            .is_none_or(|s| s.kind == LevelKind::Roof);
        if count == 0 || !fits || roof {
            return None;
        }
        self.ensure_building(storey);
        let guid = self.new_guid();
        let run = self.runs.insert(WallRun {
            guid,
            points: pts,
            closed,
            ref_side,
            base: LevelRef::bottom(storey),
            top: LevelRef::top(storey),
            storey,
            segments: Vec::new(),
        });
        note!(self, Run, new run);
        let template = Element {
            guid: Guid(0),
            number: String::new(),
            category,
            storey,
            layer_set: Some(layer_set),
            seq: self.storey_seq(storey).0,
            kind: ElementKind::Wall(Wall {
                run,
                seg: 0,
                coupling: None,
            }),
            props: PropSet::new(),
            locked: false,
        };
        let segments = (0..count)
            .map(|k| self.new_wall(run, k, &template))
            .collect();
        self.runs.get_mut(run)?.segments = segments;
        self.sync_parts(run);
        self.update_joins(&[run]);
        if self.needs_foundation(run) {
            let (mut below, mut at) = (run, storey);
            while let Some(up) = self.level_above(at) {
                let Some(r) = self.stack_run(below, up) else {
                    break;
                };
                (below, at) = (r, up);
            }
        } else if self.needs_floor(run) {
            // Im obersten Geschoss gezeichnet: Aufkantung darüber
            if let Some(fd) = self
                .level_above(storey)
                .filter(|l| self.storey(*l).is_some_and(|s| s.kind == LevelKind::Roof))
            {
                self.stack_run(run, fd);
            }
        }
        self.touch();
        Some(run)
    }

    /// Gebäude aus einem Polygon (B12): geschlossener Außenwandzug im EG des
    /// Gebäudes `b` mit dem voreingestellten Aufbau, darüber alle Geschosse.
    pub fn build_from_polygon(&mut self, b: BuildingId, points: &[Vec3]) -> Option<RunId> {
        let eg = self.ground_of(Some(b))?;
        let set = self.defaults.exterior_wall;
        self.add_wall_run(points, true, RefSide::Left, eg, set, Category::ExteriorWall)
    }

    /// Zug im Geschoss `storey` über dem Zug `below`: gleiche Punkte (Versatz
    /// 0), gleiche Bezugsseite, je Segment eine Wand gleicher Art, an die
    /// Wand darunter gekoppelt; mit Decke an OK des Geschosses.
    fn stack_run(&mut self, below: RunId, storey: StoreyId) -> Option<RunId> {
        let r = self.run(below)?.clone();
        let guid = self.new_guid();
        let run = self.runs.insert(WallRun {
            guid,
            points: r.points.clone(),
            closed: r.closed,
            ref_side: r.ref_side,
            base: LevelRef::bottom(storey),
            top: LevelRef::top(storey),
            storey,
            segments: Vec::new(),
        });
        note!(self, Run, new run);
        let seq = self.storey_seq(storey).0;
        // Auf der Ebene Flachdach wird die Außenwand zur Aufkantung
        let roof = self
            .storey(storey)
            .is_some_and(|s| s.kind == LevelKind::Roof);
        let mut segments = Vec::with_capacity(r.segments.len());
        for (k, &w) in r.segments.iter().enumerate() {
            let Some(t) = self.element(w).cloned() else {
                continue;
            };
            let category = if roof { Category::Parapet } else { t.category };
            let id = self.new_wall(
                run,
                k,
                &Element {
                    storey,
                    seq,
                    category,
                    props: PropSet::new(),
                    ..t
                },
            );
            if let Some(ElementKind::Wall(x)) = self.elements.get_mut(id).map(|e| &mut e.kind) {
                x.coupling = Some(Coupling {
                    below: w,
                    offset: 0.0,
                    linked: true,
                });
            }
            segments.push(id);
        }
        self.runs.get_mut(run)?.segments = segments;
        self.sync_parts(run);
        self.update_joins(&[run]);
        Some(run)
    }

    /// Züge, deren Wände an Wände des Zuges `below` gekoppelt sind.
    pub fn runs_above(&self, below: RunId) -> Vec<RunId> {
        let Some(segs) = self.run(below).map(|r| &r.segments) else {
            return Vec::new();
        };
        if let Some(v) = self.verz() {
            return v.darueber(segs);
        }
        let mut out = Vec::new();
        for (_, e) in self.elements.iter() {
            if let ElementKind::Wall(Wall {
                run,
                coupling: Some(c),
                ..
            }) = e.kind
            {
                if segs.contains(&c.below) && !out.contains(&run) {
                    out.push(run);
                }
            }
        }
        out
    }

    /// Alle Züge, die über `run` gestapelt sind (Geschoss für Geschoss).
    pub fn stack_above(&self, run: RunId) -> Vec<RunId> {
        let mut out: Vec<RunId> = Vec::new();
        let mut k = 0;
        let mut at = vec![run];
        while k < at.len() {
            for up in self.runs_above(at[k]) {
                if !at.contains(&up) {
                    at.push(up);
                    out.push(up);
                }
            }
            k += 1;
        }
        out
    }

    /// Ist der Zug an einen Zug darunter gekoppelt (gestapelte Außenwand)?
    pub fn is_coupled(&self, run: RunId) -> bool {
        self.run(run).is_some_and(|r| {
            r.segments.iter().any(|e| {
                matches!(
                    self.element(*e).map(|x| &x.kind),
                    Some(ElementKind::Wall(Wall {
                        coupling: Some(_),
                        ..
                    }))
                )
            })
        })
    }

    /// Zug darunter, an den `run` gekoppelt ist.
    pub fn run_below(&self, run: RunId) -> Option<RunId> {
        self.run(run)?
            .segments
            .iter()
            .find_map(|e| match self.element(*e)?.kind {
                ElementKind::Wall(Wall {
                    coupling: Some(c), ..
                }) => self.segment_of(c.below).map(|s| s.0),
                _ => None,
            })
    }

    /// Setzt neue Eckpunkte eines Wandzugs. Bleibt die Segmentzahl gleich, behält
    /// jede Wand ihren Platz (Verschieben). Ändert sie sich, behalten die Wände
    /// ihre Kennung nach Lage (Regel 11): ein geteiltes Segment gibt sie an seinen
    /// längeren Teil weiter, ein weggefallenes Segment nimmt genau seine mit.
    ///
    /// Angeschlossene Züge werden im selben Schritt mitgeführt (B5a). Liefert
    /// alle Züge, deren Körper sich dadurch ändern kann, `None` bei ungültigen
    /// Punkten.
    pub fn set_run_points(&mut self, id: RunId, points: &[Vec3]) -> Option<Vec<RunId>> {
        self.set_run_points_with(id, points, |_| {})
    }

    /// Wie [`Model::set_run_points`]; `fix` läuft gleich nach dem Setzen der
    /// Punkte (Kopplungen eines gestapelten Zuges mit Stufen, W-12).
    pub(super) fn set_run_points_with(
        &mut self,
        id: RunId,
        points: &[Vec3],
        fix: impl FnOnce(&mut Model),
    ) -> Option<Vec<RunId>> {
        let flat: Vec<Vec3> = points.iter().map(|p| vec3(p.x, p.y, 0.0)).collect();
        if !self.stack_fits(id, &flat) {
            return None;
        }
        // Punkte der Züge vorher: Deckenumriss alt gegen neu (U3). Nur die
        // geänderten Züge werden kopiert, nicht alle (L40)
        self.runs.aufzeichnen();
        let mut grab = self.opening_grab(id);
        if !self.set_points(id, &flat) {
            self.runs.vorher_nehmen();
            self.keep_opening_grab(grab);
            return None;
        }
        fix(self);
        self.remeasure(id);
        let mut moved = Vec::new();
        let mut roots = vec![id];
        roots.extend(self.carry_stack(id));
        for r in roots {
            for x in self.follow(r) {
                if !moved.contains(&x) {
                    moved.push(x);
                }
            }
        }
        for r in &moved {
            self.sync_parts(*r);
        }
        let mut out = moved.clone();
        let partners = |m: &Model, out: &mut Vec<RunId>| {
            for r in &moved {
                for p in m.joined_runs(*r) {
                    if !out.contains(&p) {
                        out.push(p);
                    }
                }
            }
        };
        partners(self, &mut out);
        self.update_joins(&moved);
        partners(self, &mut out);
        // Öffnungen wandern mit ihrer Wand (Paket O5)
        self.carry_openings(&mut grab);
        self.keep_opening_grab(grab);
        // Der Versatz zwischen den Geschossen formt Decke und Außenschichten
        // darunter (G7 K4) und den Fuß darüber (K3)
        for r in &moved {
            for p in self.run_below(*r).into_iter().chain(self.runs_above(*r)) {
                if !out.contains(&p) {
                    out.push(p);
                }
            }
        }
        // Innenwände, die unter eine mitbewegte Decke kommen oder sie verlassen
        // (Deckenband). Ein Zug ohne Stand davor blieb, wie er war
        let before = self.runs.vorher_nehmen();
        for r in &moved {
            let old = match before.iter().find(|(id, _)| id == r) {
                Some((_, x)) => x.as_ref().filter(|x| x.closed).map(|x| &x.points[..]),
                None => self.run(*r).filter(|x| x.closed).map(|x| &x.points[..]),
            };
            for i in self.runs_under_moved_floor(*r, old) {
                if !out.contains(&i) {
                    out.push(i);
                }
            }
        }
        Some(out)
    }

    /// [`Model::set_run_points`] ohne Mitführen und ohne Anschlüsse.
    fn set_points(&mut self, id: RunId, points: &[Vec3]) -> bool {
        let Some(run) = self.runs.get(id) else {
            return false;
        };
        let pts = clean_points(points, run.closed);
        let closed = run.closed && pts.len() >= 3;
        let count = segment_count(pts.len(), closed);
        if count == 0 {
            return false;
        }
        let old = run.segments.clone();
        note!(self, Run, self.runs, id);
        let (lines_old, lines_new) = (
            segment_lines(&run.points, run.closed),
            segment_lines(&pts, closed),
        );
        let matched = teilen::assign_walls(&lines_old, &lines_new, closed);
        let mut kept = vec![false; old.len()];
        let mut segments = Vec::with_capacity(count);
        let mut geteilt = Vec::new();
        for (k, &m) in matched.iter().enumerate() {
            // Neues Stück: Vorlage ist die Wand, aus der es geteilt wurde,
            // sonst der Nachbar (Stufe), sonst die erste
            let template = teilen::template_of(&lines_old, &lines_new, &matched, k)
                .or(Some(0))
                .and_then(|o| old.get(o))
                .and_then(|e| self.elements.get(*e))
                .cloned();
            match (m, &template) {
                (Some(o), _) => {
                    kept[o] = true;
                    let e = old[o];
                    if self.segment_of(e).map(|s| s.1) != Some(k) {
                        note!(self, Element, self.elements, e);
                    }
                    if let Some(ElementKind::Wall(w)) =
                        self.elements.get_mut(e).map(|el| &mut el.kind)
                    {
                        w.seg = k as u32;
                    }
                    segments.push(e);
                }
                (None, Some(t)) => {
                    let neu = self.new_wall(id, k, t);
                    // Nur ein Teilstück übernimmt die Wandseiten, nicht der
                    // Rückfall auf Nachbar oder erste Wand (Review #190 H-190c)
                    let teil = teilen::split_of(&lines_old, &lines_new, k).map(|o| old[o]);
                    if let Some(g) = self.elements.get(neu).map(|e| e.guid) {
                        if teil.and_then(|e| self.elements.get(e)).map(|e| e.guid) == Some(t.guid) {
                            geteilt.push((t.guid, g));
                        }
                    }
                    segments.push(neu);
                }
                (None, None) => return false,
            }
        }
        self.ausbau_wand_geteilt(&geteilt);
        let mut weg = Vec::new();
        for (e, keep) in old.into_iter().zip(kept) {
            if !keep {
                note!(self, Element, self.elements, e);
                weg.extend(self.elements.remove(e).map(|x| x.guid));
            }
        }
        self.ausbau_wand_weg(&weg);
        let Some(run) = self.runs.get_mut(id) else {
            return false;
        };
        run.points = pts;
        run.closed = closed;
        run.segments = segments;
        self.touch();
        true
    }

    /// Entfernt einen Wandzug mit allen seinen Wänden und den Zügen, die
    /// darüber an ihn gekoppelt sind; die Raumbuch-Abweichungen an ihren
    /// Wandseiten gehen mit (BIM Regel 24).
    pub fn remove_run(&mut self, id: RunId) -> bool {
        let Some(weg) = self.remove_run_roh(id) else {
            return false;
        };
        self.ausbau_wand_weg(&weg);
        true
    }

    /// Wie [`Model::remove_run`], die Raumbuch-Abweichungen an den Wänden
    /// bleiben: für Schritte, die sie selbst übertragen (Wände vereinen).
    /// Liefert die Kennungen der gelöschten Wände, auch der Züge darüber.
    pub(super) fn remove_run_roh(&mut self, id: RunId) -> Option<Vec<Guid>> {
        if !self.runs.contains(id) {
            return None;
        }
        let mut weg = Vec::new();
        for up in self.runs_above(id) {
            weg.extend(self.remove_run_roh(up).unwrap_or_default());
        }
        note!(self, Run, self.runs, id);
        let run = self.runs.remove(id)?;
        for e in run.segments {
            note!(self, Element, self.elements, e);
            weg.extend(self.elements.remove(e).map(|x| x.guid));
        }
        self.sync_parts(id);
        self.update_joins(&[id]);
        self.touch();
        Some(weg)
    }

    /// Wandzug und Segment einer Wand.
    pub fn segment_of(&self, wall: ElementId) -> Option<(RunId, usize)> {
        match self.element(wall)?.kind {
            ElementKind::Wall(w) => Some((w.run, w.seg as usize)),
            _ => None,
        }
    }

    /// Wand zu Segment `seg` eines Wandzugs.
    pub fn wall_at(&self, run: RunId, seg: usize) -> Option<ElementId> {
        self.run(run)?.segments.get(seg).copied()
    }

    /// Geometrie eines Wandzugs mit den Schichten seines Aufbaus und seinen
    /// Anschlüssen an andere Züge. Haben die Wände eines Zuges verschiedene
    /// Aufbauten, gilt der des ersten Segments.
    pub fn chain(&self, id: RunId) -> Option<WallChain> {
        self.chain_and_floor(id).map(|(c, _)| c)
    }

    /// [`Model::chain`] und die Decke über dem Zug ([`Model::floor`]) in
    /// einem Gang: der Deckenumriss entsteht nur einmal.
    #[allow(clippy::type_complexity)]
    pub fn chain_and_floor(
        &self,
        id: RunId,
    ) -> Option<(WallChain, Option<Result<FloorSlab, FloorError>>)> {
        let mut c = self.base_chain(id)?;
        // Die Löcher hängen nur am Grundkörper, nicht an den Anschlüssen:
        // einmal suchen, für Decke und Zug (Review W48 B3)
        let holes = self.holes_of(id, &c);
        let floor = self.floor_mit_loechern(id, &c, Some(&holes));
        let prio = |k: u16| self.material_by_key(k).map_or(0, |m| m.priority);
        // Im lesenden Gang nur die Anschlüsse dieses Zuges
        let v = self.verz();
        let anschluesse: Box<dyn Iterator<Item = &Join>> = match &v {
            Some(v) => Box::new(
                v.anschluesse(id, &self.joins)
                    .into_iter()
                    .map(|k| &self.joins[k]),
            ),
            None => Box::new(self.joins.iter()),
        };
        for j in anschluesse {
            if j.a_run == id {
                let cut = match (j.kind, j.b_end) {
                    (JoinKind::L, Some(eb)) => self
                        .base_chain(j.b_run)
                        .and_then(|b| join::l_miter(&c, j.a_end, &b, eb))
                        .map(EndCut::Miter),
                    _ => self
                        .segment_of(j.b)
                        .zip(self.base_chain(j.b_run))
                        .and_then(|((_, s), h)| join::t_cut(&c, j.a_end, &h, s, &prio))
                        .map(|x| EndCut::Layers(x.0)),
                };
                if let Some(cut) = cut {
                    c.joints.ends[j.a_end.index()] = cut;
                }
            }
            if j.b_run == id && j.kind == JoinKind::T {
                let gaps = self
                    .segment_of(j.b)
                    .zip(self.base_chain(j.a_run))
                    .and_then(|((_, s), a)| join::t_cut(&a, j.a_end, &c, s, &prio));
                if let Some((_, g)) = gaps {
                    c.joints.gaps.extend(g);
                }
            }
        }
        c.joints.seamless = matches!(&floor, Some(Ok(f)) if f.strip.is_some());
        c.joints.strip_below = self
            .run_below(id)
            .is_some_and(|b| self.strip_params(b).is_some());
        if c.joints.strip_below {
            c.joints.strip_open = self.open_segments(id);
        }
        if let Some(Ok(f)) = &floor {
            c.joints.overhang = self.overhang_offsets(id).map(|offsets| {
                let (b, t) = f.band();
                // Außenschichten bis unter die Bekleidung (W3)
                let drip = self
                    .drip_layer(id)
                    .zip(f.drip_bottom())
                    .map(|(layer, from)| {
                        let mut segs = vec![false; c.segment_count()];
                        for (k, _) in &f.claddings {
                            if let Some(x) = segs.get_mut(*k) {
                                *x = true;
                            }
                        }
                        Drip { layer, from, segs }
                    });
                Overhang {
                    offsets,
                    from: f.soffit_band().map_or(b, |s| s.0),
                    to: t,
                    drip,
                }
            });
        }
        if let Some(Ok(f)) = &floor {
            c.joints.attika = f
                .attika_band()
                .filter(|b| b.1 > b.0 + 1e-6)
                .map(|band| Attika {
                    band,
                    pieces: f.terraces.attika.clone(),
                });
        }
        c.joints.facing_foot = self.facing_foot(id, &c);
        c.joints.holes = holes;
        c.joints.slab_band = match &floor {
            Some(Ok(f)) => Some(f.band()),
            _ if self.category_of(id) == Some(Category::InteriorWall) => self
                .run(id)
                .and_then(|r| self.floor_over(r.storey, &c))
                .map(|f| f.band()),
            _ => None,
        };
        Some((c, floor))
    }

    /// Fußpunkt der Verblendschale des Zuges auf der Dachterrasse darunter
    /// (W2); `None` ohne Verblender, ohne Terrasse oder solange es den
    /// Baustoff Schaumglas noch nicht gibt ([`Model::sync_terraces`] legt
    /// ihn an).
    fn facing_foot(&self, run: RunId, c: &WallChain) -> Option<FacingFoot> {
        let mat = self.foamglass_material()?;
        let (band, segs, layer) = self.foot_plan(run, c)?;
        Some(FacingFoot {
            band,
            segs,
            layer,
            mat: material_key(mat),
        })
    }

    /// Wo der Zug `run` (Körper `c`) einen Fußpunkt aus Schaumglas braucht
    /// (W2, Tagesplan P1/P2): Steht seine äußere Verblendschale
    /// ([`crate::qto::is_facing`]) auf der Dachterrasse der Decke darunter,
    /// dann auf deren Segmenten vom Wandfuß bis OK Terrassenaufbau.
    /// Monolithisch und mit WDVS nie (kein Verblender).
    fn foot_plan(&self, run: RunId, c: &WallChain) -> Option<((f64, f64), Vec<bool>, usize)> {
        if self.category_of(run) != Some(Category::ExteriorWall) {
            return None;
        }
        let layer = self.facing_layer(run)?;
        let below = self.run_below(run)?;
        if self.runs_above(below).first() != Some(&run) {
            return None;
        }
        let Some(Ok(f)) = self.floor(below) else {
            return None;
        };
        let (_, top) = f.terrace_band()?;
        let top = top.min(c.top());
        if top <= c.base + 1e-6 {
            return None;
        }
        // Segmente darunter mit Terrasse; oben das Teilstück darüber und die
        // Stufen daneben (W-12)
        let on: Vec<usize> = f
            .terraces
            .outlines
            .iter()
            .flat_map(|t| t.segments.iter().copied())
            .collect();
        let map = self.stack_map(run);
        let m = map.len();
        let segs: Vec<bool> = (0..c.segment_count())
            .map(|j| match map.get(j).copied().flatten() {
                Some(k) => on.contains(&k),
                None if m > 0 => [map[(j + m - 1) % m], map[(j + 1) % m]]
                    .iter()
                    .flatten()
                    .any(|k| on.contains(k)),
                None => false,
            })
            .collect();
        segs.iter()
            .any(|x| *x)
            .then_some(((c.base, top), segs, layer))
    }

    /// Äußere Verblendschale im Typ des Zuges: die erste Schicht vor dem
    /// Kern, die [`crate::qto::is_facing`] erfüllt.
    fn facing_layer(&self, run: RunId) -> Option<usize> {
        let t = self
            .run(run)?
            .segments
            .first()
            .and_then(|w| self.element(*w))
            .and_then(|e| e.layer_set)
            .and_then(|t| self.layer_set(t))?;
        let core = t.layers.iter().position(|l| l.core)?;
        t.layers[..core]
            .iter()
            .position(|l| crate::qto::is_facing(self, l))
    }

    /// Innerste der Außenschichten, die unter die Bekleidung der Untersicht
    /// laufen (W3, Jörn 14:03): bei zweischaligen Wänden der Verblender,
    /// sonst die äußere Dämmung (WDVS) samt Putz davor, ohne Dämmung die
    /// Schichten vor dem Kern (Außenputz). Die Bekleidung stößt stumpf von
    /// innen dagegen. `None` ohne Schicht vor dem Kern.
    fn drip_layer(&self, run: RunId) -> Option<usize> {
        if let Some(i) = self.facing_layer(run) {
            return Some(i);
        }
        let t = self
            .run(run)?
            .segments
            .first()
            .and_then(|w| self.element(*w))
            .and_then(|e| e.layer_set)
            .and_then(|t| self.layer_set(t))?;
        let core = t.layers.iter().position(|l| l.core)?;
        let outer = &t.layers[..core];
        outer
            .iter()
            .position(|l| l.function == LayerFunction::Insulation)
            .or_else(|| core.checked_sub(1))
    }

    /// Braucht irgendein Zug in `scope` (`None`: alle) einen Fußpunkt aus
    /// Schaumglas (W2)? Ein Zug außerhalb des Bereichs blieb im Schritt
    /// gleich; brauchte er einen, legte ihn schon der Abgleich an, als er
    /// zuletzt im Bereich lag (L40: sonst geht jedes Schließen über alle
    /// Züge).
    fn foot_wanted(&self, scope: Option<&[RunId]>) -> bool {
        let alle: Vec<RunId>;
        let zuege = match scope {
            Some(s) => s,
            None => {
                alle = self.runs.ids().collect();
                &alle
            }
        };
        zuege.iter().any(|&r| {
            self.facing_layer(r).is_some()
                && self
                    .base_chain(r)
                    .is_some_and(|c| self.foot_plan(r, &c).is_some())
        })
    }

    /// Kategorie der Wände eines Zuges (die des ersten Segments).
    fn category_of(&self, run: RunId) -> Option<Category> {
        let first = *self.run(run)?.segments.first()?;
        Some(self.element(first)?.category)
    }

    /// Decke im Geschoss `storey`, unter der der Zug `c` steht: eine
    /// Segmentmitte liegt in ihrem Umriss.
    fn floor_over(&self, storey: StoreyId, c: &WallChain) -> Option<FloorSlab> {
        let mids = mids(&c.points, c.closed);
        let (lo, hi) = bounds2(&mids);
        for (id, r) in self.runs_near(storey, lo, hi) {
            // Vortest: geschlossen und Rechteck der Zugpunkte überdeckt die
            // Segmentmitten, bevor der Umriss entsteht
            if !r.closed || r.storey != storey || !overlap(bounds2(&r.points), (lo, hi)) {
                continue;
            }
            let (rlo, rhi) = bounds2(&r.points);
            if !mids.iter().any(|m| inside(*m, rlo, rhi)) {
                continue;
            }
            if let Some(Ok(slab)) = self.floor_ohne_kerben(id) {
                if mids.iter().any(|m| contains(&slab.outline, *m)) {
                    return Some(slab);
                }
            }
        }
        None
    }

    /// Züge, unter denen die im Geschoss `storey` mit Rechteck `lo`–`hi`
    /// sind: im lesenden Gang nur die aus dem Raster, sonst alle.
    fn runs_near(
        &self,
        storey: StoreyId,
        lo: Vec3,
        hi: Vec3,
    ) -> Box<dyn Iterator<Item = (RunId, &WallRun)> + '_> {
        match self.verz() {
            Some(v) => Box::new(
                v.zuege_bei(storey, lo, hi)
                    .into_iter()
                    .filter_map(|id| Some((id, self.runs.get(id)?))),
            ),
            None => Box::new(self.runs.iter()),
        }
    }

    /// Innenwandzüge, die die Decke über dem Zug `run` unterbricht.
    pub fn runs_under_floor(&self, run: RunId) -> Vec<RunId> {
        self.runs_under_outline(run, None)
    }

    /// [`Model::runs_under_floor`] zum schon berechneten Deckenumriss.
    pub(crate) fn runs_under_outline(&self, run: RunId, slab: Option<&[Vec3]>) -> Vec<RunId> {
        let Some(r) = self.run(run).filter(|r| r.closed) else {
            return Vec::new();
        };
        let (lo, hi) = bounds2(&r.points);
        // Vortest am Rechteck, bevor der Umriss entsteht
        let near: Vec<(RunId, Vec<Vec3>)> = self
            .runs_near(r.storey, lo, hi)
            .filter(|(id, x)| {
                *id != run && x.storey == r.storey && overlap(bounds2(&x.points), (lo, hi))
            })
            .filter(|(id, _)| self.category_of(*id) == Some(Category::InteriorWall))
            .map(|(id, x)| (id, mids(&x.points, x.closed)))
            .filter(|(_, m)| m.iter().any(|p| inside(*p, lo, hi)))
            .collect();
        if near.is_empty() {
            return Vec::new();
        }
        let outline = match slab {
            Some(o) => o.to_vec(),
            None => match self.floor_ohne_kerben(run) {
                Some(Ok(f)) => f.outline,
                _ => return Vec::new(),
            },
        };
        near.into_iter()
            .filter(|(_, m)| m.iter().any(|p| contains(&outline, *p)))
            .map(|(r, _)| r)
            .collect()
    }

    /// Innenwandzüge, deren Deckenband sich ändert, weil die Decke über `run`
    /// von den Punkten `old` auf die jetzigen gewandert ist: Eine Segmentmitte
    /// liegt jetzt im Umriss und vorher in keinem oder umgekehrt. Das Band
    /// selbst (UK, OK) hängt nicht vom Grundriss ab; Innenwände, die drunter
    /// bleiben, ändern sich nicht (U3). Ohne alten Stand oder ohne gültige
    /// Decke vorher und nachher wie [`Model::runs_under_floor`].
    fn runs_under_moved_floor(&self, run: RunId, old: Option<&[Vec3]>) -> Vec<RunId> {
        let Some(r) = self.run(run).filter(|r| r.closed) else {
            return Vec::new();
        };
        let Some(old) = old else {
            return self.runs_under_floor(run);
        };
        if old == &r.points[..] {
            return Vec::new();
        }
        let before = self.base_chain(run).and_then(|mut c| {
            c.points = old.to_vec();
            self.floor_mit_loechern(run, &c, Some(&[]))
        });
        let (Some(Ok(before)), Some(Ok(after))) = (before, self.floor_ohne_kerben(run)) else {
            return self.runs_under_floor(run);
        };
        let (alo, ahi) = bounds2(&before.outline);
        let (blo, bhi) = bounds2(&after.outline);
        let (lo, hi) = (
            vec3(alo.x.min(blo.x), alo.y.min(blo.y), 0.0),
            vec3(ahi.x.max(bhi.x), ahi.y.max(bhi.y), 0.0),
        );
        self.runs_near(r.storey, lo, hi)
            .filter(|(id, x)| {
                *id != run && x.storey == r.storey && overlap(bounds2(&x.points), (lo, hi))
            })
            .filter(|(id, _)| self.category_of(*id) == Some(Category::InteriorWall))
            .filter(|(_, x)| {
                let m = mids(&x.points, x.closed);
                let under = |o: &[Vec3]| m.iter().any(|p| contains(o, *p));
                under(&before.outline) != under(&after.outline)
            })
            .map(|(id, _)| id)
            .collect()
    }

    /// Fläche des Kernumrisses eines geschlossenen Wandzugs (mm²): Umriss an
    /// der Außenseite seiner tragenden Schicht, ohne Außendämmung, mit
    /// derselben Eckberechnung wie die Decke, aber nur aus der eigenen Kette
    /// (kein Versatz des Geschosses darüber). Rechnet keine Körper.
    /// Grundfläche, stammdaten/verwaltung.md §9 (Entscheid 17:20).
    pub fn core_area(&self, run: RunId) -> Option<f64> {
        let chain = self.base_chain(run)?;
        if !chain.closed || chain.clean_points().len() < 3 {
            return None;
        }
        let core = chain.layers.iter().position(|l| l.core)?;
        let depth: f64 = chain.layers[..core].iter().map(|l| l.thickness).sum();
        let face = chain.face_corners(chain.outer_offset());
        let outline = if depth > 0.0 {
            sk_math::polygon::inset(&face, depth).ok()?.pts
        } else {
            face
        };
        Some(sk_math::polygon::area(&outline))
    }

    /// Geometrie eines Wandzugs ohne Anschlüsse.
    fn base_chain(&self, id: RunId) -> Option<WallChain> {
        let run = self.run(id)?;
        let set = run
            .segments
            .first()
            .and_then(|e| self.element(*e))
            .and_then(|e| e.layer_set)?;
        let base = self.level_z(run.base).unwrap_or(0.0);
        Some(WallChain {
            points: run.points.clone(),
            closed: run.closed,
            ref_side: run.ref_side,
            layers: self.run_layers(id, set),
            base,
            height: self.level_z(run.top).unwrap_or(base) - base,
            joints: Default::default(),
        })
    }

    /// Alle Wandzüge als Geometrie.
    pub fn chains(&self) -> impl Iterator<Item = (RunId, WallChain)> + '_ {
        self.runs
            .ids()
            .filter_map(|id| self.chain(id).map(|c| (id, c)))
    }

    // --- Rückgängig -------------------------------------------------------

    /// Verlangt ab jetzt für jede Änderung einen offenen Schritt (App).
    pub fn require_steps(&mut self) {
        self.strict = true;
    }

    /// Gegenstück zu [`Model::require_steps`]: Änderungen auch ohne Schritt,
    /// z. B. an einer Kopie des App-Modells zum Ausprobieren (Tests).
    pub fn allow_unstepped(&mut self) {
        self.strict = false;
    }

    /// Öffnet einen Schritt. Es darf keiner offen sein ([`Model::commit`] vorher).
    pub fn begin(&mut self, label: &'static str) {
        debug_assert!(self.txn.is_none(), "Schritt schon offen");
        self.txn = Some(Open {
            building_number: self.building_number,
            numbers: self.numbers.to_vec(),
            label,
            changes: Vec::new(),
            noted: Default::default(),
            grab: Default::default(),
            revision: self.revision,
        });
    }

    pub fn in_step(&self) -> bool {
        self.txn.is_some()
    }

    /// Modellstand beim Öffnen des offenen Schritts; `None` ohne Schritt.
    /// Was nur nach einem abgeschlossenen Schritt neu entsteht, gilt im
    /// Schritt zu diesem Stand weiter (Review #220 B1).
    pub fn step_revision(&self) -> Option<u64> {
        self.txn.as_ref().map(|t| t.revision)
    }

    /// Schließt den offenen Schritt. `None`, wenn er nichts geändert hat
    /// oder ein gesperrtes Bauteil geändert hätte ([`Model::try_commit`]).
    pub fn commit(&mut self) -> Option<Txn> {
        self.try_commit().ok().flatten()
    }

    /// Schließt den offenen Schritt. Randdämmstreifen folgen Wänden, Decken
    /// und Typen im selben Schritt. Ändert er ein gesperrtes Bauteil, wird
    /// er ganz zurückgerollt (Sicherheitsnetz, Paket 4 §2.2).
    pub fn try_commit(&mut self) -> Result<Option<Txn>, Locked> {
        if self.txn.is_some() && !self.only_swings() {
            let scope = self.sync_scope();
            let scope = scope.as_deref();
            // Was die vier Abgleiche wollen, in einem lesenden Gang: Keiner
            // hängt an den Bauteilen, die ein anderer anlegt (Review zu #148)
            let w = self.lesend(|| self.abgleich_wunsch(scope));
            self.sync_edge_strips_mit(scope, Some(w.streifen));
            self.sync_soffits_mit(scope, Some(w.untersicht));
            self.sync_terraces_mit(scope, Some(w.terrasse));
            self.sync_flat_roofs_mit(scope, Some(w.flachdach));
            // „Gelände ans Haus“ folgt dem Außenumriss
            self.sync_house_strips(scope);
            self.terrain_nhn_merken();
        }
        if let Some(id) = self.locked_change() {
            self.rollback();
            return Err(Locked(id));
        }
        Ok(self.close())
    }

    fn close(&mut self) -> Option<Txn> {
        let open = self.txn.take()?;
        let mut changes = open.changes;
        for c in &mut changes {
            self.fill_new(c);
        }
        changes.retain(|c| !c.is_noop());
        self.raum_log.schritt(&changes);
        self.bau_log
            .schritt(bau_log::gebaeude(self, &changes), self.gelaendestand());
        (!changes.is_empty()).then_some(Txn {
            label: open.label,
            changes,
        })
    }

    /// Verwirft den offenen Schritt und stellt den Stand bei [`Model::begin`]
    /// wieder her (Esc beim Ziehen).
    pub fn rollback(&mut self) -> Touched {
        if let Some(open) = &self.txn {
            self.building_number = open.building_number;
            for (n, o) in self.numbers.iter_mut().zip(&open.numbers) {
                *n = *o;
            }
        }
        match self.close() {
            Some(t) => self.apply(&t, Direction::Undo),
            None => Touched::default(),
        }
    }

    /// Wandzüge, die der offene Schritt bisher berührt, samt Schürzen-,
    /// Decken- und Anschlusspartnern wie bei [`Model::apply`]: für eine
    /// Live-Vorschau, die nur diese Züge neu rechnet.
    pub fn step_touched(&self) -> Vec<RunId> {
        let mut t = Touched::default();
        let Some(open) = &self.txn else {
            return Vec::new();
        };
        let (mut footings, mut floors, mut strips) = (Vec::new(), Vec::new(), Vec::new());
        let mut openings = Vec::new();
        for c in &open.changes {
            match c {
                Change::Run { id, .. } => t.run(*id),
                Change::Element { id, old, .. } => {
                    for e in [old.as_ref(), self.elements.get(*id)].into_iter().flatten() {
                        match e.kind {
                            ElementKind::Wall(w) => t.run(w.run),
                            ElementKind::GroundSlab(s) => t.run(s.run),
                            ElementKind::StripFooting(f) => footings.push(f.slab),
                            ElementKind::PerimeterInsulation { slab } => footings.push(slab),
                            ElementKind::Floor(f) => {
                                t.run(f.run);
                                floors.push(f.run);
                            }
                            ElementKind::EdgeStrip { wall, .. } => strips.push(wall),
                            ElementKind::SoffitInsulation { floor }
                            | ElementKind::RoofTerrace { floor }
                            | ElementKind::Coping { floor }
                            | ElementKind::Roof { floor, .. } => strips.push(floor),
                            ElementKind::Downpipe { roof, .. } => {
                                strips.extend(self.floor_of_roof(roof))
                            }
                            ElementKind::Ext(_) | ElementKind::Space(_) => {}
                            ElementKind::Opening(ref o) => openings.extend(self.opening_runs(o)),
                        }
                    }
                }
                _ => {}
            }
        }
        for slab in footings.into_iter().chain(strips) {
            if let Some(r) = self.run_of(slab) {
                t.run(r);
            }
        }
        for r in floors {
            self.runs_under_floor(r).into_iter().for_each(|i| t.run(i));
        }
        self.stack_partners(&mut t);
        let runs = t.runs.clone();
        for r in runs {
            self.joined_runs(r).into_iter().for_each(|p| t.run(p));
        }
        // Öffnungen treffen nur die Wände an ihrem Pfad: Ein Loch ändert
        // weder die angeschlossenen Züge noch die darüber (Review #109 B4)
        for r in openings {
            t.run(r);
        }
        t.runs
    }

    /// Ergänzt die Züge darunter und darüber: Der Versatz zwischen ihnen
    /// formt Decke und Außenschichten unten (G7 K4) und den Fuß oben (K3).
    fn stack_partners(&self, t: &mut Touched) {
        let runs = t.runs.clone();
        for r in runs {
            if self.run(r).is_none() {
                continue;
            }
            self.run_below(r).into_iter().for_each(|b| t.run(b));
            self.runs_above(r).into_iter().for_each(|u| t.run(u));
        }
    }

    /// Trägt den heutigen Stand als „nachher“ ein.
    fn fill_new(&self, c: &mut Change) {
        match c {
            Change::Run { id, new, .. } => *new = self.runs.get(*id).cloned(),
            Change::Element { id, new, .. } => *new = self.elements.get(*id).cloned(),
            Change::LayerSet { id, new, .. } => *new = self.layer_sets.get(*id).cloned(),
            Change::Material { id, new, .. } => *new = self.materials.get(*id).cloned(),
            Change::Storey { id, new, .. } => *new = self.storeys.get(*id).cloned(),
            Change::Building { id, new, .. } => *new = self.buildings.get(*id).cloned(),
            Change::Pen { id, new, .. } => *new = self.attr.pen(*id).cloned(),
            Change::LineType { id, new, .. } => *new = self.attr.line_type(*id).cloned(),
            Change::Fill { id, new, .. } => *new = self.attr.fill(*id).cloned(),
            Change::Surface { id, new, .. } => *new = self.attr.surface(*id).cloned(),
            Change::Display { new, .. } => *new = self.attr.display().clone(),
            Change::Defaults { new, .. } => *new = self.defaults,
            Change::Trades { new, .. } => *new = self.trades.clone(),
            Change::SpaceTypes { new, .. } => *new = self.space_types.clone(),
            Change::ForeignRecords { new, .. } => *new = self.foreign.records.clone(),
            Change::ExtDefs { new, .. } => *new = self.ext_defs.clone(),
            // schon beim Ändern eingetragen
            Change::Ext { .. } => {}
            Change::Project { new, .. } => **new = self.project.clone(),
            Change::Location { new, foot, .. } => {
                *new = self.location;
                foot[1] = self.north_foot;
            }
            Change::Bemassung { new, .. } => **new = self.bemassung.clone(),
        }
    }

    /// Macht einen Schritt rückgängig oder wiederholt ihn. Kennungen bleiben
    /// erhalten; Guid-Erzeuger und Nummernzähler laufen weiter, die Revision steigt.
    pub fn apply(&mut self, t: &Txn, dir: Direction) -> Touched {
        debug_assert!(self.txn.is_none(), "Rückgängig in einem offenen Schritt");
        // Was hier ohne Schritt geändert wird, ist `t`
        let raeume = std::mem::take(&mut self.raum_log);
        let bau = std::mem::replace(&mut self.bau_log, bau_log::BauLog::platzhalter());
        let mut touched = Touched::default();
        let mut footings = Vec::new();
        let mut floors = Vec::new();
        let mut apply_one = |m: &mut Model, c: &Change| match c {
            Change::Run { id, old, new } => {
                m.runs.set(*id, pick(dir, old, new));
                touched.run(*id);
            }
            Change::Element { id, old, new } => {
                for e in [old, new].into_iter().flatten() {
                    match e.kind {
                        ElementKind::Wall(w) => touched.run(w.run),
                        ElementKind::GroundSlab(s) => touched.run(s.run),
                        // Wand oder Platte stehen ggf. selbst im Schritt
                        ElementKind::StripFooting(f) => footings.push(f.slab),
                        ElementKind::PerimeterInsulation { slab } => footings.push(slab),
                        ElementKind::EdgeStrip { wall, .. } => footings.push(wall),
                        ElementKind::SoffitInsulation { floor }
                        | ElementKind::RoofTerrace { floor }
                        | ElementKind::Coping { floor }
                        | ElementKind::Roof { floor, .. } => footings.push(floor),
                        ElementKind::Downpipe { roof, .. } => {
                            footings.extend(m.floor_of_roof(roof))
                        }
                        ElementKind::Floor(f) => {
                            touched.run(f.run);
                            floors.push(f.run);
                        }
                        ElementKind::Ext(_) | ElementKind::Space(_) => {}
                        ElementKind::Opening(ref o) => {
                            m.opening_runs(o).into_iter().for_each(|r| touched.run(r))
                        }
                    }
                }
                m.elements.set(*id, pick(dir, old, new));
            }
            Change::LayerSet { id, old, new } => {
                m.layer_sets.set(*id, pick(dir, old, new));
                // Ein neuer oder entfernter Aufbau trägt keine Wand, die nicht
                // selbst im Schritt steht (Ausbautypen mit dem Raum, RB3):
                // Rückgängig rechnet dann nur die Züge des Schritts
                touched.library |= old.is_some() && new.is_some();
            }
            Change::Material { id, old, new } => {
                // Nur Darstellungsverweise geändert: allein die Zeichentabelle;
                // ebenso ein neuer oder entfernter Baustoff (wie beim Aufbau)
                if old.is_none() || new.is_none() || bim_equal(old.as_ref(), new.as_ref()) {
                    touched.attr = true;
                } else {
                    touched.library = true;
                }
                m.materials.set(*id, pick(dir, old, new));
            }
            Change::Storey { id, old, new } => {
                m.storeys.set(*id, pick(dir, old, new));
                touched.library = true;
            }
            Change::Building { id, old, new } => {
                m.buildings.set(*id, pick(dir, old, new));
            }
            Change::Pen { id, old, new } => {
                m.attr.put_pen(*id, pick(dir, old, new));
                touched.attr = true;
            }
            Change::LineType { id, old, new } => {
                m.attr.put_line_type(*id, pick(dir, old, new));
                touched.attr = true;
            }
            Change::Fill { id, old, new } => {
                m.attr.put_fill(*id, pick(dir, old, new));
                touched.attr = true;
            }
            Change::Surface { id, old, new } => {
                m.attr.put_surface(*id, pick(dir, old, new));
                touched.attr = true;
            }
            Change::Display { old, new } => {
                m.attr.put_display(pick(dir, old, new));
                touched.attr = true;
            }
            Change::Defaults { old, new } => m.defaults = pick(dir, old, new),
            Change::Trades { old, new } => m.trades = pick(dir, old, new),
            // Raumvorlagen tragen keine Wand
            Change::SpaceTypes { old, new } => m.space_types = pick(dir, old, new),
            Change::ForeignRecords { old, new } => m.foreign.records = pick(dir, old, new),
            Change::ExtDefs { old, new } => m.ext_defs = pick(dir, old, new),
            Change::Ext {
                section,
                at,
                old,
                new,
                ..
            } => {
                let (to, from) = match dir {
                    Direction::Undo => (old, new),
                    Direction::Redo => (new, old),
                };
                m.ext.set(section, *at, to.clone(), from.clone());
                m.ext_revision += 1;
            }
            Change::Project { old, new } => m.project = *pick(dir, old, new),
            Change::Location {
                old,
                new,
                raw,
                foot,
            } => {
                m.location = pick(dir, old, new);
                m.north_foot = pick(dir, &foot[0], &foot[1]);
                m.location_raw = match dir {
                    Direction::Undo => raw.clone(),
                    Direction::Redo => None,
                };
            }
            Change::Bemassung { old, new } => m.bemassung = *pick(dir, old, new),
        };
        // Rückwärts in umgekehrter Reihenfolge: ein Platz wird erst frei, dann neu belegt
        match dir {
            Direction::Undo => t.changes.iter().rev().for_each(|c| apply_one(self, c)),
            Direction::Redo => t.changes.iter().for_each(|c| apply_one(self, c)),
        }
        // Schürze geändert: ihr Zug ist der ihrer Platte (eine gelöschte Platte
        // steht selbst im Schritt)
        for slab in footings {
            if let Some(r) = self.run_of(slab) {
                touched.run(r);
            }
        }
        // Decke geändert: auch die Innenwände darunter (Deckenband)
        for r in floors {
            self.runs_under_floor(r)
                .into_iter()
                .for_each(|i| touched.run(i));
        }
        self.stack_partners(&mut touched);
        if touched.attr {
            self.attr.bump();
        }
        if touched.library {
            self.joins.setzen(self.detect_all());
        } else {
            let runs = touched.runs.clone();
            for r in &runs {
                self.joined_runs(*r)
                    .into_iter()
                    .for_each(|p| touched.run(p));
            }
            self.update_joins(&runs);
            for r in &runs {
                self.joined_runs(*r)
                    .into_iter()
                    .for_each(|p| touched.run(p));
            }
        }
        self.touch();
        self.raum_log = raeume;
        self.raum_log.schritt(&t.changes);
        self.bau_log = bau;
        self.bau_log
            .schritt(bau_log::gebaeude(self, &t.changes), self.gelaendestand());
        touched
    }
}

/// Guids der Werkstypen „AW 31,5 Porenbeton + WDVS“ und „IW 17,5 Porenbeton“:
/// in jedem Projekt und Firmenkatalog derselbe Typ (K1, Regel 19).
pub const EXTERIOR_TYPE_GUID: Guid = Guid(0xbf19d5c9cf9241c9b5a191d5bdac9382);
pub const INTERIOR_TYPE_GUID: Guid = Guid(0xe3753d4ddf2d435299910b99a65cfba2);
/// Werkstypen aus K4 (Jörn 06.10. 16:38), in jedem Projekt gleich.
pub const ETICS_TYPE_GUID: Guid = Guid(0x72147f93f58782c40c7064abc28678f7);
pub const CAVITY_TYPE_GUID: Guid = Guid(0xab62c0bdc94bb6740b98633edb84415c);
pub const INTERIOR_115_TYPE_GUID: Guid = Guid(0x2f8dff421c184680660acedac925c20e);
pub const INTERIOR_240_TYPE_GUID: Guid = Guid(0x399dbdcd165ab68f996acfde24d981df);
/// Werkstyp aus K5: AW monolithisch 36,5 mit Randdämmstreifen.
pub const MONO_TYPE_GUID: Guid = Guid(0x9c03de8332f6e5a1d90474f347dbbf7e);
/// Laufende Zahl einer Gebäudenummer: „GB-02“ → 2.
pub fn building_index(number: &str) -> Option<u32> {
    number.strip_prefix("GB-")?.parse().ok()
}

/// Art eines Werkstyps nach seiner festen Guid.
pub(crate) fn werk_category(g: Guid) -> Option<TypeCategory> {
    match g {
        EXTERIOR_TYPE_GUID | ETICS_TYPE_GUID | CAVITY_TYPE_GUID | MONO_TYPE_GUID => {
            Some(TypeCategory::ExteriorWall)
        }
        INTERIOR_TYPE_GUID | INTERIOR_115_TYPE_GUID | INTERIOR_240_TYPE_GUID => {
            Some(TypeCategory::InteriorWall)
        }
        _ => None,
    }
}

/// Startwert der Guids der Startbibliothek in [`Model::new`]: Stifte,
/// Schraffuren, Oberflächen und Baustoffe sind in jedem neuen Projekt gleich.
const LIBRARY_SEED: u64 = 0x534b_4b41_5441_4c47;

/// Bauabschnitt der Wände (nach Frostschürze 1 und Sohlplatte 2).
pub const WALL_SEQ: u16 = 3;
const SLAB_SEQ: u16 = 2;
const FOOTING_SEQ: u16 = 1;
/// Die Perimeterdämmung liegt vor der Platte (nach der Schürze).
const PERIMETER_SEQ: u16 = 2;
/// Bauabschnitt der Erdgeschossdecke (nach den Wänden).
const FLOOR_SEQ: u16 = 4;
/// Standardwerte der Geschossbänder (B11, mm): Geschosshöhe EG und OG,
/// Gründungstiefe, Deckendicke.
pub(crate) const STOREY_HEIGHT: f64 = 2855.0;
/// Geschosshöhe OG (Jörn 10:13): lichte Höhe 2,635 + Decke 0,22.
pub(crate) const UPPER_HEIGHT: f64 = 2855.0;
pub(crate) const FOUNDATION_DEPTH: f64 = 800.0;
/// Ebene Flachdach (Jörn 10.10.): ihre Höhe ist die Aufkantung über OK
/// Rohdecke des obersten Geschosses, 50 cm, einstellbar von 15 cm bis
/// 1,50 m (OK Mauerwerk, ohne Attikablech).
pub const ROOF_UPSTAND: f64 = 500.0;
pub const MIN_ROOF_UPSTAND: f64 = 150.0;
pub const MAX_ROOF_UPSTAND: f64 = 1500.0;
pub const FLOOR_THICKNESS: f64 = 220.0;
/// Untersichtdämmung unter einem Vorsprung (Jörn 04:03, G7 K4, BIM Regel
/// 35): 12 cm, einstellbar von 4 bis 30 cm, nie 0.
pub const SOFFIT_THICKNESS: f64 = 120.0;
pub const MIN_SOFFIT: f64 = 40.0;
pub const MAX_SOFFIT: f64 = 300.0;
/// Bekleidung unter der Untersichtdämmung (Jörn 05:55–05:58, Tagesplan
/// P5–P7): 40 mm (Platte 8 und Traglattung 30 in einer Schicht), 10 … 80,
/// 0 ohne; der Verblender steht 30 mm (20 … 40) unter UK Bekleidung über.
/// Lattung nur als Menge: Grundlattung 800 mm, Traglattung quer 400 mm.
pub const SOFFIT_CLADDING: f64 = 40.0;
pub const MIN_CLADDING: f64 = 10.0;
pub const MAX_CLADDING: f64 = 80.0;
pub const SOFFIT_DRIP: f64 = 30.0;
pub const MIN_DRIP: f64 = 20.0;
pub const MAX_DRIP: f64 = 40.0;
pub const BATTEN: f64 = 800.0;
pub const COUNTER: f64 = 400.0;
pub const MIN_BATTEN: f64 = 300.0;
pub const MAX_BATTEN: f64 = 1500.0;
/// Sohlplatte neuer Gebäude (Jörn 10:13): 22 cm, die Frostschürze reicht
/// darunter bis UK Gründung −0,80 (58 cm).
pub const SLAB_THICKNESS: f64 = 220.0;
/// Dickste Sohlplatte (mm), wie im Dialog „Gebäude erstellen“.
pub const MAX_SLAB_THICKNESS: f64 = 790.0;
/// Grenzen (G4): lichte Höhe jedes Geschosses, Schürzentiefe, größte
/// Gründungstiefe.
pub const MIN_CLEAR: f64 = 1000.0;
pub const MIN_FOOTING: f64 = 100.0;
pub const MAX_FOUNDATION: f64 = 10000.0;
/// Kleinster Sockelrücksprung außer 0 (mm).
pub const MIN_RECESS: f64 = 20.0;
/// Mindesteinbindetiefe ins Erdreich (mm): OK Gelände bis UK Gründung,
/// frostfrei (Jörn 10.10., Gelände Thema 1).
pub const FROST_DEPTH: f64 = 800.0;
/// Größter Versatz OK Sohlplatte gegen OK Gelände, nach oben wie unten (mm).
pub const MAX_TERRAIN_OFFSET: f64 = 3000.0;
/// Perimeterdämmung unter der Sohlplatte (Gelände Thema 4): XPS 12 cm beim
/// Einschalten, einstellbar von 2 bis 30 cm.
pub const PERIMETER_THICKNESS: f64 = 120.0;
pub const MIN_PERIMETER: f64 = 20.0;
pub const MAX_PERIMETER: f64 = 300.0;
/// Eingebauter Baustoff der Perimeterdämmung (XPS), angelegt, sobald die
/// erste Dämmung ihn braucht (ältere Dateien bleiben bytegleich).
pub const PERIMETER_MAT_GUID: Guid = Guid(0x3c1f7a92d4e84b06a5b2e9c07d18f455);

/// Breite der Frostschürze (mm): 30 bis 45 cm, unabhängig von der Wand
/// (Jörn 06.10. 17:34).
pub const FOOTING_WIDTH: (f64, f64) = (300.0, 450.0);
/// Teil des Körpers eines Wandzugs: Sohlplatte bzw. Frostschürze (statt Segment).
pub const SLAB_PART: u32 = u32::MAX - 1;
pub const FOOTING_PART: u32 = u32::MAX - 2;
/// Teil des Körpers eines Wandzugs: Erdgeschossdecke darüber.
pub const FLOOR_PART: u32 = u32::MAX - 3;
/// Teil des Körpers eines Wandzugs: Randdämmstreifen auf Segment `k` ist
/// `STRIP_PART + k` (K5).
pub const STRIP_PART: u32 = u32::MAX - 3 - MAX_STRIPS;
const MAX_STRIPS: u32 = 1 << 16;
/// Teil des Körpers eines Wandzugs: Untersichtdämmung unter seiner Decke (G7 K4).
pub const SOFFIT_PART: u32 = STRIP_PART - 1;
/// Teil des Körpers eines Wandzugs: Dachterrasse und Attikablech auf seiner
/// Decke (D1, D3).
pub const TERRACE_PART: u32 = STRIP_PART - 2;
pub const COPING_PART: u32 = STRIP_PART - 3;
/// Teil des Körpers eines Wandzugs: Perimeterdämmung unter seiner
/// Sohlplatte (Gelände Thema 4).
pub const PERIMETER_PART: u32 = STRIP_PART - 4;
/// Teil des Körpers einer Aufkantung: Dachaufbau und Attikablech des
/// Flachdachs (D3, D4); Abstand zu den Teilen oben, damit weitere dazwischen
/// passen.
pub const ROOF_PART: u32 = STRIP_PART - 10;
pub const ROOF_COPING_PART: u32 = STRIP_PART - 11;
/// Teil des Körpers einer Aufkantung: Regenfallrohr am Ablauf `k` ist
/// `DOWNPIPE_PART - k` (Jörn 11.10.), höchstens [`MAX_DOWNPIPES`].
pub const DOWNPIPE_PART: u32 = STRIP_PART - 12;
pub const MAX_DOWNPIPES: u32 = 64;
/// Attika über OK Belag (Jörn 08:35, BIM §3): 6 cm, einstellbar 0 bis 30 cm.
pub const TERRACE_UPSTAND: f64 = 60.0;
pub const MAX_UPSTAND: f64 = 300.0;
/// Dämmstreifen auf der Attikakrone unter dem Blech (Jörn 10.10. 16:07,
/// konzept-attikadaemmung.md): 4 cm, einstellbar 2 bis 10 cm oder 0.
pub const TERRACE_CROWN: f64 = 40.0;
pub const MIN_CROWN: f64 = 20.0;
pub const MAX_CROWN: f64 = 100.0;

/// Gültige Dicke des Dämmstreifens auf der Krone (mm): ungültige Werte
/// (nur aus fremden Dateien) zählen als keiner.
pub(crate) fn crown_of(t: &Terrace) -> f64 {
    if (MIN_CROWN..=MAX_CROWN).contains(&t.crown) {
        t.crown
    } else {
        0.0
    }
}
/// Attikadämmung innen am Flachdach (Jörn 10.10. 16:07,
/// konzept-attikadaemmung.md): 6 cm, einstellbar 2 bis 20 cm oder 0.
pub const ROOF_INNER: f64 = 60.0;
pub const MIN_INNER: f64 = 20.0;
pub const MAX_INNER: f64 = 200.0;

/// Gültige Dicke der Attikadämmung innen (mm), sonst keine.
pub(crate) fn inner_of(t: &Terrace) -> f64 {
    if (MIN_INNER..=MAX_INNER).contains(&t.inner) {
        t.inner
    } else {
        0.0
    }
}
/// Werkstyp „Dachterrasse 14“ (R4 §1.4) und die eingebauten Baustoffe von
/// Dachterrasse und Attikablech: feste Guids, angelegt, sobald das erste
/// Bauteil sie braucht (ältere Dateien bleiben bytegleich).
pub const TERRACE_TYPE_GUID: Guid = Guid(0xa75aeb4f33ba46c696cd177dded0ab96);
pub const TERRACE_FINISH_GUID: Guid = Guid(0x79bfecdae5104f34bd95a7116f06ba0c);
pub const TERRACE_INSULATION_GUID: Guid = Guid(0xd750cacad2e543e3a3e35c5c7f3d8d41);
pub const COPING_MAT_GUID: Guid = Guid(0x97fe946502194178be60bb9eb54c46fb);
/// Die eingebauten Baustoffe, die erst das erste Bauteil anlegt, mit Name
/// und Art: Der Werksbestand verweist auf sie (`mat=`), auch wenn ein
/// Projekt sie noch nicht führt. Dazu die des Ausbaus (Raumbuch).
pub const BUILTIN_MATERIALS: [(Guid, &str, MatCategory); 23] = [
    (
        PERIMETER_MAT_GUID,
        "XPS Perimeterdämmung",
        MatCategory::Insulation,
    ),
    (TERRACE_FINISH_GUID, "Terrassenbelag", MatCategory::Concrete),
    (
        TERRACE_INSULATION_GUID,
        "Dämmung hart (Terrasse)",
        MatCategory::Insulation,
    ),
    (COPING_MAT_GUID, "Titanzink 0,7", MatCategory::Metal),
    (
        flachdach::ROOF_SEAL_GUID,
        "Abdichtung Bitumen 2-lagig",
        MatCategory::Membrane,
    ),
    (
        flachdach::ROOF_INSULATION_GUID,
        "Dämmung EPS 035 DAA dh",
        MatCategory::Insulation,
    ),
    (
        flachdach::ROOF_CROWN_GUID,
        flachdach::ROOF_CROWN_NAME,
        MatCategory::Insulation,
    ),
    (
        flachdach::ROOF_VAPOUR_GUID,
        "Dampfsperre Bitumen-Alu",
        MatCategory::Membrane,
    ),
    (
        FOAMGLASS_MAT_GUID,
        "Schaumglas-Dämmstein",
        MatCategory::Insulation,
    ),
    (
        CLADDING_MAT_GUID,
        "Bekleidung Faserzement",
        MatCategory::Concrete,
    ),
    ausbau::finish_material(0),
    ausbau::finish_material(1),
    ausbau::finish_material(2),
    ausbau::finish_material(3),
    ausbau::finish_material(4),
    ausbau::finish_material(5),
    ausbau::finish_material(6),
    ausbau::finish_material(7),
    ausbau::finish_material(8),
    ausbau::finish_material(9),
    ausbau::finish_material(10),
    ausbau::finish_material(11),
    ausbau::finish_material(12),
];

/// Fußpunkt der Verblendschale auf der Dachterrasse (W1, W2): Baustoff
/// „Schaumglas-Dämmstein“ und Werksschraffur „Dämmung hart (Kreuz)“, feste
/// Guids, angelegt beim ersten Fußpunkt (ältere Dateien bleiben bytegleich).
pub const FOAMGLASS_MAT_GUID: Guid = Guid(0x58d0e30a2b7844caa14c045d3881490a);
/// „Bekleidung Faserzement“ unter der Untersichtdämmung (W3), feste Guids
/// wie beim Schaumglas, angelegt bei der ersten Bekleidung.
pub const CLADDING_MAT_GUID: Guid = Guid(0xa5196bbc329d43338725e72fb7c4b14f);
const CLADDING_SURFACE_GUID: Guid = Guid(0x224a86c94719422c9c2e22fd2776f5df);
pub const CROSS_FILL_GUID: Guid = Guid(0x2929d285ea0e4a69aaf92400ac5b37fc);
/// Oberfläche des Schaumglases, fest wie der Baustoff: beim Lesen angelegt,
/// träfe eine Guid aus der Folge sonst eine schon in der Datei vergebene.
const FOAMGLASS_SURFACE_GUID: Guid = Guid(0x7e5276007e7f4e499b3552895b3474dd);
/// Aufbau des Werkstyps von oben nach unten (Jörn 08:31–08:33).
const TERRACE_BUILD_UP: [(f64, LayerFunction); 2] = [
    (60.0, LayerFunction::Finish),
    (80.0, LayerFunction::Insulation),
];
/// Ab dieser lichten Tiefe gilt die Terrasse als begehbar (BIM E1), mm.
pub const WALKABLE_DEPTH: f64 = 500.0;

/// Warum keine Decke entsteht, als Satz.
/// Zwei Stände eines Baustoffs unterscheiden sich höchstens in den
/// Darstellungsverweisen (beide vorhanden).
fn bim_equal(a: Option<&Material>, b: Option<&Material>) -> bool {
    match (a, b) {
        (Some(a), Some(b)) => {
            let same = Material {
                cut_fill: a.cut_fill,
                cut_fg: a.cut_fg,
                cut_bg: a.cut_bg,
                surface: a.surface,
                ..b.clone()
            };
            same == *a
        }
        _ => false,
    }
}

pub(crate) fn explain_floor(e: FloorError) -> &'static str {
    match e {
        FloorError::NotClosed => "der Wandzug ist nicht geschlossen",
        FloorError::NoCore => "der Wandaufbau hat keine tragende Schicht",
        FloorError::NotSimple => "der Umriss überschneidet sich",
        FloorError::BadThickness => "die Deckendicke muss größer als 0 sein",
        FloorError::BelowWallFoot => "die Unterkante liegt auf oder unter dem Wandfuß",
        FloorError::AboveWallTop => "die Oberkante liegt über der Wandkrone",
    }
}

/// Umschließendes Rechteck von Punkten (Grundriss).
fn bounds2(pts: &[Vec3]) -> (Vec3, Vec3) {
    pts.iter().fold(
        (vec3(f64::MAX, f64::MAX, 0.0), vec3(f64::MIN, f64::MIN, 0.0)),
        |(lo, hi), p| {
            (
                vec3(lo.x.min(p.x), lo.y.min(p.y), 0.0),
                vec3(hi.x.max(p.x), hi.y.max(p.y), 0.0),
            )
        },
    )
}

/// Überschneiden sich zwei Rechtecke (Grundriss)?
fn overlap((alo, ahi): (Vec3, Vec3), (blo, bhi): (Vec3, Vec3)) -> bool {
    alo.x <= bhi.x && blo.x <= ahi.x && alo.y <= bhi.y && blo.y <= ahi.y
}

/// Segmentmitten eines Zuges.
fn mids(pts: &[Vec3], closed: bool) -> Vec<Vec3> {
    segment_lines(pts, closed)
        .into_iter()
        .map(|(a, b)| (a + b) * 0.5)
        .collect()
}

/// Liegt `p` im Vieleck `poly` (Grundriss, Strahltest)?
fn contains(poly: &[Vec3], p: Vec3) -> bool {
    let n = poly.len();
    let mut inside = false;
    for i in 0..n {
        let (a, b) = (poly[i], poly[(i + 1) % n]);
        if (a.y > p.y) != (b.y > p.y) && p.x < a.x + (p.y - a.y) / (b.y - a.y) * (b.x - a.x) {
            inside = !inside;
        }
    }
    inside
}

/// Warum keine Gründung entsteht, als Satz.
pub(crate) fn explain(e: FoundationError) -> &'static str {
    match e {
        FoundationError::NotClosed => "der Wandzug ist nicht geschlossen",
        FoundationError::NotSimple => "der Umriss überschneidet sich",
        FoundationError::RecessTooLarge => "der Rücksprung ist nicht kleiner als die Wanddicke",
        FoundationError::RecessTooSmall => "der Rücksprung liegt zwischen 0 und 20 mm",
        FoundationError::BadSize => "ein Maß ist nicht größer als 0",
    }
}

/// Zug ohne Anschlüsse, zum Erkennen der Anschlüsse.
struct Base {
    run: RunId,
    /// Freie Enden und Segmente: Punkt und Richtung, einmal berechnet (U3).
    ends: [Option<(Vec3, Vec3)>; 2],
    frames: Vec<(Vec3, Vec3)>,
    chain: WallChain,
    segments: Vec<ElementId>,
    foot: Vec<[Vec3; 4]>,
    /// Umschließendes Rechteck des Grundrisses.
    lo: Vec3,
    hi: Vec3,
}

/// Grundrisse der Züge, je Zug einmal je Erkennungslauf berechnet (U3): Ein
/// großer Außenzug ist Kandidat für fast jedes Innenwandende und kostete sonst
/// je Ende einen ganzen Grundriss. Die Züge ändern sich während eines Laufs
/// nicht.
#[derive(Default)]
struct Bases(std::collections::HashMap<RunId, Option<Base>>);

impl Bases {
    fn fill(&mut self, m: &Model, id: RunId) {
        // Je Zug ein Eintrag, nicht ein Platz je Arena-Platz: Beim Ziehen
        // braucht es wenige Züge einer großen Siedlung (L40)
        self.0.entry(id).or_insert_with(|| m.base(id));
    }

    fn get(&self, id: RunId) -> Option<&Base> {
        self.0.get(&id)?.as_ref()
    }
}

/// Liegt `p` im Rechteck `lo`..`hi` (Grundriss)?
fn inside(p: Vec3, lo: Vec3, hi: Vec3) -> bool {
    p.x >= lo.x && p.y >= lo.y && p.x <= hi.x && p.y <= hi.y
}

/// Feste Reihenfolge: nach anschließendem Zug und Ende.
fn sort_joins(j: &mut [Join]) {
    j.sort_by_key(|j| (j.a_run.index(), j.a_end));
}

/// Gleicher Inhalt zweier Typen; der Änderungsstand zählt nicht mit.
pub(crate) fn same_type(a: &LayerSet, b: &LayerSet) -> bool {
    a.guid == b.guid
        && a.name == b.name
        && a.code == b.code
        && a.category == b.category
        && a.layers == b.layers
        && a.props == b.props
        && a.note == b.note
        && a.bearing == b.bearing
        && a.height == b.height
}

/// `code`, wenn `taken` es nicht kennt, sonst „code-2“, „code-3“ …
pub(crate) fn free_code(code: &str, taken: impl Fn(&str) -> bool) -> String {
    if !taken(code) {
        return code.to_string();
    }
    (2..)
        .map(|n| format!("{code}-{n}"))
        .find(|c| !taken(c))
        .unwrap_or_default()
}

/// Aufbau „IW 17,5 Porenbeton“: eine tragende Schicht aus `material`.
pub(crate) fn interior_set(guid: Guid, material: MaterialId) -> LayerSet {
    LayerSet {
        guid,
        name: "IW 17,5 Porenbeton".into(),
        code: "IW-17,5".into(),
        category: TypeCategory::InteriorWall,
        props: PropSet::new(),
        note: String::new(),
        changed: 1,
        bearing: Bearing::Core,
        layers: vec![MaterialLayer::new(material, 175.0, LayerFunction::Structure).core()],
        height: None,
    }
}

/// Segmente eines Zuges als (Anfang, Ende).
/// Stand vorher (Rückgängig) bzw. nachher (Wiederholen).
fn pick<T: Clone>(dir: Direction, old: &T, new: &T) -> T {
    match dir {
        Direction::Undo => old.clone(),
        Direction::Redo => new.clone(),
    }
}

fn segment_lines(pts: &[Vec3], closed: bool) -> Vec<(Vec3, Vec3)> {
    let n = pts.len();
    (0..segment_count(n, closed))
        .map(|k| (pts[k], pts[(k + 1) % n]))
        .collect()
}

/// Ordnet jedem neuen Segment höchstens ein altes zu, mit dem es auf derselben
/// Linie in gleicher Richtung liegt; bei mehreren Kandidaten bekommt die
/// längste Überdeckung den Vorrang.
fn match_segments(old: &[(Vec3, Vec3)], new: &[(Vec3, Vec3)]) -> Vec<Option<usize>> {
    let mut pairs: Vec<(f64, usize, usize)> = Vec::new();
    for (i, &(a, b)) in new.iter().enumerate() {
        for (k, &(c, d)) in old.iter().enumerate() {
            let len = (d - c).length();
            if len < 1e-9 || (b - a).length() < 1e-9 {
                continue;
            }
            let dir = (d - c) * (1.0 / len);
            let dn = (b - a).normalized();
            let side = vec3(-dir.y, dir.x, 0.0);
            let parallel = (dn.x * dir.y - dn.y * dir.x).abs() < 1e-6 && dn.dot(dir) > 0.0;
            if !parallel || (a - c).dot(side).abs() > 1.0 {
                continue;
            }
            let (s0, s1) = ((a - c).dot(dir), (b - c).dot(dir));
            let overlap = s1.min(len) - s0.max(0.0);
            if overlap > 1.0 {
                pairs.push((overlap, i, k));
            }
        }
    }
    pairs.sort_by(|x, y| y.0.total_cmp(&x.0));
    let mut out = vec![None; new.len()];
    let mut used = vec![false; old.len()];
    for (_, i, k) in pairs {
        if out[i].is_none() && !used[k] {
            out[i] = Some(k);
            used[k] = true;
        }
    }
    out
}

/// Kleinster Versatz einer gestapelten Wand; darunter rastet er auf 0 ein
/// (Regel 31, wie der Sockelrücksprung). Erst ab ihm zählt ein Segment als
/// Vor- oder Rücksprung: Deckenstreifen, Untersichtdämmung und Abfangung
/// entstehen nicht für Reste darunter, etwa aus alten Dateien (Review 1t T1).
pub const MIN_OFFSET: f64 = 20.0;

/// Warum „Bündig setzen“ an eine Zielwand nicht geht ([`Model::flush_to`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FlushError {
    /// Die Zielwand steht nicht direkt über oder unter der Wand.
    NotPartners,
    /// Die Wand steht selbst auf einem Geschoss darunter (mehr als zwei
    /// Geschosse).
    Unsupported,
    /// Ein Wandrest würde zu kurz (Regel 30), der Umriss kreuzte sich oder
    /// es bliebe kein Raum.
    Invalid,
}

impl FlushError {
    /// Text für die Hinweiskarte.
    pub fn message(self) -> &'static str {
        match self {
            FlushError::NotPartners => "Die Zielwand steht nicht direkt über oder unter dieser Wand.",
            FlushError::Unsupported => "Diese Wand steht selbst auf einer Wand darunter; sie kann nicht an die Wand darüber rücken.",
            FlushError::Invalid => "Bündig geht hier nicht: Ein Wandstück würde zu kurz oder der Grundriss ungültig.",
        }
    }
}

/// Bleibt innerhalb der Wand eines geschlossenen Zuges ein Raum (Innenfläche
/// mit dem robusten Versatz)? Sonst entstünde keine Decke (G7 K2).
fn room_inside(c: &WallChain) -> bool {
    !c.closed || sk_math::polygon::inset(&c.face_corners(c.outer_offset()), c.thickness()).is_ok()
}

/// Was die Abgleiche beim Schließen eines Schritts wollen
/// ([`Model::abgleich_wunsch`]).
pub(super) struct Wunsch {
    streifen: Vec<(ElementId, ElementId)>,
    untersicht: Vec<ElementId>,
    terrasse: Vec<ElementId>,
    flachdach: Vec<(ElementId, StoreyId, u16)>,
}

impl Model {
    /// Randdämmstreifen, Untersichten, Dachterrassen und Flachdächer, die
    /// `scope` braucht; jede Decke einmal gerechnet.
    pub(super) fn abgleich_wunsch(&self, scope: Option<&[RunId]>) -> Wunsch {
        let (untersicht, terrasse) = self.untersicht_und_terrasse_in(scope);
        Wunsch {
            streifen: self.edge_strip_pairs_in(scope),
            untersicht,
            terrasse,
            flachdach: self.flat_roof_want_in(scope),
        }
    }
}

/// `run` liegt in `scope` (`None`: alles).
fn in_scope(scope: Option<&[RunId]>, run: RunId) -> bool {
    // `scope` ist nach Platz geordnet ([`Model::sync_scope`]); so bleibt der
    // Abgleich bei einem großen Bereich linear (Review L39)
    scope.is_none_or(|s| {
        let i = s.partition_point(|r| r.index() < run.index());
        s[i..]
            .iter()
            .take_while(|r| r.index() == run.index())
            .any(|r| *r == run)
    })
}

#[cfg(test)]
#[path = "model_tests.rs"]
mod tests;

/// OG Phase 2 im Kern (G7 K1–K3): gelöste und versetzte OG-Segmente.
#[cfg(test)]
#[path = "og_phase2.rs"]
mod og_phase2;

/// Gelände Thema 1 und 4: Versatz OK Sohle über Gelände, Frosttiefe und
/// Perimeterdämmung.
#[cfg(test)]
#[path = "gelaende_tests.rs"]
mod gelaende_tests;
