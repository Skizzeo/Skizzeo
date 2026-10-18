//! KA-0 Abnahme Nr. 12 (Fall aus dem Prüfprotokoll): Planstein 17,5
//! (A-PB175) auf 99,00 € im RH-1. Die Vorschau nennt Netto vorher und
//! nachher, ohne das Modell zu ändern: 60.089,83 → 73.323,52 €. Zum Vergleich
//! Lohn 65 €/h: 60.089,83 → 62.501,62 €. Mit Erdarbeiten und
//! Baustelleneinrichtung (Werksbestand Stand 8): 73.035,98 → 86.269,67 €,
//! Lohn 65 €/h → 75.844,69 €. Mit Los 4 Dach (Stand 9, Dachterrasse und
//! Attikablech): 75.953,63 → 89.187,32 €, Lohn 65 €/h → 78.889,33 €. Mit
//! der Dämmung auf der Attika der Dachterrasse (1,781 m² × 27,00 €/m², Jörn
//! 10.10. 16:07): 76.001,72 → 89.235,41 €, Lohn 65 €/h → 78.940,53 €. Mit
//! der Sandplatte (Stand 20, Jörn 18.10.): 79.794,22 → 93.027,91 €, Lohn
//! 65 €/h → 82.806,60 €.

use sk_cost::{lesen, vorschau_kosten, Dez, Op, Rolle, Umfang};
use sk_model::{qto, szo, Guid, GuidGen};

const PB175: &str = "1S7bUW0010080100000002";

#[test]
fn ka0_12_vorschau_planstein_99() {
    let m = szo::read_with(
        include_str!("../referenz/rh1-standardhaus.szo"),
        GuidGen::with_seed(12),
        &lesen::ABSCHNITTE_SZO,
    )
    .unwrap()
    .model;
    let sched = qto::schedule(&m);
    let rev = (m.revision(), m.ext_revision());
    let netto = |ops: &[Op]| {
        vorschau_kosten(&m, &sched, None, Rolle::Admin, ops, &Umfang::projekt())
            .unwrap()
            .netto
            .map(|(a, b)| (a.0, b.0))
    };
    let stein = Op::PreisSetzen {
        artikel: Guid::from_ifc(PB175).unwrap(),
        preis: Some(Dez::ganz(99)),
        stand: "10/2026".into(),
        quelle: "Abnahme 12".into(),
        eingabe: String::new(),
    };
    assert_eq!(netto(&[stein]), Some((7_979_422, 9_302_791)));
    let lohn = Op::FirmenwertSetzen {
        schluessel: "wage".into(),
        wert: Dez::ganz(65),
    };
    assert_eq!(netto(&[lohn]), Some((7_979_422, 8_280_660)));
    assert_eq!((m.revision(), m.ext_revision()), rev, "Modell unverändert");
}
