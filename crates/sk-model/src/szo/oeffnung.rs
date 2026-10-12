//! Öffnungen in der `.szo` (Paket O2, [`crate::oeffnung`]):
//!
//! `[opening] guid=… art=window pts="x y;x y" bottom=Guid:sill
//! top=Guid:lintel number="OE-001" cat=opening seq=3 storey=…`
//!
//! Eine Höhe ist `Guid:sill` bzw. `Guid:lintel` (an die Höhenlinie des
//! Geschosses gebunden) oder `Guid:u|o:Versatz` (frei). `reveal=l,r,o,u`
//! steht nur, wenn der Anschlag nicht dem Wandtyp folgt, `install=…`
//! (äußere Einbautiefe) und `element=…` (Bautiefe) nur, wenn gesetzt.
//! Eine Tür mit Blatt trägt `swing=l|r` (Aufschlagseite zur Pfadrichtung)
//! und `din=links|rechts` (Türhand nach DIN 107). Breitenschlösser stehen
//! als `lock="0 Guid 1;…"` (eigenes Pfadende, Öffnung, deren Pfadende),
//! nur wenn beide Öffnungen aufeinander zeigen.

use super::{category, err, keyword, Line, LoadError, Record};
use crate::element::{Element, ElementKind, LevelEdge, LevelRef};
use crate::model::RawOpening;
use crate::oeffnung::{
    Din, EndLock, FaceSide, HeightLine, HeightRef, Install, Opening, OpeningKind, Swing,
    MAX_INSTALL, MAX_REVEAL,
};
use crate::{ElementId, Guid, Model, StoreyId};
use sk_math::vec3;
use std::collections::HashMap;

/// Höhe als Text.
fn height(m: &Model, h: HeightRef) -> String {
    let g = |st| m.storey(st).map_or("-".to_string(), |s| s.guid.to_ifc());
    match h {
        HeightRef::Line(st, HeightLine::Sill) => format!("{}:sill", g(st)),
        HeightRef::Line(st, HeightLine::Lintel) => format!("{}:lintel", g(st)),
        HeightRef::Line(st, HeightLine::Door) => format!("{}:door", g(st)),
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
        let l = match o.swing {
            Some(sw) => l
                .word("swing", side_word(sw.side))
                .word("din", sw.din.word()),
            None => l,
        };
        let l = match locks_text(m, e, o) {
            Some(t) => l.text("lock", &t),
            None => l,
        };
        l.text("number", &e.number)
            .word("cat", category(e.category))
            .num("seq", e.seq)
            .guid("storey", m.storey(e.storey).map(|s| s.guid))
            .finish(out);
    }
    for raw in m.opening_raw() {
        out.push_str(raw);
        out.push('\n');
    }
}

/// Breitenschlösser als Text, nur die, deren Partner zurückzeigt.
fn locks_text(m: &Model, e: &Element, o: &Opening) -> Option<String> {
    let t: Vec<String> = o
        .lock
        .iter()
        .filter_map(|(k, l)| {
            let p = m.element(l.partner)?;
            let ElementKind::Opening(po) = &p.kind else {
                return None;
            };
            let back = po.lock.iter().any(|(pk, pl)| {
                *pk == l.end
                    && pl.end == *k
                    && m.element(pl.partner).is_some_and(|x| x.guid == e.guid)
            });
            back.then(|| format!("{k} {} {}", p.guid.to_ifc(), l.end))
        })
        .collect();
    (!t.is_empty()).then(|| t.join(";"))
}

/// Gelesene, noch nicht aufgelöste Breitenschlösser einer Öffnung: eigenes
/// Pfadende, Guid der Partneröffnung, deren Pfadende.
pub(super) type RawLocks = Vec<(usize, Guid, usize)>;

/// Breitenschlösser aus `lock=`; ein unlesbarer Eintrag gibt einen
/// Hinweis und entfällt.
fn read_locks(r: &Record, n: usize, hints: &mut Vec<String>) -> RawLocks {
    let Some(v) = r.opt("lock") else {
        return Vec::new();
    };
    let ende = |x: &str| x.parse::<usize>().ok();
    let mut out = Vec::new();
    for t in v.split(';') {
        let f: Vec<&str> = t.split_whitespace().collect();
        let eintrag = match f[..] {
            [k, g, e] => ende(k)
                .filter(|k| *k == 0 || *k + 1 == n)
                .zip(Guid::from_ifc(g))
                .zip(ende(e))
                .map(|((k, g), e)| (k, g, e)),
            _ => None,
        };
        match eintrag {
            Some(x) => out.push(x),
            None => hints.push(format!(
                "Zeile {}: [opening] Breitenschloss „{t}“ unlesbar, entfällt",
                r.line
            )),
        }
    }
    out
}

