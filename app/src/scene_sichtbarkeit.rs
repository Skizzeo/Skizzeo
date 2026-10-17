//! Szene, Teil „Sichtbarkeit, Strahl und Picken“ (aus scene.rs, Paket T2).

use super::*;

impl Scene {
    /// Ändert die Sichtbarkeit sofort: kein Schritt, keine Revision, Mengen
    /// bleiben; die Schnelloption der Ansicht bleibt. `false`, wenn sich
    /// nichts ändert.
    pub fn set_visibility(&mut self, v: Visibility) -> bool {
        let view_cat = self.model.visibility().view_cat.clone();
        self.set_vis(Visibility { view_cat, ..v })
    }

    pub(super) fn set_vis(&mut self, v: Visibility) -> bool {
        if *self.model.visibility() == v {
            return false;
        }
        self.model.set_visibility(v);
        self.vis_rev += 1;
        self.vis_anim = None;
        true
    }

    /// Wie [`Scene::set_visibility`], mit Übergang in `anim_ms` (§2):
    /// Ausgeblendetes wird durchsichtig, Erscheinendes deckend, beim
    /// Isolieren wird der Rest blass bzw. wieder deckend.
    #[cfg_attr(not(test), allow(dead_code))] // Baumpanel (Paket 4)
    pub fn fade_visibility(&mut self, v: Visibility) -> bool {
        let view_cat = self.model.visibility().view_cat.clone();
        self.fade_vis(Visibility { view_cat, ..v })
    }

    pub(super) fn fade_vis(&mut self, v: Visibility) -> bool {
        let old = self.model.visibility().clone();
        if !self.set_vis(v) {
            return false;
        }
        let new = self.model.visibility();
        let kind = match (&old.isolate, &new.isolate) {
            (None, Some(_)) => Some(VisFade::IsolateOn),
            (Some(_), None) => Some(VisFade::IsolateOff),
            (None, None) => {
                let (mut out, mut inn) = (false, false);
                for (id, _) in self.model.elements().iter() {
                    let (o, n) = (self.model.masks_in(&old, id), self.model.masks(id));
                    out |= o.solid & !n.solid != 0;
                    inn |= n.solid & !o.solid != 0;
                }
                match (out, inn) {
                    (true, false) => Some(VisFade::Out),
                    (false, true) => Some(VisFade::In),
                    _ => None,
                }
            }
            (Some(_), Some(_)) => None,
        };
        if self.theme.size.anim_ms > 0.0 {
            self.vis_anim = kind.map(|kind| VisAnim {
                kind,
                old,
                start: self.now,
            });
        }
        true
    }

    /// Läuft ein Übergang der Sichtbarkeit?
    pub fn vis_animating(&self) -> bool {
        self.vis_anim.is_some()
    }

    /// Stellt die Uhr; `true`, wenn der Übergang endet und die Netze neu
    /// zusammengesetzt werden müssen.
    pub fn vis_tick(&mut self, now: u64) -> bool {
        self.now = now;
        let ms = self.theme.size.anim_ms;
        match &self.vis_anim {
            Some(a) if ms <= 0.0 || now.saturating_sub(a.start) as f32 >= ms => {
                self.vis_anim = None;
                true
            }
            _ => false,
        }
    }

    /// Endet ein laufender Übergang sofort (Klick, wie beim Wachsen)?
    pub fn skip_vis_animation(&mut self) -> bool {
        self.vis_anim.take().is_some()
    }

    /// Deckkraft des blassen Netzes in diesem Bild: beim Isolieren
    /// `ghost_alpha_3d` bzw. `ghost_alpha_paper` (Zeichnung), im Übergang
    /// dazwischen (ease-out).
    pub fn ghost_alpha(&self, paper: bool) -> f32 {
        let size = &self.theme.size;
        let g = if paper {
            size.ghost_alpha_paper
        } else {
            size.ghost_alpha_3d
        };
        let Some(a) = &self.vis_anim else {
            return if self.model.visibility().isolate.is_some() {
                g
            } else {
                0.0
            };
        };
        let k = ease_out(self.now.saturating_sub(a.start) as f32 / size.anim_ms.max(1.0));
        match a.kind {
            VisFade::Out => 1.0 - k,
            VisFade::In => k,
            VisFade::IsolateOn => 1.0 + (g - 1.0) * k,
            VisFade::IsolateOff => g + (1.0 - g) * k,
        }
    }

    /// Ist das Bauteil zu sehen (nicht ganz ausgeblendet)? Blasses zählt.
    pub fn visible(&self, id: ElementId) -> bool {
        self.model.visibility().is_plain() || self.model.shown(id).0 != sk_model::view::Shown::None
    }

    /// Ist ein Bauteil anklickbar und fangbar (§3.5): nicht ausgeblendet,
    /// beim Isolieren nur Isoliertes?
    pub fn pickable(&self, id: ElementId) -> bool {
        self.model.visibility().is_plain() || self.model.masks(id).solid != 0
    }

    /// Bauteil, an dem der letzte Schritt wegen einer Sperre scheiterte
    /// (einmal abholen).
    pub fn take_locked(&mut self) -> Option<ElementId> {
        self.locked_hit.take()
    }

    /// Ist das Gelände ausgeblendet?
    pub fn terrain_hidden(&self) -> bool {
        self.model.visibility().terrain_hidden
    }

    /// Ist der Ast isoliert?
    #[cfg_attr(not(test), allow(dead_code))] // Baumpanel (Paket 4)
    pub fn isolating(&self) -> Option<&Isolate> {
        self.model.visibility().isolate.as_ref()
    }

