use super::*;

// Abnahmetests A254–A264: Texturgenerator (Paket 7,
// projektstruktur/paket-7-texturgenerator.md §2 und §4, BIM-Regeln 62–66
// in bim/paket-7-texturgenerator.md, Review 3d N1–N4, Darstellung
// einstellungen/paket-p7-darstellung.md; Koordinator 11:31, 11:33).
// Spezifikation: test/abnahme-texturen.md.
//
// Einbau: als `mod texturen { use super::*; … }` ans Ende von
// app/src/abnahme.rs, nach `mod muster` (Paket 6). Nutzt aus abnahme.rs:
// gebaeude.
//
// Angenommene Namen nur in den Adaptern: die neuen Arten in
// `proctex::Pattern` (`Concrete`, `Timber`, `Tiles`, `Stone`),
// `Bond::{Block, Cross}`, `proctex::{presets, PatternPreset, limits}`,
// `sk_model::save_preset`, `Library::presets`. Alles Übrige wie Paket 6.
// Die meisten Fälle nehmen ihre Muster aus den Werksvorlagen, damit die
// Feldnamen der neuen Arten nur an wenigen Stellen stehen.
//
// CPU ↔ GPU (paket-7 §4.6) braucht ein Fenster mit GL und ist darum kein
// Unit-Test: Bildprüfung B7 in test/abnahme-texturen.md.
//
// BIM-Nachträge 18:10, 2 (18:25) und 3 (18:40) zu Jörns Referenztexturen
// (referenz/texturen/, Auswertung dort; Regeln 68–70, Entscheidung 31
// erweitert; Koordinator 18:36): Die Werksvorlagen „Klinker
// friesisch-bunt“, „Reibeputz weiß“ und „Sichtbeton mittelgrau“ ersetzen
// „Klinker bunt“, „Putz fein“ und „Sichtbeton“ und sind die Werksmuster
// von Verblender, Putz und Stahlbeton (A254, A272). Neue Schlüssel am
// Mauerwerk: `hpal=` (Köpfe), `flame=`, `fend=` (A270), `relief=`
// (Regel 70, nur Darstellung); am Beton `pores=` (A271), Stoßbreite 0
// erlaubt. A273 vergleicht Kennwerte mit der Auswertung, bis
// Einstellungen die B7-Kennwerte liefert. Angenommene Feldnamen nur in
// den Adaptern `mit_flammung`, `flammung`, `kopfpalette`,
// `ohne_streuung`, `ohne_relief`, `mit_poren`, `mit_tafeln`; `hpal`
// als `Option` der Palette.

use sk_model::proctex::{Bond, Pattern, PatternPreset};
use sk_model::SurfaceId;

// ===== Adapter Paket 7 =====

/// Die neun Werksvorlagen in Listenreihenfolge.
fn vorlagen() -> &'static [PatternPreset] {
    sk_model::proctex::presets()
}

fn vorlage(name: &str) -> &'static PatternPreset {
    vorlagen()
        .iter()
        .find(|p| p.name == name)
        .unwrap_or_else(|| panic!("Werksvorlage {name} fehlt"))
}

/// Dasselbe Muster mit anderem Startwert.
fn mit_startwert(p: &Pattern, s: u32) -> Pattern {
    let mut p = p.clone();
    match &mut p {
        Pattern::Masonry { seed, .. }
        | Pattern::Plaster { seed, .. }
        | Pattern::Concrete { seed, .. }
        | Pattern::Timber { seed, .. }
        | Pattern::Tiles { seed, .. }
        | Pattern::Stone { seed, .. } => *seed = s,
        Pattern::Foreign(_) => {}
    }
    p
}

/// Platten mit Halbversatz statt Kreuzfuge.
fn halbversatz(p: &Pattern) -> Pattern {
    let mut p = p.clone();
    if let Pattern::Tiles { half, .. } = &mut p {
        *half = true;
    }
    p
}

/// Holzschalung mit waagerechten Brettern.
fn waagerechte_bretter(p: &Pattern) -> Pattern {
    let mut p = p.clone();
    if let Pattern::Timber { vertical, .. } = &mut p {
        *vertical = false;
    }
    p
}

/// Sichtbeton mit Ankerlöchern.
fn mit_ankern(p: &Pattern) -> Pattern {
    let mut p = p.clone();
    if let Pattern::Concrete { anchors, .. } = &mut p {
        *anchors = true;
    }
    p
}

/// Mauerwerk mit Flammung (Regel 68, Anteil der Steinlänge in %).
fn mit_flammung(p: &Pattern, x: f32) -> Pattern {
    let mut p = p.clone();
    if let Pattern::Masonry { flame, .. } = &mut p {
        *flame = x;
    }
    p
}

fn flammung(p: &Pattern) -> f32 {
    match p {
        Pattern::Masonry { flame, .. } => *flame,
        _ => panic!("kein Mauerwerk: {p:?}"),
    }
}

/// (flame, fend, relief) eines Mauerwerks.
fn flammung_alle(p: &Pattern) -> (f32, f32, f32) {
    match p {
        Pattern::Masonry {
            flame,
            fend,
            relief,
            ..
        } => (*flame, *fend, *relief),
        _ => panic!("kein Mauerwerk: {p:?}"),
    }
}

/// Palette der Köpfe (`hpal=`), ganzzahlige Anteile; ohne `hpal=` leer.
fn kopfpalette(p: &Pattern) -> Vec<([u8; 3], u8)> {
    match p {
        Pattern::Masonry { hpal, .. } => hpal
            .iter()
            .flatten()
            .filter(|(_, a)| *a > 0.0)
            .map(|(c, a)| (*c, *a as u8))
            .collect(),
        _ => panic!("kein Mauerwerk: {p:?}"),
    }
}

/// Mauerwerk ohne Relief (Regel 70).
fn ohne_relief(p: &Pattern) -> Pattern {
    let mut p = p.clone();
    if let Pattern::Masonry { relief, .. } = &mut p {
        *relief = 0.0;
    }
    p
}

/// Mauerwerk ohne Streuung zwischen Steinen derselben Familie.
fn ohne_streuung(p: &Pattern) -> Pattern {
    let mut p = p.clone();
    if let Pattern::Masonry { spread, .. } = &mut p {
        *spread = 0.0;
    }
    p
}

/// Sichtbeton mit Porenanteil in % (Regel 69).
fn mit_poren(p: &Pattern, x: f32) -> Pattern {
    let mut p = p.clone();
    if let Pattern::Concrete { pores, .. } = &mut p {
        *pores = x;
    }
    p
}

/// Sichtbeton mit Schaltafeln w × h und Stoßbreite.
fn mit_tafeln(p: &Pattern, tw: f32, th: f32, stoss: f32) -> Pattern {
    let mut p = p.clone();
    if let Pattern::Concrete { w, h, joint, .. } = &mut p {
        (*w, *h, *joint) = (tw, th, stoss);
    }
    p
}

/// Mauerwerk mit Verband „block“ oder „cross“.
fn verband(p: &Pattern, v: &str) -> Pattern {
    let mut p = p.clone();
    if let Pattern::Masonry { bond, .. } = &mut p {
        *bond = if v == "block" {
            Bond::Block
        } else {
            Bond::Cross
        };
    }
    p
}

/// Fugenfarbe von Platten und Naturstein.
fn fugenfarbe(p: &Pattern) -> [u8; 3] {
    match p {
        Pattern::Tiles { joint_rgb, .. } | Pattern::Stone { joint_rgb, .. } => *joint_rgb,
        _ => panic!("ohne Fugenfarbe: {p:?}"),
    }
}

/// Grenzen der Regler je `gen=`-Wort aus der Tabelle in `proctex.rs`
/// (Regel 62: Test liest dieselbe Tabelle): (Schlüssel, kleinster,
/// größter Wert), so wie sie in der Datei stehen.
fn grenzen(gen: &str) -> Vec<(&'static str, f32, f32)> {
    sk_model::proctex::limits(gen).to_vec()
}

/// „Als Vorlage speichern …“ in den Firmenkatalog; `false` = abgelehnt.
fn speichern(lib: &mut sk_model::Library, name: &str, p: &Pattern, base: [u8; 3]) -> bool {
    sk_model::save_preset(lib, name, p, base).is_ok()
}

/// Firmenvorlagen im Katalog: (Guid, Name).
fn firmenvorlagen(lib: &sk_model::Library) -> Vec<(sk_model::Guid, String)> {
    lib.presets
        .iter()
        .map(|p| (p.guid, p.name.clone()))
        .collect()
}

fn loeschen(lib: &mut sk_model::Library, g: sk_model::Guid) {
    lib.presets.retain(|p| p.guid != g);
}

