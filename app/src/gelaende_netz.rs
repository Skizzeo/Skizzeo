//! Echtes Gelände in der App (Gelände G1, plan-gelaendemodul.md §5 G1
//! „Darstellung“, §4.5 und §4.6): Sobald ein Höhenaufmaß da ist, steht statt
//! der Geländeplatte sein Dreiecksnetz da, gleich bedient und gleich
//! gefärbt. In 3D ein Körper wie ein Geländemodell: Rasen oben, Erdreich
//! am Rand bis eine Geländedicke unter den tiefsten Punkt, die vollen
//! Meter als Höhenlinien. Im Grundriss Höhenlinien (alle 0,25 m, volle
//! Meter kräftig), Messpunkte und die Grenze; im Schnitt die Schnittfläche
//! mit der Schraffur „Erdreich“ und die Geländelinie; in den Ansichten die
//! Geländelinie in der Ebene der Fassade, die zum Betrachter zeigt.
//!
//! Der Zuschnitt gilt überall gleich. Abgeschnitten wird nur die
//! Darstellung: Dreiecke und Linien werden an der Grenze geteilt, gerechnet
//! wird weiter mit allen Punkten. Bei „Umgebung blass“ liegt das ganze
//! Netz blass da und das Grundstück knapp darüber in Rasen.
//!
//! Am Haus endet das Gelände wie die Platte (Außenumriss über der
//! Gründung): kein Rasen, keine Höhenlinie und kein Messpunkt im Haus,
//! im Schnitt endet die Geländelinie an der Außenwand. Darunter liegt
//! Erdreich ab der Unterkante der Gründung.

use crate::draw_table::{DrawTable, Stroke};
use crate::ui::ViewKind;
use sk_math::{polygon, vec3, Vec3};
use sk_model::solid::{right_of, SectionFrame, Tri, NO_LAYER};
use sk_model::{edge_kind, material, FootingShape, LevelKind, Model, Show, Solid};
use sk_render::Helper;

/// Abstand der Höhenlinien im Grundriss (m); volle Meter sind
/// Hauptlinien, nur sie stehen in 3D. Gäbe er zu viele Stücke, gilt der
/// gröbere aus [`Model::terrain_contour_step`] (Review #121, Auflage H1).
pub(crate) const STUFE: f64 = 0.25;

/// Das Grundstück liegt bei „Umgebung blass“ so weit über dem blassen Netz
/// (mm), damit beide nicht flimmern.
const HEBEN: f64 = 5.0;

/// Kantenlänge einer Punktmarke im Grundriss (Bildpunkte bei Skalierung 1).
const MARKE: f64 = 3.0;

/// Ein Punkt im Grundriss.
type P2 = [f64; 2];

/// Gelände aus dem Aufmaß im Modell (mm, z zu ±0,00).
pub(crate) struct Gelaende {
    dreiecke: Vec<[Vec3; 3]>,
    /// Grenzen; leer ohne Grenze.
    ringe: Vec<Vec<P2>>,
    /// Zuschnitt; ohne Grenze immer „alles“.
    show: Show,
    /// Höhenlinien: Höhe, Punkte, Hauptlinie.
    linien: Vec<(f64, Vec<P2>, bool)>,
    /// Messpunkte, die zählen.
    punkte: Vec<Vec3>,
    /// Außenumriss jedes Hauses über der Gründung und die Unterkante
    /// seiner Gründung (mm).
    haus: Vec<(Vec<P2>, f64)>,
    /// Innenseite jeder Frostschürze mit Unterkante und Oberkante des
    /// Erdreichs in ihr (UK Schürze bis UK Dämmung oder Platte).
    kern: Vec<(Vec<P2>, f64, f64)>,
    /// Unterkante des Rands: eine Geländedicke unter dem tiefsten Punkt.
    pub(crate) unten: f64,
}

impl Gelaende {
    /// Das Gelände zum Modell; `None` ohne Aufmaß, Lage oder Netz.
    pub(crate) fn aus(m: &Model) -> Option<Gelaende> {
        let dreiecke = m.terrain_triangles();
        if dreiecke.is_empty() {
            return None;
        }
        let (t, f, h0) = (m.terrain()?, m.survey_frame()?, m.terrain_nhn()?);
        let tief = dreiecke
            .iter()
            .flatten()
            .map(|p| p.z)
            .fold(f64::INFINITY, f64::min);
        let ringe = m.terrain_boundaries();
        let show = if ringe.is_empty() {
            Show::All
        } else {
            t.show()
        };
        let linien = m
            .terrain_contour_step(STUFE)
            .map_or(Vec::new(), |s| m.terrain_contours(s))
            .into_iter()
            .map(|(z, p, _)| ((z - h0) * 1000.0, p, (z - z.round()).abs() < 1e-6))
            .collect();
        let punkte = t
            .points
            .iter()
            .filter(|p| !p.off)
            .map(|p| {
                let [x, y] = f.to_model(p.e, p.n);
                vec3(x, y, (p.h - h0) * 1000.0)
            })
            .collect();
        let uk = |b| {
            m.storeys()
                .iter()
                .filter(|(_, s)| s.kind == LevelKind::Foundation && s.building == Some(b))
                .map(|(_, s)| s.elevation)
                .fold(0.0, f64::min)
        };
        let flach = |r: &[Vec3]| r.iter().map(|p| [p.x, p.y]).collect::<Vec<P2>>();
        let haus = m
            .plate_outlines()
            .into_iter()
            .map(|(b, r)| (flach(&r), uk(b)))
            .collect();
        let kern = m
            .runs()
            .iter()
            .filter_map(|(id, r)| {
                let b = m.building_of(r.storey)?;
                let Some(Ok(f)) = m.foundation(id) else {
                    return None;
                };
                let FootingShape::Ring(i) = &f.footing else {
                    return None;
                };
                Some((flach(&i.pts), uk(b), uk(b) + f.params.footing_depth))
            })
            .collect();
        Some(Gelaende {
            dreiecke,
            ringe,
            show,
            linien,
            punkte,
            haus,
            kern,
            unten: tief - m.plate_settings().thick,
        })
    }

