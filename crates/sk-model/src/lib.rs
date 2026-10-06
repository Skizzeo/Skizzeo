//! Gebäudedatenmodell. Einheit: Millimeter, Z nach oben.
//!
//! [`Model`] ist die Datenbank des Gebäudes: Baustoffe und Aufbauten als
//! Bibliothek, Geschosse, Bauteile mit [`Guid`] und Nummer, Wandzüge als
//! Eingabe. Körper ([`Solid`]) werden daraus abgeleitet.

#![forbid(unsafe_code)]

pub mod attr;
pub mod element;
pub mod guid;
pub mod id;
pub mod join;
pub mod library;
pub mod model;
pub mod qto;
pub mod solid;
pub mod szo;
pub mod txn;
pub mod wall;

pub use attr::{
    Attributes, Dash, Display, EdgeStyle, Fill, FillId, FillKind, FillSpace, HatchLine, LineType,
    LineTypeId, Pen, PenId, Surface, SurfaceId,
};
pub use element::{
    Category, Element, ElementId, ElementKind, PropSet, PropValue, RunId, Storey, StoreyId, Wall,
    WallRun,
};
pub use guid::{Guid, GuidGen};
pub use id::{Arena, Id};
pub use join::{Join, JoinEnd, JoinKind};
pub use library::{
    material_key, LayerFunction, LayerSet, LayerSetId, MatCategory, Material, MaterialId,
    MaterialLayer,
};
pub use model::{Defaults, Model, NumberError, Project};
pub use qto::{run_qto, wall_qto, LayerQto, WallQto};
pub use solid::Solid;
pub use solid::{edge_kind, material, Edge, Tri};
pub use txn::{Change, Direction, Touched, Txn};
pub use wall::{EndCut, Gap, Joints, Layer, Line2, RefSide, WallChain};
