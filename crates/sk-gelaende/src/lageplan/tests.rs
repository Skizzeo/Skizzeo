//! Prüfungen der Auswertung: eine nachgebaute Liegenschaftskarte nach Art
//! von AED-SICAD (Gitterkreuze, Rahmen, Flurstücksnummer als Bruch) und
//! der Beispiel-Lageplan.

use super::lage::PT;
use super::*;
use crate::seite::Zug;

/// Blatt einer erfundenen Karte 1:500: pt → UTM ist E = 512 020 + s·x,
/// N = 5 771 025 + s·y.
const E0: f64 = 512_020.0;
const N0: f64 = 5_771_025.0;

fn blatt(u: [f64; 2]) -> [f64; 2] {
    let s = 500.0 * PT;
    [(u[0] - E0) / s, (u[1] - N0) / s]
}

fn linie(p: &[[f64; 2]], farbe: Farbe, breite: f64) -> Pfad {
    Pfad {
        zuege: vec![Zug {
            punkte: p.to_vec(),
            zu: false,
        }],
        strich: Some((farbe, breite)),
        fuellung: None,
        gerade_ungerade: false,
        kurven: false,
    }
}

fn kreis_um(m: [f64; 2], d: f64) -> Pfad {
    let punkte = (0..36)
        .map(|i| {
            let w = i as f64 * 10f64.to_radians();
            [m[0] + d / 2.0 * w.cos(), m[1] + d / 2.0 * w.sin()]
        })
        .collect();
    Pfad {
        zuege: vec![Zug { punkte, zu: true }],
        strich: Some(([0; 3], 1.008)),
        fuellung: Some([254; 3]),
        gerade_ungerade: false,
        kurven: true,
    }
}

fn text(t: &str, lage: [f64; 2], groesse: f64, winkel: f64) -> Text {
    let (s, c) = winkel.to_radians().sin_cos();
    let l = 0.556 * groesse * t.chars().count() as f64;
    Text {
        text: t.into(),
        lage,
        ende: [lage[0] + c * l, lage[1] + s * l],
        groesse,
        winkel,
        schrift: "Arial".into(),
        farbe: [0; 3],
    }
}

/// Die sechs Ecken des erfundenen Flurstücks 45/6 (UTM), gegen den
/// Uhrzeigersinn.
const ECKEN: [[f64; 2]; 6] = [
    [512_058.40, 5_771_071.20],
    [512_089.70, 5_771_066.90],
    [512_093.10, 5_771_088.30],
    [512_084.60, 5_771_097.80],
    [512_071.30, 5_771_099.40],
    [512_060.90, 5_771_094.60],
];

/// Mitten der Nummern 45/6 und 45/9 (UTM).
const NUMMER: [f64; 2] = [512_075.0, 5_771_083.0];
const NACHBAR: [f64; 2] = [512_072.0, 5_771_061.0];

/// Nummer als Bruch: Zähler über dem Strich, Nenner darunter.
fn bruch(s: &mut Seite, m: [f64; 2], zaehler: &str, nenner: &str, strich: bool) {
    if strich {
        s.pfade.push(linie(
            &[[m[0] - 12.0, m[1]], [m[0] + 12.0, m[1]]],
            [0; 3],
            1.98,
        ));
    }
    s.texte
        .push(text(zaehler, [m[0] - 10.0, m[1] + 3.0], 20.0, 0.0));
    s.texte
        .push(text(nenner, [m[0] - 5.0, m[1] - 17.0], 20.0, 0.0));
}

