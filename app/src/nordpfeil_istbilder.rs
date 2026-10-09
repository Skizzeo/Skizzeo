//! Ist-Bilder zum Nordpfeil (Sonnenstand S2) ohne Grafikkarte: Die Szene
//! wird mit dem Pinsel der Oberfläche gezeichnet, Flächen von hinten nach
//! vorn mit fester Beleuchtung, darüber das Bild des Pfeils wie in der App. Das Bild zeigt Lage, Größe und Gestalt, nicht die Schattierung
//! der App.
//!
//! `SKIZZEO_ISTBILDER=<ordner> cargo test -p skizzeo istbilder_s2 -- --ignored`

use super::*;
use crate::scene::Scene;
use crate::ui::ViewKind;
use sk_model::{szo, GuidGen, Model};
use sk_paint::{Canvas, Path, Rgba};
use sk_ui::theme::Theme;
use sk_ui::widgets::Fonts;
use std::f64::consts::FRAC_PI_2;
use std::path::PathBuf;

const W: usize = 1600;
const H: usize = 1000;
/// Bildschirmskalierung der Bilder (Strichbreite, Pille).
const S: f32 = 2.0;
const PAPIER: Rgba = Rgba(245, 244, 239, 255);
const TINTE: [f32; 4] = [0.12, 0.12, 0.12, 1.0];
/// Wie `interact.drag` beim Ziehen.
const HEISS: [f32; 4] = [0.95, 0.55, 0.1, 1.0];

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

/// Linie im Bild, `None` hinter der Kamera.
fn strich(c: &mut Canvas, cam: &Camera, a: Vec3, b: Vec3, breite: f32, col: Rgba) {
    let (w, h) = (W as f64, H as f64);
    let (Some(p), Some(q)) = (cam.project(a, w, h), cam.project(b, w, h)) else {
        return;
    };
    let (p, q) = ((p.0 as f32, p.1 as f32), (q.0 as f32, q.1 as f32));
    let mut path = Path::new();
    path.segment(p, q, breite);
    // Runde Enden wie in der App
    let r = breite * 0.5;
    for e in [p, q] {
        path.rounded_rect(e.0 - r, e.1 - r, breite, breite, r);
    }
    c.fill(&path, col);
}

/// Dreieck im Bild: Abstand zur Kamera, Ecken, Helligkeit.
type Dreieck = (f64, [(f32, f32); 3], f64);

/// Die Szene: im Grundriss die Schnittflächen der Wände, in 3D
/// die Flächen mit fester Beleuchtung; dazu ein Raster am Boden (1 m).
fn szene(s: &mut Scene, cam: &Camera, view: ViewKind) -> Canvas {
    let licht = vec3(-0.35, -0.55, 0.75).normalized();
    szene_mit(s, cam, view, licht, None)
}

/// Wie [`szene`] mit Lichtrichtung `licht` und einem Netz dazu (der
/// Würfel des Sonnenstands).
fn szene_mit(
    s: &mut Scene,
    cam: &Camera,
    view: ViewKind,
    licht: Vec3,
    dazu: Option<&sk_render::MeshData>,
) -> Canvas {
    let (w, h) = (W as f64, H as f64);
    let mut c = Canvas::new(W, H);
    c.clear(PAPIER);
    for i in -20..=30 {
        let v = i as f64 * 1000.0;
        let col = Rgba(222, 220, 212, 255);
        strich(
            &mut c,
            cam,
            vec3(v, -20000.0, 0.0),
            vec3(v, 30000.0, 0.0),
            1.0,
            col,
        );
        strich(
            &mut c,
            cam,
            vec3(-20000.0, v, 0.0),
            vec3(30000.0, v, 0.0),
            1.0,
            col,
        );
    }
    let mesh = s.mesh(view, None, &[]);
    let mut drei: Vec<Dreieck> = Vec::new();
    let z_schnitt = s.plan_cut();
    let dazu = dazu.map_or(&[][..], |m| &m.faces[..]);
    for t in mesh.faces.chunks(3).chain(dazu.chunks(3)) {
        let p = |i: usize| vec3(t[i][0] as f64, t[i][1] as f64, t[i][2] as f64);
        let n = vec3(t[0][3] as f64, t[0][4] as f64, t[0][5] as f64);
        // Das Netz des Grundrisses endet in Schnitthöhe: dort nur die
        // Schnittflächen (dunkel), sonst die Flächen, die zur Kamera schauen
        let mitte = (p(0) + p(1) + p(2)) * (1.0 / 3.0);
        let geschnitten = view == ViewKind::Plan;
        if geschnitten && (n.z < 0.5 || mitte.z < z_schnitt - 1.0) {
            continue;
        }
        if n.dot(mitte - cam.eye) > 0.0 {
            continue;
        }
        let q: Option<Vec<(f64, f64)>> = (0..3).map(|i| cam.project(p(i), w, h)).collect();
        let Some(q) = q else {
            continue;
        };
        let tiefe = (mitte - cam.eye).length();
        let hell = if geschnitten {
            0.35
        } else {
            0.62 + 0.33 * n.dot(licht).max(0.0)
        };
        let q = [0, 1, 2].map(|i| (q[i].0 as f32, q[i].1 as f32));
        drei.push((tiefe, q, hell));
    }
    drei.sort_by(|a, b| b.0.total_cmp(&a.0));
    for (_, q, hell) in drei {
        let g = (hell * 235.0).round() as u8;
        let mut p = Path::new();
        p.move_to(q[0].0, q[0].1)
            .line_to(q[1].0, q[1].1)
            .line_to(q[2].0, q[2].1)
            .close();
        c.fill(&p, Rgba(g, g, (g as f64 * 0.97) as u8, 255));
    }
    c
}

