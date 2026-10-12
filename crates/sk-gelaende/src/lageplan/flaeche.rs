//! Flächen aus Grenzlinien (plan-gelaendemodul.md §5 G1c, „Grundstück ohne
//! Nachfahren“): die Grenzstriche werden ein ebener Graph (Enden
//! zusammengefangen, an Kreuzungen und Stößen geteilt, freie Enden wie die
//! Bruchstriche der Flurstücksnummern entfernt); seine Maschen sind die
//! Flächen. Gesucht ist die kleinste Masche um einen Punkt.

/// Enden, die näher als das zusammenliegen, sind ein Punkt (pt).
const FANG: f64 = 0.02;

/// Ebener Graph aus Strecken.
struct Graph {
    punkte: Vec<[f64; 2]>,
    /// Ausgehende Kanten je Punkt, nach Winkel sortiert.
    nach: Vec<Vec<usize>>,
}

/// Punkt an der Stelle `p` suchen oder anlegen (Raster der Größe FANG).
fn punkt_id(
    p: [f64; 2],
    punkte: &mut Vec<[f64; 2]>,
    raster: &mut std::collections::BTreeMap<(i64, i64), Vec<usize>>,
) -> usize {
    let k = ((p[0] / FANG).floor() as i64, (p[1] / FANG).floor() as i64);
    for dx in -1..=1 {
        for dy in -1..=1 {
            if let Some(v) = raster.get(&(k.0 + dx, k.1 + dy)) {
                for &i in v {
                    let q = punkte[i];
                    if (q[0] - p[0]).hypot(q[1] - p[1]) < FANG {
                        return i;
                    }
                }
            }
        }
    }
    punkte.push(p);
    raster.entry(k).or_default().push(punkte.len() - 1);
    punkte.len() - 1
}

/// Parameter t (0..1) auf a→b, an dem sich a→b und c→d schneiden, und der
/// auf c→d.
fn schnitt(a: [f64; 2], b: [f64; 2], c: [f64; 2], d: [f64; 2]) -> Option<(f64, f64)> {
    let r = [b[0] - a[0], b[1] - a[1]];
    let s = [d[0] - c[0], d[1] - c[1]];
    let n = r[0] * s[1] - r[1] * s[0];
    if n.abs() < 1e-12 {
        return None;
    }
    let q = [c[0] - a[0], c[1] - a[1]];
    let t = (q[0] * s[1] - q[1] * s[0]) / n;
    let u = (q[0] * r[1] - q[1] * r[0]) / n;
    Some((t, u))
}

/// Abstand von p zur Strecke a→b und der Parameter des Fußpunkts.
fn fuss(p: [f64; 2], a: [f64; 2], b: [f64; 2]) -> (f64, f64) {
    let r = [b[0] - a[0], b[1] - a[1]];
    let l2 = r[0] * r[0] + r[1] * r[1];
    if l2 < 1e-18 {
        return ((p[0] - a[0]).hypot(p[1] - a[1]), 0.0);
    }
    let t = (((p[0] - a[0]) * r[0] + (p[1] - a[1]) * r[1]) / l2).clamp(0.0, 1.0);
    ((p[0] - a[0] - t * r[0]).hypot(p[1] - a[1] - t * r[1]), t)
}

