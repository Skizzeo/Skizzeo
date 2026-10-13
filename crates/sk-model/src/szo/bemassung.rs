//! Bemaßung in der `.szo` ([`crate::bemassung`], Jörn 12.10.):
//!
//! - `[bemassung] key=project linie=0.25 symbol=schraeg …` mit dem, was vom
//!   Werksstil abweicht; ohne Abweichung keine Zeile.
//! - `[massversatz] storey=<guid> seite=270 kette=wand versatz=250` je von
//!   Hand verschobener Kette.
//! - `[massansicht] view=plan zeigen=1 bezug=1` je Ansicht, deren Wahl am
//!   Zahnrad von der Vorgabe abweicht (Ansichtszustand).
//!
//! Alte Dateien bleiben bytegleich. Eine unlesbare Zeile gibt einen Hinweis
//! und bleibt roh stehen (Regel 72); eine doppelte oder eine mit
//! unbekanntem Geschoss gibt einen Hinweis und entfällt (wie `[space]`,
//! Regel 115). Ein ungültiger Wert im Stil lässt die Vorgabe gelten und
//! entfällt mit Hinweis (Regel 114), die übrigen Werte gelten; so bleibt
//! „Auf Standard zurücksetzen“ auch nach dem nächsten Öffnen (Review W30 B3).

use super::{Line, Record};
use crate::bemassung::{
    Aussenbezug, Bemassung, BrhBezug, Kettenart, MassSicht, Stil, Symbol, Zahl, ANSICHTEN,
};
use crate::guid::Guid;
use crate::view::SICHT_VIEWS;
use crate::Model;
use std::collections::btree_map::Entry;

/// Abschnitte der Bemaßung.
pub(super) const SECTIONS: [&str; 3] = ["bemassung", "massversatz", "massansicht"];

/// Der Abschnitt `s`, wenn er zur Bemaßung gehört.
pub(super) fn known(s: &str) -> Option<&'static str> {
    SECTIONS.iter().find(|k| **k == s).copied()
}

const KETTE_KEYS: [&str; 4] = [
    "kette_oeffnung",
    "kette_wand",
    "kette_sprung",
    "kette_gesamt",
];

pub(super) fn write(m: &Model, out: &mut String) {
    let b = m.bemassung();
    if let Some(l) = stil_line(&b.stil) {
        l.finish(out);
    }
    for ((g, seite, art), v) in &b.versatz {
        Line::new("massversatz")
            .guid("storey", Some(*g))
            .num("seite", seite)
            .word("kette", art.word())
            .num("versatz", v)
            .finish(out);
    }
    let s = m.mass_sicht_alle();
    for (i, name) in SICHT_VIEWS.iter().enumerate().take(ANSICHTEN) {
        if s.zeigen[i] != MassSicht::VORGABE[i] || s.bezug[i] {
            Line::new("massansicht")
                .word("view", name)
                .flag("zeigen", s.zeigen[i])
                .flag("bezug", s.bezug[i])
                .finish(out);
        }
    }
    for raw in &s.raw {
        out.push_str(raw);
        out.push('\n');
    }
}

/// Zeile mit den Abweichungen vom Werksstil; `None` ohne Abweichung.
fn stil_line(s: &Stil) -> Option<Line> {
    stil_line_in("bemassung", "project", s)
}

/// Der Stil als Firmenstandard des Arbeitsplatzes (`einstellungen.txt`):
/// `[massstil] key=firma …` mit den Abweichungen vom Werk; leer ohne
/// Abweichung.
pub fn firmenstil_text(s: &Stil) -> String {
    let mut out = String::new();
    if let Some(l) = stil_line_in("massstil", "firma", s) {
        l.finish(&mut out);
    }
    out
}

/// Firmenstandard aus einer Zeile `[massstil] …`; ungültige Werte lassen
/// das Werk gelten. `None`, wenn die Zeile keine solche ist.
pub fn firmenstil_lesen(zeile: &str) -> Option<Stil> {
    let r = Record::parse(1, zeile).ok().flatten()?;
    (r.section == "massstil").then(|| read_stil(&r).0)
}

