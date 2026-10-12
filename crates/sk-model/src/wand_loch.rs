//! Löcher der Öffnungen in einem Wandzug (Paket O2, Jörn 11.10.): Die
//! äußerste Schicht bekommt die von außen sichtbare Öffnung, alle Schichten
//! dahinter die um den Anschlag größere (zwei gestaffelte Öffnungen, Jörn
//! 14:15). Der Körper entsteht wie ohne Loch; danach schneidet
//! [`WallChain::punch`] die Löcher heraus und setzt die Laibungen ein, damit
//! man bei schrägem Blick die Stirnseiten der Schichten sieht (Jörn 14:16).
//!
//! Im Grundriss fehlt im Loch die Schnittfläche; von oben sieht man auf die
//! Brüstung mit nur Außen- und Innenkante, ohne Fugen zwischen den Schichten
//! (Jörn 14:12).

use crate::solid::{edge_kind, Edge, Solid, Tri};
use crate::wall::{flat, WallChain};
use sk_math::{vec3, Vec3};

/// Loch einer Öffnung in einem Segment eines Wandzugs.
#[derive(Clone, Debug, PartialEq)]
pub struct Hole {
    pub seg: usize,
    /// Lage längs der Bezugslinie ab dem Segmentanfang (mm), Anfang und
    /// Ende; unendlich: die Öffnung läuft über das Segmentende hinaus (Ecke,
    /// Fuge), dort fällt die Wand bis zum Ende weg.
    pub u: (f64, f64),
    /// UK und OK der sichtbaren Öffnung (z absolut, mm).
    pub z: (f64, f64),
    /// Anschlag am Anfang und am Ende längs (`u`) sowie unten und oben (mm):
    /// so viel größer ist das Loch in den Schichten hinter der äußersten.
    pub reveal_u: (f64, f64),
    pub reveal_z: (f64, f64),
    /// Schichten von außen, die die sichtbare Öffnung bekommen (Verblender
    /// bis vor die Luftschicht, WDVS bis vor den Kern); die übrigen bekommen
    /// die um den Anschlag größere.
    pub shell: usize,
    /// Läuft `u` von außen gesehen nach rechts? Dann ist der Anfang links.
    pub u_right: bool,
    /// Die Öffnung, aus der das Loch stammt (Mengen, VOB-Schwelle).
    pub opening: Option<crate::ElementId>,
    /// Einbautiefen des Elements in dieser Wand (Laibungen, Raumbuch).
    pub install: crate::oeffnung::InstallDepth,
    /// In einer Außenwand oder Aufkantung; sonst in einer Innenwand
    /// (Innentür: ohne Einbauelement im Grundriss).
    pub outer: bool,
    /// Türblatt, wenn die Öffnung einen Aufschlag hat.
    pub leaf: Option<Leaf>,
}

/// Türblatt einer Öffnung im Wandzug (Jörn 12.10. 05:50): Lage der
/// Blattlinie quer zur Bezugslinie (Rohbauflucht der Aufschlagseite,
/// positiv rechts der Zeichenrichtung), Seite des Aufschlagraums und
/// Pfosten der Bänder.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Leaf {
    pub off: f64,
    pub side: crate::oeffnung::FaceSide,
    /// Bänder am kleineren `u` des Lochs.
    pub hinge_lo: bool,
}

/// Bereich eines Lochs in einer Schicht: längs (u) und Höhe (z).
type Kasten = ((f64, f64), (f64, f64));

impl Hole {
    /// Bereich in der äußersten Schicht (`aussen`) oder dahinter.
    pub fn kasten(&self, aussen: bool) -> Kasten {
        if aussen {
            return (self.u, self.z);
        }
        (
            (self.u.0 - self.reveal_u.0, self.u.1 + self.reveal_u.1),
            (self.z.0 - self.reveal_z.0, self.z.1 + self.reveal_z.1),
        )
    }
}

/// Toleranz für Lage und Höhe (mm).
const EPS: f64 = 1e-6;

/// Eckpunkt eines Vielecks beim Zuschneiden: Lage und Musterkoordinaten.
#[derive(Clone, Copy)]
struct Ecke {
    p: Vec3,
    uv: [f64; 2],
}

/// Halbebene `a·u + b·z + c ≥ 0` in Segmentkoordinaten.
type Halb = (f64, f64, f64);

/// Die vier Gebiete außerhalb eines Kastens (unter, über, davor, dahinter),
/// je als Schnitt von Halbebenen; leere bei unendlichen Grenzen fallen weg.
fn gebiete(((u0, u1), (z0, z1)): Kasten) -> Vec<Vec<Halb>> {
    let mut g = Vec::with_capacity(4);
    if z0.is_finite() {
        g.push(vec![(0.0, -1.0, z0)]);
    }
    if z1.is_finite() {
        g.push(vec![(0.0, 1.0, -z1)]);
    }
    // Seitlich: im Höhenband des Kastens
    let mut band = Vec::with_capacity(3);
    if z0.is_finite() {
        band.push((0.0, 1.0, -z0));
    }
    if z1.is_finite() {
        band.push((0.0, -1.0, z1));
    }
    if u0.is_finite() {
        g.push([band.as_slice(), &[(-1.0, 0.0, u0)]].concat());
    }
    if u1.is_finite() {
        g.push([band.as_slice(), &[(1.0, 0.0, -u1)]].concat());
    }
    g
}

