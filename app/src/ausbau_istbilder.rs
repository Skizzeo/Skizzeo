//! Ist-Bilder zum Ausbau innen (Raumbuch RB4, Bedienbarkeit 42.7) auf der
//! GPU (Mesa unter Xvfb, wie [`crate::gpu_probe`]): Referenzhaus 1 mit
//! Wohnen, Küche und Bad im Grundriss ganz und am Bad, mit und ohne Ausbau,
//! und im Schnitt durch Bad und Küche. Netz und Teile des Ausbaus kommen in
//! dieselben Ablagen wie in der App ([`crate::App::netz_hochladen`]).
//!
//! `xvfb-run -a env SKIZZEO_ISTBILDER=<ordner> cargo test -p skizzeo gpu_istbilder_ausbau -- --ignored`

use crate::ausbau_app::MESH_AUSBAU;
use crate::scene::{scene_ausbau, Plane, Scene};
use crate::ui::ViewKind;
use sk_math::{vec3, Vec3};
use sk_model::{szo, Category, GuidGen, RefSide};
use sk_render::Renderer;
use sk_ui::theme::Theme;

const W: u32 = 1600;
const H: u32 = 1000;

/// Referenzhaus 1 mit Innenwänden: links Wohnen, rechts oben Küche, rechts
/// unten Bad, je mit ihrer Raumvorlage.
fn haus() -> Scene {
    let text = include_str!("../../crates/sk-cost/referenz/rh1-standardhaus.szo");
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
            let art = Category::InteriorWall;
            m.add_wall_run(&[a, b], false, RefSide::Center, st, iw, art)
                .is_some()
        };
        wand(vec3(x, y0, 0.0), vec3(x, y1, 0.0)) && wand(vec3(x, y, 0.0), vec3(x1, y, 0.0))
    }));
    let raeume = s.model().spaces(st);
    assert_eq!(raeume.len(), 3, "Wohnen, Küche, Bad");
    for r in raeume {
        let p = s.model().space(r).unwrap().anker;
        let name = match (p.x < x, p.y < y) {
            (true, _) => "Wohnen",
            (false, true) => "Bad",
            (false, false) => "Küche",
        };
        assert!(s.raum_benennen(r, name, None));
    }
    s
}

/// Bild der Ansicht `v` mit Ausbau innen `an`, Netz und Teile wie in der
/// App; im Schnitt die Ebene `ebene`.
fn bild(
    r: &mut Renderer,
    s: &mut Scene,
    v: ViewKind,
    ebene: Option<Plane>,
    an: bool,
    cam: &crate::camera::Camera,
) -> Vec<u8> {
    let theme = Theme::dark();
    s.set_theme(&theme);
    s.set_view_finish(scene_ausbau::platz(v).unwrap(), an);
    r.set_style(crate::style(&theme.env));
    r.set_looks(&s.table().looks_with(1.0, |_| 1.0));
    s.ausbau_getrennt();
    r.set_mesh(crate::MESH_MODEL, &s.mesh(v, ebene, &[]));
    s.ausbau_liefern(|j, m| r.set_mesh(MESH_AUSBAU + j, m));
    r.set_sun(None);
    let mut view = cam.view(W, H);
    view.paper = Some(s.table().paper);
    view.patterns = crate::draw_table::pattern_mode(v, theme.env.patterns_3d);
    r.draw(W, H, 0, &view).unwrap();
    r.read_pixels(W, H)
}

/// Anteil der Bildpunkte, die sich zwischen `a` und `b` unterscheiden.
fn anders(a: &[u8], b: &[u8]) -> f64 {
    let n = a.chunks(4).zip(b.chunks(4)).filter(|(p, q)| p != q).count();
    n as f64 / (a.len() / 4) as f64
}

