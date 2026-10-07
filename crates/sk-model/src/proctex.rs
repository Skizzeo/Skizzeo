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
    /// „friesisch-bunt“) aus der Verbandstabelle, siehe [`bond_table`].
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

/// Verbandstabelle zum Startwert; je Startwert einmal gerechnet und gehalten.
pub fn bond_table(seed: u32) -> std::sync::Arc<BondTable> {
    use std::sync::{Arc, Mutex};
    static CACHE: Mutex<Vec<(u32, Arc<BondTable>)>> = Mutex::new(Vec::new());
    let mut c = CACHE.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(i) = c.iter().position(|(s, _)| *s == seed) {
        let t = c.remove(i);
        c.push(t.clone());
        return t.1;
    }
    let t = Arc::new(wild_table(seed));
    if c.len() >= 16 {
        c.remove(0);
    }
    c.push((seed, t.clone()));
    t
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
            // Viertel-Koordinate; Stoßfugen auf ganzen Vierteln
            let x = 4.0 * u / a;
            let (start, n) = bond_table(seed).stone(row, x.floor() as i32);
            let d = (x - start as f64).min((start + n) as f64 - x);
            (start, d * a / 4.0)
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
                let t = bond_table(*seed);
                let q0 = (4.0 * u0 / a).ceil() as i32;
                let q1 = (4.0 * u1 / a).floor() as i32;
                for q in q0..=q1 {
                    if t.cell(row, q) & 3 == 0 {
                        push(q as f64 * a / 4.0);
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
}
