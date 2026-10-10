//! Geschossbänder (B11): Methoden von `Model`, aus `model.rs` ausgelagert.

use super::*;

impl Model {
    /// Absolute Höhe eines Höhenbezugs (mm), `None` ohne das Geschoss.
    pub fn level_z(&self, r: LevelRef) -> Option<f64> {
        let s = self.storey(r.storey)?;
        Some(
            match r.edge {
                LevelEdge::Bottom => s.elevation,
                LevelEdge::Top => s.top(),
            } + r.offset,
        )
    }

    /// Geschosse des Gebäudes `b` von unten nach oben (`None`: die Vorlage).
    pub fn levels_in(&self, b: Option<BuildingId>) -> Vec<StoreyId> {
        let mut v: Vec<(StoreyId, f64)> = self
            .storeys
            .iter()
            .filter(|(_, s)| s.building == b)
            .map(|(id, s)| (id, s.elevation))
            .collect();
        v.sort_by(|a, b| a.1.total_cmp(&b.1));
        v.into_iter().map(|(id, _)| id).collect()
    }

    /// Geschosse des Gebäudes, zu dem `id` gehört, von unten nach oben.
    pub fn group_levels(&self, id: StoreyId) -> Vec<StoreyId> {
        match self.storey(id) {
            Some(s) => self.levels_in(s.building),
            None => Vec::new(),
        }
    }

    /// Geschosse des ersten Gebäudes (bzw. der Vorlage) von unten nach oben.
    pub fn levels(&self) -> Vec<StoreyId> {
        self.group_levels(self.defaults.storey)
    }

    /// Das Gründungsband des ersten Gebäudes (bzw. der Vorlage).
    pub fn foundation_level(&self) -> Option<StoreyId> {
        self.foundation_level_of(self.defaults.storey)
    }

    /// Das Gründungsband des Gebäudes, zu dem `id` gehört.
    pub fn foundation_level_of(&self, id: StoreyId) -> Option<StoreyId> {
        self.group_levels(id).into_iter().find(|g| {
            self.storey(*g)
                .is_some_and(|s| s.kind == LevelKind::Foundation)
        })
    }

    /// Geschoss über `id` im selben Gebäude.
    pub fn level_above(&self, id: StoreyId) -> Option<StoreyId> {
        let l = self.group_levels(id);
        let i = l.iter().position(|x| *x == id)?;
        l.get(i + 1).copied()
    }

    /// Geschoss unter `id` im selben Gebäude (beim EG die Gründung).
    pub fn level_below(&self, id: StoreyId) -> Option<StoreyId> {
        let l = self.group_levels(id);
        let i = l.iter().position(|x| *x == id)?;
        l.get(i.checked_sub(1)?).copied()
    }

    /// Dickste Decke an der OK des Geschosses `id` (bestimmt dessen lichte
    /// Höhe), ohne Decke der Standardwert.
    pub(super) fn floor_thickness_of(&self, id: StoreyId) -> f64 {
        self.elements
            .iter()
            .filter_map(|(_, e)| match e.kind {
                ElementKind::Floor(f) if f.top.storey == id => Some(f.thickness),
                _ => None,
            })
            .reduce(f64::max)
            .unwrap_or(FLOOR_THICKNESS)
    }

    /// Dickste Sohlplatte im Gebäude der Gründung `gr`, ohne Platte der
    /// Standardwert.
    pub fn max_slab_thickness(&self, gr: StoreyId) -> f64 {
        let b = self.building_of(gr);
        self.elements
            .iter()
            .filter_map(|(_, e)| match e.kind {
                ElementKind::GroundSlab(s) if self.building_of(s.top.storey) == b => {
                    Some(s.thickness)
                }
                _ => None,
            })
            .reduce(f64::max)
            .unwrap_or(SLAB_THICKNESS)
    }

    /// Lichte Höhe eines Geschosses: Geschosshöhe minus Deckendicke.
    pub fn clear_height(&self, id: StoreyId) -> f64 {
        self.storey(id)
            .map_or(0.0, |s| s.height - self.floor_thickness_of(id))
    }

