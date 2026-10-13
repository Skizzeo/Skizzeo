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
}
