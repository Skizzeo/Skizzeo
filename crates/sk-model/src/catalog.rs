//! Firmenkatalog `.szk` (K2): Bauteiltypen mit ihren Baustoffen und deren
//! Darstellung außerhalb des Projekts, als Bürostandard. Gleiches
//! Zeilenformat wie die `.szo`, ohne Geschosse, Gebäude und Bauteile.
//!
//! Abgleich mit dem Projektkatalog nur über die Guid: Übernehmen holt einen
//! Typ ins Projekt, Zurückspeichern schreibt ihn in den Katalog. Was im Ziel
//! fehlt, kommt dazu; Baustoffe und Darstellung, die es dort schon gibt,
//! bleiben, wie sie sind (Regel 8). Datei lesen und schreiben macht die App.

use crate::attr::{Fill, Pen, Surface};
use crate::guid::Guid;
use crate::id::{Arena, Id};
use crate::library::{
    Bearing, LayerSet, LayerSetId, MatCategory, Material, MaterialDisplay, MaterialId, TypeCategory,
};
use crate::model::{free_code, same_type, Model, ETICS_TYPE_GUID, EXTERIOR_TYPE_GUID};
use crate::szo::{
    self, check_header, err, keyword, register, sorted, type_category, Line, LoadError, Record,
};
use crate::trade::{self, Trade};
use std::collections::HashMap;

/// Hauptversion des Formats.
pub const VERSION: u32 = 1;

/// Inhalt eines Firmenkatalogs. Verweise sind Kennungen in die eigenen
/// Tabellen. Zwei Kataloge sind gleich, wenn ihr Text gleich ist
/// ([`write_szk`] schreibt nach Guid sortiert).
#[derive(Clone, Debug, Default)]
pub struct Library {
    pub pens: Arena<Pen>,
    pub fills: Arena<Fill>,
    pub surfaces: Arena<Surface>,
    pub materials: Arena<Material>,
    pub types: Arena<LayerSet>,
    /// Gewerke (Paket 1a); geschrieben werden nur die verwendeten.
    pub trades: Vec<Trade>,
    /// Standardtypen für neue Projekte.
    pub default_exterior: Option<LayerSetId>,
    pub default_interior: Option<LayerSetId>,
    /// Werkstypen, die der Katalog schon angeboten bekam (K4): fehlt einer
    /// davon, hat das Büro ihn entfernt, und er kommt nicht wieder.
    pub stock: Vec<Guid>,
    /// Was eine neuere Fassung geschrieben hat und dieser Leser nicht kennt.
    pub foreign: Foreign,
}

/// Fremdes aus einer neueren Fassung (F-17): bleibt beim Zurückschreiben
/// bytegleich erhalten.
#[derive(Clone, Debug, Default)]
pub struct Foreign {
    /// Zeilen mit unbekannten Angaben: so, wie dieser Leser den Eintrag
    /// schreibt, und wie er in der Datei stand. Solange der Eintrag
    /// unverändert ist, wird die Zeile der Datei geschrieben.
    pub lines: Vec<(String, String)>,
    /// Sätze unbekannter Art, unverändert in Dateireihenfolge.
    pub records: Vec<String>,
    /// Zahl der unbekannten Angaben beim Lesen (Wert, Schlüssel, Satz).
    pub unknown: usize,
}

impl PartialEq for Library {
    fn eq(&self, other: &Library) -> bool {
        write_szk(self) == write_szk(other)
    }
}

impl Library {
    /// Kurzzeichen der Typen mit ungültigem Deckenauflager (Regel 21, wie
    /// [`Model::bearing_problem`]). Sie werden übernommen und wie „ganze
    /// tragende Schicht“ gebaut.
    pub fn invalid_bearings(&self) -> Vec<String> {
        self.types
            .iter()
            .filter(|(_, t)| match t.bearing {
                Bearing::Core => false,
                Bearing::Depth { strip, .. } => {
                    t.bearing_problem().is_some()
                        || !self
                            .materials
                            .get(strip)
                            .is_some_and(|m| m.category == MatCategory::Insulation)
                }
            })
            .map(|(_, t)| {
                if t.code.is_empty() {
                    t.name.clone()
                } else {
                    t.code.clone()
                }
            })
            .collect()
    }

    /// Alle Typen eines Projekts mit seinen Standardtypen, z. B. der
    /// eingebaute Startbestand aus [`Model::new`].
    pub fn from_model(m: &Model) -> Library {
        let mut lib = Library {
            trades: m.trades().to_vec(),
            ..Library::default()
        };
        let mut order: Vec<Guid> = m.layer_sets().iter().map(|(_, t)| t.guid).collect();
        order.sort();
        for g in order {
            export_type(m, &mut lib, g);
        }
        let d = m.defaults();
        lib.default_exterior = m
            .layer_set(d.exterior_wall)
            .and_then(|t| lib.type_by_guid(t.guid));
        lib.default_interior = m
            .layer_set(d.interior_wall)
            .and_then(|t| lib.type_by_guid(t.guid));
        lib
    }

    /// Eingebauter Startbestand: die Werkstypen eines neuen Projekts.
    pub fn standard() -> Library {
        let mut lib = Library::from_model(&Model::new());
        lib.stock = lib.types.iter().map(|(_, t)| t.guid).collect();
        lib.stock.sort();
        lib
    }

