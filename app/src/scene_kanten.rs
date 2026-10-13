//! Raster für den Strahl unter der Maus an Außenwandöffnungen (Paket O7,
//! Review W43 B1): die Schenkel der Öffnungen, einmal je Modellstand
//! gesammelt, im selben Raster wie die Wandzüge der Szene (Review H2 zu
//! #142). Der Strahl läuft nur durch die Zellen, die er kreuzt, und hört
//! auf, sobald dahinter nichts Näheres mehr liegen kann. So bleibt jede
//! Mausbewegung gleich teuer, wie groß das Modell auch ist.

use super::raster::{self, ZELLE};
use super::{ray_hits_box, Aabb, Scene, Vis};
use crate::visible::Class;
use sk_math::{vec3, Vec3};
use sk_model::{ElementId, ElementKind, Tri};
use std::collections::HashSet;

/// Fester Rand um die Schenkel (mm); dazu kommt je Strahl der Spielraum,
/// in dem die Maus greift ([`Scene::kanten_strahl`]).
const RAND: f64 = 1000.0;
/// Abstand, um den eine Wand vor der Öffnung liegen darf (die eigene
/// Fläche, knapp neben dem Loch).
pub const VOR_WAND: f64 = 20.0;

/// Ein Schenkel einer Öffnung auf ihrer Außenfläche (flach) mit ihrem Band.
#[derive(Clone, Copy, Debug)]
pub struct Schenkel {
    pub id: ElementId,
    /// Stelle im Pfad und Zahl der Schenkel.
    pub k: usize,
    pub n: usize,
    pub a: Vec3,
    pub b: Vec3,
    pub z0: f64,
    pub z1: f64,
}

/// Die Schenkel eines Modellstands; die Wandzüge stehen im Raster der
/// Szene, das jeder neu berechnete Zug selbst nachführt.
#[derive(Default)]
pub(super) struct Raster {
    stand: Option<u64>,
    schenkel: Vec<Schenkel>,
    /// Je Schenkel sein Quader mit [`RAND`].
    felder: raster::Raster,
}

/// Abschnitt des Strahls im Quader: Eintritt und Austritt.
fn im_quader(o: Vec3, d: Vec3, (lo, hi): Aabb) -> Option<(f64, f64)> {
    let (mut t0, mut t1) = (0.0f64, f64::INFINITY);
    for (o, d, lo, hi) in [
        (o.x, d.x, lo.x, hi.x),
        (o.y, d.y, lo.y, hi.y),
        (o.z, d.z, lo.z, hi.z),
    ] {
        if d.abs() < 1e-12 {
            if o < lo || o > hi {
                return None;
            }
            continue;
        }
        let (a, b) = ((lo - o) / d, (hi - o) / d);
        t0 = t0.max(a.min(b));
        t1 = t1.min(a.max(b));
    }
    (t0 <= t1).then_some((t0, t1))
}

impl Scene {
    /// Sammelt die Schenkel neu, wenn sich das Modell geändert hat.
    fn kanten_raster(&self) {
        let stand = self.model.revision();
        if self.kanten.borrow().stand == Some(stand) {
            return;
        }
        let m = &self.model;
        let mut schenkel = Vec::new();
        let mut quader = Vec::new();
        for (id, e) in m.elements().iter() {
            let ElementKind::Opening(op) = &e.kind else {
                continue;
            };
            let Some((legs, z0, z1)) = crate::wall_edit::oeffnung_kanten::schenkel(m, op) else {
                continue;
            };
            let n = legs.len();
            for (k, &(a, b)) in legs.iter().enumerate() {
                let lo = vec3(a.x.min(b.x) - RAND, a.y.min(b.y) - RAND, z0 - RAND);
                let hi = vec3(a.x.max(b.x) + RAND, a.y.max(b.y) + RAND, z1 + RAND);
                schenkel.push(Schenkel {
                    id,
                    k,
                    n,
                    a,
                    b,
                    z0,
                    z1,
                });
                quader.push(Some((lo, hi)));
            }
        }
        *self.kanten.borrow_mut() = Raster {
            stand: Some(stand),
            schenkel,
            felder: raster::Raster::aus(quader),
        };
    }