/// Feste Reihenfolge zweier Punkte (x, dann y, dann z).
fn vor(a: Vec3, b: Vec3) -> bool {
    (a.x, a.y, a.z) < (b.x, b.y, b.z)
}

/// Vieleck an der Halbebene `h` zuschneiden (Sutherland–Hodgman); `uz`
/// liefert die Segmentkoordinaten eines Punkts.
fn schneiden(poly: &[Ecke], h: Halb, uz: &impl Fn(Vec3) -> (f64, f64)) -> Vec<Ecke> {
    let f = |e: &Ecke| {
        let (u, z) = uz(e.p);
        h.0 * u + h.1 * z + h.2
    };
    let mut out = Vec::with_capacity(poly.len() + 2);
    for k in 0..poly.len() {
        let (a, b) = (poly[k], poly[(k + 1) % poly.len()]);
        let (fa, fb) = (f(&a), f(&b));
        if fa >= 0.0 {
            out.push(a);
        }
        if (fa >= 0.0) != (fb >= 0.0) {
            // Immer von derselben Ecke aus rechnen: die Nachbardreiecke
            // laufen die gemeinsame Kante gegenläufig ab und müssen genau
            // denselben Punkt treffen (sonst ein Spalt im Netz, Test #79)
            let ((p, fp), (q, fq)) = if vor(a.p, b.p) {
                ((a, fa), (b, fb))
            } else {
                ((b, fb), (a, fa))
            };
            let t = fp / (fp - fq);
            out.push(Ecke {
                p: p.p + (q.p - p.p) * t,
                uv: [
                    p.uv[0] + (q.uv[0] - p.uv[0]) * t,
                    p.uv[1] + (q.uv[1] - p.uv[1]) * t,
                ],
            });
        }
    }
    out
}

/// Berührt das Rechteck um die Punkte das Innere des Kastens nicht?
fn neben(p: &[(f64, f64)], ((u0, u1), (z0, z1)): Kasten) -> bool {
    let (mut a, mut b) = ((f64::MAX, f64::MAX), (f64::MIN, f64::MIN));
    for q in p {
        (a.0, a.1, b.0, b.1) = (a.0.min(q.0), a.1.min(q.1), b.0.max(q.0), b.1.max(q.1));
    }
    b.0 <= u0 + 1e-3 || a.0 >= u1 - 1e-3 || b.1 <= z0 + 1e-3 || a.1 >= z1 - 1e-3
}

/// Liegt der Punkt (u, z) mit Toleranz ganz in einem Gebiet außerhalb?
fn aussen_in(g: &[Halb], (u, z): (f64, f64)) -> bool {
    g.iter().all(|h| h.0 * u + h.1 * z + h.2 >= -1e-3)
}

/// Dreieck `t` ohne den Kasten `k`: die Stücke außerhalb, als Dreiecke.
fn dreieck_ohne(t: Tri, k: Kasten, uz: &impl Fn(Vec3) -> (f64, f64)) -> Vec<Tri> {
    let gs = gebiete(k);
    let p: Vec<(f64, f64)> = t.p.iter().map(|q| uz(*q)).collect();
    // Neben dem Kasten oder ganz in einem Gebiet (auch auf seinem Rand):
    // bleibt, wie es ist
    if neben(&p, k) || gs.iter().any(|g| p.iter().all(|q| aussen_in(g, *q))) {
        return vec![t];
    }
    let start: Vec<Ecke> = (0..3)
        .map(|i| Ecke {
            p: t.p[i],
            uv: t.uv[i],
        })
        .collect();
    let mut out = Vec::new();
    for g in &gs {
        let mut poly = start.clone();
        for h in g {
            poly = schneiden(&poly, *h, uz);
            if poly.len() < 3 {
                break;
            }
        }
        for k in 1..poly.len().saturating_sub(1) {
            let (a, b, c) = (poly[0], poly[k], poly[k + 1]);
            if (b.p - a.p).cross(c.p - a.p).length() < 1e-6 {
                continue;
            }
            out.push(Tri {
                p: [a.p, b.p, c.p],
                uv: [a.uv, b.uv, c.uv],
                ..t
            });
        }
    }
    out
}

