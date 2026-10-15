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
        let u = Uhr::an();
        f();
        best = best.min(u.ms());
        k += 1;
    }
    best
}

fn ms(t: Instant) -> f64 {
    t.elapsed().as_secs_f64() * 1000.0
}

/// Wie lange dieser Faden bisher lauffähig auf eine CPU gewartet hat, in
/// ms (Linux: zweites Feld von /proc/thread-self/schedstat; sonst 0).
fn auf_cpu_gewartet() -> f64 {
    std::fs::read_to_string("/proc/thread-self/schedstat")
        .ok()
        .and_then(|s| s.split_whitespace().nth(1)?.parse::<f64>().ok())
        .map_or(0.0, |ns| ns / 1e6)
}

/// Uhr für ein Bild im UI-Faden: Wanduhr ohne die Zeit, in der der Faden
/// lauffähig auf eine CPU wartete. Die nebeneinander laufenden Tests
/// verdrängen ihn sonst einmal für 10 bis 20 ms, ohne dass das Bild mehr
/// Arbeit hätte; Warten auf Sperren oder den Faden der Ecken zählt weiter.
struct Uhr(Instant, f64);

impl Uhr {
    fn an() -> Uhr {
        Uhr(Instant::now(), auf_cpu_gewartet())
    }

    fn ms(&self) -> f64 {
        (ms(self.0) - (auf_cpu_gewartet() - self.1)).max(0.0)
    }
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

/// Loslassen bei offener Leiste mit vielen Zügen am bestimmenden Gebäude
/// und großem Aufmaß (Review #196 B1: im UI-Faden 20 ms bei 100.000
/// Punkten und 25 Zügen, 179 ms bei 326.000 und 100): Die Baugruben zählt
/// ein Faden; im UI-Faden bleiben die Leiste und der Start des Fadens.
#[test]
fn baugruben_im_faden() {
    let (k, n, f) = if cfg!(debug_assertions) {
        (9, 60, 40.0)
    } else {
        (25, 317, 1.0)
    };
    let mut m = Model::with_seed(5);
    let b = m.add_building(1);
    for i in 0..k {
        let (ox, oy) = ((i % 5) as f64 * 15_000.0, (i / 5) as f64 * 15_000.0);
        let p = |x: f64, y: f64| vec3(ox + x, oy + y, 0.0);
        let pts = [p(0.0, 0.0), p(0.0, 8000.0), p(1e4, 8000.0), p(1e4, 0.0)];
        m.build_from_polygon(b, &pts).unwrap();
    }
    let mut t = Terrain::new(Crs::Local);
    for i in 0..n {
        for j in 0..n {
            let (e, nn) = (1000.0 + 2.0 * i as f64, 2000.0 + 2.0 * j as f64);
            t.points.push(TerrainPoint {
                nr: (i * n + j + 1).to_string(),
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
    assert_eq!(m.runs().iter().count(), k);
    let mut z = Zustand::default();
    z.offen = true;
    let l = bestwert(|| {
        std::hint::black_box(leiste(&m, &z));
    });
    let u = Uhr::an();
    assert!(!z.baugruben(&m));
    let start = u.ms();
    let t0 = Instant::now();
    while z.prueft() {
        std::thread::sleep(Duration::from_millis(1));
        z.baugruben(&m);
    }
    let faden = ms(t0);
    assert!(
        !leiste(&m, &z).deckt_nicht,
        "das Aufmaß deckt alle Baugruben"
    );
    // danach Sockel und Einbindung aus dem Faden (H-196h)
    let d = bestwert(|| {
        std::hint::black_box(leiste(&m, &z));
    });
    eprintln!(
        "{k} Züge, {} Punkte: Leiste {l:.2} ms, danach {d:.2} ms, Start {start:.2} ms, Faden {faden:.1} ms",
        n * n
    );
    assert!(l < 4.0 * f, "Leiste {l:.2} ms");
    assert!(d < 1.0 * f, "Leiste aus dem Faden {d:.2} ms");
    assert!(start < 4.0 * f, "Start {start:.2} ms");
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
    /// die Schlüssel der Unterlage und die übrigen Linien.
    bild: f64,
    /// Einmal je neuer Unterlage: alle Kacheln für den Renderer bauen
    /// (Linien und Ecken).
    bauen: f64,
    /// Nach einem Schritt im UI-Faden: den Nachzug beginnen.
    nachzug: f64,
    /// Nach einem Wandschritt über dem Plan im UI-Faden, bis die neuen
    /// Kacheln hochgeladen sind: höchste Zeit je Bild, und wie viel je Bild
    /// hochgeladen wird (MB); die Ecken baut ein eigener Faden. `schritt`
    /// ist der zweithöchste der sechs Schritte, `schritt_max` der höchste:
    /// Ein Schritt darf im parallelen Gesamtlauf einmal verdrängt werden,
    /// ohne dass das Bild mehr Arbeit hätte (Review #188 H-188c).
    schritt: f64,
    schritt_max: f64,
    hoch: f64,
    /// Dasselbe als Median der Schritte: ein einzelner Ausreißer im
    /// parallelen Gesamtlauf kippt den Vergleich der Plangrößen nicht.
    schritt_mitte: f64,
    /// EG-Grundriss → 3D → EG-Grundriss: Zeit im ersten Bild zurück; es
    /// lädt nichts hoch (Review #139 N2).
    rueckweg: f64,
    /// Gelände verschieben: höchste Zeit je Bild im UI-Faden, bis alle
    /// Kacheln neu hochgeladen sind, und wie viel je Bild und insgesamt
    /// hochgeladen wird (MB; Review #139 N3: über mehrere Bilder verteilt).
    lage: f64,
    lage_hoch: f64,
    lage_gesamt: f64,
    /// Kacheln je Plan und höchstens neu je Wandschritt.
    kacheln: usize,
    neu: usize,
}

/// Linien und Ecken der Kacheln `ix`, wie der Faden sie für den Renderer
/// baut: Zeit in ms.
fn kacheln_bauen(g: &Gelaende, ix: &[usize], t: &DrawTable) -> f64 {
    let (t0, stil) = (Instant::now(), gelaende_netz::unterlage_stil(t, 1.0));
    for &i in ix {
        let l = gelaende_netz::unterlage_kachel(g, i, stil);
        std::hint::black_box(sk_render::helper_ecken(&l));
    }
    ms(t0)
}

/// Bilder im EG-Grundriss, bis der Renderer alle Kacheln zeigt, wie die
/// App sie zeigt (`App::platte_unterlage`): höchste Zeit je Bild im
/// UI-Faden (ms), höchstens je Bild und insgesamt hochgeladen (MB), Zahl
/// neuer Kacheln.
fn bis_hochgeladen(pl: &mut Platte, m: &Model, t: &DrawTable) -> (f64, f64, f64, usize) {
    let (mut zeit, mut je_bild, mut gesamt, mut neu) = (0.0f64, 0.0f64, 0.0, 0);
    loop {
        let u = Uhr::an();
        let h = pl.unterlage_im_bild(m, true, t, 1.0);
        zeit = zeit.max(u.ms());
        match h {
            Some(h) => {
                let b = h
                    .laden
                    .iter()
                    .map(|(_, e)| std::mem::size_of_val(&e[..]))
                    .sum::<usize>() as f64
                    / 1e6;
                je_bild = je_bild.max(b);
                gesamt += b;
                neu += h.laden.len();
            }
            None if pl.ecken_job.is_none() && pl.wartend.is_none() => break,
            None => std::thread::sleep(Duration::from_millis(1)),
        }
    }
    (zeit, je_bild, gesamt, neu)
}

/// Lageplan mit `k` Strichen; `dicht`: alle zusätzlichen Striche in 20 ×
/// 20 m um das Haus `run`, dazu ein langer Strich quer durch das Haus, der
/// in einer Kachel fern vom Haus liegt.
fn lageplan_im_modell(
    m: &mut Model,
    k: usize,
    dicht: bool,
) -> (sk_model::SitePlan, sk_model::RunId, [f64; 2]) {
    let mut plan = lageplan(if dicht { 0 } else { k });
    m.begin("Lageplan");
    assert!(m.import_parcel(&plan, "lageplan.pdf", None));
    m.commit();
    // ein Haus über dem Plan (Review #139 N1)
    let im_modell = m.underlay_lines(&plan);
    let (mut lo, mut hi) = ([f64::MAX; 2], [f64::MIN; 2]);
    for q in im_modell.iter().flat_map(|l| &l.0) {
        lo = [lo[0].min(q[0]), lo[1].min(q[1])];
        hi = [hi[0].max(q[0]), hi[1].max(q[1])];
    }
    let (run, mitte) = m
        .runs()
        .iter()
        .find_map(|(id, r)| {
            let innen = r
                .points
                .iter()
                .all(|p| (lo[0]..hi[0]).contains(&p.x) && (lo[1]..hi[1]).contains(&p.y));
            let n = r.points.len() as f64;
            let c = r
                .points
                .iter()
                .fold([0.0, 0.0], |c, p| [c[0] + p.x / n, c[1] + p.y / n]);
            innen.then_some((id, c))
        })
        .expect("ein Haus über dem Plan");
    if dicht {
        // Plan → Modell ist affin: aus drei Punkten umkehren
        let p: Vec<[f64; 2]> = plan.lines.iter().flat_map(|l| l.0.clone()).collect();
        let q: Vec<[f64; 2]> = im_modell.iter().flat_map(|l| l.0.clone()).collect();
        let j = (2..p.len())
            .max_by(|&a, &b| {
                let f = |i: usize| {
                    ((p[1][0] - p[0][0]) * (p[i][1] - p[0][1])
                        - (p[1][1] - p[0][1]) * (p[i][0] - p[0][0]))
                        .abs()
                };
                f(a).total_cmp(&f(b))
            })
            .unwrap();
        let d = |v: &[[f64; 2]], i: usize| [v[i][0] - v[0][0], v[i][1] - v[0][1]];
        let (u1, u2, w1, w2) = (d(&p, 1), d(&p, j), d(&q, 1), d(&q, j));
        // Modell-Abstand → Plan-Abstand: [u1 u2]·[w1 w2]⁻¹
        let det = w1[0] * w2[1] - w2[0] * w1[1];
        let zu_plan = |x: [f64; 2]| {
            let (a, b) = (
                (x[0] * w2[1] - w2[0] * x[1]) / det,
                (w1[0] * x[1] - x[0] * w1[1]) / det,
            );
            [
                p[0][0] + a * u1[0] + b * u2[0],
                p[0][1] + a * u1[1] + b * u2[1],
            ]
        };
        let r = |dx: f64, dy: f64| [mitte[0] - q[0][0] + dx, mitte[1] - q[0][1] + dy];
        let n = ((k - plan.lines.len()) as f64).sqrt().ceil() as usize;
        let s = 20_000.0 / n as f64;
        let mut i = 0;
        while plan.lines.len() < k {
            let (x, y) = ((i % n) as f64 * s - 10_000.0, (i / n) as f64 * s - 10_000.0);
            let a = zu_plan(r(x, y));
            let b = zu_plan(r(x + 0.7 * s, y + 0.3 * s));
            plan.lines
                .push((vec![a, b], false, sk_model::UnderlayKind::Other));
            i += 1;
        }
        // lang: von der Hausmitte 202,5 m weit bis in die Lücke zwischen
        // zwei Häusern, die Mitte fern vom Haus
        let (a, b) = (zu_plan(r(0.0, 0.0)), zu_plan(r(LANG, 0.0)));
        plan.lines
            .push((vec![a, b], false, sk_model::UnderlayKind::Boundary));
    }
    (plan, run, [mitte[0] + LANG, mitte[1]])
}

/// Länge des langen Strichs im dichten Lageplan (mm).
const LANG: f64 = 202_500.0;

fn lageplan_messen(n: usize, k: usize, dicht: bool) -> Lageplan {
    let mut m = viele(n);
    m.begin("ohne Aufmaß");
    assert!(m.set_terrain(None));
    m.commit();
    let (plan, run, ende) = lageplan_im_modell(&mut m, k, dicht);
    let mut s = Scene::with_model(m);
    let mut pl = Platte::default();
    pl.unterlage.merken(s.model(), Arc::new(plan));
    let m = s.model();
    let t = DrawTable::resolve(m, &Theme::dark());
    pl.im_bild(m);
    let n0 = bis_hochgeladen(&mut pl, m, &t).3;
    let keys = pl.im_renderer.clone();
    assert_eq!(n0, keys.len());
    let g = pl.gelaende.clone().unwrap();
    let alle: Vec<usize> = (0..keys.len()).collect();
    let bauen = kacheln_bauen(&g, &alle, &t);
    let linien = gelaende_netz::unterlage_linien(&g, ViewKind::Plan, &t, 1.0);
    assert!(linien.len() >= k / 2, "{} Striche", linien.len());
    // je Bild: dieselben Schlüssel, kein Hochladen; die Grenzen ändern
    // sich wie beim Ziehen einer Wand
    let (b_lo, b_hi) = s.bounds().unwrap();
    let mut d = 0.0;
    let bild = bestwert(|| {
        d = if d == 0.0 { 100.0 } else { 0.0 };
        let b = Some((b_lo, vec3(b_hi.x + d, b_hi.y, b_hi.z)));
        assert!(pl.unterlage_im_bild(m, true, &t, 1.0).is_none());
        assert_eq!(pl.im_renderer, keys);
        std::hint::black_box(pl.linien_im_bild(m, (ViewKind::Plan, None), b, &t, (1.0, 10.0)));
    });
    // EG-Grundriss → 3D → EG-Grundriss: ausgeblendet bleiben die Puffer
    // stehen, zurück lädt nichts hoch (Review #139 N2)
    let rueckweg = bestwert(|| {
        assert!(pl.unterlage_im_bild(m, false, &t, 1.0).is_none());
        assert!(pl.unterlage_im_bild(m, true, &t, 1.0).is_none());
        assert!(pl.ecken_job.is_none());
    });
    // nach jedem Schritt: Modell kopieren und den Faden starten; die Linien
    // der Unterlage sind gemerkt, der Faden bildet sie nicht neu ab. Danach
    // sind nur die Kacheln neu, deren Strecken das Haus schneidet; ihre
    // Ecken baut ein Faden, der UI-Faden lädt nur hoch.
    let sicht: Sicht = (ViewKind::Plan, None, [40, 41, 42], false);
    let orig = s.chain(run).unwrap().clone();
    let (mut nachzug, mut hoch, mut neu) = (f64::INFINITY, 0.0f64, 0);
    let mut vorher = keys.clone();
    let mut lang_neu = false;
    let mut je_schritt = Vec::new();
    for i in 0..6 {
        let moved = orig
            .with_segment_moved(i % 4, 300.0 * (1 + i % 2) as f64)
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
        let (z, mb, _, n) = bis_hochgeladen(&mut pl, m, &t);
        let jetzt = pl.im_renderer.clone();
        let g = pl.gelaende.clone().unwrap();
        let ix: Vec<usize> = (0..jetzt.len())
            .filter(|&i| !vorher.contains(&jetzt[i]))
            .collect();
        assert_eq!(ix.len(), n);
        // die Kachel mit dem langen Strich (sein freies Ende)
        let stil = gelaende_netz::unterlage_stil(&t, 1.0);
        lang_neu |= ix.iter().any(|&i| {
            gelaende_netz::unterlage_kachel(&g, i, stil)
                .iter()
                .any(|h| (f64::from(h.b[0]) - ende[0]).hypot(f64::from(h.b[1]) - ende[1]) < 1.0)
        });
        neu = neu.max(n);
        je_schritt.push(z);
        hoch = hoch.max(mb);
        vorher = jetzt;
    }
    // Gelände verschieben: alle Schlüssel neu, die Ecken aller Kacheln
    // baut der Faden (Review #139 N2)
    assert!(s.edit_model("Gelände verschieben", |m| m.move_terrain([1000.0, 0.0])));
    let m = s.model();
    pl.nachziehen(m, sicht);
    while pl.rechnet() {
        pl.nachziehen(m, sicht);
        std::thread::sleep(Duration::from_millis(1));
    }
    let (lage, lage_hoch, lage_gesamt, n) = bis_hochgeladen(&mut pl, m, &t);
    assert_eq!(n, pl.im_renderer.len(), "alle Kacheln neu");
    assert!(pl.im_renderer.iter().all(|k| !vorher.contains(k)));
    je_schritt.sort_by(f64::total_cmp);
    let schritt_mitte = je_schritt[je_schritt.len() / 2];
    let schritt = je_schritt[je_schritt.len() - 2];
    let schritt_max = je_schritt[je_schritt.len() - 1];
    if dicht {
        assert!(lang_neu, "der lange Strich wird am Haus neu geschnitten");
    }
    Lageplan {
        bild,
        bauen,
        nachzug,
        schritt,
        schritt_max,
        hoch,
        schritt_mitte,
        rueckweg,
        lage,
        lage_hoch,
        lage_gesamt,
        kacheln: keys.len(),
        neu,
    }
}

/// Eine neue Platte (Neu, Öffnen) leert im ersten Bild die Kacheln des
/// alten Projekts, auch ohne eigenen Lageplan (Review #139 N3).
#[test]
fn neues_projekt_leert_die_unterlage() {
    let mut m = viele(4);
    m.begin("ohne Aufmaß");
    assert!(m.set_terrain(None));
    m.commit();
    let t = DrawTable::resolve(&m, &Theme::dark());
    let mut pl = Platte::default();
    pl.im_bild(&m);
    let h = pl.unterlage_im_bild(&m, true, &t, 1.0).expect("leeren");
    assert!(h.laden.is_empty());
    assert_eq!(h.keys, Some(Vec::new()));
    assert!(pl.unterlage_im_bild(&m, true, &t, 1.0).is_none());
    // ausgeblendet erst im nächsten EG-Grundriss
    let mut pl = Platte::default();
    assert!(pl.unterlage_im_bild(&m, false, &t, 1.0).is_none());
    assert!(pl.unterlage_im_bild(&m, true, &t, 1.0).is_some());
}

/// Lageplan unter dem Gelände (Review #139): Je Bild und je Mausbewegung
/// wächst nichts mit der Zahl der Striche, der Nachzug nach einem Schritt
/// beginnt in derselben Zeit; nur das einmalige Bauen der Linien für den
/// Renderer wächst mit ihnen, ein Wandschritt baut nur die Kacheln unter
/// dem Haus neu. Bei 1000 Häusern und 50.000 Strichen (im
/// Debug 64 Häuser und 5.000 Striche).
#[test]
fn lageplan_im_budget() {
    let (n, k, f) = if cfg!(debug_assertions) {
        (64, 5_000, 40.0)
    } else {
        (1000, 50_000, 1.0)
    };
    let klein = lageplan_messen(n, 5_000, false);
    let gross = lageplan_messen(n, k, false);
    let dicht = lageplan_messen(n, k, true);
    eprintln!(
        "{n} Häuser, Lageplan 5.000 Striche: {klein:?}\n{k} Striche: {gross:?}\n\
         {k} Striche um das Haus: {dicht:?}"
    );
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
    if k > 5_000 {
        assert!(gross.bauen > klein.bauen, "{klein:?} {gross:?}");
    }
    // ein Wandschritt über dem Plan baut nur die Kacheln, deren Strecken
    // das Haus schneidet (Review #139 N1: vorher alle, 28 ms und 57 MB bei
    // 50.000 Strichen): im ersten Bild danach höchstens 8 ms, unabhängig
    // von der Zahl der Striche, auch im dichten Plan
    for x in [&klein, &gross, &dicht] {
        assert!(x.neu >= 1, "das Haus liegt über dem Plan: {x:?}");
        assert!(x.schritt < 8.0 * f, "{x:?}");
        // ein einzelner Ausreißer, aber kein Bild, das viermal so lange
        // arbeitet
        assert!(x.schritt_max < 32.0 * f, "{x:?}");
        assert!(x.hoch < 8.0, "höchstens 8 MB hochladen: {x:?}");
        // Sichtwechsel und Gelände verschieben: der UI-Faden baut nichts
        // (Review #139 N2: vorher 28 ms und 57 MB je Wechsel)
        assert!(x.rueckweg < 1.0 * f, "{x:?}");
        assert!(x.lage < 8.0 * f, "{x:?}");
        assert!(
            x.lage_gesamt >= x.hoch,
            "verschoben sind alle Kacheln neu: {x:?}"
        );
        // je Bild höchstens 4 MB und eine Kachel (höchstens 1.024 Striche)
        let kachel = (1024 * 6 * 23 * 4) as f64 / 1e6;
        assert!(x.lage_hoch <= 4.2 + kachel, "{x:?}");
    }
    assert!(
        gross.rueckweg < 2.0 * klein.rueckweg + 0.2 * f,
        "{klein:?} {gross:?}"
    );
    assert!(gross.neu * 4 <= gross.kacheln, "{gross:?}");
    // wächst nicht mit der Zahl der Striche: im Median der Schritte, die
    // höchste Zeit hält oben die 8 ms
    assert!(
        gross.schritt_mitte < 2.0 * klein.schritt_mitte + 1.0 * f,
        "{klein:?} {gross:?}"
    );
}

/// Messprogramm (H-196h): 100 Züge auf 326.000 Punkten im Abstand von
/// 30 cm; die Leiste vor dem ersten Ergebnis des Fadens (rechnet Sockel
/// und Einbindung selbst) und danach. Nur im Release sinnvoll:
/// `cargo test --release -p skizzeo folgen_im_faden_messen -- --ignored --nocapture`
#[test]
#[ignore = "Messprogramm"]
fn folgen_im_faden_messen() {
    let (k, n) = (100, 571);
    let mut m = Model::with_seed(5);
    let b = m.add_building(1);
    for i in 0..k {
        let (ox, oy) = ((i % 10) as f64 * 15_000.0, (i / 10) as f64 * 15_000.0);
        let p = |x: f64, y: f64| vec3(ox + x, oy + y, 0.0);
        let pts = [p(0.0, 0.0), p(0.0, 8000.0), p(1e4, 8000.0), p(1e4, 0.0)];
        m.build_from_polygon(b, &pts).unwrap();
    }
    let mut t = Terrain::new(Crs::Local);
    for i in 0..n {
        for j in 0..n {
            let (e, nn) = (1000.0 + 0.3 * i as f64, 2000.0 + 0.3 * j as f64);
            t.points.push(TerrainPoint {
                nr: (i * n + j + 1).to_string(),
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
    let mut z = Zustand::default();
    z.offen = true;
    let selbst = bestwert(|| {
        std::hint::black_box(leiste(&m, &z));
    });
    let t0 = Instant::now();
    z.baugruben(&m);
    while z.prueft() {
        std::thread::sleep(Duration::from_millis(1));
        z.baugruben(&m);
    }
    let faden = ms(t0);
    let danach = bestwert(|| {
        std::hint::black_box(leiste(&m, &z));
    });
    eprintln!("{k} Züge, {} Punkte: Leiste selbst {selbst:.2} ms, aus dem Faden {danach:.2} ms, Faden {faden:.1} ms", n * n);
}
