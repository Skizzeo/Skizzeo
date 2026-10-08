//! Prozedurale Muster der Oberflächen (Pakete 6 und 7): Mauerwerk, Putz,
//! Sichtbeton, Holzschalung, Platten und Naturstein, aus wenigen Zahlen
//! erzeugt, ohne Bild.
//!
//! [`sample`] ist die Formel für Vorschau und Test; der Fragment-Shader
//! (`sk_render::PATTERN_GLSL`) rechnet Zeile für Zeile dasselbe. Der Zufall
//! ist ein reiner Ganzzahl-Hash ([`hash`], BIM-Regel 61): gleiche Datei,
//! gleiches Bild auf jedem Rechner. Die Formeln der Feinheiten (Flammung,
//! Relief, Reibeputz, Lunker, Holz, Naturstein) stehen in `texgen`.
//!
//! Koordinaten: `u` waagerecht längs der Fläche, `v` senkrecht ab ±0,00, in
//! mm. Lagerfugen liegen mit ihrer Mitte bei `v = k·(h + Fuge)`.

use crate::guid::Guid;
use crate::szo::{Line, Record};
use crate::texgen::{self, InStone};
use sk_math::{vec2, Rect2, Vec2};

/// Verband des Mauerwerks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Bond {
    /// Läufer halbsteinig versetzt.
    Half,
    /// Läufer drittelsteinig versetzt.
    Third,
    /// Wilder Verband aus Läufern und Köpfen (Regel 58, Jörns Vorlage
    /// „friesisch-bunt“) aus der Verbandstabelle, siehe [`bond_table`].
    Wild,
    /// Blockverband: Läufer- und Kopfschichten im Wechsel, alle
    /// Läuferschichten übereinander (Paket 7).
    Block,
    /// Kreuzverband: wie Block, jede zweite Läuferschicht um einen halben
    /// Stein versetzt.
    Cross,
}

/// Bis zu drei Farben mit Anteil in % (Summe 100; Anteil 0 = nicht gesetzt).
pub type Palette = [([u8; 3], f32); 3];

/// Muster einer Oberfläche.
#[derive(Clone, Debug, PartialEq)]
pub enum Pattern {
    /// Steine mit Fugen. Maße in mm, Anteile der Steinfarben in % (Summe
    /// 100, Farben mit Anteil 0 gelten als nicht gesetzt), Streuung in %
    /// Helligkeit je Stein. Paket 7: `hpal` Farben der Köpfe (fehlt es,
    /// gilt `palette`), `flame` Anteil der geflammten roten Läufer %,
    /// `fend` Anteil davon mit braun-grauen Enden % (Regel 68), `relief`
    /// Rillen und Feinkorn % (Regel 70).
    Masonry {
        len: f32,
        h: f32,
        joint: f32,
        bond: Bond,
        joint_rgb: [u8; 3],
        palette: Palette,
        hpal: Option<Palette>,
        flame: f32,
        fend: f32,
        relief: f32,
        spread: f32,
        seed: u32,
    },
    /// Putz (Reibeputz, paket-7 §8.5): Körnung in mm, Streuung = Tiefe der
    /// Schatten in %, Farbe der Oberfläche.
    Plaster { grain: f32, spread: f32, seed: u32 },
    /// Sichtbeton (Regel 69): Schaltafel `w` × `h` mm, Stoßbreite `joint`
    /// mm (0 = keine Stöße), Ankerlöcher, Wolkigkeit % und Lunker %
    /// (`pores`), Farbe der Oberfläche.
    Concrete {
        w: f32,
        h: f32,
        joint: f32,
        anchors: bool,
        cloud: f32,
        pores: f32,
        seed: u32,
    },
    /// Holzschalung: Bretter senkrecht oder waagerecht, Brettbreite und
    /// Fuge in mm, Maserung %, zwei Holzfarben im Wechsel nach Hash.
    Timber {
        vertical: bool,
        board: f32,
        joint: f32,
        grain: f32,
        c1: [u8; 3],
        c2: [u8; 3],
        seed: u32,
    },
    /// Platten `len` × `wid` mm im Kreuzfugenraster, mit `half` je zweite
    /// Reihe um eine halbe Platte versetzt.
    Tiles {
        len: f32,
        wid: f32,
        joint: f32,
        half: bool,
        joint_rgb: [u8; 3],
        palette: Palette,
        spread: f32,
        seed: u32,
    },
    /// Naturstein: Zellen (Voronoi) mit mittlerer Größe `size` mm, Fuge mm,
    /// Unregelmäßigkeit `irr` %.
    Stone {
        size: f32,
        joint: f32,
        irr: f32,
        joint_rgb: [u8; 3],
        palette: Palette,
        seed: u32,
    },
    /// Zeile einer neueren Fassung mit unbekanntem `gen=` (F-17): bleibt
    /// bytegleich, dargestellt wird ohne Muster.
    Foreign(String),
}

/// Steinformate nach DIN 105 / DIN 4172 (Regel 58): Name, Länge, Höhe (mm).
pub const BRICK_FORMATS: [(&str, f32, f32); 5] = [
    ("NF", 240.0, 71.0),
    ("DF", 240.0, 52.0),
    ("2DF", 240.0, 113.0),
    ("WF", 210.0, 50.0),
    ("WDF", 210.0, 65.0),
];

/// Länge und Höhe eines Steinformats.
pub fn brick_format(name: &str) -> Option<(f32, f32)> {
    BRICK_FORMATS
        .iter()
        .find(|f| f.0 == name)
        .map(|f| (f.1, f.2))
}

/// Wertebereiche (Regel 57).
pub const JOINT_MM: (f32, f32) = (6.0, 15.0);
pub const LEN_MM: (f32, f32) = (50.0, 600.0);
pub const HEIGHT_MM: (f32, f32) = (20.0, 300.0);
pub const SPREAD_MASONRY: f32 = 20.0;
pub const SPREAD_PLASTER: f32 = 10.0;
pub const GRAIN_MM: (f32, f32) = (0.5, 5.0);
/// Größter Startwert (BIM-Nachtrag 4 zu Regel 57/61): 24 Bit, damit der
/// Shader ihn als `float` genau bekommt.
pub const SEED_MAX: u32 = 0xff_ffff;

/// Grenzen der Regler je Art (Regeln 57, 62, 68–70): Schlüssel wie in der
/// Datei, kleinster und größter Wert. Test liest dieselbe Tabelle.
const LIMITS_MASONRY: [(&str, f32, f32); 7] = [
    ("len", LEN_MM.0, LEN_MM.1),
    ("h", HEIGHT_MM.0, HEIGHT_MM.1),
    ("joint", JOINT_MM.0, JOINT_MM.1),
    ("spread", 0.0, SPREAD_MASONRY),
    ("flame", 0.0, 100.0),
    ("fend", 0.0, 100.0),
    ("relief", 0.0, 100.0),
];
const LIMITS_PLASTER: [(&str, f32, f32); 2] = [
    ("grain", GRAIN_MM.0, GRAIN_MM.1),
    ("spread", 0.0, SPREAD_PLASTER),
];
const LIMITS_CONCRETE: [(&str, f32, f32); 6] = [
    ("w", 500.0, 6000.0),
    ("h", 250.0, 3000.0),
    ("joint", 0.0, 10.0),
    ("anchors", 0.0, 1.0),
    ("cloud", 0.0, 20.0),
    ("pores", 0.0, 2.0),
];
const LIMITS_TIMBER: [(&str, f32, f32); 3] = [
    ("board", 40.0, 400.0),
    ("joint", 0.0, 30.0),
    ("grain", 0.0, 20.0),
];
const LIMITS_TILES: [(&str, f32, f32); 4] = [
    ("len", 50.0, 1500.0),
    ("wid", 50.0, 1500.0),
    ("joint", 2.0, 20.0),
    ("spread", 0.0, 20.0),
];
const LIMITS_STONE: [(&str, f32, f32); 3] = [
    ("size", 80.0, 1000.0),
    ("joint", 5.0, 40.0),
    ("irr", 0.0, 100.0),
];

/// Grenzen der Regler zum `gen=`-Wort; unbekanntes Wort: leer.
pub fn limits(gen: &str) -> &'static [(&'static str, f32, f32)] {
    match gen {
        "masonry" => &LIMITS_MASONRY,
        "plaster" => &LIMITS_PLASTER,
        "concrete" => &LIMITS_CONCRETE,
        "timber" => &LIMITS_TIMBER,
        "tiles" => &LIMITS_TILES,
        "stone" => &LIMITS_STONE,
        _ => &[],
    }
}

/// `gen=`-Wort einer Art.
pub fn gen_word(p: &Pattern) -> &'static str {
    match p {
        Pattern::Masonry { .. } => "masonry",
        Pattern::Plaster { .. } => "plaster",
        Pattern::Concrete { .. } => "concrete",
        Pattern::Timber { .. } => "timber",
        Pattern::Tiles { .. } => "tiles",
        Pattern::Stone { .. } => "stone",
        Pattern::Foreign(_) => "",
    }
}

/// Werte eines Musters zu den Schlüsseln aus [`limits`].
fn limited_values(p: &Pattern) -> Vec<f32> {
    match p {
        Pattern::Masonry {
            len,
            h,
            joint,
            spread,
            flame,
            fend,
            relief,
            ..
        } => vec![*len, *h, *joint, *spread, *flame, *fend, *relief],
        Pattern::Plaster { grain, spread, .. } => vec![*grain, *spread],
        Pattern::Concrete {
            w,
            h,
            joint,
            anchors,
            cloud,
            pores,
            ..
        } => vec![*w, *h, *joint, *anchors as u8 as f32, *cloud, *pores],
        Pattern::Timber {
            board,
            joint,
            grain,
            ..
        } => vec![*board, *joint, *grain],
        Pattern::Tiles {
            len,
            wid,
            joint,
            spread,
            ..
        } => vec![*len, *wid, *joint, *spread],
        Pattern::Stone {
            size, joint, irr, ..
        } => vec![*size, *joint, *irr],
        Pattern::Foreign(_) => Vec::new(),
    }
}