    /// Ergänzt die Werkstypen, die der Katalog noch nicht angeboten bekam
    /// (nach Guid; vorhandene bleiben, wie sie sind). Steht der Standard
    /// noch auf dem alten Werkstyp AW-31,5, wechselt er einmalig auf AW-36.
    /// Fehlendes λ an Werksbaustoffen kommt dazu. Ergebnis: Kurzzeichen der
    /// ergänzten Typen und ob der Standard gewechselt hat; beides leer bzw.
    /// `false` heißt: nichts geändert.
    pub fn add_stock(&mut self) -> (Vec<String>, bool) {
        let werk = Model::new();
        let mut order: Vec<Guid> = werk.layer_sets().iter().map(|(_, t)| t.guid).collect();
        order.sort();
        let fresh: Vec<Guid> = order
            .into_iter()
            .filter(|g| !self.stock.contains(g))
            .collect();
        if fresh.is_empty() {
            return (Vec::new(), false);
        }
        let mut added = Vec::new();
        for g in &fresh {
            if self.type_by_guid(*g).is_none() && export_type(&werk, self, *g) {
                if let Some(t) = self.type_by_guid(*g).and_then(|id| self.types.get(id)) {
                    added.push(t.code.clone());
                }
            }
            self.stock.push(*g);
        }
        self.stock.sort();
        for id in self.materials.ids().collect::<Vec<_>>() {
            if let Some(x) = self.materials.get_mut(id).filter(|x| x.lambda.is_none()) {
                x.lambda = werk
                    .materials()
                    .iter()
                    .find(|(_, w)| w.guid == x.guid)
                    .and_then(|(_, w)| w.lambda);
            }
        }
        let old_default = self
            .default_exterior
            .and_then(|id| self.types.get(id))
            .is_some_and(|t| t.guid == EXTERIOR_TYPE_GUID);
        let etics = self.type_by_guid(ETICS_TYPE_GUID);
        let switched = fresh.contains(&ETICS_TYPE_GUID) && old_default && etics.is_some();
        if switched {
            self.default_exterior = etics;
        }
        (added, switched)
    }

    pub fn type_by_guid(&self, g: Guid) -> Option<LayerSetId> {
        self.types
            .iter()
            .find(|(_, t)| t.guid == g)
            .map(|(id, _)| id)
    }

    /// Standardtyp der Art: der eingetragene, sonst der erste der Art.
    pub fn default_type(&self, cat: TypeCategory) -> Option<LayerSetId> {
        let set = match cat {
            TypeCategory::ExteriorWall => self.default_exterior,
            TypeCategory::InteriorWall => self.default_interior,
            // Nur Wandarten haben einen Standardtyp
            _ => return None,
        };
        set.filter(|id| self.types.get(*id).is_some_and(|t| t.category == cat))
            .or_else(|| {
                let mut v: Vec<(Guid, LayerSetId)> = self
                    .types
                    .iter()
                    .filter(|(_, t)| t.category == cat)
                    .map(|(id, t)| (t.guid, id))
                    .collect();
                v.sort_by_key(|x| x.0);
                v.first().map(|x| x.1)
            })
    }

    fn mat_guid(&self, id: MaterialId) -> Option<Guid> {
        self.materials.get(id).map(|x| x.guid)
    }

    /// Nächste freie Stiftnummer im Katalog.
    fn next_pen_number(&self) -> u16 {
        let max = self.pens.iter().map(|(_, p)| p.number).max();
        max.map_or(1, |n| n.saturating_add(1))
    }
}

// --- Datei ----------------------------------------------------------------

/// Kennung eines Satzes zum Wiederfinden in der eigenen Ausgabe: Abschnitt
/// und Guid, ohne Guid die Schlüsselfelder ([default]: Art, [typeprop]:
/// Typ und Merkmal, [layer]: Typ). Mehrere Sätze mit derselben Kennung
/// (Schichten eines Typs) zählen in Dateireihenfolge durch.
fn record_key(r: &Record, count: &mut HashMap<String, usize>) -> String {
    let base = match r.opt("guid") {
        Some(g) => format!("{} guid={g}", r.section),
        None => format!(
            "{} set={} cat={} key={}",
            r.section,
            r.opt("set").unwrap_or(""),
            r.opt("cat").unwrap_or(""),
            r.opt("key").unwrap_or("")
        ),
    };
    let n = count.entry(base.clone()).or_insert(0);
    *n += 1;
    format!("{base} #{n}")
}

/// Der Katalog als `.szk`-Text.
pub fn write_szk(lib: &Library) -> String {
    let plain = write_known(lib);
    let f = &lib.foreign;
    if f.lines.is_empty() && f.records.is_empty() {
        return plain;
    }
    let mut out = String::with_capacity(plain.len() + 256);
    // Jede gemerkte Zeile einmal: gleiche eigene Zeilen (zwei gleiche
    // Schichten) bekommen ihre fremden Zeilen der Reihe nach
    let mut used = vec![false; f.lines.len()];
    for l in plain.lines() {
        let hit = f
            .lines
            .iter()
            .enumerate()
            .find(|(i, (mine, _))| !used[*i] && mine == l);
        let l = match hit {
            Some((i, (_, theirs))) => {
                used[i] = true;
                theirs.as_str()
            }
            None => l,
        };
        out.push_str(l);
        out.push('\n');
    }
    for r in &f.records {
        out.push_str(r);
        out.push('\n');
    }
    out
}

