//! Abnahme KA-4b (paket-ka4 §6 Nr. 4, 5, 9, 12, 14) an RH-1 bis RH-3, an
//! den Zeilen, wie das Blatt AVA sie zeigt:
//! - Nr. 4: GP = angezeigte Menge × EP auf den Cent, Titelsumme = Summe
//!   ihrer Zeilen, Zusammenstellung = Summe der Titel, MwSt. = Netto × 19 %
//!   auf den Cent, brutto = netto + MwSt.; mit und ohne Untertitel.
//! - Summe netto aller Lose = Netto des Kostenblatts abzüglich der
//!   geschätzten Zeilen (nicht ausgeschrieben).
//! - Nr. 5: „Für Anfrage (leer)“ zeigt keine Zahl in EP, GP und Summen.
//! - Nr. 9: leere Titel stehen nicht in der Tabelle.
//! - Nr. 12/14: erstes Öffnen nur Lose und Titel, kein Detail; Karte
//!   „LV {Los} · n Pos.“.

use super::*;
use sk_cost::lv::LvWahl;
use sk_cost::Umfang;

fn haeuser() -> [(&'static str, &'static str); 3] {
    [
        (
            "RH-1",
            include_str!("../../../crates/sk-cost/referenz/rh1-standardhaus.szo"),
        ),
        (
            "RH-2",
            include_str!("../../../crates/sk-cost/referenz/rh2-mehrschalig.szo"),
        ),
        (
            "RH-3",
            include_str!("../../../crates/sk-cost/referenz/rh3-versatz-dachterrasse.szo"),
        ),
    ]
}

fn szene(text: &str) -> Scene {
    let m = sk_model::szo::read_with(
        text,
        sk_model::GuidGen::with_seed(1),
        &sk_cost::lesen::ABSCHNITTE_SZO,
    )
    .expect("lädt")
    .model;
    Scene::with_model(m)
}

/// „1.234,567“ → 1234567 (Stellen hinter dem Komma als ganze Zahl).
fn zahl(s: &str) -> Option<i64> {
    let t: String = s
        .chars()
        .filter(|c| c.is_ascii_digit() || *c == '−' || *c == '-')
        .collect();
    if t.is_empty() || !s.contains(',') {
        return None;
    }
    let neg = t.starts_with('−') || t.starts_with('-');
    let d: i64 = t.trim_start_matches(['−', '-']).parse().ok()?;
    Some(if neg { -d } else { d })
}

