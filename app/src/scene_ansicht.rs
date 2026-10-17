//! Szene, Teil „Netze der Ansichten und Übergänge“ (aus scene.rs, Paket T2).

use super::*;

impl Scene {
    /// Uhr der Übergänge stellen (ms seit Start der App).
    pub fn set_now(&mut self, ms: u64) {
        self.now = ms;
    }

    /// Führt `f` aus; ändert es das Modell (und mit `grow`), zeigt das
    /// gezeichnete Netz den Übergang vom bisherigen zum neuen Stand (K3b).
    /// Wände, deren Typ sich geändert hat, leuchten nach.
    pub(super) fn grown<R>(&mut self, grow: bool, f: impl FnOnce(&mut Scene) -> R) -> R {
        let rev = self.model.revision();
        let key = self
            .shown_key
            .clone()
            .filter(|_| grow && self.theme.size.anim_ms > 0.0);
        let from = key
            .as_ref()
            .map(|k| self.grow_mesh(k).unwrap_or_else(|| self.plain_mesh(k)));
        let sets: Vec<(sk_model::LayerSetId, sk_model::LayerSet)> = match key {
            Some(_) => self
                .model
                .layer_sets()
                .iter()
                .map(|(id, t)| (id, t.clone()))
                .collect(),
            None => Vec::new(),
        };
        let r = f(self);
        if self.model.revision() == rev {
            return r;
        }
        self.grow = None;
        let (Some(key), Some(from)) = (key, from) else {
            return r;
        };
        let to = self.plain_mesh(&key);
        let m = &self.model;
        let mut glow = Vec::new();
        for (id, t) in m.layer_sets().iter() {
            if !sets.iter().any(|(i, s)| *i == id && s == t) {
                glow.extend(m.type_users(id));
            }
        }
        let morph = morphable(&from, &to) && (from.faces != to.faces || from.edges != to.edges);
        if morph || !glow.is_empty() {
            self.grow = Some(Grow {
                key,
                morph,
                from,
                to,
                rev: self.model.revision(),
                start: self.now,
                glow,
            });
        }
        r
    }

    /// Mischt sich das Netz gerade (Form, nicht nur Leuchten)?
    #[cfg(test)]
    pub fn morphing(&self) -> bool {
        self.grow_k().is_some()
    }

    /// Läuft ein Übergang (Form oder Leuchten)? Dann braucht es Bilder.
    pub fn growing(&self) -> bool {
        self.grow.is_some()
    }

    /// Uhr weiterstellen; `true`, solange sich das Netz noch mischt oder
    /// zuletzt gemischt gezeigt wurde (dann einmal neu hochladen).
    pub fn grow_tick(&mut self, now: u64) -> bool {
        self.now = now;
        if let Some(g) = &self.grow {
            let t = now.saturating_sub(g.start) as f32;
            let size = &self.theme.size;
            if t >= size.anim_ms.max(size.flash_ms) || size.anim_ms <= 0.0 {
                self.grow = None;
            }
        }
        self.grow_k().is_some() || self.blend_shown
    }

    /// Nachleuchten der geänderten Wände: Stärke (linear von [`GROW_GLOW`]
    /// auf 0 in `flash_ms`) und Wände.
    pub fn grow_glow(&self) -> (f32, &[ElementId]) {
        let Some(g) = &self.grow else {
            return (0.0, &[]);
        };
        let ms = self.theme.size.flash_ms;
        let t = self.now.saturating_sub(g.start) as f32;
        if g.glow.is_empty() || ms <= 0.0 || self.theme.size.anim_ms <= 0.0 || t >= ms {
            return (0.0, &[]);
        }
        (GROW_GLOW * (1.0 - t / ms), &g.glow)
    }

    /// Klick während des Übergangs: sofort Endstand, Leuchten aus.
    pub fn skip_animation(&mut self) -> bool {
        self.grow.take().is_some()
    }

