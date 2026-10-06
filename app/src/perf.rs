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
            base: 0.0,
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
            base: 0.0,
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

// --- Referenzgebäude (Maßstab nach Jörn, 2026-10-06) -------------------------
//
// Ziel des Programms ist ein Wohngebäude mit drei bis vier Geschossen, Vor- und
// Rücksprüngen, dazu ein bis zwei Nebengebäude oder höchstens ein zweites
// Gebäude. Gemessen wird an diesem Maßstab, nicht an Hunderten Häusern.
//
// Geschosse kennt das Modell noch nicht: Die Geschosse liegen deshalb im
// Grundriss nebeneinander (30 m Abstand), damit sich ihre Wände nicht
// gegenseitig anschließen. Jeder geschlossene Außenzug bekommt heute eine
// Gründung; das sind mehr als später (eine je Gebäude) und misst also eher zu viel.

/// Ein Geschoss des Hauptgebäudes ab `(ox, oy)`: Außenwand mit Vor- und
/// Rücksprüngen (20 Ecken, etwa 18 × 14 m) und 14 Innenwände mit T-Anschlüssen.
fn storey(s: &mut Scene, ox: f64, oy: f64, height: f64) {
    let p = |x: f64, y: f64| vec3(ox + x, oy + y, 0.0);
    let wall = |points, closed, ref_side| WallChain {
        base: 0.0,
        points,
        closed,
        ref_side,
        layers: Vec::new(),
        height,
        joints: Default::default(),
    };
    // Im Uhrzeigersinn: eingezogene Ecken links und rechts, Risalit vorn und hinten
    let outline = [
        (0.0, 1500.0),
        (0.0, 10500.0),
        (1500.0, 10500.0),
        (1500.0, 12000.0),
        (7000.0, 12000.0),
        (7000.0, 13000.0),
        (11000.0, 13000.0),
        (11000.0, 12000.0),
        (16500.0, 12000.0),
        (16500.0, 10500.0),
        (18000.0, 10500.0),
        (18000.0, 1500.0),
        (16500.0, 1500.0),
        (16500.0, 0.0),
        (11000.0, 0.0),
        (11000.0, -1000.0),
        (7000.0, -1000.0),
        (7000.0, 0.0),
        (1500.0, 0.0),
        (1500.0, 1500.0),
    ];
    s.add_wall(&wall(
        outline.iter().map(|&(x, y)| p(x, y)).collect(),
        true,
        RefSide::Left,
    ));
    // Tragende Querwände von der vorderen zur hinteren Innenfläche
    let xs = [3500.0, 6000.0, 12500.0, 15000.0];
    for x in xs {
        let w = wall(vec![p(x, 315.0), p(x, 11685.0)], false, RefSide::Center);
        s.add_wall_as(&w, sk_model::Category::InteriorWall);
    }
    // Flurwände zwischen den Querwänden und zu den Giebeln
    let bays = [
        (1815.0, 3500.0),
        (3500.0, 6000.0),
        (6000.0, 12500.0),
        (12500.0, 15000.0),
        (15000.0, 16185.0),
    ];
    for (x0, x1) in bays {
        for y in [4500.0, 7500.0] {
            let w = wall(vec![p(x0, y), p(x1, y)], false, RefSide::Center);
            s.add_wall_as(&w, sk_model::Category::InteriorWall);
        }
    }
}

/// Nebengebäude (Garage 6 × 9 m) mit einer Innenwand.
fn annex(s: &mut Scene, ox: f64, oy: f64) {
    let p = |x: f64, y: f64| vec3(ox + x, oy + y, 0.0);
    let wall = |points, closed, ref_side| WallChain {
        base: 0.0,
        points,
        closed,
        ref_side,
        layers: Vec::new(),
        height: 2750.0,
        joints: Default::default(),
    };
    let pts = vec![
        p(0.0, 0.0),
        p(0.0, 9000.0),
        p(6000.0, 9000.0),
        p(6000.0, 0.0),
    ];
    s.add_wall(&wall(pts, true, RefSide::Left));
    let w = wall(
        vec![p(315.0, 6000.0), p(5685.0, 6000.0)],
        false,
        RefSide::Center,
    );
    s.add_wall_as(&w, sk_model::Category::InteriorWall);
}

/// Referenz: `buildings` Hauptgebäude mit je `storeys` Geschossen und `annexes`
/// Nebengebäude.
fn reference(buildings: usize, storeys: usize, annexes: usize) -> Scene {
    let mut s = Scene::new();
    for b in 0..buildings {
        for g in 0..storeys {
            storey(&mut s, g as f64 * 30000.0, b as f64 * 30000.0, 2750.0);
        }
    }
    for a in 0..annexes {
        annex(&mut s, a as f64 * 10000.0, -15000.0);
    }
    s
}

