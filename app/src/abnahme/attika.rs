use super::*;

// Abnahmetest A195: Attika, die EG-Schichten außerhalb der Deckenkante
// bis OK Attika (Commit 2b; bim/paket-dachterrasse.md E2, Regel 45,
// Sollwerte 08:54; geometrie/machbarkeit-dachterrasse.md D2).
// Spezifikation: test/abnahme-dachterrasse.md. Setzt 2a und
// test/patches/element-bounds.patch voraus.
//
// Einbau: als `mod attika { use super::*; … }` ans Ende von
// app/src/abnahme.rs. Nutzt aus abnahme.rs: gebaeude, decke, aw_schicht.
//
// Angenommene Namen nur im Adapter: `Model::set_floor_upstand`.

use sk_model::{ElementId, RunId};

// ===== Adapter 2b =====

fn attika_hoehe(s: &mut Scene, de: ElementId, mm: f64) -> bool {
    s.edit_model("Attikahöhe", |m| m.set_floor_upstand(de, mm))
}

fn ok(s: &mut Scene, id: ElementId) -> f64 {
    s.element_bounds(id).expect("Körper").1.z.round()
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

fn pruefung(s: &Scene) {
    assert!(s.model().check().is_empty(), "{:?}", s.model().check());
}

fn haus(seed: u64, code: Option<&str>, nord: f64) -> (Scene, RunId) {
    let mut s = Scene::with_model(Model::with_seed(seed));
    let (eg, _) = gebaeude(&mut s);
    if let Some(code) = code {
        let t = s
            .model()
            .layer_sets()
            .iter()
            .find(|(_, t)| t.code == code)
            .map(|(id, _)| id)
            .unwrap();
        assert!(s.edit_model("Wandtyp", |m| m.set_run_type(eg, t)));
    }
    if nord != 0.0 {
        let w = nr(&s, "AW-006");
        assert!(s.edit_model("Kopplung gelöst", |m| m.set_linked(w, false)));
        assert!(s.edit_model("Wand verschoben", |m| m.move_segment(w, nord).is_some()));
    }
    pruefung(&s);
    (s, eg)
}

/// Volumen je Schicht über alle Wände eines Zugs, m³ (4 Stellen).
fn schichten(s: &Scene, run: RunId) -> Vec<f64> {
    let m = s.model();
    let walls = &m.run(run).unwrap().segments;
    let n = s.wall_qto(walls[0]).unwrap().layers.len();
    (0..n)
        .map(|i| {
            r4(walls
                .iter()
                .map(|w| s.wall_qto(*w).unwrap().layers[i].volume)
                .sum::<f64>()
                / 1e9)
        })
        .collect()
}

/// A195 (E2, Regel 45, Sollwerte 08:54): Nord −1,50 bei AW-31,5: das
/// EG-WDVS wächst um die Attika, 1,7808 m² Grundriss × 0,20 m = 0,3562
/// m³, EG-Dämmung 14,5215 m³ (ungerundet 14,521528); Porenbeton bleibt 15,7613. Die Attika läuft
/// über AW-002 (Nord) und die Stirnstücke an AW-001 und AW-003 bis OK
/// Attika +3,055; AW-004 (Süd) endet wie bisher bei +2,855. Attika 100
/// → +3,095 und 1,7808 × 0,24 = 0,4274 m³. AW-49: Verblender, Luft und
/// Kerndämmung wachsen, der Kern bleibt. Prüfung ohne Befund.
#[test]
fn a195_attika_an_der_eg_wand() {
    let (s0, eg0) = haus(195, None, 0.0);
    let buendig = schichten(&s0, eg0);
    assert_eq!(buendig, [14.1654, 15.7613]);

    let (mut s, eg) = haus(1950, None, -1500.0);
    assert_eq!(schichten(&s, eg), [14.5215, 15.7613]);
    assert_eq!(r4(aw_schicht(&s, eg, 0) - aw_schicht(&s0, eg0, 0)), 0.3562);
    for (n, z) in [
        ("AW-001", 3055.0),
        ("AW-002", 3055.0),
        ("AW-003", 3055.0),
        ("AW-004", 2855.0),
    ] {
        let id = nr(&s, n);
        assert_eq!(ok(&mut s, id), z, "{n}");
    }
    let de = decke(&s, eg).unwrap();
    assert!(attika_hoehe(&mut s, de, 100.0));
    let id = nr(&s, "AW-002");
    assert_eq!(ok(&mut s, id), 3095.0);
    assert_eq!(schichten(&s, eg)[0], r4(14.1654 + 0.427392));
    pruefung(&s);
    assert!(s.undo());
    assert_eq!(schichten(&s, eg), [14.5215, 15.7613]);
    let schritt = s.undo_label();
    assert!(!attika_hoehe(&mut s, de, 301.0), "0–300");
    assert!(!attika_hoehe(&mut s, de, -1.0), "0–300");
    assert_eq!(s.undo_label(), schritt);

    let (b, ebv) = haus(1951, Some("AW-49"), 0.0);
    let vorher = schichten(&b, ebv);
    let (mut b, eb) = haus(1952, Some("AW-49"), -1500.0);
    let nachher = schichten(&b, eb);
    for i in 0..3 {
        if i == 1 {
            continue; // Luft ohne Menge
        }
        assert!(nachher[i] > vorher[i], "Schicht {i} wächst");
    }
    assert_eq!(nachher[3], vorher[3], "Kern bleibt");
    let id = nr(&b, "AW-002");
    assert_eq!(ok(&mut b, id), 3055.0);
    pruefung(&b);
}
