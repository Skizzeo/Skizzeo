//! Geglättetes Netz für Darstellung und Export (Jörn 17.10.): Jedes
//! Dreieck wird in `teile`² kleinere geteilt, die auf einem kubischen
//! Bézier-Dreieck aus den Höhen und Neigungen an seinen Ecken liegen.
//! Die Punkte des Aufmaßes bleiben, wo sie sind, nur zwischen ihnen wird
//! das Gelände rund. Ränder, Bruchkanten, Knicke ab [`KNICK_GRAD`], Kanten
//! zwischen einer ebenen und einer geneigten Fläche (Platte am Haus) und
//! Kanten fester Dreiecke bleiben gerade, so passt alles Angrenzende
//! ohne Spalt. Mengen rechnet hiermit niemand: Sie bleiben am Netz des
//! Aufmaßes.

use crate::netz::Netz;
use std::collections::{HashMap, HashSet};

/// Wie fein geglättet wird: aus, fein (jedes Dreieck in 4), sehr fein
/// (in 16).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Glaettung {
    #[default]
    Aus,
    Fein,
    SehrFein,
}

impl Glaettung {
    pub const ALLE: [Glaettung; 3] = [Glaettung::Aus, Glaettung::Fein, Glaettung::SehrFein];

    /// Teile je Kante eines Dreiecks.
    pub fn teile(self) -> u32 {
        match self {
            Glaettung::Aus => 1,
            Glaettung::Fein => 2,
            Glaettung::SehrFein => 4,
        }
    }

    /// Wort in der Datei.
    pub fn key(self) -> &'static str {
        match self {
            Glaettung::Aus => "aus",
            Glaettung::Fein => "fein",
            Glaettung::SehrFein => "sehrfein",
        }
    }
}

/// Ab diesem Winkel zwischen zwei Dreiecken (Grad) bleibt ihre Kante ein
/// Knick.
pub const KNICK_GRAD: f64 = 30.0;

/// Neigung, unter der ein Dreieck als eben gilt.
const EBEN: f64 = 1e-9;

/// Das geglättete Netz. Dreieck `i` des Netzes wird zu den Dreiecken
/// `i * je .. (i + 1) * je`; die Punkte des Netzes behalten ihre Nummern,
/// die neuen folgen dahinter.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Glatt {
    pub netz: Netz,
    pub je: usize,
}

/// Neigung (dz/dx, dz/dy) des Dreiecks, `None` senkrecht oder entartet.
fn neigung(p: &[[f64; 3]], t: [u32; 3]) -> Option<[f64; 2]> {
    let [a, b, c] = t.map(|i| p[i as usize]);
    let (u, v) = ([b[0] - a[0], b[1] - a[1]], [c[0] - a[0], c[1] - a[1]]);
    let (du, dv) = (b[2] - a[2], c[2] - a[2]);
    let det = u[0] * v[1] - u[1] * v[0];
    if det.abs() < 1e-12 * (u[0].hypot(u[1]) * v[0].hypot(v[1])).max(1e-300) {
        return None;
    }
    Some([(du * v[1] - dv * u[1]) / det, (u[0] * dv - v[0] * du) / det])
}

/// Winkel des Dreiecks an seiner Ecke `k` im Grundriss (Gewicht der
/// Neigung an der Ecke).
fn winkel(p: &[[f64; 3]], t: [u32; 3], k: usize) -> f64 {
    let [a, b, c] = [t[k], t[(k + 1) % 3], t[(k + 2) % 3]].map(|i| p[i as usize]);
    let (u, v) = ([b[0] - a[0], b[1] - a[1]], [c[0] - a[0], c[1] - a[1]]);
    (u[0] * v[1] - u[1] * v[0])
        .abs()
        .atan2(u[0] * v[0] + u[1] * v[1])
}

fn finden(eltern: &mut [u32], mut i: u32) -> u32 {
    while eltern[i as usize] != i {
        let e = eltern[eltern[i as usize] as usize];
        eltern[i as usize] = e;
        i = e;
    }
    i
}

fn vereinen(eltern: &mut [u32], a: u32, b: u32) {
    let (a, b) = (finden(eltern, a), finden(eltern, b));
    if a != b {
        eltern[a.max(b) as usize] = a.min(b);
    }
}

fn kante(a: u32, b: u32) -> (u32, u32) {
    (a.min(b), a.max(b))
}

