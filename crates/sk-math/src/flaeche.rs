//! Ebene Flächen aus Umrissen (Grundriss G3, Jörn 11.10. 16:53): Die
//! Umrisse der Wände eines Geschosses werden vereinigt; jede Fläche, die sie
//! ganz umschließen, ist ein Bereich (später ein Raum). Alles auf z = 0, in mm.
//!
//! Vorgehen: Kanten aller Umrisse an Schnitt- und Berührpunkten teilen, die
//! Teilstücke behalten, die links innen und rechts außen liegen, Punkte
//! innerhalb der Toleranz zusammenlegen und die Teilstücke zu Ringen
//! verbinden, an jeder Ecke mit der am weitesten links abbiegenden
//! Fortsetzung. Ringe gegen den Uhrzeigersinn sind Außenränder der Vereinigung,
//! Ringe im Uhrzeigersinn umschließen einen Bereich.

use std::collections::{HashMap, HashSet};

use crate::polygon::signed_area;
use crate::{vec3, Vec3};

/// Herkunft einer Randkante: Umriss `poly`, dessen Kante `kante` (von Punkt
/// `kante` zu Punkt `kante + 1` in der übergebenen Reihenfolge).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Herkunft {
    pub poly: usize,
    pub kante: usize,
}

/// Geschlossener Ring ohne Schlusspunkt; Kante `i` läuft von `punkte[i]` zu
/// `punkte[i + 1]` und stammt von `herkunft[i]`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Ring {
    pub punkte: Vec<Vec3>,
    pub herkunft: Vec<Herkunft>,
}

impl Ring {
    /// Fläche ohne Vorzeichen (mm²).
    pub fn flaeche(&self) -> f64 {
        signed_area(&self.punkte).abs()
    }

    /// Liegt `p` im Ring (Richtung egal)?
    pub fn enthaelt(&self, p: Vec3) -> bool {
        enthaelt(&self.punkte, p)
    }

    /// Derselbe Ring andersherum; jede Kante behält ihre Herkunft.
    pub fn umgekehrt(&self) -> Ring {
        let n = self.punkte.len();
        Ring {
            punkte: self.punkte.iter().rev().copied().collect(),
            herkunft: (0..n).map(|i| self.herkunft[(2 * n - 2 - i) % n]).collect(),
        }
    }
}

/// Von Umrissen ganz umschlossene Fläche. Der Rand läuft gegen den
/// Uhrzeigersinn, die Inseln (frei stehende Umrisse darin) im Uhrzeigersinn:
/// das Innere des Bereichs liegt immer links.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Bereich {
    pub rand: Ring,
    pub inseln: Vec<Ring>,
}

impl Bereich {
    /// Fläche ohne die Inseln (mm²).
    pub fn flaeche(&self) -> f64 {
        self.rand.flaeche() - self.inseln.iter().map(Ring::flaeche).sum::<f64>()
    }

    /// Liegt `p` im Bereich und in keiner Insel?
    pub fn enthaelt(&self, p: Vec3) -> bool {
        self.rand.enthaelt(p) && !self.inseln.iter().any(|r| r.enthaelt(p))
    }
}

/// Ergebnis von [`anordnung`]: Außenränder der Vereinigung (gegen den
/// Uhrzeigersinn) und die umschlossenen Bereiche.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Anordnung {
    pub aussen: Vec<Ring>,
    pub bereiche: Vec<Bereich>,
}

/// Punkt in Polygon (gerade/ungerade, Richtung egal).
pub fn enthaelt(pts: &[Vec3], p: Vec3) -> bool {
    let n = pts.len();
    let mut innen = false;
    for i in 0..n {
        let (a, b) = (pts[i], pts[(i + 1) % n]);
        if (a.y > p.y) != (b.y > p.y) {
            let x = a.x + (p.y - a.y) / (b.y - a.y) * (b.x - a.x);
            if p.x < x {
                innen = !innen;
            }
        }
    }
    innen
}

