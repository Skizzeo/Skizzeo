//! Ist-Bilder zum Raumstempel (Grundriss G4) ohne Grafikkarte: Papier,
//! darauf die Schnittflächen der Wände im Grundriss, darüber die Stempel
//! und die Vorschlagsliste mit denselben [`malen`] und [`liste_malen`] wie
//! in der App.
//!
//! `SKIZZEO_ISTBILDER=<ordner> cargo test -p skizzeo istbilder_g4 -- --ignored`

use super::*;
use crate::ui::ViewKind;
use sk_model::{szo, Category, GuidGen, RefSide};
use std::path::PathBuf;

const W: f64 = 1600.0;
const H: f64 = 1000.0;
/// Bildschirmskalierung der Bilder.
const S: f64 = 2.0;
const PAPIER: Rgba = Rgba(245, 244, 239, 255);

fn schriften() -> Fonts {
    let lib = std::path::Path::new("/usr/share/fonts/truetype/liberation");
    let lade = |n: &str| {
        std::fs::read(lib.join(n))
            .ok()
            .and_then(sk_paint::font::Font::parse)
    };
    Fonts {
        regular: lade("LiberationSans-Regular.ttf"),
        bold: lade("LiberationSans-Bold.ttf"),
        italic: None,
    }
}

/// Referenzhaus 1 mit Innenwänden: links Wohnen, rechts Küche und Bad,
/// dazwischen ein Schacht. Die Räume entstehen beim Schritt.
fn haus() -> Scene {
    let text = include_str!("../../../crates/sk-cost/referenz/rh1-standardhaus.szo");
    let m = szo::read_with(text, GuidGen::with_seed(1), &sk_cost::lesen::ABSCHNITTE_SZO)
        .unwrap()
        .model;
    let mut s = Scene::with_model(m);
    let (lo, hi) = s.bounds().unwrap();
    let st = s.active_storey();
    let x = lo.x + (hi.x - lo.x) * 0.55;
    let y = lo.y + (hi.y - lo.y) * 0.45;
    let (y0, y1, x1) = (lo.y + 150.0, hi.y - 150.0, hi.x - 150.0);
    assert!(s.edit_model("Innenwände", |m| {
        let iw = m.defaults().interior_wall;
        let mut wand = |a: Vec3, b: Vec3| {
            m.add_wall_run(
                &[a, b],
                false,
                RefSide::Center,
                st,
                iw,
                Category::InteriorWall,
            )
            .is_some()
        };
        wand(vec3(x, y0, 0.0), vec3(x, y1, 0.0))
            && wand(vec3(x, y, 0.0), vec3(x1, y, 0.0))
            // Schacht 0,8 × 0,8 m an der Trennwand
            && wand(vec3(x, y + 900.0, 0.0), vec3(x + 900.0, y + 900.0, 0.0))
            && wand(vec3(x + 900.0, y, 0.0), vec3(x + 900.0, y + 900.0, 0.0))
    }));
    s
}

/// Schnittflächen der Wände im Grundriss, dunkel auf Papier.
fn plan(s: &mut Scene, cam: &Camera) -> Canvas {
    let mut c = Canvas::new(W as usize, H as usize);
    c.clear(PAPIER);
    let mesh = s.mesh(ViewKind::Plan, None, &[]);
    let z = s.plan_cut();
    for t in mesh.faces.chunks(3) {
        let p = |i: usize| vec3(t[i][0] as f64, t[i][1] as f64, t[i][2] as f64);
        let mitte = (p(0) + p(1) + p(2)) * (1.0 / 3.0);
        if t[0][5] < 0.5 || mitte.z < z - 1.0 {
            continue;
        }
        let q: Option<Vec<(f64, f64)>> = (0..3).map(|i| cam.project(p(i), W, H)).collect();
        let Some(q) = q else {
            continue;
        };
        let mut pfad = Path::new();
        pfad.move_to(q[0].0 as f32, q[0].1 as f32)
            .line_to(q[1].0 as f32, q[1].1 as f32)
            .line_to(q[2].0 as f32, q[2].1 as f32)
            .close();
        c.fill(&pfad, Rgba(82, 82, 80, 255));
    }
    c
}

