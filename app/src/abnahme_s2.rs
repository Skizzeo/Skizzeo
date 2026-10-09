//! Abnahme Sonnenstand S2 (Nordpfeil): Aufziehen, Drehen und Verschieben
//! an den drei Referenzhäusern über die Ereignisse der Ansicht, wie die App
//! sie an den Pfeil gibt; Rundlauf und Rückgängig über den Speicherweg der
//! App (`szo::write`, `szo::read_with` mit den Kostenabschnitten).

use crate::camera::Camera;
use crate::nordpfeil::{
    anzeige_fuss, laenge, richtung, Ausgang, Griff, Nordpfeil, Stand, LABEL_DREHEN, LABEL_SCHIEBEN,
};
use crate::scene::Scene;
use sk_math::vec3;
use sk_model::{szo, Foot, GuidGen, Location, Model};
use sk_platform::{Event, Key, Modifiers, MouseButton};
use std::f64::consts::FRAC_PI_2;

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

const W: f64 = 1200.0;
const H: f64 = 900.0;
const OHNE: Modifiers = Modifiers {
    shift: false,
    ctrl: false,
    alt: false,
};

fn laden(text: &str) -> szo::Loaded {
    szo::read_with(text, GuidGen::with_seed(1), &sk_cost::lesen::ABSCHNITTE_SZO).unwrap()
}

/// Rundlauf: Laden ohne Hinweis, Schreiben bytegleich.
fn rundlauf(text: &str, wo: &str) {
    let l = laden(text);
    assert!(l.hints.is_empty(), "{wo}: {:?}", l.hints);
    assert_eq!(szo::write(&l.model), text, "{wo}");
}

/// Grundriss über dem Haus, so groß, dass Haus und Pfeil daneben passen.
fn grundriss(s: &Scene) -> Camera {
    let (lo, hi) = s
        .bounds()
        .unwrap_or((vec3(0.0, 0.0, 0.0), vec3(0.0, 0.0, 0.0)));
    let mitte = vec3((lo.x + hi.x) * 0.5, (lo.y + hi.y) * 0.5, 0.0);
    let half = ((hi.x - lo.x).max(hi.y - lo.y) * 0.5 + 12000.0).max(10000.0);
    Camera::parallel(mitte, FRAC_PI_2, -FRAC_PI_2, half)
}

fn stand(s: &Scene) -> Stand {
    let m = s.model();
    Stand {
        nord: m.location().north,
        fuss: anzeige_fuss(m.north_foot(), s.bounds()),
        gesetzt: m.north_foot(),
    }
}

fn maus(cam: &Camera, p: Foot, art: u8, shift: bool) -> Event {
    let (x, y) = cam.project(vec3(p[0], p[1], 0.0), W, H).unwrap();
    let mods = Modifiers { shift, ..OHNE };
    match art {
        0 => Event::MouseDown {
            button: MouseButton::Left,
            x,
            y,
            mods,
        },
        1 => Event::MouseMove { x, y, mods },
        _ => Event::MouseUp {
            button: MouseButton::Left,
            x,
            y,
            mods,
        },
    }
}

/// Ein Ereignis an den Pfeil und, wie in der App, ein Schritt daraus.
fn ereignis(n: &mut Nordpfeil, s: &mut Scene, cam: &Camera, e: Event) -> Ausgang {
    let out = n.handle(&e, stand(s), cam, W, H, 1.0, true);
    if let Some((label, nord, fuss)) = out.commit {
        s.nordpfeil_setzen(label, nord, fuss);
    }
    out
}

fn zeilen(text: &str) -> Vec<(usize, &str)> {
    text.lines()
        .enumerate()
        .filter(|(_, z)| z.starts_with("[location]"))
        .collect()
}

fn nr(text: &str, z: &str) -> usize {
    text.lines().position(|l| l.starts_with(z)).unwrap()
}

