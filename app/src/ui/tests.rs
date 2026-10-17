//! Tests der Paneele (aus ui.rs, Paket T3).

use super::*;
use sk_platform::Modifiers;

/// Paneelbilder malen in die Leinwand des letzten Bildes (Review 1x):
/// bytegleich wie frisch gemalt, auch nach anderer Skalierung.
#[test]
fn paneele_wiederverwendet_wie_neu() {
    let t = Theme::dark();
    let mut ui = Ui::new(1.0, &t);
    for scale in [1.0, 1.5, 1.5, 1.0] {
        ui.fit(scale, 1600, 900);
        let mut fresh = Ui::new(1.0, &t);
        fresh.fit(scale, 1600, 900);
        for p in [Panel::Tools, Panel::Levels, Panel::Views, Panel::Props] {
            let a = ui.paint(&t, p, 1600, 32).0.to_premul_rgba8();
            let b = fresh.paint(&t, p, 1600, 32).0.to_premul_rgba8();
            assert!(a == b, "{p:?} bei {scale}");
        }
    }
}

fn click(ui: &mut Ui, x: f64, y: f64) -> Option<Id> {
    let m = Modifiers::default();
    ui.handle(&Event::MouseMove { x, y, mods: m }, 1280, 32);
    let down = Event::MouseDown {
        button: MouseButton::Left,
        x,
        y,
        mods: m,
    };
    ui.handle(&down, 1280, 32);
    let up = Event::MouseUp {
        button: MouseButton::Left,
        x,
        y,
        mods: m,
    };
    ui.handle(&up, 1280, 32).clicked
}

/// E16: Dialog rechts neben „Geschosse“, oben bündig; modal; der Zähler
/// ist gesperrt, Gebäude im OG ebenso.
#[test]
fn dialog_gebaeude_erstellen() {
    let mut ui = Ui::new(1.0, &Theme::dark());
    ui.dialog = true;
    let (d, l) = (
        ui.rect(Panel::Dialog, 1280, 32),
        ui.rect(Panel::Levels, 1280, 32),
    );
    assert_eq!((d.w, d.h), (300.0, 290.0));
    assert!(d.x >= l.x + l.w, "rechts neben dem Paneel");
    assert_eq!(d.y, ui.rect(Panel::Tools, 1280, 32).y, "oben bündig");
    let at = |ui: &Ui, id: Id| {
        let (_, b, _) = ui
            .buttons(Panel::Dialog)
            .into_iter()
            .find(|b| b.0 == id)
            .unwrap();
        (
            (d.x + b.x + b.w / 2.0) as f64,
            (d.y + b.y + b.h / 2.0) as f64,
        )
    };
    let (x, y) = at(&ui, Id::DialogStart);
    assert_eq!(click(&mut ui, x, y), Some(Id::DialogStart));
    let (x, y) = at(&ui, Id::DialogMinus);
    assert_eq!(click(&mut ui, x, y), None, "Zähler fest auf 2");
    let (x, y) = at(&ui, Id::DialogClose);
    assert_eq!(click(&mut ui, x, y), Some(Id::DialogClose));
    // Modal: der Knopf „Gebäude“ darunter ist nicht erreichbar
    let r = ui.rect(Panel::Tools, 1280, 32);
    let (_, b, _) = ui.buttons(Panel::Tools)[2];
    let (x, y) = ((r.x + b.x + 5.0) as f64, (r.y + b.y + 5.0) as f64);
    assert_eq!(click(&mut ui, x, y), None);
    let m = Modifiers::default();
    assert!(
        ui.handle(
            &Event::MouseMove {
                x: 900.0,
                y: 500.0,
                mods: m
            },
            1280,
            32
        )
        .consumed
    );
    ui.dialog = false;
    assert_eq!(click(&mut ui, x, y), Some(Id::Building));
    ui.upper_active = true;
    assert_eq!(
        click(&mut ui, x, y),
        None,
        "Außenwände entstehen aus dem EG"
    );
    // Im Fundament sind beide Wandknöpfe gesperrt (E18)
    ui.upper_active = false;
    ui.foundation_active = true;
    assert_eq!(click(&mut ui, x, y), None, "Fundament: Gebäude gesperrt");
}