/// Was dieser Leser kennt, ohne Fremdes.
fn write_known(lib: &Library) -> String {
    let mut out = format!("SZK {VERSION}\n# Skizzeo-Firmenkatalog\n");
    for p in sorted(lib.pens.iter(), |p| p.guid) {
        szo::write_pen(&mut out, p);
    }
    for f in sorted(lib.fills.iter(), |f| f.guid) {
        szo::write_fill(&mut out, f);
    }
    for s in sorted(lib.surfaces.iter(), |s| s.guid) {
        szo::write_surface(&mut out, s);
    }
    let pen = |id| lib.pens.get(id).map(|p| p.guid);
    let used = szo::used_trades(
        lib.materials.iter().map(|(_, x)| x),
        lib.types.iter().map(|(_, t)| t),
    );
    szo::write_trades(&mut out, &lib.trades, &used);
    for x in sorted(lib.materials.iter(), |x| x.guid) {
        let refs = [
            lib.fills.get(x.cut_fill).map(|f| f.guid),
            pen(x.cut_fg),
            pen(x.cut_bg),
            lib.surfaces.get(x.surface).map(|s| s.guid),
        ];
        szo::write_material(&mut out, x, refs);
    }
    for t in sorted(lib.types.iter(), |t| t.guid) {
        szo::write_type(&mut out, t, |id| lib.mat_guid(id));
    }
    for (cat, set) in [
        (TypeCategory::ExteriorWall, lib.default_exterior),
        (TypeCategory::InteriorWall, lib.default_interior),
    ] {
        if let Some(t) = set.and_then(|id| lib.types.get(id)) {
            Line::new("default")
                .word("cat", type_category(cat))
                .guid("set", Some(t.guid))
                .finish(&mut out);
        }
    }
    for g in &lib.stock {
        Line::new("stock").guid("set", Some(*g)).finish(&mut out);
    }
    out
}

/// Liest einen Firmenkatalog. Bei einem Fehler wird nichts übernommen; der
/// Fehler nennt die Zeile.
pub fn read_szk(text: &str) -> Result<Library, LoadError> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    check_header(text.lines().next(), "SZK", VERSION)?;
    let mut by: HashMap<&str, Vec<Record>> = HashMap::new();
    const KNOWN: [&str; 11] = [
        "pen", "linetype", "fill", "surface", "trade", "material", "layerset", "layer", "typeprop",
        "default", "stock",
    ];
    let mut foreign = Foreign::default();
    let lines: Vec<&str> = text.lines().collect();
    // Zeilen, die unverändert stehen bleiben, mit Zeilennummer
    let mut alien: Vec<usize> = Vec::new();
    for (i, l) in lines.iter().enumerate().skip(1) {
        let Some(r) = Record::parse(i + 1, l)? else {
            continue;
        };
        // Unbekannte Abschnitte (neuere Kataloge) bleiben unverändert stehen
        match KNOWN.iter().find(|k| **k == r.section) {
            Some(k) => by.entry(k).or_default().push(r),
            None => {
                alien.push(i + 1);
                foreign.unknown += 1;
            }
        }
    }
    let empty = Vec::new();
    let recs = |k: &str| by.get(k).unwrap_or(&empty);
    let mut seen = HashMap::new();
    let mut lib = Library::default();
    let mut pen_ids = HashMap::new();
    let mut numbers = HashMap::new();
    for r in recs("pen") {
        let p = szo::read_pen(r)?;
        if let Some(first) = numbers.insert(p.number, r.line) {
            return Err(err(
                r.line,
                format!("Stiftnummer {} doppelt (schon in Zeile {first})", p.number),
            ));
        }
        let g = p.guid;
        let id = lib.pens.insert(p);
        register(&mut pen_ids, &mut seen, r, g, id)?;
    }
    let mut fill_ids = HashMap::new();
    let mut hints = Vec::new();
    for r in recs("fill") {
        let f = szo::read_fill(r, &mut hints)?;
        let g = f.guid;
        let id = lib.fills.insert(f);
        register(&mut fill_ids, &mut seen, r, g, id)?;
    }
    let mut surface_ids = HashMap::new();
    for r in recs("surface") {
        let s = szo::read_surface(r)?;
        let g = s.guid;
        let id = lib.surfaces.insert(s);
        register(&mut surface_ids, &mut seen, r, g, id)?;
    }
    // Gewerke (Paket 1a). Baustoffe älterer Kataloge bleiben ohne, damit
    // ihre Zeilen gleich bleiben; sie bekommen ihr Gewerk beim Übernehmen
    lib.trades = szo::read_trades(recs("trade"))?;
    let mut mat_ids = HashMap::new();
    for r in recs("material") {
        let mut x = szo::read_material(r, &fill_ids, &pen_ids, &surface_ids)?;
        if x.trade
            .is_some_and(|t| !lib.trades.iter().any(|y| y.id() == t))
        {
            x.trade = None;
        }
        let g = x.guid;
        let id = lib.materials.insert(x);
        register(&mut mat_ids, &mut seen, r, g, id)?;
    }
    let (types, set_ids, passed) = szo::read_types(&by, &mat_ids, &mut seen, true)?;
    lib.types = types;
    // Typen unbekannter Art (F-17) bleiben samt Schichten unverändert stehen
    foreign.unknown += passed.guids.len();
    alien.extend(&passed.lines);
    let mut codes: HashMap<&str, usize> = HashMap::new();
    for r in recs("layerset") {
        if passed.lines.contains(&r.line) {
            continue;
        }
        let code = r.get("code")?;
        if let Some(first) = codes.insert(code, r.line) {
            return Err(err(
                r.line,
                format!("Kurzzeichen {code} doppelt (schon in Zeile {first})"),
            ));
        }
    }
    if let Some(t) = lib
        .types
        .iter()
        .map(|(_, t)| t)
        .find(|t| !t.problems().is_empty())
    {
        let line = recs("layerset")
            .iter()
            .find(|r| r.guid("guid").is_ok_and(|g| g == t.guid))
            .map_or(0, |r| r.line);
        return Err(err(line, t.problems().join(", ")));
    }
    for r in recs("default") {
        // Standard einer unbekannten oder waagerechten Art: bleibt stehen
        let cat = r.get("cat")?;
        let wall = TypeCategory::WALLS.iter().any(|&c| type_category(c) == cat);
        if !wall {
            r.skip();
            alien.push(r.line);
            foreign.unknown += 1;
            continue;
        }
        let cat = keyword(r, "cat", &TypeCategory::ALL, type_category)?;
        let g = r.guid("set")?;
        let id = set_ids
            .get(&g)
            .copied()
            .filter(|id| lib.types.get(*id).is_some_and(|t| t.category == cat))
            .ok_or_else(|| err(r.line, "[default]: „set“ ist kein Typ dieser Art"))?;
        match cat {
            TypeCategory::ExteriorWall => lib.default_exterior = Some(id),
            _ => lib.default_interior = Some(id),
        }
    }
    for r in recs("stock") {
        lib.stock.push(r.guid("set")?);
    }
    lib.stock.sort();
    lib.stock.dedup();
    // Unbekannte Schlüssel und Werte: zählen, und die Zeile merken, damit
    // sie unverändert zurückgeschrieben wird. Zugeordnet wird über die
    // Kennung des Satzes ([`record_key`]), auch bei Sätzen ohne Guid.
    let mut odd = Vec::new();
    let mut count = HashMap::new();
    for r in by.values().flatten() {
        let n = r.unknown();
        let key = record_key(r, &mut count);
        if n > 0 {
            foreign.unknown += n;
            odd.push((key, lines[r.line - 1]));
        }
    }
    if !odd.is_empty() {
        let mine = write_known(&lib);
        let mut count = HashMap::new();
        let mut own = HashMap::new();
        for (i, l) in mine.lines().enumerate() {
            if let Ok(Some(r)) = Record::parse(i + 1, l) {
                own.insert(record_key(&r, &mut count), l);
            }
        }
        for (key, theirs) in odd {
            if let Some(l) = own.get(&key) {
                foreign.lines.push((l.to_string(), theirs.to_string()));
            }
        }
    }
    alien.sort_unstable();
    foreign.records = alien.iter().map(|&n| lines[n - 1].to_string()).collect();
    lib.foreign = foreign;
    Ok(lib)
}

