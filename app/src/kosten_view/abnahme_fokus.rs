//! Abnahme A338–A340 „Bauteil ↔ Kosten“ (Jörn 10.10.) an RH-1, Reiter
//! Kosten:
//! - A338: Eine Auswahl im Modell isoliert die Liste auf die Positionen,
//!   die am Bauteil hängen, mit dessen Anteil an Menge und Betrag; am
//!   Fundament auch die Erdarbeiten und die Bauvorbereitung, die an der
//!   Sohlplatte rechnen. Vollständig: Keine verknüpfte Position fehlt.
//! - A339: Die Leiste nennt Bauteil, Zahl der Positionen und die Summe der
//!   Anteile; jede Gruppe ist die Summe ihrer Zeilen, in jeder Gliederung
//!   und beiden Modi, ohne Rundungsausgleich. „Alle Positionen“ und eine
//!   leere Auswahl zeigen wieder die ganze Liste am alten Rollstand.
//! - A340: Rückwärts: Ein Klick in der Liste wählt die Bauteile der Zeile,
//!   bei den Erdarbeiten die ganze Gründung, und isoliert die Liste nicht.

use super::*;
use sk_cost::rechnung::Ansatz;
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

fn ansicht(s: &mut Scene) -> KostenView {
    let mut v = KostenView::new();
    (v.w, v.h) = (1200, 1400);
    v.sync(s, None);
    v
}

/// Auswahl von außen: wie `QuantityWindow::sync_mit` erst folgen, dann
/// angleichen.
fn waehle(v: &mut KostenView, s: &mut Scene, p: &mut Picking, auswahl: &[ElementId]) {
    p.selected = auswahl.to_vec();
    v.follow(p);
    v.sync(s, None);
}

/// Positionen der Liste (Indizes im Kostenblatt).
fn positionen(v: &KostenView) -> Vec<usize> {
    let mut out: Vec<usize> = v
        .zeilen()
        .iter()
        .filter(|z| matches!(z.art, Art::Position { .. }))
        .filter_map(|z| z.pos.map(|p| p.0))
        .collect();
    out.sort();
    out.dedup();
    out
}

/// Betrag „1.234,56“ in Cent.
fn cent(text: &str) -> i64 {
    text.replace(['.', ','], "").parse().unwrap()
}

/// Anteil der Position am Fokus, unabhängig von der Gliederung gerechnet.
fn anteil(b: &Kostenblatt, i: usize, trifft: &dyn Fn(&Ansatz) -> bool) -> Option<(Dez, Cent)> {
    let p = &b.positionen[i];
    let (_, menge) = p.teile(|a| trifft(a)).into_iter().find(|t| t.0)?;
    Some((menge, p.gp_von(menge, false)))
}

#[test]
fn a338_fundament_zeigt_seine_positionen_mit_erdarbeiten() {
    let mut s = haus();
    let mut v = ansicht(&mut s);
    let alle = positionen(&v);
    let fs = erstes(&s, Category::StripFooting);
    let platte = erstes(&s, Category::GroundSlab);
    let mut p = Picking::default();
    waehle(&mut v, &mut s, &mut p, &[fs]);
    let b = v.blatt().unwrap().clone();
    let trifft = |a: &Ansatz| a.element == fs || (a.formel.is_some() && a.element == platte);
    let soll: Vec<usize> = (0..b.positionen.len())
        .filter(|&i| b.positionen[i].ansatz.iter().any(trifft))
        .collect();
    let ist = positionen(&v);
    assert_eq!(ist, soll, "genau die verknüpften Positionen");
    assert!(ist.len() < alle.len(), "isoliert");
    // Erdarbeiten und Bauvorbereitung (Automatikmengen der Platte) sind dabei
    let auto = ist
        .iter()
        .filter(|&&i| b.positionen[i].ansatz.iter().any(|a| a.formel.is_some()))
        .count();
    assert!(auto >= 5, "Erdarbeiten am Fundament: {auto}");
    assert!(
        v.zeilen().iter().any(|z| z.text.contains("Baugrube")),
        "{:?}",
        v.zeilen().iter().map(|z| &z.text).collect::<Vec<_>>()
    );
    // Je Zeile der Anteil des Fundaments: Menge und Betrag
    for z in v
        .zeilen()
        .iter()
        .filter(|z| matches!(z.art, Art::Position { .. }))
    {
        let (i, menge) = z.pos.unwrap();
        let (m, gp) = anteil(&b, i, &trifft).unwrap();
        assert_eq!(menge, m, "{}", z.text);
        assert_eq!(z.betrag, Some(gp), "{}", z.text);
    }
    // Die Wand gehört nicht dazu
    let aw = erstes(&s, Category::ExteriorWall);
    assert!(v.zeilen().iter().all(|z| !z.elements.contains(&aw)));
}

