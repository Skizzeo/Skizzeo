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
