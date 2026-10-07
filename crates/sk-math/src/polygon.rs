//! Ebene Polygone im Grundriss (z wird ignoriert): Fläche, Umfang,
//! Einfachheit, Versatz nach innen mit sauberen Ecken, Zerlegung in Dreiecke
//! und Schnitt mit einer senkrechten Ebene.
//!
//! Polygone sind Punktfolgen ohne Schlusspunkt. „Gegen den Uhrzeigersinn“
//! heißt von oben gesehen (Z nach oben), dann liegt das Innere links.

use crate::{vec3, Vec3};

/// Abstand, unter dem zwei Punkte als gleich gelten (mm).
pub const EPS: f64 = 1e-6;

fn cross2(a: Vec3, b: Vec3) -> f64 {
    a.x * b.y - a.y * b.x
}

fn flat(p: Vec3) -> Vec3 {
    vec3(p.x, p.y, 0.0)
}

/// Linke Normale einer Richtung (zeigt bei gegen den Uhrzeigersinn ins Innere).
fn left_of(d: Vec3) -> Vec3 {
    vec3(-d.y, d.x, 0.0)
}

/// Vorzeichenbehaftete Fläche: positiv gegen den Uhrzeigersinn.
pub fn signed_area(pts: &[Vec3]) -> f64 {
    let n = pts.len();
    (0..n)
        .map(|i| cross2(pts[i], pts[(i + 1) % n]))
        .sum::<f64>()
        * 0.5
}

/// Fläche (immer ≥ 0).
pub fn area(pts: &[Vec3]) -> f64 {
    signed_area(pts).abs()
}

/// Schwerpunkt der Fläche (z = 0); `None` ohne Fläche.
pub fn centroid(pts: &[Vec3]) -> Option<Vec3> {
    let a = signed_area(pts);
    if a.abs() < 1e-9 {
        return None;
    }
    let n = pts.len();
    let (mut x, mut y) = (0.0, 0.0);
    for i in 0..n {
        let (p, q) = (pts[i], pts[(i + 1) % n]);
        let c = cross2(p, q);
        x += (p.x + q.x) * c;
        y += (p.y + q.y) * c;
    }
    Some(vec3(x / (6.0 * a), y / (6.0 * a), 0.0))
}

/// Umfang des geschlossenen Polygons.
pub fn perimeter(pts: &[Vec3]) -> f64 {
    let n = pts.len();
    (0..n)
        .map(|i| (flat(pts[(i + 1) % n]) - flat(pts[i])).length())
        .sum()
}

/// Dieselben Punkte gegen den Uhrzeigersinn.
pub fn to_ccw(pts: &[Vec3]) -> Vec<Vec3> {
    let mut v: Vec<Vec3> = pts.iter().map(|p| flat(*p)).collect();
    if signed_area(&v) < 0.0 {
        v.reverse();
    }
    v
}

/// Ohne doppelte Nachbarn und ohne Punkte mitten auf einer geraden Kante.
pub fn simplified(pts: &[Vec3]) -> Vec<Vec3> {
    let mut v: Vec<Vec3> = Vec::with_capacity(pts.len());
    for p in pts {
        let p = flat(*p);
        if v.last().is_none_or(|q: &Vec3| (p - *q).length() > EPS) {
            v.push(p);
        }
    }
    while v.len() > 1 && (v[0] - v[v.len() - 1]).length() <= EPS {
        v.pop();
    }
    let mut changed = true;
    while changed && v.len() > 3 {
        changed = false;
        let n = v.len();
        for i in 0..n {
            let (a, b, c) = (v[(i + n - 1) % n], v[i], v[(i + 1) % n]);
            let (u, w) = ((b - a).normalized(), (c - b).normalized());
            if cross2(u, w).abs() < 1e-9 && u.dot(w) > 0.0 {
                v.remove(i);
                changed = true;
                break;
            }
        }
    }
    v
}