#[test]
fn a338_wand_zeigt_nur_ihren_anteil() {
    let mut s = haus();
    let mut v = ansicht(&mut s);
    let aw = erstes(&s, Category::ExteriorWall);
    let mut p = Picking::default();
    waehle(&mut v, &mut s, &mut p, &[aw]);
    let b = v.blatt().unwrap().clone();
    let ist = positionen(&v);
    assert!(!ist.is_empty());
    for &i in &ist {
        let pos = &b.positionen[i];
        assert!(pos.ansatz.iter().any(|a| a.element == aw), "{}", pos.kurz);
        assert!(
            pos.ansatz
                .iter()
                .all(|a| a.formel.is_none() || a.element != aw),
            "keine Erdarbeiten an der Wand"
        );
    }
    // Der Anteil ist kleiner als die ganze Position, wo mehrere Wände rechnen
    let geteilt = v
        .zeilen()
        .iter()
        .filter(|z| matches!(z.art, Art::Position { .. }))
        .any(|z| {
            let (i, m) = z.pos.unwrap();
            m < b.positionen[i].menge
        });
    assert!(geteilt, "Anteil der einen Wand");
}

#[test]
fn a339_leiste_summen_und_zurueck() {
    let mut s = haus();
    let mut v = ansicht(&mut s);
    v.h = 700;
    v.sync(&mut s, None);
    let ganz = v.zeilen().to_vec();
    // Weit unten gerollt, dann isolieren: oben; zurück: wieder unten
    v.scroll = 200.0;
    v.clamp();
    let gerollt = v.scroll;
    assert!(gerollt > 0.0);
    let fs = erstes(&s, Category::StripFooting);
    let nr = s.model().element(fs).unwrap().number.clone();
    let mut p = Picking::default();
    waehle(&mut v, &mut s, &mut p, &[fs]);
    assert_eq!(v.scroll, 0.0);
    let text = v.fokus_text().expect("Leiste").to_string();
    let n = positionen(&v).len();
    assert!(
        text.starts_with(&format!("Verknüpft mit {nr} · {n} Positionen · ")),
        "{text}"
    );
    for modus in Modus::ALLE {
        for g in Gliederung::ALLE {
            v.modus = modus;
            v.gliederung = g;
            v.sync(&mut s, None);
            let z = v.zeilen();
            assert!(z.iter().all(|x| x.art != Art::Ausgleich), "{g:?}");
            for (i, x) in z.iter().enumerate() {
                if x.art != Art::Gruppe {
                    continue;
                }
                let kinder: i64 = z[i + 1..]
                    .iter()
                    .take_while(|y| y.ebene > x.ebene)
                    .filter(|y| y.ebene == x.ebene + 1)
                    .filter_map(|y| y.betrag)
                    .map(|c| c.0)
                    .sum();
                assert_eq!(
                    x.betrag.map_or(0, |c| c.0),
                    kinder,
                    "{modus:?} {g:?} {}",
                    x.text
                );
            }
            let oben: i64 = z
                .iter()
                .filter(|x| x.art == Art::Gruppe && x.ebene == 0)
                .filter_map(|x| x.betrag)
                .map(|c| c.0)
                .sum();
            let text = v.fokus_text().unwrap();
            let euro = text.rsplit(" · ").next().unwrap().trim_end_matches(" €");
            assert_eq!(cent(euro), oben, "{modus:?} {g:?} {text}");
        }
    }
    v.modus = Modus::Voll;
    v.gliederung = Gliederung::Gewerk;
    // „Alle Positionen“: ganze Liste am alten Rollstand, Auswahl bleibt
    let t = Theme::dark();
    let fonts = Fonts {
        regular: None,
        bold: None,
        italic: None,
    };
    let (x, y, w, h) = v.fokus_verweis(&t, &fonts).expect("Verweis");
    let mitte = ((x + w * 0.5) as f64, (y + h * 0.5) as f64);
    assert_eq!(v.hit(&t, &fonts, mitte.0, mitte.1), Some(Hot::Alle));
    let mods = sk_platform::Modifiers::default();
    assert_eq!(
        v.mouse_down(&t, &fonts, &mut p, mitte, mods),
        Some(ListOut::Repaint)
    );
    v.sync(&mut s, None);
    assert_eq!(v.fokus_text(), None);
    assert_eq!(v.zeilen(), &ganz[..]);
    assert_eq!(v.scroll, gerollt);
    assert_eq!(p.selected, vec![fs], "die Auswahl bleibt");
    // Wieder isolieren und mit leerer Auswahl zurück
    let aw = erstes(&s, Category::ExteriorWall);
    waehle(&mut v, &mut s, &mut p, &[aw]);
    assert!(v.fokus_text().is_some());
    waehle(&mut v, &mut s, &mut p, &[]);
    assert_eq!(v.fokus_text(), None);
    assert_eq!(v.zeilen(), &ganz[..]);
}