    pub(super) fn step(&mut self, dir: Direction) -> bool {
        let from = match dir {
            Direction::Undo => &mut self.undo,
            Direction::Redo => &mut self.redo,
        };
        let Some(t) = from.pop() else {
            return false;
        };
        let touched = self.model.apply(&t.txn, dir);
        self.ausbau.merken(&self.model, &t.txn.changes);
        self.staende_folgen(&t, dir);
        match dir {
            Direction::Undo => self.redo.push(t),
            Direction::Redo => self.undo.push(t),
        }
        self.mark_touched(&touched);
        self.rebuild_dirty(false);
        self.netz_anstossen();
        true
    }

    /// Netz einer Ansicht aus allen Wandzügen außer `except` (die gerade
    /// gezogenen). Beim Schnitt: Ebene `section` (Punkt, Normale zum Betrachter),
    /// alles davor wird weggeschnitten.
    pub fn mesh(&mut self, view: ViewKind, section: Option<Plane>, except: &[RunId]) -> MeshData {
        self.sicht_fuer(view);
        let key = (view, section, except.to_vec());
        self.shown_key = Some(key.clone());
        let m = self.grow_mesh(&key);
        self.blend_shown = m.is_some();
        m.unwrap_or_else(|| self.plain_mesh(&key))
    }

    /// Netz im laufenden Übergang, falls er für diese Ansicht gerade mischt.
    fn grow_mesh(&self, key: &MeshKey) -> Option<MeshData> {
        let g = self.grow.as_ref()?;
        let k = self.grow_k()?;
        (g.morph && g.key == *key && g.rev == self.model.revision())
            .then(|| blend(&g.from, &g.to, k))
    }

    /// Fortschritt der Form im Übergang (0 … 1, ease-out); `None` am Ende.
    fn grow_k(&self) -> Option<f32> {
        let g = self.grow.as_ref()?;
        let ms = self.theme.size.anim_ms;
        let t = self.now.saturating_sub(g.start) as f32;
        (g.morph && ms > 0.0 && t < ms).then(|| ease_out(t / ms))
    }

    fn plain_mesh(&mut self, key: &MeshKey) -> MeshData {
        let runs = self.mesh_keys(key);
        let [m, g] = self.mesh_ruhend(key.0, key.1, &runs);
        self.ghost = if self.filtering() {
            Some((key.clone(), self.vis_stamp(), g))
        } else {
            None
        };
        m
    }

    /// Züge eines Netzes: alle außer den ausgelassenen.
    fn mesh_keys(&self, key: &MeshKey) -> Vec<RunId> {
        self.cache
            .iter()
            .flatten()
            .map(|c| c.id)
            .filter(|id| !key.2.contains(id))
            .collect()
    }

    /// Wie der Zug zum Geschoss `active` liegt (Grundriss).
    pub(super) fn plan_mode(&self, run: RunId, active: StoreyId) -> PlanMode {
        if self.is_foundation(active) {
            return PlanMode::Foundation;
        }
        let floor = self.model.storey(active).map_or(0.0, |st| st.elevation);
        let lower = self
            .model
            .run(run)
            .and_then(|r| self.model.storey(r.storey))
            .is_some_and(|st| st.top() <= floor + 1e-6);
        if lower {
            PlanMode::Lower
        } else {
            PlanMode::Cut
        }
    }

    /// Geschosse, die der Bogen als Nächstes zeigen kann: das aktive und
    /// seine Nachbarn (E18).
    fn plan_candidates(&self) -> Vec<StoreyId> {
        let a = self.active_storey();
        [
            Some(a),
            self.model.level_above(a),
            self.model.level_below(a),
        ]
        .into_iter()
        .flatten()
        .collect()
    }

    /// Liegt der Grundriss des Geschosses `id` schon berechnet bereit (alle
    /// Wandzüge samt Hintergrund)?
    pub fn plan_ready(&self, id: StoreyId) -> bool {
        let cut = self.plan_cut_of(id);
        let memo = UnderMemo::default();
        self.cache.iter().flatten().all(|c| {
            c.has_plan(cut, self.plan_mode(c.id, id))
                && self
                    .under_cut_for(c.id, id, &memo)
                    .is_none_or(|u| c.under.iter().any(|(x, _)| *x == u))
        })
    }

