use super::*;

// Abnahmetests A199 und A199b: Auswahlumriss von Dachterrasse und
// Attikablech; Umriss der Decken in 3D und im Schnitt (zweiter Fehler,
// Koordinator 11:53)
// (Fehler auf main 818a6b0: selection.rs:876–892 zeichnete für DT und AB
// das Prisma der Frostschürze; paket-4-baumpanel.md §2.4).
// Spezifikation: test/abnahme-baumpanel.md.
//
// Einbau: als `mod umriss_dt_ab { use super::*; … }` ans Ende von
// app/src/abnahme.rs. Nutzt aus abnahme.rs: gebaeude, decke.
// Keine angenommenen Namen: `selection::helpers` wie auf main.

use sk_model::ElementId;

/// (xmin, ymin, zmin, xmax, ymax, zmax) aller Umrisslinien in 3D.
fn umriss(s: &Scene, id: ElementId) -> [f32; 6] {
    let h = selection::helpers(s, id, ViewKind::Persp, None, 1.0, &Theme::dark());
    assert!(!h.is_empty(), "Umriss fehlt");
    let mut r = [f32::MAX, f32::MAX, f32::MAX, f32::MIN, f32::MIN, f32::MIN];
    for p in h.iter().flat_map(|x| [x.a, x.b]) {
        for k in 0..3 {
            r[k] = r[k].min(p[k]);
            r[k + 3] = r[k + 3].max(p[k]);
        }
    }
    r
}

fn nr(s: &Scene, nummer: &str) -> ElementId {
    s.model()
        .elements()
        .iter()
        .find(|(_, e)| e.number == nummer)
        .map(|(id, _)| id)
        .unwrap_or_else(|| panic!("{nummer} fehlt"))
}

/// A199 (paket-4 §2.4): Prüfhaus 10 × 8 m, AW-31,5, OG-Nordwand AW-006
/// gelöst und 1,50 m zurück. Der Umriss von DT-001 liegt zwischen OK
/// Rohdecke +2,855 und OK Belag +2,995 und im Grundriss auf dem
/// Terrassenstreifen (x 140–9860, y 6500–7860). Der Umriss von AB-001 ist
/// ein Band über der Attikakrone: über OK Belag, bis wenig über OK Attika
/// +3,055, am Nordrand und an den Rückläufen bis zur OG-Wand. Keiner der beiden Umrisse reicht unter ±0,00 (das
/// war die Frostschürze).
#[test]
fn a199_umriss_dachterrasse_und_attikablech() {
    let mut s = Scene::with_model(Model::with_seed(199));
    let (eg, _) = gebaeude(&mut s);
    let w = nr(&s, "AW-006");
    assert!(s.edit_model("Kopplung gelöst", |m| m.set_linked(w, false)));
    assert!(s.edit_model("Wand verschoben", |m| m.move_segment(w, -1500.0).is_some()));
    let de = decke(&s, eg).unwrap();
    let dt = s.model().terrace_of(de).expect("DT");
    let ab = s.model().coping_of(de).expect("AB");

    let [x0, y0, z0, x1, y1, z1] = umriss(&s, dt);
    assert!(
        (z0 - 2855.0).abs() < 1.0 && (z1 - 2995.0).abs() < 1.0,
        "DT z {z0}–{z1}"
    );
    assert!(x0 >= 139.0 && x1 <= 9861.0, "DT x {x0}–{x1}");
    assert!(y0 >= 6499.0 && y1 <= 7861.0, "DT y {y0}–{y1}");

    // AB läuft am Nordrand und an beiden Rückläufen bis zur OG-Wand
    // (Körper auf 818a6b0: x −20–10020, y 6500–8020, z 3005–3066)
    let [x0, y0, z0, x1, y1, z1] = umriss(&s, ab);
    assert!(
        z0 >= 2995.0 && (3055.0..=3155.0).contains(&z1),
        "AB z {z0}–{z1}"
    );
    assert!(x0 >= -50.0 && x1 <= 10050.0, "AB x {x0}–{x1}");
    assert!(
        y0 >= 6499.0 && (7990.0..=8050.0).contains(&y1),
        "AB y {y0}–{y1}"
    );
}

/// Umriss im Schnitt A (nur der Teil hinter der Ebene): (zmin, zmax).
fn umriss_schnitt(s: &Scene, id: ElementId) -> (f32, f32) {
    let mut sect = SectionLine::default();
    sect.ensure(s);
    let h = selection::helpers(s, id, ViewKind::Section, sect.plane(), 1.0, &Theme::dark());
    assert!(!h.is_empty(), "Umriss im Schnitt fehlt");
    h.iter()
        .flat_map(|x| [x.a[2], x.b[2]])
        .fold((f32::MAX, f32::MIN), |(a, b), z| (a.min(z), b.max(z)))
}

/// A199b (Koordinator 11:53, zweiter Fehler): Der Auswahlumriss einer
/// Decke umfasst in 3D und im Schnitt die ganze Dicke, er wird nicht auf
/// die Schnitthöhe des Grundrisses gekürzt. Standardhaus bündig: DE-001
/// von +2,635 bis +2,855, DE-002 von +5,490 bis +5,710 (je 22 cm),
/// in der Fläche bis zur Außenkante des Kerns (x und y innerhalb des
/// Hauses, mindestens 9 × 7 m).
#[test]
fn a199b_umriss_decke_ganze_dicke() {
    let mut s = Scene::with_model(Model::with_seed(1992));
    gebaeude(&mut s);
    for (n, unten, oben) in [("DE-001", 2635.0, 2855.0), ("DE-002", 5490.0, 5710.0)] {
        let de = nr(&s, n);
        let [x0, y0, z0, x1, y1, z1] = umriss(&s, de);
        assert!(
            (z0 - unten).abs() < 1.0 && (z1 - oben).abs() < 1.0,
            "{n} 3D z {z0}–{z1}"
        );
        assert!(
            x1 - x0 >= 9000.0 && y1 - y0 >= 7000.0,
            "{n} Fläche {x0}–{x1} × {y0}–{y1}"
        );
        assert!(
            x0 >= -1.0 && x1 <= 10001.0 && y0 >= -1.0 && y1 <= 8001.0,
            "{n} im Haus"
        );
        let (a, b) = umriss_schnitt(&s, de);
        assert!(
            (a - unten).abs() < 1.0 && (b - oben).abs() < 1.0,
            "{n} Schnitt z {a}–{b}"
        );
    }
}
