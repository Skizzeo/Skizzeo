//! Gelände und Perimeterdämmung (Gelände Themen 1 und 4): Methoden von `Model`, aus `model.rs` ausgelagert.

use super::*;

impl Model {
    /// Versatz OK Sohlplatte (±0,00) über OK Gelände des aktiven Gebäudes
    /// (mm); + = das Gebäude sitzt höher. Alte Projekte: 0.
    pub fn terrain_offset(&self) -> f64 {
        self.terrain_offset_of(self.building_of(self.defaults.storey))
    }

    /// Versatz OK Sohle über Gelände eines Gebäudes (mm); ohne Gebäude 0.
    pub fn terrain_offset_of(&self, b: Option<BuildingId>) -> f64 {
        b.and_then(|b| self.building(b)).map_or(0.0, |b| b.terrain)
    }

    /// OK Gelände am aktiven Gebäude (mm, relativ zu ±0,00).
    pub fn terrain_z(&self) -> f64 {
        self.terrain_z_of(self.building_of(self.defaults.storey))
    }

    /// OK Gelände an einem Gebäude (mm, relativ zu ±0,00).
    pub fn terrain_z_of(&self, b: Option<BuildingId>) -> f64 {
        -self.terrain_offset_of(b)
    }

    /// OK Gelände an der Stelle `(x, y)` (mm, relativ zu ±0,00), wie sie
    /// Gebäude `b` sieht. Heute eine Ebene je Gebäude; ein späteres
    /// Geländemodell (Höhenlinien) ersetzt nur diese Abfrage.
    pub fn terrain_z_at(&self, b: Option<BuildingId>, x: f64, y: f64) -> f64 {
        let _ = (x, y);
        self.terrain_z_of(b)
    }

    /// Geländeflächen für die Darstellung: je Gebäude mit Wänden das
    /// Rechteck um seine Wandzüge im Grundriss, um `margin` vergrößert
    /// (`[x0, y0, x1, y1]`, mm), mit seiner Höhe von OK Gelände. Nach Guid
    /// geordnet; wo sich Rechtecke überlappen, gilt das erste.
    pub fn terrain_patches(&self, margin: f64) -> Vec<([f64; 4], f64)> {
        let mut boxes: Vec<(Guid, BuildingId, [f64; 4])> = Vec::new();
        for (_, r) in self.runs.iter() {
            let Some(b) = self.building_of(r.storey) else {
                continue;
            };
            let Some(guid) = self.building(b).map(|x| x.guid) else {
                continue;
            };
            let i = match boxes.iter().position(|x| x.1 == b) {
                Some(i) => i,
                None => {
                    let e = f64::INFINITY;
                    boxes.push((guid, b, [e, e, -e, -e]));
                    boxes.len() - 1
                }
            };
            let bx = &mut boxes[i].2;
            for p in &r.points {
                bx[0] = bx[0].min(p.x);
                bx[1] = bx[1].min(p.y);
                bx[2] = bx[2].max(p.x);
                bx[3] = bx[3].max(p.y);
            }
        }
        boxes.retain(|x| x.2[0] <= x.2[2]);
        boxes.sort_by_key(|x| x.0);
        boxes
            .into_iter()
            .map(|(_, b, x)| {
                let m = margin;
                (
                    [x[0] - m, x[1] - m, x[2] + m, x[3] + m],
                    self.terrain_z_of(Some(b)),
                )
            })
            .collect()
    }

    /// OK Gelände an der Gründung `gr` (ihr Gebäude).
    pub(super) fn terrain_z_of_level(&self, gr: StoreyId) -> f64 {
        self.terrain_z_of(self.building_of(gr))
    }

    /// Einbindetiefe der Gründung `gr`: OK Gelände bis UK Gründung (mm).
    pub fn embedment_of(&self, gr: StoreyId) -> Option<f64> {
        Some(self.terrain_z_of_level(gr) - self.storey(gr)?.elevation)
    }

    /// Setzt den Versatz OK Sohlplatte über OK Gelände des aktiven
    /// Gebäudes (mm), siehe [`Model::set_terrain_offset_of`].
    pub fn set_terrain_offset(&mut self, offset: f64) -> bool {
        self.building_of(self.defaults.storey)
            .is_some_and(|b| self.set_terrain_offset_of(b, offset))
    }

