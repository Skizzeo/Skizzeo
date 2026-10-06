//! Leistungsmessung der Echtzeit-Interaktionen (ohne Fenster und Grafikkarte).
//!
//! Misst, was der Prozessor pro Mausbewegung beim Griff-Ziehen am Gummiband
//! rechnet: Wand verschieben, Modell neu aufbauen, Netz für die Ansicht erzeugen,
//! Greifstelle suchen. Ziel: 5–15 ms pro Bild für die ganze Interaktion; der
//! Prozessoranteil sollte deutlich darunter bleiben, weil Hochladen und Zeichnen
//! auf der Grafikkarte noch dazukommen.
//!
//! Aufruf: `cargo test --release -p skizzeo perf -- --ignored --nocapture`

use crate::camera::Camera;
use crate::scene::Scene;
use crate::section::SectionLine;
use crate::ui::ViewKind;
use crate::wall_edit::WallEdit;
use sk_math::vec3;
use sk_model::{RefSide, WallChain};
use sk_platform::{Event, Modifiers};
use std::time::Instant;

const W: f64 = 1600.0;
const H: f64 = 900.0;

/// `n` Häuser (je ein geschlossener Wandzug mit `segs` Segmenten) im Raster.
fn town(n: usize, segs: usize) -> Scene {
    let mut s = Scene::new();
    let side = (n as f64).sqrt().ceil() as usize;
    for i in 0..n {
        let (ox, oy) = ((i % side) as f64 * 15000.0, (i / side) as f64 * 15000.0);
        // Vieleck mit `segs` Ecken (Radius 5 m), im Uhrzeigersinn
        let pts = (0..segs)
            .map(|k| {
                let a = -(k as f64) * std::f64::consts::TAU / segs as f64;
                vec3(ox + 5000.0 * a.cos(), oy + 5000.0 * a.sin(), 0.0)
            })
            .collect();
        let w = WallChain {
            points: pts,
            closed: true,
            ref_side: RefSide::Left,
            // Schichten kommen aus dem Aufbau der Bibliothek
            layers: Vec::new(),
            height: 3500.0,
            joints: Default::default(),
        };
        s.add_wall(&w);
    }
    s
}

fn time<F: FnMut()>(reps: usize, mut f: F) -> f64 {
    f();
    let t = Instant::now();
    for _ in 0..reps {
        f();
    }
    t.elapsed().as_secs_f64() * 1000.0 / reps as f64
}

fn bytes(m: &sk_render::MeshData) -> usize {
    // Flächen 36 B je Ecke (vor E3: 48 B); Kanten als Instanz mit 28 B
    std::mem::size_of_val(m.faces.as_slice()) + m.edges.len() * 28
}

#[test]
#[ignore]
fn perf_griff_ziehen() {
    println!();
    println!(
        "{:>6} {:>5} {:>7} | {:>8} {:>8} {:>8} {:>8} {:>8} {:>8} {:>8} | {:>8} {:>8} {:>8} {:>8} | {:>8} {:>8} {:>8}",
        "Häuser",
        "Segm",
        "Dreieck",
        "Ziehen",
        "Netz3D",
        "NetzGR",
        "NetzSch",
        "Greifen",
        "Strahl",
        "Live3D",
        "Voll",
        "Neu",
        "MB/Voll",
        "MB/Live",
        "Anfassen",
        "Kopie",
        "Rückg."
    );
    for &(n, segs) in &[(1, 4), (10, 4), (100, 4), (1000, 4), (1, 200), (100, 40)] {
        let mut s = town(n, segs);
        let tris = s.mesh(ViewKind::Persp, None, &[]).faces.len() / 3;
        let cam = Camera::looking_at(
            vec3(-20000.0, -30000.0, 25000.0),
            vec3(5000.0, 5000.0, 0.0),
            45.0,
        );
        let run = s.model().runs().ids().next().unwrap();
        let orig = s.chain(run).unwrap().clone();
        let mut flip = false;
        // Ein Ziehschritt: Segment verschieben und Szene neu aufbauen
        // Ein Verlaufsschritt; alle Mausbewegungen bleiben ein Eintrag
        s.begin("Wand verschieben");
        let drag = time(20, || {
            flip = !flip;
            let moved = orig
                .with_segment_moved(0, if flip { 1.0 } else { 2.0 })
                .unwrap_or_else(|| orig.clone());
            s.set_run_points(run, &moved.points);
        });
        s.commit();
        // Anfassen öffnet nur einen Schritt; früher kopierte es das ganze Modell
        let grab = time(20, || {
            s.begin("Wand verschieben");
            s.rollback();
        });
        let copy = time(5, || {
            std::hint::black_box(s.model().clone());
        });
        // Rückgängig und Wiederholen des Ziehens (je ein Zug neu berechnet)
        let undo = time(10, || {
            assert!(s.undo());
            assert!(s.redo());
        }) / 2.0;
        let mut sect = SectionLine::default();
        sect.ensure(&s);
        let plane = sect.plane();
        let m3 = time(10, || {
            std::hint::black_box(s.mesh(ViewKind::Persp, None, &[]));
        });
        let mgr = time(10, || {
            std::hint::black_box(s.mesh(ViewKind::Plan, None, &[]));
        });
        let msc = time(10, || {
            std::hint::black_box(s.mesh(ViewKind::Section, plane, &[]));
        });
        let mut e = WallEdit::default();
        let mv = Event::MouseMove {
            x: 800.0,
            y: 450.0,
            mods: Modifiers::default(),
        };
        let pick = time(20, || {
            e.handle(&mv, &mut s, &cam, W, H, 1.0, true);
            e.handle(&Event::MouseLeave, &mut s, &cam, W, H, 1.0, true);
        });
        let (o, d) = cam.ray(800.0, 450.0, W, H);
        let ray = time(20, || {
            std::hint::black_box(s.raycast(o, d));
        });
        // Beim Ziehen: nur das Live-Netz des gezogenen Wandzugs
        let live = time(20, || {
            std::hint::black_box(s.mesh_runs(ViewKind::Persp, None, &[run]));
        });
        let up = bytes(&s.mesh(ViewKind::Persp, None, &[])) as f64 / 1e6;
        let up_live = bytes(&s.mesh_runs(ViewKind::Persp, None, &[run])) as f64 / 1e6;
        println!(
            "{:>6} {:>5} {:>7} | {:>8.3} {:>8.3} {:>8.3} {:>8.3} {:>8.3} {:>8.3} {:>8.3} | {:>8.3} {:>8.3} {:>8.2} {:>8.3} | {:>8.4} {:>8.3} {:>8.3}",
            n,
            segs,
            tris,
            drag,
            m3,
            mgr,
            msc,
            pick,
            ray,
            live,
            drag + m3,
            drag + live,
            up,
            up_live,
            grab,
            copy,
            undo
        );
    }
    println!(
        "Zeiten in ms je Aufruf. Ziehen = Wandzug verschieben und neu berechnen. \
         Voll = Ziehen + ganzes Netz3D (ein Bild ohne Live-Netz), Neu = Ziehen + Live3D \
         (ein Bild beim Ziehen in 3D). MB/Voll bzw. MB/Live = Daten zur Grafikkarte. \
         Anfassen = Schritt öffnen beim Greifen am Gummiband, Kopie = Modellkopie, die \
         das Greifen vor B3 kostete, Rückg. = Ziehen rückgängig machen."
    );
}

