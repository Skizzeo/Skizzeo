//! Höhenlinien aus dem Netz: ein Durchgang durch die Dreiecke legt jedes
//! Stück zu seiner Höhe, je Höhe werden die Stücke zu Linien verkettet. Eine
//! Ecke genau auf der Höhe zählt als darüber; so schneidet jede Höhe ein
//! Dreieck in keiner oder genau einer Strecke, und benachbarte Stücke teilen
//! sich ihre Netzkante.

use crate::netz::Netz;

/// Eine Höhenlinie; höheres Gelände liegt links in Laufrichtung.
#[derive(Clone, Debug, PartialEq)]
pub struct Hoehenlinie {
    pub z: f64,
    pub punkte: Vec<[f64; 2]>,
    pub geschlossen: bool,
}

/// Stück einer Höhenlinie in einem Dreieck: von der Kante `von` zur Kante
/// `nach` (je kleinere Punktnummer zuerst).
#[derive(Clone, Copy, Debug)]
struct Stueck {
    von: (u32, u32),
    nach: (u32, u32),
}

fn kante(a: u32, b: u32) -> (u32, u32) {
    (a.min(b), a.max(b))
}

/// Kleinster Abstand der Höhenlinien (m); feinere Wünsche gelten als dieser.
pub const MIN_STUFE: f64 = 0.01;
/// Höchstens so viele Höhen; darüber entstehen keine Linien (unplausible
/// Höhen oder ein Kommafehler in der Datei).
pub const MAX_STUFEN: usize = 2000;
/// Höchstens so viele Stücke (je Dreieck und Höhe eins); darüber entstehen
/// keine Linien. Die Grenze der Höhen begrenzt die Arbeit nicht: 100.000
/// Punkte im Abstand 0,25 m wären 14 Mio. Stücke (Review #121, Auflage H1).
pub const MAX_STRECKEN: usize = 1_000_000;

/// Die Höhen `k · stufe` für `k` von `k0` bis `k1`.
#[derive(Clone, Copy, Debug)]
struct Stufen {
    k0: i64,
    k1: i64,
    stufe: f64,
}

impl Stufen {
    fn z(&self, k: i64) -> f64 {
        k as f64 * self.stufe
    }

    /// Die `k`, deren Höhe das Dreieck `t` schneidet: tiefste Ecke darunter,
    /// höchste auf oder über ihr (eine Ecke genau auf der Höhe zählt als
    /// darüber). In O(1) je Dreieck, genau wie [`stueck`] es entscheidet.
    fn bereich(&self, netz: &Netz, t: &[u32; 3]) -> Option<(i64, i64)> {
        let h = t.map(|i| netz.punkte[i as usize][2]);
        let lo = h[0].min(h[1]).min(h[2]);
        let hi = h[0].max(h[1]).max(h[2]);
        // erste Höhe über `lo`, letzte auf oder unter `hi`; die Schätzung
        // liegt höchstens eins daneben
        let mut a = ((lo / self.stufe).floor() as i64).clamp(self.k0, self.k1);
        while a > self.k0 && self.z(a - 1) > lo {
            a -= 1;
        }
        while a <= self.k1 && self.z(a) <= lo {
            a += 1;
        }
        let mut b = ((hi / self.stufe).floor() as i64).clamp(self.k0, self.k1);
        while b < self.k1 && self.z(b + 1) <= hi {
            b += 1;
        }
        while b >= self.k0 && self.z(b) > hi {
            b -= 1;
        }
        (a <= b).then_some((a, b))
    }
}

/// Stufen zwischen tiefstem und höchstem Punkt; `None` ohne Dreiecke, bei
/// einer Stufe, die keine Zahl, nicht positiv oder unendlich ist, und bei
/// mehr als [`MAX_STUFEN`].
fn stufen(netz: &Netz, stufe: f64) -> Option<Stufen> {
    if !stufe.is_finite() || stufe <= 0.0 {
        return None;
    }
    let stufe = stufe.max(MIN_STUFE);
    let mut it = netz
        .dreiecke
        .iter()
        .flatten()
        .map(|&i| netz.punkte[i as usize][2]);
    let z0 = it.next()?;
    let (lo, hi) = it.fold((z0, z0), |(lo, hi), z| (lo.min(z), hi.max(z)));
    let (k0, k1) = ((lo / stufe).floor(), (hi / stufe).ceil());
    // in f64 gezählt: unendliche oder riesige Höhen laufen nicht über
    let n = k1 - k0;
    if n.is_nan() || n >= MAX_STUFEN as f64 || k0.abs() > 1e15 || k1.abs() > 1e15 {
        return None;
    }
    Some(Stufen {
        k0: k0 as i64,
        k1: k1 as i64,
        stufe,
    })
}