    /// Quader um das Netz bis zur Unterkante des Rands.
    #[cfg_attr(not(all(test, target_os = "linux")), allow(dead_code))] // Prüfbilder
    pub(crate) fn rahmen(&self) -> (Vec3, Vec3) {
        let mut lo = vec3(f64::MAX, f64::MAX, self.unten);
        let mut hi = vec3(f64::MIN, f64::MIN, self.unten);
        for p in self.dreiecke.iter().flatten() {
            lo = vec3(lo.x.min(p.x), lo.y.min(p.y), lo.z);
            hi = vec3(hi.x.max(p.x), hi.y.max(p.y), hi.z.max(p.z));
        }
        (lo, hi)
    }

    /// Liegt `p` im Grundstück (ohne Grenze: überall)?
    fn innen(&self, p: P2) -> bool {
        self.ringe.is_empty() || self.ringe.iter().any(|r| im_ring(r, p))
    }

    /// Liegt `p` in einem Haus?
    fn im_haus(&self, p: P2) -> bool {
        self.haus.iter().any(|(r, _)| im_ring(r, p))
    }

    /// Teile der Strecke `a`→`b` an Grenze und Haus: Parameter von, bis
    /// und ob der Teil im Grundstück liegt; Teile im Haus fallen weg.
    fn teile(&self, a: P2, b: P2) -> Vec<(f64, f64, bool)> {
        let mut ts = vec![0.0, 1.0];
        for r in self.ringe.iter().chain(self.haus.iter().map(|h| &h.0)) {
            ts.extend(schnitte(r, a, b));
        }
        ts.sort_by(f64::total_cmp);
        ts.dedup_by(|x, y| (*x - *y).abs() < 1e-9);
        ts.windows(2)
            .filter_map(|w| {
                let m = lerp2(a, b, (w[0] + w[1]) / 2.0);
                (!self.im_haus(m)).then(|| (w[0], w[1], self.innen(m)))
            })
            .collect()
    }

    /// Die Dreiecke ohne die Häuser: Ein Dreieck, das ein Haus berührt,
    /// wird an den Geraden der Hauskanten in konvexe Stücke geteilt, die
    /// Stücke im Haus fallen weg.
    fn ohne_haus(&self, d: Vec<[Vec3; 3]>) -> Vec<[Vec3; 3]> {
        if self.haus.is_empty() {
            return d;
        }
        let kaesten: Vec<[f64; 4]> = self.haus.iter().map(|(r, _)| kasten(r)).collect();
        let mut out = Vec::with_capacity(d.len());
        for t in d {
            let ecken = t.map(|p| [p.x, p.y]);
            if !kaesten.iter().any(|k| ueberlappt(k, &kasten(&ecken))) {
                out.push(t);
                continue;
            }
            let mut stuecke = vec![ecken.to_vec()];
            for (r, _) in &self.haus {
                for i in 0..r.len() {
                    let (c, e) = (r[i], r[(i + 1) % r.len()]);
                    let k = kasten(&[c, e]);
                    stuecke = stuecke
                        .into_iter()
                        .flat_map(|s| {
                            if ueberlappt(&k, &kasten(&s)) {
                                halbieren(s, c, e)
                            } else {
                                vec![s]
                            }
                        })
                        .collect();
                }
            }
            for s in stuecke {
                let m = s.iter().fold([0.0, 0.0], |m, p| [m[0] + p[0], m[1] + p[1]]);
                let k = s.len() as f64;
                if self.im_haus([m[0] / k, m[1] / k]) {
                    continue;
                }
                let p: Vec<Vec3> = s.iter().map(|q| vec3(q[0], q[1], ebene(&t, *q))).collect();
                for i in 1..p.len() - 1 {
                    let d = [p[0], p[i], p[i + 1]];
                    if (d[1] - d[0]).cross(d[2] - d[0]).z > 1e-3 {
                        out.push(d);
                    }
                }
            }
        }
        out
    }

    /// Höhe des Netzes an `p`; `None` außerhalb.
    fn hoehe(&self, p: P2) -> Option<f64> {
        self.dreiecke.iter().find_map(|t| {
            let l = baryzentrisch(t, p)?;
            Some(l[0] * t[0].z + l[1] * t[1].z + l[2] * t[2].z)
        })
    }

    /// Wie die Strecke `a`→`b` gezeigt wird: im Grundstück voll, außerhalb
    /// blass (`Some(false)`) oder gar nicht (`None`).
    fn zeigen(&self, innen: bool) -> Option<bool> {
        match (self.show, innen) {
            (_, true) | (Show::All, false) => Some(true),
            (Show::Fade, false) => Some(false),
            (Show::Parcel, false) => None,
        }
    }