    /// Leerlauf (E18): Grundrisse des aktiven Geschosses und seiner
    /// Nachbarn vorbereiten, damit ein Wechsel nichts neu rechnet.
    pub fn prepare_neighbor_plans(&mut self) {
        for id in self.plan_candidates() {
            if !self.plan_ready(id) {
                self.prepare_plan(id);
            }
        }
    }

    /// Fehlt noch ein vorbereiteter Grundriss (Leerlauf hat zu tun)?
    pub fn plans_pending(&self) -> bool {
        self.plan_candidates()
            .into_iter()
            .any(|id| !self.plan_ready(id))
    }

    /// Grundriss des Geschosses `id` berechnen (vor einem Wechsel dorthin).
    pub fn prepare_plan(&mut self, id: StoreyId) {
        let cut = self.plan_cut_of(id);
        let memo = UnderMemo::default();
        let jobs: Vec<(RunId, PlanMode, Option<f64>)> = self
            .cache
            .iter()
            .flatten()
            .map(|c| {
                let under = self.under_cut_for(c.id, id, &memo);
                (c.id, self.plan_mode(c.id, id), under)
            })
            .collect();
        for (run, mode, under) in jobs {
            if let Some(Some(c)) = self.cache.get_mut(run.index() as usize) {
                c.view_solid(ViewKind::Plan, None, cut, mode);
                if let Some(u) = under {
                    c.under_edges(u);
                }
            }
        }
    }

    /// Kanten des Hintergrunds auf der Arbeitsebene: fangbar, nicht wählbar
    /// (E16). Leer, solange das EG aktiv ist.
    pub fn background_snaps(&mut self) -> Vec<(Vec3, Vec3)> {
        let floor = self.work_plane().0;
        let memo = UnderMemo::default();
        let cuts: Vec<(RunId, f64)> = self
            .cache
            .iter()
            .flatten()
            .filter_map(|c| Some((c.id, self.under_cut(c.id, &memo)?)))
            .collect();
        let filter = self.filtering();
        let vis = Vis {
            model: &self.model,
            anim: self.vis_anim.as_ref(),
            table: &self.vis_table,
            stamp: self.vis_stamp(),
        };
        let mut out = Vec::new();
        for (run, cut) in cuts {
            if let Some(Some(c)) = self.cache.get_mut(run.index() as usize) {
                let at = |p: Vec3| vec3(p.x, p.y, floor);
                // Kein Fang an Ausgeblendetem oder Blassem (§3.5)
                out.extend(
                    c.under_edges(cut)
                        .iter()
                        .filter(|e| !filter || vis.class(run, e.elem, e.layer) == Class::Solid)
                        .map(|e| (at(e.a), at(e.b))),
                );
            }
        }
        out
    }

    /// Netz einzelner Wandzüge (Live-Netz beim Ziehen): nur das Deckende.
    pub fn mesh_runs(
        &mut self,
        view: ViewKind,
        section: Option<Plane>,
        runs: &[RunId],
    ) -> MeshData {
        self.sicht_fuer(view);
        let [m, _] = self.mesh_split(view, section, runs);
        m
    }

    /// Wirkt gerade eine Sichtbarkeit auf die Netze (Ausgeblendetes,
    /// Isolieren oder ein Übergang)?
    pub(super) fn filtering(&self) -> bool {
        !self.model.visibility().is_plain() || self.vis_anim.is_some()
    }

    pub(super) fn vis_stamp(&self) -> VisStamp {
        (self.model.revision(), self.vis_rev, self.vis_anim.is_some())
    }

