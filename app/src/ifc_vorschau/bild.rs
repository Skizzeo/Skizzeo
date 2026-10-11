//! 3D-Bild der IFC-Vorschau in Software (ifc/plan.md §6): Tiefenpuffer,
//! flache Schattierung je Dreieck, Kanten an Knicken und Rändern. Das
//! Vorschaufenster ist ein Seitenfenster ohne OpenGL; das Bild entsteht hier
//! als Pixel. Doppelt abgetastet und gemittelt, dazu ein Puffer mit dem Teil
//! je Bildpunkt zum Wählen mit der Maus.

use crate::camera::Camera;
use sk_ifc::lesen::Teil;
use sk_math::{vec3, Mat4, Vec3};

/// Kamera wie „Alles zeigen“ in der Ansicht auf den Kasten `lo`..`hi`.
pub fn passend(lo: Vec3, hi: Vec3) -> Camera {
    crate::camera::fit_perspective(lo, hi)
}

/// Projektion mit denselben Matrizen wie [`Camera::view`], in Koordinaten
/// relativ zum Auge.
struct Proj {
    vp: Mat4,
    auge: Vec3,
    vor: Vec3,
    rechts: Vec3,
    oben: Vec3,
    near: f64,
}

fn proj(k: &Camera, w: f64, h: f64) -> Proj {
    let (vor, rechts, oben) = (k.forward(), k.right(), k.up());
    let aspect = w.max(1.0) / h.max(1.0);
    let rot = Mat4::view_rotation(vor, rechts, oben);
    let p = match k.ortho {
        Some(half) => Mat4::orthographic(half, aspect, k.near(), k.far()),
        None => Mat4::perspective(k.fov_y, aspect, k.near(), k.far()),
    };
    Proj {
        vp: p * rot,
        auge: k.eye,
        vor,
        rechts,
        oben,
        near: k.near(),
    }
}

/// Bildpunkt (x nach rechts, y nach unten) und Tiefe entlang der
/// Blickrichtung; `None` vor der Nahebene.
fn projekt(b: &Proj, p: Vec3, w: f64, h: f64) -> Option<(f64, f64, f64)> {
    let r = p - b.auge;
    let z = r.dot(b.vor);
    if z <= b.near {
        return None;
    }
    let c = b.vp.mul_vec4([r.x, r.y, r.z, 1.0]);
    Some((
        (c[0] / c[3] * 0.5 + 0.5) * w,
        (0.5 - c[1] / c[3] * 0.5) * h,
        z,
    ))
}

/// Was gezeichnet wird.
pub struct Szene<'a> {
    pub teile: &'a [Teil],
    /// Teil sichtbar (nach Geschosswahl).
    pub sichtbar: &'a [bool],
    pub gewaehlt: Option<usize>,
    pub hinter: [u8; 3],
    pub kante: [u8; 3],
    pub wahl: [u8; 3],
}

/// Fertiges Bild: RGBA (nicht vormultipliziert, deckend) und je Bildpunkt
/// das Teil (`u32::MAX` = nichts).
pub struct Bild {
    pub w: usize,
    pub h: usize,
    pub rgba: Vec<u8>,
    pub teil: Vec<u32>,
    /// Tiefe entlang der Blickrichtung je Bildpunkt (unendlich = nichts).
    pub tiefe: Vec<f64>,
}

impl Bild {
    pub fn teil_bei(&self, x: f64, y: f64) -> Option<usize> {
        if x < 0.0 || y < 0.0 {
            return None;
        }
        let (x, y) = (x as usize, y as usize);
        if x >= self.w || y >= self.h {
            return None;
        }
        let t = self.teil[y * self.w + x];
        (t != u32::MAX).then_some(t as usize)
    }

    /// Punkt im Modell unter dem Bildpunkt (Drehpunkt und Zoomziel wie in
    /// der Ansicht); `k` ist die Kamera, mit der das Bild gezeichnet wurde.
    pub fn punkt_bei(&self, k: &Camera, x: f64, y: f64) -> Option<Vec3> {
        let (xi, yi) = (x.floor(), y.floor());
        if xi < 0.0 || yi < 0.0 || xi >= self.w as f64 || yi >= self.h as f64 {
            return None;
        }
        let d = self.tiefe[yi as usize * self.w + xi as usize];
        if !d.is_finite() {
            return None;
        }
        let (o, r) = k.ray(x, y, self.w as f64, self.h as f64);
        let s = r.dot(k.forward());
        (s > 1e-9).then(|| o + r * (d / s))
    }
}

/// Licht von links oben hinter dem Betrachter, fest zur Kamera.
fn hell(n: Vec3, b: &Proj) -> f64 {
    let l = (b.oben * 0.8 - b.rechts * 0.5 - b.vor * 0.6).normalized();
    0.45 + 0.55 * n.dot(l).abs()
}

