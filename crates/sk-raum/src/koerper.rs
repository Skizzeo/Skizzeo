//! Körper des Ausbaus eines Raums (Raumbuch RB4, `raumbuch/plan.md` §8):
//! Estrichaufbau und Belag als Schichten über der Rohdecke, Deckenputz,
//! Wandputz je Seite mit Laibungen, Wandfliesen und Sockelleisten. Putz
//! und Belag sind echte Körper (Jörn 17:43); welche Ansicht sie zeigt,
//! entscheidet der Schalter „Ausbau innen“, der Estrich steht im Schnitt
//! immer.
//!
//! Die Körper folgen denselben Flächen wie die Mengen
//! ([`super::rechnen`]), aber exakt, ohne Übermessung: Estrich und Belag
//! bis zur Putzoberfläche mit Streifen und Nischen an den Öffnungen am
//! Boden; Deckenputz im Rohbaumaß, Wandputz darunter bis UK Deckenputz,
//! zwischen Rohbaufläche und Putzoberfläche mit Gehrung in den Ecken, ohne
//! die Öffnungen, mit Laibungen bis zum Element (an Öffnungen am Boden ab
//! OK Fertigfußboden). Fliesen und Sockelleisten laufen bis an die
//! geputzte Laibung; die Mengen ziehen nach VOB/C das Rohbaumaß ab.
//! Schichten ohne Dicke (Anstrich, Tapete) haben keinen Körper; sie färben
//! die Fläche zum Raum. Eine Wandbekleidung ist ein Körper aus dem
//! Baustoff ihrer dicksten Schicht. Fliesen an Laibungen haben noch keinen
//! Körper. Keine zwei Körper überschneiden sich.
//!
//! Jeder Körper besteht aus senkrechten Prismen (Umriss im Grundriss mal
//! Höhe); aus ihnen schneidet [`Koerper::geschnitten`] Grundriss und
//! Schnitt mit Schnittflächen.

use super::{Flaeche, Raum};
use crate::ausbau::{self, laibung_behandelt, RaumAusbau};
use crate::oeffnungen::{OeffnungAmRaum, OpeningKind, Rohbau};
use crate::ring;
use sk_math::polygon::signed_area;
use sk_math::{vec3, Vec3};
use sk_model::{edge_kind, material, material_key, Edge, LayerSetId, Model, Solid, Tri};

/// Ein Körper des Ausbaus.
#[derive(Clone, Debug)]
pub struct Koerper {
    pub flaeche: Flaeche,
    /// Ausbautyp und Schicht darin.
    pub typ: LayerSetId,
    pub schicht: usize,
    pub solid: Solid,
    /// Die Prismen, aus denen er besteht.
    pub prismen: Vec<Prisma>,
    /// Baustoff der Schnittflächen.
    pub schnitt: u16,
}

/// Senkrechtes Prisma: Umriss im Grundriss zwischen zwei Höhen.
#[derive(Clone, Debug, PartialEq)]
pub struct Prisma {
    /// Umriss gegen den Uhrzeigersinn, dann die Löcher im Uhrzeigersinn.
    pub ringe: Vec<Vec<Vec3>>,
    /// Der Umriss mit den Löchern über Schlitze (zum Füllen).
    deckel: Vec<Vec3>,
    pub z: (f64, f64),
}

impl Koerper {
    /// Schnitt mit der Ebene durch `p0` mit Normale `n`: der Teil hinter der
    /// Ebene (Gegenseite von `n`) mit Schnittflächen im Baustoff
    /// [`Koerper::schnitt`] und Schnittkonturen, wie die Körper der
    /// Erweiterung. Grundriss: `n` nach oben, `p0` in Schnitthöhe; Schnitt:
    /// `n` waagerecht. Eine schräge Ebene schneidet ohne Schnittflächen.
    /// Liegt die Ebene auf einer Fläche oder Fuge eines Prismas, gehört die
    /// Fläche zu dem Teil, aus dem sie nach vorn zeigt; das Prisma dahinter
    /// bekommt dort keine Schnittfläche (Geometrie zu #137).
    pub fn geschnitten(&self, p0: Vec3, n: Vec3) -> Solid {
        let mut s = self.solid.clipped(p0, n);
        let auf = |p: Vec3| (p - p0).dot(n).abs() < 1e-6;
        s.triangles
            .retain(|t| t.n.dot(n) > 0.0 || !t.p.iter().all(|&p| auf(p)));
        let mut k = Solid {
            mat: self.schnitt | material::CUT,
            edge_kind: edge_kind::CUT,
            ..Solid::default()
        };
        for p in &self.prismen {
            let (z0, z1) = p.z;
            if n.z.abs() > 1.0 - 1e-9 {
                let z = p0.z;
                if z <= z0 + 1e-9 || z >= z1 - 1e-9 {
                    continue;
                }
                k.cap(&p.deckel, z, n.z > 0.0);
                for (a, b) in p.ringe.iter().flat_map(|r| kanten(r)) {
                    k.edge(hoch(a, z), hoch(b, z));
                }
            } else if n.z.abs() < 1e-9 {
                for (a, b) in quer(&p.ringe, p0, n) {
                    let mut q = [hoch(a, z0), hoch(b, z0), hoch(b, z1), hoch(a, z1)];
                    if (q[1] - q[0]).cross(q[3] - q[0]).dot(n) < 0.0 {
                        q.reverse();
                    }
                    k.quad(q[0], q[1], q[2], q[3], n);
                    for i in 0..4 {
                        k.edge(q[i], q[(i + 1) % 4]);
                    }
                }
            }
        }
        // keine Fugen zwischen den Prismen eines Körpers
        kontur(&mut k);
        s.append(&k);
        s
    }
}

