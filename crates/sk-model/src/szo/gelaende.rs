//! Geländeplatte in der `.szo` (Gelände G0, Jörn 12.10.): höchstens eine
//! Zeile `[terrainplate] key=project thick= margin=` mit den Werten, die von
//! der Vorgabe abweichen. Ohne Abweichung keine Zeile, so bleibt eine Datei
//! ohne sie bytegleich. Ein Wert außerhalb des Bereichs gibt einen Hinweis,
//! und die Vorgabe gilt.
//!
//! [`bereich`] liest auch die Bodenkennwerte (`[soil]`, Regel 113): eine
//! unlesbare oder ferne Zahl hält das Öffnen nie auf (Regel 114).

use super::{ausserhalb, err, Line, LoadError, Record};
use crate::model::{PlateSettings, PLATE_MARGIN, PLATE_THICK};
use crate::Model;

/// Schlüssel, Bezeichnung im Hinweis und Bereich (Vorgabe, kleinster, größter).
const FELDER: [(&str, &str, (f64, f64, f64)); 2] = [
    ("thick", "Dicke der Geländeplatte", PLATE_THICK),
    ("margin", "Abstand der Geländeplatte", PLATE_MARGIN),
];

pub(super) fn write(m: &Model, out: &mut String) {
    let s = m.plate_settings();
    let werte = [s.thick, s.margin];
    if werte.iter().zip(FELDER).all(|(v, f)| *v == f.2 .0) {
        return;
    }
    let mut line = Line::new("terrainplate").word("key", "project");
    for (v, (k, ..)) in werte.into_iter().zip(FELDER) {
        if v != FELDER.iter().find(|f| f.0 == k).map_or(v, |f| f.2 .0) {
            line = line.num(k, v);
        }
    }
    line.finish(out);
}

/// Zahl `k` an `r` im Bereich `min` bis `max` (Regel 114): fehlt sie,
/// `None`; ist sie unlesbar oder außerhalb, ein Hinweis mit `name` und
/// `einheit` (wie [`ausserhalb`]), und `None`, die Vorgabe gilt.
pub(super) fn bereich(
    r: &Record,
    k: &str,
    name: &str,
    (min, max): (f64, f64),
    einheit: &str,
    hints: &mut Vec<String>,
) -> Option<f64> {
    let roh = r.opt(k)?;
    match roh.parse::<f64>().ok() {
        Some(v) if v.is_finite() && v >= min && v <= max => return Some(v),
        Some(v) if v.is_finite() => hints.push(ausserhalb(r.line, name, v, min, max, einheit)),
        // unlesbar (Text, leer, NaN, unendlich): der Wert wie geschrieben
        _ => hints.push(ausserhalb(r.line, name, 0.0, min, max, einheit).replacen(
            &format!("{name} 0{einheit}"),
            &format!("{name} {roh}{einheit}"),
            1,
        )),
    }
    None
}

pub(super) fn read(recs: &[Record], hints: &mut Vec<String>) -> Result<PlateSettings, LoadError> {
    let mut s = PlateSettings::default();
    match recs {
        [] => {}
        [r] => {
            // Kennung `key=project`; gelesen, damit sie nicht als fremd gilt
            let _ = r.opt("key");
            for (i, (k, name, (_, min, max))) in FELDER.into_iter().enumerate() {
                if let Some(v) = bereich(r, k, name, (min, max), " mm", hints) {
                    if i == 0 {
                        s.thick = v;
                    } else {
                        s.margin = v;
                    }
                }
            }
        }
        [_, r, ..] => return Err(err(r.line, "[terrainplate] doppelt")),
    }
    Ok(s)
}

#[cfg(test)]
mod tests {
    use super::super::tests::house;
    use super::super::{read, write};
    use crate::model::PlateSettings;
    use crate::GuidGen;

    /// Ohne geänderte Werte keine Zeile (alte Dateien bytegleich); geänderte
    /// Werte stehen in der Zeile, nur die abweichenden, und kommen gleich
    /// zurück; ein Wert außerhalb gibt einen Hinweis, die Vorgabe gilt.
    #[test]
    fn gelaendeplatte_in_der_datei() {
        let load = |t: &str| read(t, GuidGen::with_seed(99)).unwrap();
        let mut m = house();
        let alt = write(&m);
        assert!(!alt.contains("[terrainplate]"));
        m.begin("Platte");
        assert!(m.set_plate_settings(PlateSettings {
            thick: 800.0,
            margin: 3000.0
        }));
        m.commit();
        let neu = write(&m);
        let zeile: Vec<&str> = neu
            .lines()
            .filter(|z| z.starts_with("[terrainplate]"))
            .collect();
        assert_eq!(zeile, ["[terrainplate] key=project thick=800"]);
        let l = load(&neu);
        assert_eq!(l.model.plate_settings().thick, 800.0);
        assert_eq!(write(&l.model), neu, "bytegleich");
        let falsch = neu.replace("thick=800", "thick=800 margin=900");
        let l = load(&falsch);
        assert_eq!(l.model.plate_settings().margin, 3000.0);
        assert!(
            l.hints
                .iter()
                .any(|h| h.contains("Abstand der Geländeplatte")),
            "{:?}",
            l.hints
        );
        // unlesbar: Hinweis mit dem Wert, wie er dasteht, die Datei öffnet
        for wert in ["abc", "", "NaN"] {
            let l = load(&neu.replace("thick=800", &format!("thick={wert}")));
            assert_eq!(l.model.plate_settings().thick, 500.0, "{wert:?}");
            assert!(
                l.hints.iter().any(|h| h.contains(&format!(
                    "Dicke der Geländeplatte {wert} mm außerhalb 100 bis 2000 mm, Vorgabe gilt"
                ))),
                "{wert:?} {:?}",
                l.hints
            );
        }
    }

    /// Bodenkennwerte (Regel 113, 114): eine unlesbare Zahl an `[soil]` oder
    /// am alten Platz `[project]` gibt einen Hinweis, die Vorgabe gilt und
    /// die Datei öffnet.
    #[test]
    fn bodenwert_unlesbar_oeffnet_mit_hinweis() {
        let load = |t: &str| read(t, GuidGen::with_seed(99)).unwrap();
        let m = house();
        let text = write(&m);
        let vorgabe = m.project().soil.oberboden;
        assert!(!text.contains("[soil]"), "Vorgabe: keine Zeile");
        let projekt = text
            .lines()
            .find(|l| l.starts_with("[project] "))
            .unwrap()
            .to_string();
        let neu = text.replacen(
            &projekt,
            &format!("{projekt}\n[soil] key=project topsoil=abc"),
            1,
        );
        let alt = text.replacen("[project] ", "[project] topsoil=abc ", 1);
        for t in [neu, alt] {
            let l = load(&t);
            assert_eq!(l.model.project().soil.oberboden, vorgabe);
            assert!(
                l.hints
                    .iter()
                    .any(|h| h.contains("abc") && h.ends_with("Vorgabe gilt")),
                "{:?}",
                l.hints
            );
        }
    }
}
