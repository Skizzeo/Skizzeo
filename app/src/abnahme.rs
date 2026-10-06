//! Abnahmetests: je Anforderung aus dem Katalog `skizzeo/test/katalog.md` ein
//! Test `aNN_…`. Sie bedienen das Programm so, wie Jörn es tut: Mausklicks auf
//! Bildpunkte, Tasten, Knöpfe in den Paneelen. Geprüft wird das Ergebnis, das
//! man sieht oder abliest, nicht der innere Weg dorthin.
//!
//! Alles läuft ohne Fenster: `cargo test -p skizzeo abnahme`. Was nur unter
//! echtem Windows prüfbar ist, steht in `skizzeo/test/handtest.md`.

use crate::camera::Camera;
use crate::scene::{Scene, PLAN_CUT};
use crate::section::SectionLine;
use crate::selection::{self, Selection};
use crate::ui::{Id, Panel, Ui, ViewKind};
use crate::wall_edit::WallEdit;
use crate::wall_tool::{WallTool, WALL_HEIGHT};
use crate::{fit_parallel, fit_perspective};
use sk_math::{vec3, Vec3};
use sk_model::{edge_kind, FillKind, Model, RefSide, RunId, WallChain};
use sk_platform::{Event, Key, Modifiers, MouseButton};
use sk_render::{pattern, MeshData};
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

/// Kanten (a, b, Breite) einer Bauzeichnung, die ganz auf der Geraden x = `x`
/// liegen (zwischen y = 1 m und 7 m).
fn edges_at_x(m: &MeshData, x: f32) -> Vec<f32> {
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

    let plan = s.mesh(ViewKind::Plan, None, &[]);
    let white = [1.0f32, 1.0, 1.0];
    assert!(plan.faces.iter().all(|v| v[6..9] == white), "Flächen weiß");
    let has = |m: &MeshData, pat: f32| m.faces.iter().any(|v| v[9] == pat);
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
    let cut = s.mesh(ViewKind::Section, sect.plane(), &[]);
    assert!(has(&cut, pattern::DIAGONAL) && has(&cut, pattern::ZIGZAG));
    assert!(cut.edges.iter().any(|e| e.1 == cut_w));
    assert!(cut.edges.iter().any(|e| e.1 == layer_w));

    // Ansicht: keine Schraffur, Ansichtskanten mittel
    let front = s.mesh(ViewKind::Front, None, &[]);
    assert!(!has(&front, pattern::DIAGONAL) && !has(&front, pattern::ZIGZAG));
    assert!(front.edges.iter().any(|e| e.1 == view_w));
    // 3D: farbige Flächen, keine Schraffur
    let p3 = s.mesh(ViewKind::Persp, None, &[]);
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
    let cut = s.mesh(ViewKind::Section, sect.plane(), &[]);
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
        (ViewKind::Persp, cam3d(), None, vec3(5000.0, 8000.0, 2000.0)),
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
    assert!((vol - 30.0935).abs() < 5e-5, "Volumen {vol}");
    assert!((ins - 13.6444).abs() < 5e-5, "Dämmung {ins}");
    assert!((gas - 16.4491).abs() < 5e-5, "Gasbeton {gas}");
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
    let sig = |m: &MeshData| {
        let mut f: Vec<_> = m.faces.iter().map(|v| v.map(|x| x.to_bits())).collect();
        f.sort();
        (f, m.edges.len())
    };
    assert_eq!(
        sig(&t.mesh(ViewKind::Plan, None, &[])),
        sig(&s.mesh(ViewKind::Plan, None, &[])),
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
    assert_eq!(p.recess.as_deref(), Some("bündig"));
    assert!(s.step_recess(wall, true));
    let p = selection::props(&s, slab).unwrap();
    assert_eq!(p.recess.as_deref(), Some("2 cm"));
    assert_eq!(m2(s.foundation_qto(run).unwrap().0.area), 79.2816);
    // Die Wand bleibt an der Bezugslinie
    assert_eq!(s.chain(run).unwrap().points, points);
    assert!(s.step_recess(slab, true));
    assert_eq!(
        selection::props(&s, slab).unwrap().recess.as_deref(),
        Some("3 cm")
    );
    assert!(s.step_recess(slab, false) && s.step_recess(slab, false));
    assert_eq!(
        selection::props(&s, slab).unwrap().recess.as_deref(),
        Some("bündig")
    );
    assert!(!s.step_recess(slab, false), "unter 0 geht es nicht");
    assert_eq!(m2(s.foundation_qto(run).unwrap().0.area), 80.0);
    // 1 cm wird abgelehnt
    assert!(!s.edit_model("Rücksprung", |m| m.set_slab_recess(slab, 10.0)));
    // Rückgängig stellt den Rücksprung zurück
    assert!(s.undo());
    assert_eq!(
        selection::props(&s, slab).unwrap().recess.as_deref(),
        Some("2 cm")
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
    assert_eq!(m.elements().len(), 6, "4 Wände, Sohlplatte, Frostschürze");
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
    assert_eq!(value(&p, "Dicke"), "20 cm");
    let p = selection::props(&s, footing).unwrap();
    assert_eq!(p.values[3], ("Länge (Achse)", "34,60 m".to_string()));
    assert_eq!(value(&p, "Volumen"), "7,266 m³");
    assert_eq!(
        (value(&p, "Breite"), value(&p, "Tiefe")),
        ("35 cm".into(), "60 cm".into())
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
    let cut = s.mesh(ViewKind::Section, sect.plane(), &[]);
    let has = |pat: f32| cut.faces.iter().any(|v| v[9] == pat);
    assert!(has(pattern::CROSS) && has(pattern::DIAGONAL) && has(pattern::ZIGZAG));
    let cross_below = cut
        .faces
        .iter()
        .filter(|v| v[9] == pattern::CROSS)
        .all(|v| v[2] <= 1e-3);
    assert!(cross_below, "Kreuzschraffur nur in der Gründung");
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