    /// Profil des Netzes in der senkrechten Ebene durch `p0` mit Normale
    /// `n`: Strecken (Lauflänge, Höhe) je Dreieck, an der Grenze geteilt,
    /// mit Sichtbarkeit wie [`Gelaende::zeigen`].
    fn profil(&self, f: &SectionFrame, p0: Vec3) -> Vec<([f64; 2], [f64; 2], bool)> {
        let n = f.n;
        let mut out = Vec::new();
        for t in &self.dreiecke {
            let d = t.map(|p| (p - p0).dot(n));
            let mut q: Vec<Vec3> = Vec::new();
            for i in 0..3 {
                let (a, b) = (i, (i + 1) % 3);
                if (d[a] < 0.0) != (d[b] < 0.0) {
                    let s = d[a] / (d[a] - d[b]);
                    q.push(t[a] + (t[b] - t[a]) * s);
                }
            }
            let [a, b] = match q[..] {
                [a, b] => [a, b],
                _ => continue,
            };
            for (t0, t1, innen) in self.teile([a.x, a.y], [b.x, b.y]) {
                let Some(voll) = self.zeigen(innen) else {
                    continue;
                };
                let (pa, pb) = (a + (b - a) * t0, a + (b - a) * t1);
                let u = |p: Vec3| (p - f.pt(0.0, 0.0)).dot(f.along);
                out.push(([u(pa), pa.z], [u(pb), pb.z], voll));
            }
        }
        out
    }
}

fn lerp2(a: P2, b: P2, t: f64) -> P2 {
    [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t]
}

/// Parameter auf `a`→`b`, wo sie den Ring `r` kreuzt.
fn schnitte(r: &[P2], a: P2, b: P2) -> impl Iterator<Item = f64> + '_ {
    (0..r.len()).filter_map(move |i| kreuzung(a, b, r[i], r[(i + 1) % r.len()]))
}

/// Rechteck um die Punkte: x, y von, x, y bis.
fn kasten(p: &[P2]) -> [f64; 4] {
    p.iter()
        .fold([f64::MAX, f64::MAX, f64::MIN, f64::MIN], |k, q| {
            [
                k[0].min(q[0]),
                k[1].min(q[1]),
                k[2].max(q[0]),
                k[3].max(q[1]),
            ]
        })
}

fn ueberlappt(a: &[f64; 4], b: &[f64; 4]) -> bool {
    a[0] <= b[2] && b[0] <= a[2] && a[1] <= b[3] && b[1] <= a[3]
}

/// Konvexes Vieleck `s` an der Geraden durch `c`→`e` geteilt (beide Teile
/// im gleichen Umlaufsinn); liegt es ganz auf einer Seite, bleibt es.
fn halbieren(s: Vec<P2>, c: P2, e: P2) -> Vec<Vec<P2>> {
    let l = (e[0] - c[0]).hypot(e[1] - c[1]);
    if l < 1e-9 {
        return vec![s];
    }
    let seite = |p: P2| {
        let d = ((e[0] - c[0]) * (p[1] - c[1]) - (e[1] - c[1]) * (p[0] - c[0])) / l;
        if d.abs() < 1e-6 {
            0.0
        } else {
            d
        }
    };
    let d: Vec<f64> = s.iter().map(|p| seite(*p)).collect();
    if d.iter().all(|x| *x >= 0.0) || d.iter().all(|x| *x <= 0.0) {
        return vec![s];
    }
    let (mut links, mut rechts) = (Vec::new(), Vec::new());
    for i in 0..s.len() {
        let j = (i + 1) % s.len();
        if d[i] >= 0.0 {
            links.push(s[i]);
        }
        if d[i] <= 0.0 {
            rechts.push(s[i]);
        }
        if d[i] * d[j] < 0.0 {
            let x = lerp2(s[i], s[j], d[i] / (d[i] - d[j]));
            links.push(x);
            rechts.push(x);
        }
    }
    [links, rechts]
        .into_iter()
        .filter(|v| v.len() >= 3)
        .collect()
}

/// Punkt im Ring (gerade-ungerade, Umlaufsinn beliebig).
fn im_ring(r: &[P2], p: P2) -> bool {
    let mut innen = false;
    let n = r.len();
    for i in 0..n {
        let (a, b) = (r[i], r[(i + n - 1) % n]);
        if (a[1] > p[1]) != (b[1] > p[1])
            && p[0] < a[0] + (b[0] - a[0]) * (p[1] - a[1]) / (b[1] - a[1])
        {
            innen = !innen;
        }
    }
    innen
}

/// Parameter auf `a`→`b`, wo die Strecke `c`→`d` sie kreuzt (echt innen).
fn kreuzung(a: P2, b: P2, c: P2, d: P2) -> Option<f64> {
    let (r, s) = ([b[0] - a[0], b[1] - a[1]], [d[0] - c[0], d[1] - c[1]]);
    let den = r[0] * s[1] - r[1] * s[0];
    if den.abs() < 1e-12 {
        return None;
    }
    let q = [c[0] - a[0], c[1] - a[1]];
    let t = (q[0] * s[1] - q[1] * s[0]) / den;
    let u = (q[0] * r[1] - q[1] * r[0]) / den;
    (t > 1e-9 && t < 1.0 - 1e-9 && (0.0..=1.0).contains(&u)).then_some(t)
}

