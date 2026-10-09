use super::*;

fn tag(j: i32, m: u32, t: u32) -> Datum {
    Datum::new(j, m, t).unwrap()
}

/// Analyse §7: Ganderkesee, Denkmalsweg (eigene Rechnung nach NOAA,
/// Minuten und Grad gerundet). Höchststand, Auf- und Untergang auf die
/// Minute, Höhe geometrisch auf 0,05°, Azimut auf 0,6°, dazu der Schatten
/// des 10-m-Würfels mittags.
#[test]
fn ganderkesee_wie_in_der_analyse() {
    let g = Lage::GANDERKESEE;
    #[rustfmt::skip]
    let soll = [
        // Tag, Höchststand, Höhe, Aufgang, Azimut, Untergang, Azimut, Schatten (m)
        (tag(2026, 3, 20), (12, 33), 36.9, (6, 29), 89.0, (18, 39), 271.0, 13.3, false),
        (tag(2026, 6, 21), (13, 27), 60.4, (5, 0), 47.0, (21, 56), 313.0, 5.7, true),
        (tag(2026, 9, 23), (13, 18), 36.8, (7, 14), 89.0, (19, 23), 271.0, 13.4, true),
        (tag(2026, 12, 21), (12, 24), 13.5, (8, 38), 130.0, (16, 11), 230.0, 41.7, false),
    ];
    for (d, mittag, hoehe, auf, az_auf, unter, az_unter, schatten, sommer) in soll {
        let uhr = |(h, m): (u32, u32)| Zeitpunkt::ortszeit(d, h, m);
        let nah = |t: Zeitpunkt, s: (u32, u32), was: &str| {
            assert!(
                (t.0 - uhr(s).0).abs() <= 60,
                "{d:?} {was}: {:?}",
                t.in_ortszeit()
            );
            assert_eq!(t.in_ortszeit().sommerzeit, sommer, "{d:?} {was}");
        };
        let m = hoechststand(g, d);
        nah(m, mittag, "Höchststand");
        let s = sonnenstand(g, m);
        assert!(
            (s.hoehe_geometrisch - hoehe).abs() <= 0.05,
            "{d:?}: Höhe {s:?}"
        );
        assert!((s.azimut - 180.0).abs() < 0.01, "{d:?}: mittags Süd {s:?}");
        // Mit Lichtbrechung steht sie etwas höher, der Schatten ist etwas
        // kürzer; beides bleibt im Rahmen der Abnahme von S5 (±2 %)
        assert!(s.hoehe > s.hoehe_geometrisch);
        let l = 10.0 / s.hoehe.to_radians().tan();
        assert!(
            (l - schatten).abs() <= schatten * 0.02,
            "{d:?}: Schatten {l}"
        );
        let (a, u) = auf_untergang(g, d).unwrap();
        nah(a, auf, "Aufgang");
        nah(u, unter, "Untergang");
        assert!((sonnenstand(g, a).azimut - az_auf).abs() <= 0.6, "{d:?}");
        assert!((sonnenstand(g, u).azimut - az_unter).abs() <= 0.6, "{d:?}");
        // Beim Aufgang steht der Mittelpunkt scheinbar knapp unter dem
        // Horizont (oberer Rand am Horizont)
        assert!((sonnenstand(g, a).hoehe_geometrisch - HORIZONT).abs() < 0.01);
    }
}

/// Ein fremder Ort gegen den Bezugswert des NREL-Sonnenrechners (SPA,
/// Reda und Andreas 2004, Beispiel: Golden, Colorado, 17.10.2003,
/// 12:30:30 Ortszeit UTC−7): Zenit 50,11162°, Azimut 194,34024°.
#[test]
fn fremder_ort_wie_nrel() {
    let golden = Lage {
        breite: 39.742476,
        laenge: -105.1786,
    };
    let s = sonnenstand(golden, Zeitpunkt::utc(tag(2003, 10, 17), 19, 30, 30));
    assert!((90.0 - s.hoehe - 50.11162).abs() < 0.05, "{s:?}");
    assert!((s.azimut - 194.34024).abs() < 0.05, "{s:?}");
}

/// Meeus, Beispiel 25.a: Deklination am 13.10.1992, 0 h: −7,78507°.
#[test]
fn deklination_wie_meeus() {
    let (dekl, _) = bahn(Zeitpunkt::utc(tag(1992, 10, 13), 0, 0, 0));
    assert!((dekl + 7.78507).abs() < 0.001, "{dekl}");
}

/// Sommerzeit 2026 bis 2030 nach der EU-Regel: letzter Sonntag im März
/// und im Oktober, 01:00 UTC; die Ortszeit springt 02:00 → 03:00 und
/// 03:00 → 02:00.
#[test]
fn sommerzeit_2026_bis_2030() {
    let soll = [
        (2026, 29, 25),
        (2027, 28, 31),
        (2028, 26, 29),
        (2029, 25, 28),
        (2030, 31, 27),
    ];
    for (j, maerz, oktober) in soll {
        let (b, e) = sommerzeit(j);
        assert_eq!(b, Zeitpunkt::utc(tag(j, 3, maerz), 1, 0, 0), "{j}");
        assert_eq!(e, Zeitpunkt::utc(tag(j, 10, oktober), 1, 0, 0), "{j}");
        assert_eq!(tag(j, 3, maerz).wochentag(), 6);
        // Eine Sekunde davor 01:59:59 MEZ, danach 03:00 MESZ
        let vor = b.plus(-60).in_ortszeit();
        assert_eq!((vor.minuten, vor.sommerzeit), (119, false), "{j}");
        let nach = b.in_ortszeit();
        assert_eq!((nach.minuten, nach.sommerzeit), (180, true), "{j}");
        let vor = e.plus(-60).in_ortszeit();
        assert_eq!((vor.minuten, vor.sommerzeit), (179, true), "{j}");
        let nach = e.in_ortszeit();
        assert_eq!((nach.minuten, nach.sommerzeit), (120, false), "{j}");
        // Ortszeit hin und zurück, Mittag im Sommer und Winter
        for (m, s) in [(1, false), (7, true)] {
            let t = Zeitpunkt::ortszeit(tag(j, m, 15), 12, 0);
            let o = t.in_ortszeit();
            assert_eq!((o.datum, o.minuten, o.sommerzeit), (tag(j, m, 15), 720, s));
        }
    }
}

