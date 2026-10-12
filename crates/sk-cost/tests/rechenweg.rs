//! Rechenweg der Mengenzeilen (vob/plan-rechenweg-bestellmengen.md): An
//! allen Referenzhäusern ergeben die Terme jeder Zeile genau ihre Menge, in
//! der VOB-Sicht wie in der Bestellsicht mit Zuschlag.

use sk_cost::lesen;
use sk_model::qto::schedule;
use sk_model::rechenweg::{anzeige, fuer, wert, Art, Modus};
use sk_model::{szo, GuidGen, Model};

fn laden(text: &str) -> Model {
    szo::read_with(text, GuidGen::with_seed(1), &lesen::ABSCHNITTE_SZO)
        .expect("lädt")
        .model
}

const HAEUSER: [(&str, &str); 5] = [
    ("rh1", include_str!("../referenz/rh1-standardhaus.szo")),
    ("rh2", include_str!("../referenz/rh2-mehrschalig.szo")),
    (
        "rh3",
        include_str!("../referenz/rh3-versatz-dachterrasse.szo"),
    ),
    ("rh4", include_str!("../referenz/rh4-erweiterungen.szo")),
    ("rh5", include_str!("../referenz/rh5-flachdach.szo")),
];

#[test]
fn summe_der_terme_gleich_menge() {
    for (name, text) in HAEUSER {
        let m = laden(text);
        let s = schedule(&m);
        let mut n = 0;
        for b in &s.buildings {
            for t in &b.by_trade {
                for r in &t.rows {
                    for modus in [Modus::Vob, Modus::Bestellung] {
                        for w in fuer(&m, r, modus) {
                            let soll = anzeige(&m, r, w.bezug, modus);
                            assert!(
                                (w.menge() - soll).abs() < 1e-3,
                                "{name} {} Schicht {} {modus:?}: {:?} ≠ {soll}",
                                r.number,
                                r.layer,
                                w
                            );
                            assert!(w.terme.iter().any(|t| t.art == Art::Basis));
                            n += 1;
                        }
                    }
                    assert!(wert(r, sk_model::rechenweg::Bezug::Volumen) >= 0.0);
                }
            }
        }
        assert!(n > 10, "{name}: {n} Rechenwege");
    }
}

/// Wo ein Ausgleich „Ecken und Anschlüsse“ steht, ist er die Ausnahme: Bei
/// allen Zeilen von RH-1 geht die Rechnung ohne ihn auf.
#[test]
fn standardhaus_ohne_ausgleich() {
    let m = laden(HAEUSER[0].1);
    let s = schedule(&m);
    for b in &s.buildings {
        for t in &b.by_trade {
            for r in &t.rows {
                for w in fuer(&m, r, Modus::Vob) {
                    let texte: Vec<&str> = w.terme.iter().map(|t| t.text.as_str()).collect();
                    assert!(
                        !texte.contains(&"Ecken und Anschlüsse"),
                        "{} {:?}: {texte:?}",
                        r.number,
                        w.bezug
                    );
                }
            }
        }
    }
}
