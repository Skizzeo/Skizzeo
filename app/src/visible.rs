//! Sichtbarkeit beim Zusammensetzen des Netzes (Paket 3 §3.2, §3.5a): ein
//! Körper wird je Dreieck und Kante in deckend, blass und ausgeblendet
//! geteilt, bevor gestapelte Züge verschmolzen werden (G1).
//!
//! Die Kantenart wird erst nach dem Filter bestimmt: Eine Kante ist Kontur,
//! wenn auf ihrer anderen Seite nichts Sichtbares derselben Klasse liegt
//! (G3). Wo ein Nachbar wegfällt, entstehen die Grenzkanten neu (H1): an der
//! Ecke einer ausgeblendeten Wand, am Stoß Sohlplatte/Frostschürze im
//! Schnitt, an einer ausgeblendeten Schicht.

use sk_math::{vec3, Vec3};
use sk_model::{edge_kind, material, Edge, Solid, Tri};
use std::collections::HashMap;

/// Wie ein Dreieck oder eine Kante gezeigt wird.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Class {
    Solid,
    Ghost,
    Hidden,
}

impl Class {
    fn slot(self) -> Option<usize> {
        match self {
            Class::Solid => Some(0),
            Class::Ghost => Some(1),
            Class::Hidden => None,
        }
    }
}

/// Lagetoleranz (mm) für deckungsgleiche Strecken.
const TOL: f64 = 1e-3;

/// Teilt `s` in deckend (`[0]`) und blass (`[1]`); `class` bekommt Teil und
/// Schicht eines Dreiecks bzw. einer Kante.
pub fn split(s: &Solid, class: impl Fn(u32, u8) -> Class) -> [Solid; 2] {
    let empty = || Solid {
        mat: s.mat,
        ..Solid::default()
    };
    let mut out = [empty(), empty()];
    let tc: Vec<Class> = s.triangles.iter().map(|t| class(t.elem, t.layer)).collect();
    let ec: Vec<Class> = s.edges.iter().map(|e| class(e.elem, e.layer)).collect();
    // Einheitlich: ganz in eine Klasse, nichts neu zu bestimmen
    let first = tc.first().or(ec.first()).copied();
    if let Some(c) = first {
        if tc.iter().chain(&ec).all(|x| *x == c) {
            if let Some(k) = c.slot() {
                out[k].triangles = s.triangles.clone();
                out[k].edges = s.edges.clone();
            }
            return out;
        }
    } else {
        return out;
    }
    out[0].triangles.reserve(s.triangles.len());
    out[0].edges.reserve(s.edges.len());
    for (t, c) in s.triangles.iter().zip(&tc) {
        if let Some(k) = c.slot() {
            out[k].triangles.push(*t);
        }
    }
    let kinds = Kinds::of(&s.edges);
    // Alle Dreiecksseiten nach ihrer Geraden (Review 3h Befund 2: ein
    // Verzeichnis statt eines je Klasse und Bereich)
    let tris =
        SegIndex::build(s.triangles.iter().enumerate().flat_map(|(i, t)| {
            (0..3).map(move |m| ((i * 3 + m) as u32, t.p[m], t.p[(m + 1) % 3]))
        }));
    // Liegt an der Strecke ein Dreieck einer anderen Klasse als `c`?
    let other = |a: Vec3, b: Vec3, c: Class, skip: Option<usize>| {
        tris.adjacent(&s.triangles, a, b, skip)
            .iter()
            .any(|&j| tc[j] != c)
    };
    // Erst die Strecken an einer Grenze sammeln (Fugen und Dreiecksseiten)
    let fine: Vec<usize> = (0..s.edges.len())
        .filter(|&i| {
            let e = &s.edges[i];
            e.kind == edge_kind::FINE && ec[i].slot().is_some() && other(e.a, e.b, ec[i], None)
        })
        .collect();
    let mut sides: [Vec<(usize, usize)>; 2] = [Vec::new(), Vec::new()];
    for (i, t) in s.triangles.iter().enumerate() {
        let Some(k) = tc[i].slot() else { continue };
        for m in 0..3 {
            let (a, b) = (t.p[m], t.p[(m + 1) % 3]);
            let c = tc[i];
            if tris.may_touch(i * 3 + m, |j| tc[j as usize / 3] != c) && other(a, b, c, Some(i)) {
                sides[k].push((i, m));
            }
        }
    }
    if fine.is_empty() && sides.iter().all(Vec::is_empty) {
        // keine Grenze: Kanten nur verteilen
        for (e, c) in s.edges.iter().zip(&ec) {
            if let Some(k) = c.slot() {
                out[k].edges.push(*e);
            }
        }
        return out;
    }
    // Vorhandene Kanten: eine Fuge ohne Nachbarn derselben Klasse wird Kontur
    let mut fine = fine.into_iter().peekable();
    for (i, (e, c)) in s.edges.iter().zip(&ec).enumerate() {
        let Some(k) = c.slot() else { continue };
        let mut e = *e;
        if fine.next_if_eq(&i).is_some() {
            let adj = tris.adjacent(&s.triangles, e.a, e.b, None);
            let own: Vec<usize> = adj.into_iter().filter(|&j| tc[j] == *c).collect();
            if !two_sided(&s.triangles, &own, e.a, e.b) {
                if let Some(&j) = own.first() {
                    e.kind = kinds.contour(&s.triangles[j]);
                }
            }
        }
        out[k].edges.push(e);
    }
    // Neue Grenzkanten, wo ein Nachbar in eine andere Klasse fiel
    for (k, sides) in sides.iter().enumerate() {
        if sides.is_empty() {
            continue;
        }
        let kept = SegIndex::build(
            out[k]
                .edges
                .iter()
                .enumerate()
                .map(|(i, e)| (i as u32, e.a, e.b)),
        );
        let mut new_edges = Vec::new();
        for &(i, m) in sides {
            let t = &s.triangles[i];
            let (a, b) = (t.p[m], t.p[(m + 1) % 3]);
            let adj = tris.adjacent(&s.triangles, a, b, Some(i));
            let own: Vec<usize> = adj.iter().copied().filter(|&j| tc[j] == tc[i]).collect();
            if continues(&s.triangles, t, &own, a, b) || kept.covers(&out[k].edges, a, b) {
                continue;
            }
            new_edges.push(Edge {
                a,
                b,
                kind: kinds.contour(t),
                elem: t.elem,
                layer: t.layer,
            });
        }
        out[k].edges.extend(new_edges);
    }
    out
}

