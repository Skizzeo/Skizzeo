//! Gebäudedatenmodell. Einheit: Millimeter, Z nach oben.
//!
//! [`Model`] ist die Datenbank des Gebäudes: Baustoffe und Aufbauten als
//! Bibliothek, Geschosse, Bauteile mit [`Guid`] und Nummer, Wandzüge als
//! Eingabe. Körper ([`Solid`]) werden daraus abgeleitet.

#![forbid(unsafe_code)]

pub mod element;
pub mod guid;
pub mod id;
pub mod library;
pub mod model;
pub mod qto;
pub mod solid;
pub mod wall;

pub use element::{
    Category, Element, ElementId, ElementKind, PropSet, PropValue, RunId, Storey, StoreyId, Wall,
    WallRun,
};
pub use guid::{Guid, GuidGen};
pub use id::{Arena, Id};
pub use library::{
    material_key, Hatch, LayerFunction, LayerSet, LayerSetId, MatCategory, Material, MaterialId,
    MaterialLayer,
};
pub use model::{Defaults, Model, NumberError};
pub use qto::{run_qto, wall_qto, LayerQto, WallQto};
pub use solid::Solid;
pub use solid::{edge_kind, material, Edge, Tri};
pub use wall::{Layer, RefSide, WallChain};
