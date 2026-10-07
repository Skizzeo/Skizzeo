//! Prozedurale Muster der Oberflächen (Paket 6): Mauerwerk mit Fugen und
//! Putzkörnung, aus wenigen Zahlen erzeugt, ohne Bild.
//!
//! [`sample`] ist die Formel für Vorschau und Test; der Fragment-Shader
//! (`sk_render::PATTERN_GLSL`) rechnet Zeile für Zeile dasselbe. Der Zufall
//! ist ein reiner Ganzzahl-Hash ([`hash`], BIM-Regel 61): gleiche Datei,
//! gleiches Bild auf jedem Rechner.
//!
//! Koordinaten: `u` waagerecht längs der Fläche, `v` senkrecht ab ±0,00, in
//! mm. Lagerfugen liegen mit ihrer Mitte bei `v = k·(h + Fuge)`.

use crate::guid::Guid;
use crate::szo::{Line, Record};
use sk_math::{vec2, Rect2, Vec2};

/// Verband des Mauerwerks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Bond {
    /// Läufer halbsteinig versetzt.
    Half,
    /// Läufer drittelsteinig versetzt.
    Third,
    /// Wilder Verband aus Läufern und Köpfen (Regel 58, Jörns Vorlage
    /// „friesisch-bunt“), siehe [`WILD_PERIOD`].
    Wild,
}

