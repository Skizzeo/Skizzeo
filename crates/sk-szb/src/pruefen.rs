//! Prüfung wie die Werkbank (Vertrag §11): Fehler verhindern das
//! Einlesen, Hinweise nicht; dazu die Liste „Vollständigkeit“ für Mengen,
//! LV und Kosten. Reihenfolge und Wortlaut der Befunde folgen `check`,
//! `checkBim`, `checkBedienung`, `runAll` und `vollst` der Werkbank 0.5.

use std::collections::{BTreeMap, BTreeSet};

use crate::bestand::Bestand;
use crate::formel::{self, Formel, Umfeld};
use crate::lesen::{self, Def, Satz};
use crate::rechnen::{self, punkte, Ergebnis, Geschoss};
use crate::{zahl, Befund};

/// Grenzen eines Parameters, aus `min`, `max` und `wert` gerechnet.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Grenzen {
    pub min: Option<f64>,
    pub max: Option<f64>,
    pub wert: Option<f64>,
}

/// Ergebnis der Prüfung eines Bauteiltexts.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Pruefung {
    pub def: Def,
    pub befunde: Vec<Befund>,
    pub vollstaendigkeit: Vec<String>,
    pub grenzen: BTreeMap<String, Grenzen>,
    /// Rechnung mit den Vorgaben im Prüfgeschoss.
    pub ergebnis: Ergebnis,
}

impl Pruefung {
    pub fn fehler(&self) -> usize {
        self.befunde.iter().filter(|b| b.ist_fehler()).count()
    }

    pub fn hinweise(&self) -> usize {
        self.befunde.len() - self.fehler()
    }

    /// Ohne Fehler: einlesbar.
    pub fn einlesbar(&self) -> bool {
        self.fehler() == 0
    }
}

const SICHERHEIT: [&str; 3] = ["belegt", "mittel", "grob"];
const FUNKTION: [&str; 4] = ["loadbearing", "insulation", "finish", "membrane"];
const BEZUG: [&str; 6] = ["area", "volume", "length", "perimeter", "formwork", "steel"];
const KATEGORIE: [&str; 6] = [
    "masonry",
    "concrete",
    "insulation",
    "plaster",
    "timber",
    "metal",
];
const EINH_P: [&str; 4] = ["mm", "grad", "stk", "-"];
const EINH_M: [&str; 6] = ["stk", "m", "m2", "m3", "kg", "t"];
const RESERVIERT: [&str; 20] = [
    "GH", "DECKE", "LICHT", "i", "n", "pi", "min", "max", "abs", "wurzel", "sin", "cos", "tan",
    "atan", "atan2", "rund", "ab", "auf", "wenn", "volumen",
];

/// Ordner im Bauteilkatalog (`[bedienung] gruppe`).
pub const GRUPPEN: [(&str, &str); 7] = [
    ("tragwerk", "Tragwerk"),
    ("treppen_gelaender", "Treppen und Geländer"),
    ("dach", "Dach"),
    ("fassade", "Fassade"),
    ("ausbau", "Ausbau"),
    ("aussenanlagen", "Außenanlagen"),
    ("sonstiges", "Sonstiges"),
];
const EINFUEGEN: [&str; 3] = ["punkt", "linie", "rechteck"];
const UNWORT: [(&str, &str); 9] = [
    ("weite", "Breite"),
    ("stärke", "Dicke"),
    ("staerke", "Dicke"),
    ("hoehe", "Höhe"),
    ("laenge", "Länge"),
    ("anz", "Anzahl"),
    ("dm", "Durchmesser"),
    ("durchm", "Durchmesser"),
    ("abst", "Abstand"),
];
const MAX_NAME: usize = 20;
const MAX_GRUPPE: usize = 20;
const MAX_TYP: usize = 30;
const MAX_HILFE: usize = 120;
const MAX_ZEICHNEN: usize = 4;
/// Höchstens so viele sichtbare Felder im Paneel (Hinweis im Paneel, §14).
pub const MAX_SICHTBAR: usize = 12;

/// Pflicht- und Wahlfelder je Abschnitt; Reihenfolge wie in der Werkbank.
const SPEC: [(&str, &[&str], &[&str]); 12] = [
    (
        "bauteil",
        &["key", "name", "mehrzahl", "praefix"],
        &[
            "genus",
            "ifc",
            "kg",
            "gewerk",
            "version",
            "autor",
            "beschreibung",
            "art",
            "aussen",
        ],
    ),
    (
        "bedienung",
        &["gruppe", "einfuegen"],
        &["laenge", "breite", "tiefe", "drehen"],
    ),
    ("typ", &["key", "name", "werte"], &["standard"]),
    ("hoehe", &[], &["ab", "versatz"]),
    (
        "param",
        &["key", "name", "wert"],
        &[
            "einheit", "min", "max", "ganz", "gruppe", "wahl", "janein", "zeichnen", "sichtbar",
            "hilfe",
        ],
    ),
    ("wert", &["key", "formel"], &["name", "einheit", "anzeigen"]),
    (
        "baustoff",
        &["key", "name", "kategorie"],
        &[
            "rohdichte",
            "farbe",
            "gewerk",
            "lambda",
            "mu",
            "c",
            "euroklasse",
            "hersteller",
            "produkt",
            "untergruppe",
            "quelle",
            "sicherheit",
            "stand",
        ],
    ),
    (
        "koerper",
        &["form", "baustoff"],
        &[
            "teil", "funktion", "anzahl", "wenn", "drehung", "x", "y", "z", "b", "t", "h", "ebene",
            "punkte", "von", "bis", "r", "achse", "seiten",
        ],
    ),
    (
        "artikel",
        &["key", "name", "einheit"],
        &[
            "baustoff",
            "preis",
            "stand",
            "quelle",
            "sicherheit",
            "dicke",
            "guete",
            "format",
            "lieferant",
            "stueck",
        ],
    ),
    (
        "leistung",
        &["key", "kurztext", "einheit", "bezug"],
        &[
            "langtext",
            "gewerk",
            "stunden",
            "geraet",
            "sonstiges",
            "kg",
            "stoffe",
            "funktion",
            "quelle",
            "sicherheit",
            "stand",
            "dmin",
            "dmax",
        ],
    ),
    (
        "menge",
        &["key", "name", "einheit", "formel"],
        &[
            "baustoff", "kg", "gewerk", "leistung", "bezug", "anzeigen", "dicke",
        ],
    ),
    ("notiz", &["text"], &["art"]),
];

fn form_felder(form: &str) -> Option<&'static [&'static str]> {
    Some(match form {
        "quader" => &["b", "t", "h"],
        "prisma" => &["ebene", "punkte", "von", "bis"],
        "zylinder" => &["r", "h"],
        _ => return None,
    })
}

