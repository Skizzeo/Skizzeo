//! Ebene Flächen aus Umrissen (Grundriss G3, Jörn 11.10. 16:53): Die
//! Umrisse der Wände eines Geschosses werden vereinigt; jede Fläche, die sie
//! ganz umschließen, ist ein Bereich (später ein Raum). Alles auf z = 0, in mm.
//!
//! Vorgehen: Kanten aller Umrisse an Schnitt- und Berührpunkten teilen (ein
//! Punkt höchstens `tol` vor einer Kante teilt sie), Punkte höchstens `tol`
//! voneinander zusammenlegen und aus den Teilstücken ein ebenes Netz bilden.
//! Seine Flächen ergeben sich aus der Reihenfolge der Kanten um jede Ecke; je
//! Fläche zählt, welche Umrisse sie umlaufen (Umlaufzahl, von außen über die
//! Kanten hinweg). Ränder sind die Kanten zwischen bedeckter und freier
//! Fläche, innen links; sie verbinden sich an jeder Ecke mit der am weitesten
//! links abbiegenden Fortsetzung. Ringe gegen den Uhrzeigersinn sind
//! Außenränder der Vereinigung, Ringe im Uhrzeigersinn umschließen einen
//! Bereich. Innen und außen entscheiden so dieselben Ecken und Kanten, aus
//! denen die Ringe entstehen; auch verdrehte Umrisse kurzer Wände mit Gehrung
//! tragen.

use std::collections::HashMap;
use std::hash::{BuildHasherDefault, Hasher};

use crate::polygon::signed_area;
use crate::{vec3, Vec3};

/// Tabelle über Zahlen aus der eigenen Rechnung (Rasterzellen, Punkte,
/// Kanten), mit schnellem Streuwert statt des gegen Angriffe gehärteten.
type Tabelle<K, V> = HashMap<K, V, BuildHasherDefault<Streu>>;

/// Streuwert für [`Tabelle`]: je Zahl drehen, mischen, mit einer ungeraden
/// Konstanten malnehmen; am Ende gedreht, damit die unteren Bits (der Platz
/// in der Tabelle) von allen Bits abhängen.
#[derive(Default)]
struct Streu(u64);

impl Streu {
    fn dazu(&mut self, n: u64) {
        self.0 = (self.0.rotate_left(5) ^ n).wrapping_mul(0x517c_c1b7_2722_0a95);
    }
}

impl Hasher for Streu {
    fn finish(&self) -> u64 {
        self.0.rotate_left(26)
    }

