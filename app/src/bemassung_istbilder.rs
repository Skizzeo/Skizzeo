//! Ist-Bilder der Bemaßung auf der GPU (Mesa unter Xvfb, wie
//! [`crate::gpu_probe`]):
//!
//! `xvfb-run -a env SKIZZEO_ISTBILDER=<ordner> cargo test -p skizzeo gpu_istbilder_bemassung -- --ignored`

use super::Massketten;
use crate::camera::Camera;
use crate::scene::Scene;
use crate::ui::ViewKind;
use crate::wall_edit::mass_treffer::{treffer, Treffer};
use crate::wall_edit::WallEdit;
use sk_math::{vec3, Vec3};
use sk_model::bemassung::Kettenart;
use sk_model::{szo, GuidGen};
use sk_plan::bild::Zahlart;
use sk_platform::{Event, Key, Modifiers, MouseButton};
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
    bild_mit(r, s, v, cam, bezug, None)
}

/// Wie [`bild`], mit der Hervorhebung und den Hilfslinien des Bedienens.
fn bild_mit(
    r: &mut Renderer,
    s: &mut Scene,
    v: ViewKind,
    cam: &Camera,
    bezug: bool,
    e: Option<&WallEdit>,
) -> Vec<u8> {
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
    let hervor = e.map(|e| e.mass_hervor(s)).unwrap_or_default();
    let (mut h, t) = mk.zeichnen(s, v, &[], f.as_ref(), &blick, 1.0, &theme, &hervor);
    if let Some(e) = e {
        h.extend(e.helpers(s, cam, 1.0, false, &theme));
    }
    r.set_helpers(&h);
    r.set_ink_teile(&t);
    let mut view = cam.view(W, H);
    if v != ViewKind::Persp {
        view.paper = Some(s.table().paper);
    }
    r.draw(W, H, 0, &view).unwrap();
    r.read_pixels(W, H)
}

/// Eingabefeld der Maßzahl wie in der App rechts unter der Maus
/// (`App::paint_input_pill`); ohne Eingabe keins.
fn pille(r: &mut Renderer, e: &WallEdit, (mx, my): (f64, f64)) {
    let Some(i) = e.input() else {
        r.set_overlay(crate::OVERLAY_INPUT, 0, 0, 0, 0, &[]);
        return;
    };
    let fonts = sk_ui::widgets::Fonts {
        regular: schrift(),
        bold: schrift(),
        italic: None,
    };
    let c = i.paint(&fonts, [e.input_label(), ""], 1.0, &Theme::dark());
    let (x, y) = ((mx + 14.0).round() as i32, (my + 20.0).round() as i32);
    let (w, h) = (c.width as u32, c.height as u32);
    r.set_overlay(crate::OVERLAY_INPUT, x, y, w, h, &c.to_premul_rgba8());
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
        // „Tragende Schicht“: Rohbaumaße, Öffnungen am Loch im Mauerwerk
        // (Bedienbarkeit 40.1)
        let bezug = |s: &mut Scene, b: sk_model::bemassung::Aussenbezug| {
            let mut stil = s.model().massstil().clone();
            stil.bezug = b;
            s.begin_settings();
            s.edit_attr(|m| m.set_massstil(stil));
            s.commit();
        };
        bezug(&mut s, sk_model::bemassung::Aussenbezug::Rohbau);
        ab(
            &bild(&mut r, &mut s, ViewKind::Plan, &cam, false),
            &format!("ist-bem-{name}-grundriss-rohbau.png"),
        );
        // RH1 (WDVS) mit einem Fenster in der Südwand: im Mauerwerk 3 cm je
        // Seite breiter als außen in der Dämmung
        if name == "rh1" {
            bezug(&mut s, sk_model::bemassung::Aussenbezug::Wand);
            fenster_sued(&mut s);
            ab(
                &bild(&mut r, &mut s, ViewKind::Plan, &cam, false),
                "ist-bem-rh1-fenster-wandpaket.png",
            );
            bezug(&mut s, sk_model::bemassung::Aussenbezug::Rohbau);
            ab(
                &bild(&mut r, &mut s, ViewKind::Plan, &cam, false),
                "ist-bem-rh1-fenster-rohbau.png",
            );
        }
    }
}

