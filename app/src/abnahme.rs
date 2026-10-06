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
        assert!(neu.starts_with("SZO 3"), "{name}");
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
    assert!(text.starts_with("SZO 3"));
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
    assert!(neu.starts_with("SZO 3"));
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
/// Speichern ausgegraut bei gespeicherter, unveränderter Datei.
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
        ("—", "", false),
        ("Speichern", "Strg+S", true),
        ("Speichern unter …", "Strg+Umschalt+S", true),
        ("—", "", false),
        ("Einstellungen …", "Strg+Komma", true),
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
    assert_eq!(z[4], ("Speichern".into(), "Strg+S".into(), false));
    assert!(z.iter().enumerate().all(|(i, l)| i == 4 || *l == soll[i]));
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
    // Runter: Öffnen, Zuletzt, (Trennlinie) Speichern
    m.open();
    for _ in 0..3 {
        assert_eq!(menue_taste(&mut m, taste_runter(), true, &r), None);
    }
    assert_eq!(
        menue_taste(&mut m, Key::Enter, true, &r).as_deref(),
        Some("Speichern")
    );
    // Hoch vom Speichern über die Trennlinie zurück auf „Zuletzt geöffnet“,
    // rechts öffnet das Untermenü, links schließt es wieder
    m.open();
    for _ in 0..3 {
        menue_taste(&mut m, taste_runter(), true, &r);
    }
    menue_taste(&mut m, taste_hoch(), true, &r);
    assert!(!m.sub_open());
    menue_taste(&mut m, Key::Right, true, &r);
    assert!(m.sub_open(), "rechts öffnet „Zuletzt geöffnet“");
    menue_taste(&mut m, Key::Left, true, &r);
    assert!(
        !m.sub_open() && m.is_open(),
        "links schließt nur das Untermenü"
    );
    // Neu geöffnet steht die Markierung wieder oben. Ausgegrautes Speichern
    // wird übersprungen: runter ×3 landet auf „Speichern unter …“
    m.click_outside();
    m.open();
    for _ in 0..3 {
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
    assert_eq!(z[i + 1].0, "—");
    assert_eq!(z[i + 2].0, "Schließen");
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
    // Noch kein Gebäude (nur die Vorlage): oben ausgegraut ohne Beschriftung.
    // Bauthread: Ein geschlossenes Rechteck legt schon das ganze Gebäude mit
    // OG an (Modellierungsansatz), daher hier ohne Zeichnen.
    let s = Scene::with_model(Model::with_seed(80));
    assert_eq!(
        bogen_text(&w, &s),
        t("EG", "±0,00", None, Some("Fundament"))
    );
    assert!(!spitze_frei(&w, &s, true, false));
    assert_eq!(spitze_hinweis(&w, &s, true, false), None, "ausgegraut");
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
            [Tat::Lege(Fenster::Mengen, (1300, 100, 780, 800))],
            "520 dip bei 150 %"
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
}