/// `n` Häuser 10 × 8 m mit je zwei Innenwänden quer durch (T an beiden Enden).
fn town_with_interior(n: usize) -> Scene {
    let mut s = Scene::new();
    let side = (n as f64).sqrt().ceil() as usize;
    for i in 0..n {
        let (ox, oy) = ((i % side) as f64 * 15000.0, (i / side) as f64 * 15000.0);
        let p = |x: f64, y: f64| vec3(ox + x, oy + y, 0.0);
        let wall = |points, closed, ref_side| WallChain {
            points,
            closed,
            ref_side,
            layers: Vec::new(),
            height: 2750.0,
            joints: Default::default(),
        };
        let pts = vec![
            p(0.0, 0.0),
            p(0.0, 8000.0),
            p(10000.0, 8000.0),
            p(10000.0, 0.0),
        ];
        s.add_wall(&wall(pts, true, RefSide::Left));
        for x in [3500.0, 6500.0] {
            let w = wall(vec![p(x, 315.0), p(x, 7685.0)], false, RefSide::Center);
            s.add_wall_as(&w, sk_model::Category::InteriorWall);
        }
    }
    s
}

#[test]
#[ignore]
fn perf_ziehen_mit_innenwaenden() {
    println!();
    println!(
        "{:>6} {:>8} {:>8} {:>8} {:>8}",
        "Häuser", "Anschl.", "Ziehen", "Live3D", "Neu"
    );
    for n in [1, 10, 100, 1000] {
        let mut s = town_with_interior(n);
        let joins = s.model().joins().len();
        assert_eq!(joins, 4 * n);
        let run = s.model().runs().ids().next().unwrap();
        let orig = s.chain(run).unwrap().clone();
        let mut flip = false;
        s.begin("Wand verschieben");
        // Oberes Segment ziehen: beide Innenwände werden mitgeführt
        let drag = time(20, || {
            flip = !flip;
            let moved = orig
                .with_segment_moved(1, if flip { -100.0 } else { -200.0 })
                .unwrap();
            s.set_run_points(run, &moved.points);
        });
        let live_set = s.live_set(run);
        assert_eq!(live_set.len(), 3);
        let live = time(20, || {
            std::hint::black_box(s.mesh_runs(ViewKind::Persp, None, &live_set));
        });
        s.commit();
        assert!(s.model().check().is_empty());
        println!(
            "{:>6} {:>8} {:>8.3} {:>8.3} {:>8.3}",
            n,
            joins,
            drag,
            live,
            drag + live
        );
    }
    println!("Zeiten in ms je Mausbewegung. Ziel B5a: Ziehen < 1 ms.");
}

/// E3: Eine Attributänderung (Stiftfarbe) bei 1000 Häusern kostet nur die
/// Tabelle, kein Netz.
#[test]
#[ignore]
fn perf_attribut_aendern() {
    let mut s = town(1000, 4);
    let run = s.model().runs().ids().next().unwrap();
    let builds = s.build_count(run);
    let (id, pen) = s
        .model()
        .attr()
        .pens()
        .iter()
        .find(|(_, p)| p.number == 3)
        .map(|(id, p)| (id, p.clone()))
        .unwrap();
    let mut flip = false;
    let change = time(20, || {
        flip = !flip;
        let color = if flip { [255, 0, 0] } else { [0, 0, 0] };
        s.set_pen(
            id,
            sk_model::Pen {
                color,
                ..pen.clone()
            },
        );
    });
    assert_eq!(s.build_count(run), builds, "kein Netz neu");
    let pack = time(100, || {
        std::hint::black_box(s.table().looks(1.0));
    });
    let looks = s.table().looks(1.0);
    let kb = std::mem::size_of_val(looks.texels.as_slice()) as f64 / 1e3;
    println!();
    println!("Stift ändern (Schritt + Tabelle): {change:.3} ms, Tabelle packen: {pack:.4} ms");
    println!(
        "Tabelle: {} Schlüssel × {} Zeilen = {kb:.2} KB",
        looks.keys,
        sk_render::LOOK_ROWS
    );
}