    /// Setzt den Versatz OK Sohlplatte über OK Gelände eines Gebäudes (mm).
    /// Seine Gründung behält ihre Einbindetiefe, wandert also mit dem
    /// Gelände; wird die Schürze dabei kürzer als 10 cm, bleibt sie 10 cm
    /// (die Einbindung wächst). Nie flacher als [`FROST_DEPTH`]: Fehlt
    /// Tiefe, wächst die Schürze. Außerhalb ±[`MAX_TERRAIN_OFFSET`]
    /// abgelehnt. Andere Gebäude bleiben, wie sie sind.
    pub fn set_terrain_offset_of(&mut self, b: BuildingId, offset: f64) -> bool {
        if !(offset.is_finite() && offset.abs() <= MAX_TERRAIN_OFFSET + 1e-9) {
            return false;
        }
        let Some(old) = self.building(b).map(|x| x.terrain) else {
            return false;
        };
        let delta = offset - old;
        if delta == 0.0 {
            return false;
        }
        self.put_terrain(b, offset);
        self.follow_terrain(b, delta);
        true
    }

    /// Schreibt den Versatz eines Gebäudes (mit Rückgängig).
    fn put_terrain(&mut self, b: BuildingId, offset: f64) {
        note!(self, Building, self.buildings, b);
        if let Some(x) = self.buildings.get_mut(b) {
            x.terrain = offset;
        }
        self.touch();
    }

    /// Die Gründungen von Gebäude `b` folgen seinem um `delta` gesunkenen
    /// Gelände (Versatz um `delta` gewachsen): UK Gründung liegt wieder um
    /// die gewollte Einbindetiefe unter OK Gelände, geklemmt auf den
    /// erlaubten Bereich (Schürze mindestens 10 cm, nie flacher als die
    /// Frosttiefe). Stößt sie an eine Grenze, merkt sich das Geschoss die
    /// gewollte Tiefe; Hin und zurück ergibt so dieselbe Gründung.
    fn follow_terrain(&mut self, b: BuildingId, delta: f64) {
        let grs: Vec<StoreyId> = self
            .storeys
            .iter()
            .filter(|(_, s)| s.kind == LevelKind::Foundation && s.building == Some(b))
            .map(|(id, _)| id)
            .collect();
        let new = self.terrain_z_of(Some(b));
        let old = new + delta;
        for gr in grs {
            let Some(s) = self.storey(gr) else {
                continue;
            };
            let want = s.embed.unwrap_or(old - s.elevation);
            let (lo, hi) = self.foundation_bottom_range_of(gr);
            let z = (new - want).min(hi).max(lo);
            self.move_foundation_bottom(gr, z);
            let embed = ((new - z) - want).abs() > 1e-6;
            self.set_embed(gr, embed.then_some(want));
        }
    }

    /// Setzt die gewollte Einbindetiefe einer Gründung (siehe
    /// [`Storey::embed`]).
    pub(super) fn set_embed(&mut self, gr: StoreyId, embed: Option<f64>) {
        if self.storey(gr).is_none_or(|s| s.embed == embed) {
            return;
        }
        note!(self, Storey, self.storeys, gr);
        if let Some(s) = self.storeys.get_mut(gr) {
            s.embed = embed;
        }
        self.touch();
    }

    /// Merkt das Projekt im offenen Schritt.
    fn note_project(&mut self) {
        match self.txn.as_mut() {
            Some(t) => {
                if t.noted.insert(Key::Project) {
                    t.changes.push(Change::Project {
                        old: Box::new(self.project.clone()),
                        new: Box::new(self.project.clone()),
                    });
                }
            }
            None => debug_assert!(!self.strict, "Änderung ohne Schritt"),
        }
    }

    /// Bodenkennwerte der Erdarbeiten (im offenen Schritt). Werte außerhalb
    /// von [`crate::qto_earth::Boden::FELDER`] und gleiche Werte ändern
    /// nichts. Die erste Änderung zieht die Werte von `[project]` nach
    /// `[soil]` um (Regel 113), im selben Schritt.
    pub fn set_soil(&mut self, b: crate::qto_earth::Boden) -> bool {
        if b == self.project.soil || crate::qto_earth::Boden::aus_werten(b.werte()).is_none() {
            return false;
        }
        self.note_project();
        self.project.soil = b;
        self.project.soil_info = true;
        self.project.soil_legacy = [None; 5];
        self.touch();
        true
    }

