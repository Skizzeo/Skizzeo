//! Abweichungen des Raumbuchs je Raum in der `.szo` (RB6,
//! [`crate::model::AusbauAbweichung`]): je Abweichung eine Zeile
//! `[spacefinish] space=<Raum> at=floor|screed|ceiling|wall|tiles|skirting`,
//! an einer Wandseite mit `wall=<Wand> side=l|r`, dazu `type=<Ausbautyp>`
//! oder `type=none` und bei Wandfliesen `h=` (mm), nach den Schaltern der
//! Ansichten. Ohne Abweichung keine Zeile, alte Dateien bleiben bytegleich.
//! Eine Zeile, die nicht zählt (unlesbar, Raum oder Typ fehlt, Typ anderer
//! Art, doppelt), gibt einen Hinweis und bleibt roh stehen (Regel 72):
//! vor der gültigen Zeile derselben Kennung, wenn sie in der Datei davor
//! stand, sonst am Ende.

use super::{Line, Record};
use crate::catalog::spacefinish_kennung;
use crate::element::{Element, ElementKind};
use crate::id::Arena;
use crate::library::LayerSet;
use crate::model::{
    AusbauAbweichung, AusbauStelle, AusbauWahl, Model, RohZeile, WandSeite, TILES_HEIGHT,
};
use crate::Guid;
use std::collections::{HashMap, HashSet};

/// Wort der Stelle in `at=`.
fn wort(s: AusbauStelle) -> &'static str {
    match s {
        AusbauStelle::Boden => "floor",
        AusbauStelle::Estrich => "screed",
        AusbauStelle::Decke => "ceiling",
        AusbauStelle::Wand(_) => "wall",
        AusbauStelle::Fliesen(_) => "tiles",
        AusbauStelle::Sockel(_) => "skirting",
    }
}

pub(super) fn write(m: &Model, els: &Arena<Element>, out: &mut String) {
    let mut raeume: Vec<(Guid, &[AusbauAbweichung])> = els
        .iter()
        .filter_map(|(_, e)| match &e.kind {
            ElementKind::Space(s) if !s.ausbau.is_empty() => Some((e.guid, s.ausbau.as_slice())),
            _ => None,
        })
        .collect();
    raeume.sort_by_key(|r| r.0);
    let roh = m.space_finish_raw();
    let mut vorn: HashMap<(Guid, AusbauStelle), Vec<usize>> = HashMap::new();
    for (i, (_, an)) in roh.iter().enumerate() {
        if let Some(an) = an {
            vorn.entry(*an).or_default().push(i);
        }
    }
    let zeile = |i: usize, out: &mut String| {
        out.push_str(&roh[i].0);
        out.push('\n');
    };
    for (raum, v) in raeume {
        for a in v {
            for i in vorn.remove(&(raum, a.stelle)).unwrap_or_default() {
                zeile(i, out);
            }
            let mut l = Line::new("spacefinish")
                .guid("space", Some(raum))
                .word("at", wort(a.stelle));
            if let Some(w) = a.stelle.seite() {
                l = l
                    .guid("wall", Some(w.wand))
                    .word("side", if w.links { "l" } else { "r" });
            }
            match a.wahl {
                Some(AusbauWahl::Typ(g)) => l = l.guid("type", Some(g)),
                Some(AusbauWahl::Keiner) => l = l.word("type", "none"),
                None => {}
            }
            if let Some(h) = a.hoehe {
                l = l.num("h", h);
            }
            l.finish(out);
        }
    }
    // Der Rest in Dateifolge, auch vor einer Abweichung, die es nicht mehr gibt
    for (i, (_, an)) in roh.iter().enumerate() {
        if an.is_none_or(|an| vorn.contains_key(&an)) {
            zeile(i, out);
        }
    }
}

/// Abweichungen nach Raum und die Zeilen, die nicht zählen (roh).
pub(super) type Gelesen = (Vec<(Guid, AusbauAbweichung)>, Vec<RohZeile>);

