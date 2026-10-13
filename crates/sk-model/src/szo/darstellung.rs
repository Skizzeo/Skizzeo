//! Darstellung in der `.szo`: `[display] slot=… pen=… lt=…` je Stelle
//! ([`crate::Display`]). Die Werksdarstellung des Einbauelements (F20) und
//! der Stelle „Über der Schnittebene“ (G7b) steht nicht in der Datei, beim
//! Lesen entsteht sie wieder. Eine Zeile, die auf einen unbekannten Stift oder
//! Linientyp zeigt, zählt nicht: Hinweis, es gilt der Startwert der Stelle
//! (bei diesen beiden die Werksdarstellung), und die Zeile wird an ihrer
//! Stelle bytegleich zurückgeschrieben, solange niemand die Stelle neu setzt
//! (Regel 72/73).

use super::{err, Line, LoadError, Record, EDGE_NAMES};
use crate::attr::{Attributes, Display, EdgeStyle, LineType, Pen};
use crate::guid::{Guid, GuidGen};
use crate::id::{Arena, Id};
use crate::solid::edge_kind;
use std::collections::HashMap;

/// Werks-Linientyp für die Stellen „element“ und „above“, die in der Datei
/// fehlen (dort gilt die Werksdarstellung).
pub(super) type Werk = (Option<Id<LineType>>, Option<Id<LineType>>);

/// Stellen mit Werksdarstellung, die nur geändert in der Datei stehen.
const WERKSSTELLEN: [&str; 2] = ["element", "above"];

/// Werksstelle der Zeile, wenn sie eine ist.
fn werksstelle(r: &Record) -> Option<&'static str> {
    let slot = r.get("slot").ok()?;
    WERKSSTELLEN.into_iter().find(|w| *w == slot)
}

/// Zählt die Zeile? Sie zeigt auf einen gelesenen Stift und einen gelesenen
/// Linientyp oder den Werks-Linientyp.
fn zaehlt(
    r: &Record,
    pen_ids: &HashMap<Guid, Id<Pen>>,
    lt_ids: &HashMap<Guid, Id<LineType>>,
) -> bool {
    let werk_lt = crate::attr::ELEMENT_LINE_TYPE_GUID;
    r.guid("pen").is_ok_and(|g| pen_ids.contains_key(&g))
        && r.guid("lt")
            .is_ok_and(|g| g == werk_lt || lt_ids.contains_key(&g))
}

/// Ergänzt den Werks-Linientyp [`crate::attr::ELEMENT_LINE_TYPE`], wenn eine
/// fehlende Stelle oder eine Zeile ihn braucht.
pub(super) fn werk_linientyp(
    recs: &[Record],
    pen_ids: &HashMap<Guid, Id<Pen>>,
    line_types: &mut Arena<LineType>,
    lt_ids: &mut HashMap<Guid, Id<LineType>>,
) -> Werk {
    let gilt = |r: &Record| zaehlt(r, pen_ids, lt_ids);
    let has = |slot: &str| recs.iter().any(|r| werksstelle(r) == Some(slot) && gilt(r));
    let (has_element, has_above) = (has("element"), has("above"));
    let werk_lt = crate::attr::ELEMENT_LINE_TYPE_GUID;
    let uses_werk = recs
        .iter()
        .any(|r| gilt(r) && r.guid("lt").is_ok_and(|g| g == werk_lt));
    if !lt_ids.contains_key(&werk_lt) && (!has_element || !has_above || uses_werk) {
        let id = line_types.insert(crate::attr::element_line_type());
        lt_ids.insert(werk_lt, id);
    }
    let werk = |has: bool| (!has).then(|| lt_ids[&werk_lt]);
    (werk(has_element), werk(has_above))
}

/// Zeile, die beim Lesen nicht zählte: ihre Stelle, der Wert, der statt
/// ihrer gilt, ob sie an der Stelle allein stand, ob sie vor der gültigen
/// Zeile der Stelle stand, und die Zeile selbst.
#[derive(Clone, Debug)]
pub(crate) struct RohZeile {
    stelle: String,
    statt: EdgeStyle,
    allein: bool,
    vor: bool,
    zeile: String,
}

/// Zeilen der Darstellung, die beim Lesen nicht zählten.
pub(crate) type Roh = Vec<RohZeile>;

