//! R7 Nachprüfung Befund I (80c003c): Kopie des Firmenkatalogs ins
//! Projekt. Doppelte Kennungen kommen beide ins Projekt, die erste gilt
//! (Regel 74); Zeilen mit fremdem Schlüssel kommen wörtlich mit; die Datei
//! des Hauses läuft danach bytegleich rund; Rückgängig stellt das Haus
//! bytegleich wieder her, Wiederholen die Kopie.

use sk_cost::{lesen, op, Dez};
use sk_model::{read_szk_with, szo, Direction, GuidGen, Model};

const RH1: &str = include_str!("../referenz/rh1-standardhaus.szo");

fn haus() -> Model {
    let mut m = szo::read_with(RH1, GuidGen::with_seed(1), &lesen::ABSCHNITTE_SZO)
        .unwrap()
        .model;
    m.require_steps();
    m
}

const A1: &str = "[article] guid=1S7bUW0010080100000001 ";

#[test]
fn r7_kopie_doppelt_fremd_kaputt_rueckgaengig() {
    let werk = sk_cost::WERK;
    let erste_a1 = werk.lines().find(|l| l.starts_with(A1)).unwrap();
    let doppelt_a1 = erste_a1.replace("price=15.8", "price=99");
    let s1 = werk.lines().find(|l| l.starts_with("[service] ")).unwrap();
    let s1_fremd = format!("{s1} zukunft=\"a b\"");
    let ohne_kennung = "[article] name=\"ohne Kennung\" unit=m2 price=1";
    let kaputt = "[article] guid=0R7kaputt0000000000001 name=\"kaputt\" unit=Banane price=1";
    let fremd_abschnitt = "[zukunft] guid=0R7zukunft000000000001 x=1";
    let mut firma = werk.replacen(s1, &s1_fremd, 1);
    firma.push_str(&format!(
        "[rate] key=wage num=65\n{doppelt_a1}\n{ohne_kennung}\n{kaputt}\n{fremd_abschnitt}\n"
    ));
    let lib = read_szk_with(&firma, &lesen::ABSCHNITTE_SZK).unwrap();

    let mut m = haus();
    let vorher = szo::write(&m);
    m.begin("Kopie");
    op::kopie_anlegen(&mut m, Some(&lib));
    let t = m.commit().expect("ein Schritt");
    let nachher = szo::write(&m);

    // Doppelte Kennungen: beide Zeilen, in Reihenfolge, die erste gilt
    let lohn: Vec<&str> = m
        .ext("rate")
        .filter(|r| r.id.as_deref() == Some("wage"))
        .map(|r| r.line.as_str())
        .collect();
    assert_eq!(lohn, ["[rate] key=wage num=60", "[rate] key=wage num=65"]);
    let a1: Vec<&str> = m
        .ext("article")
        .filter(|r| r.line.starts_with(A1))
        .map(|r| r.line.as_str())
        .collect();
    assert_eq!(a1, [erste_a1, doppelt_a1.as_str()]);
    let k = lesen::katalog(&m, Some(&lib));
    assert_eq!(k.werte.lohn, Dez::ganz(60), "Regel 74: der erste Lohn gilt");
    let preis = k
        .artikel
        .iter()
        .find(|a| a.guid.to_ifc() == "1S7bUW0010080100000001")
        .and_then(|a| a.preis);
    assert_eq!(
        preis,
        Some(Dez::lesen("15.8", 4).unwrap()),
        "Regel 74: der erste Preis gilt"
    );
    // Fremder Schlüssel an einer gültigen Zeile: wörtlich im Projekt
    assert!(nachher.lines().any(|l| l == s1_fremd), "fremder Schlüssel");
    // Zeile ohne Kennung, ungültige Zeile, fremder Abschnitt: die Kopie
    // nimmt nur gültige, benutzte Sätze mit Kennung (Regeln 73, 87); sie
    // bleiben im Firmenkatalog
    for z in [ohne_kennung, kaputt, fremd_abschnitt] {
        assert!(!nachher.lines().any(|l| l == z), "nicht kopiert: {z}");
        assert!(
            sk_model::write_szk(&lib).lines().any(|l| l == z),
            "im Katalog: {z}"
        );
    }
    // Datei des Hauses rund
    let wieder = szo::read_with(&nachher, GuidGen::with_seed(1), &lesen::ABSCHNITTE_SZO)
        .unwrap()
        .model;
    assert_eq!(szo::write(&wieder), nachher, "Rundlauf nach Kopie");
    // Rückgängig und Wiederholen bytegleich
    m.apply(&t, Direction::Undo);
    assert_eq!(szo::write(&m), vorher, "Rückgängig");
    m.apply(&t, Direction::Redo);
    assert_eq!(szo::write(&m), nachher, "Wiederholen");
}