/// Wert zum Schlüssel aus [`limits`] (Ankerlöcher 0/1); anderer Schlüssel
/// oder andere Art: `None`. Für die Regler (Paket 7b).
pub fn value(p: &Pattern, key: &str) -> Option<f32> {
    limits(gen_word(p))
        .iter()
        .zip(limited_values(p))
        .find(|(l, _)| l.0 == key)
        .map(|(_, v)| v)
}

/// Setzt den Wert zum Schlüssel aus [`limits`]; `false`, wenn die Art ihn
/// nicht hat. Geprüft wird erst mit [`validate`].
pub fn set_value(p: &mut Pattern, key: &str, v: f32) -> bool {
    let slot = match (p, key) {
        (Pattern::Masonry { len, .. }, "len") => len,
        (Pattern::Masonry { h, .. }, "h") => h,
        (Pattern::Masonry { joint, .. }, "joint") => joint,
        (Pattern::Masonry { spread, .. }, "spread") => spread,
        (Pattern::Masonry { flame, .. }, "flame") => flame,
        (Pattern::Masonry { fend, .. }, "fend") => fend,
        (Pattern::Masonry { relief, .. }, "relief") => relief,
        (Pattern::Plaster { grain, .. }, "grain") => grain,
        (Pattern::Plaster { spread, .. }, "spread") => spread,
        (Pattern::Concrete { w, .. }, "w") => w,
        (Pattern::Concrete { h, .. }, "h") => h,
        (Pattern::Concrete { joint, .. }, "joint") => joint,
        (Pattern::Concrete { anchors, .. }, "anchors") => {
            *anchors = v >= 0.5;
            return true;
        }
        (Pattern::Concrete { cloud, .. }, "cloud") => cloud,
        (Pattern::Concrete { pores, .. }, "pores") => pores,
        (Pattern::Timber { board, .. }, "board") => board,
        (Pattern::Timber { joint, .. }, "joint") => joint,
        (Pattern::Timber { grain, .. }, "grain") => grain,
        (Pattern::Tiles { len, .. }, "len") => len,
        (Pattern::Tiles { wid, .. }, "wid") => wid,
        (Pattern::Tiles { joint, .. }, "joint") => joint,
        (Pattern::Tiles { spread, .. }, "spread") => spread,
        (Pattern::Stone { size, .. }, "size") => size,
        (Pattern::Stone { joint, .. }, "joint") => joint,
        (Pattern::Stone { irr, .. }, "irr") => irr,
        _ => return false,
    };
    *slot = v;
    true
}

/// Steinfarben mit Anteilen (Mauerwerk: Läufer, Platten, Naturstein).
pub fn palette_mut(p: &mut Pattern) -> Option<&mut Palette> {
    match p {
        Pattern::Masonry { palette, .. }
        | Pattern::Tiles { palette, .. }
        | Pattern::Stone { palette, .. } => Some(palette),
        _ => None,
    }
}

/// Fugenfarbe (Mauerwerk, Platten, Naturstein).
pub fn joint_rgb_mut(p: &mut Pattern) -> Option<&mut [u8; 3]> {
    match p {
        Pattern::Masonry { joint_rgb, .. }
        | Pattern::Tiles { joint_rgb, .. }
        | Pattern::Stone { joint_rgb, .. } => Some(joint_rgb),
        _ => None,
    }
}

/// Anteile ganzzahlig, Summe 100.
fn check_palette(palette: &Palette) -> Result<(), String> {
    let mut sum = 0.0;
    for (_, a) in palette {
        if !(a.is_finite() && *a >= 0.0 && a.fract() == 0.0) {
            return Err(format!("Anteil {a} nicht ganzzahlig"));
        }
        sum += a;
    }
    if sum != 100.0 {
        return Err(format!("Anteile ergeben {sum} statt 100 %"));
    }
    Ok(())
}

/// Prüft die Werte (Regeln 57, 62); `Err` nennt den ersten Verstoß.
pub fn validate(p: &Pattern) -> Result<(), String> {
    let seed = seed_of(p);
    if seed > SEED_MAX {
        return Err(format!("Startwert {seed} außerhalb 0–{SEED_MAX}"));
    }
    for (&(key, lo, hi), v) in limits(gen_word(p)).iter().zip(limited_values(p)) {
        if !(v.is_finite() && v >= lo && v <= hi) {
            return Err(format!("{key} {v} außerhalb {lo}–{hi}"));
        }
    }
    match p {
        Pattern::Masonry { palette, hpal, .. } => {
            check_palette(palette)?;
            hpal.as_ref().map_or(Ok(()), check_palette)
        }
        Pattern::Tiles { palette, .. } | Pattern::Stone { palette, .. } => check_palette(palette),
        _ => Ok(()),
    }
}

// --- Vorlagen und Werksmuster -------------------------------------------

/// Kernfarben der Läufer „Röben Jever friesisch-bunt“ (BIM-Nachtrag 2,
/// 18:25, nach referenz/texturen/auswertung.md): Rot, Braun-grau, Silbergrau.
const FRIES: Palette = [
    ([0x87, 0x49, 0x3c], 79.0),
    ([0x67, 0x55, 0x49], 9.0),
    ([0x7b, 0x6d, 0x65], 12.0),
];
/// Köpfe friesisch-bunt: Rot und Braun-grau je 50 %.
const FRIES_HEADS: Palette = [
    ([0x87, 0x49, 0x3c], 50.0),
    ([0x67, 0x55, 0x49], 50.0),
    ([0, 0, 0], 0.0),
];

/// Vorlage im Fenster „Muster“: Name, Muster und Grundfarbe der Oberfläche.
#[derive(Clone, Debug, PartialEq)]
pub struct PatternPreset {
    pub name: String,
    pub pattern: Pattern,
    pub base: [u8; 3],
}

/// Firmenvorlage im Firmenkatalog (`[patternpreset]` in `.szk`, Regel 65).
#[derive(Clone, Debug, PartialEq)]
pub struct CompanyPreset {
    pub guid: Guid,
    pub name: String,
    pub pattern: Pattern,
    pub base: [u8; 3],
}

fn masonry(
    (len, h, joint): (f32, f32, f32),
    bond: Bond,
    joint_rgb: [u8; 3],
    palette: Palette,
    spread: f32,
    seed: u32,
) -> Pattern {
    Pattern::Masonry {
        len,
        h,
        joint,
        bond,
        joint_rgb,
        palette,
        hpal: None,
        flame: 0.0,
        fend: 64.0,
        relief: 0.0,
        spread,
        seed,
    }
}

fn preset(name: &str, pattern: Pattern, base: [u8; 3]) -> PatternPreset {
    PatternPreset {
        name: name.to_string(),
        pattern,
        base,
    }
}

fn make_presets() -> Vec<PatternPreset> {
    let red: Palette = [
        ([0x8a, 0x3b, 0x2a], 40.0),
        ([0x9c, 0x4a, 0x33], 35.0),
        ([0x6e, 0x2f, 0x22], 25.0),
    ];
    let fries = Pattern::Masonry {
        len: 240.0,
        h: 71.0,
        joint: 10.0,
        bond: Bond::Wild,
        joint_rgb: [0xd1, 0xcb, 0xc2],
        palette: FRIES,
        hpal: Some(FRIES_HEADS),
        flame: 100.0,
        fend: 64.0,
        relief: 100.0,
        spread: 7.0,
        seed: 17,
    };
    let ks: Palette = [
        ([0xe6, 0xe3, 0xda], 40.0),
        ([0xde, 0xdb, 0xd2], 35.0),
        ([0xec, 0xe9, 0xe2], 25.0),
    ];
    vec![
        preset(
            "Klinker rot",
            masonry(
                (240.0, 71.0, 10.0),
                Bond::Half,
                [0xd8, 0xd4, 0xcc],
                red,
                6.0,
                17,
            ),
            [0x8a, 0x3b, 0x2a],
        ),
        preset("Klinker friesisch-bunt", fries, [0x87, 0x49, 0x3c]),
        preset(
            "Kalksandstein sichtbar",
            masonry(
                (240.0, 113.0, 10.0),
                Bond::Half,
                [0xc9, 0xc6, 0xbe],
                ks,
                3.0,
                9,
            ),
            [0xe6, 0xe3, 0xda],
        ),
        preset(
            "Reibeputz weiß",
            Pattern::Plaster {
                grain: 2.0,
                spread: 4.0,
                seed: 3,
            },
            [0xec, 0xec, 0xed],
        ),
        preset(
            "Putz grob",
            Pattern::Plaster {
                grain: 3.0,
                spread: 6.0,
                seed: 4,
            },
            [0xec, 0xe9, 0xe0],
        ),
        preset(
            "Sichtbeton mittelgrau",
            Pattern::Concrete {
                w: 2500.0,
                h: 500.0,
                joint: 0.0,
                anchors: false,
                cloud: 2.0,
                pores: 0.5,
                seed: 11,
            },
            [0x8e, 0x8e, 0x8d],
        ),
        preset(
            "Holzschalung Lärche",
            Pattern::Timber {
                vertical: true,
                board: 120.0,
                joint: 8.0,
                grain: 5.0,
                c1: [0xb0, 0x80, 0x54],
                c2: [0xc0, 0x92, 0x64],
                seed: 21,
            },
            [0xb8, 0x89, 0x5c],
        ),
        preset(
            "Betonplatten 40 × 40",
            Pattern::Tiles {
                len: 400.0,
                wid: 400.0,
                joint: 5.0,
                half: false,
                joint_rgb: [0x6e, 0x6e, 0x6a],
                palette: [
                    ([0x9a, 0x9a, 0x96], 40.0),
                    ([0xa6, 0xa5, 0xa0], 35.0),
                    ([0x8e, 0x8e, 0x8a], 25.0),
                ],
                spread: 4.0,
                seed: 13,
            },
            [0xa0, 0xa0, 0x9c],
        ),
        preset(
            "Naturstein",
            Pattern::Stone {
                size: 300.0,
                joint: 15.0,
                irr: 60.0,
                joint_rgb: [0x96, 0x92, 0x8a],
                palette: [
                    ([0xbf, 0xa9, 0x8a], 40.0),
                    ([0xa8, 0x91, 0x6f], 35.0),
                    ([0xcd, 0xbb, 0x9c], 25.0),
                ],
                seed: 31,
            },
            [0xbf, 0xa9, 0x8a],
        ),
    ]
}