/// Je Referenzhaus: in einem Zug aufziehen (Umschalt 15°), Lage über die
/// Maske, an der Spitze drehen, am Schaft verschieben; jede Stufe genau
/// eine Zeile hinter den Projektdaten, Rundlauf bytegleich ohne Hinweis;
/// Strg+Z bis zum Anfang bytegleich, Strg+Y zurück bytegleich.
#[test]
fn abnahme_s2_referenzhaeuser() {
    for (name, text) in HAEUSER {
        let alt = szo::write(&laden(text).model);
        let mut s = Scene::with_model(laden(&alt).model);
        let cam = grundriss(&s);
        let (lo, hi) = s.bounds().unwrap();
        let mut n = Nordpfeil::default();
        let mut stufen = vec![alt.clone()];

        // Ohne Nordrichtung kein Pfeil, kein Griff
        assert_eq!(n.gezeigt(stand(&s)), None, "{name}");

        // Aufziehen rechts neben dem Haus, Taste gehalten, 15° eingerastet
        n.set_aktiv(true);
        let fuss = [hi.x + 3004.0, lo.y - 1996.0];
        ereignis(&mut n, &mut s, &cam, maus(&cam, fuss, 0, false));
        let ziel = [fuss[0] + 2000.0, fuss[1] + 4000.0]; // 26,6°
        ereignis(&mut n, &mut s, &cam, maus(&cam, ziel, 1, true));
        let out = ereignis(&mut n, &mut s, &cam, maus(&cam, ziel, 2, true));
        let (label, nord, f) = out.commit.expect("Loslassen setzt");
        assert_eq!((label, nord), (LABEL_DREHEN, 30.0), "{name}");
        let f = f.unwrap();
        assert!(f.iter().all(|v| v % 10.0 == 0.0), "{name}: 10 mm {f:?}");
        assert!((f[0] - fuss[0]).abs() <= 10.0 && (f[1] - fuss[1]).abs() <= 10.0);
        assert!(!n.aktiv && !n.zieht(), "{name}");
        assert_eq!(s.undo_label(), Some(LABEL_DREHEN), "{name}");
        let t = szo::write(s.model());
        let z = zeilen(&t);
        assert_eq!(z.len(), 1, "{name}");
        assert_eq!(z[0].0, nr(&t, "[project]") + 1, "{name}: hinter [project]");
        assert_eq!(z[0].1, format!("[location] north=30 x={} y={}", f[0], f[1]));
        assert_eq!(
            stand(&s).fuss,
            f,
            "{name}: neben dem Haus steht er, wo er ist"
        );
        rundlauf(&t, name);
        stufen.push(t);

        // Breite und Länge über die Maske: Richtung und Fußpunkt bleiben
        let p = s.model().project().clone();
        let ort = Location {
            lat: Some(53.0589),
            lon: Some(8.591),
            ..*s.model().location()
        };
        assert!(s.projektdaten_setzen("Projektdaten geändert", p, ort));
        let t = szo::write(s.model());
        assert_eq!(
            zeilen(&t)[0].1,
            format!(
                "[location] lat=53.0589 lon=8.591 north=30 x={} y={}",
                f[0], f[1]
            ),
            "{name}"
        );
        rundlauf(&t, name);
        stufen.push(t);

        // An der Spitze drehen: nur die Richtung, ein Schritt
        let l = laenge(&cam, H, 1.0);
        let [dx, dy] = richtung(30.0);
        let spitze = [f[0] + dx * l, f[1] + dy * l];
        ereignis(&mut n, &mut s, &cam, maus(&cam, spitze, 1, false));
        assert_eq!(n.over(), Some(Griff::Spitze), "{name}");
        ereignis(&mut n, &mut s, &cam, maus(&cam, spitze, 0, false));
        let west = [f[0] - 5000.0, f[1] + 120.0];
        ereignis(&mut n, &mut s, &cam, maus(&cam, west, 1, false));
        let out = ereignis(&mut n, &mut s, &cam, maus(&cam, west, 2, false));
        assert_eq!(out.commit, Some((LABEL_DREHEN, 270.0, Some(f))), "{name}");
        let t = szo::write(s.model());
        assert!(t.contains(&format!(
            "\n[location] lat=53.0589 lon=8.591 north=270 x={} y={}\n",
            f[0], f[1]
        )));
        rundlauf(&t, name);
        stufen.push(t);

        // Am Schaft verschieben: nur der Fußpunkt, auf 10 mm, ein Schritt
        let [dx, dy] = richtung(270.0);
        let mitte = [f[0] + dx * l * 0.5, f[1] + dy * l * 0.5];
        ereignis(&mut n, &mut s, &cam, maus(&cam, mitte, 1, false));
        assert_eq!(n.over(), Some(Griff::Schaft), "{name}");
        ereignis(&mut n, &mut s, &cam, maus(&cam, mitte, 0, false));
        let hin = [mitte[0] + 777.0, mitte[1] + 2333.0];
        ereignis(&mut n, &mut s, &cam, maus(&cam, hin, 1, false));
        let out = ereignis(&mut n, &mut s, &cam, maus(&cam, hin, 2, false));
        let (label, nord, g) = out.commit.expect("verschoben");
        assert_eq!((label, nord), (LABEL_SCHIEBEN, 270.0), "{name}");
        let g = g.unwrap();
        assert!(g.iter().all(|v| v % 10.0 == 0.0), "{name}: {g:?}");
        assert!((g[0] - f[0] - 777.0).abs() <= 10.0 && (g[1] - f[1] - 2333.0).abs() <= 10.0);
        assert_eq!(s.undo_label(), Some(LABEL_SCHIEBEN), "{name}");
        let t = szo::write(s.model());
        assert_eq!(zeilen(&t).len(), 1, "{name}");
        rundlauf(&t, name);
        stufen.push(t);

        // Strg+Z Stufe für Stufe bytegleich, Strg+Y ebenso
        for i in (0..stufen.len() - 1).rev() {
            assert!(s.undo(), "{name}");
            assert_eq!(szo::write(s.model()), stufen[i], "{name}: Strg+Z {i}");
        }
        for (i, st) in stufen.iter().enumerate().skip(1) {
            assert!(s.redo(), "{name}");
            assert_eq!(&szo::write(s.model()), st, "{name}: Strg+Y {i}");
        }
    }
}

