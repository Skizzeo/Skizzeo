//! Bauvorbereitung (Titel Baustelleneinrichtung): Pauschalen, Vorhaltung,
//! Bauzaun und Gerüst als Automatikpositionen je Gebäude.

use sk_cost::rechnung::Quelle;
use sk_cost::{lesen, Cent, Dez, Kostenblatt, Umfang};
use sk_math::vec3;
use sk_model::{qto, szo, Guid, GuidGen, Model};

const RH1: &str = include_str!("../referenz/rh1-standardhaus.szo");
const TITEL: &str = "1S7bUW001008030000000C";
const MAURER: &str = "1S7Wf_00100800000004UQ";
const GERUEST: &str = "1S7Wf_00100800000004WJ";

/// (Guid, Kurztext, Gewerk, pos, Rest) wie im Werksbestand.
const SAETZE: [(&str, &str, &str, u32, &str); 11] = [
    (
        "0000000000000000000B01",
        "Baustelleneinrichtung einrichten und räumen",
        MAURER,
        10,
        "unit=psch basis=auto auto=site.lump hours=16 equip=1540 kg=391",
    ),
    (
        "0000000000000000000B02",
        "Baustelleneinrichtung vorhalten",
        MAURER,
        20,
        "unit=mon basis=auto auto=site.months equip=400 kg=391",
    ),
    (
        "0000000000000000000B03",
        "Bauzaun Mobilzaun h=2,0m aufstellen, vorhalten, räumen",
        MAURER,
        30,
        "unit=m basis=auto auto=site.fence hours=0.1 equip=14.5 kg=391",
    ),
    (
        "0000000000000000000B04",
        "Bauschild liefern, aufstellen, vorhalten, räumen",
        MAURER,
        40,
        "unit=st basis=auto auto=site.lump hours=2 other=480 kg=391",
    ),
    (
        "0000000000000000000B05",
        "Baustromanschluss und Verteiler einrichten und räumen",
        MAURER,
        50,
        "unit=psch basis=auto auto=site.lump hours=4 other=760 kg=391",
    ),
    (
        "0000000000000000000B06",
        "Baustromverteiler vorhalten",
        MAURER,
        60,
        "unit=mon basis=auto auto=site.months equip=70 kg=391",
    ),
    (
        "0000000000000000000B07",
        "Bauwasseranschluss Standrohr einrichten und räumen",
        MAURER,
        70,
        "unit=psch basis=auto auto=site.lump hours=2 other=330 kg=391",
    ),
    (
        "0000000000000000000B08",
        "Bauwasser-Standrohr vorhalten",
        MAURER,
        80,
        "unit=mon basis=auto auto=site.months equip=100 kg=391",
    ),
    (
        "0000000000000000000B09",
        "Toilettenkabine mobil vorhalten, inkl. Reinigung",
        MAURER,
        90,
        "unit=mon basis=auto auto=site.months other=130 kg=391",
    ),
    (
        "0000000000000000000B10",
        "Schnurgerüst herstellen, vorhalten, beseitigen",
        MAURER,
        100,
        "unit=psch basis=auto auto=site.lump hours=6 other=40 kg=391",
    ),
    (
        "0000000000000000000B11",
        "Fassadengerüst LK3 W09, 4 Wochen Standzeit, auf-/abbauen",
        GERUEST,
        110,
        "unit=m2 basis=auto auto=site.scaffold hours=0.1 equip=3 kg=392",
    ),
];

/// Standardhaus mit einer Projektkopie aus Los, Titel und den Sätzen;
/// `retired`: diese Sätze ausgemustert.
fn haus(retired: &[&str]) -> Model {
    let mut t = RH1.to_string();
    t += "[lot] guid=1S7bUW001008030000000A name=\"Rohbau\" nr=\"1\"\n";
    t += &format!(
        "[lot] guid={TITEL} name=\"Baustelleneinrichtung\" nr=\"01\" parent=1S7bUW001008030000000A\n"
    );
    for (g, kurz, gewerk, pos, rest) in SAETZE {
        t += &format!(
            "[service] guid={g} short=\"{kurz}\" trade={gewerk} title={TITEL} pos={pos} {rest}"
        );
        if retired.contains(&g) {
            t += " retired=1";
        }
        t += "\n";
    }
    szo::read_with(&t, GuidGen::with_seed(1), &lesen::ABSCHNITTE_SZO)
        .expect("lädt")
        .model
}

fn blatt(m: &Model) -> Kostenblatt {
    let k = lesen::katalog(m, None);
    assert!(k.befunde.iter().all(|b| b.regel != 79), "{:?}", k.befunde);
    lesen::kosten(m, &qto::schedule(m), &k, &Umfang::projekt())
}

