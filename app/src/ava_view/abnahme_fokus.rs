//! Abnahme A341 „Bauteil ↔ AVA“ (Jörn 10.10.) an RH-1: Eine Auswahl im
//! Modell isoliert die Tabelle des LV auf die Positionen, die am Bauteil
//! hängen (am Fundament auch die Erdarbeiten), Titel nur darüber und ohne
//! Summe; zeigt das gezeigte Los nichts, wechselt das Blatt zum ersten Los
//! mit verknüpften Positionen. Die Leiste nennt Bauteil, Zahl und Los;
//! „Alle Positionen“ zeigt wieder alles. Ein Klick in der Tabelle isoliert
//! nicht.

use super::fokus_leiste::verknuepft;
use super::*;
use crate::fokus::Fokus;
use sk_model::element::Category;

fn haus() -> Scene {
    let m = sk_model::szo::read_with(
        include_str!("../../../crates/sk-cost/referenz/rh1-standardhaus.szo"),
        sk_model::GuidGen::with_seed(1),
        &sk_cost::lesen::ABSCHNITTE_SZO,
    )
    .expect("lädt")
    .model;
    Scene::with_model(m)
}

fn erstes(s: &Scene, c: Category) -> ElementId {
    s.model()
        .elements()
        .iter()
        .find(|(_, e)| e.category == c)
        .map(|(id, _)| id)
        .unwrap_or_else(|| panic!("{c:?}"))
}

fn waehle(v: &mut AvaView, s: &mut Scene, p: &mut Picking, auswahl: &[ElementId]) {
    p.selected = auswahl.to_vec();
    v.follow(p);
    v.sync(s, None);
}

fn positionen(v: &AvaView) -> Vec<String> {
    v.zeilen
        .iter()
        .filter(|z| z.art == Art::Position)
        .map(|z| z.oz.clone())
        .collect()
}

#[test]
fn a341_lv_isoliert_auf_das_bauteil() {
    let mut s = haus();
    let mut v = AvaView::new();
    (v.w, v.h) = (1400, 1000);
    v.sync(&mut s, None);
    let erstes_los = v.los;
    let ganz = v.zeilen.clone();
    let fs = erstes(&s, Category::StripFooting);
    let nr = s.model().element(fs).unwrap().number.clone();
    let mut p = Picking::default();
    waehle(&mut v, &mut s, &mut p, &[fs]);
    let f = Fokus::neu(s.model(), &[fs]);
    let lv = v.lv.clone().unwrap();
    let b = s.kostenblatt(None, &v.leiste.umfang);
    let soll: Vec<String> = verknuepft(&lv, &b, &f)
        .iter()
        .map(|p| p.oz.clone())
        .collect();
    assert!(!soll.is_empty());
    assert_eq!(positionen(&v), soll);
    // Erdarbeiten (Automatikmengen an der Platte) gehören dazu
    let erde = lv
        .titel
        .iter()
        .flat_map(|t| &t.positionen)
        .filter(|x| soll.contains(&x.oz))
        .any(|x| x.kurztext.contains("Baugrube"));
    assert!(erde, "{soll:?}");
    // Titel nur über verknüpften Positionen, ohne Summe
    for (i, z) in v.zeilen.iter().enumerate() {
        if z.art == Art::Titel {
            assert!(z.gp.is_empty(), "{}", z.text);
            assert_eq!(v.zeilen.get(i + 1).map(|x| x.art), Some(Art::Position));
        }
    }
    let text = v.fokus_text().unwrap().to_string();
    assert!(
        text.starts_with(&format!("Verknüpft mit {nr} · {} Position", soll.len())),
        "{text}"
    );
    assert!(text.contains(&format!("LV {}", lv.kopf.los)), "{text}");
    // „Alle Positionen“
    let t = Theme::dark();
    let fonts = Fonts {
        regular: None,
        bold: None,
        italic: None,
    };
    let (x, y, w, h) = v.fokus_verweis(&t, &fonts).expect("Verweis");
    let mitte = ((x + w * 0.5) as f64, (y + h * 0.5) as f64);
    let mods = sk_platform::Modifiers::default();
    assert_eq!(
        v.mouse_down(&t, &fonts, &mut p, mitte, mods),
        Some(ListOut::Repaint)
    );
    v.sync(&mut s, None);
    assert_eq!(v.fokus_text(), None);
    if v.los == erstes_los {
        assert_eq!(v.zeilen, ganz);
    }
    assert_eq!(p.selected, vec![fs]);
}

#[test]
fn a341_wand_und_loswechsel() {
    let mut s = haus();
    let mut v = AvaView::new();
    (v.w, v.h) = (1400, 1000);
    v.sync(&mut s, None);
    let aw = erstes(&s, Category::ExteriorWall);
    let mut p = Picking::default();
    waehle(&mut v, &mut s, &mut p, &[aw]);
    let ps = positionen(&v);
    assert!(!ps.is_empty(), "Mauerwerk im gezeigten Los");
    for z in v.zeilen.iter().filter(|z| z.art == Art::Position) {
        assert!(z.elemente.contains(&aw), "{}", z.text);
    }
    // Ein Bauteil, das nur in einem anderen Los rechnet: Dachterrasse
    let Some(dt) = s
        .model()
        .elements()
        .iter()
        .find(|(_, e)| e.category == Category::RoofTerrace)
        .map(|(id, _)| id)
    else {
        return;
    };
    let los_vorher = v.los;
    waehle(&mut v, &mut s, &mut p, &[dt]);
    let lv = v.lv.clone().unwrap();
    let b = s.kostenblatt(None, &v.leiste.umfang);
    let f = Fokus::neu(s.model(), &[dt]);
    if !verknuepft(&lv, &b, &f).is_empty() {
        assert!(!positionen(&v).is_empty());
        if v.los != los_vorher {
            assert_eq!(v.ansicht, Ansicht::Lv);
        }
    }
}

#[test]
fn a341_klick_in_der_tabelle_isoliert_nicht() {
    let mut s = haus();
    let mut v = AvaView::new();
    (v.w, v.h) = (1400, 1000);
    v.sync(&mut s, None);
    let ganz = v.zeilen.clone();
    // Wie der Klick auf eine Position: das Blatt setzt die Auswahl selbst
    let z = ganz.iter().find(|z| z.art == Art::Position).unwrap();
    let p = Picking {
        selected: z.elemente.clone(),
        ..Picking::default()
    };
    v.selected = p.selected.clone();
    v.follow(&p);
    v.sync(&mut s, None);
    assert_eq!(v.fokus_text(), None);
    assert_eq!(v.zeilen, ganz);
}
