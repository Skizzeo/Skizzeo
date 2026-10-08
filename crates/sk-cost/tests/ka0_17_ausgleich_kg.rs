//! KA-0 Abnahme Nr. 17, Gliederung Kostengruppe (Sollwerte BIM-Integration
//! 10:29, sollwerte-referenzhaeuser.md §1): Ausgleich nach KG RH-1 0,00,
//! RH-2 −1,60 (B60: KG 322 1,689 t + KG 351 3,039 t = 7.564,80 € gegen
//! 4,727 t = 7.563,20 €), RH-3 0,00. Je Position und Teilung gilt die
//! Schranke Teile × (0,0005 × EP + 0,005) €.

use sk_cost::{lesen, Cent, Umfang};
use sk_model::{qto, szo, GuidGen};

#[test]
fn ka0_17_ausgleich_nach_kostengruppe() {
    for (name, text, soll) in [
        ("RH-1", include_str!("../referenz/rh1-standardhaus.szo"), 0),
        (
            "RH-2",
            include_str!("../referenz/rh2-mehrschalig.szo"),
            -160,
        ),
        (
            "RH-3",
            include_str!("../referenz/rh3-versatz-dachterrasse.szo"),
            0,
        ),
    ] {
        let m = szo::read_with(text, GuidGen::with_seed(1), &lesen::ABSCHNITTE_SZO)
            .unwrap()
            .model;
        let b = lesen::kosten(
            &m,
            &qto::schedule(&m),
            &lesen::katalog(&m, None),
            &Umfang::projekt(),
        );
        assert_eq!(b.ausgleich_kg, Cent(soll), "{name}");
        let kg: Cent = b.nach_kg.iter().map(|x| x.1).sum();
        assert_eq!(kg + b.ausgleich_kg, b.netto, "{name}");
        // Schranke je Position: Teil-GP je KG gegen den GP der Position
        for p in &b.positionen {
            let mut teile: Vec<(Option<u16>, i128)> = Vec::new();
            for a in &p.ansatz {
                match teile.iter_mut().find(|t| t.0 == a.kg) {
                    Some(t) => t.1 += a.menge,
                    None => teile.push((a.kg, a.menge)),
                }
            }
            if teile.len() < 2 {
                continue;
            }
            // Teilmenge aus dem Anteil am Mengenansatz (Dez in 10⁻⁶), auf 3 Stellen, × EP auf den Cent
            let ganz: i128 = teile.iter().map(|t| t.1).sum();
            let teil_gp: i128 = teile
                .iter()
                .map(|t| {
                    let dez = p.menge.0 as i128 * t.1 / ganz;
                    let menge3 = (dez + 500) / 1000;
                    (menge3 * p.ep.0 as i128 + 500) / 1000
                })
                .sum();
            let diff = (teil_gp - p.gp.0 as i128).abs() as f64 / 100.0;
            let schranke = teile.len() as f64 * (0.0005 * p.ep.0 as f64 / 100.0 + 0.005);
            assert!(
                diff <= schranke + 1e-9,
                "{name} {}: {diff} > {schranke}",
                p.kurz
            );
        }
    }
}
