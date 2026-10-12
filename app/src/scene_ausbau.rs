//! Ausbau innen in der Szene (Raumbuch RB4, `raumbuch/plan.md` §8): die
//! Körper des Ausbaus je Raum ([`sk_raum::raumbuch::koerper`]). Den Estrich
//! zeigen Schnitt und 3D immer; Innenputz, Wandbeläge, Deckenputz,
//! Bodenbelag und Sockelleisten nur, wo der Schalter „Ausbau innen“ der
//! Ansicht an ist ([`sk_model::FINISH_VIEWS`]). Der Grundriss zeigt ohne
//! Schalter nichts davon, mit ihm die Räume des aktiven Geschosses in
//! seiner Schnitthöhe.
//!
//! Zwischenspeicher je Geschoss, Hausgruppe und Raum (Review #112 B1: nie
//! je Revision alle Räume): Ein Geschoss wird nur angesehen, wenn sich seine
//! Wandzüge (Fassung der Szene), seine Räume, die Geschosse seines Gebäudes,
//! die Raumvorlagen oder die Typen geändert haben; darin eine Hausgruppe der
//! Erkennung nur, wenn ihre Flächen, die Fassungen ihrer Züge, ihre Räume
//! oder eins der anderen drei anders sind. Erst dann rechnet ein Raum
//! Rohbau und Ausbau, und seine Körper nur, wenn sich das ändert, was sie
//! bestimmt. Baut die Szene alle Züge neu (Baustoffe, Geschosse), prüft
//! jeder Raum. Ein Raum ohne gespeicherten Raum (Datei vor G3, vor dem
//! ersten Wandschritt) zeigt keinen Ausbau.

use super::{class_in, mesh_into, Class, Plane, Scene, Vis};
use crate::scene::MeshData;
use crate::ui::ViewKind;
use sk_math::vec3;
use sk_model::{
    ElementId, ElementKind, LayerSet, LayerSetId, LevelKind, Model, RunId, Solid, Space, SpaceType,
    StoreyId,
};
use sk_raum::ausbau::RaumAusbau;
use sk_raum::oeffnungen::Rohbau;
use sk_raum::raumbuch::koerper::{koerper, Koerper};
use sk_raum::raumbuch::Flaeche;
use sk_raum::{Begrenzung, Ketten, Raumflaeche};
use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::rc::Rc;

/// Was die Körper eines Raums bestimmt.
#[derive(Clone, Debug, PartialEq)]
struct Schluessel {
    roh: Rohbau,
    aus: RaumAusbau,
    /// Die Ausbautypen des Raums, wie sie gerade sind.
    typen: Vec<LayerSet>,
}

/// Estrich und alles andere.
type Paar = [Solid; 2];

struct Eintrag {
    id: ElementId,
    storey: StoreyId,
    schluessel: Schluessel,
    koerper: Vec<Koerper>,
    voll: Paar,
    /// Grundrisse je Schnitthöhe und Schnitte je Ebene, zuletzt gefragte
    /// zuerst.
    plan: Vec<(f64, Paar)>,
    section: Vec<(Plane, Paar)>,
}

/// So viele Grundrisse und Schnitte behält ein Raum.
const KEEP: usize = 3;

/// Eine Hausgruppe, wie die Erkennung sie gibt (dieselben Flächen, solange
/// sie aus ihrem Speicher kommen), mit den Wandzügen an ihren Räumen und
/// dem Fingerabdruck, mit dem diese zuletzt gerechnet wurden.
struct Gruppe {
    flaechen: Rc<[Raumflaeche]>,
    zuege: Vec<RunId>,
    kennung: u64,
}

/// Ein Geschoss mit seinem Fingerabdruck beim letzten Abgleich.
struct Geschoss {
    id: StoreyId,
    kennung: Kennung,
    gruppen: Vec<Gruppe>,
    items: Vec<Eintrag>,
}

