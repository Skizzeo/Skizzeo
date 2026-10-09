//! Abnahme Sonnenstand S4 (Sonne, Tagesbahn und Leiste in 3D, §8 09:25):
//! `[sun]` an den drei Referenzhäusern über den Speicherweg der App
//! (`szo::write`, `szo::read_with` mit den Kostenabschnitten) und die
//! Schritte der Szene; der Himmel gegen `sk_math::sonne` und die Sollwerte
//! Ganderkesee (Analyse §7); Würfel nur als Anzeige.

use crate::scene::Scene;
use crate::sonne_view::{
    datum_lesen, himmel, kuppel, licht, schnell, wuerfel_quader, zeit_lesen, zeitpunkt, zone,
    WUERFEL,
};
use sk_math::sonne::{sonnenstand, Datum, Lage, Zeitpunkt};
use sk_math::vec3;
use sk_model::{szo, GuidGen, Location, Model, Sun};

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

fn laden(text: &str) -> szo::Loaded {
    szo::read_with(text, GuidGen::with_seed(1), &sk_cost::lesen::ABSCHNITTE_SZO).unwrap()
}

fn rundlauf(text: &str, wo: &str) {
    let l = laden(text);
    assert!(l.hints.is_empty(), "{wo}: {:?}", l.hints);
    assert_eq!(szo::write(&l.model), text, "{wo}");
}

fn sun(j: i32, m: u32, d: u32, min: u32, on: bool) -> Sun {
    Sun {
        date: Datum::new(j, m, d).unwrap(),
        minutes: min,
        on,
    }
}

fn zeilen(text: &str) -> Vec<&str> {
    text.lines().filter(|z| z.starts_with("[sun]")).collect()
}

/// Je Referenzhaus: Ohne eingeschaltetes System kein `[sun]`, auch mit
/// Lage und Nordpfeil; Einschalten schreibt genau eine Zeile, ohne Schritt
/// und ohne neue Revision; Ausschalten lässt `date`/`time` stehen;
/// Rückgängig des Nordpfeils lässt den Sonnenstand (Ansichtszustand).
#[test]
fn abnahme_s4_referenzhaeuser() {
    for (name, text) in HAEUSER {
        let alt = szo::write(&laden(text).model);
        assert!(zeilen(&alt).is_empty(), "{name}");
        let mut s = Scene::with_model(laden(&alt).model);
        assert!(s.nordpfeil_setzen("Nordrichtung geändert", 30.0, None));
        let mit_pfeil = szo::write(s.model());
        assert!(zeilen(&mit_pfeil).is_empty(), "{name}: nie an, kein [sun]");
        let rev = s.model().revision();
        let label = s.undo_label();

        s.set_sun(sun(2026, 6, 21, 12 * 60, true));
        assert_eq!(s.model().revision(), rev, "{name}");
        assert_eq!(s.undo_label(), label, "{name}: kein Schritt");
        let an = szo::write(s.model());
        assert_eq!(zeilen(&an), ["[sun] date=2026-06-21 time=12:00 on=1"]);
        let ohne: String = an
            .lines()
            .filter(|z| !z.starts_with("[sun]"))
            .map(|z| format!("{z}\n"))
            .collect();
        assert_eq!(ohne, mit_pfeil, "{name}: sonst bytegleich");
        rundlauf(&an, name);

        s.set_sun(sun(2026, 12, 21, 9 * 60 + 5, false));
        let aus = szo::write(s.model());
        assert_eq!(zeilen(&aus), ["[sun] date=2026-12-21 time=09:05"]);
        rundlauf(&aus, name);

        // Strg+Z des Pfeils: der Sonnenstand bleibt, wie er ist
        assert!(s.undo(), "{name}");
        let zurueck = szo::write(s.model());
        assert_eq!(zeilen(&zurueck), ["[sun] date=2026-12-21 time=09:05"]);
        assert!(s.model().location().north.is_none());
        rundlauf(&zurueck, name);
        assert!(s.redo());
        assert_eq!(szo::write(s.model()), aus, "{name}");
    }
}

/// Zufällige Stände im Rundlauf: jedes gültige Datum (auch Schalttage und
/// vierstellige Jahre vor 1000), jede Minute, an und aus.
#[test]
fn abnahme_s4_zufaellige_staende() {
    let mut r: u64 = 0x5_4_0_9;
    let mut zufall = move || {
        r ^= r << 13;
        r ^= r >> 7;
        r ^= r << 17;
        r
    };
    let alt = szo::write(&laden(HAEUSER[0].1).model);
    let mut m = laden(&alt).model;
    let mut n = 0;
    while n < 500 {
        let j = 1 + (zufall() % 9999) as i32;
        let (mo, d) = (1 + (zufall() % 12) as u32, 1 + (zufall() % 31) as u32);
        let Some(date) = Datum::new(j, mo, d) else {
            continue;
        };
        let s = Sun {
            date,
            minutes: (zufall() % 1440) as u32,
            on: zufall() % 2 == 0,
        };
        m.set_sun(s);
        let t = szo::write(&m);
        assert_eq!(zeilen(&t).len(), 1);
        let back = laden(&t);
        assert!(back.hints.is_empty(), "{s:?}: {:?}", back.hints);
        assert_eq!(back.model.sun(), Some(s));
        assert_eq!(szo::write(&back.model), t);
        n += 1;
    }
    let schalt = sun(2028, 2, 29, 0, true);
    m.set_sun(schalt);
    assert_eq!(laden(&szo::write(&m)).model.sun(), Some(schalt));
}

