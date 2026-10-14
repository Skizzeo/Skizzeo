//! Flächen und Integrale über dem Netz (plan-gelaendemodul.md §7.2): das
//! Polygon wird mit jedem Dreieck geschnitten (das Dreieck ist konvex, das
//! Polygon darf es nicht sein); im Dreieck ist das Gelände eine Ebene, also
//! ist ∫ z dA je Teilfläche Fläche mal Höhe im Schwerpunkt, exakt.

use crate::netz::Netz;

/// Fläche (m²) und ∫ z dA (m³) des Netzes über dem Polygon `ring` (x, y in
/// den Koordinaten des Netzes, Umlaufsinn beliebig). Was außerhalb des
/// Netzes liegt, zählt nicht.
pub fn integral(netz: &Netz, ring: &[[f64; 2]]) -> (f64, f64) {
    if ring.len() < 3 {
        return (0.0, 0.0);
    }
    let (mut x0, mut y0, mut x1, mut y1) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
    for p in ring {
        x0 = x0.min(p[0]);
        y0 = y0.min(p[1]);
        x1 = x1.max(p[0]);
        y1 = y1.max(p[1]);
    }
    let (mut flaeche, mut summe) = (0.0, 0.0);
    for t in &netz.dreiecke {
        let p = t.map(|i| netz.punkte[i as usize]);
        if p.iter().all(|q| q[0] < x0)
            || p.iter().all(|q| q[0] > x1)
            || p.iter().all(|q| q[1] < y0)
            || p.iter().all(|q| q[1] > y1)
        {
            continue;
        }
        let teil = schneiden(
            ring,
            [[p[0][0], p[0][1]], [p[1][0], p[1][1]], [p[2][0], p[2][1]]],
        );
        let Some((a, c)) = flaeche_schwerpunkt(&teil) else {
            continue;
        };
        flaeche += a;
        summe += a * ebene(&p, c);
    }
    (flaeche, summe)
}

/// Bezugsfläche `z = a + b·x + c·y` (m) als `[a, b, c]`: eine waagerechte
/// Sohle ist `[z, 0, 0]`, eine geneigte Fläche (G3) hat Gefälle.
pub type Bezug = [f64; 3];

/// Was das Gelände über einem Polygon gegen eine Bezugsfläche hat (G2,
/// plan-gelaendemodul.md §9.1): Abtrag und Auftrag getrennt, nie als Saldo.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Erdmenge {
    /// Fläche des Polygons im Netz (m², waagerecht projiziert); weniger
    /// als das Polygon, wenn es über den Rand des Netzes reicht.
    pub flaeche: f64,
    /// ∫ (Gelände − Bezug)⁺ dA (m³): Gelände über dem Bezug.
    pub ueber: f64,
    /// ∫ (Bezug − Gelände)⁺ dA (m³): Gelände unter dem Bezug.
    pub unter: f64,
    /// Fläche, auf der das Gelände über bzw. unter dem Bezug liegt (m²,
    /// waagerecht projiziert; G3: wo eine Böschung wirklich entsteht).
    pub flaeche_ueber: f64,
    pub flaeche_unter: f64,
    /// Größtes Gefälle des Geländes über dem Polygon (m/m, Dreiecke mit
    /// Fläche darin): DIN 4124 erlaubt senkrechte Wände nur bis 1:10.
    pub steil: f64,
}

/// Höhe der Bezugsfläche an `p`.
fn bezug_an(b: &Bezug, p: [f64; 2]) -> f64 {
    b[0] + b[1] * p[0] + b[2] * p[1]
}

/// [`Erdmenge`] des Netzes über dem Polygon `ring` gegen `bezug`, über alle
/// Dreiecke (ohne Gitter; mit vielen Polygonen über einem großen Netz
/// [`crate::abfrage::Abfrage::erdmenge`]).
pub fn erdmenge(netz: &Netz, ring: &[[f64; 2]], bezug: Bezug) -> Erdmenge {
    let mut e = [Erdmenge::default()];
    for t in &netz.dreiecke {
        erdmenge_dreieck(t.map(|i| netz.punkte[i as usize]), ring, &[bezug], &mut e);
    }
    e[0]
}

