//! Schema als Text (K6): alle Abschnitte mit allen Feldern aus der einen
//! Feldtabelle, dazu die Operationen. Fenster, Tests und später eine KI lesen
//! daraus, was es gibt.

use crate::satz::{Kennung, ABSCHNITTE};

/// Das Schema als Text, aus der Feldtabelle gebaut. Jeder Aufruf baut neu:
/// `sk-cost` hält keinen Zustand (Bausteingrenze §8, kein `OnceLock`).
pub fn schema() -> String {
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
    s += "Operationen · Projekt über Scene::kosten, Firma über firma_anwenden · * nur in der Verwaltung\n";
    for (name, angaben, admin) in crate::op::NAMEN {
        s += &format!("  {name}{} ({angaben})\n", if admin { " *" } else { "" });
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

    /// Abnahme 14 (Teil Operationen): Jede Operation steht im Schema.
    #[test]
    fn schema_nennt_jede_operation() {
        let s = schema();
        let i = s.find("\nOperationen ").expect("Operationen");
        for (name, _, admin) in crate::op::NAMEN {
            let zeile = format!("\n  {name}{} (", if admin { " *" } else { "" });
            assert!(s[i..].contains(&zeile), "{name}");
        }
    }
}
