//! Stützen und Treppen in der Wohnfläche (Review #195 B1): Erweiterungen
//! mit `art=column` oder `art=stair` (`werk.stuetze`, `werk.treppe`) im
//! Grundriss, je Geschoss einmal für alle Räume gesammelt. Ihr Grundriss ist
//! die senkrechte Projektion ihrer Körper, in konvexen Stücken; mit dem
//! Fertigumriss eines Raums geschnitten ergibt sich die Fläche im Raum.
//!
//! - Stützen (freistehende Pfeiler und Säulen) über 0,1 m² und höher als
//!   1,50 m werden abgezogen (§ 3 Abs. 3 Nr. 1), sonst ein Hinweis.
//! - Treppen mit über drei Steigungen samt Podest werden abgezogen (§ 3
//!   Abs. 3 Nr. 2), sonst ein Hinweis.

use sk_math::{polygon, vec3, Vec3};
use sk_model::{ElementId, ElementKind, Model, StoreyId};
use std::collections::HashMap;

/// Ab dieser Höhe (mm) zählt eine Stütze als Abzug (§ 3 Abs. 3 Nr. 1).
pub const STUETZE_HOCH: f64 = 1500.0;
/// Ab so vielen Steigungen (mehr als drei) zählt eine Treppe als Abzug (§ 3
/// Abs. 3 Nr. 2).
pub const STEIGUNGEN: f64 = 3.0;

/// Stütze oder Treppe im Grundriss.
#[derive(Clone, Debug, PartialEq)]
pub struct Einbau {
    pub id: ElementId,
    pub treppe: bool,
    /// Grundriss in konvexen Stücken, gegen den Uhrzeigersinn (z = 0).
    pub stuecke: Vec<Vec<Vec3>>,
    /// Unter- und Oberkante (mm, absolut).
    pub z: (f64, f64),
    /// Steigungen einer Treppe aus ihrem Parameter `stufen`, wenn sie einen
    /// hat.
    pub steigungen: Option<f64>,
}

/// Stützen und Treppen je Geschoss, in einem Gang über die Bauteile.
pub fn sammeln(model: &Model) -> HashMap<StoreyId, Vec<Einbau>> {
    let mut out: HashMap<StoreyId, Vec<Einbau>> = HashMap::new();
    for (id, e) in model.elements().iter() {
        let ElementKind::Ext(p) = &e.kind else {
            continue;
        };
        let Some(d) = model.ext_def(&p.key) else {
            continue;
        };
        let treppe = match d.def.bauteil.first().and_then(|b| b.get("art")) {
            Some("column") => false,
            Some("stair") => true,
            _ => continue,
        };
        let Some((lage, erg)) = model.ext_lage(id) else {
            continue;
        };
        let mut stuecke = Vec::new();
        let (mut lo, mut hi) = (f64::INFINITY, f64::NEG_INFINITY);
        for k in &erg.koerper {
            let welt = |u: f64, v: f64, w: f64| lage.welt(k.punkt(u, v, w, 0.0));
            let unten: Vec<Vec3> = k.umriss.iter().map(|q| welt(q[0], q[1], k.von)).collect();
            let oben: Vec<Vec3> = k.umriss.iter().map(|q| welt(q[0], q[1], k.bis)).collect();
            for p in unten.iter().chain(&oben) {
                lo = lo.min(p.z);
                hi = hi.max(p.z);
            }
            let flach = |p: &Vec3| vec3(p.x, p.y, 0.0);
            let senkrecht = unten
                .first()
                .zip(oben.first())
                .is_some_and(|(a, b)| (flach(b) - flach(a)).length() < 1e-6);
            if senkrecht {
                // Grundriss ist der Umriss selbst, konvex ganz, sonst in
                // Dreiecken
                let pts = polygon::to_ccw(&unten.iter().map(flach).collect::<Vec<_>>());
                if konvex(&pts) {
                    stuecke.push(pts);
                } else {
                    for t in polygon::triangulate(&pts) {
                        stuecke.push(polygon::to_ccw(&[pts[t[0]], pts[t[1]], pts[t[2]]]));
                    }
                }
            } else {
                // Liegend ausgezogen (Treppenlauf): die Hülle der Projektion
                let alle: Vec<Vec3> = unten.iter().chain(&oben).map(flach).collect();
                let h = huelle(&alle);
                if h.len() >= 3 {
                    stuecke.push(h);
                }
            }
        }
        if stuecke.is_empty() {
            continue;
        }
        let steigungen = treppe
            .then(|| {
                d.werte(p, &model.ext_geschoss(e.storey))
                    .get("stufen")
                    .copied()
            })
            .flatten();
        out.entry(e.storey).or_default().push(Einbau {
            id,
            treppe,
            stuecke,
            z: (lo, hi),
            steigungen,
        });
    }
    out
}

