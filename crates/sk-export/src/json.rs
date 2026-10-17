//! Kleiner JSON-Schreiber für den glTF-Kopf: Zeichenketten mit Escape,
//! Zahlen ohne Exponent-Überraschung, Listen.

/// Zeichenkette in Anführungszeichen (RFC 8259, Steuerzeichen als `\u`).
pub fn text(s: &str) -> String {
    let mut o = String::with_capacity(s.len() + 2);
    o.push('"');
    for c in s.chars() {
        match c {
            '"' => o.push_str("\\\""),
            '\\' => o.push_str("\\\\"),
            '\n' => o.push_str("\\n"),
            '\r' => o.push_str("\\r"),
            '\t' => o.push_str("\\t"),
            c if (c as u32) < 0x20 => o.push_str(&format!("\\u{:04x}", c as u32)),
            c => o.push(c),
        }
    }
    o.push('"');
    o
}

/// Zahl; nicht endliche Werte werden 0 (JSON kennt sie nicht).
pub fn zahl(v: f64) -> String {
    if v.is_finite() {
        // `{}` schreibt die kürzeste Form, die genau zurückliest
        let s = format!("{v}");
        if s == "-0" {
            "0".into()
        } else {
            s
        }
    } else {
        "0".into()
    }
}

/// Liste aus fertigen Werten.
pub fn liste<I: IntoIterator<Item = String>>(v: I) -> String {
    liste_mit('[', ']', v.into_iter())
}

/// Objekt aus Schlüsseln und fertigen Werten.
pub fn objekt<'a, I: IntoIterator<Item = (&'a str, String)>>(v: I) -> String {
    liste_mit(
        '{',
        '}',
        v.into_iter().map(|(k, x)| format!("{}:{x}", text(k))),
    )
}

fn liste_mit(auf: char, zu: char, v: impl Iterator<Item = String>) -> String {
    let mut o = String::new();
    o.push(auf);
    for (i, x) in v.enumerate() {
        if i > 0 {
            o.push(',');
        }
        o.push_str(&x);
    }
    o.push(zu);
    o
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escape_und_zahlen() {
        assert_eq!(text("Außenwände \"AW\"\n"), "\"Außenwände \\\"AW\\\"\\n\"");
        assert_eq!(text("\u{1}"), "\"\\u0001\"");
        assert_eq!(zahl(1.0), "1");
        assert_eq!(zahl(-0.0), "0");
        assert_eq!(zahl(f64::NAN), "0");
        assert_eq!(zahl(0.25), "0.25");
        assert_eq!(
            objekt([("a", zahl(1.0)), ("b", liste([text("x")]))]),
            "{\"a\":1,\"b\":[\"x\"]}"
        );
    }
}
