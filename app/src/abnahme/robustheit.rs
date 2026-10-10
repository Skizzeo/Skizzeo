use super::*;

// Abnahmetests A304–A305: kleine Robustheitspunkte (nach Paket 9,
// Punkt 4; planung/nach-paket-9.md, review/bericht.md 3q, 3s, 3u,
// Koordinator 22:12). Die ungültige Musterzeile prüft schon A297.
// Vorbereitet gegen main 45eb63f.
//
// Einbau: als `mod robustheit { use super::*; … }` ans Ende von
// app/src/abnahme.rs.
//
// Angenommener Name steht NUR im Adapter: `Prefs::open_pattern(scene,
// theme, surface) -> bool` (cfg(test)), öffnet das Fenster „Muster“ für
// die Oberfläche wie der Knopf „Muster …“.

use crate::prefs::{Prefs, Win};
use sk_model::proctex::{self, Bond, Pattern};

// ===== Adapter =====

fn muster_fenster(p: &mut Prefs, s: &Scene, th: &Theme, id: sk_model::SurfaceId) -> bool {
    p.open_pattern(s, th, id)
}

// ===== A304 =====

fn mauerwerk(bond: Bond) -> Pattern {
    let mut p = proctex::factory(proctex::FACING).expect("Werksmuster Verblender");
    if let Pattern::Masonry { bond: b, .. } = &mut p {
        *b = bond;
    }
    p
}

/// A304 (nach Paket 9 Punkt 4; Review 3q „uint(row)“): Fugen unter
/// ±0,00 (Keller, Gründung mit Klinker) wie darüber.
/// - Schicht −1 ist eine eigene Schicht (Abrunden, nicht zur Null hin):
///   unter ±0,00 liegt nach einer Schichthöhe eine Lagerfuge.
/// - Der Verband wiederholt sich auch unter ±0,00: Fuge oder Stein,
///   Steinanfang und Kopf an (u, v) gleich wie eine ganze Zahl von
///   Verbandsperioden höher (halb 2, Drittel 3, Block 2, Kreuz 4,
///   wild 128 Schichten).
/// - Jede Schicht unter ±0,00 hat ihren eigenen Zufall (keine
///   Schicht doppelt wie bei einem Abschneiden negativer Reihen).
/// - Der Shader rechnet die Reihe mit `floor` und Bitmasken bzw.
///   `floor`-Rest, nie mit `%` oder Abschneiden.
#[test]
fn a304_fugen_unter_null() {
    for (bond, periode) in [
        (Bond::Half, 2),
        (Bond::Third, 3),
        (Bond::Block, 2),
        (Bond::Cross, 4),
        (Bond::Wild, 128),
    ] {
        let p = mauerwerk(bond);
        let (h, j) = (
            proctex::value(&p, "h").unwrap() as f64,
            proctex::value(&p, "joint").unwrap() as f64,
        );
        let course = h + j;
        // Schicht −1: Stein mitten in der Schicht, Fuge an ihrer Unterkante
        let mitte = proctex::masonry_probe(&p, 37.0, -course / 2.0 - j / 2.0);
        assert_eq!(mitte.map(|(_, s)| s.row), Some(-1), "{bond:?}: Schicht −1");
        assert!(
            proctex::masonry_probe(&p, 37.0, -course - j / 2.0).is_none()
                || proctex::masonry_probe(&p, 37.0, -course - j / 2.0 + 0.5).is_none(),
            "{bond:?}: Lagerfuge eine Schicht unter ±0,00"
        );
        // Verband periodisch über ±0,00 hinweg
        let hub = (periode as f64) * course * ((3000.0 / (periode as f64 * course)).ceil() + 1.0);
        let mut steine = 0;
        for i in 0..70 {
            for k in 0..60 {
                let (u, v) = (-913.0 + i as f64 * 31.7, -2900.0 + k as f64 * 47.3);
                let a = proctex::masonry_probe(&p, u, v);
                let b = proctex::masonry_probe(&p, u, v + hub);
                let schichten = (hub / course).round() as i32;
                match (a, b) {
                    (None, None) => {}
                    (Some((_, x)), Some((_, y))) => {
                        steine += 1;
                        assert_eq!(
                            (x.row + schichten, x.start, x.head),
                            (y.row, y.start, y.head),
                            "{bond:?}: Stein an ({u:.0}, {v:.0})"
                        );
                    }
                    _ => panic!("{bond:?}: Fuge und Stein vertauscht an ({u:.0}, {v:.0})"),
                }
            }
        }
        assert!(steine > 1000, "{bond:?}: genug Steine geprüft");
    }
    // eigener Zufall je Schicht unter ±0,00
    let seed = proctex::seed_of(&mauerwerk(Bond::Half));
    let z: std::collections::BTreeSet<u32> = (-40..0).map(|r| proctex::hash(r, 0, seed)).collect();
    assert_eq!(z.len(), 40, "jede Schicht unter ±0,00 eigen");
    assert!((-40..0).all(|r| proctex::hash(r, 0, seed) != proctex::hash(-r, 0, seed)));
    // Shader: Reihe abgerundet, keine Reste mit `%`, kein Abschneiden
    let glsl = sk_render::PATTERN_GLSL;
    assert!(
        glsl.contains("floor((v + j * 0.5) / course)"),
        "Reihe mit floor"
    );
    for verboten in ["row %", "row%", "int(v /", "int((v"] {
        assert!(!glsl.contains(verboten), "Shader: {verboten:?}");
    }
}

