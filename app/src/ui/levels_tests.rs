//! Tests der Paneele (aus ui.rs, Paket T3).

use super::*;
use crate::scene::Scene;

const M: Modifiers = Modifiers {
    shift: false,
    ctrl: false,
    alt: false,
};

/// Das Diagramm im Geschossfenster, 460 × 900 Pixel: nur dort steht es,
/// dort wird gezogen, getippt und ein Geschoss gewählt (Jörn 11. und
/// 12.10.).
fn ui_mit_geschossen() -> (Ui, Scene) {
    let s = Scene::new();
    let mut ui = Ui::fenster(1.0, &Theme::dark());
    ui.fit_fenster(1.0, 460, 900, 32);
    ui.set_levels(s.levels());
    (ui, s)
}

fn band(ui: &Ui, name: &str) -> Band {
    ui.levels
        .bands
        .iter()
        .find(|b| b.name == name)
        .cloned()
        .unwrap()
}

/// Fensterlage einer Linie (am Griff) für die Höhe `z`.
fn griff(ui: &Ui, z: f64) -> (f64, f64) {
    let r = ui.rect(Panel::Levels, 1440, 32);
    let lines = level_lines(&ui.levels);
    let ys = ui.line_ys(&lines, &ui.levels_layout());
    let i = lines.iter().position(|l| l.z == z).unwrap();
    let (x0, _, _) = ui.level_columns();
    ((r.x + x0 + 5.0) as f64, (r.y + ys[i]) as f64)
}

#[test]
fn drei_baender_und_vier_linien() {
    let (ui, _) = ui_mit_geschossen();
    let names: Vec<_> = ui.levels.bands.iter().map(|b| b.name.as_str()).collect();
    assert_eq!(names, ["Fundament", "EG", "OG"]);
    let lines = level_lines(&ui.levels);
    let z: Vec<_> = lines.iter().map(|l| l.z).collect();
    assert_eq!(z, [-800.0, 0.0, 2855.0, 5710.0]);
    assert_eq!(lines[3].name, "OK Decke OG");
    assert!(
        lines[1].grip.is_none() && lines[1].field.is_none(),
        "±0,00 fest"
    );
    assert!(lines[1].active, "EG aktiv");
    assert_eq!(kote_text(-800.0), "\u{2212}0,80");
    assert_eq!(kote_text(0.0), "±0,00");
    assert_eq!(kote_text(2855.0), "+2,855");
    assert_eq!(m_text(2635.0), "2,635");
    // Linien von unten nach oben im Maßstab des Fensters
    let l = ui.levels_layout();
    let ys = ui.line_ys(&lines, &l);
    assert!((ys[1] - ys[2] - 2.855 * l.ppm).abs() < 1e-3, "{l:?}");
    assert!((ys[0] - ys[1] - 0.8 * l.ppm).abs() < 1e-3, "{l:?}");
}

