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
use crate::wall_tool::{WallTool, WALL_HEIGHT};
use crate::{fit_parallel, fit_perspective};
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
    assert_eq!(chain.height, WALL_HEIGHT);
    assert_eq!(WALL_HEIGHT, 3500.0);
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
    pub const CROSS: f32 = 3.0;
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
                        (fill_kind::LINES, 2) => pattern::CROSS,
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
    assert!(lines.iter().any(|h| h.dash < 0.0), "Strichpunktlinie");
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
    let mut sect = SectionLine::default();
    sect.ensure(&s);
    let bounds = s.bounds();
    // Je Ansicht: Kamera, Schnittebene, Punkt auf der oberen bzw. einer Wand
    type Case = (ViewKind, Camera, Option<(Vec3, Vec3)>, Vec3);
    let cases: [Case; 4] = [
        // 3D: über der Erdgeschossdecke (OK +2,855), sonst trifft der Blick
        // von oben ins Haus die Decke
        (ViewKind::Persp, cam3d(), None, vec3(5000.0, 8000.0, 3000.0)),
        (
            ViewKind::Plan,
            fit_parallel(ViewKind::Plan, bounds, W, H),
            None,
            vec3(5000.0, 7800.0, PLAN_CUT),
        ),
        (
            ViewKind::Back,
            fit_parallel(ViewKind::Back, bounds, W, H),
            None,
            vec3(5000.0, 8000.0, 1500.0),
        ),
        (
            ViewKind::Section,
            fit_parallel(ViewKind::Section, bounds, W, H),
            sect.plane(),
            vec3(5000.0, 8000.0, 1500.0),
        ),
    ];
    for (v, c, plane, p) in cases {
        let (x, y) = px(&c, p);
        let mut sel = Selection::default();
        sel.press(x, y);
        assert!(sel.release(x + 1.0, y, 1.0), "{v:?}: Klick");
        let hit = selection::pick_at(&mut s, &c, v, plane, x, y, W, H);
        assert_eq!(hit, Some(top), "{v:?}: obere Wand gewählt");
        sel.set(hit);
        assert!(
            !selection::helpers(&s, top, v, plane, 1.0, &Theme::dark()).is_empty(),
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
    assert_eq!(value(&p, "Höhe"), "3,50 m");
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
    let mut wall = click(&mut t, &c, RECHTECK[0]).unwrap();
    wall.height = 2750.0;
    let run = s.add_wall(&wall).unwrap();
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
    let mut wall = t.handle(&key(Key::Enter), &c, W, H, 1.0).commit.unwrap();
    wall.height = 2750.0;
    let run = s.add_wall(&wall).unwrap();
    let q = s.wall_qto(s.model().wall_at(run, 0).unwrap()).unwrap();
    let want = 5.0 * 0.315 * 2.75;
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
    assert_eq!(walls(&t).len(), 4);
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
    assert_eq!(t.model().element(el).unwrap().number, "AW-005");
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

/// A24: Dicke parametrisch, von oben nach unten, Standard 20 cm.
#[test]
fn a24_dicke_von_oben_nach_unten() {
    let mut s = Scene::with_model(Model::with_seed(24));
    let run = zeichne_rechteck(&mut s, &cam3d());
    let (slab, _) = sohlplatte(&s, run);
    assert_eq!(
        z_range(&s.foundation(run).unwrap().slab_solid()),
        (-200.0, 0.0)
    );
    assert_eq!(m3(s.foundation_qto(run).unwrap().0.volume), 16.0);
    s.step_recess(slab, true);
    assert_eq!(m3(s.foundation_qto(run).unwrap().0.volume), 15.8563);
    s.step_recess(slab, false);
    assert!(s.edit_model("Dicke", |m| m.set_slab_thickness(slab, 250.0)));
    assert_eq!(
        z_range(&s.foundation(run).unwrap().slab_solid()),
        (-250.0, 0.0)
    );
}

/// A25: Frostschürze umlaufend unter der Platte, außen bündig, 35 × 60 cm.
#[test]
fn a25_frostschuerze() {
    let mut s = Scene::with_model(Model::with_seed(25));
    let run = zeichne_rechteck(&mut s, &cam3d());
    let (slab, _) = sohlplatte(&s, run);
    let f = s.foundation(run).unwrap();
    assert_eq!(z_range(&f.footing_solid()), (-800.0, -200.0));
    let sk_model::FootingShape::Ring(inner) = &f.footing else {
        panic!("Ring erwartet");
    };
    // Innenkante 9,30 × 7,30 m
    assert_eq!(m2(sk_math::polygon::area(&inner.pts)), 67.89);
    assert_eq!(m3(s.foundation_qto(run).unwrap().1.volume), 7.266);
    s.step_recess(slab, true);
    assert_eq!(m3(s.foundation_qto(run).unwrap().1.volume), 7.2324);
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
        7,
        "4 Wände, Sohlplatte, Frostschürze, Erdgeschossdecke"
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
    assert_eq!(p.values[3], ("Fläche", "80,00 m²".to_string()));
    assert_eq!(value(&p, "Volumen"), "16,000 m³");
    let cm = |p: &crate::ui::Props, f| field_cm(p, f).unwrap();
    assert_eq!(cm(&p, crate::ui::Field::SlabThickness), "20");
    let p = selection::props(&s, footing).unwrap();
    assert_eq!(p.values[3], ("Länge (Achse)", "34,60 m".to_string()));
    assert_eq!(value(&p, "Volumen"), "7,266 m³");
    assert_eq!(
        (
            cm(&p, crate::ui::Field::FootingWidth),
            cm(&p, crate::ui::Field::FootingDepth)
        ),
        ("35".into(), "60".into())
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

/// A28: Im Schnitt Stahlbeton-Kreuzschraffur auf Platte und Schürze, kräftige
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
    assert!(has(pattern::CROSS) && has(pattern::DIAGONAL) && has(pattern::ZIGZAG));
    let cross_below = cut
        .faces
        .iter()
        .filter(|v| v[9] == pattern::CROSS)
        .all(|v| v[2] <= 1e-3 || (2635.0 - 1e-3..=2855.0 + 1e-3).contains(&v[2]));
    assert!(
        cross_below,
        "Kreuzschraffur nur in der Gründung und im Band der Erdgeschossdecke (B10)"
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
        flat_at(-200.0, 0.0, 350.0).is_empty(),
        "keine Fuge Platte/Schürze"
    );
    // Kontur außen kräftig: UK Schürze und UK Platte zwischen den Schürzen
    assert!(flat_at(-800.0, 0.0, 350.0).iter().all(|w| *w == cut_w));
    assert!(!flat_at(-800.0, 0.0, 350.0).is_empty());
    assert!(flat_at(-200.0, 400.0, 9600.0).contains(&cut_w));
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
    let mut w = click(&mut t, c, RECHTECK[0]).unwrap();
    w.height = 2750.0;
    s.add_wall(&w).unwrap()
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
    let mut w = t.handle(&key(Key::Enter), c, W, H, 1.0).commit.unwrap();
    w.height = 2750.0;
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
    assert_eq!(m3(tq.1.volume), 7.2324);
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
        assert_eq!(m3(sp.volume), m3(flaeche * 1e6 * 200.0), "Plattenvolumen");
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
// Achtung beim Einbau: A28 prüft „Kreuzschraffur nur in der Gründung“
// (z ≤ 0). Mit der Decke gibt es Kreuzschraffur auch bei +2,635 … +2,855;
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
        (1, 1, 1)
    );
    assert!(s.model().check().is_empty(), "{:?}", s.model().check());

    // Innenwand quer durch: keine zweite Decke
    let set = s.model().defaults().interior_wall;
    let mut t = tool(&s);
    t.set_category(sk_model::Category::InteriorWall, s.model().wall_layers(set));
    click(&mut t, &c, vec3(5000.0, 0.0, 0.0));
    click(&mut t, &c, vec3(5000.0, 8000.0, 0.0));
    let w = t.handle(&key(Key::Enter), &c, W, H, 1.0).commit.unwrap();
    s.add_wall_as(&w, sk_model::Category::InteriorWall).unwrap();
    assert_eq!(anzahl(&s, "DE-"), 1);

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
        (netto - (20.93525 - 1.495375)).abs() < 5e-5,
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
    assert!((gas - 19.61932).abs() < 5e-5, "Gasbeton netto {gas}");
    let daemmung = aw_schicht(&s, run, 0);
    assert!((daemmung - 17.3656).abs() < 5e-5, "Dämmung {daemmung}");
    // Keine Doppelzählung: netto + Tasche = brutto
    assert!((gas + 1.31593 - 20.93525).abs() < 5e-5);

    // Innenwand IW 17,5 bei x = 5 m, Höhe 3,50: netto ohne Deckenstreifen
    let set = s.model().defaults().interior_wall;
    let mut t = tool(&s);
    t.set_category(sk_model::Category::InteriorWall, s.model().wall_layers(set));
    click(&mut t, &c, vec3(5000.0, 0.0, 0.0));
    click(&mut t, &c, vec3(5000.0, 8000.0, 0.0));
    let w = t.handle(&key(Key::Enter), &c, W, H, 1.0).commit.unwrap();
    assert_eq!(w.height, 3500.0);
    let iw = s.add_wall_as(&w, sk_model::Category::InteriorWall).unwrap();
    assert!(
        (wall_m3(&s, iw, 0) - 4.23038).abs() < 5e-5,
        "IW {}",
        wall_m3(&s, iw, 0)
    );
    assert_eq!(anzahl(&s, "IW-"), 1, "ein Bauteil, zwei Körperteile");
    assert!(s.model().check().is_empty(), "{:?}", s.model().check());
}

/// A35: Schnitt mit Stahlbeton-Kreuzschraffur in der Tasche und kräftiger
/// Kontur; im Grundriss (+1,00) wird die Decke nicht gezeichnet.
#[test]
fn a35_darstellung_schnitt_und_grundriss() {
    let mut s = Scene::with_model(Model::with_seed(36));
    zeichne_rechteck(&mut s, &cam3d());
    let mut sect = SectionLine::default();
    sect.ensure(&s);
    let cut = view_mesh(&mut s, ViewKind::Section, sect.plane());
    let in_band = |z: f32| (2635.0 - 1e-3..=2855.0 + 1e-3).contains(&z);
    // Kreuzschraffur im Deckenband, auch in der Tasche (x 140 … 315)
    let cross: Vec<_> = cut
        .faces
        .iter()
        .filter(|v| v[9] == pattern::CROSS && in_band(v[2]))
        .collect();
    assert!(!cross.is_empty(), "Decke im Schnitt");
    assert!(
        cross.iter().any(|v| v[0] > 139.0 && v[0] < 316.0),
        "Kreuzschraffur in der Tasche"
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
    // Grundriss bei +1,00: keine Kreuzschraffur
    let plan = view_mesh(&mut s, ViewKind::Plan, None);
    assert!(
        !plan.faces.iter().any(|v| v[9] == pattern::CROSS),
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
    assert_eq!(text.lines().filter(|l| l.starts_with("[floor]")).count(), 1);
    let loaded = crate::document::load(&path).unwrap();
    assert!(loaded.hints.is_empty(), "{:?}", loaded.hints);
    let t = Scene::with_model(loaded.model);
    let trun = t
        .model()
        .runs()
        .ids()
        .find(|r| decke(&t, *r).is_some())
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
            .any(|h| h.contains("Erdgeschossdecke ergänzt")),
        "{:?}",
        loaded.hints
    );
    let t = Scene::with_model(loaded.model);
    let trun = t
        .model()
        .runs()
        .ids()
        .find(|r| decke(&t, *r).is_some())
        .expect("Decke ergänzt");
    assert_eq!(decke_hoehen(&t, trun), (2635.0, 2855.0));
    assert_eq!(decke_mengen(&t, trun).0, 75.0384);
    let guid = |s: &Scene, id| s.model().element(id).unwrap().guid;
    let (tslab, tfooting) = sohlplatte(&t, trun);
    assert_eq!(
        (guid(&t, tslab), guid(&t, tfooting)),
        (guid(&s, slab), guid(&s, footing))
    );
    assert!((aw_schicht(&t, trun, 1) - 19.61932).abs() < 5e-5);
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
    assert_eq!((m2(q.0.area), m3(q.0.volume)), (80.0, 16.0));
    assert_eq!((q.1.width, q.1.depth), (350.0, 600.0));
}
// ---------------------------------------------------------------------------
// A39–A47: Geschossbänder (Pakete B11, E14, G4; Lastenheft H-01–H-07, A-09)
// Sollwerte: bim/paket-b11-ebenen.md „Fertig, wenn“ nach Jörns Festlegung
// vom 06.10. 08:13 (von Hand nachgerechnet):
//   Gründung −0,80 … ±0,00 · EG ±0,00 … +2,855 · OG +2,855 … +5,71
//   lichte Höhe EG 2,635 · Decke UK +2,635 / OK +2,855 · Wände 3,50 ab EG.UK
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

const STANDARD: [f64; 6] = [17.3656, 19.6193, 4.2304, 16.5084, 16.0, 7.266];

/// Höchster Punkt des Modells in 3D (Wandkrone, die Decke liegt darunter).
fn wandkrone(s: &mut Scene) -> f64 {
    let m = s.mesh(ViewKind::Persp, None, &[]);
    m.faces.iter().map(|v| v[2] as f64).fold(f64::MIN, f64::max)
}

/// A39 (H-02, H-07, A-09): Drei Geschossbänder lückenlos übereinander,
/// Decke an OK EG, Gründung an UK Gründung, Wände 3,50 ab EG.UK.
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
    assert_eq!(z_range(&f.slab_solid()), (-200.0, 0.0));
    assert_eq!(z_range(&f.footing_solid()), (-800.0, -200.0));
    assert!(
        (wandkrone(&mut s) - 3500.0).abs() < 1e-2,
        "Wände bleiben 3,50"
    );
    // Bindung (A-09) am Verhalten: OK EG bewegt Decke und OG-Band, nicht die
    // Wände; UK Gründung bewegt nur die Frostschürze.
    kante_ziehen(&mut s, "EG.OK", &[3000.0], false);
    assert_eq!(decke_hoehen(&s, aw), (2780.0, 3000.0));
    assert_eq!(band(&s, "OG"), (3000.0, 5855.0), "OG-Band wandert mit");
    assert_eq!(mengen_b11(&s, aw, iw), STANDARD, "Mengen gleich");
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
/// Esc bricht ab; Klemmen an Wandkrone und lichter Höhe 1,00; OK OG ändert
/// nur die OG-Höhe.
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
    assert_eq!(mengen_b11(&s, aw, iw), STANDARD);
    assert!(s.model().check().is_empty(), "{:?}", s.model().check());
    assert!(s.undo(), "ein Schritt für das ganze Ziehen");
    assert_eq!(band(&s, "EG"), (0.0, 2855.0));
    assert_eq!(decke_hoehen(&s, aw), (2635.0, 2855.0));
    // Esc: nichts geändert, nichts im Verlauf
    kante_ziehen(&mut s, "EG.OK", &[3100.0, 3200.0], true);
    assert_eq!(band(&s, "EG"), (0.0, 2855.0));
    // Klemmen: Decke bündig mit der Wandkrone bzw. lichte Höhe 1,00
    kante_ziehen(&mut s, "EG.OK", &[3400.0, 3600.0], false);
    assert_eq!(band(&s, "EG").1, 3500.0);
    assert_eq!(decke_hoehen(&s, aw), (3280.0, 3500.0));
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
        m3(12.11e6 * 700.0)
    );
    assert!(s.undo());
    // Deckendicke 25 cm: OK EG bleibt, lichte Höhe sinkt
    let d = decke(&s, aw).unwrap();
    assert!(decke_dicke(&mut s, d, 250.0));
    assert_eq!((band(&s, "EG").1, lichte_hoehe(&s)), (2855.0, 2605.0));
    assert_eq!(decke_hoehen(&s, aw), (2605.0, 2855.0));
    let m = mengen_b11(&s, aw, iw);
    assert_eq!((m[1], m[2], m[3]), (19.4399, 4.1917, 18.7596));
}

/// A42 (H-07): UK Gründung und Plattendicke steuern die Frostschürze.
#[test]
fn a42_gruendung_und_plattendicke() {
    let mut s = Scene::with_model(Model::with_seed(42));
    let (aw, iw) = haus_b11(&mut s);
    kante_ziehen(&mut s, "GR.UK", &[-820.0, -850.0], false);
    let m = mengen_b11(&s, aw, iw);
    assert_eq!((m[4], m[5]), (16.0, 7.8715), "Schürze 0,65 tief");
    assert_eq!(
        z_range(&s.foundation(aw).unwrap().footing_solid()),
        (-850.0, -200.0)
    );
    assert!(s.undo());
    kante_ziehen(&mut s, "GR.UK", &[-500.0, -250.0], false);
    assert_eq!(band(&s, "GR").0, -300.0, "klemmt bei Schürze 0,10");
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
    assert_eq!(band(&s, "GR").0, -900.0);
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
        !mass_eingeben(&mut s, "Geschosshöhe EG", 3600.0),
        "über der Wandkrone"
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
    // Wänden 2,75 (OK EG klemmt auf die Wandkrone)
    let floor = "[floor] guid=3DeckeB10Pruefung00001 run=1DHdFlTxf5TA6lZFrCJBxb \
                 number=\"DE-001\" cat=floor mat=10re9EBlDC5uUUBY9M$lyC t=220 top=2330 \
                 seq=4 storey=0yWX_$MH11OwV$3JY6X$_o\n";
    let niedrig = HAUS_SZO1.replace(" h=3500 ", " h=2750 ");
    for (name, text, ok_eg) in [
        ("ohne-decke", HAUS_SZO1.to_string(), 2855.0),
        ("mit-decke", format!("{HAUS_SZO1}{floor}"), 2855.0),
        ("niedrig", niedrig, 2750.0),
    ] {
        let path = d.join(format!("{name}.szo"));
        std::fs::write(&path, &text).unwrap();
        let loaded = crate::document::load(&path).unwrap();
        assert!(
            loaded
                .hints
                .iter()
                .any(|h| h.contains("auf Geschossverwaltung umgestellt")),
            "{name}: {:?}",
            loaded.hints
        );
        let s = Scene::with_model(loaded.model);
        assert_eq!(band(&s, "GR"), (-800.0, 0.0), "{name}");
        assert_eq!(band(&s, "EG"), (0.0, ok_eg), "{name}");
        let aw = s
            .model()
            .runs()
            .ids()
            .find(|r| decke(&s, *r).is_some())
            .expect("Decke");
        let iw = s.model().runs().ids().find(|r| *r != aw).unwrap();
        assert_eq!(decke_hoehen(&s, aw), (ok_eg - 220.0, ok_eg), "{name}");
        if ok_eg == 2855.0 {
            assert_eq!(mengen_b11(&s, aw, iw), STANDARD, "{name}");
        }
        assert!(
            s.model().check().is_empty(),
            "{name}: {:?}",
            s.model().check()
        );
        let p2 = d.join(format!("{name}-2.szo"));
        crate::document::save(s.model(), &p2).unwrap();
        let neu = std::fs::read_to_string(&p2).unwrap();
        assert!(neu.starts_with("SZO 2"), "{name}");
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
        .find(|r| decke(&t, *r).is_some())
        .unwrap();
    let tiw = t.model().runs().ids().find(|r| *r != taw).unwrap();
    assert_eq!(mengen_b11(&t, taw, tiw), vorher);
    assert_eq!(decke_hoehen(&t, taw), (2780.0, 3000.0));
    let p2 = d.join("Haus2.szo");
    crate::document::save(t.model(), &p2).unwrap();
    assert_eq!(std::fs::read(&p2).unwrap(), std::fs::read(&path).unwrap());
    let _ = std::fs::remove_dir_all(&d);
}

/// A46 (H-06): Schnitt und Ansicht folgen: Decke im Schnitt bei OK EG,
/// Wandkrone bleibt +3,50.
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
            .filter(|v| v[9] == pattern::CROSS)
            .map(|v| v[2])
            .fold(f32::MIN, f32::max)
    };
    let cut = view_mesh(&mut s, ViewKind::Section, sect.plane());
    assert!((top(&cut) - 3500.0).abs() < 1e-2, "Wandkrone im Schnitt");
    assert!((cross_top(&cut) - 2855.0).abs() < 1e-2, "Decke im Schnitt");
    kante_ziehen(&mut s, "EG.OK", &[3200.0], false);
    let cut = view_mesh(&mut s, ViewKind::Section, sect.plane());
    assert!((top(&cut) - 3500.0).abs() < 1e-2);
    assert!(
        (cross_top(&cut) - 3200.0).abs() < 1e-2,
        "Decke folgt auf +3,20"
    );
    let front = view_mesh(&mut s, ViewKind::Front, None);
    assert!(
        (top(&front) - 3500.0).abs() < 1e-2,
        "Ansicht bis zur Wandkrone"
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
