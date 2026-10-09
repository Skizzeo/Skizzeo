//! Abnahme Sonnenstand S0 (Analyse „Nordpfeil und Sonnenstand“ §5, §7 und
//! `sonne/sollwerte-ganderkesee.md`), nur über die öffentliche Schnittstelle:
//! die zehn Sollwerte aus NOAA und PSA, ein unabhängiges Verfahren (PSA,
//! Blanco-Muriel 2001) an Zufallsorten und -zeiten weltweit, die
//! gesetzliche Zeit 2026–2030 im Rundlauf und ein fremder Ort mit
//! veröffentlichten Zeiten.

use sk_math::sonne::*;

fn tag(j: i32, m: u32, t: u32) -> Datum {
    Datum::new(j, m, t).unwrap()
}

/// Unterschied zweier Winkel in Grad, kürzester Weg.
fn winkel(a: f64, b: f64) -> f64 {
    let d = (a - b).rem_euclid(360.0);
    d.min(360.0 - d)
}

/// Die zehn Zeilen aus sollwerte-ganderkesee.md: Ortszeit, Azimut und
/// geometrische Höhe nach NOAA und nach PSA.
#[test]
fn abnahme_sonne_sollwerte_ganderkesee() {
    let g = Lage::GANDERKESEE;
    let zeilen = [
        ((2026, 3, 20, 9, 0), (120.86, 20.97), (120.86, 20.96)),
        ((2026, 3, 20, 12, 0), (169.70, 36.43), (169.69, 36.43)),
        ((2026, 6, 21, 9, 0), (93.34, 32.32), (93.34, 32.32)),
        ((2026, 6, 21, 13, 0), (167.36, 59.92), (167.36, 59.92)),
        ((2026, 6, 21, 18, 0), (267.70, 31.56), (267.70, 31.55)),
        ((2026, 9, 23, 15, 0), (210.75, 32.65), (210.76, 32.65)),
        ((2026, 12, 21, 10, 0), (147.13, 7.39), (147.14, 7.40)),
        ((2026, 12, 21, 12, 0), (174.42, 13.33), (174.42, 13.33)),
        ((2026, 12, 21, 14, 0), (202.38, 10.69), (202.39, 10.69)),
        ((2026, 10, 9, 11, 0), (143.30, 24.30), (143.30, 24.30)),
    ];
    for ((j, m, d, h, min), noaa, psa) in zeilen {
        let s = sonnenstand(g, Zeitpunkt::ortszeit(tag(j, m, d), h, min));
        for (name, (az, hoehe)) in [("NOAA", noaa), ("PSA", psa)] {
            // Schranke 0,05° plus die Rundung der Tabelle auf 0,01°
            assert!(
                winkel(s.azimut, az) <= 0.055,
                "{d}.{m}. {h}:{min:02} Azimut {} gegen {name} {az}",
                s.azimut
            );
            assert!(
                (s.hoehe_geometrisch - hoehe).abs() <= 0.055,
                "{d}.{m}. {h}:{min:02} Höhe {} gegen {name} {hoehe}",
                s.hoehe_geometrisch
            );
        }
        // Die scheinbare Höhe liegt darüber, über 5° um höchstens 0,2°
        assert!(s.hoehe > s.hoehe_geometrisch && s.hoehe - s.hoehe_geometrisch < 0.2);
    }
}

