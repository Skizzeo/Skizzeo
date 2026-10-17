//! Gebäudemodell, Teil „Bibliothek: Baustoffe, Schichtaufbauten, Typen“ (aus model.rs, Paket T1).

use super::*;

impl Model {
    pub fn materials(&self) -> &Arena<Material> {
        &self.materials
    }

    pub fn material(&self, id: MaterialId) -> Option<&Material> {
        self.materials.get(id)
    }

    /// Baustoff zu einem Darstellungsschlüssel aus einem Körper (Schnittbit egal).
    pub fn material_by_key(&self, key: u16) -> Option<&Material> {
        let key = key & !material::CUT;
        if key == material::PLAIN {
            return None;
        }
        self.materials.at_index(key as u32 - 1).map(|(_, m)| m)
    }

    pub fn add_material(&mut self, m: Material) -> MaterialId {
        self.touch();
        let id = self.materials.insert(m);
        note!(self, Material, new id);
        id
    }

    /// Wird der Baustoff benutzt ([`Model::material_uses`], Regel 15)?
    pub fn material_used(&self, id: MaterialId) -> bool {
        !self.material_uses(id).is_empty()
    }

    /// Wo der Baustoff steckt („Verwendet in“, Regel 15): jeder Typ mit ihm
    /// in einer Schicht oder als Randdämmstreifen, auch unbenutzte, mit der
    /// Zahl seiner Bauteile; dann Bauteile ohne Typ: Sohlplatte, Schürze,
    /// Decke (auch ihre Untersichtdämmung) und das Attikablech, auch mit der
    /// Vorgabe ohne eigene Wahl (A235).
    pub fn material_uses(&self, id: MaterialId) -> Vec<Use> {
        let mut out = Vec::new();
        // Definitionen, deren Körper oder Mengen den Baustoff treffen (E8c-a)
        let ext: Vec<&str> = self.ext_nutzer(id);
        for (t, s) in self.layer_sets.iter() {
            if s.layers.iter().any(|l| l.material == id) || s.strip_material() == Some(id) {
                let n = self
                    .elements
                    .iter()
                    .filter(|(_, e)| match e.kind {
                        ElementKind::RoofTerrace { floor } => {
                            self.terrace_type_of(floor) == Some(t)
                        }
                        _ => e.layer_set == Some(t),
                    })
                    .count();
                out.push(Use::Type(t, n));
            }
        }
        let copings: Vec<ElementId> = self
            .elements
            .iter()
            .filter_map(|(_, e)| match e.kind {
                ElementKind::Coping { floor } => Some(floor),
                _ => None,
            })
            .collect();
        for (e, x) in self.elements.iter() {
            let hit = match &x.kind {
                ElementKind::Floor(f) => {
                    f.material == id
                        || f.soffit.material == Some(id)
                        || f.soffit.cladding_material == Some(id)
                        // Die Wahl des Blechs zählt am Blech, fehlt es, an der Decke
                        || (f.terrace.coping_mat == Some(id) && !copings.contains(&e))
                }
                ElementKind::GroundSlab(g) => g.material == id,
                ElementKind::StripFooting(f) => f.material == id,
                ElementKind::Coping { floor } => self.coping_material(*floor) == Some(id),
                ElementKind::PerimeterInsulation { .. } => self.perimeter_material() == Some(id),
                ElementKind::Ext(p) => ext.contains(&p.key.as_str()),
                ElementKind::Wall(_)
                | ElementKind::EdgeStrip { .. }
                | ElementKind::SoffitInsulation { .. }
                | ElementKind::RoofTerrace { .. }
                | ElementKind::Roof { .. }
                | ElementKind::Downpipe { .. }
                | ElementKind::Opening(_)
                | ElementKind::Space(_) => false,
            };
            if hit {
                out.push(Use::Element(e));
            }
        }
        out
    }

    /// Darf der Baustoff gelöscht werden (§1.4, Regeln 15 und 55)? Nicht,
    /// wenn etwas auf ihn verweist; Luft und die eingebauten Baustoffe von
    /// Dachterrasse, Flachdach, Attikablech, Fußpunkt und Untersicht-Bekleidung
    /// nie. Kennwerte halten nichts fest.
    pub fn can_remove_material(&self, id: MaterialId) -> bool {
        self.materials.get(id).is_some_and(|m| {
            m.category != MatCategory::Air
                && ![
                    TERRACE_FINISH_GUID,
                    TERRACE_INSULATION_GUID,
                    COPING_MAT_GUID,
                    flachdach::ROOF_SEAL_GUID,
                    flachdach::ROOF_INSULATION_GUID,
                    flachdach::ROOF_CROWN_GUID,
                    flachdach::ROOF_VAPOUR_GUID,
                    FOAMGLASS_MAT_GUID,
                    CLADDING_MAT_GUID,
                ]
                .contains(&m.guid)
        }) && !self.material_used(id)
    }