/// Die neun Werksvorlagen in Listenreihenfolge (paket-7 §2.2, §8.1).
pub fn presets() -> &'static [PatternPreset] {
    static P: std::sync::OnceLock<Vec<PatternPreset>> = std::sync::OnceLock::new();
    P.get_or_init(make_presets)
}

/// Werksvorlage mit diesem Namen.
pub fn preset_named(name: &str) -> Option<&'static PatternPreset> {
    presets().iter().find(|p| p.name == name)
}

/// Name der Werks-Oberfläche des Verblenders.
pub const FACING: &str = "Verblender (Vormauerziegel)";
/// Name der Werks-Oberfläche des Putzes.
pub const PLASTER: &str = "Putz";
/// Name der Werks-Oberfläche des Stahlbetons.
pub const CONCRETE: &str = "Stahlbeton";

/// Guids der Werks-Oberflächen mit Werksmuster: in jedem Projekt dieselben
/// (die eines neuen Projekts), damit das Werksmuster an der Oberfläche
/// hängt und nicht an ihrem Namen (Regel 60, BIM-Befund 6a).
pub const FACING_SURFACE_GUID: Guid = Guid(0x9202c7c486854fed87d5792dac919409);
pub const PLASTER_SURFACE_GUID: Guid = Guid(0xd6c929c9454e426199aaa965bc7425fb);
pub const CONCRETE_SURFACE_GUID: Guid = Guid(0x01243c6e2af74256bbb674ca2a7e8e2b);

/// Guid der Werks-Oberfläche mit diesem Namen (beim Anlegen des
/// Startbestands).
pub fn factory_guid(surface: &str) -> Option<Guid> {
    match surface {
        FACING => Some(FACING_SURFACE_GUID),
        PLASTER => Some(PLASTER_SURFACE_GUID),
        CONCRETE => Some(CONCRETE_SURFACE_GUID),
        _ => None,
    }
}

/// Werksmuster der Werks-Oberfläche mit der Guid `g`.
pub fn factory_for(g: Guid) -> Option<Pattern> {
    match g {
        FACING_SURFACE_GUID => factory(FACING),
        PLASTER_SURFACE_GUID => factory(PLASTER),
        CONCRETE_SURFACE_GUID => factory(CONCRETE),
        _ => None,
    }
}

/// Werksmuster einer Oberfläche des Startbestands (nach ihrem Namen): die
/// drei Vorlagen nach Jörns Referenztexturen (paket-7 §8.1).
pub fn factory(surface: &str) -> Option<Pattern> {
    let name = match surface {
        FACING => "Klinker friesisch-bunt",
        PLASTER => "Reibeputz weiß",
        CONCRETE => "Sichtbeton mittelgrau",
        _ => return None,
    };
    preset_named(name).map(|p| p.pattern.clone())
}

/// Vorgabe beim Wählen von „Mauerwerk“ an einer Oberfläche ohne Muster
/// (paket-6 §1.2): wie das Werksmuster des Verblenders.
pub fn masonry_default() -> Pattern {
    factory(FACING).expect("Werksmuster Verblender")
}

/// Vorgabe beim Wählen von „Putz“: Körnung 2, Streuung 4.
pub fn plaster_default() -> Pattern {
    factory(PLASTER).expect("Werksmuster Putz")
}

/// Startwert eines Musters (Fremdes: 0).
pub fn seed_of(p: &Pattern) -> u32 {
    match p {
        Pattern::Masonry { seed, .. }
        | Pattern::Plaster { seed, .. }
        | Pattern::Concrete { seed, .. }
        | Pattern::Timber { seed, .. }
        | Pattern::Tiles { seed, .. }
        | Pattern::Stone { seed, .. } => *seed,
        Pattern::Foreign(_) => 0,
    }
}

/// Dasselbe Muster mit anderem Startwert.
pub fn with_seed(p: &Pattern, s: u32) -> Pattern {
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

// --- Zufall -------------------------------------------------------------

/// lowbias32 (Chris Wellons): kleiner Ganzzahl-Hash mit guter Streuung.
pub const fn lowbias32(mut x: u32) -> u32 {
    x ^= x >> 16;
    x = x.wrapping_mul(0x7feb_352d);
    x ^= x >> 15;
    x = x.wrapping_mul(0x846c_a68b);
    x ^= x >> 16;
    x
}

/// Zufallszahl je Stein (Reihe, Stein, Startwert), Regel 61. Gleiche
/// Formel in `PATTERN_GLSL`.
pub fn hash(row: i32, col: i32, seed: u32) -> u32 {
    let a = lowbias32((row as u32) ^ seed.wrapping_mul(0x9e37_79b9));
    lowbias32(a.wrapping_add((col as u32).wrapping_mul(0x85eb_ca6b)))
}

/// Gleichverteilt in [0, 1) aus den oberen 24 Bit (in `f32` exakt, wie im
/// Shader).
fn unit(h: u32) -> f32 {
    (h >> 8) as f32 / 16_777_216.0
}

// --- Wilder Verband -----------------------------------------------------
//
// Rechnung in Vierteln q = (Länge + Fuge)/4 (62,5 mm bei NF). Ein Läufer
// ist 4 q, ein Kopf 2 q lang. Schicht r hat ihre Stoßfugen auf den Vierteln
// mit der Parität von r; benachbarte Schichten teilen darum nie eine
// Stoßfuge, die Überbindung ist mindestens ¼ Stein.
//
// Der Verband steht in einer Tabelle von 128 Schichten × 128 Vierteln, die
// ringsum periodisch ist (paket-6 §8.1): bei NF 8,0 m breit, 10,37 m hoch.
// Ob eine Fuge eine Treppe oder Kette zu lang macht, hängt an den Schichten
// darunter; das lässt sich nicht je Pixel aus einem Hash entscheiden. Die
// Tabelle hängt nur vom Startwert ab. Die Farben nutzen die absolute
// Stein-Nummer, darum wiederholt sich das Farbbild nie.
//
// Regeln (Regel 58, Koordinator 18:13, 18:58, 19:07), auch über beide Nähte:
// nur Läufer und Köpfe, höchstens 4 Läufer und 2 Köpfe in Folge, Treppen
// über höchstens 5 Schichten, dieselbe Fuge in k, k + 2, … höchstens
// [`WILD_CHAIN`]-mal, in je 8 Schichten keine Folge doppelt (auch
// verschoben), Kopfanteil um 30 % mit Streuung je Schicht.
//
// Erzeugung Schicht für Schicht von unten: Jede mögliche Fuge bekommt
// Kosten nach der Länge der Treppen und Ketten, die sie verlängert (zu lang
// = verboten), dazu ein Rauschen aus dem Hash. Die Schicht ist ein Kreis aus
// Läufern und Köpfen mit den geringsten Kosten (Rückwärtsrechnung über die
// Stellen, 6 Zustände: 1–4 Läufer bzw. 1–2 Köpfe in Folge). Linien aus
// Schicht 0 zählen länger, und kurz vor der Naht schauen die Kosten in die
// untersten Schichten; so schließt sich der Kreis oben. Geht eine Schicht
// nicht, rechnet die Suche einige Schichten darunter mit neuem Rauschen neu.

/// Schichten und Viertel der Verbandstabelle.
pub const WILD_SIZE: usize = 128;
/// Treppen über höchstens so viele Schichten.
pub const WILD_STAIR: u32 = 5;
/// Dieselbe Stoßfuge in k, k + 2, k + 4 … höchstens so oft.
pub const WILD_CHAIN: u32 = 4;

/// Salz für die Streuung je Stein.
const SPREAD_SALT: u32 = 0x68e3_1da4;
/// Salz der normalverteilten Streuung mit Relief (wie muster.py).
const RELIEF_SALT: u32 = 0x27d4_eb2d;

/// Verbandstabelle: je Schicht und Viertel ein Byte, Bits 0–1 Abstand zum
/// Steinanfang in Vierteln, Bit 2 Kopf. Zeile für Zeile (Schicht 0 zuerst),
/// so lädt sie auch die Grafikkarte (R8UI, 128 × 128).
#[derive(Debug, PartialEq, Eq)]
pub struct BondTable {
    pub cells: Vec<u8>,
}

impl BondTable {
    /// Byte an Schicht `row`, Viertel `q` (beliebig, periodisch).
    pub fn cell(&self, row: i32, q: i32) -> u8 {
        let n = WILD_SIZE as i32;
        self.cells[row.rem_euclid(n) as usize * WILD_SIZE + q.rem_euclid(n) as usize]
    }

    /// Stein an Viertel `q` der Schicht `row`: Anfang (absolut) und Länge
    /// in Vierteln.
    pub fn stone(&self, row: i32, q: i32) -> (i32, i32) {
        let c = self.cell(row, q);
        (q - (c & 3) as i32, if c & 4 != 0 { 2 } else { 4 })
    }
}

/// Fertige Verbandstabelle des Werksmusters (Startwert 17), mitgeliefert,
/// damit der erste Start nicht rechnet (Koordinator 19:55). Der Test
/// `werkstabelle_wie_gerechnet` hält sie gleich mit [`wild_table`].
const BUILTIN_SEED: u32 = 17;
static BUILTIN_TABLE: &[u8; WILD_SIZE * WILD_SIZE] = include_bytes!("verband17.bin");

type Tables = Vec<(u32, std::sync::Arc<BondTable>)>;

/// Gerechnete Tabellen (die zuletzt genutzten 16) und Startwerte, die
/// gerade im Hintergrund gerechnet werden.
static TABLES: std::sync::Mutex<(Tables, Vec<u32>)> =
    std::sync::Mutex::new((Vec::new(), Vec::new()));
/// Zählt fertig gewordene Hintergrundrechnungen ([`bond_generation`]).
static GENERATION: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

fn tables() -> std::sync::MutexGuard<'static, (Tables, Vec<u32>)> {
    TABLES.lock().unwrap_or_else(|e| e.into_inner())
}

