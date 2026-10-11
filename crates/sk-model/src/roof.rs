//! Flachdach: waagerechter Dachaufbau zwischen den Aufkantungen und
//! Attikablech auf ihrer Krone (Jörn 10.10., Plan Flachdach D3, D4).
//!
//! Der Aufbau liegt auf der obersten Rohdecke und füllt die Innenfläche der
//! Aufkantung; seine Schichten stehen von oben nach unten im Typ. Das Blech
//! läuft als geschlossener Ring auf der Außenfläche der Aufkantung über die
//! ganze Wanddicke und zählt nicht zur Höhe der Ebene.

use crate::gefaelle::SlopeField;
use crate::solid::{at_z, edge_kind, material, right_of, SectionFrame, Solid, SweepEnd, NO_LAYER};
use crate::terrace::{
    attika_solid, coping_girth_legs, coping_profile_legs, crown_section_caps, AttikaPiece,
    COPING_OUTER, ROOF_COPING_INNER,
};
use sk_math::{polygon, vec3, Vec3};

/// Dachaufbau und Attikablech über einer Aufkantung.
#[derive(Clone, Debug, PartialEq)]
pub struct FlatRoof {
    /// Innenfläche der Aufkantung, gegen den Uhrzeigersinn, auf z = 0.
    pub outline: Vec<Vec3>,
    /// OK Rohdecke (absolut).
    pub base: f64,
    /// Schichten von oben nach unten: Dicke (mm), Baustoff
    /// (Darstellungsschlüssel), Dämmschicht (Schraffur längs).
    pub layers: Vec<(f64, u16, bool)>,
    /// Außenfläche der Aufkantung, gegen den Uhrzeigersinn (außen liegt
    /// rechts), auf z = 0: Pfad des Blechs.
    pub ring: Vec<Vec3>,
    /// Dicke der Aufkantung, zugleich Breite des Blechs (mm).
    pub width: f64,
    /// OK Aufkantung (absolut), Unterseite des Blechs.
    pub crown: f64,
    /// Baustoff des Blechs (Darstellungsschlüssel).
    pub coping_mat: u16,
    /// Gefälleplan, wenn das Dach Gefälle hat: der Keil liegt in der
    /// Dämmschicht `tapered` und hebt die Schichten darüber an.
    pub slope: Option<SlopeField>,
    /// Gefälledämmschicht (von oben gezählt), nur mit `slope`.
    pub tapered: Option<usize>,
    /// Attikadämmung innen (Jörn 10.10. 16:07): Dicke (mm, 0 = keine) und
    /// ihre Innenfläche, gegen den Uhrzeigersinn, Ecke für Ecke zu
    /// `outline`, auf z = 0 (ohne Dämmung gleich `outline`). Sie steht auf
    /// der Dachhaut und reicht bis OK Aufkantung.
    pub inner: f64,
    pub inset: Vec<Vec3>,
    /// Dicke, um die `inset` tatsächlich einrückt (mm): `inner`, nur wo der
    /// Umriss stellenweise schmaler als 2 × `inner` ist, weniger.
    pub inner_depth: f64,
    /// Dämmstreifen auf der Krone (mm, 0 = keiner) über Aufkantung und
    /// Attikadämmung innen; das Blech liegt um ihn höher.
    pub cap: f64,
    /// Baustoff der Attikadämmung innen (Darstellungsschlüssel).
    pub ins_mat: u16,
    /// Baustoff des Dämmstreifens auf der Krone (Darstellungsschlüssel):
    /// eigener Baustoff, damit ihn das LV von innen trennt (W9 an #60).
    pub cap_mat: u16,
    /// Regenfallrohre außen, eins je Ablauf in der Reihenfolge der Abläufe
    /// (Jörn 11.10., [`crate::fallrohr`]).
    pub downpipes: Vec<crate::fallrohr::Downpipe>,
}

/// Konvexes `a` geschnitten mit konvexem `b` (beide gegen den
/// Uhrzeigersinn, z = 0), Sutherland–Hodgman.
fn clip_convex(a: &[Vec3], b: &[Vec3]) -> Vec<Vec3> {
    let mut out = a.to_vec();
    let n = b.len();
    for k in 0..n {
        let (p, q) = (b[k], b[(k + 1) % n]);
        let d = q - p;
        out = clip_half(&out, |x| d.x * (x.y - p.y) - d.y * (x.x - p.x));
        if out.is_empty() {
            break;
        }
    }
    out
}

/// Teil des Polygons, auf dem die lineare Funktion `f` nicht negativ ist.
fn clip_half(a: &[Vec3], f: impl Fn(Vec3) -> f64) -> Vec<Vec3> {
    let n = a.len();
    let mut out = Vec::with_capacity(n + 2);
    for k in 0..n {
        let (p, q) = (a[k], a[(k + 1) % n]);
        let (fp, fq) = (f(p), f(q));
        if fp >= 0.0 {
            out.push(p);
        }
        if (fp >= 0.0) != (fq >= 0.0) {
            out.push(p + (q - p) * (fp / (fp - fq)));
        }
    }
    out
}

/// Anteile `ta < tb` der Strecke `a`–`b`, die im konvexen Polygon `poly`
/// (gegen den Uhrzeigersinn) liegen; `None`, wenn kein Stück mit Länge.
fn segment_in(poly: &[Vec3], a: Vec3, b: Vec3) -> Option<(f64, f64)> {
    let (mut lo, mut hi) = (0.0_f64, 1.0_f64);
    let d = b - a;
    let n = poly.len();
    for k in 0..n {
        let (p, q) = (poly[k], poly[(k + 1) % n]);
        let e = q - p;
        // links von p→q: e × (x − p) ≥ 0, linear in t
        let f0 = e.x * (a.y - p.y) - e.y * (a.x - p.x);
        let df = e.x * d.y - e.y * d.x;
        if df.abs() < 1e-12 {
            if f0 < 0.0 {
                return None;
            }
            continue;
        }
        let t = -f0 / df;
        if df > 0.0 {
            lo = lo.max(t);
        } else {
            hi = hi.min(t);
        }
    }
    ((hi - lo) * d.length() > 1e-6).then_some((lo, hi))
}

impl FlatRoof {
    /// Dicke des Aufbaus (mm).
    pub fn thickness(&self) -> f64 {
        self.layers.iter().map(|l| l.0).sum()
    }

    /// Höhenband des Aufbaus (OK Rohdecke, OK Dachhaut).
    pub fn band(&self) -> (f64, f64) {
        (self.base, self.base + self.thickness())
    }

    /// Anschlusshöhe: OK Aufkantung über OK Dachhaut (mm), mit Gefälle am
    /// höchsten Punkt des Rands.
    pub fn upstand(&self) -> f64 {
        self.crown - self.band().1 - self.slope.as_ref().map_or(0.0, |g| g.wedge_edge_max())
    }

    /// Höchster Punkt der Dachhaut (absolut, mm): OK Dachhaut plus Keil.
    pub fn top_max(&self) -> f64 {
        self.band().1 + self.slope.as_ref().map_or(0.0, |g| g.wedge_max())
    }

    /// Keilplatten je Plattentyp (Stück, Konzept F4): Typ k deckt die
    /// Keildicke von k bis k + 1 Stufen (Stufe = Gefälle × 1 m), je
    /// angefangener Quadratmeter seiner Fläche eine Platte. Leer ohne
    /// Gefälle.
    pub fn plates(&self) -> Vec<usize> {
        let Some(g) = &self.slope else {
            return Vec::new();
        };
        g.bands(g.slope * PLATE_LENGTH)
            .iter()
            .map(|a| (a / 1e6 - 1e-9).ceil().max(0.0) as usize)
            .collect()
    }

