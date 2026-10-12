//! Höhenlinien aus dem Netz: je Höhe ein Durchgang durch die Dreiecke, die
//! Stücke zu Linien verkettet. Eine Ecke genau auf der Höhe zählt als
//! darüber; so schneidet jede Höhe ein Dreieck in keiner oder genau einer
//! Strecke, und benachbarte Stücke teilen sich ihre Netzkante.

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

/// Höhen von `stufe`-Vielfachen zwischen tiefstem und höchstem Punkt.
pub fn hoehen(netz: &Netz, stufe: f64) -> Vec<f64> {
    if stufe.is_nan() || stufe <= 0.0 {
        return Vec::new();
    }
    let mut it = netz
        .dreiecke
        .iter()
        .flatten()
        .map(|&i| netz.punkte[i as usize][2]);
    let Some(z0) = it.next() else {
        return Vec::new();
    };
    let (lo, hi) = it.fold((z0, z0), |(lo, hi), z| (lo.min(z), hi.max(z)));
    let (k0, k1) = ((lo / stufe).floor() as i64, (hi / stufe).ceil() as i64);
    (k0..=k1).map(|k| k as f64 * stufe).collect()
}

/// Stücke auf Höhe `z`, gerichtet mit dem höheren Gelände links.
fn stuecke(netz: &Netz, z: f64) -> Vec<Stueck> {
    let mut v = Vec::new();
    for t in &netz.dreiecke {
        let h = t.map(|i| netz.punkte[i as usize][2] - z);
        let unter = h.map(|d| d < 0.0);
        let n = unter.iter().filter(|&&u| u).count();
        if n == 0 || n == 3 {
            continue;
        }
        // die allein liegende Ecke k: Kanten (k, k+1) und (k+2, k)
        let k = (0..3)
            .find(|&k| unter[k] != unter[(k + 1) % 3] && unter[k] != unter[(k + 2) % 3])
            .unwrap_or(0);
        let (a, b, c) = (t[k], t[(k + 1) % 3], t[(k + 2) % 3]);
        // gegen den Uhrzeigersinn: liegt die Ecke unten, ist links von
        // (k+2,k) → (k,k+1) das Höhere
        let s = if unter[k] {
            Stueck {
                von: kante(c, a),
                nach: kante(a, b),
            }
        } else {
            Stueck {
                von: kante(a, b),
                nach: kante(c, a),
            }
        };
        v.push(s);
    }
    v
}

/// Punkt auf der Netzkante `e` in Höhe `z`.
fn punkt(netz: &Netz, e: (u32, u32), z: f64) -> [f64; 2] {
    let (p, q) = (netz.punkte[e.0 as usize], netz.punkte[e.1 as usize]);
    let s = (z - p[2]) / (q[2] - p[2]);
    [p[0] + s * (q[0] - p[0]), p[1] + s * (q[1] - p[1])]
}

/// Anzahl der Stücke (je Dreieck und Höhe eins) bei Abstand `stufe`.
pub fn strecken(netz: &Netz, stufe: f64) -> usize {
    hoehen(netz, stufe)
        .into_iter()
        .map(|z| stuecke(netz, z).len())
        .sum()
}

/// Höhenlinien im Abstand `stufe` (m), verkettet: erst die offenen Linien
/// von Rand zu Rand, dann die geschlossenen.
pub fn hoehenlinien(netz: &Netz, stufe: f64) -> Vec<Hoehenlinie> {
    let mut linien = Vec::new();
    for z in hoehen(netz, stufe) {
        let st = stuecke(netz, z);
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
        assert_eq!(strecken(&n, 1.0), 60);
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