    /// Löscht einen Baustoff, der gelöscht werden darf
    /// ([`Model::can_remove_material`]). `false`: nichts geändert.
    pub fn remove_material(&mut self, id: MaterialId) -> bool {
        if !self.can_remove_material(id) {
            return false;
        }
        note!(self, Material, self.materials, id);
        self.touch();
        self.materials.remove(id).is_some()
    }

    /// Kopie eines Baustoffs mit neuer Guid und dem nächsten freien Namen
    /// „Porenbeton (2)“, „(3)“ …; Kennwerte und Darstellung wie das Vorbild
    /// (Entscheidung 20: Neues entsteht per Duplizieren). `None`: fehlt.
    pub fn duplicate_material(&mut self, id: MaterialId) -> Option<MaterialId> {
        let x = self.materials.get(id)?.clone();
        let stem = match x.name.rsplit_once(" (") {
            Some((a, b))
                if b.strip_suffix(')')
                    .is_some_and(|n| n.parse::<u32>().is_ok()) =>
            {
                a.to_string()
            }
            _ => x.name.clone(),
        };
        let name = (2..)
            .map(|n| format!("{stem} ({n})"))
            .find(|n| !self.materials.iter().any(|(_, m)| m.name == *n))?;
        let guid = self.new_guid();
        Some(self.add_material(Material { guid, name, ..x }))
    }

    /// `name`, wenn kein anderer Baustoff als `except` ihn trägt, sonst der
    /// nächste freie „Name (2)“, „(3)“ …
    pub fn free_material_name(&self, name: &str, except: Option<MaterialId>) -> String {
        let taken = |n: &str| {
            self.materials
                .iter()
                .any(|(id, m)| Some(id) != except && m.name == n)
        };
        if !taken(name) {
            return name.to_string();
        }
        (2..)
            .map(|n| format!("{name} ({n})"))
            .find(|n| !taken(n))
            .unwrap_or_else(|| name.to_string())
    }

    /// Nur den Namen eines Baustoffs ändern, ohne die übrigen Werte zu
    /// prüfen (Zwischennamen beim Tauschen, Review 3q/2): Der Name muss
    /// frei und nicht leer sein.
    pub(crate) fn rename_material(&mut self, id: MaterialId, name: &str) -> bool {
        if name.trim().is_empty()
            || self
                .materials
                .iter()
                .any(|(o, x)| o != id && x.name == name)
        {
            return false;
        }
        if self.materials.get(id).is_none_or(|x| x.name == name) {
            return self.materials.get(id).is_some();
        }
        note!(self, Material, self.materials, id);
        if let Some(x) = self.materials.get_mut(id) {
            x.name = name.to_string();
        }
        self.touch();
        self.attr.bump();
        true
    }

    /// Ersetzt einen Baustoff (Materialfenster, Paket 5). Abgelehnt, wenn der
    /// Name leer oder vergeben ist, eine Darstellung oder das Gewerk fehlt,
    /// die Guid wechselt oder ein Kennwert die Regeln 50–53 verletzt.
    pub fn set_material(&mut self, id: MaterialId, m: Material) -> bool {
        let Some(old) = self.materials.get(id) else {
            return false;
        };
        if *old == m {
            return true;
        }
        let a = &self.attr;
        let ok = old.guid == m.guid
            && !m.name.trim().is_empty()
            && !self
                .materials
                .iter()
                .any(|(o, x)| o != id && x.name == m.name)
            && a.fill(m.cut_fill).is_some()
            && a.pen(m.cut_fg).is_some()
            && a.pen(m.cut_bg).is_some()
            && a.surface(m.surface).is_some()
            && m.trade.is_none_or(|t| self.trade(t).is_some())
            && crate::matprop::check_density(m.category, m.density).is_ok()
            && m.lambda
                .is_none_or(|l| crate::matprop::check_lambda(m.category, l).is_ok())
            && m.props.iter().all(|(k, v)| {
                // ein Altwert aus der Datei bleibt, auch wenn er die Prüfung
                // nicht besteht (Verschnittzuschlag, Review #77/4)
                *k == crate::matprop::normalize_key(k)
                    && (crate::matprop::check_prop(m.category, k, v).is_ok()
                        || old.props.get(k) == Some(v))
            });
        if !ok {
            return false;
        }
        note!(self, Material, self.materials, id);
        if let Some(x) = self.materials.get_mut(id) {
            *x = m;
        }
        self.touch();
        self.attr.bump();
        true
    }