/// `^[a-z][a-z0-9_]*$`
pub fn ist_key(s: &str) -> bool {
    let mut c = s.chars();
    matches!(c.next(), Some('a'..='z'))
        && c.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

/// Längster `key`; er ist zugleich der Dateiname `<key>.szb`.
pub const MAX_KEY: usize = 64;

/// `key` taugt als Dateiname: nicht zu lang und kein Gerätename unter
/// Windows (`con.stuetze.szb` wäre dort das Gerät CON).
pub fn key_als_datei(key: &str) -> Result<(), String> {
    if key.len() > MAX_KEY {
        return Err(format!("höchstens {MAX_KEY} Zeichen"));
    }
    let vorn = key.split('.').next().unwrap_or("");
    let geraet = matches!(vorn, "con" | "prn" | "aux" | "nul")
        || ["com", "lpt"].iter().any(|g| {
            vorn.strip_prefix(g)
                .is_some_and(|z| z.len() == 1 && z.as_bytes()[0].is_ascii_digit() && z != "0")
        });
    if geraet {
        return Err(format!("„{vorn}“ ist unter Windows ein Gerätename"));
    }
    Ok(())
}

/// `^-?[0-9]*\.?[0-9]+$`
fn einfache_zahl(v: &str) -> Option<f64> {
    let d = v.strip_prefix('-').unwrap_or(v);
    let (a, b) = match d.split_once('.') {
        Some((a, b)) => (a, b),
        None => ("", d),
    };
    let ok = !b.is_empty()
        && a.chars().all(|c| c.is_ascii_digit())
        && b.chars().all(|c| c.is_ascii_digit());
    if !ok {
        return None;
    }
    let t = if a.is_empty() && d.contains('.') {
        format!("{}0{}", if v.starts_with('-') { "-" } else { "" }, d)
    } else {
        v.to_string()
    };
    t.parse().ok()
}

/// Zeichenzahl wie JavaScript `length` für deutsche Texte.
fn laenge(s: &str) -> usize {
    s.chars().count()
}

/// `^(0[1-9]|1[0-2])/20\d\d$`
fn ist_monat(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() == 7
        && b[2] == b'/'
        && matches!((b[0], b[1]), (b'0', b'1'..=b'9') | (b'1', b'0'..=b'2'))
        && b[3] == b'2'
        && b[4] == b'0'
        && b[5].is_ascii_digit()
        && b[6].is_ascii_digit()
}

/// Euroklasse nach DIN EN 13501-1 in der Form des Vertrags.
fn ist_euroklasse(s: &str) -> bool {
    for k in ["A1", "E", "F"] {
        if s == k || s == format!("{k}fl") {
            return true;
        }
    }
    for k in ["A2", "B", "C", "D"] {
        let Some(r) = s.strip_prefix(k) else {
            continue;
        };
        if r.is_empty() || r == "fl" || r == "fl-s1" || r == "fl-s2" {
            return true;
        }
        if let Some(r) = r.strip_prefix("-s") {
            for s in ["1", "2", "3"] {
                if let Some(d) = r.strip_prefix(s) {
                    if d.is_empty() || matches!(d, ",d0" | ",d1" | ",d2") {
                        return true;
                    }
                }
            }
        }
    }
    false
}

/// `artikel:menge; artikel:menge`
pub fn stoffe(t: &str) -> Result<Vec<(String, f64)>, String> {
    let mut out = Vec::new();
    for x in t.split(';').map(str::trim).filter(|x| !x.is_empty()) {
        let fehler = || "stoffe: Form „artikel:menge; artikel:menge“".to_string();
        let (a, m) = x.split_once(':').ok_or_else(fehler)?;
        let (a, m) = (a.trim(), m.trim());
        let m = einfache_zahl(m)
            .filter(|_| !m.starts_with('-'))
            .ok_or_else(fehler)?;
        if !ist_key(a) && !ist_kennung(a) {
            return Err(fehler());
        }
        out.push((a.to_string(), m));
    }
    Ok(out)
}

/// Werks-Kennung: 22 Zeichen aus `0-9A-Za-z_$` (IFC-Guid).
pub fn ist_kennung(s: &str) -> bool {
    s.len() == 22
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '$')
}

/// `wert:Text|wert:Text`
pub fn wahl_liste(t: &str) -> Result<Vec<(f64, String)>, String> {
    let mut out = Vec::new();
    for x in t.split('|').map(str::trim).filter(|x| !x.is_empty()) {
        let fehler = || "wahl: Form „wert:Text|wert:Text“".to_string();
        let (v, s) = x.split_once(':').ok_or_else(fehler)?;
        let (v, s) = (v.trim(), s.trim());
        let v = einfache_zahl(v).ok_or_else(fehler)?;
        if s.is_empty() {
            return Err(fehler());
        }
        out.push((v, s.to_string()));
    }
    Ok(out)
}

/// `param=zahl; param=zahl`
pub fn typ_werte(t: &str) -> Result<Vec<(String, f64)>, String> {
    let mut out = Vec::new();
    for x in t.split(';').map(str::trim).filter(|x| !x.is_empty()) {
        let fehler = || "werte: Form „key=zahl; key=zahl“".to_string();
        let (k, v) = x.split_once('=').ok_or_else(fehler)?;
        let (k, v) = (k.trim(), v.trim());
        let v = einfache_zahl(v).ok_or_else(fehler)?;
        if !ist_key(k) {
            return Err(fehler());
        }
        out.push((k.to_string(), v));
    }
    Ok(out)
}

/// Einheit einer Leistung wie in der Werkbank (`stk` heißt dort `st`).
fn einheit_l(e: &str) -> &str {
    match e {
        "stk" | "st" => "st",
        e => e,
    }
}

/// Einheit und Bezug einer Leistung: eigene `[leistung]` oder Werks-Kennung.
fn leistung_info<'a>(def: &'a Def, best: &'a Bestand, g: &str) -> Option<(&'a str, &'a str)> {
    if let Some(r) = def.leistung.iter().find(|r| r.key() == g) {
        return Some((
            einheit_l(r.get("einheit").unwrap_or("")),
            r.get("bezug").unwrap_or(""),
        ));
    }
    best.leistung(g).map(|w| (w.einheit, w.bezug))
}

struct Pruefer<'a> {
    def: &'a Def,
    best: &'a Bestand,
    b: Vec<Befund>,
    bekannt: BTreeSet<String>,
}

