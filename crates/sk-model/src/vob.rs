//! Abrechnung der Öffnungen nach VOB/C (vob/oeffnung-vorgabe.md §1 und §2):
//! Die Abrechnungsfläche einer Wandschicht verliert ein Loch nur, wenn die
//! Aussparung in dieser Schicht größer als 2,5 m² ist (DIN 18330, 18345,
//! 18350, 18363 je Abschnitt 5); kleinere werden übermessen. Teilstücke
//! einer Ecköffnung oder einer geschossübergreifenden Öffnung sind eine
//! Aussparung: Die Schwelle gilt für ihre Summe in derselben Schicht. Jede
//! Schicht hat ihr eigenes Loch (vob/oeffnung-vorgabe.md §2): Zwei Lagen
//! aus demselben Baustoff, etwa Dämmung 2 × 70 mm, werden je für sich
//! gegen die Grenze geprüft (Review W24 B1). Dieselbe Schicht ist über
//! Wände hinweg das k-te Vorkommen ihres Baustoffs in der Wand, nicht die
//! Stelle im Wandtyp: So summiert eine Öffnung über zwei Geschosse mit
//! verschiedenen Wandtypen ihre Teile im selben Baustoff (Review #88 L14).
//!
//! Das Volumen der Mengenzeile verliert ein Loch nur über 0,5 m³ (DIN
//! 18330, 18331 Abschnitt 5, Kosten H79-1), ebenfalls je Aussparung und
//! Schicht. Das geometrische Volumen (`LayerQto.volume`) zieht jedes Loch
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
    /// Fläche der ganzen Aussparung in derselben Schicht, alle Teilstücke.
    pub total: f64,
    /// Volumen der ganzen Aussparung in derselben Schicht (mm³).
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

/// Lochfläche und -volumen je Öffnung und Schicht (Baustoff und sein
/// k-tes Vorkommen in der Wand) über alle Wände der Geschosse.
pub(crate) type Summen = HashMap<(ElementId, MaterialId, usize), (f64, f64)>;

/// Schlüssel der Schicht `i` einer Wand: Baustoff und wie oft er davor
/// schon vorkam.
fn schicht(layers: &[LayerQto], i: usize) -> Option<(MaterialId, usize)> {
    let m = layers.get(i)?.material;
    Some((m, layers[..i].iter().filter(|l| l.material == m).count()))
}

pub(crate) fn summen(storeys: &[StoreyQto]) -> Summen {
    let mut s = Summen::new();
    for r in storeys
        .iter()
        .flat_map(|s| s.groups.iter().flat_map(|g| g.rows.iter()))
    {
        let Some(ElementQto::Wall(w)) = &r.q else {
            continue;
        };
        for (i, l) in w.layers.iter().enumerate() {
            let Some((m, k)) = schicht(&w.layers, i) else {
                continue;
            };
            for h in &l.holes {
                let x = s.entry((h.opening, m, k)).or_default();
                x.0 += h.area;
                x.1 += h.volume;
            }
        }
    }
    s
}

/// Die Löcher der Wandschicht `i` mit der Fläche ihrer ganzen Aussparung.
pub(crate) fn loecher(layers: &[LayerQto], i: usize, s: &Summen) -> Vec<Loch> {
    let (Some(l), Some((m, k))) = (layers.get(i), schicht(layers, i)) else {
        return Vec::new();
    };
    l.holes
        .iter()
        .map(|h| {
            let (total, total_vol) = s
                .get(&(h.opening, m, k))
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

/// Mehrmenge der Bestellung gegenüber VOB/C vor dem Zuschlag (Fläche mm²,
/// Volumen mm³): Öffnungen ab der Größe `ab` des Baustoffs abgezogen,
/// sonst übermessen. Gilt für die Mengenzeile wie für die Summe nach
/// Baustoff.
pub fn bestell_mehr(loecher: &[Loch], ab: Option<f64>) -> (f64, f64) {
    let summe = |f: &dyn Fn(&Loch) -> bool, v: &dyn Fn(&Loch) -> f64| -> f64 {
        loecher.iter().filter(|h| f(h)).map(v).sum()
    };
    (
        summe(&|h| h.vob(), &|h| h.area) - summe(&|h| h.bestellung(ab), &|h| h.area),
        summe(&|h| !h.bestellung(ab), &|h| h.volume) - summe(&|h| !h.vob_volumen(), &|h| h.volume),
    )
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
