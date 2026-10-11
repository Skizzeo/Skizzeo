//! Gründung (B9): Methoden von `Model`, aus `model.rs` ausgelagert.

use super::*;

impl Model {
    /// Braucht der Zug eine Decke? Ein lebender, geschlossener Außenwandzug.
    pub(super) fn needs_floor(&self, run: RunId) -> bool {
        self.run(run).is_some_and(|r| {
            r.closed
                && r.segments
                    .first()
                    .and_then(|e| self.element(*e))
                    .is_some_and(|e| e.category == Category::ExteriorWall)
        })
    }

    /// Braucht der Zug eine Gründung? Ein geschlossener Außenwandzug im EG
    /// seines Gebäudes (B12: Gründung nur unter dem EG).
    pub(super) fn needs_foundation(&self, run: RunId) -> bool {
        self.needs_floor(run)
            && self
                .run(run)
                .is_some_and(|r| self.ground_storey(r.storey) == Some(r.storey))
    }

    /// Sohlplatten unter einem Wandzug (richtig: höchstens eine).
    pub(super) fn slabs_of(&self, run: RunId) -> Vec<ElementId> {
        self.elements
            .iter()
            .filter(|(_, e)| matches!(e.kind, ElementKind::GroundSlab(s) if s.run == run))
            .map(|(id, _)| id)
            .collect()
    }

    /// Frostschürzen unter einer Sohlplatte (richtig: genau eine).
    pub(super) fn footings_of(&self, slab: ElementId) -> Vec<ElementId> {
        self.elements
            .iter()
            .filter(|(_, e)| matches!(e.kind, ElementKind::StripFooting(f) if f.slab == slab))
            .map(|(id, _)| id)
            .collect()
    }

    /// Sohlplatte und Frostschürze unter einem Wandzug.
    pub fn foundation_of(&self, run: RunId) -> Option<(ElementId, Option<ElementId>)> {
        let slab = *self.slabs_of(run).first()?;
        Some((slab, self.footings_of(slab).first().copied()))
    }

    /// Wandzug, zu dem ein Bauteil gehört: Wand, Sohlplatte darunter oder
    /// Frostschürze unter deren Platte.
    pub fn run_of(&self, e: ElementId) -> Option<RunId> {
        match self.element(e)?.kind {
            ElementKind::Wall(w) => Some(w.run),
            ElementKind::GroundSlab(s) => Some(s.run),
            ElementKind::StripFooting(f) => self.run_of(f.slab),
            ElementKind::Floor(f) => Some(f.run),
            ElementKind::EdgeStrip { wall, .. } => self.run_of(wall),
            ElementKind::SoffitInsulation { floor }
            | ElementKind::RoofTerrace { floor }
            | ElementKind::Coping { floor }
            | ElementKind::Roof { floor, .. } => self.run_of(floor),
            ElementKind::PerimeterInsulation { slab } => self.run_of(slab),
            ElementKind::Ext(_) => None,
        }
    }

    /// Paare (Wand, Decke), auf denen ein Randdämmstreifen liegen muss (K5):
    /// jede Wand eines Zuges unter seiner Decke, deren Typ das Auflager
    /// `Depth` hat. In Nummernreihenfolge: Gebäude, Geschoss, Segment.
    pub fn edge_strip_pairs(&self) -> Vec<(ElementId, ElementId)> {
        self.edge_strip_pairs_in(None)
    }

