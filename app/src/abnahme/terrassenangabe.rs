use super::*;

// Abnahmetests A241–A242: Platzregel der Terrassenangabe im Grundriss
// (Review 3f K1, Ist/Soll p′; einstellungen/vorschlag-dachterrasse.md §15,
// Koordinator 12:02). Spezifikation: test/abnahme-dachterrasse.md.
//
// Einbau: als `mod terrassenangabe { use super::*; … }` ans Ende von
// app/src/abnahme.rs. Nutzt aus abnahme.rs: gebaeude, fit_parallel, W, H.
//
// Angenommene Namen nur in den Adaptern: `crate::terrace_label::{place,
// Input, Placement, Box}` als reine Funktion in Bildschirmpixeln und
// `crate::terrace_label::for_plan` (dieselbe Funktion, gefüttert aus
// Szene und Kamera, wie der Grundriss sie aufruft). Die zurückgegebene
// Lage ist der Textkasten samt 4 dip Rand.

use sk_model::ElementId;

// ===== Adapter =====

/// Kasten in Pixeln: links, oben, Breite, Höhe.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Kasten {
    x: f64,
    y: f64,
    w: f64,
    h: f64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Stufe {
    /// 1: voll, mittig in der Terrasse.
    Voll(Kasten),
    /// 2: voll, auf der Längsachse verschoben (Kettensymbol).
    Verschoben(Kasten),
    /// 3: Kurzform „13,22 m²“ in der Terrasse.
    Kurz(Kasten),
    /// 4: voll, außen vor der Blechkante.
    Aussen(Kasten),
    /// 5: entfällt.
    Keine,
}

/// Eine Lage im Bildschirm (Pixel, y nach unten).
struct Lage<'a> {
    /// Umriss der Terrasse.
    terrasse: &'a [[f64; 2]],
    /// Wände, Attika und Blechkante als Sperrflächen.
    sperren: &'a [Kasten],
    /// Kettenkästen (link_chip).
    ketten: &'a [Kasten],
    /// Blechkante außen: Anfang, Ende, Richtung nach außen (Einheitsvektor).
    blechkante: Option<([f64; 2], [f64; 2], [f64; 2])>,
    /// Textmaße ohne Rand: voll und kurz.
    voll: (f64, f64),
    kurz: (f64, f64),
    scale: f64,
}

fn kasten(b: crate::terrace_label::Box) -> Kasten {
    Kasten {
        x: b.x,
        y: b.y,
        w: b.w,
        h: b.h,
    }
}

fn platz(l: &Lage) -> Stufe {
    use crate::terrace_label::{place, Box, Input, Placement};
    let b = |k: &Kasten| Box {
        x: k.x,
        y: k.y,
        w: k.w,
        h: k.h,
    };
    let sperren: Vec<Box> = l.sperren.iter().map(b).collect();
    let ketten: Vec<Box> = l.ketten.iter().map(b).collect();
    match place(&Input {
        outline: l.terrasse,
        blocked: &sperren,
        chips: &ketten,
        edge: l.blechkante,
        full: l.voll,
        short: l.kurz,
        scale: l.scale,
    }) {
        Placement::Full(k) => Stufe::Voll(kasten(k)),
        Placement::Shifted(k) => Stufe::Verschoben(kasten(k)),
        Placement::Short(k) => Stufe::Kurz(kasten(k)),
        Placement::Outside(k) => Stufe::Aussen(kasten(k)),
        Placement::None => Stufe::Keine,
    }
}

/// Angaben im Grundriss des aktiven Geschosses, je Terrasse eine.
fn im_grundriss(s: &Scene, c: &crate::camera::Camera, w: f64, h: f64) -> Vec<Stufe> {
    let f = sk_ui::widgets::Fonts {
        regular: None,
        bold: None,
        italic: None,
    };
    crate::terrace_label::for_plan(s, c, w, h, 1.0, &f, &Theme::dark())
        .into_iter()
        .map(|p| match p {
            crate::terrace_label::Placement::Full(k) => Stufe::Voll(kasten(k)),
            crate::terrace_label::Placement::Shifted(k) => Stufe::Verschoben(kasten(k)),
            crate::terrace_label::Placement::Short(k) => Stufe::Kurz(kasten(k)),
            crate::terrace_label::Placement::Outside(k) => Stufe::Aussen(kasten(k)),
            crate::terrace_label::Placement::None => Stufe::Keine,
        })
        .collect()
}

// ===== Hilfen =====

fn rechteck(x0: f64, y0: f64, x1: f64, y1: f64) -> Vec<[f64; 2]> {
    vec![[x0, y0], [x1, y0], [x1, y1], [x0, y1]]
}

fn mitte(k: &Kasten) -> (f64, f64) {
    (k.x + k.w / 2.0, k.y + k.h / 2.0)
}