/// Schon vorhandene Tabelle (mitgeliefert oder gerechnet).
fn cached(c: &mut Tables, seed: u32) -> Option<std::sync::Arc<BondTable>> {
    if seed == BUILTIN_SEED {
        static B: std::sync::OnceLock<std::sync::Arc<BondTable>> = std::sync::OnceLock::new();
        return Some(
            B.get_or_init(|| {
                std::sync::Arc::new(BondTable {
                    cells: BUILTIN_TABLE.to_vec(),
                })
            })
            .clone(),
        );
    }
    let i = c.iter().position(|(s, _)| *s == seed)?;
    let t = c.remove(i);
    c.push(t.clone());
    Some(t.1)
}

fn keep(c: &mut Tables, seed: u32, t: std::sync::Arc<BondTable>) {
    // Ohne Obergrenze: eine Tabelle sind 16 KiB, und jede Oberfläche mit
    // wildem Verband braucht ihre ständig (Review 3s, A296)
    if !c.iter().any(|(s, _)| *s == seed) {
        c.push((seed, t));
    }
}

/// Verbandstabelle zum Startwert; je Startwert einmal gerechnet und
/// gehalten. Rechnet, wenn nötig, sofort (bis rund 0,5 s): für Vorschau,
/// Ansicht und Test. Der Zeichenpfad nimmt [`bond_table_ready`].
pub fn bond_table(seed: u32) -> std::sync::Arc<BondTable> {
    if let Some(t) = cached(&mut tables().0, seed) {
        return t;
    }
    let t = std::sync::Arc::new(wild_table(seed));
    keep(&mut tables().0, seed, t.clone());
    t
}

/// Verbandstabelle, wenn sie schon vorliegt; sonst wird sie im Hintergrund
/// gerechnet (einmal je Startwert) und die Antwort ist `None`: Die Fläche
/// zeigt bis dahin ihre Mischfarbe (Koordinator 19:55).
pub fn bond_table_ready(seed: u32) -> Option<std::sync::Arc<BondTable>> {
    let mut g = tables();
    if let Some(t) = cached(&mut g.0, seed) {
        return Some(t);
    }
    if !g.1.contains(&seed) {
        g.1.push(seed);
        std::thread::spawn(move || {
            let t = std::sync::Arc::new(wild_table(seed));
            let mut g = tables();
            keep(&mut g.0, seed, t);
            g.1.retain(|s| *s != seed);
            GENERATION.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        });
    }
    None
}

/// Lässt sich das Muster sofort zeichnen? Der wilde Verband braucht seine
/// Tabelle; fehlt sie, wird sie im Hintergrund gerechnet
/// ([`bond_table_ready`]) und die Vorschau zeigt bis dahin die Mischfarbe.
pub fn pattern_ready(p: &Pattern) -> bool {
    match p {
        Pattern::Masonry {
            bond: Bond::Wild,
            seed,
            ..
        } => bond_table_ready(*seed).is_some(),
        _ => true,
    }
}

/// Wird gerade eine Tabelle im Hintergrund gerechnet?
pub fn bond_tables_pending() -> bool {
    !tables().1.is_empty()
}

/// Zähler der fertigen Hintergrundrechnungen: ändert er sich, liegen neue
/// Tabellen vor.
pub fn bond_generation() -> u64 {
    GENERATION.load(std::sync::atomic::Ordering::SeqCst)
}

type Row = u128;
const W: usize = WILD_SIZE;
const R: usize = WILD_SIZE;
const INF: u32 = u32::MAX / 4;
/// Kosten einer Linie nach ihrer Länge.
const WT: [u32; 9] = [0, 0, 1, 3, 9, 27, 81, 243, 729];
/// Linien aus Schicht 0 zählen so viel länger.
const BOTTOM: u32 = 3;
/// Rauschen je Fuge, Kosten je Läufer, Kopf (Kopfanteil ≈ 30 %).
const NOISE: u32 = 200;
const COST_RUNNER: u32 = 165;
const COST_HEAD: i64 = 40;

fn bit(m: Row, q: i64) -> bool {
    (m >> q.rem_euclid(W as i64)) & 1 == 1
}

/// Lauf nach einem Stein der Länge `l` (Zustand 0–3: 1–4 Läufer,
/// 4–5: 1–2 Köpfe in Folge).
fn run_next(s: usize, l: usize) -> Option<usize> {
    match (l, s) {
        (4, 0..=2) => Some(s + 1),
        (4, 3) => None,
        (4, _) => Some(0),
        (_, 4) => Some(5),
        (_, 5) => None,
        _ => Some(4),
    }
}

/// Länge der Linie durch Fuge (r, q), je Schicht `dr` weiter und `dq`
/// versetzt: nach unten und über die Naht in die schon erzeugten
/// untersten Schichten. Dazu, ob sie bis Schicht 0 reicht.
fn line(rows: &[Row], r: usize, q: usize, dr: usize, dq: i64) -> (u32, bool) {
    let mut n = 1;
    let mut rr = r as i64 - dr as i64;
    let mut x = q as i64 - dq;
    while rr >= 0 && bit(rows[rr as usize], x) {
        n += 1;
        rr -= dr as i64;
        x -= dq;
    }
    let bottom = rr + (dr as i64) < dr as i64;
    let mut rr = r + dr;
    let mut x = q as i64 + dq;
    while rr >= R && rr - R < r && bit(rows[rr - R], x) {
        n += 1;
        rr += dr;
        x += dq;
    }
    (n, bottom)
}

/// Vorschau über die Naht für Schichten kurz vor dem Ende: Die Linie läuft
/// in den untersten Schichten weiter; zählt sie mit den Schichten dazwischen
/// zu lang, kostet die Fuge mehr.
fn ahead(rows: &[Row], r: usize, q: usize, dr: usize, dq: i64, n: u32, lim: u32) -> u32 {
    let gap = (R - 1 - r) / dr;
    if gap == 0 || gap > 6 {
        return 0;
    }
    let mut rr = r + dr * (gap + 1) - R;
    let mut x = q as i64 + dq * (gap as i64 + 1);
    let mut u = 0;
    while rr < r && bit(rows[rr], x) {
        u += 1;
        rr += dr;
        x += dq;
    }
    let tot = n + gap as u32 + u;
    if u == 0 || tot <= lim {
        return 0;
    }
    (40 * (tot - lim).min(6)) >> (gap - 1).min(3)
}

/// Kosten einer Fuge je Viertel der Schicht `r` (INF = verboten).
fn joint_costs(rows: &[Row], r: usize, sd: u32) -> [u32; W] {
    let mut c = [INF; W];
    for q in (r & 1..W).step_by(2) {
        let (a, a0) = line(rows, r, q, 1, 1);
        let (b, b0) = line(rows, r, q, 1, -1);
        let (k, k0) = line(rows, r, q, 2, 0);
        if a > WILD_STAIR || b > WILD_STAIR || k > WILD_CHAIN {
            continue;
        }
        let w = |n: u32, bottom: bool| WT[(n + if bottom { BOTTOM } else { 0 }).min(8) as usize];
        let look = ahead(rows, r, q, 1, 1, a, WILD_STAIR)
            + ahead(rows, r, q, 1, -1, b, WILD_STAIR)
            + 3 * ahead(rows, r, q, 2, 0, k, WILD_CHAIN);
        c[q] = 10 * (w(a, a0) + w(b, b0) + 3 * w(k, k0))
            + look
            + hash(r as i32, q as i32, sd) % (NOISE + 1);
    }
    c
}

/// Günstigste Schicht als Kreis ab Fuge `start`: beginnt mit einem Läufer,
/// endet mit einem Kopf (so zählen die Folgen über die Naht richtig).
fn cheapest_row(c: &[u32; W], start: usize, cl: u32, ck: u32) -> Option<(u32, Row)> {
    if c[start] >= INF || c[(start + 4) % W] >= INF {
        return None;
    }
    let mut best = [[INF; 6]; W + 1];
    let mut pick = [[0u8; 6]; W + 1];
    best[W][4] = 0;
    best[W][5] = 0;
    for p in (4..W).step_by(2).rev() {
        for s in 0..6 {
            for l in [2usize, 4] {
                if p + l > W {
                    continue;
                }
                let j = if p + l < W { c[(start + p + l) % W] } else { 0 };
                let Some(t) = run_next(s, l) else { continue };
                if j >= INF || best[p + l][t] >= INF {
                    continue;
                }
                let v = best[p + l][t] + j + if l == 2 { ck } else { cl };
                if v < best[p][s] {
                    best[p][s] = v;
                    pick[p][s] = l as u8;
                }
            }
        }
    }
    if best[4][0] >= INF {
        return None;
    }
    let mut m: Row = (1 << start) | (1 << ((start + 4) % W));
    let (mut p, mut s) = (4, 0);
    while p < W {
        let l = pick[p][s] as usize;
        s = run_next(s, l)?;
        p += l;
        if p < W {
            m |= 1 << ((start + p) % W);
        }
    }
    Some((best[4][0] + c[start] + c[(start + 4) % W] + cl, m))
}

/// Steinfolge einer Schicht unabhängig von der Verschiebung (kleinste
/// Drehung der Längen).
fn row_key(m: Row) -> Vec<u8> {
    let js: Vec<usize> = (0..W).filter(|&q| bit(m, q as i64)).collect();
    let d: Vec<u8> = (0..js.len())
        .map(|i| (js.get(i + 1).copied().unwrap_or(js[0] + W) - js[i]) as u8)
        .collect();
    (0..d.len())
        .map(|i| [&d[i..], &d[..i]].concat())
        .min()
        .unwrap_or_default()
}

