//! Formeln der Musterarten aus Paket 7 (Sichtbeton, Holzschalung, Platten,
//! Naturstein) und die Feinheiten nach Jörns Referenztexturen (Flammung,
//! Relief, Reibeputz, Lunker; paket-7 §8). Rechenweg wie
//! `einstellungen/muster.py` und Zeile für Zeile wie `PATTERN_GLSL`: nur der
//! Ganzzahl-Hash aus Regel 61, `floor`, Mischung und Glättung; kein `sin`.
//!
//! Koordinaten in mm, Farben 0–255 als Gleitkomma (gerundet in
//! [`crate::proctex::sample`]).

use crate::proctex::{hash, lowbias32};
use sk_math::{vec2, Vec2};

pub(crate) type Rgb = [f64; 3];

/// 16 Bit des Hashs ab `shift` als Zahl 0..1.
pub(crate) fn h01(h: u32, shift: u32) -> f64 {
    ((h >> shift) & 0xffff) as f64 / 65535.0
}

/// Näherungsweise normalverteilt (σ 1) aus drei 10-Bit-Teilen.
pub(crate) fn gauss3(h: u32) -> f64 {
    let a = ((h & 1023) + ((h >> 10) & 1023) + ((h >> 20) & 1023)) as f64 / 1023.0;
    (a - 1.5) * 2.0
}

/// Glatter Übergang 0..1.
pub(crate) fn smooth(x: f64) -> f64 {
    let x = x.clamp(0.0, 1.0);
    x * x * (3.0 - 2.0 * x)
}

/// Wertrauschen −1..1 auf einem Gitter `cu` × `cv` mm.
pub(crate) fn vnoise(u: f64, v: f64, cu: f64, cv: f64, seed: u32) -> f64 {
    let (x, y) = (u / cu, v / cv);
    let (gx, gy) = (x.floor(), y.floor());
    let (fx, fy) = (x - gx, y - gy);
    let (ix, iy) = (gx as i32, gy as i32);
    let hv = |a: i32, b: i32| h01(hash(a, b, seed), 0) * 2.0 - 1.0;
    let sx = fx * fx * (3.0 - 2.0 * fx);
    let sy = fy * fy * (3.0 - 2.0 * fy);
    let (a, b) = (hv(ix, iy), hv(ix + 1, iy));
    let (c, d) = (hv(ix, iy + 1), hv(ix + 1, iy + 1));
    (a * (1.0 - sx) + b * sx) * (1.0 - sy) + (c * (1.0 - sx) + d * sx) * sy
}

/// Je Zelle ein Wert −1..1 (ungeglättet: Korn, Sprenkel).
pub(crate) fn cellnoise(u: f64, v: f64, cell: f64, seed: u32) -> f64 {
    let (a, b) = ((u / cell).floor() as i32, (v / cell).floor() as i32);
    h01(hash(a, b, seed), 0) * 2.0 - 1.0
}

pub(crate) fn rgb(c: [u8; 3]) -> Rgb {
    c.map(|x| x as f64)
}

const SQRT3: f64 = 1.732_050_807_568_877_2;

/// Farbe nach Anteilen aus dem Hash, ganzzahlig wie im Shader: Index und
/// Farbe. Farben mit Anteil 0 gelten als nicht gesetzt.
pub(crate) fn pick(palette: &[([u8; 3], f32); 3], h: u32) -> (usize, [u8; 3]) {
    let x = (h >> 8) as u64 * 100;
    let mut cum = 0u64;
    let mut out = (0, palette[0].0);
    for (i, (c, share)) in palette.iter().enumerate() {
        if *share <= 0.0 {
            continue;
        }
        cum += *share as u64;
        out = (i, *c);
        if x < cum << 24 {
            break;
        }
    }
    out
}

// --- Mauerwerk: Flammung und Relief (Regeln 68, 70) ------------------------

/// Lage eines Punkts in seinem Stein (mm): ab linker bzw. unterer
/// Steinkante, Steinlänge ohne Fuge, Steinhöhe.
pub(crate) struct InStone {
    pub uin: f64,
    pub slen: f64,
    pub vin: f64,
    pub h: f64,
}

