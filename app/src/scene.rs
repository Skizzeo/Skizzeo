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
use crate::visible::{split, Class};
use sk_math::{polygon, vec3, Vec3};
use sk_model::qto::{Schedule, Umfang};
use sk_model::view::{Isolate, Masks, Visibility};
use sk_model::{
    edge_kind, floor_qto_of, foundation_qto_of, merge_seam, run_qto, BuildingId, Category, Deleted,
    Direction, Edge, ElementId, FloorQto, FloorSlab, FootingQto, Foundation, LayerSetId, Model,
    Refusal, RunId, SlabQto, Solid, StoreyId, Touched, Tri, Txn, TypeCategory, WallChain, WallQto,
    COPING_PART, FLOOR_PART, FOOTING_PART, SLAB_PART, SOFFIT_PART, STRIP_PART, TERRACE_PART,
};
use sk_render::MeshData;
use sk_ui::theme::Theme;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

/// Anzeigename der Gründung als Ebene (Paneel „Geschosse“, Geschossbogen).
pub const FOUNDATION_NAME: &str = "Fundament";

/// Schnitthöhe des Grundrisses über der Unterkante des aktiven Geschosses (mm).
pub const PLAN_CUT: f64 = 1000.0;

type Aabb = (Vec3, Vec3);
/// Schnittebene: Punkt und Normale zum Betrachter.
pub type Plane = (Vec3, Vec3);

/// Abgeleitete Daten eines Wandzugs.
/// Waagerechter Abstand des Punkts `p` von der Strecke `a`–`b` (mm).
fn seg_dist(p: Vec3, a: Vec3, b: Vec3) -> f64 {
    let flat = |v: Vec3| vec3(v.x, v.y, 0.0);
    let (p, a, b) = (flat(p), flat(a), flat(b));
    let d = b - a;
    let t = ((p - a).dot(d) / d.dot(d).max(1e-12)).clamp(0.0, 1.0);
    (a + d * t - p).length()
}

/// Eine Dachterrasse im Grundriss (Szene, mm, auf OK Belag).
pub struct TerraceMark {
    /// Größter Teil des Umrisses.
    pub outline: Vec<Vec3>,
    /// Fläche aller Teile (mm²).
    pub area: f64,
    /// Blechkante außen davor: Anfang, Ende, Richtung nach außen.
    pub edge: Option<(Vec3, Vec3, Vec3)>,
}

struct RunCache {
    id: RunId,
    chain: WallChain,
    /// Körper für 3D und Ansichten.
    solid: Solid,
    /// Körper waagerecht geschnitten (Grundriss) je Schnitthöhe und Art,
    /// erst bei Bedarf berechnet; die zuletzt gefragten zuerst, damit der
    /// Wechsel in ein Nachbargeschoss nichts neu rechnet (E18).
    plan: Vec<(f64, PlanMode, Solid)>,
    /// Konturen des Wandschnitts im eigenen Geschoss (UK + 1 m), als
    /// Hintergrund des Geschosses darüber (E16); je Schnitthöhe.
    under: Vec<(f64, Vec<Edge>)>,
    /// Senkrechter Schnitt für die zuletzt gefragte Ebene.
    /// Schnittkörper der zuletzt gezeigten Ebenen (A und B), neueste zuerst.
    section: Vec<(Plane, Solid)>,
    bounds: Option<Aabb>,
    /// Äußerer Wandfuß je Segment (Gummiband).
    foot: Vec<(Vec3, Vec3)>,
    /// Umschließender Quader des Wandfußes (Vortest beim Greifen).
    foot_bounds: Option<Aabb>,
    /// Mengen je Segment; leer, solange der Zug gezogen wird.
    qto: Vec<WallQto>,
    /// Gekoppelter Zug darunter (gestapelte Außenwand, B12): kein eigenes
    /// Gummiband, die Schale läuft ohne Naht in ihn über.
    below: Option<RunId>,
    /// Gründung unter einem geschlossenen Außenwandzug (B9).
    found: Option<Foundation>,
    /// Mengen von Sohlplatte und Frostschürze (wie `qto` erst nach dem Ziehen).
    found_qto: Option<(SlabQto, FootingQto)>,
    /// Erdgeschossdecke über einem geschlossenen Außenwandzug (B10).
    floor: Option<FloorSlab>,
    floor_qto: Option<FloorQto>,
    /// Wie oft dieser Zug berechnet wurde (für Tests und Messung).
    builds: u32,
    /// Geteilte Körper (deckend, blass) je Ansicht, gültig für die Masken
    /// ihrer Teile und die Generation des Körpers (Review 3h G2, Befund 2):
    /// Ein gleich neu gebauter Zug übernimmt sie, eine Änderung teilt nur
    /// Züge mit neuem Körper neu, eine neue Revision allein nichts. Je
    /// Ansicht ein Eintrag, zuletzt gefragte zuerst.
    splits: RefCell<Vec<Split>>,
}

/// Körper, aus dem ein Zug geteilt wurde.
#[derive(Clone, Copy, Debug, PartialEq)]
enum SplitKey {
    /// 3D und Ansichten.
    Solid,
    Section(Plane),
    Plan(f64, PlanMode),
}

/// Ein geteilter Körper: wovon, für welche Masken je Teil, Ergebnis.
type Split = (SplitKey, Vec<(u32, Masks)>, Rc<[Solid; 2]>);

/// So viele geteilte Körper behält ein Zug (3D, Schnitt A und B, Grundriss).
const SPLIT_KEEP: usize = 4;

impl RunCache {
    fn new(
        id: RunId,
        chain: WallChain,
        found: Option<Foundation>,
        floor: Option<FloorSlab>,
        builds: u32,
    ) -> RunCache {
        let mut solid = chain.solid();
        for p in run_parts(&found, &floor) {
            p.solid(&mut solid);
        }
        // Auf Höhe des Wandfußes (Wände im OG stehen auf ihrem Geschoss)
        let lift = vec3(0.0, 0.0, chain.base);
        let foot: Vec<(Vec3, Vec3)> = chain
            .outer_foot()
            .into_iter()
            .map(|(a, b)| (a + lift, b + lift))
            .collect();
        let foot_bounds = foot
            .iter()
            .flat_map(|&(a, b)| [a, b])
            .fold(None, |acc, p| union(acc, Some((p, p))));
        RunCache {
            id,
            plan: Vec::new(),
            under: Vec::new(),
            bounds: solid.bounds(),
            foot,
            foot_bounds,
            solid,
            chain,
            section: Vec::new(),
            qto: Vec::new(),
            below: None,
            found,
            found_qto: None,
            floor,
            floor_qto: None,
            builds,
            splits: RefCell::default(),
        }
    }

    /// Gleicht der Zug `other` diesem in Eingaben und 3D-Körper (Bit für
    /// Bit)? Dann ist es dieselbe Generation des Körpers, und Grundriss und
    /// Schnitt entstehen gleich.
    fn same_body(&self, other: &RunCache) -> bool {
        self.chain == other.chain
            && self.found == other.found
            && self.floor == other.floor
            && same_solid(&self.solid, &other.solid)
    }

    /// Schon berechneter Grundrisskörper in Höhe `cut`.
    fn plan_at(&self, cut: f64, mode: PlanMode) -> Option<&Solid> {
        self.plan
            .iter()
            .find(|(c, m, _)| (*c, *m) == (cut, mode))
            .map(|(_, _, s)| s)
    }

    /// `s` (der Körper zu `key`) geteilt nach den Masken seiner Teile;
    /// gleiche Masken wie zuletzt: aus dem Speicher.
    fn split(&self, key: SplitKey, s: &Solid, vis: &Vis) -> Rc<[Solid; 2]> {
        let mut v = self.splits.borrow_mut();
        // Je Ansicht höchstens ein Eintrag; seine Teile sind die des Körpers
        // (gleiche Generation), nur ihre Masken sind zu prüfen
        if let Some(i) = v.iter().position(|(k, _, _)| *k == key) {
            let hit = v.remove(i);
            if hit.1.iter().all(|(p, m)| vis.masks(self.id, *p) == *m) {
                let out = hit.2.clone();
                v.insert(0, hit);
                return out;
            }
        }
        let mut parts: Vec<u32> = s
            .triangles
            .iter()
            .map(|t| t.elem)
            .chain(s.edges.iter().map(|e| e.elem))
            .collect();
        parts.sort_unstable();
        parts.dedup();
        let masks: Vec<(u32, Masks)> = parts
            .into_iter()
            .map(|p| (p, vis.masks(self.id, p)))
            .collect();
        // Masken je Teil schon aufgelöst (nach Teil sortiert)
        let out = Rc::new(split(s, |p, l| {
            masks
                .binary_search_by_key(&p, |x| x.0)
                .map_or(Class::Solid, |i| class_in(masks[i].1, l))
        }));
        v.truncate(SPLIT_KEEP - 1);
        v.insert(0, (key, masks, out.clone()));
        out
    }

    /// Schon berechneter Körper einer Ansicht (nach [`RunCache::view_solid`]).
    fn shown(&self, view: ViewKind, section: Option<Plane>) -> Option<&Solid> {
        match (view, section) {
            (ViewKind::Plan, _) => self.plan.first().map(|(_, _, s)| s),
            (ViewKind::Section, Some(pl)) => {
                self.section.iter().find(|(p, _)| *p == pl).map(|(_, s)| s)
            }
            (ViewKind::Section, None) => None,
            _ => Some(&self.solid),
        }
    }

    /// Körper einer Ansicht; `cut` ist die Schnitthöhe des Grundrisses,
    /// `mode` sagt, wie der Zug zum aktiven Geschoss liegt.
    fn view_solid(
        &mut self,
        view: ViewKind,
        section: Option<Plane>,
        cut: f64,
        mode: PlanMode,
    ) -> Option<&Solid> {
        match (view, section) {
            (ViewKind::Plan, _) => {
                let i = match self
                    .plan
                    .iter()
                    .position(|(c, m, _)| (*c, *m) == (cut, mode))
                {
                    Some(i) => i,
                    None => {
                        let s = self.plan_solid(cut, mode);
                        self.plan.truncate(PLAN_KEEP - 1);
                        self.plan.insert(0, (cut, mode, s));
                        0
                    }
                };
                self.plan.get(i).map(|(_, _, s)| s)
            }
            (ViewKind::Section, Some(pl)) => {
                if let Some(i) = self.section.iter().position(|(p, _)| *p == pl) {
                    let hit = self.section.remove(i);
                    self.section.insert(0, hit);
                } else {
                    let (p0, n) = pl;
                    let mut s = self.solid.clipped(p0, n);
                    s.append(&self.chain.section_caps(p0, n));
                    for p in run_parts(&self.found, &self.floor) {
                        p.section_caps(&mut s, p0, n);
                    }
                    self.section.truncate(SECTION_KEEP - 1);
                    self.section.insert(0, (pl, s));
                }
                self.section.first().map(|(_, s)| s)
            }
            (ViewKind::Section, None) => None,
            _ => Some(&self.solid),
        }
    }
}

impl RunCache {
    /// Grundrisskörper in Höhe `cut`. Im Fundament (E18) keine Wände; liegt
    /// der Zug unter dem aktiven Geschoss, nur die Teile ohne die Wand.
    fn plan_solid(&self, cut: f64, mode: PlanMode) -> Solid {
        let mut s = match mode {
            PlanMode::Cut => self.chain.solid_cut_at(cut),
            PlanMode::Foundation | PlanMode::Lower => Solid::default(),
        };
        for p in run_parts(&self.found, &self.floor) {
            p.plan(&mut s, cut, mode);
        }
        s
    }

    fn has_plan(&self, cut: f64, mode: PlanMode) -> bool {
        self.plan.iter().any(|(c, m, _)| (*c, *m) == (cut, mode))
    }

    /// Konturen des Wandschnitts in Höhe `cut` (nur die Schnittkanten), für
    /// den Hintergrund des Geschosses darüber; bei Bedarf berechnet.
    fn under_edges(&mut self, cut: f64) -> &[Edge] {
        let i = match self.under.iter().position(|(c, _)| *c == cut) {
            Some(i) => i,
            None => {
                let s = self.chain.solid_cut_at(cut);
                let at_cut = |p: Vec3| (p.z - cut).abs() < 1e-6;
                let edges = s
                    .edges
                    .into_iter()
                    .filter(|e| at_cut(e.a) && at_cut(e.b))
                    .collect();
                self.under.truncate(PLAN_KEEP - 1);
                self.under.insert(0, (cut, edges));
                0
            }
        };
        self.under.get(i).map_or(&[], |(_, e)| e)
    }
}

/// Abgeleitete Geometrie an einem Wandzug (Gründung, Decke mit Dämmung und
/// Streifen): hängt ihre Körper für 3D, Grundriss und senkrechten Schnitt an
/// `out` an, jeweils mit der Teilnummer je Bauteil (Review 2a, R2). Ein neues
/// Bauteil am Zug braucht nur eine Umsetzung und einen Eintrag in
/// [`run_parts`].
trait RunPart {
    /// Körper für 3D und Ansichten.
    fn solid(&self, out: &mut Solid);
    /// Grundriss in Schnitthöhe `cut`, je nachdem, wie der Zug zum aktiven
    /// Geschoss liegt.
    fn plan(&self, out: &mut Solid, cut: f64, mode: PlanMode);
    /// Schnittflächen der senkrechten Ebene durch `p0` mit Normale `n`.
    fn section_caps(&self, out: &mut Solid, p0: Vec3, n: Vec3);
}

/// Die Teile eines Zuges in fester Reihenfolge (sie bestimmt die Reihenfolge
/// im Netz).
fn run_parts<'a>(
    found: &'a Option<Foundation>,
    floor: &'a Option<FloorSlab>,
) -> impl Iterator<Item = &'a dyn RunPart> {
    let found = found.as_ref().map(|f| f as &dyn RunPart);
    let floor = floor.as_ref().map(|f| f as &dyn RunPart);
    found.into_iter().chain(floor)
}

impl RunPart for Foundation {
    fn solid(&self, out: &mut Solid) {
        out.append(&part(self.slab_solid(), SLAB_PART));
        out.append(&part(self.footing_solid(), FOOTING_PART));
    }

    fn plan(&self, out: &mut Solid, cut: f64, mode: PlanMode) {
        match mode {
            // Fundament (E18): die Schürze geschnitten, der Plattenrand
            // darüber als Hintergrundlinie
            PlanMode::Foundation => {
                out.append(&part(self.footing_cut_at(cut), FOOTING_PART));
                out.edges
                    .extend(self.slab_rim(cut + BACKGROUND_LIFT).into_iter().map(|e| {
                        sk_model::Edge {
                            elem: SLAB_PART,
                            ..e
                        }
                    }));
            }
            // Die Gründung liegt unter der Schnitthöhe: Draufsicht
            PlanMode::Cut => RunPart::solid(self, out),
            PlanMode::Lower => {}
        }
    }

