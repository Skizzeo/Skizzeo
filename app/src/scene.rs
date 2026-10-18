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
use sk_model::koerper::{foundation_solid, part, run_solid};
use sk_model::qto::{Schedule, Umfang};
use sk_model::view::{Isolate, Masks, Visibility};
use sk_model::{
    edge_kind, floor_qto_of, foundation_qto_of, merge_seam, run_qto, BuildingId, Category, Deleted,
    Direction, Edge, ElementId, FlatRoof, FloorQto, FloorSlab, FootingQto, Foundation, LayerSetId,
    Model, Refusal, RunId, SlabQto, Solid, StoreyId, Touched, Tri, TypeCategory, WallChain,
    WallQto, COPING_PART, DOWNPIPE_PART, FLOOR_PART, FOOTING_PART, PERIMETER_PART,
    ROOF_COPING_PART, ROOF_PART, SLAB_PART, SOFFIT_PART, STRIP_PART, TERRACE_PART,
};
use sk_render::MeshData;
use sk_ui::theme::Theme;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};

#[path = "scene_gefaelle.rs"]
mod gefaelle;
#[path = "scene_raum.rs"]
mod raum;
#[path = "scene_ausbau.rs"]
pub mod scene_ausbau;
#[path = "scene_fallrohr.rs"]
mod scene_fallrohr;
#[path = "scene_schnitt.rs"]
mod schnitt;
#[path = "scene_sicht.rs"]
pub mod sicht;
use schnitt::UnderMemo;
#[path = "scene_boden.rs"]
mod boden;
#[path = "scene_kanten.rs"]
pub mod kanten;
#[path = "scene_lage.rs"]
mod lage;
#[path = "scene_massstand.rs"]
mod massstand;
#[path = "scene_netz.rs"]
mod netz;
#[path = "scene_neubau.rs"]
mod neubau;
#[path = "scene_raster.rs"]
pub mod raster;
#[path = "scene_ansicht.rs"]
mod scene_ansicht;
#[path = "scene_bemassung.rs"]
mod scene_bemassung;
#[path = "scene_kosten.rs"]
mod scene_kosten;
#[path = "scene_oeffnung.rs"]
mod scene_oeffnung;
#[path = "scene_sichtbarkeit.rs"]
mod scene_sichtbarkeit;
use boden::{mesh_into, Boden};
pub use massstand::{naechster_stand, neuer_stand};

/// Anzeigename der Gründung als Ebene (Paneel „Geschosse“, Geschossbogen).
pub const FOUNDATION_NAME: &str = "Fundament";

/// Vorgabe der Schnitthöhe des Grundrisses über OK Rohdecke (mm); das
/// Projekt kann eine eigene haben ([`sk_model::hoehen`]).
pub const PLAN_CUT: f64 = sk_model::PLAN_CUT;

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
    /// Fugen des Zugs (W-11), für die Plättchen in jedem Bild.
    joints: Vec<usize>,
    /// Löcher im Grundriss ([`WallChain::hole_quads`]), mit dem Zug
    /// berechnet (Review L26).
    loecher: Vec<(sk_model::wand_loch::Hole, [Vec3; 4])>,
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
    /// Dachaufbau und Attikablech auf einer Aufkantung (Flachdach D3, D4).
    roof: Option<FlatRoof>,
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
        roof: Option<FlatRoof>,
        builds: u32,
    ) -> RunCache {
        let solid = run_solid(&chain, found.as_ref(), floor.as_ref(), roof.as_ref());
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
        let loecher = chain.hole_quads();
        let loecher = loecher.into_iter().map(|(h, q)| (h.clone(), q)).collect();
        RunCache {
            id,
            joints: chain.straight_joints(),
            loecher,
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
            roof,
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
            && self.roof == other.roof
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
                    for p in run_parts(&self.found, &self.floor, &self.roof) {
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
            PlanMode::Cut => {
                let mut s = self.chain.solid_cut_at(cut);
                s.edges.extend(self.chain.element_edges(cut));
                s.edges.extend(self.chain.swing_edges(cut));
                s.edges.extend(self.chain.lintel_edges(cut));
                s
            }
            PlanMode::Foundation | PlanMode::Lower => Solid::default(),
        };
        for p in run_parts(&self.found, &self.floor, &self.roof) {
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
/// Streifen): hängt ihre Körper für Grundriss und senkrechten Schnitt an
/// `out` an, jeweils mit der Teilnummer je Bauteil (Review 2a, R2). Der
/// 3D-Körper kommt aus [`sk_model::koerper::run_solid`], damit Ansicht und
/// IFC-Export dieselbe Form haben. Ein neues Bauteil am Zug braucht eine
/// Umsetzung hier, einen Eintrag in [`run_parts`] und einen in `run_solid`.
trait RunPart {
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
    roof: &'a Option<FlatRoof>,
) -> impl Iterator<Item = &'a dyn RunPart> {
    let found = found.as_ref().map(|f| f as &dyn RunPart);
    let floor = floor.as_ref().map(|f| f as &dyn RunPart);
    let roof = roof.as_ref().map(|f| f as &dyn RunPart);
    found.into_iter().chain(floor).chain(roof)
}

impl RunPart for Foundation {
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
            PlanMode::Cut => foundation_solid(self, out),
            PlanMode::Lower => {}
        }
    }

    fn section_caps(&self, out: &mut Solid, p0: Vec3, n: Vec3) {
        let (slab, foot) = Foundation::section_caps(self, p0, n);
        out.append(&part(slab, SLAB_PART));
        out.append(&part(self.insulation_section(p0, n), PERIMETER_PART));
        out.append(&part(foot, FOOTING_PART));
    }
}

