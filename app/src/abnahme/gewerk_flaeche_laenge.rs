use super::*;

// Abnahmetest A198: Fläche und Länge je Gewerk (Mengenregel nach Paket 2,
// BIM-Vorschlag, vom Koordinator 10:46 übernommen). Spezifikation:
// test/abnahme-dachterrasse.md. Setzt 1b und 2a–2c voraus.
//
// Einbau: als `mod gewerk_flaeche_laenge { use super::*; … }` ans Ende
// von app/src/abnahme.rs. Nutzt aus abnahme.rs: gebaeude, r4.
//
// Angenommener Name nur im Adapter: `TradeSum::length` (mm, `None`, wenn
// kein Bauteil des Gewerks in m abgerechnet wird).

use sk_model::ElementId;

// ===== Adapter =====

/// Je Gewerk des ersten Gebäudes: (ATV, Fläche m², Länge m).
fn flaeche_laenge(s: &mut Scene) -> Vec<(String, Option<f64>, Option<f64>)> {
    let l = s.schedule().clone();
    let m = s.model();
    l.buildings[0]
        .by_trade
        .iter()
        .map(|t| {
            (
                m.trade(t.trade).unwrap().code.clone(),
                t.area.map(|a| r4(a / 1e6)),
                t.length.map(|x| r4(x / 1e3)),
            )
        })
        .collect()
}

// ===== Hilfen =====

fn nr(s: &Scene, nummer: &str) -> ElementId {
    s.model()
        .elements()
        .iter()
        .find(|(_, e)| e.number == nummer)
        .map(|(id, _)| id)
        .unwrap_or_else(|| panic!("{nummer} fehlt"))
}

/// Prüfhaus 10 × 8 m, AW-31,5, keine Innenwände; die OG-Wände `rueck`
/// gelöst und um 1,50 m zurückgesetzt.
fn pruefhaus(seed: u64, rueck: &[&str]) -> Scene {
    let mut s = Scene::with_model(Model::with_seed(seed));
    gebaeude(&mut s);
    for w in rueck {
        let id = nr(&s, w);
        assert!(s.edit_model("Kopplung gelöst", |m| m.set_linked(id, false)));
        assert!(s.edit_model("Wand verschoben", |m| m.move_segment(id, -1500.0).is_some()));
    }
    assert!(s.model().check().is_empty(), "{:?}", s.model().check());
    s
}

fn eintrag(v: &[(String, Option<f64>, Option<f64>)], code: &str) -> (Option<f64>, Option<f64>) {
    v.iter()
        .find(|x| x.0 == code)
        .map(|x| (x.1, x.2))
        .unwrap_or_else(|| panic!("Gewerk {code} fehlt: {v:?}"))
}

/// A198 (Mengenregel nach Paket 2): Die Gewerkfläche ist die Summe der
/// Bauteilflächen, je Bauteil einmal, für Bauteile, die in m² abgerechnet
/// werden (DT, UD, Dämmschichten); die Gewerklänge kommt aus Bauteilen in
/// m (AB, Abfangung). Nord −1,50: 18338 hat 13,2192 m² (DT einmal, nicht
/// je Schicht) und 13,00 m (AB). 18345 behält die Fläche der
/// WDVS-Schicht mit Attika (199,595 m², gemessen mit
/// geometrie/d1-d3-dachterrasse-v2.patch auf f1649a6) und hat keine
/// Länge. Beton und Mauerwerk haben weder Fläche noch Länge. Nord+Süd:
/// zwei Stücke, ein DT, 26,4384 m² und 26,00 m; ringsum 40,0384 m² und
/// 36,00 m.
#[test]
fn a198_flaeche_und_laenge_je_gewerk() {
    let mut s = pruefhaus(198, &["AW-006"]);
    let v = flaeche_laenge(&mut s);
    assert_eq!(eintrag(&v, "18338"), (Some(13.2192), Some(13.0)));
    assert_eq!(eintrag(&v, "18345"), (Some(199.595), None));
    assert_eq!(eintrag(&v, "18331"), (None, None));
    assert_eq!(eintrag(&v, "18330"), (None, None));

    for (seed, rueck, flaeche, laenge) in [
        (1980, &["AW-006", "AW-008"][..], 26.4384, 26.0),
        (
            1981,
            &["AW-005", "AW-006", "AW-007", "AW-008"][..],
            40.0384,
            36.0,
        ),
    ] {
        let mut s = pruefhaus(seed, rueck);
        let v = flaeche_laenge(&mut s);
        assert_eq!(
            eintrag(&v, "18338"),
            (Some(flaeche), Some(laenge)),
            "{rueck:?}"
        );
    }

    // Ohne Rücksprung kein 18338
    let mut s = pruefhaus(1982, &[]);
    assert!(flaeche_laenge(&mut s).iter().all(|x| x.0 != "18338"));
}

