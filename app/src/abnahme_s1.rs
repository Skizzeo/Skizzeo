//! Abnahme Sonnenstand S1 (Lage und Nordrichtung im Datenmodell) an den
//! drei Referenzhäusern, über den Speicherweg der App (`szo::write`,
//! `szo::read_with` mit den Kostenabschnitten) und die Schritte der Szene.

use crate::scene::Scene;
use sk_math::sonne::{sonnenstand, sonnenvektor_modell, Datum, Lage, Zeitpunkt};
use sk_model::{szo, GuidGen, Location};

const HAEUSER: [(&str, &str); 3] = [
    (
        "RH-1",
        include_str!("../../crates/sk-cost/referenz/rh1-standardhaus.szo"),
    ),
    (
        "RH-2",
        include_str!("../../crates/sk-cost/referenz/rh2-mehrschalig.szo"),
    ),
    (
        "RH-3",
        include_str!("../../crates/sk-cost/referenz/rh3-versatz-dachterrasse.szo"),
    ),
];

fn laden(text: &str) -> Result<szo::Loaded, szo::LoadError> {
    szo::read_with(text, GuidGen::with_seed(1), &sk_cost::lesen::ABSCHNITTE_SZO)
}

/// Ein Haus samt Lage in einem Schritt, wie über die Maske.
fn mit_lage(text: &str, l: Location) -> (Scene, String) {
    let mut s = Scene::with_model(laden(text).unwrap().model);
    let p = s.model().project().clone();
    assert!(s.projektdaten_setzen("Projektdaten geändert", p, l));
    let neu = szo::write(s.model());
    (s, neu)
}

/// Ohne Lage bleibt jedes Referenzhaus bytegleich; mit Lage genau eine
/// Zeile hinter den Projektdaten, Rundlauf bytegleich, Laden ohne
/// Revision, Strg+Z stellt die Ausgangsdatei her, Strg+Y die neue.
#[test]
fn abnahme_s1_referenzhaeuser_rundlauf() {
    for (name, text) in HAEUSER {
        let l0 = laden(text).unwrap();
        assert!(l0.model.location().is_unset(), "{name}");
        let alt = szo::write(&l0.model);
        assert!(!alt.contains("[location]"), "{name}");
        assert_eq!(szo::write(&laden(&alt).unwrap().model), alt, "{name}");

        let l = Location {
            lat: Some(52.520008),
            lon: Some(13.404954),
            north: Some(-17.25),
        };
        let (mut s, neu) = mit_lage(&alt, l);
        assert_eq!(s.undo_label(), Some("Projektdaten geändert"), "{name}");
        let zeilen: Vec<(usize, &str)> = neu
            .lines()
            .enumerate()
            .filter(|(_, z)| z.starts_with("[location]"))
            .collect();
        assert_eq!(zeilen.len(), 1, "{name}");
        assert_eq!(
            zeilen[0].1, "[location] lat=52.520008 lon=13.404954 north=342.75",
            "{name}"
        );
        // Direkt hinter [project] bzw. [projectinfo]
        let davor = neu.lines().nth(zeilen[0].0 - 1).unwrap();
        assert!(
            davor.starts_with("[project]") || davor.starts_with("[projectinfo]"),
            "{name}: vor [location] steht {davor}"
        );
        // Sonst nur diese eine Zeile mehr
        let ohne: String = neu
            .lines()
            .filter(|z| !z.starts_with("[location]"))
            .map(|z| format!("{z}\n"))
            .collect();
        assert_eq!(ohne, alt, "{name}");

        let back = laden(&neu).unwrap();
        assert!(back.hints.is_empty(), "{name}: {:?}", back.hints);
        assert_eq!(back.model.revision(), 0, "{name}");
        assert_eq!(back.model.location().north, Some(342.75), "{name}");
        assert_eq!(szo::write(&back.model), neu, "{name}");

        assert!(s.undo());
        assert_eq!(szo::write(s.model()), alt, "{name}: Strg+Z");
        assert!(s.redo());
        assert_eq!(szo::write(s.model()), neu, "{name}: Strg+Y");
    }
}

/// 400 zufällige Lagen, auch unvollständige und außerhalb des Bereichs:
/// gespeichert und geladen kommt genau die bereinigte Lage zurück, der
/// zweite Rundlauf ist bytegleich.
#[test]
fn abnahme_s1_zufaellige_lagen() {
    let mut x: u64 = 0x2545_F491_4F6C_DD1D;
    let mut zufall = || {
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        (x >> 11) as f64 / (1u64 << 53) as f64
    };
    let alt = szo::write(&laden(HAEUSER[0].1).unwrap().model);
    for i in 0..400 {
        let mut wert = |max: f64| {
            if zufall() < 0.25 {
                None
            } else {
                // bis 20 % über den Bereich hinaus, mit vielen Stellen
                Some((zufall() * 2.0 - 1.0) * max * 1.2)
            }
        };
        let l = Location {
            lat: wert(90.0),
            lon: wert(180.0),
            north: wert(800.0),
        };
        let soll = l.normalized();
        let mut s = Scene::with_model(laden(&alt).unwrap().model);
        let p = s.model().project().clone();
        let geaendert = s.projektdaten_setzen("Projektdaten geändert", p, l);
        assert_eq!(geaendert, !soll.is_unset(), "{i}: {l:?}");
        let neu = szo::write(s.model());
        assert_eq!(neu.contains("[location]"), !soll.is_unset(), "{i}");
        let back = laden(&neu).unwrap();
        assert!(back.hints.is_empty(), "{i}: {:?}", back.hints);
        let ist = *back.model.location();
        for (k, a, b) in [
            ("lat", ist.lat, soll.lat),
            ("lon", ist.lon, soll.lon),
            ("north", ist.north, soll.north),
        ] {
            match (a, b) {
                (None, None) => {}
                (Some(a), Some(b)) => assert!((a - b).abs() < 1e-9, "{i} {k}: {a} statt {b}"),
                _ => panic!("{i} {k}: {a:?} statt {b:?}"),
            }
        }
        if let Some(n) = ist.north {
            assert!((0.0..360.0).contains(&n), "{i}: north {n}");
        }
        assert_eq!(szo::write(&back.model), neu, "{i}");
    }
}