/// Baryzentrische Koordinaten von `p` im Dreieck (Grundriss); `None`
/// außerhalb.
fn baryzentrisch(t: &[Vec3; 3], p: P2) -> Option<[f64; 3]> {
    let d = (t[1].x - t[0].x) * (t[2].y - t[0].y) - (t[1].y - t[0].y) * (t[2].x - t[0].x);
    if d.abs() < 1e-12 {
        return None;
    }
    let l1 = ((t[1].x - p[0]) * (t[2].y - p[1]) - (t[1].y - p[1]) * (t[2].x - p[0])) / d;
    let l2 = ((t[2].x - p[0]) * (t[0].y - p[1]) - (t[2].y - p[1]) * (t[0].x - p[0])) / d;
    let l3 = 1.0 - l1 - l2;
    let e = -1e-9;
    (l1 >= e && l2 >= e && l3 >= e).then_some([l1, l2, l3])
}

/// Höhe der Ebene durch das Dreieck an `p`.
fn ebene(t: &[Vec3; 3], p: P2) -> f64 {
    let d = (t[1].x - t[0].x) * (t[2].y - t[0].y) - (t[1].y - t[0].y) * (t[2].x - t[0].x);
    let l1 = ((t[1].x - p[0]) * (t[2].y - p[1]) - (t[1].y - p[1]) * (t[2].x - p[0])) / d;
    let l2 = ((t[2].x - p[0]) * (t[0].y - p[1]) - (t[2].y - p[1]) * (t[0].x - p[0])) / d;
    l1 * t[0].z + l2 * t[1].z + (1.0 - l1 - l2) * t[2].z
}

/// Ring `r` mit dem Dreieck `t` (gegen den Uhrzeigersinn) geschnitten
/// (Sutherland-Hodgman, das Dreieck ist das konvexe Fenster).
fn im_dreieck(r: &[P2], t: &[Vec3; 3]) -> Vec<P2> {
    let mut v: Vec<P2> = r.to_vec();
    for k in 0..3 {
        let (a, b) = (t[k], t[(k + 1) % 3]);
        let seite = |p: P2| (b.x - a.x) * (p[1] - a.y) - (b.y - a.y) * (p[0] - a.x);
        let mut neu = Vec::with_capacity(v.len() + 2);
        for i in 0..v.len() {
            let (p, q) = (v[i], v[(i + 1) % v.len()]);
            let (sp, sq) = (seite(p), seite(q));
            if sp >= 0.0 {
                neu.push(p);
            }
            if (sp >= 0.0) != (sq >= 0.0) {
                neu.push(lerp2(p, q, sp / (sp - sq)));
            }
        }
        v = neu;
        if v.len() < 3 {
            return Vec::new();
        }
    }
    v
}

fn normale(t: &[Vec3; 3]) -> Vec3 {
    (t[1] - t[0]).cross(t[2] - t[0]).normalized()
}

fn dreieck(s: &mut Solid, p: [Vec3; 3], n: Vec3) {
    s.triangles.push(Tri {
        p,
        n,
        mat: s.mat,
        uv: [[0.0; 2]; 3],
        elem: s.elem,
        layer: NO_LAYER,
    });
}

fn heben(p: Vec3, dz: f64) -> Vec3 {
    vec3(p.x, p.y, p.z + dz)
}

/// Dreiecke im Grundstück, an der Grenze geteilt.
fn im_grundstueck(g: &Gelaende) -> Vec<[Vec3; 3]> {
    let mut out = Vec::new();
    for t in &g.dreiecke {
        // Ganz innen: alle Ecken innen, keine Kante kreuzt die Grenze und
        // kein Grenzpunkt liegt im Dreieck
        let ecken = t.iter().all(|p| g.innen([p.x, p.y]));
        let kreuzt = (0..3).any(|i| {
            let (a, b) = (t[i], t[(i + 1) % 3]);
            g.ringe
                .iter()
                .any(|r| schnitte(r, [a.x, a.y], [b.x, b.y]).next().is_some())
        });
        let grenzpunkt = g
            .ringe
            .iter()
            .flatten()
            .any(|q| baryzentrisch(t, *q).is_some());
        if ecken && !kreuzt && !grenzpunkt {
            out.push(*t);
            continue;
        }
        for r in &g.ringe {
            let v = im_dreieck(r, t);
            if v.len() < 3 {
                continue;
            }
            let p: Vec<Vec3> = v.iter().map(|q| vec3(q[0], q[1], ebene(t, *q))).collect();
            for [a, b, c] in polygon::triangulate(&p) {
                out.push([p[a], p[b], p[c]]);
            }
        }
    }
    out
}

/// Kanten, die nur ein Dreieck hat (Rand des Netzes), in seinem Umlauf.
fn randkanten(d: &[[Vec3; 3]]) -> Vec<(Vec3, Vec3)> {
    use std::collections::HashMap;
    type Ecke = (u64, u64);
    let key = |p: Vec3| -> Ecke { (p.x.to_bits(), p.y.to_bits()) };
    let mut n: HashMap<(Ecke, Ecke), u32> = HashMap::new();
    for t in d {
        for i in 0..3 {
            let (a, b) = (key(t[i]), key(t[(i + 1) % 3]));
            *n.entry(if a < b { (a, b) } else { (b, a) }).or_default() += 1;
        }
    }
    let mut out = Vec::new();
    for t in d {
        for i in 0..3 {
            let (a, b) = (t[i], t[(i + 1) % 3]);
            let (ka, kb) = (key(a), key(b));
            if n[&if ka < kb { (ka, kb) } else { (kb, ka) }] == 1 {
                out.push((a, b));
            }
        }
    }
    out
}