/// Vereinigt die Umrisse und liefert ihre Außenränder und die umschlossenen
/// Bereiche. Punkte näher als `tol` gelten als gleich, ein Punkt näher als
/// `tol` an einer Kante als auf ihr; so schließt auch eine Wand, die einen
/// Bruchteil davon vor der nächsten endet.
pub fn anordnung(umrisse: &[Vec<Vec3>], tol: f64) -> Anordnung {
    let tol = tol.max(1e-6);
    let polys = gegen_uhrzeiger(umrisse, tol);
    let kanten = teilen(&polys, tol);
    let mut ecken = Ecken::new(tol);
    let probe = 4.0 * tol;
    let mut gesehen = HashSet::new();
    let mut rand: Vec<(usize, usize, Herkunft)> = Vec::new();
    for (a, b, h) in kanten {
        let (ia, ib) = (ecken.id(a), ecken.id(b));
        if ia == ib || !gesehen.insert((ia, ib)) {
            continue;
        }
        let (a, b) = (ecken.p[ia], ecken.p[ib]);
        let d = (b - a).normalized();
        let m = (a + b) * 0.5;
        let links = m + vec3(-d.y, d.x, 0.0) * probe;
        let rechts = m - vec3(-d.y, d.x, 0.0) * probe;
        let drin = |q: Vec3| polys.iter().any(|(p, _)| enthaelt(p, q));
        if drin(links) && !drin(rechts) {
            rand.push((ia, ib, h));
        }
    }
    let ringe = verbinden(&ecken.p, &rand);
    let (mut aussen, mut loecher) = (Vec::new(), Vec::new());
    for r in ringe {
        let f = signed_area(&r.punkte);
        if f.abs() < tol * tol {
            continue;
        }
        if f > 0.0 {
            aussen.push(r);
        } else {
            loecher.push(r);
        }
    }
    let mut bereiche: Vec<Bereich> = loecher
        .iter()
        .map(|l| Bereich {
            rand: l.umgekehrt(),
            inseln: Vec::new(),
        })
        .collect();
    // Jeder Außenrand, der in einem Bereich liegt, ist dort eine Insel (im
    // kleinsten, der ihn enthält; Prüfpunkt knapp außerhalb seiner ersten Kante)
    for o in &aussen {
        let (a, b) = (o.punkte[0], o.punkte[1 % o.punkte.len()]);
        let d = (b - a).normalized();
        let q = (a + b) * 0.5 + vec3(d.y, -d.x, 0.0) * probe;
        let ziel = bereiche
            .iter()
            .enumerate()
            .filter(|(_, b)| b.rand.enthaelt(q))
            .min_by(|x, y| x.1.rand.flaeche().total_cmp(&y.1.rand.flaeche()))
            .map(|(i, _)| i);
        if let Some(i) = ziel {
            bereiche[i].inseln.push(o.umgekehrt());
        }
    }
    Anordnung { aussen, bereiche }
}

/// Umrisse flach und gegen den Uhrzeigersinn, mit der ursprünglichen
/// Kantennummer je Kante; zu kleine fallen weg.
fn gegen_uhrzeiger(umrisse: &[Vec<Vec3>], tol: f64) -> Vec<(Vec<Vec3>, Vec<Herkunft>)> {
    let mut out = Vec::new();
    for (poly, u) in umrisse.iter().enumerate() {
        let n = u.len();
        let f = signed_area(u);
        if n < 3 || f.abs() < tol * tol {
            continue;
        }
        let pts: Vec<Vec3> = u.iter().map(|p| vec3(p.x, p.y, 0.0)).collect();
        let h = |kante| Herkunft { poly, kante };
        if f > 0.0 {
            out.push((pts, (0..n).map(h).collect()));
        } else {
            let r = Ring {
                punkte: pts,
                herkunft: (0..n).map(h).collect(),
            }
            .umgekehrt();
            out.push((r.punkte, r.herkunft));
        }
    }
    out
}

