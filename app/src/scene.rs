//! Szene: das Gebäudemodell mit Rückgängig-Verlauf und den daraus abgeleiteten
//! Körpern. Einheit: Millimeter.
//!
//! Körper werden je Wandzug zwischengespeichert. Eine Änderung markiert nur den
//! betroffenen Wandzug; [`Scene::rebuild_dirty`] rechnet nur ihn neu. Das Netz
//! einer Ansicht entsteht aus den gespeicherten Körpern, beim Ziehen getrennt in
//! ein ruhendes Netz ohne den gezogenen Zug und ein Live-Netz nur für ihn.

use crate::draw_table::DrawTable;
use crate::ui::Field;
use crate::ui::ViewKind;
use sk_math::{vec3, Vec3};
use sk_model::{
    floor_qto_of, foundation_qto_of, run_qto, Category, Direction, ElementId, FloorQto, FloorSlab,
    FootingQto, Foundation, Model, RunId, SlabQto, Solid, Touched, Txn, WallChain, WallQto,
    FLOOR_PART, FOOTING_PART, SLAB_PART,
};
use sk_render::MeshData;
use sk_ui::theme::Theme;

/// Schnitthöhe des Grundrisses über der Unterkante des aktiven Geschosses (mm).
pub const PLAN_CUT: f64 = 1000.0;

type Aabb = (Vec3, Vec3);
/// Schnittebene: Punkt und Normale zum Betrachter.
pub type Plane = (Vec3, Vec3);

/// Abgeleitete Daten eines Wandzugs.
struct RunCache {
    id: RunId,
    chain: WallChain,
    /// Körper für 3D und Ansichten.
    solid: Solid,
    /// Körper waagerecht geschnitten (Grundriss) und dessen Schnitthöhe,
    /// erst bei Bedarf berechnet.
    plan: Option<(f64, Solid)>,
    /// Senkrechter Schnitt für die zuletzt gefragte Ebene.
    section: Option<(Plane, Solid)>,
    bounds: Option<Aabb>,
    /// Äußerer Wandfuß je Segment (Gummiband).
    foot: Vec<(Vec3, Vec3)>,
    /// Umschließender Quader des Wandfußes (Vortest beim Greifen).
    foot_bounds: Option<Aabb>,
    /// Mengen je Segment; leer, solange der Zug gezogen wird.
    qto: Vec<WallQto>,
    /// Gründung unter einem geschlossenen Außenwandzug (B9).
    found: Option<Foundation>,
    /// Mengen von Sohlplatte und Frostschürze (wie `qto` erst nach dem Ziehen).
    found_qto: Option<(SlabQto, FootingQto)>,
    /// Erdgeschossdecke über einem geschlossenen Außenwandzug (B10).
    floor: Option<FloorSlab>,
    floor_qto: Option<FloorQto>,
    /// Wie oft dieser Zug berechnet wurde (für Tests und Messung).
    builds: u32,
}

impl RunCache {
    fn new(
        id: RunId,
        chain: WallChain,
        found: Option<Foundation>,
        floor: Option<FloorSlab>,
        builds: u32,
    ) -> RunCache {
        let mut solid = chain.solid();
        if let Some(f) = &found {
            solid.append(&part(f.slab_solid(), SLAB_PART));
            solid.append(&part(f.footing_solid(), FOOTING_PART));
        }
        if let Some(f) = &floor {
            solid.append(&part(f.solid(), FLOOR_PART));
        }
        let foot = chain.outer_foot();
        let foot_bounds = foot
            .iter()
            .flat_map(|&(a, b)| [a, b])
            .fold(None, |acc, p| union(acc, Some((p, p))));
        RunCache {
            id,
            plan: None,
            bounds: solid.bounds(),
            foot,
            foot_bounds,
            solid,
            chain,
            section: None,
            qto: Vec::new(),
            found,
            found_qto: None,
            floor,
            floor_qto: None,
            builds,
        }
    }

    /// Körper einer Ansicht; `cut` ist die Schnitthöhe des Grundrisses.
    fn view_solid(&mut self, view: ViewKind, section: Option<Plane>, cut: f64) -> Option<&Solid> {
        match (view, section) {
            (ViewKind::Plan, _) => {
                let (chain, found, floor) = (&self.chain, &self.found, &self.floor);
                // Die Gründung liegt unter der Schnitthöhe: Draufsicht. Die
                // Decke liegt im EG darüber und bleibt leer, im OG darunter.
                if self.plan.as_ref().is_none_or(|(c, _)| *c != cut) {
                    let mut s = chain.solid_cut_at(cut);
                    if let Some(f) = found {
                        s.append(&part(f.slab_solid(), SLAB_PART));
                        s.append(&part(f.footing_solid(), FOOTING_PART));
                    }
                    if let Some(f) = floor {
                        s.append(&part(f.solid_cut_at(cut), FLOOR_PART));
                    }
                    self.plan = Some((cut, s));
                }
                self.plan.as_ref().map(|(_, s)| s)
            }
            (ViewKind::Section, Some(pl)) => {
                if self.section.as_ref().is_none_or(|(p, _)| *p != pl) {
                    let (p0, n) = pl;
                    let mut s = self.solid.clipped(p0, n);
                    s.append(&self.chain.section_caps(p0, n));
                    if let Some(f) = &self.found {
                        let (slab, foot) = f.section_caps(p0, n);
                        s.append(&part(slab, SLAB_PART));
                        s.append(&part(foot, FOOTING_PART));
                    }
                    if let Some(f) = &self.floor {
                        s.append(&part(f.section_caps(p0, n), FLOOR_PART));
                    }
                    self.section = Some((pl, s));
                }
                self.section.as_ref().map(|(_, s)| s)
            }
            (ViewKind::Section, None) => None,
            _ => Some(&self.solid),
        }
    }
}

