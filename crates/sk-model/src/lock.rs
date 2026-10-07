//! Sperren (Paket 4 §2.2): Ein gesperrtes Bauteil bleibt sichtbar, wählbar
//! und lesbar, aber keine Handlung ändert seine Werte oder seine Lage.
//! Abgeleitete Bauteile (Decke, Sohlplatte, Schürze, UD, DT, AB, RD) haben
//! kein eigenes Schloss: Sie folgen ihrer Quelle am Ende der Kette
//! [`Refusal::Derived`] und ändern sich weiter mit ihren Nachbarn.

use super::*;

/// Ein Schritt hätte das gesperrte Bauteil geändert; er ist zurückgerollt.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Locked(pub ElementId);

/// Das erste gesperrte Bauteil, das eine Handlung an `ids` ändern würde
/// (Prüfung beim Greifen): die Quelle mit dem Schloss, `None` wenn frei.
pub fn edit_blocked(m: &Model, ids: &[ElementId]) -> Option<ElementId> {
    ids.iter()
        .find(|&&id| m.is_locked(id))
        .map(|&id| m.lock_source(id))
}

impl Model {
    /// Sperrt oder entsperrt im offenen Schritt („Gesperrt“, „Entsperrt“).
    pub fn set_locked(&mut self, ids: &[ElementId], locked: bool) {
        for &id in ids {
            if self.element(id).is_none_or(|e| e.locked == locked) {
                continue;
            }
            note!(self, Element, self.elements, id);
            if let Some(e) = self.elements.get_mut(id) {
                e.locked = locked;
                let g = e.guid;
                if locked && !self.lock_order.contains(&g) {
                    self.lock_order.push(g);
                }
            }
            self.touch();
        }
    }

    /// Sperre beim Laden, ohne Schritt und ohne neue Revision.
    pub(crate) fn load_lock(&mut self, id: ElementId) {
        if let Some(e) = self.elements.get_mut(id) {
            e.locked = true;
            let g = e.guid;
            if !self.lock_order.contains(&g) {
                self.lock_order.push(g);
            }
        }
    }

    /// Gesperrte Bauteile in der Reihenfolge ihres Sperrens (aus der Datei
    /// zuerst), Unbekanntes danach nach Kennung.
    pub(crate) fn locked_in_order(&self) -> Vec<Guid> {
        let rank = |g: &Guid| {
            self.lock_order
                .iter()
                .position(|x| x == g)
                .unwrap_or(usize::MAX)
        };
        let mut v: Vec<Guid> = self
            .elements
            .iter()
            .filter(|(_, e)| e.locked)
            .map(|(_, e)| e.guid)
            .collect();
        v.sort_by_key(|g| (rank(g), *g));
        v
    }

    /// Bauteil, dessen Schloss für `id` gilt: am Ende der Kette
    /// [`Refusal::Derived`] (DT → DE → AW, FS → SP → AW), sonst es selbst.
    pub fn lock_source(&self, id: ElementId) -> ElementId {
        let mut at = id;
        for _ in 0..16 {
            match self.can_delete(at) {
                Err(Refusal::Derived { from }) if from != at => at = from,
                _ => break,
            }
        }
        at
    }

    /// Wände, deren Typ ein Typwechsel an `runs` ändert: die Züge samt
    /// gekoppeltem Stapel darunter und darüber ([`Model::set_run_type`]).
    pub fn type_set(&self, runs: &[RunId]) -> Vec<ElementId> {
        let mut at: Vec<RunId> = runs.to_vec();
        let mut k = 0;
        while k < at.len() {
            let r = at[k];
            let mut next = self.stack_above(r);
            if let Some(run) = self.run(r) {
                next.extend(
                    run.segments
                        .iter()
                        .filter_map(|&e| self.wall_below(e))
                        .filter_map(|b| self.segment_of(b).map(|s| s.0)),
                );
            }
            for n in next {
                if !at.contains(&n) {
                    at.push(n);
                }
            }
            k += 1;
        }
        at.iter()
            .filter_map(|&r| self.run(r))
            .flat_map(|r| r.segments.iter().copied())
            .collect()
    }

    /// Gilt für `id` eine Sperre (die seiner Quelle)?
    pub fn is_locked(&self, id: ElementId) -> bool {
        self.element(self.lock_source(id)).is_some_and(|e| e.locked)
    }

    /// Sicherheitsnetz für [`Model::try_commit`]: ein gesperrtes Bauteil
    /// ohne Quelle, das der offene Schritt ändert. Nur `locked` selbst zu
    /// ändern zählt nicht; bei Wänden zählen nur Lage und Typ.
    pub(crate) fn locked_change(&self) -> Option<ElementId> {
        let open = self.txn.as_ref()?;
        // War das Bauteil schon vor dem Schritt gesperrt?
        let before = |id: ElementId| {
            open.changes
                .iter()
                .find_map(|c| match c {
                    Change::Element { id: x, old, .. } if *x == id => Some(old.as_ref()),
                    _ => None,
                })
                .unwrap_or(self.elements.get(id))
                .is_some_and(|e| e.locked)
        };
        for c in &open.changes {
            match c {
                Change::Element {
                    id, old: Some(o), ..
                } if o.locked => {
                    let Some(n) = self.elements.get(*id) else {
                        return Some(*id);
                    };
                    if !n.locked || self.lock_source(*id) != *id {
                        continue;
                    }
                    let changed = match n.kind {
                        ElementKind::Wall(_) => o.layer_set != n.layer_set,
                        _ => o != n,
                    };
                    if changed {
                        return Some(*id);
                    }
                }
                Change::Run { old: Some(o), .. } => {
                    let ends = |r: &WallRun, k: usize| {
                        let n = r.points.len();
                        (r.points[k % n], r.points[(k + 1) % n])
                    };
                    for (k, &e) in o.segments.iter().enumerate() {
                        if !before(e) || !self.elements.get(e).is_some_and(|x| x.locked) {
                            continue;
                        }
                        // Das Segment kann nach einem Teilen in einem
                        // anderen Zug liegen
                        let now = self
                            .segment_of(e)
                            .and_then(|(r, kk)| Some(ends(self.runs.get(r)?, kk)));
                        let (a, b) = ends(o, k);
                        let same = now.is_some_and(|(c, d)| {
                            (a - c).length() < 1e-6 && (b - d).length() < 1e-6
                        });
                        if !same {
                            return Some(e);
                        }
                    }
                }
                _ => {}
            }
        }
        None
    }
}
