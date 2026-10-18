//! Projektdatei `.szo`: zeilenbasierter Text, UTF-8, versioniert, diff-bar.
//!
//! Eine Zeile ist ein Datensatz: `[abschnitt]`, dann `schlüssel=wert`, getrennt
//! durch Leerzeichen. Texte stehen in `"…"` mit `\"`, `\\` und `\n`; `#` beginnt
//! einen Kommentar. Verweise gehen nur über Guids. Gespeichert werden Bibliothek,
//! Attribute und Parametrik, nie Körper, Netze oder Mengen.
//!
//! Reihenfolge: Attribute (`pen`, `linetype`, `fill`, `surface`, `display`),
//! `material`, `layerset` (Bauteiltyp) mit seinen `layer` und `typeprop`, dann `project`, `building`,
//! `storey`, `run`, `wall`, `slab`, `footing`, `floor`, `prop`. Innerhalb eines Abschnitts nach Guid sortiert, damit Diffs
//! ruhig bleiben. Speichern, Öffnen und wieder Speichern ergibt dieselben Bytes.

mod aufmass;
pub(crate) mod ausbau;
mod ausbauart;
mod bemassung;
pub use bemassung::{firmenstil_lesen, firmenstil_text};
mod dach;
mod darstellung;
mod flaeche;
mod gelaende;
mod hoehen;
mod lage;
mod oeffnung;
mod raum;
mod sicht;

pub(crate) use darstellung::Roh;

use crate::attr::{Attributes, Dash, Fill, FillKind, FillSpace, HatchLine, LineType, Pen, Surface};
use crate::element::{
    Building, Category, Coupling, Element, ElementKind, Floor, GroundSlab, LevelEdge, LevelKind,
    LevelRef, PropSet, PropValue, Soffit, Storey, StripFooting, Terrace, Wall, WallRun,
};
use crate::erweiterung::{ExtDef, ExtPart};
use crate::guid::{Guid, GuidGen};
use crate::id::{Arena, Id};
use crate::library::{
    type_code, Bearing, LayerFunction, LayerSet, LayerSetId, MatCategory, Material,
    MaterialDisplay, MaterialLayer, TypeCategory,
};
use crate::model::{Defaults, Model, Project};
use crate::solid::edge_kind;
use crate::trade::{self, Trade, TradeId};
use crate::wall::{segment_count, RefSide};
use sk_math::vec3;
use std::cell::Cell;
use std::collections::{HashMap, HashSet};
use std::fmt::{self, Write as _};

/// Hauptversion des Formats. Eine Datei mit höherer Version wird nicht geöffnet.
pub const VERSION: u32 = 4;

/// Fehler beim Laden; die Datei wird dann gar nicht übernommen.
#[derive(Clone, Debug, PartialEq)]
pub struct LoadError {
    /// Zeilennummer ab 1; 0, wenn sie keine bestimmte Zeile betrifft.
    pub line: usize,
    pub message: String,
}

impl fmt::Display for LoadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.line > 0 {
            write!(f, "Zeile {}: {}", self.line, self.message)
        } else {
            f.write_str(&self.message)
        }
    }
}

pub(crate) fn err(line: usize, message: impl Into<String>) -> LoadError {
    LoadError {
        line,
        message: message.into(),
    }
}

/// Geladenes Modell und Hinweise (übersprungene Abschnitte und Schlüssel,
/// ergänzte Startwerte, Verstöße aus [`Model::check`]).
pub struct Loaded {
    pub model: Model,
    pub hints: Vec<String>,
}

// --- Zeilen schreiben -----------------------------------------------------

/// Ein Datensatz beim Schreiben.
pub struct Line(String);

impl Line {
    pub fn new(section: &str) -> Line {
        Line(format!("[{section}]"))
    }

    fn raw(mut self, key: &str, value: impl fmt::Display) -> Line {
        let _ = write!(self.0, " {key}={value}");
        self
    }

    pub fn text(self, key: &str, v: &str) -> Line {
        let mut q = String::with_capacity(v.len() + 2);
        q.push('"');
        for c in v.chars() {
            match c {
                '"' => q.push_str("\\\""),
                '\\' => q.push_str("\\\\"),
                '\n' => q.push_str("\\n"),
                c => q.push(c),
            }
        }
        q.push('"');
        self.raw(key, q)
    }

    /// Wort ohne Leerzeichen und Anführungszeichen (Schlüsselwörter, Namen von Rollen).
    pub fn word(self, key: &str, v: &str) -> Line {
        self.raw(key, v)
    }

    /// Zahl in kürzester exakter Darstellung.
    pub fn num(self, key: &str, v: impl fmt::Display) -> Line {
        self.raw(key, v)
    }

    pub fn flag(self, key: &str, v: bool) -> Line {
        self.raw(key, v as u8)
    }

    pub fn guid(self, key: &str, g: Option<Guid>) -> Line {
        match g {
            Some(g) => self.raw(key, g.to_ifc()),
            None => self.raw(key, "-"),
        }
    }

    /// Farbe als `rrggbb`.
    pub fn color(self, key: &str, c: [u8; 3]) -> Line {
        self.raw(key, hex(c))
    }

    pub fn finish(self, out: &mut String) {
        out.push_str(&self.0);
        out.push('\n');
    }
}

pub fn hex(c: [u8; 3]) -> String {
    format!("{:02x}{:02x}{:02x}", c[0], c[1], c[2])
}

// --- Zeilen lesen ---------------------------------------------------------

/// Ein gelesener Datensatz. Merkt sich, welche Schlüssel benutzt wurden, damit
/// unbekannte als Hinweis gemeldet werden können.
#[derive(Debug)]
pub struct Record {
    pub line: usize,
    pub section: String,
    fields: Vec<(String, String, Cell<bool>)>,
    /// Unbekannte Werte, für die ein Ersatz gilt (neuere Dateien).
    replaced: Cell<usize>,
}

impl Record {
    /// Zerlegt eine Zeile. `Ok(None)` für leere Zeilen und Kommentare.
    pub fn parse(line: usize, text: &str) -> Result<Option<Record>, LoadError> {
        let t = text.trim();
        if t.is_empty() || t.starts_with('#') {
            return Ok(None);
        }
        let rest = t
            .strip_prefix('[')
            .ok_or_else(|| err(line, "Datensatz muss mit [abschnitt] beginnen"))?;
        let close = rest
            .find(']')
            .ok_or_else(|| err(line, "] nach dem Abschnitt fehlt"))?;
        let section = rest[..close].trim().to_string();
        let mut fields = Vec::new();
        let mut chars = rest[close + 1..].chars().peekable();
        loop {
            while chars.peek().is_some_and(|c| c.is_whitespace()) {
                chars.next();
            }
            let Some(&c) = chars.peek() else { break };
            if c == '#' {
                break;
            }
            let mut key = String::new();
            while let Some(&c) = chars.peek() {
                if c == '=' || c.is_whitespace() {
                    break;
                }
                key.push(c);
                chars.next();
            }
            if chars.next() != Some('=') || key.is_empty() {
                return Err(err(line, format!("„{key}“ ohne Wert")));
            }
            let mut value = String::new();
            if chars.peek() == Some(&'"') {
                chars.next();
                loop {
                    match chars.next() {
                        Some('"') => break,
                        Some('\\') => match chars.next() {
                            Some('n') => value.push('\n'),
                            Some(c @ ('"' | '\\')) => value.push(c),
                            _ => return Err(err(line, "ungültiges \\ im Text")),
                        },
                        Some(c) => value.push(c),
                        None => return Err(err(line, "Text ohne schließendes \"")),
                    }
                }
            } else {
                while let Some(&c) = chars.peek() {
                    if c.is_whitespace() {
                        break;
                    }
                    value.push(c);
                    chars.next();
                }
            }
            fields.push((key, value, Cell::new(false)));
        }
        Ok(Some(Record {
            line,
            section,
            fields,
            replaced: Cell::new(0),
        }))
    }

    /// Alle Schlüssel gelten als gelesen: der Satz bleibt als Ganzes roh
    /// (Erweiterungsbauteile ohne lesbare Definition).
    pub(crate) fn all_used(&self) {
        self.fields.iter().for_each(|f| f.2.set(true));
    }

    /// Unbekannte Angaben nach dem Lesen: Schlüssel, die kein Leser abgefragt
    /// hat, und Werte, für die ein Ersatz gilt.
    pub fn unknown(&self) -> usize {
        self.fields.iter().filter(|f| !f.2.get()).count() + self.replaced.get()
    }

    /// Wert zu `key`, falls vorhanden.
    pub fn opt(&self, key: &str) -> Option<&str> {
        self.fields.iter().find(|f| f.0 == key).map(|f| {
            f.2.set(true);
            f.1.as_str()
        })
    }

    pub fn get(&self, key: &str) -> Result<&str, LoadError> {
        self.opt(key)
            .ok_or_else(|| err(self.line, format!("[{}]: „{key}“ fehlt", self.section)))
    }

    fn bad(&self, key: &str, what: &str) -> LoadError {
        err(
            self.line,
            format!(
                "[{}]: „{key}“ ist kein gültiger Wert ({what})",
                self.section
            ),
        )
    }

    pub fn f64(&self, key: &str) -> Result<f64, LoadError> {
        self.get(key)?
            .parse::<f64>()
            .ok()
            .filter(|v| v.is_finite())
            .ok_or_else(|| self.bad(key, "Zahl"))
    }

    pub fn f32(&self, key: &str) -> Result<f32, LoadError> {
        self.get(key)?
            .parse::<f32>()
            .ok()
            .filter(|v| v.is_finite())
            .ok_or_else(|| self.bad(key, "Zahl"))
    }

    pub fn int<T: std::str::FromStr>(&self, key: &str) -> Result<T, LoadError> {
        self.get(key)?
            .parse::<T>()
            .map_err(|_| self.bad(key, "ganze Zahl"))
    }

    pub fn flag(&self, key: &str) -> Result<bool, LoadError> {
        match self.get(key)? {
            "0" => Ok(false),
            "1" => Ok(true),
            _ => Err(self.bad(key, "0 oder 1")),
        }
    }

    pub fn color(&self, key: &str) -> Result<[u8; 3], LoadError> {
        parse_hex(self.get(key)?).ok_or_else(|| self.bad(key, "Farbe rrggbb"))
    }

    pub fn guid(&self, key: &str) -> Result<Guid, LoadError> {
        Guid::from_ifc(self.get(key)?).ok_or_else(|| self.bad(key, "Guid"))
    }

    /// Verweis über eine Guid auf einen schon gelesenen Datensatz.
    fn link<T>(&self, key: &str, table: &HashMap<Guid, Id<T>>) -> Result<Id<T>, LoadError> {
        let g = self.guid(key)?;
        table.get(&g).copied().ok_or_else(|| {
            err(
                self.line,
                format!(
                    "[{}]: „{key}“ verweist auf unbekannte Guid {}",
                    self.section,
                    g.to_ifc()
                ),
            )
        })
    }

    /// Wie [`Record::link`], `-` heißt „keiner“.
    fn link_opt<T>(
        &self,
        key: &str,
        table: &HashMap<Guid, Id<T>>,
    ) -> Result<Option<Id<T>>, LoadError> {
        if self.opt(key) == Some("-") {
            return Ok(None);
        }
        self.link(key, table).map(Some)
    }

    /// Satz bewusst übergangen (F-17): seine Schlüssel gelten als gelesen.
    pub fn skip(&self) {
        for f in &self.fields {
            f.2.set(true);
        }
    }

    /// Nicht benutzte Schlüssel als Hinweis.
    pub fn unused(&self, hints: &mut Vec<String>) {
        for f in self.fields.iter().filter(|f| !f.2.get()) {
            hints.push(format!(
                "Zeile {}: unbekannter Schlüssel „{}“ in [{}] übersprungen",
                self.line, f.0, self.section
            ));
        }
    }
}

/// Hinweis für einen Wert außerhalb seines Bereichs: Die Vorgabe gilt,
/// das Projekt öffnet trotzdem. `einheit` mit führendem Leerzeichen
/// (" mm"), beim Grad ohne ("°"); Zahlen mit Komma.
fn ausserhalb(line: usize, name: &str, v: f64, min: f64, max: f64, einheit: &str) -> String {
    let z = |x: f64| x.to_string().replace('.', ",");
    format!(
        "Zeile {line}: {name} {}{einheit} außerhalb {} bis {}{einheit}, Vorgabe gilt",
        z(v),
        z(min),
        z(max)
    )
}

pub fn parse_hex(s: &str) -> Option<[u8; 3]> {
    if s.len() != 6 || !s.is_ascii() {
        return None;
    }
    let b = |i: usize| u8::from_str_radix(&s[i..i + 2], 16).ok();
    Some([b(0)?, b(2)?, b(4)?])
}

/// Prüft die Kopfzeile `<kopf> <version>`.
pub fn check_header(first: Option<&str>, head: &str, version: u32) -> Result<(), LoadError> {
    let mut it = first.unwrap_or("").split_whitespace();
    if it.next() != Some(head) {
        return Err(err(1, format!("Kopfzeile „{head} {version}“ fehlt")));
    }
    let v: u32 = it
        .next()
        .and_then(|v| v.parse().ok())
        .ok_or_else(|| err(1, "Version fehlt"))?;
    if v > version {
        return Err(err(
            1,
            format!("Datei aus neuerer Skizzeo-Version ({head} {v}), diese kann {head} {version}"),
        ));
    }
    Ok(())
}

// --- Schlüsselwörter -------------------------------------------------------

const EDGE_NAMES: [(&str, u8); edge_kind::COUNT] = [
    ("view", edge_kind::VIEW),
    ("cut", edge_kind::CUT),
    ("fine", edge_kind::FINE),
    ("cut_layer", edge_kind::CUT_LAYER),
];

/// Wort der Schichtfunktion in `[layer] fn=` (auch die Regel `fn=` einer
/// Bauleistung, KA-0).
pub fn layer_function(f: LayerFunction) -> &'static str {
    match f {
        LayerFunction::Structure => "loadbearing",
        LayerFunction::Insulation => "insulation",
        LayerFunction::Finish => "finish",
        LayerFunction::Membrane => "membrane",
        LayerFunction::AirGap => "airgap",
    }
}

const LAYER_FUNCTIONS: [LayerFunction; 5] = [
    LayerFunction::Structure,
    LayerFunction::Insulation,
    LayerFunction::Finish,
    LayerFunction::Membrane,
    LayerFunction::AirGap,
];

fn category(c: Category) -> &'static str {
    crate::kinds::spec(c).szo
}

/// `set=` an Decke, Sohlplatte und Frostschürze nur mit Typ: Dateien ohne
/// bleiben bytegleich (R4).
fn slab_type(line: Line, m: &Model, e: &Element) -> Line {
    match e.layer_set.and_then(|s| m.layer_set(s)) {
        Some(t) => line.guid("set", Some(t.guid)),
        None => line,
    }
}

