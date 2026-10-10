//! Automatikmengen an der Fassade: Fußpunkt der Verblendschale aus
//! Schaumglas (`facing.*`, Recherche §A5/A6) und Lattung und Randprofil
//! der Bekleidung unter einem Vorsprung (`soffit.*`, §B3). Die Lattung wird
//! nicht modelliert (Jörn 10.10.), sie steht nur als Position im LV.
//! Quellen: planung/flachdach/recherche-foamglas-untersicht.md.
//!
//! Menge der Lattung = Fläche / Achsabstand + Umfang (Randlatten) der
//! Bekleidung ([`CladdingQto`], W3); der Verschnitt steckt im Aufwand der
//! Bauleistung. Ohne Bekleidung keine Lattung und kein Lüftungsprofil.

use crate::element::ElementKind;
use crate::model::Model;
use crate::qto::{AutoMenge, CladdingQto, ElementQto, FacingFootQto, Schedule, SoffitQto};

pub const FOOT: &str = "facing.foot";
pub const FOOT_COURSE: &str = "facing.footcourse";
pub const BATTEN: &str = "soffit.batten";
pub const COUNTER: &str = "soffit.counter";
pub const EDGE: &str = "soffit.edge";

/// Schlüssel an Fassade und Untersicht (`[service] auto=`), Einheit und
/// Bedeutung.
pub const SCHLUESSEL: [(&str, &str, &str); 5] = [
    (
        FOOT,
        "m",
        "Fußpunkt der Verblendschale aus Schaumglas: Länge in der Außenflucht",
    ),
    (
        FOOT_COURSE,
        "m",
        "Fußpunkt aus Schaumglas: Länge je weitere Lage über der ersten",
    ),
    (
        BATTEN,
        "m",
        "Grundlattung unter der Bekleidung: Fläche / Achsabstand + Umfang",
    ),
    (
        COUNTER,
        "m",
        "Traglattung quer unter der Bekleidung: Fläche / Achsabstand + Umfang",
    ),
    (
        EDGE,
        "m",
        "freie Außenkante der Bekleidung am Verblender (Lüftungsprofil)",
    ),
];

/// Kostengruppe (DIN 276): 335 Außenwandbekleidungen, wie die Verblendschale.
pub const KG_FUSSPUNKT: u16 = 335;
/// Kostengruppe (DIN 276:2018): 354 Deckenbekleidungen, wie die Untersicht.
pub const KG_UNTERSICHT: u16 = 354;

/// Mengen einer Untersicht: Schlüssel, Wert (mm) und Rechenweg; ohne
/// Bekleidung keine.
pub fn werte(s: &SoffitQto) -> Vec<(&'static str, f64, String)> {
    s.cladding.as_ref().map(bekleidung).unwrap_or_default()
}

fn bekleidung(c: &CladdingQto) -> Vec<(&'static str, f64, String)> {
    if c.area <= 0.0 {
        return Vec::new();
    }
    let m = |mm: f64| format!("{:.2} m", mm / 1000.0).replace('.', ",");
    let m2 = format!("{:.2} m²", c.area / 1e6).replace('.', ",");
    let mut out = Vec::new();
    for (key, a, value) in [
        (BATTEN, c.batten, c.batten_length()),
        (COUNTER, c.counter, c.counter_length()),
    ] {
        if value > 0.0 {
            let text = format!("{m2} / {} + {} Umfang", m(a), m(c.perimeter));
            out.push((key, value, text));
        }
    }
    if c.edge > 0.0 {
        out.push((EDGE, c.edge, format!("{} freie Kante", m(c.edge))));
    }
    out
}

