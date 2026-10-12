//! Höhen im Geschoss, gemessen über OK Rohdecke bzw. OK Sohlplatte (der
//! Unterkante des Geschossbands): die Schnitthöhe des Grundrisses (Jörn
//! 11.10. 14:05) und die Höhenlinien Brüstung und Sturz je Geschoss (14:01).
//! Die Schnitthöhe gilt in jedem Geschoss gleich; das Fundament schneidet
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

/// Vorgabe der Brüstungslinie (mm über OK Rohdecke), Jörn 14:01: 87,5 cm.
pub const SILL: f64 = 875.0;
/// Vorgabe der Sturzlinie (mm über OK Rohdecke), Jörn 14:01: 2,385 m.
pub const LINTEL: f64 = 2385.0;
/// Kleinster Abstand zwischen Brüstung und Sturz (mm).
pub const MIN_LINE_GAP: f64 = 100.0;
/// Wandöffnungshöhen der Innentüren ab OK Fertigfußboden nach DIN 18101
/// (Baurichtmaß + 5 mm), passend zu Zargen und Blättern aus dem Handel
/// (Jörn 12.10. 05:58).
pub const DOOR_HEIGHTS: [f64; 3] = [2005.0, 2130.0, 2255.0];
/// Wandöffnungsbreiten der Innentüren nach DIN 18101 (Baurichtmaß
/// + 10 mm), Wahl beim Setzen.
pub const DOOR_WIDTHS: [f64; 4] = [635.0, 760.0, 885.0, 1010.0];
/// Fußbodenaufbau über OK Rohdecke (mm), bis das Raumbuch ihn je Geschoss
/// liefert: Werksvorgabe Parkett 20 auf Estrichaufbau 140
/// ([`crate::model::FINISH_TYPES`]); OK Fertigfußboden liegt im Geschoss
/// überall gleich hoch.
pub const FLOOR_BUILDUP: f64 = 160.0;
/// Fangweite der Innentürlinie an den Normhöhen (mm).
pub const DOOR_CATCH: f64 = 30.0;
/// Vorgabe der Innentürlinie (mm über OK Rohdecke): die kleinste Normhöhe
/// über dem Fußbodenaufbau (Jörn 12.10. 05:26, 05:58).
pub const DOOR: f64 = DOOR_HEIGHTS[0] + FLOOR_BUILDUP;

/// Innentürlinie `h` (mm über OK Rohdecke) auf eine Normhöhe über dem
/// Fußbodenaufbau gefangen, wenn sie näher als [`DOOR_CATCH`] liegt.
pub fn door_snap(h: f64) -> f64 {
    DOOR_HEIGHTS
        .iter()
        .map(|n| n + FLOOR_BUILDUP)
        .find(|z| (h - z).abs() < DOOR_CATCH)
        .unwrap_or(h)
}

/// Höhenlinien eines Geschosses über OK Rohdecke (mm): Brüstung und Sturz
/// der Fenster, Oberkante der Innentüren. Öffnungen binden ihre Unter- und
/// Oberkante an sie; in der Datei am `[storey]` nur, wenn sie von der
/// Vorgabe abweichen.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HeightLines {
    pub sill: f64,
    pub lintel: f64,
    pub door: f64,
}

impl Default for HeightLines {
    fn default() -> Self {
        HeightLines {
            sill: SILL,
            lintel: LINTEL,
            door: DOOR,
        }
    }
}

impl HeightLines {
    /// Gültig: Brüstung ab 0, Sturz mindestens [`MIN_LINE_GAP`] darüber,
    /// Innentür mindestens [`crate::oeffnung::MIN_OPENING`] hoch.
    pub fn valid(&self) -> bool {
        self.sill.is_finite()
            && self.lintel.is_finite()
            && self.door.is_finite()
            && self.sill >= 0.0
            && self.lintel >= self.sill + MIN_LINE_GAP
            && self.door >= crate::oeffnung::MIN_OPENING
    }
}

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

    /// Innentürlinie (Jörn 05:58): Vorgabe 2,005 m ab OK Fertigfußboden
    /// über dem Fußbodenaufbau der Werks-Ausbautypen; nahe einer Normhöhe
    /// rastet sie dort ein, sonst bleibt sie, wo sie ist.
    #[test]
    fn innentuer_auf_normhoehe() {
        let dicke = |g: crate::Guid| {
            crate::model::FINISH_TYPES
                .iter()
                .find(|t| t.guid == g)
                .map_or(0.0, |t| t.layers.iter().map(|l| l.1).sum::<f64>())
        };
        let aufbau =
            dicke(crate::model::PARQUET_TYPE_GUID) + dicke(crate::model::SCREED_FLOOR_TYPE_GUID);
        assert_eq!(FLOOR_BUILDUP, aufbau);
        assert_eq!(DOOR, 2165.0);
        assert_eq!(door_snap(2150.0), 2165.0);
        assert_eq!(door_snap(2300.0), 2290.0);
        assert_eq!(door_snap(2400.0), 2415.0);
        assert_eq!(door_snap(2220.0), 2220.0);
    }
}