/// Schneiden sich zwei Strecken (Berühren an gemeinsamen Endpunkten zählt nicht)?
fn segments_cross(a: Vec3, b: Vec3, c: Vec3, d: Vec3) -> bool {
    let r = b - a;
    let s = d - c;
    let den = cross2(r, s);
    let ac = c - a;
    if den.abs() < 1e-12 {
        // parallel: nur Überlappung auf derselben Geraden zählt
        if cross2(ac, r).abs() > 1e-6 * r.length().max(1.0) {
            return false;
        }
        let rr = r.dot(r).max(1e-12);
        let (t0, t1) = (ac.dot(r) / rr, (d - a).dot(r) / rr);
        let (lo, hi) = (t0.min(t1), t0.max(t1));
        return hi > 1e-9 && lo < 1.0 - 1e-9;
    }
    let t = cross2(ac, s) / den;
    let u = cross2(ac, r) / den;
    let e = 1e-9;
    t > e && t < 1.0 - e && u > e && u < 1.0 - e
}

/// Ist das Polygon einfach (keine Selbstüberschneidung, mindestens drei
/// Punkte, Fläche > 0)?
///
/// Geprüft werden nur Kantenpaare, deren x-Bereiche sich überlappen (Kanten
/// nach linkem Ende sortiert, dann nach rechts abgesucht). Paare ohne
/// Überlappung können sich weder schneiden noch berühren. Bei Grundrissen
/// liegt der Aufwand so nahe n·log n statt n²; das Gründungsnetz wird bei jeder
/// Mausbewegung neu berechnet und prüft dabei mehrfach.
pub fn is_simple(pts: &[Vec3]) -> bool {
    let n = pts.len();
    if n < 3 || area(pts) < EPS {
        return false;
    }
    let edge = |i: usize| (flat(pts[i]), flat(pts[(i + 1) % n]));
    // Toleranz der Berührungsprüfung unten: Abstand bis 1e-6 · Kantenlänge
    let longest = (0..n)
        .map(|i| (edge(i).1 - edge(i).0).length())
        .fold(0.0, f64::max);
    let margin = 1e-6 * longest + 1e-9;
    let lo = |i: usize| edge(i).0.x.min(edge(i).1.x);
    let hi = |i: usize| edge(i).0.x.max(edge(i).1.x);
    let mut order: Vec<usize> = (0..n).collect();
    order.sort_by(|&a, &b| lo(a).total_cmp(&lo(b)));
    for (k, &first) in order.iter().enumerate() {
        let end = hi(first) + margin;
        for &second in &order[k + 1..] {
            if lo(second) > end {
                break;
            }
            if !edges_ok(pts, first.min(second), first.max(second)) {
                return false;
            }
        }
    }
    true
}

/// Kanten `i` und `j` (`i < j`) eines Polygons: keine Überschneidung, kein
/// Endpunkt im Inneren der anderen Kante, Nachbarn laufen nicht zurück.
fn edges_ok(pts: &[Vec3], i: usize, j: usize) -> bool {
    let n = pts.len();
    let (a, b) = (flat(pts[i]), flat(pts[(i + 1) % n]));
    let (c, d) = (flat(pts[j]), flat(pts[(j + 1) % n]));
    let neighbours = j == i + 1 || (i == 0 && j == n - 1);
    if neighbours {
        // Nachbarkanten dürfen nicht auf sich zurücklaufen
        let (u, w) = if j == i + 1 {
            (b - a, d - c)
        } else {
            (d - c, b - a)
        };
        return !(cross2(u, w).abs() < 1e-9 * u.length() * w.length() && u.dot(w) < 0.0);
    }
    if segments_cross(a, b, c, d) {
        return false;
    }
    // Ein Punkt einer Kante liegt im Inneren der anderen
    for (p, q0, q1) in [(c, a, b), (d, a, b), (a, c, d), (b, c, d)] {
        let r = q1 - q0;
        let t = (p - q0).dot(r) / r.dot(r).max(1e-12);
        if t > 1e-9 && t < 1.0 - 1e-9 && cross2(p - q0, r).abs() < 1e-6 * r.length() {
            return false;
        }
    }
    true
}