    /// Erlaubter Bereich der Oberkante eines Geschosses; `None`, wenn sie
    /// fest liegt (Gründung: ±0,00). Die lichte Höhe bleibt mindestens
    /// 1,00 m; nach oben frei, die Wände gehen mit (B12).
    pub fn storey_top_range(&self, id: StoreyId) -> Option<(f64, f64)> {
        let s = self.storey(id)?;
        match s.kind {
            LevelKind::Foundation => None,
            // Flachdach: OK Aufkantung über OK Rohdecke darunter
            LevelKind::Roof => Some((
                s.elevation + MIN_ROOF_UPSTAND,
                s.elevation + MAX_ROOF_UPSTAND,
            )),
            LevelKind::Storey => Some((
                s.elevation + self.floor_thickness_of(id) + MIN_CLEAR,
                f64::INFINITY,
            )),
        }
    }

    /// Tiefste Unterkante von Sohlplatte samt Perimeterdämmung im Gebäude
    /// der Gründung `gr` (mm unter OK Platte), ohne Platte der Standardwert.
    pub fn max_slab_depth(&self, gr: StoreyId) -> f64 {
        let b = self.building_of(gr);
        self.elements
            .iter()
            .filter_map(|(_, e)| match e.kind {
                ElementKind::GroundSlab(s) if self.building_of(s.top.storey) == b => {
                    Some(s.thickness + s.insulation.max(0.0))
                }
                _ => None,
            })
            .reduce(f64::max)
            .unwrap_or(SLAB_THICKNESS)
    }

    /// Erlaubter Bereich der Unterkante der Gründung `gr`: die Schürze
    /// bleibt mindestens 10 cm hoch, und die Gründung reicht mindestens
    /// [`FROST_DEPTH`] unter OK Gelände (frostfrei, Gelände Thema 1).
    pub fn foundation_bottom_range_of(&self, gr: StoreyId) -> (f64, f64) {
        let eg = self
            .ground_storey(gr)
            .and_then(|e| self.storey(e))
            .map_or(0.0, |s| s.elevation);
        (
            eg - MAX_FOUNDATION,
            (eg - self.max_slab_depth(gr) - MIN_FOOTING)
                .min(self.terrain_z_of_level(gr) - FROST_DEPTH),
        )
    }

    /// Erlaubter Bereich der Unterkante der Gründung des ersten Gebäudes.
    pub fn foundation_bottom_range(&self) -> (f64, f64) {
        match self.foundation_level() {
            Some(gr) => self.foundation_bottom_range_of(gr),
            None => (
                -MAX_FOUNDATION,
                (-SLAB_THICKNESS - MIN_FOOTING).min(self.terrain_z() - FROST_DEPTH),
            ),
        }
    }

    /// Oberkante auf `z` geklemmt (Ziehen).
    pub fn clamp_storey_top(&self, id: StoreyId, z: f64) -> Option<f64> {
        let (lo, hi) = self.storey_top_range(id)?;
        Some(z.min(hi).max(lo))
    }

    /// Verschiebt die Oberkante ohne Prüfung; die Bänder darüber (im selben
    /// Gebäude) wandern mit.
    fn move_storey_top(&mut self, id: StoreyId, z: f64) {
        let Some(old) = self.storey(id).map(|s| s.top()) else {
            return;
        };
        let delta = z - old;
        if delta == 0.0 {
            return;
        }
        let levels = self.group_levels(id);
        let Some(i) = levels.iter().position(|x| *x == id) else {
            return;
        };
        note!(self, Storey, self.storeys, id);
        if let Some(s) = self.storeys.get_mut(id) {
            s.height += delta;
        }
        for &above in &levels[i + 1..] {
            note!(self, Storey, self.storeys, above);
            if let Some(s) = self.storeys.get_mut(above) {
                s.elevation += delta;
            }
        }
        self.touch();
    }

    /// Oberkante eines Geschosses als Zahl (mm). Außerhalb der Grenzen
    /// abgelehnt; ±0,00 liegt fest.
    pub fn set_storey_top(&mut self, id: StoreyId, z: f64) -> bool {
        match self.storey_top_range(id) {
            Some((lo, hi)) if z.is_finite() && z >= lo - 1e-9 && z <= hi + 1e-9 => {
                self.move_storey_top(id, z);
                true
            }
            _ => false,
        }
    }