/// Fußpunkt im Haus: Der Pfeil steht vorne links daneben, die Datei
/// behält den aufgezogenen Punkt; Drehen dort schreibt ihn unverändert,
/// Verschieben schreibt den neuen. Ohne Fußpunkt (S1-Datei) schreibt
/// Drehen keinen.
#[test]
fn abnahme_s2_fusspunkt_im_haus_und_ohne() {
    let alt = szo::write(&laden(HAEUSER[0].1).model);
    let mut s = Scene::with_model(laden(&alt).model);
    let cam = grundriss(&s);
    let (lo, hi) = s.bounds().unwrap();
    let innen = [
        ((lo.x + hi.x) * 0.5 / 10.0).round() * 10.0,
        ((lo.y + hi.y) * 0.5 / 10.0).round() * 10.0,
    ];
    let mut n = Nordpfeil::default();
    n.set_aktiv(true);
    ereignis(&mut n, &mut s, &cam, maus(&cam, innen, 0, false));
    ereignis(&mut n, &mut s, &cam, maus(&cam, innen, 2, false));
    assert!(n.zieht(), "Klick setzt nur den Fußpunkt");
    let ost = [innen[0] + 4000.0, innen[1]];
    ereignis(&mut n, &mut s, &cam, maus(&cam, ost, 1, false));
    let out = ereignis(&mut n, &mut s, &cam, maus(&cam, ost, 0, false));
    assert_eq!(out.commit, Some((LABEL_DREHEN, 90.0, Some(innen))));
    ereignis(&mut n, &mut s, &cam, maus(&cam, ost, 2, false));
    let st = stand(&s);
    assert_eq!(st.fuss, [lo.x - 2000.0, lo.y - 2000.0], "daneben gezeigt");
    assert_eq!(st.gesetzt, Some(innen));
    let t = szo::write(s.model());
    assert!(t.contains(&format!(
        "[location] north=90 x={} y={}\n",
        innen[0], innen[1]
    )));
    rundlauf(&t, "innen");

    // Drehen am gezeigten Platz: der Fußpunkt der Datei bleibt
    let l = laenge(&cam, H, 1.0);
    let spitze = [st.fuss[0] + l, st.fuss[1]];
    ereignis(&mut n, &mut s, &cam, maus(&cam, spitze, 1, false));
    ereignis(&mut n, &mut s, &cam, maus(&cam, spitze, 0, false));
    let sued = [st.fuss[0], st.fuss[1] - 5000.0];
    ereignis(&mut n, &mut s, &cam, maus(&cam, sued, 1, false));
    let out = ereignis(&mut n, &mut s, &cam, maus(&cam, sued, 2, false));
    assert_eq!(out.commit, Some((LABEL_DREHEN, 180.0, Some(innen))));

    // Ohne Fußpunkt: Drehen schreibt keinen, x/y fehlen
    let mut s = Scene::with_model(laden(&alt).model);
    assert!(s.nordpfeil_setzen(LABEL_DREHEN, 45.0, None));
    let st = stand(&s);
    assert_eq!(st.gesetzt, None);
    let spitze = {
        let [dx, dy] = richtung(45.0);
        [st.fuss[0] + dx * l, st.fuss[1] + dy * l]
    };
    let mut n = Nordpfeil::default();
    ereignis(&mut n, &mut s, &cam, maus(&cam, spitze, 1, false));
    assert_eq!(n.over(), Some(Griff::Spitze));
    ereignis(&mut n, &mut s, &cam, maus(&cam, spitze, 0, false));
    let nord = [st.fuss[0] + 30.0, st.fuss[1] + 5000.0];
    ereignis(&mut n, &mut s, &cam, maus(&cam, nord, 1, false));
    let out = ereignis(&mut n, &mut s, &cam, maus(&cam, nord, 2, false));
    assert_eq!(out.commit, Some((LABEL_DREHEN, 0.0, None)));
    let t = szo::write(s.model());
    assert!(t.contains("\n[location] north=0\n"), "{}", zeilen(&t)[0].1);
    rundlauf(&t, "ohne");
}

