//! Ausbau des Raumbuchs in der `.szo` und `.szk` (Raumbuch RB1): die
//! Ausbautypen stehen in `[layerset]` mit ihrem Wort für die Typart
//! ([`crate::TypeCategory::word`]); die Sockelleiste trägt dazu ihre Höhe
//! `h=` in mm. Ohne Sockelleiste bleibt die Datei bytegleich. Die
//! Raumvorlagen (RB2) stehen in `[spacetype]`, in `.szo` und `.szk` gleich.

use super::{Line, LoadError, Record};
use crate::id::Arena;
use crate::library::LayerSet;
use crate::model::{Model, SpaceGroup, SpaceType, WallTiles, TILES_DEFAULT, TILES_HEIGHT, WOFLV};
use crate::{Guid, TypeCategory};

/// `h=` nur, wenn der Typ eine Höhe hat (Sockelleiste).
pub(super) fn write_height(line: Line, s: &LayerSet) -> Line {
    match s.height {
        Some(h) => line.num("h", h),
        None => line,
    }
}

/// Höhe des Profils aus `h=`; ob sie zur Typart passt, prüft
/// [`LayerSet::problems`].
pub(super) fn read_height(r: &Record) -> Result<Option<f64>, LoadError> {
    match r.opt("h") {
        None => Ok(None),
        Some(_) => r.f64("h").map(Some),
    }
}

/// Abschnitte des Raumbuchs in der `.szo`.
const SECTIONS: [&str; 1] = ["spacetype"];