    /// Keildicke am Punkt `p` (mm), 0 ohne Gefälle.
    pub fn wedge_at(&self, p: Vec3) -> f64 {
        self.slope.as_ref().map_or(0.0, |g| g.wedge_at(p))
    }

    /// Steigen Unter- und Oberseite der Schicht `li` (von oben gezählt) mit
    /// dem Keil? Die Gefälleschicht hat eine ebene Unterseite, alle
    /// Schichten darüber liegen auf dem Keil.
    fn rises(&self, li: u8) -> (bool, bool) {
        match (self.tapered, &self.slope) {
            (Some(t), Some(_)) => ((li as usize) < t, (li as usize) <= t),
            _ => (false, false),
        }
    }

    /// Umriss mit den Knicken des Keils am Rand: Punkte (z = 0) und
    /// Keildicke; ohne Gefälle der Umriss mit 0.
    fn rim(&self) -> Vec<(Vec3, f64)> {
        let ring = &self.outline;
        let n = ring.len();
        let mut out = Vec::new();
        for i in 0..n {
            let (a, b) = (ring[i], ring[(i + 1) % n]);
            let Some(ts) = self.breaks(a, b) else {
                continue;
            };
            out.extend(ts[..ts.len() - 1].iter().map(|&t| {
                let p = a + (b - a) * t;
                (p, self.wedge_at(p))
            }));
        }
        out
    }

    /// Knicke des Keils auf der Strecke `a`–`b` des Umrisses als Anteile
    /// 0 … 1 (mit 0 und 1); `None` für eine Strecke ohne Länge.
    fn breaks(&self, a: Vec3, b: Vec3) -> Option<Vec<f64>> {
        let d = b - a;
        let len = d.length();
        if len < 1e-6 {
            return None;
        }
        let mut ts: Vec<f64> = self.slope.as_ref().map_or_else(Vec::new, |g| {
            g.faces
                .iter()
                .flat_map(|f| f.pts.iter())
                .filter_map(|p| {
                    let t = (*p - a).dot(d) / (len * len);
                    let q = a + d * t;
                    let on = vec3(p.x - q.x, p.y - q.y, 0.0).length() < 0.5;
                    (on && t * len > 0.5 && t * len < len - 0.5).then_some(t)
                })
                .collect()
        });
        ts.push(0.0);
        ts.sort_by(f64::total_cmp);
        ts.dedup_by(|x, y| (*x - *y) * len < 0.5);
        ts.push(1.0);
        Some(ts)
    }

    /// Fläche des Aufbaus (mm²).
    pub fn area(&self) -> f64 {
        polygon::area(&self.outline)
    }

    /// Volumen je Schicht von oben nach unten (mm³).
    pub fn volumes(&self) -> Vec<f64> {
        let a = self.area();
        self.layers.iter().map(|l| l.0 * a).collect()
    }

    /// Anschlusslänge an der Innenfläche der Aufkantung (mm).
    pub fn edge_length(&self) -> f64 {
        polygon::perimeter(&self.outline)
    }

    /// Innenecken des Anschlusses an der Aufkantung (Stück): Knicke der
    /// Innenfläche ab 5°, an denen die Dachfläche konvex ist. Einspringende
    /// Ecken (Außenecken des Anschlusses) und gerade Zwischenpunkte zählen
    /// nicht (Review fe0e1ed).
    pub fn corners(&self) -> usize {
        let p = &self.outline;
        let n = p.len();
        if n < 3 {
            return 0;
        }
        // Umlaufsinn aus der vorzeichenbehafteten Fläche
        let sinn: f64 = (0..n)
            .map(|k| {
                let (a, b) = (p[k], p[(k + 1) % n]);
                a.x * b.y - b.x * a.y
            })
            .sum();
        let grenze = 5.0f64.to_radians().cos();
        (0..n)
            .filter(|&k| {
                let (a, b, c) = (p[(k + n - 1) % n], p[k], p[(k + 1) % n]);
                let (u, v) = ((b - a).normalized(), (c - b).normalized());
                let kreuz = u.x * v.y - u.y * v.x;
                u.dot(v) < grenze && kreuz * sinn > 0.0
            })
            .count()
    }

    /// Schichten mit Höhen (UK, OK, Baustoff, Dämmung, Schicht von oben
    /// gezählt), von unten nach oben.
    fn bands(&self) -> Vec<(f64, f64, u16, bool, u8)> {
        let mut z = self.base;
        self.layers
            .iter()
            .enumerate()
            .rev()
            .map(|(i, &(d, mat, ins))| {
                z += d;
                (z - d, z, mat, ins, i as u8)
            })
            .collect()
    }

    /// Körper für 3D und Ansichten: Oberfläche der Dachhaut, Unterseite auf
    /// der Rohdecke und Seiten je Schicht an der Aufkantung, ohne Kanten
    /// der Schichtfugen (wie die Dachterrasse).
    pub fn solid(&self) -> Solid {
        let mut s = self.below(f64::INFINITY);
        let bands = self.bands();
        if bands.is_empty() {
            return s;
        }
        let top_mat = s.mat;
        s.layer = bands.first().map_or(NO_LAYER, |l| l.4);
        s.cap(&self.outline, self.base, false);
        let rim = self.rim();
        let m = rim.len();
        for &(a, b, mat, _, li) in &bands {
            (s.mat, s.layer) = (mat, li);
            let (ra, rb) = self.rises(li);
            let z = |up: bool, z: f64, w: f64| if up { z + w } else { z };
            // Seiten an der Aufkantung, mit Gefälle stückweise schräg oben
            for i in 0..m {
                let ((p, wp), (q, wq)) = (rim[i], rim[(i + 1) % m]);
                let r = right_of((q - p).normalized());
                s.quad(
                    at_z(p, z(ra, a, wp)),
                    at_z(q, z(ra, a, wq)),
                    at_z(q, z(rb, b, wq)),
                    at_z(p, z(rb, b, wp)),
                    r,
                );
            }
        }
        s.mat = top_mat;
        s
    }

    /// Aufbau waagerecht geschnitten in Höhe `cut` (Grundriss).
    pub fn cut_at(&self, cut: f64) -> Solid {
        let mut s = self.below(cut);
        if let Some(g) = &self.slope {
            self.plan_symbols(&mut s, g, cut);
        }
        s
    }

