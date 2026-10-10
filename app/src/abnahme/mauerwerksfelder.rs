use super::*;

// Abnahmetest A298: Flammung und Relief aus den Feldern des
// Oberflächenfensters kommen im Muster an (Paket 7a (aj), 0d24d75;
// Koordinator 21:05, Bauthread fand dort vor dem Push einen Fehler).
// Geprüft als reine Funktion vom Feldtext bis zum Pattern und weiter
// bis Looks-Zeile 12, Farbe und Datei. Vorbereitet gegen main 85a6ecc.
//
// Einbau: als `mod mauerwerksfelder { use super::*; … }` ans Ende von
// app/src/abnahme.rs.
//
// Angenommener Name steht NUR im Adapter: eine reine Funktion, die
// `apply_attr_value` für die Musterfelder nutzt (dieselben Grenzen,
// Rundung und Meldungen), Schlüssel wie die Felder: "len", "h",
// "joint", "share1", "share2" (FieldId::PatShare(0/1)), "spread",
// "grain", "flame", "relief" und jeder Schlüssel aus
// `proctex::limits` (FieldId::PatNum). Passt das Feld nicht zur Art,
// kommt das Muster unverändert zurück (wie heute `Ok(())` ohne
// Änderung).

use sk_model::proctex::{self, Pattern};

// ===== Adapter =====

/// Muster nach Eingabe von `text` in das Feld `feld`.
fn feld(p: &Pattern, feld: &str, text: &str) -> Result<Pattern, String> {
    crate::prefs::pattern_field(p, feld, text)
}

// ===== Test =====

fn wert(p: &Pattern, key: &str) -> f32 {
    proctex::value(p, key).unwrap_or_else(|| panic!("{key} fehlt"))
}

/// Alle Werte aus `limits` außer `ausser` sind gleich.
fn sonst_gleich(a: &Pattern, b: &Pattern, ausser: &[&str], wer: &str) {
    for (key, _, _) in proctex::limits(proctex::gen_word(a)) {
        if !ausser.contains(key) {
            assert_eq!(wert(a, key), wert(b, key), "{wer}: {key} verändert");
        }
    }
}

/// Anzahl unterschiedlicher Farben auf einem Raster über 3 × 1 m.
fn unterschiede(a: &Pattern, b: &Pattern, base: [u8; 3]) -> usize {
    let mut n = 0;
    for i in 0..60 {
        for j in 0..40 {
            let (u, v) = (17.0 + i as f64 * 49.3, 11.0 + j as f64 * 23.7);
            n += usize::from(proctex::sample(a, base, u, v) != proctex::sample(b, base, u, v));
        }
    }
    n
}