    /// Sichtbarkeit für [`Scene::zug_strahl`] und ob sie filtert.
    fn sicht(&self) -> (Vis<'_>, bool) {
        let vis = Vis {
            model: &self.model,
            anim: self.vis_anim.as_ref(),
            table: &self.vis_table,
            stamp: self.vis_stamp(),
        };
        (vis, self.filtering())
    }

    /// Erster deckender Treffer des Strahls am Zug auf Platz `platz`.
    fn zug_strahl(&self, platz: usize, o: Vec3, d: Vec3, vis: &Vis, filter: bool) -> Option<f64> {
        let c = self.cache.get(platz)?.as_ref()?;
        if !c.bounds.is_some_and(|b| ray_hits_box(o, d, b)) {
            return None;
        }
        let run = c.id;
        let keep = |t: &Tri| !filter || vis.class(run, t.elem, t.layer) == Class::Solid;
        c.solid.raycast_where(o, d, keep).map(|h| h.0)
    }

    /// Läuft den Strahl `o` + t·`d` (aus dem Auge, `d` Länge 1) durch das
    /// Raster. `pruefe` testet einen Schenkel und gibt den Abstand, wenn die
    /// Maus ihn greift; `griff` sagt, wie weit (mm) sie an einem Punkt des
    /// Strahls höchstens greift. Ergebnis: der nächste Schenkeltreffer und
    /// die erste deckende Wand, beide so weit, wie sie über die Sicht
    /// entscheiden (eine Wand mehr als [`VOR_WAND`] vor dem Schenkel
    /// verdeckt ihn).
    pub fn kanten_strahl(
        &self,
        o: Vec3,
        d: Vec3,
        griff: impl Fn(Vec3) -> f64,
        mut pruefe: impl FnMut(&Schenkel) -> Option<f64>,
    ) -> (Option<f64>, Option<f64>) {
        self.kanten_raster();
        let r = self.kanten.borrow();
        let (felder, zuege) = (&r.felder, &self.raster);
        let Some((lo, hi)) = super::union(felder.quader(), zuege.quader()) else {
            return (None, None);
        };
        // Spielraum um den Strahl (Review H1 zu #147): der größte Griff am
        // fernsten Punkt, an dem der Strahl einem Schenkel nahekommen kann;
        // ein Bildpunkt wächst längs des Strahls mit der Tiefe
        let fern = (0..8)
            .map(|i| {
                let c = vec3(
                    if i & 1 == 0 { lo.x } else { hi.x },
                    if i & 2 == 0 { lo.y } else { hi.y },
                    if i & 4 == 0 { lo.z } else { hi.z },
                );
                (c - o).dot(d)
            })
            .fold(0.0, f64::max);
        let mut spiel = 0.0;
        for _ in 0..3 {
            spiel = griff(o + d * (fern + spiel)).max(0.0);
        }
        let spiel = if spiel.is_finite() { spiel } else { 1e12 };
        let rahmen = (
            lo - vec3(spiel, spiel, spiel),
            hi + vec3(spiel, spiel, spiel),
        );
        let Some((t0, t1)) = im_quader(o, d, rahmen) else {
            return (None, None);
        };
        let (vis, filter) = self.sicht();
        let (mut best_o, mut best_w): (Option<f64>, Option<f64>) = (None, None);
        let (mut gesehen_o, mut gesehen_z) = (HashSet::new(), HashSet::new());
        let mut pruefen = |os: &[u32], zs: &[u32]| {
            for &i in os {
                if gesehen_o.insert(i) {
                    if let Some(t) = pruefe(&r.schenkel[i as usize]) {
                        best_o = Some(best_o.map_or(t, |b: f64| b.min(t)));
                    }
                }
            }
            for &z in zs {
                if gesehen_z.insert(z) {
                    if let Some(t) = self.zug_strahl(z as usize, o, d, &vis, filter) {
                        best_w = Some(best_w.map_or(t, |b: f64| b.min(t)));
                    }
                }
            }
            match (best_o, best_w) {
                (Some(a), Some(b)) => Some(a.min(b)),
                (a, b) => a.or(b),
            }
        };
        pruefen(felder.immer(), zuege.immer());
        // Zellen in der Reihenfolge, in der der Strahl sie kreuzt, für die
        // Schenkel mit den Nachbarn im Spielraum; wären es mehr als
        // Einträge, gleich alle (ohne Raster)
        let (p, q) = (o + d * t0, o + d * t1);
        let (mut x, mut y) = (raster::zelle(p.x), raster::zelle(p.y));
        let schritte = (raster::zelle(q.x) as i64 - x as i64).abs()
            + (raster::zelle(q.y) as i64 - y as i64).abs();
        let alle = |r: &raster::Raster| r.alle().into_iter().map(|p| p as u32).collect::<Vec<_>>();
        if schritte as usize >= felder.plaetze() + zuege.plaetze() {
            pruefen(&alle(felder), &alle(zuege));
            return (best_o, best_w);
        }
        let k = (spiel / ZELLE).ceil().min(1e6) as i32;
        let nachbarn = (2 * k as i64 + 1).pow(2);
        let k = if nachbarn.saturating_mul(schritte + 1) >= felder.plaetze() as i64 {
            pruefen(&alle(felder), &[]);
            None
        } else {
            Some(k)
        };
        let schritt = |dv: f64| if dv > 0.0 { 1 } else { -1 };
        let grenze = |c: i32, ov: f64, dv: f64| {
            if dv.abs() < 1e-12 {
                return (f64::INFINITY, f64::INFINITY);
            }
            let rand = if dv > 0.0 { c + 1 } else { c } as f64 * ZELLE;
            ((rand - ov) / dv, ZELLE / dv.abs())
        };
        let ((mut tx, dx), (mut ty, dy)) = (grenze(x, o.x, d.x), grenze(y, o.y, d.y));
        for _ in 0..=schritte {
            let aus = tx.min(ty).min(t1);
            let mut naechst = pruefen(&[], zuege.in_zelle((x, y)));
            let um = || k.into_iter().flat_map(|k| -k..=k);
            for j in um() {
                for i in um() {
                    let c = (x.saturating_add(i), y.saturating_add(j));
                    naechst = pruefen(felder.in_zelle(c), &[]);
                }
            }
            if let Some(t) = naechst {
                if t + VOR_WAND <= aus {
                    break;
                }
            }
            if aus >= t1 {
                break;
            }
            if tx < ty {
                (x, tx) = (x + schritt(d.x), tx + dx);
            } else {
                (y, ty) = (y + schritt(d.y), ty + dy);
            }
        }
        (best_o, best_w)
    }
}