/// Ergebnis eines Versatzes nach innen.
#[derive(Clone, Debug, PartialEq)]
pub struct Inset {
    /// Versetztes Polygon, gegen den Uhrzeigersinn.
    pub pts: Vec<Vec3>,
    /// Für jeden Punkt des Ausgangspolygons der Punkt in `pts`, zu dem er wandert.
    /// Zwei Nachbarn mit demselben Ziel: Die Kante zwischen ihnen ist weggefallen.
    pub map: Vec<usize>,
}

/// Warum ein Versatz kein Polygon liefert.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InsetError {
    /// Ausgangspolygon nicht einfach oder zu wenig Punkte.
    Invalid,
    /// Die Fläche ist beim Versatz ganz verschwunden (Polygon schmaler als 2 × Abstand).
    Vanished,
    /// Das versetzte Polygon überschneidet sich (Engstelle zwischen entfernten Teilen).
    Overlap,
}

/// Lage des Schnittpunkts zweier Nachbarlinien in Abhängigkeit vom Abstand t:
/// X(t) = x0 + t · dx.
fn joint(p: &[Vec3], u: &[Vec3], a: usize, b: usize) -> (Vec3, Vec3) {
    let (na, nb) = (left_of(u[a]), left_of(u[b]));
    let c = cross2(u[a], u[b]);
    if c.abs() < 1e-12 {
        // gerade weiter (oder Spitze): Anfang der zweiten Linie
        return (p[b], nb);
    }
    let at = |t: f64| {
        let s = cross2(p[b] - p[a] + (nb - na) * t, u[b]) / c;
        p[a] + na * t + u[a] * s
    };
    let x0 = at(0.0);
    (x0, at(1.0) - x0)
}

/// Versetzt ein einfaches Polygon um `d` (> 0) nach innen, mit Gehrungsecken.
///
/// Kanten, die dabei auf null schrumpfen, fallen in der Reihenfolge ihres
/// Verschwindens weg, ihre Nachbarn treffen sich direkt (wie beim gerade
/// Skelett). So entstehen auch bei kurzen Kanten, spitzen Winkeln und
/// Rücksprüngen saubere Ecken ohne Schleifen.
pub fn inset(pts: &[Vec3], d: f64) -> Result<Inset, InsetError> {
    let p0 = to_ccw(pts);
    let ccw_flipped = signed_area(pts) < 0.0;
    let n = p0.len();
    if !is_simple(&p0) {
        return Err(InsetError::Invalid);
    }
    let u: Vec<Vec3> = (0..n)
        .map(|i| (p0[(i + 1) % n] - p0[i]).normalized())
        .collect();
    // Kante i beginnt bei Punkt i
    let mut active: Vec<usize> = (0..n).collect();
    let mut removed = vec![false; n];
    loop {
        let m = active.len();
        if m < 3 {
            return Err(InsetError::Vanished);
        }
        // Gegenläufige Nachbarn: Die Kante zwischen ihnen ist eben weggefallen,
        // ihre Linien liegen aufeinander. Übrig bleibt eine Spitze ohne
        // Fläche, beide fallen weg.
        if let Some(k) = (0..m).find(|&k| {
            let (a, b) = (u[active[k]], u[active[(k + 1) % m]]);
            cross2(a, b).abs() < 1e-9 && a.dot(b) < 0.0
        }) {
            let k2 = (k + 1) % m;
            removed[active[k]] = true;
            removed[active[k2]] = true;
            let (hi, lo) = (k.max(k2), k.min(k2));
            active.remove(hi);
            active.remove(lo);
            continue;
        }
        // Frühestes Verschwinden einer Kante suchen
        let mut first: Option<(f64, usize)> = None;
        for k in 0..m {
            let (a, e, b) = (active[(k + m - 1) % m], active[k], active[(k + 1) % m]);
            let (s0, sd) = joint(&p0, &u, a, e);
            let (e0, ed) = joint(&p0, &u, e, b);
            let l0 = (e0 - s0).dot(u[e]);
            let l1 = (ed - sd).dot(u[e]);
            let t = if l0 <= EPS {
                0.0
            } else if l1 < -1e-12 {
                -l0 / l1
            } else {
                continue;
            };
            if t < d && first.is_none_or(|(ft, _)| t < ft) {
                first = Some((t, k));
            }
        }
        let Some((_, k)) = first else { break };
        removed[active[k]] = true;
        active.remove(k);
    }
    let m = active.len();
    let out: Vec<Vec3> = (0..m)
        .map(|k| {
            let (x0, dx) = joint(&p0, &u, active[(k + m - 1) % m], active[k]);
            x0 + dx * d
        })
        .collect();
    if signed_area(&out) <= EPS {
        return Err(InsetError::Vanished);
    }
    // Jede Kante muss ihre Richtung behalten haben
    for k in 0..m {
        let v = out[(k + 1) % m] - out[k];
        if v.dot(u[active[k]]) < -EPS {
            return Err(InsetError::Overlap);
        }
    }
    if !is_simple(&simplified(&out)) {
        return Err(InsetError::Overlap);
    }
    // Punkt i des Ausgangs → Anfang der nächsten noch vorhandenen Kante ab i
    let pos_of = |e: usize| active.iter().position(|&x| x == e).unwrap();
    let mut map: Vec<usize> = (0..n)
        .map(|i| {
            let mut e = i;
            while removed[e] {
                e = (e + 1) % n;
            }
            pos_of(e)
        })
        .collect();
    let mut out = out;
    if ccw_flipped {
        // Zurück in die Richtung des Aufrufers: Punkt i des Aufrufers ist
        // Punkt n−1−i der umgedrehten Folge, Ziel j wird m−1−j
        out.reverse();
        map.reverse();
        let m = out.len();
        for x in map.iter_mut() {
            *x = m - 1 - *x;
        }
    }
    Ok(Inset { pts: out, map })
}