#[derive(Default)]
pub(crate) struct RaumCache {
    rev: Option<u64>,
    /// Raumvorlagen und Typen des Projekts beim letzten Abgleich; ändert
    /// sich eins davon, zählt `stand` weiter.
    vorlagen: Vec<SpaceType>,
    typen: Vec<(LayerSetId, LayerSet)>,
    stand: u64,
    geschosse: Vec<Geschoss>,
    /// Wie oft die Körper eines Raums gerechnet wurden (Tests).
    pub builds: u64,
    /// Wie oft ein Raum Rohbau und Ausbau gerechnet hat (Tests).
    pub geprueft: u64,
}

/// Estrich und Rest, je aus den Körpern zusammengesetzt.
fn paar(k: &[Koerper], f: impl Fn(&Koerper) -> Solid) -> Paar {
    let mut p = [Solid::default(), Solid::default()];
    for x in k {
        let i = usize::from(x.flaeche != Flaeche::Estrich);
        p[i].append(&f(x));
    }
    p
}

/// Ausbautypen, die der Ausbau `a` nennt.
fn typen(m: &Model, a: &RaumAusbau) -> Vec<LayerSet> {
    let mut ids = vec![a.boden, a.estrich, a.decke];
    for s in &a.seiten {
        ids.extend([s.wand, s.fliesen.map(|f| f.0), s.sockel]);
    }
    let mut out: Vec<LayerSet> = Vec::new();
    let mut gesehen = Vec::new();
    for id in ids.into_iter().flatten() {
        if !gesehen.contains(&id) {
            gesehen.push(id);
            out.extend(m.layer_set(id).cloned());
        }
    }
    out
}

/// Was ein Raum für die Zuordnung und seinen Ausbau trägt.
fn raum_in(h: &mut DefaultHasher, id: ElementId, s: &Space) {
    id.hash(h);
    [s.anker.x, s.anker.y].map(f64::to_bits).hash(h);
    s.stempel.map(|p| [p.x, p.y].map(f64::to_bits)).hash(h);
    s.nutzung.hash(h);
}

/// Fassung eines Zugs; ohne Fassung (eben geändert) jedes Mal anders.
fn fassung_in(h: &mut DefaultHasher, k: &impl Ketten, rev: u64, run: RunId) {
    run.hash(h);
    match k.fassung(run) {
        Some(f) => (0u8, f).hash(h),
        None => (1u8, rev).hash(h),
    }
}

/// Fingerabdruck eines Geschosses: `gebaeude` aus den Geschossen seines
/// Gebäudes (Höhen, dickste Decke darüber) und dem Stand von Vorlagen und
/// Typen, `alles` dazu aus den Fassungen seiner Züge und seinen Räumen.
#[derive(Clone, Copy, Default, PartialEq)]
struct Kennung {
    gebaeude: u64,
    alles: u64,
}

/// Fingerabdrücke aller Geschosse aus einem Gang über Bauteile und
/// Wandzüge.
fn geschoss_kennungen(m: &Model, k: &impl Ketten, stand: u64) -> HashMap<StoreyId, Kennung> {
    let mut decke: HashMap<StoreyId, f64> = HashMap::new();
    let mut raeume: HashMap<StoreyId, DefaultHasher> = HashMap::new();
    for (id, e) in m.elements().iter() {
        match &e.kind {
            ElementKind::Floor(f) => {
                let d = decke.entry(f.top.storey).or_insert(f.thickness);
                *d = d.max(f.thickness);
            }
            ElementKind::Space(s) => raum_in(raeume.entry(e.storey).or_default(), id, s),
            _ => {}
        }
    }
    let mut gebaeude: HashMap<Option<sk_model::BuildingId>, DefaultHasher> = HashMap::new();
    for (id, st) in m.storeys().iter() {
        let h = gebaeude.entry(st.building).or_default();
        id.hash(h);
        (st.kind as u8).hash(h);
        [st.elevation, st.height].map(f64::to_bits).hash(h);
        decke.get(&id).map(|d| d.to_bits()).hash(h);
    }
    let rev = m.revision();
    let mut zuege: HashMap<StoreyId, DefaultHasher> = HashMap::new();
    for (id, r) in m.runs().iter() {
        fassung_in(zuege.entry(r.storey).or_default(), k, rev, id);
    }
    m.storeys()
        .iter()
        .map(|(id, st)| {
            let mut h = DefaultHasher::new();
            (stand, gebaeude.get(&st.building).map(|g| g.finish())).hash(&mut h);
            let gebaeude = h.finish();
            zuege.get(&id).map(|z| z.finish()).hash(&mut h);
            raeume.get(&id).map(|r| r.finish()).hash(&mut h);
            let alles = h.finish();
            (id, Kennung { gebaeude, alles })
        })
        .collect()
}

