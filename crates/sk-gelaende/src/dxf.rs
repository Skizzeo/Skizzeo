//! ASCII-DXF des Vermessers lesen (G1b, plan-gelaendemodul.md §5 G1b).
//! Punkte kommen aus POINT und INSERT. Die Höhe ist Z; auf Z = 0 gilt ein
//! Höhenattribut des Blocks (2D-Plan aus einer DWG oder einzelne Blöcke).
//! Ohne Attribut bleibt ein Punkt auf Z = 0 mit Hinweis außen vor, wenn
//! alle Z = 0 sind oder alle übrigen Höhen weiter als 10 m von 0 liegen;
//! sonst ist ±0,00 eine Höhe wie jede andere. Nummer und
//! Code kommen aus Attributen, die Nummer sonst aus einem Nummerntext
//! daneben, der Code aus dem Layer. 3D-Polylinien sind Bruchkanten,
//! Polylinien mit Erhebung Höhenlinien (Punkte alle höchstens 2 m), 3DFACE
//! und Polyface-Netze geben ihre Ecken und Kanten. Grenzen und Gebäude
//! erkennt Skizzeo am Namen des Layers. Blockinhalte, Bögen, Kreise,
//! Splines und Schraffuren liest Skizzeo nicht, ein Hinweis nennt sie. Wie
//! bei der Punktliste: nur Bytes rein, Punkte, Linien und Hinweise raus.

use std::collections::{HashMap, HashSet};

use crate::lesen::{self, Aufmass, Erkannt, Lage, Punkt, Trenner, Vorgaben};

/// Größter Abstand der Punkte auf einer Höhenlinie (m).
pub const HOEHENLINIE_ABSTAND: f64 = 2.0;
/// Ein Nummerntext steht höchstens so weit neben seinem Punkt (m).
pub const NUMMER_ABSTAND: f64 = 1.0;
/// Ecken einer Linie und Punkte gelten bis hierhin als gleich (m).
const GLEICH: f64 = 0.005;
/// Größter Betrag einer Koordinate in Zeichnungseinheiten. Ein Rechtswert
/// mit Zonenkennzahl in Millimetern hat 3,3·10¹⁰; was darüber liegt, gibt
/// es auf der Erde nicht, und das Objekt bleibt außen vor (Review #150
/// B1). So bleiben alle Rechnungen endlich und jeder Rasterindex im
/// Bereich von `i64`.
pub const MAX_KOORDINATE: f64 = 1e12;
/// Liegen alle übrigen Höhen weiter als so viel von 0 (m), ist Z = 0 ohne
/// Höhenattribut eine fehlende Höhe (NHN: Schriftfeld, Rahmen,
/// Nordpfeil). Bei örtlichen Höhen ist ±0,00 echt.
const Z_NULL_FERN: f64 = 10.0;
/// Höchstens so viele Punkte, in der DXF wie in der Punktliste; mehr
/// entstehen nur aus riesigen Dateien.
pub const MAX_PUNKTE: usize = 2_000_000;

/// Hinweis, wenn mehr als [`MAX_PUNKTE`] Punkte kommen, mit Tausenderpunkt
/// (Bedienbarkeit, später zu Durchgang 43: „2000000“ las sich schwer).
pub const ZU_VIELE: &str = "Mehr als 2.000.000 Punkte; Skizzeo liest nur die ersten.";
/// Höchstens so viele Punkte aus Höhenlinien (500 km Linie bei 2 m).
pub const MAX_HOEHENLINIENPUNKTE: usize = 250_000;

/// Was eine Linie der Datei ist.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Linienart {
    /// 3D-Polylinie oder 3D-Linie auf einem Geländelayer, oder eine
    /// Polylinie im 2D-Plan, deren Ecken alle auf Punkten liegen.
    Bruchkante,
    /// Drei- oder Viereck aus 3DFACE oder einem Polyface-Netz; seine
    /// Kanten halten das Netz der Datei fest.
    Masche,
    /// Umring auf einem Layer, dessen Name Grenze oder Flurstück sagt.
    Grenze,
    /// Linie auf einem Layer, dessen Name Gebäude sagt.
    Gebaeude,
}

/// Eine Linie aus der Datei.
#[derive(Clone, Debug, PartialEq)]
pub struct Linie {
    pub art: Linienart,
    pub ebene: String,
    /// Ecken als Index in [`Aufmass::punkte`] (Bruchkante und Masche).
    pub punkte: Vec<usize>,
    /// Lage der Ecken (Rechts, Hoch in m wie die Punkte).
    pub lage: Vec<[f64; 2]>,
    pub geschlossen: bool,
}

/// Was ein Layer nach seinem Namen ist.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ebenenart {
    Gelaende,
    Grenze,
    Gebaeude,
}

/// Ein Layer der Datei und was darauf liegt (Tabelle im Dialog).
#[derive(Clone, Debug, PartialEq)]
pub struct Ebene {
    pub name: String,
    pub art: Ebenenart,
    /// POINT und INSERT.
    pub punkte: usize,
    pub linien: usize,
    pub texte: usize,
    /// Abgewählt über [`Vorgaben::ebenen_aus`].
    pub aus: bool,
}

/// Was nur eine DXF hat.
#[derive(Clone, Debug, PartialEq)]
pub struct Datei {
    /// `$ACADVER`, z. B. „AC1009“; leer, wenn der Kopf fehlt.
    pub version: String,
    /// Attribut, aus dem Höhen auf Z = 0 kommen.
    pub hoehe: Option<String>,
    /// Höhenlinien, deren Punkte im Aufmaß stehen.
    pub hoehenlinien: usize,
    pub ebenen: Vec<Ebene>,
}

impl Datei {
    /// Version, wie das CAD sie nennt („R12“, „2018“).
    pub fn version_name(&self) -> &str {
        match self.version.as_str() {
            "AC1006" => "R10",
            "AC1009" => "R12",
            "AC1012" => "R13",
            "AC1014" => "R14",
            "AC1015" => "2000",
            "AC1018" => "2004",
            "AC1021" => "2007",
            "AC1024" => "2010",
            "AC1027" => "2013",
            "AC1032" => "2018",
            v => v,
        }
    }
}

/// Großbuchstaben, Umlaute ausgeschrieben, nur Buchstaben und Ziffern.
fn norm(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars().flat_map(char::to_uppercase) {
        match c {
            'Ä' => out.push_str("AE"),
            'Ö' => out.push_str("OE"),
            'Ü' => out.push_str("UE"),
            'ß' => out.push_str("SS"),
            c if c.is_alphanumeric() => out.push(c),
            _ => {}
        }
    }
    out
}

fn ebenenart(name: &str) -> Ebenenart {
    let n = norm(name);
    let hat = |w: &[&str]| w.iter().any(|x| n.contains(x));
    if hat(&["GRENZ", "FLURST", "FLST", "PARCEL", "BOUNDARY"]) {
        Ebenenart::Grenze
    } else if n.starts_with("GEB") || hat(&["GEBAEUDE", "HAUS", "BUILDING"]) {
        Ebenenart::Gebaeude
    } else {
        Ebenenart::Gelaende
    }
}

/// Layer mit Punktnummern als Text.
fn nummernebene(name: &str) -> bool {
    let n = norm(name);
    ["NUMMER", "PNR", "PKTNR", "PUNKTNR", "NUMBER"]
        .iter()
        .any(|w| n.contains(w))
        || n.ends_with("NR")
}

#[derive(Clone, Copy, PartialEq)]
enum Tag {
    Hoehe,
    Nr,
    Code,
}

fn tag(t: &str) -> Option<Tag> {
    Some(match norm(t).as_str() {
        "HOEHE" | "HOE" | "HOHE" | "H" | "Z" | "NN" | "NHN" | "HNN" | "HOEHENN" | "HOEHENHN"
        | "ELEV" | "ELEVATION" | "HEIGHT" => Tag::Hoehe,
        "PNR" | "NR" | "PN" | "PUNKT" | "PUNKTNR" | "PUNKTNUMMER" | "PKTNR" | "PKT" | "NUMMER"
        | "NAME" | "ID" | "POINT" | "POINTNUMBER" => Tag::Nr,
        "CODE" | "KENNUNG" | "PCODE" | "PUNKTCODE" | "CD" | "SY" | "ART" | "KZ" => Tag::Code,
        _ => return None,
    })
}

/// Code aus dem Layer: „PKT_GR“ gibt „GR“, Layer „0“ keinen.
fn code_aus_ebene(e: &str) -> String {
    if e == "0" {
        return String::new();
    }
    for p in [
        "PKT_", "PKT-", "PUNKT_", "PUNKTE_", "HP_", "VP_", "MP_", "DGM_", "P_",
    ] {
        if e.len() > p.len() && e.is_char_boundary(p.len()) && e[..p.len()].eq_ignore_ascii_case(p)
        {
            return e[p.len()..].to_string();
        }
    }
    e.to_string()
}

/// `\U+00F6` im Text wird zum Zeichen.
fn unicode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find("\\U+") {
        out.push_str(&rest[..i]);
        match rest
            .get(i + 3..i + 7)
            .and_then(|h| u32::from_str_radix(h, 16).ok())
            .and_then(char::from_u32)
        {
            Some(c) => {
                out.push(c);
                rest = &rest[i + 7..];
            }
            None => {
                out.push_str("\\U+");
                rest = &rest[i + 3..];
            }
        }
    }
    out.push_str(rest);
    out
}

/// Zahl mit Dezimalpunkt oder -komma.
fn zahl(s: &str) -> Option<f64> {
    lesen::zahl(s, false).or_else(|| lesen::zahl(s, true))
}

/// Ein Objekt der Datei: Typ nach Gruppencode 0 und die Paare bis zum
/// nächsten.
struct Obj<'a> {
    typ: &'a str,
    /// Zeile des Gruppencodes 0, ab 1.
    zeile: usize,
    w: Vec<(i32, &'a str)>,
}

impl<'a> Obj<'a> {
    fn s(&self, c: i32) -> Option<&'a str> {
        self.w.iter().find(|x| x.0 == c).map(|x| x.1)
    }
    fn f(&self, c: i32) -> Option<f64> {
        self.s(c)
            .and_then(|v| v.parse::<f64>().ok())
            .filter(|v| v.is_finite())
    }
    fn i(&self, c: i32) -> i64 {
        self.s(c).and_then(|v| v.parse().ok()).unwrap_or(0)
    }
    /// Punkt aus den Codes 10+k, 20+k, 30+k (Z fehlt = 0).
    fn xyz(&self, k: i32) -> Option<[f64; 3]> {
        Some([
            self.f(10 + k)?,
            self.f(20 + k)?,
            self.f(30 + k).unwrap_or(0.0),
        ])
    }
    /// Eine Koordinate, Erhebung oder Extrusion ist keine endliche Zahl
    /// bis [`MAX_KOORDINATE`].
    fn unmoeglich(&self) -> bool {
        self.w.iter().any(|&(c, v)| {
            matches!(c, 10..=39 | 210..=239)
                && v.parse::<f64>()
                    .is_ok_and(|v| !(v.is_finite() && v.abs() <= MAX_KOORDINATE))
        })
    }
    /// Extrusionsrichtung (210, 220, 230), ohne Angabe +Z.
    fn extrusion(&self) -> [f64; 3] {
        let n = [
            self.f(210).unwrap_or(0.0),
            self.f(220).unwrap_or(0.0),
            self.f(230).unwrap_or(1.0),
        ];
        let l = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
        if l > 1e-12 {
            [n[0] / l, n[1] / l, n[2] / l]
        } else {
            [0.0, 0.0, 1.0]
        }
    }
}