/// Vorlage in eine Oberfläche übernehmen (Fenster „Muster“, OK).
fn anwenden(sc: &mut Scene, s: SurfaceId, v: &PatternPreset) {
    let p = v.pattern.clone();
    assert!(sc.edit_model("Einstellungen geändert", |m| m
        .set_surface_pattern(s, Some(p))));
}

// ===== Adapter aus Paket 6 (gleich wie `mod muster`) =====

fn farbe(p: &Pattern, u: f64, v: f64) -> [u8; 3] {
    farbe_auf(p, [200, 200, 200], u, v)
}

fn farbe_auf(p: &Pattern, base: [u8; 3], u: f64, v: f64) -> [u8; 3] {
    sk_model::proctex::sample(p, base, u, v)
}

fn fugen(p: &Pattern, u0: f64, v0: f64, u1: f64, v1: f64) -> Vec<Linie> {
    sk_model::proctex::joint_lines(p, sk_math::Rect2::new(u0, v0, u1, v1))
        .into_iter()
        .map(|(a, b)| ((a.x, a.y), (b.x, b.y)))
        .collect()
}

fn gueltig(p: &Pattern) -> bool {
    sk_model::proctex::validate(p).is_ok()
}

fn muster(m: &Model, s: SurfaceId) -> Option<Pattern> {
    // Fremdes und unverändert Behaltenes zeigt kein Muster (wie mod muster)
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

// ===== Hilfen =====

/// Fugenlinie ((u0, v0), (u1, v1)) in mm.
type Linie = ((f64, f64), (f64, f64));

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

fn zeilen<'a>(t: &'a str, kopf: &str) -> Vec<&'a str> {
    t.lines().filter(|l| l.starts_with(kopf)).collect()
}

fn csv(s: &mut Scene) -> Vec<u8> {
    let l = s.schedule().clone();
    crate::schedule_view::csv(s.model(), &l)
}

fn rgb(h: u32) -> [u8; 3] {
    [(h >> 16) as u8, (h >> 8) as u8, h as u8]
}

/// Waagerechte Fugen: v-Lagen, sortiert, ohne Doppel.
fn waagerecht(f: &[Linie]) -> Vec<f64> {
    lagen(
        f.iter()
            .filter(|(a, b)| (a.1 - b.1).abs() < 1e-6)
            .map(|(a, _)| a.1),
    )
}

/// Senkrechte Fugen, die die Höhe `v` kreuzen: u-Lagen.
fn senkrecht_bei(f: &[Linie], v: f64) -> Vec<f64> {
    lagen(
        f.iter()
            .filter(|(a, b)| (a.0 - b.0).abs() < 1e-6 && a.1.min(b.1) < v && a.1.max(b.1) > v)
            .map(|(a, _)| a.0),
    )
}

fn senkrecht(f: &[Linie]) -> Vec<f64> {
    lagen(
        f.iter()
            .filter(|(a, b)| (a.0 - b.0).abs() < 1e-6)
            .map(|(a, _)| a.0),
    )
}

fn lagen(it: impl Iterator<Item = f64>) -> Vec<f64> {
    let mut v: Vec<f64> = it.collect();
    v.sort_by(f64::total_cmp);
    v.dedup_by(|a, b| (*a - *b).abs() < 1e-6);
    v
}

/// Alle Lagen auf k·`raster`, Nachbarn genau `raster` auseinander.
fn auf_raster(l: &[f64], raster: f64, was: &str) {
    assert!(l.len() >= 2, "{was}: zu wenige Fugen {l:?}");
    for x in l {
        let k = (x / raster).round();
        assert!(
            (x - k * raster).abs() < 0.01,
            "{was}: Fuge bei {x} nicht auf k·{raster}"
        );
    }
    for w in l.windows(2) {
        assert!((w[1] - w[0] - raster).abs() < 0.01, "{was}: {w:?}");
    }
}

/// Abstand des Punkts zur Strecke.
fn abstand(p: (f64, f64), (a, b): Linie) -> f64 {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let l2 = dx * dx + dy * dy;
    let t = if l2 > 0.0 {
        (((p.0 - a.0) * dx + (p.1 - a.1) * dy) / l2).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let (x, y) = (a.0 + t * dx - p.0, a.1 + t * dy - p.1);
    (x * x + y * y).sqrt()
}

/// Strecken ohne Doppel: Polygonzüge je Zelle liefern jede Grenze
/// zweimal (je Nachbarzelle einmal), auf 0,1 mm gleich.
fn ohne_doppel(f: &[Linie]) -> Vec<Linie> {
    let key = |(a, b): Linie| {
        let r = |x: f64| (x * 10.0).round() as i64;
        let (p, q) = ((r(a.0), r(a.1)), (r(b.0), r(b.1)));
        if p <= q {
            (p, q)
        } else {
            (q, p)
        }
    };
    let mut v: Vec<Linie> = Vec::new();
    let mut seen = std::collections::BTreeSet::new();
    for l in f {
        if seen.insert(key(*l)) {
            v.push(*l);
        }
    }
    v
}

/// Kleiner eigener Zufall für Prüfpunkte (fest, kein Fremdcode).
struct Lcg(u64);
impl Lcg {
    fn next(&mut self) -> f64 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        (self.0 >> 11) as f64 / (1u64 << 53) as f64
    }
}

/// Anteil der Prüfpunkte auf einem 20 × 20-Raster über 1,20 m, an denen
/// sich zwei Muster unterscheiden.
fn unterschied(a: &Pattern, b: &Pattern) -> usize {
    let mut n = 0;
    for i in 0..20 {
        for k in 0..20 {
            let (u, v) = (13.0 + i as f64 * 61.3, -400.0 + k as f64 * 61.3);
            if farbe(a, u, v) != farbe(b, u, v) {
                n += 1;
            }
        }
    }
    n
}

const NEU: [&str; 4] = [
    "Sichtbeton mittelgrau",
    "Holzschalung Lärche",
    "Betonplatten 40 × 40",
    "Naturstein",
];

/// A254 (paket-7 §2.2, paket-p7 §2, BIM-Nachtrag 18:10): Neun
/// Werksvorlagen in dieser Reihenfolge, alle gültig. Die drei Vorgaben
/// aus Jörns Referenzbildern stehen an den Plätzen von „Klinker bunt“,
/// „Putz fein“ und „Sichtbeton“ und sind die Werksmuster von
/// Verblender, Putz und Stahlbeton (mit Grundfarbe). „Klinker rot“ ist
/// das bisherige Startmuster (halbsteinig, Rottöne 40/35/25).
/// Mauerwerk- und Putzvorlagen mit den Werten aus Sollbild bzw.
/// Nachtrag; Grundfarben der übrigen.
#[test]
fn a254_werksvorlagen() {
    let namen: Vec<&str> = vorlagen().iter().map(|p| p.name.as_str()).collect();
    assert_eq!(
        namen,
        [
            "Klinker rot",
            "Klinker friesisch-bunt",
            "Kalksandstein sichtbar",
            "Reibeputz weiß",
            "Putz grob",
            "Sichtbeton mittelgrau",
            "Holzschalung Lärche",
            "Betonplatten 40 × 40",
            "Naturstein",
        ]
    );
    for p in vorlagen() {
        assert!(gueltig(&p.pattern), "{} ungültig", p.name);
    }
    let s = Scene::with_model(Model::with_seed(254));
    for (baustoff, v) in [
        ("Verblender (Vormauerziegel)", "Klinker friesisch-bunt"),
        ("Putz", "Reibeputz weiß"),
        ("Stahlbeton", "Sichtbeton mittelgrau"),
    ] {
        let f = flaeche(s.model(), baustoff);
        assert_eq!(
            muster(s.model(), f).as_ref(),
            Some(&vorlage(v).pattern),
            "Werksmuster {baustoff}"
        );
        if baustoff != "Verblender (Vormauerziegel)" {
            assert_eq!(
                s.model().attr().surface(f).unwrap().color,
                vorlage(v).base,
                "Grundfarbe {baustoff}"
            );
        }
    }

    let mauer = |name: &str| match &vorlage(name).pattern {
        Pattern::Masonry {
            len,
            h,
            joint_rgb,
            palette,
            spread,
            ..
        } => (
            *len,
            *h,
            *joint_rgb,
            palette.map(|(c, a)| (c, a as u8)),
            *spread,
        ),
        p => panic!("{name}: {p:?}"),
    };
    assert_eq!(
        mauer("Klinker rot"),
        (
            240.0,
            71.0,
            rgb(0xd8d4cc),
            [
                (rgb(0x8a3b2a), 40),
                (rgb(0x9c4a33), 35),
                (rgb(0x6e2f22), 25)
            ],
            6.0
        )
    );
    assert!(matches!(
        vorlage("Klinker rot").pattern,
        Pattern::Masonry {
            bond: Bond::Half,
            ..
        }
    ));
    assert_eq!(flammung(&vorlage("Klinker rot").pattern), 0.0);
    assert!(kopfpalette(&vorlage("Klinker rot").pattern).is_empty());
    let fb = &vorlage("Klinker friesisch-bunt").pattern;
    assert_eq!(
        mauer("Klinker friesisch-bunt"),
        (
            240.0,
            71.0,
            rgb(0xd1cbc2),
            [(rgb(0x87493c), 79), (rgb(0x675549), 9), (rgb(0x7b6d65), 12)],
            7.0
        )
    );
    assert!(matches!(
        fb,
        Pattern::Masonry {
            bond: Bond::Wild,
            ..
        }
    ));
    assert_eq!(kopfpalette(fb), [(rgb(0x87493c), 50), (rgb(0x675549), 50)]);
    assert_eq!(
        flammung_alle(fb),
        (100.0, 64.0, 100.0),
        "flame, fend, relief"
    );
    for (name, korn, streu, grund) in [
        ("Reibeputz weiß", 2.0, 4.0, 0xececed),
        ("Putz grob", 3.0, 6.0, 0xece9e0),
    ] {
        let v = vorlage(name);
        assert!(
            matches!(v.pattern, Pattern::Plaster { grain, spread, .. } if grain == korn && spread == streu),
            "{name}: {:?}",
            v.pattern
        );
        assert_eq!(v.base, rgb(grund), "{name}");
    }
    let beton = vorlage("Sichtbeton mittelgrau");
    assert_eq!(beton.base, rgb(0x8e8e8d));
    assert!(
        matches!(
            beton.pattern,
            Pattern::Concrete { joint, anchors, cloud, pores, .. }
                if joint == 0.0 && !anchors && cloud == 2.0 && pores == 0.5
        ),
        "{:?}",
        beton.pattern
    );
    assert_eq!(
        fugenfarbe(&vorlage("Betonplatten 40 × 40").pattern),
        rgb(0x6e6e6a)
    );
    assert_eq!(fugenfarbe(&vorlage("Naturstein").pattern), rgb(0x96928a));
}