/// Gefälle des Dachaufbaus (Gefälledämmung): `slope=` in Prozent und
/// `drains=x,y;x,y` in mm, nur wenn es Gefälle oder Abläufe gibt (ältere
/// Dateien bleiben bytegleich).
fn write_drainage(line: Line, d: &crate::gefaelle::Drainage) -> Line {
    if !d.on() && d.drains.is_empty() {
        return line;
    }
    let line = line.num("slope", d.slope);
    if d.drains.is_empty() {
        return line;
    }
    let pts: Vec<String> = d
        .drains
        .iter()
        .map(|p| format!("{},{}", p.x, p.y))
        .collect();
    line.word("drains", &pts.join(";"))
}

/// Art eines Geschossbands in der Datei (`kind=`); „roof“ seit dem
/// Flachdach (Jörn 10.10.), ältere Dateien kennen es nicht.
fn level_kind_word(k: LevelKind) -> &'static str {
    match k {
        LevelKind::Foundation => "foundation",
        LevelKind::Storey => "storey",
        LevelKind::Roof => "roof",
    }
}

fn ref_side(r: RefSide) -> &'static str {
    match r {
        RefSide::Left => "left",
        RefSide::Right => "right",
        RefSide::Center => "center",
    }
}

/// Wie [`keyword`], ein unbekanntes Wort (aus einer neueren Fassung) gilt
/// aber als `fallback` und wird als unbekannte Angabe gezählt.
pub(crate) fn keyword_or<T: Copy>(
    r: &Record,
    key: &str,
    all: &[T],
    name: impl Fn(T) -> &'static str,
    fallback: T,
) -> Result<T, LoadError> {
    let v = r.get(key)?;
    Ok(all
        .iter()
        .copied()
        .find(|&x| name(x) == v)
        .unwrap_or_else(|| {
            r.replaced.set(r.replaced.get() + 1);
            fallback
        }))
}

pub(crate) fn keyword<T: Copy>(
    r: &Record,
    key: &str,
    all: &[T],
    name: impl Fn(T) -> &'static str,
) -> Result<T, LoadError> {
    let v = r.get(key)?;
    all.iter().copied().find(|&x| name(x) == v).ok_or_else(|| {
        // Unbekanntes Wort: meist aus einer neueren Version (z. B. „air“
        // seit K4), deshalb Wert und Grund nennen
        err(
            r.line,
            format!(
                "[{}]: „{key}={v}“ unbekannt, Datei vermutlich aus einer neueren Skizzeo-Version",
                r.section
            ),
        )
    })
}

// --- Schreiben ------------------------------------------------------------

pub(crate) fn sorted<'a, T: 'a>(
    it: impl Iterator<Item = (Id<T>, &'a T)>,
    guid: impl Fn(&T) -> Guid,
) -> Vec<&'a T> {
    let mut v: Vec<&T> = it.map(|(_, x)| x).collect();
    v.sort_by_key(|x| guid(x));
    v
}

/// Stift als Zeile.
pub(crate) fn write_pen(out: &mut String, p: &Pen) {
    Line::new("pen")
        .guid("guid", Some(p.guid))
        .num("nr", p.number)
        .text("name", &p.name)
        .color("color", p.color)
        .num("w", p.width_mm)
        .finish(out);
}

pub(crate) fn write_line_type(out: &mut String, l: &LineType) {
    let pat = if l.pattern.is_empty() {
        "-".to_string()
    } else {
        l.pattern
            .iter()
            .map(|d| format!("{}:{}:{}", d.len_mm, d.gap_mm, d.dot as u8))
            .collect::<Vec<_>>()
            .join(";")
    };
    Line::new("linetype")
        .guid("guid", Some(l.guid))
        .text("name", &l.name)
        .word("pat", &pat)
        .finish(out);
}

pub(crate) fn write_fill(out: &mut String, f: &Fill) {
    let space = match f.space {
        FillSpace::Paper => "paper",
        FillSpace::Model => "model",
    };
    let line = Line::new("fill")
        .guid("guid", Some(f.guid))
        .text("name", &f.name)
        .word("space", space);
    let line = match &f.kind {
        FillKind::Empty => line.word("kind", "empty"),
        FillKind::Solid => line.word("kind", "solid"),
        FillKind::Lines(ls) => {
            let v = ls
                .iter()
                .map(|l| {
                    format!(
                        "{}:{}:{}:{}:{}",
                        l.angle_deg, l.spacing_mm, l.offset_mm, l.dash_mm, l.gap_mm
                    )
                })
                .collect::<Vec<_>>()
                .join(";");
            // Winkel gegen den Uhrzeigersinn (E3b)
            line.word("kind", "lines")
                .word("lines", &v)
                .flag("ccw", true)
        }
        FillKind::Zigzag { period } => line.word("kind", "zigzag").num("period", period),
    };
    line.finish(out);
}

pub(crate) fn write_surface(out: &mut String, s: &Surface) {
    Line::new("surface")
        .guid("guid", Some(s.guid))
        .text("name", &s.name)
        .color("color", s.color)
        .color("cut", s.cut_color)
        .finish(out);
}

/// Baustoff als Zeile; `fill`, `fg`, `bg`, `surface` sind die Guids seiner
/// Darstellung.
pub(crate) fn write_material(
    out: &mut String,
    x: &Material,
    [fill, fg, bg, surface]: [Option<Guid>; 4],
) {
    let line = Line::new("material")
        .guid("guid", Some(x.guid))
        .text("name", &x.name)
        .word("cat", x.category.word())
        .num("prio", x.priority)
        .num("rho", x.density);
    let line = match x.lambda {
        Some(l) => line.num("lambda", l),
        None => line.word("lambda", "-"),
    };
    let line = line
        .guid("fill", fill)
        .guid("fg", fg)
        .guid("bg", bg)
        .guid("surface", surface);
    // Gewerk nur, wenn gesetzt (Paket 1a)
    match x.trade {
        Some(t) => line.guid("trade", Some(t.0)),
        None => line,
    }
    .finish(out);
}

/// Gewerke, die ein Baustoff oder eine Schicht nennt, je einmal.
pub(crate) fn used_trades<'a>(
    materials: impl Iterator<Item = &'a Material>,
    types: impl Iterator<Item = &'a LayerSet>,
) -> Vec<TradeId> {
    let mut v: Vec<TradeId> = materials
        .filter_map(|x| x.trade)
        .chain(types.flat_map(|t| t.layers.iter().filter_map(|l| l.trade)))
        .collect();
    v.sort();
    v.dedup();
    v
}

/// `[trade]` je verwendetem Gewerk, nach Reihe (Paket 1a §5), dazu jedes
/// mit eigenem Kurznamen. `short=` nur, wenn er vom Startbestand abweicht
/// (Regel 67): ohne eigene Kurznamen bleibt die Datei bytegleich.
pub(crate) fn write_trades(out: &mut String, trades: &[Trade], used: &[TradeId]) {
    for t in trades {
        let own = t.short.as_deref() != t.start_short();
        if !used.contains(&t.id()) && !own {
            continue;
        }
        let line = Line::new("trade")
            .guid("guid", Some(t.guid))
            .text("code", &t.code)
            .text("name", &t.name)
            .num("order", t.order);
        let line = match own {
            true => line.text("short", t.short.as_deref().unwrap_or("")),
            false => line,
        };
        line.finish(out);
    }
}

/// Gewerke aus `[trade]`, zusammengeführt mit dem Startbestand.
pub(crate) fn read_trades(recs: &[Record]) -> Result<Vec<Trade>, LoadError> {
    let mut read = Vec::new();
    for r in recs {
        read.push(Trade {
            guid: r.guid("guid")?,
            code: r.get("code")?.to_string(),
            name: r.get("name")?.to_string(),
            order: r.int("order")?,
            short: r
                .opt("short")
                .map(|k| if trade::short_ok(k) { k } else { "" }.to_string()),
        });
    }
    Ok(trade::merge(read))
}

/// Bauteiltyp mit seinen Schichten und Merkmalen.
pub(crate) fn write_type(
    out: &mut String,
    s: &LayerSet,
    mat_guid: impl Fn(crate::library::MaterialId) -> Option<Guid>,
) {
    let mut line = Line::new("layerset")
        .guid("guid", Some(s.guid))
        .text("name", &s.name)
        .text("code", &s.code)
        .word("cat", s.category.word())
        .num("changed", s.changed);
    // Deckenauflager nur, wenn es nicht der ganze Kern ist (K5)
    if let Bearing::Depth { depth, strip } = s.bearing {
        line = line.num("bearing", depth).guid("strip", mat_guid(strip));
    }
    line = ausbau::write_height(line, s);
    line.text("note", &s.note).finish(out);
    for l in &s.layers {
        let line = Line::new("layer")
            .guid("set", Some(s.guid))
            .guid("mat", mat_guid(l.material))
            .num("t", l.thickness)
            .word("fn", layer_function(l.function))
            .flag("core", l.core);
        // Abweichung von Baustoff bzw. Tabelle nur, wenn gesetzt (Paket 1a)
        let line = match l.trade {
            Some(t) => line.guid("trade", Some(t.0)),
            None => line,
        };
        let line = match l.kg {
            Some(k) => line.num("kg", k),
            None => line,
        };
        // Gewählte Bauleistung nur, wenn gesetzt (KA-0b, BIM §3.10)
        match l.svc {
            Some(g) => line.guid("svc", Some(g)),
            None => line,
        }
        .finish(out);
    }
    write_props(out, "typeprop", "set", s.guid, &s.props);
}

/// Merkmale als `[section] key=… value|num|bool=…`.
fn write_props(out: &mut String, section: &str, owner: &str, g: Guid, props: &PropSet) {
    for (k, v) in props {
        let line = Line::new(section).guid(owner, Some(g)).text("key", k);
        match v {
            PropValue::Text(t) => line.text("value", t),
            PropValue::Number(n) => line.num("num", n),
            PropValue::Bool(b) => line.flag("bool", *b),
        }
        .finish(out);
    }
}

/// Das Modell als `.szo`-Text.
pub fn write(m: &Model) -> String {
    let mut text = write_known(m);
    m.ext.write(&mut text);
    crate::catalog::with_foreign(text, &m.foreign)
}

/// Ungültige `svc=` an Schichten, die roh erhalten bleiben (A311).
pub(crate) fn raw_svc(m: &Model) -> Vec<crate::RawSvc> {
    crate::catalog::raw_svc(&m.foreign, || write_known(m))
}

