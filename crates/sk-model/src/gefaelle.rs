//! Gefälledämmung des Flachdachs (Konzept `gefaelledaemmung/konzept-
//! gefaelledaemmung.md` §4, Jörn 10.10.): Oberfläche der Keilschicht als
//! untere Hülle quadratischer Abflusskegel um die Abläufe.
//!
//! Für jeden Punkt p der Dachfläche ist der Fließweg der kürzeste Weg zum
//! nächsten Ablauf, gemessen als `max(|Δu|, |Δv|)` im Gebäuderaster (u längs
//! der längsten Kante) und um die Innenecken herum. Die Keildicke ist
//! Gefälle × Fließweg. Daraus folgt: Jede Teilfläche ist eben und fällt
//! parallel zu einer Achse (Gefälleplatten im Folgesystem), Kehlen liegen
//! unter 45° mit Gefälle / √2, und außer den Abläufen gibt es keinen
//! Tiefpunkt.
//!
//! Rechenweg (exakt, ohne Raster): Die Dachfläche wird in Dreiecke zerlegt.
//! Was ein Ablauf oder eine Innenecke sieht, folgt als konvexe Stücke aus
//! einem Trichter durch die Dreiecke. Die Werte der Innenecken liefert
//! Dijkstra über Abläufe und Innenecken. Dann schneidet jede Quelle mit
//! ihren vier Ebenen (je Keilwinkel) die bisherigen Stücke dort ab, wo sie
//! tiefer liegt. Alles bleibt konvex und wird nur mit Halbebenen geschnitten.

use sk_math::{polygon, vec3, Vec3};

/// Gefälle beim Einschalten (Prozent, Flachdachrichtlinie: ≥ 2 %).
pub const DEFAULT_SLOPE: f64 = 2.0;
/// Größter Fließweg zum nächsten Ablauf für den Vorschlag (mm; 160 mm Keil
/// bei 2 %).
pub const MAX_PATH: f64 = 8000.0;
/// Dachfläche je Ablauf für den Vorschlag (mm², DN 100 bei 300 l/(s·ha)).
pub const DRAIN_AREA: f64 = 150.0e6;
/// Abstand der vorgeschlagenen Abläufe von den Ecken (mm, Regel ≥ 30/50 cm
/// ab Flansch).
pub const CORNER_GAP: f64 = 600.0;
/// Mindestzahl der Abläufe (Flachdachrichtlinie: zwei oder einer mit
/// Notüberlauf; jeder Ablauf bekommt hier einen Notüberlauf dazu).
pub const MIN_DRAINS: usize = 2;
/// Raster der Ablaufstellen entlang der Kante (mm).
const CANDIDATE_STEP: f64 = 500.0;

/// Längentoleranz der Schnitte (mm).
const EPS: f64 = 1e-6;
/// Kleinere Stücke fallen weg (mm²).
const MIN_AREA: f64 = 1.0;
/// Gleicher Fließweg (mm): gleiche Ebene, keine Verbesserung.
const SAME: f64 = 0.01;

/// Gefälle eines Dachaufbaus: Prozent (0 = waagerecht) und Abläufe (Punkte
/// auf der Innenfläche der Aufkantung, z = 0; beim Rechnen auf die nächste
/// Kante gelegt).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Drainage {
    pub slope: f64,
    pub drains: Vec<Vec3>,
}

impl Drainage {
    /// Hat der Aufbau Gefälle?
    pub fn on(&self) -> bool {
        self.slope > 0.0
    }
}

/// Ebene Teilfläche der Keilschicht (konvex, gegen den Uhrzeigersinn, z = 0).
#[derive(Clone, Debug, PartialEq)]
pub struct Face {
    pub pts: Vec<Vec3>,
    /// Richtung, in die die Dämmung dicker wird (waagerecht, Länge 1); das
    /// Wasser fließt entgegen.
    pub rise: Vec3,
    /// Fließweg im Ursprung (mm): `path(p) = c + rise · p`.
    pub c: f64,
    /// Ablauf, in den die Fläche entwässert (Index in [`SlopeField::drains`]).
    pub drain: usize,
}

impl Face {
    /// Fließweg am Punkt `p` (mm).
    pub fn path(&self, p: Vec3) -> f64 {
        self.c + self.rise.x * p.x + self.rise.y * p.y
    }

    fn same_plane(&self, o: &Face) -> bool {
        (self.rise - o.rise).length() < 1e-9 && (self.c - o.c).abs() < SAME
    }
}

/// Kehle (Tiefpunktlinie) oder Grat (Hochpunktlinie) zwischen zwei
/// Teilflächen, z = 0.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Crease {
    pub a: Vec3,
    pub b: Vec3,
    pub valley: bool,
}

/// Gefälleplan einer Dachfläche.
#[derive(Clone, Debug, PartialEq)]
pub struct SlopeField {
    /// Gefälle als Verhältnis (0,02 für 2 %).
    pub slope: f64,
    /// Hauptrichtung u (waagerecht, Länge 1); v steht links davon.
    pub axis: Vec3,
    /// Abläufe auf dem Umriss.
    pub drains: Vec<Vec3>,
    pub faces: Vec<Face>,
    pub creases: Vec<Crease>,
    outline: Vec<Vec3>,
}

fn flat(p: Vec3) -> Vec3 {
    vec3(p.x, p.y, 0.0)
}

fn cross2(a: Vec3, b: Vec3) -> f64 {
    a.x * b.y - a.y * b.x
}

/// Links gedrehte Richtung.
fn left(d: Vec3) -> Vec3 {
    vec3(-d.y, d.x, 0.0)
}

/// Halbebene `n · p + k ≥ 0` (n Länge 1: Abstand in mm).
#[derive(Clone, Copy, Debug)]
struct Half {
    n: Vec3,
    k: f64,
}

impl Half {
    /// Links der Geraden durch `a` in Richtung `d`.
    fn left_of(a: Vec3, d: Vec3) -> Option<Half> {
        Half::new(left(d), a)
    }

    /// `n · (p − a) ≥ 0`; `None` ohne Richtung.
    fn new(n: Vec3, a: Vec3) -> Option<Half> {
        let l = (n.x * n.x + n.y * n.y).sqrt();
        (l > 1e-12).then(|| {
            let n = vec3(n.x / l, n.y / l, 0.0);
            Half {
                n,
                k: -(n.x * a.x + n.y * a.y),
            }
        })
    }

    fn at(&self, p: Vec3) -> f64 {
        self.n.x * p.x + self.n.y * p.y + self.k
    }

    fn flip(self) -> Half {
        Half {
            n: self.n * -1.0,
            k: -self.k,
        }
    }
}

/// Konvexes Polygon geschnitten mit einer Halbebene (Sutherland-Hodgman).
fn clip(poly: &[Vec3], h: &Half) -> Vec<Vec3> {
    let n = poly.len();
    let mut out = Vec::with_capacity(n + 1);
    for i in 0..n {
        let (a, b) = (poly[i], poly[(i + 1) % n]);
        let (fa, fb) = (h.at(a), h.at(b));
        if fa >= -EPS {
            out.push(a);
        }
        if (fa > EPS && fb < -EPS) || (fa < -EPS && fb > EPS) {
            out.push(a + (b - a) * (fa / (fa - fb)));
        }
    }
    out
}

fn area(poly: &[Vec3]) -> f64 {
    if poly.len() < 3 {
        0.0
    } else {
        polygon::signed_area(poly)
    }
}

