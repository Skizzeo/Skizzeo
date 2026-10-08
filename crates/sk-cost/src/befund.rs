//! Befunde: Regelnummer, Satz für Menschen und Ort (BIM §4, K6). Die Sätze
//! stehen wörtlich wie in der Satztabelle, `{…}` eingesetzt.

use sk_model::Guid;
use std::fmt;

/// Wie schwer ein Befund wiegt.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Schwere {
    /// Zur Kenntnis (Marke, Herkunft).
    Hinweis,
    /// Gerechnet wird, aber geschätzt oder unvollständig.
    Warnung,
    /// Satz zählt nicht oder Änderung abgelehnt.
    Fehler,
}

/// Wo der Befund sitzt.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Ort {
    /// Die Datei als Ganzes.
    Datei,
    /// Ein Kostensatz: Abschnitt und Kennung (oder `n`-te Zeile ohne Kennung).
    Satz {
        abschnitt: &'static str,
        kennung: String,
    },
    /// Schicht `schicht` des Typs `typ`.
    Schicht { typ: Guid, schicht: usize },
    /// Ein Bauteil.
    Bauteil(Guid),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Befund {
    /// Regelnummer nach BIM (71–107).
    pub regel: u16,
    pub schwere: Schwere,
    pub satz: String,
    pub ort: Ort,
}

impl Befund {
    pub fn neu(regel: u16, schwere: Schwere, satz: impl Into<String>, ort: Ort) -> Befund {
        Befund {
            regel,
            schwere,
            satz: satz.into(),
            ort,
        }
    }

    pub fn fehler(regel: u16, satz: impl Into<String>, ort: Ort) -> Befund {
        Befund::neu(regel, Schwere::Fehler, satz, ort)
    }

    pub fn warnung(regel: u16, satz: impl Into<String>, ort: Ort) -> Befund {
        Befund::neu(regel, Schwere::Warnung, satz, ort)
    }

    pub fn hinweis(regel: u16, satz: impl Into<String>, ort: Ort) -> Befund {
        Befund::neu(regel, Schwere::Hinweis, satz, ort)
    }
}

impl fmt::Display for Befund {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Regel {}: {}", self.regel, self.satz)
    }
}

/// Ort eines Kostensatzes.
pub fn satz_ort(abschnitt: &'static str, kennung: impl Into<String>) -> Ort {
    Ort::Satz {
        abschnitt,
        kennung: kennung.into(),
    }
}

// --- Sätze nach BIM §4 -----------------------------------------------------

pub fn r71() -> String {
    "Diese Datei enthält Kostendaten einer neueren Skizzeo-Fassung; sie bleiben unverändert erhalten."
        .into()
}

pub fn r72(n: usize, abschnitt: &str, grund: &str) -> String {
    format!(
        "Zeile {n} ({abschnitt}) wurde übersprungen: {grund}. Sie bleibt unverändert in der Datei."
    )
}

pub fn r73(satz: &str, art: &str, kennung: &str) -> String {
    format!("{satz} verweist auf {art} {kennung}, die es in dieser Datei nicht gibt.")
}

/// R73-W (Bausteingrenze §6): Werksbaustoff einer älteren Datei.
pub fn r73w(name: &str) -> String {
    format!("Baustoff {name}: Werkspreise über den Namen zugeordnet (ältere Datei).")
}

pub fn r74(abschnitt: &str, kennung: &str) -> String {
    format!("{abschnitt} {kennung} kommt doppelt vor; es gilt die erste Zeile.")
}

pub fn r75(name: &str) -> String {
    format!("Werkswert {name} hat eine andere Kennung als im Werksbestand.")
}

pub fn r76(name: &str, feld: &str, wert: &str) -> String {
    format!("Artikel {name}: {feld} ist ungültig ({wert}).")
}

pub fn r77(baustoff: &str, dicke: &str, name: &str) -> String {
    format!("Für {baustoff} {dicke} sind mehrere Standardartikel gesetzt; es gilt {name}.")
}

pub fn r79(kurztext: &str, feld: &str, wert: &str) -> String {
    format!("Bauleistung {kurztext}: {feld} ist ungültig ({wert}).")
}

pub fn r80(kurztext: &str, einheit: &str, bezug: &str) -> String {
    format!("Bauleistung {kurztext}: Einheit {einheit} passt nicht zur Menge {bezug}.")
}

pub fn r85_kette(kurztext: &str, folge: &str) -> String {
    format!(
        "{kurztext}: Folgeposition {folge} ist selbst Folgeposition und darf keine weitere haben."
    )
}

pub fn r86(oz: &str) -> String {
    format!("Ordnungszahl {oz} ist doppelt vergeben.")
}

pub fn r87(name: &str) -> String {
    format!("{name} ist ausgemustert, wird aber noch verwendet.")
}

pub fn r88(kennung: &str) -> String {
    format!("Herkunftsangabe zu {kennung}: Den Datensatz gibt es nicht.")
}

pub fn r88_hand(kennung: &str) -> String {
    format!("Herkunftsangabe zu {kennung}: Handeingaben sind immer bestätigt.")
}

pub fn r89(name: &str, stand: &str) -> String {
    format!("{name} weicht vom Firmenkatalog Stand {stand} ab.")
}

pub fn r90(nr: &str) -> String {
    format!("Protokollzeile {nr} fehlt oder steht außer der Reihe.")
}

pub fn r91() -> String {
    "Dieser Firmenkatalog ist ein Entwurf und noch nicht freigegeben.".into()
}

pub fn r92(stand: u32, eigener: &str) -> String {
    format!("Firmenkatalog Stand {stand} ist verfügbar; das Projekt rechnet mit Stand {eigener}.")
}

pub fn r93(operation: &str, grund: &str) -> String {
    format!("Änderung {operation} abgelehnt: {grund}.")
}

pub fn r99(typ: &str, baustoff: &str) -> String {
    format!("{typ}, Schicht {baustoff}: Die gewählte Bauleistung gibt es nicht (mehr); es gilt die Regel.")
}
