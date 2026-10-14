//! Räume im Mengenblatt (RB5, `raumbuch/plan.md` §6.2): nach Geschoss je
//! Raum eine aufklappbare Gruppe mit den Zeilen seines Raumbuchs (Boden,
//! Estrich, Wandseiten, Decke, Längen), jede mit ihrem Rechenweg; nach
//! Gewerk stehen die Schichten der Räume in den Schichtgruppen „… · Räume“,
//! je Raum eine Zeile, deren Rechenweg die Flächen nennt. Ausgelagert wie
//! die Erweiterungen (R4).

use super::*;
use sk_model::qto::{RoomLine, RoomQto, Side};
use sk_model::rechenweg::{Rechenweg, Term};

/// Name jeder Raumzeile nach (Raum, Stelle im Raum), für die Rechenwege
/// nach Gewerk.
pub(super) type Namen = HashMap<(ElementId, usize), String>;

/// Namen der Raumzeilen eines Gebäudes.
pub(super) fn namen(b: &BuildingQto) -> Namen {
    let mut out = Namen::new();
    for r in b
        .storeys
        .iter()
        .flat_map(|s| &s.groups)
        .flat_map(|g| &g.rows)
    {
        if let Some(ElementQto::Room(q)) = &r.q {
            for (i, l) in q.lines.iter().enumerate() {
                out.insert((r.element, i), l.name.clone());
            }
        }
    }
    out
}

/// Rechenweg einer Raumzeile nach Gewerk: vor den Termen der Name der
/// Fläche („Wand AW-004 Nord“), damit die Terme mehrerer Flächen eines
/// Raums lesbar bleiben.
pub(super) fn mit_namen(r: &mut LayerRow, namen: &Namen) {
    if let Some(n) = namen.get(&(r.element, r.layer)) {
        r.terms.insert(
            0,
            Term {
                text: n.clone(),
                wert: 0.0,
                art: Art::Hinweis,
            },
        );
    }
}

/// Flächen einer Raumzeile nach Gewerk: „Boden“, „Außenwände“ (KG 336,
/// Bekleidung innen), „Innenwände“ (KG 345), „Decke“.
pub(super) fn flaechen(r: &LayerRow) -> &'static str {
    match (r.side, r.kg) {
        (Some(Side::Floor), _) => "Boden",
        (Some(Side::Ceiling), _) => "Decke",
        (_, Some(336)) => "Außenwände",
        (_, Some(345)) => "Innenwände",
        _ => "Wände",
    }
}

/// „Wohnzimmer · lichte Höhe 2,575 m“.
fn titel(m: &Model, e: ElementId, q: &RoomQto) -> String {
    let name = m.space(e).map_or("Raum", |s| s.name.as_str());
    if q.height > 0.0 {
        format!("{name} · lichte Höhe {} m", de(q.height / 1e3, 3))
    } else {
        name.to_string()
    }
}

/// „Wand AW-004 Nord · Gipsputz 1,5 cm“, die Sockelleiste mit ihrem Typ.
fn zeile_titel(m: &Model, l: &RoomLine) -> String {
    let typ = l.finish.and_then(|(t, _)| m.layer_set(t));
    let stoff = l
        .material
        .and_then(|x| m.material(x))
        .map(|x| x.name.as_str());
    match (l.key, typ, stoff) {
        (Some(_), Some(t), _) => format!("{} · {}", l.name, t.name),
        (_, _, Some(s)) if l.thickness > 0.0 => format!("{} · {s} {}", l.name, cm(l.thickness)),
        (_, _, Some(s)) => format!("{} · {s}", l.name),
        _ => l.name.clone(),
    }
}

/// Warum eine Zeile nur in der Liste steht.
fn nur_liste(l: &RoomLine) -> Option<&'static str> {
    if !l.info {
        return None;
    }
    Some(if l.name.starts_with("Laibungen") {
        "abgerechnet an der Öffnung"
    } else if l.name.starts_with("Dichtband") {
        "Teil der Abdichtung"
    } else {
        "nur zur Kontrolle"
    })
}

