//! Breitenschloss (Jörn 12.10. 05:38): Ein Pfadende einer Öffnung lässt
//! sich an das Ende einer Öffnung im Geschoss direkt darüber oder darunter
//! im selben Gebäude koppeln, das in derselben Flucht liegt. Zieht man danach die Breite,
//! gehen die gekoppelten Enden mit, auch über mehrere Geschosse.
//! Gespeichert ist das Schloss an beiden Öffnungen ([`Opening::lock`]); es
//! gilt nur, wenn beide aufeinander zeigen.

use super::*;
use crate::oeffnung::{EndLock, Opening};
use crate::wall::flat;

/// So genau müssen gekoppelte Enden übereinander liegen (mm, Grundriss).
const FLUCHT: f64 = 1.0;
/// So weit quer zur Flucht dürfen gekoppelte Enden auseinanderliegen (mm,
/// Grundriss): eine versetzte Wand darüber, nicht die Gegenfassade
/// (Review #109 B2).
const LOCK_CROSS: f64 = 1000.0;

/// Richtung des Pfadendes `k` nach außen (vom Nachbarpunkt zum Ende).
fn end_dir(o: &Opening, k: usize) -> Option<Vec3> {
    if !o.is_end(k) {
        return None;
    }
    let nb = if k == 0 { 1 } else { k - 1 };
    let d = flat(*o.path.get(k)?) - flat(*o.path.get(nb)?);
    (d.length() > 1e-9).then(|| d.normalized())
}

impl Model {
    /// Die an Pfadende `k` der Öffnung `id` gekoppelten Enden, für die
    /// beide Öffnungen aufeinander zeigen.
    pub fn end_locks(&self, id: ElementId, k: usize) -> Vec<EndLock> {
        let Some(o) = self.opening(id) else {
            return Vec::new();
        };
        let back = EndLock {
            partner: id,
            end: k,
        };
        o.lock
            .iter()
            .filter(|(kk, _)| *kk == k)
            .map(|(_, l)| *l)
            .filter(|l| {
                self.opening(l.partner)
                    .is_some_and(|p| p.lock.contains(&(l.end, back)))
            })
            .collect()
    }

    /// Alle Pfadenden, die über Schlösser mit dem Ende `k` von `id`
    /// verbunden sind (auch über mehrere Geschosse), ohne dieses selbst.
    pub fn locked_ends(&self, id: ElementId, k: usize) -> Vec<(ElementId, usize)> {
        let mut out = vec![(id, k)];
        let mut i = 0;
        while let Some(&(a, ka)) = out.get(i) {
            i += 1;
            for l in self.end_locks(a, ka) {
                if !out.contains(&(l.partner, l.end)) {
                    out.push((l.partner, l.end));
                }
            }
        }
        out.remove(0);
        out
    }

    /// Liegen das Ende `ka` von `a` und das Ende `kb` von `b` übereinander
    /// in einer Flucht (Grundriss, beide Enden zeigen in dieselbe
    /// Richtung, quer höchstens [`LOCK_CROSS`] auseinander), in direkt
    /// übereinanderliegenden Geschossen desselben Gebäudes?
    pub fn ends_aligned(&self, a: ElementId, ka: usize, b: ElementId, kb: usize) -> bool {
        let (Some(oa), Some(ob)) = (self.opening(a), self.opening(b)) else {
            return false;
        };
        let (Some(ea), Some(eb)) = (self.element(a), self.element(b)) else {
            return false;
        };
        let (Some(da), Some(db)) = (end_dir(oa, ka), end_dir(ob, kb)) else {
            return false;
        };
        let d = flat(ob.path[kb]) - flat(oa.path[ka]);
        let laengs = d.dot(da);
        da.dot(db) > 0.999
            && laengs.abs() < FLUCHT
            && (d - da * laengs).length() <= LOCK_CROSS
            && self.storeys_adjacent(ea.storey, eb.storey)
    }

    /// Liegen die Geschosse `a` und `b` desselben Gebäudes direkt
    /// übereinander, ohne ein Geschoss dazwischen?
    pub fn storeys_adjacent(&self, a: StoreyId, b: StoreyId) -> bool {
        let (Some(sa), Some(sb)) = (self.storey(a), self.storey(b)) else {
            return false;
        };
        let (lo, hi) = (
            sa.elevation.min(sb.elevation),
            sa.elevation.max(sb.elevation),
        );
        sa.building.is_some()
            && sa.building == sb.building
            && hi > lo
            && !self.storeys().iter().any(|(id, s)| {
                id != a
                    && id != b
                    && s.building == sa.building
                    && s.elevation > lo
                    && s.elevation < hi
            })
    }

    /// Koppelt (`on`) oder löst das Ende `ka` von `a` mit dem Ende `kb`
    /// von `b`, im offenen Schritt. Koppeln nur, wenn die Enden in einer
    /// Flucht liegen ([`Model::ends_aligned`]). `false`, wenn sich nichts
    /// ändert.
    pub fn set_end_lock(
        &mut self,
        a: ElementId,
        ka: usize,
        b: ElementId,
        kb: usize,
        on: bool,
    ) -> bool {
        if on && !self.ends_aligned(a, ka, b, kb) {
            return false;
        }
        let ab = (
            ka,
            EndLock {
                partner: b,
                end: kb,
            },
        );
        let ba = (
            kb,
            EndLock {
                partner: a,
                end: ka,
            },
        );
        let mut changed = self.edit_locks(a, |l| l.retain(|x| *x != ab));
        changed |= self.edit_locks(b, |l| l.retain(|x| *x != ba));
        if on {
            self.edit_locks(a, |l| l.push(ab));
            self.edit_locks(b, |l| l.push(ba));
            changed = true;
        }
        changed
    }

    /// Ändert die Schlösser der Öffnung `id` mit `f`; `true`, wenn sie sich
    /// ändern.
    fn edit_locks(&mut self, id: ElementId, f: impl FnOnce(&mut Vec<(usize, EndLock)>)) -> bool {
        let Some(old) = self.opening(id).map(|o| o.lock.clone()) else {
            return false;
        };
        let mut new = old.clone();
        f(&mut new);
        if new == old {
            return false;
        }
        note!(self, Element, self.elements, id);
        if let Some(ElementKind::Opening(o)) = self.elements.get_mut(id).map(|e| &mut e.kind) {
            o.lock = new;
        }
        self.touch();
        true
    }
}
