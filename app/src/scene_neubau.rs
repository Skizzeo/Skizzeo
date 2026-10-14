//! Wandzüge der Szene neu rechnen: alle markierten in einem lesenden Gang
//! über das Modell, damit Anschlüsse, Decken und Öffnungen aus dem
//! Verzeichnis kommen statt aus einer Suche über das ganze Modell (L40,
//! L31). Aus `scene.rs` ausgelagert.

use super::*;
use std::collections::BTreeMap;

impl Scene {
    /// Rechnet die markierten Wandzüge neu und entfernt gelöschte. `live`: nur
    /// die Körper, die Mengen erst bei [`Scene::settle`].
    pub(super) fn rebuild_dirty(&mut self, live: bool) {
        if self.table.rev != self.model.attr().rev() {
            self.table = DrawTable::resolve(&self.model, &self.theme);
        }
        let zuege: Vec<(RunId, u32)> = if self.all_dirty {
            self.all_dirty = false;
            self.dirty.clear();
            self.nachzug.clear();
            let builds: Vec<u32> = self
                .cache
                .iter()
                .map(|c| c.as_ref().map_or(0, |c| c.builds))
                .collect();
            self.cache.clear();
            self.grenzen = Grenzen::default();
            self.raster_leeren();
            let b = |id: RunId| builds.get(id.index() as usize).copied().unwrap_or(0);
            self.model.runs().ids().map(|id| (id, b(id))).collect()
        } else {
            let ids = std::mem::take(&mut self.dirty);
            self.nachzug_ohne(&ids);
            ids.into_iter()
                .map(|id| (id, self.cached(id).map_or(0, |c| c.builds)))
                .collect()
        };
        let m = &self.model;
        let neu: Vec<(RunId, Option<RunCache>)> = m.lesend(|| {
            zuege
                .into_iter()
                .map(|(id, b)| (id, rechne(m, id, b, live)))
                .collect()
        });
        for (id, rc) in neu {
            self.store(id, rc, live);
        }
        self.ext.sync(&self.model);
        self.bounds_neu();
    }

    /// Grenzen der Szene: die gezählten Seiten der Körper
    /// ([`Grenzen`]) und die Exemplare.
    pub(super) fn bounds_neu(&mut self) {
        self.bounds = union(self.grenzen.huelle(), self.ext.bounds());
    }

    /// Legt einen neu gerechneten Wandzug ab (oder entfernt ihn, wenn es
    /// ihn nicht mehr gibt) und zählt seine Seiten um ([`Grenzen`]).
    pub(super) fn store(&mut self, id: RunId, rc: Option<RunCache>, live: bool) {
        let slot = id.index() as usize;
        if self.cache.len() <= slot {
            self.cache.resize_with(slot + 1, || None);
        }
        let bleibt = rc.is_none() && !self.cache[slot].as_ref().is_some_and(|c| c.id == id);
        if !bleibt {
            let alt = self.cache[slot].as_ref().and_then(|c| c.bounds);
            self.grenzen.raus(alt);
            self.grenzen.rein(rc.as_ref().and_then(|c| c.bounds));
        }
        match rc {
            Some(mut rc) => {
                if live {
                    if !self.unsettled.contains(&id) {
                        self.unsettled.push(id);
                    }
                } else {
                    self.unsettled.retain(|&u| u != id);
                }
                // Gleicher Körper (ein Nachbar wurde neu gerechnet, dieser Zug
                // blieb): er behält seine Generation und damit die geteilten
                // Körper (Review 3h Befund 2)
                if let Some(old) = self.cache[slot].as_mut() {
                    if old.id == id && old.same_body(&rc) {
                        rc.splits = std::mem::take(&mut old.splits);
                    }
                }
                let q = rc
                    .bounds
                    .map(|b| union(Some(b), rc.foot_bounds).unwrap_or(b));
                self.raster_setzen(slot, q, &rc);
                self.cache[slot] = Some(rc);
            }
            // Gelöscht: Platz nur räumen, wenn kein neuerer Zug darin steht
            None => {
                if self.cache[slot].as_ref().is_some_and(|c| c.id == id) {
                    self.cache[slot] = None;
                    self.raster_entfernen(slot);
                }
            }
        }
    }