/// Höhen von `stufe`-Vielfachen zwischen tiefstem und höchstem Punkt; leer,
/// wenn es mehr als [`MAX_STUFEN`] wären.
pub fn hoehen(netz: &Netz, stufe: f64) -> Vec<f64> {
    stufen(netz, stufe).map_or(Vec::new(), |s| (s.k0..=s.k1).map(|k| s.z(k)).collect())
}

/// Stück auf Höhe `z` im Dreieck `t`, gerichtet mit dem höheren Gelände
/// links; `None`, wenn die Höhe das Dreieck nicht schneidet.
fn stueck(netz: &Netz, t: &[u32; 3], z: f64) -> Option<Stueck> {
    let h = t.map(|i| netz.punkte[i as usize][2] - z);
    let unter = h.map(|d| d < 0.0);
    let n = unter.iter().filter(|&&u| u).count();
    if n == 0 || n == 3 {
        return None;
    }
    // die allein liegende Ecke k: Kanten (k, k+1) und (k+2, k)
    let k = (0..3)
        .find(|&k| unter[k] != unter[(k + 1) % 3] && unter[k] != unter[(k + 2) % 3])
        .unwrap_or(0);
    let (a, b, c) = (t[k], t[(k + 1) % 3], t[(k + 2) % 3]);
    // gegen den Uhrzeigersinn: liegt die Ecke unten, ist links von
    // (k+2,k) → (k,k+1) das Höhere
    Some(if unter[k] {
        Stueck {
            von: kante(c, a),
            nach: kante(a, b),
        }
    } else {
        Stueck {
            von: kante(a, b),
            nach: kante(c, a),
        }
    })
}

/// Punkt auf der Netzkante `e` in Höhe `z`.
fn punkt(netz: &Netz, e: (u32, u32), z: f64) -> [f64; 2] {
    let (p, q) = (netz.punkte[e.0 as usize], netz.punkte[e.1 as usize]);
    let s = (z - p[2]) / (q[2] - p[2]);
    [p[0] + s * (q[0] - p[0]), p[1] + s * (q[1] - p[1])]
}

/// Anzahl der Stücke (je Dreieck und Höhe eins) bei Abstand `stufe`, in
/// O(Dreiecke) gezählt, ohne ein Stück zu bauen. `None`, wenn bei diesem
/// Abstand keine Höhen entstehen (wie [`hoehen`] leer): Null Stücke heißt
/// dann nicht, dass er passt (Review #123 W45 B4).
pub fn strecken(netz: &Netz, stufe: f64) -> Option<usize> {
    stufen(netz, stufe).map(|s| zaehlen(netz, &s))
}

fn zaehlen(netz: &Netz, s: &Stufen) -> usize {
    netz.dreiecke
        .iter()
        .filter_map(|t| s.bereich(netz, t))
        .map(|(a, b)| (b - a + 1) as usize)
        .sum()
}

