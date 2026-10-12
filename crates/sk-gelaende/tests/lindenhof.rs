//! Abnahme G1 am Beispielgrundstück „Am Lindenhof 7“ (erfunden), Sollwerte
//! aus `gelaende/beispiel/sollwerte-lindenhof-7.md`. Alle drei Punktlisten
//! ergeben dasselbe Gelände.

use sk_gelaende::abfrage::Abfrage;
use sk_gelaende::codes::Codetabelle;
use sk_gelaende::lesen::{lesen, Lage, Spalte, Trenner, Zeichensatz};
use sk_gelaende::linien::{hoehenlinien, strecken};
use sk_gelaende::pruefen::pruefen;
use sk_gelaende::{netz, Netz, Rand};

const CSV: &[u8] = include_bytes!("daten/hoehenaufmass-lindenhof-7.csv");
const TXT: &[u8] = include_bytes!("daten/hoehenaufmass-lindenhof-7.txt");
const XY: &[u8] = include_bytes!("daten/hoehenaufmass-lindenhof-7-xy.txt");

/// Örtliches System des Beispiels: Ursprung Grenzpunkt 1, um 7° gedreht.
const E0: f64 = 467108.214;
const N0: f64 = 5878741.637;

fn oertlich(e: f64, n: f64) -> (f64, f64) {
    let (s, c) = 7f64.to_radians().sin_cos();
    let (de, dn) = (e - E0, n - N0);
    (de * c + dn * s, -de * s + dn * c)
}

/// Gelände aus einer Datei: Netz in örtlichen Metern.
fn gelaende(roh: &[u8]) -> (Netz, usize) {
    let mut a = lesen(roh).unwrap();
    let codes = Codetabelle::default();
    let pr = pruefen(&mut a.punkte, &codes);
    assert!(pr.hinweise.is_empty(), "{:?}", pr.hinweise);
    let bruch = codes.bruchkanten(&a.punkte, &pr.drin);
    let p: Vec<[f64; 3]> = a
        .punkte
        .iter()
        .map(|p| {
            let (x, y) = oertlich(p.e, p.n);
            [x, y, p.h]
        })
        .collect();
    (netz(&p, &bruch, Rand::Huelle), bruch.len())
}

#[test]
fn drei_dateien_werden_erkannt() {
    let c = lesen(CSV).unwrap();
    assert_eq!(c.punkte.len(), 109);
    assert_eq!(
        (
            c.erkannt.trenner,
            c.erkannt.komma,
            c.erkannt.kopf,
            c.erkannt.zeichensatz
        ),
        (Trenner::Semikolon, true, Some(1), Zeichensatz::Utf8)
    );
    assert_eq!(c.erkannt.lage, Lage::Utm { zone: Some(32) });
    assert_eq!(
        c.satz(),
        "109 Punkte · Semikolon · Dezimalkomma · ETRS89/UTM Zone 32 (Rechtswert mit Zonenkennzahl) · Höhen 24,15 bis 26,04 m"
    );
    let gp1 = &c.punkte[0];
    assert_eq!((gp1.nr.as_str(), gp1.code.as_str()), ("1", "GR"));
    assert!((gp1.e - 467108.214).abs() < 1e-6 && (gp1.n - 5878741.637).abs() < 1e-6);
    let (lo, hi) = c.hoehen().unwrap();
    assert!((lo - 24.145).abs() < 1e-9 && (hi - 26.043).abs() < 1e-9);
    assert_eq!(c.punkte.iter().filter(|p| p.code == "GP").count(), 67);

    let t = lesen(TXT).unwrap();
    assert_eq!(
        (t.erkannt.trenner, t.erkannt.komma, t.erkannt.kopf),
        (Trenner::Leerzeichen, false, None)
    );
    assert_eq!(t.erkannt.lage, Lage::Utm { zone: None });

    let x = lesen(XY).unwrap();
    assert_eq!(x.erkannt.zeichensatz, Zeichensatz::Windows1252);
    assert_eq!(
        x.erkannt.spalten[1],
        Spalte::Hoch,
        "Spalte 2 ist der Hochwert"
    );
    assert_eq!(x.erkannt.spalten[2], Spalte::Rechts);
    assert!(!x.erkannt.zweifel);

    // alle drei: dieselben Punkte
    for o in [&t, &x] {
        assert_eq!(o.punkte.len(), 109);
        for (a, b) in c.punkte.iter().zip(&o.punkte) {
            assert_eq!((&a.nr, &a.code), (&b.nr, &b.code));
            assert!(
                (a.e - b.e).abs() < 1e-6 && (a.n - b.n).abs() < 1e-6 && (a.h - b.h).abs() < 1e-9
            );
        }
    }
}

#[test]
fn gelaende_wie_soll() {
    for roh in [CSV, TXT, XY] {
        let (n, kanten) = gelaende(roh);
        assert_eq!(kanten, 18, "18 Bruchkanten aus BO, RI, BUK, BOK");
        assert_eq!(n.bruch.len(), 18);
        assert!(
            n.aussen.is_empty() && n.hinweise.is_empty(),
            "{:?}",
            n.hinweise
        );
        // 203 statt der früheren 200: Das Beispielskript ließ mit seinem
        // endlichen Hilfsrahmen drei flache Dreiecke an der Hülle weg
        assert_eq!(n.dreiecke.len(), 203);
        let a = Abfrage::neu(&n);
        for (x, y, soll) in [
            (7.0, 6.0, 24.523),
            (17.0, 6.0, 24.683),
            (17.0, 14.0, 24.933),
            (7.0, 14.0, 24.796),
        ] {
            let z = a.hoehe(x, y).unwrap();
            assert!(
                (z - soll).abs() <= 0.0005,
                "Hausecke {x} {y}: {z:.4} statt {soll}"
            );
        }
        assert_eq!(strecken(&n, 0.25), 139);
        let l = hoehenlinien(&n, 0.25);
        assert!(l.iter().all(|x| x.punkte.len() >= 2));
        // Flurstück 1-2-3-4
        let g: Vec<[f64; 3]> = (0..4).map(|i| n.punkte[i]).collect();
        let f: f64 = (0..4)
            .map(|i| g[i][0] * g[(i + 1) % 4][1] - g[(i + 1) % 4][0] * g[i][1])
            .sum::<f64>()
            / 2.0;
        // Koordinaten auf mm gerundet: ±0,01 m² auf 117 m Umfang
        assert!((f - 822.87).abs() < 0.01, "Fläche {f}");
    }
}