/// Strecken der senkrechten Ebene durch `p0` mit Normale `n` im Grundriss
/// innerhalb der Ringe (Umriss und Löcher), ohne die auf einer Kante der
/// Ringe: Dort liegt das Prisma nur hinter der Ebene und hat seine Fläche.
fn quer(ringe: &[Vec<Vec3>], p0: Vec3, n: Vec3) -> Vec<(Vec3, Vec3)> {
    let n = vec3(n.x, n.y, 0.0).normalized();
    let r = vec3(-n.y, n.x, 0.0);
    let o = hoch(p0, 0.0);
    let seite = |q: Vec3| (hoch(q, 0.0) - o).dot(n);
    let mut t: Vec<f64> = Vec::new();
    for (a, b) in ringe.iter().flat_map(|r| kanten(r)) {
        let (sa, sb) = (seite(a), seite(b));
        // halboffen, damit eine Ecke auf der Geraden einmal zählt
        if (sa < 0.0) != (sb < 0.0) {
            let x = hoch(a + (b - a) * (sa / (sa - sb)), 0.0);
            t.push((x - o).dot(r));
        }
    }
    t.sort_by(f64::total_cmp);
    let auf_kante = |u: f64| {
        ringe.iter().flat_map(|ring| kanten(ring)).any(|(a, b)| {
            let (ua, ub) = ((hoch(a, 0.0) - o).dot(r), (hoch(b, 0.0) - o).dot(r));
            seite(a).abs() < 1e-9 && seite(b).abs() < 1e-9 && ua.min(ub) <= u && u <= ua.max(ub)
        })
    };
    t.chunks_exact(2)
        .filter(|c| c[1] - c[0] > 1e-9 && !auf_kante((c[0] + c[1]) / 2.0))
        .map(|c| (o + r * c[0], o + r * c[1]))
        .collect()
}

/// Ein Körper im Bau: Netz und Prismen.
#[derive(Default)]
struct Bau {
    s: Solid,
    prismen: Vec<Prisma>,
}

impl Bau {
    fn koerper(self, flaeche: Flaeche, typ: LayerSetId, schicht: usize, schnitt: u16) -> Koerper {
        Koerper {
            flaeche,
            typ,
            schicht,
            solid: self.s,
            prismen: self.prismen,
            schnitt,
        }
    }
}

/// Breite des Schlitzes an seinen Enden, über den ein Loch an den Umriss
/// angeschlossen wird (mm); unsichtbar, nur damit das Polygon einfach
/// bleibt.
const SCHLITZ: f64 = 0.01;

fn kreuz(a: Vec3, b: Vec3) -> f64 {
    a.x * b.y - a.y * b.x
}

/// Schneiden sich die Strecken `a–b` und `c–d` echt (ohne Berührung an den
/// Enden)?
fn schneiden(a: Vec3, b: Vec3, c: Vec3, d: Vec3) -> bool {
    let d1 = kreuz(b - a, c - a);
    let d2 = kreuz(b - a, d - a);
    let d3 = kreuz(d - c, a - c);
    let d4 = kreuz(d - c, b - c);
    d1 * d2 < -1e-9 && d3 * d4 < -1e-9
}

fn kanten(r: &[Vec3]) -> impl Iterator<Item = (Vec3, Vec3)> + '_ {
    let n = r.len();
    (0..n).map(move |i| (r[i], r[(i + 1) % n]))
}

/// Zeigt `q` von der Ecke `i` des Polygons (gegen den Uhrzeigersinn) aus
/// ins Innere?
fn nach_innen(poly: &[Vec3], i: usize, q: Vec3) -> bool {
    let n = poly.len();
    let (a, p, c) = (poly[(i + n - 1) % n], poly[i], poly[(i + 1) % n]);
    let (e1, e2, d) = (p - a, c - p, q - p);
    let links = |u: Vec3| kreuz(u, d) > 1e-9;
    if kreuz(e1, e2) >= 0.0 {
        links(e1) && links(e2)
    } else {
        links(e1) || links(e2)
    }
}

