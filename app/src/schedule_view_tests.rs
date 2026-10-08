//! Tests des Mengenblatts (ausgelagert, damit `schedule_view.rs` unter
//! 3.000 Zeilen bleibt, R4).

use super::*;

#[test]
fn zahlen_deutsch() {
    assert_eq!(de(1234.5678, 3), "1.234,568");
    assert_eq!(de(-1.3159, 3), "−1,316");
    assert_eq!(de(0.0, 2), "0,00");
    assert_eq!(de(-0.0001, 2), "0,00");
    assert_eq!(cm(220.0), "22 cm");
    assert_eq!(cm(175.0), "17,5 cm");
    assert_eq!(csv_num(17.6), "17,6000");
    assert_eq!(csv_num(-1.31593), "-1,3159");
}

#[test]
fn gliederung_wird_gemerkt() {
    use crate::windows::{read_grouping, write_settings_grouped, Windows};
    let w = Windows::new(520.0);
    // nach Geschoss: nichts zu merken, wie bisher
    assert_eq!(write_settings_grouped(&w, Grouping::Storey), "");
    let text = write_settings_grouped(&w, Grouping::Trade);
    assert!(text.contains("gliederung=gewerk"), "{text}");
    assert_eq!(read_grouping(&text), Grouping::Trade);
    assert_eq!(read_grouping(""), Grouping::Storey);
    // KA-2a: das zuletzt gezeigte Blatt, nur wenn es nicht Mengen ist
    use crate::cards::Blatt;
    use crate::windows::{read_blatt, write_settings_blatt};
    let k = write_settings_blatt(&w, Grouping::Storey, Blatt::Kosten);
    assert!(k.contains(" blatt=kosten"), "{k}");
    assert_eq!(read_blatt(&k), Blatt::Kosten);
    assert_eq!(read_blatt(&text), Blatt::Mengen);
    assert!(!write_settings_blatt(&w, Grouping::Trade, Blatt::Mengen).contains("blatt="));
    assert_eq!(
        read_grouping("[mengenfenster] gliederung=quer\n"),
        Grouping::Storey
    );
}

/// Eine Wand in zwei Gewerken (§12): Porenbeton bei Mauerarbeiten, WDVS
/// beim Fassadensystem, je als „Schicht · Bauteilgruppe“; die Summen
/// gleichen denen nach Geschoss.
#[test]
fn wand_in_zwei_gewerken() {
    use sk_math::vec3;
    use sk_model::{RefSide, WallChain};
    let mut s = Scene::with_model(Model::with_seed(12));
    let chain = WallChain {
        base: 0.0,
        points: vec![
            vec3(0.0, 0.0, 0.0),
            vec3(0.0, 8000.0, 0.0),
            vec3(10000.0, 8000.0, 0.0),
            vec3(10000.0, 0.0, 0.0),
        ],
        closed: true,
        ref_side: RefSide::Left,
        layers: Vec::new(),
        height: 2750.0,
        joints: Default::default(),
    };
    s.add_wall(&chain).unwrap();
    let v = ListView::grouped(&mut s, Grouping::Trade);
    let t = v.line_texts();
    let at = |p: &str| t.iter().position(|l| l.trim_start().starts_with(p));
    let (Some(maurer), Some(gasbeton), Some(wdvs), Some(daemm)) = (
        at("Mauerarbeiten [DIN 18330]"),
        at("Porenbeton 17,5 · Außenwände | AW-001 … 008"),
        at("Wärmedämm-Verbundsysteme [DIN 18345]"),
        at("Dämmung (WDVS) 14 · Außenwände | AW-001 … 008"),
    ) else {
        panic!("{t:#?}");
    };
    assert!(
        maurer < gasbeton && gasbeton < wdvs && wdvs < daemm,
        "{t:#?}"
    );
    // Unter der Schichtgruppe die Wände mit ihrem Geschoss
    assert!(
        t[gasbeton + 1].trim_start().starts_with("AW-001 · EG"),
        "{t:#?}"
    );
    // Summen nach Baustoff wie nach Geschoss
    let g = ListView::grouped(&mut s, Grouping::Storey);
    assert_eq!(t.last(), g.line_texts().last());
    let mut v = v;
    assert!(v.set_grouping(Grouping::Storey));
    assert!(!v.set_grouping(Grouping::Storey));
    v.sync(&mut s, false);
    assert_eq!(v.line_texts(), g.line_texts());
}

#[test]
fn atv_nummer_hinter_gewerk() {
    assert_eq!(trade_tag("18331"), "DIN 18331");
    assert_eq!(trade_tag("F1"), "F1");
}

