//! Abnahme der IFC-Vorschau mit Test (ifc/plan.md §6): das 3D-Bild von r7
//! gegen ein Soll-Bild im Repository, die Wahl im Bild gegen den Baum und
//! auf Wunsch ein Ist-Bild des ganzen Fensters.
//!
//! Das Soll-Bild entsteht mit
//! `SKIZZEO_ISTBILDER=<ordner> cargo test -p skizzeo istbilder_i3 -- --ignored`
//! (Datei `ist-i3-vorschau-r7-bild.png`) und wird nach Sichtprüfung nach
//! `soll/r7.png` kopiert. Der Vergleich schreibt es nie selbst.

use super::baum::Ziel;
use super::fenster::IfcFenster;
use sk_model::{szo, GuidGen};
use sk_platform::{Event, Modifiers, MouseButton};
use sk_ui::theme::Theme;
use sk_ui::widgets::Fonts;

const R7: &str = include_str!("../../../crates/sk-cost/referenz/r7-alle-abschnitte.szo");
const SOLL: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/src/ifc_vorschau/soll/r7.png");

/// Ein Haus wie aus dem Menü: exportiert, wieder gelesen, Fenster `w` × `h`.
fn haus(szo_text: &str, (w, h): (u32, u32)) -> IfcFenster {
    let m = szo::read(szo_text, GuidGen::with_seed(1))
        .expect("Haus lädt")
        .model;
    let opt = sk_ifc::Optionen {
        zeit: 1_791_819_528,
        datei: "r7.ifc".into(),
        version: "0.2.0".into(),
    };
    let (text, _) = sk_ifc::export(&m, &opt);
    let v = sk_ifc::lesen::vorschau(&sk_ifc::step::read(&text).expect("lesbar"));
    let mut f = IfcFenster::new(v, "r7.ifc");
    f.handle(&Event::ScaleChanged(1.0));
    f.handle(&Event::Resized {
        width: w,
        height: h,
    });
    f
}

/// r7 im Fenster 1400 × 860.
fn r7() -> IfcFenster {
    haus(R7, (1400, 860))
}

/// Nur das 3D-Bild, ohne Schrift: gleich mit und ohne Schriften.
fn bild(f: &mut IfcFenster) -> (u32, u32, Vec<u8>) {
    let ohne = Fonts {
        regular: None,
        bold: None,
        italic: None,
    };
    f.frame(&Theme::dark(), &ohne);
    let b = f.bild().expect("3D-Bild");
    (b.w as u32, b.h as u32, b.rgba.clone())
}

/// Das 3D-Bild gleicht dem Soll: je Kanal höchstens 2 Stufen daneben, das
/// bei höchstens 0,1 % der Bildpunkte.
#[test]
fn r7_wie_soll() {
    let soll = std::fs::read(SOLL)
        .unwrap_or_else(|_| panic!("Soll-Bild fehlt: {SOLL} (Entstehung siehe Kopf dieser Datei)"));
    let (sw, sh, spx) = sk_paint::decode_png(&soll).expect("Soll-Bild lesbar");
    let (w, h, px) = bild(&mut r7());
    assert_eq!((w, h), (sw, sh), "Größe");
    let daneben = px
        .chunks(4)
        .zip(spx.chunks(4))
        .filter(|(a, b)| (0..3).any(|i| a[i].abs_diff(b[i]) > 2))
        .count();
    let n = (w * h) as usize;
    assert!(
        daneben * 1000 <= n,
        "{daneben} von {n} Bildpunkten weichen ab"
    );
}

/// Ein fester Punkt im Bild wählt ein Bauteil; der Baum zeigt dasselbe
/// Bauteil unter seinem Namen, die Eigenschaften seine GUID.
#[test]
fn wahl_im_bild_wie_im_baum() {
    let mut f = r7();
    bild(&mut f);
    // Mitte des 3D-Bilds (Fensterpunkte)
    let (x, y) = (680.0, 445.0);
    let t = f
        .teil_bei(x, y)
        .expect("in der Bildmitte liegt ein Bauteil");
    let m = Modifiers::default();
    f.handle(&Event::MouseDown {
        button: MouseButton::Left,
        x,
        y,
        mods: m,
    });
    f.handle(&Event::MouseUp {
        button: MouseButton::Left,
        x,
        y,
        mods: m,
    });
    assert_eq!(f.gewaehlt, Some(t));
    let teil = &f.v.teile[t];
    let zeile = f
        .baum_zeilen()
        .into_iter()
        .find(|z| z.ziel == Ziel::Teil(t))
        .expect("im Baum aufgeklappt");
    assert_eq!(zeile.text, teil.name);
    let guid = super::eigenschaften::Eintrag::Wert("GUID".into(), teil.guid.clone());
    assert!(f.eigenschaften().contains(&guid));
}

