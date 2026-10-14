//! Abnahme G1c am Beispiel-Lageplan „Am Lindenhof 7“ (erfunden), Sollwerte
//! aus `gelaende/beispiel/sollwerte-lindenhof-7.md` §G1c. Die Varianten
//! (gepackt, verschlüsselt, Objektstrom, Nachtrag) kommen aus
//! `gelaende/beispiel/erzeuge_lageplan_varianten.py` und müssen dieselbe
//! Seite ergeben.

use sk_gelaende::lageplan::lage::{Einpassung, Lage};
use sk_gelaende::lageplan::{self, Bedeutung, Lageplan};

const ORIGINAL: &[u8] = include_bytes!("daten/lageplan-lindenhof-7.pdf");
const VARIANTEN: [(&str, &[u8]); 5] = [
    (
        "RC4 40 Bit",
        include_bytes!("daten/lageplan-lindenhof-7-rc4-40.pdf"),
    ),
    (
        "RC4 128 Bit",
        include_bytes!("daten/lageplan-lindenhof-7-rc4-128.pdf"),
    ),
    (
        "AES 128 Bit",
        include_bytes!("daten/lageplan-lindenhof-7-aes-128.pdf"),
    ),
    (
        "AES 256 Bit R6",
        include_bytes!("daten/lageplan-lindenhof-7-aes-256.pdf"),
    ),
    (
        "AES 256 Bit R5",
        include_bytes!("daten/lageplan-lindenhof-7-aes-256-r5.pdf"),
    ),
];

/// Grenzpunkte: Nummer, Lage auf dem Blatt in pt, UTM 32 (E, N).
const SOLL: [(&str, [f64; 2], [f64; 2]); 4] = [
    ("1", [237.73, 374.18], [467_108.214, 5_878_741.637]),
    ("2", [372.78, 390.76], [467_132.035, 5_878_744.562]),
    ("3", [353.01, 579.68], [467_128.548, 5_878_777.885]),
    ("4", [211.27, 561.71], [467_103.548, 5_878_774.715]),
];

fn grenzpunkt(plan: &Lageplan, nr: &str) -> [f64; 2] {
    plan.grenzpunkte
        .iter()
        .find(|g| g.nr.as_deref() == Some(nr))
        .unwrap_or_else(|| panic!("Grenzpunkt {nr} fehlt"))
        .lage
}

#[test]
fn grenzpunkte_massstab_und_einpassung() {
    let plan = lageplan::lesen(ORIGINAL).unwrap();
    assert!(plan.hinweise.is_empty(), "{:?}", plan.hinweise);
    assert_eq!(plan.lage, Lage::Massstab(500.0), "1:500 aus dem Kopf");
    assert_eq!(plan.grenzpunkte.len(), 4);
    for (nr, pt, _) in SOLL {
        let p = grenzpunkt(&plan, nr);
        assert!(
            (p[0] - pt[0]).abs() < 0.005 && (p[1] - pt[1]).abs() < 0.005,
            "Grenzpunkt {nr}: {p:?} statt {pt:?}"
        );
    }
    // eingepasst über Grenzpunkt 1 und 3: 2 und 4 auf ±2 cm
    let e = Einpassung::zwei_punkte(
        grenzpunkt(&plan, "1"),
        SOLL[0].2,
        grenzpunkt(&plan, "3"),
        SOLL[2].2,
    )
    .unwrap();
    assert!((e.massstab - 500.0).abs() < 0.5, "Maßstab 1:{}", e.massstab);
    assert!(e.drehung.abs() < 0.05, "Drehung {}°", e.drehung);
    for i in [1, 3] {
        let (nr, _, u) = SOLL[i];
        let d = e.abweichung(grenzpunkt(&plan, nr), u);
        assert!(d < 0.02, "Grenzpunkt {nr}: {d} m");
    }
    // Flurstück aus dem Kopf, die Grenze durch die vier Grenzpunkte
    let f = plan.flurstueck.as_ref().unwrap();
    assert_eq!(f.nummer, "123/4");
    assert_eq!(f.ecken_pt.len(), 4);
    for (_, pt, _) in SOLL {
        assert!(f
            .ecken_pt
            .iter()
            .any(|p| (p[0] - pt[0]).abs() < 0.005 && (p[1] - pt[1]).abs() < 0.005));
    }
    assert!((f.flaeche.unwrap() - 822.87).abs() < 0.1);
    let u = lageplan::unterlage(&plan);
    assert!(u.pfade.iter().any(|(_, b)| *b == Bedeutung::Grenze));
}

#[test]
fn varianten_lesen_sich_gleich() {
    let soll = lageplan::lesen(ORIGINAL).unwrap();
    for (name, roh) in VARIANTEN {
        let ist = lageplan::lesen(roh).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!(ist.seite, soll.seite, "{name}");
        assert_eq!(ist.lage, soll.lage, "{name}");
        assert_eq!(ist.grenzpunkte, soll.grenzpunkte, "{name}");
        assert_eq!(ist.flurstueck, soll.flurstueck, "{name}");
    }
    assert_eq!(
        lageplan::lesen(include_bytes!("daten/lageplan-lindenhof-7-kennwort.pdf"))
            .err()
            .as_deref(),
        Some("Die Datei ist mit Kennwort geschützt.")
    );
}
