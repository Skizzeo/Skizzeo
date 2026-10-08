//! Kosten, AVA und Stammdaten (Regel K0, architektur/bausteingrenze-sk-cost.md).
//!
//! Abhängig nur nach unten (`sk-model`, `sk-math`). Schreiben geht nur über
//! die Operationen, Lesen nur über [`lesen`]. Die Kostenzeilen liegen roh im
//! Erweiterungsspeicher von `sk-model`; gedeutet werden sie nur hier, aus
//! einer Feldtabelle ([`satz`]).

pub mod befund;
pub mod geld;
pub mod katalog;
pub mod lesen;
pub mod satz;
mod schema;
mod zeile;

pub use befund::{Befund, Ort, Schwere};
pub use geld::{Cent, Dez};
pub use katalog::Katalog;
pub use schema::schema;

/// Werksbestand von „Stammdaten und BIM-Administration“, bytegleiche Kopie
/// von stammdaten/werk.szk (Regel 75, Bausteingrenze §6).
pub const WERK: &str = include_str!("../werk.szk");

/// Kostenzeilen des Werksbestands als `(abschnitt, zeile)`.
pub(crate) fn werk_zeilen() -> Vec<(&'static str, &'static str)> {
    WERK.lines()
        .filter_map(|l| {
            let z = zeile::zerlegen(l)?;
            let a = satz::abschnitt(&z.abschnitt)?;
            Some((a.name, l))
        })
        .collect()
}