/// Standardhaus RH-1 (Fundament, EG, OG) als Szene.
fn standardhaus() -> Scene {
    let m = sk_model::szo::read_with(
        include_str!("../../crates/sk-cost/referenz/rh1-standardhaus.szo"),
        sk_model::GuidGen::with_seed(1),
        &sk_cost::lesen::ABSCHNITTE_SZO,
    )
    .expect("lädt")
    .model;
    Scene::with_model(m)
}

/// Mitte eines Rechtecks (px).
fn mitte(r: (f32, f32, f32, f32)) -> (f64, f64) {
    ((r.0 + r.2 * 0.5) as f64, (r.1 + r.3 * 0.5) as f64)
}

/// KA-1 Abnahme 2, 3, 8 und 11a über die Maus: Klick auf den EG-Chip
/// zeigt nur das EG, ohne die Liste neu zu rechnen; „Alle“ holt alle
/// zurück; der letzte Chip wackelt; der Umfang bleibt beim
/// Gliederungswechsel.
#[test]
fn chips_mit_der_maus() {
    let (t, fonts) = (Theme::dark(), Fonts::system());
    let mut p = Picking::default();
    let mut s = standardhaus();
    let mut v = ListView::new(&mut s);
    v.w = 520;
    v.h = 600;
    let alle_zeilen = v.line_texts();
    let runs = s.schedule_runs();
    assert!(
        v.subtitle.contains(" · alle Geschosse · Stand "),
        "{}",
        v.subtitle
    );
    let l = v.leiste.layout(&fonts, v.leiste_lage(&t));
    assert!(l.field.is_none() && l.all.is_none());
    assert_eq!(l.chips.len(), 3);
    let keine = sk_platform::Modifiers::default();
    let out = v.mouse_down(&t, &fonts, &mut p, mitte(l.chips[1]), keine);
    assert!(matches!(out, Some(ListOut::Repaint)));
    v.sync(&mut s, false);
    assert_eq!(s.schedule_runs(), runs, "Chip-Klick rechnet nicht neu");
    assert!(v.subtitle.contains(" · EG · Stand "), "{}", v.subtitle);
    let nur_eg = v.line_texts();
    assert!(nur_eg.len() < alle_zeilen.len());
    // Der letzte gewählte Chip lässt sich nicht abwählen, er wackelt
    let l = v.leiste.layout(&fonts, v.leiste_lage(&t));
    let alle = l.all.expect("„Alle“ erscheint");
    let _ = v.mouse_down(&t, &fonts, &mut p, mitte(l.chips[1]), keine);
    assert_eq!(v.leiste.wobbling(), Some(1));
    v.sync(&mut s, false);
    assert_eq!(v.line_texts(), nur_eg);
    // Gliederungswechsel behält den Umfang
    assert!(v.set_grouping(Grouping::Trade));
    v.sync(&mut s, false);
    assert!(v.subtitle.contains(" · EG · Stand "));
    assert!(v.set_grouping(Grouping::Storey));
    v.sync(&mut s, false);
    assert_eq!(v.line_texts(), nur_eg);
    // „Alle“ holt alle zurück und verschwindet
    let _ = v.mouse_down(&t, &fonts, &mut p, mitte(alle), keine);
    v.sync(&mut s, false);
    assert_eq!(v.line_texts(), alle_zeilen);
    assert!(v.leiste.layout(&fonts, v.leiste_lage(&t)).all.is_none());
    assert_eq!(s.schedule_runs(), runs);
}

/// KA-1 Abnahme 7: Die CSV beginnt nach dem BOM mit der Kopfzeile; mit
/// Umfang nur EG stehen nur EG-Bauteile darin.
#[test]
fn csv_mit_kopfzeile() {
    let mut s = standardhaus();
    let ganz = s.schedule_in(&sk_model::qto::Umfang::projekt());
    let kopf = "Gebäude 1 · alle Geschosse · Stand 08.10.2026, 11:34";
    let b = csv_mit_kopf(s.model(), &ganz, Grouping::Storey, kopf);
    let ohne = csv_grouped(s.model(), &ganz, Grouping::Storey);
    assert!(b.starts_with(&[0xEF, 0xBB, 0xBF]));
    let text = String::from_utf8(b[3..].to_vec()).unwrap();
    assert!(text.starts_with(&format!("{kopf}\r\n")), "{text}");
    assert_eq!(&b[3 + kopf.len() + 2..], &ohne[3..]);
    // nur EG
    let m = s.model().clone();
    let mut u = sk_model::qto::Umfang::projekt();
    let c = umfang_view::umfang_chips(&m, None);
    assert!(umfang_view::klick(&mut u, &c, 1, false));
    let eg = s.schedule_in(&u);
    let csv = String::from_utf8(csv_grouped(&m, &eg, Grouping::Storey)).unwrap();
    let ganz_csv = String::from_utf8(ohne).unwrap();
    assert!(csv.len() < ganz_csv.len());
    assert!(
        !csv.contains("Obergeschoss") && !csv.contains("Gründung"),
        "{csv}"
    );
    assert!(ganz_csv.contains("Obergeschoss"));
}

