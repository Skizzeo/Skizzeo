//! Rechenweg und Bestellmenge im Mengenblatt (Jörn 11.10.,
//! vob/plan-rechenweg-bestellmengen.md): Zeilen des Rechenwegs unter den
//! Schichtzeilen, der Rechenweg als Text für die Tabelle und die Faktoren
//! der Bestellmenge für die Gliederung nach Geschoss. Ausgelagert, damit
//! `schedule_view.rs` nicht weiter wächst (R4).

use super::*;
use sk_model::rechenweg::Rechenweg;

/// Bestellmenge je Bauteil für die Gliederung nach Geschoss: Verhältnis der
/// Bestellmenge zur Menge über alle Schichten (Volumen, Fläche). Leer in der
/// VOB-Sicht.
pub(super) type Faktoren = HashMap<ElementId, (f64, f64)>;

pub(super) fn bestell_faktoren(m: &Model, sched: &Schedule, modus: Modus) -> Faktoren {
    let mut sum: HashMap<ElementId, [f64; 4]> = HashMap::new();
    if modus == Modus::Vob {
        return HashMap::new();
    }
    // alle Schichtzeilen, auch ohne Gewerk (Review #77, Hinweis)
    for (_, r) in sched.layer_rows(m) {
        let x = sum.entry(r.element).or_default();
        x[0] += r.volume;
        x[1] += rechenweg::anzeige(m, &r, Bezug::Volumen, modus);
        x[2] += rechenweg::wert(&r, Bezug::Flaeche);
        x[3] += rechenweg::anzeige(m, &r, Bezug::Flaeche, modus);
    }
    let q = |a: f64, b: f64| if a > 0.0 { b / a } else { 1.0 };
    sum.into_iter()
        .map(|(e, x)| (e, (q(x[0], x[1]), q(x[2], x[3]))))
        .collect()
}

/// Hauptmenge einer Schichtzeile für den Rechenweg: Beton nach Volumen,
/// Blech nach Länge, sonst die Fläche (VOB/C Abschnitt 0.5 der ATV).
pub(super) fn haupt_bezug(m: &Model, r: &LayerRow) -> Option<Bezug> {
    let cat = m.material(r.material).map(|x| x.category);
    let v = |b| rechenweg::wert(r, b) > 0.0;
    let wahl = match cat {
        Some(sk_model::MatCategory::Concrete) => [Bezug::Volumen, Bezug::Flaeche, Bezug::Laenge],
        Some(sk_model::MatCategory::Metal) => [Bezug::Laenge, Bezug::Flaeche, Bezug::Volumen],
        _ => [Bezug::Flaeche, Bezug::Volumen, Bezug::Laenge],
    };
    // Attikablech: nur in m abgerechnet
    if r.category == Category::Coping && r.layer == 0 {
        return v(Bezug::Laenge).then_some(Bezug::Laenge);
    }
    wahl.into_iter().find(|&b| v(b))
}

/// Menge im Bezug als Zelle: „12,40 m“, „31,82 m²“, „7,248 m³“.
pub(super) fn menge_zelle(x: f64, b: Bezug) -> String {
    match b {
        Bezug::Laenge => m_len(x),
        Bezug::Flaeche => m_area(x),
        Bezug::Volumen => m_vol(x),
    }
}

/// Spalte der Menge im Bezug.
pub(super) fn spalte(b: Bezug) -> usize {
    match b {
        Bezug::Laenge => 2,
        Bezug::Flaeche => 3,
        Bezug::Volumen => 4,
    }
}

/// Zeilen des Rechenwegs unter der Schichtzeile `row`, nur wenn sie in
/// `offen` aufgeklappt ist (eingeklappt nur das Dreieck):
/// Terme mit Vorzeichen, bei mehr als einem Term das Ergebnis.
pub(super) fn weg_zeilen(
    m: &Model,
    r: &LayerRow,
    row: &mut Line,
    (modus, offen): (Modus, &HashSet<Key>),
    lines: &mut Vec<Line>,
) {
    let Some(b) = haupt_bezug(m, r) else { return };
    // Zugeklappt nur das Dreieck; den Rechenweg rechnet erst das
    // Aufklappen (Review #77/1: sonst je Zeile bei jedem Neuaufbau)
    row.weg = true;
    if !offen.contains(&row.key) {
        return;
    }
    let Some(w) = rechenweg::weg(m, r, b, modus) else {
        return;
    };
    let e = r.element;
    let layer = r.layer.min(255) as u8;
    let key = |i: usize| {
        Key::Term(
            e.index(),
            e.generation(),
            layer,
            spalte(b) as u8,
            i.min(255) as u8,
        )
    };
    term_zeilen(&w, row, key, lines);
}

/// Zeilen eines beliebigen Rechenwegs unter `row` (Wandschicht, später
/// Raum und Wohnfläche): eingeklappt, Terme mit Vorzeichen, der Wert in der
/// Spalte seines Bezugs, bei mehr als einem Wert das Ergebnis. `key` gibt
/// jeder Zeile ihren Schlüssel nach ihrer Stelle. Macht `row` aufklappbar.
pub(super) fn term_zeilen(
    w: &Rechenweg,
    row: &mut Line,
    key: impl Fn(usize) -> Key,
    lines: &mut Vec<Line>,
) {
    if w.terme.is_empty() {
        return;
    }
    row.weg = true;
    let b = w.bezug;
    let mut push = |i: usize, text: String, cell: String| {
        let mut l = Line::new(Kind::Control, row.depth + 1, key(i));
        l.storey = row.storey;
        l.group = row.group;
        l.parent = row.key;
        l.elements = row.elements.clone();
        l.cells[0] = text;
        l.cells[spalte(b)] = cell;
        lines.push(l);
    };
    for (i, t) in w.terme.iter().enumerate() {
        let (text, cell) = match t.art {
            Art::Basis => (t.text.clone(), menge_zelle(t.wert, b)),
            Art::Hinweis => (t.text.clone(), String::new()),
            _ if t.wert < 0.0 => (
                format!("− {}", t.text),
                format!("− {}", menge_zelle(-t.wert, b)),
            ),
            _ => (
                format!("+ {}", t.text),
                format!("+ {}", menge_zelle(t.wert, b)),
            ),
        };
        push(i, text, cell);
    }
    if w.terme.iter().filter(|t| t.art != Art::Hinweis).count() > 1 {
        push(w.terme.len(), "= Menge".into(), menge_zelle(w.menge(), b));
    }
}

/// Rechenweg der Menge in der Sicht `modus` als eine Zeile für die Tabelle: „Länge 7,545 m ×
/// Höhe 2,855 m (21,54 m²) − Deckenauflager … (1,66 m²) = 19,88 m²“.
pub(super) fn weg_text(m: &Model, r: &LayerRow, modus: Modus) -> String {
    let Some(b) = haupt_bezug(m, r) else {
        return String::new();
    };
    let Some(w) = rechenweg::weg(m, r, b, modus) else {
        return String::new();
    };
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