impl Pruefer<'_> {
    fn f(&mut self, zeile: usize, t: impl Into<String>) {
        self.b.push(Befund::fehler(zeile, t));
    }

    fn h(&mut self, zeile: usize, t: impl Into<String>) {
        self.b.push(Befund::hinweis(zeile, t));
    }

    /// Formel übersetzbar und nur mit bekannten Namen (dazu `extra`).
    fn formel(&mut self, src: &str, zeile: usize, was: &str, extra: &[&str]) {
        match Formel::neu(src) {
            Err(e) => self.f(zeile, format!("{was}: {e}")),
            Ok(f) => {
                if !was.starts_with("[menge]") && !f.volumen_baustoffe().is_empty() {
                    self.f(zeile, format!("{was}: volumen() nur in [menge]"));
                }
                for n in f.namen() {
                    if !self.bekannt.contains(&n) && !extra.contains(&n.as_str()) {
                        self.f(zeile, format!("{was}: unbekannter Name „{n}“"));
                    }
                }
            }
        }
    }

    fn statisch(&mut self) {
        let def = self.def;
        for sec in ["bauteil", "hoehe"] {
            let v = def.abschnitt(sec).unwrap();
            if v.is_empty() {
                self.f(0, format!("[{sec}] fehlt"));
            }
            if v.len() > 1 {
                self.f(v[1].zeile, format!("[{sec}] mehrfach"));
            }
        }
        for (sec, pflicht, wahl) in SPEC {
            for r in def.abschnitt(sec).unwrap() {
                for k in pflicht {
                    if !r.hat(k) {
                        self.f(r.zeile, format!("[{sec}]: Pflichtfeld „{k}“ fehlt"));
                    }
                }
                for (k, _) in &r.felder {
                    if !pflicht.contains(&k.as_str()) && !wahl.contains(&k.as_str()) {
                        self.h(
                            r.zeile,
                            format!("[{sec}]: Feld „{k}“ unbekannt, wird ignoriert"),
                        );
                    }
                }
            }
        }
        let mut stand = 0;
        for r in &def.notiz {
            if let Some(a) = r.get("art") {
                if !["stand", "offen", "entschieden"].contains(&a) {
                    self.h(r.zeile, "[notiz] art: stand, offen, entschieden");
                }
                if a == "stand" {
                    stand += 1;
                    if stand == 2 {
                        self.h(
                            r.zeile,
                            "[notiz] art=stand mehrfach: bitte nur eine Zeile zum aktuellen Stand",
                        );
                    }
                }
            }
        }
        if let Some(bt) = def.bauteil.first() {
            self.bauteil(bt);
        }
        let hoehe = def.hoehe.first();
        if let Some(h) = hoehe {
            if h.get("ab").is_some_and(|a| a != "uk" && !a.is_empty()) {
                self.f(h.zeile, "[hoehe] ab: in SZB 0 nur „uk“");
            }
        }
        for k in ["GH", "DECKE", "LICHT", "pi"] {
            self.bekannt.insert(k.into());
        }
        let mut gesehen = BTreeSet::new();
        for r in def.param.iter().chain(&def.wert) {
            let Some(k) = r.get("key") else {
                continue;
            };
            if !ist_key(k) {
                self.f(r.zeile, format!("key „{k}“: Kleinbuchstaben, Ziffern, _"));
            }
            if RESERVIERT.contains(&k) {
                self.f(r.zeile, format!("key „{k}“ ist reserviert"));
            }
            if !gesehen.insert(k) {
                self.f(r.zeile, format!("key „{k}“ doppelt"));
            }
        }
        for r in &def.param {
            for fl in ["wert", "min", "max"] {
                if let Some(v) = r.get(fl) {
                    self.formel(v, r.zeile, &format!("[param] {} {fl}", r.key()), &[]);
                }
            }
            self.bekannt.insert(r.key().to_string());
            if let Some(e) = r.get("einheit") {
                if !e.is_empty() && !EINH_P.contains(&e) {
                    self.f(r.zeile, format!("einheit: {}", EINH_P.join(", ")));
                }
            }
            if let Some(g) = r.get("ganz") {
                if !g.is_empty() && g != "ja" && g != "nein" {
                    self.f(r.zeile, "ganz: ja oder nein");
                }
            }
        }
        for r in &def.wert {
            if let Some(f) = r.get("formel") {
                self.formel(f, r.zeile, &format!("[wert] {}", r.key()), &[]);
            }
            self.bekannt.insert(r.key().to_string());
        }
        if let Some(h) = hoehe {
            if let Some(v) = h.get("versatz") {
                self.formel(v, h.zeile, "[hoehe] versatz", &[]);
            }
        }
        let mut mats: Vec<String> = self
            .best
            .baustoffe
            .iter()
            .map(|b| b.key.to_string())
            .collect();
        for r in &def.baustoff {
            let k = r.key();
            if !r.hat("key") {
                continue;
            }
            if !ist_key(k) {
                self.f(r.zeile, format!("baustoff key „{k}“ ungültig"));
            }
            if mats.iter().any(|m| m == k) {
                self.f(r.zeile, format!("baustoff „{k}“ gibt es schon"));
            }
            mats.push(k.to_string());
            if let Some(kat) = r.get("kategorie") {
                if !kat.is_empty() && !KATEGORIE.contains(&kat) {
                    self.f(r.zeile, format!("kategorie: {}", KATEGORIE.join(", ")));
                }
            }
            if let Some(f) = r.get("farbe") {
                if !f.is_empty() && !(f.len() == 6 && f.chars().all(|c| c.is_ascii_hexdigit())) {
                    self.f(r.zeile, "farbe: 6 Hex-Ziffern");
                }
            }
            if let Some(d) = r.get("rohdichte") {
                if !d.is_empty() && !d.trim().parse::<f64>().is_ok_and(|v| v > 0.0) {
                    self.f(r.zeile, "rohdichte: Zahl > 0");
                }
            }
        }
        let mat = |k: &str| mats.iter().any(|m| m == k);
        for r in &def.koerper {
            let form = r.get("form").unwrap_or("");
            let Some(pflicht) = form_felder(form) else {
                if !form.is_empty() {
                    self.f(
                        r.zeile,
                        format!("form „{form}“: quader, prisma oder zylinder"),
                    );
                }
                continue;
            };
            for k in pflicht {
                if !r.hat(k) {
                    self.f(r.zeile, format!("[koerper] {form}: Feld „{k}“ fehlt"));
                }
            }
            if let Some(b) = r.get("baustoff") {
                if !b.is_empty() && !mat(b) {
                    self.f(
                        r.zeile,
                        format!("Baustoff „{b}“ unbekannt (Abschnitt 9 oder eigener [baustoff])"),
                    );
                }
            }
            if let Some(f) = r.get("funktion") {
                if !FUNKTION.contains(&f) {
                    self.f(r.zeile, format!("funktion: {}", FUNKTION.join(", ")));
                }
            }
            if form == "prisma"
                && r.get("ebene")
                    .is_some_and(|e| !e.is_empty() && !["xy", "xz", "yz"].contains(&e))
            {
                self.f(r.zeile, "ebene: xy, xz oder yz");
            }
            if form == "zylinder"
                && r.get("achse")
                    .is_some_and(|a| !a.is_empty() && !["x", "y", "z"].contains(&a))
            {
                self.f(r.zeile, "achse: x, y oder z");
            }
            for fl in [
                "anzahl", "wenn", "drehung", "x", "y", "z", "b", "t", "h", "von", "bis", "r",
                "seiten",
            ] {
                if let Some(v) = r.get(fl) {
                    self.formel(v, r.zeile, &format!("[koerper] {fl}"), &["i", "n"]);
                }
            }
            if form == "prisma" {
                if let Some(p) = r.get("punkte") {
                    match punkte(p) {
                        Err(e) => self.f(r.zeile, format!("punkte: {e}")),
                        Ok(p) => {
                            if p.len() < 3 {
                                self.f(r.zeile, "punkte: mindestens 3");
                            }
                            for (a, b) in p {
                                self.formel(&a, r.zeile, "[koerper] punkte", &["i", "n"]);
                                self.formel(&b, r.zeile, "[koerper] punkte", &["i", "n"]);
                            }
                        }
                    }
                }
            }
        }
        if def.koerper.is_empty() {
            self.f(0, "kein [koerper]");
        }
        let mut mk = BTreeSet::new();
        for r in &def.menge {
            if r.hat("key") {
                let k = r.key();
                if !ist_key(k) {
                    self.f(r.zeile, format!("menge key „{k}“ ungültig"));
                }
                if !mk.insert(k) {
                    self.f(r.zeile, format!("menge key „{k}“ doppelt"));
                }
            }
            if let Some(e) = r.get("einheit") {
                if !e.is_empty() && !EINH_M.contains(&e) {
                    self.f(r.zeile, format!("einheit: {}", EINH_M.join(", ")));
                }
            }
            for (feld, was) in [("formel", String::new()), ("dicke", " dicke".to_string())] {
                let Some(src) = r.get(feld) else {
                    continue;
                };
                self.formel(src, r.zeile, &format!("[menge] {}{was}", r.key()), &[]);
                if let Ok(f) = Formel::neu(src) {
                    for b in f.volumen_baustoffe() {
                        if !mat(&b) {
                            self.f(r.zeile, format!("volumen({b}): Baustoff unbekannt"));
                        }
                    }
                }
            }
            if let Some(b) = r.get("baustoff") {
                if !b.is_empty() && !mat(b) {
                    self.f(r.zeile, format!("Baustoff „{b}“ unbekannt"));
                }
            }
            if let Some(kg) = r.get("kg") {
                if !kg.is_empty() && !ist_kg(kg) {
                    self.f(r.zeile, "kg: dreistellig");
                }
            }
        }
        self.bim(&mats);
        self.bedienung();
        if def.menge.is_empty() {
            self.h(0, "keine [menge]: Skizzeo hätte nichts abzurechnen");
        }
    }

    fn bauteil(&mut self, bt: &Satz) {
        let l = bt.zeile;
        if let Some(k) = bt.get("key") {
            let ok = k
                .split_once('.')
                .is_some_and(|(a, b)| ist_key(a) && ist_key(b));
            if !ok {
                self.f(
                    l,
                    format!("key „{k}“: Form herkunft.name, Kleinbuchstaben, genau ein Punkt"),
                );
            } else if let Err(e) = key_als_datei(k) {
                self.f(l, format!("key „{k}“: {e}"));
            }
        }
        if let Some(p) = bt.get("praefix") {
            let ok = (2..=3).contains(&p.len()) && p.chars().all(|c| c.is_ascii_uppercase());
            if !p.is_empty() && !ok {
                self.f(l, "praefix: 2-3 Großbuchstaben");
            }
            if self.best.praefixe.iter().any(|x| x == p) {
                self.f(l, format!("praefix „{p}“ ist in Skizzeo belegt"));
            }
        }
        if let Some(g) = bt.get("genus") {
            if !g.is_empty() && !["f", "m", "n"].contains(&g) {
                self.f(l, "genus: f, m oder n");
            }
        }
        if let Some(kg) = bt.get("kg") {
            if !ist_kg(kg) {
                self.f(l, "kg: dreistellige Kostengruppe 300-499");
            } else if !self.best.kgs.contains(&kg.parse().unwrap_or(0)) {
                self.h(
                    l,
                    format!(
                        "KG {kg} kennt der Werksbestand noch nicht, wird beim Einlesen geprüft"
                    ),
                );
            }
        }
        if let Some(g) = bt.get("gewerk") {
            if !ist_gewerk(g) {
                self.f(l, "gewerk: ATV-Nummer 18xxx");
            } else if !self.best.hat_gewerk(g.parse().unwrap_or(0)) {
                self.h(
                    l,
                    format!("Gewerk {g} nicht im Werksbestand, Skizzeo fragt beim Einlesen"),
                );
            }
        }
        if let Some(i) = bt.get("ifc") {
            if !i.is_empty() && !ist_ifc(i) {
                self.h(l, "ifc: Form IfcKlasse oder IfcKlasse.TYP");
            }
        }
        if let Some(v) = bt.get("version") {
            let ok = v.starts_with(|c: char| ('1'..='9').contains(&c))
                && v.chars().all(|c| c.is_ascii_digit());
            if !v.is_empty() && !ok {
                self.f(l, "version: ganze Zahl ab 1");
            }
        }
        if let Some(a) = bt.get("art") {
            let mut c = a.chars();
            let ok = matches!(c.next(), Some('a'..='z'))
                && c.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit());
            if !ok {
                self.f(
                    l,
                    format!(
                        "art: ein Wort in Kleinbuchstaben, z. B. {}",
                        self.best.arten[..3].join(", ")
                    ),
                );
            }
        }
        if let Some(a) = bt.get("aussen") {
            if a != "ja" && a != "nein" {
                self.f(l, "aussen: ja oder nein");
            }
        }
    }

    fn bim(&mut self, mats: &[String]) {
        let def = self.def;
        let mat = |k: &str| mats.iter().any(|m| m == k);
        for r in &def.baustoff {
            let w = format!("[baustoff] {}", r.key());
            self.num(r, "lambda", 0.001, &w);
            self.num(r, "mu", 0.0, &w);
            self.num(r, "c", 0.0, &w);
            if let Some(l) = r.get("lambda") {
                if l.parse::<f64>().is_ok_and(|v| v > 5.0) {
                    self.h(r.zeile, format!("{w}: λ {l} W/(mK) ungewöhnlich hoch"));
                }
            }
            if let Some(e) = r.get("euroklasse") {
                if !ist_euroklasse(e) {
                    self.f(r.zeile, format!("{w} euroklasse: A1, E, F nur ohne Zusatz; A2 bis D auch mit -s1 bis -s3 und ,d0 bis ,d2; Bodenbeläge mit fl, dann nur -s1 oder -s2"));
                }
            }
            self.meta(r, &w);
        }
        let mut ak = BTreeSet::new();
        for r in &def.artikel {
            let w = format!("[artikel] {}", r.key());
            if r.hat("key") {
                if !ist_key(r.key()) {
                    self.f(r.zeile, format!("{w}: key ungültig"));
                }
                if !ak.insert(r.key()) {
                    self.f(r.zeile, format!("{w}: key doppelt"));
                }
            }
            if let Some(e) = r.get("einheit") {
                if !e.is_empty() && !EINH_M.contains(&e) {
                    self.f(r.zeile, format!("{w} einheit: {}", EINH_M.join(", ")));
                }
            }
            if let Some(b) = r.get("baustoff") {
                if !b.is_empty() && !mat(b) {
                    self.f(r.zeile, format!("{w}: Baustoff „{b}“ unbekannt"));
                }
            }
            if r.get("name").is_some_and(|n| laenge(n) > 70) {
                self.h(r.zeile, format!("{w}: name länger als 70 Zeichen"));
            }
            self.num(r, "preis", 0.0, &w);
            self.num(r, "dicke", 0.001, &w);
            self.num(r, "stueck", 0.0001, &w);
            self.meta(r, &w);
        }
        let mut lk = BTreeSet::new();
        for r in &def.leistung {
            let w = format!("[leistung] {}", r.key());
            if r.hat("key") {
                if !ist_key(r.key()) {
                    self.f(r.zeile, format!("{w}: key ungültig"));
                }
                if !lk.insert(r.key()) {
                    self.f(r.zeile, format!("{w}: key doppelt"));
                }
            }
            if let Some(e) = r.get("einheit") {
                if !e.is_empty() && !EINH_M.contains(&e) {
                    self.f(r.zeile, format!("{w} einheit: {}", EINH_M.join(", ")));
                }
            }
            if let Some(b) = r.get("bezug") {
                if !b.is_empty() && !BEZUG.contains(&b) {
                    self.f(r.zeile, format!("{w} bezug: {}", BEZUG.join(", ")));
                }
            }
            if let Some(k) = r.get("kurztext") {
                let n = laenge(k);
                if n > 70 {
                    self.h(
                        r.zeile,
                        format!("{w}: kurztext hat {n} Zeichen, Skizzeo nimmt höchstens 70"),
                    );
                }
            }
            if r.get("gewerk").is_some_and(|g| !ist_gewerk(g)) {
                self.f(r.zeile, format!("{w} gewerk: ATV-Nummer 18xxx"));
            }
            if r.get("kg").is_some_and(|k| !k.is_empty() && !ist_kg(k)) {
                self.f(r.zeile, format!("{w} kg: dreistellig"));
            }
            if r.get("funktion").is_some_and(|f| !FUNKTION.contains(&f)) {
                self.f(r.zeile, format!("{w} funktion: {}", FUNKTION.join(", ")));
            }
            for k in ["stunden", "geraet", "sonstiges", "dmin", "dmax"] {
                self.num(r, k, 0.0, &w);
            }
            let zahl_von = |k: &str| r.get(k).and_then(einfache_zahl);
            if let (Some(lo), Some(hi)) = (zahl_von("dmin"), zahl_von("dmax")) {
                if lo > hi {
                    self.f(r.zeile, format!("{w}: dmin größer als dmax"));
                }
            }
            if let Some(s) = r.get("stoffe") {
                match stoffe(s) {
                    Err(e) => self.f(r.zeile, format!("{w}: {e}")),
                    Ok(v) => {
                        for (a, _) in v {
                            if def.artikel.iter().any(|x| x.key() == a)
                                || self.best.artikel(&a).is_some()
                            {
                                continue;
                            }
                            if ist_kennung(&a) {
                                self.h(r.zeile, format!("{w}: Artikel „{a}“ nicht im Werksbestand, Skizzeo fragt beim Einlesen nach"));
                            } else {
                                self.f(r.zeile, format!("{w}: Artikel „{a}“ fehlt als [artikel] und ist keine Werks-Kennung"));
                            }
                        }
                    }
                }
            }
            self.meta(r, &w);
        }
        for r in &def.menge {
            let w = format!("[menge] {}", r.key());
            if let Some(b) = r.get("bezug") {
                if !BEZUG.contains(&b) {
                    self.f(r.zeile, format!("{w} bezug: {}", BEZUG.join(", ")));
                }
            }
            let Some(g) = r.get("leistung").filter(|g| !g.is_empty()) else {
                continue;
            };
            let Some((einheit, bezug)) = leistung_info(def, self.best, g) else {
                if ist_kennung(g) {
                    self.h(r.zeile, format!("{w}: Leistung „{g}“ nicht im Werksbestand, Skizzeo fragt beim Einlesen nach"));
                } else {
                    self.f(
                        r.zeile,
                        format!("{w}: Leistung „{g}“ weder [leistung] noch Werks-Kennung"),
                    );
                }
                continue;
            };
            if let Some(e) = r.get("einheit").filter(|e| !e.is_empty()) {
                if einheit_l(e) != einheit {
                    self.h(
                        r.zeile,
                        format!("{w}: Leistung rechnet in {einheit}, Menge in {e}"),
                    );
                }
            }
            if let Some(b) = r.get("bezug").filter(|b| !b.is_empty()) {
                if !bezug.is_empty() && b != bezug {
                    self.h(
                        r.zeile,
                        format!("{w}: Bezug {b}, Leistung rechnet nach {bezug}"),
                    );
                }
            }
        }
    }

    fn num(&mut self, r: &Satz, k: &str, min: f64, was: &str) {
        let Some(v) = r.get(k) else {
            return;
        };
        if !einfache_zahl(v).is_some_and(|x| x >= min) {
            self.f(
                r.zeile,
                format!("{was} {k}: Zahl ≥ {} (Dezimalpunkt)", js_zahl(min)),
            );
        }
    }

    fn meta(&mut self, r: &Satz, was: &str) {
        if let Some(s) = r.get("sicherheit") {
            if !SICHERHEIT.contains(&s) {
                self.f(
                    r.zeile,
                    format!("{was} sicherheit: {}", SICHERHEIT.join(", ")),
                );
            }
        }
        if let Some(s) = r.get("stand") {
            if !ist_monat(s) {
                self.f(r.zeile, format!("{was} stand: MM/JJJJ, z. B. 10/2026"));
            }
        }
    }

    fn bedienung(&mut self) {
        let def = self.def;
        if def.bedienung.len() > 1 {
            self.f(def.bedienung[1].zeile, "[bedienung] mehrfach");
        }
        if let Some(bd) = def.bedienung.first() {
            let l = bd.zeile;
            if let Some(g) = bd.get("gruppe").filter(|g| !g.is_empty()) {
                if !GRUPPEN.iter().any(|(k, _)| *k == g) {
                    let alle: Vec<_> = GRUPPEN.iter().map(|(k, _)| *k).collect();
                    self.f(l, format!("[bedienung] gruppe: {}", alle.join(", ")));
                }
            }
            let ein = bd.get("einfuegen").unwrap_or("");
            if !ein.is_empty() && !EINFUEGEN.contains(&ein) {
                let zusatz = if ein == "polygon" {
                    " (polygon kommt nach SZB 0)"
                } else {
                    ""
                };
                self.f(
                    l,
                    format!("[bedienung] einfuegen: {}{zusatz}", EINFUEGEN.join(", ")),
                );
            }
            for k in ["laenge", "breite", "tiefe"] {
                let Some(v) = bd.get(k).filter(|v| !v.is_empty()) else {
                    continue;
                };
                match def.param_von(v) {
                    None => self.f(
                        l,
                        format!("[bedienung] {k}={v}: kein [param] mit diesem key"),
                    ),
                    Some(p) => {
                        if p.get("einheit").filter(|e| !e.is_empty()).unwrap_or("mm") != "mm" {
                            self.f(
                                l,
                                format!("[bedienung] {k}: Parameter muss einheit=mm haben"),
                            );
                        }
                    }
                }
            }
            let hat = |k: &str| bd.get(k).is_some_and(|v| !v.is_empty());
            if ein == "linie" && !hat("laenge") {
                self.f(l, "[bedienung] einfuegen=linie braucht laenge=<param>");
            }
            if ein == "rechteck" && (!hat("breite") || !hat("tiefe")) {
                self.f(
                    l,
                    "[bedienung] einfuegen=rechteck braucht breite= und tiefe=",
                );
            }
            if ein == "punkt" && (hat("laenge") || hat("breite") || hat("tiefe")) {
                self.h(l, "[bedienung] einfuegen=punkt bindet keine Maße, laenge/breite/tiefe werden ignoriert");
            }
            if let Some(d) = bd.get("drehen") {
                if d != "ja" && d != "nein" {
                    self.f(l, "[bedienung] drehen: ja oder nein");
                }
            }
        }
        let mut nz = 0;
        for r in &def.param {
            let (w, l) = (format!("[param] {}", r.key()), r.zeile);
            if let Some(n) = r.get("name").filter(|n| !n.is_empty()) {
                let z = laenge(n);
                if z > MAX_NAME {
                    self.h(
                        l,
                        format!("{w}: name hat {z} Zeichen, im Paneel passen {MAX_NAME}"),
                    );
                }
                if einheit_im_namen(n) {
                    self.h(
                        l,
                        format!("{w}: Einheit gehört in einheit=, nicht in den Namen"),
                    );
                }
                let klein = n.to_lowercase();
                let klein = klein.strip_suffix('.').unwrap_or(&klein);
                if let Some((_, g)) = UNWORT.iter().find(|(u, _)| *u == klein) {
                    self.h(l, format!("{w}: Begriff „{n}“, im Glossar heißt es „{g}“"));
                }
            }
            if r.get("gruppe").is_some_and(|g| laenge(g) > MAX_GRUPPE) {
                self.h(l, format!("{w}: gruppe länger als {MAX_GRUPPE} Zeichen"));
            }
            if r.get("hilfe").is_some_and(|g| laenge(g) > MAX_HILFE) {
                self.h(l, format!("{w}: hilfe länger als {MAX_HILFE} Zeichen"));
            }
            for k in ["janein", "zeichnen"] {
                if r.get(k).is_some_and(|v| v != "ja" && v != "nein") {
                    self.f(l, format!("{w} {k}: ja oder nein"));
                }
            }
            if r.ja("zeichnen") {
                nz += 1;
            }
            if let Some(s) = r.get("sichtbar") {
                self.formel(s, l, &format!("{w} sichtbar"), &[]);
            }
            if let Some(wl) = r.get("wahl") {
                match wahl_liste(wl) {
                    Err(e) => self.f(l, format!("{w} {e}")),
                    Ok(liste) => {
                        if liste.len() < 2 {
                            self.f(l, format!("{w} wahl: mindestens zwei Einträge"));
                        }
                        if let Some(v) = r.get("wert") {
                            if let Some(x) = einfache_zahl(v) {
                                if !liste.iter().any(|(y, _)| *y == x) {
                                    self.f(l, format!("{w}: Vorgabe {v} ist keine der Wahlen"));
                                }
                            }
                        }
                    }
                }
            }
            if r.ja("janein") && r.get("wert").is_some_and(|v| v != "0" && v != "1") {
                self.f(l, format!("{w}: janein braucht wert=0 oder wert=1"));
            }
            if r.ja("janein") && r.get("wahl").is_some_and(|v| !v.is_empty()) {
                self.f(l, format!("{w}: entweder janein oder wahl"));
            }
        }
        if nz > MAX_ZEICHNEN {
            self.h(
                0,
                format!("{nz} Parameter mit zeichnen=ja, im Werkzeug-Paneel passen {MAX_ZEICHNEN}"),
            );
        }
        for (sec, v) in [("wert", &def.wert), ("menge", &def.menge)] {
            for r in v {
                if r.get("anzeigen").is_some_and(|a| a != "ja" && a != "nein") {
                    self.f(r.zeile, format!("[{sec}] anzeigen: ja oder nein"));
                }
            }
        }
        let mut tk = BTreeSet::new();
        let mut std = 0;
        for r in &def.typ {
            let (w, l) = (format!("[typ] {}", r.key()), r.zeile);
            if r.hat("key") {
                if !ist_key(r.key()) {
                    self.f(l, format!("{w}: key ungültig"));
                }
                if !tk.insert(r.key()) {
                    self.f(l, format!("{w}: key doppelt"));
                }
            }
            if r.get("name").is_some_and(|n| laenge(n) > MAX_TYP) {
                self.h(l, format!("{w}: name länger als {MAX_TYP} Zeichen"));
            }
            match r.get("standard") {
                Some("ja") => std += 1,
                Some("nein") | None => {}
                Some(_) => self.f(l, format!("{w} standard: ja oder nein")),
            }
            if let Some(t) = r.get("werte") {
                match typ_werte(t) {
                    Err(e) => self.f(l, format!("{w} {e}")),
                    Ok(v) => {
                        for (k, x) in v {
                            let Some(p) = def.param_von(&k) else {
                                self.f(l, format!("{w}: „{k}“ ist kein [param]"));
                                continue;
                            };
                            if let Some(Ok(liste)) = p.get("wahl").map(wahl_liste) {
                                if !liste.iter().any(|(y, _)| *y == x) {
                                    self.f(
                                        l,
                                        format!("{w}: {k}={} ist keine der Wahlen", js_zahl(x)),
                                    );
                                }
                            }
                        }
                    }
                }
            }
        }
        if std > 1 {
            self.h(0, "[typ]: mehr als ein Typ mit standard=ja");
        }
    }
}