/// Seitenfläche von der Kante `a`→`b` bis `unten`, nach außen rechts.
fn seite(s: &mut Solid, a: Vec3, b: Vec3, unten: f64) {
    let d = (b - a).normalized();
    let (au, bu) = (vec3(a.x, a.y, unten), vec3(b.x, b.y, unten));
    s.quad(au, bu, b, a, right_of(d));
    s.edge(a, b);
    s.edge(au, bu);
}

/// Körper in 3D; `keys`: Rasen, Erdreich, Rasen blass.
pub(crate) fn koerper(g: &Gelaende, keys: [u16; 3]) -> Solid {
    let mut s = Solid {
        mat: keys[0],
        edge_kind: edge_kind::VIEW,
        ..Solid::default()
    };
    let ganz = match g.show {
        Show::Parcel => im_grundstueck(g),
        Show::All | Show::Fade => g.dreiecke.clone(),
    };
    let oben = g.ohne_haus(ganz.clone());
    s.mat = if g.show == Show::Fade {
        keys[2]
    } else {
        keys[0]
    };
    for t in &oben {
        dreieck(&mut s, *t, normale(t));
    }
    if g.show == Show::Fade {
        s.mat = keys[0];
        for t in g.ohne_haus(im_grundstueck(g)) {
            dreieck(&mut s, t.map(|p| heben(p, HEBEN)), normale(&t));
        }
    }
    // Boden und Rand in Erdreich
    s.mat = keys[1];
    let unten = g.unten;
    let down = vec3(0.0, 0.0, -1.0);
    for t in &ganz {
        let u = t.map(|p| vec3(p.x, p.y, unten));
        dreieck(&mut s, [u[0], u[2], u[1]], down);
    }
    match g.show {
        Show::Parcel => {
            for r in &g.ringe {
                rand_an_grenze(&mut s, g, r, unten, true);
            }
        }
        Show::All | Show::Fade => {
            for (a, b) in randkanten(&g.dreiecke) {
                seite(&mut s, a, b, unten);
            }
        }
    }
    // Am Haus bis zur Unterkante der Gründung, darunter Erdreich
    for (r, uk) in &g.haus {
        rand_an_grenze(&mut s, g, r, uk.max(unten), false);
        if *uk > unten + 1.0 {
            let p: Vec<Vec3> = r.iter().map(|q| vec3(q[0], q[1], *uk)).collect();
            for [a, b, c] in polygon::triangulate(&p) {
                dreieck(&mut s, [p[a], p[b], p[c]], vec3(0.0, 0.0, 1.0));
            }
        }
    }
    // Volle Meter als feine Kanten auf dem Rasen
    s.edge_kind = edge_kind::FINE;
    for (z, p, haupt) in &g.linien {
        if !*haupt {
            continue;
        }
        for w in p.windows(2) {
            for (t0, t1, innen) in g.teile(w[0], w[1]) {
                if g.show == Show::Parcel && !innen {
                    continue;
                }
                let dz = if innen && g.show == Show::Fade {
                    HEBEN
                } else {
                    0.0
                };
                let (a, b) = (lerp2(w[0], w[1], t0), lerp2(w[0], w[1], t1));
                s.edge(vec3(a[0], a[1], z + dz), vec3(b[0], b[1], z + dz));
            }
        }
    }
    s
}

/// Rand längs des Rings `r` bis `unten`: geteilt an jeder Netzkante,
/// damit die Oberkante genau auf dem Netz liegt. An der Grenze (`aussen`)
/// nach außen gerichtet, am Haus nach innen und nur, wo das Gelände
/// gezeigt wird; liegt das Netz dort tiefer als `unten`, entfällt er.
fn rand_an_grenze(s: &mut Solid, g: &Gelaende, r: &[P2], unten: f64, aussen: bool) {
    let ccw =
        polygon::signed_area(&r.iter().map(|p| vec3(p[0], p[1], 0.0)).collect::<Vec<_>>()) > 0.0;
    let dz = if !aussen && g.show == Show::Fade {
        HEBEN
    } else {
        0.0
    };
    let n = r.len();
    for i in 0..n {
        let (a, b) = if ccw == aussen {
            (r[i], r[(i + 1) % n])
        } else {
            (r[(i + 1) % n], r[i])
        };
        let mut ts = vec![0.0, 1.0];
        for t in &g.dreiecke {
            for k in 0..3 {
                let (c, d) = (t[k], t[(k + 1) % 3]);
                if let Some(x) = kreuzung(a, b, [c.x, c.y], [d.x, d.y]) {
                    ts.push(x);
                }
            }
        }
        ts.sort_by(f64::total_cmp);
        ts.dedup_by(|x, y| (*x - *y).abs() < 1e-9);
        for w in ts.windows(2) {
            let (p, q) = (lerp2(a, b, w[0]), lerp2(a, b, w[1]));
            if !aussen && g.zeigen(g.innen(lerp2(p, q, 0.5))).is_none() {
                continue;
            }
            if let (Some(zp), Some(zq)) = (g.hoehe(p), g.hoehe(q)) {
                let (zp, zq) = (zp + dz, zq + dz);
                if zp.max(zq) > unten {
                    let (zp, zq) = (zp.max(unten), zq.max(unten));
                    seite(s, vec3(p[0], p[1], zp), vec3(q[0], q[1], zq), unten);
                }
            }
        }
    }
}

