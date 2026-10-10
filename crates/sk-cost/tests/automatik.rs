//! Automatikmengen (Erdarbeiten, Bauvorbereitung): Bauleistungen mit
//! `auto=` rechnen mit den Mengen der Gründung, ohne Schicht.

use sk_cost::rechnung::Quelle;
use sk_cost::{lesen, Dez, Kostenblatt, Umfang};
use sk_model::{qto, szo, Guid, GuidGen, Model};

const RH1: &str = include_str!("../referenz/rh1-standardhaus.szo");
const TITEL: &str = "1S7bUW001008030000000B";
const ERDBAU: &str = "1S7Wf_00100800000004Ty";

/// Standardhaus mit einer Projektkopie aus Los, Titel und `zeilen`.
fn haus(zeilen: &[String]) -> Model {
    let mut t = RH1.to_string();
    t += "[lot] guid=1S7bUW001008030000000A name=\"Rohbau\" nr=\"1\"\n";
    t += &format!(
        "[lot] guid={TITEL} name=\"Erdarbeiten\" nr=\"02\" parent=1S7bUW001008030000000A\n"
    );
    for z in zeilen {
        t += z;
        t += "\n";
    }
    szo::read_with(&t, GuidGen::with_seed(1), &lesen::ABSCHNITTE_SZO)
        .expect("lädt")
        .model
}

fn leistung(guid: &str, kurz: &str, pos: u32, rest: &str) -> String {
    format!("[service] guid={guid} short=\"{kurz}\" trade={ERDBAU} title={TITEL} pos={pos} {rest}")
}

fn blatt(m: &Model) -> Kostenblatt {
    let k = lesen::katalog(m, None);
    lesen::kosten(m, &qto::schedule(m), &k, &Umfang::projekt())
}

const OBERBODEN: &str = "1S7bUW001008020000000N";
const AUSHUB: &str = "1S7bUW001008020000000O";

#[test]
fn automatikposition_aus_der_gruendung() {
    let m = haus(&[
        leistung(
            OBERBODEN,
            "Oberboden abtragen",
            10,
            "unit=m3 basis=auto auto=earth.topsoil hours=0.1 kg=311",
        ),
        leistung(
            AUSHUB,
            "Aushub",
            20,
            "unit=m3 basis=auto auto=earth.trench equip=12",
        ),
    ]);
    let s = qto::schedule(&m);
    let summe = |k: &str| -> f64 {
        s.auto
            .iter()
            .filter(|a| a.key == k)
            .map(|a| a.value)
            .sum::<f64>()
    };
    assert!(summe("earth.topsoil") > 0.0, "{:?}", s.auto);
    let b = blatt(&m);
    let g = Guid::from_ifc(OBERBODEN).unwrap();
    let p = b
        .positionen
        .iter()
        .find(|p| p.quelle == Quelle::Leistung(g))
        .expect("Oberboden als Position");
    // Menge auf 3 Stellen aus der Automatikmenge, KG der Bauleistung
    let soll = (summe("earth.topsoil") / 1e9 * 1000.0).round() as i64 * 1000;
    assert_eq!(p.menge, Dez(soll));
    assert_eq!(p.oz, "1.02.0010");
    assert!(p
        .ansatz
        .iter()
        .all(|a| a.kg == Some(311) && a.formel.is_some()));
    // Ohne KG an der Bauleistung gilt die der Menge (311)
    let a = Guid::from_ifc(AUSHUB).unwrap();
    let q = b
        .positionen
        .iter()
        .find(|p| p.quelle == Quelle::Leistung(a))
        .expect("Graben als Position");
    assert!(q.ansatz.iter().all(|a| a.kg == Some(311)));
    assert_eq!(q.ep.0, 1200);
}

/// Ausgemustert: keine Position (so schaltet ein Projekt sie ab).
#[test]
fn ausgemustert_rechnet_nicht() {
    let m = haus(&[leistung(
        OBERBODEN,
        "Oberboden abtragen",
        10,
        "unit=m3 basis=auto auto=earth.topsoil retired=1",
    )]);
    let g = Guid::from_ifc(OBERBODEN).unwrap();
    assert!(blatt(&m)
        .positionen
        .iter()
        .all(|p| p.quelle != Quelle::Leistung(g)));
}

/// Einheit passt nicht zur Menge: Hinweis, keine Position. `basis=auto`
/// ohne `auto=` gilt nicht.
#[test]
fn einheit_und_pflichtfeld() {
    let m = haus(&[
        leistung(
            OBERBODEN,
            "Oberboden abtragen",
            10,
            "unit=m2 basis=auto auto=earth.topsoil",
        ),
        leistung(AUSHUB, "Aushub", 20, "unit=m3 basis=auto"),
    ]);
    let b = blatt(&m);
    assert!(b.positionen.is_empty(), "{:?}", b.positionen);
    assert!(b.befunde.iter().any(|x| x.regel == 80), "{:?}", b.befunde);
    let k = lesen::katalog(&m, None);
    assert!(k.befunde.iter().any(|x| x.regel == 79), "{:?}", k.befunde);
}

/// Umfang: ein Gebäude ohne Gründungsband rechnet keine Erdarbeiten.
#[test]
fn umfang_ohne_gruendung() {
    let m = haus(&[leistung(
        OBERBODEN,
        "Oberboden abtragen",
        10,
        "unit=m3 basis=auto auto=earth.topsoil",
    )]);
    let s = qto::schedule(&m);
    let gr = s.auto[0].storey;
    let u = Umfang {
        gebaeude: None,
        ohne: vec![gr],
    };
    let k = lesen::katalog(&m, None);
    let b = lesen::kosten(&m, &s, &k, &u);
    assert!(b.positionen.is_empty());
}

/// Prüfung 10.10. (Briefing QS §4): Ein unbekannter Schlüssel an `auto=`
/// liefert nie eine Menge. Das darf nicht still bleiben, sondern gibt einen
/// Befund mit der Kurzbezeichnung der Bauleistung.
#[test]
fn unbekannter_auto_schluessel_gibt_befund() {
    let m = haus(&[leistung(
        OBERBODEN,
        "Oberboden abtragen",
        10,
        "unit=m3 basis=auto auto=earth.tippfehler hours=0.1 kg=311",
    )]);
    let k = lesen::katalog(&m, None);
    let bf = lesen::befunde(&m, &k);
    assert!(
        bf.iter()
            .any(|b| b.satz.contains("Oberboden abtragen") && b.satz.contains("earth.tippfehler")),
        "{:?}",
        bf.iter().map(|b| &b.satz).collect::<Vec<_>>()
    );
}