/// Unlesbares bleibt roh mit einem Hinweis samt Zeile; doppelt ist ein
/// Ladefehler mit der Zeile; eine ältere Fassung ohne `[sun]` behält die
/// Zeile als Fremdes.
#[test]
fn abnahme_s4_laden_fehler_hinweise() {
    let alt = szo::write(&laden(HAEUSER[0].1).model);
    let ein = |z: &str| alt.replacen("[storey]", &format!("{z}\n[storey]"), 1);
    let nr = |text: &str, z: &str| text.lines().position(|l| l.starts_with(z)).unwrap() + 1;
    for roh in [
        "[sun] date=2027-02-29 time=12:00 on=1",
        "[sun] date=2026-13-01 time=12:00",
        "[sun] date=26-06-21 time=12:00",
        "[sun] date=2026-06-21 time=12:60",
        "[sun] date=2026-06-21 time=7:30",
        "[sun] time=12:00 on=1",
        "[sun] date=2026-06-21 time=12:00 on=2",
        "[sun] date=0000-06-21 time=12:00",
    ] {
        let text = ein(roh);
        let l = laden(&text);
        assert_eq!(l.model.sun(), None, "{roh}");
        assert_eq!(l.hints.len(), 1, "{roh}: {:?}", l.hints);
        assert!(
            l.hints[0].contains(&format!("Zeile {}", nr(&text, "[sun]"))),
            "{roh}: {:?}",
            l.hints
        );
        let w = szo::write(&l.model);
        assert_eq!(zeilen(&w), [roh], "{roh}: bleibt roh");
        assert_eq!(szo::write(&laden(&w).model), w, "{roh}");
        // Ein Stand ersetzt die Zeile
        let mut m = l.model;
        m.set_sun(sun(2026, 3, 21, 600, false));
        let neu = szo::write(&m);
        assert_eq!(zeilen(&neu), ["[sun] date=2026-03-21 time=10:00"], "{roh}");
    }
    let doppelt = ein("[sun] date=2026-06-21 time=12:00\n[sun] date=2026-06-22 time=12:00");
    let e = szo::read_with(
        &doppelt,
        GuidGen::with_seed(1),
        &sk_cost::lesen::ABSCHNITTE_SZO,
    )
    .err()
    .expect("doppelt ist ein Fehler");
    let zweite = nr(&doppelt, "[sun] date=2026-06-22");
    assert!(e.to_string().contains(&zweite.to_string()), "{e}");
}

/// Der Himmel in Ganderkesee (Analyse §7): Mittags am 21.06. (13:27 MESZ)
/// 60,4°, am 21.12. (12:24 MEZ) 13,5°; die Scheibe liegt in Richtung des
/// Sonnenstands, gedreht mit der Nordrichtung; alle Bahnpunkte über dem
/// Boden; die Stundenmarken liegen zwischen Auf- und Untergang; nachts
/// keine Scheibe und das feste Licht.
#[test]
fn abnahme_s4_himmel_ganderkesee() {
    let q = (vec3(0.0, 0.0, 0.0), vec3(10_000.0, 8_000.0, 7_000.0));
    let (mitte, r) = kuppel(q);
    for (nord, s, hoehe) in [
        (0.0, sun(2026, 6, 21, 13 * 60 + 27, true), 60.4),
        (90.0, sun(2026, 6, 21, 13 * 60 + 27, true), 60.4),
        (225.0, sun(2026, 12, 21, 12 * 60 + 24, true), 13.5),
    ] {
        let l = Location {
            lat: None,
            lon: None,
            north: Some(nord),
        };
        let h = himmel(&l, &s, q);
        let p = h.sonne.expect("am Tag eine Scheibe");
        let d = (p - mitte) * (1.0 / r);
        let soll = sonnenstand(Lage::GANDERKESEE, zeitpunkt(&s)).richtung_modell(nord);
        assert!((d - soll).length() < 1e-9, "{nord}: {d:?} {soll:?}");
        let gemessen = d.z.asin().to_degrees();
        assert!((gemessen - hoehe).abs() < 0.1, "{nord}: {gemessen}");
        assert!(h.tag.iter().all(|(_, v)| v.z >= -1e-9));
        assert!(h.sommer.iter().chain(&h.winter).all(|v| v.z >= -1e-9));
        let (a, b) = (h.tag.first().unwrap().0, h.tag.last().unwrap().0);
        let volle = (0..24)
            .filter(|&st| {
                let t = Zeitpunkt::ortszeit(s.date, st, 0);
                a <= t && t <= b
            })
            .count();
        assert_eq!(h.stunden.len(), volle, "{nord}");
        assert!(licht(&l, &s).is_some());
    }
    // 21.06. Aufgang 05:00, Untergang 21:56 MESZ (±2 min)
    let l = Location::default();
    let h = himmel(&l, &sun(2026, 6, 21, 12 * 60, true), q);
    let m = |t: Zeitpunkt| t.in_ortszeit().minuten as i64;
    assert!((m(h.tag[0].0) - 300).abs() <= 2, "{}", m(h.tag[0].0));
    assert!((m(h.tag.last().unwrap().0) - (21 * 60 + 56)).abs() <= 2);
    // Nachts: keine Scheibe, festes Licht
    let nacht = sun(2026, 6, 21, 23 * 60 + 30, true);
    assert_eq!(himmel(&l, &nacht, q).sonne, None);
    assert_eq!(licht(&l, &nacht), None);
}