    /// Netze einzelner Wandzüge: deckend und blass (Paket 3). Gefiltert wird
    /// je Körper vor dem Verschmelzen gestapelter Züge (Review 3a G1).
    pub(super) fn mesh_split(
        &mut self,
        view: ViewKind,
        section: Option<Plane>,
        runs: &[RunId],
    ) -> [MeshData; 2] {
        let cut = self.plan_cut();
        let filter = self.filtering();
        let stamp = self.vis_stamp();
        // OK Gelände je Zug: aus dem Aufmaß, sonst das seines Gebäudes
        // (Entscheidung B)
        let raster = self.terrain_grid();
        let model = &self.model;
        let ground = |r: RunId| Boden {
            raster: raster.as_deref(),
            eben: model.terrain_z_of(model.run(r).and_then(|x| model.building_of(x.storey))),
        };
        let grounds: Vec<Boden> = runs.iter().map(|r| ground(*r)).collect();
        let mut m = [MeshData::default(), MeshData::default()];
        if view == ViewKind::Plan {
            let active = self.active_storey();
            let modes: Vec<PlanMode> = runs.iter().map(|r| self.plan_mode(*r, active)).collect();
            // Hintergrund: Wandschnitt des Geschosses direkt darunter in
            // seiner eigenen Schnitthöhe, nur Konturen, knapp über dem Boden
            // des aktiven Geschosses (unter dessen Wänden)
            let floor = self.work_plane().0;
            let memo = UnderMemo::default();
            let under: Vec<Option<f64>> = runs.iter().map(|r| self.under_cut(*r, &memo)).collect();
            let vis = Vis {
                model: &self.model,
                anim: self.vis_anim.as_ref(),
                table: &self.vis_table,
                stamp,
            };
            for (i, &run) in runs.iter().enumerate() {
                if let Some(Some(c)) = self.cache.get_mut(run.index() as usize) {
                    if c.id == run {
                        c.view_solid(view, section, cut, modes[i]);
                        if let Some(s) = c.plan_at(cut, modes[i]) {
                            if filter {
                                let key = SplitKey::Plan(cut, modes[i]);
                                let parts = c.split(key, s, &vis);
                                for (k, x) in parts.iter().enumerate() {
                                    mesh_into(&mut m[k], x, grounds[i]);
                                }
                            } else {
                                mesh_into(&mut m[0], s, grounds[i]);
                            }
                        }
                        if let Some(bcut) = under[i] {
                            let z = (floor + BACKGROUND_LIFT) as f32;
                            for e in c.under_edges(bcut) {
                                let k = if filter {
                                    match vis.class(run, e.elem, e.layer) {
                                        Class::Solid => 0,
                                        Class::Ghost => 1,
                                        Class::Hidden => continue,
                                    }
                                } else {
                                    0
                                };
                                let (mut a, mut b) = (e.a.to_f32(), e.b.to_f32());
                                (a[2], b[2]) = (z, z);
                                m[k].edges.push(([a, b], edge_kind::BACKGROUND as f32));
                            }
                        }
                    }
                }
            }
            return m;
        }
        // Gestapelte Züge: die Schale läuft ohne waagerechte Naht durch (G6,
        // E16); die Partner stehen dafür auch berechnet bereit, gezeichnet
        // wird nur `runs`
        let mut above: Vec<(RunId, RunId)> = Vec::new();
        for c in self.cache.iter().flatten() {
            if let Some(b) = c.below {
                above.push((b, c.id));
            }
        }
        let partners: Vec<RunId> = runs
            .iter()
            .flat_map(|&r| {
                let below = self.cached(r).and_then(|c| c.below);
                above
                    .iter()
                    .filter(move |(b, _)| *b == r)
                    .map(|(_, u)| *u)
                    .chain(below)
            })
            .collect();
        for &run in runs.iter().chain(&partners) {
            if let Some(Some(c)) = self.cache.get_mut(run.index() as usize) {
                if c.id == run {
                    c.view_solid(view, section, cut, PlanMode::Cut);
                }
            }
        }
        // Je Zug deckend und blass, vor dem Verschmelzen (G1)
        let vis = Vis {
            model: &self.model,
            anim: self.vis_anim.as_ref(),
            table: &self.vis_table,
            stamp,
        };
        let mut parts: HashMap<RunId, Rc<[Solid; 2]>> = HashMap::new();
        if filter {
            let key = match section {
                Some(pl) if view == ViewKind::Section => SplitKey::Section(pl),
                _ => SplitKey::Solid,
            };
            for &run in runs.iter().chain(&partners) {
                if parts.contains_key(&run) {
                    continue;
                }
                if let Some(c) = self.cached(run) {
                    if let Some(s) = c.shown(view, section) {
                        parts.insert(run, c.split(key, s, &vis));
                    }
                }
            }
        }
        let classes = if filter { 2 } else { 1 };
        for (&run, &ground) in runs.iter().zip(&grounds) {
            let Some(c) = self.cached(run) else {
                continue;
            };
            let Some(whole) = c.shown(view, section) else {
                continue;
            };
            for (k, mk) in m.iter_mut().enumerate().take(classes) {
                let pick = |r: RunId, s: &'_ Solid| -> Option<Solid> {
                    if filter {
                        parts.get(&r).map(|p| p[k].clone())
                    } else {
                        Some(s.clone())
                    }
                };
                let lower = c
                    .below
                    .and_then(|b| self.cached(b))
                    .and_then(|b| b.shown(view, section).map(|s| (b.id, s)))
                    .and_then(|(b, s)| pick(b, s));
                let uppers: Vec<(f64, Solid)> = above
                    .iter()
                    .filter(|(b, _)| *b == run)
                    .filter_map(|(_, u)| self.cached(*u))
                    .filter_map(|u| Some((u.chain.base, pick(u.id, u.shown(view, section)?)?)))
                    .collect();
                let own: &Solid = if filter {
                    match parts.get(&run) {
                        Some(p) => &p[k],
                        None => continue,
                    }
                } else {
                    whole
                };
                if lower.is_none() && uppers.is_empty() {
                    mesh_into(mk, own, ground);
                    continue;
                }
                let mut x = own.clone();
                if let Some(mut l) = lower {
                    merge_seam(&mut l, &mut x, c.chain.base);
                }
                for (z, mut u) in uppers {
                    merge_seam(&mut x, &mut u, z);
                }
                mesh_into(mk, &x, ground);
            }
        }
        m
    }

    /// Wie oft ein Erweiterungsbauteil gerechnet wurde (Tests).
    #[cfg(test)]
    pub fn ext_builds(&self) -> u64 {
        self.ext.builds
    }

    /// Erweiterungsbauteile ins Netz, deckend oder blass (E4). Grundriss:
    /// die des aktiven Geschosses in seiner Schnitthöhe.
    pub(super) fn ext_into(
        &mut self,
        m: &mut [MeshData; 2],
        view: ViewKind,
        section: Option<Plane>,
    ) {
        let Some((plan, section)) = self.ext_view(view, section) else {
            return;
        };
        let filter = self.filtering();
        let raster = self.terrain_grid();
        let model = &self.model;
        let ground = |id: ElementId| Boden {
            raster: raster.as_deref(),
            eben: model.terrain_z_of(model.building_of_element(id)),
        };
        let vis = Vis {
            model: &self.model,
            anim: self.vis_anim.as_ref(),
            table: &self.vis_table,
            stamp: self.vis_stamp(),
        };
        for (id, s) in self.ext.shown(plan, section) {
            let k = if filter {
                match class_in(vis.element_masks(id), sk_model::NO_LAYER) {
                    Class::Solid => 0,
                    Class::Ghost => 1,
                    Class::Hidden => continue,
                }
            } else {
                0
            };
            mesh_into(&mut m[k], s, ground(id));
        }
    }

    /// Blasses Netz (Isolieren, Übergänge) zur Ansicht wie [`Scene::mesh`];
    /// leer, wenn nichts blass ist.
    pub fn ghost_mesh(
        &mut self,
        view: ViewKind,
        section: Option<Plane>,
        except: &[RunId],
    ) -> MeshData {
        self.sicht_fuer(view);
        if !self.filtering() {
            return MeshData::default();
        }
        let key = (view, section, except.to_vec());
        let stamp = self.vis_stamp();
        if let Some((k, st, g)) = &self.ghost {
            if *k == key && *st == stamp {
                return g.clone();
            }
        }
        let runs = self.mesh_keys(&key);
        let [_, g] = self.mesh_ruhend(view, section, &runs);
        g
    }

    /// Zwischengespeicherter 3D-Körper eines Zugs (mit Stempeln an Dreieck
    /// und Kante).
    #[cfg(test)]
    pub fn run_solid(&mut self, run: RunId) -> Option<&Solid> {
        self.rebuild_dirty(false);
        self.cached(run).map(|c| &c.solid)
    }

    // --- Sichtbarkeit (Paket 3) -------------------------------------------
}
