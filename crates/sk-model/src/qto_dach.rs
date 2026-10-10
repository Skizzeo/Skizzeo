//! Automatikmengen am Dach (`coping.*`): Zulagen der Attikaabdeckung, die an
//! keiner Schicht hängen. Sie stehen am Attikablech selbst, damit Gebäude,
//! Geschoss und Umfang wie bei seiner Mengenzeile gelten. Herleitung und
//! Quellen: planung/flachdach/recherche-flachdach-attika.md §3.
//!
//! Die Werte sind roh (mm bzw. ganze Stück); was eine Bauleistung daraus
//! macht (Einheit, Preis), steht im Katalog.

use crate::element::ElementKind;
use crate::model::Model;
use crate::qto::{AutoMenge, CopingQto, ElementQto, Schedule};
use crate::terrace::CopingPath;

/// Handelsmaße des Zuschnitts (Abwicklung) einer Mauerabdeckung (mm).
pub const ZUSCHNITTE: [f64; 9] = [
    200.0, 250.0, 333.0, 400.0, 500.0, 625.0, 667.0, 750.0, 1000.0,
];
/// Der Grundpreis der Attikaabdeckung gilt bis zu diesem Zuschnitt (mm).
pub const ZUSCHNITT_GRUND: f64 = 400.0;
/// Ab dieser Richtungsänderung zählt ein Knick als Ecke (Grad).
const ECKE_GRAD: f64 = 5.0;

pub const CUT500: &str = "coping.cut500";
pub const CUT667: &str = "coping.cut667";
pub const CUT1000: &str = "coping.cut1000";
pub const CORNER: &str = "coping.corner";
pub const END: &str = "coping.end";
pub const SEAL: &str = "coping.seal";

/// Schlüssel am Dach (`[service] auto=`), Einheit und Bedeutung.
pub const SCHLUESSEL: [(&str, &str, &str); 6] = [
    (
        CUT500,
        "m",
        "Attikablech mit Zuschnitt über 400 bis 500 mm (Länge Außenkante)",
    ),
    (
        CUT667,
        "m",
        "Attikablech mit Zuschnitt über 500 bis 667 mm (Länge Außenkante)",
    ),
    (
        CUT1000,
        "m",
        "Attikablech mit Zuschnitt über 667 mm (Länge Außenkante)",
    ),
    (CORNER, "st", "Ecken des Attikablechs (Knick ab 5°)"),
    (END, "st", "Endabschlüsse: zwei je offenem Blechstrang"),
    (
        SEAL,
        "m",
        "Abdichtungsanschluss unter dem Attikablech der Dachterrasse (Länge Außenkante); \
         am Flachdach gilt roof.edge",
    ),
];

/// Kostengruppe (DIN 276): 363 Dachbeläge, dazu der Dachrandabschluss.
pub const KG_DACH: u16 = 363;

/// Zuschnitt: die Abwicklung auf das nächste Handelsmaß aufgerundet; über
/// dem größten Handelsmaß die Abwicklung selbst (mm).
pub fn zuschnitt(abwicklung: f64) -> f64 {
    ZUSCHNITTE
        .iter()
        .copied()
        .find(|z| abwicklung <= z + 0.5)
        .unwrap_or(abwicklung)
}

/// Ecken eines Blechstrangs: Knicke ab 5°, beim geschlossenen Ring an
/// jedem Punkt, beim offenen nur innen.
pub fn ecken(p: &CopingPath) -> u32 {
    let n = p.points.len();
    if n < 3 {
        return 0;
    }
    let innen = if p.closed { 0..n } else { 1..n - 1 };
    let grenze = ECKE_GRAD.to_radians().cos();
    innen
        .filter(|&k| {
            let a = p.points[(k + n - 1) % n];
            let b = p.points[k];
            let c = p.points[(k + 1) % n];
            let (u, v) = ((b - a).normalized(), (c - b).normalized());
            u.dot(v) < grenze
        })
        .count() as u32
}

/// Endabschlüsse: zwei je offenem Blechstrang.
pub fn enden(p: &CopingPath) -> u32 {
    if p.closed || p.points.len() < 2 {
        0
    } else {
        2
    }
}