/// Liegen die Strecken `a`–`b` und `c`–`d` auf einer Geraden und überlappen
/// sie sich (mehr als [`TOL`])?
fn overlap(a: Vec3, b: Vec3, c: Vec3, d: Vec3) -> f64 {
    let ab = b - a;
    let len = ab.length();
    if len < TOL {
        return 0.0;
    }
    let u = ab * (1.0 / len);
    let off = |p: Vec3| {
        let r = p - a;
        (r - u * r.dot(u)).length()
    };
    if off(c) > TOL || off(d) > TOL {
        return 0.0;
    }
    let (t0, t1) = ((c - a).dot(u), (d - a).dot(u));
    (t0.max(t1).min(len) - t0.min(t1).max(0.0)).max(0.0)
}

/// Seite der Geraden `a`–`b`, auf der das Dreieck liegt (bezogen auf die
/// Normale `n`).
fn side(t: &Tri, a: Vec3, b: Vec3, n: Vec3) -> f64 {
    let u = (b - a).normalized();
    let far =
        t.p.iter()
            .copied()
            .max_by(|p, q| {
                let d = |x: Vec3| (x - a - u * (x - a).dot(u)).length();
                d(*p).total_cmp(&d(*q))
            })
            .unwrap_or(a);
    (b - a).cross(far - a).dot(n)
}

/// Liegen unter `own` zwei Dreiecke in einer Ebene auf beiden Seiten der
/// Strecke (eine Fuge mitten in einer Fläche)?
fn two_sided(tris: &[Tri], own: &[usize], a: Vec3, b: Vec3) -> bool {
    own.iter().any(|&i| {
        let n = tris[i].n;
        let s = side(&tris[i], a, b, n);
        own.iter()
            .any(|&j| j != i && tris[j].n.dot(n) > 1.0 - 1e-6 && side(&tris[j], a, b, n) * s < 0.0)
    })
}

/// Setzt ein Dreieck aus `own` die Fläche von `t` über die Strecke fort?
fn continues(tris: &[Tri], t: &Tri, own: &[usize], a: Vec3, b: Vec3) -> bool {
    let s = side(t, a, b, t.n);
    own.iter()
        .any(|&j| tris[j].n.dot(t.n) > 1.0 - 1e-6 && side(&tris[j], a, b, t.n) * s < 0.0)
}