/// Alle Kanten, geteilt an Schnittpunkten und an Punkten anderer Kanten, die
/// auf ihnen liegen (T-Stoß, Überdeckung).
fn teilen(polys: &[(Vec<Vec3>, Vec<Herkunft>)], tol: f64) -> Vec<(Vec3, Vec3, Herkunft)> {
    let mut alle = Vec::new();
    for (p, h) in polys {
        let n = p.len();
        for k in 0..n {
            let (a, b) = (p[k], p[(k + 1) % n]);
            if (b - a).length() >= tol {
                alle.push((a, b, h[k]));
            }
        }
    }
    let mut out = Vec::new();
    for (i, &(a, b, h)) in alle.iter().enumerate() {
        let e = b - a;
        let len = e.length();
        let mut ts = vec![0.0, 1.0];
        for (j, &(c, d, _)) in alle.iter().enumerate() {
            if i == j || !kasten_nah(a, b, c, d, tol) {
                continue;
            }
            for q in [c, d] {
                let t = ((q - a).dot(e) / (len * len)).clamp(0.0, 1.0);
                if (a + e * t - q).length() < tol {
                    ts.push(t);
                }
            }
            let f = d - c;
            let nenner = e.x * f.y - e.y * f.x;
            if nenner.abs() > 1e-9 * len * f.length() {
                let w = c - a;
                let t = (w.x * f.y - w.y * f.x) / nenner;
                let s = (w.x * e.y - w.y * e.x) / nenner;
                if (0.0..=1.0).contains(&t) && (0.0..=1.0).contains(&s) {
                    ts.push(t);
                }
            }
        }
        ts.sort_by(f64::total_cmp);
        let mut von = 0.0;
        for &t in &ts[1..] {
            if (t - von) * len >= tol || t == 1.0 {
                out.push((a + e * von, a + e * t, h));
                von = t;
            }
        }
    }
    out
}

fn kasten_nah(a: Vec3, b: Vec3, c: Vec3, d: Vec3, tol: f64) -> bool {
    a.x.min(b.x) - tol <= c.x.max(d.x)
        && c.x.min(d.x) - tol <= a.x.max(b.x)
        && a.y.min(b.y) - tol <= c.y.max(d.y)
        && c.y.min(d.y) - tol <= a.y.max(b.y)
}

/// Punkte, zusammengelegt innerhalb der Toleranz (Raster mit Nachbarzellen).
struct Ecken {
    p: Vec<Vec3>,
    raster: HashMap<(i64, i64), Vec<usize>>,
    tol: f64,
}

impl Ecken {
    fn new(tol: f64) -> Ecken {
        Ecken {
            p: Vec::new(),
            raster: HashMap::new(),
            tol,
        }
    }

    fn zelle(&self, p: Vec3) -> (i64, i64) {
        (
            (p.x / self.tol).floor() as i64,
            (p.y / self.tol).floor() as i64,
        )
    }

    fn id(&mut self, p: Vec3) -> usize {
        let (cx, cy) = self.zelle(p);
        for x in cx - 1..=cx + 1 {
            for y in cy - 1..=cy + 1 {
                let Some(v) = self.raster.get(&(x, y)) else {
                    continue;
                };
                if let Some(&i) = v.iter().find(|&&i| (self.p[i] - p).length() < self.tol) {
                    return i;
                }
            }
        }
        self.p.push(p);
        let i = self.p.len() - 1;
        self.raster.entry((cx, cy)).or_default().push(i);
        i
    }
}

/// Randkanten zu Ringen: an jeder Ecke weiter mit der Kante, die am
/// weitesten links abbiegt (so trennen sich Umrisse, die sich nur in einem
/// Punkt berühren). Folgekanten derselben Herkunft werden eine Kante.
fn verbinden(p: &[Vec3], rand: &[(usize, usize, Herkunft)]) -> Vec<Ring> {
    let mut ab: HashMap<usize, Vec<usize>> = HashMap::new();
    for (k, &(a, _, _)) in rand.iter().enumerate() {
        ab.entry(a).or_default().push(k);
    }
    let richtung = |k: usize| (p[rand[k].1] - p[rand[k].0]).normalized();
    let mut benutzt = vec![false; rand.len()];
    let mut ringe = Vec::new();
    for k0 in 0..rand.len() {
        if benutzt[k0] {
            continue;
        }
        let mut folge = vec![k0];
        benutzt[k0] = true;
        let mut k = k0;
        let geschlossen = loop {
            let din = richtung(k);
            let weiter = ab.get(&rand[k].1).and_then(|v| {
                v.iter().copied().max_by(|&x, &y| {
                    let w = |e: usize| {
                        let d = richtung(e);
                        let t = (din.x * d.y - din.y * d.x).atan2(din.dot(d));
                        // Umkehr zuletzt
                        if t > std::f64::consts::PI - 1e-9 {
                            -t
                        } else {
                            t
                        }
                    };
                    w(x).total_cmp(&w(y))
                })
            });
            match weiter {
                Some(n) if n == k0 => break true,
                Some(n) if !benutzt[n] => {
                    benutzt[n] = true;
                    folge.push(n);
                    k = n;
                }
                _ => break false,
            }
        };
        if !geschlossen {
            continue;
        }
        // Folgekanten gleicher Herkunft zusammenfassen
        let m = folge.len();
        let start = (0..m)
            .find(|&i| rand[folge[i]].2 != rand[folge[(i + m - 1) % m]].2)
            .unwrap_or(0);
        let mut ring = Ring::default();
        for s in 0..m {
            let e = folge[(start + s) % m];
            if s > 0 && ring.herkunft.last() == Some(&rand[e].2) {
                continue;
            }
            ring.punkte.push(p[rand[e].0]);
            ring.herkunft.push(rand[e].2);
        }
        if ring.punkte.len() >= 3 {
            ringe.push(ring);
        }
    }
    ringe
}