/// Kante `e` ohne den Kasten `k`: die Stücke außerhalb.
fn kante_ohne(e: Edge, k: Kasten, uz: &impl Fn(Vec3) -> (f64, f64)) -> Vec<Edge> {
    let gs = gebiete(k);
    let (pa, pb) = (uz(e.a), uz(e.b));
    if neben(&[pa, pb], k) || gs.iter().any(|g| aussen_in(g, pa) && aussen_in(g, pb)) {
        return vec![e];
    }
    let mut out = Vec::new();
    for g in &gs {
        let (mut t0, mut t1) = (0.0f64, 1.0f64);
        for h in g {
            let fa = h.0 * pa.0 + h.1 * pa.1 + h.2;
            let fb = h.0 * pb.0 + h.1 * pb.1 + h.2;
            if fa < 0.0 && fb < 0.0 {
                t1 = -1.0;
                break;
            }
            if (fa < 0.0) != (fb < 0.0) {
                let t = fa / (fa - fb);
                if fa < 0.0 {
                    t0 = t0.max(t);
                } else {
                    t1 = t1.min(t);
                }
            }
        }
        if t1 - t0 > 1e-9 {
            let d = e.b - e.a;
            out.push(Edge {
                a: e.a + d * t0,
                b: e.a + d * t1,
                ..e
            });
        }
    }
    out
}

/// Vieleck (waagerecht) an `u ∈ [u0, u1]` zuschneiden.
fn streifen(poly: &[Vec3], (u0, u1): (f64, f64), uz: &impl Fn(Vec3) -> (f64, f64)) -> Vec<Vec3> {
    let mut p: Vec<Ecke> = poly.iter().map(|&p| Ecke { p, uv: [0.0; 2] }).collect();
    if u0.is_finite() {
        p = schneiden(&p, (1.0, 0.0, -u0), uz);
    }
    if u1.is_finite() && p.len() >= 3 {
        p = schneiden(&p, (-1.0, 0.0, u1), uz);
    }
    p.into_iter().map(|e| e.p).collect()
}

/// Schnittstrecke des Vielecks (waagerecht, konvex) mit der Linie `u = at`.
fn quer(poly: &[Vec3], at: f64, uz: &impl Fn(Vec3) -> (f64, f64)) -> Option<(Vec3, Vec3)> {
    let mut hits: Vec<Vec3> = Vec::new();
    for k in 0..poly.len() {
        let (a, b) = (poly[k], poly[(k + 1) % poly.len()]);
        let (ua, ub) = (uz(a).0 - at, uz(b).0 - at);
        if (ua < 0.0) != (ub < 0.0) {
            hits.push(a + (b - a) * (ua / (ua - ub)));
        } else if ua == 0.0 {
            hits.push(a);
        }
    }
    let (a, b) = (*hits.first()?, *hits.last()?);
    ((b - a).length() > 1e-3).then_some((a, b))
}

impl WallChain {
    /// Bereich des Lochs `h` in Schicht `li`: die Schichten der Außenschale
    /// bekommen die sichtbare Öffnung, die übrigen die um den Anschlag
    /// größere.
    pub(crate) fn loch_in(&self, h: &Hole, li: usize) -> Kasten {
        h.kasten(li < h.shell)
    }

    /// Löcher des Segments `seg` in Schicht `li`.
    fn loecher(&self, seg: usize, li: usize) -> Vec<Kasten> {
        self.joints
            .holes
            .iter()
            .filter(|h| h.seg == seg)
            .map(|h| self.loch_in(h, li))
            .collect()
    }

    /// Ist die Fläche `off` von Schicht `li` an der Lochkante sichtbar? Auf
    /// der Wandkontur, an Luft oder wo die Nachbarschicht anders weit
    /// ausgeschnitten ist (Anschlag): ja; sonst ist es eine Fuge.
    fn sichtbar(&self, li: usize, off: f64, seite: impl Fn(Kasten) -> f64, h: &Hole) -> bool {
        let offs = self.layer_offsets();
        let nachbar = offs
            .iter()
            .enumerate()
            .find(|(j, o)| *j != li && ((o.0 - off).abs() < EPS || (o.1 - off).abs() < EPS))
            .map(|(j, _)| j);
        match nachbar {
            None => true,
            Some(j) if self.layers[j].air => true,
            Some(j) => (seite(self.loch_in(h, j)) - seite(self.loch_in(h, li))).abs() > EPS,
        }
    }

