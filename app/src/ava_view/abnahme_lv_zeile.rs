//! Abnahme LV-Zeile (Jörn 09.10. 01:47, Bild: Kurztext fehlt, „Menge“ und
//! „Kurztext“ überdecken sich): Die sichtbaren Zellen einer Positionszeile
//! im schmalen Fenster (1160 px bei 150 %, wie bei Jörn) und im breiten
//! (1440 px bei 100 %, wie die Ist-Bilder). Je Zelle zeichnet das Blatt
//! einmal mit und einmal ohne ihren Text; die Spalten, in denen sich das
//! Bild ändert, sind die Tinte der Zelle. Geprüft wird:
//! - Kurztext da und lesbar (mindestens die ersten zwölf Zeichen, nicht
//!   nur „…“),
//! - Menge ungekürzt (Tinte so breit wie der ganze Text),
//! - OZ, Kurztext, Menge und Einheit überdecken sich nicht.
//!
//! Braucht eine echte Schrift (Windows-Schriften oder Liberation Sans);
//! ohne Schrift prüft der Test nichts und sagt das.

use super::*;
use sk_paint::font::Font;
use std::path::Path as Pfad;

fn schrift() -> Option<Fonts> {
    let f = Fonts::system();
    if f.regular.is_some() {
        return Some(f);
    }
    let lib = Pfad::new("/usr/share/fonts/truetype/liberation");
    let lade = |n: &str| std::fs::read(lib.join(n)).ok().and_then(Font::parse);
    Some(Fonts {
        regular: Some(lade("LiberationSans-Regular.ttf")?),
        bold: lade("LiberationSans-Bold.ttf"),
        italic: None,
    })
}

fn standardhaus() -> Scene {
    let m = sk_model::szo::read_with(
        include_str!("../../../crates/sk-cost/referenz/rh1-standardhaus.szo"),
        sk_model::GuidGen::with_seed(1),
        &sk_cost::lesen::ABSCHNITTE_SZO,
    )
    .expect("lädt")
    .model;
    Scene::with_model(m)
}

fn bild(v: &AvaView, t: &Theme, f: &Fonts) -> Vec<u8> {
    let mut c = Canvas::new(v.w as usize, v.h as usize);
    v.paint(&mut c, t, f, Instant::now());
    c.to_rgba8()
}

/// Spalten (x von, x bis), in denen sich die Zeile `y..y+h` zwischen
/// `a` und `b` unterscheidet.
fn tinte(a: &[u8], b: &[u8], w: usize, y: f32, h: f32) -> Option<(usize, usize)> {
    let (y0, y1) = (y.max(0.0) as usize, (y + h) as usize);
    let mut r: Option<(usize, usize)> = None;
    for yy in y0..y1 {
        for x in 0..w {
            let k = (yy * w + x) * 4;
            if a[k..k + 4] != b[k..k + 4] {
                r = Some(r.map_or((x, x), |(l, rr)| (l.min(x), rr.max(x))));
            }
        }
    }
    r
}

#[derive(Clone, Copy)]
enum Zelle {
    Oz,
    Kurztext,
    Menge,
    Einheit,
}

fn ohne(v: &mut AvaView, i: usize, z: Zelle) -> String {
    let feld = match z {
        Zelle::Oz => &mut v.zeilen[i].oz,
        Zelle::Kurztext => &mut v.zeilen[i].text,
        Zelle::Menge => &mut v.zeilen[i].menge,
        Zelle::Einheit => &mut v.zeilen[i].einheit,
    };
    std::mem::take(feld)
}

fn zurueck(v: &mut AvaView, i: usize, z: Zelle, alt: String) {
    match z {
        Zelle::Oz => v.zeilen[i].oz = alt,
        Zelle::Kurztext => v.zeilen[i].text = alt,
        Zelle::Menge => v.zeilen[i].menge = alt,
        Zelle::Einheit => v.zeilen[i].einheit = alt,
    }
}