/// Wandzüge an den Räumen einer Hausgruppe.
fn zuege_der(m: &Model, flaechen: &[Raumflaeche]) -> Vec<RunId> {
    let mut v: Vec<RunId> = Vec::new();
    for f in flaechen {
        for k in f.form.umriss.iter().chain(f.form.loecher.iter().flatten()) {
            let Begrenzung::Wand { wand, .. } = k.quelle;
            if let Some((run, _)) = m.segment_of(wand) {
                if !v.contains(&run) {
                    v.push(run);
                }
            }
        }
    }
    v
}

/// Fingerabdruck einer Hausgruppe: Gebäude und Stand ([`Kennung`]), die
/// Fassungen ihrer Züge und ihre Räume. Was sonst im Geschoss geschieht,
/// geht sie nichts an.
fn gruppe_kennung(m: &Model, k: &impl Ketten, gebaeude: u64, gr: &Gruppe) -> u64 {
    let mut h = DefaultHasher::new();
    gebaeude.hash(&mut h);
    for &run in &gr.zuege {
        fassung_in(&mut h, k, m.revision(), run);
    }
    for f in gr.flaechen.iter() {
        if let Some((id, s)) = f.form.raum.and_then(|id| Some((id, m.space(id)?))) {
            raum_in(&mut h, id, s);
        }
    }
    h.finish()
}

impl RaumCache {
    /// Gleicht den Speicher mit dem Modell ab (einmal je Modellstand), die
    /// Flächen aus der Erkennung der Szene, die Züge aus ihren berechneten.
    fn sync(&mut self, s: &Scene) {
        let m = s.model();
        if self.rev == Some(m.revision()) {
            return;
        }
        self.rev = Some(m.revision());
        let k = s.ketten();
        // Eine geänderte Raumvorlage oder ein geänderter Typ kann jeden Raum
        // treffen; seine Körper bleiben, wenn sie ihn nicht trifft
        let typen = m.layer_sets();
        if self.vorlagen.as_slice() != m.space_types()
            || self.typen.len() != typen.len()
            || !self
                .typen
                .iter()
                .zip(typen.iter())
                .all(|(a, b)| a.0 == b.0 && a.1 == *b.1)
        {
            self.vorlagen = m.space_types().to_vec();
            self.typen = typen.iter().map(|(id, t)| (id, t.clone())).collect();
            self.stand += 1;
        }
        let kennungen = geschoss_kennungen(m, &k, self.stand);
        let mut alt: HashMap<StoreyId, Geschoss> = std::mem::take(&mut self.geschosse)
            .into_iter()
            .map(|g| (g.id, g))
            .collect();
        for (g, st) in m.storeys().iter() {
            if st.kind != LevelKind::Storey {
                continue;
            }
            let kennung = kennungen.get(&g).copied().unwrap_or_default();
            let vorher = alt.remove(&g);
            let neu = match vorher {
                Some(x) if x.kennung == kennung => x,
                x => self.geschoss(s, &k, g, kennung, x),
            };
            self.geschosse.push(neu);
        }
    }

