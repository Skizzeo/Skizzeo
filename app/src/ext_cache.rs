//! Zwischenspeicher der Erweiterungsbauteile (Schrittplan E4, Review Nr. 13):
//! je Exemplar Körper, Grundriss und Schnitt. Der Schlüssel ist alles, was
//! die Geometrie bestimmt (Exemplar, Definition, Geschoss, Baustoffe); neu
//! gerechnet wird nur ein Exemplar, dessen Schlüssel sich ändert, und nur
//! einmal je Modellstand geprüft, nie je Bild oder Mausbewegung.

use sk_math::{vec3, Vec3};
use sk_model::erweiterung::{Ergebnis, ExtPart};
use sk_model::erweiterung_koerper::{self as koerper, Lage};
use sk_model::{ElementId, ElementKind, Model, Solid, StoreyId};

type Aabb = (Vec3, Vec3);
type Plane = (Vec3, Vec3);

/// Was die Geometrie eines Exemplars bestimmt.
#[derive(Clone, Debug, PartialEq)]
struct Schluessel {
    part: ExtPart,
    version: u32,
    text: u64,
    gh: f64,
    decke: f64,
    uk: f64,
    /// Darstellungsschlüssel je Baustoff der Definition.
    mats: Vec<u16>,
}

struct Eintrag {
    id: ElementId,
    storey: StoreyId,
    schluessel: Schluessel,
    lage: Lage,
    erg: Ergebnis,
    solid: Solid,
    bounds: Option<Aabb>,
    /// Grundrisse je Schnitthöhe und Schnitte je Ebene, zuletzt gefragte
    /// zuerst.
    plan: Vec<(f64, Solid)>,
    section: Vec<(Plane, Solid)>,
    /// Darstellungsschlüssel je Baustoffschlüssel.
    mat_of: Vec<(String, u16)>,
}

/// So viele Grundrisse und Schnitte behält ein Exemplar.
const KEEP: usize = 3;

#[derive(Default)]
pub struct ExtCache {
    rev: Option<u64>,
    items: Vec<Eintrag>,
    /// Wie oft ein Exemplar gerechnet wurde (Messung, Tests).
    pub builds: u64,
}

/// FNV-1a über den Text einer Definition.
fn hash(t: &str) -> u64 {
    t.bytes().fold(0xcbf29ce484222325, |h, b| {
        (h ^ b as u64).wrapping_mul(0x100000001b3)
    })
}

impl ExtCache {
    /// Gleicht den Speicher mit dem Modell ab (einmal je Modellstand).
    pub fn sync(&mut self, m: &Model) {
        if self.rev == Some(m.revision()) {
            return;
        }
        self.rev = Some(m.revision());
        let texte: Vec<(&str, u64)> = m
            .ext_defs()
            .iter()
            .map(|d| (d.key.as_str(), hash(&d.text)))
            .collect();
        let mut alt = std::mem::take(&mut self.items);
        for (id, e) in m.elements().iter() {
            let ElementKind::Ext(p) = &e.kind else {
                continue;
            };
            let Some(d) = m.ext_def(&p.key) else {
                continue;
            };
            let g = m.ext_geschoss(e.storey);
            let mat_of: Vec<(String, u16)> = d
                .def
                .koerper
                .iter()
                .filter_map(|k| k.get("baustoff"))
                .map(|b| {
                    let key = m.ext_material(d, b).map_or(0, sk_model::material_key);
                    (b.to_string(), key)
                })
                .collect();
            let s = Schluessel {
                part: p.clone(),
                version: d.version,
                text: texte.iter().find(|t| t.0 == d.key).map_or(0, |t| t.1),
                gh: g.gh,
                decke: g.decke,
                uk: m.storey(e.storey).map_or(0.0, |s| s.elevation),
                mats: mat_of.iter().map(|x| x.1).collect(),
            };
            if let Some(i) = alt
                .iter()
                .position(|x| x.id == id && x.schluessel == s && x.storey == e.storey)
            {
                self.items.push(alt.swap_remove(i));
                continue;
            }
            let Some((lage, erg)) = m.ext_lage(id) else {
                continue;
            };
            let (erg, _) = koerper::begrenzt(erg);
            let mat = |b: &str| mat_of.iter().find(|x| x.0 == b).map_or(0, |x| x.1);
            let solid = koerper::solid(&erg, &lage, &mat);
            self.builds += 1;
            self.items.push(Eintrag {
                id,
                storey: e.storey,
                schluessel: s,
                lage,
                bounds: solid.bounds(),
                solid,
                erg,
                plan: Vec::new(),
                section: Vec::new(),
                mat_of,
            });
        }
    }