/// A255 (§4.1, Regel 61): Jede neue Art ist deterministisch und
/// verändert sich bei anderem Startwert (mindestens ein Viertel der
/// Prüfpunkte), auch unter ±0,00.
#[test]
fn a255_neue_arten_deterministisch() {
    for name in NEU {
        let p = &vorlage(name).pattern;
        assert_eq!(unterschied(p, &p.clone()), 0, "{name}: gleicher Startwert");
        let n = unterschied(p, &mit_startwert(p, 4711));
        assert!(n >= 100, "{name}: anderer Startwert, nur {n}/400 anders");
    }
}

/// A256 (§4.2): Platten Kreuz 400 × 400, Fuge 5: Fugenmittellinien bei
/// k·405 in u und v, in jeder Reihe dieselben Stöße. Halbversatz: Reihe
/// n+1 um 202,5 versetzt. Auf der Fuge zeigt `sample` die Fugenfarbe.
#[test]
fn a256_platten() {
    let p = vorlage("Betonplatten 40 × 40").pattern.clone();
    let f = fugen(&p, -1000.0, -1000.0, 2000.0, 2000.0);
    auf_raster(&waagerecht(&f), 405.0, "Platten waagerecht");
    let r0 = senkrecht_bei(&f, 202.5);
    let r1 = senkrecht_bei(&f, 607.5);
    auf_raster(&r0, 405.0, "Platten Reihe 0");
    assert_eq!(r0, r1, "Kreuzfuge: Stöße übereinander");
    assert_eq!(farbe(&p, 100.0, 405.0), fugenfarbe(&p), "auf der Fuge");
    assert_ne!(farbe(&p, 202.5, 202.5), fugenfarbe(&p), "Plattenmitte");

    let h = halbversatz(&p);
    let f = fugen(&h, -1000.0, -1000.0, 2000.0, 2000.0);
    auf_raster(&waagerecht(&f), 405.0, "Halbversatz waagerecht");
    let (a, b) = (senkrecht_bei(&f, 202.5), senkrecht_bei(&f, 607.5));
    for w in a.windows(2).chain(b.windows(2)) {
        assert!((w[1] - w[0] - 405.0).abs() < 0.01, "Halbversatz {w:?}");
    }
    let versatz = (b[0] - a[0]).rem_euclid(405.0);
    assert!((versatz - 202.5).abs() < 0.01, "Halbversatz: {versatz}");
}

/// A257 (§4.3): Holzschalung senkrecht, Brett 120, Fuge 8: Fugen bei
/// k·128 in u, keine waagerechten Fugen. Waagerecht dasselbe in v.
#[test]
fn a257_holzschalung() {
    let p = vorlage("Holzschalung Lärche").pattern.clone();
    let f = fugen(&p, -500.0, -500.0, 1500.0, 1500.0);
    assert!(waagerecht(&f).is_empty(), "senkrecht: keine Fugen in v");
    auf_raster(&senkrecht(&f), 128.0, "Bretter senkrecht");
    for (a, b) in &f {
        assert!(
            (a.1.min(b.1) + 500.0).abs() < 0.01 && (a.1.max(b.1) - 1500.0).abs() < 0.01,
            "Brettfuge durchgehend"
        );
    }
    let w = waagerechte_bretter(&p);
    let f = fugen(&w, -500.0, -500.0, 1500.0, 1500.0);
    assert!(senkrecht(&f).is_empty(), "waagerecht: keine Fugen in u");
    auf_raster(&waagerecht(&f), 128.0, "Bretter waagerecht");
}

/// A258 (§4.4): Sichtbeton mit Tafeln 2500 × 500, Stoß 2 (die
/// Werksvorlage hat seit dem Nachtrag 18:10 keine Stöße, darum hier
/// eingestellt): Stöße bei k·2500 in u und k·500 in v. Ankerlöcher nur, wenn eingeschaltet: Die
/// Stöße bleiben gleich, das Bild ändert sich an einigen wenigen
/// Stellen (über 0, unter 5 % der Punkte eines 5-mm-Rasters).
#[test]
fn a258_sichtbeton() {
    let p = mit_tafeln(
        &vorlage("Sichtbeton mittelgrau").pattern,
        2500.0,
        500.0,
        2.0,
    );
    let f = fugen(&p, -5000.0, -1000.0, 7600.0, 1600.0);
    auf_raster(&waagerecht(&f), 500.0, "Tafelstöße waagerecht");
    auf_raster(&senkrecht(&f), 2500.0, "Tafelstöße senkrecht");
    let a = mit_ankern(&p);
    assert_eq!(
        fugen(&a, -5000.0, -1000.0, 7600.0, 1600.0),
        f,
        "Anker ändern keine Stöße"
    );
    let (mut n, mut anders) = (0, 0);
    for i in 0..500 {
        for k in 0..100 {
            let (u, v) = (2.5 + i as f64 * 5.0, 2.5 + k as f64 * 5.0);
            n += 1;
            if farbe(&p, u, v) != farbe(&a, u, v) {
                anders += 1;
            }
        }
    }
    assert!(anders > 0, "Ankerlöcher sichtbar");
    assert!(anders * 20 < n, "nur Löcher: {anders}/{n}");
}