    /// Schneidet die Löcher aus dem Körper `s` (Dreiecke und Kanten, je
    /// Segment und Schicht) und setzt Laibungen, Brüstung und Sturz ein;
    /// `top`: Schnitthöhe des Grundrisses (unendlich in 3D und Ansicht).
    pub(crate) fn punch(&self, s: &mut Solid, top: f64) {
        if self.joints.holes.is_empty() {
            return;
        }
        let Some((pts, _, dirs)) = self.layout() else {
            return;
        };
        let uz_of = |i: usize| {
            let (p0, d) = (pts[i], dirs[i]);
            move |p: Vec3| ((flat(p) - p0).dot(d), p.z)
        };
        // Höhe jeder Schicht; reicht ein Loch bis an ihre Unter- oder
        // Oberkante (Tür ab Unterkante Wand) oder an den Schnitt, ist es dort
        // offen, damit keine Boden-, Deck- oder Schnittfläche durchs Loch
        // läuft (Review #79)
        let hoehen: Vec<(f64, f64)> = (0..self.layers.len())
            .map(|li| {
                let p = self.layer_parts(li);
                (
                    p.iter().map(|x| x.0).fold(f64::MAX, f64::min),
                    p.iter().map(|x| x.1).fold(f64::MIN, f64::max).min(top),
                )
            })
            .collect();
        let kaesten = |elem: u32, layer: u8| -> Vec<Kasten> {
            let (i, li) = (elem as usize, layer as usize);
            if i >= dirs.len() || li >= self.layers.len() {
                return Vec::new();
            }
            let (unten, oben) = hoehen[li];
            self.loecher(i, li)
                .into_iter()
                .map(|(u, (z0, z1))| {
                    (
                        u,
                        (
                            if z0 <= unten + 1e-3 {
                                f64::NEG_INFINITY
                            } else {
                                z0
                            },
                            if z1 >= oben - 1e-3 { f64::INFINITY } else { z1 },
                        ),
                    )
                })
                .collect()
        };
        let mut tris = Vec::with_capacity(s.triangles.len());
        for t in std::mem::take(&mut s.triangles) {
            let ks = kaesten(t.elem, t.layer);
            let uz = uz_of((t.elem as usize).min(dirs.len().saturating_sub(1)));
            let mut stuecke = vec![t];
            for k in ks {
                stuecke = stuecke
                    .into_iter()
                    .flat_map(|t| dreieck_ohne(t, k, &uz))
                    .collect();
            }
            tris.extend(stuecke);
        }
        s.triangles = tris;
        let mut kanten = Vec::with_capacity(s.edges.len());
        for e in std::mem::take(&mut s.edges) {
            let ks = kaesten(e.elem, e.layer);
            let uz = uz_of((e.elem as usize).min(dirs.len().saturating_sub(1)));
            let mut stuecke = vec![e];
            for k in ks {
                stuecke = stuecke
                    .into_iter()
                    .flat_map(|e| kante_ohne(e, k, &uz))
                    .collect();
            }
            kanten.extend(stuecke);
        }
        s.edges = kanten;
        self.laibungen(s, top, &pts, &dirs);
    }

    /// Laibungen, Brüstung, Sturz und ihre Kanten je Loch, Schicht und
    /// Abschnitt der Schicht.
    fn laibungen(&self, s: &mut Solid, top: f64, pts: &[Vec3], dirs: &[Vec3]) {
        let offs = self.layer_offsets();
        let ext = self.overhang_chain();
        let (clo, chi) = (
            offs.iter().map(|x| x.0).fold(f64::MAX, f64::min),
            offs.iter().map(|x| x.1).fold(f64::MIN, f64::max),
        );
        let (keep_mat, keep_kind, keep_elem, keep_layer) = (s.mat, s.edge_kind, s.elem, s.layer);
        for h in &self.joints.holes {
            let i = h.seg;
            let Some(&d) = dirs.get(i) else {
                continue;
            };
            let p0 = pts[i];
            let uz = |p: Vec3| ((flat(p) - p0).dot(d), p.z);
            for (li, l) in self.layers.iter().enumerate() {
                if l.air {
                    continue;
                }
                let (lo, hi, mat) = offs[li];
                let ((u0, u1), (z0, z1)) = self.loch_in(h, li);
                for (pz0, pz1, e) in self.layer_parts(li) {
                    let chain = if e {
                        ext.as_ref().unwrap_or(self)
                    } else {
                        self
                    };
                    let (ca, cb) = (
                        chain.face_corners_in(lo, Some(li)),
                        chain.face_corners_in(hi, Some(li)),
                    );
                    let j = (i + 1) % ca.len().max(1);
                    let (Some(&a0), Some(&a1), Some(&b0), Some(&b1)) =
                        (ca.get(i), ca.get(j), cb.get(i), cb.get(j))
                    else {
                        continue;
                    };
                    let fuss = [flat(a0), flat(a1), flat(b1), flat(b0)];
                    let oben = pz1.min(top);
                    let (zlo, zhi) = (pz0.max(z0), oben.min(z1));
                    s.elem = i as u32;
                    s.layer = li as u8;
                    // Laibungen am Anfang (Fläche zeigt ins Loch, +u) und Ende
                    if zhi > zlo + EPS {
                        for (at, n) in [(u0, d), (u1, -d)] {
                            if !at.is_finite() {
                                continue;
                            }
                            let Some((a, b)) = quer(&fuss, at, &uz) else {
                                continue;
                            };
                            let (a, b) = if (b - a).cross(n).z > 0.0 {
                                (b, a)
                            } else {
                                (a, b)
                            };
                            let (lo_z, hi_z) = (vec3(0.0, 0.0, zlo), vec3(0.0, 0.0, zhi));
                            s.mat = mat;
                            s.quad(a + lo_z, b + lo_z, b + hi_z, a + hi_z, n);
                            let seite =
                                move |k: Kasten| if n.dot(d) > 0.0 { k.0 .0 } else { k.0 .1 };
                            for p in [a, b] {
                                let off = (flat(p) - p0).dot(crate::wall::right_of(d));
                                let face = if (off - lo).abs() < (off - hi).abs() {
                                    lo
                                } else {
                                    hi
                                };
                                let kontur = (face - clo).abs() < EPS || (face - chi).abs() < EPS;
                                s.edge_kind = if kontur || self.sichtbar(li, face, seite, h) {
                                    edge_kind::VIEW
                                } else {
                                    edge_kind::FINE
                                };
                                s.edge(p + lo_z, p + hi_z);
                            }
                            // Oben am Schnitt: Schnittkontur quer durch die Schicht
                            if (zhi - top).abs() < EPS && top < pz1 - EPS {
                                s.edge_kind = l.cut_kind();
                                s.edge(a + hi_z, b + hi_z);
                            }
                        }
                    }
                    // Brüstung (oben, nach oben) und Sturz (unten, nach unten)
                    for (z, up) in [(z0, true), (z1, false)] {
                        if !(z > pz0 + EPS && z < oben - EPS) {
                            continue;
                        }
                        let poly = streifen(&fuss, (u0, u1), &uz);
                        if poly.len() < 3 {
                            continue;
                        }
                        s.mat = mat;
                        s.cap(&poly, z, up);
                        // Nur Außen- und Innenkante und die Enden, keine Fugen
                        let n = poly.len();
                        for q in 0..n {
                            let (a, b) = (poly[q], poly[(q + 1) % n]);
                            let off = |p: Vec3| (flat(p) - p0).dot(crate::wall::right_of(d));
                            let (oa, ob) = (off(a), off(b));
                            let laengs = (oa - ob).abs() < 1e-3;
                            let ende = (uz(a).0 - uz(b).0).abs() < 1e-3;
                            let kontur =
                                laengs && ((oa - clo).abs() < 1e-3 || (oa - chi).abs() < 1e-3);
                            if kontur || (ende && !laengs) {
                                s.edge_kind = edge_kind::VIEW;
                                s.edge(vec3(a.x, a.y, z), vec3(b.x, b.y, z));
                            }
                        }
                    }
                }
            }
        }
        (s.mat, s.edge_kind, s.elem, s.layer) = (keep_mat, keep_kind, keep_elem, keep_layer);
    }