/// Ist der Ring (gegen den Uhrzeigersinn) konvex?
fn konvex(p: &[Vec3]) -> bool {
    let n = p.len();
    n >= 3
        && (0..n).all(|i| {
            let (a, b, c) = (p[i], p[(i + 1) % n], p[(i + 2) % n]);
            (b.x - a.x) * (c.y - b.y) - (b.y - a.y) * (c.x - b.x) >= -1e-9
        })
}

/// Konvexe Hülle (z = 0), gegen den Uhrzeigersinn.
fn huelle(p: &[Vec3]) -> Vec<Vec3> {
    let mut p: Vec<Vec3> = p.to_vec();
    p.sort_by(|a, b| a.x.total_cmp(&b.x).then(a.y.total_cmp(&b.y)));
    p.dedup_by(|a, b| (a.x - b.x).abs() < 1e-9 && (a.y - b.y).abs() < 1e-9);
    if p.len() < 3 {
        return p;
    }
    let kreuz = |o: Vec3, a: Vec3, b: Vec3| (a.x - o.x) * (b.y - o.y) - (a.y - o.y) * (b.x - o.x);
    let mut h: Vec<Vec3> = Vec::with_capacity(2 * p.len());
    for pass in 0..2 {
        let start = h.len();
        let it: Box<dyn Iterator<Item = &Vec3>> = if pass == 0 {
            Box::new(p.iter())
        } else {
            Box::new(p.iter().rev())
        };
        for &q in it {
            while h.len() >= start + 2 && kreuz(h[h.len() - 2], h[h.len() - 1], q) <= 1e-9 {
                h.pop();
            }
            h.push(q);
        }
        h.pop();
    }
    h
}

/// Schnitt des Polygons `p` mit dem konvexen `c` (gegen den
/// Uhrzeigersinn), Sutherland–Hodgman.
fn schneiden(p: &[Vec3], c: &[Vec3]) -> Vec<Vec3> {
    let mut out: Vec<Vec3> = p.to_vec();
    for i in 0..c.len() {
        let (a, b) = (c[i], c[(i + 1) % c.len()]);
        let innen = |q: Vec3| (b.x - a.x) * (q.y - a.y) - (b.y - a.y) * (q.x - a.x) >= 0.0;
        let schnitt = |p: Vec3, q: Vec3| {
            let (dx, dy) = (q.x - p.x, q.y - p.y);
            let (ex, ey) = (b.x - a.x, b.y - a.y);
            let nenner = dx * ey - dy * ex;
            let t = if nenner.abs() < 1e-12 {
                0.0
            } else {
                ((a.x - p.x) * ey - (a.y - p.y) * ex) / nenner
            };
            vec3(p.x + dx * t, p.y + dy * t, 0.0)
        };
        let ein = std::mem::take(&mut out);
        for j in 0..ein.len() {
            let (p, q) = (ein[j], ein[(j + 1) % ein.len()]);
            match (innen(p), innen(q)) {
                (true, true) => out.push(q),
                (true, false) => out.push(schnitt(p, q)),
                (false, true) => {
                    out.push(schnitt(p, q));
                    out.push(q);
                }
                (false, false) => {}
            }
        }
        if out.is_empty() {
            break;
        }
    }
    out
}

/// Fläche (mm²) des Einbaus im Raum, dessen Fertigumriss in Dreiecken
/// `raum` vorliegt.
pub fn im_raum(e: &Einbau, raum: &[[Vec3; 3]]) -> f64 {
    let mut f = 0.0;
    for s in &e.stuecke {
        for t in raum {
            let q = schneiden(t, s);
            if q.len() >= 3 {
                f += polygon::area(&q);
            }
        }
    }
    f
}

/// Fertigumriss in Dreiecken (z = 0).
pub fn dreiecke(umriss: &[Vec3]) -> Vec<[Vec3; 3]> {
    let p: Vec<Vec3> = umriss.iter().map(|p| vec3(p.x, p.y, 0.0)).collect();
    polygon::triangulate(&p)
        .into_iter()
        .map(|t| [p[t[0]], p[t[1]], p[t[2]]])
        .collect()
}

/// Seitenlängen (mm), wenn der Einbau ein Rechteck ganz im Raum ist.
pub fn rechteck(e: &Einbau, f: f64) -> Option<(f64, f64)> {
    let [s] = e.stuecke.as_slice() else {
        return None;
    };
    if s.len() != 4 || (polygon::area(s) - f).abs() > 1.0 {
        return None;
    }
    let (a, b, c) = (s[1] - s[0], s[2] - s[1], s[3] - s[2]);
    let rechtwinklig = a.dot(b).abs() < 1e-6 * a.length() * b.length()
        && b.dot(c).abs() < 1e-6 * b.length() * c.length();
    rechtwinklig.then(|| (a.length(), b.length()))
}
