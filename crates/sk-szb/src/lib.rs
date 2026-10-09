//! Erweiterungsbauteile im Format SZB 0 (Skizzeo-Bauteilvertrag 0.5).
//!
//! Ein Bauteil ist kein Programmcode, sondern eine Beschreibung: Parameter,
//! abgeleitete Werte, Körper aus Grundformen und Mengen, alles über Formeln.
//! Dieses Crate liest die Beschreibung ([`lesen`]), rechnet Formeln
//! ([`formel`]), prüft sie wie die Werkbank ([`pruefen`]) und rechnet daraus
//! Körper und Mengen ([`rechnen`]). Es kennt weder Modell noch Oberfläche;
//! Skizzeo setzt die Ergebnisse in Geschosse und Paneele um.
//!
//! Maßstab für jedes Ergebnis ist die Werkbank des Entwicklerpakets 0.5:
//! gleiche Befunde, gleiche Körper, gleiche Mengen.

#![forbid(unsafe_code)]

pub mod bestand;
pub mod formel;
pub mod lesen;
pub mod pruefen;
pub mod rechnen;

pub use bestand::Bestand;
pub use lesen::{lesen, Def, Satz};
pub use pruefen::{pruefen, pruefen_beim_oeffnen, Pruefung};
pub use rechnen::{rechnen, Ergebnis, Geschoss, Koerper};

/// Stufe eines Befunds: Fehler verhindern das Einlesen, Hinweise nicht.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stufe {
    Fehler,
    Hinweis,
}

/// Ein Befund der Prüfung, mit Zeilennummer der Datei (0 = ganze Datei).
#[derive(Clone, Debug, PartialEq)]
pub struct Befund {
    pub stufe: Stufe,
    pub zeile: usize,
    pub text: String,
}

impl Befund {
    pub fn fehler(zeile: usize, text: impl Into<String>) -> Befund {
        Befund {
            stufe: Stufe::Fehler,
            zeile,
            text: text.into(),
        }
    }

    pub fn hinweis(zeile: usize, text: impl Into<String>) -> Befund {
        Befund {
            stufe: Stufe::Hinweis,
            zeile,
            text: text.into(),
        }
    }

    pub fn ist_fehler(&self) -> bool {
        self.stufe == Stufe::Fehler
    }
}

/// Zahl wie in der Werkbank: deutsch, `d` Nachkommastellen, Tausenderpunkt;
/// „–“, wenn keine Zahl.
pub fn zahl(v: f64, d: usize) -> String {
    if !v.is_finite() {
        return "–".into();
    }
    let s = format!("{:.*}", d, v.abs());
    let (ganz, rest) = s.split_once('.').unwrap_or((&s, ""));
    let mut g = String::new();
    for (i, c) in ganz.chars().enumerate() {
        if i > 0 && (ganz.len() - i) % 3 == 0 {
            g.push('.');
        }
        g.push(c);
    }
    let null = s.chars().all(|c| c == '0' || c == '.');
    let mut out = if v < 0.0 && !null {
        "-".to_string()
    } else {
        String::new()
    };
    out.push_str(&g);
    if !rest.is_empty() {
        out.push(',');
        out.push_str(rest);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zahl_deutsch() {
        assert_eq!(zahl(1234.5, 2), "1.234,50");
        assert_eq!(zahl(0.152, 3), "0,152");
        assert_eq!(zahl(-3.0, 0), "-3");
        assert_eq!(zahl(-0.0001, 2), "0,00");
        assert_eq!(zahl(1_000_000.0, 0), "1.000.000");
        assert_eq!(zahl(f64::NAN, 2), "–");
    }
}