    /// Höhenbänder eines Schnitts durch Schicht `li` von Segment `seg`
    /// zwischen `z0` und `z1`, ohne die Löcher, die die Schnittlinie an der
    /// Lage `u` (längs) trifft.
    pub(crate) fn baender(
        &self,
        seg: usize,
        li: usize,
        u: f64,
        (z0, z1): (f64, f64),
    ) -> Vec<(f64, f64)> {
        let mut out = vec![(z0, z1)];
        for ((u0, u1), (hz0, hz1)) in self.loecher(seg, li) {
            if !(u > u0 + EPS && u < u1 - EPS) {
                continue;
            }
            out = out
                .into_iter()
                .flat_map(|(a, b)| {
                    let mut v = Vec::new();
                    if hz0 > a + EPS {
                        v.push((a, hz0.min(b)));
                    }
                    if hz1 < b - EPS {
                        v.push((hz1.max(a), b));
                    }
                    v.into_iter().filter(|(x, y)| y > x)
                })
                .collect();
        }
        out
    }
}

impl WallChain {
    /// Einbauelement im Grundriss (Jörn 12.10. 05:24): je Loch, das die
    /// Schnitthöhe `cut` trifft, zwei Linien längs der Wand in Höhe des
    /// Schnitts, an der Außen- und Innenseite des späteren Elements (äußere
    /// Einbautiefe und Elementtiefe aus [`Hole::install`]), so breit wie das
    /// Loch in der Schicht, in der das Element sitzt. Zur Ecke hin offene
    /// Löcher enden an der Ecke der Linie, so laufen sie um die Ecke.
    pub fn element_edges(&self, cut: f64) -> Vec<Edge> {
        let mut out = Vec::new();
        let Some((pts, _, dirs)) = self.layout() else {
            return out;
        };
        let (o, inner) = (self.outer_offset(), self.inner_offset());
        let sign = (o - inner).signum();
        for h in &self.joints.holes {
            if !(h.outer && h.z.0 < cut && cut < h.z.1) {
                continue;
            }
            let (Some(&p0), Some(&d)) = (pts.get(h.seg), dirs.get(h.seg)) else {
                continue;
            };
            let nr = crate::wall::right_of(d);
            let i = &h.install;
            let mitte = o - sign * (i.outer + i.element * 0.5);
            let li = self.layer_at(mitte).unwrap_or(0);
            let ((u0, u1), _) = self.loch_in(h, li);
            for tiefe in [i.outer, i.outer + i.element] {
                let off = o - sign * tiefe;
                let ecken = self.face_corners(off);
                let punkt = |u: f64, k: usize| {
                    if u.is_finite() {
                        let p = p0 + d * u + nr * off;
                        Some(vec3(p.x, p.y, cut))
                    } else {
                        ecken
                            .get(k % ecken.len().max(1))
                            .map(|p| vec3(p.x, p.y, cut))
                    }
                };
                let (Some(a), Some(b)) = (punkt(u0, h.seg), punkt(u1, h.seg + 1)) else {
                    continue;
                };
                if (b - a).length() > EPS {
                    out.push(Edge {
                        a,
                        b,
                        kind: edge_kind::ELEMENT,
                        elem: h.seg as u32,
                        layer: li as u8,
                    });
                }
            }
        }
        out
    }

