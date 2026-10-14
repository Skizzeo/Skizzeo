//! Gebäudedatenmodell. Einheit: Millimeter, Z nach oben.
//!
//! [`Model`] ist die Datenbank des Gebäudes: Baustoffe und Aufbauten als
//! Bibliothek, Geschosse, Bauteile mit [`Guid`] und Nummer, Wandzüge als
//! Eingabe. Körper ([`Solid`]) werden daraus abgeleitet.

#![forbid(unsafe_code)]

pub mod attr;
pub mod bemassung;
pub mod catalog;
pub mod decke_aussparung;
pub mod din276;
pub mod element;
pub mod erweiterung;
pub mod erweiterung_koerper;
pub mod ext;
pub mod fallrohr;
pub mod floor;
pub mod foundation;
pub mod fuge;
pub mod gefaelle;
pub mod guid;
pub mod hoehen;
pub mod id;
pub mod join;
pub mod kinds;
pub mod koerper;
pub mod library;
pub mod matprop;
pub mod model;
pub mod oeffnung;
pub mod oeffnung_koerper;
#[cfg(test)]
mod oeffnung_tests;
pub mod proctex;
pub mod qto;
pub mod qto_dach;
pub mod qto_earth;
pub mod qto_fassade;
pub mod qto_laibung;
pub mod qto_oeffnung;
pub mod qto_site;
pub mod rechenweg;
pub mod roof;
pub mod sha256;
pub mod solid;
pub mod szo;
pub mod terrace;
pub(crate) mod texgen;
pub mod trade;
pub mod tree;
pub mod txn;
#[cfg(test)]
mod type_tests;
pub mod view;
pub mod vob;
#[cfg(test)]
mod vob_tests;
pub mod wall;
pub mod wand_loch;
#[cfg(test)]
mod wand_tests;