fn karte() -> Seite {
    let mut s = Seite {
        mediabox: [0.0, 0.0, 595.28, 841.89],
        ..Seite::default()
    };
    let schwarz = [0u8; 3];
    // Rahmen 90 × 110 m, Maßstabsleiste in derselben Linienart
    let (r0, r1) = (
        blatt([512_030.0, 5_771_045.0]),
        blatt([512_120.0, 5_771_155.0]),
    );
    for (a, b) in [
        ([r0[0], r0[1]], [r1[0], r0[1]]),
        ([r1[0], r0[1]], [r1[0], r1[1]]),
        ([r1[0], r1[1]], [r0[0], r1[1]]),
        ([r0[0], r1[1]], [r0[0], r0[1]]),
        ([280.0, 100.0], [365.0, 100.0]),
    ] {
        s.pfade.push(linie(&[a, b], [0, 0, 1], 0.288));
    }
    // Gitterkreuze im 50-m-Raster, der waagerechte Arm aus zwei Hälften
    for e in [512_050.0, 512_100.0] {
        for n in [5_771_050.0, 5_771_100.0, 5_771_150.0] {
            let m = blatt([e, n]);
            s.pfade.push(linie(
                &[[m[0], m[1] - 7.0], [m[0], m[1] + 7.0]],
                schwarz,
                0.0,
            ));
            s.pfade.push(linie(&[[m[0] - 7.0, m[1]], m], schwarz, 0.0));
            s.pfade.push(linie(&[m, [m[0] + 7.0, m[1]]], schwarz, 0.0));
        }
    }
    // Grenze von 45/6 in zwei Linienzügen, der Nachbar im Süden, ein
    // Bruchstrich mit freien Enden
    let e: Vec<[f64; 2]> = ECKEN.iter().map(|u| blatt(*u)).collect();
    s.pfade
        .push(linie(&[e[0], e[1], e[2], e[3]], schwarz, 1.98));
    s.pfade
        .push(linie(&[e[3], e[4], e[5], e[0]], schwarz, 1.98));
    let sued = [
        blatt([512_056.5, 5_771_055.0]),
        blatt([512_087.0, 5_771_052.5]),
    ];
    s.pfade
        .push(linie(&[e[0], sued[0], sued[1], e[1]], schwarz, 1.98));
    for p in &e {
        s.pfade.push(kreis_um(*p, 8.0));
    }
    bruch(&mut s, blatt(NUMMER), "45", "6", true);
    // derselbe Zähler beim Nachbarn mit anderem Nenner
    bruch(&mut s, blatt(NACHBAR), "45", "9", false);
    // Rahmenwerte, Kopf und Fußzeile
    s.texte.push(text(
        "E = 32512030",
        [r0[0] - 10.0, r0[1] + 37.0],
        6.0,
        -90.0,
    ));
    s.texte
        .push(text("N = 5771045", [r0[0], r0[1] - 10.0], 6.0, 0.0));
    s.texte
        .push(text("N = 5771155", [r1[0] - 34.0, r1[1] + 6.0], 6.0, 0.0));
    s.texte
        .push(text("E = 32512120", [r1[0] + 6.0, r1[1]], 6.0, -90.0));
    s.texte
        .push(text("Erfundene Karte 1:500", [360.0, 800.0], 14.0, 0.0));
    s.texte.push(text("Flurstück:", [150.0, 772.0], 8.0, 0.0));
    s.texte.push(text("45/6", [198.0, 772.0], 8.0, 0.0));
    s.texte
        .push(text("Fußzeile mit Bedingungen", [40.0, 50.0], 8.0, 0.0));
    s.bilder.push([40.0, 760.0, 90.0, 815.0]);
    s
}