#[test]
fn a339_ohne_verknuepfung_sagt_es() {
    let mut s = haus();
    let mut v = ansicht(&mut s);
    // Ein Raum hat keine Kostenposition
    let Some(raum) = s
        .model()
        .elements()
        .iter()
        .find(|(_, e)| e.category == Category::Space)
        .map(|(id, _)| id)
    else {
        return;
    };
    let mut p = Picking::default();
    waehle(&mut v, &mut s, &mut p, &[raum]);
    let text = v.fokus_text().unwrap();
    assert!(
        text.ends_with("ist hier keine Position verknüpft."),
        "{text}"
    );
    assert!(v.zeilen().is_empty());
}

#[test]
fn a340_klick_in_der_liste_waehlt_die_gruendung_und_isoliert_nicht() {
    let t = Theme::dark();
    let fonts = Fonts {
        regular: None,
        bold: None,
        italic: None,
    };
    let mut s = haus();
    let mut v = ansicht(&mut s);
    let fs = erstes(&s, Category::StripFooting);
    let platte = erstes(&s, Category::GroundSlab);
    let i = v
        .zeilen()
        .iter()
        .position(|z| z.text.contains("Baugrube"))
        .expect("Erdarbeiten");
    let z = &v.zeilen()[i];
    assert!(
        z.elements.contains(&platte) && z.elements.contains(&fs),
        "Erdarbeiten hängen an der ganzen Gründung"
    );
    v.springe(|x| x.text.contains("Baugrube"));
    let (_, y, h) = v
        .sichtbar()
        .into_iter()
        .find(|(j, _, _)| *j == i)
        .expect("sichtbar");
    let (x0, _) = v.content_x(&t);
    let mut p = Picking::default();
    let mods = sk_platform::Modifiers::default();
    let klick = ((x0 + 60.0) as f64, (y + h * 0.5) as f64);
    assert_eq!(
        v.mouse_down(&t, &fonts, &mut p, klick, mods),
        Some(ListOut::Picking { selection: true })
    );
    assert!(p.selected.contains(&fs) && p.selected.contains(&platte));
    let vorher = v.zeilen().to_vec();
    v.follow(&p);
    v.sync(&mut s, None);
    assert_eq!(v.fokus_text(), None, "eigener Klick isoliert nicht");
    assert_eq!(v.zeilen(), &vorher[..]);
}