/// Schreibt die Darstellungs-Stellen. Gilt an einer Stelle noch der Wert,
/// der beim Lesen statt ungültiger Zeilen gesetzt wurde, stehen dort die
/// Zeilen bytegleich, neben einer gültigen Zeile der Stelle davor oder
/// danach wie beim Lesen; neu gesetzt steht die Stelle nur gültig in der
/// Datei.
pub(super) fn write(a: &Attributes, out: &mut String) {
    let d = a.display();
    let mut slot = |name: &str, s: &EdgeStyle, schreiben: bool| {
        let roh = a.display_raw().iter();
        let roh: Vec<&RohZeile> = roh.filter(|r| r.stelle == name && r.statt == *s).collect();
        let zeilen = |out: &mut String, vor: bool| {
            for r in roh.iter().filter(|r| r.vor == vor) {
                out.push_str(&r.zeile);
                out.push('\n');
            }
        };
        // Ungültige Zeilen vor der gültigen bleiben davor (Review #133 H1)
        zeilen(out, true);
        if schreiben && !roh.iter().any(|r| r.allein) {
            Line::new("display")
                .word("slot", name)
                .guid("pen", a.pen(s.pen).map(|p| p.guid))
                .guid("lt", a.line_type(s.line_type).map(|l| l.guid))
                .finish(out);
        }
        zeilen(out, false);
    };
    for (prefix, table) in [("drawing", &d.drawing), ("model3d", &d.model3d)] {
        for (name, k) in EDGE_NAMES {
            slot(&format!("{prefix}.{name}"), &table[k as usize], true);
        }
    }
    slot("ground", &d.ground, true);
    slot("section_line", &d.section_line, true);
    slot("section_ends", &d.section_ends, true);
    slot("background", &d.background, true);
    slot("pattern", &d.pattern, true);
    slot("element", &d.element, !a.element_is_default());
    slot("above", &d.above, !a.above_is_default());
    Line::new("display")
        .word("slot", "paper")
        .color("color", d.paper)
        .finish(out);
}

/// Liest die Darstellungs-Slots. Fehlt einer, gilt sein Startwert (gleiche
/// Stiftnummer bzw. gleicher Linientyp-Name wie in den Starttabellen).
#[allow(clippy::too_many_arguments)]
pub(super) fn read(
    recs: &[Record],
    lines: &[&str],
    pens: &Arena<Pen>,
    line_types: &Arena<LineType>,
    pen_ids: &HashMap<Guid, Id<Pen>>,
    lt_ids: &HashMap<Guid, Id<LineType>>,
    (pattern_pen, (element_lt, above_lt)): (Option<Id<Pen>>, Werk),
    hints: &mut Vec<String>,
) -> Result<(Display, Roh), LoadError> {
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
    // Ungültige Zeilen je Stelle (Nummer in `slots`), ob noch vor einer
    // gültigen
    let mut ungueltig: Vec<(usize, bool, &str)> = Vec::new();
    for r in recs {
        let slot = r.get("slot")?;
        if slot == "paper" {
            paper = Some(r.color("color")?);
            continue;
        }
        let Some(i) = slots.iter().position(|s| s.0 == slot) else {
            hints.push(format!(
                "Zeile {}: unbekannter Darstellungs-Slot „{slot}“ übersprungen",
                r.line
            ));
            continue;
        };
        if !zaehlt(r, pen_ids, lt_ids) {
            r.skip();
            ungueltig.push((i, styles[i].is_none(), lines[r.line - 1]));
            let gilt = match werksstelle(r) {
                Some(_) => "die Werksdarstellung gilt",
                None => "der Startwert gilt",
            };
            hints.push(format!(
                "Zeile {}: Darstellung „{slot}“ zeigt auf einen unbekannten Stift oder Linientyp, übersprungen; {gilt}, die Zeile bleibt",
                r.line
            ));
            continue;
        }
        styles[i] = Some(EdgeStyle {
            pen: r.link("pen", pen_ids)?,
            line_type: r.link("lt", lt_ids)?,
        });
    }
    let allein: Vec<bool> = styles.iter().map(Option::is_none).collect();
    let mut resolved = Vec::with_capacity(slots.len());
    for (i, ((name, std), s)) in slots.iter().zip(styles).enumerate() {
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
                // Der Hintergrund (E16) fehlt in allen älteren Dateien: still
                // ergänzen; zu einer ungültigen Zeile steht der Hinweis schon
                if name != "background" && !ungueltig.iter().any(|u| u.0 == i) {
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
    let roh = ungueltig
        .into_iter()
        .map(|(i, vor, zeile)| RohZeile {
            stelle: slots[i].0.clone(),
            statt: resolved[i],
            allein: allein[i],
            vor,
            zeile: zeile.to_string(),
        })
        .collect();
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
    let display = Display {
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
    };
    Ok((display, roh))
}
