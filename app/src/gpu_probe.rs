//! Prüfbilder S5 auf der GPU (nur Linux): Mesa unter Xvfb über
//! [`sk_render::glx`].
//!
//! `xvfb-run -a env SKIZZEO_ISTBILDER=<ordner> cargo test -p skizzeo gpu_ -- --ignored`

mod tests {
    use crate::camera::Camera;
    use crate::scene::Scene;
    use crate::sonne_view as sv;
    use crate::ui::ViewKind;
    use sk_math::sonne::Datum;
    use sk_math::{vec3, Vec3};
    use sk_model::{szo, GuidGen, Model, Sun};
    use sk_render::Renderer;
    use sk_ui::theme::Theme;

    const W: u32 = 1600;
    const H: u32 = 1000;

    fn haus() -> Scene {
        let text = include_str!("../../crates/sk-cost/referenz/rh1-standardhaus.szo");
        let m = szo::read_with(text, GuidGen::with_seed(1), &sk_cost::lesen::ABSCHNITTE_SZO)
            .unwrap()
            .model;
        Scene::with_model(m)
    }

    fn sonne(m: u32, d: u32, minuten: u32) -> Sun {
        Sun {
            date: Datum::new(2026, m, d).unwrap(),
            minutes: minuten,
            on: true,
        }
    }

    /// Ein Bild wie in der App: Flächen und Kanten, Himmel und Boden, Licht
    /// und Schatten der Sonne; ohne Gebäude der Würfel.
    fn bild(r: &mut Renderer, s: &mut Scene, sun: Sun, cam: &Camera) -> Vec<u8> {
        let wuerfel = s.bounds().is_none().then(sv::wuerfel_netz);
        bild_mit(r, s, sun, cam, wuerfel.unwrap_or_default())
    }

    /// Wie [`bild`] mit `dazu` als Anzeige-Netz (Würfel oder Probestück).
    fn bild_mit(
        r: &mut Renderer,
        s: &mut Scene,
        sun: Sun,
        cam: &Camera,
        dazu: sk_render::MeshData,
    ) -> Vec<u8> {
        let theme = Theme::dark();
        s.set_theme(&theme);
        r.set_style(crate::style(&theme.env));
        r.set_looks(&s.table().looks_with(1.0, |_| 1.0));
        r.set_mesh(crate::MESH_MODEL, &s.mesh(ViewKind::Persp, None, &[]));
        r.set_mesh(crate::MESH_WUERFEL, &dazu);
        let ort = *s.model().location();
        if let Some(l) = sv::licht(&ort, &sun) {
            r.set_light(l);
        }
        r.set_sun(sv::sonnenlicht(&ort, &sun));
        r.draw(W, H, 0, &cam.view(W, H)).unwrap();
        r.read_pixels(W, H)
    }

    fn blick(s: &Scene, von: Vec3, weite: f64) -> Camera {
        let (lo, hi) = s.bounds().unwrap_or_else(sv::wuerfel_quader);
        let ziel = (lo + hi) * 0.5;
        Camera::looking_at(ziel + von.normalized() * weite, ziel, 45.0)
    }