/// Der Würfel ist nur Anzeige: ein leeres Projekt mit eingeschaltetem
/// Sonnenstand hat keinen Hüllquader und keine Bauteile, die Datei nur die
/// `[sun]`-Zeile mehr; der Himmel steht um den Würfel von 10 m.
#[test]
fn abnahme_s4_wuerfel_nur_anzeige() {
    let mut s = Scene::with_model(Model::with_seed(1));
    let leer = szo::write(s.model());
    s.set_sun(sun(2026, 6, 21, 720, true));
    assert!(s.bounds().is_none(), "kein Hüllquader");
    assert!(s.model().elements().is_empty());
    let t = szo::write(s.model());
    let ohne: String = t
        .lines()
        .filter(|z| !z.starts_with("[sun]"))
        .map(|z| format!("{z}\n"))
        .collect();
    assert_eq!(ohne, leer);
    let (lo, hi) = wuerfel_quader();
    assert_eq!((hi - lo).x, WUERFEL);
    assert_eq!((hi - lo).z, WUERFEL);
    let (mitte, r) = kuppel((lo, hi));
    assert!(r >= WUERFEL);
    assert_eq!((mitte.x, mitte.y, mitte.z), (5000.0, 5000.0, 0.0));
}

/// Leiste: Zahl + Enter für Datum und Uhrzeit, Schnellwahl, MEZ/MESZ am
/// Tag der Umstellung.
#[test]
fn abnahme_s4_eingaben_und_zone() {
    let d = |j, m, t| Datum::new(j, m, t);
    assert_eq!(datum_lesen("21.6.", 2026), d(2026, 6, 21));
    assert_eq!(datum_lesen("21.06.2027", 2026), d(2027, 6, 21));
    assert_eq!(datum_lesen("21.6.27", 2026), d(2027, 6, 21));
    assert_eq!(datum_lesen("2106", 2026), d(2026, 6, 21));
    assert_eq!(datum_lesen("21062030", 2026), d(2030, 6, 21));
    assert_eq!(datum_lesen("29.2.", 2027), None);
    assert_eq!(datum_lesen("29.2.", 2028), d(2028, 2, 29));
    assert_eq!(datum_lesen("32.1.", 2026), None);
    assert_eq!(datum_lesen("", 2026), None);
    assert_eq!(zeit_lesen("12"), Some(720));
    assert_eq!(zeit_lesen("9.30"), Some(570));
    assert_eq!(zeit_lesen("12,05"), Some(725));
    assert_eq!(zeit_lesen("930"), Some(570));
    assert_eq!(zeit_lesen("23:59"), Some(1439));
    assert_eq!(zeit_lesen("24"), None);
    assert_eq!(zeit_lesen("12:60"), None);
    assert_eq!(zeit_lesen("abc"), None);
    // Schnellwahl im Jahr des Datums, die Uhrzeit bleibt
    let s = sun(2031, 8, 5, 615, true);
    let soll = [(3, 21), (6, 21), (9, 23), (12, 21)];
    for (i, (m, t)) in soll.into_iter().enumerate() {
        assert_eq!(schnell(s, i), sun(2031, m, t, 615, true));
    }
    // Umstellung 2026: 29.03. und 25.10.
    assert_eq!(zone(&sun(2026, 3, 28, 720, true)), "MEZ");
    assert_eq!(zone(&sun(2026, 3, 29, 720, true)), "MESZ");
    assert_eq!(zone(&sun(2026, 10, 24, 720, true)), "MESZ");
    assert_eq!(zone(&sun(2026, 10, 25, 720, true)), "MEZ");
    // 12:00 MESZ ist 10:00 UTC, 12:00 MEZ 11:00 UTC
    let zurueck = |s: &Sun| zeitpunkt(s).in_ortszeit().minuten;
    assert_eq!(zurueck(&sun(2026, 6, 21, 720, true)), 720);
    assert_eq!(
        zeitpunkt(&sun(2026, 6, 21, 720, true)),
        Zeitpunkt::utc(Datum::new(2026, 6, 21).unwrap(), 10, 0, 0)
    );
    assert_eq!(
        zeitpunkt(&sun(2026, 12, 21, 720, true)),
        Zeitpunkt::utc(Datum::new(2026, 12, 21).unwrap(), 11, 0, 0)
    );
}