/// Was dieser Schreiber kennt, ohne Fremdes aus der gelesenen Datei.
fn write_known(m: &Model) -> String {
    let mut out = format!("SZO {VERSION}\n# Skizzeo-Projekt\n");
    let a = m.attr();
    let pen_guid = |id| a.pen(id).map(|p| p.guid);

    for p in sorted(a.pens().iter(), |p| p.guid) {
        write_pen(&mut out, p);
    }
    // Der Werks-Linientyp der Einbauelemente steht nur in der Datei, wenn er
    // geändert ist (F20, G7b); beim Lesen entsteht er wieder
    let werk = crate::attr::element_line_type();
    for l in sorted(a.line_types().iter(), |l| l.guid) {
        if *l == werk {
            continue;
        }
        write_line_type(&mut out, l);
    }
    for f in sorted(a.fills().iter(), |f| f.guid) {
        write_fill(&mut out, f);
    }
    for s in sorted(a.surfaces().iter(), |s| s.guid) {
        write_surface(&mut out, s);
    }
    darstellung::write(a, &mut out);

    let mat_guid = |id| m.material(id).map(|x| x.guid);
    let used = used_trades(
        m.materials().iter().map(|(_, x)| x),
        m.layer_sets().iter().map(|(_, t)| t),
    );
    write_trades(&mut out, m.trades(), &used);
    for x in sorted(m.materials().iter(), |x| x.guid) {
        let refs = [
            a.fill(x.cut_fill).map(|f| f.guid),
            pen_guid(x.cut_fg),
            pen_guid(x.cut_bg),
            a.surface(x.surface).map(|s| s.guid),
        ];
        write_material(&mut out, x, refs);
    }
    for s in sorted(m.layer_sets().iter(), |s| s.guid) {
        write_type(&mut out, s, mat_guid);
    }
    ausbau::write(&mut out, m);

    let storey_guid = |id| m.storey(id).map(|s| s.guid);
    let p = m.project();
    let defaults = m.defaults();
    let line = Line::new("project")
        .guid("guid", Some(p.guid))
        .text("name", &p.name)
        .guid("storey", storey_guid(defaults.storey))
        .guid(
            "wallset",
            m.layer_set(defaults.exterior_wall).map(|s| s.guid),
        )
        .guid("iwset", m.layer_set(defaults.interior_wall).map(|s| s.guid));
    // Bauvorhaben, Bauherr, Aufsteller nur, wenn gesetzt (BIM §3.12); seit
    // `[projectinfo]` nur die alten, bis zur ersten Änderung (Regel 110)
    let mut line = line;
    let alt = if p.info {
        [&p.legacy[0], &p.legacy[1], &p.legacy[2]]
    } else {
        [&p.site, &p.client, &p.author]
    };
    for (k, v) in ["site", "client", "author"].into_iter().zip(alt) {
        if !v.is_empty() {
            line = line.text(k, v);
        }
    }
    // Nummernzähler nur, wenn gelöscht wurde (Regel 25): sonst ergibt er
    // sich aus der höchsten Nummer
    let gaps = m.number_gaps();
    let line = if gaps.is_empty() {
        line
    } else {
        let v: Vec<String> = gaps.iter().map(|(p, n)| format!("{p}:{n}")).collect();
        line.word("next", &v.join(","))
    };
    // Bodenkennwerte an `[project]` nur noch, wie sie vor Regel 113 in der
    // Datei standen, bis zur ersten Änderung
    let mut line = line;
    for ((k, ..), v) in crate::qto_earth::Boden::FELDER.iter().zip(p.soil_legacy) {
        if let Some(v) = v {
            line = line.num(k, v);
        }
    }
    line.finish(&mut out);
    // Projektdaten (BIM §3.12a): nur gesetzte Felder, ohne Daten keine
    // Zeile; leer aber doch, solange alte Werte an `[project]` stehen, denn
    // sonst gälten diese beim nächsten Laden wieder (BIM-Routine 09.10.)
    let alte = p.legacy.iter().any(|v| !v.is_empty());
    if p.info && (!p.is_blank() || alte) {
        let mut line = Line::new("projectinfo").word("key", "project");
        for (k, v) in p.fields() {
            if !v.is_empty() {
                line = line.text(k, v);
            }
        }
        line.finish(&mut out);
    }
    // Bodenkennwerte der Erdarbeiten (Regel 113): `[soil]` mit dem, was von
    // der Vorgabe abweicht, ohne Abweichung keine Zeile; leer aber doch,
    // solange alte Werte an `[project]` stehen, denn sonst gälten diese beim
    // nächsten Laden wieder. Stehen die Werte noch an `[project]`, keine.
    let alte = p.soil_legacy.iter().any(Option::is_some);
    if p.soil_info || !alte {
        let vorgabe = crate::qto_earth::Boden::default().werte();
        let ab: Vec<(&str, f64)> = crate::qto_earth::Boden::FELDER
            .iter()
            .zip(p.soil.werte().into_iter().zip(vorgabe))
            .filter(|(_, (v, d))| v != d)
            .map(|((k, ..), (v, _))| (*k, v))
            .collect();
        // ohne kapillarbrechende Schicht `capillary_on=0`; Dateien davor
        // haben sie
        let ohne_kies = !p.soil.kies;
        if !ab.is_empty() || alte || ohne_kies {
            let mut line = Line::new("soil").word("key", "project");
            for (k, v) in ab {
                line = line.num(k, v);
            }
            if ohne_kies {
                line = line.num(crate::qto_earth::Boden::SCHALTER, 0.0);
            }
            line.finish(&mut out);
        }
    }
    gelaende::write(m, &mut out);
    lage::write(m, &mut out);
    for b in sorted(m.buildings().iter(), |b| b.guid) {
        let l = Line::new("building")
            .guid("guid", Some(b.guid))
            .text("name", &b.name)
            .text("number", &b.number);
        // Versatz OK Sohlplatte über Gelände (Gelände Thema 1): nur ≠ 0,
        // damit ältere Dateien bytegleich bleiben
        let l = if b.terrain != 0.0 {
            l.num("terrain", b.terrain)
        } else {
            l
        };
        l.finish(&mut out);
    }
    for s in sorted(m.storeys().iter(), |s| s.guid) {
        let l = Line::new("storey")
            .guid("guid", Some(s.guid))
            .guid(
                "building",
                s.building.and_then(|b| m.building(b)).map(|b| b.guid),
            )
            .text("name", &s.name)
            .text("short", &s.short)
            .word("kind", level_kind_word(s.kind))
            .num("elev", s.elevation)
            .num("h", s.height);
        // Gewollte Einbindetiefe (Gelände Thema 1) nur, wenn sie von der
        // tatsächlichen abweicht; ältere Dateien bleiben bytegleich
        let l = match s.embed {
            Some(e) => l.num("embed", e),
            None => l,
        };
        hoehen::write_lines(l, &s.lines).finish(&mut out);
    }
    // Höhenbezug als `Guid:u|o:Versatz`
    let level = |r: LevelRef| {
        let g = m
            .storey(r.storey)
            .map_or("-".to_string(), |s| s.guid.to_ifc());
        let e = match r.edge {
            LevelEdge::Bottom => "u",
            LevelEdge::Top => "o",
        };
        format!("{g}:{e}:{}", r.offset)
    };
    for r in sorted(m.runs().iter(), |r| r.guid) {
        let pts = r
            .points
            .iter()
            .map(|p| format!("{} {}", p.x, p.y))
            .collect::<Vec<_>>()
            .join(";");
        Line::new("run")
            .guid("guid", Some(r.guid))
            .guid("storey", storey_guid(r.storey))
            .word("base", &level(r.base))
            .word("top", &level(r.top))
            .word("ref", ref_side(r.ref_side))
            .flag("closed", r.closed)
            .text("pts", &pts)
            .finish(&mut out);
    }
    let walls = sorted(m.elements().iter(), |e| e.guid);
    let mat_guid = |id| m.material(id).map(|x| x.guid);
    for e in &walls {
        let ElementKind::Wall(w) = e.kind else {
            continue;
        };
        let line = Line::new("wall")
            .guid("guid", Some(e.guid))
            .guid("run", m.run(w.run).map(|r| r.guid))
            .num("seg", w.seg)
            .text("number", &e.number)
            .word("cat", category(e.category))
            .guid(
                "set",
                e.layer_set.and_then(|s| m.layer_set(s)).map(|s| s.guid),
            )
            .num("seq", e.seq)
            .guid("storey", storey_guid(e.storey));
        match w.coupling {
            Some(c) => {
                let line = line
                    .guid("below", m.element(c.below).map(|x| x.guid))
                    .num("off", c.offset);
                // Nur gelöste Segmente: Dateien ohne bleiben bytegleich
                if c.linked {
                    line
                } else {
                    line.word("link", "0")
                }
            }
            None => line,
        }
        .finish(&mut out);
    }
    for e in &walls {
        let ElementKind::GroundSlab(s) = e.kind else {
            continue;
        };
        let l = slab_type(
            Line::new("slab")
                .guid("guid", Some(e.guid))
                .guid("run", m.run(s.run).map(|r| r.guid))
                .text("number", &e.number)
                .word("cat", category(e.category)),
            m,
            e,
        )
        .guid("mat", mat_guid(s.material))
        .word("top", &level(s.top))
        .num("t", s.thickness)
        .num("recess", s.recess);
        // Perimeterdämmung (Gelände Thema 4): nur, wenn es sie gibt, damit
        // ältere Dateien bytegleich bleiben
        let l = if s.insulation > 0.0 {
            l.num("insulation", s.insulation)
        } else {
            l
        };
        l.num("seq", e.seq)
            .guid("storey", storey_guid(e.storey))
            .finish(&mut out);
    }
    for e in &walls {
        let ElementKind::StripFooting(f) = e.kind else {
            continue;
        };
        slab_type(
            Line::new("footing")
                .guid("guid", Some(e.guid))
                .guid("slab", m.element(f.slab).map(|x| x.guid))
                .text("number", &e.number)
                .word("cat", category(e.category)),
            m,
            e,
        )
        .guid("mat", mat_guid(f.material))
        .num("w", f.width)
        .word("base", &level(f.base))
        .num("seq", e.seq)
        .guid("storey", storey_guid(e.storey))
        .finish(&mut out);
    }
    for e in &walls {
        let ElementKind::Floor(f) = e.kind else {
            continue;
        };
        let mut l = slab_type(
            Line::new("floor")
                .guid("guid", Some(e.guid))
                .guid("run", m.run(f.run).map(|r| r.guid))
                .text("number", &e.number)
                .word("cat", category(e.category)),
            m,
            e,
        )
        .guid("mat", mat_guid(f.material))
        .word("top", &level(f.top))
        .num("t", f.thickness);
        // Untersichtdämmung (G7 K4): nur abweichend vom Standard, damit
        // Dateien ohne Vorsprung bytegleich bleiben
        if f.soffit.thickness != crate::model::SOFFIT_THICKNESS {
            l = l.num("soffit", f.soffit.thickness);
        }
        if let Some(sm) = f.soffit.material {
            l = l.guid("soffit_mat", mat_guid(sm));
        }
        // Bekleidung (W3): fehlt der Wert, gibt es keine (Dateien davor
        // bleiben bytegleich); Lattung und Überstand nur abweichend
        let d = Soffit::default();
        if f.soffit.cladding != 0.0 {
            l = l.num("soffit_clad", f.soffit.cladding);
        }
        if let Some(cm) = f.soffit.cladding_material {
            l = l.guid("soffit_clad_mat", mat_guid(cm));
        }
        for (k, v, d) in [
            ("soffit_drip", f.soffit.drip, d.drip),
            ("batten", f.soffit.batten, d.batten),
            ("counter", f.soffit.counter, d.counter),
        ] {
            if v != d {
                l = l.num(k, v);
            }
        }
        // Dachterrasse (D1–D3): ebenso nur abweichend
        if let Some(t) = f.terrace.build_up.and_then(|t| m.layer_set(t)) {
            l = l.guid("terrace", Some(t.guid));
        }
        if f.terrace.upstand != crate::model::TERRACE_UPSTAND {
            l = l.num("upstand", f.terrace.upstand);
        }
        if f.terrace.crown != crate::model::TERRACE_CROWN {
            l = l.num("crown", f.terrace.crown);
        }
        if f.terrace.inner != crate::model::ROOF_INNER {
            l = l.num("inner", f.terrace.inner);
        }
        if let Some(cm) = f.terrace.coping_mat {
            l = l.guid("coping_mat", mat_guid(cm));
        }
        l.num("seq", e.seq)
            .guid("storey", storey_guid(e.storey))
            .finish(&mut out);
    }
    for e in &walls {
        let ElementKind::EdgeStrip { wall, floor } = e.kind else {
            continue;
        };
        Line::new("strip")
            .guid("guid", Some(e.guid))
            .guid("wall", m.element(wall).map(|x| x.guid))
            .guid("floor", m.element(floor).map(|x| x.guid))
            .text("number", &e.number)
            .word("cat", category(e.category))
            .num("seq", e.seq)
            .guid("storey", storey_guid(e.storey))
            .finish(&mut out);
    }
    for e in &walls {
        let ElementKind::SoffitInsulation { floor } = e.kind else {
            continue;
        };
        Line::new("soffit")
            .guid("guid", Some(e.guid))
            .guid("floor", m.element(floor).map(|x| x.guid))
            .text("number", &e.number)
            .word("cat", category(e.category))
            .num("seq", e.seq)
            .guid("storey", storey_guid(e.storey))
            .finish(&mut out);
    }
    // Perimeterdämmung (Gelände Thema 4): nur, wenn es sie gibt
    for e in &walls {
        let ElementKind::PerimeterInsulation { slab } = e.kind else {
            continue;
        };
        Line::new("perimeter")
            .guid("guid", Some(e.guid))
            .guid("slab", m.element(slab).map(|x| x.guid))
            .text("number", &e.number)
            .word("cat", category(e.category))
            .num("seq", e.seq)
            .guid("storey", storey_guid(e.storey))
            .finish(&mut out);
    }
    // Dachterrasse und Attikablech (D1–D3), Dachaufbau des Flachdachs
    // (Jörn 10.10.): nur, wenn es sie gibt
    for e in &walls {
        let (word, floor) = match e.kind {
            ElementKind::RoofTerrace { floor } => ("terrace", floor),
            ElementKind::Coping { floor } => ("coping", floor),
            ElementKind::Roof { floor, .. } => ("roof", floor),
            _ => continue,
        };
        let l = Line::new(word)
            .guid("guid", Some(e.guid))
            .guid("floor", m.element(floor).map(|x| x.guid))
            .text("number", &e.number)
            .word("cat", category(e.category));
        let l = if word != "coping" {
            slab_type(l, m, e)
        } else {
            l
        };
        let l = match &e.kind {
            ElementKind::Roof { drainage, .. } => write_drainage(l, drainage),
            _ => l,
        };
        l.num("seq", e.seq)
            .guid("storey", storey_guid(e.storey))
            .finish(&mut out);
    }
    // Regenfallrohre (Jörn 11.10.): nur, wenn es sie gibt
    dach::write_downpipes(m, &walls, &mut out);
    oeffnung::write(m, &walls, &mut out);
    // Erweiterungsbauteile (E3): Definition vollständig, je Exemplar seine
    // Lage und eigenen Werte; unlesbare Zeilen der gelesenen Datei bleiben
    for d in m.ext_defs() {
        Line::new("extdef")
            .word("key", &d.key)
            .num("version", d.version)
            .text("text", &d.text)
            .finish(&mut out);
    }
    for e in &walls {
        let ElementKind::Ext(p) = &e.kind else {
            continue;
        };
        let mut l = Line::new("extpart")
            .guid("guid", Some(e.guid))
            .word("key", &p.key)
            .text("number", &e.number)
            .num("x", p.at[0])
            .num("y", p.at[1]);
        if p.rot != 0.0 {
            l = l.num("rot", p.rot);
        }
        if let Some(t) = &p.typ {
            l = l.text("typ", t);
        }
        if !p.werte.is_empty() {
            l = l.text("werte", &p.werte_text());
        }
        l.num("seq", e.seq)
            .guid("storey", storey_guid(e.storey))
            .finish(&mut out);
    }
    raum::write_spaces(m, &walls, &mut out);
    for raw in m.ext_raw() {
        out.push_str(raw);
        out.push('\n');
    }
    for e in &walls {
        write_props(&mut out, "prop", "elem", e.guid, &e.props);
    }
    // Schnitte nur, wenn sie einmal gezeigt wurden: sonst bleibt die Datei
    // bytegleich; `active=1` nur, wenn zuletzt nicht A gezeigt wurde
    for (i, (name, c)) in crate::model::CUT_NAMES.iter().zip(m.cuts()).enumerate() {
        if let Some(pos) = c.pos {
            let mut l = Line::new("cut")
                .word("name", name)
                .num("pos", pos)
                .flag("flip", c.flip);
            if i != 0 && i == m.active_cut() {
                l = l.flag("active", true);
            }
            l.finish(&mut out);
        }
    }
    // Sonnenstand (S4) erst, wenn das System in der Datei einmal an war;
    // eine unlesbare Zeile bleibt roh, bis ein Stand sie ersetzt
    if let Some(s) = m.sun() {
        let mut l = Line::new("sun")
            .word("date", &s.date_text())
            .word("time", &s.time_text());
        if s.on {
            l = l.flag("on", true);
        }
        l.finish(&mut out);
    } else if let Some(raw) = m.sun_raw() {
        out.push_str(raw);
        out.push('\n');
    }
    // Schatten der Ansichten (S7) nur mit eigener Wahl; unlesbare Zeilen
    // bleiben roh
    for (i, name) in crate::SHADE_VIEWS.iter().enumerate() {
        if let Some(v) = m.view_shade_own(i) {
            Line::new("viewshade")
                .word("view", name)
                .flag("on", v.on)
                .word("fill", v.fill_text())
                .word("light", v.light_text())
                .finish(&mut out);
        }
    }
    for raw in m.view_shade_raw() {
        out.push_str(raw);
        out.push('\n');
    }
    // Unter Gelände gestrichelt (S11) nur, wo gewählt
    for (i, name) in crate::SHADE_VIEWS.iter().enumerate() {
        if m.view_below(i) {
            Line::new("viewbelow")
                .word("view", name)
                .flag("dashed", true)
                .finish(&mut out);
        }
    }
    for raw in m.view_below_raw() {
        out.push_str(raw);
        out.push('\n');
    }
    sicht::write(m, &mut out);
    bemassung::write(m, &mut out);
    hoehen::write_plan_cut(m, &mut out);
    // Ausgeblendetes (Paket 3 §3.6) nur, wenn es etwas gibt; Isolieren nie
    let v = m.visibility();
    for g in &v.hidden {
        if m.elements().iter().any(|(_, e)| e.guid == *g) {
            Line::new("hide").guid("elem", Some(*g)).finish(&mut out);
        }
    }
    for c in &v.hidden_cat {
        Line::new("hide")
            .word("cat", crate::kinds::spec(*c).szo)
            .finish(&mut out);
    }
    for t in &v.hidden_trade {
        if m.trade(*t).is_some() {
            Line::new("hide").guid("trade", Some(t.0)).finish(&mut out);
        }
    }
    ausbauart::write(m, &mut out);
    if v.terrain_hidden {
        Line::new("hide").flag("terrain", true).finish(&mut out);
    }
    // Gesperrtes (Paket 4 §2.3): eine Zeile je Bauteil, nur wenn es eins gibt
    for g in m.locked_in_order() {
        Line::new("lock").guid("elem", Some(g)).finish(&mut out);
    }
    // Baustoffkennwerte (Paket 5 §2.3) nur, wenn gesetzt
    for x in sorted(m.materials().iter(), |x| x.guid) {
        crate::matprop::write_lines(&mut out, x.guid, &x.props);
    }
    // Muster (Paket 6 §2.2): je Oberfläche mit Muster eine Zeile; eine
    // Werks-Oberfläche ohne Muster schreibt die Abwahl (Regel 60)
    for s in sorted(a.surfaces().iter(), |s| s.guid) {
        match &s.pattern {
            Some(p) => crate::proctex::write_line(&mut out, s.guid, Some(p)),
            None if crate::proctex::factory_for(s.guid).is_some() => {
                crate::proctex::write_line(&mut out, s.guid, None)
            }
            None => {}
        }
    }
    out
}

