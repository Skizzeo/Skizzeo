//! Kosten, AVA und Stammdaten (Regel K0, architektur/bausteingrenze-sk-cost.md).
//!
//! Abhängig nur nach unten (`sk-model`, `sk-math`). Schreiben geht nur über
//! die Operationen, Lesen nur über [`lesen`]. Die Kostenzeilen liegen roh im
//! Erweiterungsspeicher von `sk-model`; gedeutet werden sie nur hier, aus
//! einer Feldtabelle ([`satz`]).

pub mod abgleich;
pub mod ablauf;
pub mod befund;
pub mod din276;
pub mod einheit;
pub mod erweiterung;
pub mod geld;
pub mod gliederung;
pub mod katalog;
pub mod lesen;
pub mod lv;
pub mod neue_saetze;
pub mod op;
pub mod preis;
pub mod rechnung;
pub mod satz;
mod schema;
pub use sk_model::sha256;
pub mod verwaltung;
pub mod wahl;
pub mod wort;
mod zeile;
pub mod zuordnung;

pub use befund::{Befund, Ort, Schwere};
pub use geld::{Cent, Dez};
pub use katalog::Katalog;
pub use op::{
    ausfuehren, ausfuehren_folge, entwurf_anwenden, firma_anwenden, neues_projekt, pruefen,
    vorschau, vorschau_kosten, vorschlag_werte, Aenderung, FirmaNeu, Herkunft, HerkunftArt, Op,
    Plan, Rolle, SatzId, SatzNeu, Sicherheit, VorschlagWert, Ziel,
};
pub use rechnung::{Kostenblatt, Kostenspeicher, Position};
pub use schema::schema;
pub use sk_model::qto::Umfang;

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
