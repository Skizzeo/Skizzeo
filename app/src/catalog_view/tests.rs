use super::*;

#[test]
fn name_folgt_der_dicke() {
    assert_eq!(
        follow_name("AW mit WDVS 36", 360.0, 400.0).as_deref(),
        Some("AW mit WDVS 40")
    );
    assert_eq!(
        follow_name("AW monolithisch 36,5", 365.0, 425.0).as_deref(),
        Some("AW monolithisch 42,5")
    );
    assert_eq!(follow_name("AW 136", 36.0, 40.0), None);
    assert_eq!(follow_name("Wand", 360.0, 400.0), None);
    assert_eq!(
        follow_name("AW 31,5 Porenbeton + WDVS", 315.0, 335.0).as_deref(),
        Some("AW 33,5 Porenbeton + WDVS")
    );
    assert_eq!(parse_thick("16"), Ok(160.0));
    // Kurzzeichen einer Kopie zieht mit
    let mut c = Catalog::open(&szene(), None);
    let id = c.work.duplicate_type(c.sel.unwrap()).unwrap();
    c.show(id);
    assert_eq!(c.draft.code, "AW-31,5-2");
    c.apply_value(FieldId::Thick(0), "20").unwrap();
    assert_eq!(c.draft.code, "AW-37,5");
    assert_eq!(parse_thick("12,3"), Ok(125.0));
    assert!(parse_thick("0").is_err());
    assert!(parse_thick("101").is_err());
}

/// Deckenauflager bearbeiten (Skizze 3b, Regel 21): „fest“ nur ohne
/// Schicht vor dem Kern, Tiefe 10 cm ab Kern bis Dicke − 2 cm in
/// Schritten von 0,5 cm, Streifen nur aus Dämmung; „Standard“ rückt
/// unter die Felder.
#[test]
fn deckenauflager_bearbeiten() {
    let mut c = Catalog::open(&szene(), None);
    let t = Theme::dark();
    // AW-31,5 hat WDVS vor dem Kern
    assert!(c.fixed_blocked().is_some());
    c.set_bearing(true);
    assert!(!c.fixed());
    let core_y = c.standard_rect(&t, &WIN).y;
    assert!(core_y > c.bearing_box(&t, &WIN).y);
    // Kopie von AW-36,5 auf 42,5
    let mono = c.work.type_by_guid(sk_model::MONO_TYPE_GUID).unwrap();
    let id = c.work.duplicate_type(mono).unwrap();
    c.show(id);
    assert!(c.fixed());
    c.apply_value(FieldId::Thick(0), "42,5").unwrap();
    assert_eq!(c.draft.bearing_range(), Some((100.0, 405.0)));
    for bad in ["9,5", "41", "24,25", "x"] {
        assert!(c.apply_value(FieldId::Depth, bad).is_err(), "{bad}");
    }
    for ok in ["10", "40,5", "24"] {
        assert_eq!(c.apply_value(FieldId::Depth, ok), Ok(()), "{ok}");
    }
    assert_eq!(c.draft.strip_width(), Some(185.0));
    let saved = c.work.layer_set(id).unwrap();
    assert_eq!(saved.strip_width(), Some(185.0));
    // Nur Dämmstoffe als Streifen, „Randdämmung“ zuerst
    let mats = c.strip_materials();
    assert!(!mats.is_empty());
    assert!(mats
        .iter()
        .all(|m| c.work.material(*m).unwrap().category == MatCategory::Insulation));
    assert_eq!(c.work.material(mats[0]).unwrap().name, "Randdämmung");
    // Felder und „Standard“ darunter
    let depth = c.field_rect(&t, &WIN, FieldId::Depth).unwrap();
    let strip = c.combo_rect(&t, &WIN, ComboId::Strip);
    let st = c.standard_rect(&t, &WIN);
    assert!(depth.y + depth.h <= strip.y && strip.y + strip.h <= st.y);
    assert!(st.y > core_y);
    // Kern dünner als die Tiefe: Skizzeo ändert die Tiefe nicht, der
    // Typ bleibt ungültig und wird nicht übernommen
    c.apply_value(FieldId::Depth, "40,5").unwrap();
    c.apply_value(FieldId::Thick(0), "36,5").unwrap();
    assert!(c.draft.bearing_problem().is_some());
    assert!(!c.draft_problems().is_empty());
    assert_eq!(
        c.work.layer_set(id).unwrap().thickness(),
        425.0,
        "ungültig nicht übernommen"
    );
    // Zurück auf ganze tragende Schicht
    c.set_bearing(false);
    assert!(!c.fixed() && c.draft_problems().is_empty());
    assert_eq!(c.standard_rect(&t, &WIN).y, core_y);
    assert!(c.field_rect(&t, &WIN, FieldId::Depth).is_none());
    // Wieder „fest“: Vorgabe 24 cm
    c.set_bearing(true);
    assert!(matches!(
        c.draft.bearing,
        sk_model::Bearing::Depth { depth, .. } if depth == 240.0
    ));
}