/// Stempel und Liste auf das Bild, wie die App sie hochlädt.
fn mit_stempeln(c: &mut Canvas, st: &Stempel, gewaehlt: Option<ElementId>, f: &Fonts) {
    let t = Theme::dark();
    for i in 0..st.bilder.len() {
        let (k, [x, y]) = st.bild(i, gewaehlt, S, 0).unwrap();
        c.blit(&malen(&k, f, &t), x.round() as i32, y.round() as i32);
    }
    if let Some((k, [x, y])) = st.liste_bild(S, 0) {
        c.blit(&liste_malen(&k, f, &t), x.round() as i32, y.round() as i32);
    }
}

#[test]
#[ignore = "legt Ist-Bilder ab, nur mit SKIZZEO_ISTBILDER"]
fn istbilder_g4() {
    let Some(ziel) = std::env::var_os("SKIZZEO_ISTBILDER").map(PathBuf::from) else {
        return;
    };
    let ab = |c: &Canvas, n: &str| std::fs::write(ziel.join(n), c.to_png()).unwrap();
    let (f, t) = (schriften(), Theme::dark());
    let mut s = haus();
    let cam = crate::fit_parallel(ViewKind::Plan, s.bounds(), W, H);
    let mut st = Stempel::default();
    let lege = |st: &mut Stempel, s: &Scene| st.auslegen(s, &cam, W, H, S, &f, &t, &[]);

    // a) Nach dem Schritt: der größte neue Raum hat das Namensfeld offen,
    // „K“ getippt, die Liste gefiltert
    let neu = s.neuer_raum().expect("neuer Raum");
    st.oeffnen(&s, neu);
    st.text('K');
    lege(&mut st, &s);
    let mut c = plan(&mut s, &cam);
    mit_stempeln(&mut c, &st, None, &f);
    ab(&c, "ist-g4-namensfeld.png");

    // b) Benannt, einer gewählt, einer unter der Maus
    st.key(Key::Escape, Modifiers::default(), &mut s);
    let st_id = s.active_storey();
    let namen = ["Wohnen", "Küche", "Bad", "Schacht"];
    let mut raeume = s.model().spaces(st_id);
    raeume.sort_by_key(|id| {
        let a = sk_raum::raeume(s.model(), st_id)
            .into_iter()
            .find(|f| f.raum == Some(*id))
            .map_or(0.0, |f| f.flaeche);
        -(a as i64)
    });
    for (id, n) in raeume.iter().zip(namen) {
        assert!(s.raum_benennen(*id, n));
    }
    lege(&mut st, &s);
    st.hover = st.bilder.iter().position(|b| b.raum == Some(raeume[2]));
    let mut c = plan(&mut s, &cam);
    mit_stempeln(&mut c, &st, Some(raeume[1]), &f);
    ab(&c, "ist-g4-stempel.png");

    // c) Weit herausgezoomt: kleine Räume nur mit Fläche oder ohne Stempel
    let (lo, hi) = s.bounds().unwrap();
    let mitte = (lo + hi) * 0.5;
    let weit = Camera::parallel(
        vec3(mitte.x, mitte.y, 0.0),
        std::f64::consts::FRAC_PI_2,
        -std::f64::consts::FRAC_PI_2,
        (hi.y - lo.y) * 2.2,
    );
    st.hover = None;
    st.auslegen(&s, &weit, W, H, S, &f, &t, &[]);
    let mut c = plan(&mut s, &weit);
    mit_stempeln(&mut c, &st, None, &f);
    ab(&c, "ist-g4-weit.png");
}