fn innen(k: &Kasten, x0: f64, y0: f64, x1: f64, y1: f64) -> bool {
    k.x >= x0 - 0.5 && k.y >= y0 - 0.5 && k.x + k.w <= x1 + 0.5 && k.y + k.h <= y1 + 0.5
}

/// Überdecken sich die Kästen, wenn `b` um `abstand` wächst?
fn beruehrt(a: &Kasten, b: &Kasten, abstand: f64) -> bool {
    a.x < b.x + b.w + abstand
        && b.x - abstand < a.x + a.w
        && a.y < b.y + b.h + abstand
        && b.y - abstand < a.y + a.h
}

/// Text „Dachterrasse 13,22 m²“ 124 × 16 px, „13,22 m²“ 44 × 16 px bei
/// 100 %; mit Rand 4 dip: 132 × 24 und 52 × 24.
const VOLL: (f64, f64) = (124.0, 16.0);
const KURZ: (f64, f64) = (44.0, 16.0);

/// A241 (§15, Platzregel als reine Funktion): Stufen 1–5 an
/// konstruierten Lagen, bei 100 % und 150 % (Rand 4 dip, Abstand 6 dip).
/// - nah (Terrasse 600 × 80 px): Stufe 1, voller Kasten mittig.
/// - Kette mittig (Kasten links der Mitte): Stufe 2, auf der Längsachse
///   zur Seite mit mehr Platz (rechts), mindestens 6 dip neben der Kette,
///   ganz in der Terrasse.
/// - mittel (100 × 30 px): voll passt nicht, Stufe 3 Kurzform mittig.
/// - klein mit freier Blechkante darunter: Stufe 4, voller Kasten 6 dip
///   vor der Blechkante außen, mittig auf ihrer Länge.
/// - weit (dazu Papier außen belegt): Stufe 5, entfällt.
/// - Eine Wand quer durch die Mitte wird nie überdeckt.
#[test]
fn a241_platzregel_stufen() {
    for s in [1.0, 1.5] {
        let (voll, kurz) = ((VOLL.0 * s, VOLL.1 * s), (KURZ.0 * s, KURZ.1 * s));
        let rand = 4.0 * s;
        let abstand = 6.0 * s;
        let p = |v: f64| v * s;

        // 1 nah
        let t = rechteck(p(100.0), p(300.0), p(700.0), p(380.0));
        let l = Lage {
            terrasse: &t,
            sperren: &[],
            ketten: &[],
            blechkante: None,
            voll,
            kurz,
            scale: s,
        };
        let Stufe::Voll(k) = platz(&l) else {
            panic!("nah: {:?}", platz(&l))
        };
        let (mx, my) = mitte(&k);
        assert!(
            (mx - p(400.0)).abs() < 0.5 && (my - p(340.0)).abs() < 0.5,
            "mittig {k:?}"
        );
        assert!(
            (k.w - (voll.0 + 2.0 * rand)).abs() < 0.5,
            "Rand 4 dip: {k:?}"
        );
        assert!((k.h - (voll.1 + 2.0 * rand)).abs() < 0.5);

        // 2 Kette mittig
        let kette = Kasten {
            x: p(370.0),
            y: p(330.0),
            w: p(20.0),
            h: p(20.0),
        };
        let l = Lage {
            ketten: &[kette],
            ..l
        };
        let Stufe::Verschoben(k) = platz(&l) else {
            panic!("Kette mittig: {:?}", platz(&l))
        };
        assert!(
            !beruehrt(&k, &kette, abstand - 0.5),
            "6 dip zur Kette: {k:?}"
        );
        assert!(
            innen(&k, p(100.0), p(300.0), p(700.0), p(380.0)),
            "in der Terrasse"
        );
        assert!((mitte(&k).1 - p(340.0)).abs() < 0.5, "auf der Längsachse");
        assert!(mitte(&k).0 > kette.x + kette.w, "zur Seite mit mehr Platz");

        // 3 mittel
        let t = rechteck(p(100.0), p(300.0), p(200.0), p(330.0));
        let l = Lage {
            terrasse: &t,
            ketten: &[],
            ..l
        };
        let Stufe::Kurz(k) = platz(&l) else {
            panic!("mittel: {:?}", platz(&l))
        };
        assert!((mitte(&k).0 - p(150.0)).abs() < 0.5, "{k:?}");
        assert!(innen(&k, p(100.0), p(300.0), p(200.0), p(330.0)));
        assert!((k.w - (kurz.0 + 2.0 * rand)).abs() < 0.5);

        // 4 außen vor der Blechkante
        let t = rechteck(p(370.0), p(300.0), p(430.0), p(320.0));
        let kante = ([p(100.0), p(330.0)], [p(700.0), p(330.0)], [0.0, 1.0]);
        let l = Lage {
            terrasse: &t,
            blechkante: Some(kante),
            ..l
        };
        let Stufe::Aussen(k) = platz(&l) else {
            panic!("außen: {:?}", platz(&l))
        };
        assert!(
            (k.y - (p(330.0) + abstand)).abs() < 0.5,
            "6 dip vor der Kante: {k:?}"
        );
        assert!(
            (mitte(&k).0 - p(400.0)).abs() < 0.5,
            "mittig auf ihrer Länge"
        );
        assert!((k.w - (voll.0 + 2.0 * rand)).abs() < 0.5, "voll");

        // 5 weit: Papier außen belegt
        let belegt = [Kasten {
            x: p(0.0),
            y: p(331.0),
            w: p(800.0),
            h: p(200.0),
        }];
        let l = Lage {
            sperren: &belegt,
            ..l
        };
        assert_eq!(platz(&l), Stufe::Keine, "weit");

        // Wand quer durch die Mitte der großen Terrasse
        let t = rechteck(p(100.0), p(300.0), p(700.0), p(380.0));
        let wand = Kasten {
            x: p(390.0),
            y: p(300.0),
            w: p(20.0),
            h: p(80.0),
        };
        let l = Lage {
            terrasse: &t,
            sperren: &[wand],
            ketten: &[],
            blechkante: None,
            voll,
            kurz,
            scale: s,
        };
        match platz(&l) {
            Stufe::Voll(k) | Stufe::Verschoben(k) | Stufe::Kurz(k) => {
                assert!(!beruehrt(&k, &wand, 0.0), "Wand überdeckt: {k:?}");
                assert!(innen(&k, p(100.0), p(300.0), p(700.0), p(380.0)));
            }
            x => panic!("Wand in der Mitte: {x:?}"),
        }
    }
}

