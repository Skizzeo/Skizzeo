//! Abweichungen des Raumbuchs je Raum in der `.szo` (RB6,
//! [`crate::model::AusbauAbweichung`]): je Abweichung eine Zeile
//! `[spacefinish] space=<Raum> at=floor|screed|ceiling|wall|tiles|skirting`,
//! an einer Wandseite mit `wall=<Wand> side=l|r`, dazu `type=<Ausbautyp>`
//! oder `type=none` und bei Wandfliesen `h=` (mm), nach den Schaltern der
//! Ansichten. Ohne Abweichung keine Zeile, alte Dateien bleiben bytegleich.
//! Eine Zeile, die nicht zählt (unlesbar, Raum oder Typ fehlt, Typ anderer
//! Art, doppelt), gibt einen Hinweis und bleibt roh stehen (Regel 72).

use super::{Line, Record};
use crate::element::{Element, ElementKind};
use crate::id::Arena;
use crate::library::LayerSet;
use crate::model::{AusbauAbweichung, AusbauStelle, AusbauWahl, Model, WandSeite, TILES_HEIGHT};
use crate::Guid;

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
    for (raum, v) in raeume {
        for a in v {
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
    for raw in m.space_finish_raw() {
        out.push_str(raw);
        out.push('\n');
    }
}

/// Abweichungen nach Raum und die Zeilen, die nicht zählen (roh).
pub(super) type Gelesen = (Vec<(Guid, AusbauAbweichung)>, Vec<String>);

/// Liest `[spacefinish]`; `types` und `els` sind Bauteiltypen und Bauteile
/// derselben Datei.
pub(super) fn read(
    recs: &[Record],
    lines: &[&str],
    types: &Arena<LayerSet>,
    els: &Arena<Element>,
    hints: &mut Vec<String>,
) -> Gelesen {
    let mut out: Vec<(Guid, AusbauAbweichung)> = Vec::new();
    let mut roh = Vec::new();
    for r in recs {
        let a = zeile(r, types, els);
        match a {
            Ok((raum, a)) if !out.iter().any(|x| x.0 == raum && x.1.stelle == a.stelle) => {
                out.push((raum, a));
            }
            fehler => {
                r.skip();
                roh.push(lines[r.line - 1].to_string());
                let grund = fehler.err().unwrap_or_else(|| "doppelt".into());
                hints.push(format!(
                    "Zeile {}: [spacefinish] {grund}; der Raum nimmt dort die Vorlage, die Zeile bleibt",
                    r.line
                ));
            }
        }
    }
    (out, roh)
}

fn zeile(
    r: &Record,
    types: &Arena<LayerSet>,
    els: &Arena<Element>,
) -> Result<(Guid, AusbauAbweichung), String> {
    let unlesbar = |_| "unlesbar".to_string();
    let raum = r.guid("space").map_err(unlesbar)?;
    if !els
        .iter()
        .any(|(_, e)| e.guid == raum && matches!(e.kind, ElementKind::Space(_)))
    {
        return Err("Raum fehlt".into());
    }
    let seite = match r.opt("wall") {
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
            let t = types
                .iter()
                .find(|(_, t)| t.guid == g)
                .map(|(_, t)| t)
                .ok_or("Ausbautyp fehlt")?;
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
