//! Gebäudedatenmodell. Einheit: Millimeter, Z nach oben.
//!
//! [`Model`] ist die Datenbank des Gebäudes: Baustoffe und Aufbauten als
//! Bibliothek, Geschosse, Bauteile mit [`Guid`] und Nummer, Wandzüge als
//! Eingabe. Körper ([`Solid`]) werden daraus abgeleitet.

#![forbid(unsafe_code)]

pub mod attr;
pub mod catalog;
pub mod element;
pub mod erweiterung;
pub mod erweiterung_koerper;
pub mod ext;
pub mod floor;
pub mod foundation;
pub mod guid;
pub mod id;
pub mod join;
pub mod kinds;
pub mod library;
pub mod matprop;
pub mod model;
pub mod proctex;
pub mod qto;
pub mod qto_dach;
pub mod qto_earth;
pub mod qto_fassade;
pub mod qto_site;
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
pub mod wall;

pub use attr::{
    display_slots, AttrRef, AttrUser, Attributes, Dash, Display, EdgeStyle, Fill, FillId, FillKind,
    FillSpace, HatchLine, LineType, LineTypeId, Pen, PenId, Surface, SurfaceId,
};
pub use catalog::{
    compare, compare_materials, compare_surfaces, export_material, export_surface, export_type,
    import_material, import_type, read_szk, read_szk_with, save_preset, sync_materials,
    sync_materials_report, write_szk, Library, RawSvc, TypeState,
};
pub use element::{
    Building, BuildingId, Category, Coupling, Element, ElementId, ElementKind, LevelEdge,
    LevelKind, LevelRef, PropSet, PropValue, RunId, Soffit, Storey, StoreyId, Terrace, Wall,
    WallRun,
};
pub use erweiterung::{ExtDef, ExtPart};
pub use ext::{ExtRec, ExtStore};
pub use floor::{FloorError, FloorParams, FloorSlab, SoffitParams, StripParams, TerraceParams};
pub use foundation::{FootingShape, Foundation, FoundationError, FoundationParams};
pub use guid::{Guid, GuidGen};
pub use id::{Arena, Id};
pub use join::{Join, JoinEnd, JoinKind};
pub use library::{
    material_key, type_code, Bearing, LayerFunction, LayerSet, LayerSetId, MatCategory, Material,
    MaterialDisplay, MaterialId, MaterialLayer, TypeCategory, TERRACE_FINISH, TERRACE_INSULATION,
    TYPE_PROPS,
};
pub use matprop::{MatProp, MatPropKind, MAT_PROPS};
pub use model::{
    building_index, edit_blocked, refusal_lines, refusal_text, Cut, Defaults, Deleted, ExtError,
    FlushError, Foot, GroundBasis, Location, Locked, Model, NumberError, Project, Refusal,
    ShadeLight, Sun, Use, ViewShade, CAVITY_TYPE_GUID, COPING_PART, CUT_NAMES, ETICS_TYPE_GUID,
    EXTERIOR_TYPE_GUID, EXT_SEQ, FLOOR_PART, FLOOR_THICKNESS, FOOTING_PART, FOOTING_WIDTH,
    FROST_DEPTH, INTERIOR_115_TYPE_GUID, INTERIOR_240_TYPE_GUID, INTERIOR_TYPE_GUID,
    MAX_FOUNDATION, MAX_PERIMETER, MAX_SOFFIT, MAX_TERRAIN_OFFSET, MAX_UPSTAND, MIN_CLEAR,
    MIN_FOOTING, MIN_PERIMETER, MIN_RECESS, MIN_SOFFIT, MONO_TYPE_GUID, PERIMETER_MAT_GUID,
    PERIMETER_PART, PERIMETER_THICKNESS, SHADE_VIEWS, SLAB_PART, SLAB_THICKNESS, SOFFIT_PART,
    SOFFIT_THICKNESS, STRIP_PART, TERRACE_PART, TERRACE_TYPE_GUID, TERRACE_UPSTAND, WALKABLE_DEPTH,
};
pub use qto::{
    coping_qto, coping_qto_of, edge_strip_qto, edge_strip_qto_of, floor_formwork_of, floor_qto,
    floor_qto_of, formwork_qto, foundation_qto, foundation_qto_of, perimeter_qto_of, run_qto,
    soffit_qto, soffit_qto_of, terrace_qto, terrace_qto_of, wall_qto, CopingQto, EdgeStripQto,
    FloorQto, FootingQto, FormworkQto, LayerQto, PerimeterQto, SlabQto, SoffitQto, SupportQto,
    TerraceQto, Umfang, WallQto,
};
pub use solid::Solid;
pub use solid::{edge_kind, material, merge_seam, Edge, SweepEnd, Tri, NO_LAYER};
pub use terrace::{AttikaPiece, CopingPath, TerraceOutline, TerracePlan, COPING_DRIP};
pub use trade::{Trade, TradeId};
pub use txn::{step_label, Change, Direction, Touched, Txn};
pub use wall::{Attika, EndCut, Gap, Joints, Layer, Line2, Overhang, RefSide, WallChain};
