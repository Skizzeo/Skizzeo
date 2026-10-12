//! Schalter „Ausbau innen“ je Ansicht (Raumbuch RB4, `raumbuch/plan.md`
//! §8): Grundriss, Schnitt und 3D zeigen Innenputz, Wandbeläge, Deckenputz,
//! Bodenbelag und Sockelleisten nur, wenn er an ist (Vorgabe aus, wie
//! Allplan); den Estrich zeigen Schnitt und 3D immer. Ansichtszustand wie
//! die Schattenwahl der Ansichten: ohne Rückgängig-Schritt und ohne neue
//! Revision, in der Datei `[viewfinish] view=plan on=1` nur, wo er an ist.
//! Unlesbare Zeilen bleiben roh stehen (Regel 72).

use super::Model;

/// Ansichten mit dem Schalter, in dieser Reihenfolge.
pub const FINISH_VIEWS: [&str; 3] = ["plan", "section", "3d"];

impl Model {
    /// Zeigt die Ansicht `i` (Reihenfolge [`FINISH_VIEWS`]) den Ausbau
    /// innen?
    pub fn view_finish(&self, i: usize) -> bool {
        self.view_finish.get(i).copied().unwrap_or(false)
    }

    /// Wie [`Model::set_view_below`]: Ansichtszustand ohne Schritt; eine
    /// unlesbare Zeile dieser Ansicht entfällt.
    pub fn set_view_finish(&mut self, i: usize, an: bool) {
        if let Some(v) = self.view_finish.get_mut(i) {
            *v = an;
            let name = format!("view={}", FINISH_VIEWS[i]);
            self.view_finish_raw
                .retain(|l| !l.split_whitespace().any(|w| w == name));
        }
    }

    /// `[viewfinish]`-Zeilen der Datei, die nicht zählen, roh.
    pub(crate) fn view_finish_raw(&self) -> &[String] {
        &self.view_finish_raw
    }

    pub(crate) fn load_view_finish(&mut self, an: [bool; 3], raw: Vec<String>) {
        self.view_finish = an;
        self.view_finish_raw = raw;
    }
}