/// Test 2 und 4 aus E14: OG-Griff 30 px hoch, auf 1 cm gefangen; Esc
/// meldet die App über `cancel_level_drag`.
#[test]
fn griff_ziehen_meldet_gefangene_hoehe() {
    let (mut ui, _) = ui_mit_geschossen();
    let ppm = ui.levels_layout().ppm as f64;
    let og = band(&ui, "OG");
    let (x, y) = griff(&ui, og.top);
    let out = ui.handle(&Event::MouseMove { x, y, mods: M }, 1440, 32);
    assert_eq!(ui.hover, Some(Id::Grip(Grip::Top(og.id))));
    assert!(out.consumed);
    let h = ui.rect(Panel::Levels, 1440, 32).h;
    let down = Event::MouseDown {
        button: MouseButton::Left,
        x,
        y,
        mods: M,
    };
    let out = ui.handle(&down, 1440, 32);
    assert_eq!(out.level, Some(LevelEvent::Begin(Grip::Top(og.id))));
    let out = ui.handle(
        &Event::MouseMove {
            x,
            y: y - 30.0,
            mods: M,
        },
        1440,
        32,
    );
    // 30 px im Maßstab des Fensters, auf 1 cm gefangen
    let z = og.top + 30.0 / ppm * 1000.0;
    assert_eq!(
        out.level,
        Some(LevelEvent::Move(
            Grip::Top(og.id),
            (z / 10.0).round() * 10.0
        ))
    );
    // Das Paneel bleibt beim Ziehen stehen, auch wenn die Werte wachsen
    let mut l = ui.levels.clone();
    l.bands[2].top += 750.0;
    ui.set_levels(l);
    assert_eq!(ui.rect(Panel::Levels, 1440, 32).h, h);
    let shift = Modifiers { shift: true, ..M };
    let out = ui.handle(
        &Event::MouseMove {
            x,
            y: y - 30.0,
            mods: shift,
        },
        1440,
        32,
    );
    assert_eq!(
        out.level,
        // mit Umschalt auf 5 cm
        Some(LevelEvent::Move(
            Grip::Top(og.id),
            (z / 50.0).round() * 50.0
        ))
    );
    let up = Event::MouseUp {
        button: MouseButton::Left,
        x,
        y,
        mods: M,
    };
    let out = ui.handle(&up, 1440, 32);
    assert_eq!(out.level, Some(LevelEvent::End));
    assert!(out.relayout && ui.level_dragging().is_none());
    // ±0,00 hat keinen Griff; bei Versatz 0 liegt dort OK Gelände,
    // das sich ziehen lässt
    let (x, y) = griff(&ui, 0.0);
    let out = ui.handle(&down_at(x, y), 1440, 32);
    assert_eq!(out.level, Some(LevelEvent::Begin(Grip::Terrain)));
    assert!(ui.cancel_level_drag());
}

fn down_at(x: f64, y: f64) -> Event {
    Event::MouseDown {
        button: MouseButton::Left,
        x,
        y,
        mods: M,
    }
}

fn klick_auf(ui: &mut Ui, f: Field) {
    let r = ui.rect(Panel::Levels, 1440, 32);
    let b = ui.field_rect(f).unwrap();
    let (x, y) = (
        (r.x + b.x + b.w * 0.5) as f64,
        (r.y + b.y + b.h * 0.5) as f64,
    );
    ui.handle(&Event::MouseMove { x, y, mods: M }, 1440, 32);
    ui.handle(&down_at(x, y), 1440, 32);
}

fn tippe(ui: &mut Ui, text: &str) -> UiOut {
    for c in text.chars() {
        ui.key(Key::Char(c), true, M);
    }
    ui.key(Key::Enter, true, M).unwrap()
}

/// Test 3 aus E14: Klick auf die lichte Höhe, „2,60“ und Enter; „0“ wird
/// mit Grund abgelehnt.
#[test]
fn masszahl_als_zahl_in_metern() {
    let (mut ui, _) = ui_mit_geschossen();
    let eg = band(&ui, "EG").id;
    let f = Field::ClearHeight(eg);
    klick_auf(&mut ui, f);
    let e = ui.edit.as_ref().expect("Eingabe offen");
    assert_eq!((e.field, e.text.as_str()), (f, "2,635"));
    let out = tippe(&mut ui, "0");
    assert_eq!(out.submit, None);
    assert_eq!(
        ui.edit.as_ref().unwrap().error.as_deref(),
        Some("mindestens 1,00 m")
    );
    ui.key(Key::Backspace, true, M);
    let out = tippe(&mut ui, "2,60");
    assert_eq!(out.submit, Some((f, 2600.0)));
    assert!(out.relayout && ui.edit.is_none());
    // Kote der Gründung mit Vorzeichen, Punkt oder Komma
    klick_auf(&mut ui, Field::LevelBottom);
    assert_eq!(ui.edit.as_ref().unwrap().text, "-0,80");
    let out = tippe(&mut ui, "-0.9");
    assert_eq!(out.submit, Some((Field::LevelBottom, -900.0)));
    // Esc verwirft
    klick_auf(&mut ui, Field::StoreyHeight(eg));
    ui.key(Key::Char('9'), true, M);
    let out = ui.key(Key::Escape, true, M).unwrap();
    assert!(out.submit.is_none() && ui.edit.is_none());
}

