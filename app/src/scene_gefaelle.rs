//! Gefälle des Flachdachs in der Szene (Gefälledämmung G2, G4): Gefälle
//! einstellen, Abläufe vorschlagen, wählen, hinzufügen, löschen und an der
//! Aufkantung ziehen.

use super::Scene;
use sk_math::{vec3, Vec3};
use sk_model::ElementId;

impl Scene {
    /// Dachaufbau zum Bauteil `id`: er selbst oder der zum Attikablech.
    pub(super) fn roof_element(&self, id: ElementId) -> Option<ElementId> {
        let m = &self.model;
        match m.element(id).map(|e| &e.kind) {
            Some(sk_model::ElementKind::Roof { .. }) => Some(id),
            Some(sk_model::ElementKind::Coping { floor }) => m.flat_roof_of(*floor),
            _ => None,
        }
    }

    /// Gefälle des Flachdachs in % (Paneel „Gefälle“, G2): 0 schaltet es
    /// aus, beim ersten Einschalten schlägt Skizzeo die Abläufe vor. Ein
    /// Schritt.
    pub fn set_roof_slope(&mut self, id: ElementId, pct: f64) -> bool {
        let Some(roof) = self.roof_element(id) else {
            return false;
        };
        if self
            .model
            .drainage_of_roof(roof)
            .is_some_and(|d| d.slope == pct)
        {
            return false;
        }
        self.edit_model("Gefälle des Flachdachs", |m| m.set_roof_slope(roof, pct))
    }

    /// Schlägt die Abläufe des Flachdachs neu vor (ersetzt die gesetzten).
    /// Ein Schritt.
    pub fn propose_roof_drains(&mut self, id: ElementId) -> bool {
        let Some(roof) = self.roof_element(id) else {
            return false;
        };
        self.edit_model("Abläufe vorschlagen", |m| m.propose_roof_drains(roof))
    }

    /// Gewählter Ablauf des Flachdachs zum Bauteil `id` (Index wie in
    /// [`Scene::roof_drains`]); `None`, wenn keiner oder er nicht mehr da ist.
    pub fn active_drain(&self, id: ElementId) -> Option<usize> {
        let (roof, drains) = self.roof_drains(id);
        let (r, k) = self.drain?;
        (Some(r) == roof && k < drains.len()).then_some(k)
    }

    /// Wählt den Ablauf `k` des Dachaufbaus `roof` (oder keinen). `true`,
    /// wenn sich die Wahl geändert hat.
    pub fn set_active_drain(&mut self, d: Option<(ElementId, usize)>) -> bool {
        std::mem::replace(&mut self.drain, d) != d
    }

    /// Ein weiterer Ablauf am Flachdach zum Bauteil `id`, dort, wo er am
    /// meisten entlastet; er ist danach gewählt. Ein Schritt.
    pub fn add_roof_drain(&mut self, id: ElementId) -> bool {
        let Some(roof) = self.roof_element(id) else {
            return false;
        };
        if !self.edit_model("Ablauf hinzufügen", |m| m.add_roof_drain(roof)) {
            return false;
        }
        let n = self.roof_drains(roof).1.len();
        self.drain = n.checked_sub(1).map(|k| (roof, k));
        true
    }

    /// Löscht den gewählten Ablauf am Flachdach zum Bauteil `id` (nicht den
    /// letzten). Ein Schritt.
    pub fn remove_roof_drain(&mut self, id: ElementId) -> bool {
        let (Some(roof), Some(k)) = (self.roof_element(id), self.active_drain(id)) else {
            return false;
        };
        let ok = self.edit_model("Ablauf löschen", |m| m.remove_roof_drain(roof, k));
        if ok {
            self.drain = None;
        }
        ok
    }

    /// Abläufe des Flachdachs zum Bauteil `id` (Dachaufbau oder Blech), wie
    /// sie gezeichnet werden: auf der Innenfläche der Aufkantung; leer ohne
    /// Gefälle.
    pub fn roof_drains(&self, id: ElementId) -> (Option<ElementId>, Vec<Vec3>) {
        let Some(roof) = self.roof_element(id) else {
            return (None, Vec::new());
        };
        let Some(sk_model::ElementKind::Roof { floor, drainage }) =
            self.model.element(roof).map(|e| &e.kind)
        else {
            return (None, Vec::new());
        };
        // Index wie im Modell, damit Ziehen den richtigen Ablauf trifft
        let on = self
            .flat_roof_over(*floor)
            .is_some_and(|r| r.slope.is_some());
        let drains = if on {
            drainage.drains.clone()
        } else {
            Vec::new()
        };
        (Some(roof), drains)
    }

    /// Zieht den Ablauf `k` des Dachaufbaus `roof` an den Punkt der
    /// Aufkantung, der `p` am nächsten liegt, mit Abstand zu den Ecken (im
    /// offenen Schritt, G4). `true`, wenn er sich bewegt hat.
    pub fn drag_drain(&mut self, roof: ElementId, k: usize, p: Vec3) -> bool {
        let m = &self.model;
        let Some(sk_model::ElementKind::Roof { floor, drainage }) =
            m.element(roof).map(|e| &e.kind)
        else {
            return false;
        };
        let floor = *floor;
        let Some(outline) = self.flat_roof_over(floor).map(|r| r.outline.clone()) else {
            return false;
        };
        let gap = sk_model::gefaelle::Limits::default().corner_gap;
        let n = outline.len();
        let mut best: Option<(f64, Vec3)> = None;
        for i in 0..n {
            let (a, b) = (outline[i], outline[(i + 1) % n]);
            let len = (b - a).length();
            if len < 1.0 {
                continue;
            }
            let d = (b - a) * (1.0 / len);
            let g = gap.min(len / 2.0);
            let t = (p - a).dot(d).clamp(g, len - g);
            let q = a + d * t;
            let dist = vec3(p.x - q.x, p.y - q.y, 0.0).length();
            if best.is_none_or(|(bd, _)| dist < bd) {
                best = Some((dist, vec3(q.x, q.y, 0.0)));
            }
        }
        let Some((_, q)) = best else {
            return false;
        };
        let mut d = drainage.clone();
        match d.drains.get_mut(k) {
            Some(x) if (*x - q).length() > 0.5 => *x = q,
            _ => return false,
        }
        if !self.model.set_roof_drainage(roof, d) {
            return false;
        }
        if let Some(ak) = self
            .model
            .run_of(floor)
            .and_then(|r| self.model.parapet_above(r))
        {
            self.mark(ak);
        }
        self.rebuild_dirty(true);
        true
    }
}