/// Zerlegt ein einfaches Polygon in Dreiecke (Ohrenschneiden).
/// Liefert Indizes gegen den Uhrzeigersinn, unabhängig von der Eingaberichtung.
///
/// Ob ein Punkt im Ohr liegt, wird nur für nach innen geknickte Ecken geprüft:
/// Liegt irgendeine Ecke im Dreieck, dann auch eine geknickte. Ecken werden nur
/// von geknickt zu gerade oder konvex, nie umgekehrt. Bei fast konvexen
/// Grundrissen ist der Aufwand so nahezu linear statt quadratisch; die
/// Gründung wird bei jeder Mausbewegung neu zerlegt.
pub fn triangulate(pts: &[Vec3]) -> Vec<[usize; 3]> {
    let n = pts.len();
    let mut idx: Vec<usize> = (0..n).collect();
    if signed_area(pts) < 0.0 {
        idx.reverse();
    }
    let p = |i: usize| flat(pts[i]);
    let turn = |idx: &[usize], k: usize| {
        let m = idx.len();
        let (a, b, c) = (p(idx[(k + m - 1) % m]), p(idx[k]), p(idx[(k + 1) % m]));
        let t = cross2(b - a, c - b);
        let straight = t.abs() <= 1e-9 * (b - a).length() * (c - b).length();
        (t, straight)
    };
    // Gerade weiter laufende oder doppelte Punkte entfernen (ohne Dreieck)
    let drop_straight = |idx: &mut Vec<usize>| {
        let mut k = 0;
        while idx.len() > 3 && k < idx.len() {
            if turn(idx, k).1 {
                idx.remove(k);
                k = k.saturating_sub(1);
            } else {
                k += 1;
            }
        }
    };
    drop_straight(&mut idx);
    let mut reflex = vec![false; n];
    for k in 0..idx.len() {
        reflex[idx[k]] = turn(&idx, k).0 <= 0.0;
    }
    let mut reflex_list: Vec<usize> = idx.iter().copied().filter(|&i| reflex[i]).collect();
    let mut tris = Vec::with_capacity(n.saturating_sub(2));
    let mut guard = 0;
    while idx.len() > 3 && guard < 4 * n * n + 16 {
        guard += 1;
        let m = idx.len();
        reflex_list.retain(|&i| reflex[i]);
        let mut cut = None;
        for k in 0..m {
            let (ia, ib, ic) = (idx[(k + m - 1) % m], idx[k], idx[(k + 1) % m]);
            if reflex[ib] {
                continue; // nach innen geknickt
            }
            let (a, b, c) = (p(ia), p(ib), p(ic));
            let inside = reflex_list.iter().any(|&j| {
                if j == ia || j == ib || j == ic {
                    return false;
                }
                let q = p(j);
                cross2(b - a, q - a) >= -1e-9
                    && cross2(c - b, q - b) >= -1e-9
                    && cross2(a - c, q - c) >= -1e-9
            });
            if !inside {
                cut = Some(k);
                break;
            }
        }
        let Some(k) = cut else {
            break; // kein Ohr: Polygon nicht einfach
        };
        tris.push([idx[(k + m - 1) % m], idx[k], idx[(k + 1) % m]]);
        reflex[idx[k]] = false;
        idx.remove(k);
        // Die beiden Nachbarn können gerade oder konvex geworden sein; fällt
        // ein gerader weg, sind wiederum seine Nachbarn zu prüfen
        let m = idx.len();
        let mut check = vec![idx[(k + m - 1) % m], idx[k % m]];
        while let Some(v) = check.pop() {
            if idx.len() <= 3 {
                break;
            }
            let Some(k) = idx.iter().position(|&i| i == v) else {
                continue;
            };
            let (t, straight) = turn(&idx, k);
            if straight {
                reflex[v] = false;
                idx.remove(k);
                let m = idx.len();
                check.push(idx[(k + m - 1) % m]);
                check.push(idx[k % m]);
            } else {
                reflex[v] = t <= 0.0;
            }
        }
    }
    if idx.len() == 3 {
        let (a, b, c) = (p(idx[0]), p(idx[1]), p(idx[2]));
        if cross2(b - a, c - b) > 0.0 {
            tris.push([idx[0], idx[1], idx[2]]);
        }
    }
    tris
}