/// Zeilen `[pattern]` an die gelesenen Oberflächen (`.szo` und `.szk`,
/// Regeln 57, 59, 60). Je Oberfläche gilt die erste Zeile; weitere bleiben
/// mit Hinweis unverändert stehen (ihre Zeilennummern kommen nach `keep`).
/// Ein Verweis auf eine unbekannte Oberfläche und falsche Werte werden mit
/// Hinweis verworfen. Gibt die Oberflächen mit einer gültigen oder
/// abwählenden Zeile zurück.
pub(crate) fn read_patterns(
    recs: &[Record],
    lines: &[&str],
    ids: &HashMap<Guid, Id<Surface>>,
    surfaces: &mut Arena<Surface>,
    hints: &mut Vec<String>,
    keep: &mut Vec<usize>,
) -> Vec<Guid> {
    let mut done: Vec<Guid> = Vec::new();
    for r in recs {
        let g = r.opt("surface").and_then(Guid::from_ifc);
        let Some((g, id)) = g.and_then(|g| ids.get(&g).map(|id| (g, *id))) else {
            r.skip();
            hints.push(format!(
                "Zeile {}: Muster für unbekannte Oberfläche, verworfen",
                r.line
            ));
            continue;
        };
        let name = surfaces.get(id).map_or(String::new(), |s| s.name.clone());
        if done.contains(&g) {
            r.skip();
            keep.push(r.line);
            hints.push(format!(
                "Zeile {}: zweites Muster für „{name}“, es gilt das erste",
                r.line
            ));
            continue;
        }
        match crate::proctex::read_line(r, lines[r.line - 1]) {
            Ok(p) => {
                done.push(g);
                if let Some(s) = surfaces.get_mut(id) {
                    s.pattern = p;
                }
            }
            Err(e) => {
                // Die Oberfläche bleibt ohne Muster, auch eine Werks-Oberfläche
                // bekommt nicht still ihr Werksmuster; die Zeile bleibt
                // bytegleich stehen, bis man ein Muster setzt (Review 3q/3, 3s)
                done.push(g);
                if let Some(s) = surfaces.get_mut(id) {
                    s.pattern = Some(crate::proctex::Pattern::Foreign(
                        lines[r.line - 1].to_string(),
                    ));
                }
                r.skip();
                hints.push(format!(
                    "Zeile {}: Muster für „{name}“ ungültig ({e}); bleibt unverändert, ohne Muster",
                    r.line
                ));
            }
        }
    }
    done
}

/// Zeilen `[matprop]` an die gelesenen Baustoffe (`.szo` und `.szk`).
pub(crate) fn read_matprops(
    recs: &[Record],
    mat_ids: &HashMap<Guid, Id<Material>>,
    materials: &mut Arena<Material>,
    hints: &mut Vec<String>,
) {
    for r in recs {
        let x = r
            .opt("mat")
            .and_then(Guid::from_ifc)
            .and_then(|g| mat_ids.get(&g))
            .and_then(|&id| materials.get_mut(id));
        let res = match x {
            Some(x) => crate::matprop::read_line(r, x.category, &mut x.props),
            None => {
                for k in ["key", "value", "num", "bool", "unit"] {
                    r.opt(k);
                }
                Err(format!(
                    "Zeile {}: Kennwert für unbekannten Baustoff, verworfen",
                    r.line
                ))
            }
        };
        if let Err(h) = res {
            hints.push(h);
        }
    }
}

// --- Lesen ----------------------------------------------------------------

/// Guid → Kennung, mit Fehler bei doppelter Guid.
pub(crate) fn register<T>(
    table: &mut HashMap<Guid, Id<T>>,
    seen: &mut HashMap<Guid, usize>,
    r: &Record,
    g: Guid,
    id: Id<T>,
) -> Result<(), LoadError> {
    if let Some(first) = seen.insert(g, r.line) {
        return Err(err(
            r.line,
            format!("Guid {} doppelt (schon in Zeile {first})", g.to_ifc()),
        ));
    }
    table.insert(g, id);
    Ok(())
}

/// Lage, Typ und eigene Werte eines `[extpart]`.
fn read_ext_part(r: &Record) -> Result<ExtPart, LoadError> {
    let werte = match r.opt("werte") {
        None => Vec::new(),
        Some(t) => sk_szb::pruefen::typ_werte(t)
            .map_err(|e| err(r.line, format!("[extpart]: „werte“ ungültig ({e})")))?,
    };
    if werte.iter().any(|(_, v)| !v.is_finite()) {
        return Err(r.bad("werte", "Zahl"));
    }
    Ok(ExtPart {
        key: r.get("key")?.to_string(),
        at: [r.f64("x")?, r.f64("y")?],
        rot: match r.opt("rot") {
            Some(_) => r.f64("rot")?,
            None => 0.0,
        },
        typ: r.opt("typ").map(str::to_string),
        werte,
    })
}

/// Fremdes einer gelesenen Datei (F-17, F-17b, wie im Firmenkatalog):
/// Zeilen bekannter Sätze mit unbekannten Schlüsseln oder Werten, gepaart
/// mit der Zeile, die dieser Schreiber für denselben Satz schreibt, und die
/// Sätze unbekannter Art in Dateireihenfolge.
fn foreign(
    model: &Model,
    by: &HashMap<&str, Vec<Record>>,
    lines: &[&str],
    alien: &[usize],
) -> crate::catalog::Foreign {
    use crate::catalog::record_key;
    let mut f = crate::catalog::Foreign::default();
    let mut odd = Vec::new();
    let mut count = HashMap::new();
    for r in by.values().flatten() {
        let n = r.unknown();
        let key = record_key(r, &mut count);
        if n > 0 {
            f.unknown += n;
            odd.push((key, lines[r.line - 1]));
        }
    }
    if !odd.is_empty() {
        let mine = write_known(model);
        let own = crate::catalog::own_lines(&mine);
        odd.sort();
        for (key, theirs) in odd {
            if let Some((l, nth)) = own.get(&key) {
                f.lines.push((l.to_string(), *nth, theirs.to_string()));
            }
        }
    }
    f.unknown += alien.len();
    f.records = alien.iter().map(|&n| lines[n - 1].to_string()).collect();
    f
}

/// Liest eine `.szo`-Datei. Neue Laufzeit-Kennungen, Guids aus der Datei; neue
/// Guids kommen aus `guids`. Bei einem Fehler wird nichts übernommen.
pub fn read(text: &str, guids: GuidGen) -> Result<Loaded, LoadError> {
    read_with(text, guids, &[])
}