/// So viele Schritte lassen sich rückgängig machen.
const HISTORY: usize = 200;

pub struct Scene {
    model: Model,
    /// Schritte für „Rückgängig“ (die ältesten fallen nach [`HISTORY`] weg).
    undo: Vec<Txn>,
    /// Rückgängig gemachte Schritte für „Wiederholen“.
    redo: Vec<Txn>,
    /// Zwischenspeicher je Wandzug, über den Arena-Platz der [`RunId`].
    cache: Vec<Option<RunCache>>,
    /// Wandzüge, die neu berechnet werden müssen.
    dirty: Vec<RunId>,
    all_dirty: bool,
    /// Wandzüge, deren Mengen nach einer Live-Änderung noch fehlen.
    unsettled: Vec<RunId>,
    /// Züge, die sich im offenen Schritt live geändert haben (der gezogene und
    /// die mitgeführten oder angeschlossenen).
    live: Vec<RunId>,
    bounds: Option<Aabb>,
    /// Aufgelöste Attributtabellen des Modells.
    table: DrawTable,
    /// Farbschema der App (Rückfallfarben und Bildpunkte je mm der Tabelle).
    theme: Theme,
    /// Aktives Geschoss (Sitzungszustand, nicht in der Datei); `None` = EG.
    active: Option<sk_model::StoreyId>,
}

fn union(a: Option<Aabb>, b: Option<Aabb>) -> Option<Aabb> {
    match (a, b) {
        (Some((l0, h0)), Some((l1, h1))) => Some((
            vec3(l0.x.min(l1.x), l0.y.min(l1.y), l0.z.min(l1.z)),
            vec3(h0.x.max(h1.x), h0.y.max(h1.y), h0.z.max(h1.z)),
        )),
        (a, None) => a,
        (None, b) => b,
    }
}

/// Trifft der Strahl den Quader (Slab-Test)?
fn ray_hits_box(o: Vec3, d: Vec3, (lo, hi): Aabb) -> bool {
    let (mut t0, mut t1) = (0.0f64, f64::INFINITY);
    for (o, d, lo, hi) in [
        (o.x, d.x, lo.x, hi.x),
        (o.y, d.y, lo.y, hi.y),
        (o.z, d.z, lo.z, hi.z),
    ] {
        if d.abs() < 1e-12 {
            if o < lo || o > hi {
                return false;
            }
            continue;
        }
        let (a, b) = ((lo - o) / d, (hi - o) / d);
        t0 = t0.max(a.min(b));
        t1 = t1.min(a.max(b));
        if t0 > t1 {
            return false;
        }
    }
    true
}

impl Scene {
    pub fn new() -> Scene {
        Scene::with_model(Model::new())
    }

    pub fn with_model(mut model: Model) -> Scene {
        model.require_steps();
        let theme = Theme::dark();
        let table = DrawTable::resolve(&model, &theme);
        let mut s = Scene {
            model,
            undo: Vec::new(),
            redo: Vec::new(),
            cache: Vec::new(),
            dirty: Vec::new(),
            all_dirty: true,
            unsettled: Vec::new(),
            live: Vec::new(),
            bounds: None,
            table,
            theme,
            active: None,
        };
        s.rebuild_dirty(false);
        s
    }

    pub fn model(&self) -> &Model {
        &self.model
    }

    /// Aktives Geschoss; gibt es das gewählte nicht mehr, das EG.
    pub fn active_storey(&self) -> sk_model::StoreyId {
        let eg = self.model.defaults().storey;
        self.active
            .filter(|&id| {
                self.model
                    .storey(id)
                    .is_some_and(|st| st.kind != sk_model::LevelKind::Foundation)
            })
            .unwrap_or(eg)
    }

    /// Macht ein Geschoss aktiv; `false`, wenn es schon aktiv ist oder keines
    /// ist (die Gründung).
    pub fn set_active_storey(&mut self, id: sk_model::StoreyId) -> bool {
        let ok = self
            .model
            .storey(id)
            .is_some_and(|st| st.kind != sk_model::LevelKind::Foundation);
        if !ok || id == self.active_storey() {
            return false;
        }
        self.active = Some(id);
        true
    }

    /// Schnitthöhe des Grundrisses: Unterkante des aktiven Geschosses + 1 m.
    pub fn plan_cut(&self) -> f64 {
        let id = self.active_storey();
        self.model.storey(id).map_or(0.0, |st| st.elevation) + PLAN_CUT
    }

    /// Zeichentabelle zum aktuellen Stand der Attribute.
    pub fn table(&self) -> &DrawTable {
        &self.table
    }

