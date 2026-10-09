//! Fremdes in `einstellungen.txt` (wie in der `.szo`, F-17): Abschnitte,
//! die diese Fassung nicht kennt, und unbekannte Schlüssel in bekannten
//! Abschnitten stammen aus einer neueren Fassung (z. B. Firmenvorgaben).
//! Sie bleiben beim Speichern erhalten: unbekannte Abschnitte Zeile für
//! Zeile im Wortlaut und in ihrer Reihenfolge, eine Zeile mit unbekannten
//! Schlüsseln im Wortlaut, solange die App ihren bekannten Teil nicht
//! ändert, sonst mit ihren unbekannten Schlüsseln hinter der neuen Zeile
//! (bei einem Abschnitt, den die App nicht mehr schreibt, allein).

/// Bekannte Abschnitte, ihre Schlüssel und der Schlüssel, der einen Satz
/// unter mehreren gleichen Abschnitts erkennt (`None`: der Abschnitt steht
/// einmal; `Some("")`: die bekannten Schlüssel selbst, für `[env]`).
const BEKANNT: [(&str, &[&str], Option<&str>); 13] = [
    ("theme", &["base"], None),
    ("color", &["role", "value"], Some("role")),
    ("size", &["key", "value"], Some("key")),
    ("screen", &["px_per_mm"], None),
    (
        "env",
        &["horizon_softness", "ground_opacity", "patterns_3d"],
        Some(""),
    ),
    ("sky", &["t", "value"], Some("t")),
    ("zuletzt", &["datei"], Some("datei")),
    (
        "mengenfenster",
        &[
            "offen",
            "angedockt",
            "x",
            "y",
            "breite",
            "hoehe",
            "gliederung",
            "blatt",
        ],
        None,
    ),
    ("baum", &["karte", "zu", "grenze"], None),
    ("hinweise", &["hint_seen"], None),
    ("firmenkatalog", &["datei"], None),
    ("planung", &["name", "anschrift"], None),
    ("lvblatt", &["titelblatt", "verzeichnis"], None),
];

/// Eine Zeile roh zerlegt: Abschnitt und je Schlüssel sein Text samt
/// führendem Leerzeichen (` key=wert`, Text in Anführungszeichen wie
/// geschrieben). `None` für Leerzeilen, Kommentare und Kaputtes.
fn zerlegen(zeile: &str) -> Option<(&str, Vec<(&str, &str)>)> {
    let t = zeile.trim_end();
    let rest = t.strip_prefix('[')?;
    let zu = rest.find(']')?;
    let abschnitt = rest[..zu].trim();
    let mut teile = Vec::new();
    let s = &rest[zu + 1..];
    let b = s.as_bytes();
    let mut i = 0;
    loop {
        let start = i;
        while i < b.len() && b[i].is_ascii_whitespace() {
            i += 1;
        }
        if i >= b.len() || b[i] == b'#' {
            break;
        }
        let k = i;
        while i < b.len() && b[i] != b'=' && !b[i].is_ascii_whitespace() {
            i += 1;
        }
        if i >= b.len() || b[i] != b'=' || i == k {
            return None;
        }
        let key = &s[k..i];
        i += 1;
        if b.get(i) == Some(&b'"') {
            i += 1;
            loop {
                match b.get(i)? {
                    b'"' => break,
                    b'\\' => i += 2,
                    _ => i += 1,
                }
            }
            i += 1;
        } else {
            while i < b.len() && !b[i].is_ascii_whitespace() {
                i += 1;
            }
        }
        teile.push((key, s.get(start..i)?));
    }
    Some((abschnitt, teile))
}

/// Woran ein Satz in der neuen Datei wiedererkannt wird.
fn kennung(abschnitt: &str, teile: &[(&str, &str)]) -> Option<String> {
    let (_, bekannt, id) = BEKANNT.iter().find(|b| b.0 == abschnitt)?;
    let mut k = abschnitt.to_string();
    for (key, roh) in teile {
        let nimm = match id {
            None => false,
            Some("") => bekannt.contains(key),
            Some(id) => key == id,
        };
        if nimm {
            k.push_str(if *id == Some("") { key } else { roh });
        }
    }
    Some(k)
}

/// Das Fremde einer gelesenen Datei.
#[derive(Default, Debug)]
pub struct Fremd {
    /// Zeilen unbekannter Abschnitte und unbekannter Sätze bekannter
    /// Abschnitte (eine Farbrolle aus einer neueren Fassung, ein Satz nur
    /// mit unbekannten Schlüsseln), im Wortlaut.
    zeilen: Vec<String>,
    /// Zeilen mit unbekannten Schlüsseln: Wortlaut, ihr bekannter Teil,
    /// ihre Kennung und die unbekannten Schlüssel.
    schluessel: Vec<(String, String, String, String)>,
}