/// Automatikmengen eines Bauteils; `vorlage` trägt Bauteil, Nummer,
/// Geschoss und Gebäude.
fn mengen(
    vorlage: &AutoMenge,
    (werte, kg): (Vec<(&'static str, f64, String)>, u16),
) -> Vec<AutoMenge> {
    werte
        .into_iter()
        .map(|(key, value, formula)| AutoMenge {
            key,
            unit: "m",
            value,
            kg: Some(kg),
            formula,
            ..vorlage.clone()
        })
        .collect()
}

/// Mengen eines Fußpunkts: Schlüssel, Wert (mm) und Rechenweg.
pub fn fuss_werte(f: &FacingFootQto) -> Vec<(&'static str, f64, String)> {
    fuss(f.length, f.courses)
}

fn fuss(length: f64, courses: u32) -> Vec<(&'static str, f64, String)> {
    if length <= 0.0 {
        return Vec::new();
    }
    let m = |mm: f64| format!("{:.2} m", mm / 1000.0).replace('.', ",");
    let lagen = |n: u32| match n {
        1 => "1 Lage".to_string(),
        n => format!("{n} Lagen"),
    };
    let mut out = vec![(
        FOOT,
        length,
        format!("{} Außenflucht, {} à 125 mm", m(length), lagen(courses)),
    )];
    if courses > 1 {
        let n = courses - 1;
        out.push((
            FOOT_COURSE,
            length * n as f64,
            format!(
                "{} × {n} weitere {}",
                m(length),
                if n == 1 { "Lage" } else { "Lagen" }
            ),
        ));
    }
    out
}

/// Automatikmengen an Fassade und Untersicht aus der fertigen Mengenliste,
/// in der Reihenfolge ihrer Zeilen.
pub(crate) fn bauteil_mengen(model: &Model, sched: &Schedule) -> Vec<AutoMenge> {
    let mut out = Vec::new();
    for s in sched
        .buildings
        .iter()
        .flat_map(|b| &b.storeys)
        .chain(&sched.loose)
    {
        for r in s.groups.iter().flat_map(|g| &g.rows) {
            let Some(e) = model.element(r.element) else {
                continue;
            };
            let werte = match (&r.q, &e.kind) {
                (Some(ElementQto::Soffit(q)), ElementKind::SoffitInsulation { .. }) => {
                    (werte(q), KG_UNTERSICHT)
                }
                (Some(ElementQto::Wall(w)), ElementKind::Wall(_)) => match &w.facing_foot {
                    Some(f) => (fuss_werte(f), KG_FUSSPUNKT),
                    None => continue,
                },
                _ => continue,
            };
            let vorlage = AutoMenge {
                element: r.element,
                number: r.number.clone(),
                storey: s.id,
                building: model.storey(s.id).and_then(|x| x.building),
                key: "",
                unit: "",
                value: 0.0,
                kg: None,
                formula: String::new(),
            };
            out.extend(mengen(&vorlage, werte));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 140 mm Terrassenaufbau: 2 Lagen, die zweite als Zulage.
    #[test]
    fn fusspunkt_mit_lagen() {
        let w = fuss(4500.0, crate::qto::foot_courses(140.0));
        let zahlen: Vec<(&str, f64)> = w.iter().map(|a| (a.0, a.1)).collect();
        assert_eq!(zahlen, [(FOOT, 4500.0), (FOOT_COURSE, 4500.0)]);
        assert_eq!(w[0].2, "4,50 m Außenflucht, 2 Lagen à 125 mm");
        assert_eq!(w[1].2, "4,50 m × 1 weitere Lage");
        assert_eq!(fuss(4500.0, 1).len(), 1);
    }

    #[test]
    fn lattung_aus_flaeche_und_umfang() {
        let c = CladdingQto {
            thickness: 40.0,
            area: 15e6,
            volume: 0.0,
            perimeter: 23000.0,
            edge: 10000.0,
            batten: 800.0,
            counter: 400.0,
        };
        let mut s = SoffitQto {
            area: 15e6,
            volume: 0.0,
            thickness: 120.0,
            cladding: Some(c.clone()),
        };
        let w = werte(&s);
        // 15 m² / 0,8 m + 23 m = 41,75 m; / 0,4 m + 23 m = 60,5 m
        let zahlen: Vec<(&str, f64)> = w.iter().map(|a| (a.0, a.1)).collect();
        assert_eq!(
            zahlen,
            [(BATTEN, 41750.0), (COUNTER, 60500.0), (EDGE, 10000.0)]
        );
        assert_eq!(w[0].2, "15,00 m² / 0,80 m + 23,00 m Umfang");
        assert_eq!(w[2].2, "10,00 m freie Kante");
        // ohne Traglattung kein Wert, ohne Bekleidung gar keiner
        s.cladding = Some(CladdingQto { counter: 0.0, ..c });
        assert_eq!(werte(&s).len(), 2);
        s.cladding = None;
        assert!(werte(&s).is_empty());
    }
}
