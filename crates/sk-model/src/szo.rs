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
    LevelRef, PropSet, PropValue, Storey, StripFooting, Wall, WallRun,
};
use crate::guid::{Guid, GuidGen};
use crate::id::{Arena, Id};
use crate::library::{
    type_code, Bearing, LayerFunction, LayerSet, LayerSetId, MatCategory, Material, MaterialLayer,
    TypeCategory,
};
use crate::model::{Defaults, Model, Project};
use crate::solid::edge_kind;
use crate::wall::{segment_count, RefSide};
use sk_math::vec3;
use std::cell::Cell;
use std::collections::HashMap;
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
        }))
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
    }
}

const MAT_CATEGORIES: [MatCategory; 6] = MatCategory::ALL;

fn layer_function(f: LayerFunction) -> &'static str {
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
    match c {
        Category::ExteriorWall => "exterior",
        Category::InteriorWall => "interior",
        Category::Floor => "floor",
        Category::GroundSlab => "groundslab",
        Category::Roof => "roof",
        Category::Window => "window",
        Category::Door => "door",
        Category::Opening => "opening",
        Category::Space => "space",
        Category::StripFooting => "stripfooting",
        Category::EdgeInsulation => "edgeinsulation",
    }
}

pub(crate) fn type_category(c: TypeCategory) -> &'static str {
    match c {
        TypeCategory::ExteriorWall => "exterior",
        TypeCategory::InteriorWall => "interior",
    }
}

fn ref_side(r: RefSide) -> &'static str {
    match r {
        RefSide::Left => "left",
        RefSide::Right => "right",
        RefSide::Center => "center",
    }
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
    line.guid("fill", fill)
        .guid("fg", fg)
        .guid("bg", bg)
        .guid("surface", surface)
        .finish(out);
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
        Line::new("layer")
            .guid("set", Some(s.guid))
            .guid("mat", mat_guid(l.material))
            .num("t", l.thickness)
            .word("fn", layer_function(l.function))
            .flag("core", l.core)
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
    Line::new("display")
        .word("slot", "paper")
        .color("color", d.paper)
        .finish(&mut out);

    let mat_guid = |id| m.material(id).map(|x| x.guid);
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
    Line::new("project")
        .guid("guid", Some(p.guid))
        .text("name", &p.name)
        .guid("storey", storey_guid(defaults.storey))
        .guid(
            "wallset",
            m.layer_set(defaults.exterior_wall).map(|s| s.guid),
        )
        .guid("iwset", m.layer_set(defaults.interior_wall).map(|s| s.guid))
        .finish(&mut out);
    for b in sorted(m.buildings().iter(), |b| b.guid) {
        Line::new("building")
            .guid("guid", Some(b.guid))
            .text("name", &b.name)
            .text("number", &b.number)
            .finish(&mut out);
    }
    for s in sorted(m.storeys().iter(), |s| s.guid) {
        Line::new("storey")
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
            .num("h", s.height)
            .finish(&mut out);
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
            Some(c) => line
                .guid("below", m.element(c.below).map(|x| x.guid))
                .num("off", c.offset),
            None => line,
        }
        .finish(&mut out);
    }
    for e in &walls {
        let ElementKind::GroundSlab(s) = e.kind else {
            continue;
        };
        Line::new("slab")
            .guid("guid", Some(e.guid))
            .guid("run", m.run(s.run).map(|r| r.guid))
            .text("number", &e.number)
            .word("cat", category(e.category))
            .guid("mat", mat_guid(s.material))
            .word("top", &level(s.top))
            .num("t", s.thickness)
            .num("recess", s.recess)
            .num("seq", e.seq)
            .guid("storey", storey_guid(e.storey))
            .finish(&mut out);
    }
    for e in &walls {
        let ElementKind::StripFooting(f) = e.kind else {
            continue;
        };
        Line::new("footing")
            .guid("guid", Some(e.guid))
            .guid("slab", m.element(f.slab).map(|x| x.guid))
            .text("number", &e.number)
            .word("cat", category(e.category))
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
        Line::new("floor")
            .guid("guid", Some(e.guid))
            .guid("run", m.run(f.run).map(|r| r.guid))
            .text("number", &e.number)
            .word("cat", category(e.category))
            .guid("mat", mat_guid(f.material))
            .word("top", &level(f.top))
            .num("t", f.thickness)
            .num("seq", e.seq)
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
        write_props(&mut out, "prop", "elem", e.guid, &e.props);
    }
    out
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

