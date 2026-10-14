//! Abnahme G1c an einer echten Liegenschaftskarte aus dem Projektordner.
//! Karte und Sollwerte liegen nur dort, in einer Textdatei (Pfad in
//! `SKIZZEO_KARTE_SOLLWERTE`, sonst [`SOLLWERTE`]); fehlt sie oder die
//! Karte, tut der Test nichts. Je Zeile `schlüssel = wert`, `#` leitet
//! einen Kommentar ein:
//!
//! - `datei`: Pfad der Karte (PDF)
//! - `flurstueck`: Nummer, die Skizzeo ohne Klick finden soll
//! - `ecke`: je Ecke „E N“ in ETRS89/UTM, gegen den Uhrzeigersinn (Anfang
//!   beliebig), eine Zeile je Ecke
//! - `flaeche`: „Soll Toleranz“ in m²
//! - wahlweise `ecken_toleranz` (m je Achse, sonst 0,01), `erzeuger`
//!   (`AedSicad` oder `Unbekannt`), `zone` und `gitterkreuze` (Lage aus
//!   dem Gitter), `massstab` („Soll Toleranz“), `restfehler` („von bis“ in
//!   m), `grenzpunkte` (Anzahl) und `kreis_abstand` (größter Abstand in pt
//!   zwischen Ecke und Grenzpunktkreis).

use sk_gelaende::lageplan::{self, lage::Lage};

const SOLLWERTE: &str =
    "/mnt/project-files/skizzeo/gelaende/liegenschaftskarte/sollwerte-lokal.txt";

/// Sollwerte als Schlüssel und Wert, in der Reihenfolge der Datei.
struct Soll(Vec<(String, String)>);

impl Soll {
    fn lesen(text: &str) -> Soll {
        Soll(
            text.lines()
                .map(|l| l.split('#').next().unwrap_or("").trim())
                .filter_map(|l| l.split_once('='))
                .map(|(k, w)| (k.trim().to_string(), w.trim().to_string()))
                .collect(),
        )
    }

    fn text(&self, k: &str) -> Option<&str> {
        self.0.iter().find(|(s, _)| s == k).map(|(_, w)| w.as_str())
    }

    fn alle(&self, k: &str) -> Vec<Vec<f64>> {
        self.0
            .iter()
            .filter(|(s, _)| s == k)
            .map(|(_, w)| {
                w.split_whitespace()
                    .map(|z| {
                        z.parse()
                            .unwrap_or_else(|_| panic!("Sollwert {k}: keine Zahl"))
                    })
                    .collect()
            })
            .collect()
    }

    fn zahlen(&self, k: &str) -> Option<Vec<f64>> {
        self.alle(k).into_iter().next()
    }
}

#[test]
fn flurstueck_ohne_klick() {
    let pfad = std::env::var("SKIZZEO_KARTE_SOLLWERTE").unwrap_or_else(|_| SOLLWERTE.into());
    let Ok(text) = std::fs::read_to_string(&pfad) else {
        return;
    };
    let soll = Soll::lesen(&text);
    let datei = soll.text("datei").expect("Sollwert datei fehlt");
    let Ok(roh) = std::fs::read(datei) else {
        return;
    };
    let plan = lageplan::lesen(&roh).unwrap();
    assert!(plan.hinweise.is_empty(), "{} Hinweise", plan.hinweise.len());
    if let Some(e) = soll.text("erzeuger") {
        assert_eq!(format!("{:?}", plan.erzeuger), e);
    }
    let massstab = soll.zahlen("massstab");
    match &plan.lage {
        Lage::Gitter(e) => {
            if let Some(z) = soll.zahlen("zone") {
                assert_eq!(e.zone, Some(z[0] as u8));
            }
            if let Some(n) = soll.zahlen("gitterkreuze") {
                assert_eq!(e.punkte, n[0] as usize);
            }
            if let Some(m) = &massstab {
                assert!((e.massstab - m[0]).abs() <= m[1], "1:{}", e.massstab);
            }
            if let Some(r) = soll.zahlen("restfehler") {
                assert!(
                    e.restfehler >= r[0] && e.restfehler <= r[1],
                    "Restfehler {} m",
                    e.restfehler
                );
            }
        }
        Lage::Massstab(m) => {
            assert!(soll.text("zone").is_none(), "Lage nicht aus dem Gitter");
            if let Some(s) = &massstab {
                assert!((m - s[0]).abs() <= s[1], "1:{m}");
            }
        }
        Lage::Offen => panic!("keine Lage gefunden"),
    }
    if let Some(n) = soll.zahlen("grenzpunkte") {
        assert_eq!(plan.grenzpunkte.len(), n[0] as usize);
    }
    let nummer = soll.text("flurstueck").expect("Sollwert flurstueck fehlt");
    assert_eq!(plan.nummer.as_deref(), Some(nummer));
    let f = plan.flurstueck.as_ref().expect("kein Flurstück");
    let ecken_soll = soll.alle("ecke");
    assert!(!ecken_soll.is_empty(), "Sollwert ecke fehlt");
    let tol = soll.zahlen("ecken_toleranz").map_or(0.01, |t| t[0]);
    let ecken = f.ecken.as_ref().expect("Ecken ohne Lage");
    assert_eq!(ecken.len(), ecken_soll.len());
    // gleiche Ecken, Anfang beliebig
    let k = ecken
        .iter()
        .position(|p| {
            (p[0] - ecken_soll[0][0]).abs() < 0.5 && (p[1] - ecken_soll[0][1]).abs() < 0.5
        })
        .expect("Ecke 1");
    for (i, s) in ecken_soll.iter().enumerate() {
        let p = ecken[(k + i) % ecken.len()];
        assert!(
            (p[0] - s[0]).abs() <= tol && (p[1] - s[1]).abs() <= tol,
            "Ecke {}: {p:?} statt {s:?}",
            i + 1
        );
    }
    let a = f.flaeche.expect("keine Fläche");
    let fl = soll.zahlen("flaeche").expect("Sollwert flaeche fehlt");
    assert!((a - fl[0]).abs() <= fl[1], "{a} m²");
    // jede Ecke ist ein Grenzpunkt
    if let Some(d) = soll.zahlen("kreis_abstand") {
        for p in &f.ecken_pt {
            assert!(plan
                .grenzpunkte
                .iter()
                .any(|g| (g.lage[0] - p[0]).hypot(g.lage[1] - p[1]) <= d[0]));
        }
    }
}
