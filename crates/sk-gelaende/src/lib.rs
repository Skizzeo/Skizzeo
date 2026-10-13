//! Rechenkern des Geländemoduls (plan-gelaendemodul.md §7): Höhenaufmaß
//! lesen und prüfen, Dreiecksnetz durch die Messpunkte, Höhe an einer
//! Stelle, Profil entlang einer Linie, Höhenlinien und Flächenmittel;
//! dazu der Lageplan als PDF (§4.5, G1c): Flate entpacken, Standard-Schutz
//! öffnen, Objekte und Inhaltsstrom lesen, Lage im UTM-Gitter einpassen
//! und das Flurstück samt Ecken und Fläche finden. Gelesen werden
//! Punktlisten und ASCII-DXF.
//!
//! Gerechnet wird in Metern relativ zu einem Bezugspunkt nahe den Punkten.
//! Die Crate kennt weder das Modell noch Ausbaustufen; sie bekommt Bytes
//! oder Punkte und gibt Punkte, Netze, Linien, Höhen und Hinweise zurück.

#![forbid(unsafe_code)]

pub mod abfrage;
pub mod codes;
pub mod dxf;
pub mod inflate;
pub mod krypto;
pub mod lageplan;
pub mod lesen;
pub mod linien;
pub mod netz;
pub mod pdf;
pub mod praedikate;
pub mod pruefen;
pub mod seite;
pub mod volumen;

pub use netz::{netz, Netz, Rand};
