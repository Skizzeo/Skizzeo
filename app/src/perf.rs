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
    // Flächen 48 B je Ecke; Kanten als Instanz mit 28 B
    m.faces.len() * 48 + m.edges.len() * 28
}

#[test]
#[ignore]
fn perf_griff_ziehen() {
    println!();
    println!(
        "{:>6} {:>5} {:>7} | {:>8} {:>8} {:>8} {:>8} {:>8} {:>8} {:>8} | {:>8} {:>8} {:>8} {:>8}",
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
        "MB/Live"
    );
    for &(n, segs) in &[(1, 4), (10, 4), (100, 4), (1000, 4), (1, 200), (100, 40)] {
        let mut s = town(n, segs);
        let tris = s.mesh(ViewKind::Persp, None, None).faces.len() / 3;
        let cam = Camera::looking_at(
            vec3(-20000.0, -30000.0, 25000.0),
            vec3(5000.0, 5000.0, 0.0),
            45.0,
        );
        let run = s.model().runs().ids().next().unwrap();
        let orig = s.chain(run).unwrap().clone();
        let mut flip = false;
        // Ein Ziehschritt: Segment verschieben und Szene neu aufbauen
        let drag = time(20, || {
            flip = !flip;
            let moved = orig
                .with_segment_moved(0, if flip { 1.0 } else { 2.0 })
                .unwrap_or_else(|| orig.clone());
            s.set_run_points(run, &moved.points);
        });
        let mut sect = SectionLine::default();
        sect.ensure(&s);
        let plane = sect.plane();
        let m3 = time(10, || {
            std::hint::black_box(s.mesh(ViewKind::Persp, None, None));
        });
        let mgr = time(10, || {
            std::hint::black_box(s.mesh(ViewKind::Plan, None, None));
        });
        let msc = time(10, || {
            std::hint::black_box(s.mesh(ViewKind::Section, plane, None));
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
            std::hint::black_box(s.mesh_run(ViewKind::Persp, None, run));
        });
        let up = bytes(&s.mesh(ViewKind::Persp, None, None)) as f64 / 1e6;
        let up_live = bytes(&s.mesh_run(ViewKind::Persp, None, run)) as f64 / 1e6;
        println!(
            "{:>6} {:>5} {:>7} | {:>8.3} {:>8.3} {:>8.3} {:>8.3} {:>8.3} {:>8.3} {:>8.3} | {:>8.3} {:>8.3} {:>8.2} {:>8.3}",
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
            up_live
        );
    }
    println!(
        "Zeiten in ms je Aufruf. Ziehen = Wandzug verschieben und neu berechnen. \
         Voll = Ziehen + ganzes Netz3D (ein Bild ohne Live-Netz), Neu = Ziehen + Live3D \
         (ein Bild beim Ziehen in 3D). MB/Voll bzw. MB/Live = Daten zur Grafikkarte."
    );
}