impl Graph {
    fn neu(strecken: &[[[f64; 2]; 2]]) -> Graph {
        // Teilungsstellen je Strecke: Kreuzungen und Enden anderer Strecken
        let n = strecken.len();
        let mut teile: Vec<Vec<f64>> = vec![vec![0.0, 1.0]; n];
        let kasten: Vec<[f64; 4]> = strecken
            .iter()
            .map(|s| {
                [
                    s[0][0].min(s[1][0]) - FANG,
                    s[0][1].min(s[1][1]) - FANG,
                    s[0][0].max(s[1][0]) + FANG,
                    s[0][1].max(s[1][1]) + FANG,
                ]
            })
            .collect();
        for i in 0..n {
            for j in 0..n {
                if i == j {
                    continue;
                }
                let (ki, kj) = (kasten[i], kasten[j]);
                if ki[0] > kj[2] || kj[0] > ki[2] || ki[1] > kj[3] || kj[1] > ki[3] {
                    continue;
                }
                let (a, b) = (strecken[i][0], strecken[i][1]);
                for e in strecken[j] {
                    let (d, t) = fuss(e, a, b);
                    if d < FANG && t > 0.0 && t < 1.0 {
                        teile[i].push(t);
                    }
                }
                if i < j {
                    let (c, d) = (strecken[j][0], strecken[j][1]);
                    if let Some((t, u)) = schnitt(a, b, c, d) {
                        if t > 0.0 && t < 1.0 && u > 0.0 && u < 1.0 {
                            teile[i].push(t);
                            teile[j].push(u);
                        }
                    }
                }
            }
        }
        let mut punkte = Vec::new();
        let mut raster = std::collections::BTreeMap::new();
        let mut kanten = std::collections::BTreeSet::new();
        for (s, t) in strecken.iter().zip(&mut teile) {
            t.sort_by(f64::total_cmp);
            let ids: Vec<usize> = t
                .iter()
                .map(|&t| {
                    let p = [
                        s[0][0] + t * (s[1][0] - s[0][0]),
                        s[0][1] + t * (s[1][1] - s[0][1]),
                    ];
                    punkt_id(p, &mut punkte, &mut raster)
                })
                .collect();
            for w in ids.windows(2) {
                if w[0] != w[1] {
                    kanten.insert((w[0].min(w[1]), w[0].max(w[1])));
                }
            }
        }
        let mut nach = vec![Vec::new(); punkte.len()];
        for &(a, b) in &kanten {
            nach[a].push(b);
            nach[b].push(a);
        }
        // freie Enden abbauen (Bruchstriche, Linien ins Leere)
        let mut stapel: Vec<usize> = (0..punkte.len()).filter(|&i| nach[i].len() == 1).collect();
        while let Some(i) = stapel.pop() {
            if nach[i].len() != 1 {
                continue;
            }
            let j = nach[i][0];
            nach[i].clear();
            nach[j].retain(|&k| k != i);
            if nach[j].len() == 1 {
                stapel.push(j);
            }
        }
        for i in 0..punkte.len() {
            let p = punkte[i];
            let w = |k: &usize| (punkte[*k][1] - p[1]).atan2(punkte[*k][0] - p[0]);
            nach[i].sort_by(|a, b| w(a).total_cmp(&w(b)));
        }
        Graph { punkte, nach }
    }

    /// Alle Maschen als Punktfolgen, die Fläche jeweils links (innere
    /// Maschen gegen den Uhrzeigersinn, Fläche positiv).
    fn maschen(&self) -> Vec<Vec<usize>> {
        let mut gesehen = std::collections::BTreeSet::new();
        let mut aus = Vec::new();
        for u in 0..self.punkte.len() {
            for &v in &self.nach[u] {
                if gesehen.contains(&(u, v)) {
                    continue;
                }
                let mut m = Vec::new();
                let (mut a, mut b) = (u, v);
                while gesehen.insert((a, b)) && m.len() <= 4 * self.punkte.len() {
                    m.push(a);
                    // in b: die Kante im Uhrzeigersinn nach b→a
                    let n = &self.nach[b];
                    let Some(k) = n.iter().position(|&x| x == a) else {
                        break;
                    };
                    let c = n[(k + n.len() - 1) % n.len()];
                    (a, b) = (b, c);
                }
                aus.push(m);
            }
        }
        aus
    }
}

/// Fläche mit Vorzeichen (gegen den Uhrzeigersinn positiv).
pub fn flaeche(p: &[[f64; 2]]) -> f64 {
    let Some(&o) = p.first() else { return 0.0 };
    let mut a = 0.0;
    for i in 0..p.len() {
        let (u, v) = (p[i], p[(i + 1) % p.len()]);
        a += (u[0] - o[0]) * (v[1] - o[1]) - (v[0] - o[0]) * (u[1] - o[1]);
    }
    a / 2.0
}

/// Liegt q im Polygon (gerade-ungerade)?
pub fn drin(p: &[[f64; 2]], q: [f64; 2]) -> bool {
    let mut d = false;
    for i in 0..p.len() {
        let (a, b) = (p[i], p[(i + 1) % p.len()]);
        if (a[1] > q[1]) != (b[1] > q[1]) {
            let x = a[0] + (q[1] - a[1]) / (b[1] - a[1]) * (b[0] - a[0]);
            if q[0] < x {
                d = !d;
            }
        }
    }
    d
}

