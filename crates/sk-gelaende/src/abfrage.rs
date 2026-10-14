//! Höhe an einer Stelle und Profil entlang einer Strecke. Ein Gitter über
//! den Dreiecken findet das Dreieck in O(1); im Dreieck gilt die Ebene durch
//! seine drei Punkte. Außerhalb des Netzes gilt die Höhe des nächsten
//! Randpunkts (plan-gelaendemodul.md §5 G1 „Haus und Gelände“).

use crate::netz::Netz;
use crate::volumen::{erdmenge_dreieck, Bezug, Erdmenge};

/// Höhenabfrage über einem fertigen Netz (x, y, z in m wie im Netz).
#[derive(Clone, Debug)]
pub struct Abfrage {
    punkte: Vec<[f64; 3]>,
    dreiecke: Vec<[u32; 3]>,
    /// Randkanten (Inneres links von a → b).
    rand: Vec<[u32; 2]>,
    x0: f64,
    y0: f64,
    zelle: f64,
    nx: usize,
    ny: usize,
    /// Dreiecke je Zelle, Zeile für Zeile.
    start: Vec<u32>,
    inhalt: Vec<u32>,
    /// Tiefste und höchste Höhe, einmal beim Bau bestimmt.
    bereich: Option<(f64, f64)>,
}

/// Toleranz für „auf der Kante“ in baryzentrischen Anteilen.
const AUF_KANTE: f64 = 1e-9;

impl Abfrage {
    pub fn neu(netz: &Netz) -> Abfrage {
        let punkte = netz.punkte.clone();
        let dreiecke = netz.dreiecke.clone();
        // Randkanten: gerichtete Kanten ohne Gegenkante
        let mut kanten: std::collections::HashSet<(u32, u32)> = std::collections::HashSet::new();
        for t in &dreiecke {
            for k in 0..3 {
                kanten.insert((t[k], t[(k + 1) % 3]));
            }
        }
        let mut rand: Vec<[u32; 2]> = kanten
            .iter()
            .filter(|(a, b)| !kanten.contains(&(*b, *a)))
            .map(|&(a, b)| [a, b])
            .collect();
        rand.sort_unstable();
        let (mut x0, mut y0, mut x1, mut y1) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
        for t in &dreiecke {
            for &i in t {
                let p = punkte[i as usize];
                x0 = x0.min(p[0]);
                y0 = y0.min(p[1]);
                x1 = x1.max(p[0]);
                y1 = y1.max(p[1]);
            }
        }
        if dreiecke.is_empty() {
            (x0, y0, x1, y1) = (0.0, 0.0, 0.0, 0.0);
        }
        let n = dreiecke.len().max(1) as f64;
        let zelle = (((x1 - x0) * (y1 - y0)) / n)
            .sqrt()
            .max((x1 - x0).max(y1 - y0) / 4096.0)
            .max(1e-6);
        let nx = (((x1 - x0) / zelle) as usize + 1).max(1);
        let ny = (((y1 - y0) / zelle) as usize + 1).max(1);
        let mut a = Abfrage {
            punkte,
            dreiecke,
            rand,
            x0,
            y0,
            zelle,
            nx,
            ny,
            start: vec![0; nx * ny + 1],
            inhalt: Vec::new(),
            bereich: None,
        };
        let mut z = a.punkte.iter().map(|p| p[2]);
        a.bereich = z
            .next()
            .map(|z0| z.fold((z0, z0), |(lo, hi), z| (lo.min(z), hi.max(z))));
        // zweimal durch: zählen, dann füllen
        let zellen = |a: &Abfrage, t: &[u32; 3]| {
            let p = t.map(|i| a.punkte[i as usize]);
            let lo = a.zelle_von(
                p[0][0].min(p[1][0]).min(p[2][0]),
                p[0][1].min(p[1][1]).min(p[2][1]),
            );
            let hi = a.zelle_von(
                p[0][0].max(p[1][0]).max(p[2][0]),
                p[0][1].max(p[1][1]).max(p[2][1]),
            );
            (lo, hi)
        };
        let mut zahl = vec![0u32; nx * ny + 1];
        for t in &a.dreiecke {
            let ((cx0, cy0), (cx1, cy1)) = zellen(&a, t);
            for cy in cy0..=cy1 {
                for cx in cx0..=cx1 {
                    zahl[cy * nx + cx + 1] += 1;
                }
            }
        }
        for k in 0..nx * ny {
            zahl[k + 1] += zahl[k];
        }
        let mut pos = zahl.clone();
        let mut inhalt = vec![0u32; zahl[nx * ny] as usize];
        for (ti, t) in a.dreiecke.iter().enumerate() {
            let ((cx0, cy0), (cx1, cy1)) = zellen(&a, t);
            for cy in cy0..=cy1 {
                for cx in cx0..=cx1 {
                    let c = cy * nx + cx;
                    inhalt[pos[c] as usize] = ti as u32;
                    pos[c] += 1;
                }
            }
        }
        a.start = zahl;
        a.inhalt = inhalt;
        a
    }

