//! Platten nahe beieinander trennen (H-243a, VOB; Geometrie und VOB zu
//! #245): Jede Sohle samt Überstand liegt ganz auf der Höhe ihres Hauses,
//! der Übergang zum Nachbarn liegt dazwischen, Wand an Wand unter der
//! gemeinsamen Wand. Reicht die Sohle des einen Hauses weiter als die des
//! anderen (Garage kürzer als die Hauswand), hat das längere Vorrang: Die
//! Platte des kürzeren endet in der Flucht seiner Sohle (VOB-Spruch c zu
//! H-245a). Gedrehte Häuser gleicher Richtung trennt es in ihrer Richtung
//! (B-245g, [`Dreh`]). Überdecken sich die Rechtecke um die Wandzüge auch
//! dann (Garage in der Kehle eines L, Nachbarn in verschiedener Richtung)
//! oder reichen die Sohlen übereinander (Wände ineinander), bleibt es wie
//! auf main seit #243: Die frühere Platte gilt, die spätere verliert ihre
//! Ecken darin und ihren Rand als Bruchkante (Geometrie H-245-geo-c,
//! B-245e: nirgends schlechter als main).

use std::f64::consts::FRAC_PI_2;

/// Eine Platte vor dem Trennen (mm, `[x0, y0, x1, y1]`).
#[derive(Clone, Copy, Debug)]
pub(super) struct Platte {
    /// Rechteck der Platte: Wandzüge um den Plattenabstand vergrößert.
    pub rect: [f64; 4],
    /// Rechteck um die Wandzüge.
    pub wand: [f64; 4],
    /// Rechteck um die Sohle samt Überstand.
    pub sohle: [f64; 4],
    /// OK Gelände (mm zu ±0,00).
    pub z: f64,
}

/// Was von einer Platte bleibt: ihr Rechteck ohne die Löcher.
#[derive(Clone, Debug)]
pub(super) struct Bereich {
    pub rect: [f64; 4],
    pub loecher: Vec<[f64; 4]>,
    pub z: f64,
    /// Der Rand wird Bruchkante; nicht, wenn sich die Rechtecke um die
    /// Wandzüge mit denen eines früheren Hauses überdecken.
    pub rand: bool,
    /// Rechtecke früherer Platten, mit denen sich diese überdeckt: Ecken
    /// darin fallen weg, dort gilt die frühere (wie auf main seit #243).
    pub verdeckt: Vec<[f64; 4]>,
    /// Stelle der Platte in der Eingabe von [`trennen`].
    pub platte: usize,
    /// In diesem System liegen `rect`, `loecher` und `verdeckt`.
    pub dreh: Dreh,
}

/// Das System, in dem getrennt wird (B-245g): achsparallel (`None`) oder
/// um θ gedreht (`[sin θ, cos θ]`). Die Rechtecke um gedrehte Häuser
/// überdeckten sich achsparallel und rechneten gemeinsam; in ihrer eigenen
/// Richtung liegen sie wie achsparallele.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct Dreh(Option<[f64; 2]>);

impl Dreh {
    /// Um `theta` (rad) gedreht; bei genau 0 achsparallel ohne Rechnung.
    pub fn um(theta: f64) -> Self {
        if theta == 0.0 {
            Dreh(None)
        } else {
            let (s, c) = theta.sin_cos();
            Dreh(Some([s, c]))
        }
    }

    /// Vom Grundriss ins System.
    pub fn hin(self, p: [f64; 2]) -> [f64; 2] {
        match self.0 {
            None => p,
            Some([s, c]) => [p[0] * c + p[1] * s, p[1] * c - p[0] * s],
        }
    }

    /// Vom System in den Grundriss.
    pub fn zurueck(self, p: [f64; 2]) -> [f64; 2] {
        match self.0 {
            None => p,
            Some([s, c]) => [p[0] * c - p[1] * s, p[0] * s + p[1] * c],
        }
    }
}

/// Unter diesem Winkel (rad) laufen zwei Wände gleich (bis auf 90°): 1 mm
/// auf 1 m.
pub(super) const RICHTUNG: f64 = 1e-3;

/// Abstand der Richtungen `a` und `b` (rad) bis auf Vielfache von 90°.
pub(super) fn richtung_ab(a: f64, b: f64) -> f64 {
    let d = (a - b).rem_euclid(FRAC_PI_2);
    d.min(FRAC_PI_2 - d)
}