/// Kasten um alle sichtbaren Teile.
pub fn grenzen(teile: &[Teil], sichtbar: &[bool]) -> Option<(Vec3, Vec3)> {
    let mut lo = vec3(f64::MAX, f64::MAX, f64::MAX);
    let mut hi = vec3(f64::MIN, f64::MIN, f64::MIN);
    for (i, t) in teile.iter().enumerate() {
        if !sichtbar.get(i).copied().unwrap_or(true) {
            continue;
        }
        for n in &t.netze {
            for p in &n.punkte {
                lo = vec3(lo.x.min(p.x), lo.y.min(p.y), lo.z.min(p.z));
                hi = vec3(hi.x.max(p.x), hi.y.max(p.y), hi.z.max(p.z));
            }
        }
    }
    (lo.x <= hi.x).then_some((lo, hi))
}

/// Kanten eines Teils: Ränder und Knicke über 20° (einmal je Teil gerechnet).
pub fn kanten(t: &Teil) -> Vec<(Vec3, Vec3)> {
    let mut out = Vec::new();
    for n in &t.netze {
        let normale = |tri: &[u32; 3]| {
            let [a, b, c] = tri.map(|i| n.punkte[i as usize]);
            (b - a).cross(c - a).normalized()
        };
        let mut an: std::collections::HashMap<(u32, u32), Vec<usize>> =
            std::collections::HashMap::new();
        for (k, tri) in n.dreiecke.iter().enumerate() {
            for j in 0..3 {
                let (a, b) = (tri[j], tri[(j + 1) % 3]);
                an.entry((a.min(b), a.max(b))).or_default().push(k);
            }
        }
        let grenze = 20f64.to_radians().cos();
        let mut keys: Vec<_> = an.into_iter().collect();
        keys.sort_by_key(|(k, _)| *k);
        for ((a, b), tris) in keys {
            let zeigen = match tris.as_slice() {
                [s, t] => normale(&n.dreiecke[*s]).dot(normale(&n.dreiecke[*t])) < grenze,
                _ => true,
            };
            if zeigen {
                out.push((n.punkte[a as usize], n.punkte[b as usize]));
            }
        }
    }
    out
}

/// Zeichnet die Szene in `w` × `h` Bildpunkte, `ss` × `ss` Proben je
/// Bildpunkt (2 ruhend, 1 beim Ziehen).
pub fn zeichnen(
    s: &Szene,
    kanten: &[Vec<(Vec3, Vec3)>],
    k: &Camera,
    (w, h): (usize, usize),
    ss: usize,
) -> Bild {
    let ss = ss.max(1);
    let (bw, bh) = (w * ss, h * ss);
    let b = proj(k, bw as f64, bh as f64);
    let mut farbe = vec![s.hinter; bw * bh];
    let mut tiefe = vec![f64::INFINITY; bw * bh];
    let mut teil = vec![u32::MAX; bw * bh];
    let misch = |c: [f32; 3], m: [u8; 3], a: f32| -> [f32; 3] {
        [0, 1, 2].map(|i| c[i] * (1.0 - a) + m[i] as f32 / 255.0 * a)
    };
    for (ti, t) in s.teile.iter().enumerate() {
        if !s.sichtbar.get(ti).copied().unwrap_or(true) {
            continue;
        }
        let gew = s.gewaehlt == Some(ti);
        for n in &t.netze {
            let p: Vec<Option<(f64, f64, f64)>> = n
                .punkte
                .iter()
                .map(|&q| projekt(&b, q, bw as f64, bh as f64))
                .collect();
            for tri in &n.dreiecke {
                let [ia, ib, ic] = tri.map(|i| i as usize);
                let (Some(a), Some(bb), Some(c)) = (p[ia], p[ib], p[ic]) else {
                    continue;
                };
                let [wa, wb, wc] = [ia, ib, ic].map(|i| n.punkte[i]);
                let nrm = (wb - wa).cross(wc - wa);
                if nrm.length() < 1e-9 {
                    continue;
                }
                let i = hell(nrm.normalized(), &b) as f32;
                let c0 = if gew {
                    misch(n.farbe, s.wahl, 0.55)
                } else {
                    n.farbe
                };
                let rgb = c0.map(|v| ((v * i).clamp(0.0, 1.0) * 255.0) as u8);
                dreieck(a, bb, c, (bw, bh), |idx, z| {
                    if z < tiefe[idx] {
                        tiefe[idx] = z;
                        farbe[idx] = rgb;
                        teil[idx] = ti as u32;
                    }
                });
            }
        }
    }
    // Kanten mit Tiefentest, etwas vor den Flächen
    for (ti, ks) in kanten.iter().enumerate() {
        if !s.sichtbar.get(ti).copied().unwrap_or(true) {
            continue;
        }
        for &(p, q) in ks {
            let (Some(a), Some(c)) = (
                projekt(&b, p, bw as f64, bh as f64),
                projekt(&b, q, bw as f64, bh as f64),
            ) else {
                continue;
            };
            linie(a, c, (bw, bh), |idx, z| {
                if z <= tiefe[idx] * 1.002 + 2.0 {
                    farbe[idx] = s.kante;
                }
            });
        }
    }
    // Mitteln auf die Zielgröße
    let mut rgba = vec![0u8; w * h * 4];
    let mut ids = vec![u32::MAX; w * h];
    let mut tiefen = vec![f64::INFINITY; w * h];
    for y in 0..h {
        for x in 0..w {
            let mut sum = [0u32; 3];
            for dy in 0..ss {
                for dx in 0..ss {
                    let c = farbe[(y * ss + dy) * bw + x * ss + dx];
                    for i in 0..3 {
                        sum[i] += c[i] as u32;
                    }
                }
            }
            let o = (y * w + x) * 4;
            for i in 0..3 {
                rgba[o + i] = (sum[i] / (ss * ss) as u32) as u8;
            }
            rgba[o + 3] = 255;
            let m = (y * ss + ss / 2) * bw + x * ss + ss / 2;
            ids[y * w + x] = teil[m];
            tiefen[y * w + x] = tiefe[m];
        }
    }
    Bild {
        w,
        h,
        rgba,
        teil: ids,
        tiefe: tiefen,
    }
}