#[cfg(test)]
mod tests {
    use super::super::raster::tests::{kameras, siedlung, H, W};
    use super::*;
    use crate::camera::Camera;

    /// Ein Schenkel greift, wenn der Strahl seine Fläche bis `tol` mm neben
    /// dem Band trifft.
    fn greift(s: &Schenkel, o: Vec3, d: Vec3, tol: f64) -> Option<f64> {
        let len = (s.b - s.a).length();
        let dir = (s.b - s.a) * (1.0 / len);
        let nrm = vec3(-dir.y, dir.x, 0.0);
        let dn = d.dot(nrm);
        if len < 1.0 || dn.abs() < 1e-9 {
            return None;
        }
        let t = (s.a - o).dot(nrm) / dn;
        let p = o + d * t;
        let u = (p - s.a).dot(dir);
        let im_band = u > -tol && u < len + tol && p.z > s.z0 - tol && p.z < s.z1 + tol;
        (t > 0.0 && im_band).then_some(t)
    }

    /// Nächster Schenkel über alle Öffnungen des Modells und nächste Wand
    /// über alle Züge, ohne Raster.
    fn ueber_alle(s: &Scene, o: Vec3, d: Vec3, tol: f64) -> (Option<f64>, Option<f64>) {
        let m = s.model();
        let min = |a: Option<f64>, t: Option<f64>| match (a, t) {
            (Some(a), Some(t)) => Some(a.min(t)),
            (a, t) => a.or(t),
        };
        let mut best_o = None;
        for (id, e) in m.elements().iter() {
            let ElementKind::Opening(op) = &e.kind else {
                continue;
            };
            let Some((legs, z0, z1)) = crate::wall_edit::oeffnung_kanten::schenkel(m, op) else {
                continue;
            };
            for (k, &(a, b)) in legs.iter().enumerate() {
                let n = legs.len();
                let sch = Schenkel {
                    id,
                    k,
                    n,
                    a,
                    b,
                    z0,
                    z1,
                };
                best_o = min(best_o, greift(&sch, o, d, tol));
            }
        }
        let (vis, filter) = s.sicht();
        let best_w = (0..s.cache.len())
            .filter_map(|p| s.zug_strahl(p, o, d, &vis, filter))
            .min_by(f64::total_cmp);
        (best_o, best_w)
    }

