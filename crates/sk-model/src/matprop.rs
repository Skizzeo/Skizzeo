//! Kennwerte der Baustoffe (Paket 5, BIM-Regeln 50–53): feste Schlüssel
//! mit Art und Prüfung, Richtpreis mit Einheit nach Baustoffart.

use crate::element::{PropSet, PropValue};
use crate::guid::Guid;
use crate::library::MatCategory;
use crate::szo::{Line, Record};

/// Art eines festen Kennwerts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MatPropKind {
    Number,
    Text,
}

/// Bereich eines festen Kennwerts: Mengen oder Kosten (Review #120 H5);
/// ohne Bereich ist er allgemein. Was daraus folgt, entscheidet die App.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MatPropModule {
    Quantities,
    Costs,
}

/// Fester Kennwert: Schlüssel, Einheit zur Anzeige, Art, Stufe (Grundstufe
/// oder „Mehr“), Zusatzmodul.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MatProp {
    pub key: &'static str,
    pub unit: &'static str,
    pub kind: MatPropKind,
    pub basic: bool,
    pub module: Option<MatPropModule>,
}

pub const PRICE: &str = "Richtpreis";
pub const PRICE_UNIT: &str = "Preiseinheit";
pub const PRICE_DATE: &str = "Preisstand";
pub const SUBGROUP: &str = "Untergruppe";
pub const MU: &str = "\u{3bc}";
pub const EUROCLASS: &str = "Euroklasse";
pub const MAKER: &str = "Hersteller";
pub const PRICE_SOURCE: &str = "Preisquelle";
/// Verschnitt der Bestellmenge in % (Mengenpanel „Bestellung“, vob/plan-
/// rechenweg-bestellmengen.md); ohne Wert gilt [`waste_default`]. War
/// vorher ein freier eigener Kennwert: Altwerte als Text („5 %“) bleiben
/// roh erhalten und an ihrer Stelle unter den eigenen (Review #77/4).
pub const WASTE: &str = "Verschnittzuschlag";
/// Öffnungen ab dieser Einzelgröße (m²) von der Bestellmenge abziehen;
/// ohne Wert werden alle übermessen (Jörn 11.10.). Wie [`WASTE`] vorher
/// ein freier eigener Kennwert: Altwerte bleiben roh erhalten.
pub const OPENING_FROM: &str = "Öffnungen abziehen ab";
/// Feste Kennwerte, die vorher freie eigene waren: Sie stehen in der Datei
/// weiter unter den eigenen, und ein Altwert, der die Prüfung nicht
/// besteht, bleibt roh erhalten (Review #77/4).
const FRUEHER_FREI: [&str; 2] = [WASTE, OPENING_FROM];

const fn prop(
    key: &'static str,
    unit: &'static str,
    kind: MatPropKind,
    basic: bool,
    module: Option<MatPropModule>,
) -> MatProp {
    MatProp {
        key,
        unit,
        kind,
        basic,
        module,
    }
}

/// Feste Kennwerte in Anzeige- und Dateireihenfolge. Die Einheit des
/// Richtpreises steht am Baustoff ([`PRICE_UNIT`]); λ und Rohdichte sind
/// Felder des Baustoffs. Preis, Einheit, Stand und Quelle gehören zu Kosten,
/// Verschnitt und Öffnungen zu Mengen (Review #120 B2).
pub const MAT_PROPS: [MatProp; 13] = {
    use MatPropKind::{Number as N, Text as T};
    const K: Option<MatPropModule> = Some(MatPropModule::Costs);
    const M: Option<MatPropModule> = Some(MatPropModule::Quantities);
    [
        prop(PRICE, "", N, true, K),
        prop(PRICE_UNIT, "", T, true, K),
        prop(PRICE_DATE, "", T, true, K),
        prop(SUBGROUP, "", T, false, None),
        prop(MU, "", N, false, None),
        prop("c", "J/(kg\u{b7}K)", N, false, None),
        prop(EUROCLASS, "", T, false, None),
        prop(MAKER, "", T, false, None),
        prop("Produkt", "", T, false, None),
        prop("Bemerkung", "", T, false, None),
        prop(PRICE_SOURCE, "", T, false, K),
        prop(WASTE, "%", N, false, M),
        prop(OPENING_FROM, "m\u{b2}", N, false, M),
    ]
};

/// Grundklassen nach DIN EN 13501-1, wie die Auswahl sie zeigt.
pub const EUROCLASSES: [&str; 7] = ["A1", "A2", "B", "C", "D", "E", "F"];