/// Punkt im Objektkoordinatensystem (OCS) mit Extrusion `n` nach WCS,
/// nach dem Arbitrary-Axis-Verfahren der DXF-Referenz. INSERT, TEXT,
/// LWPOLYLINE und 2D-POLYLINE stehen in OCS; gespiegelte Blöcke haben
/// n = (0, 0, −1), dort ist X und Z gegenüber WCS umgekehrt.
fn ocs(n: [f64; 3], p: [f64; 3]) -> [f64; 3] {
    if n == [0.0, 0.0, 1.0] {
        return p;
    }
    let kreuz = |a: [f64; 3], b: [f64; 3]| {
        [
            a[1] * b[2] - a[2] * b[1],
            a[2] * b[0] - a[0] * b[2],
            a[0] * b[1] - a[1] * b[0],
        ]
    };
    let einheit = |a: [f64; 3]| {
        let l = (a[0] * a[0] + a[1] * a[1] + a[2] * a[2]).sqrt();
        [a[0] / l, a[1] / l, a[2] / l]
    };
    let ax = if n[0].abs() < 1.0 / 64.0 && n[1].abs() < 1.0 / 64.0 {
        einheit(kreuz([0.0, 1.0, 0.0], n))
    } else {
        einheit(kreuz([0.0, 0.0, 1.0], n))
    };
    let ay = einheit(kreuz(n, ax));
    [
        p[0] * ax[0] + p[1] * ay[0] + p[2] * n[0],
        p[0] * ax[1] + p[1] * ay[1] + p[2] * n[1],
        p[0] * ax[2] + p[1] * ay[2] + p[2] * n[2],
    ]
}

/// Liegt die Extrusion senkrecht (±Z), ist eine OCS-Ebene in WCS waagrecht.
fn senkrecht(n: [f64; 3]) -> bool {
    n[0].abs() < 1e-9 && n[1].abs() < 1e-9
}

/// Objekte der Datei nacheinander, mit einem Objekt Vorschau.
struct Objekte<'a> {
    /// Zeilen an \r\n, \n und einzelnem \r getrennt (Review #150).
    zeilen: Box<dyn Iterator<Item = &'a str> + 'a>,
    nr: usize,
    /// Nächstes Paar, schon gelesen.
    paar: Option<(i32, &'a str, usize)>,
    vor: Option<Obj<'a>>,
    /// Zeile, an der kein Gruppencode steht.
    fehler: Option<usize>,
}

impl<'a> Objekte<'a> {
    fn neu(s: &'a str) -> Objekte<'a> {
        let mut o = Objekte {
            zeilen: Box::new(lesen::zeilen(s)),
            nr: 0,
            paar: None,
            vor: None,
            fehler: None,
        };
        o.paar = o.lies_paar();
        o
    }

    fn lies_paar(&mut self) -> Option<(i32, &'a str, usize)> {
        if self.fehler.is_some() {
            return None;
        }
        let c = self.zeilen.next()?.trim();
        self.nr += 1;
        let zeile = self.nr;
        let w = self.zeilen.next();
        self.nr += 1;
        match (c.parse::<i32>(), w) {
            (Ok(code), Some(w)) => Some((code, w.trim(), zeile)),
            _ if c.is_empty() && w.is_none_or(|w| w.trim().is_empty()) => None,
            _ => {
                self.fehler = Some(zeile);
                None
            }
        }
    }

    fn lies_obj(&mut self) -> Option<Obj<'a>> {
        // bis zum nächsten Gruppencode 0
        while let Some((c, _, _)) = self.paar {
            if c == 0 {
                break;
            }
            self.paar = self.lies_paar();
        }
        let (_, typ, zeile) = self.paar?;
        let mut w = Vec::new();
        loop {
            self.paar = self.lies_paar();
            match self.paar {
                Some((c, v, _)) if c != 0 => w.push((c, v)),
                _ => break,
            }
        }
        Some(Obj { typ, zeile, w })
    }

    fn naechstes(&mut self) -> Option<Obj<'a>> {
        self.vor.take().or_else(|| self.lies_obj())
    }

    /// Nächstes Objekt, wenn es vom Typ `typ` ist.
    fn naechstes_wenn(&mut self, typ: &str) -> Option<Obj<'a>> {
        if self.vor.is_none() {
            self.vor = self.lies_obj();
        }
        if self.vor.as_ref()?.typ == typ {
            self.vor.take()
        } else {
            None
        }
    }

    /// Folgeobjekte vom Typ `typ` (ATTRIB, VERTEX) bis SEQEND.
    fn folgende(&mut self, typ: &str) -> Vec<Obj<'a>> {
        let mut v = Vec::new();
        while let Some(f) = self.naechstes_wenn(typ) {
            v.push(f);
        }
        self.naechstes_wenn("SEQEND");
        v
    }
}

/// Kopfvariablen der Datei.
struct Kopf {
    version: String,
    codepage: Option<String>,
    einheiten: i64,
}

fn kopf(s: &str) -> Kopf {
    let mut k = Kopf {
        version: String::new(),
        codepage: None,
        einheiten: 0,
    };
    let mut o = Objekte::neu(s);
    while let Some(x) = o.naechstes() {
        if x.typ != "SECTION" {
            continue;
        }
        if x.s(2) == Some("HEADER") {
            let mut name = "";
            for &(c, v) in &x.w {
                match (c, name) {
                    (9, _) => name = v,
                    (_, "$ACADVER") => k.version = v.to_string(),
                    (_, "$DWGCODEPAGE") => k.codepage = Some(v.to_string()),
                    (_, "$INSUNITS") => k.einheiten = v.parse().unwrap_or(0),
                    _ => {}
                }
            }
        }
        break;
    }
    k
}

/// Ein Punkt der Datei, bevor feststeht, woher die Höhe kommt.
struct Kandidat {
    p: [f64; 3],
    /// Höhe aus einem Attribut und dessen Name.
    attribut: Option<(f64, String)>,
    nr: Option<String>,
    code: Option<String>,
    zeile: usize,
}

#[derive(Clone, Copy, PartialEq)]
enum Form {
    /// Ecken mit eigener Höhe (3D-Polylinie, Linie).
    Ecken,
    /// Polylinie in einer Höhe (Erhebung).
    Eben(f64),
    /// Fläche eines Netzes.
    Masche,
    /// Nur die Ecken (Maschennetz ohne Flächen).
    Punkte,
}

struct LinieRoh {
    ebene: String,
    art: Ebenenart,
    form: Form,
    ecken: Vec<[f64; 3]>,
    geschlossen: bool,
}

/// Raster für die Suche nach nahen Punkten.
struct Raster {
    zelle: f64,
    z: HashMap<(i64, i64), Vec<usize>>,
}

impl Raster {
    fn neu(zelle: f64) -> Raster {
        Raster {
            zelle,
            z: HashMap::new(),
        }
    }
    fn k(&self, x: f64, y: f64) -> (i64, i64) {
        // `as` kappt auf den Bereich von i64, `nahe` addiert sättigend
        (
            (x / self.zelle).floor() as i64,
            (y / self.zelle).floor() as i64,
        )
    }
    fn rein(&mut self, x: f64, y: f64, i: usize) {
        let k = self.k(x, y);
        self.z.entry(k).or_default().push(i);
    }
    fn nahe(&self, x: f64, y: f64) -> impl Iterator<Item = usize> + '_ {
        let (a, b) = self.k(x, y);
        (-1..=1)
            .flat_map(move |i| (-1..=1).map(move |j| (a.saturating_add(i), b.saturating_add(j))))
            .filter_map(|k| self.z.get(&k))
            .flatten()
            .copied()
    }
}

/// Was beim Durchgang durch die Objekte gesammelt wird.
#[derive(Default)]
struct Inhalt {
    ebenen: Vec<Ebene>,
    kandidaten: Vec<Kandidat>,
    texte: Vec<([f64; 2], String)>,
    linien: Vec<LinieRoh>,
    /// Objekte mit unmöglichen Koordinaten (Review #150 B1).
    unmoeglich: usize,
    /// Bögen, Kreise, Ellipsen, Splines und Schraffuren.
    kurven: usize,
    /// Polylinien mit Bögen (Gruppencode 42).
    boegen: usize,
    /// Blockreihen (INSERT mit mehr als einer Spalte oder Zeile).
    reihen: usize,
    /// Blöcke mit 3D-Punkten, 3D-Linien oder Flächen im Inhalt.
    bloecke_3d: HashSet<String>,
    /// Blöcke, die in den Objekten eingefügt sind.
    eingefuegt: HashSet<String>,
}

impl Inhalt {
    /// Hinweise zu dem, was Skizzeo aus der Datei nicht liest.
    fn hinweise(&self) -> Vec<String> {
        let mut h = Vec::new();
        if self.unmoeglich > 0 {
            h.push(match self.unmoeglich {
                1 => "1 Objekt mit unmöglichen Koordinaten bleibt außen vor.".into(),
                n => format!("{n} Objekte mit unmöglichen Koordinaten bleiben außen vor."),
            });
        }
        let bloecke = self.bloecke_3d.intersection(&self.eingefuegt).count();
        if bloecke > 0 {
            h.push(format!(
                "{} 3D-Punkte, 3D-Linien oder Flächen; Skizzeo liest von einem Block nur den Einfügepunkt. Bitte Blöcke mit Geländedaten im CAD auflösen.",
                match bloecke {
                    1 => "1 eingefügter Block enthält".to_string(),
                    n => format!("{n} eingefügte Blöcke enthalten"),
                }
            ));
        }
        if self.reihen > 0 {
            h.push(match self.reihen {
                1 => "1 Blockreihe (Spalten oder Zeilen) zählt nur mit ihrem ersten Block.".into(),
                n => format!(
                    "{n} Blockreihen (Spalten oder Zeilen) zählen nur mit ihrem ersten Block."
                ),
            });
        }
        let mut s = Vec::new();
        if self.kurven > 0 {
            s.push(format!(
                "Bögen, Kreise, Splines und Schraffuren liest Skizzeo nicht ({} in der Datei).",
                self.kurven
            ));
        }
        if self.boegen > 0 {
            s.push(match self.boegen {
                1 => "1 Polylinie mit Bögen gilt mit geraden Kanten.".into(),
                n => format!("{n} Polylinien mit Bögen gelten mit geraden Kanten."),
            });
        }
        if !s.is_empty() {
            h.push(s.join(" "));
        }
        h
    }
}

/// Liegt in einem Block etwas, das Geländedaten sein kann: 3D-Punkte,
/// 3D-Linien, 3D-Polylinien, Netze oder Flächen.
fn gelaende_im_block(x: &Obj) -> bool {
    let hoch = |k: i32| x.f(30 + k).is_some_and(|z| z != 0.0);
    match x.typ {
        "3DFACE" => true,
        "POLYLINE" => x.i(70) & (8 | 16 | 64) != 0,
        "POINT" => hoch(0),
        "LINE" => hoch(0) || hoch(1),
        _ => false,
    }
}

/// Liest Skizzeo diesen Objekttyp.
fn gelesen(typ: &str) -> bool {
    matches!(
        typ,
        "POINT" | "INSERT" | "TEXT" | "MTEXT" | "POLYLINE" | "LWPOLYLINE" | "LINE" | "3DFACE"
    )
}