/// Zahl wie JavaScript sie in Texte schreibt (`0.001`, `900`).
fn js_zahl(v: f64) -> String {
    let s = format!("{v}");
    s.strip_suffix(".0").map(str::to_string).unwrap_or(s)
}

/// `^[3-4]\d\d$`
fn ist_kg(s: &str) -> bool {
    s.len() == 3 && matches!(s.as_bytes()[0], b'3' | b'4') && s.chars().all(|c| c.is_ascii_digit())
}

/// `^18\d{3}$`
fn ist_gewerk(s: &str) -> bool {
    s.len() == 5 && s.starts_with("18") && s.chars().all(|c| c.is_ascii_digit())
}

/// `^Ifc[A-Za-z]+(\.[A-Z_]+)?$`
fn ist_ifc(s: &str) -> bool {
    let Some(r) = s.strip_prefix("Ifc") else {
        return false;
    };
    let (k, t) = match r.split_once('.') {
        Some((k, t)) => (k, Some(t)),
        None => (r, None),
    };
    !k.is_empty()
        && k.chars().all(|c| c.is_ascii_alphabetic())
        && t.is_none_or(|t| !t.is_empty() && t.chars().all(|c| c.is_ascii_uppercase() || c == '_'))
}

/// Klammer, Grad oder eine Einheit als eigenes Wort (`mm`, `cm`, `m`,
/// `grad`); Wortgrenzen wie in JavaScript (nur ASCII-Wortzeichen).
fn einheit_im_namen(n: &str) -> bool {
    if n.contains(['(', ')', '°']) {
        return true;
    }
    n.split(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
        .any(|w| matches!(w, "mm" | "cm" | "m" | "grad"))
}

/// Dickenband (mm) der Leistung `g` einer Menge: eigene `[leistung]` mit
/// `dmin`/`dmax`, sonst Werks-Leistung mit ihrem Band (Vertrag §5, §10);
/// `None` ohne Band.
pub fn leistung_band(def: &Def, best: &Bestand, g: &str) -> Option<(Option<f64>, Option<f64>)> {
    let band = match def.leistung.iter().find(|r| r.key() == g) {
        Some(r) => {
            let z = |k: &str| r.get(k).and_then(einfache_zahl);
            (z("dmin"), z("dmax"))
        }
        None => {
            let (lo, hi) = best.leistung(g)?.band?;
            (Some(f64::from(lo)), Some(f64::from(hi)))
        }
    };
    (band.0.is_some() || band.1.is_some()).then_some(band)
}

/// „180–250 mm“, „bis 600 mm“, „ab 100 mm“.
pub fn band_text(b: (Option<f64>, Option<f64>)) -> String {
    match b {
        (Some(lo), Some(hi)) => format!("{}–{} mm", zahl(lo, 0), zahl(hi, 0)),
        (None, Some(hi)) => format!("bis {} mm", zahl(hi, 0)),
        (Some(lo), _) => format!("ab {} mm", zahl(lo, 0)),
        (None, None) => String::new(),
    }
}

/// Liegt die Dicke `d` außerhalb des Bands?
pub fn band_aus(b: (Option<f64>, Option<f64>), d: f64) -> bool {
    b.0.is_some_and(|m| d < m - 1e-9) || b.1.is_some_and(|m| d > m + 1e-9)
}

/// Hinweise zum Dickenband je Menge mit Leistung: `dicke` fehlt (nur mit
/// `fehlt`) oder liegt außerhalb.
fn band_hinweise(def: &Def, best: &Bestand, e: &Ergebnis, fehlt: bool) -> Vec<Befund> {
    let mut out = Vec::new();
    for (i, _) in &e.mengen {
        let r = &def.menge[*i];
        let Some(g) = r.get("leistung").filter(|g| !g.is_empty()) else {
            continue;
        };
        let Some(band) = leistung_band(def, best, g) else {
            continue;
        };
        let w = format!("[menge] {}: ", r.key());
        if !r.hat("dicke") {
            if fehlt {
                out.push(Befund::hinweis(
                    r.zeile,
                    format!(
                        "{w}Leistung {g} gilt für {}, ohne dicke prüft Skizzeo das nicht",
                        band_text(band)
                    ),
                ));
            }
            continue;
        }
        let Some(&(_, d)) = e.dicken.iter().find(|(j, _)| j == i) else {
            continue;
        };
        if band_aus(band, d) {
            out.push(Befund::hinweis(
                r.zeile,
                format!(
                    "{w}Dicke {} mm liegt außerhalb von {g} ({}), Skizzeo setzt dafür keine Position",
                    zahl(d, 0),
                    band_text(band)
                ),
            ));
        }
    }
    out
}

/// Die Grenzen eines Parameters mit den Werten `pv`.
fn grenzen_von(r: &Satz, pv: &Umfeld, g: &Geschoss) -> Grenzen {
    let mut u = g.umfeld();
    u.extend(pv.iter().map(|(k, v)| (k.clone(), *v)));
    let f = |k: &str| r.get(k).and_then(|s| formel::rechnen(s, &u, None).ok());
    Grenzen {
        min: f("min"),
        max: f("max"),
        wert: f("wert"),
    }
}

/// Prüft den Text eines Bauteils wie die Werkbank, mit den Vorgaben im
/// Geschoss `g`.
pub fn pruefen(text: &str, best: &Bestand, g: &Geschoss) -> Pruefung {
    pruefen_mit(text, best, g, true)
}

/// Wie [`pruefen`], aber ohne Grenzprüfung: für die Definition in einer
/// Projektdatei, die beim Öffnen nur gelesen und gerechnet wird (Review,
/// Durchsicht Schrittplan Nr. 12). Dieselben Grenzen für Datei, Zeilen,
/// Sätze und Formeln gelten.
pub fn pruefen_beim_oeffnen(text: &str, best: &Bestand, g: &Geschoss) -> Pruefung {
    pruefen_mit(text, best, g, false)
}

fn pruefen_mit(text: &str, best: &Bestand, g: &Geschoss, grenzpruefung: bool) -> Pruefung {
    let (def, lese) = lesen::lesen(text);
    let mut p = Pruefer {
        def: &def,
        best,
        b: lese,
        bekannt: BTreeSet::new(),
    };
    p.statisch();
    let mut b = p.b;
    let pv = rechnen::vorgaben(&def, g);
    let mut grenzen = BTreeMap::new();
    for r in &def.param {
        let o = grenzen_von(r, &pv, g);
        let k = r.key();
        let v = pv.get(k).copied().unwrap_or(0.0);
        if let (Some(lo), Some(hi)) = (o.min, o.max) {
            if lo > hi {
                b.push(Befund::fehler(
                    r.zeile,
                    format!("[param] {k}: min größer als max"),
                ));
            }
        }
        let raus =
            |x: f64| o.min.is_some_and(|m| x < m - 1e-9) || o.max.is_some_and(|m| x > m + 1e-9);
        if let Some(w) = o.wert {
            if raus(w) {
                b.push(Befund::fehler(
                    r.zeile,
                    format!("[param] {k}: Vorgabe {} außerhalb min/max", zahl(w, 2)),
                ));
            }
            if r.ja("ganz") && (w - w.round()).abs() > 1e-9 {
                b.push(Befund::fehler(
                    r.zeile,
                    format!("[param] {k}: Vorgabe nicht ganz"),
                ));
            }
        }
        if o.wert.is_none_or(|w| (v - w).abs() > 1e-9) && raus(v) {
            b.push(Befund::hinweis(
                r.zeile,
                format!(
                    "Eingestellter Wert {k}={} liegt außerhalb min/max",
                    zahl(v, 2)
                ),
            ));
        }
        grenzen.insert(k.to_string(), o);
    }
    for t in &def.typ {
        let Ok(werte) = typ_werte(t.get("werte").unwrap_or("")) else {
            continue;
        };
        for (k, v) in werte {
            let Some(o) = grenzen.get(&k) else {
                continue;
            };
            if o.min.is_some_and(|m| v < m - 1e-9) || o.max.is_some_and(|m| v > m + 1e-9) {
                b.push(Befund::fehler(
                    t.zeile,
                    format!("[typ] {}: {k}={} außerhalb min/max", t.key(), js_zahl(v)),
                ));
            }
        }
    }
    // Standardtyp = Vorgaben: Skizzeo beginnt mit dem Standardtyp, die
    // Werkbank mit den Vorgaben (aenderung-dicke.md, Nachtrag 19:40)
    for t in def.typ.iter().filter(|t| t.ja("standard")) {
        let Ok(werte) = typ_werte(t.get("werte").unwrap_or("")) else {
            continue;
        };
        let ab: Vec<String> = werte
            .iter()
            .filter_map(|(k, v)| {
                let w = grenzen.get(k)?.wert?;
                ((v - w).abs() > 1e-9).then(|| format!("{k}={} statt {}", js_zahl(*v), js_zahl(w)))
            })
            .collect();
        if !ab.is_empty() {
            b.push(Befund::fehler(
                t.zeile,
                format!(
                    "[typ] {}: Standardtyp weicht von den Vorgaben ab ({})",
                    t.key(),
                    ab.join(", ")
                ),
            ));
        }
    }
    let hart = b.iter().any(Befund::ist_fehler);
    // Rechenschritte für Rechnung und Grenzprüfung zusammen (Review 3cg)
    let mut rc = rechnen::Rechner::neu(rechnen::MAX_SCHRITTE_PRUEFUNG);
    let ergebnis = if def.bauteil.is_empty() && def.koerper.is_empty() {
        Ergebnis::default()
    } else {
        rechnen::rechnen_mit(&mut rc, &def, &pv, g)
    };
    b.extend(ergebnis.befunde.iter().cloned());
    let haupt = band_hinweise(&def, best, &ergebnis, true);
    let band_zeilen: BTreeSet<usize> = haupt.iter().map(|h| h.zeile).collect();
    b.extend(haupt);
    if grenzpruefung && !hart && !rc.erschoepft {
        let mut gesehen: BTreeSet<String> = b.iter().map(|x| x.text.clone()).collect();
        for r in &def.param {
            let k = r.key();
            let o = grenzen[k];
            for (seite, x) in [("min", o.min), ("max", o.max)] {
                let Some(x) = x else {
                    continue;
                };
                if rc.rest == 0 {
                    break;
                }
                let mut pv2 = pv.clone();
                pv2.insert(k.to_string(), x);
                let e = rechnen::rechnen_mit(&mut rc, &def, &pv2, g);
                for f in e.befunde.iter().filter(|f| f.ist_fehler()) {
                    let t = format!("Grenzprüfung {k}={seite} ({}): {}", zahl(x, 2), f.text);
                    if gesehen.insert(t.clone()) {
                        b.push(Befund::fehler(f.zeile, t));
                    }
                }
                for h in band_hinweise(&def, best, &e, false) {
                    if band_zeilen.contains(&h.zeile) {
                        continue;
                    }
                    let t = format!("Grenzprüfung {k}={seite} ({}): {}", zahl(x, 2), h.text);
                    if gesehen.insert(t.clone()) {
                        b.push(Befund::hinweis(h.zeile, t));
                    }
                }
            }
        }
    }
    for (i, v) in &ergebnis.mengen {
        let r = &def.menge[*i];
        let (Some(v), Some("m3"), Some(bs)) = (v, r.get("einheit"), r.get("baustoff")) else {
            continue;
        };
        if bs.is_empty() {
            continue;
        }
        let g = ergebnis.vol.get(bs).copied().unwrap_or(0.0);
        if g > 0.0 && (v - g).abs() / g > 0.01 {
            b.push(Befund::hinweis(
                r.zeile,
                format!(
                    "[menge] {}: {} m³ weicht von {} m³ Körpervolumen ab",
                    r.key(),
                    zahl(*v, 3),
                    zahl(g, 3)
                ),
            ));
        }
        if g == 0.0 && *v > 0.0 {
            b.push(Befund::hinweis(
                r.zeile,
                format!("[menge] {}: kein Körper aus {bs}", r.key()),
            ));
        }
    }
    if ergebnis.anzahl == 0 && !def.koerper.is_empty() && !b.iter().any(Befund::ist_fehler) {
        b.push(Befund::hinweis(
            0,
            "kein Körper sichtbar (anzahl 0 oder wenn = 0)",
        ));
    }
    let vollstaendigkeit = vollstaendigkeit(&def, best);
    Pruefung {
        def,
        befunde: b,
        vollstaendigkeit,
        grenzen,
        ergebnis,
    }
}

/// Was für Mengen, LV und Kosten noch fehlt (kein Fehler).
pub fn vollstaendigkeit(def: &Def, best: &Bestand) -> Vec<String> {
    let mut o = Vec::new();
    for (k, t) in [
        ("art", "Bauteilart (art)"),
        ("aussen", "außen ja/nein (aussen)"),
        ("ifc", "IFC-Klasse (ifc)"),
        ("kg", "Kostengruppe (kg)"),
        ("gewerk", "Gewerk (gewerk)"),
    ] {
        if def.bauteil_feld(k).is_none_or(str::is_empty) {
            o.push(format!("[bauteil]: {t} fehlt"));
        }
    }
    if let Some(a) = def.bauteil_feld("art") {
        let mut c = a.chars();
        let wort = matches!(c.next(), Some('a'..='z'))
            && c.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit());
        if wort && !best.arten.contains(&a) {
            o.push(format!(
                "Bauteilart „{a}“ ist neu: Skizzeo legt sie beim Einlesen nach Rückfrage an"
            ));
        }
    }
    if def.bedienung.is_empty() {
        o.push("[bedienung] fehlt: gruppe und einfuegen festlegen".into());
    }
    let nh = def
        .param
        .iter()
        .filter(|r| r.get("hilfe").is_none_or(str::is_empty))
        .count();
    if nh > 0 {
        o.push(format!("{nh} [param] ohne hilfe (Tooltip und F1)"));
    }
    let nf = def
        .koerper
        .iter()
        .filter(|r| r.get("funktion").is_none_or(str::is_empty))
        .count();
    if nf > 0 {
        o.push(format!("{nf} [koerper] ohne funktion"));
    }
    let leer = |r: &Satz, k: &str| r.get(k).is_none_or(str::is_empty);
    for r in &def.baustoff {
        let w = format!("[baustoff] {}: ", r.key());
        if leer(r, "rohdichte") {
            o.push(format!("{w}rohdichte fehlt"));
        }
        if r.get("kategorie") == Some("insulation") && leer(r, "lambda") {
            o.push(format!("{w}λ (lambda) fehlt"));
        }
        if leer(r, "euroklasse") {
            o.push(format!("{w}euroklasse fehlt"));
        }
        let kennwerte = ["lambda", "mu", "euroklasse", "rohdichte"]
            .iter()
            .any(|k| !leer(r, k));
        if kennwerte && (leer(r, "quelle") || leer(r, "sicherheit")) {
            o.push(format!("{w}Kennwerte ohne quelle oder sicherheit"));
        }
    }
    for r in &def.menge {
        if r.get("einheit") == Some("stk") {
            continue;
        }
        let w = format!("[menge] {}: ", r.key());
        if leer(r, "bezug") {
            o.push(format!("{w}bezug fehlt"));
        }
        if leer(r, "leistung") {
            o.push(format!("{w}keine Leistung, wird in Skizzeo nicht bepreist"));
        }
    }
    for r in &def.artikel {
        let w = format!("[artikel] {}: ", r.key());
        for k in ["preis", "stand", "quelle", "sicherheit"] {
            if !r.hat(k) {
                o.push(format!("{w}{k} fehlt"));
            }
        }
    }
    for r in &def.leistung {
        let w = format!("[leistung] {}: ", r.key());
        for k in ["gewerk", "stunden", "quelle", "sicherheit"] {
            if !r.hat(k) {
                o.push(format!("{w}{k} fehlt"));
            }
        }
        if leer(r, "stoffe") && leer(r, "geraet") && leer(r, "sonstiges") {
            o.push(format!(
                "{w}keine Stoffe (stoffe) und keine Kosten außer Lohn"
            ));
        }
    }
    let grob = def
        .baustoff
        .iter()
        .chain(&def.artikel)
        .chain(&def.leistung)
        .filter(|r| r.get("sicherheit") == Some("grob"))
        .count();
    if grob > 0 {
        o.push(if grob == 1 {
            "1 Angabe ist grob: vor produktivem Einsatz prüfen".to_string()
        } else {
            format!("{grob} Angaben sind grob: vor produktivem Einsatz prüfen")
        });
    }
    o
}