/// Einheiten des Richtpreises in der Datei (`unit=`).
pub const PRICE_UNITS: [&str; 4] = ["m3", "m2", "m", "t"];

/// Fester Kennwert zum Schlüssel (nach [`normalize_key`]).
pub fn mat_prop(key: &str) -> Option<&'static MatProp> {
    MAT_PROPS.iter().find(|p| p.key == key)
}

/// Schlüssel, wie er gespeichert wird: „µ“ (U+00B5, AltGr+M) wird zu „μ“
/// (U+03BC), sonst gäbe es zwei Schlüssel (Regel 50).
pub fn normalize_key(key: &str) -> String {
    key.trim().replace('\u{b5}', MU)
}

/// Einheit des Richtpreises nach Baustoffart (§2.2); Luft hat keinen Preis.
pub fn price_unit(c: MatCategory) -> Option<&'static str> {
    match c {
        MatCategory::Masonry
        | MatCategory::Concrete
        | MatCategory::Insulation
        | MatCategory::Timber => Some("m3"),
        MatCategory::Plaster | MatCategory::Metal | MatCategory::Membrane => Some("m2"),
        // Ausbau des Raumbuchs: nach Fläche bei seiner Dicke
        MatCategory::Screed
        | MatCategory::Ceramic
        | MatCategory::Flooring
        | MatCategory::Coating => Some("m2"),
        MatCategory::Air => None,
    }
}

/// Verschnitt der Bestellmenge in %, wenn der Baustoff keinen nennt:
/// Spannen aus Händler-Rechnern, Herstellerangaben und Kalkulationslehre
/// (vob/plan-rechenweg-bestellmengen.md); keine Norm.
pub fn waste_default(c: MatCategory) -> f64 {
    match c {
        MatCategory::Masonry | MatCategory::Concrete | MatCategory::Insulation => 5.0,
        MatCategory::Plaster | MatCategory::Timber | MatCategory::Metal => 10.0,
        MatCategory::Membrane => 15.0,
        // Raumbuch (#80, Kosten H80-1): Estrich wie Beton, Fliesen,
        // Beläge und Beschichtung mit Zuschnitt
        MatCategory::Screed => 5.0,
        MatCategory::Ceramic | MatCategory::Flooring | MatCategory::Coating => 10.0,
        MatCategory::Air => 0.0,
    }
}

/// Verschnitt der Bestellmenge in % für diesen Baustoff; ein Altwert als
/// Text zählt, wenn er sich als Zahl lesen lässt („5 %“, „2,5“).
pub fn waste(m: &crate::library::Material) -> f64 {
    match m.props.get(WASTE) {
        Some(PropValue::Number(n)) if (0.0..=100.0).contains(n) => *n,
        Some(PropValue::Text(t)) => waste_text(t).unwrap_or(waste_default(m.category)),
        _ => waste_default(m.category),
    }
}

/// Verschnitt aus freiem Text: „5 %“, „5,5%“, „7“; nur 0 bis 100.
fn waste_text(t: &str) -> Option<f64> {
    let n = t.trim().trim_end_matches('%').trim().replace(',', ".");
    n.parse::<f64>().ok().filter(|n| (0.0..=100.0).contains(n))
}

/// Einzelgröße (mm²), ab der Öffnungen von der Bestellmenge abgezogen
/// werden; `None`: alle übermessen. Ein Altwert als Text zählt, wenn er
/// sich lesen lässt („2,5 m²“).
pub fn opening_from(m: &crate::library::Material) -> Option<f64> {
    let m2 = match m.props.get(OPENING_FROM)? {
        PropValue::Number(n) => Some(*n),
        PropValue::Text(t) => {
            let n = t.trim().trim_end_matches("m²").trim().replace(',', ".");
            n.parse::<f64>().ok()
        }
        PropValue::Bool(_) => None,
    };
    m2.filter(|n| *n >= 0.0).map(|n| n * 1e6)
}

/// Anzeige einer Preiseinheit: „m3“ → „€/m³“.
pub fn price_unit_label(unit: &str) -> &'static str {
    match unit {
        "m3" => "\u{20ac}/m\u{b3}",
        "m2" => "\u{20ac}/m\u{b2}",
        "m" => "\u{20ac}/m",
        "t" => "\u{20ac}/t",
        _ => "\u{20ac}",
    }
}