fn wert(m: &Model, key: &str) -> f64 {
    qto::schedule(m)
        .auto
        .iter()
        .filter(|a| a.key == key)
        .map(|a| a.value)
        .sum()
}

#[test]
fn alle_positionen_mit_menge_ep_und_kg() {
    let m = haus(&[]);
    let b = blatt(&m);
    let pos = |g: &str| {
        let g = Guid::from_ifc(g).unwrap();
        b.positionen
            .iter()
            .find(|p| p.quelle == Quelle::Leistung(g))
            .unwrap_or_else(|| panic!("Position {g:?} fehlt: {:?}", b.befunde))
    };
    // Pauschalen: 1, EP aus Lohn 60 €/h + Gerät/Sonstiges
    let be = pos(SAETZE[0].0);
    assert_eq!(be.menge, Dez::ganz(1));
    assert_eq!(be.ep, Cent(250_000));
    assert_eq!(be.oz, "1.01.0010");
    assert_eq!(pos(SAETZE[3].0).menge, Dez::ganz(1), "Bauschild in St");
    assert_eq!(pos(SAETZE[3].0).ep, Cent(60_000));
    // Vorhaltung: Standardhaus mit zwei Geschossen, 3 Monate
    let vorhalten = pos(SAETZE[1].0);
    assert_eq!(vorhalten.menge, Dez::ganz(3));
    assert_eq!(vorhalten.gp, Cent(120_000));
    // Bauzaun und Gerüst in m bzw. m² aus dem Gebäude
    let zaun = pos(SAETZE[2].0);
    let soll = (wert(&m, "site.fence") / 1e3 * 1000.0).round() as i64 * 1000;
    assert_eq!(zaun.menge, Dez(soll));
    assert!(zaun.menge >= Dez::ganz(60), "{:?}", zaun.menge);
    let geruest = pos(SAETZE[10].0);
    assert!(geruest.menge > Dez::ganz(250), "{:?}", geruest.menge);
    assert_eq!(geruest.ep, Cent(900));
    assert!(geruest.ansatz.iter().all(|a| a.kg == Some(392)));
    assert!(zaun
        .ansatz
        .iter()
        .all(|a| a.kg == Some(391) && a.formel.is_some()));
    // Alle elf, je Gebäude ein Ansatz
    let n = b
        .positionen
        .iter()
        .filter(|p| p.oz.starts_with("1.01."))
        .inspect(|p| assert_eq!(p.ansatz.len(), 1, "{}", p.kurz))
        .count();
    assert_eq!(n, 11);
}

/// Ausgemustert im Projekt: Position fällt weg, die übrigen bleiben.
#[test]
fn einzeln_abschalten() {
    let m = haus(&[SAETZE[8].0, SAETZE[10].0]);
    let b = blatt(&m);
    let n = b
        .positionen
        .iter()
        .filter(|p| p.oz.starts_with("1.01."))
        .count();
    assert_eq!(n, 9);
    let wc = Guid::from_ifc(SAETZE[8].0).unwrap();
    assert!(b
        .positionen
        .iter()
        .all(|p| p.quelle != Quelle::Leistung(wc)));
}

/// Die Mengen folgen dem Gebäude: 1 m breiter, Zaun 2 m länger, Gerüst
/// größer; ein Geschoss mehr, ein Monat Vorhaltung mehr.
#[test]
fn mengen_folgen_dem_gebaeude() {
    let neu = |breite: f64, geschosse: u8| {
        let mut m = Model::with_seed(12);
        let b = m.add_building(geschosse);
        let pts = [
            vec3(0.0, 0.0, 0.0),
            vec3(breite, 0.0, 0.0),
            vec3(breite, 8000.0, 0.0),
            vec3(0.0, 8000.0, 0.0),
        ];
        m.build_from_polygon(b, &pts).expect("Gebäude");
        m
    };
    let (a, b, c) = (neu(10_000.0, 2), neu(11_000.0, 2), neu(10_000.0, 3));
    assert!((wert(&b, "site.fence") - wert(&a, "site.fence") - 2000.0).abs() < 1.0);
    assert!(wert(&b, "site.scaffold") > wert(&a, "site.scaffold"));
    assert!(wert(&c, "site.scaffold") > wert(&a, "site.scaffold"));
    assert_eq!(wert(&a, "site.months"), 3.0);
    assert_eq!(wert(&c, "site.months"), 4.0);
    assert_eq!(wert(&a, "site.lump"), 1.0);
}
