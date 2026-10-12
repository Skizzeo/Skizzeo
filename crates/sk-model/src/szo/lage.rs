//! Lage des Bauorts in der `.szo` ([`crate::model::Location`],
//! Sonnenstand S1/S2, Landeskoordinaten I4c): höchstens eine Zeile
//! `[location] lat= lon= north= x= y= utm_e= utm_n= utm_zone= nhn=`, jedes
//! Feld nur, wenn es gesetzt ist. Ohne Angaben keine Zeile, so bleibt eine
//! Datei ohne sie bytegleich. Eine Zeile, die nicht zählt, bleibt roh stehen
//! (Regel 72).

use super::{err, Line, LoadError, Record};
use crate::model::{Foot, Location, FOOT_MAX};
use crate::Model;

/// Die Zeile der Lage, wenn es eine gibt.
pub(super) fn write(m: &Model, out: &mut String) {
    let l = m.location();
    if let Some(raw) = m.location_raw().filter(|_| l.is_unset()) {
        out.push_str(raw);
        out.push('\n');
    } else if !l.is_unset() {
        let mut line = Line::new("location");
        let [x, y] = m.north_foot().map_or([None; 2], |p| p.map(Some));
        for (k, v) in [
            ("lat", l.lat),
            ("lon", l.lon),
            ("north", l.north),
            ("x", x),
            ("y", y),
            ("utm_e", l.utm.map(|u| u.e)),
            ("utm_n", l.utm.map(|u| u.n)),
            ("utm_zone", l.utm.map(|u| u.zone as f64)),
            ("nhn", l.nhn),
        ] {
            if let Some(v) = v {
                line = line.num(k, v);
            }
        }
        line.finish(out);
    }
}

/// Lage, Fußpunkt des Nordpfeils und die rohe Zeile, wenn sie nicht zählt.
pub(super) fn read(
    recs: &[Record],
    lines: &[&str],
    hints: &mut Vec<String>,
) -> Result<(Location, Option<Foot>, Option<String>), LoadError> {
    let mut location = Location::default();
    let mut location_raw = None;
    let mut foot = None;
    match recs {
        [] => {}
        [r] => {
            let mut falsch = Vec::new();
            let mut num = |k: &'static str, bereich: (f64, f64)| {
                let v = r.opt(k)?;
                let x = v
                    .parse::<f64>()
                    .ok()
                    .filter(|x| crate::model::im_bereich(*x, bereich));
                if x.is_none() {
                    falsch.push(format!("„{k}“ ist keine Zahl im Bereich"));
                }
                x
            };
            let lat = num("lat", (-90.0, 90.0));
            let lon = num("lon", (-180.0, 180.0));
            let north = num("north", (-f64::MAX, f64::MAX));
            // Fußpunkt des Nordpfeils (S2): x und y zusammen, nur mit north
            let (x, y) = (
                num("x", (-FOOT_MAX, FOOT_MAX)),
                num("y", (-FOOT_MAX, FOOT_MAX)),
            );
            // Landeskoordinaten (I4c): Ost, Nord und Zone nur zusammen
            let (e, n) = (
                num("utm_e", crate::model::UTM_E),
                num("utm_n", crate::model::UTM_N),
            );
            let zone = num("utm_zone", (32.0, 33.0));
            let nhn = num("nhn", crate::model::NHN);
            // 32,5 liegt im Bereich, ist aber keine Zone
            let zone = zone.and_then(|z| {
                let z = crate::model::UTM_ZONEN.into_iter().find(|&k| k as f64 == z);
                if z.is_none() {
                    falsch.push("„utm_zone“ ist keine Zahl im Bereich".into());
                }
                z
            });
            let read = Location {
                lat,
                lon,
                north,
                utm: match (e, n, zone) {
                    (Some(e), Some(n), Some(zone)) => Some(crate::model::Utm { e, n, zone }),
                    _ => None,
                },
                nhn,
            };
            let fehlt = |k: &str| falsch.iter().all(|f| !f.starts_with(&format!("„{k}“")));
            let luecke = match (x, y, read.north) {
                (Some(_), None, _) if fehlt("y") => Some("y"),
                (None, Some(_), _) if fehlt("x") => Some("x"),
                (Some(_), Some(_), None) if fehlt("north") => Some("north"),
                _ => None,
            };
            let utm_da = ["utm_e", "utm_n", "utm_zone"].map(|k| r.opt(k).is_some());
            let utm_luecke = (utm_da.contains(&true))
                .then(|| {
                    ["utm_e", "utm_n", "utm_zone"]
                        .into_iter()
                        .zip(utm_da)
                        .find(|&(k, da)| !da && fehlt(k))
                        .map(|(k, _)| k)
                })
                .flatten();
            for k in luecke.into_iter().chain(utm_luecke) {
                falsch.push(format!("„{k}“ fehlt"));
            }
            if falsch.is_empty() && !read.is_unset() {
                location = read.normalized();
                foot = x.zip(y).map(|(x, y)| [x, y]);
            } else if falsch.is_empty() {
                // Keine bekannte Angabe (`elev=` einer neueren Fassung): zählt
                // nicht als Lage, bleibt im Wortlaut (Review 3br); Unbekanntes
                // meldet der Hinweis zu fremden Schlüsseln
                location_raw = Some(lines[r.line - 1].to_string());
            } else {
                r.skip();
                location_raw = Some(lines[r.line - 1].to_string());
                hints.push(format!(
                    "Zeile {}: [location] {}; die Lage gilt als nicht gesetzt, die Zeile bleibt",
                    r.line,
                    falsch.join(", ")
                ));
            }
        }
        [_, r, ..] => return Err(err(r.line, "[location] doppelt")),
    }
    Ok((location, foot, location_raw))
}