/// Liegt `p` im konvexen Polygon (gegen den Uhrzeigersinn, mit Toleranz)?
fn contains(poly: &[Vec3], p: Vec3, tol: f64) -> bool {
    let n = poly.len();
    n >= 3
        && (0..n).all(|i| {
            let (a, b) = (poly[i], poly[(i + 1) % n]);
            let d = b - a;
            let l = (d.x * d.x + d.y * d.y).sqrt();
            l < EPS || cross2(d, p - a) / l >= -tol
        })
}

fn dist_seg(p: Vec3, a: Vec3, b: Vec3) -> f64 {
    sk_math::dist_to_segment((p.x, p.y), (a.x, a.y), (b.x, b.y))
}

/// Nächster Punkt auf dem geschlossenen Umriss: Kante und Punkt.
fn project(outline: &[Vec3], p: Vec3) -> (usize, Vec3) {
    let n = outline.len();
    let mut best = (0, outline[0], f64::INFINITY);
    for i in 0..n {
        let (a, b) = (outline[i], outline[(i + 1) % n]);
        let d = b - a;
        let ll = d.x * d.x + d.y * d.y;
        let t = if ll > 0.0 {
            (((p - a).x * d.x + (p - a).y * d.y) / ll).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let q = a + d * t;
        let e = (flat(p) - q).length();
        if e < best.2 {
            best = (i, q, e);
        }
    }
    (best.0, best.1)
}

/// Hauptrichtung: längste Kante, eindeutig ausgerichtet.
fn main_axis(outline: &[Vec3]) -> Vec3 {
    let n = outline.len();
    let mut best = (Vec3::X, 0.0);
    for i in 0..n {
        let d = outline[(i + 1) % n] - outline[i];
        let l = (d.x * d.x + d.y * d.y).sqrt();
        if l > best.1 + 1e-6 {
            best = (vec3(d.x / l, d.y / l, 0.0), l);
        }
    }
    let u = best.0;
    if u.x < -1e-9 || (u.x.abs() <= 1e-9 && u.y < 0.0) {
        u * -1.0
    } else {
        u
    }
}

/// Fließweg-Abstand im Raster (u, v).
fn linf(axis: Vec3, a: Vec3, b: Vec3) -> f64 {
    let d = b - a;
    let u = d.x * axis.x + d.y * axis.y;
    let v = cross2(axis, d);
    u.abs().max(v.abs())
}

/// Einspringende Ecken eines Umrisses gegen den Uhrzeigersinn.
fn reflex(outline: &[Vec3]) -> Vec<Vec3> {
    let n = outline.len();
    (0..n)
        .filter(|&i| {
            let (a, b, c) = (outline[(i + n - 1) % n], outline[i], outline[(i + 1) % n]);
            let t = cross2(b - a, c - b);
            t < -1e-9 * (b - a).length() * (c - b).length()
        })
        .map(|i| outline[i])
        .collect()
}

/// Dreieckszerlegung mit Nachbarschaft über Diagonalen.
struct Mesh {
    tris: Vec<[Vec3; 3]>,
    /// Nachbar über Kante k (Ecke k → k+1), `None` am Umriss.
    adj: Vec<[Option<usize>; 3]>,
}

impl Mesh {
    fn new(outline: &[Vec3]) -> Mesh {
        let idx = polygon::triangulate(outline);
        let tris: Vec<[Vec3; 3]> = idx.iter().map(|t| t.map(|i| outline[i])).collect();
        let mut adj = vec![[None; 3]; idx.len()];
        let mut seen: std::collections::HashMap<(usize, usize), (usize, usize)> =
            std::collections::HashMap::new();
        for (ti, t) in idx.iter().enumerate() {
            for k in 0..3 {
                let (a, b) = (t[k], t[(k + 1) % 3]);
                let key = (a.min(b), a.max(b));
                if let Some(&(tj, kj)) = seen.get(&key) {
                    adj[ti][k] = Some(tj);
                    adj[tj][kj] = Some(ti);
                } else {
                    seen.insert(key, (ti, k));
                }
            }
        }
        Mesh { tris, adj }
    }

    /// Was der Punkt `s` (auf dem Umriss oder innen) sieht: konvexe Stücke.
    fn visible(&self, s: Vec3) -> Vec<Vec<Vec3>> {
        let mut out = Vec::new();
        let tol = 1e-3;
        let seeds: Vec<usize> = (0..self.tris.len())
            .filter(|&t| contains(&self.tris[t], s, tol))
            .collect();
        for &t in &seeds {
            out.push(self.tris[t].to_vec());
            for k in 0..3 {
                let (a, b) = (self.tris[t][k], self.tris[t][(k + 1) % 3]);
                let Some(next) = self.adj[t][k] else {
                    continue;
                };
                if seeds.contains(&next) || dist_seg(s, a, b) < tol {
                    continue;
                }
                if let Some(w) = Wedge::of(s, a, b) {
                    self.funnel(s, next, t, w, &mut out);
                }
            }
        }
        out
    }

    fn funnel(&self, s: Vec3, t: usize, from: usize, w: Wedge, out: &mut Vec<Vec<Vec3>>) {
        let mut piece = self.tris[t].to_vec();
        for h in w.halves(s) {
            piece = clip(&piece, &h);
        }
        if area(&piece) >= MIN_AREA {
            out.push(piece);
        }
        for k in 0..3 {
            let Some(next) = self.adj[t][k] else {
                continue;
            };
            if next == from {
                continue;
            }
            let (a, b) = (self.tris[t][k], self.tris[t][(k + 1) % 3]);
            if let Some(w2) = Wedge::of(s, a, b).and_then(|e| w.meet(e)) {
                self.funnel(s, next, t, w2, out);
            }
        }
    }
}

/// Sichtkegel von einem Punkt: von Richtung `a` gegen den Uhrzeigersinn bis
/// `b`, unter 180°.
#[derive(Clone, Copy, Debug)]
struct Wedge {
    a: Vec3,
    b: Vec3,
}

impl Wedge {
    fn of(s: Vec3, p: Vec3, q: Vec3) -> Option<Wedge> {
        let (a, b) = (flat(p - s), flat(q - s));
        let c = cross2(a, b);
        let w = if c >= 0.0 {
            Wedge { a, b }
        } else {
            Wedge { a: b, b: a }
        };
        w.open().then_some(w)
    }

    fn open(&self) -> bool {
        cross2(self.a, self.b) > 1e-9 * self.a.length() * self.b.length()
    }

    fn meet(&self, o: Wedge) -> Option<Wedge> {
        let a = if cross2(self.a, o.a) > 0.0 {
            o.a
        } else {
            self.a
        };
        let b = if cross2(self.b, o.b) > 0.0 {
            self.b
        } else {
            o.b
        };
        let w = Wedge { a, b };
        (w.open() && cross2(self.a, b) > 0.0 && cross2(o.a, b) > 0.0).then_some(w)
    }

    fn halves(&self, s: Vec3) -> Vec<Half> {
        [
            Half::left_of(s, self.a),
            Half::left_of(s, self.b).map(Half::flip),
        ]
        .into_iter()
        .flatten()
        .collect()
    }
}

/// Quelle eines Abflusskegels: Ablauf (Wert 0) oder Innenecke (Fließweg
/// von dort zum Ablauf).
#[derive(Clone, Debug)]
struct Source {
    at: Vec3,
    value: f64,
    drain: usize,
    sees: Vec<Vec<Vec3>>,
}

/// Abläufe und Innenecken mit Werten (Dijkstra über die Sichtbarkeit).
fn sources(mesh: &Mesh, axis: Vec3, drains: &[Vec3], corners: &[Vec3]) -> Vec<Source> {
    let mut src: Vec<Source> = drains
        .iter()
        .enumerate()
        .map(|(i, d)| Source {
            at: *d,
            value: 0.0,
            drain: i,
            sees: mesh.visible(*d),
        })
        .chain(corners.iter().map(|c| Source {
            at: *c,
            value: f64::INFINITY,
            drain: usize::MAX,
            sees: mesh.visible(*c),
        }))
        .collect();
    relax(&mut src, axis);
    src.retain(|s| s.value.is_finite());
    src
}

fn sees(s: &Source, p: Vec3) -> bool {
    s.sees.iter().any(|piece| contains(piece, p, 1e-3))
}

fn relax(src: &mut [Source], axis: Vec3) {
    let n = src.len();
    let mut done = vec![false; n];
    loop {
        let next = (0..n)
            .filter(|&i| !done[i] && src[i].value.is_finite())
            .min_by(|&a, &b| src[a].value.total_cmp(&src[b].value));
        let Some(i) = next else {
            break;
        };
        done[i] = true;
        for j in 0..n {
            if done[j] || !sees(&src[i], src[j].at) {
                continue;
            }
            let v = src[i].value + linf(axis, src[i].at, src[j].at);
            if v < src[j].value - 1e-9 {
                src[j].value = v;
                src[j].drain = src[i].drain;
            }
        }
    }
}

/// Stück der Hülle: Polygon und Ebene der Quelle (`None`: noch keine).
#[derive(Clone, Debug)]
struct Piece {
    poly: Vec<Vec3>,
    face: Option<Face>,
    src: usize,
}

fn bbox(p: &[Vec3]) -> (f64, f64, f64, f64) {
    p.iter().fold(
        (
            f64::INFINITY,
            f64::INFINITY,
            f64::NEG_INFINITY,
            f64::NEG_INFINITY,
        ),
        |b, q| (b.0.min(q.x), b.1.min(q.y), b.2.max(q.x), b.3.max(q.y)),
    )
}

impl SlopeField {
    /// Gefälleplan der Dachfläche `outline` (Innenfläche der Aufkantung) mit
    /// den Abläufen `drains` und dem Gefälle `slope` in Prozent; `None` ohne
    /// Ablauf, ohne Gefälle oder ohne Fläche.
    pub fn compute(outline: &[Vec3], drains: &[Vec3], slope: f64) -> Option<SlopeField> {
        let outline = polygon::to_ccw(&polygon::simplified(outline));
        if slope <= 0.0 || drains.is_empty() || outline.len() < 3 || area(&outline) < MIN_AREA {
            return None;
        }
        let axis = main_axis(&outline);
        // Abläufe auf den Umriss, doppelte zusammen
        let mut on: Vec<Vec3> = Vec::new();
        for d in drains {
            let p = project(&outline, *d).1;
            if on.iter().all(|q| (*q - p).length() > 10.0) {
                on.push(p);
            }
        }
        let mesh = Mesh::new(&outline);
        if mesh.tris.is_empty() {
            return None;
        }
        let src = sources(&mesh, axis, &on, &reflex(&outline));
        let mut pieces: Vec<Piece> = mesh
            .tris
            .iter()
            .map(|t| Piece {
                poly: t.to_vec(),
                face: None,
                src: usize::MAX,
            })
            .collect();
        let mut order: Vec<usize> = (0..src.len()).collect();
        order.sort_by(|&a, &b| src[a].value.total_cmp(&src[b].value));
        let dirs = [axis, axis * -1.0, left(axis), left(axis) * -1.0];
        for &si in &order {
            let s = &src[si];
            for v in &s.sees {
                for d in dirs {
                    let w = left(d);
                    let wedge = [Half::new(d - w, s.at), Half::new(d + w, s.at)];
                    let mut r = v.clone();
                    for h in wedge.iter().flatten() {
                        r = clip(&r, h);
                    }
                    if area(&r) < MIN_AREA {
                        continue;
                    }
                    let face = Face {
                        pts: Vec::new(),
                        rise: d,
                        c: s.value - (d.x * s.at.x + d.y * s.at.y),
                        drain: s.drain,
                    };
                    pieces = lower(pieces, &r, &face, si, &src);
                }
            }
        }
        let mut faces: Vec<(Vec<Vec3>, Face)> = pieces
            .into_iter()
            .filter_map(|p| p.face.map(|f| (p.poly, f)))
            .collect();
        merge(&mut faces);
        let faces: Vec<Face> = faces
            .into_iter()
            .map(|(poly, f)| Face {
                pts: polygon::simplified(&poly),
                ..f
            })
            .filter(|f| f.pts.len() >= 3)
            .collect();
        let creases = creases(&faces);
        Some(SlopeField {
            slope: slope / 100.0,
            axis,
            drains: on,
            faces,
            creases,
            outline,
        })
    }

    /// Umriss, auf dem gerechnet wurde (gegen den Uhrzeigersinn).
    pub fn outline(&self) -> &[Vec3] {
        &self.outline
    }

    /// Teilfläche, in der `p` liegt.
    pub fn face_at(&self, p: Vec3) -> Option<&Face> {
        let p = flat(p);
        self.faces
            .iter()
            .find(|f| contains(&f.pts, p, 1e-3))
            .or_else(|| {
                self.faces.iter().min_by(|a, b| {
                    let da = edge_dist(&a.pts, p);
                    let db = edge_dist(&b.pts, p);
                    da.total_cmp(&db)
                })
            })
    }

    /// Keildicke über der Dicke am Ablauf am Punkt `p` (mm).
    pub fn wedge_at(&self, p: Vec3) -> f64 {
        self.face_at(p)
            .map_or(0.0, |f| (self.slope * f.path(p)).max(0.0))
    }

    /// Dachfläche (mm²).
    pub fn area(&self) -> f64 {
        self.faces.iter().map(|f| area(&f.pts)).sum()
    }

    /// Volumen des Keils über der Dicke am Ablauf (mm³): je Fläche Fläche ×
    /// Dicke im Schwerpunkt (exakt, die Dicke ist linear).
    pub fn wedge_volume(&self) -> f64 {
        self.faces
            .iter()
            .map(|f| {
                let a = area(&f.pts);
                polygon::centroid(&f.pts).map_or(0.0, |c| a * self.slope * f.path(c))
            })
            .sum()
    }

    /// Mittlere Keildicke (mm).
    pub fn wedge_mean(&self) -> f64 {
        let a = self.area();
        if a > 0.0 {
            self.wedge_volume() / a
        } else {
            0.0
        }
    }

    /// Größte Keildicke (mm).
    pub fn wedge_max(&self) -> f64 {
        self.faces
            .iter()
            .flat_map(|f| f.pts.iter().map(move |p| self.slope * f.path(*p)))
            .fold(0.0, f64::max)
    }

    /// Größte Keildicke am Rand, also an der Aufkantung (mm).
    pub fn wedge_edge_max(&self) -> f64 {
        let n = self.outline.len();
        self.faces
            .iter()
            .flat_map(|f| f.pts.iter().map(move |p| (f, *p)))
            .filter(|(_, p)| {
                (0..n).any(|i| dist_seg(*p, self.outline[i], self.outline[(i + 1) % n]) < 0.5)
            })
            .map(|(f, p)| self.slope * f.path(p))
            .fold(0.0, f64::max)
    }

    /// Einzugsfläche je Ablauf (mm²).
    pub fn catchments(&self) -> Vec<f64> {
        let mut out = vec![0.0; self.drains.len()];
        for f in &self.faces {
            if let Some(a) = out.get_mut(f.drain) {
                *a += area(&f.pts);
            }
        }
        out
    }

    /// Länge der Kehlen und der Grate (mm).
    pub fn crease_lengths(&self) -> (f64, f64) {
        self.creases.iter().fold((0.0, 0.0), |acc, c| {
            let l = (c.b - c.a).length();
            if c.valley {
                (acc.0 + l, acc.1)
            } else {
                (acc.0, acc.1 + l)
            }
        })
    }

    /// Fläche je Dickenstufe des Keils: Stufe k reicht von `k · step` bis
    /// `(k + 1) · step` mm (Gefälleplatten im Folgesystem, z. B. 20 mm bei
    /// 2 % und 1 m Platte).
    pub fn bands(&self, step: f64) -> Vec<f64> {
        if step <= 0.0 {
            return Vec::new();
        }
        let n = (self.wedge_max() / step - 1e-6).ceil().max(1.0) as usize;
        let mut out = vec![0.0; n];
        for f in &self.faces {
            // Dicke als Abstand: slope · (c + rise·p) ≥ lo
            for (k, a) in out.iter_mut().enumerate() {
                let (lo, hi) = (k as f64 * step, (k + 1) as f64 * step);
                let s = self.slope;
                let above = Half {
                    n: f.rise,
                    k: f.c - lo / s,
                };
                let below = Half {
                    n: f.rise * -1.0,
                    k: hi / s - f.c,
                };
                let part = clip(&clip(&f.pts, &above), &below);
                *a += area(&part).max(0.0);
            }
        }
        out
    }

    /// Mittlerer Wärmedurchgangskoeffizient nach DIN EN ISO 6946 Anhang C
    /// (W/(m²K)): `r0` Widerstand aller Schichten samt Rsi + Rse an der
    /// dünnsten Stelle (m²K/W), `lambda` des Keils (W/(mK)). Gerechnet als
    /// Flächenmittel von 1/(R0 + d/λ), je Dreieck fein unterteilt.
    pub fn u_value(&self, r0: f64, lambda: f64) -> f64 {
        let a = self.area();
        if a <= 0.0 || r0 <= 0.0 || lambda <= 0.0 {
            return 0.0;
        }
        const N: usize = 12;
        let mut sum = 0.0;
        for f in &self.faces {
            let u = |p: Vec3| 1.0 / (r0 + self.slope * f.path(p) / 1000.0 / lambda);
            // Kantenmitten-Regel (genau für quadratische Verläufe) auf N²
            // Teildreiecken
            let tri = |a: Vec3, b: Vec3, c: Vec3| {
                area(&[a, b, c]) * (u((a + b) * 0.5) + u((b + c) * 0.5) + u((c + a) * 0.5)) / 3.0
            };
            for t in 1..f.pts.len() - 1 {
                let (p0, p1, p2) = (f.pts[0], f.pts[t], f.pts[t + 1]);
                let (e1, e2) = ((p1 - p0) / N as f64, (p2 - p0) / N as f64);
                let at = |i: usize, j: usize| p0 + e1 * i as f64 + e2 * j as f64;
                for i in 0..N {
                    for j in 0..N - i {
                        sum += tri(at(i, j), at(i + 1, j), at(i, j + 1));
                        if i + j + 1 < N {
                            sum += tri(at(i + 1, j), at(i + 1, j + 1), at(i, j + 1));
                        }
                    }
                }
            }
        }
        sum / a
    }
}

fn edge_dist(poly: &[Vec3], p: Vec3) -> f64 {
    let n = poly.len();
    (0..n)
        .map(|i| dist_seg(p, poly[i], poly[(i + 1) % n]))
        .fold(f64::INFINITY, f64::min)
}

/// Schneidet die neue Ebene `face` der Quelle `si` im Bereich `r` (konvex)
/// in die Stücke, wo sie tiefer liegt als die bisherige.
fn lower(pieces: Vec<Piece>, r: &[Vec3], face: &Face, si: usize, src: &[Source]) -> Vec<Piece> {
    let n = r.len();
    let rh: Vec<Half> = (0..n)
        .filter_map(|i| {
            let (a, b) = (r[i], r[(i + 1) % n]);
            ((b - a).length() > EPS)
                .then(|| Half::left_of(a, b - a))
                .flatten()
        })
        .collect();
    let rb = bbox(r);
    let mut out = Vec::with_capacity(pieces.len() + 8);
    for k in pieces {
        let kb = bbox(&k.poly);
        if kb.0 > rb.2 + EPS || kb.2 < rb.0 - EPS || kb.1 > rb.3 + EPS || kb.3 < rb.1 - EPS {
            out.push(k);
            continue;
        }
        let mut inside = k.poly.clone();
        for h in &rh {
            inside = clip(&inside, h);
        }
        if area(&inside) < MIN_AREA {
            out.push(k);
            continue;
        }
        // Wo ist die neue Ebene tiefer?
        let better = match &k.face {
            None => Ok(None),
            Some(old) => {
                let dn = face.rise - old.rise;
                let dc = face.c - old.c;
                if dn.length() < 1e-12 {
                    if dc < -SAME {
                        Ok(None)
                    } else if dc <= SAME
                        && src[si].value == 0.0
                        && src
                            .get(k.src)
                            .is_some_and(|o| o.value == 0.0 && o.drain != face.drain)
                    {
                        // gleiche Ebene zweier Abläufe: Mittelsenkrechte
                        let (a, b) = (src[si].at, src[k.src].at);
                        Half::new(a - b, (a + b) * 0.5).map(Some).ok_or(())
                    } else {
                        Err(())
                    }
                } else {
                    // −(dc + dn·p) ≥ 0: genau auf der Schnittgeraden, damit
                    // kein Streifen der alten Ebene stehen bleibt
                    let l = (dn.x * dn.x + dn.y * dn.y).sqrt();
                    Ok(Some(Half {
                        n: vec3(-dn.x / l, -dn.y / l, 0.0),
                        k: -dc / l,
                    }))
                }
            }
        };
        let Ok(better) = better else {
            out.push(k);
            continue;
        };
        let mut halves = rh.clone();
        halves.extend(better);
        if let Some(b) = better {
            if area(&clip(&inside, &b)) < MIN_AREA {
                out.push(k);
                continue;
            }
        }
        let mut cur = k.poly.clone();
        for h in &halves {
            let rest = clip(&cur, &h.flip());
            if area(&rest) >= MIN_AREA {
                out.push(Piece {
                    poly: rest,
                    face: k.face.clone(),
                    src: k.src,
                });
            }
            cur = clip(&cur, h);
        }
        if area(&cur) >= MIN_AREA {
            out.push(Piece {
                poly: cur,
                face: Some(face.clone()),
                src: si,
            });
        }
    }
    out
}

fn near(a: Vec3, b: Vec3) -> bool {
    (a - b).length() < 0.01
}

/// Fasst Stücke derselben Ebene und desselben Ablaufs mit gemeinsamer Kante
/// zusammen, solange das Ergebnis konvex bleibt.
fn merge(faces: &mut Vec<(Vec<Vec3>, Face)>) {
    loop {
        let mut changed = false;
        let mut i = 0;
        while i < faces.len() {
            let mut j = i + 1;
            while j < faces.len() {
                let same =
                    faces[i].1.drain == faces[j].1.drain && faces[i].1.same_plane(&faces[j].1);
                match same.then(|| union(&faces[i].0, &faces[j].0)).flatten() {
                    Some(u) => {
                        faces[i].0 = u;
                        faces.remove(j);
                        changed = true;
                    }
                    None => j += 1,
                }
            }
            i += 1;
        }
        if !changed {
            break;
        }
    }
}

/// Vereinigung zweier konvexer Polygone mit gemeinsamer Kante, wenn sie
/// konvex ist.
fn union(p: &[Vec3], q: &[Vec3]) -> Option<Vec<Vec3>> {
    let (n, m) = (p.len(), q.len());
    for i in 0..n {
        let (a, b) = (p[i], p[(i + 1) % n]);
        for j in 0..m {
            if !(near(q[j], b) && near(q[(j + 1) % m], a)) {
                continue;
            }
            // p von b bis a, dann q nach a bis vor b
            let mut u: Vec<Vec3> = (0..n).map(|k| p[(i + 1 + k) % n]).collect();
            u.extend((2..m).map(|k| q[(j + k) % m]));
            let u = polygon::simplified(&u);
            let l = u.len();
            let convex = l >= 3
                && (0..l).all(|k| {
                    let (a, b, c) = (u[(k + l - 1) % l], u[k], u[(k + 1) % l]);
                    cross2(b - a, c - b) >= -1e-6 * (b - a).length().max(1.0)
                });
            return convex.then_some(u);
        }
    }
    None
}

/// Kehlen und Grate: Abschnitte, auf denen zwei Teilflächen verschiedener
/// Ebene aneinanderstoßen.
fn creases(faces: &[Face]) -> Vec<Crease> {
    struct Seg {
        a: Vec3,
        d: Vec3,
        len: f64,
        face: usize,
    }
    let mut segs = Vec::new();
    for (fi, f) in faces.iter().enumerate() {
        let n = f.pts.len();
        for i in 0..n {
            let (a, b) = (f.pts[i], f.pts[(i + 1) % n]);
            let len = (b - a).length();
            if len > 0.5 {
                segs.push(Seg {
                    a,
                    d: (b - a) / len,
                    len,
                    face: fi,
                });
            }
        }
    }
    let mut out: Vec<Crease> = Vec::new();
    for i in 0..segs.len() {
        for j in i + 1..segs.len() {
            let (s, t) = (&segs[i], &segs[j]);
            if s.face == t.face || s.d.dot(t.d) > -0.999_999 {
                continue;
            }
            // auf derselben Geraden?
            if cross2(s.d, t.a - s.a).abs() > 0.05 {
                continue;
            }
            let (fa, fb) = (&faces[s.face], &faces[t.face]);
            if fa.same_plane(fb) {
                continue;
            }
            let t0 = (t.a - s.a).dot(s.d);
            let t1 = t0 - t.len;
            let (lo, hi) = (t1.max(0.0), t0.min(s.len));
            if hi - lo < 0.5 {
                continue;
            }
            // Normale aus Fläche a in Fläche b (b liegt rechts von s); die
            // Steigung quer zur Linie nimmt zu: Kehle (konvex), sonst Grat
            let e = vec3(s.d.y, -s.d.x, 0.0);
            let valley = fb.rise.dot(e) - fa.rise.dot(e) > 1e-6;
            out.push(Crease {
                a: flat(s.a + s.d * lo),
                b: flat(s.a + s.d * hi),
                valley,
            });
        }
    }
    join_creases(out)
}

/// Fügt gerade weiterlaufende Abschnitte gleicher Art zusammen.
fn join_creases(mut c: Vec<Crease>) -> Vec<Crease> {
    loop {
        let mut hit = None;
        'o: for i in 0..c.len() {
            for j in 0..c.len() {
                if i == j || c[i].valley != c[j].valley {
                    continue;
                }
                let (a, b) = (c[i], c[j]);
                let d = b.b - b.a;
                let e = a.b - a.a;
                if d.length() < EPS || e.length() < EPS {
                    continue;
                }
                let par = cross2(d.normalized(), e.normalized()).abs() < 1e-6;
                if !par {
                    continue;
                }
                let joined = if near(a.b, b.a) && e.dot(d) > 0.0 {
                    Some((a.a, b.b))
                } else if near(a.b, b.b) && e.dot(d) < 0.0 {
                    Some((a.a, b.a))
                } else {
                    None
                };
                if let Some((p, q)) = joined {
                    hit = Some((i, j, p, q));
                    break 'o;
                }
            }
        }
        let Some((i, j, p, q)) = hit else {
            break;
        };
        c[i].a = p;
        c[i].b = q;
        c.remove(j);
    }
    c
}