/// Ring mit jeder Kante um ihren eigenen Abstand nach links versetzt: beim
/// Rand eines Bereichs nach innen, bei einer Insel aus ihr heraus (Putz je
/// Wandseite). Ecken liegen im Schnitt der Nachbarlinien; fluchtende Nachbarn
/// mit verschiedenem Abstand bekommen eine Stufe. `None`, wenn eine Kante
/// sich dabei umkehrt oder zwei Nachbarn gegeneinander laufen.
pub fn versatz(punkte: &[Vec3], abstand: &[f64]) -> Option<Vec<Vec3>> {
    let n = punkte.len();
    if n < 3 || abstand.len() != n {
        return None;
    }
    let d: Vec<Vec3> = (0..n)
        .map(|i| (punkte[(i + 1) % n] - punkte[i]).normalized())
        .collect();
    let links = |v: Vec3| vec3(-v.y, v.x, 0.0);
    // Je Ecke i (zwischen Kante i − 1 und i): erster und letzter neuer Punkt
    let mut ecken: Vec<(Vec3, Vec3)> = Vec::with_capacity(n);
    for i in 0..n {
        let h = (i + n - 1) % n;
        let p = punkte[i];
        let (a, b) = (p + links(d[h]) * abstand[h], p + links(d[i]) * abstand[i]);
        let x = d[h].x * d[i].y - d[h].y * d[i].x;
        if x.abs() < 1e-9 {
            if d[h].dot(d[i]) < 0.0 {
                return None;
            }
            ecken.push((a, b));
            continue;
        }
        // a + d[h]·s = b + d[i]·t
        let w = b - a;
        let s = (w.x * d[i].y - w.y * d[i].x) / x;
        let q = a + d[h] * s;
        ecken.push((q, q));
    }
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let (von, bis) = (ecken[i].1, ecken[(i + 1) % n].0);
        if (bis - von).dot(d[i]) <= 0.0 {
            return None;
        }
        if out.last() != Some(&ecken[i].0) {
            out.push(ecken[i].0);
        }
        if ecken[i].1 != ecken[i].0 {
            out.push(ecken[i].1);
        }
    }
    Some(out)
}

/// Vorzeichenbehafteter Abstand von `p` zum Bereich: positiv innen (bis zum
/// nächsten Rand oder zur nächsten Insel), negativ außen.
pub fn abstand(b: &Bereich, p: Vec3) -> f64 {
    let mut m = f64::INFINITY;
    for r in std::iter::once(&b.rand).chain(&b.inseln) {
        let n = r.punkte.len();
        for i in 0..n {
            let (a, c) = (r.punkte[i], r.punkte[(i + 1) % n]);
            let e = c - a;
            let l2 = e.x * e.x + e.y * e.y;
            let t = if l2 > 0.0 {
                (((p.x - a.x) * e.x + (p.y - a.y) * e.y) / l2).clamp(0.0, 1.0)
            } else {
                0.0
            };
            let q = a + e * t;
            m = m.min(((p.x - q.x).powi(2) + (p.y - q.y).powi(2)).sqrt());
        }
    }
    if b.enthaelt(p) {
        m
    } else {
        -m
    }
}