#[cfg(test)]
mod tests {
    use super::super::tests::house;
    use super::super::{read, write};
    use crate::model::{Location, Utm};
    use crate::GuidGen;

    fn mit(z: &str) -> String {
        let alt = write(&house());
        alt.replacen("[building]", &format!("{z}\n[building]"), 1)
    }

    /// I4c: Landeskoordinaten und Höhe über NHN hinter x/y, nur gesetzte;
    /// Rundlauf mit allen Feldern; eine Datei ohne sie bleibt bytegleich.
    #[test]
    fn landeskoordinaten_rundlauf() {
        let alt = write(&house());
        assert!(!alt.contains("utm_"));
        let mut m = house();
        m.begin("Projektdaten geändert");
        let l = Location {
            lat: Some(53.0589),
            lon: Some(8.591),
            north: Some(12.0),
            utm: Some(Utm {
                e: 467_123.456,
                n: 5_878_765.432,
                zone: 32,
            }),
            nhn: Some(14.25),
        };
        assert!(m.set_location(l));
        m.commit();
        let neu = write(&m);
        let zeilen: Vec<&str> = neu
            .lines()
            .filter(|z| z.starts_with("[location]"))
            .collect();
        assert_eq!(
            zeilen,
            ["[location] lat=53.0589 lon=8.591 north=12 utm_e=467123.456 utm_n=5878765.432 utm_zone=32 nhn=14.25"]
        );
        let back = read(&neu, GuidGen::with_seed(9)).unwrap();
        assert!(back.hints.is_empty(), "{:?}", back.hints);
        assert_eq!(*back.model.location(), l);
        assert_eq!(write(&back.model), neu, "bytegleich");
        // nhn allein
        let nur = mit("[location] nhn=-2.5");
        let l = read(&nur, GuidGen::with_seed(9)).unwrap();
        assert!(l.hints.is_empty(), "{:?}", l.hints);
        assert_eq!(l.model.location().nhn, Some(-2.5));
        assert_eq!(write(&l.model), nur);
    }

    /// I4c: Ost, Nord und Zone nur zusammen; fehlt eines oder liegt eines
    /// außerhalb, zählt die Zeile nicht und bleibt roh (Regel 72).
    #[test]
    fn landeskoordinaten_unvollstaendig_bleibt_roh() {
        for (z, wort) in [
            (
                "[location] lat=53 utm_e=467000 utm_zone=32",
                "„utm_n“ fehlt",
            ),
            ("[location] utm_n=5878000 utm_zone=33", "„utm_e“ fehlt"),
            ("[location] utm_e=467000 utm_n=5878000", "„utm_zone“ fehlt"),
            (
                "[location] utm_e=467000 utm_n=5878000 utm_zone=31",
                "„utm_zone“ ist keine",
            ),
            (
                "[location] utm_e=467000 utm_n=5878000 utm_zone=32.5",
                "„utm_zone“ ist keine",
            ),
            (
                "[location] utm_e=99999 utm_n=5878000 utm_zone=32",
                "„utm_e“ ist keine",
            ),
            (
                "[location] utm_e=467000 utm_n=-1 utm_zone=32",
                "„utm_n“ ist keine",
            ),
            ("[location] nhn=9001", "„nhn“ ist keine"),
        ] {
            let t = mit(z);
            let l = read(&t, GuidGen::with_seed(9)).unwrap_or_else(|e| panic!("{z}: {e:?}"));
            assert_eq!(l.hints.len(), 1, "{z}: {:?}", l.hints);
            assert!(l.hints[0].contains(wort), "{z}: {:?}", l.hints);
            assert!(l.model.location().is_unset(), "{z}");
            assert_eq!(write(&l.model), t, "{z}: bytegleich");
        }
    }
}
