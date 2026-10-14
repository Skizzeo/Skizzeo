//! Gestapelte Züge (OG Phase 2, W-12): Methoden von `Model`, aus
//! `model.rs` ausgelagert. Ein Zug über einem anderen folgt ihm Segment für
//! Segment mit einem Versatz je Wand (Regel 28); springt ein Teilstück an
//! einer Fuge allein zurück, steht dort eine Stufe ohne Partner darunter.

use super::*;

/// Neue Lage eines gestapelten Zuges nach [`Model::offset_plan`].
struct OffsetPlan {
    run: RunId,
    lsegs: Vec<ElementId>,
    offsets: Vec<f64>,
    free: Vec<bool>,
    chain: WallChain,
    map: Vec<Option<usize>>,
}

impl Model {
    /// Führt die gekoppelten Züge über `root` mit (Linie = Partnerlinie +
    /// Versatz, Ecken neu geschnitten), Geschoss für Geschoss nach oben, und
    /// ordnet die Kopplungen neu zu, wenn sich die Segmentzahl geändert hat.
    /// Liefert die bewegten Züge.
    pub(super) fn carry_stack(&mut self, root: RunId) -> Vec<RunId> {
        let mut out = Vec::new();
        let mut queue = vec![root];
        let mut k = 0;
        while k < queue.len() {
            let below = queue[k];
            k += 1;
            for up in self.runs_above(below) {
                if queue.contains(&up) {
                    continue;
                }
                self.follow_below(up, below);
                queue.push(up);
                out.push(up);
            }
        }
        out
    }

    /// Je Segment `k` des Zuges darunter (Wände `lsegs`) die Wand des Zuges
    /// `up`, die darauf steht (Index in `up`); `None`, wenn keine.
    pub(super) fn partners(&self, up: RunId, lsegs: &[ElementId]) -> Vec<Option<usize>> {
        let usegs = self.run(up).map(|r| r.segments.clone()).unwrap_or_default();
        lsegs
            .iter()
            .map(|e| usegs.iter().position(|w| self.wall_below(*w) == Some(*e)))
            .collect()
    }

    /// Versatz der Außenfläche des Zuges `up`: Daran stehen seine Stufen
    /// auf der Fuge (W-12, Geometrie #69).
    fn step_face(&self, up: RunId) -> f64 {
        self.base_chain(up).map_or(0.0, |c| c.outer_offset())
    }

    /// Je Segment des gestapelten Zuges `up` das Segment des Zuges darunter,
    /// auf dem es steht (über die Kopplung); `None` für eine Stufe (W-12)
    /// oder ohne Zug darunter.
    pub fn stack_map(&self, up: RunId) -> Vec<Option<usize>> {
        let Some(r) = self.run(up) else {
            return Vec::new();
        };
        let lsegs = self
            .run_below(up)
            .and_then(|b| self.run(b))
            .map(|b| b.segments.clone())
            .unwrap_or_default();
        r.segments
            .iter()
            .map(|w| {
                let b = self.wall_below(*w)?;
                lsegs.iter().position(|x| *x == b)
            })
            .collect()
    }

    /// Fuge unter der Stufe `wall` (W-12): Zug darunter und das Segment
    /// darin, das an ihr beginnt. Zieht man die Stufe, wandert diese Fuge
    /// längs der Flucht und die Stufe mit ihr. `None` für jede andere Wand.
    pub fn step_joint(&self, wall: ElementId) -> Option<(RunId, usize)> {
        let (up, s) = self.segment_of(wall)?;
        let map = self.stack_map(up);
        if map.get(s)?.is_some() {
            return None;
        }
        let next = map[(s + 1) % map.len()]?;
        let below = self.run_below(up)?;
        self.base_chain(below)?
            .straight_joints()
            .contains(&next)
            .then_some((below, next))
    }