/// Löst die Breitenschlösser nach dem Lesen aller Öffnungen auf: Ein
/// Schloss, dessen Partner fehlt, kein Pfadende hat oder nicht
/// zurückzeigt, entfällt mit Hinweis.
pub(super) fn resolve_locks(
    elements: &mut crate::id::Arena<Element>,
    ids: &HashMap<Guid, ElementId>,
    pending: Vec<(ElementId, usize, RawLocks)>,
    hints: &mut Vec<String>,
) {
    let mut gelesen = Vec::new();
    for (id, line, raw) in pending {
        if !raw.is_empty() {
            gelesen.push((id, line));
        }
        let mut locks = Vec::new();
        for (k, g, end) in raw {
            let partner = ids.get(&g).copied().filter(|p| {
                *p != id
                    && matches!(elements.get(*p).map(|e| &e.kind),
                        Some(ElementKind::Opening(o)) if o.is_end(end))
            });
            match partner {
                Some(partner) => locks.push((k, EndLock { partner, end })),
                None => hints.push(format!(
                    "Zeile {line}: [opening] Breitenschloss zeigt auf keine Öffnung, entfällt"
                )),
            }
        }
        if let Some(ElementKind::Opening(o)) = elements.get_mut(id).map(|e| &mut e.kind) {
            o.lock = locks;
        }
    }
    // Einseitig gilt es nicht und fiele beim Speichern still weg (Review
    // #109 B6)
    for (id, line) in gelesen {
        let Some(ElementKind::Opening(o)) = elements.get(id).map(|e| &e.kind) else {
            continue;
        };
        let einseitig: Vec<(usize, EndLock)> = o
            .lock
            .iter()
            .copied()
            .filter(|(k, l)| {
                let back = (
                    l.end,
                    EndLock {
                        partner: id,
                        end: *k,
                    },
                );
                !matches!(elements.get(l.partner).map(|e| &e.kind),
                    Some(ElementKind::Opening(p)) if p.lock.contains(&back))
            })
            .collect();
        for _ in &einseitig {
            hints.push(format!(
                "Zeile {line}: [opening] Breitenschloss ohne Gegenstück, entfällt"
            ));
        }
        if let Some(ElementKind::Opening(o)) = elements.get_mut(id).map(|e| &mut e.kind) {
            o.lock.retain(|x| !einseitig.contains(x));
        }
    }
}

/// Eine verworfene Zeile roh (Regel 72). Sie hält ihre Nummer und ihre
/// Guid (Review #86 B3), und sie merkt sich die Geschosse, auf die sie
/// zeigt (`storey`, `bottom`, `top`): Ist eines gelöscht, entfällt sie beim
/// Speichern. Trägt sie eine Nummer oder Guid, die schon vergeben ist,
/// entfällt sie gleich mit Hinweis, statt das Laden abzubrechen (L12).
pub(super) fn raw(
    r: &Record,
    line: &str,
    number: &mut impl FnMut(&Record) -> Result<String, LoadError>,
    seen: &mut HashMap<Guid, usize>,
    hints: &mut Vec<String>,
) -> Option<RawOpening> {
    let guid = r.opt("guid").and_then(Guid::from_ifc);
    let doppelt = |was: String| {
        format!(
            "Zeile {}: [opening] {was} doppelt, die Zeile entfällt",
            r.line
        )
    };
    if let Some(g) = guid.filter(|g| seen.contains_key(g)) {
        hints.push(doppelt(format!("Guid {}", g.to_ifc())));
        return None;
    }
    let number = match r.opt("number") {
        Some(n) => match number(r) {
            Ok(n) => Some(n),
            Err(_) => {
                hints.push(doppelt(format!("Nummer {n}")));
                return None;
            }
        },
        None => None,
    };
    if let Some(g) = guid {
        seen.insert(g, r.line);
    }
    let storeys = ["storey", "bottom", "top"]
        .iter()
        .filter_map(|k| r.opt(k))
        .filter_map(|v| Guid::from_ifc(v.split(':').next().unwrap_or("")))
        .collect();
    Some(RawOpening {
        line: line.to_string(),
        storeys,
        number,
    })
}

