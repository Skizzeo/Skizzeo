//! Öffnungen in der `.szo` (Paket O2, [`crate::oeffnung`]):
//!
//! `[opening] guid=… art=window pts="x y;x y" bottom=Guid:sill
//! top=Guid:lintel number="OE-001" cat=opening seq=3 storey=…`
//!
//! Eine Höhe ist `Guid:sill` bzw. `Guid:lintel` (an die Höhenlinie des
//! Geschosses gebunden) oder `Guid:u|o:Versatz` (frei). `reveal=l,r,o,u`
//! steht nur, wenn der Anschlag nicht dem Wandtyp folgt, `install=…`
//! (äußere Einbautiefe) und `element=…` (Bautiefe) nur, wenn gesetzt.

use super::{category, err, keyword, Line, LoadError, Record};
use crate::element::{Element, ElementKind, LevelEdge, LevelRef};
use crate::oeffnung::{
    HeightLine, HeightRef, Install, Opening, OpeningKind, MAX_INSTALL, MAX_REVEAL,
};
use crate::{Guid, Model, StoreyId};
use sk_math::vec3;
use std::collections::HashMap;

/// Höhe als Text.
fn height(m: &Model, h: HeightRef) -> String {
    let g = |st| m.storey(st).map_or("-".to_string(), |s| s.guid.to_ifc());
    match h {
        HeightRef::Line(st, HeightLine::Sill) => format!("{}:sill", g(st)),
        HeightRef::Line(st, HeightLine::Lintel) => format!("{}:lintel", g(st)),
        HeightRef::Level(r) => {
            let e = match r.edge {
                LevelEdge::Bottom => "u",
                LevelEdge::Top => "o",
            };
            format!("{}:{e}:{}", g(r.storey), r.offset)
        }
    }
}

pub(super) fn write(m: &Model, elements: &[&Element], out: &mut String) {
    for e in elements {
        let ElementKind::Opening(o) = &e.kind else {
            continue;
        };
        let pts = o
            .path
            .iter()
            .map(|p| format!("{} {}", p.x, p.y))
            .collect::<Vec<_>>()
            .join(";");
        let l = Line::new("opening")
            .guid("guid", Some(e.guid))
            .word("art", o.kind.word())
            .text("pts", &pts)
            .word("bottom", &height(m, o.bottom))
            .word("top", &height(m, o.top));
        let l = match o.reveal {
            Some(r) => l.word("reveal", &format!("{},{},{},{}", r[0], r[1], r[2], r[3])),
            None => l,
        };
        let mm = |l: Line, key: &str, v: Option<f64>| match v {
            Some(v) => l.word(key, &v.to_string()),
            None => l,
        };
        let l = mm(l, "install", o.install.outer);
        let l = mm(l, "element", o.install.element);
        l.text("number", &e.number)
            .word("cat", category(e.category))
            .num("seq", e.seq)
            .guid("storey", m.storey(e.storey).map(|s| s.guid))
            .finish(out);
    }
}

/// Höhe aus `key`.
fn read_height(
    r: &Record,
    key: &str,
    storeys: &HashMap<Guid, StoreyId>,
) -> Result<HeightRef, LoadError> {
    let v = r.get(key)?;
    let form = "Guid:sill|lintel oder Guid:u|o:Versatz";
    let parts: Vec<&str> = v.split(':').collect();
    let storey = Guid::from_ifc(parts[0])
        .and_then(|g| storeys.get(&g).copied())
        .ok_or_else(|| {
            err(
                r.line,
                format!("[opening]: „{key}“ verweist auf unbekanntes Geschoss"),
            )
        })?;
    match parts[1..] {
        ["sill"] => Ok(HeightRef::Line(storey, HeightLine::Sill)),
        ["lintel"] => Ok(HeightRef::Line(storey, HeightLine::Lintel)),
        [e, o] => {
            let edge = match e {
                "u" => LevelEdge::Bottom,
                "o" => LevelEdge::Top,
                _ => return Err(r.bad(key, form)),
            };
            let offset = o
                .parse::<f64>()
                .ok()
                .filter(|v| v.is_finite())
                .ok_or_else(|| r.bad(key, form))?;
            Ok(HeightRef::Level(LevelRef {
                storey,
                edge,
                offset,
            }))
        }
        _ => Err(r.bad(key, form)),
    }
}