/// Schnittflächen mit der Ebene (zeigen in Richtung `f.n`), Erdreich.
pub(crate) fn kappen(g: &Gelaende, f: &SectionFrame, p0: Vec3, soil: u16) -> Solid {
    let mut s = Solid {
        mat: soil | material::CUT,
        edge_kind: edge_kind::CUT,
        ..Solid::default()
    };
    let (mut lo, mut hi) = (f64::INFINITY, f64::NEG_INFINITY);
    for (a, b, voll) in g.profil(f, p0) {
        if !voll {
            continue;
        }
        let (a, b) = if a[0] <= b[0] { (a, b) } else { (b, a) };
        (lo, hi) = (lo.min(a[0]), hi.max(b[0]));
        s.quad(
            f.pt(a[0], g.unten),
            f.pt(b[0], g.unten),
            f.pt(b[0], b[1]),
            f.pt(a[0], a[1]),
            f.n,
        );
    }
    // Unter dem Haus bis zur Unterkante der Gründung, in der Schürze bis
    // unter Dämmung oder Platte
    let schichten = g.haus.iter().map(|(r, uk)| (r, g.unten, *uk));
    let kern = g
        .kern
        .iter()
        .map(|(r, von, bis)| (r, von.max(g.unten), *bis));
    for (r, von, bis) in schichten.chain(kern) {
        if bis <= von + 1.0 {
            continue;
        }
        let q: Vec<Vec3> = r.iter().map(|p| vec3(p[0], p[1], 0.0)).collect();
        for (a, b) in polygon::plane_intervals(&q, p0, f.n, f.along) {
            let (a, b) = (a.max(lo), b.min(hi));
            if a < b {
                s.quad(f.pt(a, von), f.pt(b, von), f.pt(b, bis), f.pt(a, bis), f.n);
            }
        }
    }
    s
}

/// Linien in Grundriss, Schnitt und Ansicht. `mm_px`: Millimeter je
/// Bildpunkt (Grundriss), `fassade`: Ebene der Geländelinie einer Ansicht.
pub(crate) fn linien(
    g: &Gelaende,
    view: ViewKind,
    plane: Option<(Vec3, Vec3)>,
    fassade: Option<(Vec3, Vec3)>,
    t: &DrawTable,
    scale: f32,
    mm_px: f64,
) -> Vec<Helper> {
    let fine = t.drawing_edges[edge_kind::FINE as usize];
    let blass = t.background;
    let line = |a: Vec3, b: Vec3, (w, c): Stroke, pattern| Helper {
        a: a.to_f32(),
        b: b.to_f32(),
        color: c,
        width: w * scale,
        dash: 0.0,
        pattern,
        occlude: false,
        round: false,
    };
    let solid = sk_render::SOLID;
    let mut out = Vec::new();
    match (view, plane.or(fassade)) {
        (ViewKind::Plan, _) => {
            for (z, p, haupt) in &g.linien {
                let stift = if *haupt { t.ground } else { fine };
                for w in p.windows(2) {
                    for (t0, t1, innen) in g.teile(w[0], w[1]) {
                        let Some(voll) = g.zeigen(innen) else {
                            continue;
                        };
                        let (a, b) = (lerp2(w[0], w[1], t0), lerp2(w[0], w[1], t1));
                        let st = if voll { stift } else { blass };
                        let (a, b) = (vec3(a[0], a[1], *z), vec3(b[0], b[1], *z));
                        out.push(line(a, b, st, solid));
                    }
                }
            }
            let k = MARKE * f64::from(scale) * mm_px;
            for p in g.punkte.iter().filter(|p| !g.im_haus([p.x, p.y])) {
                let Some(voll) = g.zeigen(g.innen([p.x, p.y])) else {
                    continue;
                };
                let st = if voll { fine } else { blass };
                for (dx, dy) in [(k, k), (k, -k)] {
                    let (a, b) = (vec3(p.x - dx, p.y - dy, p.z), vec3(p.x + dx, p.y + dy, p.z));
                    out.push(line(a, b, st, solid));
                }
            }
            // Grenze strichpunktiert, kräftig
            for r in &g.ringe {
                for i in 0..r.len() {
                    let (a, b) = (r[i], r[(i + 1) % r.len()]);
                    let za = g.hoehe(a).unwrap_or(0.0);
                    let zb = g.hoehe(b).unwrap_or(0.0);
                    let (a, b) = (vec3(a[0], a[1], za), vec3(b[0], b[1], zb));
                    out.push(line(a, b, t.ground, t.section_dash));
                }
            }
        }
        (ViewKind::Persp, _) | (_, None) => {}
        (_, Some((p0, n))) => {
            let f = SectionFrame::new(p0, n);
            let profil = g.profil(&f, p0);
            let (mut lo, mut hi) = (f64::INFINITY, f64::NEG_INFINITY);
            for (a, b, voll) in &profil {
                let st = if *voll { t.ground } else { blass };
                out.push(line(f.pt(a[0], a[1]), f.pt(b[0], b[1]), st, solid));
                if *voll {
                    lo = lo.min(a[0]).min(b[0]);
                    hi = hi.max(a[0]).max(b[0]);
                }
            }
            // Unterkante und Enden fein, wie bei der Platte
            if lo < hi {
                let z = |u: f64| {
                    profil
                        .iter()
                        .find_map(|(a, b, _)| {
                            [a, b]
                                .into_iter()
                                .find(|p| (p[0] - u).abs() < 1e-6)
                                .map(|p| p[1])
                        })
                        .unwrap_or(g.unten)
                };
                out.push(line(f.pt(lo, g.unten), f.pt(hi, g.unten), fine, solid));
                out.push(line(f.pt(lo, g.unten), f.pt(lo, z(lo)), fine, solid));
                out.push(line(f.pt(hi, g.unten), f.pt(hi, z(hi)), fine, solid));
            }
        }
    }
    out
}