#[test]
fn karte_ohne_klicken() {
    let plan = auswerten(karte(), "AED-SICAD");
    assert_eq!(plan.erzeuger, Erzeuger::AedSicad);
    assert!(plan.hinweise.is_empty(), "{:?}", plan.hinweise);
    let r = plan.rahmen.unwrap();
    assert!((r[2] - r[0] - 90.0 / (500.0 * PT)).abs() < 1e-6);
    let Lage::Gitter(e) = &plan.lage else {
        panic!("{:?}", plan.lage);
    };
    assert_eq!((e.zone, e.punkte), (Some(32), 6));
    assert!((e.massstab - 500.0).abs() < 1e-6 && e.drehung.abs() < 1e-9);
    assert!(e.restfehler < 1e-6);
    assert_eq!(plan.grenzpunkte.len(), 6);
    assert_eq!(plan.nummer.as_deref(), Some("45/6"));
    let f = plan.flurstueck.as_ref().unwrap();
    let ecken = f.ecken.as_ref().unwrap();
    assert_eq!(ecken.len(), 6);
    // dieselben Ecken in derselben Richtung, Anfang beliebig
    let k = ecken
        .iter()
        .position(|p| (p[0] - ECKEN[0][0]).hypot(p[1] - ECKEN[0][1]) < 1e-6)
        .expect("Ecke 1");
    for (i, soll) in ECKEN.iter().enumerate() {
        let ist = ecken[(k + i) % 6];
        assert!((ist[0] - soll[0]).abs() < 1e-6 && (ist[1] - soll[1]).abs() < 1e-6);
    }
    let a = flaeche::flaeche(&ECKEN);
    assert!(
        (f.flaeche.unwrap() - a).abs() < 1e-6 && (a - 893.25).abs() < 0.01,
        "{a}"
    );
    // Nachbar 45/9 über seine Nummer
    let n = flurstueck(&plan, "45/9").unwrap();
    assert_eq!(n.ecken_pt.len(), 4);
    assert_eq!(flurstueck(&plan, "45/99"), None);
    // Unterlage: ohne Rahmen, Maßstabsleiste, Kreuze, Kopf und Fußzeile
    let u = unterlage(&plan);
    assert!(u
        .pfade
        .iter()
        .all(|(_, b)| !matches!(b, Bedeutung::Rahmen | Bedeutung::Gitter)));
    assert_eq!(
        u.pfade
            .iter()
            .filter(|(_, b)| *b == Bedeutung::Grenzpunkt)
            .count(),
        6
    );
    let t: Vec<&str> = u.texte.iter().map(|t| t.text.as_str()).collect();
    assert_eq!(t, ["45", "6", "45", "9"]);
    let z = &plan.zuordnung;
    assert!(z
        .iter()
        .any(|z| z.bedeutung == Bedeutung::Grenze && z.art.breite == 1.98 && z.anzahl == 4));
    assert!(z
        .iter()
        .any(|z| z.bedeutung == Bedeutung::Gitter && z.anzahl == 18));
}

#[test]
fn unbekannter_erzeuger_dickste_schwarze_linie() {
    // ohne Kreise: die dickste schwarze Linienart ist die Grenze
    let mut s = karte();
    s.pfade.retain(|p| !p.kurven);
    let plan = auswerten(s, "");
    assert_eq!(plan.erzeuger, Erzeuger::Unbekannt);
    let f = plan.flurstueck.unwrap();
    assert_eq!(f.ecken.unwrap().len(), 6);
    assert!(plan
        .zuordnung
        .iter()
        .any(|z| z.bedeutung == Bedeutung::Rahmen));
    // ohne Grenzlinien: ein Satz statt eines Flurstücks
    let mut s = karte();
    s.pfade.retain(|p| p.strich.is_none_or(|(_, b)| b < 1.5));
    let plan = auswerten(s, "");
    assert!(plan.flurstueck.is_none());
    assert_eq!(plan.hinweise.len(), 1);
}

#[test]
fn nummern_im_kopf_und_in_der_karte() {
    let t = |s: &str| text(s, [0.0, 0.0], 8.0, 0.0);
    assert_eq!(
        nummer_im_kopf(&[t("Flurstück: 45/6")]).as_deref(),
        Some("45/6")
    );
    assert_eq!(
        nummer_im_kopf(&[t("Maßstab 1:500 · Flurstück 123/4")]).as_deref(),
        Some("123/4")
    );
    assert_eq!(nummer_im_kopf(&[t("Flst. 12")]).as_deref(), Some("12"));
    assert_eq!(nummer_im_kopf(&[t("Flurstücksnachweis")]), None);
    assert_eq!(nummer_im_kopf(&[]), None);
    let k = karte();
    // als ein Text im Kopf, als Bruch in der Karte
    let kopf = k.texte.iter().position(|t| t.text == "45/6").unwrap();
    let q = nummer_in_karte(&k.texte, "45/6", &[]).unwrap();
    assert!(q[1] > 760.0);
    let q = nummer_in_karte(&k.texte, "45/6", &[kopf]).unwrap();
    let m = blatt(NUMMER);
    assert!(
        (q[0] - m[0]).abs() < 3.0 && (q[1] - m[1]).abs() < 3.0,
        "{q:?} {m:?}"
    );
}

