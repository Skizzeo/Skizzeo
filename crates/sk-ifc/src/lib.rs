//! IFC-Austausch (ISO 16739-1): Export des Gebäudemodells nach IFC4 Reference
//! View, STEP-Schreiber und -Leser (ISO 10303-21), Vorschau einer gelesenen
//! Datei. Hängt nur an `sk-model`
//! und `sk-math`, kennt die App nicht (architektur.md §9.2). Plan:
//! `ifc/plan.md` im Projektordner.

#![forbid(unsafe_code)]

pub mod export;
pub mod geometrie;
pub mod klassen;
pub mod lesen;
pub mod step;

pub use export::{export, Bericht, Grund, Optionen};