pub use attr::{
    display_slots, AttrRef, AttrUser, Attributes, Dash, Display, EdgeStyle, Fill, FillId, FillKind,
    FillSpace, HatchLine, LineType, LineTypeId, Pen, PenId, Surface, SurfaceId,
};
pub use catalog::{
    compare, compare_materials, compare_materials_ohne, compare_surfaces, export_material,
    export_space_type, export_surface, export_type, import_material, import_space_type,
    import_type, read_szk, read_szk_with, save_preset, sync_materials, sync_materials_report,
    write_szk, Library, RawSvc, TypeState,
};
pub use element::{
    Building, BuildingId, Category, CladdingValue, Coupling, Element, ElementId, ElementKind,
    LevelEdge, LevelKind, LevelRef, PropSet, PropValue, RunId, Soffit, Storey, StoreyId, Terrace,
    Wall, WallRun,
};
pub use erweiterung::{ExtDef, ExtPart};
pub use ext::{ExtRec, ExtStore};
pub use floor::{
    FloorError, FloorParams, FloorSlab, SoffitParams, StripParams, StripTeil, TerraceParams,
};
pub use foundation::{FootingShape, Foundation, FoundationError, FoundationParams};
pub use gefaelle::{Drainage, SlopeField};
pub use guid::{Guid, GuidGen};
pub use hoehen::{
    HeightLines, LINTEL, MAX_PLAN_CUT, MIN_PLAN_CUT, PLAN_CUT, PLAN_CUT_TOP_GAP, SILL,
};
pub use id::{Arena, Id};
pub use join::{Join, JoinEnd, JoinKind};
pub use library::{
    material_key, type_code, Bearing, LayerFunction, LayerSet, LayerSetId, MatCategory, Material,
    MaterialDisplay, MaterialId, MaterialLayer, TypeCategory, ROOF_INSULATION, TERRACE_FINISH,
    TERRACE_INSULATION, TYPE_PROPS,
};
pub use matprop::{MatProp, MatPropKind, MAT_PROPS};
pub use model::FINISH_VIEWS;
pub use model::{affine_apply, Affine, SitePlan, Underlay, UnderlayKind};
pub use model::{
    building_index, edit_blocked, im_bereich, refusal_lines, refusal_text, Cut, Defaults, Deleted,
    ExtError, FlushError, Foot, FugeError, GroundBasis, Location, Locked, Model, NumberError,
    Project, Refusal, ShadeLight, Sun, Use, Utm, ViewShade, CAVITY_TYPE_GUID, COPING_PART,
    CUT_NAMES, DOWNPIPE_PART, ETICS_TYPE_GUID, EXTERIOR_TYPE_GUID, EXT_SEQ, FLOOR_PART,
    FLOOR_THICKNESS, FOOTING_PART, FOOTING_WIDTH, FROST_DEPTH, INTERIOR_115_TYPE_GUID,
    INTERIOR_240_TYPE_GUID, INTERIOR_TYPE_GUID, MAX_CROWN, MAX_DOWNPIPES, MAX_FOUNDATION,
    MAX_INNER, MAX_PERIMETER, MAX_ROOF_UPSTAND, MAX_SLAB_THICKNESS, MAX_SOFFIT, MAX_TERRAIN_OFFSET,
    MAX_UPSTAND, MIN_CLEAR, MIN_CROWN, MIN_FOOTING, MIN_INNER, MIN_PERIMETER, MIN_RECESS,
    MIN_ROOF_EDGE, MIN_ROOF_UPSTAND, MIN_SOFFIT, MONO_TYPE_GUID, NHN, PERIMETER_MAT_GUID,
    PERIMETER_PART, PERIMETER_THICKNESS, ROOF_COPING_PART, ROOF_INNER, ROOF_PART, ROOF_TYPE_GUID,
    ROOF_UPSTAND, SHADE_VIEWS, SLAB_PART, SLAB_THICKNESS, SOFFIT_PART, SOFFIT_THICKNESS,
    STRIP_PART, TERRACE_CROWN, TERRACE_PART, TERRACE_TYPE_GUID, TERRACE_UPSTAND, UTM_N,
    WALKABLE_DEPTH,
};
pub use model::{
    Crs, HeightRef, HeightSystem, LocalFrame, Show, SurveyFrame, Terrain, TerrainFolgen,
    TerrainLine, TerrainMesh, TerrainPoint, Trim,
};
pub use model::{EckeFehler, Fangziele, OpeningFault, Wandende, Wandflaeche, ZugPunkte};
pub use model::{FreieRaumnamen, Space, SPACE_SEQ};
pub use model::{PlateInput, PlateSettings, TerrainPlate, PLATE_MARGIN, PLATE_THICK};
pub use model::{SpaceGroup, SpaceType, WallTiles, DEFAULT_SPACE_GUID};
pub use model::{
    BATTEN, CLADDING_MAT_GUID, COUNTER, FOAMGLASS_MAT_GUID, MAX_BATTEN, MAX_CLADDING, MAX_DRIP,
    MIN_BATTEN, MIN_CLADDING, MIN_DRIP, SOFFIT_CLADDING, SOFFIT_DRIP,
};
pub use qto::{
    coping_qto, coping_qto_of, edge_strip_qto, edge_strip_qto_of, flat_roof_qto, flat_roof_qto_of,
    floor_formwork_of, floor_qto, floor_qto_of, formwork_qto, foundation_qto, foundation_qto_of,
    perimeter_qto_of, roof_coping_qto_of, run_qto, soffit_qto, soffit_qto_of, terrace_qto,
    terrace_qto_of, wall_qto, CopingQto, EdgeStripQto, FloorQto, FootingQto, FormworkQto, LayerQto,
    PerimeterQto, SlabQto, SoffitQto, SupportQto, TerraceQto, Umfang, WallQto,
};
pub use roof::FlatRoof;
pub use solid::Solid;
pub use solid::{edge_kind, material, merge_seam, Edge, SweepEnd, Tri, NO_LAYER};
pub use terrace::{AttikaPiece, CopingPath, TerraceOutline, TerracePlan, COPING_DRIP};
pub use trade::{Trade, TradeId};
pub use txn::{step_label, Change, Direction, Touched, Txn};
pub use view::{ENTWAESSERUNG, SICHT_VIEWS};
pub use wall::{Attika, EndCut, Gap, Joints, Layer, Line2, Overhang, RefSide, WallChain};
