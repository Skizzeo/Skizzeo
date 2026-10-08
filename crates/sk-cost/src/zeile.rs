//! Eine Dateizeile zerlegen und schreiben, mit derselben Schreibweise wie
//! `sk_model::szo` (Texte in Anführungszeichen mit `\"`, `\\` und `\n`).
//! Anders als `szo::Record` behält die Zerlegung die Reihenfolge der
//! Schlüssel, damit fremde Schlüssel beim Neuschreiben an ihrem Platz hinter
//! den eigenen bleiben (Bausteingrenze §4.1).

/// Abschnitt und Schlüssel einer Zeile in Dateireihenfolge.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Zeile {
    pub abschnitt: String,
    pub paare: Vec<(String, String)>,
}

/// Zerlegt eine Zeile; `None` für leere Zeilen, Kommentare und Zeilen, die
/// keine Datensätze sind.
pub fn zerlegen(text: &str) -> Option<Zeile> {
    let t = text.trim();
    if t.is_empty() || t.starts_with('#') {
        return None;
    }
    let rest = t.strip_prefix('[')?;
    let close = rest.find(']')?;
    let abschnitt = rest[..close].trim().to_string();
    let mut paare = Vec::new();
    let mut chars = rest[close + 1..].chars().peekable();
    loop {
        while chars.peek().is_some_and(|c| c.is_whitespace()) {
            chars.next();
        }
        let Some(&c) = chars.peek() else { break };
        if c == '#' {
            break;
        }
        let mut key = String::new();
        while let Some(&c) = chars.peek() {
            if c == '=' || c.is_whitespace() {
                break;
            }
            key.push(c);
            chars.next();
        }
        if chars.next() != Some('=') || key.is_empty() {
            return None;
        }
        let mut value = String::new();
        if chars.peek() == Some(&'"') {
            chars.next();
            loop {
                match chars.next()? {
                    '"' => break,
                    '\\' => match chars.next()? {
                        'n' => value.push('\n'),
                        c @ ('"' | '\\') => value.push(c),
                        _ => return None,
                    },
                    c => value.push(c),
                }
            }
        } else {
            while let Some(&c) = chars.peek() {
                if c.is_whitespace() {
                    break;
                }
                value.push(c);
                chars.next();
            }
        }
        paare.push((key, value));
    }
    Some(Zeile { abschnitt, paare })
}

/// Text in Anführungszeichen, wie `szo::Line::text`.
pub fn text(v: &str) -> String {
    let mut q = String::with_capacity(v.len() + 2);
    q.push('"');
    for c in v.chars() {
        match c {
            '"' => q.push_str("\\\""),
            '\\' => q.push_str("\\\\"),
            '\n' => q.push_str("\\n"),
            c => q.push(c),
        }
    }
    q.push('"');
    q
}

/// Muss der Wert in Anführungszeichen stehen, damit er wieder so gelesen
/// wird? (fremde Schlüssel beim Neuschreiben)
pub fn braucht_text(v: &str) -> bool {
    v.is_empty()
        || v.chars()
            .any(|c| c.is_whitespace() || c == '"' || c == '\\' || c == '#')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zerlegen_wie_szo() {
        let z = zerlegen("[article] guid=1 name=\"a \\\"b\\\"\\nc\" t=115 # Rest").unwrap();
        assert_eq!(z.abschnitt, "article");
        assert_eq!(
            z.paare,
            vec![
                ("guid".into(), "1".into()),
                ("name".into(), "a \"b\"\nc".into()),
                ("t".into(), "115".into()),
            ]
        );
        assert_eq!(text("a \"b\"\nc"), "\"a \\\"b\\\"\\nc\"");
        assert!(zerlegen("# Kommentar").is_none());
        assert!(zerlegen("").is_none());
        assert!(zerlegen("[a] x").is_none());
        assert!(zerlegen("[a] x=\"offen").is_none());
    }
}
