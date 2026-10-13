//! Erdgeschossdecke (B10): Methoden von `Model`, aus `model.rs` ausgelagert.

use super::*;

impl Model {
    /// Decken über einem Wandzug (richtig: höchstens eine).
    pub(super) fn floors_of(&self, run: RunId) -> Vec<ElementId> {
        if let Some(v) = self.verz() {
            return v.decken(run);
        }
        self.elements
            .iter()
            .filter(|(_, e)| matches!(e.kind, ElementKind::Floor(f) if f.run == run))
            .map(|(id, _)| id)
            .collect()
    }

    /// Erdgeschossdecke über einem geschlossenen Außenwandzug.
    pub fn floor_of(&self, run: RunId) -> Option<ElementId> {
        self.floors_of(run).first().copied()
    }

    /// Jeder geschlossene Außenwandzug hat genau eine Erdgeschossdecke, ihre
    /// Oberkante wird beim Anlegen aus der Wandhöhe vorbelegt und dann
    /// gespeichert. Ist der Zug offen oder weg, verschwindet sie.
    pub(super) fn sync_floor(&mut self, run: RunId) {
        let floors = self.floors_of(run);
        if !self.needs_floor(run) {
            for f in floors {
                note!(self, Element, self.elements, f);
                self.elements.remove(f);
            }
            return;
        }
        if !floors.is_empty() {
            return;
        }
        let (Some(r), Some(material)) = (self.run(run), self.concrete()) else {
            return;
        };
        let storey = r.storey;
        let seq = self.storey_seq(storey).1;
        self.new_element(
            Category::Floor,
            storey,
            seq,
            ElementKind::Floor(Floor {
                run,
                material,
                thickness: FLOOR_THICKNESS,
                top: LevelRef::top(storey),
                soffit: Soffit::default(),
                terrace: Terrace::default(),
            }),
        );
    }

    /// Ergänzt in Dateien vor B10 die Erdgeschossdecke über jedem
    /// geschlossenen Außenwandzug. Ohne Rückgängig-Schritt; liefert Hinweise.
    pub(crate) fn complete_pre_b10(&mut self) -> Vec<String> {
        let missing: Vec<RunId> = self
            .runs
            .ids()
            .filter(|r| self.needs_floor(*r) && self.floors_of(*r).is_empty())
            .collect();
        for r in &missing {
            self.sync_floor(*r);
        }
        let eg = missing.iter().all(|r| self.needs_foundation(*r));
        match (missing.len(), eg) {
            (0, _) => Vec::new(),
            (1, true) => vec!["Erdgeschossdecke ergänzt".to_string()],
            (n, true) => vec![format!("Erdgeschossdecke über {n} Außenwandzügen ergänzt")],
            (1, false) => vec!["Geschossdecke ergänzt".to_string()],
            (n, false) => vec![format!("Geschossdecken über {n} Außenwandzügen ergänzt")],
        }
    }

    /// Stellt Dateien vor B12 auf Gebäude um: alle Geschosse gehören zu
    /// „Gebäude 1“, das OG wird 2,98 hoch (lichte Höhe 2,76), und über jedem
    /// geschlossenen EG-Außenwandzug entsteht der gekoppelte OG-Zug mit Decke.
    /// Ohne Rückgängig-Schritt; liefert den Hinweis.
    pub(crate) fn complete_pre_b12(&mut self) -> Vec<String> {
        if !self.buildings.is_empty() || self.levels_in(None).is_empty() {
            return Vec::new();
        }
        let guid = self.new_guid();
        let b = self.buildings.insert(Building {
            guid,
            name: "Gebäude 1".into(),
            number: "GB-01".into(),
            terrain: 0.0,
        });
        self.building_number = self.building_number.max(1);
        let levels = self.levels_in(None);
        let ground = self.ground_of(None);
        let mut z = None;
        for id in levels {
            let Some(st) = self.storeys.get_mut(id) else {
                continue;
            };
            st.building = Some(b);
            if let Some(z) = z {
                st.elevation = z;
                st.height = UPPER_HEIGHT;
            }
            if Some(id) == ground || z.is_some() {
                z = Some(st.top());
            }
        }
        let roots: Vec<RunId> = self
            .runs
            .ids()
            .filter(|r| self.needs_foundation(*r) && self.runs_above(*r).is_empty())
            .collect();
        for r in roots {
            let (mut below, mut at) = match self.run(r) {
                Some(x) => (r, x.storey),
                None => continue,
            };
            while let Some(up) = self.level_above(at) {
                let Some(n) = self.stack_run(below, up) else {
                    break;
                };
                (below, at) = (n, up);
            }
        }
        self.touch();
        vec!["Datei auf Gebäude umgestellt, Obergeschoss ergänzt".to_string()]
    }