/// Alle Schichten; None, wenn die Suche festläuft.
fn wild_rows(seed: u32) -> Option<Vec<Row>> {
    let mut rows = vec![0 as Row; R];
    let mut keys: Vec<Vec<u8>> = vec![Vec::new(); R];
    let mut tries = vec![0u32; R];
    let mut back = 0u32;
    let mut r = 0;
    while r < R {
        let salt = tries[r].wrapping_mul(0x27d4_eb2f);
        let c = joint_costs(&rows, r, lowbias32(seed ^ salt ^ 0x51ed));
        // Kopfneigung je Schicht gestreut (Kopfanteil schwankt)
        let ck = (COST_HEAD + (hash(r as i32, 0xb1a5, seed) % 21) as i64 - 10).max(0) as u32;
        let mut found: Option<(u32, Row)> = None;
        for t in 0..W / 2 {
            let h = hash(r as i32, t as i32, seed ^ salt ^ 0x7f4a);
            let start = (r & 1) + 2 * (h as usize % (W / 2));
            if let Some(x) = cheapest_row(&c, start, COST_RUNNER, ck) {
                if found.is_none_or(|f| x.0 < f.0) {
                    found = Some(x);
                }
            }
            if found.is_some() && t >= 6 {
                break;
            }
        }
        if let Some((_, m)) = found {
            // keine Folge doppelt in 8 Schichten, auch über die Naht
            let key = row_key(m);
            let twin = (r.saturating_sub(7)..r).any(|x| keys[x] == key)
                || (r + 8 > R && (0..r + 8 - R).any(|x| keys[x] == key));
            if !twin || tries[r] >= 1000 {
                keys[r] = key;
                rows[r] = m;
                r += 1;
                continue;
            }
            tries[r] += 1;
            continue;
        }
        back += 1;
        if back > 2000 || r == 0 {
            return None;
        }
        let d = if r + 10 > R {
            3 + back as usize % 14
        } else {
            2 + back as usize % 6
        };
        let r0 = r.saturating_sub(d);
        for x in r0..=r {
            rows[x] = 0;
            keys[x].clear();
            if x > r0 {
                tries[x] = 0;
            }
        }
        tries[r0] += 1;
        r = r0;
    }
    Some(rows)
}

fn wild_table(seed: u32) -> BondTable {
    let rows = (0..64u32)
        .find_map(|k| wild_rows(seed ^ k.wrapping_mul(0x9e37_79b1)))
        .unwrap_or_else(fallback_rows);
    let mut cells = vec![0u8; R * W];
    for (r, &m) in rows.iter().enumerate() {
        let js: Vec<usize> = (0..W).filter(|&q| bit(m, q as i64)).collect();
        for (i, &a) in js.iter().enumerate() {
            let b = js.get(i + 1).copied().unwrap_or(js[0] + W);
            let head = if b - a == 2 { 4 } else { 0 };
            for k in 0..b - a {
                cells[r * W + (a + k) % W] = k as u8 | head;
            }
        }
    }
    BondTable { cells }
}

/// Notfall, falls keine Suche schließt (kommt in den Tests nicht vor):
/// je Schicht 12 × (Läufer, Läufer, Kopf) und zwei Läufer, versetzt.
fn fallback_rows() -> Vec<Row> {
    (0..R)
        .map(|r| {
            let mut m: Row = 0;
            let mut q = (r & 1) + 2 * (r * 5 % (W / 2));
            for i in 0..38 {
                m |= 1 << (q % W);
                q += if i % 3 == 2 && i < 36 { 2 } else { 4 };
            }
            m
        })
        .collect()
}
// --- Stein und Fuge -----------------------------------------------------

/// Lage eines Punkts im Mauerwerk: Reihe, Stein, Fuge ja/nein, Kopf, Lage
/// im Stein.
struct Spot {
    row: i32,
    stone: i32,
    joint: bool,
    head: bool,
    s: InStone,
}

#[allow(clippy::too_many_arguments)]
fn locate(len: f64, h: f64, joint: f64, bond: Bond, seed: u32, u: f64, v: f64) -> Spot {
    let course = h + joint;
    let rf = ((v + joint / 2.0) / course).floor();
    let dv = v + joint / 2.0 - rf * course;
    let row = rf as i32;
    let a = len + joint;
    // Stein, Lage ab der Stoßfugenmitte am Steinanfang, Achsmaß, Kopf
    let regular = |off: f64, pitch: f64| {
        let col = ((u + off) / pitch).floor();
        (col as i32, u + off - col * pitch, pitch)
    };
    let (stone, x, pitch, head) = match bond {
        Bond::Wild => {
            // Viertel-Koordinate; Stoßfugen auf ganzen Vierteln
            let q = 4.0 * u / a;
            let (start, n) = bond_table(seed).stone(row, q.floor() as i32);
            (
                start,
                (q - start as f64) * a / 4.0,
                n as f64 * a / 4.0,
                n == 2,
            )
        }
        Bond::Half | Bond::Third => {
            let off = match bond {
                Bond::Half => (row & 1) as f64 * a / 2.0,
                _ => row.rem_euclid(3) as f64 * a / 3.0,
            };
            let (c, x, p) = regular(off, a);
            (c, x, p, false)
        }
        Bond::Block | Bond::Cross => {
            if row.rem_euclid(2) == 1 {
                // Kopfschicht: halbe Steine, um ¼ Stein versetzt
                let (c, x, p) = regular(a / 4.0, a / 2.0);
                (c, x, p, true)
            } else {
                let shift = bond == Bond::Cross && row.div_euclid(2).rem_euclid(2) == 1;
                let (c, x, p) = regular(if shift { a / 2.0 } else { 0.0 }, a);
                (c, x, p, false)
            }
        }
    };
    Spot {
        row,
        stone,
        joint: dv < joint || x.min(pitch - x) < joint / 2.0,
        head,
        s: InStone {
            uin: x - joint / 2.0,
            slen: pitch - joint,
            vin: dv - joint,
            h,
        },
    }
}

fn to_u8(c: texgen::Rgb) -> [u8; 3] {
    c.map(|x| x.round().clamp(0.0, 255.0) as u8)
}

/// Helligkeit je Stein bzw. Platte: ± Streuung % gleichverteilt (Paket 6).
fn spread_factor(h: u32, spread: f32) -> f64 {
    if spread <= 0.0 {
        return 1.0;
    }
    (1.0 + spread / 100.0 * (2.0 * unit(lowbias32(h ^ SPREAD_SALT)) - 1.0)) as f64
}

/// Farbe des Mauerwerks an (u, v): Familie nach den Anteilen (Köpfe nach
/// `hpal`), Flammung roter Läufer, Streuung, Relief.
fn masonry_rgb(p: &Pattern, u: f64, v: f64) -> [u8; 3] {
    let Pattern::Masonry {
        len,
        h,
        joint,
        bond,
        joint_rgb,
        palette,
        hpal,
        flame,
        fend,
        relief,
        spread,
        seed,
    } = p
    else {
        return [0; 3];
    };
    let sp = locate(*len as f64, *h as f64, *joint as f64, *bond, *seed, u, v);
    if sp.joint {
        return if *relief > 0.0 {
            to_u8(texgen::relief_joint(*joint_rgb, *seed, u, v))
        } else {
            *joint_rgb
        };
    }
    let hs = hash(sp.row, sp.stone, *seed);
    let pal = match hpal {
        Some(hp) if sp.head => hp,
        _ => palette,
    };
    let (fam0, c0) = texgen::pick(pal, hs);
    let mut c = texgen::rgb(c0);
    let mut fam = fam0;
    if *flame > 0.0 && !sp.head && fam0 == 0 {
        if let Some((a, silver)) =
            texgen::flame_at(sp.row, sp.stone, *seed, *flame, *fend, &sp.s, u, v)
        {
            let end = texgen::rgb(palette[if silver { 2 } else { 1 }].0);
            for k in 0..3 {
                c[k] = c[k] * (1.0 - a) + end[k] * a;
            }
            if a > 0.5 {
                fam = if silver { 2 } else { 1 };
            }
        }
    }
    // Mit Relief streut die Helligkeit wie in Jörns Vorlage normalverteilt
    // (σ = Streuung, einstellungen/muster.py); ohne bleibt das Paket-6-Bild
    let f = if *relief > 0.0 {
        1.0 + *spread as f64 / 100.0 * texgen::gauss3(lowbias32(hs ^ RELIEF_SALT))
    } else {
        spread_factor(hs, *spread)
    };
    if f != 1.0 {
        c = c.map(|x| x * f);
    }
    if *relief > 0.0 {
        c = texgen::relief_stone(c, fam == 2, *seed, *relief, &sp.s, u, v);
    }
    to_u8(c)
}

/// Stein des Mauerwerks für die Probe `--musterprobe` (b7-kennwerte §1).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProbeStone {
    /// Schicht und Steinanfang (wilder Verband: Viertel, sonst Spalte).
    pub row: i32,
    pub start: i32,
    pub head: bool,
    /// Kernfarbe: Nummer der Palettenfarbe (Köpfe nach `hpal`).
    pub core: usize,
    pub geflammt: bool,
}

/// Familie am Punkt (u, v) aus der Rechnung, nicht aus der Farbe: 0/1/2 =
/// erste/zweite/dritte Palettenfarbe (geflammte Enden zählen zur
/// Endfarbe), `None` = Fuge; dazu der Stein. Nur Mauerwerk, sonst `None`.
pub fn masonry_probe(p: &Pattern, u: f64, v: f64) -> Option<(usize, ProbeStone)> {
    let Pattern::Masonry {
        len,
        h,
        joint,
        bond,
        palette,
        hpal,
        flame,
        fend,
        seed,
        ..
    } = p
    else {
        return None;
    };
    let sp = locate(*len as f64, *h as f64, *joint as f64, *bond, *seed, u, v);
    if sp.joint {
        return None;
    }
    let hs = hash(sp.row, sp.stone, *seed);
    let pal = match hpal {
        Some(hp) if sp.head => hp,
        _ => palette,
    };
    let (core, _) = texgen::pick(pal, hs);
    let mut fam = core;
    let mut geflammt = false;
    if *flame > 0.0 && !sp.head && core == 0 {
        if let Some((a, silver)) =
            texgen::flame_at(sp.row, sp.stone, *seed, *flame, *fend, &sp.s, u, v)
        {
            geflammt = true;
            if a > 0.5 {
                fam = if silver { 2 } else { 1 };
            }
        }
    }
    Some((
        fam,
        ProbeStone {
            row: sp.row,
            start: sp.stone,
            head: sp.head,
            core,
            geflammt,
        },
    ))
}

