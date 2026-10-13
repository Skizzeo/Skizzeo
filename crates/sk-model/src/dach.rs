//! Dachterrasse, Attika, Attikablech (D1–D3): Methoden von `Model`, aus `model.rs` ausgelagert.

use super::*;

impl Model {
    /// Typ der Dachterrasse einer Decke: der gewählte oder der Werkstyp
    /// „Dachterrasse 14“, sobald es ihn im Projekt gibt.
    pub fn terrace_type_of(&self, floor: ElementId) -> Option<LayerSetId> {
        let ElementKind::Floor(f) = self.element(floor)?.kind else {
            return None;
        };
        self.terrace_type(&f.terrace)
    }

    fn terrace_type(&self, t: &Terrace) -> Option<LayerSetId> {
        t.build_up
            .filter(|id| {
                self.layer_set(*id)
                    .is_some_and(|x| x.category == TypeCategory::RoofTerrace)
            })
            .or_else(|| self.type_by_guid(TERRACE_TYPE_GUID))
    }

    /// Schichten der Dachterrasse auf der Decke `floor`, von oben nach unten
    /// (die einzige Leseschnittstelle, BIM §3): die des Typs. Leer, solange
    /// es den Werkstyp noch nicht gibt (vor der ersten Terrasse).
    pub fn terrace_layers(&self, floor: ElementId) -> Vec<MaterialLayer> {
        self.terrace_type_of(floor)
            .and_then(|t| self.layer_set(t))
            .map_or_else(Vec::new, |t| t.layers.clone())
    }

    /// Schichten für den Körper; ohne Typ der Werkstyp ohne Baustoffe (nur
    /// im offenen Schritt, bis [`Model::sync_terraces`] ihn anlegt).
    pub(super) fn terrace_params_layers(
        &self,
        floor: ElementId,
        t: &Terrace,
    ) -> Vec<(f64, u16, bool)> {
        let ins = |m: MaterialId| {
            self.material(m)
                .is_some_and(|x| x.category == MatCategory::Insulation)
        };
        match self.terrace_type(t).and_then(|t| self.layer_set(t)) {
            Some(set) => set
                .layers
                .iter()
                .filter(|l| l.function != LayerFunction::AirGap)
                .map(|l| (l.thickness, material_key(l.material), ins(l.material)))
                .collect(),
            None => {
                let _ = floor;
                TERRACE_BUILD_UP
                    .iter()
                    .map(|&(d, f)| (d, material::PLAIN, f == LayerFunction::Insulation))
                    .collect()
            }
        }
    }

    /// Baustoff des Attikablechs: die Wahl an der Decke oder Titanzink 0,7.
    pub(super) fn coping_material_of(&self, t: &Terrace) -> Option<MaterialId> {
        t.coping_mat
            .filter(|m| self.materials.contains(*m))
            .or_else(|| self.material_by_guid(COPING_MAT_GUID))
    }

    /// Baustoff des Attikablechs einer Decke.
    pub fn coping_material(&self, floor: ElementId) -> Option<MaterialId> {
        let ElementKind::Floor(f) = self.element(floor)?.kind else {
            return None;
        };
        self.coping_material_of(&f.terrace)
    }

    pub(crate) fn material_by_guid(&self, g: Guid) -> Option<MaterialId> {
        self.materials
            .iter()
            .find(|(_, m)| m.guid == g)
            .map(|(id, _)| id)
    }

