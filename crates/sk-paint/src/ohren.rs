//! Ohrenschnitt für Umrisse mit Löchern, wie sie Glyphen haben (Review W30
//! zu #116): je Umriss etwa so viele Dreiecke wie Ecken statt eines Trapezpaars
//! je Streifen und Kante. Löcher werden über eine Brücke vom Eckpunkt mit dem
//! größten x zum sichtbaren Punkt des Außenrands eingefügt (Eberly,
//! „Triangulation by Ear Clipping“).
//!
//! Gilt nur für Umrisse, die sich weder kreuzen noch berühren und deren
//! Löcher gegenläufig zum Rand laufen; sonst `None`, und
//! [`crate::Path::triangles`] zerlegt in Streifen. Das Ergebnis wird an der
//! Fläche geprüft.

use crate::{pt, Pt};

type P = [f64; 2];

fn kreuz(o: P, a: P, b: P) -> f64 {
    (a[0] - o[0]) * (b[1] - o[1]) - (a[1] - o[1]) * (b[0] - o[0])
}

fn flaeche(r: &[P]) -> f64 {
    let n = r.len();
    0.5 * (0..n)
        .map(|i| {
            let (a, b) = (r[i], r[(i + 1) % n]);
            a[0] * b[1] - b[0] * a[1]
        })
        .sum::<f64>()
}

/// Ring ohne doppelte und ohne gerade durchlaufende Punkte.
fn bereinigt(poly: &[Pt], eps: f64) -> Vec<P> {
    let mut r: Vec<P> = Vec::with_capacity(poly.len());
    for p in poly {
        let q = [f64::from(p.x), f64::from(p.y)];
        if r.last()
            .is_none_or(|l| (l[0] - q[0]).abs() + (l[1] - q[1]).abs() > eps)
        {
            r.push(q);
        }
    }
    while r.len() > 1 {
        let (a, b) = (r[0], r[r.len() - 1]);
        if (a[0] - b[0]).abs() + (a[1] - b[1]).abs() > eps {
            break;
        }
        r.pop();
    }
    let mut i = 0;
    while r.len() >= 3 && i < r.len() {
        let n = r.len();
        let (a, b, c) = (r[(i + n - 1) % n], r[i], r[(i + 1) % n]);
        let gerade = kreuz(a, b, c).abs() <= eps * eps;
        let weiter = (b[0] - a[0]) * (c[0] - b[0]) + (b[1] - a[1]) * (c[1] - b[1]) > 0.0;
        if gerade && weiter {
            r.remove(i);
            i = i.saturating_sub(1);
        } else {
            i += 1;
        }
    }
    r
}

/// Liegt `q` im Ring (gerade/ungerade)?
fn innen(q: P, r: &[P]) -> bool {
    let n = r.len();
    let mut drin = false;
    for i in 0..n {
        let (a, b) = (r[i], r[(i + n - 1) % n]);
        if (a[1] > q[1]) != (b[1] > q[1]) {
            let x = a[0] + (q[1] - a[1]) * (b[0] - a[0]) / (b[1] - a[1]);
            if q[0] < x {
                drin = !drin;
            }
        }
    }
    drin
}

/// Berühren oder kreuzen sich die Strecken `ab` und `cd`?
fn treffen(a: P, b: P, c: P, d: P, eps: f64) -> bool {
    let (d1, d2) = (kreuz(a, b, c), kreuz(a, b, d));
    let (d3, d4) = (kreuz(c, d, a), kreuz(c, d, b));
    let e = eps * eps;
    if ((d1 > e && d2 < -e) || (d1 < -e && d2 > e)) && ((d3 > e && d4 < -e) || (d3 < -e && d4 > e))
    {
        return true;
    }
    let auf = |p: P, q: P, r: P, k: f64| {
        k.abs() <= e
            && r[0] >= p[0].min(q[0]) - eps
            && r[0] <= p[0].max(q[0]) + eps
            && r[1] >= p[1].min(q[1]) - eps
            && r[1] <= p[1].max(q[1]) + eps
    };
    auf(a, b, c, d1) || auf(a, b, d, d2) || auf(c, d, a, d3) || auf(c, d, b, d4)
}