/// Die Richtung eines Hauses aus seinen Wänden `strecken` (mm): die der
/// längsten, in [0, 90°); nahe 0 oder 90° genau 0, ohne Wand länger als
/// [`GLEICH`] auch. Jede Richtung deckt alle Wände (das Rechteck umfasst
/// alle Ecken im System); sie bestimmt nur, wie eng.
pub(super) fn richtung(strecken: &[[[f64; 2]; 2]]) -> f64 {
    let laenge = |s: &[[f64; 2]; 2]| (s[1][0] - s[0][0]).hypot(s[1][1] - s[0][1]);
    let Some(lang) = strecken
        .iter()
        .filter(|s| laenge(s) > GLEICH)
        .max_by(|a, b| laenge(a).total_cmp(&laenge(b)))
    else {
        return 0.0;
    };
    let phi = (lang[1][1] - lang[0][1])
        .atan2(lang[1][0] - lang[0][0])
        .rem_euclid(FRAC_PI_2);
    if richtung_ab(phi, 0.0) <= RICHTUNG {
        0.0
    } else {
        phi
    }
}

/// Unter diesem Unterschied (mm) reichen zwei Sohlen gleich weit.
const GLEICH: f64 = 1.0;

/// Trennt die Platten mit Streifen von 2 × `fuge` zwischen ihnen. Die
/// Richtung ist die, in der die Wandzüge am weitesten auseinander liegen;
/// getrennt wird in der Mitte zwischen den Sohlen. Überdecken sich die
/// Rechtecke um die Wandzüge oder um die Sohlen, gilt die frühere Platte.
pub(super) fn trennen(platten: &[Platte], fuge: f64) -> Vec<Bereich> {
    let n = platten.len();
    let mut rect: Vec<[f64; 4]> = platten.iter().map(|p| p.rect).collect();
    // (k, i, Richtung, Mitte, k liegt vorn)
    let mut paare: Vec<(usize, usize, usize, f64, bool)> = Vec::new();
    let mut ohne_rand = vec![false; n];
    let mut verdeckt: Vec<Vec<[f64; 4]>> = vec![Vec::new(); n];
    for i in 0..n {
        for k in 0..i {
            let (a, b) = (platten[k].rect, platten[i].rect);
            if a[0] > b[2] || b[0] > a[2] || a[1] > b[3] || b[1] > a[3] {
                continue;
            }
            let (wk, wi) = (platten[k].wand, platten[i].wand);
            let spalt = |d: usize| {
                if wk[d + 2] <= wi[d] {
                    Some((wi[d] - wk[d + 2], true))
                } else if wi[d + 2] <= wk[d] {
                    Some((wk[d] - wi[d + 2], false))
                } else {
                    None
                }
            };
            let (d, k_vorn) = match (spalt(0), spalt(1)) {
                (Some(x), Some(y)) if y.0 > x.0 => (1, y.1),
                (Some(x), _) => (0, x.1),
                (None, Some(y)) => (1, y.1),
                (None, None) => {
                    ohne_rand[i] = true;
                    verdeckt[i].push(platten[k].rect);
                    continue;
                }
            };
            let (sk, si) = (platten[k].sohle, platten[i].sohle);
            // Reichen die Sohlen in Richtung d übereinander (gedrehte
            // Häuser, Wände ineinander), schnitte die Mitte durch eine
            // Sohle: dann wie überdeckt
            let ueber = if k_vorn {
                sk[d + 2] - si[d]
            } else {
                si[d + 2] - sk[d]
            };
            if ueber > GLEICH {
                ohne_rand[i] = true;
                verdeckt[i].push(platten[k].rect);
                continue;
            }
            let mitte = if k_vorn {
                (sk[d + 2] + si[d]) / 2.0
            } else {
                (si[d + 2] + sk[d]) / 2.0
            };
            // quer dazu: Wo eine Sohle weiter reicht, endet die Platte der
            // anderen in der Flucht ihrer Sohle
            let e = 1 - d;
            if sk[e] < si[e] - GLEICH {
                rect[i][e] = rect[i][e].max(si[e] - fuge);
            } else if si[e] < sk[e] - GLEICH {
                rect[k][e] = rect[k][e].max(sk[e] - fuge);
            }
            if sk[e + 2] > si[e + 2] + GLEICH {
                rect[i][e + 2] = rect[i][e + 2].min(si[e + 2] + fuge);
            } else if si[e + 2] > sk[e + 2] + GLEICH {
                rect[k][e + 2] = rect[k][e + 2].min(sk[e + 2] + fuge);
            }
            paare.push((k, i, d, mitte, k_vorn));
        }
    }
    let mut loecher: Vec<Vec<[f64; 4]>> = vec![Vec::new(); n];
    let inf = f64::INFINITY;
    for &(k, i, d, mitte, k_vorn) in &paare {
        let e = 1 - d;
        // was jenseits der Mitte liegt, so weit quer wie die andere Platte
        for (x, y, vorn) in [(k, i, k_vorn), (i, k, !k_vorn)] {
            let mut l = [0.0; 4];
            (l[d], l[d + 2]) = if vorn {
                (mitte - fuge, inf)
            } else {
                (-inf, mitte + fuge)
            };
            (l[e], l[e + 2]) = (rect[y][e] - 2.0 * fuge, rect[y][e + 2] + 2.0 * fuge);
            loecher[x].push(l);
        }
    }
    (0..n)
        .filter(|&i| rect[i][0] < rect[i][2] && rect[i][1] < rect[i][3])
        .map(|i| Bereich {
            rect: rect[i],
            loecher: std::mem::take(&mut loecher[i]),
            z: platten[i].z,
            rand: !ohne_rand[i],
            verdeckt: std::mem::take(&mut verdeckt[i]),
            platte: i,
            dreh: Dreh::default(),
        })
        .collect()
}

