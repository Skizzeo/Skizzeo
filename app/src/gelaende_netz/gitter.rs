//! Rechteckraster im Grundriss für das Gelände (Review #129 B3, B4): je
//! Feld die Stellen, deren Rechteck es trifft. So prüfen ein Dreieck, ein
//! Linienstück oder ein Punkt nur die Häuser und Dreiecke in ihrer Nähe
//! statt aller, und das Netz am Haus wächst nicht mehr mit dem Quadrat der
//! Häuser.

/// Rechteck: x, y von, x, y bis.
pub(super) type Kasten = [f64; 4];

/// Höchstens so viele Felder je Richtung.
const MAX_FELDER: usize = 1024;

/// Rechtecke und je Feld die Stellen, deren Rechteck es trifft.
pub(super) struct Gitter {
    kaesten: Vec<Kasten>,
    /// Ecke des Rasters und Kantenlänge eines Felds (mm).
    lo: [f64; 2],
    feld: f64,
    nx: usize,
    ny: usize,
    /// Je Feld (zeilenweise) der Anfang seiner Stellen in `stellen`, dazu
    /// das Ende des letzten.
    anfang: Vec<u32>,
    stellen: Vec<u32>,
}

pub(super) fn ueberlappt(a: &Kasten, b: &Kasten) -> bool {
    a[0] <= b[2] && b[0] <= a[2] && a[1] <= b[3] && b[1] <= a[3]
}

impl Gitter {
    /// Raster über `kaesten`: ein Feld ist etwa so groß wie das mittlere
    /// Rechteck, mindestens so groß, dass auf jedes Rechteck ein Feld
    /// kommt, und höchstens [`MAX_FELDER`] je Richtung.
    pub(super) fn neu(kaesten: Vec<Kasten>) -> Gitter {
        let gueltig = |k: &Kasten| k.iter().all(|v| v.is_finite()) && k[0] <= k[2] && k[1] <= k[3];
        let mut r = [f64::MAX, f64::MAX, f64::MIN, f64::MIN];
        let (mut n, mut mittel) = (0usize, 0.0);
        for k in kaesten.iter().filter(|k| gueltig(k)) {
            r = [
                r[0].min(k[0]),
                r[1].min(k[1]),
                r[2].max(k[2]),
                r[3].max(k[3]),
            ];
            mittel += (k[2] - k[0]).max(k[3] - k[1]);
            n += 1;
        }
        let mut g = Gitter {
            kaesten,
            lo: [0.0, 0.0],
            feld: 1.0,
            nx: 0,
            ny: 0,
            anfang: vec![0],
            stellen: Vec::new(),
        };
        if n == 0 {
            return g;
        }
        let (w, h) = (r[2] - r[0], r[3] - r[1]);
        let feld = (mittel / n as f64)
            .max((w * h / n as f64).sqrt())
            .max(w.max(h) / MAX_FELDER as f64)
            .max(1.0);
        g.lo = [r[0], r[1]];
        g.feld = feld;
        g.nx = ((w / feld) as usize + 1).min(MAX_FELDER);
        g.ny = ((h / feld) as usize + 1).min(MAX_FELDER);
        // erst zählen, dann je Feld der Reihe nach eintragen
        let mut zahl = vec![0u32; g.nx * g.ny + 1];
        for k in g.kaesten.iter().filter(|k| gueltig(k)) {
            let (x0, x1, y0, y1) = g.spanne(k).expect("im Rahmen");
            for y in y0..=y1 {
                for x in x0..=x1 {
                    zahl[y * g.nx + x + 1] += 1;
                }
            }
        }
        for i in 1..zahl.len() {
            zahl[i] += zahl[i - 1];
        }
        let mut frei = zahl.clone();
        g.stellen = vec![0; zahl[g.nx * g.ny] as usize];
        for (i, k) in g.kaesten.iter().enumerate().filter(|(_, k)| gueltig(k)) {
            let (x0, x1, y0, y1) = g.spanne(k).expect("im Rahmen");
            for y in y0..=y1 {
                for x in x0..=x1 {
                    let f = &mut frei[y * g.nx + x];
                    g.stellen[*f as usize] = i as u32;
                    *f += 1;
                }
            }
        }
        g.anfang = zahl;
        g
    }

