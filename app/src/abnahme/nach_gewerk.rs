use super::*;

// Abnahmetest A188: Mengen „nach Gewerk“ und „nach KG“ (Commit 1b;
// bim/paket-1a-gewerke.md §7 mit Sollwerten, gemessen auf d57e43f).
// Spezifikation: test/abnahme-kategorien.md. Setzt 1a voraus (Adapter
// dort: `Model::trades`, `set_layer_trade`, `trade_by_code`).
//
// Einbau: als `mod nach_gewerk { use super::*; … }` ans Ende von
// app/src/abnahme.rs. Nutzt aus abnahme.rs: gebaeude.
//
// Aussehen der Gliederung im Mengenfenster und die CSV folgen nach der
// Skizze von Einstellungen als Bildvergleich bzw. eigener Test.
//
// Angenommene Namen stehen nur in den Adaptern: `BuildingQto::by_trade`
// mit `TradeSum { trade, volume, area }` (nach `order` sortiert, nur
// vorkommende Gewerke), `BuildingQto::by_kg` mit `KgSum { kg, volume }`.

use sk_model::ElementId;

// ===== Adapter 1b =====

/// Summen nach Gewerk des ersten Gebäudes: (ATV, m³, m²), in der
/// Reihenfolge des Mengenfensters.
fn nach_gewerk(s: &mut Scene) -> Vec<(String, f64, Option<f64>)> {
    let l = s.schedule().clone();
    let m = s.model();
    l.buildings[0]
        .by_trade
        .iter()
        .map(|t| {
            (
                m.trade(t.trade).unwrap().code.clone(),
                r4(t.volume / 1e9),
                t.area.map(|a| r4(a / 1e6)),
            )
        })
        .collect()
}

/// Summen nach KG des ersten Gebäudes: (KG, m³), aufsteigend.
fn nach_kg(s: &mut Scene) -> Vec<(u16, f64)> {
    let l = s.schedule().clone();
    l.buildings[0]
        .by_kg
        .iter()
        .map(|k| (k.kg, r4(k.volume / 1e9)))
        .collect()
}