    fn write(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.dazu(u64::from(b));
        }
    }

    fn write_u64(&mut self, n: u64) {
        self.dazu(n);
    }

    fn write_i64(&mut self, n: i64) {
        self.dazu(n as u64);
    }

    fn write_usize(&mut self, n: usize) {
        self.dazu(n as u64);
    }
}

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
/// Bereiche. Punkte höchstens `tol` voneinander gelten als gleich, ein Punkt
/// höchstens `tol` vor einer Kante als auf ihr; so schließt auch eine Wand,
/// die bis zu `tol` vor der nächsten endet. Ein weiterer Spalt bleibt offen.
pub fn anordnung(umrisse: &[Vec<Vec3>], tol: f64) -> Anordnung {
    let tol = tol.max(1e-6);
    let polys = gegen_uhrzeiger(umrisse, tol);
    let netz = Netz::neu(&polys, tol);
    let (mut aussen, mut loecher) = (Vec::new(), Vec::new());
    for r in netz.raender() {
        let f = signed_area(&r.ring.punkte);
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
            rand: l.ring.umgekehrt(),
            inseln: Vec::new(),
        })
        .collect();
    // Jeder Außenrand, der in einem Bereich liegt, ist dort eine Insel (im
    // kleinsten, der ihn enthält; geprüft an einer seiner Ecken, die nicht
    // auf dem Rand des Bereichs liegt). Vortest über die Kästen (Review #87
    // B3: sonst Außenränder mal Bereiche mal Ecken)
    let kasten = |pts: &[Vec3]| {
        pts.iter().fold(
            [f64::MAX, f64::MAX, f64::MIN, f64::MIN],
            |[x0, y0, x1, y1], p| [x0.min(p.x), y0.min(p.y), x1.max(p.x), y1.max(p.y)],
        )
    };
    let kaesten: Vec<[f64; 4]> = loecher.iter().map(|l| kasten(&l.ring.punkte)).collect();
    for o in &aussen {
        let ko = kasten(&o.ring.punkte);
        let ziel = loecher
            .iter()
            .enumerate()
            .filter(|(i, _)| {
                let k = &kaesten[*i];
                k[0] - tol <= ko[0]
                    && k[1] - tol <= ko[1]
                    && ko[2] <= k[2] + tol
                    && ko[3] <= k[3] + tol
            })
            .filter_map(|(i, l)| {
                let j = o.ecken.iter().position(|e| !l.alle.contains(e))?;
                bereiche[i].rand.enthaelt(o.ring.punkte[j]).then_some(i)
            })
            .min_by(|&x, &y| {
                bereiche[x]
                    .rand
                    .flaeche()
                    .total_cmp(&bereiche[y].rand.flaeche())
            });
        if let Some(i) = ziel {
            bereiche[i].inseln.push(o.ring.umgekehrt());
        }
    }
    Anordnung {
        aussen: aussen.into_iter().map(|r| r.ring).collect(),
        bereiche,
    }
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
    let kaesten: Vec<[f64; 4]> = alle
        .iter()
        .map(|&(a, b, _)| {
            [
                a.x.min(b.x) - tol,
                a.y.min(b.y) - tol,
                a.x.max(b.x) + tol,
                a.y.max(b.y) + tol,
            ]
        })
        .collect();
    let raster = Raster::neu(&kaesten);
    let mut marke = vec![false; alle.len()];
    let mut nahe = Vec::new();
    let mut out = Vec::new();
    for (i, &(a, b, h)) in alle.iter().enumerate() {
        let e = b - a;
        let len = e.length();
        let mut ts = vec![0.0, 1.0];
        raster.um(&kaesten[i], &mut marke, &mut nahe);
        for &j in &nahe {
            let (c, d, _) = alle[j];
            if i == j || !kasten_nah(a, b, c, d, tol) {
                continue;
            }
            for q in [c, d] {
                let t = ((q - a).dot(e) / (len * len)).clamp(0.0, 1.0);
                if (a + e * t - q).length() <= tol {
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
        // Jeder Teilpunkt bleibt, auch dicht am nächsten: Zusammengelegt wird
        // nur in [`Ecken`], an denselben Punkten, an denen das Fangen
        // entschieden hat. Ein übersprungener Teilpunkt ließ eine Kante über
        // eine andere laufen, ohne dass sich beide trafen.
        let mut von = 0.0;
        for &t in &ts[1..] {
            if t > von {
                out.push((a + e * von, a + e * t, h));
                von = t;
            }
        }
    }
    out
}

/// Raster über achsenparallele Kästen `[x0, y0, x1, y1]`: findet die Kästen,
/// die einen gegebenen berühren können, ohne alle zu prüfen. So wächst die
/// Arbeit mit der Zahl der Wände, nicht mit ihrem Quadrat.
struct Raster {
    zelle: f64,
    karte: Tabelle<(i64, i64), Vec<usize>>,
}

impl Raster {
    fn neu(kaesten: &[[f64; 4]]) -> Raster {
        // Zellgröße: mittlere Seitenlänge der Kästen, höchstens 4096 Zellen
        // je Richtung über alles
        let n = kaesten.len().max(1) as f64;
        let mittel = kaesten
            .iter()
            .map(|k| (k[2] - k[0]).max(k[3] - k[1]))
            .sum::<f64>()
            / n;
        let (mut lo, mut hi) = ([f64::MAX; 2], [f64::MIN; 2]);
        for k in kaesten {
            lo = [lo[0].min(k[0]), lo[1].min(k[1])];
            hi = [hi[0].max(k[2]), hi[1].max(k[3])];
        }
        let weite = (hi[0] - lo[0]).max(hi[1] - lo[1]).max(0.0);
        let zelle = mittel.max(weite / 4096.0).max(1e-3);
        let mut karte: Tabelle<(i64, i64), Vec<usize>> = Tabelle::default();
        for (i, k) in kaesten.iter().enumerate() {
            for z in Raster::zellen(zelle, k) {
                karte.entry(z).or_default().push(i);
            }
        }
        Raster { zelle, karte }
    }

    fn zellen(zelle: f64, k: &[f64; 4]) -> impl Iterator<Item = (i64, i64)> {
        let z = |v: f64| (v / zelle).floor() as i64;
        let (x0, y0, x1, y1) = (z(k[0]), z(k[1]), z(k[2]), z(k[3]));
        (x0..=x1).flat_map(move |x| (y0..=y1).map(move |y| (x, y)))
    }

    /// Die Kästen in den Zellen von `k`, jeder einmal; `marke` hat je Kasten
    /// einen Platz und ist danach wieder leer.
    fn um(&self, k: &[f64; 4], marke: &mut [bool], out: &mut Vec<usize>) {
        out.clear();
        for z in Raster::zellen(self.zelle, k) {
            for &i in self.karte.get(&z).into_iter().flatten() {
                if !marke[i] {
                    marke[i] = true;
                    out.push(i);
                }
            }
        }
        for &i in out.iter() {
            marke[i] = false;
        }
    }
}

fn kasten_nah(a: Vec3, b: Vec3, c: Vec3, d: Vec3, tol: f64) -> bool {
    a.x.min(b.x) - tol <= c.x.max(d.x)
        && c.x.min(d.x) - tol <= a.x.max(b.x)
        && a.y.min(b.y) - tol <= c.y.max(d.y)
        && c.y.min(d.y) - tol <= a.y.max(b.y)
}

/// Punkte, zusammengelegt innerhalb der Toleranz: Je zwei Punkte höchstens
/// `tol` voneinander kommen in dieselbe Gruppe, auch über einen dritten
/// hinweg. Das hängt nicht von der Reihenfolge ab, und derselbe Punkt landet
/// immer in derselben Gruppe; so trifft jede Entscheidung des Teilens (ein
/// Punkt höchstens `tol` vor einer Kante) auch hier zu. Die Gruppe liegt am
/// zuerst übergebenen ihrer Punkte.
struct Ecken {
    p: Vec<Vec3>,
    nummer: Tabelle<(u64, u64), usize>,
}

impl Ecken {
    fn neu(tol: f64, punkte: impl IntoIterator<Item = Vec3>) -> Ecken {
        let schluessel = |p: Vec3| (p.x.to_bits(), p.y.to_bits());
        let mut roh: Vec<Vec3> = Vec::new();
        let mut index: Tabelle<(u64, u64), usize> = Tabelle::default();
        for p in punkte {
            index.entry(schluessel(p)).or_insert_with(|| {
                roh.push(p);
                roh.len() - 1
            });
        }
        let zelle = |p: Vec3| ((p.x / tol).floor() as i64, (p.y / tol).floor() as i64);
        let mut raster: Tabelle<(i64, i64), Vec<usize>> = Tabelle::default();
        for (i, &p) in roh.iter().enumerate() {
            raster.entry(zelle(p)).or_default().push(i);
        }
        fn wurzel(t: &mut [usize], mut i: usize) -> usize {
            while t[i] != i {
                t[i] = t[t[i]];
                i = t[i];
            }
            i
        }
        let mut teil: Vec<usize> = (0..roh.len()).collect();
        for (i, &p) in roh.iter().enumerate() {
            let (cx, cy) = zelle(p);
            for x in cx - 1..=cx + 1 {
                for y in cy - 1..=cy + 1 {
                    for &j in raster.get(&(x, y)).into_iter().flatten() {
                        if j > i && (roh[j] - p).length() <= tol {
                            let (a, b) = (wurzel(&mut teil, i), wurzel(&mut teil, j));
                            // Die kleinere Nummer bleibt Wurzel: die Gruppe
                            // liegt am ersten ihrer Punkte
                            teil[a.max(b)] = a.min(b);
                        }
                    }
                }
            }
        }
        let mut gruppe = vec![usize::MAX; roh.len()];
        let mut p = Vec::new();
        for i in 0..roh.len() {
            let w = wurzel(&mut teil, i);
            if gruppe[w] == usize::MAX {
                gruppe[w] = p.len();
                p.push(roh[w]);
            }
            gruppe[i] = gruppe[w];
        }
        let nummer = index.into_iter().map(|(k, i)| (k, gruppe[i])).collect();
        Ecken { p, nummer }
    }

    fn id(&self, p: Vec3) -> usize {
        self.nummer[&(p.x.to_bits(), p.y.to_bits())]
    }
}

/// Umlaufzahlen der Umrisse an einer Stelle: je Umriss (Nummer wie in
/// [`Herkunft::poly`]) die Zahl ungleich null, nach Umriss geordnet.
type Umlauf = Vec<(usize, i32)>;

fn addieren(w: &mut Umlauf, poly: usize, d: i32) {
    match w.binary_search_by_key(&poly, |&(q, _)| q) {
        Ok(i) => {
            w[i].1 += d;
            if w[i].1 == 0 {
                w.remove(i);
            }
        }
        Err(i) if d != 0 => w.insert(i, (poly, d)),
        Err(_) => {}
    }
}

fn hat(w: &[(usize, i32)], poly: usize) -> bool {
    w.binary_search_by_key(&poly, |&(q, _)| q).is_ok()
}

/// Beitrag der Kante `a` → `b` zur Umlaufzahl um `x` (Strahl nach +x).
fn kreuzung(a: Vec3, b: Vec3, x: Vec3) -> i32 {
    let links = (b.x - a.x) * (x.y - a.y) - (x.x - a.x) * (b.y - a.y);
    if a.y <= x.y {
        i32::from(b.y > x.y && links > 0.0)
    } else {
        -i32::from(b.y <= x.y && links < 0.0)
    }
}

/// Die gefangenen Teilstücke aller Umrisse als ebenes Netz. Innen und außen
/// werden hier entschieden, an denselben Ecken und Kanten, aus denen die
/// Ringe entstehen: Die Flächen des Netzes ergeben sich aus der Reihenfolge
/// der Kanten um jede Ecke, ihre Umlaufzahlen von außen nach innen über die
/// Kanten hinweg. So können Fangen und Prüfen an der Toleranzgrenze nicht
/// verschieden entscheiden.
///
/// Kante `k` verbindet die Ecken `u < v`; Halbkante `2k` läuft von `u` nach
/// `v`, `2k + 1` zurück.
struct Netz {
    p: Vec<Vec3>,
    kanten: Vec<NetzKante>,
    /// Abgehende Halbkanten je Ecke, gegen den Uhrzeigersinn nach Richtung.
    ab: Vec<Vec<usize>>,
    /// Stelle jeder Halbkante in `ab` ihrer Anfangsecke.
    stelle: Vec<usize>,
}

struct NetzKante {
    u: usize,
    v: usize,
    /// Herkunft der Teilstücke darauf, mit 1, wenn das Stück von `u` nach
    /// `v` lief, sonst -1.
    herkunft: Vec<(Herkunft, i32)>,
    /// Je Umriss: wie oft er die Kante von `u` nach `v` läuft, abzüglich
    /// zurück (nur ungleich null).
    lauf: Umlauf,
}

/// Rand der Vereinigung mit den Nummern seiner Ecken: `ecken` zu
/// `ring.punkte`, `alle` auch die zwischen Kanten gleicher Herkunft.
struct RandRing {
    ring: Ring,
    ecken: Vec<usize>,
    alle: Vec<usize>,
}

impl Netz {
    fn neu(polys: &[(Vec<Vec3>, Vec<Herkunft>)], tol: f64) -> Netz {
        let stuecke = teilen(polys, tol);
        let ecken = Ecken::neu(tol, stuecke.iter().flat_map(|&(a, b, _)| [a, b]));
        let mut index: Tabelle<(usize, usize), usize> = Tabelle::default();
        let mut kanten: Vec<NetzKante> = Vec::new();
        for (a, b, h) in stuecke {
            let (ia, ib) = (ecken.id(a), ecken.id(b));
            if ia == ib {
                continue;
            }
            let (u, v, s) = if ia < ib { (ia, ib, 1) } else { (ib, ia, -1) };
            let k = *index.entry((u, v)).or_insert_with(|| {
                kanten.push(NetzKante {
                    u,
                    v,
                    herkunft: Vec::new(),
                    lauf: Vec::new(),
                });
                kanten.len() - 1
            });
            kanten[k].herkunft.push((h, s));
            addieren(&mut kanten[k].lauf, h.poly, s);
        }
        let p = ecken.p;
        let winkel: Vec<f64> = (0..2 * kanten.len())
            .map(|h| {
                let e = &kanten[h / 2];
                let d = if h.is_multiple_of(2) {
                    p[e.v] - p[e.u]
                } else {
                    p[e.u] - p[e.v]
                };
                d.y.atan2(d.x)
            })
            .collect();
        let mut ab = vec![Vec::new(); p.len()];
        for (k, e) in kanten.iter().enumerate() {
            ab[e.u].push(2 * k);
            ab[e.v].push(2 * k + 1);
        }
        let mut stelle = vec![0; winkel.len()];
        for v in &mut ab {
            v.sort_by(|&x, &y| winkel[x].total_cmp(&winkel[y]).then(x.cmp(&y)));
            for (i, &h) in v.iter().enumerate() {
                stelle[h] = i;
            }
        }
        Netz {
            p,
            kanten,
            ab,
            stelle,
        }
    }

    fn von(&self, h: usize) -> usize {
        let e = &self.kanten[h / 2];
        if h.is_multiple_of(2) {
            e.u
        } else {
            e.v
        }
    }

    /// Nächste Halbkante derselben Fläche (die Fläche liegt links): an der
    /// Endecke die erste im Uhrzeigersinn nach der Gegenrichtung, also die am
    /// weitesten links abbiegende.
    fn weiter(&self, h: usize) -> usize {
        let t = h ^ 1;
        let liste = &self.ab[self.von(t)];
        liste[(self.stelle[t] + liste.len() - 1) % liste.len()]
    }

    /// Flächen des Netzes: je Halbkante die Fläche links von ihr, je Fläche
    /// ihre Halbkanten.
    fn flaechen(&self) -> (Vec<usize>, Vec<Vec<usize>>) {
        let n = 2 * self.kanten.len();
        let mut flaeche = vec![usize::MAX; n];
        let mut zyklen = Vec::new();
        for h0 in 0..n {
            if flaeche[h0] != usize::MAX {
                continue;
            }
            let mut z = Vec::new();
            let mut h = h0;
            while flaeche[h] == usize::MAX {
                flaeche[h] = zyklen.len();
                z.push(h);
                h = self.weiter(h);
            }
            zyklen.push(z);
        }
        (flaeche, zyklen)
    }

    /// Umlaufzahlen je Fläche. Je zusammenhängendem Teil des Netzes beginnt
    /// es an der Fläche um seine linkeste Ecke: Dort zählen nur die Umrisse
    /// der anderen Teile, die eigenen liegen alle innerhalb. Von dort über
    /// jede Kante: rechts ist, was links ist, minus ihr Lauf.
    fn umlaeufe(&self, flaeche: &[usize], zyklen: &[Vec<usize>]) -> Vec<Umlauf> {
        let mut teil: Vec<usize> = (0..self.p.len()).collect();
        fn wurzel(t: &mut [usize], mut i: usize) -> usize {
            while t[i] != i {
                t[i] = t[t[i]];
                i = t[i];
            }
            i
        }
        for e in &self.kanten {
            let (a, b) = (wurzel(&mut teil, e.u), wurzel(&mut teil, e.v));
            teil[a] = b;
        }
        let mut links_unten: Tabelle<usize, usize> = Tabelle::default();
        for v in 0..self.p.len() {
            if self.ab[v].is_empty() {
                continue;
            }
            let w = wurzel(&mut teil, v);
            let q = self.p[v];
            let besser = |o: usize| (q.x, q.y) < (self.p[o].x, self.p[o].y);
            match links_unten.get(&w) {
                Some(&o) if !besser(o) => {}
                _ => {
                    links_unten.insert(w, v);
                }
            }
        }
        // Je Umriss seine Kanten mit Lauf und sein Kasten: Nur ein Umriss,
        // dessen Kasten den Startpunkt enthält, kann ihn umlaufen
        let mut je_umriss: Tabelle<usize, Vec<(usize, i32)>> = Tabelle::default();
        for (k, e) in self.kanten.iter().enumerate() {
            for &(poly, n) in &e.lauf {
                je_umriss.entry(poly).or_default().push((k, n));
            }
        }
        let mut laeufe: Vec<(usize, Vec<(usize, i32)>)> = je_umriss.into_iter().collect();
        laeufe.sort_by_key(|(poly, _)| *poly);
        let kaesten: Vec<[f64; 4]> = laeufe
            .iter()
            .map(|(_, kk)| {
                let mut k = [f64::MAX, f64::MAX, f64::MIN, f64::MIN];
                for &(e, _) in kk {
                    for q in [self.p[self.kanten[e].u], self.p[self.kanten[e].v]] {
                        k = [k[0].min(q.x), k[1].min(q.y), k[2].max(q.x), k[3].max(q.y)];
                    }
                }
                k
            })
            .collect();
        let umrisse = Raster::neu(&kaesten);
        let mut marke = vec![false; laeufe.len()];
        let mut nahe = Vec::new();
        let mut umlauf: Vec<Option<Umlauf>> = vec![None; zyklen.len()];
        let mut start: Vec<(usize, usize)> = links_unten.into_iter().collect();
        start.sort();
        for (w, v) in start {
            // Alle abgehenden Kanten zeigen nach rechts (oder senkrecht nach
            // oben): Die Fläche um die Richtung nach links liegt links der
            // letzten.
            let f0 = flaeche[*self.ab[v].last().unwrap()];
            if umlauf[f0].is_some() {
                continue;
            }
            let x = self.p[v];
            let mut w0 = Umlauf::new();
            umrisse.um(&[x.x, x.y, x.x, x.y], &mut marke, &mut nahe);
            for &i in &nahe {
                let (poly, ref kk) = laeufe[i];
                let k = kaesten[i];
                if wurzel(&mut teil, self.kanten[kk[0].0].u) == w
                    || x.x < k[0]
                    || x.x > k[2]
                    || x.y < k[1]
                    || x.y > k[3]
                {
                    continue;
                }
                let mut z = 0;
                for &(e, n) in kk {
                    let e = &self.kanten[e];
                    z += n * kreuzung(self.p[e.u], self.p[e.v], x);
                }
                addieren(&mut w0, poly, z);
            }
            umlauf[f0] = Some(w0);
            let mut offen = vec![f0];
            while let Some(f) = offen.pop() {
                for &h in &zyklen[f] {
                    let g = flaeche[h ^ 1];
                    if umlauf[g].is_some() {
                        continue;
                    }
                    let mut w = umlauf[f].clone().unwrap_or_default();
                    let vz = if h.is_multiple_of(2) { -1 } else { 1 };
                    for &(poly, n) in &self.kanten[h / 2].lauf {
                        addieren(&mut w, poly, vz * n);
                    }
                    umlauf[g] = Some(w);
                    offen.push(g);
                }
            }
        }
        umlauf.into_iter().map(Option::unwrap_or_default).collect()
    }

    /// Ränder der Vereinigung: Halbkanten mit bedeckter Fläche links und
    /// freier rechts, an jeder Ecke weiter mit der nächsten solchen im
    /// Uhrzeigersinn. Folgekanten derselben Herkunft werden eine Kante.
    fn raender(&self) -> Vec<RandRing> {
        let (flaeche, zyklen) = self.flaechen();
        let umlauf = self.umlaeufe(&flaeche, &zyklen);
        let bedeckt = |h: usize| !umlauf[flaeche[h]].is_empty();
        let rand = |h: usize| bedeckt(h) && !bedeckt(h ^ 1);
        let folgende = |h: usize| {
            let mut g = self.weiter(h);
            for _ in 0..self.ab[self.von(g)].len() {
                if rand(g) {
                    return Some(g);
                }
                g = self.weiter(g ^ 1);
            }
            None
        };
        // Herkunft: ein Umriss, der links innen und rechts außen ist, in
        // Laufrichtung, wenn möglich
        let herkunft = |h: usize| {
            let e = &self.kanten[h / 2];
            let vz = if h.is_multiple_of(2) { 1 } else { -1 };
            let traegt =
                |poly| hat(&umlauf[flaeche[h]], poly) && !hat(&umlauf[flaeche[h ^ 1]], poly);
            e.herkunft
                .iter()
                .find(|(k, s)| *s == vz && traegt(k.poly))
                .or_else(|| e.herkunft.iter().find(|(k, _)| traegt(k.poly)))
                .unwrap_or(&e.herkunft[0])
                .0
        };
        let n = 2 * self.kanten.len();
        let mut benutzt = vec![false; n];
        let mut ringe = Vec::new();
        for h0 in 0..n {
            if benutzt[h0] || !rand(h0) {
                continue;
            }
            let mut folge = vec![h0];
            benutzt[h0] = true;
            let mut h = h0;
            let geschlossen = loop {
                match folgende(h) {
                    Some(g) if g == h0 => break true,
                    Some(g) if !benutzt[g] => {
                        benutzt[g] = true;
                        folge.push(g);
                        h = g;
                    }
                    _ => break false,
                }
            };
            // Jede Randkante hat genau eine Nachfolgerin und eine Vorgängerin;
            // ein offener Ring hieße, das Netz widerspricht sich
            debug_assert!(
                geschlossen,
                "Randkanten ohne Ring ab {:?}",
                self.p[self.von(h0)]
            );
            if !geschlossen {
                continue;
            }
            let m = folge.len();
            let hk: Vec<Herkunft> = folge.iter().map(|&h| herkunft(h)).collect();
            let start = (0..m).find(|&i| hk[i] != hk[(i + m - 1) % m]).unwrap_or(0);
            let mut r = RandRing {
                ring: Ring::default(),
                ecken: Vec::new(),
                alle: folge.iter().map(|&h| self.von(h)).collect(),
            };
            for s in 0..m {
                let i = (start + s) % m;
                if s > 0 && r.ring.herkunft.last() == Some(&hk[i]) {
                    continue;
                }
                let v = self.von(folge[i]);
                r.ring.punkte.push(self.p[v]);
                r.ring.herkunft.push(hk[i]);
                r.ecken.push(v);
            }
            if r.ring.punkte.len() >= 3 {
                ringe.push(r);
            }
        }
        ringe
    }
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
///
/// Oft sind viele Stellen fast gleich gut, längs der Mitte eines
/// rechteckigen Raums alle. Welche davon die Suche griffe, hinge an Raster
/// und Rundung und spränge, während eine Wand gezogen wird (Grundriss G7).
/// Darum gilt unter den Stellen, deren Abstand höchstens `genau` unter dem
/// besten liegt, die dem Schwerpunkt des Rands nächste: im Rechteck der
/// Schwerpunkt selbst. In schmalen Räumen, wo `genau` an den besten Abstand
/// heranreicht, höchstens ein Zehntel darunter, so bleibt die Stelle innen.
/// Liegt der Schwerpunkt außerhalb, etwa in einem Pfeiler oder Kern, sind in
/// symmetrischen Räumen mehrere Stellen gleich nah; der Weg dorthin ist
/// darum leicht nach links unten geneigt ([`NEIGUNG`]), so gewinnt immer
/// dieselbe (Review #140 W46).
pub fn innenpunkt(b: &Bereich, genau: f64) -> (Vec3, f64) {
    innenpunkt_mit(b, genau, |_| f64::INFINITY)
}

/// [`innenpunkt`], der außerdem Abstand zu etwas im Bereich hält, das frei
/// bleiben soll (etwa der Aufschlag einer Tür): `weg` ist der Abstand dazu
/// (mm). Er darf sich je mm Weg um höchstens 1 mm ändern, wie jeder Abstand
/// zu einer Menge, sonst trägt die Schranke der Zellen nicht.
pub fn innenpunkt_mit(b: &Bereich, genau: f64, weg: impl Fn(Vec3) -> f64) -> (Vec3, f64) {
    use std::cmp::Ordering;
    use std::collections::BinaryHeap;

    /// `rang`: wonach die Schlange zuerst zieht.
    struct Zelle {
        c: Vec3,
        h: f64,
        d: f64,
        rang: f64,
    }
    impl Zelle {
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
            self.rang.total_cmp(&o.rang)
        }
    }

    // Wie oft der Abstand gerechnet wurde, für das Budget der zweiten Suche
    let gerechnet = std::cell::Cell::new(0usize);
    let neu = |c: Vec3, h: f64| {
        gerechnet.set(gerechnet.get() + 1);
        let d = abstand(b, c).min(weg(c));
        Zelle {
            c,
            h,
            d,
            rang: d + h * std::f64::consts::SQRT_2,
        }
    };
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
    let raster = |zelle: &dyn Fn(Vec3, f64) -> Zelle| {
        let mut schlange = BinaryHeap::new();
        let mut x = x0;
        while x < x1 {
            let mut y = y0;
            while y < y1 {
                schlange.push(zelle(vec3(x + h, y + h, 0.0), h));
                y += seite;
            }
            x += seite;
        }
        schlange
    };
    let viertel = [(-1.0, -1.0), (1.0, -1.0), (-1.0, 1.0), (1.0, 1.0)];
    let mitte =
        crate::polygon::centroid(pts).unwrap_or(vec3((x0 + x1) / 2.0, (y0 + y1) / 2.0, 0.0));
    let mitte = vec3(mitte.x, mitte.y, 0.0);
    // Zellen teilen, bis keine den besten Abstand um mehr als `bis()`
    // verbessern kann; die, die ihn noch um mehr als `scharf` verbessern
    // könnten, kommen nach `rest`
    let scharf = genau / 1000.0;
    let teilen = |mut schlange: BinaryHeap<Zelle>,
                  best: &mut Zelle,
                  bis: &dyn Fn() -> f64,
                  rest: &mut Vec<Zelle>| {
        while let Some(z) = schlange.pop() {
            if z.d > best.d {
                *best = Zelle { h: 0.0, ..z };
            }
            let luft = z.hoechstens() - best.d;
            if luft <= bis() {
                if luft > scharf {
                    rest.push(z);
                }
                continue;
            }
            let h = z.h / 2.0;
            for (sx, sy) in viertel {
                schlange.push(neu(z.c + vec3(sx * h, sy * h, 0.0), h));
            }
        }
    };
    let mut best = neu(mitte, 0.0);
    let mut rest = Vec::new();
    teilen(raster(&neu), &mut best, &|| genau, &mut rest);
    let erste = gerechnet.get();

    // Unter den Stellen mit mindestens `soll` die dem Schwerpunkt nächste,
    // gemessen mit `weit`: der Weg dorthin, geneigt um `neigung`. Am
    // Schwerpunkt ist `weit` null, sonst größer; gilt er, bleibt er es.
    // Zellen aufsteigend nach ihrem kleinstmöglichen `weit` teilen, bis keine
    // um mehr als `fein` besser sein kann, so genau, dass die Stelle beim
    // Ziehen nicht zittert. Bei vielen Kanten (Rundungen) rechnet sie den
    // Abstand höchstens doppelt so oft wie die erste Suche, dann gilt die
    // beste Stelle bis dahin (Review #140 H1); mindestens aber [`BUDGET`]
    // durch die Zahl der Kanten, bei 40 Kanten 5.000 Mal.
    let soll = |d: f64| d - genau.min(0.1 * d);
    let m = neu(mitte, 0.0);
    // Der beste Abstand liegt höchstens `genau` über `best.d`. Reicht der
    // Schwerpunkt auch dafür, gilt er (im Rechteck immer). Sonst hängt die
    // Stelle an `soll`; schwankte der beste Abstand beim Ziehen um `genau`,
    // wechselte sie, wo `soll` an der halben Breite weiter Teile liegt,
    // zwischen ihnen hin und her (Review #140 H2: U mit 1,5 m breiten
    // Schenkeln). Darum ihn dann auf `scharf` genau, mit höchstens viermal
    // so vielen Abständen wie bis hier und höchstens einem Viertel von
    // [`BUDGET`] durch die Zahl der Kanten; wo der beste Abstand auf einer
    // Linie liegt (Flur, Kreisring), reicht das nicht, dort bleibt `genau`.
    if m.d >= soll(best.d + genau) {
        return (m.c, m.d);
    }
    let kanten = pts.len() + b.inseln.iter().map(|i| i.punkte.len()).sum::<usize>();
    let vorab = erste + (4 * erste).min(BUDGET / 4 / kanten.max(1));
    let bis = || {
        if gerechnet.get() > vorab {
            genau
        } else {
            scharf
        }
    };
    let schlange = std::mem::take(&mut rest).into_iter().collect();
    teilen(schlange, &mut best, &bis, &mut rest);
    let soll = soll(best.d);
    if m.d >= soll {
        return (m.c, m.d);
    }
    let fein = genau / 100.0;
    let neigung = vec3(1.0, 2.0, 0.0) * (NEIGUNG / 5f64.sqrt());
    let weit = |c: Vec3| (c - mitte).length() + neigung.dot(c - mitte);
    let nah = |c: Vec3, h: f64| {
        // Je für sich das Kleinste in der Zelle: Weg zum Quadrat, Neigung
        let s = c - mitte;
        let (dx, dy) = ((s.x.abs() - h).max(0.0), (s.y.abs() - h).max(0.0));
        let schraeg = neigung.dot(s) - h * (neigung.x.abs() + neigung.y.abs());
        Zelle {
            rang: -((dx * dx + dy * dy).sqrt() + schraeg).max(0.0),
            ..neu(c, h)
        }
    };
    let mut wahl = (best.c, best.d, weit(best.c));
    let mut schlange = raster(&nah);
    let budget = gerechnet.get() + (2 * erste).max(BUDGET / kanten.max(1));
    while let Some(z) = schlange.pop() {
        if z.d >= soll {
            let r = weit(z.c);
            if r < wahl.2 {
                wahl = (z.c, z.d, r);
            }
        }
        if -z.rang >= wahl.2 - fein || gerechnet.get() > budget {
            break;
        }
        if z.hoechstens() < soll || z.h < fein {
            continue;
        }
        let h = z.h / 2.0;
        for (sx, sy) in viertel {
            schlange.push(nah(z.c + vec3(sx * h, sy * h, 0.0), h));
        }
    }
    (wahl.0, wahl.1)
}

/// Neigung des Wegs zum Schwerpunkt bei [`innenpunkt`]: je mm nach rechts
/// zählt er 0,045 mm mehr, je mm nach oben 0,089 mm (0,1 in Richtung (1, 2)).
/// Kleiner als 1, so bleibt der Schwerpunkt selbst die nächste Stelle.
const NEIGUNG: f64 = 0.1;

/// Abstände mal Kanten, die die zweite Suche von [`innenpunkt`] mindestens
/// rechnen darf; für den genaueren besten Abstand höchstens ein Viertel.
const BUDGET: usize = 200_000;

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
        // Längs der Mitte ist jede Stelle gleich gut, es gilt der Schwerpunkt
        assert!((p - vec3(2000.0, 1500.0, 0.0)).length() < 1e-6, "{p:?}");
        // L-Form: der breitere Schenkel gewinnt; von den Stellen höchstens
        // `genau` unter dem besten Abstand die dem Schwerpunkt (2500, 3000)
        // nächste, am oberen Rand des Streifens um y = 2000, mit der Neigung
        // ein Stück nach links: wo sie den Zuwachs des Wegs aufwiegt
        let l = vec![
            vec3(0.0, 0.0, 0.0),
            vec3(6000.0, 0.0, 0.0),
            vec3(6000.0, 4000.0, 0.0),
            vec3(2000.0, 4000.0, 0.0),
            vec3(2000.0, 8000.0, 0.0),
            vec3(0.0, 8000.0, 0.0),
        ];
        let (p, d) = innenpunkt(&bereich(l, vec![]), 1.0);
        assert!(d > 1998.0 - 1e-6 && d < 2000.0 + 1e-6, "{d}");
        let k = NEIGUNG / 5f64.sqrt();
        let x = 2500.0 - (3000.0 - p.y) * k / (1.0 - k * k).sqrt();
        assert!(
            (p.x - x).abs() < 1.0 && (p.y - (4000.0 - d)).abs() < 1.0,
            "{p:?} {x}"
        );
    }

    /// Grundriss G7 (Jörn): Wird eine Wand gezogen, wandert die Stelle stetig
    /// mit, statt zwischen gleich guten Stellen längs der Mitte zu springen.
    /// Rechteck 8 m tief, das Rechte einmal mit Türnische, Genauigkeit wie
    /// beim Raumstempel (2 % der kürzeren Seite).
    #[test]
    fn innenpunkt_springt_nicht() {
        for nische in [false, true] {
            let mut vorher: Option<Vec3> = None;
            for k in 0..60 {
                let w = 5000.0 + 7.3 * k as f64;
                let mut rand = vec![vec3(0.0, 0.0, 0.0), vec3(w, 0.0, 0.0)];
                if nische {
                    rand.extend([
                        vec3(w, 5500.0, 0.0),
                        vec3(w + 120.0, 5500.0, 0.0),
                        vec3(w + 120.0, 6400.0, 0.0),
                        vec3(w, 6400.0, 0.0),
                    ]);
                }
                rand.extend([vec3(w, 8000.0, 0.0), vec3(0.0, 8000.0, 0.0)]);
                let (p, d) = innenpunkt(&bereich(rand, vec![]), 0.02 * w);
                assert!(d > w / 2.0 - 0.02 * w, "{w}: {d}");
                if let Some(q) = vorher {
                    assert!((p - q).length() < 10.0, "{nische} {w}: {q:?} → {p:?}");
                }
                vorher = Some(p);
            }
        }
    }

    /// Review #140 W46: Liegt der Schwerpunkt in einem Pfeiler, einem Kern
    /// oder außerhalb eines symmetrischen U, sind mehrere Stellen gleich nah.
    /// Wird eine Wand gezogen (200 Schritte zu 7,3 mm), bleibt die Stelle bei
    /// derselben: Sprünge über 30 mm höchstens wenige (main: Pfeiler 42, Kern
    /// 19), über 300 mm nur, wenn ein anderer Teil des Raums besser wird (beim
    /// Kern der rechte Flur, in zwei Schritten); bleibt der Raum symmetrisch,
    /// keiner.
    #[test]
    fn innenpunkt_springt_nicht_um_pfeiler_und_kern() {
        let u = |w: f64, h: f64| {
            vec![
                vec3(0.0, 0.0, 0.0),
                vec3(w, 0.0, 0.0),
                vec3(w, h, 0.0),
                vec3(w - 1500.0, h, 0.0),
                vec3(w - 1500.0, 1500.0, 0.0),
                vec3(1500.0, 1500.0, 0.0),
                vec3(1500.0, h, 0.0),
                vec3(0.0, h, 0.0),
            ]
        };
        let pfeiler = rechteck(2300.0, 3800.0, 2700.0, 4200.0);
        let pfeiler_quer = rechteck(3800.0, 2300.0, 4200.0, 2700.0);
        let kern = rechteck(3000.0, 3000.0, 7000.0, 7000.0);
        type Fall<'a> = (&'a str, Box<dyn Fn(f64) -> Bereich + 'a>, usize, usize);
        let faelle: [Fall; 6] = [
            (
                "Pfeiler",
                Box::new(|z| {
                    bereich(
                        rechteck(0.0, 0.0, 5000.0 + z, 8000.0),
                        vec![pfeiler.clone()],
                    )
                }),
                10,
                1,
            ),
            (
                "Pfeiler oben",
                Box::new(|z| {
                    bereich(
                        rechteck(0.0, 0.0, 8000.0, 5000.0 + z),
                        vec![pfeiler_quer.clone()],
                    )
                }),
                10,
                1,
            ),
            (
                "Kern",
                Box::new(|z| bereich(rechteck(0.0, 0.0, 10000.0 + z, 10000.0), vec![kern.clone()])),
                10,
                2,
            ),
            (
                "U rechts",
                Box::new(|z| bereich(u(8000.0 + z, 8000.0), vec![])),
                0,
                0,
            ),
            (
                "U oben",
                Box::new(|z| bereich(u(8000.0, 8000.0 + z), vec![])),
                0,
                0,
            ),
            // Review #140 H2: Bei 6,0 bis 7,45 m Breite liegt `soll` an der
            // halben Breite von Schenkeln und Steg, vorher sieben Sprünge.
            // Einmal wechselt die Stelle in der Ecke links unten um 0,5 m
            // die Seite, wenn der Schwerpunkt über die Diagonale wandert.
            (
                "U 6 bis 7,45 m",
                Box::new(|z| bereich(u(6000.0 + z, 8000.0), vec![])),
                1,
                1,
            ),
        ];
        for (name, raum, ueber30, ueber300) in &faelle {
            let mut vorher: Option<Vec3> = None;
            let (mut n30, mut n300) = (0, 0);
            for k in 0..200 {
                let b = raum(7.3 * k as f64);
                let (w, h) = b
                    .rand
                    .punkte
                    .iter()
                    .fold((0.0f64, 0.0f64), |m, p| (m.0.max(p.x), m.1.max(p.y)));
                let (p, d) = innenpunkt(&b, 0.02 * w.min(h));
                assert!(b.enthaelt(p) && d > 0.0, "{name} {k}: {p:?} {d}");
                if let Some(q) = vorher {
                    let s = (p - q).length();
                    n30 += usize::from(s > 30.0);
                    n300 += usize::from(s > 300.0);
                }
                vorher = Some(p);
            }

            assert!(
                n30 <= *ueber30 && n300 <= *ueber300,
                "{name}: {n30} über 30 mm, {n300} über 300 mm"
            );
        }
    }

    /// Schmaler U-Raum 20 × 20 m (Flur um einen Kern), `genau` größer als
    /// der halbe Flur: Die Stelle bleibt innen, mit Abstand zu den Wänden
    /// (Geometrie, Befund zu #140).
    #[test]
    fn innenpunkt_bleibt_im_schmalen_u() {
        for (s, genau) in [
            (600.0, 400.0),
            (900.0, 400.0),
            (1200.0, 400.0),
            (600.0, 200.0),
        ] {
            let u = vec![
                vec3(0.0, 0.0, 0.0),
                vec3(20000.0, 0.0, 0.0),
                vec3(20000.0, 20000.0, 0.0),
                vec3(20000.0 - s, 20000.0, 0.0),
                vec3(20000.0 - s, s, 0.0),
                vec3(s, s, 0.0),
                vec3(s, 20000.0, 0.0),
                vec3(0.0, 20000.0, 0.0),
            ];
            let b = bereich(u, vec![]);
            let (p, d) = innenpunkt(&b, genau);
            assert!(b.enthaelt(p), "{s} {genau}: {p:?} {d}");
            assert!(
                d > s / 3.0 && (abstand(&b, p) - d).abs() < 1e-6,
                "{s} {genau}: {d}"
            );
        }
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

    /// Viertelkreis 2,50 m um die Ecke unten links frei halten (Aufschlag
    /// einer Tür): Die Stelle rückt auf der Diagonale nach oben rechts, bis
    /// Bogen und Wände gleich weit weg sind.
    #[test]
    fn innenpunkt_mit_haelt_abstand() {
        let b = bereich(rechteck(0.0, 0.0, 4000.0, 4000.0), vec![]);
        let weg = |p: Vec3| (vec3(p.x, p.y, 0.0).length() - 2500.0).max(0.0);
        let (p, d) = innenpunkt_mit(&b, 1.0, weg);
        let t = 6500.0 / (1.0 + std::f64::consts::SQRT_2);
        assert!((d - (4000.0 - t)).abs() < 2.0, "{d}");
        assert!((p - vec3(t, t, 0.0)).length() < 20.0, "{p:?}");
        assert!(weg(p) >= d - 1e-9 && abstand(&b, p) >= d - 1e-9);
        // Ohne Weiteres wie innenpunkt
        assert_eq!(
            innenpunkt_mit(&b, 1.0, |_| f64::INFINITY),
            innenpunkt(&b, 1.0)
        );
    }

    /// Eine Wand, die knapp mehr als die Toleranz vor der nächsten endet,
    /// lässt keinen Raum verschwinden: Der Spalt ist schmaler als der
    /// Prüfabstand links/rechts (4 · tol), dann fallen Spalt- und Stirnkanten
    /// weg und die Ringe brechen ab. Ob ein oder zwei Räume entstehen, ist
    /// offen; die Fläche muss bleiben.
    #[test]
    fn spalt_knapp_ueber_toleranz_verliert_keinen_raum() {
        for g in [0.6, 1.0, 1.5, 1.9] {
            let mut u = aussen();
            u.push(rechteck(300.0, 3950.0, 9700.0 - g, 4050.0));
            let a = anordnung(&u, TOL);
            let summe: f64 = a.bereiche.iter().map(Bereich::flaeche).sum();
            assert!(
                (summe - 9400.0 * 7400.0 + (9400.0 - g) * 100.0).abs() < 1e4,
                "Spalt {g} mm: {:?}",
                flaechen(&a)
            );
        }
    }

    /// Fangweite und Prüfabstand ohne Lücke dazwischen, auch schräg: Bis
    /// `tol` schließt die Querwand zwei Räume, darüber bleibt ein Raum mit
    /// Spalt. Kein Spalt lässt Fläche verschwinden.
    #[test]
    fn spalt_um_die_toleranz_gerade_und_gedreht() {
        let um = vec3(5000.0, 4000.0, 0.0);
        for grad in [0.0_f64, 1.0, 30.0] {
            let (sin, cos) = grad.to_radians().sin_cos();
            let drehen = |p: &Vec3| {
                let d = *p - um;
                um + vec3(d.x * cos - d.y * sin, d.x * sin + d.y * cos, 0.0)
            };
            for g in [0.0, 0.2, 0.49, 0.5, 0.51, 0.6, 1.0, 1.9, 2.0, 2.1, 5.0] {
                if grad != 0.0 && g == TOL {
                    continue; // gedreht liegt der Abstand nur ungefähr auf der Grenze
                }
                let mut u = aussen();
                u.push(rechteck(300.0, 3950.0, 9700.0 - g, 4050.0));
                let u: Vec<Vec<Vec3>> = u.iter().map(|r| r.iter().map(drehen).collect()).collect();
                let a = anordnung(&u, TOL);
                let f = flaechen(&a);
                let (n, soll) = if g <= TOL {
                    (2, 9400.0 * 7400.0 - 9400.0 * 100.0)
                } else {
                    (1, 9400.0 * 7400.0 - (9400.0 - g) * 100.0)
                };
                assert_eq!(a.bereiche.len(), n, "{grad}°, Spalt {g} mm: {f:?}");
                let summe: f64 = f.iter().sum();
                assert!((summe - soll).abs() < 1e3, "{grad}°, Spalt {g} mm: {f:?}");
            }
        }
    }

    /// Kurze Wände mit Gehrung ergeben verdrehte Umrisse (A31: Vorsprung
    /// 5 × 5 cm an einer 31,5 cm dicken Außenwand). Ihre Ränder schließen
    /// trotzdem, innen bleibt ein Raum.
    #[test]
    fn verdrehter_umriss_schliesst() {
        let p = |v: &[(f64, f64)]| v.iter().map(|&(x, y)| vec3(x, y, 0.0)).collect();
        let u: Vec<Vec<Vec3>> = vec![
            p(&[(0.0, 0.0), (0.0, 8000.0), (315.0, 7685.0), (315.0, 315.0)]),
            p(&[
                (0.0, 8000.0),
                (10000.0, 8000.0),
                (9685.0, 7685.0),
                (315.0, 7685.0),
            ]),
            p(&[
                (10000.0, 8000.0),
                (10000.0, 0.0),
                (9685.0, 315.0),
                (9685.0, 7685.0),
            ]),
            p(&[
                (10000.0, 0.0),
                (4050.0, 0.0),
                (3735.0, 315.0),
                (9685.0, 315.0),
            ]),
            p(&[
                (4050.0, 0.0),
                (4050.0, -50.0),
                (3735.0, 265.0),
                (3735.0, 315.0),
            ]),
            p(&[
                (4050.0, -50.0),
                (4000.0, -50.0),
                (4315.0, 265.0),
                (3735.0, 265.0),
            ]),
            p(&[
                (4000.0, -50.0),
                (4000.0, 0.0),
                (4315.0, 315.0),
                (4315.0, 265.0),
            ]),
            p(&[(4000.0, 0.0), (0.0, 0.0), (315.0, 315.0), (4315.0, 315.0)]),
        ];
        let a = anordnung(&u, TOL);
        assert_eq!(a.bereiche.len(), 1, "{:?}", flaechen(&a));
        assert!(
            nah(a.bereiche[0].flaeche(), 9370.0 * 7370.0),
            "{:?}",
            flaechen(&a)
        );
    }

    /// Genau auf der Fangweite (Spalt oder Überdeckung ≈ `tol`, schräg)
    /// entscheidet die Rundung. Beide Ausgänge sind recht, solange Fangen
    /// und Prüfen gleich entscheiden; sonst schließt kein Ring und alle
    /// Räume des Geschosses fehlen (release) bzw. greift das debug_assert.
    #[test]
    fn auf_der_fangweite_bleiben_die_raeume() {
        let um = vec3(5000.0, 4000.0, 0.0);
        // (Drehung in Grad, Spalt der Querwand; die Längswand endet unten
        // g/2 vor bzw. in der Außenwand und oben 0,7·g vor der Querwand)
        for (grad, g) in [(0.123_f64, 0.5), (0.123, -1.0), (90.6, 1.0)] {
            let (sin, cos) = grad.to_radians().sin_cos();
            let drehen = |p: &Vec3| {
                let d = *p - um;
                um + vec3(d.x * cos - d.y * sin, d.x * sin + d.y * cos, 0.0)
                    + vec3(1234.5678, -987.654, 0.0)
            };
            let mut u = aussen();
            u.push(rechteck(300.0, 3950.0, 9700.0 - g, 4050.0));
            u.push(rechteck(4950.0, 300.0 - g * 0.5, 5050.0, 3950.0 - g * 0.7));
            let u: Vec<Vec<Vec3>> = u.iter().map(|r| r.iter().map(drehen).collect()).collect();
            let a = anordnung(&u, TOL);
            let summe: f64 = a.bereiche.iter().map(Bereich::flaeche).sum();
            let soll = 9400.0 * 7400.0
                - (9400.0 - g.max(0.0)) * 100.0
                - (3650.0 - g.max(0.0) * 1.2) * 100.0;
            assert!(
                (summe - soll).abs() < 2e3,
                "{grad}°, Spalt {g} mm: {:?}",
                flaechen(&a)
            );
        }
    }

    /// Durchlauf der Geometrie zu #87: Querwand und Längswand mit Spalt
    /// bzw. Überdeckung von −1 bis 2 mm, gedreht von 0 bis 91°, mit den
    /// Fällen genau auf der Fangweite. Die Fläche bleibt, jeder Ring schließt
    /// (sonst greift das debug_assert).
    #[test]
    fn spalt_und_drehung_durchlauf() {
        let um = vec3(5000.0, 4000.0, 0.0);
        let mut grade: Vec<f64> = (0..=13).map(|i| i as f64 * 7.0).collect();
        grade.extend([0.123, 1.0, 45.5, 89.9, 90.6, 91.0]);
        let mut spalte: Vec<f64> = (0..=12).map(|i| -1.0 + i as f64 * 0.25).collect();
        spalte.extend([-0.51, -0.49, 0.49, 0.51, 0.7, 0.71, 1.4]);
        for &grad in &grade {
            let (sin, cos) = grad.to_radians().sin_cos();
            let drehen = |p: &Vec3| {
                let d = *p - um;
                um + vec3(d.x * cos - d.y * sin, d.x * sin + d.y * cos, 0.0)
                    + vec3(1234.5678, -987.654, 0.0)
            };
            for &g in &spalte {
                let mut u = aussen();
                u.push(rechteck(300.0, 3950.0, 9700.0 - g, 4050.0));
                u.push(rechteck(4950.0, 300.0 - g * 0.5, 5050.0, 3950.0 - g * 0.7));
                let u: Vec<Vec<Vec3>> = u.iter().map(|r| r.iter().map(drehen).collect()).collect();
                let a = anordnung(&u, TOL);
                let summe: f64 = a.bereiche.iter().map(Bereich::flaeche).sum();
                let soll = 9400.0 * 7400.0
                    - (9400.0 - g.max(0.0)) * 100.0
                    - (3650.0 - g.max(0.0) * 1.2) * 100.0;
                assert!(
                    (summe - soll).abs() < 2e3,
                    "{grad}°, Spalt {g} mm: {:?}",
                    flaechen(&a)
                );
            }
        }
    }
}
