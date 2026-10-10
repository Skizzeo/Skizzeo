//! Bauvorbereitung (Titel Baustelleneinrichtung): Pauschalen, Vorhaltung,
//! Bauzaun und Gerüst als Automatikpositionen je Gebäude.

use sk_cost::katalog::{self, Katalog, Umfeld};
use sk_cost::rechnung::Quelle;
use sk_cost::{lesen, Cent, Dez, Kostenblatt, Umfang};
use sk_math::vec3;
use sk_model::{qto, szo, Guid, GuidGen, Model};

const RH1: &str = include_str!("../referenz/rh1-standardhaus.szo");

/// Werks-Guids der elf Sätze im Titel 1.01 (Werksbestand Stand 8), in
/// Positionsfolge 10 … 110.
const SAETZE: [&str; 11] = [
    "1S7bUW001008020000000W",
    "1S7bUW001008020000000X",
    "1S7bUW001008020000000Y",
    "1S7bUW001008020000000Z",
    "1S7bUW001008020000000a",
    "1S7bUW001008020000000b",
    "1S7bUW001008020000000c",
    "1S7bUW001008020000000d",
    "1S7bUW001008020000000e",
    "1S7bUW001008020000000f",
    "1S7bUW001008020000000g",
];

fn rh1() -> Model {
    szo::read_with(RH1, GuidGen::with_seed(1), &lesen::ABSCHNITTE_SZO)
        .expect("lädt")
        .model
}

/// Werksbestand, die Sätze `retired` ausgemustert (wie im Projekt).
fn werk(m: &Model, retired: &[&str]) -> Katalog {
    let zeilen: Vec<(String, String)> = sk_cost::WERK
        .lines()
        .filter_map(|l| {
            let a = l.strip_prefix('[')?.split(']').next()?.to_string();
            let aus = a == "service" && retired.iter().any(|g| l.contains(&format!("guid={g} ")));
            Some((
                a,
                if aus {
                    format!("{l} retired=1")
                } else {
                    l.to_string()
                },
            ))
        })
        .collect();
    let k = katalog::lesen(
        zeilen.iter().map(|(a, l)| (a.as_str(), l.as_str())),
        &Umfeld::aus_modell(m),
        katalog::Quelle::Werk {
            stand: "10/2026".into(),
        },
    );
    assert!(k.befunde.iter().all(|b| b.regel != 79), "{:?}", k.befunde);
    k
}

fn blatt(m: &Model, k: &Katalog) -> Kostenblatt {
    lesen::kosten(m, &qto::schedule(m), k, &Umfang::projekt())
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
    let m = rh1();
    // ohne Projektkopie gilt der eingebaute Werksbestand
    let b = lesen::kosten(
        &m,
        &qto::schedule(&m),
        &lesen::katalog(&m, None),
        &Umfang::projekt(),
    );
    let pos = |g: &str| {
        let g = Guid::from_ifc(g).unwrap();
        b.positionen
            .iter()
            .find(|p| p.quelle == Quelle::Leistung(g))
            .unwrap_or_else(|| panic!("Position {g:?} fehlt: {:?}", b.befunde))
    };
    // Pauschalen: 1, EP aus Lohn 60 €/h + Gerät/Sonstiges
    let be = pos(SAETZE[0]);
    assert_eq!(be.menge, Dez::ganz(1));
    assert_eq!(be.ep, Cent(250_000));
    assert_eq!(be.oz, "1.01.0010");
    assert_eq!(pos(SAETZE[3]).menge, Dez::ganz(1), "Bauschild in St");
    assert_eq!(pos(SAETZE[3]).ep, Cent(60_000));
    // Vorhaltung: Standardhaus mit zwei Geschossen, 3 Monate
    let vorhalten = pos(SAETZE[1]);
    assert_eq!(vorhalten.menge, Dez::ganz(3));
    assert_eq!(vorhalten.gp, Cent(120_000));
    // Bauzaun und Gerüst in m bzw. m² aus dem Gebäude
    let zaun = pos(SAETZE[2]);
    let soll = (wert(&m, "site.fence") / 1e3 * 1000.0).round() as i64 * 1000;
    assert_eq!(zaun.menge, Dez(soll));
    assert!(zaun.menge >= Dez::ganz(60), "{:?}", zaun.menge);
    let geruest = pos(SAETZE[10]);
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
    // Die Vorbemerkung schließt Baustelleneinrichtung nicht mehr aus
    assert!(!sk_cost::WERK.contains("nicht Gegenstand"));
}

/// Ausgemustert im Projekt: Position fällt weg, die übrigen bleiben.
#[test]
fn einzeln_abschalten() {
    let m = rh1();
    let b = blatt(&m, &werk(&m, &[SAETZE[8], SAETZE[10]]));
    let n = b
        .positionen
        .iter()
        .filter(|p| p.oz.starts_with("1.01."))
        .count();
    assert_eq!(n, 9);
    let wc = Guid::from_ifc(SAETZE[8]).unwrap();
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