/// Den Teil des Polygons im Dreieck `p` je Bezug zu `e` zählen: Im Dreieck
/// ist das Gelände eine Ebene, Gelände minus Bezug also linear. Die
/// Teilfläche wird an seiner Nulllinie geteilt; je Teil ist das Integral
/// Fläche mal Wert im Schwerpunkt, exakt. Geschnitten wird einmal für alle
/// Bezüge.
pub(crate) fn erdmenge_dreieck(
    p: [[f64; 3]; 3],
    ring: &[[f64; 2]],
    bezuege: &[Bezug],
    mengen: &mut [Erdmenge],
) {
    if ring.len() < 3 || !ueberlappt(&p, ring) {
        return;
    }
    let teil = schneiden(
        ring,
        [[p[0][0], p[0][1]], [p[1][0], p[1][1]], [p[2][0], p[2][1]]],
    );
    let Some((a, _)) = flaeche_schwerpunkt(&teil) else {
        return;
    };
    let steil = gefaelle(&p);
    for (bezug, e) in bezuege.iter().zip(mengen.iter_mut()) {
        e.flaeche += a;
        e.steil = e.steil.max(steil);
        let f = |q: [f64; 2]| ebene(&p, q) - bezug_an(bezug, q);
        for (vz, summe, fl) in [
            (1.0, &mut e.ueber, &mut e.flaeche_ueber),
            (-1.0, &mut e.unter, &mut e.flaeche_unter),
        ] {
            let halb = halbieren(&teil, |q| vz * f(q));
            if let Some((a, c)) = flaeche_schwerpunkt(&halb) {
                let h = (vz * f(c)).max(0.0);
                *summe += a * h;
                // wo Gelände und Bezug zusammenfallen, entsteht nichts
                if h > 1e-12 {
                    *fl += a;
                }
            }
        }
    }
}

/// Gefälle der Ebene durch das Dreieck (m/m); senkrecht stehende
/// Dreiecke zählen nicht.
fn gefaelle(p: &[[f64; 3]; 3]) -> f64 {
    let (u, v) = (
        [p[1][0] - p[0][0], p[1][1] - p[0][1], p[1][2] - p[0][2]],
        [p[2][0] - p[0][0], p[2][1] - p[0][1], p[2][2] - p[0][2]],
    );
    let n = [
        u[1] * v[2] - u[2] * v[1],
        u[2] * v[0] - u[0] * v[2],
        u[0] * v[1] - u[1] * v[0],
    ];
    if n[2].abs() < 1e-12 {
        return 0.0;
    }
    n[0].hypot(n[1]) / n[2].abs()
}

/// Liegt das Rechteck um das Dreieck über dem um das Polygon?
fn ueberlappt(p: &[[f64; 3]; 3], ring: &[[f64; 2]]) -> bool {
    let (mut x0, mut y0, mut x1, mut y1) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
    for q in ring {
        x0 = x0.min(q[0]);
        y0 = y0.min(q[1]);
        x1 = x1.max(q[0]);
        y1 = y1.max(q[1]);
    }
    !(p.iter().all(|q| q[0] < x0)
        || p.iter().all(|q| q[0] > x1)
        || p.iter().all(|q| q[1] < y0)
        || p.iter().all(|q| q[1] > y1))
}

/// Der Teil des Polygons `v`, in dem die lineare Funktion `f` nicht
/// negativ ist (Sutherland-Hodgman an einer Halbebene; ein nicht konvexer
/// Teil bekommt Stege ohne Fläche, Fläche und Schwerpunkt stimmen).
pub(crate) fn halbieren(v: &[[f64; 2]], f: impl Fn([f64; 2]) -> f64) -> Vec<[f64; 2]> {
    let w: Vec<f64> = v.iter().map(|&q| f(q)).collect();
    let mut neu = Vec::with_capacity(v.len() + 1);
    for i in 0..v.len() {
        let j = (i + 1) % v.len();
        if w[i] >= 0.0 {
            neu.push(v[i]);
        }
        if (w[i] >= 0.0) != (w[j] >= 0.0) {
            let s = w[i] / (w[i] - w[j]);
            neu.push([
                v[i][0] + s * (v[j][0] - v[i][0]),
                v[i][1] + s * (v[j][1] - v[i][1]),
            ]);
        }
    }
    neu
}