/// Ein Polygon gegen den Uhrzeigersinn ohne Löcher: jedes Loch (im
/// Uhrzeigersinn) hängt über einen schmalen Schlitz am Umriss. Findet sich
/// keine freie Verbindung, bleibt das Loch weg.
fn mit_schlitzen(aussen: &[Vec3], loecher: &[Vec<Vec3>]) -> Vec<Vec3> {
    let mut poly = aussen.to_vec();
    for (h, loch) in loecher.iter().enumerate() {
        let frei = |p: Vec3, q: Vec3, poly: &[Vec3]| {
            let mut alle = kanten(poly).chain(loecher[h..].iter().flat_map(|l| kanten(l)));
            !alle.any(|(c, d)| schneiden(p, q, c, d))
        };
        let mut best: Option<(f64, usize, usize)> = None;
        for (i, &p) in poly.iter().enumerate() {
            for (j, &q) in loch.iter().enumerate() {
                let d = (q - p).length();
                if best.is_some_and(|b| b.0 <= d) || !nach_innen(&poly, i, q) || !frei(p, q, &poly)
                {
                    continue;
                }
                best = Some((d, i, j));
            }
        }
        let Some((_, i, j)) = best else {
            continue;
        };
        // zurück knapp neben der Verbindung, auf den Kanten von Loch und
        // Umriss: so bleibt das Polygon im Raum
        let m = loch.len();
        let (p, q) = (poly[i], loch[j]);
        let neben = |a: Vec3, b: Vec3| a + (b - a) * (SCHLITZ / (b - a).length()).min(0.5);
        let mut neu: Vec<Vec3> = poly[..=i].to_vec();
        neu.extend((0..m).map(|k| loch[(j + k) % m]));
        neu.push(neben(q, loch[(j + m - 1) % m]));
        neu.push(neben(p, poly[(i + 1) % poly.len()]));
        neu.extend_from_slice(&poly[i + 1..]);
        poly = neu;
    }
    poly
}

fn hoch(p: Vec3, z: f64) -> Vec3 {
    vec3(p.x, p.y, z)
}

/// Prisma über dem Umriss `aussen` mit Löchern zwischen `z0` und `z1`, nach
/// außen gerichtet, mit Sichtkanten; Baustoffe oben, unten, Seiten.
fn prisma(b: &mut Bau, aussen: &[Vec3], loecher: &[Vec<Vec3>], z: (f64, f64), m: [u16; 3]) {
    let s = &mut b.s;
    let (z0, z1) = z;
    if aussen.len() < 3 || z1 - z0 <= 1e-9 {
        return;
    }
    let mut a = aussen.to_vec();
    if signed_area(&a) < 0.0 {
        a.reverse();
    }
    let loecher: Vec<Vec<Vec3>> = loecher
        .iter()
        .filter(|l| l.len() >= 3)
        .map(|l| {
            let mut l = l.clone();
            if signed_area(&l) > 0.0 {
                l.reverse();
            }
            l
        })
        .collect();
    let deckel = mit_schlitzen(&a, &loecher);
    s.mat = m[0];
    s.cap(&deckel, z1, true);
    s.mat = m[1];
    s.cap(&deckel, z0, false);
    s.mat = m[2];
    s.edge_kind = edge_kind::VIEW;
    for r in std::iter::once(&a).chain(&loecher) {
        s.sides(r, z0, z1, true);
        for (i, (p, q)) in kanten(r).enumerate() {
            s.edge(hoch(p, z0), hoch(q, z0));
            s.edge(hoch(p, z1), hoch(q, z1));
            if ring::innenecke(r, i) || ring::springt_vor(r, i) {
                s.edge(hoch(p, z0), hoch(p, z1));
            }
        }
    }
    b.prismen.push(Prisma {
        ringe: std::iter::once(a).chain(loecher).collect(),
        deckel,
        z,
    });
}

/// Geschlossener Raumkörper über dem Umriss mit Löchern zwischen `z.0` und
/// `z.1`, ohne Baustoff (für den Austausch, [`crate::raumliste`]).
pub(crate) fn raumkoerper(aussen: &[Vec3], loecher: &[Vec<Vec3>], z: (f64, f64)) -> Solid {
    let mut b = Bau {
        s: Solid::default(),
        prismen: Vec::new(),
    };
    prisma(&mut b, aussen, loecher, z, [0; 3]);
    b.s
}