/// Automatikmengen eines Attikablechs; `vorlage` trägt Bauteil, Nummer,
/// Geschoss und Gebäude. `terrasse`: das Blech schließt eine Dachterrasse
/// ab, dann zählt auch der Abdichtungsanschluss darunter (`coping.seal`);
/// am Flachdach liegt er an der Innenfläche der Aufkantung (`roof.edge`).
pub(crate) fn coping_mengen(vorlage: &AutoMenge, c: &CopingQto, terrasse: bool) -> Vec<AutoMenge> {
    let m = |mm: f64| format!("{:.2} m", mm / 1000.0).replace('.', ",");
    let menge = |key, unit, value, formula| AutoMenge {
        key,
        unit,
        value,
        kg: Some(KG_DACH),
        formula,
        ..vorlage.clone()
    };
    let mut out = Vec::new();
    let stufe = if c.cut <= ZUSCHNITT_GRUND + 0.5 {
        None
    } else if c.cut <= 500.5 {
        Some(CUT500)
    } else if c.cut <= 667.5 {
        Some(CUT667)
    } else {
        Some(CUT1000)
    };
    if let Some(key) = stufe {
        out.push(menge(
            key,
            "m",
            c.length,
            format!(
                "{} Länge, Abwicklung {:.0} mm → Zuschnitt {:.0} mm",
                m(c.length),
                c.girth,
                c.cut
            ),
        ));
    }
    if terrasse && c.length > 0.0 {
        out.push(menge(
            SEAL,
            "m",
            c.length,
            format!("{} Länge Attikablech der Dachterrasse", m(c.length)),
        ));
    }
    if c.corners > 0 {
        out.push(menge(
            CORNER,
            "st",
            c.corners as f64,
            format!("{} Ecken", c.corners),
        ));
    }
    if c.ends > 0 {
        out.push(menge(
            END,
            "st",
            c.ends as f64,
            format!("{} Endabschlüsse", c.ends),
        ));
    }
    out
}

/// Automatikmengen der Bauteile am Dach aus der fertigen Mengenliste, in
/// der Reihenfolge ihrer Zeilen.
pub(crate) fn bauteil_mengen(model: &Model, sched: &Schedule) -> Vec<AutoMenge> {
    let mut out = Vec::new();
    for s in sched
        .buildings
        .iter()
        .flat_map(|b| &b.storeys)
        .chain(&sched.loose)
    {
        for r in s.groups.iter().flat_map(|g| &g.rows) {
            let (Some(ElementQto::Coping(c)), Some(e)) = (&r.q, model.element(r.element)) else {
                continue;
            };
            let ElementKind::Coping { floor } = e.kind else {
                continue;
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
            // dieselbe Regel wie die Zuordnung des Blechs zum Flachdach
            let terrasse = !model.flat_roof_coping(r.element, floor);
            out.extend(coping_mengen(&vorlage, c, terrasse));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::solid::SweepEnd;
    use sk_math::vec3;

    #[test]
    fn zuschnitt_auf_handelsmass() {
        assert_eq!(zuschnitt(250.0), 250.0);
        assert_eq!(zuschnitt(270.0), 333.0);
        assert_eq!(zuschnitt(455.0), 500.0);
        assert_eq!(zuschnitt(630.0), 667.0);
        assert_eq!(zuschnitt(1200.0), 1200.0);
    }

    #[test]
    fn ecken_und_enden() {
        let u = CopingPath {
            points: vec![
                vec3(0.0, 0.0, 0.0),
                vec3(0.0, 1500.0, 0.0),
                vec3(10000.0, 1500.0, 0.0),
                vec3(10000.0, 0.0, 0.0),
            ],
            closed: false,
            ends: [SweepEnd::Square; 2],
        };
        assert_eq!((ecken(&u), enden(&u)), (2, 2));
        let ring = CopingPath {
            closed: true,
            ..u.clone()
        };
        assert_eq!((ecken(&ring), enden(&ring)), (4, 0));
        // gerade Zwischenpunkte zählen nicht
        let gerade = CopingPath {
            points: vec![
                vec3(0.0, 0.0, 0.0),
                vec3(5000.0, 0.0, 0.0),
                vec3(10000.0, 0.0, 0.0),
            ],
            closed: false,
            ends: [SweepEnd::Square; 2],
        };
        assert_eq!(ecken(&gerade), 0);
    }
}