/// Kalender: Hin- und Rückweg über Jahrhunderte, Schaltjahre, Wochentage.
#[test]
fn kalender() {
    assert_eq!(tag(1970, 1, 1).tage(), 0);
    assert_eq!(tag(2000, 3, 1).tage(), 11_017);
    assert_eq!(tag(1970, 1, 1).wochentag(), 3);
    assert_eq!(tag(2026, 10, 9).wochentag(), 4);
    assert!(Datum::new(2028, 2, 29).is_some());
    assert!(Datum::new(2100, 2, 29).is_none());
    assert!(Datum::new(2000, 2, 29).is_some());
    assert!(Datum::new(2026, 13, 1).is_none());
    for t in (-800_000..800_000).step_by(997) {
        assert_eq!(Datum::aus_tagen(t).tage(), t);
    }
    assert_eq!(tag(2026, 12, 31).plus(1), tag(2027, 1, 1));
}

/// Die Tagesbahn beginnt mit dem Aufgang und endet mit dem Untergang,
/// steigt bis mittags und fällt dann, das Azimut wächst stetig von Ost
/// über Süd nach West.
#[test]
fn tagesbahn_von_auf_bis_untergang() {
    let g = Lage::GANDERKESEE;
    for d in [tag(2026, 6, 21), tag(2026, 12, 21)] {
        let b = tagesbahn(g, d, 600);
        let (a, u) = auf_untergang(g, d).unwrap();
        assert_eq!(b.first().unwrap().0, a);
        assert_eq!(b.last().unwrap().0, u);
        let m = hoechststand(g, d);
        for w in b.windows(2) {
            assert!(w[1].1.azimut > w[0].1.azimut, "{d:?}");
            let steigt = w[1].1.hoehe > w[0].1.hoehe;
            assert_eq!(steigt, w[1].0 <= m.plus(300), "{d:?} {:?}", w[1].0);
        }
    }
    // Ganz im Norden: Mitternachtssonne und Polarnacht
    let tromsoe = Lage {
        breite: 69.65,
        laenge: 18.96,
    };
    assert!(auf_untergang(tromsoe, tag(2026, 6, 21)).is_none());
    assert_eq!(tagesbahn(tromsoe, tag(2026, 6, 21), 3600).len(), 25);
    assert!(tagesbahn(tromsoe, tag(2026, 12, 21), 3600).is_empty());
}

/// Südhalbkugel: mittags steht die Sonne im Norden.
#[test]
fn suedhalbkugel() {
    let sydney = Lage {
        breite: -33.87,
        laenge: 151.21,
    };
    let d = tag(2026, 12, 21);
    let s = sonnenstand(sydney, hoechststand(sydney, d));
    assert!(s.azimut < 0.01 || s.azimut > 359.99, "{s:?}");
    assert!(
        (s.hoehe_geometrisch - (90.0 - 33.87 + 23.43)).abs() < 0.1,
        "{s:?}"
    );
}

/// Die Richtung zur Sonne: mittags im Süden und oben, Länge 1.
#[test]
fn richtung() {
    let s = Sonnenstand {
        azimut: 180.0,
        hoehe: 30.0,
        hoehe_geometrisch: 30.0,
    };
    let r = s.richtung();
    assert!(r.x.abs() < 1e-12 && r.y < 0.0 && (r.z - 0.5).abs() < 1e-12);
    assert!((r.dot(r) - 1.0).abs() < 1e-12);
    let ost = Sonnenstand { azimut: 90.0, ..s }.richtung();
    assert!(ost.x > 0.8 && ost.y.abs() < 1e-12);
}

/// S1: Mit Nord 90° (Norden zeigt nach +x) steht die Mittagssonne bei −x,
/// die Morgensonne (Osten) bei −y; die Höhe bleibt.
#[test]
fn richtung_im_modell() {
    let s = Sonnenstand {
        azimut: 180.0,
        hoehe: 30.0,
        hoehe_geometrisch: 30.0,
    };
    let r = s.richtung_modell(90.0);
    assert!(r.x < -0.86 && r.y.abs() < 1e-12 && (r.z - 0.5).abs() < 1e-12);
    let ost = Sonnenstand { azimut: 90.0, ..s }.richtung_modell(90.0);
    assert!(ost.y < -0.86 && ost.x.abs() < 1e-12);
    assert_eq!(s.richtung_modell(0.0), s.richtung());
    let t = Zeitpunkt::ortszeit(Datum::new(2026, 6, 21).unwrap(), 13, 27);
    let g = Lage::GANDERKESEE;
    let v = sonnenvektor_modell(g, 30.0, t);
    let w = sonnenstand(g, t).richtung_modell(30.0);
    assert_eq!(v, w);
    assert!((v.dot(v) - 1.0).abs() < 1e-12);
}
