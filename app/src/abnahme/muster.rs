use super::*;

// Abnahmetests A243–A253: Prozedurale Muster mit Fugenmuster (Paket 6,
// projektstruktur/paket-6-prozedural.md §5, BIM-Regeln 56–61 und 64 in
// bim/paket-6-muster.md, Review 3c P1–P5, Darstellung
// einstellungen/paket-p6-darstellung.md; Koordinator 11:29, 11:30, 11:33,
// 11:46). Spezifikation: test/abnahme-muster.md.
//
// Einbau: als `mod muster { use super::*; … }` ans Ende von
// app/src/abnahme.rs. Nutzt aus abnahme.rs: gebaeude.
//
// Angenommene Namen nur in den Adaptern (Vorlage §2, §3):
// `sk_model::proctex::{Pattern, Bond, sample, joint_lines, hash,
// brick_format, validate}`, `Surface::pattern`, `Model::set_surface_pattern`,
// `sk_render::PATTERN_GLSL`, `crate::draw_table::{pattern_use, PatternUse,
// pattern_rows, pack_rgb, unpack_rgb}`, `sk_model::compare_surfaces`.
//
// Hash (Regel 61): Die festen Werte in A243 stammen aus der abgenommenen
// Skizzenformel (einstellungen/skizzen-p6.py, `hash3`): lowbias32 auf
// (Reihe ^ seed·0x9E3779B9), plus Stein·0x85EBCA6B, noch einmal
// lowbias32. Wählt der Bauthread eine andere gleichwertige Ganzzahlformel,
// passt Test die Werte an; verbindlich ist nur, dass CPU und GLSL dieselbe
// Formel mit denselben Konstanten rechnen.
//
// BIM-Nachtrag 2 (18:25, ersetzt 18:10; Jörns Referenztexturen unter
// referenz/texturen/, Auswertung dort): Das Werksmuster des Verblenders
// ist der wilde Verband „friesisch-bunt“ (NF, Fuge 10 #D1CBC2, Kernfarben
// der Läufer 79/9/12, Streuung 7), Putz ist Reibeputz K2 #ECECED,
// Stahlbeton einfarbig #8E8E8D. Regel 58 endgültig für den wilden
// Verband (A268). Köpfe (`hpal=`), Flammung und Relief kommen erst mit
// Paket 7 (mod texturen). Die Tests mit eigenem Muster (A244–A246,
// A248–A253) bleiben beim halbsteinigen Rot.

use sk_model::proctex::{Bond, Pattern};
use sk_model::SurfaceId;

// ===== Adapter Paket 6 =====

/// Mauerwerk aus Feldern wie im Reiter „Oberflächen“ (Anteile in %).
#[allow(clippy::too_many_arguments)]
fn mw(
    len: f32,
    h: f32,
    joint: f32,
    bond: &str,
    pal: &[([u8; 3], u8)],
    jrgb: [u8; 3],
    spread_pct: f32,
    seed: u32,
) -> Pattern {
    let mut palette = [([0u8; 3], 0.0f32); 3];
    for (i, (c, a)) in pal.iter().enumerate() {
        palette[i] = (*c, *a as f32);
    }
    Pattern::Masonry {
        len,
        h,
        joint,
        bond: match bond {
            "half" => Bond::Half,
            "third" => Bond::Third,
            _ => Bond::Wild,
        },
        joint_rgb: jrgb,
        palette,
        hpal: None,
        flame: 0.0,
        fend: 64.0,
        relief: 0.0,
        spread: spread_pct,
        seed,
    }
}

fn putz(grain: f32, spread_pct: f32, seed: u32) -> Pattern {
    Pattern::Plaster {
        grain,
        spread: spread_pct,
        seed,
    }
}

fn farbe(p: &Pattern, u: f64, v: f64) -> [u8; 3] {
    sk_model::proctex::sample(p, [200, 200, 200], u, v)
}

/// Fugen als Mittellinien in einem Rechteck (mm), je ((u0, v0), (u1, v1)).
fn fugen(p: &Pattern, u0: f64, v0: f64, u1: f64, v1: f64) -> Vec<Linie> {
    sk_model::proctex::joint_lines(p, sk_math::Rect2::new(u0, v0, u1, v1))
        .into_iter()
        .map(|(a, b)| ((a.x, a.y), (b.x, b.y)))
        .collect()
}

fn hash(row: i32, col: i32, seed: u32) -> u32 {
    sk_model::proctex::hash(row, col, seed)
}

fn glsl() -> &'static str {
    sk_render::PATTERN_GLSL
}

fn format(name: &str) -> Option<(f32, f32)> {
    sk_model::proctex::brick_format(name)
}

fn gueltig(p: &Pattern) -> bool {
    sk_model::proctex::validate(p).is_ok()
}

fn muster(m: &Model, s: SurfaceId) -> Option<Pattern> {
    // Fremdes aus einer neueren Fassung zeigt kein Muster
    m.attr()
        .surface(s)
        .unwrap()
        .pattern
        .clone()
        .filter(|p| !matches!(p, Pattern::Foreign(_)))
}