/// Hinweis zu einem Wert, der nicht zählt.
fn verworfen(r: &Record, key: &str, folge: &str) -> String {
    format!(
        "Zeile {}: [opening] „{key}={}“ ungültig, {folge}",
        r.line,
        r.opt(key).unwrap_or("")
    )
}

/// Parametrik einer `[opening]`-Zeile. Ein ungültiger Wert gibt einen
/// Hinweis: Art, Anschlag und Einbautiefen nehmen die Vorgabe, ein
/// unlesbarer Pfad verwirft die Öffnung (`None`, Review #79). Eine Höhe, die
/// auf kein Geschoss oder keine Höhenlinie zeigt, lehnt die Datei ab wie
/// jeder andere Verweis.
pub(super) fn read(
    r: &Record,
    storeys: &HashMap<Guid, StoreyId>,
    hints: &mut Vec<String>,
) -> Result<Option<ElementKind>, LoadError> {
    let kind = match r.opt("art") {
        Some(_) => match keyword(r, "art", &OpeningKind::ALL, OpeningKind::word) {
            Ok(k) => k,
            Err(_) => {
                hints.push(verworfen(r, "art", "Fenster gilt"));
                OpeningKind::Window
            }
        },
        None => keyword(r, "art", &OpeningKind::ALL, OpeningKind::word)?,
    };
    let path = r
        .get("pts")?
        .split(';')
        .map(|p| {
            let mut it = p.split_whitespace().map(|x| x.parse::<f64>().ok());
            match (it.next(), it.next(), it.next()) {
                (Some(Some(x)), Some(Some(y)), None) if x.is_finite() && y.is_finite() => {
                    Some(vec3(x, y, 0.0))
                }
                _ => None,
            }
        })
        .collect::<Option<Vec<_>>>()
        .filter(|p| (2..=3).contains(&p.len()));
    let Some(path) = path else {
        hints.push(verworfen(r, "pts", "Öffnung verworfen"));
        // Die übrigen Schlüssel der Zeile nicht noch einmal melden
        for k in [
            "guid", "bottom", "top", "reveal", "install", "element", "number", "cat", "seq",
            "storey",
        ] {
            r.opt(k);
        }
        return Ok(None);
    };
    let reveal = r.opt("reveal").and_then(|v| {
        let n: Vec<Option<f64>> = v
            .split(',')
            .map(|x| {
                x.parse::<f64>()
                    .ok()
                    .filter(|x| (0.0..=MAX_REVEAL).contains(x))
            })
            .collect();
        match n[..] {
            [Some(a), Some(b), Some(c), Some(d)] => Some([a, b, c, d]),
            _ => {
                hints.push(verworfen(r, "reveal", "Anschlag nach Wandtyp"));
                None
            }
        }
    });
    let mut mm = |key: &str| -> Option<f64> {
        let v = r.opt(key)?;
        let x = v
            .parse::<f64>()
            .ok()
            .filter(|x| (0.0..=MAX_INSTALL).contains(x));
        if x.is_none() {
            hints.push(verworfen(r, key, "Vorgabe gilt"));
        }
        x
    };
    let install = Install {
        outer: mm("install"),
        element: mm("element"),
    };
    Ok(Some(ElementKind::Opening(Opening {
        kind,
        path,
        bottom: read_height(r, "bottom", storeys)?,
        top: read_height(r, "top", storeys)?,
        reveal,
        install,
    })))
}