impl Bereich {
    /// Liegt `p` im Bereich oder auf seinem Rand?
    pub fn enthaelt(&self, p: [f64; 2]) -> bool {
        let r = self.rect;
        (r[0]..=r[2]).contains(&p[0])
            && (r[1]..=r[3]).contains(&p[1])
            && !self
                .loecher
                .iter()
                .any(|l| l[0] < p[0] && p[0] < l[2] && l[1] < p[1] && p[1] < l[3])
    }

    /// Die Ränder des Bereichs als geschlossene, achsparallele Umläufe,
    /// ohne Ecken auf geraden Stücken; ohne Löcher das Rechteck.
    pub fn umlaeufe(&self) -> Vec<Vec<[f64; 2]>> {
        let r = self.rect;
        let mut xs = vec![r[0], r[2]];
        let mut ys = vec![r[1], r[3]];
        for l in &self.loecher {
            xs.extend([l[0], l[2]].into_iter().filter(|&x| r[0] < x && x < r[2]));
            ys.extend([l[1], l[3]].into_iter().filter(|&y| r[1] < y && y < r[3]));
        }
        for v in [&mut xs, &mut ys] {
            v.sort_by(f64::total_cmp);
            v.dedup();
        }
        let (nx, ny) = (xs.len() - 1, ys.len() - 1);
        let eigen = |a: usize, b: usize| {
            let p = [(xs[a] + xs[a + 1]) / 2.0, (ys[b] + ys[b + 1]) / 2.0];
            self.enthaelt(p)
        };
        let besetzt: Vec<Vec<bool>> = (0..nx)
            .map(|a| (0..ny).map(|b| eigen(a, b)).collect())
            .collect();
        let frei = |a: isize, b: isize| {
            a < 0
                || b < 0
                || a >= nx as isize
                || b >= ny as isize
                || !besetzt[a as usize][b as usize]
        };
        // Randkanten gegen den Uhrzeigersinn um das Besetzte, in Gitterzahlen
        let mut kanten: Vec<([usize; 2], [usize; 2])> = Vec::new();
        for (a, spalte) in besetzt.iter().enumerate() {
            for (b, &drin) in spalte.iter().enumerate() {
                if !drin {
                    continue;
                }
                let (ai, bi) = (a as isize, b as isize);
                if frei(ai, bi - 1) {
                    kanten.push(([a, b], [a + 1, b]));
                }
                if frei(ai + 1, bi) {
                    kanten.push(([a + 1, b], [a + 1, b + 1]));
                }
                if frei(ai, bi + 1) {
                    kanten.push(([a + 1, b + 1], [a, b + 1]));
                }
                if frei(ai - 1, bi) {
                    kanten.push(([a, b + 1], [a, b]));
                }
            }
        }
        let mut offen = vec![true; kanten.len()];
        let mut out = Vec::new();
        for s in 0..kanten.len() {
            if !offen[s] {
                continue;
            }
            let mut umlauf: Vec<[usize; 2]> = Vec::new();
            let mut j = s;
            loop {
                offen[j] = false;
                let (von, nach) = kanten[j];
                umlauf.push(von);
                let r = [
                    nach[0] as isize - von[0] as isize,
                    nach[1] as isize - von[1] as isize,
                ];
                // weiter mit der Kante, die am stärksten links abbiegt
                let naechste = (0..kanten.len())
                    .filter(|&x| offen[x] && kanten[x].0 == nach)
                    .max_by_key(|&x| {
                        let (p, q) = kanten[x];
                        let v = [q[0] as isize - p[0] as isize, q[1] as isize - p[1] as isize];
                        let kreuz = r[0] * v[1] - r[1] * v[0];
                        let skalar = r[0] * v[0] + r[1] * v[1];
                        // links vor geradeaus vor rechts
                        if kreuz > 0 {
                            2
                        } else if skalar > 0 {
                            1
                        } else {
                            0
                        }
                    });
                match naechste {
                    Some(x) => j = x,
                    None => break,
                }
            }
            // Ecken auf geraden Stücken weglassen
            let m = umlauf.len();
            let ecken: Vec<[f64; 2]> = (0..m)
                .filter(|&q| {
                    let (a, b, c) = (umlauf[(q + m - 1) % m], umlauf[q], umlauf[(q + 1) % m]);
                    !((a[0] == b[0] && b[0] == c[0]) || (a[1] == b[1] && b[1] == c[1]))
                })
                .map(|q| [xs[umlauf[q][0]], ys[umlauf[q][1]]])
                .collect();
            if ecken.len() >= 3 {
                out.push(ecken);
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Die Richtung eines Hauses ist die seiner längsten Wand bis auf 90°,
    /// nahe 0 genau 0; hin und zurück ist dieselbe Stelle, achsparallel
    /// ohne Rechnung (B-245g).
    #[test]
    fn richtung_und_dreh() {
        let wand = |grad: f64, l: f64| {
            let (s, c) = grad.to_radians().sin_cos();
            [[100.0, 200.0], [100.0 + l * c, 200.0 + l * s]]
        };
        for grad in [0.0, 90.0, 180.0, -90.0, 0.03, 89.97] {
            assert_eq!(richtung(&[wand(grad, 10000.0)]), 0.0, "{grad}°");
        }
        for (grad, soll) in [
            (30.0_f64, 30.0_f64),
            (120.0, 30.0),
            (-60.0, 30.0),
            (45.0, 45.0),
        ] {
            let r = richtung(&[wand(5.0, 3000.0), wand(grad, 10000.0)]);
            assert!((r - soll.to_radians()).abs() < 1e-9, "{grad}°: {r}");
        }
        assert_eq!(richtung(&[wand(30.0, 0.5)]), 0.0);
        let p = [12345.6, -987.0];
        assert_eq!(Dreh::um(0.0).hin(p), p);
        assert_eq!(Dreh::um(0.0).zurueck(p), p);
        let d = Dreh::um(30.0_f64.to_radians());
        let q = d.zurueck(d.hin(p));
        assert!((q[0] - p[0]).abs() < 1e-9 && (q[1] - p[1]).abs() < 1e-9);
        // eine Wand in Richtung 30° liegt im System waagrecht
        let w = wand(30.0, 10000.0);
        let (a, b) = (d.hin(w[0]), d.hin(w[1]));
        assert!((a[1] - b[1]).abs() < 1e-9);
    }

    fn platte(w: [f64; 4], z: f64) -> Platte {
        let u = 315.0;
        let m = 3000.0;
        Platte {
            rect: [w[0] - m, w[1] - m, w[2] + m, w[3] + m],
            wand: w,
            sohle: [w[0] - u, w[1] - u, w[2] + u, w[3] + u],
            z,
        }
    }

    /// Allein bleibt die Platte ein Rechteck.
    #[test]
    fn allein() {
        let b = trennen(&[platte([0.0, 0.0, 8000.0, 10000.0], 0.0)], 10.0);
        assert_eq!(b.len(), 1);
        assert_eq!(
            b[0].umlaeufe(),
            vec![vec![
                [-3000.0, -3000.0],
                [11000.0, -3000.0],
                [11000.0, 13000.0],
                [-3000.0, 13000.0]
            ]]
        );
    }

    /// Doppelhaus Wand an Wand: getrennt unter der gemeinsamen Wand. Stehen
    /// die Wände ineinander, reichen die Sohlen übereinander: Dann gilt die
    /// frühere Platte, die spätere bleibt ohne Rand (wie main).
    #[test]
    fn doppelhaus() {
        let b = trennen(
            &[
                platte([0.0, 0.0, 8000.0, 10000.0], -500.0),
                platte([8630.0, 0.0, 16630.0, 10000.0], -1500.0),
            ],
            10.0,
        );
        assert_eq!(b[0].umlaeufe()[0][1], [8305.0, -3000.0]);
        assert_eq!(b[1].umlaeufe()[0][0], [8325.0, -3000.0]);
        assert_eq!(b[0].umlaeufe().len(), 1);
        assert_eq!(b[0].umlaeufe()[0].len(), 4);
        assert!(b[0].rand && b[1].rand);
        let a = platte([0.0, 0.0, 8000.0, 10000.0], -500.0);
        let b = trennen(&[a, platte([8000.0, 0.0, 16000.0, 10000.0], -1500.0)], 10.0);
        assert!(b[0].rand && !b[1].rand);
        assert_eq!(b[1].verdeckt, vec![a.rect]);
        assert!(b[0].verdeckt.is_empty());
    }

    /// Garage, kürzer als die Hauswand, 1 m daneben: Ihre Platte endet in
    /// der Flucht ihrer Sohle, die des Hauses umgreift sie mit einer
    /// Kerbe; beide Sohlen liegen ganz in ihrem Bereich, keiner in dem des
    /// anderen.
    #[test]
    fn garage() {
        let haus = platte([0.0, 0.0, 8000.0, 10000.0], -500.0);
        let garage = platte([9000.0, 0.0, 12000.0, 3000.0], -1500.0);
        let b = trennen(&[haus, garage], 10.0);
        // Garage: quer bis zur Flucht ihrer Sohle (3,315 m) + Fuge
        assert_eq!(b[1].rect[3], 3325.0);
        let u = &b[0].umlaeufe();
        assert_eq!(u.len(), 1);
        assert_eq!(u[0].len(), 6, "{u:?}");
        for (p, s) in [(&b[0], haus.sohle), (&b[1], garage.sohle)] {
            for x in [s[0], s[2]] {
                for y in [s[1], s[3]] {
                    assert!(p.enthaelt([x, y]));
                }
            }
        }
        for s in [haus.sohle, garage.sohle] {
            let drin = b
                .iter()
                .filter(|p| p.enthaelt([(s[0] + s[2]) / 2.0, s[1] + 1.0]))
                .count();
            assert_eq!(drin, 1);
        }
        // hinter der Garage gilt das Haus
        assert!(b[0].enthaelt([8500.0, 5000.0]));
        assert!(!b[1].enthaelt([8500.0, 5000.0]));
    }

    /// Ein Haus um eine Garage herum: Die Wandzüge überdecken sich, die
    /// frühere Platte gilt und behält ihren Rand, die spätere verliert ihn;
    /// ein drittes Haus weit weg behält seinen.
    #[test]
    fn umschlossen() {
        let b = trennen(
            &[
                platte([0.0, 0.0, 20000.0, 20000.0], 0.0),
                platte([5000.0, 5000.0, 8000.0, 8000.0], -1000.0),
                platte([40000.0, 0.0, 48000.0, 10000.0], 500.0),
            ],
            10.0,
        );
        assert!(b.iter().all(|x| x.loecher.is_empty()));
        assert_eq!(
            b.iter().map(|x| x.rand).collect::<Vec<_>>(),
            [true, false, true]
        );
        assert_eq!(b[1].verdeckt, [b[0].rect]);
        assert!(b[0].verdeckt.is_empty() && b[2].verdeckt.is_empty());
    }
}
