//! OK Gelände als Raster (Gelände G1): das Netz des Höhenaufmaßes, auf
//! ein Gitter im Grundriss gelegt. Der Renderer fragt es für „unter
//! Gelände“ (S11) je Bildpunkt und Kante ab, die App teilt die Kanten an
//! derselben Höhe ([`TerrainGrid::z`] rechnet wie der Shader).

use sk_math::Vec3;
use std::sync::atomic::{AtomicU64, Ordering};

/// Höhe eines Gitterpunkts außerhalb des Netzes.
pub const KEIN: f32 = -1e30;

/// Höchstens so viele Gitterpunkte je Richtung.
pub const GITTER_MAX: usize = 256;

static STAND: AtomicU64 = AtomicU64::new(1);

/// Höhen auf einem Gitter im Grundriss: Rechteck `rect` (`[x0, y0, x1,
/// y1]`, mm), `n` Punkte je Richtung (Ecken eingeschlossen), Höhen zeilenweise
/// ab `y0` (mm zu ±0,00), [`KEIN`] außerhalb des Netzes.
#[derive(Clone, Debug, PartialEq)]
pub struct TerrainGrid {
    pub rect: [f64; 4],
    pub n: [usize; 2],
    pub z: Vec<f32>,
    /// Kennung für das Hochladen: jedes neue Gitter eine eigene.
    pub(crate) stand: u64,
}

impl TerrainGrid {
    /// Das Gitter über den Dreiecken (Modellkoordinaten, mm): ihr Rahmen,
    /// höchstens [`GITTER_MAX`] Punkte je Richtung, kein Abstand unter
    /// 10 cm. `None` ohne Dreiecke oder ohne Fläche.
    pub fn aus_dreiecken(dreiecke: &[[Vec3; 3]]) -> Option<TerrainGrid> {
        let mut r = [
            f64::INFINITY,
            f64::INFINITY,
            f64::NEG_INFINITY,
            f64::NEG_INFINITY,
        ];
        for p in dreiecke.iter().flatten() {
            r = [r[0].min(p.x), r[1].min(p.y), r[2].max(p.x), r[3].max(p.y)];
        }
        let (w, h) = (r[2] - r[0], r[3] - r[1]);
        if !(w > 0.0 && h > 0.0 && w.is_finite() && h.is_finite()) {
            return None;
        }
        let schritt = (w.max(h) / (GITTER_MAX - 1) as f64).max(100.0);
        let anzahl = |l: f64| ((l / schritt).ceil() as usize + 1).clamp(2, GITTER_MAX);
        let n = [anzahl(w), anzahl(h)];
        let (dx, dy) = (w / (n[0] - 1) as f64, h / (n[1] - 1) as f64);
        let mut z = vec![KEIN; n[0] * n[1]];
        for d in dreiecke {
            let [a, b, c] = *d;
            let det = (b.x - a.x) * (c.y - a.y) - (c.x - a.x) * (b.y - a.y);
            if det.abs() < 1e-9 {
                continue;
            }
            let lo =
                |v: f64, o: f64, s: f64, m: usize| (((v - o) / s).ceil().max(0.0) as usize).min(m);
            let hi =
                |v: f64, o: f64, s: f64, m: usize| (((v - o) / s).floor().max(0.0) as usize).min(m);
            let (x0, x1) = (a.x.min(b.x).min(c.x), a.x.max(b.x).max(c.x));
            let (y0, y1) = (a.y.min(b.y).min(c.y), a.y.max(b.y).max(c.y));
            let (i0, i1) = (lo(x0, r[0], dx, n[0] - 1), hi(x1, r[0], dx, n[0] - 1));
            let (j0, j1) = (lo(y0, r[1], dy, n[1] - 1), hi(y1, r[1], dy, n[1] - 1));
            for j in j0..=j1 {
                let y = r[1] + j as f64 * dy;
                for i in i0..=i1 {
                    let x = r[0] + i as f64 * dx;
                    // baryzentrisch; Punkte auf einer Kante zählen zu beiden
                    let u = ((x - a.x) * (c.y - a.y) - (c.x - a.x) * (y - a.y)) / det;
                    let v = ((b.x - a.x) * (y - a.y) - (x - a.x) * (b.y - a.y)) / det;
                    let e = -1e-9;
                    if u >= e && v >= e && u + v <= 1.0 - e {
                        let h = a.z + u * (b.z - a.z) + v * (c.z - a.z);
                        z[j * n[0] + i] = h as f32;
                    }
                }
            }
        }
        Some(TerrainGrid {
            rect: r,
            n,
            z,
            stand: STAND.fetch_add(1, Ordering::Relaxed),
        })
    }