/// Flachdach (Jörn 10.10.): Der Schalter rechts in der Titelzeile legt
/// über dem OG das Band FD an, OK Aufkantung 0,50 m über OK Decke OG,
/// mit Feldern für OK und Höhe der Aufkantung, ohne lichte Höhe.
/// Rückgängig nimmt es wieder weg.
#[test]
fn flachdach_band_und_schalter() {
    let (mut ui, mut s) = ui_mit_geschossen();
    assert_eq!(ui.levels.flat_roof, Some(false));
    let r = ui.rect(Panel::Levels, 1440, 32);
    let b = ui.flat_roof_rect().unwrap();
    let (x, y) = ((r.x + b.x + 4.0) as f64, (r.y + b.y + 6.0) as f64);
    ui.handle(&Event::MouseMove { x, y, mods: M }, 1440, 32);
    assert_eq!(ui.hover, Some(Id::FlatRoof));
    let down = Event::MouseDown {
        button: MouseButton::Left,
        x,
        y,
        mods: M,
    };
    ui.handle(&down, 1440, 32);
    let up = Event::MouseUp {
        button: MouseButton::Left,
        x,
        y,
        mods: M,
    };
    assert_eq!(ui.handle(&up, 1440, 32).clicked, Some(Id::FlatRoof));
    assert!(s.set_flat_roof(true));
    assert!(!s.set_flat_roof(true), "schon an");
    ui.set_levels(s.levels());
    assert_eq!(ui.levels.flat_roof, Some(true));
    let names: Vec<_> = ui.levels.bands.iter().map(|b| b.name.as_str()).collect();
    assert_eq!(names, ["Fundament", "EG", "OG", "FD"]);
    let lines = level_lines(&ui.levels);
    let last = lines.last().unwrap();
    assert_eq!((last.z, last.name.as_str()), (6210.0, "OK Aufkantung"));
    let fd = band(&ui, "FD");
    assert!(fd.roof && !fd.foundation);
    let top = ui.field_row(Field::LevelTop(fd.id)).unwrap();
    assert_eq!(
        (top.label, top.min, top.max),
        ("OK Aufkantung", 5860.0, 7210.0)
    );
    let h = ui.field_row(Field::StoreyHeight(fd.id)).unwrap();
    assert_eq!((h.label, h.value), ("Aufkantung", 500.0));
    assert!(ui.field_row(Field::ClearHeight(fd.id)).is_none());
    assert!(ui.levels.clear.iter().all(|c| c.0 != fd.id));
    assert!(s.undo());
    ui.set_levels(s.levels());
    assert_eq!(ui.levels.bands.len(), 3);
    assert_eq!(ui.levels.flat_roof, Some(false));
}

/// A47 und Test 5 aus E14: auch im kleinen Fenster nichts abgeschnitten.
/// Die Zahlenliste für zu wenig Platz entfällt (Jörn 11.10.); das
/// Geschossfenster zeigt das Diagramm, im knappen Fenster rücken die
/// Linien auf den Zeilenabstand zusammen, bei 200 px weicht der Fuß mit
/// dem Gelände.
#[test]
fn kleines_fenster_immer_das_diagramm() {
    let (mut ui, _) = ui_mit_geschossen();
    for (w, h) in [(460u32, 600u32), (460, 400), (460, 300), (460, 200)] {
        ui.fit_fenster(1.0, w, h, 32);
        let r = ui.rect(Panel::Levels, w, 32);
        assert!(r.y == 32.0 && r.y + r.h == h as f32, "{w}×{h}: {r:?}");
        // Zahlen (mit lichter Höhe EG und OG), die Namen von
        // Fundament, EG und OG (E18: das Fundament ist anklickbar) und
        // die Kote OK Gelände im Fuß, dazu in der Titelzeile die Schalter
        // Flachdach (Jörn 10.10.), Fenster und Innentüren (05:26)
        let fuss = ui.levels_layout().fuss > 0.0;
        assert_eq!(fuss, h > 200, "{w}×{h}");
        assert_eq!(
            ui.level_buttons().len(),
            if fuss { 15 } else { 14 },
            "{w}×{h}"
        );
        for (_, b, _) in ui.level_buttons() {
            assert!(b.y >= 0.0 && b.y + b.h <= r.h, "{w}×{h}: {b:?}");
        }
        let l = ui.levels_layout();
        let ys = ui.line_ys(&level_lines(&ui.levels), &l);
        assert!(ys.iter().all(|&y| y > 0.0 && y < r.h), "{w}×{h}: {ys:?}");
    }
}