/// `netz` geglättet, jedes Dreieck in `teile`² (bis 1: unverändert).
/// `fest[i]`: Dreieck `i` bleibt eben und seine Kanten gerade (an seiner
/// Stelle steht etwas anderes, das an diese Kanten anschließt); fehlt
/// der Eintrag, gilt es nicht als fest.
pub fn glaetten(netz: &Netz, teile: u32, fest: &[bool]) -> Glatt {
    let n = teile.max(1) as usize;
    if n == 1 {
        return Glatt {
            netz: netz.clone(),
            je: 1,
        };
    }
    let p = &netz.punkte;
    let tris = &netz.dreiecke;
    let ist_fest = |t: usize| fest.get(t).copied().unwrap_or(false);
    let neig: Vec<Option<[f64; 2]>> = tris.iter().map(|&t| neigung(p, t)).collect();
    // Dreiecke an jeder Kante
    let mut an: HashMap<(u32, u32), [u32; 2]> = HashMap::with_capacity(tris.len() * 2);
    for (i, t) in tris.iter().enumerate() {
        for k in 0..3 {
            let e = an
                .entry(kante(t[k], t[(k + 1) % 3]))
                .or_insert([u32::MAX; 2]);
            if e[0] == u32::MAX {
                e[0] = i as u32;
            } else {
                e[1] = i as u32;
            }
        }
    }
    let bruch: HashSet<(u32, u32)> = netz.bruch.iter().map(|b| kante(b[0], b[1])).collect();
    let knick_cos = KNICK_GRAD.to_radians().cos();
    let normale = |g: [f64; 2]| {
        let l = (g[0] * g[0] + g[1] * g[1] + 1.0).sqrt();
        [-g[0] / l, -g[1] / l, 1.0 / l]
    };
    let gerade = |e: &(u32, u32), ts: &[u32; 2]| -> bool {
        let [t0, t1] = *ts;
        if t1 == u32::MAX || bruch.contains(e) {
            return true;
        }
        let (t0, t1) = (t0 as usize, t1 as usize);
        if ist_fest(t0) || ist_fest(t1) {
            return true;
        }
        let (Some(g0), Some(g1)) = (neig[t0], neig[t1]) else {
            return true;
        };
        let eben = |g: [f64; 2]| g[0].abs() < EBEN && g[1].abs() < EBEN;
        if eben(g0) != eben(g1) {
            return true;
        }
        let (a, b) = (normale(g0), normale(g1));
        a[0] * b[0] + a[1] * b[1] + a[2] * b[2] < knick_cos
    };
    let knick: HashSet<(u32, u32)> = an
        .iter()
        .filter(|(e, ts)| gerade(e, ts))
        .map(|(e, _)| *e)
        .collect();
    // Neigung je Ecke: gemittelt über die Dreiecke um den Punkt, die über
    // glatte Kanten zusammenhängen (über einen Knick hinweg nicht)
    let ecke = |t: u32, v: u32| -> u32 {
        let d = tris[t as usize];
        t * 3 + (0..3).find(|&k| d[k] == v).unwrap_or(0) as u32
    };
    let mut eltern: Vec<u32> = (0..tris.len() as u32 * 3).collect();
    for (e, ts) in &an {
        if ts[1] == u32::MAX || knick.contains(e) {
            continue;
        }
        for v in [e.0, e.1] {
            let (a, b) = (ecke(ts[0], v), ecke(ts[1], v));
            vereinen(&mut eltern, a, b);
        }
    }
    let mut summe = vec![[0.0f64; 3]; eltern.len()];
    for (i, t) in tris.iter().enumerate() {
        let Some(g) = neig[i] else { continue };
        for k in 0..3 {
            let w = winkel(p, *t, k);
            let r = finden(&mut eltern, (i * 3 + k) as u32) as usize;
            summe[r][0] += g[0] * w;
            summe[r][1] += g[1] * w;
            summe[r][2] += w;
        }
    }
    let mut grad = vec![[0.0f64; 2]; eltern.len()];
    for (s, g) in grad.iter_mut().enumerate() {
        let r = finden(&mut eltern, s as u32) as usize;
        let [x, y, w] = summe[r];
        *g = if w > 0.0 {
            [x / w, y / w]
        } else {
            neig[s / 3].unwrap_or([0.0; 2])
        };
    }
    // Kontrollhöhe auf der Kante von Ecke `k` zur nächsten, nahe `k`
    let kontrolle = |t: usize, k: usize, l: usize| -> f64 {
        let d = tris[t];
        let (a, b) = (p[d[k] as usize], p[d[l] as usize]);
        if knick.contains(&kante(d[k], d[l])) || neig[t].is_none() {
            return a[2] + (b[2] - a[2]) / 3.0;
        }
        let g = grad[t * 3 + k];
        a[2] + (g[0] * (b[0] - a[0]) + g[1] * (b[1] - a[1])) / 3.0
    };
    let mut punkte = p.clone();
    let mut kanten: HashMap<(u32, u32), Vec<u32>> = HashMap::with_capacity(an.len());
    let mut dreiecke = Vec::with_capacity(tris.len() * n * n);
    let nf = n as f64;
    let mut gitter = vec![0u32; (n + 1) * (n + 2) / 2];
    let stelle = |i: usize, j: usize| -> usize {
        // Zeile j hat n + 1 - j Punkte
        j * (2 * n + 3 - j) / 2 + i
    };
    for (ti, d) in tris.iter().enumerate() {
        let [a, b, c] = d.map(|i| p[i as usize]);
        let [za, zb, zc] = [a[2], b[2], c[2]];
        let (cab, cba) = (kontrolle(ti, 0, 1), kontrolle(ti, 1, 0));
        let (cbc, ccb) = (kontrolle(ti, 1, 2), kontrolle(ti, 2, 1));
        let (cca, cac) = (kontrolle(ti, 2, 0), kontrolle(ti, 0, 2));
        let e = (cab + cba + cbc + ccb + cca + cac) / 6.0;
        let mitte = e + (e - (za + zb + zc) / 3.0) / 2.0;
        // Punkte einer Kante einmal, von ihrer kleineren Nummer aus
        let mut kante_holen = |x: u32, y: u32, cx: f64, cy: f64, punkte: &mut Vec<[f64; 3]>| {
            let (lo, hi, clo, chi) = if x < y {
                (x, y, cx, cy)
            } else {
                (y, x, cy, cx)
            };
            kanten
                .entry((lo, hi))
                .or_insert_with(|| {
                    let (q, r) = (p[lo as usize], p[hi as usize]);
                    (1..n)
                        .map(|s| {
                            let t = s as f64 / nf;
                            let u = 1.0 - t;
                            let z = q[2] * u * u * u
                                + 3.0 * clo * u * u * t
                                + 3.0 * chi * u * t * t
                                + r[2] * t * t * t;
                            punkte.push([q[0] + (r[0] - q[0]) * t, q[1] + (r[1] - q[1]) * t, z]);
                            (punkte.len() - 1) as u32
                        })
                        .collect()
                })
                .clone()
        };
        // auf der Kante von x nach y der s-te Punkt ab x (1..n)
        let auf = |v: &[u32], x: u32, y: u32, s: usize| -> u32 {
            if x < y {
                v[s - 1]
            } else {
                v[n - s - 1]
            }
        };
        let ab = kante_holen(d[0], d[1], cab, cba, &mut punkte);
        let bc = kante_holen(d[1], d[2], cbc, ccb, &mut punkte);
        let ca = kante_holen(d[2], d[0], cca, cac, &mut punkte);
        // Gitter: (i, j) mit Anteil i von b und j von c
        for j in 0..=n {
            for i in 0..=n - j {
                let k = n - i - j;
                let id = match (i, j, k) {
                    (0, 0, _) => d[0],
                    (_, 0, 0) => d[1],
                    (0, _, 0) => d[2],
                    (_, 0, _) => auf(&ab, d[0], d[1], i),
                    (_, _, 0) => auf(&bc, d[1], d[2], j),
                    (0, _, _) => auf(&ca, d[2], d[0], k),
                    _ => {
                        let (u, v, w) = (k as f64 / nf, i as f64 / nf, j as f64 / nf);
                        let z = za * u * u * u
                            + zb * v * v * v
                            + zc * w * w * w
                            + 3.0 * (cab * u * u * v + cba * u * v * v)
                            + 3.0 * (cbc * v * v * w + ccb * v * w * w)
                            + 3.0 * (cca * w * w * u + cac * w * u * u)
                            + 6.0 * mitte * u * v * w;
                        let x = a[0] * u + b[0] * v + c[0] * w;
                        let y = a[1] * u + b[1] * v + c[1] * w;
                        punkte.push([x, y, z]);
                        (punkte.len() - 1) as u32
                    }
                };
                gitter[stelle(i, j)] = id;
            }
        }
        for j in 0..n {
            for i in 0..n - j {
                let g = |i: usize, j: usize| gitter[stelle(i, j)];
                dreiecke.push([g(i, j), g(i + 1, j), g(i, j + 1)]);
                if i + j + 1 < n {
                    dreiecke.push([g(i + 1, j), g(i + 1, j + 1), g(i, j + 1)]);
                }
            }
        }
    }
    // Bruchkanten in ihren Stücken
    let mut bruch_neu = Vec::with_capacity(netz.bruch.len() * n);
    for b in &netz.bruch {
        let (lo, hi) = kante(b[0], b[1]);
        match kanten.get(&(lo, hi)) {
            Some(v) => {
                let kette: Vec<u32> = std::iter::once(lo)
                    .chain(v.iter().copied())
                    .chain(std::iter::once(hi))
                    .collect();
                bruch_neu.extend(kette.windows(2).map(|w| [w[0].min(w[1]), w[0].max(w[1])]));
            }
            None => bruch_neu.push([lo, hi]),
        }
    }
    Glatt {
        netz: Netz {
            punkte,
            dreiecke,
            bruch: bruch_neu,
            aussen: netz.aussen.clone(),
            hinweise: netz.hinweise.clone(),
        },
        je: n * n,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Netz über einem Raster `k` × `k` im Abstand 10 m mit Höhe `h(x, y)`.
    fn raster(k: u32, h: impl Fn(f64, f64) -> f64) -> Netz {
        let mut punkte = Vec::new();
        for j in 0..k {
            for i in 0..k {
                let (x, y) = (f64::from(i) * 10.0, f64::from(j) * 10.0);
                punkte.push([x, y, h(x, y)]);
            }
        }
        let mut dreiecke = Vec::new();
        for j in 0..k - 1 {
            for i in 0..k - 1 {
                let a = j * k + i;
                dreiecke.push([a, a + 1, a + k + 1]);
                dreiecke.push([a, a + k + 1, a + k]);
            }
        }
        Netz {
            punkte,
            dreiecke,
            ..Netz::default()
        }
    }

    /// Kanten, die nur ein Dreieck haben, und Kanten mit mehr als zwei.
    fn offen(n: &Netz) -> (usize, usize) {
        let mut z: HashMap<(u32, u32), usize> = HashMap::new();
        for t in &n.dreiecke {
            for k in 0..3 {
                *z.entry(kante(t[k], t[(k + 1) % 3])).or_default() += 1;
            }
        }
        (
            z.values().filter(|&&c| c == 1).count(),
            z.values().filter(|&&c| c > 2).count(),
        )
    }

    fn flaeche(n: &Netz) -> f64 {
        n.dreiecke
            .iter()
            .map(|t| {
                let [a, b, c] = t.map(|i| n.punkte[i as usize]);
                ((b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0])) / 2.0
            })
            .sum()
    }

    /// Eine schiefe Ebene bleibt eben; jedes Dreieck wird zu `teile`²,
    /// gleich ausgerichtet, ohne Lücke und mit derselben Fläche.
    #[test]
    fn ebene_bleibt_eben() {
        let netz = raster(5, |x, y| 100.0 + 0.1 * x - 0.05 * y);
        for teile in [2, 4] {
            let g = glaetten(&netz, teile, &[]);
            assert_eq!(g.je, (teile * teile) as usize);
            assert_eq!(g.netz.dreiecke.len(), netz.dreiecke.len() * g.je);
            assert_eq!(&g.netz.punkte[..netz.punkte.len()], &netz.punkte[..]);
            for q in &g.netz.punkte {
                assert!(
                    (q[2] - (100.0 + 0.1 * q[0] - 0.05 * q[1])).abs() < 1e-9,
                    "{q:?}"
                );
            }
            assert!((flaeche(&g.netz) - flaeche(&netz)).abs() < 1e-6);
            assert!(g.netz.dreiecke.iter().all(|t| {
                let [a, b, c] = t.map(|i| g.netz.punkte[i as usize]);
                (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0]) > 0.0
            }));
            // nur der Außenrand ist offen, 4 Seiten zu 4 Kanten je `teile`
            assert_eq!(offen(&g.netz), (16 * teile as usize, 0));
        }
    }

    /// Eine Kuppe wird rund: Die Punkte des Aufmaßes bleiben, die neuen
    /// liegen nicht mehr auf den ebenen Dreiecken, das Netz bleibt dicht.
    #[test]
    fn kuppe_wird_rund() {
        let h = |x: f64, y: f64| {
            100.0 + 5.0 * (-((x - 20.0).powi(2) + (y - 20.0).powi(2)) / 300.0).exp()
        };
        let netz = raster(5, h);
        let g = glaetten(&netz, 4, &[]);
        assert_eq!(&g.netz.punkte[..25], &netz.punkte[..]);
        assert_eq!(offen(&g.netz), (64, 0));
        // näher an der echten Kuppe als die ebenen Dreiecke
        let fehler = |n: &Netz, glatt: bool| -> f64 {
            n.punkte[25..]
                .iter()
                .map(|q| {
                    let z = if glatt {
                        q[2]
                    } else {
                        linear(&netz, q[0], q[1])
                    };
                    (z - h(q[0], q[1])).powi(2)
                })
                .sum::<f64>()
        };
        assert!(fehler(&g.netz, true) < 0.5 * fehler(&g.netz, false));
    }

    /// Höhe des Netzes `n` bei (x, y), linear im Dreieck.
    fn linear(n: &Netz, x: f64, y: f64) -> f64 {
        for t in &n.dreiecke {
            let [a, b, c] = t.map(|i| n.punkte[i as usize]);
            let d = (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0]);
            let v = ((x - a[0]) * (c[1] - a[1]) - (y - a[1]) * (c[0] - a[0])) / d;
            let w = ((b[0] - a[0]) * (y - a[1]) - (b[1] - a[1]) * (x - a[0])) / d;
            let u = 1.0 - v - w;
            if u > -1e-9 && v > -1e-9 && w > -1e-9 {
                return a[2] * u + b[2] * v + c[2] * w;
            }
        }
        f64::NAN
    }

    /// Eine ebene Platte neben einem Hang, eine Bruchkante und feste
    /// Dreiecke bleiben gerade: Die neuen Punkte darauf liegen auf der
    /// Geraden bzw. Ebene.
    #[test]
    fn knicke_bleiben() {
        // links eben auf 100, rechts ab x = 20 ansteigend
        let netz = raster(5, |x, _| {
            100.0 + if x > 20.0 { (x - 20.0) * 0.2 } else { 0.0 }
        });
        let g = glaetten(&netz, 4, &[]);
        for q in &g.netz.punkte {
            if q[0] <= 20.0 + 1e-9 {
                assert!((q[2] - 100.0).abs() < 1e-9, "Platte eben: {q:?}");
            }
        }
        // feste Dreiecke: ihre Punkte liegen auf ihnen
        let h = |x: f64, y: f64| 100.0 + 3.0 * ((x / 13.0).sin() + (y / 17.0).cos());
        let netz = raster(5, h);
        let fest: Vec<bool> = (0..netz.dreiecke.len()).map(|i| i % 3 == 0).collect();
        let g = glaetten(&netz, 2, &fest);
        for (i, t) in netz.dreiecke.iter().enumerate().filter(|(i, _)| fest[*i]) {
            for s in &g.netz.dreiecke[i * g.je..(i + 1) * g.je] {
                for &k in s {
                    let q = g.netz.punkte[k as usize];
                    let mini = Netz {
                        punkte: netz.punkte.clone(),
                        dreiecke: vec![*t],
                        ..Netz::default()
                    };
                    assert!((q[2] - linear(&mini, q[0], q[1])).abs() < 1e-9);
                }
            }
        }
        // Bruchkante: in Stücke geteilt, gerade
        let mut netz = raster(3, h);
        netz.bruch = vec![[1, 4]];
        let g = glaetten(&netz, 4, &[]);
        assert_eq!(g.netz.bruch.len(), 4);
        let (a, b) = (netz.punkte[1], netz.punkte[4]);
        for s in &g.netz.bruch {
            for &k in s {
                let q = g.netz.punkte[k as usize];
                let t = (q[1] - a[1]) / (b[1] - a[1]);
                assert!((q[2] - (a[2] + (b[2] - a[2]) * t)).abs() < 1e-9);
            }
        }
    }

    #[test]
    fn stufen_und_woerter() {
        let t: Vec<u32> = Glaettung::ALLE.iter().map(|g| g.teile()).collect();
        assert_eq!(t, [1, 2, 4]);
        let k: Vec<&str> = Glaettung::ALLE.iter().map(|g| g.key()).collect();
        assert_eq!(k, ["aus", "fein", "sehrfein"]);
    }

    /// Unverändert bei `teile` 1.
    #[test]
    fn eins_ist_das_netz() {
        let netz = raster(3, |x, y| x + y);
        let g = glaetten(&netz, 1, &[]);
        assert_eq!(g.je, 1);
        assert_eq!(g.netz, netz);
    }
}
