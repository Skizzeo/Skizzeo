//! Ist-Bilder der Bemaßung auf der GPU (Mesa unter Xvfb, wie
//! [`crate::gpu_probe`]):
//!
//! `xvfb-run -a env SKIZZEO_ISTBILDER=<ordner> cargo test -p skizzeo gpu_istbilder_bemassung -- --ignored`

use super::Massketten;
use crate::camera::Camera;
use crate::scene::Scene;
use crate::ui::ViewKind;
use sk_math::vec3;
use sk_model::{szo, GuidGen};
use sk_render::Renderer;
use sk_ui::theme::Theme;

const W: u32 = 1600;
const H: u32 = 1000;

fn haus(datei: &str) -> Scene {
    let text = std::fs::read_to_string(datei).unwrap();
    let m = szo::read_with(
        &text,
        GuidGen::with_seed(1),
        &sk_cost::lesen::ABSCHNITTE_SZO,
    )
    .unwrap()
    .model;
    Scene::with_model(m)
}

fn schrift() -> Option<sk_paint::font::Font> {
    std::fs::read("/usr/share/fonts/truetype/liberation/LiberationSans-Regular.ttf")
        .ok()
        .and_then(sk_paint::font::Font::parse)
}

fn bild(r: &mut Renderer, s: &mut Scene, v: ViewKind, cam: &Camera, bezug: bool) -> Vec<u8> {
    let theme = Theme::dark();
    s.set_theme(&theme);
    r.set_style(crate::style(&theme.env));
    r.set_looks(&s.table().looks_with(1.0, |_| 1.0));
    r.set_mesh(crate::MESH_MODEL, &s.mesh(v, None, &[]));
    r.set_sun(None);
    let mut mk = Massketten::default();
    s.set_mass_sicht(v, true, bezug);
    let f = schrift();
    let blick = super::Blick {
        cam,
        w: f64::from(W),
        h: f64::from(H),
    };
    let (h, t) = mk.zeichnen(s, v, &[], f.as_ref(), &blick, 1.0, &theme);
    r.set_helpers(&h);
    r.set_ink_teile(&t);
    let mut view = cam.view(W, H);
    if v != ViewKind::Persp {
        view.paper = Some(s.table().paper);
    }
    r.draw(W, H, 0, &view).unwrap();
    r.read_pixels(W, H)
}

#[test]
#[ignore = "braucht einen X-Server (xvfb-run) und SKIZZEO_ISTBILDER"]
fn gpu_istbilder_bemassung() {
    let Some(ziel) = std::env::var_os("SKIZZEO_ISTBILDER").map(std::path::PathBuf::from) else {
        return;
    };
    let k = sk_render::glx::kontext(W as i32, H as i32).expect("GLX-Kontext (DISPLAY?)");
    let theme = Theme::dark();
    let mut r = Renderer::new(k.gl, crate::style(&theme.env)).unwrap();
    let ab =
        |px: &[u8], n: &str| std::fs::write(ziel.join(n), sk_paint::encode_png(W, H, px)).unwrap();
    let referenz = concat!(env!("CARGO_MANIFEST_DIR"), "/../crates/sk-cost/referenz/");
    for (datei, name) in [
        ("rh1-standardhaus.szo", "rh1"),
        ("rh3-versatz-dachterrasse.szo", "rh3"),
        ("r7-alle-abschnitte.szo", "r7"),
    ] {
        let mut s = haus(&format!("{referenz}{datei}"));
        // Im zweiten Gebäude von R7 stehen die Fenster und die Tür: dessen
        // EG aktiv (dieselbe Ebene wie das EG des ersten)
        let eg2 = s
            .model()
            .storeys()
            .iter()
            .filter(|(_, st)| st.kind == sk_model::LevelKind::Storey && st.elevation == 0.0)
            .map(|(id, _)| id)
            .last();
        if let Some(st) = eg2 {
            s.set_active_storey(st);
        }
        let (lo, hi) = s.bounds().unwrap();
        let rand = vec3(6000.0, 6000.0, 0.0);
        let cam = crate::fit_parallel(
            ViewKind::Plan,
            Some((lo - rand, hi + rand)),
            W as f64,
            H as f64,
        );
        ab(
            &bild(&mut r, &mut s, ViewKind::Plan, &cam, false),
            &format!("ist-bem-{name}-grundriss.png"),
        );
        ab(
            &bild(&mut r, &mut s, ViewKind::Plan, &cam, true),
            &format!("ist-bem-{name}-grundriss-bezug.png"),
        );
        let ziel3 = (lo + hi) * 0.5;
        let cam3 = Camera::looking_at(
            ziel3 + vec3(0.55, -1.0, 0.9).normalized() * 30000.0,
            ziel3,
            45.0,
        );
        ab(
            &bild(&mut r, &mut s, ViewKind::Persp, &cam3, false),
            &format!("ist-bem-{name}-3d.png"),
        );
    }
}