    pub fn layer_sets(&self) -> &Arena<LayerSet> {
        &self.layer_sets
    }

    pub fn layer_set(&self, id: LayerSetId) -> Option<&LayerSet> {
        self.layer_sets.get(id)
    }

    /// Legt einen Bauteiltyp an. `None`, wenn das Kurzzeichen leer oder
    /// schon vergeben ist, die Guid schon existiert oder der Typ gegen die
    /// Typregeln verstößt ([`LayerSet::problems`], Deckenauflager, Schicht
    /// ohne Dicke im Ausbau nur als Beschichtung).
    pub fn add_layer_set(&mut self, s: LayerSet) -> Option<LayerSetId> {
        self.insert_layer_set(s, true)
    }

    /// Wie [`Model::add_layer_set`], ein ungültiges Deckenauflager (Regel 21)
    /// bleibt aber stehen und wird wie „ganze tragende Schicht“ gebaut, wie
    /// beim Laden einer `.szo` (Übernahme aus dem Firmenkatalog); ebenso
    /// eine Ausbauschicht ohne Dicke, die keine Beschichtung ist.
    pub(crate) fn adopt_layer_set(&mut self, s: LayerSet) -> Option<LayerSetId> {
        self.insert_layer_set(s, false)
    }

    pub(super) fn insert_layer_set(&mut self, s: LayerSet, strict: bool) -> Option<LayerSetId> {
        let taken = self
            .layer_sets
            .iter()
            .any(|(_, t)| t.guid == s.guid || t.code == s.code);
        if taken
            || !s.problems().is_empty()
            || (strict && self.bearing_problem(&s).is_some())
            || (strict && self.finish_problems(&s).is_some())
            || s.layers
                .iter()
                .map(|l| l.material)
                .chain(s.strip_material())
                .any(|m| !self.materials.contains(m))
        {
            return None;
        }
        self.touch();
        let id = self.layer_sets.insert(s);
        note!(self, LayerSet, new id);
        Some(id)
    }

    /// Ändert einen Bauteiltyp; alle Bauteile dieses Typs folgen. Die Guid
    /// bleibt (Regel 19), das Kurzzeichen bleibt eindeutig, die Typart
    /// ändert sich nur bei unbenutzten Typen, die nicht Standard sind.
    /// `changed` zählt eins hoch (auch beim Übernehmen aus dem
    /// Firmenkatalog: es ist der Stand im Projekt). Gleicher Inhalt ändert
    /// nichts und gilt als Erfolg; `false`: abgelehnt, nichts geändert.
    pub fn set_layer_set(&mut self, id: LayerSetId, s: LayerSet) -> bool {
        self.replace_layer_set(id, s, true)
    }

    /// Setzt nur die Bauleistung einer Schicht (`svc=`, Regel 99). Anders
    /// als [`Model::set_layer_set`] prüft das den Typ nicht noch einmal:
    /// Die Bauleistung ändert keine Geometrie und muss auch an einem Typ
    /// mit gemeldetem Problem (Regel 21) ankommen. Anschlüsse bleiben.
    pub fn set_layer_svc(&mut self, id: LayerSetId, layer: usize, svc: Option<Guid>) -> bool {
        let Some(l) = self.layer_sets.get(id).and_then(|t| t.layers.get(layer)) else {
            return false;
        };
        if l.svc == svc {
            return true;
        }
        note!(self, LayerSet, self.layer_sets, id);
        if let Some(t) = self.layer_sets.get_mut(id) {
            t.layers[layer].svc = svc;
            t.changed += 1;
        }
        self.touch();
        true
    }

    /// Wie [`Model::set_layer_set`] mit ungültigem Deckenauflager wie
    /// [`Model::adopt_layer_set`].
    pub(crate) fn adopt_set_layer_set(&mut self, id: LayerSetId, s: LayerSet) -> bool {
        self.replace_layer_set(id, s, false)
    }