/// Dialogfelder (Jörn 10:13): Reihenfolge von oben nach unten, jede
/// gültige Taste gilt sofort, Enter springt ins nächste Feld, ungültig
/// sperrt „Zeichnen beginnen“, Esc stellt den alten Wert her.
#[test]
fn dialog_felder() {
    let mut ui = Ui::new(1.0, &Theme::dark());
    ui.dialog = true;
    let row = |d: Draft, value: f64, min: f64, max: f64| FieldRow {
        field: Field::Draft(d),
        label: d.label(),
        value,
        min,
        max,
        zero: false,
        einheit: None,
    };
    ui.set_dialog_fields(vec![
        row(Draft::RoofUpstand, 500.0, 150.0, 1500.0),
        row(Draft::FloorOg, 220.0, 100.0, 600.0),
        row(Draft::ClearOg, 2635.0, 1000.0, 10000.0),
        row(Draft::FloorEg, 220.0, 100.0, 600.0),
        row(Draft::ClearEg, 2635.0, 1000.0, 10000.0),
        row(Draft::Slab, 220.0, 100.0, 790.0),
        row(Draft::Insulation, 0.0, 20.0, 300.0),
        row(Draft::Terrain, 0.0, -3000.0, 3000.0),
    ]);
    // Felder von oben nach unten, alle über den Knöpfen
    let rects: Vec<Rect> = Draft::ALL
        .iter()
        .map(|d| ui.field_rect(Field::Draft(*d)).unwrap())
        .collect();
    let start = ui
        .buttons(Panel::Dialog)
        .into_iter()
        .find(|b| b.0 == Id::DialogStart)
        .unwrap()
        .1;
    assert!(rects.windows(2).all(|w| w[0].y + w[0].h < w[1].y));
    assert!(
        rects[4].y + rects[4].h + 15.0 < start.y,
        "Platz für den Hinweis"
    );
    assert_eq!(rects[1].y - rects[0].y, 30.0, "dialog_row");
    assert_eq!(
        ui.field_row(Field::Draft(Draft::ClearOg))
            .unwrap()
            .field
            .text(2635.0),
        "2,635"
    );
    // Versatz mit Vorzeichen (OG Phase 2, Befund c)
    assert_eq!(Field::Offset.display(300.0), "+0,30");
    assert_eq!(Field::Offset.display(-300.0), "\u{2212}0,30");
    assert_eq!(Field::Offset.display(0.0), "0,00");
    assert_eq!(Field::Offset.text(-300.0), "-0,30");

    ui.focus_field(Field::Draft(Draft::FloorOg));
    let m = Modifiers::default();
    let key = |ui: &mut Ui, k: Key| ui.key(k, true, m).unwrap();
    let out = key(&mut ui, Key::Char('2'));
    assert_eq!(out.submit, None, "2 cm ist zu dünn");
    assert!(ui.is_disabled(Id::DialogStart));
    assert!(ui.edit.as_ref().unwrap().error.is_some());
    let out = key(&mut ui, Key::Char('5'));
    assert_eq!(
        out.submit,
        Some((Field::Draft(Draft::FloorOg), 250.0)),
        "sofort"
    );
    assert!(!ui.is_disabled(Id::DialogStart));
    let out = key(&mut ui, Key::Enter);
    assert_eq!(out.submit, Some((Field::Draft(Draft::FloorOg), 250.0)));
    assert_eq!(
        ui.edit.as_ref().unwrap().field,
        Field::Draft(Draft::ClearOg)
    );
    // lichte Höhe in m
    key(&mut ui, Key::Char('2'));
    key(&mut ui, Key::Char(','));
    key(&mut ui, Key::Char('8'));
    assert_eq!(
        key(&mut ui, Key::Char('0')).submit,
        Some((Field::Draft(Draft::ClearOg), 2800.0))
    );
    // Esc: alter Wert zurück
    assert_eq!(
        key(&mut ui, Key::Escape).submit,
        Some((Field::Draft(Draft::ClearOg), 2635.0))
    );
    assert!(ui.edit.is_none());
    // Nach dem letzten Feld keine Eingabe mehr: Enter beginnt (App)
    ui.focus_field(Field::Draft(Draft::Terrain));
    key(&mut ui, Key::Enter);
    assert!(ui.edit.is_none());
}

