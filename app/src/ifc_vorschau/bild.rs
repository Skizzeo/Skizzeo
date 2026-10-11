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
/// relativ zum Auge, für ein Bild `w` × `h`.
struct Proj {
    vp: Mat4,
    auge: Vec3,
    vor: Vec3,
    rechts: Vec3,
    oben: Vec3,
    near: f64,
    w: f64,
    h: f64,
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
        w,
        h,
    }
}

/// Punkt relativ zum Auge und seine Tiefe entlang der Blickrichtung.
#[derive(Clone, Copy, Debug, Default)]
struct Ecke {
    r: Vec3,
    z: f64,
}

impl Proj {
    fn ecke(&self, p: Vec3) -> Ecke {
        let r = p - self.auge;
        Ecke {
            r,
            z: r.dot(self.vor),
        }
    }

    /// Bildpunkt (x nach rechts, y nach unten) und Kehrwert der Tiefe; nur
    /// für Ecken vor der Nahebene.
    fn bildpunkt(&self, e: Ecke) -> (f64, f64, f64) {
        let c = self.vp.mul_vec4([e.r.x, e.r.y, e.r.z, 1.0]);
        (
            (c[0] / c[3] * 0.5 + 0.5) * self.w,
            (0.5 - c[1] / c[3] * 0.5) * self.h,
            1.0 / e.z,
        )
    }

    /// Ebene, an der geschnitten wird: knapp hinter der Nahebene.
    fn schnitt(&self) -> f64 {
        self.near * 1.0001
    }
}

/// Bildpunkt und Tiefe; `None` vor der Nahebene.
#[cfg(test)]
fn projekt(b: &Proj, p: Vec3) -> Option<(f64, f64, f64)> {
    let e = b.ecke(p);
    (e.z > b.near).then(|| {
        let (x, y, _) = b.bildpunkt(e);
        (x, y, e.z)
    })
}

/// Schneidet ein Dreieck an der Ebene `z = near` (Sutherland–Hodgman):
/// bis zu vier Ecken vor ihr, ihre Zahl als Ergebnis.
fn schneiden(e: [Ecke; 3], near: f64, out: &mut [Ecke; 4]) -> usize {
    let mut n = 0;
    for i in 0..3 {
        let (a, b) = (e[i], e[(i + 1) % 3]);
        if a.z >= near {
            out[n] = a;
            n += 1;
        }
        if (a.z >= near) != (b.z >= near) {
            let t = (near - a.z) / (b.z - a.z);
            out[n] = Ecke {
                r: a.r + (b.r - a.r) * t,
                z: near,
            };
            n += 1;
        }
    }
    n
}

