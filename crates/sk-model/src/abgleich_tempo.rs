//! Tempo des Abgleichs beim Schließen eines Schritts, der nur Geschoss-
//! bänder oder den Geländeversatz eines Gebäudes ändert (Review L39).

use crate::{BuildingId, Category, Model, StoreyId};
use sk_math::vec3;
use std::collections::HashSet;
use std::time::Instant;

/// `n` Häuser 10 × 8 m mit zwei Geschossen im Raster zu 15 m, je Haus ein
/// Gebäude oder alle in einem.
fn haeuser(n: usize, je_haus: bool) -> (Model, Vec<BuildingId>) {
    haeuser_mit(n, je_haus, 2)
}

/// Wie [`haeuser`] mit `geschosse` Geschossen.
fn haeuser_mit(n: usize, je_haus: bool, geschosse: u8) -> (Model, Vec<BuildingId>) {
    let mut m = Model::with_seed(5);
    let b0 = m.add_building(geschosse);
    let mut gebaeude = vec![b0];
    for i in 0..n {
        let b = if je_haus && i > 0 {
            let b = m.add_building(geschosse);
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
            // nur Züge dieses Gebäudes; beim Band die des EG und darüber
            // (Review zu #148: Nachbarn des geänderten Bands, nicht alle)
            let scope = m.sync_scope().expect("nur ein Gebäude");
            assert!(!scope.is_empty(), "{i} {gelaende}");
            for &x in &gb {
                if x != b {
                    assert!(zuege(&m, x).iter().all(|r| !scope.contains(r)), "{i}");
                }
            }
            assert!(scope.iter().all(|r| zuege(&m, b).contains(r)), "{i}");
            if !gelaende {
                let eg = m.ground_of(Some(b)).unwrap();
                let og = m.level_above(eg).unwrap();
                for (id, r) in m.runs().iter() {
                    if r.storey == eg || r.storey == og {
                        assert!(scope.contains(&id), "{i}");
                    }
                }
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

/// Budget (Review zu #148, B2): ein Großgebäude mit 200 Einheiten und fünf
/// Geschossen, Loslassen nach einem Zug am EG; die Bänder darüber wandern
/// nur mit. Vorher 27 ms im Release.
#[test]
fn budget_grossgebaeude() {
    let (mut m, gb) = haeuser_mit(200, false, 5);
    let eg = m.ground_of(Some(gb[0])).unwrap();
    // Bestwert aus drei: Die Tests laufen nebeneinander
    let ms = [100.0, -100.0, 100.0]
        .map(|dz| {
            let top = m.storey(eg).unwrap().top();
            m.begin("Geschoss ziehen");
            assert!(m.set_storey_top(eg, top + dz));
            let t = Instant::now();
            assert!(m.commit().is_some());
            ms(t)
        })
        .into_iter()
        .fold(f64::INFINITY, f64::min);
    // Vor #148 27 ms, danach 4,8–7,2 ms je nach Last (Review zu #148, H3)
    let budget = if cfg!(debug_assertions) { 200.0 } else { 15.0 };
    assert!(ms < budget, "{ms:.1} ms");
}

/// Zufällige Schritte an Bändern, Gründung, Gelände und Flachdach in drei
/// Gebäuden mit Vor- und Rücksprüngen: Nach jedem gleicht der Abgleich im
/// Bereich dem über alles (Review zu #148).
#[test]
fn zufall_wie_ueber_alles() {
    let (mut m, gb) = haeuser_mit(6, true, 4);
    let mut z = 148u64;
    let mut f = || {
        z = z
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        (z >> 11) as f64 / (1u64 << 53) as f64
    };
    // Vor- und Rücksprünge in den oberen Geschossen
    let ids: Vec<_> = m.runs().ids().collect();
    for (k, r) in ids.into_iter().enumerate() {
        let Some(&og) = m.runs_above(r).first() else {
            continue;
        };
        let w = m.wall_at(og, 1 + k % 3).unwrap();
        m.begin("Versatz");
        m.set_linked(w, false);
        assert!(m.set_offset(w, [-1200.0, 900.0, 600.0][k % 3]).is_some());
        m.commit();
    }
    assert_eq!(ganz(&m), ((0, 0), (0, 0), true));
    for k in 0..80 {
        let b = gb[(f() * gb.len() as f64) as usize];
        let ebenen = m.levels_in(Some(b));
        let st = ebenen[(f() * ebenen.len() as f64) as usize];
        m.begin("Zufall");
        let was = (f() * 5.0) as usize;
        let geaendert = match was {
            0 => {
                let top = m.storey(st).unwrap().top();
                m.drag_storey_top(st, top + (f() - 0.5) * 800.0)
            }
            1 => m.set_storey_height(st, 2400.0 + f() * 1200.0),
            2 => m.set_terrain_offset_of(b, (f() - 0.5) * 1500.0),
            3 => {
                let gr = m.foundation_level_of(st).unwrap();
                let uk = m.storey(gr).unwrap().elevation;
                m.drag_foundation_bottom_of(gr, uk + (f() - 0.5) * 600.0)
            }
            _ => {
                let an = m.roof_level_of(Some(b)).is_none();
                m.set_flat_roof(st, an)
            }
        };
        // Die Bereichswahl selbst (Review zu #148, H2): nur Züge dieses
        // Gebäudes (ein im Schritt entfernter Zug zählt nicht mit); ein Band oder die Gründung bringt höchstens sich und
        // die Nachbarn, nicht das ganze Gebäude
        let scope = m.sync_scope().expect("nur ein Gebäude");
        let geschosse: HashSet<StoreyId> = scope
            .iter()
            .filter_map(|&r| m.run(r).map(|x| x.storey))
            .collect();
        assert!(
            geschosse
                .iter()
                .all(|&s| m.storey(s).and_then(|s| s.building) == Some(b)),
            "Schritt {k}, Art {was}: fremde Züge"
        );
        if was < 4 {
            assert!(
                geschosse.len() <= 3,
                "Schritt {k}, Art {was}: {geschosse:?}"
            );
            // Ein Schritt, der etwas ändert, bringt Züge mit
            let leer = m.txn.as_ref().is_some_and(|t| t.changes.is_empty());
            assert_eq!(
                scope.is_empty(),
                leer,
                "Schritt {k}, Art {was}, {geaendert}"
            );
        }
        m.commit();
        assert_eq!(
            ganz(&m),
            ((0, 0), (0, 0), true),
            "Schritt {k}, Art {was}, {geaendert}"
        );
        assert!(
            m.check().is_empty(),
            "Schritt {k}, Art {was}: {:?}",
            m.check()
        );
    }
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
