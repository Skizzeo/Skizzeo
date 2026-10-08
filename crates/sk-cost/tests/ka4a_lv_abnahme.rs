//! KA-4a Abnahme (paket-ka4 §3, §6 Nr. 4, 7; Koordinator 14:20) an RH-1 bis
//! RH-3, ganzes Projekt:
//! - Die LV aller Lose zusammen ergeben mit Preisen auf den Cent das Netto
//!   des Kostenblatts, zuzüglich der geschätzten Zeilen, die in keinem LV
//!   stehen (nur „nicht ausgeschrieben“ in der Zusammenstellung).
//! - Jede Position mit Bauleistung steht in genau einem LV.
//! - Jede OZ ist im LV eines Loses eindeutig, mit und ohne Untertitel.
//! - Anzeige rechnet nach: Titelsumme = Summe der GP, Zusammenstellung =
//!   Summe der Titel, MwSt. = Netto × Satz auf den Cent.

use sk_cost::lv::{lv_aus, LvWahl};
use sk_cost::rechnung::Quelle;
use sk_cost::{lesen, Cent, Umfang};
use sk_model::{qto, szo, GuidGen};
use std::collections::BTreeSet;

fn haeuser() -> [(&'static str, &'static str); 3] {
    [
        ("RH-1", include_str!("../referenz/rh1-standardhaus.szo")),
        ("RH-2", include_str!("../referenz/rh2-mehrschalig.szo")),
        (
            "RH-3",
            include_str!("../referenz/rh3-versatz-dachterrasse.szo"),
        ),
    ]
}

#[test]
fn ka4a_lv_aller_lose_gleich_netto_und_oz_eindeutig() {
    for (name, text) in haeuser() {
        let m = szo::read_with(text, GuidGen::with_seed(4), &lesen::ABSCHNITTE_SZO)
            .unwrap()
            .model;
        let k = lesen::katalog(&m, None);
        let sched = qto::schedule(&m);
        let b = lesen::kosten(&m, &sched, &k, &Umfang::projekt());
        let lose: Vec<_> = k
            .lose
            .iter()
            .filter(|l| l.parent.is_none() && !l.retired)
            .collect();
        assert!(lose.len() >= 3, "{name}: {} Lose", lose.len());
        let mut summe = 0i64;
        let mut zeilen_im_lv: Vec<usize> = Vec::new();
        for los in &lose {
            for untertitel in [false, true] {
                for preise in [true, false] {
                    let w = LvWahl {
                        los: los.guid,
                        untertitel,
                        preise,
                        heute: None,
                    };
                    let lv = lv_aus(&m, &b, &k, &w);
                    // OZ je Los eindeutig
                    let mut ozs = BTreeSet::new();
                    for t in &lv.titel {
                        for p in &t.positionen {
                            assert!(
                                ozs.insert(p.oz.clone()),
                                "{name} Los {} ut={untertitel} preise={preise}: OZ {} doppelt",
                                los.name,
                                p.oz
                            );
                        }
                    }
                    if !preise {
                        assert!(lv.titel.iter().all(|t| t.summe.is_none()
                            && t.positionen
                                .iter()
                                .all(|p| p.ep.is_none() && p.gp.is_none())));
                        assert_eq!(lv.zusammenstellung.netto, None);
                        continue;
                    }
                    // Anzeige rechnet nach
                    let z = &lv.zusammenstellung;
                    let mut titel_summe = 0i64;
                    for t in lv.titel.iter().filter(|t| !t.positionen.is_empty()) {
                        let s: i64 = t.positionen.iter().filter_map(|p| p.gp).map(|c| c.0).sum();
                        assert_eq!(t.summe, Some(Cent(s)), "{name} Titel {}", t.nr);
                        titel_summe += s;
                        assert!(z
                            .zeilen
                            .iter()
                            .any(|(oz, _, x)| oz == &t.nr && *x == Some(Cent(s))));
                    }
                    if z.unvollstaendig {
                        continue;
                    }
                    assert_eq!(z.netto, Some(Cent(titel_summe)), "{name} {}", los.name);
                    // MwSt. = Netto × Satz auf den Cent (kaufmännisch)
                    let roh = i128::from(titel_summe) * i128::from(z.mwst_satz.0);
                    let teiler = 100 * 1_000_000i128;
                    let mwst = (roh + teiler / 2).div_euclid(teiler) as i64;
                    assert_eq!(z.mwst, Some(Cent(mwst)), "{name}: MwSt.");
                    assert_eq!(
                        z.brutto.map(|c| c.0),
                        z.netto.zip(z.mwst).map(|(n, m)| n.0 + m.0),
                        "{name}: brutto = netto + MwSt."
                    );
                    if !untertitel {
                        summe += titel_summe;
                        for t in &lv.titel {
                            for p in &t.positionen {
                                zeilen_im_lv.extend(&p.blatt);
                            }
                        }
                    }
                }
            }
        }
        // Jede Zeile mit Bauleistung in genau einem LV
        let mut gesehen = zeilen_im_lv.clone();
        gesehen.sort();
        gesehen.dedup();
        assert_eq!(
            gesehen.len(),
            zeilen_im_lv.len(),
            "{name}: Zeile in zwei LV"
        );
        let geschaetzt: i64 = b
            .positionen
            .iter()
            .enumerate()
            .filter(|(i, p)| {
                let im_lv = gesehen.binary_search(i).is_ok();
                assert_eq!(
                    im_lv,
                    matches!(p.quelle, Quelle::Leistung(_)) && p.menge.0 != 0,
                    "{name}: {} {:?}",
                    p.kurz,
                    p.quelle
                );
                !im_lv
            })
            .map(|(_, p)| p.gp.0)
            .sum();
        assert_eq!(
            summe + geschaetzt,
            b.netto.0,
            "{name}: LV aller Lose {summe} + nicht ausgeschrieben {geschaetzt} ≠ Netto {}",
            b.netto.0
        );
        eprintln!(
            "KA4A {name}: {} Lose, LV {:.2} € + nicht im LV {:.2} € = Netto {:.2} €",
            lose.len(),
            summe as f64 / 100.0,
            geschaetzt as f64 / 100.0,
            b.netto.0 as f64 / 100.0
        );
    }
}