    /// Ein Geschoss neu ansehen: je Hausgruppe der Erkennung, die anders
    /// ist als vorher, ihre Räume.
    fn geschoss(
        &mut self,
        s: &Scene,
        k: &impl Ketten,
        g: StoreyId,
        kennung: Kennung,
        vorher: Option<Geschoss>,
    ) -> Geschoss {
        let m = s.model();
        let (gruppen, items) = vorher.map_or((Vec::new(), Vec::new()), |x| (x.gruppen, x.items));
        // Nach dem Zeiger ihrer Flächen; die alte Gruppe hält sie am Leben
        let mut gruppen: HashMap<usize, Gruppe> = gruppen
            .into_iter()
            .map(|gr| (Rc::as_ptr(&gr.flaechen).cast::<()>() as usize, gr))
            .collect();
        let mut alt: HashMap<ElementId, Eintrag> = items.into_iter().map(|x| (x.id, x)).collect();
        let mut out = Geschoss {
            id: g,
            kennung,
            gruppen: Vec::new(),
            items: Vec::new(),
        };
        for flaechen in s.raumgruppen(g).gruppen {
            let zeiger = Rc::as_ptr(&flaechen).cast::<()>() as usize;
            let vorige = gruppen.remove(&zeiger);
            let mut gr = Gruppe {
                zuege: vorige
                    .as_ref()
                    .map_or_else(|| zuege_der(m, &flaechen), |v| v.zuege.clone()),
                flaechen,
                kennung: 0,
            };
            gr.kennung = gruppe_kennung(m, k, kennung.gebaeude, &gr);
            let gleich = vorige.is_some_and(|v| v.kennung == gr.kennung);
            for f in gr.flaechen.iter() {
                let Some(id) = f.form.raum else {
                    continue;
                };
                match alt.remove(&id).filter(|x| x.storey == g) {
                    Some(x) if gleich => out.items.push(x),
                    x => {
                        if let Some(e) = self.raum(m, k, g, f, x) {
                            out.items.push(e);
                        }
                    }
                }
            }
            out.gruppen.push(gr);
        }
        out
    }

    /// Rohbau und Ausbau eines Raums; die Körper nur, wenn sie sich ändern.
    fn raum(
        &mut self,
        m: &Model,
        k: &impl Ketten,
        g: StoreyId,
        f: &Raumflaeche,
        alt: Option<Eintrag>,
    ) -> Option<Eintrag> {
        let id = f.form.raum?;
        self.geprueft += 1;
        let roh = Rohbau::mit(m, k, f.form.clone());
        let aus = RaumAusbau::fuer(m, &roh)?;
        let s = Schluessel {
            typen: typen(m, &aus),
            roh,
            aus,
        };
        if let Some(x) = alt.filter(|x| x.schluessel == s) {
            return Some(x);
        }
        let k = koerper(m, &s.roh, &s.aus);
        self.builds += 1;
        Some(Eintrag {
            id,
            storey: g,
            voll: paar(&k, |x| x.solid.clone()),
            koerper: k,
            schluessel: s,
            plan: Vec::new(),
            section: Vec::new(),
        })
    }