/// Sonnenstand nach PSA (Blanco-Muriel u. a., Solar Energy 70, 2001), wie
/// in sonne/sollwerte-psa.py, mit Parallaxe: Azimut und geometrische Höhe.
fn psa(lage: Lage, t: Zeitpunkt) -> (f64, f64) {
    let tage = t.0 as f64 / 86_400.0;
    let n = tage + 2_440_587.5 - 2_451_545.0;
    let stunde = t.0.rem_euclid(86_400) as f64 / 3600.0;
    let om = 2.1429 - 0.0010394594 * n;
    let l = 4.8950630 + 0.017202791698 * n;
    let g = 6.2400600 + 0.0172019699 * n;
    let lam =
        l + 0.03341607 * g.sin() + 0.00034894 * (2.0 * g).sin() - 0.0001134 - 0.0000203 * om.sin();
    let ep = 0.4090928 - 6.2140e-9 * n + 0.0000396 * om.cos();
    let ra = (ep.cos() * lam.sin())
        .atan2(lam.cos())
        .rem_euclid(std::f64::consts::TAU);
    let d = (ep.sin() * lam.sin()).asin();
    let gmst = 6.6974243242 + 0.0657098283 * n + stunde;
    let w = (gmst * 15.0 + lage.laenge).to_radians() - ra;
    let la = lage.breite.to_radians();
    let mut z = (la.cos() * w.cos() * d.cos() + d.sin() * la.sin())
        .clamp(-1.0, 1.0)
        .acos();
    let y = -w.sin();
    let x = d.tan() * la.cos() - la.sin() * w.cos();
    let az = y.atan2(x).to_degrees().rem_euclid(360.0);
    z += 6371.01 / 149_597_890.0 * z.sin();
    (az, 90.0 - z.to_degrees())
}

/// Richtung aus Azimut und Höhe (Ost, Nord, oben).
fn vektor(az: f64, h: f64) -> [f64; 3] {
    let (a, h) = (az.to_radians(), h.to_radians());
    [a.sin() * h.cos(), a.cos() * h.cos(), h.sin()]
}

/// Ein zweites, unabhängiges Verfahren an 20 000 Zufallspunkten: Orte
/// zwischen 80° S und 80° N, Zeiten 2020 bis 2035. Der Winkel zwischen
/// beiden Sonnenrichtungen bleibt unter 0,05°.
#[test]
fn abnahme_sonne_unabhaengig_wie_psa() {
    let mut x: u64 = 0x9E37_79B9_7F4A_7C15;
    let mut zufall = || {
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        (x >> 11) as f64 / (1u64 << 53) as f64
    };
    let von = Zeitpunkt::utc(tag(2020, 1, 1), 0, 0, 0).0;
    let bis = Zeitpunkt::utc(tag(2036, 1, 1), 0, 0, 0).0;
    let mut groesste = 0.0f64;
    for _ in 0..20_000 {
        let lage = Lage {
            breite: -80.0 + 160.0 * zufall(),
            laenge: -180.0 + 360.0 * zufall(),
        };
        let t = Zeitpunkt(von + ((bis - von) as f64 * zufall()) as i64);
        let s = sonnenstand(lage, t);
        let (az, h) = psa(lage, t);
        let a = vektor(s.azimut, s.hoehe_geometrisch);
        let b = vektor(az, h);
        let c = (a[0] * b[0] + a[1] * b[1] + a[2] * b[2]).clamp(-1.0, 1.0);
        let abstand = c.acos().to_degrees();
        groesste = groesste.max(abstand);
        assert!(
            abstand < 0.05,
            "{lage:?} {t:?}: {s:?} gegen PSA {az} {h}, {abstand}°"
        );
        // Ganz unabhängig von der Höhe: richtung() ist ein Einheitsvektor
        // zur scheinbaren Höhe
        let r = s.richtung();
        assert!((r.dot(r) - 1.0).abs() < 1e-9);
        assert!((r.z - s.hoehe.to_radians().sin()).abs() < 1e-9);
    }
    println!("größte Abweichung zu PSA: {groesste:.4}°");
    assert!(groesste > 0.0);
}

