//! Höhen im Geschoss, gemessen über OK Rohdecke bzw. OK Sohlplatte (der
//! Unterkante des Geschossbands): die Schnitthöhe des Grundrisses (Jörn
//! 11.10. 14:05). Sie gilt in jedem Geschoss gleich; das Fundament schneidet
//! weiter in der Mitte der Frostschürze.
//!
//! Die Schnitthöhe ist Ansichtszustand wie die Schatten der Ansichten: ohne
//! Rückgängig-Schritt, in der Datei `[plancut] h=1500` nur, wenn sie von der
//! Vorgabe abweicht. Eine Zeile, die nicht zählt, bleibt roh stehen.

use crate::Model;

/// Vorgabe der Schnitthöhe (mm über OK Rohdecke), Jörns Vorgabe 1,50 m.
pub const PLAN_CUT: f64 = 1500.0;
/// Kleinste und größte Schnitthöhe (mm).
pub const MIN_PLAN_CUT: f64 = 100.0;
pub const MAX_PLAN_CUT: f64 = 5000.0;
/// So weit bleibt der Schnitt mindestens unter UK Rohdecke darüber (mm),
/// sonst schnitte er im niedrigen Geschoss die Decke.
pub const PLAN_CUT_TOP_GAP: f64 = 50.0;

impl Model {
    /// Schnitthöhe des Grundrisses über OK Rohdecke (mm): die eigene des
    /// Projekts oder [`PLAN_CUT`].
    pub fn plan_cut_height(&self) -> f64 {
        self.plan_cut.unwrap_or(PLAN_CUT)
    }

    /// Die eigene Schnitthöhe des Projekts, falls es eine gibt.
    pub fn plan_cut_own(&self) -> Option<f64> {
        self.plan_cut
    }

    /// Schnitthöhe wählen (mm, auf [`MIN_PLAN_CUT`]…[`MAX_PLAN_CUT`]
    /// begrenzt und auf 1 mm gerundet): Ansichtszustand ohne Schritt. Gleicht
    /// sie der Vorgabe, entfällt die eigene (und ihre Zeile); eine unlesbare
    /// Zeile entfällt damit.
    pub fn set_plan_cut_height(&mut self, h: f64) {
        if !h.is_finite() {
            return;
        }
        let h = h.clamp(MIN_PLAN_CUT, MAX_PLAN_CUT).round();
        self.plan_cut = (h != PLAN_CUT).then_some(h);
        self.plan_cut_raw = None;
    }

    /// `[plancut]`-Zeile der Datei, die nicht zählt, roh.
    pub(crate) fn plan_cut_raw(&self) -> Option<&str> {
        self.plan_cut_raw.as_deref()
    }

    pub(crate) fn load_plan_cut(&mut self, h: Option<f64>, raw: Option<String>) {
        self.plan_cut = h;
        self.plan_cut_raw = raw;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vorgabe_und_eigene() {
        let mut m = Model::with_seed(1);
        assert_eq!(m.plan_cut_height(), 1500.0);
        assert_eq!(m.plan_cut_own(), None);
        m.set_plan_cut_height(1000.4);
        assert_eq!(
            (m.plan_cut_height(), m.plan_cut_own()),
            (1000.0, Some(1000.0))
        );
        m.set_plan_cut_height(9000.0);
        assert_eq!(m.plan_cut_height(), MAX_PLAN_CUT);
        m.set_plan_cut_height(f64::NAN);
        assert_eq!(m.plan_cut_height(), MAX_PLAN_CUT);
        m.set_plan_cut_height(PLAN_CUT);
        assert_eq!(m.plan_cut_own(), None);
    }
}