// ---- Ablaufvorschlag (Konzept §4.3) ----

/// Grenzen für den Ablaufvorschlag.
#[derive(Clone, Copy, Debug)]
pub struct Limits {
    /// Größter Fließweg (mm).
    pub max_path: f64,
    /// Größte Einzugsfläche je Ablauf (mm²).
    pub max_area: f64,
    pub min_drains: usize,
    /// Abstand von den Ecken (mm).
    pub corner_gap: f64,
}

impl Default for Limits {
    fn default() -> Limits {
        Limits {
            max_path: MAX_PATH,
            max_area: DRAIN_AREA,
            min_drains: MIN_DRAINS,
            corner_gap: CORNER_GAP,
        }
    }
}

/// Mögliche Ablaufstellen: je Kante symmetrisch zur Mitte im Raster, mit
/// Abstand zu den Ecken.
fn candidates(outline: &[Vec3], gap: f64) -> Vec<Vec3> {
    let n = outline.len();
    let mut out = Vec::new();
    for i in 0..n {
        let (a, b) = (outline[i], outline[(i + 1) % n]);
        let l = (b - a).length();
        if l < 2.0 * gap - EPS {
            continue;
        }
        let mut ts = vec![l / 2.0];
        let mut k = 1.0;
        while l / 2.0 - k * CANDIDATE_STEP >= gap - EPS {
            ts.push(l / 2.0 - k * CANDIDATE_STEP);
            ts.push(l / 2.0 + k * CANDIDATE_STEP);
            k += 1.0;
        }
        ts.sort_by(f64::total_cmp);
        out.extend(ts.into_iter().map(|t| a + (b - a) * (t / l)));
    }
    out
}