#[cfg(test)]
mod tests {
    use super::*;

    const STUETZE: &str = include_str!("../beispiele/werk.stuetze.szb");

    fn texte(p: &Pruefung) -> Vec<String> {
        p.befunde.iter().map(|b| b.text.clone()).collect()
    }

    fn mit(zeile: &str) -> Pruefung {
        pruefen(
            &format!("{STUETZE}\n{zeile}"),
            &Bestand::werk(),
            &Geschoss::PROBE,
        )
    }

    #[test]
    fn praefix_einer_anderen_erweiterung() {
        let text = STUETZE.replace("key=werk.stuetze", "key=test.stuetze");
        let mut best = Bestand::werk();
        assert!(pruefen(&text, &best, &Geschoss::PROBE).einlesbar());
        best.praefixe.push("ST".into());
        let p = pruefen(&text, &best, &Geschoss::PROBE);
        assert_eq!(texte(&p), ["praefix „ST“ ist in Skizzeo belegt"]);
    }

    #[test]
    fn kennwerte_und_herkunft() {
        let p = mit("[baustoff] key=holz name=\"Holz\" kategorie=timber euroklasse=D-s2,d0 stand=10/2026 sicherheit=belegt lambda=0.13");
        assert!(p.einlesbar(), "{:?}", p.befunde);
        let p = mit("[baustoff] key=holz name=\"Holz\" kategorie=holz euroklasse=A1-s1 stand=2026 sicherheit=gut lambda=,1 farbe=xyz");
        assert_eq!(
            texte(&p),
            [
                "kategorie: masonry, concrete, insulation, plaster, timber, metal",
                "farbe: 6 Hex-Ziffern",
                "[baustoff] holz lambda: Zahl ≥ 0.001 (Dezimalpunkt)",
                "[baustoff] holz euroklasse: A1, E, F nur ohne Zusatz; A2 bis D auch mit -s1 bis -s3 und ,d0 bis ,d2; Bodenbeläge mit fl, dann nur -s1 oder -s2",
                "[baustoff] holz sicherheit: belegt, mittel, grob",
                "[baustoff] holz stand: MM/JJJJ, z. B. 10/2026",
            ]
        );
        for (e, ok) in [
            ("A1", true),
            ("Efl", true),
            ("Bfl-s1", true),
            ("Bfl-s3", false),
            ("C-s3,d2", true),
            ("A2-s1", true),
            ("A2,d0", false),
            ("F-s1", false),
        ] {
            assert_eq!(ist_euroklasse(e), ok, "{e}");
        }
    }