/// Abschnitte, in denen die senkrechte Ebene durch `p0` mit Normale `n` das
/// Innere des Polygons trifft, als Lage längs `along` (sortiert, paarweise).
pub fn plane_intervals(pts: &[Vec3], p0: Vec3, n: Vec3, along: Vec3) -> Vec<(f64, f64)> {
    let side = |p: Vec3| (flat(p) - flat(p0)).dot(n);
    let k = pts.len();
    let mut hits: Vec<f64> = Vec::new();
    for i in 0..k {
        let (a, b) = (flat(pts[i]), flat(pts[(i + 1) % k]));
        let (da, db) = (side(a), side(b));
        // halboffen, damit Ecken auf der Ebene genau einmal zählen
        if (da < 0.0) != (db < 0.0) {
            let x = a + (b - a) * (da / (da - db));
            hits.push(x.dot(along));
        }
    }
    hits.sort_by(f64::total_cmp);
    hits.chunks_exact(2)
        .map(|c| (c[0], c[1]))
        .filter(|(a, b)| b - a > EPS)
        .collect()
}

#[cfg(test)]
mod tests {
    /// Alle Kantenpaare, wie vor der Sortierung: Vergleich für [`is_simple`].
    fn is_simple_all_pairs(pts: &[Vec3]) -> bool {
        let n = pts.len();
        if n < 3 || area(pts) < EPS {
            return false;
        }
        (0..n).all(|i| (i + 1..n).all(|j| edges_ok(pts, i, j)))
    }

