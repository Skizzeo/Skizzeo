//! Rechenkern des Geländemoduls (plan-gelaendemodul.md §7): Höhenaufmaß
//! lesen und prüfen, Dreiecksnetz durch die Messpunkte, Höhe an einer
//! Stelle, Profil entlang einer Linie, Höhenlinien und Flächenmittel.
//!
//! Gerechnet wird in Metern relativ zu einem Bezugspunkt nahe den Punkten.
//! Die Crate kennt weder das Modell noch Ausbaustufen; sie bekommt Bytes
//! oder Punkte und gibt Punkte, Netze, Höhen und Hinweise zurück.

#![forbid(unsafe_code)]

pub mod abfrage;
pub mod codes;
pub mod inflate;
pub mod lesen;
pub mod linien;
pub mod netz;
pub mod praedikate;
pub mod pruefen;
pub mod volumen;

pub use netz::{netz, Netz, Rand};
