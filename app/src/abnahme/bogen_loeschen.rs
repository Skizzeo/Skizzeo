use super::*;

// Abnahmetest A137: Geschossbogen nach „Gebäude löschen“ (Nacharbeit zu
// 7fa905b, Koordinator 07.10. 03:04). Der Bogen zeigt dieselben Geschosse wie
// das Paneel „Geschosse“, auch wenn nur noch die Vorlage steht (Ist-Bild
// loeschen-3c: Paneel mit OG, Bogen oben ausgegraut). Ist ein Bauteil gewählt,
// steht der Bogen links neben „Eigenschaften“.
// Spezifikation: test/abnahme-loeschen.md (A137).
//
// Einbau: als `mod bogen_loeschen { use super::*; … }` ans Ende von
// app/src/abnahme.rs. Nutzt aus abnahme.rs die E18-Adapter (bogen,
// bogen_text, spitze_frei, spitze_klick, tick, bogen_lage, bogen_rechts),
// haus_b11 und test_dir. Eigener Adapter nur `gebaeude_loeschen`, wie in
// `mod loeschen`.

use sk_model::{BuildingId, Model};

// ===== Adapter =====

fn gebaeude_loeschen(s: &mut Scene, b: BuildingId) -> bool {
    s.remove_building(b)
}

// ===== Hilfen =====

/// Bogen und Paneel zeigen dasselbe: Das aktive Geschoss in der Mitte ist das
/// aktive Band, die Spitzen nennen die Nachbarbänder im Paneel (oder sind
/// ausgegraut, wenn das Paneel dort keines hat).
fn wie_paneel(w: &crate::wheel::Wheel, s: &Scene, ctx: &str) {
    let l = s.levels();
    let i = l
        .bands
        .iter()
        .position(|b| b.active)
        .unwrap_or_else(|| panic!("{ctx}: kein aktives Band"));
    let name = |k: Option<usize>| k.and_then(|k| l.bands.get(k)).map(|b| b.name.clone());
    let ((mitte, _), oben, unten) = bogen_text(w, s);
    assert_eq!(Some(mitte), name(Some(i)), "{ctx}: Mitte");
    assert_eq!(oben, name(Some(i + 1)), "{ctx}: Spitze oben");
    assert_eq!(unten, name(i.checked_sub(1)), "{ctx}: Spitze unten");
    assert_eq!(spitze_frei(w, s, true, false), oben.is_some(), "{ctx}");
    assert_eq!(spitze_frei(w, s, false, false), unten.is_some(), "{ctx}");
}

fn erstes_gebaeude(s: &Scene) -> BuildingId {
    s.model()
        .buildings()
        .iter()
        .next()
        .map(|(id, _)| id)
        .unwrap()
}

// ===== Tests =====

/// A137: Bogen und Paneel zeigen dieselben Geschosse: ohne Gebäude, mit
/// Gebäude, nach „Gebäude löschen“ (Vorlage mit OG: Spitze oben „OG“, frei,
/// ein Klick führt ins OG), nach Rückgängig, Wiederholen und nach Speichern
/// und Öffnen der Vorlage. Mit gewähltem Bauteil liegt der ganze Bogen samt
/// Schild links von „Eigenschaften“ (Abstand panel_margin), ohne Auswahl
/// wieder am rechten Rand; drei Fenstergrößen.
#[test]
fn a137_bogen_wie_paneel_nach_gebaeude_loeschen() {
    let th = Theme::dark();
    let mut w = bogen(&th);
    let s = Scene::with_model(Model::with_seed(137));
    wie_paneel(&w, &s, "ohne Gebäude");

    let mut s = Scene::with_model(Model::with_seed(137));
    haus_b11(&mut s);
    wie_paneel(&w, &s, "mit Gebäude");
    let b = erstes_gebaeude(&s);
    assert!(gebaeude_loeschen(&mut s, b));
    assert!(
        s.levels().bands.iter().any(|b| b.name == "OG"),
        "Paneel zeigt das OG der Vorlage"
    );
    wie_paneel(&w, &s, "nach Gebäude löschen");
    assert_eq!(bogen_text(&w, &s).1.as_deref(), Some("OG"));
    spitze_klick(&mut w, &mut s, true, false, 0);
    tick(&mut w, &mut s, 1000);
    assert_eq!(bogen_text(&w, &s).0 .0, "OG", "Spitze führt ins OG");
    wie_paneel(&w, &s, "Vorlage im OG");
    assert!(s.undo());
    wie_paneel(&w, &s, "Rückgängig");
    assert!(s.redo());
    wie_paneel(&w, &s, "Wiederholen");
    let text = sk_model::szo::write(s.model());
    let p = test_dir("a137").join("vorlage.szo");
    std::fs::write(&p, text).unwrap();
    let l = crate::document::load(&p).expect("öffnet");
    let s2 = Scene::with_model(l.model);
    wie_paneel(&w, &s2, "Vorlage gespeichert und geöffnet");

    // Ausweichen neben „Eigenschaften“
    let mut s = Scene::with_model(Model::with_seed(137));
    let (aw, _) = haus_b11(&mut s);
    let el = s.model().wall_at(aw, 0).unwrap();
    let mut ui = Ui::new(1.0, &th);
    for (bw, bh) in [(1280u32, 800u32), (1440, 900), (1920, 1080)] {
        ui.fit(1.0, bw, bh);
        ui.set_props(None);
        // Befund ac (Nachtrag 2): links neben der rechten Spalte
        let views = ui.rect(Panel::Views, bw, 32);
        let frei = bogen_rechts(&w, &s, &ui, bw, bh);
        assert!(
            (frei - (views.x - th.size.panel_margin)).abs() < 1.0,
            "{bw}: ohne Auswahl links der Spalte"
        );
        ui.set_props(crate::selection::props(&s, el));
        assert!(ui.has_props());
        let props = ui.rect(Panel::Props, bw, 32);
        let rechts = bogen_rechts(&w, &s, &ui, bw, bh);
        assert!(
            rechts <= props.x - th.size.panel_margin + 1.0,
            "{bw}: Bogen {rechts} reicht unter Eigenschaften ({})",
            props.x
        );
        let (cx, _, r) = bogen_lage(&w, &ui, bw, bh);
        assert!(cx + r < rechts, "{bw}: Bogen links vom Schild");
    }
}