/// Mittlere Höhe des Netzes über dem Polygon (Flächenmittel); `None`, wenn
/// es das Netz nicht berührt.
pub fn mittel(netz: &Netz, ring: &[[f64; 2]]) -> Option<f64> {
    let (a, s) = integral(netz, ring);
    (a > 1e-12).then(|| s / a)
}

/// Höhe der Ebene durch das Dreieck `p` an `c`.
fn ebene(p: &[[f64; 3]; 3], c: [f64; 2]) -> f64 {
    let d = (p[1][0] - p[0][0]) * (p[2][1] - p[0][1]) - (p[1][1] - p[0][1]) * (p[2][0] - p[0][0]);
    let l1 = ((p[1][0] - c[0]) * (p[2][1] - c[1]) - (p[1][1] - c[1]) * (p[2][0] - c[0])) / d;
    let l2 = ((p[2][0] - c[0]) * (p[0][1] - c[1]) - (p[2][1] - c[1]) * (p[0][0] - c[0])) / d;
    l1 * p[0][2] + l2 * p[1][2] + (1.0 - l1 - l2) * p[2][2]
}

/// Polygon mit dem Dreieck `t` (gegen den Uhrzeigersinn) geschnitten
/// (Sutherland-Hodgman, das Dreieck ist das konvexe Fenster).
fn schneiden(ring: &[[f64; 2]], t: [[f64; 2]; 3]) -> Vec<[f64; 2]> {
    let mut v: Vec<[f64; 2]> = ring.to_vec();
    for k in 0..3 {
        let (a, b) = (t[k], t[(k + 1) % 3]);
        let seite = |p: [f64; 2]| (b[0] - a[0]) * (p[1] - a[1]) - (b[1] - a[1]) * (p[0] - a[0]);
        let mut neu = Vec::with_capacity(v.len() + 2);
        for i in 0..v.len() {
            let (p, q) = (v[i], v[(i + 1) % v.len()]);
            let (sp, sq) = (seite(p), seite(q));
            if sp >= 0.0 {
                neu.push(p);
            }
            if (sp >= 0.0) != (sq >= 0.0) {
                let s = sp / (sp - sq);
                neu.push([p[0] + s * (q[0] - p[0]), p[1] + s * (q[1] - p[1])]);
            }
        }
        v = neu;
        if v.len() < 3 {
            return Vec::new();
        }
    }
    v
}