#[test]
#[ignore]
fn perf_referenzgebaeude() {
    println!();
    println!(
        "{:<34} {:>5} {:>6} {:>6} {:>7} | {:>7} {:>7} {:>7} {:>7} {:>7} {:>7} {:>7} | {:>6} {:>6} {:>6}",
        "Modell", "Züge", "Wände", "Anschl", "Dreieck", "Ziehen", "Live3D", "Neu", "Greifen", "Rückg.",
        "Netz3D", "NetzGR", "MB", "Speich", "Laden"
    );
    let cases: [(&str, usize, usize, usize); 4] = [
        ("Haus 4 Geschosse", 1, 4, 0),
        ("Referenz: 4 Geschosse + 2 Nebengeb.", 1, 4, 2),
        ("Groß: 2 Häuser à 4 G. + 2 Nebengeb.", 2, 4, 2),
        ("Reserve ×4 (8 Häuser à 4 G.)", 8, 4, 2),
    ];
    for (name, buildings, storeys, annexes) in cases {
        let mut s = reference(buildings, storeys, annexes);
        let runs = s.model().runs().iter().count();
        let walls = s.model().elements().iter().count();
        let joins = s.model().joins().len();
        let tris = s.mesh(ViewKind::Persp, None, &[]).faces.len() / 3;
        // Gezogen wird die Front des obersten Geschosses (Segment 1, linker Giebel
        // mit zwei Anschlüssen an Flurwände)
        let run = s.model().runs().ids().next().unwrap();
        let orig = s.chain(run).unwrap().clone();
        let mut flip = false;
        s.begin("Wand verschieben");
        let drag = time(20, || {
            flip = !flip;
            let moved = orig
                .with_segment_moved(0, if flip { -100.0 } else { -200.0 })
                .unwrap_or_else(|| orig.clone());
            s.set_run_points(run, &moved.points);
        });
        let live_set = s.live_set(run);
        let live = time(20, || {
            std::hint::black_box(s.mesh_runs(ViewKind::Persp, None, &live_set));
        });
        s.commit();
        let undo = time(10, || {
            s.undo();
            s.redo();
        }) / 2.0;
        let cam = Camera::looking_at(
            vec3(-20000.0, -30000.0, 25000.0),
            vec3(9000.0, 6000.0, 0.0),
            45.0,
        );
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
        let m3 = time(10, || {
            std::hint::black_box(s.mesh(ViewKind::Persp, None, &[]));
        });
        let mgr = time(10, || {
            std::hint::black_box(s.mesh(ViewKind::Plan, None, &[]));
        });
        let mb = bytes(&s.mesh(ViewKind::Persp, None, &[])) as f64 / 1e6;
        let mut text = String::new();
        let save = time(5, || text = sk_model::szo::write(s.model()));
        let load = time(5, || {
            let l = sk_model::szo::read(&text, sk_model::GuidGen::with_seed(1)).unwrap();
            let mut sc = Scene::with_model(l.model);
            std::hint::black_box(sc.mesh(ViewKind::Persp, None, &[]));
        });
        println!(
            "{:<34} {:>5} {:>6} {:>6} {:>7} | {:>7.3} {:>7.3} {:>7.3} {:>7.3} {:>7.3} {:>7.3} {:>7.3} | {:>6.2} {:>6.2} {:>6.2}",
            name, runs, walls, joins, tris, drag, live, drag + live, pick, undo, m3, mgr, mb, save, load
        );
    }
    println!(
        "Zeiten in ms. Ziehen/Live3D/Neu/Greifen je Mausbewegung, Rückg. je Schritt, \
         Netz3D/NetzGR beim Greifen, Loslassen oder Ansichtswechsel, Speich/Laden einmalig \
         (Laden = Datei lesen + Szene + erstes Netz). MB = volles Netz zur Grafikkarte."
    );
}

/// Paneel „Geschosse“ (E14): OK EG ziehen. Je Bild ändern sich alle daran
/// gebundenen Decken und Wände; gemessen wird Modell plus 3D-Netz.
#[test]
#[ignore]
fn perf_ebene_ziehen() {
    println!();
    for (name, buildings, storeys, annexes) in [
        ("Referenz: 4 Geschosse + 2 Nebengeb.", 1, 4, 2),
        ("Groß: 2 Häuser à 4 G. + 2 Nebengeb.", 2, 4, 2),
    ] {
        let mut s = reference(buildings, storeys, annexes);
        let eg = s.model().defaults().storey;
        let mut flip = false;
        s.begin("Geschoss ziehen");
        let drag = time(20, || {
            flip = !flip;
            s.drag_storey_top(eg, if flip { 2500.0 } else { 2600.0 });
            std::hint::black_box(s.mesh(ViewKind::Persp, None, &[]));
        });
        // `time` ruft einmal vorab auf: Loslassen direkt messen
        let t = Instant::now();
        s.commit();
        let release = t.elapsed().as_secs_f64() * 1000.0;
        println!("{name:<36} Ziehen {drag:7.2} ms je Bild, Loslassen {release:7.2} ms");
    }
}