    /// OK Gelände bei `(x, y)` (mm): bilinear zwischen den vier
    /// Gitterpunkten um die Stelle, wie `terrain_grid` im Shader; `None`
    /// außerhalb des Rechtecks oder neben einem Punkt ohne Netz.
    pub fn z(&self, x: f64, y: f64) -> Option<f64> {
        let [x0, y0, x1, y1] = self.rect;
        if !(x >= x0 && y >= y0 && x <= x1 && y <= y1) {
            return None;
        }
        let [nx, ny] = self.n;
        let fx = (x - x0) / (x1 - x0) * (nx - 1) as f64;
        let fy = (y - y0) / (y1 - y0) * (ny - 1) as f64;
        let (i, j) = ((fx as usize).min(nx - 2), (fy as usize).min(ny - 2));
        let (t, u) = (fx - i as f64, fy - j as f64);
        let at = |i: usize, j: usize| self.z[j * nx + i];
        let q = [at(i, j), at(i + 1, j), at(i, j + 1), at(i + 1, j + 1)];
        if q.iter().any(|v| *v < KEIN * 0.5) {
            return None;
        }
        let q = q.map(f64::from);
        let unten = q[0] + (q[1] - q[0]) * t;
        let oben = q[2] + (q[3] - q[2]) * t;
        Some(unten + (oben - unten) * u)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sk_math::vec3;

    /// Schräge Ebene z = x / 10 über zwei Dreiecken: das Gitter trifft
    /// sie überall, außerhalb gibt es nichts.
    #[test]
    fn schraege_ebene() {
        let p = |x: f64, y: f64| vec3(x, y, x / 10.0);
        let d = [
            [p(0.0, 0.0), p(20_000.0, 0.0), p(20_000.0, 10_000.0)],
            [p(0.0, 0.0), p(20_000.0, 10_000.0), p(0.0, 10_000.0)],
        ];
        let g = TerrainGrid::aus_dreiecken(&d).unwrap();
        assert!(g.n[0] <= GITTER_MAX && g.n[1] >= 2);
        for (x, y) in [
            (0.0, 0.0),
            (1234.5, 6789.0),
            (20_000.0, 10_000.0),
            (19_999.0, 3.0),
        ] {
            let z = g.z(x, y).unwrap();
            assert!((z - x / 10.0).abs() < 0.01, "{x} {y}: {z}");
        }
        assert_eq!(g.z(-1.0, 5.0), None);
        assert_eq!(g.z(5.0, 10_001.0), None);
        assert!(TerrainGrid::aus_dreiecken(&d).unwrap().stand != g.stand);
    }

    /// Ein Dreieck: die Ecke gegenüber bleibt leer.
    #[test]
    fn leer_ausserhalb_des_netzes() {
        let d = [[
            vec3(0.0, 0.0, 5.0),
            vec3(1000.0, 0.0, 5.0),
            vec3(0.0, 1000.0, 5.0),
        ]];
        let g = TerrainGrid::aus_dreiecken(&d).unwrap();
        assert_eq!(g.z(100.0, 100.0), Some(5.0));
        assert_eq!(g.z(990.0, 990.0), None);
        assert_eq!(TerrainGrid::aus_dreiecken(&[]), None);
    }
}
