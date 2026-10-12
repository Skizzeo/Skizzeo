//! Darstellung in der `.szo`: `[display] slot=… pen=… lt=…` je Stelle
//! ([`crate::Display`]). Die Werksdarstellung des Einbauelements (F20) und
//! der Stelle „Über der Schnittebene“ (G7b) steht nicht in der Datei, beim
//! Lesen entsteht sie wieder.

use super::{err, LoadError, Record, EDGE_NAMES};
use crate::attr::{Display, EdgeStyle, LineType, Pen};
use crate::guid::{Guid, GuidGen};
use crate::id::{Arena, Id};
use crate::solid::edge_kind;
use std::collections::HashMap;

/// Werks-Linientyp für die Stellen „element“ und „above“, die in der Datei
/// fehlen (dort gilt die Werksdarstellung).
pub(super) type Werk = (Option<Id<LineType>>, Option<Id<LineType>>);

/// Ergänzt den Werks-Linientyp [`crate::attr::ELEMENT_LINE_TYPE`], wenn eine
/// fehlende Stelle oder eine Zeile ihn braucht.
pub(super) fn werk_linientyp(
    recs: &[Record],
    line_types: &mut Arena<LineType>,
    lt_ids: &mut HashMap<Guid, Id<LineType>>,
) -> Werk {
    let has = |slot: &str| recs.iter().any(|r| r.get("slot").is_ok_and(|s| s == slot));
    let (has_element, has_above) = (has("element"), has("above"));
    let werk_lt = crate::attr::ELEMENT_LINE_TYPE_GUID;
    let uses_werk = recs
        .iter()
        .any(|r| r.guid("lt").is_ok_and(|g| g == werk_lt));
    if !lt_ids.contains_key(&werk_lt) && (!has_element || !has_above || uses_werk) {
        let id = line_types.insert(crate::attr::element_line_type());
        lt_ids.insert(werk_lt, id);
    }
    let werk = |has: bool| (!has).then(|| lt_ids[&werk_lt]);
    (werk(has_element), werk(has_above))
}

/// Liest die Darstellungs-Slots. Fehlt einer, gilt sein Startwert (gleiche
/// Stiftnummer bzw. gleicher Linientyp-Name wie in den Starttabellen).
pub(super) fn read(
    recs: &[Record],
    pens: &Arena<Pen>,
    line_types: &Arena<LineType>,
    pen_ids: &HashMap<Guid, Id<Pen>>,
    lt_ids: &HashMap<Guid, Id<LineType>>,
    (pattern_pen, (element_lt, above_lt)): (Option<Id<Pen>>, Werk),
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
    slots.push(("element".into(), sd.element));
    slots.push(("above".into(), sd.above));
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
                // Einbauelement (F20) und über der Schnittebene (G7b): Stift
                // der feinen Linie in Zeichnungen, der ergänzte Linientyp
                let werk = match name.as_str() {
                    "element" => element_lt,
                    "above" => above_lt,
                    _ => None,
                };
                if let Some(line_type) = werk {
                    let fine = EDGE_NAMES
                        .iter()
                        .position(|(_, k)| *k == edge_kind::FINE)
                        .and_then(|i| resolved.get(i).copied())
                        .ok_or_else(|| {
                            err(0, format!("Darstellung „{name}“ fehlt, keine feine Linie"))
                        })?;
                    resolved.push(EdgeStyle {
                        pen: fine.pen,
                        line_type,
                    });
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
        element: resolved[2 * n + 5],
        above: resolved[2 * n + 6],
        paper,
    })
}