/// Liest `[spacefinish]`; `types` und `els` sind Bauteiltypen und Bauteile
/// derselben Datei. Räume und Typen einmal nachgeschlagen (Review #190 B3).
pub(super) fn read(
    recs: &[Record],
    lines: &[&str],
    types: &Arena<LayerSet>,
    els: &Arena<Element>,
    hints: &mut Vec<String>,
) -> Gelesen {
    let raeume: HashSet<Guid> = els
        .iter()
        .filter(|(_, e)| matches!(e.kind, ElementKind::Space(_)))
        .map(|(_, e)| e.guid)
        .collect();
    let waende: HashSet<Guid> = els
        .iter()
        .filter(|(_, e)| matches!(e.kind, ElementKind::Wall(_)))
        .map(|(_, e)| e.guid)
        .collect();
    let typen: HashMap<Guid, &LayerSet> = types.iter().map(|(_, t)| (t.guid, t)).collect();
    let mut out: Vec<(Guid, AusbauAbweichung)> = Vec::new();
    let mut gesehen: HashSet<(Guid, AusbauStelle)> = HashSet::new();
    // Kennung der gültigen Zeilen; rohe mit ihrer Kennung und ob die
    // gültige schon davor stand
    let mut gueltig: HashMap<String, (Guid, AusbauStelle)> = HashMap::new();
    let mut roh: Vec<(String, String, bool)> = Vec::new();
    for r in recs {
        match zeile(r, &typen, &raeume, &waende) {
            Ok((raum, a)) if gesehen.insert((raum, a.stelle)) => {
                gueltig.insert(spacefinish_kennung(r), (raum, a.stelle));
                out.push((raum, a));
            }
            fehler => {
                r.skip();
                let k = spacefinish_kennung(r);
                let danach = gueltig.contains_key(&k);
                roh.push((lines[r.line - 1].to_string(), k, danach));
                let grund = fehler.err().unwrap_or_else(|| "doppelt".into());
                hints.push(format!(
                    "Zeile {}: [spacefinish] {grund}; der Raum nimmt dort die Vorlage, die Zeile bleibt",
                    r.line
                ));
            }
        }
    }
    let roh = roh
        .into_iter()
        .map(|(z, k, danach)| {
            let an = if danach {
                None
            } else {
                gueltig.get(&k).copied()
            };
            (z, an)
        })
        .collect();
    (out, roh)
}

fn zeile(
    r: &Record,
    types: &HashMap<Guid, &LayerSet>,
    raeume: &HashSet<Guid>,
    waende: &HashSet<Guid>,
) -> Result<(Guid, AusbauAbweichung), String> {
    let unlesbar = |_| "unlesbar".to_string();
    let raum = r.guid("space").map_err(unlesbar)?;
    if !raeume.contains(&raum) {
        return Err("Raum fehlt".into());
    }
    let seite = match r.opt("wall") {
        // Seite ohne Wand: als rohe Zeile wörtlich behalten, sonst ginge
        // `side=` und mit der Kennung jeder fremde Schlüssel verloren
        // (Review #190 H-190g)
        None if r.opt("side").is_some() => return Err("unlesbar".into()),
        None => None,
        Some(_) => Some(WandSeite {
            wand: r.guid("wall").map_err(unlesbar)?,
            links: match r.get("side").map_err(unlesbar)? {
                "l" => true,
                "r" => false,
                _ => return Err("unlesbar".into()),
            },
        }),
    };
    if seite.is_some_and(|w| !waende.contains(&w.wand)) {
        return Err("Wand fehlt".into());
    }
    let stelle = match (r.get("at").map_err(unlesbar)?, seite) {
        ("floor", None) => AusbauStelle::Boden,
        ("screed", None) => AusbauStelle::Estrich,
        ("ceiling", None) => AusbauStelle::Decke,
        ("wall", s) => AusbauStelle::Wand(s),
        ("tiles", s) => AusbauStelle::Fliesen(s),
        ("skirting", s) => AusbauStelle::Sockel(s),
        _ => return Err("unlesbar".into()),
    };
    let wahl = match r.opt("type") {
        None => None,
        Some("none") => Some(AusbauWahl::Keiner),
        Some(_) => {
            let g = r.guid("type").map_err(unlesbar)?;
            let t = types.get(&g).ok_or("Ausbautyp fehlt")?;
            if t.category != stelle.art() {
                return Err(format!("{} ist kein {}", t.name, stelle.art().name()));
            }
            Some(AusbauWahl::Typ(g))
        }
    };
    let hoehe = match r.opt("h") {
        None => None,
        Some(_) => {
            let h = r.f64("h").map_err(unlesbar)?;
            let (lo, hi) = TILES_HEIGHT;
            if !matches!(stelle, AusbauStelle::Fliesen(_)) || !(lo..=hi).contains(&h) {
                return Err("Höhe nur bei Wandfliesen, 0,10 bis 4,00 m".into());
            }
            Some(h)
        }
    };
    let a = AusbauAbweichung {
        stelle,
        wahl,
        hoehe,
    };
    if a.leer() {
        return Err("ohne Typ und Höhe".into());
    }
    Ok((raum, a))
}