/// Je Gewerk des ersten Gebäudes: (ATV, Volumen m³).
fn volumen(s: &mut Scene) -> Vec<(String, f64)> {
    let l = s.schedule().clone();
    let m = s.model();
    l.buildings[0]
        .by_trade
        .iter()
        .map(|t| (m.trade(t.trade).unwrap().code.clone(), r4(t.volume / 1e9)))
        .collect()
}

/// A198b (BIM-Befund zu 0e8e08d, Koordinator 11:56): Bauteile, die in m
/// abgerechnet werden, tragen kein Volumen zur Gewerksumme bei. Nord
/// −1,50: 18338 = 1,8507 m³ (Dämmung 1,0575 + Belag 0,7932; ohne das
/// Blech des AB, 0,0023 m³; auf 0e8e08d 1,8530), dazu 13,2192 m² und
/// 13,00 m. Abfangung (AW-49, OG-Nord +0,30, wie A159): 18330 hat
/// 10,485 m Abfangung und als Volumen genau die Summe seiner
/// Wandschichten Verblender, Kerndämmung und Porenbeton, 81,5458 m³; die
/// Abfangung bringt kein eigenes Volumen dazu (die Verblenderschicht der
/// Wände zählt weiter). Die UD-Kerndämmung (0,3373 m³) zählt bei 18345.
#[test]
fn a198b_meter_bauteile_ohne_volumen() {
    let mut s = pruefhaus(1983, &["AW-006"]);
    let v = volumen(&mut s);
    let dd = v.iter().find(|x| x.0 == "18338").expect("18338").1;
    assert_eq!(dd, 1.8507, "18338 ohne Blechvolumen");
    assert_eq!(
        eintrag(&flaeche_laenge(&mut s), "18338"),
        (Some(13.2192), Some(13.0))
    );

    let mut s = Scene::with_model(Model::with_seed(1984));
    let (eg, _) = gebaeude(&mut s);
    let t = s
        .model()
        .layer_sets()
        .iter()
        .find(|(_, t)| t.code == "AW-49")
        .map(|(id, _)| id)
        .expect("Werkstyp AW-49");
    assert!(s.edit_model("Wandtyp geändert", |m| m.set_run_type(eg, t)));
    let w = nr(&s, "AW-006");
    assert!(s.edit_model("Kopplung gelöst", |m| m.set_linked(w, false)));
    assert!(s.edit_model("Wand verschoben", |m| m.move_segment(w, 300.0).is_some()));
    let fl = flaeche_laenge(&mut s);
    assert_eq!(eintrag(&fl, "18330").1, Some(10.485), "Abfangung");
    let v = volumen(&mut s);
    let maurer = v.iter().find(|x| x.0 == "18330").expect("18330").1;
    // 81,5458 + Verblender unter die Bekleidung (W3): 9,885 × 0,115 × 0,07
    assert_eq!(maurer, 81.6254, "nur Wandschichten, kein Abfangungsvolumen");
    let wdvs = v.iter().find(|x| x.0 == "18345").expect("18345").1;
    assert_eq!(wdvs, 0.3373, "UD-Kerndämmung");
}