/// Fehler und Hinweise beim Laden; fremde Schlüssel an [location] und ein
/// fremder Abschnitt dahinter bleiben, solange die Zeile unverändert ist
/// (F-17b); der fremde Abschnitt auch danach.
#[test]
fn abnahme_s1_laden_fehler_hinweise_fremdes() {
    let alt = szo::write(&laden(HAEUSER[0].1).unwrap().model);
    let ein = |zeilen: &str| alt.replacen("[storey]", &format!("{zeilen}[storey]"), 1);
    let nr = |text: &str, z: &str| text.lines().position(|l| l.starts_with(z)).unwrap() + 1;

    let doppelt = ein("[location] lat=1\n[location] lat=2\n");
    let e = laden(&doppelt).err().expect("doppelt ist ein Fehler");
    assert!(
        e.to_string()
            .contains(&nr(&doppelt, "[location] lat=2").to_string()),
        "{e}"
    );
    // Befund A (Sonnenstand-Thread): Unlesbares öffnet das Projekt; die
    // Zeile zählt nicht, gibt einen Hinweis mit der Zeile und bleibt
    // bytegleich (an der Stelle, an die der Schreiber [location] setzt)
    for z in [
        "[location] lat=abc lon=8\n",
        "[location] lat=95 lon=8.5 north=10\n",
    ] {
        let kaputt = ein(z);
        let l = laden(&kaputt).expect("Unlesbares öffnet");
        assert_eq!(l.hints.len(), 1, "{:?}", l.hints);
        assert!(l.hints[0].contains("lat"), "{:?}", l.hints);
        assert!(
            l.hints[0].contains(&nr(&kaputt, "[location]").to_string()),
            "{:?}",
            l.hints
        );
        assert!(l.model.location().is_unset(), "{z}");
        let neu = szo::write(&l.model);
        assert_eq!(neu.matches(z).count(), 1, "{z}\n{neu}");
        assert_eq!(neu.matches("[location]").count(), 1, "{neu}");
        assert_eq!(szo::write(&laden(&neu).unwrap().model), neu, "{z}");
    }

    let fremd =
        ein("[location] lat=53 lon=8.5 hoehe_nn=12 quelle=\"GPS\"\n[sonnenzukunft] modus=2\n");
    let l = laden(&fremd).unwrap();
    assert_eq!(l.model.location().lat, Some(53.0));
    let neu = szo::write(&l.model);
    for teil in ["hoehe_nn=12", "quelle=\"GPS\"", "[sonnenzukunft] modus=2"] {
        assert_eq!(neu.matches(teil).count(), 1, "{teil}\n{neu}");
    }
    assert_eq!(szo::write(&laden(&neu).unwrap().model), neu);
    // Auch nach einer Änderung der Lage bleibt das Fremde genau einmal
    let mut s = Scene::with_model(l.model);
    let p = s.model().project().clone();
    let l2 = Location {
        lat: Some(48.0),
        ..*s.model().location()
    };
    assert!(s.projektdaten_setzen("Projektdaten geändert", p, l2));
    let neu = szo::write(s.model());
    for teil in ["[sonnenzukunft] modus=2", "lat=48"] {
        assert_eq!(neu.matches(teil).count(), 1, "{teil}\n{neu}");
    }
    // Fremde Schlüssel der geänderten Zeile gehen nach F-17b verloren
    // (Regel 71), verdoppeln sich aber nie
    for teil in ["hoehe_nn=12", "quelle=\"GPS\""] {
        assert!(neu.matches(teil).count() <= 1, "{teil}\n{neu}");
    }
    assert_eq!(neu.matches("[location]").count(), 1, "{neu}");
}

/// Die Sonne im Modell: um die Nordrichtung im Uhrzeigersinn gedreht.
/// Zeigt Nord nach +x (90°), steht die Mittagssonne (Süden) bei −x.
#[test]
fn abnahme_s1_sonnenvektor_modell() {
    let g = Lage::GANDERKESEE;
    let d = Datum::new(2026, 6, 21).unwrap();
    for stunde in 4..20 {
        let t = Zeitpunkt::ortszeit(d, stunde, 30);
        let geo = sonnenstand(g, t).richtung();
        for nord in [0.0, 90.0, 180.0, 270.0, 33.3, 347.5] {
            let m = sonnenvektor_modell(g, nord, t);
            let (s, c) = nord.to_radians().sin_cos();
            // Drehung im Uhrzeigersinn um z: (x, y) → (x c + y s, −x s + y c)
            let soll = (geo.x * c + geo.y * s, -geo.x * s + geo.y * c);
            assert!((m.x - soll.0).abs() < 1e-12 && (m.y - soll.1).abs() < 1e-12);
            assert!((m.z - geo.z).abs() < 1e-12);
        }
    }
    let mittag = sk_math::sonne::hoechststand(g, d);
    let m = sonnenvektor_modell(g, 90.0, mittag);
    assert!(m.x < -0.4 && m.y.abs() < 0.01, "{m:?}");
}
