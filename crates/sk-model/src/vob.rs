//! Abrechnung der Öffnungen nach VOB/C (vob/oeffnung-vorgabe.md §1 und §2):
//! Die Abrechnungsfläche einer Wandschicht verliert ein Loch nur, wenn die
//! Aussparung in dieser Schicht größer als 2,5 m² ist (DIN 18330, 18345,
//! 18350, 18363 je Abschnitt 5); kleinere werden übermessen. Teilstücke
//! einer Ecköffnung oder einer geschossübergreifenden Öffnung sind eine
//! Aussparung: Die Schwelle gilt für ihre Summe im selben Baustoff.
//!
//! Das Volumen der Mengenzeile verliert ein Loch nur über 0,5 m³ (DIN
//! 18330, 18331 Abschnitt 5, Kosten H79-1), ebenfalls je Aussparung und
//! Baustoff. Das geometrische Volumen (`LayerQto.volume`) zieht jedes Loch
//! ab; das bleibt so.

use crate::qto::{ElementQto, LayerQto, StoreyQto};
use crate::{ElementId, MaterialId};
use std::collections::HashMap;

/// Bis zu dieser Fläche (mm²) wird eine Öffnung übermessen.
pub const UEBERMESSEN_BIS: f64 = 2.5e6;
/// Bis zu diesem Volumen (mm³) wird eine Öffnung übermessen.
pub const UEBERMESSEN_BIS_VOL: f64 = 0.5e9;

/// Loch einer Öffnung in einer Schicht einer Mengenzeile. Maße in mm.
#[derive(Clone, Debug, PartialEq)]
pub struct Loch {
    pub opening: ElementId,
    pub width: f64,
    pub height: f64,
    /// Fläche dieses Teilstücks in der Schicht (mm²).
    pub area: f64,
    /// Volumen dieses Teilstücks (mm³), geometrisch abgezogen.
    pub volume: f64,
    /// Fläche der ganzen Aussparung im selben Baustoff, alle Teilstücke.
    pub total: f64,
    /// Volumen der ganzen Aussparung im selben Baustoff (mm³).
    pub total_vol: f64,
}

impl Loch {
    /// Abzug nach VOB/C: Aussparung über 2,5 m², verglichen in m² auf drei
    /// Stellen wie im LV (2,500 m² wird übermessen).
    pub fn vob(&self) -> bool {
        ueber_grenze(self.total)
    }

    /// Abzug vom Volumen nach VOB/C: Aussparung über 0,5 m³, in m³ auf
    /// drei Stellen.
    pub fn vob_volumen(&self) -> bool {
        ueber_grenze_vol(self.total_vol)
    }

    /// Abzug in der Bestellmenge: ab der Größe `ab` (mm²) des Baustoffs;
    /// ohne Angabe werden alle Öffnungen übermessen.
    pub fn bestellung(&self, ab: Option<f64>) -> bool {
        ab.is_some_and(|a| self.total >= a)
    }
}

/// Aussparung mit Fläche `total` (mm²) über 2,5 m², in m² auf drei Stellen.
pub fn ueber_grenze(total: f64) -> bool {
    (total / 1e3).round() > UEBERMESSEN_BIS / 1e3
}

/// Aussparung mit Volumen `v` (mm³) über 0,5 m³, in m³ auf drei Stellen.
pub fn ueber_grenze_vol(v: f64) -> bool {
    (v / 1e6).round() > UEBERMESSEN_BIS_VOL / 1e6
}

/// Lochfläche und -volumen je Öffnung und Baustoff über alle Wände der
/// Geschosse.
pub(crate) type Summen = HashMap<(ElementId, MaterialId), (f64, f64)>;

pub(crate) fn summen(storeys: &[StoreyQto]) -> Summen {
    let mut s = Summen::new();
    for r in storeys
        .iter()
        .flat_map(|s| s.groups.iter().flat_map(|g| g.rows.iter()))
    {
        let Some(ElementQto::Wall(w)) = &r.q else {
            continue;
        };
        for l in &w.layers {
            for h in &l.holes {
                let x = s.entry((h.opening, l.material)).or_default();
                x.0 += h.area;
                x.1 += h.volume;
            }
        }
    }
    s
}

/// Die Löcher einer Wandschicht mit der Fläche ihrer ganzen Aussparung.
pub(crate) fn loecher(l: &LayerQto, s: &Summen) -> Vec<Loch> {
    l.holes
        .iter()
        .map(|h| {
            let (total, total_vol) = s
                .get(&(h.opening, l.material))
                .copied()
                .unwrap_or((h.area, h.volume));
            Loch {
                opening: h.opening,
                width: h.width,
                height: h.height,
                area: h.area,
                volume: h.volume,
                total,
                total_vol,
            }
        })
        .collect()
}

/// Fläche, die nach VOB/C von der Abrechnungsfläche abgeht.
pub fn abzug(loecher: &[Loch]) -> f64 {
    loecher.iter().filter(|h| h.vob()).map(|h| h.area).sum()
}

/// Volumen der übermessenen Löcher (bis 0,5 m³): Es gehört nach VOB/C zum
/// Volumen der Mengenzeile, geometrisch ist es abgezogen.
pub fn uebermessen_vol(loecher: &[Loch]) -> f64 {
    loecher
        .iter()
        .filter(|h| !h.vob_volumen())
        .map(|h| h.volume)
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grenze_zweieinhalb() {
        assert!(!ueber_grenze(2.5e6));
        assert!(!ueber_grenze(2.5004e6), "rundet auf 2,500 m²");
        assert!(ueber_grenze(2.5006e6));
        assert!(!ueber_grenze(1.57e6));
        assert!(ueber_grenze(4.54e6));
    }

    #[test]
    fn grenze_halber_kubikmeter() {
        assert!(!ueber_grenze_vol(0.5e9));
        assert!(!ueber_grenze_vol(0.5004e9), "rundet auf 0,500 m³");
        assert!(ueber_grenze_vol(0.5006e9));
        assert!(!ueber_grenze_vol(0.33e9));
    }
}
