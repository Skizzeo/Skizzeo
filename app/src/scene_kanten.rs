//! Raster für den Strahl unter der Maus an Außenwandöffnungen (Paket O7,
//! Review W43 B1): Schenkel der Öffnungen und Wandzüge je Feld, einmal je
//! Modellstand gesammelt. Der Strahl läuft nur durch die Felder, die er
//! kreuzt, und hört auf, sobald dahinter nichts Näheres mehr liegen kann.
//! So bleibt jede Mausbewegung gleich teuer, wie groß das Modell auch ist.

use super::{ray_hits_box, Aabb, Scene, Vis};
use crate::visible::Class;
use sk_math::{vec3, Vec3};
use sk_model::{ElementId, ElementKind, RunId, Tri};
use std::collections::{HashMap, HashSet};

/// Kantenlänge eines Felds (mm).
const FELD: f64 = 5000.0;
/// Rand um die Schenkel (mm): So weit neben einer Kante greift die Maus
/// noch, auch wenn der Strahl das Feld der Öffnung knapp verfehlt.
const RAND: f64 = 1000.0;
/// Was über mehr Felder reicht, wird bei jedem Strahl geprüft.
const MAX_FELDER: i64 = 256;
/// Weiter läuft kein Strahl (Felder).
const MAX_SCHRITTE: usize = 20_000;
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

type Feld = (i64, i64);

/// Die Raster eines Modellstands.
#[derive(Default)]
pub(super) struct Raster {
    /// Modellstand und ob die Züge dabei fertig gerechnet waren.
    stand: Option<(u64, bool)>,
    schenkel: Vec<Schenkel>,
    oeffnungen: HashMap<Feld, Vec<usize>>,
    zuege: HashMap<Feld, Vec<RunId>>,
    /// Über sehr viele Felder: bei jedem Strahl.
    gross_o: Vec<usize>,
    gross_z: Vec<RunId>,
    /// Quader um alles.
    rahmen: Option<Aabb>,
}

fn feld(v: f64) -> i64 {
    (v / FELD).floor() as i64
}

fn vereinen(r: &mut Option<Aabb>, (lo, hi): Aabb) {
    *r = Some(match *r {
        Some((a, b)) => (
            vec3(a.x.min(lo.x), a.y.min(lo.y), a.z.min(lo.z)),
            vec3(b.x.max(hi.x), b.y.max(hi.y), b.z.max(hi.z)),
        ),
        None => (lo, hi),
    });
}