    /// [`Model::edge_strip_pairs`] nur an den Decken der Züge `scope`
    /// (`None`: alle).
    fn edge_strip_pairs_in(&self, scope: Option<&[RunId]>) -> Vec<(ElementId, ElementId)> {
        let mut floors: Vec<(String, f64, ElementId, RunId)> = self
            .elements
            .iter()
            .filter_map(|(id, e)| match e.kind {
                ElementKind::Floor(f) if in_scope(scope, f.run) => {
                    let r = self.run(f.run)?;
                    let st = self.storey(r.storey)?;
                    let b = st
                        .building
                        .and_then(|b| self.building(b))
                        .map_or(String::new(), |b| b.number.clone());
                    Some((b, st.elevation, id, f.run))
                }
                _ => None,
            })
            .collect();
        floors.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.total_cmp(&b.1)));
        let mut out = Vec::new();
        for (_, _, floor, run) in floors {
            let Some(r) = self.run(run) else { continue };
            for &wall in &r.segments {
                let depth = self
                    .element(wall)
                    .and_then(|e| e.layer_set)
                    .and_then(|t| self.layer_set(t))
                    .is_some_and(|t| {
                        matches!(t.bearing, Bearing::Depth { .. })
                            && self.bearing_problem(t).is_none()
                    });
                if depth {
                    out.push((wall, floor));
                }
            }
        }
        out
    }

    /// Randdämmstreifen eines Paares (Wand, Decke).
    pub fn edge_strip_of(&self, wall: ElementId, floor: ElementId) -> Option<ElementId> {
        self.elements
            .iter()
            .find(|(_, e)| e.kind == ElementKind::EdgeStrip { wall, floor })
            .map(|(id, _)| id)
    }

    /// Nach dem Laden: Randdämmstreifen passend zu den Typen. Ein ungültiges
    /// Auflager (Regel 21) bleibt im Typ stehen, seine Streifen fallen weg,
    /// damit Decke und Streifen nicht doppelt zählen.
    /// Andere Abweichungen (Regel 22) bleiben stehen und meldet `check()`.
    pub(crate) fn complete_edge_strips(&mut self) {
        let invalid: Vec<ElementId> = self
            .elements
            .iter()
            .filter(|(_, e)| match e.kind {
                ElementKind::EdgeStrip { wall, .. } => self
                    .element(wall)
                    .and_then(|w| w.layer_set)
                    .and_then(|t| self.layer_set(t))
                    .is_some_and(|t| self.bearing_problem(t).is_some()),
                _ => false,
            })
            .map(|(id, _)| id)
            .collect();
        for id in invalid {
            self.elements.remove(id);
            self.touch();
        }
    }

    /// Legt fehlende Randdämmstreifen an und entfernt überzählige, im offenen
    /// Schritt. Bleibt das Paar (Wand, Decke), bleibt der Streifen mit Guid
    /// und Nummer.
    /// Mit `scope` nur an den Decken dieser Züge (Z4, die übrigen stehen
    /// unverändert).
    pub(super) fn sync_edge_strips(&mut self, scope: Option<&[RunId]>) {
        let want = self.edge_strip_pairs_in(scope);
        let have: Vec<(ElementId, (ElementId, ElementId))> = self
            .elements
            .iter()
            .filter_map(|(id, e)| match e.kind {
                ElementKind::EdgeStrip { wall, floor }
                    if self.floor_in_scope(floor, scope) || self.element(wall).is_none() =>
                {
                    Some((id, (wall, floor)))
                }
                _ => None,
            })
            .collect();
        let mut kept = Vec::with_capacity(have.len());
        for (id, pair) in have {
            if want.contains(&pair) && !kept.contains(&pair) {
                kept.push(pair);
            } else {
                note!(self, Element, self.elements, id);
                self.elements.remove(id);
                self.touch();
            }
        }
        for (wall, floor) in want {
            if kept.contains(&(wall, floor)) {
                continue;
            }
            let (Some(storey), Some(seq)) = (
                self.element(wall).map(|e| e.storey),
                self.element(floor).map(|e| e.seq),
            ) else {
                continue;
            };
            self.new_element(
                Category::EdgeInsulation,
                storey,
                seq,
                ElementKind::EdgeStrip { wall, floor },
            );
            self.touch();
        }
    }

    /// Bauteil hinter einem Teil des Körpers eines Wandzugs (Treffer beim
    /// Klicken): Segmentnummer, [`SLAB_PART`] oder [`FOOTING_PART`].
    pub fn part_of(&self, run: RunId, part: u32) -> Option<ElementId> {
        match part {
            SLAB_PART => self.foundation_of(run).map(|f| f.0),
            FOOTING_PART => self.foundation_of(run).and_then(|f| f.1),
            PERIMETER_PART => self.perimeter_of(self.foundation_of(run)?.0),
            FLOOR_PART => self.floor_of(run),
            SOFFIT_PART => self.soffit_of(self.floor_of(run)?),
            TERRACE_PART => self.terrace_of(self.floor_of(run)?),
            COPING_PART => self.coping_of(self.floor_of(run)?),
            ROOF_PART => self.flat_roof_of(self.floor_of(self.run_below(run)?)?),
            ROOF_COPING_PART => self.coping_of(self.floor_of(self.run_below(run)?)?),
            p if (STRIP_PART..STRIP_PART + MAX_STRIPS).contains(&p) => {
                let wall = self.wall_at(run, (p - STRIP_PART) as usize)?;
                self.edge_strip_of(wall, self.floor_of(run)?)
            }
            seg => self.wall_at(run, seg as usize),
        }
    }

    pub(super) fn new_element(
        &mut self,
        category: Category,
        storey: StoreyId,
        seq: u16,
        kind: ElementKind,
    ) -> ElementId {
        let guid = self.new_guid();
        let number = self.next_number(category);
        let id = self.elements.insert(Element {
            guid,
            number,
            category,
            storey,
            layer_set: None,
            seq,
            kind,
            props: PropSet::new(),
            locked: false,
        });
        note!(self, Element, new id);
        id
    }

    /// Baustoff neuer Gründungen: Stahlbeton (erster Beton der Bibliothek).
    pub(super) fn concrete(&self) -> Option<MaterialId> {
        self.materials
            .iter()
            .find(|(_, m)| m.category == MatCategory::Concrete)
            .or_else(|| self.materials.iter().next())
            .map(|(id, _)| id)
    }

    /// Jeder geschlossene Außenwandzug hat genau eine Sohlplatte mit einer
    /// Frostschürze; ist der Zug offen oder weg, verschwinden beide. Im selben
    /// Schritt wie die Änderung am Zug.
    fn sync_foundation(&mut self, run: RunId) {
        let slabs = self.slabs_of(run);
        if !self.needs_foundation(run) {
            for slab in slabs {
                for f in self
                    .footings_of(slab)
                    .into_iter()
                    .chain(self.perimeters_of(slab))
                {
                    note!(self, Element, self.elements, f);
                    self.elements.remove(f);
                }
                note!(self, Element, self.elements, slab);
                self.elements.remove(slab);
            }
            return;
        }
        let (Some(storey), Some(material)) = (self.run(run).map(|r| r.storey), self.concrete())
        else {
            return;
        };
        let slab = match slabs.first() {
            Some(s) => *s,
            None => self.new_element(
                Category::GroundSlab,
                storey,
                SLAB_SEQ,
                ElementKind::GroundSlab(GroundSlab {
                    run,
                    material,
                    top: LevelRef::bottom(storey),
                    thickness: SLAB_THICKNESS,
                    recess: 0.0,
                    insulation: 0.0,
                }),
            ),
        };
        self.sync_perimeter(slab);
        if self.footings_of(slab).is_empty() {
            let Some(gr) = self.foundation_level_of(storey) else {
                return;
            };
            self.new_element(
                Category::StripFooting,
                storey,
                FOOTING_SEQ,
                ElementKind::StripFooting(StripFooting {
                    slab,
                    material,
                    width: 350.0,
                    base: LevelRef::bottom(gr),
                }),
            );
        }
    }

    /// Gründung und Erdgeschossdecke eines Zuges abgleichen.
    pub(super) fn sync_parts(&mut self, run: RunId) {
        self.sync_foundation(run);
        self.sync_floor(run);
    }

    /// Ergänzt in Dateien vor B9 die Stahlbeton-Schraffur (E15) und die
    /// Gründung unter jedem geschlossenen Außenwandzug. Ohne Rückgängig-Schritt,
    /// gleich nach dem Lesen; liefert Hinweise auf das Ergänzte.
    pub(crate) fn complete_pre_b9(&mut self) -> Vec<String> {
        let mut hints = Vec::new();
        if !self
            .attr
            .fills()
            .iter()
            .any(|(_, f)| f.name == "Stahlbeton")
        {
            let guid = self.new_guid();
            let cross = self.attr.add_fill(crate::attr::Fill {
                guid,
                name: "Stahlbeton".into(),
                kind: crate::attr::FillKind::Lines(crate::attr::concrete_lines()),
                space: crate::attr::FillSpace::Paper,
            });
            let ids: Vec<MaterialId> = self.materials.ids().collect();
            for id in ids {
                if let Some(m) = self
                    .materials
                    .get_mut(id)
                    .filter(|m| m.name == "Stahlbeton")
                {
                    m.cut_fill = cross;
                }
            }
            hints.push("Schraffur „Stahlbeton“ ergänzt".to_string());
        }
        let missing: Vec<RunId> = self
            .runs
            .ids()
            .filter(|r| self.needs_foundation(*r) && self.slabs_of(*r).is_empty())
            .collect();
        for r in &missing {
            self.sync_foundation(*r);
        }
        if !missing.is_empty() {
            hints.push(match missing.len() {
                1 => "Gründung unter dem geschlossenen Außenwandzug ergänzt".to_string(),
                n => format!("Gründung unter {n} geschlossenen Außenwandzügen ergänzt"),
            });
        }
        hints
    }

    /// Geometrie der Gründung unter einem Wandzug; `None` ohne Sohlplatte,
    /// `Err` wenn kein Körper entstehen kann.
    pub fn foundation(&self, run: RunId) -> Option<Result<Foundation, FoundationError>> {
        let (slab, footing) = self.foundation_of(run)?;
        let footing = footing?;
        let ElementKind::GroundSlab(s) = self.element(slab)?.kind else {
            return None;
        };
        let ElementKind::StripFooting(f) = self.element(footing)?.kind else {
            return None;
        };
        if s.recess > 0.0 && s.recess < MIN_RECESS {
            return Some(Err(FoundationError::RecessTooSmall));
        }
        let chain = self.base_chain(run)?;
        let slab_bottom = self.level_z(s.top)? - s.thickness;
        let insulation = s.insulation.max(0.0);
        let p = FoundationParams {
            recess: s.recess,
            slab_thickness: s.thickness,
            footing_width: f.width,
            footing_depth: slab_bottom - insulation - self.level_z(f.base)?,
            slab_mat: material_key(s.material),
            footing_mat: material_key(f.material),
            insulation,
            insulation_mat: self
                .perimeter_material()
                .map_or(material::PLAIN, material_key),
        };
        Some(Foundation::from_chain(&chain, &p).map(|mut x| {
            x.layers = (self.core_layer(slab), self.core_layer(footing));
            x
        }))
    }

    /// Schicht des Kerns im Aufbau eines Bauteils ([`Model::element_layers`]),
    /// für die Darstellung (Paket 3); ohne Kern die erste.
    pub(super) fn core_layer(&self, id: ElementId) -> u8 {
        self.element_layers(id)
            .iter()
            .position(|l| l.core)
            .unwrap_or(0)
            .min(u8::MAX as usize - 1) as u8
    }

    /// Setzt den Sockelrücksprung einer Sohlplatte (mm). Erlaubt sind 0
    /// (bündig) und Werte ab 20 mm; 1 bis 19 mm werden abgelehnt.
    pub fn set_slab_recess(&mut self, slab: ElementId, recess: f64) -> bool {
        if !(recess == 0.0 || recess >= MIN_RECESS) || !recess.is_finite() {
            return false;
        }
        self.edit_slab(slab, |s| s.recess = recess)
    }

    /// Setzt die Dicke einer Sohlplatte (mm, von oben nach unten).
    /// Grenze: Schürze bleibt mindestens 10 cm tief (UK Gründung steht),
    /// unter einer Perimeterdämmung ab deren Unterkante.
    pub fn set_slab_thickness(&mut self, slab: ElementId, thickness: f64) -> bool {
        let bottom = self
            .element(slab)
            .and_then(|e| self.foundation_level_of(e.storey))
            .and_then(|g| self.storey(g))
            .map_or(f64::MIN, |g| g.elevation);
        let insulation = self.slab_insulation(slab);
        thickness > 0.0
            && thickness.is_finite()
            && thickness + insulation <= -bottom - MIN_FOOTING
            && self.edit_slab(slab, |s| s.thickness = thickness)
    }

    pub(super) fn edit_slab(&mut self, slab: ElementId, f: impl FnOnce(&mut GroundSlab)) -> bool {
        if !matches!(
            self.element(slab).map(|e| &e.kind),
            Some(ElementKind::GroundSlab(_))
        ) {
            return false;
        }
        note!(self, Element, self.elements, slab);
        if let Some(ElementKind::GroundSlab(s)) = self.elements.get_mut(slab).map(|e| &mut e.kind) {
            f(s);
        }
        self.touch();
        true
    }

    /// Setzt die Breite einer Frostschürze (mm), 30 bis 45 cm (Jörn 17:34:
    /// die Schürze schützt vor Frost, tragend ist die Platte). Alte Dateien
    /// behalten einen Wert außerhalb, bis er geändert wird.
    pub fn set_footing_width(&mut self, footing: ElementId, width: f64) -> bool {
        let (lo, hi) = FOOTING_WIDTH;
        if !(width >= lo - 1e-9 && width <= hi + 1e-9)
            || !matches!(
                self.element(footing).map(|e| &e.kind),
                Some(ElementKind::StripFooting(_))
            )
        {
            return false;
        }
        note!(self, Element, self.elements, footing);
        if let Some(ElementKind::StripFooting(f)) =
            self.elements.get_mut(footing).map(|e| &mut e.kind)
        {
            f.width = width;
        }
        self.touch();
        true
    }

    /// Tiefe einer Frostschürze ab UK Sohlplatte (mm), abgeleitet aus UK
    /// Gründung. Eine Perimeterdämmung liegt innerhalb der Schürze und
    /// ändert die Tiefe nicht (Jörn 11.10.).
    pub fn footing_depth(&self, footing: ElementId) -> Option<f64> {
        let ElementKind::StripFooting(f) = self.element(footing)?.kind else {
            return None;
        };
        let ElementKind::GroundSlab(s) = self.element(f.slab)?.kind else {
            return None;
        };
        Some(self.level_z(s.top)? - s.thickness - self.level_z(f.base)?)
    }

    /// Schürzentiefe als Zahl: verschiebt UK Gründung (B11), für alle
    /// Schürzen. Ein Wert außerhalb der Grenzen wird abgelehnt.
    pub fn set_footing_depth(&mut self, footing: ElementId, depth: f64) -> bool {
        let (Some(d), true) = (self.footing_depth(footing), depth.is_finite()) else {
            return false;
        };
        let Some(ElementKind::StripFooting(f)) = self.element(footing).map(|e| e.kind.clone())
        else {
            return false;
        };
        let gr = f.base.storey;
        let Some(z) = self.storey(gr).map(|g| g.elevation) else {
            return false;
        };
        self.set_foundation_bottom_of(gr, z - (depth - d))
    }
}
