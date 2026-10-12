use super::*;
use crate::aussen::aussenmasse;
use crate::bild::bild;
use crate::schrift::tests::schrift;
use sk_math::vec3;
use sk_model::oeffnung::{HeightLine, HeightRef, Opening, OpeningKind};
use sk_model::{Model, StoreyId};

/// Zwei Häuser 10 × 8 m nebeneinander, `luecke` mm zwischen den
/// Außenwänden; das rechte hat ein Fenster in der Wand zum linken.
fn zwei(luecke: f64) -> (Model, StoreyId, StoreyId) {
    let mut m = Model::new();
    m.begin("Häuser");
    let mut st = Vec::new();
    for x in [0.0, 10_000.0 + luecke] {
        let b = m.add_building(1);
        let p = |dx: f64, y: f64| vec3(x + dx, y, 0.0);
        let run = m
            .build_from_polygon(
                b,
                &[
                    p(0.0, 0.0),
                    p(0.0, 8000.0),
                    p(10_000.0, 8000.0),
                    p(10_000.0, 0.0),
                ],
            )
            .unwrap();
        st.push(m.run(run).unwrap().storey);
    }
    m.add_opening(Opening {
        kind: OpeningKind::Window,
        path: vec![
            vec3(10_000.0 + luecke, 3000.0, 0.0),
            vec3(10_000.0 + luecke, 4010.0, 0.0),
        ],
        bottom: HeightRef::Line(st[1], HeightLine::Sill),
        top: HeightRef::Line(st[1], HeightLine::Lintel),
        reveal: None,
        install: Default::default(),
        swing: None,
        lock: Vec::new(),
    })
    .unwrap();
    m.commit();
    (m, st[0], st[1])
}

fn arten(a: &Aussenmasse, grad: i32) -> Vec<Kettenart> {
    let s = a.seiten.iter().find(|s| s.grad == grad).unwrap();
    s.ketten.iter().map(|k| k.art).collect()
}

/// Achsparallele Kästen der Maßzahlen.
fn zahlen(a: &Aussenmasse, stil: &Stil, sch: &crate::schrift::Schrift) -> Vec<[f64; 4]> {
    bild(a, stil, sch)
        .zahlen
        .iter()
        .map(|z| {
            z.rahmen
                .iter()
                .fold([f64::MAX, f64::MAX, f64::MIN, f64::MIN], |k, p| {
                    [k[0].min(p.x), k[1].min(p.y), k[2].max(p.x), k[3].max(p.y)]
                })
        })
        .collect()
}

fn ueberdeckt(a: &[[f64; 4]], b: &[[f64; 4]]) -> usize {
    a.iter()
        .flat_map(|p| b.iter().map(move |q| (p, q)))
        .filter(|(p, q)| p[0] < q[2] && q[0] < p[2] && p[1] < q[3] && q[1] < p[3])
        .count()
}

/// Zwei Häuser 6 m auseinander (wie R7): Ohne Platz greifen die Ketten
/// ineinander und Zahlen liegen übereinander; mit Platz weichen auf den
/// Seiten zueinander die Gesamtmaße, keine Zahl überdeckt eine des anderen
/// Hauses, die Öffnungskette bleibt und die Gegenseiten bleiben vollständig.
#[test]
fn sechs_meter_ohne_ueberdeckte_zahlen() {
    let Some(sch) = schrift() else {
        eprintln!("Liberation Sans fehlt, Test übersprungen");
        return;
    };
    let (m, l, r) = zwei(6000.0);
    let stil = m.massstil();
    let (al, ar) = (aussenmasse(&m, l).unwrap(), aussenmasse(&m, r).unwrap());
    let vorher = ueberdeckt(&zahlen(&al, stil, &sch), &zahlen(&ar, stil, &sch));
    assert!(vorher > 0, "ohne Platz sollten sich Zahlen überdecken");
    let weg = platz(&[&al, &ar], stil);
    let (nl, nr) = (ohne(&al, &weg[0]), ohne(&ar, &weg[1]));
    assert_eq!(
        ueberdeckt(&zahlen(&nl, stil, &sch), &zahlen(&nr, stil, &sch)),
        0
    );
    // Linkes Haus nach Osten (0°), rechtes nach Westen (180°)
    assert_eq!(arten(&nl, 0), [Kettenart::Wand]);
    assert_eq!(arten(&nr, 180), [Kettenart::Oeffnung, Kettenart::Wand]);
    assert_eq!(weg[0], [(0, Kettenart::Gesamt)]);
    assert_eq!(weg[1], [(180, Kettenart::Gesamt)]);
    // Die Gegenseiten behalten ihr Gesamtmaß
    assert_eq!(arten(&nl, 180), arten(&al, 180));
    assert_eq!(arten(&nr, 0), arten(&ar, 0));
    // Kein Streifen ragt mehr in einen fremden Kettenbereich
    let rr = rand(stil);
    for (x, y) in [(&nl, &nr), (&nr, &nl)] {
        for s in &x.seiten {
            for k in &s.ketten {
                let f = streifen(s, k, rr);
                let fremd = y.seiten.iter().filter_map(|t| bereich(t, &Weg::new(), rr));
                for b in fremd {
                    assert!(
                        k.art == Kettenart::Oeffnung || !f.stoert(&b),
                        "{:?} {}",
                        k.art,
                        s.grad
                    );
                }
            }
        }
    }
}