use sk_math::vec3;
use sk_model::{RefSide, WallChain};

fn szene() -> Scene {
    let mut s = Scene::with_model(Model::with_seed(7));
    s.add_wall(&WallChain {
        base: 0.0,
        points: vec![
            vec3(0.0, 0.0, 0.0),
            vec3(0.0, 4000.0, 0.0),
            vec3(5000.0, 4000.0, 0.0),
            vec3(5000.0, 0.0, 0.0),
        ],
        closed: true,
        ref_side: RefSide::Left,
        layers: Vec::new(),
        height: 3500.0,
        joints: Default::default(),
    })
    .unwrap();
    s
}

fn fonts() -> Fonts {
    Fonts {
        regular: None,
        bold: None,
        italic: None,
    }
}

const WIN: Win = Win {
    w: 1280,
    h: 800,
    top: 32,
    scale: 1.0,
};

/// Außenwandtypen ohne die Werkstypen aus K4.
fn aussen(m: &Model) -> Vec<&LayerSet> {
    let k4 = [
        sk_model::ETICS_TYPE_GUID,
        sk_model::CAVITY_TYPE_GUID,
        sk_model::MONO_TYPE_GUID,
    ];
    m.layer_sets()
        .iter()
        .filter(|(_, t)| t.category == TypeCategory::ExteriorWall && !k4.contains(&t.guid))
        .map(|(_, t)| t)
        .collect()
}

