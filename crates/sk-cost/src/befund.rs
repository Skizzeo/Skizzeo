//! Befunde: Regelnummer, Satz für Menschen und Ort (BIM §4, K6). Die Sätze
//! stehen wörtlich wie in der Satztabelle, `{…}` eingesetzt.

use crate::wort;
use sk_model::Guid;

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
    /// Eine Position im LV, mit ihrer OZ ohne Los (AVA, KA-4).
    Position(String),
    /// Kopf und Vorbemerkungen des LV (AVA, KA-4).
    Kopf,
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

impl Befund {
    /// Für Fehlerprotokoll, Testausgaben und KI (Bausteingrenze §5): mit
    /// Regel und Ort, „Regel 76 · article 1S7…: Artikel …“. Im Fenster steht
    /// nur `satz`; ein `Display` gibt es absichtlich nicht.
    pub fn protokoll(&self) -> String {
        let ort = match &self.ort {
            Ort::Datei => "Datei".to_string(),
            Ort::Satz { abschnitt, kennung } => format!("{abschnitt} {kennung}"),
            Ort::Schicht { typ, schicht } => format!("Typ {} Schicht {schicht}", typ.to_ifc()),
            Ort::Bauteil(g) => format!("Bauteil {}", g.to_ifc()),
            Ort::Position(oz) => format!("Position {oz}"),
            Ort::Kopf => "Kopf".to_string(),
        };
        format!("Regel {} · {ort}: {}", self.regel, self.satz)
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

/// `abschnitt`: Schlüssel des Abschnitts, im Satz als Wort.
pub fn r72(n: usize, abschnitt: &str, grund: &str) -> String {
    let a = wort::abschnitt(abschnitt);
    format!("Zeile {n} ({a}) wurde übersprungen: {grund}. Sie bleibt unverändert in der Datei.")
}

/// `verweis` mit Artikel und Relativpronomen: „einen Artikel, den“.
pub fn r73(satz: &str, verweis: &str) -> String {
    format!("{satz} verweist auf {verweis} es in dieser Datei nicht gibt.")
}

/// R73-W (Bausteingrenze §6): Werksbaustoff einer älteren Datei.
pub fn r73w(name: &str) -> String {
    format!("Baustoff {name}: Werkspreise über den Namen zugeordnet (ältere Datei).")
}

/// `abschnitt`: Schlüssel, im Satz als Wort; `name` des Eintrags.
pub fn r74(abschnitt: &str, name: &str) -> String {
    let a = wort::abschnitt(abschnitt);
    format!("{a} {name} kommt doppelt vor; es gilt der erste Eintrag.")
}

pub fn r75(name: &str) -> String {
    format!("Werkswert {name} passt nicht zu seinem Eintrag im Werksbestand.")
}

/// `feld`: Schlüssel, im Satz als Wort.
pub fn r76(name: &str, feld: &str, wert: &str) -> String {
    let f = wort::feld(Some("article"), feld);
    format!("Artikel {name}: {f} ist ungültig ({wert}).")
}

pub fn r77(baustoff: &str, dicke: &str, name: &str) -> String {
    format!("Für {baustoff} {dicke} sind mehrere Standardartikel gesetzt; es gilt {name}.")
}

/// `feld`: Schlüssel, im Satz als Wort.
pub fn r79(kurztext: &str, feld: &str, wert: &str) -> String {
    let f = wort::feld(Some("service"), feld);
    format!("Bauleistung {kurztext}: {f} ist ungültig ({wert}).")
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
    format!("{name} liegt im Papierkorb, wird aber noch verwendet.")
}

pub fn r88() -> String {
    "Eine Herkunftsangabe gehört zu keinem Eintrag mehr.".into()
}

pub fn r88_hand(name: &str) -> String {
    format!("Herkunft von {name}: Handeingaben sind immer bestätigt.")
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

/// `vorgang`: Anzeigename (`Op::bezeichnung`), nie der Operationsname.
pub fn r93(vorgang: &str, grund: &str) -> String {
    format!("Änderung „{vorgang}“ abgelehnt: {grund}.")
}

pub fn r99(typ: &str, baustoff: &str) -> String {
    format!("{typ}, Schicht {baustoff}: Die gewählte Bauleistung gibt es nicht (mehr); es gilt die Zuordnung nach Baustoff und Dicke.")
}