/// Schneller Prüfer für den Vorschlag: Fließwege auf einem Punktraster.
struct Probe {
    axis: Vec3,
    pts: Vec<Vec3>,
    cell: f64,
    /// Innenecken, dann Kandidaten: Lage und Sichtbarkeit je Rasterpunkt
    /// und je Quelle.
    at: Vec<Vec3>,
    sees_pt: Vec<Vec<bool>>,
    sees_src: Vec<Vec<bool>>,
    corners: usize,
}

/// Bewertung: (Überschuss, größter Weg, mittlerer Weg), kleiner ist besser.
type Score = (f64, f64, f64);

impl Probe {
    fn new(outline: &[Vec3], cands: &[Vec3]) -> Probe {
        let axis = main_axis(outline);
        let mesh = Mesh::new(outline);
        let a = area(outline);
        let h = (a / 900.0).sqrt().max(200.0);
        let (x0, y0, x1, y1) = bbox(outline);
        let mut pts = Vec::new();
        let mut y = y0 + h / 2.0;
        while y < y1 {
            let mut x = x0 + h / 2.0;
            while x < x1 {
                let p = vec3(x, y, 0.0);
                if mesh.tris.iter().any(|t| contains(t, p, 0.0)) {
                    pts.push(p);
                }
                x += h;
            }
            y += h;
        }
        let corners = reflex(outline);
        let nc = corners.len();
        let at: Vec<Vec3> = corners.into_iter().chain(cands.iter().copied()).collect();
        let vis: Vec<Vec<Vec<Vec3>>> = at.iter().map(|s| mesh.visible(*s)).collect();
        let hit = |k: usize, p: Vec3| vis[k].iter().any(|piece| contains(piece, p, 1e-3));
        let sees_pt = (0..at.len())
            .map(|k| pts.iter().map(|p| hit(k, *p)).collect())
            .collect();
        let sees_src = (0..at.len())
            .map(|k| at.iter().map(|q| hit(k, *q)).collect())
            .collect();
        Probe {
            axis,
            cell: area(outline) / pts.len().max(1) as f64,
            pts,
            at,
            sees_pt,
            sees_src,
            corners: nc,
        }
    }