impl RunPart for FloorSlab {
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

/// Flachdach auf einer Aufkantung (D3, D4): Dachaufbau zwischen den
/// Aufkantungen, Blech auf ihrer Krone.
impl RunPart for FlatRoof {
    /// Im Grundriss der Ebene Flachdach liegt der Aufbau unter der
    /// Schnitthöhe; das Blech erscheint, solange es ganz darunter liegt.
    fn plan(&self, out: &mut Solid, cut: f64, mode: PlanMode) {
        match mode {
            PlanMode::Foundation => {}
            PlanMode::Lower => {
                out.append(&part(seen_below(self.cut_at(cut)), ROOF_PART));
                out.append(&part(seen_below(self.coping_solid()), ROOF_COPING_PART));
            }
            PlanMode::Cut => {
                out.append(&part(self.cut_at(cut), ROOF_PART));
                // Attikadämmung geschnitten oder gesehen, das Blech nur
                // ganz unter der Schnitthöhe
                out.append(&part(self.coping_cut_at(cut), ROOF_COPING_PART));
                // Regenfallrohre außen, von oben gesehen
                for (k, d) in self.downpipes.iter().enumerate() {
                    out.append(&part(
                        seen_below(d.plan_below(cut)),
                        DOWNPIPE_PART - k as u32,
                    ));
                }
            }
        }
    }