/// Punkte, an denen der Zug geradeaus weitergeht (unter 0,5°), fallen weg.
fn ecken(p: Vec<[f64; 2]>) -> Vec<[f64; 2]> {
    let n = p.len();
    if n < 4 {
        return p;
    }
    let mut aus = Vec::new();
    for i in 0..n {
        let (a, b, c) = (p[(i + n - 1) % n], p[i], p[(i + 1) % n]);
        let w1 = (b[1] - a[1]).atan2(b[0] - a[0]);
        let w2 = (c[1] - b[1]).atan2(c[0] - b[0]);
        let mut d = (w2 - w1).to_degrees().abs() % 360.0;
        if d > 180.0 {
            d = 360.0 - d;
        }
        if d >= 0.5 {
            aus.push(b);
        }
    }
    aus
}

/// Kleinste Fläche aus den Strecken, die `q` enthält: Ecken gegen den
/// Uhrzeigersinn (Punkte auf geraden Seiten fallen weg).
pub fn kleinste_um(strecken: &[[[f64; 2]; 2]], q: [f64; 2]) -> Option<Vec<[f64; 2]>> {
    let g = Graph::neu(strecken);
    g.maschen()
        .into_iter()
        .map(|m| m.iter().map(|&i| g.punkte[i]).collect::<Vec<_>>())
        .filter(|p| p.len() >= 3 && flaeche(p) > 0.0 && drin(p, q))
        .min_by(|a, b| flaeche(a).total_cmp(&flaeche(b)))
        .map(ecken)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn zug(p: &[[f64; 2]]) -> Vec<[[f64; 2]; 2]> {
        p.windows(2).map(|w| [w[0], w[1]]).collect()
    }

    #[test]
    fn zwei_flurstuecke_mit_stoss_und_bruchstrich() {
        // zwei Rechtecke nebeneinander, die Mittellinie stößt oben und unten
        // stumpf an; dazu ein Bruchstrich mit freien Enden und eine Linie, die
        // über den Rand hinausläuft
        let mut s = zug(&[
            [0.0, 0.0],
            [20.0, 0.0],
            [20.0, 10.0],
            [0.0, 10.0],
            [0.0, 0.0],
        ]);
        s.extend(zug(&[[8.0, 0.0], [8.0, 10.0]]));
        s.extend(zug(&[[2.0, 5.0], [6.0, 5.0]]));
        s.extend(zug(&[[20.0, 3.0], [30.0, 3.0]]));
        let links = kleinste_um(&s, [4.0, 4.0]).unwrap();
        assert_eq!(links.len(), 4, "{links:?}");
        assert!((flaeche(&links) - 80.0).abs() < 1e-9);
        let rechts = kleinste_um(&s, [12.0, 2.0]).unwrap();
        assert!((flaeche(&rechts) - 120.0).abs() < 1e-9);
        assert_eq!(rechts.len(), 4, "Punkt auf gerader Seite fällt weg");
        assert_eq!(kleinste_um(&s, [25.0, 5.0]), None);
        // Kreuzung zweier Strecken teilt beide
        let x = vec![
            [[0.0, 0.0], [10.0, 10.0]],
            [[0.0, 10.0], [10.0, 0.0]],
            [[0.0, 0.0], [10.0, 0.0]],
            [[0.0, 10.0], [10.0, 10.0]],
            [[0.0, 0.0], [0.0, 10.0]],
        ];
        let d = kleinste_um(&x, [5.0, 1.0]).unwrap();
        assert!((flaeche(&d) - 25.0).abs() < 1e-9, "{d:?}");
    }

    #[test]
    fn fast_gleiche_enden_werden_eins() {
        let s = vec![
            [[0.0, 0.0], [10.0, 0.0]],
            [[10.005, 0.003], [10.0, 10.0]],
            [[10.0, 10.0], [0.0, 10.01]],
            [[0.0, 10.0], [0.0, 0.0]],
        ];
        let f = kleinste_um(&s, [5.0, 5.0]).unwrap();
        assert_eq!(f.len(), 4);
        assert!(drin(&f, [9.0, 9.0]) && !drin(&f, [11.0, 5.0]));
        assert_eq!(kleinste_um(&[], [0.0, 0.0]), None);
    }
}
