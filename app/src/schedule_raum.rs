//! Räume im Mengenblatt (RB5, `raumbuch/plan.md` §6.2): nach Geschoss je
//! Raum eine aufklappbare Gruppe mit den Zeilen seines Raumbuchs (Boden,
//! Estrich, Wandseiten, Decke, Längen), jede mit ihrem Rechenweg; nach
//! Gewerk stehen die Schichten der Räume in den Schichtgruppen „… · Räume“,
//! je Raum eine Zeile, deren Rechenweg die Flächen nennt, und dahinter die
//! Längen der Räume ([`laengen`]). Ausgelagert wie die Erweiterungen (R4).

use super::*;
use sk_model::qto::{RoomLine, RoomQto, RowQto, Side};
use sk_model::rechenweg::{Rechenweg, Term};
use sk_model::trade::TradeId;
use sk_model::StoreyId;

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
/// Raums lesbar bleiben. Hängt die Terme von `r` an `terme` an.
pub(super) fn mit_namen(r: &LayerRow, namen: &Namen, terme: &mut Vec<Term>) {
    terme.reserve(r.terms.len() + 1);
    if let Some(n) = namen.get(&(r.element, r.layer)) {
        terme.push(Term {
            text: n.clone(),
            wert: 0.0,
            art: Art::Hinweis,
        });
    }
    terme.extend(r.terms.iter().cloned());
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

/// „Wohnzimmer · Grundfläche (fertig) · lichte Höhe 2,575 m“: die Fläche
/// der Gruppenzeile ist die Grundfläche im Fertigmaß, die Zeile „Boden
/// (VOB)“ darunter die Abrechnungsfläche mit Türnischen (43.8).
fn titel(m: &Model, e: ElementId, q: &RoomQto) -> String {
    let name = m.space(e).map_or("Raum", |s| s.name.as_str());
    if q.height > 0.0 {
        format!(
            "{name} · Grundfläche (fertig) · lichte Höhe {} m",
            de(q.height / 1e3, 3)
        )
    } else {
        format!("{name} · Grundfläche (fertig)")
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
            // Überfahren, Klick und Doppelklick wie an Bauteilzeilen (43.9)
            rl.elements = vec![e];
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

/// Schlüssel der Längen in der Reihenfolge nach Gewerk.
const SCHLUESSEL: [&str; 4] = [
    sk_raum::raumbuch::SOCKEL,
    sk_raum::raumbuch::RANDSTREIFEN,
    sk_raum::raumbuch::ECKSCHIENE,
    sk_raum::raumbuch::SILIKON,
];

/// Eine Länge der Räume nach Gewerk: Sockelleiste je Typ,
/// Randdämmstreifen, Eckschienen, Silikonfugen, mit den Räumen.
pub(super) struct Laengen<'a> {
    pub(super) trade: TradeId,
    /// Stelle des Schlüssels in [`SCHLUESSEL`] und Typ der Sockelleiste.
    art: (u32, Option<sk_model::LayerSetId>),
    titel: String,
    /// Raum, Geschoss, Stelle der Zeile im Raum, Zeile.
    zeilen: Vec<(&'a RowQto, StoreyId, usize, &'a RoomLine)>,
}

/// Längen der Räume des Gebäudes `b` nach Gewerk (Bedienbarkeit 43.10, mit
/// Kosten abgestimmt), im Gewerk der Fläche, an der sie liegen
/// (`sk_raum::mengen`). Nur Anzeige: Die Längen sind Automatikmengen am
/// Raum (`room.*`, in den Kosten über `auto=` bepreist), keine Schichten;
/// jede steht einmal, in der Summe des Gewerks zählen sie nicht. Einmal je
/// Aufbau, so wächst das mit den Räumen.
pub(super) fn laengen<'a>(m: &Model, b: &'a BuildingQto) -> Vec<Laengen<'a>> {
    let mut out: Vec<Laengen> = Vec::new();
    for st in &b.storeys {
        for r in st.groups.iter().flat_map(|g| &g.rows) {
            let Some(ElementQto::Room(q)) = &r.q else {
                continue;
            };
            for (i, l) in q.lines.iter().enumerate() {
                let (Some(t), Some(k)) = (l.trade, l.key) else {
                    continue;
                };
                let Some(k) = SCHLUESSEL.iter().position(|x| *x == k) else {
                    continue;
                };
                if !l.is_auto() || l.quantity() <= 0.0 {
                    continue;
                }
                let art = (k as u32, l.finish.map(|f| f.0));
                let j = match out.iter().position(|x| x.trade == t && x.art == art) {
                    Some(j) => j,
                    None => {
                        let typ = l.finish.and_then(|(t, _)| m.layer_set(t));
                        let name = typ.map_or(l.name.as_str(), |t| t.name.as_str());
                        out.push(Laengen {
                            trade: t,
                            art,
                            titel: format!("{name} · Räume"),
                            zeilen: Vec::new(),
                        });
                        out.len() - 1
                    }
                };
                out[j].zeilen.push((r, st.id, i, l));
            }
        }
    }
    out.sort_by_key(|x| x.art.0);
    out
}