/// Liest eine `.szo`-Datei. Neue Laufzeit-Kennungen, Guids aus der Datei; neue
/// Guids kommen aus `guids`. Bei einem Fehler wird nichts übernommen.
pub fn read(text: &str, mut guids: GuidGen) -> Result<Loaded, LoadError> {
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
    let mut by: HashMap<&str, Vec<Record>> = HashMap::new();
    const KNOWN: [&str; 19] = [
        "pen", "linetype", "fill", "surface", "display", "material", "layerset", "layer",
        "typeprop", "project", "building", "storey", "run", "wall", "slab", "footing", "floor",
        "strip", "prop",
    ];
    for (i, l) in text.lines().enumerate().skip(1) {
        let Some(r) = Record::parse(i + 1, l)? else {
            continue;
        };
        match KNOWN.iter().find(|k| **k == r.section) {
            Some(k) => by.entry(k).or_default().push(r),
            None => hints.push(format!(
                "Zeile {}: unbekannter Abschnitt [{}] übersprungen",
                i + 1,
                r.section
            )),
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
    let display = read_display(
        recs("display"),
        &pens,
        &line_types,
        &pen_ids,
        &lt_ids,
        &mut hints,
    )?;

    // Bibliothek
    let mut materials = Arena::new();
    let mut mat_ids = HashMap::new();
    for r in recs("material") {
        let x = read_material(r, &fill_ids, &pen_ids, &surface_ids)?;
        let g = x.guid;
        let id = materials.insert(x);
        register(&mut mat_ids, &mut seen, r, g, id)?;
    }
    let (mut layer_sets, set_ids) = read_types(&by, &mat_ids, &mut seen, version >= 4)?;

    // Projekt, Gebäude und Geschosse
    let mut buildings = Arena::new();
    let mut building_ids = HashMap::new();
    let mut building_numbers: HashMap<String, usize> = HashMap::new();
    for r in recs("building") {
        let b = Building {
            guid: r.guid("guid")?,
            name: r.get("name")?.to_string(),
            number: r.get("number")?.to_string(),
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
    let project = Project {
        guid: p.guid("guid")?,
        name: p.get("name")?.to_string(),
    };
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
        });
        storeys.insert(Storey {
            guid: guids.next_guid(),
            building: None,
            name: "Obergeschoss".into(),
            short: "OG".into(),
            kind: LevelKind::Storey,
            elevation: crate::model::STOREY_HEIGHT,
            height: crate::model::STOREY_HEIGHT,
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
        };
        if r.opt("below").is_some() {
            below.push((r.line, r.guid("below")?, r.f64("off")?, e.guid));
        }
        let g = e.guid;
        let id = elements.insert(e);
        register(&mut elem_ids, &mut seen, r, g, id)?;
        slots.entry(run).or_default().push((seg, id));
    }
    // Kopplungen erst, wenn alle Wände gelesen sind (Verweise nach vorn)
    for (line, g, offset, me) in below {
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
    for (section, ix) in [("slab", 0), ("footing", 1), ("floor", 2), ("strip", 3)] {
        for r in recs(section) {
            let kind = if ix == 3 {
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
                })
            } else if ix == 0 {
                ElementKind::GroundSlab(GroundSlab {
                    run: r.link("run", &run_ids)?,
                    material: r.link("mat", &mat_ids)?,
                    top: level(r, "top", LevelRef::bottom(ground))?,
                    thickness: r.f64("t")?,
                    recess: r.f64("recess")?,
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
            let e = Element {
                guid: r.guid("guid")?,
                number: number(r)?,
                category: keyword(r, "cat", &Category::ALL, category)?,
                storey: r.link("storey", &storey_ids)?,
                layer_set: None,
                seq: r.int("seq")?,
                kind,
                props: Default::default(),
            };
            let g = e.guid;
            let id = elements.insert(e);
            register(&mut elem_ids, &mut seen, r, g, id)?;
        }
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
        let id = r.link("elem", &elem_ids)?;
        let key = r.get("key")?.to_string();
        let value = read_prop_value(r)?;
        elements
            .get_mut(id)
            .expect("eben angelegt")
            .props
            .insert(key, value);
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
    hints.extend(model.complete_pre_b9());
    hints.extend(model.complete_line_types());
    hints.extend(model.complete_pre_b10());
    if !v3 {
        hints.extend(model.complete_pre_b12());
    }
    hints.extend(model.check());
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
    Ok(Material {
        guid: r.guid("guid")?,
        name: r.get("name")?.to_string(),
        category: keyword(r, "cat", &MAT_CATEGORIES, mat_category)?,
        priority: r.int("prio")?,
        density: r.f64("rho")?,
        lambda,
        cut_fill: r.link("fill", fill_ids)?,
        cut_fg: r.link("fg", pen_ids)?,
        cut_bg: r.link("bg", pen_ids)?,
        surface: r.link("surface", surface_ids)?,
    })
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

/// Bauteiltypen aus `[layerset]`, `[layer]` und `[typeprop]`. Vor SZO 4
/// (`typed` falsch) fehlen Kurzzeichen und Art; sie stellt der Aufrufer
/// danach ein ([`assign_codes`]).
#[allow(clippy::type_complexity)]
pub(crate) fn read_types(
    by: &HashMap<&str, Vec<Record>>,
    mat_ids: &HashMap<Guid, Id<Material>>,
    seen: &mut HashMap<Guid, usize>,
    typed: bool,
) -> Result<(Arena<LayerSet>, HashMap<Guid, LayerSetId>), LoadError> {
    let empty = Vec::new();
    let recs = |k: &str| by.get(k).unwrap_or(&empty);
    let mut set_layers: HashMap<Guid, Vec<MaterialLayer>> = HashMap::new();
    for r in recs("layer") {
        let set = r.guid("set")?;
        let layer = MaterialLayer {
            material: r.link("mat", mat_ids)?,
            thickness: r.f64("t")?,
            function: keyword(r, "fn", &LAYER_FUNCTIONS, layer_function)?,
            core: r.flag("core")?,
        };
        set_layers.entry(set).or_default().push(layer);
    }
    let mut set_props: HashMap<Guid, PropSet> = HashMap::new();
    for r in recs("typeprop") {
        let set = r.guid("set")?;
        let key = r.get("key")?.to_string();
        let value = read_prop_value(r)?;
        set_props.entry(set).or_default().insert(key, value);
    }
    let mut layer_sets = Arena::new();
    let mut set_ids = HashMap::new();
    for r in recs("layerset") {
        let guid = r.guid("guid")?;
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
    Ok((layer_sets, set_ids))
}

/// Ergänzt fehlendes λ an Werksbaustoffen (Nachtrag K5): Treffer über die
/// Guid, sonst über Name und Kategorie der vier alten Startbaustoffe (Dateien
/// vor 137fca7 haben zeitbasierte Guids). Vorhandenes λ bleibt; das Modell
/// gilt danach als unverändert.
fn add_lambda(materials: &mut Arena<Material>) {
    const OLD: [&str; 4] = ["Gasbeton", "Dämmung (WDVS)", "Stahlbeton", "Putz"];
    let werk = Model::new();
    let ids: Vec<_> = materials.ids().collect();
    for id in ids {
        let Some(x) = materials.get(id).filter(|x| x.lambda.is_none()) else {
            continue;
        };
        let by_guid = werk.materials().iter().find(|(_, w)| w.guid == x.guid);
        let by_name = || {
            werk.materials().iter().find(|(_, w)| {
                OLD.contains(&w.name.as_str()) && w.name == x.name && w.category == x.category
            })
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
        });
        m
    }

    fn load(text: &str) -> Result<Loaded, LoadError> {
        read(text, GuidGen::with_seed(99))
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

    #[test]
    fn unbekannter_abschnitt_und_schluessel_werden_uebersprungen() {
        let a = write(&house());
        let b = a.replacen("[storey]", "[zukunft] x=1\n[storey] neu=\"ja\"", 1);
        let l = load(&b).unwrap();
        assert_eq!(l.hints.len(), 2, "{:?}", l.hints);
        assert!(l.hints[0].contains("[zukunft]"));
        assert!(l.hints[1].contains("„neu“"));
        assert_eq!(write(&l.model), a);
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
}
