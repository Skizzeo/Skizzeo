//! Standardtyp gleich Vorgaben (aenderung-dicke.md, Nachtrag 19:40): die
//! Grenzfälle der Werkbank `pruefdateien/standardtyp/*.szb` und die
//! betroffenen Mutationen mit ihren vollständigen Befunden im Prüfgeschoss.

use sk_szb::{pruefen, Bestand, Geschoss};
use std::path::Path;

fn befunde(text: &str) -> Vec<String> {
    let p = pruefen(text, &Bestand::werk(), &Geschoss::PROBE);
    p.befunde
        .iter()
        .map(|b| {
            let stufe = if b.ist_fehler() { "F" } else { "H" };
            format!("{stufe} {} {}", b.zeile, b.text)
        })
        .collect()
}

/// `=== datei` gefolgt von den Befunden.
fn erwartet(dir: &Path) -> Vec<(String, Vec<String>)> {
    let text = std::fs::read_to_string(dir.join("erwartet.txt")).unwrap();
    let mut out: Vec<(String, Vec<String>)> = Vec::new();
    for z in text.lines() {
        match z.strip_prefix("=== ") {
            Some(n) => out.push((n.trim().to_string(), Vec::new())),
            None if !z.trim().is_empty() => out.last_mut().unwrap().1.push(z.to_string()),
            None => {}
        }
    }
    out
}

fn wie_die_werkbank(dir: &str, anzahl: usize) {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join(dir);
    let soll = erwartet(&dir);
    assert_eq!(soll.len(), anzahl);
    for (name, s) in soll {
        let text = std::fs::read_to_string(dir.join(&name)).unwrap();
        assert_eq!(befunde(&text), s, "{name}");
    }
}

#[test]
fn grenzfaelle() {
    wie_die_werkbank("pruefdateien/standardtyp", 13);
}

#[test]
fn mutationen() {
    wie_die_werkbank("pruefdateien/standardtyp/mutationen", 14);
}