// --- Abgleich -------------------------------------------------------------

/// Eintrag mit der Guid `g`.
fn find<T>(a: &Arena<T>, g: Guid, guid: impl Fn(&T) -> Guid) -> Option<Id<T>> {
    a.iter().find(|(_, x)| guid(x) == g).map(|(id, _)| id)
}

/// Stand eines Typs im Projekt gegenüber dem Firmenkatalog.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TypeState {
    OnlyProject,
    OnlyCompany,
    Same,
    Differs,
}

/// Gleicher Inhalt über zwei Tabellen hinweg (Baustoffe über ihre Guid);
/// der Änderungsstand zählt nicht mit.
fn same_across(
    a: &LayerSet,
    a_mat: impl Fn(MaterialId) -> Option<Guid>,
    b: &LayerSet,
    b_mat: impl Fn(MaterialId) -> Option<Guid>,
) -> bool {
    let shape = |l: &crate::library::MaterialLayer| (l.thickness, l.function, l.core);
    let mut x = a.clone();
    x.layers.clone_from(&b.layers);
    x.bearing = b.bearing;
    let bearing = match (a.bearing, b.bearing) {
        (Bearing::Core, Bearing::Core) => true,
        (Bearing::Depth { depth: d, strip: s }, Bearing::Depth { depth: e, strip: t }) => {
            d == e && a_mat(s).is_some() && a_mat(s) == b_mat(t)
        }
        _ => false,
    };
    bearing
        && same_type(&x, b)
        && a.layers.len() == b.layers.len()
        && a.layers.iter().zip(&b.layers).all(|(p, q)| {
            shape(p) == shape(q)
                && a_mat(p.material).is_some()
                && a_mat(p.material) == b_mat(q.material)
        })
}

/// Abgleich aller Typen von Projekt und Firmenkatalog, nach Guid sortiert.
pub fn compare(m: &Model, lib: &Library) -> Vec<(Guid, TypeState)> {
    let mut out = Vec::new();
    let m_mat = |id| m.material(id).map(|x| x.guid);
    for (_, t) in m.layer_sets().iter() {
        let state = match lib.type_by_guid(t.guid).and_then(|id| lib.types.get(id)) {
            None => TypeState::OnlyProject,
            Some(c) if same_across(t, m_mat, c, |id| lib.mat_guid(id)) => TypeState::Same,
            Some(_) => TypeState::Differs,
        };
        out.push((t.guid, state));
    }
    for (_, c) in lib.types.iter() {
        if m.type_by_guid(c.guid).is_none() {
            out.push((c.guid, TypeState::OnlyCompany));
        }
    }
    out.sort_by_key(|x| x.0);
    out
}