/// Konturarten je Teil und Schicht, aus den vorhandenen Kanten: Schnittkontur
/// (Schnittstift oder Schicht) und Sichtkante (auch Hintergrund).
struct Kinds {
    cut: HashMap<(u32, u8), u8>,
    view: HashMap<(u32, u8), u8>,
}

impl Kinds {
    fn of(edges: &[Edge]) -> Kinds {
        let mut cut = HashMap::new();
        let mut view = HashMap::new();
        for e in edges {
            let key = (e.elem, e.layer);
            match e.kind {
                edge_kind::CUT | edge_kind::CUT_LAYER => {
                    cut.entry(key).or_insert(e.kind);
                }
                edge_kind::FINE => {}
                k => {
                    view.entry(key).or_insert(k);
                }
            }
        }
        Kinds { cut, view }
    }

    /// Art einer neuen Kontur am Dreieck `t`.
    fn contour(&self, t: &Tri) -> u8 {
        let key = (t.elem, t.layer);
        if t.mat & material::CUT != 0 {
            self.cut.get(&key).copied().unwrap_or(edge_kind::CUT)
        } else {
            self.view.get(&key).copied().unwrap_or(edge_kind::VIEW)
        }
    }
}

/// Strecken (Dreiecksseiten oder Kanten) nach ihrer Geraden geordnet, für
/// die Suche nach deckungsgleichen Strecken: Eine Strecke steht unter dem
/// Schlüssel ihrer Richtung und ihres Lotfußpunkts (gerastert); wo sie nahe
/// an einer Rastergrenze liegt, auch unter dem Nachbarn. Eine Suche liest
/// dann genau einen Eimer statt aller Rasterzellen um die Strecke.
#[derive(Default)]
struct SegIndex {
    /// Strecken: Kennung und umschließender Quader (mit [`TOL`]).
    segs: Vec<(u32, Vec3, Vec3)>,
    /// Einträge: Schlüssel der Geraden (gestreut, ein Zusammenfallen
    /// zweier Schlüssel liest nur mehr), Stelle in `segs`, nächster Eintrag
    /// im selben Fach.
    lines: Vec<(u64, u32, u32)>,
    /// Fächer der Streutabelle: erster Eintrag oder [`NONE`].
    heads: Vec<u32>,
    /// Je Strecke der Schlüssel, unter dem eine Suche entlang ihrer eigenen
    /// Geraden liest (bei sehr kurzen ohne Bedeutung).
    own: Vec<u64>,
    /// Sehr kurze Strecken (Richtung unsicher): jede Suche prüft sie alle.
    short: Vec<u32>,
    /// Bezugspunkt der Lotfußpunkte (Mitte aller Strecken).
    center: Vec3,
    /// Rasterweite der Lotfußpunkte (mm).
    step: f64,
    /// Umschließender Quader aller Strecken (schneller Ausschluss).
    bounds: Option<(Vec3, Vec3)>,
}

/// Leeres Fach bzw. Ende einer Kette in [`SegIndex`].
const NONE: u32 = u32::MAX;

/// Kürzere Strecken stehen in [`SegIndex::short`] (mm).
const SHORT: f64 = 5.0;
/// Größte Richtungsabweichung deckungsgleicher Strecken (je Komponente des
/// Einheitsvektors): beide Enden höchstens [`TOL`] neben der Geraden.
const DIR_EPS: f64 = 2.0 * TOL / SHORT * 1.01 + 1e-9;
/// Rasterweite der Richtung.
const DIR_STEP: f64 = 0.02;

/// Richtung der Geraden durch `a`, `b`, Vorzeichen fest: die erste
/// Komponente über 0,5 (es gibt immer eine) ist positiv.
fn direction(a: Vec3, b: Vec3) -> Vec3 {
    let d = (b - a).normalized();
    let first = [d.x, d.y, d.z]
        .into_iter()
        .find(|c| c.abs() > 0.5)
        .unwrap_or(d.z);
    if first < 0.0 {
        d * -1.0
    } else {
        d
    }
}

/// Rasterstelle von `v`; 0 liegt mitten in einer Stelle (achsparallele
/// Richtungen sind häufig).
fn slot(v: f64, step: f64) -> i64 {
    let x = v / step + 0.5;
    let i = x as i64;
    // abrunden ohne `floor` (auf dem Grundziel ein Funktionsaufruf)
    i - (x < i as f64) as i64
}