    fn section_caps(&self, out: &mut Solid, p0: Vec3, n: Vec3) {
        out.append(&part(FlatRoof::section_caps(self, p0, n), ROOF_PART));
        out.append(&part(self.coping_section_caps(p0, n), ROOF_COPING_PART));
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

/// Zählt die Szenen für [`Scene::kennung`].
static SZENEN: AtomicU64 = AtomicU64::new(0);

/// Ein Umfang der Vorschau: Kostenspeicher und Liste im Umfang mit der
/// Berechnung der Liste, aus der sie stammt.
#[derive(Default)]
struct LiveUmfang {
    speicher: sk_cost::Kostenspeicher,
    liste: Option<(u64, Umfang, Rc<Schedule>)>,
}

pub struct Scene {
    model: Model,
    /// Kennung dieser Szene ([`Scene::kennung`]).
    kennung: u64,
    /// Deckendicke über den Geschossen je Modellstand ([`schnitt`]).
    decken: schnitt::Decken,
    /// Bauphase in 3D und Schnitt ([`Scene::bauphase`]); nur Ansicht, nicht
    /// in der Datei.
    bauphase: crate::gelaende_platte::bauphase::Bauphase,
    /// Rolle dieser Sitzung für Kostenoperationen (KA-3b1): `Nutzer`, wenn
    /// der Firmenkatalog ein Verwaltungskennwort hat und es hier nicht
    /// eingegeben ist; sonst `Admin`. Die App setzt sie.
    pub rolle: sk_cost::Rolle,
    /// Gewählter Ablauf: Dachaufbau und Index (Jörn 11.10.).
    drain: Option<(ElementId, usize)>,
    /// Zuletzt angelegte Räume mit Geschoss und Fläche, bis die App ein
    /// Namensfeld öffnet (G4).
    neue_raeume: Vec<(ElementId, sk_model::StoreyId, f64)>,
    /// Raumerkennung mit Zwischenspeicher je Wandzug und Hausgruppe (Review
    /// #87 B2, B3).
    erkennung: RefCell<sk_raum::Erkennung>,
    /// Geltende Räume je Geschoss zum Stand vor dem offenen Schritt, für
    /// den Abgleich nach Überdeckung (B4); Rückgängig und Wiederholen führen
    /// ihn nach ([`Scene::staende_folgen`]), fehlt einer, rechnet ihn
    /// [`Scene::begin`].
    staende: HashMap<sk_model::StoreyId, sk_raum::Stand>,
    /// Schritte für „Rückgängig“ (die ältesten fallen nach [`HISTORY`] weg).
    undo: Vec<raum::Schritt>,
    /// Rückgängig gemachte Schritte für „Wiederholen“.
    redo: Vec<raum::Schritt>,
    /// Zählt jeden neuen Schritt im Verlauf (Paket 8b, K3).
    serial: u64,
    /// Zwischenspeicher je Wandzug, über den Arena-Platz der [`RunId`].
    cache: Vec<Option<RunCache>>,
    /// Quader der Wandzüge je Platz im Raster (Review L26).
    raster: raster::Raster,
    /// Je Platz der Bereich, in dem die Öffnungen des Zugs Griffe zeigen.
    griffraster: raster::Raster,
    /// Wandzüge, die neu berechnet werden müssen.
    dirty: Vec<RunId>,
    all_dirty: bool,
    /// Stand der Bemaßung je Geschoss und aller ([`Scene::mass_stand`]).
    mass_stand: HashMap<StoreyId, u64>,
    mass_alle: u64,
    /// Wandzüge, deren Mengen nach einer Live-Änderung noch fehlen.
    unsettled: Vec<RunId>,
    /// Züge, die sich im offenen Schritt live geändert haben (der gezogene und
    /// die mitgeführten oder angeschlossenen).
    live: Vec<RunId>,
    /// Laufender Zug an OK Gelände mit den Sohlplatten-Zügen des gezogenen
    /// Gebäudes ([`Scene::drag_terrain`]).
    gelaende_zug: Option<(sk_model::model::GelaendeZug, Vec<RunId>)>,
    /// Züge mit Sohlplatte der übrigen Gebäude, die einem Gelände-Zug im
    /// Leerlauf folgen ([`Scene::nachziehen`]).
    nachzug: Vec<RunId>,
    /// Faden, der das Geländenetz neu rechnet ([`Scene::netz_abholen`]).
    netz_faden: Option<netz::NetzFaden>,
    bounds: Option<Aabb>,
    /// Seiten aller Körper, gezählt ([`neubau::Grenzen`]).
    grenzen: neubau::Grenzen,
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
    /// Indizes je Dicke aus dem Blatt des Projekts, gemerkt je Blatt
    /// (Review #213 H-213k).
    dickenrang: Option<(Rc<sk_cost::Kostenblatt>, Rc<sk_cost::lv::Dickenrang>)>,
    /// Firmen- oder Werkskatalog ohne Projekt (Firmenwerte im Preisblatt).
    firmenkatalog: Option<((u64, u64, u64), Rc<sk_cost::Katalog>)>,
    /// Bezeichnungen mit Werten, je Wortlaut einmal ([`Scene::bezeichnung`]).
    bezeichnungen: Vec<&'static str>,
    kostenspeicher: sk_cost::Kostenspeicher,
    /// Eigene Speicher der Vorschau (Taste im Preisblatt), je Umfang einer:
    /// Ändert sie nur Preise, bleibt die Zuordnung (Review #197 B3). Ein
    /// gemeinsamer Speicher behielte nur die Typen des zuletzt gerechneten
    /// Umfangs (Review #197 H-197n). Dazu die Liste im Umfang, gemerkt je
    /// Berechnung der Liste: Sie hält ihre Schichtzeilen, die Taste rechnet
    /// sie nicht neu (Review #213 H-213b).
    kostenspeicher_live: Vec<LiveUmfang>,
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
    /// OK Gelände aus dem Aufmaß als Raster, zu seinem Schlüssel.
    gelaende_raster: Option<(
        Option<boden::RasterKey>,
        Option<Rc<sk_render::gelaende::TerrainGrid>>,
    )>,
    /// Laufender Übergang beim Aus- und Einblenden oder Isolieren.
    vis_anim: Option<VisAnim>,
    /// Masken je Zug und Teil (Review 3a G2), gültig für Revision,
    /// Sichtbarkeit und Übergang.
    vis_table: RefCell<VisTable>,
    /// Öffnungsschenkel und Züge je Feld für den Strahl unter der Maus (O7).
    kanten: RefCell<kanten::Raster>,
    /// Ein Schritt hätte dieses gesperrte Bauteil geändert (Paket 4).
    locked_hit: Option<ElementId>,
    /// Blasses Netz zum zuletzt zusammengesetzten Modellnetz.
    ghost: Option<(MeshKey, VisStamp, MeshData)>,
    /// Körper der Erweiterungsbauteile je Exemplar (E4).
    ext: crate::ext_cache::ExtCache,
    /// Körper des Ausbaus je Raum (Raumbuch RB4).
    ausbau: scene_ausbau::RaumCache,
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
            self.element_masks(id)
        })
    }