#[test]
#[ignore = "braucht einen X-Server (xvfb-run) und SKIZZEO_ISTBILDER"]
fn gpu_istbilder_ausbau() {
    let Some(ziel) = std::env::var_os("SKIZZEO_ISTBILDER").map(std::path::PathBuf::from) else {
        return;
    };
    std::fs::create_dir_all(&ziel).unwrap();
    let ab = |px: &[u8], n: &str| {
        std::fs::write(ziel.join(n), sk_paint::encode_png_gepackt(W, H, px)).unwrap();
    };
    let k = sk_render::glx::kontext(W as i32, H as i32).expect("GLX-Kontext (DISPLAY?)");
    let theme = Theme::dark();
    let mut r = Renderer::new(k.gl, crate::style(&theme.env)).unwrap();
    let mut s = haus();
    let (lo, hi) = s.bounds().unwrap();
    // Grundriss ganz, mit und ohne Ausbau
    let plan = ViewKind::Plan;
    let cam = crate::fit_parallel(plan, Some((lo, hi)), W as f64, H as f64);
    let aus = bild(&mut r, &mut s, plan, None, false, &cam);
    let an = bild(&mut r, &mut s, plan, None, true, &cam);
    assert!(anders(&aus, &an) > 0.001, "der Ausbau ändert das Bild");
    ab(&an, "ist-rb4-ausbau-grundriss.png");
    ab(&aus, "ist-rb4-ausbau-grundriss-aus.png");
    // Nah an der Ecke von Wohnen und Bad an der Außenwand: Putz, im Bad
    // Fliesen auf Putz
    let x = lo.x + (hi.x - lo.x) * 0.55;
    let y = lo.y + (hi.y - lo.y) * 0.45;
    let ecke = (
        vec3(x - 900.0, lo.y - 250.0, lo.z),
        vec3(x + 1100.0, lo.y + 1000.0, hi.z),
    );
    let cam = crate::fit_parallel(plan, Some(ecke), W as f64, H as f64);
    ab(
        &bild(&mut r, &mut s, plan, None, true, &cam),
        "ist-rb4-ausbau-grundriss-ecke.png",
    );
    ab(
        &bild(&mut r, &mut s, plan, None, false, &cam),
        "ist-rb4-ausbau-grundriss-ecke-aus.png",
    );
    // Schnitt quer durch Bad und Wohnen, Blick nach Norden: der Estrich
    // immer, Putz und Belag nur mit Schalter
    let ebene: Plane = (vec3(0.0, (lo.y + y) * 0.5, 0.0), vec3(0.0, -1.0, 0.0));
    let schnitt = ViewKind::Section;
    let cam = crate::fit_parallel_dir(
        (std::f64::consts::FRAC_PI_2, 0.0),
        Some((lo, hi)),
        W as f64,
        W as f64,
        H as f64,
        W as f64 * 0.5,
    );
    let aus = bild(&mut r, &mut s, schnitt, Some(ebene), false, &cam);
    let an = bild(&mut r, &mut s, schnitt, Some(ebene), true, &cam);
    assert!(anders(&aus, &an) > 0.0005, "der Ausbau ändert den Schnitt");
    ab(&an, "ist-rb4-ausbau-schnitt.png");
    ab(&aus, "ist-rb4-ausbau-schnitt-aus.png");
    // Nah an der Innenwand über dem Boden: Bodenaufbau, Sockelleiste im
    // Wohnen, Fliesen im Bad
    let z = s.model().storey(s.active_storey()).unwrap().elevation;
    let fuss = (
        vec3(x - 1000.0, ebene.0.y - 1.0, z - 350.0),
        vec3(x + 1000.0, ebene.0.y + 1.0, z + 1400.0),
    );
    let cam = crate::fit_parallel_dir(
        (std::f64::consts::FRAC_PI_2, 0.0),
        Some(fuss),
        W as f64,
        W as f64,
        H as f64,
        W as f64 * 0.5,
    );
    let an = bild(&mut r, &mut s, schnitt, Some(ebene), true, &cam);
    ab(&an, "ist-rb4-ausbau-schnitt-fuss.png");
    let aus = bild(&mut r, &mut s, schnitt, Some(ebene), false, &cam);
    ab(&aus, "ist-rb4-ausbau-schnitt-fuss-aus.png");
}
