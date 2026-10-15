//! Fang beim Zeichnen einer Geländefläche (Review #189 H-189a): Ecken und
//! Kanten der Außenkante des Hauses (Wandzüge mit Gründung), der anderen
//! Geländeflächen und der Grundstücksgrenzen. Eine Ecke geht vor einer
//! Kante; ohne Ziel in der Nähe rastet der Punkt auf 1 cm. Der Punkt auf
//! einer Kante wird nur auf 1 µm gerundet: auf 1 cm läge er an einer
//! schrägen Kante bis 0,7 mm in der Nachbarfläche (Review #198 H-198c),
//! ungerundet stünde er mit 17 Stellen in der Datei (H-198f).

use crate::scene::Scene;
use std::borrow::Cow;

/// So nah (Bildpunkte) rastet ein Punkt an einem Ziel.
pub const FANG_PX: f64 = 10.0;

/// Fangziele im Modell (mm), je Modellstand einmal gesammelt.
#[derive(Clone, Debug, Default)]
pub struct Ziele {
    pub ecken: Vec<[f64; 2]>,
    pub kanten: Vec<[[f64; 2]; 2]>,
}

/// Woran der Punkt gerastet ist; die Hilfslinien zeigen es.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Fang {
    Ecke,
    Kante,
}

impl Ziele {
    fn ring(&mut self, r: &[[f64; 2]]) {
        let n = r.len();
        for i in 0..n {
            self.ecken.push(r[i]);
            if n >= 2 {
                self.kanten.push([r[i], r[(i + 1) % n]]);
            }
        }
    }

    /// Nächstes Ziel zu `p` innerhalb `tol` (mm): erst Ecken, dann Kanten
    /// (Lotfußpunkt, auf 1 µm).
    pub fn fangen(&self, p: [f64; 2], tol: f64) -> Option<([f64; 2], Fang)> {
        let d = |q: [f64; 2]| (q[0] - p[0]).hypot(q[1] - p[1]);
        let ecke = self
            .ecken
            .iter()
            .map(|&q| (d(q), q))
            .filter(|x| x.0 <= tol)
            .min_by(|a, b| a.0.total_cmp(&b.0));
        if let Some((_, q)) = ecke {
            return Some((q, Fang::Ecke));
        }
        self.kanten
            .iter()
            .filter_map(|&[a, b]| {
                let e = [b[0] - a[0], b[1] - a[1]];
                let l2 = e[0] * e[0] + e[1] * e[1];
                if l2 < 1.0 {
                    return None;
                }
                let t = (((p[0] - a[0]) * e[0] + (p[1] - a[1]) * e[1]) / l2).clamp(0.0, 1.0);
                let um = |x: f64| (x * 1000.0).round() / 1000.0 + 0.0;
                let q = [um(a[0] + e[0] * t), um(a[1] + e[1] * t)];
                Some((d(q), q))
            })
            .filter(|x| x.0 <= tol)
            .min_by(|a, b| a.0.total_cmp(&b.0))
            .map(|(_, q)| (q, Fang::Kante))
    }
}

/// Ziele der Szene: Außenkante jedes geschlossenen Wandzugs mit Gründung,
/// alle Geländeflächen, die Grundstücksgrenzen im Modell. Die Züge aus der
/// Szene, sonst aus dem Modell, wie [`Scene::fangziele`] (H-198d); alles
/// in einem Gang über das Verzeichnis (H-198a).
pub fn ziele(scene: &Scene) -> Ziele {
    let m = scene.model();
    m.lesend(|| sammeln(scene))
}

fn sammeln(scene: &Scene) -> Ziele {
    let m = scene.model();
    let mut z = Ziele::default();
    for p in m.site_patches() {
        z.ring(&p.outline);
    }
    for r in m.terrain_boundaries() {
        z.ring(&r);
    }
    for (id, _) in m.runs().iter() {
        if m.foundation_of(id).is_none() {
            continue;
        }
        let Some(c) = scene
            .chain(id)
            .map(Cow::Borrowed)
            .or_else(|| m.chain(id).map(Cow::Owned))
            .filter(|c| c.closed)
        else {
            continue;
        };
        let r: Vec<[f64; 2]> = c
            .face_corners(c.outer_offset())
            .iter()
            .map(|v| [v.x, v.y])
            .collect();
        z.ring(&r);
    }
    z
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ecke_vor_kante() {
        let mut z = Ziele::default();
        z.ring(&[[0.0, 0.0], [4000.0, 0.0], [4000.0, 3000.0], [0.0, 3000.0]]);
        // nahe der Ecke: die Ecke, auch wenn die Kante näher liegt
        assert_eq!(
            z.fangen([4060.0, 20.0], 100.0),
            Some(([4000.0, 0.0], Fang::Ecke))
        );
        // an der Kante: der Lotfußpunkt, auf 1 µm
        let (q, f) = z.fangen([2000.4, 55.0], 100.0).unwrap();
        assert_eq!(f, Fang::Kante);
        assert_eq!(q, [2000.4, 0.0]);
        // schräge Kante: der Punkt liegt auf ihr, nicht daneben (H-198c),
        // und hat höchstens drei Nachkommastellen (H-198f)
        let mut s = Ziele::default();
        s.ring(&[[0.0, 0.0], [3000.0, 1000.0], [0.0, 2000.0]]);
        let (q, f) = s.fangen([1501.0, 497.0], 100.0).unwrap();
        assert_eq!(f, Fang::Kante);
        assert!((q[0] / 3.0 - q[1]).abs() < 1e-3, "{q:?}");
        for x in q {
            let t = x.to_string();
            assert!(t.split('.').nth(1).is_none_or(|n| n.len() <= 3), "{t}");
        }
        // zu weit weg: nichts
        assert_eq!(z.fangen([2000.0, 500.0], 100.0), None);
        assert_eq!(Ziele::default().fangen([0.0, 0.0], 100.0), None);
    }
}
