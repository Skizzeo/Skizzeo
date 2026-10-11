//! Regenfallrohr in der Szene (Jörn 11.10.): Knöpfe und Felder im Paneel
//! „Eigenschaften“, je ein Schritt.

use super::Scene;
use crate::selection::fallrohr::Action;
use sk_model::fallrohr::{DownpipeSpec, Overflow};
use sk_model::ElementId;

impl Scene {
    /// Knopf unter einem Abschnitt der Eigenschaften des Bauteils `id`.
    pub fn props_action(&mut self, id: ElementId, a: Action) -> bool {
        match a {
            Action::Overflow => self.edit_downpipe(id, "Notentwässerung", |s| {
                s.overflow = match s.overflow {
                    Overflow::Pipe => Overflow::Spout,
                    Overflow::Spout => Overflow::Pipe,
                }
            }),
            Action::Control => self.edit_downpipe(id, "Kontrollablauf", |s| s.control = !s.control),
        }
    }

    /// Schellenabstand des Regenfallrohrs `id` (mm).
    pub fn set_downpipe_spacing(&mut self, id: ElementId, mm: f64) -> bool {
        self.edit_downpipe(id, "Schellenabstand", |s| s.spacing = mm)
    }

    fn edit_downpipe(
        &mut self,
        id: ElementId,
        label: &'static str,
        f: impl FnOnce(&mut DownpipeSpec),
    ) -> bool {
        let Some(old) = self.model.downpipe_spec(id) else {
            return false;
        };
        let mut spec = old;
        f(&mut spec);
        if spec == old {
            return false;
        }
        self.edit_model(label, |m| m.set_downpipe_spec(id, spec))
    }
}