#[test]
fn abnahme_lv_zeile_kurztext_und_menge_sichtbar() {
    let Some(f) = schrift() else {
        eprintln!("abnahme_lv_zeile: keine Schrift, nichts geprüft");
        return;
    };
    let regular = f.regular.as_ref().unwrap();
    let t = Theme::dark();
    let mut befunde = Vec::new();
    for (fenster, w, h, scale) in [
        ("schmal 1160 px, 150 %", 1160u32, 1170u32, 1.5f32),
        ("schmal 1000 px, 100 %", 1000, 900, 1.0),
        ("breit 1440 px, 100 %", 1440, 960, 1.0),
        ("breit 1920 px, 150 %", 1920, 1170, 1.5),
    ] {
        let mut s = standardhaus();
        let mut v = AvaView::new();
        (v.w, v.h) = (w, h);
        v.scale = scale;
        v.top = 0.0;
        v.datei = "haus.szo".into();
        v.sync(&mut s, None);
        let voll = bild(&v, &t, &f);
        let px = 11.0 * scale;
        let pos: Vec<(usize, f32, f32)> = v
            .sichtbar()
            .into_iter()
            .filter(|(i, _, _)| v.zeilen[*i].art == Art::Position)
            .take(4)
            .collect();
        assert!(!pos.is_empty(), "{fenster}: keine Position sichtbar");
        for (i, y, hz) in pos {
            let oz = v.zeilen[i].oz.clone();
            let mut lage = Vec::new();
            for z in [Zelle::Oz, Zelle::Kurztext, Zelle::Menge, Zelle::Einheit] {
                let alt = ohne(&mut v, i, z);
                let leer = bild(&v, &t, &f);
                zurueck(&mut v, i, z, alt);
                lage.push(tinte(&voll, &leer, w as usize, y, hz));
            }
            let z = &v.zeilen[i];
            let [l_oz, l_kt, l_me, l_eh] = [lage[0], lage[1], lage[2], lage[3]];
            // Kurztext: mindestens die ersten zwölf Zeichen breit
            let anfang: String = z.text.chars().take(12).collect();
            let soll_kt = regular.width(&anfang, px) * 0.9;
            match l_kt {
                None => befunde.push(format!("{fenster} {oz}: Kurztext fehlt ({:?})", z.text)),
                Some((a, b)) if ((b - a + 1) as f32) < soll_kt => befunde.push(format!(
                    "{fenster} {oz}: Kurztext nur {} px breit, „{anfang}“ braucht {soll_kt:.0}",
                    b - a + 1
                )),
                _ => {}
            }
            // Menge: so breit wie der ganze Text, also nicht gekürzt
            let soll_me = regular.width(&z.menge, px) * 0.9;
            match l_me {
                None => befunde.push(format!("{fenster} {oz}: Menge fehlt")),
                Some((a, b)) if ((b - a + 1) as f32) < soll_me => befunde.push(format!(
                    "{fenster} {oz}: Menge {:?} gekürzt ({} px statt {soll_me:.0})",
                    z.menge,
                    b - a + 1
                )),
                _ => {}
            }
            // Keine Zelle überdeckt die nächste (von links nach rechts)
            let namen = ["OZ", "Kurztext", "Menge", "Einheit"];
            let l = [l_oz, l_kt, l_me, l_eh];
            for k in 0..3 {
                if let (Some((_, b)), Some((a, _))) = (l[k], l[k + 1]) {
                    if a <= b {
                        befunde.push(format!(
                            "{fenster} {oz}: {} (bis {b}) überdeckt {} (ab {a})",
                            namen[k],
                            namen[k + 1]
                        ));
                    }
                }
            }
        }
    }
    assert!(
        befunde.is_empty(),
        "{} Befunde:\n{}",
        befunde.len(),
        befunde.join("\n")
    );
}