/// Füllt ein Dreieck im Bild; `set(index, tiefe)` für jeden getroffenen
/// Bildpunkt (Mitte), Tiefe perspektivisch richtig über 1/z.
fn dreieck(
    a: (f64, f64, f64),
    b: (f64, f64, f64),
    c: (f64, f64, f64),
    (w, h): (usize, usize),
    mut set: impl FnMut(usize, f64),
) {
    let fl = (b.0 - a.0) * (c.1 - a.1) - (b.1 - a.1) * (c.0 - a.0);
    if fl.abs() < 1e-12 {
        return;
    }
    let x0 = a.0.min(b.0).min(c.0).floor().max(0.0) as usize;
    let y0 = a.1.min(b.1).min(c.1).floor().max(0.0) as usize;
    let x1 = (a.0.max(b.0).max(c.0).ceil() as i64).clamp(0, w as i64) as usize;
    let y1 = (a.1.max(b.1).max(c.1).ceil() as i64).clamp(0, h as i64) as usize;
    let (ia, ib, ic) = (1.0 / a.2, 1.0 / b.2, 1.0 / c.2);
    for y in y0..y1 {
        let py = y as f64 + 0.5;
        for x in x0..x1 {
            let px = x as f64 + 0.5;
            let w0 = ((b.0 - px) * (c.1 - py) - (b.1 - py) * (c.0 - px)) / fl;
            let w1 = ((c.0 - px) * (a.1 - py) - (c.1 - py) * (a.0 - px)) / fl;
            let w2 = 1.0 - w0 - w1;
            if w0 < 0.0 || w1 < 0.0 || w2 < 0.0 {
                continue;
            }
            let z = 1.0 / (w0 * ia + w1 * ib + w2 * ic);
            set(y * w + x, z);
        }
    }
}

