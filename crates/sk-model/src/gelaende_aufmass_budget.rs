//! Budget mit Höhenaufmaß (Review #123 W45 B1): Eine Geländeabfrage kostet
//! je Modellstand einen Gang über die Wandzüge, nicht je Abfrage. Gemessen
//! bei 100 und 1000 Häusern mit je eigenem Gebäude (je Haus eine Gründung):
//! Einlesen, Versatz eines Gebäudes, eine Mausbewegung im Geschossmanager
//! (UK ziehen, dann Einbindetiefe und Bereich wie `sync_levels`; die Lage
//! bleibt dabei gemerkt) und das Loslassen mit Platten und Erdmengen; dazu
//! OK EG und OK Gelände ziehen bei 1000 und 4000 Häusern.

use super::*;
use std::collections::{HashMap, HashSet};
use std::time::Instant;

fn budget(ms: f64) -> f64 {
    if cfg!(debug_assertions) {
        ms * 40.0
    } else {
        ms
    }
}

fn ms_seit(t: Instant) -> f64 {
    t.elapsed().as_secs_f64() * 1000.0
}

/// `n` Häuser 10 × 8 m im Raster zu 15 m, je Haus ein Gebäude, und ein
/// gewelltes örtliches Aufmaß (Punkte alle 20 m), das alle überdeckt.
fn haeuser_am_hang(n: usize) -> (Model, Terrain) {
    let mut m = Model::with_seed(5);
    for i in 0..n {
        let b = m.add_building(1);
        let (ox, oy) = ((i % 64) as f64 * 15_000.0, (i / 64) as f64 * 15_000.0);
        let p = |x: f64, y: f64| vec3(ox + x, oy + y, 0.0);
        let ecken = [
            p(0.0, 0.0),
            p(0.0, 8000.0),
            p(10_000.0, 8000.0),
            p(10_000.0, 0.0),
        ];
        m.build_from_polygon(b, &ecken).unwrap();
    }
    let breite = 64.0 * 15.0 + 80.0;
    let tiefe = n.div_ceil(64) as f64 * 15.0 + 80.0;
    let (nx, ny) = ((breite / 20.0) as usize + 1, (tiefe / 20.0) as usize + 1);
    let mut t = Terrain::new(Crs::Local);
    for i in 0..nx {
        for j in 0..ny {
            let (e, nn) = (1000.0 + 20.0 * i as f64, 2000.0 + 20.0 * j as f64);
            t.points.push(TerrainPoint {
                nr: (i * ny + j + 1).to_string(),
                e,
                n: nn,
                h: 50.0 + 3.0 * (e / 37.0).sin() + 2.0 * (nn / 23.0).cos(),
                code: String::new(),
                off: false,
                hand: false,
                gemessen: None,
            });
        }
    }
    (m, t)
}

/// Gemerkt wie gerechnet: je Gründung der tiefste Punkt am Plattenrand,
/// ±0,00 am höchsten Punkt des bestimmenden Gebäudes, direkt aus den
/// Umrissen (wie vor W45).
fn wie_gerechnet(m: &Model) {
    let (t, f) = (m.terrain().unwrap(), m.survey_frame().unwrap());
    let ring = |id| -> Vec<(f64, f64)> {
        let o = m.plate_outline(id).unwrap();
        o.iter().map(|p| f.to_survey(p.x, p.y)).collect()
    };
    let mut tief: HashMap<StoreyId, f64> = HashMap::new();
    let mut hoch = f64::NEG_INFINITY;
    let haus = m.level_building().unwrap();
    for (id, r) in m.runs().iter() {
        let gr = m.foundation_level_of(r.storey).unwrap();
        let lo = t.min_along(&ring(id)).unwrap();
        let x = tief.entry(gr).or_insert(lo);
        *x = x.min(lo);
        if m.building_of(r.storey) == Some(haus) {
            hoch = hoch.max(t.max_along(&ring(id)).unwrap());
        }
    }
    let h0 = hoch + m.terrain_offset_of(Some(haus)) / 1000.0;
    assert!((m.terrain_nhn().unwrap() - h0).abs() < 1e-9);
    assert_eq!(tief.len(), m.runs().len());
    for (gr, lo) in tief {
        let soll = (lo - h0) * 1000.0;
        assert!((m.terrain_z_of_level(gr) - soll).abs() < 1e-6, "{gr:?}");
    }
}

