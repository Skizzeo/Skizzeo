//! Einfacher Körper aus Dreiecken und sichtbaren Kanten (Darstellung und Treffertest).

use sk_math::{polygon, ray_triangle, vec3, Vec3};

/// Baustoff einer Fläche als Darstellungsschlüssel (siehe [`crate::material_key`]).
/// Farbe und Schraffur stehen in der Baustoffbibliothek des Modells.
pub mod material {
    /// Ohne Baustoff.
    pub const PLAIN: u16 = 0;
    /// Zusatzbit für Schnittflächen (Grundriss, Schnitt).
    pub const CUT: u16 = 0x8000;
}

/// Kantenarten für die Strichstärke in Zeichnungen.
pub mod edge_kind {
    /// Sichtkante (Ansicht).
    pub const VIEW: u8 = 0;
    /// Schnittkontur.
    pub const CUT: u8 = 1;
    /// Feine Linie, z. B. Fuge zwischen zwei Schichten.
    pub const FINE: u8 = 2;
    /// Schnittkontur einer nicht tragenden Schicht (z. B. Dämmung), mitteldick.
    pub const CUT_LAYER: u8 = 3;
    /// Anzahl der Kantenarten (für Tabellen je Art).
    pub const COUNT: usize = 4;
    /// Hintergrund: Grundriss des Geschosses unter dem aktiven (E16); Stil aus
    /// [`crate::Display::background`], nicht aus den Tabellen je Art.
    pub const BACKGROUND: u8 = 4;
}

/// Punkt `p` auf Höhe `z`.
pub fn at_z(p: Vec3, z: f64) -> Vec3 {
    vec3(p.x, p.y, z)
}

/// Rechte Normale einer waagerechten Richtung (bei Umriss gegen den
/// Uhrzeigersinn: außen).
pub fn right_of(d: Vec3) -> Vec3 {
    vec3(d.y, -d.x, 0.0)
}

/// Läuft der geschlossene Umriss `c` an Punkt `i` gerade weiter (dort keine
/// senkrechte Kante)?
pub fn straight_at(c: &[Vec3], i: usize) -> bool {
    let n = c.len();
    let (a, b, d) = (c[(i + n - 1) % n], c[i], c[(i + 1) % n]);
    let (u, w) = ((b - a).normalized(), (d - b).normalized());
    (u.x * w.y - u.y * w.x).abs() < 1e-9 && u.dot(w) > 0.0
}

/// Senkrechte Schnittebene durch `p0` mit waagerechter Normale `n`: `along`
/// läuft in der Ebene, [`SectionFrame::pt`] setzt Punkte aus Lauflänge und
/// Höhe zusammen (gleiche Lauflänge wie [`polygon::plane_intervals`]).
#[derive(Clone, Copy, Debug)]
pub struct SectionFrame {
    /// Normale, waagerecht und normiert.
    pub n: Vec3,
    /// Richtung in der Ebene.
    pub along: Vec3,
    base: Vec3,
}

impl SectionFrame {
    pub fn new(p0: Vec3, n: Vec3) -> SectionFrame {
        let n = vec3(n.x, n.y, 0.0).normalized();
        let along = vec3(-n.y, n.x, 0.0);
        let base = vec3(p0.x, p0.y, 0.0) - along * vec3(p0.x, p0.y, 0.0).dot(along);
        SectionFrame { n, along, base }
    }