/// Kreuzt oder berührt sich irgendeine Kante mit einer nicht benachbarten?
fn beruehrt(ringe: &[Vec<P>], eps: f64) -> bool {
    let kanten: Vec<(usize, usize, P, P)> = ringe
        .iter()
        .enumerate()
        .flat_map(|(ri, r)| (0..r.len()).map(move |i| (ri, i, r[i], r[(i + 1) % r.len()])))
        .collect();
    for (k, &(ri, i, a, b)) in kanten.iter().enumerate() {
        for &(rj, j, c, d) in &kanten[k + 1..] {
            if ri == rj {
                let n = ringe[ri].len();
                if j == (i + 1) % n || i == (j + 1) % n {
                    continue;
                }
            }
            if treffen(a, b, c, d, eps) {
                return true;
            }
        }
    }
    false
}

/// Fügt das Loch `loch` (im Uhrzeigersinn) in den Rand `rand` (gegen den
/// Uhrzeigersinn) ein: Brücke vom Lochpunkt mit größtem x zum sichtbaren
/// Randpunkt, beide Brückenpunkte doppelt.
fn bruecke(rand: &[P], loch: &[P], eps: f64) -> Option<Vec<P>> {
    let mi = (0..loch.len()).max_by(|&i, &j| loch[i][0].total_cmp(&loch[j][0]))?;
    let m = loch[mi];
    let n = rand.len();
    // Nächster Schnitt des Strahls nach +x mit dem Rand
    let mut best: Option<(f64, usize)> = None;
    for k in 0..n {
        let (a, b) = (rand[k], rand[(k + 1) % n]);
        if (a[1] > m[1]) == (b[1] > m[1]) {
            continue;
        }
        let x = a[0] + (m[1] - a[1]) * (b[0] - a[0]) / (b[1] - a[1]);
        if x >= m[0] - eps && best.is_none_or(|(bx, _)| x < bx) {
            best = Some((x, k));
        }
    }
    let (ix, k) = best?;
    let i = [ix, m[1]];
    let (a, b) = (rand[k], rand[(k + 1) % n]);
    let mut pi = if a[0] >= b[0] { k } else { (k + 1) % n };
    // Liegt ein einspringender Randpunkt im Dreieck (M, I, P), nimm den
    // mit dem kleinsten Winkel zur Strahlrichtung
    let p = rand[pi];
    let im_dreieck = |q: P| {
        let (d1, d2, d3) = (kreuz(m, i, q), kreuz(i, p, q), kreuz(p, m, q));
        let neg = d1 < 0.0 || d2 < 0.0 || d3 < 0.0;
        let pos = d1 > 0.0 || d2 > 0.0 || d3 > 0.0;
        !(neg && pos)
    };
    let mut bester: Option<(f64, f64)> = None;
    for j in 0..n {
        let v = rand[j];
        if j == pi || v[0] < m[0] {
            continue;
        }
        let (vor, nach) = (rand[(j + n - 1) % n], rand[(j + 1) % n]);
        if kreuz(vor, v, nach) > 0.0 || !im_dreieck(v) {
            continue;
        }
        let (dx, dy) = (v[0] - m[0], v[1] - m[1]);
        let w = (dy.abs()).atan2(dx);
        let l = dx * dx + dy * dy;
        if bester.is_none_or(|(bw, bl)| w < bw - 1e-12 || ((w - bw).abs() <= 1e-12 && l < bl)) {
            bester = Some((w, l));
            pi = j;
        }
    }
    let mut out = Vec::with_capacity(n + loch.len() + 2);
    out.extend_from_slice(&rand[..=pi]);
    out.extend(loch[mi..].iter().chain(&loch[..mi]).copied());
    out.push(m);
    out.push(rand[pi]);
    out.extend_from_slice(&rand[pi + 1..]);
    Some(out)
}