/// A298 (Paket 7a (aj), Koordinator 21:05): Flammung und Relief aus
/// den Feldern.
/// - Feld „Flammung“ 40 und „Relief“ 30 an der Werksvorlage
///   friesisch-bunt: genau diese Werte stehen im Muster, alles andere
///   bleibt; Komma und Runden auf ganze %.
/// - Außerhalb 0–100 oder kein Wert: Meldung, Muster unverändert.
/// - Andere Felder (Länge, Höhe, Fuge, Anteil, Streuung, Kopfenden)
///   lassen Flammung und Relief stehen.
/// - Bei Putz ändern die Felder nichts.
/// - Die Werte kommen an: Looks-Zeile 12 = (Flammung, Enden, Relief),
///   die Farbe ändert sich mit Flammung und mit Relief, die Datei
///   schreibt `flame=40` und `relief=30` und liest sie zurück.
#[test]
fn a298_flammung_und_relief_aus_den_feldern() {
    let start = proctex::factory(proctex::FACING).expect("Werksmuster Verblender");
    assert_eq!(
        (wert(&start, "flame"), wert(&start, "relief")),
        (100.0, 100.0)
    );

    let p = feld(&start, "flame", "40").expect("Flammung 40");
    assert_eq!(wert(&p, "flame"), 40.0);
    sonst_gleich(&start, &p, &["flame"], "Flammung");
    let p = feld(&p, "relief", "30").expect("Relief 30");
    assert_eq!((wert(&p, "flame"), wert(&p, "relief")), (40.0, 30.0));
    sonst_gleich(&start, &p, &["flame", "relief"], "Relief");
    if let (
        Pattern::Masonry {
            palette: a,
            hpal: ha,
            joint_rgb: ja,
            bond: ba,
            seed: sa,
            ..
        },
        Pattern::Masonry {
            palette: b,
            hpal: hb,
            joint_rgb: jb,
            bond: bb,
            seed: sb,
            ..
        },
    ) = (&start, &p)
    {
        assert_eq!(
            (a, ha, ja, ba, sa),
            (b, hb, jb, bb, sb),
            "Farben, Verband, Zufall"
        );
    } else {
        panic!("Mauerwerk erwartet");
    }

    assert_eq!(wert(&feld(&p, "flame", "12,6").unwrap(), "flame"), 13.0);
    assert_eq!(wert(&feld(&p, "relief", " 0 ").unwrap(), "relief"), 0.0);
    assert_eq!(wert(&feld(&p, "relief", "100").unwrap(), "relief"), 100.0);
    for (f, t) in [
        ("flame", "101"),
        ("flame", "-1"),
        ("relief", "abc"),
        ("relief", ""),
    ] {
        assert!(feld(&p, f, t).is_err(), "{f}={t:?} abgelehnt");
    }

    for (f, t) in [
        ("len", "290"),
        ("h", "65"),
        ("joint", "12"),
        ("share1", "70"),
        ("spread", "5"),
        ("fend", "20"),
    ] {
        let q = feld(&p, f, t).unwrap_or_else(|e| panic!("{f}={t}: {e}"));
        assert_ne!(q, p, "{f}={t} wirkt");
        assert_eq!(
            (wert(&q, "flame"), wert(&q, "relief")),
            (40.0, 30.0),
            "{f}={t} lässt Flammung und Relief stehen"
        );
    }

    let putz = proctex::factory(proctex::PLASTER).expect("Werksmuster Putz");
    assert_eq!(feld(&putz, "flame", "40"), Ok(putz.clone()));
    assert_eq!(feld(&putz, "relief", "30"), Ok(putz.clone()));

    // Kommt an: Looks-Zeile 12, Farbe, Datei
    let mut s = Scene::with_model(Model::new());
    let vb = s
        .model()
        .materials()
        .iter()
        .find(|(_, x)| x.name == proctex::FACING)
        .map(|(_, x)| x.surface)
        .expect("Verblender");
    assert!(s.edit_model("Einstellungen geändert", |m| m
        .set_surface_pattern(vb, Some(p.clone()))));
    let z = crate::draw_table::pattern_rows(s.model(), vb);
    assert_eq!(z[4][0], 40.0, "Looks-Zeile 12: Flammung");
    assert_eq!(z[4][1], wert(&p, "fend"), "Looks-Zeile 12: Enden");
    assert_eq!(z[4][2], 30.0, "Looks-Zeile 12: Relief");

    let base = s.model().attr().surface(vb).unwrap().color;
    let ohne_f = feld(&p, "flame", "0").unwrap();
    let voll_f = feld(&p, "flame", "100").unwrap();
    assert!(
        unterschiede(&ohne_f, &voll_f, base) > 0,
        "Flammung ändert die Farbe"
    );
    let ohne_r = feld(&p, "relief", "0").unwrap();
    let voll_r = feld(&p, "relief", "100").unwrap();
    assert!(
        unterschiede(&ohne_r, &voll_r, base) > 0,
        "Relief ändert die Farbe"
    );

    let text = sk_model::szo::write(s.model());
    let g = s.model().attr().surface(vb).unwrap().guid;
    let zeile = text
        .lines()
        .find(|l| l.starts_with("[pattern]") && l.contains(&format!("surface={g} ")))
        .expect("Musterzeile Verblender");
    assert!(
        zeile.contains(" flame=40 ") && zeile.contains(" relief=30 "),
        "{zeile}"
    );
    let l = sk_model::szo::read(&text, sk_model::GuidGen::with_seed(1)).expect("öffnet");
    let gelesen = l
        .model
        .attr()
        .surfaces()
        .iter()
        .find(|(_, x)| x.guid == g)
        .and_then(|(_, x)| x.pattern.clone())
        .expect("Muster gelesen");
    assert_eq!(gelesen, p, "Rundlauf");
}