    #[test]
    fn zerlegung_deckt_zufaellige_vielecke() {
        let mut seed: u64 = 0x2545_f491_4f6c_dd1d;
        let mut rnd = move || {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            (seed % 10_000) as f64 / 10_000.0
        };
        let mut checked = 0;
        for case in 0..4000 {
            let n = 3 + case % 60;
            let mut pts: Vec<Vec3> = (0..n)
                .map(|k| {
                    let a = k as f64 / n as f64 * std::f64::consts::TAU;
                    let r = 1000.0 + 2500.0 * rnd() * (case % 4) as f64;
                    vec3((r * a.cos()).round(), (r * a.sin()).round(), 0.0)
                })
                .collect();
            if case % 5 == 0 {
                // Zwischenpunkt auf einer Kante (gerade weiter)
                let k = case % n;
                let mid = (pts[k] + pts[(k + 1) % n]) * 0.5;
                pts.insert(k + 1, mid);
            }
            if case % 2 == 1 {
                pts.reverse();
            }
            if !is_simple(&pts) {
                continue;
            }
            checked += 1;
            let t = triangulate(&pts);
            let sum: f64 = t
                .iter()
                .map(|t| area(&[pts[t[0]], pts[t[1]], pts[t[2]]]))
                .sum();
            assert!(
                (sum - area(&pts)).abs() < 1e-6 * area(&pts).max(1.0),
                "Fall {case}: {sum} statt {}",
                area(&pts)
            );
            for t in &t {
                let (a, b, c) = (pts[t[0]], pts[t[1]], pts[t[2]]);
                assert!(
                    cross2(b - a, c - b) > 0.0,
                    "Fall {case}: Dreieck nicht gegen Uhrzeigersinn"
                );
            }
        }
        assert!(checked > 2000, "{checked}");
    }

    #[test]
    fn einfach_gleich_wie_alle_paare() {
        // Pseudozufällige Vielecke: Sterne, Zickzack, Kreise mit Störung,
        // doppelte und zurücklaufende Punkte
        let mut seed: u64 = 0x9e37_79b9_7f4a_7c15;
        let mut rnd = move || {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            (seed % 10_000) as f64 / 10_000.0
        };
        let mut simple = 0;
        for case in 0..3000 {
            let n = 3 + case % 40;
            let mut pts: Vec<Vec3> = (0..n)
                .map(|k| {
                    let step = std::f64::consts::TAU / n as f64;
                    // jeder vierte Fall mit Winkelrauschen: Kanten kreuzen sich
                    let jitter = if case % 4 == 0 {
                        (rnd() - 0.5) * 6.0
                    } else {
                        0.0
                    };
                    let a = (k as f64 + jitter) * step;
                    let r = 1000.0 + 900.0 * rnd() * (case % 3) as f64;
                    vec3((r * a.cos()).round(), (r * a.sin()).round(), 0.0)
                })
                .collect();
            if case % 7 == 0 {
                let k = (rnd() * n as f64) as usize % n;
                pts[k] = pts[(k + 1) % n];
            }
            if case % 11 == 0 {
                let k = (rnd() * n as f64) as usize % n;
                pts[k] = vec3((rnd() * 4000.0 - 2000.0).round(), 0.0, 0.0);
            }
            let fast = is_simple(&pts);
            assert_eq!(fast, is_simple_all_pairs(&pts), "Fall {case}: {pts:?}");
            simple += fast as usize;
        }
        // beide Ausgänge kommen vor
        assert!(simple > 300 && simple < 2700, "{simple}");
    }

    use super::*;

    fn rect(w: f64, h: f64) -> Vec<Vec3> {
        vec![
            vec3(0.0, 0.0, 0.0),
            vec3(w, 0.0, 0.0),
            vec3(w, h, 0.0),
            vec3(0.0, h, 0.0),
        ]
    }

    fn l_shape() -> Vec<Vec3> {
        // 10 × 8 m mit 4 × 3 m Ausschnitt oben rechts
        vec![
            vec3(0.0, 0.0, 0.0),
            vec3(10000.0, 0.0, 0.0),
            vec3(10000.0, 5000.0, 0.0),
            vec3(6000.0, 5000.0, 0.0),
            vec3(6000.0, 8000.0, 0.0),
            vec3(0.0, 8000.0, 0.0),
        ]
    }