    /// Fließweg und Ablauf je Rasterpunkt für die Abläufe `drains`
    /// (Indizes der Kandidaten).
    fn paths(&self, drains: &[usize]) -> Vec<(f64, usize)> {
        // Quellen: Abläufe, dann Innenecken
        let mut nodes: Vec<(usize, f64, usize)> = drains
            .iter()
            .enumerate()
            .map(|(i, &c)| (self.corners + c, 0.0, i))
            .chain((0..self.corners).map(|k| (k, f64::INFINITY, usize::MAX)))
            .collect();
        let n = nodes.len();
        let mut done = vec![false; n];
        loop {
            let next = (0..n)
                .filter(|&i| !done[i] && nodes[i].1.is_finite())
                .min_by(|&a, &b| nodes[a].1.total_cmp(&nodes[b].1));
            let Some(i) = next else {
                break;
            };
            done[i] = true;
            for j in 0..n {
                if done[j] || !self.sees_src[nodes[i].0][nodes[j].0] {
                    continue;
                }
                let v = nodes[i].1 + linf(self.axis, self.at[nodes[i].0], self.at[nodes[j].0]);
                if v < nodes[j].1 {
                    nodes[j].1 = v;
                    nodes[j].2 = nodes[i].2;
                }
            }
        }
        self.pts
            .iter()
            .enumerate()
            .map(|(pi, p)| {
                let mut best = (f64::INFINITY, usize::MAX, f64::INFINITY);
                for &(k, v, d) in &nodes {
                    if !v.is_finite() || !self.sees_pt[k][pi] {
                        continue;
                    }
                    let w = v + linf(self.axis, self.at[k], *p);
                    let eu = v + (self.at[k] - *p).length();
                    if w < best.0 - SAME || (w < best.0 + SAME && eu < best.2) {
                        best = (w, d, eu);
                    }
                }
                (best.0, best.1)
            })
            .collect()
    }