/// Höhenlinien im Abstand `stufe` (m), verkettet: erst die offenen Linien
/// von Rand zu Rand, dann die geschlossenen.
/// Mehr als [`MAX_STRECKEN`] Stücke geben keine Linien; das wird vorab in
/// O(Dreiecke) gezählt, danach kostet jedes Stück einmal (nicht jede Höhe
/// alle Dreiecke).
pub fn hoehenlinien(netz: &Netz, stufe: f64) -> Vec<Hoehenlinie> {
    let mut linien = Vec::new();
    let Some(s) = stufen(netz, stufe) else {
        return linien;
    };
    if zaehlen(netz, &s) > MAX_STRECKEN {
        return linien;
    }
    // je Höhe die Stücke in der Reihenfolge der Dreiecke
    let mut je: Vec<Vec<Stueck>> = vec![Vec::new(); (s.k1 - s.k0 + 1) as usize];
    for t in &netz.dreiecke {
        if let Some((a, b)) = s.bereich(netz, t) {
            for k in a..=b {
                if let Some(st) = stueck(netz, t, s.z(k)) {
                    je[(k - s.k0) as usize].push(st);
                }
            }
        }
    }
    for (i, st) in je.into_iter().enumerate() {
        let z = s.z(s.k0 + i as i64);
        let mut ab: std::collections::HashMap<(u32, u32), usize> = std::collections::HashMap::new();
        let mut an: std::collections::HashSet<(u32, u32)> = std::collections::HashSet::new();
        for (i, s) in st.iter().enumerate() {
            ab.insert(s.von, i);
            an.insert(s.nach);
        }
        let mut frei = vec![true; st.len()];
        let lauf = |i0: usize, frei: &mut Vec<bool>| {
            let mut pk = vec![punkt(netz, st[i0].von, z)];
            let mut i = i0;
            loop {
                frei[i] = false;
                pk.push(punkt(netz, st[i].nach, z));
                match ab.get(&st[i].nach) {
                    Some(&j) if frei[j] => i = j,
                    Some(&j) => {
                        return Hoehenlinie {
                            z,
                            punkte: {
                                if j == i0 {
                                    pk.pop();
                                }
                                pk
                            },
                            geschlossen: j == i0,
                        };
                    }
                    None => {
                        return Hoehenlinie {
                            z,
                            punkte: pk,
                            geschlossen: false,
                        }
                    }
                }
            }
        };
        // offene zuerst: Anfang an einer Kante, an der kein Stück endet
        let mut neu = Vec::new();
        for i in 0..st.len() {
            if frei[i] && !an.contains(&st[i].von) {
                neu.push(lauf(i, &mut frei));
            }
        }
        for i in 0..st.len() {
            if frei[i] {
                neu.push(lauf(i, &mut frei));
            }
        }
        // Stücke durch eine Ecke auf der Höhe haben die Länge null
        for mut l in neu {
            l.punkte
                .dedup_by(|a, b| (a[0] - b[0]).hypot(a[1] - b[1]) < 1e-9);
            if l.geschlossen && l.punkte.len() > 1 {
                let (a, b) = (l.punkte[0], l.punkte[l.punkte.len() - 1]);
                if (a[0] - b[0]).hypot(a[1] - b[1]) < 1e-9 {
                    l.punkte.pop();
                }
            }
            if l.punkte.len() >= 2 {
                linien.push(l);
            }
        }
    }
    linien
}

impl Hoehenlinie {
    /// Länge in m.
    pub fn laenge(&self) -> f64 {
        let mut l: f64 = self
            .punkte
            .windows(2)
            .map(|w| (w[1][0] - w[0][0]).hypot(w[1][1] - w[0][1]))
            .sum();
        if self.geschlossen && self.punkte.len() > 2 {
            let (a, b) = (self.punkte[self.punkte.len() - 1], self.punkte[0]);
            l += (b[0] - a[0]).hypot(b[1] - a[1]);
        }
        l
    }

