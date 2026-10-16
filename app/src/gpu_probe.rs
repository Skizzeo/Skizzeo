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
    /// Ablage für ein Netz dazu (Würfel als Testkörper, Probestück), hinter
    /// allen Ablagen der App.
    const MESH_DAZU: usize = crate::ausbau_app::MESH_FREI;

    fn haus() -> Scene {
        let text = include_str!("../../crates/sk-cost/referenz/rh1-standardhaus.szo");
        let m = szo::read_with(text, GuidGen::with_seed(1), &sk_cost::lesen::ABSCHNITTE_SZO)
            .unwrap()
            .model;
        Scene::with_model(m)
    }

    /// RH-3 mit Versatz und Dachterrasse: Vor- und Rücksprünge für
    /// Schatten in den Ansichten.
    fn haus3() -> Scene {
        let text = include_str!("../../crates/sk-cost/referenz/rh3-versatz-dachterrasse.szo");
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
    /// und Schatten der Sonne; ohne Gebäude der Würfel als Testkörper (die
    /// App zeigt seit S9 ohne Gebäude keine Sonne).
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
        r.set_mesh(MESH_DAZU, &dazu);
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
        // Tiefe Sonne (§8 10:50): 21.12. 15:30 MEZ, 3,5°, Schatten bis
        // weit über den Ausschnitt; ohne Akne, nicht abgelöst
        let tief = sonne(12, 21, 15 * 60 + 30);
        let hoehe = sv::zur_sonne(h.model().location(), &tief).unwrap().z.asin();
        assert!(
            (hoehe.to_degrees() - 3.5).abs() < 0.5,
            "{}",
            hoehe.to_degrees()
        );
        let px = bild(&mut r, &mut h, tief, &cam);
        ab(&px, "ist-s5-haus-1221-1530.png");
        let von_sueden = blick(&w, vec3(0.2, -1.0, 0.5), 45000.0);
        let px = bild(&mut r, &mut w, tief, &von_sueden);
        ab(&px, "ist-s5-wuerfel-1221-1530.png");
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

    /// Wie [`bild`] mit dem Griff an der Schattenspitze und seiner Kurve
    /// (wie beim Darüberfahren) über Bahnen und Sonne (S6).
    fn bild_s6(r: &mut Renderer, s: &mut Scene, sun: Sun, cam: &Camera) -> Vec<u8> {
        let ort = *s.model().location();
        let ecken = match s.bounds() {
            Some(_) => sv::ecken(&s.mesh(ViewKind::Persp, None, &[]).faces),
            None => sv::ecken(&sv::wuerfel_netz().faces),
        };
        let q = s.bounds().unwrap_or_else(sv::wuerfel_quader);
        let h = sv::himmel(&ort, &sun, q);
        let g = sv::griff(&ort, &sun, &ecken);
        assert!(g.is_some(), "Griff");
        let mut sys = sv::Sonnensystem::default();
        sys.ueber_schatten = true;
        let mut helpers = sv::helpers(&h, false, 1.0);
        helpers.extend(sv::schatten_helpers(
            &sys,
            g.as_ref(),
            sv::zur_sonne(&ort, &sun),
            1.0,
        ));
        r.set_helpers(&helpers);
        let px = bild(r, s, sun, cam);
        r.set_helpers(&[]);
        px
    }

    /// Prüfbilder S6 (Griff und Kurve) und die Zeit des Tiefen-Durchgangs
    /// beim Ziehen: je Bild eine neue Sonne, wie beim Ziehen über den Tag.
    #[test]
    #[ignore = "braucht einen X-Server (xvfb-run) und SKIZZEO_ISTBILDER"]
    fn gpu_istbilder_s6() {
        let Some(ziel) = std::env::var_os("SKIZZEO_ISTBILDER").map(std::path::PathBuf::from) else {
            return;
        };
        let k = sk_render::glx::kontext(W as i32, H as i32).expect("GLX-Kontext (DISPLAY?)");
        let theme = Theme::dark();
        let mut r = Renderer::new(k.gl, crate::style(&theme.env)).unwrap();
        let ab = |px: &[u8], n: &str| {
            std::fs::write(ziel.join(n), sk_paint::encode_png(W, H, px)).unwrap()
        };
        let mut w = Scene::with_model(Model::with_seed(1));
        let mitte = vec3(5000.0, 5000.0, 0.0);
        let cam = Camera::looking_at(mitte + vec3(-30000.0, -95000.0, 110000.0), mitte, 45.0);
        let px = bild_s6(&mut r, &mut w, sonne(6, 21, 10 * 60), &cam);
        ab(&px, "ist-s6-wuerfel-0621-1000.png");
        let mut h = haus();
        let cam = blick(&h, vec3(0.3, -1.0, 0.9), 70000.0);
        let px = bild_s6(&mut r, &mut h, sonne(12, 21, 14 * 60), &cam);
        ab(&px, "ist-s6-haus-1221-1400.png");

        // Ziehen: 40 Bilder, je fünf Minuten weiter; die Messung kommt ein
        // Bild später
        let zeiten = |r: &mut Renderer, s: &mut Scene, entwurf: bool| {
            let mut v = Vec::new();
            r.set_shadow_draft(entwurf);
            for i in 0..40 {
                bild(r, s, sonne(6, 21, 9 * 60 + 5 * i), &cam);
                // Die Messung des ersten Bildes stammt noch vom Lauf davor
                let m = r.take_shadow_pass_ms();
                if i >= 1 {
                    v.extend(m);
                }
            }
            r.set_shadow_draft(false);
            // Der Renderer wählt nach der zuletzt gelesenen Messung (Test S6)
            let letzte = v.last().map_or(0.0, |m| m.1);
            v.sort_by(|a, b| a.1.total_cmp(&b.1));
            (v, letzte)
        };
        let mut bericht = String::new();
        for (n, s) in [("Würfel", &mut w), ("Haus RH-1", &mut h)] {
            let (v, letzte) = zeiten(&mut r, s, false);
            assert!(!v.is_empty(), "keine Messung");
            assert!(
                v.iter().all(|m| m.0 == sk_render::schatten::GROESSE),
                "{v:?}"
            );
            let median = v[v.len() / 2].1;
            bericht += &format!("{n}: {} mal 4096², Median {median:.1} ms\n", v.len());
            // Beim Ziehen: letzte volle Messung über 8 ms kleiner, sonst
            // volle Größe
            let (e, _) = zeiten(&mut r, s, true);
            let gross = if letzte > sk_render::schatten::ENTWURF_AB_MS {
                sk_render::schatten::ENTWURF
            } else {
                sk_render::schatten::GROESSE
            };
            // Beim Greifen gewählt, ohne Wechsel mitten im Zug
            assert!(
                !e.is_empty() && e.iter().all(|m| m.0 == gross),
                "{n}: {e:?}"
            );
            let m = e[e.len() / 2].1;
            bericht += &format!(
                "{n} beim Ziehen: {} mal {gross}², Median {m:.1} ms\n",
                e.len()
            );
        }
        eprintln!("{bericht}");
        std::fs::write(ziel.join("s6-tiefendurchgang.txt"), bericht).unwrap();
        assert_eq!(r.take_shadow_error(), None);
    }

    /// Eine Ansicht auf Papier wie in der App, mit dem Schatten der Wahl
    /// `vs` (S7); `dazu`: zusätzliches Netz (Probestück).
    fn ansicht(
        r: &mut Renderer,
        s: &mut Scene,
        v: ViewKind,
        vs: sk_model::ViewShade,
        sun: Sun,
        dazu: sk_render::MeshData,
    ) -> (Vec<u8>, Camera) {
        use crate::ansicht_schatten as asch;
        let theme = Theme::dark();
        s.set_theme(&theme);
        r.set_style(crate::style(&theme.env));
        r.set_looks(&s.table().looks_with(1.0, |_| 1.0));
        r.set_mesh(crate::MESH_MODEL, &s.mesh(v, None, &[]));
        let q = huelle(&dazu.faces);
        r.set_mesh(MESH_DAZU, &dazu);
        r.set_sun(None);
        let ort = *s.model().location();
        let papier = asch::licht(v, vs, &ort, sun).map(|d| {
            let (_, t) = s.table().pattern;
            let h = crate::settings::vorgaben::Vorgaben::WERK.h_linie;
            asch::papier(vs, d, [t[0], t[1], t[2]], theme.px_per_mm, 1.0, h)
        });
        assert!(papier.is_some() || !vs.on, "Licht");
        r.set_paper_shade(papier);
        let b = s.bounds().or(q);
        let cam = crate::fit_parallel(v, b, W as f64, H as f64);
        let mut view = cam.view(W, H);
        view.paper = Some(s.table().paper);
        view.patterns = crate::draw_table::pattern_mode(v, theme.env.patterns_3d);
        r.draw(W, H, 0, &view).unwrap();
        let px = r.read_pixels(W, H);
        r.set_paper_shade(None);
        r.set_mesh(MESH_DAZU, &Default::default());
        (px, cam)
    }

    fn huelle(faces: &[[f32; 9]]) -> Option<(Vec3, Vec3)> {
        let mut it = faces
            .iter()
            .map(|f| vec3(f[0] as f64, f[1] as f64, f[2] as f64));
        let a = it.next()?;
        Some(it.fold((a, a), |(lo, hi), p| {
            (
                vec3(lo.x.min(p.x), lo.y.min(p.y), lo.z.min(p.z)),
                vec3(hi.x.max(p.x), hi.y.max(p.y), hi.z.max(p.z)),
            )
        }))
    }

    /// Prüfbilder S7: RH-3 in den vier Ansichten (Fläche, vorne links),
    /// Vorne mit Schraffur und mit der Sonne vom 21.06. 15:00; dazu das
    /// Band unter 50 cm Überstand im Bild gemessen.
    #[test]
    #[ignore = "braucht einen X-Server (xvfb-run) und SKIZZEO_ISTBILDER"]
    fn gpu_istbilder_s7() {
        use sk_model::{ShadeLight, ViewShade};
        let Some(ziel) = std::env::var_os("SKIZZEO_ISTBILDER").map(std::path::PathBuf::from) else {
            return;
        };
        let k = sk_render::glx::kontext(W as i32, H as i32).expect("GLX-Kontext (DISPLAY?)");
        let theme = Theme::dark();
        let mut r = Renderer::new(k.gl, crate::style(&theme.env)).unwrap();
        let ab = |px: &[u8], n: &str| {
            std::fs::write(ziel.join(n), sk_paint::encode_png(W, H, px)).unwrap()
        };
        let mut h = haus3();
        let leer = sk_render::MeshData::default;
        let sun = sonne(6, 21, 15 * 60);
        for (v, n) in [
            (ViewKind::Front, "vorne"),
            (ViewKind::Back, "hinten"),
            (ViewKind::Left, "links"),
            (ViewKind::Right, "rechts"),
        ] {
            let (px, _) = ansicht(&mut r, &mut h, v, ViewShade::WERK, sun, leer());
            ab(&px, &format!("ist-s7-{n}-flaeche.png"));
        }
        let schraffur = ViewShade {
            hatch: true,
            ..ViewShade::WERK
        };
        let (px, _) = ansicht(&mut r, &mut h, ViewKind::Back, schraffur, sun, leer());
        ab(&px, "ist-s7-hinten-schraffur.png");
        let mut m = h.model().clone();
        let mut l = *m.location();
        l.north = Some(180.0);
        m.begin("Lage");
        m.set_location(l);
        m.commit();
        let mut hn = Scene::with_model(m);
        let mit_sonne = ViewShade {
            light: ShadeLight::Sun,
            ..ViewShade::WERK
        };
        let (px, _) = ansicht(&mut r, &mut hn, ViewKind::Back, mit_sonne, sun, leer());
        ab(&px, "ist-s7-hinten-sonne-0621-1500.png");

        // Wand mit 50 cm Überstand: Band bis 2,50 m, im Bild gemessen
        let q = |a: [f64; 3], b: [f64; 3]| {
            sv::quader_netz((vec3(a[0], a[1], a[2]), vec3(b[0], b[1], b[2])))
        };
        let mut stueck = q([0.0, 0.0, 0.0], [6000.0, 365.0, 3000.0]);
        let platte = q([1000.0, -500.0, 3000.0], [5000.0, 365.0, 3200.0]);
        stueck.faces.extend(platte.faces);
        stueck.edges.extend(platte.edges);
        let mut w = Scene::with_model(Model::with_seed(1));
        let (px, cam) = ansicht(
            &mut r,
            &mut w,
            ViewKind::Front,
            ViewShade::WERK,
            sun,
            stueck,
        );
        ab(&px, "ist-s7-ueberstand.png");
        let hell = |p: Vec3| {
            let (x, y) = cam.project(p, W as f64, H as f64).unwrap();
            let i = (y as usize * W as usize + x as usize) * 4;
            px[i] as f64 + px[i + 1] as f64 + px[i + 2] as f64
        };
        let papier = hell(vec3(3000.0, 0.0, 1500.0));
        let schatten = hell(vec3(3000.0, 0.0, 2750.0));
        assert!((schatten - papier).abs() > 30.0, "{papier} {schatten}");
        // Übergang von oben nach unten in 5-mm-Schritten
        let mut z = 2900.0;
        while (hell(vec3(3000.0, 0.0, z)) - schatten).abs() < (papier - schatten).abs() * 0.5 {
            z -= 5.0;
        }
        let mm_px = {
            let a = cam
                .project(vec3(0.0, 0.0, 0.0), W as f64, H as f64)
                .unwrap();
            let b = cam
                .project(vec3(0.0, 0.0, 1000.0), W as f64, H as f64)
                .unwrap();
            1000.0 / (a.1 - b.1).abs()
        };
        assert!(
            (z - 2500.0).abs() <= 2.0 * mm_px + 5.0,
            "Band bis {z} mm, {mm_px:.1} mm je Pixel"
        );
        std::fs::write(
            ziel.join("s7-band.txt"),
            format!(
                "Band unter 50 cm Überstand endet bei {z} mm (Soll 2500, {mm_px:.1} mm je Pixel)\n"
            ),
        )
        .unwrap();
        assert_eq!(r.take_shadow_error(), None);

        // Das offene Feld mit „vorne oben“ gewählt, Maus über „Sonne“
        let b = crate::ansicht_schatten::Bild {
            gestrichelt: false,
            entw: false,
            vs: ViewShade {
                light: ShadeLight::Top,
                ..ViewShade::WERK
            },
            offen: true,
            hover: Some(crate::ansicht_schatten::Teil::Licht(ShadeLight::Sun)),
            sonne_ok: true,
            zu_tief: true,
            eigen: true,
            rechts: 1300f32.to_bits(),
            scale: 1.5f32.to_bits(),
        };
        // Unter Linux ohne Windows-Schriften: Schrift aus SKIZZEO_SCHRIFT
        let mut fonts = sk_ui::widgets::Fonts::system();
        if let Some(f) = std::env::var_os("SKIZZEO_SCHRIFT")
            .and_then(|p| std::fs::read(p).ok())
            .and_then(sk_paint::font::Font::parse)
        {
            fonts.regular = Some(f);
        }
        let (c, _, _) = crate::ansicht_schatten::malen(&b, &fonts, &theme);
        std::fs::write(ziel.join("ist-s7-feld.png"), c.to_png()).unwrap();
    }

    /// Ist-Bilder S11 (Jörn 09.10. 14:10): RH-1 vorne, Teile unter dem
    /// Gelände ausgeblendet (ab Werk) und gestrichelt (2 mm / 1 mm).
    /// `xvfb-run -a env SKIZZEO_ISTBILDER=<ordner> cargo test -p skizzeo gpu_istbilder_s11 -- --ignored`
    #[test]
    #[ignore = "braucht einen X-Server (xvfb-run) und SKIZZEO_ISTBILDER"]
    fn gpu_istbilder_s11() {
        use sk_model::ViewShade;
        let Some(ziel) = std::env::var_os("SKIZZEO_ISTBILDER").map(std::path::PathBuf::from) else {
            return;
        };
        let k = sk_render::glx::kontext(W as i32, H as i32).expect("GLX-Kontext (DISPLAY?)");
        let theme = Theme::dark();
        let mut r = Renderer::new(k.gl, crate::style(&theme.env)).unwrap();
        let ab = |px: &[u8], n: &str| {
            std::fs::write(ziel.join(n), sk_paint::encode_png(W, H, px)).unwrap()
        };
        let mut h = haus();
        assert!(
            h.bounds().unwrap().0.z < -500.0,
            "Fundament unter dem Gelände"
        );
        let sun = sonne(6, 21, 15 * 60);
        let leer = sk_render::MeshData::default;
        let mm = theme.px_per_mm;
        for (modus, n) in [
            (None, "alles"),
            (Some(None), "ausgeblendet"),
            (Some(Some([2.0 * mm, mm])), "gestrichelt"),
        ] {
            r.set_below_ground(modus);
            let (px, _) = ansicht(
                &mut r,
                &mut h,
                ViewKind::Front,
                ViewShade::WERK,
                sun,
                leer(),
            );
            ab(&px, &format!("ist-s11-vorne-{n}.png"));
        }
        r.set_below_ground(None);
    }
    /// Ist-Bilder Flachdach Wand (Jörn 10.10.): Schnitte quer durch die
    /// Wand über der Dachterrasse (Fußpunkt Schaumglas unter dem
    /// Verblender) und durch den Vorsprung (Bekleidung unter der
    /// Untersichtdämmung, Verblender steht 3 cm über).
    /// `xvfb-run -a env SKIZZEO_ISTBILDER=<ordner> cargo test -p skizzeo gpu_istbilder_flachdach_wand -- --ignored`
    #[test]
    #[ignore = "braucht einen X-Server (xvfb-run) und SKIZZEO_ISTBILDER"]
    fn gpu_istbilder_flachdach_wand() {
        let Some(ziel) = std::env::var_os("SKIZZEO_ISTBILDER").map(std::path::PathBuf::from) else {
            return;
        };
        let k = sk_render::glx::kontext(W as i32, H as i32).expect("GLX-Kontext (DISPLAY?)");
        let theme = Theme::dark();
        let mut r = Renderer::new(k.gl, crate::style(&theme.env)).unwrap();
        // 10 × 8 m, OG auf der Rückseite 1,50 m zurückgesetzt (Dachterrasse),
        // vorn 30 cm vor (Untersicht): zweischalig (AW-49) und mit WDVS
        let faelle = [
            (
                sk_model::CAVITY_TYPE_GUID,
                (
                    Some("ist-flachdach-fusspunkt-schaumglas.png"),
                    "ist-flachdach-untersicht-bekleidung.png",
                ),
            ),
            (
                sk_model::ETICS_TYPE_GUID,
                (None, "ist-flachdach-untersicht-wdvs.png"),
            ),
        ];
        for (typ, bilder) in faelle {
            // 10 × 8 m, zweischalig (AW-49), OG auf der Rückseite 1,50 m
            // zurückgesetzt: dort Dachterrasse und Verblender darauf
            let mut m = Model::with_seed(102);
            let b = m.add_building(2);
            let pts = [
                vec3(0.0, 0.0, 0.0),
                vec3(0.0, 8000.0, 0.0),
                vec3(10000.0, 8000.0, 0.0),
                vec3(10000.0, 0.0, 0.0),
            ];
            let eg = m.build_from_polygon(b, &pts).unwrap();
            let og = m.runs_above(eg)[0];
            let t = m.type_by_guid(typ).unwrap();
            m.begin("Typ");
            assert!(m.set_run_type(eg, t));
            m.commit();
            // Norden 1,50 m zurück (Terrasse), Süden 30 cm vor (Untersicht)
            for (k, d) in [(1, -1500.0), (3, 300.0)] {
                let w = m.wall_at(og, k).unwrap();
                m.begin("Versatz");
                m.set_linked(w, false);
                assert!(m.set_offset(w, d).is_some());
                m.commit();
            }
            let mut h = Scene::with_model(m);
            h.set_theme(&theme);
            r.set_style(crate::style(&theme.env));
            r.set_looks(&h.table().looks_with(1.0, |_| 1.0));
            let chains: Vec<_> = h
                .model()
                .runs()
                .iter()
                .filter_map(|(id, _)| h.model().chain(id))
                .collect();
            let seg = |c: &sk_model::WallChain, k: usize| {
                let n = c.points.len();
                (c.points[k], c.points[(k + 1) % n])
            };
            // Wand mit Fußpunkt: ihr erstes Segment auf der Terrasse
            let fuss = chains.iter().find_map(|c| {
                let f = c.joints.facing_foot.as_ref()?;
                let k = (0..f.segs.len()).find(|&k| f.at(k))?;
                Some((seg(c, k), f.band))
            });
            assert_eq!(
                fuss.is_some(),
                bilder.0.is_some(),
                "Fußpunkt nur zweischalig"
            );
            // EG-Wand unter dem Vorsprung: Verblender (WDVS: Dämmung) bis unter
            // die Bekleidung
            let vor = chains
                .iter()
                .find_map(|c| {
                    let o = c.joints.overhang.as_ref()?;
                    let d = o.drip.as_ref()?;
                    let k = d.segs.iter().position(|x| *x)?;
                    Some((seg(c, k), (d.from, o.to)))
                })
                .expect("Bekleidung unter dem Vorsprung");
            let fuss = fuss.zip(bilder.0);
            for ((pts, band), name) in fuss.into_iter().chain([(vor, bilder.1)]) {
                let m = (pts.0 + pts.1) * 0.5;
                let d = (pts.1 - pts.0).normalized();
                // Blick entlang der Wand (Schnittebene quer dazu), Ausschnitt
                let yaw = d.y.atan2(d.x);
                let plane = (vec3(m.x, m.y, 0.0), -vec3(d.x, d.y, 0.0));
                let netz = h.mesh(ViewKind::Section, Some(plane), &[]);
                r.set_mesh(crate::MESH_MODEL, &netz);
                let q = vec3(-d.y, d.x, 0.0) * 700.0;
                let b = Some((
                    vec3(m.x, m.y, band.0 - 350.0) - q,
                    vec3(m.x, m.y, band.1 + 350.0) + q,
                ));
                let cam = crate::fit_parallel_dir(
                    (yaw, 0.0),
                    b,
                    W as f64,
                    W as f64,
                    H as f64,
                    W as f64 * 0.5,
                );
                let mut view = cam.view(W, H);
                view.paper = Some(h.table().paper);
                view.patterns =
                    crate::draw_table::pattern_mode(ViewKind::Section, theme.env.patterns_3d);
                r.draw(W, H, 0, &view).unwrap();
                std::fs::write(
                    ziel.join(name),
                    sk_paint::encode_png(W, H, &r.read_pixels(W, H)),
                )
                .unwrap();
            }
        }
    }
    /// Ist-Bilder Gelände (Jörn 10.10., Themen 1 und 4; Geländeplatte G0,
    /// Jörn 12.10.): RH-1 mit OK Sohle 0,40 über Gelände und 12 cm
    /// Perimeterdämmung, vorne ausgeblendet und gestrichelt, dazu 0,30
    /// unter Gelände gestrichelt und ein Schnitt; statt der Geländelinie das
    /// Band bzw. die Schnittfläche der Geländeplatte.
    /// `xvfb-run -a env SKIZZEO_ISTBILDER=<ordner> cargo test -p skizzeo gpu_istbilder_gelaende -- --ignored`
    #[test]
    #[ignore = "braucht einen X-Server (xvfb-run) und SKIZZEO_ISTBILDER"]
    fn gpu_istbilder_gelaende() {
        use crate::gelaende_platte as gp;
        use sk_model::ViewShade;
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
        let mut h = haus();
        let slab = h
            .model()
            .elements()
            .iter()
            .find(|(_, e)| matches!(e.kind, sk_model::ElementKind::GroundSlab(_)))
            .map(|(id, _)| id)
            .unwrap();
        assert!(h.edit_model("Perimeterdämmung", |m| m.set_slab_insulation(slab, 120.0)));
        assert!(h.edit_model("Gelände", |m| m.set_terrain_offset(400.0)));
        let sun = sonne(6, 21, 15 * 60);
        let leer = sk_render::MeshData::default;
        let mm = theme.px_per_mm;
        let linien = |h: &Scene, v: ViewKind, plane| {
            gp::linien(
                &h.model().terrain_plates(),
                v,
                plane,
                h.bounds(),
                h.table(),
                1.0,
            )
        };
        for (versatz, modus, n) in [
            (400.0, Some(None), "plus40-ausgeblendet"),
            (400.0, Some(Some([2.0 * mm, mm])), "plus40-gestrichelt"),
            (-300.0, Some(Some([2.0 * mm, mm])), "minus30-gestrichelt"),
        ] {
            h.edit_model("Gelände", |m| m.set_terrain_offset(versatz));
            r.set_terrain(h.model().terrain_z() as f32);
            r.set_below_ground(modus);
            r.set_helpers(&linien(&h, ViewKind::Front, None));
            let (px, _) = ansicht(
                &mut r,
                &mut h,
                ViewKind::Front,
                ViewShade::WERK,
                sun,
                leer(),
            );
            ab(&px, &format!("ist-gelaende-vorne-{n}.png"));
        }
        // Schnitt quer durch die Mitte: Platte, Dämmung, Schürze und die
        // Schnittfläche der Geländeplatte bis an die Wand
        r.set_below_ground(None);
        let (lo, hi) = h.bounds().unwrap();
        let c = (lo + hi) * 0.5;
        let plane = (vec3(c.x, c.y, 0.0), vec3(0.0, -1.0, 0.0));
        h.set_theme(&theme);
        r.set_style(crate::style(&theme.env));
        r.set_looks(&h.table().looks_with(1.0, |_| 1.0));
        for (versatz, n, b) in [
            (400.0, "ecke", None),
            (100.0, "ecke-plus10", None),
            (-300.0, "ecke-minus30", None),
            (
                -300.0,
                "ganz",
                Some((
                    vec3(lo.x - 4000.0, c.y, -3500.0),
                    vec3(hi.x + 4000.0, c.y, hi.z),
                )),
            ),
        ] {
            h.edit_model("Gelände", |m| m.set_terrain_offset(versatz));
            let z = h.model().terrain_z();
            r.set_terrain(z as f32);
            r.set_mesh(
                crate::MESH_MODEL,
                &h.mesh(ViewKind::Section, Some(plane), &[]),
            );
            let plates = h.model().terrain_plates();
            let keys = h.table().terrain;
            r.set_mesh(
                gp::MESH_PLATE,
                &gp::netz(&plates, ViewKind::Section, Some(plane), keys),
            );
            r.set_helpers(&linien(&h, ViewKind::Section, Some(plane)));
            // Ausschnitt: linke Ecke der Gründung
            let b = b.or(Some((
                vec3(lo.x - 3600.0, c.y, -1500.0),
                vec3(lo.x + 1800.0, c.y, 600.0),
            )));
            let cam = crate::fit_parallel(ViewKind::Section, b, W as f64, H as f64);
            let mut view = cam.view(W, H);
            view.paper = Some(h.table().paper);
            view.patterns =
                crate::draw_table::pattern_mode(ViewKind::Section, theme.env.patterns_3d);
            r.draw(W, H, 0, &view).unwrap();
            ab(
                &r.read_pixels(W, H),
                &format!("ist-gelaende-schnitt-{n}.png"),
            );
        }
        r.set_mesh(gp::MESH_PLATE, &leer());
        // Entscheidung B: ein zweites Haus daneben auf eigenem Gelände
        // (RH-1 +0,40, das neue −0,30), gestrichelt unter dem jeweiligen
        let x0 = hi.x + 4000.0;
        assert!(h.edit_model("Haus 2", |m| {
            let b = m.add_building(1);
            let pts = [
                vec3(x0, lo.y, 0.0),
                vec3(x0, lo.y + 8000.0, 0.0),
                vec3(x0 + 9000.0, lo.y + 8000.0, 0.0),
                vec3(x0 + 9000.0, lo.y, 0.0),
            ];
            m.build_from_polygon(b, &pts).is_some() && m.set_terrain_offset_of(b, -300.0)
        }));
        h.edit_model("Gelände", |m| m.set_terrain_offset(400.0));
        let patches = h.model().terrain_patches(gp::TERRAIN_MARGIN);
        assert_eq!(patches.len(), 2);
        assert_eq!(h.model().terrain_plates().len(), 2);
        r.set_terrain(h.model().terrain_z() as f32);
        r.set_terrain_boxes(&gp::terrain_boxes(&patches));
        r.set_below_ground(Some(Some([2.0 * mm, mm])));
        r.set_helpers(&linien(&h, ViewKind::Front, None));
        let (px, _) = ansicht(
            &mut r,
            &mut h,
            ViewKind::Front,
            ViewShade::WERK,
            sun,
            leer(),
        );
        ab(&px, "ist-gelaende-vorne-zwei-haeuser.png");
        r.set_terrain_boxes(&[]);
        r.set_helpers(&[]);
        r.set_below_ground(None);
        r.set_terrain(0.0);
    }

    /// Ist-Bilder Geländeplatte in 3D (Gelände G0, Jörn 12.10.): RH-1 auf
    /// der grünen Platte, 50 cm dick und 3 m umlaufend, am 21.06. um 15:00
    /// von Südwesten, nah an der Ecke und mit 0,80 m Dicke und 1,50 m
    /// Abstand ums Haus; dazu zwei Häuser auf verschiedener Höhe.
    /// `xvfb-run -a env SKIZZEO_ISTBILDER=<ordner> cargo test -p skizzeo gpu_istbilder_platte -- --ignored`
    #[test]
    #[ignore = "braucht einen X-Server (xvfb-run) und SKIZZEO_ISTBILDER"]
    fn gpu_istbilder_platte() {
        use crate::gelaende_platte as gp;
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
        let mut h = haus();
        let sun = sonne(6, 21, 15 * 60);
        let zeige = |r: &mut Renderer, h: &mut Scene, cam: &Camera, n: &str| {
            h.set_theme(&theme);
            let plates = h.model().terrain_plates();
            let keys = h.table().terrain;
            r.set_terrain(h.model().terrain_z() as f32);
            r.set_ground_z(plates.iter().map(|p| p.bottom as f32).reduce(f32::min));
            r.set_grass(keys[0]);
            r.set_mesh(
                gp::MESH_PLATE,
                &gp::netz(&plates, ViewKind::Persp, None, keys),
            );
            let px = bild(r, h, sun, cam);
            ab(&px, n);
        };
        let cam = blick(&h, vec3(-1.0, -1.3, 0.8), 32000.0);
        zeige(&mut r, &mut h, &cam, "ist-platte-3d-suedwest.png");
        let (lo, _) = h.bounds().unwrap();
        let ecke = vec3(lo.x, lo.y, 0.0);
        let cam = Camera::looking_at(ecke + vec3(-5500.0, -7000.0, 3200.0), ecke, 45.0);
        zeige(&mut r, &mut h, &cam, "ist-platte-3d-ecke.png");
        assert!(h.edit_model("Geländeplatte", |m| m.set_plate_settings(
            sk_model::PlateSettings {
                thick: 800.0,
                margin: 1500.0
            }
        )));
        zeige(&mut r, &mut h, &cam, "ist-platte-3d-ecke-80-150.png");
        let (lo, hi) = h.bounds().unwrap();
        let x0 = hi.x + 4000.0;
        assert!(h.edit_model("Haus 2", |m| {
            let b = m.add_building(1);
            let pts = [
                vec3(x0, lo.y, 0.0),
                vec3(x0, lo.y + 8000.0, 0.0),
                vec3(x0 + 9000.0, lo.y + 8000.0, 0.0),
                vec3(x0 + 9000.0, lo.y, 0.0),
            ];
            m.build_from_polygon(b, &pts).is_some() && m.set_terrain_offset_of(b, -600.0)
        }));
        let cam = blick(&h, vec3(-1.0, -1.6, 0.9), 42000.0);
        zeige(&mut r, &mut h, &cam, "ist-platte-3d-zwei-haeuser.png");
        r.set_mesh(gp::MESH_PLATE, &Default::default());
        r.set_ground_z(None);
        r.set_grass(0);
    }

    /// Ist-Bilder Gelände G1: RH-1 auf dem Höhenaufmaß „Am Lindenhof 7“
    /// (ohne Lage: das Grundstück mittig unter dem Haus), in 3D von
    /// Südwesten mit „Umgebung zeigen“ ganz, blass und nicht; Grundriss mit
    /// Höhenlinien, Punkten, Grenze und Höhenzahlen (G3t), ganz und nah ums
    /// Haus; Schnitt und Vorne mit der Geländelinie.
    /// `xvfb-run -a env SKIZZEO_ISTBILDER=<ordner> cargo test -p skizzeo gpu_istbilder_aufmass -- --ignored`
    #[test]
    #[ignore = "braucht einen X-Server (xvfb-run) und SKIZZEO_ISTBILDER"]
    fn gpu_istbilder_aufmass() {
        use crate::gelaende_netz::{self as gn, Gelaende};
        use crate::gelaende_platte as gp;
        use sk_model::Show;
        let Some(ziel) = std::env::var_os("SKIZZEO_ISTBILDER").map(std::path::PathBuf::from) else {
            return;
        };
        let k = sk_render::glx::kontext(W as i32, H as i32).expect("GLX-Kontext (DISPLAY?)");
        let theme = Theme::dark();
        let mut r = Renderer::new(k.gl, crate::style(&theme.env)).unwrap();
        let ab = |px: &[u8], n: &str| {
            std::fs::write(ziel.join(n), sk_paint::encode_png(W, H, px)).unwrap()
        };
        let csv =
            include_bytes!("../../crates/sk-gelaende/tests/daten/hoehenaufmass-lindenhof-7.csv");
        let (t, ..) = sk_model::Terrain::lesen(csv, "aufmass.csv", "2026-10-12", 32).unwrap();
        let mut h = haus();
        assert!(h.edit_model("Höhenaufmaß", |m| m.import_terrain(t)));
        let sun = sonne(6, 21, 15 * 60);
        for (show, n) in [
            (Show::All, "alles"),
            (Show::Fade, "blass"),
            (Show::Parcel, "grundstueck"),
        ] {
            h.edit_model("Zuschnitt", |m| m.set_terrain_show(show));
            h.set_theme(&theme);
            let g = Gelaende::aus(h.model()).unwrap();
            let keys = h.table().terrain;
            r.set_ground_z(Some(g.unten as f32));
            r.set_grass(keys[0]);
            let netz = crate::scene::mesh_of(&gn::koerper(&g, keys));
            r.set_mesh(gp::MESH_PLATE, &netz);
            let cam = blick(&h, vec3(-1.0, -1.3, 0.8), 52_000.0);
            let px = bild(&mut r, &mut h, sun, &cam);
            ab(&px, &format!("ist-aufmass-3d-{n}.png"));
            // mit Hilfsnetz (I4b, Jörns §4.3a)
            let netz = crate::scene::mesh_of(&gn::koerper_mit_netz(&g, keys, true));
            r.set_mesh(gp::MESH_PLATE, &netz);
            let px = bild(&mut r, &mut h, sun, &cam);
            ab(&px, &format!("ist-aufmass-3d-{n}-netz.png"));
            // dazu die Nadeln und am Punkt vor dem Haus Höhenpfeil und
            // Quadrat (I4c)
            if show == Show::All {
                use crate::gelaende_flaeche_app::{netz, punkte};
                let spots = h.model().terrain_spots();
                let (lo, _) = h.bounds().unwrap();
                let vorn = spots
                    .iter()
                    .filter(|s| s.p[0] < lo.x && s.p[1] < lo.y)
                    .max_by(|a, b| (a.p[0] + a.p[1]).total_cmp(&(b.p[0] + b.p[1])))
                    .unwrap();
                let sel = theme.interact.select;
                let haeuser = h.model().terrain_patches(0.0);
                let im_haus = |p: [f64; 3]| {
                    haeuser.iter().any(|(b, _)| {
                        (b[0]..=b[2]).contains(&p[0]) && (b[1]..=b[3]).contains(&p[1])
                    })
                };
                let a = vec3(vorn.p[0], vorn.p[1], vorn.p[2]);
                let nah =
                    Camera::looking_at(a + vec3(-1.0, -1.3, 0.8).normalized() * 14_000.0, a, 45.0);
                for (cam, n) in [(&cam, "gizmo"), (&nah, "gizmo-nah")] {
                    let mut l = Vec::new();
                    let (lo, hi) = h.bounds().unwrap();
                    let quader: Vec<_> = haeuser
                        .iter()
                        .map(|(b, _)| (vec3(b[0], b[1], lo.z), vec3(b[2], b[3], hi.z)))
                        .collect();
                    let dahinter = |p: [f64; 3]| {
                        let p = vec3(p[0], p[1], p[2]);
                        quader
                            .iter()
                            .any(|&(lo, hi)| netz::verdeckt(cam.eye, p, lo, hi))
                    };
                    for s in spots.iter().filter(|s| !im_haus(s.p) && !dahinter(s.p)) {
                        let w = s.i == vorn.i;
                        netz::nadel(&mut l, s, w.then_some(sel), !w, 1.0);
                    }
                    let v = (W as f64, H as f64, 1.0);
                    l.extend(punkte::gizmo_linien(cam, a, v, (sel, sel), 1.0));
                    r.set_helpers(&l);
                    let px = bild(&mut r, &mut h, sun, cam);
                    ab(&px, &format!("ist-aufmass-3d-{n}.png"));
                }
                r.set_helpers(&[]);
            }
        }
        r.set_mesh(gp::MESH_PLATE, &Default::default());
        // Grundriss, Schnitt und Vorne auf Papier
        h.edit_model("Zuschnitt", |m| m.set_terrain_show(Show::Fade));
        let g = Gelaende::aus(h.model()).unwrap();
        let (lo, hi) = g.rahmen();
        let (hlo, hhi) = h.bounds().unwrap();
        let c = (hlo + hhi) * 0.5;
        let plane = (vec3(c.x, c.y, 0.0), vec3(0.0, -1.0, 0.0));
        // „unter Gelände“ aus dem Raster des Netzes (Geometrie zu #129)
        r.set_terrain_grid(h.terrain_grid().as_deref());
        for (v, n) in [
            (ViewKind::Plan, "grundriss-blass"),
            (ViewKind::Plan, "grundriss-nah"),
            (ViewKind::Section, "schnitt"),
            (ViewKind::Front, "vorne"),
            (ViewKind::Right, "rechts-unter-gelaende"),
        ] {
            let unter = v == ViewKind::Right;
            r.set_below_ground(unter.then_some(Some([6.0, 4.0])));
            h.set_theme(&theme);
            r.set_style(crate::style(&theme.env));
            r.set_looks(&h.table().looks_with(1.0, |_| 1.0));
            let p = (v == ViewKind::Section).then_some(plane);
            r.set_mesh(crate::MESH_MODEL, &h.mesh(v, p, &[]));
            let keys = h.table().terrain;
            let kappe = match p {
                Some((p0, nn)) => {
                    let f = sk_model::solid::SectionFrame::new(p0, nn);
                    crate::scene::mesh_of(&gn::kappen(&g, &f, p0, keys[1]))
                }
                None => Default::default(),
            };
            r.set_mesh(gp::MESH_PLATE, &kappe);
            let b = if n == "grundriss-nah" {
                // Haus mit 6 m ringsum, etwa 1:100 am Bildschirm
                let d = vec3(6000.0, 6000.0, 0.0);
                Some((hlo - d, hhi + d))
            } else {
                Some((vec3(lo.x, lo.y, lo.z), vec3(hi.x, hi.y, hhi.z)))
            };
            let cam = crate::fit_parallel(v, b, W as f64, H as f64);
            let mm_px = cam.ortho.map_or(10.0, |half| 2.0 * half / H as f64);
            let f = gn::fassade(&g, v, h.bounds());
            r.set_helpers(&gn::linien(&g, v, p, f, h.table(), 1.0, mm_px));
            // Höhenzahlen im Grundriss (G3t), in der Schrifthöhe der Maße
            let zahlen = crate::wall_edit::mass_treffer::tests::schrift()
                .filter(|_| v == ViewKind::Plan)
                .map(|f| {
                    let stil = h.model().massstil();
                    let s = sk_plan::schrift::Schrift::new(f);
                    gn::zahlen_tinte(&g, &s, stil.am_bau(stil.schrift_mm), h.table())
                })
                .unwrap_or_default();
            r.set_ink_teile(&[(v as u64 + 1, zahlen)]);
            let mut view = cam.view(W, H);
            view.paper = Some(h.table().paper);
            view.patterns = crate::draw_table::pattern_mode(v, theme.env.patterns_3d);
            r.draw(W, H, 0, &view).unwrap();
            ab(&r.read_pixels(W, H), &format!("ist-aufmass-{n}.png"));
        }
        r.set_ink_teile::<Vec<sk_render::Ink>>(&[]);
        r.set_helpers(&[]);
        r.set_mesh(gp::MESH_PLATE, &Default::default());
        r.set_ground_z(None);
        r.set_grass(0);
        r.set_below_ground(None);
        r.set_terrain_grid(None);
    }

    /// Ist-Bilder Gelände G3c: RH-1 auf dem Höhenaufmaß „Am Lindenhof 7“
    /// mit zwei Geländeflächen, einer Terrasse 0,60 m über dem Gelände vor
    /// dem Haus und einem Garten 0,80 m unter dem Gelände daneben, je mit
    /// Böschung 1:1,5. 3D von Südwesten, Grundriss mit Umriss und
    /// Schraffen, Schnitt durch beide Flächen mit dem Urgelände
    /// gestrichelt. `ist-flaeche-{3d,grundriss,schnitt}.png`.
    /// `xvfb-run -a env SKIZZEO_ISTBILDER=<ordner> cargo test -p skizzeo gpu_istbilder_gelaendeflaeche -- --ignored`
    #[test]
    #[ignore = "braucht einen X-Server (xvfb-run) und SKIZZEO_ISTBILDER"]
    fn gpu_istbilder_gelaendeflaeche() {
        use crate::gelaende_netz::{self as gn, Gelaende};
        use crate::gelaende_platte as gp;
        use sk_model::{Show, TerrainPatch};
        let Some(ziel) = std::env::var_os("SKIZZEO_ISTBILDER").map(std::path::PathBuf::from) else {
            return;
        };
        let k = sk_render::glx::kontext(W as i32, H as i32).expect("GLX-Kontext (DISPLAY?)");
        let theme = Theme::dark();
        let mut r = Renderer::new(k.gl, crate::style(&theme.env)).unwrap();
        let ab = |px: &[u8], n: &str| {
            std::fs::write(ziel.join(n), sk_paint::encode_png(W, H, px)).unwrap()
        };
        let csv =
            include_bytes!("../../crates/sk-gelaende/tests/daten/hoehenaufmass-lindenhof-7.csv");
        let (t, ..) = sk_model::Terrain::lesen(csv, "aufmass.csv", "2026-10-12", 32).unwrap();
        let mut h = haus();
        assert!(h.edit_model("Höhenaufmaß", |m| m.import_terrain(t)));
        h.edit_model("Zuschnitt", |m| m.set_terrain_show(Show::All));
        let (hlo, hhi) = h.bounds().unwrap();
        let rechteck =
            |x0: f64, y0: f64, x1: f64, y1: f64| vec![[x0, y0], [x1, y0], [x1, y1], [x0, y1]];
        let terrasse = rechteck(hlo.x, hlo.y - 5_000.0, hhi.x, hlo.y - 1_000.0);
        let garten = rechteck(hhi.x + 3_000.0, hlo.y, hhi.x + 9_000.0, hhi.y);
        let z = |m: &sk_model::Model, r: &[[f64; 2]]| {
            let c = [(r[0][0] + r[2][0]) / 2.0, (r[0][1] + r[2][1]) / 2.0];
            m.terrain_z_at(None, c[0], c[1])
        };
        assert!(h.edit_model("Flächen", |m| {
            let (zt, zg) = (z(m, &terrasse) + 600.0, z(m, &garten) - 800.0);
            m.add_site_patch(TerrainPatch::flat(terrasse.clone(), zt))
                .is_some()
                && m.add_site_patch(TerrainPatch::flat(garten.clone(), zg))
                    .is_some()
        }));
        h.set_theme(&theme);
        let g = Gelaende::aus(h.model()).unwrap();
        let keys = h.table().terrain;
        r.set_ground_z(Some(g.unten as f32));
        r.set_grass(keys[0]);
        r.set_mesh(
            gp::MESH_PLATE,
            &crate::scene::mesh_of(&gn::koerper(&g, keys)),
        );
        let sun = sonne(6, 21, 15 * 60);
        let cam = blick(&h, vec3(-1.0, -1.3, 0.8), 40_000.0);
        let px = bild(&mut r, &mut h, sun, &cam);
        ab(&px, "ist-flaeche-3d.png");
        r.set_mesh(gp::MESH_PLATE, &Default::default());
        // Grundriss und Schnitt durch beide Flächen auf Papier, um Haus und
        // Flächen
        let y = (hlo.y + hhi.y) / 2.0 - 3_000.0;
        let plane = (vec3(hhi.x, y, 0.0), vec3(0.0, -1.0, 0.0));
        let b = Some((
            vec3(hlo.x - 6_000.0, hlo.y - 9_000.0, g.unten),
            vec3(hhi.x + 13_000.0, hhi.y + 4_000.0, hhi.z),
        ));
        for (v, n) in [
            (ViewKind::Plan, "grundriss"),
            (ViewKind::Section, "schnitt"),
        ] {
            h.set_theme(&theme);
            r.set_style(crate::style(&theme.env));
            r.set_looks(&h.table().looks_with(1.0, |_| 1.0));
            let p = (v == ViewKind::Section).then_some(plane);
            r.set_mesh(crate::MESH_MODEL, &h.mesh(v, p, &[]));
            let kappe = match p {
                Some((p0, nn)) => {
                    let f = sk_model::solid::SectionFrame::new(p0, nn);
                    crate::scene::mesh_of(&gn::kappen(&g, &f, p0, keys[1]))
                }
                None => Default::default(),
            };
            r.set_mesh(gp::MESH_PLATE, &kappe);
            let cam = crate::fit_parallel(v, b, W as f64, H as f64);
            let mm_px = cam.ortho.map_or(10.0, |half| 2.0 * half / H as f64);
            r.set_helpers(&gn::linien(&g, v, p, None, h.table(), 1.0, mm_px));
            let mut view = cam.view(W, H);
            view.paper = Some(h.table().paper);
            view.patterns = crate::draw_table::pattern_mode(v, theme.env.patterns_3d);
            r.draw(W, H, 0, &view).unwrap();
            ab(&r.read_pixels(W, H), &format!("ist-flaeche-{n}.png"));
        }
        r.set_helpers(&[]);
        r.set_mesh(gp::MESH_PLATE, &Default::default());
        r.set_ground_z(None);
        r.set_grass(0);
    }

    /// Ist-Bilder G1c: der Beispiel-Lageplan übernommen (eben, in Form des
    /// Grundstücks, Unterlage im Grundriss), dann das Aufmaß über die
    /// Grenzpunkte eingepasst. `ist-lageplan-{3d,grundriss}.png`,
    /// `ist-lageplan-hoehen-{3d,grundriss}.png`.
    #[test]
    #[ignore = "braucht einen X-Server (xvfb-run) und SKIZZEO_ISTBILDER"]
    fn gpu_istbilder_lageplan() {
        use crate::gelaende_netz::{self as gn, Gelaende};
        use crate::gelaende_platte as gp;
        let Some(ziel) = std::env::var_os("SKIZZEO_ISTBILDER").map(std::path::PathBuf::from) else {
            return;
        };
        let k = sk_render::glx::kontext(W as i32, H as i32).expect("GLX-Kontext (DISPLAY?)");
        let theme = Theme::dark();
        let mut r = Renderer::new(k.gl, crate::style(&theme.env)).unwrap();
        let ab = |px: &[u8], n: &str| {
            std::fs::write(ziel.join(n), sk_paint::encode_png(W, H, px)).unwrap()
        };
        let pdf = include_bytes!("../../crates/sk-gelaende/tests/daten/lageplan-lindenhof-7.pdf");
        let csv =
            include_bytes!("../../crates/sk-gelaende/tests/daten/hoehenaufmass-lindenhof-7.csv");
        let plan = sk_model::SitePlan::read(pdf, 32).unwrap();
        let mut h = haus();
        assert!(h.edit_model("Lageplan", |m| m.import_parcel(&plan, "lageplan.pdf", None)));
        let sun = sonne(6, 21, 15 * 60);
        for stufe in ["", "-hoehen"] {
            if !stufe.is_empty() {
                let (t, ..) =
                    sk_model::Terrain::lesen(csv, "aufmass.csv", "2026-10-12", 32).unwrap();
                let (fit, _) = plan.fit_to(&t.points).unwrap();
                assert!(h.edit_model("Höhen", |m| m.import_terrain_fitted(t, fit)));
            }
            h.set_theme(&theme);
            let m = h.model();
            let g = Gelaende::aus(m)
                .unwrap()
                .mit_unterlage(&m.underlay_lines(&plan));
            let keys = h.table().terrain;
            r.set_ground_z(Some(g.unten as f32));
            r.set_grass(keys[0]);
            r.set_mesh(
                gp::MESH_PLATE,
                &crate::scene::mesh_of(&gn::koerper(&g, keys)),
            );
            let cam = blick(&h, vec3(-1.0, -1.3, 0.8), 52_000.0);
            let px = bild(&mut r, &mut h, sun, &cam);
            ab(&px, &format!("ist-lageplan{stufe}-3d.png"));
            r.set_mesh(gp::MESH_PLATE, &Default::default());
            // Grundriss auf Papier mit der Unterlage
            let v = ViewKind::Plan;
            h.set_theme(&theme);
            r.set_style(crate::style(&theme.env));
            r.set_looks(&h.table().looks_with(1.0, |_| 1.0));
            r.set_mesh(crate::MESH_MODEL, &h.mesh(v, None, &[]));
            let (lo, hi) = g.rahmen();
            let b = Some((lo, vec3(hi.x, hi.y, h.bounds().unwrap().1.z)));
            let cam = crate::fit_parallel(v, b, W as f64, H as f64);
            let mm_px = cam.ortho.map_or(10.0, |half| 2.0 * half / H as f64);
            r.set_helpers(&gn::linien(&g, v, None, None, h.table(), 1.0, mm_px));
            let mut view = cam.view(W, H);
            view.paper = Some(h.table().paper);
            r.draw(W, H, 0, &view).unwrap();
            ab(
                &r.read_pixels(W, H),
                &format!("ist-lageplan{stufe}-grundriss.png"),
            );
            r.set_helpers(&[]);
        }
        r.set_ground_z(None);
        r.set_grass(0);
    }

    /// Ist-Bilder E4: die fünf Beispiel-Erweiterungen vor RH-1 in 3D, im
    /// Grundriss des EG und in „Vorne“; dazu die Treppe allein in „Vorne“
    /// (schaut in +Y) zum Vergleich mit der Werkbank.
    /// `xvfb-run -a env SKIZZEO_ISTBILDER=<ordner> cargo test -p skizzeo gpu_istbilder_e4 -- --ignored`
    #[test]
    #[ignore = "braucht einen X-Server (xvfb-run) und SKIZZEO_ISTBILDER"]
    fn gpu_istbilder_e4() {
        use sk_model::erweiterung::{ExtDef, ExtPart};
        use sk_model::ViewShade;
        let Some(ziel) = std::env::var_os("SKIZZEO_ISTBILDER").map(std::path::PathBuf::from) else {
            return;
        };
        let k = sk_render::glx::kontext(W as i32, H as i32).expect("GLX-Kontext (DISPLAY?)");
        let theme = Theme::dark();
        let mut r = Renderer::new(k.gl, crate::style(&theme.env)).unwrap();
        let ab = |px: &[u8], n: &str| {
            std::fs::write(ziel.join(n), sk_paint::encode_png(W, H, px)).unwrap()
        };
        let beispiele = [
            include_str!("../../crates/sk-szb/beispiele/werk.bodenplatte.szb"),
            include_str!("../../crates/sk-szb/beispiele/werk.stabgelaender.szb"),
            include_str!("../../crates/sk-szb/beispiele/werk.streifenfundament.szb"),
            include_str!("../../crates/sk-szb/beispiele/werk.stuetze.szb"),
            include_str!("../../crates/sk-szb/beispiele/werk.treppe.szb"),
        ];
        let setzen = |s: &mut Scene, texte: &[&str], lo: Vec3| {
            let eg = s.active_storey();
            for (i, t) in texte.iter().enumerate() {
                let d = ExtDef::lesen(t).unwrap();
                let p = ExtPart::new(&d, [lo.x + 6000.0 * i as f64, lo.y - 5000.0]);
                assert!(s.edit_model("Einsetzen", |m| {
                    m.put_ext_def(d).is_ok() && m.add_ext(eg, p).is_ok()
                }));
            }
        };
        let sun = sonne(6, 21, 15 * 60);
        let leer = sk_render::MeshData::default;
        let mut h = haus();
        let lo = h.bounds().unwrap().0;
        setzen(&mut h, &beispiele, lo);
        let cam = blick(&h, vec3(-1.0, -1.6, 0.9), 32_000.0);
        let px = bild(&mut r, &mut h, sun, &cam);
        ab(&px, "ist-e4-3d.png");
        let ohne = ViewShade {
            on: false,
            ..ViewShade::WERK
        };
        for (v, vs, n) in [
            (ViewKind::Plan, ohne, "grundriss"),
            (ViewKind::Front, ViewShade::WERK, "vorne"),
        ] {
            let (px, _) = ansicht(&mut r, &mut h, v, vs, sun, leer());
            ab(&px, &format!("ist-e4-{n}.png"));
        }
        let mut m = Model::new();
        m.add_building(1);
        let mut t = Scene::with_model(m);
        setzen(&mut t, &beispiele[4..], vec3(0.0, 5000.0, 0.0));
        let (px, _) = ansicht(
            &mut r,
            &mut t,
            ViewKind::Front,
            ViewShade::WERK,
            sun,
            leer(),
        );
        ab(&px, "ist-e4-treppe-vorne.png");
    }

    /// Ist-Bilder Dachentwässerung (Jörn 12.10.): Prüfhaus mit Flachdach in
    /// 3D und vorne, je mit Regenfallrohren und mit dem Häkchen
    /// „Dachentwässerung ausblenden“, dazu die offenen Felder der Zahnräder
    /// in 3D, im Grundriss und vorne.
    /// `xvfb-run -a env SKIZZEO_ISTBILDER=<ordner> SKIZZEO_SCHRIFT=<ttf> cargo test -p skizzeo gpu_istbilder_entwaesserung -- --ignored`
    #[test]
    #[ignore = "braucht einen X-Server (xvfb-run) und SKIZZEO_ISTBILDER"]
    fn gpu_istbilder_entwaesserung() {
        use crate::{ansicht_schatten as asch, grundriss_schnitt as gs, sicht_zahnrad as sz};
        let Some(ziel) = std::env::var_os("SKIZZEO_ISTBILDER").map(std::path::PathBuf::from) else {
            return;
        };
        let k = sk_render::glx::kontext(W as i32, H as i32).expect("GLX-Kontext (DISPLAY?)");
        let theme = Theme::dark();
        let mut r = Renderer::new(k.gl, crate::style(&theme.env)).unwrap();
        let ab = |px: &[u8], n: &str| {
            std::fs::write(ziel.join(n), sk_paint::encode_png(W, H, px)).unwrap()
        };
        let mut h = crate::scene::sicht::tests::haus();
        let cam = blick(&h, vec3(0.8, -1.0, 0.45), 21000.0);
        let sun = sonne(6, 21, 15 * 60);
        for (aus, n) in [(false, "zeigen"), (true, "ausgeblendet")] {
            h.set_drain_hidden(ViewKind::Persp, aus);
            h.set_drain_hidden(ViewKind::Front, aus);
            h.skip_vis_animation();
            ab(
                &bild(&mut r, &mut h, sun, &cam),
                &format!("ist-entw-3d-{n}.png"),
            );
            let leer = sk_render::MeshData::default();
            let (px, _) = ansicht(
                &mut r,
                &mut h,
                ViewKind::Front,
                sk_model::ViewShade::WERK,
                sun,
                leer,
            );
            ab(&px, &format!("ist-entw-vorne-{n}.png"));
        }
        // Die Felder, Maus über dem Häkchen; Schrift aus SKIZZEO_SCHRIFT
        let mut fonts = sk_ui::widgets::Fonts::system();
        if let Some(f) = std::env::var_os("SKIZZEO_SCHRIFT")
            .and_then(|p| std::fs::read(p).ok())
            .and_then(sk_paint::font::Font::parse)
        {
            fonts.regular = Some(f);
        }
        let (rechts, scale) = (1300f32.to_bits(), 1.5f32.to_bits());
        let b = sz::Bild {
            an: true,
            ausbau: false,
            offen: true,
            hover: Some(sz::Teil::Haken),
            mass: Some((true, false)),
            rechts,
            scale,
        };
        let (c, _, _) = sz::malen(&b, &fonts, &theme);
        std::fs::write(ziel.join("ist-entw-feld-3d.png"), c.to_png()).unwrap();
        let b = gs::Bild {
            mm: sk_model::PLAN_CUT.to_bits(),
            eigen: false,
            offen: true,
            hover: None,
            eingabe: None,
            falsch: false,
            entw: false,
            ausbau: true,
            mass: (true, true),
            rechts,
            scale,
        };
        let (c, _, _) = gs::malen(&b, &fonts, &theme);
        std::fs::write(ziel.join("ist-entw-feld-grundriss.png"), c.to_png()).unwrap();
        let b = asch::Bild {
            vs: sk_model::ViewShade::WERK,
            gestrichelt: false,
            entw: true,
            offen: true,
            hover: None,
            sonne_ok: true,
            zu_tief: false,
            eigen: false,
            rechts,
            scale,
        };
        let (c, _, _) = asch::malen(&b, &fonts, &theme);
        std::fs::write(ziel.join("ist-entw-feld-vorne.png"), c.to_png()).unwrap();
    }

    /// Review #137 B4: Geländeplatte und Ausbau haben je eigene Ablagen. In
    /// der Reihenfolge der App (erst der Ausbau, dann die Platte, im
    /// nächsten Bild nur die Platte) bleiben beide zu sehen.
    #[test]
    #[ignore = "braucht einen X-Server (xvfb-run)"]
    fn gpu_platte_und_ausbau() {
        use crate::ausbau_app::MESH_AUSBAU;
        use crate::gelaende_platte::MESH_PLATE;
        let k = sk_render::glx::kontext(W as i32, H as i32).expect("GLX-Kontext (DISPLAY?)");
        let theme = Theme::dark();
        let mut r = Renderer::new(k.gl, crate::style(&theme.env)).unwrap();
        let platte = sv::quader_netz((vec3(0.0, 0.0, 0.0), vec3(2000.0, 2000.0, 100.0)));
        let ausbau = sv::quader_netz((vec3(4000.0, 0.0, 0.0), vec3(6000.0, 2000.0, 100.0)));
        r.set_mesh(MESH_AUSBAU, &ausbau);
        r.set_mesh(MESH_PLATE, &platte);
        r.set_mesh(MESH_PLATE, &platte);
        let b = Some((vec3(-500.0, -500.0, 0.0), vec3(6500.0, 2500.0, 100.0)));
        let cam = crate::fit_parallel(ViewKind::Plan, b, W as f64, H as f64);
        let view = cam.view(W, H);
        r.draw(W, H, 0, &view).unwrap();
        let px = r.read_pixels(W, H);
        // Bildpunkt über dem Punkt `p` des Grundrisses gegen den Hintergrund
        let farbe = |x: f64, y: f64| {
            let (u, v) = cam.project(vec3(x, y, 100.0), W as f64, H as f64).unwrap();
            let i = 4 * (v as usize * W as usize + u as usize);
            px[i..i + 3].to_vec()
        };
        let grund = farbe(3000.0, 1000.0);
        assert_ne!(farbe(1000.0, 1000.0), grund, "Platte fehlt");
        assert_ne!(farbe(5000.0, 1000.0), grund, "Ausbau fehlt");
    }
}
