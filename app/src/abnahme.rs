//! Abnahmetests: je Anforderung aus dem Katalog `skizzeo/test/katalog.md` ein
//! Test `aNN_…`. Sie bedienen das Programm so, wie Jörn es tut: Mausklicks auf
//! Bildpunkte, Tasten, Knöpfe in den Paneelen. Geprüft wird das Ergebnis, das
//! man sieht oder abliest, nicht der innere Weg dorthin.
//!
//! Alles läuft ohne Fenster: `cargo test -p skizzeo abnahme`. Was nur unter
//! echtem Windows prüfbar ist, steht in `skizzeo/test/handtest.md`.

use crate::camera::Camera;
use crate::draw_table::{fill_kind, DrawTable};
use crate::scene::{Plane, Scene, PLAN_CUT};
use crate::section::SectionLine;
use crate::selection::{self, Selection};
use crate::ui::{Id, Panel, Ui, ViewKind};
use crate::wall_edit::WallEdit;
use crate::wall_tool::WallTool;
use crate::{fit_parallel, fit_parallel_in, fit_perspective, plan_camera};
use sk_math::{vec3, Vec3};
use sk_model::{edge_kind, FillKind, Model, RefSide, RunId, WallChain};
use sk_platform::{Event, Key, Modifiers, MouseButton};
use sk_render::MeshData;
use sk_ui::theme::Theme;
use sk_ui::titlebar::{Button, TitleBar};

/// Größe der 3D-Ansicht in den Tests (Pixel, 96 dpi).
const W: f64 = 1440.0;
const H: f64 = 778.0;
const M: Modifiers = Modifiers {
    shift: false,
    ctrl: false,
    alt: false,
};

fn mv(x: f64, y: f64) -> Event {
    Event::MouseMove { x, y, mods: M }
}
fn down(x: f64, y: f64) -> Event {
    Event::MouseDown {
        button: MouseButton::Left,
        x,
        y,
        mods: M,
    }
}
fn up(x: f64, y: f64) -> Event {
    Event::MouseUp {
        button: MouseButton::Left,
        x,
        y,
        mods: M,
    }
}
fn key(k: Key) -> Event {
    Event::Key {
        key: k,
        down: true,
        repeat: false,
        mods: M,
    }
}

/// 3D-Ansicht schräg von vorne oben auf ein Baufeld um (5 m, 4 m).
fn cam3d() -> Camera {
    Camera::looking_at(
        vec3(5000.0, -14000.0, 16000.0),
        vec3(5000.0, 4000.0, 0.0),
        45.0,
    )
}

/// Grundriss so, wie die App ihn für ein leeres Baufeld einrichtet.
fn cam_plan(scene: &Scene) -> Camera {
    fit_parallel(ViewKind::Plan, scene.bounds(), W, H)
}

fn px(c: &Camera, p: Vec3) -> (f64, f64) {
    c.project(p, W, H).expect("Punkt im Bild")
}

/// Bildabstand zweier Modellpunkte in Pixeln.
fn px_dist(c: &Camera, a: Vec3, b: Vec3) -> f64 {
    let (pa, pb) = (px(c, a), px(c, b));
    (pa.0 - pb.0).hypot(pa.1 - pb.1)
}

/// Werkzeug „Gebäude“, eingeschaltet, mit dem Aufbau aus der Bibliothek.
fn tool(scene: &Scene) -> WallTool {
    let mut t = WallTool::new();
    let set = scene.model().defaults().exterior_wall;
    t.layers = scene.model().wall_layers(set);
    (t.z, t.height) = scene.work_plane();
    t.set_enabled(true);
    t
}

/// Mausklick des Nutzers auf einen Bildpunkt (Bewegen, Drücken).
fn click_px(t: &mut WallTool, c: &Camera, (x, y): (f64, f64)) -> Option<WallChain> {
    t.handle(&mv(x, y), c, W, H, 1.0);
    t.handle(&down(x, y), c, W, H, 1.0).commit
}

fn click(t: &mut WallTool, c: &Camera, p: Vec3) -> Option<WallChain> {
    click_px(t, c, px(c, p))
}

/// Rechteck 10 × 8 m im Uhrzeigersinn, so wie Jörn es eingibt.
const RECHTECK: [Vec3; 4] = [
    vec3(0.0, 0.0, 0.0),
    vec3(0.0, 8000.0, 0.0),
    vec3(10000.0, 8000.0, 0.0),
    vec3(10000.0, 0.0, 0.0),
];

/// Zeichnet das Rechteck mit der Maus und legt es im Modell an.
fn zeichne_rechteck(scene: &mut Scene, c: &Camera) -> RunId {
    let mut t = tool(scene);
    for p in RECHTECK {
        assert!(click(&mut t, c, p).is_none());
    }
    let wall = click(&mut t, c, RECHTECK[0]).expect("Klick auf den Startpunkt schließt");
    scene.add_wall(&wall).expect("Wandzug angelegt")
}

fn close(a: Vec3, b: Vec3) -> bool {
    (a - b).length() < 1e-6
}

/// Sucht in einem Paneel senkrecht nach Knöpfen, wie man mit der Maus darüber
/// fährt, und liefert ihre Mitte im Fenster.
fn find_buttons(ui: &mut Ui, p: Panel, win_w: u32, top: u32) -> Vec<(Id, f64, f64)> {
    let r = ui.rect(p, win_w, top);
    let mut found: Vec<(Id, f64, f64, f64)> = Vec::new();
    let mut y = r.y as f64;
    while y < (r.y + r.h) as f64 {
        let mut x = r.x as f64 + 2.0;
        while x < (r.x + r.w) as f64 {
            ui.handle(&mv(x, y), win_w, top);
            if let Some(id) = ui.hover {
                match found.iter_mut().find(|f| f.0 == id) {
                    Some(f) => f.3 = y,
                    None => found.push((id, x, y, y)),
                }
            }
            x += 8.0;
        }
        y += 1.0;
    }
    ui.handle(&Event::MouseLeave, win_w, top);
    found
        .into_iter()
        .map(|(id, x, y0, y1)| (id, x + 4.0, (y0 + y1) * 0.5))
        .collect()
}

fn ui_click(ui: &mut Ui, x: f64, y: f64, win_w: u32, top: u32) -> Option<Id> {
    ui.handle(&mv(x, y), win_w, top);
    ui.handle(&down(x, y), win_w, top);
    ui.handle(&up(x, y), win_w, top).clicked
}

// ---------------------------------------------------------------------------
// A01 – A06: Wandzug-Eingabe
// ---------------------------------------------------------------------------

/// A01: Der Knopf „Gebäude“ im Paneel „Werkzeuge“ startet die Eingabe.
#[test]
fn a01_knopf_gebaeude_im_paneel_werkzeuge() {
    let mut ui = Ui::new(1.0, &Theme::dark());
    ui.fit(1.0, 1440, 900);
    let buttons = find_buttons(&mut ui, Panel::Tools, 1440, 32);
    let &(_, x, y) = buttons
        .iter()
        .find(|b| b.0 == Id::Building)
        .expect("Knopf „Gebäude“ im Paneel „Werkzeuge“");
    assert_eq!(ui_click(&mut ui, x, y, 1440, 32), Some(Id::Building));
    // Das Paneel liegt links
    let r = ui.rect(Panel::Tools, 1440, 32);
    assert!(r.x < 50.0, "Werkzeuge links: {r:?}");
    // Bezugsseite und 90°-Sprung lassen sich auch dort einstellen
    for id in [
        Id::Ref(RefSide::Left),
        Id::Ref(RefSide::Center),
        Id::Ref(RefSide::Right),
        Id::Ortho,
    ] {
        assert!(buttons.iter().any(|b| b.0 == id), "{id:?} fehlt");
    }
}

/// A02: In 3D ein Rechteck klicken; Klick auf den Startpunkt schließt den Zug.
/// Ergebnis: vier Außenwände, zweischalig 31,5 cm, Höhe 3,50 m.
#[test]
fn a02_rechteck_in_3d_schliesst_am_startpunkt() {
    let mut s = Scene::with_model(Model::with_seed(1));
    let c = cam3d();
    let run = zeichne_rechteck(&mut s, &c);
    let chain = s.chain(run).unwrap();
    assert!(chain.closed);
    assert_eq!(chain.points.len(), 4);
    for (a, b) in chain.points.iter().zip(RECHTECK) {
        assert!(close(*a, b), "{a:?} ≠ {b:?}");
    }
    assert_eq!(
        chain.ref_side,
        RefSide::Left,
        "Bezugsseite außen ist Standard"
    );
    // B12: die EG-Wand reicht von UK EG bis OK EG
    assert_eq!((chain.base, chain.height), (0.0, 2855.0));
    // Vier Bauteile mit fortlaufender Nummer
    let numbers: Vec<_> = (0..4)
        .map(|i| {
            let id = s.model().wall_at(run, i).unwrap();
            s.model().element(id).unwrap().number.clone()
        })
        .collect();
    assert_eq!(numbers, ["AW-001", "AW-002", "AW-003", "AW-004"]);
    assert!(s.model().check().is_empty(), "{:?}", s.model().check());
}

/// A03: Der 90°-Sprung ist an. Ein schräg gesetzter Punkt landet rechtwinklig
/// zur letzten Wand; R schaltet ihn aus, Umschalt kehrt ihn kurz um.
#[test]
fn a03_neunzig_grad_sprung() {
    let s = Scene::new();
    let c = cam3d();
    let mut t = tool(&s);
    assert!(t.ortho, "90°-Sprung ist standardmäßig an");
    // Leicht schräg geklickt: Die Wände werden achsparallel bzw. rechtwinklig
    click(&mut t, &c, vec3(0.0, 0.0, 0.0));
    click(&mut t, &c, vec3(400.0, 6000.0, 0.0));
    click(&mut t, &c, vec3(5000.0, 6700.0, 0.0));
    let w = t.handle(&key(Key::Enter), &c, W, H, 1.0).commit.unwrap();
    let p = w.clean_points();
    assert!(p[1].x.abs() < 1e-6, "erste Wand senkrecht: {p:?}");
    assert!(
        (p[2].y - p[1].y).abs() < 1e-6,
        "zweite Wand rechtwinklig: {p:?}"
    );

    // R: aus. Der Punkt bleibt, wo geklickt wurde
    let mut t = tool(&s);
    t.handle(&key(Key::Char('R')), &c, W, H, 1.0);
    assert!(!t.ortho);
    click(&mut t, &c, vec3(0.0, 0.0, 0.0));
    click(&mut t, &c, vec3(3000.0, 5000.0, 0.0));
    let w = t.handle(&key(Key::Enter), &c, W, H, 1.0).commit.unwrap();
    let p = w.clean_points();
    assert!((p[1] - vec3(3000.0, 5000.0, 0.0)).length() < 5.0, "{p:?}");

    // Umschalt gedrückt halten kehrt den Sprung kurz um
    let mut t = tool(&s);
    click(&mut t, &c, vec3(0.0, 0.0, 0.0));
    let shift = Event::Key {
        key: Key::Shift,
        down: true,
        repeat: false,
        mods: Modifiers { shift: true, ..M },
    };
    t.handle(&shift, &c, W, H, 1.0);
    let (x, y) = px(&c, vec3(3000.0, 5000.0, 0.0));
    let sm = Modifiers { shift: true, ..M };
    t.handle(&Event::MouseMove { x, y, mods: sm }, &c, W, H, 1.0);
    t.handle(
        &Event::MouseDown {
            button: MouseButton::Left,
            x,
            y,
            mods: sm,
        },
        &c,
        W,
        H,
        1.0,
    );
    let w = t.handle(&key(Key::Enter), &c, W, H, 1.0).commit.unwrap();
    let p = w.clean_points();
    assert!((p[1] - vec3(3000.0, 5000.0, 0.0)).length() < 5.0, "{p:?}");
}

/// A04: Der Anfangspunkt fängt: Ein Klick knapp daneben (wenige Pixel) schließt
/// den Zug genau dort. Spurlinien fangen die letzte Ecke rechtwinklig zum Start.
#[test]
fn a04_fang_des_anfangspunkts() {
    let s = Scene::new();
    let c = cam3d();
    let mut t = tool(&s);
    for p in &RECHTECK[..3] {
        click(&mut t, &c, *p);
    }
    // Vierte Ecke etwas daneben: Spurlinie vom Start fängt (10 m, 0)
    click(&mut t, &c, vec3(10040.0, 30.0, 0.0));
    // Knapp neben dem Startpunkt (unter 12 px) klicken
    let (sx, sy) = px(&c, RECHTECK[0]);
    let w = click_px(&mut t, &c, (sx + 6.0, sy - 5.0)).expect("Zug geschlossen");
    assert!(w.closed);
    assert_eq!(w.points.len(), 4);
    assert!(close(w.points[0], RECHTECK[0]));
    assert!(
        close(w.points[3], RECHTECK[3]),
        "Spurlinie: {:?}",
        w.points[3]
    );
    // Weiter weg (über 12 px) schließt nicht
    let mut t = tool(&s);
    for p in &RECHTECK {
        click(&mut t, &c, *p);
    }
    let far = px_dist(&c, RECHTECK[0], vec3(800.0, 0.0, 0.0));
    assert!(far > 15.0, "Testpunkt weit genug weg: {far}");
    assert!(click(&mut t, &c, vec3(0.0, -800.0, 0.0)).is_none());
}

/// A05: Die Eingabe geht auch im Grundriss (Blick von oben, Parallelprojektion).
#[test]
fn a05_eingabe_im_grundriss() {
    let mut s = Scene::with_model(Model::with_seed(2));
    let c = cam_plan(&s);
    assert!(c.ortho.is_some(), "Grundriss ist parallel");
    assert!(
        c.forward().z < -0.999,
        "Grundriss blickt senkrecht nach unten"
    );
    let run = zeichne_rechteck(&mut s, &c);
    let chain = s.chain(run).unwrap();
    assert!(chain.closed);
    for (a, b) in chain.points.iter().zip(RECHTECK) {
        assert!((*a - b).length() < 1e-6, "{a:?} ≠ {b:?}");
    }
}

/// A06: Offener Zug: Enter oder Doppelklick beendet ihn, Rücktaste nimmt den
/// letzten Punkt zurück, Esc bricht ab, Tab wechselt die Bezugsseite,
/// Strg+Z/Strg+Y nehmen die Wand zurück.
#[test]
fn a06_offener_zug_ruecktaste_esc_rueckgaengig() {
    let mut s = Scene::with_model(Model::with_seed(3));
    let c = cam3d();
    let mut t = tool(&s);
    click(&mut t, &c, vec3(0.0, 0.0, 0.0));
    click(&mut t, &c, vec3(0.0, 5000.0, 0.0));
    click(&mut t, &c, vec3(4000.0, 5000.0, 0.0));
    t.handle(&key(Key::Backspace), &c, W, H, 1.0);
    let w = t.handle(&key(Key::Enter), &c, W, H, 1.0).commit.unwrap();
    assert!(!w.closed);
    assert_eq!(
        w.clean_points().len(),
        2,
        "Rücktaste nahm den letzten Punkt"
    );
    // Doppelklick auf den letzten Punkt beendet ebenfalls
    let mut t = tool(&s);
    click(&mut t, &c, vec3(0.0, 0.0, 0.0));
    click(&mut t, &c, vec3(5000.0, 0.0, 0.0));
    let w = click(&mut t, &c, vec3(5000.0, 0.0, 0.0)).expect("Doppelklick beendet");
    assert_eq!(w.clean_points().len(), 2);
    let run = s.add_wall(&w).unwrap();
    // Esc bricht einen angefangenen Zug ab
    click(&mut t, &c, vec3(0.0, 3000.0, 0.0));
    assert!(t.is_active());
    t.handle(&key(Key::Escape), &c, W, H, 1.0);
    assert!(!t.is_active());
    // Tab wechselt die Bezugsseite während der Eingabe: außen, innen, Achse
    let mut t = tool(&s);
    click(&mut t, &c, vec3(0.0, 0.0, 0.0));
    let mut sides = vec![t.ref_side];
    for _ in 0..3 {
        t.handle(&key(Key::Tab), &c, W, H, 1.0);
        sides.push(t.ref_side);
    }
    assert_eq!(
        sides,
        [
            RefSide::Left,
            RefSide::Right,
            RefSide::Center,
            RefSide::Left
        ]
    );
    assert!(t.is_active(), "Tab bricht die Eingabe nicht ab");
    // Rückgängig und Wiederholen
    assert!(s.undo());
    assert!(s.model().run(run).is_none());
    assert!(s.redo());
    assert!(s.model().run(run).is_some());
}

// ---------------------------------------------------------------------------
// A07 – A08: Gummiband
// ---------------------------------------------------------------------------

/// A07: Gummiband nur beim Darüberfahren, in 3D und im Grundriss. Ziehen
/// verschiebt die Wand quer im 10-mm-Raster, die Nachbarn behalten ihre
/// Richtung, Esc bricht ab, Rückgängig nimmt das Ziehen zurück.
#[test]
fn a07_gummiband_nur_beim_hovern_in_3d_und_grundriss() {
    for plan in [false, true] {
        let mut s = Scene::with_model(Model::with_seed(4));
        let c0 = cam3d();
        let run = zeichne_rechteck(&mut s, &c0);
        let c = if plan { cam_plan(&s) } else { c0 };
        let mut e = WallEdit::default();
        let ctx = if plan { "Grundriss" } else { "3D" };

        // Maus im Leeren: kein Band
        e.handle(&mv(5.0, 5.0), &mut s, &c, W, H, 1.0, true);
        assert!(
            e.helpers(&s, &c, 1.0, !plan, &Theme::dark()).is_empty(),
            "{ctx}: Band ohne Hover"
        );
        // Über dem oberen Wandfuß (Segment 1, y = 8 m): Band erscheint
        let (x, y) = px(&c, vec3(5000.0, 8000.0, 0.0));
        e.handle(&mv(x, y), &mut s, &c, W, H, 1.0, true);
        assert!(
            !e.helpers(&s, &c, 1.0, !plan, &Theme::dark()).is_empty(),
            "{ctx}: Band beim Hover"
        );
        // Während der Wandeingabe gibt es kein Band
        let mut off = WallEdit::default();
        off.handle(&mv(x, y), &mut s, &c, W, H, 1.0, false);
        assert!(
            off.helpers(&s, &c, 1.0, !plan, &Theme::dark()).is_empty(),
            "{ctx}: Band bei Eingabe"
        );

        // Ziehen: 1,003 m nach außen ergibt genau 1,00 m (Raster 10 mm)
        assert!(e.handle(&down(x, y), &mut s, &c, W, H, 1.0, true).consumed);
        let (x2, y2) = px(&c, vec3(5300.0, 9003.0, 0.0));
        e.handle(&mv(x2, y2), &mut s, &c, W, H, 1.0, true);
        let p = s.chain(run).unwrap().points.clone();
        assert!((p[1].y - 9000.0).abs() < 1e-6, "{ctx}: {p:?}");
        assert!((p[2].y - 9000.0).abs() < 1e-6, "{ctx}: {p:?}");
        assert!(p[1].x.abs() < 1e-6 && (p[2].x - 10000.0).abs() < 1e-6);
        // Esc bricht ab
        e.handle(&key(Key::Escape), &mut s, &c, W, H, 1.0, true);
        assert!(
            (s.chain(run).unwrap().points[1].y - 8000.0).abs() < 1e-6,
            "{ctx}"
        );
        // Nochmal ziehen und loslassen, dann rückgängig
        e.handle(&mv(x, y), &mut s, &c, W, H, 1.0, true);
        e.handle(&down(x, y), &mut s, &c, W, H, 1.0, true);
        e.handle(&mv(x2, y2), &mut s, &c, W, H, 1.0, true);
        e.handle(&up(x2, y2), &mut s, &c, W, H, 1.0, true);
        assert!((s.chain(run).unwrap().points[1].y - 9000.0).abs() < 1e-6);
        assert!(s.model().check().is_empty());
        assert!(s.undo());
        assert!(
            (s.chain(run).unwrap().points[1].y - 8000.0).abs() < 1e-6,
            "{ctx}"
        );
    }
}

/// A08: In Schnitt und Ansichten lassen sich die Wände ziehen, die vom
/// Betrachter weg laufen (Kugel am Fuß). Wände quer zum Blick und verdeckte
/// Wände sind nicht greifbar; im Schnitt nur, was hinter der Ebene liegt.
#[test]
fn a08_ziehpunkte_am_wandfuss_in_schnitt_und_ansichten() {
    let mut s = Scene::with_model(Model::with_seed(5));
    let run = zeichne_rechteck(&mut s, &cam3d());
    let left = s.model().wall_at(run, 0).unwrap(); // x = 0, läuft nach +y
    let right = s.model().wall_at(run, 2).unwrap(); // x = 10 m

    // Ansicht von vorne (Blick nach +y)
    let c = fit_parallel(ViewKind::Front, s.bounds(), W, H);
    let mut e = WallEdit::default();
    // Linke Wand: ihr Fuß erscheint als Punkt bei x = 0
    let (x, y) = px(&c, vec3(0.0, 0.0, 0.0));
    let out = e.handle(&mv(x, y), &mut s, &c, W, H, 1.0, true);
    assert!(out.redraw, "Kugel erscheint");
    assert!(e.is_busy());
    let ball = e.helpers(&s, &c, 1.0, false, &Theme::dark());
    assert!(
        !ball.is_empty() && ball.iter().any(|h| h.a == h.b && h.round),
        "Kugel"
    );
    // Nach links ziehen: Wand rückt 50 cm nach außen
    e.handle(&down(x, y), &mut s, &c, W, H, 1.0, true);
    let (x2, y2) = px(&c, vec3(-500.0, 0.0, 0.0));
    e.handle(&mv(x2, y2), &mut s, &c, W, H, 1.0, true);
    e.handle(&up(x2, y2), &mut s, &c, W, H, 1.0, true);
    let p = &s.chain(run).unwrap().points;
    assert!(
        (p[0].x + 500.0).abs() < 1e-6 && (p[1].x + 500.0).abs() < 1e-6,
        "{p:?}"
    );
    assert_eq!(s.model().wall_at(run, 0), Some(left), "Kennung bleibt");
    s.undo();

    // Mitte der vorderen Wand (quer zum Blick): kein Griff
    let mut e = WallEdit::default();
    let (x, y) = px(&c, vec3(5000.0, 0.0, 0.0));
    e.handle(&mv(x, y), &mut s, &c, W, H, 1.0, true);
    assert!(!e.is_busy(), "Wand quer zum Blick ist nicht greifbar");

    // Schnitt bei y = 4 m, Blick nach +y: Seitenwände reichen hinter die Ebene
    let mut sect = SectionLine::default();
    sect.ensure(&s);
    let plane = sect.plane();
    let c = fit_parallel(ViewKind::Section, s.bounds(), W, H);
    let mut e = WallEdit::default();
    e.section = plane;
    let (x, y) = px(&c, vec3(10000.0, 4000.0, 0.0));
    e.handle(&mv(x, y), &mut s, &c, W, H, 1.0, true);
    assert!(e.is_busy(), "Seitenwand im Schnitt greifbar");
    e.handle(&down(x, y), &mut s, &c, W, H, 1.0, true);
    let (x2, y2) = px(&c, vec3(10500.0, 4000.0, 0.0));
    e.handle(&mv(x2, y2), &mut s, &c, W, H, 1.0, true);
    e.handle(&up(x2, y2), &mut s, &c, W, H, 1.0, true);
    let wall = s.model().wall_at(run, 2);
    assert_eq!(wall, Some(right));
    let p = &s.chain(run).unwrap().points;
    assert!((p[2].x - 10500.0).abs() < 1e-6, "{p:?}");
    assert!(s.model().check().is_empty());
}

// ---------------------------------------------------------------------------
// A09 – A11: Wandaufbau und Bauzeichnung
// ---------------------------------------------------------------------------

/// A09: Die Außenwand ist zweischalig: außen 14 cm Dämmung (WDVS, Zickzack),
/// innen 17,5 cm Gasbeton (schräg schraffiert), zusammen 31,5 cm.
#[test]
fn a09_zweischalige_aussenwand() {
    let mut s = Scene::with_model(Model::with_seed(6));
    let run = zeichne_rechteck(&mut s, &cam3d());
    let m = s.model();
    let el = m.element(m.wall_at(run, 0).unwrap()).unwrap();
    let set = m.layer_set(el.layer_set.unwrap()).unwrap();
    let rows: Vec<_> = set
        .layers
        .iter()
        .map(|l| {
            let mat = m.material(l.material).unwrap();
            let fill = &m.attr().fill(mat.cut_fill).unwrap().kind;
            let zigzag = matches!(fill, FillKind::Zigzag { .. });
            let lines = matches!(fill, FillKind::Lines(_));
            (mat.name.as_str(), l.thickness, zigzag, lines)
        })
        .collect();
    assert_eq!(
        rows,
        [
            ("Dämmung (WDVS)", 140.0, true, false),
            ("Gasbeton", 175.0, false, true)
        ]
    );
    let q = s.wall_qto(m.wall_at(run, 0).unwrap()).unwrap();
    assert_eq!(q.width, 315.0);
    // Die Wand liegt innen an der geklickten Linie (Bezugsseite außen):
    // Im Grundriss reicht der Körper von x = 0 bis x = 315 mm
    let mesh = s.mesh(ViewKind::Plan, None, &[]);
    // (nur die Wand über der Sohlplatte)
    let xs = mesh
        .faces
        .iter()
        .filter(|v| v[0] < 1000.0 && v[2] > 1.0)
        .map(|v| v[0]);
    let (lo, hi) = xs.fold((f32::MAX, f32::MIN), |(a, b), x| (a.min(x), b.max(x)));
    assert!(lo.abs() < 1e-3 && (hi - 315.0).abs() < 1e-3, "{lo} … {hi}");
}

/// Muster einer Fläche, wie der Shader sie zeigt.
mod pattern {
    pub const NONE: f32 = 0.0;
    pub const DIAGONAL: f32 = 1.0;
    pub const ZIGZAG: f32 = 2.0;
    pub const CONCRETE: f32 = 3.0;
    pub const SOLID: f32 = 4.0;
}

/// Ein Netz so, wie Netz und Zeichentabelle es auf den Bildschirm bringen:
/// je Ecke Lage (0..3), Normale (3..6), Farbe (6..9) und Muster (9), je
/// Kante die Strichbreite in Bildpunkten bei 96 dpi.
struct Shown {
    faces: Vec<[f32; 10]>,
    edges: Vec<([[f32; 3]; 2], f32)>,
}

fn shown(t: &DrawTable, m: &MeshData, drawing: bool) -> Shown {
    let faces = m
        .faces
        .iter()
        .map(|v| {
            let key = v[6] as u16;
            let look = t.look(key);
            let cut = key & sk_model::material::CUT != 0;
            let (c, pat) = match (drawing, cut) {
                (true, true) => (
                    look.cut_bg,
                    match (look.kind, look.line_count) {
                        (fill_kind::LINES, 2) => pattern::CONCRETE,
                        (fill_kind::LINES, _) => pattern::DIAGONAL,
                        (fill_kind::ZIGZAG, _) => pattern::ZIGZAG,
                        (fill_kind::SOLID, _) => pattern::SOLID,
                        _ => pattern::NONE,
                    },
                ),
                (true, false) => (look.cut_bg, pattern::NONE),
                (false, true) => (look.cut, pattern::NONE),
                (false, false) => (look.face, pattern::NONE),
            };
            [v[0], v[1], v[2], v[3], v[4], v[5], c[0], c[1], c[2], pat]
        })
        .collect();
    let edges = m
        .edges
        .iter()
        .map(|(p, k)| (*p, t.edge_width(drawing, *k as u8)))
        .collect();
    Shown { faces, edges }
}

/// Netz einer Ansicht so, wie es auf dem Bildschirm erscheint.
fn view_mesh(s: &mut Scene, v: ViewKind, section: Option<Plane>) -> Shown {
    let m = s.mesh(v, section, &[]);
    shown(s.table(), &m, v != ViewKind::Persp)
}

/// Kanten (a, b, Breite) einer Bauzeichnung, die ganz auf der Geraden x = `x`
/// liegen (zwischen y = 1 m und 7 m).
fn edges_at_x(m: &Shown, x: f32) -> Vec<f32> {
    m.edges
        .iter()
        .filter(|(p, _)| {
            (p[0][0] - x).abs() < 0.5
                && (p[1][0] - x).abs() < 0.5
                && (p[0][1] - p[1][1]).abs() > 1000.0
        })
        .map(|e| e.1)
        .collect()
}

/// A10: Bauzeichnungs-Look in Grundriss und Schnitt: weiße Schnittflächen,
/// Gasbeton schräg schraffiert und dick umrandet, Dämmung Zickzack und
/// mitteldick umrandet. 3D bleibt farbig ohne Schraffur.
#[test]
fn a10_bauzeichnung_linienstaerken_und_schraffuren() {
    let mut s = Scene::with_model(Model::with_seed(7));
    zeichne_rechteck(&mut s, &cam3d());
    // Strichbreiten aus der Zeichentabelle (Stifte der Bauteildarstellung)
    let w = |k| s.table().edge_width(true, k);
    let (cut_w, layer_w, view_w, fine_w) = (
        w(edge_kind::CUT),
        w(edge_kind::CUT_LAYER),
        w(edge_kind::VIEW),
        w(edge_kind::FINE),
    );
    assert!(cut_w > layer_w && layer_w > fine_w);

    let plan = view_mesh(&mut s, ViewKind::Plan, None);
    let white = [1.0f32, 1.0, 1.0];
    assert!(plan.faces.iter().all(|v| v[6..9] == white), "Flächen weiß");
    let has = |m: &Shown, pat: f32| m.faces.iter().any(|v| v[9] == pat);
    assert!(has(&plan, pattern::DIAGONAL), "Gasbeton schräg schraffiert");
    assert!(has(&plan, pattern::ZIGZAG), "Dämmung Zickzack");
    // Linke Wand im Grundriss: Außenkante Dämmung (x = 0) mitteldick,
    // Fuge (x = 140) und Innenkante Gasbeton (x = 315) dick
    let max = |v: Vec<f32>| v.into_iter().fold(0.0f32, f32::max);
    assert_eq!(max(edges_at_x(&plan, 0.0)), layer_w, "Dämmung außen");
    assert_eq!(max(edges_at_x(&plan, 140.0)), cut_w, "Gasbeton außen");
    assert_eq!(max(edges_at_x(&plan, 315.0)), cut_w, "Gasbeton innen");
    // Geschnitten in 1,00 m Höhe
    assert_eq!(PLAN_CUT, 1000.0);
    assert!(plan.faces.iter().all(|v| v[2] <= PLAN_CUT as f32 + 1e-3));

    // Schnitt A–A: ebenfalls Schraffuren und dicke Kontur
    let mut sect = SectionLine::default();
    sect.ensure(&s);
    let cut = view_mesh(&mut s, ViewKind::Section, sect.plane());
    assert!(has(&cut, pattern::DIAGONAL) && has(&cut, pattern::ZIGZAG));
    assert!(cut.edges.iter().any(|e| e.1 == cut_w));
    assert!(cut.edges.iter().any(|e| e.1 == layer_w));

    // Ansicht: keine Schraffur, Ansichtskanten mittel
    let front = view_mesh(&mut s, ViewKind::Front, None);
    assert!(!has(&front, pattern::DIAGONAL) && !has(&front, pattern::ZIGZAG));
    assert!(front.edges.iter().any(|e| e.1 == view_w));
    // 3D: farbige Flächen, keine Schraffur
    let p3 = view_mesh(&mut s, ViewKind::Persp, None);
    assert!(p3.faces.iter().all(|v| v[9] == pattern::NONE));
    assert!(p3.faces.iter().any(|v| v[6..9] != white));
}

/// A11: Schnittlinie A–A nach DIN 1356 im Grundriss: liegt zuerst in der
/// Modellmitte, Strichpunktlinie mit zwei Endsymbolen; mit der Maus quer
/// verschiebbar im 10-mm-Raster; der Schnitt folgt der Linie.
#[test]
fn a11_schnittlinie_a_a() {
    let mut s = Scene::with_model(Model::with_seed(8));
    zeichne_rechteck(&mut s, &cam3d());
    let mut sect = SectionLine::default();
    sect.ensure(&s);
    // Mitte: Außenmaß 0 … 8 m
    assert_eq!(sect.y, Some(4000.0));
    let c = cam_plan(&s);
    let lines = sect.helpers(&s, &c, H, 1.0, &Theme::dark());
    assert!(
        lines.iter().any(|h| h.pattern[0][2] > 0.0),
        "Strichpunktlinie"
    );
    let marks = sect.marks(&s, &c, W, H);
    assert_eq!(marks.len(), 2, "zwei Endsymbole");
    assert!(marks.iter().any(|m| m.left) && marks.iter().any(|m| !m.left));

    // Greifen und verschieben (nur im Grundriss)
    let (x, y) = px(&c, vec3(5000.0, 4000.0, 0.0));
    let out = sect.handle(&down(x, y), &s, &c, W, H, 1.0, true);
    assert!(out.consumed, "Linie greifbar");
    let (x2, y2) = px(&c, vec3(5000.0, 2503.0, 0.0));
    let out = sect.handle(&mv(x2, y2), &s, &c, W, H, 1.0, true);
    assert!(out.changed);
    sect.handle(&up(x2, y2), &s, &c, W, H, 1.0, true);
    let ny = sect.y.unwrap();
    assert_eq!(ny % 10.0, 0.0, "10-mm-Raster: {ny}");
    assert!((ny - 2500.0).abs() <= 10.0, "{ny}");
    // Außerhalb des Grundrisses nicht greifbar
    let mut other = SectionLine::default();
    other.ensure(&s);
    assert!(!other.handle(&down(x, y), &s, &c, W, H, 1.0, false).consumed);

    // Der Schnitt zeigt das Gebäude an der neuen Stelle: Schnittflächen bei y = ny
    let cut = view_mesh(&mut s, ViewKind::Section, sect.plane());
    let on_plane = cut
        .faces
        .iter()
        .filter(|v| v[9] != pattern::NONE)
        .all(|v| (v[1] - ny as f32).abs() < 1e-2);
    assert!(on_plane, "Schnittflächen liegen in der Ebene");
    // Davor ist nichts
    assert!(cut.faces.iter().all(|v| v[1] >= ny as f32 - 1e-2));
}

// ---------------------------------------------------------------------------
// A12 – A14: Oberfläche
// ---------------------------------------------------------------------------

/// A12: Paneel „Ansichten“ rechts mit 3D, Grundriss, Schnitt, Vorne, Hinten,
/// Links, Rechts. Alle außer 3D sind Parallelprojektionen in der richtigen
/// Blickrichtung.
#[test]
fn a12_ansichten_paneel() {
    let mut ui = Ui::new(1.0, &Theme::dark());
    ui.fit(1.0, 1440, 900);
    let r = ui.rect(Panel::Views, 1440, 32);
    assert!(r.x + r.w > 1300.0, "Ansichten rechts: {r:?}");
    let buttons = find_buttons(&mut ui, Panel::Views, 1440, 32);
    let all = [
        ViewKind::Persp,
        ViewKind::Plan,
        ViewKind::Section,
        ViewKind::Front,
        ViewKind::Back,
        ViewKind::Left,
        ViewKind::Right,
    ];
    for v in all {
        let &(_, x, y) = buttons
            .iter()
            .find(|b| b.0 == Id::View(v))
            .unwrap_or_else(|| panic!("Knopf {v:?} fehlt"));
        assert_eq!(ui_click(&mut ui, x, y, 1440, 32), Some(Id::View(v)));
    }
    let mut s = Scene::with_model(Model::with_seed(9));
    zeichne_rechteck(&mut s, &cam3d());
    let bounds = s.bounds();
    let dir = |v| fit_parallel(v, bounds, W, H).forward();
    assert!(dir(ViewKind::Plan).z < -0.999, "Grundriss von oben");
    assert!(dir(ViewKind::Front).y > 0.999, "Vorne: Blick nach +y");
    assert!(dir(ViewKind::Section).y > 0.999, "Schnitt: Blick nach +y");
    assert!(dir(ViewKind::Back).y < -0.999, "Hinten: Blick nach −y");
    assert!(dir(ViewKind::Left).x > 0.999, "Links: Blick nach +x");
    assert!(dir(ViewKind::Right).x < -0.999, "Rechts: Blick nach −x");
    for v in &all[1..] {
        let c = fit_parallel(*v, bounds, W, H);
        assert!(c.ortho.is_some(), "{v:?} parallel");
        // Das ganze Gebäude ist im Bild
        let (lo, hi) = bounds.unwrap();
        for p in [lo, hi] {
            let (x, y) = px(&c, p);
            assert!(
                (0.0..=W).contains(&x) && (0.0..=H).contains(&y),
                "{v:?}: {p:?}"
            );
        }
    }
    let (lo, hi) = bounds.unwrap();
    assert!(fit_perspective(lo, hi).ortho.is_none(), "3D perspektivisch");
}

/// A13: Paneele skalieren mit der Fenstergröße: voll ab 1440 × 810 dip,
/// kleiner darunter, höchstens auf 60 %. Die Bildschirmskalierung wirkt mit.
#[test]
fn a13_paneele_skalieren_mit_dem_fenster() {
    let mut ui = Ui::new(1.0, &Theme::dark());
    ui.fit(1.0, 1440, 810);
    assert_eq!(ui.scale, 1.0);
    ui.fit(1.0, 2560, 1440);
    assert_eq!(ui.scale, 1.0, "nie größer als voll");
    ui.fit(1.0, 1080, 900);
    assert_eq!(ui.scale, 0.75);
    ui.fit(1.0, 600, 400);
    assert_eq!(ui.scale, 0.6, "höchstens auf 60 %");
    ui.fit(1.5, 2160, 1215);
    assert_eq!(ui.scale, 1.5, "150 % Bildschirmskalierung");
    // Kleineres Fenster: Paneele schmaler, Knöpfe weiter treffbar
    ui.fit(1.0, 1440, 900);
    let full = ui.rect(Panel::Tools, 1440, 32);
    ui.fit(1.0, 1000, 600);
    let small = ui.rect(Panel::Tools, 1000, 32);
    assert!(small.w < full.w && small.h < full.h);
    let b = find_buttons(&mut ui, Panel::Views, 1000, 32);
    assert_eq!(b.iter().filter(|b| matches!(b.0, Id::View(_))).count(), 7);
    // Beide Paneele passen nebeneinander ins kleinste Fenster
    ui.fit(1.0, 600, 400);
    let (l, r) = (
        ui.rect(Panel::Tools, 600, 32),
        ui.rect(Panel::Views, 600, 32),
    );
    assert!(l.x + l.w < r.x, "Paneele überlappen: {l:?} {r:?}");
}

/// A14: Eigene Titelleiste, dunkel wie die Paneele, mit weißem Logo links und
/// den drei Fensterknöpfen rechts (Maße wie Windows 11).
#[test]
fn a14_dunkle_titelleiste_mit_weissem_logo() {
    let t = TitleBar::new(1.0);
    assert_eq!(t.height(), 32);
    assert_eq!(t.button_width(), 46);
    let w = 1440u32;
    assert_eq!(t.button_at(w as f64 - 10.0, 10.0, w), Some(Button::Close));
    assert_eq!(
        t.button_at(w as f64 - 60.0, 10.0, w),
        Some(Button::Maximize)
    );
    assert_eq!(
        t.button_at(w as f64 - 110.0, 10.0, w),
        Some(Button::Minimize)
    );
    assert_eq!(t.button_at(400.0, 10.0, w), None, "Rest zieht das Fenster");
    let th = Theme::dark();
    assert_eq!(th.title.bg, th.ui.bg);

    let c = t.paint(&th, None, w);
    let px = c.to_rgba8();
    let at = |x: usize, y: usize| {
        let i = (y * c.width + x) * 4;
        (px[i], px[i + 1], px[i + 2])
    };
    let bg = th.ui.bg;
    // Mitte der Leiste: Paneelfarbe
    assert_eq!(at(700, 16), (bg.0, bg.1, bg.2), "dunkel wie die Paneele");
    // Links: weiße Logopixel
    let white = (0..60)
        .flat_map(|x| (0..32).map(move |y| (x, y)))
        .filter(|&(x, y)| {
            let (r, g, b) = at(x, y);
            r > 240 && g > 240 && b > 240
        })
        .count();
    assert!(white > 30, "weißes Logo links ({white} Pixel)");
    // Rechts: helle Knopfsymbole auf dunklem Grund
    let glyph = (w as usize - 138..w as usize)
        .flat_map(|x| (0..32).map(move |y| (x, y)))
        .filter(|&(x, y)| at(x, y).0 > 150)
        .count();
    assert!(glyph > 10, "Fensterknöpfe gezeichnet ({glyph} Pixel)");
}

// ---------------------------------------------------------------------------
// A15 – A16: Auswahl und Mengen (Paket B4)
// ---------------------------------------------------------------------------

/// Wert eines Zahlenfelds in cm, wie er im Feld steht („0“, „2“, „2,5“).
fn field_cm(p: &crate::ui::Props, f: crate::ui::Field) -> Option<String> {
    p.fields
        .iter()
        .find(|r| r.field == f)
        .map(|r| crate::ui::cm_text(r.value))
}

fn value(p: &crate::ui::Props, k: &str) -> String {
    p.values
        .iter()
        .find(|v| v.0 == k)
        .unwrap_or_else(|| panic!("Zeile {k} fehlt"))
        .1
        .clone()
}

/// A15: Ein Klick wählt eine Wand in jeder Ansicht; das Paneel „Eigenschaften“
/// zeigt ihre Daten. Klick ins Leere hebt die Auswahl auf, nach Rückgängig des
/// Anlegens ist sie leer.
#[test]
fn a15_auswahl_per_klick_in_jeder_ansicht() {
    let mut s = Scene::with_model(Model::with_seed(10));
    let run = zeichne_rechteck(&mut s, &cam3d());
    let top = s.model().wall_at(run, 1).unwrap(); // y = 8 m, 10 m lang
                                                  // Darüber die gekoppelte OG-Wand (B12)
    let og = s.model().runs_above(run)[0];
    let top_og = s.model().wall_at(og, 1).unwrap();
    let mut sect = SectionLine::default();
    sect.ensure(&s);
    let bounds = s.bounds();
    // Je Ansicht: Kamera, Schnittebene, Punkt auf der oberen bzw. einer Wand
    type Case = (
        ViewKind,
        Camera,
        Option<(Vec3, Vec3)>,
        Vec3,
        sk_model::ElementId,
    );
    let cases: [Case; 4] = [
        // 3D von oben: die Krone der OG-Wand (Dämmung neben der OG-Decke)
        (
            ViewKind::Persp,
            cam3d(),
            None,
            vec3(5000.0, 7930.0, 5710.0),
            top_og,
        ),
        (
            ViewKind::Plan,
            fit_parallel(ViewKind::Plan, bounds, W, H),
            None,
            vec3(5000.0, 7800.0, PLAN_CUT),
            top,
        ),
        (
            ViewKind::Back,
            fit_parallel(ViewKind::Back, bounds, W, H),
            None,
            vec3(5000.0, 8000.0, 1500.0),
            top,
        ),
        (
            ViewKind::Section,
            fit_parallel(ViewKind::Section, bounds, W, H),
            sect.plane(),
            vec3(5000.0, 8000.0, 1500.0),
            top,
        ),
    ];
    for (v, c, plane, p, want) in cases {
        let (x, y) = px(&c, p);
        let mut sel = Selection::default();
        sel.press(x, y);
        assert!(sel.release(x + 1.0, y, 1.0), "{v:?}: Klick");
        let hit = selection::pick_at(&mut s, &c, v, plane, x, y, W, H);
        assert_eq!(hit, Some(want), "{v:?}: obere Wand gewählt");
        sel.set(hit);
        assert!(
            !selection::helpers(&s, want, v, plane, 1.0, &Theme::dark()).is_empty(),
            "{v:?}: Umriss"
        );
        // Klick ins Leere
        let empty = selection::pick_at(&mut s, &c, v, plane, 3.0, 3.0, W, H);
        assert_eq!(empty, None, "{v:?}: Leere");
    }

    let p = selection::props(&s, top).unwrap();
    let number = s.model().element(top).unwrap().number.clone();
    assert_eq!(value(&p, "Nummer"), number);
    assert_eq!(value(&p, "Kategorie"), "Außenwand");
    assert_eq!(value(&p, "Geschoss"), "EG");
    assert_eq!(value(&p, "Länge"), "10,00 m");
    assert_eq!(value(&p, "Dicke"), "31,5 cm");
    assert_eq!(value(&p, "Höhe"), "2,855 m");
    assert_eq!(value(&p, "Gebäude"), "GB-01");
    assert_eq!(p.layers.len(), 2);
    assert!(p
        .layers
        .iter()
        .all(|l| l.2.contains(" m³ · ") && l.2.ends_with(" kg")));

    // Rückgängig des Anlegens: Auswahl leer
    let mut sel = Selection::default();
    sel.set(Some(top));
    assert!(s.undo());
    assert!(sel.validate(&s));
    assert_eq!(sel.id, None);
}

/// A16: Mengen-Sollwerte aus Paket B4: Rechteck 10 × 8 m, AW 31,5, Höhe 2,75 m.
#[test]
fn a16_mengen_sollwerte_rechteck_und_gerade_wand() {
    let mut s = Scene::with_model(Model::with_seed(11));
    let c = cam3d();
    let mut t = tool(&s);
    for p in RECHTECK {
        click(&mut t, &c, p);
    }
    let wall = click(&mut t, &c, RECHTECK[0]).unwrap();
    let run = s.add_wall(&wall).unwrap();
    // Höhe 2,75 m: OK EG auf +2,75 (B12: Wände von UK bis OK EG)
    kante_ziehen(&mut s, "EG.OK", &[2750.0], false);
    let q: Vec<_> = (0..4)
        .map(|i| {
            s.wall_qto(s.model().wall_at(run, i).unwrap())
                .unwrap()
                .clone()
        })
        .collect();
    let m3 = |mm3: f64| mm3 / 1e9;
    let vol: f64 = q.iter().map(|q| m3(q.volume)).sum();
    let ins: f64 = q.iter().map(|q| m3(q.layers[0].volume)).sum();
    let gas: f64 = q.iter().map(|q| m3(q.layers[1].volume)).sum();
    // Seit B10 netto ohne die Auflagertasche der Erdgeschossdecke:
    // 5,9815 m² × 0,22 m = 1,31593 m³ weniger Gasbeton
    assert!((vol - 28.7776).abs() < 5e-5, "Volumen {vol}");
    assert!((ins - 13.6444).abs() < 5e-5, "Dämmung {ins}");
    assert!((gas - 15.1332).abs() < 5e-5, "Gasbeton {gas}");
    let len: f64 = q.iter().map(|q| q.length).sum();
    assert!((len - 36000.0).abs() < 1e-6, "Länge {len}");
    assert!(q.iter().all(|q| q.width == 315.0 && q.height == 2750.0));
    // Paneel: obere Wand 10 m mit Gehrung an beiden Enden
    let p = selection::props(&s, s.model().wall_at(run, 1).unwrap()).unwrap();
    assert_eq!(value(&p, "Fläche außen"), "27,50 m²");
    assert_eq!(value(&p, "Fläche innen"), "25,77 m²");

    // Gerade Wand 5 m
    let mut s = Scene::with_model(Model::with_seed(12));
    let mut t = tool(&s);
    click(&mut t, &c, vec3(0.0, 0.0, 0.0));
    click(&mut t, &c, vec3(5000.0, 0.0, 0.0));
    let wall = t.handle(&key(Key::Enter), &c, W, H, 1.0).commit.unwrap();
    let run = s.add_wall(&wall).unwrap();
    let q = s.wall_qto(s.model().wall_at(run, 0).unwrap()).unwrap();
    // Offener Zug: nur die EG-Wand von UK bis OK EG, ohne Stapel und Decke
    assert_eq!(s.model().runs().len(), 1);
    let want = 5.0 * 0.315 * 2.855;
    assert!(
        (m3(q.volume) - want).abs() < 1e-9,
        "{} ≠ {want}",
        m3(q.volume)
    );
}

// ---------------------------------------------------------------------------
// A20: Projektdatei .szo (Meilenstein M1)
// ---------------------------------------------------------------------------

/// Eigener leerer Ordner für Dateitests.
fn test_dir(name: &str) -> std::path::PathBuf {
    let d = std::env::temp_dir().join(format!("skizzeo-abnahme-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// A20: Ein gezeichnetes und bearbeitetes Haus wird als `.szo` gespeichert und
/// wieder geöffnet. Danach ist alles wie vorher: Wände, Nummern, Kennungen,
/// Mengen und Zeichnung. Die Titelleiste zeigt den Dateinamen und `•` bei
/// ungespeicherten Änderungen. Speichern ist atomar, Fehler sind lesbar.
#[test]
fn a20_szo_speichern_und_oeffnen() {
    use crate::document::{self, Document, FILTERS};
    let d = test_dir("szo");
    let mut s = Scene::with_model(Model::with_seed(20));
    let mut doc = Document::new(s.model().revision());
    assert_eq!(doc.caption(s.model()), "Unbenannt");

    // Zeichnen und eine Wand am Gummiband 1 m nach außen ziehen
    let c = cam3d();
    let run = zeichne_rechteck(&mut s, &c);
    let mut e = WallEdit::default();
    let (x, y) = px(&c, vec3(5000.0, 8000.0, 0.0));
    e.handle(&mv(x, y), &mut s, &c, W, H, 1.0, true);
    e.handle(&down(x, y), &mut s, &c, W, H, 1.0, true);
    let (x2, y2) = px(&c, vec3(5000.0, 9000.0, 0.0));
    e.handle(&mv(x2, y2), &mut s, &c, W, H, 1.0, true);
    e.handle(&up(x2, y2), &mut s, &c, W, H, 1.0, true);
    assert_eq!(doc.caption(s.model()), "Unbenannt •");

    // Speichern unter „Haus.szo“: Dateiendung .szo, keine .tmp-Reste
    assert!(FILTERS[0].1 == "*.szo", "Dialog filtert auf .szo");
    let path = d.join("Haus.szo");
    document::save(s.model(), &path).unwrap();
    doc.mark_saved(path.clone(), s.model().revision());
    assert_eq!(doc.caption(s.model()), "Haus.szo");
    assert!(path.exists() && !d.join("Haus.szo.tmp").exists());
    let bytes = std::fs::read(&path).unwrap();
    assert!(bytes.starts_with(b"SZO "), "Kopfzeile");

    // Öffnen in eine neue Sitzung
    let loaded = document::load(&path).unwrap();
    assert!(loaded.hints.is_empty(), "{:?}", loaded.hints);
    let mut t = Scene::with_model(loaded.model);
    let walls = |s: &Scene| {
        let mut v: Vec<_> = s
            .model()
            .elements()
            .iter()
            .filter(|(id, _)| s.model().segment_of(*id).is_some())
            .map(|(id, el)| {
                let q = s.wall_qto(id).unwrap();
                let vol = (q.volume / 1e3).round();
                (el.guid, el.number.clone(), (q.length * 10.0).round(), vol)
            })
            .collect();
        v.sort_by(|a, b| a.1.cmp(&b.1));
        v
    };
    // EG und gekoppeltes OG (B12)
    assert_eq!(walls(&t).len(), 8);
    assert_eq!(
        walls(&t),
        walls(&s),
        "Nummern, Guids, Längen, Volumen gleich"
    );
    let tr = t.model().runs().ids().next().unwrap();
    assert_eq!(
        t.chain(tr).unwrap().points,
        s.chain(run).unwrap().points,
        "gezogene Wand steht an der neuen Stelle"
    );
    // Gleich, wie es aussieht (die Darstellungsschlüssel hängen an den Plätzen
    // der Baustoffe und dürfen sich nach dem Öffnen unterscheiden)
    let sig = |m: Shown| {
        let mut f: Vec<_> = m.faces.iter().map(|v| v.map(|x| x.to_bits())).collect();
        f.sort();
        (f, m.edges.len())
    };
    assert_eq!(
        sig(view_mesh(&mut t, ViewKind::Plan, None)),
        sig(view_mesh(&mut s, ViewKind::Plan, None)),
        "Grundriss gleich"
    );
    assert!(t.model().check().is_empty(), "{:?}", t.model().check());
    let reopened = Document::opened(path.clone(), t.model().revision());
    assert_eq!(reopened.caption(t.model()), "Haus.szo");

    // Speichern, öffnen, speichern ergibt dieselben Bytes
    let path2 = d.join("Haus2.szo");
    document::save(t.model(), &path2).unwrap();
    assert_eq!(std::fs::read(&path2).unwrap(), bytes, "Rundlauf bytegleich");

    // Weiterarbeiten nach dem Öffnen: neue Wand bekommt eine neue Nummer
    let mut tool2 = tool(&t);
    click(&mut tool2, &c, vec3(20000.0, 0.0, 0.0));
    click(&mut tool2, &c, vec3(25000.0, 0.0, 0.0));
    let w = tool2
        .handle(&key(Key::Enter), &c, W, H, 1.0)
        .commit
        .unwrap();
    let nr = t.add_wall(&w).unwrap();
    let el = t.model().wall_at(nr, 0).unwrap();
    assert_eq!(t.model().element(el).unwrap().number, "AW-009");
    assert_eq!(reopened.caption(t.model()), "Haus.szo •");

    // Scheitert das Speichern, bleibt die alte Datei heil, Meldung nennt die Datei
    let bad = d.join("fehlt").join("Haus.szo");
    let err = document::save(t.model(), &bad).unwrap_err();
    assert!(err.contains("Haus.szo"), "{err}");
    assert_eq!(std::fs::read(&path).unwrap(), bytes);
    // Kaputte Datei: lesbare Meldung statt Absturz
    std::fs::write(d.join("kaputt.szo"), b"Hallo").unwrap();
    let err = document::load(&d.join("kaputt.szo")).err().unwrap();
    assert!(err.contains("kaputt.szo"), "{err}");

    // Start mit Datei: `skizzeo.exe Haus.szo`
    let args = ["skizzeo.exe".to_string(), path.display().to_string()];
    assert_eq!(
        document::path_from_args(args.into_iter()),
        Some(path.clone())
    );
    let _ = std::fs::remove_dir_all(&d);
}

// ---------------------------------------------------------------------------
// A21 – A28: Sohlplatte und Frostschürze (Paket B9, abnahme-sohlplatte.md)
// ---------------------------------------------------------------------------

fn sohlplatte(s: &Scene, run: RunId) -> (sk_model::ElementId, sk_model::ElementId) {
    let (slab, footing) = s.model().foundation_of(run).expect("Sohlplatte");
    (slab, footing.expect("Frostschürze"))
}

fn m2(v: f64) -> f64 {
    (v / 1e6 * 1e4).round() / 1e4
}

fn m3(v: f64) -> f64 {
    (v / 1e9 * 1e4).round() / 1e4
}

/// Kleinste und größte Höhe eines Körpers.
fn z_range(s: &sk_model::Solid) -> (f64, f64) {
    s.triangles
        .iter()
        .flat_map(|t| t.p.map(|p| p.z))
        .fold((f64::MAX, f64::MIN), |a, z| (a.0.min(z), a.1.max(z)))
}

/// A21: Ein geschlossener Zug erzeugt automatisch eine Sohlplatte unter dem
/// ganzen Polygon, ein offener keine.
#[test]
fn a21_geschlossener_zug_erzeugt_sohlplatte() {
    let mut s = Scene::with_model(Model::with_seed(21));
    let c = cam3d();
    let run = zeichne_rechteck(&mut s, &c);
    let (slab, _) = sohlplatte(&s, run);
    assert_eq!(s.model().element(slab).unwrap().number, "SP-001");
    assert_eq!(m2(s.foundation_qto(run).unwrap().0.area), 80.0);
    // Offener Zug: keine Platte
    let mut t = tool(&s);
    click(&mut t, &c, vec3(20000.0, 0.0, 0.0));
    click(&mut t, &c, vec3(25000.0, 0.0, 0.0));
    let w = t.handle(&key(Key::Enter), &c, W, H, 1.0).commit.unwrap();
    let open = s.add_wall(&w).unwrap();
    assert!(s.model().foundation_of(open).is_none());
    // Rückgängig des Rechtecks nimmt beide Bauteile mit, Wiederholen bringt dieselben
    assert!(s.undo() && s.undo());
    assert!(s.model().element(slab).is_none());
    assert!(s.redo());
    assert_eq!(sohlplatte(&s, run).0, slab);
    assert!(s.model().check().is_empty(), "{:?}", s.model().check());
}

/// A22: Sockel bündig ist Standard: Plattenaußenkante = Wandaußenseite.
#[test]
fn a22_sockel_buendig() {
    let mut s = Scene::with_model(Model::with_seed(22));
    let run = zeichne_rechteck(&mut s, &cam3d());
    let chain = s.chain(run).unwrap().clone();
    let outer = chain.face_corners(chain.outer_offset());
    let f = s.foundation(run).unwrap();
    assert_eq!(f.outline.len(), 4);
    for p in &f.outline {
        let d = outer
            .iter()
            .map(|q| (*p - *q).length())
            .fold(f64::MAX, f64::min);
        assert!(d < 1e-9, "Abweichung {d} mm");
    }
}

/// A23: Sockelrücksprung als Parameter im Paneel „Eigenschaften“.
#[test]
fn a23_sockelruecksprung() {
    let mut s = Scene::with_model(Model::with_seed(23));
    let run = zeichne_rechteck(&mut s, &cam3d());
    let (slab, _) = sohlplatte(&s, run);
    let wall = s.model().wall_at(run, 0).unwrap();
    let points = s.chain(run).unwrap().points.clone();
    // Auch bei gewählter Wand verstellbar
    let p = selection::props(&s, wall).unwrap();
    assert_eq!(field_cm(&p, crate::ui::Field::Recess).as_deref(), Some("0"));
    assert!(s.step_recess(wall, true));
    let p = selection::props(&s, slab).unwrap();
    assert_eq!(field_cm(&p, crate::ui::Field::Recess).as_deref(), Some("2"));
    assert_eq!(m2(s.foundation_qto(run).unwrap().0.area), 79.2816);
    // Die Wand bleibt an der Bezugslinie
    assert_eq!(s.chain(run).unwrap().points, points);
    assert!(s.step_recess(slab, true));
    assert_eq!(
        field_cm(
            &selection::props(&s, slab).unwrap(),
            crate::ui::Field::Recess
        )
        .as_deref(),
        Some("3")
    );
    assert!(s.step_recess(slab, false) && s.step_recess(slab, false));
    assert_eq!(
        field_cm(
            &selection::props(&s, slab).unwrap(),
            crate::ui::Field::Recess
        )
        .as_deref(),
        Some("0")
    );
    assert!(!s.step_recess(slab, false), "unter 0 geht es nicht");
    assert_eq!(m2(s.foundation_qto(run).unwrap().0.area), 80.0);
    // 1 cm wird abgelehnt
    assert!(!s.edit_model("Rücksprung", |m| m.set_slab_recess(slab, 10.0)));
    // Rückgängig stellt den Rücksprung zurück
    assert!(s.undo());
    assert_eq!(
        field_cm(
            &selection::props(&s, slab).unwrap(),
            crate::ui::Field::Recess
        )
        .as_deref(),
        Some("2")
    );
}

/// A24: Dicke parametrisch, von oben nach unten, Standard 22 cm (Jörn 10:13).
#[test]
fn a24_dicke_von_oben_nach_unten() {
    let mut s = Scene::with_model(Model::with_seed(24));
    let run = zeichne_rechteck(&mut s, &cam3d());
    let (slab, _) = sohlplatte(&s, run);
    assert_eq!(
        z_range(&s.foundation(run).unwrap().slab_solid()),
        (-220.0, 0.0)
    );
    assert_eq!(m3(s.foundation_qto(run).unwrap().0.volume), 17.6);
    s.step_recess(slab, true);
    assert_eq!(m3(s.foundation_qto(run).unwrap().0.volume), 17.442);
    s.step_recess(slab, false);
    assert!(s.edit_model("Dicke", |m| m.set_slab_thickness(slab, 250.0)));
    assert_eq!(
        z_range(&s.foundation(run).unwrap().slab_solid()),
        (-250.0, 0.0)
    );
}

/// A25: Frostschürze umlaufend unter der Platte, außen bündig, 35 × 58 cm
/// (−0,22 … −0,80, Jörn 10:13).
#[test]
fn a25_frostschuerze() {
    let mut s = Scene::with_model(Model::with_seed(25));
    let run = zeichne_rechteck(&mut s, &cam3d());
    let (slab, _) = sohlplatte(&s, run);
    let f = s.foundation(run).unwrap();
    assert_eq!(z_range(&f.footing_solid()), (-800.0, -220.0));
    let sk_model::FootingShape::Ring(inner) = &f.footing else {
        panic!("Ring erwartet");
    };
    // Innenkante 9,30 × 7,30 m
    assert_eq!(m2(sk_math::polygon::area(&inner.pts)), 67.89);
    assert_eq!(m3(s.foundation_qto(run).unwrap().1.volume), 7.0238);
    s.step_recess(slab, true);
    assert_eq!(m3(s.foundation_qto(run).unwrap().1.volume), 6.9913);
}

/// A26: Zwei getrennte Bauteile mit eigener Kategorie, Nummer und Guid.
#[test]
fn a26_zwei_getrennte_bauteile() {
    let mut s = Scene::with_model(Model::with_seed(26));
    let run = zeichne_rechteck(&mut s, &cam3d());
    let (slab, footing) = sohlplatte(&s, run);
    let m = s.model();
    assert_eq!(
        m.elements().len(),
        12,
        "4 EG- und 4 OG-Wände, Sohlplatte, Frostschürze, DE-001, DE-002"
    );
    let (a, b) = (m.element(slab).unwrap(), m.element(footing).unwrap());
    assert_eq!(
        (a.category.ifc_class(), b.category.ifc_class()),
        ("IfcSlab.BASESLAB", "IfcFooting.STRIP_FOOTING")
    );
    assert_eq!((a.number.as_str(), b.number.as_str()), ("SP-001", "FS-001"));
    assert_ne!(a.guid, b.guid);
    assert_eq!((b.seq, a.seq), (1, 2), "Bauabfolge: Schürze vor Platte");
    assert!(m.check().is_empty(), "{:?}", m.check());
    // Klick trifft das jeweilige Bauteil, im Schnitt und in 3D von unten
    let mut sect = SectionLine::default();
    sect.ensure(&s);
    let c = fit_parallel(ViewKind::Section, s.bounds(), W, H);
    let pick = |s: &mut Scene, p: Vec3| {
        let (x, y) = px(&c, p);
        selection::pick_at(s, &c, ViewKind::Section, sect.plane(), x, y, W, H)
    };
    assert_eq!(pick(&mut s, vec3(5000.0, 4000.0, -100.0)), Some(slab));
    assert_eq!(pick(&mut s, vec3(175.0, 4000.0, -500.0)), Some(footing));
    assert!(!selection::helpers(
        &s,
        footing,
        ViewKind::Section,
        sect.plane(),
        1.0,
        &Theme::dark()
    )
    .is_empty());
}

/// A27: Sohlplatte flächenbezogen, Frostschürze längenbezogen; das Paneel
/// zeigt die Hauptmenge zuerst.
#[test]
fn a27_mengen_flaeche_und_laenge() {
    let mut s = Scene::with_model(Model::with_seed(27));
    let run = zeichne_rechteck(&mut s, &cam3d());
    let (slab, footing) = sohlplatte(&s, run);
    let walls = |s: &Scene| -> f64 {
        (0..4)
            .map(|k| {
                s.wall_qto(s.model().wall_at(run, k).unwrap())
                    .unwrap()
                    .volume
            })
            .sum()
    };
    let before = walls(&s);
    let p = selection::props(&s, slab).unwrap();
    // nach Nummer, Kategorie, Geschoss, Gebäude
    assert_eq!(p.values[4], ("Fläche", "80,00 m²".to_string()));
    assert_eq!(value(&p, "Volumen"), "17,600 m³");
    let cm = |p: &crate::ui::Props, f| field_cm(p, f).unwrap();
    assert_eq!(cm(&p, crate::ui::Field::SlabThickness), "22");
    let p = selection::props(&s, footing).unwrap();
    assert_eq!(p.values[4], ("Länge (Achse)", "34,60 m".to_string()));
    assert_eq!(value(&p, "Volumen"), "7,024 m³");
    assert_eq!(
        (
            cm(&p, crate::ui::Field::FootingWidth),
            cm(&p, crate::ui::Field::FootingDepth)
        ),
        ("35".into(), "58".into())
    );
    assert_eq!(p.layer_set, "Stahlbeton");
    s.step_recess(slab, true);
    let p = selection::props(&s, footing).unwrap();
    assert_eq!(value(&p, "Länge (Achse)"), "34,44 m");
    let p = selection::props(&s, slab).unwrap();
    assert_eq!(value(&p, "Fläche"), "79,28 m²");
    // Wandmengen ändern sich durch Platte und Rücksprung nicht (A16)
    assert_eq!(walls(&s), before);
}

/// A28: Im Schnitt Stahlbeton-Schraffur auf Platte und Schürze, kräftige
/// Kontur wie tragendes Mauerwerk, keine Fuge zwischen Platte und Schürze;
/// die Fuge zur Wand bleibt.
#[test]
fn a28_schnitt_stahlbeton_ohne_fuge() {
    let mut s = Scene::with_model(Model::with_seed(28));
    zeichne_rechteck(&mut s, &cam3d());
    let mut sect = SectionLine::default();
    sect.ensure(&s);
    let cut = view_mesh(&mut s, ViewKind::Section, sect.plane());
    let has = |pat: f32| cut.faces.iter().any(|v| v[9] == pat);
    assert!(has(pattern::CONCRETE) && has(pattern::DIAGONAL) && has(pattern::ZIGZAG));
    let cross_below = cut
        .faces
        .iter()
        .filter(|v| v[9] == pattern::CONCRETE)
        .all(|v| {
            v[2] <= 1e-3
                || (2635.0 - 1e-3..=2855.0 + 1e-3).contains(&v[2])
                || (5490.0 - 1e-3..=5710.0 + 1e-3).contains(&v[2])
        });
    assert!(
        cross_below,
        "Stahlbeton-Schraffur nur in der Gründung und in den Bändern der Decken (B10, B12)"
    );
    let cut_w = s.table().edge_width(true, edge_kind::CUT);
    // Waagerechte Kanten in der Schnittebene auf Höhe z zwischen x0 und x1
    let y = sect.y.unwrap() as f32;
    let flat_at = |z: f32, x0: f32, x1: f32| {
        cut.edges
            .iter()
            .filter(|e| (e.0[0][1] - y).abs() < 1e-2 && (e.0[1][1] - y).abs() < 1e-2)
            .filter(|e| (e.0[0][2] - z).abs() < 1e-3 && (e.0[1][2] - z).abs() < 1e-3)
            .filter(|e| e.0[0][0].min(e.0[1][0]) < x1 - 1.0 && e.0[0][0].max(e.0[1][0]) > x0 + 1.0)
            .map(|e| e.1)
            .collect::<Vec<f32>>()
    };
    assert!(
        flat_at(-220.0, 0.0, 350.0).is_empty(),
        "keine Fuge Platte/Schürze"
    );
    // Kontur außen kräftig: UK Schürze und UK Platte zwischen den Schürzen
    assert!(flat_at(-800.0, 0.0, 350.0).iter().all(|w| *w == cut_w));
    assert!(!flat_at(-800.0, 0.0, 350.0).is_empty());
    assert!(flat_at(-220.0, 400.0, 9600.0).contains(&cut_w));
    // Fuge zur Wand (OK Platte, unter dem Gasbeton) bleibt
    assert!(!flat_at(0.0, 140.0, 315.0).is_empty(), "Linie Wand/Platte");
}

// ---------------------------------------------------------------------------
// A29: Innenwände mit T-Anschluss (Paket B5a)
// ---------------------------------------------------------------------------

/// Haus 10 × 8 m mit der Maus, Höhe 2,75 m wie in Paket B5a.
fn haus_275(s: &mut Scene, c: &Camera) -> RunId {
    let mut t = tool(s);
    for p in RECHTECK {
        click(&mut t, c, p);
    }
    let w = click(&mut t, c, RECHTECK[0]).unwrap();
    let run = s.add_wall(&w).unwrap();
    // B12: die Wände reichen bis OK EG
    kante_ziehen(s, "EG.OK", &[2750.0], false);
    run
}

/// Innenwand mit dem Werkzeug von `a` nach `b`, Enter beendet, Höhe 2,75 m.
fn innenwand(s: &mut Scene, c: &Camera, a: Vec3, b: Vec3) -> RunId {
    use sk_model::Category;
    let mut t = tool(s);
    let set = s.model().defaults().interior_wall;
    t.set_category(Category::InteriorWall, s.model().wall_layers(set));
    assert_eq!(t.ref_side, RefSide::Center, "Innenwand: Bezug Achse");
    click(&mut t, c, a);
    click(&mut t, c, b);
    let w = t.handle(&key(Key::Enter), c, W, H, 1.0).commit.unwrap();
    s.add_wall_as(&w, Category::InteriorWall).unwrap()
}

fn wall_m3(s: &Scene, run: RunId, seg: usize) -> f64 {
    s.wall_qto(s.model().wall_at(run, seg).unwrap())
        .unwrap()
        .volume
        / 1e9
}

/// A29: Knopf „Innenwand“ zeichnet eine 17,5-cm-Gasbetonwand, die mit
/// T-Anschluss an die Außenwand stößt: Volumen netto, Gasbeton ohne Fuge,
/// die Innenwand geht beim Gummiband mit, Rückgängig und Speichern/Öffnen
/// behalten alles. Seit B10 steht die Innenwand unter der Erdgeschossdecke
/// (OK 1,83 bei 2,75 m Wandhöhe, 22 cm): netto Höhe 2,53 m, die Außenwand
/// ohne Tasche (−1,31593 m³).
#[test]
fn a29_innenwand_mit_t_anschluss() {
    use sk_model::join::JoinKind;
    // Knopf im Paneel
    let mut ui = Ui::new(1.0, &Theme::dark());
    ui.fit(1.0, 1440, 900);
    let b = find_buttons(&mut ui, Panel::Tools, 1440, 32);
    let &(_, x, y) = b
        .iter()
        .find(|b| b.0 == Id::Interior)
        .expect("Knopf Innenwand");
    assert_eq!(ui_click(&mut ui, x, y, 1440, 32), Some(Id::Interior));

    let c = cam3d();
    for (from, to) in [(315.0, 7685.0), (0.0, 8000.0)] {
        let mut s = Scene::with_model(Model::with_seed(29));
        let aw = haus_275(&mut s, &c);
        let iw = innenwand(&mut s, &c, vec3(5000.0, from, 0.0), vec3(5000.0, to, 0.0));
        let ctx = format!("Innenwand {from}…{to}");
        let id = s.model().wall_at(iw, 0).unwrap();
        assert_eq!(s.model().element(id).unwrap().number, "IW-001", "{ctx}");
        let joins = s.model().joins();
        assert_eq!(joins.len(), 2, "{ctx}: {joins:?}");
        assert!(joins.iter().all(|j| j.kind == JoinKind::T), "{ctx}");
        assert!(
            (wall_m3(&s, iw, 0) - 3.2630675).abs() < 1e-6,
            "{ctx}: {}",
            wall_m3(&s, iw, 0)
        );
        let aw_sum: f64 = (0..4).map(|i| wall_m3(&s, aw, i)).sum();
        assert!((aw_sum - 28.7776).abs() < 5e-5, "{ctx}: AW {aw_sum}");
        let gas: f64 = (0..4)
            .map(|i| {
                let q = s.wall_qto(s.model().wall_at(aw, i).unwrap()).unwrap();
                q.layers[1].volume / 1e9
            })
            .sum::<f64>()
            + wall_m3(&s, iw, 0);
        assert!((gas - 18.3962625).abs() < 5e-5, "{ctx}: Gasbeton {gas}");
        assert!(
            s.model().check().is_empty(),
            "{ctx}: {:?}",
            s.model().check()
        );

        // Grundriss: keine Kante quer über den Anschluss an der Innenfläche
        let plan = s.mesh(ViewKind::Plan, None, &[]);
        for yi in [315.0f32, 7685.0] {
            let seam = plan.edges.iter().any(|(p, _)| {
                (p[0][1] - yi).abs() < 0.5
                    && (p[1][1] - yi).abs() < 0.5
                    && p[0][0].min(p[1][0]) < 5000.0
                    && p[0][0].max(p[1][0]) > 5000.0
            });
            assert!(!seam, "{ctx}: Fuge bei y = {yi}");
            // Gegenprobe: Neben dem Anschluss ist die Innenkante durchgezogen
            let wall_edge = plan.edges.iter().any(|(p, _)| {
                (p[0][1] - yi).abs() < 0.5
                    && (p[1][1] - yi).abs() < 0.5
                    && p[0][0].min(p[1][0]) < 2500.0
                    && p[0][0].max(p[1][0]) > 2500.0
            });
            assert!(wall_edge, "{ctx}: Innenkante bei y = {yi} fehlt");
        }
    }

    // Lücke über 50 mm: kein Anschluss
    let mut s = Scene::with_model(Model::with_seed(30));
    haus_275(&mut s, &c);
    let iw = innenwand(
        &mut s,
        &c,
        vec3(5000.0, 1000.0, 0.0),
        vec3(5000.0, 7000.0, 0.0),
    );
    assert!(s.model().joins().is_empty());
    assert!((wall_m3(&s, iw, 0) - 2.6565).abs() < 1e-6);

    // Gummiband: obere Außenwand 1 m nach außen, Innenwand geht mit
    let mut s = Scene::with_model(Model::with_seed(31));
    let aw = haus_275(&mut s, &c);
    let iw = innenwand(
        &mut s,
        &c,
        vec3(5000.0, 0.0, 0.0),
        vec3(5000.0, 8000.0, 0.0),
    );
    let mut e = WallEdit::default();
    let (x, y) = px(&c, vec3(2500.0, 8000.0, 0.0));
    e.handle(&mv(x, y), &mut s, &c, W, H, 1.0, true);
    e.handle(&down(x, y), &mut s, &c, W, H, 1.0, true);
    let (x2, y2) = px(&c, vec3(2500.0, 9000.0, 0.0));
    e.handle(&mv(x2, y2), &mut s, &c, W, H, 1.0, true);
    e.handle(&up(x2, y2), &mut s, &c, W, H, 1.0, true);
    assert!((s.chain(aw).unwrap().points[1].y - 9000.0).abs() < 1e-6);
    assert!(
        (wall_m3(&s, iw, 0) - 3.7058175).abs() < 1e-6,
        "{}",
        wall_m3(&s, iw, 0)
    );
    let ends = &s.chain(iw).unwrap().points;
    assert!(ends.iter().any(|p| (p.y - 8685.0).abs() < 1e-6), "{ends:?}");
    assert!(s.model().check().is_empty(), "{:?}", s.model().check());

    // Speichern und Öffnen: dieselben Anschlüsse und Mengen, bytegleich
    let d = test_dir("innenwand");
    let path = d.join("Haus.szo");
    crate::document::save(s.model(), &path).unwrap();
    let t = Scene::with_model(crate::document::load(&path).unwrap().model);
    assert_eq!(t.model().joins().len(), 2);
    let tiw = t
        .model()
        .elements()
        .iter()
        .find(|(_, el)| el.number == "IW-001")
        .map(|(id, _)| id)
        .unwrap();
    assert!((t.wall_qto(tiw).unwrap().volume / 1e9 - 3.7058175).abs() < 1e-6);
    let p2 = d.join("Haus2.szo");
    crate::document::save(t.model(), &p2).unwrap();
    assert_eq!(std::fs::read(&p2).unwrap(), std::fs::read(&path).unwrap());
    let _ = std::fs::remove_dir_all(&d);

    // Rückgängig stellt beide Züge zurück
    assert!(s.undo());
    assert!((s.chain(aw).unwrap().points[1].y - 8000.0).abs() < 1e-6);
    assert!(
        (wall_m3(&s, iw, 0) - 3.2630675).abs() < 1e-6,
        "{}",
        wall_m3(&s, iw, 0)
    );
}

// ---------------------------------------------------------------------------
// A30: Gründung im Zusammenspiel (Gummiband, Innenwand, Speichern)
// ---------------------------------------------------------------------------

/// A30: Die Gründung folgt dem Haus. Gummiband → Platte und Schürze wachsen
/// mit, Rückgängig stellt sie zurück; eine Innenwand erzeugt keine eigene
/// Gründung; Rücksprung, Nummern und Guids überstehen Speichern/Öffnen.
#[test]
fn a30_gruendung_folgt_dem_haus() {
    let mut s = Scene::with_model(Model::with_seed(30));
    let c = cam3d();
    let run = zeichne_rechteck(&mut s, &c);
    let (slab, footing) = sohlplatte(&s, run);
    let area = |s: &Scene| m2(s.foundation_qto(run).unwrap().0.area);
    let axis = |s: &Scene| (s.foundation_qto(run).unwrap().1.length / 10.0).round() / 100.0;

    // Gummiband: obere Wand 1 m nach außen → Platte 10 × 9 m, Schürze 36,60 m
    let mut e = WallEdit::default();
    let (x, y) = px(&c, vec3(2500.0, 8000.0, 0.0));
    e.handle(&mv(x, y), &mut s, &c, W, H, 1.0, true);
    e.handle(&down(x, y), &mut s, &c, W, H, 1.0, true);
    let (x2, y2) = px(&c, vec3(2500.0, 9000.0, 0.0));
    e.handle(&mv(x2, y2), &mut s, &c, W, H, 1.0, true);
    e.handle(&up(x2, y2), &mut s, &c, W, H, 1.0, true);
    assert_eq!(area(&s), 90.0);
    assert_eq!(axis(&s), 36.6, "Achse 2 × (9,65 + 8,65)");
    assert_eq!(sohlplatte(&s, run), (slab, footing), "dieselben Bauteile");
    assert!(s.undo());
    assert_eq!(area(&s), 80.0);
    assert_eq!(axis(&s), 34.6);

    // Innenwand quer durch: keine weitere Gründung
    innenwand(
        &mut s,
        &c,
        vec3(5000.0, 0.0, 0.0),
        vec3(5000.0, 8000.0, 0.0),
    );
    let n = |s: &Scene| {
        s.model()
            .elements()
            .iter()
            .filter(|(_, el)| el.number.starts_with("SP-") || el.number.starts_with("FS-"))
            .count()
    };
    assert_eq!(n(&s), 2, "nur Platte und Schürze des Hauses");

    // Rücksprung 2 cm, speichern, öffnen
    assert!(s.step_recess(slab, true));
    let d = test_dir("gruendung");
    let path = d.join("Haus.szo");
    crate::document::save(s.model(), &path).unwrap();
    let loaded = crate::document::load(&path).unwrap();
    assert!(loaded.hints.is_empty(), "{:?}", loaded.hints);
    let t = Scene::with_model(loaded.model);
    let find = |s: &Scene, nr: &str| {
        s.model()
            .elements()
            .iter()
            .find(|(_, el)| el.number == nr)
            .map(|(id, el)| (id, el.guid))
            .unwrap_or_else(|| panic!("{nr} fehlt"))
    };
    let (tslab, g) = find(&t, "SP-001");
    assert_eq!(g, s.model().element(slab).unwrap().guid);
    assert_eq!(
        find(&t, "FS-001").1,
        s.model().element(footing).unwrap().guid
    );
    assert_eq!(
        field_cm(
            &selection::props(&t, tslab).unwrap(),
            crate::ui::Field::Recess
        )
        .as_deref(),
        Some("2")
    );
    let trun = t
        .model()
        .runs()
        .ids()
        .find(|r| t.model().foundation_of(*r).is_some());
    let tq = t.foundation_qto(trun.unwrap()).unwrap();
    assert_eq!(m2(tq.0.area), 79.2816);
    assert_eq!(m3(tq.1.volume), 6.9913);
    assert_eq!(n(&t), 2);
    assert!(t.model().check().is_empty(), "{:?}", t.model().check());
    let p2 = d.join("Haus2.szo");
    crate::document::save(t.model(), &p2).unwrap();
    assert_eq!(std::fs::read(&p2).unwrap(), std::fs::read(&path).unwrap());
    let _ = std::fs::remove_dir_all(&d);
}

// ---------------------------------------------------------------------------
// A31 (G2): großer Sockelrücksprung an kurzem Vorsprung
// ---------------------------------------------------------------------------

/// Rechteck 10 × 8 m mit einem Vorsprung `b` × `v` mm an der Unterseite
/// (ab x = 4000). Gezeichnet wird das Rechteck mit der Maus; die Ecken des
/// Vorsprungs liegen nur wenige Bildpunkte auseinander und werden direkt
/// in den Zug eingesetzt.
fn haus_mit_vorsprung(s: &mut Scene, b: f64, v: f64) -> RunId {
    let c = cam3d();
    let mut t = tool(s);
    for p in RECHTECK {
        assert!(click(&mut t, &c, p).is_none());
    }
    let mut w = click(&mut t, &c, RECHTECK[0]).expect("Klick auf den Startpunkt schließt");
    w.points.extend([
        vec3(4000.0 + b, 0.0, 0.0),
        vec3(4000.0 + b, -v, 0.0),
        vec3(4000.0, -v, 0.0),
        vec3(4000.0, 0.0, 0.0),
    ]);
    s.add_wall(&w).expect("Wandzug angelegt")
}

/// A31 (G2): Großer, erlaubter Sockelrücksprung (30 cm bei AW 31,5) an einem
/// kurzen Vorsprung. Platte und Schürze bleiben vorhanden, die Mengen stimmen.
#[test]
fn a31_grosser_ruecksprung_an_kurzem_vorsprung() {
    // (Breite, Tiefe, Platte m², Schürzenachse m) bei 30 cm Rücksprung.
    // Schmale Vorsprünge (≤ 60 cm) verschwinden in der Platte: 9,40 × 7,40.
    // Der breite behält einen eingerückten Rest von 1,40 × 0,20 m.
    for (b, v, flaeche, achse) in [
        (50.0, 50.0, 69.56, 32.2),
        (200.0, 200.0, 69.56, 32.2),
        (2000.0, 200.0, 69.84, 32.6),
    ] {
        let mut s = Scene::with_model(Model::with_seed(31));
        let run = haus_mit_vorsprung(&mut s, b, v);
        let (slab, footing) = sohlplatte(&s, run);
        let buendig = m2(80e6 + b * v);
        let (sp, fs) = s.foundation_qto(run).unwrap();
        let fs_vol = fs.volume;
        assert_eq!(m2(sp.area), buendig, "bündig, {b} × {v}");
        assert!(s.edit_model("Rücksprung", |m| m.set_slab_recess(slab, 300.0)));
        let fehler = s.model().check();
        assert!(fehler.is_empty(), "{b} × {v}: {fehler:?}");
        // Gleiche Bauteile, beide mit Körper
        assert_eq!(s.model().foundation_of(run), Some((slab, Some(footing))));
        for id in [slab, footing] {
            let p = selection::props(&s, id).unwrap();
            assert!(
                !format!("{p:?}").contains("Kein Körper"),
                "{b} × {v}: {p:?}"
            );
        }
        let (sp, fs) = s.foundation_qto(run).unwrap();
        assert_eq!(m2(sp.area), flaeche, "Platte, {b} × {v}");
        assert_eq!(m3(sp.volume), m3(flaeche * 1e6 * 220.0), "Plattenvolumen");
        assert_eq!(
            (fs.length / 10.0).round() / 100.0,
            achse,
            "Schürzenachse, {b} × {v}"
        );
        assert!(
            fs.volume > 0.0 && fs.volume < fs_vol,
            "Schürzenvolumen, {b} × {v}"
        );
        // Rückgängig: wieder bündig
        assert!(s.undo());
        assert_eq!(m2(s.foundation_qto(run).unwrap().0.area), buendig);
    }
}

// ---------------------------------------------------------------------------
// A32–A38: Erdgeschossdecke (Paket B10, Geometrie G3)
// Sollwerte: bim/paket-b10-decke.md „Fertig, wenn“ und
// test/abnahme-erdgeschossdecke.md.
//
// Vorbereitet vor dem Einbau. Die Zugriffe auf die neue API stehen nur in den
// fünf Hilfsfunktionen direkt hier unten; Namen bitte beim Einbau an die
// tatsächliche B10/G3-API anpassen, die Tests selbst nicht.
//
// Achtung beim Einbau: A28 prüft „Stahlbeton-Schraffur nur in der Gründung“
// (z ≤ 0). Mit der Decke gibt es Stahlbeton-Schraffur auch bei +2,635 … +2,855;
// die Bedingung in A28 dann auf `v[2] <= 1e-3 || (2635..=2855).contains(z)`
// erweitern.
// ---------------------------------------------------------------------------

/// Erdgeschossdecke des Zuges. API-Annahme: `Model::floor_of(run)`.
fn decke(s: &Scene, run: RunId) -> Option<sk_model::ElementId> {
    s.model().floor_of(run)
}

/// Fläche m², Volumen m³, Umfang m. API-Annahme: `Scene::floor_qto(run)`.
fn decke_mengen(s: &Scene, run: RunId) -> (f64, f64, f64) {
    let q = s.floor_qto(run).expect("Deckenmengen");
    (
        m2(q.area),
        m3(q.volume),
        (q.perimeter / 10.0).round() / 100.0,
    )
}

/// Unterkante und Oberkante des Deckenkörpers. API-Annahme: `Scene::floor(run).solid()`.
fn decke_hoehen(s: &Scene, run: RunId) -> (f64, f64) {
    z_range(&s.floor(run).expect("Deckenkörper").solid())
}

/// Grundriss des Deckenkörpers (x min, x max, y min, y max).
fn decke_umriss(s: &Scene, run: RunId) -> (f64, f64, f64, f64) {
    let solid = s.floor(run).expect("Deckenkörper").solid();
    let mut r = (f64::MAX, f64::MIN, f64::MAX, f64::MIN);
    for p in solid.triangles.iter().flat_map(|t| t.p) {
        r = (r.0.min(p.x), r.1.max(p.x), r.2.min(p.y), r.3.max(p.y));
    }
    r
}

/// Dicke ändern, ein Rückgängig-Schritt. API-Annahme: `Model::set_floor_thickness`.
fn decke_dicke(s: &mut Scene, id: sk_model::ElementId, t: f64) -> bool {
    s.edit_model("Deckendicke", |m| m.set_floor_thickness(id, t))
}

/// Summe einer Schicht (0 = Dämmung, 1 = Gasbeton) über die vier Außenwände, m³.
fn aw_schicht(s: &Scene, run: RunId, layer: usize) -> f64 {
    (0..4)
        .map(|i| {
            s.wall_qto(s.model().wall_at(run, i).unwrap())
                .unwrap()
                .layers[layer]
                .volume
                / 1e9
        })
        .sum()
}

fn anzahl(s: &Scene, prefix: &str) -> usize {
    s.model()
        .elements()
        .iter()
        .filter(|(_, el)| el.number.starts_with(prefix))
        .count()
}

/// A32: Ein geschlossener Außenwandzug bekommt genau eine Erdgeschossdecke
/// (DE-001) im selben Schritt wie die Gründung; offener Zug und Innenwand
/// erzeugen keine.
#[test]
fn a32_decke_entsteht_mit_dem_zug() {
    let c = cam3d();
    let mut s = Scene::with_model(Model::with_seed(32));
    let run = zeichne_rechteck(&mut s, &c);
    let id = decke(&s, run).expect("Decke angelegt");
    assert_eq!(s.model().element(id).unwrap().number, "DE-001");
    assert_eq!(
        (anzahl(&s, "SP-"), anzahl(&s, "FS-"), anzahl(&s, "DE-")),
        (1, 1, 2),
        "DE-001 im EG, DE-002 im OG"
    );
    assert!(s.model().check().is_empty(), "{:?}", s.model().check());

    // Innenwand quer durch: keine weitere Decke
    let set = s.model().defaults().interior_wall;
    let mut t = tool(&s);
    t.set_category(sk_model::Category::InteriorWall, s.model().wall_layers(set));
    click(&mut t, &c, vec3(5000.0, 0.0, 0.0));
    click(&mut t, &c, vec3(5000.0, 8000.0, 0.0));
    let w = t.handle(&key(Key::Enter), &c, W, H, 1.0).commit.unwrap();
    s.add_wall_as(&w, sk_model::Category::InteriorWall).unwrap();
    assert_eq!(anzahl(&s, "DE-"), 2);

    // Rückgängig bis vor das Schließen: Decke und Gründung weg
    assert!(s.undo() && s.undo());
    assert_eq!(
        (anzahl(&s, "SP-"), anzahl(&s, "FS-"), anzahl(&s, "DE-")),
        (0, 0, 0)
    );

    // Offener Zug: keine Decke
    let mut s = Scene::with_model(Model::with_seed(33));
    let mut t = tool(&s);
    for p in &RECHTECK[..3] {
        click(&mut t, &c, *p);
    }
    let w = t.handle(&key(Key::Enter), &c, W, H, 1.0).commit.unwrap();
    let run = s.add_wall(&w).unwrap();
    assert!(decke(&s, run).is_none());
    assert_eq!(anzahl(&s, "DE-"), 0);
}

/// A33: Dicke 22 cm von der Oberkante +2,855 (OK EG, B11) nach unten, veränderbar.
#[test]
fn a33_dicke_von_oben_nach_unten() {
    let mut s = Scene::with_model(Model::with_seed(34));
    let run = zeichne_rechteck(&mut s, &cam3d());
    let id = decke(&s, run).unwrap();
    assert_eq!(decke_mengen(&s, run), (75.0384, 16.5084, 34.88));
    assert_eq!(decke_hoehen(&s, run), (2635.0, 2855.0));
    // 25 cm: Oberkante bleibt, Decke und Tasche wachsen nach unten
    assert!(decke_dicke(&mut s, id, 250.0));
    assert_eq!(decke_hoehen(&s, run), (2605.0, 2855.0));
    assert_eq!(decke_mengen(&s, run).1, 18.7596);
    let netto = aw_schicht(&s, run, 1);
    assert!(
        (netto - (17.0771825 - 1.495375)).abs() < 5e-5,
        "Gasbeton {netto}"
    );
    // Rückgängig: wieder 22 cm
    assert!(s.undo());
    assert_eq!(decke_hoehen(&s, run), (2635.0, 2855.0));
    assert_eq!(decke_mengen(&s, run).1, 16.5084);
}

/// A34: Auflagertasche über die ganze tragende Schicht bis an das WDVS;
/// die Dämmung läuft durch, die Innenwand wird unterbrochen.
#[test]
fn a34_auflagertasche_bis_ans_wdvs() {
    let c = cam3d();
    let mut s = Scene::with_model(Model::with_seed(35));
    let run = zeichne_rechteck(&mut s, &c);
    // Umriss = Außenseite Gasbeton = WDVS-Innenseite, 140 mm innen
    let (x0, x1, y0, y1) = decke_umriss(&s, run);
    for (ist, soll) in [(x0, 140.0), (x1, 9860.0), (y0, 140.0), (y1, 7860.0)] {
        assert!((ist - soll).abs() < 1e-6, "Umriss {ist} statt {soll}");
    }
    let gas = aw_schicht(&s, run, 1);
    assert!((gas - 15.761252).abs() < 5e-5, "Gasbeton netto {gas}");
    let daemmung = aw_schicht(&s, run, 0);
    assert!((daemmung - 14.165368).abs() < 5e-5, "Dämmung {daemmung}");
    // Keine Doppelzählung: netto + Tasche = brutto
    assert!((gas + 1.31593 - 17.0771825).abs() < 5e-5);

    // Innenwand IW 17,5 bei x = 5 m, bis UK Decke: netto ohne Deckenstreifen
    let set = s.model().defaults().interior_wall;
    let mut t = tool(&s);
    t.set_category(sk_model::Category::InteriorWall, s.model().wall_layers(set));
    click(&mut t, &c, vec3(5000.0, 0.0, 0.0));
    click(&mut t, &c, vec3(5000.0, 8000.0, 0.0));
    let w = t.handle(&key(Key::Enter), &c, W, H, 1.0).commit.unwrap();
    let iw = s.add_wall_as(&w, sk_model::Category::InteriorWall).unwrap();
    assert!(
        (wall_m3(&s, iw, 0) - 3.398491).abs() < 5e-5,
        "IW {}",
        wall_m3(&s, iw, 0)
    );
    assert_eq!(anzahl(&s, "IW-"), 1, "ein Bauteil, zwei Körperteile");
    assert!(s.model().check().is_empty(), "{:?}", s.model().check());
}

/// A35: Schnitt mit Stahlbeton-Schraffur in der Tasche und kräftiger
/// Kontur; im Grundriss (+1,00) wird die Decke nicht gezeichnet.
#[test]
fn a35_darstellung_schnitt_und_grundriss() {
    let mut s = Scene::with_model(Model::with_seed(36));
    zeichne_rechteck(&mut s, &cam3d());
    let mut sect = SectionLine::default();
    sect.ensure(&s);
    let cut = view_mesh(&mut s, ViewKind::Section, sect.plane());
    let in_band = |z: f32| (2635.0 - 1e-3..=2855.0 + 1e-3).contains(&z);
    // Stahlbeton-Schraffur im Deckenband, auch in der Tasche (x 140 … 315)
    let cross: Vec<_> = cut
        .faces
        .iter()
        .filter(|v| v[9] == pattern::CONCRETE && in_band(v[2]))
        .collect();
    assert!(!cross.is_empty(), "Decke im Schnitt");
    assert!(
        cross.iter().any(|v| v[0] > 139.0 && v[0] < 316.0),
        "Stahlbeton-Schraffur in der Tasche"
    );
    // Kontur kräftig an OK und UK Decke
    let cut_w = s.table().edge_width(true, edge_kind::CUT);
    let y = sect.y.unwrap() as f32;
    for z in [2635.0f32, 2855.0] {
        let w: Vec<f32> = cut
            .edges
            .iter()
            .filter(|e| (e.0[0][1] - y).abs() < 1e-2 && (e.0[1][1] - y).abs() < 1e-2)
            .filter(|e| (e.0[0][2] - z).abs() < 1e-3 && (e.0[1][2] - z).abs() < 1e-3)
            .map(|e| e.1)
            .collect();
        assert!(w.contains(&cut_w), "Kontur bei z = {z}: {w:?}");
    }
    // Grundriss bei +1,00: keine Stahlbeton-Schraffur
    let plan = view_mesh(&mut s, ViewKind::Plan, None);
    assert!(
        !plan.faces.iter().any(|v| v[9] == pattern::CONCRETE),
        "Decke liegt über der Schnittebene"
    );
}

/// A36: Gummiband zieht die Decke mit, Rückgängig stellt beide zurück.
#[test]
fn a36_decke_folgt_dem_gummiband() {
    let c = cam3d();
    let mut s = Scene::with_model(Model::with_seed(37));
    let run = zeichne_rechteck(&mut s, &c);
    let id = decke(&s, run).unwrap();
    let mut e = WallEdit::default();
    let (x, y) = px(&c, vec3(2500.0, 8000.0, 0.0));
    e.handle(&mv(x, y), &mut s, &c, W, H, 1.0, true);
    e.handle(&down(x, y), &mut s, &c, W, H, 1.0, true);
    let (x2, y2) = px(&c, vec3(2500.0, 9000.0, 0.0));
    e.handle(&mv(x2, y2), &mut s, &c, W, H, 1.0, true);
    e.handle(&up(x2, y2), &mut s, &c, W, H, 1.0, true);
    assert_eq!(decke_mengen(&s, run).0, 84.7584);
    assert_eq!(decke(&s, run), Some(id), "dieselbe Decke");
    assert!(s.undo());
    assert_eq!(decke_mengen(&s, run).0, 75.0384);
}

/// A37: Speichern und Öffnen: Decke mit Dicke, Nummer und Guid, bytegleich.
#[test]
fn a37_decke_speichern_und_oeffnen() {
    let mut s = Scene::with_model(Model::with_seed(38));
    let run = zeichne_rechteck(&mut s, &cam3d());
    let id = decke(&s, run).unwrap();
    assert!(decke_dicke(&mut s, id, 250.0));
    let d = test_dir("decke");
    let path = d.join("Haus.szo");
    crate::document::save(s.model(), &path).unwrap();
    let text = std::fs::read_to_string(&path).unwrap();
    assert_eq!(text.lines().filter(|l| l.starts_with("[floor]")).count(), 2);
    let loaded = crate::document::load(&path).unwrap();
    assert!(loaded.hints.is_empty(), "{:?}", loaded.hints);
    let t = Scene::with_model(loaded.model);
    let trun = t
        .model()
        .runs()
        .ids()
        .find(|r| t.foundation(*r).is_some())
        .unwrap();
    let tid = decke(&t, trun).unwrap();
    assert_eq!(t.model().element(tid).unwrap().number, "DE-001");
    assert_eq!(
        t.model().element(tid).unwrap().guid,
        s.model().element(id).unwrap().guid
    );
    assert_eq!(decke_hoehen(&t, trun), (2605.0, 2855.0));
    assert_eq!(decke_mengen(&t, trun).1, 18.7596);
    let p2 = d.join("Haus2.szo");
    crate::document::save(t.model(), &p2).unwrap();
    assert_eq!(std::fs::read(&p2).unwrap(), std::fs::read(&path).unwrap());
    let _ = std::fs::remove_dir_all(&d);
}

/// A38: Eine Datei ohne Decke (Stand vor B10) bekommt beim Öffnen DE-001
/// mit OK +2,855 (OK EG) und einen Hinweis; Wände und Gründung bleiben unverändert.
#[test]
fn a38_alte_datei_bekommt_decke() {
    let mut s = Scene::with_model(Model::with_seed(39));
    let run = zeichne_rechteck(&mut s, &cam3d());
    let (slab, footing) = sohlplatte(&s, run);
    let d = test_dir("decke-alt");
    let path = d.join("Alt.szo");
    crate::document::save(s.model(), &path).unwrap();
    let alt: String = std::fs::read_to_string(&path)
        .unwrap()
        .lines()
        .filter(|l| !l.starts_with("[floor]"))
        .map(|l| format!("{l}\n"))
        .collect();
    std::fs::write(&path, alt).unwrap();
    let loaded = crate::document::load(&path).unwrap();
    assert!(
        loaded
            .hints
            .iter()
            .any(|h| h.contains("Geschossdecken über 2 Außenwandzügen ergänzt")),
        "{:?}",
        loaded.hints
    );
    let t = Scene::with_model(loaded.model);
    let trun = t
        .model()
        .runs()
        .ids()
        .find(|r| t.foundation(*r).is_some())
        .expect("Decke ergänzt");
    assert_eq!(decke_hoehen(&t, trun), (2635.0, 2855.0));
    assert_eq!(decke_mengen(&t, trun).0, 75.0384);
    let guid = |s: &Scene, id| s.model().element(id).unwrap().guid;
    let (tslab, tfooting) = sohlplatte(&t, trun);
    assert_eq!(
        (guid(&t, tslab), guid(&t, tfooting)),
        (guid(&s, slab), guid(&s, footing))
    );
    assert!((aw_schicht(&t, trun, 1) - 15.761252).abs() < 5e-5);
    assert!(t.model().check().is_empty(), "{:?}", t.model().check());
    let _ = std::fs::remove_dir_all(&d);
}

// ---------------------------------------------------------------------------
// P1: Zahlenfelder im Paneel „Eigenschaften“
// ---------------------------------------------------------------------------

/// Tippt `text` in das Feld `f` des Paneels „Eigenschaften“ und drückt Enter;
/// eine gültige Eingabe geht wie in der App als ein Schritt ins Modell.
fn tippe(
    ui: &mut Ui,
    s: &mut Scene,
    sel: sk_model::ElementId,
    f: crate::ui::Field,
    text: &str,
) -> crate::ui::UiOut {
    ui.set_props(selection::props(s, sel));
    let b = find_buttons(ui, Panel::Props, 1440, 32);
    let &(_, x, y) = b
        .iter()
        .find(|b| b.0 == Id::Field(f))
        .unwrap_or_else(|| panic!("Feld {f:?}"));
    ui.handle(&down(x, y), 1440, 32);
    ui.handle(&up(x, y), 1440, 32);
    assert!(ui.edit.is_some(), "Eingabe beginnt mit dem Klick");
    for c in text.chars() {
        ui.key(Key::Char(c), true, M).unwrap();
    }
    let out = ui.key(Key::Enter, true, M).unwrap();
    if let Some((field, mm)) = out.submit {
        assert!(s.set_field(sel, field, mm));
        ui.set_props(selection::props(s, sel));
    }
    out
}

/// P1: Dicke der Sohlplatte, Sockelrücksprung, Breite und Tiefe der
/// Frostschürze als Zahlenfelder: Tippen und Enter ändern das Modell in
/// einem Schritt, Ungültiges bleibt mit Hinweis stehen, Esc bricht ab.
#[test]
fn p1_zahlenfelder_der_gruendung() {
    use crate::ui::Field;
    let mut s = Scene::with_model(Model::with_seed(41));
    let run = zeichne_rechteck(&mut s, &cam3d());
    let (slab, footing) = sohlplatte(&s, run);
    let mut ui = Ui::new(1.0, &Theme::dark());
    ui.fit(1.0, 1440, 900);

    // Dicke 25 cm: wächst nach unten, 20 m³
    tippe(&mut ui, &mut s, slab, Field::SlabThickness, "25");
    assert_eq!(m3(s.foundation_qto(run).unwrap().0.volume), 20.0);
    assert_eq!(
        z_range(&s.foundation(run).unwrap().slab_solid()),
        (-250.0, 0.0)
    );
    assert_eq!(s.undo_label(), Some("Plattendicke"));

    // Rücksprung 1 cm: abgelehnt mit Hinweis, das Modell bleibt
    let out = tippe(&mut ui, &mut s, slab, Field::Recess, "1");
    assert!(out.submit.is_none() && out.relayout);
    let e = ui.edit.as_ref().unwrap();
    assert_eq!(e.error.as_deref(), Some("0 oder mindestens 2 cm"));
    assert_eq!(m2(s.foundation_qto(run).unwrap().0.area), 80.0);
    // Esc bricht ab
    let out = ui.key(Key::Escape, true, M).unwrap();
    assert!(ui.edit.is_none() && out.relayout && out.submit.is_none());
    // 2,5 cm mit Komma
    tippe(&mut ui, &mut s, slab, Field::Recess, "2,5");
    assert_eq!(m2(s.foundation_qto(run).unwrap().0.area), 79.1025);

    // Schürze 40 cm breit und 80 cm tief: Achse 2 × (9,55 + 7,55), UK −1,05 m
    tippe(&mut ui, &mut s, footing, Field::FootingWidth, "40");
    tippe(&mut ui, &mut s, footing, Field::FootingDepth, "80");
    let fq = &s.foundation_qto(run).unwrap().1;
    assert_eq!((fq.width, fq.depth), (400.0, 800.0));
    assert_eq!((fq.length / 10.0).round() / 100.0, 34.2);
    assert_eq!(
        z_range(&s.foundation(run).unwrap().footing_solid()).0,
        -1050.0
    );
    assert!(s.model().check().is_empty(), "{:?}", s.model().check());

    // Jede Eingabe ist ein Schritt: viermal Rückgängig = Ausgangslage
    for _ in 0..4 {
        assert!(s.undo());
    }
    let q = s.foundation_qto(run).unwrap();
    assert_eq!((m2(q.0.area), m3(q.0.volume)), (80.0, 17.6));
    assert_eq!((q.1.width, q.1.depth), (350.0, 580.0));
}
// ---------------------------------------------------------------------------
// A39–A47: Geschossbänder (Pakete B11, E14, G4; Lastenheft H-01–H-07, A-09)
// Sollwerte: bim/paket-b11-ebenen.md „Fertig, wenn“ nach Jörns Festlegung
// vom 06.10. 08:13 (von Hand nachgerechnet):
//   Gründung −0,80 … ±0,00 · EG ±0,00 … +2,855 · OG +2,855 … +5,71
//   lichte Höhe EG 2,635 · Decke UK +2,635 / OK +2,855 · Wände je Geschoss
//   (B12) · Platte 22, Schürze 58 cm (Jörn 10:13)
//
// Vorbereitet vor dem Einbau. Setzt die Decke (A32–A38, vorbereitet/a32-a38-decke.rs)
// voraus und nutzt deren Hilfsfunktionen decke, decke_mengen, decke_hoehen,
// decke_dicke, aw_schicht. Die Zugriffe auf die neue B11-API stehen nur in den
// Hilfsfunktionen direkt hier unten; Namen beim Einbau anpassen, Tests nicht.
// ---------------------------------------------------------------------------

/// Geschoss nach Kurzname („GR“, „EG“, „OG“). API-Annahme: `Storey::short`.
fn geschoss(s: &Scene, short: &str) -> sk_model::StoreyId {
    s.model()
        .storeys()
        .iter()
        .find(|(_, st)| st.short == short)
        .map(|(id, _)| id)
        .unwrap_or_else(|| panic!("Geschoss {short} fehlt"))
}

/// Unter- und Oberkante eines Geschossbands in mm (elevation, elevation + height).
fn band(s: &Scene, short: &str) -> (f64, f64) {
    let st = s.model().storey(geschoss(s, short)).unwrap();
    (st.elevation, st.elevation + st.height)
}

/// Eine Kante im Paneel ziehen: greifen, je Bild ein Wert, loslassen oder Esc.
/// `kante` ist „GR.UK“ oder „EG.OK“/„OG.OK“. API-Annahmen:
/// `Scene::drag_foundation_bottom(z)`, `Scene::drag_storey_top(id, z)`
/// (beide klemmen und bauen das Live-Netz).
fn kante_ziehen(s: &mut Scene, kante: &str, bilder: &[f64], esc: bool) {
    s.begin("Geschoss ziehen");
    for &z in bilder {
        match kante {
            "GR.UK" => s.drag_foundation_bottom(z),
            "EG.OK" => s.drag_storey_top(geschoss(s, "EG"), z),
            "OG.OK" => s.drag_storey_top(geschoss(s, "OG"), z),
            _ => unreachable!(),
        }
    }
    if esc {
        s.rollback();
    } else {
        s.commit();
    }
}

/// Lichte Raumhöhe EG in mm. API-Annahme: `Model::clear_height`.
fn lichte_hoehe(s: &Scene) -> f64 {
    s.model().clear_height(geschoss(s, "EG"))
}

/// Zahleneingabe im Paneel: ein Rückgängig-Schritt, `false` = abgelehnt.
/// API-Annahmen: set_storey_height, set_clear_height, set_foundation_depth,
/// set_storey_top.
fn mass_eingeben(s: &mut Scene, mass: &str, wert: f64) -> bool {
    let eg = geschoss(s, "EG");
    let gr = geschoss(s, "GR");
    match mass {
        "Geschosshöhe EG" => s.edit_model("Geschosshöhe", |m| m.set_storey_height(eg, wert)),
        "lichte Höhe" => s.edit_model("lichte Höhe", |m| m.set_clear_height(eg, wert)),
        "Gründungstiefe" => s.edit_model("Gründungstiefe", |m| m.set_foundation_depth(wert)),
        "OK Gründung" => s.edit_model("OK Gründung", |m| m.set_storey_top(gr, wert)),
        _ => unreachable!(),
    }
}

/// Paneel „Geschosse“. API-Annahme: `Panel::Levels`.
const PANEEL_GESCHOSSE: Panel = Panel::Levels;

/// Prüfhaus aus B11: Rechteck, Innenwand bei x = 5 m, Gründung und Decke.
fn haus_b11(s: &mut Scene) -> (RunId, RunId) {
    let c = cam3d();
    let aw = zeichne_rechteck(s, &c);
    let set = s.model().defaults().interior_wall;
    let mut t = tool(s);
    t.set_category(sk_model::Category::InteriorWall, s.model().wall_layers(set));
    click(&mut t, &c, vec3(5000.0, 0.0, 0.0));
    click(&mut t, &c, vec3(5000.0, 8000.0, 0.0));
    let w = t.handle(&key(Key::Enter), &c, W, H, 1.0).commit.unwrap();
    let iw = s.add_wall_as(&w, sk_model::Category::InteriorWall).unwrap();
    (aw, iw)
}

/// Alle Mengen des Prüfhauses, gerundet (m³): Dämmung, Gasbeton netto,
/// Innenwand, Decke, Sohlplatte, Frostschürze.
fn mengen_b11(s: &Scene, aw: RunId, iw: RunId) -> [f64; 6] {
    let r = |v: f64| (v * 1e4).round() / 1e4;
    let (sp, fs) = s.foundation_qto(aw).unwrap();
    [
        r(aw_schicht(s, aw, 0)),
        r(aw_schicht(s, aw, 1)),
        r(wall_m3(s, iw, 0)),
        decke_mengen(s, aw).1,
        m3(sp.volume),
        m3(fs.volume),
    ]
}

/// Der Innenwandzug im EG neben dem Außenwandzug `aw` (nicht gestapelt).
fn innenzug(s: &Scene, aw: RunId) -> RunId {
    s.model()
        .runs()
        .ids()
        .find(|r| *r != aw && s.model().run_below(*r).is_none())
        .expect("Innenwand")
}

const STANDARD: [f64; 6] = [14.1654, 15.7613, 3.3985, 16.5084, 17.6, 7.0238];

/// Mengen des Prüfhauses mit OK EG +3,00: die EG-Wände werden höher.
const EG_300: [f64; 6] = [14.8848, 16.6286, 3.5855, 16.5084, 17.6, 7.0238];

/// Höchster Punkt des Modells in 3D (OK OG-Decke = Krone der OG-Wände).
fn wandkrone(s: &mut Scene) -> f64 {
    let m = s.mesh(ViewKind::Persp, None, &[]);
    m.faces.iter().map(|v| v[2] as f64).fold(f64::MIN, f64::max)
}

/// A39 (H-02, H-07, A-09): Drei Geschossbänder lückenlos übereinander,
/// Decke an OK EG, Gründung an UK Gründung, Wände je Geschoss von UK bis OK.
#[test]
fn a39_geschossbaender_und_bindung() {
    let mut s = Scene::with_model(Model::with_seed(39));
    let (aw, iw) = haus_b11(&mut s);
    assert_eq!(band(&s, "GR"), (-800.0, 0.0));
    assert_eq!(band(&s, "EG"), (0.0, 2855.0));
    assert_eq!(band(&s, "OG"), (2855.0, 5710.0));
    assert_eq!(lichte_hoehe(&s), 2635.0);
    assert!(s.model().check().is_empty(), "{:?}", s.model().check());
    assert_eq!(mengen_b11(&s, aw, iw), STANDARD);
    assert_eq!(decke_hoehen(&s, aw), (2635.0, 2855.0));
    let f = s.foundation(aw).unwrap();
    assert_eq!(z_range(&f.slab_solid()), (-220.0, 0.0));
    assert_eq!(z_range(&f.footing_solid()), (-800.0, -220.0));
    assert!(
        (wandkrone(&mut s) - 5710.0).abs() < 1e-2,
        "Krone an OK OG-Decke"
    );
    // Bindung (A-09) am Verhalten: OK EG bewegt Decke, EG-Wände und das
    // OG-Band; UK Gründung bewegt nur die Frostschürze.
    kante_ziehen(&mut s, "EG.OK", &[3000.0], false);
    assert_eq!(decke_hoehen(&s, aw), (2780.0, 3000.0));
    assert_eq!(band(&s, "OG"), (3000.0, 5855.0), "OG-Band wandert mit");
    assert_eq!(mengen_b11(&s, aw, iw), EG_300, "EG-Wände 3,00 hoch");
    assert!(s.undo());
    kante_ziehen(&mut s, "GR.UK", &[-900.0], false);
    let m = mengen_b11(&s, aw, iw);
    assert_eq!(&m[..5], &STANDARD[..5]);
    assert!(m[5] > STANDARD[5]);
    assert_eq!(decke_hoehen(&s, aw), (2635.0, 2855.0));
    assert!(s.undo());
    assert_eq!(mengen_b11(&s, aw, iw), STANDARD);
}

/// A40 (H-03, H-06, G4): OK EG ziehen, live ab dem ersten Bild, ein Schritt;
/// Esc bricht ab; Klemmen an der lichten Höhe 1,00; OK OG ändert nur die
/// OG-Höhe.
#[test]
fn a40_ok_eg_ziehen_live_klemmen_rueckgaengig() {
    let mut s = Scene::with_model(Model::with_seed(40));
    let (aw, iw) = haus_b11(&mut s);
    let eg = geschoss(&s, "EG");
    s.begin("Geschoss ziehen");
    s.drag_storey_top(eg, 2900.0);
    assert_eq!(
        decke_hoehen(&s, aw),
        (2680.0, 2900.0),
        "live im ersten Bild"
    );
    s.drag_storey_top(eg, 3000.0);
    s.commit();
    assert_eq!((band(&s, "EG"), lichte_hoehe(&s)), ((0.0, 3000.0), 2780.0));
    assert_eq!(mengen_b11(&s, aw, iw), EG_300);
    assert!(s.model().check().is_empty(), "{:?}", s.model().check());
    assert!(s.undo(), "ein Schritt für das ganze Ziehen");
    assert_eq!(band(&s, "EG"), (0.0, 2855.0));
    assert_eq!(decke_hoehen(&s, aw), (2635.0, 2855.0));
    // Esc: nichts geändert, nichts im Verlauf
    kante_ziehen(&mut s, "EG.OK", &[3100.0, 3200.0], true);
    assert_eq!(band(&s, "EG"), (0.0, 2855.0));
    // Keine Wandkrone mehr: +3,60 wird angenommen; Klemmen nur an lichter Höhe 1,00
    kante_ziehen(&mut s, "EG.OK", &[3400.0, 3600.0], false);
    assert_eq!(band(&s, "EG").1, 3600.0);
    assert_eq!(decke_hoehen(&s, aw), (3380.0, 3600.0));
    assert!(s.model().check().is_empty(), "{:?}", s.model().check());
    assert!(s.undo());
    kante_ziehen(&mut s, "EG.OK", &[2000.0, 1100.0], false);
    assert_eq!(band(&s, "EG").1, 1220.0);
    assert!(s.undo());
    // OK OG ziehen: nur die OG-Höhe ändert sich
    kante_ziehen(&mut s, "OG.OK", &[6000.0], false);
    assert_eq!(band(&s, "OG"), (2855.0, 6000.0));
    assert_eq!(band(&s, "EG"), (0.0, 2855.0));
    assert_eq!(mengen_b11(&s, aw, iw), STANDARD);
}

/// A41 (H-04, H-05): Maßzahlen numerisch ändern.
#[test]
fn a41_masszahlen_eingeben() {
    let mut s = Scene::with_model(Model::with_seed(41));
    let (aw, iw) = haus_b11(&mut s);
    assert!(mass_eingeben(&mut s, "lichte Höhe", 2500.0));
    assert_eq!(band(&s, "EG").1, 2720.0, "OK EG = 2,50 + 0,22");
    assert_eq!(decke_hoehen(&s, aw), (2500.0, 2720.0));
    assert!(s.undo());
    assert!(mass_eingeben(&mut s, "Geschosshöhe EG", 3100.0));
    assert_eq!((band(&s, "EG").1, lichte_hoehe(&s)), (3100.0, 2880.0));
    assert!(s.undo());
    assert!(mass_eingeben(&mut s, "Gründungstiefe", 900.0));
    assert_eq!(band(&s, "GR"), (-900.0, 0.0));
    assert_eq!(
        m3(s.foundation_qto(aw).unwrap().1.volume),
        m3(12.11e6 * 680.0)
    );
    assert!(s.undo());
    // Deckendicke 25 cm: OK EG bleibt, lichte Höhe sinkt
    let d = decke(&s, aw).unwrap();
    assert!(decke_dicke(&mut s, d, 250.0));
    assert_eq!((band(&s, "EG").1, lichte_hoehe(&s)), (2855.0, 2605.0));
    assert_eq!(decke_hoehen(&s, aw), (2605.0, 2855.0));
    let m = mengen_b11(&s, aw, iw);
    assert_eq!((m[1], m[2], m[3]), (15.5818, 3.3598, 18.7596));
}

/// A42 (H-07): UK Gründung und Plattendicke steuern die Frostschürze.
#[test]
fn a42_gruendung_und_plattendicke() {
    let mut s = Scene::with_model(Model::with_seed(42));
    let (aw, iw) = haus_b11(&mut s);
    kante_ziehen(&mut s, "GR.UK", &[-820.0, -850.0], false);
    let m = mengen_b11(&s, aw, iw);
    assert_eq!((m[4], m[5]), (17.6, 7.6293), "Schürze 0,63 tief");
    assert_eq!(
        z_range(&s.foundation(aw).unwrap().footing_solid()),
        (-850.0, -220.0)
    );
    assert!(s.undo());
    kante_ziehen(&mut s, "GR.UK", &[-500.0, -250.0], false);
    assert_eq!(band(&s, "GR").0, -320.0, "klemmt bei Schürze 0,10");
    assert!(s.undo());
    // Plattendicke 25 cm: UK Gründung bleibt, Schürze wird kürzer
    let (slab, footing) = sohlplatte(&s, aw);
    assert!(s.edit_model("Dicke", |m| m.set_slab_thickness(slab, 250.0)));
    assert_eq!(band(&s, "GR").0, -800.0);
    let m = mengen_b11(&s, aw, iw);
    assert_eq!((m[4], m[5]), (20.0, 6.6605));
    assert!(s.undo());
    // Schürzentiefe als Zahl verschiebt UK Gründung (API-Annahme set_footing_depth)
    assert!(s.edit_model("Tiefe", |m| m.set_footing_depth(footing, 700.0)));
    assert_eq!(band(&s, "GR").0, -920.0);
    assert!(s.model().check().is_empty(), "{:?}", s.model().check());
}

/// A43 (G4): Zahleneingaben außerhalb der Grenzen werden abgelehnt; ±0,00
/// liegt fest.
#[test]
fn a43_grenzen_ablehnen() {
    let mut s = Scene::with_model(Model::with_seed(43));
    let (aw, iw) = haus_b11(&mut s);
    assert!(!mass_eingeben(&mut s, "OK Gründung", 100.0), "±0,00 fest");
    assert!(!mass_eingeben(&mut s, "lichte Höhe", 900.0));
    assert!(
        !mass_eingeben(&mut s, "Geschosshöhe EG", 1100.0),
        "lichte Höhe 0,88 < 1,00"
    );
    assert!(!mass_eingeben(&mut s, "Gründungstiefe", 250.0));
    let d = decke(&s, aw).unwrap();
    assert!(!decke_dicke(&mut s, d, 2000.0), "Decke ≤ OK EG − 1,00");
    let (slab, _) = sohlplatte(&s, aw);
    assert!(!s.edit_model("Dicke", |m| m.set_slab_thickness(slab, 750.0)));
    assert_eq!(
        (band(&s, "GR"), band(&s, "EG")),
        ((-800.0, 0.0), (0.0, 2855.0))
    );
    assert_eq!(mengen_b11(&s, aw, iw), STANDARD);
}

/// Datei im Format vor B11 (SZO 1, Wände 3,50 m, ohne Decke), gespeichert mit
/// main ded8144: Rechteck, Innenwand bei x = 5 m, Gründung.
const HAUS_SZO1: &str = r##"SZO 1
# Skizzeo-Projekt
[pen] guid=195nZUDNvDg8j3N8vqOMUM nr=5 name="Hintergrund weiß" color=ffffff w=0
[pen] guid=1dTTnt0LPFOPhyr4qKpul_ nr=6 name="3D-Kante" color=000000 w=0.23
[pen] guid=1lcsskRqn7gAh2pXUbUKex nr=8 name="Schnittlinie Enden" color=000000 w=0.58
[pen] guid=1nkrJOqG55kODD2$_G5GA0 nr=3 name="Kräftig" color=000000 w=0.5
[pen] guid=1qTi_AIwfDm8UpGTQGrwAA nr=7 name="Schnittlinie" color=000000 w=0.22
[pen] guid=2H2YtiYG9CmRxhZQ5bZknd nr=1 name="Fein" color=000000 w=0.13
[pen] guid=3WcUnirpPCfONdkmyI9uLr nr=4 name="Schraffur" color=000000 w=0.18
[pen] guid=3uawBk_p95Nh71Xf3kGiaB nr=2 name="Mittel" color=000000 w=0.3
[linetype] guid=2bDAQcj$rBOv2wqDfNAwhn name="Volllinie" pat=-
[fill] guid=0GuiHePUb7HfJNbpnSAaIS name="Mauerwerk" space=paper kind=lines lines=45:1.27:0
[fill] guid=1_yVqEqLHFpP_446C_ymQi name="Dämmung hart" space=paper kind=zigzag period=1
[fill] guid=2kX3UMCAz9xg9Z63TpxsK8 name="Leer" space=paper kind=empty
[fill] guid=3asJ0o8JT0Swcc40HXxTLw name="Stahlbeton" space=paper kind=lines lines=45:1.27:0;135:1.27:0
[surface] guid=0BCnP5H5jD9xziP_WP2NUA name="Dämmung (WDVS)" color=f4efdc cut=e8c45c
[surface] guid=19SmN5qQf9dun3G7t1TxRt name="Gasbeton" color=eeede8 cut=b0b1ae
[surface] guid=1bhE9eMWTCRR1XRoz8tE0S name="Stahlbeton" color=d6d6d2 cut=969694
[surface] guid=2BBW94Naj0zR_CMRi0DLF1 name="Putz" color=f0eee8 cut=c8c6c0
[display] slot=drawing.view pen=3uawBk_p95Nh71Xf3kGiaB lt=2bDAQcj$rBOv2wqDfNAwhn
[display] slot=drawing.cut pen=1nkrJOqG55kODD2$_G5GA0 lt=2bDAQcj$rBOv2wqDfNAwhn
[display] slot=drawing.fine pen=2H2YtiYG9CmRxhZQ5bZknd lt=2bDAQcj$rBOv2wqDfNAwhn
[display] slot=drawing.cut_layer pen=3uawBk_p95Nh71Xf3kGiaB lt=2bDAQcj$rBOv2wqDfNAwhn
[display] slot=model3d.view pen=1dTTnt0LPFOPhyr4qKpul_ lt=2bDAQcj$rBOv2wqDfNAwhn
[display] slot=model3d.cut pen=1dTTnt0LPFOPhyr4qKpul_ lt=2bDAQcj$rBOv2wqDfNAwhn
[display] slot=model3d.fine pen=2H2YtiYG9CmRxhZQ5bZknd lt=2bDAQcj$rBOv2wqDfNAwhn
[display] slot=model3d.cut_layer pen=1dTTnt0LPFOPhyr4qKpul_ lt=2bDAQcj$rBOv2wqDfNAwhn
[display] slot=ground pen=1nkrJOqG55kODD2$_G5GA0 lt=2bDAQcj$rBOv2wqDfNAwhn
[display] slot=section_line pen=1qTi_AIwfDm8UpGTQGrwAA lt=2bDAQcj$rBOv2wqDfNAwhn
[display] slot=section_ends pen=1lcsskRqn7gAh2pXUbUKex lt=2bDAQcj$rBOv2wqDfNAwhn
[display] slot=paper color=f5f4ef
[material] guid=10re9EBlDC5uUUBY9M$lyC name="Stahlbeton" cat=concrete prio=900 rho=2500 lambda=- fill=3asJ0o8JT0Swcc40HXxTLw fg=3WcUnirpPCfONdkmyI9uLr bg=195nZUDNvDg8j3N8vqOMUM surface=1bhE9eMWTCRR1XRoz8tE0S
[material] guid=23_HodXaf1DRQvhkyDBVUh name="Gasbeton" cat=masonry prio=800 rho=350 lambda=- fill=0GuiHePUb7HfJNbpnSAaIS fg=3WcUnirpPCfONdkmyI9uLr bg=195nZUDNvDg8j3N8vqOMUM surface=19SmN5qQf9dun3G7t1TxRt
[material] guid=2P3DSB4iL0X9Oh6MV91ucw name="Dämmung (WDVS)" cat=insulation prio=300 rho=20 lambda=- fill=1_yVqEqLHFpP_446C_ymQi fg=3WcUnirpPCfONdkmyI9uLr bg=195nZUDNvDg8j3N8vqOMUM surface=0BCnP5H5jD9xziP_WP2NUA
[material] guid=3HQgIobkjD6AideN6_MnCy name="Putz" cat=plaster prio=100 rho=1400 lambda=- fill=2kX3UMCAz9xg9Z63TpxsK8 fg=3WcUnirpPCfONdkmyI9uLr bg=195nZUDNvDg8j3N8vqOMUM surface=2BBW94Naj0zR_CMRi0DLF1
[layerset] guid=1J3WvX6Eb8NgY$_UemJP_M name="IW 17,5 Gasbeton"
[layer] set=1J3WvX6Eb8NgY$_UemJP_M mat=23_HodXaf1DRQvhkyDBVUh t=175 fn=loadbearing core=1
[layerset] guid=3SeCT9O7vCXhLBF4287Ya7 name="AW 31,5 Gasbeton + WDVS"
[layer] set=3SeCT9O7vCXhLBF4287Ya7 mat=2P3DSB4iL0X9Oh6MV91ucw t=140 fn=insulation core=0
[layer] set=3SeCT9O7vCXhLBF4287Ya7 mat=23_HodXaf1DRQvhkyDBVUh t=175 fn=loadbearing core=1
[project] guid=3USD41dyP0WPRUQiqIo7uu name="Projekt" storey=0yWX_$MH11OwV$3JY6X$_o wallset=3SeCT9O7vCXhLBF4287Ya7 iwset=1J3WvX6Eb8NgY$_UemJP_M
[storey] guid=0yWX_$MH11OwV$3JY6X$_o name="EG" elev=0 height=3500
[run] guid=1DHdFlTxf5TA6lZFrCJBxb storey=0yWX_$MH11OwV$3JY6X$_o ref=left h=3500 closed=1 pts="0 0.0000000000018189894035458565;0 7999.999999999996;10000 7999.999999999996;10000 0.0000000000018189894035458565"
[run] guid=2uR9gODPvBOeUft6SvCb_i storey=0yWX_$MH11OwV$3JY6X$_o ref=center h=3500 closed=0 pts="5000.000000000001 0;5000.000000000001 7999.999999999996"
[wall] guid=09bKMqodD0yuMYuOb1mvDh run=2uR9gODPvBOeUft6SvCb_i seg=0 number="IW-001" cat=interior set=1J3WvX6Eb8NgY$_UemJP_M storey=0yWX_$MH11OwV$3JY6X$_o
[wall] guid=1JO03qlMf8_B2lFEGfoXUG run=1DHdFlTxf5TA6lZFrCJBxb seg=0 number="AW-001" cat=exterior set=3SeCT9O7vCXhLBF4287Ya7 storey=0yWX_$MH11OwV$3JY6X$_o
[wall] guid=1anmiB35jAZvPrXoSdKUol run=1DHdFlTxf5TA6lZFrCJBxb seg=1 number="AW-002" cat=exterior set=3SeCT9O7vCXhLBF4287Ya7 storey=0yWX_$MH11OwV$3JY6X$_o
[wall] guid=2RPvo5cirAhwVDNvx8nfJC run=1DHdFlTxf5TA6lZFrCJBxb seg=2 number="AW-003" cat=exterior set=3SeCT9O7vCXhLBF4287Ya7 storey=0yWX_$MH11OwV$3JY6X$_o
[wall] guid=3rLK2ol$12Kg0eKf9QJSXI run=1DHdFlTxf5TA6lZFrCJBxb seg=3 number="AW-004" cat=exterior set=3SeCT9O7vCXhLBF4287Ya7 storey=0yWX_$MH11OwV$3JY6X$_o
[slab] guid=26nT6mNE9E5960iZeGTRT$ run=1DHdFlTxf5TA6lZFrCJBxb number="SP-001" cat=groundslab mat=10re9EBlDC5uUUBY9M$lyC t=200 recess=0 seq=2 storey=0yWX_$MH11OwV$3JY6X$_o
[footing] guid=30cXg1UHHFl8YubE507j9R slab=26nT6mNE9E5960iZeGTRT$ number="FS-001" cat=stripfooting mat=10re9EBlDC5uUUBY9M$lyC w=350 d=600 seq=1 storey=0yWX_$MH11OwV$3JY6X$_o
"##;

/// A44 (H-07, F-03): Alte Datei öffnen → auf Geschosse umgestellt, Hinweis;
/// speichern schreibt SZO 2, erneut öffnen und speichern bytegleich.
#[test]
fn a44_alte_datei_wird_umgestellt() {
    let d = test_dir("geschosse-alt");
    // Ohne Decke (vor B10), mit Decke bei der alten Vorbelegung +2,33, und mit
    // Wänden 2,75 (die gespeicherte Wandhöhe wird seit B12 verworfen)
    let floor = "[floor] guid=3DeckeB10Pruefung00001 run=1DHdFlTxf5TA6lZFrCJBxb \
                 number=\"DE-001\" cat=floor mat=10re9EBlDC5uUUBY9M$lyC t=220 top=2330 \
                 seq=4 storey=0yWX_$MH11OwV$3JY6X$_o\n";
    let niedrig = HAUS_SZO1.replace(" h=3500 ", " h=2750 ");
    for (name, text, ok_eg) in [
        ("ohne-decke", HAUS_SZO1.to_string(), 2855.0),
        ("mit-decke", format!("{HAUS_SZO1}{floor}"), 2855.0),
        ("niedrig", niedrig, 2855.0),
    ] {
        let path = d.join(format!("{name}.szo"));
        std::fs::write(&path, &text).unwrap();
        let loaded = crate::document::load(&path).unwrap();
        assert!(
            loaded
                .hints
                .iter()
                .any(|h| h.contains("auf Gebäude umgestellt, Obergeschoss ergänzt")),
            "{name}: {:?}",
            loaded.hints
        );
        let s = Scene::with_model(loaded.model);
        assert_eq!(band(&s, "GR"), (-800.0, 0.0), "{name}");
        assert_eq!(band(&s, "EG"), (0.0, ok_eg), "{name}");
        assert_eq!(band(&s, "OG"), (2855.0, 5710.0), "{name}");
        assert_eq!((anzahl(&s, "AW-"), anzahl(&s, "DE-")), (8, 2), "{name}");
        let aw = s
            .model()
            .runs()
            .ids()
            .find(|r| s.foundation(*r).is_some())
            .expect("EG-Zug");
        let iw = innenzug(&s, aw);
        assert_eq!(decke_hoehen(&s, aw), (ok_eg - 220.0, ok_eg), "{name}");
        // Alte Dateien behalten ihre Gründung: Platte 20, Schürze 60 cm
        let mut alt = STANDARD;
        (alt[4], alt[5]) = (16.0, 7.266);
        assert_eq!(mengen_b11(&s, aw, iw), alt, "{name}");
        assert!(
            s.model().check().is_empty(),
            "{name}: {:?}",
            s.model().check()
        );
        let p2 = d.join(format!("{name}-2.szo"));
        crate::document::save(s.model(), &p2).unwrap();
        let neu = std::fs::read_to_string(&p2).unwrap();
        assert!(neu.starts_with("SZO 4"), "{name}");
        let t = Scene::with_model(crate::document::load(&p2).unwrap().model);
        let p3 = d.join(format!("{name}-3.szo"));
        crate::document::save(t.model(), &p3).unwrap();
        assert_eq!(
            std::fs::read(&p3).unwrap(),
            neu.into_bytes(),
            "{name}: bytegleich"
        );
    }
    let _ = std::fs::remove_dir_all(&d);
}

/// A45 (F-03): Geänderte Geschosse überstehen Speichern und Öffnen.
#[test]
fn a45_geschosse_speichern_und_oeffnen() {
    let mut s = Scene::with_model(Model::with_seed(45));
    let (aw, iw) = haus_b11(&mut s);
    kante_ziehen(&mut s, "EG.OK", &[3000.0], false);
    kante_ziehen(&mut s, "GR.UK", &[-850.0], false);
    let vorher = mengen_b11(&s, aw, iw);
    let d = test_dir("geschosse");
    let path = d.join("Haus.szo");
    crate::document::save(s.model(), &path).unwrap();
    let loaded = crate::document::load(&path).unwrap();
    assert!(loaded.hints.is_empty(), "{:?}", loaded.hints);
    let t = Scene::with_model(loaded.model);
    assert_eq!(band(&t, "GR"), (-850.0, 0.0));
    assert_eq!(band(&t, "EG"), (0.0, 3000.0));
    assert_eq!(band(&t, "OG"), (3000.0, 5855.0));
    for short in ["GR", "EG", "OG"] {
        let a = s.model().storey(geschoss(&s, short)).unwrap().guid;
        assert_eq!(
            t.model().storey(geschoss(&t, short)).unwrap().guid,
            a,
            "Guid {short}"
        );
    }
    let taw = t
        .model()
        .runs()
        .ids()
        .find(|r| t.foundation(*r).is_some())
        .unwrap();
    let tiw = innenzug(&t, taw);
    assert_eq!(mengen_b11(&t, taw, tiw), vorher);
    assert_eq!(decke_hoehen(&t, taw), (2780.0, 3000.0));
    let p2 = d.join("Haus2.szo");
    crate::document::save(t.model(), &p2).unwrap();
    assert_eq!(std::fs::read(&p2).unwrap(), std::fs::read(&path).unwrap());
    let _ = std::fs::remove_dir_all(&d);
}

/// A46 (H-06): Schnitt und Ansicht folgen: Decke im Schnitt bei OK EG,
/// oben endet das Haus an OK OG-Decke.
#[test]
fn a46_ansichten_folgen() {
    let mut s = Scene::with_model(Model::with_seed(46));
    haus_b11(&mut s);
    let mut sect = SectionLine::default();
    sect.ensure(&s);
    let top = |m: &Shown| m.faces.iter().map(|v| v[2]).fold(f32::MIN, f32::max);
    let cross_top = |m: &Shown| {
        m.faces
            .iter()
            .filter(|v| v[9] == pattern::CONCRETE)
            .map(|v| v[2])
            .fold(f32::MIN, f32::max)
    };
    let concrete_in = |m: &Shown, lo: f32, hi: f32| {
        m.faces
            .iter()
            .any(|v| v[9] == pattern::CONCRETE && v[2] > lo - 1e-2 && v[2] < hi + 1e-2)
    };
    let cut = view_mesh(&mut s, ViewKind::Section, sect.plane());
    assert!((top(&cut) - 5710.0).abs() < 1e-2, "OK OG-Decke im Schnitt");
    assert!((cross_top(&cut) - 5710.0).abs() < 1e-2, "DE-002 im Schnitt");
    assert!(concrete_in(&cut, 2635.0, 2855.0), "DE-001 im Schnitt");
    kante_ziehen(&mut s, "EG.OK", &[3200.0], false);
    let cut = view_mesh(&mut s, ViewKind::Section, sect.plane());
    assert!((top(&cut) - 6055.0).abs() < 1e-2);
    assert!(
        (cross_top(&cut) - 6055.0).abs() < 1e-2,
        "DE-002 wandert mit auf +6,055"
    );
    assert!(concrete_in(&cut, 2980.0, 3200.0), "DE-001 folgt auf +3,20");
    let front = view_mesh(&mut s, ViewKind::Front, None);
    assert!(
        (top(&front) - 6055.0).abs() < 1e-2,
        "Ansicht bis OK OG-Decke"
    );
}

/// A47 (H-01): Paneel „Geschosse“ links unter „Werkzeuge“, gleiche Breite,
/// nicht abgeschnitten, auch im kleinen Fenster.
#[test]
fn a47_paneel_geschosse_links() {
    for (w, h) in [(1440u32, 900u32), (900, 600)] {
        let mut ui = Ui::new(1.0, &Theme::dark());
        ui.fit(1.0, w, h);
        let tools = ui.rect(Panel::Tools, w, 32);
        let levels = ui.rect(PANEEL_GESCHOSSE, w, 32);
        assert_eq!(levels.x, tools.x, "{w}×{h}: links bündig");
        assert_eq!(levels.w, tools.w, "{w}×{h}: gleiche Breite");
        assert!(levels.y >= tools.y + tools.h, "{w}×{h}: unter Werkzeuge");
        assert!(
            levels.y + levels.h <= h as f32,
            "{w}×{h}: nicht abgeschnitten"
        );
    }
}

/// A48 (E14b Test 3): Klick auf „OG“ macht es aktiv. Der Grundriss schneidet
/// dann 1 m über UK OG (+3,855) und zeigt nur noch Wände und Decke von oben;
/// das Fundament ist aktivierbar (Schnitt −0,51, siehe A87); das Werkzeug
/// zeichnet auf UK OG.
#[test]
fn a48_aktives_geschoss() {
    let mut s = Scene::with_model(Model::with_seed(48));
    haus_b11(&mut s);
    let (eg, og) = (geschoss(&s, "EG"), geschoss(&s, "OG"));
    let top = |m: &Shown| m.faces.iter().map(|v| v[2]).fold(f32::MIN, f32::max);
    assert_eq!(s.active_storey(), eg);
    assert_eq!(s.plan_cut(), PLAN_CUT);
    let plan = view_mesh(&mut s, ViewKind::Plan, None);
    assert!(
        (top(&plan) - 1000.0).abs() < 1e-2,
        "EG: Wände bei +1,00 geschnitten"
    );

    let gr = s.levels().bands.iter().find(|b| b.foundation).unwrap().id;
    assert!(s.set_active_storey(gr), "Fundament aktivierbar");
    assert_eq!(s.plan_cut(), -510.0, "Mitte Frostschürze");
    assert!(s.set_active_storey(og));
    assert!(!s.set_active_storey(og), "schon aktiv");
    assert_eq!(s.plan_cut(), 2855.0 + 1000.0);
    let l = s.levels();
    let active: Vec<_> = l
        .bands
        .iter()
        .filter(|b| b.active)
        .map(|b| b.name.as_str())
        .collect();
    assert_eq!(active, ["OG"]);
    let plan = view_mesh(&mut s, ViewKind::Plan, None);
    assert!(
        (top(&plan) - 3855.0).abs() < 1e-2,
        "OG: Wände bei +3,855 geschnitten"
    );
    assert!(
        plan.faces.iter().any(|v| (v[2] - 2855.0).abs() < 1e-2),
        "Decke von oben sichtbar"
    );

    // Ebene ziehen verschiebt die Schnitthöhe mit
    kante_ziehen(&mut s, "EG.OK", &[3000.0], false);
    assert_eq!(s.plan_cut(), 3000.0 + 1000.0);

    // Oberfläche: Hinweis unter „Gebäude“ (E16), das Werkzeug zeichnet auf
    // der Ebene des aktiven Geschosses
    let mut ui = Ui::new(1.0, &Theme::dark());
    ui.fit(1.0, 1440, 900);
    let h0 = ui.rect(Panel::Tools, 1440, 32).h;
    ui.upper_active = true;
    assert!(ui.rect(Panel::Tools, 1440, 32).h > h0, "Hinweiszeile");
    assert!(!s.ground_active());
    assert_eq!(s.work_plane(), (3000.0, 2855.0), "UK OG und OG-Höhe");
    let c = cam3d();
    let mut t = tool(&s);
    click(&mut t, &c, vec3(20000.0, 0.0, 3000.0));
    click(&mut t, &c, vec3(24000.0, 0.0, 3000.0));
    let w = t.handle(&key(Key::Enter), &c, W, H, 1.0).commit.unwrap();
    assert_eq!(w.base, 3000.0);
    assert!((w.points[1] - vec3(24000.0, 0.0, 3000.0)).length() < 1e-6);
}

/// A49 (E14b Test 2): Beim Ziehen einer Ebene eine violette Hilfslinie in
/// Ebenenhöhe: in 3D ein Rechteck 0,5 m um das Modell, in Schnitt und
/// Ansichten eine waagerechte Linie, im Grundriss nichts.
#[test]
fn a49_hilfslinie_der_ebene() {
    let th = Theme::dark();
    let b = Some((vec3(0.0, 0.0, -800.0), vec3(10000.0, 8000.0, 3500.0)));
    let z = 5710.0;
    let r = crate::level_guide(ViewKind::Persp, b, z, 1.0, &th);
    assert_eq!(r.len(), 4, "Rechteck");
    for h in &r {
        assert_eq!((h.a[2], h.b[2]), (z as f32, z as f32));
        assert_eq!(h.color, th.interact.drag);
        assert_eq!(h.width, th.size.level_guide);
        assert!(!h.occlude, "auch hinter Wänden sichtbar");
    }
    let xs: Vec<f32> = r.iter().flat_map(|h| [h.a[0], h.b[0]]).collect();
    assert_eq!(xs.iter().cloned().fold(f32::MAX, f32::min), -500.0);
    assert_eq!(xs.iter().cloned().fold(f32::MIN, f32::max), 10500.0);
    for v in [ViewKind::Section, ViewKind::Front, ViewKind::Left] {
        let l = crate::level_guide(v, b, z, 2.0, &th);
        assert_eq!(l.len(), 1, "{v:?}");
        assert_eq!((l[0].a[2], l[0].b[2]), (z as f32, z as f32));
        assert_eq!(l[0].width, 2.0 * th.size.level_guide);
    }
    assert!(crate::level_guide(ViewKind::Plan, b, z, 1.0, &th).is_empty());
}

// ---------------------------------------------------------------------------
// A50–A57: Gebäude aus einem Polygon, Obergeschoss, OG-Decke (B12 Phase 1, G6, E16)
// Sollwerte: bim/paket-b12-obergeschoss.md „Fertig, wenn“ (von Hand nachgerechnet),
// eingeschränkt nach Jörn 06.10. 09:29: Der Dialog fragt nur die Anzahl der
// Geschosse, derzeit fest 2 (EG + OG); der Baukörper endet an OK OG-Decke +5,835.
// Fälle mit 0 oder 2 OG aus B12 sind deshalb nicht enthalten.
//
// Stand B12/E16 final (09:34). Vorbereitet vor dem Einbau, setzt A32–A47 voraus (decke, aw_schicht, anzahl,
// geschoss, band, kante_ziehen, HAUS_SZO1). Zugriffe auf die neue API stehen
// nur in den Hilfsfunktionen direkt hier unten.
//
// Beim Einbau ändern sich ältere Sollwerte, weil die EG-Wände dann an OK EG
// (+2,855) enden statt 3,50 hoch zu sein: A16, A29, A30, A34, A39–A48.
// ---------------------------------------------------------------------------

/// Knopf „Gebäude“ → Dialog (2 Geschosse) → OK: Transaktion offen, Gebäude
/// mit GR/EG/OG steht schon im Geschosspaneel. API-Annahme:
/// `Scene::open_building_dialog()` (begin + add_building),
/// `Scene::cancel_building()` (rollback).
fn dialog_ok(s: &mut Scene) {
    s.open_building_dialog();
}

fn dialog_abbrechen(s: &mut Scene) {
    s.cancel_building();
}

/// Ein Feld im Gebäudedialog setzen (Jörn 10:13: lichte Höhen, Deckendicken,
/// Sohlplattendicke). Werte in mm; `false` = abgelehnt. Feldnamen:
/// "lichte_eg", "lichte_og", "decke_eg", "decke_og", "sohlplatte".
fn dialog_feld(s: &mut Scene, feld: &str, wert: f64) -> bool {
    s.set_building_dialog_value(feld, wert)
}

/// Dialog mit OK, dann das Rechteck mit dem Werkzeug; das Schließen legt das
/// ganze Gebäude an (ein Schritt „Gebäude erstellt“). Liefert (EG-Zug, OG-Zug).
fn gebaeude(s: &mut Scene) -> (RunId, RunId) {
    dialog_ok(s);
    let eg = zeichne_rechteck(s, &cam3d());
    (eg, og_zug(s, eg))
}

fn og_zug(s: &Scene, eg: RunId) -> RunId {
    let og = geschoss(s, "OG");
    s.model()
        .runs()
        .ids()
        .find(|r| {
            *r != eg
                && s.model()
                    .wall_at(*r, 0)
                    .and_then(|w| s.model().element(w))
                    .is_some_and(|el| el.storey == og)
        })
        .expect("OG-Zug angelegt")
}

/// Gebäude im Modell. API-Annahme: `Model::buildings()` mit `number`.
fn gebaeude_nummern(s: &Scene) -> Vec<String> {
    let mut v: Vec<String> = s
        .model()
        .buildings()
        .iter()
        .map(|(_, b)| b.number.clone())
        .collect();
    v.sort();
    v
}

/// Lichte Höhe OG als Zahl. API-Annahme: `set_clear_height` gilt je Geschoss.
fn lichte_og(s: &mut Scene, wert: f64) -> bool {
    let og = geschoss(s, "OG");
    s.edit_model("lichte Höhe OG", |m| m.set_clear_height(og, wert))
}

fn lichte_og_hoehe(s: &Scene) -> f64 {
    s.model().clear_height(geschoss(s, "OG"))
}

fn r4(v: f64) -> f64 {
    (v * 1e4).round() / 1e4
}

/// Gasbeton netto und Dämmung eines Zugs (m³, gerundet).
fn schale(s: &Scene, run: RunId) -> (f64, f64) {
    (r4(aw_schicht(s, run, 1)), r4(aw_schicht(s, run, 0)))
}

/// Gummiband am Wandfuß einer Wand (Fußhöhe z) um dy verschieben.
fn ziehen_am_fuss(s: &mut Scene, z: f64, dy: f64) {
    let c = cam3d();
    let mut e = WallEdit::default();
    let (x, y) = px(&c, vec3(2500.0, 8000.0, z));
    e.handle(&mv(x, y), s, &c, W, H, 1.0, true);
    e.handle(&down(x, y), s, &c, W, H, 1.0, true);
    let (x2, y2) = px(&c, vec3(2500.0, 8000.0 + dy, z));
    e.handle(&mv(x2, y2), s, &c, W, H, 1.0, true);
    e.handle(&up(x2, y2), s, &c, W, H, 1.0, true);
}

const EG_SCHALE: (f64, f64) = (15.7613, 14.1654);
// Jörn 10:13: lichte Höhe OG 2,635 wie EG, daher gleiche Schale (Wandhöhe 2,855)
const OG_SCHALE: (f64, f64) = (15.7613, 14.1654);

/// A50 (B12): Das erste Polygon erzeugt in einem Schritt das ganze Gebäude:
/// GB-01, GR/EG/OG, FS-001, SP-001, AW-001…008, DE-001, DE-002.
#[test]
fn a50_gebaeude_aus_einem_polygon() {
    let mut s = Scene::with_model(Model::with_seed(50));
    // Dialog öffnen: Geschosse sofort da; Abbrechen lässt nichts zurück
    dialog_ok(&mut s);
    assert_eq!(gebaeude_nummern(&s), ["GB-01"]);
    assert_eq!(band(&s, "OG"), (2855.0, 5710.0));
    // Dialogfelder gehen live in den Geschossmanager (keine Kopie)
    assert!(dialog_feld(&mut s, "lichte_eg", 2700.0));
    assert_eq!(band(&s, "EG"), (0.0, 2920.0));
    assert_eq!(band(&s, "OG"), (2920.0, 5775.0));
    assert!(dialog_feld(&mut s, "decke_og", 250.0));
    assert_eq!(band(&s, "OG"), (2920.0, 5805.0));
    assert!(
        !dialog_feld(&mut s, "lichte_og", 900.0),
        "lichte Höhe ≥ 1,00"
    );
    assert!(
        !dialog_feld(&mut s, "sohlplatte", 50.0),
        "Plattendicke 10–100 cm"
    );
    assert_eq!(
        band(&s, "GR"),
        (-800.0, 0.0),
        "UK Frostschürze bleibt −0,80"
    );
    dialog_abbrechen(&mut s);
    assert!(gebaeude_nummern(&s).is_empty());
    assert!(!s.undo(), "Rückgängig-Liste unverändert leer");
    let (eg, og) = gebaeude(&mut s);
    assert_eq!(gebaeude_nummern(&s), ["GB-01"]);
    assert_eq!(band(&s, "GR"), (-800.0, 0.0));
    assert_eq!(band(&s, "EG"), (0.0, 2855.0));
    assert_eq!(band(&s, "OG"), (2855.0, 5710.0));
    let n = |s: &Scene| ["FS-", "SP-", "AW-", "DE-", "IW-"].map(|p| anzahl(s, p));
    assert_eq!(n(&s), [1, 1, 8, 2, 0]);
    let nr = |run: RunId, i: usize| {
        s.model()
            .element(s.model().wall_at(run, i).unwrap())
            .unwrap()
            .number
            .clone()
    };
    assert_eq!(nr(eg, 0), "AW-001");
    assert_eq!(nr(og, 0), "AW-005");
    let de = |run: RunId| {
        s.model()
            .element(decke(&s, run).unwrap())
            .unwrap()
            .number
            .clone()
    };
    assert_eq!(
        (de(eg), de(og)),
        ("DE-001".to_string(), "DE-002".to_string())
    );
    assert!(s.foundation(og).is_none(), "Gründung nur unter dem EG");
    assert!(s.model().check().is_empty(), "{:?}", s.model().check());
    // Ein Schritt: Rückgängig entfernt alles, Wiederherstellen bringt dieselben Kennungen
    let guids = |s: &Scene| {
        let mut g: Vec<_> = s.model().elements().iter().map(|(_, e)| e.guid).collect();
        g.sort();
        g
    };
    let vorher = guids(&s);
    assert!(s.undo());
    assert_eq!(n(&s), [0, 0, 0, 0, 0]);
    assert!(s.redo());
    assert_eq!(guids(&s), vorher);
}

/// A51 (B12): Mengen je Geschoss getrennt, Gründung nur einmal.
#[test]
fn a51_mengen_je_geschoss() {
    let mut s = Scene::with_model(Model::with_seed(51));
    let (eg, og) = gebaeude(&mut s);
    assert_eq!(decke_mengen(&s, eg).0, 75.0384);
    assert_eq!(decke_mengen(&s, eg).1, 16.5084);
    assert_eq!(decke_mengen(&s, og).0, 75.0384);
    assert_eq!(decke_mengen(&s, og).1, 16.5084);
    assert_eq!(decke_hoehen(&s, eg), (2635.0, 2855.0));
    assert_eq!(decke_hoehen(&s, og), (5490.0, 5710.0));
    assert_eq!(lichte_og_hoehe(&s), 2635.0);
    assert_eq!(schale(&s, eg), EG_SCHALE);
    assert_eq!(schale(&s, og), OG_SCHALE);
    let (sp, fs) = s.foundation_qto(eg).unwrap();
    assert_eq!((m3(sp.volume), m3(fs.volume)), (17.6, 7.0238));
    // EG-Innenwand netto, nicht gestapelt
    let c = cam3d();
    let set = s.model().defaults().interior_wall;
    let mut t = tool(&s);
    t.set_category(sk_model::Category::InteriorWall, s.model().wall_layers(set));
    click(&mut t, &c, vec3(5000.0, 0.0, 0.0));
    click(&mut t, &c, vec3(5000.0, 8000.0, 0.0));
    let w = t.handle(&key(Key::Enter), &c, W, H, 1.0).commit.unwrap();
    let iw = s.add_wall_as(&w, sk_model::Category::InteriorWall).unwrap();
    assert!(
        (wall_m3(&s, iw, 0) - 3.39846).abs() < 5e-5,
        "IW {}",
        wall_m3(&s, iw, 0)
    );
    assert_eq!(anzahl(&s, "IW-"), 1, "Innenwände werden nicht gestapelt");
    assert_eq!(s.model().joins().len(), 2, "Anschlüsse nur im EG");
    assert!(s.model().check().is_empty(), "{:?}", s.model().check());
}

/// A52 (B12, Jörn 09:26): Das Gummiband am EG-Wandfuß ändert die Außenkontur
/// des ganzen Gebäudes in einem Schritt. Der OG-Zug hat in Phase 1 keine
/// eigenen Fußgriffe (E16), er folgt dem EG.
#[test]
fn a52_gummiband_zieht_alle_geschosse() {
    let mut s = Scene::with_model(Model::with_seed(52));
    let (eg, og) = gebaeude(&mut s);
    ziehen_am_fuss(&mut s, 0.0, 1000.0);
    assert_eq!(decke_mengen(&s, eg).0, 84.7584, "DE-001");
    assert_eq!(decke_mengen(&s, og).0, 84.7584, "DE-002");
    assert_eq!(s.chain(eg).unwrap().points, s.chain(og).unwrap().points);
    assert_eq!(m2(s.foundation_qto(eg).unwrap().0.area), 90.0);
    assert!(s.model().check().is_empty(), "{:?}", s.model().check());
    assert!(s.undo(), "ein Schritt");
    assert_eq!(decke_mengen(&s, eg).0, 75.0384);
    assert_eq!(decke_mengen(&s, og).0, 75.0384);
    // Am OG-Wandfuß gibt es keinen Griff: nichts ändert sich
    ziehen_am_fuss(&mut s, 2855.0, 1000.0);
    assert_eq!(decke_mengen(&s, og).0, 75.0384);
    assert_eq!(s.chain(eg).unwrap().points, s.chain(og).unwrap().points);
}

/// A53 (B11/B12): Höhen ändern: lichte Höhe OG und OK EG; Wände gehen mit.
#[test]
fn a53_hoehen_aendern() {
    let mut s = Scene::with_model(Model::with_seed(53));
    let (eg, og) = gebaeude(&mut s);
    assert!(lichte_og(&mut s, 2900.0));
    assert_eq!(band(&s, "OG"), (2855.0, 5975.0));
    assert!((aw_schicht(&s, og, 1) - 17.34635).abs() < 5e-5);
    assert_eq!(schale(&s, eg), EG_SCHALE, "EG unverändert");
    assert!(s.undo());
    assert!(!lichte_og(&mut s, 900.0), "lichte Höhe ≥ 1,00");
    assert_eq!(band(&s, "OG"), (2855.0, 5710.0));
    // OK EG auf +3,00: EG-Wände 3,00, OG wandert mit, OG-Mengen gleich
    kante_ziehen(&mut s, "EG.OK", &[3000.0], false);
    assert_eq!(band(&s, "OG"), (3000.0, 5855.0));
    assert_eq!(schale(&s, eg), (16.6286, 14.8848));
    assert_eq!(schale(&s, og), OG_SCHALE);
    assert_eq!(decke_hoehen(&s, og), (5635.0, 5855.0));
    assert!(s.model().check().is_empty(), "{:?}", s.model().check());
    assert!(s.undo());
    assert_eq!(schale(&s, eg), EG_SCHALE);
}

/// A54 (Jörn 09:24): EG- und OG-Schale ohne waagerechte Naht in Ansicht und
/// Schnitt; Decken im Schnitt in der Tasche.
#[test]
fn a54_schale_ohne_naht() {
    let mut s = Scene::with_model(Model::with_seed(54));
    gebaeude(&mut s);
    let flach = |m: &Shown, z: f32, sel: &dyn Fn(&[[f32; 3]; 2]) -> bool| {
        m.edges
            .iter()
            .filter(|e| (e.0[0][2] - z).abs() < 1e-2 && (e.0[1][2] - z).abs() < 1e-2)
            .filter(|e| sel(&e.0))
            .count()
    };
    // Ansicht Vorne: Fassade y = 0, keine Kante bei +2,855 über die Breite
    let front = view_mesh(&mut s, ViewKind::Front, None);
    let quer = |p: &[[f32; 3]; 2]| {
        p[0][1].abs() < 1.0 && p[0][0].min(p[1][0]) < 4000.0 && p[0][0].max(p[1][0]) > 6000.0
    };
    assert_eq!(flach(&front, 2855.0, &quer), 0, "Naht in der Ansicht");
    assert!(
        flach(&front, 5710.0, &quer) > 0,
        "Gegenprobe: Oberkante gezeichnet"
    );
    // Schnitt A–A: in der Dämmung (x 0 … 140) keine Kante bei +2,855
    let mut sect = SectionLine::default();
    sect.ensure(&s);
    let y = sect.y.unwrap() as f32;
    let cut = view_mesh(&mut s, ViewKind::Section, sect.plane());
    let daemmung = |p: &[[f32; 3]; 2]| {
        (p[0][1] - y).abs() < 1e-2 && p[0][0].max(p[1][0]) < 139.0 && p[0][0].min(p[1][0]) > 1.0
            || (p[0][1] - y).abs() < 1e-2
                && p[0][0].min(p[1][0]) < 70.0
                && p[0][0].max(p[1][0]) > 70.0
                && p[0][0].max(p[1][0]) < 141.0
    };
    assert_eq!(flach(&cut, 2855.0, &daemmung), 0, "Naht in der Dämmung");
    // Beide Decken im Schnitt mit Stahlbeton-Schraffur
    let in_band = |lo: f32, hi: f32| {
        cut.faces.iter().any(|v| {
            v[9] == pattern::CONCRETE && v[2] > lo - 1e-2 && v[2] < hi + 1e-2 && v[2] > 0.0
        })
    };
    assert!(in_band(2635.0, 2855.0) && in_band(5490.0, 5710.0));
    let top = cut.faces.iter().map(|v| v[2]).fold(f32::MIN, f32::max);
    assert!(
        (top - 5710.0).abs() < 1e-2,
        "Baukörper endet an OK OG-Decke"
    );
}

/// A55 (B12, E14b): Innenwand im aktiven OG gehört zum OG und endet an der
/// OG-Decke; Anschlüsse nur im eigenen Geschoss.
#[test]
fn a55_innenwand_im_og() {
    let mut s = Scene::with_model(Model::with_seed(55));
    gebaeude(&mut s);
    let og = geschoss(&s, "OG");
    assert!(s.set_active_storey(og));
    let c = cam3d();
    let set = s.model().defaults().interior_wall;
    let mut t = tool(&s);
    t.set_category(sk_model::Category::InteriorWall, s.model().wall_layers(set));
    click(&mut t, &c, vec3(5000.0, 0.0, 2855.0));
    click(&mut t, &c, vec3(5000.0, 8000.0, 2855.0));
    let w = t.handle(&key(Key::Enter), &c, W, H, 1.0).commit.unwrap();
    let iw = s.add_wall_as(&w, sk_model::Category::InteriorWall).unwrap();
    let id = s.model().wall_at(iw, 0).unwrap();
    assert_eq!(s.model().element(id).unwrap().storey, og);
    // 7,37 × 0,175 × 2,635
    assert!(
        (wall_m3(&s, iw, 0) - 3.398491).abs() < 5e-5,
        "IW OG {}",
        wall_m3(&s, iw, 0)
    );
    assert_eq!(s.model().joins().len(), 2);
    assert!(s.model().check().is_empty(), "{:?}", s.model().check());
}

/// A56 (F-03): .szo Version 3 bytegleich; alte Datei (SZO 1) wird zum
/// Gebäude mit einem Obergeschoss umgestellt.
#[test]
fn a56_datei_version_3_und_umstellung() {
    let d = test_dir("obergeschoss");
    let mut s = Scene::with_model(Model::with_seed(56));
    gebaeude(&mut s);
    let path = d.join("Haus.szo");
    crate::document::save(s.model(), &path).unwrap();
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(text.starts_with("SZO 4"));
    assert_eq!(
        text.lines().filter(|l| l.starts_with("[building]")).count(),
        1
    );
    let loaded = crate::document::load(&path).unwrap();
    assert!(loaded.hints.is_empty(), "{:?}", loaded.hints);
    let p2 = d.join("Haus2.szo");
    crate::document::save(&loaded.model, &p2).unwrap();
    assert_eq!(std::fs::read(&p2).unwrap(), text.clone().into_bytes());

    // Alte Datei: Rechteck mit Innenwand, Wände 3,50, ohne Decke
    let alt = d.join("Alt.szo");
    std::fs::write(&alt, HAUS_SZO1).unwrap();
    let loaded = crate::document::load(&alt).unwrap();
    assert!(
        loaded
            .hints
            .iter()
            .any(|h| h.contains("auf Gebäude umgestellt")),
        "{:?}",
        loaded.hints
    );
    let t = Scene::with_model(loaded.model);
    assert_eq!(gebaeude_nummern(&t), ["GB-01"]);
    assert_eq!(band(&t, "OG"), (2855.0, 5710.0));
    assert_eq!(
        ["AW-", "DE-", "IW-", "SP-", "FS-"].map(|p| anzahl(&t, p)),
        [8, 2, 1, 1, 1]
    );
    let eg = t
        .model()
        .runs()
        .ids()
        .find(|r| t.foundation(*r).is_some())
        .unwrap();
    let og = og_zug(&t, eg);
    assert_eq!(schale(&t, eg), EG_SCHALE);
    assert_eq!(schale(&t, og), OG_SCHALE);
    // Alte Datei behält ihre Gründung (20 cm Platte, 60 cm Schürze), nur das
    // fehlende OG kommt mit den neuen Standards (Koordinator 10:15)
    let (sp, fs) = t.foundation_qto(eg).unwrap();
    assert_eq!((m3(sp.volume), m3(fs.volume)), (16.0, 7.266));
    assert!(t.model().check().is_empty(), "{:?}", t.model().check());
    let p3 = d.join("Alt-3.szo");
    crate::document::save(t.model(), &p3).unwrap();
    let neu = std::fs::read_to_string(&p3).unwrap();
    assert!(neu.starts_with("SZO 4"));
    let u = crate::document::load(&p3).unwrap().model;
    let p4 = d.join("Alt-4.szo");
    crate::document::save(&u, &p4).unwrap();
    assert_eq!(std::fs::read(&p4).unwrap(), neu.into_bytes(), "bytegleich");
    let _ = std::fs::remove_dir_all(&d);
}

/// A57 (B12): Ein zweites Polygon daneben wird ein zweites Gebäude mit
/// eigenen Geschossen; beide bleiben unabhängig.
#[test]
fn a57_zweites_gebaeude() {
    let mut s = Scene::with_model(Model::with_seed(57));
    let (eg1, _) = gebaeude(&mut s);
    dialog_ok(&mut s);
    let c = cam3d();
    let mut t = tool(&s);
    let off = vec3(20000.0, 0.0, 0.0);
    for p in RECHTECK {
        assert!(click(&mut t, &c, p + off).is_none());
    }
    let w = click(&mut t, &c, RECHTECK[0] + off).expect("schließt");
    s.add_wall(&w).unwrap();
    assert_eq!(gebaeude_nummern(&s), ["GB-01", "GB-02"]);
    assert_eq!(s.model().storeys().len(), 6, "je Gebäude GR, EG, OG");
    assert_eq!(
        ["AW-", "DE-", "SP-", "FS-"].map(|p| anzahl(&s, p)),
        [16, 4, 2, 2]
    );
    // Gummiband am ersten Haus ändert das zweite nicht
    let haus2 = |s: &Scene| {
        let mut v: Vec<_> = s
            .model()
            .runs()
            .ids()
            .filter_map(|r| s.chain(r))
            .filter(|c| c.points[0].x >= 19999.0)
            .map(|c| c.points.clone())
            .collect();
        v.sort_by(|a, b| a[0].z.total_cmp(&b[0].z));
        v
    };
    let vorher = haus2(&s);
    assert_eq!(vorher.len(), 2, "EG- und OG-Zug von Gebäude 2");
    ziehen_am_fuss(&mut s, 0.0, 1000.0);
    assert_eq!(decke_mengen(&s, eg1).0, 84.7584);
    assert_eq!(haus2(&s), vorher, "Gebäude 2 unverändert");
    assert!(s.model().check().is_empty(), "{:?}", s.model().check());
}

/// E16 Test 5: OG aktiv, Grundriss: EG-Wände als Hintergrund (Kantenart
/// `BACKGROUND`, Stift 9 grau 0,13) ohne Schraffur, nicht wählbar, Endpunkt
/// fangbar. EG aktiv: kein Hintergrund.
#[test]
fn e16_hintergrund_im_og_grundriss() {
    let mut s = Scene::with_model(Model::with_seed(58));
    gebaeude(&mut s);
    let bg = edge_kind::BACKGROUND as f32;
    let plan = view_mesh(&mut s, ViewKind::Plan, None);
    assert!(
        !plan.edges.iter().any(|e| e.1 == bg),
        "EG aktiv: kein Hintergrund"
    );
    assert!(s.background_snaps().is_empty());
    assert!(s.set_active_storey(geschoss(&s, "OG")));
    let m = s.mesh(ViewKind::Plan, None, &[]);
    let under: Vec<_> = m.edges.iter().filter(|e| e.1 == bg).collect();
    assert!(!under.is_empty(), "Hintergrund gezeichnet");
    assert!(
        under
            .iter()
            .all(|e| e.0[0][2] == 2865.0 && e.0[1][2] == 2865.0),
        "knapp über dem Boden des OG"
    );
    // Außenkante des WDVS (x = 0) ist eine Hintergrundkante
    assert!(under.iter().any(|e| e.0[0][0] == 0.0 && e.0[1][0] == 0.0));
    let t = s.table();
    assert_eq!(
        t.background.1,
        [160.0 / 255.0, 160.0 / 255.0, 160.0 / 255.0, 1.0]
    );
    let looks = t.edge_looks(true, 1.0);
    assert_eq!(looks.width[edge_kind::BACKGROUND as usize], t.background.0);
    // Nicht wählbar: ein Klick auf die EG-Außenwand trifft die OG-Wand, ein
    // Klick in den Raum den Boden (DE-001), nie eine EG-Wand
    let pc = fit_parallel(ViewKind::Plan, s.bounds(), W, H);
    let (x, y) = px(&pc, vec3(100.0, 4000.0, 3855.0));
    let hit = selection::pick_at(&mut s, &pc, ViewKind::Plan, None, x, y, W, H).unwrap();
    assert_eq!(s.model().element(hit).unwrap().storey, geschoss(&s, "OG"));
    let (x, y) = px(&pc, vec3(5000.0, 4000.0, 2855.0));
    let hit = selection::pick_at(&mut s, &pc, ViewKind::Plan, None, x, y, W, H).unwrap();
    assert_eq!(s.model().element(hit).unwrap().number, "DE-001");
    let snaps = s.background_snaps();
    assert!(snaps
        .iter()
        .any(|(a, _)| (*a - vec3(0.0, 0.0, 2855.0)).length() < 1e-6));
    // Fangbar: das Werkzeug im OG springt auf die Ecke des EG
    let c = cam3d();
    let mut t = tool(&s);
    t.snaps = snaps;
    t.set_category(
        sk_model::Category::InteriorWall,
        s.model().wall_layers(s.model().defaults().interior_wall),
    );
    let (x, y) = px(&c, vec3(30.0, 25.0, 2855.0));
    t.handle(&mv(x, y), &c, W, H, 1.0);
    t.handle(&down(x, y), &c, W, H, 1.0);
    click(&mut t, &c, vec3(3000.0, 0.0, 2855.0));
    let w = t.handle(&key(Key::Enter), &c, W, H, 1.0).commit.unwrap();
    assert!(
        (w.points[0] - vec3(0.0, 0.0, 2855.0)).length() < 1e-6,
        "{:?}",
        w.points
    );
}

// Abnahmetests E17 „Dateimenü am Logo, Rückgängig/Wiederherstellen in der
// Titelleiste“ (einstellungen/paket-e17-dateimenue.md), vorbereitet gegen main
// b1c8f37. Spezifikation: test/abnahme-dateimenue.md.
//
// Alle angenommenen Namen der neuen Schnittstellen stehen NUR in den Adaptern
// unten. Der Bauthread passt die Adapter an seine Namen an; die Tests selbst
// bleiben unverändert. Die Ablaufsteuerung in main.rs (Fenster, Windows-
// Dialoge) ist in der Cloud nicht prüfbar und steht im Handtest H37–H41.

// ===== Adapter E17 =====

use crate::document::Document;
use crate::menu::{Command, FileMenu, Recent, SaveAnswer as SaveAnswer2, SaveDialog, Shortcuts};

/// Befehle als Text, damit die Tests nicht vom Enum abhängen.
fn befehl(c: Option<Command>) -> Option<String> {
    c.map(|c| match c {
        Command::New => "Neu".into(),
        Command::Open => "Öffnen".into(),
        Command::OpenRecent(i) => format!("Zuletzt {i}"),
        Command::Save => "Speichern".into(),
        Command::SaveAs => "Speichern unter".into(),
        Command::Close => "Schließen".into(),
        Command::Quit => "Beenden".into(),
        Command::Undo => "Rückgängig".into(),
        Command::Redo => "Wiederherstellen".into(),
        Command::OpenMenu => "Menü".into(),
        Command::ClearRecent => "Liste leeren".into(),
        Command::Settings => "Einstellungen".into(),
        Command::Catalog => "Bauteilkatalog".into(),
        Command::Delete => "Löschen".into(),
        Command::Backups => "Sicherungen".into(),
        Command::OpenBackup(i) => format!("Sicherung {i}"),
    })
}

/// Linke Knopfgruppe der Titelleiste: "menu", "undo", "redo" oder `None`
/// (Ziehfläche bzw. rechte Fensterknöpfe).
fn linker_knopf(t: &TitleBar, x: f64, y: f64) -> Option<&'static str> {
    t.left_button_at(x, y).map(|b| match b {
        Button::Menu => "menu",
        Button::Undo => "undo",
        Button::Redo => "redo",
        _ => "rechts",
    })
}

/// Breite der linken Gruppe in Pixeln (geht als `left_width` an die Plattform).
fn linke_breite(t: &TitleBar) -> u32 {
    t.left_width()
}

/// Hinweis beim Darüberfahren über Rückgängig (`redo = false`) bzw.
/// Wiederherstellen; `None`, wenn der Knopf ausgegraut ist.
fn verlauf_hinweis(s: &Scene, redo: bool) -> Option<String> {
    crate::menu::history_hint(s, redo)
}

/// Kontext des Menüs: Speichern aktiv? und die Liste „Zuletzt geöffnet“.
fn speichern_aktiv(doc: &Document, s: &Scene) -> bool {
    crate::menu::save_enabled(doc, s.model())
}

/// Sichtbare Zeilen des geöffneten Menüs (ohne Untermenü):
/// (Text, Kürzel, aktiv); Trennlinien als ("—", "", false).
fn zeilen(m: &FileMenu, save: bool, r: &Recent) -> Vec<(String, String, bool)> {
    m.items(save, r)
        .into_iter()
        .map(|i| {
            if i.separator {
                ("—".into(), String::new(), false)
            } else {
                (i.label, i.shortcut, i.enabled)
            }
        })
        .collect()
}

/// Zeilen des Untermenüs „Zuletzt geöffnet“: (Dateiname, Ordner, aktiv).
fn unterzeilen(m: &FileMenu, r: &Recent) -> Vec<(String, String, bool)> {
    m.sub_items(r)
        .into_iter()
        .map(|i| (i.label, i.detail, i.enabled))
        .collect()
}

/// Taste im offenen Menü; liefert den ausgelösten Befehl.
fn menue_taste(m: &mut FileMenu, k: Key, save: bool, r: &Recent) -> Option<String> {
    befehl(m.key(k, save, r))
}

/// Tastendruck im Hauptfenster (`free`: kein Werkzeug in Eingabe, nichts
/// gezogen). `down = false` ist das Loslassen.
fn kuerzel(k: &mut Shortcuts, key: Key, down: bool, mods: Modifiers, free: bool) -> Option<String> {
    befehl(k.key(key, down, mods, free))
}

// Tasten, die `Key` heute nur als `Other(vk)` kennt (Windows-Tastencodes)
fn taste_hoch() -> Key {
    Key::Other(0x26)
}
fn taste_runter() -> Key {
    Key::Other(0x28)
}
fn taste_f4() -> Key {
    Key::Other(0x73)
}
fn taste_f10() -> Key {
    Key::Other(0x79)
}

/// Einstellungsdatei mit Schema und Liste schreiben bzw. lesen.
fn einstellungen_schreiben(t: &Theme, r: &Recent) -> String {
    crate::settings::write_all(t, r)
}
fn einstellungen_lesen(text: &str) -> (Theme, Recent) {
    let (t, r, _hints) = crate::settings::read_all(text);
    (t, r)
}

/// Ordner in der Mitte gekürzt auf höchstens `max` Zeichen.
fn ordner_kurz(p: &std::path::Path, max: usize) -> String {
    crate::menu::short_folder(p, max)
}

/// Text der Nachfrage: (Frage, zweiter Satz). `gespeichert` = Uhrzeit (h, min)
/// des letzten Speicherns.
fn nachfrage(doc: &Document, gespeichert: Option<(u8, u8)>) -> (String, Option<String>) {
    crate::menu::save_question(doc, gespeichert)
}

fn antwort(a: Option<SaveAnswer2>) -> Option<&'static str> {
    a.map(|a| match a {
        SaveAnswer2::Save => "Speichern",
        SaveAnswer2::Discard => "Nicht speichern",
        SaveAnswer2::Cancel => "Abbrechen",
    })
}

const MODS_STRG: Modifiers = Modifiers {
    shift: false,
    ctrl: true,
    alt: false,
};
const MODS_STRG_UMSCHALT: Modifiers = Modifiers {
    shift: true,
    ctrl: true,
    alt: false,
};
const MODS_ALT: Modifiers = Modifiers {
    shift: false,
    ctrl: false,
    alt: true,
};

// ===== Tests =====

/// A58 (E17 §1, Test 1): Linke Knopfgruppe der Titelleiste. Von links Menü
/// 46 px, 8 px Abstand, Rückgängig 40 px, Wiederherstellen 40 px; der Rest
/// bleibt Ziehfläche, die drei Fensterknöpfe rechts unverändert; skaliert mit.
#[test]
fn a58_titelleiste_linke_knopfgruppe() {
    let w = 1440;
    for scale in [1.0f32, 1.5] {
        let t = TitleBar::new(scale);
        let p = |v: f64| v * scale as f64;
        assert_eq!(linker_knopf(&t, p(1.0), p(10.0)), Some("menu"), "{scale}");
        assert_eq!(linker_knopf(&t, p(45.0), p(31.0)), Some("menu"));
        assert_eq!(linker_knopf(&t, p(50.0), p(10.0)), None, "Abstand 8 px");
        assert_eq!(linker_knopf(&t, p(55.0), p(10.0)), Some("undo"));
        assert_eq!(linker_knopf(&t, p(93.0), p(10.0)), Some("undo"));
        assert_eq!(linker_knopf(&t, p(95.0), p(10.0)), Some("redo"));
        assert_eq!(linker_knopf(&t, p(133.0), p(10.0)), Some("redo"));
        assert_eq!(linker_knopf(&t, p(140.0), p(10.0)), None, "Ziehfläche");
        assert_eq!(linker_knopf(&t, p(10.0), p(40.0)), None, "unter der Leiste");
        assert_eq!(linke_breite(&t), (134.0 * scale).round() as u32);
        // Rechts wie bisher (A-Titelleiste)
        assert_eq!(
            t.button_at(w as f64 - p(10.0), p(10.0), w),
            Some(Button::Close)
        );
        assert_eq!(t.button_at(p(400.0), p(10.0), w), None, "Rest zieht");
    }
}

/// A59 (E17 §1, Test 2): Rückgängig und Wiederherstellen als Knöpfe. Leeres
/// Modell: beide ausgegraut. Nach „Wand zeichnen“ ist Rückgängig aktiv mit
/// dem Namen des Schritts; zurückgenommen wird Wiederherstellen aktiv.
#[test]
fn a59_rueckgaengig_knoepfe() {
    let mut s = Scene::with_model(Model::with_seed(59));
    assert_eq!(verlauf_hinweis(&s, false), None, "leer: ausgegraut");
    assert_eq!(verlauf_hinweis(&s, true), None);
    let c = cam3d();
    let mut t = tool(&s);
    click(&mut t, &c, vec3(0.0, 0.0, 0.0));
    click(&mut t, &c, vec3(5000.0, 0.0, 0.0));
    let w = t.handle(&key(Key::Enter), &c, W, H, 1.0).commit.unwrap();
    s.add_wall(&w).unwrap();
    assert_eq!(
        verlauf_hinweis(&s, false).as_deref(),
        Some("Rückgängig: Wand zeichnen (Strg+Z)")
    );
    assert_eq!(verlauf_hinweis(&s, true), None);
    assert!(s.undo(), "Klick auf den Knopf = ein Schritt zurück");
    assert_eq!(verlauf_hinweis(&s, false), None);
    assert_eq!(
        verlauf_hinweis(&s, true).as_deref(),
        Some("Wiederherstellen: Wand zeichnen (Strg+Y)")
    );
    assert!(s.redo());
    assert_eq!(s.model().runs().len(), 1);
    assert_eq!(verlauf_hinweis(&s, true), None);
}

/// A60 (E17 §2, Test 4): Einträge, Kürzel, Reihenfolge, Trennlinien;
/// Speichern ausgegraut bei gespeicherter, unveränderter Datei. Seit F-13
/// (paket-f13-sichern.md §4) steht „Sicherungen …“ direkt unter „Zuletzt
/// geöffnet“, ohne Kürzel.
#[test]
fn a60_dateimenue_eintraege_und_ausgrauen() {
    let mut s = Scene::with_model(Model::with_seed(60));
    let r = Recent::default();
    let mut m = FileMenu::default();
    assert!(!m.is_open());
    m.open();
    assert!(m.is_open());
    let soll: Vec<(String, String, bool)> = [
        ("Neu", "Strg+N", true),
        ("Öffnen …", "Strg+O", true),
        ("Zuletzt geöffnet", "▸", true),
        ("Sicherungen …", "", true),
        ("—", "", false),
        ("Speichern", "Strg+S", true),
        ("Speichern unter …", "Strg+Umschalt+S", true),
        ("—", "", false),
        ("Einstellungen …", "Strg+Komma", true),
        ("Bauteilkatalog …", "", true),
        ("—", "", false),
        ("Schließen", "Strg+W", true),
        ("Beenden", "Alt+F4", true),
    ]
    .iter()
    .map(|(a, b, c)| (a.to_string(), b.to_string(), *c))
    .collect();
    // Unbenannt, unverändert: Speichern aktiv (es gibt noch keine Datei)
    let neu = Document::new(s.model().revision());
    assert!(speichern_aktiv(&neu, &s));
    assert_eq!(zeilen(&m, speichern_aktiv(&neu, &s), &r), soll);
    // Gespeicherte Datei, unverändert: Speichern ausgegraut, sonst alles gleich
    let d = test_dir("menue");
    let mut doc = Document::new(s.model().revision());
    doc.mark_saved(d.join("Haus.szo"), s.model().revision());
    assert!(!speichern_aktiv(&doc, &s));
    let z = zeilen(&m, false, &r);
    assert_eq!(z[5], ("Speichern".into(), "Strg+S".into(), false));
    assert!(z.iter().enumerate().all(|(i, l)| i == 5 || *l == soll[i]));
    // Nach einer Änderung wieder aktiv
    assert!(s.edit_model("Test", |m| {
        m.add_building(2);
        true
    }));
    assert!(speichern_aktiv(&doc, &s));
    let _ = std::fs::remove_dir_all(&d);
}

/// A61 (E17 §2, Test 3): Bedienung mit Tasten. Pfeile wandern über die
/// Einträge (Trennlinien übersprungen), rechts öffnet „Zuletzt geöffnet“,
/// links schließt es, Enter führt aus und schließt, Esc und Klick daneben
/// schließen ohne Wirkung.
#[test]
fn a61_menue_bedienung() {
    let r = Recent::default();
    let mut m = FileMenu::default();
    m.open();
    // Erster Eintrag ist markiert; Enter = Neu
    assert_eq!(
        menue_taste(&mut m, Key::Enter, true, &r).as_deref(),
        Some("Neu")
    );
    assert!(!m.is_open(), "Enter schließt das Menü");
    // Runter: Öffnen, Zuletzt, Sicherungen
    m.open();
    for _ in 0..3 {
        assert_eq!(menue_taste(&mut m, taste_runter(), true, &r), None);
    }
    assert_eq!(
        menue_taste(&mut m, Key::Enter, true, &r).as_deref(),
        Some("Sicherungen")
    );
    // Runter ×4: über die Trennlinie auf Speichern
    m.open();
    for _ in 0..4 {
        assert_eq!(menue_taste(&mut m, taste_runter(), true, &r), None);
    }
    assert_eq!(
        menue_taste(&mut m, Key::Enter, true, &r).as_deref(),
        Some("Speichern")
    );
    // Hoch vom Speichern über die Trennlinie und „Sicherungen …“ zurück auf
    // „Zuletzt geöffnet“, rechts öffnet das Untermenü, links schließt es
    m.open();
    for _ in 0..4 {
        menue_taste(&mut m, taste_runter(), true, &r);
    }
    for _ in 0..2 {
        menue_taste(&mut m, taste_hoch(), true, &r);
    }
    assert!(!m.sub_open());
    menue_taste(&mut m, Key::Right, true, &r);
    assert!(m.sub_open(), "rechts öffnet „Zuletzt geöffnet“");
    menue_taste(&mut m, Key::Left, true, &r);
    assert!(
        !m.sub_open() && m.is_open(),
        "links schließt nur das Untermenü"
    );
    // Neu geöffnet steht die Markierung wieder oben. Ausgegrautes Speichern
    // wird übersprungen: runter ×4 landet auf „Speichern unter …“
    m.click_outside();
    m.open();
    for _ in 0..4 {
        menue_taste(&mut m, taste_runter(), false, &r);
    }
    assert_eq!(
        menue_taste(&mut m, Key::Enter, false, &r).as_deref(),
        Some("Speichern unter")
    );
    // Esc und Klick daneben: zu, kein Befehl
    m.open();
    assert_eq!(menue_taste(&mut m, Key::Escape, true, &r), None);
    assert!(!m.is_open());
    m.open();
    m.click_outside();
    assert!(!m.is_open());
}

/// A62 (E17 §4, Tests 2, 3, 7): Tastenkürzel. Strg+W schließt, Strg+Umschalt+Z
/// stellt wieder her, F10 und das Loslassen von Alt öffnen das Menü, Alt+F4
/// und Alt+Tab öffnen es nicht. Während einer Eingabe gelten keine Kürzel.
#[test]
fn a62_tastenkuerzel() {
    let mut k = Shortcuts::default();
    let mut drueck = |key, mods| kuerzel(&mut k, key, true, mods, true);
    assert_eq!(drueck(Key::Char('N'), MODS_STRG).as_deref(), Some("Neu"));
    assert_eq!(drueck(Key::Char('O'), MODS_STRG).as_deref(), Some("Öffnen"));
    assert_eq!(
        drueck(Key::Char('S'), MODS_STRG).as_deref(),
        Some("Speichern")
    );
    assert_eq!(
        drueck(Key::Char('S'), MODS_STRG_UMSCHALT).as_deref(),
        Some("Speichern unter")
    );
    assert_eq!(
        drueck(Key::Char('W'), MODS_STRG).as_deref(),
        Some("Schließen")
    );
    assert_eq!(
        drueck(Key::Char('Z'), MODS_STRG).as_deref(),
        Some("Rückgängig")
    );
    assert_eq!(
        drueck(Key::Char('Y'), MODS_STRG).as_deref(),
        Some("Wiederherstellen")
    );
    assert_eq!(
        drueck(Key::Char('Z'), MODS_STRG_UMSCHALT).as_deref(),
        Some("Wiederherstellen")
    );
    assert_eq!(drueck(taste_f10(), M).as_deref(), Some("Menü"));
    assert_eq!(drueck(Key::Char('W'), M), None, "W allein ist nichts");

    // Alt allein: Menü erst beim Loslassen
    let mut k = Shortcuts::default();
    assert_eq!(kuerzel(&mut k, Key::Alt, true, MODS_ALT, true), None);
    assert_eq!(
        kuerzel(&mut k, Key::Alt, false, M, true).as_deref(),
        Some("Menü")
    );
    // Alt+F4 und Alt+Tab: kein Menü
    for zweite in [taste_f4(), Key::Tab] {
        let mut k = Shortcuts::default();
        kuerzel(&mut k, Key::Alt, true, MODS_ALT, true);
        kuerzel(&mut k, zweite, true, MODS_ALT, true);
        kuerzel(&mut k, zweite, false, MODS_ALT, true);
        assert_eq!(
            kuerzel(&mut k, Key::Alt, false, M, true),
            None,
            "{zweite:?}"
        );
    }
    // Werkzeug in Eingabe oder Ziehen: keine Kürzel
    let mut k = Shortcuts::default();
    for (key, mods) in [
        (Key::Char('W'), MODS_STRG),
        (Key::Char('Z'), MODS_STRG_UMSCHALT),
        (Key::Char('N'), MODS_STRG),
    ] {
        assert_eq!(kuerzel(&mut k, key, true, mods, false), None);
    }
}

/// A63 (E17 §2, §5, Test 5): Liste „Zuletzt geöffnet“. Neueste oben, ohne
/// Doppel, höchstens 8; fehlende Datei ausgegraut mit „nicht gefunden“, Klick
/// entfernt sie; „Liste leeren“; Rundlauf über einstellungen.txt.
#[test]
fn a63_zuletzt_geoeffnet() {
    let d = test_dir("zuletzt");
    let datei = |n: &str| {
        let p = d.join(format!("{n}.szo"));
        std::fs::write(&p, "SZO 3\n").unwrap();
        p
    };
    let (a, b, c) = (datei("A"), datei("B"), datei("C"));
    let mut r = Recent::default();
    let mut m = FileMenu::default();
    m.open();
    assert_eq!(
        unterzeilen(&m, &r),
        [("Keine Dateien".to_string(), String::new(), false)]
    );
    for p in [&a, &b, &c] {
        r.push(p.clone());
    }
    let namen = |m: &FileMenu, r: &Recent| {
        unterzeilen(m, r)
            .into_iter()
            .map(|z| z.0)
            .collect::<Vec<_>>()
    };
    assert_eq!(namen(&m, &r), ["C.szo", "B.szo", "A.szo", "Liste leeren"]);
    // Erneut geöffnet: nach oben, kein Doppel
    r.push(a.clone());
    assert_eq!(namen(&m, &r), ["A.szo", "C.szo", "B.szo", "Liste leeren"]);
    // Höchstens 8
    for i in 0..10 {
        r.push(datei(&format!("N{i}")));
    }
    let z = namen(&m, &r);
    assert_eq!(z.len(), 9, "8 Dateien + Liste leeren");
    assert_eq!(z[0], "N9.szo");
    // Eintrag wählen liefert seinen Platz
    let mut r = Recent::default();
    for p in [&a, &b, &c] {
        r.push(p.clone());
    }
    menue_taste(&mut m, taste_runter(), true, &r);
    menue_taste(&mut m, taste_runter(), true, &r);
    menue_taste(&mut m, Key::Right, true, &r);
    menue_taste(&mut m, taste_runter(), true, &r);
    assert_eq!(
        menue_taste(&mut m, Key::Enter, true, &r).as_deref(),
        Some("Zuletzt 1"),
        "zweiter Eintrag (B)"
    );
    // Umbenannt: ausgegraut, „nicht gefunden“
    std::fs::rename(&b, d.join("B2.szo")).unwrap();
    let z = unterzeilen(&m, &r);
    assert_eq!(z[1].0, "B.szo");
    assert!(!z[1].2 && z[1].1.contains("nicht gefunden"), "{:?}", z[1]);
    assert!(z[0].2 && z[2].2);
    r.remove(1);
    assert_eq!(namen(&m, &r), ["C.szo", "A.szo", "Liste leeren"]);
    // Rundlauf über die Einstellungsdatei, Schema bleibt erhalten
    let mut th = Theme::dark();
    th.rev = 7;
    let text = einstellungen_schreiben(&th, &r);
    assert!(text.contains("[zuletzt]"), "{text}");
    let i_c = text.find("C.szo").unwrap();
    let i_a = text.find("A.szo").unwrap();
    assert!(i_c < i_a, "neueste oben");
    let (th2, r2) = einstellungen_lesen(&text);
    assert_eq!(namen(&m, &r2), ["C.szo", "A.szo", "Liste leeren"]);
    assert_eq!(
        crate::settings::write(&th2),
        crate::settings::write(&th),
        "Schema unverändert"
    );
    assert_eq!(einstellungen_schreiben(&th2, &r2), text, "bytegleich");
    // Ohne [zuletzt] (alte Datei): leere Liste, kein Fehler
    let (_, r3) = einstellungen_lesen(&crate::settings::write(&th));
    assert_eq!(namen(&m, &r3), ["Keine Dateien"]);
    // Liste leeren
    r.clear();
    assert_eq!(namen(&m, &r), ["Keine Dateien"]);
    // Mit --ohne-einstellungen gibt es keine Datei, also auch keine Liste
    let st = crate::settings::Settings::new(
        ["skizzeo", "--ohne-einstellungen"]
            .map(String::from)
            .into_iter(),
        Some(d.clone()),
    );
    assert!(st.path.is_none());
    // Ordner in der Mitte gekürzt
    let lang = std::path::Path::new(r"C:\Projekte\2026\Wohnhaus Müller\Entwurf\Haus");
    let k = ordner_kurz(lang, 24);
    assert!(k.chars().count() <= 24, "{k}");
    assert!(
        k.starts_with(r"C:\Projekte\") && k.ends_with(r"\Haus") && k.contains('…'),
        "{k}"
    );
    assert_eq!(
        ordner_kurz(std::path::Path::new(r"C:\Haus"), 24),
        r"C:\Haus"
    );
    let _ = std::fs::remove_dir_all(&d);
}

/// A64 (E17 §3, Test 6): Nachfrage „Änderungen speichern?“ im eigenen
/// Fenster. Frage mit Dateiname, zweiter Satz mit der Uhrzeit des letzten
/// Speicherns (bei „Unbenannt“ ohne), drei Knöpfe, Enter = Speichern,
/// Esc = Abbrechen.
#[test]
fn a64_nachfrage_aenderungen_speichern() {
    let s = Scene::with_model(Model::with_seed(64));
    let mut doc = Document::new(s.model().revision());
    doc.mark_saved(std::path::PathBuf::from("Haus.szo"), s.model().revision());
    let (frage, satz) = nachfrage(&doc, Some((10, 42)));
    assert_eq!(frage, "Änderungen an „Haus“ speichern?");
    assert_eq!(
        satz.as_deref(),
        Some("Ohne Speichern gehen die Änderungen seit 10:42 verloren.")
    );
    let (_, satz) = nachfrage(&doc, Some((9, 5)));
    assert_eq!(
        satz.as_deref(),
        Some("Ohne Speichern gehen die Änderungen seit 09:05 verloren.")
    );
    let neu = Document::new(s.model().revision());
    let (frage, satz) = nachfrage(&neu, None);
    assert_eq!(frage, "Änderungen an „Unbenannt“ speichern?");
    assert_eq!(satz, None);
    let mut dlg = SaveDialog::default();
    assert_eq!(dlg.buttons(), ["Speichern", "Nicht speichern", "Abbrechen"]);
    assert_eq!(dlg.default_button(), 0, "Speichern ist Standard");
    assert_eq!(antwort(dlg.key(Key::Enter)), Some("Speichern"));
    assert_eq!(antwort(dlg.key(Key::Escape)), Some("Abbrechen"));
    assert_eq!(antwort(dlg.key(Key::Char('X'))), None);
    // Tab wandert zum nächsten Knopf, Enter löst ihn aus
    dlg.key(Key::Tab);
    assert_eq!(antwort(dlg.key(Key::Enter)), Some("Nicht speichern"));
}

/// A65 (ba88dc1, B12): Der Fuß einer OG-Innenwand liegt auf der Höhe des OG
/// (+2,855): dort greift das violette Band und verschiebt die Wand um genau
/// das gezogene Maß (vorher lag das Band auf dem Boden).
#[test]
fn a65_og_innenwand_fuss_auf_geschosshoehe() {
    let mut s = Scene::with_model(Model::with_seed(65));
    gebaeude(&mut s);
    let og = geschoss(&s, "OG");
    assert!(s.set_active_storey(og));
    let c = cam3d();
    let set = s.model().defaults().interior_wall;
    let mut t = tool(&s);
    t.set_category(sk_model::Category::InteriorWall, s.model().wall_layers(set));
    click(&mut t, &c, vec3(5000.0, 0.0, 2855.0));
    click(&mut t, &c, vec3(5000.0, 8000.0, 2855.0));
    let w = t.handle(&key(Key::Enter), &c, W, H, 1.0).commit.unwrap();
    let iw = s.add_wall_as(&w, sk_model::Category::InteriorWall).unwrap();
    let x_iw = |s: &Scene| s.chain(iw).unwrap().points[0].x;
    let ziehen = |s: &mut Scene, z: f64, dx: f64| {
        // Fuß in der Mitte der 17,5-cm-Wand (x 4825 … 5000)
        let mut e = WallEdit::default();
        let (x, y) = px(&c, vec3(4912.5, 4000.0, z));
        e.handle(&mv(x, y), s, &c, W, H, 1.0, true);
        e.handle(&down(x, y), s, &c, W, H, 1.0, true);
        let (x2, y2) = px(&c, vec3(4912.5 + dx, 4000.0, z));
        e.handle(&mv(x2, y2), s, &c, W, H, 1.0, true);
        e.handle(&up(x2, y2), s, &c, W, H, 1.0, true);
    };
    let x0 = x_iw(&s);
    assert_eq!(s.chain(iw).unwrap().base, 2855.0, "Fuß auf UK OG");
    // Auf +2,855: die Wand geht 1 m mit, Länge und Menge bleiben
    ziehen(&mut s, 2855.0, 1000.0);
    assert!(
        (x_iw(&s) - x0 - 1000.0).abs() < 1.0,
        "{} → {}",
        x0,
        x_iw(&s)
    );
    assert!((wall_m3(&s, iw, 0) - 3.398491).abs() < 5e-5);
    assert_eq!(
        s.model()
            .element(s.model().wall_at(iw, 0).unwrap())
            .unwrap()
            .storey,
        og
    );
    assert!(s.model().check().is_empty(), "{:?}", s.model().check());
    assert!(s.undo());
    assert_eq!(x_iw(&s), x0);
}

// Abnahmetests E5 „Einstellungsfenster: Gerüst, Reiter Stifte und
// Bedienoberfläche“ (einstellungen/paket-e5-einstellungsfenster.md),
// vorbereitet gegen main 650258e. Spezifikation: test/abnahme-einstellungen.md.
//
// Alle angenommenen Namen der neuen Schnittstellen stehen NUR in den Adaptern
// unten. Der Bauthread passt die Adapter an; die Tests bleiben unverändert.
// Fensterlage, Ziehen am Kopf, Farbwähler mit der Maus und das Sperren der
// Ansichten prüft der Handtest H43–H50.
//
// Braucht aus dem E17-Teil von abnahme.rs: `befehl`, `kuerzel`, `zeilen`,
// `MODS_STRG`, `test_dir`; `befehl` bekommt den Fall `Command::Settings` →
// "Einstellungen".

// ===== Adapter E5 =====

use crate::prefs::{Prefs, Tab};
use sk_model::attr::{Pen, PenId};

/// Fenster öffnen: beginnt den Schritt „Einstellungen geändert“ und merkt sich
/// das Schema.
fn oeffnen(s: &mut Scene, th: &Theme) -> Prefs {
    Prefs::open(s, th)
}

/// Eine Eingabe im Fenster (Projektseite), wirkt sofort.
fn eingabe(p: &mut Prefs, s: &mut Scene, f: impl FnOnce(&mut Model) -> bool) -> bool {
    p.edit(s, f)
}

/// Übernehmen: Projekt = ein Schritt (wenn geändert), Programm = Datei
/// schreiben; Fenster bleibt offen.
fn uebernehmen(p: &mut Prefs, s: &mut Scene, th: &Theme, st: &mut crate::settings::Settings) {
    p.apply(s, th, st)
}

/// OK: wie Übernehmen, dann schließen.
fn ok(p: &mut Prefs, s: &mut Scene, th: &Theme, st: &mut crate::settings::Settings) {
    p.ok(s, th, st)
}

/// Abbrechen: alles seit Öffnen bzw. letztem Übernehmen zurück.
fn abbrechen(p: &mut Prefs, s: &mut Scene, th: &mut Theme) {
    p.cancel(s, th)
}

/// „Auf Standard zurücksetzen“ im genannten Reiter (nach der Nachfrage).
fn zuruecksetzen(p: &mut Prefs, s: &mut Scene, th: &mut Theme, reiter: &str) {
    let tab = match reiter {
        "Stifte" => Tab::Pens,
        "Bedienoberfläche" => Tab::Ui,
        "Linientypen" => Tab::LineTypes,
        "Schraffuren" => Tab::Fills,
        "Oberflächen" => Tab::Surfaces,
        "Baustoffe" => Tab::Materials,
        _ => panic!("Reiter {reiter}"),
    };
    p.reset_tab(s, th, tab)
}

fn offen(p: &Prefs) -> bool {
    p.is_open()
}

/// Neuer Stift über den Knopf „Neu“.
fn stift_neu(p: &mut Prefs, s: &mut Scene) -> PenId {
    p.new_pen(s)
}

/// Stift löschen über den Knopf; `false` = gesperrt (wird verwendet).
fn stift_loeschen(p: &mut Prefs, s: &mut Scene, id: PenId) -> bool {
    p.remove_pen(s, id)
}

/// Verwender eines Stifts (Spalte „Verwendet“, Hinweis beim Darüberfahren).
fn stift_verwender(s: &Scene, id: PenId) -> Vec<String> {
    crate::prefs::pen_users(s.model(), id)
}

/// Satz unter „Strichstärke am Bildschirm“.
fn px_satz(px_per_mm: f32) -> String {
    crate::prefs::px_hint(px_per_mm)
}

/// Farbwähler: Hex-Text und zurück, HSV.
fn hex(c: [u8; 3]) -> String {
    crate::prefs::to_hex(c)
}
fn hex_lesen(t: &str) -> Option<[u8; 3]> {
    crate::prefs::parse_hex(t)
}
fn hsv(c: [u8; 3]) -> [f32; 3] {
    let (h, s, v) = sk_paint::rgb_to_hsv(sk_paint::Rgba::rgb(c[0], c[1], c[2]));
    [h, s, v]
}
fn rgb(h: [f32; 3]) -> [u8; 3] {
    let c = sk_paint::hsv_to_rgb(h[0], h[1], h[2]);
    [c.0, c.1, c.2]
}

/// Himmel: neue Enden setzen, Zwischenstufen im selben Verhältnis.
fn himmel_enden(th: &mut Theme, unten: sk_paint::Rgba, oben: sk_paint::Rgba) {
    crate::prefs::set_sky_ends(th, unten, oben)
}

/// Neue Quelldateien des Fensters (für Prüfregel 1).
const E5_DATEIEN: [&str; 3] = [
    "app/src/prefs.rs",
    "app/src/prefs_attr.rs",
    "crates/sk-ui/src/widgets.rs",
];

// ===== Hilfen =====

fn stift(s: &Scene, nr: u16) -> (PenId, Pen) {
    s.model()
        .attr()
        .pens()
        .iter()
        .find(|(_, p)| p.number == nr)
        .map(|(id, p)| (id, p.clone()))
        .unwrap_or_else(|| panic!("Stift {nr}"))
}

fn breite(s: &mut Scene, nr: u16, mm: f32) -> impl FnOnce(&mut Model) -> bool {
    let (id, mut pen) = stift(s, nr);
    pen.width_mm = mm;
    move |m: &mut Model| m.set_pen(id, pen)
}

fn szo(s: &Scene) -> String {
    sk_model::szo::write(s.model())
}

/// Zeichentabelle wie die Grafikkarte sie bekommt.
fn tabelle(s: &Scene, th: &Theme) -> Vec<[f32; 4]> {
    // Baustofftexel und dazu Breite und Farbe je Kantenart (Zeichnung, 3D)
    let l = DrawTable::resolve(s.model(), th).looks(1.0);
    let mut t = l.texels;
    for e in [l.drawing, l.model] {
        t.extend(
            (0..e.width.len()).map(|i| [e.width[i], e.color[i][0], e.color[i][1], e.color[i][2]]),
        );
    }
    t
}

fn ohne_datei() -> crate::settings::Settings {
    crate::settings::Settings::new(
        ["skizzeo", "--ohne-einstellungen"]
            .map(String::from)
            .into_iter(),
        None,
    )
}

fn haus_mit_wand() -> Scene {
    let mut s = Scene::with_model(Model::with_seed(66));
    zeichne_rechteck(&mut s, &cam3d());
    s
}

// ===== Tests =====

/// A66 (E5 §1): Aufruf über das Dateimenü („Einstellungen …“, eigene Gruppe
/// zwischen „Speichern unter …“ und „Schließen“) und Strg+Komma; das Kürzel
/// gilt nicht während einer Eingabe.
#[test]
fn a66_einstellungen_aufrufen() {
    let mut m = FileMenu::default();
    m.open();
    let z = zeilen(&m, true, &Recent::default());
    let i = z
        .iter()
        .position(|l| l.0 == "Einstellungen …")
        .expect("Eintrag");
    assert_eq!(z[i].1, "Strg+Komma");
    assert!(z[i].2, "aktiv");
    assert_eq!(z[i - 2].0, "Speichern unter …");
    assert_eq!(z[i - 1].0, "—", "eigene Gruppe");
    // K3: „Bauteilkatalog …“ in derselben Gruppe
    assert_eq!(z[i + 1].0, "Bauteilkatalog …");
    assert_eq!(z[i + 2].0, "—");
    assert_eq!(z[i + 3].0, "Schließen");
    let komma = Key::Other(0xBC);
    let mut k = Shortcuts::default();
    assert_eq!(
        kuerzel(&mut k, komma, true, MODS_STRG, true).as_deref(),
        Some("Einstellungen")
    );
    assert_eq!(kuerzel(&mut k, komma, true, MODS_STRG, false), None);
    assert_eq!(kuerzel(&mut k, komma, true, M, true), None, "Komma allein");
}

/// A67 (E5 §2, Test 2): Abbrechen stellt das Projekt bitgenau her: gleiche
/// .szo, gleiche Zeichentabelle, kein Rückgängig-Eintrag, Titel ohne „•“.
#[test]
fn a67_abbrechen_bitgenau() {
    let mut s = haus_mit_wand();
    let mut th = Theme::dark();
    let doc = crate::document::Document::new(s.model().revision());
    let (vor_szo, vor_tab, vor_undo) = (szo(&s), tabelle(&s, &th), s.undo_label());
    let mut p = oeffnen(&mut s, &th);
    assert!(offen(&p));
    // Breite, Farbe, Name ändern, neuen Stift anlegen
    let f = breite(&mut s, 3, 0.70);
    assert!(eingabe(&mut p, &mut s, f));
    let (id3, mut p3) = stift(&s, 3);
    p3.color = [192, 57, 43];
    p3.name = "Kräftig rot".into();
    assert!(eingabe(&mut p, &mut s, move |m| m.set_pen(id3, p3)));
    stift_neu(&mut p, &mut s);
    // Live: die Tabelle ist schon anders, CUT-Kanten 1,4-mal so breit
    assert_ne!(tabelle(&s, &th), vor_tab, "wirkt sofort");
    let cut =
        |s: &Scene, th: &Theme| DrawTable::resolve(s.model(), th).edge_width(true, edge_kind::CUT);
    let jetzt = cut(&s, &th);
    abbrechen(&mut p, &mut s, &mut th);
    assert!(!offen(&p));
    assert!((jetzt / cut(&s, &th) - 1.4).abs() < 1e-4, "0,70 / 0,50");
    assert_eq!(szo(&s), vor_szo, ".szo bitgenau");
    assert_eq!(tabelle(&s, &th), vor_tab, "Zeichentabelle bitgenau");
    assert_eq!(s.undo_label(), vor_undo, "kein Rückgängig-Eintrag");
    assert!(!doc.is_dirty(s.model()), "Titel ohne •");
    assert_eq!(stift(&s, 3).1.width_mm, 0.50);
}

/// A68 (E5 §2, Test 2): OK erzeugt genau einen Rückgängig-Schritt
/// „Einstellungen geändert“; Strg+Z stellt alles in einem Schritt her.
/// OK ohne Änderung erzeugt keinen.
#[test]
fn a68_ok_ein_rueckgaengig_schritt() {
    let mut s = haus_mit_wand();
    let th = Theme::dark();
    let mut st = ohne_datei();
    let vor = szo(&s);
    let vor_label = s.undo_label();
    let mut p = oeffnen(&mut s, &th);
    let f = breite(&mut s, 3, 0.70);
    assert!(eingabe(&mut p, &mut s, f));
    let f = breite(&mut s, 1, 0.18);
    assert!(eingabe(&mut p, &mut s, f));
    let neu = stift_neu(&mut p, &mut s);
    ok(&mut p, &mut s, &th, &mut st);
    assert!(!offen(&p));
    assert_eq!(s.undo_label(), Some("Einstellungen geändert"));
    let nach = szo(&s);
    assert!(s.undo(), "ein Schritt");
    assert_eq!(szo(&s), vor, "alles in einem Schritt zurück");
    assert_eq!(s.undo_label(), vor_label);
    assert!(s.model().attr().pen(neu).is_none());
    assert!(s.redo());
    assert_eq!(szo(&s), nach);
    assert_eq!(stift(&s, 3).1.width_mm, 0.70);
    // OK ohne Änderung: kein Eintrag
    let mut p = oeffnen(&mut s, &th);
    ok(&mut p, &mut s, &th, &mut st);
    assert_eq!(s.undo_label(), Some("Einstellungen geändert"));
    assert!(s.undo());
    assert_eq!(szo(&s), vor, "nur der eine Schritt von oben lag darauf");
}

/// A69 (E5 §2, Test 3): Übernehmen mit Änderungen dazwischen → zwei Schritte;
/// Übernehmen ohne Änderung → keiner; Abbrechen danach nimmt nur zurück, was
/// seit dem letzten Übernehmen kam.
#[test]
fn a69_uebernehmen() {
    let mut s = haus_mit_wand();
    let mut th = Theme::dark();
    let mut st = ohne_datei();
    let vor = szo(&s);
    let mut p = oeffnen(&mut s, &th);
    let f = breite(&mut s, 3, 0.70);
    eingabe(&mut p, &mut s, f);
    uebernehmen(&mut p, &mut s, &th, &mut st);
    assert!(offen(&p), "Fenster bleibt offen");
    let nach1 = szo(&s);
    let f = breite(&mut s, 2, 0.35);
    eingabe(&mut p, &mut s, f);
    uebernehmen(&mut p, &mut s, &th, &mut st);
    let nach2 = szo(&s);
    uebernehmen(&mut p, &mut s, &th, &mut st);
    let f = breite(&mut s, 1, 0.25);
    eingabe(&mut p, &mut s, f);
    abbrechen(&mut p, &mut s, &mut th);
    assert_eq!(szo(&s), nach2, "Abbrechen nur bis zum letzten Übernehmen");
    assert!(s.undo());
    assert_eq!(szo(&s), nach1);
    assert!(s.undo());
    assert_eq!(szo(&s), vor, "genau zwei Schritte");
}

/// A70 (E5 §4, Tests 5, 6; BIM-Bedingungen 1, 3, 4): Stift neu/löschen,
/// Löschen gesperrt bei verwendeten Stiften (Baustoffe, Display-Slots),
/// Nummern nie neu vergeben, check() meldet doppelte Nummern; .szo nach dem
/// Löschen verlustfrei und bytegleich.
#[test]
fn a70_stifte_neu_loeschen_speichern() {
    let mut s = haus_mit_wand();
    let th = Theme::dark();
    let mut st = ohne_datei();
    let mut p = oeffnen(&mut s, &th);
    let neu = stift_neu(&mut p, &mut s);
    let pen = s.model().attr().pen(neu).unwrap().clone();
    assert_eq!(
        (pen.number, pen.name.as_str(), pen.color, pen.width_mm),
        (10, "Stift 10", [0, 0, 0], 0.25)
    );
    assert!(stift_verwender(&s, neu).is_empty());
    let (id3, _) = stift(&s, 3);
    assert!(
        !stift_verwender(&s, id3).is_empty(),
        "Stift 3 zeichnet Schnittkanten"
    );
    assert!(!stift_loeschen(&mut p, &mut s, id3), "gesperrt");
    assert!(s.model().attr().pen(id3).is_some());
    // BIM 1: Verweise aus Display-Slots (und Baustoffen) sperren das Löschen
    let zweiter = stift_neu(&mut p, &mut s);
    assert_eq!(s.model().attr().pen(zweiter).unwrap().number, 11);
    let mut disp = s.model().attr().display().clone();
    let alt_bg = disp.background;
    disp.background.pen = zweiter;
    eingabe(&mut p, &mut s, move |m| {
        m.set_display(disp);
        true
    });
    assert!(!stift_verwender(&s, zweiter).is_empty());
    assert!(
        !stift_loeschen(&mut p, &mut s, zweiter),
        "Hintergrund nutzt ihn"
    );
    let mut disp = s.model().attr().display().clone();
    disp.background = alt_bg;
    eingabe(&mut p, &mut s, move |m| {
        m.set_display(disp);
        true
    });
    // BIM 3: Nummern werden nicht neu vergeben (höchste + 1)
    let dritter = stift_neu(&mut p, &mut s);
    assert_eq!(s.model().attr().pen(dritter).unwrap().number, 12);
    assert!(stift_loeschen(&mut p, &mut s, zweiter));
    assert!(s.model().attr().pen(zweiter).is_none());
    let vierter = stift_neu(&mut p, &mut s);
    assert_eq!(
        s.model().attr().pen(vierter).unwrap().number,
        13,
        "die 11 kommt nicht wieder"
    );
    assert!(stift_loeschen(&mut p, &mut s, vierter));
    assert!(stift_loeschen(&mut p, &mut s, dritter));
    // check() meldet doppelte Nummern
    let (id1, mut doppelt) = stift(&s, 1);
    doppelt.number = 3;
    eingabe(&mut p, &mut s, move |m| m.set_pen(id1, doppelt));
    assert!(!s.model().check().is_empty(), "Nummer 3 doppelt");
    // Nach der Doppelung gibt es keine Nummer 1 mehr: Stift über die Kennung
    let mut eins = s.model().attr().pen(id1).unwrap().clone();
    eins.number = 1;
    eingabe(&mut p, &mut s, move |m| m.set_pen(id1, eins));
    let f = breite(&mut s, 3, 0.70);
    eingabe(&mut p, &mut s, f);
    let (id10, mut p10) = (neu, pen);
    p10.name = "Achse".into();
    p10.color = [192, 57, 43];
    p10.width_mm = 0.35;
    eingabe(&mut p, &mut s, move |m| m.set_pen(id10, p10));
    ok(&mut p, &mut s, &th, &mut st);
    assert!(s.model().check().is_empty(), "{:?}", s.model().check());
    // Speichern und Öffnen: alles da, zweimal speichern bytegleich
    let d = test_dir("stifte");
    let pfad = d.join("Haus.szo");
    crate::document::save(s.model(), &pfad).unwrap();
    let text = std::fs::read_to_string(&pfad).unwrap();
    let loaded = crate::document::load(&pfad).unwrap();
    assert!(loaded.hints.is_empty(), "{:?}", loaded.hints);
    let t = Scene::with_model(loaded.model);
    assert_eq!(stift(&t, 3).1.width_mm, 0.70);
    let p10 = stift(&t, 10).1;
    assert_eq!(
        (p10.name.as_str(), p10.color, p10.width_mm),
        ("Achse", [192, 57, 43], 0.35)
    );
    assert_eq!(
        p10.guid,
        s.model().attr().pen(neu).unwrap().guid,
        "Guid bleibt"
    );
    let pfad2 = d.join("Haus2.szo");
    crate::document::save(t.model(), &pfad2).unwrap();
    assert_eq!(std::fs::read_to_string(&pfad2).unwrap(), text, "bytegleich");
    let _ = std::fs::remove_dir_all(&d);
}

/// A71 (E5 §5, Test 7): Programmeinstellungen. Akzent wirkt sofort;
/// Abbrechen stellt das Schema her und schreibt nichts; Übernehmen schreibt
/// nur die Abweichungen, die beim Lesen genau dieses Schema ergeben.
#[test]
fn a71_bedienoberflaeche_speichern() {
    let d = test_dir("bedien");
    let mut st =
        crate::settings::Settings::new(["skizzeo"].map(String::from).into_iter(), Some(d.clone()));
    let datei = st.path.clone().unwrap();
    let mut s = Scene::with_model(Model::with_seed(71));
    let mut th = st.load();
    let vor = th.clone();
    let blau = sk_paint::Rgba::rgb(47, 111, 208);
    let mut p = oeffnen(&mut s, &th);
    th.set_accent(blau);
    assert_eq!(th.ui.accent, blau, "wirkt sofort");
    abbrechen(&mut p, &mut s, &mut th);
    assert_eq!(th, vor, "Schema wie vorher");
    assert!(!datei.exists(), "keine Datei geschrieben");
    let mut p = oeffnen(&mut s, &th);
    th.set_accent(blau);
    uebernehmen(&mut p, &mut s, &th, &mut st);
    assert!(datei.exists());
    let text = std::fs::read_to_string(&datei).unwrap();
    let (gelesen, hints) = crate::settings::read(&text);
    assert!(hints.is_empty(), "{hints:?}");
    assert_eq!(gelesen.ui.accent, blau);
    assert_eq!(
        crate::settings::write(&gelesen),
        crate::settings::write(&th),
        "verlustfrei"
    );
    let dunkel = crate::settings::write(&Theme::dark());
    let abw = text.lines().filter(|l| !dunkel.contains(*l)).count();
    assert!((1..=3).contains(&abw), "nur Abweichungen ({abw}):\n{text}");
    // Abbrechen nach Übernehmen: bleibt blau, Datei unverändert
    th.set_accent(sk_paint::Rgba::rgb(200, 30, 30));
    abbrechen(&mut p, &mut s, &mut th);
    assert_eq!(th.ui.accent, blau);
    assert_eq!(std::fs::read_to_string(&datei).unwrap(), text);
    let _ = std::fs::remove_dir_all(&d);
}

/// A72 (E5 §2, Test 8): „Auf Standard zurücksetzen“ wirkt nur im aktiven
/// Reiter: Stifte → Startsatz (eigene neue Stifte bleiben), Bedienoberfläche
/// → `Theme::dark()`; bis OK noch abbrechbar.
#[test]
fn a72_auf_standard_zuruecksetzen() {
    let mut s = haus_mit_wand();
    let mut th = Theme::dark();
    let mut st = ohne_datei();
    let start = szo(&s);
    let start_stifte: Vec<Pen> = s
        .model()
        .attr()
        .pens()
        .iter()
        .map(|(_, p)| p.clone())
        .collect();
    let mut p = oeffnen(&mut s, &th);
    let f = breite(&mut s, 3, 0.70);
    eingabe(&mut p, &mut s, f);
    let neu = stift_neu(&mut p, &mut s);
    th.set_accent(sk_paint::Rgba::rgb(47, 111, 208));
    th.size.font *= 1.5;
    let th_geaendert = th.clone();
    // Stifte zurück: Startsatz wieder da, Stift 10 bleibt, Schema unberührt
    zuruecksetzen(&mut p, &mut s, &mut th, "Stifte");
    for alt in &start_stifte {
        assert_eq!(&stift(&s, alt.number).1, alt, "Stift {}", alt.number);
    }
    assert!(s.model().attr().pen(neu).is_some(), "eigener Stift bleibt");
    assert_eq!(th, th_geaendert, "Bedienoberfläche unberührt");
    // Bedienoberfläche zurück: Theme::dark(), Stifte unberührt
    let f = breite(&mut s, 3, 0.70);
    eingabe(&mut p, &mut s, f);
    zuruecksetzen(&mut p, &mut s, &mut th, "Bedienoberfläche");
    assert_eq!(
        crate::settings::write(&th),
        crate::settings::write(&Theme::dark())
    );
    assert_eq!(stift(&s, 3).1.width_mm, 0.70, "Stifte unberührt");
    // Noch umkehrbar
    abbrechen(&mut p, &mut s, &mut th);
    assert_eq!(szo(&s), start);
    // Zurücksetzen + OK: ein Schritt
    let mut p = oeffnen(&mut s, &th);
    let f = breite(&mut s, 3, 0.70);
    eingabe(&mut p, &mut s, f);
    ok(&mut p, &mut s, &th, &mut st);
    let mut p = oeffnen(&mut s, &th);
    zuruecksetzen(&mut p, &mut s, &mut th, "Stifte");
    ok(&mut p, &mut s, &th, &mut st);
    assert_eq!(szo(&s), start);
    assert!(s.undo());
    assert_eq!(stift(&s, 3).1.width_mm, 0.70);
}

/// A73 (E5 §3, §5, Prüfregel 1): Hilfen des Fensters. Satz zur Strichstärke,
/// Hex und HSV im Farbwähler, Himmelsenden mit Zwischenstufen im selben
/// Verhältnis; kein Farbliteral im neuen Code.
#[test]
fn a73_hilfen_und_pruefregel_1() {
    assert_eq!(px_satz(5.5), "Eine 0,50-mm-Linie ist 2,8 px breit");
    assert_eq!(px_satz(8.0), "Eine 0,50-mm-Linie ist 4,0 px breit");
    assert_eq!(hex([192, 57, 43]), "#C0392B");
    assert_eq!(hex_lesen("#c0392b"), Some([192, 57, 43]));
    assert_eq!(hex_lesen("C0392B"), Some([192, 57, 43]), "ohne # geht auch");
    assert_eq!(hex_lesen("#C0392"), None);
    assert_eq!(hex_lesen("#GG0000"), None);
    for c in [
        [192, 57, 43],
        [0, 0, 0],
        [255, 255, 255],
        [47, 111, 208],
        [160, 160, 160],
    ] {
        assert_eq!(rgb(hsv(c)), c, "HSV-Rundlauf {c:?}");
    }
    // Himmel: Verhältnis je Kanal bleibt
    let mut th = Theme::dark();
    let alt = th.env.sky.clone();
    let (u, o) = (
        sk_paint::Rgba::rgb(200, 220, 240),
        sk_paint::Rgba::rgb(20, 60, 120),
    );
    himmel_enden(&mut th, u, o);
    let neu = &th.env.sky;
    assert_eq!(neu.len(), alt.len());
    assert_eq!((neu[0].1, neu[neu.len() - 1].1), (u, o));
    let ch = |c: sk_paint::Rgba| [c.0 as f32, c.1 as f32, c.2 as f32];
    let (a0, a1) = (ch(alt[0].1), ch(alt[alt.len() - 1].1));
    let (n0, n1) = (ch(u), ch(o));
    for (i, ((ta, ca), (tn, cn))) in alt.iter().zip(neu.iter()).enumerate() {
        assert_eq!(ta, tn, "Lage der Stufe {i} bleibt");
        for k in 0..3 {
            if (a1[k] - a0[k]).abs() > 2.0 {
                let r = (ch(*ca)[k] - a0[k]) / (a1[k] - a0[k]);
                let soll = n0[k] + r * (n1[k] - n0[k]);
                assert!((ch(*cn)[k] - soll).abs() <= 1.0, "Stufe {i} Kanal {k}");
            }
        }
    }
    // Prüfregel 1: kein Farbliteral außerhalb von Theme::dark() und Tests
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    for f in E5_DATEIEN {
        let text = std::fs::read_to_string(root.join(f)).unwrap();
        let code = text.split("#[cfg(test)]").next().unwrap();
        for (n, z) in code.lines().enumerate() {
            let z = z.split("//").next().unwrap();
            for muster in ["Rgba::rgb(", "from_rgb8([", "Rgba::from_f32(["] {
                if let Some(i) = z.find(muster) {
                    let rest = z[i + muster.len()..].trim_start();
                    assert!(
                        !rest.starts_with(|c: char| c.is_ascii_digit()),
                        "Farbliteral in {f}:{}: {z}",
                        n + 1
                    );
                }
            }
        }
    }
}
// Abnahmetests E4 + E6 „Linientypen im Kanten-Shader; Reiter Schraffuren,
// Linientypen, Oberflächen, Baustoffe“ (einstellungen/paket-e4-e6-attribute.md)
// mit den vier BIM-Bedingungen (Koordinator 11:41), vorbereitet gegen main
// 650258e. Spezifikation: test/abnahme-einstellungen.md.
//
// Baut auf dem E5-Teil auf (a66-a73-einstellungsfenster.rs: `oeffnen`,
// `eingabe`, `ok`, `abbrechen`, `zuruecksetzen`, `stift`, `szo`, `tabelle`,
// `ohne_datei`, `haus_mit_wand`). Angenommene Namen nur in den Adaptern.
// Pixelvergleiche (Strichmuster am Bildschirm, Vorschau gegen Shader) prüft
// der Bauthread mit Bildvergleich bzw. Unit-Test; hier die Daten dahinter.

// ===== Adapter E4 + E6 =====

use sk_model::attr::{Dash, Fill, FillId, HatchLine, LineType, LineTypeId, SurfaceId};
use sk_model::{MaterialDisplay, MaterialId};

/// Strichmuster einer Kantenart, wie es an den Shader geht (zwei Einträge
/// `[len_px, gap_px, dot, 0]`, alles 0 = Volllinie).
fn strichmuster(s: &Scene, th: &Theme, drawing: bool, kind: u8) -> [[f32; 4]; 2] {
    let d = DrawTable::resolve(s.model(), th)
        .edge_looks(drawing, 1.0)
        .dash;
    [d[2 * kind as usize], d[2 * kind as usize + 1]]
}

/// Strichmuster der Schnittlinie A–A (Hilfslinien-Shader).
fn schnittlinie_muster(s: &Scene, th: &Theme) -> [[f32; 4]; 2] {
    crate::section::dash_pattern(s.model(), th)
}

fn linientyp_aendern(m: &mut Model, id: LineTypeId, l: LineType) -> bool {
    m.set_line_type(id, l)
}
fn linientyp_loeschen(m: &mut Model, id: LineTypeId) -> bool {
    m.remove_line_type(id)
}
fn schraffur_loeschen(m: &mut Model, id: FillId) -> bool {
    m.remove_fill(id)
}
fn oberflaeche_loeschen(m: &mut Model, id: SurfaceId) -> bool {
    m.remove_surface(id)
}

/// Baustoffverweise ändern (nur cut_fill, cut_fg, cut_bg, surface).
fn baustoff_darstellung(
    m: &mut Model,
    id: MaterialId,
    f: impl FnOnce(&mut MaterialDisplay),
) -> bool {
    let Some(mat) = m.material(id).cloned() else {
        return false;
    };
    let mut d = MaterialDisplay {
        cut_fill: mat.cut_fill,
        cut_fg: mat.cut_fg,
        cut_bg: mat.cut_bg,
        surface: mat.surface,
    };
    f(&mut d);
    m.set_material_display(id, d)
}

/// Neue Einträge über den Knopf „Neu“ im jeweiligen Reiter.
fn linientyp_neu(p: &mut Prefs, s: &mut Scene) -> LineTypeId {
    p.new_line_type(s)
}
fn schraffur_neu(p: &mut Prefs, s: &mut Scene) -> FillId {
    p.new_fill(s)
}

/// Name im Reiter noch frei? (doppelter Name = ungültig)
fn name_frei(s: &Scene, reiter: &str, name: &str) -> bool {
    let tab = match reiter {
        "Stifte" => Tab::Pens,
        "Linientypen" => Tab::LineTypes,
        "Schraffuren" => Tab::Fills,
        "Oberflächen" => Tab::Surfaces,
        _ => panic!("{reiter}"),
    };
    crate::prefs::name_free(s.model(), tab, name)
}

// ===== Hilfen =====

fn linientyp(s: &Scene, name: &str) -> (LineTypeId, LineType) {
    s.model()
        .attr()
        .line_types()
        .iter()
        .find(|(_, l)| l.name == name)
        .map(|(id, l)| (id, l.clone()))
        .unwrap_or_else(|| panic!("Linientyp {name}"))
}

fn schraffur(s: &Scene, name: &str) -> (FillId, Fill) {
    s.model()
        .attr()
        .fills()
        .iter()
        .find(|(_, f)| f.name == name)
        .map(|(id, f)| (id, f.clone()))
        .unwrap_or_else(|| panic!("Schraffur {name}"))
}

fn baustoff(s: &Scene, name: &str) -> (MaterialId, sk_model::Material) {
    s.model()
        .materials()
        .iter()
        .find(|(_, m)| m.name == name)
        .map(|(id, m)| (id, m.clone()))
        .unwrap_or_else(|| panic!("Baustoff {name}"))
}

const NULL: [[f32; 4]; 2] = [[0.0; 4]; 2];

// ===== Tests =====

/// A74 (E4 A1–A3, Tests 1–3): Startsatz der Linientypen; alle Kanten bleiben
/// Volllinie (Muster 0, damit bitgleich zu vorher); die Schnittlinie A–A kommt
/// aus den Attributen (Strichpunkt); Strichlinie an den Ansichtskanten gibt
/// 3,0/1,0 mm × px_per_mm.
#[test]
fn a74_linientypen_startsatz_und_shadermuster() {
    let mut s = Scene::with_model(Model::with_seed(74));
    let th = Theme::dark();
    let namen: Vec<String> = s
        .model()
        .attr()
        .line_types()
        .iter()
        .map(|(_, l)| l.name.clone())
        .collect();
    assert_eq!(
        namen,
        ["Volllinie", "Strichlinie", "Strichpunktlinie", "Punktlinie"]
    );
    let d = |len_mm, gap_mm, dot| Dash {
        len_mm,
        gap_mm,
        dot,
    };
    assert!(linientyp(&s, "Volllinie").1.pattern.is_empty());
    assert_eq!(linientyp(&s, "Strichlinie").1.pattern, [d(3.0, 1.0, false)]);
    assert_eq!(
        linientyp(&s, "Strichpunktlinie").1.pattern[0],
        d(6.0, 1.0, true)
    );
    assert_eq!(linientyp(&s, "Punktlinie").1.pattern[0], d(0.0, 1.0, true));
    for l in s.model().attr().line_types().iter().map(|(_, l)| l) {
        assert!(l.pattern.len() <= 2, "{}: höchstens zwei Einträge", l.name);
    }
    for kind in 0..edge_kind::COUNT as u8 {
        assert_eq!(strichmuster(&s, &th, true, kind), NULL, "Zeichnung {kind}");
        assert_eq!(strichmuster(&s, &th, false, kind), NULL, "3D {kind}");
    }
    let (spl, _) = linientyp(&s, "Strichpunktlinie");
    assert_eq!(s.model().attr().display().section_line.line_type, spl);
    let px = th.px_per_mm;
    let m = schnittlinie_muster(&s, &th);
    assert_eq!((m[0][0], m[0][1], m[0][2]), (6.0 * px, 1.0 * px, 1.0));
    // Ansichtskanten auf Strichlinie: nur VIEW gestrichelt
    let (strich, _) = linientyp(&s, "Strichlinie");
    let mut disp = s.model().attr().display().clone();
    disp.drawing[edge_kind::VIEW as usize].line_type = strich;
    assert!(s.edit_model("Linientyp", |m| {
        m.set_display(disp);
        true
    }));
    assert_eq!(
        strichmuster(&s, &th, true, edge_kind::VIEW),
        [[3.0 * px, 1.0 * px, 0.0, 0.0], [0.0; 4]]
    );
    assert_eq!(strichmuster(&s, &th, true, edge_kind::CUT), NULL);
    assert_eq!(
        strichmuster(&s, &th, false, edge_kind::VIEW),
        NULL,
        "3D unverändert"
    );
}

/// A75 (E4 A4, Test 5): Alte Datei ohne die neuen Linientypen: sie werden
/// ergänzt, die Schnittlinie zeigt auf Strichpunkt (sieht aus wie bisher),
/// Hinweis „Linientypen ergänzt“; danach bytegleicher Rundlauf ohne Hinweis.
#[test]
fn a75_alte_datei_bekommt_linientypen() {
    let d = test_dir("linientypen");
    let alt = d.join("Alt.szo");
    std::fs::write(&alt, HAUS_SZO1).unwrap();
    let loaded = crate::document::load(&alt).unwrap();
    assert!(
        loaded
            .hints
            .iter()
            .any(|h| h.contains("Linientypen ergänzt")),
        "{:?}",
        loaded.hints
    );
    let t = Scene::with_model(loaded.model);
    assert_eq!(t.model().attr().line_types().len(), 4);
    let (spl, _) = linientyp(&t, "Strichpunktlinie");
    assert_eq!(t.model().attr().display().section_line.line_type, spl);
    let th = Theme::dark();
    for kind in 0..edge_kind::COUNT as u8 {
        assert_eq!(strichmuster(&t, &th, true, kind), NULL);
    }
    let p2 = d.join("Neu.szo");
    crate::document::save(t.model(), &p2).unwrap();
    let text = std::fs::read_to_string(&p2).unwrap();
    let again = crate::document::load(&p2).unwrap();
    assert!(again.hints.is_empty(), "{:?}", again.hints);
    let p3 = d.join("Neu2.szo");
    crate::document::save(&again.model, &p3).unwrap();
    assert_eq!(std::fs::read_to_string(&p3).unwrap(), text, "bytegleich");
    let _ = std::fs::remove_dir_all(&d);
}

/// A76 (E6 B1, Test 1; BIM 1, 2): Linientyp im Fenster ändern wirkt sofort
/// auf die Kanten, Abbrechen bitgenau; Löschen nur ohne Verwender; tote
/// Kennungen werden abgelehnt; jede Änderung ein Rückgängig-Schritt.
#[test]
fn a76_linientypen_bearbeiten() {
    let mut s = haus_mit_wand();
    let mut th = Theme::dark();
    let px = th.px_per_mm;
    let (strich, mut l) = linientyp(&s, "Strichlinie");
    let mut disp = s.model().attr().display().clone();
    disp.drawing[edge_kind::VIEW as usize].line_type = strich;
    s.edit_model("Linientyp", |m| {
        m.set_display(disp);
        true
    });
    let vor = szo(&s);
    let mut p = oeffnen(&mut s, &th);
    l.pattern[0].len_mm = 5.0;
    let l2 = l.clone();
    assert!(eingabe(&mut p, &mut s, move |m| linientyp_aendern(
        m, strich, l2
    )));
    assert_eq!(
        strichmuster(&s, &th, true, edge_kind::VIEW)[0][0],
        5.0 * px,
        "sofort"
    );
    abbrechen(&mut p, &mut s, &mut th);
    assert_eq!(szo(&s), vor, "bitgenau");
    assert_eq!(strichmuster(&s, &th, true, edge_kind::VIEW)[0][0], 3.0 * px);
    // Löschen: verwendet → nein; neu und unbenutzt → ja
    let (spl, _) = linientyp(&s, "Strichpunktlinie");
    assert!(
        !s.edit_model("Löschen", |m| linientyp_loeschen(m, spl)),
        "Schnittlinie"
    );
    assert!(
        !s.edit_model("Löschen", |m| linientyp_loeschen(m, strich)),
        "VIEW"
    );
    assert_eq!(szo(&s), vor, "abgelehnt ändert nichts");
    let mut p = oeffnen(&mut s, &th);
    let neu = linientyp_neu(&mut p, &mut s);
    ok(&mut p, &mut s, &th, &mut ohne_datei());
    let mit_neu = szo(&s);
    assert!(s.edit_model("Löschen", |m| linientyp_loeschen(m, neu)));
    assert_eq!(s.undo_label(), Some("Löschen"), "Löschen = ein Schritt");
    let geloescht = szo(&s);
    // Tote Kennung: abgelehnt, nichts ändert sich
    let tot = l.clone();
    assert!(!s.edit_model("Tot", |m| linientyp_aendern(m, neu, tot)));
    assert_eq!(szo(&s), geloescht);
    assert!(!s.edit_model("Tot", |m| linientyp_loeschen(m, neu)));
    assert!(s.undo());
    assert_eq!(szo(&s), mit_neu, "Löschen zurück");
    assert!(s.model().check().is_empty(), "{:?}", s.model().check());
}

/// A77 (E6 B2, Tests 2, 4; BIM 1, 4): Schraffur Mauerwerk 135° → 45° wirkt
/// sofort, Abbrechen → 135°. Neue Schraffur „Kies“, Putz verweist darauf:
/// Löschen gesperrt, bis Putz wieder „Leer“ hat; danach .szo bytegleich.
#[test]
fn a77_schraffuren_bearbeiten() {
    let mut s = haus_mit_wand();
    let mut th = Theme::dark();
    let vor = szo(&s);
    let vor_tab = tabelle(&s, &th);
    let (mw, mut f) = schraffur(&s, "Mauerwerk");
    let mut p = oeffnen(&mut s, &th);
    let FillKind::Lines(l) = &mut f.kind else {
        panic!("Linien erwartet")
    };
    assert_eq!(l[0].angle_deg, 135.0);
    l[0].angle_deg = 45.0;
    assert!(eingabe(&mut p, &mut s, move |m| m.set_fill(mw, f)));
    assert_ne!(tabelle(&s, &th), vor_tab, "Gasbeton sofort „/“");
    abbrechen(&mut p, &mut s, &mut th);
    assert_eq!(szo(&s), vor);
    assert_eq!(tabelle(&s, &th), vor_tab);
    // Kies anlegen und Putz darauf verweisen lassen
    let mut p = oeffnen(&mut s, &th);
    let kies = schraffur_neu(&mut p, &mut s);
    let mut k = s.model().attr().fill(kies).unwrap().clone();
    k.name = "Kies".into();
    k.kind = FillKind::Lines(vec![
        HatchLine::solid(0.0, 2.0, 0.0),
        HatchLine::solid(90.0, 2.0, 0.0),
    ]);
    assert!(eingabe(&mut p, &mut s, move |m| m.set_fill(kies, k)));
    let (putz, alt) = baustoff(&s, "Putz");
    assert!(eingabe(&mut p, &mut s, move |m| baustoff_darstellung(
        m,
        putz,
        |d| d.cut_fill = kies
    )));
    ok(&mut p, &mut s, &th, &mut ohne_datei());
    assert_eq!(baustoff(&s, "Putz").1.cut_fill, kies);
    assert!(
        !s.edit_model("Löschen", |m| schraffur_loeschen(m, kies)),
        "Putz nutzt Kies"
    );
    let leer = alt.cut_fill;
    assert!(
        s.edit_model("Putz", |m| baustoff_darstellung(m, putz, |d| d.cut_fill =
            leer))
    );
    assert!(s.edit_model("Löschen", |m| schraffur_loeschen(m, kies)));
    assert!(s.model().attr().fill(kies).is_none());
    assert!(s.model().check().is_empty(), "{:?}", s.model().check());
    // .szo nach dem Löschen bytegleich im Rundlauf
    let d = test_dir("schraffuren");
    let pfad = d.join("Haus.szo");
    crate::document::save(s.model(), &pfad).unwrap();
    let text = std::fs::read_to_string(&pfad).unwrap();
    let t = crate::document::load(&pfad).unwrap();
    assert!(t.hints.is_empty(), "{:?}", t.hints);
    let pfad2 = d.join("Haus2.szo");
    crate::document::save(&t.model, &pfad2).unwrap();
    assert_eq!(std::fs::read_to_string(&pfad2).unwrap(), text);
    let _ = std::fs::remove_dir_all(&d);
}

/// A78 (E6 B3, B4, Teil C, Tests 5, 6; BIM 1, 2): Oberflächenfarbe wirkt
/// sofort, Abbrechen bitgenau. Baustoffverweise änderbar (Stahlbeton Stift
/// Schraffur 4 → 3), BIM-Daten bleiben unberührt; tote Kennungen und
/// verwendete Oberflächen werden abgelehnt.
#[test]
fn a78_oberflaechen_und_baustoffverweise() {
    let mut s = haus_mit_wand();
    let mut th = Theme::dark();
    let vor = szo(&s);
    let vor_tab = tabelle(&s, &th);
    let (gb, gasbeton) = baustoff(&s, "Gasbeton");
    let mut o = s.model().attr().surface(gasbeton.surface).unwrap().clone();
    let mut p = oeffnen(&mut s, &th);
    o.color = [200, 120, 90];
    let sid = gasbeton.surface;
    assert!(eingabe(&mut p, &mut s, move |m| m.set_surface(sid, o)));
    assert_ne!(tabelle(&s, &th), vor_tab, "3D sofort");
    abbrechen(&mut p, &mut s, &mut th);
    assert_eq!(szo(&s), vor);
    assert!(
        !s.edit_model("Löschen", |m| oberflaeche_loeschen(m, sid)),
        "Gasbeton nutzt sie"
    );
    // Stahlbeton: Stift Schraffur 4 → 3, BIM-Daten bleiben
    let (sb, alt) = baustoff(&s, "Stahlbeton");
    let (p3, _) = stift(&s, 3);
    assert_eq!(alt.cut_fg, stift(&s, 4).0);
    assert!(
        s.edit_model("Baustoff", |m| baustoff_darstellung(m, sb, |d| d.cut_fg =
            p3))
    );
    assert_eq!(s.undo_label(), Some("Baustoff"), "ein Schritt");
    let neu = baustoff(&s, "Stahlbeton").1;
    assert_eq!(neu.cut_fg, p3);
    assert_eq!(
        (
            &neu.name,
            neu.category,
            neu.priority,
            neu.density,
            neu.lambda
        ),
        (
            &alt.name,
            alt.category,
            alt.priority,
            alt.density,
            alt.lambda
        ),
        "BIM-Daten unberührt"
    );
    assert_ne!(tabelle(&s, &th), vor_tab, "Decke und Platte mit Stift 3");
    // Tote Kennung: neuen Stift anlegen und löschen, dann darauf verweisen
    let mut q = oeffnen(&mut s, &th);
    let frei = stift_neu(&mut q, &mut s);
    ok(&mut q, &mut s, &th, &mut ohne_datei());
    let mut q = oeffnen(&mut s, &th);
    assert!(stift_loeschen(&mut q, &mut s, frei));
    ok(&mut q, &mut s, &th, &mut ohne_datei());
    let stand = szo(&s);
    assert!(!s.edit_model("Tot", |m| baustoff_darstellung(m, gb, |d| d.cut_bg = frei)));
    assert_eq!(szo(&s), stand, "nichts geändert");
    assert!(s.model().check().is_empty(), "{:?}", s.model().check());
}

/// A79 (E6 B5, Test 7): Mehrere Änderungen in verschiedenen Reitern, OK →
/// ein Rückgängig-Schritt; „Auf Standard zurücksetzen“ je Reiter; Namen je
/// Tabelle eindeutig.
#[test]
fn a79_ok_zuruecksetzen_namen() {
    let mut s = haus_mit_wand();
    let mut th = Theme::dark();
    let vor = szo(&s);
    let mut p = oeffnen(&mut s, &th);
    let (strich, mut l) = linientyp(&s, "Strichlinie");
    l.pattern[0].len_mm = 5.0;
    eingabe(&mut p, &mut s, move |m| linientyp_aendern(m, strich, l));
    let (mw, mut f) = schraffur(&s, "Mauerwerk");
    if let FillKind::Lines(l) = &mut f.kind {
        l[0].angle_deg = 45.0;
    }
    eingabe(&mut p, &mut s, move |m| m.set_fill(mw, f));
    let (_, gb) = baustoff(&s, "Gasbeton");
    let sid = gb.surface;
    let mut o = s.model().attr().surface(sid).unwrap().clone();
    o.color = [200, 120, 90];
    eingabe(&mut p, &mut s, move |m| m.set_surface(sid, o));
    // Zurücksetzen nur im Reiter Schraffuren: Mauerwerk wieder 135°, Rest bleibt
    zuruecksetzen(&mut p, &mut s, &mut th, "Schraffuren");
    let FillKind::Lines(l) = schraffur(&s, "Mauerwerk").1.kind else {
        panic!()
    };
    assert_eq!(l[0].angle_deg, 135.0);
    assert_eq!(linientyp(&s, "Strichlinie").1.pattern[0].len_mm, 5.0);
    assert_eq!(s.model().attr().surface(sid).unwrap().color, [200, 120, 90]);
    zuruecksetzen(&mut p, &mut s, &mut th, "Linientypen");
    assert_eq!(linientyp(&s, "Strichlinie").1.pattern[0].len_mm, 3.0);
    zuruecksetzen(&mut p, &mut s, &mut th, "Oberflächen");
    assert_eq!(szo(&s), vor, "alle drei Reiter zurück = Ausgangslage");
    let (mw, mut f) = schraffur(&s, "Mauerwerk");
    if let FillKind::Lines(l) = &mut f.kind {
        l[0].spacing_mm = 2.0;
    }
    eingabe(&mut p, &mut s, move |m| m.set_fill(mw, f));
    let (strich, mut l) = linientyp(&s, "Strichlinie");
    l.pattern[0].gap_mm = 2.0;
    eingabe(&mut p, &mut s, move |m| linientyp_aendern(m, strich, l));
    ok(&mut p, &mut s, &th, &mut ohne_datei());
    assert_eq!(s.undo_label(), Some("Einstellungen geändert"));
    assert!(s.undo());
    assert_eq!(szo(&s), vor, "ein Schritt für beide Reiter");
    // Namen eindeutig je Tabelle
    assert!(!name_frei(&s, "Schraffuren", "Mauerwerk"));
    assert!(name_frei(&s, "Schraffuren", "Kies"));
    assert!(!name_frei(&s, "Linientypen", "Strichlinie"));
    assert!(name_frei(&s, "Linientypen", "Mauerwerk"), "andere Tabelle");
    assert!(!name_frei(&s, "Stifte", "Kräftig"));
}
// Abnahmetests E18 „Geschossbogen im Grundriss“, Fassung 3 (einstellungen/paket-e18-geschossrad.md,
// Jörns Handskizze 12:05), vorbereitet gegen main c86de72. Spezifikation: test/abnahme-geschossrad.md.
// Ersetzt a80-a86-geschossrad.rs (Fassung 1, Halbkreisrad).
//
// Angenommene Namen stehen nur in den Adaptern. Die Zeit wird als Millisekunden
// hineingereicht, damit die Animation ohne Warten prüfbar ist. Aussehen
// (Leuchten, Überblendung, Höhenversatz) und echte Bildzeiten (--zeiten)
// prüfen Handtest H55–H61 und Review.
//
// Das Fundament als Ebene (A87, Teile von A80–A82) hat Jörn noch nicht
// bestätigt. Lehnt er ab, entfallen A87 und die Fundament-Zeilen in A80–A82
// (dort mit „Fundament:“ markiert); die Spitze unten im EG ist dann gesperrt.

// ===== Adapter E18 =====

use crate::wheel::Wheel;

/// Ist der Bogen in dieser Ansicht zu sehen? `gesperrt`: Dialog, Dateimenü
/// oder Einstellungsfenster offen.
fn bogen_sichtbar(w: &Wheel, view: ViewKind, gesperrt: bool) -> bool {
    w.visible(view, gesperrt)
}

/// Kreismittelpunkt des Bogens (x, y) in Bildpunkten und Radius bis zur
/// Bandmitte.
fn bogen_lage(w: &Wheel, ui: &Ui, breite: u32, hoehe: u32) -> (f32, f32, f32) {
    w.placement(ui, breite, hoehe)
}

/// Rechte Kante des ganzen Bogens samt Schild des aktiven Geschosses (px).
fn bogen_rechts(w: &Wheel, s: &Scene, ui: &Ui, breite: u32, hoehe: u32) -> f32 {
    w.right_edge(s, ui, breite, hoehe)
}

/// Spitze frei? `hoch` = Spitze oben. `eingabe`: Wandzug angefangen.
fn spitze_frei(w: &Wheel, s: &Scene, hoch: bool, eingabe: bool) -> bool {
    w.arrow_enabled(s, hoch, eingabe)
}

/// Klick auf eine Spitze (oder ihre Beschriftung) zur Zeit `t` (ms).
fn spitze_klick(w: &mut Wheel, s: &mut Scene, hoch: bool, eingabe: bool, t: u64) {
    w.click_arrow(s, hoch, eingabe, t)
}

/// Klick auf Band oder Schild des aktiven Geschosses.
fn band_klick(w: &mut Wheel, s: &mut Scene, t: u64) {
    w.click_band(s, t)
}

/// Mausrad über dem Bogen: `rasten` > 0 = hoch.
fn mausrad(w: &mut Wheel, s: &mut Scene, rasten: i32, t: u64) {
    w.scroll(s, rasten, t)
}

/// Taste in der Ansicht `view` (Bild↑/Bild↓); `true` = vom Bogen verbraucht.
fn taste(w: &mut Wheel, s: &mut Scene, view: ViewKind, key: Key, eingabe: bool, t: u64) -> bool {
    w.key(s, view, key, eingabe, t)
}

/// Klick auf einen Geschossnamen im Paneel „Geschosse“: derselbe Wechsel mit
/// Animation wie über den Bogen.
fn paneel_klick(w: &mut Wheel, s: &mut Scene, st: sk_model::StoreyId, t: u64) {
    w.select(s, st, t)
}

/// Um wie viele Plätze die Beschriftungen in der laufenden Animation rollen
/// (+ = nach oben); 0 ohne Animation.
fn rollt(w: &Wheel, t: u64) -> i32 {
    w.anim_steps(t)
}

/// Fortschritt der Zeit: führt vorgemerkte Eingaben nach dem Ende aus.
fn tick(w: &mut Wheel, s: &mut Scene, t: u64) {
    w.tick(s, t)
}

fn laeuft(w: &Wheel, t: u64) -> bool {
    w.animating(t)
}

/// Beschriftung: aktives Geschoss (Name, Kote), Nachbar an der oberen und an
/// der unteren Spitze (Name; `None` = Spitze ausgegraut ohne Beschriftung).
type Text = ((String, String), Option<String>, Option<String>);
fn bogen_text(w: &Wheel, s: &Scene) -> Text {
    (w.center(s), w.neighbor(s, true), w.neighbor(s, false))
}

/// Hinweis an der Spitze nach der Hover-Verzögerung: (Zeile, Kote) bzw. der
/// Sperrhinweis; `None` über einer ausgegrauten Spitze.
fn spitze_hinweis(w: &Wheel, s: &Scene, hoch: bool, eingabe: bool) -> Option<(String, String)> {
    w.arrow_hint(s, hoch, eingabe)
}

/// Dauer der Animation in ms aus Schema und Startart.
fn anim_dauer(th: &Theme, screenshot: bool) -> u64 {
    crate::wheel::anim_duration(th, screenshot)
}

/// Grundrissnetz eines Geschosses liegt schon bereit (vorbereitet im Leerlauf).
fn grundriss_bereit(s: &Scene, st: sk_model::StoreyId) -> bool {
    s.plan_ready(st)
}

/// Leerlauf: Nachbargeschosse vorbereiten.
fn leerlauf(s: &mut Scene) {
    s.prepare_neighbor_plans()
}

/// Sperrhinweis des Wandwerkzeugs im aktiven Geschoss (`None` = frei).
fn wand_sperre(s: &Scene) -> Option<String> {
    s.wall_tool_block()
}

/// Das Fundament (Gründungsband des Geschossmanagers).
fn fundament(s: &Scene) -> sk_model::StoreyId {
    s.levels().bands.iter().find(|b| b.foundation).unwrap().id
}

fn bogen(th: &Theme) -> Wheel {
    Wheel::new(th, false)
}

const BILD_HOCH: Key = Key::Other(0x21);
const BILD_RUNTER: Key = Key::Other(0x22);

fn t(a: &str, b: &str, oben: Option<&str>, unten: Option<&str>) -> Text {
    (
        (a.into(), b.into()),
        oben.map(Into::into),
        unten.map(Into::into),
    )
}

// ===== Tests =====

/// A80 (E18 §1, §2, Test 1): Bogen nur im Grundriss, ausgeblendet bei Dialog,
/// Menü oder Einstellungsfenster; rechts am Rand mit Abstand `panel_margin`,
/// senkrecht mittig im freien Bereich unter „Ansichten“, nie höher als die
/// Fenstermitte. Rechts groß „EG ±0,00“, oben klein „OG“, unten „Fundament“.
#[test]
fn a80_bogen_nur_im_grundriss() {
    let th = Theme::dark();
    let w = bogen(&th);
    for v in [
        ViewKind::Persp,
        ViewKind::Section,
        ViewKind::Front,
        ViewKind::Back,
        ViewKind::Left,
        ViewKind::Right,
    ] {
        assert!(!bogen_sichtbar(&w, v, false), "{v:?}");
    }
    assert!(bogen_sichtbar(&w, ViewKind::Plan, false));
    assert!(!bogen_sichtbar(&w, ViewKind::Plan, true), "Dialog offen");
    // Gebäude mit Fundament, EG und OG
    let mut s = Scene::with_model(Model::with_seed(81));
    gebaeude(&mut s);
    assert_eq!(
        bogen_text(&w, &s),
        t("EG", "±0,00", Some("OG"), Some("Fundament"))
    );
    assert!(spitze_frei(&w, &s, true, false));
    assert!(spitze_frei(&w, &s, false, false), "Fundament: unten frei");
    // Lage
    let mut ui = Ui::new(1.0, &th);
    for (bw, bh) in [(1440u32, 900u32), (1920, 1080), (1000, 640)] {
        ui.fit(1.0, bw, bh);
        let (cx, cy, r) = bogen_lage(&w, &ui, bw, bh);
        let views = ui.rect(Panel::Views, bw, 32);
        let unten = views.y + views.h;
        let soll = ((unten + bh as f32) / 2.0).max(bh as f32 / 2.0);
        assert_eq!(r, th.size.arc_r);
        assert!(
            (cy - soll).abs() < 1.0,
            "senkrecht {bw}×{bh}: {cy} ≠ {soll}"
        );
        let rechts = bogen_rechts(&w, &s, &ui, bw, bh);
        assert!(
            (rechts - (bw as f32 - th.size.panel_margin)).abs() < 1.0,
            "rechter Rand {bw}: {rechts}"
        );
        assert!(cx + r < rechts, "Schild rechts vom Bogen");
    }
    // Noch kein Gebäude (nur die Vorlage): Der Bogen zeigt dieselben
    // Geschosse wie das Paneel, also oben das OG der Vorlage (Koordinator
    // 07.10. 03:04, vorher ausgegraut; siehe A137).
    // Bauthread: Ein geschlossenes Rechteck legt schon das ganze Gebäude mit
    // OG an (Modellierungsansatz), daher hier ohne Zeichnen.
    let s = Scene::with_model(Model::with_seed(80));
    assert_eq!(
        bogen_text(&w, &s),
        t("EG", "±0,00", Some("OG"), Some("Fundament"))
    );
    assert!(spitze_frei(&w, &s, true, false));
    assert_eq!(
        spitze_hinweis(&w, &s, true, false),
        Some(("Obergeschoss ↑".into(), "+2,855".into())),
        "wie mit Gebäude"
    );
}

/// A81 (E18 §3, §5, Tests 2, 3, 5): Spitzen wechseln, an den Enden gesperrt;
/// Hinweise; Bogen und Paneel zeigen dasselbe Geschoss; Band und Schild
/// reagieren nicht.
#[test]
fn a81_spitzen_und_kopplung_mit_paneel() {
    let th = Theme::dark();
    let mut w = bogen(&th);
    let mut s = Scene::with_model(Model::with_seed(82));
    gebaeude(&mut s);
    let (fu, eg, og) = (fundament(&s), geschoss(&s, "EG"), geschoss(&s, "OG"));
    assert_eq!(
        spitze_hinweis(&w, &s, true, false),
        Some(("Obergeschoss ↑".into(), "+2,855".into()))
    );
    assert_eq!(
        spitze_hinweis(&w, &s, false, false),
        Some(("Fundament ↓".into(), "−0,80".into())),
        "Fundament"
    );
    band_klick(&mut w, &mut s, 0);
    assert_eq!(s.active_storey(), eg, "Band: nichts");
    assert!(!laeuft(&w, 0));
    spitze_klick(&mut w, &mut s, true, false, 0);
    assert_eq!(s.active_storey(), og, "Paneel und Grundriss sofort auf OG");
    assert_eq!(s.plan_cut(), 2855.0 + 1000.0);
    tick(&mut w, &mut s, 1000);
    assert_eq!(bogen_text(&w, &s), t("OG", "+2,855", None, Some("EG")));
    assert!(!spitze_frei(&w, &s, true, false), "oben zu");
    assert_eq!(spitze_hinweis(&w, &s, true, false), None);
    assert_eq!(
        spitze_hinweis(&w, &s, false, false),
        Some(("Erdgeschoss ↓".into(), "±0,00".into()))
    );
    spitze_klick(&mut w, &mut s, true, false, 1000);
    tick(&mut w, &mut s, 2000);
    assert_eq!(s.active_storey(), og, "Spitze oben gesperrt: nichts");
    // Paneel → Bogen: set_active_storey (Klick im Paneel) zeigt der Bogen mit
    assert!(s.set_active_storey(eg));
    assert_eq!(bogen_text(&w, &s).0 .0, "EG");
    // Fundament: Spitze unten, dann unten ausgegraut
    spitze_klick(&mut w, &mut s, false, false, 3000);
    tick(&mut w, &mut s, 4000);
    assert_eq!(s.active_storey(), fu);
    assert_eq!(
        bogen_text(&w, &s),
        t("Fundament", "−0,80", Some("EG"), None)
    );
    assert!(!spitze_frei(&w, &s, false, false), "unten zu");
    let l = s.levels();
    let aktiv: Vec<_> = l
        .bands
        .iter()
        .filter(|b| b.active)
        .map(|b| b.name.as_str())
        .collect();
    assert_eq!(
        aktiv,
        ["Fundament"],
        "Paneel heißt „Fundament“ und zeigt es aktiv"
    );
}

/// A82 (E18 §3, Tests 2, 4): Bild↑/Bild↓ nur im Grundriss und nicht während
/// einer Eingabe; Mausrad über dem Bogen: eine Raste = ein Geschoss.
#[test]
fn a82_bildtasten_und_mausrad() {
    let th = Theme::dark();
    let mut w = bogen(&th);
    let mut s = Scene::with_model(Model::with_seed(83));
    gebaeude(&mut s);
    let (fu, eg, og) = (fundament(&s), geschoss(&s, "EG"), geschoss(&s, "OG"));
    assert!(
        !taste(&mut w, &mut s, ViewKind::Persp, BILD_HOCH, false, 0),
        "nicht in 3D"
    );
    assert_eq!(s.active_storey(), eg);
    assert!(
        !taste(&mut w, &mut s, ViewKind::Plan, BILD_HOCH, true, 0),
        "nicht während der Eingabe"
    );
    assert_eq!(s.active_storey(), eg);
    assert!(taste(&mut w, &mut s, ViewKind::Plan, BILD_HOCH, false, 0));
    assert_eq!(s.active_storey(), og);
    tick(&mut w, &mut s, 1000);
    assert!(taste(
        &mut w,
        &mut s,
        ViewKind::Plan,
        BILD_RUNTER,
        false,
        1000
    ));
    assert_eq!(s.active_storey(), eg);
    tick(&mut w, &mut s, 2000);
    mausrad(&mut w, &mut s, 1, 2000);
    assert_eq!(s.active_storey(), og, "Rad hoch = Geschoss hoch");
    tick(&mut w, &mut s, 3000);
    mausrad(&mut w, &mut s, 1, 3000);
    tick(&mut w, &mut s, 4000);
    assert_eq!(s.active_storey(), og, "oben bleibt oben");
    mausrad(&mut w, &mut s, -1, 4000);
    assert_eq!(s.active_storey(), eg);
    tick(&mut w, &mut s, 5000);
    assert!(taste(
        &mut w,
        &mut s,
        ViewKind::Plan,
        BILD_RUNTER,
        false,
        5000
    ));
    assert_eq!(s.active_storey(), fu, "Fundament: Bild↓ im EG");
    tick(&mut w, &mut s, 6000);
    mausrad(&mut w, &mut s, -1, 6000);
    tick(&mut w, &mut s, 7000);
    assert_eq!(s.active_storey(), fu, "unten bleibt unten");
}

/// A83 (E18 §5, Test 9): Während eines angefangenen Wandzugs ist der Wechsel
/// gesperrt: Spitzen zu mit Hinweis, Klick, Mausrad und Bildtasten wirkungslos.
#[test]
fn a83_gesperrt_beim_wand_zeichnen() {
    let th = Theme::dark();
    let mut w = bogen(&th);
    let mut s = Scene::with_model(Model::with_seed(84));
    gebaeude(&mut s);
    let eg = geschoss(&s, "EG");
    for hoch in [true, false] {
        assert!(!spitze_frei(&w, &s, hoch, true));
        assert_eq!(
            spitze_hinweis(&w, &s, hoch, true).map(|h| h.0),
            Some("Erst die Wand fertig zeichnen oder Esc".into())
        );
    }
    spitze_klick(&mut w, &mut s, true, true, 0);
    spitze_klick(&mut w, &mut s, false, true, 0);
    assert!(!taste(&mut w, &mut s, ViewKind::Plan, BILD_HOCH, true, 0));
    tick(&mut w, &mut s, 1000);
    assert_eq!(s.active_storey(), eg);
    assert!(!laeuft(&w, 1000));
}

/// A84 (E18 §4, §5, Tests 6, 7, 8): Animation 280 ms (17 ± 2 Bilder bei
/// 60 Hz); eine Eingabe während der Animation wird vorgemerkt (höchstens
/// eine) und danach ausgeführt; Paneelklick über zwei Stufen ist eine
/// Animation; anim_ms = 0 und --screenshot ohne Animation.
#[test]
fn a84_animation_und_vormerken() {
    let th = Theme::dark();
    assert_eq!(th.size.anim_ms, 280.0);
    assert_eq!(anim_dauer(&th, false), 280);
    assert_eq!(anim_dauer(&th, true), 0, "--screenshot");
    let mut aus = Theme::dark();
    aus.size.anim_ms = 0.0;
    assert_eq!(anim_dauer(&aus, false), 0);
    // Bilder zählen
    let mut w = bogen(&th);
    let mut s = Scene::with_model(Model::with_seed(85));
    gebaeude(&mut s);
    let (fu, eg, og) = (fundament(&s), geschoss(&s, "EG"), geschoss(&s, "OG"));
    spitze_klick(&mut w, &mut s, true, false, 0);
    assert_eq!(rollt(&w, 0), 1);
    let mut bilder = 0;
    let mut t = 0.0f64;
    while laeuft(&w, t as u64) {
        bilder += 1;
        t += 1000.0 / 60.0;
        assert!(bilder < 100);
    }
    assert!((15..=19).contains(&bilder), "{bilder} Bilder");
    // Vormerken: im OG nach unten anstoßen, dann zwei schnelle Rasten nach
    // oben während der Animation → genau ein weiterer Wechsel
    tick(&mut w, &mut s, 1000);
    mausrad(&mut w, &mut s, -1, 1000);
    assert_eq!(s.active_storey(), eg);
    mausrad(&mut w, &mut s, 1, 1050);
    mausrad(&mut w, &mut s, 1, 1100);
    assert_eq!(s.active_storey(), eg, "noch in der Animation");
    tick(&mut w, &mut s, 1300);
    assert_eq!(s.active_storey(), og, "vorgemerkter Wechsel ausgeführt");
    assert!(laeuft(&w, 1400), "Animation des vorgemerkten Wechsels");
    tick(&mut w, &mut s, 1700);
    assert!(
        !laeuft(&w, 1700),
        "keine zweite vorgemerkte Eingabe gestaut"
    );
    assert_eq!(s.active_storey(), og);
    // Fundament: Paneelklick Fundament → OG rollt in einem Zug zwei Plätze
    paneel_klick(&mut w, &mut s, fu, 2000);
    tick(&mut w, &mut s, 3000);
    assert_eq!(s.active_storey(), fu);
    paneel_klick(&mut w, &mut s, og, 3000);
    assert_eq!(s.active_storey(), og);
    assert_eq!(rollt(&w, 3000), 2, "zwei Plätze");
    assert!(laeuft(&w, 3200));
    assert!(!laeuft(&w, 3300), "eine Animation von 280 ms, nicht zwei");
    // anim_ms = 0: sofort, ohne Animation
    tick(&mut w, &mut s, 4000);
    let mut w0 = bogen(&aus);
    spitze_klick(&mut w0, &mut s, false, false, 5000);
    assert_eq!(s.active_storey(), eg);
    assert!(!laeuft(&w0, 5000));
    let mut ws = Wheel::new(&th, true);
    spitze_klick(&mut ws, &mut s, true, false, 6000);
    assert_eq!(s.active_storey(), og);
    assert!(!laeuft(&ws, 6000), "--screenshot");
}

/// A85 (E18 §5, Test 10): Kein Rückgängig-Eintrag, keine Modell- oder
/// Attributänderung, Titel ohne „•“, auch über das Fundament.
#[test]
fn a85_kein_rueckgaengig() {
    let th = Theme::dark();
    let mut w = bogen(&th);
    let mut s = Scene::with_model(Model::with_seed(86));
    gebaeude(&mut s);
    let doc = crate::document::Document::new(s.model().revision());
    let (label, rev, attr_rev) = (s.undo_label(), s.model().revision(), s.model().attr().rev());
    let text = sk_model::szo::write(s.model());
    spitze_klick(&mut w, &mut s, true, false, 0);
    tick(&mut w, &mut s, 1000);
    mausrad(&mut w, &mut s, -1, 1000);
    tick(&mut w, &mut s, 2000);
    mausrad(&mut w, &mut s, -1, 2000);
    tick(&mut w, &mut s, 3000);
    assert_eq!(s.undo_label(), label);
    assert_eq!(s.model().revision(), rev);
    assert_eq!(s.model().attr().rev(), attr_rev);
    assert_eq!(sk_model::szo::write(s.model()), text);
    assert!(!doc.is_dirty(s.model()));
}

/// A86 (E18 §4 Leistung, §6, Test 11): Der Grundriss der Nachbargeschosse
/// liegt nach dem Leerlauf bereit, damit die Animation nichts neu aufbaut;
/// nach einer Modelländerung wird neu vorbereitet. Neue Größen im Schema,
/// `anim_ms` in einstellungen.txt, Prüfregel 1.
#[test]
fn a86_vorbereitung_und_schema() {
    let th = Theme::dark();
    let mut s = Scene::with_model(Model::with_seed(87));
    gebaeude(&mut s);
    let (fu, eg, og) = (fundament(&s), geschoss(&s, "EG"), geschoss(&s, "OG"));
    leerlauf(&mut s);
    assert!(grundriss_bereit(&s, og), "Nachbar OG vorbereitet");
    assert!(grundriss_bereit(&s, fu), "Fundament: Nachbar vorbereitet");
    assert!(grundriss_bereit(&s, eg));
    ziehen_am_fuss(&mut s, 0.0, 500.0);
    assert!(!grundriss_bereit(&s, og), "nach der Änderung veraltet");
    leerlauf(&mut s);
    assert!(grundriss_bereit(&s, og));
    let mut w = bogen(&th);
    spitze_klick(&mut w, &mut s, true, false, 0);
    assert!(
        grundriss_bereit(&s, og),
        "die Animation baut nichts neu auf"
    );
    // Schema
    let z = &th.size;
    assert_eq!(
        (
            z.arc_r,
            z.arc_span_deg,
            z.arc_band,
            z.arc_head_l,
            z.arc_head_w
        ),
        (80.0, 58.0, 14.0, 24.0, 34.0)
    );
    assert_eq!((z.arc_label, z.arc_label_small), (26.0, 13.0));
    assert_eq!(z.hover_delay_hud, 0.25);
    assert_eq!(th.ui.hud_bg.3, 199);
    assert_eq!(
        (th.ui.hud_bg.0, th.ui.hud_bg.1, th.ui.hud_bg.2),
        (th.ui.bg.0, th.ui.bg.1, th.ui.bg.2)
    );
    assert_eq!(th.ui.hud_glow, th.ui.accent);
    let geaendert = {
        let mut t = Theme::dark();
        t.size.anim_ms = 0.0;
        crate::settings::write(&t)
    };
    let (gelesen, hints) = crate::settings::read(&geaendert);
    assert!(hints.is_empty(), "{hints:?}");
    assert_eq!(gelesen.size.anim_ms, 0.0, "anim_ms in einstellungen.txt");
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let text = std::fs::read_to_string(root.join("app/src/wheel.rs")).unwrap();
    let code = text.split("#[cfg(test)]").next().unwrap();
    for (n, z) in code.lines().enumerate() {
        let z = z.split("//").next().unwrap();
        for muster in ["Rgba::rgb(", "from_rgb8([", "Rgba::from_f32(["] {
            if let Some(i) = z.find(muster) {
                let rest = z[i + muster.len()..].trim_start();
                assert!(
                    !rest.starts_with(|c: char| c.is_ascii_digit()),
                    "wheel.rs:{}: {z}",
                    n + 1
                );
            }
        }
    }
}

/// A87 (E18 §3 „Fundament als Ebene“, Test 3; Lastenheft A-09, Prüfregel 14):
/// Im Fundament schneidet der Grundriss in der Mitte der Frostschürze
/// (Standard −0,51) über den Höhenbezug: Plattendicke 30 cm → −0,55, UK
/// Fundament −0,85 → −0,535. Zu sehen: Frostschürze mit Stahlbeton-Schraffur,
/// Plattenrand als Hintergrundlinie (Stift 9), keine Wände, kein
/// EG-Hintergrund. Das Wandwerkzeug ist gesperrt.
#[test]
fn a87_fundament_als_ebene() {
    let mut s = Scene::with_model(Model::with_seed(88));
    let (eg_zug, _) = gebaeude(&mut s);
    let fu = fundament(&s);
    assert_eq!(wand_sperre(&s), None, "EG: frei");
    assert!(s.set_active_storey(fu), "Fundament aktivierbar");
    assert_eq!(s.plan_cut(), -510.0, "Mitte Frostschürze −0,80 … −0,22");
    assert_eq!(
        wand_sperre(&s).as_deref(),
        Some("Im Fundament gibt es noch nichts zu zeichnen")
    );
    let plan = view_mesh(&mut s, ViewKind::Plan, None);
    let has = |pat: f32| plan.faces.iter().any(|v| v[9] == pat);
    assert!(
        has(pattern::CONCRETE),
        "Frostschürze geschnitten, Stahlbeton"
    );
    assert!(
        !has(pattern::DIAGONAL) && !has(pattern::ZIGZAG),
        "keine Wände (Gasbeton, Dämmung)"
    );
    assert!(
        plan.faces.iter().all(|v| v[2] <= -510.0 + 1e-2),
        "nichts über dem Schnitt geschnitten"
    );
    let m = s.mesh(ViewKind::Plan, None, &[]);
    let bg = edge_kind::BACKGROUND as f32;
    let auf_x = |x: f32| {
        m.edges.iter().any(|e| {
            e.1 == bg
                && (e.0[0][0] - x).abs() < 0.5
                && (e.0[1][0] - x).abs() < 0.5
                && (e.0[0][1] - e.0[1][1]).abs() > 1000.0
        })
    };
    assert!(auf_x(0.0), "Plattenrand als Hintergrundlinie");
    assert!(
        !auf_x(140.0) && !auf_x(315.0),
        "kein EG-Hintergrund (Fuge, Innenkante Gasbeton)"
    );
    // Höhenbezug: Plattendicke 30 cm, UK Schürze bleibt −0,80
    let (slab, _) = sohlplatte(&s, eg_zug);
    assert!(s.edit_model("Dicke", |m| m.set_slab_thickness(slab, 300.0)));
    assert_eq!(s.plan_cut(), -550.0, "Plattendicke 30 cm");
    s.undo();
    assert_eq!(s.plan_cut(), -510.0);
    kante_ziehen(&mut s, "GR.UK", &[-850.0], false);
    assert_eq!(s.plan_cut(), -535.0, "UK Fundament −0,85");
    // Zurück ins EG und wieder ins Fundament (Paneel)
    assert!(s.set_active_storey(geschoss(&s, "EG")));
    assert!(s.set_active_storey(fu));
}

// Abnahmetests F2 „Fensterschicht“: zweites Programmfenster ohne Inhalt
// (bim/paket-b7-mengenliste.md „Das Mengenfenster“, bim/b7-zweites-fenster-technik.md),
// vorbereitet gegen main 89d28ac. Spezifikation: test/abnahme-mengenliste.md.
// Vorgabe Jörn 06.10. 14:00 (zweites Fenster rechts angedockt, gemeinsam
// minimieren, Hover und Auswahl in beiden Richtungen).
//
// Geprüft wird die Logik hinter den Win32-Aufrufen: welche Fenster angelegt,
// verschoben, minimiert, geholt oder geschlossen werden, als Liste von
// „Taten“. Die Win32-Seite selbst (WM_SIZE, SetWindowPos, Taskleiste,
// Win+D, Aero Snap, zweiter Monitor, DWM-Einblenden) prüft nur der Handtest
// H63–H72 unter echtem Windows.
//
// Angenommene Namen stehen nur in den Adaptern: `crate::windows::{Windows,
// WindowId, Action, Show}`, Rechtecke in Bildpunkten `(x, y, w, h)`,
// `crate::picking::Picking { selected, hover }`, `Theme.interact.hover_element`,
// `crate::selection::hover_helpers`.
mod f2 {
    use super::*;

    // ===== Adapter F2 =====

    use crate::windows::{Action, Show, WindowId, Windows};

    type R = (i32, i32, i32, i32);

    #[derive(Clone, Copy, Debug, PartialEq)]
    enum Fenster {
        Haupt,
        Mengen,
    }

    /// Was die Fensterlogik vom UI-Thread verlangt.
    #[derive(Clone, Debug, PartialEq)]
    enum Tat {
        /// Fenster anlegen (sichtbar) an dieser Stelle.
        Lege(Fenster, R),
        /// Lage und Größe setzen (SetWindowPos).
        Setze(Fenster, R),
        Minimiere(Fenster),
        /// Wiederherstellen ohne Aktivieren (SW_SHOWNOACTIVATE).
        Hole(Fenster),
        Maximiere(Fenster),
        Vorne(Fenster),
        Schliesse(Fenster),
        /// „Änderungen speichern?“ wie bisher.
        Frage,
    }

    fn fenster(id: WindowId) -> Fenster {
        match id {
            WindowId::Main => Fenster::Haupt,
            WindowId::Quantity => Fenster::Mengen,
        }
    }

    fn id(f: Fenster) -> WindowId {
        match f {
            Fenster::Haupt => WindowId::Main,
            Fenster::Mengen => WindowId::Quantity,
        }
    }

    fn taten(v: Vec<Action>) -> Vec<Tat> {
        v.into_iter()
            .map(|a| match a {
                Action::Create(w, r) => Tat::Lege(fenster(w), r),
                Action::Place(w, r) => Tat::Setze(fenster(w), r),
                Action::Show(w, Show::Minimize) => Tat::Minimiere(fenster(w)),
                Action::Show(w, Show::RestoreNoActivate) => Tat::Hole(fenster(w)),
                Action::Show(w, Show::Maximize) => Tat::Maximiere(fenster(w)),
                Action::Front(w) => Tat::Vorne(fenster(w)),
                Action::Close(w) => Tat::Schliesse(fenster(w)),
                Action::AskSave => Tat::Frage,
            })
            .collect()
    }

    /// Neue Fensterlogik mit Breite des Mengenfensters in dip (einstellungen.txt).
    fn logik(breite_dip: f32) -> Windows {
        Windows::new(breite_dip)
    }

    /// Knopf „Mengenermittlung“. `haupt`: Lage des Hauptfensters; `maximiert`
    /// mit Arbeitsbereich des Bildschirms; `scale` 1,0 = 96 dpi.
    fn oeffne(w: &mut Windows, haupt: R, maximiert: bool, arbeit: R, scale: f32) -> Vec<Tat> {
        taten(w.open_quantity(haupt, maximiert, arbeit, scale))
    }

    /// Hauptfenster wurde verschoben oder in der Größe geändert.
    fn haupt_bewegt(w: &mut Windows, r: R) -> Vec<Tat> {
        taten(w.main_moved(r))
    }

    /// Mengenfenster an der Titelleiste gezogen (Lage nach dem Ziehen).
    fn mengen_gezogen(w: &mut Windows, r: R) -> Vec<Tat> {
        taten(w.quantity_moved(r))
    }

    /// Mengenfenster am Rand in der Größe geändert.
    fn mengen_groesse(w: &mut Windows, r: R) -> Vec<Tat> {
        taten(w.quantity_resized(r))
    }

    /// Windows meldet: Fenster wurde minimiert / wiederhergestellt / maximiert
    /// (vom Nutzer, der Taskleiste, Win+D, oder als Folge einer eigenen Tat).
    fn minimiert(w: &mut Windows, f: Fenster) -> Vec<Tat> {
        taten(w.minimized(id(f)))
    }
    fn wiederhergestellt(w: &mut Windows, f: Fenster) -> Vec<Tat> {
        taten(w.restored(id(f)))
    }
    fn maximiert(w: &mut Windows, f: Fenster) -> Vec<Tat> {
        taten(w.maximized(id(f)))
    }

    /// Schließen angefordert; `geaendert`: ungespeicherte Änderungen.
    fn schliessen(w: &mut Windows, f: Fenster, geaendert: bool) -> Vec<Tat> {
        taten(w.close_requested(id(f), geaendert))
    }

    fn offen(w: &Windows) -> bool {
        w.quantity_open()
    }

    fn angedockt(w: &Windows) -> bool {
        w.docked()
    }

    fn lage(w: &Windows) -> R {
        w.quantity_rect()
    }

    /// Titel des Mengenfensters.
    fn titel(doc: &crate::document::Document, m: &Model) -> String {
        crate::windows::quantity_caption(doc, m.revision())
    }

    /// Abschnitt in einstellungen.txt und zurück; `monitore`: Arbeitsbereiche
    /// der angeschlossenen Bildschirme.
    fn merken(w: &Windows) -> String {
        crate::windows::write_settings(w)
    }
    fn lesen(text: &str, monitore: &[R]) -> Windows {
        crate::windows::read_settings(text, monitore)
    }

    // Gemeinsamer Hover- und Auswahlzustand (eine Szene, beide Fenster lesen ihn)
    use crate::picking::Picking;

    fn waehle(p: &mut Picking, e: sk_model::ElementId, strg: bool) {
        p.click(e, strg)
    }
    fn esc(p: &mut Picking) {
        p.clear()
    }
    fn pruefe(p: &mut Picking, s: &Scene) {
        p.validate(s);
    }
    /// Farben der Hilfslinien, die das Hover-Bauteil in einer Ansicht hervorheben.
    fn hover_farben(s: &Scene, p: &Picking, v: ViewKind, th: &Theme) -> Vec<[f32; 4]> {
        // Im Schnitt die Schnittlinie wie beim ersten Gebrauch (Mitte des Modells)
        let mut sect = SectionLine::default();
        sect.ensure(s);
        let plane = (v == ViewKind::Section).then(|| sect.plane()).flatten();
        crate::selection::hover_helpers(s, p.hover, v, plane, 1.0, th)
            .iter()
            .map(|h| h.color)
            .collect()
    }

    // ===== Hilfen =====

    const HAUPT: R = (100, 100, 1200, 800);
    const ARBEIT: R = (0, 0, 1920, 1040);

    fn offen_angedockt() -> Windows {
        let mut w = logik(520.0);
        oeffne(&mut w, HAUPT, false, ARBEIT, 1.0);
        w
    }

    // ===== Tests =====

    /// A96 (F2 Öffnen, Titel, Schließen): Der Knopf legt das Mengenfenster rechts
    /// bündig an (gleiche Oberkante und Höhe, 520 dip × Skalierung); ein zweiter
    /// Klick holt es nur nach vorn. Titel „Mengenermittlung – Datei“ mit „•“.
    /// Schließen am Mengenfenster schließt nur dieses; am Hauptfenster beide,
    /// mit der Nachfrage bei ungespeicherten Änderungen.
    #[test]
    fn a96_oeffnen_titel_schliessen() {
        let mut w = logik(520.0);
        assert!(!offen(&w));
        assert_eq!(
            oeffne(&mut w, HAUPT, false, ARBEIT, 1.0),
            [Tat::Lege(Fenster::Mengen, (1300, 100, 520, 800))]
        );
        assert!(offen(&w) && angedockt(&w));
        assert_eq!(
            oeffne(&mut w, HAUPT, false, ARBEIT, 1.0),
            [Tat::Vorne(Fenster::Mengen)],
            "kein zweites Fenster"
        );
        let mut w15 = logik(520.0);
        assert_eq!(
            oeffne(&mut w15, HAUPT, false, ARBEIT, 1.5),
            [
                Tat::Setze(Fenster::Haupt, (100, 100, 1040, 800)),
                Tat::Lege(Fenster::Mengen, (1140, 100, 780, 800))
            ],
            "520 dip bei 150 %: Hauptfenster gibt rechts Breite ab"
        );
        // Titel
        let mut s = Scene::with_model(Model::with_seed(96));
        let doc = crate::document::Document::new(s.model().revision());
        assert_eq!(
            titel(&doc, s.model()),
            format!("Mengenermittlung – {}", doc.caption(s.model()))
        );
        zeichne_rechteck(&mut s, &cam3d());
        assert!(titel(&doc, s.model()).ends_with(" •"), "ungespeichert");
        // Schließen
        assert_eq!(
            schliessen(&mut w, Fenster::Mengen, true),
            [Tat::Schliesse(Fenster::Mengen)],
            "nur das Mengenfenster, keine Nachfrage"
        );
        assert!(!offen(&w));
        let mut w = offen_angedockt();
        assert_eq!(schliessen(&mut w, Fenster::Haupt, true), [Tat::Frage]);
        let t = schliessen(&mut w, Fenster::Haupt, false);
        assert!(t.contains(&Tat::Schliesse(Fenster::Mengen)), "{t:?}");
        assert!(t.contains(&Tat::Schliesse(Fenster::Haupt)), "{t:?}");
    }

    /// A97 (F2 Andocken): Angedockt folgt das Mengenfenster dem Hauptfenster
    /// (Verschieben, Größe); Ziehen bis 24 px bleibt angedockt, weiter löst es;
    /// frei folgt es nicht mehr; bis 12 px heran rastet es ein. Angedockt ändert
    /// nur der rechte Rand die Breite. Bei maximiertem Hauptfenster ⅔ zu ⅓,
    /// nach dem Schließen wieder maximiert.
    #[test]
    fn a97_andocken_loesen_einrasten() {
        let mut w = offen_angedockt();
        assert_eq!(
            haupt_bewegt(&mut w, (300, 50, 1200, 800)),
            [Tat::Setze(Fenster::Mengen, (1500, 50, 520, 800))]
        );
        assert_eq!(
            haupt_bewegt(&mut w, (300, 50, 1000, 700)),
            [Tat::Setze(Fenster::Mengen, (1300, 50, 520, 700))],
            "Größe des Hauptfensters"
        );
        // Breite nur am rechten Rand: Versuch am linken Rand bleibt am Anker
        assert_eq!(
            mengen_groesse(&mut w, (1250, 50, 600, 700)),
            [Tat::Setze(Fenster::Mengen, (1300, 50, 550, 700))]
        );
        assert_eq!(lage(&w), (1300, 50, 550, 700));
        // 20 px weg: bleibt angedockt und springt zurück
        assert_eq!(
            mengen_gezogen(&mut w, (1320, 50, 550, 700)),
            [Tat::Setze(Fenster::Mengen, (1300, 50, 550, 700))]
        );
        assert!(angedockt(&w));
        // 30 px weg: frei, bleibt wo es ist
        assert_eq!(mengen_gezogen(&mut w, (1330, 80, 550, 700)), []);
        assert!(!angedockt(&w));
        assert_eq!(
            haupt_bewegt(&mut w, (0, 0, 1000, 700)),
            [],
            "frei: folgt nicht"
        );
        // Auf den zweiten Bildschirm und zurück
        assert_eq!(mengen_gezogen(&mut w, (2100, 80, 550, 700)), []);
        assert_eq!(
            mengen_gezogen(&mut w, (1020, 90, 550, 700)),
            [],
            "20 px vor dem Rand: noch frei"
        );
        assert_eq!(
            mengen_gezogen(&mut w, (1010, 90, 550, 700)),
            [Tat::Setze(Fenster::Mengen, (1000, 0, 550, 700))],
            "12 px heran: rastet ein, Oberkante und Höhe vom Hauptfenster"
        );
        assert!(angedockt(&w));
        // Maximiertes Hauptfenster: Arbeitsbereich teilen
        let mut w = logik(520.0);
        let t = oeffne(&mut w, ARBEIT, true, ARBEIT, 1.0);
        assert_eq!(
            t,
            [
                Tat::Setze(Fenster::Haupt, (0, 0, 1280, 1040)),
                Tat::Lege(Fenster::Mengen, (1280, 0, 640, 1040)),
            ]
        );
        let t = schliessen(&mut w, Fenster::Mengen, false);
        assert_eq!(
            t,
            [
                Tat::Schliesse(Fenster::Mengen),
                Tat::Maximiere(Fenster::Haupt)
            ],
            "zurück in den vorherigen Zustand"
        );
    }

    /// A98 (F2 Taskleiste): Minimieren und Wiederherstellen immer gemeinsam,
    /// egal an welchem Fenster oder Eintrag; die Rückmeldung der eigenen Tat
    /// stößt nicht zurück (Sperre). Maximieren betrifft nur das eigene Fenster.
    /// Ohne Mengenfenster nichts.
    #[test]
    fn a98_gemeinsam_minimieren() {
        let mut w = offen_angedockt();
        assert_eq!(
            minimiert(&mut w, Fenster::Haupt),
            [Tat::Minimiere(Fenster::Mengen)]
        );
        assert_eq!(
            minimiert(&mut w, Fenster::Mengen),
            [],
            "Rückmeldung: Sperre"
        );
        assert_eq!(
            wiederhergestellt(&mut w, Fenster::Mengen),
            [Tat::Hole(Fenster::Haupt)],
            "Taskleiste Mengenfenster holt beide"
        );
        assert_eq!(wiederhergestellt(&mut w, Fenster::Haupt), [], "Rückmeldung");
        assert_eq!(
            minimiert(&mut w, Fenster::Mengen),
            [Tat::Minimiere(Fenster::Haupt)]
        );
        assert_eq!(minimiert(&mut w, Fenster::Haupt), []);
        assert_eq!(
            wiederhergestellt(&mut w, Fenster::Haupt),
            [Tat::Hole(Fenster::Mengen)]
        );
        assert_eq!(wiederhergestellt(&mut w, Fenster::Mengen), []);
        // Nach der Rückmeldung ist die Sperre wieder offen
        assert_eq!(
            minimiert(&mut w, Fenster::Haupt),
            [Tat::Minimiere(Fenster::Mengen)]
        );
        minimiert(&mut w, Fenster::Mengen);
        wiederhergestellt(&mut w, Fenster::Haupt);
        wiederhergestellt(&mut w, Fenster::Mengen);
        assert_eq!(maximiert(&mut w, Fenster::Mengen), [], "nur das eigene");
        assert_eq!(maximiert(&mut w, Fenster::Haupt), []);
        let mut zu = logik(520.0);
        assert_eq!(minimiert(&mut zu, Fenster::Haupt), [], "ohne Mengenfenster");
    }

    /// A99 (F2 Merken): Offen/zu, angedockt/frei, Lage, Größe und Bildschirm
    /// stehen in einstellungen.txt und kommen beim Neustart zurück; fehlt der
    /// gemerkte Bildschirm, wird wieder angedockt. Die .szo bleibt unberührt.
    #[test]
    fn a99_lage_merken() {
        let monitore = [ARBEIT, (1920, 0, 1920, 1040)];
        let mut w = offen_angedockt();
        mengen_gezogen(&mut w, (2100, 80, 600, 900));
        assert!(!angedockt(&w));
        let text = merken(&w);
        let w2 = lesen(&text, &monitore);
        assert!(offen(&w2) && !angedockt(&w2));
        assert_eq!(lage(&w2), (2100, 80, 600, 900));
        assert_eq!(merken(&w2), text, "verlustfrei");
        let w1 = lesen(&text, &monitore[..1]);
        assert!(offen(&w1) && angedockt(&w1), "Bildschirm fehlt: angedockt");
        let mut zu = offen_angedockt();
        schliessen(&mut zu, Fenster::Mengen, false);
        assert!(!offen(&lesen(&merken(&zu), &monitore)), "zu bleibt zu");
        assert!(!offen(&lesen("", &monitore)), "ohne Eintrag zu");
        // Nicht in der .szo
        let mut s = Scene::with_model(Model::with_seed(99));
        zeichne_rechteck(&mut s, &cam3d());
        let szo = sk_model::szo::write(s.model());
        assert!(!szo.contains("Mengen") && !szo.contains("quantity"));
    }

    /// A100 (F2 gemeinsamer Zustand): Hover und Auswahl gibt es einmal; beide
    /// Fenster lesen ihn. Strg+Klick fügt hinzu und nimmt weg, Esc hebt auf,
    /// ein Bauteil, das es nicht mehr gibt, fällt heraus. Hover hebt das Bauteil
    /// in 3D, Grundriss und Schnitt in der Rolle `interact.hover_element` hervor.
    /// Nichts davon ändert Modell, Rückgängig oder Datei.
    #[test]
    fn a100_gemeinsamer_hover_und_auswahl() {
        let th = Theme::dark();
        let mut s = Scene::with_model(Model::with_seed(100));
        let aw = zeichne_rechteck(&mut s, &cam3d());
        let iw = innenwand(
            &mut s,
            &cam3d(),
            vec3(5000.0, 0.0, 0.0),
            vec3(5000.0, 8000.0, 0.0),
        );
        let (a1, a2) = (
            s.model().wall_at(aw, 0).unwrap(),
            s.model().wall_at(aw, 1).unwrap(),
        );
        let i1 = s.model().wall_at(iw, 0).unwrap();
        let (rev, label, text) = (s.model().revision(), s.undo_label(), szo(&s));
        let mut p = Picking::default();
        waehle(&mut p, a1, false);
        waehle(&mut p, a2, true);
        assert_eq!(p.selected, [a1, a2], "Strg fügt hinzu");
        waehle(&mut p, a1, true);
        assert_eq!(p.selected, [a2], "Strg nimmt weg");
        waehle(&mut p, i1, false);
        assert_eq!(p.selected, [i1], "Klick ersetzt");
        p.hover = Some(a1);
        for v in [ViewKind::Persp, ViewKind::Plan, ViewKind::Front] {
            let f = hover_farben(&s, &p, v, &th);
            assert!(!f.is_empty(), "{v:?}: hervorgehoben");
            assert!(f.iter().all(|c| *c == th.interact.hover_element), "{v:?}");
        }
        let mut sect = SectionLine::default();
        sect.ensure(&s);
        assert!(!hover_farben(&s, &p, ViewKind::Section, &th).is_empty());
        p.hover = None;
        assert!(hover_farben(&s, &p, ViewKind::Persp, &th).is_empty());
        assert_eq!(
            (s.model().revision(), s.undo_label(), szo(&s)),
            (rev, label, text),
            "nur Sitzungszustand"
        );
        // Innenwand rückgängig: fällt aus der Auswahl
        p.hover = Some(i1);
        assert!(s.undo());
        pruefe(&mut p, &s);
        assert!(p.selected.is_empty() && p.hover.is_none());
        waehle(&mut p, a1, false);
        esc(&mut p);
        assert!(p.selected.is_empty(), "Esc");
        // Prüfregel 1: eigene Rolle im Schema
        assert_ne!(th.interact.hover_element, th.interact.select);
    }

    // Nachtrag zu F2 (Koordinator 14:36, Bauthread): zwei Erwartungen für den
    // kleinen Nachtrag zur Fensterschicht, vorbereitet gegen main 023eb7a, nachgezogen auf 656204f.
    // Gehört in das Modul `f2` von abnahme.rs (nutzt dessen Adapter `logik`,
    // `oeffne`, `schliessen`, `mengen_gezogen`, `lage`, `Tat`, `Fenster`,
    // `ARBEIT`). Spezifikation: test/abnahme-mengenliste.md (A96, A98).
    //
    // Stand 656204f: eingebaut als `Windows::maximize_main(main, work)`; leere
    // Liste = normal maximieren lassen. Lesart des Bauthreads (abgenommen
    // 2026-10-06): Das Hauptfenster füllt den Arbeitsbereich abzüglich der
    // Breite des Mengenfensters, das Mengenfenster behält seine Breite und
    // rückt an den rechten Rand des Arbeitsbereichs (volle Höhe). Nochmal
    // maximieren bringt die Lage von vorher zurück. Die gleichen Fälle decken
    // die Unit-Tests in sk-platform/src/layout.rs ab
    // (`oeffnen_gibt_breite_ab_und_zurueck`, `maximieren_fuellt_den_rest`);
    // diese Datei hebt sie in den Abnahmekatalog (A96/A98).

    // ===== Adapter Nachtrag =====

    /// Hauptfenster soll maximiert werden; leer: normal maximieren lassen.
    fn haupt_maximiert(w: &mut Windows, haupt: R, arbeit: R) -> Vec<Tat> {
        taten(w.maximize_main(haupt, arbeit))
    }

    // ===== Tests =====

    /// A96 Nachtrag (1): Reicht rechts neben dem Hauptfenster der Platz auf
    /// demselben Bildschirm nicht für 520 dip, gibt das Hauptfenster rechts
    /// Breite ab (linke Kante, Oberkante und Höhe bleiben); das Mengenfenster
    /// liegt ganz auf dem Bildschirm. Beim Schließen bekommt das Hauptfenster
    /// seine alte Breite zurück. Mit genug Platz ändert sich nichts.
    #[test]
    fn a96_nachtrag_platz_rechts_zu_knapp() {
        let haupt: R = (300, 100, 1500, 800); // rechte Kante 1800, Bildschirm 1920
        let mut w = logik(520.0);
        assert_eq!(
            oeffne(&mut w, haupt, false, ARBEIT, 1.0),
            [
                Tat::Setze(Fenster::Haupt, (300, 100, 1100, 800)),
                Tat::Lege(Fenster::Mengen, (1400, 100, 520, 800)),
            ]
        );
        assert_eq!(
            schliessen(&mut w, Fenster::Mengen, false),
            [
                Tat::Schliesse(Fenster::Mengen),
                Tat::Setze(Fenster::Haupt, haupt),
            ],
            "alte Breite zurück"
        );
        // 150 %: 780 px
        let mut w = logik(520.0);
        assert_eq!(
            oeffne(&mut w, haupt, false, ARBEIT, 1.5),
            [
                Tat::Setze(Fenster::Haupt, (300, 100, 840, 800)),
                Tat::Lege(Fenster::Mengen, (1140, 100, 780, 800)),
            ]
        );
        // Genug Platz (rechte Kante 1300 + 520 ≤ 1920): Hauptfenster bleibt
        let mut w = logik(520.0);
        assert_eq!(
            oeffne(&mut w, (100, 100, 1200, 800), false, ARBEIT, 1.0),
            [Tat::Lege(Fenster::Mengen, (1300, 100, 520, 800))]
        );
        assert_eq!(
            schliessen(&mut w, Fenster::Mengen, false),
            [Tat::Schliesse(Fenster::Mengen)]
        );
    }

    /// A98 Nachtrag (2): Maximieren des Hauptfensters bei angedocktem
    /// Mengenfenster: Das Hauptfenster füllt den Arbeitsbereich abzüglich der
    /// Breite des Mengenfensters, das Mengenfenster behält seine Breite und
    /// liegt rechts daneben auf voller Höhe. Nochmal maximieren gibt die alte
    /// Lage zurück. Ist es gelöst oder auf einem anderen Bildschirm, maximiert
    /// das Hauptfenster normal.
    #[test]
    fn a98_nachtrag_maximieren_angedockt() {
        let haupt: R = (100, 100, 1200, 800);
        let mut w = logik(520.0);
        oeffne(&mut w, haupt, false, ARBEIT, 1.0);
        assert_eq!(lage(&w), (1300, 100, 520, 800));
        assert_eq!(
            haupt_maximiert(&mut w, haupt, ARBEIT),
            [
                Tat::Setze(Fenster::Haupt, (0, 0, 1400, 1040)),
                Tat::Setze(Fenster::Mengen, (1400, 0, 520, 1040)),
            ],
            "Arbeitsbereich minus Mengenfenster, Breite bleibt"
        );
        let t = haupt_maximiert(&mut w, (0, 0, 1400, 1040), ARBEIT);
        assert_eq!(
            t.first(),
            Some(&Tat::Setze(Fenster::Haupt, haupt)),
            "zurück"
        );
        // Gelöst: normal maximieren
        let mut w = logik(520.0);
        oeffne(&mut w, haupt, false, ARBEIT, 1.0);
        mengen_gezogen(&mut w, (1400, 150, 520, 800));
        assert!(haupt_maximiert(&mut w, haupt, ARBEIT).is_empty(), "gelöst");
        // Auf dem zweiten Bildschirm: normal maximieren
        mengen_gezogen(&mut w, (2100, 80, 520, 800));
        assert!(
            haupt_maximiert(&mut w, haupt, ARBEIT).is_empty(),
            "anderer Bildschirm"
        );
        // Ohne Mengenfenster: normal
        let mut zu = logik(520.0);
        assert!(haupt_maximiert(&mut zu, haupt, ARBEIT).is_empty());
    }
}

mod b7 {
    use super::*;

    // Abnahmetests B7 „Mengenermittlung im zweiten Fenster“, Datenseite und
    // Kopplung (bim/paket-b7-mengenliste.md, Jörn 06.10. 14:00), vorbereitet gegen
    // main 89d28ac. Spezifikation: test/abnahme-mengenliste.md. Setzt F2
    // (a96-a100-fensterschicht.rs: `Picking`) voraus.
    //
    // Die Daten hinter der Liste: Gliederung, Sollwerte, Taschen, Summen nach
    // Baustoff, Bauteile ohne Körper, Rechenzeitpunkt mit „wird aktualisiert“,
    // CSV; dazu A101 die Kopplung Liste ↔ Modell über den gemeinsamen Zustand.
    // Aussehen des Mengenfensters (Zeilen, Bänder, Kacheln, Rollen,
    // Aufleuchten) folgt nach Jörns Abnahme der Skizzen als Bildvergleich und
    // Handtest.
    //
    // Die unterste Gruppe heißt „Fundament“ (Koordinator 13:37), Kurzname
    // des Geschosses weiter „GR“.
    //
    // Prüfhaus: Gebäude aus dem Dialog (2 Geschosse, Standardhöhen Jörn 10:13),
    // Rechteck 10 × 8 m, AW 31,5, EG-Innenwand IW 17,5 bei x = 5 m. Alle Sollwerte
    // von Hand nachgerechnet (abnahme-mengenliste.md).
    //
    // Angenommene Namen stehen nur in den Adaptern. Neu angenommen: das Paket-API
    // `sk_model::qto::{Schedule, ElementQto}` mit den Feldern aus dem Paket,
    // `WallQto::pocket`, `WallQto::list_length`, `LayerQto::side_area`, `FloorQto::bearing`, für Bauteile
    // ohne Körper `RowQto::q` als `Option` mit `RowQto::note`; in der App
    // `Scene::schedule` (je Revision gemerkt), `Scene::schedule_runs` (Zähler der
    // Berechnungen), `Scene::schedule_stale` („wird aktualisiert“) und im
    // Mengenfenster `crate::schedule_view::{ListView, csv}`.

    // ===== Adapter B7 =====

    use sk_model::qto::{ElementQto, FloorQto, LayerQto, Schedule, WallQto};
    use sk_model::Category;

    /// Die Liste, wie das Mengenfenster sie zeigt (je Revision gemerkt).
    fn liste(s: &mut Scene) -> Schedule {
        s.schedule().clone()
    }

    /// Steht oben im Mengenfenster „wird aktualisiert“ (Liste älter als das
    /// Modell, z. B. beim Ziehen)?
    fn veraltet(s: &Scene) -> bool {
        s.schedule_stale()
    }

    /// Wie oft die Liste bisher berechnet wurde.
    fn berechnungen(s: &Scene) -> u64 {
        s.schedule_runs()
    }

    /// Inhalt der .csv aus „Als Tabelle speichern“ (Bytes, wie geschrieben).
    fn csv(s: &mut Scene) -> Vec<u8> {
        let l = liste(s);
        crate::schedule_view::csv(s.model(), &l)
    }

    /// Volumen, das die Decke aus einer Wand bzw. Schicht nimmt (mm³).
    fn tasche(w: &WallQto) -> f64 {
        w.pocket
    }

    /// Außenfläche einer Schicht (mm²), für die WDVS-m².
    fn schicht_flaeche(l: &LayerQto) -> f64 {
        l.side_area
    }

    /// Länge in der Liste (mm): Außenwand Außenseite (side_outer / height),
    /// Innenwand lichte Länge der Kernschicht (BIM 13:38).
    fn listen_laenge(w: &WallQto) -> f64 {
        w.list_length
    }

    /// „davon Auflager in den Außenwänden“ einer Decke (mm³).
    fn auflager(f: &FloorQto) -> f64 {
        f.bearing
    }

    /// Eine Zeile der Liste, flach: Geschoss der Gruppe (Kurzname, „GR“ für die
    /// Gruppe Fundament), Bauteilart, Nummer, Mengen (`None` = kein Körper) und
    /// der Grund in grauer Schrift.
    struct Zeile {
        geschoss: String,
        art: Category,
        nummer: String,
        q: Option<ElementQto>,
        grund: Option<String>,
    }

    fn zeilen(m: &Model, l: &Schedule) -> Vec<Zeile> {
        let mut v = Vec::new();
        for b in &l.buildings {
            for st in &b.storeys {
                let kurz = m.storey(st.id).map_or(String::new(), |x| x.short.clone());
                for g in &st.groups {
                    for r in &g.rows {
                        v.push(Zeile {
                            geschoss: kurz.clone(),
                            art: g.category,
                            nummer: r.number.clone(),
                            q: r.q.clone(),
                            grund: r.note.clone(),
                        });
                    }
                }
            }
        }
        v
    }

    /// Gebäudenummern und je Gebäude die Gruppen in Listenreihenfolge:
    /// (Geschoss, Bauteilart, Nummern).
    type Gruppen = Vec<(String, Vec<(String, Category, Vec<String>)>)>;
    fn gruppen(m: &Model, l: &Schedule) -> Gruppen {
        l.buildings
            .iter()
            .map(|b| {
                let nr = m.building(b.id).map_or(String::new(), |x| x.number.clone());
                let g = b
                    .storeys
                    .iter()
                    .flat_map(|st| {
                        let kurz = m.storey(st.id).map_or(String::new(), |x| x.short.clone());
                        st.groups.iter().map(move |g| {
                            let n = g.rows.iter().map(|r| r.number.clone()).collect();
                            (kurz.clone(), g.category, n)
                        })
                    })
                    .collect();
                (nr, g)
            })
            .collect()
    }

    /// Summe nach Baustoff des ersten Gebäudes: (Name, m³, m²).
    fn baustoffe(m: &Model, l: &Schedule) -> Vec<(String, f64, Option<f64>)> {
        l.buildings[0]
            .by_material
            .iter()
            .map(|x| {
                let name = m
                    .material(x.material)
                    .map_or(String::new(), |y| y.name.clone());
                (name, x.volume / 1e9, x.area.map(|a| a / 1e6))
            })
            .collect()
    }

    // Mengenfenster: Zeilen nach Bauteilnummer bzw. Gruppe (Geschoss-Kurzname,
    // Bauteilart); der gemeinsame Zustand `Picking` kommt aus F2.
    use crate::picking::Picking;
    use crate::schedule_view::ListView;

    fn mengenfenster(s: &mut Scene) -> ListView {
        ListView::new(s)
    }
    /// Maus über der Zeile eines Bauteils bzw. einer Gruppe.
    fn zeile_hover(v: &mut ListView, s: &mut Scene, p: &mut Picking, nummer: &str) {
        v.hover_row(s, p, nummer)
    }
    fn gruppe_hover(v: &mut ListView, s: &mut Scene, p: &mut Picking, gs: &str, art: Category) {
        v.hover_group(s, p, gs, art)
    }
    /// Klick auf eine Zeile; `strg`/`umschalt` wie in Windows-Listen.
    fn zeile_klick(
        v: &mut ListView,
        s: &mut Scene,
        p: &mut Picking,
        nummer: &str,
        strg: bool,
        umschalt: bool,
    ) {
        v.click_row(s, p, nummer, strg, umschalt)
    }
    fn gruppe_klick(v: &mut ListView, s: &mut Scene, p: &mut Picking, gs: &str, art: Category) {
        v.click_group(s, p, gs, art)
    }
    /// Die Liste folgt dem gemeinsamen Zustand (nach Auswahl oder Hover in einer
    /// Ansicht des Hauptfensters).
    fn folgen(v: &mut ListView, s: &mut Scene, p: &Picking) {
        v.follow(s, p);
    }
    /// Ist die Zeile sichtbar (Gruppe aufgeklappt, in den sichtbaren Bereich
    /// gerollt)?
    fn zeile_sichtbar(v: &ListView, nummer: &str) -> bool {
        v.row_visible(nummer)
    }
    /// Zeilen mit Auswahlband bzw. Hover-Band.
    fn zeilen_gewaehlt(v: &ListView) -> Vec<String> {
        v.selected_rows()
    }
    fn zeilen_hover(v: &ListView) -> Vec<String> {
        v.hover_rows()
    }
    /// Esc im Mengenfenster.
    fn liste_esc(v: &mut ListView, s: &mut Scene, p: &mut Picking) {
        v.escape(s, p)
    }

    // ===== Hilfen =====

    /// Prüfhaus B7: Gebäude aus dem Dialog, EG-Innenwand bei x = 5 m.
    fn haus_b7(s: &mut Scene) -> (RunId, RunId) {
        let (eg, _) = gebaeude(s);
        let iw = innenwand(
            s,
            &cam3d(),
            vec3(5000.0, 0.0, 0.0),
            vec3(5000.0, 8000.0, 0.0),
        );
        (eg, iw)
    }

    fn r4(v: f64) -> f64 {
        (v * 1e4).round() / 1e4
    }

    /// Meter aus mm, 4 Nachkommastellen.
    fn lfm(v: f64) -> f64 {
        r4(v / 1e3)
    }

    fn zeile<'a>(z: &'a [Zeile], nummer: &str) -> &'a Zeile {
        z.iter()
            .find(|x| x.nummer == nummer)
            .unwrap_or_else(|| panic!("Zeile {nummer} fehlt"))
    }

    fn wand(z: &Zeile) -> &WallQto {
        match &z.q {
            Some(ElementQto::Wall(w)) => w,
            q => panic!("{}: keine Wand {q:?}", z.nummer),
        }
    }

    fn decke(z: &Zeile) -> &FloorQto {
        match &z.q {
            Some(ElementQto::Floor(f)) => f,
            q => panic!("{}: keine Decke {q:?}", z.nummer),
        }
    }

    /// Außenwände eines Geschosses.
    fn aussenwaende<'a>(z: &'a [Zeile], geschoss: &str) -> Vec<&'a Zeile> {
        z.iter()
            .filter(|x| x.geschoss == geschoss && x.art == Category::ExteriorWall)
            .collect()
    }

    fn baustoff_summe(v: &[(String, f64, Option<f64>)], name: &str) -> (f64, Option<f64>) {
        v.iter()
            .find(|x| x.0 == name)
            .map(|x| (r4(x.1), x.2.map(r4)))
            .unwrap_or_else(|| panic!("Baustoff {name} fehlt: {v:?}"))
    }

    // ===== Tests =====

    /// A88 (B7 Aufbau, Gruppierung): Ein Gebäude GB-01; Gruppen nach Bauablauf:
    /// Fundament (FS-001, SP-001 nach Kostengruppe 322, obwohl die Sohlplatte zum
    /// EG gehört), EG (AW-001…004, IW-001, DE-001), OG (AW-005…008, DE-002);
    /// innerhalb der Gruppe nach Nummer. Jedes Bauteil ist an ein Geschoss des
    /// Gebäudes gebunden (A-09). Die Liste wird nie gespeichert.
    #[test]
    fn a88_gliederung_nach_bauablauf() {
        let mut s = Scene::with_model(Model::with_seed(88));
        haus_b7(&mut s);
        let text = szo(&s);
        let l = liste(&mut s);
        let g = gruppen(s.model(), &l);
        assert_eq!(g.len(), 1, "ein Gebäude");
        assert_eq!(g[0].0, "GB-01");
        let n = |v: &[&str]| v.iter().map(|x| x.to_string()).collect::<Vec<_>>();
        assert_eq!(
            g[0].1,
            vec![
                ("GR".into(), Category::StripFooting, n(&["FS-001"])),
                ("GR".into(), Category::GroundSlab, n(&["SP-001"])),
                (
                    "EG".into(),
                    Category::ExteriorWall,
                    n(&["AW-001", "AW-002", "AW-003", "AW-004"])
                ),
                ("EG".into(), Category::InteriorWall, n(&["IW-001"])),
                ("EG".into(), Category::Floor, n(&["DE-001"])),
                (
                    "OG".into(),
                    Category::ExteriorWall,
                    n(&["AW-005", "AW-006", "AW-007", "AW-008"])
                ),
                ("OG".into(), Category::Floor, n(&["DE-002"])),
            ]
        );
        // Jedes Bauteil der Liste ist im Modell an ein Geschoss des Gebäudes
        // gebunden; außer bei Sohlplatte und Frostschürze (Gruppe Fundament nach
        // Kostengruppe 322, im Modell heute am EG) ist es das der Gruppe
        let m = s.model();
        let z = zeilen(m, &l);
        assert_eq!(z.len(), 13, "alle Bauteile, keines doppelt");
        for x in &z {
            let (_, el) = m
                .elements()
                .iter()
                .find(|(_, e)| e.number == x.nummer)
                .unwrap();
            let st = m.storey(el.storey).expect("Geschoss");
            assert!(m.building_of(el.storey).is_some(), "{}: Gebäude", x.nummer);
            if x.geschoss == "GR" {
                assert!(["GR", "EG"].contains(&st.short.as_str()), "{}", x.nummer);
            } else {
                assert_eq!(st.short, x.geschoss, "{}", x.nummer);
            }
            assert!(x.grund.is_none(), "{}: {:?}", x.nummer, x.grund);
        }
        assert_eq!(szo(&s), text, "Liste nicht in der Datei");
    }

    /// A89 (B7 Fertig, wenn: Fundament, Innenwand, Decken): FS-001 34,60 m und
    /// 7,0238 m³; SP-001 80,00 m², 17,6000 m³, Umfang 36,00 m; IW-001 7,37 m,
    /// 3,3985 m³ netto, Abzug Deckenstreifen 0,2837 m³ (Paket: 0,2838; genau
    /// 0,283745); DE-001 und DE-002 je
    /// 75,0384 m², 16,5084 m³, Rand 34,88 m, Auflager in den Außenwänden
    /// 1,3159 m³. Die Länge der Innenwand in der Liste (`list_length`) ist die
    /// lichte Länge der Kernschicht (7,37 m); `WallQto::length` bleibt die
    /// Bezugslinie bis zur Außenkante (8,00 m).
    #[test]
    fn a89_sollwerte_gruendung_innenwand_decken() {
        let mut s = Scene::with_model(Model::with_seed(89));
        haus_b7(&mut s);
        let l = liste(&mut s);
        let z = zeilen(s.model(), &l);
        match &zeile(&z, "FS-001").q {
            Some(ElementQto::Footing(f)) => {
                assert_eq!(lfm(f.length), 34.6);
                assert_eq!(m3(f.volume), 7.0238);
            }
            q => panic!("FS-001 {q:?}"),
        }
        match &zeile(&z, "SP-001").q {
            Some(ElementQto::Slab(p)) => {
                assert_eq!(m2(p.area), 80.0);
                assert_eq!(m3(p.volume), 17.6);
                assert_eq!(lfm(p.perimeter), 36.0);
            }
            q => panic!("SP-001 {q:?}"),
        }
        let iw = wand(zeile(&z, "IW-001"));
        assert_eq!(lfm(listen_laenge(iw)), 7.37, "lichte Länge");
        assert_eq!(lfm(iw.length), 8.0, "Bezugslinie");
        assert_eq!(m3(iw.volume), 3.3985, "netto");
        assert_eq!(
            m3(tasche(iw)),
            0.2837,
            "Abzug Deckenstreifen 7,37 × 0,175 × 0,22"
        );
        for nr in ["DE-001", "DE-002"] {
            let d = decke(zeile(&z, nr));
            assert_eq!(m2(d.area), 75.0384, "{nr}");
            assert_eq!(m3(d.volume), 16.5084, "{nr}");
            assert_eq!(lfm(d.perimeter), 34.88, "{nr}");
            assert_eq!(m3(auflager(d)), 1.3159, "{nr}: davon Auflager");
        }
    }

    /// A90 (B7 Fertig, wenn: Außenwände): je Geschoss 4 Stück, 36,00 m;
    /// Gasbeton 15,7613 m³ (Längswand 4,4014, Querwand 3,4792); Dämmung
    /// 14,1654 m³ und 102,78 m²; Abzug Deckenauflager 1,3159 m³ (Längswand
    /// 0,3675, Querwand 0,2905); netto 29,9266 m³ (Paket: 29,9267 aus den
    /// gerundeten Schichtsummen; ungerundet 15,761256 + 14,165368). OG gleich EG.
    #[test]
    fn a90_sollwerte_aussenwaende_je_geschoss() {
        let mut s = Scene::with_model(Model::with_seed(90));
        haus_b7(&mut s);
        let l = liste(&mut s);
        let z = zeilen(s.model(), &l);
        for gs in ["EG", "OG"] {
            let aw = aussenwaende(&z, gs);
            assert_eq!(aw.len(), 4, "{gs}");
            let sum = |f: &dyn Fn(&WallQto) -> f64| aw.iter().map(|x| f(wand(x))).sum::<f64>();
            assert_eq!(lfm(sum(&listen_laenge)), 36.0, "{gs}: Länge");
            assert_eq!(lfm(sum(&|w| w.length)), 36.0, "{gs}: Bezugslinie");
            assert_eq!(m3(sum(&|w| w.layers[1].volume)), 15.7613, "{gs}: Gasbeton");
            assert_eq!(m3(sum(&|w| w.layers[0].volume)), 14.1654, "{gs}: Dämmung");
            assert_eq!(
                m2(sum(&|w| schicht_flaeche(&w.layers[0]))),
                102.78,
                "{gs}: WDVS-Fläche 36,00 × 2,855"
            );
            assert_eq!(m3(sum(&tasche)), 1.3159, "{gs}: Abzug Deckenauflager");
            assert_eq!(m3(sum(&|w| w.volume)), 29.9266, "{gs}: netto");
            for x in &aw {
                let w = wand(x);
                assert!(
                    (listen_laenge(w) - w.side_outer / w.height).abs() < 1e-6,
                    "{gs} {}: Außenseite",
                    x.nummer
                );
                let (gb, t) = match lfm(listen_laenge(w)) {
                    10.0 => (4.4014, 0.3675),
                    8.0 => (3.4792, 0.2905),
                    l => panic!("{}: Länge {l}", x.nummer),
                };
                assert_eq!(m3(w.layers[1].volume), gb, "{gs} {}: Gasbeton", x.nummer);
                assert_eq!(m3(tasche(w)), t, "{gs} {}: Tasche", x.nummer);
                assert_eq!(w.height, 2855.0, "{gs} {}: Wandhöhe", x.nummer);
            }
        }
    }

    /// A91 (B7 „Taschenvolumen als eigene Zeile“): Die Tasche wird genau einmal
    /// gezählt: in der Decke enthalten, in der Wand abgezogen. Je Geschoss gilt
    /// Auflager der Decke = Summe der Wandtaschen; Gasbeton netto + Tasche =
    /// Gasbeton brutto (Fläche × 2,855); die Tasche liegt nur im Gasbeton, nicht
    /// im WDVS; Stahlbeton = Platte + Schürze + beide Decken (mit Taschen).
    #[test]
    fn a91_tasche_nicht_doppelt() {
        let mut s = Scene::with_model(Model::with_seed(91));
        haus_b7(&mut s);
        let l = liste(&mut s);
        let m = s.model();
        let z = zeilen(m, &l);
        for (gs, de) in [("EG", "DE-001"), ("OG", "DE-002")] {
            let aw = aussenwaende(&z, gs);
            let taschen: f64 = aw.iter().map(|x| tasche(wand(x))).sum();
            assert!(
                (auflager(decke(zeile(&z, de))) - taschen).abs() < 1.0,
                "{gs}: Auflager der Decke = Summe der Wandtaschen"
            );
            for x in &aw {
                let w = wand(x);
                let gb = &w.layers[1];
                assert!(
                    (gb.volume + tasche(w) - gb.area * 2855.0).abs() < 1.0,
                    "{gs} {}: netto + Tasche = brutto",
                    x.nummer
                );
                let wdvs = &w.layers[0];
                assert!(
                    (wdvs.volume - wdvs.area * 2855.0).abs() < 1.0,
                    "{gs} {}: WDVS ohne Abzug",
                    x.nummer
                );
            }
        }
        // Stahlbeton: jedes Bauteil genau einmal, Decken mit Taschen
        let sb: f64 = ["SP-001", "FS-001", "DE-001", "DE-002"]
            .iter()
            .map(|nr| match &zeile(&z, nr).q {
                Some(ElementQto::Slab(p)) => p.volume,
                Some(ElementQto::Footing(f)) => f.volume,
                Some(ElementQto::Floor(f)) => f.volume,
                q => panic!("{nr} {q:?}"),
            })
            .sum();
        let b = baustoffe(m, &l);
        assert_eq!(baustoff_summe(&b, "Stahlbeton").0, r4(sb / 1e9));
        // Gasbeton: Wände netto plus Innenwand netto, Taschen nicht enthalten
        let gb: f64 = z
            .iter()
            .filter(|x| matches!(x.art, Category::ExteriorWall))
            .map(|x| wand(x).layers[1].volume)
            .sum::<f64>()
            + wand(zeile(&z, "IW-001")).volume;
        assert_eq!(baustoff_summe(&b, "Gasbeton").0, r4(gb / 1e9));
    }

    /// A92 (B7 Summen, „Immer aktuell“): Summe nach Baustoff GB-01: Stahlbeton
    /// 57,6407 m³, Gasbeton 34,9210 m³, Dämmung (WDVS) 28,3307 m³ / 205,56 m²
    /// (aus ungerundeten Werten). Gummiband EG-Wand y = 8 um +1 m: SP 90,00 m²,
    /// DE-001 und DE-002 je 84,7584 m², ohne Knopfdruck.
    #[test]
    fn a92_summe_nach_baustoff_und_immer_aktuell() {
        let mut s = Scene::with_model(Model::with_seed(92));
        haus_b7(&mut s);
        let l = liste(&mut s);
        let b = baustoffe(s.model(), &l);
        assert_eq!(baustoff_summe(&b, "Stahlbeton"), (57.6407, None));
        assert_eq!(baustoff_summe(&b, "Gasbeton"), (34.921, None));
        assert_eq!(
            baustoff_summe(&b, "Dämmung (WDVS)"),
            (28.3307, Some(205.56))
        );
        assert_eq!(b.len(), 3, "nur vorhandene Baustoffe: {b:?}");
        // Gummiband
        ziehen_am_fuss(&mut s, 0.0, 1000.0);
        let l = liste(&mut s);
        let z = zeilen(s.model(), &l);
        match &zeile(&z, "SP-001").q {
            Some(ElementQto::Slab(p)) => assert_eq!(m2(p.area), 90.0),
            q => panic!("SP-001 {q:?}"),
        }
        for nr in ["DE-001", "DE-002"] {
            assert_eq!(m2(decke(zeile(&z, nr)).area), 84.7584, "{nr}");
        }
    }

    /// A93 (B7 Prüfung): Bauteile ohne Körper verschwinden nicht still. Mit
    /// Sockelrücksprung 40 cm (größer als die Wand) bleiben SP-001 und FS-001 als
    /// Zeile mit „–“ (keine Mengen) und dem Grund; sie zählen in keine Summe.
    #[test]
    fn a93_ohne_koerper_mit_grund() {
        let mut s = Scene::with_model(Model::with_seed(93));
        let (eg, _) = haus_b7(&mut s);
        let (slab, _) = sohlplatte(&s, eg);
        assert!(s.edit_model("Rücksprung", |m| m.set_slab_recess(slab, 400.0)));
        let l = liste(&mut s);
        let z = zeilen(s.model(), &l);
        assert_eq!(z.len(), 13, "keine Zeile verschwunden");
        for nr in ["SP-001", "FS-001"] {
            let x = zeile(&z, nr);
            assert!(x.q.is_none(), "{nr}: keine Mengen");
            let g = x.grund.as_deref().unwrap_or("");
            assert!(g.contains("Rücksprung zu groß"), "{nr}: Grund „{g}“");
        }
        let b = baustoffe(s.model(), &l);
        assert_eq!(
            baustoff_summe(&b, "Stahlbeton").0,
            33.0169,
            "nur die beiden Decken"
        );
        // Rückgängig: wieder mit Körper
        assert!(s.undo());
        let l = liste(&mut s);
        assert!(zeilen(s.model(), &l).iter().all(|x| x.q.is_some()));
    }

    /// A94 (B7 Leistung, Regel 4): Die Liste wird einmal je Modellrevision
    /// berechnet und gemerkt, nie während des Ziehens (auch wenn die Ansicht sie
    /// in jedem Bild anfragt) und erst nach dem Loslassen neu. Sie ändert weder
    /// Revision noch Rückgängig-Liste noch Datei.
    #[test]
    fn a94_rechnung_nie_waehrend_des_ziehens() {
        let mut s = Scene::with_model(Model::with_seed(94));
        haus_b7(&mut s);
        let (rev, label, text) = (s.model().revision(), s.undo_label(), szo(&s));
        let n0 = berechnungen(&s);
        let erste = liste(&mut s);
        assert_eq!(berechnungen(&s), n0 + 1, "einmal berechnet");
        assert!(!veraltet(&s));
        for _ in 0..5 {
            liste(&mut s);
        }
        assert_eq!(berechnungen(&s), n0 + 1, "gemerkt, solange nichts ändert");
        assert_eq!(
            (s.model().revision(), s.undo_label(), szo(&s)),
            (rev, label, text),
            "nur Lesen"
        );
        // Ziehen der Geschosslinie über zehn Bilder, die Ansicht fragt jedes Mal
        s.begin("Geschoss ziehen");
        for i in 0..10 {
            s.drag_storey_top(geschoss(&s, "EG"), 2855.0 + 10.0 * i as f64);
            let l = liste(&mut s);
            assert_eq!(
                l.buildings.len(),
                erste.buildings.len(),
                "Stand vor dem Ziehen"
            );
        }
        assert_eq!(berechnungen(&s), n0 + 1, "nie während des Ziehens");
        assert!(veraltet(&s), "beim Ziehen: „wird aktualisiert“");
        s.commit();
        liste(&mut s);
        assert!(!veraltet(&s), "nach dem Loslassen aktuell");
        assert_eq!(berechnungen(&s), n0 + 2, "nach dem Loslassen einmal neu");
        // Gummiband: dasselbe
        let n = berechnungen(&s);
        ziehen_am_fuss(&mut s, 0.0, 500.0);
        assert_eq!(berechnungen(&s), n, "Ziehen allein rechnet nicht");
        liste(&mut s);
        assert_eq!(berechnungen(&s), n + 1);
    }

    /// A95 (B7 „Als Tabelle speichern“): Die .csv ist UTF-8 mit BOM, Semikolon,
    /// Dezimalkomma, 4 Nachkommastellen, alle Gruppen aufgeklappt (jede
    /// Bauteilnummer), Umlaute richtig; die Summen nach Baustoff stehen darin und
    /// stimmen mit der Liste überein. Ohne Körper: Zeile mit Grund.
    #[test]
    fn a95_csv_mit_dezimalkomma() {
        let mut s = Scene::with_model(Model::with_seed(95));
        let (eg, _) = haus_b7(&mut s);
        let bytes = csv(&mut s);
        assert_eq!(&bytes[..3], &[0xEF, 0xBB, 0xBF], "UTF-8 mit BOM");
        let text = std::str::from_utf8(&bytes[3..]).expect("UTF-8");
        let zeilen_csv: Vec<&str> = text.lines().filter(|z| !z.trim().is_empty()).collect();
        assert!(zeilen_csv.iter().all(|z| !z.contains('\t')));
        assert!(zeilen_csv.iter().any(|z| z.contains(';')), "Semikolon");
        for nr in [
            "GB-01", "FS-001", "SP-001", "AW-001", "AW-002", "AW-003", "AW-004", "IW-001",
            "DE-001", "AW-005", "AW-006", "AW-007", "AW-008", "DE-002",
        ] {
            assert!(text.contains(nr), "{nr} fehlt (alle Gruppen aufgeklappt)");
        }
        for wert in [
            "7,0238", "17,6000", "80,0000", "34,6000", "16,5084", "75,0384", "15,7613", "14,1654",
            "1,3159", "3,3985", "57,6407", "34,9210", "28,3307", "205,5600",
        ] {
            assert!(text.contains(wert), "Wert {wert} fehlt");
        }
        for falsch in ["17.6000", "7.0238", "57.6407"] {
            assert!(!text.contains(falsch), "Dezimalpunkt: {falsch}");
        }
        for wort in ["Außenwände", "Dämmung", "Fundament"] {
            assert!(text.contains(wort), "{wort}");
        }
        assert!(
            !text.contains("Guid") && !text.contains("guid"),
            "keine Guids"
        );
        // Ohne Körper: Zeile mit Grund statt Zahl
        let (slab, _) = sohlplatte(&s, eg);
        assert!(s.edit_model("Rücksprung", |m| m.set_slab_recess(slab, 400.0)));
        let bytes = csv(&mut s);
        let text = String::from_utf8_lossy(&bytes);
        let sp = text.lines().find(|z| z.contains("SP-001")).expect("SP-001");
        assert!(sp.contains("Rücksprung zu groß"), "{sp}");
        assert!(!sp.contains("17,6000"));
    }

    /// A101 (B7 Kopplung, Jörn 14:00): Hover über AW-003 in der Liste hebt
    /// AW-003 im Modell hervor, Hover über die Gruppe „Außenwände“ im EG alle
    /// vier; Klick wählt (ersetzt), Strg+Klick fügt hinzu, Umschalt+Klick wählt
    /// einen Bereich, Klick auf die OG-Außenwände wählt AW-005…008. Umgekehrt:
    /// DE-002 in einer Ansicht gewählt → Liste klappt das OG auf, Zeile sichtbar
    /// mit Auswahlband; Hover im Modell → Hover-Band. Esc in der Liste hebt die
    /// Auswahl in beiden Fenstern auf. Nichts davon rechnet die Liste neu.
    #[test]
    fn a101_kopplung_liste_und_modell() {
        let mut s = Scene::with_model(Model::with_seed(101));
        haus_b7(&mut s);
        let el = |s: &Scene, nr: &str| {
            s.model()
                .elements()
                .iter()
                .find(|(_, e)| e.number == nr)
                .unwrap()
                .0
        };
        let mut p = Picking::default();
        let mut v = mengenfenster(&mut s);
        liste(&mut s);
        let n = berechnungen(&s);
        // Liste → Modell
        zeile_hover(&mut v, &mut s, &mut p, "AW-003");
        assert_eq!(p.hover, Some(el(&s, "AW-003")));
        assert_eq!(zeilen_hover(&v), ["AW-003"]);
        gruppe_hover(&mut v, &mut s, &mut p, "EG", Category::ExteriorWall);
        assert_eq!(
            zeilen_hover(&v),
            ["AW-001", "AW-002", "AW-003", "AW-004"],
            "Gruppe: alle vier hervorgehoben"
        );
        zeile_klick(&mut v, &mut s, &mut p, "AW-003", false, false);
        assert_eq!(p.selected, [el(&s, "AW-003")]);
        zeile_klick(&mut v, &mut s, &mut p, "AW-004", true, false);
        assert_eq!(p.selected, [el(&s, "AW-003"), el(&s, "AW-004")], "Strg");
        zeile_klick(&mut v, &mut s, &mut p, "AW-001", false, false);
        zeile_klick(&mut v, &mut s, &mut p, "AW-003", false, true);
        assert_eq!(
            p.selected,
            ["AW-001", "AW-002", "AW-003"].map(|nr| el(&s, nr)),
            "Umschalt: Bereich"
        );
        gruppe_klick(&mut v, &mut s, &mut p, "OG", Category::ExteriorWall);
        assert_eq!(
            p.selected,
            ["AW-005", "AW-006", "AW-007", "AW-008"].map(|nr| el(&s, nr))
        );
        assert_eq!(
            zeilen_gewaehlt(&v),
            ["AW-005", "AW-006", "AW-007", "AW-008"]
        );
        // Modell → Liste: DE-002 in 3D gewählt
        p.selected = vec![el(&s, "DE-002")];
        p.hover = Some(el(&s, "IW-001"));
        folgen(&mut v, &mut s, &p);
        assert!(zeile_sichtbar(&v, "DE-002"), "OG aufgeklappt, gerollt");
        assert_eq!(zeilen_gewaehlt(&v), ["DE-002"]);
        assert_eq!(zeilen_hover(&v), ["IW-001"]);
        // Esc
        liste_esc(&mut v, &mut s, &mut p);
        assert!(p.selected.is_empty());
        assert!(zeilen_gewaehlt(&v).is_empty());
        assert_eq!(berechnungen(&s), n, "Hover und Auswahl rechnen nicht neu");
    }
}

/// Bauteilkatalog K1/K2 (A102–A109), von „Test und Abnahme“ vorbereitet.
mod katalog {
    use super::*;

    // Abnahmetests Bauteilkatalog K1 (Typ im Datenmodell, .szo v4) und K2
    // (Firmenkatalog .szk), vorbereitet gegen main 1f0763e.
    // Paket: bim/paket-k1-k3-bauteilkatalog.md („Fertig, wenn“), Konzept
    // bim/konzept-bauteilkatalog.md, Spezifikation test/abnahme-bauteilkatalog.md.
    //
    // Einbau: als `mod katalog { use super::*; … }` ans Ende von app/src/abnahme.rs.
    // Die Datei `a102-haus-v3.szo` (Prüfhaus aus haus_b11, mit main 1f0763e
    // gespeichert, Version 3) neben abnahme.rs als `abnahme_haus_v3.szo` legen.
    // Nutzt aus abnahme.rs: haus_b11, mengen_b11, STANDARD, og_zug, schale,
    // decke_hoehen, test_dir, m2.
    //
    // Angenommene Namen stehen NUR in den Adaptern unten. Weicht der Bau ab,
    // bitte nur die Adapter anpassen, nicht die Tests.

    use sk_model::catalog::{self as szk, Library, TypeState};
    use sk_model::{
        Category, ElementId, Guid, GuidGen, LayerFunction, LayerSet, LayerSetId, MaterialLayer,
        Pen, PropSet, PropValue, TypeCategory,
    };
    use std::collections::BTreeMap;

    const HAUS_V3: &str = include_str!("abnahme_haus_v3.szo");
    const AW1_GUID: &str = "33i8p9bQ580uop$jCXCU6X";
    const IW1_GUID: &str = "2bI2Vt9ej8GAcbFzlgyhUb";

    /// Mengen des Prüfhauses mit AW-31,5 auf 12 Dämmung + 24 Gasbeton (Paket,
    /// nachgerechnet auf main 1f0763e über set_layer_set).
    const K1_36: [f64; 6] = [12.1692, 21.5522, 3.357, 16.6623, 17.6, 7.0238];

    // ===== Adapter K1 =====

    fn kurz(t: &LayerSet) -> &str {
        &t.code
    }
    fn aussen(t: &LayerSet) -> bool {
        matches!(t.category, TypeCategory::ExteriorWall)
    }
    fn stand(t: &LayerSet) -> u32 {
        t.changed
    }
    fn typ_merkmale(t: &LayerSet) -> &PropSet {
        &t.props
    }
    fn typ_merkmale_mut(t: &mut LayerSet) -> &mut PropSet {
        &mut t.props
    }

    /// Neuer Typ (noch nicht im Modell).
    fn neuer_typ(
        m: &mut Model,
        name: &str,
        code: &str,
        ist_aussen: bool,
        layers: Vec<MaterialLayer>,
    ) -> LayerSet {
        LayerSet {
            guid: m.new_guid(),
            name: name.into(),
            code: code.into(),
            category: if ist_aussen {
                TypeCategory::ExteriorWall
            } else {
                TypeCategory::InteriorWall
            },
            layers,
            props: PropSet::new(),
            note: String::new(),
            changed: 1,
            bearing: sk_model::Bearing::Core,
        }
    }

    /// `add_layer_set` prüft das Kurzzeichen; `None` bei doppelt oder leer.
    fn typ_anlegen(m: &mut Model, t: LayerSet) -> Option<LayerSetId> {
        m.add_layer_set(t)
    }
    fn benutzer(m: &Model, id: LayerSetId) -> usize {
        m.type_users(id).len()
    }
    fn duplizieren(m: &mut Model, id: LayerSetId) -> LayerSetId {
        m.duplicate_type(id).expect("kopiert")
    }
    fn loeschen(m: &mut Model, id: LayerSetId) -> Result<(), usize> {
        m.remove_type(id)
    }
    fn als_standard(m: &mut Model, ist_aussen: bool, id: LayerSetId) {
        let cat = if ist_aussen {
            TypeCategory::ExteriorWall
        } else {
            TypeCategory::InteriorWall
        };
        m.set_default_type(cat, id);
    }
    /// Typ eines Wandzugs wechseln, ein Rückgängig-Schritt „Wandtyp geändert“.
    fn zugtyp(s: &mut Scene, run: RunId, id: LayerSetId) -> bool {
        s.edit_model("Wandtyp geändert", |m| m.set_run_type(run, id))
    }
    fn merkmale(m: &Model, el: ElementId) -> PropSet {
        m.props_of(el)
    }
    /// Kopie des App-Modells, die sich ohne Rückgängig-Schritt ändern lässt.
    fn kopie(m: &Model) -> Model {
        let mut m = m.clone();
        m.allow_unstepped();
        m
    }

    // ===== Adapter K2 =====

    fn leere_bibliothek() -> Library {
        Library::default()
    }
    fn szk_schreiben(l: &Library) -> String {
        szk::write_szk(l)
    }
    fn szk_lesen(t: &str) -> Result<Library, String> {
        szk::read_szk(t).map_err(|e| e.to_string())
    }
    fn abgleich(m: &Model, l: &Library) -> BTreeMap<Guid, TypeState> {
        szk::compare(m, l).into_iter().collect()
    }
    /// Projekt ← Firma, ein Rückgängig-Schritt „Typ übernommen“.
    fn uebernehmen(s: &mut Scene, l: &Library, g: Guid) -> Option<LayerSetId> {
        let mut r = None;
        s.edit_model("Typ übernommen", |m| {
            r = szk::import_type(m, l, g);
            r.is_some()
        });
        r
    }
    /// Projekt → Firma (nur die Bibliothek im Speicher).
    fn zurueck(m: &Model, l: &mut Library, g: Guid) -> bool {
        szk::export_type(m, l, g)
    }
    /// Standardtyp der Bibliothek für neue Projekte (`[default]`).
    fn bib_standard(l: &mut Library, ist_aussen: bool, g: Guid) {
        if ist_aussen {
            l.default_exterior = l.type_by_guid(g);
        } else {
            l.default_interior = l.type_by_guid(g);
        }
    }
    /// Neues Projekt aus dem Firmenkatalog (ohne Rückgängig).
    fn neues_projekt(l: &Library) -> Model {
        Model::from_library(l)
    }

    // ===== Adapter Firmenkatalog als Datei (App) =====

    /// Lädt den Firmenkatalog; `vorgabe`: Pfad ist der Vorgabeort
    /// (%APPDATA%\Skizzeo\firmenkatalog.szk). Liefert Hinweise statt Fehler.
    fn firma_laden(p: &std::path::Path, vorgabe: bool) -> (crate::catalog::Company, Vec<String>) {
        crate::catalog::Company::load(p, vorgabe)
    }
    fn firma_bibliothek(c: &crate::catalog::Company) -> &Library {
        c.library()
    }
    /// Zurückspeichern in die Datei. `true`: geschrieben; `false`: die Datei
    /// wurde inzwischen von einem anderen geändert, die Rückfrage ist fällig.
    fn firma_zurueck(c: &mut crate::catalog::Company, m: &Model, g: Guid) -> bool {
        matches!(c.save_type(m, g), crate::catalog::SaveResult::Saved)
    }

    // ===== Hilfen (keine Annahmen) =====

    fn lesen(text: &str) -> Result<Model, String> {
        sk_model::szo::read(text, GuidGen::with_seed(1))
            .map(|l| l.model)
            .map_err(|e| e.to_string())
    }

    /// Alles, was beim Laden anschlägt: Ladefehler, Hinweise, Prüfregeln.
    fn pruefung(text: &str) -> Vec<String> {
        match sk_model::szo::read(text, GuidGen::with_seed(1)) {
            Err(e) => vec![e.to_string()],
            Ok(l) => {
                let mut v = l.model.check();
                v.extend(l.hints);
                v
            }
        }
    }

    fn typ_nach_guid(m: &Model, g: Guid) -> Option<LayerSetId> {
        m.layer_sets()
            .iter()
            .find(|(_, t)| t.guid == g)
            .map(|(id, _)| id)
    }

    /// (Kurzzeichen, außen, Name, Stand, Merkmale leer), nach Kurzzeichen.
    fn typen(m: &Model) -> Vec<(String, bool, String, u32, bool)> {
        let mut v: Vec<_> = m
            .layer_sets()
            .iter()
            .map(|(_, t)| {
                (
                    kurz(t).to_string(),
                    aussen(t),
                    t.name.clone(),
                    stand(t),
                    typ_merkmale(t).is_empty(),
                )
            })
            .collect();
        v.sort();
        v
    }

    fn kurzzeichen(m: &Model) -> Vec<String> {
        typen(m).into_iter().map(|t| t.0).collect()
    }

    /// Außenwandzug im EG und Innenwandzug eines geladenen Prüfhauses.
    fn zuege(s: &Scene) -> (RunId, RunId) {
        let m = s.model();
        let cat = |r: RunId| m.element(m.wall_at(r, 0).unwrap()).unwrap().category;
        let aw = m
            .runs()
            .ids()
            .find(|r| cat(*r) == Category::ExteriorWall && m.run_below(*r).is_none())
            .expect("EG-Außenwandzug");
        let iw = m
            .runs()
            .ids()
            .find(|r| cat(*r) == Category::InteriorWall)
            .expect("Innenwandzug");
        (aw, iw)
    }

    /// Typen aller Segmente eines Zugs.
    fn zug_typen(s: &Scene, run: RunId) -> Vec<Option<LayerSetId>> {
        let m = s.model();
        (0..)
            .map_while(|i| m.wall_at(run, i))
            .map(|w| m.element(w).unwrap().layer_set)
            .collect()
    }

    /// Kopie des Typs mit neuen Dicken (Dämmung, Gasbeton) in mm.
    fn umbau(m: &Model, id: LayerSetId, d0: f64, d1: f64) -> LayerSet {
        let mut t = m.layer_set(id).unwrap().clone();
        t.layers[0].thickness = d0;
        t.layers[1].thickness = d1;
        t
    }

    fn text(v: &str) -> PropValue {
        PropValue::Text(v.into())
    }

    /// Jedes Bauteil ist an ein vorhandenes Geschoss gebunden (A-09).
    fn alle_gebunden(m: &Model) -> bool {
        m.elements()
            .iter()
            .all(|(_, e)| m.storey(e.storey).is_some())
    }

    // ===== Tests K1 =====

    /// A102 (K1, .szo v4): Eine Datei der Version 3 lädt. Ihre Typen heißen
    /// danach AW-31,5 (Außenwand) und IW-17,5 (Innenwand), Kurzzeichen aus
    /// Kategorie und Dicke, alte Guids bleiben, Stand 1, ohne Merkmale,
    /// Guids unverändert. Speichern schreibt Version 4 mit Kurzzeichen und
    /// Kategorie; Laden und erneutes Speichern ergeben denselben Text. Die
    /// Mengen bleiben, alle Bauteile sind an Geschosse gebunden.
    #[test]
    fn a102_alte_datei_wird_version_4() {
        assert_eq!(sk_model::szo::VERSION, 4);
        let m = lesen(HAUS_V3).expect("v3 lädt");
        assert_eq!(
            typen(&m),
            [
                (
                    "AW-31,5".into(),
                    true,
                    "AW 31,5 Gasbeton + WDVS".into(),
                    1,
                    true
                ),
                ("IW-17,5".into(), false, "IW 17,5 Gasbeton".into(), 1, true),
            ]
        );
        let d = m.defaults();
        assert_eq!(
            m.layer_set(d.exterior_wall).unwrap().guid.to_string(),
            AW1_GUID
        );
        assert_eq!(
            m.layer_set(d.interior_wall).unwrap().guid.to_string(),
            IW1_GUID
        );
        assert!(m.check().is_empty(), "{:?}", m.check());
        assert!(alle_gebunden(&m));
        let t = sk_model::szo::write(&m);
        assert!(t.starts_with("SZO 4\n"), "{}", &t[..20]);
        assert!(t.contains(r#"code="AW-31,5""#) && t.contains(r#"code="IW-17,5""#));
        assert!(t.contains("cat=exterior") && t.contains("cat=interior"));
        let m2 = lesen(&t).unwrap();
        assert_eq!(sk_model::szo::write(&m2), t, "Rundlauf stabil");
        let s = Scene::with_model(m);
        let (aw, iw) = zuege(&s);
        assert_eq!(mengen_b11(&s, aw, iw), STANDARD, "Mengen unverändert");
    }

    /// A103 (K1, F2 „Typänderung wirkt auf alle Wände“): AW-31,5 über
    /// set_layer_set auf 12 Dämmung + 24 Gasbeton. Alle acht Außenwände (EG
    /// und OG) folgen, die Decke wird größer (Kernaußenseite 2 cm weiter
    /// außen), Sohlplatte und Frostschürze bleiben (Außenseite steht).
    /// Höhenbezug bleibt (A-09). Stand +1, Guid bleibt. Ein Rückgängig stellt
    /// alles wieder her.
    #[test]
    fn a103_typaenderung_wirkt_auf_alle_waende() {
        let mut s = Scene::with_model(Model::with_seed(103));
        let (aw, iw) = haus_b11(&mut s);
        let og = og_zug(&s, aw);
        let id = s.model().defaults().exterior_wall;
        let (g, st) = {
            let t = s.model().layer_set(id).unwrap();
            (t.guid, stand(t))
        };
        assert_eq!(mengen_b11(&s, aw, iw), STANDARD);
        let neu = umbau(s.model(), id, 120.0, 240.0);
        assert!(s.edit_model("Typ geändert", |m| m.set_layer_set(id, neu)));
        assert_eq!(mengen_b11(&s, aw, iw), K1_36);
        assert_eq!(m2(s.floor_qto(aw).unwrap().area), 75.7376);
        assert_eq!(schale(&s, og), (21.5522, 12.1692), "OG folgt");
        assert_eq!(decke_hoehen(&s, aw), (2635.0, 2855.0), "Höhenbezug bleibt");
        assert_eq!(m2(s.foundation_qto(aw).unwrap().0.area), 80.0);
        assert!(s.model().check().is_empty(), "{:?}", s.model().check());
        let t = s.model().layer_set(id).unwrap();
        assert_eq!((t.guid, stand(t)), (g, st + 1), "Guid bleibt, Stand +1");
        assert!(s.undo());
        assert_eq!(mengen_b11(&s, aw, iw), STANDARD);
        assert_eq!(schale(&s, og), (15.7613, 14.1654));
        assert_eq!(stand(s.model().layer_set(id).unwrap()), st);
    }

    /// A104 (K1, set_run_type): Ein Zug hat genau einen Typ. Ein Außenwandzug
    /// nimmt keinen Innenwandtyp an (abgelehnt, kein Rückgängig-Schritt).
    /// Duplizieren gibt neue Guid, „(Kopie)“ und ein freies Kurzzeichen.
    /// Wechsel auf die Kopie mit 12 + 24 liefert dieselben Mengen wie A103,
    /// der gekoppelte OG-Zug wechselt mit, ein Rückgängig-Schritt.
    #[test]
    fn a104_typ_eines_wandzugs_wechseln() {
        let mut s = Scene::with_model(Model::with_seed(104));
        let (aw, iw) = haus_b11(&mut s);
        let og = og_zug(&s, aw);
        let aw1 = s.model().defaults().exterior_wall;
        let iw1 = s.model().defaults().interior_wall;
        // Abgelehnt: falsche Kategorie, kein leerer Schritt
        let label = s.undo_label();
        assert!(
            !zugtyp(&mut s, aw, iw1),
            "Außenwand nimmt keinen Innenwandtyp"
        );
        assert!(
            !zugtyp(&mut s, iw, aw1),
            "Innenwand nimmt keinen Außenwandtyp"
        );
        assert_eq!(s.undo_label(), label, "kein Rückgängig-Schritt");
        assert_eq!(mengen_b11(&s, aw, iw), STANDARD);
        // Duplizieren
        let mut dup = None;
        s.edit_model("Typ dupliziert", |m| {
            dup = Some(duplizieren(m, aw1));
            true
        });
        let dup = dup.unwrap();
        {
            let (a, d) = (
                s.model().layer_set(aw1).unwrap(),
                s.model().layer_set(dup).unwrap(),
            );
            assert_eq!(d.name, "AW 31,5 Gasbeton + WDVS (Kopie)");
            assert_ne!(d.guid, a.guid);
            assert!(!kurz(d).is_empty() && kurz(d) != "AW-31,5", "{}", kurz(d));
            assert!(aussen(d));
            assert_eq!(d.layers, a.layers);
        }
        assert_eq!(benutzer(s.model(), dup), 0);
        let neu = umbau(s.model(), dup, 120.0, 240.0);
        assert!(s.edit_model("Typ geändert", |m| m.set_layer_set(dup, neu)));
        assert_eq!(
            mengen_b11(&s, aw, iw),
            STANDARD,
            "unbenutzter Typ ändert nichts"
        );
        // Wechsel
        assert!(zugtyp(&mut s, aw, dup));
        assert_eq!(s.undo_label(), Some("Wandtyp geändert"));
        assert_eq!(mengen_b11(&s, aw, iw), K1_36);
        assert!(zug_typen(&s, aw).iter().all(|t| *t == Some(dup)));
        assert!(
            zug_typen(&s, og).iter().all(|t| *t == Some(dup)),
            "OG wechselt mit"
        );
        assert_eq!(schale(&s, og), (21.5522, 12.1692));
        assert_eq!((benutzer(s.model(), aw1), benutzer(s.model(), dup)), (0, 8));
        assert!(s.model().check().is_empty(), "{:?}", s.model().check());
        // Ein Schritt zurück
        assert!(s.undo());
        assert_eq!(mengen_b11(&s, aw, iw), STANDARD);
        assert!(zug_typen(&s, aw).iter().all(|t| *t == Some(aw1)));
        assert!(zug_typen(&s, og).iter().all(|t| *t == Some(aw1)));
    }

    /// A105 (K1, Prüfregeln 16–19): Löschen nur ohne Benutzer und nicht für
    /// Standardtypen (AW-31,5: Err(8), IW-17,5: Err(1)); Kurzzeichen eindeutig und
    /// nicht leer; Guid bleibt. Gezielt kaputte Dateien schlagen an: doppeltes
    /// oder leeres Kurzzeichen, Zug mit zwei Typen, Kategorie falsch, Schicht 0.
    #[test]
    fn a105_pruefregeln_16_bis_19() {
        let mut s = Scene::with_model(Model::with_seed(105));
        haus_b11(&mut s);
        let aw1 = s.model().defaults().exterior_wall;
        let iw1 = s.model().defaults().interior_wall;
        let aw1_layers = s.model().layer_set(aw1).unwrap().layers.clone();
        let mut m = kopie(s.model());
        // Regel 18
        assert_eq!(loeschen(&mut m, aw1), Err(8), "EG und OG je 4 Segmente");
        assert_eq!(loeschen(&mut m, iw1), Err(1));
        // Regel 17
        let t = neuer_typ(&mut m, "Test", "AW-31,5", true, aw1_layers.clone());
        assert!(typ_anlegen(&mut m, t).is_none(), "Kurzzeichen doppelt");
        let t = neuer_typ(&mut m, "Test", "", true, aw1_layers.clone());
        assert!(typ_anlegen(&mut m, t).is_none(), "Kurzzeichen leer");
        let t = neuer_typ(&mut m, "Test", "AW-9", true, aw1_layers.clone());
        let neu = typ_anlegen(&mut m, t).expect("frei");
        assert_eq!(loeschen(&mut m, neu), Ok(()), "unbenutzt");
        assert!(m.layer_set(neu).is_none());
        // Standardtyp ohne Benutzer bleibt
        let dup = duplizieren(&mut m, aw1);
        als_standard(&mut m, true, dup);
        assert!(loeschen(&mut m, dup).is_err(), "Standardtyp");
        als_standard(&mut m, true, aw1);
        assert_eq!(loeschen(&mut m, dup), Ok(()));
        // Regel 19: Guid bleibt bei Änderung und Rundlauf
        let g = m.layer_set(aw1).unwrap().guid;
        let t2 = umbau(&m, aw1, 120.0, 240.0);
        assert!(m.set_layer_set(aw1, t2));
        assert_eq!(m.layer_set(aw1).unwrap().guid, g);
        let m2 = lesen(&sk_model::szo::write(&m)).unwrap();
        assert!(typ_nach_guid(&m2, g).is_some());
        // Kaputte Dateien (Grundlage: sauberer Text mit einem zweiten AW-Typ)
        let mut m = kopie(s.model());
        let dup = duplizieren(&mut m, aw1);
        let (awg, iwg, dupg) = (
            m.layer_set(aw1).unwrap().guid.to_string(),
            m.layer_set(iw1).unwrap().guid.to_string(),
            m.layer_set(dup).unwrap().guid.to_string(),
        );
        let sauber = sk_model::szo::write(&m);
        assert!(pruefung(&sauber).is_empty(), "{:?}", pruefung(&sauber));
        let zeile = |text: &str, wo: &dyn Fn(&str) -> bool, alt: &str, neu: &str| -> String {
            let mut erst = true;
            let mut out: Vec<String> = Vec::new();
            for l in text.lines() {
                if erst && wo(l) && l.contains(alt) {
                    erst = false;
                    out.push(l.replacen(alt, neu, 1));
                } else {
                    out.push(l.to_string());
                }
            }
            assert!(!erst, "Stelle für {alt} nicht gefunden");
            out.join("\n") + "\n"
        };
        let iw_satz = format!("guid={iwg}");
        let aw_satz = format!("set={awg}");
        let kaputt = [
            (
                "Kurzzeichen doppelt",
                sauber.replacen(r#"code="IW-17,5""#, r#"code="AW-31,5""#, 1),
            ),
            (
                "Kurzzeichen leer",
                sauber.replacen(r#"code="IW-17,5""#, r#"code="""#, 1),
            ),
            (
                "Kategorie falsch",
                zeile(
                    &sauber,
                    &|l| l.starts_with("[layerset]") && l.contains(&iw_satz),
                    "cat=interior",
                    "cat=exterior",
                ),
            ),
            (
                "Zug mit zwei Typen",
                zeile(
                    &sauber,
                    &|l| l.starts_with("[wall]"),
                    &aw_satz,
                    &format!("set={dupg}"),
                ),
            ),
            (
                "Schicht 0 dick",
                zeile(
                    &sauber,
                    &|l| l.starts_with("[layer]") && l.contains(&aw_satz),
                    "t=140",
                    "t=0",
                ),
            ),
        ];
        for (name, t) in kaputt {
            assert_ne!(t, sauber, "{name}: Text verändert");
            assert!(!pruefung(&t).is_empty(), "{name} schlägt nicht an");
        }
    }

    /// A106 (K1, Merkmale): Typmerkmale gelten für alle Wände des Typs; ein
    /// Bauteilmerkmal mit gleichem Schlüssel überschreibt (IFC-Regel). Außenwand,
    /// tragend und Dicke sind abgeleitet und nie gespeichert. Merkmale stehen als
    /// [typeprop] in der .szo und überstehen den Rundlauf.
    #[test]
    fn a106_merkmale_typ_und_bauteil() {
        let mut s = Scene::with_model(Model::with_seed(106));
        let (aw, _iw) = haus_b11(&mut s);
        let aw1 = s.model().defaults().exterior_wall;
        let (w0, w1) = (
            s.model().wall_at(aw, 0).unwrap(),
            s.model().wall_at(aw, 1).unwrap(),
        );
        let mut t = s.model().layer_set(aw1).unwrap().clone();
        typ_merkmale_mut(&mut t).insert("Brandschutz".into(), text("F90"));
        typ_merkmale_mut(&mut t).insert("Schallschutz".into(), text("R'w 53 dB"));
        assert!(s.edit_model("Merkmale", |m| m.set_layer_set(aw1, t)));
        assert!(s.edit_model("Merkmal", |m| m.set_prop(
            w0,
            "Brandschutz",
            Some(text("F30"))
        )));
        let p0 = merkmale(s.model(), w0);
        let p1 = merkmale(s.model(), w1);
        assert_eq!(
            p0.get("Brandschutz"),
            Some(&text("F30")),
            "Bauteil überschreibt"
        );
        assert_eq!(
            p0.get("Schallschutz"),
            Some(&text("R'w 53 dB")),
            "Typ kommt durch"
        );
        assert_eq!(p1.get("Brandschutz"), Some(&text("F90")));
        let tp = typ_merkmale(s.model().layer_set(aw1).unwrap());
        for k in ["Dicke", "Tragend", "Außenwand", "IsExternal", "LoadBearing"] {
            assert!(!tp.contains_key(k), "{k} ist abgeleitet");
        }
        let t = sk_model::szo::write(s.model());
        assert!(t.contains("[typeprop]") && t.contains(r#"key="Brandschutz""#));
        let m2 = lesen(&t).unwrap();
        assert_eq!(sk_model::szo::write(&m2), t);
        let id2 = typ_nach_guid(&m2, s.model().layer_set(aw1).unwrap().guid).unwrap();
        assert_eq!(typ_merkmale(m2.layer_set(id2).unwrap()), tp);
        assert!(s.edit_model("Merkmal", |m| m.set_prop(w0, "Brandschutz", None)));
        assert_eq!(
            merkmale(s.model(), w0).get("Brandschutz"),
            Some(&text("F90")),
            "ohne Bauteilwert gilt der Typ"
        );
    }

    // ===== Tests K2 =====

    /// A107 (K2, Abgleich): Ein im Firmenkatalog geänderter AW-31,5 (12 + 24)
    /// gilt als „abweichend“, IW-17,5 als „nur Projekt“. Rundlauf
    /// read_szk(write_szk(x)) == x, Kopf „SZK 1“, keine Geschosse oder Bauteile.
    /// Übernehmen überschreibt den Projekttyp (gleiche Guid, kein neuer Typ),
    /// die Mengen werden die aus A103, danach „gleich“, obwohl der
    /// Änderungsstand verschieden ist. Ein Rückgängig-Schritt.
    #[test]
    fn a107_abweichenden_typ_uebernehmen() {
        let mut s = Scene::with_model(Model::with_seed(107));
        let (aw, iw) = haus_b11(&mut s);
        let aw1 = s.model().defaults().exterior_wall;
        let g = s.model().layer_set(aw1).unwrap().guid;
        let gi = s
            .model()
            .layer_set(s.model().defaults().interior_wall)
            .unwrap()
            .guid;
        // Firma: derselbe Typ, zweimal geändert (Stand 3)
        let mut b = lesen(&sk_model::szo::write(s.model())).unwrap();
        let bid = typ_nach_guid(&b, g).unwrap();
        let t1 = umbau(&b, bid, 130.0, 230.0);
        assert!(b.set_layer_set(bid, t1));
        let t2 = umbau(&b, bid, 120.0, 240.0);
        assert!(b.set_layer_set(bid, t2));
        let mut lib = leere_bibliothek();
        assert!(zurueck(&b, &mut lib, g));
        let t = szk_schreiben(&lib);
        assert!(t.starts_with("SZK 1\n"));
        for satz in ["[wall]", "[storey]", "[building]", "[project]"] {
            assert!(!t.contains(satz), "{satz} gehört nicht in die .szk");
        }
        assert_eq!(szk_lesen(&t).unwrap(), lib, "Rundlauf");
        let a = abgleich(s.model(), &lib);
        assert_eq!(a.get(&g), Some(&TypeState::Differs));
        assert_eq!(a.get(&gi), Some(&TypeState::OnlyProject));
        let n = s.model().layer_sets().len();
        assert_eq!(
            uebernehmen(&mut s, &lib, g),
            Some(aw1),
            "überschreibt den Projekttyp"
        );
        assert_eq!(s.undo_label(), Some("Typ übernommen"));
        assert_eq!(s.model().layer_sets().len(), n);
        assert_eq!(s.model().layer_set(aw1).unwrap().guid, g, "Regel 19");
        assert_eq!(mengen_b11(&s, aw, iw), K1_36);
        assert!(s.model().check().is_empty(), "{:?}", s.model().check());
        assert_ne!(stand(s.model().layer_set(aw1).unwrap()), 3);
        assert_eq!(
            abgleich(s.model(), &lib).get(&g),
            Some(&TypeState::Same),
            "Stand zählt nicht"
        );
        assert!(s.undo());
        assert_eq!(mengen_b11(&s, aw, iw), STANDARD);
        assert_eq!(abgleich(s.model(), &lib).get(&g), Some(&TypeState::Differs));
    }

    /// A108 (K2, neuer Typ): Ein Typ, den es nur im Firmenkatalog gibt („nur
    /// Firma“), kommt mit derselben Guid ins Projekt, mit seinem neuen Baustoff
    /// und dessen neuem Stift. Der Stift bekommt die nächste freie Nummer (11,
    /// weil das Projekt schon einen eigenen Stift 10 hat); keine Nummer doppelt,
    /// alte Nummern unverändert. Vorhandene Baustoffe bleiben die des Projekts.
    /// Belegt ein anderer Projekttyp das Kurzzeichen, bekommt der übernommene
    /// „AW-37-2“ (seit K4 gibt es den Werkstyp AW-36). Ein Rückgängig entfernt Typ, Baustoff und Stift.
    #[test]
    fn a108_neuen_typ_mit_baustoff_und_stift_uebernehmen() {
        let mut s = Scene::with_model(Model::with_seed(108));
        haus_b11(&mut s);
        let aw1 = s.model().defaults().exterior_wall;
        let aw1_layers = s.model().layer_set(aw1).unwrap().layers.clone();
        // Firma: neuer Stift, neuer Baustoff, neuer Typ AW-37
        let mut b = lesen(&sk_model::szo::write(s.model())).unwrap();
        let pen_g = b.new_guid();
        let pen = b.add_pen(Pen {
            guid: pen_g,
            number: 10,
            name: "Kalksandstein".into(),
            color: [200, 90, 60],
            width_mm: 0.25,
        });
        let mat = |m: &Model, name: &str| {
            m.materials()
                .iter()
                .find(|(_, x)| x.name == name)
                .map(|(id, _)| id)
                .unwrap()
        };
        let mut ks = b.material(mat(&b, "Gasbeton")).unwrap().clone();
        ks.guid = b.new_guid();
        ks.name = "Kalksandstein".into();
        ks.density = 1800.0;
        ks.cut_fg = pen;
        let ks_g = ks.guid;
        let ks_id = b.add_material(ks);
        let daemm = mat(&b, "Dämmung (WDVS)");
        let schicht = |material, thickness, function, core| MaterialLayer {
            material,
            thickness,
            function,
            core,
        };
        let t = neuer_typ(
            &mut b,
            "AW 36 KS + WDVS",
            "AW-37",
            true,
            vec![
                schicht(daemm, 160.0, LayerFunction::Insulation, false),
                schicht(ks_id, 200.0, LayerFunction::Structure, true),
            ],
        );
        let tg = t.guid;
        typ_anlegen(&mut b, t).unwrap();
        let mut lib = leere_bibliothek();
        assert!(zurueck(&b, &mut lib, tg));
        assert_eq!(
            abgleich(s.model(), &lib).get(&tg),
            Some(&TypeState::OnlyCompany)
        );
        // Projekt: eigener Stift 10, eigener Typ mit Kurzzeichen AW-37
        s.edit_model("Stift", |m| {
            let guid = m.new_guid();
            m.add_pen(Pen {
                guid,
                number: 10,
                name: "Eigener".into(),
                color: [0, 0, 0],
                width_mm: 0.18,
            });
            true
        });
        s.edit_model("Typ", |m| {
            let t = neuer_typ(m, "AW Test", "AW-37", true, aw1_layers.clone());
            typ_anlegen(m, t).is_some()
        });
        let stifte = |s: &Scene| -> BTreeMap<Guid, u16> {
            s.model()
                .attr()
                .pens()
                .iter()
                .map(|(_, p)| (p.guid, p.number))
                .collect()
        };
        let vorher = stifte(&s);
        let daemm_vorher = s
            .model()
            .material(mat(s.model(), "Dämmung (WDVS)"))
            .unwrap()
            .clone();
        let (n_typen, n_mat) = (s.model().layer_sets().len(), s.model().materials().len());
        // Übernehmen
        let id = uebernehmen(&mut s, &lib, tg).expect("übernommen");
        let t = s.model().layer_set(id).unwrap().clone();
        assert_eq!(t.guid, tg, "gleiche Guid");
        assert_eq!(kurz(&t), "AW-37-2", "Kurzzeichen belegt");
        assert_eq!(t.name, "AW 36 KS + WDVS");
        assert_eq!(
            t.layers.iter().map(|l| l.thickness).collect::<Vec<_>>(),
            [160.0, 200.0]
        );
        let (ks_proj, ks_mat) = s
            .model()
            .materials()
            .iter()
            .find(|(_, x)| x.guid == ks_g)
            .map(|(id, x)| (id, x.clone()))
            .expect("Baustoff reist mit");
        assert_eq!(t.layers[1].material, ks_proj);
        let p = s
            .model()
            .attr()
            .pen(ks_mat.cut_fg)
            .expect("Stift reist mit");
        assert_eq!((p.guid, p.number), (pen_g, 11), "nächste freie Nummer");
        let nachher = stifte(&s);
        for (g, n) in &vorher {
            assert_eq!(nachher.get(g), Some(n), "alte Stiftnummern bleiben");
        }
        let mut nummern: Vec<u16> = nachher.values().copied().collect();
        let anzahl = nummern.len();
        nummern.sort();
        nummern.dedup();
        assert_eq!(nummern.len(), anzahl, "keine Stiftnummer doppelt");
        assert_eq!(
            s.model()
                .material(mat(s.model(), "Dämmung (WDVS)"))
                .unwrap(),
            &daemm_vorher,
            "Projektbaustoff bleibt"
        );
        assert_eq!(
            s.model().materials().len(),
            n_mat + 1,
            "nur der fehlende Baustoff"
        );
        assert!(s.model().check().is_empty(), "{:?}", s.model().check());
        assert!(s.undo());
        assert_eq!(s.model().layer_sets().len(), n_typen);
        assert_eq!(s.model().materials().len(), n_mat);
        assert_eq!(stifte(&s), vorher);
    }

    /// A109 (K2, Firmenkatalog als Datei): Fehlt die Datei am Vorgabeort, wird
    /// sie mit dem Startbestand angelegt; an einem fremden Pfad wird nichts
    /// angelegt, nur ein Hinweis. Eine kaputte .szk gibt einen Hinweis mit
    /// Zeile, Skizzeo arbeitet mit dem Startbestand, die Datei bleibt unberührt.
    /// Hat ein anderer die Datei seit dem Laden geändert, schreibt
    /// Zurückspeichern nicht still darüber (Rückfrage). Sonst wird atomar
    /// geschrieben. Ein neues Projekt bekommt Typen und Standardtypen des
    /// Firmenkatalogs; Model::with_seed bleibt beim Startbestand.
    #[test]
    fn a109_firmenkatalog_datei_und_neues_projekt() {
        // Startbestand seit K4: sieben Werkstypen (K5)
        let start = [
            "AW-31,5", "AW-36", "AW-36,5", "AW-49", "IW-11,5", "IW-17,5", "IW-24",
        ]
        .map(String::from);
        let d = test_dir("a109");
        // Erster Start am Vorgabeort
        let vorgabe = d.join("firmenkatalog.szk");
        let (c, h) = firma_laden(&vorgabe, true);
        assert!(vorgabe.exists(), "angelegt");
        assert!(h.is_empty(), "{h:?}");
        assert_eq!(kurzzeichen(&neues_projekt(firma_bibliothek(&c))), start);
        // Fremder Pfad
        let fremd = d.join("netz").join("firma.szk");
        let (c, h) = firma_laden(&fremd, false);
        assert!(!fremd.exists(), "nichts angelegt");
        assert!(!h.is_empty(), "Hinweis");
        assert_eq!(kurzzeichen(&neues_projekt(firma_bibliothek(&c))), start);
        // Kaputte Datei
        let kaputt = d.join("kaputt.szk");
        let inhalt = "SZK 1\n[layerset] guid=@@@ name=\"X\" code=\"AW-31,5\" cat=exterior changed=1 note=\"\"\n[layer] set=@@@ mat=nix t=abc fn=insulation core=0\n";
        std::fs::write(&kaputt, inhalt).unwrap();
        let (c, h) = firma_laden(&kaputt, false);
        assert!(h.iter().any(|x| x.contains("Zeile")), "{h:?}");
        assert_eq!(kurzzeichen(&neues_projekt(firma_bibliothek(&c))), start);
        assert_eq!(
            std::fs::read_to_string(&kaputt).unwrap(),
            inhalt,
            "unberührt"
        );
        // Zurückspeichern
        let mut s = Scene::with_model(Model::with_seed(109));
        haus_b11(&mut s);
        let aw1 = s.model().defaults().exterior_wall;
        let g = s.model().layer_set(aw1).unwrap().guid;
        let neu = umbau(s.model(), aw1, 120.0, 240.0);
        assert!(s.edit_model("Typ geändert", |m| m.set_layer_set(aw1, neu)));
        let mut c = firma_laden(&vorgabe, true).0;
        let mut anderer = leere_bibliothek();
        assert!(zurueck(
            s.model(),
            &mut anderer,
            s.model()
                .layer_set(s.model().defaults().interior_wall)
                .unwrap()
                .guid
        ));
        std::fs::write(&vorgabe, szk_schreiben(&anderer)).unwrap();
        std::fs::File::options()
            .write(true)
            .open(&vorgabe)
            .unwrap()
            .set_modified(std::time::SystemTime::now() + std::time::Duration::from_secs(10))
            .unwrap();
        let fremdtext = std::fs::read_to_string(&vorgabe).unwrap();
        assert!(
            !firma_zurueck(&mut c, s.model(), g),
            "Rückfrage statt Überschreiben"
        );
        assert_eq!(std::fs::read_to_string(&vorgabe).unwrap(), fremdtext);
        let mut c = firma_laden(&vorgabe, true).0;
        assert!(
            firma_zurueck(&mut c, s.model(), g),
            "ohne fremde Änderung geschrieben"
        );
        let lib = szk_lesen(&std::fs::read_to_string(&vorgabe).unwrap()).unwrap();
        assert_eq!(abgleich(s.model(), &lib).get(&g), Some(&TypeState::Same));
        let reste: Vec<_> = std::fs::read_dir(&d)
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| n.contains("tmp") || n.ends_with('~'))
            .collect();
        assert!(reste.is_empty(), "atomar, keine Reste: {reste:?}");
        // Neues Projekt
        let mut b = lesen(&sk_model::szo::write(s.model())).unwrap();
        let bid = typ_nach_guid(&b, g).unwrap();
        let dup = duplizieren(&mut b, bid);
        let dg = b.layer_set(dup).unwrap().guid;
        let gi = b.layer_set(b.defaults().interior_wall).unwrap().guid;
        let mut lib = leere_bibliothek();
        assert!(zurueck(&b, &mut lib, dg));
        assert!(zurueck(&b, &mut lib, gi));
        bib_standard(&mut lib, true, dg);
        bib_standard(&mut lib, false, gi);
        let p = neues_projekt(&lib);
        let a = abgleich(&p, &lib);
        assert_eq!(a.len(), 2);
        assert!(a.values().all(|x| *x == TypeState::Same), "{a:?}");
        assert_eq!(p.layer_set(p.defaults().exterior_wall).unwrap().guid, dg);
        assert_eq!(p.layer_set(p.defaults().interior_wall).unwrap().guid, gi);
        assert!(p.check().is_empty(), "{:?}", p.check());
        assert_eq!(
            kurzzeichen(&Model::with_seed(1)),
            start,
            "Tests ohne Firmenkatalog"
        );
        // Nachtrag 16:55: Werkstypen mit festen Guids, in jedem Projekt gleich
        let werk = |m: &Model| {
            let d = m.defaults();
            (
                m.layer_set(d.exterior_wall).unwrap().guid,
                m.layer_set(d.interior_wall).unwrap().guid,
            )
        };
        assert_eq!(werk(&Model::with_seed(1)), werk(&Model::with_seed(2)));
        let neu = d.join("neu");
        std::fs::create_dir_all(&neu).unwrap();
        let c = firma_laden(&neu.join("firmenkatalog.szk"), true).0;
        // Seit K4 hat ein neues Projekt (Model::new) den Standard AW-36
        assert_eq!(
            werk(&Model::new()),
            werk(&neues_projekt(firma_bibliothek(&c))),
            "Startbestand des Firmenkatalogs = Werkstypen"
        );
    }
}

/// Abnahme K4 und K5 (Jörns Wandtypen, Randdämmstreifen).
mod wandtypen {
    use super::*;
    // Abnahmetests K4 (Jörns Wandtypen, Luftschicht, Schürzenbreite 30–45 cm)
    // und K5 (monolithische Wand mit Randdämmstreifen), vorbereitet gegen main
    // 137fca7. Paket: bim/paket-k4-k5-wandtypen.md („Fertig, wenn“),
    // Spezifikation test/abnahme-wandtypen.md.
    //
    // Einbau: als `mod wandtypen { use super::*; … }` ans Ende von
    // app/src/abnahme.rs. Nutzt aus abnahme.rs: haus_b11, og_zug, wall_m3,
    // decke_mengen, sohlplatte, tippe, m2, m3, cam3d; aus mod katalog nichts.
    //
    // Wichtig: Die Tests wechseln die Außenwand ausdrücklich auf den Werkstyp
    // (set_run_type) und hängen nicht davon ab, welcher Typ in
    // Model::with_seed Standard ist. Den neuen Standard AW-36 prüft A110 an
    // Model::new().
    //
    // Angenommene Namen stehen NUR in den Adaptern. Weicht der Bau ab, bitte
    // nur die Adapter anpassen.

    use sk_model::{
        Bearing, Category, Element, ElementId, ElementKind, GuidGen, LayerFunction, LayerSet,
        LayerSetId, MatCategory, Model,
    };

    // ===== Adapter =====

    /// Werkstyp nach Kurzzeichen.
    fn typ(m: &Model, code: &str) -> LayerSetId {
        m.layer_sets()
            .iter()
            .find(|(_, t)| t.code == code)
            .map(|(id, _)| id)
            .unwrap_or_else(|| panic!("Typ {code} fehlt"))
    }
    /// Typ eines Wandzugs wechseln (K1), ein Rückgängig-Schritt.
    fn zugtyp(s: &mut Scene, run: RunId, id: LayerSetId) -> bool {
        s.edit_model("Wandtyp geändert", |m| m.set_run_type(run, id))
    }
    /// U-Wert eines Typs (nur Anzeige); `None` bei Innenwand oder fehlendem λ.
    fn u_wert(m: &Model, id: LayerSetId) -> Option<f64> {
        m.u_value(id)
    }
    /// Deckenauflager des Typs: `None` = ganzer Kern, sonst (Tiefe mm, Baustoff).
    fn auflager(t: &LayerSet) -> Option<(f64, sk_model::MaterialId)> {
        match t.bearing {
            Bearing::Core => None,
            Bearing::Depth { depth, strip } => Some((depth, strip)),
        }
    }
    fn setze_auflager(t: &mut LayerSet, tiefe: Option<(f64, sk_model::MaterialId)>) {
        t.bearing = match tiefe {
            None => Bearing::Core,
            Some((depth, strip)) => Bearing::Depth { depth, strip },
        };
    }
    /// Länge auf der Streifenachse (mm) und Volumen (mm³) eines Randdämmstreifens.
    fn streifen_qto(s: &Scene, el: ElementId) -> (f64, f64) {
        let q = s.edge_strip_qto(el).expect("Streifenmengen");
        (q.length, q.volume)
    }
    /// (Wand, Decke) eines Randdämmstreifens.
    fn streifen_von(m: &Model, el: ElementId) -> Option<(ElementId, ElementId)> {
        match m.element(el)?.kind {
            ElementKind::EdgeStrip { wall, floor } => Some((wall, floor)),
            _ => None,
        }
    }
    /// Bauteil ist ein Randdämmstreifen.
    fn ist_streifen(e: &Element) -> bool {
        e.category == Category::EdgeInsulation
    }
    /// Baustoffkategorie Luft.
    fn ist_luft(c: MatCategory) -> bool {
        c == MatCategory::Air
    }
    /// Firmenkatalog laden (App, K2): Hinweise statt Fehler.
    fn firma_laden(p: &std::path::Path, vorgabe: bool) -> (crate::catalog::Company, Vec<String>) {
        crate::catalog::Company::load(p, vorgabe)
    }

    // ===== Hilfen (keine Annahmen über K4/K5 hinaus) =====

    fn r4(v: f64) -> f64 {
        (v * 1e4).round() / 1e4
    }

    /// Wechselt den Zug auf den Werkstyp, falls er ihn noch nicht hat.
    fn auf_typ(s: &mut Scene, run: RunId, code: &str) {
        let id = typ(s.model(), code);
        let hat = s
            .model()
            .element(s.model().wall_at(run, 0).unwrap())
            .unwrap()
            .layer_set;
        if hat != Some(id) {
            assert!(zugtyp(s, run, id), "Wechsel auf {code}");
        }
    }

    /// Volumen (m³) aller Schichten eines Baustoffs in den 4 Segmenten des Zugs.
    fn baustoff_m3(s: &Scene, run: RunId, name: &str) -> f64 {
        let m = s.model();
        let mut v = 0.0;
        for i in 0..4 {
            let q = s.wall_qto(m.wall_at(run, i).unwrap()).unwrap();
            for l in &q.layers {
                if m.material(l.material).unwrap().name == name {
                    v += l.volume;
                }
            }
        }
        r4(v / 1e9)
    }

    /// Summe der Wandvolumen (m³) der 4 Segmente.
    fn wand_m3(s: &Scene, run: RunId) -> f64 {
        let m = s.model();
        r4((0..4)
            .map(|i| s.wall_qto(m.wall_at(run, i).unwrap()).unwrap().volume)
            .sum::<f64>()
            / 1e9)
    }

    fn fs_m3(s: &Scene, aw: RunId) -> f64 {
        m3(s.foundation_qto(aw).unwrap().1.volume)
    }
    fn sp_m3(s: &Scene, aw: RunId) -> f64 {
        m3(s.foundation_qto(aw).unwrap().0.volume)
    }

    /// Randdämmstreifen im Modell: (Nummer, Element), nach Nummer.
    fn streifen(s: &Scene) -> Vec<(String, ElementId)> {
        let mut v: Vec<_> = s
            .model()
            .elements()
            .iter()
            .filter(|(_, e)| ist_streifen(e))
            .map(|(id, e)| (e.number.clone(), id))
            .collect();
        v.sort_by(|a, b| a.0.cmp(&b.0));
        v
    }

    fn pruefung(text: &str) -> Vec<String> {
        match sk_model::szo::read(text, GuidGen::with_seed(1)) {
            Err(e) => vec![e.to_string()],
            Ok(l) => {
                let mut v = l.model.check();
                v.extend(l.hints);
                v
            }
        }
    }

    // ===== Tests K4 =====

    /// A110 (K4, Startbestand): Werkstypen AW-36, AW-49, IW-11,5, IW-17,5,
    /// IW-24 mit Schichten außen → innen, AW-31,5 bleibt. Ein neues Projekt hat
    /// den Standard AW-36 / IW-17,5. Feste Guids (gleich in jedem Projekt), die
    /// Guids aus 137fca7 bleiben. Neue Baustoffe mit λ, Luft in eigener
    /// Kategorie. U-Wert nur für Außenwände: AW-36 0,16, AW-49 0,15.
    #[test]
    fn a110_werkstypen_und_standard() {
        let m = Model::with_seed(110);
        let aufbau = |code: &str| -> (String, bool, Vec<(String, f64, bool)>, f64) {
            let t = m.layer_set(typ(&m, code)).unwrap();
            (
                t.name.clone(),
                t.category == sk_model::TypeCategory::ExteriorWall,
                t.layers
                    .iter()
                    .map(|l| {
                        (
                            m.material(l.material).unwrap().name.clone(),
                            l.thickness,
                            l.core,
                        )
                    })
                    .collect(),
                t.thickness(),
            )
        };
        let s = |x: &str| x.to_string();
        assert_eq!(
            aufbau("AW-36"),
            (
                s("AW mit WDVS 36"),
                true,
                vec![
                    (s("Dämmung (WDVS)"), 120.0, false),
                    (s("Gasbeton"), 240.0, true)
                ],
                360.0
            )
        );
        let aw49 = aufbau("AW-49");
        assert_eq!(
            (aw49.0.as_str(), aw49.1, aw49.3),
            ("AW mehrschalig 49", true, 490.0)
        );
        assert_eq!(
            aw49.2.iter().map(|l| (l.1, l.2)).collect::<Vec<_>>(),
            [(115.0, false), (60.0, false), (140.0, false), (175.0, true)]
        );
        assert_eq!(aw49.2[1].0, "Luft");
        for (code, d) in [("IW-11,5", 115.0), ("IW-17,5", 175.0), ("IW-24", 240.0)] {
            let a = aufbau(code);
            assert!(!a.1, "{code} Innenwand");
            assert_eq!(a.2, [(s("Gasbeton"), d, true)], "{code}");
        }
        assert_eq!(aufbau("AW-31,5").3, 315.0, "AW-31,5 bleibt");
        let luft = m.layer_set(typ(&m, "AW-49")).unwrap().layers[1];
        assert_eq!(luft.function, LayerFunction::AirGap);
        let mat = |name: &str| {
            m.materials()
                .iter()
                .find(|(_, x)| x.name.starts_with(name))
                .map(|(_, x)| x.clone())
                .unwrap_or_else(|| panic!("Baustoff {name}"))
        };
        assert!(ist_luft(mat("Luft").category));
        for (name, lambda) in [
            ("Verblender", 0.68),
            ("Kerndämmung", 0.035),
            ("Gasbeton", 0.09),
            ("Dämmung (WDVS)", 0.035),
            ("Stahlbeton", 2.3),
            ("Putz", 0.87),
        ] {
            assert_eq!(mat(name).lambda, Some(lambda), "λ {name}");
        }
        // U-Wert
        let u = |code: &str| u_wert(&m, typ(&m, code)).map(|u| (u * 100.0).round() / 100.0);
        assert_eq!(u("AW-36"), Some(0.16));
        assert_eq!(u("AW-49"), Some(0.15));
        assert_eq!(u("IW-17,5"), None, "Innenwand ohne U-Wert");
        // Standard und feste Guids
        let neu = Model::new();
        let d = neu.defaults();
        assert_eq!(neu.layer_set(d.exterior_wall).unwrap().code, "AW-36");
        assert_eq!(neu.layer_set(d.interior_wall).unwrap().code, "IW-17,5");
        for code in ["AW-31,5", "AW-36", "AW-49", "IW-11,5", "IW-17,5", "IW-24"] {
            assert_eq!(
                neu.layer_set(typ(&neu, code)).unwrap().guid,
                m.layer_set(typ(&m, code)).unwrap().guid,
                "{code}: feste Guid"
            );
        }
        assert!(m.check().is_empty(), "{:?}", m.check());
    }

    /// A111 (K4, AW-36): Prüfhaus mit AW-36 je Geschoss: WDVS 12,1692, Gasbeton
    /// netto 21,5522 m³, Decke 75,7376 m² / 16,6623 m³, SP 17,600, FS 7,0238 m³;
    /// Innenwand IW-11,5 2,2060, IW-17,5 3,3570, IW-24 4,6039 m³. EG und OG
    /// gleich, alle gebunden (A-09).
    #[test]
    fn a111_mengen_aw36_und_innenwaende() {
        let mut s = Scene::with_model(Model::with_seed(111));
        let (aw, iw) = haus_b11(&mut s);
        auf_typ(&mut s, aw, "AW-36");
        let og = og_zug(&s, aw);
        for run in [aw, og] {
            assert_eq!(baustoff_m3(&s, run, "Dämmung (WDVS)"), 12.1692);
            assert_eq!(baustoff_m3(&s, run, "Gasbeton"), 21.5522);
            let d = decke_mengen(&s, run);
            assert_eq!((d.0, d.1), (75.7376, 16.6623));
        }
        assert_eq!((sp_m3(&s, aw), fs_m3(&s, aw)), (17.6, 7.0238));
        for (code, soll) in [("IW-11,5", 2.2060), ("IW-17,5", 3.3570), ("IW-24", 4.6039)] {
            auf_typ(&mut s, iw, code);
            assert_eq!(r4(wall_m3(&s, iw, 0)), soll, "{code}");
        }
        let m = s.model();
        assert!(m
            .elements()
            .iter()
            .all(|(_, e)| m.storey(e.storey).is_some()));
        assert!(m.check().is_empty(), "{:?}", m.check());
    }

    /// A112 (K4, AW-49 mit Luftschicht): Verblender 11,6687, Kerndämmung
    /// 13,6058, Gasbeton netto 15,1157 m³ je Geschoss. Die Luftschicht zählt zur
    /// Dicke, hat aber kein Volumen und keine Zeile; 5,9681 m³ (Luft als
    /// Körper) taucht nirgends auf, auch nicht in der CSV. Decke an der
    /// Kernaußenseite 69,0569 m² / 15,1925 m³; IW-17,5 3,2371 m³; SP und FS
    /// unverändert.
    #[test]
    fn a112_mehrschalig_mit_luftschicht() {
        let mut s = Scene::with_model(Model::with_seed(112));
        let (aw, iw) = haus_b11(&mut s);
        auf_typ(&mut s, iw, "IW-17,5");
        auf_typ(&mut s, aw, "AW-49");
        let og = og_zug(&s, aw);
        for run in [aw, og] {
            assert_eq!(baustoff_m3(&s, run, "Verblender (Vormauerziegel)"), 11.6687);
            assert_eq!(baustoff_m3(&s, run, "Kerndämmung (Mineralwolle)"), 13.6058);
            assert_eq!(baustoff_m3(&s, run, "Gasbeton"), 15.1157);
            assert_eq!(baustoff_m3(&s, run, "Luft"), 0.0, "Luft ohne Volumen");
            // Summe der gerundeten Einzelwerte: ±1 in der vierten Stelle
            assert!(
                (wand_m3(&s, run) - (11.6687 + 13.6058 + 15.1157)).abs() < 2e-4,
                "Summe ohne Luft: {}",
                wand_m3(&s, run)
            );
            let d = decke_mengen(&s, run);
            assert_eq!((d.0, d.1), (69.0569, 15.1925));
        }
        assert_eq!(
            s.wall_qto(s.model().wall_at(aw, 0).unwrap()).unwrap().width,
            490.0
        );
        assert_eq!(r4(wall_m3(&s, iw, 0)), 3.2371);
        assert_eq!((sp_m3(&s, aw), fs_m3(&s, aw)), (17.6, 7.0238));
        let liste = s.schedule().clone();
        let csv = crate::schedule_view::csv(s.model(), &liste);
        let text = String::from_utf8_lossy(&csv);
        assert!(!text.contains("5,968"), "Luft als Körper in der CSV");
        assert!(
            !text
                .lines()
                .any(|z| z.starts_with("Luft") || z.contains(";Luft;")),
            "keine Zeile Luft"
        );
        assert!(s.model().check().is_empty(), "{:?}", s.model().check());
    }

    /// A113 (K4, Frostschürze unabhängig vom Wandtyp, Jörn 17:34): Typwechsel
    /// AW-36 ↔ AW-49 lässt Breite 35 cm und FS 7,0238 m³. Das Feld „Breite“
    /// nimmt 30 und 45 cm, lehnt 29,5 und 45,5 ab; set_footing_width ebenso.
    /// Neue Schürzen 350 mm. Eine alte Datei mit 60 cm lädt mit 60 cm ohne
    /// Befund in check().
    #[test]
    fn a113_schuerze_30_bis_45_unabhaengig_vom_typ() {
        use crate::ui::Field;
        let mut s = Scene::with_model(Model::with_seed(113));
        let (aw, _iw) = haus_b11(&mut s);
        let breite = |s: &Scene| s.foundation_qto(aw).unwrap().1.width;
        assert_eq!(breite(&s), 350.0, "neue Schürze 35 cm");
        for code in ["AW-36", "AW-49", "AW-36", "AW-31,5"] {
            auf_typ(&mut s, aw, code);
            assert_eq!((breite(&s), fs_m3(&s, aw)), (350.0, 7.0238), "{code}");
        }
        let (_, footing) = sohlplatte(&s, aw);
        let mut ui = Ui::new(1.0, &Theme::dark());
        ui.fit(1.0, 1440, 900);
        for (eingabe, soll) in [
            ("29,5", 350.0),
            ("45,5", 350.0),
            ("30", 300.0),
            ("45", 450.0),
        ] {
            let out = tippe(&mut ui, &mut s, footing, Field::FootingWidth, eingabe);
            if out.submit.is_none() {
                assert!(
                    ui.edit.as_ref().is_some_and(|e| e.error.is_some()),
                    "Hinweis bei {eingabe}"
                );
                ui.key(Key::Escape, true, M).unwrap();
            }
            assert_eq!(breite(&s), soll, "Eingabe {eingabe}");
        }
        let mut m = s.model().clone();
        m.allow_unstepped();
        assert!(!m.set_footing_width(footing, 299.0));
        assert!(!m.set_footing_width(footing, 451.0));
        assert!(m.set_footing_width(footing, 350.0));
        // Alte Datei mit 60 cm
        s.edit_model("Breite", |m| m.set_footing_width(footing, 350.0));
        let text = sk_model::szo::write(s.model());
        let zeilen: Vec<String> = text
            .lines()
            .map(|l| {
                if l.starts_with("[footing]") {
                    assert!(l.contains("w=350"), "{l}");
                    l.replacen("w=350", "w=600", 1)
                } else {
                    l.to_string()
                }
            })
            .collect();
        let alt = zeilen.join("\n") + "\n";
        let geladen = sk_model::szo::read(&alt, GuidGen::with_seed(1)).expect("lädt");
        assert!(
            geladen.model.check().is_empty(),
            "{:?}",
            geladen.model.check()
        );
        let s2 = Scene::with_model(geladen.model);
        let aw2 = s2
            .model()
            .runs()
            .ids()
            .find(|r| s2.foundation_qto(*r).is_some())
            .unwrap();
        assert_eq!(
            s2.foundation_qto(aw2).unwrap().1.width,
            600.0,
            "Wert bleibt"
        );
    }

    /// A114 (K4, Prüfregel 20): Luftschicht außen oder innen am Rand, als Kern
    /// oder zweimal hintereinander wird abgelehnt oder von check() gemeldet.
    #[test]
    fn a114_pruefregel_20_luftschicht() {
        let mut s = Scene::with_model(Model::with_seed(114));
        haus_b11(&mut s);
        let m0 = s.model().clone();
        let aw49 = typ(&m0, "AW-49");
        let gut = m0.layer_set(aw49).unwrap().clone();
        let luft = gut.layers[1];
        let mut faelle: Vec<(&str, LayerSet)> = Vec::new();
        let mut t = gut.clone();
        t.layers.swap(0, 1);
        faelle.push(("Luft außen", t));
        let mut t = gut.clone();
        t.layers.push(luft);
        faelle.push(("Luft innen", t));
        let mut t = gut.clone();
        t.layers[1].core = true;
        faelle.push(("Luft als Kern", t));
        let mut t = gut.clone();
        t.layers.insert(1, luft);
        faelle.push(("zwei Luftschichten", t));
        for (name, t) in faelle {
            let mut m = m0.clone();
            m.allow_unstepped();
            let abgelehnt = !m.set_layer_set(aw49, t);
            assert!(
                abgelehnt || !m.check().is_empty(),
                "{name} schlägt nicht an"
            );
        }
        let mut m = m0.clone();
        m.allow_unstepped();
        assert!(m.set_layer_set(aw49, gut));
        assert!(m.check().is_empty());
    }

    /// A115 (K4, Firmenkatalog): Ein Firmenkatalog aus K1/K2 (nur AW-31,5,
    /// IW-17,5, Standard AW-31,5) bekommt beim Laden die fehlenden Werkstypen
    /// (nach Guid), mit Hinweis; vorhandene Typen werden nicht überschrieben;
    /// der Standard wechselt einmalig auf AW-36. Beim zweiten Laden kein Hinweis
    /// mehr, ein neues Projekt hat AW-36 / IW-17,5.
    #[test]
    fn a115_firmenkatalog_bekommt_werkstypen() {
        use sk_model::catalog::{export_type, write_szk, Library};
        let d = test_dir("a115");
        let p = d.join("firmenkatalog.szk");
        // Alter Katalog: AW-31,5 (mit geändertem Namen) und IW-17,5
        let mut alt = Model::with_seed(115);
        alt.allow_unstepped();
        let (a, i) = (typ(&alt, "AW-31,5"), typ(&alt, "IW-17,5"));
        let mut t = alt.layer_set(a).unwrap().clone();
        t.name = "AW 31,5 Büro".into();
        assert!(alt.set_layer_set(a, t));
        let mut lib = Library::default();
        for id in [a, i] {
            assert!(export_type(&alt, &mut lib, alt.layer_set(id).unwrap().guid));
        }
        lib.default_exterior = lib.type_by_guid(alt.layer_set(a).unwrap().guid);
        lib.default_interior = lib.type_by_guid(alt.layer_set(i).unwrap().guid);
        let vorher = write_szk(&lib);
        assert!(!vorher.contains("AW-36"));
        std::fs::write(&p, &vorher).unwrap();
        let (c, h) = firma_laden(&p, true);
        assert!(!h.is_empty(), "Hinweis: Werkstypen ergänzt");
        let neu = Model::from_library(c.library());
        let codes: Vec<String> = neu
            .layer_sets()
            .iter()
            .map(|(_, t)| t.code.clone())
            .collect();
        for code in ["AW-31,5", "AW-36", "AW-49", "IW-11,5", "IW-17,5", "IW-24"] {
            assert!(codes.iter().any(|c| c == code), "{code} fehlt: {codes:?}");
        }
        assert_eq!(
            neu.layer_set(typ(&neu, "AW-31,5")).unwrap().name,
            "AW 31,5 Büro",
            "nicht überschrieben"
        );
        assert_eq!(
            neu.layer_set(neu.defaults().exterior_wall).unwrap().code,
            "AW-36"
        );
        assert_eq!(
            neu.layer_set(neu.defaults().interior_wall).unwrap().code,
            "IW-17,5"
        );
        // Zweites Laden: nichts mehr zu ergänzen
        let (_, h2) = firma_laden(&p, true);
        assert!(h2.is_empty(), "{h2:?}");
    }

    // ===== Tests K5 =====

    /// Prüfhaus mit AW-36,5 (monolithisch, Auflager 24 cm, Randdämmstreifen).
    fn haus_k5(seed: u64) -> (Scene, RunId, RunId) {
        let mut s = Scene::with_model(Model::with_seed(seed));
        let (aw, iw) = haus_b11(&mut s);
        auf_typ(&mut s, iw, "IW-17,5");
        auf_typ(&mut s, aw, "AW-36,5");
        (s, aw, iw)
    }

    /// A116 (K5, Mengen): AW-36,5 = Gasbeton 36,5, Auflager 24 cm mit Streifen
    /// aus „Randdämmung“, U 0,24. Je Geschoss: Gasbeton netto 33,2197 m³ (wie
    /// bei voller Tasche, kein doppelter Abzug); Decke 9,75 × 7,75 = 75,5625 m²,
    /// 16,62375 m³; 4 Streifen (EG RD-001…004, OG RD-005…008), Achsen 9,875 /
    /// 7,875 m (zusammen 35,50), Volumen 0,27156 / 0,21656 (zusammen 0,97625);
    /// Decke + Streifen = 17,600 m³. SP 17,600, FS 7,0238, IW-17,5 3,3524 m³.
    /// Streifen gehören zum Geschoss ihrer Wand (A-09), Kostengruppe 330.
    #[test]
    fn a116_monolithisch_mit_randdaemmstreifen() {
        let (mut s, aw, iw) = haus_k5(116);
        let m = s.model();
        let t = m.layer_set(typ(m, "AW-36,5")).unwrap();
        assert_eq!(t.thickness(), 365.0);
        let (tiefe, mat) = auflager(t).expect("Auflager mit Streifen");
        assert_eq!(tiefe, 240.0);
        let rd = m.material(mat).unwrap();
        assert_eq!(
            (rd.name.as_str(), rd.category, rd.lambda),
            ("Randdämmung", MatCategory::Insulation, Some(0.035))
        );
        assert_eq!(
            u_wert(m, typ(m, "AW-36,5")).map(|u| (u * 100.0).round() / 100.0),
            Some(0.24)
        );
        let og = og_zug(&s, aw);
        let st = streifen(&s);
        assert_eq!(
            st.iter().map(|x| x.0.as_str()).collect::<Vec<_>>(),
            ["RD-001", "RD-002", "RD-003", "RD-004", "RD-005", "RD-006", "RD-007", "RD-008"]
        );
        for run in [aw, og] {
            assert_eq!(baustoff_m3(&s, run, "Gasbeton"), 33.2197);
            let (f, v, _) = decke_mengen(&s, run);
            assert_eq!(f, 75.5625);
            assert!((v - 16.62375).abs() < 1e-3, "Decke {v}");
            // Streifen dieses Zugs
            let walls: Vec<ElementId> = (0..4).map(|i| m.wall_at(run, i).unwrap()).collect();
            let mut laengen = Vec::new();
            let mut vol = 0.0;
            for (_, el) in &st {
                let (w, fl) = streifen_von(m, *el).expect("Verweise");
                if !walls.contains(&w) {
                    continue;
                }
                assert_eq!(Some(fl), decke(&s, run), "Decke des Zugs");
                assert_eq!(
                    m.element(*el).unwrap().storey,
                    m.element(w).unwrap().storey,
                    "A-09"
                );
                let (l, v) = streifen_qto(&s, *el);
                laengen.push((l / 1e3 * 1e3).round() / 1e3);
                vol += v / 1e9;
            }
            laengen.sort_by(|a, b| a.partial_cmp(b).unwrap());
            assert_eq!(laengen, [7.875, 7.875, 9.875, 9.875]);
            assert!((vol - 0.97625).abs() < 1e-4, "Streifen {vol}");
            assert!(
                (v + vol - 17.6).abs() < 1e-3,
                "Decke + Streifen = volle Tasche"
            );
        }
        assert_eq!((sp_m3(&s, aw), fs_m3(&s, aw)), (17.6, 7.0238));
        assert_eq!(r4(wall_m3(&s, iw, 0)), 3.3524);
        assert!(m.check().is_empty(), "{:?}", m.check());
        // Mengenliste: Gruppe Randdämmstreifen, ohne zusätzlichen Gasbeton-Abzug
        let liste = s.schedule().clone();
        let csv = crate::schedule_view::csv(s.model(), &liste);
        let text = String::from_utf8_lossy(&csv);
        assert!(text.contains("Randdämmstreifen"), "Gruppe in der Liste");
        assert!(text.contains("RD-001") && text.contains("RD-008"));
    }

    /// A117 (K5, Typwechsel und Gummiband): AW-36 → AW-36,5 legt die 8 Streifen
    /// in einem Rückgängig-Schritt an, zurück auf AW-36 entfernt sie;
    /// Rückgängig bringt sie mit denselben Guids und Nummern. Gummiband an einer
    /// Wand: dieselben Streifen (Guid), nur Länge und Volumen ändern sich.
    #[test]
    fn a117_streifen_folgen_typ_und_gummiband() {
        let mut s = Scene::with_model(Model::with_seed(117));
        let (aw, _iw) = haus_b11(&mut s);
        auf_typ(&mut s, aw, "AW-36");
        assert!(streifen(&s).is_empty());
        let k5 = typ(s.model(), "AW-36,5");
        assert!(zugtyp(&mut s, aw, k5));
        assert_eq!(s.undo_label(), Some("Wandtyp geändert"));
        let guids = |s: &Scene| -> Vec<(String, sk_model::Guid)> {
            streifen(s)
                .into_iter()
                .map(|(n, id)| (n, s.model().element(id).unwrap().guid))
                .collect()
        };
        let mit = guids(&s);
        assert_eq!(mit.len(), 8);
        assert!(s.undo());
        assert!(streifen(&s).is_empty(), "ein Schritt");
        assert!(s.redo());
        assert_eq!(guids(&s), mit, "gleiche Guids und Nummern");
        let k4 = typ(s.model(), "AW-36");
        assert!(zugtyp(&mut s, aw, k4));
        assert!(streifen(&s).is_empty(), "zurück auf AW-36");
        assert!(s.undo());
        assert_eq!(guids(&s), mit);
        // Gummiband an der Wand y = 8 m: +1 m
        let laenge = |s: &Scene| -> f64 {
            streifen(s)
                .iter()
                .map(|(_, el)| streifen_qto(s, *el).0)
                .sum::<f64>()
        };
        let vorher = laenge(&s);
        ziehen_am_fuss(&mut s, 0.0, 1000.0);
        assert_eq!(guids(&s), mit, "dieselben Bauteile");
        assert!(laenge(&s) > vorher, "länger");
        assert!(s.model().check().is_empty(), "{:?}", s.model().check());
    }

    /// A118 (K5, Datei und Prüfregeln 21–22): Rundlauf stabil; Auflager tiefer
    /// als die Wand oder 0 wird abgelehnt oder gemeldet (21); ein Streifen, der
    /// auf eine fremde Decke zeigt, oder ein doppelter Streifen schlägt an (22).
    #[test]
    fn a118_datei_und_pruefregeln_21_22() {
        let (s, aw, _) = haus_k5(118);
        let text = sk_model::szo::write(s.model());
        let m2 = sk_model::szo::read(&text, GuidGen::with_seed(1))
            .unwrap()
            .model;
        assert_eq!(sk_model::szo::write(&m2), text, "Rundlauf");
        assert!(pruefung(&text).is_empty(), "{:?}", pruefung(&text));
        // Regel 21
        let id = typ(s.model(), "AW-36,5");
        let t = s.model().layer_set(id).unwrap().clone();
        let (_, mat) = auflager(&t).unwrap();
        for tiefe in [365.0, 400.0, 0.0] {
            let mut m = s.model().clone();
            m.allow_unstepped();
            let mut t2 = t.clone();
            setze_auflager(&mut t2, Some((tiefe, mat)));
            let abgelehnt = !m.set_layer_set(id, t2);
            assert!(abgelehnt || !m.check().is_empty(), "Auflager {tiefe}");
        }
        // Regel 22: Streifen zeigt auf die OG-Decke statt auf die EG-Decke
        let og = og_zug(&s, aw);
        let g = |el: ElementId| s.model().element(el).unwrap().guid.to_string();
        let (de_eg, de_og) = (g(decke(&s, aw).unwrap()), g(decke(&s, og).unwrap()));
        let erster = streifen(&s)[0].1;
        let sg = g(erster);
        let mut falsch = Vec::new();
        let mut doppelt = Vec::new();
        for l in text.lines() {
            if l.contains(&format!("guid={sg}")) {
                assert!(
                    l.contains(&de_eg),
                    "Streifen verweist auf die EG-Decke: {l}"
                );
                falsch.push(l.replacen(&de_eg, &de_og, 1));
                doppelt.push(l.to_string());
                doppelt.push(l.replacen(&sg, "0000000000000000000001", 1));
            } else {
                falsch.push(l.to_string());
                doppelt.push(l.to_string());
            }
        }
        for (name, t) in [("fremde Decke", falsch), ("doppelter Streifen", doppelt)] {
            let t = t.join("\n") + "\n";
            assert_ne!(t, text);
            assert!(!pruefung(&t).is_empty(), "{name} schlägt nicht an");
        }
    }
}

/// Abnahme K5-Nachtrag: λ in alten Dateien; gelöschte Werkstypen.
mod lambda_alt {
    use super::*;
    // Abnahmetests K5-Nachtrag „λ in alten Dateien ergänzen“ (BIM, Koordinator
    // 19:06) und K4 „gelöschte Werkstypen kommen nicht wieder“ ([stock] set=…).
    // Paket: bim/paket-k4-k5-wandtypen.md, Nachtrag zu K5 („Fertig, wenn“).
    // Spezifikation: test/abnahme-wandtypen.md (A122, A123).
    //
    // Einbau: als `mod lambda_alt { use super::*; … }` ans Ende von
    // app/src/abnahme.rs. Nutzt aus abnahme.rs: test_dir und die Prüfhaus-Datei
    // abnahme_haus_v3.szo (gespeichert mit 1f0763e, Version 3, alle λ = „-“).
    // Keine angenommenen Namen: alles läuft über document::load, Document,
    // Model::u_value, catalog::{read_szk, write_szk} und Company::load.

    use sk_model::catalog::{read_szk, write_szk};
    use sk_model::{LayerSetId, Model};

    const HAUS_V3: &str = include_str!("abnahme_haus_v3.szo");

    // ===== Hilfen =====

    /// Öffnet `text` als Datei wie „Datei → Öffnen“ (main.rs open_path):
    /// laden, Szene, Dokument mit dem Stand nach dem Laden.
    fn oeffnen(name: &str, text: &str) -> (Scene, crate::document::Document, Vec<String>) {
        let d = test_dir(name);
        let p = d.join("haus.szo");
        std::fs::write(&p, text).unwrap();
        let l = crate::document::load(&p).expect("öffnet");
        let s = Scene::with_model(l.model);
        let doc = crate::document::Document::opened(p, s.model().revision());
        (s, doc, l.hints)
    }

    fn lambda(m: &Model, name: &str) -> Option<f64> {
        m.materials()
            .iter()
            .find(|(_, x)| x.name == name)
            .unwrap_or_else(|| panic!("Baustoff {name} fehlt"))
            .1
            .lambda
    }

    fn typ(m: &Model, code: &str) -> LayerSetId {
        m.layer_sets()
            .iter()
            .find(|(_, t)| t.code == code)
            .map(|(id, _)| id)
            .unwrap_or_else(|| panic!("Typ {code} fehlt"))
    }

    /// U-Wert auf zwei Stellen, wie im Katalog angezeigt.
    fn u2(m: &Model, code: &str) -> Option<f64> {
        m.u_value(typ(m, code)).map(|u| (u * 100.0).round() / 100.0)
    }

    /// Ersetzt in der Baustoffzeile `name="…"` das λ-Feld durch `neu`
    /// (bzw. benennt um, wenn `neuer_name` gesetzt ist).
    fn baustoff_zeile(
        text: &str,
        name: &str,
        lambda: Option<&str>,
        neuer_name: Option<&str>,
    ) -> String {
        let key = format!("name=\"{name}\"");
        let mut n = 0;
        let out: Vec<String> = text
            .lines()
            .map(|l| {
                if !(l.starts_with("[material]") && l.contains(&key)) {
                    return l.to_string();
                }
                n += 1;
                let mut l = l.to_string();
                if let Some(v) = lambda {
                    l = l
                        .split(' ')
                        .map(|t| {
                            if t.starts_with("lambda=") {
                                format!("lambda={v}")
                            } else {
                                t.to_string()
                            }
                        })
                        .collect::<Vec<_>>()
                        .join(" ");
                }
                if let Some(nn) = neuer_name {
                    l = l.replacen(&key, &format!("name=\"{nn}\""), 1);
                }
                l
            })
            .collect();
        assert_eq!(n, 1, "Baustoff {name} genau einmal in der Datei");
        out.join("\n") + "\n"
    }

    /// Alle λ-Felder der Baustoffzeilen auf „-“.
    fn ohne_lambda(text: &str) -> String {
        text.lines()
            .map(|l| {
                if !l.starts_with("[material]") {
                    return l.to_string();
                }
                l.split(' ')
                    .map(|t| {
                        if t.starts_with("lambda=") {
                            "lambda=-".to_string()
                        } else {
                            t.to_string()
                        }
                    })
                    .collect::<Vec<_>>()
                    .join(" ")
            })
            .collect::<Vec<_>>()
            .join("\n")
            + "\n"
    }

    // ===== Tests =====

    /// A122 (K5-Nachtrag „λ in alten Dateien“): Das Prüfhaus v3 (alle λ = „-“,
    /// zeitbasierte Baustoff-Guids) öffnet mit Gasbeton 0,09, Dämmung (WDVS)
    /// 0,035, Stahlbeton 2,3, Putz 0,87 (Treffer über Name und Kategorie). U von
    /// AW-31,5 = 0,16. Die Datei gilt als unverändert: kein •, kein
    /// Rückgängig-Schritt, kein Hinweis. Guids bleiben. Zweimal öffnen ergibt
    /// dasselbe Modell (Regel 13), beim Speichern steht λ in der Datei.
    /// Ein vorhandenes λ 0,12 bleibt 0,12; ein umbenannter „Gasbeton (alt)“
    /// bleibt ohne λ. Treffer über die Guid: ein Werksbaustoff mit fremdem Namen
    /// bekommt sein λ trotzdem.
    #[test]
    fn a122_lambda_in_alten_dateien() {
        assert_eq!(HAUS_V3.matches("lambda=-").count(), 4, "Prüfhaus ohne λ");
        let (s, doc, hints) = oeffnen("a122", HAUS_V3);
        let m = s.model();
        for (n, l) in [
            ("Gasbeton", 0.09),
            ("Dämmung (WDVS)", 0.035),
            ("Stahlbeton", 2.3),
            ("Putz", 0.87),
        ] {
            assert_eq!(lambda(m, n), Some(l), "λ {n}");
        }
        assert_eq!(
            u2(m, "AW-31,5"),
            Some(0.16),
            "0,13 + 0,14/0,035 + 0,175/0,09 + 0,04"
        );
        assert!(!doc.is_dirty(m), "Titel ohne •");
        assert_eq!(s.undo_label(), None, "kein Rückgängig-Schritt");
        assert!(hints.is_empty(), "kein Hinweis: {hints:?}");
        assert!(m.check().is_empty(), "{:?}", m.check());
        // Guids der Baustoffe wie in der Datei (Regel 19)
        for l in HAUS_V3.lines().filter(|l| l.starts_with("[material]")) {
            let g = l.split(' ').find_map(|t| t.strip_prefix("guid=")).unwrap();
            assert!(
                m.materials().iter().any(|(_, x)| x.guid.to_string() == g),
                "Guid {g} unverändert"
            );
        }
        // Immer gleich ergänzt, beim Speichern steht λ in der Datei
        let text = sk_model::szo::write(m);
        let (s2, _, _) = oeffnen("a122b", HAUS_V3);
        assert_eq!(
            sk_model::szo::write(s2.model()),
            text,
            "zweimal öffnen = dasselbe"
        );
        assert!(!text.contains("lambda=-"), "λ gespeichert");
        let (s3, doc3, _) = oeffnen("a122c", &text);
        assert_eq!(sk_model::szo::write(s3.model()), text, "Rundlauf");
        assert!(!doc3.is_dirty(s3.model()));

        // Vorhandenes λ wird nie überschrieben
        let eigen = baustoff_zeile(HAUS_V3, "Gasbeton", Some("0.12"), None);
        let (s, doc, _) = oeffnen("a122d", &eigen);
        assert_eq!(lambda(s.model(), "Gasbeton"), Some(0.12));
        assert_eq!(lambda(s.model(), "Dämmung (WDVS)"), Some(0.035));
        assert_eq!(u2(s.model(), "AW-31,5"), Some(0.18), "mit λ 0,12");
        assert!(!doc.is_dirty(s.model()));

        // Umbenannt: kein Treffer, λ bleibt leer, U zeigt „–“
        let alt = baustoff_zeile(HAUS_V3, "Gasbeton", None, Some("Gasbeton (alt)"));
        let (s, _, _) = oeffnen("a122e", &alt);
        assert_eq!(lambda(s.model(), "Gasbeton (alt)"), None);
        assert_eq!(lambda(s.model(), "Dämmung (WDVS)"), Some(0.035));
        assert_eq!(u2(s.model(), "AW-31,5"), None);

        // Treffer über die Guid: aktuelles Projekt ohne λ, Gasbeton umbenannt
        let werk = Model::new();
        let neu = baustoff_zeile(
            &ohne_lambda(&sk_model::szo::write(&werk)),
            "Gasbeton",
            None,
            Some("Porenbeton Büro"),
        );
        let (s, doc, _) = oeffnen("a122f", &neu);
        for (_, w) in werk.materials().iter() {
            let x = s
                .model()
                .materials()
                .iter()
                .find(|(_, x)| x.guid == w.guid)
                .unwrap()
                .1;
            assert_eq!(x.lambda, w.lambda, "λ über die Guid: {}", w.name);
        }
        assert_eq!(lambda(s.model(), "Porenbeton Büro"), Some(0.09));
        assert!(!doc.is_dirty(s.model()));
    }

    /// A123 (K4, Firmenkatalog): Ein Werkstyp, den das Büro aus dem Firmenkatalog
    /// gelöscht hat, kommt beim nächsten Laden nicht wieder (Vermerk
    /// [stock] set=…). Ein vom Büro gewählter Standard (AW-31,5) bleibt. Kein
    /// Hinweis.
    #[test]
    fn a123_geloeschter_werkstyp_kommt_nicht_wieder() {
        let d = test_dir("a123");
        let p = d.join("firmenkatalog.szk");
        let (_, h) = crate::catalog::Company::load(&p, true);
        assert!(h.is_empty(), "{h:?}");
        let text = std::fs::read_to_string(&p).expect("am Vorgabeort angelegt");
        assert_eq!(
            text.matches("[stock]").count(),
            7,
            "alle Werkstypen vermerkt (seit K5 mit AW-36,5)"
        );
        let mut lib = read_szk(&text).unwrap();
        let aw49 = lib
            .types
            .iter()
            .find(|(_, t)| t.code == "AW-49")
            .map(|(id, _)| id)
            .unwrap();
        lib.types.remove(aw49);
        lib.default_exterior = lib
            .types
            .iter()
            .find(|(_, t)| t.code == "AW-31,5")
            .map(|(id, _)| id);
        std::fs::write(&p, write_szk(&lib)).unwrap();
        for runde in 0..2 {
            let (c, h) = crate::catalog::Company::load(&p, true);
            assert!(h.is_empty(), "Runde {runde}: kein Hinweis {h:?}");
            let neu = Model::from_library(c.library());
            let codes: Vec<String> = neu
                .layer_sets()
                .iter()
                .map(|(_, t)| t.code.clone())
                .collect();
            assert!(
                !codes.iter().any(|c| c == "AW-49"),
                "bleibt gelöscht: {codes:?}"
            );
            assert_eq!(codes.len(), 6);
            assert_eq!(
                neu.layer_set(neu.defaults().exterior_wall).unwrap().code,
                "AW-31,5",
                "Standard des Büros bleibt"
            );
        }
    }
}

// Abnahmetests K3b: Übergänge im Bauteilkatalog (Wände wachsen nach
// „Ändern“, Leuchten klingt ab, anim_ms = 0), vorbereitet gegen main
// c23fb08. Paket: einstellungen/paket-k3b-animationen.md (§1, §3, §6),
// Spezifikation test/abnahme-bauteilkatalog.md (Abschnitt K3b).
//
// Einbau: als `mod uebergaenge { use super::*; … }` ans Ende von
// app/src/abnahme.rs. Nutzt aus abnahme.rs: haus_b11, mengen_b11, STANDARD.
//
// Die Tests prüfen das, was man sieht: Lage der Außen- und Innenfläche der
// Westwand (x = 0 … Dicke, EG) zum Zeitpunkt t nach dem Auslöser, so wie
// sie gezeichnet wird. Mischt der Bau im Vertex-Shader (Weg 1), rechnet der
// Adapter dieselbe Mischung (alt, neu, u_grow) aus den beiden Netzen nach.
// Angenommene Namen stehen NUR in den Adaptern.
mod uebergaenge {
    use super::*;

    // ===== Adapter =====

    use crate::scene::CATALOG_STEP;
    use std::cell::Cell;

    thread_local! {
        /// Uhr der Szene beim letzten Auslöser: jeder Auslöser setzt die Uhr
        /// vorwärts, so bleibt sie wie in der App stetig.
        static AUSLOESER: Cell<u64> = const { Cell::new(0) };
    }

    fn ausloesen(s: &mut Scene) {
        let t = AUSLOESER.with(|c| {
            c.set(c.get() + 1_000_000);
            c.get()
        });
        s.set_now(t);
    }

    /// Typ über den Weg der Bedienung ändern (Katalog OK → „Ändern“): ein
    /// Rückgängig-Schritt, Übergang startet bei t = 0.
    fn aendern(s: &mut Scene, id: sk_model::LayerSetId, t: sk_model::LayerSet) -> bool {
        ausloesen(s);
        s.edit_types(CATALOG_STEP, |m| m.set_layer_set(id, t))
    }
    /// Rückgängig über die Bedienung (Strg+Z), Übergang rückwärts ab t = 0.
    fn rueckgaengig(s: &mut Scene) -> bool {
        ausloesen(s);
        s.undo()
    }
    /// Uhr des Übergangs: `t` ms nach dem letzten Auslöser.
    fn uhr(s: &mut Scene, t: u64) {
        s.grow_tick(AUSLOESER.with(|c| c.get()) + t);
    }
    /// Gezeichnete x-Lage (mm) der Außen- und der Innenfläche der Westwand im
    /// EG zum Zeitpunkt `t` ms, aus dem 3D-Netz, wie es hochgeladen wird.
    fn westwand(s: &mut Scene, t: u64) -> (f64, f64) {
        uhr(s, t);
        let m = s.mesh(ViewKind::Persp, None, &[]);
        let (mut aussen, mut innen) = (f64::MAX, f64::MIN);
        for v in &m.faces {
            let (x, z) = (v[0] as f64, v[2] as f64);
            // Westwand im EG: Flächen links der Mitte bis UK EG-Decke
            if x > 2000.0 || !(-1.0..2700.0).contains(&z) {
                continue;
            }
            if v[3] < -0.99 {
                aussen = aussen.min(x);
            } else if v[3] > 0.99 {
                innen = innen.max(x);
            }
        }
        (aussen, innen)
    }
    /// Stärke des Leuchtens der betroffenen Wände zum Zeitpunkt `t` (0 … 1).
    fn leuchten(s: &mut Scene, t: u64) -> f32 {
        uhr(s, t);
        s.grow_glow().0
    }
    /// Klick ins Modell während des Übergangs.
    fn klick(s: &mut Scene, t: u64) {
        uhr(s, t);
        s.skip_animation();
    }
    /// `anim_ms` aus Einstellungen → Bedienoberfläche → „Animationen“.
    fn animationen(s: &mut Scene, an: bool) {
        let mut t = s.theme().clone();
        t.size.anim_ms = if an { 280.0 } else { 0.0 };
        s.set_theme(&t);
    }
    /// 3D-Netz, wie gezeichnet (Endstand ohne Übergang zum Vergleich).
    fn netz(s: &mut Scene) -> Vec<[f32; 3]> {
        let m = s.mesh(ViewKind::Persp, None, &[]);
        let mut v: Vec<[f32; 3]> = m.faces.iter().map(|p| [p[0], p[1], p[2]]).collect();
        v.sort_by(|a, b| a.partial_cmp(b).unwrap());
        v
    }

    // ===== Hilfen =====

    /// AW-31,5 mit 12 Dämmung + 24 Gasbeton (36 cm): wächst um 4,5 cm nach innen.
    fn dicker(s: &Scene) -> (sk_model::LayerSetId, sk_model::LayerSet) {
        let id = s.model().defaults().exterior_wall;
        let mut t = s.model().layer_set(id).unwrap().clone();
        t.layers[0].thickness = 120.0;
        t.layers[1].thickness = 240.0;
        (id, t)
    }

    /// Kurve ease-out 1 − (1 − u)³.
    fn kurve(u: f64) -> f64 {
        1.0 - (1.0 - u.clamp(0.0, 1.0)).powi(3)
    }

    fn nahe(a: f64, b: f64) -> bool {
        (a - b).abs() < 0.5
    }

    /// A119 (K3b §1, Prüfpunkte 1–2): „Ändern“ an AW-31,5 (8 Wände) → 36 cm.
    /// Die Innenfläche wandert in 280 ms ease-out von 315 auf 360 mm, die
    /// Außenfläche bleibt zu jedem Zeitpunkt stehen. Ab 280 ms ist der Endstand
    /// erreicht und gleich dem Netz ohne Übergang. Das Leuchten klingt in 600 ms
    /// linear von 0,43 auf 0 ab. Mengen sind sofort die neuen (keine Zwischenwerte),
    /// ein Rückgängig-Schritt.
    #[test]
    fn a119_waende_wachsen_nach_aendern() {
        let mut s = Scene::with_model(Model::with_seed(119));
        let (aw, iw) = haus_b11(&mut s);
        animationen(&mut s, true);
        let (aussen0, innen0) = westwand(&mut s, 0);
        assert!(nahe(innen0 - aussen0, 315.0));
        let (id, t) = dicker(&s);
        assert!(aendern(&mut s, id, t));
        assert_eq!(s.undo_label(), Some("Bauteilkatalog geändert"));
        assert_eq!(
            mengen_b11(&s, aw, iw)[1],
            21.5522,
            "Mengen sofort neu (Gasbeton netto 36 cm)"
        );
        for t in [0u64, 35, 70, 140, 210, 279, 280, 400] {
            let (a, i) = westwand(&mut s, t);
            assert!(nahe(a, aussen0), "Außenfläche bei {t} ms: {a}");
            let soll = aussen0 + 315.0 + 45.0 * kurve(t as f64 / 280.0);
            assert!(nahe(i, soll), "Innenfläche bei {t} ms: {i}, soll {soll}");
        }
        for (t, soll) in [(0u64, 0.43f32), (300, 0.215), (600, 0.0), (900, 0.0)] {
            assert!(
                (leuchten(&mut s, t) - soll).abs() < 0.01,
                "Leuchten bei {t} ms"
            );
        }
        let mit = netz(&mut s);
        let mut s2 = Scene::with_model(Model::with_seed(119));
        haus_b11(&mut s2);
        animationen(&mut s2, false);
        let (id2, t2) = dicker(&s2);
        assert!(aendern(&mut s2, id2, t2));
        assert_eq!(mit, netz(&mut s2), "Endstand gleich dem ohne Übergang");
    }

    /// A120 (K3b §1, Prüfpunkte 3 und 5): Strg+Z zeigt den Übergang rückwärts
    /// (Innenfläche 360 → 315, Außenfläche fest), Endstand und Mengen wie
    /// vorher. Ein Klick während des Übergangs springt sofort auf den Endstand.
    #[test]
    fn a120_rueckgaengig_rueckwaerts_und_klick_springt() {
        let mut s = Scene::with_model(Model::with_seed(120));
        let (aw, iw) = haus_b11(&mut s);
        animationen(&mut s, true);
        let vorher = netz(&mut s);
        let (aussen0, _) = westwand(&mut s, 0);
        let (id, t) = dicker(&s);
        assert!(aendern(&mut s, id, t));
        westwand(&mut s, 400);
        assert!(rueckgaengig(&mut s));
        for t in [0u64, 140, 280] {
            let (a, i) = westwand(&mut s, t);
            assert!(nahe(a, aussen0), "Außenfläche bei {t} ms");
            let soll = aussen0 + 360.0 - 45.0 * kurve(t as f64 / 280.0);
            assert!(nahe(i, soll), "rückwärts bei {t} ms: {i}, soll {soll}");
        }
        assert_eq!(mengen_b11(&s, aw, iw), STANDARD);
        assert_eq!(netz(&mut s), vorher, "Geometrie wie vorher");
        // Klick mitten im Übergang
        let (id, t) = dicker(&s);
        assert!(aendern(&mut s, id, t));
        westwand(&mut s, 70);
        klick(&mut s, 70);
        let (a, i) = westwand(&mut s, 71);
        assert!(
            nahe(a, aussen0) && nahe(i, aussen0 + 360.0),
            "sofort Endstand: {i}"
        );
        assert!(leuchten(&mut s, 71) < 0.01, "Leuchten aus");
    }

    /// A121 (K3b §3, Prüfpunkt 4): Mit anim_ms = 0 springt die Wand sofort auf
    /// den Endstand, kein Leuchten; das Bild gleicht dem Ende mit Übergang.
    #[test]
    fn a121_ohne_animation_sofort() {
        let mut s = Scene::with_model(Model::with_seed(121));
        haus_b11(&mut s);
        animationen(&mut s, false);
        let (aussen0, _) = westwand(&mut s, 0);
        let (id, t) = dicker(&s);
        assert!(aendern(&mut s, id, t));
        let (a, i) = westwand(&mut s, 0);
        assert!(nahe(a, aussen0) && nahe(i, aussen0 + 360.0), "sofort: {i}");
        assert!(leuchten(&mut s, 0) < 0.01, "kein Leuchten");
        let ohne = netz(&mut s);
        let mut s2 = Scene::with_model(Model::with_seed(121));
        haus_b11(&mut s2);
        animationen(&mut s2, true);
        let (id2, t2) = dicker(&s2);
        assert!(aendern(&mut s2, id2, t2));
        westwand(&mut s2, 400);
        assert_eq!(netz(&mut s2), ohne);
    }
}

// Abnahmetests Firmenkatalog-Datei .szk: Rundlauf mit allen
// Baustoffkategorien (A124) und alte .szk aus K2/K3 verlustfrei (A125).
// Anlass: Jörn 19:20, „firmenkatalog.szk nicht lesbar (Zeile 13: [material]:
// „cat“ ist kein gültiger Wert (Schlüsselwort))“.
// Spezifikation: test/abnahme-wandtypen.md (A124, A125).
//
// Einbau: als `mod szk_rundlauf { use super::*; … }` ans Ende von
// app/src/abnahme.rs. Die alte Datei `vorbereitet/a125-firmenkatalog-k2.szk`
// als `app/src/abnahme_firmenkatalog_k2.szk` daneben legen. Sie ist mit
// c23fb08 geschrieben (137fca7 schreibt bytegleich): Library::standard() des
// alten Stands, AW-31,5 umbenannt in „AW 31,5 Büro“, λ von „Dämmung (WDVS)“
// von Hand auf 0,12, Gasbeton ohne λ. Nutzt aus abnahme.rs: test_dir.
// Keine angenommenen Namen.
mod szk_rundlauf {
    use super::*;

    use sk_model::catalog::{read_szk, write_szk, Library};
    use sk_model::{MatCategory, Model};

    const SZK_K2: &str = include_str!("abnahme_firmenkatalog_k2.szk");

    fn laden(p: &std::path::Path) -> (crate::catalog::Company, Vec<String>) {
        crate::catalog::Company::load(p, true)
    }

    fn baustoff<'a>(lib: &'a Library, name: &str) -> &'a sk_model::Material {
        lib.materials
            .iter()
            .find(|(_, x)| x.name == name)
            .unwrap_or_else(|| panic!("Baustoff {name} fehlt"))
            .1
    }

    fn typ_code<'a>(lib: &'a Library, code: &str) -> Option<&'a sk_model::LayerSet> {
        lib.types
            .iter()
            .find(|(_, t)| t.code == code)
            .map(|(_, t)| t)
    }

    /// A124: Der Startbestand (mit Luft) und je ein Baustoff jeder Kategorie aus
    /// `MatCategory::ALL` (auch Holz und Luft) plus „Randdämmung“ (Dämmung,
    /// λ 0,035): schreiben → lesen ergibt dieselbe Bibliothek, schreiben →
    /// lesen → schreiben denselben Text. Über die Datei (Company::load): kein
    /// Hinweis, nichts verloren, die Datei bleibt bytegleich.
    #[test]
    fn a124_szk_rundlauf_alle_baustoffkategorien() {
        let mut m = Model::with_seed(124);
        let mut lib = Library::standard();
        assert!(
            lib.materials
                .iter()
                .any(|(_, x)| x.category == MatCategory::Air),
            "Startbestand mit Luft (AW-49)"
        );
        let vorlage = lib.materials.iter().next().unwrap().1.clone();
        for (i, cat) in MatCategory::ALL.iter().enumerate() {
            let mut x = vorlage.clone();
            x.guid = m.new_guid();
            x.name = format!("Prüfbaustoff {i}");
            x.category = *cat;
            x.lambda = Some(0.5 + i as f64 / 100.0);
            lib.materials.insert(x);
        }
        let mut rd = vorlage.clone();
        rd.guid = m.new_guid();
        rd.name = "Randdämmung".into();
        rd.category = MatCategory::Insulation;
        rd.lambda = Some(0.035);
        lib.materials.insert(rd);

        let text = write_szk(&lib);
        assert!(text.contains("cat=air"), "Luft steht in der Datei");
        let zurueck = read_szk(&text).unwrap_or_else(|e| panic!("lesbar: {e}"));
        assert_eq!(zurueck, lib, "schreiben → lesen");
        assert_eq!(write_szk(&zurueck), text, "schreiben → lesen → schreiben");
        for (i, cat) in MatCategory::ALL.iter().enumerate() {
            let x = baustoff(&zurueck, &format!("Prüfbaustoff {i}"));
            assert_eq!((x.category, x.lambda), (*cat, Some(0.5 + i as f64 / 100.0)));
        }
        let r = baustoff(&zurueck, "Randdämmung");
        assert_eq!(
            (r.category, r.lambda),
            (MatCategory::Insulation, Some(0.035))
        );

        // Über die Datei wie beim Start
        let d = test_dir("a124");
        let p = d.join("firmenkatalog.szk");
        std::fs::write(&p, &text).unwrap();
        let (c, h) = laden(&p);
        assert!(h.is_empty(), "kein Hinweis: {h:?}");
        assert_eq!(c.library(), &lib, "nichts verloren");
        assert_eq!(
            std::fs::read_to_string(&p).unwrap(),
            text,
            "Datei unberührt"
        );
        // Ein zweiter Start liest die Datei ebenso
        let (c2, h2) = laden(&p);
        assert!(h2.is_empty(), "{h2:?}");
        assert_eq!(c2.library(), &lib);
    }

    /// A125: Eine .szk aus K2/K3 (137fca7/c23fb08, ohne [stock], ohne Luft) liest
    /// der neue Stand verlustfrei: alle Guids da, Büroname, Schichten, Standards
    /// und das von Hand gesetzte λ 0,12 bleiben; jede Zeile der alten Datei steht
    /// unverändert im neu geschriebenen Text. Über die Datei: einmal der Hinweis
    /// „Werkstypen ergänzt“, Büroname und λ 0,12 bleiben, fehlendes λ (Gasbeton)
    /// wird 0,09, Standard AW-36 (K4). Danach ist die Datei für den neuen Stand
    /// wieder lesbar, der zweite Start gibt keinen Hinweis.
    #[test]
    fn a125_alte_szk_verlustfrei() {
        assert!(SZK_K2.starts_with("SZK 1\n") && !SZK_K2.contains("[stock]"));
        let lib = read_szk(SZK_K2).unwrap_or_else(|e| panic!("alte .szk lesbar: {e}"));
        // Jede Zeile der alten Datei bleibt beim Zurückschreiben erhalten
        let neu = write_szk(&lib);
        for l in SZK_K2.lines().filter(|l| l.starts_with('[')) {
            assert!(neu.lines().any(|n| n == l), "verloren: {l}");
        }
        assert_eq!(read_szk(&neu).unwrap(), lib, "Rundlauf");
        let aw = typ_code(&lib, "AW-31,5").expect("AW-31,5");
        assert_eq!(aw.name, "AW 31,5 Büro");
        assert_eq!(
            aw.layers.iter().map(|l| l.thickness).collect::<Vec<_>>(),
            [140.0, 175.0]
        );
        assert_eq!(baustoff(&lib, "Dämmung (WDVS)").lambda, Some(0.12));
        assert_eq!(
            lib.default_exterior
                .and_then(|id| lib.types.get(id))
                .map(|t| t.code.as_str()),
            Some("AW-31,5")
        );
        assert_eq!(
            lib.default_interior
                .and_then(|id| lib.types.get(id))
                .map(|t| t.code.as_str()),
            Some("IW-17,5")
        );

        // Start mit der alten Datei
        let d = test_dir("a125");
        let p = d.join("firmenkatalog.szk");
        std::fs::write(&p, SZK_K2).unwrap();
        let (c, h) = laden(&p);
        assert!(
            h.iter().any(|x| x.contains("Werkstypen")),
            "Hinweis Werkstypen ergänzt: {h:?}"
        );
        assert!(!h.iter().any(|x| x.contains("nicht lesbar")), "{h:?}");
        let b = c.library();
        assert_eq!(b.types.len(), 7, "sieben Werkstypen seit K5");
        assert_eq!(typ_code(b, "AW-31,5").unwrap().name, "AW 31,5 Büro");
        assert_eq!(
            baustoff(b, "Dämmung (WDVS)").lambda,
            Some(0.12),
            "Hand-λ bleibt"
        );
        assert_eq!(
            baustoff(b, "Gasbeton").lambda,
            Some(0.09),
            "fehlendes λ ergänzt"
        );
        assert_eq!(
            b.default_exterior
                .and_then(|id| b.types.get(id))
                .map(|t| t.code.as_str()),
            Some("AW-36")
        );
        let (c2, h2) = laden(&p);
        assert!(h2.is_empty(), "zweiter Start ohne Hinweis: {h2:?}");
        assert_eq!(c2.library(), c.library());
    }
}

// Abnahmetest A126: Firmenkatalog .szk aus einer neueren Fassung mit
// unbekannten Schlüsselwörtern (Festlegung Koordinator 19:24 nach Jörns
// Meldung 19:20). Der Leser toleriert sie: Baustoff mit Ersatzkategorie,
// ein Hinweis je Dateifassung (Statuszeile), unbekannte Zeilen und Schlüssel bleiben beim
// Zurückschreiben bytegleich erhalten.
// Spezifikation: test/abnahme-wandtypen.md (A126).
//
// Einbau: als `mod szk_unbekannt { use super::*; … }` ans Ende von
// app/src/abnahme.rs. Nutzt aus abnahme.rs: test_dir. Keine angenommenen
// Namen (read_szk, write_szk, Company::load).
mod szk_unbekannt {
    use super::*;

    use sk_model::catalog::{read_szk, write_szk, Library};
    use sk_model::{MatCategory, Model};

    /// Startbestand mit drei Zukunftsstellen: ein Baustoff „Stampflehm“ mit der
    /// erfundenen Kategorie `lehm`, ein unbekannter Schlüssel `sd=12` an der
    /// Gasbeton-Zeile und ein unbekannter Satz `[zukunft]`.
    fn zukunft() -> (String, [String; 3]) {
        let text = write_szk(&Library::standard());
        let gas = text
            .lines()
            .find(|l| l.starts_with("[material]") && l.contains("name=\"Gasbeton\""))
            .unwrap()
            .to_string();
        let alt_guid = gas
            .split(' ')
            .find_map(|t| t.strip_prefix("guid="))
            .unwrap()
            .to_string();
        let g = Model::with_seed(126).new_guid().to_string();
        let lehm = gas
            .replacen(&alt_guid, &g, 1)
            .replacen("name=\"Gasbeton\"", "name=\"Stampflehm\"", 1)
            .replacen("cat=masonry", "cat=lehm", 1);
        assert!(lehm.contains("cat=lehm"));
        let gas_sd = format!("{gas} sd=12");
        let satz = "[zukunft] guid=0000000000000000000126 wert=\"bleibt\"".to_string();
        let mut zeilen: Vec<String> = text
            .lines()
            .map(|l| {
                if l == gas {
                    gas_sd.clone()
                } else {
                    l.to_string()
                }
            })
            .collect();
        let pos = zeilen.iter().position(|l| l == &gas_sd).unwrap() + 1;
        zeilen.insert(pos, lehm.clone());
        zeilen.push(satz.clone());
        (zeilen.join("\n") + "\n", [lehm, gas_sd, satz])
    }

    /// Text des Hinweises in der Statuszeile (ohne Fachwörter). Gezählt wird
    /// jede unbekannte Angabe einmal: ein unbekannter Wert (cat=lehm), ein
    /// unbekannter Schlüssel (sd=12), ein unbekannter Satz ([zukunft]).
    fn hinweis(n: usize) -> String {
        format!("Firmenkatalog: {n} unbekannte Angaben übersprungen, alles andere ist geladen.")
    }

    fn alle_da(text: &str, zeilen: &[String; 3], wo: &str) {
        for z in zeilen {
            assert!(
                text.lines().any(|l| l == z),
                "{wo}: Zeile fehlt oder geändert: {z}"
            );
        }
    }

    /// A126: Die Zukunfts-.szk lädt (kein „nicht lesbar“, kein Rückfall auf den
    /// Startbestand). „Stampflehm“ ist da, mit Ersatzkategorie, Rohdichte und λ
    /// wie in der Datei. Ein Hinweis je Dateifassung: erster Start genau einer, zweiter Start mit unveränderter Datei keiner, nach einer Änderung der Datei wieder genau einer. Schreiben behält alle drei
    /// Zukunftszeilen bytegleich, auch nach einer Änderung an der Bibliothek;
    /// schreiben → lesen → schreiben ist stabil. Company::load lässt die Datei
    /// bytegleich (Startbestand vollständig, nichts zu ergänzen).
    #[test]
    fn a126_szk_mit_unbekannten_schluesselwoertern() {
        let (text, zeilen) = zukunft();
        let lib = read_szk(&text).unwrap_or_else(|e| panic!("lesbar: {e}"));
        assert_eq!(lib.types.len(), 7, "nichts verworfen");
        let lehm = lib
            .materials
            .iter()
            .find(|(_, x)| x.name == "Stampflehm")
            .expect("Baustoff mit unbekannter Kategorie bleibt")
            .1;
        let gas = lib
            .materials
            .iter()
            .find(|(_, x)| x.name == "Gasbeton")
            .unwrap()
            .1;
        assert!(MatCategory::ALL.contains(&lehm.category), "Ersatzkategorie");
        assert_eq!((lehm.density, lehm.lambda), (gas.density, gas.lambda));

        // Zurückschreiben, unverändert und nach einer Änderung
        let neu = write_szk(&lib);
        alle_da(&neu, &zeilen, "schreiben");
        assert_eq!(write_szk(&read_szk(&neu).unwrap()), neu, "stabil");
        let mut geaendert = lib.clone();
        let id = geaendert
            .types
            .iter()
            .find(|(_, t)| t.code == "AW-36")
            .map(|(id, _)| id)
            .unwrap();
        geaendert.types.get_mut(id).unwrap().name = "AW 36 Büro".into();
        let neu2 = write_szk(&geaendert);
        alle_da(&neu2, &zeilen, "nach Änderung");
        assert!(neu2.contains("name=\"AW 36 Büro\""));

        // Start mit der Datei: ein Hinweis je Dateifassung (Koordinator 19:44)
        let d = test_dir("a126");
        let p = d.join("firmenkatalog.szk");
        std::fs::write(&p, &text).unwrap();
        let start = |soll: Option<usize>, wo: &str, inhalt: &str| {
            let (c, h) = crate::catalog::Company::load(&p, true);
            match soll {
                Some(n) => assert_eq!(h, [hinweis(n)], "{wo}"),
                None => assert!(h.is_empty(), "{wo}: kein Hinweis {h:?}"),
            }
            assert_eq!(c.library().types.len(), 7, "{wo}");
            assert!(
                c.library()
                    .materials
                    .iter()
                    .any(|(_, x)| x.name == "Stampflehm"),
                "{wo}"
            );
            assert_eq!(
                std::fs::read_to_string(&p).unwrap(),
                inhalt,
                "{wo}: Datei bytegleich"
            );
        };
        start(Some(3), "erster Start", &text);
        start(None, "zweiter Start, Datei unverändert", &text);
        // Neue Fassung der Datei: eine weitere unbekannte Angabe
        let text2 = format!("{text}[zukunft] guid=0000000000000000000127 wert=\"neu\"\n");
        std::fs::write(&p, &text2).unwrap();
        start(Some(4), "nach Änderung der Datei", &text2);
        start(None, "danach wieder still", &text2);
    }
}

// Abnahmetest A127: Randdämmstreifen fugenlos dargestellt (K5, Jörn 16:43
// F4, BIM-Abnahme 6c8c188: „Ansicht vorne ohne Kante in Deckenhöhe, Schnitt
// mit Dämmschraffur ohne Trennlinie oben/unten“).
// Spezifikation: test/abnahme-wandtypen.md (A127), Handtest H110.
//
// Statt Bildpunkten prüft der Test das Netz, so wie es gezeichnet wird
// (view_mesh: Flächen mit Muster, Kanten mit Strichbreite). In der Cloud
// gibt es kein Windows-Bild; --screenshot zeichnet genau dieses Netz.
//
// Einbau: als `mod randstreifen_bild { use super::*; … }` ans Ende von
// app/src/abnahme.rs. Nutzt aus abnahme.rs: haus_b11, view_mesh, Shown,
// pattern, SectionLine, ViewKind. Keine angenommenen Namen.
mod randstreifen_bild {
    use super::*;

    use sk_model::{LayerSetId, Model, RunId};

    fn typ(m: &Model, code: &str) -> LayerSetId {
        m.layer_sets()
            .iter()
            .find(|(_, t)| t.code == code)
            .map(|(id, _)| id)
            .unwrap_or_else(|| panic!("Typ {code} fehlt"))
    }

    fn auf_typ(s: &mut Scene, run: RunId, code: &str) {
        let id = typ(s.model(), code);
        assert!(
            s.edit_model("Wandtyp geändert", |m| m.set_run_type(run, id)),
            "Wechsel auf {code}"
        );
    }

    /// Haus B11 (10 × 8 m) mit AW-36,5 (Streifen 12,5 cm vor 24 cm Auflager).
    fn haus() -> Scene {
        let mut s = Scene::with_model(Model::with_seed(127));
        let (aw, iw) = haus_b11(&mut s);
        auf_typ(&mut s, iw, "IW-17,5");
        auf_typ(&mut s, aw, "AW-36,5");
        s
    }

    /// Deckenhöhen, an denen Streifen liegen: UK/OK EG-Decke, UK OG-Decke.
    const DECKE: [f32; 3] = [2635.0, 2855.0, 5490.0];

    fn bei(a: f32, b: f32) -> bool {
        (a - b).abs() < 1.0
    }

    /// Kanten, die ganz in einer Fassadenebene liegen und waagerecht auf einer
    /// Deckenhöhe verlaufen.
    fn fassadenkanten_in_deckenhoehe(m: &Shown) -> Vec<([[f32; 3]; 2], f32)> {
        let ebene = |p: &[[f32; 3]; 2]| {
            (bei(p[0][1], 0.0) && bei(p[1][1], 0.0))
                || (bei(p[0][1], 8000.0) && bei(p[1][1], 8000.0))
                || (bei(p[0][0], 0.0) && bei(p[1][0], 0.0))
                || (bei(p[0][0], 10000.0) && bei(p[1][0], 10000.0))
        };
        m.edges
            .iter()
            .filter(|(p, _)| ebene(p) && DECKE.iter().any(|&z| bei(p[0][2], z) && bei(p[1][2], z)))
            .cloned()
            .collect()
    }

    /// Senkrechte Kanten im Schnitt bei (x, y) decken z0…z1 lückenlos ab, alle
    /// mit derselben Strichbreite.
    fn durchgehend(c: &Shown, y: f32, x: f32, z0: f32, z1: f32) -> bool {
        let mut st: Vec<(f32, f32, f32)> = c
            .edges
            .iter()
            .filter(|(p, _)| {
                bei(p[0][1], y) && bei(p[1][1], y) && bei(p[0][0], x) && bei(p[1][0], x)
            })
            .map(|(p, w)| (p[0][2].min(p[1][2]), p[0][2].max(p[1][2]), *w))
            .filter(|&(a, b, _)| b > z0 && a < z1)
            .collect();
        st.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
        let Some(&(a, _, w)) = st.first() else {
            return false;
        };
        if a > z0 {
            return false;
        }
        let mut bis = z0;
        for &(a, b, v) in &st {
            if a > bis + 1.0 || v != w {
                return false;
            }
            bis = bis.max(b);
        }
        bis >= z1
    }

    /// A127: Bei AW-36,5 zeigt keine Ansicht (vorne, hinten, links, rechts, 3D)
    /// eine Kante in Deckenhöhe auf der Fassade. Im Schnitt A–A liegen die
    /// Streifen (x 0…125 und 9875…10 000, je EG- und OG-Decke) in Dämmschraffur
    /// (Zickzack), ohne waagerechte Linie zur Wand darüber und darunter; die
    /// Außenkontur der Wand läuft durch, die Kontur der Decke bleibt.
    #[test]
    fn a127_randstreifen_fugenlos() {
        let mut s = haus();
        assert_eq!(s.model().edge_strip_pairs().len(), 8, "8 Streifen");
        for v in [
            ViewKind::Front,
            ViewKind::Back,
            ViewKind::Left,
            ViewKind::Right,
            ViewKind::Persp,
        ] {
            let m = view_mesh(&mut s, v, None);
            let k = fassadenkanten_in_deckenhoehe(&m);
            assert!(k.is_empty(), "{v:?}: Kante in Deckenhöhe {k:?}");
        }

        let mut sect = SectionLine::default();
        sect.ensure(&s);
        let c = view_mesh(&mut s, ViewKind::Section, sect.plane());
        let y = 4000.0;
        let im_schnitt = |p: &[[f32; 3]; 2]| bei(p[0][1], y) && bei(p[1][1], y);
        for (x0, x1) in [(0.0f32, 125.0f32), (9875.0, 10000.0)] {
            // keine Trennlinie oben/unten im Streifenbereich
            let trenn: Vec<_> = c
                .edges
                .iter()
                .filter(|(p, _)| {
                    im_schnitt(p)
                        && DECKE.iter().any(|&z| bei(p[0][2], z) && bei(p[1][2], z))
                        && p[0][0].min(p[1][0]) > x0 - 1.0
                        && p[0][0].max(p[1][0]) < x1 + 1.0
                })
                .collect();
            assert!(
                trenn.is_empty(),
                "Trennlinie am Streifen {x0}…{x1}: {trenn:?}"
            );
            // Streifenflächen in Dämmschraffur, EG und OG
            for (z0, z1) in [(2635.0f32, 2855.0f32), (5490.0, 5710.0)] {
                let tri: Vec<f32> = c
                    .faces
                    .chunks(3)
                    .filter(|t| {
                        let cx = (t[0][0] + t[1][0] + t[2][0]) / 3.0;
                        let cy = (t[0][1] + t[1][1] + t[2][1]) / 3.0;
                        let cz = (t[0][2] + t[1][2] + t[2][2]) / 3.0;
                        bei(cy, y) && cx > x0 && cx < x1 && cz > z0 && cz < z1
                    })
                    .map(|t| t[0][9])
                    .collect();
                assert!(!tri.is_empty(), "Streifen {x0}…{x1}, {z0}…{z1} im Schnitt");
                assert!(
                    tri.iter().all(|&p| p == pattern::ZIGZAG),
                    "Dämmschraffur: {tri:?}"
                );
            }
            // Außenkontur läuft über die Deckenhöhe hinweg durch: Teilstücke
            // stoßen lückenlos aneinander und haben dieselbe Strichbreite
            let aussen = if x0 == 0.0 { 0.0 } else { 10000.0 };
            assert!(
                durchgehend(&c, y, aussen, 2400.0, 3100.0),
                "Außenkontur bei x = {aussen} ohne Lücke und Breitenwechsel"
            );
            // Kontur der Decke an der Innenseite des Streifens bleibt
            let innen = if x0 == 0.0 { 125.0 } else { 9875.0 };
            assert!(
                c.edges.iter().any(|(p, _)| {
                    im_schnitt(p)
                        && bei(p[0][0], innen)
                        && bei(p[1][0], innen)
                        && p[0][2].min(p[1][2]) < 2635.0 + 1.0
                        && p[0][2].max(p[1][2]) > 2855.0 - 1.0
                }),
                "Deckenkontur bei x = {innen}"
            );
        }
    }
}

mod auflager_bearbeiten {
    use super::*;

    // Abnahmetest A128: Deckenauflager bearbeitbar (Umschalter, Tiefe, Baustoff
    // des Streifens), Koordinator 22:32. Spezifikation: test/abnahme-wandtypen.md
    // (A128), Handtest H111.
    //
    // Einbau: als `mod auflager_bearbeiten { use super::*; … }` ans Ende von
    // app/src/abnahme.rs. Nutzt aus abnahme.rs: haus_b11, og_zug, decke_mengen,
    // wall_m3, m3. Die Bedienung des Katalogs (OK → ein Schritt „Bauteilkatalog
    // geändert“) steht nur im Adapter `katalog`; baut der Bau dafür eine eigene
    // Funktion, wird nur der Adapter angepasst.
    //
    // Sollwerte auf 27213bd gerechnet und von Hand nachgeprüft (Haus B11,
    // 10 × 8 m, Geschoss 2,635 + Decke 0,22):
    // - Decke 9,63 × 7,63 = 73,4769 m², × 0,22 = 16,164918 m³
    // - Streifen 18,5 cm: Achsen 9,815 / 7,815 m, je Geschoss
    //   35,26 × 0,185 × 0,22 = 1,435082 m³; Decke + Streifen = 17,6
    // - Gasbeton netto 34,30 × 0,425 × 2,635 = 38,4117 m³ (Achse × Dicke ×
    //   lichte Höhe, Tasche voll abgezogen, Streifen nicht doppelt)
    // - IW-17,5: 7,15 × 0,175 × 2,635 = 3,2970 m³
    // - U = 1 / (0,13 + 0,425/0,09 + 0,04) = 0,2044 → 0,20

    use sk_model::catalog::{export_type, import_type, read_szk, write_szk, Library};
    use sk_model::{Bearing, Category, ElementId, GuidGen, LayerSet, LayerSetId, Model, RunId};

    // ===== Adapter =====

    /// Katalog-Dialog mit OK: eine Änderung an den Typen, ein Rückgängig-Schritt.
    fn katalog(s: &mut Scene, f: impl FnOnce(&mut Model) -> bool) -> bool {
        s.edit_types(crate::scene::CATALOG_STEP, f)
    }

    // ===== Hilfen =====

    fn typ(m: &Model, code: &str) -> LayerSetId {
        m.layer_sets()
            .iter()
            .find(|(_, t)| t.code == code)
            .map(|(id, _)| id)
            .unwrap_or_else(|| panic!("Typ {code} fehlt"))
    }

    fn r4(v: f64) -> f64 {
        (v * 1e4).round() / 1e4
    }

    fn zugtyp(s: &mut Scene, run: RunId, id: LayerSetId) -> bool {
        s.edit_model("Wandtyp geändert", |m| m.set_run_type(run, id))
    }

    /// Randdämmstreifen: (Nummer, Guid, Breite mm, Achslänge mm), nach Nummer.
    fn streifen(s: &Scene) -> Vec<(String, sk_model::Guid, f64, f64)> {
        let mut v: Vec<_> = s
            .model()
            .elements()
            .iter()
            .filter(|(_, e)| e.category == Category::EdgeInsulation)
            .map(|(id, e)| {
                let q = s.edge_strip_qto(id).expect("Streifenmengen");
                let breite = q.volume / q.length / 220.0;
                (e.number.clone(), e.guid, breite, q.length)
            })
            .collect();
        v.sort_by(|a, b| a.0.cmp(&b.0));
        v
    }

    fn streifen_m3(s: &Scene, ids: &[ElementId]) -> f64 {
        ids.iter()
            .map(|&el| s.edge_strip_qto(el).unwrap().volume)
            .sum::<f64>()
            / 1e9
    }

    fn gasbeton_m3(s: &Scene, run: RunId) -> f64 {
        let m = s.model();
        let mut v = 0.0;
        for i in 0..4 {
            for l in &s.wall_qto(m.wall_at(run, i).unwrap()).unwrap().layers {
                if m.material(l.material).unwrap().name == "Gasbeton" {
                    v += l.volume;
                }
            }
        }
        r4(v / 1e9)
    }

    fn auflager_mit(m: &Model, t: &LayerSet) -> Option<(f64, String)> {
        match t.bearing {
            Bearing::Core => None,
            Bearing::Depth { depth, strip } => {
                Some((depth, m.material(strip).unwrap().name.clone()))
            }
        }
    }

    /// Neuer Typ AW-42,5 (Gasbeton 42,5, Auflager 24 mit Randdämmung) über den
    /// Katalog: Duplizieren von AW-36,5, Dicke, Kürzel, Name, Tiefe, OK.
    fn neuer_typ(s: &mut Scene) -> LayerSetId {
        let mono = typ(s.model(), "AW-36,5");
        let mut neu = None;
        assert!(katalog(s, |m| {
            let Some(id) = m.duplicate_type(mono) else {
                return false;
            };
            let mut t = m.layer_set(id).unwrap().clone();
            t.layers[0].thickness = 425.0;
            t.code = "AW-42,5".into();
            t.name = "AW 42,5 Gasbeton monolithisch".into();
            let Bearing::Depth { strip, .. } = t.bearing else {
                return false;
            };
            t.bearing = Bearing::Depth {
                depth: 240.0,
                strip,
            };
            neu = Some(id);
            m.set_layer_set(id, t)
        }));
        neu.unwrap()
    }

    /// Ändert das Auflager eines Typs über den Katalog (ein Schritt).
    fn setze_auflager(s: &mut Scene, id: LayerSetId, tiefe: Option<f64>) -> bool {
        katalog(s, |m| {
            let mut t = m.layer_set(id).unwrap().clone();
            let strip = match t.bearing {
                Bearing::Depth { strip, .. } => strip,
                Bearing::Core => typ_strip(m),
            };
            t.bearing = match tiefe {
                None => Bearing::Core,
                Some(depth) => Bearing::Depth { depth, strip },
            };
            m.set_layer_set(id, t)
        })
    }

    /// Baustoff „Randdämmung“ (Streifen von AW-36,5).
    fn typ_strip(m: &Model) -> sk_model::MaterialId {
        match m.layer_set(typ(m, "AW-36,5")).unwrap().bearing {
            Bearing::Depth { strip, .. } => strip,
            Bearing::Core => panic!("AW-36,5 ohne Streifen"),
        }
    }

    /// A128: AW-42,5 (Gasbeton 42,5, Auflager 24, Streifen 18,5 aus
    /// Randdämmung) entsteht im Katalog in einem Rückgängig-Schritt, U 0,20.
    /// Am Haus B11: 8 Streifen 18,5 cm breit, Achsen 9,815/7,815, Decke
    /// 73,4769 m² / 16,1649 m³, Decke + Streifen = 17,6, Gasbeton 38,4117,
    /// IW 3,2970, SP/FS unverändert, `check()` leer, A-09. Tiefe 24 → 30 in einem
    /// Schritt (Streifen 12,5, gleiche Guids), Rückgängig zurück auf 18,5.
    /// Umschalter aus (ganzer Kern): keine Streifen, Decke 80 m² / 17,6;
    /// Rückgängig bringt dieselben Streifen. Rundlauf .szo und .szk behalten
    /// Tiefe und Baustoff des Streifens.
    #[test]
    fn a128_deckenauflager_bearbeiten() {
        let mut s = Scene::with_model(Model::with_seed(128));
        let (aw, iw) = haus_b11(&mut s);
        let i17 = typ(s.model(), "IW-17,5");
        assert!(zugtyp(&mut s, iw, i17));

        // Neuer Typ im Katalog: ein Schritt
        let typen = s.model().layer_sets().len();
        let id = neuer_typ(&mut s);
        assert_eq!(s.undo_label(), Some(crate::scene::CATALOG_STEP));
        let t = s.model().layer_set(id).unwrap().clone();
        assert_eq!((t.code.as_str(), t.thickness()), ("AW-42,5", 425.0));
        assert_eq!(
            auflager_mit(s.model(), &t),
            Some((240.0, "Randdämmung".to_string()))
        );
        assert_eq!(
            s.model().u_value(id).map(|u| (u * 100.0).round() / 100.0),
            Some(0.2)
        );
        assert!(s.undo());
        assert_eq!(s.model().layer_sets().len(), typen, "Rückgängig: Typ weg");
        assert!(s.redo());
        let id = typ(s.model(), "AW-42,5");

        // Am Haus
        assert!(zugtyp(&mut s, aw, id));
        let st = streifen(&s);
        assert_eq!(st.len(), 8);
        for (n, _, b, _) in &st {
            assert!((b - 185.0).abs() < 0.01, "{n} breit {b}");
        }
        let mut achsen: Vec<f64> = st.iter().map(|x| (x.3).round() / 1000.0).collect();
        achsen.sort_by(|a, b| a.partial_cmp(b).unwrap());
        assert_eq!(
            achsen,
            [7.815, 7.815, 7.815, 7.815, 9.815, 9.815, 9.815, 9.815]
        );
        let og = og_zug(&s, aw);
        let m = s.model();
        assert_eq!(m.edge_strip_pairs().len(), 8);
        for run in [aw, og] {
            let (f, v, _) = decke_mengen(&s, run);
            assert_eq!(f, 73.4769);
            assert!((v - 16.164918).abs() < 1e-3, "Decke {v}");
            let walls: Vec<ElementId> = (0..4).map(|i| m.wall_at(run, i).unwrap()).collect();
            let eigene: Vec<ElementId> = m
                .elements()
                .iter()
                .filter(|(_, e)| e.category == Category::EdgeInsulation)
                .filter(|(el, _)| streifen_wand(m, *el).is_some_and(|w| walls.contains(&w)))
                .map(|(el, _)| el)
                .collect();
            assert_eq!(eigene.len(), 4);
            for el in &eigene {
                let w = streifen_wand(m, *el).unwrap();
                assert_eq!(
                    m.element(*el).unwrap().storey,
                    m.element(w).unwrap().storey,
                    "A-09"
                );
            }
            let vs = streifen_m3(&s, &eigene);
            assert!((vs - 1.435082).abs() < 1e-4, "Streifen {vs}");
            assert!(
                (v + vs - 17.6).abs() < 1e-3,
                "Decke + Streifen = volle Tasche"
            );
            assert_eq!(gasbeton_m3(&s, run), 38.4117);
        }
        assert_eq!(r4(wall_m3(&s, iw, 0)), 3.297);
        assert!(s.model().check().is_empty(), "{:?}", s.model().check());

        // Tiefe 24 → 30: ein Schritt, gleiche Streifen (Guid), 12,5 breit
        let guids: Vec<_> = st.iter().map(|x| (x.0.clone(), x.1)).collect();
        assert!(setze_auflager(&mut s, id, Some(300.0)));
        assert_eq!(s.undo_label(), Some(crate::scene::CATALOG_STEP));
        let st2 = streifen(&s);
        assert_eq!(
            st2.iter().map(|x| (x.0.clone(), x.1)).collect::<Vec<_>>(),
            guids
        );
        assert!(st2.iter().all(|x| (x.2 - 125.0).abs() < 0.01), "12,5 breit");
        assert!(s.undo());
        assert!(
            streifen(&s).iter().all(|x| (x.2 - 185.0).abs() < 0.01),
            "zurück auf 18,5"
        );

        // Umschalter aus: ganzer Kern, keine Streifen
        assert!(setze_auflager(&mut s, id, None));
        assert!(streifen(&s).is_empty());
        let (f, v, _) = decke_mengen(&s, aw);
        assert_eq!(f, 80.0);
        assert!((v - 17.6).abs() < 1e-3);
        assert!(s.model().check().is_empty(), "{:?}", s.model().check());
        assert!(s.undo());
        assert_eq!(
            streifen(&s)
                .iter()
                .map(|x| (x.0.clone(), x.1))
                .collect::<Vec<_>>(),
            guids,
            "dieselben Streifen zurück"
        );

        // Rundlauf .szo
        let text = sk_model::szo::write(s.model());
        let l = sk_model::szo::read(&text, GuidGen::with_seed(1)).unwrap();
        assert!(l.hints.is_empty() && l.model.check().is_empty());
        assert_eq!(sk_model::szo::write(&l.model), text);
        let t2 = l.model.layer_set(typ(&l.model, "AW-42,5")).unwrap();
        assert_eq!(
            auflager_mit(&l.model, t2),
            Some((240.0, "Randdämmung".to_string()))
        );

        // Rundlauf .szk und Übernahme in ein anderes Projekt
        let g = s.model().layer_set(id).unwrap().guid;
        let mut lib = Library::default();
        assert!(export_type(s.model(), &mut lib, g));
        let szk = write_szk(&lib);
        let lib2 = read_szk(&szk).unwrap();
        assert_eq!(lib2, lib);
        assert_eq!(write_szk(&lib2), szk);
        let mut m2 = Model::with_seed(2);
        m2.allow_unstepped();
        let id2 = import_type(&mut m2, &lib2, g).expect("übernommen");
        let t3 = m2.layer_set(id2).unwrap();
        assert_eq!((t3.code.as_str(), t3.thickness()), ("AW-42,5", 425.0));
        assert_eq!(
            auflager_mit(&m2, t3),
            Some((240.0, "Randdämmung".to_string()))
        );
    }

    /// Wand eines Randdämmstreifens.
    fn streifen_wand(m: &Model, el: ElementId) -> Option<ElementId> {
        match m.element(el)?.kind {
            sk_model::ElementKind::EdgeStrip { wall, .. } => Some(wall),
            _ => None,
        }
    }
}

mod auflager_regel_21 {
    use super::*;

    // Abnahmetest A129: Grenzen des bearbeitbaren Deckenauflagers, Prüfregel 21
    // neu (BIM, bim/paket-k4-k5-wandtypen.md, „Nachtrag: Deckenauflager im
    // Katalog bearbeitbar“, „Fertig, wenn“). Spezifikation:
    // test/abnahme-wandtypen.md (A129), Handtest H111.
    //
    // Einbau: als `mod auflager_regel_21 { use super::*; … }` ans Ende von
    // app/src/abnahme.rs. Nutzt aus abnahme.rs: haus_b11, decke_mengen.
    // Angenommener Name nur im Adapter: `Model::remove_material` (Baustoff
    // löschen, `false` wenn benutzt).
    //
    // Prüfmuster wie A118: Ein unzulässiger Wert wird entweder von
    // `set_layer_set` abgelehnt oder von `check()` gemeldet.

    use sk_model::{Bearing, Category, GuidGen, LayerSet, LayerSetId, MaterialId, Model, RunId};

    // ===== Adapter =====

    /// Baustoff löschen; `false`, wenn er benutzt wird (Regel 15).
    fn baustoff_loeschen(m: &mut Model, id: MaterialId) -> bool {
        m.remove_material(id)
    }

    // ===== Hilfen =====

    fn typ(m: &Model, code: &str) -> LayerSetId {
        m.layer_sets()
            .iter()
            .find(|(_, t)| t.code == code)
            .map(|(id, _)| id)
            .unwrap_or_else(|| panic!("Typ {code} fehlt"))
    }

    fn baustoff(m: &Model, name: &str) -> MaterialId {
        m.materials()
            .iter()
            .find(|(_, x)| x.name == name)
            .map(|(id, _)| id)
            .unwrap_or_else(|| panic!("Baustoff {name} fehlt"))
    }

    /// Modell mit AW-42,5 (Duplikat von AW-36,5, Tiefe 24), ohne Rückgängig.
    fn mit_42_5() -> (Model, LayerSetId) {
        let mut m = Model::with_seed(129);
        m.allow_unstepped();
        let id = m.duplicate_type(typ(&m, "AW-36,5")).unwrap();
        let mut t = m.layer_set(id).unwrap().clone();
        t.layers[0].thickness = 425.0;
        t.code = "AW-42,5".into();
        assert!(m.set_layer_set(id, t));
        (m, id)
    }

    /// Wendet `t` auf eine Kopie an: angenommen und ohne Befund?
    fn sauber(m: &Model, id: LayerSetId, t: LayerSet) -> bool {
        let mut k = m.clone();
        k.allow_unstepped();
        k.set_layer_set(id, t) && k.check().is_empty()
    }

    fn mit_tiefe(m: &Model, id: LayerSetId, depth: f64, strip: MaterialId) -> LayerSet {
        let mut t = m.layer_set(id).unwrap().clone();
        t.bearing = Bearing::Depth { depth, strip };
        t
    }

    fn zugtyp(s: &mut Scene, run: RunId, id: LayerSetId) -> bool {
        s.edit_model("Wandtyp geändert", |m| m.set_run_type(run, id))
    }

    /// A129: Regel 21 neu. AW-42,5: Tiefe 9,5 und 41 abgelehnt oder gemeldet,
    /// 10 und 40,5 sauber (Bereich innen + 10 cm … Dicke − 2 cm). „Fest“ an AW-36
    /// (WDVS vor dem Kern) und an einer Innenwand (IW-24) nicht sauber.
    /// Streifenbaustoff Gasbeton (Mauerwerk) nicht sauber, Randdämmung sauber.
    /// Eine Datei mit Tiefe 40 an AW-36,5 lädt, der Wert bleibt, `check()`
    /// meldet, gezeichnet wie ganze tragende Schicht: keine Streifen, Decke
    /// 80 m². Randdämmung, nur als Streifenbaustoff benutzt, lässt sich nicht
    /// löschen.
    #[test]
    fn a129_auflager_grenzen_regel_21() {
        let (m, id) = mit_42_5();
        let rd = baustoff(&m, "Randdämmung");
        for d in [95.0, 410.0] {
            assert!(
                !sauber(&m, id, mit_tiefe(&m, id, d, rd)),
                "Tiefe {d} muss scheitern"
            );
        }
        for d in [100.0, 240.0, 405.0] {
            assert!(sauber(&m, id, mit_tiefe(&m, id, d, rd)), "Tiefe {d} geht");
        }
        // Streifenbaustoff nur Dämmung
        let gas = baustoff(&m, "Gasbeton");
        assert!(
            !sauber(&m, id, mit_tiefe(&m, id, 240.0, gas)),
            "Gasbeton als Streifen"
        );
        // „Fest“ nur ohne Schicht vor dem Kern und nur an Außenwänden
        let aw36 = typ(&m, "AW-36");
        assert!(
            !sauber(&m, aw36, mit_tiefe(&m, aw36, 240.0, rd)),
            "AW-36 mit WDVS außen"
        );
        let iw24 = typ(&m, "IW-24");
        assert!(
            !sauber(&m, iw24, mit_tiefe(&m, iw24, 140.0, rd)),
            "Innenwand"
        );

        // Randdämmung dient nur als Streifenbaustoff (AW-36,5): nicht löschbar
        let mut k = m.clone();
        k.allow_unstepped();
        assert!(
            !baustoff_loeschen(&mut k, rd),
            "Streifenbaustoff ist benutzt"
        );
        assert!(k.materials().get(rd).is_some());

        // Datei mit ungültiger Tiefe 40 an AW-36,5
        let mut s = Scene::with_model(Model::with_seed(129));
        let (aw, _) = haus_b11(&mut s);
        let mono = typ(s.model(), "AW-36,5");
        assert!(zugtyp(&mut s, aw, mono));
        let text = sk_model::szo::write(s.model());
        let g = s.model().layer_set(mono).unwrap().guid.to_string();
        let mut n = 0;
        let kaputt: String = text
            .lines()
            .map(|l| {
                if l.starts_with("[layerset]") && l.contains(&format!("guid={g}")) {
                    n += 1;
                    l.replacen("bearing=240", "bearing=400", 1)
                } else {
                    l.to_string()
                }
            })
            .collect::<Vec<_>>()
            .join("\n")
            + "\n";
        assert_eq!(n, 1);
        assert!(kaputt.contains("bearing=400"));
        let l = sk_model::szo::read(&kaputt, GuidGen::with_seed(1)).expect("lädt");
        assert!(!l.model.check().is_empty(), "Regel 21 gemeldet");
        assert!(
            sk_model::szo::write(&l.model).contains("bearing=400"),
            "Wert bleibt gespeichert"
        );
        let t = Scene::with_model(l.model);
        assert!(
            !t.model()
                .elements()
                .iter()
                .any(|(_, e)| e.category == Category::EdgeInsulation),
            "keine Streifen"
        );
        let (f, v, _) = decke_mengen(&t, aw);
        assert_eq!(f, 80.0, "Decke bis zur Kernaußenseite");
        assert!((v - 17.6).abs() < 1e-3);
    }
}

mod firmentyp_auflager {
    use super::*;
    // Abnahmetest A130: Firmentyp mit ungültigem Deckenauflager (Review 1p,
    // 3bd97c0). Ein Typ aus dem Firmenkatalog, dessen Auflager gegen Regel 21
    // verstößt (Tiefe 40 an 36,5 bzw. Gasbeton als Streifen), kommt beim
    // Übernehmen ins Projekt mit und wird gebaut wie „ganze tragende Schicht“,
    // genau wie dieselbe Angabe aus einer .szo (A129).
    // Spezifikation: test/abnahme-wandtypen.md (A130).
    //
    // Einbau: als `mod firmentyp_auflager { use super::*; … }` ans Ende von
    // app/src/abnahme.rs. Nutzt aus abnahme.rs: haus_b11, decke_mengen.
    // Keine angenommenen Namen: Library::standard, write_szk/read_szk,
    // import_type, Scene::edit_types(CATALOG_STEP), Model::set_run_type.

    use sk_model::catalog::{import_type, read_szk, write_szk, Library};
    use sk_model::{Category, Guid, GuidGen, LayerSetId, Model, RunId};

    fn zugtyp(s: &mut Scene, run: RunId, id: LayerSetId) -> bool {
        s.edit_model("Wandtyp geändert", |m| m.set_run_type(run, id))
    }

    /// Firmenkatalog mit Duplikat von AW-36,5 („AW-F“, Guid 0x130), dessen
    /// [layerset]-Zeile das Wort `ersetze` durch `durch` ersetzt bekommt; über den Text gelesen wie
    /// eine Datei vom Datenträger.
    fn firmenkatalog(ersetze: &str, durch: &str) -> (Library, Guid) {
        let mut lib = Library::standard();
        let mono = lib.type_by_guid(sk_model::MONO_TYPE_GUID).unwrap();
        let mut t = lib.types.get(mono).unwrap().clone();
        t.guid = Guid(0x130);
        t.code = "AW-F".into();
        t.name = "Firmentyp".into();
        lib.types.insert(t);
        let text = write_szk(&lib);
        let mut n = 0;
        let neu: String = text
            .lines()
            .map(|l| {
                if l.starts_with("[layerset]") && l.contains("code=\"AW-F\"") {
                    n += 1;
                    // ganzes Wort ersetzen; endet `ersetze` auf „=“, gilt jeder Wert
                    let mut k = 0;
                    let l = l
                        .split(' ')
                        .map(|w| {
                            if w == ersetze || (ersetze.ends_with('=') && w.starts_with(ersetze)) {
                                k += 1;
                                durch.to_string()
                            } else {
                                w.to_string()
                            }
                        })
                        .collect::<Vec<_>>()
                        .join(" ");
                    assert_eq!(k, 1, "{ersetze} genau einmal: {l}");
                    l
                } else {
                    l.to_string()
                }
            })
            .collect::<Vec<_>>()
            .join("\n")
            + "\n";
        assert_eq!(n, 1);
        (read_szk(&neu).expect("liest"), Guid(0x130))
    }

    /// A130: Firmentyp „AW-F“ mit Auflagertiefe 40 (Bereich 10–34,5) bzw. mit
    /// Gasbeton als Streifen: Übernehmen gelingt in einem Rückgängig-Schritt,
    /// Tiefe bleibt 40, `check()` meldet, an den Wänden gibt es keine Streifen,
    /// die Decke reicht bis zur Kernaußenseite (80 m², 17,6 m³), beim Speichern
    /// steht bearing=400 in der Datei. Strg+Z nimmt den Typ wieder heraus.
    #[test]
    fn a130_firmentyp_mit_ungueltigem_auflager() {
        let gasbeton = Model::new()
            .materials()
            .iter()
            .find(|(_, x)| x.name == "Gasbeton")
            .map(|(_, x)| x.guid.to_string())
            .unwrap();
        for (fall, ersetze, durch) in [
            (
                "Tiefe 40",
                "bearing=240".to_string(),
                "bearing=400".to_string(),
            ),
            (
                "Gasbeton als Streifen",
                "strip=".to_string(),
                format!("strip={gasbeton}"),
            ),
        ] {
            let mut s = Scene::with_model(Model::with_seed(130));
            let (aw, _) = haus_b11(&mut s);
            let (lib, g) = firmenkatalog(&ersetze, &durch);
            let typen = s.model().layer_sets().len();
            assert!(
                s.edit_types(crate::scene::CATALOG_STEP, |m| import_type(m, &lib, g)
                    .is_some()),
                "{fall}: Firmentyp kommt mit"
            );
            assert_eq!(s.model().layer_sets().len(), typen + 1, "{fall}");
            let id = s.model().type_by_guid(g).unwrap();
            assert!(
                s.model()
                    .bearing_problem(s.model().layer_set(id).unwrap())
                    .is_some(),
                "{fall}"
            );
            assert!(!s.model().check().is_empty(), "{fall}: Regel 21 gemeldet");
            assert!(zugtyp(&mut s, aw, id), "{fall}: Wände bekommen AW-F");
            assert!(
                !s.model()
                    .elements()
                    .iter()
                    .any(|(_, e)| e.category == Category::EdgeInsulation),
                "{fall}: keine Streifen"
            );
            let (f, v, _) = decke_mengen(&s, aw);
            assert_eq!(f, 80.0, "{fall}: Decke bis zur Kernaußenseite");
            assert!((v - 17.6).abs() < 1e-3, "{fall}: {v}");
            // Speichern und Öffnen: Angabe bleibt, Modell gleich
            let text = sk_model::szo::write(s.model());
            assert!(text.contains(&durch), "{fall}: Wert bleibt gespeichert");
            let l = sk_model::szo::read(&text, GuidGen::with_seed(1)).expect("lädt");
            assert_eq!(sk_model::szo::write(&l.model), text, "{fall}: Rundlauf");
            // Rückgängig: erst die Wände, dann der Typ
            assert!(s.undo());
            assert!(s.undo());
            assert!(
                s.model().type_by_guid(g).is_none(),
                "{fall}: Strg+Z nimmt den Typ heraus"
            );
        }
    }
}

// A131–A136: Bauteile und Gebäude löschen (Paket „Löschen“, V?-9)
mod loeschen {
    use super::*;

    // Abnahmetests A131–A136: Bauteile und Gebäude löschen (Paket „Löschen“).
    // Grundlage: bim/paket-loeschen.md (Regeln 24–27, „Fertig, wenn“),
    // einstellungen/paket-loeschen-gestaltung.md (Sätze), Jörns „Ja“ zu den
    // Sollbildern soll-loeschen-1…4 (07.10. 02:27).
    // Spezifikation: test/abnahme-loeschen.md. Handtests H112–H119.
    //
    // Einbau: als `mod loeschen { use super::*; … }` ans Ende von
    // app/src/abnahme.rs. Nutzt aus abnahme.rs: haus_b11, zeichne_rechteck,
    // cam3d, tool, click, key, W, H, aw_schicht, decke_mengen, m3, band,
    // test_dir, STANDARD.
    //
    // Angenommene Namen stehen nur in den Adaptern (Vorschlag aus dem Paket,
    // Abschnitt 6): `Scene::delete_elements`, `Model::can_delete`,
    // `sk_model::refusal_text`, `Scene::remove_building`,
    // `Model::building_part_count`, `crate::delete::hint`. Heißen sie im Bau
    // anders, ändert sich nur der Adapter.

    use crate::picking::Picking;
    use sk_model::{BuildingId, Category, ElementId, GuidGen, Model, RunId};

    // ===== Adapter =====

    /// Entf bzw. „Löschen“ im Kontextmenü mit der Auswahl `ids`: ein
    /// Rückgängig-Schritt, wenn etwas gelöscht wurde. Ergebnis: die gelöschten
    /// Bauteile und der Hinweis am Bauteil, Zeile für Zeile (leer: kein Hinweis).
    fn loeschen(s: &mut Scene, ids: &[ElementId]) -> (Vec<ElementId>, Vec<String>) {
        let d = s.delete_elements(ids);
        let hint = crate::delete::hint(s.model(), &d);
        (d.removed, hint)
    }

    /// Satz, warum `id` nicht löschbar ist (Tooltip am gedimmten „Löschen“);
    /// `None`: löschbar. Zwei Sätze stehen mit Leerzeichen hintereinander.
    fn ablehnung(m: &Model, id: ElementId) -> Option<String> {
        m.can_delete(id)
            .err()
            .map(|r| sk_model::refusal_text(m, id, &r))
    }

    /// „Gebäude löschen …“ und in der Rückfrage „Löschen“.
    fn gebaeude_loeschen(s: &mut Scene, b: BuildingId) -> bool {
        s.remove_building(b)
    }

    /// Zahl der Bauteile, die die Rückfrage nennt.
    fn gebaeude_bauteile(m: &Model, b: BuildingId) -> usize {
        m.building_part_count(b)
    }

    // ===== Hilfen =====

    /// Bauteil mit der Nummer `nr` („IW-001“).
    fn nr(s: &Scene, nr: &str) -> ElementId {
        s.model()
            .elements()
            .iter()
            .find(|(_, e)| e.number == nr)
            .map(|(id, _)| id)
            .unwrap_or_else(|| panic!("{nr} fehlt"))
    }

    fn hat(s: &Scene, n: &str) -> bool {
        s.model().elements().iter().any(|(_, e)| e.number == n)
    }

    fn nummer(s: &Scene, id: ElementId) -> String {
        s.model().element(id).unwrap().number.clone()
    }

    fn segmente(s: &Scene, run: RunId) -> Vec<String> {
        s.model()
            .run(run)
            .unwrap()
            .segments
            .iter()
            .map(|e| nummer(s, *e))
            .collect()
    }

    /// Innenwandzug mit dem Werkzeug durch `punkte`, Enter beendet.
    fn iw_zug(s: &mut Scene, punkte: &[(f64, f64)]) -> RunId {
        let c = cam3d();
        let set = s.model().defaults().interior_wall;
        let mut t = tool(s);
        t.set_category(Category::InteriorWall, s.model().wall_layers(set));
        for &(x, y) in punkte {
            click(&mut t, &c, vec3(x, y, 0.0));
        }
        let w = t.handle(&key(Key::Enter), &c, W, H, 1.0).commit.unwrap();
        s.add_wall_as(&w, Category::InteriorWall).unwrap()
    }

    /// Offener Außenwandzug neben dem Haus (kein Gebäudeumriss).
    fn offener_zug(s: &mut Scene) -> RunId {
        let c = cam3d();
        let mut t = tool(s);
        click(&mut t, &c, vec3(14000.0, 0.0, 0.0));
        click(&mut t, &c, vec3(14000.0, 5000.0, 0.0));
        let w = t.handle(&key(Key::Enter), &c, W, H, 1.0).commit.unwrap();
        assert!(!w.closed);
        s.add_wall(&w).unwrap()
    }

    fn wand_m3(s: &Scene, id: ElementId) -> f64 {
        s.wall_qto(id).unwrap().volume / 1e9
    }

    /// Mengen des Prüfhauses ohne die Innenwand (Dämmung, Gasbeton, Decke,
    /// Sohlplatte, Frostschürze), gerundet wie `mengen_b11`.
    fn mengen_ohne_iw(s: &Scene, aw: RunId) -> [f64; 5] {
        let r = |v: f64| (v * 1e4).round() / 1e4;
        let (sp, fs) = s.foundation_qto(aw).unwrap();
        [
            r(aw_schicht(s, aw, 0)),
            r(aw_schicht(s, aw, 1)),
            decke_mengen(s, aw).1,
            m3(sp.volume),
            m3(fs.volume),
        ]
    }

    fn sauber(s: &Scene, ctx: &str) {
        assert!(
            s.model().check().is_empty(),
            "{ctx}: {:?}",
            s.model().check()
        );
    }

    fn zeilen(v: &[&str]) -> Vec<String> {
        v.iter().map(|z| z.to_string()).collect()
    }

    // ===== Tests =====

    /// A131 („Fertig, wenn“: Innenwand löschen, Mengenfenster): Am Prüfhaus
    /// verschwindet IW-001 in einem Schritt „Bauteil gelöscht“. Außenwand,
    /// Decke, Sohlplatte und Frostschürze behalten die Mengen aus A39; die
    /// Innenwand fehlt in der Liste und in der .csv. Ist sie im Mengenfenster
    /// gewählt und unter der Maus, fällt sie ohne Fehler heraus. Kein Hinweis.
    /// Rückgängig bringt IW-001 mit derselben Guid und denselben Mengen zurück,
    /// Wiederholen löscht sie wieder. Regeln 24–27 sauber.
    #[test]
    fn a131_innenwand_loeschen() {
        let mut s = Scene::with_model(Model::with_seed(131));
        let (aw, iw) = haus_b11(&mut s);
        let vorher = mengen_ohne_iw(&s, aw);
        assert_eq!(
            vorher,
            [
                STANDARD[0],
                STANDARD[1],
                STANDARD[3],
                STANDARD[4],
                STANDARD[5]
            ],
            "Prüfhaus wie A39"
        );
        let id = nr(&s, "IW-001");
        let guid = s.model().element(id).unwrap().guid;
        let text_vorher = sk_model::szo::write(s.model());
        assert_eq!(ablehnung(s.model(), id), None, "Innenwand ist löschbar");
        let mut p = Picking::default();
        p.click(id, false);
        p.set_hover(Some(id), vec![id]);

        let (weg, hinweis) = loeschen(&mut s, &[id]);
        assert_eq!(weg, vec![id]);
        assert!(
            hinweis.is_empty(),
            "ein Bauteil, nichts abgelehnt: {hinweis:?}"
        );
        assert_eq!(s.undo_label(), Some("Bauteil gelöscht"));
        assert!(s.model().element(id).is_none());
        assert!(
            s.model().run(iw).is_none(),
            "Zug mit einem Segment verschwindet"
        );
        assert_eq!(mengen_ohne_iw(&s, aw), vorher, "Nachbarn unverändert");
        sauber(&s, "nach dem Löschen");
        // Mengenfenster: Zeile weg, Auswahl und Hover ohne Fehler bereinigt
        let liste = s.schedule().clone();
        let csv =
            String::from_utf8_lossy(&crate::schedule_view::csv(s.model(), &liste)).to_string();
        assert!(!csv.contains("IW-001"), "{csv}");
        p.validate(&s);
        assert!(p.selected.is_empty() && p.hover.is_none() && p.hover_group.is_empty());

        assert!(s.undo());
        let id2 = nr(&s, "IW-001");
        assert_eq!(s.model().element(id2).unwrap().guid, guid, "gleiche Guid");
        assert_eq!(
            sk_model::szo::write(s.model()),
            text_vorher,
            "alles wie vorher"
        );
        assert_eq!(mengen_ohne_iw(&s, aw), vorher);
        assert!(s.redo());
        assert!(!hat(&s, "IW-001"), "Wiederholen löscht wieder");
        sauber(&s, "nach Wiederholen");
    }

    /// A132 (Regel 25, „Nummer bleibt vergeben“): IW-001 löschen, speichern,
    /// öffnen, neue Innenwand zeichnen: Sie heißt IW-002. Die Datei nennt den
    /// Zähler; eine ältere Datei ohne Zähler lädt wie bisher (höchste vorhandene
    /// Nummer). Ein Zug mit drei Segmenten IW-002…IW-004, alle drei gelöscht:
    /// ein Schritt „3 Bauteile gelöscht“, der Zug verschwindet; die nächste
    /// Innenwand heißt IW-005.
    #[test]
    fn a132_nummern_werden_nie_neu_vergeben() {
        let mut s = Scene::with_model(Model::with_seed(132));
        haus_b11(&mut s);
        let id = nr(&s, "IW-001");
        loeschen(&mut s, &[id]);
        let text = sk_model::szo::write(s.model());
        let d = test_dir("a132");
        let p = d.join("haus.szo");
        std::fs::write(&p, &text).unwrap();
        let l = crate::document::load(&p).expect("öffnet");
        let mut s = Scene::with_model(l.model);
        sauber(&s, "nach dem Laden");
        let neu = iw_zug(&mut s, &[(5000.0, 0.0), (5000.0, 8000.0)]);
        assert_eq!(segmente(&s, neu), ["IW-002"], "IW-001 bleibt vergeben");

        // Drei Segmente auf einmal
        let z = iw_zug(
            &mut s,
            &[
                (1000.0, 1500.0),
                (1000.0, 3000.0),
                (3500.0, 3000.0),
                (3500.0, 6500.0),
            ],
        );
        assert_eq!(segmente(&s, z), ["IW-003", "IW-004", "IW-005"]);
        let alle = s.model().run(z).unwrap().segments.clone();
        let (weg, _) = loeschen(&mut s, &alle);
        assert_eq!(weg.len(), 3);
        assert_eq!(s.undo_label(), Some("3 Bauteile gelöscht"));
        assert!(s.model().run(z).is_none(), "ganzer Zug weg");
        sauber(&s, "Zug gelöscht");
        let n = iw_zug(&mut s, &[(7500.0, 0.0), (7500.0, 8000.0)]);
        assert_eq!(segmente(&s, n), ["IW-006"]);
        // Rundlauf: der Zähler übersteht Speichern und Öffnen erneut
        let text = sk_model::szo::write(s.model());
        let l = sk_model::szo::read(&text, GuidGen::with_seed(1)).expect("lädt");
        assert_eq!(sk_model::szo::write(&l.model), text, "Rundlauf");
    }

    /// A133 (Regel 11, 26: Teilen und Kürzen): Innenwandzug IW-002…IW-004 im
    /// Prüfhaus. Mittleres Segment löschen: zwei Züge, der erste behält RunId
    /// und Guid des Zuges mit IW-002, der zweite hat eine neue RunId und Guid mit
    /// IW-004 (seg 0); Wände behalten Guid und Nummer; Bezugsseite, Typ,
    /// Höhenbezug und Geschoss gleich; Punkte passend. Rückgängig: ein Zug mit
    /// drei Segmenten und derselben Guid. Endsegment löschen: ein Zug mit zwei
    /// Segmenten, RunId bleibt. Anfang und Ende zusammen gelöscht: der Rest
    /// IW-003 bleibt als ein Zug.
    #[test]
    fn a133_zug_teilen_und_kuerzen() {
        let pkt = [
            (1000.0, 1500.0),
            (1000.0, 3000.0),
            (3500.0, 3000.0),
            (3500.0, 6500.0),
        ];
        let mut s = Scene::with_model(Model::with_seed(133));
        haus_b11(&mut s);
        let z = iw_zug(&mut s, &pkt);
        assert_eq!(segmente(&s, z), ["IW-002", "IW-003", "IW-004"]);
        let alt = s.model().run(z).unwrap().clone();
        let guid_wand = |s: &Scene, n: &str| s.model().element(nr(s, n)).unwrap().guid;
        let g2 = guid_wand(&s, "IW-002");
        let g4 = guid_wand(&s, "IW-004");
        let zuege_vorher = s.model().runs().len();

        // Mitte
        let ids = [nr(&s, "IW-003")];
        let (weg, _) = loeschen(&mut s, &ids);
        assert_eq!(weg.len(), 1);
        assert_eq!(s.undo_label(), Some("Bauteil gelöscht"));
        assert_eq!(s.model().runs().len(), zuege_vorher + 1, "geteilt");
        let erster = s.model().run(z).expect("erster Teil behält die RunId");
        assert_eq!(erster.guid, alt.guid, "… und die Guid des Zuges");
        assert_eq!(segmente(&s, z), ["IW-002"]);
        assert_eq!(erster.points.len(), 2);
        let zweit_id = s.model().segment_of(nr(&s, "IW-004")).unwrap().0;
        assert_ne!(zweit_id, z);
        let zweiter = s.model().run(zweit_id).unwrap();
        assert_ne!(zweiter.guid, alt.guid, "zweiter Teil: neue Guid");
        assert_eq!(segmente(&s, zweit_id), ["IW-004"]);
        assert_eq!(
            s.model().segment_of(nr(&s, "IW-004")).unwrap().1,
            0,
            "seg ab 0"
        );
        assert_eq!(zweiter.points.len(), 2);
        for t in [erster, zweiter] {
            assert_eq!(t.ref_side, alt.ref_side);
            assert_eq!(t.base, alt.base);
            assert_eq!(t.top, alt.top);
            assert_eq!(t.storey, alt.storey);
            assert!(!t.closed);
        }
        for (a, b) in [
            (erster.points[0], alt.points[0]),
            (erster.points[1], alt.points[1]),
        ]
        .into_iter()
        .chain([
            (zweiter.points[0], alt.points[2]),
            (zweiter.points[1], alt.points[3]),
        ]) {
            assert!((a - b).length() < 1e-6, "Punkte bleiben: {a:?} {b:?}");
        }
        assert_eq!(guid_wand(&s, "IW-002"), g2);
        assert_eq!(guid_wand(&s, "IW-004"), g4);
        let typ = |s: &Scene, n: &str| s.model().element(nr(s, n)).unwrap().layer_set;
        assert_eq!(typ(&s, "IW-002"), typ(&s, "IW-004"));
        sauber(&s, "geteilt");
        assert!(s.undo());
        assert_eq!(s.model().runs().len(), zuege_vorher);
        assert_eq!(s.model().run(z).unwrap().guid, alt.guid);
        assert_eq!(segmente(&s, z), ["IW-002", "IW-003", "IW-004"]);

        // Ende
        let ids = [nr(&s, "IW-004")];
        loeschen(&mut s, &ids);
        assert_eq!(s.model().runs().len(), zuege_vorher, "nicht geteilt");
        assert_eq!(segmente(&s, z), ["IW-002", "IW-003"]);
        assert_eq!(s.model().run(z).unwrap().points.len(), 3);
        sauber(&s, "gekürzt");
        assert!(s.undo());

        // Anfang und Ende in einem Gang
        let ids = [nr(&s, "IW-002"), nr(&s, "IW-004")];
        let (weg, _) = loeschen(&mut s, &ids);
        assert_eq!(weg.len(), 2);
        assert_eq!(s.undo_label(), Some("2 Bauteile gelöscht"));
        assert_eq!(s.model().runs().len(), zuege_vorher);
        let rest = s.model().segment_of(nr(&s, "IW-003")).unwrap();
        assert_eq!(rest.1, 0);
        assert_eq!(segmente(&s, rest.0), ["IW-003"]);
        sauber(&s, "Rest");
    }

    /// A134 („Anschluss frei“): Innenwand von (2,0|4,0) bis an IW-001 bei
    /// x = 5 m (T-Anschluss). IW-001 löschen: Die erste behält ihre Punkte, kein
    /// Anschluss verweist mehr auf IW-001, ihr Ende ist frei und rechtwinklig:
    /// gleiches Volumen wie dieselbe Wand frei im Haus ohne IW-001.
    #[test]
    fn a134_anschluss_wird_frei() {
        use sk_model::join::JoinKind;
        let mut s = Scene::with_model(Model::with_seed(134));
        haus_b11(&mut s);
        let host = nr(&s, "IW-001");
        let z = iw_zug(&mut s, &[(2000.0, 4000.0), (5000.0, 4000.0)]);
        let w = s.model().wall_at(z, 0).unwrap();
        assert!(
            s.model()
                .joins()
                .iter()
                .any(|j| j.a == w && j.b == host && j.kind == JoinKind::T),
            "T-Anschluss an IW-001: {:?}",
            s.model().joins()
        );
        let punkte = s.model().run(z).unwrap().points.clone();
        loeschen(&mut s, &[host]);
        assert_eq!(s.model().run(z).unwrap().points, punkte, "Endpunkt bleibt");
        assert!(
            s.model().joins().iter().all(|j| j.a != host && j.b != host),
            "{:?}",
            s.model().joins()
        );
        sauber(&s, "Anschluss frei");

        let mut r = Scene::with_model(Model::with_seed(134));
        zeichne_rechteck(&mut r, &cam3d());
        let rz = iw_zug(&mut r, &[(2000.0, 4000.0), (5000.0, 4000.0)]);
        let soll = wand_m3(&r, r.model().wall_at(rz, 0).unwrap());
        let ist = wand_m3(&s, w);
        assert!(
            (ist - soll).abs() < 1e-6,
            "rechtwinkliges Ende: {ist} statt {soll}"
        );
    }

    /// A135 (Abschnitt 1 und 3, abgelehnt und gemischt): AW-001, eine OG-Außenwand,
    /// DE-001, SP-001, FS-001 und RD-001 (Haus in AW-36,5) sind nicht löschbar.
    /// Entf löscht nichts, der Hinweis nennt den Satz aus der Tabelle, kein
    /// Rückgängig-Schritt, das Modell bleibt unverändert (Revision gleich).
    /// Gemischt IW-001 + AW-001 + DE-001: nur IW-001 weg, Hinweis „1 Innenwand
    /// gelöscht.“ / „Außenwände und Decken bleiben, sie gehören zum
    /// Gebäudeumriss.“ / Verweis, ein Schritt „Bauteil gelöscht“. IW + AW:
    /// „Die Außenwand bleibt, sie gehört zum Gebäudeumriss.“ Eine Wand eines
    /// offenen Außenwandzugs ist löschbar; mit einer Innenwand zusammen heißt
    /// Zeile 1 „2 Wände gelöscht.“
    #[test]
    fn a135_abgelehnt_und_gemischt() {
        let aw_satz = [
            "Außenwände gehören zum Gebäudeumriss.",
            "Zum Entfernen den Umriss ändern oder das ganze Gebäude löschen.",
        ];
        let de_satz = [
            "Die Decke ergibt sich aus dem Gebäudeumriss und bleibt, solange das Gebäude steht.",
        ];
        let gr_satz = ["Sohlplatte und Frostschürze folgen dem Umriss des Erdgeschosses."];
        let rd_satz =
            ["Der Randdämmstreifen gehört zum Wandtyp; zum Entfernen den Wandtyp ändern."];

        let mut s = Scene::with_model(Model::with_seed(135));
        let (aw, _) = haus_b11(&mut s);
        let mono = s
            .model()
            .layer_sets()
            .iter()
            .find(|(_, t)| t.code == "AW-36,5")
            .map(|(id, _)| id)
            .unwrap();
        assert!(s.edit_model("Wandtyp geändert", |m| m.set_run_type(aw, mono)));
        let og_aw = s
            .model()
            .elements()
            .iter()
            .find(|(_, e)| {
                e.category == Category::ExteriorWall
                    && s.model().storey(e.storey).unwrap().short == "OG"
            })
            .map(|(id, _)| id)
            .expect("OG-Außenwand");
        let og_nr = nummer(&s, og_aw);
        for (n, satz) in [
            ("AW-001", &aw_satz[..]),
            (og_nr.as_str(), &aw_satz[..]),
            ("DE-001", &de_satz[..]),
            ("SP-001", &gr_satz[..]),
            ("FS-001", &gr_satz[..]),
            ("RD-001", &rd_satz[..]),
        ] {
            let id = nr(&s, n);
            assert_eq!(
                ablehnung(s.model(), id),
                Some(satz.join(" ")),
                "{n}: Tooltip"
            );
            let rev = s.model().revision();
            let label = s.undo_label();
            let (weg, hinweis) = loeschen(&mut s, &[id]);
            assert!(weg.is_empty(), "{n}");
            assert_eq!(hinweis, zeilen(satz), "{n}: Hinweis am Bauteil");
            assert_eq!(s.model().revision(), rev, "{n}: Datei unverändert");
            assert_eq!(s.undo_label(), label, "{n}: kein Rückgängig-Schritt");
            assert!(hat(&s, n));
        }
        // Alles Abgeleitete zusammen: weiterhin nichts gelöscht, ein Satz
        let alle: Vec<_> = ["AW-001", "DE-001", "SP-001"]
            .iter()
            .map(|n| nr(&s, n))
            .collect();
        let rev = s.model().revision();
        let (weg, hinweis) = loeschen(&mut s, &alle);
        assert!(weg.is_empty());
        assert!(
            !hinweis.is_empty() && hinweis.len() <= 2,
            "ein Satz, keine Liste: {hinweis:?}"
        );
        assert_eq!(s.model().revision(), rev);

        // Gemischt
        let iw = nr(&s, "IW-001");
        let sel = [iw, nr(&s, "AW-001"), nr(&s, "DE-001")];
        let (weg, hinweis) = loeschen(&mut s, &sel);
        assert_eq!(weg, vec![iw]);
        assert_eq!(
            hinweis,
            zeilen(&[
                "1 Wand gelöscht.",
                "Außenwände und Decken bleiben, sie gehören zum Gebäudeumriss."
            ])
        );
        assert_eq!(s.undo_label(), Some("Bauteil gelöscht"), "ein Schritt");
        assert!(hat(&s, "AW-001") && hat(&s, "DE-001"));
        sauber(&s, "gemischt");
        assert!(s.undo());
        let iw = nr(&s, "IW-001");
        let ids = [iw, nr(&s, "AW-001")];
        let (_, hinweis) = loeschen(&mut s, &ids);
        assert_eq!(
            hinweis,
            zeilen(&[
                "1 Wand gelöscht.",
                "Die Außenwand bleibt, sie gehört zum Gebäudeumriss."
            ])
        );
        assert!(s.undo());

        // Offener Außenwandzug: löschbar wie eine Innenwand
        let oz = offener_zug(&mut s);
        let ow = s.model().wall_at(oz, 0).unwrap();
        assert_eq!(ablehnung(s.model(), ow), None, "offener Zug löschbar");
        let iw = nr(&s, "IW-001");
        let ids = [iw, ow, nr(&s, "AW-001")];
        let (weg, hinweis) = loeschen(&mut s, &ids);
        assert_eq!(weg.len(), 2);
        assert_eq!(
            hinweis.first().map(String::as_str),
            Some("2 Wände gelöscht.")
        );
        assert!(s.model().run(oz).is_none());
        assert_eq!(s.undo_label(), Some("2 Bauteile gelöscht"));
        sauber(&s, "offener Zug");
    }

    /// A136 (Abschnitt 4, Regel 24/27): „Gebäude löschen“. Die Rückfrage nennt
    /// 13 Bauteile (8 Außenwände, 1 Innenwand, 2 Decken, Sohlplatte,
    /// Frostschürze). Danach: kein Bauteil, kein Zug, kein Anschluss, kein
    /// Gebäude; die Geschosse bleiben als Vorlage (building = None) mit
    /// denselben Bändern; Typen, Baustoffe und Standardtypen bleiben; `check()`
    /// leer; das Werkzeug „Gebäude“ öffnet wieder den Dialog (keine Gebäude).
    /// Ein Schritt „Gebäude gelöscht“; Rückgängig stellt die Datei bytegleich
    /// her, Wiederholen löscht wieder. Ein neues Gebäude heißt GB-02, seine
    /// Außenwände beginnen bei AW-009 (Nummern nie neu), die Geschosse haben
    /// dieselben Höhen.
    #[test]
    fn a136_gebaeude_loeschen() {
        let mut s = Scene::with_model(Model::with_seed(136));
        haus_b11(&mut s);
        let (b, gb) = s
            .model()
            .buildings()
            .iter()
            .map(|(id, b)| (id, b.number.clone()))
            .next()
            .expect("Gebäude");
        assert_eq!(gb, "GB-01");
        assert_eq!(
            gebaeude_bauteile(s.model(), b),
            13,
            "Rückfrage nennt die Bauteile"
        );
        assert_eq!(s.model().elements().len(), 13);
        let baender: Vec<_> = ["GR", "EG", "OG"].iter().map(|k| band(&s, k)).collect();
        assert_eq!(baender[1], (0.0, 2855.0), "Standardhöhen EG");
        assert_eq!(baender[2], (2855.0, 5710.0), "Standardhöhen OG");
        let typen = s.model().layer_sets().len();
        let stoffe = s.model().materials().len();
        let std = *s.model().defaults();
        let text = sk_model::szo::write(s.model());

        assert!(gebaeude_loeschen(&mut s, b));
        assert_eq!(s.undo_label(), Some("Gebäude gelöscht"));
        let m = s.model();
        assert!(m.elements().is_empty(), "kein Bauteil");
        assert_eq!(m.runs().len(), 0);
        assert!(m.joins().is_empty());
        assert!(
            m.buildings().is_empty(),
            "Werkzeug Gebäude öffnet den Dialog"
        );
        assert!(
            m.storeys().iter().all(|(_, st)| st.building.is_none()),
            "Vorlage"
        );
        let nach: Vec<_> = ["GR", "EG", "OG"].iter().map(|k| band(&s, k)).collect();
        assert_eq!(nach, baender, "Höhen bleiben als Vorlage");
        assert_eq!(s.model().layer_sets().len(), typen);
        assert_eq!(s.model().materials().len(), stoffe);
        assert_eq!(s.model().defaults(), &std);
        sauber(&s, "Gebäude gelöscht");

        assert!(s.undo());
        assert_eq!(
            sk_model::szo::write(s.model()),
            text,
            "Rückgängig: alles zurück"
        );
        assert!(s.redo());
        assert!(s.model().elements().is_empty());

        // Neues Gebäude: GB-02, Nummern laufen weiter, gleiche Höhen
        let aw = zeichne_rechteck(&mut s, &cam3d());
        let gb2: Vec<_> = s
            .model()
            .buildings()
            .iter()
            .map(|(_, b)| b.number.clone())
            .collect();
        assert_eq!(gb2, ["GB-02"]);
        assert_eq!(nummer(&s, s.model().wall_at(aw, 0).unwrap()), "AW-009");
        let neu: Vec<_> = ["GR", "EG", "OG"].iter().map(|k| band(&s, k)).collect();
        assert_eq!(neu, baender);
        sauber(&s, "neues Gebäude");
        // Auch nach Speichern und Öffnen
        let t = sk_model::szo::write(s.model());
        let l = sk_model::szo::read(&t, GuidGen::with_seed(1)).expect("lädt");
        assert_eq!(sk_model::szo::write(&l.model), t, "Rundlauf");
    }
}

// Löschen in der Oberfläche (V?-9, Gestaltung Fassung 1): Kontextmenü,
// Hinweis am Bauteil, Rückfrage „Gebäude löschen“ (Handtests H112–H118).
mod loeschen_oberflaeche {
    use super::*;
    use crate::delete::{self, Action, Answer, ConfirmCard, ContextMenu, HintCard, Link};
    use sk_model::{Category, ElementId};
    use sk_ui::widgets::{Fonts, Rect};
    use std::time::{Duration, Instant};

    fn fonts() -> Fonts {
        Fonts {
            regular: None,
            bold: None,
            italic: None,
        }
    }

    fn erstes(s: &Scene, c: Category) -> ElementId {
        s.model()
            .elements()
            .iter()
            .find(|(_, e)| e.category == c)
            .map(|(id, _)| id)
            .unwrap()
    }

    /// H112, H115: Reihenfolge der Zeilen, „Löschen“ an der Außenwand
    /// gedimmt mit dem Satz als Tooltip, „Gebäude löschen …“ aktiv.
    #[test]
    fn kontextmenue_zeilen_und_gedimmtes_loeschen() {
        let mut s = Scene::with_model(Model::with_seed(112));
        haus_b11(&mut s);
        let (t, m) = (Theme::dark(), s.model());
        let iw = erstes(&s, Category::InteriorWall);
        let c = ContextMenu::new(m, iw, &[iw], 300.0, 300.0, (1280, 800, 32), &t, 1.0);
        let zeilen: Vec<(String, bool)> = c.actions().into_iter().map(|(l, e, _)| (l, e)).collect();
        let namen: Vec<&str> = zeilen.iter().map(|z| z.0.as_str()).collect();
        assert_eq!(
            namen,
            [
                "Wandtyp ändern …",
                "Eigenschaften",
                "",
                "Löschen",
                "",
                "Gebäude löschen …"
            ]
        );
        assert!(
            zeilen.iter().all(|z| z.1 || z.0.is_empty()),
            "alles aktiv an der Innenwand"
        );
        assert_eq!(c.tip(), None);

        let aw = erstes(&s, Category::ExteriorWall);
        let mut c = ContextMenu::new(m, aw, &[aw], 1270.0, 790.0, (1280, 800, 32), &t, 1.0);
        let r = c.rect(&t, 1.0);
        assert!(
            r.x + r.w <= 1280.0 && r.y + r.h <= 800.0,
            "passt ins Fenster"
        );
        let a = c.actions();
        assert_eq!(a[3], ("Löschen".into(), false, Some(Action::Delete)));
        assert_eq!(
            a[5],
            (
                "Gebäude löschen …".into(),
                true,
                Some(Action::DeleteBuilding)
            )
        );
        // Pfeil nach unten überspringt das gedimmte „Löschen“
        let mut wahl = Vec::new();
        for _ in 0..4 {
            c.key(Key::Other(0x28)).unwrap();
            wahl.push(c.key(Key::Enter).unwrap());
        }
        assert!(!wahl.contains(&Some(Action::Delete)), "{wahl:?}");
        assert!(wahl.contains(&Some(Action::DeleteBuilding)));
        assert_eq!(c.key(Key::Escape), Err(()));
        // Maus über „Löschen“: Tooltip nennt den Satz aus H113
        let mut y = r.y as f64 + 2.0;
        while c.tip().is_none() && y < (r.y + r.h) as f64 {
            c.mouse_move(&t, 1.0, (r.x + 20.0) as f64, y);
            y += 2.0;
        }
        assert_eq!(
            c.tip().as_deref(),
            Some(
                "Außenwände gehören zum Gebäudeumriss. \
                 Zum Entfernen den Umriss ändern oder das ganze Gebäude löschen."
            )
        );
        // Ein Klick auf das gedimmte „Löschen“ bewirkt nichts
        assert!(c.press(&t, 1.0, (r.x + 20.0) as f64, y - 2.0));
        assert_eq!(c.release(&t, 1.0, (r.x + 20.0) as f64, y - 2.0), None);
        let png = c.paint(&t, &fonts(), 1.0).0;
        assert!(png.width as f32 > r.w);
    }

    /// H113, H116: Verweise im Hinweis und seine Zeit (150 ms ein, 5 s,
    /// unter der Maus länger, 150 ms aus); `anim_ms` = 0 ohne Blenden.
    #[test]
    fn hinweis_verweise_und_zeit() {
        let mut s = Scene::with_model(Model::with_seed(113));
        haus_b11(&mut s);
        let aw = erstes(&s, Category::ExteriorWall);
        let iw = erstes(&s, Category::InteriorWall);
        let b = s.model().building_of_element(aw).unwrap();
        let d = s.delete_elements(&[aw]);
        assert!(d.removed.is_empty());
        assert_eq!(
            delete::hint_link(s.model(), &d),
            Some(("Gebäude löschen …", Link::DeleteBuilding(b)))
        );
        assert_eq!(delete::hint_anchor(&d), vec![aw]);
        let d = s.delete_elements(&[iw, aw]);
        assert_eq!(
            delete::hint_link(s.model(), &d),
            Some(("Rückgängig", Link::Undo))
        );

        let (t, f) = (Theme::dark(), fonts());
        let t0 = Instant::now();
        let ms = |n: u64| t0 + Duration::from_millis(n);
        let mut h = HintCard::new(
            delete::hint(s.model(), &d),
            delete::hint_link(s.model(), &d),
            vec![aw],
            t0,
        );
        let size = h.size(&t, &f, 1.0);
        h.place(
            size,
            Some(Rect::new(500.0, 300.0, 200.0, 40.0)),
            (1280.0, 800.0, 32.0),
            1.0,
        );
        let r = h.rect.unwrap();
        assert!(r.y >= 340.0, "unter dem Bauteil: {r:?}");
        assert!(
            (h.alpha(ms(75), 150.0).unwrap() - 0.5).abs() < 0.02,
            "blendet ein"
        );
        assert_eq!(h.alpha(ms(1000), 150.0), Some(1.0));
        assert!(h.alpha(ms(4925), 150.0).unwrap() < 0.51, "blendet aus");
        assert_eq!(h.alpha(ms(5001), 150.0), None, "nach 5 s weg");
        assert_eq!(h.alpha(ms(10), 0.0), Some(1.0), "anim_ms = 0: sofort da");
        // Unter der Maus bleibt er stehen
        let (cx, cy) = ((r.x + 10.0) as f64, (r.y + 10.0) as f64);
        h.mouse_move(cx, cy, 1.0, &t, ms(4000));
        assert_eq!(h.alpha(ms(9000), 150.0), Some(1.0));
        h.mouse_move(0.0, 0.0, 1.0, &t, ms(9000));
        assert!(
            h.alpha(ms(9500), 150.0).is_some(),
            "noch kurz nach dem Verlassen"
        );
        assert_eq!(h.alpha(ms(10300), 150.0), None);
        assert_eq!(h.click(cx, cy, 1.0, &t), Some(None), "Karte, kein Verweis");
        assert_eq!(h.click(0.0, 0.0, 1.0, &t), None);
        // Der Verweis steht in der letzten Zeile
        let ly = (r.y + r.h - 22.0) as f64;
        let lx = (r.x + 40.0) as f64;
        assert_eq!(h.click(lx, ly, 1.0, &t), Some(Some(Link::Undo)));
        assert!(h.paint(&t, &f, 1.0).width as f32 > r.w);
    }

    /// H117: „Gebäude 1 löschen?“ nennt die Bauteile; Enter, Esc und ×
    /// behalten, nur „Löschen“ löscht. Ein neues Gebäude heißt „Gebäude 2“.
    #[test]
    fn rueckfrage_gebaeude_loeschen() {
        let mut s = Scene::with_model(Model::with_seed(117));
        haus_b11(&mut s);
        let aw = erstes(&s, Category::ExteriorWall);
        let b = s.model().building_of_element(aw).unwrap();
        let mut k = ConfirmCard::new(s.model(), b).unwrap();
        assert_eq!(k.title, "Gebäude 1 löschen?");
        assert_eq!(
            k.text,
            "Alle Bauteile dieses Gebäudes werden entfernt: \
             8 Außenwände, 1 Innenwand, 2 Decken, Sohlplatte, Frostschürze."
        );
        assert_eq!(k.parts.len(), 13, "das ganze Gebäude leuchtet");
        assert_eq!(k.key(Key::Enter), Some(Answer::Keep), "Behalten vorgewählt");
        assert_eq!(k.key(Key::Escape), Some(Answer::Keep));
        let (t, f) = (Theme::dark(), fonts());
        let r = k.rect(&f, &t, 1.0, 1280, 800, 32);
        assert!(r.y + r.h <= 800.0 - 16.0 && r.x > 0.0, "{r:?}");
        let [_, del, x] = k.buttons(&f, &t, 1.0);
        let at = |b: Rect| ((r.x + b.x + 4.0) as f64, (r.y + b.y + 4.0) as f64);
        let (cx, cy) = at(x);
        k.press(r, &f, &t, 1.0, cx, cy);
        assert_eq!(k.release(r, &f, &t, 1.0, cx, cy), Some(Answer::Keep), "×");
        let (dx, dy) = at(del);
        k.press(r, &f, &t, 1.0, dx, dy);
        assert_eq!(
            k.release(r, &f, &t, 1.0, cx, cy),
            None,
            "daneben losgelassen"
        );
        k.press(r, &f, &t, 1.0, dx, dy);
        assert_eq!(k.release(r, &f, &t, 1.0, dx, dy), Some(Answer::Delete));
        k.key(Key::Tab);
        assert_eq!(
            k.key(Key::Enter),
            Some(Answer::Delete),
            "Tab wechselt den Fokus"
        );
        assert!(k.paint(&t, &f, 1.0).width as f32 > r.w);

        assert!(s.remove_building(b));
        assert!(ConfirmCard::new(s.model(), b).is_none());
        haus_b11(&mut s);
        let aw = erstes(&s, Category::ExteriorWall);
        let b2 = s.model().building_of_element(aw).unwrap();
        assert_eq!(
            ConfirmCard::new(s.model(), b2).unwrap().title,
            "Gebäude 2 löschen?"
        );
    }

    /// Nacharbeit 3: Der Grundriss passt zwischen „Werkzeuge“ und den
    /// Platz des Geschossbogens neben „Eigenschaften“; der Bogen steht so
    /// nie im Plan, auch wenn er wegen einer Auswahl ausweicht.
    #[test]
    fn grundriss_laesst_platz_fuer_den_bogen() {
        let th = Theme::dark();
        let w = crate::wheel::Wheel::new(&th, false);
        let mut s = Scene::with_model(Model::with_seed(3));
        haus_b11(&mut s);
        let (lo, hi) = s.bounds().unwrap();
        let mut ui = Ui::new(1.0, &th);
        for (bw, bh) in [(1280u32, 800u32), (1440, 900), (1920, 1080)] {
            ui.fit(1.0, bw, bh);
            let (vw, vh) = (bw as f64, bh as f64 - 32.0);
            let tools = ui.rect(Panel::Tools, bw, 32);
            let x0 = (tools.x + tools.w) as f64;
            let x1 = w.left_beside_props(&ui, bw, bh) as f64;
            let c = fit_parallel_in(
                ViewKind::Plan,
                Some((lo, hi)),
                x1 - x0,
                vw,
                vh,
                (x0 + x1) * 0.5,
            );
            for i in 0..8 {
                let p = vec3(
                    if i & 1 == 0 { lo.x } else { hi.x },
                    if i & 2 == 0 { lo.y } else { hi.y },
                    if i & 4 == 0 { lo.z } else { hi.z },
                );
                let (x, _) = c.project(p, vw, vh).unwrap();
                assert!(x > x0 && x < x1, "{bw}: Ecke bei {x}, frei {x0}…{x1}");
            }
            ui.set_props(crate::selection::props(
                &s,
                erstes(&s, Category::ExteriorWall),
            ));
            let g = w.geo(&ui, bw, bh);
            assert!(
                g.right <= ui.rect(Panel::Props, bw, 32).x,
                "{bw}: neben dem Paneel"
            );
            ui.set_props(None);
        }
    }

    /// Auch im schmalen Hauptfenster (Mengenfenster angedockt) liegt der
    /// eingepasste Grundriss zwischen „Werkzeuge“ und dem Bogen.
    #[test]
    fn grundriss_passt_auch_angedockt() {
        let th = Theme::dark();
        let w = crate::wheel::Wheel::new(&th, false);
        let mut s = Scene::with_model(Model::with_seed(3));
        haus_b11(&mut s);
        let (lo, hi) = s.bounds().unwrap();
        let mut ui = Ui::new(1.0, &th);
        for (bw, bh, scale) in [
            (920u32, 800u32, 1.0f32),
            (1000, 700, 1.0),
            (1380, 1000, 1.5),
        ] {
            ui.fit(scale, bw, bh);
            let top = ui.top;
            let c = plan_camera(&ui, &w, Some((lo, hi)), bw, bh, top);
            let (vw, vh) = (bw as f64, (bh - top) as f64);
            let tools = ui.rect(Panel::Tools, bw, top);
            let x0 = (tools.x + tools.w) as f64;
            let x1 = w.left_beside_props(&ui, bw, bh) as f64;
            for i in 0..8 {
                let p = vec3(
                    if i & 1 == 0 { lo.x } else { hi.x },
                    if i & 2 == 0 { lo.y } else { hi.y },
                    if i & 4 == 0 { lo.z } else { hi.z },
                );
                let (x, _) = c.project(p, vw, vh).unwrap();
                assert!(x > x0 && x < x1, "{bw}: Ecke bei {x}, frei {x0}…{x1}");
            }
        }
    }

    /// Schritt-Bezeichnungen mit Zahl werden geteilt, nicht je Aufruf neu.
    #[test]
    fn schrittnamen_mit_zahl() {
        let a = sk_model::step_label(format!("{} Bauteile gelöscht", 4));
        let b = sk_model::step_label("4 Bauteile gelöscht".to_string());
        assert_eq!(a, "4 Bauteile gelöscht");
        assert!(std::ptr::eq(a, b));
    }

    /// Entf ist das Kürzel für „Löschen“, nur ohne Umschalt-, Strg- und Alt-Taste.
    #[test]
    fn entf_loescht() {
        let mut k = crate::menu::Shortcuts::default();
        let ohne = Modifiers::default();
        assert_eq!(
            k.key(Key::Delete, true, ohne, true),
            Some(crate::menu::Command::Delete)
        );
        assert_eq!(
            k.key(Key::Delete, true, ohne, false),
            None,
            "beim Zeichnen nicht"
        );
        let strg = Modifiers { ctrl: true, ..ohne };
        assert_eq!(k.key(Key::Delete, true, strg, true), None);
    }
}

// A137: Geschossbogen nach „Gebäude löschen“ wie das Paneel
mod bogen_loeschen {
    use super::*;

    // Abnahmetest A137: Geschossbogen nach „Gebäude löschen“ (Nacharbeit zu
    // 7fa905b, Koordinator 07.10. 03:04). Der Bogen zeigt dieselben Geschosse wie
    // das Paneel „Geschosse“, auch wenn nur noch die Vorlage steht (Ist-Bild
    // loeschen-3c: Paneel mit OG, Bogen oben ausgegraut). Ist ein Bauteil gewählt,
    // steht der Bogen links neben „Eigenschaften“.
    // Spezifikation: test/abnahme-loeschen.md (A137).
    //
    // Einbau: als `mod bogen_loeschen { use super::*; … }` ans Ende von
    // app/src/abnahme.rs. Nutzt aus abnahme.rs die E18-Adapter (bogen,
    // bogen_text, spitze_frei, spitze_klick, tick, bogen_lage, bogen_rechts),
    // haus_b11 und test_dir. Eigener Adapter nur `gebaeude_loeschen`, wie in
    // `mod loeschen`.

    use sk_model::{BuildingId, Model};

    // ===== Adapter =====

    fn gebaeude_loeschen(s: &mut Scene, b: BuildingId) -> bool {
        s.remove_building(b)
    }

    // ===== Hilfen =====

    /// Bogen und Paneel zeigen dasselbe: Das aktive Geschoss in der Mitte ist das
    /// aktive Band, die Spitzen nennen die Nachbarbänder im Paneel (oder sind
    /// ausgegraut, wenn das Paneel dort keines hat).
    fn wie_paneel(w: &crate::wheel::Wheel, s: &Scene, ctx: &str) {
        let l = s.levels();
        let i = l
            .bands
            .iter()
            .position(|b| b.active)
            .unwrap_or_else(|| panic!("{ctx}: kein aktives Band"));
        let name = |k: Option<usize>| k.and_then(|k| l.bands.get(k)).map(|b| b.name.clone());
        let ((mitte, _), oben, unten) = bogen_text(w, s);
        assert_eq!(Some(mitte), name(Some(i)), "{ctx}: Mitte");
        assert_eq!(oben, name(Some(i + 1)), "{ctx}: Spitze oben");
        assert_eq!(unten, name(i.checked_sub(1)), "{ctx}: Spitze unten");
        assert_eq!(spitze_frei(w, s, true, false), oben.is_some(), "{ctx}");
        assert_eq!(spitze_frei(w, s, false, false), unten.is_some(), "{ctx}");
    }

    fn erstes_gebaeude(s: &Scene) -> BuildingId {
        s.model()
            .buildings()
            .iter()
            .next()
            .map(|(id, _)| id)
            .unwrap()
    }

    // ===== Tests =====

    /// A137: Bogen und Paneel zeigen dieselben Geschosse: ohne Gebäude, mit
    /// Gebäude, nach „Gebäude löschen“ (Vorlage mit OG: Spitze oben „OG“, frei,
    /// ein Klick führt ins OG), nach Rückgängig, Wiederholen und nach Speichern
    /// und Öffnen der Vorlage. Mit gewähltem Bauteil liegt der ganze Bogen samt
    /// Schild links von „Eigenschaften“ (Abstand panel_margin), ohne Auswahl
    /// wieder am rechten Rand; drei Fenstergrößen.
    #[test]
    fn a137_bogen_wie_paneel_nach_gebaeude_loeschen() {
        let th = Theme::dark();
        let mut w = bogen(&th);
        let s = Scene::with_model(Model::with_seed(137));
        wie_paneel(&w, &s, "ohne Gebäude");

        let mut s = Scene::with_model(Model::with_seed(137));
        haus_b11(&mut s);
        wie_paneel(&w, &s, "mit Gebäude");
        let b = erstes_gebaeude(&s);
        assert!(gebaeude_loeschen(&mut s, b));
        assert!(
            s.levels().bands.iter().any(|b| b.name == "OG"),
            "Paneel zeigt das OG der Vorlage"
        );
        wie_paneel(&w, &s, "nach Gebäude löschen");
        assert_eq!(bogen_text(&w, &s).1.as_deref(), Some("OG"));
        spitze_klick(&mut w, &mut s, true, false, 0);
        tick(&mut w, &mut s, 1000);
        assert_eq!(bogen_text(&w, &s).0 .0, "OG", "Spitze führt ins OG");
        wie_paneel(&w, &s, "Vorlage im OG");
        assert!(s.undo());
        wie_paneel(&w, &s, "Rückgängig");
        assert!(s.redo());
        wie_paneel(&w, &s, "Wiederholen");
        let text = sk_model::szo::write(s.model());
        let p = test_dir("a137").join("vorlage.szo");
        std::fs::write(&p, text).unwrap();
        let l = crate::document::load(&p).expect("öffnet");
        let s2 = Scene::with_model(l.model);
        wie_paneel(&w, &s2, "Vorlage gespeichert und geöffnet");

        // Ausweichen neben „Eigenschaften“
        let mut s = Scene::with_model(Model::with_seed(137));
        let (aw, _) = haus_b11(&mut s);
        let el = s.model().wall_at(aw, 0).unwrap();
        let mut ui = Ui::new(1.0, &th);
        for (bw, bh) in [(1280u32, 800u32), (1440, 900), (1920, 1080)] {
            ui.fit(1.0, bw, bh);
            ui.set_props(None);
            let frei = bogen_rechts(&w, &s, &ui, bw, bh);
            assert!(
                (frei - (bw as f32 - th.size.panel_margin)).abs() < 1.0,
                "{bw}: ohne Auswahl am Rand"
            );
            ui.set_props(crate::selection::props(&s, el));
            assert!(ui.has_props());
            let props = ui.rect(Panel::Props, bw, 32);
            let rechts = bogen_rechts(&w, &s, &ui, bw, bh);
            assert!(
                rechts <= props.x - th.size.panel_margin + 1.0,
                "{bw}: Bogen {rechts} reicht unter Eigenschaften ({})",
                props.x
            );
            let (cx, _, r) = bogen_lage(&w, &ui, bw, bh);
            assert!(cx + r < rechts, "{bw}: Bogen links vom Schild");
        }
    }
}

mod mengen_entf {
    use super::*;

    // Abnahmetest A138: Entf im Mengenfenster (H119 neu, Sollbild
    // soll-loeschen-5, Jörns Freigabe 07.10. 03:23; Abschnitt „Vorschlag H119“ am
    // Ende von einstellungen/paket-loeschen-gestaltung.md).
    // Spezifikation: test/abnahme-loeschen.md (A138).
    //
    // Einbau: als `mod mengen_entf { use super::*; … }` ans Ende von
    // app/src/abnahme.rs. Nutzt aus abnahme.rs: haus_b11, cam3d, tool, click,
    // key, W, H. Zeilen wählen über die vorhandene ListView (click_row,
    // click_group, B7).
    //
    // Angenommener Name nur im Adapter: `ListView::delete_key` (Entf im
    // Mengenfenster, wirkt auf die gemeinsame Auswahl, liefert die Zeilen des
    // Hinweises). Ein Rechtsklick-Menü auf eine Zeile („Im Modell zeigen“ oben)
    // und die Übergänge prüft Handtest H119.

    use crate::picking::Picking;
    use crate::schedule_view::ListView;
    use sk_model::{Category, ElementId, Model, RunId};

    // ===== Adapter =====

    /// Entf, während das Mengenfenster vorne ist: löscht nach denselben Regeln
    /// wie im Hauptfenster (ein Rückgängig-Schritt) und gibt den Hinweis unter
    /// der Zeile Zeile für Zeile zurück (leer: kein Hinweis).
    fn mengen_entf(v: &mut ListView, s: &mut Scene, p: &mut Picking) -> Vec<String> {
        v.delete_key(s, p)
    }

    // ===== Hilfen =====

    fn hat(s: &Scene, n: &str) -> bool {
        s.model().elements().iter().any(|(_, e)| e.number == n)
    }

    fn iw_zug(s: &mut Scene, a: (f64, f64), b: (f64, f64)) -> RunId {
        let c = cam3d();
        let set = s.model().defaults().interior_wall;
        let mut t = tool(s);
        t.set_category(Category::InteriorWall, s.model().wall_layers(set));
        click(&mut t, &c, vec3(a.0, a.1, 0.0));
        click(&mut t, &c, vec3(b.0, b.1, 0.0));
        let w = t.handle(&key(Key::Enter), &c, W, H, 1.0).commit.unwrap();
        s.add_wall_as(&w, Category::InteriorWall).unwrap()
    }

    /// Prüfhaus mit drei Innenwänden im EG (IW-001 bei x = 5 m, IW-002 bei
    /// 2,5 m, IW-003 bei 7,5 m).
    fn haus() -> Scene {
        let mut s = Scene::with_model(Model::with_seed(138));
        haus_b11(&mut s);
        iw_zug(&mut s, (2500.0, 0.0), (2500.0, 8000.0));
        iw_zug(&mut s, (7500.0, 0.0), (7500.0, 8000.0));
        assert!(hat(&s, "IW-003"));
        s
    }

    fn csv(s: &mut Scene) -> String {
        let l = s.schedule().clone();
        String::from_utf8_lossy(&crate::schedule_view::csv(s.model(), &l)).to_string()
    }

    fn zeilen(v: &[&str]) -> Vec<String> {
        v.iter().map(|z| z.to_string()).collect()
    }

    fn sauber(s: &Scene, ctx: &str) {
        assert!(
            s.model().check().is_empty(),
            "{ctx}: {:?}",
            s.model().check()
        );
    }

    // ===== Tests =====

    /// A138 (H119 neu): Entf im Mengenfenster wirkt auf die gemeinsame Auswahl
    /// mit denselben Regeln und Sätzen wie im Hauptfenster.
    /// - Zeile IW-001 gewählt: IW-001 weg, kein Hinweis, Schritt „Bauteil
    ///   gelöscht“, Zeile fehlt in der Liste, Auswahl bereinigt; Rückgängig
    ///   bringt sie zurück.
    /// - Gruppenzeile „Innenwände“ im EG gewählt: alle drei weg in einem
    ///   Schritt „3 Bauteile gelöscht“, Hinweis „3 Wände gelöscht.“ (mit
    ///   „Rückgängig“).
    /// - Zeile AW-003 gewählt: nichts weg, Hinweis mit dem Außenwand-Satz, kein
    ///   Schritt, Revision gleich.
    /// - IW-001 und AW-001 mit Strg gewählt: „1 Wand gelöscht.“ / „Die
    ///   Außenwand bleibt, sie gehört zum Gebäudeumriss.“
    /// - Nichts gewählt (Geschoss- oder Summenzeile): nichts weg, Hinweis
    ///   „Hier ist kein Bauteil gewählt.“, kein Schritt.
    #[test]
    fn a138_entf_im_mengenfenster() {
        let aw_satz = [
            "Außenwände gehören zum Gebäudeumriss.",
            "Zum Entfernen den Umriss ändern oder das ganze Gebäude löschen.",
        ];

        // Eine Zeile
        let mut s = haus();
        let mut p = Picking::default();
        let mut v = ListView::new(&mut s);
        v.click_row(&mut s, &mut p, "IW-001", false, false);
        assert_eq!(p.selected.len(), 1);
        let id: ElementId = p.selected[0];
        let text = sk_model::szo::write(s.model());
        let hinweis = mengen_entf(&mut v, &mut s, &mut p);
        assert!(hinweis.is_empty(), "{hinweis:?}");
        assert!(!hat(&s, "IW-001"));
        assert!(s.model().element(id).is_none());
        assert_eq!(s.undo_label(), Some("Bauteil gelöscht"));
        assert!(!csv(&mut s).contains("IW-001"), "Zeile weg");
        assert!(p.selected.is_empty(), "Auswahl bereinigt");
        v.follow(&mut s, &p);
        sauber(&s, "eine Zeile");
        assert!(s.undo());
        assert_eq!(sk_model::szo::write(s.model()), text, "Rückgängig");
        assert!(csv(&mut s).contains("IW-001"));

        // Gruppenzeile
        let mut s = haus();
        let mut p = Picking::default();
        let mut v = ListView::new(&mut s);
        v.click_group(&mut s, &mut p, "EG", Category::InteriorWall);
        assert_eq!(p.selected.len(), 3, "Gruppe wählt alle drei");
        let hinweis = mengen_entf(&mut v, &mut s, &mut p);
        assert_eq!(
            hinweis.first().map(String::as_str),
            Some("3 Wände gelöscht.")
        );
        assert_eq!(hinweis.len(), 1, "nichts abgelehnt: {hinweis:?}");
        assert!(!hat(&s, "IW-001") && !hat(&s, "IW-002") && !hat(&s, "IW-003"));
        assert_eq!(s.undo_label(), Some("3 Bauteile gelöscht"), "ein Schritt");
        sauber(&s, "Gruppe");
        assert!(s.undo());
        assert!(hat(&s, "IW-001") && hat(&s, "IW-002") && hat(&s, "IW-003"));

        // Außenwand-Zeile
        let mut s = haus();
        let mut p = Picking::default();
        let mut v = ListView::new(&mut s);
        v.click_row(&mut s, &mut p, "AW-003", false, false);
        let rev = s.model().revision();
        let label = s.undo_label();
        let hinweis = mengen_entf(&mut v, &mut s, &mut p);
        assert_eq!(hinweis, zeilen(&aw_satz));
        assert!(hat(&s, "AW-003"));
        assert_eq!(s.model().revision(), rev, "Datei unverändert");
        assert_eq!(s.undo_label(), label, "kein Schritt");

        // Gemischt mit Strg
        v.click_row(&mut s, &mut p, "IW-001", false, false);
        v.click_row(&mut s, &mut p, "AW-001", true, false);
        assert_eq!(p.selected.len(), 2);
        let hinweis = mengen_entf(&mut v, &mut s, &mut p);
        assert_eq!(
            hinweis,
            zeilen(&[
                "1 Wand gelöscht.",
                "Die Außenwand bleibt, sie gehört zum Gebäudeumriss."
            ])
        );
        assert!(!hat(&s, "IW-001") && hat(&s, "AW-001"));
        assert_eq!(s.undo_label(), Some("Bauteil gelöscht"));

        // Nichts gewählt
        p.clear();
        let rev = s.model().revision();
        let label = s.undo_label();
        let hinweis = mengen_entf(&mut v, &mut s, &mut p);
        assert_eq!(hinweis, zeilen(&["Hier ist kein Bauteil gewählt."]));
        assert_eq!(s.model().revision(), rev);
        assert_eq!(s.undo_label(), label);
    }
}

mod auswahl_loslassen {
    use super::*;

    // Abnahmetest A139: Auswahl beim Loslassen, Strg+Klick auf das violette
    // Wandband erweitert die Auswahl (Fix aus e06cada, Handtest H121).
    // Die Entscheidung steckt in `selection::release_pick` (Bauthread, Name und
    // Signatur vom Koordinator 07.10. 03:25); main.rs und das Mengenfenster
    // nutzen sie beide.
    // Spezifikation: test/abnahme-loeschen.md (A139). Ergänzt H121, ersetzt ihn
    // nicht: ob main.rs die Funktion wirklich aufruft, sieht nur der Handtest.
    //
    // Einbau: als `mod auswahl_loslassen { use super::*; … }` ans Ende von
    // app/src/abnahme.rs. Nutzt aus abnahme.rs: haus_b11.
    // Name nur im Adapter: `selection::release_pick` und `selection::PickChange`
    // (Keep, Replace(Option<ElementId>), Add(id), Remove(id); braucht Debug und
    // PartialEq).

    use crate::selection::PickChange;
    use sk_model::{Category, ElementId, Model};

    // ===== Adapter =====

    /// Maus losgelassen. `band`: Bauteil, dessen violettes Band angeklickt (nicht
    /// gezogen) wurde, hat Vorrang. `treffer`: `None` = gezogen, kein Klick;
    /// `Some(None)` = Klick ins Leere; `Some(Some(id))` = Bauteil getroffen.
    /// `werkzeug`: Wandwerkzeug an, dann wirkt Strg nicht.
    fn loslassen(
        band: Option<ElementId>,
        treffer: Option<Option<ElementId>>,
        strg: bool,
        werkzeug: bool,
        gewaehlt: &[ElementId],
    ) -> PickChange {
        crate::selection::release_pick(band, treffer, strg, werkzeug, gewaehlt)
    }

    // ===== Hilfen =====

    fn wand(s: &Scene, c: Category, n: usize) -> ElementId {
        s.model()
            .elements()
            .iter()
            .filter(|(_, e)| e.category == c)
            .map(|(id, _)| id)
            .nth(n)
            .unwrap()
    }

    // ===== Tests =====

    /// A139: Entscheidung beim Loslassen.
    /// - Klick auf ein Bauteil ohne Strg ersetzt die Auswahl.
    /// - Strg+Klick nimmt ein nicht gewähltes Bauteil dazu und nimmt ein
    ///   gewähltes heraus, auf das Bauteil wie auf sein violettes Band (der
    ///   Fehler vor e06cada: Band + Strg ersetzte die Auswahl).
    /// - Das Band hat Vorrang vor dem Treffer darunter.
    /// - Gezogen (kein Klick) ändert nichts, auch nicht mit Strg.
    /// - Klick ins Leere hebt die Auswahl auf; mit Strg bleibt sie (Koordinator
    ///   07.10. 03:28: wer mit Strg sammelt und daneben klickt, verliert nichts).
    /// - Bei eingeschaltetem Wandwerkzeug wirkt Strg nicht: ersetzen.
    #[test]
    fn a139_strg_klick_erweitert_die_auswahl() {
        let mut s = Scene::with_model(Model::with_seed(139));
        haus_b11(&mut s);
        let iw = wand(&s, Category::InteriorWall, 0);
        let a1 = wand(&s, Category::ExteriorWall, 0);
        let a2 = wand(&s, Category::ExteriorWall, 1);
        let sel = [iw];

        // Ohne Strg: ersetzen, egal ob Treffer oder Band
        assert_eq!(
            loslassen(None, Some(Some(a1)), false, false, &sel),
            PickChange::Replace(Some(a1))
        );
        assert_eq!(
            loslassen(Some(a1), Some(None), false, false, &sel),
            PickChange::Replace(Some(a1)),
            "Klick aufs Band ohne Strg"
        );

        // Strg: dazu bzw. heraus, Treffer wie Band
        for (band, treffer, wie) in [
            (None, Some(Some(a1)), "Treffer"),
            (Some(a1), Some(None), "Band"),
            (Some(a1), Some(Some(a1)), "Band über dem Bauteil"),
        ] {
            assert_eq!(
                loslassen(band, treffer, true, false, &sel),
                PickChange::Add(a1),
                "Strg+Klick ({wie}) nimmt dazu"
            );
            assert_eq!(
                loslassen(band, treffer, true, false, &[iw, a1]),
                PickChange::Remove(a1),
                "Strg+Klick ({wie}) auf Gewähltes nimmt heraus"
            );
        }

        // Band hat Vorrang vor dem Treffer darunter
        assert_eq!(
            loslassen(Some(a2), Some(Some(a1)), false, false, &sel),
            PickChange::Replace(Some(a2))
        );
        assert_eq!(
            loslassen(Some(a2), Some(Some(a1)), true, false, &sel),
            PickChange::Add(a2)
        );

        // Gezogen: nichts
        for strg in [false, true] {
            assert_eq!(
                loslassen(None, None, strg, false, &sel),
                PickChange::Keep,
                "gezogen, Strg {strg}"
            );
        }

        // Klick ins Leere: ohne Strg weg, mit Strg bleibt sie
        assert_eq!(
            loslassen(None, Some(None), false, false, &sel),
            PickChange::Replace(None)
        );
        assert_eq!(
            loslassen(None, Some(None), true, false, &sel),
            PickChange::Keep,
            "Strg+Klick ins Leere behält die Auswahl"
        );

        // Wandwerkzeug an: Strg wirkt nicht
        assert_eq!(
            loslassen(None, Some(Some(a1)), true, true, &sel),
            PickChange::Replace(Some(a1))
        );
        assert_eq!(
            loslassen(Some(a1), Some(None), true, true, &[iw, a1]),
            PickChange::Replace(Some(a1))
        );
    }
}

mod sichern {
    use super::*;
    // Abnahmetests A140–A144: F-13 „Automatisch sichern“ (Jörns Ja 07.10.
    // 03:46). Gestaltung: einstellungen/paket-f13-sichern.md (Abschnitt 6) und
    // Sollbild soll-sichern-1.png. Spezifikation: test/abnahme-sichern.md.
    // Aussehen und Bedienung der Startkarte prüfen die Handtests H122–H125.
    //
    // Ersetzt die erste Fassung (Sicherung neben der .szo, beim Schließen
    // gelöscht).
    //
    // Einbau: als `mod sichern { use super::*; … }` ans Ende von
    // app/src/abnahme.rs. Nutzt aus abnahme.rs: haus_b11, test_dir, cam3d,
    // tool, click, key, W, H.
    //
    // Angenommene Namen stehen nur in den Adaptern (Vorschlag, frei wählbar):
    // `crate::autosave::{AutoSave, start, restore, card_title, age_text,
    // when_text}`, `AutoSave::{new, tick, saved, closed}`. Der Sicherungsordner
    // (in der App %APPDATA%\Skizzeo\Sicherungen) geht als Pfad hinein, die Zeit
    // als Millisekunden seit Programmstart, damit niemand fünf Minuten warten
    // muss.
    //
    // Annahme zum Takt: Gesichert wird, wenn das Modell seit der letzten
    // Sicherung (bzw. seit Öffnen/Speichern) geändert ist und seit der letzten
    // Sicherung (bzw. seit Öffnen/Speichern) mindestens 5 Minuten vergangen
    // sind. Zählt der Bau anders, ändern sich nur die Zeiten in A140.

    use crate::autosave::AutoSave;
    use crate::document::Document;
    use sk_model::{Category, Model};
    use std::path::{Path, PathBuf};
    use std::time::{Duration, SystemTime};

    const MIN: u64 = 60_000;
    const TAG: u64 = 24 * 60 * 60;

    // ===== Adapter =====

    /// Sicherer für den Ordner „Sicherungen“.
    fn sicherer(ordner: &Path) -> AutoSave {
        AutoSave::new(ordner.to_path_buf())
    }

    /// Zeitgeber zur Zeit `t` ms: sichert, wenn fällig. `Some(pfad)`, wenn
    /// jetzt eine Sicherung geschrieben wurde.
    fn takt(a: &mut AutoSave, s: &Scene, doc: &Document, t: u64) -> Option<PathBuf> {
        a.tick(s.model(), doc, Duration::from_millis(t))
    }

    /// Nach Strg+S (Datei ist geschrieben, `doc` kennt den neuen Stand).
    fn gespeichert(a: &mut AutoSave, doc: &Document, t: u64) {
        a.saved(doc, Duration::from_millis(t));
    }

    /// Programm normal beendet (auch „Nicht speichern“).
    fn beendet(a: &mut AutoSave, doc: &Document) {
        a.closed(doc);
    }

    /// Programmstart zur Zeit `jetzt`: räumt alte Sicherungen auf und meldet
    /// die Sicherung für die Startkarte: (Sicherung, gespeicherte Datei oder
    /// `None` bei Unbenannt). `None`: keine Karte.
    fn beim_start(ordner: &Path, jetzt: SystemTime) -> Option<(PathBuf, Option<PathBuf>)> {
        crate::autosave::start(ordner, jetzt).map(|f| (f.backup, f.original))
    }

    /// „Wiederherstellen“ auf der Startkarte.
    fn wiederherstellen(sicherung: &Path, original: Option<&Path>) -> (Scene, Document) {
        crate::autosave::restore(sicherung, original).expect("Sicherung öffnet")
    }

    /// Titel der Startkarte.
    fn karten_titel(original: Option<&Path>) -> String {
        crate::autosave::card_title(original)
    }

    /// Akzentzeile der linken Kachel: Sicherung ist `min` Minuten neuer.
    fn abstand(min: u64) -> String {
        crate::autosave::age_text(min)
    }

    /// Zeitangabe einer Kachel; Zeiten als (Jahr, Monat, Tag, Stunde, Minute)
    /// wie `sk_platform::local_date_time`.
    fn zeit(wann: (u16, u8, u8, u8, u8), jetzt: (u16, u8, u8, u8, u8)) -> String {
        crate::autosave::when_text(wann, jetzt)
    }

    // ===== Hilfen =====

    /// Prüfhaus als `projekt/haus.szo` im Testordner, geöffnet wie „Datei →
    /// Öffnen“, und ein leerer Ordner „Sicherungen“: Szene, Dokument, Pfad,
    /// Sicherungsordner.
    fn geoeffnet(name: &str) -> (Scene, Document, PathBuf, PathBuf) {
        let mut s = Scene::with_model(Model::with_seed(140));
        haus_b11(&mut s);
        let dir = test_dir(name);
        let projekt = dir.join("projekt");
        let ordner = dir.join("Sicherungen");
        std::fs::create_dir_all(&projekt).unwrap();
        std::fs::create_dir_all(&ordner).unwrap();
        let p = projekt.join("haus.szo");
        crate::document::save(s.model(), &p).unwrap();
        let l = crate::document::load(&p).expect("öffnet");
        let s = Scene::with_model(l.model);
        let doc = Document::opened(p.clone(), s.model().revision());
        (s, doc, p, ordner)
    }

    /// Eine Änderung wie von Hand: Innenwand bei x = `x`.
    fn aendern(s: &mut Scene, x: f64) {
        let c = cam3d();
        let set = s.model().defaults().interior_wall;
        let mut t = tool(s);
        t.set_category(Category::InteriorWall, s.model().wall_layers(set));
        click(&mut t, &c, vec3(x, 0.0, 0.0));
        click(&mut t, &c, vec3(x, 8000.0, 0.0));
        let w = t.handle(&key(Key::Enter), &c, W, H, 1.0).commit.unwrap();
        s.add_wall_as(&w, Category::InteriorWall).unwrap();
    }

    fn text(s: &Scene) -> String {
        sk_model::szo::write(s.model())
    }

    fn inhalt(p: &Path) -> String {
        sk_model::szo::write(&crate::document::load(p).expect("öffnet").model)
    }

    /// .szo-Dateien in `dir` (nur Namen, sortiert).
    fn dateien(dir: &Path) -> Vec<String> {
        let mut v: Vec<String> = std::fs::read_dir(dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().to_string())
            .filter(|n| n.ends_with(".szo"))
            .collect();
        v.sort();
        v
    }

    /// Alle Einträge in `dir` (nur Namen, sortiert).
    fn alles(dir: &Path) -> Vec<String> {
        let mut v: Vec<String> = std::fs::read_dir(dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().to_string())
            .collect();
        v.sort();
        v
    }

    /// Name wie „haus 2026-10-07 03-41.szo“ zum Stamm „haus“.
    fn name_passt(n: &str, stamm: &str) -> bool {
        let Some(rest) = n.strip_prefix(stamm).and_then(|r| r.strip_prefix(' ')) else {
            return false;
        };
        let Some(z) = rest.strip_suffix(".szo") else {
            return false;
        };
        let b = z.as_bytes();
        b.len() == 16
            && b.iter().enumerate().all(|(i, c)| match i {
                4 | 7 => *c == b'-',
                10 => *c == b' ',
                13 => *c == b'-',
                _ => c.is_ascii_digit(),
            })
    }

    fn alter_setzen(p: &Path, jetzt: SystemTime, tage: u64) {
        let f = std::fs::File::options().write(true).open(p).unwrap();
        f.set_modified(jetzt - Duration::from_secs(tage * TAG))
            .unwrap();
    }

    // ===== Tests =====

    /// A140 (F-13, Takt): Ohne Änderung wird nie gesichert, auch nicht nach
    /// 5, 10 oder 60 Minuten. Nach einer Änderung (0:10) nicht vor 5:00, bei
    /// 5:00 genau einmal. Neue Änderung bei 5:30: nicht bei 9:59, aber bei
    /// 10:00 (5 Minuten nach der letzten Sicherung); ohne weitere Änderung bei
    /// 15:00 nicht wieder. Nach Strg+S ohne neue Änderung: nichts mehr.
    #[test]
    fn a140_sichern_nur_bei_aenderung_alle_5_minuten() {
        let (s, doc, _, ordner) = geoeffnet("a140");
        let mut a = sicherer(&ordner);
        for t in [0, 5 * MIN, 10 * MIN, 60 * MIN] {
            assert_eq!(takt(&mut a, &s, &doc, t), None, "unverändert, {t} ms");
        }
        assert!(dateien(&ordner).is_empty(), "nichts geschrieben");

        let (mut s, mut doc, p, ordner) = geoeffnet("a140b");
        let mut a = sicherer(&ordner);
        assert_eq!(takt(&mut a, &s, &doc, 0), None);
        aendern(&mut s, 2500.0); // 0:10
        assert!(doc.is_dirty(s.model()));
        assert_eq!(takt(&mut a, &s, &doc, MIN), None, "1:00");
        assert_eq!(takt(&mut a, &s, &doc, 5 * MIN - 1), None, "4:59,999");
        assert!(takt(&mut a, &s, &doc, 5 * MIN).is_some(), "5:00");
        assert_eq!(takt(&mut a, &s, &doc, 5 * MIN + 1000), None, "nur einmal");
        aendern(&mut s, 7500.0); // 5:30
        assert_eq!(
            takt(&mut a, &s, &doc, 10 * MIN - 1),
            None,
            "noch keine 5 Minuten seit der letzten Sicherung"
        );
        assert!(takt(&mut a, &s, &doc, 10 * MIN).is_some(), "10:00");
        assert_eq!(takt(&mut a, &s, &doc, 15 * MIN), None, "nichts Neues");

        // Strg+S: danach ohne Änderung nichts
        crate::document::save(s.model(), &p).unwrap();
        doc.mark_saved(p.clone(), s.model().revision());
        gespeichert(&mut a, &doc, 16 * MIN);
        for t in [21 * MIN, 30 * MIN] {
            assert_eq!(takt(&mut a, &s, &doc, t), None, "gespeichert, {t} ms");
        }
    }

    /// A141 (F-13, Ort): Die Sicherung liegt im Ordner „Sicherungen“ und heißt
    /// „haus JJJJ-MM-TT HH-MM.szo“. Je Projekt bleibt nur die jüngste. Sie
    /// lässt sich öffnen und enthält den aktuellen Stand. Die .szo bleibt
    /// bytegleich, im Projektordner entsteht nichts, der Titel behält das •.
    /// Ein unbenanntes Projekt wird als „Unbenannt …“ gesichert.
    #[test]
    fn a141_sicherung_im_ordner_sicherungen() {
        let (mut s, doc, p, ordner) = geoeffnet("a141");
        let vorher = std::fs::read(&p).unwrap();
        let projekt = p.parent().unwrap().to_path_buf();
        let mut a = sicherer(&ordner);
        takt(&mut a, &s, &doc, 0);
        aendern(&mut s, 2500.0);
        let b = takt(&mut a, &s, &doc, 5 * MIN).expect("gesichert");
        assert_eq!(b.parent(), Some(ordner.as_path()), "im Ordner Sicherungen");
        let n = b.file_name().unwrap().to_string_lossy().to_string();
        assert!(name_passt(&n, "haus"), "Name {n}");
        assert_eq!(inhalt(&b), text(&s), "aktueller Stand");

        aendern(&mut s, 7500.0);
        let b2 = takt(&mut a, &s, &doc, 10 * MIN).expect("wieder gesichert");
        assert_eq!(inhalt(&b2), text(&s), "neuer Stand");
        let liste = dateien(&ordner);
        assert_eq!(liste.len(), 1, "nur die jüngste: {liste:?}");
        assert!(name_passt(&liste[0], "haus"));

        assert_eq!(std::fs::read(&p).unwrap(), vorher, ".szo bytegleich");
        assert_eq!(
            alles(&projekt),
            vec!["haus.szo".to_string()],
            "Projektordner"
        );
        assert_eq!(doc.caption(s.model()), "haus.szo •", "Titel behält das •");

        // Unbenannt
        let mut s = Scene::with_model(Model::with_seed(141));
        let doc = Document::new(s.model().revision());
        let ordner = test_dir("a141u").join("Sicherungen");
        std::fs::create_dir_all(&ordner).unwrap();
        let mut a = sicherer(&ordner);
        takt(&mut a, &s, &doc, 0);
        haus_b11(&mut s);
        let b = takt(&mut a, &s, &doc, 5 * MIN).expect("Unbenannt gesichert");
        let n = b.file_name().unwrap().to_string_lossy().to_string();
        assert!(name_passt(&n, "Unbenannt"), "Name {n}");
        assert_eq!(inhalt(&b), text(&s));
    }

    /// A142 (F-13, Startkarte): Nach normalem Beenden keine Karte, die
    /// Sicherung bleibt aber im Ordner. Nach einem Absturz: Karte mit
    /// Sicherung und gespeicherter Datei, Titel „Sicherung von haus.szo
    /// gefunden“. Strg+S nach der Sicherung, dann Absturz: keine Karte (die
    /// Sicherung ist nicht neuer). Unbenannt: Karte ohne gespeicherte Datei,
    /// Titel „Sicherung von Unbenannt gefunden“. Sicherungen älter als 7 Tage
    /// entfernt der Start still, jüngere bleiben.
    #[test]
    fn a142_startkarte_nur_nach_absturz() {
        let jetzt = SystemTime::now();

        // Normal beendet
        let (mut s, doc, _, ordner) = geoeffnet("a142");
        assert_eq!(beim_start(&ordner, jetzt), None, "ohne Sicherung");
        let mut a = sicherer(&ordner);
        takt(&mut a, &s, &doc, 0);
        aendern(&mut s, 2500.0);
        takt(&mut a, &s, &doc, 5 * MIN).expect("gesichert");
        beendet(&mut a, &doc);
        drop(a);
        assert_eq!(beim_start(&ordner, jetzt), None, "sauber beendet");
        assert_eq!(dateien(&ordner).len(), 1, "Sicherung bleibt");

        // Absturz
        let (mut s, doc, p, ordner) = geoeffnet("a142b");
        let mut a = sicherer(&ordner);
        takt(&mut a, &s, &doc, 0);
        aendern(&mut s, 2500.0);
        let b = takt(&mut a, &s, &doc, 5 * MIN).expect("gesichert");
        drop(a); // kein Beenden, kein Speichern
        assert_eq!(
            beim_start(&ordner, jetzt),
            Some((b.clone(), Some(p.clone())))
        );
        assert_eq!(karten_titel(Some(&p)), "Sicherung von haus.szo gefunden");
        assert!(b.exists(), "der Start löscht die Sicherung nicht");

        // Strg+S nach der Sicherung, dann Absturz
        let (mut s, mut doc, p, ordner) = geoeffnet("a142c");
        let mut a = sicherer(&ordner);
        takt(&mut a, &s, &doc, 0);
        aendern(&mut s, 2500.0);
        takt(&mut a, &s, &doc, 5 * MIN).expect("gesichert");
        crate::document::save(s.model(), &p).unwrap();
        doc.mark_saved(p.clone(), s.model().revision());
        gespeichert(&mut a, &doc, 6 * MIN);
        drop(a);
        assert_eq!(beim_start(&ordner, jetzt), None, "Datei ist aktuell");

        // Unbenannt, Absturz
        let mut s = Scene::with_model(Model::with_seed(142));
        let doc = Document::new(s.model().revision());
        let ordner = test_dir("a142u").join("Sicherungen");
        std::fs::create_dir_all(&ordner).unwrap();
        let mut a = sicherer(&ordner);
        takt(&mut a, &s, &doc, 0);
        haus_b11(&mut s);
        let b = takt(&mut a, &s, &doc, 5 * MIN).expect("gesichert");
        drop(a);
        assert_eq!(beim_start(&ordner, jetzt), Some((b, None)));
        assert_eq!(karten_titel(None), "Sicherung von Unbenannt gefunden");

        // 7 Tage
        for (tage, bleibt) in [(8, false), (6, true)] {
            let (mut s, doc, _, ordner) = geoeffnet(&format!("a142t{tage}"));
            let mut a = sicherer(&ordner);
            takt(&mut a, &s, &doc, 0);
            aendern(&mut s, 2500.0);
            let b = takt(&mut a, &s, &doc, 5 * MIN).expect("gesichert");
            beendet(&mut a, &doc);
            drop(a);
            alter_setzen(&b, jetzt, tage);
            assert_eq!(beim_start(&ordner, jetzt), None);
            assert_eq!(b.exists(), bleibt, "{tage} Tage alt");
        }
    }

    /// A143 (F-13, Wiederherstellen/Verwerfen): Wiederherstellen öffnet den
    /// Stand der Sicherung mit dem Originalpfad, Titel „haus.szo •“,
    /// Rückgängig-Verlauf leer; das nächste Strg+S schreibt die Originaldatei.
    /// Unbenannt bleibt unbenannt. Verwerfen: die Sicherung bleibt im Ordner,
    /// die .szo unverändert.
    #[test]
    fn a143_wiederherstellen_und_verwerfen() {
        let jetzt = SystemTime::now();
        let (mut s, doc, p, ordner) = geoeffnet("a143");
        let vorher = std::fs::read(&p).unwrap();
        let mut a = sicherer(&ordner);
        takt(&mut a, &s, &doc, 0);
        aendern(&mut s, 2500.0);
        takt(&mut a, &s, &doc, 5 * MIN).expect("gesichert");
        drop(a);
        let (b, orig) = beim_start(&ordner, jetzt).expect("Karte");

        // Verwerfen: nichts angefasst
        assert!(b.exists(), "Sicherung bleibt im Ordner");
        assert_eq!(std::fs::read(&p).unwrap(), vorher, ".szo unverändert");

        // Wiederherstellen
        let (neu, ndoc) = wiederherstellen(&b, orig.as_deref());
        assert_eq!(text(&neu), text(&s), "Stand der Sicherung");
        assert_eq!(ndoc.path.as_deref(), Some(p.as_path()), "Originalpfad");
        assert_eq!(ndoc.caption(neu.model()), "haus.szo •");
        assert_eq!(neu.undo_label(), None, "Verlauf beginnt leer");
        let ziel = ndoc.path.clone().unwrap();
        crate::document::save(neu.model(), &ziel).unwrap();
        assert_eq!(inhalt(&p), text(&s), "Strg+S schreibt die Originaldatei");

        // Unbenannt
        let mut s = Scene::with_model(Model::with_seed(143));
        let doc = Document::new(s.model().revision());
        let ordner = test_dir("a143u").join("Sicherungen");
        std::fs::create_dir_all(&ordner).unwrap();
        let mut a = sicherer(&ordner);
        takt(&mut a, &s, &doc, 0);
        haus_b11(&mut s);
        takt(&mut a, &s, &doc, 5 * MIN).expect("gesichert");
        drop(a);
        let (b, orig) = beim_start(&ordner, jetzt).expect("Karte");
        let (neu, ndoc) = wiederherstellen(&b, orig.as_deref());
        assert_eq!(text(&neu), text(&s));
        assert_eq!(ndoc.path, None, "bleibt unbenannt");
        assert_eq!(ndoc.caption(neu.model()), "Unbenannt •");
    }

    /// A144 (F-13, Zeitangaben auf der Startkarte): „heute, HH:MM“,
    /// „gestern, HH:MM“ (auch über den Monatswechsel), sonst „TT.MM., HH:MM“.
    /// Abstand: „1 Minute neuer“, „N Minuten neuer“, „1 Stunde neuer“,
    /// „N Stunden neuer“.
    #[test]
    fn a144_zeitangaben() {
        let jetzt = (2026, 10, 7, 3, 45);
        assert_eq!(zeit((2026, 10, 7, 3, 41), jetzt), "heute, 03:41");
        assert_eq!(zeit((2026, 10, 6, 23, 5), jetzt), "gestern, 23:05");
        assert_eq!(zeit((2026, 10, 1, 9, 0), jetzt), "01.10., 09:00");
        assert_eq!(
            zeit((2026, 9, 30, 9, 0), (2026, 10, 1, 8, 0)),
            "gestern, 09:00"
        );
        assert_eq!(
            zeit((2025, 12, 31, 18, 30), (2026, 1, 1, 0, 10)),
            "gestern, 18:30"
        );

        assert_eq!(abstand(1), "1 Minute neuer");
        assert_eq!(abstand(4), "4 Minuten neuer");
        assert_eq!(abstand(59), "59 Minuten neuer");
        assert_eq!(abstand(60), "1 Stunde neuer");
        assert_eq!(abstand(180), "3 Stunden neuer");
    }
}