    /// Versatz je Segment des Zuges darunter (Geometrie `lower`, Wände
    /// `lsegs`) für den Zug `up`, und welche Segmente gelöst sind (ohne
    /// Kopplung, OG Phase 2). Gekoppelte Segmente behalten ihren
    /// gespeicherten Versatz; gelöste behalten ihre Linie, ihr Versatz wird
    /// aus der eigenen Lage gegen `lower` gemessen (G7 K1). Stufen oben
    /// zählen nicht (W-12). Fehlt einem Segment darunter der Partner (die
    /// Segmentzahl hat sich geändert), siehe [`Model::recount_offsets`].
    pub(super) fn stack_offsets(
        &self,
        up: RunId,
        lower: &WallChain,
        lsegs: &[ElementId],
    ) -> (Vec<f64>, Vec<bool>) {
        let usegs = self.run(up).map(|r| r.segments.clone()).unwrap_or_default();
        let partners = self.partners(up, lsegs);
        let Some(partners) = partners.into_iter().collect::<Option<Vec<usize>>>() else {
            let l: Vec<Option<ElementId>> = lsegs.iter().copied().map(Some).collect();
            return self.recount_offsets(&usegs, lower, &l);
        };
        let upc = self.base_chain(up);
        partners
            .iter()
            .enumerate()
            .map(|(k, &j)| {
                let (o, linked) = self.stack_offset(usegs[j]).unwrap_or((0.0, true));
                if linked {
                    return (o, false);
                }
                let m = upc.as_ref().and_then(|c| c.offset_against(j, lower, k));
                (m.unwrap_or(0.0), true)
            })
            .unzip()
    }

    /// Versatz und Zustand je Segment von `lower`, nachdem sich seine
    /// Segmentzahl geändert hat (Regel 28): Ein Segment, dessen Wand schon
    /// einen Partner oben hat, behält dessen Versatz und Zustand; ein neues
    /// Teilstück erbt beides vom geraden Nachbarn, aus dem es geteilt wurde.
    /// Sonst bündig und gekoppelt.
    pub(super) fn recount_offsets(
        &self,
        usegs: &[ElementId],
        lower: &WallChain,
        lsegs: &[Option<ElementId>],
    ) -> (Vec<f64>, Vec<bool>) {
        let partner = |e: Option<ElementId>| {
            usegs
                .iter()
                .find_map(|w| match self.element(*w).map(|x| &x.kind) {
                    Some(ElementKind::Wall(Wall {
                        coupling: Some(c), ..
                    })) if Some(c.below) == e => Some((c.offset, !c.linked)),
                    _ => None,
                })
        };
        let n = lsegs.len();
        let normal = |k: usize| lower.segment_normal(k);
        let straight = |a: usize, b: usize| match (normal(a), normal(b)) {
            (Some(x), Some(y)) => x.dot(y) > 1.0 - 1e-9,
            _ => false,
        };
        let closed = lower.closed;
        let (mut offsets, mut free) = (vec![0.0; n], vec![false; n]);
        for k in 0..n {
            let own = partner(lsegs[k]);
            // Sonst das nächste Stück derselben Flucht mit Partner, über
            // gerade Fugen in beide Richtungen (beim Teilen an zwei Punkten
            // liegt ein Stück zwei Fugen weit, Review Geometrie #61)
            let near = || {
                let step = |j: usize, back: bool| {
                    let i = match back {
                        true => (j > 0 || closed).then(|| (j + n - 1) % n)?,
                        false => (j + 1 < n || closed).then(|| (j + 1) % n)?,
                    };
                    (i != k && straight(i, j)).then_some(i)
                };
                let (mut prev, mut next) = (Some(k), Some(k));
                while prev.is_some() || next.is_some() {
                    prev = prev.and_then(|j| step(j, true));
                    next = next.and_then(|j| step(j, false));
                    let found = [prev, next]
                        .into_iter()
                        .flatten()
                        .find_map(|j| partner(lsegs[j]));
                    if found.is_some() {
                        return found;
                    }
                }
                None
            };
            if let Some((o, f)) = own.or_else(near) {
                offsets[k] = o;
                free[k] = f;
            }
        }
        (offsets, free)
    }