/// Quader über dem ebenen, konvexen Viereck `p` zwischen `z0` und `z1`.
/// Baustoffe: oben, unten, dann je Seite `p[i]`–`p[i+1]`.
fn klotz(b: &mut Bau, p: [Vec3; 4], z: (f64, f64), m: [u16; 6]) {
    let s = &mut b.s;
    let (z0, z1) = z;
    let a = signed_area(&p);
    if z1 - z0 <= 1e-9 || a.abs() <= 1e-6 {
        return;
    }
    let (mut p, mut m) = (p, m);
    if a < 0.0 {
        // gegen den Uhrzeigersinn; Seite i wird zu Seite 2 − i (Rest 4)
        p.reverse();
        let seiten = [m[4], m[3], m[2], m[5]];
        m = [m[0], m[1], seiten[0], seiten[1], seiten[2], seiten[3]];
    }
    s.edge_kind = edge_kind::VIEW;
    s.mat = m[0];
    s.quad(
        hoch(p[0], z1),
        hoch(p[1], z1),
        hoch(p[2], z1),
        hoch(p[3], z1),
        vec3(0.0, 0.0, 1.0),
    );
    s.mat = m[1];
    s.quad(
        hoch(p[3], z0),
        hoch(p[2], z0),
        hoch(p[1], z0),
        hoch(p[0], z0),
        vec3(0.0, 0.0, -1.0),
    );
    for i in 0..4 {
        let (a, b) = (p[i], p[(i + 1) % 4]);
        let (_, n) = ring::richtung(a, b);
        s.mat = m[2 + i];
        s.quad(hoch(a, z0), hoch(b, z0), hoch(b, z1), hoch(a, z1), n * -1.0);
        s.edge(hoch(a, z0), hoch(b, z0));
        s.edge(hoch(a, z1), hoch(b, z1));
        s.edge(hoch(a, z0), hoch(a, z1));
    }
    b.prismen.push(Prisma {
        ringe: vec![p.to_vec()],
        deckel: p.to_vec(),
        z,
    });
}

/// Freie Rechtecke einer Wandseite (längs `0..l`, Höhe `z0..z1`) ohne die
/// Öffnungen `loecher` (längs von–bis, Höhe uk–ok).
fn felder(l: f64, z: (f64, f64), loecher: &[(f64, f64, f64, f64)]) -> Vec<(f64, f64, f64, f64)> {
    let (z0, z1) = z;
    let mut xs = vec![0.0, l];
    for &(a, b, ..) in loecher {
        xs.push(a.clamp(0.0, l));
        xs.push(b.clamp(0.0, l));
    }
    xs.sort_by(f64::total_cmp);
    xs.dedup_by(|a, b| (*a - *b).abs() < 1e-6);
    let mut out = Vec::new();
    for w in xs.windows(2) {
        let (s0, s1) = (w[0], w[1]);
        let mitte = 0.5 * (s0 + s1);
        let mut zu: Vec<(f64, f64)> = loecher
            .iter()
            .filter(|o| o.0 < mitte && o.1 > mitte)
            .map(|o| (o.2.max(z0), o.3.min(z1)))
            .filter(|o| o.1 > o.0)
            .collect();
        zu.sort_by(|a, b| a.0.total_cmp(&b.0));
        let mut z = z0;
        for (a, b) in zu {
            if a > z {
                out.push((s0, s1, z, a));
            }
            z = z.max(b);
        }
        if z1 > z {
            out.push((s0, s1, z, z1));
        }
    }
    out
}

/// Schichten eines Bekleidungstyps: die dickste als Körper (Index und
/// Baustoff) und die Farbe zum Raum (erste Schicht, z. B. der Anstrich).
fn bekleidung(model: &Model, typ: LayerSetId) -> Option<(usize, u16, u16)> {
    let t = model.layer_set(typ)?;
    let (j, l) = t
        .layers
        .iter()
        .enumerate()
        .filter(|(_, l)| l.thickness > 0.0)
        .max_by(|a, b| a.1.thickness.total_cmp(&b.1.thickness))?;
    let raum = t
        .layers
        .first()
        .map_or(material_key(l.material), |f| material_key(f.material));
    Some((j, material_key(l.material), raum))
}

/// Halbflächen an der Stelle `m` einer Kante mit Richtung `u`: je Dreieck,
/// das `m` berührt, die Richtung quer zur Kante, in die es von `m` aus
/// reicht, und seine Normale.
fn halbflaechen(tris: &[Tri], m: Vec3, u: Vec3) -> Vec<(Vec3, Vec3)> {
    const EPS: f64 = 1e-3;
    let mut out: Vec<(Vec3, Vec3)> = Vec::new();
    for t in tris {
        let [a, b, c] = t.p;
        let x = |v: Vec3| [v.x, v.y, v.z];
        let (xa, xb, xc, xm) = (x(a), x(b), x(c), x(m));
        if (0..3).any(|i| {
            xm[i] < xa[i].min(xb[i]).min(xc[i]) - EPS || xm[i] > xa[i].max(xb[i]).max(xc[i]) + EPS
        }) {
            continue;
        }
        let r = (b - a).cross(c - a).normalized();
        if r.dot(m - a).abs() > 1e-5 || r.dot(u).abs() > 1e-6 {
            continue;
        }
        let w = r.cross(u).normalized();
        let drin = |q: Vec3| {
            [(a, b), (b, c), (c, a)]
                .iter()
                .all(|&(p, s)| (s - p).cross(q - p).dot(r) >= -1e-9 * (s - p).length())
        };
        for w in [w, -w] {
            let neu = |x: &(Vec3, Vec3)| (x.0 - w).length() > 1e-6 || (x.1 - r).length() > 1e-6;
            if drin(m + w * EPS) && out.iter().all(neu) {
                out.push((w, r));
            }
        }
    }
    out
}