/// Hover über Griff und Maßzahl zeichnet das Paneel neu, gleich dem
/// vollen Zeichnen; Test 6: aktive Linie folgt dem Akzent.
#[test]
fn neu_zeichnen_gleicht_dem_ganzen_paneel() {
    let mut th = Theme::dark();
    let (mut ui, _) = ui_mit_geschossen();
    let og = band(&ui, "OG").id;
    for id in [Id::Grip(Grip::Top(og)), Id::Field(Field::StoreyHeight(og))] {
        ui.paint(&th, Panel::Levels, 1440, 32);
        ui.hover = Some(id);
        let patch = ui.repaint_button(&th, id).expect("Paneelbild vorhanden");
        let full = ui.paint(&th, Panel::Levels, 1440, 32).0.to_premul_rgba8();
        assert!(patch.px == full, "{id:?}");
    }
    let blue = Rgba::rgb(40, 120, 220);
    th.set_accent(blue);
    assert!(ui.repaint_button(&th, Id::Grip(Grip::Top(og))).is_none());
    ui.hover = None;
    let lines = level_lines(&ui.levels);
    let ys = ui.line_ys(&lines, &ui.levels_layout());
    let c = ui.paint(&th, Panel::Levels, 1440, 32).0;
    let m = th.size.panel_shadow;
    let px = c.to_rgba8();
    let (x, y) = ((m + 120.0) as usize, (m + ys[1]).round() as usize);
    let i = (y * c.width + x) * 4;
    assert_eq!(Rgba(px[i], px[i + 1], px[i + 2], px[i + 3]), blue);
}

/// Mitte der Klickfläche eines Geschossnamens im Fenster.
fn name_at(ui: &Ui, id: StoreyId) -> Option<(f64, f64)> {
    let r = ui.rect(Panel::Levels, 1440, 32);
    ui.buttons(Panel::Levels)
        .into_iter()
        .find(|b| b.0 == Id::Storey(id))
        .map(|(_, b, _)| {
            (
                (r.x + b.x + b.w * 0.5) as f64,
                (r.y + b.y + b.h * 0.5) as f64,
            )
        })
}

/// E14b Test 1: Zeiger über Griff, Maßzahl, Feld in Eingabe, Name und daneben.
#[test]
fn zeiger_folgt_dem_hover() {
    let (mut ui, _) = ui_mit_geschossen();
    let og = band(&ui, "OG");
    let mv = |ui: &mut Ui, (x, y): (f64, f64)| {
        ui.handle(&Event::MouseMove { x, y, mods: M }, 1440, 32);
        ui.cursor()
    };
    let g = griff(&ui, og.top);
    assert_eq!(mv(&mut ui, g), Cursor::SizeNS);
    assert_eq!(mv(&mut ui, (700.0, 400.0)), Cursor::Arrow, "über 3D");
    let f = Field::StoreyHeight(og.id);
    let r = ui.rect(Panel::Levels, 1440, 32);
    let b = ui.field_rect(f).unwrap();
    let at = (
        (r.x + b.x + b.w * 0.5) as f64,
        (r.y + b.y + b.h * 0.5) as f64,
    );
    assert_eq!(mv(&mut ui, at), Cursor::Hand, "Maßzahl");
    let down = Event::MouseDown {
        button: MouseButton::Left,
        x: at.0,
        y: at.1,
        mods: M,
    };
    ui.handle(&down, 1440, 32);
    assert!(ui.edit.is_some());
    let b = ui.field_rect(f).unwrap();
    let at = (
        (r.x + b.x + b.w * 0.5) as f64,
        (r.y + b.y + b.h * 0.5) as f64,
    );
    assert_eq!(mv(&mut ui, at), Cursor::IBeam, "Feld in Eingabe");
    ui.key(Key::Escape, true, M);
    let n = name_at(&ui, og.id).unwrap();
    assert_eq!(mv(&mut ui, n), Cursor::Hand, "Name");
    // Beim Ziehen gilt der Zeiger auch außerhalb des Paneels
    let (x, y) = griff(&ui, og.top);
    mv(&mut ui, (x, y));
    let down = Event::MouseDown {
        button: MouseButton::Left,
        x,
        y,
        mods: M,
    };
    ui.handle(&down, 1440, 32);
    assert_eq!(mv(&mut ui, (700.0, 100.0)), Cursor::SizeNS);
    assert!(ui.level_drag_z().is_some());
}