/// Der Abschnitt `s`, wenn ihn `base` (die Liste in [`super::read_with`])
/// oder das Raumbuch kennt.
pub(super) fn known(base: &[&'static str], s: &str) -> Option<&'static str> {
    base.iter().chain(&SECTIONS).find(|k| **k == s).copied()
}

/// Was das Raumbuch aus der `.szo` liest.
pub(crate) struct Daten {
    pub(crate) space_types: Vec<SpaceType>,
}

/// Schreibt die Abschnitte des Raumbuchs (nach den Bauteiltypen).
pub(super) fn write(out: &mut String, m: &Model) {
    write_space_types(out, m.space_types());
}

/// Liest die Abschnitte des Raumbuchs; `types` sind die Bauteiltypen der
/// Datei.
pub(super) fn read<'a>(
    recs: &impl Fn(&str) -> &'a Vec<Record>,
    types: &Arena<LayerSet>,
    hints: &mut Vec<String>,
) -> Result<Daten, LoadError> {
    Ok(Daten {
        space_types: read_space_types(recs("spacetype"), types, hints)?,
    })
}

/// Raumvorlagen `[spacetype]` (Raumbuch RB2) nach Guid; ohne Vorlage keine
/// Zeile. Ausbautypen als Guid, leere Felder ohne Schlüssel.
pub(crate) fn write_space_types(out: &mut String, v: &[SpaceType]) {
    let mut v: Vec<&SpaceType> = v.iter().collect();
    v.sort_by_key(|s| s.guid);
    for s in v {
        let mut l = Line::new("spacetype")
            .guid("guid", Some(s.guid))
            .text("name", &s.name)
            .word("group", s.group.word())
            .num("woflv", s.woflv);
        for (key, g) in [("floor", s.floor), ("screed", s.screed), ("wall", s.wall)] {
            if g.is_some() {
                l = l.guid(key, g);
            }
        }
        if let Some(t) = s.tiles {
            l = l.guid("tiles", Some(t.set)).num("tiles_h", t.height);
        }
        for (key, g) in [("ceiling", s.ceiling), ("skirting", s.skirting)] {
            if g.is_some() {
                l = l.guid(key, g);
            }
        }
        l.finish(out);
    }
}

/// Liest `[spacetype]`; `types` sind die Bauteiltypen derselben Datei. Ein
/// unlesbarer Verweis oder einer auf einen fehlenden Typ oder einen Typ
/// anderer Art entfällt, eine fehlende oder unbekannte Nutzungsgruppe gilt
/// als NUF 7, eine andere Anrechnung als 100 %, eine Fliesenhöhe außerhalb
/// des Bereichs als die Grenze, eine fehlende als 1,20 m, ein doppelter Name
/// bekommt „ 2“; jeweils mit Hinweis, die Datei lädt. Nur eine fehlende oder
/// doppelte Guid und ein fehlender Name brechen das Laden ab.
pub(crate) fn read_space_types(
    recs: &[Record],
    types: &Arena<LayerSet>,
    hints: &mut Vec<String>,
) -> Result<Vec<SpaceType>, LoadError> {
    let known = |g: Guid| {
        types
            .iter()
            .find(|(_, t)| t.guid == g)
            .map(|(_, t)| t.category)
    };
    let mut out: Vec<SpaceType> = Vec::new();
    for r in recs {
        let guid = r.guid("guid")?;
        if out.iter().any(|s| s.guid == guid) {
            return Err(super::err(r.line, "[spacetype]: Guid doppelt"));
        }
        let mut name = r.get("name")?.to_string();
        let who = |hints: &mut Vec<String>, what: String| {
            hints.push(format!("Zeile {}: Raumvorlage {what}", r.line));
        };
        if out.iter().any(|s| s.name == name) {
            let neu = (2..)
                .map(|i| format!("{name} {i}"))
                .find(|n| !out.iter().any(|s| &s.name == n))
                .unwrap_or_default();
            who(hints, format!("„{name}“ doppelt, heißt jetzt „{neu}“"));
            name = neu;
        }
        let group = match r.opt("group").and_then(SpaceGroup::from_word) {
            Some(g) => g,
            None => {
                who(
                    hints,
                    format!("„{name}“: Nutzungsgruppe unbekannt, gilt NUF 7"),
                );
                SpaceGroup::Nuf7
            }
        };
        let woflv = match r.int::<u8>("woflv") {
            Ok(w) if WOFLV.contains(&w) => w,
            _ => {
                who(hints, format!("„{name}“: Wohnfläche gilt zu 100 %"));
                100
            }
        };
        let mut set = |key: &str, cat: TypeCategory| -> Option<Guid> {
            let v = r.opt(key)?;
            let Some(g) = Guid::from_ifc(v) else {
                who(
                    hints,
                    format!("„{name}“: {} „{v}“ unlesbar, entfällt", cat.name()),
                );
                return None;
            };
            if known(g) == Some(cat) {
                return Some(g);
            }
            who(
                hints,
                format!("„{name}“: {} {} fehlt, entfällt", cat.name(), g.to_ifc()),
            );
            None
        };
        let floor = set("floor", TypeCategory::FloorFinish);
        let screed = set("screed", TypeCategory::Screed);
        let wall = set("wall", TypeCategory::WallFinish);
        let tiles = set("tiles", TypeCategory::WallTiles);
        let ceiling = set("ceiling", TypeCategory::CeilingFinish);
        let skirting = set("skirting", TypeCategory::Skirting);
        let tiles = match tiles {
            Some(g) => {
                // Verwerfen mit Hinweis, die Datei lädt (Review #85)
                let (lo, hi) = TILES_HEIGHT;
                let h = match r.opt("tiles_h").and_then(|v| v.parse::<f64>().ok()) {
                    Some(h) if h.is_finite() => {
                        if !(lo..=hi).contains(&h) {
                            who(hints, format!("„{name}“: Fliesenhöhe {h} mm außerhalb"));
                        }
                        h.clamp(lo, hi)
                    }
                    _ => {
                        who(hints, format!("„{name}“: Fliesenhöhe fehlt, gilt 1,20 m"));
                        TILES_DEFAULT
                    }
                };
                Some(WallTiles { set: g, height: h })
            }
            None => {
                let _ = r.opt("tiles_h");
                None
            }
        };
        out.push(SpaceType {
            guid,
            name,
            group,
            woflv,
            floor,
            screed,
            wall,
            tiles,
            ceiling,
            skirting,
        });
    }
    Ok(out)
}