    /// Geometrie der Decke über einem Wandzug; `None` ohne Decke, `Err` wenn
    /// kein Körper entstehen kann.
    pub fn floor(&self, run: RunId) -> Option<Result<FloorSlab, FloorError>> {
        self.floor_of_chain(run, &self.base_chain(run)?)
    }

    /// [`Model::floor`] zum schon gebauten Zug `chain` (ohne Anschlüsse).
    pub(super) fn floor_of_chain(
        &self,
        run: RunId,
        chain: &WallChain,
    ) -> Option<Result<FloorSlab, FloorError>> {
        let id = self.floor_of(run)?;
        let ElementKind::Floor(f) = self.element(id)?.kind else {
            return None;
        };
        let p = FloorParams {
            top: self.level_z(f.top)?,
            thickness: f.thickness,
            mat: material_key(f.material),
        };
        let ext = self
            .overhang_offsets(run)
            .and_then(|o| Some((chain.with_segment_offsets(&o)?, o)));
        let soffit = SoffitParams {
            // Bis 1 m über dem Wandfuß (wie die lichte Höhe, G4)
            thickness: f
                .soffit
                .thickness
                .clamp(MIN_SOFFIT, MAX_SOFFIT)
                .min((p.top - p.thickness - chain.base - MIN_CLEAR).max(0.0)),
            mat: f
                .soffit
                .material
                .or_else(|| self.soffit_material(run))
                .map_or(material::PLAIN, material_key),
            // Bekleidung (W3), sobald ihr Baustoff da ist
            cladding: match self.cladding_material_of(&f.soffit) {
                Some(_) => f.soffit.cladding,
                None => 0.0,
            },
            clad_mat: self
                .cladding_material_of(&f.soffit)
                .map_or(material::PLAIN, material_key),
            clad_inset: self.drip_layer(run).map_or(0.0, |i| {
                chain.layers[..=i].iter().map(|l| l.thickness).sum()
            }),
            drip: f.soffit.drip,
            batten: f.soffit.batten,
            counter: f.soffit.counter,
        };
        let over = ext.as_ref().map(|(c, o)| (c, &o[..], soffit));
        let strip = self.strip_params(run);
        let mut slab = FloorSlab::from_chain_over(chain, &p, strip, over);
        if let Ok(fs) = &mut slab {
            fs.core_layer = self.core_layer(id);
            if fs.strip.is_some_and(|sp| sp.covered) {
                fs.strip_covered = self.covered_segments(run);
            }
            // Dachterrasse über dem Rücksprung darüber (D1–D3): Deckenkante
            // ist der Umriss der Decke (Kern oder Randdämmstreifen innen)
            let up = self
                .runs_above(run)
                .first()
                .and_then(|u| self.base_chain(*u));
            if let Some(up) = up {
                let below = ext.as_ref().map_or(chain, |(c, _)| c);
                let depth = match strip {
                    Some(sp) if sp.width > 0.0 => sp.width,
                    _ => {
                        let core = below.layers.iter().position(|l| l.core).unwrap_or(0);
                        below.layers[..core].iter().map(|l| l.thickness).sum()
                    }
                };
                let map = self.runs_above(run).first().map(|u| self.stack_map(*u));
                let plan = crate::terrace::terrace_plan_mapped(
                    below,
                    &up,
                    map.as_deref().unwrap_or_default(),
                    depth,
                    strip.map(|s| s.mat),
                );
                if !plan.is_empty() {
                    fs.terrace = Some(TerraceParams {
                        layers: self.terrace_params_layers(id, &f.terrace),
                        attika_from: chain.top(),
                        upstand: f.terrace.upstand.clamp(0.0, MAX_UPSTAND),
                        coping_mat: self
                            .coping_material_of(&f.terrace)
                            .map_or(material::PLAIN, material_key),
                        crown: crown_of(&f.terrace),
                        crown_mat: self
                            .terrace_crown_material(id)
                            .map_or(material::PLAIN, material_key),
                    });
                    fs.terraces = plan;
                }
            }
        }
        Some(slab)
    }

