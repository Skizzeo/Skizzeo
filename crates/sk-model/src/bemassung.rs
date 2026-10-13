//! Bemaßung (Jörn 12.10. 14:20–14:37, skizzeo/bemassung/konzept.md): der
//! Stil der Maßketten, die Versätze von Hand verschobener Ketten und je
//! Ansicht die Wahl am Zahnrad.
//!
//! Die Ketten selbst werden nie gespeichert, sondern aus dem Modell gerechnet
//! (`sk-plan`). Papiermaße (Strich, Schrift, Symbol) in mm auf dem Blatt,
//! Abstände der Ketten in mm am Bau. Der Werksstil folgt der Recherche zu
//! DIN 1356-1 bzw. DIN EN ISO 7519 im Maßstab 1:100 und Jörns Bürostandard
//! (Kreis, Segoe UI, erste Kette 2,00 m vor der Wand, dann je 1,00 m).

use crate::guid::Guid;
use std::collections::BTreeMap;

/// Maßlinienbegrenzung.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Symbol {
    /// Offener Kreis (Jörns Wahl).
    Kreis,
    /// Schrägstrich unter 45°.
    Schraeg,
    /// Gefüllter Punkt.
    Punkt,
    /// Gefüllter Pfeil.
    Pfeil,
}

impl Symbol {
    pub const ALL: [Symbol; 4] = [Symbol::Kreis, Symbol::Schraeg, Symbol::Punkt, Symbol::Pfeil];

    /// Wort in der Datei.
    pub fn word(self) -> &'static str {
        match self {
            Symbol::Kreis => "kreis",
            Symbol::Schraeg => "schraeg",
            Symbol::Punkt => "punkt",
            Symbol::Pfeil => "pfeil",
        }
    }

    /// Name für Menschen.
    pub fn name(self) -> &'static str {
        match self {
            Symbol::Kreis => "Kreis",
            Symbol::Schraeg => "Schrägstrich",
            Symbol::Punkt => "Punkt",
            Symbol::Pfeil => "Pfeil",
        }
    }
}

/// Welche Flächen der Wände die Außenmaßketten messen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Aussenbezug {
    /// Nur die tragende Schicht (Rohbaumaß ohne Dämmung, Vorsatzschale und
    /// Putz).
    Rohbau,
    /// Das ganze Wandpaket von der Außen- bis zur Innenkante (Werk, Jörn
    /// 13.10.).
    Wand,
}

impl Aussenbezug {
    pub const ALL: [Aussenbezug; 2] = [Aussenbezug::Rohbau, Aussenbezug::Wand];

    pub fn word(self) -> &'static str {
        match self {
            Aussenbezug::Rohbau => "rohbau",
            Aussenbezug::Wand => "wand",
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Aussenbezug::Rohbau => "Tragende Schicht",
            Aussenbezug::Wand => "Ganzes Wandpaket",
        }
    }
}

/// Bezug der Brüstungshöhe (Jörn 14:31).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BrhBezug {
    /// OK Rohdecke, im EG OK Sohlplatte.
    Rohdecke,
    /// OK Estrich (nicht OK Fertigfußboden).
    Estrich,
}

impl BrhBezug {
    pub const ALL: [BrhBezug; 2] = [BrhBezug::Rohdecke, BrhBezug::Estrich];

    pub fn word(self) -> &'static str {
        match self {
            BrhBezug::Rohdecke => "rohdecke",
            BrhBezug::Estrich => "estrich",
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            BrhBezug::Rohdecke => "OK Rohdecke / Sohlplatte",
            BrhBezug::Estrich => "OK Estrich",
        }
    }
}

/// Die Außenmaßketten von der Wand nach außen (Jörn 14:31–14:33).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Kettenart {
    /// Öffnungen, ihre Abstände untereinander und zu den Außenecken.
    Oeffnung,
    /// Angeschlossene Außen- und Innenwände und die Abstände dazwischen.
    Wand,
    /// Vor- und Rücksprünge der Außenwand (L-, U-, T-Grundriss).
    Sprung,
    /// Gesamtmaß von Außenecke zu Außenecke.
    Gesamt,
}

impl Kettenart {
    pub const ALL: [Kettenart; 4] = [
        Kettenart::Oeffnung,
        Kettenart::Wand,
        Kettenart::Sprung,
        Kettenart::Gesamt,
    ];

