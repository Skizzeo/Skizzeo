//! Ausbau innen in der Szene (Raumbuch RB4, `raumbuch/plan.md` §8): die
//! Körper des Ausbaus je Raum ([`sk_raum::raumbuch::koerper`]). Den Estrich
//! zeigen Schnitt und 3D immer; Innenputz, Wandbeläge, Deckenputz,
//! Bodenbelag und Sockelleisten nur, wo der Schalter „Ausbau innen“ der
//! Ansicht an ist ([`sk_model::FINISH_VIEWS`]). Der Grundriss zeigt ohne
//! Schalter nichts davon, mit ihm die Räume des aktiven Geschosses in
//! seiner Schnitthöhe.
//!
//! Zwischenspeicher wie bei den Erweiterungen ([`crate::ext_cache`]): je
//! Raum Körper, Grundrisse und Schnitte. Der Schlüssel ist alles, was die
//! Körper bestimmt (Rohbau, Ausbau, die Ausbautypen); neu gerechnet wird
//! nur ein Raum, dessen Schlüssel sich ändert, und geprüft nur einmal je
//! Modellstand, nur wenn eine Ansicht Körper braucht. Ein Raum ohne
//! gespeicherten Raum (Datei vor G3, vor dem ersten Wandschritt) zeigt
//! keinen Ausbau.

use super::{class_in, mesh_into, Class, Plane, Scene, Vis};
use crate::scene::MeshData;
use crate::ui::ViewKind;
use sk_math::vec3;
use sk_model::{ElementId, LayerSet, LevelKind, Model, Solid, StoreyId};
use sk_raum::ausbau::RaumAusbau;
use sk_raum::oeffnungen::Rohbau;
use sk_raum::raumbuch::koerper::{koerper, Koerper};
use sk_raum::raumbuch::Flaeche;
use std::collections::HashMap;

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

#[derive(Default)]
pub(crate) struct RaumCache {
    rev: Option<u64>,
    items: Vec<Eintrag>,
    /// Wie oft die Körper eines Raums gerechnet wurden (Tests).
    pub builds: u64,
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

impl RaumCache {
    /// Gleicht den Speicher mit dem Modell ab (einmal je Modellstand).
    fn sync(&mut self, m: &Model) {
        if self.rev == Some(m.revision()) {
            return;
        }
        self.rev = Some(m.revision());
        let mut alt: HashMap<ElementId, Eintrag> = std::mem::take(&mut self.items)
            .into_iter()
            .map(|x| (x.id, x))
            .collect();
        let geschosse: Vec<StoreyId> = m
            .storeys()
            .iter()
            .filter(|(_, s)| s.kind == LevelKind::Storey)
            .map(|(id, _)| id)
            .collect();
        for g in geschosse {
            for f in sk_raum::raeume(m, g) {
                let Some(id) = f.raum else {
                    continue;
                };
                let roh = Rohbau::neu(m, f);
                let Some(aus) = RaumAusbau::fuer(m, &roh) else {
                    continue;
                };
                let s = Schluessel {
                    typen: typen(m, &aus),
                    roh,
                    aus,
                };
                if let Some(x) = alt.remove(&id) {
                    if x.schluessel == s && x.storey == g {
                        self.items.push(x);
                        continue;
                    }
                }
                let k = koerper(m, &s.roh, &s.aus);
                self.builds += 1;
                self.items.push(Eintrag {
                    id,
                    storey: g,
                    voll: paar(&k, |x| x.solid.clone()),
                    koerper: k,
                    schluessel: s,
                    plan: Vec::new(),
                    section: Vec::new(),
                });
            }
        }
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
        for x in &mut self.items {
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
        for x in &self.items {
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
        self.ausbau.sync(&self.model);
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
}

#[cfg(test)]
mod tests {
    use crate::scene::{MeshData, Scene};
    use crate::ui::ViewKind;
    use sk_math::vec3;
    use sk_model::model::{SCREED_SLAB_TYPE_GUID, WOODCHIP_WALL_TYPE_GUID};
    use sk_model::{material, material_key, ElementId, Guid, Model};

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
}