/// Ohrenschnitt eines Rings gegen den Uhrzeigersinn (Brückenpunkte dürfen
/// doppelt vorkommen).
fn ohren(p: &[P], eps: f64) -> Option<Vec<[P; 3]>> {
    let gleich = |a: P, b: P| (a[0] - b[0]).abs() + (a[1] - b[1]).abs() <= eps;
    let mut idx: Vec<usize> = (0..p.len()).collect();
    let mut out = Vec::with_capacity(p.len());
    let (mut i, mut ohne) = (0, 0);
    while idx.len() > 3 {
        let n = idx.len();
        i %= n;
        let (a, b, c) = (p[idx[(i + n - 1) % n]], p[idx[i]], p[idx[(i + 1) % n]]);
        let k = kreuz(a, b, c);
        if k.abs() <= eps * eps {
            // Ohne Fläche (gerade oder Spitze): Punkt fällt weg
            idx.remove(i);
            ohne = 0;
            continue;
        }
        let ohr = k > 0.0
            && idx.iter().all(|&j| {
                let q = p[j];
                if gleich(q, a) || gleich(q, b) || gleich(q, c) {
                    return true;
                }
                let (d1, d2, d3) = (kreuz(a, b, q), kreuz(b, c, q), kreuz(c, a, q));
                d1 < 0.0 || d2 < 0.0 || d3 < 0.0
            });
        if ohr {
            out.push([a, b, c]);
            idx.remove(i);
            ohne = 0;
        } else {
            i += 1;
            ohne += 1;
            if ohne > n {
                return None;
            }
        }
    }
    let (a, b, c) = (p[idx[0]], p[idx[1]], p[idx[2]]);
    if kreuz(a, b, c).abs() > eps * eps {
        out.push([a, b, c]);
    }
    Some(out)
}