    /// Eingebauter Baustoff mit fester Guid; fehlt er, wird er angelegt
    /// (rückgängig machbar). Darstellung wie ein Baustoff derselben Art
    /// (Dämmung: Zickzack), sonst ohne Schraffur.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn builtin_material(
        &mut self,
        guid: Guid,
        name: &str,
        category: MatCategory,
        priority: u16,
        density: f64,
        lambda: Option<f64>,
        color: [u8; 3],
        cut_color: [u8; 3],
        trade: Option<TradeId>,
    ) -> Option<MaterialId> {
        self.builtin_material_with(
            guid, name, category, priority, density, lambda, color, cut_color, trade, None, None,
        )
    }

    /// Wie [`Model::builtin_material`]; mit `fill` schneidet der Baustoff
    /// mit dieser Schraffur statt der seiner Art, mit `surface` trägt seine
    /// Oberfläche diese feste Guid.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn builtin_material_with(
        &mut self,
        guid: Guid,
        name: &str,
        category: MatCategory,
        priority: u16,
        density: f64,
        lambda: Option<f64>,
        color: [u8; 3],
        cut_color: [u8; 3],
        trade: Option<TradeId>,
        fill: Option<FillId>,
        surface: Option<Guid>,
    ) -> Option<MaterialId> {
        if let Some(id) = self.material_by_guid(guid) {
            return Some(id);
        }
        let like = self
            .materials
            .iter()
            .find(|(_, m)| m.category == category)
            .or_else(|| self.materials.iter().next())
            .map(|(_, m)| m.clone());
        let empty = self
            .attr
            .fills()
            .iter()
            .find(|(_, f)| f.kind == crate::attr::FillKind::Empty)
            .map(|(id, _)| id);
        let cut_fill = match (fill, &like, category) {
            (Some(f), _, _) => f,
            (None, Some(m), MatCategory::Insulation) => m.cut_fill,
            (None, Some(m), _) => empty.unwrap_or(m.cut_fill),
            (None, None, _) => empty.or_else(|| self.attr.fills().ids().next())?,
        };
        let (cut_fg, cut_bg) = match &like {
            Some(m) => (m.cut_fg, m.cut_bg),
            None => {
                let p = self.attr.pens().ids().next()?;
                (p, p)
            }
        };
        let sg = surface.unwrap_or_else(|| self.new_guid());
        let surface = self.add_surface(Surface {
            guid: sg,
            name: name.into(),
            color,
            cut_color,
            // Werksmuster an fester Guid (Ausbau des Raumbuchs, Regel 60)
            pattern: crate::proctex::factory_for(sg),
        });
        let d = MaterialDisplay {
            cut_fill,
            cut_fg,
            cut_bg,
            surface,
        };
        Some(
            self.add_material(
                Material::new(guid, name, category, priority, density, d)
                    .lambda(lambda)
                    .trade(trade),
            ),
        )
    }

    /// Werkstyp „Dachterrasse 14“ (DT-14): Belag 6 über Dämmung hart 8, ohne
    /// Kern; fehlt er, wird er samt Baustoffen angelegt.
    fn ensure_terrace_type(&mut self) -> Option<LayerSetId> {
        if let Some(id) = self.type_by_guid(TERRACE_TYPE_GUID) {
            return Some(id);
        }
        let finish = self.builtin_material(
            TERRACE_FINISH_GUID,
            "Terrassenbelag",
            MatCategory::Concrete,
            500,
            2200.0,
            Some(1.65),
            [196, 190, 178],
            [172, 166, 154],
            trade::roofing(),
        )?;
        let insulation = self.builtin_material(
            TERRACE_INSULATION_GUID,
            "Dämmung hart (Terrasse)",
            MatCategory::Insulation,
            300,
            30.0,
            Some(0.035),
            [214, 226, 236],
            [180, 200, 222],
            trade::roofing(),
        )?;
        let [(d0, f0), (d1, f1)] = TERRACE_BUILD_UP;
        let code = self.free_code("DT-14");
        self.add_layer_set(LayerSet {
            guid: TERRACE_TYPE_GUID,
            name: "Dachterrasse 14".into(),
            code,
            category: TypeCategory::RoofTerrace,
            layers: vec![
                MaterialLayer::new(finish, d0, f0),
                // Dämmung der Terrasse legt immer der Dachdecker (BIM §2)
                MaterialLayer::new(insulation, d1, f1).trade(trade::roofing()),
            ],
            props: PropSet::new(),
            note: String::new(),
            changed: 1,
            bearing: Bearing::Core,
            height: None,
        })
    }

    /// Werksschraffur „Dämmung hart (Kreuz)“ (Jörn 10.10. 05:53): Kreuz
    /// 45°/135° im Abstand der Mauerwerkschraffur; fehlt sie, wird sie
    /// angelegt (rückgängig machbar).
    fn ensure_cross_fill(&mut self) -> FillId {
        if let Some((id, _)) = self
            .attr
            .fills()
            .iter()
            .find(|(_, f)| f.guid == CROSS_FILL_GUID)
        {
            return id;
        }
        self.add_fill(Fill {
            guid: CROSS_FILL_GUID,
            name: crate::attr::CROSS_FILL_NAME.into(),
            kind: crate::attr::FillKind::Lines(crate::attr::cross_lines()),
            space: crate::attr::FillSpace::Paper,
        })
    }

    /// Schaumglas-Dämmstein für den Fußpunkt der Verblendschale (W1, z. B.
    /// FOAMGLAS PERINSUL HL, λ 0,058, Z-17.5-1209): Kreuzschraffur, grau,
    /// Maurer. Fehlt er, wird er samt Schraffur angelegt.
    pub(crate) fn ensure_foamglass_material(&mut self) -> Option<MaterialId> {
        if let Some(id) = self.material_by_guid(FOAMGLASS_MAT_GUID) {
            return Some(id);
        }
        let cross = self.ensure_cross_fill();
        self.builtin_material_with(
            FOAMGLASS_MAT_GUID,
            "Schaumglas-Dämmstein",
            MatCategory::Insulation,
            300,
            165.0,
            Some(0.058),
            [128, 132, 134],
            [176, 180, 182],
            trade::start_id("18330"),
            Some(cross),
            Some(FOAMGLASS_SURFACE_GUID),
        )
    }

    /// Baustoff des Fußpunkts unter der Verblendschale, sobald es ihn im
    /// Projekt gibt (W1).
    pub fn foamglass_material(&self) -> Option<MaterialId> {
        self.material_by_guid(FOAMGLASS_MAT_GUID)
    }

    /// „Bekleidung Faserzement“ unter der Untersichtdämmung (W3, Tagesplan
    /// P5): Faserzementplatte 8 mm auf Traglattung, symbolisch eine Schicht;
    /// Schnitt ohne Muster, Gewerk VHF. Fehlt er, wird er angelegt.
    pub(super) fn ensure_cladding_material(&mut self) -> Option<MaterialId> {
        self.builtin_material_with(
            CLADDING_MAT_GUID,
            "Bekleidung Faserzement",
            MatCategory::Concrete,
            500,
            1800.0,
            None,
            [168, 170, 166],
            [196, 198, 194],
            trade::start_id("18351"),
            None,
            Some(CLADDING_SURFACE_GUID),
        )
    }

    /// Baustoff der Bekleidung: die Wahl an der Decke oder der eingebaute,
    /// sobald es ihn im Projekt gibt.
    pub(super) fn cladding_material_of(&self, s: &Soffit) -> Option<MaterialId> {
        s.cladding_material
            .filter(|m| self.materials.contains(*m))
            .or_else(|| self.material_by_guid(CLADDING_MAT_GUID))
    }

    /// Baustoff der Bekleidung unter der Untersichtdämmung der Decke
    /// `floor` (W3); `None` ohne Bekleidung.
    pub fn soffit_cladding_material_of(&self, floor: ElementId) -> Option<MaterialId> {
        let ElementKind::Floor(f) = self.element(floor)?.kind else {
            return None;
        };
        (f.soffit.cladding > 0.0)
            .then(|| self.cladding_material_of(&f.soffit))
            .flatten()
    }

    /// Titanzink 0,7 für das Attikablech; fehlt er, wird er angelegt.
    pub(super) fn ensure_coping_material(&mut self) -> Option<MaterialId> {
        self.builtin_material(
            COPING_MAT_GUID,
            "Titanzink 0,7",
            MatCategory::Metal,
            900,
            7200.0,
            Some(110.0),
            [150, 158, 164],
            [110, 118, 124],
            trade::for_category(MatCategory::Metal),
        )
    }

    /// Decken mit Dachterrasse (Regel 41): über ihr springt das Geschoss um
    /// mindestens 20 mm lichte Tiefe zurück. Nach Nummer.
    pub fn terrace_floors(&self) -> Vec<ElementId> {
        self.terrace_floors_in(None)
    }

    /// [`Model::terrace_floors`] nur unter den Zügen `scope` (T6: die
    /// Decke wird nur dort neu gerechnet, wo der Schritt etwas geändert hat).
    fn terrace_floors_in(&self, scope: Option<&[RunId]>) -> Vec<ElementId> {
        let mut out: Vec<(String, ElementId)> = self
            .elements
            .iter()
            .filter_map(|(id, e)| match e.kind {
                ElementKind::Floor(f) if in_scope(scope, f.run) => match self.floor(f.run) {
                    Some(Ok(s)) if !s.terraces.is_empty() => Some((e.number.clone(), id)),
                    _ => None,
                },
                _ => None,
            })
            .collect();
        out.sort_by(|a, b| a.0.cmp(&b.0));
        out.into_iter().map(|x| x.1).collect()
    }

    /// Dachterrasse auf einer Decke.
    pub fn terrace_of(&self, floor: ElementId) -> Option<ElementId> {
        self.elements
            .iter()
            .find(|(_, e)| e.kind == ElementKind::RoofTerrace { floor })
            .map(|(id, _)| id)
    }

    /// Attikablech an der Dachterrasse einer Decke.
    pub fn coping_of(&self, floor: ElementId) -> Option<ElementId> {
        self.elements
            .iter()
            .find(|(_, e)| e.kind == ElementKind::Coping { floor })
            .map(|(id, _)| id)
    }

    /// Legt fehlende Dachterrassen und Attikableche an und entfernt
    /// überzählige (E3, wie [`Model::sync_soffits`]): je Decke höchstens
    /// eins von jeder Art; bleibt der Rücksprung, bleiben Guid und Nummer.
    /// Beim ersten Mal kommen Werkstyp und Baustoffe dazu. Liefert die Zahl
    /// der angelegten und entfernten Bauteile.
    pub(super) fn sync_terraces(&mut self) -> (usize, usize) {
        self.sync_terraces_in(None)
    }

    /// [`Model::sync_terraces`] nur an den Decken der Züge `scope`; Terrasse
    /// und Blech je Decke kommen aus einer Liste (T7).
    pub(super) fn sync_terraces_in(&mut self, scope: Option<&[RunId]>) -> (usize, usize) {
        // Fußpunkt der Verblendschale (W2): der Baustoff beim ersten Mal,
        // auch wenn der Schritt nur den Typ der Wand darüber ändert
        if self.foamglass_material().is_none() && self.foot_wanted() {
            self.ensure_foamglass_material();
        }
        let want = self.abgleich_lesend(scope, || self.terrace_floors_in(scope));
        if !want.is_empty() {
            self.ensure_terrace_type();
            if want.iter().any(|f| {
                matches!(self.element(*f).map(|e| &e.kind),
                    Some(ElementKind::Floor(x)) if x.terrace.coping_mat.is_none())
            }) {
                self.ensure_coping_material();
            }
        }
        // (Bauteil, Decke, Terrasse?) aller betroffenen DT und AB
        let have: Vec<(ElementId, ElementId, bool)> = self
            .elements
            .iter()
            .filter_map(|(id, e)| match e.kind {
                ElementKind::RoofTerrace { floor } => Some((id, floor, true)),
                ElementKind::Coping { floor } => Some((id, floor, false)),
                _ => None,
            })
            .filter(|&(_, floor, _)| self.floor_in_scope(floor, scope))
            // das Blech des Flachdachs gleicht [`Model::sync_flat_roofs_in`] ab
            .filter(|&(id, floor, dt)| dt || !self.flat_roof_coping(id, floor))
            .collect();
        let (mut added, mut removed) = (0, 0);
        // Terrasse je gewünschter Decke (für den Typ unten)
        let mut terraces: Vec<(ElementId, ElementId)> = Vec::with_capacity(want.len());
        let mut copings: Vec<ElementId> = Vec::with_capacity(want.len());
        for (id, floor, dt) in have {
            let kept = if dt {
                terraces.iter().any(|x| x.0 == floor)
            } else {
                copings.contains(&floor)
            };
            if want.contains(&floor) && !kept {
                if dt {
                    terraces.push((floor, id));
                } else {
                    copings.push(floor);
                }
            } else {
                note!(self, Element, self.elements, id);
                self.elements.remove(id);
                self.touch();
                removed += 1;
            }
        }
        for category in [Category::RoofTerrace, Category::Coping] {
            for &floor in &want {
                let dt = category == Category::RoofTerrace;
                if (dt && terraces.iter().any(|x| x.0 == floor))
                    || (!dt && copings.contains(&floor))
                {
                    continue;
                }
                let Some((storey, seq)) = self.element(floor).map(|e| (e.storey, e.seq)) else {
                    continue;
                };
                let kind = if dt {
                    ElementKind::RoofTerrace { floor }
                } else {
                    ElementKind::Coping { floor }
                };
                let id = self.new_element(category, storey, seq, kind);
                if dt {
                    terraces.push((floor, id));
                }
                self.touch();
                added += 1;
            }
        }
        // Der Typ der Dachterrasse folgt der Wahl an der Decke
        for (floor, dt) in terraces {
            let t = self.terrace_type_of(floor);
            if self.element(dt).is_some_and(|e| e.layer_set != t) {
                note!(self, Element, self.elements, dt);
                if let Some(e) = self.elements.get_mut(dt) {
                    e.layer_set = t;
                }
                self.touch();
            }
        }
        (added, removed)
    }

    /// Die Decke `floor` gehört zu einem Zug in `scope` oder gibt es nicht
    /// mehr (dann fallen ihre abgeleiteten Bauteile weg).
    pub(super) fn floor_in_scope(&self, floor: ElementId, scope: Option<&[RunId]>) -> bool {
        match self.element(floor).map(|e| &e.kind) {
            Some(ElementKind::Floor(f)) => in_scope(scope, f.run),
            _ => true,
        }
    }

    /// Züge, deren abgeleitete Bauteile der offene Schritt ändern kann
    /// (Z4): die von [`Model::step_touched`]. Reine Darstellung (Stifte,
    /// Linien, Schraffuren, Oberflächen, Anzeige, Maßketten) zählt nicht
    /// (Review 2d T8, Review W39 zu #126), ebenso wenig Raumvorlagen und
    /// neu angelegte Typen und Baustoffe: Was es vorher nicht gab, trägt
    /// kein bestehendes Bauteil, und die Werksstoffe, auf die abgeleitete
    /// Bauteile zurückfallen, legt deren Abgleich mit dem ersten an (wie
    /// `touched.library` in `apply`, RB3). So gleicht der erste Wandschritt,
    /// der mit dem ersten Raum dessen Ausbautypen anlegt, nur seine Züge ab
    /// (Bemaßung, Review #112). Ein Geschoss oder Gebäude (Band ziehen,
    /// Gelände) gleicht die Züge seines Gebäudes samt Anschlüssen ab, nicht
    /// alle (Review L39). `None` (alle), sobald der Schritt mehr ändert,
    /// etwa einen bestehenden Typ, einen Baustoff oder ein Geschoss ohne
    /// Gebäude.
    pub(super) fn sync_scope(&self) -> Option<Vec<RunId>> {
        let open = self.txn.as_ref()?;
        let mut gebaeude: Vec<BuildingId> = Vec::new();
        for c in &open.changes {
            match c {
                // `new` füllt erst das Schließen: das Geschoss, wie es jetzt
                // ist (auch ein neu angelegtes, etwa die Ebene Flachdach)
                Change::Storey { id, old, .. } => {
                    for s in [old.as_ref(), self.storeys.get(*id)].into_iter().flatten() {
                        gebaeude.push(s.building?);
                    }
                }
                Change::Building { id, .. } => gebaeude.push(*id),
                _ => {}
            }
        }
        let nur_zuege = open
            .changes
            .iter()
            .filter(|c| {
                !matches!(
                    c,
                    Change::Pen { .. }
                        | Change::LineType { .. }
                        | Change::Fill { .. }
                        | Change::Surface { .. }
                        | Change::Display { .. }
                        | Change::Trades { .. }
                        | Change::ForeignRecords { .. }
                        | Change::Ext { .. }
                        | Change::Project { .. }
                        | Change::Location { .. }
                        | Change::Bemassung { .. }
                        | Change::SpaceTypes { .. }
                        | Change::Material { old: None, .. }
                        | Change::LayerSet { old: None, .. }
                )
            })
            .all(|c| {
                matches!(
                    c,
                    Change::Run { .. }
                        | Change::Element { .. }
                        | Change::Storey { .. }
                        | Change::Building { .. }
                )
            });
        if !nur_zuege {
            return None;
        }
        let mut runs = self.step_touched();
        if !gebaeude.is_empty() {
            let dazu: Vec<RunId> = self
                .runs
                .iter()
                .filter(|(_, r)| {
                    self.storey(r.storey)
                        .and_then(|s| s.building)
                        .is_some_and(|b| gebaeude.contains(&b))
                })
                .map(|(id, _)| id)
                .collect();
            let joined: Vec<RunId> = dazu.iter().flat_map(|&r| self.joined_runs(r)).collect();
            runs.extend(dazu);
            runs.extend(joined);
        }
        // Nach Platz geordnet, ohne Doppelte: [`in_scope`] sucht binär
        let mut gesehen = std::collections::HashSet::with_capacity(runs.len());
        runs.retain(|r| gesehen.insert(*r));
        runs.sort_by_key(|r| r.index());
        Some(runs)
    }

    /// Nach dem Laden: Dachterrassen und Attikableche passend zu den
    /// Rücksprüngen; liefert Hinweise, wenn welche ergänzt oder entfernt
    /// wurden (Regel 46).
    pub(crate) fn complete_terraces(&mut self) -> Vec<String> {
        let strict = std::mem::replace(&mut self.strict, false);
        let foamglass = self.foamglass_material().is_some();
        let (added, removed) = self.sync_terraces();
        self.strict = strict;
        let mut out = Vec::new();
        if !foamglass && self.foamglass_material().is_some() {
            out.push("Fußpunkt aus Schaumglas unter dem Verblender ergänzt".to_string());
        }
        if added > 0 {
            out.push("Dachterrasse bzw. Attikablech ergänzt".to_string());
        }
        if removed > 0 {
            out.push("Dachterrasse bzw. Attikablech ohne Rücksprung entfernt".to_string());
        }
        out
    }

    /// Setzt die Attikahöhe über OK Belag einer Decke (mm, 0 …
    /// [`MAX_UPSTAND`]).
    pub fn set_floor_upstand(&mut self, floor: ElementId, mm: f64) -> bool {
        self.edit_terrace(floor, |t| {
            ((0.0..=MAX_UPSTAND).contains(&mm)).then(|| t.upstand = mm)
        })
    }

    /// Setzt den Dämmstreifen auf der Attikakrone einer Decke (mm, 0 =
    /// keiner, sonst [`MIN_CROWN`] … [`MAX_CROWN`]).
    pub fn set_floor_crown(&mut self, floor: ElementId, mm: f64) -> bool {
        self.edit_terrace(floor, |t| {
            (mm == 0.0 || (MIN_CROWN..=MAX_CROWN).contains(&mm)).then(|| t.crown = mm)
        })
    }

    /// Setzt die Attikadämmung innen am Flachdach über einer Decke (mm, 0 =
    /// keine, sonst [`MIN_INNER`] … [`MAX_INNER`]).
    pub fn set_floor_inner(&mut self, floor: ElementId, mm: f64) -> bool {
        self.edit_terrace(floor, |t| {
            (mm == 0.0 || (MIN_INNER..=MAX_INNER).contains(&mm)).then(|| t.inner = mm)
        })
    }

    /// Baustoff des Dämmstreifens auf der Attika der Dachterrasse: die
    /// (erste) Dämmschicht ihres Aufbaus.
    pub fn terrace_crown_material(&self, floor: ElementId) -> Option<MaterialId> {
        self.terrace_layers(floor)
            .iter()
            .find(|l| l.function == LayerFunction::Insulation)
            .map(|l| l.material)
            .or_else(|| self.material_by_guid(TERRACE_INSULATION_GUID))
    }

    /// Prüfpunkt zur Dämmung auf der Attika (für [`Model::check`]): Dicke 0
    /// oder [`MIN_CROWN`] … [`MAX_CROWN`].
    pub(super) fn crown_problem(&self, number: &str, t: &Terrace) -> Option<String> {
        let c = t.crown;
        (c != 0.0 && !(MIN_CROWN..=MAX_CROWN).contains(&c)).then(|| {
            format!("{number}: Dämmung auf der Attika {c} mm (erlaubt 0 oder {MIN_CROWN}…{MAX_CROWN} mm)")
        })
    }

    /// Prüfpunkt zur Attikadämmung innen (für [`Model::check`]): Dicke 0
    /// oder [`MIN_INNER`] … [`MAX_INNER`].
    pub(super) fn inner_problem(&self, number: &str, t: &Terrace) -> Option<String> {
        let c = t.inner;
        (c != 0.0 && !(MIN_INNER..=MAX_INNER).contains(&c)).then(|| {
            format!(
                "{number}: Attikadämmung innen {c} mm (erlaubt 0 oder {MIN_INNER}…{MAX_INNER} mm)"
            )
        })
    }

    /// Setzt den Baustoff des Attikablechs (`None`: Titanzink 0,7).
    pub fn set_floor_coping_material(&mut self, floor: ElementId, m: Option<MaterialId>) -> bool {
        if m.is_some_and(|m| self.material(m).is_none()) {
            return false;
        }
        self.edit_terrace(floor, |t| {
            t.coping_mat = m;
            Some(())
        })
    }

    /// Setzt den Typ der Dachterrasse einer Decke (`None`: Werkstyp); er
    /// muss die Typart Dachterrasse haben.
    pub fn set_floor_terrace_type(&mut self, floor: ElementId, t: Option<LayerSetId>) -> bool {
        if t.is_some_and(|t| {
            self.layer_set(t)
                .is_none_or(|x| x.category != TypeCategory::RoofTerrace)
        }) {
            return false;
        }
        self.edit_terrace(floor, |x| {
            x.build_up = t;
            Some(())
        })
    }

    fn edit_terrace(
        &mut self,
        floor: ElementId,
        f: impl FnOnce(&mut Terrace) -> Option<()>,
    ) -> bool {
        let Some(ElementKind::Floor(old)) = self.element(floor).map(|e| e.kind.clone()) else {
            return false;
        };
        let mut t = old.terrace;
        if f(&mut t).is_none() {
            return false;
        }
        if t == old.terrace {
            return true;
        }
        note!(self, Element, self.elements, floor);
        if let Some(ElementKind::Floor(x)) = self.elements.get_mut(floor).map(|e| &mut e.kind) {
            x.terrace = t;
        }
        self.touch();
        true
    }

    /// Randdämmstreifen der Decke über dem Zug: aus dem Typ seiner Wände (K5).
    pub(super) fn strip_params(&self, run: RunId) -> Option<StripParams> {
        let t = self
            .run(run)?
            .segments
            .first()
            .and_then(|w| self.element(*w))
            .and_then(|e| e.layer_set)
            .and_then(|t| self.layer_set(t))?;
        // Ungültiges Auflager (Regel 21): wie „ganze tragende Schicht“
        if self.bearing_problem(t).is_some() {
            return None;
        }
        let core = t.layers.iter().find(|l| l.core).or(t.layers.first())?;
        Some(StripParams {
            width: t.strip_width()?,
            mat: material_key(t.strip_material()?),
            face: material_key(core.material),
            covered: !self.runs_above(run).is_empty(),
        })
    }

    /// Setzt die Dicke einer Decke (mm, von der Oberkante nach unten).
    /// Grenze: lichte Höhe darunter mindestens 1,00 m (OK bleibt stehen).
    pub fn set_floor_thickness(&mut self, floor: ElementId, thickness: f64) -> bool {
        let Some(ElementKind::Floor(f)) = self.element(floor).map(|e| &e.kind) else {
            return false;
        };
        let room = self.level_z(f.top).unwrap_or(0.0)
            - self.storey(f.top.storey).map_or(0.0, |s| s.elevation);
        if !(thickness > 0.0 && thickness.is_finite() && thickness <= room - MIN_CLEAR) {
            return false;
        }
        note!(self, Element, self.elements, floor);
        if let Some(ElementKind::Floor(f)) = self.elements.get_mut(floor).map(|e| &mut e.kind) {
            f.thickness = thickness;
        }
        self.touch();
        true
    }

    /// Aufbau eines Bauteils als Schichten (R4, bim/paket-r4-deckenschichten.md
    /// §1.3): bei Wänden die Schichten des Typs. Decke, Sohlplatte und
    /// Frostschürze haben ohne Typ den gedachten Einschicht-Aufbau aus
    /// Baustoff und Dicke (Breite); mit Typ dessen Schichten von oben nach
    /// unten, die Kernschicht mit Dicke und Baustoff vom Bauteil.
    /// Randdämmstreifen und Untersichtdämmung haben eine eingebaute Schicht
    /// aus Wandtyp bzw. Decke; die der Untersichtdämmung gehört zum
    /// Fassadensystem (Paket 1a). Leer: ohne Baustoff.
    pub fn element_layers(&self, id: ElementId) -> Vec<MaterialLayer> {
        let Some(e) = self.element(id) else {
            return Vec::new();
        };
        let typ = e.layer_set.and_then(|t| self.layer_set(t));
        match e.kind {
            ElementKind::Wall(_) => return typ.map(|t| t.layers.clone()).unwrap_or_default(),
            ElementKind::EdgeStrip { wall, .. } => {
                let t = self
                    .element(wall)
                    .and_then(|w| w.layer_set)
                    .and_then(|t| self.layer_set(t));
                return t
                    .and_then(|t| Some((t.strip_material()?, t.strip_width()?)))
                    .map(|(m, w)| vec![MaterialLayer::new(m, w, LayerFunction::Insulation)])
                    .unwrap_or_default();
            }
            ElementKind::SoffitInsulation { floor } => {
                let Some(ElementKind::Floor(f)) = self.element(floor).map(|x| &x.kind) else {
                    return Vec::new();
                };
                let Some(m) = self.soffit_material_of(floor) else {
                    return Vec::new();
                };
                let mut out =
                    vec![
                        MaterialLayer::new(m, f.soffit.thickness, LayerFunction::Insulation)
                            .trade(trade::soffit()),
                    ];
                // Bekleidung darunter (W3), Gewerk aus dem Baustoff
                if let Some(c) = self.soffit_cladding_material_of(floor) {
                    out.push(MaterialLayer::new(
                        c,
                        f.soffit.cladding,
                        LayerFunction::Finish,
                    ));
                }
                return out;
            }
            ElementKind::RoofTerrace { floor } => return self.terrace_layers(floor),
            ElementKind::Roof { .. } => return self.flat_roof_layers(id),
            // Eingebauter Ein-Schicht-Aufbau: XPS in der Dicke an der Platte
            ElementKind::PerimeterInsulation { slab } => {
                let t = self.slab_insulation(slab);
                return match self.perimeter_material() {
                    Some(m) if t > 0.0 => {
                        vec![MaterialLayer::new(m, t, LayerFunction::Insulation)]
                    }
                    _ => Vec::new(),
                };
            }
            // Eingebauter Ein-Schicht-Aufbau (Steckbrief AB §2): Gewerk nach
            // Jörn der Dachdecker, auch wenn Titanzink den Klempner vorschlägt.
            ElementKind::Coping { floor } => {
                let Some(m) = self.coping_material(floor) else {
                    return Vec::new();
                };
                let t = crate::qto::COPING_SHEET;
                let mut out =
                    vec![MaterialLayer::new(m, t, LayerFunction::Finish).trade(trade::roofing())];
                // Dämmung unter dem Blech (Jörn 10.10. 16:07): an der
                // Dachterrasse der Streifen auf der Krone, am Flachdach
                // Attikadämmung innen und Krone
                if let Some(ElementKind::Floor(f)) = self.element(floor).map(|x| &x.kind) {
                    // am Flachdach die Krone mit eigenem Baustoff (W9 an #60)
                    let schichten = if self.flat_roof_coping(id, floor) {
                        [
                            (inner_of(&f.terrace), self.flat_roof_insulation(floor)),
                            (crown_of(&f.terrace), self.flat_roof_crown_material(floor)),
                        ]
                    } else {
                        [
                            (0.0, None),
                            (crown_of(&f.terrace), self.terrace_crown_material(floor)),
                        ]
                    };
                    out.extend(schichten.into_iter().filter(|&(c, _)| c > 0.0).filter_map(
                        |(c, ins)| {
                            Some(
                                MaterialLayer::new(ins?, c, LayerFunction::Insulation)
                                    .trade(trade::roofing()),
                            )
                        },
                    ));
                }
                return out;
            }
            _ => {}
        }
        let (Some(material), Some(thickness)) = (e.kind.material(), e.kind.core_thickness()) else {
            return Vec::new();
        };
        match typ.filter(|t| t.category.variable_core()) {
            Some(t) => t
                .layers
                .iter()
                .map(|l| {
                    if l.core {
                        let mut core = l.with_material(material);
                        core.thickness = thickness;
                        core
                    } else {
                        *l
                    }
                })
                .collect(),
            None => vec![MaterialLayer::new(material, thickness, LayerFunction::Structure).core()],
        }
    }

    /// Gewerk der Schicht `i` von [`Model::element_layers`]: Schicht vor
    /// Baustoff vor Bauteilart (R4 §3). `None`: kein Gewerk (Luft).
    pub fn layer_trade(&self, id: ElementId, i: usize) -> Option<TradeId> {
        let l = *self.element_layers(id).get(i)?;
        l.trade
            .or_else(|| self.material(l.material)?.trade)
            .or_else(|| {
                let c = crate::kinds::spec(self.element(id)?.category).default_trade?;
                self.trade_by_code(c)
            })
    }

    /// Kostengruppe der Schicht `i` (DIN 276:2018): die an der Schicht,
    /// sonst nach Bauteilart und Lage zum Kern (paket-1a §4). Tragende
    /// Innenwände nach Regel 100 ([`crate::library::bears`]) 341, sonst 342.
    pub fn layer_kg(&self, id: ElementId, i: usize) -> Option<u16> {
        let e = self.element(id)?;
        let layers = self.element_layers(id);
        let l = layers.get(i)?;
        if l.kg.is_some() {
            return l.kg;
        }
        let first = layers.iter().position(|l| l.core);
        let last = layers.iter().rposition(|l| l.core);
        // (vor bzw. über dem Kern, Kern, hinter bzw. unter dem Kern)
        let (before, at, after) = match e.category {
            Category::ExteriorWall | Category::Parapet => (335, 331, 336),
            Category::InteriorWall if crate::library::bears(&layers, true) => (345, 341, 345),
            Category::InteriorWall => (345, 342, 345),
            Category::Floor => (353, 351, 354),
            Category::GroundSlab => (324, 322, 325),
            Category::StripFooting => (322, 322, 322),
            // ohne Kern: Teil der Deckenrandschale bzw. Deckenbekleidung
            Category::EdgeInsulation => return Some(331),
            Category::SoffitInsulation => return Some(354),
            Category::PerimeterInsulation => return Some(325),
            c => return c.din276(),
        };
        Some(match (first, last) {
            (Some(f), _) if i < f => before,
            (_, Some(l)) if i > l => after,
            (Some(_), Some(_)) => at,
            _ => return e.category.din276(),
        })
    }

    /// Gewerke nach Reihe.
    pub fn trades(&self) -> &[Trade] {
        &self.trades
    }

    pub fn trade(&self, id: TradeId) -> Option<&Trade> {
        self.trades.iter().find(|t| t.guid == id.0)
    }

    /// Gewerk mit der ATV-Nummer `code`.
    pub fn trade_by_code(&self, code: &str) -> Option<TradeId> {
        self.trades.iter().find(|t| t.code == code).map(Trade::id)
    }

    /// Kurzname eines Gewerks (Regel 67) im offenen Schritt; `""` entfernt
    /// ihn (dann gilt der Langname). `false`, wenn abgelehnt: mehr als 14
    /// Zeichen, eine Ziffer, ein Zeilenumbruch oder doppelt (ohne Groß-
    /// und Kleinschrift).
    pub fn set_trade_short(&mut self, id: TradeId, k: &str) -> bool {
        let k = k.trim();
        if !crate::trade::short_ok(k) || self.trade(id).is_none() {
            return false;
        }
        let low = k.to_lowercase();
        let taken = self.trades.iter().any(|t| {
            t.id() != id
                && !k.is_empty()
                && t.short.as_deref().map(str::to_lowercase) == Some(low.clone())
        });
        if taken {
            return false;
        }
        let new = Some(k.to_string());
        if self.trade(id).is_some_and(|t| t.short == new) {
            return true;
        }
        match self.txn.as_mut() {
            Some(t) => {
                if t.noted.insert(Key::Trades) {
                    t.changes.push(Change::Trades {
                        old: self.trades.clone(),
                        new: Vec::new(),
                    });
                }
            }
            None => debug_assert!(!self.strict, "Änderung ohne Schritt"),
        }
        if let Some(t) = self.trades.iter_mut().find(|t| t.id() == id) {
            t.short = new;
        }
        self.touch();
        true
    }

    /// Nimmt ein Gewerk aus dem Firmenkatalog auf, falls es fehlt
    /// (gleiche Guid: das Projekt behält seins).
    pub(crate) fn ensure_trade(&mut self, t: &Trade) {
        if self.trade(t.id()).is_none() {
            // Review 3n/7: mit Rückgängig
            match self.txn.as_mut() {
                Some(x) => {
                    if x.noted.insert(Key::Trades) {
                        x.changes.push(Change::Trades {
                            old: self.trades.clone(),
                            new: Vec::new(),
                        });
                    }
                }
                None => debug_assert!(!self.strict, "Änderung ohne Schritt"),
            }
            self.touch();
            self.trades.push(t.clone());
            self.trades.sort_by_key(|t| (t.order, t.guid));
        }
    }

    /// Gewerke aus einer Datei ([`trade::merge`]).
    pub(crate) fn adopt_trades(&mut self, trades: Vec<Trade>) {
        self.trades = trades;
    }

    /// Nach dem Laden (Paket 1a §3, §5): Dateien vor 1a (`old`, ohne
    /// `[trade]`) bekommen still die Gewerke der Start- und
    /// Ausbaubaustoffe bzw. nach Baustoffart. Verweise auf unbekannte
    /// Gewerke fallen mit Hinweis weg.
    /// Gilt nicht als Änderung.
    pub(crate) fn complete_trades(&mut self, old: bool) -> Vec<String> {
        let mut hints = Vec::new();
        let known: Vec<TradeId> = self.trades.iter().map(Trade::id).collect();
        let ids: Vec<MaterialId> = self.materials.ids().collect();
        for id in ids {
            let Some(x) = self.materials.get_mut(id) else {
                continue;
            };
            if old && x.trade.is_none() {
                x.trade = ausbau::finish_trade(x.guid)
                    .or_else(|| trade::for_material(&x.name, x.category));
            } else if x.trade.is_some_and(|t| !known.contains(&t)) {
                hints.push(format!("Baustoff „{}“: Gewerk unbekannt, entfernt", x.name));
                x.trade = None;
            }
        }
        let ids: Vec<LayerSetId> = self.layer_sets.ids().collect();
        for id in ids {
            let Some(t) = self.layer_sets.get_mut(id) else {
                continue;
            };
            for l in &mut t.layers {
                if l.trade.is_some_and(|g| !known.contains(&g)) {
                    hints.push(format!(
                        "Typ {}: Gewerk einer Schicht unbekannt, entfernt",
                        t.code
                    ));
                    l.trade = None;
                }
            }
        }
        hints
    }

    /// Gewerk, das der Baustoff vorschlägt (`None`: keins). `false`:
    /// unbekannter Baustoff oder Gewerk.
    pub fn set_material_trade(&mut self, id: MaterialId, t: Option<TradeId>) -> bool {
        if t.is_some_and(|t| self.trade(t).is_none()) {
            return false;
        }
        let Some(x) = self.materials.get(id) else {
            return false;
        };
        if x.trade == t {
            return true;
        }
        note!(self, Material, self.materials, id);
        if let Some(x) = self.materials.get_mut(id) {
            x.trade = t;
        }
        self.touch();
        true
    }

    /// Gewerk an Schicht `i` des Typs abweichend vom Baustoff (`None`:
    /// wie der Baustoff); alle Bauteile des Typs folgen.
    pub fn set_layer_trade(&mut self, typ: LayerSetId, i: usize, t: Option<TradeId>) -> bool {
        if t.is_some_and(|t| self.trade(t).is_none()) {
            return false;
        }
        self.edit_layer(typ, i, |l| l.trade = t)
    }

    /// Kostengruppe an Schicht `i` des Typs (`None`: nach Tabelle); nur
    /// 311–399 (Regel 49).
    pub fn set_layer_kg(&mut self, typ: LayerSetId, i: usize, kg: Option<u16>) -> bool {
        if kg.is_some_and(|k| !trade::valid_kg(k)) {
            return false;
        }
        self.edit_layer(typ, i, |l| l.kg = kg)
    }

    fn edit_layer(
        &mut self,
        typ: LayerSetId,
        i: usize,
        f: impl FnOnce(&mut MaterialLayer),
    ) -> bool {
        let Some(mut t) = self.layer_sets.get(typ).cloned() else {
            return false;
        };
        let Some(l) = t.layers.get_mut(i) else {
            return false;
        };
        f(l);
        self.set_layer_set(typ, t)
    }

    /// Setzt den Typ einer Decke, Sohlplatte oder Frostschürze; `None`
    /// heißt wieder gedachter Einschicht-Aufbau. Der Typ muss passen
    /// (Regel 37); der Baustoff am Bauteil folgt seiner Kernschicht
    /// (Regel 38), die Dicke bleibt am Bauteil.
    pub fn set_slab_type(&mut self, id: ElementId, set: Option<LayerSetId>) -> bool {
        let Some(e) = self.element(id) else {
            return false;
        };
        if e.kind.material().is_none() {
            return false;
        }
        let core = match set {
            None => None,
            Some(t) => {
                let Some(t) = self
                    .layer_set(t)
                    .filter(|t| TypeCategory::of(e.category) == Some(t.category))
                else {
                    return false;
                };
                let Some(core) = t.layers.iter().find(|l| l.core) else {
                    return false;
                };
                Some(core.material)
            }
        };
        if e.layer_set == set && core.is_none_or(|m| e.kind.material() == Some(m)) {
            return true;
        }
        note!(self, Element, self.elements, id);
        if let Some(e) = self.elements.get_mut(id) {
            e.layer_set = set;
            if let Some(m) = core {
                match &mut e.kind {
                    ElementKind::Floor(f) => f.material = m,
                    ElementKind::GroundSlab(s) => s.material = m,
                    ElementKind::StripFooting(f) => f.material = m,
                    _ => {}
                }
            }
        }
        self.touch();
        true
    }

    /// Hinweise, die keinen Fehler darstellen: Der Körper entsteht, aber etwas
    /// ist ungewöhnlich (Paneel „Eigenschaften“).
    pub fn warnings(&self, e: ElementId) -> Vec<String> {
        let mut out = Vec::new();
        // Ungültiges Auflager aus einer Datei (Regel 21): gebaut wie „ganze
        // tragende Schicht“
        if self
            .element(e)
            .filter(|x| matches!(x.kind, ElementKind::Wall(_)))
            .and_then(|x| x.layer_set)
            .and_then(|t| self.layer_set(t))
            .is_some_and(|t| self.bearing_problem(t).is_some())
        {
            out.push("Deckenauflager des Typs ungültig".into());
        }
        if let Some(&ElementKind::Roof { floor, .. }) = self.element(e).map(|x| &x.kind) {
            out.extend(self.flat_roof_hint(floor));
            out.extend(self.flat_roof_inner_hint(floor));
            out.extend(self.flat_roof_slope_hints(floor));
            return out;
        }
        if let Some(d) = self.downpipe(e) {
            out.extend(d.free_hints());
            return out;
        }
        let Some(run) = self.run_of(e) else {
            return out;
        };
        if matches!(
            self.element(e).map(|x| &x.kind),
            Some(ElementKind::Floor(_))
        ) {
            return out;
        }
        let Some((slab, _)) = self.foundation_of(run) else {
            return out;
        };
        if self
            .element(e)
            .is_some_and(|x| matches!(x.kind, ElementKind::Wall(_)))
        {
            return out;
        }
        let Some(&ElementKind::GroundSlab(s)) = self.element(slab).map(|x| &x.kind) else {
            return out;
        };
        // Schichten außen vor dem tragenden Kern (bei AW 31,5 die Dämmung)
        let outside: f64 = self
            .run(run)
            .and_then(|r| r.segments.first())
            .and_then(|w| self.element(*w))
            .and_then(|w| w.layer_set)
            .and_then(|id| self.layer_set(id))
            .map_or(0.0, |set| {
                set.layers
                    .iter()
                    .take_while(|l| !l.core)
                    .map(|l| l.thickness)
                    .sum()
            });
        if s.recess > outside && outside > 0.0 {
            // Rücksprung größer als die Schichten vor dem Kern
            out.push("Kern steht teils neben Platte".into());
        }
        if let Some(Ok(f)) = self.foundation(run) {
            if matches!(f.footing, FootingShape::Full(_)) {
                out.push("Schürze füllt die Platte ganz".into());
            }
        }
        out
    }
}
