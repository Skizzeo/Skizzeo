use super::*;

// Abnahmetests A196–A197: Profilextrusion und Attikablech AB (Commit 2c;
// geometrie/machbarkeit-dachterrasse.md D0b/D3, Review R7;
// bim/paket-dachterrasse.md E3/E4, Regel 46, Sollwerte 08:54; Steckbrief
// bauteile/ab-attikablech.md). Spezifikation:
// test/abnahme-dachterrasse.md. Setzt 2a, 2b, 1a und
// test/patches/element-bounds.patch voraus.
//
// Einbau: als `mod attikablech { use super::*; … }` ans Ende von
// app/src/abnahme.rs. Nutzt aus abnahme.rs: gebaeude, decke, r4.
//
// Fassung 2 (09:45): Extrusion über `Solid::sweep` wie auf main seit
// 952a6fa. Angenommene Namen nur in den Adaptern: `Model::coping_of`,
// `Scene::coping_qto` mit `length`, Merkmal „Abwicklung“ über
// `Model::props_of`, `Category::Coping`.

use sk_math::Vec3;
use sk_model::{Category, ElementId, PropValue, RunId, Solid};

// ===== Adapter 2c =====

/// Profil (quer rechts der Laufrichtung, hoch) in mm längs `pfad`
/// ziehen, Ecken auf Gehrung, offene Enden gerade (D0b, auf main seit
/// 952a6fa).
fn extrusion(pfad: &[Vec3], geschlossen: bool, profil: &[(f64, f64)]) -> Solid {
    let mut k = Solid::default();
    k.sweep(
        pfad,
        geschlossen,
        profil,
        [sk_model::solid::SweepEnd::Square; 2],
    );
    k
}

fn ab(s: &Scene, de: ElementId) -> Option<ElementId> {
    s.model().coping_of(de)
}

/// Länge an der Außenkante in m (2 Stellen).
fn ab_laenge(s: &Scene, id: ElementId) -> f64 {
    (s.coping_qto(id).expect("Mengen AB").length / 10.0).round() / 100.0
}

fn abwicklung(s: &Scene, id: ElementId) -> f64 {
    match s.model().props_of(id).get("Abwicklung") {
        Some(PropValue::Number(v)) => *v,
        x => panic!("Abwicklung: {x:?}"),
    }
}

// ===== Hilfen =====

/// Rauminhalt eines geschlossenen Körpers (mm³) über die
/// Dreiecksnormalen; positiv, wenn die Flächen nach außen zeigen.
fn volumen(k: &Solid) -> f64 {
    k.triangles
        .iter()
        .map(|t| t.p[0].dot(t.p[1].cross(t.p[2])) / 6.0)
        .sum()
}

fn nr(s: &Scene, nummer: &str) -> ElementId {
    s.model()
        .elements()
        .iter()
        .find(|(_, e)| e.number == nummer)
        .map(|(id, _)| id)
        .unwrap_or_else(|| panic!("{nummer} fehlt"))
}

fn pruefung(s: &Scene) {
    assert!(s.model().check().is_empty(), "{:?}", s.model().check());
}

fn versetzen(s: &mut Scene, wand: &str, d: f64) {
    let w = nr(s, wand);
    if s.model().stack_offset(w).is_some_and(|(_, l)| l) {
        assert!(s.edit_model("Kopplung gelöst", |m| m.set_linked(w, false)));
    }
    assert!(s.edit_model("Wand verschoben", |m| m.move_segment(w, d).is_some()));
}

fn pruefhaus(seed: u64, rueck: &[&str]) -> (Scene, RunId) {
    let mut s = Scene::with_model(Model::with_seed(seed));
    let (eg, _) = gebaeude(&mut s);
    for w in rueck {
        versetzen(&mut s, w, -1500.0);
    }
    pruefung(&s);
    (s, eg)
}

const QUADRAT: [(f64, f64); 4] = [(-50.0, 0.0), (50.0, 0.0), (50.0, 100.0), (-50.0, 100.0)];