fn nr(s: &Scene, nummer: &str) -> ElementId {
    s.model()
        .elements()
        .iter()
        .find(|(_, e)| e.number == nummer)
        .map(|(id, _)| id)
        .unwrap_or_else(|| panic!("{nummer} fehlt"))
}

/// A242 (§15 im Grundriss): Prüfhaus Nord −1,50, Grundriss OG. Ganzes
/// Haus im Fenster 1440 × 778: eine Angabe, Stufe 1 oder 2, ganz auf der
/// Terrasse (y-Bereich der Terrasse in Pixeln). Im kleinen Fenster
/// 200 × 110 („weit“): Stufe 5. Ohne Rücksprung keine Angabe. Im
/// Grundriss EG keine Angabe.
#[test]
fn a242_grundriss_pruefhaus() {
    let mut s = Scene::with_model(Model::with_seed(242));
    let (eg, og) = gebaeude(&mut s);
    let st = |r| s.model().run(r).unwrap().storey;
    let (st_eg, st_og) = (st(eg), st(og));
    let w = nr(&s, "AW-006");
    assert!(s.edit_model("Kopplung gelöst", |m| m.set_linked(w, false)));
    assert!(s.edit_model("Wand verschoben", |m| m.move_segment(w, -1500.0).is_some()));
    assert!(s.set_active_storey(st_og));
    let c = fit_parallel(ViewKind::Plan, s.bounds(), W, H);
    let a = im_grundriss(&s, &c, W, H);
    assert_eq!(a.len(), 1, "{a:?}");
    let k = match a[0] {
        Stufe::Voll(k) | Stufe::Verschoben(k) => k,
        x => panic!("ganzes Haus: {x:?}"),
    };
    let y0 = c.project(vec3(5000.0, 7860.0, 2995.0), W, H).unwrap().1;
    let y1 = c.project(vec3(5000.0, 6500.0, 2995.0), W, H).unwrap().1;
    let (lo, hi) = (y0.min(y1), y0.max(y1));
    assert!(
        k.y >= lo - 0.5 && k.y + k.h <= hi + 0.5,
        "auf der Terrasse: {k:?} in {lo}–{hi}"
    );

    let (w2, h2) = (200.0, 110.0);
    let c = fit_parallel(ViewKind::Plan, s.bounds(), w2, h2);
    assert_eq!(im_grundriss(&s, &c, w2, h2), [Stufe::Keine], "weit");

    assert!(s.set_active_storey(st_eg));
    let c = fit_parallel(ViewKind::Plan, s.bounds(), W, H);
    assert!(im_grundriss(&s, &c, W, H).is_empty(), "Grundriss EG");

    let mut b = Scene::with_model(Model::with_seed(2420));
    let (_, og) = gebaeude(&mut b);
    let st_og = b.model().run(og).unwrap().storey;
    assert!(b.set_active_storey(st_og));
    let c = fit_parallel(ViewKind::Plan, b.bounds(), W, H);
    assert!(im_grundriss(&b, &c, W, H).is_empty(), "ohne Rücksprung");
}