    /// Lassen sich alle über `id` gestapelten Züge auf die neuen Punkte
    /// `points` von `id` legen (G7 K2)? Nein, wenn ein Segment verschwinden
    /// oder kippen würde oder sich ein Umriss selbst schneidet; dann bleibt
    /// das Gummiband stehen statt eine Decke zu verlieren.
    pub(super) fn stack_fits(&self, id: RunId, points: &[Vec3]) -> bool {
        let Some(mut lower) = self.base_chain(id) else {
            return true;
        };
        // Keine Gehrung kippt neu (dieselbe Regel wie in check()); ein Zug,
        // der schon kippt, darf weiter bewegt werden (H-160d)
        let kippte = lower.koerper_kippt().is_empty();
        lower.points = points.to_vec();
        if kippte && !lower.koerper_kippt().is_empty() {
            return false;
        }
        let Some(r) = self.run(id) else {
            return true;
        };
        // Die Wände nach Lage zuordnen wie beim Setzen
        // (Regel 11), neue Stücke ohne Partner (Regel 28)
        let new = segment_lines(&lower.clean_points(), lower.closed);
        let lsegs: Vec<Option<ElementId>> =
            teilen::assign_walls(&segment_lines(&r.points, r.closed), &new, lower.closed)
                .into_iter()
                .map(|m| m.and_then(|o| r.segments.get(o).copied()))
                .collect();
        for up in self.runs_above(id) {
            let (offsets, _) = match lsegs.iter().copied().collect::<Option<Vec<_>>>() {
                Some(l) => self.stack_offsets(up, &lower, &l),
                None => {
                    let usegs = self.run(up).map(|r| r.segments.clone()).unwrap_or_default();
                    self.recount_offsets(&usegs, &lower, &lsegs)
                }
            };
            match lower.with_segment_offsets_stepped(&offsets, self.step_face(up)) {
                Some((c, map))
                    if room_inside(&c)
                        && self.lengths_fit(up, &c, &lower, &map)
                        && self.stack_fits(up, &c.points) => {}
                _ => return false,
            }
        }
        true
    }

    /// Regel 30: Ist jedes Segment von `c` (neue Lage des Zuges `up`, auf
    /// `lower` nach `map`) mindestens min(Wanddicke, Länge des Partners in
    /// `lower`) lang? Sonst bliebe nach dem Versetzen ein Wandstummel, den
    /// die Prüfung meldet. Stummel, die schon der Zug darunter hat, bleiben
    /// erlaubt; Stufen (W-12) sind so lang wie der Unterschied im Versatz.
    pub(super) fn lengths_fit(
        &self,
        up: RunId,
        c: &WallChain,
        lower: &WallChain,
        map: &[Option<usize>],
    ) -> bool {
        let Some(r) = self.run(up) else {
            return true;
        };
        let (Some((cp, _, _)), Some((lp, _, _))) = (c.layout(), lower.layout()) else {
            return true;
        };
        let (n, nl) = (cp.len(), lp.len());
        let same = r.segments.len() == map.len();
        map.iter().enumerate().all(|(j, k)| {
            let Some(k) = *k else {
                return true;
            };
            if !c.closed && j + 1 >= n {
                return true;
            }
            let len = (cp[(j + 1) % n] - cp[j]).length();
            let partner = (lp[(k + 1) % nl] - lp[k]).length();
            let wall = if same {
                r.segments.get(j)
            } else {
                r.segments.first()
            };
            let dicke = wall
                .and_then(|w| self.element(*w))
                .and_then(|e| e.layer_set)
                .and_then(|t| self.layer_sets.get(t))
                .map_or(0.0, |t| t.thickness());
            len < 1.0 || len + 0.5 >= dicke.min(partner)
        })
    }

    /// Stapelbezug einer gestapelten Wand: (Versatz in mm, + außen;
    /// gekoppelt). `None`: keine gestapelte Wand (EG, Innenwand).
    pub fn stack_offset(&self, wall: ElementId) -> Option<(f64, bool)> {
        match self.element(wall)?.kind {
            ElementKind::Wall(Wall {
                coupling: Some(c), ..
            }) => Some((c.offset, c.linked)),
            _ => None,
        }
    }

    /// Wand im Geschoss darunter, auf der `wall` steht.
    pub fn wall_below(&self, wall: ElementId) -> Option<ElementId> {
        match self.element(wall)?.kind {
            ElementKind::Wall(Wall {
                coupling: Some(c), ..
            }) => Some(c.below),
            _ => None,
        }
    }