    /// Zeichen des Gefälleplans im Grundriss (G4), feine Linien auf der
    /// Dachhaut: je Teilfläche ein Pfeil in Fließrichtung, je Ablauf ein
    /// Kreis mit dem Rohr durch die Aufkantung und daneben der
    /// Notüberlauf als Rechteck in der Aufkantung.
    fn plan_symbols(&self, s: &mut Solid, g: &SlopeField, cut: f64) {
        let top = self.band().1;
        let at = |p: Vec3| at_z(p, (top + self.wedge_at(p)).min(cut) + 1.0);
        s.edge_kind = edge_kind::FINE;
        for f in &g.faces {
            let a = polygon::area(&f.pts);
            let len = (a.sqrt() * 0.6).min(ARROW);
            if len < ARROW / 3.0 {
                continue;
            }
            let Some(c) = polygon::centroid(&f.pts) else {
                continue;
            };
            let d = vec3(-f.rise.x, -f.rise.y, 0.0);
            let (p, q) = (c - d * (len / 2.0), c + d * (len / 2.0));
            let side = right_of(d) * (ARROW_HEAD * 0.4);
            let back = q - d * ARROW_HEAD;
            s.edge(at(p), at(q));
            s.edge(at(q), at(back + side));
            s.edge(at(q), at(back - side));
        }
        let n = self.outline.len();
        for &dr in &g.drains {
            // Kante der Aufkantung, an der der Ablauf liegt
            let Some((a, b)) = (0..n)
                .map(|i| (self.outline[i], self.outline[(i + 1) % n]))
                .min_by(|x, y| {
                    let dist = |(a, b): (Vec3, Vec3)| {
                        sk_math::dist_to_segment((dr.x, dr.y), (a.x, a.y), (b.x, b.y))
                    };
                    dist(*x).total_cmp(&dist(*y))
                })
            else {
                continue;
            };
            let d = (b - a).normalized();
            let out = right_of(d);
            let z = (top + self.wedge_at(dr)).min(cut) + 1.0;
            let ring = |c: Vec3, r: f64| -> Vec<Vec3> {
                (0..=16)
                    .map(|k| {
                        let t = k as f64 / 16.0 * std::f64::consts::TAU;
                        at_z(c + vec3(t.cos(), t.sin(), 0.0) * r, z)
                    })
                    .collect()
            };
            let c = dr - out * DRAIN_R;
            let pts = ring(c, DRAIN_R);
            for w in pts.windows(2) {
                s.edge(w[0], w[1]);
            }
            // Rohr durch die Aufkantung
            let r = DRAIN_R / 2.0;
            for k in [-1.0, 1.0] {
                let o = d * (r * k);
                s.edge(at_z(dr + o, z), at_z(dr + o + out * self.width, z));
            }
            // Notüberlauf daneben, in Richtung der längeren Kantenhälfte
            let ta = (dr - a).length();
            let tb = (b - dr).length();
            let dir = if tb >= ta { d } else { -d };
            let o0 = dr + dir * OVERFLOW_GAP;
            let o1 = o0 + dir * OVERFLOW_W;
            let q = [o0, o1, o1 + out * self.width, o0 + out * self.width];
            for i in 0..4 {
                s.edge(at_z(q[i], z), at_z(q[(i + 1) % 4], z));
            }
        }
        s.edge_kind = edge_kind::VIEW;
    }

    fn below(&self, cut: f64) -> Solid {
        let mut s = Solid::default();
        let bands = self.bands();
        let Some(&(_, top, mat, _, li)) = bands.iter().rev().find(|l| l.0 < cut) else {
            return s;
        };
        if let Some(g) = self.slope.as_ref().filter(|_| cut >= top) {
            (s.mat, s.layer) = (mat, li);
            let cut_mat = self
                .tapered
                .and_then(|t| bands.iter().find(|l| l.4 as usize == t))
                .map_or(mat, |l| l.2)
                | material::CUT;
            self.sloped_top(&mut s, g, top, cut, cut_mat);
            return s;
        }
        let (z, mat, li) = if cut < top {
            let l = bands
                .iter()
                .find(|l| l.0 < cut && cut < l.1)
                .unwrap_or(&bands[0]);
            (cut, l.2 | material::CUT, l.4)
        } else {
            (top, mat, li)
        };
        s.mat = mat;
        s.layer = li;
        s.edge_kind = if cut < top {
            edge_kind::CUT_LAYER
        } else {
            edge_kind::VIEW
        };
        let ring = &self.outline;
        s.cap(ring, z, true);
        let n = ring.len();
        for i in 0..n {
            s.edge(at_z(ring[i], z), at_z(ring[(i + 1) % n], z));
        }
        s.edge_kind = edge_kind::VIEW;
        s
    }

    /// Dachhaut mit Gefälle (OK Dachhaut `top` am Ablauf), über `cut`
    /// waagerecht abgeschnitten: Teilflächen schräg mit Kanten an Rand,
    /// Kehlen und Graten; was über `cut` liegt, als Schnittfläche `cut_mat`
    /// mit der Höhenlinie als Schnittkante.
    fn sloped_top(&self, s: &mut Solid, g: &SlopeField, top: f64, cut: f64, cut_mat: u16) {
        let h = cut - top;
        let w = |f: &crate::gefaelle::Face, p: Vec3| g.slope * f.path(p);
        let view_mat = s.mat;
        for f in &g.faces {
            let nrm = vec3(-g.slope * f.rise.x, -g.slope * f.rise.y, 1.0).normalized();
            let under = clip_lin(&f.pts, |p| h - w(f, p));
            s.mat = view_mat;
            for k in 1..under.len().saturating_sub(1) {
                let t = [under[0], under[k], under[k + 1]];
                s.oriented_tri(t.map(|p| at_z(p, top + w(f, p))), nrm);
            }
            let over = clip_lin(&f.pts, |p| w(f, p) - h);
            if over.len() >= 3 {
                s.mat = cut_mat;
                s.cap(&over, cut, true);
                // Höhenlinie: die Kante des Schnitts im Inneren der Fläche
                s.edge_kind = edge_kind::CUT_LAYER;
                let k = over.len();
                for i in 0..k {
                    let (a, b) = (over[i], over[(i + 1) % k]);
                    if (w(f, a) - h).abs() < 1e-6 && (w(f, b) - h).abs() < 1e-6 {
                        s.edge(at_z(a, cut), at_z(b, cut));
                    }
                }
            }
        }
        s.mat = view_mat;
        // Rand und Kehlen/Grate: unter dem Schnitt als Ansicht, darüber
        // als Schnittkante auf Höhe `cut`
        let line = |s: &mut Solid, a: Vec3, wa: f64, b: Vec3, wb: f64, rim: bool| {
            let x = if (wa - h) * (wb - h) < 0.0 {
                Some(a + (b - a) * ((h - wa) / (wb - wa)))
            } else {
                None
            };
            let mut seg = |p: Vec3, wp: f64, q: Vec3, wq: f64| {
                if (wp + wq) / 2.0 <= h {
                    s.edge_kind = edge_kind::VIEW;
                    s.edge(at_z(p, top + wp), at_z(q, top + wq));
                } else if rim {
                    s.edge_kind = edge_kind::CUT_LAYER;
                    s.edge(at_z(p, cut), at_z(q, cut));
                }
            };
            match x {
                Some(x) => {
                    seg(a, wa, x, h);
                    seg(x, h, b, wb);
                }
                None => seg(a, wa, b, wb),
            }
        };
        let rim = self.rim();
        let m = rim.len();
        for i in 0..m {
            let ((p, wp), (q, wq)) = (rim[i], rim[(i + 1) % m]);
            line(s, p, wp, q, wq, true);
        }
        for c in &g.creases {
            line(s, c.a, g.wedge_at(c.a), c.b, g.wedge_at(c.b), false);
        }
        s.edge_kind = edge_kind::VIEW;
    }

