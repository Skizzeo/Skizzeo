//! Ist-Bilder zum Gelände (Bedienbarkeit 42.7) am erfundenen Beispiel
//! „Am Lindenhof 7“ mit zwei Geländeflächen: die Leiste „Gelände“ mit und
//! ohne Modul und mit roter Zeile, dazu der graue Knopf „Mengen“ mit „ab
//! Pro“ in der Stufe Basis, gezeichnet mit dem Pinsel der Oberfläche wie in
//! der App:
//!
//! `SKIZZEO_ISTBILDER=<ordner> cargo test -p skizzeo istbilder_gelaende -- --ignored`
//!
//! Grundriss, Schnitt, Ansicht und 3D mit Gelände und Flächen auf der GPU
//! (Mesa unter Xvfb, wie [`crate::gpu_probe`]):
//!
//! `xvfb-run -a env SKIZZEO_ISTBILDER=<ordner> cargo test -p skizzeo gpu_istbilder_gelaende -- --ignored`
//!
//! Höhenfarben, Höhenlinien und Glätten in 3D, dazu eine Kuppe mit
//! geschlossenen Höhenlinien (Jörn 17.10., Test B-257a):
//!
//! `xvfb-run -a env SKIZZEO_ISTBILDER=<ordner> cargo test -p skizzeo gpu_istbilder_hoehen -- --ignored`

use super::*;
use crate::freischaltung::Freischaltung;
use crate::gelaende_app::{leiste, Zustand};
use crate::ui::{GelaendeLeiste, Panel, Ui};
use sk_model::{Terrain, TerrainPatch};
use sk_ui::theme::Theme;
use sk_ui::widgets::Fonts;
use std::path::PathBuf;
use std::time::Duration;

const CSV: &[u8] =
    include_bytes!("../../crates/sk-gelaende/tests/daten/hoehenaufmass-lindenhof-7.csv");

/// Haus „Am Lindenhof 7“ (x 7 bis 17 m, y 6 bis 14 m) auf dem Aufmaß des
/// Beispiels, östlich eine befestigte Terrasse 15 cm unter ±0,00, südlich
/// ein Garten 40 cm darunter.
fn lindenhof() -> Model {
    let mut m = Model::with_seed(7);
    let b = m.add_building(1);
    let pts = [
        vec3(7000.0, 6000.0, 0.0),
        vec3(7000.0, 14000.0, 0.0),
        vec3(17000.0, 14000.0, 0.0),
        vec3(17000.0, 6000.0, 0.0),
    ];
    m.build_from_polygon(b, &pts).unwrap();
    let (t, ..) = Terrain::lesen(CSV, "hoehenaufmass-lindenhof-7.csv", "2026-10-12", 32).unwrap();
    m.begin("Aufmaß");
    assert!(m.import_terrain(t));
    m.commit();
    let r = |x0: f64, y0: f64, x1: f64, y1: f64| vec![[x0, y0], [x1, y0], [x1, y1], [x0, y1]];
    let mut terrasse = TerrainPatch::flat(r(17000.0, 7000.0, 21000.0, 12000.0), -150.0);
    terrasse.paved = true;
    m.begin("Flächen");
    assert!(m.add_site_patch(terrasse).is_some());
    let garten = TerrainPatch::flat(r(7000.0, 1000.0, 15000.0, 5000.0), -400.0);
    assert!(m.add_site_patch(garten).is_some());
    m.commit();
    m
}

fn ziel() -> Option<PathBuf> {
    let d = PathBuf::from(std::env::var_os("SKIZZEO_ISTBILDER")?);
    std::fs::create_dir_all(&d).unwrap();
    Some(d)
}

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

/// Paneel `p` mit der Stufe `stufe` und, wenn offen, der Leiste.
fn paneel(p: Panel, stufe: &str, l: Option<GelaendeLeiste>) -> Vec<u8> {
    let t = Theme::dark();
    let mut ui = Ui::new(2.0, &t);
    ui.fonts = schriften();
    ui.frei = Freischaltung::stufe(stufe).unwrap();
    ui.gelaende = l;
    let (c, _, _) = ui.paint(&t, p, 1280, 32);
    c.to_png()
}