    /// Prüfbilder S5 auf der GPU (Mesa unter Xvfb): Würfel am 21.06. und
    /// 21.12. um 12:00, das Haus am 21.06. um 15:00 und nah an einem
    /// Fenster (Laibung).
    #[test]
    #[ignore = "braucht einen X-Server (xvfb-run) und SKIZZEO_ISTBILDER"]
    fn gpu_istbilder_s5() {
        let Some(ziel) = std::env::var_os("SKIZZEO_ISTBILDER").map(std::path::PathBuf::from) else {
            return;
        };
        let k = sk_render::glx::kontext(W as i32, H as i32).expect("GLX-Kontext (DISPLAY?)");
        let theme = Theme::dark();
        let mut r = Renderer::new(k.gl, crate::style(&theme.env)).unwrap();
        assert_eq!(r.pattern_error(), None);
        let ab = |px: &[u8], n: &str| {
            std::fs::write(ziel.join(n), sk_paint::encode_png(W, H, px)).unwrap()
        };
        // Würfel von Südwesten, der Ausschnitt reicht nach Norden bis über
        // den langen Winterschatten hinaus
        let mut w = Scene::with_model(Model::with_seed(1));
        let ziel = vec3(5000.0, 22000.0, 0.0);
        let cam = Camera::looking_at(ziel + vec3(-38000.0, -42000.0, 40000.0), ziel, 45.0);
        let ort = *w.model().location();
        for (n, m, d, soll) in [("0621", 6, 21, 5700.0), ("1221", 12, 21, 42000.0)] {
            // Wahrer Mittag: die Sonne steht am höchsten, genau im Süden
            let hoch = |t: u32| sv::zur_sonne(&ort, &sonne(m, d, t)).map_or(-1.0, |v| v.z);
            let t = (11 * 60..15 * 60)
                .max_by(|a, b| hoch(*a).total_cmp(&hoch(*b)))
                .unwrap();
            let l = 10000.0 / hoch(t).asin().tan();
            assert!((l - soll).abs() / soll < 0.02, "{n}: {l}");
            let px = bild(&mut r, &mut w, sonne(m, d, t), &cam);
            ab(&px, &format!("ist-s5-wuerfel-{n}-mittag.png"));
            // Am Boden hinter der Nordkante: bis kurz vor der Schattenlänge
            // dunkel, kurz danach hell wie der freie Boden (±5 %)
            let hell = |y: f64| {
                let (x, y) = cam
                    .project(vec3(5000.0, y, 0.0), W as f64, H as f64)
                    .unwrap();
                let i = (y as usize * W as usize + x as usize) * 4;
                px[i] as f64 + px[i + 1] as f64 + px[i + 2] as f64
            };
            let frei = hell(-8000.0);
            assert!(hell(10000.0 + l * 0.95) < frei * 0.8, "{n}: dunkel");
            assert!((hell(10000.0 + l * 1.05) - frei).abs() < 3.0, "{n}: hell");
        }
        let mut h = haus();
        let cam = blick(&h, vec3(0.8, -1.0, 0.55), 30000.0);
        let px = bild(&mut r, &mut h, sonne(6, 21, 15 * 60), &cam);
        ab(&px, "ist-s5-haus-0621-1500.png");
        let px = bild(&mut r, &mut h, sonne(6, 21, 10 * 60), &cam);
        ab(&px, "ist-s5-haus-0621-1000.png");
        // Wandstück 36,5 cm mit Fenster 1,00 × 1,40 m, Glas 15 cm hinter
        // der Außenseite (Süden); Sonne am Vormittag aus Südosten
        let q = |a: [f64; 3], b: [f64; 3]| {
            sv::quader_netz((vec3(a[0], a[1], a[2]), vec3(b[0], b[1], b[2])))
        };
        let mut stueck = sk_render::MeshData::default();
        for m in [
            q([0.0, 0.0, 0.0], [2500.0, 365.0, 3000.0]),
            q([3500.0, 0.0, 0.0], [6000.0, 365.0, 3000.0]),
            q([2500.0, 0.0, 0.0], [3500.0, 365.0, 1000.0]),
            q([2500.0, 0.0, 2400.0], [3500.0, 365.0, 3000.0]),
            q([2500.0, 150.0, 1000.0], [3500.0, 170.0, 2400.0]),
        ] {
            stueck.faces.extend(m.faces);
            stueck.edges.extend(m.edges);
        }
        let mut leer = Scene::with_model(Model::with_seed(1));
        let ziel = vec3(3000.0, 0.0, 1700.0);
        let cam = Camera::looking_at(ziel + vec3(1800.0, -5200.0, 900.0), ziel, 45.0);
        let px = bild_mit(&mut r, &mut leer, sonne(6, 21, 10 * 60), &cam, stueck);
        ab(&px, "ist-s5-laibung-0621-1000.png");
        assert_eq!(r.take_shadow_error(), None);
    }
}