/// Farbe des Musters an (u, v) in mm; `base` ist die Farbe der Oberfläche
/// (Putz, Sichtbeton). Ohne Ausblenden in die Ferne (das macht der Shader).
pub fn sample(p: &Pattern, base: [u8; 3], u: f64, v: f64) -> [u8; 3] {
    match p {
        Pattern::Masonry { .. } => masonry_rgb(p, u, v),
        Pattern::Plaster {
            grain,
            spread,
            seed,
        } => to_u8(texgen::plaster(base, *grain, *spread, *seed, u, v)),
        Pattern::Concrete {
            w,
            h,
            joint,
            anchors,
            cloud,
            pores,
            seed,
        } => to_u8(texgen::concrete(
            base, *w, *h, *joint, *anchors, *cloud, *pores, *seed, u, v,
        )),
        Pattern::Timber {
            vertical,
            board,
            joint,
            grain,
            c1,
            c2,
            seed,
        } => to_u8(texgen::timber(
            *vertical, *board, *joint, *grain, *c1, *c2, *seed, u, v,
        )),
        Pattern::Tiles {
            len,
            wid,
            joint,
            half,
            joint_rgb,
            palette,
            spread,
            seed,
        } => {
            let (row, col, j) =
                texgen::tile_at(*len as f64, *wid as f64, *joint as f64, *half, u, v);
            if j {
                return *joint_rgb;
            }
            let hs = hash(row, col, *seed);
            let f = spread_factor(hs, *spread);
            to_u8(texgen::rgb(texgen::pick(palette, hs).1).map(|x| x * f))
        }
        Pattern::Stone {
            size,
            joint,
            irr,
            joint_rgb,
            palette,
            seed,
        } => {
            let ((cx, cy), edge) =
                texgen::stone_cell(*size as f64, *irr as f64 / 100.0, *seed, u, v);
            if edge < *joint as f64 / 2.0 {
                return *joint_rgb;
            }
            let hs = hash(cx, cy, seed.wrapping_add(1));
            let f = spread_factor(hs, STONE_SPREAD);
            to_u8(texgen::rgb(texgen::pick(palette, hs).1).map(|x| x * f))
        }
        Pattern::Foreign(_) => base,
    }
}

/// Feste Streuung je Naturstein in % (kein Regler).
const STONE_SPREAD: f32 = 4.0;

/// Kopfanteil des wilden Verbands (Köpfe / alle Steine), solange die
/// Tabelle des Startwerts nicht vorliegt.
const WILD_HEADS: f64 = 0.3;

/// Kopfanteil (Köpfe / alle Steine) der Verbandstabelle zum Startwert, wenn
/// sie schon vorliegt (die mitgelieferte immer); wartet nie und stößt keine
/// Rechnung an.
fn head_share(seed: u32) -> Option<f64> {
    use std::sync::Mutex;
    static SHARES: Mutex<Vec<(u32, f64)>> = Mutex::new(Vec::new());
    let mut v = SHARES.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(&(_, k)) = v.iter().find(|x| x.0 == seed) {
        return Some(k);
    }
    let t = cached(&mut tables().0, seed)?;
    // Jedes Viertel trägt den Stein, in dem es liegt: Köpfe sind 2, Läufer
    // 4 Viertel lang
    let quarters = |head: bool| t.cells.iter().filter(|&&c| (c & 4 != 0) == head).count();
    let (heads, runs) = (quarters(true) as f64 / 2.0, quarters(false) as f64 / 4.0);
    let k = heads / (heads + runs);
    v.push((seed, k));
    Some(k)
}

/// Mittlere Kernbreite geflammter Läufer (Regel 68).
const FLAME_CORE: f64 = 0.53;

/// Mischfarbe aus der Ferne. Mauerwerk aus den Flächenanteilen der Farben
/// (der wilde Verband mit dem Kopfanteil seiner Tabelle, solange sie noch
/// rechnet mit 30 %; wartet nie); die
/// übrigen Arten als Mittel von [`sample`] über 64 × 64 Punkte
/// (paket-7 §8.6), je Muster einmal gerechnet.
pub fn mix(p: &Pattern, base: [u8; 3]) -> [u8; 3] {
    match p {
        Pattern::Foreign(_) => base,
        Pattern::Masonry { .. } => masonry_mix(p),
        _ => {
            use std::sync::Mutex;
            static CACHE: Mutex<Vec<(String, [u8; 3])>> = Mutex::new(Vec::new());
            let key = format!("{p:?}{base:?}");
            if let Some(c) = CACHE
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .iter()
                .find(|x| x.0 == key)
            {
                return c.1;
            }
            let mut acc = [0.0f64; 3];
            for i in 0..64 {
                for k in 0..64 {
                    let c = sample(p, base, 13.7 + i as f64 * 61.3, 7.9 + k as f64 * 47.9);
                    for j in 0..3 {
                        acc[j] += c[j] as f64;
                    }
                }
            }
            let c = to_u8(acc.map(|x| x / 4096.0));
            let mut cache = CACHE.lock().unwrap_or_else(|e| e.into_inner());
            if cache.len() >= 64 {
                cache.remove(0);
            }
            cache.push((key, c));
            c
        }
    }
}

/// Mischfarbe des Mauerwerks (wie `mix_rgb` in einstellungen/muster.py):
/// Läufer und Köpfe nach ihrer Fläche, Flammung, Relief, Fuge.
fn masonry_mix(p: &Pattern) -> [u8; 3] {
    let Pattern::Masonry {
        len,
        h,
        joint,
        bond,
        joint_rgb,
        palette,
        hpal,
        flame,
        fend,
        relief,
        seed,
        ..
    } = p
    else {
        return [0; 3];
    };
    let (len, h, j) = (*len as f64, *h as f64, *joint as f64);
    let a = len + j;
    // Anteil der Köpfe an der Steinfläche
    let heads = match bond {
        Bond::Half | Bond::Third => 0.0,
        Bond::Block | Bond::Cross => {
            let k = a / 2.0 - j;
            k / (k + len)
        }
        Bond::Wild => {
            let k = head_share(*seed).unwrap_or(WILD_HEADS);
            let (lf, kf) = ((1.0 - k) * len, k * (a / 2.0 - j));
            kf / (lf + kf)
        }
    };
    let norm = |pal: &Palette| {
        let t: f32 = pal.iter().map(|x| x.1).sum();
        pal.map(|(c, s)| (texgen::rgb(c), if t > 0.0 { (s / t) as f64 } else { 0.0 }))
    };
    let run = norm(palette);
    let head = norm(hpal.as_ref().unwrap_or(palette));
    let (f, fe) = (*flame as f64 / 100.0, *fend as f64 / 100.0);
    let mut c = [0.0f64; 3];
    for k in 0..3 {
        let mut x = 0.0;
        for (i, (rgb, s)) in run.iter().enumerate() {
            let mut share = *s;
            if i == 0 {
                share *= 1.0 - f + f * FLAME_CORE;
            }
            x += rgb[k] * share;
        }
        let red = run[0].1 * f * (1.0 - FLAME_CORE);
        x += run[1].0[k] * red * fe + run[2].0[k] * red * (1.0 - fe);
        let y: f64 = head.iter().map(|(rgb, s)| rgb[k] * s).sum();
        c[k] = x * (1.0 - heads) + y * heads;
    }
    let rl = *relief as f64 / 100.0;
    if rl > 0.0 {
        c = c.map(|x| x * (1.0 - 0.75 * 0.075 * rl) + 25.0 * 0.136 * rl);
    }
    let area = len * h / (a * (h + j));
    let jr = texgen::rgb(*joint_rgb);
    to_u8([0, 1, 2].map(|k| jr[k] * (1.0 - area) + c[k] * area))
}

/// Strecke auf das Rechteck beschnitten (Liang–Barsky).
fn clip(a: Vec2, b: Vec2, r: Rect2) -> Option<(Vec2, Vec2)> {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let (mut t0, mut t1) = (0.0f64, 1.0f64);
    for (p, q) in [
        (-dx, a.x - r.min.x),
        (dx, r.max.x - a.x),
        (-dy, a.y - r.min.y),
        (dy, r.max.y - a.y),
    ] {
        if p == 0.0 {
            if q < 0.0 {
                return None;
            }
        } else {
            let t = q / p;
            if p < 0.0 {
                t0 = t0.max(t);
            } else {
                t1 = t1.min(t);
            }
        }
    }
    (t0 < t1).then(|| {
        (
            vec2(a.x + t0 * dx, a.y + t0 * dy),
            vec2(a.x + t1 * dx, a.y + t1 * dy),
        )
    })
}

/// Linien `x = k·pitch − off` (senkrecht, `vertical`) bzw. `y = …` über das
/// ganze Rechteck.
fn grid_lines(out: &mut Vec<(Vec2, Vec2)>, r: Rect2, pitch: f64, vertical: bool) {
    let (lo, hi) = if vertical {
        (r.min.x, r.max.x)
    } else {
        (r.min.y, r.max.y)
    };
    for k in (lo / pitch).ceil() as i64..=(hi / pitch).floor() as i64 {
        let x = k as f64 * pitch;
        out.push(if vertical {
            (vec2(x, r.min.y), vec2(x, r.max.y))
        } else {
            (vec2(r.min.x, x), vec2(r.max.x, x))
        });
    }
}