/// Nach Geschoss: die Räume der Gruppe `g` im Geschoss `st`, je Raum eine
/// Gruppe mit ihren Zeilen.
pub(super) fn gruppe(
    m: &Model,
    st: &StoreyQto,
    skey: Key,
    g: &GroupQto,
    (modus, offen): (Modus, &HashSet<Key>),
    lines: &mut Vec<Line>,
) {
    for r in &g.rows {
        let Some(ElementQto::Room(q)) = &r.q else {
            continue;
        };
        let e = r.element;
        let gkey = Key::Group(st.id.index(), Category::Space as u8, e.index());
        let mut gl = Line::new(Kind::Group, 1, gkey);
        gl.storey = skey;
        gl.elements = vec![e];
        gl.cells[0] = titel(m, e, q);
        gl.cells[1] = r.number.clone();
        gl.cells[3] = m_area(q.area);
        lines.push(gl);
        let schichten = sk_model::qto_raum::layer_rows(m, r, st.id, q);
        for (i, l) in q.lines.iter().enumerate() {
            let key = Key::LayerRow(e.index(), e.generation(), i.min(u16::MAX as usize) as u16);
            let mut rl = Line::new(Kind::Row, 2, key);
            rl.storey = skey;
            rl.group = gkey;
            rl.cells[0] = zeile_titel(m, l);
            rl.note = nur_liste(l).map(str::to_string);
            let mut weg = Vec::new();
            match schichten.iter().find(|x| x.layer == i) {
                // Schicht: Menge in der Sicht, Rechenweg wie nach Gewerk
                Some(x) => {
                    let zeig = |b| rechenweg::anzeige(m, x, b, modus);
                    if x.length > 0.0 {
                        rl.cells[2] = m_len(zeig(Bezug::Laenge));
                    }
                    if x.area > 0.0 {
                        rl.cells[3] = m_area(zeig(Bezug::Flaeche));
                    }
                    if x.volume > 0.0 {
                        rl.cells[4] = m_vol(zeig(Bezug::Volumen));
                    }
                    weg_zeilen(m, x, &mut rl, (modus, offen), &mut weg);
                }
                // Längen mit Schlüssel und Hinweise: die Menge des Raumbuchs
                None => {
                    let b = l.weg.bezug;
                    rl.cells[spalte(b)] = menge_zelle(l.quantity(), b);
                    rl.weg = true;
                    if offen.contains(&key) {
                        let tk = |j: usize| {
                            Key::Term(
                                e.index(),
                                e.generation(),
                                i.min(u16::MAX as usize) as u16,
                                spalte(b) as u8,
                                j.min(u16::MAX as usize) as u16,
                            )
                        };
                        term_zeilen(&l.weg, &mut rl, tk, &mut weg);
                    }
                }
            }
            lines.push(rl);
            lines.extend(weg);
        }
    }
}

/// Nach Geschoss in der Tabelle: je Raum eine Zeile mit der Bodenfläche im
/// Fertigmaß, darunter seine Zeilen mit Kostengruppe und Rechenweg.
pub(super) fn csv_geschoss(
    m: &Model,
    (gb, sname): (&str, &str),
    st: &StoreyQto,
    g: &GroupQto,
    modus: Modus,
    row: &mut impl FnMut([&str; 10]),
) {
    let len = |mm: f64| csv_num(mm / 1e3);
    let area = |mm2: f64| csv_num(mm2 / 1e6);
    let vol = |mm3: f64| csv_num(mm3 / 1e9);
    for r in &g.rows {
        let Some(ElementQto::Room(q)) = &r.q else {
            continue;
        };
        let e = r.element;
        row([
            gb,
            sname,
            "",
            &titel(m, e, q),
            &r.number,
            "",
            "1",
            &area(q.area),
            "",
            "",
        ]);
        let schichten = sk_model::qto_raum::layer_rows(m, r, st.id, q);
        for (i, l) in q.lines.iter().enumerate() {
            let kg = l.kg.map_or(String::new(), |k| k.to_string());
            let (mut l_, mut a_, mut v_) = (String::new(), String::new(), String::new());
            let weg = match schichten.iter().find(|x| x.layer == i) {
                Some(x) => {
                    let zeig = |b| rechenweg::anzeige(m, x, b, modus);
                    if x.length > 0.0 {
                        l_ = len(zeig(Bezug::Laenge));
                    }
                    if x.area > 0.0 {
                        a_ = area(zeig(Bezug::Flaeche));
                    }
                    if x.volume > 0.0 {
                        v_ = vol(zeig(Bezug::Volumen));
                    }
                    weg_text(m, x, modus)
                }
                None => {
                    match l.weg.bezug {
                        Bezug::Laenge => l_ = len(l.quantity()),
                        Bezug::Flaeche => a_ = area(l.quantity()),
                        Bezug::Volumen => v_ = vol(l.quantity()),
                    }
                    let w = als_text(&l.weg);
                    match nur_liste(l) {
                        Some(n) => format!("{n}: {w}"),
                        None => w,
                    }
                }
            };
            row([
                gb,
                sname,
                &kg,
                &zeile_titel(m, l),
                &r.number,
                &l_,
                "",
                &a_,
                &v_,
                &weg,
            ]);
        }
    }
}

/// Rechenweg der Raumzeile als Text wie [`weg_text`].
fn als_text(w: &Rechenweg) -> String {
    let b = w.bezug;
    let mut teile: Vec<String> = Vec::new();
    for t in &w.terme {
        teile.push(match t.art {
            Art::Hinweis => format!("({})", t.text),
            Art::Basis => format!("{} ({})", t.text, menge_zelle(t.wert, b)),
            _ if t.wert < 0.0 => format!("− {} ({})", t.text, menge_zelle(-t.wert, b)),
            _ => format!("+ {} ({})", t.text, menge_zelle(t.wert, b)),
        });
    }
    teile.push(format!("= {}", menge_zelle(w.menge(), b)));
    teile.join(" ")
}