    fn zelle_von(&self, x: f64, y: f64) -> (usize, usize) {
        let c = |v: f64, n: usize| (v.max(0.0) as usize).min(n - 1);
        (
            c((x - self.x0) / self.zelle, self.nx),
            c((y - self.y0) / self.zelle, self.ny),
        )
    }

    /// Dreiecke, deren Rechteck die Zelle von (x, y) berührt.
    fn kandidaten(&self, x: f64, y: f64) -> &[u32] {
        let (cx, cy) = self.zelle_von(x, y);
        let c = cy * self.nx + cx;
        &self.inhalt[self.start[c] as usize..self.start[c + 1] as usize]
    }

    /// Baryzentrische Anteile von (x, y) im Dreieck `t`.
    fn anteile(&self, t: usize, x: f64, y: f64) -> [f64; 3] {
        let [a, b, c] = self.dreiecke[t].map(|i| self.punkte[i as usize]);
        let d = (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0]);
        let l1 = ((b[0] - x) * (c[1] - y) - (b[1] - y) * (c[0] - x)) / d;
        let l2 = ((c[0] - x) * (a[1] - y) - (c[1] - y) * (a[0] - x)) / d;
        [l1, l2, 1.0 - l1 - l2]
    }

    /// Dreieck, in dem (x, y) liegt.
    pub fn dreieck(&self, x: f64, y: f64) -> Option<usize> {
        if self.dreiecke.is_empty() {
            return None;
        }
        self.kandidaten(x, y)
            .iter()
            .map(|&t| t as usize)
            .find(|&t| self.anteile(t, x, y).iter().all(|&l| l >= -AUF_KANTE))
    }

    /// (x, y) liegt im Netz.
    pub fn innen(&self, x: f64, y: f64) -> bool {
        self.dreieck(x, y).is_some()
    }

    /// Höhe an (x, y); außerhalb die Höhe des nächsten Randpunkts, ohne
    /// Netz `None`.
    pub fn hoehe(&self, x: f64, y: f64) -> Option<f64> {
        if !(x.is_finite() && y.is_finite()) {
            return None;
        }
        if let Some(t) = self.dreieck(x, y) {
            let l = self.anteile(t, x, y);
            let z = self.dreiecke[t].map(|i| self.punkte[i as usize][2]);
            return Some(l[0] * z[0] + l[1] * z[1] + l[2] * z[2]);
        }
        self.am_rand(x, y).map(|r| r.1)
    }

    /// Nächster Punkt auf dem Rand: (Abstand, Höhe dort).
    fn am_rand(&self, x: f64, y: f64) -> Option<(f64, f64)> {
        let mut best: Option<(f64, f64)> = None;
        for &[a, b] in &self.rand {
            let (p, q) = (self.punkte[a as usize], self.punkte[b as usize]);
            let (dx, dy) = (q[0] - p[0], q[1] - p[1]);
            let l2 = dx * dx + dy * dy;
            let s = if l2 > 0.0 {
                (((x - p[0]) * dx + (y - p[1]) * dy) / l2).clamp(0.0, 1.0)
            } else {
                0.0
            };
            let d = (p[0] + s * dx - x).hypot(p[1] + s * dy - y);
            if best.is_none_or(|b| d < b.0) {
                best = Some((d, p[2] + s * (q[2] - p[2])));
            }
        }
        best
    }

    /// Tiefste und höchste Höhe des Netzes; `None` ohne Punkte.
    pub fn hoehenbereich(&self) -> Option<(f64, f64)> {
        self.bereich
    }

    /// Kantenlänge einer Zelle des Suchgitters (m), etwa der mittlere
    /// Punktabstand.
    pub fn zelle(&self) -> f64 {
        self.zelle
    }

    /// Diagonale des Rechtecks um das Netz (m); 0 ohne Netz.
    pub fn diagonale(&self) -> f64 {
        if self.dreiecke.is_empty() {
            return 0.0;
        }
        (self.nx as f64 * self.zelle).hypot(self.ny as f64 * self.zelle)
    }

    /// Die Dreiecke in den Zellen unter dem Polygon `ring`, je einmal, mit
    /// ihren Punkten.
    pub(crate) fn dreiecke_unter(&self, ring: &[[f64; 2]]) -> Vec<[[f64; 3]; 3]> {
        if ring.len() < 3 || self.dreiecke.is_empty() {
            return Vec::new();
        }
        let (mut x0, mut y0, mut x1, mut y1) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
        for q in ring {
            x0 = x0.min(q[0]);
            y0 = y0.min(q[1]);
            x1 = x1.max(q[0]);
            y1 = y1.max(q[1]);
        }
        let ((cx0, cy0), (cx1, cy1)) = (self.zelle_von(x0, y0), self.zelle_von(x1, y1));
        let mut ts = Vec::new();
        for cy in cy0..=cy1 {
            let c = cy * self.nx;
            ts.extend_from_slice(
                &self.inhalt[self.start[c + cx0] as usize..self.start[c + cx1 + 1] as usize],
            );
        }
        ts.sort_unstable();
        ts.dedup();
        ts.into_iter()
            .map(|t| self.dreiecke[t as usize].map(|i| self.punkte[i as usize]))
            .collect()
    }

    /// [`Erdmenge`] über dem Polygon `ring` gegen `bezug`
    /// ([`crate::volumen::erdmenge`]), nur über den Dreiecken in den Zellen
    /// unter dem Polygon: je Baugrube unabhängig von der Größe des Netzes.
    pub fn erdmenge(&self, ring: &[[f64; 2]], bezug: Bezug) -> Erdmenge {
        self.erdmengen(ring, &[bezug])[0]
    }

    /// [`Abfrage::erdmenge`] je Bezug, das Polygon einmal geschnitten.
    pub fn erdmengen(&self, ring: &[[f64; 2]], bezuege: &[Bezug]) -> Vec<Erdmenge> {
        let mut e = vec![Erdmenge::default(); bezuege.len()];
        if ring.len() < 3 || self.dreiecke.is_empty() {
            return e;
        }
        let (mut x0, mut y0, mut x1, mut y1) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
        for q in ring {
            x0 = x0.min(q[0]);
            y0 = y0.min(q[1]);
            x1 = x1.max(q[0]);
            y1 = y1.max(q[1]);
        }
        let ((cx0, cy0), (cx1, cy1)) = (self.zelle_von(x0, y0), self.zelle_von(x1, y1));
        let mut ts = Vec::new();
        for cy in cy0..=cy1 {
            let c = cy * self.nx;
            ts.extend_from_slice(
                &self.inhalt[self.start[c + cx0] as usize..self.start[c + cx1 + 1] as usize],
            );
        }
        ts.sort_unstable();
        ts.dedup();
        for t in ts {
            let p = self.dreiecke[t as usize].map(|i| self.punkte[i as usize]);
            erdmenge_dreieck(p, ring, bezuege, &mut e);
        }
        e
    }

    /// Profil von `a` nach `b`: (Abstand ab `a`, Höhe) an `a`, an jedem
    /// Schnitt mit einer Netzkante und an `b`. Dazwischen verläuft das
    /// Gelände im Netz geradlinig.
    pub fn profil(&self, a: [f64; 2], b: [f64; 2]) -> Vec<[f64; 2]> {
        let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
        let laenge = dx.hypot(dy);
        self.anteile_auf(a, b)
            .into_iter()
            .filter_map(|s| {
                let (x, y) = (a[0] + s * dx, a[1] + s * dy);
                Some([s * laenge, self.hoehe(x, y)?])
            })
            .collect()
    }

    /// Wie [`Abfrage::profil`], aber nur im Netz: endet mit dem letzten
    /// Punkt im Netz; `true`, wenn die Strecke das Netz dort verlässt oder
    /// außerhalb beginnt. Ohne die Randhöhen von außerhalb, darum auch
    /// ohne die Suche über den Rand (Review #169 B2, B6).
    pub fn profil_innen(&self, a: [f64; 2], b: [f64; 2]) -> (Vec<[f64; 2]>, bool) {
        let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
        let laenge = dx.hypot(dy);
        let mut out = Vec::new();
        for s in self.anteile_auf(a, b) {
            let (x, y) = (a[0] + s * dx, a[1] + s * dy);
            let Some(t) = self.dreieck(x, y) else {
                return (out, true);
            };
            let l = self.anteile(t, x, y);
            let z = self.dreiecke[t].map(|i| self.punkte[i as usize][2]);
            out.push([s * laenge, l[0] * z[0] + l[1] * z[1] + l[2] * z[2]]);
        }
        (out, false)
    }

    /// Anteile auf a → b an a, an jedem Schnitt mit einer Netzkante und an
    /// b, aufsteigend. Je Zelle nahe der Strecke ihre Dreiecke; eine Kante,
    /// die in zwei Zellen liegt, gibt denselben Anteil zweimal, das Sortieren
    /// legt sie zusammen.
    fn anteile_auf(&self, a: [f64; 2], b: [f64; 2]) -> Vec<f64> {
        let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
        let laenge = dx.hypot(dy);
        let mut t: Vec<f64> = vec![0.0, 1.0];
        if !self.dreiecke.is_empty() && laenge > 0.0 && laenge.is_finite() {
            let (lo, hi) = (
                self.zelle_von(a[0].min(b[0]), a[1].min(b[1])),
                self.zelle_von(a[0].max(b[0]), a[1].max(b[1])),
            );
            for cy in lo.1..=hi.1 {
                for cx in lo.0..=hi.0 {
                    // nur Zellen nahe der Strecke
                    let (mx, my) = (
                        self.x0 + (cx as f64 + 0.5) * self.zelle,
                        self.y0 + (cy as f64 + 0.5) * self.zelle,
                    );
                    if ((mx - a[0]) * dy - (my - a[1]) * dx).abs() / laenge > self.zelle {
                        continue;
                    }
                    let c = cy * self.nx + cx;
                    for &ti in &self.inhalt[self.start[c] as usize..self.start[c + 1] as usize] {
                        let tr = self.dreiecke[ti as usize];
                        for k in 0..3 {
                            let (u, v) = (tr[k].min(tr[(k + 1) % 3]), tr[k].max(tr[(k + 1) % 3]));
                            let (p, q) = (self.punkte[u as usize], self.punkte[v as usize]);
                            if let Some(s) = schnitt(a, b, [p[0], p[1]], [q[0], q[1]]) {
                                t.push(s);
                            }
                        }
                    }
                }
            }
        }
        t.sort_by(f64::total_cmp);
        t.dedup_by(|x, y| (*x - *y) * laenge < 1e-6);
        t
    }
}