/// Kleines Bad, lichte Maße 1,80 × 1,40 m, in der Ecke eines Hauses
/// 10 × 8 m. Die Tür, 88,5 cm, sitzt mitten in der langen Wand und schlägt
/// ins Bad auf; ihr Bogen überdeckt die Mitte des Bads.
fn bad() -> (Scene, ElementId, Vec3) {
    use sk_model::oeffnung::{Din, FaceSide, HeightLine, HeightRef, Opening, OpeningKind, Swing};
    let mut m = sk_model::Model::with_seed(7);
    let st = m.defaults().storey;
    let (aw, iw) = (m.defaults().exterior_wall, m.defaults().interior_wall);
    let t: f64 = m.wall_layers(aw).iter().map(|l| l.thickness).sum();
    let d: f64 = m.wall_layers(iw).iter().map(|l| l.thickness).sum();
    let pts = [
        vec3(0.0, 0.0, 0.0),
        vec3(0.0, 8000.0, 0.0),
        vec3(10000.0, 8000.0, 0.0),
        vec3(10000.0, 0.0, 0.0),
    ];
    m.add_wall_run(&pts, true, RefSide::Left, st, aw, Category::ExteriorWall)
        .unwrap();
    let (xc, yc) = (t + 1800.0 + d / 2.0, t + 1400.0 + d / 2.0);
    let ecke = [vec3(t, yc, 0.0), vec3(xc, yc, 0.0), vec3(xc, t, 0.0)];
    let run = m
        .add_wall_run(
            &ecke,
            false,
            RefSide::Center,
            st,
            iw,
            Category::InteriorWall,
        )
        .unwrap();
    let f = m.outer_face(run, 0).unwrap();
    let u = f.u_of(vec3(t + 900.0, yc, 0.0));
    let o = Opening {
        kind: OpeningKind::Door,
        path: vec![f.at(u - 442.5, 0.0), f.at(u + 442.5, 0.0)],
        bottom: HeightRef::Level(sk_model::LevelRef::bottom(st)),
        top: HeightRef::Line(st, HeightLine::Door),
        reveal: None,
        install: Default::default(),
        swing: Some(Swing {
            side: FaceSide::Right,
            din: Din::Links,
        }),
        lock: Default::default(),
    };
    let id = m.add_opening(o).unwrap();
    let mut s = Scene::with_model(m);
    let mitte = vec3(t + 900.0, t + 700.0, 0.0);
    let bad = s.raum_anlegen(st, mitte);
    let flur = s.raum_anlegen(st, vec3(6000.0, 5000.0, 0.0));
    assert!(s.raum_benennen(bad, "Bad") && s.raum_benennen(flur, "Wohnen"));
    (s, id, mitte)
}

/// Grundriss wie [`plan`], dazu die feinen Linien in Schnitthöhe (Aufschläge).
fn plan_mit_boegen(s: &mut Scene, cam: &Camera, w: f64, h: f64, px: f32) -> Canvas {
    let mut c = Canvas::new(w as usize, h as usize);
    c.clear(PAPIER);
    let mesh = s.mesh(ViewKind::Plan, None, &[]);
    let z = s.plan_cut();
    let p = |q: &[f32]| vec3(q[0] as f64, q[1] as f64, q[2] as f64);
    for t in mesh.faces.chunks(3) {
        let mitte = (p(&t[0]) + p(&t[1]) + p(&t[2])) * (1.0 / 3.0);
        if t[0][5] < 0.5 || mitte.z < z - 1.0 {
            continue;
        }
        let q: Option<Vec<(f64, f64)>> = (0..3).map(|i| cam.project(p(&t[i]), w, h)).collect();
        let Some(q) = q else {
            continue;
        };
        let mut pfad = Path::new();
        pfad.move_to(q[0].0 as f32, q[0].1 as f32)
            .line_to(q[1].0 as f32, q[1].1 as f32)
            .line_to(q[2].0 as f32, q[2].1 as f32)
            .close();
        c.fill(&pfad, Rgba(82, 82, 80, 255));
        // Die Kanten der Dreiecke dazu, sonst schimmern die Fugen hell
        for i in 0..3 {
            let (a, b) = (q[i], q[(i + 1) % 3]);
            let mut fuge = Path::new();
            fuge.segment((a.0 as f32, a.1 as f32), (b.0 as f32, b.1 as f32), 0.8);
            c.fill(&fuge, Rgba(82, 82, 80, 255));
        }
    }
    let fein = sk_model::edge_kind::FINE as f32;
    for (e, k) in &mesh.edges {
        let (a, b) = (p(&e[0]), p(&e[1]));
        if *k != fein || (a.z - z).abs() > 1.0 {
            continue;
        }
        let (Some(a), Some(b)) = (cam.project(a, w, h), cam.project(b, w, h)) else {
            continue;
        };
        let mut pfad = Path::new();
        pfad.segment((a.0 as f32, a.1 as f32), (b.0 as f32, b.1 as f32), px);
        c.fill(&pfad, Rgba(40, 40, 38, 255));
    }
    c
}