    /// Über das gemeinsame Raster der Szene entscheidet der Strahl wie über
    /// alle Schenkel und Züge: der vorderste Schenkel, wenn keine Wand mehr
    /// als [`VOR_WAND`] davor liegt, sonst die Wand; auch nach Ziehen einer
    /// Wand, ohne dass sich die Schenkel neu sammeln (Review H2 zu #142).
    /// Greift die Maus weiter, als der feste Rand um die Schenkel reicht,
    /// zählen die Nachbarzellen im Spielraum mit (Review H1 zu #147).
    #[test]
    fn strahl_wie_ueber_alle() {
        let mut s = siedlung();
        let pruefen = |s: &Scene, tol: f64| {
            let mut kameras = kameras().to_vec();
            kameras.push(Camera::looking_at(
                vec3(60_000.0, -50_000.0, 8000.0),
                vec3(60_000.0, 60_000.0, 0.0),
                45.0,
            ));
            let mut fenster = 0;
            for cam in &kameras {
                let mut px: Vec<(f64, f64)> = (0..24 * 14)
                    .map(|k| ((k % 24) as f64 * W / 23.0, (k / 24) as f64 * H / 13.0))
                    .collect();
                // Auf jedes Fenster und knapp daneben
                for (_, e) in s.model().elements().iter() {
                    let ElementKind::Opening(op) = &e.kind else {
                        continue;
                    };
                    let (z0, z1) = s.model().opening_band(op).unwrap();
                    let q = (op.path[0] + op.path[1]) * 0.5;
                    if let Some((x, y)) = cam.project(vec3(q.x, q.y, (z0 + z1) / 2.0), W, H) {
                        px.extend([(x, y), (x + 6.0, y), (x, y - 40.0), (x - 60.0, y)]);
                        px.extend([(x + 60.0, y), (x, y + 60.0), (x + 40.0, y - 40.0)]);
                    }
                }
                for (x, y) in px {
                    let (o, d) = cam.ray(x, y, W, H);
                    let (bo, bw) = s.kanten_strahl(o, d, |_| tol, |sch| greift(sch, o, d, tol));
                    let (go, gw) = ueber_alle(s, o, d, tol);
                    let vorn = |a: Option<f64>, b: Option<f64>| match (a, b) {
                        (Some(a), Some(b)) => Some(a.min(b)),
                        (a, b) => a.or(b),
                    };
                    assert_eq!(vorn(bo, bw), vorn(go, gw), "{x} {y}");
                    if let Some(t) = go.filter(|&t| gw.is_none_or(|w| t < w + VOR_WAND)) {
                        assert_eq!(bo, Some(t), "{x} {y}: Schenkel");
                        fenster += 1;
                    }
                    if let Some(w) = gw.filter(|&w| go.is_none_or(|t| w < t)) {
                        assert_eq!(bw, Some(w), "{x} {y}: Wand");
                    }
                }
            }
            assert!(fenster > 100, "{fenster} Fenster");
        };
        pruefen(&s, 300.0);
        pruefen(&s, 6000.0);
        // Eine Wand ziehen: die Schenkel bleiben, die Züge kommen aus dem
        // Raster der Szene
        let run = s.model().runs().ids().nth(10).unwrap();
        let moved = s.chain(run).unwrap().with_segment_moved(1, 3000.0).unwrap();
        s.begin("Wand verschieben");
        assert!(s.set_run_points(run, &moved.points));
        s.commit();
        s.rebuild_dirty(false);
        pruefen(&s, 300.0);
        pruefen(&s, 6000.0);
    }
}