    /// Schnittflächen des Aufbaus: je Schicht umrandet, Dämmung mit
    /// Schraffur längs (waagerecht), Dampfsperre und Abdichtung ohne.
    pub fn section_caps(&self, p0: Vec3, n: Vec3) -> Solid {
        let f = SectionFrame::new(p0, n);
        let (n, along) = (f.n, f.along);
        let pt = |u: f64, z: f64| f.pt(u, z);
        let mut s = Solid {
            edge_kind: edge_kind::CUT_LAYER,
            ..Solid::default()
        };
        let bands = self.bands();
        for (a, b) in polygon::plane_intervals(&self.outline, p0, n, along) {
            // Knicke des Keils längs des Schnitts: Lauflänge und Keildicke
            let us = self.section_breaks(&f, a, b);
            let w: Vec<f64> = us.iter().map(|&u| self.wedge_at(pt(u, 0.0))).collect();
            for &(z0, z1, mat, ins, li) in &bands {
                s.mat = mat | material::CUT;
                s.layer = li;
                let (ra, rb) = self.rises(li);
                let lo = |i: usize| z0 + if ra { w[i] } else { 0.0 };
                let hi = |i: usize| z1 + if rb { w[i] } else { 0.0 };
                // Schraffur: Maßstab nach der mittleren Dicke, Höhe gestreckt
                let mean = if rb && !ra && us.len() > 1 {
                    (1..us.len())
                        .map(|i| (us[i] - us[i - 1]) * (w[i] + w[i - 1]) / 2.0)
                        .sum::<f64>()
                        / (b - a).max(1.0)
                } else {
                    0.0
                };
                let d = (z1 - z0 + mean).max(1.0);
                for i in 1..us.len() {
                    let (u0, u1) = (us[i - 1], us[i]);
                    let uv = if ins {
                        [[u0 / d, 0.0], [u1 / d, 0.0], [u1 / d, 1.0], [u0 / d, 1.0]]
                    } else {
                        [[0.0; 2]; 4]
                    };
                    s.quad_uv(
                        [
                            pt(u0, lo(i - 1)),
                            pt(u1, lo(i)),
                            pt(u1, hi(i)),
                            pt(u0, hi(i - 1)),
                        ],
                        n,
                        uv,
                    );
                    s.edge(pt(u0, hi(i - 1)), pt(u1, hi(i)));
                }
                let l = us.len() - 1;
                s.edge(pt(a, lo(0)), pt(a, hi(0)));
                s.edge(pt(b, lo(l)), pt(b, hi(l)));
            }
        }
        s.edge_kind = edge_kind::VIEW;
        s
    }

    /// Lauflängen von `a` bis `b` längs des Schnitts, an denen der Keil
    /// knickt (Kanten der Teilflächen); ohne Gefälle nur `a` und `b`.
    fn section_breaks(&self, f: &SectionFrame, a: f64, b: f64) -> Vec<f64> {
        let mut us = vec![a, b];
        if let Some(g) = &self.slope {
            let base = f.pt(0.0, 0.0);
            for face in &g.faces {
                let k = face.pts.len();
                for i in 0..k {
                    let (p, q) = (face.pts[i], face.pts[(i + 1) % k]);
                    let (dp, dq) = ((p - base).dot(f.n), (q - base).dot(f.n));
                    if (dp > 0.0) == (dq > 0.0) || (dp - dq).abs() < 1e-12 {
                        continue;
                    }
                    let x = p + (q - p) * (dp / (dp - dq));
                    let u = (x - base).dot(f.along);
                    if u > a && u < b {
                        us.push(u);
                    }
                }
            }
        }
        us.sort_by(f64::total_cmp);
        us.dedup_by(|x, y| *x - *y < 0.5);
        if let Some(l) = us.last_mut() {
            *l = b;
        }
        us
    }

    // ---- Attikablech (D4) mit Attikadämmung ----

    /// Schnittflächen der Attikadämmung innen: von der Dachhaut bis OK
    /// Aufkantung, Schraffur wie eine Wanddämmung.
    fn inner_section_caps(&self, p0: Vec3, n: Vec3) -> Solid {
        let f = SectionFrame::new(p0, n);
        let (n, along) = (f.n, f.along);
        let mut s = Solid {
            edge_kind: edge_kind::CUT_LAYER,
            mat: self.ins_mat | material::CUT,
            ..Solid::default()
        };
        let w = self.inner_depth.max(1.0);
        let z1 = self.crown;
        for (quad, k, ..) in self.inner_parts() {
            s.elem = k as u32;
            for (a, b) in polygon::plane_intervals(&quad, p0, n, along) {
                let (za, zb) = (
                    self.inner_bottom(f.pt(a, 0.0)).min(z1),
                    self.inner_bottom(f.pt(b, 0.0)).min(z1),
                );
                s.quad_uv(
                    [f.pt(a, za), f.pt(b, zb), f.pt(b, z1), f.pt(a, z1)],
                    n,
                    [[za / w, 0.0], [zb / w, 1.0], [z1 / w, 1.0], [z1 / w, 0.0]],
                );
                s.edge(f.pt(a, z1), f.pt(b, z1));
                s.edge(f.pt(a, za), f.pt(a, z1));
                s.edge(f.pt(b, zb), f.pt(b, z1));
                s.edge(f.pt(a, za), f.pt(b, zb));
            }
        }
        s.edge_kind = edge_kind::VIEW;
        s
    }

    /// Schenkel des Blechs (innen, außen; mm): über dem Dämmstreifen um
    /// dessen Dicke länger, sodass sie wie ohne Streifen überdecken (innen 50
    /// über dem Hochzug bzw. der Attikadämmung innen, außen 50 unter OK
    /// Aufkantung).
    pub(crate) fn coping_legs(&self) -> (f64, f64) {
        (ROOF_COPING_INNER + self.cap, COPING_OUTER + self.cap)
    }

    /// Querschnitt des Blechs mit den Schenkeln nach [`Self::coping_legs`].
    fn profile(&self) -> Vec<(f64, f64)> {
        let (i, o) = self.coping_legs();
        coping_profile_legs(self.coping_width(), i, o)
    }

    /// Breite des Blechs: Aufkantung und Attikadämmung innen (mm), diese
    /// mit der Dicke, um die sie tatsächlich einrückt (an Engstellen
    /// weniger als bestellt, sonst hinge der innere Schenkel frei).
    fn coping_width(&self) -> f64 {
        self.width + self.inner_depth
    }

    /// UK Blech: OK Aufkantung, mit Dämmstreifen darüber (mm, absolut).
    pub fn coping_z(&self) -> f64 {
        self.crown + self.cap
    }

    /// Abwicklung des Blechs (mm).
    pub fn coping_girth(&self) -> f64 {
        let (i, o) = self.coping_legs();
        coping_girth_legs(self.coping_width(), i, o)
    }

    /// Länge des Blechs an der Außenkante der Aufkantung (mm).
    pub fn coping_length(&self) -> f64 {
        polygon::perimeter(&self.ring)
    }

    /// Körper des Blechs: Profil als geschlossener Ring auf OK Aufkantung,
    /// mit Attikadämmung innen und Dämmstreifen darunter (sie gehören zum
    /// Blech, Jörn 10.10. 16:07).
    pub fn coping_solid(&self) -> Solid {
        self.coping_cut_at(f64::INFINITY)
    }

    /// Blech mit Dämmung bis zur Höhe `cut` (Grundriss): Dämmung darüber
    /// geschnitten, das Blech nur, wenn es ganz darunter liegt.
    pub fn coping_cut_at(&self, cut: f64) -> Solid {
        let mut s = self.inner_solid(cut);
        if self.cap > 0.0 {
            let band = (self.crown, self.coping_z());
            s.append(&attika_solid(&self.cap_pieces(), band, cut));
        }
        let d = Solid::default();
        (s.mat, s.elem, s.layer, s.edge_kind) = (self.coping_mat, d.elem, d.layer, edge_kind::VIEW);
        if cut.is_finite() && self.coping_z() >= cut {
            return s;
        }
        let path: Vec<Vec3> = self
            .ring
            .iter()
            .map(|p| at_z(*p, self.coping_z()))
            .collect();
        s.sweep(&path, true, &self.profile(), [SweepEnd::Square; 2]);
        s.edge_kind = edge_kind::VIEW;
        s
    }