/// „R-001 … 004“: Nummern der ersten und letzten Zeile.
fn nummern(z: &[(&RowQto, StoreyId, usize, &RoomLine)]) -> String {
    match z {
        [] => String::new(),
        [one] => one.0.number.clone(),
        [first, .., last] => {
            let n = &last.0.number;
            format!("{} … {}", first.0.number, n.rsplit('-').next().unwrap_or(n))
        }
    }
}

/// Nach Gewerk: die Längen `ls` des Gewerks `tkey` als Gruppen
/// „Sockelleiste Eiche 60 × 15 · Räume“, je Raum eine Zeile mit Rechenweg.
pub(super) fn gewerk_laengen(
    m: &Model,
    ls: &[&Laengen],
    tkey: Key,
    offen: &HashSet<Key>,
    lines: &mut Vec<Line>,
) {
    let tk = match tkey {
        Key::Trade(bi, order) => (bi << 16) | (order & 0xffff),
        _ => 0,
    };
    for x in ls {
        let typ = x.art.1.map_or(0, |t| t.index() + 1);
        let gkey = Key::Layer(tk, Category::Space as u8, u32::MAX - x.art.0, typ);
        let mut gl = Line::new(Kind::Group, 1, gkey);
        gl.storey = tkey;
        gl.elements = x.zeilen.iter().map(|z| z.0.element).collect();
        gl.cells[0] = x.titel.clone();
        gl.cells[1] = nummern(&x.zeilen);
        let summe: f64 = x.zeilen.iter().map(|z| z.3.quantity()).sum();
        gl.cells[2] = format!("{} · {}", m_len(summe), x.zeilen.len());
        lines.push(gl);
        for &(r, st, i, l) in &x.zeilen {
            let e = r.element;
            let i = i.min(u16::MAX as usize) as u16;
            let key = Key::LayerRow(e.index(), e.generation(), i);
            let mut rl = Line::new(Kind::Row, 2, key);
            rl.storey = tkey;
            rl.group = gkey;
            rl.elements = vec![e];
            let name = m.space(e).map_or("Raum", |s| s.name.as_str());
            rl.cells[0] = format!("{} {name} · {}", r.number, storey_name(m, st).1);
            rl.cells[2] = m_len(l.quantity());
            rl.weg = true;
            let mut weg = Vec::new();
            if offen.contains(&key) {
                let s = spalte(Bezug::Laenge) as u8;
                let tk = |j: usize| {
                    Key::Term(
                        e.index(),
                        e.generation(),
                        i,
                        s,
                        j.min(u16::MAX as usize) as u16,
                    )
                };
                term_zeilen(&l.weg, &mut rl, tk, &mut weg);
            }
            lines.push(rl);
            lines.extend(weg);
        }
    }
}

/// Nach Gewerk in der Tabelle: die Längen `ls` hinter der Summe des
/// Gewerks, je Gruppe ihre Summe, darunter die Räume mit Rechenweg.
pub(super) fn csv_laengen(
    m: &Model,
    (gb, trade): (&str, &str),
    ls: &[&Laengen],
    row: &mut impl FnMut([&str; 10]),
) {
    let len = |mm: f64| csv_num(mm / 1e3);
    for x in ls {
        let summe: f64 = x.zeilen.iter().map(|z| z.3.quantity()).sum();
        row([
            gb,
            trade,
            "",
            &format!("{} (Summe, {} Stück)", x.titel, x.zeilen.len()),
            &nummern(&x.zeilen),
            "",
            &len(summe),
            "",
            "",
            "Automatikmenge am Raum, nicht in der Summe des Gewerks",
        ]);
        for &(r, st, _, l) in &x.zeilen {
            let kg = l.kg.map_or(String::new(), |k| k.to_string());
            row([
                gb,
                trade,
                &kg,
                &x.titel,
                &r.number,
                &storey_name(m, st).0,
                &len(l.quantity()),
                "",
                "",
                &als_text(&l.weg),
            ]);
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