    /// Körper je Raum für eine Ansicht: je Raum der Estrich und, mit
    /// `alles`, der übrige Ausbau. Grundriss: nur die Räume des Geschosses,
    /// geschnitten in Schnitthöhe; Schnitt: geschnitten an der Ebene; sonst
    /// die ganzen Körper.
    fn shown(
        &mut self,
        plan: Option<(StoreyId, f64)>,
        section: Option<Plane>,
        alles: bool,
    ) -> Vec<(ElementId, &Solid)> {
        for x in self.geschosse.iter_mut().flat_map(|g| g.items.iter_mut()) {
            match (plan, section) {
                (Some((st, cut)), _) if x.storey == st => {
                    if let Some(i) = x.plan.iter().position(|p| p.0 == cut) {
                        let hit = x.plan.remove(i);
                        x.plan.insert(0, hit);
                    } else {
                        let p0 = vec3(0.0, 0.0, cut);
                        let s = paar(&x.koerper, |k| k.geschnitten(p0, vec3(0.0, 0.0, 1.0)));
                        x.plan.truncate(KEEP - 1);
                        x.plan.insert(0, (cut, s));
                    }
                }
                (None, Some(pl)) => {
                    if let Some(i) = x.section.iter().position(|p| p.0 == pl) {
                        let hit = x.section.remove(i);
                        x.section.insert(0, hit);
                    } else {
                        let s = paar(&x.koerper, |k| k.geschnitten(pl.0, pl.1));
                        x.section.truncate(KEEP - 1);
                        x.section.insert(0, (pl, s));
                    }
                }
                _ => {}
            }
        }
        let mut out = Vec::new();
        for x in self.geschosse.iter().flat_map(|g| g.items.iter()) {
            let p = match (plan, section) {
                (Some((st, _)), _) if x.storey != st => continue,
                (Some(_), _) => &x.plan[0].1,
                (None, Some(_)) => &x.section[0].1,
                (None, None) => &x.voll,
            };
            out.push((x.id, &p[0]));
            if alles {
                out.push((x.id, &p[1]));
            }
        }
        out
    }
}

/// Platz der Ansicht in [`sk_model::FINISH_VIEWS`]; `None` in den vier
/// Ansichten.
pub fn platz(v: ViewKind) -> Option<usize> {
    match v {
        ViewKind::Plan => Some(0),
        ViewKind::Section => Some(1),
        ViewKind::Persp => Some(2),
        _ => None,
    }
}

impl Scene {
    /// Schalter „Ausbau innen“ der Ansicht `i` (Ansichtszustand ohne
    /// Schritt, wie die Schnitthöhe).
    pub fn set_view_finish(&mut self, i: usize, an: bool) {
        self.model.set_view_finish(i, an);
    }

    /// Was zu den Wandzügen ins ruhende Netz kommt: Erweiterungen und
    /// Ausbau innen. Das Live-Netz beim Ziehen ([`Scene::mesh_runs`]) trägt
    /// nur die gezogenen Züge, sonst stünde beides doppelt im Bild.
    pub(super) fn zusatz_into(
        &mut self,
        m: &mut [MeshData; 2],
        view: ViewKind,
        section: Option<Plane>,
    ) {
        self.ext_into(m, view, section);
        self.ausbau_into(m, view, section);
    }

