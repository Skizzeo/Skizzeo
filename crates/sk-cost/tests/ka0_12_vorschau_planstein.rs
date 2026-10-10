//! KA-0 Abnahme Nr. 12 (Fall aus dem Prüfprotokoll): Planstein 17,5
//! (A-PB175) auf 99,00 € im RH-1. Die Vorschau nennt Netto vorher und
//! nachher, ohne das Modell zu ändern: 60.089,83 → 73.323,52 €. Zum Vergleich
//! Lohn 65 €/h: 60.089,83 → 62.501,62 €.

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
    assert_eq!(netto(&[stein]), Some((6_195_388, 7_518_757)));
    let lohn = Op::FirmenwertSetzen {
        schluessel: "wage".into(),
        wert: Dez::ganz(65),
    };
    assert_eq!(netto(&[lohn]), Some((6_195_388, 6_442_692)));
    assert_eq!((m.revision(), m.ext_revision()), rev, "Modell unverändert");
}