/// Sonnenstand S3: Mit Nordrichtung heißen die Ansichten nach der
/// Himmelsrichtung der gezeigten Fassade, ohne bleiben sie „Vorne“ …
#[test]
fn ansichten_nach_himmelsrichtung() {
    use ViewKind::*;
    let namen = |nord| [Front, Back, Left, Right].map(|v| v.knopf(nord));
    assert_eq!(namen(None), ["Vorne", "Hinten", "Links", "Rechts"]);
    assert_eq!(namen(Some(0.0)), ["Süd", "Nord", "West", "Ost"]);
    assert_eq!(namen(Some(90.0)), ["Ost", "West", "Süd", "Nord"]);
    assert_eq!(namen(Some(180.0)), ["Nord", "Süd", "Ost", "West"]);
    assert_eq!(
        namen(Some(45.0)),
        ["Südost", "Nordwest", "Südwest", "Nordost"]
    );
    // Grenze 22,5°: die Hauptrichtung; knapp darüber die Zwischenrichtung
    assert_eq!(Front.knopf(Some(22.5)), "Süd");
    assert_eq!(Front.knopf(Some(337.5)), "Süd");
    assert_eq!(Front.knopf(Some(23.0)), "Südost");
    assert_eq!(Front.knopf(Some(350.0)), "Süd");
    assert_eq!(Front.titel(Some(0.0)), "Südansicht");
    assert_eq!(Left.titel(Some(45.0)), "Südwestansicht");
    assert_eq!(Front.titel(None), "Vorne");
    for v in [Persp, Plan, Section] {
        assert_eq!(v.himmel(Some(30.0)), None);
    }
    assert_eq!(Front.himmel(Some(f64::NAN)), None);
    // Im Paneel, und jeder Name passt auf seinen Knopf
    let mut ui = Ui::new(1.0, &Theme::dark());
    ui.fit(1.0, 1440, 900);
    ui.nord = Some(45.0);
    let b = ui.buttons(Panel::Views);
    let name = |v| b.iter().find(|k| k.0 == Id::View(v)).unwrap().2;
    assert_eq!(name(Front), "Südost");
    assert_eq!(name(Persp), "3D");
    let breite = b.iter().find(|k| k.0 == Id::View(Front)).unwrap().1.w;
    if let Some(f) = ui.fonts.bold.as_ref() {
        for n in HIMMEL {
            assert!(f.width(n, ui.size.font) + 16.0 < breite, "{n} in {breite}");
        }
    }
}

/// Ist-Bilder der Ansichtsknöpfe (Sonnenstand S3): ohne Nordrichtung und
/// bei Nord 30° (Südost, Nordwest, Südwest, Nordost), 100 % und 200 %.
/// `SKIZZEO_ISTBILDER=<ordner> cargo test -p skizzeo istbild_ansichten -- --ignored`
#[test]
#[ignore = "legt Ist-Bilder ab, nur mit SKIZZEO_ISTBILDER"]
fn istbild_ansichten() {
    let Some(dir) = std::env::var_os("SKIZZEO_ISTBILDER") else {
        return;
    };
    let dir = std::path::PathBuf::from(dir);
    let t = Theme::dark();
    let lib = std::path::Path::new("/usr/share/fonts/truetype/liberation");
    let lade = |n: &str| {
        std::fs::read(lib.join(n))
            .ok()
            .and_then(sk_paint::font::Font::parse)
    };
    for (n, nord, sc) in [
        ("ohne", None, 1.0),
        ("nord-30", Some(30.0), 1.0),
        ("nord-30-200", Some(30.0), 2.0),
    ] {
        let mut ui = Ui::new(sc, &t);
        ui.nord = nord;
        ui.fonts = Fonts {
            regular: lade("LiberationSans-Regular.ttf"),
            bold: lade("LiberationSans-Bold.ttf"),
            italic: None,
        };
        let (c, _, _) = ui.paint(&t, Panel::Views, 1280, 32);
        std::fs::write(dir.join(format!("ist-s3-ansichten-{n}.png")), c.to_png()).unwrap();
    }
}

/// Ist-Bilder der Kachel (Sonnenstand S4): Nord 30°, Sonnenstand an
/// (hervorgehoben) und aus, und mit langer Bezeichnung und
/// Projektnummer (§8 10:25).
/// `SKIZZEO_ISTBILDER=<ordner> cargo test -p skizzeo istbild_kachel_sonne -- --ignored`
#[test]
#[ignore = "legt Ist-Bilder ab, nur mit SKIZZEO_ISTBILDER"]
fn istbild_kachel_sonne() {
    let Some(dir) = std::env::var_os("SKIZZEO_ISTBILDER") else {
        return;
    };
    let dir = std::path::PathBuf::from(dir);
    let t = Theme::dark();
    let lib = std::path::Path::new("/usr/share/fonts/truetype/liberation");
    let lade = |n: &str| {
        std::fs::read(lib.join(n))
            .ok()
            .and_then(sk_paint::font::Font::parse)
    };
    for (n, an) in [("an", true), ("aus", false), ("projektdaten", false)] {
        let mut ui = Ui::new(2.0, &t);
        ui.nord = Some(30.0);
        ui.sonne_an = an;
        if n == "projektdaten" {
            ui.projekt_zeilen = vec![
                "Neubau Einfamilienhaus Mustermann, Am Lindenhof 12".into(),
                "Projekt-Nr. 2026/0147-B".into(),
            ];
        }
        ui.fonts = Fonts {
            regular: lade("LiberationSans-Regular.ttf"),
            bold: lade("LiberationSans-Bold.ttf"),
            italic: None,
        };
        let (c, _, _) = ui.paint(&t, Panel::Tools, 1280, 32);
        std::fs::write(dir.join(format!("ist-s4-kachel-{n}.png")), c.to_png()).unwrap();
    }
}

