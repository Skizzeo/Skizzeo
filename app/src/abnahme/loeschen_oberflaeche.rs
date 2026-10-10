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
            "Mengen · Kosten · AVA",
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
    assert_eq!(a[4], ("Löschen".into(), false, Some(Action::Delete)));
    assert_eq!(
        a[6],
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

/// A342 Schnellzugriff (Jörn 10.10.): Rechtsklick auf ein Bauteil im
/// Hauptfenster bietet „Mengen · Kosten · AVA“ (Name wie der Knopf unter
/// „Ansichten“), auch an der Sohlplatte und am Fundament; er öffnet das
/// Mengenfenster. Das Menü an einer Zeile des Mengenfensters hat ihn
/// nicht, das Fenster ist dort schon offen.
#[test]
fn a342_kontextmenue_oeffnet_mengen_kosten_ava() {
    let mut s = Scene::with_model(Model::with_seed(112));
    haus_b11(&mut s);
    let (t, m) = (Theme::dark(), s.model());
    for c in [
        Category::ExteriorWall,
        Category::InteriorWall,
        Category::GroundSlab,
        Category::StripFooting,
    ] {
        let Some(id) = m
            .elements()
            .iter()
            .find(|(_, e)| e.category == c)
            .map(|(id, _)| id)
        else {
            continue;
        };
        let menu = ContextMenu::new(m, id, &[id], 300.0, 300.0, (1280, 800, 32), &t, 1.0);
        let a = menu.actions();
        assert!(
            a.contains(&(crate::cards::KNOPF.into(), true, Some(Action::Quantity))),
            "{c:?}: {a:?}"
        );
        // Gleich nach „Eigenschaften“
        let i = a.iter().position(|x| x.0 == "Eigenschaften").unwrap();
        assert_eq!(a[i + 1].2, Some(Action::Quantity), "{c:?}");
        let liste = ContextMenu::for_list(m, id, &[id], 300.0, 300.0, (1280, 800, 32), &t, 1.0);
        assert!(liste
            .actions()
            .iter()
            .all(|x| x.2 != Some(Action::Quantity)));
    }
}

/// A307 „Wandtyp ändern …“ (Sichtprüfung 08.10.): Die Typ-Liste hängt am Chip
/// der Eigenschaften. Der steht unter Mengen und Feldern und ist bei
/// üblicher Fenstergröße hinausgerollt, dann ging nichts auf. Jetzt rollt
/// er herein, die Liste zeigt alle Außenwandtypen, die Wahl stellt die
/// Wand in einem Schritt um.
#[test]
fn a307_wandtyp_aendern_rollt_den_chip_herein() {
    use crate::type_menu::TypeMenu;
    let mut s = Scene::with_model(Model::with_seed(189));
    haus_b11(&mut s);
    let aw = erstes(&s, Category::ExteriorWall);
    let t = Theme::dark();
    let mut ui = Ui::new(1.0, &t);
    ui.set_props(selection::props(&s, aw));
    // Platz unter Ansichten und Baum bei 1280 × 800
    ui.props_room = 225.0;
    assert_eq!(
        ui.button_rect(Id::PropsType, 1280, 32),
        None,
        "ohne Rollen nicht zu sehen"
    );
    assert!(ui.reveal_props(Id::PropsType));
    let chip = ui
        .button_rect(Id::PropsType, 1280, 32)
        .expect("hereingerollt");
    let p = ui.rect(Panel::Props, 1280, 32);
    assert!(
        chip.y >= p.y && chip.y + chip.h <= p.y + p.h,
        "{chip:?} in {p:?}"
    );
    assert!(!ui.reveal_props(Id::PropsType), "schon zu sehen");

    let m = s.model();
    let e = m.element(aw).unwrap();
    let alt = e.layer_set;
    let sk_model::ElementKind::Wall(w) = e.kind else {
        panic!("Wand")
    };
    let menu = TypeMenu::new(
        Id::PropsType,
        m,
        &t,
        sk_model::TypeCategory::ExteriorWall,
        alt,
        vec![w.run],
        chip,
        p,
        1.0,
        (1280.0, 800.0),
    );
    let alle = m
        .layer_sets()
        .iter()
        .filter(|(_, x)| x.category == sk_model::TypeCategory::ExteriorWall)
        .count();
    assert_eq!(menu.items.len(), alle, "alle Außenwandtypen");
    let neu = menu
        .items
        .iter()
        .find(|i| i.name == "AW mehrschalig 49")
        .map(|i| i.id)
        .expect("mehrschalige Wand in der Liste");
    assert_ne!(Some(neu), alt);
    assert!(s.preview_run_type(&[w.run], neu));
    s.end_run_type(true);
    assert_eq!(s.model().element(aw).unwrap().layer_set, Some(neu));
    assert!(s.undo(), "ein Schritt");
    assert_eq!(s.model().element(aw).unwrap().layer_set, alt);
}

/// A308 (zu A307): kleinere Fenster und 150 %. Bei 1280 × 800 und
/// 1024 × 640 (Platz 225 bzw. 120 dip) rollt der Chip ganz ins Paneel
/// und die Liste liegt ganz im Fenster. Ist das
/// Paneel niedriger als der Chip, bleibt er verborgen; die Liste hängt
/// dann oben am Paneel (Rückfall-Anker wie in `open_type_menu`) und liegt
/// ebenfalls ganz im Fenster, mit allen Außenwandtypen.
#[test]
fn a308_wandtyp_kleine_fenster_und_rueckfall() {
    use crate::type_menu::TypeMenu;
    let mut s = Scene::with_model(Model::with_seed(308));
    haus_b11(&mut s);
    let aw = erstes(&s, Category::ExteriorWall);
    let t = Theme::dark();
    let m = s.model();
    let e = m.element(aw).unwrap();
    let sk_model::ElementKind::Wall(w) = e.kind else {
        panic!("Wand")
    };
    let alle = m
        .layer_sets()
        .iter()
        .filter(|(_, x)| x.category == sk_model::TypeCategory::ExteriorWall)
        .count();
    for scale in [1.0f32, 1.5] {
        for (ww, wh, room, sichtbar) in [
            (1280u32, 800u32, 225.0f32, Some(true)),
            (1024, 640, 120.0, Some(true)),
            (1024, 640, 60.0, None),
            (1024, 640, 12.0, Some(false)),
        ] {
            let (ww, wh) = ((ww as f32 * scale) as u32, (wh as f32 * scale) as u32);
            let top = (32.0 * scale) as u32;
            let fall = format!("{ww}×{wh} bei {scale}, Platz {room}");
            let mut ui = Ui::new(scale, &t);
            ui.set_props(selection::props(&s, aw));
            ui.props_room = room * scale;
            ui.reveal_props(Id::PropsType);
            let p = ui.rect(Panel::Props, ww, top);
            let anker = match ui.button_rect(Id::PropsType, ww, top) {
                Some(chip) => {
                    assert_ne!(
                        sichtbar,
                        Some(false),
                        "{fall}: Chip passt nicht und ist doch da"
                    );
                    assert!(
                        chip.y >= p.y && chip.y + chip.h <= p.y + p.h,
                        "{fall}: {chip:?} ganz in {p:?}"
                    );
                    chip
                }
                None => {
                    assert_ne!(sichtbar, Some(true), "{fall}: Chip hereingerollt");
                    sk_ui::widgets::Rect::new(p.x, p.y, p.w, 0.0)
                }
            };
            let menu = TypeMenu::new(
                Id::PropsType,
                m,
                &t,
                sk_model::TypeCategory::ExteriorWall,
                e.layer_set,
                vec![w.run],
                anker,
                p,
                scale,
                (ww as f32, wh as f32),
            );
            assert_eq!(menu.items.len(), alle, "{fall}: alle Außenwandtypen");
            let r = menu.rect();
            assert!(
                r.x >= 0.0 && r.y >= 0.0 && r.x + r.w <= ww as f32 + 0.5,
                "{fall}: Liste {r:?} im Fenster"
            );
            assert!(
                r.y + r.h <= wh as f32 + 0.5 || r.y == 0.0,
                "{fall}: Liste {r:?} unten im Fenster"
            );
        }
    }
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