    /// Oberkante beim Ziehen: an den Grenzen geklemmt.
    pub fn drag_storey_top(&mut self, id: StoreyId, z: f64) -> bool {
        match self.clamp_storey_top(id, z) {
            Some(z) if z.is_finite() => {
                self.move_storey_top(id, z);
                true
            }
            _ => false,
        }
    }

    /// Geschosshöhe eines Geschosses ohne Grenzprüfung (Vorgaben des Dialogs
    /// „Gebäude erstellen“, die der Aufrufer prüft); die Geschosse darüber
    /// rücken mit.
    pub fn plan_storey_height(&mut self, id: StoreyId, h: f64) {
        if let Some(e) = self.storey(id).map(|s| s.elevation) {
            if h.is_finite() && h > 0.0 {
                self.move_storey_top(id, e + h);
            }
        }
    }

    /// Geschosshöhe als Zahl; bei der Gründung die Gründungstiefe.
    pub fn set_storey_height(&mut self, id: StoreyId, h: f64) -> bool {
        let Some(s) = self.storey(id) else {
            return false;
        };
        match s.kind {
            LevelKind::Foundation => self.set_foundation_depth_of(id, h),
            _ => {
                let z = s.elevation + h;
                self.set_storey_top(id, z)
            }
        }
    }

    /// Lichte Höhe als Zahl: OK = UK + lichte Höhe + Deckendicke.
    pub fn set_clear_height(&mut self, id: StoreyId, h: f64) -> bool {
        let Some(e) = self.storey(id).map(|s| s.elevation) else {
            return false;
        };
        let z = e + h + self.floor_thickness_of(id);
        self.set_storey_top(id, z)
    }

    /// Unterkante der Gründung `gr` ohne Prüfung.
    pub(super) fn move_foundation_bottom(&mut self, gr: StoreyId, z: f64) {
        let Some(top) = self.storey(gr).map(|s| s.top()) else {
            return;
        };
        if self.storey(gr).is_some_and(|s| s.elevation == z) {
            return;
        }
        note!(self, Storey, self.storeys, gr);
        if let Some(s) = self.storeys.get_mut(gr) {
            s.elevation = z;
            s.height = top - z;
        }
        self.touch();
    }

    /// Unterkante der Gründung `gr` als Zahl (mm); außerhalb abgelehnt.
    pub fn set_foundation_bottom_of(&mut self, gr: StoreyId, z: f64) -> bool {
        let (lo, hi) = self.foundation_bottom_range_of(gr);
        if !(z.is_finite() && z >= lo - 1e-9 && z <= hi + 1e-9) {
            return false;
        }
        self.move_foundation_bottom(gr, z);
        self.set_embed(gr, None);
        true
    }

    /// Unterkante der Gründung des ersten Gebäudes als Zahl (mm).
    pub fn set_foundation_bottom(&mut self, z: f64) -> bool {
        self.foundation_level()
            .is_some_and(|gr| self.set_foundation_bottom_of(gr, z))
    }

    /// Unterkante der Gründung `gr` beim Ziehen, geklemmt.
    pub fn drag_foundation_bottom_of(&mut self, gr: StoreyId, z: f64) -> bool {
        if !z.is_finite() || self.storey(gr).is_none() {
            return false;
        }
        let (lo, hi) = self.foundation_bottom_range_of(gr);
        self.move_foundation_bottom(gr, z.min(hi).max(lo));
        self.set_embed(gr, None);
        true
    }

    /// Unterkante der Gründung des ersten Gebäudes beim Ziehen, geklemmt.
    pub fn drag_foundation_bottom(&mut self, z: f64) -> bool {
        self.foundation_level()
            .is_some_and(|gr| self.drag_foundation_bottom_of(gr, z))
    }

    /// Gründungstiefe der Gründung `gr` als Zahl: UK Gründung = −Tiefe.
    pub fn set_foundation_depth_of(&mut self, gr: StoreyId, h: f64) -> bool {
        let eg = self
            .ground_storey(gr)
            .and_then(|e| self.storey(e))
            .map_or(0.0, |s| s.elevation);
        self.set_foundation_bottom_of(gr, eg - h)
    }

    /// Gründungstiefe des ersten Gebäudes als Zahl.
    pub fn set_foundation_depth(&mut self, h: f64) -> bool {
        self.foundation_level()
            .is_some_and(|gr| self.set_foundation_depth_of(gr, h))
    }