    /// Felder, die `k` trifft (an den Rand des Rasters geklemmt); `None`,
    /// wenn `k` neben dem Raster liegt.
    fn spanne(&self, k: &Kasten) -> Option<(usize, usize, usize, usize)> {
        if self.nx == 0 || k.iter().any(|v| v.is_nan()) {
            return None;
        }
        let i = |v: f64, lo: f64, n: usize| -> Option<usize> {
            let f = ((v - lo) / self.feld).floor();
            if f < 0.0 {
                None
            } else {
                Some((f as usize).min(n - 1))
            }
        };
        let hi_x = self.lo[0] + self.feld * self.nx as f64;
        let hi_y = self.lo[1] + self.feld * self.ny as f64;
        if k[2] < self.lo[0] || k[3] < self.lo[1] || k[0] > hi_x || k[1] > hi_y {
            return None;
        }
        let x0 = i(k[0], self.lo[0], self.nx).unwrap_or(0);
        let y0 = i(k[1], self.lo[1], self.ny).unwrap_or(0);
        let x1 = i(k[2], self.lo[0], self.nx)?;
        let y1 = i(k[3], self.lo[1], self.ny)?;
        Some((x0, x1, y0, y1))
    }

    /// Stellen der Rechtecke, die `k` treffen (Rand eingeschlossen),
    /// aufsteigend und ohne doppelte, nach `out`.
    pub(super) fn bei(&self, k: &Kasten, out: &mut Vec<u32>) {
        out.clear();
        let Some((x0, x1, y0, y1)) = self.spanne(k) else {
            return;
        };
        for y in y0..=y1 {
            for x in x0..=x1 {
                let f = y * self.nx + x;
                let (a, z) = (self.anfang[f] as usize, self.anfang[f + 1] as usize);
                out.extend(&self.stellen[a..z]);
            }
        }
        if (x0, y0) != (x1, y1) {
            out.sort_unstable();
            out.dedup();
        }
        out.retain(|&i| ueberlappt(&self.kaesten[i as usize], k));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Das Raster findet dieselben Rechtecke wie die Suche über alle, auch
    /// am Rand und für Rechtecke über viele Felder.
    #[test]
    fn wie_suche_ueber_alle() {
        let mut s = 7u64;
        let mut zufall = || {
            s = s
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            (s >> 11) as f64 / (1u64 << 53) as f64
        };
        let mut kaesten = Vec::new();
        for i in 0..500 {
            let (x, y) = (zufall() * 1e6, zufall() * 4e5);
            let gross = if i % 50 == 0 { 2e5 } else { 1e4 };
            kaesten.push([x, y, x + zufall() * gross, y + zufall() * gross]);
        }
        kaesten.push([f64::NAN, 0.0, 1.0, 1.0]);
        let g = Gitter::neu(kaesten.clone());
        let mut out = Vec::new();
        for _ in 0..500 {
            let (x, y) = (zufall() * 1.2e6 - 1e5, zufall() * 6e5 - 1e5);
            let k = [x, y, x + zufall() * 3e4, y + zufall() * 3e4];
            g.bei(&k, &mut out);
            let soll: Vec<u32> = (0..kaesten.len() as u32)
                .filter(|&i| ueberlappt(&kaesten[i as usize], &k))
                .collect();
            assert_eq!(out, soll, "{k:?}");
        }
        // ein Punkt auf der Ecke eines Rechtecks trifft es
        let k = kaesten[3];
        g.bei(&[k[2], k[3], k[2], k[3]], &mut out);
        assert!(out.contains(&3));
        assert!(Gitter::neu(Vec::new()).anfang.len() == 1);
    }
}