/// Trägt `t` in die Felder des Quaders ein; `false`, wenn er zu groß ist.
fn eintragen<T: Copy>(felder: &mut HashMap<Feld, Vec<T>>, (lo, hi): Aabb, t: T) -> bool {
    let (x0, x1, y0, y1) = (feld(lo.x), feld(hi.x), feld(lo.y), feld(hi.y));
    if (x1 - x0 + 1).saturating_mul(y1 - y0 + 1) > MAX_FELDER {
        return false;
    }
    for x in x0..=x1 {
        for y in y0..=y1 {
            felder.entry((x, y)).or_default().push(t);
        }
    }
    true
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
    /// Baut das Raster neu, wenn sich das Modell geändert hat oder die
    /// Züge seitdem fertig gerechnet sind.
    fn kanten_raster(&self) {
        let stand = (
            self.model.revision(),
            self.dirty.is_empty() && !self.all_dirty,
        );
        if self.kanten.borrow().stand == Some(stand) {
            return;
        }
        let mut r = Raster {
            stand: Some(stand),
            ..Raster::default()
        };
        let m = &self.model;
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
                let i = r.schenkel.len();
                r.schenkel.push(Schenkel {
                    id,
                    k,
                    n,
                    a,
                    b,
                    z0,
                    z1,
                });
                if !eintragen(&mut r.oeffnungen, (lo, hi), i) {
                    r.gross_o.push(i);
                }
                vereinen(&mut r.rahmen, (lo, hi));
            }
        }
        for c in self.cache.iter().flatten() {
            let Some(b) = c.bounds else {
                continue;
            };
            if !eintragen(&mut r.zuege, b, c.id) {
                r.gross_z.push(c.id);
            }
            vereinen(&mut r.rahmen, b);
        }
        *self.kanten.borrow_mut() = r;
    }

    /// Erster deckender Treffer des Strahls am Zug `run`.
    fn zug_strahl(&self, run: RunId, o: Vec3, d: Vec3, vis: &Vis, filter: bool) -> Option<f64> {
        let c = self.cache.get(run.index() as usize)?.as_ref()?;
        if c.id != run || !c.bounds.is_some_and(|b| ray_hits_box(o, d, b)) {
            return None;
        }
        let keep = |t: &Tri| !filter || vis.class(run, t.elem, t.layer) == Class::Solid;
        c.solid.raycast_where(o, d, keep).map(|h| h.0)
    }

    /// Läuft den Strahl `o` + t·`d` durch das Raster. `pruefe` testet einen
    /// Schenkel und gibt den Abstand, wenn die Maus ihn greift. Ergebnis:
    /// der nächste Schenkeltreffer und die erste deckende Wand, beide so
    /// weit, wie sie über die Sicht entscheiden (eine Wand mehr als
    /// [`VOR_WAND`] vor dem Schenkel verdeckt ihn).
    pub fn kanten_strahl(
        &self,
        o: Vec3,
        d: Vec3,
        mut pruefe: impl FnMut(&Schenkel) -> Option<f64>,
    ) -> (Option<f64>, Option<f64>) {
        self.kanten_raster();
        let r = self.kanten.borrow();
        let Some((t0, t1)) = r.rahmen.and_then(|b| im_quader(o, d, b)) else {
            return (None, None);
        };
        let filter = self.filtering();
        let vis = Vis {
            model: &self.model,
            anim: self.vis_anim.as_ref(),
            table: &self.vis_table,
            stamp: self.vis_stamp(),
        };
        let (mut best_o, mut best_w): (Option<f64>, Option<f64>) = (None, None);
        let (mut gesehen_o, mut gesehen_z) = (HashSet::new(), HashSet::new());
        let mut feld_pruefen = |os: &[usize], zs: &[RunId]| {
            for &i in os {
                if gesehen_o.insert(i) {
                    if let Some(t) = pruefe(&r.schenkel[i]) {
                        best_o = Some(best_o.map_or(t, |b: f64| b.min(t)));
                    }
                }
            }
            for &z in zs {
                if gesehen_z.insert(z) {
                    if let Some(t) = self.zug_strahl(z, o, d, &vis, filter) {
                        best_w = Some(best_w.map_or(t, |b: f64| b.min(t)));
                    }
                }
            }
            match (best_o, best_w) {
                (Some(a), Some(b)) => Some(a.min(b)),
                (a, b) => a.or(b),
            }
        };
        feld_pruefen(&r.gross_o, &r.gross_z);
        // Felder in der Reihenfolge, in der der Strahl sie kreuzt
        let p = o + d * t0;
        let (mut x, mut y) = (feld(p.x), feld(p.y));
        let schritt = |dv: f64| if dv > 0.0 { 1 } else { -1 };
        let grenze = |c: i64, ov: f64, dv: f64| {
            if dv.abs() < 1e-12 {
                return (f64::INFINITY, f64::INFINITY);
            }
            let rand = if dv > 0.0 { c + 1 } else { c } as f64 * FELD;
            ((rand - ov) / dv, FELD / dv.abs())
        };
        let ((mut tx, dx), (mut ty, dy)) = (grenze(x, o.x, d.x), grenze(y, o.y, d.y));
        let leer = Vec::new();
        let leer_z = Vec::new();
        for _ in 0..MAX_SCHRITTE {
            let aus = tx.min(ty).min(t1);
            let os = r.oeffnungen.get(&(x, y)).unwrap_or(&leer);
            let zs = r.zuege.get(&(x, y)).unwrap_or(&leer_z);
            if let Some(t) = feld_pruefen(os, zs) {
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
