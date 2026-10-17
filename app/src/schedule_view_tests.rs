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
    // Kaufmännisch: die Hälfte vom Nullpunkt weg, auch knapp darunter
    // abgelegt; große Zahlen behalten die letzte Stelle (H-170a)
    assert_eq!(de(4.5675, 3), "4,568");
    assert_eq!(de(23.815, 2), "23,82");
    assert_eq!(de(-23.815, 2), "−23,82");
    assert_eq!(csv_num(236000.12341), "236000,1234");
    assert_eq!(csv_num(236000.12345), "236000,1235");
    assert_eq!(de(236000.12341, 4), "236.000,1234");
}

/// Review M1: Der schnelle Weg von `de` schreibt dieselben Ziffern wie der
/// lange über `format!`, auch an halben Stellen, bei negativen und sehr
/// großen Zahlen.
#[test]
fn zahlen_schnell_wie_lang() {
    let mut x = 0.000_123_f64;
    while x < 1e22 {
        for v in [x, -x, x * 1.000_5, (x * 1e3).round() / 1e3 + 5e-4] {
            for dec in 0..=4 {
                assert_eq!(de(v, dec), zahl::de_lang(v, dec), "{v} auf {dec}");
                assert_eq!(de(-v, dec), zahl::de_lang(-v, dec), "{} auf {dec}", -v);
            }
        }
        x *= 1.0123;
    }
    assert_eq!(de(f64::NAN, 2), zahl::de_lang(f64::NAN, 2));
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
    let b = csv_mit_kopf(s.model(), &ganz, (Grouping::Storey, Modus::Vob), kopf);
    let ohne = csv_grouped(s.model(), &ganz, Grouping::Storey);
    assert!(b.starts_with(&[0xEF, 0xBB, 0xBF]));
    let text = String::from_utf8(b[3..].to_vec()).unwrap();
    let erste = format!("VOB-Mengen · {kopf}\r\n");
    assert!(text.starts_with(&erste), "{text}");
    assert_eq!(&b[3 + erste.len()..], &ohne[3..]);
    // Bestellung: die Sicht steht vorn (Bedienbarkeit 38.3)
    let best = csv_mit_kopf(
        s.model(),
        &ganz,
        (Grouping::Storey, Modus::Bestellung),
        kopf,
    );
    let best = String::from_utf8(best[3..].to_vec()).unwrap();
    assert!(
        best.starts_with(&format!("Bestellmengen mit Verschnitt · {kopf}\r\n")),
        "{best}"
    );
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

/// Notiz Kopf §3: „Gliedern“ steht in eigener Zeile unter den Chips bei
/// x0 + 300 wie im Reiter Kosten, im schmalen Fenster links bei x0; der
/// Kopf ist immer gleich hoch.
#[test]
fn gliedern_unter_den_chips() {
    let (t, fonts) = (Theme::dark(), Fonts::system());
    for (w, links) in [(1240, false), (320, true)] {
        let v = gewerk_liste(w);
        let ((lx, _), halves) = v.toggle_layout(&t, &fonts);
        let (x0, cw) = v.content_x(&t);
        if links {
            assert!((lx - x0).abs() < 1e-3, "{w}: links");
        } else {
            assert!(
                (lx - (x0 + crate::kosten_view::GLIEDERN_X)).abs() < 1e-3,
                "{w}"
            );
        }
        let (_, (x, y, hw, _)) = halves[1];
        assert!(x + hw <= x0 + cw + 1e-3 || links, "{w}: im Inhalt");
        let lage = v.leiste_lage(&t);
        assert!(y >= lage.y + umfang_view::ROW, "{w}: unter den Chips");
        v.fit_head(&t, &fonts);
        assert_eq!(v.head(), HEAD);
    }
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
    let mut cut = Vec::new();
    let mut whole = None;
    for (i, y, h) in lines {
        let l = &v.lines[i];
        let (x, yy) = (
            v.content_x(&t).0 as f64 + 40.0,
            (list_top + y + h * 0.5) as f64,
        );
        match v.tip_at(&t, &fonts, x, yy) {
            Some(full) => cut.push((full, l.cells[0].clone())),
            None if l.kind == Kind::Row => whole = Some(l.cells[0].clone()),
            None => {}
        }
    }
    // Der Hinweis nennt den vollen Namen; gekürzt sind die langen Gruppen,
    // darunter seit RB5 auch Estrich und Dämmung der Räume
    for (full, name) in &cut {
        assert_eq!(full, name);
    }
    let namen: Vec<&str> = cut.iter().map(|(f, _)| f.as_str()).collect();
    assert!(
        namen.contains(&"Dämmung (WDVS) 14 · Außenwände"),
        "{namen:?}"
    );
    assert!(
        namen.contains(&"Zementestrich CT-C25-F4 6,5 · Räume"),
        "{namen:?}"
    );
    assert!(whole.is_some());
}

/// Jörn 08.10. (Bild): Bei schmalem Fenster lag „Als Tabelle speichern“
/// hinter dem Umschalter „Gliedern nach“. Knopf und Umschalter überlappen
/// bei keiner Breite und keiner Skalierung.
#[test]
fn knopf_und_umschalter_ueberlappen_nie() {
    let t = Theme::dark();
    let lib = std::path::Path::new("/usr/share/fonts/truetype/liberation");
    let lade = |n: &str| {
        std::fs::read(lib.join(n))
            .ok()
            .and_then(sk_paint::font::Font::parse)
    };
    let echte = Fonts {
        regular: lade("LiberationSans-Regular.ttf"),
        bold: lade("LiberationSans-Bold.ttf"),
        italic: None,
    };
    for fonts in [Fonts::system(), echte] {
        for scale in [1.0, 1.25, 1.5] {
            for w in (300..1400).step_by(20) {
                let mut v = gewerk_liste(w);
                v.scale = scale;
                let (bx, by, bw, bh) = v.button_rect(&t, &fonts);
                let ((lx, _), halves) = v.toggle_layout(&t, &fonts);
                let i = TOGGLE_INSET * scale;
                let (_, (x, y, _, h)) = halves[0];
                let (_, (x2, _, w2, _)) = halves[1];
                let (px, py, pw, ph) = (x - i, y - i, x2 + w2 + i - (x - i), h + 2.0 * i);
                let frei = |(ax, ay, aw, ah): (f32, f32, f32, f32)| {
                    ax + aw <= bx || bx + bw <= ax || ay + ah <= by || by + bh <= ay
                };
                assert!(
                    frei((px, py, pw, ph)),
                    "Breite {w}, Skala {scale}: Knopf und Umschalter überlappen"
                );
                assert!(lx >= 0.0);
                // Notiz Kopf §2: in der Kartenzeile nie über den Karten
                let (x0, _) = v.content_x(&t);
                let breit = w as f32 - 2.0 * x0;
                let karten_y = (v.top_dip() - crate::cards::HEIGHT) * scale;
                if by < v.top_dip() * scale {
                    assert!(by >= karten_y, "Breite {w}, Skala {scale}");
                    assert!(
                        crate::cards::Karten::ende(x0, scale, breit) + 16.0 * scale <= bx + 1e-3,
                        "Breite {w}, Skala {scale}: Knopf über den Karten"
                    );
                }
            }
        }
    }
}

/// Jörn 11.10.: Die Sicht der Mengen (VOB oder Bestellung) wird gemerkt;
/// `mengen=` steht nur bei der Bestellung.
#[test]
fn mengen_sicht_wird_gemerkt() {
    use crate::cards::Blatt;
    use crate::windows::{read_modus, write_settings_mengen, Windows};
    let w = Windows::new(520.0);
    assert_eq!(
        write_settings_mengen(&w, Grouping::Storey, Blatt::Mengen, Modus::Vob),
        ""
    );
    let text = write_settings_mengen(&w, Grouping::Storey, Blatt::Mengen, Modus::Bestellung);
    assert!(text.contains(" mengen=bestellung"), "{text}");
    assert_eq!(read_modus(&text), Modus::Bestellung);
    assert_eq!(read_modus(""), Modus::Vob);
    assert_eq!(read_modus("[mengenfenster] mengen=quer\n"), Modus::Vob);
}

/// Jörn 11.10.: Unter jeder Schichtzeile nach Gewerk steht der Rechenweg,
/// zugeklappt; aufgeklappt zeigt er Ansatz, Zu- und Abschläge und die
/// Menge. Mauerwerk zeigt seine Abrechnungsfläche. Gerechnet wird er erst
/// beim Aufklappen (Review #77/1).
#[test]
fn rechenweg_unter_der_zeile() {
    let mut s = standardhaus();
    let mut v = ListView::grouped(&mut s, Grouping::Trade);
    let wege: Vec<Key> = v.lines.iter().filter(|l| l.weg).map(|l| l.key).collect();
    assert!(!wege.is_empty(), "Zeilen mit Rechenweg");
    let terme = |v: &ListView, k: Key| -> Vec<usize> {
        (0..v.lines.len())
            .filter(|&i| v.lines[i].parent == k)
            .collect()
    };
    for &k in &wege {
        assert!(terme(&v, k).is_empty(), "{k:?} zugeklappt schon gerechnet");
    }
    v.alles_aufklappen();
    v.sync(&mut s, false);
    for &k in &wege {
        assert!(!terme(&v, k).is_empty(), "{k:?} ohne Terme");
    }
    // nur die Zeile wieder aufklappen: Terme erst nach dem Neuaufbau
    let k = wege[0];
    v.open_groups.clear();
    v.runs = None;
    v.sync(&mut s, false);
    assert!(terme(&v, k).is_empty());
    let i = v.lines.iter().position(|l| l.key == k).unwrap();
    let g = v.lines[i].group;
    if g != Key::None {
        v.open_groups.insert(g);
    }
    assert!(v.line_visible(&v.lines[i]));
    v.open_groups.insert(k);
    v.runs = None;
    v.sync(&mut s, false);
    let t = terme(&v, k);
    assert!(!t.is_empty());
    assert!(t.iter().all(|&j| v.line_visible(&v.lines[j])));
    let texte = v.line_texts();
    assert!(
        t.iter().any(|&j| texte[j].contains(" × ")),
        "Ansatz: {:?}",
        t.iter().map(|&j| &texte[j]).collect::<Vec<_>>()
    );
    // Mauerwerk mit Fläche
    let mw = texte
        .iter()
        .find(|x| x.contains("Mauerwerk") || x.contains("Kalksandstein"))
        .cloned();
    if let Some(mw) = mw {
        assert!(mw.contains(" m²"), "{mw}");
    }
    // auch in der Tabelle, Spalte Hinweis
    let csv = String::from_utf8(csv_grouped(
        s.model(),
        &sk_model::qto::schedule(s.model()),
        Grouping::Trade,
    ))
    .unwrap();
    assert!(
        csv.contains("− Deckenauflager") && csv.contains("× Höhe 0,580 m"),
        "{csv}"
    );
}

/// Jörn 11.10.: Die Bestellmenge liegt um den Verschnittzuschlag über der
/// VOB-Menge; der Rechenweg nennt den Zuschlag.
#[test]
fn bestellmenge_mit_zuschlag() {
    let mut s = standardhaus();
    let mut v = ListView::grouped(&mut s, Grouping::Trade);
    v.alles_aufklappen();
    v.sync(&mut s, false);
    let vob = v.line_texts();
    assert!(v.set_modus(Modus::Bestellung));
    assert!(!v.set_modus(Modus::Bestellung));
    v.sync(&mut s, false);
    assert_eq!(v.modus(), Modus::Bestellung);
    let best = v.line_texts();
    assert_ne!(vob, best);
    assert!(best.iter().any(|x| x.contains("Verschnittzuschlag")));
    assert!(!vob.iter().any(|x| x.contains("Verschnittzuschlag")));
    // Summe einer Schichtzeile mit Fläche: Bestellung größer als VOB
    let flaeche = |ls: &[Line]| -> f64 {
        ls.iter()
            .filter(|l| l.kind == Kind::Row && l.weg)
            .filter_map(|l| {
                l.cells[3]
                    .trim_end_matches(" m²")
                    .replace('.', "")
                    .replace(',', ".")
                    .parse::<f64>()
                    .ok()
            })
            .sum()
    };
    let b = flaeche(&v.lines);
    v.set_modus(Modus::Vob);
    v.sync(&mut s, false);
    let a = flaeche(&v.lines);
    assert!(a > 0.0 && b > a * 1.03, "VOB {a} Bestellung {b}");
}

/// Review #77, Nachtrag A: In der Bestell-CSV nach Gewerk trägt auch die
/// „Summe nach Baustoff“ den Zuschlag des Baustoffs.
#[test]
fn bestell_csv_summe_nach_baustoff() {
    let s = standardhaus();
    let sched = sk_model::qto::schedule(s.model());
    let summen = |modus| -> Vec<f64> {
        let csv = String::from_utf8(csv_sicht(s.model(), &sched, Grouping::Trade, modus)).unwrap();
        csv.lines()
            .filter(|l| l.contains("Summe nach Baustoff"))
            .map(|l| {
                let c: Vec<&str> = l.split(';').collect();
                let f = |x: &str| {
                    x.trim_matches('"')
                        .replace(',', ".")
                        .parse::<f64>()
                        .unwrap_or(0.0)
                };
                f(c[7]) + f(c[8])
            })
            .collect()
    };
    let (vob, best) = (summen(Modus::Vob), summen(Modus::Bestellung));
    assert_eq!(vob.len(), best.len());
    assert!(!vob.is_empty());
    for (a, b) in vob.iter().zip(&best) {
        assert!(*b >= *a, "Bestellung {b} < VOB {a}");
    }
    assert!(
        vob.iter().zip(&best).any(|(a, b)| *b > a * 1.04),
        "{vob:?} {best:?}"
    );
}

/// Haus 10 × 8 m mit zwei Geschossen, je Geschoss ein Raum mit den
/// Ausbautypen des Projekts (RB5).
fn haus_mit_raeumen() -> Scene {
    use sk_math::vec3;
    let mut m = Model::with_seed(9);
    let b = m.add_building(2);
    let pts = [
        vec3(0.0, 0.0, 0.0),
        vec3(0.0, 8000.0, 0.0),
        vec3(10000.0, 8000.0, 0.0),
        vec3(10000.0, 0.0, 0.0),
    ];
    m.build_from_polygon(b, &pts).unwrap();
    let geschosse: Vec<sk_model::StoreyId> = m
        .levels_in(Some(b))
        .into_iter()
        .filter(|g| m.storey(*g).unwrap().kind == sk_model::LevelKind::Storey)
        .collect();
    m.begin("Räume");
    assert!(sk_raum::abgleichen(&mut m, &geschosse) > 0);
    m.commit();
    for g in geschosse {
        let id = m.spaces(g)[0];
        m.begin("Ausbau");
        assert!(sk_raum::ausbau::typen_anlegen(&mut m, id));
        m.commit();
    }
    Scene::with_model(m)
}

/// Jede Zeile der Liste hat einen eigenen Schlüssel (Auf- und Zuklappen).
fn eindeutig(v: &ListView) {
    let mut keys: Vec<String> = v
        .lines
        .iter()
        .filter(|l| l.key != Key::None)
        .map(|l| format!("{:?}", l.key))
        .collect();
    let n = keys.len();
    keys.sort();
    keys.dedup();
    assert_eq!(keys.len(), n, "Schlüssel doppelt");
}

/// RB5 (raumbuch/plan.md §6.2): Nach Geschoss steht je Raum eine Gruppe
/// mit lichter Höhe und Bodenfläche, darunter Boden, Estrich, Wandseiten,
/// Decke und Längen mit Rechenweg; nach Gewerk die Schichten als
/// „… · Räume“, je Raum eine Zeile, deren Rechenweg die Flächen nennt;
/// beides auch in der Tabelle.
#[test]
fn raeume_in_der_mengenliste() {
    let mut s = haus_mit_raeumen();
    let sched = s.schedule_in(&sk_model::qto::Umfang::projekt());
    assert!(
        sched
            .auto
            .iter()
            .any(|a| a.key == "room.skirting" && a.value > 0.0),
        "Sockelleiste als Automatikmenge"
    );
    let mut v = ListView::grouped(&mut s, Grouping::Storey);
    v.alles_aufklappen();
    v.sync(&mut s, false);
    let texte = v.line_texts();
    let raeume: Vec<&String> = texte.iter().filter(|t| t.contains("lichte Höhe")).collect();
    assert_eq!(raeume.len(), 2, "{texte:#?}");
    assert!(raeume.iter().all(|t| t.contains(" m²")), "{raeume:?}");
    for name in ["Boden", "Estrich", "Wand AW-", "Decke", "Sockelleiste"] {
        assert!(
            texte.iter().any(|t| t.trim_start().starts_with(name)),
            "{name}: {texte:#?}"
        );
    }
    // Rechenweg der Wandseite (Rohbaumaß, Putz bis auf die Rohdecke) und
    // der Sockelleiste (je Wand)
    let i = texte
        .iter()
        .position(|t| t.trim_start().starts_with("Wand AW-001 West · Gipsputz"))
        .unwrap();
    let k = v.lines[i].key;
    assert!(v.lines[i].weg);
    let terme: Vec<&String> = (0..v.lines.len())
        .filter(|&j| v.lines[j].parent == k)
        .map(|j| &texte[j])
        .collect();
    assert_eq!(terme.len(), 1, "{terme:?}");
    assert!(terme[0].contains("Länge × Höhe 7,370 × 2,635"), "{terme:?}");
    assert!(texte[i].contains("19,42 m²") && texte[i].contains("0,291 m³"));
    let i = texte
        .iter()
        .position(|t| t.trim_start().starts_with("Sockelleiste"))
        .unwrap();
    let k = v.lines[i].key;
    let terme = (0..v.lines.len())
        .filter(|&j| v.lines[j].parent == k)
        .count();
    assert_eq!(terme, 5, "vier Wände und die Summe");
    // jede Zeile mit eigenem Schlüssel
    eindeutig(&v);

    // nach Gewerk: „… · Räume“, Rechenweg mit den Namen der Flächen
    assert!(v.set_grouping(Grouping::Trade));
    v.sync(&mut s, false);
    v.alles_aufklappen();
    v.sync(&mut s, false);
    let texte = v.line_texts();
    assert!(texte.iter().any(|t| t.contains(" · Räume")), "{texte:#?}");
    eindeutig(&v);
    let g = texte
        .iter()
        .position(|t| {
            t.starts_with("  Gipsputz 1,5 · Räume | R-001 … 002 | 2 | 176,44 m² | 2,647 m³")
        })
        .unwrap_or_else(|| panic!("{texte:#?}"));
    let i = g + 1;
    assert!(
        texte[i].starts_with("    R-001 Raum 1 · EG · Außenwände |  |  | 88,22 m²"),
        "{}",
        texte[i]
    );
    // Anstrich ohne Dicke und ohne m³
    let a = texte
        .iter()
        .find(|t| t.starts_with("  Dispersionsanstrich · Räume"))
        .unwrap_or_else(|| panic!("{texte:#?}"));
    assert!(!a.contains("m³"), "{a}");
    // je Kostengruppe eine Zeile: Wände (336) und Decke (354) getrennt
    assert!(
        texte
            .iter()
            .any(|t| t.starts_with("    R-001 Raum 1 · EG · Decke |  |  | 69,06 m² | ")),
        "{texte:#?}"
    );
    let k = v.lines[i].key;
    let terme: Vec<&String> = (0..v.lines.len())
        .filter(|&j| v.lines[j].parent == k)
        .map(|j| &texte[j])
        .collect();
    assert!(
        terme.iter().any(|t| t.contains("Wand AW-")),
        "Name der Fläche im Rechenweg: {texte:#?}"
    );

    // Tabelle
    let m = s.model().clone();
    // Seit Werksstand 19 (RB7) bepreisen die Kosten die Raumzeilen
    let k = sk_cost::lesen::katalog(&m, None);
    let u = sk_cost::Umfang::projekt();
    let ohne = sk_cost::lesen::kosten(&m, &sk_model::qto::schedule(&m), &k, &u);
    assert!(sk_cost::lesen::kosten(&m, &sched, &k, &u).netto.0 > ohne.netto.0);
    let csv = String::from_utf8(csv_grouped(&m, &sched, Grouping::Storey)).unwrap();
    assert!(
        csv.contains(";Raum ") && csv.contains("Sockelleiste"),
        "{csv}"
    );
    assert!(
        csv.contains(";345;") || csv.contains(";336;"),
        "Kostengruppe Wand: {csv}"
    );
    let csv = String::from_utf8(csv_grouped(&m, &sched, Grouping::Trade)).unwrap();
    assert!(
        csv.contains(" · Räume") && csv.contains("(Wand AW-"),
        "{csv}"
    );
    assert!(
        csv.contains(
            ";354;Dispersionsanstrich · Räume · Decke;R-001;Erdgeschoss;;69,0569;;(Decke)"
        ),
        "{csv}"
    );
}

/// Bedienbarkeit 43.10: Nach Gewerk stehen die Längen der Räume im Gewerk
/// ihrer Fläche, die Sockelleiste mit ihrem Typ beim Parkett, der
/// Randdämmstreifen beim Estrich, je Raum eine Zeile mit Rechenweg. Sie
/// zählen nicht in die Summe des Gewerks, jede steht einmal, und die
/// Kosten bepreisen sie als Automatikmenge am Raum (Werksstand 19).
#[test]
fn laengen_der_raeume_nach_gewerk() {
    let mut s = haus_mit_raeumen();
    let sched = s.schedule_in(&sk_model::qto::Umfang::projekt());
    let mut v = ListView::grouped(&mut s, Grouping::Trade);
    v.alles_aufklappen();
    v.sync(&mut s, false);
    let texte = v.line_texts();
    eindeutig(&v);
    let gewerk_von = |i: usize| {
        (0..i)
            .rev()
            .find(|&j| v.lines[j].kind == Kind::Storey)
            .map(|j| texte[j].clone())
            .unwrap()
    };
    for (titel, gewerk) in [
        (
            "  Sockelleiste Eiche 60 × 15 · Räume | R-001 … 002 | 66,72 m · 2 |",
            "Parkett",
        ),
        (
            "  Randdämmstreifen · Räume | R-001 … 002 | 66,72 m · 2 |",
            "Estrich",
        ),
    ] {
        let n: Vec<usize> = (0..texte.len())
            .filter(|&i| texte[i].starts_with(titel))
            .collect();
        assert_eq!(n.len(), 1, "{titel}: {texte:#?}");
        assert!(gewerk_von(n[0]).starts_with(gewerk), "{titel}");
        assert!(
            texte[n[0] + 1].starts_with("    R-001 Raum 1 · EG |  | 33,36 m"),
            "{}",
            texte[n[0] + 1]
        );
        assert!(v.lines[n[0] + 1].weg);
    }
    // Rechenweg der Sockelleiste aufgeklappt: je Wand und die Summe
    let i = texte
        .iter()
        .position(|t| t.starts_with("  Sockelleiste Eiche"))
        .unwrap()
        + 1;
    let k = v.lines[i].key;
    let terme = (0..v.lines.len())
        .filter(|&j| v.lines[j].parent == k)
        .count();
    assert_eq!(terme, 5, "vier Wände und die Summe");

    // Tabelle: hinter der Summe des Gewerks, die sie nicht enthält
    let m = s.model().clone();
    let csv = String::from_utf8(csv_grouped(&m, &sched, Grouping::Trade)).unwrap();
    let zeilen: Vec<&str> = csv.lines().collect();
    let summe = zeilen
        .iter()
        .position(|l| l.contains(";Summe Estricharbeiten;"))
        .unwrap();
    assert!(zeilen[summe].contains(";;;;137,1112;"), "{}", zeilen[summe]);
    assert!(
        zeilen[summe + 1].contains(
            ";Randdämmstreifen · Räume (Summe, 2 Stück);R-001 … 002;;66,7200;;;Automatikmenge"
        ),
        "{}",
        zeilen[summe + 1]
    );
    assert!(
        zeilen[summe + 2]
            .contains(";324;Randdämmstreifen · Räume;R-001;Erdgeschoss;33,3600;;;Umfang bis Putz"),
        "{}",
        zeilen[summe + 2]
    );
    // Die Anzeige zeigt genau die Automatikmengen, die Kosten bepreisen
    // (`auto=`): je Schlüssel dieselbe Summe
    for (key, soll) in [("room.skirting", 66_720.0), ("room.edgestrip", 66_720.0)] {
        let ist: f64 = sched
            .auto
            .iter()
            .filter(|a| a.key == key)
            .map(|a| a.value)
            .sum();
        assert!((ist - soll).abs() < 1e-6, "{key}: {ist}");
    }
    // Seit Werksstand 19 (RB7) bepreisen die Kosten diese Längen: die
    // Sockelleiste Eiche (PA20) und der Randdämmstreifen (ES40) je Raum
    let k = sk_cost::lesen::katalog(&m, None);
    let u = sk_cost::Umfang::projekt();
    let b = sk_cost::lesen::kosten(&m, &sched, &k, &u);
    for kurz in ["Sockelleiste Eiche", "Randdämmstreifen"] {
        assert!(
            b.positionen
                .iter()
                .any(|p| p.kurz.contains(kurz) && p.gp.0 > 0),
            "{kurz}"
        );
    }
}

/// Bedienbarkeit 38.2: Nach Geschoss zeigt die Wandzeile die Menge ihrer
/// Schichtzeilen in der Sicht, nach VOB/C mit den übermessenen Öffnungen,
/// in der Bestellung mit Verschnitt; so ergeben die Zeilen die Summe nach
/// Baustoff. Fenster 3,00 × 1,51 m (abgezogen) und 1,25 × 1,51 m
/// (übermessen) in der Südwand.
#[test]
fn wandzeilen_nach_vob() {
    use sk_math::vec3;
    use sk_model::oeffnung::{HeightLine, HeightRef, Opening, OpeningKind};
    let mut m = Model::with_seed(311);
    let b = m.add_building(2);
    let pts = [
        vec3(0.0, 0.0, 0.0),
        vec3(0.0, 8000.0, 0.0),
        vec3(10000.0, 8000.0, 0.0),
        vec3(10000.0, 0.0, 0.0),
    ];
    let eg = m.build_from_polygon(b, &pts).unwrap();
    let st = m.run(eg).unwrap().storey;
    let mut s = Scene::with_model(m);
    let d = vec3(0.0, 1.0, 0.0);
    for (x, breite) in [(3000.0, 3000.0), (7500.0, 1250.0)] {
        let (face, p) = s
            .outer_face_at(vec3(x, -9000.0, 1500.0), d)
            .expect("Außenfläche");
        let a = face.at(face.u_of(p) - breite / 2.0, 0.0);
        let c = face.at(face.u_of(p) + breite / 2.0, 0.0);
        s.add_opening(Opening {
            kind: OpeningKind::Window,
            path: vec![vec3(a.x, a.y, 0.0), vec3(c.x, c.y, 0.0)],
            bottom: HeightRef::Line(st, HeightLine::Sill),
            top: HeightRef::Line(st, HeightLine::Lintel),
            reveal: None,
            install: Default::default(),
            swing: None,
            lock: Default::default(),
        })
        .expect("Fenster");
    }
    let m = s.model();
    let sched = sk_model::qto::schedule(m);
    let rows = sched.layer_rows(m);
    let mut uebermessen = 0;
    for modus in Modus::ALL {
        let f = bestell_faktoren(m, &sched, modus);
        let walls = sched.buildings[0]
            .storeys
            .iter()
            .flat_map(|st| st.groups.iter().flat_map(|g| g.rows.iter()));
        for r in walls {
            let Some(ElementQto::Wall(w)) = &r.q else {
                continue;
            };
            let soll: f64 = rows
                .iter()
                .filter(|(_, x)| x.element == r.element)
                .map(|(_, x)| rechenweg::anzeige(m, x, Bezug::Volumen, modus))
                .sum();
            let ist = wand_vol(&f, r.element, w.volume);
            assert!(
                (ist - soll).abs() < 1.0,
                "{} {modus:?}: {ist} ≠ {soll}",
                r.number
            );
            if modus == Modus::Vob && ist > w.volume + 1.0 {
                uebermessen += 1;
            }
        }
    }
    assert_eq!(uebermessen, 1, "nur die Südwand mit dem kleinen Fenster");
}

/// G8 (Jörn 13.10. 02:14): Das Blatt Wohnfläche zeigt je Geschoss die
/// Räume mit Faktor, Grund- und Wohnfläche wie die Rechnung aus dem Modell
/// allein; aufgeklappt die Mengenansätze. Der Umfang wirkt wie in den
/// Mengen, die Karte nennt die Summe, die Tabelle die Ansätze.
#[test]
fn wohnflaeche_als_blatt() {
    let mut s = haus_mit_raeumen();
    let mut v = ListView::wohnflaeche(&mut s);
    let modell = sk_raum::wohnflaeche::projekt(s.model());
    assert_eq!(*v.wohn_rechnung().unwrap(), modell, "Szene wie Modell");
    let texte = v.line_texts();
    assert_eq!(
        texte,
        [
            "Erdgeschoss |  |  | 68,56 m² | 68,56 m²",
            "  Raum 1 | R-001 | 100 % | 68,56 m² | 68,56 m²",
            "Obergeschoss |  |  | 68,56 m² | 68,56 m²",
            "  Raum 2 | R-002 | 100 % | 68,56 m² | 68,56 m²",
            " |  |  |  | ",
            "Wohnfläche · Gebäude 1 |  |  | 137,12 m² | 137,12 m²",
        ],
        "{texte:#?}"
    );
    eindeutig(&v);
    // Aufgeklappt: Mengenansatz, Grundfläche, Anrechnung
    let k = v.lines[1].key;
    assert!(v.lines[1].weg);
    v.open_groups.insert(k);
    v.runs = None;
    v.sync(&mut s, false);
    let texte = v.line_texts();
    let terme: Vec<&String> = (0..v.lines.len())
        .filter(|&j| v.lines[j].parent == k)
        .map(|j| &texte[j])
        .collect();
    assert_eq!(terme.len(), 3, "{terme:#?}");
    assert_eq!(terme[0], "    (1) 9,340 × 7,340 |  |  | 68,556 m² | ");
    assert_eq!(terme[1], "    = Grundfläche |  |  | 68,56 m² | ");
    assert!(
        terme[2].starts_with("    × 100 % · lichte Höhe ") && terme[2].ends_with(" | 68,56 m²"),
        "{}",
        terme[2]
    );
    eindeutig(&v);
    // Umfang: nur das EG
    let og = s
        .model()
        .storeys()
        .iter()
        .find(|(_, x)| x.name == "Obergeschoss")
        .unwrap()
        .0;
    v.leiste.umfang.ohne = vec![og];
    v.sync(&mut s, false);
    let texte = v.line_texts();
    assert!(
        texte.iter().all(|t| !t.contains("Obergeschoss")),
        "{texte:#?}"
    );
    assert_eq!(
        texte.last().unwrap(),
        "Wohnfläche · Gebäude 1 |  |  | 68,56 m² | 68,56 m²"
    );
    let w = v.wohn_rechnung().unwrap();
    assert_eq!(wohn_karte(&w, &v.leiste.umfang), "68,56 m² · 1 Raum");
    assert_eq!(
        wohn_karte(&w, &sk_model::qto::Umfang::projekt()),
        "137,12 m² · 2 Räume"
    );
    // Tabelle: BOM, Kopf, Raum, Ansatz, Summen
    let csv =
        String::from_utf8(wohn_csv(&w, &sk_model::qto::Umfang::projekt(), "Gebäude 1")).unwrap();
    let zeilen: Vec<&str> = csv.trim_start_matches('\u{feff}').lines().collect();
    assert!(
        zeilen[0].starts_with("Wohnflächenberechnung · Gebäude 1"),
        "{csv}"
    );
    assert!(
        zeilen[1].starts_with("Gebäude;Geschoss;Raum;Nr.;Mengenansatz;"),
        "{csv}"
    );
    assert!(
        zeilen[2].starts_with("Gebäude 1;Erdgeschoss;Raum 1;R-001;;;68,56;100;68,56;lichte Höhe ")
    );
    assert_eq!(
        zeilen[3],
        "Gebäude 1;Erdgeschoss;Raum 1;R-001;(1) 9,340 × 7,340;68,5556;;;;"
    );
    assert!(csv.contains("Gebäude 1;Erdgeschoss;Summe Erdgeschoss;;;;68,56;;68,56;\r\n"));
    assert!(csv.contains("Gebäude 1;;Summe Gebäude 1;;;;137,12;;137,12;\r\n"));
    assert!(!csv.contains("Summe Projekt"), "ein Gebäude");
}

/// Das Blatt Wohnfläche hat keine Umschalter; dort stehen die Grundlagen,
/// und ein Klick an ihre Stelle schaltet nichts.
#[test]
fn wohnflaeche_ohne_umschalter() {
    let (t, fonts) = (Theme::dark(), Fonts::system());
    let mut s = haus_mit_raeumen();
    let mut p = Picking::default();
    let mut m = ListView::grouped(&mut s, Grouping::Storey);
    let mut v = ListView::wohnflaeche(&mut s);
    (m.w, m.h, v.w, v.h) = (900, 700, 900, 700);
    let (_, halves) = m.toggle_layout(&t, &fonts);
    let (x, y) = mitte(halves[1].1);
    assert_eq!(
        m.hit(&t, &fonts, x, y),
        Some(Hot::Grouping(Grouping::Trade))
    );
    assert_ne!(
        v.hit(&t, &fonts, x, y),
        Some(Hot::Grouping(Grouping::Trade))
    );
    assert_eq!(
        v.mouse_down(&t, &fonts, &mut p, (x, y), Default::default()),
        None
    );
    assert_eq!(v.grouping, Grouping::Storey);
    // Klick auf den Raum wählt seinen Raum im Modell
    let raum = v.lines[1].elements[0];
    let list_top = (v.top_dip() + v.head()) * v.scale;
    let (_, ly, lh) = v.layout(Some(&t))[1];
    let y = (list_top + (ly + lh * 0.5) * v.scale) as f64;
    let o = v.mouse_down(&t, &fonts, &mut p, (200.0, y), Default::default());
    assert_eq!(o, Some(ListOut::Picking { selection: true }));
    assert_eq!(p.selected, vec![raum]);
    assert_eq!(s.model().element(raum).unwrap().category, Category::Space);
}

/// Review M2: Die Liste der Szene nimmt die Mengen der Wandzüge aus der
/// Szene. Nach Öffnung, Ziehen, Rückgängig und Wiederholen sind die Mengen
/// jeder Wand, Decke und Gründung dieselben, die die Liste selbst rechnet.
#[test]
fn liste_mit_vorrat_wie_ohne() {
    use sk_math::vec3;
    use sk_model::oeffnung::{HeightLine, HeightRef, Opening, OpeningKind};
    let mut s = haus_mit_raeumen();
    let gleich = |s: &mut Scene, wann: &str| {
        let ohne = sk_model::qto::schedule(s.model());
        let mit = s.schedule().clone();
        let zeilen = |x: &Schedule| -> HashMap<ElementId, Option<ElementQto>> {
            x.buildings
                .iter()
                .flat_map(|b| &b.storeys)
                .chain(&x.loose)
                .flat_map(|st| &st.groups)
                .flat_map(|g| &g.rows)
                .filter(|r| !matches!(r.q, Some(ElementQto::Room(_))))
                .map(|r| (r.element, r.q.clone()))
                .collect()
        };
        let (a, b) = (zeilen(&mit), zeilen(&ohne));
        assert!(a.len() > 10, "{wann}");
        assert_eq!(a, b, "{wann}");
    };
    gleich(&mut s, "Anfang");
    let run = s.model().runs().ids().next().unwrap();
    let st = s.model().run(run).unwrap().storey;
    let (face, p) = s
        .outer_face_at(vec3(5000.0, -9000.0, 1500.0), vec3(0.0, 1.0, 0.0))
        .expect("Außenfläche");
    let (a, c) = (
        face.at(face.u_of(p) - 600.0, 0.0),
        face.at(face.u_of(p) + 600.0, 0.0),
    );
    let fenster = Opening {
        kind: OpeningKind::Window,
        path: vec![vec3(a.x, a.y, 0.0), vec3(c.x, c.y, 0.0)],
        bottom: HeightRef::Line(st, HeightLine::Sill),
        top: HeightRef::Line(st, HeightLine::Lintel),
        reveal: None,
        install: Default::default(),
        swing: None,
        lock: Default::default(),
    };
    let id = s.add_opening(fenster.clone()).expect("Fenster");
    gleich(&mut s, "Fenster");
    // Öffnung im Schritt verschieben und breiter machen (wie am Griff)
    let mut breiter = fenster;
    breiter.path[0].x -= 400.0;
    breiter.path[1].x += 200.0;
    assert!(s.opening_schritt(id, breiter, "Öffnung ändern"));
    gleich(&mut s, "Öffnung geändert");
    // Geschosshöhe und Höhenlinien (Paneel „Geschosse“), dann zurück
    let h = s.model().storey(st).unwrap().height;
    assert!(s.edit_model("Geschosshöhe", |m| m.set_storey_height(st, h + 250.0)));
    gleich(&mut s, "Geschosshöhe");
    let mut linien = s.model().storey(st).unwrap().lines;
    linien.sill += 150.0;
    assert!(s.edit_model("Höhenlinien", |m| m.set_height_lines(st, linien)));
    gleich(&mut s, "Brüstung");
    assert!(s.undo() && s.undo());
    gleich(&mut s, "Höhen rückgängig");
    let orig = s.chain(run).unwrap().clone();
    s.begin("Wand verschieben");
    for d in [-100.0, -200.0, -300.0] {
        let moved = orig.with_segment_moved(0, d).unwrap();
        s.set_run_points(run, &moved.points);
        s.schedule();
    }
    s.commit();
    gleich(&mut s, "gezogen");
    assert!(s.undo());
    gleich(&mut s, "rückgängig");
    assert!(s.redo());
    gleich(&mut s, "wiederholt");
    // Decke dicker: Auflager und Schalung der Wandzüge ändern sich ohne
    // Schritt an einem Zug (Kosten H-242a)
    let (decke, d) = s
        .model()
        .elements()
        .iter()
        .find_map(|(id, e)| match &e.kind {
            sk_model::ElementKind::Floor(f) => Some((id, f.thickness)),
            _ => None,
        })
        .expect("Decke");
    let waende = |m: &Model| -> Vec<ElementQto> {
        let x = sk_model::qto::schedule(m);
        x.buildings
            .iter()
            .flat_map(|b| &b.storeys)
            .flat_map(|st| &st.groups)
            .flat_map(|g| &g.rows)
            .filter_map(|r| r.q.clone().filter(|q| matches!(q, ElementQto::Wall(_))))
            .collect()
    };
    let vorher = waende(s.model());
    assert!(s.edit_model("Deckendicke", |m| m.set_floor_thickness(decke, d + 60.0)));
    assert_ne!(waende(s.model()), vorher, "Auflager in den Wänden");
    gleich(&mut s, "Decke dicker");
    assert!(s.undo());
    gleich(&mut s, "Decke zurück");
}