    /// Fuß der Kette: von `wall` über gekoppelte Glieder nach unten bis zum
    /// ersten gelösten Glied oder zum EG. Ziehen ohne Strg an einer
    /// gekoppelten OG-Wand zieht diese Wand (OG Phase 2).
    pub fn chain_foot(&self, wall: ElementId) -> ElementId {
        let mut at = wall;
        while let Some((_, true)) = self.stack_offset(at) {
            match self.wall_below(at) {
                Some(b) if b != wall => at = b,
                _ => break,
            }
        }
        at
    }

    /// Kette am Segment öffnen bzw. schließen. Die Lage bleibt, der Versatz
    /// auch (OG-16). `false` bei einer Wand ohne Partner darunter.
    pub fn set_linked(&mut self, wall: ElementId, linked: bool) -> bool {
        let Some((_, now)) = self.stack_offset(wall) else {
            return false;
        };
        // Die Aufkantung bleibt bündig gekoppelt (Flachdach, Jörn 10.10.)
        if !linked && self.is_parapet(wall) {
            return false;
        }
        if now == linked {
            return true;
        }
        note!(self, Element, self.elements, wall);
        if let Some(ElementKind::Wall(Wall {
            coupling: Some(c), ..
        })) = self.elements.get_mut(wall).map(|e| &mut e.kind)
        {
            c.linked = linked;
        }
        if let Some((run, _)) = self.segment_of(wall) {
            self.sync_parts(run);
            self.update_joins(&[run]);
        }
        true
    }

    /// Versatz der gestapelten Wand `wall` auf `offset` (mm, + außen) setzen;
    /// unter 20 mm rastet er auf 0 ein (Regel 31). Nur diese Wand und was
    /// über ihr steht bewegt sich, der Zustand der Kette bleibt. `None`, wenn
    /// der Stapel dabei ungültig würde (Segment verschwindet, Umriss kreuzt
    /// sich, kein Raum, ein Erker über nur einen Teil der Wand).
    pub fn set_offset(&mut self, wall: ElementId, offset: f64) -> Option<Vec<RunId>> {
        let offset = if offset.abs() < MIN_OFFSET {
            0.0
        } else {
            offset
        };
        if offset != 0.0 && self.is_parapet(wall) {
            return None;
        }
        let (old, _) = self.stack_offset(wall)?;
        let p = self.offset_plan(wall, offset)?;
        if (old - offset).abs() < 1e-9 {
            return Some(Vec::new());
        }
        let run = p.run;
        let out = self.set_run_points_with(run, &p.chain.points, |m| {
            m.couple(run, &p.lsegs, &p.map, &p.offsets, &p.free);
        })?;
        self.sync_parts(run);
        self.update_joins(&[run]);
        Some(out)
    }

    /// Neue Lage des Zuges der gestapelten Wand `wall`, wenn sie den Versatz
    /// `offset` bekommt; `None`, wenn der Zug dabei ungültig würde. Ein
    /// Teilstück allein zurück bekommt an der Fuge eine Stufe (W-12).
    fn offset_plan(&self, wall: ElementId, offset: f64) -> Option<OffsetPlan> {
        let (run, _) = self.segment_of(wall)?;
        let below = self.run_below(run)?;
        let lower = self.base_chain(below)?;
        let lsegs = self.run(below)?.segments.clone();
        let usegs = self.run(run)?.segments.clone();
        let partners: Vec<usize> = self
            .partners(run, &lsegs)
            .into_iter()
            .collect::<Option<_>>()?;
        let seg = lsegs
            .iter()
            .position(|e| Some(*e) == self.wall_below(wall))?;
        if !offset.is_finite() {
            return None;
        }
        let state = |j: usize| self.stack_offset(usegs[j]).unwrap_or((0.0, true));
        let mut offsets: Vec<f64> = partners.iter().map(|&j| state(j).0).collect();
        let free: Vec<bool> = partners.iter().map(|&j| !state(j).1).collect();
        offsets[seg] = offset;
        let (chain, map) = lower.with_segment_offsets_stepped(&offsets, self.step_face(run))?;
        if !room_inside(&chain) || !self.lengths_fit(run, &chain, &lower, &map) {
            return None;
        }
        Some(OffsetPlan {
            run,
            lsegs,
            offsets,
            free,
            chain,
            map,
        })
    }