    fn section_caps(&self, out: &mut Solid, p0: Vec3, n: Vec3) {
        let (slab, foot) = Foundation::section_caps(self, p0, n);
        out.append(&part(slab, SLAB_PART));
        out.append(&part(foot, FOOTING_PART));
    }
}

impl RunPart for FloorSlab {
    fn solid(&self, out: &mut Solid) {
        out.append(&part(FloorSlab::solid(self), FLOOR_PART));
        out.append(&part(self.soffit_solid(), SOFFIT_PART));
        out.append(&part(self.terrace_solid(), TERRACE_PART));
        out.append(&part(self.coping_solid(), COPING_PART));
        for k in 0..self.strips.len() {
            out.append(&part(self.strip_solid(k), STRIP_PART + k as u32));
        }
    }

    /// Die Decke liegt im EG über der Schnitthöhe und bleibt leer, im OG
    /// darunter; im Fundament gibt es sie nicht.
    fn plan(&self, out: &mut Solid, cut: f64, mode: PlanMode) {
        if mode == PlanMode::Foundation {
            return;
        }
        out.append(&part(self.solid_cut_at(cut), FLOOR_PART));
        out.append(&part(self.soffit_cut_at(cut), SOFFIT_PART));
        // Terrasse und Attikablech liegen über dem Boden des Geschosses
        // darüber und unter seiner Schnitthöhe: Ansichtslinien fein und in
        // gedimmter Tinte (Stil des Hintergrunds, vorschlag-dachterrasse §4)
        if mode == PlanMode::Lower {
            out.append(&part(seen_below(self.terrace_cut_at(cut)), TERRACE_PART));
            out.append(&part(seen_below(self.coping_solid()), COPING_PART));
        } else {
            out.append(&part(self.terrace_cut_at(cut), TERRACE_PART));
        }
        for k in 0..self.strips.len() {
            out.append(&part(self.strip_cut_at(k, cut), STRIP_PART + k as u32));
        }
    }

    fn section_caps(&self, out: &mut Solid, p0: Vec3, n: Vec3) {
        out.append(&part(FloorSlab::section_caps(self, p0, n), FLOOR_PART));
        out.append(&part(self.soffit_section_caps(p0, n), SOFFIT_PART));
        out.append(&part(self.terrace_section_caps(p0, n), TERRACE_PART));
        out.append(&part(self.coping_section_caps(p0, n), COPING_PART));
        for k in 0..self.strips.len() {
            let caps = self.strip_section_caps(k, p0, n);
            out.append(&part(caps, STRIP_PART + k as u32));
        }
    }
}

/// Kanten eines Körpers unter der Schnitthöhe als feine, gedimmte
/// Ansichtslinien.
fn seen_below(mut s: Solid) -> Solid {
    for e in &mut s.edges {
        e.kind = edge_kind::BACKGROUND;
    }
    s
}

/// Wie ein Wandzug zum aktiven Geschoss liegt (Grundriss).
#[derive(Clone, Copy, Debug, PartialEq)]
enum PlanMode {
    /// Im Geschoss oder darüber: waagerecht geschnitten.
    Cut,
    /// Ganz darunter: nur seine Decke (der Boden des aktiven Geschosses).
    Lower,
    /// Das Fundament ist aktiv: nur die Gründung geschnitten.
    Foundation,
}

/// So viele Grundrisse merkt sich ein Wandzug (aktives Geschoss, die beiden
/// Nachbarn und einen Rest).
const PLAN_KEEP: usize = 4;
/// So viele Schnittebenen behält ein Wandzug (Schnitt A und B).
const SECTION_KEEP: usize = 2;

/// Hintergrund im Grundriss knapp über dem Boden des aktiven Geschosses (mm):
/// über dessen Decke, unter den eigenen Wänden.
const BACKGROUND_LIFT: f64 = 10.0;

/// So viele Schritte lassen sich rückgängig machen.
const HISTORY: usize = 200;

/// Katalog mit den Operationen des Preisblatts und die Kostenblätter darauf
/// ([`Scene::kosten_live`]).
pub type KostenLive = (Rc<sk_cost::Katalog>, Vec<Rc<sk_cost::Kostenblatt>>);

/// Schlüssel eines gemerkten LV: Kostenblatt, Modellstand, Wahl.
type LvSchluessel = ((u64, u64, Umfang), u64, sk_cost::lv::LvWahl);

pub struct Scene {
    model: Model,
    /// Schritte für „Rückgängig“ (die ältesten fallen nach [`HISTORY`] weg).
    undo: Vec<Txn>,
    /// Rückgängig gemachte Schritte für „Wiederholen“.
    redo: Vec<Txn>,
    /// Zählt jeden neuen Schritt im Verlauf (Paket 8b, K3).
    serial: u64,
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
    /// Gebäude, dessen Dialog offen ist oder dessen Polygon gerade gezeichnet
    /// wird (ein offener Schritt „Gebäude erstellt“), und das vorher aktive
    /// Geschoss.
    pending: Option<(BuildingId, Option<StoreyId>)>,
    /// Vorgaben des Dialogs für dieses Gebäude.
    draft: BuildingDraft,
    /// Modellstand vor dem offenen Schritt „Gebäude erstellt“.
    pending_rev: u64,
    /// Mengenliste (B7) mit dem Modellstand, aus dem sie stammt.
    schedule: Option<(u64, Rc<Schedule>)>,
    /// Liste im zuletzt gefragten Umfang (KA-1) mit der Berechnung
    /// (`schedule_runs`), aus der sie gefiltert ist.
    schedule_in: Option<(u64, Umfang, Rc<Schedule>)>,
    /// Wie oft die Liste berechnet wurde (Messung, Abnahme).
    schedule_runs: u64,
    /// Kosten (KA-2): wirksamer Katalog je (`ext_revision`, Firmenstand, Umfeld),
    /// die letzten zwei Kostenblätter je (Berechnung der Liste,
    /// Katalogstempel, Umfang; der Reiter fragt den Umfang und für die
    /// Chip-Summen alle Geschosse) und der Zwischenspeicher der Zuordnung
    /// (Bausteingrenze §5).
    katalog: Option<((u64, u64, u64), Rc<sk_cost::Katalog>)>,
    kostenblatt: Vec<((u64, u64, Umfang), Rc<sk_cost::Kostenblatt>)>,
    /// Leistungsverzeichnisse (KA-4) je Kostenblatt, Modellstand (Kopf) und
    /// Wahl; der Reiter AVA fragt jedes Los für den Baum.
    lv: Vec<(LvSchluessel, Rc<sk_cost::lv::Lv>)>,
    /// Firmen- oder Werkskatalog ohne Projekt (Firmenwerte im Preisblatt).
    firmenkatalog: Option<((u64, u64, u64), Rc<sk_cost::Katalog>)>,
    /// Bezeichnungen mit Werten, je Wortlaut einmal ([`Scene::bezeichnung`]).
    bezeichnungen: Vec<&'static str>,
    kostenspeicher: sk_cost::Kostenspeicher,
    /// Offener Schritt „Typ gewechselt“ als Vorschau (K3).
    type_preview: bool,
    /// Uhr der Übergänge (ms seit Start), von der App vor jedem Ereignis gestellt.
    now: u64,
    /// Ansicht des zuletzt gezeichneten Netzes; für sie entsteht ein Übergang.
    shown_key: Option<MeshKey>,
    /// Laufender Übergang „Wände wachsen“ (K3b).
    grow: Option<Grow>,
    /// Hat [`Scene::mesh`] zuletzt ein gemischtes Netz geliefert?
    blend_shown: bool,
    /// Zähler der Sichtbarkeitsänderungen (Paket 3); ändert die Revision nicht.
    vis_rev: u64,
    /// Laufender Übergang beim Aus- und Einblenden oder Isolieren.
    vis_anim: Option<VisAnim>,
    /// Masken je Zug und Teil (Review 3a G2), gültig für Revision,
    /// Sichtbarkeit und Übergang.
    vis_table: RefCell<VisTable>,
    /// Ein Schritt hätte dieses gesperrte Bauteil geändert (Paket 4).
    locked_hit: Option<ElementId>,
    /// Blasses Netz zum zuletzt zusammengesetzten Modellnetz.
    ghost: Option<(MeshKey, VisStamp, MeshData)>,
}

/// Übergang der Sichtbarkeit (§2): ändert nur die Deckkraft des blassen
/// Netzes, nie die Netze selbst.
struct VisAnim {
    kind: VisFade,
    /// Sichtbarkeit davor.
    old: Visibility,
    start: u64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum VisFade {
    /// Bauteile verschwinden (1 → 0) bzw. erscheinen (0 → 1).
    Out,
    In,
    /// Der Rest wird blass bzw. wieder deckend.
    IsolateOn,
    IsolateOff,
}

/// Stand, für den Masken und blasses Netz gelten: Revision, Sichtbarkeit,
/// Übergang an oder aus.
type VisStamp = (u64, u64, bool);

#[derive(Default)]
struct VisTable {
    stamp: Option<VisStamp>,
    masks: HashMap<(RunId, u32), Masks>,
}

/// Sicht auf Modell, Übergang und Maskentabelle, getrennt von den
/// Zwischenspeichern der Züge.
struct Vis<'a> {
    model: &'a Model,
    anim: Option<&'a VisAnim>,
    table: &'a RefCell<VisTable>,
    stamp: VisStamp,
}

impl Vis<'_> {
    /// Masken eines Teils, je Stand einmal aufgelöst.
    fn masks(&self, run: RunId, part: u32) -> Masks {
        let mut t = self.table.borrow_mut();
        if t.stamp != Some(self.stamp) {
            t.stamp = Some(self.stamp);
            t.masks.clear();
        }
        *t.masks.entry((run, part)).or_insert_with(|| {
            let Some(id) = self.model.part_of(run, part) else {
                return Masks::ALL;
            };
            let m = self.model;
            match self.anim {
                None => m.masks(id),
                Some(a) => match a.kind {
                    VisFade::IsolateOn => m.masks(id),
                    VisFade::IsolateOff => m.masks_in(&a.old, id),
                    // Was sich ändert, liegt für den Übergang im blassen Netz
                    VisFade::Out | VisFade::In => {
                        let (o, n) = (m.masks_in(&a.old, id), m.masks(id));
                        Masks {
                            solid: o.solid & n.solid,
                            ghost: o.solid ^ n.solid,
                        }
                    }
                },
            }
        })
    }

    fn class(&self, run: RunId, part: u32, layer: u8) -> Class {
        class_in(self.masks(run, part), layer)
    }
}

/// Klasse einer Schicht nach den Masken ihres Teils.
fn class_in(m: Masks, layer: u8) -> Class {
    let bit = Masks::bit(layer);
    if m.solid & bit != 0 {
        Class::Solid
    } else if m.ghost & bit != 0 {
        Class::Ghost
    } else {
        Class::Hidden
    }
}

/// Ansicht, Schnittebene und ausgelassene Züge eines Netzes.
type MeshKey = (ViewKind, Option<Plane>, Vec<RunId>);

/// Übergang „Wände wachsen“ (K3b): Modell und Mengen sind sofort neu, nur das
/// gezeichnete Netz mischt sich in `anim_ms` vom alten zum neuen Stand; die
/// geänderten Wände leuchten in `flash_ms` aus.
struct Grow {
    key: MeshKey,
    from: MeshData,
    to: MeshData,
    /// Modellstand, zu dem `to` gehört; ändert sich das Modell, gilt `to` nicht mehr.
    rev: u64,
    start: u64,
    /// Passen beide Netze Punkt für Punkt zusammen? Sonst springt die Form
    /// sofort, und nur das Leuchten läuft.
    morph: bool,
    glow: Vec<ElementId>,
}

/// Schritte, die beim Rückgängigmachen denselben Übergang rückwärts zeigen.
const GROW_STEPS: [&str; 2] = [CATALOG_STEP, TYPE_STEP];

/// Verlaufseintrag „OK“ im Bauteilkatalog (K3).
pub const CATALOG_STEP: &str = "Bauteilkatalog geändert";
/// Verlaufseintrag Typwechsel in den Eigenschaften (K3).
pub const TYPE_STEP: &str = "Typ gewechselt";

/// Stärke des Nachleuchtens zu Beginn (wie der Schein der Rückfrage).
pub const GROW_GLOW: f32 = 0.43;

/// Netz zwischen `a` (k = 0) und `b` (k = 1): Lage und Texturkoordinaten
/// gemischt, Normale und Schlüssel von `b`.
fn blend(a: &MeshData, b: &MeshData, k: f32) -> MeshData {
    let mix = |x: f32, y: f32| x + (y - x) * k;
    MeshData {
        faces: a
            .faces
            .iter()
            .zip(&b.faces)
            .map(|(p, q)| {
                let mut v = *q;
                for i in [0, 1, 2, 7, 8] {
                    v[i] = mix(p[i], q[i]);
                }
                v
            })
            .collect(),
        edges: a
            .edges
            .iter()
            .zip(&b.edges)
            .map(|((p, _), (q, kind))| {
                let m =
                    |u: [f32; 3], w: [f32; 3]| [mix(u[0], w[0]), mix(u[1], w[1]), mix(u[2], w[2])];
                ([m(p[0], q[0]), m(p[1], q[1])], *kind)
            })
            .collect(),
    }
}

/// Lassen sich die Netze mischen? Gleich viele Punkte und Kanten, je Punkt
/// derselbe Baustoff und dieselbe Flächenrichtung, je Kante dieselbe Art.
/// Sonst hat sich die Gestalt geändert (andere Schichtzahl, Taschen, …).
fn morphable(a: &MeshData, b: &MeshData) -> bool {
    a.faces.len() == b.faces.len()
        && a.edges.len() == b.edges.len()
        && a.faces
            .iter()
            .zip(&b.faces)
            .all(|(p, q)| p[6] == q[6] && p[3] * q[3] + p[4] * q[4] + p[5] * q[5] > 0.999)
        && a.edges.iter().zip(&b.edges).all(|(p, q)| p.1 == q.1)
}

/// ease-out 1 − (1 − u)³
pub(crate) fn ease_out(u: f32) -> f32 {
    1.0 - (1.0 - u.clamp(0.0, 1.0)).powi(3)
}

