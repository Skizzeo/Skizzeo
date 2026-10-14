//! Leistung mit Aufmaß bei vielen Häusern (Review #129): Je Wandbewegung,
//! je Zoomstufe und je Sichtwechsel wächst die Zeit nicht mit der Zahl der
//! Häuser, und die Rechnung im Faden nach einem Schritt bleibt bei 1000
//! Häusern unter 100 ms. Gemessen wie in #153: einmal aufwärmen, dann der
//! Bestwert aus mindestens sieben Läufen über mindestens 300 ms, denn die
//! Tests laufen nebeneinander.

use super::*;
use crate::gelaende_app::{leiste, Zustand};
use crate::scene::Scene;
use sk_model::{Crs, Terrain, TerrainPoint};
use std::time::{Duration, Instant};

/// `n` Häuser 10 × 8 m im Raster zu 15 m, je eines ein Gebäude, auf einem
/// gewellten örtlichen Aufmaß (wie `viele_haeuser` in `scene_lage`).
fn viele(n: usize) -> Model {
    let mut m = Model::with_seed(5);
    for i in 0..n {
        let b = m.add_building(1);
        let (ox, oy) = ((i % 64) as f64 * 15_000.0, (i / 64) as f64 * 15_000.0);
        let p = |x: f64, y: f64| vec3(ox + x, oy + y, 0.0);
        let pts = [p(0.0, 0.0), p(0.0, 8000.0), p(1e4, 8000.0), p(1e4, 0.0)];
        m.build_from_polygon(b, &pts).unwrap();
    }
    let tiefe = n.div_ceil(64) as f64 * 15.0 + 80.0;
    let (nx, ny) = (53, (tiefe / 20.0) as usize + 1);
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
            });
        }
    }
    m.begin("Aufmaß");
    assert!(m.import_terrain(t));
    m.commit();
    m
}

/// Bestwert von `f` in ms: einmal aufwärmen, dann mindestens sieben Läufe
/// über mindestens 300 ms.
fn bestwert(mut f: impl FnMut()) -> f64 {
    f();
    let (start, mut best, mut k) = (Instant::now(), f64::INFINITY, 0);
    while k < 7 || start.elapsed() < Duration::from_millis(300) {
        let t = Instant::now();
        f();
        best = best.min(t.elapsed().as_secs_f64() * 1000.0);
        k += 1;
    }
    best
}

fn ms(t: Instant) -> f64 {
    t.elapsed().as_secs_f64() * 1000.0
}

/// Zeiten in ms bei einer Zahl von Häusern.
#[derive(Debug)]
struct Zeiten {
    /// Wand im offenen Schritt bewegen, ihr Netz und das Raster (B1), mit
    /// Aufmaß und zum Vergleich ohne. Ohne Aufmaß wächst das Bewegen selbst
    /// mit der Zahl der Häuser (im Modell, wie auf main); das Aufmaß legt
    /// nichts mehr darauf.
    wand: f64,
    wand_ohne: f64,
    /// Leiste „Gelände“ nach einem Schritt, Lage schon gerechnet (B2).
    leiste: f64,
    /// Linien im Grundriss je Zoomstufe (B3).
    zoom: f64,
    /// Sichtwechsel 3D → Grundriss → 3D (gemerkter Körper) und ein neuer
    /// Schnitt, der in den Faden geht (B4).
    sicht: f64,
    schnitt: f64,
    /// Was der Faden nach einem Schritt rechnet: Platten, Gelände, Körper.
    faden: f64,
    /// Davon das Gelände und sein Körper; das erste Bild rechnet das
    /// Gelände im UI-Faden.
    gelaende: f64,
    koerper: f64,
}

/// Wie beim Ziehen einer Wand je Mausbewegung: den ersten Zug bewegen,
/// sein Netz und das Raster; danach losgelassen.
fn wand_ziehen(s: &mut Scene) -> f64 {
    let run = s.model().runs().ids().next().unwrap();
    let orig = s.chain(run).unwrap().clone();
    s.terrain_grid();
    s.begin("Wand verschieben");
    let mut d = 0.0;
    let ms = bestwert(|| {
        d = if d == 300.0 { 600.0 } else { 300.0 };
        let moved = orig.with_segment_moved(0, d).unwrap();
        assert!(s.set_run_points(run, &moved.points));
        std::hint::black_box(s.terrain_grid());
        std::hint::black_box(s.mesh_runs(ViewKind::Persp, None, &[run]).faces.len());
    });
    s.commit();
    ms
}