/// Knickt die Oberfläche an der Stelle `m` einer Kante mit Richtung `u`?
/// Deckungsgleiche gegenläufige Flächen (Fugen zwischen den Klötzen eines
/// Körpers) heben sich auf; laufen danach genau zwei Flächen eben durch,
/// knickt sie nicht.
fn knickt(tris: &[Tri], m: Vec3, u: Vec3) -> bool {
    let h = halbflaechen(tris, m, u);
    let gleich = |a: Vec3, b: Vec3| (a - b).length() < 1e-6;
    let rest: Vec<&(Vec3, Vec3)> = h
        .iter()
        .filter(|x| !h.iter().any(|y| gleich(x.0, y.0) && gleich(x.1, -y.1)))
        .collect();
    match rest.as_slice() {
        [] => false,
        [a, b] => !(gleich(a.0, -b.0) && gleich(a.1, b.1)),
        _ => true,
    }
}

/// Nur die Kanten, an denen die Oberfläche knickt: Ein Körper aus
/// mehreren Klötzen (Putzfelder neben und über Öffnungen, Streifen vor den
/// Öffnungen) zeigt keine Fugen zwischen ihnen. Jede Kante wird an den
/// Ecken geteilt, die auf ihr liegen, und je Stück geprüft.
fn kontur(s: &mut Solid) {
    let kasten = |p: &[Vec3]| {
        let mut k = ([f64::MAX; 3], [f64::MIN; 3]);
        for v in p {
            for (i, c) in [v.x, v.y, v.z].into_iter().enumerate() {
                k.0[i] = k.0[i].min(c - 1e-3);
                k.1[i] = k.1[i].max(c + 1e-3);
            }
        }
        k
    };
    let kaesten: Vec<([f64; 3], [f64; 3])> = s.triangles.iter().map(|t| kasten(&t.p)).collect();
    let mut neu: Vec<Edge> = Vec::new();
    for e in &s.edges {
        let l = (e.b - e.a).length();
        if l <= 1e-6 {
            continue;
        }
        let u = (e.b - e.a) * (1.0 / l);
        // nur die Dreiecke in der Nähe der Kante
        let (lo, hi) = kasten(&[e.a, e.b]);
        let nah: Vec<Tri> = s
            .triangles
            .iter()
            .zip(&kaesten)
            .filter(|(_, k)| (0..3).all(|i| k.0[i] <= hi[i] && k.1[i] >= lo[i]))
            .map(|(t, _)| *t)
            .collect();
        let mut teile = vec![0.0, l];
        for p in nah.iter().flat_map(|t| t.p) {
            let t = (p - e.a).dot(u);
            if t > 1e-6 && t < l - 1e-6 && (e.a + u * t - p).length() < 1e-6 {
                teile.push(t);
            }
        }
        teile.sort_by(f64::total_cmp);
        teile.dedup_by(|a, b| (*a - *b).abs() < 1e-6);
        let mut von: Option<f64> = None;
        for w in teile.windows(2) {
            let k = knickt(&nah, e.a + u * (0.5 * (w[0] + w[1])), u);
            match (k, von) {
                (true, None) => von = Some(w[0]),
                (false, Some(a)) => {
                    neu.push(Edge {
                        a: e.a + u * a,
                        b: e.a + u * w[0],
                        ..*e
                    });
                    von = None;
                }
                _ => {}
            }
        }
        if let Some(a) = von {
            neu.push(Edge {
                a: e.a + u * a,
                ..*e
            });
        }
    }
    s.edges = neu;
}