/// Strecke vor der Nahebene, `None`, wenn sie ganz dahinter liegt.
fn strecke(a: Ecke, b: Ecke, near: f64) -> Option<(Ecke, Ecke)> {
    let schnitt = |a: Ecke, b: Ecke| {
        let t = (near - a.z) / (b.z - a.z);
        Ecke {
            r: a.r + (b.r - a.r) * t,
            z: near,
        }
    };
    match (a.z >= near, b.z >= near) {
        (true, true) => Some((a, b)),
        (true, false) => Some((a, schnitt(a, b))),
        (false, true) => Some((schnitt(b, a), b)),
        (false, false) => None,
    }
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
#[derive(Default)]
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
#[cfg(test)]
pub fn zeichnen(
    s: &Szene,
    kanten: &[Vec<(Vec3, Vec3)>],
    k: &Camera,
    groesse: (usize, usize),
    ss: usize,
) -> Bild {
    let mut z = Zeichner::default();
    z.zeichnen(s, kanten, k, groesse, ss);
    z.bild
}

/// Zeichnet Bild um Bild in dieselben Puffer (beim Drehen kein Anlegen je
/// Bild).
#[derive(Default)]
pub struct Zeichner {
    farbe: Vec<[u8; 3]>,
    /// Kehrwert der Tiefe je Probe, 0 = nichts.
    kehr: Vec<f64>,
    teil: Vec<u32>,
    ecken: Vec<Ecke>,
    pub bild: Bild,
}

fn leeren<T: Clone>(v: &mut Vec<T>, n: usize, x: T) {
    v.clear();
    v.resize(n, x);
}

impl Zeichner {
    /// Wie [`zeichnen`], das Ergebnis in [`Zeichner::bild`].
    pub fn zeichnen(
        &mut self,
        s: &Szene,
        kanten: &[Vec<(Vec3, Vec3)>],
        k: &Camera,
        (w, h): (usize, usize),
        ss: usize,
    ) -> &Bild {
        let ss = ss.max(1);
        let (bw, bh) = (w * ss, h * ss);
        let b = proj(k, bw as f64, bh as f64);
        let near = b.schnitt();
        let Zeichner {
            farbe,
            kehr,
            teil,
            ecken,
            bild,
        } = self;
        leeren(farbe, bw * bh, s.hinter);
        leeren(kehr, bw * bh, 0.0);
        leeren(teil, bw * bh, u32::MAX);
        let misch = |c: [f32; 3], m: [u8; 3], a: f32| -> [f32; 3] {
            [0, 1, 2].map(|i| c[i] * (1.0 - a) + m[i] as f32 / 255.0 * a)
        };
        for (ti, t) in s.teile.iter().enumerate() {
            if !s.sichtbar.get(ti).copied().unwrap_or(true) {
                continue;
            }
            let gew = s.gewaehlt == Some(ti);
            for n in &t.netze {
                ecken.clear();
                ecken.extend(n.punkte.iter().map(|&q| b.ecke(q)));
                let c0 = if gew {
                    misch(n.farbe, s.wahl, 0.55)
                } else {
                    n.farbe
                };
                for tri in &n.dreiecke {
                    let e = tri.map(|i| ecken[i as usize]);
                    if e.iter().all(|e| e.z < near) {
                        continue;
                    }
                    let nrm = (e[1].r - e[0].r).cross(e[2].r - e[0].r);
                    if nrm.length() < 1e-9 {
                        continue;
                    }
                    let i = hell(nrm.normalized(), &b) as f32;
                    let rgb = c0.map(|v| ((v * i).clamp(0.0, 1.0) * 255.0) as u8);
                    let mut set = |idx: usize, iz: f64| {
                        if iz > kehr[idx] {
                            kehr[idx] = iz;
                            farbe[idx] = rgb;
                            teil[idx] = ti as u32;
                        }
                    };
                    if e.iter().all(|e| e.z >= near) {
                        let [p, q, r] = e.map(|e| b.bildpunkt(e));
                        dreieck(p, q, r, (bw, bh), &mut set);
                    } else {
                        // An der Nahebene geschnitten: Fächer aus der Vieleck
                        let mut o = [Ecke::default(); 4];
                        let m = schneiden(e, near, &mut o);
                        let p = o.map(|e| b.bildpunkt(e));
                        for j in 1..m.saturating_sub(1) {
                            dreieck(p[0], p[j], p[j + 1], (bw, bh), &mut set);
                        }
                    }
                }
            }
        }
        // Kanten mit Tiefentest, etwas vor den Flächen
        for (ti, ks) in kanten.iter().enumerate() {
            if !s.sichtbar.get(ti).copied().unwrap_or(true) {
                continue;
            }
            for &(p, q) in ks {
                let Some((a, c)) = strecke(b.ecke(p), b.ecke(q), near) else {
                    continue;
                };
                linie(b.bildpunkt(a), b.bildpunkt(c), (bw, bh), |idx, iz| {
                    let zb = kehr[idx];
                    if zb == 0.0 || 1.0 / iz <= 1.002 / zb + 2.0 {
                        farbe[idx] = s.kante;
                    }
                });
            }
        }
        // Mitteln auf die Zielgröße
        (bild.w, bild.h) = (w, h);
        leeren(&mut bild.rgba, w * h * 4, 255);
        leeren(&mut bild.teil, w * h, u32::MAX);
        leeren(&mut bild.tiefe, w * h, f64::INFINITY);
        let tiefe = |iz: f64| if iz > 0.0 { 1.0 / iz } else { f64::INFINITY };
        if ss == 1 {
            for (i, c) in farbe.iter().enumerate() {
                bild.rgba[i * 4..i * 4 + 3].copy_from_slice(c);
            }
            bild.teil.copy_from_slice(teil);
            for (d, &iz) in bild.tiefe.iter_mut().zip(kehr.iter()) {
                *d = tiefe(iz);
            }
            return bild;
        }
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
                for (d, s) in bild.rgba[o..o + 3].iter_mut().zip(sum) {
                    *d = (s / (ss * ss) as u32) as u8;
                }
                let m = (y * ss + ss / 2) * bw + x * ss + ss / 2;
                bild.teil[y * w + x] = teil[m];
                bild.tiefe[y * w + x] = tiefe(kehr[m]);
            }
        }
        bild
    }
}

