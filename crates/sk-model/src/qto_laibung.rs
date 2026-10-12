//! Laibungen der Öffnungen als Automatikmengen (`opening.*`, vob/oeffnung-
//! vorgabe.md §3 und §4): Nach VOB/C werden die Laibungen von WDVS (DIN
//! 18345), Putz (18350) und Verblendmauerwerk (18330) für jede Öffnung
//! gesondert abgerechnet, auch wenn die Öffnung übermessen wird; bis 1 m
//! Tiefe in m. Unten zählt keine Laibung: dort sitzt die Fensterbank
//! (eigene Position) oder die Schwelle. Dazu die Diagonalbewehrung des WDVS
//! an den Öffnungsecken.
//!
//! Träger jeder Menge ist die Öffnung selbst; so gelten Gebäude, Geschoss
//! und Umfang wie bei jeder Mengenzeile.

use crate::qto::{is_facing, AutoMenge, ElementQto, Schedule};
use crate::{Category, ElementKind, LayerFunction, MatCategory, MaterialLayer};
use crate::{ElementId, Model};

pub const WDVS: &str = "opening.reveal.wdvs";
pub const RENDER: &str = "opening.reveal.render";
pub const INNER: &str = "opening.reveal.inner";
pub const FACING: &str = "opening.reveal.facing";
pub const MESH: &str = "opening.mesh";

/// Schlüssel der Öffnungen (`[service] auto=`), Einheit und Bedeutung.
pub const SCHLUESSEL: [(&str, &str, &str); 5] = [
    (
        WDVS,
        "m",
        "Laibung WDVS links, rechts und oben, für jede Öffnung (DIN 18345)",
    ),
    (
        RENDER,
        "m",
        "Laibung Außenputz auf einschaliger Wand links, rechts und oben (DIN 18350)",
    ),
    (
        INNER,
        "m",
        "Innenlaibung der Außenwand links, rechts und oben, Tiefe bis zum Element (Putz DIN 18350, Maler 18363)",
    ),
    (
        FACING,
        "m",
        "Laibung der Verblendschale links, rechts und oben (DIN 18330)",
    ),
    (
        MESH,
        "st",
        "Diagonalbewehrung WDVS an den Öffnungsecken: Fenster 4, Tür 2",
    ),
];

/// Kostengruppen nach DIN 276: Außenwandbekleidungen außen und innen.
const KG_AUSSEN: u16 = 335;
const KG_INNEN: u16 = 336;

/// Welche Laibung eine Wandschicht zeigt.
fn art(model: &Model, layers: &[MaterialLayer], i: usize) -> Option<&'static str> {
    let core = layers.iter().position(|l| l.core)?;
    let l = layers.get(i)?;
    let cat = model.material(l.material).map(|m| m.category)?;
    // Innenlaibung an der innersten Schicht, mit oder ohne Innenputz im
    // Wandtyp: Putz und Maler je Raum folgen aus dem Raumbuch
    if i >= core {
        return (i + 1 == layers.len()).then_some(INNER);
    }
    let aussen = &layers[..core];
    let schale = aussen.iter().any(|x| is_facing(model, x));
    if is_facing(model, l) {
        return Some(FACING);
    }
    if cat == MatCategory::Insulation && l.function != LayerFunction::AirGap && !schale {
        return Some(WDVS);
    }
    // Außenputz nur auf der einschaligen Wand; auf dem WDVS gehört er zum
    // System und seine Laibung zur WDVS-Laibung
    let gedaemmt = aussen.iter().any(|x| {
        model
            .material(x.material)
            .is_some_and(|m| m.category == MatCategory::Insulation)
    });
    (cat == MatCategory::Plaster && !schale && !gedaemmt).then_some(RENDER)
}

/// Laibungen einer Öffnung je Schlüssel: Längen (mm) als Teilstücke für
/// den Rechenweg, dazu die Tiefe.
#[derive(Default)]
struct Laibung {
    teile: Vec<f64>,
    tiefe: f64,
}

pub(crate) fn bauteil_mengen(model: &Model, sched: &Schedule) -> Vec<AutoMenge> {
    // (Öffnung, Schlüssel) → Laibung, in der Reihenfolge der Mengenliste;
    // Fenster mit Brüstung je Öffnung
    let mut sum: Vec<((ElementId, &'static str), Laibung)> = Vec::new();
    let mut bruestung: Vec<ElementId> = Vec::new();
    for r in sched
        .buildings
        .iter()
        .flat_map(|b| &b.storeys)
        .chain(&sched.loose)
        .flat_map(|s| &s.groups)
        .flat_map(|g| &g.rows)
    {
        let (Some(ElementQto::Wall(w)), Some(e)) = (&r.q, model.element(r.element)) else {
            continue;
        };
        if !matches!(e.kind, ElementKind::Wall(_)) || e.category != Category::ExteriorWall {
            continue;
        }
        let layers = model.element_layers(r.element);
        for (i, l) in w.layers.iter().enumerate() {
            let Some(key) = art(model, &layers, i) else {
                continue;
            };
            for h in &l.holes {
                if h.sill && !bruestung.contains(&h.opening) {
                    bruestung.push(h.opening);
                }
                let k = (h.opening, key);
                // `j` ist die Stelle in `sum`, `i` bleibt die Schicht
                // (Kosten H-L1: sonst falsche Schicht und Absturz)
                let j = match sum.iter().position(|x| x.0 == k) {
                    Some(j) => j,
                    None => {
                        sum.push((k, Laibung::default()));
                        sum.len() - 1
                    }
                };
                let x = &mut sum[j].1;
                x.teile
                    .extend(h.reveal[..3].iter().copied().filter(|v| *v > 0.0));
                // innen bis zum Element, mit dem Putz der Schicht
                let putz = model
                    .material(layers[i].material)
                    .is_some_and(|m| m.category == MatCategory::Plaster);
                x.tiefe = match key {
                    INNER if putz => h.install.inner + h.depth,
                    INNER => h.install.inner,
                    WDVS | RENDER => h.install.outer,
                    _ => h.depth,
                };
            }
        }
    }
    let mut out = Vec::new();
    for ((id, key), x) in &sum {
        let Some(e) = model.element(*id) else {
            continue;
        };
        let vorlage = AutoMenge {
            element: *id,
            number: e.number.clone(),
            storey: e.storey,
            building: model.storey(e.storey).and_then(|s| s.building),
            key: "",
            unit: "",
            value: 0.0,
            kg: None,
            formula: String::new(),
        };
        let wert: f64 = x.teile.iter().sum();
        if wert > 0.0 {
            out.push(AutoMenge {
                key,
                unit: "m",
                value: wert,
                kg: Some(if *key == INNER { KG_INNEN } else { KG_AUSSEN }),
                formula: formel(&x.teile, x.tiefe),
                ..vorlage.clone()
            });
        }
        if *key == WDVS {
            let n = if bruestung.contains(id) { 4.0 } else { 2.0 };
            out.push(AutoMenge {
                key: MESH,
                unit: "st",
                value: n,
                kg: Some(KG_AUSSEN),
                formula: format!("{n} Ecken ({})", if n > 2.0 { "Fenster" } else { "Tür" }),
                ..vorlage
            });
        }
    }
    out
}

/// „1,510 + 1,510 + 1,250 m, Tiefe 0,180 m“.
fn formel(teile: &[f64], tiefe: f64) -> String {
    let m = |mm: f64| format!("{:.3}", mm / 1000.0).replace('.', ",");
    let summe: Vec<String> = teile.iter().map(|v| m(*v)).collect();
    format!("{} m, Tiefe {} m", summe.join(" + "), m(tiefe))
}
