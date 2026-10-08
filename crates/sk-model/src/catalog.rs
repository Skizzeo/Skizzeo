//! Firmenkatalog `.szk` (K2): Bauteiltypen mit ihren Baustoffen und deren
//! Darstellung außerhalb des Projekts, als Bürostandard. Gleiches
//! Zeilenformat wie die `.szo`, ohne Geschosse, Gebäude und Bauteile.
//!
//! Abgleich mit dem Projektkatalog nur über die Guid: Übernehmen holt einen
//! Typ ins Projekt, Zurückspeichern schreibt ihn in den Katalog. Was im Ziel
//! fehlt, kommt dazu; Baustoffe und Darstellung, die es dort schon gibt,
//! bleiben, wie sie sind (Regel 8). Datei lesen und schreiben macht die App.

use crate::attr::{Fill, Pen, Surface};
use crate::guid::{Guid, GuidGen};
use crate::id::{Arena, Id};
use crate::library::{
    Bearing, LayerSet, LayerSetId, MatCategory, Material, MaterialDisplay, MaterialId, TypeCategory,
};
use crate::model::{free_code, same_type, Model, ETICS_TYPE_GUID, EXTERIOR_TYPE_GUID};
use crate::proctex::{CompanyPreset, Pattern};
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
    /// Firmenvorlagen für Muster (Paket 7 §2.2, Regel 65), in der
    /// Reihenfolge der Datei bzw. des Speicherns.
    pub presets: Vec<CompanyPreset>,
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
/// Typ und Merkmal, [layer]: Typ, [matprop]/[prop]: Baustoff bzw.
/// Bauteil und Merkmal). Mehrere Sätze mit derselben Kennung
/// (Schichten eines Typs) zählen in Dateireihenfolge durch.
pub(crate) fn record_key(r: &Record, count: &mut HashMap<String, usize>) -> String {
    // Muster (Paket 6) gehören über `surface=` zu ihrer Oberfläche
    let id = r
        .opt("guid")
        .map(|g| ("guid", g))
        .or_else(|| r.opt("surface").map(|g| ("surface", g)));
    let base = match id {
        Some((k, g)) => format!("{} {k}={g}", r.section),
        None => format!(
            "{} set={} cat={} mat={} elem={} key={}",
            r.section,
            r.opt("set").unwrap_or(""),
            r.opt("cat").unwrap_or(""),
            r.opt("mat").unwrap_or(""),
            r.opt("elem").unwrap_or(""),
            r.opt("key").unwrap_or("")
        ),
    };
    let n = count.entry(base.clone()).or_insert(0);
    *n += 1;
    format!("{base} #{n}")
}

/// Der Katalog als `.szk`-Text.
pub fn write_szk(lib: &Library) -> String {
    with_foreign(write_known(lib), &lib.foreign)
}

/// Text `plain`, wie dieser Schreiber ihn kennt, mit dem Fremden aus der
/// gelesenen Datei: Zeilen mit unbekannten Angaben, solange ihr Eintrag
/// unverändert ist, und Sätze unbekannter Art am Ende (F-17, F-17b).
pub(crate) fn with_foreign(plain: String, f: &Foreign) -> String {
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
    // Baustoffkennwerte (Paket 5 §2.3) nur, wenn gesetzt
    for x in sorted(lib.materials.iter(), |x| x.guid) {
        crate::matprop::write_lines(&mut out, x.guid, &x.props);
    }
    // Muster (Paket 6 §2.2), nur an Oberflächen mit Muster
    for x in sorted(lib.surfaces.iter(), |x| x.guid) {
        if let Some(p) = &x.pattern {
            crate::proctex::write_line(&mut out, x.guid, Some(p));
        }
    }
    // Firmenvorlagen (Paket 7 §2.2): gleiche Schlüssel wie `[pattern]`
    for v in &lib.presets {
        preset_line(v).finish(&mut out);
    }
    out
}

fn preset_line(v: &CompanyPreset) -> Line {
    let l = Line::new("patternpreset")
        .guid("guid", Some(v.guid))
        .text("name", &v.name)
        .color("base", v.base);
    crate::proctex::write_keys(l, &v.pattern)
}