    /// Versatz nach außen je Segment von `run` für die Wand darüber, wie sie
    /// steht (gekoppelt oder gelöst; Stufen dazwischen zählen nicht, W-12).
    /// `None` ohne Zug darüber oder wenn die Züge nicht zusammenpassen.
    fn offsets_above(&self, run: RunId) -> Option<Vec<f64>> {
        let up = *self.runs_above(run).first()?;
        let (upc, low) = (self.base_chain(up)?, self.base_chain(run)?);
        let lsegs = self.run(run)?.segments.clone();
        let partners: Vec<usize> = self
            .partners(up, &lsegs)
            .into_iter()
            .collect::<Option<_>>()?;
        upc.offsets_against(&low, partners.into_iter().enumerate().map(|(k, j)| (j, k)))
            .into_iter()
            .collect()
    }

    /// Dachterrassen auf der Decke über `run` (Review 2a R8, BIM Regel 41):
    /// je zusammenhängende Folge von Segmenten, über denen das Geschoss
    /// darüber um mindestens 20 mm lichte Tiefe zurückspringt, ein Umriss
    /// zwischen dessen Außenfläche und der Deckenkante. Leer ohne Zug
    /// darüber.
    pub fn terrace_outlines(&self, run: RunId) -> Vec<crate::terrace::TerraceOutline> {
        match self.floor(run) {
            Some(Ok(f)) => f.terraces.outlines,
            _ => Vec::new(),
        }
    }

    /// Vorsprung des Zuges darüber je Segment (≥ 0), wenn mindestens ein
    /// Segment vorspringt (G7 K4): Dort wachsen Decke, Untersichtdämmung und
    /// die Außenschichten mit.
    pub(super) fn overhang_offsets(&self, run: RunId) -> Option<Vec<f64>> {
        let o: Vec<f64> = self
            .offsets_above(run)?
            .into_iter()
            .map(|d| if d >= MIN_OFFSET { d } else { 0.0 })
            .collect();
        o.iter().any(|d| *d > 0.0).then_some(o)
    }

    /// Je Segment des Zuges: springt es gegenüber dem Zug darunter zurück
    /// (G7 K3)? Dort steht der Fuß neben dem Randdämmstreifen und zeichnet
    /// im Schnitt seine Linie. Bündig oder vorspringend steht er auf dem
    /// Streifen, der mit der Wand darüber wandert (K4).
    pub(super) fn open_segments(&self, run: RunId) -> Vec<bool> {
        let n = self.run(run).map_or(0, |r| r.segments.len());
        let Some(below) = self.run_below(run) else {
            return vec![true; n];
        };
        // Eine Stufe (W-12) steht ganz auf der Decke, neben dem Streifen
        match self.offsets_above(below) {
            Some(o) => self
                .stack_map(run)
                .into_iter()
                .map(|k| k.and_then(|k| o.get(k)).is_none_or(|d| *d <= -MIN_OFFSET))
                .collect(),
            _ => vec![true; n],
        }
    }