#[test]
fn knoepfe_werden_getroffen() {
    let mut ui = Ui::new(1.0, &Theme::dark());
    let r = ui.rect(Panel::Tools, 1280, 32);
    // Ganz oben „Projektdaten“, eigener Abschnitt (Paket PD-2)
    assert_eq!(ui.buttons(Panel::Tools)[0].0, Id::Projektdaten);
    // Rechts daneben die Kachel des Nordpfeils, quadratisch (Sonnenstand
    // S2), 64 dip (Jörn 10:06), oben bündig mit dem Knopf und neben
    // „noch leer“, ohne eigene Zeile (§8 07:30)
    let p = ui.buttons(Panel::Tools)[0].1;
    let (id, k, _) = ui.buttons(Panel::Tools)[1];
    assert_eq!((id, k.w, k.h), (Id::Nord, TILE, TILE));
    assert_eq!(TILE, 64.0);
    assert_eq!(k.y, p.y, "oben bündig");
    assert_eq!(k.x, p.x + p.w + TILE_GAP, "rechts neben dem Knopf");
    let zeile = p.y + p.h + 8.0;
    assert!(k.y + k.h > zeile + 17.0, "neben „noch leer“");
    let (id, b, _) = ui.buttons(Panel::Tools)[2];
    assert_eq!(id, Id::Building);
    // „Innenwand“ rechts neben „Gebäude“ (Paket O3: Knopfpaar)
    let (id, i, _) = ui.buttons(Panel::Tools)[3];
    assert_eq!((id, i.y), (Id::Interior, b.y));
    assert_eq!(k.x + k.w, i.x + i.w, "rechtsbündig");
    assert!(
        k.y + k.h + 3.0 + 1.0 + 10.0 + 22.0 + 12.0 <= b.y,
        "Trennstrich frei"
    );
    // Mit Bezeichnung und Projekt-Nr. bleibt sie, wo sie ist
    ui.projekt_zeilen = vec!["Haus Mustermann".into(), "Projekt-Nr. 01/26".into()];
    assert_eq!(ui.buttons(Panel::Tools)[1].1, k);
    ui.projekt_zeilen = vec![PROJEKT_LEER.into()];
    let hit = click(&mut ui, (r.x + k.x + 5.0) as f64, (r.y + k.y + 5.0) as f64);
    assert_eq!(hit, Some(Id::Nord));
    let (id, b, _) = ui.buttons(Panel::Tools)[2];
    assert_eq!(id, Id::Building);
    let hit = click(&mut ui, (r.x + b.x + 5.0) as f64, (r.y + b.y + 5.0) as f64);
    assert_eq!(hit, Some(Id::Building));
    // Darunter „Wandöffnung“ und „Türöffnung“
    let (id, o, _) = ui.buttons(Panel::Tools)[4];
    assert_eq!(id, Id::Opening(OpeningKind::Window));
    assert!(o.y > b.y + b.h);
    let hit = click(&mut ui, (r.x + o.x + 5.0) as f64, (r.y + o.y + 5.0) as f64);
    assert_eq!(hit, Some(Id::Opening(OpeningKind::Window)));

    let r = ui.rect(Panel::Views, 1280, 32);
    let grundriss = ui
        .buttons(Panel::Views)
        .into_iter()
        .find(|b| b.0 == Id::View(ViewKind::Plan))
        .unwrap()
        .1;
    let hit = click(
        &mut ui,
        (r.x + grundriss.x + 20.0) as f64,
        (r.y + grundriss.y + 10.0) as f64,
    );
    assert_eq!(hit, Some(Id::View(ViewKind::Plan)));
}

