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
/// Rasterweite des Streckenverzeichnisses (mm).
const CELL: f64 = 1000.0;

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
    for (t, c) in s.triangles.iter().zip(&tc) {
        if let Some(k) = c.slot() {
            out[k].triangles.push(*t);
        }
    }
    let kinds = Kinds::of(&s.edges);
    // Dreiecke außerhalb der Klasse deckend bzw. blass (meist wenige): nur wo
    // eins davon anliegt, ändert sich eine Kante (Review 3h)
    let others = [Class::Solid, Class::Ghost].map(|cls| {
        if !tc.contains(&cls) && !ec.contains(&cls) {
            return SegIndex::default();
        }
        SegIndex::build(
            s.triangles
                .iter()
                .enumerate()
                .filter(|(i, _)| tc[*i] != cls)
                .flat_map(|(i, t)| {
                    (0..3).map(move |m| ((i * 3 + m) as u32, t.p[m], t.p[(m + 1) % 3]))
                }),
        )
    });
    // Erst die Strecken an einer Grenze sammeln (Fugen und Dreiecksseiten),
    // dann die volle Nachbarschaft nur in ihrem Bereich aufbauen
    let mut region: Option<(Vec3, Vec3)> = None;
    let mut grow = |a: Vec3, b: Vec3| region = Some(union(region, (lo(a, b), hi(a, b))));
    let fine: Vec<usize> = (0..s.edges.len())
        .filter(|&i| {
            let e = &s.edges[i];
            let hit = e.kind == edge_kind::FINE
                && ec[i]
                    .slot()
                    .is_some_and(|k| !others[k].adjacent(&s.triangles, e.a, e.b, None).is_empty());
            if hit {
                grow(e.a, e.b);
            }
            hit
        })
        .collect();
    let mut sides: [Vec<(usize, usize)>; 2] = [Vec::new(), Vec::new()];
    for (i, t) in s.triangles.iter().enumerate() {
        let Some(k) = tc[i].slot() else { continue };
        for m in 0..3 {
            let (a, b) = (t.p[m], t.p[(m + 1) % 3]);
            if !others[k].adjacent(&s.triangles, a, b, Some(i)).is_empty() {
                grow(a, b);
                sides[k].push((i, m));
            }
        }
    }
    let Some(region) = region else {
        // keine Grenze: Kanten nur verteilen
        for (e, c) in s.edges.iter().zip(&ec) {
            if let Some(k) = c.slot() {
                out[k].edges.push(*e);
            }
        }
        return out;
    };
    let inside = |a: Vec3, b: Vec3| overlaps((lo(a, b), hi(a, b)), region);
    let tris = SegIndex::build(
        s.triangles
            .iter()
            .enumerate()
            .flat_map(|(i, t)| (0..3).map(move |m| ((i * 3 + m) as u32, t.p[m], t.p[(m + 1) % 3])))
            .filter(|(_, a, b)| inside(*a, *b)),
    );
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
        let kept = SegIndex::build(
            out[k]
                .edges
                .iter()
                .enumerate()
                .map(|(i, e)| (i as u32, e.a, e.b))
                .filter(|(_, a, b)| inside(*a, *b)),
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

/// Strecken im Raster (Dreiecksseiten oder Kanten), für die Suche nach
/// deckungsgleichen Strecken.
#[derive(Default)]
struct SegIndex {
    cells: HashMap<(i64, i64, i64), Vec<u32>>,
    /// Umschließender Quader aller Strecken (schneller Ausschluss).
    bounds: Option<(Vec3, Vec3)>,
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

fn cell(p: Vec3) -> (i64, i64, i64) {
    (
        (p.x / CELL).floor() as i64,
        (p.y / CELL).floor() as i64,
        (p.z / CELL).floor() as i64,
    )
}

impl SegIndex {
    fn build(segs: impl Iterator<Item = (u32, Vec3, Vec3)>) -> SegIndex {
        let mut cells: HashMap<(i64, i64, i64), Vec<u32>> = HashMap::new();
        let mut bounds: Option<(Vec3, Vec3)> = None;
        for (id, a, b) in segs {
            bounds = Some(union(bounds, (lo(a, b), hi(a, b))));
            let (lo, hi) = (cell(lo(a, b)), cell(hi(a, b)));
            for x in lo.0..=hi.0 {
                for y in lo.1..=hi.1 {
                    for z in lo.2..=hi.2 {
                        cells.entry((x, y, z)).or_default().push(id);
                    }
                }
            }
        }
        SegIndex { cells, bounds }
    }

    /// Kennungen in den Zellen um die Strecke, ohne Doppel.
    fn near(&self, a: Vec3, b: Vec3) -> Vec<u32> {
        let mut out = Vec::new();
        let (l, h) = (lo(a, b), hi(a, b));
        if !self.bounds.is_some_and(|b| overlaps((l, h), b)) {
            return out;
        }
        let (lo, hi) = (cell(l), cell(h));
        for x in lo.0..=hi.0 {
            for y in lo.1..=hi.1 {
                for z in lo.2..=hi.2 {
                    if let Some(v) = self.cells.get(&(x, y, z)) {
                        out.extend_from_slice(v);
                    }
                }
            }
        }
        out.sort_unstable();
        out.dedup();
        out
    }

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
}