/// Preisstand MM/JJJJ (Regel 51).
pub fn valid_price_date(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() == 7
        && b[2] == b'/'
        && b.iter()
            .enumerate()
            .all(|(i, c)| i == 2 || c.is_ascii_digit())
        && matches!(s[..2].parse::<u8>(), Ok(1..=12))
}

/// Euroklasse nach DIN EN 13501-1 (Regel 52): Grundklasse A1 … F, bei
/// Bodenbelägen mit „fl“; für A2 bis D ein Zusatz „-s1“ … „-s3“ (Bodenbeläge
/// „-s1“, „-s2“) und danach „,d0“ … „,d2“ (nicht bei Bodenbelägen).
pub fn valid_euroclass(s: &str) -> bool {
    let Some(base) = ["A1", "A2", "B", "C", "D", "E", "F"]
        .into_iter()
        .find(|b| s.starts_with(b))
    else {
        return false;
    };
    let mut rest = &s[base.len()..];
    let floor = rest.starts_with("fl");
    if floor {
        rest = &rest[2..];
    }
    if rest.is_empty() {
        return true;
    }
    if !matches!(base, "A2" | "B" | "C" | "D") {
        return false;
    }
    let smoke = if floor { "12" } else { "123" };
    let Some(r) = rest.strip_prefix("-s") else {
        return false;
    };
    let mut c = r.chars();
    if !c.next().is_some_and(|d| smoke.contains(d)) {
        return false;
    }
    let r = c.as_str();
    if r.is_empty() {
        return true;
    }
    !floor && matches!(r, ",d0" | ",d1" | ",d2")
}

/// Prüft einen Kennwert eines Baustoffs der Art `c` (Regeln 50–53). Eigene
/// Kennwerte nehmen jeden Wert. `Err`: der Satz, warum nicht.
pub fn check_prop(c: MatCategory, key: &str, v: &PropValue) -> Result<(), String> {
    let Some(p) = mat_prop(key) else {
        return if key.trim().is_empty() {
            Err("Name fehlt".into())
        } else {
            Ok(())
        };
    };
    if FRUEHER_FREI.contains(&key) && !matches!(v, PropValue::Number(_)) {
        // Altwert aus der Zeit als eigener Kennwert, roh erhalten
        return Ok(());
    }
    let (num, text) = match v {
        PropValue::Number(n) if p.kind == MatPropKind::Number => (*n, ""),
        PropValue::Text(t) if p.kind == MatPropKind::Text => (0.0, t.as_str()),
        _ => {
            return Err(match p.kind {
                MatPropKind::Number => format!("„{key}“ ist eine Zahl"),
                MatPropKind::Text => format!("„{key}“ ist ein Text"),
            })
        }
    };
    if !num.is_finite() {
        return Err(format!("„{key}“ ist keine Zahl"));
    }
    let ok = match key {
        PRICE if c == MatCategory::Air => return Err("Luft hat keinen Richtpreis".into()),
        PRICE => num >= 0.0,
        PRICE_UNIT => PRICE_UNITS.contains(&text),
        PRICE_DATE => valid_price_date(text),
        MU => num >= 1.0,
        "c" => num > 0.0,
        EUROCLASS => valid_euroclass(text),
        WASTE => (0.0..=100.0).contains(&num),
        OPENING_FROM => num >= 0.0,
        _ => true,
    };
    if ok {
        return Ok(());
    }
    Err(match key {
        PRICE => "Richtpreis ab 0".into(),
        PRICE_UNIT => "Preiseinheit m3, m2, m oder t".into(),
        PRICE_DATE => "Preisstand als MM/JJJJ, z. B. 10/2026".into(),
        MU => "\u{3bc} ab 1".into(),
        "c" => "c größer als 0".into(),
        WASTE => "Verschnittzuschlag von 0 bis 100 %".into(),
        OPENING_FROM => "Öffnungen ab 0 m²".into(),
        _ => "Euroklasse A1, A2, B, C, D, E oder F, Zusatz wie B-s1,d0".into(),
    })
}

/// λ in W/(mK): größer als 0, bei Luft auch leer (Regel 51).
pub fn check_lambda(c: MatCategory, v: f64) -> Result<(), String> {
    if v > 0.0 || (c == MatCategory::Air && v >= 0.0) {
        Ok(())
    } else {
        Err("\u{3bb} größer als 0".into())
    }
}