/// Geflammter roter Läufer (Regel 68): Anteil der Endfarbe 0..1 an diesem
/// Punkt und ob die Enden silbergrau (dritte Farbe) statt braun-grau sind.
/// `None`, wenn der Stein nicht geflammt ist.
#[allow(clippy::too_many_arguments)]
pub(crate) fn flame_at(
    row: i32,
    stone: i32,
    seed: u32,
    flame: f32,
    fend: f32,
    s: &InStone,
    u: f64,
    v: f64,
) -> Option<(f64, bool)> {
    let h2 = hash(row, stone, seed.wrapping_add(5));
    if h01(h2, 0) >= flame as f64 / 100.0 {
        return None;
    }
    let cen = 0.5 + 0.05 * gauss3(lowbias32(h2.wrapping_add(1)));
    let wid = (0.53 + 0.23 * gauss3(lowbias32(h2.wrapping_add(2)))).clamp(0.15, 0.85);
    let silver = h01(h2, 16) >= fend as f64 / 100.0;
    // Übergang 10–90 % über 40 mm, Grenze ±10 mm wolkig
    const D: f64 = 66.0;
    let xl = (cen - wid / 2.0) * s.slen + 10.0 * vnoise(u, v, 40.0, 30.0, seed.wrapping_add(9));
    let xr = (cen + wid / 2.0) * s.slen + 10.0 * vnoise(u, v, 40.0, 30.0, seed.wrapping_add(10));
    let red = smooth((s.uin - xl) / D + 0.5) * smooth((xr - s.uin) / D + 0.5);
    Some((1.0 - red, silver))
}

/// Rissrillen (Regel 70): (dunkel 0..1, Grat 0..1) bei Relief `rl` 0..1.
fn grooves(u: f64, v: f64, s: &InStone, seed: u32, rl: f64) -> (f64, f64) {
    let inner = s.vin > 1.5 && s.vin < s.h - 1.5 && s.uin > 1.5 && s.uin < s.slen - 1.5;
    if !inner {
        return (0.0, 0.0);
    }
    let n = |w: f64| {
        vnoise(u, w, 34.0, 6.0, seed.wrapping_add(43))
            + 0.35 * vnoise(u, w, 11.0, 3.0, seed.wrapping_add(44))
    };
    // Abstand zur Nulllinie in mm: |n| / |∂n/∂v|
    let dist = |w: f64| n(w).abs() / (n(w + 0.5) - n(w - 0.5)).abs().max(0.03);
    let seg = vnoise(u, v, 30.0, 14.0, seed.wrapping_add(45))
        + 0.4 * vnoise(u, v, 9.0, 6.0, seed.wrapping_add(46));
    let on = ((seg - 0.12) / 0.15).clamp(0.0, 1.0) * rl;
    let dark = (1.0 - (dist(v) - 0.7) / 0.7).clamp(0.0, 1.0) * on;
    let rim = (1.0 - (dist(v - 2.2) - 1.0) / 0.7).clamp(0.0, 1.0) * on;
    (dark, (rim - dark).clamp(0.0, 1.0))
}

/// Feinkorn, Rillen und Sinterpunkte auf der Steinfarbe `c` (Relief
/// `relief` in %); `silver` = Familie Silbergrau (Korn × 1,25).
pub(crate) fn relief_stone(
    c: Rgb,
    silver: bool,
    seed: u32,
    relief: f32,
    s: &InStone,
    u: f64,
    v: f64,
) -> Rgb {
    let rl = relief as f64 / 100.0;
    let k = if silver { 1.25 } else { 1.0 };
    let fk = k * 14.0 * rl * SQRT3 * cellnoise(u, v, 1.5, seed.wrapping_add(11));
    let (dk, rm) = grooves(u, v, s, seed, rl);
    let sp = h01(
        hash(
            (u / 1.5).floor() as i32,
            (v / 1.5).floor() as i32,
            seed.wrapping_add(12),
        ),
        0,
    ) < 0.07;
    let lift = 25.0 * rl * rm.max(if sp { 1.0 - dk } else { 0.0 });
    c.map(|x| (x + fk) * (1.0 - 0.75 * rl * dk) + lift)
}

/// Fugenfarbe mit Korn bei Relief.
pub(crate) fn relief_joint(jrgb: [u8; 3], seed: u32, u: f64, v: f64) -> Rgb {
    let g = 6.0 * SQRT3 * cellnoise(u, v, 1.5, seed.wrapping_add(13));
    rgb(jrgb).map(|x| x + g)
}

