//! Regenfallrohre am Flachdach (Jörn 11.10., [`crate::fallrohr`]): Methoden
//! von `Model`. Je Ablauf eines Dachaufbaus genau ein Bauteil RF auf der
//! Ebene des Dachaufbaus; es kommt und geht mit den Abläufen.

use super::*;
use crate::fallrohr::{Downpipe, DownpipeSpec, Overflow, SPACING_MAX, SPACING_MIN};
use crate::roof::FlatRoof;

impl Model {
    /// Regenfallrohre des Dachaufbaus `roof`, nach der Nummer ihres
    /// Ablaufs.
    pub fn downpipes_of(&self, roof: ElementId) -> Vec<ElementId> {
        let mut out: Vec<(u16, ElementId)> = self
            .elements
            .iter()
            .filter_map(|(id, e)| match e.kind {
                ElementKind::Downpipe { roof: r, drain, .. } if r == roof => Some((drain, id)),
                _ => None,
            })
            .collect();
        out.sort_by_key(|x| x.0);
        out.into_iter().map(|x| x.1).collect()
    }

    /// Alle Regenfallrohre.
    pub(super) fn downpipe_ids(&self) -> Vec<ElementId> {
        self.elements
            .iter()
            .filter(|(_, e)| matches!(e.kind, ElementKind::Downpipe { .. }))
            .map(|(id, _)| id)
            .collect()
    }

    /// Regenfallrohr am Ablauf `k` des Dachaufbaus `roof`.
    pub fn downpipe_at(&self, roof: ElementId, k: usize) -> Option<ElementId> {
        self.elements
            .iter()
            .find(|(_, e)| {
                matches!(e.kind, ElementKind::Downpipe { roof: r, drain, .. }
                    if r == roof && drain as usize == k)
            })
            .map(|(id, _)| id)
    }

    /// Decke unter dem Dachaufbau `roof`.
    pub fn floor_of_roof(&self, roof: ElementId) -> Option<ElementId> {
        match self.element(roof)?.kind {
            ElementKind::Roof { floor, .. } => Some(floor),
            _ => None,
        }
    }

    /// Prüfung (Model::check): das Regenfallrohr hängt an einem Ablauf.
    pub(super) fn check_downpipe(
        &self,
        number: &str,
        roof: ElementId,
        drain: u16,
    ) -> Option<String> {
        let ok = matches!(self.element(roof).map(|e| &e.kind),
            Some(ElementKind::Roof { drainage, .. }) if (drain as usize) < drainage.drains.len());
        (!ok).then(|| format!("{number}: Regenfallrohr ohne Ablauf"))
    }

    /// Eigenschaften des Regenfallrohrs `id`.
    pub fn downpipe_spec(&self, id: ElementId) -> Option<DownpipeSpec> {
        match self.element(id)?.kind {
            ElementKind::Downpipe { spec, .. } => Some(spec),
            _ => None,
        }
    }

    /// Ausgelegtes Regenfallrohr `id` (Lage, Höhen, Schellen).
    pub fn downpipe(&self, id: ElementId) -> Option<Downpipe> {
        let ElementKind::Downpipe { roof, drain, .. } = self.element(id)?.kind else {
            return None;
        };
        let ElementKind::Roof { floor, .. } = self.element(roof)?.kind else {
            return None;
        };
        self.flat_roof_over(floor)?
            .downpipes
            .get(drain as usize)
            .cloned()
    }

    /// Setzt die Eigenschaften des Regenfallrohrs `id`; Schellenabstand
    /// [`SPACING_MIN`] bis [`SPACING_MAX`].
    pub fn set_downpipe_spec(&mut self, id: ElementId, spec: DownpipeSpec) -> bool {
        let Some(old) = self.downpipe_spec(id) else {
            return false;
        };
        if !(SPACING_MIN..=SPACING_MAX).contains(&spec.spacing) {
            return false;
        }
        if old == spec {
            return true;
        }
        note!(self, Element, self.elements, id);
        if let Some(ElementKind::Downpipe { spec: s, .. }) =
            self.elements.get_mut(id).map(|e| &mut e.kind)
        {
            *s = spec;
        }
        self.touch();
        true
    }