    fn score(&self, drains: &[usize], lim: &Limits) -> Score {
        let w = self.paths(drains);
        if w.is_empty() {
            return (0.0, 0.0, 0.0);
        }
        let mut catch = vec![0.0; drains.len()];
        let (mut over, mut max, mut sum) = (0.0, 0.0f64, 0.0);
        for &(d, i) in &w {
            if !d.is_finite() {
                over += 1e12;
                continue;
            }
            over += (d - lim.max_path).max(0.0) * self.cell;
            max = max.max(d);
            sum += d;
            if let Some(c) = catch.get_mut(i) {
                *c += self.cell;
            }
        }
        // Einzugsfläche in Fließweg-Fläche umgerechnet: 1 m² zu viel wie 1 m
        // zu weit auf 1 m²
        over += catch
            .iter()
            .map(|a| (a - lim.max_area).max(0.0) * 1000.0)
            .sum::<f64>();
        (over, max, sum / w.len() as f64)
    }
}

fn better(a: Score, b: Score) -> bool {
    if (a.0 - b.0).abs() > 1.0 {
        return a.0 < b.0;
    }
    if (a.1 - b.1).abs() > 1.0 {
        return a.1 < b.1;
    }
    a.2 < b.2 - 1.0
}

/// Vorschlag für Attikaabläufe auf dem Umriss (Konzept §4.3): so lange
/// Abläufe dazu, wo sie den größten Überschuss über Fließweg und
/// Einzugsfläche abbauen; überzählige wieder weg; dann jeden Ablauf entlang
/// der Kante verschieben, bis größter und mittlerer Weg am kleinsten sind.
pub fn propose_drains(outline: &[Vec3], lim: &Limits) -> Vec<Vec3> {
    let outline = polygon::to_ccw(&polygon::simplified(outline));
    if outline.len() < 3 || area(&outline) < MIN_AREA {
        return Vec::new();
    }
    let mut cands = candidates(&outline, lim.corner_gap);
    if cands.is_empty() {
        // zu klein für den Eckabstand: Mitte der längsten Kante
        let n = outline.len();
        let i = (0..n)
            .max_by(|&a, &b| {
                let la = (outline[(a + 1) % n] - outline[a]).length();
                let lb = (outline[(b + 1) % n] - outline[b]).length();
                la.total_cmp(&lb)
            })
            .unwrap_or(0);
        cands.push((outline[i] + outline[(i + 1) % n]) * 0.5);
    }
    let probe = Probe::new(&outline, &cands);
    let free = |chosen: &[usize], c: usize| {
        chosen
            .iter()
            .all(|&d| linf(probe.axis, cands[d], cands[c]) >= 1000.0)
    };
    let mut drains: Vec<usize> = Vec::new();
    let ok = |s: Score| s.0 <= 1e-6;
    while drains.len() < 12 && (drains.len() < lim.min_drains || !ok(probe.score(&drains, lim))) {
        let mut best: Option<(usize, Score)> = None;
        for c in 0..cands.len() {
            if !free(&drains, c) {
                continue;
            }
            let mut t = drains.clone();
            t.push(c);
            let s = probe.score(&t, lim);
            if best.is_none_or(|b| better(s, b.1)) {
                best = Some((c, s));
            }
        }
        let Some((c, _)) = best else {
            break;
        };
        drains.push(c);
    }
    // überzählige weg
    let mut changed = true;
    while changed && drains.len() > lim.min_drains {
        changed = false;
        for i in 0..drains.len() {
            let mut t = drains.clone();
            t.remove(i);
            if ok(probe.score(&t, lim)) {
                drains = t;
                changed = true;
                break;
            }
        }
    }
    // verschieben
    for _ in 0..2 {
        for i in 0..drains.len() {
            let mut base = probe.score(&drains, lim);
            for c in 0..cands.len() {
                if c == drains[i] || linf(probe.axis, cands[c], cands[drains[i]]) > 3000.0 {
                    continue;
                }
                let others: Vec<usize> = drains
                    .iter()
                    .enumerate()
                    .filter(|(k, _)| *k != i)
                    .map(|(_, d)| *d)
                    .collect();
                if !free(&others, c) {
                    continue;
                }
                let mut t = drains.clone();
                t[i] = c;
                let s = probe.score(&t, lim);
                if better(s, base) {
                    base = s;
                    drains = t;
                }
            }
        }
    }
    drains.into_iter().map(|c| cands[c]).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn poly(p: &[(f64, f64)]) -> Vec<Vec3> {
        p.iter()
            .map(|&(x, y)| vec3(x * 1000.0, y * 1000.0, 0.0))
            .collect()
    }

    fn pt(x: f64, y: f64) -> Vec3 {
        vec3(x * 1000.0, y * 1000.0, 0.0)
    }

    /// Überall gleich: Jede Ecke einer Teilfläche hat in allen Flächen, die
    /// sie berühren, denselben Fließweg (die Hülle ist stetig).
    fn stetig(f: &SlopeField) {
        for a in &f.faces {
            for p in &a.pts {
                for b in &f.faces {
                    if contains(&b.pts, *p, 0.01) {
                        let (wa, wb) = (a.path(*p), b.path(*p));
                        assert!((wa - wb).abs() < 0.5, "Sprung {wa} / {wb} bei {p:?}");
                    }
                }
            }
        }
    }

    /// Jede Fläche fällt mit dem Gefälle parallel zu einer Achse, und der
    /// Fließweg ist nie kleiner als der gerade Abstand zu ihrem Ablauf.
    fn achsen(f: &SlopeField) {
        let v = left(f.axis);
        for a in &f.faces {
            let r = a.rise;
            assert!(
                [f.axis, f.axis * -1.0, v, v * -1.0]
                    .iter()
                    .any(|d| (*d - r).length() < 1e-9),
                "Fallrichtung {r:?}"
            );
            for p in &a.pts {
                let d = linf(f.axis, f.drains[a.drain], *p);
                assert!(a.path(*p) >= d - 0.5, "kürzer als gerade: {p:?}");
            }
        }
    }

    #[test]
    fn rechteck_raute_mit_grat() {
        let o = poly(&[(0.0, 0.0), (16.0, 0.0), (16.0, 10.0), (0.0, 10.0)]);
        let f = SlopeField::compute(&o, &[pt(8.0, 0.0), pt(8.0, 10.0)], 2.0).unwrap();
        stetig(&f);
        achsen(&f);
        assert!((f.area() - 160.0e6).abs() < 1e3);
        assert!((f.wedge_max() - 160.0).abs() < 0.01, "{}", f.wedge_max());
        assert!((f.wedge_edge_max() - 160.0).abs() < 0.01);
        // Mittelwert analytisch: 180,833 m³·m / 40 m² je Viertel → 4,5208 m
        assert!(
            (f.wedge_mean() - 90.4167).abs() < 0.05,
            "{}",
            f.wedge_mean()
        );
        let (kehle, grat) = f.crease_lengths();
        assert!(
            (kehle - 4.0 * 5000.0 * 2f64.sqrt()).abs() < 1.0,
            "Kehlen {kehle} {:#?}",
            f.creases
        );
        assert!((grat - 10000.0).abs() < 1.0, "Grat {grat}");
        let c = f.catchments();
        assert!(
            (c[0] - 80.0e6).abs() < 1e4 && (c[1] - 80.0e6).abs() < 1e4,
            "{c:?}"
        );
        // Kehlen gehen vom Ablauf aus
        assert!(f.creases.iter().filter(|c| c.valley).all(|c| [c.a, c.b]
            .iter()
            .any(|p| f.drains.iter().any(|d| (*d - *p).length() < 1.0))));
    }

    #[test]
    fn l_form_um_die_innenecke() {
        let o = poly(&[
            (0.0, 0.0),
            (16.0, 0.0),
            (16.0, 7.0),
            (8.0, 7.0),
            (8.0, 14.0),
            (0.0, 14.0),
        ]);
        // nur an den Schenkelenden: das Wasser läuft um die Ecke
        let f = SlopeField::compute(&o, &[pt(4.0, 14.0), pt(16.0, 3.5)], 2.0).unwrap();
        stetig(&f);
        achsen(&f);
        // weitester Punkt (0, 0): zum Ablauf oben 14 m gerade
        assert!((f.wedge_max() - 280.0).abs() < 0.5, "{}", f.wedge_max());
        // mit Ablauf neben der Innenecke
        let g = SlopeField::compute(&o, &[pt(8.0, 7.6), pt(10.5, 0.0)], 2.0).unwrap();
        stetig(&g);
        achsen(&g);
        assert!(
            g.wedge_max() < 165.0 && g.wedge_max() > 150.0,
            "{}",
            g.wedge_max()
        );
        assert!((g.area() - 168.0e6).abs() < 1e3);
        let sum: f64 = g.catchments().iter().sum();
        assert!((sum - g.area()).abs() < 1e3);
    }

    #[test]
    fn hinter_der_ecke_um_die_ecke() {
        // Ablauf im unteren Schenkel, oberer Schenkel liegt im Schatten der
        // Innenecke (8, 7): Weg über die Ecke
        let o = poly(&[
            (0.0, 0.0),
            (16.0, 0.0),
            (16.0, 7.0),
            (8.0, 7.0),
            (8.0, 14.0),
            (0.0, 14.0),
        ]);
        let f = SlopeField::compute(&o, &[pt(16.0, 3.5)], 2.0).unwrap();
        stetig(&f);
        // (8, 14) ist von (16, 3.5) aus verdeckt: über (8, 7) sind es 8 + 7
        let w = f.wedge_at(pt(8.0, 14.0) - vec3(1.0, 1.0, 0.0));
        assert!((w - 0.02 * 15000.0).abs() < 0.1, "{w}");
        // (0, 14) sieht die Ecke (8, 7): 8 + max(8, 7)
        let w = f.wedge_at(pt(0.0, 14.0) + vec3(1.0, -1.0, 0.0));
        assert!((w - 0.02 * 16000.0).abs() < 0.1, "{w}");
    }

    #[test]
    fn stufen_und_u_wert() {
        let o = poly(&[(0.0, 0.0), (16.0, 0.0), (16.0, 10.0), (0.0, 10.0)]);
        let f = SlopeField::compute(&o, &[pt(8.0, 0.0), pt(8.0, 10.0)], 2.0).unwrap();
        let b = f.bands(20.0);
        assert_eq!(b.len(), 8, "{}", f.wedge_max());
        let sum: f64 = b.iter().sum();
        assert!((sum - f.area()).abs() < 1e3);
        // Stufe 0: bis 1 m Weg um jeden Ablauf ein Rechteck 2 × 1 m
        assert!((b[0] - 4.0e6).abs() < 1e3, "{}", b[0]);
        // U-Wert liegt zwischen dem an der dünnsten und dem mit mittlerer Dicke
        let (r0, lam) = (4.8, 0.035);
        let u = f.u_value(r0, lam);
        let u_mitte = 1.0 / (r0 + f.wedge_mean() / 1000.0 / lam);
        assert!(u < 1.0 / r0 && u > u_mitte, "{u}");
    }

    #[test]
    fn u_wert_rechteck_nach_anhang_c() {
        // Eine Rechteckfläche 10 × 1 m, 0 … 200 mm: U = ln(1 + R1/R0) / R1
        let f = SlopeField {
            slope: 0.02,
            axis: Vec3::X,
            drains: vec![Vec3::ZERO],
            faces: vec![Face {
                pts: poly(&[(0.0, 0.0), (10.0, 0.0), (10.0, 1.0), (0.0, 1.0)]),
                rise: Vec3::X,
                c: 0.0,
                drain: 0,
            }],
            creases: Vec::new(),
            outline: Vec::new(),
        };
        let (r0, lam): (f64, f64) = (4.8, 0.035);
        let r1 = 0.2 / lam;
        let soll = (1.0 + r1 / r0).ln() / r1;
        assert!((f.u_value(r0, lam) - soll).abs() < 1e-5);
    }

    #[test]
    fn vorschlag_rechteck_mitte_der_langseiten() {
        let o = poly(&[(0.0, 0.0), (16.0, 0.0), (16.0, 10.0), (0.0, 10.0)]);
        let mut d = propose_drains(&o, &Limits::default());
        d.sort_by(|a, b| a.y.total_cmp(&b.y));
        assert_eq!(d.len(), 2, "{d:?}");
        assert!((d[0] - pt(8.0, 0.0)).length() < 1.0, "{d:?}");
        assert!((d[1] - pt(8.0, 10.0)).length() < 1.0, "{d:?}");
    }

    #[test]
    fn vorschlag_l_form_an_der_innenecke() {
        let o = poly(&[
            (0.0, 0.0),
            (16.0, 0.0),
            (16.0, 7.0),
            (8.0, 7.0),
            (8.0, 14.0),
            (0.0, 14.0),
        ]);
        let d = propose_drains(&o, &Limits::default());
        assert_eq!(d.len(), 2, "{d:?}");
        // einer höchstens 1 m neben der Innenecke
        assert!(
            d.iter().any(|p| (*p - pt(8.0, 7.0)).length() <= 1000.0),
            "{d:?}"
        );
        let f = SlopeField::compute(&o, &d, 2.0).unwrap();
        assert!(f.wedge_max() <= 160.0 + 1.0, "{}", f.wedge_max());
    }

    #[test]
    fn vorschlag_u_form_symmetrisch() {
        let o = poly(&[
            (0.0, 0.0),
            (20.0, 0.0),
            (20.0, 14.0),
            (14.0, 14.0),
            (14.0, 6.0),
            (6.0, 6.0),
            (6.0, 14.0),
            (0.0, 14.0),
        ]);
        let d = propose_drains(&o, &Limits::default());
        assert_eq!(d.len(), 3, "{d:?}");
        let f = SlopeField::compute(&o, &d, 2.0).unwrap();
        stetig(&f);
        achsen(&f);
        assert!(f.wedge_max() <= 160.0 + 1.0);
    }

    #[test]
    fn schraege_kante() {
        let o = poly(&[
            (0.0, 0.0),
            (12.0, 0.0),
            (12.0, 4.0),
            (18.0, 4.0),
            (18.0, 12.0),
            (9.0, 12.0),
            (4.0, 15.0),
            (0.0, 15.0),
        ]);
        let d = propose_drains(&o, &Limits::default());
        let f = SlopeField::compute(&o, &d, 2.0).unwrap();
        stetig(&f);
        achsen(&f);
        assert!((f.area() - area(&o)).abs() < 1e3);
        assert!(f.wedge_max() <= 160.0 + 1.0, "{}", f.wedge_max());
    }

    #[test]
    fn ohne_ablauf_oder_gefaelle_nichts() {
        let o = poly(&[(0.0, 0.0), (4.0, 0.0), (4.0, 4.0), (0.0, 4.0)]);
        assert!(SlopeField::compute(&o, &[], 2.0).is_none());
        assert!(SlopeField::compute(&o, &[pt(2.0, 0.0)], 0.0).is_none());
    }
}