    pub(super) fn replace_layer_set(
        &mut self,
        id: LayerSetId,
        mut s: LayerSet,
        strict: bool,
    ) -> bool {
        let Some(old) = self.layer_sets.get(id) else {
            return false;
        };
        let code_taken = self
            .layer_sets
            .iter()
            .any(|(other, t)| other != id && t.code == s.code);
        let category_locked = s.category != old.category
            && (self.is_default_type(id)
                || !self.type_users(id).is_empty()
                || self.type_in_space_type(id)
                || self.type_in_space_finish(id));
        if s.guid != old.guid
            || code_taken
            || category_locked
            || !s.problems().is_empty()
            || (strict && self.bearing_problem(&s).is_some())
            || (strict && self.finish_problems(&s).is_some())
            || s.layers
                .iter()
                .map(|l| l.material)
                .chain(s.strip_material())
                .any(|m| !self.materials.contains(m))
        {
            return false;
        }
        if same_type(old, &s) {
            return true;
        }
        s.changed = old.changed + 1;
        note!(self, LayerSet, self.layer_sets, id);
        if let Some(t) = self.layer_sets.get_mut(id) {
            *t = s;
        }
        // Andere Dicken: Anschlüsse neu erkennen
        self.joins.setzen(self.detect_all());
        self.touch();
        true
    }

    /// Bauteile, die den Typ benutzen (Wände).
    pub fn type_users(&self, id: LayerSetId) -> Vec<ElementId> {
        self.elements
            .iter()
            .filter(|(_, e)| e.layer_set == Some(id))
            .map(|(i, _)| i)
            .collect()
    }

    /// Typ zu einem Kurzzeichen.
    pub fn type_by_code(&self, code: &str) -> Option<LayerSetId> {
        self.layer_sets
            .iter()
            .find(|(_, t)| t.code == code)
            .map(|(id, _)| id)
    }

    /// Typ zu einer Guid.
    pub fn type_by_guid(&self, g: Guid) -> Option<LayerSetId> {
        self.layer_sets
            .iter()
            .find(|(_, t)| t.guid == g)
            .map(|(id, _)| id)
    }

    /// `code`, wenn frei, sonst das nächste freie „code-2“, „code-3“ …
    pub fn free_code(&self, code: &str) -> String {
        free_code(code, |c| self.type_by_code(c).is_some())
    }

    /// Ist der Typ Standard für neue Wände seiner Art? Standardtypen lassen
    /// sich nicht löschen (Regel 18).
    pub fn is_default_type(&self, id: LayerSetId) -> bool {
        self.defaults.exterior_wall == id || self.defaults.interior_wall == id
    }

    /// Standardtyp für neue Wände der Art. Nur Wandarten haben einen
    /// ([`TypeCategory::WALLS`]); waagerechte Bauteile kommen ohne Typ aus.
    pub fn default_type(&self, cat: TypeCategory) -> LayerSetId {
        debug_assert!(cat.is_wall(), "{cat:?} hat keinen Standardtyp");
        match cat {
            TypeCategory::InteriorWall => self.defaults.interior_wall,
            _ => self.defaults.exterior_wall,
        }
    }

    /// Regel 21: „fest, Rest Randstreifen“ nur an Außenwänden, deren erste
    /// Schicht Kern ist, Tiefe im Bereich ([`LayerSet::bearing_range`]),
    /// Streifen aus Dämmung. Ein ungültiges Auflager bleibt gespeichert und
    /// wird wie „ganze tragende Schicht“ gebaut.
    pub fn bearing_problem(&self, t: &LayerSet) -> Option<String> {
        let Bearing::Depth { strip, .. } = t.bearing else {
            return None;
        };
        if let Some(p) = t.bearing_problem() {
            return Some(p);
        }
        let insulation = self
            .material(strip)
            .is_some_and(|m| m.category == MatCategory::Insulation);
        (!insulation).then(|| {
            let who = if t.code.is_empty() { &t.name } else { &t.code };
            format!("Typ {who}: Randdämmstreifen nur aus Dämmung")
        })
    }