    /// Punkt in der Ebene bei Lauflänge `u` und Höhe `z`.
    pub fn pt(&self, u: f64, z: f64) -> Vec3 {
        self.base + self.along * u + vec3(0.0, 0.0, z)
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Edge {
    pub a: Vec3,
    pub b: Vec3,
    pub kind: u8,
}

#[derive(Clone, Copy, Debug)]
pub struct Tri {
    pub p: [Vec3; 3],
    /// Flächennormale.
    pub n: Vec3,
    pub mat: u16,
    /// Musterkoordinaten (für Schraffuren): u längs, v quer zur Schicht (0..1).
    pub uv: [[f64; 2]; 3],
    /// Teil des Körpers, zu dem das Dreieck gehört (bei Wänden: Segment im Zug).
    pub elem: u32,
}

#[derive(Clone, Debug, Default)]
pub struct Solid {
    pub triangles: Vec<Tri>,
    /// Sichtbare Kanten (Umrisse der Flächen, keine inneren Fugen).
    pub edges: Vec<Edge>,
    /// Baustoff für die nächsten Flächen.
    pub mat: u16,
    /// Art der nächsten Kanten.
    pub edge_kind: u8,
    /// Teil (Segment) für die nächsten Flächen.
    pub elem: u32,
}

impl Solid {
    /// Viereck (eben, konvex) als zwei Dreiecke.
    pub fn quad(&mut self, a: Vec3, b: Vec3, c: Vec3, d: Vec3, normal: Vec3) {
        self.quad_uv([a, b, c, d], normal, [[0.0; 2]; 4]);
    }

    /// Viereck mit Musterkoordinaten je Ecke.
    pub fn quad_uv(&mut self, p: [Vec3; 4], normal: Vec3, uv: [[f64; 2]; 4]) {
        let (mat, elem) = (self.mat, self.elem);
        self.triangles.push(Tri {
            p: [p[0], p[1], p[2]],
            n: normal,
            mat,
            uv: [uv[0], uv[1], uv[2]],
            elem,
        });
        self.triangles.push(Tri {
            p: [p[0], p[2], p[3]],
            n: normal,
            mat,
            uv: [uv[0], uv[2], uv[3]],
            elem,
        });
    }

    /// Waagerechte Fläche aus dem Polygon `pts` auf Höhe `z`, nach oben
    /// (`up`) oder nach unten gerichtet.
    pub fn cap(&mut self, pts: &[Vec3], z: f64, up: bool) {
        let nrm = vec3(0.0, 0.0, if up { 1.0 } else { -1.0 });
        for t in polygon::triangulate(pts) {
            let [a, b, c] = t.map(|k| at_z(pts[k], z));
            self.triangles.push(Tri {
                p: if up { [a, b, c] } else { [a, c, b] },
                n: nrm,
                mat: self.mat,
                uv: [[0.0; 2]; 3],
                elem: self.elem,
            });
        }
    }

    /// Senkrechte Seitenflächen entlang des geschlossenen Umrisses `c`
    /// zwischen `z0` und `z1`, nach außen (`outward`, rechts der Laufrichtung)
    /// oder nach innen gerichtet; ohne Kanten.
    pub fn sides(&mut self, c: &[Vec3], z0: f64, z1: f64, outward: bool) {
        let n = c.len();
        for i in 0..n {
            let (a, b) = (c[i], c[(i + 1) % n]);
            let r = right_of((b - a).normalized());
            let nr = if outward { r } else { -r };
            self.quad(at_z(a, z0), at_z(b, z0), at_z(b, z1), at_z(a, z1), nr);
        }
    }

    pub fn edge(&mut self, a: Vec3, b: Vec3) {
        if (a - b).length() > 1e-6 {
            self.edges.push(Edge {
                a,
                b,
                kind: self.edge_kind,
            });
        }
    }

    pub fn append(&mut self, other: &Solid) {
        self.triangles.extend_from_slice(&other.triangles);
        self.edges.extend_from_slice(&other.edges);
    }

    pub fn raycast(&self, origin: Vec3, dir: Vec3) -> Option<f64> {
        self.raycast_elem(origin, dir).map(|h| h.0)
    }

    /// Nächster Treffer: Abstand und Teil ([`Tri::elem`]) des getroffenen Dreiecks.
    pub fn raycast_elem(&self, origin: Vec3, dir: Vec3) -> Option<(f64, u32)> {
        self.triangles
            .iter()
            .filter_map(|t| ray_triangle(origin, dir, t.p[0], t.p[1], t.p[2]).map(|d| (d, t.elem)))
            .min_by(|a, b| a.0.total_cmp(&b.0))
    }

    pub fn is_empty(&self) -> bool {
        self.triangles.is_empty()
    }

    /// Kleinster umschließender Quader (min, max).
    pub fn bounds(&self) -> Option<(Vec3, Vec3)> {
        let mut pts = self.triangles.iter().flat_map(|t| t.p.iter());
        let first = *pts.next()?;
        let (mut lo, mut hi) = (first, first);
        for p in pts {
            lo = vec3(lo.x.min(p.x), lo.y.min(p.y), lo.z.min(p.z));
            hi = vec3(hi.x.max(p.x), hi.y.max(p.y), hi.z.max(p.z));
        }
        Some((lo, hi))
    }

    /// Teil des Körpers hinter der Ebene durch `p0` mit Normale `n`
    /// (behalten wird, was auf der Gegenseite von `n` liegt). Ohne Deckel.
    pub fn clipped(&self, p0: Vec3, n: Vec3) -> Solid {
        let side = |p: Vec3| (p - p0).dot(n);
        let mut out = Solid {
            mat: self.mat,
            ..Solid::default()
        };
        for t in &self.triangles {
            let mut poly: Vec<(Vec3, [f64; 2])> = Vec::with_capacity(4);
            for k in 0..3 {
                let (a, b) = (t.p[k], t.p[(k + 1) % 3]);
                let (ua, ub) = (t.uv[k], t.uv[(k + 1) % 3]);
                let (da, db) = (side(a), side(b));
                if da <= 0.0 {
                    poly.push((a, ua));
                }
                if (da < 0.0) != (db < 0.0) && (da - db).abs() > 1e-12 {
                    let f = da / (da - db);
                    let uv = [ua[0] + (ub[0] - ua[0]) * f, ua[1] + (ub[1] - ua[1]) * f];
                    poly.push((a + (b - a) * f, uv));
                }
            }
            for k in 1..poly.len().saturating_sub(1) {
                out.triangles.push(Tri {
                    p: [poly[0].0, poly[k].0, poly[k + 1].0],
                    n: t.n,
                    mat: t.mat,
                    uv: [poly[0].1, poly[k].1, poly[k + 1].1],
                    elem: t.elem,
                });
            }
        }
        for e in &self.edges {
            let (a, b) = (e.a, e.b);
            let (da, db) = (side(a), side(b));
            out.edge_kind = e.kind;
            match (da <= 0.0, db <= 0.0) {
                (true, true) => out.edges.push(*e),
                (false, false) => {}
                _ => {
                    let x = a + (b - a) * (da / (da - db));
                    out.edge(if da <= 0.0 { a } else { b }, x);
                }
            }
        }
        out.edge_kind = edge_kind::VIEW;
        out
    }
}

/// Waagerechte Kante in Höhe `z` am Rand einer senkrechten Fläche: Kante,
/// Normale und Baustoff der Fläche, und ob die Fläche unter der Kante liegt.
struct SeamEdge {
    edge: usize,
    n: Vec3,
    mat: u16,
    below: bool,
}

fn seam_edges(s: &Solid, z: f64) -> Vec<SeamEdge> {
    const EPS: f64 = 1e-6;
    let at = |p: Vec3| (p.z - z).abs() < EPS;
    // Senkrechte Dreiecke mit einer Seite in Höhe z
    let walls: Vec<(Vec3, Vec3, &Tri)> = s
        .triangles
        .iter()
        .filter(|t| t.n.z.abs() < EPS)
        .filter_map(|t| {
            (0..3).find_map(|k| {
                let (a, b) = (t.p[k], t.p[(k + 1) % 3]);
                (at(a) && at(b)).then_some((a, b, t))
            })
        })
        .collect();
    let mut out = Vec::new();
    for (i, e) in s.edges.iter().enumerate() {
        if !(at(e.a) && at(e.b)) {
            continue;
        }
        let d = e.b - e.a;
        let len = d.length();
        if len < EPS {
            continue;
        }
        let d = d * (1.0 / len);
        // Fläche, auf deren Rand die Kante liegt
        let face = walls.iter().find(|(a, b, _)| {
            let off = |p: Vec3| {
                let r = p - e.a;
                (r - d * r.dot(d)).length()
            };
            if off(*a) > 1e-3 || off(*b) > 1e-3 {
                return false;
            }
            let (ta, tb) = ((*a - e.a).dot(d), (*b - e.a).dot(d));
            ta.max(tb) > 1e-3 && ta.min(tb) < len - 1e-3
        });
        if let Some((_, _, t)) = face {
            let zc = (t.p[0].z + t.p[1].z + t.p[2].z) / 3.0;
            out.push(SeamEdge {
                edge: i,
                n: t.n,
                mat: t.mat,
                below: zc < z,
            });
        }
    }
    out
}

/// Kante `e` ohne die Abschnitte `cut` (längs der Kante ab `e.a`).
fn without(e: Edge, mut cut: Vec<(f64, f64)>, out: &mut Vec<Edge>) {
    let d = e.b - e.a;
    let len = d.length();
    cut.sort_by(|a, b| a.0.total_cmp(&b.0));
    let at = |t: f64| e.a + d * (t / len);
    let mut from = 0.0;
    for (a, b) in cut {
        if a - from > 1e-6 {
            out.push(Edge {
                a: at(from),
                b: at(a),
                kind: e.kind,
            });
        }
        from = f64::max(from, b);
    }
    if len - from > 1e-6 {
        out.push(Edge {
            a: at(from),
            b: e.b,
            kind: e.kind,
        });
    }
}

/// Fügt zwei übereinanderstehende Körper in Höhe `z` ohne Naht zusammen
/// (EG-Wand und OG-Wand, auch ihre Schnittflächen): Läuft eine senkrechte
/// Fläche mit gleichem Baustoff in derselben Ebene über `z` weiter, fällt die
/// waagerechte Kante dort weg, in beiden Körpern und genau so weit, wie sich
/// die Flächen überdecken. Deckungsgleiche Deck- und Bodenflächen in `z`
/// entfallen ebenfalls. Wo die Flächen springen (Vor- oder Rücksprung) oder
/// der Baustoff wechselt, bleibt die Kante: eine saubere Stufe.
///
/// Die Körper bleiben getrennt (Auswahl und Mengen je Geschoss).
pub fn merge_seam(lower: &mut Solid, upper: &mut Solid, z: f64) {
    let (sa, sb) = (seam_edges(lower, z), seam_edges(upper, z));
    let mut cut_a: Vec<Vec<(f64, f64)>> = vec![Vec::new(); lower.edges.len()];
    let mut cut_b: Vec<Vec<(f64, f64)>> = vec![Vec::new(); upper.edges.len()];
    for x in &sa {
        let ea = lower.edges[x.edge];
        let len = (ea.b - ea.a).length();
        let d = (ea.b - ea.a) * (1.0 / len);
        for y in &sb {
            if x.mat != y.mat || x.below == y.below || x.n.dot(y.n) < 1.0 - 1e-9 {
                continue;
            }
            let eb = upper.edges[y.edge];
            // Auf derselben Geraden (gleiche Höhe ist gegeben). Nicht über die
            // Normale geprüft: an Gehrungen mit Ersatzecke steht die Fläche
            // schräg zu ihrer angegebenen Normale.
            let off = |p: Vec3| {
                let r = p - ea.a;
                (r - d * r.dot(d)).length()
            };
            if off(eb.a) > 1e-3 || off(eb.b) > 1e-3 {
                continue;
            }
            let (t0, t1) = ((eb.a - ea.a).dot(d), (eb.b - ea.a).dot(d));
            let (lo, hi) = (t0.min(t1).max(0.0), t0.max(t1).min(len));
            if hi - lo < 1e-6 {
                continue;
            }
            cut_a[x.edge].push((lo, hi));
            // Derselbe Abschnitt längs der oberen Kante
            let lb = (eb.b - eb.a).length();
            let db = (eb.b - eb.a) * (1.0 / lb);
            let (u0, u1) = (
                (ea.a + d * lo - eb.a).dot(db),
                (ea.a + d * hi - eb.a).dot(db),
            );
            cut_b[y.edge].push((u0.min(u1), u0.max(u1)));
        }
    }
    for (s, cuts) in [(&mut *lower, cut_a), (&mut *upper, cut_b)] {
        if cuts.iter().all(|c| c.is_empty()) {
            continue;
        }
        let mut edges = Vec::with_capacity(s.edges.len());
        for (e, c) in s.edges.iter().zip(cuts) {
            if c.is_empty() {
                edges.push(*e);
            } else {
                without(*e, c, &mut edges);
            }
        }
        s.edges = edges;
    }
    // Deckungsgleiche Flächen in z (Deckfläche unten, Bodenfläche oben)
    let flat_at = |t: &Tri| t.n.z.abs() > 1.0 - 1e-9 && t.p.iter().all(|p| (p.z - z).abs() < 1e-6);
    let same = |a: &Tri, b: &Tri| {
        a.p.iter()
            .all(|p| b.p.iter().any(|q| (*p - *q).length() < 1e-6))
    };
    let tops: Vec<usize> = (0..lower.triangles.len())
        .filter(|&i| flat_at(&lower.triangles[i]) && lower.triangles[i].n.z > 0.0)
        .collect();
    let bottoms: Vec<usize> = (0..upper.triangles.len())
        .filter(|&i| flat_at(&upper.triangles[i]) && upper.triangles[i].n.z < 0.0)
        .collect();
    let (mut drop_a, mut drop_b) = (
        vec![false; lower.triangles.len()],
        vec![false; upper.triangles.len()],
    );
    for &i in &tops {
        let a = &lower.triangles[i];
        if let Some(&j) = bottoms.iter().find(|&&j| {
            !drop_b[j] && upper.triangles[j].mat == a.mat && same(a, &upper.triangles[j])
        }) {
            drop_a[i] = true;
            drop_b[j] = true;
        }
    }
    let mut k = 0;
    lower.triangles.retain(|_| {
        k += 1;
        !drop_a[k - 1]
    });
    k = 0;
    upper.triangles.retain(|_| {
        k += 1;
        !drop_b[k - 1]
    });
}