/// Holt einen Typ aus dem Firmenkatalog ins Projekt (der Aufrufer öffnet
/// den Schritt „Typ übernommen“). Neu: mit derselben Guid. Abweichend:
/// überschreibt den Projekttyp, alle Wände dieses Typs ändern sich.
/// Fehlende Baustoffe und Darstellung kommen mit; belegt ein anderer Typ das
/// Kurzzeichen, gilt das nächste freie. `None`: nichts geändert.
pub fn import_type(m: &mut Model, lib: &Library, g: Guid) -> Option<LayerSetId> {
    let t = lib.types.get(lib.type_by_guid(g)?)?.clone();
    let material = |m: &mut Model, id: MaterialId| -> Option<MaterialId> {
        let x = lib.materials.get(id)?;
        let found = m.materials().iter().find(|(_, y)| y.guid == x.guid);
        Some(match found.map(|(id, _)| id) {
            Some(id) => id,
            None => {
                let d = import_display(m, lib, x)?;
                m.add_material(Material {
                    cut_fill: d.cut_fill,
                    cut_fg: d.cut_fg,
                    cut_bg: d.cut_bg,
                    surface: d.surface,
                    // Kataloge vor Paket 1a: Gewerk wie bei alten Projekten
                    trade: x.trade.or_else(|| trade::for_material(&x.name, x.category)),
                    ..x.clone()
                })
            }
        })
    };
    let mut layers = Vec::with_capacity(t.layers.len());
    for l in &t.layers {
        let id = material(m, l.material)?;
        layers.push(l.with_material(id));
    }
    // Gewerke, die Baustoffe und Schichten nennen, kommen mit
    for id in szo::used_trades(
        t.layers
            .iter()
            .filter_map(|l| lib.materials.get(l.material)),
        std::iter::once(&t),
    ) {
        if let Some(x) = lib.trades.iter().find(|x| x.id() == id) {
            m.ensure_trade(x);
        }
    }
    let bearing = match t.bearing {
        Bearing::Core => Bearing::Core,
        Bearing::Depth { depth, strip } => Bearing::Depth {
            depth,
            strip: material(m, strip)?,
        },
    };
    let existing = m.type_by_guid(g);
    let code = free_code(&t.code, |c| {
        m.type_by_code(c)
            .is_some_and(|other| Some(other) != existing)
    });
    let new = LayerSet {
        layers,
        code,
        bearing,
        ..t
    };
    match existing {
        // Ein ungültiges Auflager kommt mit und wird wie „ganze tragende
        // Schicht“ gebaut (wie aus einer .szo); der Katalog meldet es
        Some(id) => m.adopt_set_layer_set(id, new).then_some(id),
        None => m.adopt_layer_set(new),
    }
}

/// Darstellung eines Katalog-Baustoffs im Projekt: vorhandene über die Guid,
/// fehlende neu (ein Stift mit der nächsten freien Nummer).
fn import_display(m: &mut Model, lib: &Library, x: &Material) -> Option<MaterialDisplay> {
    let pen = |m: &mut Model, id: Id<Pen>| {
        let p = lib.pens.get(id)?;
        let found = m.attr().pens().iter().find(|(_, q)| q.guid == p.guid);
        match found.map(|(id, _)| id) {
            Some(id) => Some(id),
            None => {
                let number = m.next_pen_number();
                Some(m.add_pen(Pen {
                    number,
                    ..p.clone()
                }))
            }
        }
    };
    let cut_fg = pen(m, x.cut_fg)?;
    let cut_bg = pen(m, x.cut_bg)?;
    let f = lib.fills.get(x.cut_fill)?;
    let found = m.attr().fills().iter().find(|(_, q)| q.guid == f.guid);
    let cut_fill = match found.map(|(id, _)| id) {
        Some(id) => id,
        None => m.add_fill(f.clone()),
    };
    let s = lib.surfaces.get(x.surface)?;
    let found = m.attr().surfaces().iter().find(|(_, q)| q.guid == s.guid);
    let surface = match found.map(|(id, _)| id) {
        Some(id) => id,
        None => m.add_surface(s.clone()),
    };
    Some(MaterialDisplay {
        cut_fill,
        cut_fg,
        cut_bg,
        surface,
    })
}