/// Wie [`read`]; die Abschnitte `ext` kommen roh in den Erweiterungsspeicher
/// ([`crate::ExtStore`], KA-0b) statt ins Fremde und werden in dieser
/// Reihenfolge hinter den bekannten Abschnitten geschrieben.
pub fn read_with(text: &str, mut guids: GuidGen, ext: &[&str]) -> Result<Loaded, LoadError> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    check_header(text.lines().next(), "SZO", VERSION)?;
    // SZO 1: vor der Geschossverwaltung (B11), SZO 2: vor den Gebäuden (B12);
    // beide werden beim Lesen umgestellt
    let version: u32 = text
        .lines()
        .next()
        .and_then(|l| l.split_whitespace().nth(1))
        .and_then(|v| v.parse().ok())
        .unwrap_or(VERSION);
    let v1 = version == 1;
    let v3 = version >= 3;
    let mut hints = Vec::new();
    let lines: Vec<&str> = text.lines().collect();
    // Sätze unbekannter Art (neuere Fassung, F-17b): roh behalten, ohne Hinweis
    let mut alien: Vec<usize> = Vec::new();
    let mut ext_lines: Vec<(String, &str)> = Vec::new();
    let mut by: HashMap<&str, Vec<Record>> = HashMap::new();
    const KNOWN: [&str; 49] = [
        "terrain",
        "tpoint",
        "tline",
        "tcode",
        "underlay",
        "opening",
        "pen",
        "linetype",
        "fill",
        "surface",
        "display",
        "trade",
        "material",
        "layerset",
        "layer",
        "typeprop",
        "project",
        "projectinfo",
        "soil",
        "terrainplate",
        "location",
        "building",
        "storey",
        "run",
        "wall",
        "slab",
        "footing",
        "floor",
        "strip",
        "soffit",
        "terrace",
        "coping",
        "perimeter",
        "roof",
        "downpipe",
        "extdef",
        "extpart",
        "space",
        "prop",
        "cut",
        "sun",
        "viewshade",
        "viewbelow",
        "viewhide",
        "plancut",
        "hide",
        "lock",
        "matprop",
        "pattern",
    ];
    for (i, l) in text.lines().enumerate().skip(1) {
        let Some(r) = Record::parse(i + 1, l)? else {
            continue;
        };
        match ausbau::known(&KNOWN, &r.section)
            .or_else(|| bemassung::known(&r.section))
            .or_else(|| flaeche::known(&r.section))
        {
            Some(k) => by.entry(k).or_default().push(r),
            None if ext.contains(&r.section.as_str()) => ext_lines.push((r.section.clone(), l)),
            None => alien.push(i + 1),
        }
    }
    let empty = Vec::new();
    let recs = |k: &str| by.get(k).unwrap_or(&empty);
    let mut seen = HashMap::new();

    // Attribute
    let mut pens = Arena::new();
    let mut pen_ids = HashMap::new();
    let mut numbers = HashMap::new();
    for r in recs("pen") {
        let p = read_pen(r)?;
        if let Some(first) = numbers.insert(p.number, r.line) {
            return Err(err(
                r.line,
                format!("Stiftnummer {} doppelt (schon in Zeile {first})", p.number),
            ));
        }
        let g = p.guid;
        let id = pens.insert(p);
        register(&mut pen_ids, &mut seen, r, g, id)?;
    }
    if pens.is_empty() {
        return Err(err(0, "Attribute fehlen: die Datei enthält keine Stifte"));
    }
    let mut line_types = Arena::new();
    let mut lt_ids = HashMap::new();
    for r in recs("linetype") {
        let l = read_line_type(r)?;
        let g = l.guid;
        let id = line_types.insert(l);
        register(&mut lt_ids, &mut seen, r, g, id)?;
    }
    let mut fills = Arena::new();
    let mut fill_ids = HashMap::new();
    for r in recs("fill") {
        let f = read_fill(r, &mut hints)?;
        let g = f.guid;
        let id = fills.insert(f);
        register(&mut fill_ids, &mut seen, r, g, id)?;
    }
    let mut surfaces = Arena::new();
    let mut surface_ids = HashMap::new();
    for r in recs("surface") {
        let s = read_surface(r)?;
        let g = s.guid;
        let id = surfaces.insert(s);
        register(&mut surface_ids, &mut seen, r, g, id)?;
    }
    // Muster (Paket 6); Werks-Oberflächen ohne Zeile zeigen ihr Werksmuster
    // ab dem Öffnen und schreiben es beim nächsten Speichern
    let patterned = read_patterns(
        recs("pattern"),
        &lines,
        &surface_ids,
        &mut surfaces,
        &mut hints,
        &mut alien,
    );
    // Dateien vor Paket 6a haben gar keinen `[pattern]`-Satz und zufällige
    // Guids an den Werks-Oberflächen: einmalig nach dem Namen; das nächste
    // Speichern schreibt das Muster aus, danach hängt es an der Guid
    // (BIM-Befund zu cb8eda4, Regel 60)
    let alt = recs("pattern").is_empty();
    for id in surface_ids.values() {
        if let Some(s) = surfaces.get_mut(*id) {
            if !patterned.contains(&s.guid) && s.pattern.is_none() {
                s.pattern = crate::proctex::factory_for(s.guid)
                    .or_else(|| alt.then(|| crate::proctex::factory(&s.name)).flatten());
            }
        }
    }
    // Vor E16: Stift 9 „Hintergrund“ ergänzen, wenn die Datei keinen hat
    let has_background = recs("display")
        .iter()
        .any(|r| r.get("slot").is_ok_and(|s| s == "background"));
    if !has_background && !numbers.contains_key(&crate::attr::BACKGROUND_PEN) {
        let (std_attr, _) = crate::attr::defaults(&mut GuidGen::with_seed(0));
        if let Some(p) = std_attr.pen(std_attr.display().background.pen) {
            pens.insert(Pen {
                guid: guids.next_guid(),
                ..p.clone()
            });
        }
    }
    // Vor Paket 6: Stift „Ansichtsmuster“ ergänzen, wenn die Datei keine
    // Fugen-Darstellung hat
    let has_pattern = recs("display")
        .iter()
        .any(|r| r.get("slot").is_ok_and(|s| s == "pattern"));
    let pattern_pen = (!has_pattern).then(|| {
        let g = crate::attr::PATTERN_PEN_GUID;
        match pen_ids.get(&g) {
            Some(id) => *id,
            None => {
                let taken = |n: u16| pens.iter().any(|(_, p)| p.number == n);
                let p = crate::attr::pattern_pen(g, taken);
                pens.insert(p)
            }
        }
    });
    let werk = darstellung::werk_linientyp(recs("display"), &pen_ids, &mut line_types, &mut lt_ids);
    let (display, display_raw) = darstellung::read(
        recs("display"),
        &lines,
        &pens,
        &line_types,
        &pen_ids,
        &lt_ids,
        (pattern_pen, werk),
        &mut hints,
    )?;

    // Bibliothek
    let mut trades = read_trades(recs("trade"))?;
    hints.extend(trade::dedup_shorts(&mut trades));
    let mut materials = Arena::new();
    let mut mat_ids = HashMap::new();
    for r in recs("material") {
        let x = read_material(r, &fill_ids, &pen_ids, &surface_ids)?;
        let g = x.guid;
        let id = materials.insert(x);
        register(&mut mat_ids, &mut seen, r, g, id)?;
    }
    // Kennwerte (Paket 5): Falsches und Unbekanntes mit Hinweis verworfen
    read_matprops(recs("matprop"), &mat_ids, &mut materials, &mut hints);
    let (mut layer_sets, set_ids, passed) = read_types(&by, &mat_ids, &mut seen, version >= 4)?;
    hints.extend(passed.hints);

    // Projekt, Gebäude und Geschosse
    let mut buildings = Arena::new();
    let mut building_ids = HashMap::new();
    let mut building_numbers: HashMap<String, usize> = HashMap::new();
    for r in recs("building") {
        // vor Gelände Thema 1 ohne: OK Sohlplatte auf OK Gelände
        let mut terrain = match r.opt("terrain") {
            Some(_) => r.f64("terrain")?,
            None => 0.0,
        };
        let max = crate::model::MAX_TERRAIN_OFFSET;
        if !(terrain.is_finite() && terrain.abs() <= max) {
            hints.push(ausserhalb(
                r.line,
                "OK Sohle über Gelände",
                terrain,
                -max,
                max,
                " mm",
            ));
            terrain = 0.0;
        }
        let b = Building {
            guid: r.guid("guid")?,
            name: r.get("name")?.to_string(),
            number: r.get("number")?.to_string(),
            terrain,
        };
        if let Some(first) = building_numbers.insert(b.number.clone(), r.line) {
            return Err(err(
                r.line,
                format!(
                    "Gebäudenummer {} doppelt (schon in Zeile {first})",
                    b.number
                ),
            ));
        }
        let g = b.guid;
        let id = buildings.insert(b);
        register(&mut building_ids, &mut seen, r, g, id)?;
    }
    let mut storeys = Arena::new();
    let mut storey_ids = HashMap::new();
    for r in recs("storey") {
        let building = match r.opt("building") {
            Some(_) => r.link_opt("building", &building_ids)?,
            None => None,
        };
        let s = if v1 {
            // Das eine Geschoss von SZO 1 wird das Erdgeschoss
            let _ = (r.opt("name"), r.opt("elev"), r.opt("height"));
            Storey {
                guid: r.guid("guid")?,
                building,
                name: "Erdgeschoss".into(),
                short: "EG".into(),
                kind: LevelKind::Storey,
                elevation: 0.0,
                height: crate::model::STOREY_HEIGHT,
                embed: None,
                lines: Default::default(),
            }
        } else {
            Storey {
                guid: r.guid("guid")?,
                building,
                name: r.get("name")?.to_string(),
                short: r.get("short")?.to_string(),
                kind: keyword(
                    r,
                    "kind",
                    &[LevelKind::Foundation, LevelKind::Storey, LevelKind::Roof],
                    level_kind_word,
                )?,
                elevation: r.f64("elev")?,
                height: r.f64("h")?,
                embed: match r.opt("embed") {
                    Some(_) => Some(r.f64("embed")?),
                    None => None,
                },

                lines: hoehen::read_lines(r, &mut hints),
            }
        };
        let g = s.guid;
        let id = storeys.insert(s);
        register(&mut storey_ids, &mut seen, r, g, id)?;
    }
    let p = match recs("project").as_slice() {
        [r] => r,
        [] => return Err(err(0, "[project] fehlt")),
        [_, r, ..] => return Err(err(r.line, "[project] doppelt")),
    };
    let text = |k: &str| p.opt(k).unwrap_or("").to_string();
    let mut project = Project::new(p.guid("guid")?, p.get("name")?);
    // Bodenkennwerte der Erdarbeiten (Regel 113): `[soil]` gilt; sonst die
    // Schlüssel an `[project]` wie davor, die bis zur ersten Änderung dort
    // stehen bleiben. Fehlende gelten mit der Vorgabe; ein Wert außerhalb
    // seines Bereichs mit Hinweis ebenso, das Projekt öffnet
    let felder = crate::qto_earth::Boden::FELDER;
    let mut boden_wert = |r: &Record, i: usize| {
        let (k, name, min, max) = felder[i];
        let einheit = if k == "slope" { "°" } else { " mm" };
        gelaende::bereich(r, k, name, (min, max), einheit, &mut hints)
    };
    let mut legacy = [None; 5];
    for (i, w) in legacy.iter_mut().enumerate() {
        *w = boden_wert(p, i);
    }
    let mut boden = project.soil.werte();
    let mut kies = true;
    match recs("soil").as_slice() {
        [] => {
            for (b, w) in boden.iter_mut().zip(legacy) {
                if let Some(w) = w {
                    *b = w;
                }
            }
        }
        [r] => {
            // Kennung `key=project`; gelesen, damit sie nicht als fremd gilt
            let _ = r.opt("key");
            for (i, b) in boden.iter_mut().enumerate() {
                if let Some(w) = boden_wert(r, i) {
                    *b = w;
                }
            }
            kies = gelaende::bereich(
                r,
                crate::qto_earth::Boden::SCHALTER,
                "Kapillarbrechende Schicht eingebaut",
                (0.0, 1.0),
                "",
                &mut hints,
            )
            .is_none_or(|v| v != 0.0);
            project.soil_info = true;
        }
        [_, r, ..] => return Err(err(r.line, "[soil] doppelt")),
    }
    project.soil = crate::qto_earth::Boden {
        kies,
        ..crate::qto_earth::Boden::aus_werten(boden).unwrap_or_default()
    };
    project.soil_legacy = legacy;
    gelaende::read_project(&mut project, &|k| recs(k).as_slice(), &lines, &mut hints)?;
    // Regel 110: `[projectinfo]` gilt; sonst die Schlüssel an `[project]`
    let alt = [text("site"), text("client"), text("author")];
    match recs("projectinfo").as_slice() {
        [] => [project.site, project.client, project.author] = alt,
        [i] => {
            // Kennung `key=project`; gelesen, damit sie nicht als fremd gilt
            let _ = i.opt("key");
            let t = |k: &str| i.opt(k).unwrap_or("").to_string();
            project.kind = t("kind");
            project.number = t("projno");
            project.site = t("site");
            project.place = t("place");
            project.client = t("client");
            project.client_addr = t("clientaddr");
            project.author = t("author");
            project.author_addr = t("authoraddr");
            project.info = true;
            project.legacy = alt;
        }
        [_, r, ..] => return Err(err(r.line, "[projectinfo] doppelt")),
    }
    // Lage und Nordrichtung (Sonnenstand S1). Was nicht als Lage zählt (ein
    // Wert, der keine Zahl im Bereich ist, oder keine bekannte Angabe),
    // öffnet das Projekt trotzdem und bleibt bytegleich stehen (Befund A der
    // Abnahme S1, Review 3br)
    let (location, foot, location_raw) = lage::read(recs("location"), &lines, &mut hints)?;
    // Nummernzähler (Regel 25), z. B. „IW:1,GB:2“; fehlt er, gilt die höchste
    // vorhandene Nummer
    let mut counters = Vec::new();
    for part in p
        .opt("next")
        .unwrap_or("")
        .split(',')
        .filter(|x| !x.is_empty())
    {
        match part
            .split_once(':')
            .and_then(|(k, n)| Some((k, n.parse::<u32>().ok()?)))
        {
            Some((k, n)) => counters.push((k.to_string(), n)),
            None => hints.push(format!(
                "Zeile {}: Nummernzähler „{part}“ übersprungen",
                p.line
            )),
        }
    }
    if let Some(first) = seen.insert(project.guid, p.line) {
        return Err(err(
            p.line,
            format!(
                "Guid {} doppelt (schon in Zeile {first})",
                project.guid.to_ifc()
            ),
        ));
    }
    let exterior_wall = p.link("wallset", &set_ids)?;
    // Dateien vor B5a kennen keinen Innenwand-Aufbau: aus dem Kern der Außenwand anlegen
    let interior_wall = match p.opt("iwset") {
        Some(_) => p.link("iwset", &set_ids)?,
        None => {
            let mat = layer_sets.get(exterior_wall).and_then(|s| {
                s.layers
                    .iter()
                    .find(|l| l.core)
                    .or(s.layers.first())
                    .map(|l| l.material)
            });
            let mat = mat
                .or_else(|| materials.ids().next())
                .ok_or_else(|| err(p.line, "[project]: kein Baustoff für den Innenwand-Aufbau"))?;
            layer_sets.insert(crate::model::interior_set(guids.next_guid(), mat))
        }
    };
    let defaults = Defaults {
        storey: p.link("storey", &storey_ids)?,
        exterior_wall,
        interior_wall,
    };
    // SZO 1: Gründung und Obergeschoss ergänzen (Maße folgen unten)
    let (ground, found_level) = if v1 {
        let eg = defaults.storey;
        let gr = storeys.insert(Storey {
            guid: guids.next_guid(),
            building: None,
            name: "Gründung".into(),
            short: "GR".into(),
            kind: LevelKind::Foundation,
            elevation: -crate::model::FOUNDATION_DEPTH,
            height: crate::model::FOUNDATION_DEPTH,
            embed: None,
            lines: Default::default(),
        });
        storeys.insert(Storey {
            guid: guids.next_guid(),
            building: None,
            name: "Obergeschoss".into(),
            short: "OG".into(),
            kind: LevelKind::Storey,
            elevation: crate::model::STOREY_HEIGHT,
            height: crate::model::STOREY_HEIGHT,
            embed: None,
            lines: Default::default(),
        });
        hints.push("Datei auf die neuen Geschosse umgestellt".to_string());
        (eg, gr)
    } else {
        let b = storeys
            .get(defaults.storey)
            .and_then(|s: &Storey| s.building);
        let gr = storeys
            .iter()
            .find(|(_, s)| s.kind == LevelKind::Foundation && s.building == b)
            .map(|(id, _)| id)
            .ok_or_else(|| err(0, "[storey]: Gründung fehlt"))?;
        (defaults.storey, gr)
    };
    let level = |r: &Record, key: &str, default: LevelRef| -> Result<LevelRef, LoadError> {
        if v1 {
            let _ = r.opt(key);
            return Ok(default);
        }
        let v = r.get(key)?;
        let mut it = v.split(':');
        let (Some(g), Some(e), Some(o), None) = (it.next(), it.next(), it.next(), it.next()) else {
            return Err(r.bad(key, "Guid:u|o:Versatz"));
        };
        let storey = Guid::from_ifc(g)
            .and_then(|g| storey_ids.get(&g).copied())
            .ok_or_else(|| {
                err(
                    r.line,
                    format!("[{}]: „{key}“ verweist auf unbekanntes Geschoss", r.section),
                )
            })?;
        let edge = match e {
            "u" => LevelEdge::Bottom,
            "o" => LevelEdge::Top,
            _ => return Err(r.bad(key, "Guid:u|o:Versatz")),
        };
        let offset = o
            .parse::<f64>()
            .ok()
            .filter(|v| v.is_finite())
            .ok_or_else(|| r.bad(key, "Guid:u|o:Versatz"))?;
        Ok(LevelRef {
            storey,
            edge,
            offset,
        })
    };

    // Wandzüge und Wände
    let mut runs = Arena::new();
    let mut run_ids = HashMap::new();
    for r in recs("run") {
        let pts = r.get("pts")?;
        let points = pts
            .split(';')
            .map(|p| {
                let v: Vec<f64> = p
                    .split_whitespace()
                    .map(|x| x.parse().ok().filter(|v: &f64| v.is_finite()))
                    .collect::<Option<_>>()?;
                match v[..] {
                    [x, y] => Some(vec3(x, y, 0.0)),
                    _ => None,
                }
            })
            .collect::<Option<Vec<_>>>()
            .ok_or_else(|| r.bad("pts", "x y;x y;…"))?;
        let storey = r.link("storey", &storey_ids)?;
        // SZO 1/2: Wände an UK/OK ihres Geschosses, die Höhe wird verworfen
        let top = if v3 {
            level(r, "top", LevelRef::top(storey))?
        } else {
            let _ = r.opt("h");
            LevelRef::top(storey)
        };
        let run = WallRun {
            guid: r.guid("guid")?,
            points,
            closed: r.flag("closed")?,
            ref_side: keyword(
                r,
                "ref",
                &[RefSide::Left, RefSide::Right, RefSide::Center],
                ref_side,
            )?,
            base: level(r, "base", LevelRef::bottom(ground))?,
            top,
            storey,
            segments: Vec::new(),
        };
        let g = run.guid;
        let id = runs.insert(run);
        register(&mut run_ids, &mut seen, r, g, id)?;
    }
    let mut elements = Arena::new();
    let mut elem_ids = HashMap::new();
    let mut slots: HashMap<Id<WallRun>, Vec<(u32, Id<Element>)>> = HashMap::new();
    let mut taken_numbers: HashMap<String, usize> = HashMap::new();
    let mut below = Vec::new();
    for r in recs("wall") {
        let run = r.link("run", &run_ids)?;
        let seg: u32 = r.int("seg")?;
        let number = r.get("number")?.to_string();
        if let Some(first) = taken_numbers.insert(number.clone(), r.line) {
            return Err(err(
                r.line,
                format!("Bauteilnummer {number} doppelt (schon in Zeile {first})"),
            ));
        }
        let e = Element {
            guid: r.guid("guid")?,
            number,
            category: keyword(r, "cat", &Category::ALL, category)?,
            storey: r.link("storey", &storey_ids)?,
            layer_set: r.link_opt("set", &set_ids)?,
            seq: r
                .opt("seq")
                .map_or(Ok(crate::model::WALL_SEQ), |_| r.int("seq"))?,
            kind: ElementKind::Wall(Wall {
                run,
                seg,
                coupling: None,
            }),
            props: Default::default(),
            locked: false,
        };
        if r.opt("below").is_some() {
            let linked = match r.opt("link") {
                None | Some("1") => true,
                Some("0") => false,
                Some(v) => {
                    return Err(err(
                        r.line,
                        format!("[wall]: „link“ muss 0 oder 1 sein, nicht „{v}“"),
                    ))
                }
            };
            below.push((r.line, r.guid("below")?, r.f64("off")?, linked, e.guid));
        }
        let g = e.guid;
        let id = elements.insert(e);
        register(&mut elem_ids, &mut seen, r, g, id)?;
        slots.entry(run).or_default().push((seg, id));
    }
    // Kopplungen erst, wenn alle Wände gelesen sind (Verweise nach vorn)
    for (line, g, offset, linked, me) in below {
        let target = elem_ids.get(&g).copied().filter(|id| {
            matches!(
                elements.get(*id).map(|e: &Element| &e.kind),
                Some(ElementKind::Wall(_))
            )
        });
        let Some(target) = target else {
            return Err(err(
                line,
                format!("[wall]: „below“ verweist auf keine Wand ({})", g.to_ifc()),
            ));
        };
        if !offset.is_finite() {
            return Err(err(line, "[wall]: „off“ ist keine Zahl"));
        }
        if let Some(ElementKind::Wall(w)) = elements.get_mut(elem_ids[&me]).map(|e| &mut e.kind) {
            w.coupling = Some(Coupling {
                below: target,
                offset,
                linked,
            });
        }
    }
    // Gründung: erst die Platten, dann die Schürzen, die auf sie verweisen;
    // dann die Decken über den Zügen
    let mut number = |r: &Record| -> Result<String, LoadError> {
        let n = r.get("number")?.to_string();
        if let Some(first) = taken_numbers.insert(n.clone(), r.line) {
            return Err(err(
                r.line,
                format!("Bauteilnummer {n} doppelt (schon in Zeile {first})"),
            ));
        }
        Ok(n)
    };
    let mut verworfen = Vec::new();
    let mut schloesser = Vec::new();
    for (section, ix) in [
        ("slab", 0),
        ("footing", 1),
        ("floor", 2),
        ("strip", 3),
        ("soffit", 4),
        ("terrace", 5),
        ("coping", 6),
        ("perimeter", 7),
        ("roof", 8),
        ("downpipe", 9),
        ("opening", 10),
    ] {
        for r in recs(section) {
            let mut locks = Vec::new();
            let kind = if ix == 10 {
                match oeffnung::read(r, &storey_ids, &mut hints)? {
                    Some((k, l)) => {
                        locks = l;
                        k
                    }
                    None => {
                        verworfen.push(r);
                        continue;
                    }
                }
            } else if ix == 7 {
                // Perimeterdämmung (Gelände Thema 4): nur der Verweis auf die
                // Sohlplatte
                let slab = r.link("slab", &elem_ids)?;
                if !matches!(
                    elements.get(slab).map(|e: &Element| &e.kind),
                    Some(ElementKind::GroundSlab(_))
                ) {
                    return Err(err(r.line, "[perimeter]: „slab“ ist keine Sohlplatte"));
                }
                ElementKind::PerimeterInsulation { slab }
            } else if ix == 9 {
                // Regenfallrohr (Jörn 11.10.): Dachaufbau, Nummer des Ablaufs
                let roof = r.link("roof", &elem_ids)?;
                if !matches!(
                    elements.get(roof).map(|e: &Element| &e.kind),
                    Some(ElementKind::Roof { .. })
                ) {
                    return Err(err(r.line, "[downpipe]: „roof“ ist kein Flachdach"));
                }
                let spec = dach::read_downpipe_spec(r, &mut hints)?;
                ElementKind::Downpipe {
                    roof,
                    drain: r.int("drain")?,
                    spec,
                }
            } else if ix == 8 {
                // Flachdach (Jörn 10.10.): nur der Verweis auf die Decke
                let floor = r.link("floor", &elem_ids)?;
                if !matches!(
                    elements.get(floor).map(|e: &Element| &e.kind),
                    Some(ElementKind::Floor(_))
                ) {
                    return Err(err(r.line, "[roof]: „floor“ ist keine Decke"));
                }
                ElementKind::Roof {
                    floor,
                    drainage: dach::read_drainage(r, &mut hints)?,
                }
            } else if ix >= 5 {
                // Dachterrasse, Attikablech (D1–D3): nur der Verweis auf die Decke
                let floor = r.link("floor", &elem_ids)?;
                if !matches!(
                    elements.get(floor).map(|e: &Element| &e.kind),
                    Some(ElementKind::Floor(_))
                ) {
                    return Err(err(r.line, format!("[{section}]: „floor“ ist keine Decke")));
                }
                if ix == 5 {
                    ElementKind::RoofTerrace { floor }
                } else {
                    ElementKind::Coping { floor }
                }
            } else if ix == 4 {
                // Untersichtdämmung (G7 K4): nur der Verweis auf die Decke
                let floor = r.link("floor", &elem_ids)?;
                if !matches!(
                    elements.get(floor).map(|e: &Element| &e.kind),
                    Some(ElementKind::Floor(_))
                ) {
                    return Err(err(r.line, "[soffit]: „floor“ ist keine Decke"));
                }
                ElementKind::SoffitInsulation { floor }
            } else if ix == 3 {
                // Randdämmstreifen (K5): nur Verweise auf Wand und Decke
                let wall = r.link("wall", &elem_ids)?;
                let floor = r.link("floor", &elem_ids)?;
                let kind_of = |id| elements.get(id).map(|e: &Element| &e.kind);
                if !matches!(kind_of(wall), Some(ElementKind::Wall(_))) {
                    return Err(err(r.line, "[strip]: „wall“ ist keine Wand"));
                }
                if !matches!(kind_of(floor), Some(ElementKind::Floor(_))) {
                    return Err(err(r.line, "[strip]: „floor“ ist keine Decke"));
                }
                ElementKind::EdgeStrip { wall, floor }
            } else if ix == 2 {
                ElementKind::Floor(Floor {
                    run: r.link("run", &run_ids)?,
                    material: r.link("mat", &mat_ids)?,
                    thickness: r.f64("t")?,
                    // SZO 1: die feste Zahl (⅔ der Wandhöhe) wird verworfen
                    top: level(r, "top", LevelRef::top(ground))?,
                    // vor G7 K4 ohne: Standard (wirkt nur bei Vorsprung)
                    // vor W3 ohne Bekleidung (bytegleich), übrige Werte Standard
                    soffit: {
                        let d = Soffit::default();
                        let num = |k: &str, d: f64| match r.opt(k) {
                            Some(_) => r.f64(k),
                            None => Ok(d),
                        };
                        Soffit {
                            thickness: num("soffit", d.thickness)?,
                            material: match r.opt("soffit_mat") {
                                Some(_) => Some(r.link("soffit_mat", &mat_ids)?),
                                None => None,
                            },
                            cladding: num("soffit_clad", 0.0)?,
                            cladding_material: match r.opt("soffit_clad_mat") {
                                Some(_) => Some(r.link("soffit_clad_mat", &mat_ids)?),
                                None => None,
                            },
                            drip: num("soffit_drip", d.drip)?,
                            batten: num("batten", d.batten)?,
                            counter: num("counter", d.counter)?,
                        }
                    },
                    // vor D1 ohne: Werkstyp, 6 cm, Titanzink (wirkt nur bei
                    // Rücksprung)
                    terrace: Terrace {
                        build_up: match r.opt("terrace") {
                            Some(_) => {
                                let g = r.guid("terrace")?;
                                let id = set_ids.get(&g).copied();
                                if id.is_none() {
                                    hints.push(format!(
                                        "Zeile {}: Typ der Dachterrasse fehlt, Werkstyp",
                                        r.line
                                    ));
                                }
                                id
                            }
                            None => None,
                        },
                        upstand: match r.opt("upstand") {
                            Some(_) => r.f64("upstand")?,
                            None => crate::model::TERRACE_UPSTAND,
                        },
                        // vor der Attikadämmung ohne: Vorgabe (Jörn 10.10.)
                        crown: dach::attika_wert(
                            r,
                            "crown",
                            "Dämmung auf der Attika",
                            crate::model::TERRACE_CROWN,
                            (crate::model::MIN_CROWN, crate::model::MAX_CROWN),
                            &mut hints,
                        )?,
                        inner: dach::attika_wert(
                            r,
                            "inner",
                            "Attikadämmung innen",
                            crate::model::ROOF_INNER,
                            (crate::model::MIN_INNER, crate::model::MAX_INNER),
                            &mut hints,
                        )?,
                        coping_mat: match r.opt("coping_mat") {
                            Some(_) => Some(r.link("coping_mat", &mat_ids)?),
                            None => None,
                        },
                    },
                })
            } else if ix == 0 {
                ElementKind::GroundSlab(GroundSlab {
                    run: r.link("run", &run_ids)?,
                    material: r.link("mat", &mat_ids)?,
                    top: level(r, "top", LevelRef::bottom(ground))?,
                    thickness: r.f64("t")?,
                    recess: r.f64("recess")?,
                    // vor Gelände Thema 4 ohne: keine Dämmung; außerhalb
                    // der Grenzen des Dialogs mit Hinweis verworfen
                    insulation: match r.opt("insulation") {
                        Some(_) => {
                            let t = r.f64("insulation")?;
                            let lo = crate::model::MIN_PERIMETER;
                            let hi = crate::model::MAX_PERIMETER;
                            if t == 0.0 || (lo..=hi).contains(&t) {
                                t
                            } else {
                                hints.push(format!(
                                    "Zeile {}: Perimeterdämmung {} mm außerhalb {} bis {} mm, verworfen",
                                    r.line, t, lo, hi
                                ));
                                0.0
                            }
                        }
                        None => 0.0,
                    },
                })
            } else {
                let slab = r.link("slab", &elem_ids)?;
                if !matches!(
                    elements.get(slab).map(|e: &Element| &e.kind),
                    Some(ElementKind::GroundSlab(_))
                ) {
                    return Err(err(r.line, "[footing]: „slab“ ist keine Sohlplatte"));
                }
                if v1 {
                    // UK Gründung aus Plattendicke und Schürzentiefe
                    let t = match elements.get(slab).map(|e: &Element| &e.kind) {
                        Some(ElementKind::GroundSlab(s)) => s.thickness,
                        _ => 0.0,
                    };
                    let bottom = -(t + r.f64("d")?);
                    if let Some(gr) = storeys.get_mut(found_level) {
                        gr.elevation = bottom;
                        gr.height = -bottom;
                    }
                }
                ElementKind::StripFooting(StripFooting {
                    slab,
                    material: r.link("mat", &mat_ids)?,
                    width: r.f64("w")?,
                    base: level(r, "base", LevelRef::bottom(found_level))?,
                })
            };
            let number = number(r)?;
            // Typ nur, wenn gesetzt (R4); fehlt er, gilt der Einschicht-Aufbau
            // aus „mat“ (Regel 40)
            let layer_set = match r.opt("set") {
                None | Some("-") => None,
                Some(_) => {
                    let g = r.guid("set")?;
                    let id = set_ids.get(&g).copied();
                    if id.is_none() {
                        hints.push(format!(
                            "Zeile {}: Typ von {number} fehlt, Aufbau aus dem Baustoff",
                            r.line
                        ));
                    }
                    id
                }
            };
            let e = Element {
                guid: r.guid("guid")?,
                number,
                category: keyword(r, "cat", &Category::ALL, category)?,
                storey: r.link("storey", &storey_ids)?,
                layer_set,
                seq: r.int("seq")?,
                kind,
                props: Default::default(),
                locked: false,
            };
            let g = e.guid;
            let id = elements.insert(e);
            register(&mut elem_ids, &mut seen, r, g, id)?;
            if !locks.is_empty() {
                schloesser.push((id, r.line, locks));
            }
        }
    }
    oeffnung::resolve_locks(&mut elements, &elem_ids, schloesser, &mut hints);
    // Erweiterungsbauteile (E3): die Definition aus der Datei, geprüft wie
    // beim Einlesen, aber ohne Grenzprüfung. Unlesbares bleibt roh mit
    // Hinweis, samt seiner Exemplare; das Projekt öffnet trotzdem.
    let mut ext_defs: Vec<ExtDef> = Vec::new();
    let mut ext_raw: Vec<String> = Vec::new();
    let mut gemeldet: HashSet<String> = HashSet::new();
    for r in recs("extdef") {
        let key = r.opt("key").unwrap_or("");
        let _ = r.opt("version");
        let d = r
            .opt("text")
            .ok_or_else(|| "„text“ fehlt".to_string())
            .and_then(ExtDef::oeffnen)
            .map(|(d, h)| {
                // ältere Definition: Standardtyp nur als Hinweis (§11)
                if let Some(h) = h {
                    hints.push(format!(
                        "Zeile {}: Erweiterung „{key}“: {h}; die Definition bleibt erhalten",
                        r.line
                    ));
                }
                d
            })
            .and_then(|d| match d {
                d if d.key != key => Err(format!("key „{}“ im Text", d.key)),
                d if ext_defs.iter().any(|o| o.key == d.key) => Err("doppelt".into()),
                d if ext_defs.iter().any(|o| o.prefix() == d.prefix()) => {
                    Err(format!("Präfix {} doppelt", d.prefix()))
                }
                d => Ok(d),
            });
        match d {
            Ok(d) => ext_defs.push(d),
            Err(e) => {
                hints.push(format!(
                    "Zeile {}: Erweiterung „{key}“ nicht lesbar ({e}); sie und ihre Bauteile bleiben unverändert in der Datei",
                    r.line
                ));
                gemeldet.insert(key.to_string());
                r.all_used();
                ext_raw.push(lines[r.line - 1].to_string());
            }
        }
    }
    let mut ext_raw_guids: HashSet<Guid> = HashSet::new();
    for r in recs("extpart") {
        let key = r.opt("key").unwrap_or("");
        if !ext_defs.iter().any(|d| d.key == key) {
            if gemeldet.insert(key.to_string()) {
                hints.push(format!(
                    "Zeile {}: Erweiterung „{key}“ fehlt in der Datei; ihre Bauteile bleiben unverändert in der Datei",
                    r.line
                ));
            }
            if let Some(g) = r.opt("guid").and_then(Guid::from_ifc) {
                ext_raw_guids.insert(g);
            }
            r.all_used();
            ext_raw.push(lines[r.line - 1].to_string());
            continue;
        }
        // Ein unlesbares Exemplar bleibt roh wie eine unlesbare Definition
        // (BIM-Routine 09.10.): das Projekt öffnet trotzdem
        let part = match read_ext_part(r) {
            Ok(p) => p,
            Err(e) => {
                hints.push(format!(
                    "Zeile {}: Bauteil {} der Erweiterung „{key}“ nicht lesbar ({}); es bleibt unverändert in der Datei",
                    r.line,
                    r.opt("number").unwrap_or("ohne Nummer"),
                    e.message
                ));
                if let Some(g) = r.opt("guid").and_then(Guid::from_ifc) {
                    ext_raw_guids.insert(g);
                }
                r.all_used();
                ext_raw.push(lines[r.line - 1].to_string());
                continue;
            }
        };
        let e = Element {
            guid: r.guid("guid")?,
            number: number(r)?,
            category: Category::Extension,
            storey: r.link("storey", &storey_ids)?,
            layer_set: None,
            seq: r.opt("seq").map_or(Ok(crate::EXT_SEQ), |_| r.int("seq"))?,
            kind: ElementKind::Ext(part),
            props: Default::default(),
            locked: false,
        };
        let g = e.guid;
        let id = elements.insert(e);
        register(&mut elem_ids, &mut seen, r, g, id)?;
    }
    let mut raum_roh = Vec::new();
    for r in recs("space") {
        match raum::read(r, &mut number, &storey_ids, &seen, &mut hints) {
            raum::Lesung::Raum(e) => {
                register(&mut elem_ids, &mut seen, r, e.guid, elements.insert(*e))?
            }
            raum::Lesung::Roh(grund) => raum_roh.push((r, grund)),
            raum::Lesung::Weg => {}
        }
    }
    // Rohe Räume und verworfene Öffnungen zuletzt: Gültige Bauteile behalten Nummer und Guid (L12)
    let mut space_raw: Vec<_> = raum_roh
        .into_iter()
        .filter_map(|(r, g)| raum::raw(r, &lines, g, &mut number, &mut seen, &mut hints))
        .collect();
    let opening_raw: Vec<_> = verworfen
        .into_iter()
        .filter_map(|r| oeffnung::raw(r, lines[r.line - 1], &mut number, &mut seen, &mut hints))
        .collect();
    for r in recs("run") {
        let id = run_ids[&r.guid("guid")?];
        let run = runs.get_mut(id).expect("eben angelegt");
        let mut walls = slots.remove(&id).unwrap_or_default();
        walls.sort_by_key(|w| w.0);
        let count = segment_count(run.points.len(), run.closed);
        let ok = walls.len() == count && walls.iter().enumerate().all(|(i, w)| w.0 as usize == i);
        if !ok {
            return Err(err(
                r.line,
                format!(
                    "[run]: {count} Segmente, aber Wände für Segment {:?}",
                    walls.iter().map(|w| w.0).collect::<Vec<_>>()
                ),
            ));
        }
        run.segments = walls.into_iter().map(|w| w.1).collect();
    }
    for r in recs("prop") {
        if raum::raw_prop(r, lines[r.line - 1], &mut space_raw) {
            continue;
        }
        // Eigenschaft eines Bauteils ohne lesbare Erweiterung: bleibt roh
        if r.opt("elem")
            .and_then(Guid::from_ifc)
            .is_some_and(|g| ext_raw_guids.contains(&g))
        {
            r.all_used();
            ext_raw.push(lines[r.line - 1].to_string());
            continue;
        }
        let id = r.link("elem", &elem_ids)?;
        let key = r.get("key")?.to_string();
        let value = read_prop_value(r)?;
        elements
            .get_mut(id)
            .expect("eben angelegt")
            .props
            .insert(key, value);
    }
    // Schnitte (Lage, Blickrichtung); ältere Dateien haben keine
    let mut cuts = Vec::new();
    let mut active_cut = 0;
    for r in recs("cut") {
        // Eine fehlerhafte Zeile gibt einen Hinweis und wird übersprungen
        // (Ansichtszustand, das Projekt öffnet trotzdem)
        let flag = |k: &str| match r.opt(k) {
            Some(_) => r.flag(k),
            None => Ok(false),
        };
        let name = r.opt("name").unwrap_or("");
        let parsed = match crate::model::CUT_NAMES.iter().position(|n| *n == name) {
            Some(i) => (|| {
                let cut = crate::model::Cut {
                    pos: Some(r.f64("pos")?),
                    flip: flag("flip")?,
                };
                Ok((i, cut, flag("active")?))
            })(),
            None => Err(err(r.line, format!("[cut]: Schnitt „{name}“ unbekannt"))),
        };
        match parsed {
            Ok((i, cut, active)) => {
                cuts.push((i, cut));
                if active {
                    active_cut = i;
                }
            }
            Err(e) => {
                // Die übrigen Schlüssel der Zeile nicht noch einmal melden
                for k in ["name", "pos", "flip", "active"] {
                    r.opt(k);
                }
                hints.push(format!("{e}, übersprungen"));
            }
        }
    }
    // Sonnenstand (S4): Ansichtszustand; eine Zeile, die nicht zählt, gibt
    // einen Hinweis und bleibt roh stehen (Regel 72), wie `[location]`
    let mut sun = None;
    let mut sun_raw = None;
    match recs("sun").as_slice() {
        [] => {}
        [r] => {
            let date = r.opt("date").map(crate::Sun::parse_date);
            let time = r.opt("time").map(crate::Sun::parse_time);
            let on = match r.opt("on") {
                None => Some(false),
                Some(_) => r.flag("on").ok(),
            };
            let mut falsch = Vec::new();
            for (k, ok) in [
                ("date", date.is_some_and(|d| d.is_some())),
                ("time", time.is_some_and(|t| t.is_some())),
                ("on", on.is_some()),
            ] {
                if !ok {
                    falsch.push(format!("„{k}“ fehlt oder gilt nicht"));
                }
            }
            match (date.flatten(), time.flatten(), on) {
                (Some(date), Some(minutes), Some(on)) => {
                    sun = Some(crate::Sun { date, minutes, on })
                }
                _ => {
                    r.skip();
                    sun_raw = Some(lines[r.line - 1].to_string());
                    hints.push(format!(
                        "Zeile {}: [sun] {}; der Sonnenstand gilt als nicht gesetzt, die Zeile bleibt",
                        r.line,
                        falsch.join(", ")
                    ));
                }
            }
        }
        [_, r, ..] => return Err(err(r.line, "[sun] doppelt")),
    }
    // Schatten der Ansichten (S7): Ansichtszustand; eine Zeile, die nicht
    // zählt, gibt einen Hinweis und bleibt roh (Regel 72)
    let mut shade: [Option<crate::ViewShade>; 4] = [None; 4];
    let mut shade_raw = Vec::new();
    for r in recs("viewshade") {
        let view = r
            .opt("view")
            .and_then(|v| crate::SHADE_VIEWS.iter().position(|n| *n == v));
        let on = r.opt("on").and_then(|_| r.flag("on").ok());
        let fill = r.opt("fill").and_then(crate::ViewShade::parse_fill);
        let light = r.opt("light").and_then(crate::ViewShade::parse_light);
        let doppelt = view.is_some_and(|i| shade[i].is_some());
        match (view, on, fill, light) {
            (Some(i), Some(on), Some(hatch), Some(light)) if !doppelt => {
                shade[i] = Some(crate::ViewShade { on, hatch, light });
            }
            _ => {
                r.skip();
                shade_raw.push(lines[r.line - 1].to_string());
                let mut falsch = Vec::new();
                for (k, ok) in [
                    ("view", view.is_some()),
                    ("on", on.is_some()),
                    ("fill", fill.is_some()),
                    ("light", light.is_some()),
                ] {
                    if !ok {
                        falsch.push(format!("„{k}“ fehlt oder gilt nicht"));
                    }
                }
                if doppelt {
                    falsch.push("Ansicht doppelt".to_string());
                }
                hints.push(format!(
                    "Zeile {}: [viewshade] {}; die Ansicht folgt der Vorgabe, die Zeile bleibt",
                    r.line,
                    falsch.join(", ")
                ));
            }
        }
    }
    // Sperre und Ausblenden eines Bauteils ohne lesbare Erweiterung: bleiben
    // roh wie das Bauteil selbst
    let roh = |r: &Record| {
        r.opt("elem")
            .and_then(Guid::from_ifc)
            .is_some_and(|g| ext_raw_guids.contains(&g))
    };
    for r in recs("lock").iter().chain(recs("hide")).filter(|r| roh(r)) {
        r.all_used();
        ext_raw.push(lines[r.line - 1].to_string());
    }
    // Gesperrtes (Paket 4 §2.3): Modell, darum Unbekanntes mit Hinweis
    let locks: Vec<(usize, Option<Guid>)> = recs("lock")
        .iter()
        .filter(|r| !roh(r))
        .map(|r| (r.line, r.opt("elem").and_then(Guid::from_ifc)))
        .collect();
    // Ausgeblendetes (Paket 3 §3.6): nur Ansicht, Unbekanntes still verworfen
    // je Zeile: Bauteil, Art, Gewerk, Gelände
    type Hide = (Option<Guid>, Option<crate::Category>, Option<Guid>, bool);
    let mut hide: Vec<Hide> = Vec::new();
    let (finish, finish_raw) = ausbauart::read(recs("hide"), &lines, &mut hints);
    for r in recs("hide")
        .iter()
        .filter(|r| !roh(r) && r.opt("finish").is_none())
    {
        let g = |k: &str| r.opt(k).and_then(Guid::from_ifc);
        let cat = r.opt("cat").and_then(|w| {
            crate::Category::ALL
                .into_iter()
                .chain([Category::Extension])
                .find(|c| crate::kinds::spec(*c).szo == w)
        });
        hide.push((g("elem"), cat, g("trade"), r.opt("terrain") == Some("1")));
    }
    // Unter Gelände gestrichelt (S11): wie [viewshade]
    let mut below = [false; 4];
    let mut below_raw = Vec::new();
    let mut gesehen = [false; 4];
    for r in recs("viewbelow") {
        let view = r
            .opt("view")
            .and_then(|v| crate::SHADE_VIEWS.iter().position(|n| *n == v));
        let dashed = r.opt("dashed").and_then(|_| r.flag("dashed").ok());
        match (view, dashed) {
            (Some(i), Some(d)) if !gesehen[i] => {
                gesehen[i] = true;
                below[i] = d;
            }
            _ => {
                r.skip();
                below_raw.push(lines[r.line - 1].to_string());
                hints.push(format!(
                    "Zeile {}: [viewbelow] unlesbar oder doppelt; es gilt die erste lesbare Zeile der Ansicht, sonst Ausblenden; die Zeile bleibt",
                    r.line
                ));
            }
        }
    }
    let view_hide = sicht::read(recs("viewhide"), &lines, &mut hints);
    let has_storey = |g: Guid| storeys.iter().any(|(_, s)| s.guid == g);
    let (bem, bem_sicht) = bemassung::read(&recs, &lines, &has_storey, &mut hints);
    let (plan_cut, plan_cut_raw) = hoehen::read_plan_cut(recs("plancut"), &lines, &mut hints)?;
    let ausbau = ausbau::read(&recs, &lines, (&layer_sets, &elements), &mut hints)?;
    for recs in by.values() {
        for r in recs {
            r.unused(&mut hints);
        }
    }
    hints.sort_by_key(|h| {
        h.strip_prefix("Zeile ")
            .and_then(|r| r.split(':').next())
            .and_then(|n| n.parse::<usize>().ok())
            .unwrap_or(0)
    });

    if version < 4 {
        assign_codes(&mut layer_sets, &elements, &defaults);
    }
    add_lambda(&mut materials);
    let attr = Attributes::from_parts(pens, line_types, fills, surfaces, display, display_raw);
    let mut model = Model::from_parts(
        project, attr, materials, layer_sets, buildings, storeys, elements, runs, defaults, guids,
    );
    model.load_location(location, foot, location_raw);
    model.load_sun(sun, sun_raw);
    model.load_view_shade(shade, shade_raw);
    model.load_view_below(below, below_raw);
    model.view_hide = view_hide;
    model.view_hide.hide_raw = finish_raw;
    model.load_bemassung(bem, bem_sicht);
    model.load_plan_cut(plan_cut, plan_cut_raw);
    model.load_opening_raw(opening_raw);
    model.load_space_raw(space_raw);
    model.load_ausbau(ausbau);
    model.load_ext(ext_defs, ext_raw);
    for (k, n) in counters {
        if !model.raise_counter(&k, n) {
            hints.push(format!(
                "[project]: Nummernzähler „{k}“ unbekannt, übersprungen"
            ));
        }
    }
    model.adopt_trades(trades);
    hints.extend(model.complete_trades(recs("trade").is_empty()));
    hints.extend(model.complete_pre_b9());
    hints.extend(model.complete_line_types());
    let floors = model.floor_ids();
    hints.extend(model.complete_pre_b10());
    if !v3 {
        hints.extend(model.complete_pre_b12());
    }
    model.clear_cladding_except(&floors);
    model.complete_edge_strips();
    hints.extend(model.complete_soffits());
    model.complete_perimeters();
    hints.extend(model.complete_terraces());
    hints.extend(model.complete_flat_roofs());
    for (i, c) in cuts {
        model.set_cut(i, c);
    }
    model.set_active_cut(active_cut);
    let mut vis = crate::view::Visibility {
        hidden_finish: finish,
        ..Default::default()
    };
    for (elem, cat, trade, terrain) in hide {
        if let Some(g) = elem.filter(|g| model.elements().iter().any(|(_, e)| e.guid == *g)) {
            vis.hidden.insert(g);
        }
        if let Some(c) = cat {
            vis.hidden_cat.insert(c);
        }
        if let Some(t) = trade
            .map(crate::TradeId)
            .filter(|t| model.trade(*t).is_some())
        {
            vis.hidden_trade.insert(t);
        }
        vis.terrain_hidden |= terrain;
    }
    model.set_visibility(vis);
    for (line, g) in locks {
        let id = g.and_then(|g| {
            model
                .elements()
                .iter()
                .find(|(_, e)| e.guid == g)
                .map(|x| x.0)
        });
        match id.filter(|&id| model.lock_source(id) == id) {
            Some(id) => model.load_lock(id),
            None => hints.push(format!(
                "Zeile {line}: Sperre auf unbekanntes Bauteil, verworfen"
            )),
        }
    }
    hints.extend(model.check());
    alien.sort_unstable();
    model.foreign = foreign(&model, &by, &lines, &alien);
    model.ext.declare(ext);
    for (section, l) in ext_lines {
        model.ext.push_read(&section, l);
    }
    // die `load_*` erhöhen den Lagestand nicht (W45-Nachprüfung H-e)
    model.lage_vergessen();
    Ok(Loaded { model, hints })
}