/// Objekte der Datei sammeln; abgewählte Layer zählen nur in der Tabelle.
fn sammeln(o: &mut Objekte, aus: &[String]) -> Inhalt {
    let mut c = Inhalt::default();
    let mut ebene_nr: HashMap<String, usize> = HashMap::new();
    let mut abschnitt = String::new();
    let mut block = String::new();
    while let Some(x) = o.naechstes() {
        match x.typ {
            "SECTION" => {
                abschnitt = x.s(2).unwrap_or("").to_string();
                continue;
            }
            "ENDSEC" => {
                abschnitt.clear();
                continue;
            }
            "EOF" => break,
            // Blockinhalte liest Skizzeo nicht, merkt sich aber, welche
            // Blöcke Geländedaten tragen
            "BLOCK" if abschnitt == "BLOCKS" => {
                block = unicode(x.s(2).unwrap_or(""));
                continue;
            }
            "ENDBLK" => {
                block.clear();
                continue;
            }
            _ if abschnitt == "BLOCKS" => {
                if !block.is_empty() && !block.starts_with('*') && gelaende_im_block(&x) {
                    c.bloecke_3d.insert(block.clone());
                }
                continue;
            }
            _ if abschnitt != "ENTITIES" => continue,
            _ => {}
        }
        // Folgeobjekte gehören zu ihrem INSERT oder ihrer POLYLINE
        let folge = match x.typ {
            "INSERT" if x.i(66) == 1 => o.folgende("ATTRIB"),
            "POLYLINE" => o.folgende("VERTEX"),
            _ => Vec::new(),
        };
        let name = unicode(x.s(8).unwrap_or("0"));
        let ei = *ebene_nr.entry(name.clone()).or_insert_with(|| {
            c.ebenen.push(Ebene {
                name: name.clone(),
                art: ebenenart(&name),
                punkte: 0,
                linien: 0,
                texte: 0,
                aus: aus.contains(&name),
            });
            c.ebenen.len() - 1
        });
        if !c.ebenen[ei].aus {
            if gelesen(x.typ) && (x.unmoeglich() || folge.iter().any(Obj::unmoeglich)) {
                c.unmoeglich += 1;
                continue;
            }
            let bogen = |o: &Obj| {
                o.w.iter()
                    .any(|p| p.0 == 42 && p.1.parse::<f64>().is_ok_and(|b| b != 0.0))
            };
            match x.typ {
                "ARC" | "CIRCLE" | "ELLIPSE" | "SPLINE" | "HATCH" => c.kurven += 1,
                "LWPOLYLINE" if bogen(&x) => c.boegen += 1,
                "POLYLINE" if folge.iter().any(bogen) => c.boegen += 1,
                "INSERT" => {
                    c.eingefuegt.insert(unicode(x.s(2).unwrap_or("")));
                    c.reihen += usize::from(x.i(70) > 1 || x.i(71) > 1);
                }
                _ => {}
            }
        }
        let e = &mut c.ebenen[ei];
        let art = e.art;
        let linie = |form: Form, ecken: Vec<[f64; 3]>, geschlossen: bool| LinieRoh {
            ebene: name.clone(),
            art,
            form,
            ecken,
            geschlossen,
        };
        match x.typ {
            "POINT" | "INSERT" => {
                e.punkte += 1;
                let Some(p) = x.xyz(0).filter(|_| !e.aus) else {
                    continue;
                };
                let p = if x.typ == "INSERT" {
                    ocs(x.extrusion(), p)
                } else {
                    p
                };
                let mut k = Kandidat {
                    p,
                    attribut: None,
                    nr: None,
                    code: None,
                    zeile: x.zeile,
                };
                for a in &folge {
                    let (Some(t), Some(w)) = (a.s(2), a.s(1)) else {
                        continue;
                    };
                    let w = unicode(w).trim().to_string();
                    match tag(t) {
                        Some(Tag::Hoehe) if k.attribut.is_none() => {
                            k.attribut = zahl(&w)
                                .filter(|h| h.is_finite() && h.abs() <= MAX_KOORDINATE)
                                .map(|h| (h, t.to_string()));
                        }
                        Some(Tag::Nr) if k.nr.is_none() && !w.is_empty() => k.nr = Some(w),
                        Some(Tag::Code) if k.code.is_none() => k.code = Some(w),
                        _ => {}
                    }
                }
                if k.code.is_none() {
                    k.code = Some(code_aus_ebene(&name));
                }
                c.kandidaten.push(k);
            }
            "TEXT" | "MTEXT" => {
                e.texte += 1;
                if let (false, Some(p), Some(s)) = (e.aus, x.xyz(0), x.s(1)) {
                    let p = if x.typ == "TEXT" {
                        ocs(x.extrusion(), p)
                    } else {
                        p
                    };
                    if nummernebene(&name) {
                        c.texte.push(([p[0], p[1]], unicode(s).trim().to_string()));
                    }
                }
            }
            "POLYLINE" => {
                e.linien += 1;
                if e.aus {
                    continue;
                }
                let flags = x.i(70);
                if flags & 64 != 0 {
                    // Polyface: Ecken mit Kennung 64, Flächen mit 128 und
                    // Eckennummern ab 1 in 71 bis 74 (negativ: Kante unsichtbar)
                    let pos: Vec<[f64; 3]> = folge
                        .iter()
                        .filter(|v| v.i(70) & 64 != 0)
                        .filter_map(|v| v.xyz(0))
                        .collect();
                    for f in folge.iter().filter(|v| v.i(70) & 192 == 128) {
                        let ecken: Vec<[f64; 3]> = (71..=74)
                            .map(|g| f.i(g).unsigned_abs() as usize)
                            .filter(|&i| (1..=pos.len()).contains(&i))
                            .map(|i| pos[i - 1])
                            .collect();
                        c.linien.push(linie(Form::Masche, ecken, true));
                    }
                    continue;
                }
                let mut ecken: Vec<[f64; 3]> = folge.iter().filter_map(|v| v.xyz(0)).collect();
                // 8: 3D-Polylinie, 16: Maschennetz (nur seine Ecken zählen),
                // sonst 2D in der Erhebung des Kopfs
                let form = match flags {
                    f if f & 16 != 0 => Form::Punkte,
                    f if f & 8 != 0 => Form::Ecken,
                    _ => {
                        let n = x.extrusion();
                        let h = x.f(30).unwrap_or(0.0);
                        for p in &mut ecken {
                            *p = ocs(n, [p[0], p[1], h]);
                        }
                        if senkrecht(n) {
                            Form::Eben(h * n[2])
                        } else {
                            Form::Ecken
                        }
                    }
                };
                c.linien.push(linie(form, ecken, flags & 1 != 0));
            }
            "LWPOLYLINE" => {
                e.linien += 1;
                if e.aus {
                    continue;
                }
                let wert = |g: i32| {
                    x.w.iter()
                        .filter(move |p| p.0 == g)
                        .map(|p| p.1.parse::<f64>().ok().filter(|v| v.is_finite()))
                };
                let n = x.extrusion();
                let hoehe = x.f(38).unwrap_or(0.0);
                let ecken: Vec<[f64; 3]> = wert(10)
                    .zip(wert(20))
                    .filter_map(|(a, b)| Some(ocs(n, [a?, b?, hoehe])))
                    .collect();
                let form = if senkrecht(n) {
                    Form::Eben(hoehe * n[2])
                } else {
                    Form::Ecken
                };
                c.linien.push(linie(form, ecken, x.i(70) & 1 != 0));
            }
            "LINE" => {
                e.linien += 1;
                if let (false, Some(a), Some(b)) = (e.aus, x.xyz(0), x.xyz(1)) {
                    c.linien.push(linie(Form::Ecken, vec![a, b], false));
                }
            }
            "3DFACE" => {
                e.linien += 1;
                if e.aus {
                    continue;
                }
                let mut ecken: Vec<[f64; 3]> = (0..4).filter_map(|k| x.xyz(k)).collect();
                if ecken.len() == 4 && ecken[3] == ecken[2] {
                    ecken.pop();
                }
                c.linien.push(linie(Form::Masche, ecken, true));
            }
            _ => {}
        }
    }
    c
}

/// Meter je Zeichnungseinheit nach `$INSUNITS` und der Satz dazu. Ohne
/// Angabe (0) und in Metern (6) kein Satz; eine unbekannte Einheit liest
/// Skizzeo als Meter und sagt es.
fn einheit(insunits: i64) -> (f64, Option<String>) {
    let (f, name) = match insunits {
        0 | 6 => return (1.0, None),
        1 => (0.0254, "Zoll"),
        2 => (0.3048, "Fuß"),
        4 => (0.001, "Millimetern"),
        5 => (0.01, "Zentimetern"),
        7 => (1000.0, "Kilometern"),
        14 => (0.1, "Dezimetern"),
        n => {
            return (
                1.0,
                Some(format!(
                    "Die DXF nennt die Einheit {n} ($INSUNITS), die Skizzeo nicht kennt; gelesen ist in Metern. Die Einheit lässt sich unter „Einstellungen der Datei“ umstellen."
                )),
            )
        }
    };
    (
        f,
        Some(format!(
            "Die DXF ist in {name} gezeichnet; Skizzeo rechnet in Meter um."
        )),
    )
}

/// Umringe aus Strecken, an den Enden auf mm verbunden; offene Ketten
/// fallen weg.
fn ringe(strecken: &[([f64; 2], [f64; 2])]) -> (Vec<Vec<[f64; 2]>>, usize) {
    let key = |p: [f64; 2]| {
        (
            (p[0] * 1000.0).round() as i64,
            (p[1] * 1000.0).round() as i64,
        )
    };
    let mut an: HashMap<(i64, i64), Vec<usize>> = HashMap::new();
    for (i, (a, b)) in strecken.iter().enumerate() {
        if key(*a) != key(*b) {
            an.entry(key(*a)).or_default().push(i);
            an.entry(key(*b)).or_default().push(i);
        }
    }
    let mut frei: Vec<bool> = strecken.iter().map(|(a, b)| key(*a) != key(*b)).collect();
    let (mut ringe, mut offen) = (Vec::new(), 0);
    for s in 0..strecken.len() {
        if !frei[s] {
            continue;
        }
        frei[s] = false;
        let (a, mut jetzt) = strecken[s];
        let mut ring = vec![a, jetzt];
        let geschlossen = loop {
            if key(jetzt) == key(a) {
                break true;
            }
            let Some(&t) = an
                .get(&key(jetzt))
                .and_then(|v| v.iter().find(|&&t| frei[t]))
            else {
                break false;
            };
            frei[t] = false;
            let (p, q) = strecken[t];
            jetzt = if key(p) == key(jetzt) { q } else { p };
            ring.push(jetzt);
        };
        if geschlossen {
            ring.pop();
        }
        if geschlossen && ring.len() >= 3 {
            ringe.push(ring);
        } else {
            offen += 1;
        }
    }
    (ringe, offen)
}