    #[test]
    fn flaeche_und_umfang() {
        assert!((area(&rect(10000.0, 8000.0)) - 80e6).abs() < 1e-6);
        assert!((perimeter(&rect(10000.0, 8000.0)) - 36000.0).abs() < 1e-9);
        assert!((area(&l_shape()) - 68e6).abs() < 1e-6);
    }

    #[test]
    fn rechteck_nach_innen() {
        let r = inset(&rect(10000.0, 8000.0), 175.0).unwrap();
        assert!((perimeter(&r.pts) - 34600.0).abs() < 1e-6);
        assert_eq!(r.map, vec![0, 1, 2, 3]);
        // auch im Uhrzeigersinn
        let mut cw = rect(10000.0, 8000.0);
        cw.reverse();
        let r = inset(&cw, 175.0).unwrap();
        assert!(signed_area(&r.pts) < 0.0);
        for (i, &j) in r.map.iter().enumerate() {
            assert!((r.pts[j] - cw[i]).length() < 175.0 * 1.5, "{i} {j}");
        }
    }

    #[test]
    fn l_form_mit_innenecke() {
        let r = inset(&l_shape(), 350.0).unwrap();
        assert_eq!(r.pts.len(), 6);
        let expect = 68e6 - 350.0 * perimeter(&l_shape()) + 350.0 * 350.0 * 4.0;
        // Fläche nach Versatz: A − d·U + d²·Σcot(α/2) (5 konvexe, 1 Innenecke)
        assert!((area(&r.pts) - expect).abs() < 1.0, "{}", area(&r.pts));
    }

    #[test]
    fn kurze_kante_faellt_weg() {
        // Rechteck mit 200 mm breitem Vorsprung, Versatz 350 mm
        let p = vec![
            vec3(0.0, 0.0, 0.0),
            vec3(4000.0, 0.0, 0.0),
            vec3(4000.0, -200.0, 0.0),
            vec3(4200.0, -200.0, 0.0),
            vec3(4200.0, 0.0, 0.0),
            vec3(8000.0, 0.0, 0.0),
            vec3(8000.0, 6000.0, 0.0),
            vec3(0.0, 6000.0, 0.0),
        ];
        let r = inset(&p, 350.0).unwrap();
        assert!(is_simple(&r.pts));
        // Vorsprung verschwindet, übrig bleibt ein Rechteck
        let s = simplified(&r.pts);
        assert_eq!(s.len(), 4, "{s:?}");
        assert!((area(&s) - 7300.0 * 5300.0).abs() < 1.0);
        // Die Punkte des Vorsprungs wandern auf denselben Punkt
        assert_eq!(r.map[2], r.map[3]);
    }

    #[test]
    fn spitzer_winkel() {
        // Dreieck mit 10° Spitze
        let a = 10f64.to_radians();
        let p = vec![
            vec3(0.0, 0.0, 0.0),
            vec3(20000.0, 0.0, 0.0),
            vec3(20000.0 * a.cos(), 20000.0 * a.sin(), 0.0),
        ];
        let r = inset(&p, 350.0).unwrap();
        assert!(is_simple(&r.pts));
        // Spitze rückt um d / sin(α/2) nach innen
        let tip = r.pts[0];
        assert!((tip.length() - 350.0 / (a / 2.0).sin()).abs() < 1e-6);
    }

    #[test]
    fn zu_schmal_verschwindet() {
        assert_eq!(
            inset(&rect(600.0, 5000.0), 350.0),
            Err(InsetError::Vanished)
        );
        assert!(inset(&rect(800.0, 5000.0), 350.0).is_ok());
    }

    #[test]
    fn schleife_wird_abgelehnt() {
        let p = vec![
            vec3(0.0, 0.0, 0.0),
            vec3(1000.0, 1000.0, 0.0),
            vec3(1000.0, 0.0, 0.0),
            vec3(0.0, 1000.0, 0.0),
        ];
        assert!(!is_simple(&p));
        assert_eq!(inset(&p, 10.0), Err(InsetError::Invalid));
    }