/// Schreibt einen Projekttyp in den Firmenkatalog (danach [`write_szk`]).
/// Gleiche Guid überschreibt den Typ, nicht aber Baustoffe und Darstellung;
/// was fehlt, kommt dazu. Fehlt dem Katalog ein Standardtyp dieser Art,
/// wird es dieser. `false`: kein solcher Typ im Projekt.
pub fn export_type(m: &Model, lib: &mut Library, g: Guid) -> bool {
    let Some(t) = m.type_by_guid(g).and_then(|id| m.layer_set(id)).cloned() else {
        return false;
    };
    let material = |lib: &mut Library, id: MaterialId| -> Option<MaterialId> {
        let x = m.material(id)?;
        Some(match find(&lib.materials, x.guid, |y| y.guid) {
            Some(id) => id,
            None => {
                let a = m.attr();
                let pen = |lib: &mut Library, id| -> Option<Id<Pen>> {
                    let p = a.pen(id)?;
                    match find(&lib.pens, p.guid, |q| q.guid) {
                        Some(id) => Some(id),
                        None => {
                            let number = lib.next_pen_number();
                            Some(lib.pens.insert(Pen {
                                number,
                                ..p.clone()
                            }))
                        }
                    }
                };
                let (cut_fg, cut_bg) = (pen(lib, x.cut_fg)?, pen(lib, x.cut_bg)?);
                let (f, s) = (a.fill(x.cut_fill)?, a.surface(x.surface)?);
                let cut_fill = find(&lib.fills, f.guid, |q| q.guid)
                    .unwrap_or_else(|| lib.fills.insert(f.clone()));
                let surface = find(&lib.surfaces, s.guid, |q| q.guid)
                    .unwrap_or_else(|| lib.surfaces.insert(s.clone()));
                lib.materials.insert(Material {
                    cut_fill,
                    cut_fg,
                    cut_bg,
                    surface,
                    ..x.clone()
                })
            }
        })
    };
    let mut layers = Vec::with_capacity(t.layers.len());
    for l in &t.layers {
        let Some(id) = material(lib, l.material) else {
            return false;
        };
        layers.push(l.with_material(id));
    }
    for id in szo::used_trades(
        t.layers.iter().filter_map(|l| m.material(l.material)),
        std::iter::once(&t),
    ) {
        if let Some(x) = m.trade(id) {
            if !lib.trades.iter().any(|y| y.guid == x.guid) {
                lib.trades.push(x.clone());
                lib.trades.sort_by_key(|t| (t.order, t.guid));
            }
        }
    }
    let bearing = match t.bearing {
        Bearing::Core => Bearing::Core,
        Bearing::Depth { depth, strip } => match material(lib, strip) {
            Some(strip) => Bearing::Depth { depth, strip },
            None => return false,
        },
    };
    let existing = lib.type_by_guid(g);
    let code = free_code(&t.code, |c| {
        lib.types
            .iter()
            .any(|(id, o)| o.code == c && Some(id) != existing)
    });
    let new = LayerSet {
        layers,
        code,
        bearing,
        ..t
    };
    let cat = new.category;
    let id = match existing.and_then(|id| lib.types.get_mut(id).map(|o| (id, o))) {
        Some((id, old)) => {
            *old = new;
            id
        }
        None => lib.types.insert(new),
    };
    let slot = match cat {
        TypeCategory::ExteriorWall => &mut lib.default_exterior,
        TypeCategory::InteriorWall => &mut lib.default_interior,
        _ => return true,
    };
    if slot.is_none() {
        *slot = Some(id);
    }
    true
}