// --- Putz (Reibeputz, paket-7 §8.5) ----------------------------------------

/// Reibeputz: Kornkuppen als helles Plateau (Grundfarbe + 6), darunter
/// Schatten mit Licht von oben; Korn waagerecht 1,5 : 1.
pub(crate) fn plaster(base: [u8; 3], grain: f32, spread: f32, seed: u32, u: f64, v: f64) -> Rgb {
    let lx = grain as f64 * 1.4;
    let ly = lx / 1.5;
    let hgt = |a: f64, b: f64| {
        vnoise(a, b, lx, ly, seed)
            + 0.6 * vnoise(a, b, lx / 2.0, ly / 2.0, seed.wrapping_add(1))
            + 0.25 * vnoise(a, b, lx * 2.0, ly * 2.0, seed.wrapping_add(2))
    };
    let d = ly * 0.3;
    let slope = (hgt(u, v + d) - hgt(u, v - d)) / (2.0 * d) * ly;
    let sh = (0.75 * slope - 0.35 * hgt(u, v) - 0.12).max(0.0);
    let k = spread as f64 / 4.0 * 28.0;
    rgb(base).map(|x| {
        let top = x + 6.0;
        (top - k * sh).max(top - 48.0)
    })
}

// --- Sichtbeton (Regel 69) -------------------------------------------------

#[allow(clippy::too_many_arguments)]
pub(crate) fn concrete(
    base: [u8; 3],
    w: f32,
    h: f32,
    joint: f32,
    anchors: bool,
    cloud: f32,
    pores: f32,
    seed: u32,
    u: f64,
    v: f64,
) -> Rgb {
    let s = seed;
    let sd = |k: u32| s.wrapping_add(k);
    let cl = cloud as f64 / 2.0;
    let big = (1.1 * vnoise(u, v, 32.0, 32.0, s)
        + 0.8 * vnoise(u, v, 64.0, 64.0, sd(1))
        + 0.7 * vnoise(u, v, 160.0, 160.0, sd(2)))
        * cl
        * 1.6;
    // ab 8 mm senkrecht 2 : 1 gestreckt (Schalungsspuren)
    let mid = (2.9 * vnoise(u, v, 4.0, 4.0, sd(3))
        + 2.3 * vnoise(u, v, 8.0, 16.0, sd(4))
        + 1.6 * vnoise(u, v, 16.0, 32.0, sd(5)))
        * 1.6;
    let mut dl = big + mid + 6.0 * SQRT3 * cellnoise(u, v, 1.0, sd(6));
    let mut kl = 0.0;
    if pores > 0.0 {
        let f = pores as f64 / 0.5;
        // Lunker: Zelle 10 mm, Ø 1,5–4,5 mm (Median 2,3), Kern × 0,35
        let (gx, gy) = ((u / 10.0).floor(), (v / 10.0).floor());
        let hh = hash(gx as i32, gy as i32, sd(23));
        if h01(hh, 0) < 0.117 * f {
            let cx = (gx + 0.3 + 0.4 * h01(hh, 8)) * 10.0;
            let cy = (gy + 0.3 + 0.4 * h01(hh, 16)) * 10.0;
            let dia =
                (2.3 * (0.3 * gauss3(hash(gx as i32, gy as i32, sd(29)))).exp()).clamp(1.5, 4.5);
            let dd = ((u - cx).powi(2) + (v - cy).powi(2)).sqrt();
            kl = (dia / 2.0 - dd + 0.5).clamp(0.0, 1.0);
        }
        // Mikroporen (−35 L) und Sandpunkte (+25 L), Zelle 5 mm
        let (mx, my) = ((u / 5.0).floor(), (v / 5.0).floor());
        let hm = hash(mx as i32, my as i32, sd(31));
        let pcx = (mx + 0.2 + 0.6 * h01(hm, 8)) * 5.0;
        let pcy = (my + 0.2 + 0.6 * h01(hm, 16)) * 5.0;
        let pr = 0.5 + 0.3 * h01(lowbias32(hm.wrapping_add(3)), 0);
        if h01(hm, 0) < 0.18 * f && ((u - pcx).powi(2) + (v - pcy).powi(2)).sqrt() < pr {
            dl -= 35.0;
        }
        let hs = hash(mx as i32, my as i32, sd(37));
        let scx = (mx + 0.1 + 0.8 * h01(hs, 8)) * 5.0;
        let scy = (my + 0.1 + 0.8 * h01(hs, 16)) * 5.0;
        if h01(hs, 0) < 0.1425 * f && (u - scx).abs() < 0.5 && (v - scy).abs() < 0.5 {
            dl += 25.0;
        }
    }
    let mut f = 1.0 - 0.65 * kl;
    let (w, h, j) = (w as f64, h as f64, joint as f64);
    if j > 0.0 {
        let du = (u - (u / w).round() * w).abs();
        let dv = (v - (v / h).round() * h).abs();
        if du < j / 2.0 || dv < j / 2.0 {
            f *= 1.0 - 0.18;
        }
    }
    if anchors {
        let (pu, pv) = (w / 2.0, h);
        let ax = u - w / 4.0;
        let ay = v - h / 2.0;
        let ax = ax - (ax / pu).round() * pu;
        let ay = ay - (ay / pv).round() * pv;
        if ax * ax + ay * ay < 144.0 {
            f *= 0.7;
        }
    }
    rgb(base).map(|x| (x + dl) * f)
}