/// Unterteilung eines Bildpunkts für die Ecken der Dreiecke (Festkomma).
const SUB: i64 = 256;
/// Weiter draußen liegende Ecken (Bildpunkte) gibt es nach dem Schneiden an
/// der Nahebene nicht; was darüber liegt, wird nicht gezeichnet.
const WEIT: f64 = (1u64 << 40) as f64;

/// Wert der Kante `p` → `q` am Punkt `r` (Festkomma, exakt).
fn kante(p: (i64, i64), q: (i64, i64), r: (i64, i64)) -> i128 {
    (q.0 - p.0) as i128 * (r.1 - p.1) as i128 - (q.1 - p.1) as i128 * (r.0 - p.0) as i128
}

/// Ganze Zahl für die Spannen: `i64`, solange die Ecken im üblichen Bild
/// liegen (schnell), sonst `i128` (Ecken weit draußen nach dem Schneiden).
trait Ganz:
    Copy
    + Ord
    + std::ops::Add<Output = Self>
    + std::ops::Sub<Output = Self>
    + std::ops::Mul<Output = Self>
    + std::ops::Neg<Output = Self>
{
    fn von(x: i64) -> Self;
    fn ab(self, d: Self) -> Self;
    fn breit(self) -> i128;
}

impl Ganz for i64 {
    fn von(x: i64) -> i64 {
        x
    }
    fn ab(self, d: i64) -> i64 {
        self.div_euclid(d)
    }
    fn breit(self) -> i128 {
        self as i128
    }
}

impl Ganz for i128 {
    fn von(x: i64) -> i128 {
        x as i128
    }
    fn ab(self, d: i128) -> i128 {
        self.div_euclid(d)
    }
    fn breit(self) -> i128 {
        self
    }
}

/// Kante von Ecke zu Ecke (Festkomma) mit ihrer Schwelle: 0, wenn Punkte
/// genau auf ihr dazugehören, sonst 1.
type Kante = ((i64, i64), (i64, i64), i64);

/// Spalten `x0..=x1` je Zeile `y0..=y1`, in denen alle drei Kanten
/// mindestens ihre Schwelle erreichen (Bildpunktmitten).
fn spannen<G: Ganz>(
    kanten: &[Kante; 3],
    (y0, y1): (i64, i64),
    w: usize,
    mut zeile: impl FnMut(i64, usize, usize),
) {
    let g = G::von;
    for y in y0..=y1 {
        let py = y * SUB + SUB / 2;
        let (mut lo, mut hi) = (g(0), g(w as i64 - 1));
        for &(p, q, schwelle) in kanten {
            // Wert an Spalte x: e0 + x·s
            let e0 =
                (g(q.0) - g(p.0)) * (g(py) - g(p.1)) - (g(q.1) - g(p.1)) * (g(SUB / 2) - g(p.0));
            let s = -(g(q.1) - g(p.1)) * g(SUB);
            let t = g(schwelle);
            if s > g(0) {
                lo = lo.max(-(e0 - t).ab(s));
            } else if s < g(0) {
                hi = hi.min((e0 - t).ab(-s));
            } else if e0 < t {
                hi = g(-1);
            }
        }
        if lo <= hi {
            zeile(y, lo.breit() as usize, hi.breit() as usize);
        }
    }
}