    /// Übernimmt das Farbschema der App. `true`, wenn sich dadurch die
    /// Zeichentabelle inhaltlich ändert (nur sie, die Netze bleiben).
    pub fn set_theme(&mut self, theme: &Theme) -> bool {
        if theme.rev == self.table.theme_rev && *theme == self.theme {
            return false;
        }
        self.theme = theme.clone();
        let table = DrawTable::resolve(&self.model, theme);
        let changed = DrawTable {
            theme_rev: self.table.theme_rev,
            ..table.clone()
        } != self.table;
        self.table = table;
        changed
    }

    /// Ändert einen Stift (mit Verlaufseintrag). Nur die Zeichentabelle ändert
    /// sich. Bisher nur in Tests; das Einstellungsfenster folgt.
    #[cfg(test)]
    pub fn set_pen(&mut self, id: sk_model::PenId, pen: sk_model::Pen) -> bool {
        self.begin("Stift ändern");
        let ok = self.model.set_pen(id, pen);
        self.commit();
        ok
    }

    /// Rechnet die markierten Wandzüge neu und entfernt gelöschte. `live`: nur
    /// die Körper, die Mengen erst bei [`Scene::settle`].
    fn rebuild_dirty(&mut self, live: bool) {
        if self.table.rev != self.model.attr().rev() {
            self.table = DrawTable::resolve(&self.model, &self.theme);
        }
        if self.all_dirty {
            self.all_dirty = false;
            self.dirty.clear();
            let builds: Vec<u32> = self
                .cache
                .iter()
                .map(|c| c.as_ref().map_or(0, |c| c.builds))
                .collect();
            self.cache.clear();
            let ids: Vec<RunId> = self.model.runs().ids().collect();
            for id in ids {
                let b = builds.get(id.index() as usize).copied().unwrap_or(0);
                self.store(id, b, false);
            }
        } else {
            for id in std::mem::take(&mut self.dirty) {
                let b = self.cached(id).map_or(0, |c| c.builds);
                self.store(id, b, live);
            }
        }
        self.bounds = self
            .cache
            .iter()
            .flatten()
            .fold(None, |acc, c| union(acc, c.bounds));
    }

    /// Berechnet einen Wandzug neu (oder entfernt ihn, wenn es ihn nicht mehr gibt).
    fn store(&mut self, id: RunId, builds: u32, live: bool) {
        let slot = id.index() as usize;
        if self.cache.len() <= slot {
            self.cache.resize_with(slot + 1, || None);
        }
        match self.model.chain_and_floor(id) {
            Some((c, floor)) => {
                let found = self.model.foundation(id).and_then(Result::ok);
                let floor = floor.and_then(Result::ok);
                let mut rc = RunCache::new(id, c, found, floor, builds + 1);
                if live {
                    if !self.unsettled.contains(&id) {
                        self.unsettled.push(id);
                    }
                } else {
                    rc.qto = run_qto(&self.model, id);
                    rc.found_qto = rc.found.as_ref().map(foundation_qto_of);
                    rc.floor_qto = rc.floor.as_ref().map(floor_qto_of);
                    self.unsettled.retain(|&u| u != id);
                }
                self.cache[slot] = Some(rc);
            }
            // Gelöscht: Platz nur räumen, wenn kein neuerer Zug darin steht
            None => {
                if self.cache[slot].as_ref().is_some_and(|c| c.id == id) {
                    self.cache[slot] = None;
                }
            }
        }
    }

    fn mark(&mut self, id: RunId) {
        if !self.dirty.contains(&id) {
            self.dirty.push(id);
        }
    }

    fn mark_all(&mut self) {
        self.all_dirty = true;
        self.unsettled.clear();
    }

    /// Rechnet die Mengen nach, die beim Live-Ziehen ausgelassen wurden.
    pub fn settle(&mut self) {
        for id in std::mem::take(&mut self.unsettled) {
            if self.model.run(id).is_some() {
                let q = run_qto(&self.model, id);
                if let Some(Some(c)) = self.cache.get_mut(id.index() as usize) {
                    if c.id == id {
                        c.qto = q;
                        c.found_qto = c.found.as_ref().map(foundation_qto_of);
                        c.floor_qto = c.floor.as_ref().map(floor_qto_of);
                    }
                }
            }
        }
    }

    /// Mengen einer Wand (fehlen, solange ihr Wandzug gezogen wird).
    pub fn wall_qto(&self, wall: ElementId) -> Option<&WallQto> {
        let (run, seg) = self.model.segment_of(wall)?;
        self.cached(run)?.qto.get(seg)
    }

    /// Mengen von Sohlplatte und Frostschürze unter einem Wandzug.
    pub fn foundation_qto(&self, run: RunId) -> Option<&(SlabQto, FootingQto)> {
        self.cached(run)?.found_qto.as_ref()
    }

    /// Gründung unter einem Wandzug, wie sie gezeichnet wird.
    pub fn foundation(&self, run: RunId) -> Option<&Foundation> {
        self.cached(run)?.found.as_ref()
    }

    /// Mengen der Erdgeschossdecke über einem Wandzug.
    pub fn floor_qto(&self, run: RunId) -> Option<&FloorQto> {
        self.cached(run)?.floor_qto.as_ref()
    }

    /// Erdgeschossdecke über einem Wandzug, wie sie gezeichnet wird.
    pub fn floor(&self, run: RunId) -> Option<&FloorSlab> {
        self.cached(run)?.floor.as_ref()
    }