// --- Datensätze lesen (auch für den Firmenkatalog, [`crate::catalog`]) ------

/// Start-Kreuzschraffur aus E10 (Winkel schon umgerechnet).
fn is_old_concrete_cross(l: &[HatchLine]) -> bool {
    let mut a: Vec<f32> = l.iter().map(|h| h.angle_deg).collect();
    a.sort_by(f32::total_cmp);
    a == [45.0, 135.0]
        && l.iter()
            .all(|h| h.spacing_mm == 1.27 && h.offset_mm == 0.0 && h.dash_mm == 0.0)
}

pub(crate) fn read_pen(r: &Record) -> Result<Pen, LoadError> {
    Ok(Pen {
        guid: r.guid("guid")?,
        number: r.int("nr")?,
        name: r.get("name")?.to_string(),
        color: r.color("color")?,
        width_mm: r.f32("w")?,
    })
}

pub(crate) fn read_line_type(r: &Record) -> Result<LineType, LoadError> {
    let pat = r.get("pat")?;
    let pattern = if pat == "-" {
        Vec::new()
    } else {
        pat.split(';')
            .map(|d| {
                let v: Vec<&str> = d.split(':').collect();
                match v[..] {
                    [len, gap, dot] => Some(Dash {
                        len_mm: len.parse().ok()?,
                        gap_mm: gap.parse().ok()?,
                        dot: match dot {
                            "0" => false,
                            "1" => true,
                            _ => return None,
                        },
                    }),
                    _ => None,
                }
            })
            .collect::<Option<Vec<_>>>()
            .ok_or_else(|| r.bad("pat", "länge:lücke:punkt;…"))?
    };
    Ok(LineType {
        guid: r.guid("guid")?,
        name: r.get("name")?.to_string(),
        pattern,
    })
}