    /// Versatz um `d` ändern ([`Model::set_offset`]).
    pub fn move_segment(&mut self, wall: ElementId, d: f64) -> Option<Vec<RunId>> {
        let (old, _) = self.stack_offset(wall)?;
        self.set_offset(wall, old + d)
    }

    /// „Bündig setzen“: Versatz 0 und gekoppelt.
    pub fn set_flush(&mut self, wall: ElementId) -> bool {
        self.set_offset(wall, 0.0).is_some() && self.set_linked(wall, true)
    }

    /// „Bündig setzen“ mit Zielwand (Jörn 07.10. 07:53): `wall` rückt an
    /// `target`, danach ist das Paar gekoppelt. Steht `target` unter `wall`,
    /// ist das [`Model::set_flush`]. Steht es darüber, wandert das Segment
    /// des Zuges darunter wie beim Ziehen am Fuß (Gummiband): Gründung,
    /// Decke und die gekoppelten Wände darüber gehen mit, gelöste bleiben
    /// stehen; die Zielwand selbst bleibt, wo sie ist. Ändert nichts, wenn
    /// es abgelehnt wird.
    pub fn flush_to(
        &mut self,
        wall: ElementId,
        target: ElementId,
    ) -> Result<Vec<RunId>, FlushError> {
        if self.wall_below(wall) == Some(target) {
            return if self.stack_offset(wall) == Some((0.0, true)) {
                Ok(Vec::new())
            } else if self.set_offset(wall, 0.0).is_some() && self.set_linked(wall, true) {
                Ok(self.segment_of(wall).map(|s| vec![s.0]).unwrap_or_default())
            } else {
                Err(FlushError::Invalid)
            };
        }
        let (run, pts) = self.flush_up_points(wall, target)?;
        let (d, linked) = self.stack_offset(target).ok_or(FlushError::NotPartners)?;
        if d == 0.0 {
            if !linked {
                self.set_linked(target, true);
            }
            return Ok(Vec::new());
        }
        if linked {
            self.set_linked(target, false);
        }
        let Some(out) = self.set_run_points(run, &pts) else {
            if linked {
                self.set_linked(target, true);
            }
            return Err(FlushError::Invalid);
        };
        self.set_linked(target, true);
        Ok(out)
    }

    /// Ginge [`Model::flush_to`]? Für den Hinweis beim Überfahren der
    /// Zielwand; ändert nichts.
    pub fn can_flush_to(&self, wall: ElementId, target: ElementId) -> Result<(), FlushError> {
        if self.wall_below(wall) == Some(target) {
            return match self.offset_plan(wall, 0.0) {
                Some(_) => Ok(()),
                None => Err(FlushError::Invalid),
            };
        }
        let (run, pts) = self.flush_up_points(wall, target)?;
        // Probe mit gelöster Zielwand: sie bleibt stehen, der Rest folgt
        let mut probe = self.clone();
        if let Some(ElementKind::Wall(Wall {
            coupling: Some(c), ..
        })) = probe.elements.get_mut(target).map(|e| &mut e.kind)
        {
            c.linked = false;
        }
        if probe.stack_fits(run, &pts) {
            Ok(())
        } else {
            Err(FlushError::Invalid)
        }
    }

    /// Zug von `wall` und seine neuen Punkte, wenn `wall` an die Wand
    /// `target` darüber rückt.
    pub(super) fn flush_up_points(
        &self,
        wall: ElementId,
        target: ElementId,
    ) -> Result<(RunId, Vec<Vec3>), FlushError> {
        if self.wall_below(target) != Some(wall) {
            return Err(FlushError::NotPartners);
        }
        // Eine gestapelte Wand an die darüber: ihr eigener Versatz müsste
        // mitwandern (drei Geschosse, noch nicht vorgesehen)
        if self.stack_offset(wall).is_some() {
            return Err(FlushError::Unsupported);
        }
        let (run, seg) = self.segment_of(wall).ok_or(FlushError::NotPartners)?;
        let (d, _) = self.stack_offset(target).ok_or(FlushError::NotPartners)?;
        let c = self.chain(run).ok_or(FlushError::Invalid)?;
        let c = c
            .with_segment_moved(seg, c.outward_sign() * d)
            .ok_or(FlushError::Invalid)?;
        Ok((run, c.points))
    }

