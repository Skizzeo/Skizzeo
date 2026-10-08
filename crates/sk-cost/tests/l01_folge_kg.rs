//! BIM-Integration L-01 (landkarte.md §8, Paket KA-0, ka-0-fach §1.5):
//! Eine Folgeposition nimmt ihre KG zuerst aus eigenem `kg=`, sonst von
//! der auslösenden Bauleistung, erst dann vom Bauteil.

use sk_cost::{lesen, Umfang};
use sk_model::{qto, szo, Guid, GuidGen};

/// Werk: „Stb-Decke Ortbeton“ mit den Folgen Schalung und Betonstahl.
const DECKE: &str = "1S7bUW0010080200000003";

#[test]
fn folgeposition_nimmt_kg_der_ausloesenden_bauleistung() {
    let m = szo::read_with(
        include_str!("../referenz/rh1-standardhaus.szo"),
        GuidGen::with_seed(1),
        &lesen::ABSCHNITTE_SZO,
    )
    .unwrap()
    .model;
    let decke = Guid::from_ifc(DECKE).unwrap();
    let mut k = lesen::katalog(&m, None);
    let l = k.leistungen.iter_mut().find(|l| l.guid == decke).unwrap();
    assert_eq!(l.kg, None, "Werk trägt kein kg= an der Decke");
    l.kg = Some(359);
    let folgen: Vec<Guid> = k.folgen_von(decke).map(|f| f.folge).collect();
    assert!(!folgen.is_empty());
    for f in &folgen {
        assert_eq!(k.leistung(*f).unwrap().kg, None, "Folge ohne eigenes kg=");
    }
    let b = lesen::kosten(&m, &qto::schedule(&m), &k, &Umfang::projekt());
    let mut n = 0;
    for p in &b.positionen {
        for a in p.ansatz.iter().filter(|a| a.aus == Some(decke)) {
            assert_eq!(a.kg, Some(359), "{}", a.nummer);
            n += 1;
        }
    }
    assert!(n > 0, "RH-1 hat Folgepositionen der Decke");

    // Eigenes kg= der Folge geht vor
    let f0 = folgen[0];
    k.leistungen.iter_mut().find(|l| l.guid == f0).unwrap().kg = Some(392);
    let b = lesen::kosten(&m, &qto::schedule(&m), &k, &Umfang::projekt());
    let kgs: Vec<Option<u16>> = b
        .positionen
        .iter()
        .flat_map(|p| &p.ansatz)
        .filter(|a| a.aus == Some(decke))
        .map(|a| a.kg)
        .collect();
    assert!(
        kgs.contains(&Some(392)),
        "Folge mit eigenem kg=392: {kgs:?}"
    );
    assert!(
        kgs.contains(&Some(359)),
        "die anderen Folgen weiter 359: {kgs:?}"
    );
    assert!(kgs.iter().all(|k| matches!(k, Some(392 | 359))), "{kgs:?}");
}