/// Fenster 1,01 m breit, 2 m von der Südwestecke der Außenhaut, zwischen
/// Brüstung und Sturz des Geschosses (Ketten über das Wandpaket).
fn fenster_sued(s: &mut Scene) {
    use sk_model::oeffnung::{HeightLine, HeightRef, Opening, OpeningKind};
    let st = s.active_storey();
    let a = sk_plan::aussen::aussenmasse(s.model(), st).expect("Ketten");
    let sued = a.seiten.iter().find(|x| x.grad == 270).expect("Südseite");
    let g = sued.ketten.iter().find(|k| k.art == Kettenart::Gesamt);
    let t0 = g.expect("Gesamtmaß").bezuege[0].t;
    let p = |t: f64| sued.t * t + sued.n * sued.aussen;
    let o = Opening {
        kind: OpeningKind::Window,
        path: vec![p(t0 + 3010.0), p(t0 + 2000.0)],
        bottom: HeightRef::Line(st, HeightLine::Sill),
        top: HeightRef::Line(st, HeightLine::Lintel),
        reveal: None,
        install: Default::default(),
        swing: None,
        lock: Vec::new(),
    };
    assert!(s.add_opening(o).is_some(), "Fenster");
}

/// Stufe 2 (Bedienen) an RH1 im Grundriss: Block Süd gewählt und eine
/// Maßzahl unter der Maus; dieselbe Zahl in Eingabe, erst mit dem Maß
/// vorbelegt und markiert (Bedienbarkeit 40.3), dann mit Pfeil und live
/// gesetztem Wert, je mit dem Eingabefeld wie in der App (40.8); die
/// Gesamtkette 1 m nach außen gezogen.
#[test]
#[ignore = "braucht einen X-Server (xvfb-run) und SKIZZEO_ISTBILDER"]
fn gpu_istbilder_bedienen() {
    let Some(ziel) = std::env::var_os("SKIZZEO_ISTBILDER").map(std::path::PathBuf::from) else {
        return;
    };
    let k = sk_render::glx::kontext(W as i32, H as i32).expect("GLX-Kontext (DISPLAY?)");
    let theme = Theme::dark();
    let mut r = Renderer::new(k.gl, crate::style(&theme.env)).unwrap();
    let ab =
        |px: &[u8], n: &str| std::fs::write(ziel.join(n), sk_paint::encode_png(W, H, px)).unwrap();
    let referenz = concat!(env!("CARGO_MANIFEST_DIR"), "/../crates/sk-cost/referenz/");
    let mut s = haus(&format!("{referenz}rh1-standardhaus.szo"));
    let (lo, hi) = s.bounds().unwrap();
    let rand = vec3(6000.0, 6000.0, 0.0);
    let cam = crate::fit_parallel(
        ViewKind::Plan,
        Some((lo - rand, hi + rand)),
        W as f64,
        H as f64,
    );
    let (w, h) = (W as f64, H as f64);
    s.set_mass_sicht(ViewKind::Plan, true, false);
    let f = schrift();
    let mut mk = Massketten::default();
    let mut e = WallEdit::default();
    let mut ev = |e: &mut WallEdit, s: &mut Scene, x: Event| {
        let st = mk.gezeigt(s, ViewKind::Plan, &[], f.as_ref());
        e.masse.setze(st, s);
        e.handle(&x, s, &cam, w, h, 1.0, true);
    };
    // Mitte einer Zahl der Südseite, die sich überschreiben lässt (Wand)
    // bzw. des Gesamtmaßes
    let mitte = |s: &Scene, art: Kettenart| -> (f64, f64) {
        let st = Massketten::default()
            .gezeigt(s, ViewKind::Plan, &[], schrift().as_ref())
            .unwrap();
        for (a, b) in st.paare() {
            for z in &b.zahlen {
                let Zahlart::Mass { seite, kette, .. } = z.art else {
                    continue;
                };
                let sd = &a.seiten[seite];
                if sd.grad != 270 || sd.ketten[kette].art != art {
                    continue;
                }
                let m = z.rahmen.iter().fold(Vec3::ZERO, |s, p| s + *p) * 0.25;
                let p = cam.project(m, w, h).unwrap();
                let t = treffer(&st, &cam, w, h, 1.0, p.0, p.1);
                if art == Kettenart::Gesamt || matches!(t, Some(Treffer::Zahl(_))) {
                    return p;
                }
            }
        }
        panic!("keine Zahl")
    };
    let maus = |x: f64, y: f64, mods: Modifiers| {
        [
            Event::MouseMove { x, y, mods },
            Event::MouseDown {
                button: MouseButton::Left,
                x,
                y,
                mods,
            },
            Event::MouseUp {
                button: MouseButton::Left,
                x,
                y,
                mods,
            },
        ]
    };
    let umsch = Modifiers {
        shift: true,
        ..Default::default()
    };
    let g = mitte(&s, Kettenart::Gesamt);
    for x in maus(g.0, g.1, umsch) {
        ev(&mut e, &mut s, x);
    }
    let z = mitte(&s, Kettenart::Wand);
    let m = Modifiers::default();
    ev(
        &mut e,
        &mut s,
        Event::MouseMove {
            x: z.0,
            y: z.1,
            mods: m,
        },
    );
    ab(
        &bild_mit(&mut r, &mut s, ViewKind::Plan, &cam, false, Some(&e)),
        "ist-bem-rh1-block.png",
    );
    for x in maus(z.0, z.1, m) {
        ev(&mut e, &mut s, x);
    }
    pille(&mut r, &e, z);
    ab(
        &bild_mit(&mut r, &mut s, ViewKind::Plan, &cam, false, Some(&e)),
        "ist-bem-rh1-eingabe-offen.png",
    );
    for ch in "4,5".chars() {
        ev(
            &mut e,
            &mut s,
            Event::Key {
                key: Key::Char(ch),
                down: true,
                repeat: false,
                mods: m,
            },
        );
    }
    pille(&mut r, &e, z);
    ab(
        &bild_mit(&mut r, &mut s, ViewKind::Plan, &cam, false, Some(&e)),
        "ist-bem-rh1-eingabe.png",
    );
    pille(&mut r, &WallEdit::default(), z);
    let esc = Event::Key {
        key: Key::Escape,
        down: true,
        repeat: false,
        mods: m,
    };
    ev(&mut e, &mut s, esc);
    // Gesamtkette Süd greifen und 1 m nach außen (unten) ziehen
    let g = mitte(&s, Kettenart::Gesamt);
    let p = cam.plane_point(g.0, g.1, w, h, 0.0).unwrap();
    let q = cam.project(p + vec3(0.0, -1000.0, 0.0), w, h).unwrap();
    let [mv, down, _] = maus(g.0, g.1, m);
    ev(&mut e, &mut s, mv);
    ev(&mut e, &mut s, down);
    ev(
        &mut e,
        &mut s,
        Event::MouseMove {
            x: q.0,
            y: q.1,
            mods: m,
        },
    );
    ev(
        &mut e,
        &mut s,
        Event::MouseUp {
            button: MouseButton::Left,
            x: q.0,
            y: q.1,
            mods: m,
        },
    );
    ab(
        &bild_mit(&mut r, &mut s, ViewKind::Plan, &cam, false, Some(&e)),
        "ist-bem-rh1-verschoben.png",
    );
}