/// Ebene der Geländelinie einer Ansicht: die Fassade, die zum Betrachter
/// zeigt („Vorne“ blickt nach +y), am Rechteck um das Haus; ohne Haus
/// durch die Mitte des Netzes.
pub(crate) fn fassade(
    g: &Gelaende,
    view: ViewKind,
    haus: Option<(Vec3, Vec3)>,
) -> Option<(Vec3, Vec3)> {
    let (lo, hi) = haus.unwrap_or_else(|| {
        let mut lo = vec3(f64::MAX, f64::MAX, 0.0);
        let mut hi = vec3(f64::MIN, f64::MIN, 0.0);
        for p in g.dreiecke.iter().flatten() {
            lo = vec3(lo.x.min(p.x), lo.y.min(p.y), 0.0);
            hi = vec3(hi.x.max(p.x), hi.y.max(p.y), 0.0);
        }
        let m = (lo + hi) * 0.5;
        (m, m)
    });
    let n = match view {
        ViewKind::Front => (vec3(lo.x, lo.y, 0.0), vec3(0.0, 1.0, 0.0)),
        ViewKind::Back => (vec3(hi.x, hi.y, 0.0), vec3(0.0, -1.0, 0.0)),
        ViewKind::Left => (vec3(lo.x, lo.y, 0.0), vec3(1.0, 0.0, 0.0)),
        ViewKind::Right => (vec3(hi.x, hi.y, 0.0), vec3(-1.0, 0.0, 0.0)),
        ViewKind::Persp | ViewKind::Plan | ViewKind::Section => return None,
    };
    Some(n)
}

#[cfg(test)]
mod tests {
    use super::*;

    const CSV: &[u8] =
        include_bytes!("../../crates/sk-gelaende/tests/daten/hoehenaufmass-lindenhof-7.csv");

    fn lindenhof() -> Model {
        let mut m = Model::with_seed(7);
        let b = m.add_building(1);
        let pts = [
            vec3(7000.0, 6000.0, 0.0),
            vec3(7000.0, 14000.0, 0.0),
            vec3(17000.0, 14000.0, 0.0),
            vec3(17000.0, 6000.0, 0.0),
        ];
        m.build_from_polygon(b, &pts).unwrap();
        let (t, ..) = sk_model::Terrain::lesen(CSV, "a.csv", "2026-10-12", 32).unwrap();
        m.begin("Aufmaß");
        assert!(m.import_terrain(t));
        m.commit();
        m
    }

    fn flaeche(d: &[[Vec3; 3]]) -> f64 {
        d.iter()
            .map(|t| (t[1] - t[0]).cross(t[2] - t[0]).z.abs() / 2.0)
            .sum::<f64>()
            / 1e6
    }

    /// Zugeschnitten (Vorgabe mit Grenze) ist die Rasenfläche das
    /// Grundstück (822,87 m²), der Rand geschlossen; mit „alles“ die Fläche
    /// des ganzen Netzes.
    #[test]
    fn zuschnitt_am_grundstueck() {
        let mut m = lindenhof();
        assert_eq!(Gelaende::aus(&m).unwrap().show, Show::Parcel);
        m.begin("Zuschnitt");
        assert!(m.set_terrain_show(Show::All));
        m.commit();
        let g = Gelaende::aus(&m).unwrap();
        let ganz = flaeche(&g.dreiecke);
        assert!(ganz > 900.0, "{ganz}");
        let innen = flaeche(&im_grundstueck(&g));
        assert!((innen - 822.87).abs() < 0.05, "{innen}");
        // Rasen oben bis auf das Haus, Erdreich unten und am Rand, jede
        // Fläche zeigt nach außen
        let haus = {
            let q: Vec<Vec3> = g.haus[0].0.iter().map(|p| vec3(p[0], p[1], 0.0)).collect();
            polygon::area(&q) / 1e6
        };
        let keys = [10, 11, 12];
        let s = koerper(&g, keys);
        let rasen: Vec<[Vec3; 3]> = s
            .triangles
            .iter()
            .filter(|t| t.mat == 10)
            .map(|t| t.p)
            .collect();
        assert!((flaeche(&rasen) + haus - ganz).abs() < 0.01);
        assert!(s.triangles.iter().all(|t| t.n.length() > 0.99));
        m.begin("Zuschnitt");
        assert!(m.set_terrain_show(Show::Parcel));
        m.commit();
        let g = Gelaende::aus(&m).unwrap();
        let s = koerper(&g, keys);
        let oben: Vec<[Vec3; 3]> = s
            .triangles
            .iter()
            .filter(|t| t.mat == 10)
            .map(|t| t.p)
            .collect();
        assert!((flaeche(&oben) + haus - 822.87).abs() < 0.05);
        // Der Rand reicht überall vom Netz bis zur Unterkante
        let seiten = s
            .triangles
            .iter()
            .filter(|t| t.mat == 11 && t.n.z.abs() < 1e-9)
            .count();
        assert!(seiten > 8, "{seiten}");
        assert!(s
            .triangles
            .iter()
            .all(|t| t.p.iter().all(|p| p.z >= g.unten - 1e-6)));
    }