// ---------------------------------------------------------------------------
// Ab B12 (Obergeschoss): Geschosse liegen wirklich übereinander. Der
// geschlossene Außenzug im EG erzeugt die gekoppelten Züge darüber; die
// Innenwände liegen je Geschoss.
// ---------------------------------------------------------------------------

/// Legt ein Gebäude mit `storeys` Geschossen an und macht sein EG aktiv.
fn new_building(s: &mut Scene, storeys: u8) {
    s.edit_model("Gebäude erstellt", |m| {
        m.add_building(storeys);
        true
    });
    let b = s.model().buildings().ids().last().unwrap();
    let eg = s.model().ground_of(Some(b)).unwrap();
    s.set_active_storey(eg);
}

/// Hauptgebäude mit `storeys` Geschossen ab `(ox, oy)`: Außenwand wie
/// [`storey`] einmal im EG (gestapelt bis oben), Innenwände in jedem Geschoss.
fn stacked_house(s: &mut Scene, ox: f64, oy: f64, storeys: u8) {
    new_building(s, storeys);
    let b = s.active_building();
    let levels: Vec<_> = s
        .model()
        .levels_in(b)
        .into_iter()
        .filter(|id| {
            s.model()
                .storey(*id)
                .is_some_and(|st| st.kind != sk_model::LevelKind::Foundation)
        })
        .collect();
    let eg = levels[0];
    for (i, id) in levels.into_iter().enumerate() {
        if i > 0 {
            s.set_active_storey(id);
        }
        // `storey` legt die Außenwand im EG des aktiven Gebäudes an; über dem
        // EG nur die Innenwände (die Außenwand steht dort schon gekoppelt)
        let before = s.model().runs().len();
        storey_walls(s, ox, oy, i == 0);
        assert!(s.model().runs().len() > before);
    }
    s.set_active_storey(eg);
}

/// Wände eines Geschosses wie in [`storey`]; die Außenwand nur mit `outer`.
fn storey_walls(s: &mut Scene, ox: f64, oy: f64, outer: bool) {
    let p = |x: f64, y: f64| vec3(ox + x, oy + y, 0.0);
    let wall = |points, closed, ref_side| WallChain {
        base: 0.0,
        points,
        closed,
        ref_side,
        layers: Vec::new(),
        height: 2750.0,
        joints: Default::default(),
    };
    if outer {
        let outline = [
            (0.0, 1500.0),
            (0.0, 10500.0),
            (1500.0, 10500.0),
            (1500.0, 12000.0),
            (7000.0, 12000.0),
            (7000.0, 13000.0),
            (11000.0, 13000.0),
            (11000.0, 12000.0),
            (16500.0, 12000.0),
            (16500.0, 10500.0),
            (18000.0, 10500.0),
            (18000.0, 1500.0),
            (16500.0, 1500.0),
            (16500.0, 0.0),
            (11000.0, 0.0),
            (11000.0, -1000.0),
            (7000.0, -1000.0),
            (7000.0, 0.0),
            (1500.0, 0.0),
            (1500.0, 1500.0),
        ];
        s.add_wall(&wall(
            outline.iter().map(|&(x, y)| p(x, y)).collect(),
            true,
            RefSide::Left,
        ));
    }
    for x in [3500.0, 6000.0, 12500.0, 15000.0] {
        let w = wall(vec![p(x, 315.0), p(x, 11685.0)], false, RefSide::Center);
        s.add_wall_as(&w, sk_model::Category::InteriorWall);
    }
    let bays = [
        (1815.0, 3500.0),
        (3500.0, 6000.0),
        (6000.0, 12500.0),
        (12500.0, 15000.0),
        (15000.0, 16185.0),
    ];
    for (x0, x1) in bays {
        for y in [4500.0, 7500.0] {
            let w = wall(vec![p(x0, y), p(x1, y)], false, RefSide::Center);
            s.add_wall_as(&w, sk_model::Category::InteriorWall);
        }
    }
}