#[test]
fn aufmass_im_budget() {
    // Grenzen je Fall (Release, Debug mal 40): Einlesen, Versatz,
    // Mausbewegung, Loslassen; vorher bei 1000 Häusern 59 s, 39 s,
    // 38,6 ms und 1,77 s (Review W45)
    for (n, grenzen) in [
        (100, [30.0, 20.0, 0.5, 20.0]),
        (1000, [600.0, 400.0, 2.0, 500.0]),
    ] {
        let (mut m, t) = haeuser_am_hang(n);
        let t0 = Instant::now();
        m.begin("Aufmaß");
        assert!(m.import_terrain(t));
        m.commit();
        let einlesen = ms_seit(t0);
        wie_gerechnet(&m);
        // Versatz des ersten Gebäudes: alle Gründungen folgen in einem Gang
        let b = m.building_of(m.defaults().storey).unwrap();
        let t0 = Instant::now();
        m.begin("Versatz");
        assert!(m.set_terrain_offset_of(b, 300.0));
        m.commit();
        let versatz = ms_seit(t0);
        wie_gerechnet(&m);
        // UK der Gründung ziehen: je Bewegung ein neuer Modellstand, die
        // Lage bleibt
        let gr = m.foundation_level().unwrap();
        let uk = m.storey(gr).unwrap().elevation;
        m.begin("UK ziehen");
        let t0 = Instant::now();
        for k in 0..10 {
            assert!(m.drag_foundation_bottom_of(gr, uk - 10.0 * (k + 1) as f64));
            std::hint::black_box((m.embedment_of(gr), m.foundation_bottom_range_of(gr)));
        }
        let zug = ms_seit(t0) / 10.0;
        let t0 = Instant::now();
        m.commit();
        let platten = m.terrain_plates();
        let basen = m.ground_bases();
        let los = ms_seit(t0);
        wie_gerechnet(&m);
        assert!(!platten.is_empty());
        assert_eq!(basen.len(), n);
        eprintln!(
            "{n} Häuser mit Aufmaß: Einlesen {einlesen:.1} ms, Versatz {versatz:.1} ms, \
             Mausbewegung {zug:.2} ms, Loslassen {los:.1} ms"
        );
        for (wert, grenze, was) in [
            (einlesen, grenzen[0], "Einlesen"),
            (versatz, grenzen[1], "Versatz"),
            (zug, grenzen[2], "Mausbewegung"),
            (los, grenzen[3], "Loslassen"),
        ] {
            assert!(wert < budget(grenze), "{n} Häuser, {was}: {wert:.1} ms");
        }
    }
}

/// Ziehen im Geschossmanager mit Aufmaß (W45-Nachprüfung B1): je
/// Mausbewegung OK EG des ersten Gebäudes oder sein OK Gelände, danach
/// Einbindetiefe und Bereich wie `sync_levels`. Die Lage bleibt dabei
/// gemerkt. Beim Gelände folgt je Bewegung nur die Gründung des ersten
/// Gebäudes, die übrigen beim Loslassen in einem Gang, weil das erste
/// Gebäude die Höhenlage bestimmt (Review #123 B1 zum Gelände-Zug); das
/// Ergebnis gleicht demselben Versatz in einem Schritt. Vorher 15,5 und
/// 67,9 ms (OK EG) und 26,2 und 1020 ms (Gelände) bei 1000 und 4000
/// Häusern. 4000 Häuser nur in Release, im Debug dauert schon das Bauen zu
/// lange.
#[test]
fn zuege_im_budget() {
    let mut faelle = vec![(1000, [1.0, 0.1, 50.0])];
    if !cfg!(debug_assertions) {
        faelle.push((4000, [2.0, 0.1, 200.0]));
    }
    let mut zuege = Vec::new();
    for (n, grenzen) in faelle {
        let (mut m, t) = haeuser_am_hang(n);
        m.begin("Aufmaß");
        assert!(m.import_terrain(t));
        m.commit();
        let b = m.building_of(m.defaults().storey).unwrap();
        assert_eq!(m.level_building(), Some(b));
        let gr = m.foundation_level().unwrap();
        let eg = m.ground_storey(gr).unwrap();
        let ok = m.storey(eg).unwrap().top();
        m.begin("OK EG ziehen");
        let t0 = Instant::now();
        for k in 1..=10 {
            assert!(m.drag_storey_top(eg, ok + 10.0 * k as f64));
            std::hint::black_box((m.embedment_of(gr), m.foundation_bottom_range_of(gr)));
        }
        let eg_zug = ms_seit(t0) / 10.0;
        m.rollback();
        // Gelände ziehen; zum Vergleich derselbe Versatz in einem Schritt
        let mut gleich = m.clone();
        gleich.begin("Versatz");
        assert!(gleich.set_terrain_offset_of(b, 400.0));
        gleich.commit();
        // wie der Geschossmanager (Review #123 B1 zum Gelände-Zug): je
        // Bewegung nur das gezogene Gebäude, beim Loslassen die übrigen in
        // einem Gang
        m.begin("Gelände ziehen");
        // die Lage rechnet das erste Bild ohnehin (siehe oben)
        std::hint::black_box(m.terrain_nhn());
        let t0 = Instant::now();
        let zug = m.gelaende_zug(b).unwrap();
        let beginn = ms_seit(t0);
        let mut gelaende_zug = 0.0;
        for k in 1..=100 {
            let t0 = Instant::now();
            assert!(m.gelaende_ziehen(&zug, 4.0 * k as f64));
            gelaende_zug += ms_seit(t0) / 100.0;
            std::hint::black_box((m.embedment_of(gr), m.foundation_bottom_range_of(gr)));
        }
        let t0 = Instant::now();
        let bewegt = m.gelaende_loslassen(zug);
        let loslassen = ms_seit(t0);
        // Gründungen an einer Grenze bleiben; ob gleich, zeigt der
        // Vergleich unten
        assert!(!bewegt.is_empty() && !bewegt.contains(&b));
        m.commit();
        zuege.push(gelaende_zug);
        wie_gerechnet(&m);
        for (id, s) in m.storeys().iter() {
            let soll = gleich.storey(id).unwrap();
            assert!((s.elevation - soll.elevation).abs() < 1e-6, "{id:?}");
            assert_eq!(s.embed, soll.embed, "{id:?}");
        }
        eprintln!(
            "{n} Häuser mit Aufmaß je Mausbewegung: OK EG {eg_zug:.2} ms, \
             Gelände {gelaende_zug:.4} ms (Beginn {beginn:.1} ms, Loslassen {loslassen:.1} ms)"
        );
        for (wert, grenze, was) in [
            (eg_zug, grenzen[0], "OK EG ziehen"),
            (gelaende_zug, grenzen[1], "Gelände ziehen"),
            (beginn + loslassen, grenzen[2], "Beginn und Loslassen"),
        ] {
            assert!(wert < budget(grenze), "{n} Häuser, {was}: {wert:.2} ms");
        }
    }
    // Die Mausbewegung hängt nicht an der Zahl der Häuser; unter 20 µs
    // zählt das Rauschen der Uhr
    if let [z1, z4] = zuege[..] {
        assert!(
            z4 <= 1.5 * z1.max(0.02),
            "1000 Häuser {z1:.4} ms, 4000 Häuser {z4:.4} ms"
        );
    }
}