/// Stelle im Bereich mit dem größten Abstand zu Rand und Inseln (Mitte des
/// größten Kreises darin), auf `genau` mm genau, und dieser Abstand. Für die
/// Lage des Raumstempels. Zellen absteigend nach ihrem bestmöglichen Abstand
/// teilen, bis keine mehr besser sein kann.
pub fn innenpunkt(b: &Bereich, genau: f64) -> (Vec3, f64) {
    use std::cmp::Ordering;
    use std::collections::BinaryHeap;

    struct Zelle {
        c: Vec3,
        h: f64,
        d: f64,
    }
    impl Zelle {
        fn neu(b: &Bereich, c: Vec3, h: f64) -> Zelle {
            Zelle {
                c,
                h,
                d: abstand(b, c),
            }
        }
        fn hoechstens(&self) -> f64 {
            self.d + self.h * std::f64::consts::SQRT_2
        }
    }
    impl PartialEq for Zelle {
        fn eq(&self, o: &Zelle) -> bool {
            self.cmp(o) == Ordering::Equal
        }
    }
    impl Eq for Zelle {}
    impl PartialOrd for Zelle {
        fn partial_cmp(&self, o: &Zelle) -> Option<Ordering> {
            Some(self.cmp(o))
        }
    }
    impl Ord for Zelle {
        fn cmp(&self, o: &Zelle) -> Ordering {
            self.hoechstens().total_cmp(&o.hoechstens())
        }
    }

    let pts = &b.rand.punkte;
    if pts.is_empty() {
        return (Vec3::default(), 0.0);
    }
    let (mut x0, mut y0, mut x1, mut y1) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
    for p in pts {
        (x0, y0, x1, y1) = (x0.min(p.x), y0.min(p.y), x1.max(p.x), y1.max(p.y));
    }
    let seite = (x1 - x0).min(y1 - y0);
    if seite <= 0.0 {
        return (pts[0], 0.0);
    }
    let genau = genau.max(seite * 1e-6);
    let h = seite / 2.0;
    let mut schlange = BinaryHeap::new();
    let mut x = x0;
    while x < x1 {
        let mut y = y0;
        while y < y1 {
            schlange.push(Zelle::neu(b, vec3(x + h, y + h, 0.0), h));
            y += seite;
        }
        x += seite;
    }
    let mitte =
        crate::polygon::centroid(pts).unwrap_or(vec3((x0 + x1) / 2.0, (y0 + y1) / 2.0, 0.0));
    let mut best = Zelle::neu(b, vec3(mitte.x, mitte.y, 0.0), 0.0);
    while let Some(z) = schlange.pop() {
        if z.d > best.d {
            best = Zelle { h: 0.0, ..z };
        }
        if z.hoechstens() - best.d <= genau {
            continue;
        }
        let h = z.h / 2.0;
        for (sx, sy) in [(-1.0, -1.0), (1.0, -1.0), (-1.0, 1.0), (1.0, 1.0)] {
            schlange.push(Zelle::neu(b, z.c + vec3(sx * h, sy * h, 0.0), h));
        }
    }
    (best.c, best.d)
}

#[cfg(test)]
mod tests {
    use super::*;

    const TOL: f64 = 0.5;

    fn rechteck(x0: f64, y0: f64, x1: f64, y1: f64) -> Vec<Vec3> {
        vec![
            vec3(x0, y0, 0.0),
            vec3(x1, y0, 0.0),
            vec3(x1, y1, 0.0),
            vec3(x0, y1, 0.0),
        ]
    }

    /// Außenwände 10 × 8 m, 30 cm dick, als vier Rechtecke, die sich an den
    /// Ecken überdecken. Innen 9,40 × 7,40 m.
    fn aussen() -> Vec<Vec<Vec3>> {
        vec![
            rechteck(0.0, 0.0, 10000.0, 300.0),
            rechteck(9700.0, 0.0, 10000.0, 8000.0),
            rechteck(0.0, 7700.0, 10000.0, 8000.0),
            rechteck(0.0, 0.0, 300.0, 8000.0),
        ]
    }

