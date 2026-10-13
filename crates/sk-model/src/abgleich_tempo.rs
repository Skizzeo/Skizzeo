//! Tempo des Abgleichs beim Schließen eines Schritts, der nur Geschoss-
//! bänder oder den Geländeversatz eines Gebäudes ändert (Review L39).

use crate::{BuildingId, Category, Model};
use sk_math::vec3;
use std::time::Instant;

/// `n` Häuser 10 × 8 m mit zwei Geschossen im Raster zu 15 m, je Haus ein
/// Gebäude oder alle in einem.
fn haeuser(n: usize, je_haus: bool) -> (Model, Vec<BuildingId>) {
    let mut m = Model::with_seed(5);
    let b0 = m.add_building(2);
    let mut gebaeude = vec![b0];
    for i in 0..n {
        let b = if je_haus && i > 0 {
            let b = m.add_building(2);
            gebaeude.push(b);
            b
        } else {
            b0
        };
        let (ox, oy) = ((i % 64) as f64 * 15000.0, (i / 64) as f64 * 15000.0);
        let p = |x: f64, y: f64| vec3(ox + x, oy + y, 0.0);
        m.build_from_polygon(
            b,
            &[
                p(0.0, 0.0),
                p(0.0, 8000.0),
                p(10000.0, 8000.0),
                p(10000.0, 0.0),
            ],
        )
        .unwrap();
    }
    (m, gebaeude)
}

fn ms(t: Instant) -> f64 {
    t.elapsed().as_secs_f64() * 1000.0
}

/// Abgleich über das ganze Modell auf einer Kopie: Was er hinzufügt und
/// entfernt (Terrassen, Flachdächer) und ob die Zahl der Bauteile bleibt.
fn ganz(m: &Model) -> ((usize, usize), (usize, usize), bool) {
    let mut x = m.clone();
    x.sync_edge_strips(None);
    x.sync_soffits();
    let t = x.sync_terraces();
    let f = x.sync_flat_roofs_in(None);
    (t, f, x.elements.len() == m.elements.len())
}

/// Ein Geschossband oder der Geländeversatz eines Gebäudes gleicht nur die
/// Züge dieses Gebäudes ab; das Ergebnis gleicht dem Abgleich über alles,
/// auch mit Vorsprung, Rücksprung und Flachdach (Review L39).
#[test]
fn band_und_gelaende_nur_im_gebaeude() {
    let (mut m, gb) = haeuser(3, true);
    // Haus 1: OG springt vor (Untersicht) und zurück (Terrasse); Haus 2
    // mit Flachdach
    let og1 = m.runs_above(m.runs().ids().next().unwrap())[0];
    for (k, d) in [(1, -1200.0), (3, 900.0)] {
        let w = m.wall_at(og1, k).unwrap();
        m.begin("Versatz");
        m.set_linked(w, false);
        assert!(m.set_offset(w, d).is_some());
        m.commit();
    }
    let og2 = m.levels_in(Some(gb[1]))[2];
    m.begin("Flachdach");
    assert!(m.set_flat_roof(og2, true));
    m.commit();
    assert_eq!(ganz(&m), ((0, 0), (0, 0), true));
    for c in [
        Category::SoffitInsulation,
        Category::RoofTerrace,
        Category::Roof,
    ] {
        assert!(m.elements().iter().any(|(_, e)| e.category == c), "{c:?}");
    }
    let zuege = |m: &Model, b| -> Vec<_> {
        m.runs()
            .iter()
            .filter(|(_, r)| m.storey(r.storey).and_then(|s| s.building) == Some(b))
            .map(|(id, _)| id)
            .collect()
    };
    for (i, &b) in gb.iter().enumerate() {
        for gelaende in [false, true] {
            m.begin("Ziehen");
            if gelaende {
                assert!(m.set_terrain_offset_of(b, 120.0 * (i + 1) as f64));
            } else {
                let eg = m.ground_of(Some(b)).unwrap();
                let top = m.storey(eg).unwrap().top();
                assert!(m.set_storey_top(eg, top - 150.0));
            }
            let scope = m.sync_scope().expect("nur ein Gebäude");
            for &x in &gb {
                let drin = zuege(&m, x).iter().all(|r| scope.contains(r));
                let draussen = zuege(&m, x).iter().all(|r| !scope.contains(r));
                assert!(if x == b { drin } else { draussen }, "{i} {gelaende}");
            }
            assert!(m.commit().is_some());
            assert_eq!(ganz(&m), ((0, 0), (0, 0), true), "{i} {gelaende}");
            assert!(m.check().is_empty(), "{:?}", m.check());
        }
    }
}

/// Budget (Review L39): Loslassen nach einem Geschosszug bei 1000 Häusern
/// je in einem Gebäude; vorher rund 1 s im Release.
#[test]
fn budget_geschosszug() {
    let (mut m, gb) = haeuser(1000, true);
    let eg = m.ground_of(Some(gb[500])).unwrap();
    let top = m.storey(eg).unwrap().top();
    m.begin("Geschoss ziehen");
    assert!(m.set_storey_top(eg, top + 100.0));
    let t = Instant::now();
    assert!(m.commit().is_some());
    let ms = ms(t);
    let budget = if cfg!(debug_assertions) { 400.0 } else { 20.0 };
    assert!(ms < budget, "{ms:.1} ms");
}

#[test]
#[ignore = "Messung"]
fn messen() {
    for n in [1000, 4000] {
        for je_haus in [true, false] {
            let (mut m, gb) = haeuser(n, je_haus);
            let b = gb[gb.len() / 2];
            let eg = m.ground_of(Some(b)).unwrap();
            let top = m.storey(eg).unwrap().top();
            m.begin("Geschoss ziehen");
            assert!(m.set_storey_top(eg, top + 100.0));
            let t = Instant::now();
            assert!(m.commit().is_some());
            let geschoss = ms(t);
            m.begin("Gebäude ziehen");
            assert!(m.set_terrain_offset_of(b, 250.0));
            let t = Instant::now();
            assert!(m.commit().is_some());
            let gelaende = ms(t);
            println!(
                "{n} Häuser, {}: Geschoss {geschoss:.1} ms, Gelände {gelaende:.1} ms",
                if je_haus {
                    "je Haus ein Gebäude"
                } else {
                    "ein Gebäude"
                }
            );
        }
    }
}