    /// Ausbau innen ins Netz, deckend oder blass wie die Erweiterungen.
    fn ausbau_into(&mut self, m: &mut [MeshData; 2], view: ViewKind, section: Option<Plane>) {
        let Some(i) = platz(view) else {
            return;
        };
        let alles = self.model.view_finish(i);
        if view == ViewKind::Plan && !alles {
            return;
        }
        let Some((plan, section)) = self.ext_view(view, section) else {
            return;
        };
        // Der Speicher liest die Szene (Erkennung, berechnete Züge)
        let mut c = std::mem::take(&mut self.ausbau);
        c.sync(self);
        self.ausbau = c;
        let filter = self.filtering();
        let model = &self.model;
        let ground = |id: ElementId| model.terrain_z_of(model.building_of_element(id));
        let vis = Vis {
            model: &self.model,
            anim: self.vis_anim.as_ref(),
            table: &self.vis_table,
            stamp: self.vis_stamp(),
        };
        for (id, s) in self.ausbau.shown(plan, section, alles) {
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

    /// Wie oft die Körper eines Raums gerechnet wurden (Tests).
    #[cfg(test)]
    pub fn ausbau_builds(&self) -> u64 {
        self.ausbau.builds
    }

    /// Wie oft ein Raum Rohbau und Ausbau gerechnet hat (Tests).
    #[cfg(test)]
    pub fn ausbau_geprueft(&self) -> u64 {
        self.ausbau.geprueft
    }
}

#[cfg(test)]
mod tests {
    use crate::scene::{MeshData, Scene};
    use crate::ui::ViewKind;
    use sk_math::vec3;
    use sk_model::model::{
        BATH_SPACE_GUID, SCREED_SLAB_TYPE_GUID, SPACE_TEMPLATES, WOODCHIP_WALL_TYPE_GUID,
    };
    use sk_model::{
        material, material_key, Category, ElementId, Guid, Model, RefSide, RunId, WallChain,
    };

    /// Haus 10 × 8 m mit zwei Geschossen; der Raum im EG hat seinen Ausbau
    /// (Wohnzimmer).
    fn haus() -> (Scene, ElementId) {
        let mut s = Scene::with_model(Model::with_seed(85));
        assert!(s.edit_model("Haus", |m| {
            let b = m.add_building(2);
            let pts = [
                vec3(0.0, 0.0, 0.0),
                vec3(0.0, 8000.0, 0.0),
                vec3(10000.0, 8000.0, 0.0),
                vec3(10000.0, 0.0, 0.0),
            ];
            m.build_from_polygon(b, &pts).is_some()
        }));
        let m = s.model();
        let eg = m.levels_in(m.buildings().iter().next().map(|b| b.0))[1];
        let r = m.spaces(eg)[0];
        assert!(s.edit_model("Ausbau", |m| sk_raum::ausbau::typen_anlegen(m, r)));
        (s, r)
    }

    /// Baustoffschlüssel der Schichten eines Typs.
    fn schluessel(s: &Scene, g: Guid) -> Vec<u16> {
        let m = s.model();
        let t = m.layer_set(m.type_by_guid(g).unwrap()).unwrap();
        t.layers.iter().map(|l| material_key(l.material)).collect()
    }

    /// Flächen mit einem der Schlüssel, geschnitten (`schnitt`) oder nicht.
    fn flaechen(m: &MeshData, k: &[u16], schnitt: bool) -> usize {
        m.faces
            .iter()
            .map(|f| f[6] as u16)
            .filter(|x| (x & material::CUT != 0) == schnitt && k.contains(&(x & !material::CUT)))
            .count()
    }

    /// Das Live-Netz beim Ziehen trägt nur die gezogenen Züge: ruhendes und
    /// Live-Netz ergeben zusammen das ganze, Ausbau und Erweiterungen
    /// stehen nur einmal darin.
    #[test]
    fn nicht_doppelt_im_live_netz() {
        let (mut s, _) = haus();
        s.set_view_finish(2, true);
        let estrich = schluessel(&s, SCREED_SLAB_TYPE_GUID);
        let a = s.model().runs().iter().next().unwrap().0;
        let live = s.live_set(a);
        let ganz = s.mesh(ViewKind::Persp, None, &[]);
        let rest = s.mesh(ViewKind::Persp, None, &live);
        let gezogen = s.mesh_runs(ViewKind::Persp, None, &live);
        assert!(flaechen(&rest, &estrich, false) > 0);
        assert_eq!(flaechen(&gezogen, &estrich, false), 0);
        assert_eq!(ganz.faces.len(), rest.faces.len() + gezogen.faces.len());
        assert_eq!(ganz.edges.len(), rest.edges.len() + gezogen.edges.len());
    }

    /// Estrich im Schnitt und im 3D immer, Putz nur mit dem Schalter der
    /// Ansicht; im Grundriss ohne Schalter nichts. Umschalten rechnet keinen
    /// Raum neu.
    #[test]
    fn ausbau_innen_je_ansicht() {
        let (mut s, _) = haus();
        let estrich = schluessel(&s, SCREED_SLAB_TYPE_GUID);
        let putz: Vec<u16> = schluessel(&s, WOODCHIP_WALL_TYPE_GUID)
            .into_iter()
            .filter(|k| !estrich.contains(k))
            .collect();
        let quer = Some((vec3(5000.0, 4000.0, 0.0), vec3(1.0, 0.0, 0.0)));
        let m = s.mesh(ViewKind::Section, quer, &[]);
        assert!(flaechen(&m, &estrich, true) > 0, "Estrich geschnitten");
        assert_eq!(flaechen(&m, &putz, true) + flaechen(&m, &putz, false), 0);
        let gebaut = s.ausbau_builds();
        assert!(gebaut >= 1);
        s.set_view_finish(1, true);
        let m = s.mesh(ViewKind::Section, quer, &[]);
        assert!(flaechen(&m, &putz, true) > 0, "Putz geschnitten");
        // Grundriss: ohne Schalter nichts, mit ihm Putz in Schnitthöhe
        let m = s.mesh(ViewKind::Plan, None, &[]);
        assert_eq!(flaechen(&m, &estrich, false) + flaechen(&m, &putz, true), 0);
        s.set_view_finish(0, true);
        let m = s.mesh(ViewKind::Plan, None, &[]);
        assert!(flaechen(&m, &putz, true) > 0);
        assert!(
            m.faces
                .iter()
                .filter(|f| f[6] as u16 & material::CUT != 0
                    && putz.contains(&(f[6] as u16 & !material::CUT)))
                .all(|f| f[5] > 0.99),
            "Schnittfläche nach oben"
        );
        // 3D: Estrich immer, Putz mit Schalter
        let m = s.mesh(ViewKind::Persp, None, &[]);
        assert!(flaechen(&m, &estrich, false) > 0);
        assert_eq!(flaechen(&m, &putz, false), 0);
        s.set_view_finish(2, true);
        let m = s.mesh(ViewKind::Persp, None, &[]);
        assert!(flaechen(&m, &putz, false) > 0);
        // Ansichten ohne Ausbau
        let m = s.mesh(ViewKind::Front, None, &[]);
        assert_eq!(
            flaechen(&m, &estrich, false) + flaechen(&m, &putz, false),
            0
        );
        assert_eq!(s.ausbau_builds(), gebaut, "Umschalten rechnet nichts neu");
    }

    /// Häuser 10 × 8 m nebeneinander, je mit zwei Innenwänden: drei Räume
    /// je Haus im aktiven Geschoss, einer darüber; die Innenwände.
    fn haeuser(s: &mut Scene, von: usize, bis: usize) -> Vec<RunId> {
        let mut innen = Vec::new();
        for i in von..bis {
            let ox = i as f64 * 15000.0;
            let p = |x: f64, y: f64| vec3(ox + x, y, 0.0);
            let wand = |points, closed, ref_side| WallChain {
                base: 0.0,
                points,
                closed,
                ref_side,
                layers: Vec::new(),
                height: 2750.0,
                joints: Default::default(),
            };
            let aussen = vec![
                p(0.0, 0.0),
                p(0.0, 8000.0),
                p(10000.0, 8000.0),
                p(10000.0, 0.0),
            ];
            s.add_wall(&wand(aussen, true, RefSide::Left));
            for x in [3500.0, 6500.0] {
                let w = wand(vec![p(x, 315.0), p(x, 7685.0)], false, RefSide::Center);
                innen.extend(s.add_wall_as(&w, Category::InteriorWall));
            }
        }
        innen
    }

    /// Geprüfte und gebaute Räume seit dem letzten Aufruf, nach dem Netz
    /// der 3D-Ansicht.
    fn neu(s: &mut Scene, vorher: &mut (u64, u64)) -> (u64, u64) {
        s.mesh(ViewKind::Persp, None, &[]);
        let jetzt = (s.ausbau_geprueft(), s.ausbau_builds());
        let d = (jetzt.0 - vorher.0, jetzt.1 - vorher.1);
        *vorher = jetzt;
        d
    }

    /// Review #112 B1: Ein Wandschritt rechnet nur die Räume seines Hauses
    /// und baut nur die Körper der Räume, die er ändert; ein neues Haus nur
    /// seine Räume; ein Netz ohne Änderung nichts.
    #[test]
    fn nur_die_raeume_des_geaenderten_hauses() {
        let mut s = Scene::new();
        let innen = haeuser(&mut s, 0, 2);
        s.set_view_finish(2, true);
        let st = s.active_storey();
        assert_eq!(s.model().spaces(st).len(), 6);
        let mut z = (0, 0);
        assert_eq!(neu(&mut s, &mut z), (8, 8));
        assert_eq!(neu(&mut s, &mut z), (0, 0), "Netz ohne Änderung");
        // Erste Innenwand von Haus A schrittweise nach links
        let iw = innen[0];
        let orig = s.model().run(iw).unwrap().points.clone();
        for i in 1..=7 {
            let d = vec3(-100.0 - 10.0 * f64::from(i), 0.0, 0.0);
            let pts: Vec<_> = orig.iter().map(|p| *p + d).collect();
            s.begin("Wand verschieben");
            s.set_run_points(iw, &pts);
            s.commit();
            assert_eq!(
                neu(&mut s, &mut z),
                (3, 2),
                "Haus A, zwei Räume an der Wand ({i})"
            );
        }
        // Ein drittes Haus: nur seine Räume
        haeuser(&mut s, 2, 3);
        assert_eq!(s.model().spaces(st).len(), 9);
        assert_eq!(neu(&mut s, &mut z), (4, 4));
    }

    /// Eine andere Nutzung baut nur diesen Raum neu; bringt sie neue Typen
    /// ins Projekt oder ändert sich ein Typ, prüft jeder Raum, gebaut wird
    /// nur, wen es trifft.
    #[test]
    fn nutzung_und_typ() {
        let mut s = Scene::new();
        haeuser(&mut s, 0, 2);
        s.set_view_finish(2, true);
        let st = s.active_storey();
        let mut z = (0, 0);
        neu(&mut s, &mut z);
        let raeume = s.model().spaces(st).to_vec();
        let bad = BATH_SPACE_GUID;
        // Wie die Wahl am Raumstempel: ein Schritt ohne Neubau der Züge
        let nutzung = |s: &mut Scene, r: ElementId| {
            s.begin("Nutzung");
            assert!(s.model.set_space(r, |x| x.nutzung = Some(bad)));
            assert!(sk_raum::ausbau::typen_anlegen(&mut s.model, r));
            s.commit();
        };
        // Das erste Bad bringt Fliesen und ihre Typen
        nutzung(&mut s, raeume[0]);
        assert_eq!(neu(&mut s, &mut z), (8, 1));
        // Das zweite nur noch sich, geprüft werden die Räume seines Hauses
        let b = *raeume
            .iter()
            .find(|&&r| !same_haus(&s, raeume[0], r))
            .unwrap();
        nutzung(&mut s, b);
        assert_eq!(neu(&mut s, &mut z), (3, 1));
        // Wandfliesen dicker: nur die zwei Bäder
        let m = s.model();
        let id = SPACE_TEMPLATES
            .iter()
            .find(|t| t.guid == bad)
            .and_then(|t| m.type_by_guid(t.tiles?.0))
            .unwrap();
        let mut t = m.layer_set(id).unwrap().clone();
        t.layers[0].thickness += 5.0;
        assert!(s.edit_types("Typ", |m| m.set_layer_set(id, t)));
        assert_eq!(neu(&mut s, &mut z), (8, 2));
    }

    /// Liegen die Anker der Räume `a` und `b` im selben Haus?
    fn same_haus(s: &Scene, a: ElementId, b: ElementId) -> bool {
        let x = |r| (s.model().space(r).unwrap().anker.x / 15000.0).floor();
        x(a) == x(b)
    }
}