/// A259 (§2.1, §4.5, Review 3d N1, Koordinator 11:33 „Abnahmefall 5
/// bleibt so“): Naturstein, Größe 300, Fuge 15.
/// - Fugenbreite überall gleich, auch an schrägen Grenzen: Punkte bis
///   7,0 mm von der nächsten Fugenmittellinie zeigen die Fugenfarbe,
///   Punkte ab 8,0 mm nicht (20 000 Zufallspunkte und je schräger Grenze
///   die Senkrechte durch ihre Mitte bei 6,5 und 8,5 mm).
/// - Mittlere Steingröße auf ±15 %: geschätzt als 2·Fläche / Fugenlänge
///   (Voronoi mit Abstand s hat rund 2/s Grenzlänge je Fläche).
/// - Mindestens ein Drittel der Grenzen ist schräg.
#[test]
fn a259_naturstein() {
    let p = vorlage("Naturstein").pattern.clone();
    let jf = fugenfarbe(&p);
    let f = ohne_doppel(&fugen(&p, 0.0, 0.0, 3000.0, 3000.0));
    let laenge: f64 = f
        .iter()
        .map(|(a, b)| ((b.0 - a.0).powi(2) + (b.1 - a.1).powi(2)).sqrt())
        .sum();
    let s = 2.0 * 3000.0 * 3000.0 / laenge;
    assert!(
        (255.0..=345.0).contains(&s),
        "mittlere Steingröße {s:.0} statt 300 ± 15 %"
    );

    let schraeg: Vec<Linie> = f
        .iter()
        .copied()
        .filter(|(a, b)| (a.0 - b.0).abs() > 1.0 && (a.1 - b.1).abs() > 1.0)
        .collect();
    assert!(
        schraeg.len() * 3 >= f.len(),
        "schräge Grenzen {}/{}",
        schraeg.len(),
        f.len()
    );

    let naechste = |q: (f64, f64)| f.iter().map(|l| abstand(q, *l)).fold(f64::MAX, f64::min);
    let mut z = Lcg(259);
    let mut geprueft = 0;
    for _ in 0..20_000 {
        let q = (500.0 + z.next() * 2000.0, 500.0 + z.next() * 2000.0);
        let d = naechste(q);
        if d <= 7.0 {
            assert_eq!(
                farbe(&p, q.0, q.1),
                jf,
                "{q:?} ist {d:.2} mm von der Fuge: Fugenfarbe"
            );
            geprueft += 1;
        } else if d >= 8.0 {
            assert_ne!(
                farbe(&p, q.0, q.1),
                jf,
                "{q:?} ist {d:.2} mm von der Fuge: Stein"
            );
        }
    }
    assert!(geprueft > 500, "zu wenig Fugenpunkte: {geprueft}");

    let mut n = 0;
    for (a, b) in &schraeg {
        let (dx, dy) = (b.0 - a.0, b.1 - a.1);
        let l = (dx * dx + dy * dy).sqrt();
        let m = ((a.0 + b.0) / 2.0, (a.1 + b.1) / 2.0);
        if l < 60.0 || !(400.0..2600.0).contains(&m.0) || !(400.0..2600.0).contains(&m.1) {
            continue;
        }
        let (nx, ny) = (-dy / l, dx / l);
        for seite in [1.0, -1.0] {
            let innen = (m.0 + seite * 6.5 * nx, m.1 + seite * 6.5 * ny);
            let aussen = (m.0 + seite * 8.5 * nx, m.1 + seite * 8.5 * ny);
            assert_eq!(
                farbe(&p, innen.0, innen.1),
                jf,
                "schräg {m:?}: 6,5 mm ist Fuge"
            );
            if naechste(aussen) >= 8.0 {
                assert_ne!(
                    farbe(&p, aussen.0, aussen.1),
                    jf,
                    "schräg {m:?}: 8,5 mm ist Stein"
                );
            }
        }
        n += 1;
    }
    assert!(n >= 20, "zu wenig schräge Grenzen geprüft: {n}");
}

/// A260 (§2.1): Mauerwerk Blockverband und Kreuzverband: Reihen mit
/// Läufern (Stoßfugen alle 250) und Köpfen als halbe Steine (alle 125)
/// wechseln. Block: jede zweite Läuferreihe gleich. Kreuz: aufeinander
/// folgende Läuferreihen um 125 versetzt, nach vier Reihen wieder gleich.
#[test]
fn a260_block_und_kreuzverband() {
    let nf = vorlage("Klinker rot").pattern.clone();
    for art in ["block", "cross"] {
        let p = verband(&nf, art);
        let f = fugen(&p, 0.0, 0.0, 3000.0, 500.0);
        let reihen: Vec<Vec<f64>> = (0..5)
            .map(|n| senkrecht_bei(&f, 40.5 + 81.0 * n as f64))
            .collect();
        let abstand = |r: &[f64]| r[1] - r[0];
        for r in &reihen {
            for w in r.windows(2) {
                assert!(
                    (w[1] - w[0] - abstand(r)).abs() < 0.01,
                    "{art}: ungleich {w:?}"
                );
            }
        }
        let laeufer: Vec<bool> = reihen
            .iter()
            .map(|r| (abstand(r) - 250.0).abs() < 0.01)
            .collect();
        for (n, r) in reihen.iter().enumerate() {
            let soll = if laeufer[n] { 250.0 } else { 125.0 };
            assert!(
                (abstand(r) - soll).abs() < 0.01,
                "{art} Reihe {n}: {}",
                abstand(r)
            );
        }
        for n in 0..4 {
            assert_ne!(
                laeufer[n],
                laeufer[n + 1],
                "{art}: Läufer und Köpfe wechseln"
            );
        }
        let l0 = if laeufer[0] { 0 } else { 1 };
        let versatz = (reihen[l0 + 2][0] - reihen[l0][0]).rem_euclid(250.0);
        let versatz = versatz.min(250.0 - versatz);
        if art == "block" {
            assert!(versatz < 0.01, "Block: Läuferreihen gleich, {versatz}");
        } else {
            assert!(
                (versatz - 125.0).abs() < 0.01,
                "Kreuz: Läuferreihen um 125 versetzt, {versatz}"
            );
        }
    }
}

/// A261 (§2.3, Regeln 59, 62, 68–70; Koordinator 18:58: flame, fend,
/// relief 0–100, pores 0–2, Beton-Stoß ab 0): Die neuen Wörter `gen=concrete|timber|
/// tiles|stone` und `bond=block|cross` im Rundlauf, `.szo` bleibt 4,
/// beim Öffnen kein Hinweis. Grenzen aus der Tabelle in `proctex.rs`:
/// Je Schlüssel gelten kleinster und größter Wert; knapp darunter oder
/// darüber wird die Zeile mit Hinweis verworfen, die Oberfläche ist ohne
/// Muster.
#[test]
fn a261_datei_neue_arten_und_grenzen() {
    // Koordinator 18:58: Grenzen der neuen Schlüssel, Beton-Stoß ab 0
    for (gen, key, lo, hi) in [
        ("masonry", "flame", 0.0, 100.0),
        ("masonry", "fend", 0.0, 100.0),
        ("masonry", "relief", 0.0, 100.0),
        ("concrete", "pores", 0.0, 2.0),
    ] {
        assert!(
            grenzen(gen).contains(&(key, lo, hi)),
            "{gen} {key} {lo}–{hi}: {:?}",
            grenzen(gen)
        );
    }
    assert!(
        grenzen("concrete")
            .iter()
            .any(|(k, lo, _)| *k == "joint" && *lo == 0.0),
        "Beton-Stoß ab 0: {:?}",
        grenzen("concrete")
    );
    let mut s = Scene::with_model(Model::with_seed(261));
    gebaeude(&mut s);
    let gb = flaeche(s.model(), "Porenbeton");
    let g = sguid(s.model(), gb);
    let mut faelle: Vec<(Pattern, &str)> = NEU
        .iter()
        .zip(["gen=concrete", "gen=timber", "gen=tiles", "gen=stone"])
        .map(|(n, w)| (vorlage(n).pattern.clone(), w))
        .collect();
    let nf = vorlage("Klinker rot").pattern.clone();
    faelle.push((verband(&nf, "block"), "bond=block"));
    faelle.push((verband(&nf, "cross"), "bond=cross"));
    for (p, wort) in faelle {
        muster_setzen(&mut s, gb, Some(p.clone()));
        let text = sk_model::szo::write(s.model());
        assert!(text.starts_with("SZO 4\n"));
        let z: Vec<&str> = zeilen(&text, "[pattern]")
            .into_iter()
            .filter(|l| l.contains(&format!("surface={g}")))
            .collect();
        assert_eq!(z.len(), 1, "{wort}: {z:?}");
        assert!(z[0].contains(wort), "{wort}: {}", z[0]);
        let l = lesen(&text);
        assert!(l.hints.is_empty(), "{wort}: {:?}", l.hints);
        assert_eq!(sk_model::szo::write(&l.model), text, "{wort}: Rundlauf");
        assert_eq!(muster(&l.model, flaeche(&l.model, "Porenbeton")), Some(p));

        if !wort.starts_with("gen=") {
            continue;
        }
        let ohne: String = text
            .lines()
            .filter(|l| !(l.starts_with("[pattern]") && l.contains(&format!("surface={g}"))))
            .map(|l| format!("{l}\n"))
            .collect();
        let tabelle = grenzen(&wort[4..]);
        assert!(!tabelle.is_empty(), "{wort}: Grenzen fehlen");
        for (key, lo, hi) in tabelle {
            let alt = z[0]
                .split(' ')
                .find(|t| t.starts_with(&format!("{key}=")))
                .unwrap_or_else(|| panic!("{wort}: Schlüssel {key} fehlt in {}", z[0]));
            let d = ((hi - lo) / 100.0).max(0.01);
            for (wert, gut) in [(lo, true), (hi, true), (lo - d, false), (hi + d, false)] {
                let zeile = z[0].replace(alt, &format!("{key}={wert}"));
                let l = lesen(&format!("{ohne}{zeile}\n"));
                let m = muster(&l.model, flaeche(&l.model, "Porenbeton"));
                if gut {
                    assert!(l.hints.is_empty() && m.is_some(), "{zeile}: {:?}", l.hints);
                } else {
                    assert!(!l.hints.is_empty() && m.is_none(), "{zeile} verworfen");
                }
            }
        }
    }
}