/// DXF lesen; aus `v` gelten Zeichensatz, Einheit und abgewählte Layer.
pub fn lesen(roh: &[u8], v: &Vorgaben) -> Result<Aufmass, String> {
    // UTF-8-BOM vor dem ersten Gruppencode (Review #150 B4)
    let roh = roh.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(roh);
    if roh.starts_with(b"AutoCAD Binary DXF") {
        return Err(
            "Das ist eine Binär-DXF. Bitte im CAD als ASCII-DXF speichern und diese einlesen."
                .into(),
        );
    }
    let k = kopf(&String::from_utf8_lossy(&roh[..roh.len().min(1 << 18)]));
    let (faktor, satz) = match v.einheit {
        Some(f) => (f, None),
        None => einheit(k.einheiten),
    };
    let (inhalt, zeichensatz) = lesen::text(roh, v.zeichensatz);
    let mut objekte = Objekte::neu(&inhalt);
    let c = sammeln(&mut objekte, &v.ebenen_aus);
    let mut hinweise = Vec::new();
    if let Some(z) = objekte.fehler {
        hinweise.push(format!(
            "Die DXF bricht an Zeile {z} ab; gelesen ist, was davor steht."
        ));
    }
    if zeichensatz == lesen::Zeichensatz::Windows1252
        && k.codepage
            .as_deref()
            .is_some_and(|cp| !cp.eq_ignore_ascii_case("ANSI_1252"))
    {
        hinweise.push(format!(
            "Die DXF nennt den Zeichensatz {}; Skizzeo liest Windows-1252, Umlaute in Texten können falsch aussehen.",
            k.codepage.as_deref().unwrap_or_default()
        ));
    }
    hinweise.extend(satz);
    hinweise.extend(c.hinweise());
    let m = |p: [f64; 3]| [p[0] * faktor, p[1] * faktor, p[2] * faktor];

    // Höhe je Punkt (Review #150 B2): Z, wenn es nicht 0 ist, auch bei
    // Blöcken ohne Attribute; auf Z = 0 ein Höhenattribut. Ohne Attribut
    // fehlt die Höhe, wenn alle Z = 0 sind (2D-Plan) oder alle übrigen
    // Höhen fern von 0 liegen (NHN); sonst ist Z = 0 die Höhe ±0,00.
    let z_null = c.kandidaten.iter().all(|k| k.p[2] == 0.0);
    let fern = c.kandidaten.iter().all(|k| match &k.attribut {
        _ if k.p[2] != 0.0 => (k.p[2] * faktor).abs() > Z_NULL_FERN,
        Some((h, _)) => h.abs() > Z_NULL_FERN,
        None => true,
    });
    let mut hoehe_aus: Option<String> = None;
    let (mut ohne, mut auf_null) = (0usize, 0usize);
    let mut punkte: Vec<Punkt> = Vec::new();
    let mut nr_fehlt: Vec<usize> = Vec::new();
    for k in &c.kandidaten {
        let p = m(k.p);
        let h = if k.p[2] != 0.0 {
            p[2]
        } else if let Some((h, t)) = &k.attribut {
            hoehe_aus.get_or_insert_with(|| t.clone());
            *h
        } else if z_null || fern {
            ohne += 1;
            continue;
        } else {
            auf_null += 1;
            0.0
        };
        if k.nr.is_none() {
            nr_fehlt.push(punkte.len());
        }
        punkte.push(Punkt {
            nr: k.nr.clone().unwrap_or_default(),
            e: p[0],
            n: p[1],
            h,
            code: k.code.clone().unwrap_or_default(),
            text: String::new(),
            zeile: k.zeile,
        });
    }
    if ohne > 0 {
        hinweise.push(match (ohne, z_null) {
            (1, true) => "1 Punkt ohne Höhe (Z = 0, kein Höhenattribut) bleibt außen vor.".into(),
            (n, true) => format!("{n} Punkte ohne Höhe (Z = 0, kein Höhenattribut) bleiben außen vor."),
            (1, false) => "1 Punkt auf Z = 0 zwischen Punkten mit Höhe, ohne Höhenattribut, bleibt außen vor.".into(),
            (n, false) => format!("{n} Punkte auf Z = 0 zwischen Punkten mit Höhe, ohne Höhenattribut, bleiben außen vor."),
        });
    }
    if auf_null > 0 {
        hinweise.push(match auf_null {
            1 => {
                "1 Punkt genau auf 0,000 (Z = 0, kein Höhenattribut) zählt mit dieser Höhe.".into()
            }
            n => format!(
                "{n} Punkte genau auf 0,000 (Z = 0, kein Höhenattribut) zählen mit dieser Höhe."
            ),
        });
    }
    nummern(&mut punkte, &nr_fehlt, &c.texte, faktor);

    // Linien: Bruchkanten an die Punkte, Höhenlinien zu Punkten, Grenzen zu Umringen
    let mut raster = Raster::neu(GLEICH * 2.0);
    for (i, p) in punkte.iter().enumerate() {
        raster.rein(p.e, p.n, i);
    }
    let mut linien: Vec<Linie> = Vec::new();
    let mut grenze: Vec<([f64; 2], [f64; 2])> = Vec::new();
    let mut grenzringe: Vec<(String, Vec<[f64; 2]>)> = Vec::new();
    let (mut hoehenlinien, mut verworfen, mut zu_viele) = (0usize, 0usize, false);
    // Punkte aus Höhenlinien und ob deren Grenze erreicht ist (Review #150)
    let (mut aus_linien, mut zu_viele_hl) = (0usize, false);
    let (mut neu_l, mut neu_n) = (0usize, 0usize);
    for l in &c.linien {
        let ecken: Vec<[f64; 3]> = l.ecken.iter().map(|&p| m(p)).collect();
        match l.art {
            Ebenenart::Grenze => {
                let r: Vec<[f64; 2]> = ecken.iter().map(|p| [p[0], p[1]]).collect();
                if l.geschlossen && r.len() >= 3 {
                    grenzringe.push((l.ebene.clone(), r));
                } else {
                    grenze.extend(r.windows(2).map(|w| (w[0], w[1])));
                }
                continue;
            }
            Ebenenart::Gebaeude => {
                linien.push(Linie {
                    art: Linienart::Gebaeude,
                    ebene: l.ebene.clone(),
                    punkte: Vec::new(),
                    lage: ecken.iter().map(|p| [p[0], p[1]]).collect(),
                    geschlossen: l.geschlossen,
                });
                continue;
            }
            Ebenenart::Gelaende => {}
        }
        match l.form {
            Form::Eben(h) if h != 0.0 => {
                // Höhenlinie: Punkte alle höchstens 2 m
                let h = h * faktor;
                hoehenlinien += 1;
                let mut ring = ecken.clone();
                if l.geschlossen && ring.len() > 2 {
                    ring.push(ring[0]);
                }
                let mut lage: Vec<[f64; 2]> = Vec::new();
                let mut abgeschnitten = false;
                for w in ring.windows(2) {
                    let d = (w[1][0] - w[0][0]).hypot(w[1][1] - w[0][1]);
                    let schritte = (d / HOEHENLINIE_ABSTAND).ceil().max(1.0);
                    let frei = MAX_PUNKTE.saturating_sub(punkte.len() + lage.len());
                    let frei_hl = MAX_HOEHENLINIENPUNKTE.saturating_sub(aus_linien + lage.len());
                    let nimm = (schritte as usize).min(frei).min(frei_hl);
                    lage.extend((0..nimm).map(|s| {
                        let t = s as f64 / schritte;
                        [
                            w[0][0] + t * (w[1][0] - w[0][0]),
                            w[0][1] + t * (w[1][1] - w[0][1]),
                        ]
                    }));
                    if nimm < schritte as usize {
                        if frei_hl < frei {
                            zu_viele_hl = true;
                        } else {
                            zu_viele = true;
                        }
                        abgeschnitten = true;
                        break;
                    }
                }
                if let (Some(q), false, false) = (ring.last(), l.geschlossen, abgeschnitten) {
                    lage.push([q[0], q[1]]);
                }
                aus_linien += lage.len();
                for (k, q) in lage.into_iter().enumerate() {
                    punkte.push(Punkt {
                        nr: format!("H{hoehenlinien}-{}", k + 1),
                        e: q[0],
                        n: q[1],
                        h,
                        code: String::new(),
                        text: l.ebene.clone(),
                        zeile: 0,
                    });
                }
            }
            Form::Punkte => {
                for p in &ecken {
                    if punkte.len() >= MAX_PUNKTE {
                        zu_viele = true;
                        break;
                    }
                    neu_n += 1;
                    raster.rein(p[0], p[1], punkte.len());
                    punkte.push(Punkt {
                        nr: format!("N{neu_n}"),
                        e: p[0],
                        n: p[1],
                        h: p[2],
                        code: String::new(),
                        text: l.ebene.clone(),
                        zeile: 0,
                    });
                }
            }
            Form::Eben(_) | Form::Ecken | Form::Masche => {
                let masche = l.form == Form::Masche;
                let drei_d = l.form != Form::Eben(0.0) && ecken.iter().any(|p| p[2] != 0.0);
                // eine 2D-Linie zählt nur im 2D-Plan, und nur, wenn alle
                // Ecken auf Punkten liegen
                let nur_lage = !drei_d;
                if nur_lage && (masche || !z_null) {
                    continue;
                }
                let mut idx = Vec::with_capacity(ecken.len());
                for p in &ecken {
                    let gefunden = raster.nahe(p[0], p[1]).find(|&i| {
                        let q = &punkte[i];
                        (q.e - p[0]).abs() <= GLEICH
                            && (q.n - p[1]).abs() <= GLEICH
                            && (nur_lage || (q.h - p[2]).abs() <= GLEICH)
                    });
                    match gefunden {
                        Some(i) => idx.push(i),
                        None if nur_lage => {
                            idx.clear();
                            break;
                        }
                        None if punkte.len() >= MAX_PUNKTE => zu_viele = true,
                        None => {
                            let i = punkte.len();
                            let nr = if masche {
                                neu_n += 1;
                                format!("N{neu_n}")
                            } else {
                                format!("L{}-{}", neu_l + 1, idx.len() + 1)
                            };
                            punkte.push(Punkt {
                                nr,
                                e: p[0],
                                n: p[1],
                                h: p[2],
                                code: String::new(),
                                text: l.ebene.clone(),
                                zeile: 0,
                            });
                            raster.rein(p[0], p[1], i);
                            idx.push(i);
                        }
                    }
                }
                idx.dedup();
                if l.geschlossen && idx.len() > 1 && idx.first() == idx.last() {
                    idx.pop();
                }
                if idx.len() < 2 {
                    verworfen += usize::from(!ecken.is_empty() && !nur_lage);
                    continue;
                }
                if !masche {
                    neu_l += 1;
                }
                linien.push(Linie {
                    art: if masche {
                        Linienart::Masche
                    } else {
                        Linienart::Bruchkante
                    },
                    ebene: l.ebene.clone(),
                    lage: idx.iter().map(|&i| [punkte[i].e, punkte[i].n]).collect(),
                    punkte: idx,
                    geschlossen: l.geschlossen || masche,
                });
            }
        }
    }
    if zu_viele {
        hinweise.push(ZU_VIELE.to_string());
    }
    if zu_viele_hl {
        hinweise.push(format!(
            "Die Höhenlinien geben mehr als {MAX_HOEHENLINIENPUNKTE} Punkte; Skizzeo liest nur die ersten."
        ));
    }
    if verworfen > 0 {
        hinweise.push(format!(
            "{verworfen} Linien ohne zwei verschiedene Ecken bleiben außen vor."
        ));
    }
    let (ketten, offen) = ringe(&grenze);
    grenzringe.extend(ketten.into_iter().map(|r| (String::new(), r)));
    if offen > 0 {
        hinweise
            .push("Eine Grenzlinie schließt sich nicht zum Umring und bleibt außen vor.".into());
    }
    if punkte.is_empty() {
        let texte = c.ebenen.iter().any(|e| e.texte > 0);
        return Err(if z_null && !c.kandidaten.is_empty() && texte {
            "Die Punkte der DXF haben keine Höhe (Z = 0, kein Höhenattribut); Höhen stehen hier nur als Text daneben. Bitte beim Vermesser 3D-Punkte, Blöcke mit Höhenattribut oder die Punktliste anfordern."
        } else {
            "In der DXF stehen keine Punkte mit Höhe (POINT oder Block mit Z oder Höhenattribut, 3D-Polylinien, Höhenlinien, 3D-Flächen)."
        }
        .into());
    }

    // Lagebezug wie bei der Punktliste; die Zonenkennzahl fällt weg
    let bereich = |f: fn(&Punkt) -> f64| {
        punkte
            .iter()
            .map(f)
            .fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), v| {
                (lo.min(v), hi.max(v))
            })
    };
    let (e0, e1) = bereich(|p| p.e);
    let (n0, n1) = bereich(|p| p.n);
    let (h0, h1) = bereich(|p| p.h);
    let lage = match (lesen::art(e0, e1), lesen::art(n0, n1)) {
        (lesen::Art::UtmZone(z), lesen::Art::Hoch) => Lage::Utm { zone: Some(z) },
        (lesen::Art::Utm, lesen::Art::Hoch) => Lage::Utm { zone: None },
        (lesen::Art::GaussKrueger(s), lesen::Art::Hoch) => Lage::GaussKrueger { streifen: s },
        (lesen::Art::Hoch, lesen::Art::Hoch) => Lage::GaussKrueger { streifen: 5 },
        _ => Lage::Oertlich,
    };
    if let Lage::Utm { zone: Some(z) } = lage {
        let z0 = z as f64 * 1e6;
        for p in &mut punkte {
            p.e -= z0;
        }
        for l in &mut linien {
            for q in &mut l.lage {
                q[0] -= z0;
            }
        }
        for (_, r) in &mut grenzringe {
            for q in r {
                q[0] -= z0;
            }
        }
    }
    if let Lage::GaussKrueger { .. } = lage {
        hinweise.push(lesen::GAUSS_KRUEGER.into());
    }
    // ohne Einheit wie bei der Punktliste: Millimeter, wenn Lage und Höhe
    // über 10.000 liegen (Review #150)
    let gross = |lo: f64, hi: f64| lo.abs().max(hi.abs()) > 10_000.0;
    let millimeter = k.einheiten == 0
        && v.einheit.is_none()
        && lage == Lage::Oertlich
        && gross(h0, h1)
        && (gross(e0, e1) || gross(n0, n1));
    if millimeter {
        hinweise.push(lesen::MILLIMETER.into());
    }
    linien.extend(grenzringe.into_iter().map(|(ebene, lage)| Linie {
        art: Linienart::Grenze,
        ebene,
        punkte: Vec::new(),
        lage,
        geschlossen: true,
    }));
    Ok(Aufmass {
        punkte,
        linien,
        erkannt: Erkannt {
            zeichensatz,
            trenner: Trenner::Leerzeichen,
            komma: false,
            kopf: None,
            spalten: Vec::new(),
            lage,
            zweifel: false,
            millimeter,
        },
        hinweise,
        dxf: Some(Datei {
            version: k.version,
            hoehe: hoehe_aus,
            hoehenlinien,
            ebenen: c.ebenen,
        }),
    })
}