    #[test]
    fn bedienung() {
        let p = mit("[param] key=w name=\"Weite (mm)\" wert=2 wahl=\"0:a|1:b\" janein=ja zeichnen=vielleicht");
        assert_eq!(
            texte(&p),
            [
                "[param] w: Einheit gehört in einheit=, nicht in den Namen",
                "[param] w zeichnen: ja oder nein",
                "[param] w: Vorgabe 2 ist keine der Wahlen",
                "[param] w: janein braucht wert=0 oder wert=1",
                "[param] w: entweder janein oder wahl",
            ]
        );
        let p = mit("[typ] key=st24 name=\"x\" werte=\"b=700; q=1\"");
        assert_eq!(
            texte(&p),
            [
                "[typ] st24: key doppelt",
                "[typ] st24: „q“ ist kein [param]",
                "[typ] st24: b=700 außerhalb min/max",
            ]
        );
        assert!(einheit_im_namen("Fußm"));
        assert!(!einheit_im_namen("Länge"));
    }

    /// Leerer key in [param] und [wert] ist ein Fehler wie in der Werkbank.
    #[test]
    fn leerer_key() {
        for z in [
            "[param] key=\"\" name=\"Zusatz\" einheit=mm wert=1 min=0 max=2",
            "[wert] key=\"\" formel=1",
        ] {
            let p = mit(z);
            assert!(
                texte(&p).contains(&"key „“: Kleinbuchstaben, Ziffern, _".to_string()),
                "{z}: {:?}",
                texte(&p)
            );
        }
        // Ebenso in [bauteil] (Werkbank tests/grenzen/g_leer.szb)
        let p = pruefen(
            &STUETZE.replace("key=werk.stuetze", "key=\"\""),
            &Bestand::werk(),
            &Geschoss::PROBE,
        );
        assert!(texte(&p)
            .contains(&"key „“: Form herkunft.name, Kleinbuchstaben, genau ein Punkt".to_string()));
        assert!(!p.einlesbar());
    }