// --- Holzschalung ----------------------------------------------------------

/// Dreieckswelle −1..1 mit Periode 1, sinusähnlich geglättet.
fn wave(x: f64) -> f64 {
    let t = 1.0 - 4.0 * (x - x.floor() - 0.5).abs();
    t * (1.5 - 0.5 * t * t)
}

/// Lage in der Brettreihe: (Brett k, Abstand ab Fugenbeginn) mit Fugen
/// mittig auf k·(Brett + Fuge).
pub(crate) fn board_at(a: f64, board: f64, joint: f64) -> (i32, f64) {
    let p = board + joint;
    let x = a + joint / 2.0;
    let k = (x / p).floor();
    (k as i32, x - k * p)
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn timber(
    vertical: bool,
    board: f32,
    joint: f32,
    grain: f32,
    c1: [u8; 3],
    c2: [u8; 3],
    seed: u32,
    u: f64,
    v: f64,
) -> Rgb {
    let (a, b) = if vertical { (u, v) } else { (v, u) };
    let j = joint as f64;
    let (k, ai) = board_at(a, board as f64, j);
    let hh = hash(k, 3, seed);
    let c = if hh & 1 == 1 { c1 } else { c2 };
    let br = 1.0 + 0.08 * (((hh >> 8) & 255) as f64 / 255.0 * 2.0 - 1.0);
    let t = ai - j;
    let warp = vnoise(t, b, 90.0, 90.0, seed.wrapping_add(k.rem_euclid(97) as u32));
    let g = grain as f64 / 100.0 * wave((t * 0.22 + warp * 5.0) / std::f64::consts::TAU);
    let f = if ai < j { 1.0 - 0.55 } else { 1.0 };
    rgb(c).map(|x| x * br * (1.0 + g) * f)
}

// --- Platten ---------------------------------------------------------------

/// Platte an (u, v): (Reihe, Spalte, Fuge ja/nein); Fugen mittig auf
/// k·(Länge + Fuge) bzw. k·(Breite + Fuge), bei Halbversatz jede zweite Reihe
/// um eine halbe Platte verschoben.
pub(crate) fn tile_at(
    len: f64,
    wid: f64,
    joint: f64,
    half: bool,
    u: f64,
    v: f64,
) -> (i32, i32, bool) {
    let (pu, pv) = (len + joint, wid + joint);
    let y = v + joint / 2.0;
    let row = (y / pv).floor();
    let dv = y - row * pv;
    let off = if half && (row as i64).rem_euclid(2) == 1 {
        pu / 2.0
    } else {
        0.0
    };
    let x = u + off + joint / 2.0;
    let col = (x / pu).floor();
    let du = x - col * pu;
    (row as i32, col as i32, du < joint || dv < joint)
}

// --- Naturstein (Voronoi) --------------------------------------------------

/// Zellpunkt der Gitterzelle (cx, cy) bei Größe `s` und Unregelmäßigkeit
/// `irr` (0..1).
pub(crate) fn stone_point(cx: i32, cy: i32, s: f64, irr: f64, seed: u32) -> Vec2 {
    let hh = hash(cx, cy, seed);
    vec2(
        (cx as f64 + 0.5 + (h01(hh, 0) - 0.5) * irr) * s,
        (cy as f64 + 0.5 + (h01(hh, 16) - 0.5) * irr) * s,
    )
}

/// Nächster Zellpunkt (Gitterzelle) und Abstand zur Zellgrenze in mm, aus
/// einer Suche über 3 × 3 Zellen: die Grenze ist die nächste
/// Mittelsenkrechte zu den übrigen Punkten derselben Suche (Review 3t,
/// statt einer zweiten Schleife über 5 × 5 Nachbarn; gleich im Shader).
pub(crate) fn stone_cell(s: f64, irr: f64, seed: u32, u: f64, v: f64) -> ((i32, i32), f64) {
    let (gx, gy) = ((u / s).floor() as i32, (v / s).floor() as i32);
    let x = vec2(u, v);
    let mut pts = [x; 9];
    let mut best = (gx, gy);
    let (mut a, mut bd) = (0, f64::MAX);
    for (i, p) in pts.iter_mut().enumerate() {
        let (dx, dy) = (i as i32 % 3 - 1, i as i32 / 3 - 1);
        *p = stone_point(gx + dx, gy + dy, s, irr, seed);
        let d = (*p - x).length_squared();
        if d < bd {
            (a, bd) = (i, d);
            best = (gx + dx, gy + dy);
        }
    }
    let pa = pts[a];
    let mut edge = f64::MAX;
    for (i, p) in pts.iter().enumerate() {
        let n = *p - pa;
        let l = n.length();
        if i == a || l < 1e-9 {
            continue;
        }
        edge = edge.min(((pa + *p) * 0.5 - x).dot(n) / l);
    }
    (best, edge)
}

/// Umriss der Zelle (cx, cy) als Strecken, die auf Zellgrenzen liegen.
pub(crate) fn stone_outline(cx: i32, cy: i32, s: f64, irr: f64, seed: u32) -> Vec<(Vec2, Vec2)> {
    let a = stone_point(cx, cy, s, irr, seed);
    let r = 3.0 * s;
    // Ecken mit der Kennung der Kante, die an ihnen beginnt (−1 = Rahmen)
    let mut poly: Vec<(Vec2, i32)> = vec![
        (a + vec2(-r, -r), -1),
        (a + vec2(r, -r), -1),
        (a + vec2(r, r), -1),
        (a + vec2(-r, r), -1),
    ];
    let mut tag = 0;
    for dy in -2..=2 {
        for dx in -2..=2 {
            if dx == 0 && dy == 0 {
                continue;
            }
            tag += 1;
            let p = stone_point(cx + dx, cy + dy, s, irr, seed);
            let n = p - a;
            if n.length() < 1e-9 {
                continue;
            }
            let m = (a + p) * 0.5;
            // innen: (x − m)·n ≤ 0
            let side = |q: Vec2| (q - m).dot(n);
            let mut out: Vec<(Vec2, i32)> = Vec::with_capacity(poly.len() + 1);
            for i in 0..poly.len() {
                let (q0, t0) = poly[i];
                let (q1, _) = poly[(i + 1) % poly.len()];
                let (s0, s1) = (side(q0), side(q1));
                if s0 <= 0.0 {
                    out.push((q0, t0));
                }
                if (s0 <= 0.0) != (s1 <= 0.0) {
                    let t = s0 / (s0 - s1);
                    let q = q0 + (q1 - q0) * t;
                    // die neue Kante beginnt dort, wo es nach innen geht
                    out.push((q, if s0 <= 0.0 { tag } else { t0 }));
                }
            }
            poly = out;
            if poly.len() < 3 {
                return Vec::new();
            }
        }
    }
    let mut edges = Vec::new();
    for i in 0..poly.len() {
        let (q0, t0) = poly[i];
        let (q1, _) = poly[(i + 1) % poly.len()];
        if t0 >= 0 && (q1 - q0).length() > 1e-6 {
            edges.push((q0, q1));
        }
    }
    edges
}