#[test]
fn neben_den_paneelen_gehoert_die_maus_der_3d_ansicht() {
    let mut ui = Ui::new(1.0, &Theme::dark());
    let out = ui.handle(
        &Event::MouseMove {
            x: 640.0,
            y: 400.0,
            mods: Modifiers::default(),
        },
        1280,
        32,
    );
    assert!(!out.consumed);
}

/// Jede Beschriftung eines Werkzeug- oder Ansichtsknopfs passt mit
/// echter Schrift samt Rand in ihren Knopf; sonst ragt sie in den
/// Nachbarn, und das Einzelbild weicht vom Paneel ab (Gate #86:
/// „Wandöffnung“ im halben Knopf). Liberation unter Linux, sonst die
/// Schrift des Systems (Windows: Segoe UI); ohne beide sagt der Test,
/// dass er nichts prüft.
#[test]
fn beschriftung_passt_in_den_knopf() {
    let lib = std::path::Path::new("/usr/share/fonts/truetype/liberation");
    let lade = |n: &str| {
        std::fs::read(lib.join(n))
            .ok()
            .and_then(sk_paint::font::Font::parse)
    };
    let schrift = || match lade("LiberationSans-Bold.ttf") {
        Some(bold) => Fonts {
            regular: lade("LiberationSans-Regular.ttf"),
            bold: Some(bold),
            italic: None,
        },
        None => Fonts::system(),
    };
    if schrift().bold.is_none() {
        eprintln!("beschriftung_passt_in_den_knopf: keine Schrift gefunden, nichts geprüft");
        return;
    }
    let th = Theme::dark();
    for scale in [0.6f32, 1.0, 1.25, 2.0] {
        let mut ui = Ui::new(scale, &th);
        ui.building = true;
        ui.fonts = schrift();
        let f = ui.fonts.bold.as_ref().expect("oben geprüft");
        let px = th.size.font * scale;
        for p in [Panel::Tools, Panel::Views] {
            for (id, b, label) in ui.buttons(p) {
                if matches!(id, Id::Ref(_) | Id::Sprung(_)) || label.is_empty() {
                    continue;
                }
                let w = f.width(label, px) + 6.0 * scale;
                assert!(w <= b.w, "{scale} {id:?} „{label}“: {w} > {}", b.w);
            }
        }
    }
}

/// Hover und Drücken zeichnen nur den Knopf neu; das Ergebnis gleicht dem
/// vollen Neuzeichnen aufs Pixel.
#[test]
fn knopf_einzeln_neu_gleicht_dem_ganzen_paneel() {
    let th = Theme::dark();
    for scale in [0.6f32, 0.875, 1.25, 2.0] {
        let mut ui = Ui::new(scale, &th);
        ui.wall_layers = vec![(Rgba::rgb(240, 190, 60), "14 cm Dämmung".into())];
        ui.building = true;
        for p in [Panel::Tools, Panel::Views] {
            let ids: Vec<Id> = ui.buttons(p).into_iter().map(|b| b.0).collect();
            for id in ids {
                for (hover, pressed) in [(Some(id), None), (Some(id), Some(id)), (None, None)] {
                    let mut img = ui.paint(&th, p, 1280, 32).0.to_premul_rgba8();
                    let w = ui.images[panel_index(p)].as_ref().unwrap().cur.width;
                    (ui.hover, ui.pressed) = (hover, pressed);
                    let patch = ui.repaint_button(&th, id).expect("Paneelbild vorhanden");
                    assert_eq!(patch.panel, p);
                    for row in 0..patch.h {
                        let dst = ((patch.y + row) * w + patch.x) * 4;
                        img[dst..dst + patch.w * 4]
                            .copy_from_slice(&patch.px[row * patch.w * 4..][..patch.w * 4]);
                    }
                    let full = ui.paint(&th, p, 1280, 32).0.to_premul_rgba8();
                    assert!(img == full, "{scale} {id:?} {hover:?} {pressed:?}");
                    (ui.hover, ui.pressed) = (None, None);
                }
            }
        }
    }
}

#[test]
fn hover_meldet_alten_und_neuen_knopf() {
    let mut ui = Ui::new(1.0, &Theme::dark());
    let m = Modifiers::default();
    let r = ui.rect(Panel::Views, 1280, 32);
    let bs = ui.buttons(Panel::Views);
    let at = |b: &(Id, Rect, &str)| ((r.x + b.1.x + 5.0) as f64, (r.y + b.1.y + 5.0) as f64);
    let (x, y) = at(&bs[0]);
    let out = ui.handle(&Event::MouseMove { x, y, mods: m }, 1280, 32);
    assert_eq!(out.changed, vec![bs[0].0]);
    let (x, y) = at(&bs[1]);
    let out = ui.handle(&Event::MouseMove { x, y, mods: m }, 1280, 32);
    assert_eq!(out.changed, vec![bs[0].0, bs[1].0]);
    let out = ui.handle(&Event::MouseMove { x, y, mods: m }, 1280, 32);
    assert!(out.changed.is_empty());
    let out = ui.handle(&Event::MouseLeave, 1280, 32);
    assert_eq!(out.changed, vec![bs[1].0]);
}