    #[test]
    fn key_ist_dateiname() {
        assert_eq!(key_als_datei("werk.stuetze"), Ok(()));
        assert_eq!(key_als_datei("com0.x"), Ok(()));
        assert_eq!(key_als_datei("consult.x"), Ok(()));
        for k in ["con.stuetze", "nul.x", "com1.x", "lpt9.a"] {
            assert!(key_als_datei(k).is_err(), "{k}");
        }
        let p = pruefen(
            &STUETZE.replace("key=werk.stuetze", "key=aux.stuetze"),
            &Bestand::werk(),
            &Geschoss::PROBE,
        );
        assert_eq!(
            texte(&p),
            ["key „aux.stuetze“: „aux“ ist unter Windows ein Gerätename"]
        );
        let lang = format!("key=a.{}", "b".repeat(70));
        let p = pruefen(
            &STUETZE.replace("key=werk.stuetze", &lang),
            &Bestand::werk(),
            &Geschoss::PROBE,
        );
        assert_eq!(p.fehler(), 1);
    }

    #[test]
    fn grenzpruefung() {
        // Bei b = min wird die Breite 0: Fehler nur an der Grenze
        let text = STUETZE.replace("b=b t=d h=h", "b=\"b-200\" t=d h=h");
        let p = pruefen(&text, &Bestand::werk(), &Geschoss::PROBE);
        assert_eq!(
            texte(&p),
            ["Grenzprüfung b=min (200,00): [koerper] Stütze: b, t und h müssen > 0 sein (b=0 t=240 h=2.635)"]
        );
    }