// ===== A305 =====

/// A305 (nach Paket 9 Punkt 4; Review 3u): Fenster „Muster“ offen, die
/// Oberfläche verschwindet (Rückgängig, Löschen), während eine
/// Verbandstabelle im Hintergrund gerechnet wird.
/// - Die Vorschau gibt es dann nicht mehr (`pattern_preview` = None),
///   ohne Absturz.
/// - Ist die Tabelle fertig, meldet `tick` höchstens einmal „neu
///   zeichnen“, danach nicht mehr; `wait` ist dann None: das Fenster
///   schläft und bleibt nicht auf „warten“.
#[test]
fn a305_musterfenster_ohne_oberflaeche_schlaeft() {
    let mut s = Scene::with_model(Model::with_seed(305));
    let mut th = Theme::dark();
    th.size.anim_ms = 0.0;
    let w = Win {
        w: 1440,
        h: 900,
        top: 32,
        scale: 1.0,
    };
    // eigener Startwert, dessen Tabelle sonst niemand rechnet
    let muster = proctex::with_seed(&mauerwerk(Bond::Wild), 0x00a3_0517);
    let mut p = Prefs::open(&mut s, &th);
    let mut id = None;
    assert!(p.edit(&mut s, |m| {
        let guid = m.new_guid();
        let vorlage = m
            .attr()
            .surfaces()
            .iter()
            .find(|(_, o)| o.name == proctex::FACING)
            .map(|(_, o)| o.clone())
            .expect("Verblender");
        id = Some(m.add_surface(sk_model::Surface {
            guid,
            name: "A305".into(),
            pattern: Some(muster.clone()),
            ..vorlage
        }));
        true
    }));
    let id = id.unwrap();
    assert!(
        muster_fenster(&mut p, &s, &th, id),
        "Fenster „Muster“ offen"
    );
    assert!(p.pattern_preview(&th, &w, &s).is_some(), "Vorschau da");
    // Tabelle noch nicht fertig: das Fenster wartet
    if !proctex::pattern_ready(&muster) {
        assert!(p.wait().is_some(), "wartet auf die Tabelle");
    }

    assert!(p.edit(&mut s, |m| m.remove_surface(id)), "Oberfläche weg");
    assert!(
        p.pattern_preview(&th, &w, &s).is_none(),
        "keine Vorschau ohne Oberfläche"
    );

    let start = std::time::Instant::now();
    while !proctex::pattern_ready(&muster) {
        assert!(start.elapsed().as_secs() < 30, "Tabelle fertig");
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    // Andere Tests können den Tabellenzähler weiterdrehen: nur zählen,
    // wenn er zwischen den beiden Aufrufen stehen blieb.
    let mut geprueft = false;
    for _ in 0..20 {
        let g = proctex::bond_generation();
        let _ = p.tick();
        let _ = p.pattern_preview(&th, &w, &s);
        let zweiter = p.tick();
        if proctex::bond_generation() == g {
            assert!(
                !zweiter,
                "tick meldet nach der fertigen Tabelle nicht dauernd „neu zeichnen“"
            );
            assert_eq!(p.wait(), None, "Fenster schläft");
            geprueft = true;
            break;
        }
    }
    assert!(geprueft, "Tabellenzähler stand still");
}