    /// Grundriss: Höhenlinien im Zuschnitt nur im Grundstück, außerhalb
    /// blass oder weg; die Grenze strichpunktiert.
    #[test]
    fn grundriss_und_schnitt() {
        let mut m = lindenhof();
        let t = DrawTable::resolve(&m, &sk_ui::theme::Theme::dark());
        m.begin("Zuschnitt");
        assert!(m.set_terrain_show(Show::All));
        m.commit();
        let g = Gelaende::aus(&m).unwrap();
        let alle = linien(&g, ViewKind::Plan, None, None, &t, 1.0, 10.0);
        assert!(alle.len() > 50, "{}", alle.len());
        m.begin("Zuschnitt");
        m.set_terrain_show(Show::Parcel);
        m.commit();
        let g = Gelaende::aus(&m).unwrap();
        let innen = linien(&g, ViewKind::Plan, None, None, &t, 1.0, 10.0);
        assert!(innen.len() < alle.len());
        for h in &innen {
            let mitte = [
                f64::from(h.a[0] + h.b[0]) / 2.0,
                f64::from(h.a[1] + h.b[1]) / 2.0,
            ];
            // Grenze und Marken der Grenzpunkte liegen auf der Grenze
            let auf_grenze = h.pattern == t.section_dash;
            let grenzpunkt = g
                .ringe
                .iter()
                .flatten()
                .any(|q| (q[0] - mitte[0]).hypot(q[1] - mitte[1]) < 1.0);
            assert!(auf_grenze || grenzpunkt || g.innen(mitte), "{h:?}");
            assert!(auf_grenze || !g.im_haus(mitte), "im Haus: {h:?}");
        }
        // Schnitt quer durch das Haus: Schnittfläche bis zur Unterkante
        let (p0, n) = (vec3(12000.0, 10000.0, 0.0), vec3(0.0, 1.0, 0.0));
        let f = SectionFrame::new(p0, n);
        let k = kappen(&g, &f, p0, 11);
        assert!(!k.triangles.is_empty());
        assert!(k.triangles.iter().all(|t| t.mat == 11 | material::CUT));
        let l = linien(&g, ViewKind::Section, Some((p0, n)), None, &t, 1.0, 10.0);
        assert!(l.len() > 3);
        // Am Haus endet die Geländelinie, darunter Erdreich ab der
        // Unterkante der Gründung, in der Schürze bis unter die Platte
        let (ring, uk) = &g.haus[0];
        assert!(*uk < 0.0 && *uk > g.unten, "{uk} {}", g.unten);
        let [(_, von, dach)] = g.kern[..] else {
            panic!("eine Schürze: {:?}", g.kern.len());
        };
        assert_eq!(von, *uk);
        assert!(dach > *uk && dach < 0.0, "{dach}");
        let q: Vec<Vec3> = ring.iter().map(|p| vec3(p[0], p[1], 0.0)).collect();
        let [(ha, hb)] = polygon::plane_intervals(&q, p0, f.n, f.along)[..] else {
            panic!("Schnitt trifft das Haus nicht");
        };
        let u = |p: [f32; 3]| {
            let p = vec3(f64::from(p[0]), f64::from(p[1]), f64::from(p[2]));
            (p - f.pt(0.0, 0.0)).dot(f.along)
        };
        let oben = |h: &&Helper| [h.a, h.b].iter().all(|p| f64::from(p[2]) > g.unten + 1.0);
        for h in l.iter().filter(oben) {
            let m = (u(h.a) + u(h.b)) / 2.0;
            assert!(m < ha || m > hb, "Geländelinie im Haus: {h:?}");
        }
        for tri in &k.triangles {
            let m = tri
                .p
                .iter()
                .map(|p| (*p - f.pt(0.0, 0.0)).dot(f.along))
                .sum::<f64>()
                / 3.0;
            if m > ha && m < hb {
                assert!(tri.p.iter().all(|p| p.z <= dach + 1e-6), "{tri:?}");
            }
        }
        // In 3D kein Rasen im Haus, die Fläche um die Grundfläche kleiner
        let s = koerper(&g, [10, 11, 12]);
        let rasen: Vec<[Vec3; 3]> = s
            .triangles
            .iter()
            .filter(|t| t.mat == 10)
            .map(|t| t.p)
            .collect();
        for d in &rasen {
            let m = [
                (d[0].x + d[1].x + d[2].x) / 3.0,
                (d[0].y + d[1].y + d[2].y) / 3.0,
            ];
            assert!(!g.im_haus(m), "{d:?}");
        }
        let haus = polygon::area(&q) / 1e6;
        assert!(
            (flaeche(&rasen) + haus - 822.87).abs() < 0.05,
            "{} + {haus}",
            flaeche(&rasen)
        );
    }
}