/// Die Leiste nach dem Einlesen mit Flächen, ohne Modul und mit roter
/// Zeile; der Knopf „Mengen“ in Basis grau mit „ab Pro“, in Pro frei.
#[test]
#[ignore = "braucht SKIZZEO_ISTBILDER"]
fn istbilder_gelaende() {
    let Some(ziel) = ziel() else {
        return;
    };
    let ab = |png: Vec<u8>, n: &str| std::fs::write(ziel.join(n), png).unwrap();
    let mut m = lindenhof();
    let mut z = Zustand::default();
    z.offen = true;
    let l = leiste(&m, &z);
    assert!(l.aufmass && !l.deckt_nicht);
    let tools = Panel::Tools;
    ab(
        paneel(tools, "ultimate+gelaende", Some(l.clone())),
        "ist-427-leiste.png",
    );
    ab(
        paneel(tools, "ultimate", Some(l)),
        "ist-427-leiste-ohne-modul.png",
    );
    ab(
        paneel(Panel::Views, "basis", None),
        "ist-427-knopf-grau.png",
    );
    ab(paneel(Panel::Views, "pro", None), "ist-427-knopf-pro.png");
    // weit verschoben: die rote Zeile, sobald der Faden gezählt hat
    m.begin("Verschieben");
    assert!(m.move_terrain([200_000.0, 0.0]));
    m.commit();
    z.baugruben(&m);
    while z.prueft() {
        std::thread::sleep(Duration::from_millis(1));
        z.baugruben(&m);
    }
    let l = leiste(&m, &z);
    assert!(l.deckt_nicht);
    ab(
        paneel(tools, "ultimate+gelaende", Some(l)),
        "ist-427-leiste-rot.png",
    );
}

/// GLX gibt es nur unter Linux, wie bei [`crate::gpu_probe`].
#[cfg(target_os = "linux")]
mod gpu {
    use super::*;
    use crate::camera::Camera;
    use crate::scene::Scene;
    use sk_render::Renderer;

    const W: u32 = 1600;
    const H: u32 = 1000;

    /// Bild der Ansicht `v` wie in der App: Modell, Netz des Geländes aus dem
    /// Faden der Platte und ihre Linien; im Schnitt die Ebene `ebene`.
    fn bild(
        r: &mut Renderer,
        s: &mut Scene,
        pl: &mut Platte,
        v: ViewKind,
        ebene: Option<(Vec3, Vec3)>,
        cam: &Camera,
    ) -> Vec<u8> {
        bild_mit(r, s, pl, (v, ebene), cam, (true, false))
    }

    /// Wie [`bild`], mit `(linien, farben)`: Höhenlinien in 3D und
    /// Höhenfarben wie mit den Schaltern der Leiste.
    fn bild_mit(
        r: &mut Renderer,
        s: &mut Scene,
        pl: &mut Platte,
        (v, ebene): (ViewKind, Option<(Vec3, Vec3)>),
        cam: &Camera,
        (linien, farben): (bool, bool),
    ) -> Vec<u8> {
        let theme = Theme::dark();
        s.set_theme(&theme);
        r.set_style(crate::style(&theme.env));
        r.set_looks(&s.table().looks_with(1.0, |_| 1.0));
        r.set_mesh(crate::MESH_MODEL, &s.mesh(v, ebene, &[]));
        r.set_sun(None);
        let m = s.model();
        pl.im_bild(m);
        let sicht: Sicht = (v, ebene, s.table().terrain, false, false, linien);
        let key = netz_key(pl.fassung, &sicht);
        let netz = loop {
            if let Some(n) = pl.netz_zur_sicht(key, sicht) {
                break n;
            }
            while pl.rechnet() {
                pl.nachziehen(m, sicht);
                std::thread::sleep(Duration::from_millis(1));
            }
        };
        r.set_mesh(MESH_PLATE, &netz);
        r.set_grass(sicht.2[0]);
        let stufen = pl
            .hoehen
            .filter(|_| farben && v == ViewKind::Persp)
            .map(|(lo, hi)| farben::stufen(lo, hi, m.terrain_nhn().unwrap_or(0.0)));
        r.set_hoehenfarben(stufen.map(|st| sk_render::Hoehenfarben {
            key: sicht.2[0],
            z0: st.z0 as f32,
            stufe: st.stufe as f32,
            n: st.n,
        }));
        let mm_px = cam.ortho.map_or(10.0, |half| 2.0 * half / f64::from(H));
        let h = pl
            .linien_im_bild(m, (v, ebene), s.bounds(), s.table(), (1.0, mm_px))
            .to_vec();
        r.set_helpers(&h);
        let mut view = cam.view(W, H);
        if v != ViewKind::Persp {
            view.paper = Some(s.table().paper);
        }
        view.patterns = crate::draw_table::pattern_mode(v, theme.env.patterns_3d);
        r.draw(W, H, 0, &view).unwrap();
        r.read_pixels(W, H)
    }