/// Nummern für Punkte ohne Nummernattribut. Vermessungsprogramme setzen
/// den Nummerntext mit festem Versatz neben den Punkt; dieser Versatz (der
/// häufigste zum nächsten Text, auf cm) entscheidet zuerst, auch wenn ein
/// Nachbarpunkt dem Text näher liegt. Übrige Punkte bekommen den nächsten
/// freien Text bis [`NUMMER_ABSTAND`], die nächsten Paare zuerst; sonst die
/// laufende Nummer. Jeder Text gilt einmal.
fn nummern(punkte: &mut [Punkt], fehlt: &[usize], texte: &[([f64; 2], String)], faktor: f64) {
    let r = NUMMER_ABSTAND;
    let mut raster = Raster::neu(r);
    let texte: Vec<([f64; 2], &str)> = texte
        .iter()
        .filter(|(_, s)| !s.is_empty() && s.len() <= 16 && !s.contains(char::is_whitespace))
        .map(|(p, s)| ([p[0] * faktor, p[1] * faktor], s.as_str()))
        .collect();
    for (i, (p, _)) in texte.iter().enumerate() {
        raster.rein(p[0], p[1], i);
    }
    let abstand = |i: usize, t: usize, v: [f64; 2]| {
        let p = &punkte[i];
        (texte[t].0[0] - p.e - v[0]).hypot(texte[t].0[1] - p.n - v[1])
    };
    // häufigster Versatz zum nächsten Text
    let mut versatz: HashMap<(i64, i64), usize> = HashMap::new();
    for &i in fehlt {
        let p = &punkte[i];
        let naechster = raster
            .nahe(p.e, p.n)
            .map(|t| (abstand(i, t, [0.0; 2]), t))
            .filter(|x| x.0 <= r)
            .min_by(|a, b| a.0.total_cmp(&b.0));
        if let Some((_, t)) = naechster {
            let v = (texte[t].0[0] - p.e, texte[t].0[1] - p.n);
            *versatz
                .entry(((v.0 * 100.0).round() as i64, (v.1 * 100.0).round() as i64))
                .or_default() += 1;
        }
    }
    let haeufig = versatz
        .into_iter()
        .max_by_key(|&(k, n)| (n, std::cmp::Reverse(k)))
        .map(|((x, y), _)| [x as f64 / 100.0, y as f64 / 100.0]);
    let (mut p_belegt, mut t_belegt) = (HashSet::new(), HashSet::new());
    let mut zuordnen = |v: [f64; 2], bis: f64, punkte: &mut [Punkt]| {
        let mut paare: Vec<(f64, usize, usize)> = Vec::new();
        for &i in fehlt.iter().filter(|i| !p_belegt.contains(*i)) {
            let (e, n) = (punkte[i].e + v[0], punkte[i].n + v[1]);
            for t in raster.nahe(e, n).filter(|t| !t_belegt.contains(t)) {
                let d = (texte[t].0[0] - e).hypot(texte[t].0[1] - n);
                if d <= bis {
                    paare.push((d, i, t));
                }
            }
        }
        paare.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
        for (_, i, t) in paare {
            if !p_belegt.contains(&i) && !t_belegt.contains(&t) {
                p_belegt.insert(i);
                t_belegt.insert(t);
                punkte[i].nr = texte[t].1.to_string();
            }
        }
    };
    if let Some(v) = haeufig {
        zuordnen(v, 0.02, punkte);
    }
    zuordnen([0.0; 2], r, punkte);
    for &i in fehlt {
        if punkte[i].nr.is_empty() {
            punkte[i].nr = (i + 1).to_string();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Der Hinweis nennt die Grenze mit Tausenderpunkt (ohne eigene
    /// Gruppierlogik, Review #208 H-208c).
    #[test]
    fn zu_viele_mit_tausenderpunkt() {
        assert_eq!(MAX_PUNKTE, 2_000_000);
        assert!(
            ZU_VIELE.starts_with("Mehr als 2.000.000 Punkte"),
            "{ZU_VIELE}"
        );
    }

    fn dxf(paare: &[(i32, &str)]) -> Vec<u8> {
        let mut s = String::from("0\r\nSECTION\r\n2\r\nENTITIES\r\n");
        for (c, v) in paare {
            s.push_str(&format!("{c}\r\n{v}\r\n"));
        }
        s.push_str("0\r\nENDSEC\r\n0\r\nEOF\r\n");
        s.into_bytes()
    }

    fn lies(paare: &[(i32, &str)]) -> Aufmass {
        lesen(&dxf(paare), &Vorgaben::default()).unwrap()
    }

    fn punkt(e: &str, n: &str, z: &str, ebene: &str) -> Vec<(i32, &'static str)> {
        let s = |x: &str| -> &'static str { Box::leak(x.to_string().into_boxed_str()) };
        vec![
            (0, "POINT"),
            (8, s(ebene)),
            (10, s(e)),
            (20, s(n)),
            (30, s(z)),
        ]
    }

    #[test]
    fn punkte_mit_z_code_aus_layer_nummer_aus_text() {
        let mut p = punkt("100.0", "200.0", "30.5", "PKT_GR");
        p.extend(punkt("101.0", "200.0", "30.6", "PKT_GP"));
        p.extend(punkt("100.0", "201.0", "30.7", "HOEHEN"));
        // Nummern 0,38 m daneben; der Text von 2 liegt auch 0,8 m an 1
        p.extend([
            (0, "TEXT"),
            (8, "PUNKTNUMMER"),
            (10, "100.15"),
            (20, "199.65"),
            (1, "17"),
            (0, "TEXT"),
            (8, "PUNKTNUMMER"),
            (10, "100.75"),
            (20, "199.75"),
            (1, "18"),
            (0, "TEXT"),
            (8, "HOEHENTEXT"),
            (10, "100.1"),
            (20, "201.05"),
            (1, "30,70"),
        ]);
        let a = lies(&p);
        let n: Vec<(&str, &str, f64)> = a
            .punkte
            .iter()
            .map(|p| (p.nr.as_str(), p.code.as_str(), p.h))
            .collect();
        assert_eq!(
            n,
            [
                ("17", "GR", 30.5),
                ("18", "GP", 30.6),
                ("3", "HOEHEN", 30.7)
            ]
        );
        assert_eq!(a.erkannt.lage, Lage::Oertlich);
        let d = a.dxf.as_ref().unwrap();
        assert_eq!(d.ebenen.len(), 5);
        assert_eq!(
            (d.ebenen[0].name.as_str(), d.ebenen[0].punkte),
            ("PKT_GR", 1)
        );
        assert!(a.hinweise.is_empty(), "{:?}", a.hinweise);
    }

    /// Bord und Rinne 0,30 m nebeneinander: Der Nummerntext von Bord liegt
    /// der Rinne näher, gehört aber nach dem festen Versatz zum Bord.
    #[test]
    fn nummer_nach_dem_festen_versatz() {
        let mut p = Vec::new();
        for (nr, e, n) in [
            ("106", 105.268, 39.462),
            ("111", 105.570, 39.448),
            ("1", 120.0, 40.0),
            ("2", 130.0, 40.0),
        ] {
            p.extend(punkt(&e.to_string(), &n.to_string(), "24", "PKT"));
            p.extend(
                [
                    (0, "TEXT".to_string()),
                    (8, "PNR".into()),
                    (10, format!("{:.3}", e + 0.15)),
                    (20, format!("{:.3}", n - 0.35)),
                    (1, nr.into()),
                ]
                .map(|(c, v)| (c, &*Box::leak(v.into_boxed_str()))),
            );
        }
        let a = lies(&p);
        let nr: Vec<&str> = a.punkte.iter().map(|p| p.nr.as_str()).collect();
        assert_eq!(nr, ["106", "111", "1", "2"]);
    }

    #[test]
    fn bloecke_auf_null_mit_hoehenattribut() {
        let mut p = Vec::new();
        for (e, n, h, nr, code) in [
            ("467108.214", "5878741.637", "24,337", "1", "GR"),
            ("467132.035", "5878744.562", "24,624", "2", "GR"),
            ("467120.000", "5878760.000", "", "3", "GP"),
        ] {
            p.extend([
                (0, "INSERT"),
                (8, "HOEHENPUNKTE"),
                (66, "1"),
                (2, "HP"),
                (10, e),
                (20, n),
                (30, "0.0"),
                (0, "ATTRIB"),
                (2, "PNR"),
                (1, nr),
                (0, "ATTRIB"),
                (2, "HÖHE"),
                (1, h),
                (0, "ATTRIB"),
                (2, "CODE"),
                (1, code),
                (0, "SEQEND"),
            ]);
        }
        let a = lies(&p);
        assert_eq!(a.punkte.len(), 2);
        assert_eq!(
            (
                a.punkte[1].nr.as_str(),
                a.punkte[1].code.as_str(),
                a.punkte[1].h
            ),
            ("2", "GR", 24.624)
        );
        assert_eq!(a.erkannt.lage, Lage::Utm { zone: None });
        assert_eq!(a.dxf.as_ref().unwrap().hoehe.as_deref(), Some("HÖHE"));
        assert_eq!(
            a.hinweise,
            ["1 Punkt ohne Höhe (Z = 0, kein Höhenattribut) bleibt außen vor."]
        );
        assert!(a.satz().contains("Höhe aus Attribut HÖHE"), "{}", a.satz());
    }

    #[test]
    fn bruchkante_an_punkten_und_neue_ecken() {
        let mut p = punkt("0", "0", "10", "PKT_GP");
        p.extend(punkt("10", "0", "11", "PKT_GP"));
        p.extend(punkt("0", "10", "12", "PKT_GP"));
        p.extend([
            (0, "POLYLINE"),
            (8, "BRUCHKANTE"),
            (66, "1"),
            (70, "8"),
            (0, "VERTEX"),
            (10, "0.001"),
            (20, "0"),
            (30, "10.002"),
            (0, "VERTEX"),
            (10, "5"),
            (20, "5"),
            (30, "10.5"),
            (0, "VERTEX"),
            (10, "10"),
            (20, "0"),
            (30, "11"),
            (0, "SEQEND"),
        ]);
        let a = lies(&p);
        assert_eq!(a.punkte.len(), 4);
        assert_eq!(a.punkte[3].nr, "L1-2");
        assert_eq!(a.linien.len(), 1);
        assert_eq!(a.linien[0].punkte, [0, 3, 1]);
        let codes = crate::codes::Codetabelle::default();
        let k = a.bruchkanten(&codes, &[true; 4]);
        assert_eq!(k, [[0, 3], [3, 1]]);
        assert!(a.bruchkanten(&codes, &[true, true, true, false]).is_empty());
        assert!(a.satz().contains("1 Bruchkante"), "{}", a.satz());
    }

    #[test]
    fn hoehenlinie_alle_zwei_meter() {
        let a = lies(&[
            (0, "LWPOLYLINE"),
            (8, "HOEHENLINIEN"),
            (90, "2"),
            (70, "0"),
            (38, "25.5"),
            (10, "0"),
            (20, "0"),
            (10, "9"),
            (20, "0"),
            (0, "LWPOLYLINE"),
            (8, "HOEHENLINIEN"),
            (90, "3"),
            (70, "1"),
            (38, "26"),
            (10, "0"),
            (20, "5"),
            (10, "4"),
            (20, "5"),
            (10, "4"),
            (20, "8"),
        ]);
        // 9 m in fünf Schritten: 6 Punkte; Dreieck 4 + 3 + 5 m: 2 + 2 + 3
        assert_eq!(a.punkte.len(), 6 + 7);
        assert!(a.punkte[..6].iter().all(|p| p.h == 25.5));
        assert!((a.punkte[1].e - 1.8).abs() < 1e-12);
        assert_eq!(a.punkte[6].nr, "H2-1");
        assert_eq!(a.dxf.as_ref().unwrap().hoehenlinien, 2);
        assert!(a.linien.is_empty());
    }

    #[test]
    fn flaechen_geben_ecken_und_kanten() {
        let f = |x: [&'static str; 9]| {
            vec![
                (0, "3DFACE"),
                (8, "DGM"),
                (10, x[0]),
                (20, x[1]),
                (30, x[2]),
                (11, x[3]),
                (21, x[4]),
                (31, x[5]),
                (12, x[6]),
                (22, x[7]),
                (32, x[8]),
                (13, x[6]),
                (23, x[7]),
                (33, x[8]),
            ]
        };
        let mut p = f(["0", "0", "1", "10", "0", "2", "0", "10", "3"]);
        p.extend(f(["10", "0", "2", "10", "10", "4", "0", "10", "3"]));
        let a = lies(&p);
        assert_eq!(a.punkte.len(), 4);
        assert_eq!(a.punkte[3].nr, "N4");
        assert_eq!(a.linien.len(), 2);
        assert!(a.linien.iter().all(|l| l.art == Linienart::Masche));
        let k = a.bruchkanten(&crate::codes::Codetabelle::default(), &[true; 4]);
        // fünf Kanten, die Diagonale einmal
        assert_eq!(k.len(), 5);
        assert!(a.satz().contains("2 Netzflächen"), "{}", a.satz());
    }

    #[test]
    fn grenze_aus_polylinie_und_linien() {
        let mut p = punkt("0", "0", "10", "PKT");
        p.extend(punkt("10", "0", "11", "PKT"));
        p.extend(punkt("0", "10", "12", "PKT"));
        p.extend([
            (0, "LWPOLYLINE"),
            (8, "Flurstücksgrenze"),
            (90, "3"),
            (70, "1"),
            (10, "0"),
            (20, "0"),
            (10, "10"),
            (20, "0"),
            (10, "0"),
            (20, "10"),
        ]);
        for (a, b, c, d) in [
            ("20", "0", "30", "0"),
            ("30", "10", "30", "0"),
            ("30", "10", "20", "0"),
        ] {
            p.extend([
                (0, "LINE"),
                (8, "GRENZE"),
                (10, a),
                (20, b),
                (11, c),
                (21, d),
            ]);
        }
        p.extend([
            (0, "LINE"),
            (8, "GRENZE"),
            (10, "50"),
            (20, "0"),
            (11, "60"),
            (21, "0"),
        ]);
        let a = lies(&p);
        let g: Vec<&Linie> = a
            .linien
            .iter()
            .filter(|l| l.art == Linienart::Grenze)
            .collect();
        assert_eq!(g.len(), 2);
        assert_eq!(g[0].ebene, "Flurstücksgrenze");
        assert_eq!(g[1].lage, [[20.0, 0.0], [30.0, 0.0], [30.0, 10.0]]);
        assert_eq!(
            a.hinweise,
            ["Eine Grenzlinie schließt sich nicht zum Umring und bleibt außen vor."]
        );
        assert_eq!(a.punkte.len(), 3);
    }

    #[test]
    fn kopf_einheit_zeichensatz_und_fehler() {
        // Kopf, dann zwei Punkte, dann eine kaputte Zeile (45) statt ENDSEC
        let mut roh = b"999\r\nKommentar\r\n0\r\nSECTION\r\n2\r\nHEADER\r\n9\r\n$ACADVER\r\n1\r\nAC1009\r\n9\r\n$DWGCODEPAGE\r\n3\r\nANSI_1252\r\n9\r\n$INSUNITS\r\n70\r\n4\r\n0\r\nENDSEC\r\n0\r\nSECTION\r\n2\r\nENTITIES\r\n".to_vec();
        roh.extend(
            b"0\r\nPOINT\r\n8\r\nPKT_H\\U+00F6he\r\n10\r\n1000\r\n20\r\n2000\r\n30\r\n24500\r\n",
        );
        roh.extend(
            b"0\r\nPOINT\r\n8\r\nPKT_\xF6\r\n10\r\n3000\r\n20\r\n2000\r\n30\r\n24600\r\nkaputt\r\n",
        );
        let a = lesen(&roh, &Vorgaben::default()).unwrap();
        assert_eq!(a.punkte.len(), 2);
        assert_eq!(a.punkte[0].code, "Höhe");
        assert_eq!(a.punkte[1].code, "ö");
        assert_eq!((a.punkte[0].e, a.punkte[0].h), (1.0, 24.5));
        assert_eq!(a.erkannt.zeichensatz, lesen::Zeichensatz::Windows1252);
        assert_eq!(a.dxf.as_ref().unwrap().version_name(), "R12");
        assert_eq!(
            a.hinweise,
            [
                "Die DXF bricht an Zeile 45 ab; gelesen ist, was davor steht.",
                "Die DXF ist in Millimetern gezeichnet; Skizzeo rechnet in Meter um."
            ]
        );
        // Einheit von Hand, Layer abgewählt
        let v = Vorgaben {
            einheit: Some(1.0),
            ebenen_aus: vec!["PKT_ö".into()],
            ..Vorgaben::default()
        };
        let b = lesen(&roh, &v).unwrap();
        assert_eq!((b.punkte.len(), b.punkte[0].h), (1, 24500.0));
        assert!(b.dxf.unwrap().ebenen[1].aus);
    }

    #[test]
    fn ohne_hoehen_und_binaer() {
        let mut p = punkt("100", "200", "0", "PKT");
        p.extend([
            (0, "TEXT"),
            (8, "HOEHE"),
            (10, "100.1"),
            (20, "200.1"),
            (1, "24,5"),
        ]);
        let e = lesen(&dxf(&p), &Vorgaben::default()).unwrap_err();
        assert!(e.contains("nur als Text"), "{e}");
        let e = lesen(&dxf(&[(0, "CIRCLE"), (8, "0")]), &Vorgaben::default()).unwrap_err();
        assert!(e.starts_with("In der DXF stehen keine Punkte"), "{e}");
        assert!(lesen(b"AutoCAD Binary DXF\r\n\x1a\0", &Vorgaben::default())
            .unwrap_err()
            .contains("Binär-DXF"));
    }

    #[test]
    fn plan_linien_nur_auf_punkten() {
        // 2D-Plan: Bruchkante aus Polylinie auf Z = 0 gilt nur, wenn jede
        // Ecke auf einem Punkt liegt
        let mut p = Vec::new();
        for (e, n, h) in [("0", "0", "10"), ("10", "0", "11"), ("0", "10", "12")] {
            p.extend([
                (0, "INSERT"),
                (8, "HP"),
                (66, "1"),
                (10, e),
                (20, n),
                (0, "ATTRIB"),
                (2, "H"),
                (1, h),
                (0, "SEQEND"),
            ]);
        }
        let linie = |x: &'static str| {
            vec![
                (0, "LWPOLYLINE"),
                (8, "BRUCH"),
                (90, "2"),
                (10, "0"),
                (20, "0"),
                (10, x),
                (20, "0"),
            ]
        };
        p.extend(linie("10"));
        p.extend(linie("7"));
        let a = lies(&p);
        assert_eq!(a.punkte.len(), 3);
        assert_eq!(a.linien.len(), 1);
        assert_eq!(a.linien[0].punkte, [0, 1]);
    }

    /// 100.000 Punkte mit Nummerntext, eine Bruchkante über 1.000 von
    /// ihnen und 4 km Höhenlinie bleiben im Budget; riesige Höhenlinien
    /// enden bei [`MAX_HOEHENLINIENPUNKTE`] (Review #150: 82 Byte gaben
    /// vorher 2 Mio. Punkte und 300 MB).
    #[test]
    fn viele_punkte_im_budget() {
        let mut s = String::from("0\nSECTION\n2\nENTITIES\n");
        for i in 0..100_000 {
            let (e, n) = ((i % 400) as f64 * 1.5, (i / 400) as f64 * 1.5);
            s.push_str(&format!(
                "0\nPOINT\n8\nPKT_GP\n10\n{e}\n20\n{n}\n30\n{}\n0\nTEXT\n8\nPNR\n10\n{}\n20\n{}\n1\n{}\n",
                24.0 + (i % 7) as f64 * 0.1,
                e + 0.15,
                n - 0.35,
                i + 1
            ));
        }
        s.push_str("0\nPOLYLINE\n8\nBRUCH\n66\n1\n70\n8\n");
        for i in 0..1000 {
            s.push_str(&format!(
                "0\nVERTEX\n10\n{}\n20\n0\n30\n{}\n",
                i as f64 * 1.5,
                24.0 + (i % 7) as f64 * 0.1
            ));
        }
        s.push_str("0\nSEQEND\n0\nLWPOLYLINE\n8\nHL\n38\n25\n10\n0\n20\n-10\n10\n4000\n20\n-10\n");
        s.push_str("0\nENDSEC\n0\nEOF\n");
        let t = std::time::Instant::now();
        let a = lesen(s.as_bytes(), &Vorgaben::default()).unwrap();
        let ms = t.elapsed().as_secs_f64() * 1000.0;
        let grenze = if cfg!(debug_assertions) {
            8000.0
        } else {
            800.0
        };
        assert!(ms < grenze, "{ms:.0} ms");
        eprintln!("DXF 100.000 Punkte: {ms:.0} ms");
        // 600 Ecken der Bruchkante liegen jenseits der Punkte, 4 km
        // Höhenlinie geben 2.001
        assert_eq!(a.punkte.len(), 100_000 + 600 + 2001);
        assert_eq!(a.punkte[99_999].nr, "100000");
        assert_eq!(a.linien[0].punkte.len(), 1000);
        // eine Höhenlinie über 10.000 km gäbe 5 Mrd. Punkte
        let riesig = dxf(&[
            (0, "LWPOLYLINE"),
            (8, "HL"),
            (38, "25"),
            (10, "0"),
            (20, "0"),
            (10, "1e7"),
            (20, "0"),
        ]);
        let t = std::time::Instant::now();
        let b = lesen(&riesig, &Vorgaben::default()).unwrap();
        assert!(t.elapsed().as_secs_f64() < 5.0);
        assert_eq!(b.punkte.len(), MAX_HOEHENLINIENPUNKTE);
        assert_eq!(
            b.hinweise,
            [format!(
                "Die Höhenlinien geben mehr als {MAX_HOEHENLINIENPUNKTE} Punkte; Skizzeo liest nur die ersten."
            )]
        );
    }

    #[test]
    fn umlaute_und_hilfen() {
        assert_eq!(unicode("H\\U+00F6he \\U+zz"), "Höhe \\U+zz");
        assert_eq!(norm("Höhe_NN"), "HOEHENN");
        assert_eq!(code_aus_ebene("pkt_BO"), "BO");
        assert_eq!(code_aus_ebene("0"), "");
        assert_eq!(ebenenart("Flurstück"), Ebenenart::Grenze);
        assert_eq!(ebenenart("GEB_UMRING"), Ebenenart::Gebaeude);
        assert_eq!(ebenenart("PKT_GR"), Ebenenart::Gelaende);
        assert!(nummernebene("PUNKTNUMMER") && nummernebene("PNR") && !nummernebene("HOEHE"));
    }
}

#[cfg(test)]
mod ocs_und_z_null {
    use super::*;

    fn lies(s: &str) -> Aufmass {
        let s = format!("0\nSECTION\n2\nENTITIES\n{s}0\nENDSEC\n0\nEOF\n");
        lesen(s.as_bytes(), &Vorgaben::default()).unwrap()
    }
    fn pt(e: f64, n: f64, z: f64) -> String {
        format!("0\nPOINT\n8\nPKT\n10\n{e}\n20\n{n}\n30\n{z}\n")
    }
    fn umgebung() -> String {
        [
            (0.0, 0.0, 24.1),
            (30.0, 0.0, 24.2),
            (0.0, 30.0, 24.3),
            (30.0, 30.0, 24.4),
        ]
        .iter()
        .map(|(x, y, z)| pt(467100.0 + x, 5878700.0 + y, *z))
        .collect()
    }

    /// Geometrie-Befund #150: gespiegelter Block (Extrusion 0,0,−1): Einfügepunkt steht in OCS.
    #[test]
    fn sonde_insert_extrusion_minus_z() {
        let s = umgebung()
            + "0\nINSERT\n8\nPKT\n2\nHP\n10\n-467110.0\n20\n5878710.0\n30\n-24.25\n210\n0.0\n220\n0.0\n230\n-1.0\n";
        let a = lies(&s);
        let p = a.punkte.last().unwrap();
        assert!(
            (p.e - 467110.0).abs() < 1e-6 && (p.h - 24.25).abs() < 1e-6,
            "e={} h={}",
            p.e,
            p.h
        );
    }

    /// Höhenlinie als LWPOLYLINE mit Extrusion 0,0,−1: Erhebung in OCS.
    #[test]
    fn sonde_hoehenlinie_extrusion_minus_z() {
        let s = umgebung()
            + "0\nLWPOLYLINE\n8\nHL\n90\n2\n38\n-24.0\n10\n-467105.0\n20\n5878705.0\n10\n-467106.0\n20\n5878705.0\n210\n0.0\n220\n0.0\n230\n-1.0\n";
        let a = lies(&s);
        let p = a.punkte.last().unwrap();

        assert!(
            (p.e - 467106.0).abs() < 1e-6 && (p.h - 24.0).abs() < 1e-6,
            "e={} h={}",
            p.e,
            p.h
        );
    }

    /// 3D-Datei, ein Block auf Z = 0 mit Höhenattribut.
    #[test]
    fn sonde_gemischt_z_und_attribut() {
        let s = umgebung()
            + "0\nINSERT\n8\nPKT\n66\n1\n2\nHP\n10\n467110.0\n20\n5878710.0\n30\n0.0\n0\nATTRIB\n2\nHOEHE\n1\n24.35\n0\nSEQEND\n";
        let a = lies(&s);
        let p = a.punkte.last().unwrap();
        assert!((p.h - 24.35).abs() < 1e-6, "h={}", p.h);
    }

    /// 3D-Datei, ein POINT ohne Höhe (Z = 0) zwischen Punkten um 24 m.
    #[test]
    fn sonde_gemischt_z_null_ohne_attribut() {
        let s = umgebung() + &pt(467110.0, 5878710.0, 0.0);
        let a = lies(&s);
        let n0 = a.punkte.iter().filter(|p| p.h == 0.0).count();
        assert_eq!(n0, 0);
    }
}

/// Befunde aus Review #150 zu `a953651`.
#[cfg(test)]
mod grenzfaelle {
    use super::*;

    fn datei(vorn: &str, objekte: &str) -> String {
        format!("{vorn}0\nSECTION\n2\nENTITIES\n{objekte}0\nENDSEC\n0\nEOF\n")
    }
    fn lies(s: &str) -> Aufmass {
        lesen(s.as_bytes(), &Vorgaben::default()).unwrap()
    }
    fn pt(e: f64, n: f64, z: f64) -> String {
        format!("0\nPOINT\n8\nPKT\n10\n{e}\n20\n{n}\n30\n{z}\n")
    }
    fn umgebung() -> String {
        [
            (0.0, 0.0, 24.1),
            (30.0, 0.0, 24.2),
            (0.0, 30.0, 24.3),
            (30.0, 30.0, 24.4),
        ]
        .iter()
        .map(|(x, y, z)| pt(467100.0 + x, 5878700.0 + y, *z))
        .collect()
    }

    /// B1: Riesige Zahlen geben weder Überlauf noch NaN; die Objekte
    /// bleiben mit Hinweis außen vor.
    #[test]
    fn unmoegliche_koordinaten_bleiben_aussen_vor() {
        let s = umgebung()
            + &pt(1e300, 1e300, 24.0)
            + "0\nLWPOLYLINE\n8\nHL\n90\n2\n38\n-1e308\n10\n-1e308\n20\n0\n10\n1e308\n20\n0\n"
            + "0\nLINE\n8\nBK\n10\n467100\n20\n5878700\n30\n24\n11\n1e300\n21\n5878700\n31\n24\n"
            + "0\nTEXT\n8\nPNR\n10\n1e300\n20\n1e300\n1\n7\n"
            + "0\nINSERT\n8\nPKT\n66\n1\n2\nHP\n10\n467110\n20\n5878710\n30\n0\n0\nATTRIB\n2\nHOEHE\n1\n1e300\n0\nSEQEND\n";
        let a = lies(&datei("", &s));
        assert_eq!(a.punkte.len(), 4);
        assert!(a
            .punkte
            .iter()
            .all(|p| p.e.is_finite() && p.n.is_finite() && p.h.is_finite()));
        assert_eq!(
            a.hinweise,
            [
                "4 Objekte mit unmöglichen Koordinaten bleiben außen vor.",
                "1 Punkt auf Z = 0 zwischen Punkten mit Höhe, ohne Höhenattribut, bleibt außen vor."
            ]
        );
        // am Rand des Rasters zählt die Nachbarschaft ohne Überlauf
        let mut r = Raster::neu(1.0);
        r.rein(1e19, -1e19, 0);
        let nahe: Vec<usize> = r.nahe(1e19, -1e19).collect();
        assert!(!nahe.is_empty() && nahe.iter().all(|&i| i == 0));
        // bis zur Grenze gilt jede Zahl (UTM in Millimetern mit Zone)
        let a = lies(&datei("", &pt(32_467_100_000.0, 5_878_700_000.0, 24_500.0)));
        assert_eq!(a.punkte.len(), 1);
    }

    fn insert(z: f64, name: &str, attrib: &str) -> String {
        let folge = if attrib.is_empty() { "" } else { "66\n1\n" };
        format!(
            "0\nINSERT\n8\nPKT\n{folge}2\n{name}\n10\n467110.0\n20\n5878710.0\n30\n{z}\n{attrib}"
        )
    }
    fn hoehe(h: &str) -> String {
        format!("0\nATTRIB\n2\nHOEHE\n1\n{h}\n0\nSEQEND\n")
    }
    fn hoehen(z: [f64; 4]) -> String {
        [(0.0, 0.0), (30.0, 0.0), (0.0, 30.0), (30.0, 30.0)]
            .iter()
            .zip(z)
            .map(|((x, y), z)| pt(467100.0 + x, 5878700.0 + y, z))
            .collect()
    }

    /// B2: Ein Block ohne Attribute mit Z ≠ 0 ist ein Punkt mit Höhe Z
    /// (Punktsymbol des Vermessers), wie ein POINT.
    #[test]
    fn block_ohne_attribute_mit_z_ist_punkt() {
        let s = umgebung()
            + &insert(24.65, "VP", "")
            + &insert(0.35, "VP", "").replace("467110.0", "467120.0");
        let a = lies(&datei("", &s));
        let h: Vec<f64> = a.punkte.iter().map(|p| p.h).collect();
        assert_eq!(h, [24.1, 24.2, 24.3, 24.4, 24.65, 0.35]);
        assert!(a.hinweise.is_empty(), "{:?}", a.hinweise);
    }

    /// B2, NHN-Datei: Rahmen, Nordpfeil, ein Block nur mit Nummer und ein
    /// POINT auf Z = 0 ohne Höhenattribut fallen mit Hinweis weg; kein
    /// Punkt bekommt die Höhe 0.
    #[test]
    fn nhn_datei_mit_rahmen_auf_z_null() {
        let s = umgebung()
            + &insert(0.0, "RAHMEN", "")
            + &insert(0.0, "NORD", "")
            + &insert(0.0, "HP", "0\nATTRIB\n2\nPNR\n1\n77\n0\nSEQEND\n")
            + &pt(467111.0, 5878711.0, 0.0);
        let a = lies(&datei("", &s));
        assert_eq!(a.punkte.len(), 4);
        assert!(a.punkte.iter().all(|p| p.h != 0.0));
        assert_eq!(
            a.hinweise,
            ["4 Punkte auf Z = 0 zwischen Punkten mit Höhe, ohne Höhenattribut, bleiben außen vor."]
        );
        // knapp über 10 m gilt als fern
        let a = lies(&datei(
            "",
            &(hoehen([10.01, -10.5, 12.0, 30.0]) + &pt(1.0, 1.0, 0.0)),
        ));
        assert_eq!(a.punkte.len(), 4);
    }

    /// B2, örtliche Höhen: ±0,00 ist echt. POINT und Block ohne Attribute
    /// auf Z = 0 bleiben mit der Höhe 0 und einem Hinweis; 10 m gelten
    /// noch als nah, Millimeter zählen umgerechnet.
    #[test]
    fn oertliche_hoehen_mit_null_punkt() {
        let s =
            hoehen([-1.25, 0.4, 1.5, 2.3]) + &pt(467111.0, 5878711.0, 0.0) + &insert(0.0, "HP", "");
        let a = lies(&datei("", &s));
        assert_eq!(a.punkte.len(), 6);
        assert_eq!(a.punkte.iter().filter(|p| p.h == 0.0).count(), 2);
        assert_eq!(
            a.hinweise,
            ["2 Punkte genau auf 0,000 (Z = 0, kein Höhenattribut) zählen mit dieser Höhe."]
        );
        let a = lies(&datei(
            "",
            &(hoehen([10.0, 12.0, 30.0, 40.0]) + &pt(1.0, 1.0, 0.0)),
        ));
        assert_eq!(a.punkte.len(), 5);
        let kopf = "0\nSECTION\n2\nHEADER\n9\n$INSUNITS\n70\n4\n0\nENDSEC\n";
        let s = hoehen([-1250.0, 400.0, 1500.0, 2300.0]) + &pt(467111.0, 5878711.0, 0.0);
        let a = lies(&datei(kopf, &s));
        assert_eq!(a.punkte.len(), 5);
        assert_eq!(
            a.hinweise,
            [
                "Die DXF ist in Millimetern gezeichnet; Skizzeo rechnet in Meter um.",
                "1 Punkt genau auf 0,000 (Z = 0, kein Höhenattribut) zählt mit dieser Höhe."
            ]
        );
    }

    /// B2, gemischte Datei: Auf Z = 0 gilt das HOEHE-Attribut, bei NHN wie
    /// bei örtlichen Höhen.
    #[test]
    fn gemischte_datei_mit_hoehenattribut() {
        for (z, h, w) in [
            ([24.1, 24.2, 24.3, 24.4], "24,8", 24.8),
            ([-1.25, 0.4, 1.5, 2.3], "0,85", 0.85),
        ] {
            let s = hoehen(z) + &insert(0.0, "HP", &hoehe(h));
            let a = lies(&datei("", &s));
            assert_eq!(a.punkte.len(), 5);
            assert_eq!(a.punkte[4].h, w);
            assert_eq!(a.dxf.as_ref().unwrap().hoehe.as_deref(), Some("HOEHE"));
            assert!(a.hinweise.is_empty(), "{:?}", a.hinweise);
        }
    }

    /// B3 wie im Befund: LWPOLYLINE mit Extrusion (0, 0, −1) und Erhebung
    /// 25 liegt in WCS bei x = −10 bis −20 auf −25.
    #[test]
    fn gespiegelte_hoehenlinie_wie_im_befund() {
        let s = "0\nLWPOLYLINE\n8\nHL\n90\n2\n38\n25\n10\n10\n20\n5\n10\n20\n20\n5\n210\n0\n220\n0\n230\n-1\n";
        let a = lies(&datei("", s));
        assert_eq!(a.punkte.len(), 6);
        for p in &a.punkte {
            assert!(
                (-20.0..=-10.0).contains(&p.e) && p.n == 5.0 && p.h == -25.0,
                "{p:?}"
            );
        }
    }

    /// B3: Bei schräger Extrusion liegt die Polylinie schräg im Raum und
    /// gilt als Bruchkante mit den Höhen ihrer Ecken.
    #[test]
    fn schraege_extrusion_gibt_bruchkante() {
        // n = (0, 0,6, 0,8): OCS-x ist −x, OCS-y ist (0, −0,8, 0,6)
        let s = "0\nLWPOLYLINE\n8\nBK\n90\n2\n38\n25\n10\n0\n20\n0\n10\n0\n20\n10\n210\n0\n220\n0.6\n230\n0.8\n";
        let a = lies(&datei("", s));
        let ecken: Vec<[f64; 3]> = a.punkte.iter().map(|p| [p.e, p.n, p.h]).collect();
        let soll = [[0.0, 15.0, 20.0], [0.0, 7.0, 26.0]];
        assert_eq!(ecken.len(), 2);
        for (p, q) in ecken.iter().zip(soll) {
            assert!((0..3).all(|k| (p[k] - q[k]).abs() < 1e-9), "{p:?} {q:?}");
        }
        assert_eq!(a.linien.len(), 1);
        assert_eq!(
            (a.linien[0].art, a.linien[0].punkte.as_slice()),
            (Linienart::Bruchkante, [0, 1].as_slice())
        );
    }

    /// B4: UTF-8-BOM vor „0 / SECTION“: Die Datei ist eine DXF, und die
    /// Einheit aus dem Kopf gilt.
    #[test]
    fn bom_vor_dem_kopf() {
        let kopf = "0\nSECTION\n2\nHEADER\n9\n$INSUNITS\n70\n4\n0\nENDSEC\n";
        let mut roh = b"\xEF\xBB\xBF".to_vec();
        roh.extend(datei(kopf, &pt(1000.0, 2000.0, 24500.0)).bytes());
        assert_eq!(lesen::format(&roh), lesen::Format::Dxf);
        let a = lesen(&roh, &Vorgaben::default()).unwrap();
        assert_eq!((a.punkte[0].e, a.punkte[0].h), (1.0, 24.5));
        assert_eq!(
            a.hinweise,
            ["Die DXF ist in Millimetern gezeichnet; Skizzeo rechnet in Meter um."]
        );
    }

    /// Nur \r als Zeilenende (alte Mac-Dateien) liest sich wie \n.
    #[test]
    fn nur_cr_als_zeilenende() {
        let s = datei("", &umgebung());
        let a = lies(&s);
        let b = lies(&s.replace('\n', "\r"));
        assert_eq!(a.punkte, b.punkte);
        assert!(b.hinweise.is_empty(), "{:?}", b.hinweise);
    }

    /// Was Skizzeo nicht liest, nennt ein Hinweis: Blöcke mit Geländedaten,
    /// Blockreihen, Bögen, Kreise und Polylinien mit Bögen.
    #[test]
    fn bloecke_reihen_und_boegen_mit_hinweis() {
        let flaeche = "0\n3DFACE\n8\n0\n10\n0\n20\n0\n30\n24\n11\n1\n21\n0\n31\n24\n12\n0\n22\n1\n32\n24\n13\n0\n23\n1\n33\n24\n";
        let bloecke = format!(
            "0\nSECTION\n2\nBLOCKS\n0\nBLOCK\n8\n0\n2\nDGM\n{flaeche}0\nENDBLK\n0\nBLOCK\n8\n0\n2\nKREUZ\n0\nLINE\n8\n0\n10\n-1\n20\n0\n30\n0\n11\n1\n21\n0\n31\n0\n0\nENDBLK\n0\nBLOCK\n8\n0\n2\n*U1\n{flaeche}0\nENDBLK\n0\nENDSEC\n"
        );
        let s = umgebung()
            + "0\nINSERT\n8\nDGM\n2\nDGM\n10\n467100\n20\n5878700\n30\n24\n"
            + "0\nINSERT\n8\nPKT\n2\nKREUZ\n70\n3\n71\n2\n10\n467105\n20\n5878705\n30\n24.2\n"
            + "0\nINSERT\n8\nPKT\n2\n*U1\n10\n467106\n20\n5878706\n30\n24.2\n"
            + "0\nCIRCLE\n8\nBAUM\n10\n467101\n20\n5878701\n30\n0\n40\n2\n"
            + "0\nARC\n8\nBORD\n10\n467102\n20\n5878702\n30\n0\n40\n2\n50\n0\n51\n90\n"
            + "0\nLWPOLYLINE\n8\nWEG\n90\n2\n10\n467100\n20\n5878700\n42\n0.5\n10\n467110\n20\n5878700\n";
        let a = lies(&datei(&bloecke, &s));
        assert_eq!(a.punkte.len(), 7);
        assert_eq!(
            a.hinweise,
            [
                "1 eingefügter Block enthält 3D-Punkte, 3D-Linien oder Flächen; Skizzeo liest von einem Block nur den Einfügepunkt. Bitte Blöcke mit Geländedaten im CAD auflösen.",
                "1 Blockreihe (Spalten oder Zeilen) zählt nur mit ihrem ersten Block.",
                "Bögen, Kreise, Splines und Schraffuren liest Skizzeo nicht (2 in der Datei). 1 Polylinie mit Bögen gilt mit geraden Kanten."
            ]
        );
    }

    /// Ohne `$INSUNITS` sehen UTM-Werte in Millimetern wie bei der
    /// Punktliste nach Millimetern aus: Hinweis und Merker, gelesen wird
    /// unverändert. Dezimeter (`$INSUNITS` 14) rechnet Skizzeo um.
    #[test]
    fn millimeter_ohne_einheit_und_dezimeter() {
        let s: String = [
            (0.0, 0.0, 24_337.0),
            (30_000.0, 0.0, 24_624.0),
            (0.0, 30_000.0, 24_500.0),
        ]
        .iter()
        .map(|(x, y, z)| pt(467_100_000.0 + x, 5_878_700_000.0 + y, *z))
        .collect();
        let a = lies(&datei("", &s));
        assert!(a.erkannt.millimeter);
        assert_eq!(a.hinweise, [lesen::MILLIMETER]);
        let v = Vorgaben {
            einheit: Some(0.001),
            ..Vorgaben::default()
        };
        let b = lesen(datei("", &s).as_bytes(), &v).unwrap();
        assert!(
            !b.erkannt.millimeter && b.hinweise.is_empty(),
            "{:?}",
            b.hinweise
        );
        assert!((b.punkte[0].h - 24.337).abs() < 1e-9);
        let kopf = "0\nSECTION\n2\nHEADER\n9\n$INSUNITS\n70\n14\n0\nENDSEC\n";
        let d = lies(&datei(kopf, &pt(10.0, 20.0, 245.0)));
        assert_eq!(d.punkte[0].h, 24.5);
        assert_eq!(
            d.hinweise,
            ["Die DXF ist in Dezimetern gezeichnet; Skizzeo rechnet in Meter um."]
        );
    }
}
