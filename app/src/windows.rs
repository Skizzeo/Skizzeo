//! Mengenfenster neben dem Hauptfenster (F2): Regeln aus der Plattform,
//! Titel und Merken in `einstellungen.txt` (Abschnitt `[mengenfenster]`,
//! nie in der `.szo`).

use crate::cards::Blatt;
use crate::document::Document;
use crate::schedule_view::Grouping;
use sk_model::szo::{Line, Record};
pub use sk_platform::layout::{on_screen, Rect, WindowId, Windows, WIDTH_DIP};
#[cfg(test)]
pub use sk_platform::layout::{Action, Show};

/// Titel des Mengenfensters, mit „•“ bei ungespeicherten Änderungen.
pub fn quantity_caption(doc: &Document, rev: u64) -> String {
    format!("Mengenermittlung – {}", doc.caption_at(rev))
}

/// Titel nach dem gezeigten Blatt (KA-2a): „Kosten – haus.szo“; das
/// Mengenblatt behält „Mengenermittlung – …“.
pub fn blatt_caption(b: Blatt, doc: &Document, rev: u64) -> String {
    match b {
        Blatt::Mengen => quantity_caption(doc, rev),
        Blatt::Kosten | Blatt::Ava => format!("{} – {}", b.name(), doc.caption_at(rev)),
    }
}

/// Abschnitt für `einstellungen.txt`; leer, solange das Fenster nie offen war.
#[cfg(test)]
pub fn write_settings(w: &Windows) -> String {
    write_settings_grouped(w, Grouping::Storey)
}

/// Wie [`write_settings`], dazu die Gliederung der Liste (Paket 1b);
/// `gliederung=` steht nur, wenn sie nicht nach Geschoss ist.
#[cfg(test)]
pub fn write_settings_grouped(w: &Windows, g: Grouping) -> String {
    write_settings_blatt(w, g, Blatt::Mengen)
}

/// Wie [`write_settings_grouped`], dazu das zuletzt gezeigte Blatt (KA-2a);
/// `blatt=` steht nur, wenn es nicht das Mengenblatt ist.
pub fn write_settings_blatt(w: &Windows, g: Grouping, b: Blatt) -> String {
    let mut out = String::new();
    let rect = w.remembered_rect();
    if !w.quantity_open() && rect.is_none() && g == Grouping::Storey && b == Blatt::Mengen {
        return out;
    }
    let mut l = Line::new("mengenfenster")
        .flag("offen", w.quantity_open())
        .flag("angedockt", w.docked());
    if let Some((x, y, b, h)) = rect {
        l = l.num("x", x).num("y", y).num("breite", b).num("hoehe", h);
    }
    if g != Grouping::Storey {
        l = l.word("gliederung", g.key());
    }
    if b != Blatt::Mengen {
        l = l.word("blatt", b.key());
    }
    l.finish(&mut out);
    out
}

/// Gemerkte Gliederung der Liste; ohne Angabe nach Geschoss.
pub fn read_grouping(text: &str) -> Grouping {
    text.lines()
        .enumerate()
        .filter_map(|(i, l)| Record::parse(i + 1, l).ok().flatten())
        .filter(|r| r.section == "mengenfenster")
        .find_map(|r| r.opt("gliederung").and_then(Grouping::from_key))
        .unwrap_or_default()
}

/// Zuletzt gezeigtes Blatt; ohne Angabe das Mengenblatt.
pub fn read_blatt(text: &str) -> Blatt {
    text.lines()
        .enumerate()
        .filter_map(|(i, l)| Record::parse(i + 1, l).ok().flatten())
        .filter(|r| r.section == "mengenfenster")
        .find_map(|r| r.opt("blatt").and_then(Blatt::from_key))
        .unwrap_or_default()
}

/// Liest den Abschnitt aus `einstellungen.txt`. `monitors`: Arbeitsbereiche
/// der angeschlossenen Bildschirme; liegt das frei gemerkte Fenster auf
/// keinem davon, wird es wieder angedockt.
pub fn read_settings(text: &str, monitors: &[Rect]) -> Windows {
    let mut w = Windows::new(WIDTH_DIP);
    for (i, l) in text.lines().enumerate() {
        if let Ok(Some(r)) = Record::parse(i + 1, l) {
            if r.section == "mengenfenster" {
                w = from_record(&r, monitors);
            }
        }
    }
    w
}

/// Eine Zeile `[mengenfenster]`.
pub fn from_record(r: &Record, monitors: &[Rect]) -> Windows {
    let flag = |k: &str| r.opt(k) == Some("1");
    let num = |k: &str| r.opt(k).and_then(|v| v.parse::<i32>().ok());
    let rect = match (num("x"), num("y"), num("breite"), num("hoehe")) {
        (Some(x), Some(y), Some(b), Some(h)) if b > 0 && h > 0 => Some((x, y, b, h)),
        _ => None,
    };
    let docked = flag("angedockt") || rect.is_none_or(|r| !on_screen(r, monitors));
    Windows::remembered(WIDTH_DIP, flag("offen"), docked, rect)
}