/// Referenz ab B12: `houses` Häuser mit `storeys` Geschossen übereinander
/// und `annexes` eingeschossige Nebengebäude.
fn reference_stacked(houses: usize, storeys: u8, annexes: usize) -> Scene {
    let mut s = Scene::new();
    for h in 0..houses {
        stacked_house(&mut s, 0.0, h as f64 * 30000.0, storeys);
    }
    for a in 0..annexes {
        new_building(&mut s, 1);
        annex(&mut s, a as f64 * 10000.0, -15000.0);
    }
    s
}

/// B12: Wand ziehen am EG-Fuß (alle Geschosse darüber folgen), OK EG ziehen
/// und Loslassen am Referenzgebäude mit echten Obergeschossen.
#[test]
#[ignore]
fn perf_obergeschoss() {
    println!();
    println!(
        "{:<34} {:>5} {:>6} {:>7} | {:>7} {:>7} {:>7} {:>7} {:>7} | {:>7} {:>7} {:>7} | {:>6}",
        "Modell (gestapelt)",
        "Züge",
        "Teile",
        "Dreieck",
        "Ziehen",
        "Live3D",
        "Neu",
        "Loslas",
        "Rückg.",
        "Ebene",
        "Loslas",
        "Netz3D",
        "Laden"
    );
    for (name, houses, storeys, annexes) in [
        ("Haus 4 Geschosse", 1, 4u8, 0),
        ("Referenz: 4 Geschosse + 2 Nebengeb.", 1, 4, 2),
        ("Groß: 2 Häuser à 4 G. + 2 Nebengeb.", 2, 4, 2),
        ("Reserve ×4 (8 Häuser à 4 G.)", 8, 4, 2),
    ] {
        let mut s = reference_stacked(houses, storeys, annexes);
        assert!(s.model().check().is_empty(), "{:?}", s.model().check());
        let runs = s.model().runs().len();
        let parts = s.model().elements().len();
        let tris = s.mesh(ViewKind::Persp, None, &[]).faces.len() / 3;
        // EG-Außenwand des ersten Hauses: Segment 0 nach außen schieben
        let run = s
            .model()
            .runs()
            .ids()
            .find(|r| !s.model().stack_above(*r).is_empty())
            .unwrap();
        let above = s.model().stack_above(run).len();
        assert_eq!(above, storeys as usize - 1, "Züge darüber");
        let orig = s.chain(run).unwrap().clone();
        let mut flip = false;
        s.begin("Wand verschieben");
        let drag = time(20, || {
            flip = !flip;
            let moved = orig
                .with_segment_moved(0, if flip { -100.0 } else { -200.0 })
                .unwrap_or_else(|| orig.clone());
            s.set_run_points(run, &moved.points);
        });
        let live_set = s.live_set(run);
        let live = time(20, || {
            std::hint::black_box(s.mesh_runs(ViewKind::Persp, None, &live_set));
        });
        let t = Instant::now();
        s.commit();
        let release = t.elapsed().as_secs_f64() * 1000.0;
        let undo = time(10, || {
            s.undo();
            s.redo();
        }) / 2.0;
        let eg = s.active_storey();
        let mut flip = false;
        s.begin("Geschoss ziehen");
        let level = time(20, || {
            flip = !flip;
            s.drag_storey_top(eg, if flip { 2800.0 } else { 2900.0 });
            std::hint::black_box(s.mesh(ViewKind::Persp, None, &[]));
        });
        let t = Instant::now();
        s.commit();
        let level_release = t.elapsed().as_secs_f64() * 1000.0;
        let m3 = time(10, || {
            std::hint::black_box(s.mesh(ViewKind::Persp, None, &[]));
        });
        let text = sk_model::szo::write(s.model());
        if let Err(e) = sk_model::szo::read(&text, sk_model::GuidGen::with_seed(1)) {
            let l = text.lines().nth(e.line.saturating_sub(1)).unwrap_or("");
            panic!("{e:?}: {l}");
        }
        let load = time(5, || {
            let l = sk_model::szo::read(&text, sk_model::GuidGen::with_seed(1)).unwrap();
            let mut sc = Scene::with_model(l.model);
            std::hint::black_box(sc.mesh(ViewKind::Persp, None, &[]));
        });
        println!(
            "{:<34} {:>5} {:>6} {:>7} | {:>7.3} {:>7.3} {:>7.3} {:>7.3} {:>7.3} | {:>7.3} {:>7.3} {:>7.3} | {:>6.2}",
            name, runs, parts, tris, drag, live, drag + live, release, undo, level, level_release, m3, load
        );
    }
    println!(
        "Zeiten in ms. Ziehen/Live3D/Neu je Mausbewegung (EG-Fuß, alle Geschosse darüber \
         folgen), Ebene = OK EG ziehen inkl. 3D-Netz je Bild, Loslas = Loslassen einmalig."
    );
}
