//! 3D-Bild der IFC-Vorschau in Software (ifc/plan.md §6): Tiefenpuffer,
//! flache Schattierung je Dreieck, Kanten an Knicken und Rändern. Das
//! Vorschaufenster ist ein Seitenfenster ohne OpenGL; das Bild entsteht hier
//! als Pixel. Doppelt abgetastet und gemittelt, dazu ein Puffer mit dem Teil
//! je Bildpunkt zum Wählen mit der Maus.

use sk_ifc::lesen::Teil;
use sk_math::{vec3, Vec3};

/// Kamera, die um einen Zielpunkt kreist (wie die Ansicht: Ziehen dreht,
/// Rad nähert).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Kamera {
    pub ziel: Vec3,
    /// Abstand des Auges vom Ziel (mm).
    pub abstand: f64,
    /// Drehung um die Hochachse (rad), 0 = Blick nach +y.
    pub dreh: f64,
    /// Neigung (rad), positiv = von oben.
    pub neigung: f64,
    /// Öffnungswinkel senkrecht (rad).
    pub fov: f64,
}

/// Lage des Auges und Bildachsen.
struct Basis {
    auge: Vec3,
    rechts: Vec3,
    oben: Vec3,
    vor: Vec3,
    f: f64,
}

impl Kamera {
    /// Blick von vorn links oben auf den ganzen Kasten `lo`..`hi`.
    pub fn passend(lo: Vec3, hi: Vec3) -> Kamera {
        let fov = 40f64.to_radians();
        let r = ((hi - lo).length() * 0.5).max(1000.0);
        Kamera {
            ziel: (lo + hi) * 0.5,
            abstand: r / (fov * 0.5).sin() * 1.05,
            dreh: (-35f64).to_radians(),
            neigung: 25f64.to_radians(),
            fov,
        }
    }

    fn basis(&self, h: f64) -> Basis {
        let (sd, cd) = self.dreh.sin_cos();
        let (sn, cn) = self.neigung.sin_cos();
        // Blickrichtung vom Auge zum Ziel
        let vor = vec3(sd * cn, cd * cn, -sn);
        let rechts = vec3(cd, -sd, 0.0);
        let oben = rechts.cross(vor);
        Basis {
            auge: self.ziel - vor * self.abstand,
            rechts,
            oben,
            vor,
            f: h * 0.5 / (self.fov * 0.5).tan(),
        }
    }

    /// Bildpunkt (x nach rechts, y nach unten) und Tiefe; `None` hinter dem
    /// Auge.
    pub fn projizieren(&self, p: Vec3, w: f64, h: f64) -> Option<(f64, f64, f64)> {
        let b = self.basis(h);
        projekt(&b, p, w, h)
    }

    /// Ziehen mit der Maus um `dx`, `dy` Bildpunkte.
    pub fn drehen(&mut self, dx: f64, dy: f64) {
        self.dreh += dx * 0.008;
        self.neigung = (self.neigung + dy * 0.008).clamp(-1.45, 1.45);
    }

    /// Rad: näher (positiv) oder weiter.
    pub fn naehern(&mut self, schritte: f64) {
        self.abstand = (self.abstand * 0.85f64.powf(schritte)).clamp(500.0, 2.0e6);
    }

    /// Ziel im Bild verschieben (rechte Maustaste).
    pub fn schieben(&mut self, dx: f64, dy: f64, h: f64) {
        let b = self.basis(h);
        let k = self.abstand / b.f;
        self.ziel = self.ziel - b.rechts * (dx * k) + b.oben * (dy * k);
    }
}

fn projekt(b: &Basis, p: Vec3, w: f64, h: f64) -> Option<(f64, f64, f64)> {
    let d = p - b.auge;
    let z = d.dot(b.vor);
    if z < 10.0 {
        return None;
    }
    let x = w * 0.5 + d.dot(b.rechts) * b.f / z;
    let y = h * 0.5 - d.dot(b.oben) * b.f / z;
    Some((x, y, z))
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
}

/// Licht von links oben hinter dem Betrachter, fest zur Kamera.
fn hell(n: Vec3, b: &Basis) -> f64 {
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
    k: &Kamera,
    (w, h): (usize, usize),
    ss: usize,
) -> Bild {
    let ss = ss.max(1);
    let (bw, bh) = (w * ss, h * ss);
    let b = k.basis(bh as f64);
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
            ids[y * w + x] = teil[(y * ss + ss / 2) * bw + x * ss + ss / 2];
        }
    }
    Bild {
        w,
        h,
        rgba,
        teil: ids,
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
        let mut k = Kamera::passend(vec3(0.0, 0.0, 0.0), vec3(1000.0, 4000.0, 1000.0));
        k.ziel = vec3(500.0, 500.0, 500.0);
        k.dreh = 0.0;
        k.neigung = 0.0;
        k.abstand = 6000.0;
        let b = zeichnen(&s, &ks, &k, (200, 150), 2);
        let px = |x: usize, y: usize| b.rgba[(y * 200 + x) * 4..][..3].to_vec();
        assert_eq!(px(2, 2), vec![255, 255, 255]);
        let m = px(100, 75);
        assert!(m[0] > 100 && m[2] == 0, "rot vorn: {m:?}");
        assert_eq!(b.teil_bei(100.0, 75.0), Some(0));
        assert_eq!(b.teil_bei(2.0, 2.0), None);
        // Von hinten: blau vorn
        k.dreh = std::f64::consts::PI;
        k.ziel = vec3(500.0, 3500.0, 500.0);
        let b = zeichnen(&s, &ks, &k, (200, 150), 2);
        assert_eq!(b.teil_bei(100.0, 75.0), Some(1));
    }

    #[test]
    fn kamera_bedienen() {
        let mut k = Kamera::passend(vec3(0.0, 0.0, 0.0), vec3(10000.0, 8000.0, 6000.0));
        let (w, h) = (800.0, 600.0);
        let m = k.projizieren(k.ziel, w, h).unwrap();
        assert!((m.0 - 400.0).abs() < 1e-6 && (m.1 - 300.0).abs() < 1e-6);
        // Alle Ecken im Bild
        for i in 0..8 {
            let p = vec3(
                (i & 1) as f64 * 10000.0,
                ((i >> 1) & 1) as f64 * 8000.0,
                ((i >> 2) & 1) as f64 * 6000.0,
            );
            let (x, y, _) = k.projizieren(p, w, h).unwrap();
            assert!(x > 0.0 && x < w && y > 0.0 && y < h, "{i}: {x} {y}");
        }
        let a = k.abstand;
        k.naehern(1.0);
        assert!(k.abstand < a);
        k.drehen(0.0, 10_000.0);
        assert!(k.neigung <= 1.45);
        let z = k.ziel;
        k.schieben(100.0, 0.0, h);
        assert!((k.ziel - z).length() > 0.0);
    }
}