/// A262 (§5, Regeln 62, 65; plan B7): Rücknahme. Läuft ohne Adapter und
/// gilt auch auf dem Stand vor 7a bzw. 7b (mit F-17b): `[pattern]` mit
/// den neuen Arten bleibt in `.szo` bytegleich, `[patternpreset]` in
/// `.szk` ebenso. Die Zeilen sind so geschrieben, wie 7a sie schreiben
/// soll; schreibt der Bau die Schlüssel anders, setzt Test hier dessen
/// Zeilen ein.
#[test]
fn a262_ruecknahme_neue_arten() {
    let mut s = Scene::with_model(Model::with_seed(262));
    gebaeude(&mut s);
    let text = sk_model::szo::write(s.model());
    let mut gs = s.model().attr().surfaces().iter().map(|(_, x)| x.guid);
    let (a, b, c, d, e) = (
        gs.next().unwrap(),
        gs.next().unwrap(),
        gs.next().unwrap(),
        gs.next().unwrap(),
        gs.next().unwrap(),
    );
    let neu = format!(
            "[pattern] surface={a} gen=concrete w=2500 h=500 joint=0 anchors=0 cloud=2 pores=0.5 seed=11\n\
             [pattern] surface={b} gen=timber dir=v board=120 joint=8 grain=5 c1=b08054 c2=c09264 seed=21\n\
             [pattern] surface={c} gen=tiles len=400 wid=400 joint=5 bond=cross jrgb=6e6e6a pal=9a9a96:40;a6a5a0:35;8e8e8a:25 spread=4 seed=13\n\
             [pattern] surface={d} gen=stone size=300 joint=15 irr=60 jrgb=96928a pal=bfa98a:40;a8916f:35;cdbb9c:25 seed=31\n\
             [pattern] surface={e} gen=masonry len=240 h=71 joint=10 bond=wild jrgb=d1cbc2 pal=87493c:79;675549:9;7b6d65:12 hpal=87493c:50;675549:50 flame=100 fend=64 relief=100 spread=7 seed=17\n"
        );
    // Werks-Oberflächen (Stahlbeton, Putz, Verblender) tragen schon eine
    // Zeile; eine zweite bliebe nach §2.2 hinter der ersten stehen
    // (Bauthread 20:40). Darum vorher die eigenen Zeilen der fünf entfernen.
    let text: String = text
        .lines()
        .filter(|z| {
            !(z.starts_with("[pattern]")
                && [a, b, c, d, e]
                    .iter()
                    .any(|g| z.contains(&format!("surface={g} "))))
        })
        .map(|z| format!("{z}\n"))
        .collect();
    let l = lesen(&format!("{text}{neu}"));
    assert!(l.model.check().is_empty());
    let geschrieben = sk_model::szo::write(&l.model);
    // Der Schreiber ordnet nach Guid der Oberfläche (Bauthread 20:55)
    let mut ist: Vec<&str> = zeilen(&geschrieben, "[pattern]")
        .into_iter()
        .filter(|z| {
            [a, b, c, d, e]
                .iter()
                .any(|g| z.contains(&format!("surface={g}")))
        })
        .collect();
    let mut soll: Vec<&str> = neu.lines().collect();
    ist.sort_unstable();
    soll.sort_unstable();
    assert_eq!(ist, soll);

    let lib = sk_model::Library::from_model(s.model());
    let szk = sk_model::write_szk(&lib);
    let vorlage = "[patternpreset] guid=0bTdQXV2v4F9fBAYaFw6qr name=\"Klinker Müller\" base=8a3b2a gen=masonry len=240 h=71 joint=10 bond=half jrgb=d8d4cc pal=8a3b2a:35;6b4a3a:35;a0623e:30 spread=8 seed=5";
    let lib2 = sk_model::read_szk(&format!("{szk}{vorlage}\n")).expect("liest");
    let zurueck = sk_model::write_szk(&lib2);
    assert_eq!(zeilen(&zurueck, "[patternpreset]"), [vorlage]);
    // Guid in IFC-Kurzform (22 Zeichen, Bauthread 20:25): echte Vorlage
    assert_eq!(
        firmenvorlagen(&lib2)
            .iter()
            .map(|x| x.1.as_str())
            .collect::<Vec<_>>(),
        ["Klinker Müller"]
    );
}

/// A263 (§2.2, Regeln 65, 66): Firmenvorlagen.
/// - „Als Vorlage speichern“ legt genau eine `[patternpreset]`-Zeile mit
///   Guid und Namen in `.szk` an; Rundlauf bytegleich.
/// - Abgelehnt: leerer Name, Name einer Werksvorlage, Name doppelt.
/// - Beim Lesen verworfen: Zeile ohne Guid, Name einer Werksvorlage,
///   Wert außerhalb der Grenzen; der Rest des Katalogs bleibt.
/// - Übernehmen kopiert: `.szo` enthält nie `[patternpreset]` und
///   keinen Verweis; Ändern oder Löschen der Firmenvorlage ändert die
///   Projektdatei nicht.
#[test]
fn a263_firmenvorlagen() {
    let bunt = vorlage("Klinker friesisch-bunt");
    let eigen = mit_startwert(&bunt.pattern, 5);
    let mut lib = sk_model::Library::default();
    assert!(speichern(&mut lib, "Klinker Müller", &eigen, bunt.base));
    assert!(!speichern(&mut lib, "", &eigen, bunt.base), "leerer Name");
    assert!(
        !speichern(&mut lib, "Klinker rot", &eigen, bunt.base),
        "Name einer Werksvorlage"
    );
    assert!(
        !speichern(&mut lib, "Klinker Müller", &eigen, bunt.base),
        "Name doppelt"
    );
    assert!(speichern(
        &mut lib,
        "Naturstein Hof",
        &vorlage("Naturstein").pattern,
        [191, 169, 138]
    ));
    let fv = firmenvorlagen(&lib);
    assert_eq!(
        fv.iter().map(|x| x.1.as_str()).collect::<Vec<_>>(),
        ["Klinker Müller", "Naturstein Hof"]
    );
    assert_ne!(fv[0].0, fv[1].0, "eigene Guids");

    let szk = sk_model::write_szk(&lib);
    let z = zeilen(&szk, "[patternpreset]");
    assert_eq!(z.len(), 2, "{szk}");
    assert!(
        z[0].contains(&format!("guid={}", fv[0].0)) && z[0].contains("name=\"Klinker Müller\""),
        "{}",
        z[0]
    );
    assert!(
        z[0].contains("gen=masonry"),
        "gleiche Schlüssel wie [pattern]: {}",
        z[0]
    );
    let gelesen = sk_model::read_szk(&szk).expect("liest");
    assert_eq!(sk_model::write_szk(&gelesen), szk, "Rundlauf");
    assert_eq!(firmenvorlagen(&gelesen), fv);

    let ohne: String = szk
        .lines()
        .filter(|l| !l.starts_with("[patternpreset]"))
        .map(|l| format!("{l}\n"))
        .collect();
    let zweite = z[1];
    for falsch in [
        z[0].replace(&format!("guid={} ", fv[0].0), ""),
        z[0].replace("name=\"Klinker Müller\"", "name=\"Naturstein\""),
        z[0].replace("joint=10", "joint=40"),
    ] {
        let l = sk_model::read_szk(&format!("{ohne}{falsch}\n{zweite}\n")).expect("liest trotzdem");
        assert_eq!(
            firmenvorlagen(&l)
                .iter()
                .map(|x| x.1.as_str())
                .collect::<Vec<_>>(),
            ["Naturstein Hof"],
            "verworfen: {falsch}"
        );
    }

    let mut s = Scene::with_model(Model::with_seed(263));
    gebaeude(&mut s);
    let gb = flaeche(s.model(), "Porenbeton");
    let fl = PatternPreset {
        name: "Klinker Müller".into(),
        pattern: eigen.clone(),
        base: bunt.base,
    };
    anwenden(&mut s, gb, &fl);
    assert_eq!(
        muster(s.model(), gb),
        Some(eigen.clone()),
        "Kopie an der Oberfläche"
    );
    let projekt = sk_model::szo::write(s.model());
    assert!(!projekt.contains("[patternpreset]"), "nie in .szo");
    assert!(
        !projekt.contains(&fv[0].0.to_string()),
        "kein Verweis auf die Vorlage"
    );
    assert!(
        !projekt.contains("Klinker Müller"),
        "kein Vorlagenname im Projekt"
    );
    loeschen(&mut lib, fv[0].0);
    assert!(speichern(
        &mut lib,
        "Klinker Müller",
        &mit_startwert(&eigen, 6),
        bunt.base
    ));
    assert_eq!(
        sk_model::szo::write(s.model()),
        projekt,
        "Löschen und Neuanlegen ändern das Projekt nicht"
    );
    assert_eq!(muster(s.model(), gb), Some(eigen));
}

