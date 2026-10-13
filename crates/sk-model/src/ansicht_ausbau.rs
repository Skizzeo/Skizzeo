//! Schalter „Ausbau innen“ je Ansicht (Raumbuch RB4, `raumbuch/plan.md`
//! §8): Grundriss, Schnitt und 3D zeigen Innenputz, Wandbeläge, Deckenputz,
//! Bodenbelag und Sockelleisten nur, wenn er an ist (Vorgabe aus, wie
//! Allplan); den Estrich zeigt der Schnitt immer, 3D nur mit dem Schalter.
//! Ansichtszustand wie die Schattenwahl der Ansichten: ohne
//! Rückgängig-Schritt und ohne neue Revision. In der Datei steht
//! `[viewfinish] view=plan on=1` nur, wo er an ist oder die Datei die
//! Ansicht schon nannte (dann auch `on=0`, Review #137 H4). Unlesbare
//! Zeilen bleiben roh stehen (Regel 72).

use super::Model;
use crate::Change;

/// Ansichten mit dem Schalter, in dieser Reihenfolge.
pub const FINISH_VIEWS: [&str; 3] = ["plan", "section", "3d"];

impl Model {
    /// Zeigt die Ansicht `i` (Reihenfolge [`FINISH_VIEWS`]) den Ausbau
    /// innen?
    pub fn view_finish(&self, i: usize) -> bool {
        self.view_finish.get(i).is_some_and(|v| v.0)
    }

    /// Steht die Ansicht `i` in der Datei (an oder aus)?
    pub(crate) fn view_finish_line(&self, i: usize) -> bool {
        self.view_finish.get(i).is_some_and(|v| v.0 || v.1)
    }

    /// Wie [`Model::set_view_below`]: Ansichtszustand ohne Schritt; eine
    /// unlesbare Zeile dieser Ansicht entfällt.
    pub fn set_view_finish(&mut self, i: usize, an: bool) {
        if let Some(v) = self.view_finish.get_mut(i) {
            v.0 = an;
            let name = format!("view={}", FINISH_VIEWS[i]);
            self.view_finish_raw
                .retain(|l| !l.split_whitespace().any(|w| w == name));
        }
    }

    /// `[viewfinish]`-Zeilen der Datei, die nicht zählen, roh.
    pub(crate) fn view_finish_raw(&self) -> &[String] {
        &self.view_finish_raw
    }

    /// Die Änderungen des offenen Schritts bisher, leer ohne Schritt; den
    /// neuen Stand hat das Modell. Die Szene merkt sich daran, was ein
    /// Schritt am Ausbau innen berührt.
    pub fn schritt_aenderungen(&self) -> &[Change] {
        self.txn.as_ref().map_or(&[], |t| &t.changes)
    }

    /// Je Ansicht: an, und ob die Datei sie nannte.
    pub(crate) fn load_view_finish(&mut self, an: [(bool, bool); 3], raw: Vec<String>) {
        self.view_finish = an;
        self.view_finish_raw = raw;
    }
}