    pub fn word(self) -> &'static str {
        match self {
            Kettenart::Oeffnung => "oeffnung",
            Kettenart::Wand => "wand",
            Kettenart::Sprung => "sprung",
            Kettenart::Gesamt => "gesamt",
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Kettenart::Oeffnung => "Öffnungskette",
            Kettenart::Wand => "Wandkette",
            Kettenart::Sprung => "Vor- und Rücksprünge",
            Kettenart::Gesamt => "Gesamtmaß",
        }
    }

    fn index(self) -> usize {
        self as usize
    }
}

/// Zahlenwerte des Stils, je mit Schlüssel in der Datei, Bezeichnung,
/// Bereich, Nachkommastellen und Einheit (für Datei und Einstellungsfenster).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Zahl {
    Linie,
    Hilfslinie,
    Ueberstand,
    HilfsUeber,
    SymbolGroesse,
    SymbolStrich,
    SchriftHoehe,
    Hochzahl,
    TextAbstand,
    BrhAbstand,
    Erste,
    Abstand,
    Massstab,
}

impl Zahl {
    pub const ALL: [Zahl; 13] = [
        Zahl::Linie,
        Zahl::Hilfslinie,
        Zahl::Ueberstand,
        Zahl::HilfsUeber,
        Zahl::SymbolGroesse,
        Zahl::SymbolStrich,
        Zahl::SchriftHoehe,
        Zahl::Hochzahl,
        Zahl::TextAbstand,
        Zahl::BrhAbstand,
        Zahl::Erste,
        Zahl::Abstand,
        Zahl::Massstab,
    ];

    /// Schlüssel in der Datei.
    pub fn key(self) -> &'static str {
        match self {
            Zahl::Linie => "linie",
            Zahl::Hilfslinie => "hilfslinie",
            Zahl::Ueberstand => "ueberstand",
            Zahl::HilfsUeber => "hilfsueber",
            Zahl::SymbolGroesse => "symbol_mm",
            Zahl::SymbolStrich => "symbol_strich",
            Zahl::SchriftHoehe => "schrift_mm",
            Zahl::Hochzahl => "hochzahl",
            Zahl::TextAbstand => "text_abstand",
            Zahl::BrhAbstand => "brh_abstand",
            Zahl::Erste => "erste",
            Zahl::Abstand => "abstand",
            Zahl::Massstab => "massstab",
        }
    }

    /// Bezeichnung im Einstellungsfenster.
    pub fn label(self) -> &'static str {
        match self {
            Zahl::Linie => "Maßlinie",
            Zahl::Hilfslinie => "Maßhilfslinie",
            Zahl::Ueberstand => "Überstand der Maßlinie",
            Zahl::HilfsUeber => "Hilfslinie über die Maßlinie",
            Zahl::SymbolGroesse => "Größe der Begrenzung",
            Zahl::SymbolStrich => "Strich der Begrenzung",
            Zahl::SchriftHoehe => "Schrifthöhe",
            Zahl::Hochzahl => "Hochzahl",
            Zahl::TextAbstand => "Abstand zur Maßlinie",
            Zahl::BrhAbstand => "BRH vor der Wand",
            Zahl::Erste => "Erste Kette vor der Wand",
            Zahl::Abstand => "Abstand der Ketten",
            Zahl::Massstab => "Bezugsmaßstab 1:",
        }
    }

    /// Kleinster und größter Wert, Nachkommastellen, Einheit.
    pub fn range(self) -> (f64, f64, usize, &'static str) {
        match self {
            Zahl::Linie | Zahl::Hilfslinie | Zahl::SymbolStrich => (0.05, 2.0, 2, "mm"),
            Zahl::Ueberstand | Zahl::HilfsUeber => (0.0, 10.0, 1, "mm"),
            Zahl::SymbolGroesse => (0.3, 10.0, 1, "mm"),
            Zahl::SchriftHoehe => (1.0, 10.0, 1, "mm"),
            Zahl::Hochzahl => (30.0, 100.0, 0, "%"),
            Zahl::TextAbstand | Zahl::BrhAbstand => (0.0, 10.0, 1, "mm"),
            Zahl::Erste | Zahl::Abstand => (100.0, 20_000.0, 0, "mm"),
            Zahl::Massstab => (1.0, 1000.0, 0, ""),
        }
    }
}

