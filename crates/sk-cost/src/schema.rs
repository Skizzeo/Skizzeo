//! Schema als Text (K6): alle Abschnitte mit allen Feldern aus der einen
//! Feldtabelle, dazu die Operationen. Fenster, Tests und später eine KI lesen
//! daraus, was es gibt.

use crate::satz::{Kennung, ABSCHNITTE};
use std::sync::OnceLock;

/// Das Schema als Text, einmal aus der Feldtabelle gebaut.
pub fn schema() -> &'static str {
    static S: OnceLock<String> = OnceLock::new();
    S.get_or_init(bauen)
}

fn bauen() -> String {
    let mut s = String::from("Skizzeo-Kostenschema\n\n");
    for a in ABSCHNITTE {
        let datei = match (a.szo, a.szk) {
            (true, true) => ".szo, .szk",
            (true, false) => ".szo",
            _ => ".szk",
        };
        let kennung = match a.kennung {
            Kennung::Guid => "guid",
            Kennung::Key => "key",
        };
        s += &format!(
            "[{}] {} · Kennung {kennung} · {datei}\n",
            a.name, a.bedeutung
        );
        for f in a.felder {
            s += &format!(
                "  {} = {}{} · {}\n",
                f.name,
                f.art.beschreibung(),
                if f.pflicht { ", Pflicht" } else { "" },
                f.bedeutung
            );
        }
        s.push('\n');
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Abnahme 14 (Teil Abschnitte): Jedes Feld, das der Leser kennt, steht
    /// im Schema.
    #[test]
    fn schema_nennt_jedes_feld() {
        let s = schema();
        for a in ABSCHNITTE {
            let kopf = format!("\n[{}] ", a.name);
            let i = s.find(&kopf).unwrap_or_else(|| panic!("{kopf}"));
            let block = &s[i..s[i + 1..].find("\n\n").map_or(s.len(), |j| i + 1 + j)];
            for f in a.felder {
                assert!(
                    block.contains(&format!("\n  {} = ", f.name)),
                    "{} {}",
                    a.name,
                    f.name
                );
            }
        }
    }
}