fn stil_line_in(section: &str, key: &str, s: &Stil) -> Option<Line> {
    let w = Stil::werk();
    if *s == w {
        return None;
    }
    let mut l = Line::new(section).word("key", key);
    if s.farbe != w.farbe {
        l = l.color("farbe", s.farbe);
    }
    for z in Zahl::ALL {
        if s.zahl(z) != w.zahl(z) {
            l = l.num(z.key(), s.zahl(z));
        }
    }
    if s.symbol != w.symbol {
        l = l.word("symbol", s.symbol.word());
    }
    if s.schrift != w.schrift {
        l = l.text("schrift", &s.schrift);
    }
    if s.rundung != w.rundung {
        l = l.num("rundung", s.rundung);
    }
    if s.bezug != w.bezug {
        l = l.word("bezug", s.bezug.word());
    }
    for (k, key) in Kettenart::ALL.iter().zip(KETTE_KEYS) {
        if s.kette(*k) != w.kette(*k) {
            l = l.flag(key, s.kette(*k));
        }
    }
    if s.hoehe != w.hoehe {
        l = l.flag("hoehe", s.hoehe);
    }
    if s.brh != w.brh {
        l = l.flag("brh", s.brh);
    }
    if s.brh_bezug != w.brh_bezug {
        l = l.word("brh_bezug", s.brh_bezug.word());
    }
    Some(l)
}

/// Liest die drei Abschnitte; `storeys` ordnet Geschoss-Guids zu.
pub(super) fn read<'a>(
    recs: &impl Fn(&str) -> &'a Vec<Record>,
    lines: &[&str],
    storeys: &dyn Fn(Guid) -> bool,
    hints: &mut Vec<String>,
) -> (Bemassung, MassSicht) {
    let mut b = Bemassung::default();
    let mut sicht = MassSicht::default();
    let roh = |r: &Record, warum: &str, hints: &mut Vec<String>, raw: &mut Vec<String>| {
        r.skip();
        raw.push(lines[r.line - 1].to_string());
        hints.push(format!(
            "Zeile {}: [{}] {warum}; die Zeile bleibt stehen",
            r.line, r.section
        ));
    };
    let weg = |r: &Record, warum: &str, hints: &mut Vec<String>| {
        r.skip();
        hints.push(format!(
            "Zeile {}: [{}] {warum}; die Zeile entfällt",
            r.line, r.section
        ));
    };
    let mut raw = Vec::new();
    for (i, r) in recs("bemassung").iter().enumerate() {
        if i > 0 {
            weg(r, "doppelt, es gilt die erste", hints);
            continue;
        }
        let (stil, falsch) = read_stil(r);
        b.stil = stil;
        if !falsch.is_empty() {
            hints.push(format!(
                "Zeile {}: [bemassung] {} ungültig, es gilt die Vorgabe",
                r.line,
                falsch.join(", ")
            ));
        }
    }
    for r in recs("massversatz") {
        let g = r.opt("storey").and_then(Guid::from_ifc);
        let seite = r.opt("seite").and_then(|s| s.parse::<i32>().ok());
        let art = r
            .opt("kette")
            .and_then(|w| Kettenart::ALL.into_iter().find(|k| k.word() == w));
        let v = r
            .opt("versatz")
            .and_then(|s| s.parse::<f64>().ok())
            .filter(|v| v.is_finite() && v.abs() <= 100_000.0 && *v != 0.0);
        // Erst lesbar, dann bekannt: Unlesbares bleibt roh stehen
        // (Regel 72), ein lesbarer Eintrag für ein fehlendes Geschoss
        // entfällt (Regel 115).
        let (Some(g), Some(s), Some(a), Some(v)) = (g, seite, art, v) else {
            roh(r, "unlesbar", hints, &mut raw);
            continue;
        };
        if !storeys(g) {
            weg(r, "Geschoss gibt es nicht", hints);
        } else if let Entry::Vacant(e) = b.versatz.entry((g, s.rem_euclid(360), a)) {
            e.insert(v);
        } else {
            weg(r, "doppelt, es gilt die erste", hints);
        }
    }
    let mut gesehen = [false; ANSICHTEN];
    for r in recs("massansicht") {
        let view = r
            .opt("view")
            .and_then(|w| SICHT_VIEWS.iter().position(|n| *n == w))
            .filter(|i| *i < ANSICHTEN);
        let flag = |k: &str| match r.opt(k) {
            Some("0") => Some(false),
            Some("1") => Some(true),
            _ => None,
        };
        match (view, flag("zeigen"), flag("bezug")) {
            (Some(i), Some(_), Some(_)) if gesehen[i] => {
                weg(r, "doppelt, es gilt die erste", hints)
            }
            (Some(i), Some(z), Some(bz)) => {
                gesehen[i] = true;
                sicht.zeigen[i] = z;
                sicht.bezug[i] = bz;
            }
            _ => roh(r, "unlesbar", hints, &mut raw),
        }
    }
    sicht.raw = raw;
    (b, sicht)
}