    /// Umschließender Quader aller Exemplare.
    pub fn bounds(&self) -> Option<Aabb> {
        self.items.iter().filter_map(|x| x.bounds).reduce(|a, b| {
            (
                vec3(a.0.x.min(b.0.x), a.0.y.min(b.0.y), a.0.z.min(b.0.z)),
                vec3(a.1.x.max(b.1.x), a.1.y.max(b.1.y), a.1.z.max(b.1.z)),
            )
        })
    }

    /// Körper je Exemplar für eine Ansicht. Grundriss: nur Exemplare des
    /// aktiven Geschosses, geschnitten in Schnitthöhe `cut`; Schnitt:
    /// geschnitten an der Ebene; sonst der ganze Körper.
    pub fn shown(
        &mut self,
        plan: Option<(StoreyId, f64)>,
        section: Option<Plane>,
    ) -> Vec<(ElementId, &Solid)> {
        for x in &mut self.items {
            let mat = |b: &str| x.mat_of.iter().find(|y| y.0 == b).map_or(0, |y| y.1);
            match (plan, section) {
                (Some((st, cut)), _) if x.storey == st => {
                    if let Some(i) = x.plan.iter().position(|p| p.0 == cut) {
                        let hit = x.plan.remove(i);
                        x.plan.insert(0, hit);
                    } else {
                        let p0 = vec3(0.0, 0.0, cut);
                        let s = koerper::geschnitten(
                            &x.solid,
                            &x.erg,
                            &x.lage,
                            &mat,
                            p0,
                            vec3(0.0, 0.0, 1.0),
                        );
                        x.plan.truncate(KEEP - 1);
                        x.plan.insert(0, (cut, s));
                    }
                }
                (None, Some(pl)) => {
                    if let Some(i) = x.section.iter().position(|p| p.0 == pl) {
                        let hit = x.section.remove(i);
                        x.section.insert(0, hit);
                    } else {
                        let s = koerper::geschnitten(&x.solid, &x.erg, &x.lage, &mat, pl.0, pl.1);
                        x.section.truncate(KEEP - 1);
                        x.section.insert(0, (pl, s));
                    }
                }
                _ => {}
            }
        }
        self.items
            .iter()
            .filter_map(|x| match (plan, section) {
                (Some((st, _)), _) if x.storey != st => None,
                (Some(_), _) => x.plan.first().map(|p| (x.id, &p.1)),
                (None, Some(_)) => x.section.first().map(|p| (x.id, &p.1)),
                (None, None) => Some((x.id, &x.solid)),
            })
            .collect()
    }

    /// Nächster Treffer des Strahls: Abstand und Exemplar. `keep` sagt, ob
    /// ein Exemplar wählbar ist (Sichtbarkeit).
    pub fn pick(
        &mut self,
        plan: Option<(StoreyId, f64)>,
        section: Option<Plane>,
        o: Vec3,
        d: Vec3,
        keep: impl Fn(ElementId) -> bool,
        hits_box: impl Fn(Aabb) -> bool,
    ) -> Option<(f64, ElementId)> {
        let boxes: Vec<(ElementId, Option<Aabb>)> =
            self.items.iter().map(|x| (x.id, x.bounds)).collect();
        let mut best: Option<(f64, ElementId)> = None;
        for (id, s) in self.shown(plan, section) {
            let b = boxes.iter().find(|x| x.0 == id).and_then(|x| x.1);
            if !b.is_some_and(&hits_box) || !keep(id) {
                continue;
            }
            if let Some(t) = s.raycast(o, d) {
                if best.is_none_or(|b| t < b.0) {
                    best = Some((t, id));
                }
            }
        }
        best
    }
}
