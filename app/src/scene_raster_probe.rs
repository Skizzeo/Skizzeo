//! Probe zu Review L26 und L29: was die Maus über einer Wand je Bewegung
//! kostet (Wand wählen im Grundriss, Strahl in 3D, Öffnung unter der Maus,
//! Gummiband in 3D, Maßketten in 3D nah und fern) bei 100, 1000 und 4000
//! Häusern (je 10 × 8 m mit einer Innenwand und einem Fenster, 20 m Raster).
//!
//! Probe: `cargo test --release -p skizzeo probe_maus_ueber_wand -- --ignored --nocapture`

use crate::bemassung::Massketten;
use crate::camera::Camera;
use crate::scene::Scene;
use crate::ui::ViewKind;
use crate::wall_edit::mass_treffer::treffer;
use crate::wall_edit::tuer_edit::oeffnung_bei;
use crate::wall_edit::WallEdit;
use sk_math::vec3;
use sk_model::oeffnung::{HeightLine, HeightRef, Opening, OpeningKind};
use sk_model::{Category, Model, RefSide};
use sk_paint::font::Font;
use sk_platform::{Event, Modifiers};
use std::time::Instant;

const W: f64 = 1600.0;
const H: f64 = 900.0;

fn siedlung(n: usize) -> Scene {
    let mut m = Model::new();
    m.begin("Siedlung");
    let seite = (n as f64).sqrt().ceil() as usize;
    for i in 0..n {
        let (ox, oy) = ((i % seite) as f64 * 20_000.0, (i / seite) as f64 * 20_000.0);
        let p = |x: f64, y: f64| vec3(ox + x, oy + y, 0.0);
        let b = m.add_building(1);
        let pts = [
            p(0.0, 0.0),
            p(0.0, 8000.0),
            p(10_000.0, 8000.0),
            p(10_000.0, 0.0),
        ];
        let eg = m.build_from_polygon(b, &pts).expect("Haus");
        let st = m.run(eg).unwrap().storey;
        let set = m.defaults().interior_wall;
        m.add_wall_run(
            &[p(4000.0, 0.0), p(4000.0, 5000.0)],
            false,
            RefSide::Center,
            st,
            set,
            Category::InteriorWall,
        )
        .expect("Innenwand");
        m.add_opening(Opening {
            kind: OpeningKind::Window,
            path: vec![p(2510.0, 0.0), p(1500.0, 0.0)],
            bottom: HeightRef::Line(st, HeightLine::Sill),
            top: HeightRef::Line(st, HeightLine::Lintel),
            reveal: None,
            install: Default::default(),
            swing: None,
            lock: Vec::new(),
        })
        .expect("Fenster");
    }
    m.commit();
    Scene::with_model(m)
}

fn ms(t: Instant, reps: usize) -> f64 {
    t.elapsed().as_secs_f64() * 1000.0 / reps as f64
}

