//! Körper je Bauteil an einer Stelle: der 3D-Körper eines Wandzugs mit
//! Gründung, Decke und Flachdach, wie ihn die Ansicht zeichnet, und daraus
//! getrennt je Bauteil (IFC-Export, später Räume). Die App setzt ihre Körper
//! aus [`run_solid`] zusammen, damit Ansicht und Austausch dieselbe Form
//! zeigen.

use crate::element::{ElementId, ElementKind, RunId};
use crate::erweiterung_koerper;
use crate::floor::FloorSlab;
use crate::foundation::Foundation;
use crate::model::{
    Model, COPING_PART, DOWNPIPE_PART, FLOOR_PART, FOOTING_PART, PERIMETER_PART, ROOF_COPING_PART,
    ROOF_PART, SLAB_PART, SOFFIT_PART, STRIP_PART, TERRACE_PART,
};
use crate::roof::FlatRoof;
use crate::solid::Solid;
use crate::wall::WallChain;
use std::collections::HashMap;

/// Körper mit allen Dreiecken und Kanten dem Teil `part` zugeordnet (Treffer
/// beim Klicken, Trennung je Bauteil über [`Model::part_of`]).
pub fn part(mut s: Solid, part: u32) -> Solid {
    for t in &mut s.triangles {
        t.elem = part;
    }
    for e in &mut s.edges {
        e.elem = part;
    }
    s
}

/// Gründung unter einem Zug: Sohlplatte, Perimeterdämmung, Frostschürze.
pub fn foundation_solid(f: &Foundation, out: &mut Solid) {
    out.append(&part(f.slab_solid(), SLAB_PART));
    out.append(&part(f.insulation_solid(), PERIMETER_PART));
    out.append(&part(f.footing_solid(), FOOTING_PART));
}

/// Decke über einem Zug mit Untersicht, Dachterrasse, Attikablech und
/// Randdämmstreifen.
pub fn floor_solid(f: &FloorSlab, out: &mut Solid) {
    out.append(&part(f.solid(), FLOOR_PART));
    out.append(&part(f.soffit_solid(), SOFFIT_PART));
    out.append(&part(f.terrace_solid(), TERRACE_PART));
    out.append(&part(f.coping_solid(), COPING_PART));
    for k in 0..f.strips.len() {
        out.append(&part(f.strip_solid(k), STRIP_PART + k as u32));
    }
}

/// Flachdach auf einer Aufkantung: Aufbau, Blech, Regenfallrohre.
pub fn roof_solid(r: &FlatRoof, out: &mut Solid) {
    out.append(&part(r.solid(), ROOF_PART));
    out.append(&part(r.coping_solid(), ROOF_COPING_PART));
    for (k, d) in r.downpipes.iter().enumerate() {
        out.append(&part(d.solid(), DOWNPIPE_PART - k as u32));
    }
}

/// Körper eines Wandzugs für 3D und Ansichten: die Wände (Teil = Segment),
/// dann Gründung, Decke und Flachdach in dieser Reihenfolge (sie bestimmt die
/// Reihenfolge im Netz).
pub fn run_solid(
    chain: &WallChain,
    found: Option<&Foundation>,
    floor: Option<&FloorSlab>,
    roof: Option<&FlatRoof>,
) -> Solid {
    let mut s = chain.solid();
    if let Some(f) = found {
        foundation_solid(f, &mut s);
    }
    if let Some(f) = floor {
        floor_solid(f, &mut s);
    }
    if let Some(r) = roof {
        roof_solid(r, &mut s);
    }
    s
}

impl Model {
    /// 3D-Körper des Wandzugs `id` wie in der Ansicht ([`run_solid`]).
    pub fn run_body(&self, id: RunId) -> Option<Solid> {
        let (chain, floor) = self.chain_and_floor(id)?;
        let found = self.foundation(id).and_then(Result::ok);
        let floor = floor.and_then(Result::ok);
        let roof = self.flat_roof_on(id, &chain);
        Some(run_solid(
            &chain,
            found.as_ref(),
            floor.as_ref(),
            roof.as_ref(),
        ))
    }

    /// Körper eines Erweiterungsbauteils wie in der Ansicht.
    pub fn ext_solid(&self, id: ElementId) -> Option<Solid> {
        let ElementKind::Ext(p) = &self.element(id)?.kind else {
            return None;
        };
        let d = self.ext_def(&p.key)?;
        let (lage, erg) = self.ext_lage(id)?;
        let (erg, _) = erweiterung_koerper::begrenzt(erg);
        let mat = |b: &str| self.ext_material(d, b).map_or(0, crate::material_key);
        Some(erweiterung_koerper::solid(&erg, &lage, &mat))
    }

    /// Körper je Bauteil in Weltkoordinaten (mm), nach Bauteil geordnet
    /// (Platz in der Arena): die Körper der Wandzüge getrennt nach
    /// [`Model::part_of`], dazu die Erweiterungsbauteile. Teile ohne Bauteil
    /// fallen weg.
    pub fn element_solids(&self) -> Vec<(ElementId, Solid)> {
        let mut out: Vec<(ElementId, Solid)> = Vec::new();
        let mut at: HashMap<ElementId, usize> = HashMap::new();
        let mut slot = |id: ElementId, out: &mut Vec<(ElementId, Solid)>| {
            *at.entry(id).or_insert_with(|| {
                out.push((id, Solid::default()));
                out.len() - 1
            })
        };
        let runs: Vec<RunId> = self.runs().ids().collect();
        for run in runs {
            let Some(s) = self.run_body(run) else {
                continue;
            };
            // Teil → Bauteil einmal je Teil nachschlagen
            let mut of: HashMap<u32, Option<ElementId>> = HashMap::new();
            let mut of_part = |p: u32| *of.entry(p).or_insert_with(|| self.part_of(run, p));
            for t in &s.triangles {
                if let Some(id) = of_part(t.elem) {
                    let k = slot(id, &mut out);
                    out[k].1.triangles.push(*t);
                }
            }
            for e in &s.edges {
                if let Some(id) = of_part(e.elem) {
                    let k = slot(id, &mut out);
                    out[k].1.edges.push(*e);
                }
            }
        }
        let ext: Vec<ElementId> = self
            .elements()
            .iter()
            .filter(|(_, e)| matches!(e.kind, ElementKind::Ext(_)))
            .map(|(id, _)| id)
            .collect();
        for id in ext {
            if let Some(s) = self.ext_solid(id) {
                out.push((id, s));
            }
        }
        out.sort_by_key(|(id, _)| id.index());
        out
    }
}