/// Zug an der Kote, wie ihn der Geschossmanager führt: über den Versatz
/// des bestimmenden Gebäudes und bei absoluter Höhenlage über ±0,00 ü.
/// NHN. Während des Zugs stehen die anderen Gebäude auf ihrer alten
/// Gründung, nach dem Loslassen liegt alles wie nach
/// `set_terrain_z_of_level` in einem Schritt (Review #123 B1 zum
/// Gelände-Zug).
#[test]
fn zug_an_der_kote_wie_in_einem_schritt() {
    let (mut m, t) = haeuser_am_hang(6);
    m.begin("Aufmaß");
    assert!(m.import_terrain(t));
    m.commit();
    let b = m.building_of(m.defaults().storey).unwrap();
    assert_eq!(m.level_building(), Some(b));
    let gr = m.foundation_level().unwrap();
    for abs in [false, true] {
        if abs {
            let h = (m.terrain_nhn().unwrap() * 1000.0).round() / 1000.0;
            m.begin("Höhe");
            assert!(m.set_terrain_nhn(h + 0.2));
            m.commit();
        }
        let z0 = m.terrain_z_of_level(gr);
        let mut gleich = m.clone();
        gleich.begin("Gelände");
        assert!(gleich.set_terrain_z_of_level(gr, z0 - 350.0));
        gleich.commit();
        let uk: Vec<_> = m
            .storeys()
            .iter()
            .map(|(id, s)| (id, s.elevation))
            .collect();
        m.begin("Gelände ziehen");
        let zug = m.gelaende_zug(b).unwrap();
        for z in [z0 - 500.0, z0 + 120.0, z0 - 350.0] {
            assert!(m.gelaende_ziehen_auf(&zug, gr, z), "{abs} {z}");
            for &(id, e) in &uk {
                if m.building_of(id) != Some(b) {
                    assert_eq!(m.storey(id).unwrap().elevation, e, "{abs} {id:?}");
                }
            }
        }
        let bewegt = m.gelaende_loslassen(zug);
        assert!(!bewegt.is_empty() && !bewegt.contains(&b), "{abs}");
        m.commit();
        assert_eq!(m.terrain().unwrap().href == HeightRef::Abs, abs);
        assert_eq!(m.location().nhn, gleich.location().nhn, "{abs}");
        // der Versatz summiert die Schritte, auf Rundung genau
        let o = (
            m.terrain_offset_of(Some(b)),
            gleich.terrain_offset_of(Some(b)),
        );
        assert!((o.0 - o.1).abs() < 1e-6, "{abs} {o:?}");
        for (id, s) in m.storeys().iter() {
            let soll = gleich.storey(id).unwrap();
            assert!((s.elevation - soll.elevation).abs() < 1e-6, "{abs} {id:?}");
            assert_eq!(s.embed, soll.embed, "{abs} {id:?}");
        }
    }
}