/// A196 (D0b, R7): Profilextrusion. Rechteck 10 × 8 m geschlossen, Profil
/// 100 × 100 mittig auf dem Pfad: Rauminhalt = 100 × 100 × 36 000 mm
/// (Gehrungen heben sich auf), Flächen nach außen. Offener Winkel 10 + 8
/// m mit geraden Enden: 100 × 100 × 18 000. Spitzer Winkel 20°: endlicher,
/// positiver Körper innerhalb eines vernünftigen Rahmens (Gehrung
/// begrenzt). Kurzes Segment 50 mm: kein Fehler.
#[test]
fn a196_profilextrusion() {
    let rechteck = [
        vec3(0.0, 0.0, 0.0),
        vec3(10000.0, 0.0, 0.0),
        vec3(10000.0, 8000.0, 0.0),
        vec3(0.0, 8000.0, 0.0),
    ];
    for pfad in [rechteck.to_vec(), rechteck.iter().rev().copied().collect()] {
        let k = extrusion(&pfad, true, &QUADRAT);
        assert!((volumen(&k) - 1e4 * 36000.0).abs() < 1e3, "{}", volumen(&k));
        let (lo, hi) = k.bounds().unwrap();
        assert_eq!((lo.z, hi.z), (0.0, 100.0));
        assert_eq!((lo.x.round(), hi.x.round()), (-50.0, 10050.0));
    }
    let winkel = [
        vec3(0.0, 0.0, 0.0),
        vec3(10000.0, 0.0, 0.0),
        vec3(10000.0, 8000.0, 0.0),
    ];
    let k = extrusion(&winkel, false, &QUADRAT);
    assert!(
        (volumen(&k).abs() - 1e4 * 18000.0).abs() < 1e3,
        "{}",
        volumen(&k)
    );

    let a = 20f64.to_radians();
    let spitz = [
        vec3(0.0, 0.0, 0.0),
        vec3(10000.0, 0.0, 0.0),
        vec3(10000.0 - 10000.0 * a.cos(), 10000.0 * a.sin(), 0.0),
    ];
    let k = extrusion(&spitz, false, &QUADRAT);
    let (lo, hi) = k.bounds().unwrap();
    assert!(lo.x.is_finite() && hi.x.is_finite() && volumen(&k).abs() > 0.0);
    assert!(hi.x < 10000.0 + 1000.0, "Gehrungsspitze begrenzt: {}", hi.x);

    let kurz = [
        vec3(0.0, 0.0, 0.0),
        vec3(50.0, 0.0, 0.0),
        vec3(50.0, 5000.0, 0.0),
    ];
    assert!(!extrusion(&kurz, false, &QUADRAT).is_empty());
}