/// Muster einer Oberfläche.
#[derive(Clone, Debug, PartialEq)]
pub enum Pattern {
    /// Steine mit Fugen. Maße in mm, Anteile der Steinfarben in % (Summe
    /// 100, Farben mit Anteil 0 gelten als nicht gesetzt), Streuung in %
    /// Helligkeit je Stein.
    Masonry {
        len: f32,
        h: f32,
        joint: f32,
        bond: Bond,
        joint_rgb: [u8; 3],
        palette: [([u8; 3], f32); 3],
        spread: f32,
        seed: u32,
    },
    /// Putz: Körnung in mm, Streuung in % Helligkeit, Farbe der Oberfläche.
    Plaster { grain: f32, spread: f32, seed: u32 },
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

/// Prüft die Werte (Regel 57); `Err` nennt den ersten Verstoß.
pub fn validate(p: &Pattern) -> Result<(), String> {
    let within = |v: f32, (lo, hi): (f32, f32), what: &str| {
        if v.is_finite() && v >= lo && v <= hi {
            Ok(())
        } else {
            Err(format!("{what} {v} außerhalb {lo}–{hi}"))
        }
    };
    match p {
        Pattern::Masonry {
            len,
            h,
            joint,
            palette,
            spread,
            ..
        } => {
            within(*len, LEN_MM, "Steinlänge")?;
            within(*h, HEIGHT_MM, "Steinhöhe")?;
            within(*joint, JOINT_MM, "Fuge")?;
            within(*spread, (0.0, SPREAD_MASONRY), "Streuung")?;
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
        Pattern::Plaster { grain, spread, .. } => {
            within(*grain, GRAIN_MM, "Körnung")?;
            within(*spread, (0.0, SPREAD_PLASTER), "Streuung")
        }
        Pattern::Foreign(_) => Ok(()),
    }
}

// --- Werksmuster --------------------------------------------------------

/// Kernfarben der Läufer „Röben Jever friesisch-bunt“ (BIM-Nachtrag 2,
/// 18:25, nach referenz/texturen/auswertung.md): Rot, Braun-grau, Silbergrau.
const FRIES: [([u8; 3], f32); 3] = [
    ([0x87, 0x49, 0x3c], 79.0),
    ([0x67, 0x55, 0x49], 9.0),
    ([0x7b, 0x6d, 0x65], 12.0),
];

/// Name der Werks-Oberfläche des Verblenders.
pub const FACING: &str = "Verblender (Vormauerziegel)";
/// Name der Werks-Oberfläche des Putzes.
pub const PLASTER: &str = "Putz";

/// Werksmuster einer Oberfläche des Startbestands (nach ihrem Namen), aus
/// Jörns Referenztexturen: Klinker NF im wilden Verband „friesisch-bunt“,
/// Reibeputz K2.
pub fn factory(surface: &str) -> Option<Pattern> {
    match surface {
        FACING => Some(Pattern::Masonry {
            len: 240.0,
            h: 71.0,
            joint: 10.0,
            bond: Bond::Wild,
            joint_rgb: [0xd1, 0xcb, 0xc2],
            palette: FRIES,
            spread: 7.0,
            seed: 17,
        }),
        PLASTER => Some(Pattern::Plaster {
            grain: 2.0,
            spread: 4.0,
            seed: 3,
        }),
        _ => None,
    }
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
// Rechnung in Einheiten e = (Länge + Fuge)/2 (halber Stein im Achsmaß); ein
// Läufer ist 2 e, ein Kopf 1 e lang. Die Schicht r hat ihre Stoßfugen auf den
// Viertelsteinen q = e/2 mit der Parität von r: `u = (2·t − r)·q` für eine
// ganze Einheit t. Benachbarte Schichten teilen darum nie eine Stoßfuge, die
// Überbindung ist mindestens ¼ Stein.
//
// Jede Schicht ist in Abschnitte zu 12 e geteilt (5 Läufer, 2 Köpfe wie in
// Jörns Vorlage: Kopfanteil 2/7 ≈ 29 %). Ein Abschnitt hat feste
// Läufermitten bei 0, 2 und 7 e ab seinem Anfang φ(r); dazwischen wählt der
// Zufall je Abschnitt die Lage der zwei Köpfe (L K L oder K L L). Die
// Anfänge wandern je Schicht um eine feste Steigung, φ(r) = m·r + c mit
// m = 2 oder m = −1 (spiegelbildlich, ¾ Stein je Schicht nach rechts oder
// links); nur diese zwei Steigungen erfüllen alle Regeln zugleich (Suche
// über alle φ-Folgen, Bericht 6a): In je 6 aufeinanderfolgenden Schichten
// trifft jede Fugentreppe (je Schicht ¼ Stein nach links oder rechts) auf
// eine Läufermitte, ebenso jede Fuge, die in k, k + 2, k + 4, k + 6 stehen
// bliebe. So läuft keine Treppe über mehr als 5 Schichten, keine Fuge steht
// in mehr als 3 Schichten k, k + 2, k + 4, nie stehen mehr als 4 Läufer
// oder 2 Köpfe nebeneinander (Regel 58, BIM-Nachträge 2 und 3).

/// Länge eines Abschnitts in Einheiten.
pub const WILD_PERIOD: i32 = 12;

/// Salz für die Wahl von Steigung und Versatz.
const WILD_SALT: u32 = 0x63d8_3595;
/// Salz für die Kopflage je Abschnitt.
const BLOCK_SALT: u32 = 0xa511_e9b3;
/// Salz für die Streuung je Stein.
const SPREAD_SALT: u32 = 0x68e3_1da4;

/// Stoßfugen eines Abschnitts in Einheiten ab seinem Anfang (aufsteigend):
/// Läufer 1–3, dann L K L oder K L L in 3–8 und 8–13.
fn wild_joints(h: u32) -> [i32; 7] {
    let b1 = (h >> 31) & 1;
    let b2 = (h >> 30) & 1;
    [
        1,
        3,
        if b1 == 1 { 4 } else { 5 },
        6,
        8,
        if b2 == 1 { 9 } else { 10 },
        11,
    ]
}

/// Anfang φ des Abschnitts 0 in Schicht `row`, in Einheiten.
fn wild_phi(row: i32, seed: u32) -> i32 {
    let s = lowbias32(seed ^ WILD_SALT);
    let slope = if s & 1 == 1 { 2 } else { -1 };
    let shift = ((s >> 1) % WILD_PERIOD as u32) as i32;
    (slope * row + shift).rem_euclid(WILD_PERIOD)
}

/// Stein im wilden Verband an Einheit `t` (gebrochen) der Schicht `row`:
/// Nummer des Steins (Anfang in Einheiten ab φ) und Abstand zur nächsten
/// Stoßfuge in Einheiten.
fn wild_stone(row: i32, t: f64, seed: u32) -> (i32, f64) {
    let x = t - wild_phi(row, seed) as f64;
    let xi = x.floor();
    let fr = x - xi;
    let xi = xi as i32;
    let p = xi.div_euclid(WILD_PERIOD);
    let k = xi.rem_euclid(WILD_PERIOD);
    let j = wild_joints(hash(row, p, seed ^ BLOCK_SALT));
    let mut s = -1;
    let mut e = WILD_PERIOD + 1;
    for &q in &j {
        if q <= k {
            s = q;
        } else if e > WILD_PERIOD {
            e = q;
        }
    }
    let y = k as f64 + fr;
    (WILD_PERIOD * p + s, (y - s as f64).min(e as f64 - y))
}

// --- Stein und Fuge -----------------------------------------------------

/// Lage eines Punkts im Mauerwerk: Reihe, Stein, Fuge ja/nein.
struct Spot {
    row: i32,
    stone: i32,
    joint: bool,
}

#[allow(clippy::too_many_arguments)]
fn locate(len: f64, h: f64, joint: f64, bond: Bond, seed: u32, u: f64, v: f64) -> Spot {
    let course = h + joint;
    let row = ((v + joint / 2.0) / course).floor();
    let dv = v + joint / 2.0 - row * course;
    let row = row as i32;
    let a = len + joint;
    let (stone, du) = match bond {
        Bond::Wild => {
            let t = 2.0 * u / a + row as f64 / 2.0;
            let (s, d) = wild_stone(row, t, seed);
            (s, d * a / 2.0)
        }
        Bond::Half | Bond::Third => {
            let off = match bond {
                Bond::Half => (row & 1) as f64 * a / 2.0,
                _ => row.rem_euclid(3) as f64 * a / 3.0,
            };
            let col = ((u + off) / a).floor();
            let x = u + off - col * a;
            (col as i32, x.min(a - x))
        }
    };
    Spot {
        row,
        stone,
        joint: dv < joint || du < joint / 2.0,
    }
}

/// Farbe eines Steins: Familie nach den Anteilen, Helligkeit gestreut.
fn stone_rgb(
    palette: &[([u8; 3], f32); 3],
    spread: f32,
    seed: u32,
    row: i32,
    stone: i32,
) -> [u8; 3] {
    let h = hash(row, stone, seed);
    // Ganzzahlig wie im Shader: (h >> 8)·100 < Summe der Anteile · 2²⁴
    let x = (h >> 8) as u64 * 100;
    let mut cum = 0u64;
    let mut c = palette[0].0;
    for (rgb, share) in palette {
        if *share <= 0.0 {
            continue;
        }
        cum += *share as u64;
        c = *rgb;
        if x < cum << 24 {
            break;
        }
    }
    if spread <= 0.0 {
        return c;
    }
    let f = 1.0 + spread / 100.0 * (2.0 * unit(lowbias32(h ^ SPREAD_SALT)) - 1.0);
    scale(c, f)
}

fn scale(c: [u8; 3], f: f32) -> [u8; 3] {
    c.map(|x| (x as f32 * f).round().clamp(0.0, 255.0) as u8)
}

/// Helligkeit des Putzes an (u, v): Kornkuppen auf einem hellen Plateau,
/// Schatten darunter (Reibeputz, auswertung.md §2); Korn waagerecht 1,5 : 1.
fn plaster_light(grain: f32, spread: f32, seed: u32, u: f64, v: f64) -> f32 {
    let gx = (u / (grain as f64 * 1.5)) as f32;
    let gy = (v / grain as f64) as f32;
    let (ix, iy) = (gx.floor(), gy.floor());
    let (fx, fy) = (gx - ix, gy - iy);
    let (ix, iy) = (ix as i32, iy as i32);
    let n = |dx: i32, dy: i32| unit(hash(iy + dy, ix + dx, seed));
    let sx = fx * fx * (3.0 - 2.0 * fx);
    let sy = fy * fy * (3.0 - 2.0 * fy);
    let a = n(0, 0) + (n(1, 0) - n(0, 0)) * sx;
    let b = n(0, 1) + (n(1, 1) - n(0, 1)) * sx;
    let k = a + (b - a) * sy;
    let t = ((k - 0.45) / 0.55).clamp(0.0, 1.0);
    let shade = t * t * (3.0 - 2.0 * t);
    1.0 + spread / 100.0 * (0.5 - 2.5 * shade)
}

/// Farbe des Musters an (u, v) in mm; `base` ist die Farbe der Oberfläche
/// (Putz). Ohne Ausblenden in die Ferne (das macht der Shader).
pub fn sample(p: &Pattern, base: [u8; 3], u: f64, v: f64) -> [u8; 3] {
    match p {
        Pattern::Masonry {
            len,
            h,
            joint,
            bond,
            joint_rgb,
            palette,
            spread,
            seed,
        } => {
            let s = locate(*len as f64, *h as f64, *joint as f64, *bond, *seed, u, v);
            if s.joint {
                *joint_rgb
            } else {
                stone_rgb(palette, *spread, *seed, s.row, s.stone)
            }
        }
        Pattern::Plaster {
            grain,
            spread,
            seed,
        } => scale(base, plaster_light(*grain, *spread, *seed, u, v)),
        Pattern::Foreign(_) => base,
    }
}

/// Mischfarbe aus der Ferne: Steinfarben nach Anteil und Fläche, Fuge nach
/// ihrem Flächenanteil; Putz und Fremdes in der Farbe der Oberfläche.
pub fn mix(p: &Pattern, base: [u8; 3]) -> [u8; 3] {
    let Pattern::Masonry {
        len,
        h,
        joint,
        joint_rgb,
        palette,
        ..
    } = p
    else {
        return base;
    };
    let stone = len * h / ((len + joint) * (h + joint));
    let mut c = [0.0f32; 3];
    for (rgb, share) in palette {
        for k in 0..3 {
            c[k] += rgb[k] as f32 * share / 100.0;
        }
    }
    let mut out = [0u8; 3];
    for k in 0..3 {
        out[k] = (c[k] * stone + joint_rgb[k] as f32 * (1.0 - stone))
            .round()
            .clamp(0.0, 255.0) as u8;
    }
    out
}

/// Fugen als Mittellinien in einem Rechteck (mm): Lagerfugen über die ganze
/// Breite, Stoßfugen je Reihe zwischen ihren Lagerfugen; alles auf das
/// Rechteck beschnitten. Für die Ansichtskachel, Test und später PDF/DXF.
pub fn joint_lines(p: &Pattern, rect: Rect2) -> Vec<(Vec2, Vec2)> {
    let Pattern::Masonry {
        len,
        h,
        joint,
        bond,
        seed,
        ..
    } = p
    else {
        return Vec::new();
    };
    let (len, h, joint) = (*len as f64, *h as f64, *joint as f64);
    let (u0, v0, u1, v1) = (rect.min.x, rect.min.y, rect.max.x, rect.max.y);
    let course = h + joint;
    let a = len + joint;
    let mut out = Vec::new();
    let k0 = (v0 / course).ceil() as i64;
    let k1 = (v1 / course).floor() as i64;
    for k in k0..=k1 {
        let v = k as f64 * course;
        out.push((vec2(u0, v), vec2(u1, v)));
    }
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
        match bond {
            Bond::Wild => {
                // Stoßfugen u = (2·t − r)·a/4 für die Fugen-Einheiten t
                let t0 = 2.0 * u0 / a + row as f64 / 2.0;
                let t1 = 2.0 * u1 / a + row as f64 / 2.0;
                let phi = wild_phi(row, *seed);
                let p0 = ((t0 - phi as f64) / WILD_PERIOD as f64).floor() as i32;
                let p1 = ((t1 - phi as f64) / WILD_PERIOD as f64).floor() as i32;
                for pp in p0..=p1 {
                    let base = phi + WILD_PERIOD * pp;
                    for q in wild_joints(hash(row, pp, *seed ^ BLOCK_SALT)) {
                        let t = (base + q) as f64;
                        push((2.0 * t - row as f64) * a / 4.0);
                    }
                }
            }
            Bond::Half | Bond::Third => {
                let off = match bond {
                    Bond::Half => (row & 1) as f64 * a / 2.0,
                    _ => row.rem_euclid(3) as f64 * a / 3.0,
                };
                let c0 = ((u0 + off) / a).ceil() as i64;
                let c1 = ((u1 + off) / a).floor() as i64;
                for c in c0..=c1 {
                    push(c as f64 * a - off);
                }
            }
        }
    }
    out
}

// --- Datei --------------------------------------------------------------

fn bond_word(b: Bond) -> &'static str {
    match b {
        Bond::Half => "half",
        Bond::Third => "third",
        Bond::Wild => "wild",
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
        Some(Pattern::Masonry {
            len,
            h,
            joint,
            bond,
            joint_rgb,
            palette,
            spread,
            seed,
        }) => {
            let pal: Vec<String> = palette
                .iter()
                .filter(|(_, a)| *a > 0.0)
                .map(|(c, a)| format!("{}:{a}", crate::szo::hex(*c)))
                .collect();
            l.word("gen", "masonry")
                .num("len", len)
                .num("h", h)
                .num("joint", joint)
                .word("bond", bond_word(*bond))
                .color("jrgb", *joint_rgb)
                .word("pal", &pal.join(";"))
                .num("spread", spread)
                .num("seed", seed)
                .finish(out)
        }
        Some(Pattern::Plaster {
            grain,
            spread,
            seed,
        }) => l
            .word("gen", "plaster")
            .num("grain", grain)
            .num("spread", spread)
            .num("seed", seed)
            .finish(out),
    }
}

/// Liest eine Zeile `[pattern]` (ohne `surface=`): `Ok(None)` = Abwahl,
/// `Err` = falscher Wert (Hinweis, ohne Muster). `raw` ist die Zeile, wie sie
/// in der Datei steht (für unbekanntes `gen=`).
pub(crate) fn read_line(r: &Record, raw: &str) -> Result<Option<Pattern>, String> {
    let bad = |e: crate::szo::LoadError| e.message;
    let gen = r.get("gen").map_err(bad)?;
    let p = match gen {
        "none" => return Ok(None),
        "masonry" => {
            let bond = match r.get("bond").map_err(bad)? {
                "half" => Bond::Half,
                "third" => Bond::Third,
                "wild" => Bond::Wild,
                b => return Err(format!("Verband „{b}“ unbekannt")),
            };
            let mut palette = [([0u8; 3], 0.0f32); 3];
            let pal = r.get("pal").map_err(bad)?;
            let parts: Vec<&str> = pal.split(';').collect();
            if parts.len() > 3 {
                return Err(format!("{} Steinfarben, höchstens 3", parts.len()));
            }
            for (slot, part) in palette.iter_mut().zip(&parts) {
                let (c, a) = part
                    .split_once(':')
                    .ok_or_else(|| format!("Steinfarbe „{part}“ ohne Anteil"))?;
                let c = crate::szo::parse_hex(c).ok_or_else(|| format!("Farbe „{c}“"))?;
                let a: f32 = a
                    .parse()
                    .ok()
                    .filter(|a: &f32| a.is_finite())
                    .ok_or_else(|| format!("Anteil „{a}“"))?;
                *slot = (c, a);
            }
            Pattern::Masonry {
                len: r.f32("len").map_err(bad)?,
                h: r.f32("h").map_err(bad)?,
                joint: r.f32("joint").map_err(bad)?,
                bond,
                joint_rgb: r.color("jrgb").map_err(bad)?,
                palette,
                spread: r.f32("spread").map_err(bad)?,
                seed: r.int("seed").map_err(bad)?,
            }
        }
        "plaster" => Pattern::Plaster {
            grain: r.f32("grain").map_err(bad)?,
            spread: r.f32("spread").map_err(bad)?,
            seed: r.int("seed").map_err(bad)?,
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

    /// Werksmuster friesisch-bunt mit Startwert `seed`.
    fn wild(seed: u32) -> Pattern {
        match masonry_default() {
            Pattern::Masonry {
                len,
                h,
                joint,
                bond,
                joint_rgb,
                palette,
                spread,
                ..
            } => Pattern::Masonry {
                len,
                h,
                joint,
                bond,
                joint_rgb,
                palette,
                spread,
                seed,
            },
            p => p,
        }
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

    #[test]
    fn wild_treppen_und_folgen() {
        for seed in [0, 1, 17, 4711, 99, 123_456] {
            let rows = quarter_joints(seed, -30..30, 40);
            for (i, r) in rows.iter().enumerate() {
                let row = i as i64 - 30;
                for w in r.windows(2) {
                    let d = w[1] - w[0];
                    assert!(d == 2 || d == 4, "Stein {d} Viertel");
                    assert_eq!(w[0].rem_euclid(2), row.rem_euclid(2), "Parität");
                }
                let lens: Vec<i64> = r.windows(2).map(|w| w[1] - w[0]).collect();
                for run in lens.chunk_by(|a, b| a == b) {
                    let max = if run[0] == 2 { 2 } else { 4 };
                    assert!(run.len() <= max, "Folge {run:?}");
                }
            }
            for d in [1i64, -1] {
                for i in 0..rows.len() {
                    for &x in &rows[i] {
                        let mut n = 1;
                        while i + n < rows.len() && rows[i + n].contains(&(x + d * n as i64)) {
                            n += 1;
                        }
                        // am Rand abgeschnitten nur kürzer
                        assert!(n <= 5, "Treppe {n} ab {i}, {x}");
                    }
                }
            }
            for i in 0..rows.len() {
                for &x in &rows[i] {
                    let mut n = 1;
                    while i + 2 * n < rows.len() && rows[i + 2 * n].contains(&x) {
                        n += 1;
                    }
                    assert!(n <= 3, "Fuge {x} {n}-mal in k, k + 2, … ab {i}");
                }
            }
        }
    }
}