/// Ist-Bilder G7a für Einstellungen: kleines Bad, dessen Türbogen die Mitte
/// überdeckt, bei 100 % und 150 % Bildschirmskalierung, und die beiden
/// Rückfälle beim Herauszoomen: der ganze Stempel an der Mitte über dem
/// Bogen, dann nur die Fläche.
///
/// `SKIZZEO_ISTBILDER=<ordner> cargo test -p skizzeo istbilder_g7 -- --ignored --nocapture`
#[test]
#[ignore = "legt Ist-Bilder ab, nur mit SKIZZEO_ISTBILDER"]
fn istbilder_g7() {
    let Some(ziel) = std::env::var_os("SKIZZEO_ISTBILDER").map(PathBuf::from) else {
        return;
    };
    let (f, t) = (schriften(), Theme::dark());
    let (mut s, tuer, mitte) = bad();
    let l = s.model().door_leaf(tuer).unwrap();
    assert!(l.swing_into.y < -0.99, "ins Bad: {l:?}");
    let k = s.model().chain(l.run).unwrap();
    let (_, sw) = k
        .door_swings()
        .into_iter()
        .find(|(h, _)| h.opening == Some(tuer))
        .unwrap();
    let a = super::aufschlag::Aufschlag::aus(sw).unwrap();
    assert_eq!(a.abstand(mitte), 0.0, "der Bogen überdeckt die Mitte");
    // 800 × 600 Bildpunkte bei 100 %, Ausschnitt um das Bad
    let (w0, h0) = (800.0, 600.0);
    let blick = vec3(mitte.x + 600.0, mitte.y + 300.0, 0.0);
    let cam = |halb: f64| {
        Camera::parallel(
            blick,
            std::f64::consts::FRAC_PI_2,
            -std::f64::consts::FRAC_PI_2,
            halb,
        )
    };
    let lage = |s: &Scene, halb: f64, sk: f64| {
        let mut st = Stempel::default();
        let (w, h) = (w0 * sk, h0 * sk);
        st.auslegen(s, &cam(halb), w, h, sk, &f, &t, &[]);
        let b = st
            .bilder
            .iter()
            .find(|b| (b.punkt - mitte).length() < 1500.0 && b.punkt.x < mitte.x + 1000.0)
            .cloned();
        (st, b)
    };
    let art = |b: &Option<Bild>| match b {
        None => "ohne Stempel",
        Some(b) if b.kurz => "nur Fläche",
        Some(b) if a.abstand(b.punkt) == 0.0 => "Mitte über dem Bogen",
        Some(_) => "ausgewichen",
    };
    let ab = |s: &mut Scene, halb: f64, sk: f64, name: &str| {
        let (st, b) = lage(s, halb, sk);
        let mut c = plan_mit_boegen(s, &cam(halb), w0 * sk, h0 * sk, sk as f32);
        let t = Theme::dark();
        for i in 0..st.bilder.len() {
            let (k, [x, y]) = st.bild(i, None, sk, 0).unwrap();
            c.blit(&malen(&k, &f, &t), x.round() as i32, y.round() as i32);
        }
        std::fs::write(ziel.join(name), c.to_png()).unwrap();
        let b = b
            .as_ref()
            .map(|b| (b.name.clone(), b.flaeche.clone(), b.punkt));
        println!(
            "{name}: halbe Höhe {halb} mm, {} {b:?}",
            art(&lage(s, halb, sk).1)
        );
    };
    // Vorgabe: 3 m hoch im Bild, der Stempel weicht aus
    ab(&mut s, 1500.0, 1.0, "ist-g7a-bad-100.png");
    ab(&mut s, 1500.0, 1.5, "ist-g7a-bad-150.png");
    // Herauszoomen bis zu den Rückfällen
    let mut halb = 1500.0;
    let mut gesehen = vec![art(&lage(&s, halb, 1.0).1)];
    while halb < 40000.0 {
        halb *= 1.1;
        let jetzt = art(&lage(&s, halb, 1.0).1);
        if !gesehen.contains(&jetzt) {
            gesehen.push(jetzt);
            let name = match jetzt {
                "Mitte über dem Bogen" => "ist-g7a-bad-mitte.png",
                "nur Fläche" => "ist-g7a-bad-flaeche.png",
                _ => "ist-g7a-bad-ohne.png",
            };
            ab(&mut s, halb, 1.0, name);
        }
    }
    println!("Folge: {gesehen:?}");
}