/// Pfeil und Pille auf das Bild der Szene.
fn mit_pfeil(c: &mut Canvas, n: &Nordpfeil, s: &Scene, cam: &Camera, theme: &Theme) {
    mit_pfeil_bei(c, n, s, cam, theme, S);
}

/// Wie [`mit_pfeil`] mit der Skalierung `sk`.
fn mit_pfeil_bei(c: &mut Canvas, n: &Nordpfeil, s: &Scene, cam: &Camera, theme: &Theme, sk: f32) {
    let st = Stand {
        nord: s.model().location().north,
        fuss: anzeige_fuss(
            s.model().north_foot(),
            s.bounds(),
            laenge(cam, H as f64, sk as f64),
        ),
        gesetzt: s.model().north_foot(),
    };
    let (w, h) = (W as f64, H as f64);
    // Dasselbe Bild wie in der App
    if let Some((bild, x, y)) = n
        .bild(st, cam, (w, h), sk, TINTE, HEISS, 1.5)
        .and_then(|b| b.malen())
    {
        c.blit(&bild, x, y);
    }
    if let Some((p, text)) = n.label(cam, (w, h), sk as f64) {
        let pille = crate::flush_pick::paint_label(&schriften(), &text, sk, theme);
        if let Some((x, y)) = cam.project(p, w, h) {
            let x = (x - pille.width as f64 * 0.5).round() as i32;
            let y = (y - pille.height as f64 * 0.5).round() as i32;
            c.blit(&pille, x, y);
        }
    }
}

fn haus() -> Scene {
    let text = include_str!("../../crates/sk-cost/referenz/rh1-standardhaus.szo");
    let m = szo::read_with(text, GuidGen::with_seed(1), &sk_cost::lesen::ABSCHNITTE_SZO)
        .unwrap()
        .model;
    Scene::with_model(m)
}

/// Grundriss, eingepasst auf das Haus und den Platz vorne links davor.
fn grundriss(s: &Scene) -> Camera {
    let (lo, hi) = s
        .bounds()
        .unwrap_or((vec3(0.0, 0.0, 0.0), vec3(10000.0, 8000.0, 0.0)));
    let lo = lo - vec3(4000.0, 4000.0, 0.0);
    let mitte = (lo + hi) * 0.5;
    let half =
        ((hi.y - lo.y) * 0.5 + 5000.0).max((hi.x - lo.x) * 0.5 * H as f64 / W as f64 + 5000.0);
    Camera::parallel(vec3(mitte.x, mitte.y, 0.0), FRAC_PI_2, -FRAC_PI_2, half)
}

/// 3D von vorne links oben, wie die Startansicht.
fn raeumlich(s: &Scene) -> Camera {
    let (lo, hi) = s.bounds().unwrap();
    let mitte = (lo + hi) * 0.5;
    let d = (hi - lo).length() * 1.4;
    let eye = mitte + vec3(-0.55 * d, -0.85 * d, 0.55 * d);
    Camera::looking_at(eye, vec3(mitte.x, mitte.y, 0.0), 45.0)
}