/// Gesetzliche Zeit: jede Viertelstunde 2026 bis 2030 hin und zurück.
/// Außer in der doppelten Stunde im Oktober ergibt die Ortszeit wieder
/// denselben Zeitpunkt; die übersprungene Stunde im März kommt nie vor.
#[test]
fn abnahme_sonne_ortszeit_rundlauf_2026_bis_2030() {
    let von = Zeitpunkt::utc(tag(2026, 1, 1), 0, 0, 0);
    let bis = Zeitpunkt::utc(tag(2031, 1, 1), 0, 0, 0);
    let mut t = von;
    let (mut sommer, mut doppelt) = (0, 0);
    while t < bis {
        let o = t.in_ortszeit();
        let (h, m) = (o.minuten / 60, o.minuten % 60);
        let (beginn, ende) = sommerzeit(o.datum.jahr);
        // Am Tag der Umstellung im März gibt es 02:00 bis 02:59 nicht
        if o.datum == beginn.in_ortszeit().datum {
            assert!(h != 2, "{t:?}: {o:?} in der übersprungenen Stunde");
        }
        let zurueck = Zeitpunkt::ortszeit(o.datum, h, m);
        if zurueck != t {
            // Nur die zweite 02:xx im Oktober (MEZ) ist mehrdeutig
            assert!(
                !o.sommerzeit && h == 2 && t >= ende && t.0 < ende.0 + 3600,
                "{t:?} {o:?}"
            );
            assert_eq!(zurueck.0, t.0 - 3600);
            doppelt += 1;
        }
        sommer += usize::from(o.sommerzeit);
        t = t.plus(900);
    }
    // Vier Viertelstunden je Jahr sind mehrdeutig
    assert_eq!(doppelt, 5 * 4);
    // Sommerzeit dauert 2026–2030 je 30 oder 31 Wochen
    assert!(sommer > 5 * 30 * 7 * 96 && sommer < 5 * 31 * 7 * 96);
}

/// Fremder Ort mit veröffentlichten Zeiten: Sydney (33,8688° S,
/// 151,2093° O) am 21.12.2026, Auf- und Untergang laut Bureau of
/// Meteorology / timeanddate 05:41 und 20:05 AEDT (UTC+11), Mittag 12:53
/// (Richtwerte aus den üblichen Tabellen, Schranke ±2 min).
#[test]
fn abnahme_sonne_sydney() {
    let sydney = Lage {
        breite: -33.8688,
        laenge: 151.2093,
    };
    let d = tag(2026, 12, 21);
    let aedt = |t: Zeitpunkt| (t.0 + 11 * 3600).rem_euclid(86_400) / 60;
    let (a, u) = auf_untergang(sydney, d).unwrap();
    let m = hoechststand(sydney, d);
    for (was, t, soll) in [
        ("Aufgang", a, 5 * 60 + 41),
        ("Untergang", u, 20 * 60 + 5),
        ("Mittag", m, 12 * 60 + 53),
    ] {
        let ist = aedt(t);
        assert!((ist - soll).abs() <= 2, "{was}: {ist} min statt {soll}");
    }
    // Ortszeit-Tag ist der 21.12. (Weltzeit-Tag des Mittags)
    assert_eq!(Datum::aus_tagen((m.0 + 11 * 3600).div_euclid(86_400)), d);
    // Mittags steht die Sonne im Norden, fast im Zenit
    let s = sonnenstand(sydney, m);
    assert!(
        winkel(s.azimut, 0.0) < 1.0 && s.hoehe_geometrisch > 79.0,
        "{s:?}"
    );
}

/// Würfelschatten nach §7: ein 10-m-Würfel wirft mittags einen Schatten
/// von 10 m / tan(h); mit der scheinbaren Höhe innerhalb ±2 % der Tabelle.
#[test]
fn abnahme_sonne_wuerfelschatten() {
    let g = Lage::GANDERKESEE;
    for (d, soll) in [
        (tag(2026, 3, 20), 13.3),
        (tag(2026, 6, 21), 5.7),
        (tag(2026, 9, 23), 13.4),
        (tag(2026, 12, 21), 41.7),
    ] {
        let s = sonnenstand(g, hoechststand(g, d));
        let r = s.richtung();
        let schatten = 10.0 * (r.x * r.x + r.y * r.y).sqrt() / r.z;
        assert!(
            (schatten / soll - 1.0).abs() <= 0.02,
            "{d:?}: {schatten:.2} m statt {soll} m"
        );
    }
}