    /// Ist die Gründung `gr` frostfrei (Einbindetiefe ≥ 80 cm)? Alte Dateien
    /// können flacher sein, bis man sie ändert.
    pub fn frost_safe(&self, gr: StoreyId) -> bool {
        self.embedment_of(gr)
            .is_none_or(|e| e >= FROST_DEPTH - 1e-6)
    }

    /// Dicke der Perimeterdämmung unter einer Sohlplatte (mm), 0 = keine.
    pub fn slab_insulation(&self, slab: ElementId) -> f64 {
        match self.element(slab).map(|e| &e.kind) {
            Some(ElementKind::GroundSlab(s)) => s.insulation.max(0.0),
            _ => 0.0,
        }
    }

    /// Setzt die Perimeterdämmung unter einer Sohlplatte (mm): 0 = keine,
    /// sonst [`MIN_PERIMETER`] bis [`MAX_PERIMETER`]. Die Dämmung baut sich
    /// nach unten auf (Jörn 11.10.): OK Sohlplatte, Versatz zum Gelände und
    /// UK Gründung bleiben, die Schürze behält ihre Höhe. Unter der Dämmung
    /// bleibt die Schürze mindestens [`MIN_FOOTING`] tief; ist sie dafür zu
    /// flach, sinkt UK Gründung um den Rest.
    pub fn set_slab_insulation(&mut self, slab: ElementId, t: f64) -> bool {
        let ok = t == 0.0 || (MIN_PERIMETER - 1e-9..=MAX_PERIMETER + 1e-9).contains(&t);
        let old = self.slab_insulation(slab);
        if !ok || !t.is_finite() || t == old {
            return false;
        }
        if !matches!(
            self.element(slab).map(|e| &e.kind),
            Some(ElementKind::GroundSlab(_))
        ) {
            return false;
        }
        if t > 0.0 && self.ensure_perimeter_material().is_none() {
            return false;
        }
        self.edit_slab(slab, |s| s.insulation = t);
        // Unter der Dämmung bleibt die Schürze mindestens 10 cm tief: Ist
        // sie zu flach, wächst sie nach unten (UK Gründung sinkt), sonst
        // bleibt alles stehen
        let gr = self
            .element(slab)
            .and_then(|e| self.foundation_level_of(e.storey));
        if let Some(gr) = gr {
            let (lo, hi) = self.foundation_bottom_range_of(gr);
            let z = self.storey(gr).map_or(hi, |s| s.elevation);
            if z > hi + 1e-9 {
                if hi < lo - 1e-9 {
                    self.edit_slab(slab, |s| s.insulation = old);
                    return false;
                }
                self.move_foundation_bottom(gr, hi);
            }
        }
        self.sync_perimeter(slab);
        self.touch();
        true
    }

    /// Perimeterdämmungen unter einer Sohlplatte (richtig: höchstens eine).
    pub(super) fn perimeters_of(&self, slab: ElementId) -> Vec<ElementId> {
        self.elements
            .iter()
            .filter(|(_, e)| e.kind == ElementKind::PerimeterInsulation { slab })
            .map(|(id, _)| id)
            .collect()
    }

    /// Perimeterdämmung unter einer Sohlplatte.
    pub fn perimeter_of(&self, slab: ElementId) -> Option<ElementId> {
        self.perimeters_of(slab).first().copied()
    }

    /// Genau eine Perimeterdämmung unter einer gedämmten Platte, keine unter
    /// einer ungedämmten.
    pub(super) fn sync_perimeter(&mut self, slab: ElementId) {
        let want = self.slab_insulation(slab) > 0.0;
        let have = self.perimeters_of(slab);
        for (i, id) in have.iter().enumerate() {
            if !want || i > 0 {
                note!(self, Element, self.elements, *id);
                self.elements.remove(*id);
                self.touch();
            }
        }
        if want && have.is_empty() {
            let Some(storey) = self.element(slab).map(|e| e.storey) else {
                return;
            };
            self.new_element(
                Category::PerimeterInsulation,
                storey,
                PERIMETER_SEQ,
                ElementKind::PerimeterInsulation { slab },
            );
            self.touch();
        }
    }

