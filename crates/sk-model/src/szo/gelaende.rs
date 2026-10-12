//! Geländeplatte in der `.szo` (Gelände G0, Jörn 12.10.): höchstens eine
//! Zeile `[terrainplate] key=project thick= margin=` mit den Werten, die von
//! der Vorgabe abweichen. Ohne Abweichung keine Zeile, so bleibt eine Datei
//! ohne sie bytegleich. Ein Wert außerhalb des Bereichs gibt einen Hinweis,
//! und die Vorgabe gilt.

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

pub(super) fn read(recs: &[Record], hints: &mut Vec<String>) -> Result<PlateSettings, LoadError> {
    let mut s = PlateSettings::default();
    match recs {
        [] => {}
        [r] => {
            // Kennung `key=project`; gelesen, damit sie nicht als fremd gilt
            let _ = r.opt("key");
            for (i, (k, name, (_, min, max))) in FELDER.into_iter().enumerate() {
                if r.opt(k).is_none() {
                    continue;
                }
                let v = r.f64(k)?;
                if v.is_finite() && v >= min && v <= max {
                    if i == 0 {
                        s.thick = v;
                    } else {
                        s.margin = v;
                    }
                } else {
                    hints.push(ausserhalb(r.line, name, v, min, max, " mm"));
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
    }
}