/// `cargo test --release -p skizzeo knopf_zeit -- --ignored --nocapture`
#[test]
#[ignore]
fn knopf_zeit() {
    let th = Theme::dark();
    for scale in [1.0f32, 1.5, 2.0] {
        let mut ui = Ui::new(scale, &th);
        let n = 50;
        let t = std::time::Instant::now();
        for _ in 0..n {
            std::hint::black_box(ui.paint(&th, Panel::Views, 1920, 32).0.to_premul_rgba8());
        }
        let full = t.elapsed().as_secs_f64() * 1000.0 / n as f64;
        let id = Id::View(ViewKind::Front);
        let t = std::time::Instant::now();
        for i in 0..n {
            ui.hover = (i % 2 == 0).then_some(id);
            std::hint::black_box(ui.repaint_button(&th, id).unwrap());
        }
        let part = t.elapsed().as_secs_f64() * 1000.0 / n as f64;
        println!("Skalierung {scale}: Paneel {full:.3} ms, ein Knopf {part:.3} ms");
    }
}

/// Ein neuer Stand des Farbschemas verwirft die Paneelbilder; das nächste
/// Zeichnen nimmt den neuen Akzent.
#[test]
fn neuer_akzent_zeichnet_die_paneele_neu() {
    let mut th = Theme::dark();
    let mut ui = Ui::new(1.0, &th);
    let p = Panel::Views;
    // „3D“ ist eingeschaltet und damit in Akzentfarbe
    let (id, b, _) = ui.buttons(p)[0];
    assert!(ui.is_on(id));
    let m = th.size.panel_shadow;
    let at = |c: &Canvas| {
        let px = c.to_rgba8();
        let i = (((b.y + m + b.h * 0.5) as usize) * c.width + (b.x + m + 6.0) as usize) * 4;
        Rgba(px[i], px[i + 1], px[i + 2], px[i + 3])
    };
    assert_eq!(at(ui.paint(&th, p, 1280, 32).0), th.ui.accent);
    let blue = Rgba::rgb(40, 120, 220);
    th.set_accent(blue);
    assert!(ui.repaint_button(&th, id).is_none(), "altes Bild verworfen");
    assert_eq!(at(ui.paint(&th, p, 1280, 32).0), blue);
    assert!(ui.repaint_button(&th, id).is_some());
}

fn props_mit_feldern() -> Props {
    Props {
        values: vec![("Nummer", "SP-001".into())],
        fields: vec![
            FieldRow {
                field: Field::SlabThickness,
                label: "Dicke",
                value: 200.0,
                min: 100.0,
                max: 1000.0,
                zero: false,
                einheit: None,
            },
            FieldRow {
                field: Field::Recess,
                label: "Sockelrücksprung",
                value: 0.0,
                min: 20.0,
                max: 500.0,
                zero: true,
                einheit: None,
            },
        ],
        ..Default::default()
    }
}

fn feld_mitte(ui: &Ui, f: Field) -> (f64, f64) {
    let r = ui.rect(Panel::Props, 1280, 32);
    let b = ui.field_rect(f).unwrap();
    (
        (r.x + b.x + b.w * 0.5) as f64,
        (r.y + b.y + b.h * 0.5) as f64,
    )
}

fn taste(ui: &mut Ui, k: Key) -> UiOut {
    ui.key(k, true, Modifiers::default())
        .expect("Feld in Eingabe")
}

