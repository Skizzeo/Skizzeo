//! Gebäudedatenmodell. Einheit: Millimeter, Z nach oben.

#![forbid(unsafe_code)]

pub mod solid;
pub mod wall;

pub use solid::Solid;
pub use solid::{material, Tri};
pub use wall::{exterior_wall_layers, Layer, RefSide, WallChain};