/// Vorgaben im Dialog „Gebäude erstellen“ (Jörn 10:13, mm): lichte Höhen und
/// Deckendicken gehen sofort in die Geschossbänder, die Plattendicke beim
/// Schließen des Polygons in die Sohlplatte.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BuildingDraft {
    pub clear_eg: f64,
    pub clear_og: f64,
    pub floor_eg: f64,
    pub floor_og: f64,
    pub slab: f64,
}

impl Default for BuildingDraft {
    fn default() -> BuildingDraft {
        BuildingDraft {
            clear_eg: 2635.0,
            clear_og: 2635.0,
            floor_eg: sk_model::FLOOR_THICKNESS,
            floor_og: sk_model::FLOOR_THICKNESS,
            slab: sk_model::SLAB_THICKNESS,
        }
    }
}

/// Grenzen der Dialogfelder (mm): Decke 10–60 cm (B10), Sohlplatte 10–79 cm
/// (die Frostschürze bleibt unter der Platte, UK −0,80).
pub const DRAFT_FLOOR: (f64, f64) = (100.0, 600.0);
pub const DRAFT_SLAB: (f64, f64) = (100.0, 790.0);
pub const DRAFT_CLEAR: (f64, f64) = (sk_model::MIN_CLEAR, 10000.0);

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

/// Verlaufseintrag des Einstellungsfensters (E5).
pub const SETTINGS_STEP: &str = "Einstellungen geändert";