fn messen(n: usize) -> Zeiten {
    let m = viele(n);
    let mut ohne = m.clone();
    ohne.begin("ohne Aufmaß");
    assert!(ohne.set_terrain(None));
    ohne.commit();
    let wand_ohne = wand_ziehen(&mut Scene::with_model(ohne));
    let mut s = Scene::with_model(m);
    assert!(s.terrain_grid().is_some());
    let wand = wand_ziehen(&mut s);
    // B2: einmal je Schritt, die Lage rechnet der Schritt ohnehin
    let m = s.model();
    assert!(m.terrain_nhn().is_some());
    let z = Zustand::default();
    let leiste = bestwert(|| {
        std::hint::black_box(leiste(m, &z));
    });
    // Gelände im UI-Faden beim ersten Bild, B3 und B4
    let mut pl = Platte::default();
    let t0 = Instant::now();
    pl.im_bild(m);
    assert!(pl.gelaende.is_some());
    let gelaende = ms(t0);
    let t = DrawTable::resolve(m, &Theme::dark());
    let plan = (ViewKind::Plan, None);
    let mut px = 10.0;
    let zoom = bestwert(|| {
        px = if px == 10.0 { 12.5 } else { 10.0 };
        std::hint::black_box(pl.linien_im_bild(m, plan, None, &t, (1.0, px)).len());
    });
    let sicht_von = |v: ViewKind, plane| -> Sicht { (v, plane, [40, 41, 42], false) };
    let persp = sicht_von(ViewKind::Persp, None);
    let grund = sicht_von(ViewKind::Plan, None);
    let warten = |pl: &mut Platte| {
        while pl.rechnet() {
            pl.nachziehen(m, persp);
            std::thread::sleep(Duration::from_millis(1));
        }
    };
    let key = |pl: &Platte, s: &Sicht| netz_key(pl.fassung, s);
    assert!(pl.netz_zur_sicht(key(&pl, &persp), persp).is_none());
    warten(&mut pl);
    let sicht = bestwert(|| {
        assert!(pl.netz_zur_sicht(key(&pl, &grund), grund).is_some());
        assert!(pl.netz_zur_sicht(key(&pl, &persp), persp).is_some());
    });
    // je neuem Schnitt nur der Start des Fadens, ohne das Warten
    let (start, mut schnitt, mut k) = (Instant::now(), f64::INFINITY, 0);
    while k < 8 || start.elapsed() < Duration::from_millis(300) {
        let y = 1000.0 * k as f64;
        let ebene = sicht_von(
            ViewKind::Section,
            Some((vec3(0.0, y, 0.0), vec3(0.0, 1.0, 0.0))),
        );
        let t = Instant::now();
        assert!(pl.netz_zur_sicht(key(&pl, &ebene), ebene).is_none());
        if k > 0 {
            schnitt = schnitt.min(ms(t));
        }
        warten(&mut pl);
        k += 1;
    }
    // im Faden nach einem Schritt
    let kopie = m.clone();
    let (mut koerper, mut faden) = (f64::INFINITY, f64::INFINITY);
    for _ in 0..3 {
        let t0 = Instant::now();
        let e = kopie.terrain_plate_input();
        let g = Gelaende::aus(&kopie);
        let p = e.build();
        let t1 = Instant::now();
        let netz = netz_der_sicht(&p, g.as_ref(), persp);
        koerper = koerper.min(ms(t1));
        faden = faden.min(ms(t0));
        assert!(!netz.faces.is_empty());
    }
    Zeiten {
        wand,
        wand_ohne,
        leiste,
        zoom,
        sicht,
        schnitt,
        faden,
        gelaende,
        koerper,
    }
}