    fn flaechen(a: &Anordnung) -> Vec<f64> {
        let mut v: Vec<f64> = a.bereiche.iter().map(Bereich::flaeche).collect();
        v.sort_by(f64::total_cmp);
        v
    }

    fn nah(a: f64, b: f64) -> bool {
        (a - b).abs() < 1.0
    }

    #[test]
    fn ein_raum_im_ring() {
        let a = anordnung(&aussen(), TOL);
        assert_eq!(a.aussen.len(), 1);
        assert!(nah(a.aussen[0].flaeche(), 80e6));
        assert_eq!(a.bereiche.len(), 1);
        let r = &a.bereiche[0];
        assert_eq!(r.rand.punkte.len(), 4, "vier Ecken: {:?}", r.rand.punkte);
        assert!(nah(r.flaeche(), 9400.0 * 7400.0));
        assert!(
            signed_area(&r.rand.punkte) > 0.0,
            "Rand gegen den Uhrzeigersinn"
        );
        assert!(r.enthaelt(vec3(5000.0, 4000.0, 0.0)));
        assert!(!r.enthaelt(vec3(100.0, 4000.0, 0.0)));
    }

    #[test]
    fn trennwand_teilt_in_zwei() {
        let mut u = aussen();
        u.push(rechteck(4942.5, 300.0, 5057.5, 7700.0));
        let a = anordnung(&u, TOL);
        assert_eq!(a.bereiche.len(), 2);
        let f = flaechen(&a);
        assert!(nah(f[0], (9700.0 - 5057.5) * 7400.0), "{f:?}");
        assert!(nah(f[1], (4942.5 - 300.0) * 7400.0), "{f:?}");
        for b in &a.bereiche {
            assert_eq!(b.rand.punkte.len(), 4);
        }
    }

    #[test]
    fn herkunft_zeigt_die_wandflaeche() {
        let mut u = aussen();
        u.push(rechteck(4942.5, 300.0, 5057.5, 7700.0));
        let a = anordnung(&u, TOL);
        let links = a
            .bereiche
            .iter()
            .find(|b| b.enthaelt(vec3(1000.0, 1000.0, 0.0)))
            .unwrap();
        // Kanten aus: unterer Wand (0), Trennwand (4), oberer Wand (2), linker Wand (3)
        let mut polys: Vec<usize> = links.rand.herkunft.iter().map(|h| h.poly).collect();
        polys.sort();
        assert_eq!(polys, vec![0, 2, 3, 4]);
        // Die Kante an der Trennwand ist deren linke Fläche (Kante 3: x0, oben nach unten)
        let i = links
            .rand
            .herkunft
            .iter()
            .position(|h| h.poly == 4)
            .unwrap();
        assert_eq!(links.rand.herkunft[i].kante, 3);
        let (p, q) = (links.rand.punkte[i], links.rand.punkte[(i + 1) % 4]);
        assert!(nah(p.x, 4942.5) && nah(q.x, 4942.5));
    }

    #[test]
    fn luecke_ergibt_keinen_raum() {
        let mut u = aussen();
        u.remove(1); // rechte Wand fehlt
        let a = anordnung(&u, TOL);
        assert!(a.bereiche.is_empty());
        // Wand, die 5 mm vor der Gegenwand endet: ein Raum, nicht zwei
        let mut u = aussen();
        u.push(rechteck(4942.5, 300.0, 5057.5, 7695.0));
        let a = anordnung(&u, TOL);
        assert_eq!(a.bereiche.len(), 1);
        assert!(nah(
            a.bereiche[0].flaeche(),
            9400.0 * 7400.0 - 115.0 * 7395.0
        ));
    }

    #[test]
    fn knapp_vor_der_wand_schliesst() {
        let mut u = aussen();
        u.push(rechteck(4942.5, 300.0, 5057.5, 7699.7));
        let a = anordnung(&u, TOL);
        assert_eq!(a.bereiche.len(), 2, "0,3 mm Spalt liegt in der Toleranz");
    }