    /// Aufschlag der Türen im Grundriss in Höhe `cut` (Jörn 05:50): das
    /// offene Blatt senkrecht vom Bandpfosten in den Aufschlagraum, so lang
    /// wie die Lichte, und der Viertelkreis bis zum Gegenpfosten, dünn.
    pub fn swing_edges(&self, cut: f64) -> Vec<Edge> {
        let mut out = Vec::new();
        let Some((pts, _, dirs)) = self.layout() else {
            return out;
        };
        for h in &self.joints.holes {
            let Some(l) = h.leaf else {
                continue;
            };
            if !(h.z.0 < cut && cut < h.z.1 && h.u.0.is_finite() && h.u.1.is_finite()) {
                continue;
            }
            let (Some(&p0), Some(&d)) = (pts.get(h.seg), dirs.get(h.seg)) else {
                continue;
            };
            let at = |u: f64| {
                let p = p0 + d * u + crate::wall::right_of(d) * l.off;
                vec3(p.x, p.y, cut)
            };
            let (uh, uo) = if l.hinge_lo { h.u } else { (h.u.1, h.u.0) };
            let w = (uo - uh).abs();
            let (band, offen, zu) = (at(uh), l.side.normal(d) * w, d * (uo - uh));
            let mut punkte = vec![band + offen];
            for k in 1..=BOGEN {
                let t = std::f64::consts::FRAC_PI_2 * k as f64 / BOGEN as f64;
                punkte.push(band + offen * t.cos() + zu * t.sin());
            }
            let mut kante = |a: Vec3, b: Vec3| {
                out.push(Edge {
                    a,
                    b,
                    kind: edge_kind::FINE,
                    elem: h.seg as u32,
                    layer: 0,
                });
            };
            kante(band, punkte[0]);
            for p in punkte.windows(2) {
                kante(p[0], p[1]);
            }
        }
        out
    }
}

