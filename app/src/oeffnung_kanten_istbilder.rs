//! Ist-Bilder „Öffnung von außen ändern“ auf der GPU (Mesa unter Xvfb, wie
//! [`crate::gpu_probe`]):
//!
//! `xvfb-run -a env SKIZZEO_ISTBILDER=<ordner> cargo test -p skizzeo gpu_istbilder_kanten -- --ignored`

use super::tests::{bei, haus, H, W};
use super::*;
use crate::ui::ViewKind;
use sk_render::Renderer;

fn bild(r: &mut Renderer, s: &mut Scene, cam: &Camera, h: &[Helper]) -> Vec<u8> {
    let theme = Theme::dark();
    s.set_theme(&theme);
    r.set_style(crate::style(&theme.env));
    r.set_looks(&s.table().looks_with(1.0, |_| 1.0));
    r.set_mesh(crate::MESH_MODEL, &s.mesh(ViewKind::Persp, None, &[]));
    r.set_sun(None);
    r.set_helpers(h);
    let (w, hh) = (W as u32, H as u32);
    r.draw(w, hh, 0, &cam.view(w, hh)).unwrap();
    r.read_pixels(w, hh)
}

#[test]
#[ignore = "braucht einen X-Server (xvfb-run) und SKIZZEO_ISTBILDER"]
fn gpu_istbilder_kanten() {
    let Some(ziel) = std::env::var_os("SKIZZEO_ISTBILDER").map(std::path::PathBuf::from) else {
        return;
    };
    let k = sk_render::glx::kontext(W as i32, H as i32).expect("GLX-Kontext (DISPLAY?)");
    let theme = Theme::dark();
    let mut r = Renderer::new(k.gl, crate::style(&theme.env)).unwrap();
    let ab = |px: &[u8], n: &str| {
        let png = sk_paint::encode_png(W as u32, H as u32, px);
        std::fs::write(ziel.join(n), png).unwrap()
    };
    let (mut s, _, id, _, og) = haus();
    let cam = Camera::looking_at(
        vec3(13000.0, -14000.0, 6500.0),
        vec3(7000.0, 0.0, 3500.0),
        45.0,
    );
    let m = s.model();
    let (_, z1) = m.opening_band(m.opening(id).unwrap()).unwrap();
    // 1: Maus an der Oberkante, alle Kanten als Band, die obere heiß
    let p = bei(&s, &cam, id, 7000.0, z1 - 5.0);
    let t = treffer(&s, &cam, p, W, H, 1.0, None).unwrap();
    let h = baender(&s, &cam, t, 1.0, true, &theme);
    ab(&bild(&mut r, &mut s, &cam, &h), "ist-o7-kanten.png");
    // 2: Oberkante bis an den Sturz des OG gezogen, gebunden
    let mut z = HoeheZug::greifen(&mut s, t, true, &cam, p, W, H, 1.0).unwrap();
    let st_og = s.model().run(og).unwrap().storey;
    let z_og = s
        .model()
        .height_z(HeightRef::Line(st_og, HeightLine::Lintel))
        .unwrap();
    let (x, y) = bei(&s, &cam, id, 7000.0, z_og + 20.0);
    let mods = Default::default();
    z.handle(&Event::MouseMove { x, y, mods }, &mut s, &cam, W, H);
    let h = z.helpers(&s, &cam, 1.0, true, &theme);
    ab(&bild(&mut r, &mut s, &cam, &h), "ist-o7-hoehe-og.png");
}