    /// Kopie eines Typs: neue Guid, Name „… (Kopie)“, nächstes freies
    /// Kurzzeichen „…-2“, Stand 1.
    pub fn duplicate_type(&mut self, id: LayerSetId) -> Option<LayerSetId> {
        let t = self.layer_set(id)?.clone();
        let guid = self.new_guid();
        // Das eigene Kurzzeichen ist belegt: „…-2“, „…-3“ …
        let code = self.free_code(&t.code);
        self.add_layer_set(LayerSet {
            guid,
            name: format!("{} (Kopie)", t.name),
            code,
            changed: 1,
            ..t
        })
    }

    /// Löscht einen Typ, den kein Bauteil benutzt und der nicht Standard ist
    /// (Regel 18). `Err(n)`: n Bauteile benutzen ihn; `Err(0)`: Standardtyp
    /// oder kein Typ.
    pub fn remove_type(&mut self, id: LayerSetId) -> Result<(), usize> {
        let users = self.type_users(id).len();
        if users > 0 {
            return Err(users);
        }
        if self.is_default_type(id)
            || !self.layer_sets.contains(id)
            || self.type_in_space_type(id)
            || self.type_in_space_finish(id)
        {
            return Err(0);
        }
        note!(self, LayerSet, self.layer_sets, id);
        self.layer_sets.remove(id);
        self.touch();
        Ok(())
    }

    /// Setzt den Standardtyp für neue Wände der Art; `false`, wenn der Typ
    /// fehlt oder eine andere Art hat.
    pub fn set_default_type(&mut self, cat: TypeCategory, id: LayerSetId) -> bool {
        if self.layer_set(id).is_none_or(|t| t.category != cat) {
            return false;
        }
        let mut d = self.defaults;
        match cat {
            TypeCategory::ExteriorWall => d.exterior_wall = id,
            TypeCategory::InteriorWall => d.interior_wall = id,
            _ => return false,
        }
        if d == self.defaults {
            return true;
        }
        match self.txn.as_mut() {
            Some(t) => {
                if t.noted.insert(Key::Defaults) {
                    t.changes.push(Change::Defaults {
                        old: self.defaults,
                        new: d,
                    });
                }
            }
            None => debug_assert!(!self.strict, "Änderung ohne Schritt"),
        }
        self.defaults = d;
        self.touch();
        true
    }

    /// Wechselt den Typ eines Wandzugs. Alle Segmente bekommen ihn, dazu
    /// die gekoppelten Züge darunter und darüber, damit die Schale fugenlos
    /// bleibt. Bei Außenwänden bleibt die Außenseite stehen und die Wand
    /// wächst nach innen: Liegt die Außenseite nicht auf der Bezugslinie,
    /// wandert die Linie um die Dickenänderung. Decke, Gründung und
    /// Anschlüsse ziehen nach. `false` (nichts geändert), wenn der Typ fehlt,
    /// nicht zur Wand passt oder die Linie sich nicht verschieben lässt.
    pub fn set_run_type(&mut self, run: RunId, id: LayerSetId) -> bool {
        let Some(t) = self.layer_set(id) else {
            return false;
        };
        let cat = t.category;
        let mut root = run;
        for _ in 0..64 {
            match self.run_below(root) {
                Some(b) if b != root => root = b,
                _ => break,
            }
        }
        let mut stack = vec![root];
        stack.extend(self.stack_above(root));
        let walls: Vec<ElementId> = stack
            .iter()
            .filter_map(|r| self.run(*r))
            .flat_map(|r| r.segments.iter().copied())
            .collect();
        if !stack.contains(&run)
            || walls.iter().any(|w| {
                self.element(*w)
                    .is_none_or(|e| TypeCategory::of(e.category) != Some(cat))
            })
        {
            return false;
        }
        if walls
            .iter()
            .all(|w| self.element(*w).is_some_and(|e| e.layer_set == Some(id)))
        {
            return true;
        }
        // Neue Bezugslinie vorab, damit bei einem Fehler nichts geändert ist
        let mut points = None;
        if cat == TypeCategory::ExteriorWall {
            let Some(old) = self.base_chain(root) else {
                return false;
            };
            let new = WallChain {
                layers: self.wall_layers(id),
                ..old.clone()
            };
            let d = (old.outer_offset() - new.outer_offset()) * old.outward_sign();
            if d.abs() > 1e-9 {
                let offsets = vec![d; old.segment_count()];
                match old.with_segment_offsets(&offsets) {
                    Some(c) => points = Some(c.points),
                    None => return false,
                }
            }
        }
        let mut vorher = Vec::with_capacity(walls.len());
        for w in walls {
            note!(self, Element, self.elements, w);
            if let Some(e) = self.elements.get_mut(w) {
                vorher.push((w, e.layer_set));
                e.layer_set = Some(id);
            }
        }
        match points {
            Some(p) => {
                // Lehnt das Setzen die neue Linie ab (etwa weil eine Gehrung
                // kippte, H-160d), bleibt der alte Typ
                if self.set_run_points(root, &p).is_none() {
                    for (w, t) in vorher {
                        if let Some(e) = self.elements.get_mut(w) {
                            e.layer_set = t;
                        }
                    }
                    return false;
                }
            }
            None => self.joins.setzen(self.detect_all()),
        }
        self.touch();
        true
    }

