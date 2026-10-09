//! Erweiterungsbauteile (.szb, Bauteilvertrag 0.5) im Modell (Schrittplan
//! E3): die Definition steht je `key` einmal im Projekt, die Exemplare sind
//! Bauteile der Art [`Category::Extension`](crate::Category::Extension)
//! mit Nummern aus dem Präfix der Definition. Körper und Mengen rechnet
//! `sk-szb` aus Definition, Werten des Exemplars und Geschoss.

use sk_szb::formel::{self, Umfeld};
use sk_szb::{Bestand, Def};
pub use sk_szb::{Ergebnis, Geschoss};

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

/// Höchstzahl Zeichen eines Textes aus einer .szb in der Anzeige.
pub const ANZEIGE_MAX: usize = 200;

/// Text aus einer .szb für die Anzeige (Robustheit Nr. 16): ohne Steuer-
/// und Richtungszeichen, Leerraumfolgen als ein Leerzeichen, höchstens
/// `max` Zeichen, gekürzt mit „…“.
pub fn anzeige(text: &str, max: usize) -> String {
    let richtung = |c: char| matches!(c, '\u{061c}' | '\u{200b}'..='\u{200f}' | '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}' | '\u{feff}');
    let woerter: Vec<String> = text
        .split(|c: char| c.is_whitespace() || c.is_control())
        .map(|w| w.chars().filter(|&c| !richtung(c)).collect::<String>())
        .filter(|w| !w.is_empty())
        .collect();
    let s = woerter.join(" ");
    if s.chars().count() <= max {
        return s;
    }
    let mut k: String = s.chars().take(max.saturating_sub(1)).collect();
    k.truncate(k.trim_end().len());
    k.push('…');
    k
}

#[cfg(test)]
mod anzeige_tests {
    #[test]
    fn steuerzeichen_richtung_laenge() {
        use super::anzeige;
        assert_eq!(anzeige("Stütze\u{202e} 24/24", 50), "Stütze 24/24");
        assert_eq!(anzeige("a\n\tb\u{0007}c  d", 50), "a b c d");
        assert_eq!(anzeige("abcdef ghij", 8), "abcdef…");
        assert_eq!(anzeige("abcdefgh", 8), "abcdefgh");
        assert_eq!(anzeige("\u{200f}", 8), "");
    }
}

impl ExtDef {
    /// Liest eine Definition aus dem Text einer .szb, ohne Grenzprüfung
    /// (sie läuft nur beim Einlesen). `Err`: der erste Fehler mit Zeile.
    pub fn lesen(text: &str) -> Result<ExtDef, String> {
        ExtDef::mit(text, false)
    }

    /// Wie [`ExtDef::lesen`], mit Grenzprüfung: zum Einlesen einer .szb.
    pub fn einlesen(text: &str) -> Result<ExtDef, String> {
        ExtDef::mit(text, true)
    }

    fn mit(text: &str, grenzen: bool) -> Result<ExtDef, String> {
        let text = normal(text);
        let (best, g) = (Bestand::werk(), Geschoss::PROBE);
        let p = if grenzen {
            sk_szb::pruefen(&text, &best, &g)
        } else {
            sk_szb::pruefen_beim_oeffnen(&text, &best, &g)
        };
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

    /// Gruppe aus `[bedienung]` als Anzeigename („Tragwerk“), sonst
    /// „Sonstiges“.
    pub fn gruppe(&self) -> &'static str {
        sk_szb::pruefen::GRUPPEN[self.gruppe_rang()].1
    }

    /// Stelle der Gruppe in der Reihenfolge des Bauteilkatalogs.
    pub fn gruppe_rang(&self) -> usize {
        let k = self.def.bedienung_feld("gruppe").unwrap_or("");
        let g = &sk_szb::pruefen::GRUPPEN;
        g.iter().position(|g| g.0 == k).unwrap_or(g.len() - 1)
    }

    /// Feld aus `[bedienung]`, z. B. `laenge` → `l`.
    pub fn bedienung(&self, k: &str) -> Option<&str> {
        self.def.bedienung_feld(k).filter(|v| !v.is_empty())
    }

    /// Darf das Werkzeug drehen (`drehen=ja`)?
    pub fn drehen(&self) -> bool {
        self.bedienung("drehen") == Some("ja")
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

/// Körper und Mengen des Exemplars `part` von `d` im Geschoss `g`.
pub fn rechnen(d: &ExtDef, part: &ExtPart, g: &Geschoss) -> Ergebnis {
    sk_szb::rechnen(&d.def, &d.werte(part, g), g)
}

/// Ein `[param]` im Paneel (Vertrag §14): Wert des Exemplars, Grenzen und
/// Bedienangaben.
#[derive(Clone, Debug, PartialEq)]
pub struct ExtFeld {
    pub key: String,
    pub name: String,
    /// `mm`, `stk`, `grad`, … oder leer.
    pub einheit: String,
    pub wert: f64,
    pub min: Option<f64>,
    pub max: Option<f64>,
    pub ganz: bool,
    /// Im Werkzeug-Paneel (`zeichnen=ja`).
    pub zeichnen: bool,
    /// `sichtbar` ergibt nicht 0 (ohne Angabe sichtbar).
    pub sichtbar: bool,
    pub gruppe: String,
    pub hilfe: String,
    /// `wahl="0:a|1:b"`: Wert und Text.
    pub wahl: Vec<(f64, String)>,
    pub janein: bool,
}

impl ExtFeld {
    /// Der Wert liegt außerhalb von min/max.
    pub fn ausserhalb(&self) -> bool {
        self.min.is_some_and(|m| self.wert < m - 1e-9)
            || self.max.is_some_and(|m| self.wert > m + 1e-9)
    }
}

impl ExtDef {
    /// Alle Parameter mit den Werten des Exemplars `part` im Geschoss `g`,
    /// in Satzreihenfolge.
    pub fn felder(&self, part: &ExtPart, g: &Geschoss) -> Vec<ExtFeld> {
        let pv = self.werte(part, g);
        let mut u = g.umfeld();
        u.extend(pv.iter().map(|(k, v)| (k.clone(), *v)));
        let f = |s: Option<&str>| s.and_then(|s| formel::rechnen(s, &u, None).ok());
        self.def
            .param
            .iter()
            .map(|r| ExtFeld {
                key: r.key().to_string(),
                name: r.get("name").unwrap_or(r.key()).to_string(),
                einheit: r.get("einheit").unwrap_or("").to_string(),
                wert: pv.get(r.key()).copied().unwrap_or(0.0),
                min: f(r.get("min")),
                max: f(r.get("max")),
                ganz: r.ja("ganz"),
                zeichnen: r.ja("zeichnen"),
                sichtbar: f(r.get("sichtbar")).is_none_or(|v| v != 0.0),
                gruppe: r.get("gruppe").unwrap_or("").to_string(),
                hilfe: r.get("hilfe").unwrap_or("").to_string(),
                wahl: r
                    .get("wahl")
                    .map(|w| {
                        w.split('|')
                            .filter_map(|x| {
                                let (a, b) = x.split_once(':')?;
                                Some((a.trim().parse().ok()?, b.trim().to_string()))
                            })
                            .collect()
                    })
                    .unwrap_or_default(),
                janein: r.ja("janein"),
            })
            .collect()
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