/// Stücke des Viertelkreises im Türaufschlag.
const BOGEN: usize = 12;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::solid::edge_kind;

    fn uz(p: Vec3) -> (f64, f64) {
        (p.x, p.z)
    }

    fn flaeche(ts: &[Tri]) -> f64 {
        ts.iter()
            .map(|t| (t.p[1] - t.p[0]).cross(t.p[2] - t.p[0]).length() * 0.5)
            .sum()
    }

    /// Zwei Dreiecke mit gemeinsamer Kante, gegenläufig abgelaufen: der
    /// Schnittpunkt auf der Kante ist bitgleich, sonst klafft das Netz (Tür
    /// am Wandfuß in r7, Test #79).
    #[test]
    fn schnitt_auf_gemeinsamer_kante_gleich() {
        let e = |x: f64, z: f64| Ecke {
            p: vec3(x, 8000.0, z),
            uv: [0.0; 2],
        };
        let (a, b) = (e(13_017.3, 0.0), e(21_982.486_865_148_86, 2135.0));
        let h = (-1.0, 0.0, 18_010.0);
        let eins = schneiden(&[a, b, e(21_982.486_865_148_86, 0.0)], h, &uz);
        let zwei = schneiden(&[b, a, e(13_017.3, 2135.0)], h, &uz);
        let auf = |v: &[Ecke]| -> Vec3 {
            v.iter()
                .map(|x| x.p)
                .find(|p| p.x == 18_010.0 && p.z > 0.0 && p.z < 2135.0)
                .unwrap()
        };
        let (p, q) = (auf(&eins), auf(&zwei));
        assert!(p.z.to_bits() == q.z.to_bits(), "{p:?} {q:?}");
    }

    /// Senkrechte Wandfläche 4 × 3 m mit Loch 1 × 1,5 m: 12 − 1,5 m² bleiben,
    /// ohne Stück im Loch.
    #[test]
    fn rechteck_ohne_loch() {
        let mut s = Solid::default();
        let (a, b, c, d) = (
            vec3(0.0, 0.0, 0.0),
            vec3(4000.0, 0.0, 0.0),
            vec3(4000.0, 0.0, 3000.0),
            vec3(0.0, 0.0, 3000.0),
        );
        s.quad(a, b, c, d, vec3(0.0, -1.0, 0.0));
        let k = ((1000.0, 2000.0), (875.0, 2375.0));
        let rest: Vec<Tri> = s
            .triangles
            .iter()
            .flat_map(|t| dreieck_ohne(*t, k, &uz))
            .collect();
        assert!(
            (flaeche(&rest) - (12e6 - 1.5e6)).abs() < 1.0,
            "{}",
            flaeche(&rest)
        );
        for t in &rest {
            let m = (t.p[0] + t.p[1] + t.p[2]) * (1.0 / 3.0);
            assert!(!(m.x > 1000.0 && m.x < 2000.0 && m.z > 875.0 && m.z < 2375.0));
        }
        // Zur Ecke hin offen: alles rechts ab 1 m im Band fällt weg
        let k = ((1000.0, f64::INFINITY), (875.0, 2375.0));
        let rest: Vec<Tri> = s
            .triangles
            .iter()
            .flat_map(|t| dreieck_ohne(*t, k, &uz))
            .collect();
        assert!((flaeche(&rest) - (12e6 - 4.5e6)).abs() < 1.0);
        // Ganz außerhalb: unverändert
        let k = ((5000.0, 6000.0), (875.0, 2375.0));
        let rest: Vec<Tri> = s
            .triangles
            .iter()
            .flat_map(|t| dreieck_ohne(*t, k, &uz))
            .collect();
        assert_eq!(rest.len(), 2);
    }

    #[test]
    fn kante_ohne_loch() {
        let e = Edge {
            a: vec3(0.0, 0.0, 1500.0),
            b: vec3(4000.0, 0.0, 1500.0),
            kind: edge_kind::CUT,
            elem: 0,
            layer: 0,
        };
        let r = kante_ohne(e, ((1000.0, 2000.0), (875.0, 2375.0)), &uz);
        let mut l: Vec<(f64, f64)> = r
            .iter()
            .map(|e| (e.a.x.min(e.b.x), e.a.x.max(e.b.x)))
            .collect();
        l.sort_by(|a, b| a.0.total_cmp(&b.0));
        assert_eq!(l.len(), 2);
        assert!((l[0].1 - 1000.0).abs() < 1e-6 && (l[1].0 - 2000.0).abs() < 1e-6);
    }

    #[test]
    fn kasten_mit_anschlag() {
        let h = Hole {
            seg: 0,
            u: (1000.0, 2000.0),
            z: (875.0, 2385.0),
            reveal_u: (60.0, 60.0),
            reveal_z: (0.0, 60.0),
            shell: 1,
            u_right: true,
            opening: None,
            install: Default::default(),
            outer: true,
            leaf: None,
        };
        assert_eq!(h.kasten(true), ((1000.0, 2000.0), (875.0, 2385.0)));
        assert_eq!(h.kasten(false), ((940.0, 2060.0), (875.0, 2445.0)));
    }

    /// Prüfhaus 10 × 8 m mit WDVS; Segment 0 läuft an x = 0 von y = 0 bis
    /// 8 m. Loch 1 × 1,51 m mit Anschlag 30 mm seitlich und oben.
    fn wand_mit_loch() -> (WallChain, Hole) {
        let mut m = crate::model::Model::with_seed(7);
        let b = m.add_building(1);
        let pts = [
            vec3(0.0, 0.0, 0.0),
            vec3(0.0, 8000.0, 0.0),
            vec3(10000.0, 8000.0, 0.0),
            vec3(10000.0, 0.0, 0.0),
        ];
        let eg = m.build_from_polygon(b, &pts).unwrap();
        let mut c = m.chain_and_floor(eg).unwrap().0;
        let z0 = c.base.max(0.0);
        let h = Hole {
            seg: 0,
            u: (3000.0, 4000.0),
            z: (z0 + 875.0, z0 + 2385.0),
            reveal_u: (30.0, 30.0),
            reveal_z: (0.0, 30.0),
            shell: 1,
            u_right: true,
            opening: None,
            install: Default::default(),
            outer: true,
            leaf: None,
        };
        c.joints.holes.push(h.clone());
        (c, h)
    }

    /// Einbauelement im Grundriss: je Loch zwei Linien im Abstand der
    /// Elementtiefe; an einer Ecköffnung treffen sich die Linien beider
    /// Schenkel an der Ecke. Über oder unter dem Loch keine.
    #[test]
    fn einbauelement_um_die_ecke() {
        let (mut c, mut h) = wand_mit_loch();
        h.install = crate::oeffnung::InstallDepth {
            outer: 120.0,
            element: 80.0,
            inner: 100.0,
        };
        // Schenkel bis zur Ecke von Segment 0 und ab der Ecke auf Segment 1
        h.u = (7000.0, f64::INFINITY);
        let zwei = Hole {
            seg: 1,
            u: (f64::NEG_INFINITY, 1000.0),
            ..h.clone()
        };
        c.joints.holes = vec![h.clone(), zwei];
        let cut = h.z.0 + 500.0;
        let e = c.element_edges(cut);
        assert_eq!(e.len(), 4, "{e:?}");
        assert!(e
            .iter()
            .all(|x| x.kind == edge_kind::ELEMENT && x.a.z == cut));
        for k in 0..2 {
            let (a, b) = (&e[k], &e[k + 2]);
            assert!((a.b - b.a).length() < 1e-6, "Ecke: {:?} {:?}", a.b, b.a);
        }
        let quer = (e[0].a - e[1].a).length();
        assert!((quer - 80.0).abs() < 1e-6, "{quer}");
        assert!(c.element_edges(h.z.0 - 1.0).is_empty());
        assert!(c.element_edges(h.z.1 + 1.0).is_empty());
    }

    fn mitte(t: &Tri) -> Vec3 {
        (t.p[0] + t.p[1] + t.p[2]) * (1.0 / 3.0)
    }

    /// Im Loch bleibt keine Fläche einer Schicht stehen; die Laibungen
    /// zeigen ins Loch, Brüstung nach oben, Sturz nach unten.
    #[test]
    fn wand_mit_loch_im_koerper() {
        let (c, h) = wand_mit_loch();
        let s = c.solid();
        assert!(c.layers.iter().filter(|l| !l.air).count() >= 2);
        for t in s.triangles.iter().filter(|t| t.elem == 0) {
            let k @ ((u0, u1), (z0, z1)) = c.loch_in(&h, t.layer as usize);
            let m = mitte(t);
            let inn = m.y > u0 + 1.0 && m.y < u1 - 1.0 && m.z > z0 + 1.0 && m.z < z1 - 1.0;
            // Nur Laibungen, Brüstung und Sturz liegen im Bereich: deren
            // Normale liegt in der Lochebene
            assert!(!inn || t.n.x.abs() < 1e-6, "{k:?} {m:?} {:?}", t.n);
        }
        let laibung = |y: f64, ny: f64| {
            s.triangles
                .iter()
                .any(|t| (t.n.y - ny).abs() < 1e-6 && t.p.iter().all(|p| (p.y - y).abs() < 1e-3))
        };
        assert!(laibung(3000.0, 1.0) && laibung(4000.0, -1.0));
        assert!(laibung(2970.0, 1.0) && laibung(4030.0, -1.0));
        let deckel = |z: f64, nz: f64| {
            s.triangles
                .iter()
                .any(|t| (t.n.z - nz).abs() < 1e-6 && t.p.iter().all(|p| (p.z - z).abs() < 1e-3))
        };
        assert!(deckel(h.z.0, 1.0) && deckel(h.z.1, -1.0) && deckel(h.z.1 + 30.0, -1.0));
    }

    /// Grundriss durch das Loch: keine Schnittfläche im Loch, dafür die
    /// Schnittkontur quer an den Laibungen.
    #[test]
    fn wand_mit_loch_im_grundriss() {
        let (c, h) = wand_mit_loch();
        let cut = h.z.0 + 625.0;
        let s = c.solid_cut_at(cut);
        let schnitt: Vec<&Tri> = s
            .triangles
            .iter()
            .filter(|t| t.elem == 0 && t.mat & crate::solid::material::CUT != 0)
            .collect();
        assert!(!schnitt.is_empty());
        for t in &schnitt {
            let ((u0, u1), _) = c.loch_in(&h, t.layer as usize);
            let m = mitte(t);
            assert!(!(m.y > u0 + 1.0 && m.y < u1 - 1.0), "{m:?}");
        }
        // Brüstung von oben sichtbar, nur mit Außen- und Innenkante längs
        let br: Vec<&Edge> = s
            .edges
            .iter()
            .filter(|e| (e.a.z - h.z.0).abs() < 1e-3 && (e.b.z - h.z.0).abs() < 1e-3)
            .filter(|e| (e.a.x - e.b.x).abs() < 1e-3)
            .collect();
        let mut xs: Vec<f64> = br.iter().map(|e| e.a.x).collect();
        xs.sort_by(f64::total_cmp);
        xs.dedup_by(|a, b| (*a - *b).abs() < 1e-3);
        assert_eq!(xs.len(), 2, "{xs:?}");
    }

    /// Senkrechter Schnitt quer durch das Loch: die Schnittfläche jeder
    /// Schicht spart das Loch aus.
    #[test]
    fn wand_mit_loch_im_schnitt() {
        let (c, h) = wand_mit_loch();
        let s = c.section_caps(vec3(0.0, 3500.0, 0.0), vec3(0.0, 1.0, 0.0));
        for t in s.triangles.iter().filter(|t| t.elem == 0) {
            let (_, (z0, z1)) = c.loch_in(&h, t.layer as usize);
            let m = mitte(t);
            assert!(!(m.z > z0 + 1.0 && m.z < z1 - 1.0), "{m:?}");
        }
        assert!(s
            .triangles
            .iter()
            .any(|t| t.elem == 0 && mitte(t).z > h.z.1));
    }
}