/// Weit genug auseinander weicht nichts; ganz eng bleibt zueinander nur
/// die Öffnungskette.
#[test]
fn weit_nichts_eng_nur_oeffnungen() {
    let (m, l, r) = zwei(20_000.0);
    let stil = m.massstil();
    let (al, ar) = (aussenmasse(&m, l).unwrap(), aussenmasse(&m, r).unwrap());
    assert_eq!(platz(&[&al, &ar], stil), [Weg::new(), Weg::new()]);
    let (m, l, r) = zwei(2000.0);
    let (al, ar) = (aussenmasse(&m, l).unwrap(), aussenmasse(&m, r).unwrap());
    let weg = platz(&[&al, &ar], m.massstil());
    assert!(arten(&ohne(&al, &weg[0]), 0).is_empty());
    assert_eq!(arten(&ohne(&ar, &weg[1]), 180), [Kettenart::Oeffnung]);
    assert_eq!(arten(&ohne(&al, &weg[0]), 180), arten(&al, 180));
}

/// Eine von Hand verschobene Kette weicht nicht und ihr Bereich gilt: Das
/// nach außen gezogene Gesamtmaß des linken Hauses bleibt, dafür weicht
/// beim rechten Haus auch die Wandkette.
#[test]
fn verschobene_kette_bleibt() {
    let (mut m, l, r) = zwei(6000.0);
    m.begin("Versatz");
    m.set_massversatz(l, 0, Kettenart::Gesamt, 500.0);
    m.commit();
    let stil = m.massstil();
    let (al, ar) = (aussenmasse(&m, l).unwrap(), aussenmasse(&m, r).unwrap());
    let weg = platz(&[&al, &ar], stil);
    assert_eq!(arten(&ohne(&al, &weg[0]), 0), arten(&al, 0));
    assert_eq!(arten(&ohne(&ar, &weg[1]), 180), [Kettenart::Oeffnung]);
}

/// Nach Änderung einiger Kästen findet [`nachbarn_neu`] dieselben Paare wie
/// [`nachbarn`] über alle (Review L24), auch wenn ein Kasten wegfällt oder
/// dazukommt und zwei geänderte sich überdecken.
#[test]
fn nachbarn_neu_wie_alle() {
    let mut z: u64 = 7;
    let mut zufall = move || {
        z = z
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        (z >> 33) as f64 / (1u64 << 31) as f64
    };
    let mut kasten = |zufall: &mut dyn FnMut() -> f64| {
        let (x, y) = (zufall() * 100_000.0, zufall() * 100_000.0);
        Some([
            x,
            y,
            x + 5000.0 + zufall() * 15_000.0,
            y + 5000.0 + zufall() * 15_000.0,
        ])
    };
    let sortiert = |mut p: Vec<(usize, usize)>| {
        p.sort();
        p
    };
    let mut k: Vec<_> = (0..200).map(|_| kasten(&mut zufall)).collect();
    k[17] = None;
    let mut paare = nachbarn(&k);
    for runde in 0..20 {
        let mut neu: Vec<usize> = (0..1 + runde % 5)
            .map(|_| (zufall() * 200.0) as usize % 200)
            .collect();
        neu.sort();
        neu.dedup();
        for &i in &neu {
            k[i] = if runde % 7 == 3 {
                None
            } else {
                kasten(&mut zufall)
            };
        }
        // Zwei geänderte übereinander
        if neu.len() > 1 {
            k[neu[1]] = k[neu[0]];
        }
        paare = nachbarn_neu(&k, &paare, &neu);
        assert_eq!(
            sortiert(paare.clone()),
            sortiert(nachbarn(&k)),
            "Runde {runde}"
        );
    }
    assert!(paare.len() > 20, "zu wenig Nachbarn: {}", paare.len());
}
