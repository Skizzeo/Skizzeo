//! Abnahme Befunde (Koordinator 01:24, vor der neuen Darstellung der
//! Befunde): Regelnummern und Zählung bleiben gleich. Je Referenzhaus
//! RH-1 bis RH-3 stehen die Befunde (Regel, Schwere, Satz) im
//! Kostenblatt (Rechnung), im Katalog und im LV jedes Loses fest, dazu
//! das Netto. Eine neue Ordnung oder Zusammenfassung in der Anzeige (etwa
//! eine Meldung je Dachterrasse) darf an dieser Zählung nichts ändern.
//! Seit Werksbestand Stand 9 hat die Dachterrasse Bauleistungen; ihre
//! Befunde „Ohne Bauleistung“ sind damit entfallen.

use super::*;
use sk_cost::{Befund, Schwere, Umfang};

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

fn lesen(text: &str) -> Model {
    sk_model::szo::read_with(
        text,
        sk_model::GuidGen::with_seed(1),
        &sk_cost::lesen::ABSCHNITTE_SZO,
    )
    .expect("lädt")
    .model
}

/// Befunde als sortierte Zeilen „Regel Schwere Satz“.
fn zeilen(wo: &str, b: &[Befund]) -> Vec<String> {
    let mut v: Vec<String> = b
        .iter()
        .map(|x| {
            let s = match x.schwere {
                Schwere::Hinweis => "H",
                Schwere::Warnung => "W",
                Schwere::Fehler => "F",
            };
            format!("{wo} {} {s} {}", x.regel, x.satz)
        })
        .collect();
    v.sort();
    v
}

fn stand(text: &str) -> (i64, Vec<String>) {
    let mut s = Scene::with_model(lesen(text));
    let blatt = s.kostenblatt(None, &Umfang::projekt());
    let mut aus = zeilen("blatt", &blatt.befunde);
    let kat = s.katalog(None);
    aus.extend(zeilen("katalog", &kat.befunde));
    let lose: Vec<Guid> = kat
        .lose
        .iter()
        .filter(|l| l.parent.is_none() && !l.retired)
        .map(|l| l.guid)
        .collect();
    for (i, los) in lose.iter().enumerate() {
        let mut v = AvaView::new();
        (v.w, v.h) = (1400, 900);
        v.datei = "haus.szo".into();
        v.los = Some(*los);
        v.sync(&mut s, None);
        let lv = v.lv.as_deref().expect("LV");
        aus.extend(zeilen(&format!("los{}", i + 1), &lv.befunde));
    }
    (blatt.netto.0, aus)
}

/// Zählung je (Ort, Regel, Schwere) ohne den Satz: „ort regel schwere ×n“.
fn zaehlung(zeilen: &[String]) -> Vec<String> {
    let mut m = std::collections::BTreeMap::<String, usize>::new();
    for z in zeilen {
        let k: Vec<&str> = z.splitn(4, ' ').take(3).collect();
        *m.entry(k.join(" ")).or_default() += 1;
    }
    m.into_iter().map(|(k, n)| format!("{k} ×{n}")).collect()
}

#[test]
fn abnahme_befunde_regeln_und_zaehlung() {
    // Seit Werksbestand Stand 9 (Los 4 Dach) haben Dachterrasse und
    // Attikablech Werksleistungen: keine Befunde der Rechnung, Katalog ohne
    // Befunde, je Los nur die zwei Kopf-Hinweise (Regel 0)
    for (name, text, netto_soll) in [
        (haeuser()[0].0, haeuser()[0].1, 7_595_363),
        (haeuser()[1].0, haeuser()[1].1, 8_291_878),
        (haeuser()[2].0, haeuser()[2].1, 8_160_685),
    ] {
        let (netto, ist) = stand(text);
        assert_eq!(netto, netto_soll, "{name}: Netto");
        let blatt: Vec<&str> = ist
            .iter()
            .filter(|z| z.starts_with("blatt "))
            .map(String::as_str)
            .collect();
        assert!(blatt.is_empty(), "{name}: Befunde der Rechnung {blatt:#?}");
        let soll: Vec<String> = ["los1", "los2", "los3", "los4", "los5"]
            .iter()
            .map(|los| format!("{los} 0 H ×2"))
            .collect();
        assert_eq!(zaehlung(&ist), soll, "{name}: Regeln und Zählung\n{ist:#?}");
    }
}