/// Schlüssel der Geraden mit Richtung `d` und Lotfußpunkt `f`.
fn key(d: Vec3, f: Vec3, step: f64) -> u64 {
    mix([
        slot(d.x, DIR_STEP),
        slot(d.y, DIR_STEP),
        slot(d.z, DIR_STEP),
        slot(f.x, step),
        slot(f.y, step),
        slot(f.z, step),
    ])
}

/// Streut sechs Rasterstellen auf eine Zahl.
fn mix(k: [i64; 6]) -> u64 {
    k.iter().fold(0x9e37_79b9_7f4a_7c15, |h: u64, &x| {
        (h.rotate_left(5) ^ x as u64).wrapping_mul(0x51_7c_c1_b7_27_22_0a_95)
    })
}

/// Rasterstellen von `v` ± `eps` (eine oder zwei).
fn steps(v: f64, eps: f64, step: f64) -> std::ops::RangeInclusive<i64> {
    slot(v - eps, step)..=slot(v + eps, step)
}

impl SegIndex {
    fn build(segs: impl Iterator<Item = (u32, Vec3, Vec3)>) -> SegIndex {
        let segs: Vec<(u32, Vec3, Vec3)> = segs.collect();
        let mut index = SegIndex::default();
        let Some(bounds) = segs
            .iter()
            .map(|&(_, a, b)| (lo(a, b), hi(a, b)))
            .reduce(|x, y| union(Some(x), y))
        else {
            return index;
        };
        index.bounds = Some(bounds);
        let center = (bounds.0 + bounds.1) * 0.5;
        let r = (bounds.1 - bounds.0).length() * 0.5;
        // Lotfußpunkte deckungsgleicher Strecken liegen höchstens so weit
        // auseinander: TOL neben der Geraden, dazu die Richtungsabweichung
        // über den Abstand zur Mitte
        let pos_eps = TOL * 1.01 + 2.0 * DIR_EPS * r + 1e-6;
        index.center = center;
        index.step = (8.0 * pos_eps).max(50.0);
        index.own = vec![0; segs.len()];
        index.segs.reserve(segs.len());
        index.lines.reserve(segs.len() * 2);
        for (id, a, b) in segs {
            let at = index.segs.len() as u32;
            index.segs.push((id, lo(a, b), hi(a, b)));
            if (b - a).length() < SHORT {
                index.short.push(at);
                continue;
            }
            let d = direction(a, b);
            // Nahe der Grenze der Vorzeichenregel: beide Richtungen
            let both = [d.x, d.y, d.z]
                .iter()
                .any(|c| (c.abs() - 0.5).abs() <= DIR_EPS);
            let foot = index.foot(a, d);
            index.own[at as usize] = key(d, foot, index.step);
            let pos = [foot.x, foot.y, foot.z].map(|v| steps(v, pos_eps, index.step));
            for d in [d, d * -1.0].into_iter().take(if both { 2 } else { 1 }) {
                let dir = [d.x, d.y, d.z].map(|v| steps(v, DIR_EPS, DIR_STEP));
                for dx in dir[0].clone() {
                    for dy in dir[1].clone() {
                        for dz in dir[2].clone() {
                            for px in pos[0].clone() {
                                for py in pos[1].clone() {
                                    for pz in pos[2].clone() {
                                        index.lines.push((mix([dx, dy, dz, px, py, pz]), at, NONE));
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        // Streutabelle mit Ketten, höchstens halb voll
        let size = (2 * index.lines.len()).next_power_of_two().max(16);
        index.heads = vec![NONE; size];
        for i in 0..index.lines.len() {
            let f = (index.lines[i].0 as usize) & (size - 1);
            index.lines[i].2 = index.heads[f];
            index.heads[f] = i as u32;
        }
        index
    }

    /// Stellen in `segs` unter dem Schlüssel `k`.
    fn bucket(&self, k: u64) -> impl Iterator<Item = u32> + '_ {
        let mut i = if self.heads.is_empty() {
            NONE
        } else {
            self.heads[(k as usize) & (self.heads.len() - 1)]
        };
        std::iter::from_fn(move || {
            while i != NONE {
                let (x, at, next) = self.lines[i as usize];
                i = next;
                if x == k {
                    return Some(at);
                }
            }
            None
        })
    }

    /// Könnte an der Strecke Nummer `at` (in der Reihenfolge beim Bauen)
    /// eine Strecke liegen, für deren Kennung `pick` gilt? Vortest nur nach
    /// dem Eimer ihrer Geraden, ohne Lage.
    fn may_touch(&self, at: usize, pick: impl Fn(u32) -> bool) -> bool {
        // Eine sehr kurze Strecke hat keinen Eimer: immer suchen
        self.short.contains(&(at as u32))
            || self
                .bucket(self.own[at])
                .any(|k| pick(self.segs[k as usize].0))
            || self.short.iter().any(|&k| pick(self.segs[k as usize].0))
    }

    /// Lotfußpunkt der Geraden durch `a` mit Richtung `d`, von der Mitte aus.
    fn foot(&self, a: Vec3, d: Vec3) -> Vec3 {
        let p = a - self.center;
        p - d * p.dot(d)
    }

    /// Kennungen der Strecken, die auf der Geraden durch `a`–`b` liegen
    /// könnten und deren Quader den der Strecke berührt, aufsteigend und
    /// ohne Doppel.
    fn near(&self, a: Vec3, b: Vec3) -> Vec<u32> {
        let mut out = Vec::new();
        let (l, h) = (lo(a, b), hi(a, b));
        if !self.bounds.is_some_and(|b| overlaps((l, h), b)) || (b - a).length() < TOL {
            return out;
        }
        let d = direction(a, b);
        let hits = self.bucket(key(d, self.foot(a, d), self.step));
        out.extend(hits.chain(self.short.iter().copied()).filter_map(|k| {
            let (id, sl, sh) = self.segs[k as usize];
            overlaps((l, h), (sl, sh)).then_some(id)
        }));
        out.sort_unstable();
        out.dedup();
        out
    }
}

fn lo(a: Vec3, b: Vec3) -> Vec3 {
    vec3(a.x.min(b.x) - TOL, a.y.min(b.y) - TOL, a.z.min(b.z) - TOL)
}

fn hi(a: Vec3, b: Vec3) -> Vec3 {
    vec3(a.x.max(b.x) + TOL, a.y.max(b.y) + TOL, a.z.max(b.z) + TOL)
}

/// Vereinigung zweier Quader.
fn union(a: Option<(Vec3, Vec3)>, (l, h): (Vec3, Vec3)) -> (Vec3, Vec3) {
    match a {
        None => (l, h),
        Some((bl, bh)) => (
            vec3(bl.x.min(l.x), bl.y.min(l.y), bl.z.min(l.z)),
            vec3(bh.x.max(h.x), bh.y.max(h.y), bh.z.max(h.z)),
        ),
    }
}

/// Berühren oder schneiden sich zwei Quader?
fn overlaps((l, h): (Vec3, Vec3), (bl, bh): (Vec3, Vec3)) -> bool {
    l.x <= bh.x && h.x >= bl.x && l.y <= bh.y && h.y >= bl.y && l.z <= bh.z && h.z >= bl.z
}

impl SegIndex {
    /// Dreiecke mit einer Seite auf der Strecke `a`–`b` (ohne `skip`).
    fn adjacent(&self, tris: &[Tri], a: Vec3, b: Vec3, skip: Option<usize>) -> Vec<usize> {
        let mut out: Vec<usize> = self
            .near(a, b)
            .into_iter()
            .filter_map(|id| {
                let (i, m) = (id as usize / 3, id as usize % 3);
                let t = &tris[i];
                (Some(i) != skip && overlap(a, b, t.p[m], t.p[(m + 1) % 3]) > TOL).then_some(i)
            })
            .collect();
        out.dedup();
        out
    }

    /// Deckt eine Kante aus `edges` die Strecke `a`–`b` (fast) ganz?
    fn covers(&self, edges: &[Edge], a: Vec3, b: Vec3) -> bool {
        let len = (b - a).length();
        let sum: f64 = self
            .near(a, b)
            .into_iter()
            .map(|i| {
                let e = &edges[i as usize];
                overlap(a, b, e.a, e.b)
            })
            .sum();
        sum >= len - TOL
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Zwei Quader nebeneinander (Teil 0 und 1), Stoß bei x = 1000, ohne
    /// Kante am Stoß (gleicher Baustoff, wie Sohlplatte und Frostschürze).
    fn zwei_bloecke() -> Solid {
        let mut s = Solid::default();
        for (part, x0, x1) in [(0u32, 0.0, 1000.0), (1, 1000.0, 2000.0)] {
            s.elem = part;
            let p = |x: f64, y: f64, z: f64| vec3(x, y, z);
            let (y0, y1, z0, z1) = (0.0, 500.0, 0.0, 300.0);
            // Vorderseite (y = 0), Oberseite
            s.quad(
                p(x0, y0, z0),
                p(x1, y0, z0),
                p(x1, y0, z1),
                p(x0, y0, z1),
                vec3(0.0, -1.0, 0.0),
            );
            s.quad(
                p(x0, y0, z1),
                p(x1, y0, z1),
                p(x1, y1, z1),
                p(x0, y1, z1),
                vec3(0.0, 0.0, 1.0),
            );
            s.edge(p(x0, y0, z1), p(x1, y0, z1));
            s.edge(p(x0, y0, z0), p(x1, y0, z0));
        }
        // Außenkanten an den Enden
        s.elem = 0;
        s.edge(vec3(0.0, 0.0, 0.0), vec3(0.0, 0.0, 300.0));
        s.elem = 1;
        s.edge(vec3(2000.0, 0.0, 0.0), vec3(2000.0, 0.0, 300.0));
        s
    }

    #[test]
    fn einheitlich_bleibt_alles() {
        let s = zwei_bloecke();
        let [a, b] = split(&s, |_, _| Class::Solid);
        assert_eq!(a.triangles.len(), s.triangles.len());
        assert_eq!(a.edges.len(), s.edges.len());
        assert!(b.triangles.is_empty() && b.edges.is_empty());
    }

    #[test]
    fn ausgeblendeter_nachbar_gibt_grenzkante() {
        let s = zwei_bloecke();
        let [a, b] = split(&s, |p, _| if p == 0 { Class::Solid } else { Class::Hidden });
        assert!(b.triangles.is_empty());
        // senkrechte Kante am Stoß x = 1000 (Vorderseite) und auf der Oberseite
        let am_stoss = |e: &Edge| (e.a.x - 1000.0).abs() < 1e-6 && (e.b.x - 1000.0).abs() < 1e-6;
        let n = a.edges.iter().filter(|e| am_stoss(e)).count();
        assert_eq!(n, 2, "{:?}", a.edges);
        // keine Kanten des ausgeblendeten Teils
        assert!(a.edges.iter().all(|e| e.elem == 0));
    }

    #[test]
    fn blass_und_deckend_je_ihre_grenze() {
        let s = zwei_bloecke();
        let [a, b] = split(&s, |p, _| if p == 0 { Class::Solid } else { Class::Ghost });
        let am_stoss = |e: &&Edge| (e.a.x - 1000.0).abs() < 1e-6 && (e.b.x - 1000.0).abs() < 1e-6;
        assert_eq!(a.edges.iter().filter(am_stoss).count(), 2);
        assert_eq!(b.edges.iter().filter(am_stoss).count(), 2);
        assert_eq!(a.triangles.len() + b.triangles.len(), s.triangles.len());
    }

    /// Eine feine Kante fern vom ausgeblendeten Teil bleibt fein, wie ohne
    /// Ausblenden (Review 3h: nur an der Grenze ändert sich etwas).
    #[test]
    fn feine_kante_fern_der_grenze_bleibt() {
        let mut s = zwei_bloecke();
        s.elem = 0;
        s.edge_kind = edge_kind::FINE;
        s.edge(vec3(0.0, 0.0, 0.0), vec3(0.0, 0.0, 300.0));
        let [a, _] = split(&s, |p, _| if p == 0 { Class::Solid } else { Class::Hidden });
        let fein = a
            .edges
            .iter()
            .filter(|e| e.a.x.abs() < 1e-6 && e.b.x.abs() < 1e-6)
            .any(|e| e.kind == edge_kind::FINE);
        assert!(fein, "{:?}", a.edges);
    }

    #[test]
    fn fuge_wird_kontur_ohne_nachbarschicht() {
        // Zwei Schichten in einer Ebene (Schnittflächen), Fuge fein
        let mut s = Solid {
            mat: material::CUT | 1,
            ..Solid::default()
        };
        let n = vec3(0.0, -1.0, 0.0);
        s.layer = 0;
        s.quad(
            vec3(0.0, 0.0, 0.0),
            vec3(100.0, 0.0, 0.0),
            vec3(100.0, 0.0, 300.0),
            vec3(0.0, 0.0, 300.0),
            n,
        );
        s.edge_kind = edge_kind::CUT;
        s.edge(vec3(0.0, 0.0, 0.0), vec3(0.0, 0.0, 300.0));
        s.layer = 1;
        s.quad(
            vec3(100.0, 0.0, 0.0),
            vec3(300.0, 0.0, 0.0),
            vec3(300.0, 0.0, 300.0),
            vec3(100.0, 0.0, 300.0),
            n,
        );
        s.edge(vec3(300.0, 0.0, 0.0), vec3(300.0, 0.0, 300.0));
        s.edge_kind = edge_kind::FINE;
        s.edge(vec3(100.0, 0.0, 0.0), vec3(100.0, 0.0, 300.0));
        // beide sichtbar: Fuge bleibt fein
        let [a, _] = split(&s, |_, l| if l == 0 { Class::Solid } else { Class::Ghost });
        let fuge = |s: &Solid| {
            s.edges
                .iter()
                .filter(|e| (e.a.x - 100.0).abs() < 1e-6 && (e.b.x - 100.0).abs() < 1e-6)
                .map(|e| e.kind)
                .collect::<Vec<_>>()
        };
        assert_eq!(fuge(&a), [edge_kind::CUT], "neue Kontur an Schicht 0");
        let [a, _] = split(&s, |_, l| if l == 1 { Class::Solid } else { Class::Hidden });
        assert_eq!(fuge(&a), [edge_kind::CUT], "Fuge wird Schnittkontur");
        let [a, _] = split(&s, |_, _| Class::Solid);
        assert_eq!(fuge(&a), [edge_kind::FINE]);
    }

    /// Das Verzeichnis nach Geraden findet jede deckungsgleiche Strecke wie
    /// die Suche über alle: schräge Richtungen, Richtungen an der Grenze der
    /// Vorzeichenregel (Komponente 0,5) und des Rasters, achsparallele,
    /// Enden bis knapp [`TOL`] neben der Geraden, weit von der Mitte.
    #[test]
    fn verzeichnis_findet_wie_alle() {
        let mut seed = 0x2545_f491_4f6c_dd1du64;
        let mut rnd = move || {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            (seed >> 11) as f64 / (1u64 << 53) as f64
        };
        let mut dirs = vec![
            vec3(1.0, 0.0, 0.0),
            vec3(0.0, 1.0, 0.0),
            vec3(0.0, 0.0, 1.0),
            vec3(1.0, 1.0, 0.0),
            vec3(0.5, (0.75f64).sqrt(), 0.0),
            vec3(0.5 + 1e-5, (1.0 - (0.5f64 + 1e-5).powi(2)).sqrt(), 0.0),
            vec3(-0.5 + 1e-5, 0.0, (1.0 - (0.5f64 - 1e-5).powi(2)).sqrt()),
            vec3(0.01, 1.0, 0.0),
            vec3(0.02 * 1.5, 1.0, 0.0),
        ];
        for _ in 0..12 {
            dirs.push(vec3(rnd() - 0.5, rnd() - 0.5, rnd() - 0.5));
        }
        let mut segs = Vec::new();
        for d in dirs {
            let d = d.normalized();
            for _ in 0..3 {
                let base = vec3(rnd() - 0.5, rnd() - 0.5, rnd() - 0.5) * 40000.0;
                for _ in 0..4 {
                    let t0 = rnd() * 6000.0;
                    let len = [3.0, 8.0, 40.0, 900.0, 4000.0][(rnd() * 5.0) as usize];
                    let mut jit = || vec3(rnd() - 0.5, rnd() - 0.5, rnd() - 0.5) * (0.9 * TOL);
                    let a = base + d * t0 + jit();
                    let b = base + d * (t0 + len) + jit();
                    segs.push((segs.len() as u32, a, b));
                }
            }
        }
        let index = SegIndex::build(segs.iter().copied());
        for &(_, a, b) in &segs {
            let near: Vec<u32> = index
                .near(a, b)
                .into_iter()
                .filter(|&j| {
                    let (_, c, d) = segs[j as usize];
                    overlap(a, b, c, d) > TOL
                })
                .collect();
            let all: Vec<u32> = segs
                .iter()
                .filter(|&&(_, c, d)| overlap(a, b, c, d) > TOL)
                .map(|&(j, _, _)| j)
                .collect();
            assert_eq!(near, all, "Strecke {a:?}–{b:?}");
        }
    }
}