    /// Regenfallrohre an den Abläufen des Flachdachs über der Decke
    /// `floor`, dessen Geometrie `r` gerade entsteht; Eigenschaften vom
    /// Bauteil, sonst die Vorgabe.
    pub(super) fn layout_downpipes(&self, floor: ElementId, r: &FlatRoof) -> Vec<Downpipe> {
        let Some(roof) = self.flat_roof_of(floor) else {
            return Vec::new();
        };
        let Some(ElementKind::Roof { drainage, .. }) = self.element(roof).map(|e| &e.kind) else {
            return Vec::new();
        };
        let b = self.building_of_element(roof);
        drainage
            .drains
            .iter()
            .take(MAX_DOWNPIPES as usize)
            .enumerate()
            .filter_map(|(k, p)| {
                let spec = self
                    .downpipe_at(roof, k)
                    .and_then(|d| self.downpipe_spec(d))
                    .unwrap_or_default();
                Downpipe::layout(r, *p, self.terrain_z_at(b, p.x, p.y), spec)
            })
            .collect()
    }

    /// Legt fehlende Regenfallrohre an und entfernt überzählige: je Ablauf
    /// eines Dachaufbaus im Bereich `scope` genau eins, auf dessen Ebene.
    /// Neue übernehmen die Eigenschaften eines Nachbarn am selben Dach.
    pub(super) fn sync_downpipes(&mut self, scope: Option<&[RunId]>) {
        let roofs: Vec<(ElementId, usize, StoreyId, u16)> = self
            .elements
            .iter()
            .filter_map(|(id, e)| match &e.kind {
                ElementKind::Roof { floor, drainage } if self.floor_in_scope(*floor, scope) => {
                    let n = drainage.drains.len().min(MAX_DOWNPIPES as usize);
                    Some((id, n, e.storey, e.seq))
                }
                _ => None,
            })
            .collect();
        let have: Vec<(ElementId, ElementId, u16, StoreyId, DownpipeSpec)> = self
            .elements
            .iter()
            .filter_map(|(id, e)| match e.kind {
                ElementKind::Downpipe { roof, drain, spec } => {
                    Some((id, roof, drain, e.storey, spec))
                }
                _ => None,
            })
            .collect();
        let mut kept: Vec<(ElementId, u16, DownpipeSpec)> = Vec::new();
        for (id, roof, k, storey, spec) in have {
            let keep = match roofs.iter().find(|r| r.0 == roof) {
                Some(r) => {
                    (k as usize) < r.1
                        && r.2 == storey
                        && !kept.iter().any(|x| x.0 == roof && x.1 == k)
                }
                // Dach außerhalb des Bereichs: bleibt, wie es ist
                None => matches!(
                    self.element(roof).map(|e| &e.kind),
                    Some(ElementKind::Roof { .. })
                ),
            };
            if keep {
                kept.push((roof, k, spec));
            } else {
                note!(self, Element, self.elements, id);
                self.elements.remove(id);
                self.touch();
            }
        }
        for (roof, n, storey, seq) in roofs {
            let spec = kept
                .iter()
                .find(|x| x.0 == roof)
                .map_or_else(DownpipeSpec::default, |x| x.2);
            for k in 0..n as u16 {
                if kept.iter().any(|x| x.0 == roof && x.1 == k) {
                    continue;
                }
                let kind = ElementKind::Downpipe {
                    roof,
                    drain: k,
                    spec,
                };
                self.new_element(Category::Downpipe, storey, seq, kind);
                kept.push((roof, k, spec));
                self.touch();
            }
        }
    }

    /// Abgeleitete Merkmale des Regenfallrohrs (Bauteileigenschaften):
    /// Nennweite, Längen, Schellen, Notentwässerung, Kontrollablauf.
    pub(super) fn downpipe_props(&self, id: ElementId) -> Vec<(String, PropValue)> {
        let Some(d) = self.downpipe(id) else {
            return Vec::new();
        };
        let mut out = vec![
            ("Nennweite".into(), PropValue::Text("DN 100".into())),
            ("Länge Fallrohr".into(), PropValue::Number(d.pipe_length())),
            (
                "Rohrschellen".into(),
                PropValue::Number(d.clamp_count() as f64),
            ),
            (
                "Notentwässerung".into(),
                PropValue::Text(
                    match d.spec.overflow {
                        Overflow::Pipe => "Notfallrohr",
                        Overflow::Spout => "Wasserspeier",
                    }
                    .into(),
                ),
            ),
            (
                "Kontrollablauf".into(),
                PropValue::Bool(d.control.is_some()),
            ),
        ];
        if d.spec.overflow == Overflow::Pipe {
            out.push((
                "Länge Notfallrohr".into(),
                PropValue::Number(d.overflow_length()),
            ));
        }
        out
    }
}