/// A264 (§4.9, Regeln 56, 63): Die neuen Arten ändern keine Mengen,
/// Nummern und keine Geometrie. Haus mit AW-49 und Dachterrasse:
/// Betonplatten auf dem Terrassenbelag (Raster auch waagerecht),
/// Sichtbeton mit Tafelstößen am Stahlbeton, Naturstein am Verblender. CSV, Netzflächen
/// und die Datei ohne `[pattern]` bleiben gleich (gleiche CSV: keine
/// Plattenzahl). OG-Nord −1,50 wie A198.
#[test]
fn a264_neue_arten_aendern_keine_mengen() {
    let mut s = Scene::with_model(Model::with_seed(264));
    let (eg, _) = gebaeude(&mut s);
    let t = s
        .model()
        .layer_sets()
        .iter()
        .find(|(_, t)| t.code == "AW-49")
        .map(|(id, _)| id)
        .unwrap();
    assert!(s.edit_model("Wandtyp geändert", |m| m.set_run_type(eg, t)));
    let w = s
        .model()
        .elements()
        .iter()
        .find(|(_, e)| e.number == "AW-006")
        .map(|(id, _)| id)
        .unwrap();
    assert!(s.edit_model("Kopplung gelöst", |m| m.set_linked(w, false)));
    assert!(s.edit_model("Wand verschoben", |m| m.move_segment(w, -1500.0).is_some()));
    assert!(
        s.model()
            .elements()
            .iter()
            .any(|(_, e)| e.number == "DT-001"),
        "Dachterrasse über dem Rücksprung"
    );
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
    let vorher = stand(&mut s);
    let tafeln = mit_tafeln(
        &vorlage("Sichtbeton mittelgrau").pattern,
        2500.0,
        500.0,
        2.0,
    );
    for (baustoff, v, p) in [
        ("Terrassenbelag", "Betonplatten 40 × 40", None),
        ("Stahlbeton", "Sichtbeton mit Tafeln", Some(tafeln)),
        ("Verblender (Vormauerziegel)", "Naturstein", None),
        ("Porenbeton", "Holzschalung Lärche", None),
    ] {
        let f = flaeche(s.model(), baustoff);
        let p = p.unwrap_or_else(|| vorlage(v).pattern.clone());
        muster_setzen(&mut s, f, Some(p));
        assert_eq!(stand(&mut s), vorher, "{v} an {baustoff}");
    }
}

/// Kernfarben „friesisch-bunt“ (BIM-Nachtrag 2): Rot, Braun-grau,
/// Silbergrau.
const FAMILIEN: [u32; 3] = [0x87493c, 0x675549, 0x7b6d65];
const FUGE_FB: u32 = 0xd1cbc2;

fn hell(c: [u8; 3]) -> f64 {
    (c[0] as f64 + c[1] as f64 + c[2] as f64) / 3.0
}

fn abstand2(a: [u8; 3], b: [u8; 3]) -> i32 {
    (0..3).map(|k| (a[k] as i32 - b[k] as i32).pow(2)).sum()
}

/// Nächste der Farben `wahl` (Index).
fn naechste(c: [u8; 3], wahl: &[[u8; 3]]) -> usize {
    (0..wahl.len())
        .min_by_key(|&i| abstand2(c, wahl[i]))
        .unwrap()
}

/// Steine im Rechteck: (Reihenmitte v, linke und rechte
/// Stoßfugen-Mittellinie).
fn steine(p: &Pattern, u0: f64, v0: f64, u1: f64, v1: f64) -> Vec<(f64, f64, f64)> {
    let f = fugen(p, u0, v0, u1, v1);
    let mut st = Vec::new();
    for w in waagerecht(&f).windows(2) {
        let v = (w[0] + w[1]) / 2.0;
        for x in senkrecht_bei(&f, v).windows(2) {
            st.push((v, x[0], x[1]));
        }
    }
    st
}

/// A270 (Regel 68 nach BIM-Nachtrag 3: flame=100, fend=64; Nachtrag 2:
/// hpal=): Flammung, Kopfpalette und Flächenanteile
/// „friesisch-bunt“. Ohne Streuung und ohne Relief:
/// - Köpfe werden nie geflammt (an drei Stellen gleich) und tragen nur
///   Farben aus `hpal=`: Rot und Braun-grau je 50 ± 5 %.
/// - Läufer in Braun-grau oder Silbergrau sind einfarbig.
/// - Rote Läufer sind mit flame=100 alle geflammt: 2 mm vor jedem Ende
///   liegt die Farbe näher an einer Graufamilie als an Rot (mindestens
///   90 % der Enden). Beide Enden eines Steins in derselben Familie
///   (mindestens 90 % der Steine), 64/36 braun-grau/silbergrau (±6).
/// - Mit flame=0 sind auch rote Läufer einfarbig.
/// - Nach Fläche auf 10 m² (5-mm-Raster, Punkte der nächsten Farbe
///   zugeordnet, Fuge ausgenommen): Rot 43 ± 3 %, Braun-grau 36 ± 4 %,
///   Silbergrau 21 ± 4 %.
/// - `flame=`, `fend=` und `hpal=` in der Zeile; flame und fend 0 und
///   100 gelten, 101 und −1 werden mit Hinweis verworfen.
#[test]
fn a270_flammung_und_koepfe() {
    let fb = &vorlage("Klinker friesisch-bunt").pattern;
    let p = ohne_relief(&ohne_streuung(fb));
    let flach = mit_flammung(&p, 0.0);
    let fam = FAMILIEN.map(rgb);
    let st = steine(&p, 0.0, 0.0, 8000.0, 4000.0);
    assert!(st.len() > 1500, "{} Steine", st.len());
    assert_eq!(
        st,
        steine(&flach, 0.0, 0.0, 8000.0, 4000.0),
        "Verband gleich"
    );
    let (mut koepfe, mut kopf_rot) = (0usize, 0usize);
    let (mut enden, mut grau_enden) = (0usize, 0usize);
    let (mut gleich, mut silber, mut geflammt) = (0usize, 0usize, 0usize);
    for &(v, x0, x1) in &st {
        let mitte = farbe(&p, (x0 + x1) / 2.0, v);
        let (a, b) = (x0 + 7.0, x1 - 7.0);
        if x1 - x0 < 200.0 {
            koepfe += 1;
            for u in [a, b] {
                assert_eq!(farbe(&p, u, v), mitte, "Kopf {x0}..{x1}, {v}: geflammt");
            }
            match fam.iter().position(|&c| c == mitte) {
                Some(0) => kopf_rot += 1,
                Some(1) => {}
                _ => panic!("Kopf {x0}..{x1}, {v}: {mitte:?} nicht aus hpal"),
            }
            continue;
        }
        let ende = [farbe(&p, a, v), farbe(&p, b, v)];
        let flach_mitte = farbe(&flach, (x0 + x1) / 2.0, v);
        for u in [a, b] {
            assert_eq!(
                farbe(&flach, u, v),
                flach_mitte,
                "flame=0: Läufer einfarbig"
            );
        }
        match fam.iter().position(|&c| c == flach_mitte) {
            Some(0) => {
                let k = ende.map(|e| naechste(e, &fam));
                enden += 2;
                grau_enden += k.iter().filter(|&&x| x != 0).count();
                if k[0] != 0 && k[1] != 0 {
                    geflammt += 1;
                    if k[0] == k[1] {
                        gleich += 1;
                    }
                    if k[0] == 2 && k[1] == 2 {
                        silber += 1;
                    }
                }
            }
            Some(_) => {
                assert_eq!(
                    ende, [flach_mitte; 2],
                    "grauer Läufer {x0}..{x1}, {v} geflammt"
                );
            }
            None => panic!("Läufer {x0}..{x1}, {v}: Kernfarbe {flach_mitte:?}"),
        }
    }
    let anteil = kopf_rot as f64 / koepfe as f64 * 100.0;
    assert!((anteil - 50.0).abs() <= 5.0, "Köpfe rot: {anteil:.1} %");
    assert!(enden > 1000, "{enden} Enden roter Läufer");
    assert!(
        grau_enden * 10 >= enden * 9,
        "geflammt: nur {grau_enden} von {enden} Enden grau"
    );
    assert!(
        gleich * 10 >= geflammt * 9,
        "beide Enden gleich: nur {gleich} von {geflammt}"
    );
    let silber = silber as f64 / geflammt as f64 * 100.0;
    assert!(
        (silber - 36.0).abs() <= 6.0,
        "Enden silbergrau (fend=64): {silber:.1} %"
    );

    let mit_fuge = [fam[0], fam[1], fam[2], rgb(FUGE_FB)];
    let mut n = [0usize; 4];
    for i in 0..800 {
        for k in 0..500 {
            let c = farbe(&p, 2.5 + i as f64 * 5.0, 2.5 + k as f64 * 5.0);
            n[naechste(c, &mit_fuge)] += 1;
        }
    }
    let stein = (n[0] + n[1] + n[2]) as f64;
    let pct = |x: usize| x as f64 / stein * 100.0;
    let (rot, braun, silber) = (pct(n[0]), pct(n[1]), pct(n[2]));
    assert!((rot - 43.0).abs() <= 3.0, "Rot nach Fläche {rot:.1} %");
    assert!(
        (braun - 36.0).abs() <= 4.0,
        "Braun-grau nach Fläche {braun:.1} %"
    );
    assert!(
        (silber - 21.0).abs() <= 4.0,
        "Silbergrau nach Fläche {silber:.1} %"
    );

    for key in ["flame", "fend"] {
        assert!(
            grenzen("masonry").contains(&(key, 0.0, 100.0)),
            "{key}: {:?}",
            grenzen("masonry")
        );
    }
    let mut s = Scene::with_model(Model::with_seed(270));
    gebaeude(&mut s);
    let gb = flaeche(s.model(), "Porenbeton");
    let g = sguid(s.model(), gb);
    muster_setzen(&mut s, gb, Some(fb.clone()));
    let text = sk_model::szo::write(s.model());
    let z = zeilen(&text, "[pattern]")
        .into_iter()
        .find(|l| l.contains(&format!("surface={g}")))
        .expect("Zeile");
    for t in [" flame=100", " fend=64", " hpal=87493c:50;675549:50"] {
        assert!(z.contains(t), "{t} fehlt: {z}");
    }
    let l = lesen(&text);
    assert!(l.hints.is_empty(), "{:?}", l.hints);
    assert_eq!(
        muster(&l.model, flaeche(&l.model, "Porenbeton")).as_ref(),
        Some(fb)
    );
    let ohne: String = text
        .lines()
        .filter(|l| *l != z)
        .map(|l| format!("{l}\n"))
        .collect();
    for (key, alt) in [("flame", " flame=100"), ("fend", " fend=64")] {
        for (wert, gut) in [("0", true), ("100", true), ("101", false), ("-1", false)] {
            let zeile = z.replace(alt, &format!(" {key}={wert}"));
            let l = lesen(&format!("{ohne}{zeile}\n"));
            let m = muster(&l.model, flaeche(&l.model, "Porenbeton"));
            assert_eq!(
                l.hints.is_empty() && m.is_some(),
                gut,
                "{zeile}: {:?}",
                l.hints
            );
        }
    }
}

