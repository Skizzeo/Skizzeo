//! Bemaßung bedienen (Konzept `bemassung/konzept.md` §4, §5): Versatz einer
//! Kette und Öffnungen, die eine überschriebene Maßzahl im offenen Schritt
//! verschiebt. Aus `scene.rs` ausgelagert.

use super::Scene;
use sk_model::bemassung::Kettenart;
use sk_model::oeffnung::Opening;
use sk_model::{ElementId, StoreyId};

impl Scene {
    /// Versatz einer Maßkette im offenen Schritt. Nur das Bild der Ketten
    /// ändert sich, kein Wandzug wird neu gerechnet.
    pub fn set_massversatz(&mut self, st: StoreyId, seite: i32, art: Kettenart, v: f64) -> bool {
        debug_assert!(self.model.in_step(), "Versatz ohne Schritt");
        self.model.set_massversatz(st, seite, art, v)
    }

    /// Ändert die Öffnung `id` im offenen Schritt (Maßzahl überschrieben).
    /// Die Züge, die sie berührt, gehen live mit wie beim Ziehen; `false`,
    /// wenn sie so nicht stehen kann.
    pub fn set_opening_live(&mut self, id: ElementId, o: Opening) -> bool {
        if !self.model.set_opening(id, o) {
            return false;
        }
        for r in self.model.step_touched() {
            self.mark(r);
            if !self.live.contains(&r) {
                self.live.push(r);
            }
        }
        self.rebuild_dirty(true);
        true
    }
}