fn muster_setzen(sc: &mut Scene, s: SurfaceId, p: Option<Pattern>) {
    assert!(sc.edit_model("Einstellungen geändert", |m| m.set_surface_pattern(s, p)));
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Art {
    /// Kein Muster (einfarbig bzw. Schraffur).
    Ohne,
    /// Ansicht: nur Fugenlinien in Tinte, keine Steinfarben.
    Linien,
    /// 3D: Steinfarben und Fugen.
    Farben,
}

/// Welche Musterdarstellung eine Fläche bekommt (reine Funktion hinter
/// dem Shader-Zweig): Ansicht, geschnitten, Deckkraft, Schalter „Muster
/// in 3D“.
fn musterart(view: ViewKind, cut: bool, alpha: f32, an: bool) -> Art {
    use crate::draw_table::PatternUse;
    match crate::draw_table::pattern_use(view, cut, alpha, an) {
        PatternUse::None => Art::Ohne,
        PatternUse::Lines => Art::Linien,
        PatternUse::Colors => Art::Farben,
    }
}

/// Looks-Zeilen 8–11 einer Oberfläche.
fn looks(m: &Model, s: SurfaceId) -> [[f32; 4]; 4] {
    let z = crate::draw_table::pattern_rows(m, s);
    [z[0], z[1], z[2], z[3]]
}

fn pack(c: [u8; 3]) -> f32 {
    crate::draw_table::pack_rgb(c)
}

fn unpack(f: f32) -> [u8; 3] {
    crate::draw_table::unpack_rgb(f)
}

fn abgleich(m: &Model, lib: &sk_model::Library) -> Vec<(sk_model::Guid, sk_model::TypeState)> {
    sk_model::compare_surfaces(m, lib)
}

// ===== Hilfen =====

/// Fugenlinie ((u0, v0), (u1, v1)) in mm.
type Linie = ((f64, f64), (f64, f64));

const ROT: [([u8; 3], u8); 3] = [
    ([0x8a, 0x3b, 0x2a], 40),
    ([0x9c, 0x4a, 0x33], 35),
    ([0x6e, 0x2f, 0x22], 25),
];
const FUGE: [u8; 3] = [0xd8, 0xd4, 0xcc];

/// NF 240 × 71, Fuge 10, Läufer halbsteinig, Rottöne 40/35/25.
fn nf(spread: f32, seed: u32) -> Pattern {
    mw(240.0, 71.0, 10.0, "half", &ROT, FUGE, spread, seed)
}

/// Kernfarben der Läufer „Röben Jever friesisch-bunt“ (BIM-Nachtrag 2):
/// Rot, Braun-grau, Silbergrau.
const FRIES: [([u8; 3], u8); 3] = [
    ([0x87, 0x49, 0x3c], 79),
    ([0x67, 0x55, 0x49], 9),
    ([0x7b, 0x6d, 0x65], 12),
];
const FUGE_FB: [u8; 3] = [0xd1, 0xcb, 0xc2];

/// Werksmuster des Verblenders: NF, wilder Verband, friesisch-bunt.
fn friesisch(spread: f32, seed: u32) -> Pattern {
    mw(240.0, 71.0, 10.0, "wild", &FRIES, FUGE_FB, spread, seed)
}

/// Oberfläche des Baustoffs `name`.
fn flaeche(m: &Model, name: &str) -> SurfaceId {
    m.materials()
        .iter()
        .find(|(_, x)| x.name == name)
        .map(|(_, x)| x.surface)
        .unwrap_or_else(|| panic!("{name} fehlt"))
}

fn sguid(m: &Model, s: SurfaceId) -> sk_model::Guid {
    m.attr().surface(s).unwrap().guid
}

fn lesen(t: &str) -> sk_model::szo::Loaded {
    sk_model::szo::read(t, sk_model::GuidGen::with_seed(1)).expect("öffnet")
}

fn muster_zeilen(t: &str) -> Vec<&str> {
    t.lines().filter(|l| l.starts_with("[pattern]")).collect()
}

fn csv(s: &mut Scene) -> Vec<u8> {
    let l = s.schedule().clone();
    crate::schedule_view::csv(s.model(), &l)
}

fn waagerecht(f: &[Linie]) -> Vec<f64> {
    let mut v: Vec<f64> = f
        .iter()
        .filter(|(a, b)| (a.1 - b.1).abs() < 1e-6)
        .map(|(a, _)| a.1)
        .collect();
    v.sort_by(f64::total_cmp);
    v.dedup_by(|a, b| (*a - *b).abs() < 1e-6);
    v
}

/// Stoßfugen (senkrecht) in der Reihe, deren Mitte bei `v` liegt: u-Lagen.
fn stoss(f: &[Linie], v: f64) -> Vec<f64> {
    let mut u: Vec<f64> = f
        .iter()
        .filter(|(a, b)| (a.0 - b.0).abs() < 1e-6 && a.1.min(b.1) < v && a.1.max(b.1) > v)
        .map(|(a, _)| a.0)
        .collect();
    u.sort_by(f64::total_cmp);
    u
}

/// A243 (Regel 61, Review 3c, Koordinator 11:33): Ganzzahl-Hash mit
/// festen Werten, bitgleich zur GLSL-Fassung. Die GLSL-Fassung rechnet
/// mit `uint`, enthält dieselben Konstanten und keinen
/// Gleitkomma-Hash (`sin`, `fract`).
#[test]
fn a243_hash_fest_und_wie_glsl() {
    for (row, col, seed, soll) in [
        (0, 0, 0, 0x0000_0000u32),
        (0, 0, 17, 0xd15b_3f5d),
        (1, 0, 17, 0x3fc1_3b7b),
        (0, 1, 17, 0x32c2_c8ce),
        (5, -3, 17, 0xc1d0_a80d),
        (-1, -1, 17, 0xdaa5_641e),
        (12, 4, 3, 0xcb14_3bf5),
        (123_456, -654_321, u32::MAX, 0xc596_1d58),
    ] {
        assert_eq!(hash(row, col, seed), soll, "hash({row}, {col}, {seed})");
    }
    let g = glsl().to_lowercase();
    for k in ["0x7feb352d", "0x846ca68b", "0x9e3779b9", "0x85ebca6b"] {
        assert!(g.contains(k), "GLSL ohne Konstante {k}");
    }
    assert!(g.contains("uint"), "GLSL rechnet mit uint");
    assert!(
        !g.contains("sin(") && !g.contains("fract("),
        "kein Gleitkomma-Hash"
    );
}

/// A244 (paket-6 §5.1/5.2, Regel 61): `sample` ist deterministisch:
/// gleicher Startwert → gleiche Farben; anderer Startwert → anderes
/// Steinbild (mindestens ein Viertel der Steinmitten anders). Ohne
/// Streuung trägt jede Steinmitte genau eine der drei Steinfarben, ihre
/// Anteile stimmen über 10 000 Steine auf ±3 Prozentpunkte. Auch unter
/// ±0,00 (negative Reihen).
#[test]
fn a244_sample_deterministisch_und_anteile() {
    let (rl, rh) = (250.0, 81.0);
    let mitte = |r: i32, c: i32| {
        // Steinmitte: Reihen ab ±0,00, Halbversatz in ungeraden Reihen
        let off = if r.rem_euclid(2) == 1 { 125.0 } else { 0.0 };
        let v = r as f64 * rh + 5.0 + 35.5;
        let u = c as f64 * rl - off + 5.0 + 120.0;
        (u, v)
    };
    let a = nf(6.0, 17);
    let b = nf(6.0, 18);
    let mut anders = 0;
    for r in -10..10 {
        for c in -10..10 {
            let (u, v) = mitte(r, c);
            assert_eq!(farbe(&a, u, v), farbe(&nf(6.0, 17), u, v));
            if farbe(&a, u, v) != farbe(&b, u, v) {
                anders += 1;
            }
        }
    }
    assert!(
        anders >= 100,
        "anderer Startwert, anderes Bild: {anders}/400"
    );

    let p = nf(0.0, 17);
    let mut n = [0usize; 3];
    for r in -50..50 {
        for c in 0..100 {
            let (u, v) = mitte(r, c);
            let f = farbe(&p, u, v);
            let k = ROT
                .iter()
                .position(|x| x.0 == f)
                .unwrap_or_else(|| panic!("Steinmitte {u},{v} ohne Steinfarbe: {f:?}"));
            n[k] += 1;
        }
    }
    for (k, (_, soll)) in ROT.iter().enumerate() {
        let ist = n[k] as f64 / 100.0;
        assert!(
            (ist - *soll as f64).abs() <= 3.0,
            "Farbe {k}: {ist} % statt {soll} %"
        );
    }
}

/// A245 (paket-6 §5.3/5.4, Review 3c P1/P2): Lagerfugen bei
/// v = k·(h + Fuge) ab ±0,00 (Mittellinien, auch unter ±0,00 und weit
/// oben bei 100 m), Abstand genau 81 mm. Halbsteinig: Stoßfugen je
/// Reihe im Abstand 250, Reihe n+1 um 125 = Länge/2 + Fuge/2 versetzt.
/// Drittelsteinig: Versatz 250/3 je Reihe, nach drei Reihen wieder gleich.
/// Auf einer Lagerfuge zeigt `sample` die Fugenfarbe, in der Steinmitte
/// nicht.
#[test]
fn a245_verband_und_lagerfugen() {
    let p = nf(0.0, 17);
    for (v0, v1) in [(10.0, 1010.0), (-1000.0, -10.0), (100_010.0, 101_010.0)] {
        let h = waagerecht(&fugen(&p, 0.0, v0, 1000.0, v1));
        assert!(!h.is_empty());
        for v in &h {
            let k = (v / 81.0).round();
            assert!(
                (v - k * 81.0).abs() < 0.01,
                "Lagerfuge bei {v} nicht auf k·81"
            );
        }
        for w in h.windows(2) {
            assert!((w[1] - w[0] - 81.0).abs() < 0.01, "{w:?}");
        }
    }
    let f = fugen(&p, 0.0, 0.0, 2000.0, 400.0);
    let r0 = stoss(&f, 40.5);
    let r1 = stoss(&f, 121.5);
    assert!(r0.len() >= 6 && r1.len() >= 6, "{r0:?} {r1:?}");
    for w in r0.windows(2).chain(r1.windows(2)) {
        assert!((w[1] - w[0] - 250.0).abs() < 0.01, "Stoßfugen {w:?}");
    }
    let versatz = (r1[0] - r0[0]).rem_euclid(250.0);
    assert!((versatz - 125.0).abs() < 0.01, "halbsteinig: {versatz}");

    let d = mw(240.0, 71.0, 10.0, "third", &ROT, FUGE, 0.0, 17);
    let f = fugen(&d, 0.0, 0.0, 2000.0, 400.0);
    let r: Vec<f64> = (0..4)
        .map(|n| stoss(&f, 40.5 + 81.0 * n as f64)[0])
        .collect();
    for n in 0..2 {
        let x = (r[n + 1] - r[n]).rem_euclid(250.0);
        let y = (250.0 - x).rem_euclid(250.0);
        assert!(
            (x - 250.0 / 3.0).abs() < 0.01 || (y - 250.0 / 3.0).abs() < 0.01,
            "drittelsteinig Reihe {n}: {x}"
        );
    }
    assert!(((r[3] - r[0]).rem_euclid(250.0)).min(250.0 - (r[3] - r[0]).rem_euclid(250.0)) < 0.01);

    assert_eq!(farbe(&p, 600.0, 3.0 * 81.0), FUGE, "auf der Lagerfuge");
    let u = (r0[1] + r0[2]) / 2.0;
    assert_ne!(farbe(&p, u, 40.5), FUGE, "Steinmitte");
}

/// A246 (paket-6 §5.5): `joint_lines` in einem Rechteck 1,00 × 1,00 m
/// mit NF: 12 Lagerfugen (Raster 81 mm), je Reihe die Stoßfugen im
/// Abstand 250 mm (3 bis 5 je Reihe), alle Linien im Rechteck.
#[test]
fn a246_fugenlinien_ein_meter() {
    let p = nf(6.0, 17);
    let f = fugen(&p, 0.0, 10.0, 1000.0, 1010.0);
    assert_eq!(waagerecht(&f).len(), 12, "Lagerfugen");
    for ((a, b), (c, d)) in &f {
        for (x, y) in [(*a, *b), (*c, *d)] {
            assert!((-0.01..=1000.01).contains(&x) && (9.99..=1010.01).contains(&y));
        }
    }
    for k in 0..12 {
        let v = 81.0 * (k as f64 + 0.5) + 10.0;
        let s = stoss(&f, v.min(1009.0));
        assert!((3..=5).contains(&s.len()), "Reihe {k}: {s:?}");
    }
}

/// A247 (§2.2, Regeln 59/60, F-17): `[pattern]` im Rundlauf, `.szo`
/// bleibt 4. Werks-Oberflächen Verblender und Putz tragen ihr
/// Werksmuster ab dem Öffnen (BIM-Nachtrag 2: NF 240 × 71, Fuge 10
/// #D1CBC2, wilder Verband, friesisch-bunt 79/9/12, Streuung 7; Putz
/// Körnung 2, Streuung 4) und schreiben es beim
/// Speichern; sonst ändert sich an einer alten Datei nichts. Eigenes
/// Muster an einer anderen Oberfläche und Abwahl des Werksmusters
/// (`gen=none`) überstehen Speichern und Öffnen; die Abwahl bleibt weg.
#[test]
fn a247_datei_pattern() {
    let mut s = Scene::with_model(Model::with_seed(247));
    gebaeude(&mut s);
    let (vb, pu, gb) = (
        flaeche(s.model(), "Verblender (Vormauerziegel)"),
        flaeche(s.model(), "Putz"),
        flaeche(s.model(), "Porenbeton"),
    );
    assert!(matches!(
        muster(s.model(), vb),
        Some(Pattern::Masonry { len, h, joint, bond: Bond::Wild, .. })
            if len == 240.0 && h == 71.0 && joint == 10.0
    ));
    assert!(matches!(muster(s.model(), pu), Some(Pattern::Plaster { grain, .. }) if grain == 2.0));
    assert_eq!(muster(s.model(), gb), None);
    // Paket 6: zwei Werksmuster; ab Paket 7 hat auch Beton eines (A272)
    let werk = s
        .model()
        .attr()
        .surfaces()
        .iter()
        .filter(|(_, x)| x.pattern.is_some())
        .count();
    assert!(werk >= 2, "Werksmuster: {werk}");
    let text = sk_model::szo::write(s.model());
    assert!(text.starts_with("SZO 4\n"));
    let z = muster_zeilen(&text);
    assert_eq!(z.len(), werk, "je Werksmuster eine Zeile: {z:?}");
    let zv = z
        .iter()
        .find(|l| l.contains(&format!("surface={}", sguid(s.model(), vb))))
        .expect("Verblender");
    assert!(
            zv.contains(
                "gen=masonry len=240 h=71 joint=10 bond=wild jrgb=d1cbc2 pal=87493c:79;675549:9;7b6d65:12"
            ) && zv.contains(" spread=7"),
            "{zv}"
        );
    let zp = z
        .iter()
        .find(|l| l.contains(&format!("surface={}", sguid(s.model(), pu))))
        .expect("Putz");
    assert!(zp.contains("gen=plaster grain=2 spread=4"), "{zp}");
    let alt: String = text
        .lines()
        .filter(|l| !l.starts_with("[pattern]"))
        .map(|l| format!("{l}\n"))
        .collect();
    let l = lesen(&alt);
    assert!(l.hints.is_empty(), "alte Datei: {:?}", l.hints);
    assert_eq!(
        sk_model::szo::write(&l.model),
        text,
        "Werksmuster ab dem Öffnen"
    );

    muster_setzen(&mut s, gb, Some(putz(3.0, 8.0, 5)));
    muster_setzen(&mut s, vb, None);
    let text = sk_model::szo::write(s.model());
    let z = muster_zeilen(&text);
    assert_eq!(z.len(), werk + 1, "{z:?}");
    assert!(z
        .iter()
        .any(|l| *l == format!("[pattern] surface={} gen=none", sguid(s.model(), vb))));
    let l = lesen(&text);
    assert!(l.hints.is_empty(), "{:?}", l.hints);
    assert_eq!(sk_model::szo::write(&l.model), text, "Rundlauf");
    // Nach dem Lesen sind die SurfaceIds andere (Guid-Reihenfolge)
    let (vb, gb) = (
        flaeche(&l.model, "Verblender (Vormauerziegel)"),
        flaeche(&l.model, "Porenbeton"),
    );
    assert_eq!(muster(&l.model, vb), None, "Abwahl bleibt weg");
    assert!(matches!(muster(&l.model, gb), Some(Pattern::Plaster { grain, .. }) if grain == 3.0));
}

/// A248 (§6, plan B7): Rücknahme von 6a. `[pattern]`-Zeilen (beide
/// Arten, `gen=none`) bleiben auf dem Stand vor Paket 6 (mit F-17b)
/// bytegleich und in Reihenfolge. Läuft ohne Adapter. Verglichen werden
/// nur die Zeilen dieser Oberfläche, denn ab 6a schreibt das Modell
/// zusätzlich die Werksmuster von Verblender und Putz (A247).
#[test]
fn a248_pattern_ruecknahme() {
    let mut s = Scene::with_model(Model::with_seed(248));
    gebaeude(&mut s);
    let text = sk_model::szo::write(s.model());
    let g = s.model().attr().surfaces().iter().next().unwrap().1.guid;
    let zeilen = format!(
            "[pattern] surface={g} gen=masonry len=240 h=71 joint=10 bond=half jrgb=d8d4cc pal=8a3b2a:40;9c4a33:35;6e2f22:25 spread=6 seed=17\n\
             [pattern] surface={g} gen=none\n"
        );
    let l = lesen(&format!("{text}{zeilen}"));
    assert!(l.model.check().is_empty());
    let neu = sk_model::szo::write(&l.model);
    let neu_g: Vec<&str> = muster_zeilen(&neu)
        .into_iter()
        .filter(|z| z.contains(&format!("surface={g}")))
        .collect();
    assert_eq!(neu_g, zeilen.lines().collect::<Vec<_>>());
}

/// A249 (§2.2, §5.7): `.szk` mit `[pattern]`, Rundlauf bytegleich; der
/// Abgleich meldet „gleich“ und bei anderem Startwert „abweichend“.
#[test]
fn a249_firmenkatalog_abgleich() {
    let mut s = Scene::with_model(Model::with_seed(249));
    gebaeude(&mut s);
    let gb = flaeche(s.model(), "Porenbeton");
    muster_setzen(&mut s, gb, Some(nf(6.0, 17)));
    let g = sguid(s.model(), gb);
    let mut lib = sk_model::Library::default();
    sk_model::export_surface(s.model(), &mut lib, gb);
    let text = sk_model::write_szk(&lib);
    assert_eq!(muster_zeilen(&text).len(), 1, "{text}");
    let lib = sk_model::read_szk(&text).expect("liest");
    assert_eq!(sk_model::write_szk(&lib), text);
    let stand = |m: &Model| {
        abgleich(m, &lib)
            .into_iter()
            .find(|x| x.0 == g)
            .map(|x| x.1)
    };
    assert_eq!(stand(s.model()), Some(sk_model::TypeState::Same));
    muster_setzen(&mut s, gb, Some(nf(6.0, 18)));
    assert_eq!(
        stand(s.model()),
        Some(sk_model::TypeState::Differs),
        "anderer Startwert"
    );
}

/// A250 (Regeln 57, 58): Formate NF 240×71, DF 240×52, 2DF 240×113,
/// WF 210×50, WDF 210×65. Grenzen: Fuge 6–15, Streuung Mauerwerk 0–20,
/// Putz 0–10, Körnung 0,5–5, frei 50–600 × 20–300, höchstens drei
/// Farben, ganzzahlige Anteile mit Summe 100. Beim Laden wird ein Wert
/// außerhalb mit Hinweis verworfen, die Oberfläche ist dann ohne Muster.
#[test]
fn a250_formate_und_grenzen() {
    for (n, m) in [
        ("NF", (240.0, 71.0)),
        ("DF", (240.0, 52.0)),
        ("2DF", (240.0, 113.0)),
        ("WF", (210.0, 50.0)),
        ("WDF", (210.0, 65.0)),
    ] {
        assert_eq!(format(n), Some(m), "{n}");
    }
    assert_eq!(format("XL"), None);
    let ein = [([0x8a, 0x3b, 0x2a], 100u8)];
    let ok = |len, h, joint, pal: &[([u8; 3], u8)], spread| {
        gueltig(&mw(len, h, joint, "wild", pal, FUGE, spread, 1))
    };
    assert!(ok(50.0, 20.0, 6.0, &ein, 0.0) && ok(600.0, 300.0, 15.0, &ein, 20.0));
    assert!(ok(240.0, 71.0, 10.0, &ROT, 6.0));
    for (len, h, joint, spread, warum) in [
        (49.0, 71.0, 10.0, 6.0, "Länge < 50"),
        (601.0, 71.0, 10.0, 6.0, "Länge > 600"),
        (240.0, 19.0, 10.0, 6.0, "Höhe < 20"),
        (240.0, 301.0, 10.0, 6.0, "Höhe > 300"),
        (240.0, 71.0, 5.0, 6.0, "Fuge < 6"),
        (240.0, 71.0, 16.0, 6.0, "Fuge > 15"),
        (240.0, 71.0, 10.0, 21.0, "Streuung > 20"),
    ] {
        assert!(!ok(len, h, joint, &ROT, spread), "{warum}");
    }
    let summe99 = [([1, 2, 3], 40u8), ([4, 5, 6], 35), ([7, 8, 9], 24)];
    assert!(!ok(240.0, 71.0, 10.0, &summe99, 6.0), "Summe 99");
    assert!(gueltig(&putz(0.5, 0.0, 1)) && gueltig(&putz(5.0, 10.0, 1)));
    assert!(!gueltig(&putz(0.4, 4.0, 1)) && !gueltig(&putz(5.1, 4.0, 1)));
    assert!(!gueltig(&putz(1.5, 11.0, 1)), "Putz-Streuung > 10");

    let mut s = Scene::with_model(Model::with_seed(250));
    gebaeude(&mut s);
    let gb = flaeche(s.model(), "Porenbeton");
    let g = sguid(s.model(), gb);
    let ohne: String = sk_model::szo::write(s.model());
    let gut = format!(
            "[pattern] surface={g} gen=masonry len=50 h=20 joint=6 bond=wild jrgb=d8d4cc pal=8a3b2a:100 spread=0 seed=1"
        );
    let l = lesen(&format!("{ohne}{gut}\n"));
    assert!(l.hints.is_empty(), "{:?}", l.hints);
    assert!(muster(&l.model, flaeche(&l.model, "Porenbeton")).is_some());
    for falsch in [
        gut.replace("len=50", "len=40"),
        gut.replace("h=20", "h=310"),
        gut.replace("joint=6", "joint=5"),
        gut.replace("spread=0", "spread=25"),
        gut.replace("pal=8a3b2a:100", "pal=8a3b2a:99"),
        gut.replace("pal=8a3b2a:100", "pal=8a3b2a:40.5;9c4a33:59.5"),
        gut.replace(
            "pal=8a3b2a:100",
            "pal=8a3b2a:25;9c4a33:25;6e2f22:25;111111:25",
        ),
        format!("[pattern] surface={g} gen=plaster grain=6 spread=4 seed=3"),
        format!("[pattern] surface={g} gen=plaster grain=1.5 spread=11 seed=3"),
    ] {
        let l = lesen(&format!("{ohne}{falsch}\n"));
        assert!(!l.hints.is_empty(), "Hinweis: {falsch}");
        assert_eq!(
            muster(&l.model, flaeche(&l.model, "Porenbeton")),
            None,
            "{falsch}"
        );
    }
}

/// A251 (Regeln 59, 64, F-17): Höchstens ein `[pattern]` je Oberfläche:
/// bei einer zweiten Zeile gilt die erste, mit Hinweis. Unbekannte
/// Oberfläche: Hinweis, verworfen. Unbekanntes `gen=` und unbekannte
/// Schlüssel bleiben bytegleich (die Werkszeile „friesisch-bunt“ aus
/// Paket 7 mit `hpal=`, `flame=`, `fend=`, `relief=`; ab Paket 7 sind sie
/// bekannt und die Zeile bleibt gleich, wenn der Bau sie in dieser
/// Reihenfolge schreibt). Regel 64: Steht eine fremde Zeile
/// (`gen=tiles` aus einer neueren Fassung) an einer Oberfläche, und der
/// Nutzer setzt dort ein eigenes Muster bzw. „ohne“, steht nach dem
/// Speichern genau eine `[pattern]`-Zeile für diese Oberfläche.
#[test]
fn a251_eine_zeile_je_oberflaeche() {
    let mut s = Scene::with_model(Model::with_seed(251));
    gebaeude(&mut s);
    let (gb, vb) = (
        flaeche(s.model(), "Porenbeton"),
        flaeche(s.model(), "Verblender (Vormauerziegel)"),
    );
    let (g, v) = (sguid(s.model(), gb), sguid(s.model(), vb));
    let text = sk_model::szo::write(s.model());
    let erste = format!("[pattern] surface={g} gen=plaster grain=2 spread=4 seed=3");
    let zweite = format!("[pattern] surface={g} gen=plaster grain=4 spread=4 seed=3");
    let l = lesen(&format!("{text}{erste}\n{zweite}\n"));
    assert_eq!(l.hints.len(), 1, "{:?}", l.hints);
    assert!(matches!(
        muster(&l.model, flaeche(&l.model, "Porenbeton")),
        Some(Pattern::Plaster { grain, .. }) if grain == 2.0
    ));

    let fremd = "[pattern] surface=00000000-0000-0000-0000-00000000abcd gen=plaster grain=2 spread=4 seed=3";
    let l = lesen(&format!("{text}{fremd}\n"));
    assert_eq!(l.hints.len(), 1, "{:?}", l.hints);
    assert!(!sk_model::szo::write(&l.model).contains("abcd"));

    let zukunft = format!("[pattern] surface={g} gen=zukunft a=1 b=\"x\"");
    let schluessel = format!(
            "[pattern] surface={v} gen=masonry len=240 h=71 joint=10 bond=wild jrgb=d1cbc2 pal=87493c:79;675549:9;7b6d65:12 hpal=87493c:50;675549:50 flame=100 fend=64 relief=100 spread=7 seed=17"
        );
    let ohne_v: String = text
        .lines()
        .filter(|l| !(l.starts_with("[pattern]") && l.contains(&format!("surface={v}"))))
        .map(|l| format!("{l}\n"))
        .collect();
    let neu = sk_model::szo::write(&lesen(&format!("{ohne_v}{zukunft}\n{schluessel}\n")).model);
    assert!(neu.lines().any(|l| l == zukunft), "unbekanntes gen= bleibt");
    assert!(
        neu.lines().any(|l| l == schluessel),
        "unbekannter Schlüssel bleibt"
    );

    // Regel 64
    let tiles_g = format!("[pattern] surface={g} gen=tiles w=300 h=300 joint=3");
    let tiles_v = format!("[pattern] surface={v} gen=tiles w=300 h=300 joint=3");
    let ohne_v_tiles = format!("{ohne_v}{tiles_g}\n{tiles_v}\n");
    let mut w = Scene::with_model(lesen(&ohne_v_tiles).model);
    let (gb2, vb2) = (
        flaeche(w.model(), "Porenbeton"),
        flaeche(w.model(), "Verblender (Vormauerziegel)"),
    );
    muster_setzen(&mut w, gb2, Some(nf(6.0, 9)));
    muster_setzen(&mut w, vb2, None);
    let neu = sk_model::szo::write(w.model());
    let fuer = |guid: sk_model::Guid| -> Vec<&str> {
        muster_zeilen(&neu)
            .into_iter()
            .filter(|l| l.contains(&format!("surface={guid}")))
            .collect()
    };
    let zg = fuer(g);
    assert_eq!(zg.len(), 1, "eigenes Muster: {zg:?}");
    assert!(zg[0].contains("gen=masonry"));
    assert_eq!(
        fuer(v),
        [format!("[pattern] surface={v} gen=none").as_str()],
        "„ohne“"
    );
}

/// A252 (Regel 56): Ein Muster ändert keine Mengen, Guids, Nummern,
/// Schichten, Gewerke, KG und keine Geometrie. Haus mit AW-49:
/// CSV, Nummern, Flächenzahl des 3D-Netzes und die Datei ohne
/// `[pattern]`-Zeilen sind mit, ohne und mit anderem Muster gleich.
#[test]
fn a252_muster_aendert_keine_mengen() {
    let mut s = Scene::with_model(Model::with_seed(252));
    let (eg, _) = gebaeude(&mut s);
    let t = s
        .model()
        .layer_sets()
        .iter()
        .find(|(_, t)| t.code == "AW-49")
        .map(|(id, _)| id)
        .unwrap();
    assert!(s.edit_model("Wandtyp geändert", |m| m.set_run_type(eg, t)));
    let vb = flaeche(s.model(), "Verblender (Vormauerziegel)");
    let ohne_muster = |s: &Scene| -> String {
        sk_model::szo::write(s.model())
            .lines()
            .filter(|l| !l.starts_with("[pattern]"))
            .map(|l| format!("{l}\n"))
            .collect()
    };
    let stand = |s: &mut Scene| {
        (
            csv(s),
            s.mesh(ViewKind::Persp, None, &[]).faces.len(),
            ohne_muster(s),
        )
    };
    let mit = stand(&mut s);
    muster_setzen(&mut s, vb, None);
    assert_eq!(stand(&mut s), mit, "ohne Muster");
    muster_setzen(
        &mut s,
        vb,
        Some(mw(210.0, 50.0, 12.0, "wild", &ROT, FUGE, 10.0, 99)),
    );
    assert_eq!(stand(&mut s), mit, "anderes Muster");
}

/// A253 (§3.2, §3.3, Review 3c, Koordinator 11:29 und 11:46): Wann ein
/// Muster gezeichnet wird, Packung der Looks-Zeilen, Werks-Stift.
/// - 3D, deckend, Schalter an: Farben; Schalter aus oder blass
///   (`u_alpha` < 1): ohne.
/// - Ansichten vorne, hinten, links, rechts, nicht geschnitten: nur
///   Linien (keine Steinfarben), auch mit Schalter aus.
/// - Schnitt (geschnitten oder dahinter), Grundriss: ohne.
/// - Farbe → f32 → Farbe für alle 2²⁴ Farben bitgleich.
/// - Looks-Zeilen 8–11 der Verblender-Oberfläche: Art 1, 240, 71, 10;
///   Versatz 0,5; Farben 1–3 und Fugenfarbe gepackt.
/// - Werks-Stift „Ansichtsmuster“ 0,13 mm, (96, 96, 96).
#[test]
fn a253_darstellung_packung_stift() {
    assert_eq!(musterart(ViewKind::Persp, false, 1.0, true), Art::Farben);
    assert_eq!(musterart(ViewKind::Persp, false, 1.0, false), Art::Ohne);
    assert_eq!(
        musterart(ViewKind::Persp, false, 0.15, true),
        Art::Ohne,
        "blass"
    );
    for v in [
        ViewKind::Front,
        ViewKind::Back,
        ViewKind::Left,
        ViewKind::Right,
    ] {
        assert_eq!(musterart(v, false, 1.0, true), Art::Linien, "{v:?}");
        assert_eq!(
            musterart(v, false, 1.0, false),
            Art::Linien,
            "{v:?} Schalter aus"
        );
        assert_eq!(musterart(v, false, 0.25, true), Art::Ohne, "{v:?} blass");
    }
    for cut in [true, false] {
        assert_eq!(
            musterart(ViewKind::Section, cut, 1.0, true),
            Art::Ohne,
            "Schnitt"
        );
        assert_eq!(
            musterart(ViewKind::Plan, cut, 1.0, true),
            Art::Ohne,
            "Grundriss"
        );
    }

    for r in 0..=255u8 {
        for g in 0..=255u8 {
            for b in 0..=255u8 {
                let c = [r, g, b];
                let f = pack(c);
                assert_eq!(f, (r as u32 * 65536 + g as u32 * 256 + b as u32) as f32);
                if unpack(f) != c {
                    panic!("{c:?} → {f} → {:?}", unpack(f));
                }
            }
        }
    }

    let mut s = Scene::with_model(Model::with_seed(253));
    gebaeude(&mut s);
    let vb = flaeche(s.model(), "Verblender (Vormauerziegel)");
    muster_setzen(&mut s, vb, Some(nf(6.0, 17)));
    let z = looks(s.model(), vb);
    assert_eq!(z[0], [1.0, 240.0, 71.0, 10.0]);
    assert_eq!(z[1][0], 0.5);
    assert_eq!(z[1][2], 17.0, "Startwert");
    assert_eq!(
        (z[2][0], z[2][2], z[3][0]),
        (pack(ROT[0].0), pack(ROT[1].0), pack(ROT[2].0))
    );
    assert_eq!(z[3][1], pack(FUGE));
    let gb = flaeche(s.model(), "Porenbeton");
    assert_eq!(looks(s.model(), gb)[0][0], 0.0, "ohne Muster: Art 0");

    let stift = s
        .model()
        .attr()
        .pens()
        .iter()
        .find(|(_, p)| p.name == "Ansichtsmuster")
        .map(|(_, p)| p.clone())
        .expect("Werks-Stift „Ansichtsmuster“");
    assert_eq!((stift.width_mm, stift.color), (0.13, [96, 96, 96]));
}

/// Stoßfugen je Reihe im Rechteck: (v der Reihenmitte, u-Lagen
/// aufsteigend). Reihen zwischen zwei Lagerfugen im Rechteck.
fn reihen(p: &Pattern, u0: f64, v0: f64, u1: f64, v1: f64) -> Vec<(f64, Vec<f64>)> {
    let f = fugen(p, u0, v0, u1, v1);
    waagerecht(&f)
        .windows(2)
        .map(|w| {
            let v = (w[0] + w[1]) / 2.0;
            (v, stoss(&f, v))
        })
        .collect()
}

/// Prüft den wilden Verband (Regel 58, BIM-Nachträge 2/3, Koordinator
/// 18:13, 18:47, 18:58, 19:02) in einem Rechteck; gibt (Köpfe, Läufer)
/// zurück. q = ¼ Stein im Achsmaß ((Länge + Fuge)/4, 62,5 mm bei NF).
/// - Zwischen zwei Stoßfugen einer Reihe liegt ein Kopf (2q) oder ein
///   Läufer (4q).
/// - Versatz ¼ Stein: Jede Stoßfuge liegt zu jeder Stoßfuge der
///   Nachbarreihe auf q + k·2q, also Überbindung mindestens q und keine
///   gemeinsame Stoßfuge.
/// - Höchstens 2 Köpfe nebeneinander, höchstens 4 Läufer in Folge.
/// - Fugentreppen (Stoßfugen in Folgeschichten je q in dieselbe
///   Richtung versetzt) über höchstens 5 Schichten.
/// - In jedem Fenster von 64 Schichten wiederholt sich keine Folge von
///   3 Schichten (gleiche Stoßfugen an gleicher Stelle).
/// - Kein linearer Verband: In je 8 aufeinanderfolgenden Schichten
///   mindestens 6 verschiedene Steinfolgen, unabhängig von der
///   Verschiebung (zwei Schichten gelten als gleich, wenn die Folge der
///   einen ohne je 2 Randsteine in der anderen vorkommt).
/// - Der Kopfanteil schwankt je Schicht: Streuung über die Schichten
///   mindestens 2 Prozentpunkte (Tabelle seed 17: 3,7).
///
/// Am Rand abgeschnittene Folgen und Treppen werden nur kürzer, darum
/// gelten die Höchstwerte auch dort.
/// - Fugenketten (dieselbe Stoßfuge in k, k + 2, k + 4, …) höchstens 4
///   Fugen lang (K = 4, Koordinator 19:32; mit K = 3 periodisch nicht
///   lösbar).
fn wild_pruefen(p: &Pattern, rechteck: (f64, f64, f64, f64)) -> (usize, usize) {
    let Pattern::Masonry { len, joint, .. } = p else {
        panic!("{p:?}")
    };
    let q = (*len as f64 + *joint as f64) / 4.0;
    let (u0, v0, u1, v1) = rechteck;
    let r = reihen(p, u0, v0, u1, v1);
    assert!(r.len() > 20, "Reihen: {}", r.len());
    let (mut k, mut l) = (0, 0);
    let mut folgen: Vec<String> = Vec::new();
    let mut anteile: Vec<f64> = Vec::new();
    for (v, xs) in &r {
        assert!(xs.len() > 5, "Reihe {v}: {xs:?}");
        let mut folge = (' ', 0);
        let mut text = String::new();
        for w in xs.windows(2) {
            let d = w[1] - w[0];
            let art = if (d - 2.0 * q).abs() < 0.01 {
                k += 1;
                'K'
            } else if (d - 4.0 * q).abs() < 0.01 {
                l += 1;
                'L'
            } else {
                panic!(
                    "Reihe {v}: Stein {d} mm (Kopf {}, Läufer {})",
                    2.0 * q,
                    4.0 * q
                )
            };
            text.push(art);
            folge = if folge.0 == art {
                (art, folge.1 + 1)
            } else {
                (art, 1)
            };
            let max = if art == 'K' { 2 } else { 4 };
            assert!(
                folge.1 <= max,
                "Reihe {v} bei {}: {} × {art} in Folge",
                w[1],
                folge.1
            );
        }
        anteile.push(text.matches('K').count() as f64 / text.len() as f64 * 100.0);
        folgen.push(text);
    }
    for w in r.windows(2) {
        let ((va, a), (_, b)) = (&w[0], &w[1]);
        for x in a {
            for y in b {
                let rest = (x - y).rem_euclid(2.0 * q);
                assert!(
                    (rest - q).abs() < 0.01,
                    "Reihe {va}, Stoßfuge {x}: zur Nachbarfuge {y} nicht um ¼ Stein versetzt"
                );
            }
        }
    }
    let da = |xs: &[f64], x: f64| xs.iter().any(|y| (y - x).abs() < 0.01);
    for d in [q, -q] {
        for (i, (v, xs)) in r.iter().enumerate() {
            for x in xs {
                let mut n = 1;
                while i + n < r.len() && da(&r[i + n].1, x + d * n as f64) {
                    n += 1;
                }
                assert!(n <= 5, "Treppe ab Reihe {v}, Fuge {x}: {n} Schichten");
            }
        }
    }
    for (i, (v, xs)) in r.iter().enumerate() {
        for x in xs {
            let mut n = 1;
            while i + 2 * n < r.len() && da(&r[i + 2 * n].1, *x) {
                n += 1;
            }
            assert!(
                n <= 4,
                "Reihe {v}, Fuge {x}: Kette über {n} Fugen (k, k + 2, …)"
            );
        }
    }
    let schicht =
        |xs: &[f64]| -> Vec<i64> { xs.iter().map(|x| (x * 10.0).round() as i64).collect() };
    let s: Vec<Vec<i64>> = r.iter().map(|(_, xs)| schicht(xs)).collect();
    for i in 0..s.len().saturating_sub(2) {
        for j in i + 1..(i + 64).min(s.len() - 2) {
            assert!(
                s[i..i + 3] != s[j..j + 3],
                "Schichten {}–{} wiederholen sich ab Reihe {}",
                r[i].0,
                r[i + 2].0,
                r[j].0
            );
        }
    }
    let innen = |t: &str| t[2..t.len() - 2].to_string();
    let gleich = |a: &str, b: &str| b.contains(&innen(a)) || a.contains(&innen(b));
    for (i, w) in folgen.windows(8).enumerate() {
        let verschieden = (0..8)
            .filter(|&a| (0..a).all(|b| !gleich(&w[a], &w[b])))
            .count();
        assert!(
            verschieden >= 6,
            "ab Reihe {}: nur {verschieden} verschiedene Folgen in 8 Schichten",
            r[i].0
        );
    }
    let n = anteile.len() as f64;
    let mittel = anteile.iter().sum::<f64>() / n;
    let streu = (anteile.iter().map(|a| (a - mittel).powi(2)).sum::<f64>() / n).sqrt();
    assert!(
        streu >= 2.0,
        "Kopfanteil je Schicht fast gleich: Streuung {streu:.2} Prozentpunkte"
    );
    (k, l)
}

/// A268 (Regel 58, BIM-Nachträge 2/3, Koordinator 18:13, 18:47, 18:58,
/// 19:02; Jörns Vorlage „Röben Jever friesisch-bunt“): wilder Verband.
/// Die Verbandstabelle ist 128 Schichten × 128 Viertel groß und ringsum
/// periodisch (projektstruktur/paket-6-prozedural.md §8.1). Geprüft wird
/// darum ein Rechteck über beide Nähte (u −1,0 … 9,5 m bei 8,0 m
/// Periode, v −1,5 … 10,9 m bei 10,368 m Periode) für 50 Startwerte und
/// zusätzlich 17 und 4711:
/// - Läufer und Köpfe mischen sich; in jeder Reihe nur diese zwei
///   Steinlängen.
/// - Versatz ¼ Stein zur Nachbarschicht (62,5 mm bei NF, 55,5 mm bei
///   WF mit Fuge 12), keine gemeinsame Stoßfuge.
/// - Höchstens 2 Köpfe nebeneinander, 4 Läufer in Folge, Treppen über
///   höchstens 5 Schichten, Fugenketten in k, k + 2, … höchstens 4 Fugen,
///   keine Folge von 3 Schichten doppelt in 64.
/// - In je 8 Schichten mindestens 6 verschiedene Steinfolgen unabhängig
///   von der Verschiebung; Kopfanteil schwankt je Schicht.
/// - Kopfanteil 29 % ± 4 Prozentpunkte nach Anzahl (Vorlage: 32 von
///   112; B7: 30 ± 3).
/// - Auch unter ±0,00 und bei anderem Format, deterministisch, anderer
///   Startwert gibt einen anderen Verband.
#[test]
fn a268_wilder_verband() {
    let ringsum = (-1000.0, -1500.0, 9500.0, 10_900.0);
    for seed in (1..=50).chain([17, 4711]) {
        let p = friesisch(7.0, seed);
        let (k, l) = wild_pruefen(&p, ringsum);
        let anteil = k as f64 / (k + l) as f64 * 100.0;
        assert!(
            (anteil - 29.0).abs() <= 4.0,
            "Startwert {seed}: Kopfanteil {anteil:.1} % ({k} Köpfe, {l} Läufer)"
        );
    }
    assert_eq!(
        fugen(&friesisch(7.0, 17), -1000.0, -500.0, 1000.0, 500.0),
        fugen(&friesisch(7.0, 17), -1000.0, -500.0, 1000.0, 500.0),
        "deterministisch"
    );
    assert_ne!(
        fugen(&friesisch(7.0, 17), 0.0, 0.0, 2000.0, 1000.0),
        fugen(&friesisch(7.0, 18), 0.0, 0.0, 2000.0, 1000.0),
        "anderer Startwert, anderer Verband"
    );
    let wf = mw(210.0, 50.0, 12.0, "wild", &ROT, FUGE, 0.0, 99);
    let (k, l) = wild_pruefen(&wf, (-3000.0, -3000.0, 3000.0, 1000.0));
    assert!(k > 0 && l > 0, "WF: {k} Köpfe, {l} Läufer");
}

/// A269 (Paket 6 §2.2 und Regel 58, BIM-Nachtrag 2): Werksmuster
/// „friesisch-bunt“ und Grundfarben.
/// - Ohne Streuung trägt jede Läufermitte genau eine der drei
///   Kernfarben, Anteile 79/9/12 über mindestens 8 000 Läufer auf ±3
///   Prozentpunkte. Köpfe (ab Paket 7 eigene Palette `hpal=`) zählen
///   hier nicht.
/// - Auf der Lagerfuge die Fugenfarbe #D1CBC2.
/// - Neue Projekte: Oberfläche „Putz“ #ECECED, „Stahlbeton“ #8E8E8D
///   (einfarbig bis Paket 7).
#[test]
fn a269_friesisch_bunt_und_grundfarben() {
    let p = friesisch(0.0, 17);
    let mut n = [0usize; 3];
    for (v, xs) in reihen(&p, 0.0, -4000.0, 25_000.0, 4700.0) {
        for w in xs.windows(2) {
            if w[1] - w[0] < 200.0 {
                continue;
            }
            let u = (w[0] + w[1]) / 2.0;
            let f = farbe(&p, u, v);
            let k = FRIES
                .iter()
                .position(|x| x.0 == f)
                .unwrap_or_else(|| panic!("Läufermitte {u},{v} ohne Kernfarbe: {f:?}"));
            n[k] += 1;
        }
    }
    let summe: usize = n.iter().sum();
    assert!(summe >= 8_000, "nur {summe} Läufer");
    for (k, (_, soll)) in FRIES.iter().enumerate() {
        let ist = n[k] as f64 / summe as f64 * 100.0;
        assert!(
            (ist - *soll as f64).abs() <= 3.0,
            "Familie {k}: {ist:.1} % statt {soll} %"
        );
    }
    assert_eq!(farbe(&p, 600.0, 3.0 * 81.0), FUGE_FB, "auf der Lagerfuge");

    let m = Model::with_seed(269);
    let grund = |name: &str| m.attr().surface(flaeche(&m, name)).unwrap().color;
    assert_eq!(grund("Putz"), [0xec, 0xec, 0xed], "Reibeputz weiß");
    assert_eq!(grund("Stahlbeton"), [0x8e, 0x8e, 0x8d], "Beton mittelgrau");
}

/// A274 (BIM-Befund 6a, Koordinator 19:31; paket-6 §2.2): Ein
/// unbekanntes Verbandswort (später etwa `bond=block` aus Paket 7a)
/// macht die Zeile nicht ungültig. Sie bleibt bytegleich erhalten, die
/// Oberfläche zeigt kein Muster. Falsche Zahlen oder Farben geben einen
/// Hinweis und kein Muster; die Zeile selbst bleibt seit Review 3s
/// (Koordinator 20:32, F-17b) ebenfalls bytegleich, siehe A297.
#[test]
fn a274_unbekannter_verband_bleibt() {
    let mut s = Scene::with_model(Model::with_seed(274));
    gebaeude(&mut s);
    let g = sguid(s.model(), flaeche(s.model(), "Porenbeton"));
    let text = sk_model::szo::write(s.model());
    let zeile = |bond: &str, len: &str, jrgb: &str| {
        format!(
                "[pattern] surface={g} gen=masonry len={len} h=71 joint=10 bond={bond} jrgb={jrgb} pal=8a3b2a:40;9c4a33:35;6e2f22:25 spread=6 seed=17"
            )
    };
    let zukunft = zeile("zukunft", "240", "d8d4cc");
    let l = lesen(&format!("{text}{zukunft}\n"));
    assert!(l.model.check().is_empty());
    assert_eq!(
        muster(&l.model, flaeche(&l.model, "Porenbeton")),
        None,
        "unbekannter Verband: ohne Muster"
    );
    let neu = sk_model::szo::write(&l.model);
    assert!(
        neu.lines().any(|z| z == zukunft),
        "Zeile bleibt bytegleich: {:?}",
        muster_zeilen(&neu)
    );
    assert_eq!(sk_model::szo::write(&lesen(&neu).model), neu, "Rundlauf");

    for falsch in [
        zeile("half", "abc", "d8d4cc"),
        zeile("half", "240", "zzzzzz"),
        zeile("zukunft", "abc", "d8d4cc"),
        zeile("zukunft", "240", "zzzzzz"),
    ] {
        let l = lesen(&format!("{text}{falsch}\n"));
        assert!(!l.hints.is_empty(), "Hinweis fehlt: {falsch}");
        assert_eq!(muster(&l.model, flaeche(&l.model, "Porenbeton")), None);
        let neu = sk_model::szo::write(&l.model);
        let eigene: Vec<&str> = muster_zeilen(&neu)
            .into_iter()
            .filter(|z| z.contains(&format!("surface={g}")))
            .collect();
        assert_eq!(eigene, [falsch.as_str()], "falsche Zeile bleibt bytegleich");
    }
}

/// A275 (BIM-Befund 6a, Koordinator 19:31; Regel 60): Das Werksmuster
/// hängt an der Guid der Werks-Oberfläche, nicht an ihrem Namen. Neues
/// Projekt (feste Guids der Startbibliothek):
/// - Eine umbenannte Werks-Oberfläche behält ihr Werksmuster, auch in
///   einer Datei ohne `[pattern]`-Zeile, und schreibt bei Abwahl
///   `gen=none`.
/// - Eine eigene Oberfläche mit dem Namen „Putz“ bekommt kein
///   Werksmuster und schreibt keine Musterzeile.
#[test]
fn a275_werksmuster_nach_guid() {
    let m = Model::new();
    let pu = flaeche(&m, "Putz");
    let g = sguid(&m, pu);
    assert!(muster(&m, pu).is_some(), "Werksmuster Putz");
    let text = sk_model::szo::write(&m);
    let eigen = sk_model::GuidGen::with_seed(275).next_guid();
    let alt = format!("[surface] guid={g} name=\"Putz\"");
    let ohne: String = text
        .lines()
        .filter(|l| !(l.starts_with("[pattern]") && l.contains(&format!("surface={g}"))))
        .map(|l| {
            if l.starts_with(&alt) {
                let neu = l.replacen("name=\"Putz\"", "name=\"Putz innen\"", 1);
                format!(
                    "{neu}\n{}\n",
                    l.replacen(&g.to_string(), &eigen.to_string(), 1)
                )
            } else {
                format!("{l}\n")
            }
        })
        .collect();
    assert!(ohne.contains("name=\"Putz innen\""), "Putz-Zeile gefunden");
    let l = lesen(&ohne);
    let s = |name: &str| {
        l.model
            .attr()
            .surfaces()
            .iter()
            .find(|(_, x)| x.name == name)
            .map(|(id, x)| (id, x.guid))
            .unwrap_or_else(|| panic!("{name} fehlt"))
    };
    let (innen, gi) = s("Putz innen");
    let (fremd, gf) = s("Putz");
    assert_eq!((gi, gf), (g, eigen));
    assert!(
        matches!(muster(&l.model, innen), Some(Pattern::Plaster { grain, .. }) if grain == 2.0),
        "umbenannte Werks-Oberfläche behält ihr Werksmuster"
    );
    assert_eq!(
        muster(&l.model, fremd),
        None,
        "eigener „Putz“ ohne Werksmuster"
    );
    let neu = sk_model::szo::write(&l.model);
    assert!(
        !neu.contains(&format!("surface={eigen}")),
        "eigener „Putz“ schreibt keine Zeile: {:?}",
        muster_zeilen(&neu)
    );

    let mut w = Scene::with_model(l.model);
    muster_setzen(&mut w, innen, None);
    let neu = sk_model::szo::write(w.model());
    assert!(
        neu.lines()
            .any(|z| z == format!("[pattern] surface={g} gen=none")),
        "Abwahl der umbenannten Werks-Oberfläche: {:?}",
        muster_zeilen(&neu)
    );
    let l = lesen(&neu);
    assert_eq!(muster(&l.model, s_id(&l.model, g)), None, "Abwahl bleibt");
}

/// A296 (Review 3s, Koordinator 20:32): Mit 17 und mehr verschiedenen
/// wilden Startwerten kommt die Hintergrundrechnung zur Ruhe, und jede
/// Tabelle wird nur einmal gerechnet. Ursache war der Speicher für nur
/// 16 Tabellen: Die 17. verdrängte die älteste, die beim nächsten
/// Neuaufbau wieder bestellt wurde, ohne Ende.
/// - 20 Startwerte, die sonst kein Test nutzt, werden wie im Zeichenpfad
///   immer wieder mit `bond_table_ready` abgefragt, bis alle 20 in einem
///   Durchgang da sind (höchstens 60 s).
/// - Danach kommen bei weiteren Durchgängen, auch über `bond_table`,
///   dieselben Tabellen zurück (gleicher Zeiger, also nicht neu
///   gerechnet), und für sie läuft nichts mehr im Hintergrund.
#[test]
fn a296_viele_wilde_startwerte_kommen_zur_ruhe() {
    use sk_model::proctex::{bond_table, bond_table_ready, bond_tables_pending};
    use std::sync::Arc;
    let seeds: Vec<u32> = (0..20).map(|i| 0x00a2_9600 + i).collect();
    let bis = std::time::Instant::now() + std::time::Duration::from_secs(60);
    let erste = loop {
        let r: Vec<_> = seeds.iter().map(|s| bond_table_ready(*s)).collect();
        if r.iter().all(Option::is_some) {
            break r.into_iter().flatten().collect::<Vec<_>>();
        }
        let fehlt = r.iter().filter(|x| x.is_none()).count();
        assert!(
            std::time::Instant::now() < bis,
            "nach 60 s fehlen noch {fehlt} von 20 Tabellen"
        );
        std::thread::sleep(std::time::Duration::from_millis(20));
    };
    for runde in 0..3 {
        for (s, t) in seeds.iter().zip(&erste) {
            let r = bond_table_ready(*s).unwrap_or_else(|| {
                panic!("Runde {runde}: Tabelle {s:#x} verdrängt und neu bestellt")
            });
            assert!(Arc::ptr_eq(&r, t), "Runde {runde}: {s:#x} neu gerechnet");
            assert!(Arc::ptr_eq(&bond_table(*s), t), "bond_table {s:#x}");
        }
    }
    // Andere Tests können eigene Tabellen bestellen; die Rechnung muss
    // trotzdem in kurzer Zeit zur Ruhe kommen.
    let bis = std::time::Instant::now() + std::time::Duration::from_secs(60);
    while bond_tables_pending() {
        assert!(
            std::time::Instant::now() < bis,
            "Hintergrund kommt nicht zur Ruhe"
        );
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    for (s, t) in seeds.iter().zip(&erste) {
        assert!(Arc::ptr_eq(&bond_table_ready(*s).unwrap(), t), "{s:#x}");
    }
}

/// A297 (Review 3s zu 3q/3, Koordinator 20:32; F-17b): Eine ungültige
/// `[pattern]`-Zeile (etwa `joint=20`, für eine neuere Fassung gültig)
/// übersteht Laden und Speichern bytegleich, wie ein unbekanntes
/// Verbandswort (A274). Beim Lesen gibt es einen Hinweis, die Oberfläche
/// hat kein Muster, auch eine Werks-Oberfläche bekommt keins
/// untergeschoben, und es entsteht keine zweite Zeile (`gen=none`) für
/// sie. Erst ein neu gesetztes Muster ersetzt die Zeile (Regel 64).
#[test]
fn a297_ungueltige_musterzeile_bleibt() {
    let m = Model::new();
    let text = sk_model::szo::write(&m);
    let gb = sguid(&m, flaeche(&m, "Porenbeton"));
    let vb = sguid(&m, flaeche(&m, "Verblender (Vormauerziegel)"));
    let ziegel = |g: sk_model::Guid, joint: &str| {
        format!(
                "[pattern] surface={g} gen=masonry len=240 h=71 joint={joint} bond=half jrgb=d8d4cc pal=8a3b2a:40;9c4a33:35;6e2f22:25 spread=6 seed=17"
            )
    };
    let a = ziegel(gb, "20");
    let b = ziegel(vb, "20");
    let c = format!("[pattern] surface={gb} gen=plaster grain=9 spread=4 seed=5");
    for (falsch, g) in [(&a, gb), (&b, vb), (&c, gb)] {
        let ohne: String = text
            .lines()
            .filter(|l| !(l.starts_with("[pattern]") && l.contains(&format!("surface={g}"))))
            .map(|l| format!("{l}\n"))
            .collect();
        let l = lesen(&format!("{ohne}{falsch}\n"));
        assert!(!l.hints.is_empty(), "Hinweis fehlt: {falsch}");
        assert!(l.model.check().is_empty());
        let s = s_id(&l.model, g);
        assert_eq!(muster(&l.model, s), None, "kein Muster: {falsch}");
        let neu = sk_model::szo::write(&l.model);
        let eigene: Vec<&str> = muster_zeilen(&neu)
            .into_iter()
            .filter(|z| z.contains(&format!("surface={g}")))
            .collect();
        assert_eq!(eigene, [falsch.as_str()], "bytegleich, keine zweite Zeile");
        let l2 = lesen(&neu);
        assert_eq!(muster(&l2.model, s_id(&l2.model, g)), None);
        assert_eq!(sk_model::szo::write(&l2.model), neu, "Rundlauf");

        let mut w = Scene::with_model(l2.model);
        let s = s_id(w.model(), g);
        muster_setzen(&mut w, s, Some(sk_model::proctex::plaster_default()));
        let neu = sk_model::szo::write(w.model());
        let eigene: Vec<&str> = muster_zeilen(&neu)
            .into_iter()
            .filter(|z| z.contains(&format!("surface={g}")))
            .collect();
        assert_eq!(eigene.len(), 1, "eine Zeile je Oberfläche: {eigene:?}");
        assert!(eigene[0].contains("gen=plaster"), "{}", eigene[0]);
        assert!(
            !neu.contains(falsch.as_str()),
            "neues Muster ersetzt die Zeile"
        );
    }
}

fn s_id(m: &Model, g: sk_model::Guid) -> SurfaceId {
    m.attr()
        .surfaces()
        .iter()
        .find(|(_, x)| x.guid == g)
        .map(|(id, _)| id)
        .expect("Oberfläche")
}