    /// Je Kante ein Stück zwischen den Ringen `a` (außen) und `b` (innen),
    /// Ecken auf Gehrung.
    fn pieces(a: &[Vec3], b: &[Vec3], mat: u16) -> Vec<AttikaPiece> {
        let n = a.len().min(b.len());
        (0..n)
            .map(|i| {
                let j = (i + 1) % n;
                AttikaPiece {
                    seg: i,
                    layer: None,
                    quad: [a[i], a[j], b[j], b[i]],
                    mat,
                    free: [false; 2],
                    contour: [true; 2],
                }
            })
            .collect()
    }

    /// Grundriss des Dämmstreifens auf der Krone: Aufkantung und
    /// Attikadämmung innen.
    fn cap_pieces(&self) -> Vec<AttikaPiece> {
        Self::pieces(&self.ring, &self.inset, self.cap_mat)
    }

    /// Unterkante der Attikadämmung innen am Punkt `p`: Dachhaut mit Keil.
    fn inner_bottom(&self, p: Vec3) -> f64 {
        self.band().1 + self.wedge_at(p)
    }

    /// Teilstücke der Attikadämmung innen: je Kante zwischen den Knicken
    /// des Keils (außen Anfang, außen Ende, innen Ende, innen Anfang; Kante,
    /// erstes und letztes Stück der Kante).
    fn inner_parts(&self) -> Vec<([Vec3; 4], usize, bool, bool)> {
        if self.inner <= 0.0 {
            return Vec::new();
        }
        let (o, i) = (&self.outline, &self.inset);
        let n = o.len().min(i.len());
        let mut out = Vec::new();
        for k in 0..n {
            let j = (k + 1) % n;
            let Some(ts) = self.breaks(o[k], o[j]) else {
                continue;
            };
            let at = |t: f64| (o[k] + (o[j] - o[k]) * t, i[k] + (i[j] - i[k]) * t);
            for w in 0..ts.len() - 1 {
                let ((o0, i0), (o1, i1)) = (at(ts[w]), at(ts[w + 1]));
                out.push(([o0, o1, i1, i0], k, w == 0, w + 2 == ts.len()));
            }
        }
        out
    }

    /// Körper der Attikadämmung innen bis zur Höhe `cut`: von der Dachhaut
    /// bis OK Aufkantung, oben bei `cut` geschnitten.
    fn inner_solid(&self, cut: f64) -> Solid {
        let mut s = Solid {
            mat: self.ins_mat,
            ..Solid::default()
        };
        let top = self.crown.min(cut);
        let is_cut = cut < self.crown;
        let lo = |p: Vec3| at_z(p, self.inner_bottom(p).min(top));
        let hi = |p: Vec3| at_z(p, top);
        for ([o0, o1, i1, i0], k, first, last) in self.inner_parts() {
            if [o0, o1, i1, i0]
                .iter()
                .all(|p| self.inner_bottom(*p) >= top)
            {
                continue;
            }
            s.elem = k as u32;
            let d = (o1 - o0).normalized();
            let r = right_of(d);
            let out = if (o0 - i0).dot(r) > 0.0 { r } else { -r };
            s.mat = self.ins_mat;
            s.oriented_quad([lo(o0), lo(o1), hi(o1), hi(o0)], out);
            s.oriented_quad([lo(i1), lo(i0), hi(i0), hi(i1)], -out);
            s.oriented_quad([lo(o0), lo(o1), lo(i1), lo(i0)], vec3(0.0, 0.0, -1.0));
            for (a, b, away, on) in [(i0, o0, -d, first), (o1, i1, d, last)] {
                if on {
                    let m = right_of((b - a).normalized());
                    let nn = if m.dot(away) < 0.0 { -m } else { m };
                    s.oriented_quad([lo(a), lo(b), hi(b), hi(a)], nn);
                }
            }
            s.mat = if is_cut {
                self.ins_mat | material::CUT
            } else {
                self.ins_mat
            };
            s.oriented_quad([hi(o0), hi(o1), hi(i1), hi(i0)], vec3(0.0, 0.0, 1.0));
            s.edge_kind = if is_cut {
                edge_kind::CUT_LAYER
            } else {
                edge_kind::VIEW
            };
            s.edge(hi(o0), hi(o1));
            s.edge(hi(i0), hi(i1));
            s.edge_kind = edge_kind::VIEW;
            // Fuge zur Dachhaut innen, senkrechte Kanten an den Ecken
            s.edge(lo(i0), lo(i1));
            for (on, a, b) in [(first, o0, i0), (last, o1, i1)] {
                if on {
                    s.edge(lo(b), hi(b));
                    s.edge_kind = if is_cut {
                        edge_kind::CUT_LAYER
                    } else {
                        edge_kind::FINE
                    };
                    s.edge(hi(a), hi(b));
                    s.edge_kind = edge_kind::VIEW;
                }
            }
        }
        s.edge_kind = edge_kind::VIEW;
        s
    }

    /// Ansichtsfläche (innen, mm²) und Volumen (mm³) der Attikadämmung
    /// innen: Höhe von der Dachhaut bis OK Aufkantung. Mit Keil exakt: je
    /// Stück über die ebenen Teilflächen des Gefälleplans integriert (die
    /// Höhe ist dort linear), wo die Dämmung über OK Aufkantung käme, null.
    /// So bleibt die Summe gleich, wenn ein Punkt im Umriss dazukommt
    /// (Teilen einer Wand, Review Geometrie #61).
    pub fn inner_qto(&self) -> (f64, f64) {
        let mut out = (0.0, 0.0);
        let top = self.crown - self.band().1;
        for ([o0, o1, i1, i0], ..) in self.inner_parts() {
            let quad = polygon::to_ccw(&[o0, o1, i1, i0]);
            let Some(g) = &self.slope else {
                let h = top.max(0.0);
                out.0 += (i1 - i0).length() * h;
                out.1 += polygon::area(&quad) * h;
                continue;
            };
            // Höhe über einer Teilfläche: linear, a + b · p
            let lin = |f: &crate::gefaelle::Face| {
                let s = g.slope;
                (top - s * f.c, vec3(-s * f.rise.x, -s * f.rise.y, 0.0))
            };
            let (mut face, mut vol, mut seen) = (0.0, 0.0, 0.0);
            for f in &g.faces {
                let (a, b) = lin(f);
                let h = |p: Vec3| a + b.x * p.x + b.y * p.y;
                // Grundfläche: Stück ∩ Teilfläche ∩ {h ≥ 0}
                let mut poly = clip_convex(&quad, &f.pts);
                seen += polygon::area(&poly);
                poly = clip_half(&poly, h);
                if let Some(c) = polygon::centroid(&poly) {
                    vol += polygon::area(&poly) * h(c);
                }
                // Ansicht: Innenkante ∩ Teilfläche, h ≥ 0, trapezweise
                if let Some((ta, tb)) = segment_in(&f.pts, i0, i1) {
                    let (pa, pb) = (i0 + (i1 - i0) * ta, i0 + (i1 - i0) * tb);
                    let (ha, hb) = (h(pa), h(pb));
                    let len = (pb - pa).length();
                    face += match (ha >= 0.0, hb >= 0.0) {
                        (true, true) => len * (ha + hb) / 2.0,
                        (false, false) => 0.0,
                        // nur der Teil mit h ≥ 0: Dreieck bis zur Nullstelle
                        _ => {
                            let (hp, hn) = if ha >= 0.0 { (ha, hb) } else { (hb, ha) };
                            len * hp / (hp - hn) * hp / 2.0
                        }
                    };
                }
            }
            // Rest ohne Teilfläche (Rundung am Rand): Höhe wie gezeichnet
            let rest = polygon::area(&quad) - seen;
            if rest > 1.0 {
                if let Some(c) = polygon::centroid(&quad) {
                    vol += rest * (self.crown - self.inner_bottom(c)).max(0.0);
                }
            }
            out.0 += face;
            out.1 += vol;
        }
        out
    }