/// Liest eine Zeile `[patternpreset]`; `Err` = verworfen (Grund).
fn read_preset(r: &Record, raw: &str) -> Result<CompanyPreset, String> {
    let guid = r.guid("guid").map_err(|e| e.message)?;
    let name = r.get("name").map_err(|e| e.message)?.to_string();
    if name.trim().is_empty() {
        return Err("Name leer".into());
    }
    if crate::proctex::preset_named(&name).is_some() {
        return Err(format!("„{name}“ ist eine Werksvorlage"));
    }
    let base = r.color("base").map_err(|e| e.message)?;
    match crate::proctex::read_line(r, raw)? {
        Some(
            p @ (Pattern::Masonry { .. }
            | Pattern::Plaster { .. }
            | Pattern::Concrete { .. }
            | Pattern::Timber { .. }
            | Pattern::Tiles { .. }
            | Pattern::Stone { .. }),
        ) => Ok(CompanyPreset {
            guid,
            name,
            pattern: p,
            base,
        }),
        Some(Pattern::Foreign(_)) => Err("Muster einer neueren Fassung".into()),
        None => Err("ohne Muster".into()),
    }
}

/// Guid und Name der Firmenvorlagen, die beim Lesen roh stehen blieben
/// (neuere Fassung, unbrauchbar): Ein neuer Name darf auch sie nicht
/// treffen, sonst überspringt das nächste Lesen eine der beiden (Regel 65).
fn raw_presets(lib: &Library) -> Vec<(Option<Guid>, String)> {
    lib.foreign
        .records
        .iter()
        .filter_map(|l| Record::parse(0, l).ok().flatten())
        .filter(|r| r.section == "patternpreset")
        .filter_map(|r| Some((r.guid("guid").ok(), r.opt("name")?.trim().to_string())))
        .collect()
}

/// „Als Vorlage speichern …“ (Paket 7 §2.2): legt eine Firmenvorlage mit
/// neuer Guid an. Abgelehnt werden ein leerer Name, der Name einer
/// Werksvorlage oder einer vorhandenen Firmenvorlage und ungültige Werte.
pub fn save_preset(
    lib: &mut Library,
    name: &str,
    pattern: &Pattern,
    base: [u8; 3],
) -> Result<(), String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("Bitte einen Namen eingeben.".into());
    }
    if crate::proctex::preset_named(name).is_some() {
        return Err(format!("„{name}“ ist schon eine Werksvorlage."));
    }
    let raw = raw_presets(lib);
    if lib.presets.iter().any(|v| v.name == name) || raw.iter().any(|(_, n)| n == name) {
        return Err(format!("„{name}“ gibt es schon."));
    }
    if matches!(pattern, Pattern::Foreign(_)) {
        return Err("Dieses Muster kennt diese Fassung nicht.".into());
    }
    crate::proctex::validate(pattern)?;
    let mut gen = GuidGen::from_time();
    let mut guid = gen.next_guid();
    while lib.presets.iter().any(|v| v.guid == guid) || raw.iter().any(|(g, _)| *g == Some(guid)) {
        guid = gen.next_guid();
    }
    lib.presets.push(CompanyPreset {
        guid,
        name: name.to_string(),
        pattern: pattern.clone(),
        base,
    });
    Ok(())
}

