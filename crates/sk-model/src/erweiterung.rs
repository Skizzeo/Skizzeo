//! Erweiterungsbauteile (.szb, Bauteilvertrag 0.5) im Modell (Schrittplan
//! E3): die Definition steht je `key` einmal im Projekt, die Exemplare sind
//! Bauteile der Art [`Category::Extension`](crate::Category::Extension)
//! mit Nummern aus dem Präfix der Definition. Körper und Mengen rechnet
//! `sk-szb` aus Definition, Werten des Exemplars und Geschoss.

use sk_szb::formel::{self, Umfeld};
use sk_szb::{Bestand, Def, Geschoss};

/// Eine Definition im Projekt. Die Datei trägt sie vollständig mit, damit
/// das Projekt auch ohne eingelesene Erweiterung öffnet.
#[derive(Clone, Debug, PartialEq)]
pub struct ExtDef {
    pub key: String,
    pub version: u32,
    /// Text der .szb: Zeilenenden `\n`, ohne Byte-Order-Mark.
    pub text: String,
    pub def: Def,
}

/// Zeilenenden `\n`, ohne Byte-Order-Mark.
pub fn normal(text: &str) -> String {
    text.trim_start_matches('\u{feff}').replace("\r\n", "\n")
}

impl ExtDef {
    /// Liest eine Definition aus dem Text einer .szb, ohne Grenzprüfung
    /// (sie läuft nur beim Einlesen). `Err`: der erste Fehler mit Zeile.
    pub fn lesen(text: &str) -> Result<ExtDef, String> {
        let text = normal(text);
        let p = sk_szb::pruefen_beim_oeffnen(&text, &Bestand::werk(), &Geschoss::PROBE);
        if let Some(b) = p.befunde.iter().find(|b| b.ist_fehler()) {
            return Err(if b.zeile > 0 {
                format!("Zeile {}: {}", b.zeile, b.text)
            } else {
                b.text.clone()
            });
        }
        let key = p.def.bauteil_feld("key").unwrap_or("").to_string();
        let version = p
            .def
            .bauteil_feld("version")
            .and_then(|v| v.parse::<u32>().ok())
            .unwrap_or(1);
        Ok(ExtDef {
            key,
            version,
            text,
            def: p.def,
        })
    }

    fn feld(&self, k: &str) -> &str {
        self.def.bauteil_feld(k).unwrap_or("")
    }

    /// „Stahlbetonstütze“.
    pub fn name(&self) -> &str {
        self.feld("name")
    }

    /// „Stahlbetonstützen“.
    pub fn plural(&self) -> &str {
        self.feld("mehrzahl")
    }

    /// Präfix der Bauteilnummer, z. B. „ST“.
    pub fn prefix(&self) -> &str {
        self.feld("praefix")
    }

    /// IFC-Klasse; ohne Angabe ein Stellvertreter.
    pub fn ifc(&self) -> &str {
        match self.feld("ifc") {
            "" => "IfcBuildingElementProxy",
            s => s,
        }
    }

    /// Kostengruppe nach DIN 276.
    pub fn kg(&self) -> Option<u16> {
        self.feld("kg").parse().ok()
    }

    /// `einfuegen` aus `[bedienung]`: punkt, linie oder rechteck.
    pub fn einfuegen(&self) -> &str {
        self.def.bedienung_feld("einfuegen").unwrap_or("punkt")
    }

    /// Typ `key`.
    pub fn typ(&self, key: &str) -> Option<&sk_szb::Satz> {
        self.def.typ.iter().find(|t| t.key() == key)
    }

    /// Der vorgewählte Typ (`standard=ja`), sonst keiner.
    pub fn standard_typ(&self) -> Option<&str> {
        self.def
            .typ
            .iter()
            .find(|t| t.ja("standard"))
            .map(|t| t.key())
    }

    /// Werte der Parameter eines Exemplars in Satzreihenfolge: eigener Wert
    /// des Exemplars, sonst der des Typs, sonst die Vorgabe (Formel aus den
    /// früheren Parametern). Eine fehlerhafte Vorgabe gilt als 0 wie in
    /// [`sk_szb::rechnen::vorgaben`].
    pub fn werte(&self, part: &ExtPart, g: &Geschoss) -> Umfeld {
        let typ: Vec<(String, f64)> = part
            .typ
            .as_deref()
            .and_then(|k| self.typ(k))
            .and_then(|t| sk_szb::pruefen::typ_werte(t.get("werte").unwrap_or("")).ok())
            .unwrap_or_default();
        let mut pv = Umfeld::new();
        for r in &self.def.param {
            let k = r.key();
            let eigen = part.werte.iter().find(|(n, _)| n == k).map(|(_, v)| *v);
            let v = eigen
                .or_else(|| typ.iter().find(|(n, _)| n == k).map(|(_, v)| *v))
                .unwrap_or_else(|| {
                    let mut u = g.umfeld();
                    u.extend(pv.iter().map(|(k, v)| (k.clone(), *v)));
                    r.get("wert")
                        .and_then(|w| formel::rechnen(w, &u, None).ok())
                        .unwrap_or(0.0)
                });
            pv.insert(k.to_string(), v);
        }
        pv
    }
}

/// Ein Exemplar: welche Definition, wo, wie gedreht, welcher Typ und welche
/// Werte der Nutzer selbst gesetzt hat. Alles andere folgt der Definition,
/// so dass eine neue Version auch gesetzte Exemplare ändert (mit Rückfrage
/// beim Aktualisieren, E5).
#[derive(Clone, Debug, PartialEq)]
pub struct ExtPart {
    pub key: String,
    /// Einfügepunkt im Grundriss (mm); die Höhe folgt aus `[hoehe]` und dem
    /// Geschoss.
    pub at: [f64; 2],
    /// Drehung um den Einfügepunkt in Grad, gegen den Uhrzeigersinn.
    pub rot: f64,
    /// Gewählter Typ.
    pub typ: Option<String>,
    /// Eigene Werte der Parameter in der Reihenfolge, in der sie gesetzt
    /// wurden.
    pub werte: Vec<(String, f64)>,
}

impl ExtPart {
    /// Neues Exemplar mit dem Standardtyp der Definition.
    pub fn new(def: &ExtDef, at: [f64; 2]) -> ExtPart {
        ExtPart {
            key: def.key.clone(),
            at,
            rot: 0.0,
            typ: def.standard_typ().map(str::to_string),
            werte: Vec::new(),
        }
    }

    /// Setzt einen eigenen Wert.
    pub fn set(&mut self, key: &str, v: f64) {
        match self.werte.iter_mut().find(|(n, _)| n == key) {
            Some(x) => x.1 = v,
            None => self.werte.push((key.to_string(), v)),
        }
    }

    /// Eigene Werte als Text „b=300; t=300“ (Datei).
    pub fn werte_text(&self) -> String {
        self.werte
            .iter()
            .map(|(k, v)| format!("{k}={v}"))
            .collect::<Vec<_>>()
            .join("; ")
    }
}