#[test]
fn beispiel_lageplan_massstab_und_grenzpunkte() {
    let plan = lesen(include_bytes!("../../tests/daten/lageplan-lindenhof-7.pdf")).unwrap();
    assert_eq!(plan.lage, Lage::Massstab(500.0));
    assert_eq!(plan.erzeuger, Erzeuger::Unbekannt);
    let nr: Vec<&str> = plan
        .grenzpunkte
        .iter()
        .filter_map(|g| g.nr.as_deref())
        .collect();
    assert_eq!(nr, ["1", "2", "3", "4"]);
    // die rote Linie durch die Grenzpunkte ist die Grenze
    assert!(plan
        .zuordnung
        .iter()
        .any(|z| z.bedeutung == Bedeutung::Grenze && z.art.farbe == [204, 0, 0]));
    let f = plan.flurstueck.as_ref().unwrap();
    assert_eq!(f.nummer, "123/4");
    assert_eq!(f.ecken_pt.len(), 4);
    assert!((f.flaeche.unwrap() - 822.87).abs() < 0.1, "{:?}", f.flaeche);
    // ohne Rahmen: Kopf, Fußzeile, Maßstabsleiste und Nordpfeil fallen weg
    let u = unterlage(&plan);
    assert!(u
        .texte
        .iter()
        .all(|t| !t.text.contains("1:500") && t.text != "N" && t.text != "10 m"));
    assert!(u.texte.iter().any(|t| t.text == "123/4"));
    assert!(u.pfade.iter().any(|(_, b)| *b == Bedeutung::Grenze));
}

#[test]
fn eingescannter_plan_und_unsinn() {
    let s = Seite {
        bilder: vec![[0.0, 0.0, 595.0, 842.0]],
        ..Seite::default()
    };
    let plan = auswerten(s, "");
    assert_eq!(plan.lage, Lage::Offen);
    assert_eq!(plan.hinweise.len(), 1);
    assert_eq!(unterlage(&plan), Unterlage::default());
    assert!(lesen(b"%PDF-1.4 kaputt").is_err());
}

/// Bomben (Review W37 B7, B8): 20.000 dünne Striche aus Stücken, die sich
/// alle verbinden ließen; 20.000 Grenzstriche im Gitter; 5000 gleiche
/// Nummern; Koordinaten am Rand von f64. Alles bleibt schnell und läuft
/// nicht über.
#[test]
fn bomben_der_auswertung() {
    use std::time::Instant;
    // je Zeile 40 kollineare Stücke von 1 pt, 500 Zeilen
    let mut s = Seite::default();
    for z in 0..500 {
        for k in 0..40 {
            let (x, y) = (k as f64, z as f64 * 2.0);
            s.pfade.push(linie(&[[x, y], [x + 1.0, y]], [0; 3], 0.1));
        }
    }
    let t = Instant::now();
    let _ = lage::gitterkreuze(&s);
    assert!(t.elapsed().as_secs_f64() < 10.0, "verbinden");
    // Grenzen als Gitter aus 20.000 kurzen Strichen
    let mut g = Vec::new();
    for i in 0..100 {
        for j in 0..100 {
            let (x, y) = (i as f64 * 10.0, j as f64 * 10.0);
            g.push([[x, y], [x + 10.0, y]]);
            g.push([[x, y], [x, y + 10.0]]);
        }
    }
    let t = Instant::now();
    let r = flaeche::kleinste_um(&g, [55.0, 55.0]).unwrap();
    assert!((flaeche::flaeche(&r).abs() - 100.0).abs() < 1e-6);
    assert!(t.elapsed().as_secs_f64() < 10.0, "Graph");
    // ganz außen: kein Überlauf im Raster
    let fern = [
        [[1e300, 1e300], [-1e300, 1e300]],
        [[f64::MAX, 0.0], [0.0, f64::MAX]],
    ];
    let _ = flaeche::kleinste_um(&fern, [0.0, 0.0]);
    // 5000-mal dieselbe Zahl als Zähler und Nenner
    let texte: Vec<Text> = (0..5000)
        .map(|i| text("1", [i as f64 * 0.01, 0.0], 2.0, 0.0))
        .collect();
    let t = Instant::now();
    let _ = nummer_in_karte(&texte, "1/1", &[1, 2, 3]);
    assert!(t.elapsed().as_secs_f64() < 10.0, "Nummer");
}