    /// Je Segment des Zuges: steht die Wand darüber bündig oder vorspringend
    /// auf dem Randdämmstreifen (G7 K3, K4)? Springt sie zurück, deckt sie ihn
    /// nicht ganz.
    pub(super) fn covered_segments(&self, run: RunId) -> Vec<bool> {
        let n = self.run(run).map_or(0, |r| r.segments.len());
        match self.offsets_above(run) {
            Some(o) if o.len() == n => o.iter().map(|d| *d > -MIN_OFFSET).collect(),
            _ => vec![false; n],
        }
    }

    /// Baustoff der Untersichtdämmung unter der Decke `floor`: die eigene
    /// Wahl oder [`Model::soffit_material`].
    pub fn soffit_material_of(&self, floor: ElementId) -> Option<MaterialId> {
        let ElementKind::Floor(f) = self.element(floor)?.kind else {
            return None;
        };
        f.soffit.material.or_else(|| self.soffit_material(f.run))
    }

    /// Baustoff der Untersichtdämmung ohne eigene Wahl (K4, BIM): die
    /// äußerste Dämmschicht der Wand darüber; einschalig der Baustoff des
    /// Randdämmstreifens; sonst der erste Dämmstoff im Projekt.
    fn soffit_material(&self, run: RunId) -> Option<MaterialId> {
        let set_of = |r: RunId| {
            self.run(r)?
                .segments
                .first()
                .and_then(|w| self.element(*w))
                .and_then(|e| e.layer_set)
                .and_then(|t| self.layer_set(t))
        };
        let insul = |m: MaterialId| {
            self.material(m)
                .is_some_and(|x| x.category == MatCategory::Insulation)
        };
        let above = self.runs_above(run).first().and_then(|u| set_of(*u));
        let outer = |t: &LayerSet| {
            t.layers
                .iter()
                .take_while(|l| !l.core)
                .map(|l| l.material)
                .find(|m| insul(*m))
        };
        above
            .and_then(outer)
            .or_else(|| set_of(run).and_then(outer))
            .or_else(|| above.and_then(|t| t.strip_material()))
            .or_else(|| set_of(run).and_then(|t| t.strip_material()))
            .or_else(|| {
                self.materials
                    .iter()
                    .find(|(_, m)| m.category == MatCategory::Insulation)
                    .map(|(id, _)| id)
            })
    }

    /// Setzt die Dicke der Untersichtdämmung einer Decke (mm,
    /// [`MIN_SOFFIT`] … [`MAX_SOFFIT`]).
    pub fn set_floor_soffit(&mut self, floor: ElementId, thickness: f64) -> bool {
        if !matches!(
            self.element(floor).map(|e| &e.kind),
            Some(ElementKind::Floor(_))
        ) || !(MIN_SOFFIT..=MAX_SOFFIT).contains(&thickness)
        {
            return false;
        }
        note!(self, Element, self.elements, floor);
        if let Some(ElementKind::Floor(f)) = self.elements.get_mut(floor).map(|e| &mut e.kind) {
            f.soffit.thickness = thickness;
        }
        self.touch();
        true
    }

    /// Setzt den Baustoff der Untersichtdämmung (`None`: wie die Wand darüber).
    pub fn set_floor_soffit_material(&mut self, floor: ElementId, m: Option<MaterialId>) -> bool {
        if !matches!(
            self.element(floor).map(|e| &e.kind),
            Some(ElementKind::Floor(_))
        ) || m.is_some_and(|m| self.material(m).is_none())
        {
            return false;
        }
        note!(self, Element, self.elements, floor);
        if let Some(ElementKind::Floor(f)) = self.elements.get_mut(floor).map(|e| &mut e.kind) {
            f.soffit.material = m;
        }
        self.touch();
        true
    }