/// Wie eine Kette aussieht und wo sie liegt.
#[derive(Clone, Debug, PartialEq)]
pub struct Stil {
    /// Farbe von Linien, Begrenzung und Schrift.
    pub farbe: [u8; 3],
    /// Strichstärke der Maßlinie (mm auf dem Papier).
    pub linie: f64,
    /// Strichstärke der Maßhilfslinie.
    pub hilfslinie: f64,
    /// So weit läuft die Maßlinie über die äußeren Hilfslinien hinaus.
    pub ueberstand: f64,
    /// So weit ragt die Hilfslinie zu beiden Seiten über die Maßlinie.
    pub hilfs_ueber: f64,
    pub symbol: Symbol,
    /// Durchmesser von Kreis und Punkt, Länge von Schrägstrich und Pfeil.
    pub symbol_mm: f64,
    /// Strichstärke von Kreis und Schrägstrich.
    pub symbol_strich: f64,
    /// Name der Schrift (Jörn: Segoe UI).
    pub schrift: String,
    /// Höhe der Ziffern (mm auf dem Papier).
    pub schrift_mm: f64,
    /// Größe der Hochzahl in Prozent der Ziffernhöhe.
    pub hochzahl: f64,
    /// Abstand der Maßzahl von der Maßlinie.
    pub text_abstand: f64,
    /// Abstand der Brüstungshöhe vor der Wand.
    pub brh_abstand: f64,
    /// Rundung der Maßzahlen in mm (1 oder 5).
    pub rundung: u8,
    /// Abstand der ersten Kette von der äußersten Außenwand (mm am Bau).
    pub erste: f64,
    /// Abstand der Ketten untereinander (mm am Bau).
    pub abstand: f64,
    pub bezug: Aussenbezug,
    /// Je Kettenart ein oder aus ([`Kettenart::ALL`]).
    pub ketten: [bool; 4],
    /// Öffnungshöhe unter der Breite.
    pub hoehe: bool,
    /// Brüstungshöhe an der Wand.
    pub brh: bool,
    pub brh_bezug: BrhBezug,
    /// Papier-mm × Maßstab = mm am Bau (1:100).
    pub massstab: f64,
}

impl Default for Stil {
    fn default() -> Stil {
        Stil::werk()
    }
}

impl Stil {
    /// Werksstil (Konzept §6).
    pub fn werk() -> Stil {
        Stil {
            farbe: [0, 0, 0],
            linie: 0.18,
            hilfslinie: 0.18,
            ueberstand: 1.5,
            hilfs_ueber: 2.0,
            symbol: Symbol::Kreis,
            symbol_mm: 1.5,
            symbol_strich: 0.25,
            schrift: "Segoe UI".into(),
            schrift_mm: 2.5,
            hochzahl: 65.0,
            text_abstand: 0.8,
            brh_abstand: 1.5,
            rundung: 1,
            erste: 2000.0,
            abstand: 1000.0,
            bezug: Aussenbezug::Wand,
            ketten: [true; 4],
            hoehe: true,
            brh: true,
            brh_bezug: BrhBezug::Rohdecke,
            massstab: 100.0,
        }
    }

    pub fn zahl(&self, z: Zahl) -> f64 {
        match z {
            Zahl::Linie => self.linie,
            Zahl::Hilfslinie => self.hilfslinie,
            Zahl::Ueberstand => self.ueberstand,
            Zahl::HilfsUeber => self.hilfs_ueber,
            Zahl::SymbolGroesse => self.symbol_mm,
            Zahl::SymbolStrich => self.symbol_strich,
            Zahl::SchriftHoehe => self.schrift_mm,
            Zahl::Hochzahl => self.hochzahl,
            Zahl::TextAbstand => self.text_abstand,
            Zahl::BrhAbstand => self.brh_abstand,
            Zahl::Erste => self.erste,
            Zahl::Abstand => self.abstand,
            Zahl::Massstab => self.massstab,
        }
    }

    /// Setzt einen Zahlenwert; `false` außerhalb des Bereichs.
    pub fn set_zahl(&mut self, z: Zahl, v: f64) -> bool {
        let (lo, hi, ..) = z.range();
        if !v.is_finite() || v < lo || v > hi {
            return false;
        }
        let f = match z {
            Zahl::Linie => &mut self.linie,
            Zahl::Hilfslinie => &mut self.hilfslinie,
            Zahl::Ueberstand => &mut self.ueberstand,
            Zahl::HilfsUeber => &mut self.hilfs_ueber,
            Zahl::SymbolGroesse => &mut self.symbol_mm,
            Zahl::SymbolStrich => &mut self.symbol_strich,
            Zahl::SchriftHoehe => &mut self.schrift_mm,
            Zahl::Hochzahl => &mut self.hochzahl,
            Zahl::TextAbstand => &mut self.text_abstand,
            Zahl::BrhAbstand => &mut self.brh_abstand,
            Zahl::Erste => &mut self.erste,
            Zahl::Abstand => &mut self.abstand,
            Zahl::Massstab => &mut self.massstab,
        };
        *f = v;
        true
    }