pub(crate) fn read_fill(r: &Record, hints: &mut Vec<String>) -> Result<Fill, LoadError> {
    let space = match r.get("space")? {
        "paper" => FillSpace::Paper,
        "model" => FillSpace::Model,
        _ => return Err(r.bad("space", "paper oder model")),
    };
    let kind = match r.get("kind")? {
        "empty" => FillKind::Empty,
        "solid" => FillKind::Solid,
        "zigzag" => FillKind::Zigzag {
            period: r.f32("period")?,
        },
        "lines" => {
            let mut lines = r
                .get("lines")?
                .split(';')
                .map(|l| {
                    let v: Vec<f32> = l
                        .split(':')
                        .map(|x| x.parse().ok())
                        .collect::<Option<_>>()?;
                    match v[..] {
                        [a, s, o] => Some(HatchLine::solid(a, s, o)),
                        [a, s, o, dash_mm, gap_mm] => Some(HatchLine {
                            dash_mm,
                            gap_mm,
                            ..HatchLine::solid(a, s, o)
                        }),
                        _ => None,
                    }
                })
                .collect::<Option<Vec<_>>>()
                .ok_or_else(|| r.bad("lines", "winkel:abstand:versatz[:strich:lücke];…"))?;
            // Vor E3b zählten Winkel im Uhrzeigersinn
            if r.opt("ccw") != Some("1") {
                for l in &mut lines {
                    l.angle_deg = (180.0 - l.angle_deg).rem_euclid(180.0);
                }
                if r.opt("name") == Some("Stahlbeton") && is_old_concrete_cross(&lines) {
                    lines = crate::attr::concrete_lines();
                    hints.push("Stahlbeton-Schraffur auf gestrichelte Diagonale umgestellt".into());
                }
            }
            FillKind::Lines(lines)
        }
        _ => return Err(r.bad("kind", "empty, solid, lines oder zigzag")),
    };
    Ok(Fill {
        guid: r.guid("guid")?,
        name: r.get("name")?.to_string(),
        kind,
        space,
    })
}