impl Raum<'_> {
    /// Punkt auf Kante `k`: `s` längs ab ihrem Anfang, `t` quer zum Raum.
    fn punkt(&self, k: usize, s: f64, t: f64) -> Vec3 {
        let kante = self.kanten[k];
        let (u, n) = ring::richtung(kante.a, kante.b);
        hoch(kante.a, 0.0) + u * s + n * t
    }

    fn rechteck(&self, k: usize, s: (f64, f64), t: (f64, f64)) -> [Vec3; 4] {
        [
            self.punkt(k, s.0, t.0),
            self.punkt(k, s.1, t.0),
            self.punkt(k, s.1, t.1),
            self.punkt(k, s.0, t.1),
        ]
    }

    /// Je Kante Anfang und Ende ihrer Lage im Abstand `d[k]` zum Raum hin,
    /// auf Gehrung mit den Nachbarn.
    fn enden(&self, d: &[f64]) -> Vec<(Vec3, Vec3)> {
        self.ringe
            .iter()
            .flat_map(|(e, p)| ring::versetzt(p, &d[*e..*e + p.len()]).enden)
            .collect()
    }

    /// Viereck einer Schicht vor der Seite `k` zwischen den Lagen `innen` und
    /// `aussen` (Abstände je Kante und ihre Enden), längs `s0..s1`; Seite 2
    /// des Vierecks liegt zum Raum.
    fn streifen(
        &self,
        k: usize,
        s: (f64, f64),
        innen: (&[f64], &[(Vec3, Vec3)]),
        aussen: (&[f64], &[(Vec3, Vec3)]),
    ) -> [Vec3; 4] {
        let l = self.roh[k];
        let lage = |d: &[f64], e: &[(Vec3, Vec3)], s: f64| {
            if s <= 1e-6 {
                e[k].0
            } else if s >= l - 1e-6 {
                e[k].1
            } else {
                self.punkt(k, s, d[k])
            }
        };
        [
            lage(innen.0, innen.1, s.0),
            lage(innen.0, innen.1, s.1),
            lage(aussen.0, aussen.1, s.1),
            lage(aussen.0, aussen.1, s.0),
        ]
    }

    /// Anteil der Nische einer Öffnung am Boden dieses Raums (§5.5).
    fn nische(&self, o: &OeffnungAmRaum) -> f64 {
        match (o.art, o.aufschlag_hier) {
            (OpeningKind::InnerDoor, Some(true)) => 0.0,
            (OpeningKind::InnerDoor, None) => 0.5,
            _ => 1.0,
        }
    }

    /// Rechtecke des Bodens an den Öffnungen: Streifen vor der Öffnung
    /// (im Eck einer Ecköffnung nur einmal), Nische, Eckfeld.
    fn boden_extra(&self) -> Vec<[Vec3; 4]> {
        let mut extra = Vec::new();
        for o in self.form.oeffnungen.iter().filter(|o| self.am_boden(o)) {
            let k = o.kante;
            let p = self.putz[k];
            let (am_anfang, am_ende) = self.ueber_ecke(o);
            let nische = self.mit_nische(o);
            let s0 = if am_anfang {
                o.von + self.putz[self.vorige(k)]
            } else {
                o.von
            };
            if p > 0.0 && o.bis > s0 {
                extra.push(self.rechteck(k, (s0, o.bis), (0.0, p)));
            }
            let d = if nische {
                o.tiefe * self.nische(o)
            } else {
                0.0
            };
            if d > 0.0 {
                extra.push(self.rechteck(k, (o.von, o.bis), (-d, 0.0)));
            }
            if nische && am_ende && o.art != OpeningKind::InnerDoor {
                let n = self.naechste(k);
                if let Some(q) = self
                    .an(n)
                    .find(|q| q.oeffnung == o.oeffnung && q.von <= 0.5)
                {
                    let l = self.roh[k];
                    extra.push(self.rechteck(k, (l, l + q.tiefe), (-o.tiefe, 0.0)));
                }
            }
        }
        extra
    }

    fn boden_koerper(&self, out: &mut Vec<Koerper>) {
        let aussen = &self.ringe_putz[0].punkte;
        let loecher: Vec<Vec<Vec3>> = self.ringe_putz[1..]
            .iter()
            .map(|v| v.punkte.clone())
            .collect();
        let extra = self.boden_extra();
        // von unten nach oben: Estrichaufbau, dann Belag, je unterste Schicht zuerst
        let mut stapel: Vec<(Flaeche, LayerSetId, u16, usize, f64)> = Vec::new();
        let schichten =
            |id: Option<LayerSetId>| id.and_then(|id| Some((id, self.model.layer_set(id)?)));
        if let Some((e, t)) = schichten(self.aus.estrich) {
            for (j, l) in t.layers.iter().enumerate().rev() {
                let d = self.estrich.get(j).copied().unwrap_or(l.thickness);
                stapel.push((Flaeche::Estrich, e, material_key(l.material), j, d));
            }
        }
        if let Some((b, t)) = schichten(self.aus.boden) {
            for (j, l) in t.layers.iter().enumerate().rev() {
                stapel.push((Flaeche::Boden, b, material_key(l.material), j, l.thickness));
            }
        }
        let mut z = self.form.z_boden;
        for (flaeche, typ, m, schicht, d) in stapel {
            let zs = (z, z + d);
            z += d;
            if d <= 0.0 {
                continue;
            }
            let mut b = Bau::default();
            prisma(&mut b, aussen, &loecher, zs, [m; 3]);
            for r in &extra {
                klotz(&mut b, *r, zs, [m; 6]);
            }
            out.push(b.koerper(flaeche, typ, schicht, m));
        }
    }

    fn decke_koerper(&self, out: &mut Vec<Koerper>) {
        let Some((typ, t)) = self
            .aus
            .decke
            .and_then(|id| Some((id, self.model.layer_set(id)?)))
        else {
            return;
        };
        let aussen = &self.ringe[0].1;
        let loecher: Vec<Vec<Vec3>> = self.ringe[1..].iter().map(|r| r.1.clone()).collect();
        let raum = t.layers.first().map(|l| material_key(l.material));
        let unterste = t.layers.iter().position(|l| l.thickness > 0.0);
        // von der Rohdecke nach unten: letzte Schicht zuerst
        let mut z = self.form.z_decke;
        for (j, l) in t.layers.iter().enumerate().rev() {
            let zs = (z - l.thickness, z);
            z -= l.thickness;
            if l.thickness <= 0.0 {
                continue;
            }
            let m = material_key(l.material);
            let unten = if Some(j) == unterste {
                raum.unwrap_or(m)
            } else {
                m
            };
            let mut b = Bau::default();
            prisma(&mut b, aussen, &loecher, zs, [m, unten, m]);
            out.push(b.koerper(Flaeche::Decke, typ, j, m));
        }
    }

    /// Öffnungen der Seite `k` vor der Putzoberfläche als Rechtecke längs
    /// und in der Höhe: um den Putz geputzter Laibungen schmaler (an der
    /// offenen Ecke nicht) und oben niedriger. Bis dahin laufen Fliesen und
    /// Sockelleisten.
    fn lichte(&self, k: usize) -> Vec<(f64, f64, f64, f64)> {
        let d = self.putz[k];
        self.an(k)
            .map(|o| {
                if !laibung_behandelt(o.art) {
                    return (o.von, o.bis, o.uk, o.ok);
                }
                let (am_anfang, am_ende) = self.ueber_ecke(o);
                let a = if am_anfang { 0.0 } else { d };
                let e = if am_ende { 0.0 } else { d };
                (o.von + a, o.bis - e, o.uk, o.ok - d)
            })
            .collect()
    }

    fn wand_koerper(&self, k: usize, lagen: &Lagen, out: &mut Vec<Koerper>) {
        let Some(typ) = self.seite(k).wand else {
            return;
        };
        if !self.kanten[k].ausbau_typ.is_empty() {
            return; // Putz als Schicht des Wandtyps
        }
        let Some((j, m, raum)) = bekleidung(self.model, typ) else {
            return;
        };
        let d = self.putz[k];
        // bis UK Deckenputz; unter Öffnungen am Boden liegt der Streifen des
        // Bodens bis zur Putzoberfläche
        let zs = (
            self.form.z_boden,
            self.form.z_decke - ausbau::dicke(self.model, self.aus.decke),
        );
        let loecher: Vec<(f64, f64, f64, f64)> = self
            .an(k)
            .map(|o| {
                let uk = if self.am_boden(o) {
                    zs.0.min(o.uk)
                } else {
                    o.uk
                };
                (o.von, o.bis, uk, o.ok)
            })
            .collect();
        let mut b = Bau::default();
        for (s0, s1, za, zb) in felder(self.roh[k], zs, &loecher) {
            let q = self.streifen(
                k,
                (s0, s1),
                (&lagen.null, &lagen.e_roh),
                (&self.putz, &lagen.e_putz),
            );
            klotz(&mut b, q, (za, zb), [m, m, m, m, raum, m]);
        }
        // Laibungen bis zum Element: seitlich ab OK Fertigfußboden bzw.
        // Brüstung (an der offenen Ecke nicht) und oben; an der Ecköffnung
        // läuft die obere über das Eckfeld, die der Folgekante beginnt
        // hinter ihr
        for o in self.an(k).filter(|o| laibung_behandelt(o.art)) {
            let (am_anfang, am_ende) = self.ueber_ecke(o);
            let unten = if self.am_boden(o) {
                self.ffb
            } else {
                o.uk.max(zs.0)
            };
            let hoehe = (unten, o.ok.min(zs.1));
            let t = (-o.tiefe, d);
            if !am_anfang {
                let q = self.rechteck(k, (o.von, o.von + d), t);
                klotz(&mut b, q, hoehe, [m, m, m, raum, m, m]);
            }
            if !am_ende {
                let q = self.rechteck(k, (o.bis - d, o.bis), t);
                klotz(&mut b, q, hoehe, [m, m, m, m, m, raum]);
            }
            if o.ok < zs.1 - 1e-6 {
                let sa = if am_anfang {
                    o.von + self.putz[self.vorige(k)]
                } else {
                    o.von + d
                };
                let sb = if am_ende {
                    let n = self.naechste(k);
                    let q = self
                        .an(n)
                        .find(|q| q.oeffnung == o.oeffnung && q.von <= 0.5);
                    o.bis + q.map_or(0.0, |q| q.tiefe)
                } else {
                    o.bis - d
                };
                let q = self.rechteck(k, (sa, sb), t);
                klotz(&mut b, q, (o.ok - d, o.ok), [m, raum, m, m, m, m]);
            }
        }
        out.push(b.koerper(Flaeche::Wand(k), typ, j, m));
    }

    fn fliesen_koerper(&self, k: usize, lagen: &Lagen, out: &mut Vec<Koerper>) {
        let Some((typ, h)) = self.seite(k).fliesen else {
            return;
        };
        let Some((j, m, _)) = bekleidung(self.model, typ) else {
            return;
        };
        let oben = self.form.z_decke - ausbau::dicke(self.model, self.aus.decke);
        let zs = (self.ffb, (self.ffb + h).min(oben));
        let mut b = Bau::default();
        for (s0, s1, za, zb) in felder(self.roh[k], zs, &self.lichte(k)) {
            let q = self.streifen(
                k,
                (s0, s1),
                (&self.putz, &lagen.e_putz),
                (&lagen.fliese, &lagen.e_fliese),
            );
            klotz(&mut b, q, (za, zb), [m; 6]);
        }
        out.push(b.koerper(Flaeche::Fliesen(k), typ, j, m));
    }

    fn sockel_koerper(&self, lagen: &Lagen, out: &mut Vec<Koerper>) {
        let mut je_typ: Vec<(LayerSetId, Bau, u16)> = Vec::new();
        for k in 0..self.kanten.len() {
            let Some(typ) = self.seite(k).sockel else {
                continue;
            };
            let Some(t) = self.model.layer_set(typ) else {
                continue;
            };
            let Some(l) = t.layers.first() else {
                continue;
            };
            let m = material_key(l.material);
            let h = t.height.unwrap_or(60.0);
            let zs = (self.ffb, self.ffb + h);
            let i = match je_typ.iter().position(|x| x.0 == typ) {
                Some(i) => i,
                None => {
                    je_typ.push((typ, Bau::default(), m));
                    je_typ.len() - 1
                }
            };
            for (s0, s1, za, zb) in felder(self.roh[k], zs, &self.lichte(k)) {
                let q = self.streifen(
                    k,
                    (s0, s1),
                    (&lagen.fliese, &lagen.e_fliese),
                    (&lagen.sockel, &lagen.e_sockel),
                );
                klotz(&mut je_typ[i].1, q, (za, zb), [m; 6]);
            }
        }
        for (typ, b, m) in je_typ {
            out.push(b.koerper(Flaeche::Sockel, typ, 0, m));
        }
    }
}