impl Fremd {
    /// Sammelt das Fremde aus `text`. `satz_bekannt(abschnitt, zeile)`
    /// sagt, ob ein Satz eines bekannten Abschnitts gelesen wird (eine
    /// unbekannte Farbrolle nicht).
    pub fn sammeln(text: &str, satz_bekannt: impl Fn(&str, &str) -> bool) -> Fremd {
        let mut f = Fremd::default();
        for zeile in text.lines().skip(1) {
            let Some((abschnitt, teile)) = zerlegen(zeile) else {
                continue;
            };
            let Some((_, bekannt, id)) = BEKANNT.iter().find(|b| b.0 == abschnitt) else {
                f.zeilen.push(zeile.to_string());
                continue;
            };
            // Ein Satz ohne einen bekannten Schlüssel in einem Abschnitt, der
            // mehrfach steht ([env] einer neueren Fassung), ist ganz fremd;
            // ein einmaliger behält seine Stelle wie jeder andere
            let ohne_bekannte = id.is_some()
                && !teile.is_empty()
                && teile.iter().all(|(k, _)| !bekannt.contains(k));
            if ohne_bekannte || !satz_bekannt(abschnitt, zeile) {
                f.zeilen.push(zeile.to_string());
                continue;
            }
            let fremde: String = teile
                .iter()
                .filter(|(k, _)| !bekannt.contains(k))
                .map(|(_, roh)| *roh)
                .collect();
            if fremde.is_empty() {
                continue;
            }
            let mut eigen = format!("[{abschnitt}]");
            for (k, roh) in &teile {
                if bekannt.contains(k) {
                    eigen.push(' ');
                    eigen.push_str(roh.trim_start());
                }
            }
            let id = kennung(abschnitt, &teile).unwrap_or_default();
            f.schluessel.push((zeile.to_string(), eigen, id, fremde));
        }
        f
    }

    /// Setzt das Fremde in den neu geschriebenen Text `neu`.
    pub fn einsetzen(&self, neu: &str) -> String {
        let mut zeilen: Vec<String> = neu.lines().map(String::from).collect();
        // Schon im Wortlaut da (Abschnitte, die die App roh weiterreicht)
        let mut fertig = vec![false; zeilen.len()];
        for (wortlaut, eigen, id, fremde) in &self.schluessel {
            let frei = |z: &[String], fertig: &[bool], p: &dyn Fn(&str) -> bool| {
                (0..z.len()).find(|&i| !fertig[i] && p(&z[i]))
            };
            if let Some(i) = frei(&zeilen, &fertig, &|z| z == wortlaut) {
                fertig[i] = true;
            } else if let Some(i) = frei(&zeilen, &fertig, &|z| z == eigen) {
                // Unverändert: im Wortlaut
                zeilen[i] = wortlaut.clone();
                fertig[i] = true;
            } else if let Some(i) = frei(&zeilen, &fertig, &|z| {
                zerlegen(z).and_then(|(a, t)| kennung(a, &t)).as_ref() == Some(id)
            }) {
                // Geändert: die neue Zeile mit den unbekannten Schlüsseln
                zeilen[i].push_str(fremde);
                fertig[i] = true;
            } else if BEKANNT.iter().any(|b| b.0 == id && b.2.is_none()) {
                // Einmaliger Abschnitt ohne Zeile (z. B. beide Häkchen aus):
                // nur die unbekannten Schlüssel
                zeilen.push(format!("[{id}]{fremde}"));
                fertig.push(true);
            }
            // Sonst gibt es den Satz nicht mehr (z. B. Farbe wieder wie im
            // Grundschema); mit ihm entfallen seine Schlüssel
        }
        let mut out = String::with_capacity(neu.len());
        for z in zeilen.iter().chain(&self.zeilen) {
            out.push_str(z);
            out.push('\n');
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zerlegen_haelt_den_wortlaut() {
        let (a, t) = zerlegen(r#"[planung] name="A \"B\" C" anschrift="x\ny"  neu=3"#).unwrap();
        assert_eq!(a, "planung");
        assert_eq!(
            t,
            [
                ("name", r#" name="A \"B\" C""#),
                ("anschrift", r#" anschrift="x\ny""#),
                ("neu", "  neu=3"),
            ]
        );
        assert!(zerlegen("# Kommentar").is_none());
        assert!(zerlegen("[x] kaputt").is_none());
    }
}