/// U7: Nach einer Hervorhebung kommen nur Ausschnitte; zusammen mit dem
/// letzten ganzen Bild gleichen sie dem neu gemalten ganzen Bild. Ohne
/// bekannten Bereich (Klick, Größe) kommt das ganze Bild.
#[test]
fn teilbilder_gleichen_dem_ganzen_bild() {
    let mut s = szene();
    let theme = Theme::dark();
    let f = Fonts::system();
    for win in [WIN, Win { scale: 1.5, ..WIN }] {
        let mut c = Catalog::open(&s, None);
        assert!(matches!(
            c.paint_frame(&theme, &f, &win),
            Frame::Full { .. }
        ));
        let (tiles, ..) = c.list_layout(&theme, &win);
        let thick = c.field_rect(&theme, &win, FieldId::Thick(1)).unwrap();
        let ok = c.foot_buttons(&theme, &win)[2].1;
        let mid = |r: Rect| ((r.x + r.w * 0.5) as f64, (r.y + r.h * 0.5) as f64);
        let total = {
            let img = c.img.as_ref().unwrap();
            img.width * img.height
        };
        let mut prev = (0.0, 0.0);
        for (x, y) in [
            mid(tiles[1].1),
            mid(tiles[2].1),
            mid(thick),
            mid(ok),
            (1.0, 1.0),
        ] {
            let mut cx = Ctx {
                scene: &mut s,
                theme: &theme,
                fonts: &f,
                win,
                company: None,
                company_standard: false,
            };
            let e = Event::MouseMove {
                x,
                y,
                mods: Modifiers::default(),
            };
            assert!(c.handle(&e, &mut cx).repaint);
            // Streifen über die ganze Breite: Schichtzeile samt
            // Schnittbild ist fast das halbe Fenster und darf ganz malen
            match c.paint_frame(&theme, &f, &win) {
                Frame::Parts(parts) => {
                    let area: usize = parts.iter().map(|p| (p.2 * p.3) as usize).sum();
                    assert!(!parts.is_empty() && area * 2 < total, "{area} von {total}");
                }
                Frame::Full { .. } => assert!(
                    [(x, y), prev].contains(&mid(thick)),
                    "Teilbild erwartet bei {x}, {y}"
                ),
            }
            prev = (x, y);
            let a = c.img.as_ref().unwrap().to_premul_rgba8();
            let b = c.paint(&theme, &f, &win).0.to_premul_rgba8();
            let diff = a.iter().zip(&b).map(|(p, q)| p.abs_diff(*q)).max();
            assert_eq!(diff, Some(0), "Abweichung {diff:?} bei {x}, {y}");
        }
        // Übergang der Schichtgrenzen: nur das Schnittbild (sehr langsam,
        // damit Teil- und Vergleichsbild denselben Stand zeigen)
        let mut slow = theme.clone();
        slow.size.anim_ms = 1e9;
        c.anim = Some((vec![60.0, 300.0], Instant::now()));
        assert!(c.tick(&slow));
        let Frame::Parts(parts) = c.paint_frame(&slow, &f, &win) else {
            panic!("Teilbild erwartet beim Übergang");
        };
        assert_eq!(parts.len(), 1);
        let a = c.img.as_ref().unwrap().to_premul_rgba8();
        let b = c.paint(&slow, &f, &win).0.to_premul_rgba8();
        let diff = a.iter().zip(&b).map(|(p, q)| p.abs_diff(*q)).max();
        assert_eq!(diff, Some(0), "Übergang: Abweichung {diff:?}");
        c.anim = None;
        // Klick: ganzes Bild
        let mut cx = Ctx {
            scene: &mut s,
            theme: &theme,
            fonts: &f,
            win,
            company: None,
            company_standard: false,
        };
        let (x, y) = mid(tiles[2].1);
        let e = Event::MouseDown {
            button: MouseButton::Left,
            x,
            y,
            mods: Modifiers::default(),
        };
        c.handle(&e, &mut cx);
        assert!(matches!(
            c.paint_frame(&theme, &f, &win),
            Frame::Full { .. }
        ));
        // Die App verlangt ein Bild ohne Änderung im Fenster: ganz
        assert!(matches!(
            c.paint_frame(&theme, &f, &win),
            Frame::Full { .. }
        ));
    }
}

/// Markieren mit der Maus im Namensfeld: je Bewegung nur das Feld, und
/// nur wenn sich die Markierung ändert.
#[test]
fn markieren_malt_nur_das_feld() {
    let mut s = szene();
    let theme = Theme::dark();
    let f = Fonts::system();
    // Ohne Schrift hat der Text keine Breite, die Maus keine Wirkung
    if f.regular.is_none() {
        return;
    }
    for win in [WIN, Win { scale: 1.5, ..WIN }] {
        let mut c = Catalog::open(&s, None);
        c.paint_frame(&theme, &f, &win);
        let r = c.field_rect(&theme, &win, FieldId::Name).unwrap();
        let y = (r.y + r.h * 0.5) as f64;
        let at = |k: f32| (r.x + r.w * k) as f64;
        let mut cx = Ctx {
            scene: &mut s,
            theme: &theme,
            fonts: &f,
            win,
            company: None,
            company_standard: false,
        };
        let mods = Modifiers::default();
        let left = MouseButton::Left;
        for _ in 0..2 {
            c.handle(
                &Event::MouseDown {
                    button: left,
                    x: at(0.3),
                    y,
                    mods,
                },
                &mut cx,
            );
            c.handle(
                &Event::MouseUp {
                    button: left,
                    x: at(0.3),
                    y,
                    mods,
                },
                &mut cx,
            );
        }
        c.handle(
            &Event::MouseDown {
                button: left,
                x: at(0.3),
                y,
                mods,
            },
            &mut cx,
        );
        assert!(matches!(c.drag, Some(Drag::Select(FieldId::Name))));
        c.paint_frame(&theme, &f, &win);
        let e = Event::MouseMove {
            x: r.x as f64 + 2.0,
            y,
            mods,
        };
        assert!(c.handle(&e, &mut cx).repaint, "Markierung geändert");
        let Frame::Parts(parts) = c.paint_frame(&theme, &f, &win) else {
            panic!("Teilbild erwartet");
        };
        assert_eq!(parts.len(), 1);
        let a = c.img.as_ref().unwrap().to_premul_rgba8();
        let b = c.paint(&theme, &f, &win).0.to_premul_rgba8();
        let diff = a.iter().zip(&b).map(|(p, q)| p.abs_diff(*q)).max();
        assert_eq!(diff, Some(0), "Abweichung {diff:?}");
        // Dieselbe Stelle noch einmal: nichts zu malen
        assert!(!c.handle(&e, &mut cx).repaint);
    }
}