    pub(super) fn mark(&mut self, id: RunId) {
        self.mass_zug(id);
        if !self.dirty.contains(&id) {
            self.dirty.push(id);
        }
    }

    pub(super) fn mark_all(&mut self) {
        self.all_dirty = true;
        self.mass_alle = neuer_stand();
        self.unsettled.clear();
    }

    /// Rechnet die Mengen nach, die beim Live-Ziehen ausgelassen wurden.
    pub fn settle(&mut self) {
        let m = &self.model;
        let mengen: Vec<(RunId, Vec<WallQto>)> = m.lesend(|| {
            std::mem::take(&mut self.unsettled)
                .into_iter()
                .filter(|&id| m.run(id).is_some())
                .map(|id| (id, run_qto(m, id)))
                .collect()
        });
        for (id, q) in mengen {
            if let Some(Some(c)) = self.cache.get_mut(id.index() as usize) {
                if c.id == id {
                    c.qto = q;
                    c.found_qto = c.found.as_ref().map(foundation_qto_of);
                    c.floor_qto = c.floor.as_ref().map(floor_qto_of);
                }
            }
        }
    }
}

/// Rechnet einen Wandzug mit `builds` früheren Ständen neu; `None`, wenn
/// es ihn nicht mehr gibt. `live`: ohne Mengen.
pub(super) fn rechne(m: &Model, id: RunId, builds: u32, live: bool) -> Option<RunCache> {
    let (c, floor) = m.chain_and_floor(id)?;
    let found = m.foundation(id).and_then(Result::ok);
    let floor = floor.and_then(Result::ok);
    let roof = m.flat_roof_on(id, &c);
    let mut rc = RunCache::new(id, c, found, floor, roof, builds + 1);
    rc.below = m.run_below(id);
    if !live {
        rc.qto = run_qto(m, id);
        rc.found_qto = rc.found.as_ref().map(foundation_qto_of);
        rc.floor_qto = rc.floor.as_ref().map(floor_qto_of);
    }
    Some(rc)
}

/// Die sechs Seiten aller Körper der Szene, je Wert gezählt: Einsetzen und
/// Entfernen eines Körpers kosten log n, die Hülle ist das jeweils Äußerste.
/// Der Gang über alle Körper je Neubau kostete bei 4000 Häusern beim Ziehen
/// Millisekunden an Cache-Fehlgriffen (L40).
#[derive(Default)]
pub(super) struct Grenzen([BTreeMap<Wert, u32>; 6]);

/// `f64` mit totaler Ordnung als Schlüssel.
#[derive(Clone, Copy, PartialEq)]
struct Wert(f64);

impl Eq for Wert {}

impl PartialOrd for Wert {
    fn partial_cmp(&self, o: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(o))
    }
}

impl Ord for Wert {
    fn cmp(&self, o: &Self) -> std::cmp::Ordering {
        self.0.total_cmp(&o.0)
    }
}

impl Grenzen {
    fn seiten((lo, hi): Aabb) -> [f64; 6] {
        [lo.x, lo.y, lo.z, hi.x, hi.y, hi.z]
    }

    fn rein(&mut self, b: Option<Aabb>) {
        for (m, v) in self
            .0
            .iter_mut()
            .zip(b.map(Self::seiten).into_iter().flatten())
        {
            *m.entry(Wert(v)).or_default() += 1;
        }
    }

    fn raus(&mut self, b: Option<Aabb>) {
        for (m, v) in self
            .0
            .iter_mut()
            .zip(b.map(Self::seiten).into_iter().flatten())
        {
            if let std::collections::btree_map::Entry::Occupied(mut e) = m.entry(Wert(v)) {
                *e.get_mut() -= 1;
                if *e.get() == 0 {
                    e.remove();
                }
            }
        }
    }

    /// Hülle aller gezählten Körper.
    fn huelle(&self) -> Option<Aabb> {
        let unten = |k: usize| self.0[k].keys().next().map(|w| w.0);
        let oben = |k: usize| self.0[k].keys().next_back().map(|w| w.0);
        Some((
            vec3(unten(0)?, unten(1)?, unten(2)?),
            vec3(oben(3)?, oben(4)?, oben(5)?),
        ))
    }
}