#[test]
#[ignore = "legt Ist-Bilder ab, nur mit SKIZZEO_ISTBILDER"]
fn istbilder_s2() {
    let Some(ziel) = std::env::var_os("SKIZZEO_ISTBILDER").map(PathBuf::from) else {
        return;
    };
    let theme = Theme::dark();
    let ab = |c: &Canvas, n: &str| std::fs::write(ziel.join(n), c.to_png()).unwrap();
    let ruhig = Nordpfeil::default();

    // a) Haus mit Pfeil rechts vorn, Nord 30°: Grundriss und 3D
    let mut s = haus();
    let (lo, hi) = s.bounds().unwrap();
    assert!(s.nordpfeil_setzen(LABEL_DREHEN, 30.0, Some([hi.x + 2500.0, lo.y - 1500.0])));
    let cam = grundriss(&s);
    let mut c = szene(&mut s, &cam, ViewKind::Plan);
    mit_pfeil(&mut c, &ruhig, &s, &cam, &theme);
    ab(&c, "ist-s2-grundriss.png");
    let cam = raeumlich(&s);
    let mut c = szene(&mut s, &cam, ViewKind::Persp);
    mit_pfeil(&mut c, &ruhig, &s, &cam, &theme);
    ab(&c, "ist-s2-3d.png");

    // b) Aufziehen im Grundriss: Fußpunkt gesetzt, Maus bei 40°, Pille
    let mut s = haus();
    let cam = grundriss(&s);
    let (lo, hi) = s.bounds().unwrap();
    let fuss = [hi.x + 2500.0, lo.y - 1500.0];
    let mut n = Nordpfeil::default();
    n.set_aktiv(true);
    let (w, h) = (W as f64, H as f64);
    let st = Stand {
        nord: None,
        fuss: [0.0, 0.0],
        gesetzt: None,
    };
    let bild = |p: Foot| cam.project(vec3(p[0], p[1], 0.0), w, h).unwrap();
    let m = Modifiers::default();
    let (x, y) = bild(fuss);
    let e = Event::MouseDown {
        button: MouseButton::Left,
        x,
        y,
        mods: m,
    };
    n.handle(&e, st, &cam, w, h, S as f64, true);
    let r = 2600.0;
    let maus = [
        fuss[0] + r * 40f64.to_radians().sin(),
        fuss[1] + r * 40f64.to_radians().cos(),
    ];
    let (x, y) = bild([maus[0] + 30.0, maus[1] - 20.0]);
    n.handle(
        &Event::MouseMove { x, y, mods: m },
        st,
        &cam,
        w,
        h,
        S as f64,
        true,
    );
    assert_eq!(n.label(&cam, (w, h), S as f64).unwrap().1, "N 40°");
    let mut c = szene(&mut s, &cam, ViewKind::Plan);
    mit_pfeil(&mut c, &n, &s, &cam, &theme);
    ab(&c, "ist-s2-aufziehen.png");

    // c) Ersatzplatz: Pfeil am Ursprung, danach das Haus darüber; der Pfeil
    // steht vorne links, die Datei behält x=0 y=0
    let mut s = Scene::with_model(Model::with_seed(1));
    assert!(s.nordpfeil_setzen(LABEL_DREHEN, 40.0, Some([0.0, 0.0])));
    assert!(szo::write(s.model()).contains("x=0 y=0"));
    let mut mit = haus();
    assert!(mit.nordpfeil_setzen(LABEL_DREHEN, 40.0, Some([0.0, 0.0])));
    assert!(szo::write(mit.model()).contains("[location] north=40 x=0 y=0\n"));
    let (lo, _) = mit.bounds().unwrap();
    assert!(lo.x <= 0.0 && lo.y <= 0.0, "Haus über dem Ursprung: {lo:?}");
    // Wie in einem Fenster von 1600 × 1000 dip (Skalierung 1); der
    // Ausschnitt fasst Haus und Platz, der Platz wächst mit dem Ausschnitt
    let (lo, hi) = mit.bounds().unwrap();
    let mut cam = grundriss(&mit);
    for _ in 0..8 {
        let l = laenge(&cam, H as f64, 1.0);
        let f = anzeige_fuss(mit.model().north_foot(), mit.bounds(), l);
        let r = n_reichweite(l) + 500.0;
        let (a, b) = (vec3(f[0] - r, f[1] - r, 0.0), hi + vec3(500.0, 500.0, 0.0));
        let mitte = (a + b) * 0.5;
        let half = ((b.y - a.y) * 0.5).max((b.x - a.x) * 0.5 * H as f64 / W as f64);
        cam = Camera::parallel(vec3(mitte.x, mitte.y, 0.0), FRAC_PI_2, -FRAC_PI_2, half);
    }
    let l = laenge(&cam, H as f64, 1.0);
    let f = anzeige_fuss(mit.model().north_foot(), mit.bounds(), l);
    assert!(f[0] < lo.x - 1000.0 && f[1] < lo.y - 1000.0);
    let mut c = szene(&mut mit, &cam, ViewKind::Plan);
    mit_pfeil_bei(&mut c, &ruhig, &mit, &cam, &theme, 1.0);
    ab(&c, "ist-s2-ersatzplatz.png");
}

