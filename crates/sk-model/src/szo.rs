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

use crate::attr::{
    Attributes, Dash, Display, EdgeStyle, Fill, FillKind, FillSpace, HatchLine, LineType, Pen,
    Surface,
};
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
use crate::model::{Defaults, Location, Model, Project, FOOT_MAX};
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

fn mat_category(c: MatCategory) -> &'static str {
    match c {
        MatCategory::Masonry => "masonry",
        MatCategory::Concrete => "concrete",
        MatCategory::Insulation => "insulation",
        MatCategory::Plaster => "plaster",
        MatCategory::Timber => "timber",
        MatCategory::Air => "air",
        MatCategory::Metal => "metal",
    }
}

const MAT_CATEGORIES: [MatCategory; 7] = MatCategory::ALL;

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

pub(crate) fn type_category(c: TypeCategory) -> &'static str {
    crate::kinds::spec(c.category()).szo
}

/// `set=` an Decke, Sohlplatte und Frostschürze nur mit Typ: Dateien ohne
/// bleiben bytegleich (R4).
fn slab_type(line: Line, m: &Model, e: &Element) -> Line {
    match e.layer_set.and_then(|s| m.layer_set(s)) {
        Some(t) => line.guid("set", Some(t.guid)),
        None => line,
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
        .word("cat", mat_category(x.category))
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
        .word("cat", type_category(s.category))
        .num("changed", s.changed);
    // Deckenauflager nur, wenn es nicht der ganze Kern ist (K5)
    if let Bearing::Depth { depth, strip } = s.bearing {
        line = line.num("bearing", depth).guid("strip", mat_guid(strip));
    }
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
    let lt_guid = |id| a.line_type(id).map(|l| l.guid);

    for p in sorted(a.pens().iter(), |p| p.guid) {
        write_pen(&mut out, p);
    }
    for l in sorted(a.line_types().iter(), |l| l.guid) {
        write_line_type(&mut out, l);
    }
    for f in sorted(a.fills().iter(), |f| f.guid) {
        write_fill(&mut out, f);
    }
    for s in sorted(a.surfaces().iter(), |s| s.guid) {
        write_surface(&mut out, s);
    }
    let d = a.display();
    let slot = |out: &mut String, name: &str, s: &EdgeStyle| {
        Line::new("display")
            .word("slot", name)
            .guid("pen", pen_guid(s.pen))
            .guid("lt", lt_guid(s.line_type))
            .finish(out);
    };
    for (prefix, table) in [("drawing", &d.drawing), ("model3d", &d.model3d)] {
        for (name, k) in EDGE_NAMES {
            slot(&mut out, &format!("{prefix}.{name}"), &table[k as usize]);
        }
    }
    slot(&mut out, "ground", &d.ground);
    slot(&mut out, "section_line", &d.section_line);
    slot(&mut out, "section_ends", &d.section_ends);
    slot(&mut out, "background", &d.background);
    slot(&mut out, "pattern", &d.pattern);
    Line::new("display")
        .word("slot", "paper")
        .color("color", d.paper)
        .finish(&mut out);

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
    // Bodenkennwerte der Erdarbeiten: nur, was von der Vorgabe abweicht
    let mut line = line;
    let vorgabe = crate::qto_earth::Boden::default().werte();
    for ((k, ..), (v, d)) in crate::qto_earth::Boden::FELDER
        .iter()
        .zip(p.soil.werte().into_iter().zip(vorgabe))
    {
        if v != d {
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
    // Lage und Nordrichtung (Sonnenstand S1): ohne Angaben keine Zeile, so
    // bleibt eine Datei ohne sie bytegleich
    let l = m.location();
    if let Some(raw) = m.location_raw().filter(|_| l.is_unset()) {
        out.push_str(raw);
        out.push('\n');
    } else if !l.is_unset() {
        let mut line = Line::new("location");
        let [x, y] = m.north_foot().map_or([None; 2], |p| p.map(Some));
        for (k, v) in [
            ("lat", l.lat),
            ("lon", l.lon),
            ("north", l.north),
            ("x", x),
            ("y", y),
        ] {
            if let Some(v) = v {
                line = line.num(k, v);
            }
        }
        line.finish(&mut out);
    }
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
            .word(
                "kind",
                match s.kind {
                    LevelKind::Foundation => "foundation",
                    LevelKind::Storey => "storey",
                },
            )
            .num("elev", s.elevation)
            .num("h", s.height);
        // Gewollte Einbindetiefe (Gelände Thema 1) nur, wenn sie von der
        // tatsächlichen abweicht; ältere Dateien bleiben bytegleich
        let l = match s.embed {
            Some(e) => l.num("embed", e),
            None => l,
        };
        l.finish(&mut out);
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
        // Dachterrasse (D1–D3): ebenso nur abweichend
        if let Some(t) = f.terrace.build_up.and_then(|t| m.layer_set(t)) {
            l = l.guid("terrace", Some(t.guid));
        }
        if f.terrace.upstand != crate::model::TERRACE_UPSTAND {
            l = l.num("upstand", f.terrace.upstand);
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
    // Dachterrasse und Attikablech (D1–D3): nur, wenn es sie gibt
    for e in &walls {
        let (word, floor) = match e.kind {
            ElementKind::RoofTerrace { floor } => ("terrace", floor),
            ElementKind::Coping { floor } => ("coping", floor),
            _ => continue,
        };
        let l = Line::new(word)
            .guid("guid", Some(e.guid))
            .guid("floor", m.element(floor).map(|x| x.guid))
            .text("number", &e.number)
            .word("cat", category(e.category));
        let l = if word == "terrace" {
            slab_type(l, m, e)
        } else {
            l
        };
        l.num("seq", e.seq)
            .guid("storey", storey_guid(e.storey))
            .finish(&mut out);
    }
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
    const KNOWN: [&str; 36] = [
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
        "extdef",
        "extpart",
        "prop",
        "cut",
        "sun",
        "viewshade",
        "viewbelow",
        "hide",
        "lock",
        "matprop",
        "pattern",
    ];
    for (i, l) in text.lines().enumerate().skip(1) {
        let Some(r) = Record::parse(i + 1, l)? else {
            continue;
        };
        match KNOWN.iter().find(|k| **k == r.section) {
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
    let display = read_display(
        recs("display"),
        &pens,
        &line_types,
        &pen_ids,
        &lt_ids,
        pattern_pen,
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
        let terrain = match r.opt("terrain") {
            Some(_) => r.f64("terrain")?,
            None => 0.0,
        };
        if !(terrain.is_finite() && terrain.abs() <= crate::model::MAX_TERRAIN_OFFSET) {
            return Err(err(r.line, "[building]: „terrain“ außerhalb des Bereichs"));
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
                    &[LevelKind::Foundation, LevelKind::Storey],
                    |k| match k {
                        LevelKind::Foundation => "foundation",
                        LevelKind::Storey => "storey",
                    },
                )?,
                elevation: r.f64("elev")?,
                height: r.f64("h")?,
                embed: match r.opt("embed") {
                    Some(_) => Some(r.f64("embed")?),
                    None => None,
                },
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
    // Bodenkennwerte der Erdarbeiten; fehlende gelten mit der Vorgabe
    let mut boden = project.soil.werte();
    for (w, (k, _, min, max)) in boden.iter_mut().zip(crate::qto_earth::Boden::FELDER) {
        if p.opt(k).is_some() {
            let v = p.f64(k)?;
            if !(v.is_finite() && v >= min && v <= max) {
                return Err(err(
                    p.line,
                    format!("[project]: „{k}“ außerhalb des Bereichs"),
                ));
            }
            *w = v;
        }
    }
    project.soil = crate::qto_earth::Boden::aus_werten(boden).unwrap_or_default();
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
    let mut location = Location::default();
    let mut location_raw = None;
    let mut foot = None;
    match recs("location").as_slice() {
        [] => {}
        [r] => {
            let mut falsch = Vec::new();
            let mut num = |k: &'static str, max: f64| {
                let v = r.opt(k)?;
                let x = v
                    .parse::<f64>()
                    .ok()
                    .filter(|x| x.is_finite() && x.abs() <= max);
                if x.is_none() {
                    falsch.push(format!("„{k}“ ist keine Zahl im Bereich"));
                }
                x
            };
            let read = Location {
                lat: num("lat", 90.0),
                lon: num("lon", 180.0),
                north: num("north", f64::MAX),
            };
            // Fußpunkt des Nordpfeils (S2): x und y zusammen, nur mit north
            let (x, y) = (num("x", FOOT_MAX), num("y", FOOT_MAX));
            let fehlt = |k: &str| falsch.iter().all(|f| !f.starts_with(&format!("„{k}“")));
            let luecke = match (x, y, read.north) {
                (Some(_), None, _) if fehlt("y") => Some("y"),
                (None, Some(_), _) if fehlt("x") => Some("x"),
                (Some(_), Some(_), None) if fehlt("north") => Some("north"),
                _ => None,
            };
            if let Some(k) = luecke {
                falsch.push(format!("„{k}“ fehlt"));
            }
            if falsch.is_empty() && !read.is_unset() {
                location = read.normalized();
                foot = x.zip(y).map(|(x, y)| [x, y]);
            } else if falsch.is_empty() {
                // Keine bekannte Angabe (`elev=` einer neueren Fassung): zählt
                // nicht als Lage, bleibt im Wortlaut (Review 3br); Unbekanntes
                // meldet der Hinweis zu fremden Schlüsseln
                location_raw = Some(lines[r.line - 1].to_string());
            } else {
                r.skip();
                location_raw = Some(lines[r.line - 1].to_string());
                hints.push(format!(
                    "Zeile {}: [location] {}; die Lage gilt als nicht gesetzt, die Zeile bleibt",
                    r.line,
                    falsch.join(", ")
                ));
            }
        }
        [_, r, ..] => return Err(err(r.line, "[location] doppelt")),
    }
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
        });
        hints.push("Datei auf Geschossverwaltung umgestellt".to_string());
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
            seq: match r.opt("seq") {
                Some(_) => r.int("seq")?,
                None => crate::model::WALL_SEQ,
            },
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
    for (section, ix) in [
        ("slab", 0),
        ("footing", 1),
        ("floor", 2),
        ("strip", 3),
        ("soffit", 4),
        ("terrace", 5),
        ("coping", 6),
        ("perimeter", 7),
    ] {
        for r in recs(section) {
            let kind = if ix == 7 {
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
                    soffit: Soffit {
                        thickness: match r.opt("soffit") {
                            Some(_) => r.f64("soffit")?,
                            None => crate::model::SOFFIT_THICKNESS,
                        },
                        material: match r.opt("soffit_mat") {
                            Some(_) => Some(r.link("soffit_mat", &mat_ids)?),
                            None => None,
                        },
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
                    // vor Gelände Thema 4 ohne: keine Dämmung
                    insulation: match r.opt("insulation") {
                        Some(_) => r.f64("insulation")?,
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
        }
    }
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
            seq: match r.opt("seq") {
                Some(_) => r.int("seq")?,
                None => crate::EXT_SEQ,
            },
            kind: ElementKind::Ext(part),
            props: Default::default(),
            locked: false,
        };
        let g = e.guid;
        let id = elements.insert(e);
        register(&mut elem_ids, &mut seen, r, g, id)?;
    }
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
    for r in recs("hide").iter().filter(|r| !roh(r)) {
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
    let attr = Attributes::from_parts(pens, line_types, fills, surfaces, display);
    let mut model = Model::from_parts(
        project, attr, materials, layer_sets, buildings, storeys, elements, runs, defaults, guids,
    );
    model.load_location(location, foot, location_raw);
    model.load_sun(sun, sun_raw);
    model.load_view_shade(shade, shade_raw);
    model.load_view_below(below, below_raw);
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
    hints.extend(model.complete_pre_b10());
    if !v3 {
        hints.extend(model.complete_pre_b12());
    }
    model.complete_edge_strips();
    model.complete_soffits();
    model.complete_perimeters();
    hints.extend(model.complete_terraces());
    for (i, c) in cuts {
        model.set_cut(i, c);
    }
    model.set_active_cut(active_cut);
    let mut vis = crate::view::Visibility::default();
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
        &MAT_CATEGORIES,
        mat_category,
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
            if !TypeCategory::ALL.iter().any(|&c| type_category(c) == cat) {
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
                keyword(r, "cat", &TypeCategory::ALL, type_category)?,
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

/// Liest die Darstellungs-Slots. Fehlt einer, gilt sein Startwert (gleiche
/// Stiftnummer bzw. gleicher Linientyp-Name wie in den Starttabellen).
fn read_display(
    recs: &[Record],
    pens: &Arena<Pen>,
    line_types: &Arena<LineType>,
    pen_ids: &HashMap<Guid, Id<Pen>>,
    lt_ids: &HashMap<Guid, Id<LineType>>,
    pattern_pen: Option<Id<Pen>>,
    hints: &mut Vec<String>,
) -> Result<Display, LoadError> {
    let (std_attr, _) = crate::attr::defaults(&mut GuidGen::with_seed(0));
    let sd = std_attr.display();
    let first_lt = line_types.ids().next();
    // Startwert in den geladenen Tabellen: Stift mit derselben Nummer
    let fallback = |slot: &str, s: &EdgeStyle| -> Option<EdgeStyle> {
        let nr = std_attr.pen(s.pen)?.number;
        // Die Schnittlinie A–A ist im Startsatz Strichpunkt (E4)
        let lt_name = match slot {
            "section_line" => crate::attr::SECTION_LINE_TYPE,
            _ => &std_attr.line_type(s.line_type)?.name,
        };
        let pen = pens
            .iter()
            .find(|(_, p)| p.number == nr)
            .map(|(id, _)| id)?;
        let line_type = line_types
            .iter()
            .find(|(_, l)| l.name == lt_name)
            .map(|(id, _)| id)
            .or(first_lt)?;
        Some(EdgeStyle { pen, line_type })
    };
    let mut slots: Vec<(String, EdgeStyle)> = Vec::new();
    for (prefix, table) in [("drawing", &sd.drawing), ("model3d", &sd.model3d)] {
        for (name, k) in EDGE_NAMES {
            slots.push((format!("{prefix}.{name}"), table[k as usize]));
        }
    }
    slots.push(("ground".into(), sd.ground));
    slots.push(("section_line".into(), sd.section_line));
    slots.push(("section_ends".into(), sd.section_ends));
    slots.push(("background".into(), sd.background));
    slots.push(("pattern".into(), sd.pattern));
    let mut styles: Vec<Option<EdgeStyle>> = vec![None; slots.len()];
    let mut paper = None;
    for r in recs {
        let slot = r.get("slot")?;
        if slot == "paper" {
            paper = Some(r.color("color")?);
            continue;
        }
        match slots.iter().position(|s| s.0 == slot) {
            Some(i) => {
                styles[i] = Some(EdgeStyle {
                    pen: r.link("pen", pen_ids)?,
                    line_type: r.link("lt", lt_ids)?,
                })
            }
            None => hints.push(format!(
                "Zeile {}: unbekannter Darstellungs-Slot „{slot}“ übersprungen",
                r.line
            )),
        }
    }
    let mut resolved = Vec::with_capacity(slots.len());
    for ((name, std), s) in slots.iter().zip(styles) {
        let s = match s {
            Some(s) => s,
            None => {
                // Fugen in Ansichten (Paket 6): der ergänzte Stift, Volllinie
                if let (Some(pen), "pattern") = (pattern_pen, name.as_str()) {
                    // Volllinie: leeres Strichmuster, sonst der erste Typ
                    // (Review 3q/1: nicht die kleinste Guid, die ist oft die
                    // Strichlinie)
                    let line_type = line_types
                        .iter()
                        .find(|(_, l)| l.pattern.is_empty())
                        .map(|(id, _)| id)
                        .or_else(|| line_types.ids().next())
                        .ok_or_else(|| err(0, "Darstellung „pattern“ fehlt, kein Linientyp"))?;
                    resolved.push(EdgeStyle { pen, line_type });
                    continue;
                }
                // Der Hintergrund (E16) fehlt in allen älteren Dateien: still ergänzen
                if name != "background" {
                    hints.push(format!("Darstellung „{name}“ fehlt, Startwert gesetzt"));
                }
                fallback(name, std).ok_or_else(|| {
                    err(
                        0,
                        format!("Darstellung „{name}“ fehlt, kein passender Stift"),
                    )
                })?
            }
        };
        resolved.push(s);
    }
    let paper = paper.unwrap_or_else(|| {
        hints.push("Darstellung „paper“ fehlt, Startwert gesetzt".into());
        sd.paper
    });
    let n = edge_kind::COUNT;
    let mut drawing = [resolved[0]; edge_kind::COUNT];
    let mut model3d = [resolved[0]; edge_kind::COUNT];
    for (i, (_, k)) in EDGE_NAMES.iter().enumerate() {
        drawing[*k as usize] = resolved[i];
        model3d[*k as usize] = resolved[n + i];
    }
    Ok(Display {
        drawing,
        model3d,
        ground: resolved[2 * n],
        section_line: resolved[2 * n + 1],
        section_ends: resolved[2 * n + 2],
        background: resolved[2 * n + 3],
        pattern: resolved[2 * n + 4],
        paper,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::element::PropSet;
    use crate::qto::run_qto;

    /// Haus 10 × 8 m, eine offene Wand, umbenannte Nummer, Eigenschaften,
    /// eigener Stift, Schraffur mit zwei Linienscharen, eigene Oberfläche.
    pub(super) fn house() -> Model {
        let mut m = Model::with_seed(1);
        let set = m.defaults().exterior_wall;
        let rect = [
            vec3(0.0, 0.0, 0.0),
            vec3(0.0, 8000.0, 0.0),
            vec3(10000.0, 8000.0, 0.0),
            vec3(10000.0, 0.0, 0.0),
        ];
        let eg = m.eg_at(2750.0);
        let r = m
            .add_wall_run(&rect, true, RefSide::Left, eg, set, Category::ExteriorWall)
            .unwrap();
        let line = [vec3(12000.0, 0.0, 0.0), vec3(12000.0, 4500.25, 0.0)];
        let eg = m.eg_at(2750.0);
        m.add_wall_run(
            &line,
            false,
            RefSide::Center,
            eg,
            set,
            Category::ExteriorWall,
        )
        .unwrap();
        let w = m.run(r).unwrap().segments[1];
        m.set_number(w, "AW-Nord \"alt\"").unwrap();
        m.set_prop(w, "Brandschutz", Some(PropValue::Text("F90".into())));
        m.set_prop(w, "U-Wert", Some(PropValue::Number(0.21)));
        m.set_prop(w, "Tragend", Some(PropValue::Bool(true)));
        let (pid, pen) = m
            .attr()
            .pens()
            .iter()
            .next()
            .map(|(i, p)| (i, p.clone()))
            .unwrap();
        m.set_pen(
            pid,
            Pen {
                color: [12, 34, 200],
                width_mm: 0.35,
                ..pen
            },
        );
        let guid = m.new_guid();
        m.add_fill(Fill {
            guid,
            name: "Kreuz".into(),
            kind: FillKind::Lines(vec![
                HatchLine::solid(45.0, 2.5, 0.0),
                HatchLine {
                    dash_mm: 1.0,
                    gap_mm: 0.5,
                    ..HatchLine::solid(-45.0, 2.5, 1.25)
                },
            ]),
            space: FillSpace::Paper,
        });
        let guid = m.new_guid();
        m.add_surface(Surface {
            guid,
            name: "Klinker rot".into(),
            color: [160, 60, 40],
            cut_color: [200, 90, 70],
            pattern: None,
        });
        m
    }

    fn load(text: &str) -> Result<Loaded, LoadError> {
        read(text, GuidGen::with_seed(99))
    }

    /// A310 (Vorankündigung KA-0, Umbenennung Gasbeton → Porenbeton): Eine
    /// Altdatei mit „Gasbeton“ unter zeitbasierter Guid und ohne λ bekommt
    /// weiter das λ des Werksbaustoffs; der Name in der Datei bleibt.
    #[test]
    fn a310_altdatei_gasbeton_bekommt_lambda() {
        let m = Model::with_seed(1);
        let werk = m
            .materials()
            .iter()
            .find(|(_, x)| x.name == "Gasbeton" || x.name == "Porenbeton")
            .map(|(_, x)| x.clone())
            .expect("Werksbaustoff Gas- oder Porenbeton");
        let lambda = werk.lambda.expect("Werks-λ");
        let neu = write(&m);
        let g = werk.guid.to_ifc();
        let other = if g.ends_with('A') { 'B' } else { 'A' };
        let g2 = format!("{}{other}", &g[..g.len() - 1]);
        let mut hit = 0;
        let alt: String = neu
            .replace(&g, &g2)
            .lines()
            .map(|l| {
                if !(l.starts_with("[material] ") && l.contains(&format!("guid={g2}"))) {
                    return format!("{l}\n");
                }
                hit += 1;
                let l = l.replacen(&format!("name=\"{}\"", werk.name), "name=\"Gasbeton\"", 1);
                let i = l.find("lambda=").unwrap();
                let j = l[i..].find(' ').map_or(l.len(), |j| i + j);
                format!("{}lambda=-{}\n", &l[..i], &l[j..])
            })
            .collect();
        assert_eq!(hit, 1, "{neu}");
        assert!(alt.contains("name=\"Gasbeton\""), "{alt}");
        let got = load(&alt).unwrap().model;
        let (_, x) = got
            .materials()
            .iter()
            .find(|(_, x)| x.name == "Gasbeton" && x.guid != werk.guid)
            .expect("Altbaustoff bleibt „Gasbeton“");
        assert_eq!(x.lambda, Some(lambda));
    }

    /// E15/E3b: Schraffuren vor E15 (Winkel im Uhrzeigersinn, Stahlbeton
    /// gekreuzt) werden umgerechnet bzw. ersetzt; Guid und Verweise bleiben.
    #[test]
    fn alte_schraffuren_werden_umgestellt() {
        let m = Model::with_seed(15);
        let neu = write(&m);
        assert!(
            neu.contains("lines=135:2.54:0:0:0;135:2.54:1.27:1.5:0.75 ccw=1"),
            "{neu}"
        );
        let alt: String = neu
            .lines()
            .map(|l| {
                let l = l.replace(" ccw=1", "");
                let l = l.replace("lines=135:1.27:0:0:0", "lines=45:1.27:0");
                let l = l.replace(
                    "lines=135:2.54:0:0:0;135:2.54:1.27:1.5:0.75",
                    "lines=45:1.27:0;135:1.27:0",
                );
                format!("{l}\n")
            })
            .collect();
        assert!(!alt.contains("ccw") && alt.contains("lines=45:1.27:0;135:1.27:0"));
        let l = load(&alt).unwrap();
        assert_eq!(
            l.hints,
            ["Stahlbeton-Schraffur auf gestrichelte Diagonale umgestellt"]
        );
        assert_eq!(write(&l.model), neu, "gleich einer neuen Datei");
        // Eine selbst geänderte Kreuzschraffur wird nur umgerechnet
        let eigen = alt.replace("lines=45:1.27:0;135:1.27:0", "lines=30:2:0;120:2:0.5");
        let l = load(&eigen).unwrap();
        assert!(l.hints.is_empty(), "{:?}", l.hints);
        let (_, f) = l
            .model
            .attr()
            .fills()
            .iter()
            .find(|(_, f)| f.name == "Stahlbeton")
            .unwrap();
        assert_eq!(
            f.kind,
            FillKind::Lines(vec![
                HatchLine::solid(150.0, 2.0, 0.0),
                HatchLine::solid(60.0, 2.0, 0.5)
            ])
        );
    }

    /// Ab zwei Obergeschossen heißen sie „1. OG“, „2. OG“: das Kürzel hat ein
    /// Leerzeichen und muss in Anführungszeichen stehen.
    #[test]
    fn gebaeude_mit_drei_obergeschossen_laedt_wieder() {
        let mut m = Model::with_seed(12);
        m.add_building(4);
        let a = write(&m);
        assert!(a.contains("short=\"2. OG\""), "{a}");
        let l = load(&a).unwrap();
        assert_eq!(write(&l.model), a);
    }

    /// Schnitte A und B: Lage und Blickrichtung stehen in der Datei, nie
    /// gezeigte fehlen (Datei bleibt bytegleich), Unbekanntes wird gemeldet.
    #[test]
    fn schnitte_in_der_datei() {
        let mut m = Model::with_seed(16);
        let leer = write(&m);
        assert!(!leer.contains("[cut]"));
        m.set_cut(
            0,
            crate::model::Cut {
                pos: Some(4000.0),
                flip: false,
            },
        );
        m.set_cut(
            1,
            crate::model::Cut {
                pos: Some(5000.5),
                flip: true,
            },
        );
        let a = write(&m);
        assert!(a.contains("[cut] name=A pos=4000 flip=0"), "{a}");
        assert!(a.contains("[cut] name=B pos=5000.5 flip=1"), "{a}");
        let l = load(&a).unwrap();
        assert!(l.hints.is_empty(), "{:?}", l.hints);
        assert_eq!(l.model.cuts(), m.cuts());
        assert_eq!(l.model.active_cut(), 0);
        assert_eq!(write(&l.model), a);
        // Zuletzt gezeigt: B
        m.set_active_cut(1);
        let b = write(&m);
        assert!(b.contains("[cut] name=B pos=5000.5 flip=1 active=1"), "{b}");
        let l = load(&b).unwrap();
        assert_eq!(l.model.active_cut(), 1);
        assert_eq!(write(&l.model), b);
        let l = load(&a.replace("name=B", "name=C")).unwrap();
        assert!(l.hints.iter().any(|h| h.contains("„C“")), "{:?}", l.hints);
        assert_eq!(l.model.cuts()[1], crate::model::Cut::default());
    }

    #[test]
    fn speichern_oeffnen_speichern_gibt_dieselben_bytes() {
        let m = house();
        let a = write(&m);
        let l = load(&a).unwrap();
        assert!(l.hints.is_empty(), "{:?}", l.hints);
        assert_eq!(write(&l.model), a);
        assert!(a.starts_with("SZO 4\n"));
        assert!(a.contains("number=\"AW-Nord \\\"alt\\\"\""), "{a}");
        assert!(l.model.check().is_empty());
        assert_eq!(l.model.project(), m.project());
    }

    #[test]
    fn geladenes_modell_gleicht_dem_gespeicherten() {
        let m = house();
        let l = load(&write(&m)).unwrap().model;
        let g = |m: &Model| -> Vec<(Guid, String, PropSet)> {
            let mut v: Vec<_> = m
                .elements()
                .iter()
                .map(|(_, e)| (e.guid, e.number.clone(), e.props.clone()))
                .collect();
            v.sort_by_key(|x| x.0);
            v
        };
        assert_eq!(g(&l), g(&m));
        let pen = |m: &Model| -> Vec<Pen> {
            let mut v: Vec<Pen> = m.attr().pens().iter().map(|(_, p)| p.clone()).collect();
            v.sort_by_key(|p| p.guid);
            v
        };
        assert_eq!(pen(&l), pen(&m));
        assert!(l.attr().fills().iter().any(|(_, f)| f.name == "Kreuz"
            && matches!(&f.kind, FillKind::Lines(ls) if ls.len() == 2 && ls[1].offset_mm == 1.25)));
        assert!(l
            .attr()
            .surfaces()
            .iter()
            .any(|(_, s)| s.name == "Klinker rot" && s.cut_color == [200, 90, 70]));
        // Rechteck: dieselben Mengen wie vor dem Speichern
        let r = l
            .runs()
            .iter()
            .find(|(id, r)| r.closed && l.run_below(*id).is_none())
            .map(|(id, _)| id)
            .unwrap();
        let vol: f64 = run_qto(&l, r).iter().map(|w| w.volume).sum();
        assert!((vol / 1e9 - 28.7776).abs() < 1e-4, "{}", vol / 1e9);
        let open = l.runs().iter().find(|(_, r)| !r.closed).unwrap().1;
        assert_eq!(open.points[1].y, 4500.25);
    }

    /// Baustoffe sehen nach dem Laden gleich aus (Schraffur, Stifte, Oberfläche).
    #[test]
    fn baustoffe_behalten_ihr_aussehen() {
        let m = house();
        let l = load(&write(&m)).unwrap().model;
        type Look = (
            Guid,
            Option<Fill>,
            Option<Pen>,
            Option<Pen>,
            Option<Surface>,
        );
        let look = |m: &Model| -> Vec<Look> {
            let a = m.attr();
            let mut v: Vec<_> = m
                .materials()
                .iter()
                .map(|(_, x)| {
                    (
                        x.guid,
                        a.fill(x.cut_fill).cloned(),
                        a.pen(x.cut_fg).cloned(),
                        a.pen(x.cut_bg).cloned(),
                        a.surface(x.surface).cloned(),
                    )
                })
                .collect();
            v.sort_by_key(|x| x.0);
            v
        };
        assert_eq!(look(&l), look(&m));
        let shown = |m: &Model| -> Vec<(Guid, Guid)> {
            let a = m.attr();
            let d = a.display();
            d.drawing
                .iter()
                .chain(&d.model3d)
                .chain([&d.ground, &d.section_line, &d.section_ends, &d.background])
                .map(|s| {
                    (
                        a.pen(s.pen).unwrap().guid,
                        a.line_type(s.line_type).unwrap().guid,
                    )
                })
                .collect()
        };
        assert_eq!(shown(&l), shown(&m));
        assert_eq!(l.attr().display().paper, m.attr().display().paper);
    }

    #[test]
    fn naechste_nummer_nach_dem_laden() {
        let m = house();
        let mut l = load(&write(&m)).unwrap().model;
        let set = l.defaults().exterior_wall;
        let pts = [vec3(0.0, -5000.0, 0.0), vec3(3000.0, -5000.0, 0.0)];
        let eg = l.eg_at(2750.0);
        let r = l
            .add_wall_run(&pts, false, RefSide::Left, eg, set, Category::ExteriorWall)
            .unwrap();
        let w = l.run(r).unwrap().segments[0];
        // AW-001..AW-009 vergeben (EG, OG, gerade Wand; AW-002 umbenannt),
        // weiter mit AW-010
        assert_eq!(l.element(w).unwrap().number, "AW-010");
        let mut taken: Vec<Guid> = l.elements().iter().map(|(_, e)| e.guid).collect();
        taken.sort();
        taken.dedup();
        assert_eq!(taken.len(), l.elements().len());
    }

    /// F-17b: Ein unbekannter Abschnitt bleibt ohne Hinweis roh erhalten
    /// (am Ende), ein unbekannter Schlüssel mit Hinweis an seiner Zeile;
    /// ohne beides schreibt das Modell wie vorher.
    #[test]
    fn unbekannter_abschnitt_und_schluessel_bleiben() {
        let a = write(&house());
        let b = a.replacen("[storey]", "[zukunft] x=1\n[storey] neu=\"ja\"", 1);
        let l = load(&b).unwrap();
        assert_eq!(l.hints.len(), 1, "{:?}", l.hints);
        assert!(l.hints[0].contains("„neu“"));
        let c = write(&l.model);
        let storey = b.lines().find(|x| x.starts_with("[storey]")).unwrap();
        assert!(c.lines().any(|x| x == storey), "{c}");
        assert!(c.ends_with("[zukunft] x=1\n"), "{c}");
        assert_eq!(write_known(&l.model), a);
        assert_eq!(write(&load(&c).unwrap().model), c, "zweiter Rundlauf");
    }

    /// Review 3n: Fremde Schlüssel an [matprop]-Zeilen bleiben beim
    /// richtigen Baustoff, auch wenn eine neuere Fassung die Zeilen anders
    /// ordnet (Satzkennung mit `mat=`).
    #[test]
    fn fremder_kennwert_bleibt_beim_baustoff() {
        let mut m = house();
        let ids: Vec<_> = m
            .materials()
            .iter()
            .filter(|(_, x)| crate::matprop::price_unit(x.category).is_some())
            .map(|(id, _)| id)
            .take(2)
            .collect();
        assert_eq!(ids.len(), 2);
        for (i, id) in ids.iter().enumerate() {
            let mut x = m.material(*id).unwrap().clone();
            x.props
                .insert("Richtpreis".into(), PropValue::Number(10.0 + i as f64));
            assert!(m.set_material(*id, x));
        }
        let a = write(&m);
        let rows: Vec<&str> = a.lines().filter(|l| l.starts_with("[matprop]")).collect();
        assert_eq!(rows.len(), 2, "{a}");
        // Getauscht, die zweite Zeile mit fremdem Schlüssel
        let marked = format!("{} zukunft=1", rows[1]);
        let b = a
            .replacen(rows[0], "@@", 1)
            .replacen(rows[1], rows[0], 1)
            .replacen("@@", &marked, 1);
        let c = write(&load(&b).unwrap().model);
        assert!(c.lines().any(|l| l == marked), "{c}");
        assert!(c.lines().any(|l| l == rows[0]), "{c}");
    }

    #[test]
    fn kaputter_verweis_nennt_die_zeile() {
        let a = write(&house());
        let (i, line) = a
            .lines()
            .enumerate()
            .find(|(_, l)| l.starts_with("[wall]"))
            .unwrap();
        let run = line
            .split(" run=")
            .nth(1)
            .unwrap()
            .split(' ')
            .next()
            .unwrap();
        let b = a.replacen(&format!(" run={run}"), " run=0000000000000000000000", 1);
        let e = load(&b).err().unwrap();
        assert_eq!(e.line, i + 1, "{e}");
        assert!(e.message.contains("unbekannte Guid"), "{e}");
    }

    #[test]
    fn neuere_version_wird_nicht_geoeffnet() {
        let a = write(&house()).replacen("SZO 4", "SZO 5", 1);
        let e = load(&a).err().unwrap();
        assert_eq!(e.line, 1);
        assert!(e.to_string().contains("neuerer Skizzeo-Version"), "{e}");
        assert!(load("Hallo").is_err());
        assert!(load("").is_err());
    }

    #[test]
    fn fehlender_slot_bekommt_den_startwert() {
        let m = house();
        let a = write(&m);
        let b: String = a
            .lines()
            .filter(|l| !l.starts_with("[display] slot=section_line "))
            .map(|l| format!("{l}\n"))
            .collect();
        assert_ne!(a, b);
        let l = load(&b).unwrap();
        assert_eq!(l.hints.len(), 1, "{:?}", l.hints);
        assert!(l.hints[0].contains("section_line"));
        assert_eq!(write(&l.model), a);
    }

    #[test]
    fn doppelte_guid_und_falsche_segmente_sind_fehler() {
        let a = write(&house());
        let first = a.lines().find(|l| l.starts_with("[storey]")).unwrap();
        let b = format!("{a}{first}\n");
        let e = load(&b).err().unwrap();
        assert!(e.message.contains("doppelt"), "{e}");
        let c: String = {
            let mut skipped = false;
            a.lines()
                .filter(|l| {
                    // eine OG-Wand: auf sie verweist keine Kopplung
                    let drop = !skipped && l.starts_with("[wall]") && l.contains(" below=");
                    skipped |= drop;
                    !drop
                })
                .map(|l| format!("{l}\n"))
                .collect()
        };
        let e = load(&c).err().unwrap();
        assert!(e.message.contains("Segmente"), "{e}");
    }

    /// Doppelte Punkte und Schichtdicke 0 laden, melden sich aber.
    #[test]
    fn unsinnige_masse_ergeben_hinweise() {
        let a = write(&house());
        let b = a
            .replacen("pts=\"0 0;0 8000;", "pts=\"0 0;0 0;", 1)
            .replacen(" t=140 ", " t=0 ", 1);
        assert_ne!(a, b);
        let l = load(&b).unwrap();
        // dazu je OG-Wand, dass sie nicht mehr auf ihrer EG-Wand steht
        assert_eq!(l.hints.len(), 6, "{:?}", l.hints);
        assert!(l.hints.iter().any(|h| h.contains("ohne Länge")));
        assert!(l.hints.iter().any(|h| h.contains("Schichtdicke")));
        assert_eq!(
            l.hints
                .iter()
                .filter(|h| h.contains("nicht parallel"))
                .count(),
            4
        );
    }

    #[test]
    fn texte_mit_sonderzeichen() {
        let mut out = String::new();
        Line::new("x")
            .text("t", "a \"b\" \\ c\nd # e")
            .finish(&mut out);
        let r = Record::parse(1, out.trim_end()).unwrap().unwrap();
        assert_eq!(r.get("t").unwrap(), "a \"b\" \\ c\nd # e");
        assert!(Record::parse(1, "[x] t=\"offen").is_err());
        assert!(Record::parse(1, "  # nur Kommentar").unwrap().is_none());
    }

    // --- R4: Aufbau für Decke, Sohlplatte und Frostschürze ---------------

    /// Haus mit einem Deckentyp „Estrich 50 + Kern“ (noch nicht gesetzt);
    /// liefert die erste Decke und den Typ.
    fn haus_mit_deckentyp() -> (Model, crate::ElementId, LayerSetId) {
        let mut m = house();
        let de = m
            .elements()
            .iter()
            .find(|(_, e)| e.category == Category::Floor)
            .map(|(id, _)| id)
            .expect("Decke");
        let beton = m.element(de).unwrap().kind.material().unwrap();
        let putz = m
            .materials()
            .iter()
            .find(|(_, x)| x.name == "Putz")
            .map(|(id, _)| id)
            .unwrap();
        let t = m
            .add_layer_set(LayerSet {
                guid: Guid(0x4e00),
                name: "Decke mit Estrich".into(),
                code: "DE-E".into(),
                category: TypeCategory::Floor,
                layers: vec![
                    MaterialLayer::new(putz, 50.0, LayerFunction::Finish),
                    MaterialLayer::new(beton, 100.0, LayerFunction::Structure).core(),
                ],
                props: PropSet::new(),
                note: String::new(),
                changed: 1,
                bearing: Bearing::Core,
            })
            .expect("Typ");
        (m, de, t)
    }

    fn nach_guid(m: &Model, g: Guid) -> crate::ElementId {
        m.elements()
            .iter()
            .find(|(_, e)| e.guid == g)
            .map(|(id, _)| id)
            .unwrap()
    }

    /// Ohne Typ hat jedes Bauteil mit Baustoff einen Einschicht-Aufbau aus
    /// Baustoff und Dicke (Breite); Wände haben die Schichten ihres Typs.
    #[test]
    fn r4_gedachter_einschicht_aufbau() {
        let m = house();
        let mut seen = Vec::new();
        for (id, e) in m.elements().iter() {
            let a = m.element_layers(id);
            match e.kind {
                ElementKind::Wall(_) => {
                    let t = m.layer_set(e.layer_set.unwrap()).unwrap();
                    assert_eq!(a, t.layers);
                }
                ElementKind::Floor(_)
                | ElementKind::GroundSlab(_)
                | ElementKind::StripFooting(_) => {
                    let one = MaterialLayer::new(
                        e.kind.material().unwrap(),
                        e.kind.core_thickness().unwrap(),
                        LayerFunction::Structure,
                    )
                    .core();
                    assert_eq!(a, vec![one], "{}", e.number);
                    seen.push(e.category);
                }
                _ => assert!(a.len() <= 1, "{}", e.number),
            }
        }
        for c in [
            Category::Floor,
            Category::GroundSlab,
            Category::StripFooting,
        ] {
            assert!(seen.contains(&c), "{c:?} im Prüfhaus");
        }
    }

    /// Decke mit Typ: Schichten von oben nach unten, der Kern mit Dicke und
    /// Baustoff der Decke (variable Kernschicht, Regel 38). Ein Typ fremder
    /// Art passt nicht (Regel 37).
    #[test]
    fn r4_decke_mit_typ() {
        let (mut m, de, t) = haus_mit_deckentyp();
        let dicke = m.element(de).unwrap().kind.core_thickness().unwrap();
        let mat = m.element(de).unwrap().kind.material().unwrap();
        assert!(m.set_slab_type(de, Some(t)));
        assert_eq!(m.element(de).unwrap().layer_set, Some(t));
        let a = m.element_layers(de);
        assert_eq!(a.len(), 2);
        assert_eq!(a.iter().map(|l| l.thickness).sum::<f64>(), 50.0 + dicke);
        assert_eq!((a[1].core, a[1].material), (true, mat));
        assert!(m.check().is_empty(), "{:?}", m.check());
        assert!(m.set_floor_thickness(de, 250.0));
        assert_eq!(m.element_layers(de)[1].thickness, 250.0);
        // Regel 37: Wandtyp an der Decke, Deckentyp an der Sohlplatte
        let aw = m.defaults().exterior_wall;
        assert!(!m.set_slab_type(de, Some(aw)));
        let sp = m
            .elements()
            .iter()
            .find(|(_, e)| e.category == Category::GroundSlab)
            .map(|(id, _)| id)
            .unwrap();
        assert!(!m.set_slab_type(sp, Some(t)));
        // Regel 38: Kernbaustoff = Baustoff am Bauteil (Datei von Hand
        // geändert)
        let putz = m.layer_set(t).unwrap().layers[0].material;
        let g = |id| m.material(id).unwrap().guid.to_ifc();
        let text = write(&m);
        let zeile = text.lines().find(|l| l.starts_with("[floor]")).unwrap();
        let falsch = zeile.replace(&format!(" mat={} ", g(mat)), &format!(" mat={} ", g(putz)));
        assert_ne!(zeile, falsch);
        let back = read(&text.replace(zeile, &falsch), GuidGen::with_seed(1)).unwrap();
        assert!(
            back.hints.iter().any(|p| p.contains("Kern des Typs")),
            "{:?}",
            back.hints
        );
        assert!(m.set_slab_type(de, None));
        assert_eq!(m.element_layers(de).len(), 1);
        // Regel 38 am Typ: waagerecht genau ein Kern
        let mut zwei = m.layer_set(t).unwrap().clone();
        zwei.layers[0].core = true;
        assert!(zwei
            .problems()
            .iter()
            .any(|p| p.contains("genau eine Kernschicht")));
    }

    /// Ohne Typ an Decke, Sohlplatte oder Frostschürze steht kein `set=` in
    /// der Datei; mit Typ steht es, und der Rundlauf ist bytegleich.
    #[test]
    fn r4_typ_in_der_datei() {
        let (mut m, de, t) = haus_mit_deckentyp();
        let ohne = write(&m);
        for l in ohne.lines().filter(|l| {
            l.starts_with("[floor]") || l.starts_with("[slab]") || l.starts_with("[footing]")
        }) {
            assert!(!l.contains(" set="), "{l}");
        }
        assert!(ohne.contains(" cat=floor "), "Deckentyp in der Datei");
        assert!(m.set_slab_type(de, Some(t)));
        let mit = write(&m);
        let g = m.layer_set(t).unwrap().guid.to_ifc();
        assert!(mit
            .lines()
            .any(|l| l.starts_with("[floor]") && l.contains(&format!(" set={g} "))));
        let back = read(&mit, GuidGen::with_seed(1)).unwrap();
        assert!(back.hints.is_empty(), "{:?}", back.hints);
        assert_eq!(write(&back.model), mit, "Rundlauf bytegleich");
        let de2 = nach_guid(&back.model, m.element(de).unwrap().guid);
        let form = |m: &Model, id| {
            m.element_layers(id)
                .iter()
                .map(|l| (m.material(l.material).unwrap().guid, l.thickness, l.core))
                .collect::<Vec<_>>()
        };
        assert_eq!(form(&back.model, de2), form(&m, de));
    }

    /// F-17: Ein Typ unbekannter Art (`cat=zukunft`) wird mit Hinweis
    /// übersprungen; die Decke mit `set=` darauf öffnet mit Einschicht-Aufbau
    /// aus `mat=` (Regel 40). Eine unbekannte Schichtaufgabe gilt als
    /// Bekleidung.
    #[test]
    fn r4_unbekannte_art_und_aufgabe() {
        let (mut m, de, t) = haus_mit_deckentyp();
        assert!(m.set_slab_type(de, Some(t)));
        let text = write(&m)
            .lines()
            .map(|l| match l.starts_with("[layerset]") {
                true => l.replace(" cat=floor ", " cat=zukunft "),
                false => l.to_string(),
            })
            .collect::<Vec<_>>()
            .join("\n");
        let back = read(&text, GuidGen::with_seed(1)).expect("öffnet");
        assert!(
            back.hints
                .iter()
                .any(|h| h.contains("unbekannter Art „zukunft“")),
            "{:?}",
            back.hints
        );
        assert!(
            back.hints
                .iter()
                .any(|h| h.contains("Typ von DE-001 fehlt")),
            "{:?}",
            back.hints
        );
        let de2 = nach_guid(&back.model, m.element(de).unwrap().guid);
        assert_eq!(back.model.element(de2).unwrap().layer_set, None);
        assert_eq!(back.model.element_layers(de2).len(), 1);
        assert!(back
            .model
            .layer_sets()
            .iter()
            .all(|(_, s)| s.code != "DE-E"));

        let text = write(&house());
        let text = text.replacen(" fn=insulation ", " fn=zukunft ", 1);
        let back = read(&text, GuidGen::with_seed(1)).expect("öffnet");
        assert!(
            back.hints
                .iter()
                .any(|h| h.contains("unbekannte Schichtaufgabe „zukunft“")),
            "{:?}",
            back.hints
        );
    }

    use crate::txn::Direction;

    /// Abschnitte von `sk-cost` in `.szo` (Bausteingrenze §4.1).
    const KA0: [&str; 8] = [
        "article",
        "service",
        "svcpart",
        "svcfollow",
        "rate",
        "lot",
        "origin",
        "costproject",
    ];

    /// Alle acht Abschnitte, mit ungültigem Wert, doppelter Kennung, Zeile
    /// ohne Kennung und fremdem Schlüssel; dahinter ein fremder Abschnitt.
    fn mit_erweiterung(m: &Model) -> String {
        let mut t = write(m);
        t.push_str(
            "[article] guid=0Art1 name=\"Ziegel\" unit=Banane price=12.5\n\
             [article] guid=0Art1 name=\"doppelt\" price=1\n\
             [article] name=\"ohne Kennung\" price=2\n\
             [service] guid=0Svc1 name=\"Mauern\" zukunft=\"ja\"\n\
             [svcpart] key=0Svc1/0Art1 svc=0Svc1 art=0Art1 qty=0.12\n\
             [svcfollow] key=0Svc1/0Svc2 from=0Svc1 to=0Svc2\n\
             [rate] key=wage num=60.00\n\
             [lot] key=300 name=\"Mauerarbeiten\"\n\
             [origin] key=0Art1 rec=article from=werk\n\
             [costproject] key=vat num=19\n\
             [zukunft] guid=0Zuk1 wert=\"neu\"\n",
        );
        t
    }

    /// KA-0b (paket-ka0.md §6 Nr. 4): Rundlauf aller Abschnitte bytegleich,
    /// mit und ohne Erweiterungsliste.
    #[test]
    fn ka0b_erweiterung_rundlauf_bytegleich() {
        let text = mit_erweiterung(&house());
        let mit = read_with(&text, GuidGen::with_seed(99), &KA0).unwrap();
        assert_eq!(write(&mit.model), text);
        assert_eq!(mit.model.ext("article").count(), 3);
        assert_eq!(mit.model.ext("costproject").count(), 1);
        assert_eq!(mit.model.ext("zukunft").count(), 0);
        let ohne = load(&text).unwrap();
        assert_eq!(write(&ohne.model), text);
        assert_eq!(ohne.model.ext("article").count(), 0);
    }

    /// KA-0b (§6 Nr. 5): Stand vor KA-0 → KA-0 → vorher ohne Verlust; und
    /// eine Datei mit Erweiterungszeilen mitten zwischen den bekannten
    /// Sätzen kommt hinter sie, ohne Zeile zu verlieren.
    #[test]
    fn ka0b_alt_neu_alt_ohne_verlust() {
        let text = mit_erweiterung(&house());
        let neu = read_with(&text, GuidGen::with_seed(99), &KA0).unwrap();
        let mut m = neu.model;
        m.begin("Satz");
        m.ext_put("rate", "wage", "[rate] key=wage num=61.00".into(), None);
        m.commit().unwrap();
        let geschrieben = write(&m);
        let alt = load(&geschrieben).unwrap();
        assert_eq!(write(&alt.model), geschrieben);
        let wieder = read_with(&geschrieben, GuidGen::with_seed(99), &KA0).unwrap();
        assert_eq!(write(&wieder.model), geschrieben);
        assert_eq!(
            wieder.model.ext("rate").next().unwrap().line,
            "[rate] key=wage num=61.00"
        );
        // Erweiterungszeile vor den Stiften: gleicher Inhalt, neue Stelle
        let plain = write(&house());
        let (kopf, rest) = plain.split_once('\n').unwrap();
        let vorn = format!("{kopf}\n[lot] key=300 name=\"Mauerarbeiten\"\n{rest}");
        let back = read_with(&vorn, GuidGen::with_seed(99), &KA0).unwrap();
        assert_eq!(
            write(&back.model),
            format!("{plain}[lot] key=300 name=\"Mauerarbeiten\"\n")
        );
    }

    /// KA-0b (§6 Nr. 6): Rückgängig und Wiederholen eines Erweiterungssatzes
    /// an seine Stelle; Modell- und Erweiterungsänderung sind ein Schritt.
    #[test]
    fn ka0b_ein_schritt_rueckgaengig_an_seine_stelle() {
        let text = mit_erweiterung(&house());
        let mut m = read_with(&text, GuidGen::with_seed(99), &KA0)
            .unwrap()
            .model;
        m.require_steps();
        let rev = m.revision();
        let ext_rev = m.ext_revision();
        m.begin("Kosten und Projekt");
        m.ext_put(
            "article",
            "0Art2",
            "[article] guid=0Art2 name=\"Mörtel\"".into(),
            Some("0Art1"),
        );
        m.ext_put(
            "article",
            "0Art1",
            "[article] guid=0Art1 name=\"Ziegel\" price=13".into(),
            None,
        );
        m.ext_remove("lot", "300");
        m.ext_remove("lot", "999");
        assert!(m.set_project(Project {
            site: "Hofweg 3".into(),
            client: "Familie Muster".into(),
            author: "J. Architekt".into(),
            ..m.project().clone()
        }));
        let w = m.runs().iter().next().map(|(_, r)| r.segments[0]).unwrap();
        m.set_number(w, "AW-Süd").unwrap();
        let t = m.commit().expect("ein Schritt");
        assert!(m.revision() > rev);
        assert!(m.ext_revision() > ext_rev);
        let nachher = write(&m);
        assert!(nachher.contains(
            "[article] guid=0Art2 name=\"Mörtel\"\n[article] guid=0Art1 name=\"Ziegel\" price=13\n\
             [article] guid=0Art1 name=\"doppelt\" price=1\n"
        ));
        assert!(!nachher.contains("[lot]"));
        assert!(nachher.contains(" site=\"Hofweg 3\""));
        m.apply(&t, Direction::Undo);
        assert_eq!(write(&m), text);
        m.apply(&t, Direction::Redo);
        assert_eq!(write(&m), nachher);
        m.apply(&t, Direction::Undo);
        assert_eq!(write(&m), text);
        // Abbrechen rollt die Erweiterung mit zurück
        m.begin("verworfen");
        m.ext_put("rate", "wage", "[rate] key=wage num=1".into(), None);
        m.rollback();
        assert_eq!(write(&m), text);
    }

    fn projektdaten() -> Project {
        Project {
            kind: "Neubau Einfamilienhaus".into(),
            site: "Haus Mustermann".into(),
            place: "Musterweg 1\n27777 Ganderkesee".into(),
            number: "01/26".into(),
            client: "Max Mustermann".into(),
            client_addr: "Phantasiestraße 7\n12345 Irgendwo".into(),
            author: "Dipl.-Ing. (FH) Jörn Horstmann".into(),
            author_addr: "Denkmalsweg 18b\n27777 Ganderkesee".into(),
            ..Project::new(Guid(0), "")
        }
    }

    /// Paket PD Abnahme 1 und 2: „Neu“ mit Daten schreibt eine
    /// `[projectinfo]`-Zeile mit acht Feldern, `[project]` bleibt; ohne
    /// Daten bleibt die Datei bytegleich. Zeilenumbrüche überstehen den
    /// Rundlauf.
    #[test]
    fn projektdaten_neu_mit_und_ohne() {
        let m = Model::with_seed(1);
        let leer = write(&m);
        let mut ohne = Model::with_seed(1);
        ohne.init_project(Project::new(Guid(0), ""));
        assert_eq!(write(&ohne), leer);
        let mut m = Model::with_seed(1);
        m.init_project(projektdaten());
        let text = write(&m);
        let projekt = |t: &str| {
            t.lines()
                .find(|l| l.starts_with("[project] "))
                .unwrap()
                .to_string()
        };
        assert_eq!(projekt(&text), projekt(&leer));
        let info: Vec<&str> = text
            .lines()
            .filter(|l| l.starts_with("[projectinfo]"))
            .collect();
        assert_eq!(
            info,
            [
                "[projectinfo] key=project kind=\"Neubau Einfamilienhaus\" projno=\"01/26\" \
              site=\"Haus Mustermann\" place=\"Musterweg 1\\n27777 Ganderkesee\" \
              client=\"Max Mustermann\" clientaddr=\"Phantasiestraße 7\\n12345 Irgendwo\" \
              author=\"Dipl.-Ing. (FH) Jörn Horstmann\" \
              authoraddr=\"Denkmalsweg 18b\\n27777 Ganderkesee\""
            ]
        );
        let back = load(&text).unwrap();
        assert!(back.hints.is_empty(), "{:?}", back.hints);
        assert_eq!(back.model.project().fields(), projektdaten().fields());
        assert_eq!(back.model.project().place, "Musterweg 1\n27777 Ganderkesee");
        assert_eq!(write(&back.model), text);
    }

    /// Paket PD Abnahme 3 und 5 (Regel 110): Eine alte Datei mit `site` und
    /// `client` an `[project]` liest sich wie bisher und bleibt bytegleich.
    /// Die erste Änderung zieht alles nach `[projectinfo]` um, ein Schritt;
    /// Rückgängig stellt die alte Datei bytegleich her. Unverändert
    /// übernehmen ist kein Schritt.
    #[test]
    fn projektdaten_alte_datei_zieht_um() {
        // vor Regel 110 geschrieben: Schlüssel an [project]
        let heute = write(&house());
        let zeile = heute
            .lines()
            .find(|l| l.starts_with("[project] "))
            .unwrap()
            .to_string();
        assert!(!zeile.contains(" next="), "{zeile}");
        let alt = heute.replace(
            &zeile,
            &format!("{zeile} site=\"Haus Meier\" client=\"Meier\""),
        );
        let mut m = load(&alt).unwrap().model;
        assert_eq!(
            (m.project().site.as_str(), m.project().client.as_str()),
            ("Haus Meier", "Meier")
        );
        assert!(!m.project().info);
        assert_eq!(write(&m), alt);
        m.begin("Projektdaten geändert");
        assert!(!m.set_project(m.project().clone()), "unverändert");
        assert!(m.set_project(Project {
            number: "01/26".into(),
            ..m.project().clone()
        }));
        let t = m.commit().expect("ein Schritt");
        let neu = write(&m);
        assert!(neu.contains(&format!("{zeile}\n")), "{neu}");
        assert!(neu.contains(
            "\n[projectinfo] key=project projno=\"01/26\" site=\"Haus Meier\" client=\"Meier\"\n"
        ));
        assert!(!neu.contains("[project] guid") || !projekt_hat_site(&neu));
        m.apply(&t, crate::txn::Direction::Undo);
        assert_eq!(write(&m), alt);
        m.apply(&t, crate::txn::Direction::Redo);
        assert_eq!(write(&m), neu);
        // Beides in einer Datei: [projectinfo] gilt, [project] bleibt bytegleich
        let beides = alt.replace(
            &format!("{zeile} site=\"Haus Meier\" client=\"Meier\"\n"),
            &format!(
                "{zeile} site=\"Haus Meier\" client=\"Meier\"\n[projectinfo] key=project site=\"Haus Neu\"\n"
            ),
        );
        let b = load(&beides).unwrap();
        assert!(b.hints.is_empty(), "{:?}", b.hints);
        assert_eq!(b.model.project().site, "Haus Neu");
        assert_eq!(b.model.project().client, "");
        assert_eq!(write(&b.model), beides);
        // Ein zu langer Wert und ein Umbruch im einzeiligen Feld bleiben
        let lang = "x".repeat(300);
        let odd = alt.replace(
            &format!("{zeile} site=\"Haus Meier\" client=\"Meier\"\n"),
            &format!("{zeile}\n[projectinfo] key=project projno=\"{lang}\" client=\"A\\nB\"\n"),
        );
        let o = load(&odd).unwrap();
        assert_eq!(o.model.project().number, lang);
        assert_eq!(o.model.project().client, "A\nB");
        assert_eq!(write(&o.model), odd);
        // Leere [projectinfo] neben alten Werten an [project]: die Anzeige
        // bleibt über Speichern und Laden leer, die Datei bytegleich
        let leer = beides.replace(
            "[projectinfo] key=project site=\"Haus Neu\"\n",
            "[projectinfo] key=project\n",
        );
        let l = load(&leer).unwrap();
        assert!(l.model.project().is_blank(), "{:?}", l.model.project());
        assert_eq!(write(&l.model), leer);
        let l = load(&write(&l.model)).unwrap();
        assert!(l.model.project().is_blank());
        // doppelt: Fehler mit der Zeile
        let doppelt = odd.replace("[projectinfo]", "[projectinfo] key=project\n[projectinfo]");
        assert!(load(&doppelt).is_err());
    }

    fn projekt_hat_site(text: &str) -> bool {
        text.lines()
            .any(|l| l.starts_with("[project] ") && l.contains(" site="))
    }

    /// KA-0b (§6 Nr. 7): `svc=` und `site/client/author` stehen nur, wo
    /// gesetzt; heutige Dateien bleiben bytegleich.
    #[test]
    fn ka0b_bauleistung_und_projektangaben() {
        let m = house();
        let plain = write(&m);
        assert!(!plain.contains(" svc="));
        assert!(!plain.contains(" site="));
        assert!(!plain.contains(" client="));
        assert!(!plain.contains(" author="));
        let mut m = m;
        let id = m.layer_sets().ids().next().unwrap();
        let mut s = m.layer_set(id).unwrap().clone();
        let svc = m.new_guid();
        s.layers[0].svc = Some(svc);
        assert!(m.set_layer_set(id, s));
        m.set_project(Project {
            client: "Familie \"Muster\"".into(),
            ..m.project().clone()
        });
        let text = write(&m);
        assert_eq!(text.matches(" svc=").count(), 1);
        assert!(text.contains(" client="));
        assert!(!text.contains(" site="));
        let back = load(&text).unwrap();
        assert!(back.hints.is_empty(), "{:?}", back.hints);
        assert_eq!(write(&back.model), text);
        assert_eq!(back.model.project().client, "Familie \"Muster\"");
        let set = m.layer_set(id).unwrap().guid;
        assert_eq!(
            back.model
                .layer_sets()
                .iter()
                .find(|(_, s)| s.guid == set)
                .map(|(_, s)| s.layers[0].svc),
            Some(Some(svc))
        );
    }

    /// Review 3ac: Zwei gleiche Schichten, nur die zweite mit ungültigem
    /// `svc=`. Der rohe Wert bleibt an der zweiten; vorher wanderte er an
    /// die erste gleiche Zeile.
    #[test]
    fn a311_gleiche_schichten_behalten_ihre_zeile() {
        let mut m = house();
        let id = m.layer_sets().ids().next().unwrap();
        let mut s = m.layer_set(id).unwrap().clone();
        let l0 = s.layers[0];
        s.layers.insert(0, l0);
        let svc = m.new_guid();
        s.layers[1].svc = Some(svc);
        assert!(m.set_layer_set(id, s));
        let text = write(&m);
        let kaputt = text.replace(&format!(" svc={}", svc.to_ifc()), " svc=nix");
        assert_ne!(kaputt, text);
        let back = load(&kaputt).unwrap();
        assert_eq!(write(&back.model), kaputt, "bytegleich");
    }

    /// KA-0d, Befund 99: `Model::raw_svc` nennt Typ, Schicht und rohen Wert,
    /// genau solange die Datei ihn zurückschreibt; nach Zuordnen weg, nach
    /// Rückgängig wieder da. Gleiche Schichten: die richtige.
    #[test]
    fn a311_raw_svc_nennt_typ_und_schicht() {
        use crate::txn::Direction;
        let mut m = house();
        assert!(m.raw_svc().is_empty());
        let id = m.layer_sets().ids().next().unwrap();
        let mut s = m.layer_set(id).unwrap().clone();
        let l0 = s.layers[0];
        s.layers.insert(0, l0);
        let svc = m.new_guid();
        s.layers[1].svc = Some(svc);
        assert!(m.set_layer_set(id, s));
        let set = m.layer_set(id).unwrap().guid;
        let text = write(&m);
        assert!(load(&text).unwrap().model.raw_svc().is_empty(), "gültig");
        let kaputt = text.replace(&format!(" svc={}", svc.to_ifc()), " svc=nix");
        let mut b = load(&kaputt).unwrap().model;
        let id = b
            .layer_sets()
            .iter()
            .find(|(_, s)| s.guid == set)
            .map(|(i, _)| i)
            .unwrap();
        let roh = vec![crate::RawSvc {
            set,
            layer: 1,
            value: "nix".into(),
        }];
        assert_eq!(b.raw_svc(), roh);
        // Zuordnen an der Schicht ersetzt den rohen Wert, Rückgängig holt ihn
        let mut s = b.layer_set(id).unwrap().clone();
        s.layers[1].svc = Some(svc);
        b.begin("Bauleistung zuordnen");
        assert!(b.set_layer_set(id, s));
        let t = b.commit().unwrap();
        assert!(b.raw_svc().is_empty());
        assert!(!write(&b).contains(" svc=nix"));
        b.apply(&t, Direction::Undo);
        assert_eq!(b.raw_svc(), roh);
        assert_eq!(write(&b), kaputt, "bytegleich nach Rückgängig");
    }

    /// Wie oben für den Firmenkatalog.
    #[test]
    fn a311_raw_svc_im_firmenkatalog() {
        let mut lib = crate::catalog::Library::from_model(&house());
        let id = lib.types.ids().next().unwrap();
        let t = lib.types.get_mut(id).unwrap();
        let set = t.guid;
        let svc = Guid::from_ifc("0Svc000000000000000001").unwrap();
        t.layers[0].svc = Some(svc);
        let text = crate::write_szk(&lib);
        let kaputt = text.replace(&format!(" svc={}", svc.to_ifc()), " svc=-x-");
        assert_ne!(kaputt, text);
        let back = crate::read_szk(&kaputt).unwrap();
        assert_eq!(crate::write_szk(&back), kaputt);
        assert_eq!(
            back.raw_svc(),
            vec![crate::RawSvc {
                set,
                layer: 0,
                value: "-x-".into(),
            }]
        );
    }

    /// A311 (Nachtrag KA-0c/d zu §6 Nr. 4/7, R7): Ein ungültiges `svc=` an
    /// einer Schicht bleibt roh erhalten. Es wird nicht benutzt (Schicht ohne
    /// Bauleistung), mit Befund „Bauleistung“ gemeldet und beim Speichern
    /// unverändert zurückgeschrieben; die Datei bleibt bytegleich, auch nach
    /// einer Änderung an anderer Stelle.
    #[test]
    fn a311_ungueltige_bauleistung_bleibt_erhalten() {
        let mut m = house();
        let id = m.layer_sets().ids().next().unwrap();
        let mut s = m.layer_set(id).unwrap().clone();
        let svc = m.new_guid();
        s.layers[0].svc = Some(svc);
        assert!(m.set_layer_set(id, s));
        let set = m.layer_set(id).unwrap().guid;
        let text = write(&m);
        for roh in ["nix", "0Svc1", "-x-", "\"\""] {
            let kaputt = text.replace(&format!(" svc={}", svc.to_ifc()), &format!(" svc={roh}"));
            assert_ne!(kaputt, text);
            let back = load(&kaputt).unwrap();
            assert!(
                back.hints.iter().any(|h| h.contains("Bauleistung")),
                "{roh}: {:?}",
                back.hints
            );
            let layer0 = back
                .model
                .layer_sets()
                .iter()
                .find(|(_, s)| s.guid == set)
                .map(|(_, s)| s.layers[0].svc);
            assert_eq!(layer0, Some(None), "{roh}: nicht benutzt");
            assert_eq!(write(&back.model), kaputt, "{roh}: bytegleich");
            // Änderung an anderer Stelle: der rohe Wert bleibt stehen
            let mut b = back.model;
            let w = b.runs().iter().next().map(|(_, r)| r.segments[0]).unwrap();
            b.set_number(w, "AW-Süd").unwrap();
            let neu = write(&b);
            assert!(neu.contains(&format!(" svc={roh}")), "{roh}: nach Änderung");
        }
    }

    /// Sonnenstand S4 (§8 09:25): Ohne eingeschaltetes System kein
    /// `[sun]`, alte Dateien bleiben bytegleich; danach `date` und `time`,
    /// `on=1` nur, solange es an ist. Ansichtszustand: kein Schritt, keine
    /// neue Revision. Unlesbares bleibt roh mit Hinweis, bis ein Stand es
    /// ersetzt.
    #[test]
    fn s4_sonnenstand_in_der_datei() {
        use sk_math::sonne::Datum;
        let mut m = house();
        let alt = write(&m);
        assert!(!alt.contains("[sun]"));
        let rev = m.revision();
        let mut s = crate::Sun {
            date: Datum::new(2026, 6, 21).unwrap(),
            minutes: 12 * 60,
            on: true,
        };
        m.set_sun(s);
        assert_eq!(
            m.revision(),
            rev,
            "Ansichtszustand ändert die Revision nicht"
        );
        let an = write(&m);
        let zeilen: Vec<&str> = an.lines().filter(|z| z.starts_with("[sun]")).collect();
        assert_eq!(zeilen, ["[sun] date=2026-06-21 time=12:00 on=1"]);
        let back = load(&an).unwrap();
        assert!(back.hints.is_empty(), "{:?}", back.hints);
        assert_eq!(back.model.sun(), Some(s));
        assert_eq!(write(&back.model), an);
        s.on = false;
        s.minutes = 9 * 60 + 5;
        m.set_sun(s);
        let aus = write(&m);
        assert!(
            aus.contains("\n[sun] date=2026-06-21 time=09:05\n"),
            "{aus}"
        );
        assert_eq!(load(&aus).unwrap().model.sun(), Some(s));
        assert_eq!(aus.replace("[sun] date=2026-06-21 time=09:05\n", ""), alt);

        // Unlesbar: Hinweis, die Zeile bleibt bytegleich, bis ein Stand sie ersetzt
        for roh in [
            "[sun] date=2026-02-30 time=12:00",
            "[sun] date=21.06.2026 time=12:00 on=1",
            "[sun] date=2026-06-21 time=24:00",
            "[sun] date=2026-06-21",
            "[sun] date=2026-06-21 time=12:00 on=ja",
        ] {
            let text = alt.replacen("[storey]", &format!("{roh}\n[storey]"), 1);
            let l = load(&text).unwrap();
            assert_eq!(l.model.sun(), None, "{roh}");
            assert_eq!(l.hints.len(), 1, "{roh}: {:?}", l.hints);
            assert!(l.hints[0].contains("[sun]"), "{:?}", l.hints);
            let w = write(&l.model);
            assert_eq!(
                w.lines()
                    .filter(|z| z.starts_with("[sun]"))
                    .collect::<Vec<_>>(),
                [roh]
            );
            let mut b = l.model;
            b.set_sun(s);
            let neu = write(&b);
            assert!(!neu.lines().any(|z| z == roh), "{roh}: ersetzt");
            assert_eq!(neu.matches("[sun]").count(), 1);
        }
        let doppelt = an.replacen("[sun]", "[sun] date=2026-01-01 time=08:00\n[sun]", 1);
        assert!(load(&doppelt).is_err());
    }

    /// S11 (Jörn 09.10. 14:10): Unter Gelände gestrichelt je Ansicht, nur
    /// gewählt eine Zeile; alte Dateien bytegleich, keine neue Revision;
    /// Unlesbares bleibt roh mit Hinweis.
    #[test]
    fn s11_unter_gelaende_in_der_datei() {
        let mut m = house();
        let alt = write(&m);
        assert!(!alt.contains("[viewbelow]"));
        assert!(!m.view_below(1));
        let rev = m.revision();
        m.set_view_below(1, true);
        assert_eq!(m.revision(), rev, "Ansichtszustand");
        let neu = write(&m);
        assert!(
            neu.lines().any(|z| z == "[viewbelow] view=back dashed=1"),
            "{neu}"
        );
        let back = load(&neu).unwrap();
        assert!(back.hints.is_empty(), "{:?}", back.hints);
        assert!(back.model.view_below(1) && !back.model.view_below(0));
        assert_eq!(write(&back.model), neu);
        let mut m2 = back.model;
        m2.set_view_below(1, false);
        assert_eq!(write(&m2), alt);
        for roh in [
            "[viewbelow] view=oben dashed=1",
            "[viewbelow] view=back dashed=ja",
        ] {
            let text = alt.replacen("[storey]", &format!("{roh}\n[storey]"), 1);
            let l = load(&text).unwrap();
            assert_eq!(l.hints.len(), 1, "{roh}: {:?}", l.hints);
            assert!(!l.model.view_below(1));
            assert!(write(&l.model).lines().any(|z| z == roh), "{roh}: roh");
        }
    }

    /// Sonnenstand S7: Ohne eigene Wahl kein `[viewshade]`, alte Dateien
    /// bleiben bytegleich; je gewählter Ansicht eine Zeile, Rundlauf
    /// bytegleich, keine neue Revision. Unlesbares bleibt roh mit Hinweis,
    /// bis eine Wahl für diese Ansicht es ersetzt.
    #[test]
    fn s7_schatten_der_ansichten_in_der_datei() {
        use crate::{ShadeLight, ViewShade};
        let mut m = house();
        let alt = write(&m);
        assert!(!alt.contains("[viewshade]"));
        assert_eq!(m.view_shade(2, ViewShade::WERK), ViewShade::WERK);
        let rev = m.revision();
        let a = ViewShade {
            on: true,
            hatch: true,
            light: ShadeLight::Sun,
        };
        let b = ViewShade {
            on: false,
            hatch: false,
            light: ShadeLight::FrontRight,
        };
        m.set_view_shade(3, b, ViewShade::WERK);
        m.set_view_shade(0, a, ViewShade::WERK);
        assert_eq!(m.revision(), rev, "Ansichtszustand");
        let neu = write(&m);
        let zeilen: Vec<&str> = neu
            .lines()
            .filter(|z| z.starts_with("[viewshade]"))
            .collect();
        assert_eq!(
            zeilen,
            [
                "[viewshade] view=front on=1 fill=hatch light=sun",
                "[viewshade] view=right on=0 fill=area light=front-right",
            ]
        );
        let back = load(&neu).unwrap();
        assert!(back.hints.is_empty(), "{:?}", back.hints);
        assert_eq!(back.model.view_shade_own(0), Some(a));
        assert_eq!(back.model.view_shade_own(1), None);
        assert_eq!(back.model.view_shade(3, ViewShade::WERK), b);
        assert_eq!(write(&back.model), neu);
        let ohne: String = neu
            .lines()
            .filter(|z| !z.starts_with("[viewshade]"))
            .map(|z| format!("{z}\n"))
            .collect();
        assert_eq!(ohne, alt);

        // Unlesbar: Hinweis, die Zeile bleibt, bis die Ansicht gewählt wird
        for roh in [
            "[viewshade] view=oben on=1 fill=area light=sun",
            "[viewshade] view=back on=ja fill=area light=sun",
            "[viewshade] view=back on=1 fill=grau light=sun",
            "[viewshade] view=back on=1 fill=area light=hinten",
            "[viewshade] view=back on=1 fill=area",
        ] {
            let text = alt.replacen("[storey]", &format!("{roh}\n[storey]"), 1);
            let l = load(&text).unwrap();
            assert_eq!(l.hints.len(), 1, "{roh}: {:?}", l.hints);
            assert!(l.hints[0].contains("[viewshade]"), "{:?}", l.hints);
            assert_eq!(l.model.view_shade_own(1), None, "{roh}");
            let w = write(&l.model);
            assert!(w.lines().any(|z| z == roh), "{roh}: roh");
            let mut m2 = l.model;
            m2.set_view_shade(1, a, ViewShade::WERK);
            let w2 = write(&m2);
            let ersetzt = roh.contains("view=back");
            assert_eq!(!w2.lines().any(|z| z == roh), ersetzt, "{roh}");
        }
        // Dieselbe Ansicht doppelt: die zweite Zeile zählt nicht
        let doppelt = neu.replacen(
            "[viewshade] view=front",
            "[viewshade] view=front on=0 fill=area light=front-left\n[viewshade] view=front",
            1,
        );
        let l = load(&doppelt).unwrap();
        assert_eq!(l.hints.len(), 1, "{:?}", l.hints);
        assert!(!l.model.view_shade_own(0).unwrap().on);

        // „vorne oben“ (§8 11:55, Rückfrage a) im Rundlauf
        let oben = ViewShade {
            light: ShadeLight::Top,
            ..ViewShade::WERK
        };
        let mut m3 = back.model;
        m3.set_view_shade(1, oben, ViewShade::WERK);
        let w3 = write(&m3);
        assert!(w3
            .lines()
            .any(|z| z == "[viewshade] view=back on=1 fill=area light=top"));
        assert_eq!(load(&w3).unwrap().model.view_shade_own(1), Some(oben));
        // Gleicht eine Ansicht wieder der Vorgabe, entfällt ihre Zeile
        for i in 0..4 {
            m3.set_view_shade(i, ViewShade::WERK, ViewShade::WERK);
        }
        assert_eq!(m3.view_shade_own(0), None);
        assert_eq!(write(&m3), alt);
        // Eine andere Vorgabe (S8): dieselbe Wahl ist dann eigene Wahl
        let vorgabe = ViewShade {
            on: false,
            ..ViewShade::WERK
        };
        m3.set_view_shade(2, ViewShade::WERK, vorgabe);
        assert_eq!(m3.view_shade_own(2), Some(ViewShade::WERK));
    }

    /// Sonnenstand S1: Ohne Lage keine Zeile, die alte Datei bleibt
    /// bytegleich; mit Lage eine Zeile `[location]` hinter den
    /// Projektdaten, Rundlauf bytegleich; ein Schritt „Nordrichtung
    /// geändert“ und Strg+Z stellt die alte Datei her.
    #[test]
    fn s1_lage_und_nordrichtung() {
        let mut m = house();
        let alt = write(&m);
        assert!(!alt.contains("[location]"));
        assert_eq!(write(&load(&alt).unwrap().model), alt);
        assert!(load(&alt).unwrap().model.location().is_unset());

        m.begin("Nordrichtung geändert");
        let l = Location {
            lat: Some(53.0589),
            lon: Some(8.591),
            north: Some(-12.5),
        };
        assert!(m.set_location(l));
        assert!(!m.set_location(l), "gleiche Werte ändern nichts");
        let t = m.commit().expect("ein Schritt");
        assert_eq!(t.label, "Nordrichtung geändert");
        assert_eq!(t.changes.len(), 1);
        let neu = write(&m);
        let zeilen: Vec<&str> = neu
            .lines()
            .filter(|z| z.starts_with("[location]"))
            .collect();
        assert_eq!(zeilen, ["[location] lat=53.0589 lon=8.591 north=347.5"]);
        let vor = neu.find("[location]").unwrap();
        assert!(neu.find("[project]").unwrap() < vor);
        assert!(vor < neu.find("[building]").unwrap_or(neu.len()));
        assert!(vor < neu.find("[storey]").unwrap());

        let back = load(&neu).unwrap();
        assert!(back.hints.is_empty(), "{:?}", back.hints);
        assert_eq!(back.model.location().north, Some(347.5));
        assert_eq!(back.model.revision(), 0, "Laden ist keine Änderung");
        assert_eq!(write(&back.model), neu);

        m.apply(&t, Direction::Undo);
        assert_eq!(write(&m), alt);
        m.apply(&t, Direction::Redo);
        assert_eq!(write(&m), neu);

        // Nur die Nordrichtung: Breite und Länge bleiben ungeschrieben
        let nur = alt.replacen("[storey]", "[location] north=90\n[storey]", 1);
        let l = load(&nur).unwrap();
        assert_eq!(l.model.location().lat, None);
        assert_eq!(l.model.location().north, Some(90.0));
        assert!(write(&l.model).contains("\n[location] north=90\n"));
    }

    /// Sonnenstand S1: Unbekanntes an `[location]` (neuere Fassung) bleibt;
    /// eine unlesbare Zeile bleibt roh (Befund A); eine zweite Zeile ist ein
    /// Fehler mit der Zeile.
    #[test]
    fn s1_lage_fremd_und_kaputt() {
        let alt = write(&house());
        // an der Stelle, an die der Schreiber [location] setzt
        let mit = |z: &str| alt.replacen("[building]", &format!("{z}\n[building]"), 1);
        let fremd = mit("[location] lat=52.5 lon=13.4 north=0 elev=34");
        let l = load(&fremd).unwrap();
        assert_eq!(l.hints.len(), 1, "{:?}", l.hints);
        assert!(l.hints[0].contains("„elev“"));
        assert_eq!(l.model.location().north, Some(0.0));
        let text = write(&l.model);
        assert!(text.contains("\n[location] lat=52.5 lon=13.4 north=0 elev=34\n"));

        // Befund A: Unlesbares öffnet das Projekt, die Zeile zählt nicht und
        // bleibt bytegleich; eine gesetzte Lage ersetzt sie (genau eine
        // Zeile), Rückgängig holt sie zurück
        for z in [
            "[location] lat=95 lon=13.4",
            "[location] lat=abc",
            "[location] lat=53°03' lon=8.591 north=12",
            "[location] north=inf elev=3",
        ] {
            let kaputt = mit(z);
            let l = load(&kaputt).unwrap_or_else(|e| panic!("{z}: {e:?}"));
            assert_eq!(l.hints.len(), 1, "{z}: {:?}", l.hints);
            assert!(
                l.hints[0].contains("keine Zahl im Bereich"),
                "{:?}",
                l.hints
            );
            assert!(l.model.location().is_unset(), "{z}");
            assert_eq!(write(&l.model), kaputt, "{z}: bytegleich");
            let mut m = l.model;
            m.begin("Nordrichtung geändert");
            assert!(m.set_location(Location {
                north: Some(30.0),
                ..Default::default()
            }));
            let t = m.commit().expect("ein Schritt");
            let neu = write(&m);
            let zeilen: Vec<&str> = neu
                .lines()
                .filter(|z| z.starts_with("[location]"))
                .collect();
            assert_eq!(zeilen, ["[location] north=30"], "{z}");
            m.apply(&t, crate::txn::Direction::Undo);
            assert_eq!(write(&m), kaputt, "{z}: Rückgängig");
            m.apply(&t, crate::txn::Direction::Redo);
            assert_eq!(write(&m), neu, "{z}: Wiederholen");
            // Leere Lage übernommen (Maske ohne Breite und Länge): die Zeile bleibt
            let mut m = load(&kaputt).unwrap().model;
            m.begin("Projektdaten geändert");
            assert!(!m.set_location(Location::default()));
            assert!(m.commit().is_none());
            assert_eq!(write(&m), kaputt);
        }
        // Nur Fremdes oder leer (Review 3br): zählt nicht als Lage, bleibt
        // beim Speichern ohne Änderung im Wortlaut; eine gesetzte Lage
        // ersetzt die Zeile, Rückgängig holt sie zurück
        for z in [
            "[location] elev=34",
            "[location] lat=95 elev=34",
            "[location]",
        ] {
            let f = mit(z);
            let l = load(&f).unwrap();
            assert!(l.model.location().is_unset(), "{z}");
            let text = write(&l.model);
            assert_eq!(text, f, "{z}");
            assert_eq!(write(&load(&text).unwrap().model), text, "{z}");
            let mut m = l.model;
            m.begin("Nordrichtung geändert");
            assert!(m.set_location(Location {
                north: Some(5.0),
                ..Default::default()
            }));
            let t = m.commit().unwrap();
            let neu = write(&m);
            let zeilen: Vec<&str> = neu
                .lines()
                .filter(|z| z.starts_with("[location]"))
                .collect();
            assert_eq!(zeilen, ["[location] north=5"], "{z}");
            m.apply(&t, crate::txn::Direction::Undo);
            assert_eq!(write(&m), f, "{z}: Rückgängig");
        }
        let l = load(&mit("[location] elev=34")).unwrap();
        assert_eq!(l.hints.len(), 1, "{:?}", l.hints);
        assert!(l.hints[0].contains("„elev“"), "{:?}", l.hints);

        // Gesetzt und im selben Schritt wieder leer: kein leerer Schritt,
        // der die Zeile stillschweigend verliert
        let kaputt = mit("[location] lat=abc");
        let mut m = load(&kaputt).unwrap().model;
        m.begin("Projektdaten geändert");
        assert!(m.set_location(Location {
            lat: Some(1.0),
            ..Default::default()
        }));
        assert!(m.set_location(Location::default()));
        let t = m.commit().expect("Schritt mit der Zeile");
        m.apply(&t, crate::txn::Direction::Undo);
        assert_eq!(write(&m), kaputt);

        assert!(load(&mit("[location] north=1\n[location] north=2")).is_err());
    }

    /// Sonnenstand S2: Der Fußpunkt des Nordpfeils steht als `x=`/`y=` an
    /// `[location]`, nur zusammen und nur mit `north`; ein Schritt mit der
    /// Richtung, Rundlauf und Rückgängig bytegleich. Eine Lücke oder
    /// Unbrauchbares lässt die Zeile roh stehen (Befund A).
    #[test]
    fn s2_fusspunkt() {
        let mut m = house();
        let alt = write(&m);
        m.begin("Nordrichtung geändert");
        assert!(m.set_north_arrow(372.0, Some([12500.0, -3000.5])));
        assert!(!m.set_north_arrow(12.0, Some([12500.0, -3000.5])));
        let t = m.commit().unwrap();
        assert_eq!(t.changes.len(), 1);
        let neu = write(&m);
        let zeilen: Vec<&str> = neu
            .lines()
            .filter(|z| z.starts_with("[location]"))
            .collect();
        assert_eq!(zeilen, ["[location] north=12 x=12500 y=-3000.5"]);
        let back = load(&neu).unwrap();
        assert!(back.hints.is_empty(), "{:?}", back.hints);
        assert_eq!(back.model.north_foot(), Some([12500.0, -3000.5]));
        assert_eq!(write(&back.model), neu);
        // Verschieben: ein eigener Schritt, Richtung bleibt
        m.begin("Nordpfeil verschoben");
        assert!(m.set_north_arrow(12.0, Some([0.0, 0.0])));
        let t2 = m.commit().unwrap();
        assert!(write(&m).contains("\n[location] north=12 x=0 y=0\n"));
        // Breite über die Maske: der Fußpunkt bleibt
        m.begin("Projektdaten geändert");
        let l = Location {
            lat: Some(52.5),
            ..*m.location()
        };
        assert!(m.set_location(l));
        let t3 = m.commit().unwrap();
        assert!(write(&m).contains("\n[location] lat=52.5 north=12 x=0 y=0\n"));
        for t in [&t3, &t2] {
            m.apply(t, crate::txn::Direction::Undo);
        }
        assert_eq!(write(&m), neu);
        m.apply(&t, crate::txn::Direction::Undo);
        assert_eq!(write(&m), alt);

        let mit = |z: &str| alt.replacen("[building]", &format!("{z}\n[building]"), 1);
        for (z, k) in [
            ("[location] north=12 x=1", "„y“ fehlt"),
            ("[location] north=12 y=1", "„x“ fehlt"),
            ("[location] x=1 y=2", "„north“ fehlt"),
            ("[location] north=12 x=1 y=abc", "„y“ ist keine Zahl"),
            ("[location] north=12 x=1e12 y=0", "„x“ ist keine Zahl"),
        ] {
            let f = mit(z);
            let l = load(&f).unwrap();
            assert_eq!(l.hints.len(), 1, "{z}: {:?}", l.hints);
            assert!(l.hints[0].contains(k), "{z}: {:?}", l.hints);
            assert_eq!(l.hints[0].matches("„").count(), 1, "{:?}", l.hints);
            assert!(l.model.location().is_unset(), "{z}");
            assert_eq!(l.model.north_foot(), None);
            assert_eq!(write(&l.model), f, "{z}");
        }
    }
}