/// Rohdichte in kg/m³: größer als 0, außer bei Luft (Regel 51).
pub fn check_density(c: MatCategory, v: f64) -> Result<(), String> {
    if v > 0.0 || (c == MatCategory::Air && v >= 0.0) {
        Ok(())
    } else {
        Err("Rohdichte größer als 0".into())
    }
}

/// Zeilen `[matprop]` eines Baustoffs (`.szo` und `.szk`, §2.3): feste
/// Kennwerte in der Reihenfolge von [`MAT_PROPS`], der Richtpreis mit seiner
/// Einheit (`unit=`), dann die eigenen. Ohne Kennwerte keine Zeile.
pub(crate) fn write_lines(out: &mut String, g: Guid, props: &PropSet) {
    // Die Preiseinheit steht als `unit=` am Richtpreis; ohne Richtpreis in
    // einer eigenen Zeile (Review 3n/6)
    let has_price = props.contains_key(PRICE);
    // Verschnitt und Öffnungsgrenze stehen wie früher als eigene Kennwerte
    // an ihrer Stelle unter den eigenen, damit alte Dateien bytegleich
    // bleiben
    let fixed = MAT_PROPS
        .iter()
        .filter(|p| !FRUEHER_FREI.contains(&p.key) && (p.key != PRICE_UNIT || !has_price))
        .map(|p| p.key);
    let custom = props
        .keys()
        .map(String::as_str)
        .filter(|k| FRUEHER_FREI.contains(k) || mat_prop(k).is_none());
    for k in fixed.chain(custom) {
        let Some(v) = props.get(k) else {
            continue;
        };
        let line = Line::new("matprop").guid("mat", Some(g)).text("key", k);
        let line = match v {
            PropValue::Text(t) => line.text("value", t),
            PropValue::Number(n) => line.num("num", n),
            PropValue::Bool(b) => line.flag("bool", *b),
        };
        let line = match props.get(PRICE_UNIT) {
            Some(PropValue::Text(u)) if k == PRICE => line.word("unit", u),
            _ => line,
        };
        line.finish(out);
    }
}