/// Dreiecke der Polygone `polys` nach der Regel „Umlaufzahl ungleich
/// null“, oder `None`, wenn sie sich nicht als Ränder mit Löchern lesen
/// lassen.
pub(crate) fn ohrenschnitt(polys: &[Vec<Pt>]) -> Option<Vec<[Pt; 3]>> {
    let (mut lo, mut hi) = ([f64::MAX; 2], [f64::MIN; 2]);
    for p in polys.iter().flatten() {
        let q = [f64::from(p.x), f64::from(p.y)];
        for k in 0..2 {
            lo[k] = lo[k].min(q[k]);
            hi[k] = hi[k].max(q[k]);
        }
    }
    let gross = (hi[0] - lo[0]).max(hi[1] - lo[1]);
    if gross.is_nan() || gross <= 0.0 {
        return Some(Vec::new());
    }
    let eps = gross * 1e-6;
    let ringe: Vec<Vec<P>> = polys
        .iter()
        .map(|p| bereinigt(p, eps))
        .filter(|r| r.len() >= 3)
        .collect();
    if beruehrt(&ringe, eps) {
        return None;
    }
    // Erst nach der Prüfung: eine Acht hat die Fläche null
    let ringe: Vec<Vec<P>> = ringe
        .into_iter()
        .filter(|r| flaeche(r).abs() > eps * eps)
        .collect();
    if ringe.is_empty() {
        return Some(Vec::new());
    }
    let n = ringe.len();
    let tiefe: Vec<usize> = (0..n)
        .map(|i| {
            (0..n)
                .filter(|&j| j != i && innen(ringe[i][0], &ringe[j]))
                .count()
        })
        .collect();
    // Löcher laufen gegen ihren Rand, sonst zählt die Umlaufzahl anders
    let s = ringe
        .iter()
        .zip(&tiefe)
        .find(|(_, &t)| t == 0)
        .map(|(r, _)| flaeche(r).signum())?;
    if ringe
        .iter()
        .zip(&tiefe)
        .any(|(r, &t)| (flaeche(r).signum() == s) != (t % 2 == 0))
    {
        return None;
    }
    let mut out: Vec<[P; 3]> = Vec::new();
    let mut soll = 0.0;
    for (i, r) in ringe.iter().enumerate() {
        if tiefe[i] % 2 == 1 {
            soll -= flaeche(r).abs();
            continue;
        }
        soll += flaeche(r).abs();
        let mut rand = r.clone();
        if flaeche(&rand) < 0.0 {
            rand.reverse();
        }
        let mut loecher: Vec<Vec<P>> = (0..n)
            .filter(|&j| tiefe[j] == tiefe[i] + 1 && innen(ringe[j][0], r))
            .map(|j| {
                let mut l = ringe[j].clone();
                if flaeche(&l) > 0.0 {
                    l.reverse();
                }
                l
            })
            .collect();
        let max_x = |l: &Vec<P>| l.iter().map(|p| p[0]).fold(f64::MIN, f64::max);
        loecher.sort_by(|a, b| max_x(b).total_cmp(&max_x(a)));
        for l in &loecher {
            rand = bruecke(&rand, l, eps)?;
        }
        out.extend(ohren(&rand, eps)?);
    }
    let ist: f64 = out
        .iter()
        .map(|[a, b, c]| 0.5 * kreuz(*a, *b, *c).abs())
        .sum();
    if (ist - soll).abs() > 1e-4 * soll.abs().max(eps) {
        return None;
    }
    let f = |q: P| pt(q[0] as f32, q[1] as f32);
    Some(
        out.into_iter()
            .map(|[a, b, c]| [f(a), f(b), f(c)])
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ring(p: &[(f32, f32)]) -> Vec<Pt> {
        p.iter().map(|&(x, y)| pt(x, y)).collect()
    }

    fn summe(t: &[[Pt; 3]]) -> f64 {
        t.iter()
            .map(|[a, b, c]| {
                let k = |p: Pt| [f64::from(p.x), f64::from(p.y)];
                0.5 * kreuz(k(*a), k(*b), k(*c)).abs()
            })
            .sum()
    }

    #[test]
    fn vieleck_hat_n_minus_zwei_dreiecke() {
        // L-Form im Uhrzeigersinn
        let l = ring(&[(0., 0.), (0., 4.), (2., 4.), (2., 2.), (4., 2.), (4., 0.)]);
        let t = ohrenschnitt(&[l]).unwrap();
        assert_eq!(t.len(), 4);
        assert!((summe(&t) - 12.0).abs() < 1e-9);
    }

    #[test]
    fn zwei_loecher_wie_die_acht() {
        let aussen = ring(&[(0., 0.), (0., 10.), (6., 10.), (6., 0.)]);
        let oben = ring(&[(1., 6.), (5., 6.), (5., 9.), (1., 9.)]);
        let unten = ring(&[(1., 1.), (5., 1.), (5., 4.), (1., 4.)]);
        let t = ohrenschnitt(&[aussen, oben, unten]).unwrap();
        assert!((summe(&t) - (60.0 - 12.0 - 12.0)).abs() < 1e-9);
        assert!(t.len() <= 4 + 4 + 4 + 2 * 2);
    }

    #[test]
    fn insel_im_loch_ist_gefuellt() {
        let aussen = ring(&[(0., 0.), (0., 10.), (10., 10.), (10., 0.)]);
        let loch = ring(&[(2., 2.), (8., 2.), (8., 8.), (2., 8.)]);
        let insel = ring(&[(4., 4.), (4., 6.), (6., 6.), (6., 4.)]);
        let t = ohrenschnitt(&[aussen, loch, insel]).unwrap();
        assert!((summe(&t) - (100.0 - 36.0 + 4.0)).abs() < 1e-9);
    }

    #[test]
    fn kreuzung_und_gleichlaeufiges_loch_gehen_an_die_streifen() {
        let acht = ring(&[(0., 0.), (2., 2.), (2., 0.), (0., 2.)]);
        assert!(ohrenschnitt(&[acht]).is_none());
        let aussen = ring(&[(0., 0.), (0., 10.), (10., 10.), (10., 0.)]);
        let gleich = ring(&[(2., 2.), (2., 8.), (8., 8.), (8., 2.)]);
        assert!(ohrenschnitt(&[aussen, gleich]).is_none());
    }
}