    /// Dickenband (aenderung-dicke.md): `dicke` fehlt, außerhalb mit den
    /// eingestellten Werten oder an einer Grenze, eigene Leistung mit
    /// `dmax`, `dmin` > `dmax`, Werks-Artikel in `stoffe`.
    #[test]
    fn dickenband() {
        const PLATTE: &str = include_str!("../beispiele/werk.bodenplatte.szb");
        const FUND: &str = include_str!("../beispiele/werk.streifenfundament.szb");
        let best = Bestand::werk();
        let p = |t: &str| pruefen(t, &best, &Geschoss::PROBE);
        let l = "1S7bUW0010080200000001";
        let aus = |d: &str| {
            format!("[menge] beton: Dicke {d} mm liegt außerhalb von {l} (180–250 mm), Skizzeo setzt dafür keine Position")
        };
        assert_eq!(
            texte(&p(PLATTE)),
            [
                format!("Grenzprüfung d=min (150,00): {}", aus("150")),
                format!("Grenzprüfung d=max (400,00): {}", aus("400")),
            ]
        );
        // ohne dicke: ein Hinweis, nicht je Grenze
        assert_eq!(
            texte(&p(&PLATTE.replace(" dicke=d", ""))),
            [format!("[menge] beton: Leistung {l} gilt für 180–250 mm, ohne dicke prüft Skizzeo das nicht")]
        );
        // eingestellt außerhalb: die Grenzen derselben Zeile entfallen; der
        // Standardtyp folgt der Vorgabe (Nachtrag 19:40)
        let t = PLATTE.replace("wert=200 min=150", "wert=300 min=150");
        assert_eq!(
            texte(&p(&t))[0],
            "[typ] bp20: Standardtyp weicht von den Vorgaben ab (d=200 statt 300)"
        );
        let t = t.replace("werte=\"d=200\" standard=ja", "werte=\"d=300\" standard=ja");
        assert_eq!(texte(&p(&t)), [aus("300")]);
        assert!(p(&t).einlesbar());
        // eigene Leistung mit dmax
        assert_eq!(
            texte(&p(FUND)),
            ["Grenzprüfung b=max (1.200,00): [menge] beton: Dicke 1.200 mm liegt außerhalb von fundament_beton (bis 600 mm), Skizzeo setzt dafür keine Position"]
        );
        let t = FUND.replace("dmax=600", "dmin=700 dmax=600");
        assert!(
            texte(&p(&t)).contains(&"[leistung] fundament_beton: dmin größer als dmax".to_string())
        );
        let t = FUND.replace("dmax=600", "dmin=x");
        assert!(texte(&p(&t))
            .contains(&"[leistung] fundament_beton dmin: Zahl ≥ 0 (Dezimalpunkt)".to_string()));
        // dicke wie eine Formel geprüft
        let t = FUND.replace("dicke=b", "dicke=q");
        assert!(
            texte(&p(&t)).contains(&"[menge] beton dicke: unbekannter Name „q“".to_string()),
            "{:?}",
            texte(&p(&t))
        );
        // stoffe: Werks-Artikel, unbekannte Kennung, unbekannter key
        let t = FUND.replace(
            "stoffe=\"1S7bUW0010080100000006:1.03\"",
            "stoffe=\"1S7bUW0010080100000006:1.03; 1S7bUW00100801000000ZZ:1; kies:2\"",
        );
        let x = texte(&p(&t));
        assert!(x.contains(&"[leistung] fundament_beton: Artikel „1S7bUW00100801000000ZZ“ nicht im Werksbestand, Skizzeo fragt beim Einlesen nach".to_string()), "{x:?}");
        assert!(x.contains(&"[leistung] fundament_beton: Artikel „kies“ fehlt als [artikel] und ist keine Werks-Kennung".to_string()), "{x:?}");
        assert_eq!(p(&t).fehler(), 1);
    }
}