/// Abweichendes Gewerk an Schicht `i` eines Typs, ein Schritt.
fn schicht_gewerk(s: &mut Scene, code_typ: &str, i: usize, code: &str) -> bool {
    let t = s.model().trade_by_code(code).unwrap();
    let typ = s
        .model()
        .layer_sets()
        .iter()
        .find(|(_, x)| x.code == code_typ)
        .map(|(id, _)| id)
        .unwrap();
    s.edit_model("Gewerk geändert", |m| m.set_layer_trade(typ, i, Some(t)))
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

/// Prüfhaus der BIM-Sollwerte: Dialog, 10 × 8 m, AW-31,5, keine
/// Innenwände; AW-006 gelöst um `d` mm versetzt (0: bündig).
fn pruefhaus(seed: u64, d: f64) -> Scene {
    let mut s = Scene::with_model(Model::with_seed(seed));
    gebaeude(&mut s);
    if d != 0.0 {
        let w = nr(&s, "AW-006");
        assert!(s.edit_model("Kopplung gelöst", |m| m.set_linked(w, false)));
        assert!(s.edit_model("Wand verschoben", |m| m.move_segment(w, d).is_some()));
    }
    s
}

fn g(code: &str, m3: f64, m2: Option<f64>) -> (String, f64, Option<f64>) {
    (code.to_string(), m3, m2)
}

/// Summe aller Baustoffe außer Luft und dem Blech des Attikablechs
/// (m³, 4 Stellen): In m abgerechnete Bauteile tragen kein Volumen zur
/// Gewerksumme bei (BIM-Befund zu 0e8e08d, A198b).
fn summe_baustoffe(s: &mut Scene) -> f64 {
    let l = s.schedule().clone();
    let m = s.model();
    r4(l.buildings[0]
        .by_material
        .iter()
        .filter(|x| {
            !["Luft", "Titanzink 0,7"].contains(&m.material(x.material).unwrap().name.as_str())
        })
        .map(|x| x.volume)
        .sum::<f64>()
        / 1e9)
}

/// A188 (paket-1a §7): Das Mengenfenster summiert nach Gewerk, nur
/// vorkommende Gewerke, in der Reihenfolge des Bauablaufs (18331, 18330,
/// 18345), mit Fläche bei Dämmgewerken. Sollwerte von BIM für bündig,
/// Vorsprung +0,30 (UD über ihre eingebaute Schicht 18345) und
/// Rücksprung −0,30 (vor Paket 2; ab 2a mit DT, AB und Attika: 18338
/// 0,2755 ohne das Blech des AB, mit dem Dämmstreifen auf der Attika, 18345 28,3799 / 205,967 m²). Nach KG im bündigen Fall 322, 331,
/// 335, 351, beim Vorsprung dazu 354. Die Summe über alle Gewerke ist die
/// Summe über alle Baustoffe. Eine Abweichung an der WDVS-Schicht
/// (18330) verschiebt ihre Menge zu 18330, die Summe bleibt.
///
/// Seit Raumbuch RB3 bringt jeder Raum seinen Ausbau mit (je Geschoss ein
/// Wohnzimmer, Fertigmaß 9,34 × 7,34 m, OK FFB 160 mm): Abdichtung 18336
/// (5 mm, nur EG), Putz 18350 an Wand und Decke, Estrich samt Dämmung
/// darunter 18353, Parkett 18356 (20 mm), Anstrich 18363 und Raufaser
/// 18366 ohne Dicke, nur mit Fläche. Die Rohbauwerte von BIM bleiben
/// gleich. Nach KG kommen 324 und 353 (Bodenaufbau EG 155 mm über der
/// Abdichtung, OG 160 mm), 325 (Abdichtung), 336 (Putz innen an der
/// Außenwand) und 354 (Deckenputz 10 mm auf der Rohbaufläche) dazu.
#[test]
fn a188_mengen_nach_gewerk_und_kg() {
    for (seed, d, soll) in [
        (
            188,
            0.0,
            vec![
                g("18331", 57.6407, None),
                g("18330", 31.5225, None),
                g("18336", 0.3428, Some(68.5556)),
                g("18345", 28.3307, Some(205.56)),
                g("18350", 4.0277, Some(314.5534)),
                g("18353", 18.8528, Some(411.3336)),
                g("18356", 2.7422, Some(137.1112)),
                g("18363", 0.0, Some(314.5534)),
                g("18366", 0.0, Some(176.4396)),
            ],
        ),
        (
            1880,
            300.0,
            // Bekleidung unter der UD (W3) zwischen EG-Außenflucht und
            // Innenseite der WDVS-Dämmung, die 70 mm vor ihr herunterläuft
            // (Jörn 14:03): WDVS + 0,70 m², Bekleidung 1,5776 m² × 40 mm
            // Räume je 0,30 m länger
            vec![
                g("18331", 58.9237, None),
                g("18330", 31.7992, None),
                g("18336", 0.3428, Some(68.5556)),
                g("18345", 29.0457, Some(211.093)),
                g("18351", 0.0631, Some(1.5776)),
                g("18350", 4.0796, Some(318.9454)),
                g("18353", 19.2451, Some(419.7396)),
                g("18356", 2.7983, Some(139.9132)),
                g("18363", 0.0, Some(318.9454)),
                g("18366", 0.0, Some(178.0206)),
            ],
        ),
    ] {
        let mut s = pruefhaus(seed, d);
        assert_eq!(nach_gewerk(&mut s), soll, "Versatz {d}");
        let summe: f64 = soll.iter().map(|x| x.1).sum();
        assert!(
            (summe - summe_baustoffe(&mut s)).abs() < 2e-4,
            "Versatz {d}"
        );
    }

    // Rücksprung −0,30: ab Paket 2a entstehen DT und AB
    // (Regel 41, lichte Tiefe 0,16 m) mit Gewerk 18338, und das EG-WDVS
    // läuft als Attika bis +3,055 (A195). Werte gemessen mit
    // geometrie/d1-d3-dachterrasse-v2.patch auf 4d5c8a4. 18338 ohne das
    // Blech des AB (0,0019 m³, in m abgerechnet): Dämmung hart 0,1244 +
    // Belag 0,0933, dazu der Dämmstreifen 40 mm auf der Attika 0,0578
    // (Jörn 10.10. 16:07). Die Fassung vor 2a (18345 28,0909 / 203,847 m²
    // ohne DT) ist mit 2a auf main entfallen; die Räume sind 0,30 m kürzer.
    let mut s = pruefhaus(1881, -300.0);
    assert!(s
        .model()
        .elements()
        .iter()
        .any(|(_, e)| e.number.starts_with("DT-")));
    let ist = nach_gewerk(&mut s);
    let v: Vec<(String, f64)> = ist.iter().map(|x| (x.0.clone(), x.1)).collect();
    assert_eq!(
        v,
        [
            ("18331".to_string(), 56.9992),
            ("18330".to_string(), 31.2458),
            ("18336".to_string(), 0.3428),
            ("18338".to_string(), 0.2755),
            ("18345".to_string(), 28.3799),
            ("18350".to_string(), 3.9759),
            ("18353".to_string(), 18.4605),
            ("18356".to_string(), 2.6862),
            ("18363".to_string(), 0.0),
            ("18366".to_string(), 0.0),
        ]
    );
    assert_eq!(ist[4].2, Some(205.967), "WDVS-Fläche mit Attika");
    let summe: f64 = ist.iter().map(|x| x.1).sum();
    assert!((summe - summe_baustoffe(&mut s)).abs() < 3e-4, "Rücksprung");

    let mut s = pruefhaus(1882, 0.0);
    assert_eq!(
        nach_kg(&mut s),
        [
            (322, 24.6238),
            (324, 10.6261),
            (325, 0.3428),
            (331, 31.5225),
            (335, 28.3307),
            (336, 2.6466),
            (351, 33.0169),
            (353, 10.9689),
            (354, 1.3811)
        ]
    );
    let mut s = pruefhaus(1883, 300.0);
    let kg = nach_kg(&mut s);
    // UD 0,3499 und Bekleidung darunter 0,0631 (W3, bis an die WDVS-Dämmung),
    // dazu der Deckenputz der zwei Räume 1,4093
    assert!(kg.contains(&(354, 1.8223)), "{kg:?}");

    // Abweichung an der Schicht: WDVS zu 18330
    let mut s = pruefhaus(1884, 0.0);
    assert!(schicht_gewerk(&mut s, "AW-31,5", 0, "18330"));
    let v: Vec<(String, f64)> = nach_gewerk(&mut s)
        .into_iter()
        .map(|x| (x.0, x.1))
        .collect();
    assert_eq!(
        v,
        [
            ("18331".to_string(), 57.6407),
            ("18330".to_string(), 59.8532),
            ("18336".to_string(), 0.3428),
            ("18350".to_string(), 4.0277),
            ("18353".to_string(), 18.8528),
            ("18356".to_string(), 2.7422),
            ("18363".to_string(), 0.0),
            ("18366".to_string(), 0.0),
        ]
    );
    assert!(s.undo());
    assert_eq!(nach_gewerk(&mut s).len(), 9);
}
