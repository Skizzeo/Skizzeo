//! Abnahme G1 am Beispielgrundstück „Am Lindenhof 7“ (erfunden), Sollwerte
//! aus `gelaende/beispiel/sollwerte-lindenhof-7.md`. Alle drei Punktlisten
//! und beide DXF (G1b) ergeben dasselbe Gelände. Dazu die Erdmengen von G2
//! über dem Netz.

use sk_gelaende::abfrage::Abfrage;
use sk_gelaende::codes::Codetabelle;
use sk_gelaende::dxf::Linienart;
use sk_gelaende::lesen::{lesen, Lage, Spalte, Trenner, Zeichensatz};
use sk_gelaende::linien::{hoehenlinien, strecken};
use sk_gelaende::pruefen::pruefen;
use sk_gelaende::volumen::{erdmenge, mittel};
use sk_gelaende::{netz, Netz, Rand};

const CSV: &[u8] = include_bytes!("daten/hoehenaufmass-lindenhof-7.csv");
const TXT: &[u8] = include_bytes!("daten/hoehenaufmass-lindenhof-7.txt");
const XY: &[u8] = include_bytes!("daten/hoehenaufmass-lindenhof-7-xy.txt");
/// POINT mit Z auf Layern PKT_<Code>, Nummern als Text, 3D-Polylinien.
const DXF: &[u8] = include_bytes!("daten/hoehenaufmass-lindenhof-7.dxf");
/// Blöcke auf Z = 0, Höhe, Nummer und Code als Attribut.
const BLOECKE: &[u8] = include_bytes!("daten/hoehenaufmass-lindenhof-7-bloecke.dxf");

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
    let bruch = a.bruchkanten(&codes, &pr.drin);
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

/// Beide DXF geben dieselben Punkte wie die Punktliste, dazu die Grenze
/// und in der ersten die Bruchkanten als Linien.
#[test]
fn dxf_wie_punktliste() {
    let c = lesen(CSV).unwrap();
    let d = lesen(DXF).unwrap();
    let b = lesen(BLOECKE).unwrap();
    for o in [&d, &b] {
        assert_eq!(o.punkte.len(), 109);
        assert!(o.hinweise.is_empty(), "{:?}", o.hinweise);
        assert_eq!(o.erkannt.lage, Lage::Utm { zone: None });
        for (a, x) in c.punkte.iter().zip(&o.punkte) {
            assert_eq!((&a.nr, &a.code), (&x.nr, &x.code));
            assert!(
                (a.e - x.e).abs() < 1e-6 && (a.n - x.n).abs() < 1e-6 && (a.h - x.h).abs() < 1e-9
            );
        }
        // Flurstück aus FLURSTUECKSGRENZE: die vier Grenzpunkte
        let g: Vec<_> = o
            .linien
            .iter()
            .filter(|l| l.art == Linienart::Grenze)
            .collect();
        assert_eq!(g.len(), 1);
        assert_eq!(g[0].lage.len(), 4);
        for (q, p) in g[0].lage.iter().zip(&c.punkte[..4]) {
            assert!((q[0] - p.e).abs() < 1e-6 && (q[1] - p.n).abs() < 1e-6);
        }
    }
    assert_eq!(
        d.satz(),
        "109 Punkte · DXF R12 · 4 Bruchkanten · 1 Grenze · ETRS89/UTM · Höhen 24,15 bis 26,04 m"
    );
    assert_eq!(
        b.satz(),
        "109 Punkte · DXF R12 · Höhe aus Attribut HOEHE · 1 Grenze · ETRS89/UTM · Höhen 24,15 bis 26,04 m"
    );
    let ebenen = &d.dxf.as_ref().unwrap().ebenen;
    assert_eq!(ebenen.len(), 18);
}

#[test]
fn gelaende_wie_soll() {
    for roh in [CSV, TXT, XY, DXF, BLOECKE] {
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
        let m = mittel(&n, &[[7.0, 6.0], [17.0, 6.0], [17.0, 14.0], [7.0, 14.0]]).unwrap();
        assert!(
            (m - 24.753).abs() <= 0.0005,
            "Mittel unter der Platte {m:.4}"
        );
        assert_eq!(strecken(&n, 0.25), Some(139));
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

/// G2: Erdmengen unter RH-1 bei ±0,00 = 24,95 m ü. NHN, exakt über die
/// Dreiecke (Sollwerte G2). Auffüllung und Abtrag stehen getrennt; eine
/// Ersatzebene auf mittlerer Höhe gäbe keinen Abtrag.
#[test]
fn erdmengen_wie_soll() {
    let (n, _) = gelaende(CSV);
    let a = Abfrage::neu(&n);
    let rechteck = |x0: f64, y0: f64, x1: f64, y1: f64| [[x0, y0], [x1, y0], [x1, y1], [x0, y1]];
    // Platte 10 × 8 m, innerhalb der Schürze (0,35 m) 9,30 × 7,30 m
    let aussen = rechteck(7.0, 6.0, 17.0, 14.0);
    let innen = rechteck(7.35, 6.35, 16.65, 13.65);
    // N = Gelände − 0,30 m Oberboden: N gegen z heißt Gelände gegen z + 0,30
    // Schürze 0,80 m unter dem tiefsten Punkt am Haus (24,523)
    let (uk_platte, uk_schicht, uk_schuerze, oberboden) = (24.73, 24.58, 23.723, 0.30);
    let e = |r: &[[f64; 2]], z: f64| a.erdmenge(r, [z + oberboden, 0.0, 0.0]);
    let fill = e(&innen, uk_schicht).unter;
    // min(UK Platte, N) − UK Schicht = (N − UK Schicht)⁺ − (N − UK Platte)⁺
    let bett = e(&innen, uk_schicht).ueber - e(&innen, uk_platte).ueber;
    // Ring: außen minus innen; N liegt überall über UK Schürze
    let ring = |z: f64| {
        let (o, i) = (e(&aussen, z), e(&innen, z));
        (o.ueber - i.ueber, o.unter - i.unter, o.flaeche - i.flaeche)
    };
    let (s_ueber, s_unter, s_flaeche) = ring(uk_schuerze);
    assert_eq!(s_unter, 0.0);
    let trench = s_ueber - ring(uk_platte).0;
    for (was, ist, soll) in [
        ("Auffüllung", fill, 8.549),
        ("Bett", bett, 0.044),
        ("Graben", trench, 8.762),
        ("Ring m²", s_flaeche, 12.11),
        ("innen m²", e(&innen, 0.0).flaeche, 67.89),
    ] {
        assert!((ist - soll).abs() <= 0.001, "{was}: {ist:.4} statt {soll}");
    }
    // ohne Gitter dasselbe
    let o = erdmenge(&n, &innen, [uk_schicht + oberboden, 0.0, 0.0]);
    assert!((o.unter - fill).abs() < 1e-9 && (o.ueber - e(&innen, uk_schicht).ueber).abs() < 1e-9);
    // Ersatzebene: Mittel innerhalb der Schürze 24,755, kein Abtrag
    let m = mittel(&n, &innen).unwrap();
    assert!((m - 24.755).abs() <= 0.0005, "Mittel {m:.4}");
    let ersatz = 67.89 * (uk_schicht - (m - oberboden));
    assert!(
        (ersatz - 8.504).abs() <= 0.002 && bett > 0.04,
        "{ersatz:.4}"
    );
}