    /// Legt den Zug `up` auf den Zug `below` (mit den Versätzen seiner Wände)
    /// und koppelt jede Wand an ihre Wand darunter; gelöste Wände bleiben
    /// gelöst und stehen still. Springt ein Teilstück an einer Fuge allein
    /// zurück, steht dort eine Stufe ohne Kopplung (W-12).
    pub(super) fn follow_below(&mut self, up: RunId, below: RunId) {
        let (Some(lower), Some(lsegs)) = (
            self.base_chain(below),
            self.run(below).map(|r| r.segments.clone()),
        ) else {
            return;
        };
        let (mut offsets, free) = self.stack_offsets(up, &lower, &lsegs);
        // Gelöste Wand, die durch das Ziehen darunter fast bündig steht: rastet
        // wie beim Ziehen am Fuß unter 20 mm auf 0 ein (Regel 31, Review 1t T1)
        for (d, f) in offsets.iter_mut().zip(&free) {
            if *f && d.abs() < MIN_OFFSET {
                *d = 0.0;
            }
        }
        let recount = self.partners(up, &lsegs).iter().any(|p| p.is_none());
        let (pts, map) = match lower.with_segment_offsets_stepped(&offsets, self.step_face(up)) {
            Some((c, map)) => (c.points, map),
            // Neue Segmentzahl: bündig neu auflegen, die Kopplungen folgen
            None if recount => (lower.points.clone(), (0..lsegs.len()).map(Some).collect()),
            // Ungültig (siehe stack_fits): Zug bleibt liegen, statt bündig
            // zu springen
            None => return,
        };
        let same = self
            .run(up)
            .is_some_and(|r| r.points == pts && r.segments.len() == map.len());
        if !same && !self.set_points(up, &pts) {
            return;
        }
        self.couple(up, &lsegs, &map, &offsets, &free);
    }

    /// Kopplungen des Zuges `up` nach `map`: Segment j steht auf der Wand
    /// `lsegs[k]` mit Versatz `offsets[k]`, gelöst nach `free[k]`; eine
    /// Stufe (`None`) steht auf keiner.
    pub(super) fn couple(
        &mut self,
        up: RunId,
        lsegs: &[ElementId],
        map: &[Option<usize>],
        offsets: &[f64],
        free: &[bool],
    ) {
        let segs = self.run(up).map(|r| r.segments.clone()).unwrap_or_default();
        for (j, w) in segs.into_iter().enumerate() {
            let want = map.get(j).copied().flatten().and_then(|k| {
                Some(Coupling {
                    below: *lsegs.get(k)?,
                    offset: *offsets.get(k)?,
                    linked: free.get(k) != Some(&true),
                })
            });
            let now = match self.element(w).map(|e| &e.kind) {
                Some(ElementKind::Wall(x)) => x.coupling,
                _ => continue,
            };
            if now != want {
                note!(self, Element, self.elements, w);
                if let Some(ElementKind::Wall(x)) = self.elements.get_mut(w).map(|e| &mut e.kind) {
                    x.coupling = want;
                }
            }
        }
    }

    /// Gelöste Segmente eines gestapelten Zuges, der selbst bewegt wurde:
    /// ihr Versatz ist wieder die wahre Lage gegen den Zug darunter.
    pub(super) fn remeasure(&mut self, id: RunId) {
        let Some(below) = self.run_below(id) else {
            return;
        };
        let (Some(lower), Some(upc)) = (self.base_chain(below), self.base_chain(id)) else {
            return;
        };
        let segs = self.run(id).map(|r| r.segments.clone()).unwrap_or_default();
        for (j, k) in self.stack_map(id).into_iter().enumerate() {
            let (Some(k), Some(&w)) = (k, segs.get(j)) else {
                continue;
            };
            let now = match self.element(w).map(|e| &e.kind) {
                Some(ElementKind::Wall(x)) => x.coupling,
                _ => continue,
            };
            let Some(c) = now.filter(|c| !c.linked) else {
                continue;
            };
            let o = upc.offset_against(j, &lower, k).unwrap_or(0.0);
            if c.offset != o {
                note!(self, Element, self.elements, w);
                if let Some(ElementKind::Wall(x)) = self.elements.get_mut(w).map(|e| &mut e.kind) {
                    x.coupling = Some(Coupling { offset: o, ..c });
                }
            }
        }
    }