/// Füllt ein Dreieck im Bild; `set(index, 1/tiefe)` für jeden getroffenen
/// Bildpunkt (Mitte). Die Ecken liegen im Festkomma, die Kantenwerte sind
/// exakt und haben je Kante eine feste Richtung: Ein Bildpunkt genau auf
/// einer gemeinsamen Kante gehört genau einem der beiden Dreiecke (Regel
/// wie „oben links“), dazwischen bleibt keine Lücke. Je Zeile wird nur die
/// Spanne innerhalb des Dreiecks besucht; die Tiefe läuft schrittweise.
fn dreieck(
    a: (f64, f64, f64),
    b: (f64, f64, f64),
    c: (f64, f64, f64),
    (w, h): (usize, usize),
    mut set: impl FnMut(usize, f64),
) {
    let fest = |p: (f64, f64, f64)| {
        (p.0.abs() < WEIT && p.1.abs() < WEIT).then(|| {
            let f = SUB as f64;
            (((p.0 * f).round() as i64, (p.1 * f).round() as i64), p.2)
        })
    };
    let (Some(a), Some(mut b), Some(mut c)) = (fest(a), fest(b), fest(c)) else {
        return;
    };
    let mut fl = kante(a.0, b.0, c.0);
    if fl == 0 {
        return;
    }
    if fl < 0 {
        std::mem::swap(&mut b, &mut c);
        fl = -fl;
    }
    // Tiefe: 1/z ist im Bild linear, iz = gz0·x + gz1·y + gz2 (Bildpunkte)
    let f = SUB as f64;
    let p = |q: ((i64, i64), f64)| (q.0 .0 as f64 / f, q.0 .1 as f64 / f, q.1);
    let (af, bf, cf) = (p(a), p(b), p(c));
    let flf = fl as f64 / (f * f);
    let g0 = [
        (bf.1 - cf.1) / flf,
        (cf.0 - bf.0) / flf,
        (bf.0 * cf.1 - bf.1 * cf.0) / flf,
    ];
    let g1 = [
        (cf.1 - af.1) / flf,
        (af.0 - cf.0) / flf,
        (cf.0 * af.1 - cf.1 * af.0) / flf,
    ];
    let g2 = [-g0[0] - g1[0], -g0[1] - g1[1], 1.0 - g0[2] - g1[2]];
    let gz = [0, 1, 2].map(|i| g0[i] * af.2 + g1[i] * bf.2 + g2[i] * cf.2);
    // Kanten gegen den Uhrzeigersinn im Bild (alle Werte ≥ 0 innen); eine
    // Kante zählt ihre eigenen Punkte nur, wenn sie nach unten läuft oder
    // waagrecht nach links: die Gegenkante des Nachbarn tut es dann nicht
    let kanten = [(a.0, b.0), (b.0, c.0), (c.0, a.0)].map(|(p, q)| {
        let (dx, dy) = (q.0 - p.0, q.1 - p.1);
        let eigen = dy > 0 || (dy == 0 && dx < 0);
        (p, q, if eigen { 0 } else { 1 })
    });
    let ys = [a.0 .1, b.0 .1, c.0 .1];
    let (lo_y, hi_y) = (*ys.iter().min().unwrap(), *ys.iter().max().unwrap());
    let zeilen = (
        (lo_y.div_euclid(SUB) - 1).max(0),
        (hi_y.div_euclid(SUB) + 1).min(h as i64 - 1),
    );
    let fuellen = |y: i64, x0: usize, x1: usize| {
        let reihe = y as usize * w;
        let mut iz = gz[0] * (x0 as f64 + 0.5) + gz[1] * (y as f64 + 0.5) + gz[2];
        for x in x0..=x1 {
            set(reihe + x, iz);
            iz += gz[0];
        }
    };
    // Kantenwerte bis 2·(2^30)²: in i64, sonst breit rechnen
    let klein = [a.0, b.0, c.0]
        .iter()
        .all(|p| p.0.abs() < 1 << 29 && p.1.abs() < 1 << 29);
    if klein {
        spannen::<i64>(&kanten, zeilen, w, fuellen);
    } else {
        spannen::<i128>(&kanten, zeilen, w, fuellen);
    }
}