#[test]
fn abnahme_ka4b_anzeige_rechnet_nach() {
    for (name, text) in haeuser() {
        let mut s = szene(text);
        let u = Umfang::projekt();
        let kat = s.katalog(None);
        let lose: Vec<Guid> = kat
            .lose
            .iter()
            .filter(|l| l.parent.is_none() && !l.retired)
            .map(|l| l.guid)
            .collect();
        let blatt = s.kostenblatt(None, &u);
        for untertitel in [false, true] {
            let mut netto_alle = 0i64;
            let mut geschaetzt = 0i64;
            let mut unvollstaendig = false;
            for los in &lose {
                let w = LvWahl {
                    los: *los,
                    untertitel,
                    preise: true,
                    heute: None,
                };
                let lv = s.lv(None, &u, &w);
                let z = lv_zeilen(&lv);
                let wo = |x: &Zeile| format!("{name} ut={untertitel} {} {}", x.oz, x.text);
                // Nr. 9: kein leerer Titel in der Tabelle
                let titel_mit = lv.titel.iter().filter(|t| !t.positionen.is_empty()).count();
                assert_eq!(z.iter().filter(|x| x.art == Art::Titel).count(), titel_mit);
                // Nr. 4: GP = Menge × EP; Titel = Summe seiner Positionen
                let mut titel_gp: Option<(String, i64)> = None;
                let mut im_titel = 0i64;
                let mut titel_summen = Vec::new();
                for x in z
                    .iter()
                    .chain(std::iter::once(&Zeile::neu(Art::Titel, "", "")))
                {
                    match x.art {
                        Art::Titel => {
                            if let Some((t, gp)) = titel_gp.take() {
                                assert_eq!(gp, im_titel, "{name} ut={untertitel} Titel {t}");
                                titel_summen.push(gp);
                            }
                            if !x.oz.is_empty() || !x.text.is_empty() {
                                if let Some(gp) = zahl(&x.gp) {
                                    titel_gp = Some((x.oz.clone(), gp));
                                }
                            }
                            im_titel = 0;
                        }
                        Art::Position => {
                            let (Some(m), Some(ep), Some(gp)) =
                                (zahl(&x.menge), zahl(&x.ep), zahl(&x.gp))
                            else {
                                continue;
                            };
                            // Menge in Tausendsteln, EP in Cent → Cent
                            let roh = m * ep;
                            let erwartet = (roh + if roh < 0 { -500 } else { 500 }) / 1000;
                            assert_eq!(gp, erwartet, "{}: {} × {}", wo(x), x.menge, x.ep);
                            im_titel += gp;
                        }
                        _ => {}
                    }
                }
                // Zusammenstellung
                let zs = zusammenstellung(&lv);
                let summe = |t: &str| {
                    zs.iter()
                        .find(|x| x.art == Art::Summe && x.text.starts_with(t))
                        .and_then(|x| zahl(&x.gp))
                };
                let netto = summe("Summe netto").unwrap_or(0);
                let titel_in_zs: i64 = zs
                    .iter()
                    .filter(|x| x.art == Art::Position)
                    .filter_map(|x| zahl(&x.gp))
                    .sum();
                assert_eq!(
                    titel_in_zs, netto,
                    "{name} ut={untertitel} Zusammenstellung"
                );
                assert_eq!(
                    titel_summen.iter().sum::<i64>(),
                    netto,
                    "{name} ut={untertitel} Titel = Netto"
                );
                if netto != 0 {
                    let mwst = summe("MwSt.").expect("MwSt.");
                    assert_eq!(mwst, (netto * 19 + 50) / 100, "{name} MwSt.");
                    assert_eq!(summe("Summe brutto"), Some(netto + mwst), "{name} brutto");
                }
                unvollstaendig |= lv.zusammenstellung.unvollstaendig;
                netto_alle += netto;
                geschaetzt += lv.zusammenstellung.geschaetzt.map_or(0, |c| c.0);

                // Nr. 5: Für Anfrage (leer) ohne Zahl in EP, GP, Summen
                let w0 = LvWahl {
                    preise: false,
                    ..w.clone()
                };
                let leer = s.lv(None, &u, &w0);
                for x in lv_zeilen(&leer)
                    .iter()
                    .chain(zusammenstellung(&leer).iter())
                {
                    assert!(
                        !x.ep.chars().any(|c| c.is_ascii_digit())
                            && !x.gp.chars().any(|c| c.is_ascii_digit()),
                        "Anfrage {}: EP „{}“ GP „{}“",
                        wo(x),
                        x.ep,
                        x.gp
                    );
                }
            }
            let _ = geschaetzt;
            // LV aller Lose = Netto des Kostenblatts ohne die Zeilen, die in
            // keinem LV stehen (ohne Untertitel; mit Untertiteln rundet je
            // Untertitelposition)
            if !untertitel && !unvollstaendig {
                let im_lv: i64 = blatt
                    .positionen
                    .iter()
                    .filter(|p| {
                        matches!(p.quelle, sk_cost::rechnung::Quelle::Leistung(_)) && p.menge.0 != 0
                    })
                    .map(|p| p.gp.0)
                    .sum();
                assert_eq!(
                    netto_alle, im_lv,
                    "{name}: Σ LV netto aller Lose ≠ Kostenblatt (Positionen im LV)"
                );
                eprintln!("KA4B {name}: Σ LV netto {:.2} €", netto_alle as f64 / 100.0);
            }
        }
    }
}

#[test]
fn abnahme_ka4b_erstes_oeffnen() {
    for (name, text) in haeuser() {
        let mut s = szene(text);
        let mut v = AvaView::new();
        (v.w, v.h) = (1400, 900);
        v.top = 120.0;
        v.sync(&mut s, None);
        assert!(v.detail.is_none(), "{name}: Detail zu");
        assert!(v.gewaehlt.is_none(), "{name}: nichts gewählt");
        let lv = v.lv.as_deref().expect("LV").clone();
        assert_eq!(
            v.karte,
            format!("LV {} · {} Pos.", lv.kopf.los, lv.anzahl())
        );
        // Nur das erste Los offen, die anderen nur als Zeile
        let offen: Vec<_> = v
            .baum
            .iter()
            .filter_map(|k| match k {
                Knoten::Los { offen, .. } => Some(*offen),
                _ => None,
            })
            .collect();
        assert_eq!(offen.iter().filter(|o| **o).count(), 1, "{name}: {offen:?}");
        assert!(offen[0], "{name}: erstes Los offen");
        assert!(matches!(v.ansicht, Ansicht::Lv));
    }
}