    #[test]
    fn freie_wand_ist_insel() {
        let mut u = aussen();
        u.push(rechteck(3000.0, 3000.0, 6000.0, 3115.0));
        let a = anordnung(&u, TOL);
        assert_eq!(a.aussen.len(), 2);
        assert_eq!(a.bereiche.len(), 1);
        let r = &a.bereiche[0];
        assert_eq!(r.inseln.len(), 1);
        assert!(
            signed_area(&r.inseln[0].punkte) < 0.0,
            "Insel im Uhrzeigersinn"
        );
        assert!(nah(r.flaeche(), 9400.0 * 7400.0 - 3000.0 * 115.0));
        assert!(!r.enthaelt(vec3(4000.0, 3050.0, 0.0)));
    }

    #[test]
    fn schacht_im_raum() {
        let mut u = aussen();
        // Schacht 1 × 1 m außen, Wände 11,5 cm, frei im Raum
        u.push(rechteck(2000.0, 2000.0, 3000.0, 2115.0));
        u.push(rechteck(2885.0, 2000.0, 3000.0, 3000.0));
        u.push(rechteck(2000.0, 2885.0, 3000.0, 3000.0));
        u.push(rechteck(2000.0, 2000.0, 2115.0, 3000.0));
        let a = anordnung(&u, TOL);
        assert_eq!(a.bereiche.len(), 2);
        let f = flaechen(&a);
        assert!(nah(f[0], 770.0 * 770.0), "{f:?}");
        assert!(nah(f[1], 9400.0 * 7400.0 - 1000.0 * 1000.0), "{f:?}");
    }

    #[test]
    fn schraege_wand_schneidet_ecke_ab() {
        let mut u = aussen();
        // Wand zwischen x + y = 2000 und x + y = 2200, reicht in die Außenwände
        u.push(vec![
            vec3(-100.0, 2100.0, 0.0),
            vec3(2100.0, -100.0, 0.0),
            vec3(2300.0, -100.0, 0.0),
            vec3(-100.0, 2300.0, 0.0),
        ]);
        let a = anordnung(&u, TOL);
        assert_eq!(a.bereiche.len(), 2);
        let f = flaechen(&a);
        assert!(nah(f[0], 1400.0 * 1400.0 / 2.0), "{f:?}");
        assert!(nah(f[1], 9400.0 * 7400.0 - 1600.0 * 1600.0 / 2.0), "{f:?}");
        let dreieck = a
            .bereiche
            .iter()
            .find(|b| b.enthaelt(vec3(500.0, 500.0, 0.0)))
            .unwrap();
        assert_eq!(dreieck.rand.punkte.len(), 3);
    }

    #[test]
    fn beruehrung_im_punkt_trennt() {
        // Zwei Ringe, die sich in einer Ecke berühren: zwei Außenränder, zwei Bereiche
        let mut u = aussen();
        for r in aussen() {
            u.push(r.iter().map(|p| *p + vec3(10000.0, 8000.0, 0.0)).collect());
        }
        let a = anordnung(&u, TOL);
        assert_eq!(a.aussen.len(), 2);
        assert_eq!(a.bereiche.len(), 2);
    }

    #[test]
    fn richtung_der_umrisse_egal() {
        let mut u = aussen();
        u.push(rechteck(4942.5, 300.0, 5057.5, 7700.0));
        let rueck: Vec<Vec<Vec3>> = u
            .iter()
            .map(|p| p.iter().rev().copied().collect())
            .collect();
        let a = anordnung(&rueck, TOL);
        assert_eq!(a.bereiche.len(), 2);
        // Herkunft zählt in der übergebenen Reihenfolge: die linke Fläche der
        // umgedrehten Trennwand ist dort Kante 3 (x0, unten nach oben)
        let links = a
            .bereiche
            .iter()
            .find(|b| b.enthaelt(vec3(1000.0, 1000.0, 0.0)))
            .unwrap();
        let h = links.rand.herkunft.iter().find(|h| h.poly == 4).unwrap();
        assert_eq!(h.kante, 3);
    }