    /// Wie [`FlatRoof::inner_qto`], als feine Mittelpunktsumme mit `n` × `n`
    /// Teilen je Stück (Gegenprobe in Tests).
    #[cfg(test)]
    pub(crate) fn inner_qto_fein(&self, n: usize) -> (f64, f64) {
        let h = |p: Vec3| (self.crown - self.inner_bottom(p)).max(0.0);
        let mut out = (0.0, 0.0);
        for ([o0, o1, i1, i0], ..) in self.inner_parts() {
            let quad = polygon::to_ccw(&[o0, o1, i1, i0]);
            let area = polygon::area(&quad);
            let nn = n as f64;
            for a in 0..n {
                let s = (a as f64 + 0.5) / nn;
                out.0 += (i1 - i0).length() / nn * h(i0 + (i1 - i0) * s);
                for b in 0..n {
                    let t = (b as f64 + 0.5) / nn;
                    // bilinear im Trapez; Flächenanteil je Zelle über die
                    // Breite an der Stelle t
                    let (p, q) = (o0 + (o1 - o0) * s, i0 + (i1 - i0) * s);
                    let x = p + (q - p) * t;
                    let wa = (o1 - o0).length();
                    let wb = (i1 - i0).length();
                    let w = wa + (wb - wa) * t;
                    let mean = (wa + wb) / 2.0;
                    out.1 += area / (nn * nn) * (w / mean) * h(x);
                }
            }
        }
        out
    }

    /// Grundfläche des Dämmstreifens auf der Krone (mm²).
    pub fn cap_area(&self) -> f64 {
        if self.cap <= 0.0 {
            return 0.0;
        }
        self.cap_pieces()
            .iter()
            .map(|p| polygon::area(&polygon::to_ccw(&p.quad)))
            .sum()
    }

    /// Schnittfläche des Blechs: das Profil in der Ebene, kräftig umrandet;
    /// darunter die Dämmung, umrandet wie Wandschichten.
    pub fn coping_section_caps(&self, p0: Vec3, n: Vec3) -> Solid {
        let nn = vec3(n.x, n.y, 0.0).normalized();
        let mut s = self.inner_section_caps(p0, n);
        if self.cap > 0.0 {
            let band = (self.crown, self.coping_z());
            s.append(&crown_section_caps(&self.cap_pieces(), band, p0, n));
        }
        let d = Solid::default();
        (s.mat, s.elem, s.layer, s.edge_kind) = (
            self.coping_mat | material::CUT,
            d.elem,
            d.layer,
            edge_kind::CUT,
        );
        let profile = self.profile();
        let flat: Vec<Vec3> = profile.iter().map(|p| vec3(p.0, p.1, 0.0)).collect();
        let tris = polygon::triangulate(&flat);
        let z = self.coping_z();
        let m = self.ring.len();
        for k in 0..m {
            let (a, b) = (self.ring[k], self.ring[(k + 1) % m]);
            let (da, db) = ((a - p0).dot(nn), (b - p0).dot(nn));
            if (da < 0.0) == (db < 0.0) || (da - db).abs() < 1e-9 {
                continue;
            }
            let x = a + (b - a) * (da / (da - db));
            let d = vec3(b.x - a.x, b.y - a.y, 0.0).normalized();
            let r = right_of(d);
            // Querrichtung in der Ebene (das Profil geschert)
            let q = r - d * (r.dot(nn) / d.dot(nn));
            let at = |(u, h): (f64, f64)| vec3(x.x, x.y, z) + q * u + vec3(0.0, 0.0, h);
            for tri in &tris {
                s.oriented_tri(tri.map(|i| at(profile[i])), nn);
            }
            let np = profile.len();
            for i in 0..np {
                s.edge(at(profile[i]), at(profile[(i + 1) % np]));
            }
        }
        s.edge_kind = edge_kind::VIEW;
        s
    }
}

/// Pfeil im Grundriss: höchstens so lang, Spitze so lang (mm).
const ARROW: f64 = 1000.0;
const ARROW_HEAD: f64 = 200.0;
/// Halbmesser des Ablaufzeichens (mm).
const DRAIN_R: f64 = 120.0;
/// Notüberlauf im Grundriss: Abstand vom Ablauf und Breite (mm).
pub(crate) const OVERFLOW_GAP: f64 = 400.0;
pub(crate) const OVERFLOW_W: f64 = 300.0;

/// Teil des konvexen Polygons `pts`, in dem die lineare Funktion `f` nicht
/// negativ ist (z = 0).
fn clip_lin(pts: &[Vec3], f: impl Fn(Vec3) -> f64) -> Vec<Vec3> {
    let n = pts.len();
    let mut out = Vec::with_capacity(n + 1);
    for i in 0..n {
        let (p, q) = (pts[i], pts[(i + 1) % n]);
        let (fp, fq) = (f(p), f(q));
        if fp >= 0.0 {
            out.push(p);
        }
        if (fp >= 0.0) != (fq >= 0.0) {
            out.push(p + (q - p) * (fp / (fp - fq)));
        }
    }
    out
}

// ---- Automatikmengen (`roof.*`, Plan Flachdach D3, für die Leistungen K3) ----

pub const EDGE: &str = "roof.edge";
pub const CORNERS: &str = "roof.corners";
pub const DRAINS: &str = "roof.drains";
pub const OVERFLOWS: &str = "roof.overflows";
pub const TAPER: &str = "roof.taper";
pub const VALLEYS: &str = "roof.valleys";
pub const RIDGES: &str = "roof.ridges";

/// Schlüssel des Flachdachs (`[service] auto=`), Einheit und Bedeutung.
pub const SCHLUESSEL: [(&str, &str, &str); 7] = [
    (
        EDGE,
        "m",
        "Anschluss der Abdichtung an die Aufkantung: Länge an ihrer Innenfläche",
    ),
    (
        CORNERS,
        "st",
        "Innenecken des Anschlusses an der Aufkantung (Knick ab 5°, ohne einspringende Ecken)",
    ),
    (
        DRAINS,
        "st",
        "Dachabläufe: 1 je angefangene 150 m² Dachfläche",
    ),
    (OVERFLOWS, "st", "Notüberläufe: so viele wie Dachabläufe"),
    (
        TAPER,
        "m3",
        "Gefälledämmung: Keil über der Mindestdicke (nur mit Gefälle)",
    ),
    (VALLEYS, "m", "Kehlen der Gefälledämmung: Länge (Zuschnitt)"),
    (RIDGES, "m", "Grate der Gefälledämmung: Länge (Zuschnitt)"),
];

/// Länge einer Keilplatte in Fließrichtung (mm): eine Stufe des
/// Plattenplans ist Gefälle × Länge (2 % → 20 mm).
pub const PLATE_LENGTH: f64 = 1000.0;