    #[test]
    fn schmaler_hof_weitet_sich() {
        // U-Form mit 500 mm Schlitz von oben: Der Schlitz liegt außen und wird breiter
        let p = vec![
            vec3(0.0, 0.0, 0.0),
            vec3(10000.0, 0.0, 0.0),
            vec3(10000.0, 8000.0, 0.0),
            vec3(5250.0, 8000.0, 0.0),
            vec3(5250.0, 2000.0, 0.0),
            vec3(4750.0, 2000.0, 0.0),
            vec3(4750.0, 8000.0, 0.0),
            vec3(0.0, 8000.0, 0.0),
        ];
        let r = inset(&p, 350.0).unwrap();
        assert!(is_simple(&r.pts));
        assert!((r.pts[4].x - 5600.0).abs() < 1e-6 && (r.pts[5].x - 4400.0).abs() < 1e-6);
    }

    #[test]
    fn enger_flur_wird_erkannt() {
        // Zwei Räume, verbunden durch einen 500 mm schmalen Flur: Bei 350 mm
        // Versatz laufen die Flurkanten übereinander
        let p = vec![
            vec3(0.0, 0.0, 0.0),
            vec3(4000.0, 0.0, 0.0),
            vec3(4000.0, 1750.0, 0.0),
            vec3(8000.0, 1750.0, 0.0),
            vec3(8000.0, 0.0, 0.0),
            vec3(12000.0, 0.0, 0.0),
            vec3(12000.0, 4000.0, 0.0),
            vec3(8000.0, 4000.0, 0.0),
            vec3(8000.0, 2250.0, 0.0),
            vec3(4000.0, 2250.0, 0.0),
            vec3(4000.0, 4000.0, 0.0),
            vec3(0.0, 4000.0, 0.0),
        ];
        assert_eq!(inset(&p, 350.0), Err(InsetError::Overlap));
        assert!(inset(&p, 200.0).is_ok());
    }

    #[test]
    fn dreiecke_decken_flaeche() {
        for p in [rect(10000.0, 8000.0), l_shape()] {
            let t = triangulate(&p);
            assert_eq!(t.len(), p.len() - 2);
            let sum: f64 = t.iter().map(|t| area(&[p[t[0]], p[t[1]], p[t[2]]])).sum();
            assert!((sum - area(&p)).abs() < 1e-3);
        }
        // mit Zwischenpunkt auf gerader Kante
        let mut p = rect(4000.0, 3000.0);
        p.insert(1, vec3(2000.0, 0.0, 0.0));
        let t = triangulate(&p);
        let sum: f64 = t.iter().map(|t| area(&[p[t[0]], p[t[1]], p[t[2]]])).sum();
        assert!((sum - 12e6).abs() < 1e-3);
    }

    #[test]
    fn schnitt_durch_l_form() {
        let iv = plane_intervals(
            &l_shape(),
            vec3(0.0, 6000.0, 0.0),
            vec3(0.0, 1.0, 0.0),
            vec3(1.0, 0.0, 0.0),
        );
        assert_eq!(iv, vec![(0.0, 6000.0)]);
        let iv = plane_intervals(
            &l_shape(),
            vec3(8000.0, 0.0, 0.0),
            vec3(1.0, 0.0, 0.0),
            vec3(0.0, 1.0, 0.0),
        );
        assert_eq!(iv, vec![(0.0, 5000.0)]);
    }

    #[test]
    fn schwerpunkt_rechteck_und_winkel() {
        let r = [
            vec3(0.0, 0.0, 0.0),
            vec3(4.0, 0.0, 0.0),
            vec3(4.0, 2.0, 0.0),
            vec3(0.0, 2.0, 0.0),
        ];
        let c = centroid(&r).unwrap();
        assert!((c.x - 2.0).abs() < 1e-9 && (c.y - 1.0).abs() < 1e-9);
        // Uhrzeigersinn gibt denselben Punkt
        let mut cw = r;
        cw.reverse();
        let d = centroid(&cw).unwrap();
        assert!((d.x - 2.0).abs() < 1e-9 && (d.y - 1.0).abs() < 1e-9);
        assert!(centroid(&r[..2]).is_none());
    }
}