/// Fugen als Mittellinien in einem Rechteck (mm): Lagerfugen über die ganze
/// Breite, Stoßfugen je Reihe zwischen ihren Lagerfugen, Naturstein als
/// Zellgrenzen; alles auf das Rechteck beschnitten. Für die Ansichtskachel,
/// Test und später PDF/DXF.
pub fn joint_lines(p: &Pattern, rect: Rect2) -> Vec<(Vec2, Vec2)> {
    let (u0, v0, u1, v1) = (rect.min.x, rect.min.y, rect.max.x, rect.max.y);
    let mut out = Vec::new();
    match p {
        Pattern::Masonry {
            len,
            h,
            joint,
            bond,
            seed,
            ..
        } => {
            let (len, h, joint) = (*len as f64, *h as f64, *joint as f64);
            let course = h + joint;
            let a = len + joint;
            grid_lines(&mut out, rect, course, false);
            let r0 = (v0 / course).floor() as i32;
            let r1 = (v1 / course).ceil() as i32;
            for row in r0..r1 {
                let lo = (row as f64 * course).max(v0);
                let hi = ((row + 1) as f64 * course).min(v1);
                if hi <= lo {
                    continue;
                }
                let mut push = |u: f64| {
                    if u >= u0 && u <= u1 {
                        out.push((vec2(u, lo), vec2(u, hi)));
                    }
                };
                let (off, pitch) = match bond {
                    Bond::Wild => {
                        let t = bond_table(*seed);
                        let q0 = (4.0 * u0 / a).ceil() as i32;
                        let q1 = (4.0 * u1 / a).floor() as i32;
                        for q in q0..=q1 {
                            if t.cell(row, q) & 3 == 0 {
                                push(q as f64 * a / 4.0);
                            }
                        }
                        continue;
                    }
                    Bond::Half => ((row & 1) as f64 * a / 2.0, a),
                    Bond::Third => (row.rem_euclid(3) as f64 * a / 3.0, a),
                    Bond::Block | Bond::Cross if row.rem_euclid(2) == 1 => (a / 4.0, a / 2.0),
                    Bond::Block => (0.0, a),
                    Bond::Cross => (row.div_euclid(2).rem_euclid(2) as f64 * a / 2.0, a),
                };
                let c0 = ((u0 + off) / pitch).ceil() as i64;
                let c1 = ((u1 + off) / pitch).floor() as i64;
                for c in c0..=c1 {
                    push(c as f64 * pitch - off);
                }
            }
        }
        Pattern::Concrete { w, h, joint, .. } => {
            if *joint > 0.0 {
                grid_lines(&mut out, rect, *w as f64, true);
                grid_lines(&mut out, rect, *h as f64, false);
            }
        }
        Pattern::Timber {
            vertical,
            board,
            joint,
            ..
        } => grid_lines(&mut out, rect, (*board + *joint) as f64, *vertical),
        Pattern::Tiles {
            len,
            wid,
            joint,
            half,
            ..
        } => {
            let (pu, pv) = ((*len + *joint) as f64, (*wid + *joint) as f64);
            grid_lines(&mut out, rect, pv, false);
            for row in (v0 / pv).floor() as i64..(v1 / pv).ceil() as i64 {
                let lo = (row as f64 * pv).max(v0);
                let hi = ((row + 1) as f64 * pv).min(v1);
                if hi <= lo {
                    continue;
                }
                let off = if *half && row.rem_euclid(2) == 1 {
                    pu / 2.0
                } else {
                    0.0
                };
                for c in ((u0 + off) / pu).ceil() as i64..=((u1 + off) / pu).floor() as i64 {
                    let u = c as f64 * pu - off;
                    out.push((vec2(u, lo), vec2(u, hi)));
                }
            }
        }
        Pattern::Stone {
            size, irr, seed, ..
        } => {
            let s = *size as f64;
            let irr = *irr as f64 / 100.0;
            for cy in (v0 / s).floor() as i32 - 2..=(v1 / s).floor() as i32 + 2 {
                for cx in (u0 / s).floor() as i32 - 2..=(u1 / s).floor() as i32 + 2 {
                    for (a, b) in texgen::stone_outline(cx, cy, s, irr, *seed) {
                        if let Some(l) = clip(a, b, rect) {
                            out.push(l);
                        }
                    }
                }
            }
        }
        Pattern::Plaster { .. } | Pattern::Foreign(_) => {}
    }
    out
}

// --- Datei --------------------------------------------------------------

fn bond_word(b: Bond) -> &'static str {
    match b {
        Bond::Half => "half",
        Bond::Third => "third",
        Bond::Wild => "wild",
        Bond::Block => "block",
        Bond::Cross => "cross",
    }
}

fn palette_word(palette: &Palette) -> String {
    let pal: Vec<String> = palette
        .iter()
        .filter(|(_, a)| *a > 0.0)
        .map(|(c, a)| format!("{}:{a}", crate::szo::hex(*c)))
        .collect();
    pal.join(";")
}

/// Schlüssel eines Musters nach `[pattern] surface=…` bzw. nach dem Namen
/// einer Vorlage (`[patternpreset]`): ab `gen=`.
pub(crate) fn write_keys(l: Line, p: &Pattern) -> Line {
    match p {
        Pattern::Foreign(_) => l,
        Pattern::Masonry {
            len,
            h,
            joint,
            bond,
            joint_rgb,
            palette,
            hpal,
            flame,
            fend,
            relief,
            spread,
            seed,
        } => {
            let mut l = l
                .word("gen", "masonry")
                .num("len", len)
                .num("h", h)
                .num("joint", joint)
                .word("bond", bond_word(*bond))
                .color("jrgb", *joint_rgb)
                .word("pal", &palette_word(palette));
            if let Some(hp) = hpal {
                l = l.word("hpal", &palette_word(hp));
            }
            if *flame > 0.0 {
                l = l.num("flame", flame);
            }
            if *flame > 0.0 || *fend != 64.0 {
                l = l.num("fend", fend);
            }
            if *relief > 0.0 {
                l = l.num("relief", relief);
            }
            l.num("spread", spread).num("seed", seed)
        }
        Pattern::Plaster {
            grain,
            spread,
            seed,
        } => l
            .word("gen", "plaster")
            .num("grain", grain)
            .num("spread", spread)
            .num("seed", seed),
        Pattern::Concrete {
            w,
            h,
            joint,
            anchors,
            cloud,
            pores,
            seed,
        } => l
            .word("gen", "concrete")
            .num("w", w)
            .num("h", h)
            .num("joint", joint)
            .flag("anchors", *anchors)
            .num("cloud", cloud)
            .num("pores", pores)
            .num("seed", seed),
        Pattern::Timber {
            vertical,
            board,
            joint,
            grain,
            c1,
            c2,
            seed,
        } => l
            .word("gen", "timber")
            .word("dir", if *vertical { "v" } else { "h" })
            .num("board", board)
            .num("joint", joint)
            .num("grain", grain)
            .color("c1", *c1)
            .color("c2", *c2)
            .num("seed", seed),
        Pattern::Tiles {
            len,
            wid,
            joint,
            half,
            joint_rgb,
            palette,
            spread,
            seed,
        } => l
            .word("gen", "tiles")
            .num("len", len)
            .num("wid", wid)
            .num("joint", joint)
            .word("bond", if *half { "half" } else { "cross" })
            .color("jrgb", *joint_rgb)
            .word("pal", &palette_word(palette))
            .num("spread", spread)
            .num("seed", seed),
        Pattern::Stone {
            size,
            joint,
            irr,
            joint_rgb,
            palette,
            seed,
        } => l
            .word("gen", "stone")
            .num("size", size)
            .num("joint", joint)
            .num("irr", irr)
            .color("jrgb", *joint_rgb)
            .word("pal", &palette_word(palette))
            .num("seed", seed),
    }
}

/// Zeile `[pattern]` einer Oberfläche; `None` = Abwahl (`gen=none`).
pub(crate) fn write_line(out: &mut String, surface: Guid, p: Option<&Pattern>) {
    let l = Line::new("pattern").guid("surface", Some(surface));
    match p {
        None => l.word("gen", "none").finish(out),
        Some(Pattern::Foreign(raw)) => {
            out.push_str(raw);
            out.push('\n');
        }
        Some(p) => write_keys(l, p).finish(out),
    }
}

/// Palette aus `pal=`/`hpal=`: bis drei `rrggbb:Anteil`.
fn read_palette(pal: &str) -> Result<Palette, String> {
    let mut palette = [([0u8; 3], 0.0f32); 3];
    let parts: Vec<&str> = pal.split(';').collect();
    if parts.len() > 3 {
        return Err(format!("{} Farben, höchstens 3", parts.len()));
    }
    for (slot, part) in palette.iter_mut().zip(&parts) {
        let (c, a) = part
            .split_once(':')
            .ok_or_else(|| format!("Farbe „{part}“ ohne Anteil"))?;
        let c = crate::szo::parse_hex(c).ok_or_else(|| format!("Farbe „{c}“"))?;
        let a: f32 = a
            .parse()
            .ok()
            .filter(|a: &f32| a.is_finite())
            .ok_or_else(|| format!("Anteil „{a}“"))?;
        *slot = (c, a);
    }
    Ok(palette)
}

