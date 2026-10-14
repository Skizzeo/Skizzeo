//! Mengen des Raumbuchs in der Mengenliste (RB5, raumbuch/plan.md §6.2,
//! mit Kosten abgestimmt). sk-model kennt sk-raum nicht (architektur.md
//! §2): Die Zeilen je Raum rechnet sk-raum, die App gibt sie über
//! [`crate::qto::schedule_mit`] herein. Ohne Räume bleibt die Liste, wie
//! sie ist ([`crate::qto::schedule`]).
//!
//! Was eine Zeile wird:
//! - eine Schicht eines Ausbautyps (Estrich, Dämmung, Belag, Putz, Anstrich,
//!   Tapete, Fliesen, Abdichtung) eine [`crate::qto::LayerRow`] mit dem Raum
//!   als Bauteil, Gewerk aus Schicht bzw. Baustoff und der Kostengruppe nach
//!   Träger;
//! - eine Länge mit Schlüssel (Sockelleiste, Eckschienen, Randdämmstreifen,
//!   Silikon) eine [`crate::qto::AutoMenge`] am Raum;
//! - Laibungen und Dichtband stehen nur in der Liste: Die Innenlaibung der
//!   Außenwand rechnet `opening.reveal.inner` an der Öffnung (#88), das
//!   Dichtband gehört zur Verbundabdichtung.

use crate::element::{Category, ElementId, StoreyId};
use crate::library::{LayerSetId, MatCategory, MaterialId};
use crate::model::Model;
use crate::qto::{AutoMenge, ElementQto, LayerRow, RowQto, Schedule};
use crate::rechenweg::{Bezug, Rechenweg};
use crate::trade::TradeId;

/// Seite einer Mengenzeile für die Regeln der Kosten (`side=`): Boden, Wand
/// und Decke eines Raums, Wandschichten vor bzw. hinter dem Kern.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Side {
    Floor,
    Wall,
    Ceiling,
    Inner,
    Outer,
}

impl Side {
    pub const ALL: [Side; 5] = [
        Side::Floor,
        Side::Wall,
        Side::Ceiling,
        Side::Inner,
        Side::Outer,
    ];

    /// Wert im Regelfeld `side=`.
    pub fn key(self) -> &'static str {
        match self {
            Side::Floor => "floor",
            Side::Wall => "wall",
            Side::Ceiling => "ceiling",
            Side::Inner => "inner",
            Side::Outer => "outer",
        }
    }

    pub fn from_key(k: &str) -> Option<Side> {
        Side::ALL.into_iter().find(|s| s.key() == k)
    }
}

/// Eine Zeile des Raumbuchs: eine Schicht an einer Fläche, eine Länge mit
/// Schlüssel oder ein Hinweis.
#[derive(Clone, Debug, PartialEq)]
pub struct RoomLine {
    /// „Boden“, „Wand AW-004 Nord“, „Laibungen AW-004“, „Sockelleiste“ …
    pub name: String,
    pub side: Side,
    /// Ausbautyp und Stelle der Schicht darin.
    pub finish: Option<(LayerSetId, usize)>,
    pub material: Option<MaterialId>,
    /// Wirksame Dicke (mm), im Estrich mit Ausgleich; 0 bei Anstrich,
    /// Tapete und Längen.
    pub thickness: f64,
    pub trade: Option<TradeId>,
    /// Kostengruppe nach Träger (DIN 276:2018).
    pub kg: Option<u16>,
    /// Automatik-Schlüssel (`room.*`) einer Länge.
    pub key: Option<&'static str>,
    /// Nur in der Liste: zählt nicht nach Gewerk, Kostengruppe und Kosten.
    pub info: bool,
    /// Menge (mm² bzw. mm) mit ihren Termen; Summe der Terme = Menge.
    pub weg: Rechenweg,
}

impl RoomLine {
    pub fn quantity(&self) -> f64 {
        self.weg.menge()
    }

    /// Schicht mit Fläche oder Länge, die als Mengenzeile zählt.
    pub fn is_layer(&self) -> bool {
        !self.info && self.key.is_none() && self.material.is_some()
    }

    /// Länge mit Schlüssel: eine Automatikmenge am Raum.
    pub fn is_auto(&self) -> bool {
        !self.info && self.key.is_some()
    }

    pub fn is_length(&self) -> bool {
        self.weg.bezug == Bezug::Laenge
    }
}