/// Linie mit Tiefe, ein Schritt je Bildpunkt der längeren Richtung.
fn linie(
    a: (f64, f64, f64),
    b: (f64, f64, f64),
    (w, h): (usize, usize),
    mut set: impl FnMut(usize, f64),
) {
    let n = (b.0 - a.0)
        .abs()
        .max((b.1 - a.1).abs())
        .ceil()
        .clamp(1.0, 20_000.0) as usize;
    for i in 0..=n {
        let t = i as f64 / n as f64;
        let x = a.0 + (b.0 - a.0) * t;
        let y = a.1 + (b.1 - a.1) * t;
        if x < 0.0 || y < 0.0 || x >= w as f64 || y >= h as f64 {
            continue;
        }
        let z = 1.0 / ((1.0 - t) / a.2 + t / b.2);
        set(y as usize * w + x as usize, z);
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use sk_ifc::lesen::Netz;

    /// Würfel 1 m mit Ecke im Ursprung.
    pub(crate) fn wuerfel(farbe: [f32; 3], o: Vec3) -> Teil {
        let p: Vec<Vec3> = (0..8)
            .map(|i| {
                o + vec3(
                    (i & 1) as f64 * 1000.0,
                    ((i >> 1) & 1) as f64 * 1000.0,
                    ((i >> 2) & 1) as f64 * 1000.0,
                )
            })
            .collect();
        let q = |a: u32, b: u32, c: u32, d: u32| [[a, b, c], [a, c, d]];
        let mut d = Vec::new();
        for f in [
            q(0, 2, 3, 1),
            q(4, 5, 7, 6),
            q(0, 1, 5, 4),
            q(2, 6, 7, 3),
            q(0, 4, 6, 2),
            q(1, 3, 7, 5),
        ] {
            d.extend(f);
        }
        Teil {
            netze: vec![Netz {
                farbe,
                punkte: p,
                dreiecke: d,
            }],
            ..Teil::default()
        }
    }

    /// Ein Würfel steht in der Bildmitte, der Rand bleibt Hintergrund; der
    /// vordere verdeckt den hinteren; Wahl über den Teilepuffer.
    #[test]
    fn wuerfel_im_bild() {
        let teile = vec![
            wuerfel([1.0, 0.0, 0.0], vec3(0.0, 0.0, 0.0)),
            wuerfel([0.0, 0.0, 1.0], vec3(0.0, 3000.0, 0.0)),
        ];
        let sichtbar = vec![true; 2];
        let s = Szene {
            teile: &teile,
            sichtbar: &sichtbar,
            gewaehlt: None,
            hinter: [255, 255, 255],
            kante: [0, 0, 0],
            wahl: [0, 128, 255],
        };
        let ks: Vec<_> = teile.iter().map(kanten).collect();
        assert_eq!(ks[0].len(), 12, "nur die Würfelkanten, keine Diagonalen");
        // Blick genau entlang +y: der rote verdeckt den blauen
        let k = Camera::looking_at(vec3(500.0, -5500.0, 500.0), vec3(500.0, 500.0, 500.0), 45.0);
        let b = zeichnen(&s, &ks, &k, (200, 150), 2);
        let px = |x: usize, y: usize| b.rgba[(y * 200 + x) * 4..][..3].to_vec();
        assert_eq!(px(2, 2), vec![255, 255, 255]);
        let m = px(100, 75);
        assert!(m[0] > 100 && m[2] == 0, "rot vorn: {m:?}");
        assert_eq!(b.teil_bei(100.0, 75.0), Some(0));
        assert_eq!(b.teil_bei(2.0, 2.0), None);
        // Punkt unter der Maus: auf der Vorderseite des roten
        let p = b.punkt_bei(&k, 100.5, 75.5).unwrap();
        assert!(p.y.abs() < 5.0 && (p.x - 500.0).abs() < 50.0, "{p:?}");
        assert!(b.punkt_bei(&k, 2.0, 2.0).is_none());
        // Gleiche Abbildung wie die Ansicht
        let (x, y) = k.project(vec3(0.0, 0.0, 1000.0), 200.0, 150.0).unwrap();
        let q = projekt(
            &proj(&k, 200.0, 150.0),
            vec3(0.0, 0.0, 1000.0),
            200.0,
            150.0,
        )
        .unwrap();
        assert!((q.0 - x).abs() < 1e-9 && (q.1 - y).abs() < 1e-9);
        // Von hinten: blau vorn
        let k = Camera::looking_at(vec3(500.0, 9500.0, 500.0), vec3(500.0, 3500.0, 500.0), 45.0);
        let b = zeichnen(&s, &ks, &k, (200, 150), 2);
        assert_eq!(b.teil_bei(100.0, 75.0), Some(1));
        assert_eq!(
            zeichnen(&s, &ks, &k, (200, 150), 1).teil_bei(100.0, 75.0),
            Some(1)
        );
    }

    /// Passende Kamera: der ganze Kasten im Bild, die Mitte in der Mitte.
    #[test]
    fn passende_kamera() {
        let (lo, hi) = (vec3(0.0, 0.0, 0.0), vec3(10000.0, 8000.0, 6000.0));
        let k = passend(lo, hi);
        let (w, h) = (800.0, 600.0);
        let m = k.project((lo + hi) * 0.5, w, h).unwrap();
        assert!((m.0 - 400.0).abs() < 1e-6 && (m.1 - 300.0).abs() < 1e-6);
        for i in 0..8 {
            let p = vec3(
                (i & 1) as f64 * 10000.0,
                ((i >> 1) & 1) as f64 * 8000.0,
                ((i >> 2) & 1) as f64 * 6000.0,
            );
            let (x, y) = k.project(p, w, h).unwrap();
            assert!(x > 0.0 && x < w && y > 0.0 && y < h, "{i}: {x} {y}");
        }
        assert!(k.forward().z < 0.0, "von oben");
    }
}
