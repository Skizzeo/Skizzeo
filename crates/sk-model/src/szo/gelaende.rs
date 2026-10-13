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

/// `[terrainplate]` und das Höhenaufmaß (Gelände G0 und G1).
pub(super) fn write(m: &Model, out: &mut String) {
    write_plate(m, out);
    super::aufmass::write(m, out);
}

fn write_plate(m: &Model, out: &mut String) {
    let s = m.plate_settings();
    let werte = [s.thick, s.margin];
    if werte.iter().zip(FELDER).all(|(v, f)| *v == f.2 .0) {
        // eine Zeile, die nicht galt, bleibt roh, bis neue Werte sie ersetzen
        if let Some(raw) = &m.project().terrain_plate_raw {
            out.push_str(raw);
            out.push('\n');
        }
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

/// Geländeplatte und Höhenaufmaß ins Projekt (Gelände G0 und G1).
pub(super) fn read_project<'r>(
    project: &mut crate::model::Project,
    recs: &dyn Fn(&str) -> &'r [Record],
    lines: &[&str],
    hints: &mut Vec<String>,
) -> Result<(), LoadError> {
    (project.terrain_plate, project.terrain_plate_raw) = read(recs("terrainplate"), lines, hints)?;
    project.terrain = super::aufmass::read(recs, lines, hints)?.map(std::sync::Arc::new);
    Ok(())
}

/// Dicke und Abstand; dazu die Zeile roh, wenn ein Wert nicht gilt
/// (Regel 72 wie `[sun]`): dann ein Hinweis, und beide Vorgaben gelten.
pub(super) fn read(
    recs: &[Record],
    lines: &[&str],
    hints: &mut Vec<String>,
) -> Result<(PlateSettings, Option<String>), LoadError> {
    let mut s = PlateSettings::default();
    match recs {
        [] => {}
        [r] => {
            // Kennung `key=project`; gelesen, damit sie nicht als fremd gilt
            let _ = r.opt("key");
            let mut falsch = Vec::new();
            for (i, (k, name, (_, min, max))) in FELDER.into_iter().enumerate() {
                match bereich(r, k, name, (min, max), " mm", &mut falsch) {
                    Some(v) if i == 0 => s.thick = v,
                    Some(v) => s.margin = v,
                    None => {}
                }
            }
            if !falsch.is_empty() {
                r.skip();
                let kopf = format!("Zeile {}: ", r.line);
                let teile: Vec<&str> = falsch
                    .iter()
                    .map(|h| {
                        h.trim_start_matches(kopf.as_str())
                            .trim_end_matches(", Vorgabe gilt")
                    })
                    .collect();
                hints.push(format!(
                    "{kopf}[terrainplate] {}; die Geländeplatte hat ihre Vorgaben, die Zeile bleibt",
                    teile.join(", ")
                ));
                let raw = lines.get(r.line - 1).map(|l| l.to_string());
                return Ok((PlateSettings::default(), raw));
            }
            // Vorgaben mit unbekannten Angaben: Ohne abweichende Werte
            // schreibt Skizzeo keine Zeile, die Angaben hätten keinen Platz;
            // die Zeile bleibt roh, bis neue Werte sie ersetzen (Review N1)
            if r.unknown() > 0 && s == PlateSettings::default() {
                return Ok((s, lines.get(r.line - 1).map(|l| l.to_string())));
            }
        }
        [_, r, ..] => return Err(err(r.line, "[terrainplate] doppelt")),
    }
    Ok((s, None))
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
        // ein Wert außerhalb: beide Vorgaben, ein Hinweis, die Zeile bleibt
        // bytegleich, bis neue Werte sie ersetzen (Regel 72)
        let falsch = neu.replace("thick=800", "thick=800 margin=900");
        let mut l = load(&falsch);
        assert_eq!(l.model.plate_settings(), PlateSettings::default());
        let platte: Vec<&String> = l
            .hints
            .iter()
            .filter(|h| h.contains("terrainplate"))
            .collect();
        assert_eq!(platte.len(), 1, "{:?}", l.hints);
        assert!(platte[0].ends_with("[terrainplate] Abstand der Geländeplatte 900 mm außerhalb 1500 bis 3000 mm; die Geländeplatte hat ihre Vorgaben, die Zeile bleibt"), "{}", platte[0]);
        assert_eq!(write(&l.model), falsch, "roh behalten");
        l.model.begin("Platte");
        assert!(l.model.set_plate_settings(PlateSettings {
            thick: 800.0,
            margin: 3000.0
        }));
        l.model.commit();
        assert_eq!(write(&l.model), neu, "neue Werte ersetzen die rohe Zeile");
        // unlesbar, auch beide: ein Hinweis mit den Werten, wie sie dastehen
        for wert in ["abc", "", "NaN"] {
            let t = neu.replace("thick=800", &format!("thick={wert} margin=x"));
            let l = load(&t);
            assert_eq!(l.model.plate_settings().thick, 500.0, "{wert:?}");
            let h: Vec<&String> = l
                .hints
                .iter()
                .filter(|h| h.contains("terrainplate"))
                .collect();
            assert_eq!(h.len(), 1, "{wert:?} {:?}", l.hints);
            assert!(
                h[0].contains(&format!(
                    "Dicke der Geländeplatte {wert} mm außerhalb 100 bis 2000 mm, Abstand der Geländeplatte x mm außerhalb 1500 bis 3000 mm;"
                )),
                "{wert:?} {}",
                h[0]
            );
            assert_eq!(write(&l.model), t);
        }
        // doppelt: Fehler wie bei [sun] und [location]
        let doppelt = neu.replace(
            "[terrainplate] key=project thick=800",
            "[terrainplate] key=project thick=800\n[terrainplate] key=project thick=900",
        );
        assert!(read(&doppelt, GuidGen::with_seed(99)).is_err());
        // eine unbekannte Angabe bleibt, auch ohne abweichende Werte
        // (Review N1, Regel 72), bis neue Werte die Zeile ersetzen
        for zeile in [
            "[terrainplate] key=project neu=1",
            "[terrainplate] key=project thick=800 neu=1",
        ] {
            let t = neu.replace("[terrainplate] key=project thick=800", zeile);
            let mut l = load(&t);
            assert_eq!(write(&l.model), t, "{zeile}");
            l.model.begin("Platte");
            assert!(l.model.set_plate_settings(PlateSettings {
                thick: 900.0,
                margin: 3000.0
            }));
            l.model.commit();
            let w = write(&l.model);
            assert!(
                w.contains("[terrainplate] key=project thick=900\n"),
                "{zeile}"
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