/// A271 (Regel 69 ersetzt, BIM-Nachtrag 2): „Sichtbeton mittelgrau“,
/// Maßstab 1 px = 1 mm.
/// - Ohne Tafelstöße (Stoßbreite 0) und ohne Ankerlöcher: keine
///   Fugenlinien.
/// - Lunker (Kern Faktor 0,35): Auf einem 1-mm-Raster über 1 m² sind
///   gegenüber `pores=0` bei 0,5 % zwischen 0,2 und 0,8 % der Punkte
///   dunkler als 0,6 × glatt; mit 2 % das 3- bis 5-Fache.
/// - Lunker höchstens 4,5 mm: kein solcher Lauf in einer Rasterzeile
///   über 5 mm.
/// - Sandpunkte wachsen mit: bei 0,5 % gibt es hellere Punkte.
/// - Grenze 0–2 % (2,1 ungültig).
#[test]
fn a271_sichtbeton_lunker() {
    let v = vorlage("Sichtbeton mittelgrau");
    let p = &v.pattern;
    assert!(
        fugen(p, -5000.0, -2000.0, 7600.0, 3000.0).is_empty(),
        "keine Stöße"
    );
    let glatt = mit_poren(p, 0.0);
    let zaehlen = |q: &Pattern| {
        let (mut n, mut lunker, mut heller, mut laengster) = (0usize, 0usize, 0usize, 0usize);
        for k in 0..1000 {
            let mut lauf = 0;
            for i in 0..1000 {
                let (u, w) = (0.5 + i as f64, 0.5 + k as f64);
                let (a, b) = (farbe_auf(q, v.base, u, w), farbe_auf(&glatt, v.base, u, w));
                n += 1;
                if hell(a) > hell(b) {
                    heller += 1;
                }
                if hell(a) < 0.6 * hell(b) {
                    lunker += 1;
                    lauf += 1;
                    laengster = laengster.max(lauf);
                } else {
                    lauf = 0;
                }
            }
        }
        (lunker as f64 / n as f64 * 100.0, laengster, heller)
    };
    let (anteil, laengster, heller) = zaehlen(p);
    assert!((0.2..=0.8).contains(&anteil), "Lunker 0,5 %: {anteil:.3} %");
    assert!((1..=5).contains(&laengster), "Lunkergröße: {laengster} mm");
    assert!(heller > 0, "Sandpunkte");
    let (viel, _, _) = zaehlen(&mit_poren(p, 2.0));
    let faktor = viel / anteil;
    assert!(
        (3.0..=5.0).contains(&faktor),
        "2 % gegen 0,5 %: Faktor {faktor:.2}"
    );
    assert!(gueltig(&mit_poren(p, 2.0)) && !gueltig(&mit_poren(p, 2.1)));
}

/// A272 (Entscheidung 31 erweitert, Regeln 59/60, BIM-Nachtrag 2):
/// Werksmuster je Baustoffart in der Datei. Ein neues Projekt schreibt
/// drei `[pattern]`-Zeilen: Verblender (wild, friesisch-bunt mit Köpfen,
/// Flammung und Relief), Putz (Reibeputz K2) und Stahlbeton
/// (`gen=concrete`, Lunker 0,5 %, Wolkigkeit 2 %, ohne Stöße). Eine alte
/// Datei ohne `[pattern]` öffnet ohne Hinweis und schreibt danach genau
/// dieselben Zeilen.
#[test]
fn a272_werksmuster_je_baustoffart() {
    let mut s = Scene::with_model(Model::with_seed(272));
    gebaeude(&mut s);
    let m = s.model();
    let text = sk_model::szo::write(m);
    assert!(text.starts_with("SZO 4\n"));
    let z = zeilen(&text, "[pattern]");
    assert_eq!(z.len(), 3, "drei Werksmuster: {z:?}");
    for (baustoff, teile) in [
        (
            "Verblender (Vormauerziegel)",
            &[
                "gen=masonry",
                "len=240",
                "h=71",
                "joint=10",
                "bond=wild",
                "jrgb=d1cbc2",
                "pal=87493c:79;675549:9;7b6d65:12",
                "hpal=87493c:50;675549:50",
                "flame=100",
                "fend=64",
                "relief=100",
                "spread=7",
            ][..],
        ),
        ("Putz", &["gen=plaster", "grain=2", "spread=4"][..]),
        ("Stahlbeton", &["gen=concrete", "pores=0.5", "cloud=2"][..]),
    ] {
        let g = sguid(m, flaeche(m, baustoff));
        let zeile = z
            .iter()
            .find(|l| l.contains(&format!("surface={g}")))
            .unwrap_or_else(|| panic!("{baustoff}: keine Zeile"));
        for t in teile {
            assert!(
                zeile.contains(&format!(" {t}")),
                "{baustoff}: {t} fehlt in {zeile}"
            );
        }
    }
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
}

/// Bild einer Probe wie `--musterprobe` (b7-kennwerte.md §1): `b` × `h`
/// Bildpunkte zu `mm`, Ausschnitt ab (u0, v0), v nach oben, Bildzeile 0
/// oben, ohne Licht. Je Bildpunkt die Farbe in der Pixelmitte.
fn probe(
    p: &Pattern,
    base: [u8; 3],
    (b, h): (usize, usize),
    mm: f64,
    (u0, v0): (f64, f64),
) -> Vec<[u8; 3]> {
    let mut bild = Vec::with_capacity(b * h);
    for y in 0..h {
        for x in 0..b {
            let u = u0 + (x as f64 + 0.5) * mm;
            let v = v0 + ((h - y) as f64 - 0.5) * mm;
            bild.push(farbe_auf(p, base, u, v));
        }
    }
    bild
}

/// Luma auf sRGB wie kennwerte.py.
fn luma(c: [u8; 3]) -> f64 {
    0.2126 * c[0] as f64 + 0.7152 * c[1] as f64 + 0.0722 * c[2] as f64
}

fn mittelfarbe<'a>(it: impl Iterator<Item = &'a [u8; 3]>) -> [f64; 3] {
    let (mut s, mut n) = ([0.0f64; 3], 0.0);
    for c in it {
        for k in 0..3 {
            s[k] += c[k] as f64;
        }
        n += 1.0;
    }
    s.map(|x| x / n)
}