    #[test]
    fn versatz_je_kante() {
        // Raum 4 × 3 m, Putz 15 mm, an der rechten Wand 30 mm
        let r = rechteck(0.0, 0.0, 4000.0, 3000.0);
        let v = versatz(&r, &[15.0, 30.0, 15.0, 15.0]).unwrap();
        assert_eq!(v.len(), 4);
        assert!(nah(signed_area(&v), (4000.0 - 45.0) * (3000.0 - 30.0)));
        assert!((v[0] - vec3(15.0, 15.0, 0.0)).length() < 1e-9);
        assert!((v[1] - vec3(3970.0, 15.0, 0.0)).length() < 1e-9);
    }

    #[test]
    fn versatz_mit_stufe_und_schraege() {
        // Fluchtende Kanten mit verschiedenem Abstand: eine Stufe
        let r = vec![
            vec3(0.0, 0.0, 0.0),
            vec3(2000.0, 0.0, 0.0),
            vec3(4000.0, 0.0, 0.0),
            vec3(4000.0, 3000.0, 0.0),
            vec3(0.0, 3000.0, 0.0),
        ];
        let v = versatz(&r, &[10.0, 20.0, 10.0, 10.0, 10.0]).unwrap();
        assert_eq!(v.len(), 6);
        let soll = 3980.0 * 2980.0 - 2000.0 * 10.0 + 10.0 * 10.0;
        assert!(
            nah(signed_area(&v), soll),
            "{} statt {soll}",
            signed_area(&v)
        );
        // Schräge Ecke: Dreieck mit Katheten 3 m, Versatz 10 mm
        let d = vec![
            vec3(0.0, 0.0, 0.0),
            vec3(3000.0, 0.0, 0.0),
            vec3(0.0, 3000.0, 0.0),
        ];
        let v = versatz(&d, &[10.0; 3]).unwrap();
        // Inkreisradius r = 3000 (2 − √2) / 2, Dreieck schrumpft um (r − 10) / r
        let ri = 3000.0 * (2.0 - 2f64.sqrt()) / 2.0;
        let k = (ri - 10.0) / ri;
        assert!(nah(signed_area(&v), 4.5e6 * k * k));
        // Zu großer Versatz kehrt Kanten um
        assert!(versatz(&d, &[2000.0; 3]).is_none());
    }

    fn bereich(r: Vec<Vec3>, inseln: Vec<Vec<Vec3>>) -> Bereich {
        let ring = |p: Vec<Vec3>| Ring {
            herkunft: vec![Herkunft { poly: 0, kante: 0 }; p.len()],
            punkte: p,
        };
        Bereich {
            rand: ring(r),
            inseln: inseln
                .into_iter()
                .map(|i| ring(i.into_iter().rev().collect()))
                .collect(),
        }
    }

    #[test]
    fn innenpunkt_rechteck_und_l() {
        let b = bereich(rechteck(0.0, 0.0, 4000.0, 3000.0), vec![]);
        let (p, d) = innenpunkt(&b, 1.0);
        assert!(nah(d, 1500.0), "{d}");
        assert!(
            (p.y - 1500.0).abs() < 2.0 && p.x > 1499.0 && p.x < 2501.0,
            "{p:?}"
        );
        // L-Form: der breitere Schenkel gewinnt
        let l = vec![
            vec3(0.0, 0.0, 0.0),
            vec3(6000.0, 0.0, 0.0),
            vec3(6000.0, 4000.0, 0.0),
            vec3(2000.0, 4000.0, 0.0),
            vec3(2000.0, 8000.0, 0.0),
            vec3(0.0, 8000.0, 0.0),
        ];
        let (p, d) = innenpunkt(&bereich(l, vec![]), 1.0);
        assert!(nah(d, 2000.0), "{d}");
        assert!(p.y < 4000.0 && p.x > 1999.0, "{p:?}");
    }

    #[test]
    fn innenpunkt_weicht_der_insel_aus() {
        let insel = rechteck(1800.0, 1300.0, 2200.0, 1700.0);
        let b = bereich(rechteck(0.0, 0.0, 4000.0, 3000.0), vec![insel]);
        assert!(abstand(&b, vec3(2000.0, 1500.0, 0.0)) < 0.0, "in der Insel");
        let (p, d) = innenpunkt(&b, 1.0);
        assert!(b.enthaelt(p));
        assert!(d > 600.0 && d < 1500.0, "{d}");
        assert!((p - vec3(2000.0, 1500.0, 0.0)).length() > 400.0);
    }
}