/// A197 (D3, E3/E4, Regel 46, Sollwerte 08:54): Nord −1,50 ergibt
/// AB-001 auf DE-001: Länge an der Außenkante 13,00 m (10,00 + 2 × 1,50),
/// Abwicklung 250 mm (140 + 20 + 50 + 40), Gewerk 18338 über die
/// eingebaute Schicht (der Baustoff Titanzink schlägt 18339 vor), KG 363,
/// AB / IfcCovering COPING. Das Blech sitzt auf OK Attika +3,055, keine
/// Kante unter OK Belag +2,995. Nord+Ost 21,00 m, Nord+Süd 26,00 m,
/// ringsum 36,00 m (geschlossen). Wechsel auf Vorsprung: AB weg; zurück:
/// neue Nummer; Rückgängig bringt AB-001 zurück. Löschen abgelehnt.
/// Datei: `[coping] number="AB-001" floor=<Guid DE-001>`, Rundlauf
/// bytegleich, fehlende Zeile wird mit Hinweis ergänzt.
#[test]
fn a197_attikablech() {
    let (mut s, eg) = pruefhaus(197, &["AW-006"]);
    let de = decke(&s, eg).unwrap();
    let id = ab(&s, de).expect("AB");
    let m = s.model();
    assert_eq!(m.element(id).unwrap().number, "AB-001");
    let c = Category::Coping;
    assert_eq!(
        (c.prefix(), c.ifc_class(), c.din276()),
        ("AB", "IfcCovering.COPING", Some(363))
    );
    assert_eq!(ab_laenge(&s, id), 13.0);
    // Tropfkante 40 statt 20 (Jörn 10.10., Plan Flachdach P12)
    assert_eq!(abwicklung(&s, id), 270.0);
    let gw = m
        .layer_trade(id, 0)
        .and_then(|t| m.trade(t))
        .map(|t| t.code.clone());
    assert_eq!(gw.as_deref(), Some("18338"));
    assert_eq!(m.layer_kg(id, 0), Some(363));
    let (lo, hi) = s.element_bounds(id).expect("Körper");
    assert!(hi.z >= 3055.0 && hi.z < 3055.0 + 100.0, "{}", hi.z);
    assert!(
        lo.z > 2995.0 && lo.z < 3055.0,
        "Schenkel enden über dem Belag: {}",
        lo.z
    );
    pruefung(&s);

    for (seed, rueck, laenge) in [
        (1970, &["AW-006", "AW-007"][..], 21.0),
        (1971, &["AW-006", "AW-008"][..], 26.0),
        (1972, &["AW-005", "AW-006", "AW-007", "AW-008"][..], 36.0),
    ] {
        let (s, eg) = pruefhaus(seed, rueck);
        let id = ab(&s, decke(&s, eg).unwrap()).expect("AB");
        assert_eq!(ab_laenge(&s, id), laenge, "{rueck:?}");
        assert_eq!(
            s.model()
                .elements()
                .iter()
                .filter(|(_, e)| e.category == Category::Coping)
                .count(),
            1,
            "ein AB je Decke"
        );
    }

    // Wechsel und Rückgängig (E3)
    let g1 = s.model().element(id).unwrap().guid;
    versetzen(&mut s, "AW-006", 1800.0);
    assert!(ab(&s, de).is_none(), "Vorsprung: kein AB");
    versetzen(&mut s, "AW-006", -1800.0);
    let id2 = ab(&s, de).expect("AB neu");
    assert_ne!(s.model().element(id2).unwrap().number, "AB-001");
    assert!(s.undo());
    assert!(s.undo());
    let id = ab(&s, de).expect("AB-001 zurück");
    assert_eq!(
        (
            s.model().element(id).unwrap().guid,
            s.model().element(id).unwrap().number.as_str()
        ),
        (g1, "AB-001")
    );

    let r = s.model().can_delete(id).expect_err("nicht löschbar");
    assert_eq!(
        sk_model::refusal_text(s.model(), id, &r),
        "Das Attikablech folgt der Dachterrasse. Seinen Baustoff stellst du im Paneel ein."
    );

    let text = sk_model::szo::write(s.model());
    let gde = s.model().element(de).unwrap().guid.to_string();
    let z: Vec<&str> = text.lines().filter(|l| l.starts_with("[coping]")).collect();
    assert_eq!(z.len(), 1);
    assert!(z[0].contains("number=\"AB-001\"") && z[0].contains(&format!("floor={gde}")));
    let lesen = |t: &str| sk_model::szo::read(t, sk_model::GuidGen::with_seed(1)).expect("öffnet");
    assert_eq!(sk_model::szo::write(&lesen(&text).model), text, "Rundlauf");
    let ohne: String = text
        .lines()
        .filter(|l| !l.starts_with("[coping]"))
        .map(|l| format!("{l}\n"))
        .collect();
    let geladen = lesen(&ohne);
    assert!(!geladen.hints.is_empty());
    assert_eq!(
        geladen
            .model
            .elements()
            .iter()
            .filter(|(_, e)| e.category == Category::Coping)
            .count(),
        1
    );
    assert!(geladen.model.check().is_empty());
}