    /// Masken eines Bauteils im laufenden Übergang ([`Vis::masks_mit`]).
    fn element_masks(&self, id: ElementId) -> Masks {
        self.masks_mit(|v| self.model.masks_in(v, id))
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
    /// Perimeterdämmung unter der Platte (0 = keine) und OK Sohlplatte über
    /// OK Gelände (Gelände Themen 1 und 4).
    pub insulation: f64,
    pub terrain: f64,
    /// Höhe der Aufkantung des Flachdachs über dem OG (0 = kein
    /// Flachdach; Plan Flachdach P8: vorgewählt mit 50 cm).
    pub roof: f64,
}

impl Default for BuildingDraft {
    fn default() -> BuildingDraft {
        BuildingDraft {
            clear_eg: 2635.0,
            clear_og: 2635.0,
            floor_eg: sk_model::FLOOR_THICKNESS,
            floor_og: sk_model::FLOOR_THICKNESS,
            slab: sk_model::SLAB_THICKNESS,
            insulation: 0.0,
            terrain: 0.0,
            roof: sk_model::ROOF_UPSTAND,
        }
    }
}

/// Grenzen der Dialogfelder (mm): Decke 10–60 cm (B10), Sohlplatte 10–79 cm
/// (die Frostschürze bleibt unter der Platte, UK −0,80).
pub const DRAFT_FLOOR: (f64, f64) = (100.0, 600.0);
pub const DRAFT_SLAB: (f64, f64) = (100.0, sk_model::MAX_SLAB_THICKNESS);
pub const DRAFT_CLEAR: (f64, f64) = (sk_model::MIN_CLEAR, 10000.0);
/// Perimeterdämmung 2–30 cm oder 0 (keine); OK Sohle ±3 m über Gelände.
pub const DRAFT_INSULATION: (f64, f64) = (sk_model::MIN_PERIMETER, sk_model::MAX_PERIMETER);
pub const DRAFT_TERRAIN: (f64, f64) = (-sk_model::MAX_TERRAIN_OFFSET, sk_model::MAX_TERRAIN_OFFSET);
/// Aufkantung des Flachdachs 15–150 cm oder 0 (kein Flachdach).
pub const DRAFT_ROOF: (f64, f64) = (sk_model::MIN_ROOF_UPSTAND, sk_model::MAX_ROOF_UPSTAND);

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
        model.netz_im_faden(true);
        let theme = Theme::dark();
        let table = DrawTable::resolve(&model, &theme);
        let mut s = Scene {
            model,
            kennung: SZENEN.fetch_add(1, Ordering::Relaxed),
            decken: Default::default(),
            bauphase: Default::default(),
            undo: Vec::new(),
            serial: 0,
            redo: Vec::new(),
            rolle: sk_cost::Rolle::Admin,
            drain: None,
            neue_raeume: Vec::new(),
            erkennung: RefCell::default(),
            staende: HashMap::new(),
            cache: Vec::new(),
            raster: Default::default(),
            griffraster: Default::default(),
            dirty: Vec::new(),
            all_dirty: true,
            mass_stand: HashMap::new(),
            mass_alle: neuer_stand(),
            unsettled: Vec::new(),
            live: Vec::new(),
            gelaende_zug: None,
            nachzug: Vec::new(),
            netz_faden: None,
            bounds: None,
            grenzen: Default::default(),
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
            dickenrang: None,
            firmenkatalog: None,
            bezeichnungen: Vec::new(),
            kostenspeicher: sk_cost::Kostenspeicher::default(),
            kostenspeicher_live: Vec::new(),
            type_preview: false,
            now: 0,
            shown_key: None,
            grow: None,
            blend_shown: false,
            vis_rev: 0,
            gelaende_raster: None,
            vis_anim: None,
            vis_table: RefCell::default(),
            kanten: RefCell::default(),
            locked_hit: None,
            ghost: None,
            ext: Default::default(),
            ausbau: Default::default(),
        };
        s.rebuild_dirty(false);
        s
    }

    pub fn model(&self) -> &Model {
        &self.model
    }

    /// Unterscheidet Szenen (Neu, Öffnen, Sicherung) auch bei gleichem
    /// Modellstand, für Zwischenspeicher außerhalb der Szene (Review #87 R4).
    pub fn kennung(&self) -> u64 {
        self.kennung
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
        // Versatz je Gebäude (Entscheidung B): der des bisher aktiven
        // Gebäudes vorbelegt, Nachbarn stehen meist ähnlich
        let before = self.pending.and_then(|(_, a)| a);
        let terrain = before.map_or(0.0, |a| {
            self.model.terrain_offset_of(self.model.building_of(a))
        });
        self.draft = BuildingDraft {
            terrain,
            ..BuildingDraft::default()
        };
        self.apply_draft();
    }

    /// Vorgaben des offenen Dialogs.
    pub fn building_draft(&self) -> BuildingDraft {
        self.draft
    }

    /// Ein Feld des Dialogs „Gebäude erstellen“ (mm): `lichte_eg`,
    /// `lichte_og`, `decke_eg`, `decke_og`, `sohlplatte`, `perimeter` (0 =
    /// keine), `gelaende` (OK Sohle über Gelände), `flachdach` (Aufkantung,
    /// 0 = kein Flachdach). Gilt sofort im Paneel
    /// „Geschosse“; `false` außerhalb der Grenzen oder ohne Dialog.
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
            "perimeter" => (&mut d.insulation, DRAFT_INSULATION),
            "gelaende" => (&mut d.terrain, DRAFT_TERRAIN),
            "flachdach" => (&mut d.roof, DRAFT_ROOF),
            _ => return false,
        };
        let zero = matches!(field, "perimeter" | "flachdach") && mm == 0.0;
        if !(ok(range) || zero) {
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
        // Gelände sofort (Paneel „Geschosse“ zeigt es), die Dämmung erst mit
        // der Platte
        if let Some((b, _)) = self.pending {
            self.model.set_terrain_offset_of(b, d.terrain);
        }
        if let Some((eg, og)) = self.draft_storeys() {
            self.model.plan_storey_height(eg, d.clear_eg + d.floor_eg);
            if let Some(og) = og {
                self.model.plan_storey_height(og, d.clear_og + d.floor_og);
            }
            // Ebene Flachdach über dem OG (P8), die Aufkantung folgt beim
            // Zeichnen
            self.model.set_flat_roof(eg, d.roof > 0.0);
            if let Some(fd) = self.model.roof_level(eg).filter(|_| d.roof > 0.0) {
                self.model.plan_storey_height(fd, d.roof);
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
            // Die Dämmung baut sich nach unten auf, der Versatz bleibt
            if d.insulation > 0.0 {
                self.model.set_slab_insulation(slab, d.insulation);
            }
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

    /// Schatten einer Ansicht wählen (ohne Schritt, wie die Schnitte;
    /// Sonnenstand S7).
    pub fn set_view_shade(
        &mut self,
        i: usize,
        v: sk_model::ViewShade,
        vorgabe: sk_model::ViewShade,
    ) {
        self.model.set_view_shade(i, v, vorgabe);
    }

    /// Schnitthöhe des Grundrisses über OK Rohdecke (mm), Ansichtszustand
    /// ohne Schritt; die Grundrisse schneiden beim nächsten Netz neu.
    pub fn set_plan_cut_height(&mut self, h: f64) {
        self.model.set_plan_cut_height(h);
    }

    /// Ansicht `i`: unter dem Gelände gestrichelt oder ausgeblendet (S11),
    /// Ansichtszustand ohne Schritt.
    pub fn set_view_below(&mut self, i: usize, gestrichelt: bool) {
        self.model.set_view_below(i, gestrichelt);
    }

    /// Datum, Uhrzeit und Schalter des Sonnenstands-Systems merken (ohne
    /// Schritt, wie die Schnitte; Sonnenstand S4).
    pub fn set_sun(&mut self, s: sk_model::Sun) {
        self.model.set_sun(s);
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
    /// Schritt. `false`, wenn sich nichts geändert hat. Die App setzt sie
    /// samt Lage ([`Scene::projektdaten_setzen`]).
    #[cfg(test)]
    pub fn projekt_setzen(
        &mut self,
        label: &'static str,
        p: impl std::borrow::Borrow<sk_model::Project>,
    ) -> bool {
        self.begin(label);
        let ok = self.model.set_project(p.borrow().clone());
        self.commit();
        ok
    }

    /// Projektdaten eines neuen Projekts (Maske bei „Neu“, Paket PD-2):
    /// Anfangszustand ohne Rückgängig-Schritt.
    pub fn projekt_anfang(&mut self, p: sk_model::Project) {
        self.model.init_project(p);
    }

    /// Breite und Länge eines neuen Projekts: ohne Rückgängig-Schritt wie
    /// [`Scene::projekt_anfang`].
    pub fn lage_anfang(&mut self, ort: sk_model::Location) {
        self.model.init_location(ort);
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
        if m.flat_roof_coping(coping, floor) {
            return Some(sk_model::roof_coping_qto_of(
                m,
                floor,
                self.flat_roof_over(floor)?,
            ));
        }
        let run = m.run_of(floor)?;
        sk_model::coping_qto_of(m, floor, self.floor(run)?)
    }

    /// Flachdach auf der Decke `floor`, wie es gezeichnet wird (am Zug der
    /// Aufkantung darüber).
    pub fn flat_roof_over(&self, floor: ElementId) -> Option<&FlatRoof> {
        let m = &self.model;
        let ak = m.parapet_above(m.run_of(floor)?)?;
        self.cached(ak)?.roof.as_ref()
    }

    /// Mengen eines Dachaufbaus, aus dem gezeichneten Flachdach.
    pub fn flat_roof_qto(&self, roof: ElementId) -> Option<sk_model::TerraceQto> {
        let m = &self.model;
        let sk_model::ElementKind::Roof { floor, .. } = m.element(roof)?.kind else {
            return None;
        };
        Some(sk_model::flat_roof_qto_of(
            m,
            roof,
            self.flat_roof_over(floor)?,
        ))
    }

    /// Dämmdicke des Flachdachs (Paneel „Aufbau“, D3): ändert seinen Typ, ein
    /// Schritt.
    pub fn set_roof_insulation(&mut self, id: ElementId, mm: f64) -> bool {
        let m = &self.model;
        let Some(roof) = self.roof_element(id) else {
            return false;
        };
        let Some(t) = m.flat_roof_type(roof) else {
            return false;
        };
        let Some(mut set) = m.layer_set(t).cloned() else {
            return false;
        };
        match set
            .layers
            .iter_mut()
            .find(|l| l.function == sk_model::LayerFunction::Insulation)
        {
            Some(l) if l.thickness != mm => l.thickness = mm,
            _ => return false,
        }
        self.edit_model("Aufbau des Flachdachs", |m| m.set_layer_set(t, set))
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

    /// Fangziele eines Geschosses ([`Model::fangziele`]) aus den berechneten
    /// Wandzügen (Review #82 V4).
    pub fn fangziele(&self, storey: sk_model::StoreyId) -> sk_model::Fangziele {
        use std::borrow::Cow;
        self.model.fangziele_aus(storey, |id| {
            self.chain(id)
                .map(Cow::Borrowed)
                .or_else(|| self.model.chain(id).map(Cow::Owned))
        })
    }

    /// Fugen eines Wandzugs (siehe [`WallChain::straight_joints`]), mit
    /// dem Zug berechnet.
    pub fn joints(&self, run: RunId) -> &[usize] {
        self.cached(run).map_or(&[], |c| c.joints.as_slice())
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
        // Erst das Netz des letzten Schritts einsetzen, damit seine Folgen
        // zu diesem Schritt kommen und nicht zum neuen
        self.netz_fertig();
        self.live.clear();
        self.staende_fuellen();
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
        self.gelaende_loslassen();
        // Räume folgen den Wänden im selben Schritt (G3)
        let raeume = self.raeume_abgleichen();
        match self.model.try_commit() {
            Ok(Some(txn)) => {
                self.ausbau.merken(&self.model, &txn.changes);
                self.undo.push(raum::Schritt { txn, raeume });
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
                self.staende.clear();
            }
        }
        self.live.clear();
        self.rebuild_dirty(false);
        self.settle();
        self.netz_anstossen();
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
        self.gelaende_zug = None;
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
                // Dachaufbau und Blech hängen an der Aufkantung darüber
                if let Some(ak) = self.model.parapet_above(id) {
                    self.mark(ak);
                }
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
    /// verworfen. `false`, wenn ein Zug den Typ nicht annimmt; dann ist
    /// nichts geändert und keine Vorschau offen.
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
        if !ok {
            // Lehnt ein Zug ab, wechselt keiner (Review H-163d)
            self.type_preview = false;
            self.rollback();
            return false;
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
            return field == Field::RaumFliesen && self.raum_fliesenhoehe(id, mm);
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
        if field == Field::RoofInsulation {
            return self.set_roof_insulation(id, mm);
        }
        if field == Field::RoofSlope {
            return self.set_roof_slope(id, mm);
        }
        if field == Field::DownpipeSpacing {
            return self.set_downpipe_spacing(id, mm);
        }
        if let Field::Cladding(v) = field {
            return self.set_cladding(id, v, mm);
        }
        if matches!(
            field,
            Field::TerraceInsulation
                | Field::TerraceFinish
                | Field::Upstand
                | Field::Crown
                | Field::AttikaInner
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
        if field == Field::Insulation {
            if s.insulation == mm {
                return false;
            }
            // baut sich nach unten auf; eine zu flache Schürze wächst mit
            // (UK Gründung für alle Gründungen des Gebäudes)
            return self.edit_model("Perimeterdämmung", |m| m.set_slab_insulation(slab, mm));
        }
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
            // kann UK Gründung verschieben (Schürze wächst, PR #51): alle
            // Gründungen des Gebäudes ändern sich
            (Field::SlabThickness, _) => {
                let ok = self.model.set_slab_thickness(slab, mm);
                self.mark_all();
                ok
            }
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
        let mut l = Levels {
            plan_cut: (!self.is_foundation(active)).then(|| self.plan_cut()),
            ..Levels::default()
        };
        let row = |field, label, value, (min, max): (f64, f64)| FieldRow {
            field,
            label,
            value,
            min,
            max,
            zero: false,
            einheit: None,
        };
        for id in self.level_ids() {
            let Some(st) = m.storey(id) else {
                continue;
            };
            let foundation = st.kind == sk_model::LevelKind::Foundation;
            let roof = st.kind == sk_model::LevelKind::Roof;
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
                roof,
                active: id == active,
                lines: (!foundation && !roof).then(|| {
                    let (l, e) = (st.lines, st.elevation);
                    (e + l.sill, e + l.lintel, e + l.door)
                }),
            });
            if foundation {
                // OK Gelände und Einbindetiefe im Fuß (Gelände Thema 1); mit
                // Aufmaß der tiefste Punkt am Plattenrand, ab dem die
                // Einbindetiefe zählt (G1)
                let tz = m.terrain_z_of_level(id);
                l.terrain = Some(tz);
                l.embedment = m.embedment_of(id);
                let range = m.terrain_z_of_level_range(id);
                l.fields.push(row(Field::Terrain, "OK Gelände", tz, range));
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
            let (e, h) = (st.elevation, st.height);
            // Flachdach (Jörn 10.10.): OK und Höhe der Aufkantung, keine
            // lichte Höhe
            if roof {
                l.fields.push(row(
                    Field::LevelTop(id),
                    "OK Aufkantung",
                    st.top(),
                    (lo, hi),
                ));
                l.fields.push(row(
                    Field::StoreyHeight(id),
                    "Aufkantung",
                    h,
                    (lo - e, hi - e),
                ));
                continue;
            }
            l.fields
                .push(row(Field::LevelTop(id), "Oberkante", st.top(), (lo, hi)));
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
        // Schalter „Flachdach“, sobald es ein Geschoss gibt
        if l.bands.iter().any(|b| !b.foundation && !b.roof) {
            l.flat_roof = Some(m.roof_level(active).is_some());
        }
        l
    }

    /// Schaltet die Ebene Flachdach des aktiven Gebäudes als ein Schritt
    /// (Paneel „Geschosse“, Jörn 10.10.); `false`, wenn nichts geschah.
    pub fn set_flat_roof(&mut self, on: bool) -> bool {
        let st = self.active_storey();
        if self.model.roof_level(st).is_some() == on {
            return false;
        }
        self.edit_model("Flachdach", |m| m.set_flat_roof(st, on))
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
            // Kote OK Gelände an der Gründung: ohne Aufmaß der Gegenwert
            // des Versatzes, mit Aufmaß wandert ±0,00 gegen das Gelände
            Field::Terrain => match self.model.foundation_level_of(self.active_storey()) {
                Some(gr) => self.edit_model("Gelände", |m| m.set_terrain_z_of_level(gr, mm)),
                None => false,
            },
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
        self.decke_markieren(run);
        self.commit();
        ok
    }

    /// Dicke der Untersichtdämmung an der Decke `id` (oder an der Decke der
    /// Untersichtdämmung `id`): ein Schritt „Untersichtdämmung“; die
    /// Außenschichten wandern mit.
    /// Feld im Abschnitt „Aufbau“ der Dachterrasse (D1, H152): Dämmung und
    /// Belag ändern ihren Typ, die Attika steht an der Decke. Ein Schritt.
    pub fn set_terrace_field(&mut self, id: ElementId, field: Field, mm: f64) -> bool {
        use sk_model::ElementKind::{Coping, Floor, Roof, RoofTerrace};
        let m = &self.model;
        let floor = match m.element(id).map(|e| &e.kind) {
            Some(RoofTerrace { floor } | Coping { floor } | Roof { floor, .. }) => *floor,
            Some(Floor(_)) => id,
            _ => return false,
        };
        if field == Field::Upstand {
            return self.edit_model("Attika", |m| m.set_floor_upstand(floor, mm));
        }
        if field == Field::Crown {
            return self.edit_model("Dämmung auf der Attika", |m| m.set_floor_crown(floor, mm));
        }
        if field == Field::AttikaInner {
            return self.edit_model("Attikadämmung innen", |m| m.set_floor_inner(floor, mm));
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

    /// Ein Wert der Bekleidung unter der Untersichtdämmung (W3) an der Decke
    /// von `id` (Decke oder Untersichtdämmung); ein Schritt im Verlauf.
    pub fn set_cladding(&mut self, id: ElementId, v: sk_model::CladdingValue, mm: f64) -> bool {
        let m = &self.model;
        let floor = match m.element(id).map(|e| &e.kind) {
            Some(sk_model::ElementKind::SoffitInsulation { floor }) => *floor,
            Some(sk_model::ElementKind::Floor(_)) => id,
            _ => return false,
        };
        match m.element(floor).map(|e| &e.kind) {
            Some(sk_model::ElementKind::Floor(f)) if v.of(&f.soffit) != mm => {}
            _ => return false,
        }
        self.begin("Untersicht-Bekleidung");
        let ok = self.model.set_floor_cladding(floor, v, mm);
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
    pub fn set_run_points(&mut self, run: RunId, points: &[Vec3]) -> bool {
        let Some(runs) = self.model.set_run_points(run, points) else {
            return false;
        };
        for r in runs {
            self.mark(r);
            if !self.live.contains(&r) {
                self.live.push(r);
            }
        }
        self.rebuild_dirty(true);
        true
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
        self.set_run_points(run, &pts)
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
            .is_some_and(|t| GROW_STEPS.contains(&t.txn.label));
        self.grown(grow, |s| s.step(Direction::Undo))
    }

    /// Bezeichnung des Schritts, den „Rückgängig“ als Nächstes zurücknimmt
    /// (ein Gebäude im Entstehen: „Gebäude erstellt“).
    pub fn undo_label(&self) -> Option<&'static str> {
        if self.pending.is_some() {
            return Some("Gebäude erstellt");
        }
        self.undo.last().map(|t| t.txn.label)
    }

    /// Bezeichnung des Schritts, den „Wiederherstellen“ als Nächstes ausführt.
    pub fn redo_label(&self) -> Option<&'static str> {
        if self.pending.is_some() {
            return None;
        }
        self.redo.last().map(|t| t.txn.label)
    }

    pub fn redo(&mut self) -> bool {
        if self.pending.is_some() {
            return false;
        }
        let grow = self
            .redo
            .last()
            .is_some_and(|t| GROW_STEPS.contains(&t.txn.label));
        self.grown(grow, |s| s.step(Direction::Redo))
    }
}

/// Netz eines Körpers. Es trägt nur Darstellungsschlüssel und Kantenarten;
/// wie sie aussehen (3D oder Zeichnung), steht in der Tabelle des Renderers.
pub fn mesh_of(s: &Solid) -> MeshData {
    let mut m = MeshData::default();
    mesh_into(&mut m, s, Boden::eben(0.0));
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

/// Der Stammsatz, den eine Operation des Preisblatts oder der Lohnkarte
/// setzt.
fn ziel(op: &sk_cost::Op) -> Option<sk_cost::SatzId> {
    use sk_cost::{Op, SatzId};
    match op {
        Op::PreisSetzen { artikel, .. } | Op::UmrechnungSetzen { artikel, .. } => {
            Some(SatzId::neu("article", artikel.to_ifc()))
        }
        Op::BauleistungAendern { bauleistung, .. } => {
            Some(SatzId::neu("service", bauleistung.to_ifc()))
        }
        Op::FirmenwertSetzen { schluessel, .. } => Some(SatzId::neu("rate", schluessel.clone())),
        _ => None,
    }
}

#[cfg(test)]
#[path = "scene_tests.rs"]
mod tests;