pub(crate) fn read_surface(r: &Record) -> Result<Surface, LoadError> {
    Ok(Surface {
        guid: r.guid("guid")?,
        name: r.get("name")?.to_string(),
        color: r.color("color")?,
        cut_color: r.color("cut")?,
        pattern: None,
    })
}

pub(crate) fn read_material(
    r: &Record,
    fill_ids: &HashMap<Guid, Id<Fill>>,
    pen_ids: &HashMap<Guid, Id<Pen>>,
    surface_ids: &HashMap<Guid, Id<Surface>>,
) -> Result<Material, LoadError> {
    let lambda = match r.get("lambda")? {
        "-" => None,
        _ => Some(r.f64("lambda")?),
    };
    let guid = r.guid("guid")?;
    let name = r.get("name")?;
    // Unbekannte Art (neuere Fassung): wie Mauerwerk, die Zeile bleibt
    let category = keyword_or(
        r,
        "cat",
        &MatCategory::ALL,
        MatCategory::word,
        MatCategory::Masonry,
    )?;
    let (priority, density) = (r.int("prio")?, r.f64("rho")?);
    let d = MaterialDisplay {
        cut_fill: r.link("fill", fill_ids)?,
        cut_fg: r.link("fg", pen_ids)?,
        cut_bg: r.link("bg", pen_ids)?,
        surface: r.link("surface", surface_ids)?,
    };
    Ok(Material::new(guid, name, category, priority, density, d)
        .lambda(lambda)
        .trade(trade_ref(r)?))
}

/// `trade=` als Verweis; ob es das Gewerk gibt, prüft [`known_trades`].
fn trade_ref(r: &Record) -> Result<Option<TradeId>, LoadError> {
    match r.opt("trade") {
        None => Ok(None),
        Some(_) => Ok(Some(TradeId(r.guid("trade")?))),
    }
}

/// Wert eines Merkmals: `value` (Text), `num` oder `bool`.
fn read_prop_value(r: &Record) -> Result<PropValue, LoadError> {
    if let Some(v) = r.opt("value") {
        Ok(PropValue::Text(v.to_string()))
    } else if r.opt("num").is_some() {
        Ok(PropValue::Number(r.f64("num")?))
    } else if r.opt("bool").is_some() {
        Ok(PropValue::Bool(r.flag("bool")?))
    } else {
        Err(err(
            r.line,
            format!("[{}]: Wert fehlt (value, num oder bool)", r.section),
        ))
    }
}

/// Was [`read_types`] nach F-17 übergeht: Typen unbekannter Art mit ihren
/// Schichten und Merkmalen (Zeilen und Guids) und Hinweise dazu.
#[derive(Default)]
pub(crate) struct Passed {
    pub lines: Vec<usize>,
    pub guids: Vec<Guid>,
    pub hints: Vec<String>,
}

/// Bauteiltypen aus `[layerset]`, `[layer]` und `[typeprop]`. Vor SZO 4
/// (`typed` falsch) fehlen Kurzzeichen und Art; sie stellt der Aufrufer
/// danach ein ([`assign_codes`]). Ein Typ unbekannter Art (aus einer
/// neueren Fassung) wird übersprungen, eine unbekannte Schichtaufgabe gilt
/// als Bekleidung (F-17, R4).
#[allow(clippy::type_complexity)]
pub(crate) fn read_types(
    by: &HashMap<&str, Vec<Record>>,
    mat_ids: &HashMap<Guid, Id<Material>>,
    seen: &mut HashMap<Guid, usize>,
    typed: bool,
) -> Result<(Arena<LayerSet>, HashMap<Guid, LayerSetId>, Passed), LoadError> {
    let empty = Vec::new();
    let recs = |k: &str| by.get(k).unwrap_or(&empty);
    let mut passed = Passed::default();
    if typed {
        for r in recs("layerset") {
            let cat = r.get("cat")?;
            if !TypeCategory::ALL.iter().any(|&c| c.word() == cat) {
                passed.hints.push(format!(
                    "Zeile {}: Typ „{}“ unbekannter Art „{cat}“ übersprungen",
                    r.line,
                    r.opt("name").unwrap_or("")
                ));
                passed.guids.push(r.guid("guid")?);
                passed.lines.push(r.line);
                r.skip();
            }
        }
    }
    let mut set_layers: HashMap<Guid, Vec<MaterialLayer>> = HashMap::new();
    for r in recs("layer") {
        let set = r.guid("set")?;
        if passed.guids.contains(&set) {
            passed.lines.push(r.line);
            r.skip();
            continue;
        }
        let function = keyword_or(
            r,
            "fn",
            &LAYER_FUNCTIONS,
            layer_function,
            LayerFunction::Finish,
        )?;
        if r.replaced.get() > 0 {
            passed.hints.push(format!(
                "Zeile {}: unbekannte Schichtaufgabe „{}“, als Bekleidung gelesen",
                r.line,
                r.get("fn")?
            ));
        }
        let mut layer =
            MaterialLayer::new(r.link("mat", mat_ids)?, r.f64("t")?, function).trade(trade_ref(r)?);
        layer.core = r.flag("core")?;
        if r.opt("kg").is_some() {
            let kg: u16 = r.int("kg")?;
            // Regel 49: nur Gruppe 300
            if trade::valid_kg(kg) {
                layer.kg = Some(kg);
            } else {
                passed.hints.push(format!(
                    "Zeile {}: Kostengruppe {kg} ungültig, übergangen",
                    r.line
                ));
            }
        }
        // Gewählte Bauleistung (KA-0b); ein ungültiger Wert gilt nicht und
        // bleibt als Fremdes bytegleich stehen (A311, Regel 72)
        if let Some(v) = r.opt("svc") {
            match Guid::from_ifc(v) {
                Some(g) => layer.svc = Some(g),
                None => {
                    r.replaced.set(r.replaced.get() + 1);
                    passed.hints.push(format!(
                        "Zeile {}: Bauleistung „{v}“ ungültig, nicht benutzt; sie bleibt unverändert in der Datei",
                        r.line
                    ));
                }
            }
        }
        set_layers.entry(set).or_default().push(layer);
    }
    let mut set_props: HashMap<Guid, PropSet> = HashMap::new();
    for r in recs("typeprop") {
        let set = r.guid("set")?;
        if passed.guids.contains(&set) {
            passed.lines.push(r.line);
            r.skip();
            continue;
        }
        let key = r.get("key")?.to_string();
        let value = read_prop_value(r)?;
        set_props.entry(set).or_default().insert(key, value);
    }
    let mut layer_sets = Arena::new();
    let mut set_ids = HashMap::new();
    for r in recs("layerset") {
        let guid = r.guid("guid")?;
        if passed.guids.contains(&guid) {
            continue;
        }
        let (code, category, changed, note) = if typed {
            (
                r.get("code")?.to_string(),
                keyword(r, "cat", &TypeCategory::ALL, TypeCategory::word)?,
                match r.opt("changed") {
                    Some(_) => r.int("changed")?,
                    None => 1,
                },
                r.opt("note").unwrap_or("").to_string(),
            )
        } else {
            (String::new(), TypeCategory::ExteriorWall, 1, String::new())
        };
        let bearing = match r.opt("bearing") {
            None | Some("core") => Bearing::Core,
            Some(_) => Bearing::Depth {
                depth: r.f64("bearing")?,
                strip: r.link("strip", mat_ids)?,
            },
        };
        let s = LayerSet {
            guid,
            name: r.get("name")?.to_string(),
            code,
            category,
            layers: set_layers.remove(&guid).unwrap_or_default(),
            props: set_props.remove(&guid).unwrap_or_default(),
            note,
            changed,
            bearing,
            height: ausbau::read_height(r)?,
        };
        let id = layer_sets.insert(s);
        register(&mut set_ids, seen, r, guid, id)?;
    }
    let left: [Vec<Guid>; 2] = [
        set_layers.keys().copied().collect(),
        set_props.keys().copied().collect(),
    ];
    for (section, left) in ["layer", "typeprop"].into_iter().zip(left) {
        if let Some(r) = recs(section)
            .iter()
            .find(|r| r.guid("set").is_ok_and(|g| left.contains(&g)))
        {
            return Err(err(
                r.line,
                format!("[{section}]: „set“ verweist auf unbekannten Typ"),
            ));
        }
    }
    passed.lines.sort_unstable();
    Ok((layer_sets, set_ids, passed))
}

/// Alte Namen von Werksbaustoffen zu ihrem heutigen Werksnamen (E8:
/// „Gasbeton“ heißt seit KA-0a3 „Porenbeton“). Die einzige Liste; λ hier und
/// die Werkspreise in `sk-cost` (R73-W) lesen sie.
pub const ALTNAMEN: [(&str, &str); 1] = [("Gasbeton", "Porenbeton")];

/// Trägt ein Baustoff namens `name` in einer älteren Datei den Werksbaustoff
/// `werk` (gleicher oder alter Name)?
pub fn werksname(name: &str, werk: &str) -> bool {
    name == werk
        || ALTNAMEN
            .iter()
            .any(|(alt, neu)| *alt == name && *neu == werk)
}

/// Ergänzt fehlendes λ an Werksbaustoffen (Nachtrag K5): Treffer über die
/// Guid, sonst über Name und Kategorie der vier alten Startbaustoffe (Dateien
/// vor 137fca7 haben zeitbasierte Guids). Der alte Name zählt unter dem
/// heutigen Werksnamen ([`ALTNAMEN`]).
/// Vorhandenes λ bleibt; das Modell gilt danach als unverändert.
fn add_lambda(materials: &mut Arena<Material>) {
    // Namen der vier alten Startbaustoffe in Altdateien
    const ALT: [&str; 4] = ["Gasbeton", "Dämmung (WDVS)", "Stahlbeton", "Putz"];
    let werk = Model::new();
    let ids: Vec<_> = materials.ids().collect();
    for id in ids {
        let Some(x) = materials.get(id).filter(|x| x.lambda.is_none()) else {
            continue;
        };
        let by_guid = werk.materials().iter().find(|(_, w)| w.guid == x.guid);
        let by_name = || {
            if !ALT.contains(&x.name.as_str()) {
                return None;
            }
            werk.materials()
                .iter()
                .find(|(_, w)| werksname(&x.name, &w.name) && w.category == x.category)
        };
        let lambda = by_guid.or_else(by_name).and_then(|(_, w)| w.lambda);
        if let Some(x) = materials.get_mut(id) {
            x.lambda = lambda;
        }
    }
}

/// Dateien vor SZO 4: Typart aus der Benutzung (Innenwände → Innenwandtyp;
/// unbenutzt: die Art des Werkstyps, der Platz in den Standardtypen, sonst
/// Außenwand), Kurzzeichen
/// aus Art und Dicke („AW-31,5“), bei Gleichstand mit „-2“ … in
/// Guid-Reihenfolge.
fn assign_codes(layer_sets: &mut Arena<LayerSet>, elements: &Arena<Element>, defaults: &Defaults) {
    let mut users: HashMap<LayerSetId, (bool, bool)> = HashMap::new();
    for (_, e) in elements.iter() {
        if let Some(s) = e.layer_set {
            let u = users.entry(s).or_default();
            u.0 = true;
            u.1 |= e.category == Category::InteriorWall;
        }
    }
    let mut order: Vec<(Guid, LayerSetId)> =
        layer_sets.iter().map(|(id, s)| (s.guid, id)).collect();
    order.sort_by_key(|x| x.0);
    let mut taken: Vec<String> = Vec::new();
    for (_, id) in order {
        let (used, interior) = users.get(&id).copied().unwrap_or_default();
        let werk = layer_sets
            .get(id)
            .and_then(|t| crate::model::werk_category(t.guid));
        let category = match werk {
            Some(c) if !used => c,
            _ if interior || (!used && id == defaults.interior_wall) => TypeCategory::InteriorWall,
            _ => TypeCategory::ExteriorWall,
        };
        if let Some(t) = layer_sets.get_mut(id) {
            let code = crate::model::free_code(&type_code(category, t.thickness()), |c| {
                taken.iter().any(|x| x == c)
            });
            taken.push(code.clone());
            (t.category, t.code, t.changed) = (category, code, 1);
        }
    }
}

#[cfg(test)]
mod tests;