/// Anteil auf a → b, an dem die Strecke p–q sie schneidet.
fn schnitt(a: [f64; 2], b: [f64; 2], p: [f64; 2], q: [f64; 2]) -> Option<f64> {
    let r = [b[0] - a[0], b[1] - a[1]];
    let s = [q[0] - p[0], q[1] - p[1]];
    let d = r[0] * s[1] - r[1] * s[0];
    if d.abs() < 1e-15 {
        return None;
    }
    let w = [p[0] - a[0], p[1] - a[1]];
    let t = (w[0] * s[1] - w[1] * s[0]) / d;
    let u = (w[0] * r[1] - w[1] * r[0]) / d;
    ((0.0..=1.0).contains(&t) && (0.0..=1.0).contains(&u)).then_some(t)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::netz::{netz, Rand};

    fn schraeg() -> Netz {
        // Ebene z = 10 + 0,1 x + 0,05 y über einem Gitter
        let mut p = Vec::new();
        for i in 0..6 {
            for j in 0..5 {
                let (x, y) = (i as f64 * 2.0, j as f64 * 2.0);
                p.push([x, y, 10.0 + 0.1 * x + 0.05 * y]);
            }
        }
        netz(&p, &[], Rand::Huelle)
    }

    #[test]
    fn ebene_genau_und_rand() {
        let n = schraeg();
        let a = Abfrage::neu(&n);
        for (x, y) in [
            (0.0, 0.0),
            (3.3, 1.7),
            (10.0, 8.0),
            (5.0, 4.0),
            (7.123, 0.0),
        ] {
            let z = a.hoehe(x, y).unwrap();
            assert!(
                (z - (10.0 + 0.1 * x + 0.05 * y)).abs() < 1e-12,
                "{x} {y}: {z}"
            );
        }
        assert!(a.innen(1.0, 1.0) && !a.innen(-0.1, 1.0));
        // außen: nächster Randpunkt
        assert!((a.hoehe(-5.0, 4.0).unwrap() - 10.2).abs() < 1e-12);
        assert!((a.hoehe(15.0, 12.0).unwrap() - (10.0 + 1.0 + 0.4)).abs() < 1e-12);
        assert_eq!(a.hoehe(f64::NAN, 1.0), None);
        assert_eq!(a.hoehe(1.0, f64::INFINITY), None);
        let leer = Abfrage::neu(&netz(&[], &[], Rand::Huelle));
        assert_eq!(leer.hoehe(0.0, 0.0), None);
    }

    #[test]
    fn profil_schneidet_alle_kanten() {
        let n = schraeg();
        let a = Abfrage::neu(&n);
        let pr = a.profil([-1.0, 1.0], [11.0, 1.0]);
        assert_eq!(pr.first().unwrap()[0], 0.0);
        assert_eq!(pr.last().unwrap()[0], 12.0);
        // innen auf der Ebene, außen waagerecht
        for w in &pr {
            let x = w[0] - 1.0;
            let soll = 10.0 + 0.1 * x.clamp(0.0, 10.0) + 0.05;
            assert!((w[1] - soll).abs() < 1e-12, "{w:?}");
        }
        // jede senkrechte Gitterlinie x = 0, 2, … 10 ist eine Kante
        for x in [0.0, 2.0, 4.0, 6.0, 8.0, 10.0] {
            assert!(
                pr.iter().any(|w| (w[0] - 1.0 - x).abs() < 1e-9),
                "x {x}: {pr:?}"
            );
        }
    }

    #[test]
    fn erdmengen_einmal_geschnitten() {
        let n = schraeg();
        let a = Abfrage::neu(&n);
        let ring = [[1.0, 1.0], [9.0, 2.0], [8.0, 7.0], [2.0, 6.0]];
        let bezuege = [[10.5, 0.0, 0.0], [10.2, 0.02, 0.0], [0.0, 0.0, 0.0]];
        let alle = a.erdmengen(&ring, &bezuege);
        for (b, e) in bezuege.iter().zip(&alle) {
            assert_eq!(*e, a.erdmenge(&ring, *b));
            let v = crate::volumen::erdmenge(&n, &ring, *b);
            for (x, y) in [
                (e.flaeche, v.flaeche),
                (e.ueber, v.ueber),
                (e.unter, v.unter),
            ] {
                assert!((x - y).abs() < 1e-9, "{e:?} {v:?}");
            }
        }
        // gegen null: das Flächenmittel wie über alle Dreiecke
        let m = (alle[2].ueber - alle[2].unter) / alle[2].flaeche;
        let soll = crate::volumen::mittel(&n, &ring).unwrap();
        assert!((m - soll).abs() < 1e-12, "{m} {soll}");
        assert!(a.erdmengen(&ring, &[]).is_empty());
    }
}