    /// Entfernt einen Typ ohne Rückgängig und ohne Prüfung (nur beim
    /// Anlegen eines Projekts aus dem Firmenkatalog, [`crate::catalog`]).
    pub(crate) fn forget_type(&mut self, id: LayerSetId) {
        debug_assert!(self.type_users(id).is_empty() && self.txn.is_none());
        self.layer_sets.remove(id);
    }

    /// Merkmale eines Bauteils: die seines Typs, überlagert von den eigenen
    /// mit gleichem Schlüssel (IFC-Regel). Schichten und Dicken lassen sich
    /// am Bauteil nicht überschreiben.
    pub fn props_of(&self, el: ElementId) -> PropSet {
        let Some(e) = self.element(el) else {
            return PropSet::new();
        };
        let mut p = e
            .layer_set
            .and_then(|s| self.layer_set(s))
            .map_or_else(PropSet::new, |t| t.props.clone());
        p.extend(e.props.iter().map(|(k, v)| (k.clone(), v.clone())));
        // Abgeleitete Merkmale der Dachterrasse und des Blechs (BIM §6)
        if let ElementKind::RoofTerrace { floor } | ElementKind::Coping { floor } = e.kind {
            let slab = self
                .run_of(floor)
                .and_then(|r| self.floor(r))
                .and_then(|f| f.ok());
            if let Some(f) = slab.filter(|f| !f.terraces.is_empty()) {
                match e.kind {
                    ElementKind::RoofTerrace { .. } => {
                        let walk = f.terraces.depth() >= WALKABLE_DEPTH - 1e-6;
                        p.insert("begehbar".into(), PropValue::Bool(walk));
                    }
                    _ => {
                        let g = f.coping_girth();
                        p.insert("Abwicklung".into(), PropValue::Number(g));
                        let w = crate::terrace::coping_cut_width(g);
                        p.insert("Zuschnitt".into(), PropValue::Number(w));
                    }
                }
            }
        }
        // ... und des Flachdachs (D3, D4)
        p.extend(self.flat_roof_props(el));
        p.extend(self.downpipe_props(el));
        p
    }

    /// U-Wert eines Typs in W/(m²K), nur zur Anzeige ([`Model::u_value_of`]).
    pub fn u_value(&self, id: LayerSetId) -> Option<f64> {
        self.u_value_of(self.layer_set(id)?)
    }

    /// U = 1 / (0,13 + Σ d/λ + 0,18 je Luftschicht + 0,04), nur bei
    /// Außenwänden; `None`, wenn einer Schicht λ fehlt.
    pub fn u_value_of(&self, s: &LayerSet) -> Option<f64> {
        if !s.is_external() || s.layers.is_empty() {
            return None;
        }
        let mut r = 0.13 + 0.04;
        for l in &s.layers {
            if l.function == LayerFunction::AirGap {
                r += 0.18;
                continue;
            }
            let lambda = self.material(l.material)?.lambda?;
            if lambda <= 0.0 {
                return None;
            }
            r += l.thickness / 1000.0 / lambda;
        }
        Some(1.0 / r)
    }

    /// Schichten eines Aufbaus für die Geometrie (Darstellungsschlüssel statt Kennung).
    pub fn wall_layers(&self, id: LayerSetId) -> Vec<Layer> {
        self.layer_set(id).map_or_else(Vec::new, |s| {
            s.layers
                .iter()
                .map(|l| Layer {
                    thickness: l.thickness,
                    material: material_key(l.material),
                    core: l.core,
                    air: l.function == LayerFunction::AirGap,
                })
                .collect()
        })
    }
}