/// Liste mit einem Haus 10 × 8 m nach Gewerk in Breite `w` (dip).
fn gewerk_liste(w: u32) -> ListView {
    use sk_math::vec3;
    use sk_model::{RefSide, WallChain};
    let mut s = Scene::with_model(Model::with_seed(14));
    let chain = WallChain {
        base: 0.0,
        points: vec![
            vec3(0.0, 0.0, 0.0),
            vec3(0.0, 8000.0, 0.0),
            vec3(10000.0, 8000.0, 0.0),
            vec3(10000.0, 0.0, 0.0),
        ],
        closed: true,
        ref_side: RefSide::Left,
        layers: Vec::new(),
        height: 2750.0,
        joints: Default::default(),
    };
    s.add_wall(&chain).unwrap();
    let mut v = ListView::grouped(&mut s, Grouping::Trade);
    v.subtitle = "Gebäude 1 (GB-01) · Stand 07.10.2026, 10:56".into();
    v.w = w;
    v
}

/// Einstellungen §14 (n): Bei 520 steht der Umschalter rechtsbündig auf
/// der Unterzeile, Mitte auf Mitte, der Kopf bleibt so hoch wie bisher;
/// reicht der Platz dort nicht, bekommt er eine eigene Zeile darunter
/// und die Liste rückt um diese Zeile nach unten.
#[test]
fn umschalter_auf_der_unterzeile() {
    let (t, fonts) = (Theme::dark(), Fonts::system());
    let v = gewerk_liste(520);
    let (_, halves, own) = v.toggle_layout(&t, &fonts);
    assert!(!own);
    let (_, (x, y, w, h)) = halves[1];
    let (x0, cw) = v.content_x(&t);
    let inset = TOGGLE_INSET;
    assert!((x + w + inset - (x0 + cw)).abs() < 1e-3, "rechtsbündig");
    let cap = fonts.regular.as_ref().map_or(7.5, |f| f.cap_height(10.5));
    let sub_mid = v.top_dip() + 52.0 - cap * 0.5;
    assert!((y + h * 0.5 - sub_mid).abs() < 1e-3, "Mitte auf Mitte");
    v.fit_head(&t, &fonts);
    assert_eq!(v.head(), HEAD);

    let v = gewerk_liste(320);
    let (_, halves, own) = v.toggle_layout(&t, &fonts);
    assert!(own);
    let (_, (_, y2, _, h2)) = halves[0];
    assert!((y2 + h2 * 0.5 - (sub_mid + TOGGLE_ROW)).abs() < 1e-3);
    v.fit_head(&t, &fonts);
    assert_eq!(v.head(), HEAD + TOGGLE_ROW);
}

/// Einstellungen §14 (o): Ein gekürzter Zeilenname gibt den vollen Namen
/// als Hinweis; ein ganz gezeigter nicht.
#[test]
fn gekuerzter_name_als_hinweis() {
    let (t, fonts) = (Theme::dark(), Fonts::system());
    if fonts.regular.is_none() {
        return;
    }
    let v = gewerk_liste(520);
    v.fit_head(&t, &fonts);
    let list_top = v.top_dip() + v.head();
    let lines: Vec<(usize, f32, f32)> = v.layout(Some(&t));
    let mut cut = None;
    let mut whole = None;
    for (i, y, h) in lines {
        let l = &v.lines[i];
        let (x, yy) = (
            v.content_x(&t).0 as f64 + 40.0,
            (list_top + y + h * 0.5) as f64,
        );
        match v.tip_at(&t, &fonts, x, yy) {
            Some(full) => cut = Some((full, l.cells[0].clone())),
            None if l.kind == Kind::Row => whole = Some(l.cells[0].clone()),
            None => {}
        }
    }
    let (full, name) = cut.expect("eine Zeile ist gekürzt");
    assert_eq!(full, name);
    assert!(full.starts_with("Dämmung (WDVS) 14 · Außenwände"), "{full}");
    assert!(whole.is_some());
}