fn streuung(a: &[f64]) -> f64 {
    let n = a.len() as f64;
    let m = a.iter().sum::<f64>() / n;
    (a.iter().map(|x| (x - m).powi(2)).sum::<f64>() / n).sqrt()
}

/// Mittel im Fenster (2r+1)², Ränder gespiegelt (numpy `reflect`).
fn kasten(a: &[f64], (b, h): (usize, usize), r: usize) -> Vec<f64> {
    let spiegel = |i: isize, n: usize| -> usize {
        let n = n as isize;
        let i = if i < 0 { -i } else { i };
        (if i >= n { 2 * n - 2 - i } else { i }) as usize
    };
    let (pb, ph) = (b + 2 * r, h + 2 * r);
    let mut c = vec![0.0f64; (pb + 1) * (ph + 1)];
    for y in 0..ph {
        for x in 0..pb {
            let v =
                a[spiegel(y as isize - r as isize, h) * b + spiegel(x as isize - r as isize, b)];
            c[(y + 1) * (pb + 1) + x + 1] =
                v + c[y * (pb + 1) + x + 1] + c[(y + 1) * (pb + 1) + x] - c[y * (pb + 1) + x];
        }
    }
    let n = 2 * r + 1;
    let mut out = vec![0.0; b * h];
    for y in 0..h {
        for x in 0..b {
            let s =
                c[(y + n) * (pb + 1) + x + n] - c[y * (pb + 1) + x + n] - c[(y + n) * (pb + 1) + x]
                    + c[y * (pb + 1) + x];
            out[y * b + x] = s / (n * n) as f64;
        }
    }
    out
}

/// Flecken (4er-Nachbarschaft) mit mindestens `min` Bildpunkten.
fn flecken(maske: &[bool], (b, h): (usize, usize), min: usize) -> usize {
    let mut gesehen = vec![false; b * h];
    let mut n = 0;
    for s in 0..b * h {
        if !maske[s] || gesehen[s] {
            continue;
        }
        gesehen[s] = true;
        let (mut stapel, mut k) = (vec![s], 0);
        while let Some(i) = stapel.pop() {
            k += 1;
            let (x, y) = (i % b, i / b);
            let mut nb = Vec::with_capacity(4);
            if x > 0 {
                nb.push(i - 1);
            }
            if x + 1 < b {
                nb.push(i + 1);
            }
            if y > 0 {
                nb.push(i - b);
            }
            if y + 1 < h {
                nb.push(i + b);
            }
            for j in nb {
                if maske[j] && !gesehen[j] {
                    gesehen[j] = true;
                    stapel.push(j);
                }
            }
        }
        if k >= min {
            n += 1;
        }
    }
    n
}

fn nah(ist: [f64; 3], soll: [f64; 3], tol: f64, was: &str) {
    for k in 0..3 {
        assert!(
            (ist[k] - soll[k]).abs() <= tol,
            "{was}: {ist:?} statt {soll:?} ± {tol}"
        );
    }
}

fn im(ist: f64, soll: f64, tol: f64, was: &str) {
    assert!(
        (ist - soll).abs() <= tol,
        "{was}: {ist:.3} statt {soll} ± {tol}"
    );
}

/// A273 (B7 als Kennwertvergleich, einstellungen/b7-kennwerte.md §1/§2,
/// Rechenweg einstellungen/kennwerte.py; Koordinator 19:02): Die drei
/// Werksvorlagen treffen Jörns Vorlagen in den Bildkennwerten. Die
/// Probe wird hier auf der CPU (`sample`) gerechnet; die GPU-Kachel aus
/// `--musterprobe` gleicht ihr nach Regel 61 bitgleich (B7). Bezug ist
/// die Spalte „Vorlage“, Toleranz wie §2.
/// - Klinker (1024 × 868, 1,464 mm, ab (3000, 2000)): Fugenanteil
///   (min > 175, max − min < 30) 0,164 ± 0,015; Fugenfarbe (209, 203,
///   194) ± 6; Steinmittel (119, 85, 74) ± 6; Bildmittel (134, 104, 94)
///   ± 6; Rillenanteil (L < 60 im Stein) 0,115 ± 0,03.
/// - Reibeputz (512², 0,35 mm, ab (1000, 400)): Bildmittel ± 2; σ(L)
///   10,0 ± 1,5; Plateau 242 ± 2 mit Anteil 0,61 ± 0,08; Wolkigkeit
///   (σ des 17²-Mittels) ≤ 2,5.
/// - Sichtbeton (512², 1 mm, ab (1000, 400)): Bildmittel ± 2; σ(L)
///   10,1 ± 1,5; Wolkigkeit 2,0 ± 0,8; Lunker (L < 13²-Mittel − 30,
///   ≥ 3 px) 11,3 ± 3 je 100 × 100 px.
///
/// Flächenanteile und Flammung aus der Familienkarte prüft A270,
/// Kopfanteil und Verband A268.
#[test]
fn a273_kennwerte_wie_vorlage() {
    let k = vorlage("Klinker friesisch-bunt");
    let bild = probe(&k.pattern, k.base, (1024, 868), 1.464, (3000.0, 2000.0));
    let fuge: Vec<bool> = bild
        .iter()
        .map(|c| {
            let (mn, mx) = (*c.iter().min().unwrap(), *c.iter().max().unwrap());
            mn > 175 && mx - mn < 30
        })
        .collect();
    let n_fuge = fuge.iter().filter(|&&f| f).count();
    im(
        n_fuge as f64 / bild.len() as f64,
        0.164,
        0.015,
        "Klinker Fugenanteil",
    );
    let stein: Vec<&[u8; 3]> = bild
        .iter()
        .zip(&fuge)
        .filter(|(_, f)| !**f)
        .map(|(c, _)| c)
        .collect();
    nah(
        mittelfarbe(bild.iter().zip(&fuge).filter(|(_, f)| **f).map(|(c, _)| c)),
        [209.0, 203.0, 194.0],
        6.0,
        "Klinker Fugenfarbe",
    );
    nah(
        mittelfarbe(stein.iter().copied()),
        [119.0, 85.0, 74.0],
        6.0,
        "Klinker Steinmittel",
    );
    nah(
        mittelfarbe(bild.iter()),
        [134.0, 104.0, 94.0],
        6.0,
        "Klinker Bildmittel",
    );
    let rillen = stein.iter().filter(|c| luma(***c) < 60.0).count();
    im(
        rillen as f64 / stein.len() as f64,
        0.115,
        0.03,
        "Klinker Rillenanteil",
    );

    let gr = (512, 512);
    let v = vorlage("Reibeputz weiß");
    let bild = probe(&v.pattern, v.base, gr, 0.35, (1000.0, 400.0));
    let l: Vec<f64> = bild.iter().map(|c| luma(*c)).collect();
    nah(
        mittelfarbe(bild.iter()),
        [235.6, 235.6, 236.6],
        2.0,
        "Putz Bildmittel",
    );
    im(streuung(&l), 10.0, 1.5, "Putz σ(L)");
    let mut zahl = std::collections::BTreeMap::new();
    for x in &l {
        *zahl.entry(x.round() as i64).or_insert(0usize) += 1;
    }
    let plateau = zahl
        .iter()
        .max_by_key(|(w, n)| (**n, -**w))
        .map(|(w, _)| *w as f64)
        .unwrap();
    im(plateau, 242.0, 2.0, "Putz Plateau");
    let anteil = l.iter().filter(|x| (*x - plateau).abs() <= 1.0).count() as f64 / l.len() as f64;
    im(anteil, 0.61, 0.08, "Putz Plateauanteil");
    let wolke = streuung(&kasten(&l, gr, 8));
    assert!(wolke <= 2.5, "Putz Wolkigkeit {wolke:.2}");

    let v = vorlage("Sichtbeton mittelgrau");
    let bild = probe(&v.pattern, v.base, gr, 1.0, (1000.0, 400.0));
    let l: Vec<f64> = bild.iter().map(|c| luma(*c)).collect();
    nah(
        mittelfarbe(bild.iter()),
        [141.6, 142.0, 141.0],
        2.0,
        "Beton Bildmittel",
    );
    im(streuung(&l), 10.1, 1.5, "Beton σ(L)");
    im(streuung(&kasten(&l, gr, 8)), 2.0, 0.8, "Beton Wolkigkeit");
    let ort = kasten(&l, gr, 6);
    let maske: Vec<bool> = l.iter().zip(&ort).map(|(a, m)| *a < m - 30.0).collect();
    let dichte = flecken(&maske, gr, 3) as f64 * 1e4 / l.len() as f64;
    im(dichte, 11.3, 3.0, "Beton Lunker je 100 × 100 px");
}