/// Schreibt Gefällepläne als JSON (Sichtprüfung gegen den Prototyp):
/// `SKIZZEO_GEFAELLE=<ordner> cargo test -p sk-model gefaelle_json -- --ignored`.
#[cfg(test)]
mod json {
    use super::*;

    fn dump(name: &str, outline: &[Vec3]) {
        let Ok(dir) = std::env::var("SKIZZEO_GEFAELLE") else {
            return;
        };
        let d = propose_drains(outline, &Limits::default());
        let f = SlopeField::compute(outline, &d, 2.0).unwrap();
        let pts = |v: &[Vec3]| {
            v.iter()
                .map(|p| format!("[{:.1},{:.1}]", p.x, p.y))
                .collect::<Vec<_>>()
                .join(",")
        };
        let faces: Vec<String> = f
            .faces
            .iter()
            .map(|x| {
                format!(
                    "{{\"pts\":[{}],\"rise\":[{:.3},{:.3}],\"c\":{:.3},\"drain\":{}}}",
                    pts(&x.pts),
                    x.rise.x,
                    x.rise.y,
                    x.c,
                    x.drain
                )
            })
            .collect();
        let cr: Vec<String> = f
            .creases
            .iter()
            .map(|c| {
                format!(
                    "{{\"a\":[{:.1},{:.1}],\"b\":[{:.1},{:.1}],\"valley\":{}}}",
                    c.a.x, c.a.y, c.b.x, c.b.y, c.valley
                )
            })
            .collect();
        let (k, g) = f.crease_lengths();
        let s = format!(
            "{{\"outline\":[{}],\"drains\":[{}],\"faces\":[{}],\"creases\":[{}],\"mean\":{:.1},\"max\":{:.1},\"edge\":{:.1},\"catch\":{:?},\"kehle\":{:.0},\"grat\":{:.0},\"bands\":{:?}}}",
            pts(f.outline()),
            pts(&f.drains),
            faces.join(","),
            cr.join(","),
            f.wedge_mean(),
            f.wedge_max(),
            f.wedge_edge_max(),
            f.catchments().iter().map(|a| (a / 1e4).round() / 100.0).collect::<Vec<_>>(),
            k,
            g,
            f.bands(20.0).iter().map(|a| (a / 1e4).round() / 100.0).collect::<Vec<_>>()
        );
        std::fs::write(format!("{dir}/{name}.json"), s).unwrap();
    }