/// Paket 4: Eigenschaften höher als ihr Platz rollen mit dem Rad; nur
/// ganz sichtbare Felder sind da, der Rollstand bleibt in den Grenzen.
#[test]
fn eigenschaften_rollen() {
    let mut ui = Ui::new(1.0, &Theme::dark());
    ui.set_props(Some(props_mit_feldern()));
    let full = ui.props_natural_height();
    let unten = ui.field_rect(Field::Recess).unwrap();
    // Ohne Grenze: nichts zu rollen
    assert!(!ui.scroll_props(-1.0));
    ui.props_room = (unten.y + 4.0).round();
    assert_eq!(ui.rect(Panel::Props, 1280, 32).h, ui.props_room);
    assert!(ui.field_rect(Field::Recess).is_none(), "abgeschnitten");
    assert!(ui.scroll_props(-1.0), "Rad nach unten rollt");
    let mut n = 0;
    while ui.scroll_props(-1.0) {
        n += 1;
        assert!(n < 50, "am Ende hält es an");
    }
    let b = ui.field_rect(Field::Recess).expect("hereingerollt");
    assert_eq!(b.y, unten.y - (full - ui.props_room));
    assert!(ui.scroll_props(5.0));
    assert_eq!(ui.field_rect(Field::Recess), None);
    assert!(ui.field_rect(Field::SlabThickness).is_some());
    ui.reset_props_scroll();
    assert!(!ui.scroll_props(1.0), "oben");
}

#[test]
fn zahlenfeld_tippen_pruefen_abbrechen() {
    let mut ui = Ui::new(1.0, &Theme::dark());
    ui.set_props(Some(props_mit_feldern()));
    assert!(ui.key(Key::Char('1'), true, Modifiers::default()).is_none());
    let (x, y) = feld_mitte(&ui, Field::SlabThickness);
    click(&mut ui, x, y);
    // Alles markiert: Tippen ersetzt den Wert
    assert_eq!(ui.edit.as_ref().unwrap().text, "20");
    taste(&mut ui, Key::Char('3'));
    taste(&mut ui, Key::Char('5'));
    assert_eq!(ui.edit.as_ref().unwrap().text, "35");
    taste(&mut ui, Key::Char('A'));
    assert_eq!(
        ui.edit.as_ref().unwrap().text,
        "35",
        "Buchstaben zählen nicht"
    );
    // Schreibmarke: Pos1, Entf, Ende, Rücktaste
    taste(&mut ui, Key::Home);
    taste(&mut ui, Key::Delete);
    taste(&mut ui, Key::End);
    taste(&mut ui, Key::Char(','));
    taste(&mut ui, Key::Char('5'));
    assert_eq!(ui.edit.as_ref().unwrap().text, "5,5");
    let out = taste(&mut ui, Key::Enter);
    assert_eq!(out.submit, None, "unter 10 cm");
    assert!(out.relayout);
    assert_eq!(
        ui.edit.as_ref().unwrap().error.as_deref(),
        Some("mindestens 10 cm")
    );
    let h = ui.panel_height(Panel::Props);
    taste(&mut ui, Key::Backspace);
    taste(&mut ui, Key::Backspace);
    taste(&mut ui, Key::Backspace);
    taste(&mut ui, Key::Char('2'));
    taste(&mut ui, Key::Char('2'));
    let out = taste(&mut ui, Key::Enter);
    assert_eq!(out.submit, Some((Field::SlabThickness, 220.0)));
    assert!(out.relayout && ui.edit.is_none());
    assert!(ui.panel_height(Panel::Props) < h, "Hinweis weg");
    // Rücksprung: 0 erlaubt, 1 cm nicht, Esc verwirft
    let (x, y) = feld_mitte(&ui, Field::Recess);
    click(&mut ui, x, y);
    assert_eq!(ui.edit.as_ref().unwrap().text, "0");
    taste(&mut ui, Key::Char('1'));
    taste(&mut ui, Key::Enter);
    assert_eq!(
        ui.edit.as_ref().unwrap().error.as_deref(),
        Some("0 oder mindestens 2 cm")
    );
    let out = taste(&mut ui, Key::Escape);
    assert!(ui.edit.is_none() && out.submit.is_none());
    // Klick neben das Feld übernimmt eine gültige Eingabe
    click(&mut ui, x, y);
    taste(&mut ui, Key::Char('4'));
    let out = ui.handle(
        &Event::MouseDown {
            button: MouseButton::Left,
            x: 600.0,
            y: 400.0,
            mods: Modifiers::default(),
        },
        1280,
        32,
    );
    assert_eq!(out.submit, Some((Field::Recess, 40.0)));
    assert!(!out.consumed, "der Klick gehört weiter der 3D-Ansicht");
    // Felder, die es nicht mehr gibt, beenden die Eingabe
    click(&mut ui, x, y);
    ui.set_props(Some(Props::default()));
    assert!(ui.edit.is_none());
}