    /// Stellen für die Beschriftung im Abstand `abstand` (m), die erste
    /// nach dem halben Abstand: Punkt und Winkel (rad) so, dass die Zahl
    /// aufrecht steht.
    pub fn stellen(&self, abstand: f64) -> Vec<([f64; 2], f64)> {
        let mut v = Vec::new();
        if abstand.is_nan() || abstand <= 0.0 || self.punkte.len() < 2 {
            return v;
        }
        let mut ziel = abstand / 2.0;
        let mut weg = 0.0;
        let n = self.punkte.len();
        let kanten = if self.geschlossen { n } else { n - 1 };
        for k in 0..kanten {
            let (a, b) = (self.punkte[k], self.punkte[(k + 1) % n]);
            let l = (b[0] - a[0]).hypot(b[1] - a[1]);
            while l > 0.0 && ziel <= weg + l {
                let s = (ziel - weg) / l;
                let mut w = (b[1] - a[1]).atan2(b[0] - a[0]);
                if w > std::f64::consts::FRAC_PI_2 {
                    w -= std::f64::consts::PI;
                } else if w <= -std::f64::consts::FRAC_PI_2 {
                    w += std::f64::consts::PI;
                }
                v.push(([a[0] + s * (b[0] - a[0]), a[1] + s * (b[1] - a[1])], w));
                ziel += abstand;
            }
            weg += l;
        }
        v
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::netz::{netz, Rand};

    #[test]
    fn grenzhoehen_geben_keine_linien() {
        // Kommafehler: ein Punkt 1e30 m hoch, oder eine winzige Stufe
        let mut p = vec![[0.0, 0.0, 24.345], [10.0, 0.0, 24.5], [0.0, 10.0, 24.0]];
        let n = netz(&p, &[], Rand::Huelle);
        assert_eq!(hoehen(&n, 0.25), [24.0, 24.25, 24.5]);
        assert_eq!(hoehen(&n, 1e-12), hoehen(&n, MIN_STUFE));
        p.push([10.0, 10.0, 1e30]);
        let n = netz(&p, &[], Rand::Huelle);
        assert!(hoehen(&n, 0.25).is_empty());
        assert!(hoehenlinien(&n, 0.25).is_empty());
        p[3][2] = 2434.5;
        let n = netz(&p, &[], Rand::Huelle);
        assert!(hoehen(&n, 0.25).is_empty());
        assert_eq!(hoehen(&n, 5.0).len(), 484);
        // unendliche Höhen nimmt das Netz gar nicht erst auf
        p[3][2] = f64::INFINITY;
        let n = netz(&p, &[], Rand::Huelle);
        assert_eq!(hoehen(&n, 0.25), [24.0, 24.25, 24.5]);
        // unendliche oder keine Stufe: keine Höhen (Review #121, H3)
        for stufe in [f64::INFINITY, f64::NAN, f64::NEG_INFINITY, 0.0, -1.0] {
            assert!(hoehen(&n, stufe).is_empty(), "{stufe}");
            assert!(hoehenlinien(&n, stufe).is_empty(), "{stufe}");
            assert_eq!(strecken(&n, stufe), None, "{stufe}");
        }
    }

    /// Gezählt wie gebaut: je Höhe und Dreieck ein Stück, auch mit Ecken
    /// genau auf einer Höhe und Höhen unter null.
    #[test]
    fn zaehlen_wie_einzeln() {
        let mut x: u64 = 7;
        let mut zufall = || {
            x = x
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            (x >> 33) as f64 / (1u64 << 31) as f64
        };
        for runde in 0..20 {
            let p: Vec<[f64; 3]> = (0..200)
                .map(|_| {
                    let z = (zufall() * 160.0).round() * 0.05 - 3.0;
                    [zufall() * 50.0, zufall() * 50.0, z]
                })
                .collect();
            let n = netz(&p, &[], Rand::Huelle);
            for stufe in [0.05, 0.1, 0.25, 0.3, 1.0, 0.7] {
                let einzeln: usize = hoehen(&n, stufe)
                    .into_iter()
                    .map(|z| {
                        n.dreiecke
                            .iter()
                            .filter(|t| stueck(&n, t, z).is_some())
                            .count()
                    })
                    .sum();
                assert_eq!(
                    strecken(&n, stufe),
                    Some(einzeln),
                    "Runde {runde}, Stufe {stufe}"
                );
            }
        }
    }

    /// Viele Stücke unter der Grenze der Höhen: vorab gezählt, keine Linien,
    /// ohne sie zu bauen (Review #121, Auflage H1).
    #[test]
    fn viele_stuecke_geben_keine_linien() {
        // Zickzack: jede zweite Reihe 400 m höher, 1600 Höhen je Dreieck
        let p: Vec<[f64; 3]> = (0..30)
            .flat_map(|i| (0..30).map(move |j| [j as f64, i as f64, (i % 2) as f64 * 400.0 + 0.1]))
            .collect();
        let n = netz(&p, &[], Rand::Huelle);
        assert_eq!(hoehen(&n, 0.25).len(), 1602);
        let t = std::time::Instant::now();
        let st = strecken(&n, 0.25).unwrap();
        assert!(st > MAX_STRECKEN, "{st}");
        assert!(hoehenlinien(&n, 0.25).is_empty());
        let ms = t.elapsed().as_secs_f64() * 1000.0;
        assert!(
            ms < if cfg!(debug_assertions) { 400.0 } else { 20.0 },
            "{ms:.1} ms"
        );
        // gröber geht es: 8 Höhen schneiden jedes Dreieck, je Reihenpaar
        // eine Linie von Rand zu Rand
        assert_eq!(strecken(&n, 50.0), Some(8 * n.dreiecke.len()));
        let l = hoehenlinien(&n, 50.0);
        assert_eq!(l.len(), 8 * 29);
        assert!(l
            .iter()
            .all(|x| !x.geschlossen && (x.laenge() - 29.0).abs() < 1e-6));
    }

    #[test]
    fn kegel_gibt_ringe() {
        // Spitze in der Mitte, Ring außen tiefer: geschlossene Linien
        let mut p = vec![[0.0, 0.0, 3.0]];
        for k in 0..12 {
            let w = k as f64 * std::f64::consts::TAU / 12.0;
            p.push([10.0 * w.cos(), 10.0 * w.sin(), 0.0]);
        }
        for k in 0..12 {
            let w = (k as f64 + 0.5) * std::f64::consts::TAU / 12.0;
            p.push([20.0 * w.cos(), 20.0 * w.sin(), -1.0]);
        }
        let n = netz(&p, &[], Rand::Huelle);
        let l = hoehenlinien(&n, 1.0);
        let ring: Vec<&Hoehenlinie> = l.iter().filter(|x| x.z == 1.0).collect();
        assert_eq!(ring.len(), 1);
        assert!(ring[0].geschlossen);
        assert_eq!(ring[0].punkte.len(), 12);
        // höher liegt links: gegen den Uhrzeigersinn um die Spitze
        let fl: f64 = (0..12)
            .map(|i| {
                let (a, b) = (ring[0].punkte[i], ring[0].punkte[(i + 1) % 12]);
                a[0] * b[1] - b[0] * a[1]
            })
            .sum();
        assert!(fl > 0.0);
        // Ecke genau auf der Höhe (Ring auf 0): zählt als darüber, Linie
        // läuft durch die Ecken und bleibt geschlossen
        let null: Vec<&Hoehenlinie> = l.iter().filter(|x| x.z == 0.0).collect();
        assert_eq!(null.len(), 1);
        assert!(null[0].geschlossen);
        // −1: alle Außenpunkte genau darauf, nichts liegt darunter
        assert!(l.iter().all(|x| x.z != -1.0));
        assert_eq!(hoehen(&n, 1.0), [-1.0, 0.0, 1.0, 2.0, 3.0]);
        // Stücke je Dreieck, auch die der Länge null an der Spitze (3)
        // und am Ring (0): 24 + 12 + 12 + 12
        assert_eq!(strecken(&n, 1.0), Some(60));
        assert!(l.iter().all(|x| x.z != 3.0));
    }

    #[test]
    fn schraege_ebene_offen_von_rand_zu_rand() {
        let mut p = Vec::new();
        for i in 0..5 {
            for j in 0..4 {
                let (x, y) = (i as f64 * 2.5, j as f64 * 2.0);
                p.push([x, y, 0.1 * x]);
            }
        }
        let n = netz(&p, &[], Rand::Huelle);
        let l = hoehenlinien(&n, 0.3);
        assert_eq!(l.len(), 3); // 0,3 0,6 0,9; 0 liegt auf Ecken, 1,2 über allem
        for x in &l {
            assert!(!x.geschlossen);
            assert!((x.laenge() - 6.0).abs() < 1e-9, "{x:?}");
            // höher links: Lauf nach −y
            assert!(x.punkte.last().unwrap()[1] < x.punkte[0][1]);
            let st = x.stellen(4.0);
            assert_eq!(st.len(), 2);
            assert!((st[0].1.abs() - std::f64::consts::FRAC_PI_2).abs() < 1e-9);
        }
    }
}