#[test]
#[ignore]
fn probe_maus_ueber_wand() {
    println!();
    println!(
        "{:>7} {:>9} {:>9} {:>9} {:>9} {:>9} {:>9} {:>9} {:>9} {:>9} {:>9}",
        "Häuser",
        "Wahl",
        "Strahl3D",
        "Öffnung",
        "Daneben",
        "Gummi3D",
        "Maß3D",
        "MaßFern",
        "Maß×20",
        "Maß×60",
        "MaßErst"
    );
    let mods = Modifiers::default();
    let font = std::fs::read("/usr/share/fonts/truetype/liberation/LiberationSans-Regular.ttf")
        .ok()
        .and_then(Font::parse);
    for n in [100, 1000, 4000] {
        let mut s = siedlung(n);
        // Mitten in der Siedlung, damit ringsum Häuser stehen
        let seite = (n as f64).sqrt().ceil() as usize;
        let k = seite / 2 * seite + seite / 2;
        let o = vec3(
            (k % seite) as f64 * 20_000.0,
            (k / seite) as f64 * 20_000.0,
            0.0,
        );
        let wand = o + vec3(7000.0, 0.0, 0.0);
        let oben = Camera::parallel(
            o + vec3(5000.0, 4000.0, 0.0),
            90f64.to_radians(),
            -89.9f64.to_radians(),
            12_000.0,
        );
        let (x, y) = oben.project(wand, W, H).unwrap();
        let (ro, rd) = oben.ray(x, y, W, H);
        assert!(s.pick(ViewKind::Plan, None, ro, rd).is_some());
        let reps = 200;
        let t = Instant::now();
        for _ in 0..reps {
            std::hint::black_box(s.pick(ViewKind::Plan, None, ro, rd));
        }
        let wahl = ms(t, reps);
        let cam3 = Camera::looking_at(o + vec3(5000.0, -14_000.0, 9000.0), wand, 45.0);
        let (x3, y3) = cam3.project(wand + vec3(0.0, 0.0, 1500.0), W, H).unwrap();
        let (o3, d3) = cam3.ray(x3, y3, W, H);
        assert!(s.raycast(o3, d3).is_some());
        let t = Instant::now();
        for _ in 0..reps {
            std::hint::black_box(s.raycast(o3, d3));
        }
        let strahl = ms(t, reps);
        // Im Geschoss dieses Hauses, wie nach einem Klick darauf
        let ecke = |c: &sk_model::WallChain| {
            c.points
                .iter()
                .any(|p| (p.x - o.x).abs() < 1.0 && (p.y - o.y).abs() < 1.0)
        };
        let run = s
            .model()
            .runs()
            .ids()
            .find(|&r| s.chain(r).is_some_and(ecke));
        let st = s.model().run(run.unwrap()).unwrap().storey;
        assert!(s.set_active_storey(st) || s.active_storey() == st);
        let fenster = o + vec3(2005.0, 100.0, 0.0);
        assert!(oeffnung_bei(&s, fenster).is_some());
        let t = Instant::now();
        for _ in 0..reps {
            std::hint::black_box(oeffnung_bei(&s, fenster));
        }
        let oeffnung = ms(t, reps);
        let neben = o + vec3(15_000.0, 14_000.0, 0.0);
        let t = Instant::now();
        for _ in 0..reps {
            std::hint::black_box(oeffnung_bei(&s, neben));
        }
        let daneben = ms(t, reps);
        // Maus am Wandfuß in 3D, abwechselnd einen Bildpunkt daneben
        let (fx, fy) = cam3.project(wand, W, H).unwrap();
        let mut e = WallEdit::default();
        let t = Instant::now();
        for i in 0..reps {
            let x = fx + (i % 2) as f64;
            e.handle(
                &Event::MouseMove { x, y: fy, mods },
                &mut s,
                &cam3,
                W,
                H,
                1.0,
                true,
            );
        }
        let gummi = ms(t, reps);
        // Maßketten unter der Maus in 3D, nah am Haus und mit der ganzen
        // Siedlung im Bild
        let (lo, hi) = s.bounds().unwrap();
        let mitte = (lo + hi) * 0.5;
        // Abstand f × Diagonale der Siedlung (Review W47 B1: 20 und 60)
        let weit = |f: f64| {
            Camera::looking_at(
                mitte + vec3(0.55, -1.0, 0.9).normalized() * (f * (hi - lo).length()),
                mitte,
                45.0,
            )
        };
        let fern = weit(1.5);
        let mass = |cam: &Camera| {
            let f = font.as_ref()?;
            let mut mk = Massketten::default();
            let st = mk.gezeigt(&s, ViewKind::Persp, &[], Some(f))?;
            let (x, y) = cam.project(wand + vec3(0.0, -2500.0, 0.0), W, H)?;
            // Das erste Zeigen nach einer Änderung baut das Raster der Geschosse
            let t = Instant::now();
            std::hint::black_box(treffer(&st, cam, W, H, 1.0, x, y));
            let erstes = ms(t, 1);
            let t = Instant::now();
            for i in 0..reps {
                let x = x + (i % 2) as f64;
                std::hint::black_box(treffer(&st, cam, W, H, 1.0, x, y));
            }
            Some((ms(t, reps), erstes))
        };
        let nan = (f64::NAN, f64::NAN);
        let ((nah, erstes), (fern, _)) = (mass(&cam3).unwrap_or(nan), mass(&fern).unwrap_or(nan));
        let (f20, f60) = (
            mass(&weit(20.0)).unwrap_or(nan).0,
            mass(&weit(60.0)).unwrap_or(nan).0,
        );
        println!(
            "{n:>7} {wahl:>9.4} {strahl:>9.4} {oeffnung:>9.4} {daneben:>9.4} {gummi:>9.4} \
             {nah:>9.4} {fern:>9.4} {f20:>9.4} {f60:>9.4} {erstes:>9.4}"
        );
    }
    println!(
        "ms je Aufruf. Wahl = Bauteil unter der Maus im Grundriss (Scene::pick), Strahl3D = \
         Wand unter der Maus in 3D (Scene::raycast), Öffnung/Daneben = Öffnung unter der Maus \
         im Loch und im Freien, Gummi3D = Mausbewegung des Wandwerkzeugs am Wandfuß in 3D, \
         Maß3D/MaßFern = Maßzahl oder Maßlinie unter der Maus in 3D nah am Haus und mit der \
         ganzen Siedlung im Bild, Maß×20/×60 = ebenso aus 20 bzw. 60 Diagonalen der Siedlung \
         Abstand, MaßErst = erstes Zeigen nach einer Änderung (baut das \
         Raster der Geschosse)."
    );
}