/// Bild zum Sonnenstand (S4): Szene mit dem Licht der Sonne, Würfel ohne
/// Gebäude, Bahnen und Scheibe, Pfeil und Leiste wie in der App. Die
/// Kamera fasst die ganze Himmelskuppel.
fn s4_bild(s: &mut Scene, sun: sk_model::Sun, theme: &Theme, blick: Vec3) -> Canvas {
    use crate::sonne_view as sv;
    let q = s.bounds().unwrap_or_else(sv::wuerfel_quader);
    let ort = *s.model().location();
    let himmel = sv::himmel(&ort, &sun, q);
    let (m, r) = sv::kuppel(q);
    let ziel = m + vec3(0.0, 0.0, 0.3 * r);
    let cam = Camera::looking_at(ziel + blick.normalized() * (3.0 * r), ziel, 45.0);
    let licht = sv::licht(&ort, &sun).map_or(vec3(-0.35, -0.55, 0.75).normalized(), |l| {
        vec3(l[0] as f64, l[1] as f64, l[2] as f64)
    });
    let wuerfel = s.bounds().is_none().then(sv::wuerfel_netz);
    let mut c = szene_mit(s, &cam, ViewKind::Persp, licht, wuerfel.as_ref());
    sv::malen(&mut c, &himmel, &cam, (W as f64, H as f64), S);
    mit_pfeil(&mut c, &Nordpfeil::default(), s, &cam, theme);
    let b = sv::LeistenBild {
        sun,
        unter: himmel.sonne.is_none(),
        eingabe: None,
        hover: None,
        vw: W as u32,
        scale: S.to_bits(),
    };
    let (leiste, x, y) = sv::leiste_malen(&b, &schriften(), theme);
    c.blit(&leiste, x, y);
    c
}

/// Ist-Bilder Sonnenstand S4 (§8 09:25): 3D mit Leiste am 21.06. und am
/// 21.12. um 12:00 (Haus, Nord 30°), der Würfel ohne Gebäude.
/// `SKIZZEO_ISTBILDER=<ordner> cargo test -p skizzeo istbilder_s4 -- --ignored`
#[test]
#[ignore = "legt Ist-Bilder ab, nur mit SKIZZEO_ISTBILDER"]
fn istbilder_s4() {
    use sk_math::sonne::Datum;
    let Some(ziel) = std::env::var_os("SKIZZEO_ISTBILDER").map(PathBuf::from) else {
        return;
    };
    let theme = Theme::dark();
    let ab = |c: &Canvas, n: &str| std::fs::write(ziel.join(n), c.to_png()).unwrap();
    let sun = |m, d| sk_model::Sun {
        date: Datum::new(2026, m, d).unwrap(),
        minutes: 12 * 60,
        on: true,
    };
    let blick = vec3(0.75, -1.0, 0.62);
    let mut s = haus();
    let (lo, hi) = s.bounds().unwrap();
    assert!(s.nordpfeil_setzen(LABEL_DREHEN, 30.0, Some([hi.x + 2500.0, lo.y - 1500.0])));
    for (n, m, d) in [("0621", 6, 21), ("1221", 12, 21)] {
        s.set_sun(sun(m, d));
        ab(
            &s4_bild(&mut s, sun(m, d), &theme, blick),
            &format!("ist-s4-{n}-1200.png"),
        );
    }
    let mut s = Scene::with_model(Model::with_seed(1));
    assert!(s.nordpfeil_setzen(LABEL_DREHEN, 0.0, Some([-4000.0, -4000.0])));
    s.set_sun(sun(6, 21));
    ab(
        &s4_bild(&mut s, sun(6, 21), &theme, blick),
        "ist-s4-wuerfel-0621-1200.png",
    );
}