/// Betrag der Fläche und Schwerpunkt eines Polygons.
fn flaeche_schwerpunkt(v: &[[f64; 2]]) -> Option<(f64, [f64; 2])> {
    if v.len() < 3 {
        return None;
    }
    let o = v[0];
    let (mut a2, mut cx, mut cy) = (0.0, 0.0, 0.0);
    for i in 0..v.len() {
        let (p, q) = (v[i], v[(i + 1) % v.len()]);
        let (px, py, qx, qy) = (p[0] - o[0], p[1] - o[1], q[0] - o[0], q[1] - o[1]);
        let c = px * qy - qx * py;
        a2 += c;
        cx += (px + qx) * c;
        cy += (py + qy) * c;
    }
    if a2.abs() < 1e-18 {
        return None;
    }
    Some((
        a2.abs() / 2.0,
        [o[0] + cx / (3.0 * a2), o[1] + cy / (3.0 * a2)],
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::netz::{netz, Rand};

    #[test]
    fn ebene_und_l_form() {
        // z = 2 + 0,1 x + 0,2 y über einem Gitter 0 bis 20
        let mut p = Vec::new();
        for i in 0..=4 {
            for j in 0..=4 {
                let (x, y) = (i as f64 * 5.0, j as f64 * 5.0);
                p.push([x, y, 2.0 + 0.1 * x + 0.2 * y]);
            }
        }
        let n = netz(&p, &[], Rand::Huelle);
        // L-Form, im Uhrzeigersinn: Fläche 64 − 16 = 48
        let l = [
            [2.0, 2.0],
            [2.0, 10.0],
            [6.0, 10.0],
            [6.0, 6.0],
            [10.0, 6.0],
            [10.0, 2.0],
        ];
        let (a, s) = integral(&n, &l);
        assert!((a - 48.0).abs() < 1e-9, "{a}");
        // ∫ über die Ebene = Fläche × Höhe im Schwerpunkt der L-Form
        // (Streifen unten 8 × 4 um (6, 4), Streifen links 4 × 4 um (4, 8))
        let (cx, cy) = (
            (32.0 * 6.0 + 16.0 * 4.0) / 48.0,
            (32.0 * 4.0 + 16.0 * 8.0) / 48.0,
        );
        let soll = 48.0 * (2.0 + 0.1 * cx + 0.2 * cy);
        assert!((s - soll).abs() < 1e-9, "{s} {soll}");
        // halb außerhalb: nur der Teil im Netz zählt
        let (a, _) = integral(&n, &[[-5.0, 0.0], [5.0, 0.0], [5.0, 5.0], [-5.0, 5.0]]);
        assert!((a - 25.0).abs() < 1e-9);
        assert_eq!(
            mittel(&n, &[[30.0, 30.0], [31.0, 30.0], [31.0, 31.0]]),
            None
        );
    }

    /// Gitter 0 bis `n`·`d` mit der Höhe `z(x, y)`.
    fn gitter(n: usize, d: f64, z: impl Fn(f64, f64) -> f64) -> Netz {
        let mut p = Vec::new();
        for i in 0..=n {
            for j in 0..=n {
                let (x, y) = (i as f64 * d, j as f64 * d);
                p.push([x, y, z(x, y)]);
            }
        }
        netz(&p, &[], Rand::Huelle)
    }

    /// Mittelpunktsumme auf einem feinen Raster: ∫(f)⁺ und ∫(−f)⁺ über das
    /// Rechteck, zum Vergleich.
    fn fein(r: [f64; 4], f: impl Fn(f64, f64) -> f64) -> (f64, f64) {
        let k = 800;
        let (dx, dy) = ((r[2] - r[0]) / k as f64, (r[3] - r[1]) / k as f64);
        let (mut a, mut b) = (0.0, 0.0);
        for i in 0..k {
            for j in 0..k {
                let v = f(r[0] + (i as f64 + 0.5) * dx, r[1] + (j as f64 + 0.5) * dy);
                a += v.max(0.0) * dx * dy;
                b += (-v).max(0.0) * dx * dy;
            }
        }
        (a, b)
    }

    /// Eine schräge Ebene gegen eine waagerechte Sohle: Abtrag und Auftrag
    /// getrennt, ihre Differenz exakt das Integral, beide wie die feine
    /// Summe; ganz über der Sohle gibt es keinen Auftrag.
    #[test]
    fn ebene_gegen_sohle() {
        let n = gitter(4, 5.0, |x, y| 2.0 + 0.1 * x + 0.2 * y);
        let r = [[0.0, 0.0], [10.0, 0.0], [10.0, 10.0], [0.0, 10.0]];
        let e = erdmenge(&n, &r, [3.0, 0.0, 0.0]);
        assert!((e.flaeche - 100.0).abs() < 1e-9);
        // ∫ (−1 + 0,1 x + 0,2 y) = 100 · (−1 + 0,5 + 1) = 50
        assert!((e.ueber - e.unter - 50.0).abs() < 1e-9, "{e:?}");
        // Gefälle √(0,1² + 0,2²)
        assert!((e.steil - 0.05f64.sqrt()).abs() < 1e-9, "{e:?}");
        let (a, b) = fein([0.0, 0.0, 10.0, 10.0], |x, y| -1.0 + 0.1 * x + 0.2 * y);
        assert!(
            (e.ueber - a).abs() < 1e-3 && (e.unter - b).abs() < 1e-3,
            "{e:?} {a} {b}"
        );
        // Sonderfall Ebene: Sohle unter allem, dieselbe Zahl wie das Integral
        let e = erdmenge(&n, &r, [1.0, 0.0, 0.0]);
        let (_, s) = integral(&n, &r);
        assert_eq!(e.unter, 0.0);
        assert!((e.ueber - (s - 100.0)).abs() < 1e-9);
    }

    /// Hügel gegen eine geneigte Bezugsfläche, L-förmiges Polygon zum Teil
    /// außerhalb des Netzes: mit und ohne Gitter dasselbe, wie die feine
    /// Summe über den Teil im Netz.
    #[test]
    fn huegel_l_form_und_gitter() {
        let z = |x: f64, y: f64| 10.0 + 2.0 * (x / 7.0).sin() + 1.5 * (y / 5.0).cos();
        let n = gitter(30, 1.0, z);
        let b: Bezug = [9.5, 0.03, -0.02];
        let l = [
            [-4.0, 2.0],
            [-4.0, 25.0],
            [12.0, 25.0],
            [12.0, 12.0],
            [24.0, 12.0],
            [24.0, 2.0],
        ];
        let e = erdmenge(&n, &l, b);
        assert!(
            (e.flaeche - (16.0 * 23.0 + 12.0 * 10.0 - 4.0 * 23.0)).abs() < 1e-6,
            "{e:?}"
        );
        let g = crate::abfrage::Abfrage::neu(&n).erdmenge(&l, b);
        for (x, y) in [
            (e.flaeche, g.flaeche),
            (e.ueber, g.ueber),
            (e.unter, g.unter),
        ] {
            assert!((x - y).abs() < 1e-9, "{e:?} {g:?}");
        }
        // feine Summe über die beiden Rechtecke im Netz; das Netz ist
        // stückweise eben, die Summe über die echte Funktion weicht so wenig ab
        let h = crate::abfrage::Abfrage::neu(&n);
        let f = |x: f64, y: f64| h.hoehe(x, y).unwrap() - (b[0] + b[1] * x + b[2] * y);
        let (a1, b1) = fein([0.0, 2.0, 12.0, 25.0], f);
        let (a2, b2) = fein([12.0, 2.0, 24.0, 12.0], f);
        assert!((e.ueber - a1 - a2).abs() < 1e-2, "{e:?} {}", a1 + a2);
        assert!((e.unter - b1 - b2).abs() < 1e-2, "{e:?} {}", b1 + b2);
        assert!(e.ueber > 1.0 && e.unter > 1.0);
    }

    /// Mit Gitter hängt die Zeit je Baugrube nicht an der Größe des Netzes
    /// (150 × 150 Punkte, wie `netz_der_datei_im_budget`).
    #[test]
    fn baugrube_im_budget() {
        let n = gitter(150, 1.0, |x, y| 20.0 + 0.05 * x + 0.5 * (y / 9.0).sin());
        let a = crate::abfrage::Abfrage::neu(&n);
        let r = [[40.0, 40.0], [53.0, 40.0], [53.0, 50.5], [40.0, 50.5]];
        let t = std::time::Instant::now();
        let mut e = Erdmenge::default();
        for _ in 0..100 {
            e = a.erdmenge(&r, [21.0, 0.0, 0.0]);
        }
        let ms = t.elapsed().as_secs_f64() * 1000.0 / 100.0;
        assert!((e.flaeche - 13.0 * 10.5).abs() < 1e-9);
        let grenze = if cfg!(debug_assertions) { 20.0 } else { 1.0 };
        assert!(ms < grenze, "{ms} ms je Baugrube");
    }
}