    /// Setzt einen Wert der Bekleidung unter der Untersichtdämmung (W3):
    /// Dicke (0 oder [`MIN_CLADDING`] … [`MAX_CLADDING`]), Überstand des
    /// Verblenders ([`MIN_DRIP`] … [`MAX_DRIP`]) oder die Achsabstände der
    /// Grund- und Traglattung ([`MIN_BATTEN`] … [`MAX_BATTEN`], quer auch 0).
    pub fn set_floor_cladding(&mut self, floor: ElementId, value: CladdingValue, mm: f64) -> bool {
        let ok = match value {
            CladdingValue::Thickness => mm == 0.0 || (MIN_CLADDING..=MAX_CLADDING).contains(&mm),
            CladdingValue::Drip => (MIN_DRIP..=MAX_DRIP).contains(&mm),
            CladdingValue::Batten => (MIN_BATTEN..=MAX_BATTEN).contains(&mm),
            CladdingValue::Counter => mm == 0.0 || (MIN_BATTEN..=MAX_BATTEN).contains(&mm),
        };
        let Some(ElementKind::Floor(f)) = self.element(floor).map(|e| &e.kind) else {
            return false;
        };
        if !ok || value.of(&f.soffit) == mm {
            return false;
        }
        note!(self, Element, self.elements, floor);
        if let Some(ElementKind::Floor(f)) = self.elements.get_mut(floor).map(|e| &mut e.kind) {
            *value.of_mut(&mut f.soffit) = mm;
        }
        self.touch();
        true
    }

    /// Setzt den Baustoff der Bekleidung (`None`: der eingebaute).
    pub fn set_floor_cladding_material(&mut self, floor: ElementId, m: Option<MaterialId>) -> bool {
        if !matches!(
            self.element(floor).map(|e| &e.kind),
            Some(ElementKind::Floor(_))
        ) || m.is_some_and(|m| self.material(m).is_none())
        {
            return false;
        }
        note!(self, Element, self.elements, floor);
        if let Some(ElementKind::Floor(f)) = self.elements.get_mut(floor).map(|e| &mut e.kind) {
            f.soffit.cladding_material = m;
        }
        self.touch();
        true
    }

    /// Decken, unter denen eine Untersichtdämmung liegen muss (Regel 35):
    /// die Decke kragt unter einem Vorsprung darüber aus.
    pub fn soffit_floors(&self) -> Vec<ElementId> {
        self.soffit_floors_in(None)
    }

    pub(super) fn soffit_floors_in(&self, scope: Option<&[RunId]>) -> Vec<ElementId> {
        self.untersicht_und_terrasse_in(scope).0
    }

    /// Decken mit Untersicht und Decken mit Dachterrasse
    /// ([`Model::terrace_floors`]) unter den Zügen `scope`, je nach Nummer,
    /// mit einer Rechnung je Decke (Review zu #148).
    pub(super) fn untersicht_und_terrasse_in(
        &self,
        scope: Option<&[RunId]>,
    ) -> (Vec<ElementId>, Vec<ElementId>) {
        let (mut u, mut t) = (Vec::<(String, ElementId)>::new(), Vec::new());
        for (id, e) in self.elements.iter() {
            let ElementKind::Floor(f) = e.kind else {
                continue;
            };
            if !in_scope(scope, f.run) {
                continue;
            }
            if let Some(Ok(s)) = self.floor(f.run) {
                if !s.soffits.is_empty() {
                    u.push((e.number.clone(), id));
                }
                if !s.terraces.is_empty() {
                    t.push((e.number.clone(), id));
                }
            }
        }
        let ordnen = |mut v: Vec<(String, ElementId)>| -> Vec<ElementId> {
            v.sort_by(|a, b| a.0.cmp(&b.0));
            v.into_iter().map(|x| x.1).collect()
        };
        (ordnen(u), ordnen(t))
    }

    /// Untersichtdämmung unter einer Decke.
    pub fn soffit_of(&self, floor: ElementId) -> Option<ElementId> {
        self.elements
            .iter()
            .find(|(_, e)| e.kind == ElementKind::SoffitInsulation { floor })
            .map(|(id, _)| id)
    }

