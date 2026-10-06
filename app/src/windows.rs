//! Mengenfenster neben dem Hauptfenster (F2): Regeln aus der Plattform,
//! Titel und Merken in `einstellungen.txt` (Abschnitt `[mengenfenster]`,
//! nie in der `.szo`).

use crate::document::Document;
use sk_model::szo::{Line, Record};
pub use sk_platform::layout::{on_screen, Rect, WindowId, Windows, WIDTH_DIP};
#[cfg(test)]
pub use sk_platform::layout::{Action, Show};

/// Titel des Mengenfensters, mit „•“ bei ungespeicherten Änderungen.
pub fn quantity_caption(doc: &Document, rev: u64) -> String {
    format!("Mengenermittlung – {}", doc.caption_at(rev))
}

/// Abschnitt für `einstellungen.txt`; leer, solange das Fenster nie offen war.
pub fn write_settings(w: &Windows) -> String {
    let mut out = String::new();
    let rect = w.remembered_rect();
    if !w.quantity_open() && rect.is_none() {
        return out;
    }
    let mut l = Line::new("mengenfenster")
        .flag("offen", w.quantity_open())
        .flag("angedockt", w.docked());
    if let Some((x, y, b, h)) = rect {
        l = l.num("x", x).num("y", y).num("breite", b).num("hoehe", h);
    }
    l.finish(&mut out);
    out
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