    fn poly(p: &[(f64, f64)]) -> Vec<Vec3> {
        p.iter()
            .map(|&(x, y)| vec3(x * 1000.0, y * 1000.0, 0.0))
            .collect()
    }

    #[test]
    #[ignore]
    fn gefaelle_json() {
        dump(
            "1-rechteck",
            &poly(&[(0.0, 0.0), (16.0, 0.0), (16.0, 10.0), (0.0, 10.0)]),
        );
        dump(
            "2-l-form",
            &poly(&[
                (0.0, 0.0),
                (16.0, 0.0),
                (16.0, 7.0),
                (8.0, 7.0),
                (8.0, 14.0),
                (0.0, 14.0),
            ]),
        );
        dump(
            "3-u-form",
            &poly(&[
                (0.0, 0.0),
                (20.0, 0.0),
                (20.0, 14.0),
                (14.0, 14.0),
                (14.0, 6.0),
                (6.0, 6.0),
                (6.0, 14.0),
                (0.0, 14.0),
            ]),
        );
        dump(
            "4-schraeg",
            &poly(&[
                (0.0, 0.0),
                (12.0, 0.0),
                (12.0, 4.0),
                (18.0, 4.0),
                (18.0, 12.0),
                (9.0, 12.0),
                (4.0, 15.0),
                (0.0, 15.0),
            ]),
        );
        dump(
            "5-t-form",
            &poly(&[
                (0.0, 6.0),
                (6.0, 6.0),
                (6.0, 0.0),
                (12.0, 0.0),
                (12.0, 6.0),
                (18.0, 6.0),
                (18.0, 12.0),
                (0.0, 12.0),
            ]),
        );
        dump(
            "6-gedreht",
            &[
                vec3(0.0, 0.0, 0.0),
                vec3(12000.0, 5000.0, 0.0),
                vec3(9000.0, 12200.0, 0.0),
                vec3(-3000.0, 7200.0, 0.0),
            ],
        );
    }
}
