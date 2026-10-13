//! Gelände und Perimeterdämmung (Gelände Themen 1 und 4): Methoden von `Model`, aus `model.rs` ausgelagert.

use super::geschossbaender::GruendungsBasis;
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
    /// Gebäude `b` sieht: aus dem Netz des Höhenaufmaßes (G1), ohne Aufmaß
    /// eine Ebene je Gebäude.
    pub fn terrain_z_at(&self, b: Option<BuildingId>, x: f64, y: f64) -> f64 {
        self.terrain_z_survey(x, y)
            .unwrap_or_else(|| self.terrain_z_of(b))
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

    /// OK Gelände an der Gründung `gr`: ohne Aufmaß die Ebene ihres
    /// Gebäudes, mit Aufmaß der tiefste Geländepunkt am Plattenrand; die
    /// Einbindetiefe zählt ab dort (plan-gelaendemodul.md §5 G1).
    pub(super) fn terrain_z_of_level(&self, gr: StoreyId) -> f64 {
        self.terrain_low_survey(gr)
            .unwrap_or_else(|| self.terrain_z_of(self.building_of(gr)))
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
        if offset == old {
            return false;
        }
        let vorher = self.foundation_terrain();
        self.put_terrain(b, offset);
        self.resettle_foundations(vorher);
        true
    }

    /// Beginnt einen Zug an OK Gelände des Gebäudes `b` (Geschossmanager,
    /// Review #123 B1 zum Gelände-Zug): merkt einmal OK Gelände je Gründung
    /// und den Bereich je Gebäude. `None` ohne Gebäude.
    pub fn gelaende_zug(&self, b: BuildingId) -> Option<GelaendeZug> {
        self.building(b)?;
        let vorher = self.foundation_terrain();
        let eigene = vorher
            .iter()
            .filter(|(gr, _)| self.building_of(*gr) == Some(b))
            .filter_map(|&(gr, t)| {
                let s = self.storey(gr)?;
                Some((gr, s.embed.unwrap_or(t - s.elevation)))
            })
            .collect();
        Some(GelaendeZug {
            haus: b,
            basis: self.gruendungs_basis(),
            vorher,
            eigene,
        })
    }

    /// Eine Mausbewegung im Zug: Versatz wie
    /// [`Model::set_terrain_offset_of`], aber nur die Gründungen des
    /// gezogenen Gebäudes folgen, mit der Einbindetiefe vom Beginn. Bestimmt
    /// es mit Aufmaß ±0,00, stehen die anderen bis zum Loslassen auf ihrer
    /// alten Gründung (Vorschau). Kostet so viel wie das Gebäude, nicht wie
    /// das Modell.
    pub fn gelaende_ziehen(&mut self, zug: &GelaendeZug, offset: f64) -> bool {
        if !(offset.is_finite() && offset.abs() <= MAX_TERRAIN_OFFSET + 1e-9) {
            return false;
        }
        let b = zug.haus;
        let Some(old) = self.building(b).map(|x| x.terrain) else {
            return false;
        };
        if offset == old {
            return false;
        }
        self.put_terrain(b, offset);
        for &(gr, want) in &zug.eigene {
            let t = self.terrain_z_of_level(gr);
            let bereich = zug.basis.bereich(Some(b), t);
            self.settle_foundation_in(gr, want, t, bereich);
        }
        true
    }

    /// Loslassen: Die übrigen Gründungen folgen ihrem Gelände in einem
    /// Gang, wie es [`Model::set_terrain_offset_of`] getan hätte. Mit
    /// Aufmaß hat sich OK Gelände für alle um dasselbe Δ bewegt; jede
    /// Gründung wandert um Δ, eine an einer Grenze bleibt dort und merkt
    /// sich ihre Tiefe. Die Gebäude, deren Unterkante sich bewegt hat.
    pub fn gelaende_loslassen(&mut self, zug: GelaendeZug) -> Vec<BuildingId> {
        let mut bewegt = std::collections::HashSet::new();
        for (gr, alt) in zug.vorher {
            if zug.eigene.iter().any(|e| e.0 == gr) {
                continue;
            }
            let t = self.terrain_z_of_level(gr);
            let Some(s) = self.storey(gr).filter(|_| (t - alt).abs() > 1e-6) else {
                continue;
            };
            let (b, z0, want) = (
                s.building,
                s.elevation,
                s.embed.unwrap_or(alt - s.elevation),
            );
            self.settle_foundation_in(gr, want, t, zug.basis.bereich(b, t));
            if self.storey(gr).is_some_and(|s| s.elevation != z0) {
                bewegt.extend(b);
            }
        }
        bewegt.into_iter().collect()
    }

    /// Schreibt den Versatz eines Gebäudes (mit Rückgängig). Die Lage im
    /// Aufmaß bleibt gemerkt, der Versatz kommt beim Lesen dazu.
    fn put_terrain(&mut self, b: BuildingId, offset: f64) {
        note!(self, Building, self.buildings, b);
        if let Some(x) = self.buildings.get_mut(b) {
            x.terrain = offset;
        }
        self.touch_height();
    }

    /// OK Gelände je Gründung, vor einer Änderung des Geländes gemerkt
    /// (für [`Model::resettle_foundations`]).
    pub(super) fn foundation_terrain(&self) -> Vec<(StoreyId, f64)> {
        let grs: Vec<StoreyId> = self
            .storeys
            .iter()
            .filter(|(_, s)| s.kind == LevelKind::Foundation)
            .map(|(id, _)| id)
            .collect();
        grs.into_iter()
            .map(|gr| (gr, self.terrain_z_of_level(gr)))
            .collect()
    }

    /// Die Gründungen folgen ihrem geänderten Gelände (Versatz, Aufmaß,
    /// Lage, Höhenlage): UK Gründung liegt wieder um die gewollte
    /// Einbindetiefe unter OK Gelände, geklemmt auf den erlaubten Bereich
    /// (Schürze mindestens 10 cm, nie flacher als die Frosttiefe). Stößt sie
    /// an eine Grenze, merkt sich das Geschoss die gewollte Tiefe; Hin und
    /// zurück ergibt so dieselbe Gründung. Wo sich das Gelände nicht
    /// bewegt hat, bleibt alles.
    pub(super) fn resettle_foundations(&mut self, vorher: Vec<(StoreyId, f64)>) {
        // OK Gelände je Gründung einmal für den ganzen Gang: Eine Gründung
        // zu setzen ändert kein Gelände (Review #123 W45 B1, sonst je
        // Gründung alle Umrisse neu)
        let jetzt: std::collections::HashMap<StoreyId, f64> =
            self.foundation_terrain().into_iter().collect();
        let bewegt: Vec<(StoreyId, f64, f64)> = vorher
            .into_iter()
            .filter_map(|(gr, alt)| {
                let t = *jetzt.get(&gr)?;
                ((t - alt).abs() > 1e-6).then_some((gr, alt, t))
            })
            .collect();
        if bewegt.is_empty() {
            return;
        }
        // Bereich je Gebäude in einem Gang: Eine Gründung zu setzen ändert
        // weder OK EG noch eine Platte (W45-Nachprüfung B1, sonst suchte
        // jede Gründung alle Geschosse und Bauteile ab)
        let basis = self.gruendungs_basis();
        for (gr, alt, t) in bewegt {
            let Some(s) = self.storey(gr) else {
                continue;
            };
            let want = s.embed.unwrap_or(alt - s.elevation);
            let bereich = basis.bereich(s.building, t);
            self.settle_foundation_in(gr, want, t, bereich);
        }
    }

    /// Legt UK Gründung `gr` auf die gewollte Einbindetiefe `want` (OK
    /// Gelände bis UK), geklemmt auf den erlaubten Bereich. Stößt sie an
    /// eine Grenze, merkt sich das Geschoss `want`; fällt die Grenze weg,
    /// gilt wieder `want`. `false`, wenn kein Bereich bleibt (die Gründung
    /// liegt dann an der unteren Grenze).
    fn settle_foundation(&mut self, gr: StoreyId, want: f64) -> bool {
        let t = self.terrain_z_of_level(gr);
        self.settle_foundation_at(gr, want, t)
    }

    /// Wie [`Model::settle_foundation`] mit schon bekanntem OK Gelände `t`.
    fn settle_foundation_at(&mut self, gr: StoreyId, want: f64, t: f64) -> bool {
        let bereich = self.foundation_bottom_range_at(gr, t);
        self.settle_foundation_in(gr, want, t, bereich)
    }

    /// Wie [`Model::settle_foundation_at`] mit schon bekanntem Bereich
    /// ([`Model::foundation_bottom_range_at`]).
    fn settle_foundation_in(
        &mut self,
        gr: StoreyId,
        want: f64,
        t: f64,
        (lo, hi): (f64, f64),
    ) -> bool {
        let z = (t - want).min(hi).max(lo);
        self.move_foundation_bottom(gr, z);
        let embed = ((t - z) - want).abs() > 1e-6;
        self.set_embed(gr, embed.then_some(want));
        hi >= lo - 1e-9
    }

    /// Die Tiefe unter der Sohlplatte (Plattendicke, Dämmung) hat sich
    /// geändert: Unter Platte und Dämmung bleibt die Schürze mindestens
    /// [`MIN_FOOTING`] tief (Jörn 11.10.). Ist sie dafür zu flach, wächst
    /// sie nach unten; wird es wieder flacher, nimmt sie dieses Wachstum
    /// zurück bis zur gewollten Einbindetiefe. Sonst bleibt UK Gründung.
    /// `false`, wenn keine Schürze mehr passt.
    pub(super) fn settle_below_slab(&mut self, slab: ElementId) -> bool {
        let gr = self
            .element(slab)
            .and_then(|e| self.foundation_level_of(e.storey));
        let Some((gr, s)) = gr.and_then(|g| Some((g, self.storey(g)?))) else {
            return true;
        };
        let want = s.embed.unwrap_or(self.terrain_z_of_level(gr) - s.elevation);
        self.settle_foundation(gr, want)
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
        self.touch_height();
    }

    /// Merkt das Projekt im offenen Schritt.
    pub(super) fn note_project(&mut self) {
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
    /// flach, sinkt UK Gründung um den Rest, und weniger Dämmung nimmt das
    /// wieder zurück ([`Model::settle_below_slab`]).
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
        let gr = self.foundation_bottom_of_slab(slab);
        self.edit_slab(slab, |s| s.insulation = t);
        if !self.settle_below_slab(slab) {
            self.edit_slab(slab, |s| s.insulation = old);
            self.restore_foundation_bottom(gr);
            return false;
        }
        self.sync_perimeter(slab);
        self.touch();
        true
    }

    /// UK und gewollte Einbindetiefe der Gründung unter einer Sohlplatte,
    /// zum Zurücklegen nach einer abgelehnten Änderung.
    pub(super) fn foundation_bottom_of_slab(
        &self,
        slab: ElementId,
    ) -> Option<(StoreyId, f64, Option<f64>)> {
        let gr = self
            .element(slab)
            .and_then(|e| self.foundation_level_of(e.storey))?;
        self.storey(gr).map(|s| (gr, s.elevation, s.embed))
    }

    /// Legt eine Gründung zurück (siehe [`Model::foundation_bottom_of_slab`]).
    pub(super) fn restore_foundation_bottom(&mut self, old: Option<(StoreyId, f64, Option<f64>)>) {
        if let Some((gr, z, embed)) = old {
            self.move_foundation_bottom(gr, z);
            self.set_embed(gr, embed);
        }
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
        // mit Aufmaß: Erdmengen über das Flächenmittel unter der Platte, die
        // Einbindetiefe ab dem tiefsten Gelände am Rand (§5 G1)
        let (terrain_z, terrain_nhn, tief) = match self.terrain_mean_survey(&f.outline) {
            Some((z, h0)) => {
                let gr = self
                    .element(slab)
                    .and_then(|e| self.foundation_level_of(e.storey));
                (z, Some(h0), gr.map_or(z, |g| self.terrain_z_of_level(g)))
            }
            None => {
                let z = self.terrain_z_of(building);
                (z, None, z)
            }
        };
        Some(GroundBasis {
            run,
            slab,
            footing,
            building,
            terrain_z,
            terrain_nhn,
            slab_top_z,
            slab_bottom_z,
            insulation,
            insulation_bottom_z: slab_bottom_z - insulation,
            footing_bottom_z,
            footing_width: sf.width,
            embedment: tief - footing_bottom_z,
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

/// Ein laufender Zug an OK Gelände ([`Model::gelaende_zug`]).
pub struct GelaendeZug {
    haus: BuildingId,
    /// OK EG und Plattentiefe je Gebäude; der Zug ändert beides nicht.
    basis: GruendungsBasis,
    /// OK Gelände je Gründung beim Beginn.
    vorher: Vec<(StoreyId, f64)>,
    /// Gründungen des gezogenen Gebäudes mit ihrer gewollten Einbindetiefe.
    eigene: Vec<(StoreyId, f64)>,
}