/// Zeitmessung Katalogfenster (U7), Median aus 7 Runden; mit Schrift
/// über WINDIR wie in perf.rs. `cargo test --release perf_katalog --
/// --ignored --nocapture`
#[test]
#[ignore]
fn perf_katalogfenster() {
    let median = |f: &mut dyn FnMut()| {
        let mut v: Vec<f64> = (0..7)
            .map(|_| {
                let t = Instant::now();
                for _ in 0..10 {
                    f();
                }
                t.elapsed().as_secs_f64() * 100.0
            })
            .collect();
        v.sort_by(f64::total_cmp);
        v[3]
    };
    let mut s = szene();
    let theme = Theme::dark();
    let f = Fonts::system();
    println!();
    println!(
        "{:<6} {:>10} {:>12} {:>12} {:>12}",
        "Skala", "ganz", "Hover Kachel", "Hover Zeile", "Übergang"
    );
    for scale in [1.0, 1.5] {
        let win = Win {
            w: (1280.0 * scale) as u32,
            h: (800.0 * scale) as u32,
            top: (32.0 * scale) as u32,
            scale,
        };
        let mut c = Catalog::open(&s, None);
        // Wie die App: Bytes eines ganzen Bildes zurückgeben
        let full = median(&mut || {
            c.damage.clear();
            if let Frame::Full { px, .. } = c.paint_frame(&theme, &f, &win) {
                c.give_back(std::hint::black_box(px));
            }
        });
        let (tiles, ..) = c.list_layout(&theme, &win);
        let thick = c.field_rect(&theme, &win, FieldId::Thick(1)).unwrap();
        let mid = |r: Rect| ((r.x + r.w * 0.5) as f64, (r.y + r.h * 0.5) as f64);
        let mut hover = |pts: [(f64, f64); 2], c: &mut Catalog| {
            let mut k = 0;
            median(&mut || {
                k += 1;
                let (x, y) = pts[k % 2];
                let mut cx = Ctx {
                    scene: &mut s,
                    theme: &theme,
                    fonts: &f,
                    win,
                    company: None,
                    company_standard: false,
                };
                let e = Event::MouseMove {
                    x,
                    y,
                    mods: Modifiers::default(),
                };
                c.handle(&e, &mut cx);
                std::hint::black_box(c.paint_frame(&theme, &f, &win));
            })
        };
        let tile = hover([mid(tiles[1].1), mid(tiles[2].1)], &mut c);
        let row = hover([mid(thick), mid(tiles[2].1)], &mut c);
        c.anim = Some((vec![120.0, 195.0], Instant::now()));
        let anim = median(&mut || {
            c.anim = Some((vec![120.0, 195.0], Instant::now()));
            c.tick(&theme);
            std::hint::black_box(c.paint_frame(&theme, &f, &win));
        });
        println!(
            "{:<6} {:>8.2}ms {:>10.2}ms {:>10.2}ms {:>10.2}ms",
            scale, full, tile, row, anim
        );
    }
    // Kachelflug (K3b): Bilder im Flug nach „Ins Projekt übernehmen“
    let d = std::env::temp_dir().join(format!("skizzeo-perf-kachelflug-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    let (company, _) = Company::load(&d.join("firmenkatalog.szk"), true);
    for scale in [1.0, 1.5] {
        let win = Win {
            w: (1280.0 * scale) as u32,
            h: (800.0 * scale) as u32,
            top: (32.0 * scale) as u32,
            scale,
        };
        let mut c = Catalog::open(&s, Some(&company));
        c.tab = Tab::Company;
        let g = c.company_lib().unwrap().types.iter().last().unwrap().1.guid;
        c.csel = Some(g);
        let mut v = Vec::new();
        for _ in 0..5 {
            c.paint_frame(&theme, &f, &win);
            c.transfer(Item::Company(g), Tab::Project, Some(Mark::CompanyNewer));
            while c.tick(&theme) {
                let t = Instant::now();
                std::hint::black_box(c.paint_frame(&theme, &f, &win));
                v.push(t.elapsed().as_secs_f64() * 1000.0);
            }
        }
        v.sort_by(f64::total_cmp);
        println!(
            "Katalog: Kachelflug bei {scale}: je Bild {:.2} ms ({} Bilder, höchstens {:.2} ms)",
            v[v.len() / 2],
            v.len(),
            v[v.len() - 1]
        );
    }
    let _ = std::fs::remove_dir_all(&d);
}

/// Ein Ausbautyp, den nur die Abweichung eines Raums nennt: weder löschen
/// noch die Art umstellen, der Hinweis sagt warum (Review #190 B2).
#[test]
fn typ_einer_abweichung_gesperrt() {
    use sk_model::model::{AusbauAbweichung, AusbauStelle, AusbauWahl, WET_FLOOR_TYPE_GUID};
    let mut s = szene();
    assert!(s.edit_model("Raum", |m| {
        let st = m.storeys().iter().next().unwrap().0;
        let r = m.add_space(
            st,
            sk_model::Space::neu(vec3(2500.0, 2000.0, 0.0), "Bad".into()),
        );
        let a = AusbauAbweichung {
            stelle: AusbauStelle::Boden,
            wahl: Some(AusbauWahl::Typ(WET_FLOOR_TYPE_GUID)),
            hoehe: None,
        };
        m.set_space_finish(r, a).is_ok()
    }));
    let mut c = Catalog::open(&s, None);
    c.show_guid(WET_FLOOR_TYPE_GUID);
    assert!(c.sel.is_some());
    assert!(!c.can_delete() && c.category_locked());
    c.hover = Some(Target::Btn(Btn::Delete));
    let tip = c.tip().unwrap();
    assert!(tip.starts_with("In Raumvorlage oder Raum gewählt"), "{tip}");
}

/// Dicke eines verbauten Typs ändern: Rückfrage mit den Wänden, „Ändern“
/// wirkt auf alle, ein Rückgängig-Schritt; Name und Kurzzeichen folgen.
#[test]
fn aendern_fragt_und_ist_ein_schritt() {
    let mut s = szene();
    let theme = Theme::dark();
    let f = fonts();
    let mut c = Catalog::open(&s, None);
    assert_eq!(c.apply_value(FieldId::Thick(0), "16"), Ok(()));
    assert_eq!(c.draft.code, "AW-33,5");
    assert_eq!(c.draft.name, "AW 33,5 Porenbeton + WDVS");
    let mut cx = Ctx {
        scene: &mut s,
        theme: &theme,
        fonts: &f,
        win: WIN,
        company: None,
        company_standard: false,
    };
    let mut out = Out::default();
    c.ok(&mut cx, &mut out);
    assert!(c.asking());
    assert!(
        !out.highlight.as_ref().unwrap().is_empty(),
        "Wände markiert"
    );
    assert!(!out.applied);
    let mut out = Out::default();
    c.answer(false, &mut cx, &mut out);
    assert!(out.applied && out.closed);
    assert_eq!(out.highlight, Some(Vec::new()));
    let t = aussen(s.model());
    assert_eq!(t.len(), 1);
    assert_eq!(t[0].thickness(), 335.0);
    assert_eq!(t[0].code, "AW-33,5");
    assert_eq!(s.undo_label(), Some("Bauteilkatalog geändert"));
    s.undo();
    assert_eq!(aussen(s.model())[0].thickness(), 315.0);
}

/// „Als neuen Typ speichern“: die Wände behalten den alten Typ, der neue
/// bekommt Name und Kurzzeichen der neuen Dicke.
#[test]
fn als_neuen_typ_speichern() {
    let mut s = szene();
    let theme = Theme::dark();
    let f = fonts();
    let old = s.model().default_type(TypeCategory::ExteriorWall);
    let users = s.model().type_users(old).len();
    let mut c = Catalog::open(&s, None);
    c.apply_value(FieldId::Thick(0), "16").unwrap();
    let mut cx = Ctx {
        scene: &mut s,
        theme: &theme,
        fonts: &f,
        win: WIN,
        company: None,
        company_standard: false,
    };
    let mut out = Out::default();
    c.ok(&mut cx, &mut out);
    c.answer(true, &mut cx, &mut out);
    assert!(out.applied);
    let m = s.model();
    let t = aussen(m);
    assert_eq!(t.len(), 2);
    let alt = m.layer_set(old).unwrap();
    assert_eq!((alt.thickness(), alt.code.as_str()), (315.0, "AW-31,5"));
    assert_eq!(m.type_users(old).len(), users, "Wände bleiben");
    let neu = t.iter().find(|x| x.guid != alt.guid).unwrap();
    assert_eq!((neu.thickness(), neu.code.as_str()), (335.0, "AW-33,5"));
    assert_eq!(neu.name, "AW 33,5 Porenbeton + WDVS");
}

/// Was eine Arbeitskopie vergibt, vergibt die nächste nicht noch einmal
/// (zweimal öffnen, je duplizieren).
#[test]
fn guids_der_arbeitskopie_sind_neu() {
    let mut s = szene();
    let theme = Theme::dark();
    let f = fonts();
    for n in 2..4 {
        let mut c = Catalog::open(&s, None);
        let id = c.work.duplicate_type(c.sel.unwrap()).expect("Kopie");
        c.show(id);
        let mut cx = Ctx {
            scene: &mut s,
            theme: &theme,
            fonts: &f,
            win: WIN,
            company: None,
            company_standard: false,
        };
        let mut out = Out::default();
        c.ok(&mut cx, &mut out);
        assert_eq!(aussen(s.model()).len(), n);
    }
}

/// „Neu“ aus dem Firmenkatalog: ein eigener Typ mit freiem Kurzzeichen,
/// sofort weiter bearbeitbar; OK legt ihn ohne Rückfrage an.
#[test]
fn neu_aus_firmenkatalog() {
    let d = std::env::temp_dir().join(format!("skizzeo-katalog-neu-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    let (mut company, hints) = Company::load(&d.join("firmenkatalog.szk"), true);
    assert!(hints.is_empty(), "{hints:?}");
    let mut s = szene();
    let theme = Theme::dark();
    let f = fonts();
    let mut c = Catalog::open(&s, Some(&company));
    let g = sk_model::EXTERIOR_TYPE_GUID;
    // Der Projekttyp mit derselben Guid weicht ab: es gilt der Stand der Firma
    c.apply_value(FieldId::Thick(0), "16").unwrap();
    c.create_type(TypeCategory::ExteriorWall, Some(g));
    assert!(c.draft_problems().is_empty(), "{:?}", c.draft_problems());
    assert_eq!(c.draft.thickness(), 315.0);
    assert_ne!(c.draft.guid, g);
    assert_eq!(c.draft.code, "AW-31,5");
    assert!(!c.blocked());
    let mut cx = Ctx {
        scene: &mut s,
        theme: &theme,
        fonts: &f,
        win: WIN,
        company: Some(&mut company),
        company_standard: true,
    };
    let mut out = Out::default();
    c.ok(&mut cx, &mut out);
    // Der geänderte verbaute Typ fragt nach
    assert!(c.asking());
    c.answer(false, &mut cx, &mut out);
    assert!(out.applied);
    assert_eq!(aussen(s.model()).len(), 2);
    let _ = std::fs::remove_dir_all(&d);
}

/// Kachelflug (K3b §2): „Ins Projekt übernehmen“ bleibt im Firmenreiter,
/// die Marke heißt danach „wie im Projekt“, eine Kopie der Kachel fliegt
/// zum Reiter „Projekt“ (Teilbilder gleich dem ganzen Bild). Ohne
/// Animationen wird keine Kopie gezeichnet.
#[test]
fn kachelflug_zum_reiter() {
    let d = std::env::temp_dir().join(format!("skizzeo-kachelflug-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    let (company, _) = Company::load(&d.join("firmenkatalog.szk"), true);
    let s = szene();
    let f = Fonts::system();
    let mut slow = Theme::dark();
    slow.size.anim_ms = 1e6;
    slow.size.flash_ms = 1e6;
    for win in [WIN, Win { scale: 1.5, ..WIN }] {
        let mut c = Catalog::open(&s, Some(&company));
        c.tab = Tab::Company;
        let lib = c.company_lib().unwrap().clone();
        let g = lib
            .types
            .iter()
            .map(|(_, t)| t.guid)
            .find(|g| c.mark_of(Item::Company(*g)) != Some(Mark::Same))
            .unwrap_or(lib.types.iter().next().unwrap().1.guid);
        c.csel = Some(g);
        c.paint_frame(&slow, &f, &win);
        c.import_selected();
        // Der Klick malt das ganze Bild (Marke, Vergleich)
        c.paint_frame(&slow, &f, &win);
        assert_eq!(c.tab, Tab::Company);
        assert_eq!(c.mark_of(Item::Company(g)), Some(Mark::Same));
        assert!(c.fly.is_some() && c.pulse.is_some() && c.fade.is_some());
        // Mitten im Flug (Uhr zurückgestellt), dann in Teilbildern weiter
        let Some(at) = Instant::now().checked_sub(Duration::from_secs(400)) else {
            return;
        };
        c.fly = Some((Item::Company(g), Tab::Project, at));
        // Die Marke blendet in festen 150 ms über: schon vorbei
        c.fade = c.fade.map(|(it, old, _)| (it, old, at));
        // Feste Uhr: Teilbilder und ganzes Bild zeigen denselben Stand
        c.test_clock = Some(at + Duration::from_secs_f32(slow.size.anim_ms / 2000.0));
        for _ in 0..2 {
            assert!(c.tick(&slow));
            let Frame::Parts(_) = c.paint_frame(&slow, &f, &win) else {
                panic!("Teilbild erwartet im Flug");
            };
        }
        let start = c.list_layout(&slow, &win).0;
        let from = start.iter().find(|x| x.0 == Item::Company(g)).unwrap().1;
        let r = c.fly_drawn.expect("Kopie gezeichnet");
        assert!(r.w < from.w * 0.95 && r.y < from.y, "{r:?} von {from:?}");
        // Teilbilder und ganzes Bild stimmen überein
        let a = c.img.as_ref().unwrap().to_premul_rgba8();
        let b = c.paint(&slow, &f, &win).0.to_premul_rgba8();
        let fr = c.frame(&slow, &win);
        let m = (slow.size.panel_shadow * win.scale).round();
        let iw = c.img.as_ref().unwrap().width;
        for (i, (p, q)) in a.iter().zip(&b).enumerate() {
            assert!(
                p == q,
                "Flug: Abweichung {} bei {}, {} (Kopie {r:?})",
                p.abs_diff(*q),
                (i / 4 % iw) as f32 + fr.x - m,
                (i / 4 / iw) as f32 + fr.y - m
            );
        }
        // Flug vorbei: das letzte Bild löscht die Kopie überall
        c.test_clock = None;
        let past = Instant::now() - Duration::from_millis(700);
        c.fly = Some((Item::Company(g), Tab::Project, past));
        c.pulse = Some((Tab::Project, past));
        let normal = Theme::dark();
        assert!(c.tick(&normal), "ein letztes Bild");
        c.paint_frame(&normal, &f, &win);
        assert!(c.fly_drawn.is_none());
        let a = c.img.as_ref().unwrap().to_premul_rgba8();
        let b = c.paint(&normal, &f, &win).0.to_premul_rgba8();
        let diff = a.iter().zip(&b).map(|(p, q)| p.abs_diff(*q)).max();
        assert_eq!(diff, Some(0), "Rest der Kopie: {diff:?}");
        // Ohne Animationen: keine Kopie, der Übergang ist sofort vorbei
        let mut off = Theme::dark();
        off.size.anim_ms = 0.0;
        c.transfer(Item::Company(g), Tab::Project, None);
        assert!(c.tick(&off), "ein letztes Bild");
        c.paint_frame(&off, &f, &win);
        assert!(c.fly.is_none() && c.fly_drawn.is_none());
        assert!(!c.tick(&off));
    }
    let _ = std::fs::remove_dir_all(&d);
}