/// Schrift wie unter Windows, sonst `SKIZZEO_SCHRIFT` oder Liberation Sans.
fn schriften() -> Fonts {
    let f = Fonts::system();
    if f.regular.is_some() {
        return f;
    }
    let pfad = std::env::var("SKIZZEO_SCHRIFT").unwrap_or_else(|_| {
        "/usr/share/fonts/truetype/liberation/LiberationSans-Regular.ttf".into()
    });
    let lade = |p: &str| std::fs::read(p).ok().and_then(sk_paint::font::Font::parse);
    Fonts {
        regular: lade(&pfad),
        bold: lade(&pfad.replace("Regular", "Bold")),
        italic: None,
    }
}

/// Ist-Bilder für Test: das ganze Fenster mit Schrift und das 3D-Bild
/// allein (Vorlage für `soll/r7.png`).
#[test]
#[ignore = "legt Ist-Bilder ab, nur mit SKIZZEO_ISTBILDER"]
fn istbilder_i3() {
    let Some(dir) = std::env::var_os("SKIZZEO_ISTBILDER").map(std::path::PathBuf::from) else {
        return;
    };
    std::fs::create_dir_all(&dir).unwrap();
    let mut f = r7();
    let (w, h, px) = bild(&mut f);
    std::fs::write(
        dir.join("ist-i3-vorschau-r7-bild.png"),
        sk_paint::encode_png_gepackt(w, h, &px),
    )
    .unwrap();
    let fonts = schriften();
    if fonts.regular.is_none() {
        return;
    }
    f.dirty = true;
    let px = f.frame(&Theme::dark(), &fonts).unwrap().0.to_vec();
    // Vormultipliziert und deckend: gleich den Farben
    std::fs::write(
        dir.join("ist-i3-vorschau-r7.png"),
        sk_paint::encode_png_gepackt(1400, 860, &px),
    )
    .unwrap();
}

/// Bildzeit beim Drehen (Review B4 zu #83, Ziel ≤ 15 ms bei 1200 × 800):
/// `cargo test --release -p skizzeo bildzeit_beim_drehen -- --ignored
/// --nocapture`.
#[test]
#[ignore = "misst Zeiten, nur im Release sinnvoll"]
fn bildzeit_beim_drehen() {
    const RH1: &str = include_str!("../../../crates/sk-cost/referenz/rh1-standardhaus.szo");
    let fonts = schriften();
    let th = Theme::dark();
    for (name, text) in [("RH-1", RH1), ("r7", R7)] {
        let mut f = haus(text, (1200, 800));
        f.frame(&th, &fonts);
        let m = Modifiers::default();
        let (x, y) = (600.0, 400.0);
        f.handle(&Event::MouseDown {
            button: MouseButton::Left,
            x,
            y,
            mods: m,
        });
        let mut zeiten = Vec::new();
        for i in 1..=40 {
            f.handle(&Event::MouseMove {
                x: x + i as f64 * 6.0,
                y: y + (i % 7) as f64,
                mods: m,
            });
            let t0 = std::time::Instant::now();
            assert!(f.frame(&th, &fonts).is_some());
            zeiten.push(t0.elapsed().as_secs_f64() * 1000.0);
        }
        zeiten.sort_by(f64::total_cmp);
        let teile = f.v.teile.len();
        let dreiecke: usize =
            f.v.teile
                .iter()
                .flat_map(|t| &t.netze)
                .map(|n| n.dreiecke.len())
                .sum();
        eprintln!(
            "{name}: {teile} Teile, {dreiecke} Dreiecke, Median {:.1} ms, max {:.1} ms",
            zeiten[20], zeiten[39]
        );
    }
}