/// Liest die Schlüssel ab `gen=` (Zeile `[pattern]` ohne `surface=` bzw.
/// `[patternpreset]`): `Ok(None)` = Abwahl, `Err` = falscher Wert (Hinweis,
/// ohne Muster). `raw` ist die Zeile, wie sie in der Datei steht (für
/// unbekanntes `gen=` und unbekannten Verband).
pub(crate) fn read_line(r: &Record, raw: &str) -> Result<Option<Pattern>, String> {
    let bad = |e: crate::szo::LoadError| e.message;
    let num = |key: &str| r.f32(key).map_err(bad);
    let opt = |key: &str, default: f32| match r.opt(key) {
        None => Ok(default),
        Some(_) => r.f32(key).map_err(bad),
    };
    let seed = || r.int::<u32>("seed").map_err(bad);
    let gen = r.get("gen").map_err(bad)?;
    let p = match gen {
        "none" => return Ok(None),
        "masonry" => {
            // Unbekanntes Verbandswort (neuere Fassung): Zeile bleibt
            // bytegleich, wenn der Rest stimmt (BIM-Befund 6a)
            let (bond, known) = match r.get("bond").map_err(bad)? {
                "half" => (Bond::Half, true),
                "third" => (Bond::Third, true),
                "wild" => (Bond::Wild, true),
                "block" => (Bond::Block, true),
                "cross" => (Bond::Cross, true),
                _ => (Bond::Half, false),
            };
            let hpal = match r.opt("hpal") {
                Some(t) => Some(read_palette(t)?),
                None => None,
            };
            let p = Pattern::Masonry {
                len: num("len")?,
                h: num("h")?,
                joint: num("joint")?,
                bond,
                joint_rgb: r.color("jrgb").map_err(bad)?,
                palette: read_palette(r.get("pal").map_err(bad)?)?,
                hpal,
                flame: opt("flame", 0.0)?,
                fend: opt("fend", 64.0)?,
                relief: opt("relief", 0.0)?,
                spread: num("spread")?,
                seed: seed()?,
            };
            if !known {
                validate(&p)?;
                r.skip();
                return Ok(Some(Pattern::Foreign(raw.to_string())));
            }
            p
        }
        "plaster" => Pattern::Plaster {
            grain: num("grain")?,
            spread: num("spread")?,
            seed: seed()?,
        },
        "concrete" => Pattern::Concrete {
            w: num("w")?,
            h: num("h")?,
            joint: num("joint")?,
            anchors: r.flag("anchors").map_err(bad)?,
            cloud: num("cloud")?,
            pores: num("pores")?,
            seed: seed()?,
        },
        "timber" => Pattern::Timber {
            vertical: match r.get("dir").map_err(bad)? {
                "v" => true,
                "h" => false,
                d => return Err(format!("Richtung „{d}“ (v oder h)")),
            },
            board: num("board")?,
            joint: num("joint")?,
            grain: num("grain")?,
            c1: r.color("c1").map_err(bad)?,
            c2: r.color("c2").map_err(bad)?,
            seed: seed()?,
        },
        "tiles" => Pattern::Tiles {
            len: num("len")?,
            wid: num("wid")?,
            joint: num("joint")?,
            half: match r.get("bond").map_err(bad)? {
                "cross" => false,
                "half" => true,
                b => return Err(format!("Verlegung „{b}“ (cross oder half)")),
            },
            joint_rgb: r.color("jrgb").map_err(bad)?,
            palette: read_palette(r.get("pal").map_err(bad)?)?,
            spread: num("spread")?,
            seed: seed()?,
        },
        "stone" => Pattern::Stone {
            size: num("size")?,
            joint: num("joint")?,
            irr: num("irr")?,
            joint_rgb: r.color("jrgb").map_err(bad)?,
            palette: read_palette(r.get("pal").map_err(bad)?)?,
            seed: seed()?,
        },
        _ => {
            r.skip();
            return Ok(Some(Pattern::Foreign(raw.to_string())));
        }
    };
    validate(&p)?;
    Ok(Some(p))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Die Mischfarbe des wilden Verbands nimmt den Kopfanteil der Tabelle
    /// (Startwert 17: 30,4 %, einstellungen/b7-kennwerte.md); ohne Tabelle
    /// 30 %, ohne auf sie zu warten.
    #[test]
    fn mischfarbe_mit_kopfanteil_der_tabelle() {
        let k = head_share(17).expect("mitgeliefert");
        assert!((k - 0.304).abs() < 0.0005, "{k}");
        assert_eq!(head_share(0x0bad_5eed), None, "nie gerechnet");
        let p = factory(FACING).unwrap();
        let fest = |k: f64| {
            let Pattern::Masonry { len, joint, .. } = &p else {
                unreachable!()
            };
            let (len, j) = (*len as f64, *joint as f64);
            let kf = k * ((len + j) / 2.0 - j);
            kf / ((1.0 - k) * len + kf)
        };
        assert!(fest(k) > fest(WILD_HEADS));
    }

    /// Die mitgelieferte Werkstabelle ist die gerechnete (Startwert 17).
    #[test]
    fn werkstabelle_wie_gerechnet() {
        assert_eq!(wild_table(BUILTIN_SEED).cells, BUILTIN_TABLE.to_vec());
    }

    /// Ein fremder Startwert kommt aus dem Hintergrund und gleicht der
    /// sofort gerechneten Tabelle.
    #[test]
    fn tabelle_im_hintergrund() {
        let seed = 9_876_543;
        let g0 = bond_generation();
        let mut t = bond_table_ready(seed);
        let start = std::time::Instant::now();
        while t.is_none() {
            assert!(start.elapsed().as_secs() < 60, "Hintergrund fertig");
            std::thread::yield_now();
            t = bond_table_ready(seed);
        }
        assert!(bond_generation() > g0);
        assert_eq!(t.unwrap().cells, wild_table(seed).cells);
        assert!(
            bond_table_ready(BUILTIN_SEED).is_some(),
            "Werkstabelle sofort"
        );
    }

    /// Werksmuster friesisch-bunt mit Startwert `seed`.
    fn wild(seed: u32) -> Pattern {
        with_seed(&masonry_default(), seed)
    }

    /// Stoßfugen (Viertel-Index) je Schicht im Bereich.
    fn quarter_joints(seed: u32, rows: std::ops::Range<i32>, n: i32) -> Vec<Vec<i64>> {
        let a = 250.0;
        rows.map(|r| {
            let rect = Rect2::new(
                -(n as f64) * a,
                r as f64 * 81.0 + 1.0,
                n as f64 * a,
                r as f64 * 81.0 + 2.0,
            );
            let mut q: Vec<i64> = joint_lines(&wild(seed), rect)
                .into_iter()
                .filter(|(p, q)| p.x == q.x)
                .map(|(p, _)| (p.x / (a / 4.0)).round() as i64)
                .collect();
            q.sort_unstable();
            q
        })
        .collect()
    }

    /// Regeln der Verbandstabelle ringsum über beide Nähte (§8.1).
    #[test]
    fn verbandstabelle_ringsum() {
        let n = WILD_SIZE as i64;
        for seed in [1, 17, 4711] {
            let t = bond_table(seed);
            let j = |r: i64, q: i64| t.cell(r as i32, q as i32) & 3 == 0;
            let (mut heads, mut all) = (0, 0);
            let mut shares = Vec::new();
            let mut keys = Vec::new();
            for r in 0..n {
                let js: Vec<i64> = (0..n).filter(|&q| j(r, q)).collect();
                assert!(js.iter().all(|q| q.rem_euclid(2) == r & 1), "Parität");
                let d: Vec<i64> = (0..js.len())
                    .map(|i| js.get(i + 1).copied().unwrap_or(js[0] + n) - js[i])
                    .collect();
                assert!(d.iter().all(|&x| x == 2 || x == 4), "Stein {d:?}");
                for q in 0..n {
                    let (start, len) = t.stone(r as i32, q as i32);
                    assert!(j(r, start as i64) && q - (start as i64) < len as i64);
                    assert_eq!(
                        len == 2,
                        d.contains(&2) && t.cell(r as i32, q as i32) & 4 != 0
                    );
                }
                let dd = [d.clone(), d.clone()].concat();
                for run in dd.chunk_by(|a, b| a == b) {
                    let max = if run[0] == 2 { 2 } else { 4 };
                    assert!(run.len() <= max, "Folge {run:?} in Schicht {r}");
                }
                let h = d.iter().filter(|&&x| x == 2).count();
                heads += h;
                all += d.len();
                shares.push(h as f64 / d.len() as f64 * 100.0);
                keys.push((0..d.len()).map(|i| [&d[i..], &d[..i]].concat()).min());
            }
            for r in 0..n {
                for q in (0..n).filter(|&q| j(r, q)) {
                    for dq in [1, -1] {
                        let mut k = 1;
                        while k <= n && j(r + k, q + dq * k) {
                            k += 1;
                        }
                        assert!(k as u32 <= WILD_STAIR, "Treppe {k} ab {r}, {q}");
                    }
                    let mut k = 1;
                    while k <= n && j(r + 2 * k, q) {
                        k += 1;
                    }
                    assert!(k as u32 <= WILD_CHAIN, "Kette {k} ab {r}, {q}");
                }
                for d in 1..8 {
                    assert_ne!(
                        keys[r as usize],
                        keys[((r + d) % n) as usize],
                        "Folge doppelt ab {r}"
                    );
                }
            }
            let share = heads as f64 / all as f64 * 100.0;
            assert!((share - 30.0).abs() <= 3.0, "Kopfanteil {share:.1}");
            let mean = shares.iter().sum::<f64>() / n as f64;
            let sd = (shares.iter().map(|a| (a - mean).powi(2)).sum::<f64>() / n as f64).sqrt();
            assert!(sd >= 2.0, "Streuung {sd:.2}");
            assert_eq!(*t, wild_table(seed), "deterministisch");
        }
        assert_ne!(bond_table(17).cells, bond_table(18).cells);
    }

    /// Fugenlinien und Farbformel lesen dieselbe Tabelle.
    #[test]
    fn fugen_aus_der_tabelle() {
        let t = bond_table(17);
        let rows = quarter_joints(17, -3..140, 40);
        for (i, js) in rows.iter().enumerate() {
            let r = i as i32 - 3;
            for &q in js {
                assert_eq!(t.cell(r, q as i32) & 3, 0, "Schicht {r}, Viertel {q}");
            }
            let n = (-160..=160).filter(|&q| t.cell(r, q) & 3 == 0).count();
            assert_eq!(js.len(), n, "Schicht {r}");
        }
    }

    /// Paket 7b: Jeder Schlüssel der Reglergrenzen lässt sich lesen und
    /// setzen, an jeder Werksvorlage.
    #[test]
    fn regler_lesen_und_setzen() {
        for pr in presets() {
            let mut p = pr.pattern.clone();
            for &(key, lo, hi) in limits(gen_word(&p)) {
                assert!(value(&p, key).is_some(), "{} {key}", pr.name);
                let v = if key == "anchors" {
                    1.0
                } else {
                    (lo + hi) / 2.0
                };
                assert!(set_value(&mut p, key, v), "{} {key}", pr.name);
                assert_eq!(value(&p, key), Some(v), "{} {key}", pr.name);
            }
            assert!(!set_value(&mut p, "gibtsnicht", 1.0));
            assert!(validate(&p).is_ok(), "{}", pr.name);
        }
    }
}