/// E14b Test 3 (Oberfläche): Klick auf „OG“ meldet das Geschoss; auch das
/// Fundament hat einen klickbaren Namen (E18).
#[test]
fn klick_auf_geschossnamen() {
    let (mut ui, mut s) = ui_mit_geschossen();
    let og = band(&ui, "OG");
    assert!(name_at(&ui, band(&ui, "Fundament").id).is_some());
    let (x, y) = name_at(&ui, og.id).unwrap();
    let mut clicked = None;
    for e in [
        Event::MouseMove { x, y, mods: M },
        Event::MouseDown {
            button: MouseButton::Left,
            x,
            y,
            mods: M,
        },
        Event::MouseUp {
            button: MouseButton::Left,
            x,
            y,
            mods: M,
        },
    ] {
        clicked = clicked.or(ui.handle(&e, 1440, 32).clicked);
    }
    assert_eq!(clicked, Some(Id::Storey(og.id)));
    assert!(s.set_active_storey(og.id));
    ui.set_levels(s.levels());
    assert!(band(&ui, "OG").active && !band(&ui, "EG").active);
    // Auch im kleinen Fenster
    ui.fit_fenster(1.0, 300, 380, 32);
    assert!(name_at(&ui, og.id).is_some());
    assert!(name_at(&ui, band(&ui, "Fundament").id).is_some());
}

/// Jörn 11.10.: Nach Klick auf „OG“ wurde das Diagramm zur Liste, zurück
/// ging es nur über „EG“. Die Liste gibt es nicht mehr: der Hinweis zum
/// aktiven Geschoss in den „Werkzeuge“ steht immer, das Paneel bleibt
/// über dem Fensterrand und zeigt alle Namen.
#[test]
fn geschosswechsel_behaelt_das_diagramm() {
    let mut s = Scene::new();
    let mut ui = Ui::new(1.25, &Theme::dark());
    ui.projekt_zeilen = vec!["Haus Mustermann".into(), "Projekt-Nr. 01/26".into()];
    ui.ext_knopf = true;
    let wechsel = |ui: &mut Ui, s: &mut Scene, name: &str| {
        let id = band(ui, name).id;
        assert!(s.set_active_storey(id) || band(ui, name).active);
        ui.set_levels(s.levels());
        ui.upper_active = !s.ground_active() && !s.foundation_active();
        ui.foundation_active = s.foundation_active();
    };
    let hinweis = |ui: &Ui| {
        ui.rows(Panel::Tools)
            .iter()
            .any(|r| matches!(r, Row::Hint(h) if h.starts_with("Außenwände") || h.starts_with("Im Fundament")))
    };
    // 1920 × 1040 bei 125 %: knapp genug, dass früher die Liste kam;
    // im Hauptfenster steht unten der Knopf, das Diagramm im
    // Geschossfenster daneben
    ui.fit(1.25, 1920, 1040);
    ui.set_levels(s.levels());
    let mut f = Ui::fenster(1.25, &Theme::dark());
    f.fit_fenster(1.25, 575, 1040, 40);
    for (name, h) in [
        ("OG", true),
        ("EG", false),
        ("Fundament", true),
        ("OG", true),
    ] {
        wechsel(&mut ui, &mut s, name);
        assert_eq!(hinweis(&ui), h, "{name}: Hinweis");
        let (t, r) = (
            ui.rect(Panel::Tools, 1920, 32),
            ui.rect(Panel::Levels, 1920, 32),
        );
        assert!(r.y >= t.y + t.h && r.y + r.h <= 1040.0, "{name}: {r:?}");
        f.set_levels(s.levels());
        for b in ["Fundament", "EG", "OG"] {
            assert!(name_at(&f, band(&f, b).id).is_some(), "{name}: {b}");
        }
    }
}