    /// Grundriss, Schnitt durch Haus und Terrasse, Ansicht von Süden mit dem
    /// Garten davor und 3D.
    #[test]
    #[ignore = "braucht einen X-Server (xvfb-run) und SKIZZEO_ISTBILDER"]
    fn gpu_istbilder_gelaende() {
        let Some(ziel) = ziel() else {
            return;
        };
        let ab = |px: &[u8], n: &str| {
            std::fs::write(ziel.join(n), sk_paint::encode_png_gepackt(W, H, px)).unwrap();
        };
        let k = sk_render::glx::kontext(W as i32, H as i32).expect("GLX-Kontext (DISPLAY?)");
        let theme = Theme::dark();
        let mut r = Renderer::new(k.gl, crate::style(&theme.env)).unwrap();
        let mut s = Scene::with_model(lindenhof());
        let mut pl = Platte::default();
        // Haus, Terrasse und Garten mit Rand
        let rahmen = Some((
            vec3(3000.0, -2000.0, -3000.0),
            vec3(25000.0, 18000.0, 6000.0),
        ));
        let fit = |v| crate::fit_parallel(v, rahmen, f64::from(W), f64::from(H));
        let plan = ViewKind::Plan;
        ab(
            &bild(&mut r, &mut s, &mut pl, plan, None, &fit(plan)),
            "ist-427-grundriss.png",
        );
        let schnitt = ViewKind::Section;
        let ebene = Some((vec3(0.0, 9500.0, 0.0), vec3(0.0, -1.0, 0.0)));
        ab(
            &bild(&mut r, &mut s, &mut pl, schnitt, ebene, &fit(schnitt)),
            "ist-427-schnitt.png",
        );
        let vorne = ViewKind::Front;
        ab(
            &bild(&mut r, &mut s, &mut pl, vorne, None, &fit(vorne)),
            "ist-427-ansicht-sued.png",
        );
        let mitte = vec3(14000.0, 8000.0, 0.0);
        let cam3 = Camera::looking_at(
            mitte + vec3(0.55, -1.0, 0.8).normalized() * 32000.0,
            mitte,
            45.0,
        );
        ab(
            &bild(&mut r, &mut s, &mut pl, ViewKind::Persp, None, &cam3),
            "ist-427-3d.png",
        );
    }

    /// Kuppe 40 × 40 m, in der Mitte 1,50 m höher: Höhenlinien als Ringe
    /// um die Kuppe, im Grundriss und in 3D, eckig und geglättet.
    fn kuppe() -> Model {
        let mut m = Model::with_seed(7);
        let mut text = String::new();
        for i in 0..9 {
            for j in 0..9 {
                let (x, y) = (5.0 * j as f64, 5.0 * i as f64);
                let r2 = (x - 20.0).powi(2) + (y - 20.0).powi(2);
                let h = 10.0 + 1.5 * (-r2 / 120.0).exp();
                text.push_str(&format!("{} {x} {y} {h:.3}\n", i * 9 + j + 1));
            }
        }
        let (t, ..) = Terrain::lesen(text.as_bytes(), "kuppe.txt", "2026-10-17", 32).unwrap();
        m.begin("Aufmaß");
        assert!(m.import_terrain(t));
        m.commit();
        m
    }

