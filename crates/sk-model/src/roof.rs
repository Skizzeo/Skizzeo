//! Flachdach: waagerechter Dachaufbau zwischen den Aufkantungen und
//! Attikablech auf ihrer Krone (Jörn 10.10., Plan Flachdach D3, D4).
//!
//! Der Aufbau liegt auf der obersten Rohdecke und füllt die Innenfläche der
//! Aufkantung; seine Schichten stehen von oben nach unten im Typ. Das Blech
//! läuft als geschlossener Ring auf der Außenfläche der Aufkantung über die
//! ganze Wanddicke und zählt nicht zur Höhe der Ebene.

use crate::gefaelle::SlopeField;
use crate::solid::{at_z, edge_kind, material, right_of, SectionFrame, Solid, SweepEnd, NO_LAYER};
use crate::terrace::{coping_girth_with, coping_profile_with, ROOF_COPING_INNER};
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
        let Some(g) = &self.slope else {
            return ring.iter().map(|p| (*p, 0.0)).collect();
        };
        let n = ring.len();
        let mut out = Vec::new();
        for i in 0..n {
            let (a, b) = (ring[i], ring[(i + 1) % n]);
            let d = b - a;
            let len = d.length();
            if len < 1e-6 {
                continue;
            }
            let mut ts: Vec<f64> = g
                .faces
                .iter()
                .flat_map(|f| f.pts.iter())
                .filter_map(|p| {
                    let t = (*p - a).dot(d) / (len * len);
                    let q = a + d * t;
                    let on = vec3(p.x - q.x, p.y - q.y, 0.0).length() < 0.5;
                    (on && t * len > 0.5 && t * len < len - 0.5).then_some(t)
                })
                .collect();
            ts.push(0.0);
            ts.sort_by(f64::total_cmp);
            ts.dedup_by(|x, y| (*x - *y) * len < 0.5);
            out.extend(ts.into_iter().map(|t| {
                let p = a + d * t;
                (p, g.wedge_at(p))
            }));
        }
        out
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
        self.below(cut)
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

    // ---- Attikablech (D4) ----

    /// Querschnitt des Blechs (Innenschenkel 50 über dem Hochzug).
    fn profile(&self) -> Vec<(f64, f64)> {
        coping_profile_with(self.width, ROOF_COPING_INNER)
    }

    /// Abwicklung des Blechs (mm).
    pub fn coping_girth(&self) -> f64 {
        coping_girth_with(self.width, ROOF_COPING_INNER)
    }

    /// Länge des Blechs an der Außenkante der Aufkantung (mm).
    pub fn coping_length(&self) -> f64 {
        polygon::perimeter(&self.ring)
    }

    /// Körper des Blechs: Profil als geschlossener Ring auf OK Aufkantung.
    pub fn coping_solid(&self) -> Solid {
        let mut s = Solid {
            mat: self.coping_mat,
            ..Solid::default()
        };
        let path: Vec<Vec3> = self.ring.iter().map(|p| at_z(*p, self.crown)).collect();
        s.sweep(&path, true, &self.profile(), [SweepEnd::Square; 2]);
        s.edge_kind = edge_kind::VIEW;
        s
    }

    /// Schnittfläche des Blechs: das Profil in der Ebene, kräftig umrandet.
    pub fn coping_section_caps(&self, p0: Vec3, n: Vec3) -> Solid {
        let nn = vec3(n.x, n.y, 0.0).normalized();
        let mut s = Solid {
            mat: self.coping_mat | material::CUT,
            edge_kind: edge_kind::CUT,
            ..Solid::default()
        };
        let profile = self.profile();
        let flat: Vec<Vec3> = profile.iter().map(|p| vec3(p.0, p.1, 0.0)).collect();
        let tris = polygon::triangulate(&flat);
        let z = self.crown;
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

/// Schlüssel des Flachdachs (`[service] auto=`), Einheit und Bedeutung.
pub const SCHLUESSEL: [(&str, &str, &str); 4] = [
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
];

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
    let drains = (area / DRAIN_AREA).ceil().max(1.0);
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
    vec![
        menge(
            EDGE,
            "m",
            r.edge_length(),
            format!("Innenfläche der Aufkantung {}", m(r.edge_length())),
        ),
        menge(CORNERS, "st", corners, format!("{corners} Innenecken")),
        menge(
            DRAINS,
            "st",
            drains,
            format!("{fl} ÷ 150 m², aufgerundet, mindestens 1"),
        ),
        menge(OVERFLOWS, "st", drains, "so viele wie Dachabläufe".into()),
    ]
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
}