/// Dachfläche je Ablauf (mm², Schätzung des Plans Flachdach D3; die
/// Bemessung nach DIN 1986-100 macht der Fachplaner).
pub const DRAIN_AREA: f64 = 150.0e6;

/// Kostengruppe der Dachbeläge samt Entwässerung (DIN 276: 363).
pub const KG_ROOF: u16 = 363;

/// Automatikmengen eines Flachdachs: Anschlusslänge, Ecken, Abläufe und
/// Notüberläufe. `vorlage` trägt Bauteil, Nummer und Geschoss (den
/// Dachaufbau).
pub fn roof_mengen(vorlage: &crate::qto::AutoMenge, r: &FlatRoof) -> Vec<crate::qto::AutoMenge> {
    let m = |mm: f64| format!("{:.2} m", mm / 1000.0).replace('.', ",");
    let area = r.area();
    let geplant = r.slope.as_ref().map(|g| g.drains.len() as f64);
    // ohne Gefälle so viele wie Regenfallrohre (B30 an #68), sonst nach
    // Fläche
    let rf = (!r.downpipes.is_empty()).then_some(r.downpipes.len() as f64);
    let drains = geplant
        .or(rf)
        .unwrap_or_else(|| (area / DRAIN_AREA).ceil().max(1.0));
    let menge = |key, unit, value, formula| crate::qto::AutoMenge {
        key,
        unit,
        value,
        kg: Some(KG_ROOF),
        formula,
        ..vorlage.clone()
    };
    let corners = r.corners() as f64;
    let fl = format!("{:.2} m²", area / 1e6).replace('.', ",");
    let drain_formula = match (geplant, rf) {
        (Some(n), _) => format!("{n} Abläufe im Gefälleplan"),
        (None, Some(n)) => format!("{n} Abläufe mit Regenfallrohr"),
        (None, None) => format!("{fl} ÷ 150 m², aufgerundet, mindestens 1"),
    };
    let mut out = vec![
        menge(
            EDGE,
            "m",
            r.edge_length(),
            format!("Innenfläche der Aufkantung {}", m(r.edge_length())),
        ),
        menge(CORNERS, "st", corners, format!("{corners} Innenecken")),
        menge(DRAINS, "st", drains, drain_formula),
    ];
    // ohne eigenen Notüberlauf, wo der Dachrand zu niedrig ist: Das
    // Notwasser läuft dort frei über die Attika (Geometrie an #71)
    let frei = r.downpipes.iter().filter(|d| d.overflow.free).count() as f64;
    let overflows = (drains - frei).max(0.0);
    if overflows > 0.0 {
        out.push(menge(
            OVERFLOWS,
            "st",
            overflows,
            if frei > 0.0 {
                format!("{drains} Dachabläufe, davon {frei} frei über die Attika")
            } else {
                "so viele wie Dachabläufe".into()
            },
        ));
    }
    if let Some(g) = &r.slope {
        let v = g.wedge_volume();
        let (valley, ridge) = g.crease_lengths();
        out.extend([
            menge(
                TAPER,
                "m3",
                v,
                format!(
                    "{fl} × {} cm mittlerer Keil",
                    format!("{:.1}", g.wedge_mean() / 10.0).replace('.', ",")
                ),
            ),
            menge(VALLEYS, "m", valley, format!("Kehlen {}", m(valley))),
            menge(RIDGES, "m", ridge, format!("Grate {}", m(ridge))),
        ]);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// L-förmige Innenfläche mit einem geraden Zwischenpunkt: 6 Knicke,
    /// davon einer einspringend, also 5 Innenecken; in beiden Umlaufrichtungen.
    #[test]
    fn innenecken_ohne_einspringende() {
        let mut outline = vec![
            vec3(0.0, 0.0, 0.0),
            vec3(5000.0, 0.0, 0.0),
            vec3(10000.0, 0.0, 0.0),
            vec3(10000.0, 4000.0, 0.0),
            vec3(4000.0, 4000.0, 0.0),
            vec3(4000.0, 8000.0, 0.0),
            vec3(0.0, 8000.0, 0.0),
        ];
        let mut r = FlatRoof {
            outline: outline.clone(),
            base: 0.0,
            layers: Vec::new(),
            ring: Vec::new(),
            width: 0.0,
            inner: 0.0,
            inset: Vec::new(),
            inner_depth: 0.0,
            cap: 0.0,
            ins_mat: 0,
            cap_mat: 0,
            downpipes: Vec::new(),
            crown: 0.0,
            coping_mat: 0,
            slope: None,
            tapered: None,
        };
        assert_eq!(r.corners(), 5);
        outline.reverse();
        r.outline = outline;
        assert_eq!(r.corners(), 5);
    }

    /// Rechteck 10 × 8 m, Abläufe in der Mitte der Langseiten, 2 %: 1 cm
    /// Abdichtung, 20 cm Dämmung (Gefälleschicht), 5 mm Dampfsperre.
    fn mit_gefaelle() -> FlatRoof {
        let outline = vec![
            vec3(0.0, 0.0, 0.0),
            vec3(10000.0, 0.0, 0.0),
            vec3(10000.0, 8000.0, 0.0),
            vec3(0.0, 8000.0, 0.0),
        ];
        let drains = [vec3(5000.0, 0.0, 0.0), vec3(5000.0, 8000.0, 0.0)];
        FlatRoof {
            slope: SlopeField::compute(&outline, &drains, 2.0),
            tapered: Some(1),
            outline,
            base: 1000.0,
            layers: vec![(10.0, 1, false), (200.0, 2, true), (5.0, 3, false)],
            ring: Vec::new(),
            width: 0.0,
            inner: 0.0,
            inset: Vec::new(),
            inner_depth: 0.0,
            cap: 0.0,
            ins_mat: 0,
            cap_mat: 0,
            downpipes: Vec::new(),
            crown: 1600.0,
            coping_mat: 0,
        }
    }

    /// Rauminhalt eines geschlossenen Körpers (Divergenzsatz, mm³).
    fn rauminhalt(s: &Solid) -> f64 {
        s.triangles
            .iter()
            .map(|t| t.p[0].dot(t.p[1].cross(t.p[2])) / 6.0)
            .sum()
    }

    /// Der Körper ist geschlossen und hat das Volumen von Aufbau und Keil;
    /// die Anschlusshöhe zählt ab dem höchsten Randpunkt (Mitte der
    /// Schmalseiten: 4 m Fließweg, 8 cm).
    #[test]
    fn koerper_mit_keil() {
        let r = mit_gefaelle();
        let g = r.slope.as_ref().unwrap();
        let soll = 215.0 * 80.0e6 + g.wedge_volume();
        let v = rauminhalt(&r.solid());
        assert!((v - soll).abs() < 1e-6 * soll, "{v} statt {soll}");
        assert!(
            (g.wedge_edge_max() - 100.0).abs() < 1e-6,
            "{}",
            g.wedge_edge_max()
        );
        assert!((r.upstand() - (1600.0 - 1215.0 - 100.0)).abs() < 1e-6);
        assert!((r.top_max() - 1215.0 - g.wedge_max()).abs() < 1e-9);
        // ohne Gefälle wie bisher
        let flach = FlatRoof {
            slope: None,
            ..mit_gefaelle()
        };
        assert!((rauminhalt(&flach.solid()) - 215.0 * 80.0e6).abs() < 1.0);
        assert!((flach.upstand() - 385.0).abs() < 1e-9);
    }

    /// Schnitt quer durch die Abläufe (x = 5 m) und längs (y = 4 m): die
    /// Schnittfläche ist Aufbau mal Länge plus die Fläche unter dem Keil.
    #[test]
    fn schnitt_mit_keil() {
        let r = mit_gefaelle();
        for (p0, n, len) in [
            (vec3(5000.0, 0.0, 0.0), vec3(1.0, 0.0, 0.0), 8000.0),
            (vec3(0.0, 4000.0, 0.0), vec3(0.0, 1.0, 0.0), 10000.0),
            (vec3(3000.0, 0.0, 0.0), vec3(1.0, 0.0, 0.0), 8000.0),
        ] {
            let s = r.section_caps(p0, n);
            let flaeche: f64 = s
                .triangles
                .iter()
                .map(|t| (t.p[1] - t.p[0]).cross(t.p[2] - t.p[0]).length() / 2.0)
                .sum();
            let f = SectionFrame::new(p0, n);
            let (a, b) = polygon::plane_intervals(&r.outline, p0, n, f.along)[0];
            let k = 4000;
            let keil: f64 = (0..k)
                .map(|i| r.wedge_at(f.pt(a + (b - a) * (i as f64 + 0.5) / k as f64, 0.0)))
                .sum::<f64>()
                * (b - a)
                / k as f64;
            let soll = 215.0 * len + keil;
            assert!(
                (flaeche - soll).abs() < 1e-3 * soll,
                "{flaeche} statt {soll}"
            );
        }
    }

    /// Attikadämmung innen 6 cm und Krone 4 cm am Dach mit Gefälle (Jörn
    /// 10.10. 16:07): Die Dämmung innen steht auf der Dachhaut mit Keil, ihr
    /// Körper hat das Volumen der Mengen; im Schnitt quer zur Langseite je
    /// Seite 6 cm × (OK Aufkantung − Dachhaut am Rand); das Blech liegt auf
    /// dem Streifen.
    fn mit_daemmung() -> FlatRoof {
        let ring = vec![
            vec3(-300.0, -300.0, 0.0),
            vec3(10300.0, -300.0, 0.0),
            vec3(10300.0, 8300.0, 0.0),
            vec3(-300.0, 8300.0, 0.0),
        ];
        let inset = vec![
            vec3(60.0, 60.0, 0.0),
            vec3(9940.0, 60.0, 0.0),
            vec3(9940.0, 7940.0, 0.0),
            vec3(60.0, 7940.0, 0.0),
        ];
        FlatRoof {
            ring,
            width: 300.0,
            inner: 60.0,
            inset,
            inner_depth: 60.0,
            cap: 40.0,
            ins_mat: 2,
            cap_mat: 2,
            downpipes: Vec::new(),
            coping_mat: 4,
            ..mit_gefaelle()
        }
    }

    #[test]
    fn attikadaemmung_mit_keil() {
        let r = mit_daemmung();
        let (face, vol) = r.inner_qto();
        let v = rauminhalt(&r.inner_solid(f64::INFINITY));
        assert!((v - vol).abs() < 1e-3 * vol, "{v} statt {vol}");
        // ohne Keil: 35,52 m Innenfläche × (1600 − 1215)
        let flach = FlatRoof {
            slope: None,
            ..mit_daemmung()
        };
        assert!((flach.inner_qto().0 - 35_520.0 * 385.0).abs() < 1e-3);
        assert!(face < flach.inner_qto().0);
        // Schnitt x = 5 m: am Ablauf (y = 0 und 8 m) liegt die Dachhaut tief
        let s = r.inner_section_caps(vec3(5000.0, 0.0, 0.0), vec3(1.0, 0.0, 0.0));
        let flaeche: f64 = s
            .triangles
            .iter()
            .map(|t| (t.p[1] - t.p[0]).cross(t.p[2] - t.p[0]).length() / 2.0)
            .sum();
        let h = |y: f64| 1600.0 - 1215.0 - r.wedge_at(vec3(5000.0, y, 0.0));
        let soll = 60.0 * ((h(0.0) + h(60.0)) / 2.0 + (h(7940.0) + h(8000.0)) / 2.0);
        assert!(
            (flaeche - soll).abs() < 1e-6 * soll,
            "{flaeche} statt {soll}"
        );
        // Blech auf dem Streifen, Breite 360, beide Schenkel 40 länger:
        // Abwicklung + 60 + 2 × 40
        assert_eq!(r.coping_z(), 1640.0);
        assert_eq!(r.coping_girth(), flach.coping_girth());
        assert_eq!(
            r.coping_girth() - 60.0 - 80.0,
            coping_girth_legs(300.0, ROOF_COPING_INNER, COPING_OUTER)
        );
        assert_eq!(r.cap_area(), (38_400.0 + 35_520.0) / 2.0 * 360.0);
        // im Grundriss unter OK Aufkantung geschnitten: kein Blech, Dämmung
        // innen geschnitten
        let g = r.coping_cut_at(1500.0);
        assert!(g.triangles.iter().any(|t| t.mat == 2 | material::CUT));
        assert!(g.triangles.iter().all(|t| t.mat & !material::CUT == 2));
    }

    /// Grundriss: knapp über OK Dachhaut am Ablauf geschnitten, deckt der
    /// Aufbau (schräg gesehen plus Schnittfläche) genau die Dachfläche.
    #[test]
    fn grundriss_mit_keil() {
        let r = mit_gefaelle();
        for cut in [1250.0, 1500.0, f64::INFINITY] {
            let s = r.cut_at(cut);
            let flaeche: f64 = s
                .triangles
                .iter()
                .map(|t| {
                    let (u, v) = (t.p[1] - t.p[0], t.p[2] - t.p[0]);
                    (u.x * v.y - u.y * v.x) / 2.0
                })
                .sum();
            assert!((flaeche - 80.0e6).abs() < 1.0, "{cut}: {flaeche}");
            let schnitt = s.triangles.iter().any(|t| t.mat & material::CUT != 0);
            assert_eq!(schnitt, cut == 1250.0, "{cut}");
            assert!(s
                .triangles
                .iter()
                .all(|t| t.p.iter().all(|p| p.z <= cut + 1e-9)));
        }
    }

    /// Schnitt durch den Dämmstreifen auf der Krone: Schraffur längs wie
    /// liegende Dämmung (v quer zur Schicht von UK bis OK Streifen), die
    /// Attikadämmung innen darunter steht (Jörn 11.10.).
    #[test]
    fn kronendaemmung_schraffur_liegend() {
        let r = mit_daemmung();
        let s = r.coping_section_caps(vec3(5000.0, 0.0, 0.0), vec3(1.0, 0.0, 0.0));
        let ins = |t: &&crate::solid::Tri| t.mat == 2 | material::CUT;
        let krone: Vec<&crate::solid::Tri> = s
            .triangles
            .iter()
            .filter(ins)
            .filter(|t| t.p.iter().all(|p| p.z >= 1600.0 - 1e-9))
            .collect();
        assert!(!krone.is_empty());
        for t in &krone {
            for k in 0..3 {
                let v = (t.p[k].z - 1600.0) / 40.0;
                assert!((t.uv[k][1] - v).abs() < 1e-9, "{t:?}");
            }
        }
        // innen steht die Dämmung: v quer läuft waagerecht, nicht mit z
        assert!(s
            .triangles
            .iter()
            .filter(ins)
            .filter(|t| t.p.iter().all(|p| p.z <= 1600.0 + 1e-9))
            .any(|t| (0..3).any(|k| t.uv[k][1] > 0.5 && t.p[k].z < 1500.0)));
    }
}