/// Liest einen Firmenkatalog. Bei einem Fehler wird nichts übernommen; der
/// Fehler nennt die Zeile.
pub fn read_szk(text: &str) -> Result<Library, LoadError> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    check_header(text.lines().next(), "SZK", VERSION)?;
    let mut by: HashMap<&str, Vec<Record>> = HashMap::new();
    const KNOWN: [&str; 14] = [
        "pen",
        "linetype",
        "fill",
        "surface",
        "trade",
        "material",
        "layerset",
        "layer",
        "typeprop",
        "default",
        "stock",
        "matprop",
        "pattern",
        "patternpreset",
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
    // Muster (Paket 6); zweite Zeilen bleiben unverändert stehen
    let mut kept = Vec::new();
    szo::read_patterns(
        recs("pattern"),
        &lines,
        &surface_ids,
        &mut lib.surfaces,
        &mut hints,
        &mut kept,
    );
    foreign.unknown += kept.len();
    alien.extend(kept);
    // Gewerke (Paket 1a). Baustoffe älterer Kataloge bleiben ohne, damit
    // ihre Zeilen gleich bleiben; sie bekommen ihr Gewerk beim Übernehmen
    lib.trades = szo::read_trades(recs("trade"))?;
    trade::dedup_shorts(&mut lib.trades);
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
    szo::read_matprops(recs("matprop"), &mat_ids, &mut lib.materials, &mut hints);
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
    // Firmenvorlagen (Paket 7): Unbrauchbare bleiben unverändert stehen
    // (Hinweis), damit eine neuere Fassung sie wiederfindet
    for r in recs("patternpreset") {
        let raw = lines[r.line - 1];
        let dup = |g: Guid, n: &str| lib.presets.iter().any(|v| v.guid == g || v.name == n);
        match read_preset(r, raw) {
            Ok(v) if !dup(v.guid, &v.name) => lib.presets.push(v),
            res => {
                let why = res.map_or_else(|e| e, |v| format!("„{}“ doppelt", v.name));
                hints.push(format!(
                    "Zeile {}: Firmenvorlage übersprungen ({why})",
                    r.line
                ));
                r.skip();
                alien.push(r.line);
                foreign.unknown += 1;
            }
        }
    }
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
                let d = export_display(m, lib, x)?;
                lib.materials.insert(with_display(x, d))
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

/// Baustoff `x` mit der Darstellung `d` (Kennungen einer anderen Tabelle).
fn with_display(x: &Material, d: MaterialDisplay) -> Material {
    Material {
        cut_fill: d.cut_fill,
        cut_fg: d.cut_fg,
        cut_bg: d.cut_bg,
        surface: d.surface,
        ..x.clone()
    }
}

/// Darstellung eines Projekt-Baustoffs im Firmenkatalog: vorhandene über
/// die Guid, fehlende neu (ein Stift mit der nächsten freien Nummer).
fn export_display(m: &Model, lib: &mut Library, x: &Material) -> Option<MaterialDisplay> {
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
    let cut_fill =
        find(&lib.fills, f.guid, |q| q.guid).unwrap_or_else(|| lib.fills.insert(f.clone()));
    let surface =
        find(&lib.surfaces, s.guid, |q| q.guid).unwrap_or_else(|| lib.surfaces.insert(s.clone()));
    Some(MaterialDisplay {
        cut_fill,
        cut_fg,
        cut_bg,
        surface,
    })
}

/// Gleicher Baustoff in Projekt und Firmenkatalog (Paket 5 §1.3): alle
/// Angaben samt Kennwerten, die Darstellung über die Guids.
fn same_material(m: &Model, x: &Material, lib: &Library, y: &Material) -> bool {
    let a = m.attr();
    let mine = (
        a.fill(x.cut_fill).map(|f| f.guid),
        a.pen(x.cut_fg).map(|p| p.guid),
        a.pen(x.cut_bg).map(|p| p.guid),
        a.surface(x.surface).map(|s| s.guid),
    );
    let theirs = (
        lib.fills.get(y.cut_fill).map(|f| f.guid),
        lib.pens.get(y.cut_fg).map(|p| p.guid),
        lib.pens.get(y.cut_bg).map(|p| p.guid),
        lib.surfaces.get(y.surface).map(|s| s.guid),
    );
    mine == theirs && with_display(x, y.display()) == *y
}

/// Abgleich der Oberflächen von Projekt und Firmenkatalog nach Guid,
/// sortiert (Paket 6 §2.2): gleich, wenn Name, Farben und Muster gleich sind.
pub fn compare_surfaces(m: &Model, lib: &Library) -> Vec<(Guid, TypeState)> {
    let mut out = Vec::new();
    for (_, x) in m.attr().surfaces().iter() {
        let state =
            match find(&lib.surfaces, x.guid, |y| y.guid).and_then(|id| lib.surfaces.get(id)) {
                None => TypeState::OnlyProject,
                Some(y) if y == x => TypeState::Same,
                Some(_) => TypeState::Differs,
            };
        out.push((x.guid, state));
    }
    for (_, y) in lib.surfaces.iter() {
        if !m.attr().surfaces().iter().any(|(_, x)| x.guid == y.guid) {
            out.push((y.guid, TypeState::OnlyCompany));
        }
    }
    out.sort_by_key(|x| x.0);
    out
}

/// Schreibt eine Projekt-Oberfläche samt Muster in den Firmenkatalog; gleiche
/// Guid überschreibt sie. `false`: keine solche Oberfläche.
pub fn export_surface(m: &Model, lib: &mut Library, id: crate::SurfaceId) -> bool {
    let Some(s) = m.attr().surface(id) else {
        return false;
    };
    match find(&lib.surfaces, s.guid, |y| y.guid) {
        Some(at) => lib.surfaces.set(at, Some(s.clone())),
        None => {
            lib.surfaces.insert(s.clone());
        }
    }
    true
}

/// Abgleich der Baustoffe von Projekt und Firmenkatalog nach Guid, sortiert
/// (Paket 5 §1.3). Luft fehlt: sie wird weder übernommen noch
/// zurückgespeichert.
pub fn compare_materials(m: &Model, lib: &Library) -> Vec<(Guid, TypeState)> {
    let mut out = Vec::new();
    for (_, x) in m.materials().iter() {
        if x.category == MatCategory::Air {
            continue;
        }
        let state =
            match find(&lib.materials, x.guid, |y| y.guid).and_then(|id| lib.materials.get(id)) {
                None => TypeState::OnlyProject,
                Some(y) if same_material(m, x, lib, y) => TypeState::Same,
                Some(_) => TypeState::Differs,
            };
        out.push((x.guid, state));
    }
    for (_, y) in lib.materials.iter() {
        if y.category != MatCategory::Air && !m.materials().iter().any(|(_, x)| x.guid == y.guid) {
            out.push((y.guid, TypeState::OnlyCompany));
        }
    }
    out.sort_by_key(|x| x.0);
    out
}

/// Schreibt einen Projekt-Baustoff samt Kennwerten in den Firmenkatalog
/// („In den Firmenkatalog …“, danach [`write_szk`]); gleiche Guid
/// überschreibt ihn. Darstellung und Gewerk kommen mit, wenn sie fehlen.
/// `false`: kein solcher Baustoff, oder Luft.
pub fn export_material(m: &Model, lib: &mut Library, id: MaterialId) -> bool {
    m.material(id)
        .is_some_and(|x| x.category != MatCategory::Air)
        && put_in_library(m, lib, id)
}

/// [`export_material`] für jeden Baustoff, auch Luft.
fn put_in_library(m: &Model, lib: &mut Library, id: MaterialId) -> bool {
    let Some(x) = m.material(id) else {
        return false;
    };
    let Some(d) = export_display(m, lib, x) else {
        return false;
    };
    if let Some(t) = x.trade.and_then(|t| m.trade(t)) {
        if !lib.trades.iter().any(|y| y.guid == t.guid) {
            lib.trades.push(t.clone());
            lib.trades.sort_by_key(|t| (t.order, t.guid));
        }
    }
    let new = with_display(x, d);
    match find(&lib.materials, x.guid, |y| y.guid).and_then(|id| lib.materials.get_mut(id)) {
        Some(old) => *old = new,
        None => {
            lib.materials.insert(new);
        }
    }
    true
}

/// Holt einen Baustoff samt Kennwerten aus dem Firmenkatalog ins Projekt
/// („Ins Projekt übernehmen“; der Aufrufer öffnet den Schritt). Gleiche
/// Guid überschreibt den Projekt-Baustoff, Mengen bleiben, Preis und
/// Kennwerte ändern sich. Ist der Name schon vergeben, gilt der nächste
/// freie. `false`: nichts geändert.
pub fn import_material(m: &mut Model, lib: &Library, g: Guid) -> bool {
    let air = find(&lib.materials, g, |y| y.guid)
        .and_then(|id| lib.materials.get(id))
        .is_none_or(|x| x.category == MatCategory::Air);
    !air && adopt_material(m, lib, g, false)
}

/// [`import_material`] für jeden Baustoff, auch Luft. `exact` (OK im
/// Materialfenster): Name und Gewerk genau wie in der Kopie, ohne
/// Ausweichnamen und ohne Gewerk nach Baustoffart (Review 3n/2, 3n/3).
fn adopt_material(m: &mut Model, lib: &Library, g: Guid, exact: bool) -> bool {
    let Some(x) = find(&lib.materials, g, |y| y.guid).and_then(|id| lib.materials.get(id)) else {
        return false;
    };
    let Some(d) = import_display(m, lib, x) else {
        return false;
    };
    if let Some(t) = x
        .trade
        .and_then(|t| lib.trades.iter().find(|y| y.id() == t))
    {
        m.ensure_trade(t);
    }
    let existing = m
        .materials()
        .iter()
        .find(|(_, y)| y.guid == g)
        .map(|(id, _)| id);
    let (name, trade) = if exact {
        (x.name.clone(), x.trade)
    } else {
        (
            m.free_material_name(&x.name, existing),
            x.trade.or_else(|| trade::for_material(&x.name, x.category)),
        )
    };
    let new = Material {
        name,
        trade,
        ..with_display(x, d)
    };
    match existing {
        Some(id) => m.material(id) != Some(&new) && m.set_material(id, new),
        None => {
            m.add_material(new);
            true
        }
    }
}

/// Übernimmt die Baustoffe der Arbeitskopie `work` ins Modell (OK im
/// Materialfenster, Paket 5 §1.3; der Aufrufer öffnet den Schritt): was in
/// `work` fehlt, wird gelöscht, Neues und Geändertes kommt über die Guid
/// samt Darstellung und Gewerk. `true`: etwas hat sich geändert.
pub fn sync_materials(m: &mut Model, work: &Model) -> bool {
    sync_materials_report(m, work, &mut Vec::new())
}

/// Wie [`sync_materials`]; was nicht übernommen werden konnte, steht als
/// Satz in `problems` (Review 3n/4: nicht still verwerfen).
pub fn sync_materials_report(m: &mut Model, work: &Model, problems: &mut Vec<String>) -> bool {
    let mut changed = false;
    let name_of =
        |m: &Model, id: MaterialId| m.material(id).map_or(String::new(), |x| x.name.clone());
    let keep: Vec<Guid> = work.materials().iter().map(|(_, x)| x.guid).collect();
    let gone: Vec<MaterialId> = m
        .materials()
        .iter()
        .filter(|(_, x)| !keep.contains(&x.guid))
        .map(|(id, _)| id)
        .collect();
    for id in gone {
        let name = name_of(m, id);
        if m.remove_material(id) {
            changed = true;
        } else {
            problems.push(format!(
                "„{name}“ konnte nicht gelöscht werden (wird noch verwendet)."
            ));
        }
    }
    // Namen, die in der Kopie getauscht oder weitergereicht wurden: erst
    // auf freie Zwischennamen, damit jeder Baustoff genau seinen neuen
    // Namen bekommt (Review 3n/2)
    let target = |g: Guid| {
        work.materials()
            .iter()
            .find(|(_, x)| x.guid == g)
            .map(|(_, x)| x.name.clone())
    };
    let mut renamed: Vec<(MaterialId, String, String)> = Vec::new();
    let moving: Vec<(MaterialId, Material)> = m
        .materials()
        .iter()
        .filter(|(_, x)| target(x.guid).is_some_and(|n| n != x.name))
        .map(|(id, x)| (id, x.clone()))
        .collect();
    for (id, x) in moving {
        let mut k = 1;
        let tmp = loop {
            let n = format!("{} ~{k}", x.name);
            if !m.materials().iter().any(|(_, y)| y.name == n) {
                break n;
            }
            k += 1;
        };
        // nur der Name, ohne Wertprüfung (Review 3q/2)
        changed |= m.rename_material(id, &tmp);
        let tmp = m.material(id).map_or(tmp, |y| y.name.clone());
        renamed.push((id, x.name.clone(), tmp));
    }
    let mut lib = Library {
        trades: work.trades().to_vec(),
        ..Library::default()
    };
    for (id, x) in work.materials().iter() {
        if !put_in_library(work, &mut lib, id) {
            problems.push(format!("„{}“ konnte nicht übernommen werden.", x.name));
            continue;
        }
        let Some(y) = find(&lib.materials, x.guid, |y| y.guid).and_then(|id| lib.materials.get(id))
        else {
            continue;
        };
        let same = m
            .materials()
            .iter()
            .find(|(_, z)| z.guid == x.guid)
            .is_some_and(|(_, z)| same_material(m, z, &lib, y));
        if !same {
            if adopt_material(m, &lib, x.guid, true) {
                changed = true;
            } else {
                problems.push(format!(
                    "„{}“ wurde nicht übernommen: ein Wert ist ungültig.",
                    x.name
                ));
            }
        }
    }
    // Zwischennamen nie stehen lassen: Was nicht übernommen wurde, bekommt
    // seinen Zielnamen, sonst den alten zurück (Review 3q/2)
    for (id, old, tmp) in renamed {
        let Some(g) = m.material(id).filter(|x| x.name == tmp).map(|x| x.guid) else {
            continue;
        };
        let want = target(g).unwrap_or_else(|| old.clone());
        if !m.rename_material(id, &want) && !m.rename_material(id, &old) {
            problems.push(format!("„{old}“ behält den Zwischennamen „{tmp}“."));
        }
    }
    changed
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

    /// Review 3n/2, 3n/3, 3n/7: OK im Materialfenster tauscht Namen genau,
    /// setzt kein Gewerk nach Baustoffart und ein neues Gewerk geht mit
    /// Rückgängig wieder weg.
    #[test]
    fn baustoffe_uebernehmen_genau() {
        let mut m = Model::with_seed(31);
        m.require_steps();
        let ids: Vec<MaterialId> = m.materials().ids().take(2).collect();
        let (a, b) = (
            m.material(ids[0]).unwrap().clone(),
            m.material(ids[1]).unwrap().clone(),
        );
        let mut work = m.clone();
        let mut x = a.clone();
        x.name = b.name.clone();
        x.trade = None;
        x.density += 1.0;
        let mut y = b.clone();
        y.name = a.name.clone();
        work.begin("Kopie");
        // Zwischenname, damit die Kopie selbst gültig bleibt
        assert!(work.set_material(
            ids[1],
            Material {
                name: "~".into(),
                ..y.clone()
            }
        ));
        assert!(work.set_material(ids[0], x));
        assert!(work.set_material(ids[1], y));
        work.commit();
        m.begin("Baustoffe geändert");
        assert!(sync_materials(&mut m, &work));
        let tx = m.commit().unwrap();
        assert_eq!(m.material(ids[0]).unwrap().name, b.name, "getauscht");
        assert_eq!(m.material(ids[1]).unwrap().name, a.name, "getauscht");
        assert_eq!(
            m.material(ids[0]).unwrap().trade,
            None,
            "kein Gewerk nach Art"
        );
        m.apply(&tx, Direction::Undo);
        assert_eq!(m.material(ids[0]).unwrap(), &a);
        assert_eq!(m.material(ids[1]).unwrap(), &b);
    }

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
        let [mw, sb, dae, putz, holz, luft, _blech] = mats[..] else {
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
                    lage(putz, 20.0, LayerFunction::Finish, false),
                    lage(luft, 40.0, LayerFunction::AirGap, false),
                    lage(dae, 2.0, LayerFunction::Membrane, false),
                    lage(dae, 100.0, LayerFunction::Insulation, false),
                    // waagerechte Typen: genau eine Kernschicht (Regel 38),
                    // die Dachterrasse keine (Regel 39)
                    lage(mw, 175.0, LayerFunction::Structure, cat.is_wall()),
                    lage(
                        sb,
                        200.0,
                        LayerFunction::Structure,
                        cat != TypeCategory::RoofTerrace,
                    ),
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
            short: None,
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

    /// Eine Firmenvorlage einer neueren Fassung bleibt roh stehen; „Als
    /// Vorlage speichern …“ lehnt ihren Namen ab (sonst überspränge das
    /// nächste Lesen eine der beiden) und nimmt einen anderen an.
    #[test]
    fn rohe_vorlage_belegt_ihren_namen() {
        let text = write_szk(&Library::standard());
        let roh = "[patternpreset] guid=0bTdQXV2v4F9fBAYaFw6qr name=\"Zukunft\" base=8a3b2a gen=hologramm tiefe=3";
        let lib = read_szk(&format!("{text}{roh}\n")).expect("öffnet");
        assert!(lib.presets.is_empty());
        let p = crate::proctex::factory(crate::proctex::FACING).unwrap();
        let mut neu = lib.clone();
        assert_eq!(
            save_preset(&mut neu, " Zukunft ", &p, [1, 2, 3]),
            Err("„Zukunft“ gibt es schon.".into())
        );
        assert!(save_preset(&mut neu, "Gegenwart", &p, [1, 2, 3]).is_ok());
        let out = write_szk(&neu);
        assert!(out.lines().any(|l| l == roh), "roh zurück");
        let zurueck = read_szk(&out).unwrap();
        assert_eq!(zurueck.presets.len(), 1);
        assert_eq!(zurueck.presets[0].name, "Gegenwart");
        assert_ne!(
            Some(zurueck.presets[0].guid),
            Guid::from_ifc("0bTdQXV2v4F9fBAYaFw6qr")
        );
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