/// Linie mit Tiefe, ein Schritt je Bildpunkt der längeren Richtung;
/// `set(index, 1/tiefe)`.
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
        if !(x >= 0.0 && y >= 0.0 && x < w as f64 && y < h as f64) {
            continue;
        }
        set(y as usize * w + x as usize, a.2 + (b.2 - a.2) * t);
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
        let q = projekt(&proj(&k, 200.0, 150.0), vec3(0.0, 0.0, 1000.0)).unwrap();
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

    /// Review B5 zu #83: Dreiecke, die durch die Nahebene gehen, werden
    /// geschnitten statt verworfen. Im Würfel stehend sieht man ringsum
    /// seine Wände, ohne Loch.
    #[test]
    fn nahebene_schneidet() {
        let teile = vec![wuerfel([0.5, 0.5, 0.5], vec3(0.0, 0.0, 0.0))];
        let s = Szene {
            teile: &teile,
            sichtbar: &[true],
            gewaehlt: None,
            hinter: [255, 255, 255],
            kante: [0, 0, 0],
            wahl: [0, 128, 255],
        };
        let k = Camera::looking_at(vec3(500.0, 100.0, 500.0), vec3(500.0, 600.0, 500.0), 45.0);
        for ss in [1, 2] {
            let b = zeichnen(&s, &[], &k, (120, 90), ss);
            assert!(b.teil.iter().all(|&t| t == 0), "Loch im Bild ({ss}×)");
            assert!(b.tiefe.iter().all(|d| d.is_finite()));
        }
        let mut e = [Ecke::default(); 4];
        let z = |z: f64| Ecke {
            r: vec3(0.0, z, 0.0),
            z,
        };
        assert_eq!(schneiden([z(-5.0), z(5.0), z(5.0)], 1.0, &mut e), 4);
        assert_eq!(schneiden([z(-5.0), z(-5.0), z(5.0)], 1.0, &mut e), 3);
        assert!(strecke(z(-1.0), z(0.5), 1.0).is_none());
        assert_eq!(strecke(z(-1.0), z(3.0), 1.0).unwrap().0.z, 1.0);
    }

    /// Nachprüfung N2 zu #83: eine Wand aus vielen Dreiecken, frontal und
    /// parallel gesehen, hat zwischen den Dreiecken keine Lücke, auch wenn
    /// Kanten genau auf Bildpunktmitten fallen; kein Bildpunkt doppelt.
    #[test]
    fn dicht_an_gemeinsamen_kanten() {
        // 12 × 9 Felder zu 100 mm in der Ebene y = 0, je zwei Dreiecke
        let (nx, nz) = (12u32, 9u32);
        let mut netz = sk_ifc::lesen::Netz {
            farbe: [0.5; 3],
            ..Default::default()
        };
        for k in 0..=nz {
            for i in 0..=nx {
                netz.punkte
                    .push(vec3(i as f64 * 100.0, 0.0, k as f64 * 100.0));
            }
        }
        let n = |i: u32, k: u32| k * (nx + 1) + i;
        for k in 0..nz {
            for i in 0..nx {
                let (a, b, c, d) = (n(i, k), n(i + 1, k), n(i + 1, k + 1), n(i, k + 1));
                netz.dreiecke.push([a, b, c]);
                netz.dreiecke.push([a, c, d]);
            }
        }
        let teile = vec![Teil {
            netze: vec![netz],
            ..Teil::default()
        }];
        let s = Szene {
            teile: &teile,
            sichtbar: &[true],
            gewaehlt: None,
            hinter: [255, 255, 255],
            kante: [0, 0, 0],
            wahl: [0, 0, 0],
        };
        let ziel = vec3(600.0, 0.0, 450.0);
        for schritt in 0..32 {
            let mut k;
            if schritt % 2 == 1 {
                // parallel, ein Feld genau 8 Bildpunkte, Ecken auf Mitten
                // oder Rändern der Bildpunkte
                let d = (schritt / 2) as f64 * 3.125;
                k = Camera::looking_at(ziel + vec3(d, -3000.0, d), ziel + vec3(d, 0.0, d), 40.0);
                k.ortho = Some(750.0);
            } else {
                let d = schritt as f64 * 0.37;
                k = Camera::looking_at(
                    ziel + vec3(d, -2000.0 - 40.0 * d, d * 0.5),
                    ziel + vec3(d, 0.0, d * 0.5),
                    40.0,
                );
            }
            let (w, h) = (160, 120);
            let mut treffer = vec![0u8; w * h];
            let p = proj(&k, w as f64, h as f64);
            for tri in &teile[0].netze[0].dreiecke {
                let e = tri.map(|i| p.bildpunkt(p.ecke(teile[0].netze[0].punkte[i as usize])));
                dreieck(e[0], e[1], e[2], (w, h), |i, _| treffer[i] += 1);
            }
            assert!(treffer.iter().all(|&t| t <= 1), "doppelt ({schritt})");
            // Innen (Kasten der Treffer, 2 Bildpunkte eingerückt) jeder einmal
            let xs = (0..w * h).filter(|&i| treffer[i] > 0);
            let (mut x0, mut x1, mut y0, mut y1) = (w, 0, h, 0);
            for i in xs {
                (x0, x1, y0, y1) = (x0.min(i % w), x1.max(i % w), y0.min(i / w), y1.max(i / w));
            }
            let mut luecken = 0;
            for y in y0 + 2..y1.saturating_sub(1) {
                for x in x0 + 2..x1.saturating_sub(1) {
                    luecken += usize::from(treffer[y * w + x] == 0);
                }
            }
            assert_eq!(luecken, 0, "Lücken ({schritt})");
            let b = zeichnen(&s, &[], &k, (w, h), 1);
            assert!(b.teil.iter().filter(|&&t| t == 0).count() > 1000);
        }
    }

    /// Ecken weit draußen (nach dem Schneiden an der Nahebene) rechnen breit
    /// und treffen dieselben Bildpunkte; jenseits von `WEIT` nichts.
    #[test]
    fn ecken_weit_draussen() {
        for weit in [1.0e3, 1.0e7, 1.0e11] {
            let mut n = vec![0u8; 50 * 40];
            let (a, b, c) = ((-weit, -weit, 1.0), (weit, -weit, 1.0), (0.0, weit, 1.0));
            dreieck(a, b, c, (50, 40), |i, iz| {
                assert!((iz - 1.0).abs() < 1e-6);
                n[i] += 1;
            });
            assert!(n.iter().all(|&k| k == 1), "{weit}");
        }
        let mut n = 0;
        dreieck(
            (0.0, 0.0, 1.0),
            (WEIT * 2.0, 0.0, 1.0),
            (0.0, 30.0, 1.0),
            (50, 40),
            |_, _| n += 1,
        );
        assert_eq!(n, 0);
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