/// Bei 1000 Häusern (im Debug 64): je Ereignis wenige Millisekunden, der
/// Faden unter 100 ms (Review #129 B1 bis B4).
#[test]
fn aufmass_im_budget() {
    let (n, f) = if cfg!(debug_assertions) {
        (64, 40.0)
    } else {
        (1000, 1.0)
    };
    let z = messen(n);
    eprintln!("{n} Häuser mit Aufmaß: {z:?}");
    // vorher je Bewegung 11 ms mehr als ohne Aufmaß, je Zoomstufe 233 ms,
    // je Sichtwechsel 1,1 s im UI-Faden
    assert!(z.wand < 1.3 * z.wand_ohne + 1.0 * f, "{z:?}");
    assert!(z.leiste < 4.0 * f, "{z:?}");
    assert!(z.zoom < 1.0 * f, "{z:?}");
    assert!(z.sicht < 0.5 * f, "{z:?}");
    assert!(z.schnitt < 1.0 * f, "{z:?}");
    assert!(z.faden < 100.0 * f && z.koerper < z.faden, "{z:?}");
    assert!(z.gelaende < 100.0 * f, "{z:?}");
}

/// Messprogramm: dasselbe bei 1000 und 4000 Häusern; je Zoomstufe und
/// Sichtwechsel höchstens wenig mehr bei 4000 als bei 1000, die Wand mit
/// Aufmaß kaum mehr als ohne. Nur im Release sinnvoll: `cargo test
/// -p skizzeo --release aufmass_4000 -- --ignored --nocapture`.
#[test]
#[ignore = "Messprogramm, 4000 Häuser"]
fn aufmass_4000() {
    let a = messen(1000);
    let b = messen(4000);
    eprintln!("1000: {a:?}\n4000: {b:?}");
    assert!(b.wand < 1.3 * b.wand_ohne + 1.0, "{b:?}");
    for (x, y, was) in [
        (a.zoom, b.zoom, "Zoom"),
        (a.sicht, b.sicht, "Sicht"),
        (a.schnitt, b.schnitt, "Schnitt"),
    ] {
        assert!(y < 2.0 * x + 0.5, "{was}: {x:.2} → {y:.2} ms");
    }
}

/// Ein Lageplan wie das Beispiel „Am Lindenhof 7“ mit `k` Strichen: zu
/// den Linien des Blatts kurze Striche im Raster über ihrem Bereich.
fn lageplan(k: usize) -> sk_model::SitePlan {
    const PDF: &[u8] =
        include_bytes!("../../../crates/sk-gelaende/tests/daten/lageplan-lindenhof-7.pdf");
    let mut p = sk_model::SitePlan::read(PDF, 32).unwrap();
    let (mut lo, mut hi) = ([f64::MAX; 2], [f64::MIN; 2]);
    for q in p.lines.iter().flat_map(|l| &l.0) {
        lo = [lo[0].min(q[0]), lo[1].min(q[1])];
        hi = [hi[0].max(q[0]), hi[1].max(q[1])];
    }
    let n = (k as f64).sqrt().ceil() as usize;
    let (dx, dy) = ((hi[0] - lo[0]) / n as f64, (hi[1] - lo[1]) / n as f64);
    let mut i = 0;
    while p.lines.len() < k {
        let a = [lo[0] + (i % n) as f64 * dx, lo[1] + (i / n) as f64 * dy];
        let b = [a[0] + 0.7 * dx, a[1] + 0.3 * dy];
        p.lines
            .push((vec![a, b], false, sk_model::UnderlayKind::Other));
        i += 1;
    }
    p
}

/// Zeiten in ms mit einem Lageplan.
#[derive(Debug)]
struct Lageplan {
    /// Je Bild im Grundriss, die Modellgrenzen ändern sich wie im Zug:
    /// der Schlüssel der Unterlage und die übrigen Linien.
    bild: f64,
    /// Einmal je neuer Unterlage: ihre Linien für den Renderer bauen.
    bauen: f64,
    /// Nach einem Schritt im UI-Faden: den Nachzug beginnen.
    nachzug: f64,
}