/// Kein Schritt ohne Änderung: Klick auf die Spitze oder den Schaft ohne
/// Ziehen, Esc mitten im Drehen oder Verschieben; Zahl + Enter mit
/// negativer und zu großer Zahl; Esc der Reihe nach ohne Schritt.
#[test]
fn abnahme_s2_kein_schritt_und_zahlen() {
    let mut s = Scene::with_model(Model::with_seed(1));
    let cam = grundriss(&s);
    assert!(s.nordpfeil_setzen(LABEL_DREHEN, 20.0, Some([3000.0, 3000.0])));
    let vorher = szo::write(s.model());
    let l = laenge(&cam, H, 1.0);
    let [dx, dy] = richtung(20.0);
    let spitze = [3000.0 + dx * l, 3000.0 + dy * l];
    let mitte = [3000.0 + dx * l * 0.5, 3000.0 + dy * l * 0.5];
    let mut n = Nordpfeil::default();
    for p in [spitze, mitte] {
        ereignis(&mut n, &mut s, &cam, maus(&cam, p, 1, false));
        assert!(ereignis(&mut n, &mut s, &cam, maus(&cam, p, 0, false)).consumed);
        let out = ereignis(&mut n, &mut s, &cam, maus(&cam, p, 2, false));
        assert_eq!(out.commit, None, "Klick ohne Ziehen");
        // Ziehen, dann Esc, dann loslassen: nichts
        ereignis(&mut n, &mut s, &cam, maus(&cam, p, 0, false));
        let weg = [p[0] - 4000.0, p[1] - 1500.0];
        ereignis(&mut n, &mut s, &cam, maus(&cam, weg, 1, false));
        assert!(
            n.gezeigt(stand(&s)).unwrap() != (20.0, [3000.0, 3000.0]),
            "Vorschau"
        );
        assert!(n.escape());
        assert_eq!(
            n.gezeigt(stand(&s)),
            Some((20.0, [3000.0, 3000.0])),
            "Esc verwirft"
        );
        let out = ereignis(&mut n, &mut s, &cam, maus(&cam, weg, 2, false));
        assert_eq!(out.commit, None);
    }
    assert_eq!(szo::write(s.model()), vorher);
    assert_eq!(s.undo_label(), Some(LABEL_DREHEN));

    // Zahl + Enter: −17 heißt 343°, 360 heißt 0°
    for (tasten, soll) in [("-17", 343.0), ("360", 0.0), ("0", 0.0), ("359,5", 359.5)] {
        n.set_aktiv(true);
        ereignis(&mut n, &mut s, &cam, maus(&cam, [-5000.0, 0.0], 0, false));
        ereignis(&mut n, &mut s, &cam, maus(&cam, [-5000.0, 0.0], 2, false));
        let mut out = Ausgang::default();
        for ch in tasten.chars() {
            assert!(n.key(Key::Char(ch), OHNE, &mut out), "{tasten}");
        }
        assert!(n.key(Key::Enter, OHNE, &mut out));
        let (_, nord, f) = out.commit.expect(tasten);
        assert_eq!((nord, f), (soll, Some([-5000.0, 0.0])), "{tasten}");
        s.nordpfeil_setzen(LABEL_DREHEN, nord, f);
        let t = szo::write(s.model());
        assert_eq!(zeilen(&t).len(), 1);
        let n_text = format!("{}", soll);
        assert!(
            t.contains(&format!("[location] north={n_text} x=-5000 y=0\n")),
            "{tasten}: {}",
            zeilen(&t)[0].1
        );
        rundlauf(&t, tasten);
        assert!(!n.aktiv);
    }

    // Über 360°: Enter setzt nicht, die Eingabe bleibt mit Meldung offen
    let vorher = szo::write(s.model());
    n.set_aktiv(true);
    ereignis(&mut n, &mut s, &cam, maus(&cam, [100.0, 100.0], 0, false));
    let mut out = Ausgang::default();
    for ch in "400".chars() {
        n.key(Key::Char(ch), OHNE, &mut out);
    }
    assert!(n.key(Key::Enter, OHNE, &mut out));
    assert_eq!(out.commit, None);
    assert!(n.input().is_some_and(|i| i.error.is_some()));
    assert!(n.escape() && n.escape() && n.escape() && !n.aktiv);
    assert_eq!(szo::write(s.model()), vorher);

    // Esc der Reihe nach: Eingabe, Fußpunkt, Werkzeug; kein Schritt
    let vorher = szo::write(s.model());
    n.set_aktiv(true);
    ereignis(&mut n, &mut s, &cam, maus(&cam, [100.0, 100.0], 0, false));
    n.key(Key::Char('5'), OHNE, &mut Ausgang::default());
    assert!(n.escape() && n.input().is_none() && n.zieht());
    assert!(n.escape() && !n.zieht() && n.aktiv);
    assert!(n.escape() && !n.aktiv);
    assert!(!n.escape());
    assert_eq!(szo::write(s.model()), vorher);
}