    fn cached(&self, id: RunId) -> Option<&RunCache> {
        self.cache
            .get(id.index() as usize)?
            .as_ref()
            .filter(|c| c.id == id)
    }

    /// Wie oft ein Wandzug berechnet wurde.
    #[cfg(test)]
    pub fn build_count(&self, id: RunId) -> u32 {
        self.cached(id).map_or(0, |c| c.builds)
    }

    pub fn chain(&self, run: RunId) -> Option<&WallChain> {
        self.cached(run).map(|c| &c.chain)
    }

    /// Äußerer Wandfuß eines Wandzugs, je Segment (Anfang, Ende).
    pub fn foot(&self, run: RunId) -> Option<&[(Vec3, Vec3)]> {
        self.cached(run).map(|c| c.foot.as_slice())
    }

    /// Äußerer Wandfuß aller Wandzüge: umschließender Quader und je Segment
    /// (Anfang, Ende).
    pub fn feet(&self) -> impl Iterator<Item = (RunId, Option<Aabb>, &[(Vec3, Vec3)])> + '_ {
        self.cache
            .iter()
            .flatten()
            .map(|c| (c.id, c.foot_bounds, c.foot.as_slice()))
    }

    /// Öffnet einen Schritt für „Rückgängig“ (z. B. beim Greifen am Gummiband).
    /// Ist noch einer offen (Loslassen ging verloren, weil das Fenster beim
    /// Ziehen die Maus verlor), wird er vorher abgeschlossen und bleibt im Verlauf.
    pub fn begin(&mut self, label: &'static str) {
        if self.model.in_step() {
            self.commit();
        }
        self.live.clear();
        self.model.begin(label);
    }

    /// Schließt den offenen Schritt und legt ihn in den Verlauf, falls er etwas
    /// geändert hat. Berechnet ausstehende Mengen und Wandzüge.
    pub fn commit(&mut self) {
        if let Some(t) = self.model.commit() {
            self.undo.push(t);
            if self.undo.len() > HISTORY {
                self.undo.remove(0);
            }
            self.redo.clear();
        }
        self.live.clear();
        self.rebuild_dirty(false);
        self.settle();
    }

    /// Verwirft den offenen Schritt (Esc beim Ziehen); nur die betroffenen
    /// Wandzüge werden neu berechnet.
    pub fn rollback(&mut self) {
        let touched = self.model.rollback();
        self.live.clear();
        self.mark_touched(&touched);
        self.rebuild_dirty(false);
    }

    fn mark_touched(&mut self, t: &Touched) {
        // Attribute (Stifte, Schraffuren, Oberflächen) stehen nur in der Tabelle
        // des Renderers; neu gerechnet wird erst, wenn sich Baustoffe oder
        // Aufbauten ändern
        if t.library {
            self.mark_all();
        } else {
            for &id in &t.runs {
                self.mark(id);
            }
        }
    }

    /// Legt die Außenwand an, die `w` beschreibt (Punkte, Bezugsseite, Höhe),
    /// mit dem voreingestellten Außenwand-Aufbau.
    #[cfg(test)]
    pub fn add_wall(&mut self, w: &WallChain) -> Option<RunId> {
        self.add_wall_as(w, Category::ExteriorWall)
    }

    /// Legt eine Wand der Kategorie `cat` an (Außen- oder Innenwand), mit dem
    /// dafür voreingestellten Aufbau. Angeschlossene Züge werden neu berechnet.
    pub fn add_wall_as(&mut self, w: &WallChain, cat: Category) -> Option<RunId> {
        self.begin("Wand zeichnen");
        let d = self.model.defaults();
        let set = match cat {
            Category::InteriorWall => d.interior_wall,
            _ => d.exterior_wall,
        };
        let run = self
            .model
            .add_wall_run(&w.points, w.closed, w.ref_side, w.height, set, cat);
        if let Some(id) = run {
            self.mark(id);
            for p in self.model.joined_runs(id) {
                self.mark(p);
            }
            for p in self.model.runs_under_floor(id) {
                self.mark(p);
            }
        }
        self.commit();
        run
    }

    /// Setzt einen Parameter der Gründung unter dem Zug des Bauteils `id`
    /// (Wert in mm, aus einem Zahlenfeld). Ein Schritt im Verlauf; `true`,
    /// wenn sich etwas geändert hat.
    pub fn set_field(&mut self, id: ElementId, field: Field, mm: f64) -> bool {
        use sk_model::ElementKind::{GroundSlab, StripFooting};
        let m = &self.model;
        let Some(run) = m.run_of(id) else {
            return false;
        };
        if field == Field::FloorThickness {
            return self.set_floor_thickness(run, mm);
        }
        let Some((slab, footing)) = m.foundation_of(run) else {
            return false;
        };
        let (Some(GroundSlab(s)), Some(StripFooting(f))) = (
            m.element(slab).map(|e| e.kind.clone()),
            footing.and_then(|f| m.element(f)).map(|e| e.kind.clone()),
        ) else {
            return false;
        };
        let (label, old) = match field {
            Field::SlabThickness => ("Plattendicke", s.thickness),
            Field::Recess => ("Sockelrücksprung", s.recess),
            Field::FootingWidth => ("Schürzenbreite", f.width),
            Field::FootingDepth => match footing.and_then(|f| m.footing_depth(f)) {
                Some(d) => ("Schürzentiefe", d),
                None => return false,
            },
            _ => return false,
        };
        if old == mm {
            return false;
        }
        self.begin(label);
        let ok = match (field, footing) {
            (Field::SlabThickness, _) => self.model.set_slab_thickness(slab, mm),
            (Field::Recess, _) => self.model.set_slab_recess(slab, mm),
            (Field::FootingWidth, Some(fid)) => self.model.set_footing_width(fid, mm),
            // verschiebt UK Gründung: alle Gründungen ändern sich (B11)
            (Field::FootingDepth, Some(fid)) => {
                let ok = self.model.set_footing_depth(fid, mm);
                self.mark_all();
                ok
            }
            _ => false,
        };
        self.mark(run);
        self.commit();
        ok
    }

    /// Inhalt des Paneels „Geschosse“: Bänder, lichte Höhe des EG und die
    /// änderbaren Zahlen mit ihren Grenzen.
    pub fn levels(&self) -> crate::ui::Levels {
        use crate::ui::{Band, FieldRow, Levels};
        let m = &self.model;
        let eg = m.defaults().storey;
        let active = self.active_storey();
        let mut l = Levels::default();
        let row = |field, label, value, (min, max): (f64, f64)| FieldRow {
            field,
            label,
            value,
            min,
            max,
            zero: false,
        };
        for id in m.levels() {
            let Some(st) = m.storey(id) else {
                continue;
            };
            let foundation = st.kind == sk_model::LevelKind::Foundation;
            l.bands.push(Band {
                id,
                name: if foundation {
                    st.name.clone()
                } else {
                    st.short.clone()
                },
                bottom: st.elevation,
                top: st.top(),
                foundation,
                active: id == active,
            });
            if foundation {
                let (lo, hi) = m.foundation_bottom_range();
                l.fields.push(row(
                    Field::LevelBottom,
                    "UK Gründung",
                    st.elevation,
                    (lo, hi),
                ));
                let range = (st.top() - hi, st.top() - lo);
                l.fields.push(row(
                    Field::StoreyHeight(id),
                    "Gründungstiefe",
                    st.height,
                    range,
                ));
                continue;
            }
            let Some((lo, hi)) = m.storey_top_range(id) else {
                continue;
            };
            l.fields
                .push(row(Field::LevelTop(id), "Oberkante", st.top(), (lo, hi)));
            let (e, h) = (st.elevation, st.height);
            l.fields.push(row(
                Field::StoreyHeight(id),
                "Geschosshöhe",
                h,
                (lo - e, hi - e),
            ));
            if id == eg {
                let clear = m.clear_height(id);
                let t = h - clear;
                l.clear = Some((id, clear));
                let range = (lo - e - t, hi - e - t);
                l.fields
                    .push(row(Field::ClearHeight(id), "lichte Höhe", clear, range));
            }
        }
        l
    }

    /// Zahl aus dem Paneel „Geschosse“ als ein Schritt; `false`, wenn das
    /// Modell sie ablehnt.
    pub fn set_level(&mut self, field: Field, mm: f64) -> bool {
        match field {
            Field::LevelBottom => self.edit_model("UK Gründung", |m| m.set_foundation_bottom(mm)),
            Field::LevelTop(id) => {
                self.edit_model("Oberkante Geschoss", |m| m.set_storey_top(id, mm))
            }
            Field::StoreyHeight(id) => {
                self.edit_model("Geschosshöhe", |m| m.set_storey_height(id, mm))
            }
            Field::ClearHeight(id) => {
                self.edit_model("lichte Höhe", |m| m.set_clear_height(id, mm))
            }
            _ => false,
        }
    }

    /// Dicke der Erdgeschossdecke über dem Zug `run`. Neu gerechnet werden
    /// der Zug (Tasche) und die Innenwände unter der Decke.
    fn set_floor_thickness(&mut self, run: RunId, mm: f64) -> bool {
        let Some(id) = self.model.floor_of(run) else {
            return false;
        };
        let Some(sk_model::ElementKind::Floor(f)) = self.model.element(id).map(|e| e.kind.clone())
        else {
            return false;
        };
        if f.thickness == mm {
            return false;
        }
        self.begin("Deckendicke");
        let ok = self.model.set_floor_thickness(id, mm);
        self.mark(run);
        for r in self.model.runs_under_floor(run) {
            self.mark(r);
        }
        self.commit();
        ok
    }

    /// Sockelrücksprung der Sohlplatte unter dem Zug von `id` um eine Stufe
    /// größer oder kleiner: 0 ↔ 2 cm, darüber in 1-cm-Schritten. Ein Schritt
    /// im Verlauf. `true`, wenn sich etwas geändert hat.
    #[cfg(test)]
    pub fn step_recess(&mut self, id: ElementId, up: bool) -> bool {
        let m = &self.model;
        let Some(run) = m.run_of(id) else {
            return false;
        };
        let Some((slab, _)) = m.foundation_of(run) else {
            return false;
        };
        let Some(sk_model::ElementKind::GroundSlab(s)) = m.element(slab).map(|e| e.kind.clone())
        else {
            return false;
        };
        let r = s.recess;
        let next = match (up, r <= sk_model::MIN_RECESS) {
            (true, _) if r < sk_model::MIN_RECESS => sk_model::MIN_RECESS,
            (true, _) => r + 10.0,
            (false, true) => 0.0,
            (false, false) => (r - 10.0).max(sk_model::MIN_RECESS),
        };
        if next == r {
            return false;
        }
        self.begin("Sockelrücksprung");
        let ok = self.model.set_slab_recess(slab, next);
        self.mark(run);
        self.commit();
        ok
    }

    /// Ändert das Modell in einem Schritt (Zahlen im Paneel „Geschosse“ und
    /// Tests); alles wird neu berechnet.
    pub fn edit_model(&mut self, label: &'static str, f: impl FnOnce(&mut Model) -> bool) -> bool {
        self.begin(label);
        let ok = f(&mut self.model);
        self.mark_all();
        self.commit();
        ok
    }

    /// Oberkante eines Geschosses beim Ziehen im Paneel „Geschosse“: geklemmt,
    /// ohne Verlaufseintrag (der Schritt ist offen, [`Scene::begin`]). Alle
    /// Züge folgen im Live-Netz, die Mengen kommen beim Loslassen.
    pub fn drag_storey_top(&mut self, id: sk_model::StoreyId, z: f64) {
        if self.model.drag_storey_top(id, z) {
            self.relevel();
        }
    }

    /// Unterkante der Gründung beim Ziehen, wie [`Scene::drag_storey_top`].
    pub fn drag_foundation_bottom(&mut self, z: f64) {
        if self.model.drag_foundation_bottom(z) {
            self.relevel();
        }
    }

    /// Nach einer Änderung der Geschossbänder: alle Züge live neu.
    fn relevel(&mut self) {
        let ids: Vec<RunId> = self.model.runs().ids().collect();
        for id in ids {
            self.mark(id);
        }
        self.rebuild_dirty(true);
    }

    /// Neue Eckpunkte eines Wandzugs ohne Verlaufseintrag (Live-Änderung beim
    /// Ziehen). Neu berechnet werden dieser Wandzug und die, die an ihm hängen.
    pub fn set_run_points(&mut self, run: RunId, points: &[Vec3]) {
        if let Some(runs) = self.model.set_run_points(run, points) {
            for r in runs {
                self.mark(r);
                if !self.live.contains(&r) {
                    self.live.push(r);
                }
            }
            self.rebuild_dirty(true);
        }
    }

    /// Züge im Live-Netz, während `run` gezogen wird: er selbst und alle, die
    /// sich dabei mitändern.
    pub fn live_set(&self, run: RunId) -> Vec<RunId> {
        let mut v = vec![run];
        for &r in self.live.iter().chain(&self.model.joined_runs(run)) {
            if !v.contains(&r) {
                v.push(r);
            }
        }
        v
    }

    pub fn undo(&mut self) -> bool {
        self.step(Direction::Undo)
    }

    /// Bezeichnung des Schritts, den „Rückgängig“ als Nächstes zurücknimmt.
    #[cfg(test)]
    pub fn undo_label(&self) -> Option<&'static str> {
        self.undo.last().map(|t| t.label)
    }

    pub fn redo(&mut self) -> bool {
        self.step(Direction::Redo)
    }

    fn step(&mut self, dir: Direction) -> bool {
        let (from, to) = match dir {
            Direction::Undo => (&mut self.undo, &mut self.redo),
            Direction::Redo => (&mut self.redo, &mut self.undo),
        };
        let Some(t) = from.pop() else {
            return false;
        };
        let touched = self.model.apply(&t, dir);
        to.push(t);
        self.mark_touched(&touched);
        self.rebuild_dirty(false);
        true
    }

    /// Netz einer Ansicht aus allen Wandzügen außer `except` (die gerade
    /// gezogenen). Beim Schnitt: Ebene `section` (Punkt, Normale zum Betrachter),
    /// alles davor wird weggeschnitten.
    pub fn mesh(&mut self, view: ViewKind, section: Option<Plane>, except: &[RunId]) -> MeshData {
        let cut = self.plan_cut();
        let mut m = MeshData::default();
        for c in self.cache.iter_mut().flatten() {
            if except.contains(&c.id) {
                continue;
            }
            if let Some(s) = c.view_solid(view, section, cut) {
                mesh_into(&mut m, s);
            }
        }
        m
    }

    /// Netz einzelner Wandzüge (Live-Netz beim Ziehen).
    pub fn mesh_runs(
        &mut self,
        view: ViewKind,
        section: Option<Plane>,
        runs: &[RunId],
    ) -> MeshData {
        let cut = self.plan_cut();
        let mut m = MeshData::default();
        for &run in runs {
            if let Some(Some(c)) = self.cache.get_mut(run.index() as usize) {
                if c.id == run {
                    if let Some(s) = c.view_solid(view, section, cut) {
                        mesh_into(&mut m, s);
                    }
                }
            }
        }
        m
    }

    /// Umschließender Quader des Modells.
    pub fn bounds(&self) -> Option<Aabb> {
        self.bounds
    }

    /// Nächster Treffer eines Strahls: Abstand und getroffene Wand.
    pub fn raycast(&self, origin: Vec3, dir: Vec3) -> Option<(f64, ElementId)> {
        let mut best: Option<(f64, RunId, u32)> = None;
        for c in self.cache.iter().flatten() {
            if !c.bounds.is_some_and(|b| ray_hits_box(origin, dir, b)) {
                continue;
            }
            if let Some((t, seg)) = c.solid.raycast_elem(origin, dir) {
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
        let cut = self.plan_cut();
        let mut best: Option<(f64, RunId, u32)> = None;
        for c in self.cache.iter_mut().flatten() {
            if !c.bounds.is_some_and(|b| ray_hits_box(origin, dir, b)) {
                continue;
            }
            let id = c.id;
            if let Some((t, seg)) = c
                .view_solid(view, section, cut)
                .and_then(|s| s.raycast_elem(origin, dir))
            {
                if best.is_none_or(|b| t < b.0) {
                    best = Some((t, id, seg));
                }
            }
        }
        let (_, run, seg) = best?;
        self.model.part_of(run, seg)
    }

    /// Mitte des umschließenden Quaders aller Flächen.
    pub fn center(&self) -> Option<Vec3> {
        self.bounds().map(|(lo, hi)| (lo + hi) * 0.5)
    }
}

/// Körper mit allen Dreiecken einem Teil zugeordnet (Treffer beim Klicken).
fn part(mut s: Solid, part: u32) -> Solid {
    for t in &mut s.triangles {
        t.elem = part;
    }
    s
}

/// Netz eines Körpers. Es trägt nur Darstellungsschlüssel und Kantenarten;
/// wie sie aussehen (3D oder Zeichnung), steht in der Tabelle des Renderers.
pub fn mesh_of(s: &Solid) -> MeshData {
    let mut m = MeshData::default();
    mesh_into(&mut m, s);
    m
}

/// Hängt das Netz eines Körpers an `m` an.
fn mesh_into(m: &mut MeshData, s: &Solid) {
    m.faces.reserve(s.triangles.len() * 3);
    for t in &s.triangles {
        let n = t.n.to_f32();
        let key = t.mat as f32;
        for (v, uv) in t.p.iter().zip(t.uv) {
            let p = v.to_f32();
            m.faces.push([
                p[0],
                p[1],
                p[2],
                n[0],
                n[1],
                n[2],
                key,
                uv[0] as f32,
                uv[1] as f32,
            ]);
        }
    }
    m.edges.extend(
        s.edges
            .iter()
            .map(|e| ([e.a.to_f32(), e.b.to_f32()], e.kind as f32)),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use sk_model::{Pen, RefSide};

    fn rechteck(x: f64) -> WallChain {
        WallChain {
            base: 0.0,
            points: vec![
                vec3(x, 0.0, 0.0),
                vec3(x, 4000.0, 0.0),
                vec3(x + 5000.0, 4000.0, 0.0),
                vec3(x + 5000.0, 0.0, 0.0),
            ],
            closed: true,
            ref_side: RefSide::Left,
            layers: Vec::new(),
            height: 3500.0,
            joints: Default::default(),
        }
    }

    fn same(a: &MeshData, b: &MeshData) -> bool {
        a.faces == b.faces && a.edges == b.edges
    }

    #[test]
    fn nur_der_geaenderte_zug_wird_neu_berechnet() {
        let mut s = Scene::with_model(Model::with_seed(1));
        let a = s.add_wall(&rechteck(0.0)).unwrap();
        let b = s.add_wall(&rechteck(10000.0)).unwrap();
        assert_eq!((s.build_count(a), s.build_count(b)), (1, 1));
        s.begin("Wand verschieben");
        // Oberes Segment 50 cm nach außen
        let moved = s.chain(a).unwrap().with_segment_moved(1, -500.0).unwrap();
        s.set_run_points(a, &moved.points);
        s.commit();
        assert_eq!((s.build_count(a), s.build_count(b)), (2, 1));
        // Gesamtquader folgt der Änderung
        assert!((s.bounds().unwrap().1.y - 4500.0).abs() < 1e-6);
        // Rückgängig rechnet nur den betroffenen Zug neu, und alles stimmt mit
        // einem vollständigen Neuaufbau überein
        assert!(s.undo());
        assert_eq!((s.build_count(a), s.build_count(b)), (3, 1));
        assert!((s.bounds().unwrap().1.y - 4000.0).abs() < 1e-6);
        let mut full = Scene::with_model(s.model().clone());
        for v in [ViewKind::Persp, ViewKind::Plan] {
            assert!(same(&s.mesh(v, None, &[]), &full.mesh(v, None, &[])));
        }
        let pl = Some((vec3(0.0, 2000.0, 0.0), vec3(0.0, -1.0, 0.0)));
        assert!(same(
            &s.mesh(ViewKind::Section, pl, &[]),
            &full.mesh(ViewKind::Section, pl, &[])
        ));
        assert!(s.model().check().is_empty());
    }

    #[test]
    fn nach_rueckgaengig_bekommt_eine_neue_wand_eine_neue_kennung() {
        let mut s = Scene::with_model(Model::with_seed(4));
        let a = s.add_wall(&rechteck(0.0)).unwrap();
        let wa = s.model().wall_at(a, 0).unwrap();
        assert!(s.undo());
        let b = s.add_wall(&rechteck(0.0)).unwrap();
        assert_ne!(a, b);
        // Die alte Wandkennung zeigt nicht auf die neue Wand
        assert!(s.model().element(wa).is_none());
        assert!(s.model().check().is_empty());
        // Wiederholen geht nach einer neuen Wand nicht mehr, Rückgängig schon
        assert!(!s.redo());
        assert!(s.undo());
        assert!(s.model().run(b).is_none());
    }

    /// E3, Test 4: Stiftfarben ändern nur die Zeichentabelle; kein Netz wird
    /// neu gebaut, und das Netz bleibt Byte für Byte gleich.
    #[test]
    fn stiftfarbe_aendern_baut_kein_netz_neu() {
        let mut s = Scene::with_model(Model::with_seed(8));
        let run = s.add_wall(&rechteck(0.0)).unwrap();
        let rev = s.table().rev;
        let builds = s.build_count(run);
        let pen = |s: &Scene, n| {
            s.model()
                .attr()
                .pens()
                .iter()
                .find(|(_, p)| p.number == n)
                .map(|(id, p)| (id, p.clone()))
                .unwrap()
        };
        let before = s.mesh(ViewKind::Plan, None, &[]);
        let looks = s.table().looks(1.0);
        // Stift 3 (kräftig, Schnittkante) und Stift 5 (Grund) auf Rot
        for n in [3, 5] {
            let (id, p) = pen(&s, n);
            assert!(s.set_pen(
                id,
                Pen {
                    color: [255, 0, 0],
                    ..p
                }
            ));
        }
        assert!(s.table().rev > rev);
        assert_eq!(s.build_count(run), builds, "kein Wandzug neu gerechnet");
        let after = s.mesh(ViewKind::Plan, None, &[]);
        assert_eq!(before.faces, after.faces);
        assert_eq!(before.edges, after.edges);
        let red = [1.0, 0.0, 0.0];
        let now = s.table().looks(1.0);
        assert_eq!(now.drawing.color[sk_model::edge_kind::CUT as usize], red);
        assert_eq!(
            now.drawing.color[sk_model::edge_kind::VIEW as usize],
            [0.0; 3]
        );
        let key = (after.faces[0][6] as usize) & 0x7FFF;
        assert_eq!(now.texels[2 * now.keys + key][..3], red, "Grund rot");
        // Rückgängig: alte Tabelle, wieder ohne Neuaufbau
        assert!(s.undo());
        assert!(s.undo());
        assert_eq!(s.table().looks(1.0), looks);
        assert_eq!(s.build_count(run), builds);
    }

    #[test]
    fn ruhendes_und_live_netz_ergeben_das_ganze() {
        let mut s = Scene::with_model(Model::with_seed(2));
        let a = s.add_wall(&rechteck(0.0)).unwrap();
        s.add_wall(&rechteck(10000.0)).unwrap();
        let whole = s.mesh(ViewKind::Persp, None, &[]);
        let rest = s.mesh(ViewKind::Persp, None, &[a]);
        let live = s.mesh_runs(ViewKind::Persp, None, &[a]);
        assert_eq!(whole.faces.len(), rest.faces.len() + live.faces.len());
        assert_eq!(whole.edges.len(), rest.edges.len() + live.edges.len());
    }

    #[test]
    fn strahl_liefert_die_wand() {
        let mut s = Scene::with_model(Model::with_seed(3));
        s.add_wall(&rechteck(10000.0)).unwrap();
        let a = s.add_wall(&rechteck(0.0)).unwrap();
        // Segment 2 läuft bei x = 5000 von y = 4000 nach 0; von außen (+x) treffen
        let hit = s.raycast(vec3(9000.0, 2000.0, 1000.0), vec3(-1.0, 0.0, 0.0));
        let (t, wall) = hit.unwrap();
        assert!((t - 4000.0).abs() < 1e-6, "{t}");
        assert_eq!(Some(wall), s.model().wall_at(a, 2));
        assert_eq!(s.model().element(wall).unwrap().number, "AW-007");
        // Am Modell vorbei: kein Treffer
        assert!(s
            .raycast(vec3(0.0, -9000.0, 5000.0), vec3(0.0, 0.0, 1.0))
            .is_none());
    }

    /// Ein neuer Akzent ändert die Zeichnung nicht: Die Tabelle übernimmt den
    /// Schema-Stand, Netze müssen nicht neu entstehen. Andere Bildpunkte je mm
    /// ändern die Strichbreiten.
    #[test]
    fn farbschema_aendern() {
        let mut s = Scene::with_model(Model::with_seed(8));
        s.add_wall(&rechteck(0.0)).unwrap();
        let mut theme = Theme::dark();
        assert!(!s.set_theme(&theme), "gleiches Schema");
        theme.set_accent(sk_paint::Rgba::rgb(40, 120, 220));
        assert!(!s.set_theme(&theme), "Akzent ohne neue Netze");
        assert_eq!(s.table().theme_rev, theme.rev);
        let width = s.table().edge_width(true, sk_model::edge_kind::CUT);
        theme.px_per_mm = 6.0;
        theme.rev += 1;
        assert!(s.set_theme(&theme), "Strichbreiten ändern sich");
        assert!(s.table().edge_width(true, sk_model::edge_kind::CUT) > width);
    }
}