    /// Legt fehlende Untersichtdämmungen an und entfernt überzählige (im
    /// offenen Schritt oder nach dem Laden). Bleibt die Decke auskragend,
    /// bleibt die Dämmung mit Guid und Nummer; entsteht sie neu, bekommt sie
    /// eine neue (Regel 25).
    pub(super) fn sync_soffits(&mut self) {
        self.sync_soffits_in(None);
    }

    /// [`Model::sync_soffits`] nur an den Decken der Züge `scope` (Z4).
    pub(super) fn sync_soffits_in(&mut self, scope: Option<&[RunId]>) {
        self.sync_soffits_mit(scope, None);
    }

    /// [`Model::sync_soffits_in`] mit schon gesuchten Decken
    /// ([`Model::abgleich_wunsch`]).
    pub(super) fn sync_soffits_mit(
        &mut self,
        scope: Option<&[RunId]>,
        want: Option<Vec<ElementId>>,
    ) {
        let want =
            want.unwrap_or_else(|| self.abgleich_lesend(scope, || self.soffit_floors_in(scope)));
        // Bekleidung (W3): der eingebaute Baustoff bei der ersten
        if self.material_by_guid(CLADDING_MAT_GUID).is_none()
            && want.iter().any(|f| {
                matches!(self.element(*f).map(|e| &e.kind),
                    Some(ElementKind::Floor(x)) if x.soffit.cladding > 0.0
                        && x.soffit.cladding_material.is_none())
            })
        {
            self.ensure_cladding_material();
        }
        let have: Vec<(ElementId, ElementId)> = self
            .elements
            .iter()
            .filter_map(|(id, e)| match e.kind {
                ElementKind::SoffitInsulation { floor } if self.floor_in_scope(floor, scope) => {
                    Some((id, floor))
                }
                _ => None,
            })
            .collect();
        let mut kept = Vec::with_capacity(have.len());
        for (id, floor) in have {
            if want.contains(&floor) && !kept.contains(&floor) {
                kept.push(floor);
            } else {
                note!(self, Element, self.elements, id);
                self.elements.remove(id);
                self.touch();
            }
        }
        for floor in want {
            if kept.contains(&floor) {
                continue;
            }
            let Some((storey, seq)) = self.element(floor).map(|e| (e.storey, e.seq)) else {
                continue;
            };
            self.new_element(
                Category::SoffitInsulation,
                storey,
                seq,
                ElementKind::SoffitInsulation { floor },
            );
            self.touch();
        }
    }

    /// Nach dem Laden: Untersichtdämmungen passend zu den Vorsprüngen;
    /// Dateien vor W3 bekommen die Bekleidung darunter (mit Hinweis).
    /// Decken, die erst beim Laden einer alten Datei entstehen (vor B10 und
    /// B12), tragen keine Bekleidung der Untersicht: die Datei kannte sie
    /// nicht und bleibt beim Speichern bytegleich (Test F-a, wie
    /// `soffit_clad` fehlt = 0 beim Lesen). Ohne Rückgängig-Schritt.
    pub(crate) fn clear_cladding_except(&mut self, before: &[ElementId]) {
        for id in self.floor_ids() {
            if before.contains(&id) {
                continue;
            }
            if let Some(ElementKind::Floor(f)) = self.elements.get_mut(id).map(|e| &mut e.kind) {
                f.soffit.cladding = 0.0;
            }
        }
    }

    /// Alle Decken des Modells.
    pub(crate) fn floor_ids(&self) -> Vec<ElementId> {
        self.elements
            .iter()
            .filter(|(_, e)| matches!(e.kind, ElementKind::Floor(_)))
            .map(|(id, _)| id)
            .collect()
    }

    pub(crate) fn complete_soffits(&mut self) -> Vec<String> {
        let strict = std::mem::replace(&mut self.strict, false);
        let had = self.material_by_guid(CLADDING_MAT_GUID).is_some();
        self.sync_soffits();
        self.strict = strict;
        if !had && self.material_by_guid(CLADDING_MAT_GUID).is_some() {
            vec!["Bekleidung unter der Untersichtdämmung ergänzt".to_string()]
        } else {
            Vec::new()
        }
    }
}
