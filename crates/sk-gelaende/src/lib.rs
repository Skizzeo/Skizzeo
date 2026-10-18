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
pub mod glatt;
pub mod inflate;
pub mod krypto;
pub mod lageplan;
pub mod lesen;
pub mod linien;
pub mod netz;
pub mod pdf;
pub mod planung;
pub mod praedikate;
pub mod pruefen;
pub mod seite;
pub mod volumen;

pub use netz::{netz, Netz, Rand};

/// Wie lange dieser Faden bisher lauffähig auf eine CPU gewartet hat, in
/// ms (Linux: zweites Feld von /proc/thread-self/schedstat; sonst 0). Für
/// die Zeittests: Ziehen sie die Wartezeit ab, verdrängen die nebeneinander
/// laufenden Tests sie nicht über ihr Budget (eine Fassung für alle,
/// Review H-282a).
#[doc(hidden)]
pub fn auf_cpu_gewartet() -> f64 {
    std::fs::read_to_string("/proc/thread-self/schedstat")
        .ok()
        .and_then(|s| s.split_whitespace().nth(1)?.parse::<f64>().ok())
        .map_or(0.0, |ns| ns / 1e6)
}