/// Stil aus `[bemassung]` und die Schlüssel mit ungültigem Wert.
fn read_stil(r: &Record) -> (Stil, Vec<String>) {
    let _ = r.opt("key");
    let mut s = Stil::werk();
    let mut falsch = Vec::new();
    if let Some(v) = r.opt("farbe") {
        match super::parse_hex(v) {
            Some(c) => s.farbe = c,
            None => falsch.push("farbe".to_string()),
        }
    }
    for z in Zahl::ALL {
        if let Some(v) = r.opt(z.key()) {
            if !v.parse::<f64>().is_ok_and(|v| s.set_zahl(z, v)) {
                falsch.push(z.key().to_string());
            }
        }
    }
    let wort = |k: &str, falsch: &mut Vec<String>| -> Option<&str> {
        let v = r.opt(k)?;
        falsch.push(k.to_string());
        Some(v)
    };
    if let Some(v) = wort("symbol", &mut falsch) {
        if let Some(x) = Symbol::ALL.into_iter().find(|x| x.word() == v) {
            s.symbol = x;
            falsch.pop();
        }
    }
    if let Some(v) = wort("schrift", &mut falsch) {
        if !v.trim().is_empty() && !v.contains(['"', '\n', '\\']) {
            s.schrift = v.to_string();
            falsch.pop();
        }
    }
    if let Some(v) = wort("rundung", &mut falsch) {
        if let Some(x) = v.parse::<u8>().ok().filter(|x| matches!(x, 1 | 5)) {
            s.rundung = x;
            falsch.pop();
        }
    }
    if let Some(v) = wort("bezug", &mut falsch) {
        if let Some(x) = Aussenbezug::ALL.into_iter().find(|x| x.word() == v) {
            s.bezug = x;
            falsch.pop();
        }
    }
    if let Some(v) = wort("brh_bezug", &mut falsch) {
        if let Some(x) = BrhBezug::ALL.into_iter().find(|x| x.word() == v) {
            s.brh_bezug = x;
            falsch.pop();
        }
    }
    let mut flag = |k: &str, f: &mut dyn FnMut(bool)| {
        if let Some(v) = r.opt(k) {
            match v {
                "0" => f(false),
                "1" => f(true),
                _ => falsch.push(k.to_string()),
            }
        }
    };
    for (k, key) in Kettenart::ALL.iter().zip(KETTE_KEYS) {
        flag(key, &mut |b| s.set_kette(*k, b));
    }
    flag("hoehe", &mut |b| s.hoehe = b);
    flag("brh", &mut |b| s.brh = b);
    (s, falsch)
}

#[cfg(test)]
mod tests {
    #[test]
    fn firmenstil_im_rundlauf() {
        use crate::bemassung::{Stil, Symbol};
        assert_eq!(super::firmenstil_text(&Stil::werk()), "");
        let mut s = Stil::werk();
        s.symbol = Symbol::Schraeg;
        s.schrift = "Arial".into();
        s.erste = 2500.0;
        let t = super::firmenstil_text(&s);
        assert!(t.starts_with("[massstil] key=firma"), "{t}");
        assert_eq!(super::firmenstil_lesen(t.trim_end()), Some(s));
        assert_eq!(super::firmenstil_lesen("[bemassung] key=project"), None);
    }

    use super::super::tests::house;
    use super::super::{read, write};
    use crate::bemassung::{Kettenart, Symbol};
    use crate::GuidGen;

    fn load(t: &str) -> super::super::Loaded {
        read(t, GuidGen::with_seed(99)).unwrap()
    }

    /// Ohne eigenen Stil, Versatz und Wahl keine Zeile; mit allem ein
    /// Rundlauf bytegleich, ohne Hinweis, gleiche Werte.
    #[test]
    fn rundlauf_bytegleich() {
        let mut m = house();
        let alt = write(&m);
        assert!(!alt.contains("[bemassung]") && !alt.contains("[mass"));
        let st = m.storeys().iter().map(|s| s.0).last().unwrap();
        let mut s = m.massstil().clone();
        s.symbol = Symbol::Schraeg;
        s.schrift = "Arial Narrow".into();
        s.linie = 0.25;
        s.rundung = 5;
        s.set_kette(Kettenart::Sprung, false);
        s.farbe = [10, 20, 30];
        assert!(m.set_massstil(s.clone()));
        assert!(m.set_massversatz(st, 270, Kettenart::Gesamt, -350.0));
        m.set_mass_sicht(1, true, true);
        m.set_mass_sicht(0, false, false);
        let neu = write(&m);
        let zeilen: Vec<&str> = neu
            .lines()
            .filter(|z| z.starts_with("[bemassung]") || z.starts_with("[mass"))
            .collect();
        assert_eq!(zeilen.len(), 4, "{zeilen:?}");
        assert!(
            zeilen[0].contains("symbol=schraeg") && zeilen[0].contains("schrift=\"Arial Narrow\"")
        );
        assert!(zeilen[0].contains("linie=0.25") && !zeilen[0].contains("hilfslinie="));
        let back = load(&neu);
        assert!(back.hints.is_empty(), "{:?}", back.hints);
        assert_eq!(*back.model.massstil(), s);
        let g = m.storey(st).unwrap().guid;
        let st2 = back
            .model
            .storeys()
            .iter()
            .find(|s| s.1.guid == g)
            .unwrap()
            .0;
        assert_eq!(back.model.massversatz(st2, 270, Kettenart::Gesamt), -350.0);
        assert_eq!(back.model.mass_sicht(1), (true, true));
        assert_eq!(back.model.mass_sicht(0), (false, false));
        assert_eq!(write(&back.model), neu);
    }