    /// Nach dem Laden: Perimeterdämmungen passend zu den Platten.
    pub(crate) fn complete_perimeters(&mut self) {
        let strict = std::mem::replace(&mut self.strict, false);
        let slabs: Vec<ElementId> = self
            .elements
            .iter()
            .filter(|(_, e)| matches!(e.kind, ElementKind::GroundSlab(_)))
            .map(|(id, _)| id)
            .collect();
        for slab in slabs {
            self.sync_perimeter(slab);
        }
        let orphans: Vec<ElementId> = self
            .elements
            .iter()
            .filter(|(_, e)| match e.kind {
                ElementKind::PerimeterInsulation { slab } => !matches!(
                    self.element(slab).map(|x| &x.kind),
                    Some(ElementKind::GroundSlab(_))
                ),
                _ => false,
            })
            .map(|(id, _)| id)
            .collect();
        for id in orphans {
            self.elements.remove(id);
        }
        self.strict = strict;
    }

    /// Baustoff der Perimeterdämmung: XPS, sobald es ihn im Projekt gibt,
    /// sonst die erste Dämmung der Bibliothek.
    pub fn perimeter_material(&self) -> Option<MaterialId> {
        self.material_by_guid(PERIMETER_MAT_GUID).or_else(|| {
            self.materials
                .iter()
                .find(|(_, m)| m.category == MatCategory::Insulation)
                .map(|(id, _)| id)
        })
    }

    /// XPS für die Perimeterdämmung; fehlt er, wird er angelegt.
    fn ensure_perimeter_material(&mut self) -> Option<MaterialId> {
        self.builtin_material(
            PERIMETER_MAT_GUID,
            "XPS Perimeterdämmung",
            MatCategory::Insulation,
            300,
            33.0,
            Some(0.035),
            [178, 212, 226],
            [150, 190, 210],
            trade::start_id("18331"),
        )
    }

    /// Grundlagen der Erdarbeiten je Gründung (Schnittstelle
    /// gelaende/schnittstelle.md): Höhen, Umriss, Flächen und Längen.
    /// `None` ohne Gründung oder ohne Körper.
    pub fn ground_basis(&self, run: RunId) -> Option<GroundBasis> {
        let (slab, footing) = self.foundation_of(run)?;
        let footing = footing?;
        let f = self.foundation(run)?.ok()?;
        let ElementKind::GroundSlab(s) = self.element(slab)?.kind else {
            return None;
        };
        let ElementKind::StripFooting(sf) = self.element(footing)?.kind else {
            return None;
        };
        let slab_top_z = self.level_z(s.top)?;
        let slab_bottom_z = slab_top_z - s.thickness;
        let insulation = s.insulation.max(0.0);
        let footing_bottom_z = self.level_z(sf.base)?;
        let building = self.element(slab).and_then(|e| self.building_of(e.storey));
        let terrain_z = self.terrain_z_of(building);
        Some(GroundBasis {
            run,
            slab,
            footing,
            building,
            terrain_z,
            slab_top_z,
            slab_bottom_z,
            insulation,
            insulation_bottom_z: slab_bottom_z - insulation,
            footing_bottom_z,
            footing_width: sf.width,
            embedment: terrain_z - footing_bottom_z,
            slab_area: f.slab_area(),
            slab_perimeter: f.slab_perimeter(),
            footing_area: f.footing_area(),
            footing_axis_length: f.footing_axis_length(),
            outline: f.outline,
        })
    }

    /// [`Model::ground_basis`] aller Gründungen, nach Bauteilnummer der Platte.
    pub fn ground_bases(&self) -> Vec<GroundBasis> {
        let mut v: Vec<(String, GroundBasis)> = self
            .elements
            .iter()
            .filter_map(|(_, e)| match e.kind {
                ElementKind::GroundSlab(s) => Some((e.number.clone(), self.ground_basis(s.run)?)),
                _ => None,
            })
            .collect();
        v.sort_by(|a, b| a.0.cmp(&b.0));
        v.into_iter().map(|x| x.1).collect()
    }
}