/// Paneel einer Geländefläche (Bedienbarkeit 43.1 A und B): die Felder
/// lassen sich anklicken und tippen, Werte außerhalb melden ihren
/// Bereich; die Felder der Leiste bleiben in den Werkzeugen.
#[test]
fn felder_der_gelaendeflaeche() {
    use sk_model::{Model, TerrainPatch};
    let mut m = Model::with_seed(1);
    m.begin("Fläche");
    let ring = vec![[0.0, 0.0], [5000.0, 0.0], [5000.0, 4000.0], [0.0, 4000.0]];
    m.add_site_patch(TerrainPatch::flat(ring, -600.0));
    m.commit();
    let mut ui = Ui::new(1.0, &Theme::dark());
    ui.set_props(crate::gelaende_flaeche_app::props(&m, 0));
    let g = Field::Gelaende;
    assert_eq!(Ui::field_panel(g(GelaendeFeld::Faktor)), Panel::Props);
    assert_eq!(Ui::field_panel(g(GelaendeFeld::Versatz)), Panel::Tools);
    let mut tippe = |f: GelaendeFeld, t: &str| {
        let (x, y) = feld_mitte(&ui, g(f));
        click(&mut ui, x, y);
        t.chars().for_each(|c| drop(taste(&mut ui, Key::Char(c))));
        let out = taste(&mut ui, Key::Enter);
        let fehler = ui.edit.take().and_then(|e| e.error).map(|e| e.to_string());
        (out.submit, fehler)
    };
    let h = g(GelaendeFeld::Hoehe);
    assert_eq!(tippe(GelaendeFeld::Hoehe, "0,3"), (Some((h, 300.0)), None));
    let b = g(GelaendeFeld::Boeschung);
    let zu_steil = Some("0 oder zulässig 0,5 bis 20".to_string());
    assert_eq!(tippe(GelaendeFeld::Boeschung, "0,2"), (None, zu_steil));
    assert_eq!(tippe(GelaendeFeld::Boeschung, "0"), (Some((b, 0.0)), None));
    let f = g(GelaendeFeld::Faktor);
    let fehler = Some("zulässig 0,8 bis 1,2".to_string());
    assert_eq!(tippe(GelaendeFeld::Faktor, "1,5"), (None, fehler));
    assert_eq!(tippe(GelaendeFeld::Faktor, "0,9"), (Some((f, 0.9)), None));
    let a = g(GelaendeFeld::Andecken);
    assert_eq!(
        tippe(GelaendeFeld::Andecken, "15"),
        (Some((a, 150.0)), None)
    );
}

#[test]
fn zahl_in_zentimetern() {
    let f = &props_mit_feldern().fields[1];
    assert_eq!(cm_text(200.0), "20");
    assert_eq!(cm_text(25.0), "2,5");
    assert_eq!(f.parse("2,5"), Ok(25.0));
    assert_eq!(f.parse(" 3.0 "), Ok(30.0));
    assert_eq!(f.parse("0"), Ok(0.0));
    assert_eq!(f.parse("-2").unwrap_err(), "0 oder mindestens 2 cm");
    assert_eq!(f.parse("51").unwrap_err(), "höchstens 50 cm");
    assert_eq!(f.parse("").unwrap_err(), "Zahl fehlt");
    assert_eq!(f.parse("2,,5").unwrap_err(), "keine Zahl");
}

/// Ein Feld unter der Maus oder in Eingabe zeichnet nur sich neu, gleich
/// dem vollen Paneel.
#[test]
fn feld_einzeln_neu_gleicht_dem_ganzen_paneel() {
    let th = Theme::dark();
    for scale in [1.0f32, 1.5] {
        let mut ui = Ui::new(scale, &th);
        ui.set_props(Some(props_mit_feldern()));
        let id = Id::Field(Field::SlabThickness);
        for step in 0..3 {
            let mut img = ui.paint(&th, Panel::Props, 1280, 32).0.to_premul_rgba8();
            let w = ui.images[panel_index(Panel::Props)]
                .as_ref()
                .unwrap()
                .cur
                .width;
            match step {
                0 => ui.hover = Some(id),
                1 => ui.edit = Some(Edit::new(&props_mit_feldern().fields[0])),
                _ => {
                    ui.key(Key::Left, true, Modifiers::default());
                }
            }
            let patch = ui.repaint_button(&th, id).expect("Paneelbild vorhanden");
            for row in 0..patch.h {
                let dst = ((patch.y + row) * w + patch.x) * 4;
                img[dst..dst + patch.w * 4]
                    .copy_from_slice(&patch.px[row * patch.w * 4..][..patch.w * 4]);
            }
            let full = ui.paint(&th, Panel::Props, 1280, 32).0.to_premul_rgba8();
            assert!(img == full, "{scale} Schritt {step}");
        }
    }
}