/// Zufallsfolgen im Modell: Nordpfeil setzen (auch ohne Fußpunkt und mit
/// Unbrauchbarem), Lage über die Maske, Rückgängig und Wiederholen. Nach
/// jedem Schritt: höchstens eine Zeile, `x`/`y` nur zusammen und nur mit
/// `north`, Rundlauf bytegleich ohne Hinweis; Rückgängig und Wiederholen
/// treffen jeden früheren Stand bytegleich.
#[test]
fn abnahme_s2_zufallsfolgen() {
    let mut r: u64 = 0x5eed_5202;
    let mut zufall = move || {
        r ^= r << 13;
        r ^= r >> 7;
        r ^= r << 17;
        r
    };
    let alt = szo::write(&laden(HAEUSER[1].1).model);
    let mut s = Scene::with_model(laden(&alt).model);
    let mut verlauf = vec![alt.clone()];
    let mut pos = 0usize;
    for i in 0..600 {
        let w = zufall() % 10;
        let wert = |z: u64, max: f64| (z % 2_000_001) as f64 / 1_000_000.0 * max - max;
        let geaendert = match w {
            0..=3 => {
                let nord = match zufall() % 5 {
                    0 => f64::NAN,
                    1 => -725.25,
                    _ => wert(zufall(), 720.0),
                };
                let fuss = match zufall() % 5 {
                    0 => None,
                    1 => Some([1e10, 0.0]),
                    _ => Some([wert(zufall(), 50_000.0), wert(zufall(), 50_000.0)]),
                };
                let label = if w % 2 == 0 {
                    LABEL_DREHEN
                } else {
                    LABEL_SCHIEBEN
                };
                s.nordpfeil_setzen(label, nord, fuss)
            }
            4..=5 => {
                let p = s.model().project().clone();
                let ort = Location {
                    lat: (zufall() % 3 != 0).then(|| wert(zufall(), 90.0)),
                    lon: (zufall() % 3 != 0).then(|| wert(zufall(), 180.0)),
                    north: s.model().location().north,
                };
                s.projektdaten_setzen("Projektdaten geändert", p, ort)
            }
            6..=7 => {
                if s.undo() {
                    pos -= 1;
                    assert_eq!(szo::write(s.model()), verlauf[pos], "{i}: Strg+Z");
                }
                false
            }
            _ => {
                if s.redo() {
                    pos += 1;
                    assert_eq!(szo::write(s.model()), verlauf[pos], "{i}: Strg+Y");
                }
                false
            }
        };
        let t = szo::write(s.model());
        if geaendert {
            verlauf.truncate(pos + 1);
            verlauf.push(t.clone());
            pos += 1;
        } else {
            assert_eq!(t, verlauf[pos], "{i}: ohne Änderung bytegleich");
        }
        let z = zeilen(&t);
        assert!(z.len() <= 1, "{i}");
        if let Some((_, zeile)) = z.first() {
            let hat = |k: &str| zeile.contains(&format!(" {k}="));
            assert_eq!(hat("x"), hat("y"), "{i}: {zeile}");
            assert!(!hat("x") || hat("north"), "{i}: {zeile}");
            if let Some(n) = s.model().location().north {
                assert!((0.0..360.0).contains(&n), "{i}: {n}");
            }
        }
        rundlauf(&t, &format!("{i}"));
    }
    while s.undo() {}
    assert_eq!(szo::write(s.model()), alt, "Strg+Z bis zum Anfang");
}
