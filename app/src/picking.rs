//! Gemeinsamer Hover- und Auswahlzustand (F2): Es gibt ihn einmal, Hauptfenster
//! und Mengenfenster lesen und schreiben denselben. Sitzungszustand: nicht im
//! Modell, nicht in der `.szo`, nicht im Rückgängig.

use crate::scene::Scene;
use sk_model::ElementId;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Picking {
    /// Gewählte Bauteile in der Reihenfolge des Wählens (Mehrfachauswahl).
    pub selected: Vec<ElementId>,
    /// Bauteil unter der Maus, egal in welchem Fenster.
    pub hover: Option<ElementId>,
}

impl Picking {
    /// Klick auf ein Bauteil: ersetzt die Auswahl; mit Strg fügt er hinzu
    /// oder nimmt weg.
    pub fn click(&mut self, id: ElementId, ctrl: bool) {
        if !ctrl {
            self.selected = vec![id];
        } else if let Some(i) = self.selected.iter().position(|&e| e == id) {
            self.selected.remove(i);
        } else {
            self.selected.push(id);
        }
    }

    /// Genau dieses Bauteil wählen (oder nichts). `true`, wenn sich etwas ändert.
    pub fn select_only(&mut self, id: Option<ElementId>) -> bool {
        let new: Vec<ElementId> = id.into_iter().collect();
        let changed = new != self.selected;
        self.selected = new;
        changed
    }

    /// Zuletzt gewähltes Bauteil (zeigt das Paneel „Eigenschaften“).
    pub fn primary(&self) -> Option<ElementId> {
        self.selected.last().copied()
    }

    pub fn is_selected(&self, id: ElementId) -> bool {
        self.selected.contains(&id)
    }

    /// Esc: Auswahl aufheben.
    #[cfg(test)]
    pub fn clear(&mut self) {
        self.selected.clear();
    }

    /// Bauteile, die es nicht mehr gibt (gelöscht, Rückgängig), fallen aus
    /// Auswahl und Hover. `true`, wenn sich etwas geändert hat.
    pub fn validate(&mut self, scene: &Scene) -> bool {
        let m = scene.model();
        let n = self.selected.len();
        self.selected.retain(|&e| m.element(e).is_some());
        let mut changed = n != self.selected.len();
        if self.hover.is_some_and(|e| m.element(e).is_none()) {
            self.hover = None;
            changed = true;
        }
        changed
    }
}