impl Scene {
    #[cfg(test)]
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
            serial: 0,
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
            pending: None,
            draft: BuildingDraft::default(),
            pending_rev: 0,
            schedule: None,
            schedule_in: None,
            schedule_runs: 0,
            katalog: None,
            kostenblatt: Vec::new(),
            lv: Vec::new(),
            firmenkatalog: None,
            bezeichnungen: Vec::new(),
            kostenspeicher: sk_cost::Kostenspeicher::default(),
            type_preview: false,
            now: 0,
            shown_key: None,
            grow: None,
            blend_shown: false,
            vis_rev: 0,
            vis_anim: None,
            vis_table: RefCell::default(),
            locked_hit: None,
            ghost: None,
        };
        s.rebuild_dirty(false);
        s
    }

    /// Mengenliste (B7): einmal je Modellstand berechnet und gemerkt, nie
    /// während eines offenen Schritts (Ziehen); dann bleibt der alte Stand.
    pub fn schedule(&mut self) -> &Schedule {
        let rev = self.model.revision();
        let fresh = matches!(&self.schedule, Some((r, _)) if *r == rev);
        if !fresh && (self.schedule.is_none() || !self.model.in_step()) {
            self.schedule = Some((rev, Rc::new(sk_model::qto::schedule(&self.model))));
            self.schedule_runs += 1;
        }
        &self.schedule.as_ref().expect("gerade berechnet").1
    }

    /// Mengenliste im Umfang `u` (KA-1): gefiltert aus [`Scene::schedule`]
    /// über `Schedule::restrict`, gemerkt je Berechnung und Umfang. Rechnet
    /// keine Geometrie; ein Chip-Klick rechnet die Liste nicht neu.
    pub fn schedule_in(&mut self, u: &Umfang) -> Rc<Schedule> {
        self.schedule();
        let runs = self.schedule_runs;
        let full = self.schedule.as_ref().expect("gerade berechnet").1.clone();
        if u.alles() {
            return full;
        }
        if let Some((r, w, s)) = &self.schedule_in {
            if *r == runs && w == u {
                return s.clone();
            }
        }
        let s = Rc::new(full.restrict(&self.model, u));
        self.schedule_in = Some((runs, u.clone(), s.clone()));
        s
    }

    /// Wirksamer Kostenkatalog (Projekt, Firma oder Werk), gemerkt je
    /// `ext_revision`, Firmenstand und Baustoffen und Gewerken des Modells
    /// (`lesen::umfeld_stempel`). `firma`: Firmenkatalog und sein Stand
    /// ([`crate::catalog::Company::stand`]).
    pub fn katalog(&mut self, firma: Option<(&sk_model::Library, u64)>) -> Rc<sk_cost::Katalog> {
        let key = (
            self.model.ext_revision(),
            firma.map_or(0, |f| f.1),
            sk_cost::lesen::umfeld_stempel(&self.model),
        );
        if let Some((k, kat)) = &self.katalog {
            if *k == key {
                return kat.clone();
            }
        }
        let kat = Rc::new(sk_cost::lesen::katalog(&self.model, firma.map(|f| f.0)));
        self.katalog = Some((key, kat.clone()));
        kat
    }

    /// Kostenblatt im Umfang `u` (KA-2, paket-ka2 §3): `lesen::kosten_mit`
    /// über die Liste aus [`Scene::schedule`] und den Zwischenspeicher,
    /// gemerkt je Berechnung der Liste, Katalogstempel und Umfang. Während
    /// eines offenen Schritts bleibt es wie die Liste beim letzten Stand.
    pub fn kostenblatt(
        &mut self,
        firma: Option<(&sk_model::Library, u64)>,
        u: &Umfang,
    ) -> Rc<sk_cost::Kostenblatt> {
        let kat = self.katalog(firma);
        self.schedule();
        let key = (self.schedule_runs, kat.stempel, u.clone());
        if let Some(i) = self.kostenblatt.iter().position(|(k, _)| *k == key) {
            let e = self.kostenblatt.remove(i);
            let b = e.1.clone();
            self.kostenblatt.insert(0, e);
            return b;
        }
        let sched = self.schedule.as_ref().expect("gerade berechnet").1.clone();
        let sp = std::mem::take(&mut self.kostenspeicher);
        let (b, sp) = sk_cost::lesen::kosten_mit(sp, &self.model, &sched, &kat, u);
        self.kostenspeicher = sp;
        let b = Rc::new(b);
        self.kostenblatt.insert(0, (key, b.clone()));
        self.kostenblatt.truncate(2);
        b
    }

    /// Leistungsverzeichnis des Loses `w.los` im Umfang `u` (KA-4,
    /// paket-ka4 §2): `lv::lv_aus` auf dem Kostenblatt desselben Umfangs,
    /// gemerkt je Kostenblatt, Modellstand und Wahl.
    pub fn lv(
        &mut self,
        firma: Option<(&sk_model::Library, u64)>,
        u: &Umfang,
        w: &sk_cost::lv::LvWahl,
    ) -> Rc<sk_cost::lv::Lv> {
        let kat = self.katalog(firma);
        let blatt = self.kostenblatt(firma, u);
        let von = (self.schedule_runs, kat.stempel, u.clone());
        let key: LvSchluessel = (von, self.model.revision(), w.clone());
        if let Some((_, lv)) = self.lv.iter().find(|(k, _)| *k == key) {
            return lv.clone();
        }
        // Beim Ziehen (offener Schritt) steht das Kostenblatt bis zum
        // Loslassen; Kopf und Geschossnamen ändern sich dabei nicht. Das
        // letzte LV derselben Wahl gilt weiter, statt jedes Bild alle Lose
        // neu zu ordnen (Review 3ao)
        if self.model.in_step() {
            if let Some((_, lv)) = self.lv.iter().find(|(k, _)| k.0 == key.0 && k.2 == key.2) {
                return lv.clone();
            }
        }
        let lv = Rc::new(sk_cost::lv::lv_aus(&self.model, &blatt, &kat, w));
        // Ein anderes Blatt oder ein anderer Stand macht die alten ungültig
        self.lv.retain(|(k, _)| k.0 == key.0 && k.1 == key.1);
        self.lv.insert(0, (key, lv.clone()));
        self.lv.truncate(8);
        lv
    }

    /// Katalog der Firma oder des Werks ohne die Werte des Projekts
    /// (`lesen::firma_oder_werk`), gemerkt wie [`Scene::katalog`]: die
    /// Firmenwerte im Preisblatt und der Punkt „im Projekt geändert“.
    pub fn firmenkatalog(
        &mut self,
        firma: Option<(&sk_model::Library, u64)>,
    ) -> Rc<sk_cost::Katalog> {
        let key = (
            self.model.ext_revision(),
            firma.map_or(0, |f| f.1),
            sk_cost::lesen::umfeld_stempel(&self.model),
        );
        if let Some((k, kat)) = &self.firmenkatalog {
            if *k == key {
                return kat.clone();
            }
        }
        let kat = Rc::new(sk_cost::lesen::firma_oder_werk(
            &self.model,
            firma.map(|f| f.0),
        ));
        self.firmenkatalog = Some((key, kat.clone()));
        kat
    }

    /// Kosten mit noch nicht ausgeführten Operationen (Preisblatt beim
    /// Tippen, paket-ka2 §4): der Katalog des Plans und je Umfang das
    /// Kostenblatt darauf. Modell, `revision` und Verlauf bleiben (Regel 94).
    pub fn kosten_live(
        &mut self,
        firma: Option<(&sk_model::Library, u64)>,
        ops: &[sk_cost::Op],
        umfaenge: &[&Umfang],
    ) -> Result<KostenLive, Vec<sk_cost::Befund>> {
        self.schedule();
        let sched = self.schedule.as_ref().expect("gerade berechnet").1.clone();
        // Preise und Aufwandswerte direkt auf dem wirksamen Katalog
        // (`Katalog::mit`), alles andere über den Plan
        let k = match self.katalog(firma).mit(ops) {
            Some(k) => k,
            None => {
                sk_cost::vorschau(&self.model, firma.map(|f| f.0), sk_cost::Rolle::Admin, ops)?
                    .katalog
            }
        };
        let k = Rc::new(k);
        // Gleiche Umfänge (Blatt und Chips ohne Abwahl) nur einmal rechnen
        let mut b: Vec<Rc<sk_cost::Kostenblatt>> = Vec::new();
        for (i, u) in umfaenge.iter().enumerate() {
            let gleich = umfaenge[..i].iter().position(|x| x == u);
            b.push(match gleich {
                Some(j) => b[j].clone(),
                None => Rc::new(sk_cost::lesen::kosten(&self.model, &sched, &k, u)),
            });
        }
        Ok((k, b))
    }

    /// Liegt die Liste hinter dem Modell („wird aktualisiert“)?
    pub fn schedule_stale(&self) -> bool {
        self.schedule
            .as_ref()
            .is_some_and(|(r, _)| *r != self.model.revision())
    }

    /// Anzahl der Berechnungen der Liste.
    pub fn schedule_runs(&self) -> u64 {
        self.schedule_runs
    }

    pub fn model(&self) -> &Model {
        &self.model
    }

    /// Aktives Geschoss; gibt es das gewählte nicht mehr, das EG des ersten
    /// Gebäudes.
    pub fn active_storey(&self) -> sk_model::StoreyId {
        let eg = self.model.defaults().storey;
        self.active
            .filter(|&id| self.model.storey(id).is_some())
            .unwrap_or(eg)
    }

    /// Ist das Fundament (die Gründung) das aktive Geschoss (E18)?
    pub fn foundation_active(&self) -> bool {
        self.is_foundation(self.active_storey())
    }

    fn is_foundation(&self, id: StoreyId) -> bool {
        self.model
            .storey(id)
            .is_some_and(|st| st.kind == sk_model::LevelKind::Foundation)
    }

    /// Warum das Wandwerkzeug im aktiven Geschoss nicht zeichnet (`None`:
    /// es zeichnet). Das Paneel „Werkzeuge“ zeigt denselben Satz.
    #[cfg(test)]
    pub fn wall_tool_block(&self) -> Option<String> {
        self.foundation_active()
            .then(|| "Im Fundament gibt es noch nichts zu zeichnen".to_string())
    }

    /// Aktives Gebäude: das des aktiven Geschosses (`None`: noch keines).
    pub fn active_building(&self) -> Option<BuildingId> {
        self.model.building_of(self.active_storey())
    }

    /// Ist das aktive Geschoss das EG seines Gebäudes?
    pub fn ground_active(&self) -> bool {
        let a = self.active_storey();
        self.model.ground_storey(a) == Some(a)
    }

    /// Wechselt in das Gebäude des Bauteils `e` (Auswahl, B12): aktiv wird
    /// das Geschoss des Bauteils, bei der Gründung das EG. `true`, wenn sich
    /// das Gebäude geändert hat.
    pub fn follow_selection(&mut self, e: ElementId) -> bool {
        let m = &self.model;
        let Some(st) = m.element(e).map(|x| x.storey) else {
            return false;
        };
        if m.building_of(st) == self.active_building() {
            return false;
        }
        let st = match m.storey(st).map(|s| s.kind) {
            Some(sk_model::LevelKind::Foundation) => m.ground_storey(st),
            _ => Some(st),
        };
        match st {
            Some(id) => {
                self.active = Some(id);
                true
            }
            None => false,
        }
    }

    /// Knopf „Gebäude“ → Dialog „Gebäude erstellen“ (B12, E16): öffnet den
    /// Schritt „Gebäude erstellt“ und legt das Gebäude mit GR, EG und OG an;
    /// das Paneel „Geschosse“ zeigt es sofort. Das Schließen des Polygons
    /// ([`Scene::add_wall_as`]) schließt den Schritt, [`Scene::cancel_building`]
    /// verwirft ihn.
    pub fn open_building_dialog(&mut self) {
        if self.pending.is_some() {
            return;
        }
        self.pending_rev = self.model.revision();
        self.begin("Gebäude erstellt");
        let b = self.model.add_building(2);
        self.pending = Some((b, self.active));
        self.active = self.model.ground_of(Some(b));
        self.draft = BuildingDraft::default();
        self.apply_draft();
    }

    /// Vorgaben des offenen Dialogs.
    pub fn building_draft(&self) -> BuildingDraft {
        self.draft
    }

    /// Ein Feld des Dialogs „Gebäude erstellen“ (mm): `lichte_eg`,
    /// `lichte_og`, `decke_eg`, `decke_og`, `sohlplatte`. Gilt sofort im
    /// Paneel „Geschosse“; `false` außerhalb der Grenzen oder ohne Dialog.
    pub fn set_building_dialog_value(&mut self, field: &str, mm: f64) -> bool {
        if self.pending.is_none() {
            return false;
        }
        let ok = |(lo, hi): (f64, f64)| mm.is_finite() && mm >= lo - 1e-9 && mm <= hi + 1e-9;
        let d = &mut self.draft;
        let (slot, range) = match field {
            "lichte_eg" => (&mut d.clear_eg, DRAFT_CLEAR),
            "lichte_og" => (&mut d.clear_og, DRAFT_CLEAR),
            "decke_eg" => (&mut d.floor_eg, DRAFT_FLOOR),
            "decke_og" => (&mut d.floor_og, DRAFT_FLOOR),
            "sohlplatte" => (&mut d.slab, DRAFT_SLAB),
            _ => return false,
        };
        if !ok(range) {
            return false;
        }
        *slot = mm;
        self.apply_draft();
        true
    }

    /// EG und OG des entstehenden Gebäudes: (EG, OG).
    fn draft_storeys(&self) -> Option<(StoreyId, Option<StoreyId>)> {
        let (b, _) = self.pending?;
        let eg = self.model.ground_of(Some(b))?;
        Some((eg, self.model.level_above(eg)))
    }

    /// Geschosshöhen aus den Vorgaben (lichte Höhe + Deckendicke).
    fn apply_draft(&mut self) {
        let d = self.draft;
        if let Some((eg, og)) = self.draft_storeys() {
            self.model.plan_storey_height(eg, d.clear_eg + d.floor_eg);
            if let Some(og) = og {
                self.model.plan_storey_height(og, d.clear_og + d.floor_og);
            }
        }
    }

    /// Das Polygon des entstehenden Gebäudes ist geschlossen: Deckendicken
    /// und Plattendicke aus dem Dialog (die Geschosshöhen stehen schon).
    fn apply_draft_parts(&mut self, eg_run: RunId) {
        let d = self.draft;
        let mut runs = vec![eg_run];
        runs.extend(self.model.stack_above(eg_run));
        for r in runs {
            let Some(floor) = self.model.floor_of(r) else {
                continue;
            };
            let st = self.model.run(r).map(|x| x.storey);
            let ground = st.is_some_and(|s| self.model.ground_storey(s) == Some(s));
            let t = if ground { d.floor_eg } else { d.floor_og };
            self.model.set_floor_thickness(floor, t);
        }
        if let Some((slab, _)) = self.model.foundation_of(eg_run) {
            self.model.set_slab_thickness(slab, d.slab);
        }
    }

    /// Deckendicke aus den Vorgaben, solange das Gebäude entsteht.
    fn draft_floor(&self, id: StoreyId) -> Option<f64> {
        let (eg, og) = self.draft_storeys()?;
        if id == eg {
            Some(self.draft.floor_eg)
        } else if Some(id) == og {
            Some(self.draft.floor_og)
        } else {
            None
        }
    }

    /// Bricht Dialog oder Polygon ab: nichts bleibt, kein Verlaufseintrag.
    pub fn cancel_building(&mut self) {
        if let Some((_, prev)) = self.pending.take() {
            self.rollback();
            self.active = prev;
        }
    }

    /// Ist ein Gebäude im Entstehen (Dialog offen oder Polygon begonnen)?
    pub fn building_pending(&self) -> bool {
        self.pending.is_some()
    }

    /// Modellstand für die Titelleiste: ein Gebäude im Entstehen zählt erst
    /// mit dem geschlossenen Polygon als Änderung.
    pub fn shown_revision(&self) -> u64 {
        if self.pending.is_some() {
            self.pending_rev
        } else {
            self.model.revision()
        }
    }

    /// Macht ein Geschoss aktiv (auch das Fundament, E18); `false`, wenn es
    /// schon aktiv ist oder es nicht gibt.
    pub fn set_active_storey(&mut self, id: sk_model::StoreyId) -> bool {
        let ok = self.model.storey(id).is_some();
        if !ok || id == self.active_storey() {
            return false;
        }
        self.active = Some(id);
        true
    }

    /// Schnitt der Ansicht „Schnitt“ (steht mit in der Datei).
    pub fn active_cut(&self) -> usize {
        self.model.active_cut()
    }

    /// Anderen Schnitt zeigen; `false`, wenn es ihn nicht gibt oder er es
    /// schon ist.
    pub fn set_active_cut(&mut self, i: usize) -> bool {
        if i >= sk_model::CUT_NAMES.len() || i == self.model.active_cut() {
            return false;
        }
        self.model.set_active_cut(i);
        true
    }

    /// Lage und Blickrichtung eines Schnitts für die Datei merken (ohne
    /// Schritt, wie die Kamera).
    pub fn set_cut(&mut self, i: usize, c: sk_model::Cut) {
        self.model.set_cut(i, c);
    }

    /// Arbeitsebene des Wandwerkzeugs: (UK, Geschosshöhe) des aktiven
    /// Geschosses in mm.
    pub fn work_plane(&self) -> (f64, f64) {
        let id = self.active_storey();
        self.model
            .storey(id)
            .map_or((0.0, crate::wall_tool::WALL_HEIGHT), |st| {
                (st.elevation, st.height)
            })
    }

    /// Schnitthöhe des Grundrisses: Unterkante des aktiven Geschosses + 1 m,
    /// im Fundament die Mitte der Frostschürze (E18).
    pub fn plan_cut(&self) -> f64 {
        self.plan_cut_of(self.active_storey())
    }

    /// Schnitthöhe des Grundrisses, wenn `id` aktiv ist. Im Fundament über
    /// den Höhenbezug (Prüfregel 14): UK Fundament + halbe Tiefe bis UK
    /// Sohlplatte (die Mitte der Frostschürze), so folgt der Schnitt
    /// Plattendicke und Gründungstiefe.
    fn plan_cut_of(&self, id: StoreyId) -> f64 {
        let Some(st) = self.model.storey(id) else {
            return PLAN_CUT;
        };
        let at = if st.kind == sk_model::LevelKind::Foundation {
            let slab = self.model.max_slab_thickness(id);
            sk_model::LevelRef {
                storey: id,
                edge: sk_model::LevelEdge::Bottom,
                offset: (st.height - slab) / 2.0,
            }
        } else {
            sk_model::LevelRef {
                storey: id,
                edge: sk_model::LevelEdge::Bottom,
                offset: PLAN_CUT,
            }
        };
        self.model.level_z(at).unwrap_or(PLAN_CUT)
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

    /// Projektangaben (Bauvorhaben, Bauherr, Aufsteller; KA-4c) als ein
    /// Schritt. `false`, wenn sich nichts geändert hat.
    pub fn projekt_setzen(&mut self, label: &'static str, p: sk_model::Project) -> bool {
        self.begin(label);
        let ok = self.model.set_project(p);
        self.commit();
        ok
    }

    /// Öffnet den Schritt des Einstellungsfensters (E5, auch nach
    /// „Übernehmen“). Liefert die Revision zu Beginn für [`Scene::cancel_settings`].
    pub fn begin_settings(&mut self) -> u64 {
        self.begin(SETTINGS_STEP);
        self.model.revision()
    }

    /// Eine Eingabe im Einstellungsfenster: ändert das Modell im offenen
    /// Schritt; die Zeichentabelle folgt sofort (ohne Netzneubau).
    pub fn edit_attr(&mut self, f: impl FnOnce(&mut Model) -> bool) -> bool {
        debug_assert!(self.model.in_step(), "Eingabe ohne Schritt");
        let ok = f(&mut self.model);
        if self.table.rev != self.model.attr().rev() {
            self.table = DrawTable::resolve(&self.model, &self.theme);
        }
        ok
    }

    /// „Abbrechen“ im Einstellungsfenster: alles seit [`Scene::begin_settings`]
    /// zurück, ohne Verlaufseintrag, und die Revision wie vorher (Titel ohne
    /// „•“). Im Schritt ändern sich nur Attribute; Zwischenstände der Revision
    /// hat nichts behalten, was von Wänden oder Mengen abhängt.
    pub fn cancel_settings(&mut self, rev: u64) {
        self.rollback();
        self.model.restore_revision(rev);
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
                rc.below = self.model.run_below(id);
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
                // Gleicher Körper (ein Nachbar wurde neu gerechnet, dieser Zug
                // blieb): er behält seine Generation und damit die geteilten
                // Körper (Review 3h Befund 2)
                if let Some(old) = self.cache[slot].as_mut() {
                    if old.id == id && old.same_body(&rc) {
                        rc.splits = std::mem::take(&mut old.splits);
                    }
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

    /// Mengen einer Untersichtdämmung (OG Phase 2), aus der gezeichneten Decke.
    pub fn soffit_qto(&self, soffit: ElementId) -> Option<sk_model::SoffitQto> {
        let m = &self.model;
        let sk_model::ElementKind::SoffitInsulation { floor } = m.element(soffit)?.kind else {
            return None;
        };
        let run = m.run_of(floor)?;
        sk_model::soffit_qto_of(self.floor(run)?)
    }

    /// Mengen einer Dachterrasse, aus der gezeichneten Decke.
    pub fn terrace_qto(&self, terrace: ElementId) -> Option<sk_model::TerraceQto> {
        let m = &self.model;
        let sk_model::ElementKind::RoofTerrace { floor } = m.element(terrace)?.kind else {
            return None;
        };
        let run = m.run_of(floor)?;
        sk_model::terrace_qto_of(m, floor, self.floor(run)?)
    }

    /// Dachterrassen im Grundriss des aktiven Geschosses (sie liegen auf
    /// seinem Boden), für die Angabe „Dachterrasse 13,22 m²“: je Terrasse der
    /// größte Teil und die Fläche, dazu die Sperrflächen im Grundriss (Wände
    /// des Geschosses, Attika samt Blech), alles auf OK Belag.
    pub fn terrace_marks(&self) -> (Vec<TerraceMark>, Vec<[Vec3; 4]>) {
        let active = self.active_storey();
        let m = &self.model;
        let mut marks = Vec::new();
        let mut blocked = Vec::new();
        let mut z = None;
        for c in self.cache.iter().flatten() {
            if self.plan_mode(c.id, active) != PlanMode::Lower {
                continue;
            }
            let Some(f) = c.floor.as_ref() else { continue };
            let Some((_, top)) = f.terrace_band() else {
                continue;
            };
            z = Some(top);
            let at = |p: Vec3| vec3(p.x, p.y, top);
            // Blechband von der Innenkante der Attika bis zur Tropfkante
            let w = f.terraces.width;
            let mut edges = Vec::new();
            for cp in &f.terraces.coping {
                let n = cp.points.len();
                let segs = if cp.closed { n } else { n.saturating_sub(1) };
                for k in 0..segs {
                    let (a, b) = (cp.points[k], cp.points[(k + 1) % n]);
                    let d = vec3(b.x - a.x, b.y - a.y, 0.0);
                    if d.length() < 1.0 {
                        continue;
                    }
                    let r = vec3(d.y, -d.x, 0.0).normalized();
                    let (i, o) = (r * -(w + 3.0), r * sk_model::COPING_DRIP);
                    blocked.push([at(a + i), at(b + i), at(b + o), at(a + o)]);
                    edges.push((at(a + o), at(b + o), r));
                }
            }
            for t in &f.terraces.outlines {
                let Some(largest) = t
                    .parts
                    .iter()
                    .max_by(|a, b| polygon::area(a).total_cmp(&polygon::area(b)))
                else {
                    continue;
                };
                let outline: Vec<Vec3> = largest.iter().map(|p| at(*p)).collect();
                // Die Blechkante vor dieser Terrasse: die längste in ihrer Nähe
                let near = |e: &(Vec3, Vec3, Vec3)| {
                    let mid = (e.0 + e.1) * 0.5;
                    outline
                        .iter()
                        .zip(outline.iter().cycle().skip(1))
                        .map(|(p, q)| seg_dist(mid, *p, *q))
                        .fold(f64::MAX, f64::min)
                        <= w + sk_model::COPING_DRIP + 50.0
                };
                let edge = edges
                    .iter()
                    .filter(|e| near(e))
                    .max_by(|a, b| (a.1 - a.0).length().total_cmp(&(b.1 - b.0).length()))
                    .copied();
                marks.push(TerraceMark {
                    outline,
                    area: t.area(),
                    edge,
                });
            }
        }
        let Some(z) = z else {
            return (marks, blocked);
        };
        for c in self.cache.iter().flatten() {
            if m.run(c.id).map(|r| r.storey) != Some(active) {
                continue;
            }
            for k in 0..c.chain.segment_count() {
                if let Some(q) = c.chain.segment_footprint(k) {
                    blocked.push(q.map(|p| vec3(p.x, p.y, z)));
                }
            }
        }
        (marks, blocked)
    }

    /// Mengen eines Attikablechs, aus der gezeichneten Decke.
    pub fn coping_qto(&self, coping: ElementId) -> Option<sk_model::CopingQto> {
        let m = &self.model;
        let sk_model::ElementKind::Coping { floor } = m.element(coping)?.kind else {
            return None;
        };
        let run = m.run_of(floor)?;
        sk_model::coping_qto_of(m, floor, self.floor(run)?)
    }

    /// Mengen eines Randdämmstreifens (K5), aus der gezeichneten Decke.
    pub fn edge_strip_qto(&self, strip: ElementId) -> Option<sk_model::EdgeStripQto> {
        let m = &self.model;
        let sk_model::ElementKind::EdgeStrip { wall, floor } = m.element(strip)?.kind else {
            return None;
        };
        let (run, seg) = m.segment_of(wall)?;
        (m.floor_of(run) == Some(floor)).then_some(())?;
        sk_model::edge_strip_qto_of(self.floor(run)?, seg)
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
        // Gestapelte Züge folgen dem EG: kein eigenes Gummiband (E16, A52)
        self.cache
            .iter()
            .flatten()
            .filter(|c| c.below.is_none())
            .map(|c| (c.id, c.foot_bounds, c.foot.as_slice()))
    }

    /// Wandfüße der gestapelten Züge (OG): ohne Band, nur der Hinweis
    /// „gekoppelt: am EG-Wandfuß ziehen“ (A52).
    pub fn stacked_feet(
        &self,
    ) -> impl Iterator<Item = (RunId, Option<Aabb>, &[(Vec3, Vec3)])> + '_ {
        self.cache
            .iter()
            .flatten()
            .filter(|c| c.below.is_some())
            .map(|c| (c.id, c.foot_bounds, c.foot.as_slice()))
    }

    /// Öffnet einen Schritt für „Rückgängig“ (z. B. beim Greifen am Gummiband).
    /// Ist noch einer offen (Loslassen ging verloren, weil das Fenster beim
    /// Ziehen die Maus verlor), wird er vorher abgeschlossen und bleibt im Verlauf.
    pub fn begin(&mut self, label: &'static str) {
        // Eine andere Änderung, während das Gebäude entsteht, bricht es ab
        self.cancel_building();
        if self.model.in_step() {
            self.commit();
        }
        self.live.clear();
        self.model.begin(label);
    }

    /// Bezeichnung mit Werten für „Rückgängig: …“ („Lohn 65,00 €/h für
    /// dieses und neue Häuser“, Bedienbarkeit 4.4). Der Verlauf hält
    /// `&'static str`; jeder Wortlaut wird einmal angelegt und danach
    /// wiederverwendet, so bleibt der Speicher durch die Zahl verschiedener
    /// ausdrücklicher Firmenänderungen begrenzt.
    pub fn bezeichnung(&mut self, text: String) -> &'static str {
        if let Some(l) = self.bezeichnungen.iter().find(|l| **l == text) {
            return l;
        }
        let l: &'static str = Box::leak(text.into_boxed_str());
        self.bezeichnungen.push(l);
        l
    }

    /// Schließt den offenen Schritt und legt ihn in den Verlauf, falls er etwas
    /// geändert hat. Berechnet ausstehende Mengen und Wandzüge.
    pub fn commit(&mut self) {
        match self.model.try_commit() {
            Ok(Some(t)) => {
                self.undo.push(t);
                self.serial += 1;
                if self.undo.len() > HISTORY {
                    self.undo.remove(0);
                }
                self.redo.clear();
            }
            Ok(None) => {}
            // Sicherheitsnetz (Paket 4 §2.2): zurückgerollt, die App zeigt
            // den Hinweis
            Err(sk_model::Locked(id)) => {
                self.locked_hit = Some(id);
                self.mark_all();
            }
        }
        self.live.clear();
        self.rebuild_dirty(false);
        self.settle();
    }

    /// Zähler der Schritte im Verlauf: wächst mit jedem neuen Schritt
    /// (Nachkorrektur nur, solange er gleich bleibt; Paket 8b, K3).
    pub fn step_serial(&self) -> u64 {
        self.serial
    }

    /// „Wiederherstellen“ leeren (Nachkorrektur ohne neuen Schritt, K4).
    pub fn clear_redo(&mut self) {
        self.redo.clear();
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
    ///
    /// Außenwände entstehen im EG des aktiven Gebäudes (ein geschlossener Zug
    /// mit allen Geschossen darüber, B12), Innenwände im aktiven Geschoss des
    /// Gebäudes, in dessen Umriss ihr Anfang liegt. Ist ein Gebäude im
    /// Entstehen, schließt die Wand dessen Schritt „Gebäude erstellt“.
    #[cfg(test)]
    pub fn add_wall_as(&mut self, w: &WallChain, cat: Category) -> Option<RunId> {
        self.add_wall_typed(w, cat, None)
    }

    /// Wie [`Scene::add_wall_as`] mit dem Typ `set` (K3: im Werkzeug
    /// gewählt); `None` oder ein unpassender Typ: der Standardtyp.
    pub fn add_wall_typed(
        &mut self,
        w: &WallChain,
        cat: Category,
        set: Option<LayerSetId>,
    ) -> Option<RunId> {
        let pending = self.pending.take();
        if pending.is_none() {
            self.begin("Wand zeichnen");
        }
        let tc = TypeCategory::of(cat).unwrap_or(TypeCategory::ExteriorWall);
        let set = set
            .filter(|s| self.model.layer_set(*s).is_some_and(|t| t.category == tc))
            .unwrap_or_else(|| self.model.default_type(tc));
        let active = self.active_storey();
        let storey = match (cat, w.points.first()) {
            (Category::InteriorWall, Some(p)) => self.model.storey_at(*p, active),
            _ => self.model.ground_storey(active).unwrap_or(active),
        };
        let run = self
            .model
            .add_wall_run(&w.points, w.closed, w.ref_side, storey, set, cat);
        if run.is_none() && pending.is_some() {
            // Nichts angelegt: das Gebäude entsteht weiter
            self.pending = pending;
            return None;
        }
        if let (Some(id), Some(_)) = (run, pending) {
            self.apply_draft_parts(id);
        }
        if let Some(id) = run {
            for r in self.model.stack_above(id) {
                self.mark(r);
            }
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

    /// Typwechsel als Vorschau (K3: Typ in den Eigenschaften überfahren):
    /// ein offener Schritt „Typ gewechselt“, neu gerechnet werden nur die
    /// Züge, die er berührt, ohne Mengen. Eine vorige Vorschau wird vorher
    /// verworfen. `false`, wenn ein Zug den Typ nicht annimmt.
    pub fn preview_run_type(&mut self, runs: &[RunId], id: LayerSetId) -> bool {
        self.grown(true, |s| s.preview_run_type_now(runs, id))
    }

    fn preview_run_type_now(&mut self, runs: &[RunId], id: LayerSetId) -> bool {
        if std::mem::take(&mut self.type_preview) {
            self.rollback();
        }
        self.begin(TYPE_STEP);
        self.type_preview = true;
        // Auch Anschlüsse, die der Wechsel löst
        let mut marks: Vec<RunId> = runs
            .iter()
            .flat_map(|r| self.model.joined_runs(*r))
            .collect();
        let mut ok = true;
        for &r in runs {
            ok &= self.model.set_run_type(r, id);
        }
        marks.extend(self.model.step_touched());
        for r in marks {
            self.mark(r);
        }
        self.rebuild_dirty(true);
        ok
    }

    /// Beendet die Vorschau: `keep` übernimmt sie als einen Schritt, sonst
    /// ist alles wie vorher.
    pub fn end_run_type(&mut self, keep: bool) {
        if !std::mem::take(&mut self.type_preview) {
            return;
        }
        if keep {
            // Körper stehen schon, es fehlen nur die Mengen
            self.commit();
        } else {
            self.grown(true, |s| s.rollback());
        }
    }

    /// Läuft eine Typ-Vorschau?
    pub fn previewing_type(&self) -> bool {
        self.type_preview
    }

    /// Farbschema, mit dem die Zeichentabelle aufgelöst ist.
    pub fn theme(&self) -> &Theme {
        &self.theme
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
        if field == Field::Offset {
            return self.stack_wall(id).is_some_and(|w| self.type_offset(w, mm));
        }
        if field == Field::Soffit {
            return self.set_soffit(id, mm);
        }
        if matches!(
            field,
            Field::TerraceInsulation | Field::TerraceFinish | Field::Upstand
        ) {
            return self.set_terrace_field(id, field, mm);
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

    /// Ebenen des aktiven Gebäudes (ohne Gebäude: der Vorlage, etwa nach
    /// „Gebäude löschen“) von unten nach oben. Paneel „Geschosse“ und
    /// Geschossbogen zeigen genau diese.
    pub fn level_ids(&self) -> Vec<sk_model::StoreyId> {
        self.model.group_levels(self.active_storey())
    }

    /// Inhalt des Paneels „Geschosse“: Bänder, lichte Höhe des EG und die
    /// änderbaren Zahlen mit ihren Grenzen.
    pub fn levels(&self) -> crate::ui::Levels {
        use crate::ui::{Band, FieldRow, Levels};
        let m = &self.model;
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
        for id in self.level_ids() {
            let Some(st) = m.storey(id) else {
                continue;
            };
            let foundation = st.kind == sk_model::LevelKind::Foundation;
            l.bands.push(Band {
                id,
                // Jörns Wort (E18); im Modell bleibt es die Gründung
                name: if foundation {
                    FOUNDATION_NAME.to_string()
                } else {
                    st.short.clone()
                },
                bottom: st.elevation,
                top: st.top(),
                foundation,
                active: id == active,
            });
            if foundation {
                let (lo, hi) = m.foundation_bottom_range_of(id);
                l.fields.push(row(
                    Field::LevelBottom,
                    "UK Fundament",
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
            let clear = self.draft_floor(id).map_or(m.clear_height(id), |t| h - t);
            let t = h - clear;
            l.clear.push((id, clear));
            let range = (lo - e - t, hi - e - t);
            l.fields
                .push(row(Field::ClearHeight(id), "lichte Höhe", clear, range));
        }
        l
    }

    /// Zahl aus dem Paneel „Geschosse“ als ein Schritt; `false`, wenn das
    /// Modell sie ablehnt.
    pub fn set_level(&mut self, field: Field, mm: f64) -> bool {
        match field {
            Field::LevelBottom => match self.model.foundation_level_of(self.active_storey()) {
                Some(gr) => self.edit_model("UK Fundament", |m| m.set_foundation_bottom_of(gr, mm)),
                None => false,
            },
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

    /// Dicke der Untersichtdämmung an der Decke `id` (oder an der Decke der
    /// Untersichtdämmung `id`): ein Schritt „Untersichtdämmung“; die
    /// Außenschichten wandern mit.
    /// Feld im Abschnitt „Aufbau“ der Dachterrasse (D1, H152): Dämmung und
    /// Belag ändern ihren Typ, die Attika steht an der Decke. Ein Schritt.
    pub fn set_terrace_field(&mut self, id: ElementId, field: Field, mm: f64) -> bool {
        use sk_model::ElementKind::{Coping, Floor, RoofTerrace};
        let m = &self.model;
        let floor = match m.element(id).map(|e| &e.kind) {
            Some(RoofTerrace { floor } | Coping { floor }) => *floor,
            Some(Floor(_)) => id,
            _ => return false,
        };
        if field == Field::Upstand {
            return self.edit_model("Attika", |m| m.set_floor_upstand(floor, mm));
        }
        let func = match field {
            Field::TerraceInsulation => sk_model::LayerFunction::Insulation,
            _ => sk_model::LayerFunction::Finish,
        };
        let Some(t) = m.terrace_type_of(floor) else {
            return false;
        };
        let Some(mut set) = m.layer_set(t).cloned() else {
            return false;
        };
        match set.layers.iter_mut().find(|l| l.function == func) {
            Some(l) if l.thickness != mm => l.thickness = mm,
            _ => return false,
        }
        self.edit_model("Aufbau der Dachterrasse", |m| m.set_layer_set(t, set))
    }

    pub fn set_soffit(&mut self, id: ElementId, mm: f64) -> bool {
        let m = &self.model;
        let floor = match m.element(id).map(|e| &e.kind) {
            Some(sk_model::ElementKind::SoffitInsulation { floor }) => *floor,
            Some(sk_model::ElementKind::Floor(_)) => id,
            _ => return false,
        };
        let Some(sk_model::ElementKind::Floor(f)) = m.element(floor).map(|e| &e.kind) else {
            return false;
        };
        if f.soffit.thickness == mm {
            return false;
        }
        self.begin("Untersichtdämmung");
        let ok = self.model.set_floor_soffit(floor, mm);
        for r in self.model.step_touched() {
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

    /// Entf bzw. „Löschen“ (Paket „Löschen“): löscht, was von `ids` löschbar
    /// ist, in einem Schritt „Bauteil gelöscht“ bzw. „N Bauteile gelöscht“.
    /// Ist nichts löschbar, entsteht kein Schritt und das Modell bleibt
    /// unberührt. Neu gerechnet werden die betroffenen Züge und ihre Partner.
    pub fn delete_elements(&mut self, ids: &[ElementId]) -> Deleted {
        let m = &self.model;
        let mut ok: Vec<ElementId> = Vec::new();
        for &id in ids {
            if m.can_delete(id).is_ok() && !ok.contains(&id) {
                ok.push(id);
            }
        }
        if ok.is_empty() {
            let mut d = Deleted::default();
            for &id in ids {
                match m.can_delete(id) {
                    Err(r) if r != Refusal::Missing && !d.refused.iter().any(|x| x.0 == id) => {
                        d.refused.push((id, r))
                    }
                    _ => {}
                }
            }
            return d;
        }
        let mut marks: Vec<RunId> = Vec::new();
        for &id in &ok {
            if let Some((r, _)) = m.segment_of(id) {
                marks.push(r);
                marks.extend(m.joined_runs(r));
            }
        }
        let label = match ok.len() {
            1 => "Bauteil gelöscht",
            n => sk_model::step_label(format!("{n} Bauteile gelöscht")),
        };
        self.begin(label);
        let d = self.model.delete_elements(ids);
        marks.extend(self.model.step_touched());
        for r in marks {
            self.mark(r);
        }
        self.commit();
        d
    }

    /// „Gebäude löschen“ (Paket „Löschen“): ein Schritt „Gebäude gelöscht“.
    pub fn remove_building(&mut self, b: BuildingId) -> bool {
        if self.model.building(b).is_none() {
            return false;
        }
        self.begin("Gebäude gelöscht");
        let ok = self.model.remove_building(b);
        self.mark_all();
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

    /// Kosten (KA-0d, Bausteingrenze §5): der eine Eingang der App für
    /// Stammdaten im Projekt. Eine Operation ist genau ein Rückgängig-Schritt
    /// mit `op.bezeichnung()`; ein Fehler ändert nichts und liefert die
    /// Befunde. Bis KA-3 immer als Administrator.
    #[cfg_attr(not(test), allow(dead_code))] // Kostenreiter (KA-1)
    pub fn kosten(
        &mut self,
        firma: Option<&sk_model::Library>,
        herkunft: &sk_cost::Herkunft,
        op: sk_cost::Op,
    ) -> Result<(), Vec<sk_cost::Befund>> {
        let label = sk_model::step_label(op.bezeichnung());
        self.kosten_folge(label, firma, herkunft, std::slice::from_ref(&op))
    }

    /// Mehrere Kostenoperationen als **ein** Schritt `label`; scheitert eine,
    /// bleibt nichts (Abnahme 13).
    #[cfg_attr(not(test), allow(dead_code))] // Kostenreiter (KA-1)
    pub fn kosten_folge(
        &mut self,
        label: &'static str,
        firma: Option<&sk_model::Library>,
        herkunft: &sk_cost::Herkunft,
        ops: &[sk_cost::Op],
    ) -> Result<(), Vec<sk_cost::Befund>> {
        // Erst prüfen: ein abgelehnter Plan öffnet keinen Schritt
        sk_cost::vorschau(&self.model, firma, sk_cost::Rolle::Admin, ops)?;
        self.begin(label);
        match sk_cost::ausfuehren_folge(
            &mut self.model,
            firma,
            sk_cost::Rolle::Admin,
            herkunft,
            ops,
        ) {
            Ok(()) => {
                self.commit();
                Ok(())
            }
            Err(b) => {
                self.rollback();
                Err(b)
            }
        }
    }

    /// Ausdrücklich „Auch für neue Häuser“ (Bausteingrenze §5, KA-2c2):
    /// prüfen, dann die Firma als neuer Stand mit `[log]`
    /// ([`crate::catalog::Company::fuer_firma`]), dann, nur mit
    /// Projektkopie, ein Projektschritt `StandUebernehmen` mit `label`. Die
    /// Firma ist nie Teil des Rückgängig-Schritts; Strg+Z nimmt nur das
    /// Projekt zurück. `Err`: nichts geschrieben, der Satz für die Meldung.
    /// `Ok`: ein Hinweis für die Statuszeile, wenn es einen gibt.
    ///
    /// Werte, die dieses Haus selbst abweichend hält, bleiben (Regel 89),
    /// etwa beim OK der Verwaltung. Wer an diesem Haus ausdrücklich „für
    /// dieses und neue Häuser“ ändert, nimmt [`Scene::fuer_firma_auch_hier`].
    pub fn fuer_firma(
        &mut self,
        label: &'static str,
        firma: &mut crate::catalog::Company,
        herkunft: &sk_cost::Herkunft,
        ops: &[sk_cost::Op],
    ) -> Result<Option<crate::meldung::Meldung>, crate::meldung::Meldung> {
        self.fuer_firma_mit(label, firma, herkunft, ops, true)
    }

    /// Wie [`Scene::fuer_firma`], aber dieses Haus übernimmt die geänderten
    /// Sätze auch dort, wo es abweicht: Preisblatt und Lohnkarte, an denen
    /// der Nutzer genau diesen Wert für dieses und neue Häuser setzt (Regel
    /// 89, „außer der Nutzer wählt sie ausdrücklich“).
    pub fn fuer_firma_auch_hier(
        &mut self,
        label: &'static str,
        firma: &mut crate::catalog::Company,
        herkunft: &sk_cost::Herkunft,
        ops: &[sk_cost::Op],
    ) -> Result<Option<crate::meldung::Meldung>, crate::meldung::Meldung> {
        self.fuer_firma_mit(label, firma, herkunft, ops, false)
    }

    fn fuer_firma_mit(
        &mut self,
        label: &'static str,
        firma: &mut crate::catalog::Company,
        herkunft: &sk_cost::Herkunft,
        ops: &[sk_cost::Op],
        eigene_behalten: bool,
    ) -> Result<Option<crate::meldung::Meldung>, crate::meldung::Meldung> {
        use crate::meldung::Meldung;
        let satz = |b: Vec<sk_cost::Befund>| Meldung::aus_befunden(&b, "Nichts geändert.");
        sk_cost::vorschau(
            &self.model,
            Some(firma.library()),
            sk_cost::Rolle::Admin,
            ops,
        )
        .map_err(satz)?;
        // Ohne Kopie rechnet dieses Haus mit der Firma. Damit Strg+Z genau
        // diese Änderung zurücknimmt (und nicht den letzten Bauschritt),
        // bekommt es vorher still die Kopie der bisherigen Werte; der
        // Schritt danach bringt die neuen. Rückgängig: dieses Haus wieder
        // mit den bisherigen Werten, neue Häuser mit den neuen.
        let vorher = (!sk_cost::op::hat_kopie(&self.model)).then(|| firma.library().clone());
        let neu = firma.fuer_firma(herkunft, ops)?;
        let mut hinweis = None;
        if let Some(alt) = vorher.filter(|_| !neu.saetze.is_empty()) {
            self.begin(label);
            sk_cost::op::kopie_anlegen(&mut self.model, Some(&alt));
            // ohne eigenen Schritt: der Stand vor der Änderung
            let _ = self.model.try_commit();
            // Das Modell hat sich außerhalb des Verlaufs geändert: Ein
            // Wiederholen von vorher passte nicht mehr dazu (es legte z. B.
            // eine schon zurückgenommene Kopie ein zweites Mal an), auch
            // wenn der Schritt danach scheitert oder nichts ändert
            self.redo.clear();
        }
        let saetze = if eigene_behalten {
            sk_cost::op::ohne_abweichung(&self.model, neu.saetze)
        } else {
            neu.saetze
        };
        if sk_cost::op::hat_kopie(&self.model) && !saetze.is_empty() {
            let op = sk_cost::Op::StandUebernehmen { saetze };
            if let Err(b) = self.kosten_folge(
                label,
                Some(firma.library()),
                herkunft,
                std::slice::from_ref(&op),
            ) {
                hinweis = Some(Meldung::mit(
                    "Für neue Häuser gespeichert, dieses Haus nicht geändert: {}",
                    &[&satz(b)],
                ));
            }
        }
        Ok(hinweis)
    }

    /// Vorschau der Kostenoperationen: geänderte Sätze alt/neu, der wirksame
    /// Katalog und das Netto im Umfang `u` vorher und nachher; Modell,
    /// `revision` und Verlauf bleiben (Abnahme 12).
    #[cfg_attr(not(test), allow(dead_code))] // Kostenreiter (KA-2)
    pub fn kosten_vorschau(
        &mut self,
        firma: Option<&sk_model::Library>,
        ops: &[sk_cost::Op],
        u: &sk_model::qto::Umfang,
    ) -> Result<sk_cost::Plan, Vec<sk_cost::Befund>> {
        self.schedule();
        let sched = self.schedule.as_ref().expect("gerade berechnet").1.clone();
        sk_cost::vorschau_kosten(&self.model, &sched, firma, sk_cost::Rolle::Admin, ops, u)
    }

    /// Ändert Bauteiltypen in einem Schritt (K3: „OK“ im Bauteilkatalog).
    /// Neu berechnet werden nur die Züge, deren Typ sich geändert hat, und die,
    /// die an ihnen hängen; ändern sich Baustoffe, alles. `true`, wenn sich
    /// etwas geändert hat.
    pub fn edit_types(&mut self, label: &'static str, f: impl FnOnce(&mut Model) -> bool) -> bool {
        self.grown(true, |s| s.edit_types_now(label, f))
    }

    fn edit_types_now(&mut self, label: &'static str, f: impl FnOnce(&mut Model) -> bool) -> bool {
        let rev = self.model.revision();
        // Nur was Netze und Zeichentabelle trägt; Name, Kennwerte, Preis und
        // Gewerk bauen keine Netze neu (Review 3n/5)
        let shape = |m: &Model| -> Vec<_> {
            m.materials()
                .iter()
                .map(|(_, x)| (x.guid, x.category, x.priority, x.display()))
                .collect()
        };
        let mats = shape(&self.model);
        let sets: Vec<_> = self
            .model
            .layer_sets()
            .iter()
            .map(|(_, t)| t.clone())
            .collect();
        self.begin(label);
        f(&mut self.model);
        if shape(&self.model) != mats {
            self.table = DrawTable::resolve(&self.model, &self.theme);
            self.mark_all();
        } else {
            let m = &self.model;
            let mut runs: Vec<RunId> = Vec::new();
            for (id, t) in m.layer_sets().iter() {
                if sets.iter().any(|s| s == t) {
                    continue;
                }
                for e in m.type_users(id) {
                    runs.extend(m.run_of(e));
                }
            }
            runs.sort_by_key(|r| r.index());
            runs.dedup();
            let mut marks = runs.clone();
            for &r in &runs {
                marks.extend(m.joined_runs(r));
                marks.extend(m.runs_under_floor(r));
                marks.extend(m.run_below(r));
                marks.extend(m.stack_above(r));
            }
            for r in marks {
                self.mark(r);
            }
        }
        self.commit();
        self.model.revision() != rev
    }

    /// Oberkante eines Geschosses beim Ziehen im Paneel „Geschosse“: geklemmt,
    /// ohne Verlaufseintrag (der Schritt ist offen, [`Scene::begin`]). Alle
    /// Züge folgen im Live-Netz, die Mengen kommen beim Loslassen.
    pub fn drag_storey_top(&mut self, id: sk_model::StoreyId, z: f64) {
        if self.model.drag_storey_top(id, z) {
            self.relevel(id);
        }
    }

    /// Unterkante der Gründung beim Ziehen, wie [`Scene::drag_storey_top`].
    pub fn drag_foundation_bottom(&mut self, z: f64) {
        let Some(gr) = self.model.foundation_level_of(self.active_storey()) else {
            return;
        };
        if self.model.drag_foundation_bottom_of(gr, z) {
            self.relevel(gr);
        }
    }

    /// Nach einer Änderung der Geschossbänder des Gebäudes, zu dem `level`
    /// gehört: dessen Züge live neu. Jedes Gebäude hat eigene Geschosse, die
    /// Züge anderer Gebäude bleiben (U4); ohne Gebäude alle.
    fn relevel(&mut self, level: sk_model::StoreyId) {
        let m = &self.model;
        let b = m.storey(level).and_then(|s| s.building);
        let ids: Vec<RunId> = m
            .runs()
            .iter()
            .filter(|(_, r)| b.is_none() || m.building_of(r.storey) == b)
            .map(|(id, _)| id)
            .collect();
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

    /// Versatz einer gestapelten Wand beim Ziehen (OG Phase 2, im offenen
    /// Schritt). `false`, wenn der Stapel dabei ungültig würde.
    pub fn set_offset(&mut self, wall: sk_model::ElementId, offset: f64) -> bool {
        let Some(runs) = self.model.set_offset(wall, offset) else {
            return false;
        };
        let own = self.model.segment_of(wall).map(|s| s.0);
        for r in runs.into_iter().chain(own) {
            self.mark(r);
            if !self.live.contains(&r) {
                self.live.push(r);
            }
        }
        self.rebuild_dirty(true);
        true
    }

    /// Gestapelte Wand zu `id` (OG Phase 2): sie selbst oder, an einem
    /// unteren Segment, die Wand darüber; `None` ohne Partner.
    pub fn stack_wall(&self, id: sk_model::ElementId) -> Option<sk_model::ElementId> {
        let m = &self.model;
        if m.stack_offset(id).is_some() {
            return Some(id);
        }
        m.elements()
            .iter()
            .find(|(w, _)| m.wall_below(*w) == Some(id))
            .map(|(w, _)| w)
    }

    /// Kette einer gestapelten Wand schließen oder lösen (OG Phase 2): ein
    /// Schritt „Wand gekoppelt“ bzw. „Kopplung gelöst“; der Versatz bleibt.
    /// `false`, wenn sich nichts ändert.
    pub fn set_linked(&mut self, wall: sk_model::ElementId, linked: bool) -> bool {
        if self.model.stack_offset(wall).is_none_or(|o| o.1 == linked) {
            return false;
        }
        let label = if linked {
            "Wand gekoppelt"
        } else {
            "Kopplung gelöst"
        };
        self.stack_step(label, wall, |m| m.set_linked(wall, linked))
    }

    /// „Bündig setzen“ mit Zielwand (Jörn 07.10. 07:53): `wall` rückt an
    /// `target` (die Wand direkt darüber oder darunter), ein Schritt
    /// „Bündig gesetzt“. `Ok(false)`, wenn das Paar schon bündig gekoppelt
    /// ist; `Err` mit dem Grund für die Hinweiskarte, dann ändert sich nichts.
    pub fn flush_to(
        &mut self,
        wall: sk_model::ElementId,
        target: sk_model::ElementId,
    ) -> Result<bool, sk_model::FlushError> {
        self.model.can_flush_to(wall, target)?;
        let upper = if self.model.wall_below(wall) == Some(target) {
            wall
        } else {
            target
        };
        if self.model.stack_offset(upper) == Some((0.0, true)) {
            return Ok(false);
        }
        let mut res = Ok(());
        let ok = self.stack_step("Bündig gesetzt", wall, |m| {
            res = m.flush_to(wall, target).map(|_| ());
            res.is_ok()
        });
        res.map(|_| ok)
    }

    /// Ein Bild des Gleitens beim „Bündig setzen“ (im offenen Schritt):
    /// `wall` rückt an `target`, bis der Versatz des Paars `rest` (mm) ist.
    /// Rückt die EG-Wand, ist die OG-Wand dabei gelöst und bleibt stehen;
    /// Platten und Decke folgen wie beim Ziehen am EG.
    pub fn glide_flush(
        &mut self,
        wall: sk_model::ElementId,
        target: sk_model::ElementId,
        rest: f64,
    ) -> bool {
        if self.model.wall_below(wall) == Some(target) {
            return self.set_offset(wall, rest);
        }
        let Some((now, linked)) = self.model.stack_offset(target) else {
            return false;
        };
        if linked {
            self.model.set_linked(target, false);
        }
        let moved = self.model.segment_of(wall).and_then(|(run, seg)| {
            let c = self.model.chain(run)?;
            let c = c.with_segment_moved(seg, c.outward_sign() * (now - rest))?;
            Some((run, c.points))
        });
        let Some((run, pts)) = moved else {
            return false;
        };
        self.set_run_points(run, &pts);
        true
    }

    /// Versatz eingetippt (Paneel): nur dieses Segment, die Kette bleibt, wie
    /// sie ist. Ein Schritt „Wand verschoben“.
    pub fn type_offset(&mut self, wall: sk_model::ElementId, offset: f64) -> bool {
        let Some((now, _)) = self.model.stack_offset(wall) else {
            return false;
        };
        if now == offset {
            return false;
        }
        // Ungültige Lage: nichts geändert, der leere Schritt fällt weg
        self.stack_step("Wand verschoben", wall, |m| {
            m.set_offset(wall, offset).is_some()
        })
    }

    /// Ein Schritt an einer gestapelten Wand; neu gebaut wird, was er berührt.
    fn stack_step(
        &mut self,
        label: &'static str,
        wall: sk_model::ElementId,
        f: impl FnOnce(&mut Model) -> bool,
    ) -> bool {
        self.begin(label);
        let ok = f(&mut self.model);
        let own = self.model.segment_of(wall).map(|s| s.0);
        for r in self.model.step_touched().into_iter().chain(own) {
            self.mark(r);
        }
        self.commit();
        ok
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
        if self.pending.is_some() {
            self.cancel_building();
            return true;
        }
        let grow = self
            .undo
            .last()
            .is_some_and(|t| GROW_STEPS.contains(&t.label));
        self.grown(grow, |s| s.step(Direction::Undo))
    }

    /// Bezeichnung des Schritts, den „Rückgängig“ als Nächstes zurücknimmt
    /// (ein Gebäude im Entstehen: „Gebäude erstellt“).
    pub fn undo_label(&self) -> Option<&'static str> {
        if self.pending.is_some() {
            return Some("Gebäude erstellt");
        }
        self.undo.last().map(|t| t.label)
    }

    /// Bezeichnung des Schritts, den „Wiederherstellen“ als Nächstes ausführt.
    pub fn redo_label(&self) -> Option<&'static str> {
        if self.pending.is_some() {
            return None;
        }
        self.redo.last().map(|t| t.label)
    }

    pub fn redo(&mut self) -> bool {
        if self.pending.is_some() {
            return false;
        }
        let grow = self
            .redo
            .last()
            .is_some_and(|t| GROW_STEPS.contains(&t.label));
        self.grown(grow, |s| s.step(Direction::Redo))
    }

    /// Uhr der Übergänge stellen (ms seit Start der App).
    pub fn set_now(&mut self, ms: u64) {
        self.now = ms;
    }

    /// Führt `f` aus; ändert es das Modell (und mit `grow`), zeigt das
    /// gezeichnete Netz den Übergang vom bisherigen zum neuen Stand (K3b).
    /// Wände, deren Typ sich geändert hat, leuchten nach.
    fn grown<R>(&mut self, grow: bool, f: impl FnOnce(&mut Scene) -> R) -> R {
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
        let [m, g] = self.mesh_split(key.0, key.1, &runs);
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

    /// Schnitthöhe des Hintergrunds für den Zug `run`: sein Geschoss liegt
    /// direkt unter dem aktiven (E16).
    fn under_cut(&self, run: RunId) -> Option<f64> {
        self.under_cut_for(run, self.active_storey())
    }

    /// Wie [`Scene::under_cut`], wenn `active` aktiv ist.
    fn under_cut_for(&self, run: RunId, active: StoreyId) -> Option<f64> {
        if self.is_foundation(active) {
            return None;
        }
        let floor = self.model.storey(active).map_or(0.0, |st| st.elevation);
        let st = self.model.storey(self.model.run(run)?.storey)?;
        let below = st.kind != sk_model::LevelKind::Foundation && (st.top() - floor).abs() < 1e-6;
        below.then_some(st.elevation + PLAN_CUT)
    }

    /// Wie der Zug zum Geschoss `active` liegt (Grundriss).
    fn plan_mode(&self, run: RunId, active: StoreyId) -> PlanMode {
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
        self.cache.iter().flatten().all(|c| {
            c.has_plan(cut, self.plan_mode(c.id, id))
                && self
                    .under_cut_for(c.id, id)
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
        let jobs: Vec<(RunId, PlanMode, Option<f64>)> = self
            .cache
            .iter()
            .flatten()
            .map(|c| (c.id, self.plan_mode(c.id, id), self.under_cut_for(c.id, id)))
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
        let cuts: Vec<(RunId, f64)> = self
            .cache
            .iter()
            .flatten()
            .filter_map(|c| Some((c.id, self.under_cut(c.id)?)))
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
        let [m, _] = self.mesh_split(view, section, runs);
        m
    }

    /// Wirkt gerade eine Sichtbarkeit auf die Netze (Ausgeblendetes,
    /// Isolieren oder ein Übergang)?
    fn filtering(&self) -> bool {
        !self.model.visibility().is_plain() || self.vis_anim.is_some()
    }

    fn vis_stamp(&self) -> VisStamp {
        (self.model.revision(), self.vis_rev, self.vis_anim.is_some())
    }

    /// Netze einzelner Wandzüge: deckend und blass (Paket 3). Gefiltert wird
    /// je Körper vor dem Verschmelzen gestapelter Züge (Review 3a G1).
    fn mesh_split(
        &mut self,
        view: ViewKind,
        section: Option<Plane>,
        runs: &[RunId],
    ) -> [MeshData; 2] {
        let cut = self.plan_cut();
        let filter = self.filtering();
        let stamp = self.vis_stamp();
        let mut m = [MeshData::default(), MeshData::default()];
        if view == ViewKind::Plan {
            let active = self.active_storey();
            let modes: Vec<PlanMode> = runs.iter().map(|r| self.plan_mode(*r, active)).collect();
            // Hintergrund: Wandschnitt des Geschosses direkt darunter in
            // seiner eigenen Schnitthöhe, nur Konturen, knapp über dem Boden
            // des aktiven Geschosses (unter dessen Wänden)
            let floor = self.work_plane().0;
            let under: Vec<Option<f64>> = runs.iter().map(|r| self.under_cut(*r)).collect();
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
                                    mesh_into(&mut m[k], x);
                                }
                            } else {
                                mesh_into(&mut m[0], s);
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
        for &run in runs {
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
                    mesh_into(mk, own);
                    continue;
                }
                let mut x = own.clone();
                if let Some(mut l) = lower {
                    merge_seam(&mut l, &mut x, c.chain.base);
                }
                for (z, mut u) in uppers {
                    merge_seam(&mut x, &mut u, z);
                }
                mesh_into(mk, &x);
            }
        }
        m
    }

    /// Blasses Netz (Isolieren, Übergänge) zur Ansicht wie [`Scene::mesh`];
    /// leer, wenn nichts blass ist.
    pub fn ghost_mesh(
        &mut self,
        view: ViewKind,
        section: Option<Plane>,
        except: &[RunId],
    ) -> MeshData {
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
        let [_, g] = self.mesh_split(view, section, &runs);
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

    /// Ändert die Sichtbarkeit sofort: kein Schritt, keine Revision, Mengen
    /// bleiben. `false`, wenn sich nichts ändert.
    pub fn set_visibility(&mut self, v: Visibility) -> bool {
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
        let old = self.model.visibility().clone();
        if !self.set_visibility(v) {
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
        for c in self.cache.iter().flatten() {
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
        let cut = self.plan_cut();
        let active = self.active_storey();
        let modes: Vec<(RunId, PlanMode)> = self
            .cache
            .iter()
            .flatten()
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
        for c in self.cache.iter_mut().flatten() {
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
    for e in &mut s.edges {
        e.elem = part;
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

/// Gleichen sich zwei Körper Bit für Bit (Dreiecke und Kanten)?
fn same_solid(a: &Solid, b: &Solid) -> bool {
    let v = |p: Vec3| [p.x.to_bits(), p.y.to_bits(), p.z.to_bits()];
    a.triangles.len() == b.triangles.len()
        && a.edges.len() == b.edges.len()
        && a.triangles.iter().zip(&b.triangles).all(|(s, t)| {
            s.p.map(v) == t.p.map(v)
                && v(s.n) == v(t.n)
                && (s.mat, s.elem, s.layer) == (t.mat, t.elem, t.layer)
                && s.uv.map(|q| q.map(f64::to_bits)) == t.uv.map(|q| q.map(f64::to_bits))
        })
        && a.edges.iter().zip(&b.edges).all(|(s, t)| {
            (v(s.a), v(s.b), s.kind, s.elem, s.layer) == (v(t.a), v(t.b), t.kind, t.elem, t.layer)
        })
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
    /// Review 3ao: Beim Ziehen (offener Schritt) gibt `lv` das gemerkte LV
    /// derselben Wahl zurück, statt jedes Bild neu zu ordnen; nach dem
    /// Loslassen gilt der neue Stand.
    #[test]
    fn lv_beim_ziehen_gemerkt() {
        let m = sk_model::szo::read_with(
            include_str!("../../crates/sk-cost/referenz/rh1-standardhaus.szo"),
            sk_model::GuidGen::with_seed(1),
            &sk_cost::lesen::ABSCHNITTE_SZO,
        )
        .unwrap()
        .model;
        let mut s = Scene::with_model(m);
        let u = sk_model::qto::Umfang::projekt();
        let k = s.katalog(None);
        let los = k.lose.iter().find(|l| l.parent.is_none()).unwrap().guid;
        let w = sk_cost::lv::LvWahl {
            los,
            preise: true,
            untertitel: false,
            heute: None,
        };
        let vorher = s.lv(None, &u, &w);
        s.begin("Ziehen");
        let mut p = s.model.project().clone();
        p.client = "Familie Muster".into();
        assert!(s.model.set_project(p));
        let beim_ziehen = s.lv(None, &u, &w);
        assert!(Rc::ptr_eq(&vorher, &beim_ziehen), "beim Ziehen gemerkt");
        s.commit();
        let danach = s.lv(None, &u, &w);
        assert!(!Rc::ptr_eq(&vorher, &danach));
        assert_eq!(danach.kopf.bauherr.as_deref(), Some("Familie Muster"));
    }

    /// Review 3ai: Der gemerkte Katalog hängt auch an den Baustoffen des
    /// Modells (R73-W). Umbenennen von „Stahlbeton“ in einer Altdatei nimmt
    /// die Werkspreise weg; Katalog und Kostenblatt folgen sofort.
    #[test]
    fn katalog_folgt_umbenanntem_baustoff() {
        let text = include_str!("abnahme_p5.szo");
        let m = sk_model::szo::read_with(
            text,
            sk_model::GuidGen::with_seed(1),
            &sk_cost::lesen::ABSCHNITTE_SZO,
        )
        .unwrap()
        .model;
        let mut s = Scene::with_model(m);
        let u = sk_model::qto::Umfang::projekt();
        let vorher = s.kostenblatt(None, &u).netto;
        let (id, mut x) = s
            .model()
            .materials()
            .iter()
            .find(|(_, x)| x.name == "Stahlbeton")
            .map(|(id, x)| (id, x.clone()))
            .unwrap();
        x.name = "Beton alt".into();
        assert!(s.edit_model("Name", |m| m.set_material(id, x)));
        let frisch = sk_cost::lesen::katalog(s.model(), None);
        assert_eq!(s.katalog(None).befunde, frisch.befunde);
        let sched = sk_model::qto::schedule(s.model());
        let soll = sk_cost::lesen::kosten(s.model(), &sched, &frisch, &u).netto;
        assert_ne!(soll, vorher);
        assert_eq!(s.kostenblatt(None, &u).netto, soll);
    }

    use super::*;
    use sk_model::{Pen, RefSide};

    /// Paket 3 (Vor-Patch): Jede Fläche und Kante des Prüfhauses mit AW-49
    /// und Dachterrasse trägt ein Bauteil; Wandflächen eine Schicht des
    /// Wandtyps, Decke und Terrasse eine ihres Aufbaus, alles andere
    /// [`sk_model::NO_LAYER`]. Gilt in 3D, im Grundriss (jede Art), im
    /// Schnitt A und B und an den Hintergrundkanten; jede Schicht der Wand
    /// hat Flächen.
    #[test]
    fn jede_flaeche_und_kante_traegt_bauteil_und_schicht() {
        use sk_model::{Category, NO_LAYER};
        let mut s = Scene::with_model(Model::with_seed(75));
        let pts = [
            vec3(0.0, 0.0, 0.0),
            vec3(0.0, 8000.0, 0.0),
            vec3(10000.0, 8000.0, 0.0),
            vec3(10000.0, 0.0, 0.0),
        ];
        let mut eg = None;
        assert!(s.edit_model("Gebäude erstellt", |m| {
            let b = m.add_building(2);
            eg = m.build_from_polygon(b, &pts);
            eg.is_some()
        }));
        let eg = eg.unwrap();
        let og = s.model().runs_above(eg)[0];
        let aw49 = s.model().type_by_guid(sk_model::CAVITY_TYPE_GUID).unwrap();
        let nord = s.model().wall_at(og, 1).unwrap();
        assert!(s.edit_model("Prüfhaus", |m| {
            m.set_run_type(eg, aw49)
                && m.set_run_type(og, aw49)
                && m.set_linked(nord, false)
                && m.set_offset(nord, -1500.0).is_some()
        }));
        let de = s.model().floor_of(eg).unwrap();
        assert!(s.model().terrace_of(de).is_some(), "Dachterrasse");
        s.rebuild_dirty(false);

        let model = s.model.clone();
        let check = |run: RunId, sol: &Solid, what: &str, per_layer: &mut Vec<usize>| {
            let layers = |id: ElementId| model.element_layers(id).len();
            let ok_layer = |id: ElementId, layer: u8| {
                let e = model.element(id).unwrap();
                match e.category {
                    Category::ExteriorWall | Category::InteriorWall => {
                        (layer as usize) < layers(id)
                    }
                    Category::Floor | Category::GroundSlab | Category::StripFooting => {
                        (layer as usize) < layers(id).max(1)
                    }
                    Category::RoofTerrace => (layer as usize) < layers(id),
                    _ => layer == NO_LAYER,
                }
            };
            for t in &sol.triangles {
                let id = model
                    .part_of(run, t.elem)
                    .unwrap_or_else(|| panic!("{what}: Fläche ohne Bauteil (Teil {})", t.elem));
                assert!(
                    ok_layer(id, t.layer),
                    "{what}: Fläche {} Schicht {}",
                    t.elem,
                    t.layer
                );
                let e = model.element(id).unwrap();
                if e.category == Category::ExteriorWall {
                    per_layer[t.layer as usize] += 1;
                }
            }
            for e in &sol.edges {
                let id = model
                    .part_of(run, e.elem)
                    .unwrap_or_else(|| panic!("{what}: Kante ohne Bauteil (Teil {})", e.elem));
                assert!(
                    ok_layer(id, e.layer),
                    "{what}: Kante {} Schicht {}",
                    e.elem,
                    e.layer
                );
            }
        };
        let nlayers = model.layer_set(aw49).unwrap().layers.len();
        let planes = [
            (vec3(5000.0, 4000.0, 0.0), vec3(0.0, 1.0, 0.0)),
            (vec3(5000.0, 4000.0, 0.0), vec3(1.0, 0.0, 0.0)),
            (vec3(5000.0, 7300.0, 0.0), vec3(0.0, -1.0, 0.0)),
        ];
        for run in [eg, og] {
            let i = s
                .cache
                .iter()
                .position(|c| c.as_ref().is_some_and(|c| c.id == run));
            let c = s.cache[i.unwrap()].as_mut().unwrap();
            let mut per_layer = vec![0; nlayers];
            let solid = c.solid.clone();
            check(run, &solid, "3D", &mut per_layer);
            for (k, n) in per_layer.iter().enumerate() {
                let air = model.layer_set(aw49).unwrap().layers[k].function
                    == sk_model::LayerFunction::AirGap;
                assert!(air || *n > 0, "Schicht {k} ohne Flächen");
            }
            for cut in [1000.0, 2900.0, 3900.0, 6000.0, -300.0] {
                for mode in [PlanMode::Cut, PlanMode::Lower, PlanMode::Foundation] {
                    let p = c.plan_solid(cut, mode);
                    check(
                        run,
                        &p,
                        &format!("Grundriss {cut} {mode:?}"),
                        &mut vec![0; nlayers],
                    );
                }
                let under = Solid {
                    edges: c.under_edges(cut).to_vec(),
                    ..Solid::default()
                };
                check(run, &under, "Hintergrund", &mut vec![0; nlayers]);
            }
            for pl in planes {
                let sec = c
                    .view_solid(ViewKind::Section, Some(pl), 0.0, PlanMode::Cut)
                    .unwrap()
                    .clone();
                check(run, &sec, &format!("Schnitt {pl:?}"), &mut vec![0; nlayers]);
                // Durch die Terrasse: Belag 0 und Dämmung 1
                if run == eg && pl.1.x == 1.0 {
                    let dt = model.terrace_of(de).unwrap();
                    let mut got: Vec<u8> = sec
                        .triangles
                        .iter()
                        .filter(|t| model.part_of(run, t.elem) == Some(dt))
                        .map(|t| t.layer)
                        .collect();
                    got.sort();
                    got.dedup();
                    assert_eq!(got, [0, 1], "Schichten der Terrasse im Schnitt");
                }
            }
        }
    }

    #[test]
    fn buendig_an_zielwand_ein_schritt() {
        let mut s = Scene::with_model(Model::with_seed(74));
        let pts = [
            vec3(0.0, 0.0, 0.0),
            vec3(0.0, 8000.0, 0.0),
            vec3(10000.0, 8000.0, 0.0),
            vec3(10000.0, 0.0, 0.0),
        ];
        let mut eg = None;
        assert!(s.edit_model("Gebäude erstellt", |m| {
            let b = m.add_building(2);
            eg = m.build_from_polygon(b, &pts);
            eg.is_some()
        }));
        let eg = eg.unwrap();
        let og = s.model().runs_above(eg)[0];
        let (w, e) = (
            s.model().wall_at(og, 1).unwrap(),
            s.model().wall_at(eg, 1).unwrap(),
        );
        assert!(s.set_linked(w, false));
        assert!(s.type_offset(w, 300.0));
        let y = |s: &Scene, r| s.model().run(r).unwrap().points[1].y;
        // Ziel OG: die EG-Wand rückt nach außen, ein Schritt
        assert_eq!(s.flush_to(e, w), Ok(true));
        assert_eq!((y(&s, eg), y(&s, og)), (8300.0, 8300.0));
        assert_eq!(s.model().stack_offset(w), Some((0.0, true)));
        assert_eq!(s.flush_to(e, w), Ok(false));
        assert!(s.model().check().is_empty(), "{:?}", s.model().check());
        assert!(s.undo());
        assert_eq!((y(&s, eg), y(&s, og)), (8000.0, 8300.0));
        assert_eq!(s.model().stack_offset(w), Some((300.0, false)));
        // Ziel EG: das OG rückt wie bisher
        assert_eq!(s.flush_to(w, e), Ok(true));
        assert_eq!((y(&s, eg), y(&s, og)), (8000.0, 8000.0));
        // Keine Partner: Grund, kein Schritt
        let label = s.undo_label();
        assert_eq!(
            s.flush_to(e, s.model().wall_at(og, 0).unwrap()),
            Err(sk_model::FlushError::NotPartners)
        );
        assert_eq!(s.undo_label(), label);
    }

    #[test]
    fn blech_vorgabe_gilt_als_verwendet_und_terrasse_hat_kante() {
        let mut s = Scene::with_model(Model::with_seed(75));
        let pts = [
            vec3(0.0, 0.0, 0.0),
            vec3(0.0, 8000.0, 0.0),
            vec3(10000.0, 8000.0, 0.0),
            vec3(10000.0, 0.0, 0.0),
        ];
        let mut eg = None;
        assert!(s.edit_model("Gebäude erstellt", |m| {
            let b = m.add_building(2);
            eg = m.build_from_polygon(b, &pts);
            eg.is_some()
        }));
        let og = s.model().runs_above(eg.unwrap())[0];
        let w = s.model().wall_at(og, 1).unwrap();
        assert!(s.set_linked(w, false));
        assert!(s.type_offset(w, -1500.0));
        let m = s.model();
        let floor = m
            .elements()
            .iter()
            .find_map(|(_, e)| match e.kind {
                sk_model::ElementKind::Coping { floor } => Some(floor),
                _ => None,
            })
            .expect("Attikablech");
        let mat = m.coping_material(floor).expect("Vorgabe");
        assert!(m.material_used(mat), "Vorgabe des Blechs");
        assert!(s.set_active_storey(m.run(og).unwrap().storey));
        let (marks, blocked) = s.terrace_marks();
        assert_eq!(marks.len(), 1);
        let (a, b, n) = marks[0].edge.expect("Blechkante");
        assert!(
            (a.y - b.y).abs() < 1.0 && a.y > 8000.0 && n.y > 0.9,
            "{a:?} {b:?} {n:?}"
        );
        assert!(!blocked.is_empty());
    }

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

    /// U4: OK EG eines Gebäudes ziehen rechnet nur dessen Züge neu; das
    /// andere Gebäude (eigene Geschosse) bleibt, wie es ist.
    #[test]
    fn ebene_ziehen_rechnet_nur_das_eigene_gebaeude() {
        let mut s = Scene::with_model(Model::with_seed(1));
        let mut eg = Vec::new();
        for x in [0.0, 20000.0] {
            s.edit_model("Gebäude erstellt", |m| {
                m.add_building(2);
                true
            });
            let b = s.model().buildings().ids().last().unwrap();
            let g = s.model().ground_of(Some(b)).unwrap();
            s.set_active_storey(g);
            s.add_wall(&rechteck(x)).unwrap();
            eg.push(g);
        }
        let of = |s: &Scene, g| -> Vec<RunId> {
            let b = s.model().building_of(g);
            s.model()
                .runs()
                .iter()
                .filter(|(_, r)| s.model().building_of(r.storey) == b)
                .map(|(id, _)| id)
                .collect()
        };
        let (eigen, fremd) = (of(&s, eg[0]), of(&s, eg[1]));
        assert_eq!((eigen.len(), fremd.len()), (2, 2), "EG und OG je Gebäude");
        let count =
            |s: &Scene, v: &[RunId]| v.iter().map(|r| s.build_count(*r)).collect::<Vec<_>>();
        let (vorher_e, vorher_f) = (count(&s, &eigen), count(&s, &fremd));
        s.begin("Geschoss ziehen");
        s.drag_storey_top(eg[0], 2900.0);
        s.commit();
        let nachher_e = count(&s, &eigen);
        assert!(
            vorher_e.iter().zip(&nachher_e).all(|(a, b)| b > a),
            "{vorher_e:?} → {nachher_e:?}"
        );
        assert_eq!(count(&s, &fremd), vorher_f, "anderes Gebäude unberührt");
        // Ergebnis gleich einer vollen Neuberechnung
        let live = s.mesh(ViewKind::Persp, None, &[]);
        let mut frisch = Scene::with_model(s.model().clone());
        assert!(same(&live, &frisch.mesh(ViewKind::Persp, None, &[])));
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

    /// Paket 3 §2: Ausblenden gleitet in `anim_ms` von deckend nach
    /// durchsichtig, Isolieren macht den Rest blass; ein Klick springt ans
    /// Ende. Während des Übergangs ist nichts Ausgehendes anklickbar.
    #[test]
    fn sichtbarkeit_gleitet() {
        let mut s = Scene::with_model(Model::with_seed(4));
        s.add_wall(&rechteck(0.0)).unwrap();
        let ms = s.theme.size.anim_ms as u64;
        let w = s.model().elements().iter().next().unwrap().0;
        let g = s.model().element(w).unwrap().guid;
        s.set_now(1000);
        let mut v = s.model().visibility().clone();
        v.hidden.insert(g);
        assert!(s.fade_visibility(v.clone()));
        assert!(!s.fade_visibility(v), "gleich: nichts neu");
        assert!(s.vis_animating() && !s.visible(w));
        assert_eq!(s.ghost_alpha(false), 1.0);
        assert!(!s.ghost_mesh(ViewKind::Persp, None, &[]).faces.is_empty());
        assert!(!s.vis_tick(1000 + ms / 2));
        assert!(s.ghost_alpha(false) > 0.0 && s.ghost_alpha(false) < 1.0);
        assert!(s.vis_tick(1000 + ms));
        assert_eq!(s.ghost_alpha(false), 0.0);
        assert!(s.ghost_mesh(ViewKind::Persp, None, &[]).faces.is_empty());

        let mut v = s.model().visibility().clone();
        v.hidden.clear();
        v.isolate = Some(Isolate::Elements([g].into_iter().collect()));
        assert!(s.fade_visibility(v));
        assert!(s.isolating().is_some());
        assert!(s.skip_vis_animation());
        let a = s.theme.size.ghost_alpha_3d;
        assert_eq!(s.ghost_alpha(false), a);
        assert_eq!(s.ghost_alpha(true), s.theme.size.ghost_alpha_paper);
        assert!(s.pickable(w));
    }

    /// Review 3h G2 und Befund 2: Das Teilen in deckend und blass hängt an
    /// der Generation des Körpers und den Masken, nicht an der Revision.
    /// Eine Änderung an einem Haus teilt nur dessen Züge neu; das andere
    /// behält seine geteilten Körper, ebenso ein Zug, der gleich neu gebaut
    /// wurde. Je Zug und Ansicht höchstens ein Eintrag.
    #[test]
    fn teilen_nur_neu_gebauter_zuege() {
        let mut s = Scene::with_model(Model::with_seed(5));
        let a = s.add_wall(&rechteck(0.0)).unwrap();
        let b = s.add_wall(&rechteck(10000.0)).unwrap();
        let mut v = s.model().visibility().clone();
        v.hidden_cat.insert(Category::Floor);
        assert!(s.set_visibility(v));
        s.mesh(ViewKind::Persp, None, &[]);
        let geteilt = |s: &Scene, r: RunId| {
            s.cached(r)
                .and_then(|c| c.splits.borrow().first().map(|x| x.2.clone()))
                .expect("geteilt")
        };
        let (va, vb) = (geteilt(&s, a), geteilt(&s, b));
        // Neue Revision ohne neuen Körper: nichts neu geteilt
        let w = s.model().wall_at(a, 0).unwrap();
        let rev = s.model().revision();
        s.begin("Merkmal");
        let text = Some(sk_model::PropValue::Text("neu".into()));
        assert!(s.model.set_prop(w, "Bemerkung", text));
        s.commit();
        assert!(s.model().revision() > rev);
        s.mesh(ViewKind::Persp, None, &[]);
        assert!(Rc::ptr_eq(&va, &geteilt(&s, a)), "Revision allein");
        // Gleich neu gebaut (ein Nachbar hat sich geändert): dieselbe
        // Generation, nichts neu geteilt
        s.mark(a);
        s.rebuild_dirty(false);
        s.mesh(ViewKind::Persp, None, &[]);
        assert!(Rc::ptr_eq(&va, &geteilt(&s, a)), "gleicher Körper");
        // Körper geändert: nur dieser Zug wird neu geteilt
        let moved = s.chain(a).unwrap().with_segment_moved(0, -100.0).unwrap();
        s.begin("Wand verschieben");
        s.set_run_points(a, &moved.points);
        s.commit();
        s.mesh(ViewKind::Persp, None, &[]);
        assert!(
            !Rc::ptr_eq(&va, &geteilt(&s, a)),
            "neuer Körper, neu geteilt"
        );
        assert!(
            Rc::ptr_eq(&vb, &geteilt(&s, b)),
            "anderer Zug aus dem Speicher"
        );
        // Andere Masken: neu geteilt
        let mut v = s.model().visibility().clone();
        v.hidden_cat.clear();
        v.hidden_cat.insert(Category::ExteriorWall);
        assert!(s.set_visibility(v));
        s.mesh(ViewKind::Persp, None, &[]);
        assert!(!Rc::ptr_eq(&vb, &geteilt(&s, b)));
        assert_eq!(s.cached(b).unwrap().splits.borrow().len(), 1, "ein Eintrag");
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
        // AW-001…008: erstes Haus mit OG, AW-009…012: EG des zweiten
        assert_eq!(s.model().element(wall).unwrap().number, "AW-011");
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

    /// KA-0d, Abnahme 11–13: `kosten` ist ein Schritt mit Bezeichnung,
    /// `kosten_folge` einer oder nichts, `kosten_vorschau` ändert nichts.
    #[test]
    fn kosten_ein_schritt_vorschau_ohne_aenderung() {
        use sk_cost::{Dez, Herkunft, HerkunftArt, Op};
        let lohn = |w: i64| Op::FirmenwertSetzen {
            schluessel: "wage".into(),
            wert: Dez::ganz(w),
        };
        let h = Herkunft::neu(HerkunftArt::Manual, "2026-10-08", "09:00");
        let mut s = Scene::new();
        let leer = sk_model::szo::write(s.model());
        // Vorschau: alles bleibt
        let (rev, ext) = (s.model().revision(), s.model().ext_revision());
        let p = s
            .kosten_vorschau(None, &[lohn(65)], &sk_model::qto::Umfang::projekt())
            .unwrap();
        assert!(p.aenderungen.iter().any(|a| a.satz.kennung == "wage"));
        assert_eq!(p.netto, Some((sk_cost::Cent(0), sk_cost::Cent(0))));
        assert_eq!(p.katalog.werte.lohn, Dez::ganz(65));
        assert_eq!((s.model().revision(), s.model().ext_revision()), (rev, ext));
        assert_eq!(s.undo_label(), None);
        // ein Schritt mit Bezeichnung; Rückgängig gibt die leere Datei zurück
        s.kosten(None, &h, lohn(65)).unwrap();
        assert_eq!(s.undo_label(), Some("Lohn 65,00 €/h"));
        assert_eq!(
            sk_cost::lesen::katalog(s.model(), None).werte.lohn,
            Dez::ganz(65)
        );
        assert!(s.undo());
        assert_eq!(s.undo_label(), None);
        assert_eq!(sk_model::szo::write(s.model()), leer);
        // Fehler: nichts, kein Schritt
        let rev = s.model().revision();
        let b = s.kosten(None, &h, lohn(1_000_000)).unwrap_err();
        assert!(b.iter().any(|b| b.regel == 93), "{b:?}");
        assert_eq!(s.model().revision(), rev);
        assert_eq!(s.undo_label(), None);
        // Folge: drei sind ein Schritt; scheitert die dritte, bleibt nichts
        let b = s
            .kosten_folge("Löhne", None, &h, &[lohn(61), lohn(62), lohn(1_000_000)])
            .unwrap_err();
        assert!(!b.is_empty());
        assert_eq!(s.undo_label(), None);
        assert_eq!(sk_model::szo::write(s.model()), leer);
        s.kosten_folge("Löhne", None, &h, &[lohn(61), lohn(62), lohn(63)])
            .unwrap();
        assert_eq!(s.undo_label(), Some("Löhne"));
        assert_eq!(
            sk_cost::lesen::katalog(s.model(), None).werte.lohn,
            Dez::ganz(63)
        );
        assert!(s.undo());
        assert_eq!(sk_model::szo::write(s.model()), leer);
    }
}