    /// Höhenfarben mit und ohne Höhenlinien, Glätten fein und die Kuppe.
    #[test]
    #[ignore = "braucht einen X-Server (xvfb-run) und SKIZZEO_ISTBILDER"]
    fn gpu_istbilder_hoehen() {
        use sk_model::Glaettung;
        let Some(ziel) = ziel() else {
            return;
        };
        let ab = |px: &[u8], n: &str| {
            std::fs::write(ziel.join(n), sk_paint::encode_png_gepackt(W, H, px)).unwrap();
        };
        let k = sk_render::glx::kontext(W as i32, H as i32).expect("GLX-Kontext (DISPLAY?)");
        let theme = Theme::dark();
        let mut r = Renderer::new(k.gl, crate::style(&theme.env)).unwrap();
        let p3 = (ViewKind::Persp, None);
        // Lindenhof: Rasen, Farben, Farben ohne Linien, geglättet
        let mitte = vec3(14000.0, 8000.0, 0.0);
        let cam3 = Camera::looking_at(
            mitte + vec3(0.55, -1.0, 0.8).normalized() * 32000.0,
            mitte,
            45.0,
        );
        let mut s = Scene::with_model(lindenhof());
        let mut pl = Platte::default();
        for (n, art) in [
            ("ist-hoehen-3d-rasen.png", (true, false)),
            ("ist-hoehen-3d-farben.png", (true, true)),
            ("ist-hoehen-3d-farben-ohne-linien.png", (false, true)),
        ] {
            ab(&bild_mit(&mut r, &mut s, &mut pl, p3, &cam3, art), n);
        }
        let mut m = lindenhof();
        m.begin("Gelände glätten");
        assert!(m.set_terrain_smoothing(Glaettung::Fein));
        m.commit();
        let mut s = Scene::with_model(m);
        let mut pl = Platte::default();
        ab(
            &bild_mit(&mut r, &mut s, &mut pl, p3, &cam3, (true, false)),
            "ist-hoehen-3d-glatt-fein.png",
        );
        ab(
            &bild_mit(&mut r, &mut s, &mut pl, p3, &cam3, (true, true)),
            "ist-hoehen-3d-glatt-fein-farben.png",
        );
        // Kuppe: geschlossene Ringe im Grundriss und in 3D
        for (glatt, n) in [(Glaettung::Aus, "eckig"), (Glaettung::Fein, "glatt")] {
            let mut m = kuppe();
            if glatt != Glaettung::Aus {
                m.begin("Gelände glätten");
                assert!(m.set_terrain_smoothing(glatt));
                m.commit();
            }
            let k = m.terrain_border().iter().flatten().fold(
                [f64::MAX, f64::MAX, f64::MAX, f64::MIN, f64::MIN, f64::MIN],
                |k, p| {
                    [
                        k[0].min(p.x),
                        k[1].min(p.y),
                        k[2].min(p.z),
                        k[3].max(p.x),
                        k[4].max(p.y),
                        k[5].max(p.z),
                    ]
                },
            );
            let rahmen = Some((
                vec3(k[0] - 3000.0, k[1] - 3000.0, k[2] - 1000.0),
                vec3(k[3] + 3000.0, k[4] + 3000.0, k[5] + 3000.0),
            ));
            let plan = ViewKind::Plan;
            let cam = crate::fit_parallel(plan, rahmen, f64::from(W), f64::from(H));
            let mut s = Scene::with_model(m);
            let mut pl = Platte::default();
            ab(
                &bild_mit(&mut r, &mut s, &mut pl, (plan, None), &cam, (true, false)),
                &format!("ist-kuppe-grundriss-{n}.png"),
            );
            let mitte = vec3((k[0] + k[3]) / 2.0, (k[1] + k[4]) / 2.0, k[2]);
            let cam3 = Camera::looking_at(
                mitte + vec3(0.6, -1.0, 0.7).normalized() * 55000.0,
                mitte,
                45.0,
            );
            ab(
                &bild_mit(&mut r, &mut s, &mut pl, p3, &cam3, (true, false)),
                &format!("ist-kuppe-3d-{n}.png"),
            );
            ab(
                &bild_mit(&mut r, &mut s, &mut pl, p3, &cam3, (true, true)),
                &format!("ist-kuppe-3d-{n}-farben.png"),
            );
        }
    }
}