/// Die Lagen vor den Wandseiten: Abstände zur Rohbaufläche je Kante und
/// die Enden auf Gehrung.
struct Lagen {
    null: Vec<f64>,
    e_roh: Vec<(Vec3, Vec3)>,
    e_putz: Vec<(Vec3, Vec3)>,
    fliese: Vec<f64>,
    e_fliese: Vec<(Vec3, Vec3)>,
    sockel: Vec<f64>,
    e_sockel: Vec<(Vec3, Vec3)>,
}

impl Lagen {
    fn new(r: &Raum) -> Lagen {
        let n = r.kanten.len();
        let null = vec![0.0; n];
        let dicke = |id: Option<LayerSetId>| ausbau::dicke(r.model, id);
        let fliese: Vec<f64> = (0..n)
            .map(|k| r.putz[k] + dicke(r.seite(k).fliesen.map(|f| f.0)))
            .collect();
        // eine Sockelleiste steht vor den Fliesen, wo beide sind
        let sockel: Vec<f64> = (0..n)
            .map(|k| fliese[k] + dicke(r.seite(k).sockel))
            .collect();
        Lagen {
            e_roh: r.enden(&null),
            e_putz: r.enden(&r.putz),
            e_fliese: r.enden(&fliese),
            e_sockel: r.enden(&sockel),
            null,
            fliese,
            sockel,
        }
    }
}

/// Die Körper des Ausbaus eines Raums: Estrich und Belag je Schicht, Decke,
/// Putz je Wandseite, Wandfliesen, Sockelleisten je Typ.
pub fn koerper(model: &Model, form: &Rohbau, aus: &RaumAusbau) -> Vec<Koerper> {
    let regeln = super::Regeln::default();
    let r = Raum::new(model, form, aus, &regeln);
    let lagen = Lagen::new(&r);
    let mut out = Vec::new();
    r.boden_koerper(&mut out);
    r.decke_koerper(&mut out);
    for k in 0..r.kanten.len() {
        r.wand_koerper(k, &lagen, &mut out);
        r.fliesen_koerper(k, &lagen, &mut out);
    }
    r.sockel_koerper(&lagen, &mut out);
    for k in &mut out {
        kontur(&mut k.solid);
    }
    out
}