/// Höhe aus `key`; `None`, wenn der Versatz einer freien Höhe unlesbar
/// ist (die Öffnung fällt mit Hinweis weg). Ein unbekanntes Geschoss oder
/// eine unbekannte Höhenlinie lehnt die Datei ab.
fn read_height(
    r: &Record,
    key: &str,
    storeys: &HashMap<Guid, StoreyId>,
) -> Result<Option<HeightRef>, LoadError> {
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
        ["sill"] => Ok(Some(HeightRef::Line(storey, HeightLine::Sill))),
        ["lintel"] => Ok(Some(HeightRef::Line(storey, HeightLine::Lintel))),
        ["door"] => Ok(Some(HeightRef::Line(storey, HeightLine::Door))),
        [e, o] => {
            let edge = match e {
                "u" => Some(LevelEdge::Bottom),
                "o" => Some(LevelEdge::Top),
                _ => None,
            };
            let offset = o.parse::<f64>().ok().filter(|v| v.is_finite());
            Ok(edge.zip(offset).map(|(edge, offset)| {
                HeightRef::Level(LevelRef {
                    storey,
                    edge,
                    offset,
                })
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
/// Hinweis: Art (auch fehlend), Anschlag und Einbautiefen nehmen die
/// Vorgabe, ein unlesbarer Pfad oder Versatz verwirft die Öffnung (`None`,
/// Review #79; die Zeile bleibt roh, Regel 72). Eine Höhe, die auf kein Geschoss oder keine Höhenlinie
/// zeigt, lehnt die Datei ab wie jeder andere Verweis.
pub(super) fn read(
    r: &Record,
    storeys: &HashMap<Guid, StoreyId>,
    hints: &mut Vec<String>,
) -> Result<Option<(ElementKind, RawLocks)>, LoadError> {
    let kind = match r.opt("art") {
        Some(_) => match keyword(r, "art", &OpeningKind::ALL, OpeningKind::word) {
            Ok(k) => k,
            Err(_) => {
                hints.push(verworfen(r, "art", "Fenster gilt"));
                OpeningKind::Window
            }
        },
        None => {
            hints.push(format!(
                "Zeile {}: [opening] ohne „art“, Fenster gilt",
                r.line
            ));
            OpeningKind::Window
        }
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
    // Die übrigen Schlüssel einer verworfenen Zeile nicht noch einmal melden
    let rest = |r: &Record| {
        for k in [
            "guid", "bottom", "top", "reveal", "install", "element", "swing", "din", "lock",
            "number", "cat", "seq", "storey",
        ] {
            r.opt(k);
        }
    };
    let Some(path) = path else {
        hints.push(verworfen(r, "pts", "Öffnung verworfen, die Zeile bleibt"));
        rest(r);
        return Ok(None);
    };
    let (Some(bottom), Some(top)) = (
        read_height(r, "bottom", storeys)?,
        read_height(r, "top", storeys)?,
    ) else {
        hints.push(format!(
            "Zeile {}: [opening] Höhe „bottom={}“ / „top={}“ unlesbar, Öffnung verworfen, die Zeile bleibt",
            r.line,
            r.opt("bottom").unwrap_or(""),
            r.opt("top").unwrap_or("")
        ));
        rest(r);
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
    let swing = read_swing(r, kind, path.len(), hints);
    let locks = read_locks(r, path.len(), hints);
    let o = Opening {
        kind,
        path,
        bottom,
        top,
        reveal,
        install,
        swing,
        lock: Vec::new(),
    };
    Ok(Some((ElementKind::Opening(o), locks)))
}

fn side_word(s: FaceSide) -> &'static str {
    match s {
        FaceSide::Left => "l",
        FaceSide::Right => "r",
    }
}

/// Aufschlag aus `swing=` und `din=`; fehlen beide, keiner. Ein ungültiger,
/// halber oder an einer Öffnung ohne Blatt stehender Aufschlag gibt einen
/// Hinweis und entfällt (wie `door=`, Integration 05:54).
fn read_swing(r: &Record, kind: OpeningKind, n: usize, hints: &mut Vec<String>) -> Option<Swing> {
    let (s, d) = (r.opt("swing"), r.opt("din"));
    if s.is_none() && d.is_none() {
        return None;
    }
    let side = match s {
        Some("l") => Some(FaceSide::Left),
        Some("r") => Some(FaceSide::Right),
        _ => None,
    };
    let din = Din::ALL.into_iter().find(|x| d == Some(x.word()));
    match (side, din) {
        (Some(side), Some(din)) if kind == OpeningKind::Door && n == 2 => Some(Swing { side, din }),
        _ => {
            hints.push(format!(
                "Zeile {}: [opening] Aufschlag „swing={}“ / „din={}“ ungültig, ohne Aufschlag",
                r.line,
                s.unwrap_or(""),
                d.unwrap_or("")
            ));
            None
        }
    }
}