/// Grenzen über viele Punkte: Jede Nummer wird einmal nachgeschlagen, eine
/// [tline] über 100.000 Punkte kostet nicht Punkte mal Grenzpunkte
/// (Review W45 H4).
#[test]
fn grenzen_mit_vielen_punkten() {
    let mut t = Terrain::new(Crs::Local);
    let n = 100_000;
    for i in 0..n {
        let w = i as f64 / n as f64 * std::f64::consts::TAU;
        t.points.push(TerrainPoint {
            nr: i.to_string(),
            e: 1000.0 + 100.0 * w.cos(),
            n: 2000.0 + 100.0 * w.sin(),
            h: 50.0,
            code: String::new(),
            off: false,
            hand: false,
            gemessen: None,
        });
    }
    t.boundaries
        .push((0..n).rev().map(|i| i.to_string()).collect());
    let t0 = Instant::now();
    let ringe = t.boundary_rings();
    let zeit = ms_seit(t0);
    assert_eq!(ringe.len(), 1);
    assert_eq!(ringe[0].len(), n);
    assert!((t.parcel_area() - std::f64::consts::PI * 100.0 * 100.0).abs() < 1.0);
    assert!(zeit < budget(100.0), "{zeit:.1} ms");
}

/// Ein fertiges Netz aus der DXF (G1b): 150 × 150 Punkte und 44.402
/// Netzflächen, deren Kanten das Netz festhalten. Vernetzen mit allen
/// Kanten kostet höchstens wenig mehr als ohne, und jede Fläche gibt ihr
/// Dreieck zurück, auch wo das Raster vier Punkte auf einem Kreis hat.
#[test]
fn netz_der_datei_im_budget() {
    let k = 150;
    let mut t = Terrain::new(Crs::Local);
    for i in 0..k {
        for j in 0..k {
            let (e, n) = (1000.0 + 2.0 * i as f64, 2000.0 + 2.0 * j as f64);
            t.points.push(TerrainPoint {
                nr: format!("N{}", i * k + j + 1),
                e,
                n,
                h: 50.0 + (e / 7.0).sin() + (n / 5.0).cos(),
                code: String::new(),
                off: false,
                hand: false,
                gemessen: None,
            });
        }
    }
    t.trim = Trim::Hull;
    let mut t2 = t.clone();
    let ohne = Instant::now();
    let m0 = t.mesh();
    let ohne = ms_seit(ohne);
    let nr = |i: usize, j: usize| format!("N{}", i * k + j + 1);
    for i in 0..k - 1 {
        for j in 0..k - 1 {
            // Diagonale abwechselnd, so weicht das Netz vom Delaunay ab
            let (a, b, c, d) = (nr(i, j), nr(i + 1, j), nr(i + 1, j + 1), nr(i, j + 1));
            let [x, y] = if (i + j) % 2 == 0 {
                [vec![a, b.clone(), c.clone()], vec![nr(i, j), c, d]]
            } else {
                [vec![a, b.clone(), d.clone()], vec![b, nr(i + 1, j + 1), d]]
            };
            for pts in [x, y] {
                t2.breaks.push(TerrainLine { pts, closed: true });
            }
        }
    }
    let t0 = Instant::now();
    let m = t2.mesh();
    let mit = ms_seit(t0);
    eprintln!("Netz der Datei: {mit:.0} ms, ohne Kanten {ohne:.0} ms");
    assert_eq!(m.netz.dreiecke.len(), 2 * (k - 1) * (k - 1));
    assert_eq!(m0.netz.dreiecke.len(), m.netz.dreiecke.len());
    // jedes Dreieck der Datei ist eines des Netzes
    let mut da: HashSet<[u32; 3]> = HashSet::new();
    for d in &m.netz.dreiecke {
        let mut d = *d;
        d.sort();
        da.insert(d);
    }
    let ix: HashMap<&str, u32> = m
        .index
        .iter()
        .enumerate()
        .map(|(k, &i)| (t2.points[i].nr.as_str(), k as u32))
        .collect();
    for l in &t2.breaks {
        let mut d = [
            ix[l.pts[0].as_str()],
            ix[l.pts[1].as_str()],
            ix[l.pts[2].as_str()],
        ];
        d.sort();
        assert!(da.contains(&d), "{:?}", l.pts);
    }
    assert!(
        mit < budget(ohne * 4.0 + 300.0),
        "{mit:.0} ms, ohne {ohne:.0} ms"
    );
}
