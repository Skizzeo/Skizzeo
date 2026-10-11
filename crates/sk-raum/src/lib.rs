//! Räume aus Wänden (Grundriss G3): Erkennung je Geschoss, Begrenzung je
//! Kante und Abgleich mit den gespeicherten Räumen. Hängt nur an `sk-model` und `sk-math` (architektur.md §2). Plan und
//! Schnittstelle: `grundriss/plan.md`, `grundriss/schnittstelle-raum.md` im
//! Projektordner.

#![forbid(unsafe_code)]

pub mod abgleich;
pub mod erkennung;
pub mod flaechen;

pub use abgleich::{abgleichen, ruhende};
pub use erkennung::{raeume, Begrenzung, Ecke, Kante, RaumForm, Seite};