    /// Prüfung des Zuges `id` über `lower` (Regeln 28 und 30): Wand für
    /// Wand in Laufrichtung über den Segmenten darunter, dazwischen nur
    /// Stufen an einer Fuge darunter (W-12); jedes Segment mindestens
    /// min(Wanddicke, Länge des Partners) lang.
    pub(super) fn check_stack(
        &self,
        id: RunId,
        r: &WallRun,
        lower: &WallRun,
        out: &mut Vec<String>,
    ) {
        let nl = lower.segments.len();
        let map: Vec<Option<usize>> = r
            .segments
            .iter()
            .map(|w| {
                let b = self.wall_below(*w)?;
                lower.segments.iter().position(|x| *x == b)
            })
            .collect();
        let coupled = map.iter().flatten().count();
        if coupled != nl && r.segments.len() != nl {
            out.push(format!(
                "Wandzug {id:?}: {} Segmente über {} im Geschoss darunter",
                r.segments.len(),
                nl
            ));
            return;
        }
        let lower_dirs = lower_directions(lower);
        let joint = |k: usize| {
            let (a, b) = (k, (k + 1) % nl.max(1));
            (lower.closed || b > 0)
                && lower_dirs
                    .get(a)
                    .zip(lower_dirs.get(b))
                    .is_some_and(|(x, y)| x.dot(*y) > 1.0 - 1e-9)
        };
        let mut expect = 0;
        let m = r.segments.len();
        for (j, w) in r.segments.iter().enumerate() {
            let Some(e) = self.element(*w) else { continue };
            let ElementKind::Wall(x) = &e.kind else {
                continue;
            };
            match (x.coupling, map[j]) {
                (Some(_), Some(k)) if k == expect => expect += 1,
                (Some(_), _) => {
                    out.push(format!(
                        "{}: steht nicht über Segment {} darunter",
                        e.number,
                        expect + 1
                    ));
                    expect += 1;
                }
                (None, _) => {
                    // Stufe: zwischen zwei Teilstücken, die darunter an
                    // einer Fuge aneinanderstoßen
                    let before = (j + m - 1) % m;
                    let after = (j + 1) % m;
                    let step = expect > 0
                        && map[before] == Some(expect - 1)
                        && map[after] == Some(expect % nl.max(1))
                        && joint(expect - 1);
                    if !step {
                        out.push(format!("{}: ohne Stapelbezug", e.number));
                    }
                }
            }
        }
        // Regel 30: jedes Segment mindestens min(Wanddicke, Länge des
        // Partners) lang
        let (n, nlp) = (r.points.len(), lower.points.len());
        for (j, w) in r.segments.iter().enumerate() {
            let (Some(e), Some(k)) = (self.element(*w), map[j]) else {
                continue;
            };
            let len = (r.points[(j + 1) % n] - r.points[j]).length();
            let partner = (lower.points[(k + 1) % nlp] - lower.points[k]).length();
            let dicke = e
                .layer_set
                .and_then(|t| self.layer_sets.get(t))
                .map_or(0.0, |t| t.thickness());
            let min = dicke.min(partner);
            if len >= 1.0 && len + 0.5 < min {
                out.push(format!(
                    "{}: {:.0} mm lang, kürzer als {:.0} mm",
                    e.number, len, min
                ));
            }
        }
    }
}

/// Richtung je Segment des Zuges (Einheitsvektoren).
fn lower_directions(r: &WallRun) -> Vec<Vec3> {
    let n = r.points.len();
    (0..r.segments.len())
        .map(|k| (r.points[(k + 1) % n] - r.points[k]).normalized())
        .collect()
}