/// Liest eine Zeile `[matprop]` in die Kennwerte `props` eines Baustoffs
/// der Art `c` (Regeln 50–53). `Err`: Hinweis, die Zeile ist verworfen.
pub(crate) fn read_line(r: &Record, c: MatCategory, props: &mut PropSet) -> Result<(), String> {
    let drop = |why: String| {
        // Die übrigen Schlüssel der Zeile nicht noch einmal melden
        for k in ["key", "value", "num", "bool", "unit"] {
            r.opt(k);
        }
        Err(format!(
            "Zeile {}: Baustoffkennwert {why}, verworfen",
            r.line
        ))
    };
    let key = normalize_key(r.opt("key").unwrap_or(""));
    // Nur den ersten Wert lesen; weitere bleiben als fremde Schlüssel
    // erhalten (Review 3n/6)
    let value = if let Some(t) = r.opt("value") {
        PropValue::Text(t.to_string())
    } else if let Some(n) = r.opt("num") {
        match n.parse::<f64>() {
            Ok(n) if n.is_finite() => PropValue::Number(n),
            _ => return drop(format!("„{key}“: keine Zahl")),
        }
    } else if let Some(b) = r.opt("bool") {
        PropValue::Bool(b == "1")
    } else {
        return drop(format!("„{key}“ ohne Wert"));
    };
    if let Err(why) = check_prop(c, &key, &value) {
        if !FRUEHER_FREI.contains(&key.as_str()) {
            return drop(format!("„{key}“: {why}"));
        }
        // Altwert außerhalb 0 bis 100 % roh und mit seinem Typ erhalten
        // statt verwerfen; er zählt nicht ([`waste`])
        props.insert(key, value);
        return Ok(());
    }
    if key == PRICE {
        match r.opt("unit") {
            Some(u) if PRICE_UNITS.contains(&u) => {
                props.insert(PRICE_UNIT.into(), PropValue::Text(u.into()));
            }
            Some(u) => return drop(format!("„{key}“: Einheit „{u}“ unbekannt")),
            None => {}
        }
    }
    props.insert(key, value);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn euroklassen() {
        for s in ["A1", "F", "B-s1,d0", "Cfl-s1", "A2-s3,d2", "Dfl", "B-s2"] {
            assert!(valid_euroclass(s), "{s}");
        }
        for s in [
            "G",
            "A1-s1",
            "E-s1",
            "F,d0",
            "B-s4",
            "Cfl-s1,d0",
            "B,d0",
            "",
            "a1",
        ] {
            assert!(!valid_euroclass(s), "{s}");
        }
    }

    #[test]
    fn preisstand() {
        assert!(valid_price_date("01/2027") && valid_price_date("12/2026"));
        for s in ["13/2026", "00/2026", "2026-10", "1/2026", "10/26"] {
            assert!(!valid_price_date(s), "{s}");
        }
    }

    /// Review 3n/6: Preiseinheit ohne Richtpreis übersteht Schreiben und
    /// Lesen.
    #[test]
    fn preiseinheit_ohne_richtpreis() {
        let mut props = PropSet::new();
        props.insert(PRICE_UNIT.into(), PropValue::Text("m2".into()));
        let mut out = String::new();
        write_lines(&mut out, Guid(7), &props);
        assert_eq!(out.lines().count(), 1, "{out}");
        let r = Record::parse(1, out.lines().next().unwrap())
            .unwrap()
            .unwrap();
        let mut back = PropSet::new();
        read_line(&r, MatCategory::Masonry, &mut back).unwrap();
        assert_eq!(back, props);
    }

    #[test]
    fn mikro_wird_my() {
        assert_eq!(normalize_key("\u{b5}"), MU);
        assert!(mat_prop(&normalize_key(" \u{b5} ")).is_some());
    }

    /// Bestellmenge: Verschnitt am Baustoff, Prüfung und Vorgabe nach Art;
    /// übersteht Schreiben und Lesen.
    #[test]
    fn bestellung_kennwerte() {
        let c = MatCategory::Masonry;
        assert!(check_prop(c, WASTE, &PropValue::Number(5.0)).is_ok());
        assert!(check_prop(c, WASTE, &PropValue::Number(101.0)).is_err());
        assert_eq!(waste_default(MatCategory::Membrane), 15.0);
        assert_eq!(waste_default(MatCategory::Screed), 5.0);
        assert_eq!(waste_default(MatCategory::Coating), 10.0);
        let mut props = PropSet::new();
        props.insert(WASTE.into(), PropValue::Number(7.5));
        let mut out = String::new();
        write_lines(&mut out, Guid(7), &props);
        let mut back = PropSet::new();
        for (i, l) in out.lines().enumerate() {
            let r = Record::parse(i + 1, l).unwrap().unwrap();
            read_line(&r, c, &mut back).unwrap();
        }
        assert_eq!(back, props);
    }

    /// Review #77/4: alte freie Werte („5 %“, „2,5 m²“ unter anderem
    /// Schlüssel, Zahlen über 100) gehen nicht verloren, die Zeilen bleiben
    /// in alter Reihenfolge bytegleich, „5 %“ zählt als 5 %.
    #[test]
    fn altwerte_bleiben() {
        let c = MatCategory::Masonry;
        let line = |k: &str, v: &str| {
            let mut out = String::new();
            Line::new("matprop")
                .guid("mat", Some(Guid(7)))
                .text("key", k)
                .text("value", v)
                .finish(&mut out);
            out
        };
        // eigene Kennwerte stehen in Schlüsselfolge (BTreeMap), so wie sie
        // vor #77 geschrieben wurden
        let alt = [
            line("Bemerkung", "x"),
            line(WASTE, "5 %"),
            line("Zuschnitt", "3"),
            line("Öffnungen abziehen ab", "2,5 m²"),
        ]
        .concat();
        let mut props = PropSet::new();
        for (i, l) in alt.lines().enumerate() {
            let r = Record::parse(i + 1, l).unwrap().unwrap();
            read_line(&r, c, &mut props).unwrap();
        }
        let mut out = String::new();
        write_lines(&mut out, Guid(7), &props);
        assert_eq!(out, alt);
        assert_eq!(waste_text("5 %"), Some(5.0));
        assert_eq!(waste_text(" 2,5% "), Some(2.5));
        assert_eq!(waste_text("viel"), None);
        let mut out = String::new();
        Line::new("matprop")
            .guid("mat", Some(Guid(7)))
            .text("key", WASTE)
            .num("num", 150)
            .finish(&mut out);
        let r = Record::parse(1, out.trim_end()).unwrap().unwrap();
        let mut p = PropSet::new();
        read_line(&r, c, &mut p).unwrap();
        assert_eq!(p.get(WASTE), Some(&PropValue::Number(150.0)));
        let mut back = String::new();
        write_lines(&mut back, Guid(7), &p);
        assert_eq!(back, out, "Zahl bleibt Zahl, bytegleich");
    }
}