fn lageplan_messen(n: usize, k: usize) -> Lageplan {
    let mut m = viele(n);
    m.begin("ohne Aufmaß");
    assert!(m.set_terrain(None));
    m.commit();
    let plan = lageplan(k);
    m.begin("Lageplan");
    assert!(m.import_parcel(&plan, "lageplan.pdf", None));
    m.commit();
    let mut s = Scene::with_model(m);
    let mut pl = Platte::default();
    pl.unterlage.merken(s.model(), Arc::new(plan));
    let m = s.model();
    let t = DrawTable::resolve(m, &Theme::dark());
    pl.im_bild(m);
    let (key, g) = pl.unterlage_im_bild(m, true, &t, 1.0);
    let g = g.unwrap();
    let (t0, linien) = (
        Instant::now(),
        gelaende_netz::unterlage_linien(&g, ViewKind::Plan, &t, 1.0),
    );
    let bauen = ms(t0);
    assert!(linien.len() >= k / 2, "{} Striche", linien.len());
    // je Bild: derselbe Schlüssel, kein Hochladen; die Grenzen ändern
    // sich wie beim Ziehen einer Wand
    let (lo, hi) = s.bounds().unwrap();
    let mut d = 0.0;
    let bild = bestwert(|| {
        d = if d == 0.0 { 100.0 } else { 0.0 };
        let b = Some((lo, vec3(hi.x + d, hi.y, hi.z)));
        assert_eq!(pl.unterlage_im_bild(m, true, &t, 1.0).0, key);
        std::hint::black_box(pl.linien_im_bild(m, (ViewKind::Plan, None), b, &t, (1.0, 10.0)));
    });
    // nach jedem Schritt: Modell kopieren und den Faden starten; die Linien
    // der Unterlage sind gemerkt, der Faden bildet sie nicht neu ab
    let sicht: Sicht = (ViewKind::Plan, None, [40, 41, 42], false);
    let run = s.model().runs().ids().next().unwrap();
    let orig = s.chain(run).unwrap().clone();
    let mut nachzug = f64::INFINITY;
    for i in 0..6 {
        let moved = orig
            .with_segment_moved(0, 300.0 * (1 + i % 2) as f64)
            .unwrap();
        s.begin("Wand verschieben");
        assert!(s.set_run_points(run, &moved.points));
        s.commit();
        let m = s.model();
        let t0 = Instant::now();
        pl.nachziehen(m, sicht);
        nachzug = nachzug.min(ms(t0));
        assert!(pl.rechnet());
        while pl.rechnet() {
            pl.nachziehen(m, sicht);
            std::thread::sleep(Duration::from_millis(1));
        }
        assert!(pl.unterlage.abbild(m).unwrap().gemerkt());
    }
    Lageplan {
        bild,
        bauen,
        nachzug,
    }
}

/// Lageplan unter dem Gelände (Review #139): Je Bild und je Mausbewegung
/// wächst nichts mit der Zahl der Striche, der Nachzug nach einem Schritt
/// beginnt in derselben Zeit; nur das einmalige Bauen der Linien für den
/// Renderer wächst mit ihnen. Bei 1000 Häusern und 50.000 Strichen (im
/// Debug 64 Häuser und 5.000 Striche).
#[test]
fn lageplan_im_budget() {
    let (n, k, f) = if cfg!(debug_assertions) {
        (64, 5_000, 40.0)
    } else {
        (1000, 50_000, 1.0)
    };
    let klein = lageplan_messen(n, 300);
    let gross = lageplan_messen(n, k);
    eprintln!("{n} Häuser, Lageplan 300 Striche: {klein:?}\n{k} Striche: {gross:?}");
    // vorher je Bild 30 ms bei 50.000 Strichen, je Nachzug 4,3 ms mehr
    assert!(
        gross.bild < 2.0 * klein.bild + 0.2 * f,
        "{klein:?} {gross:?}"
    );
    assert!(gross.bild < 1.0 * f, "{gross:?}");
    assert!(
        gross.nachzug < 2.0 * klein.nachzug + 1.0 * f,
        "{klein:?} {gross:?}"
    );
    assert!(gross.nachzug < 8.0 * f, "{gross:?}");
    assert!(gross.bauen > klein.bauen);
}