    /// Prüfregeln der Geschossbänder (B11, B12): je Gebäude eine Gründung
    /// zuunterst und ein EG bei ±0,00, lückenlos.
    pub(super) fn check_levels(&self) -> Vec<String> {
        let mut out = Vec::new();
        let mut groups: Vec<Option<BuildingId>> =
            self.buildings.ids().map(Some).collect::<Vec<_>>();
        if self.storeys.iter().any(|(_, s)| s.building.is_none()) {
            if !self.buildings.is_empty() {
                out.push("Geschosse: Vorlage ohne Gebäude neben einem Gebäude".into());
            }
            groups.push(None);
        }
        for (_, s) in self.storeys.iter() {
            if s.building.is_some_and(|b| !self.buildings.contains(b)) {
                out.push(format!("Geschoss {}: Gebäude fehlt", s.short));
            }
        }
        for g in groups {
            let name = g
                .and_then(|b| self.building(b))
                .map_or("Vorlage".to_string(), |b| b.number.clone());
            let levels = self.levels_in(g);
            let found = levels
                .iter()
                .filter(|id| {
                    self.storey(**id)
                        .is_some_and(|s| s.kind == LevelKind::Foundation)
                })
                .count();
            if found != 1 || levels.len() < 2 {
                out.push(format!(
                    "Geschosse {name}: nicht genau eine Gründung und mindestens ein EG"
                ));
            }
            // Flachdach (Jörn 10.10.): höchstens eins, zuoberst, über einem
            // Geschoss
            let roofs: Vec<usize> = levels
                .iter()
                .enumerate()
                .filter(|(_, id)| self.storey(**id).is_some_and(|s| s.kind == LevelKind::Roof))
                .map(|(i, _)| i)
                .collect();
            if roofs.len() > 1 {
                out.push(format!("Geschosse {name}: mehr als ein Flachdach"));
            }
            if roofs.iter().any(|&i| {
                i + 1 != levels.len()
                    || !levels[..i].iter().any(|id| {
                        self.storey(*id)
                            .is_some_and(|s| s.kind == LevelKind::Storey)
                    })
            }) {
                out.push(format!(
                    "Geschosse {name}: Flachdach nicht zuoberst über einem Geschoss"
                ));
            }
            let first = levels.first().and_then(|id| self.storey(*id));
            if first.is_some_and(|s| s.kind != LevelKind::Foundation) {
                out.push(format!(
                    "Geschosse {name}: Gründung ist nicht das unterste Band"
                ));
            }
            for w in levels.windows(2) {
                let (a, b) = (self.storey(w[0]), self.storey(w[1]));
                if let (Some(a), Some(b)) = (a, b) {
                    if (a.top() - b.elevation).abs() > 1e-6 {
                        out.push(format!(
                            "Geschosse {} und {} nicht lückenlos",
                            a.short, b.short
                        ));
                    }
                }
            }
            if self
                .ground_of(g)
                .and_then(|e| self.storey(e))
                .is_none_or(|s| s.elevation != 0.0)
            {
                out.push(format!("Geschosse {name}: UK EG liegt nicht bei ±0,00"));
            }
        }
        for (_, s) in self.storeys.iter() {
            if !(s.height > 0.0 && s.height.is_finite()) {
                out.push(format!("Geschoss {}: Höhe {} ungültig", s.short, s.height));
            }
        }
        let mut refs: Vec<LevelRef> = self
            .runs
            .iter()
            .flat_map(|(_, r)| [r.base, r.top])
            .collect();
        for (_, e) in self.elements.iter() {
            match e.kind {
                ElementKind::GroundSlab(s) => refs.push(s.top),
                ElementKind::StripFooting(f) => refs.push(f.base),
                ElementKind::Floor(f) => refs.push(f.top),
                ElementKind::Wall(_)
                | ElementKind::PerimeterInsulation { .. }
                | ElementKind::EdgeStrip { .. }
                | ElementKind::SoffitInsulation { .. }
                | ElementKind::RoofTerrace { .. }
                | ElementKind::Coping { .. }
                | ElementKind::Roof { .. }
                | ElementKind::Ext(_) => {}
            }
        }
        if refs.iter().any(|r| !self.storeys.contains(r.storey)) {
            out.push("Höhenbezug auf ein fehlendes Geschoss".into());
        }
        out
    }
}