impl Model {
    /// Neues Projekt mit allen Typen und Standardtypen des Firmenkatalogs.
    /// Werkstypen, die der Katalog nicht führt, weichen, wenn er Typen ihrer
    /// Art hat. Ohne Rückgängig; die Revision beginnt bei 0.
    pub fn from_library(lib: &Library) -> Model {
        let mut m = Model::new();
        for cat in TypeCategory::WALLS {
            if lib.default_type(cat).is_none() {
                continue;
            }
            let gone: Vec<LayerSetId> = m
                .layer_sets()
                .iter()
                .filter(|(_, t)| t.category == cat && lib.type_by_guid(t.guid).is_none())
                .map(|(id, _)| id)
                .collect();
            for id in gone {
                m.forget_type(id);
            }
        }
        let mut order: Vec<Guid> = lib.types.iter().map(|(_, t)| t.guid).collect();
        order.sort();
        for g in order {
            import_type(&mut m, lib, g);
        }
        for cat in TypeCategory::WALLS {
            let set = lib
                .default_type(cat)
                .and_then(|id| lib.types.get(id))
                .and_then(|t| m.type_by_guid(t.guid));
            if let Some(id) = set {
                m.set_default_type(cat, id);
            }
        }
        m.restore_revision(0);
        m
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::library::{LayerFunction, MatCategory, MaterialLayer};
    use crate::txn::Direction;

    /// Jede Baustoffkategorie, Typart und Schichtaufgabe übersteht Schreiben
    /// und Lesen (ein K3-Programm las „cat=air“ aus K4 nicht).
    #[test]
    fn alle_kategorien_im_rundlauf() {
        let mut lib = Library::standard();
        let vorlage = lib.materials.iter().next().unwrap().1.clone();
        let mut mats = Vec::new();
        for (i, c) in MatCategory::ALL.into_iter().enumerate() {
            mats.push(lib.materials.insert(Material {
                guid: Guid(0x5a00 + i as u128),
                name: format!("Probe {}", c.name()),
                category: c,
                lambda: Some(0.5),
                ..vorlage.clone()
            }));
        }
        let [mw, sb, dae, putz, holz, luft] = mats[..] else {
            unreachable!()
        };
        let lage = |material, thickness, function, core| {
            let l = MaterialLayer::new(material, thickness, function);
            if core {
                l.core()
            } else {
                l
            }
        };
        let vorlage = lib.types.iter().next().unwrap().1.clone();
        for (i, cat) in TypeCategory::ALL.into_iter().enumerate() {
            lib.types.insert(LayerSet {
                guid: Guid(0x5b00 + i as u128),
                name: format!("Probe {}", cat.name()),
                code: format!("{}-P", cat.prefix()),
                category: cat,
                layers: vec![
                    lage(putz, 15.0, LayerFunction::Finish, false),
                    lage(luft, 40.0, LayerFunction::AirGap, false),
                    lage(dae, 2.0, LayerFunction::Membrane, false),
                    lage(dae, 100.0, LayerFunction::Insulation, false),
                    // waagerechte Typen: genau eine Kernschicht (Regel 38)
                    lage(mw, 175.0, LayerFunction::Structure, cat.is_wall()),
                    lage(sb, 200.0, LayerFunction::Structure, true),
                    lage(holz, 20.0, LayerFunction::Finish, false),
                ],
                bearing: if cat.is_wall() {
                    Bearing::Depth {
                        depth: 300.0,
                        strip: dae,
                    }
                } else {
                    Bearing::Core
                },
                ..vorlage.clone()
            });
        }
        let text = write_szk(&lib);
        for w in ["air", "concrete", "plaster", "timber", "membrane", "airgap"] {
            assert!(text.contains(&format!("={w} ")), "{w}");
        }
        let back = read_szk(&text).unwrap();
        assert_eq!(back, lib);
        assert_eq!(write_szk(&back), text);
    }

    /// Paket 1a: Der Katalog schreibt die verwendeten Gewerke und liest sie
    /// wieder; ein eigenes Gewerk an einer Schicht kommt beim Übernehmen
    /// mit ins Projekt.
    #[test]
    fn gewerke_im_katalog() {
        let mut lib = Library::standard();
        let text = write_szk(&lib);
        let codes: Vec<&str> = text
            .lines()
            .filter(|l| l.starts_with("[trade] "))
            .map(|l| {
                l.split("code=\"")
                    .nth(1)
                    .unwrap()
                    .split('"')
                    .next()
                    .unwrap()
            })
            .collect();
        assert_eq!(codes, ["18330", "18345"], "nach Reihe, nur verwendete");
        assert_eq!(read_szk(&text).unwrap(), lib);

        let eigen = Trade {
            guid: Guid(0x1a7e),
            code: "F-1".into(),
            name: "Eigenes Gewerk".into(),
            order: 90,
        };
        lib.trades.push(eigen.clone());
        let t = lib.types.ids().next().unwrap();
        let mut typ = lib.types.get(t).unwrap().clone();
        typ.guid = Guid(0x1a7f);
        typ.code = "AW-F".into();
        typ.layers[0].trade = Some(eigen.id());
        typ.layers[0].kg = Some(336);
        lib.types.insert(typ);
        let text = write_szk(&lib);
        assert!(text.contains("code=\"F-1\""));
        let back = read_szk(&text).unwrap();
        assert_eq!(back, lib);

        let mut m = Model::with_seed(1);
        let id = import_type(&mut m, &back, Guid(0x1a7f)).unwrap();
        assert_eq!(m.trade(eigen.id()), Some(&eigen));
        assert_eq!(m.layer_set(id).unwrap().layers[0].kg, Some(336));
        assert!(m.check().is_empty(), "{:?}", m.check());
    }

    /// F-17 (R4): Ein Typ unbekannter Art samt Schichten und Standard bleibt
    /// beim Lesen außen vor und steht beim Schreiben unverändert wieder da.
    #[test]
    fn typ_unbekannter_art_bleibt_stehen() {
        let text = write_szk(&Library::standard());
        let typ = text.lines().find(|l| l.starts_with("[layerset] ")).unwrap();
        let g = Record::parse(1, typ)
            .unwrap()
            .unwrap()
            .get("guid")
            .unwrap()
            .to_string();
        let neu = Guid(0x7e57).to_ifc();
        let mut fremd: Vec<String> = text
            .lines()
            .filter(|l| l.contains(&g))
            .map(|l| {
                l.replace(&g, &neu)
                    .replace(" cat=exterior ", " cat=zukunft ")
                    .replace(" cat=interior ", " cat=zukunft ")
            })
            .collect();
        fremd.push(format!("[default] cat=zukunft set={neu}"));
        assert!(fremd.len() >= 3, "{fremd:?}");
        let alt = format!("{text}{}\n", fremd.join("\n"));
        let lib = read_szk(&alt).expect("öffnet");
        assert_eq!(lib.types.len(), Library::standard().types.len());
        assert!(lib.foreign.unknown >= 2);
        let out = write_szk(&lib);
        for l in &fremd {
            assert!(out.lines().any(|o| o == l), "{l} fehlt in\n{out}");
        }
        assert_eq!(write_szk(&read_szk(&out).unwrap()), out);
    }

    /// Zwei gleiche Schichten eines Typs mit verschiedenen fremden Angaben:
    /// jede kommt an ihrer Stelle zurück.
    #[test]
    fn gleiche_schichten_behalten_ihre_fremden_angaben() {
        let text = include_str!("../../../app/src/firmenkatalog_k4.szk");
        let i = text
            .lines()
            .position(|l| l.starts_with("[layer] "))
            .unwrap();
        let mut lines: Vec<String> = text.lines().map(String::from).collect();
        let layer = lines[i].clone();
        lines[i] = format!("{layer} neu=a");
        lines.insert(i + 1, format!("{layer} neu=b"));
        let alt = lines.join("\n") + "\n";
        let lib = read_szk(&alt).unwrap();
        assert_eq!(lib.foreign.unknown, 2);
        let out = write_szk(&lib);
        let a = out.lines().position(|l| l.ends_with(" neu=a"));
        let b = out.lines().position(|l| l.ends_with(" neu=b"));
        assert!(a.is_some() && b == a.map(|a| a + 1), "{out}");
    }

    /// Jörns Firmenkatalog nach K4 (Startbestand aus K3, beim Start mit K4
    /// um die Werkstypen ergänzt; Zeile 13 ist „Luft“ mit cat=air) liest
    /// sich vollständig. Ein Wort aus einer neueren Fassung wird übersprungen.
    #[test]
    fn katalog_aus_k4_mit_luft() {
        let text = include_str!("../../../app/src/firmenkatalog_k4.szk");
        assert!(text.lines().nth(12).unwrap().contains("cat=air"));
        let lib = read_szk(text).unwrap();
        let luft = lib
            .materials
            .iter()
            .find(|(_, m)| m.name == "Luft")
            .unwrap()
            .1;
        assert_eq!(luft.category, MatCategory::Air);
        assert_eq!(lib.types.len(), 6);
        assert_eq!(lib.stock.len(), 6);
        // Ein unbekanntes Wort aus einer neueren Fassung: Ersatzkategorie,
        // gezählt, und die Zeile bleibt beim Zurückschreiben erhalten.
        let alt = text.replace("cat=air", "cat=gas");
        let lib = read_szk(&alt).unwrap();
        assert_eq!(lib.foreign.unknown, 1);
        assert_eq!(lib.types.len(), 6);
        let gas = alt.lines().nth(12).unwrap();
        assert!(write_szk(&lib).lines().any(|l| l == gas));
    }

    #[test]
    fn startbestand_im_rundlauf() {
        let lib = Library::standard();
        let text = write_szk(&lib);
        assert!(text.starts_with("SZK 1\n"));
        assert!(text.contains("code=\"AW-31,5\"") && text.contains("code=\"IW-17,5\""));
        assert!(text.contains("[default] cat=exterior"));
        assert!(!text.contains("[storey]") && !text.contains("[project]"));
        assert_eq!(read_szk(&text).unwrap(), lib);
        assert_eq!(lib.types.len(), 7);
        assert_eq!(lib.stock.len(), 7, "alle Werkstypen angeboten");
        // Nur die Baustoffe der Typen und deren Darstellung (ohne Putz und
        // Stahlbeton)
        assert_eq!(lib.materials.len(), 6);
        assert!(!text.contains("name=\"Putz\""));
    }

    #[test]
    fn neues_projekt_aus_dem_startbestand_gleicht_model_new() {
        let m = Model::from_library(&Library::standard());
        let n = Model::new();
        let typen = |m: &Model| {
            let mut v: Vec<LayerSet> = m.layer_sets().iter().map(|(_, t)| t.clone()).collect();
            v.sort_by_key(|t| t.guid);
            v
        };
        assert_eq!(typen(&m), typen(&n));
        assert_eq!(m.materials().len(), n.materials().len());
        assert_eq!(m.attr().pens().len(), n.attr().pens().len());
        assert_eq!(m.revision(), 0);
        assert!(compare(&m, &Library::standard())
            .iter()
            .all(|x| x.1 == TypeState::Same));
    }

    #[test]
    fn typ_mit_neuem_baustoff_hin_und_zurueck() {
        // Firma: Kalksandstein mit eigenem Stift in einem neuen Typ
        // Gleicher Startwert wie das Projekt: Stifte und Baustoffe sind dieselben
        let mut f = Model::with_seed(4);
        let gas = f.layer_set(f.defaults().interior_wall).unwrap().layers[0].material;
        let mut ks = f.material(gas).unwrap().clone();
        let pen_guid = f.new_guid();
        let number = f.next_pen_number();
        let pen = f.add_pen(Pen {
            guid: pen_guid,
            number,
            name: "KS".into(),
            color: [1, 2, 3],
            width_mm: 0.25,
        });
        (ks.guid, ks.name, ks.cut_fg) = (f.new_guid(), "Kalksandstein".into(), pen);
        let ks = f.add_material(ks);
        let mut t = f.layer_set(f.defaults().interior_wall).unwrap().clone();
        (t.guid, t.code, t.name) = (f.new_guid(), "IW-24-KS".into(), "IW 24 KS".into());
        t.layers[0].material = ks;
        t.layers[0].thickness = 240.0;
        let g = t.guid;
        f.add_layer_set(t).unwrap();
        let mut lib = Library::standard();
        let max = lib.pens.iter().map(|(_, p)| p.number).max().unwrap();
        assert!(export_type(&f, &mut lib, g));
        assert!(!export_type(&f, &mut lib, Guid(1)));
        let p = lib.pens.iter().find(|(_, p)| p.guid == pen_guid).unwrap().1;
        assert_eq!(p.number, max + 1, "nächste freie Nummer im Katalog");
        let lib = read_szk(&write_szk(&lib)).unwrap();
        // Projekt holt ihn
        let mut m = Model::with_seed(4);
        m.require_steps();
        assert_eq!(
            compare(&m, &lib).iter().find(|x| x.0 == g).map(|x| x.1),
            Some(TypeState::OnlyCompany)
        );
        let (pens, mats) = (m.attr().pens().len(), m.materials().len());
        m.begin("Typ übernommen");
        let id = import_type(&mut m, &lib, g).unwrap();
        let tx = m.commit().unwrap();
        assert_eq!(m.layer_set(id).unwrap().guid, g);
        assert_eq!(
            (m.attr().pens().len(), m.materials().len()),
            (pens + 1, mats + 1)
        );
        assert!(m.check().is_empty(), "{:?}", m.check());
        assert_eq!(
            compare(&m, &lib).iter().find(|x| x.0 == g).map(|x| x.1),
            Some(TypeState::Same)
        );
        m.apply(&tx, Direction::Undo);
        assert!(m.type_by_guid(g).is_none());
        assert_eq!((m.attr().pens().len(), m.materials().len()), (pens, mats));
    }

    #[test]
    fn kaputter_katalog_nennt_die_zeile() {
        let e = read_szk("SZK 1\n[layerset] guid=x name=\"A\"\n").unwrap_err();
        assert_eq!(e.line, 2);
        let mut t = write_szk(&Library::standard());
        t = t.replacen("code=\"IW-17,5\"", "code=\"AW-31,5\"", 1);
        let e = read_szk(&t).unwrap_err();
        assert!(e.message.contains("doppelt"), "{e}");
        assert!(read_szk("SZK 2\n").is_err());
        assert!(read_szk("SZO 4\n").is_err());
    }
}
