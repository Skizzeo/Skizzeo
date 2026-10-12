use super::*;
use crate::aussen::aussenmasse;
use crate::schrift::tests::schrift;
use sk_model::oeffnung::{HeightLine, HeightRef, Opening, OpeningKind};
use sk_model::{Model, StoreyId};

/// Haus 10 × 8 m mit einem Fenster in der Südwand.
fn haus() -> (Model, StoreyId) {
    let mut m = Model::new();
    m.begin("Haus");
    let b = m.add_building(1);
    let run = m
        .build_from_polygon(
            b,
            &[
                vec3(0.0, 0.0, 0.0),
                vec3(0.0, 8000.0, 0.0),
                vec3(10_000.0, 8000.0, 0.0),
                vec3(10_000.0, 0.0, 0.0),
            ],
        )
        .unwrap();
    let st = m.run(run).unwrap().storey;
    m.add_opening(Opening {
        kind: OpeningKind::Window,
        path: vec![vec3(3010.0, 0.0, 0.0), vec3(2000.0, 0.0, 0.0)],
        bottom: HeightRef::Line(st, HeightLine::Sill),
        top: HeightRef::Line(st, HeightLine::Lintel),
        reveal: None,
        install: Default::default(),
        swing: None,
        lock: Vec::new(),
    })
    .unwrap();
    m.commit();
    (m, st)
}

#[test]
fn jede_kette_hat_linie_hilfslinien_begrenzungen_und_zahlen() {
    let Some(sch) = schrift() else {
        eprintln!("Liberation Sans fehlt, Test übersprungen");
        return;
    };
    let (m, st) = haus();
    let a = aussenmasse(&m, st).unwrap();
    let stil = m.massstil();
    let b = bild(&a, stil, &sch);
    let ketten: usize = a.seiten.iter().map(|s| s.ketten.len()).sum();
    let punkte: usize = a
        .seiten
        .iter()
        .flat_map(|s| &s.ketten)
        .map(|k| k.bezuege.len())
        .sum();
    let zaehle = |art| b.striche.iter().filter(|s| s.art == art).count();
    assert_eq!(zaehle(Strichart::Masslinie), ketten);
    assert_eq!(b.linien.len(), ketten);
    assert_eq!(zaehle(Strichart::Hilfslinie), punkte);
    assert_eq!(zaehle(Strichart::Symbol), punkte * KREIS);
    assert_eq!(b.bezuege.len(), punkte);
    // Maße, dazu Höhe und BRH des Fensters
    assert_eq!(b.zahlen.len(), punkte - ketten + 2);
    // Alles liegt auf der Geschossebene
    let z = a.z;
    assert!(b.tinte.iter().flatten().all(|p| (p.z - z).abs() < 1e-9));
    assert!(b.striche.iter().all(|s| s.a.z == z && s.b.z == z));
    // Die Zahl des Gesamtmaßes Süd steht mittig über ihrer Linie
    let sued = a.seiten.iter().position(|s| s.grad == 270).unwrap();
    let ki = a.seiten[sued]
        .ketten
        .iter()
        .position(|k| k.art == Kettenart::Gesamt)
        .unwrap();
    let g = b
        .zahlen
        .iter()
        .find(|z| {
            z.art
                == Zahlart::Mass {
                    seite: sued,
                    kette: ki,
                    i: 0,
                }
        })
        .unwrap();
    let k = &a.seiten[sued].ketten[ki];
    let mitte = 0.5 * (g.rahmen[0].x + g.rahmen[1].x);
    assert!((mitte - 0.5 * (k.bezuege[0].t + k.bezuege[1].t)).abs() < 1e-6);
    // Über der Linie in Leserichtung (Süd: +y), Abstand 0,8 mm × 100
    assert!(
        (g.rahmen[0].y - (-k.lage + 80.0)).abs() < 1e-6,
        "{:?}",
        g.rahmen
    );
    assert!(g.rahmen[3].y > g.rahmen[0].y);
    assert!(!g.tinte.is_empty());
    // Brüstung: vor der Wand, Text beginnt mit „BRH“
    let brh = b
        .zahlen
        .iter()
        .find(|z| matches!(z.art, Zahlart::Brh { .. }))
        .unwrap();
    assert!(brh.rahmen[3].y <= -150.0 + 1e-6, "{:?}", brh.rahmen);
    assert!(brh.rahmen[0].y < brh.rahmen[3].y);
}

#[test]
fn zu_schmale_zahl_rueckt_aus() {
    let Some(sch) = schrift() else {
        return;
    };
    let (m, st) = haus();
    let a = aussenmasse(&m, st).unwrap();
    let b = bild(&a, m.massstil(), &sch);
    // Die Wanddicke (rund 17 cm) ist schmaler als ihre Zahl: am Rand der
    // Kette außen daneben, nicht über ihren Hilfslinien
    for z in &b.zahlen {
        let Zahlart::Mass { seite, kette, i } = z.art else {
            continue;
        };
        let s = &a.seiten[seite];
        let k = &s.ketten[kette];
        let (ta, tb) = (k.bezuege[i].t, k.bezuege[i + 1].t);
        let x0 = (z.rahmen[0] - vec3(0.0, 0.0, a.z)).dot(s.t);
        let x1 = (z.rahmen[1] - vec3(0.0, 0.0, a.z)).dot(s.t);
        let w = x1 - x0;
        if w + 2.0 * LUFT * 250.0 > tb - ta {
            let n = k.bezuege.len() - 1;
            if i == 0 {
                assert!(x1 < ta, "links daneben: {x0} {x1} {ta}");
            } else if i + 1 == n {
                assert!(x0 > tb, "rechts daneben: {x0} {x1} {tb}");
            }
        } else {
            assert!(x0 > ta && x1 < tb);
        }
    }
}

#[test]
fn begrenzung_schraeg_punkt_und_pfeil() {
    let Some(sch) = schrift() else {
        return;
    };
    let (mut m, st) = haus();
    let a = aussenmasse(&m, st).unwrap();
    let punkte: usize = a
        .seiten
        .iter()
        .flat_map(|s| &s.ketten)
        .map(|k| k.bezuege.len())
        .sum();
    let mass: usize = a
        .seiten
        .iter()
        .flat_map(|s| &s.ketten)
        .map(|k| k.bezuege.len() - 1)
        .sum();
    let ohne = bild(&a, m.massstil(), &sch).tinte.len();
    for (symbol, striche, tinte) in [
        (Symbol::Schraeg, punkte, 0),
        (Symbol::Punkt, 0, punkte * KREIS),
        (Symbol::Pfeil, 0, 2 * mass),
    ] {
        let mut s = m.massstil().clone();
        s.symbol = symbol;
        m.begin("Stil");
        m.set_massstil(s);
        m.commit();
        let b = bild(&a, m.massstil(), &sch);
        let n = b
            .striche
            .iter()
            .filter(|s| s.art == Strichart::Symbol)
            .count();
        assert_eq!(n, striche, "{symbol:?}");
        assert_eq!(b.tinte.len() - ohne, tinte, "{symbol:?}");
    }
}