/// Mengen eines Raums in der Liste.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RoomQto {
    /// Bodenfläche im Fertigmaß (mm²) für die Summe der Gruppe.
    pub area: f64,
    /// Lichte Höhe OK Fertigfußboden bis UK Fertigdecke (mm).
    pub height: f64,
    pub lines: Vec<RoomLine>,
}

/// Zeilen der Räume eines Geschosses: Raum und seine Mengen. Rechnet
/// sk-raum (`sk_raum::mengen::raeume`).
pub type RoomFn<'a> = &'a dyn Fn(&Model, StoreyId) -> Vec<(ElementId, RoomQto)>;

/// „4,240 − 0,885 = 3,355 m“: Rechenweg einer Länge für den Mengenansatz.
pub fn formel(weg: &Rechenweg) -> String {
    let m = |mm: f64| format!("{:.3}", mm.abs() / 1e3).replace('.', ",");
    let mut out = String::new();
    for t in weg.terme.iter().filter(|t| t.wert != 0.0) {
        match (out.is_empty(), t.wert < 0.0) {
            (true, false) => {}
            (true, true) => out.push('−'),
            (false, false) => out.push_str(" + "),
            (false, true) => out.push_str(" − "),
        }
        out.push_str(&m(t.wert));
    }
    let n = weg.terme.iter().filter(|t| t.wert != 0.0).count();
    if n > 1 {
        out.push_str(&format!(" = {}", m(weg.menge())));
    }
    out.push_str(" m");
    out
}

/// Automatikmengen der Räume: je Länge mit Schlüssel eine Menge am Raum,
/// in der Reihenfolge der Liste.
pub(crate) fn auto_mengen(model: &Model, sched: &Schedule) -> Vec<AutoMenge> {
    let mut out = Vec::new();
    let storeys = sched
        .buildings
        .iter()
        .flat_map(|b| b.storeys.iter().map(move |s| (Some(b.id), s)))
        .chain(sched.loose.iter().map(|s| (None, s)));
    for (building, s) in storeys {
        for r in s.groups.iter().flat_map(|g| &g.rows) {
            let Some(ElementQto::Room(q)) = &r.q else {
                continue;
            };
            for l in q.lines.iter().filter(|l| l.is_auto()) {
                let (Some(key), v) = (l.key, l.quantity()) else {
                    continue;
                };
                if v <= 0.0 || model.element(r.element).is_none() {
                    continue;
                }
                out.push(AutoMenge {
                    element: r.element,
                    number: r.number.clone(),
                    storey: s.id,
                    building,
                    key,
                    unit: "m",
                    value: v,
                    kg: l.kg,
                    formula: formel(&l.weg),
                });
            }
        }
    }
    out
}

/// Mengenzeilen der Schichten eines Raums: Fläche und Abrechnungsfläche
/// sind die Menge nach VOB/C, das Volumen Fläche × Dicke (ohne Dicke 0).
/// `layer` ist die Stelle der Zeile im Raum (eindeutig je Raum), die
/// Schicht im Ausbautyp steht in `finish`.
pub fn layer_rows(model: &Model, row: &RowQto, storey: StoreyId, q: &RoomQto) -> Vec<LayerRow> {
    let mut out = Vec::new();
    for (i, l) in q.lines.iter().enumerate().filter(|(_, l)| l.is_layer()) {
        let Some(material) = l.material else { continue };
        let v = l.quantity();
        if v <= 0.0 {
            continue;
        }
        let (length, area) = if l.is_length() { (v, 0.0) } else { (0.0, v) };
        out.push(LayerRow {
            element: row.element,
            number: row.number.clone(),
            category: Category::Space,
            storey,
            layer: i,
            material,
            thickness: l.thickness,
            trade: l.trade,
            kg: l.kg,
            length,
            area,
            volume: area * l.thickness,
            insulation: model
                .material(material)
                .is_some_and(|m| m.category == MatCategory::Insulation),
            whole: false,
            attika: false,
            bill_area: area,
            bill_length: length,
            once: false,
            face: area,
            pocket: 0.0,
            parts: None,
            holes: Vec::new(),
            side: Some(l.side),
            finish: l.finish,
            terms: l.weg.terme.clone(),
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seiten_als_wort() {
        for s in Side::ALL {
            assert_eq!(Side::from_key(s.key()), Some(s));
        }
        assert_eq!(Side::from_key("roof"), None);
    }
}
