//! Entdecken-Hinweise (Paket 4 §1.8, A0b): einmalige, dezente Hinweise in
//! der Statuszeile, wenn man etwas zum ersten Mal tut. Gemerkt wird in
//! `einstellungen.txt` (`[hinweise] hint_seen="hide_counts,trades"`).

use sk_model::szo::{Line, Record};
use std::collections::BTreeSet;

/// Kennung und Text je Hinweis.
pub const HINTS: [(&str, &str); 2] = [
    (
        "hide_counts",
        "Ausgeblendetes zählt in den Mengen weiter. ‚Alles zeigen‘ holt es zurück.",
    ),
    (
        "trades",
        "Die Gewerke folgen der VOB/C und kommen vom Baustoff. Das Mengenfenster gliedert auch nach Gewerk.",
    ),
];

/// Text zum Hinweis `id`.
pub fn text(id: &str) -> Option<&'static str> {
    HINTS.iter().find(|(k, _)| *k == id).map(|(_, t)| *t)
}

/// Gesehene Hinweise aus dem Text von `einstellungen.txt`.
pub fn seen(text: &str) -> BTreeSet<String> {
    text.lines()
        .enumerate()
        .filter_map(|(i, l)| Record::parse(i + 1, l).ok().flatten())
        .filter(|r| r.section == "hinweise")
        .filter_map(|r| r.opt("hint_seen").map(str::to_string))
        .flat_map(|v| {
            v.split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect::<Vec<_>>()
        })
        .collect()
}

/// Zeile `[hinweise]`; leer, solange keiner gesehen ist.
pub fn line(seen: &BTreeSet<String>) -> String {
    let mut out = String::new();
    if !seen.is_empty() {
        let v: Vec<&str> = seen.iter().map(String::as_str).collect();
        Line::new("hinweise")
            .text("hint_seen", &v.join(","))
            .finish(&mut out);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gesehene_hinweise_rundlauf() {
        let mut s = BTreeSet::new();
        assert_eq!(line(&s), "");
        s.insert("trades".to_string());
        s.insert("hide_counts".to_string());
        let l = line(&s);
        assert_eq!(l, "[hinweise] hint_seen=\"hide_counts,trades\"\n");
        assert_eq!(seen(&l), s);
        assert!(text("trades").is_some_and(|t| t.contains("VOB/C")));
        assert!(text("unbekannt").is_none());
    }
}
