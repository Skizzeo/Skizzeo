//! Räume aus Wänden (Grundriss G3): Erkennung je Geschoss, Begrenzung je
//! Kante und Abgleich mit den gespeicherten Räumen. Hängt nur an `sk-model` und `sk-math` (architektur.md §2). Plan und
//! Schnittstelle: `grundriss/plan.md`, `grundriss/schnittstelle-raum.md` im
//! Projektordner. Dazu das Raumbuch (RB3, `raumbuch/plan.md`): Öffnungen und
//! Höhen je Raum, Ausbau nach der Raumvorlage, Mengen nach VOB/C und Körper.
//! Daraus die Wohnfläche nach WoFlV (G8, [`wohnflaeche`]).

#![forbid(unsafe_code)]

pub mod abgleich;
pub mod ausbau;
pub mod erkennung;
pub mod flaechen;
pub mod mengen;
pub mod oeffnungen;
pub mod raumbuch;
pub mod raumliste;
mod ring;
pub mod speicher;
pub mod wohnflaeche;

pub use abgleich::{abgleichen, abgleichen_lage, ruhende, Abgleich, Stand, Wechsel};
pub use erkennung::{raeume, Begrenzung, Ecke, Kante, RaumForm, Seite};
pub use speicher::{Erkennung, Ketten, Lage, Raumflaeche, Raumformen};