    /// Ist die Kettenart eingeschaltet?
    pub fn kette(&self, k: Kettenart) -> bool {
        self.ketten[k.index()]
    }

    pub fn set_kette(&mut self, k: Kettenart, an: bool) {
        self.ketten[k.index()] = an;
    }

    /// Alle Werte in ihren Bereichen, Schrift nicht leer, Rundung 1 oder 5.
    pub fn gueltig(&self) -> bool {
        Zahl::ALL.iter().all(|z| {
            let (lo, hi, ..) = z.range();
            let v = self.zahl(*z);
            v.is_finite() && v >= lo && v <= hi
        }) && !self.schrift.trim().is_empty()
            && !self.schrift.contains(['"', '\n', '\\'])
            && matches!(self.rundung, 1 | 5)
    }

    /// Papiermaß in mm am Bau.
    pub fn am_bau(&self, papier_mm: f64) -> f64 {
        papier_mm * self.massstab
    }
}

/// Schlüssel einer verschobenen Kette: Geschoss, Seite (Richtung der
/// Außennormalen in ganzen Grad, 0 = +x, gegen den Uhrzeigersinn) und Art.
pub type VersatzKey = (Guid, i32, Kettenart);

/// Stil und Versätze, als Ganzes ein Rückgängig-Eintrag.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Bemassung {
    pub stil: Stil,
    /// mm am Bau nach außen (+) bzw. zur Wand (−), nur ungleich null.
    pub versatz: BTreeMap<VersatzKey, f64>,
}

/// Seite einer Kette als ganzer Grad in 0..360.
pub fn seite_grad(nx: f64, ny: f64) -> i32 {
    (ny.atan2(nx).to_degrees().round() as i32).rem_euclid(360)
}

/// Ansichten für die Wahl am Zahnrad, wie [`crate::view::SICHT_VIEWS`]:
/// 3D, Grundriss, Schnitt, Vorne, Hinten, Links, Rechts.
pub const ANSICHTEN: usize = 7;

/// Wahl am Zahnrad je Ansicht: Bemaßung zeigen, Maßbezugspunkte zeigen.
/// Ansichtszustand ohne Rückgängig-Schritt wie `[viewhide]`.
#[derive(Clone, Debug, PartialEq)]
pub struct MassSicht {
    pub zeigen: [bool; ANSICHTEN],
    pub bezug: [bool; ANSICHTEN],
    /// `[massansicht]`-Zeilen, die nicht zählen, roh.
    pub raw: Vec<String>,
}

impl Default for MassSicht {
    fn default() -> MassSicht {
        MassSicht {
            zeigen: MassSicht::VORGABE,
            bezug: [false; ANSICHTEN],
            raw: Vec::new(),
        }
    }
}

impl MassSicht {
    /// Bemaßung zeigen: in 3D und im Grundriss (Stufe 1); Schnitt und
    /// Ansichten bekommen ihre Bemaßung später.
    pub const VORGABE: [bool; ANSICHTEN] = [true, true, false, false, false, false, false];
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn werksstil_ist_gueltig_und_folgt_dem_konzept() {
        let s = Stil::werk();
        assert!(s.gueltig());
        assert_eq!(s.symbol, Symbol::Kreis);
        assert_eq!(s.schrift, "Segoe UI");
        assert_eq!((s.erste, s.abstand), (2000.0, 1000.0));
        assert_eq!(s.am_bau(s.schrift_mm), 250.0);
        for z in Zahl::ALL {
            let (lo, hi, ..) = z.range();
            let v = s.zahl(z);
            assert!(v >= lo && v <= hi, "{z:?}");
            let mut t = s.clone();
            assert!(!t.set_zahl(z, hi + 1.0));
            assert!(t.set_zahl(z, lo));
            assert_eq!(t.zahl(z), lo);
        }
    }

    #[test]
    fn seite_in_ganzen_grad() {
        assert_eq!(seite_grad(0.0, -1.0), 270);
        assert_eq!(seite_grad(1.0, 0.0), 0);
        assert_eq!(seite_grad(-1.0, 1e-9), 180);
        assert_eq!(seite_grad(0.0, 1.0), 90);
    }
}