    /// Umschließender Quader des Modells.
    pub fn bounds(&self) -> Option<Aabb> {
        self.bounds
    }

    /// Umschließender Quader des Körpers eines Bauteils (`None`: ohne
    /// Körper), aus den Netzen der Wandzüge; rechnet veraltete Züge vorher
    /// neu. Für Tests, später auch für „auf Bauteil zoomen“ im Baum.
    #[cfg(test)]
    pub fn element_bounds(&mut self, id: ElementId) -> Option<Aabb> {
        self.rebuild_dirty(false);
        let mut out: Option<Aabb> = None;
        for c in self.cache.iter().flatten() {
            for t in &c.solid.triangles {
                if self.model.part_of(c.id, t.elem) != Some(id) {
                    continue;
                }
                for p in t.p {
                    out = Some(match out {
                        None => (p, p),
                        Some((lo, hi)) => (
                            vec3(lo.x.min(p.x), lo.y.min(p.y), lo.z.min(p.z)),
                            vec3(hi.x.max(p.x), hi.y.max(p.y), hi.z.max(p.z)),
                        ),
                    });
                }
            }
        }
        out
    }

    /// Nächster Treffer eines Strahls: Abstand und getroffene Wand.
    pub fn raycast(&self, origin: Vec3, dir: Vec3) -> Option<(f64, ElementId)> {
        let filter = self.filtering();
        let vis = Vis {
            model: &self.model,
            anim: self.vis_anim.as_ref(),
            table: &self.vis_table,
            stamp: self.vis_stamp(),
        };
        let mut best: Option<(f64, RunId, u32)> = None;
        let plaetze = self.plaetze_am_strahl(origin, dir, 0.0);
        for c in plaetze.iter().filter_map(|&i| self.cache.get(i)?.as_ref()) {
            if !c.bounds.is_some_and(|b| ray_hits_box(origin, dir, b)) {
                continue;
            }
            // Nur Deckendes ist anklickbar (§3.5): nichts Ausgeblendetes,
            // beim Isolieren nichts Blasses
            let run = c.id;
            let keep = |t: &Tri| !filter || vis.class(run, t.elem, t.layer) == Class::Solid;
            if let Some((t, seg)) = c.solid.raycast_where(origin, dir, keep) {
                if best.is_none_or(|b| t < b.0) {
                    best = Some((t, c.id, seg));
                }
            }
        }
        let (t, run, seg) = best?;
        Some((t, self.model.part_of(run, seg)?))
    }

    /// Bauteil unter einem Strahl, so wie die Ansicht es zeigt (Grundriss:
    /// waagerecht geschnitten, Schnitt: nur hinter der Ebene).
    pub fn pick(
        &mut self,
        view: ViewKind,
        section: Option<Plane>,
        origin: Vec3,
        dir: Vec3,
    ) -> Option<ElementId> {
        self.sicht_fuer(view);
        let cut = self.plan_cut();
        let active = self.active_storey();
        // Nur Züge nahe dem Strahl (Review L26)
        let plaetze = self.plaetze_am_strahl(origin, dir, 0.0);
        let modes: Vec<(RunId, PlanMode)> = plaetze
            .iter()
            .filter_map(|&i| self.cache.get(i)?.as_ref())
            .map(|c| (c.id, self.plan_mode(c.id, active)))
            .collect();
        let filter = self.filtering();
        let vis = Vis {
            model: &self.model,
            anim: self.vis_anim.as_ref(),
            table: &self.vis_table,
            stamp: self.vis_stamp(),
        };
        let mut best: Option<(f64, RunId, u32)> = None;
        for &i in &plaetze {
            let Some(Some(c)) = self.cache.get_mut(i) else {
                continue;
            };
            if !c.bounds.is_some_and(|b| ray_hits_box(origin, dir, b)) {
                continue;
            }
            let id = c.id;
            let keep = |t: &Tri| !filter || vis.class(id, t.elem, t.layer) == Class::Solid;
            if let Some((t, seg)) = c
                .view_solid(
                    view,
                    section,
                    cut,
                    modes
                        .iter()
                        .find(|m| m.0 == id)
                        .map_or(PlanMode::Cut, |m| m.1),
                )
                .and_then(|s| s.raycast_where(origin, dir, keep))
            {
                if best.is_none_or(|b| t < b.0) {
                    best = Some((t, id, seg));
                }
            }
        }
        // Erweiterungsbauteile (E4): Treffer vor dem Wandzug, wenn näher
        let ext = self.ext_view(view, section).and_then(|(plan, sec)| {
            let vis = Vis {
                model: &self.model,
                anim: self.vis_anim.as_ref(),
                table: &self.vis_table,
                stamp: self.vis_stamp(),
            };
            let keep =
                |id| !filter || class_in(vis.element_masks(id), sk_model::NO_LAYER) == Class::Solid;
            self.ext.pick(plan, sec, origin, dir, keep, |b| {
                ray_hits_box(origin, dir, b)
            })
        });
        match (best, ext) {
            (Some(b), Some(e)) if e.0 < b.0 => Some(e.1),
            (None, Some(e)) => Some(e.1),
            (Some((_, run, seg)), _) => self.model.part_of(run, seg),
            (None, None) => None,
        }
    }

    /// Mitte des umschließenden Quaders aller Flächen.
    pub fn center(&self) -> Option<Vec3> {
        self.bounds().map(|(lo, hi)| (lo + hi) * 0.5)
    }
}