    /// Unlesbares bleibt roh mit Hinweis, der Rundlauf bleibt bytegleich.
    #[test]
    fn unlesbares_bleibt_stehen() {
        let alt = write(&house());
        for roh in [
            "[massversatz] storey=0000000000000000000000 seite=nord kette=wand versatz=10",
            "[massversatz] storey=0000000000000000000000 seite=0 kette=wand",
            "[massansicht] view=oben zeigen=1 bezug=1",
            "[massansicht] view=plan zeigen=2 bezug=1",
        ] {
            let text = alt.replacen("[storey]", &format!("{roh}\n[storey]"), 1);
            let l = load(&text);
            assert_eq!(l.hints.len(), 1, "{roh}: {:?}", l.hints);
            assert!(l.hints[0].contains("bleibt stehen"), "{:?}", l.hints);
            let w = write(&l.model);
            assert!(w.lines().any(|z| z == roh), "{roh}: roh");
            assert_eq!(write(&load(&w).model), w, "{roh}");
        }
    }

    /// Unbekanntes Geschoss und Doppeltes entfallen mit Hinweis (Regel 115);
    /// im Stil gelten die gültigen Werte, ein ungültiger entfällt, und nach
    /// „Auf Standard zurücksetzen“ bleibt beim nächsten Öffnen das Werk
    /// (Review W30 B3).
    #[test]
    fn unbekanntes_und_doppeltes_entfallen() {
        let m = house();
        let alt = write(&m);
        let g = m.storeys().iter().next().unwrap().1.guid.to_ifc();
        let versatz = format!("[massversatz] storey={g} seite=270 kette=wand versatz=10");
        for (zeilen, rest) in [
            (
                vec!["[massversatz] storey=0000000000000000000000 seite=0 kette=wand versatz=10"],
                0,
            ),
            (vec![versatz.as_str(), versatz.as_str()], 1),
            (
                vec![
                    "[massansicht] view=plan zeigen=1 bezug=1",
                    "[massansicht] view=plan zeigen=0 bezug=0",
                ],
                1,
            ),
            (
                vec![
                    "[bemassung] key=project symbol=schraeg",
                    "[bemassung] key=project symbol=punkt",
                ],
                1,
            ),
        ] {
            let text = alt.replacen("[storey]", &format!("{}\n[storey]", zeilen.join("\n")), 1);
            let l = load(&text);
            assert_eq!(l.hints.len(), 1, "{zeilen:?}: {:?}", l.hints);
            assert!(l.hints[0].contains("entfällt"), "{:?}", l.hints);
            let w = write(&l.model);
            let n = w
                .lines()
                .filter(|z| z.starts_with("[bemassung]") || z.starts_with("[mass"))
                .count();
            assert_eq!(n, rest, "{zeilen:?}: {w}");
            assert!(load(&w).hints.is_empty());
        }
        // Teils ungültiger Stil: das Gültige gilt, das Ungültige entfällt
        let text = alt.replacen(
            "[storey]",
            "[bemassung] key=project linie=9 symbol=schraeg\n[storey]",
            1,
        );
        let mut l = load(&text);
        assert_eq!(l.hints.len(), 1, "{:?}", l.hints);
        assert_eq!(l.model.massstil().symbol, Symbol::Schraeg);
        assert_eq!(l.model.massstil().linie, 0.18);
        let w = write(&l.model);
        assert!(
            w.contains("symbol=schraeg") && !w.contains("linie=9"),
            "{w}"
        );
        // Zurück auf Werk: beim nächsten Öffnen keine Stilzeile mehr
        l.model.begin("Werk");
        assert!(l.model.set_massstil(crate::bemassung::Stil::werk()));
        l.model.commit();
        let w = write(&l.model);
        assert!(!w.contains("[bemassung]"), "{w}");
        assert_eq!(*load(&w).model.massstil(), crate::bemassung::Stil::werk());
    }
}
