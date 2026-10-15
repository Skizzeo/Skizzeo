//! Neutrale Raumliste für den Austausch (IFC I6, `raumbuch/plan.md` §9): je
//! gespeichertem Raum Nummer, Name, Nutzung, Umriss roh und fertig, Höhen,
//! die Mengen für `Qto_SpaceBaseQuantities`, der geschlossene Raumkörper von
//! OK Fertigfußboden bis UK Fertigdecke und je Ausbaufläche der Typ mit
//! seinen Schichten (Menge nach VOB/C und Körper) und das bekleidete
//! Bauteil. Ansicht und Export nutzen dieselben Körper
//! ([`crate::raumbuch::koerper`]).

use crate::ausbau::{self, RaumAusbau};
use crate::erkennung::{Begrenzung, Kante, RaumForm};
use crate::mengen::kostengruppe;
use crate::oeffnungen::{Geschosse, Rohbau};
use crate::raumbuch::koerper::{koerper, raumkoerper};
use crate::raumbuch::{rechnen, Flaeche, Regeln};
use crate::{Erkennung, Ketten};
use sk_math::Vec3;
use sk_model::{
    BuildingId, ElementId, ElementKind, Guid, LayerSetId, LevelKind, Model, RunId, Solid,
    SpaceGroup, StoreyId,
};
use std::collections::HashMap;

/// Umriss gegen den Uhrzeigersinn und die Löcher im Uhrzeigersinn
/// (Grundrissebene).
pub type Umriss = (Vec<Vec3>, Vec<Vec<Vec3>>);

/// Ein Raum für den Austausch.
#[derive(Clone, Debug)]
pub struct RaumDaten {
    pub raum: ElementId,
    pub guid: Guid,
    /// Nummer (IFC `Name`) und Raumname (`LongName`).
    pub nummer: String,
    pub name: String,
    /// Raumvorlage der Nutzung und ihre Nutzungsgruppe.
    pub nutzung: Guid,
    pub gruppe: Option<SpaceGroup>,
    pub geschoss: StoreyId,
    /// Rohbau und fertig (Putzoberfläche, mit Wandfliesen am Boden);
    /// fertig `None`, wenn der Raum dabei zerfällt.
    pub roh: Umriss,
    pub fertig: Option<Umriss>,
    /// OK Rohdecke und UK Rohdecke darüber (mm, absolut).
    pub z_roh: (f64, f64),
    /// OK Fertigfußboden und UK Fertigdecke (mm, absolut).
    pub z_fertig: (f64, f64),
    pub mengen: RaumMengen,
    /// Raumkörper im Fertigumriss von OK Fertigfußboden bis UK Fertigdecke;
    /// leer, wenn der Raum zerfällt.
    pub koerper: Solid,
    pub ausbau: Vec<AusbauDaten>,
    /// Rohbaukanten in der fortlaufenden Zählung von `Rohbau::kanten`, auf
    /// die `Flaeche::Wand` zeigt (stabile Kennung der Wandseiten im IFC).
    pub kanten: Vec<Kante>,
}

/// Mengen für `Qto_SpaceBaseQuantities` (mm, mm², mm³): roh (Gross) und
/// fertig (Net).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct RaumMengen {
    /// `Height`: OK Rohdecke bis UK Rohdecke.
    pub hoehe_roh: f64,
    /// `FinishCeilingHeight`: OK Fertigfußboden bis UK Fertigdecke.
    pub hoehe_fertig: f64,
    /// `FinishFloorHeight`: Fußbodenaufbau über der Rohdecke.
    pub fussboden: f64,
    pub umfang_roh: f64,
    pub umfang_fertig: f64,
    pub flaeche_roh: f64,
    pub flaeche_fertig: f64,
    pub volumen_roh: f64,
    pub volumen_fertig: f64,
}

/// Eine Ausbaufläche: ein Ausbautyp an einer Fläche des Raums.
#[derive(Clone, Debug)]
pub struct AusbauDaten {
    pub flaeche: Flaeche,
    pub typ: LayerSetId,
    pub typ_guid: Guid,
    pub schichten: Vec<SchichtDaten>,
    /// Bekleidetes Bauteil: die Wand der Seite, die Decke darüber, die
    /// Decke bzw. Sohlplatte darunter; `None` bei der Sockelleiste.
    pub traeger: Option<Guid>,
}

/// Eine Schicht einer Ausbaufläche.
#[derive(Clone, Debug)]
pub struct SchichtDaten {
    pub schicht: usize,
    /// Wirksame Dicke (mm), im Estrich mit Ausgleich.
    pub dicke: f64,
    /// Menge nach VOB/C aus dem Raumbuch (mm², bei der Sockelleiste mm).
    pub menge: f64,
    /// Kostengruppe nach DIN 276 wie im Mengenblatt ([`crate::mengen`]).
    pub kg: u16,
    /// Körper mit Baustoffstempel; `None`, wo das Raumbuch keinen baut
    /// (Laibungen als eigene Fläche, Schichten ohne Dicke, Fliesen an
    /// Laibungen).
    pub koerper: Option<Solid>,
}

/// Alle gespeicherten Räume der Geschosse mit ihrem Ausbau; Räume ohne
/// Ausbau (kein Raumkatalog) fehlen.
pub fn raumliste(model: &Model) -> Vec<RaumDaten> {
    raumliste_mit_formen(model).0
}

/// Wie [`raumliste`], dazu die Ids aller gespeicherten Räume mit Form, also
/// auch derer ohne Ausbau, die in der Liste fehlen.
pub fn raumliste_mit_formen(model: &Model) -> (Vec<RaumDaten>, Vec<ElementId>) {
    let geschosse: Vec<StoreyId> = model
        .storeys()
        .iter()
        .filter(|(_, s)| s.kind == LevelKind::Storey)
        .map(|(id, _)| id)
        .collect();
    // Platten und Höhen der Geschosse einmal für alle Räume (Review #137)
    let platten = Platten::neu(model, model);
    let mut out = Vec::new();
    let mut formen = Vec::new();
    // Alle Geschosse in einem Durchgang (Review #138 H-138f)
    for f in Erkennung::raeume_alle(model, &geschosse)
        .into_iter()
        .flatten()
    {
        formen.extend(f.raum);
        out.extend(raum(model, &platten, f));
    }
    (out, formen)
}

/// Ein Raum, `None` ohne gespeicherten Raum oder ohne Ausbau.
fn raum(model: &Model, platten: &Platten, f: RaumForm) -> Option<RaumDaten> {
    let id = f.raum?;
    let e = model.element(id)?;
    let s = model.space(id)?;
    let lagen = &platten.geschosse;
    let roh = Rohbau::mit_geschossen(model, model, f, lagen);
    let aus = RaumAusbau::fuer_in(model, &roh, lagen)?;
    let nutzung = ausbau::nutzung(model, &roh.form);
    let gruppe = ausbau::vorlage(model, nutzung).map(|v| v.group);
    let ringe: Vec<&[Kante]> = roh.ringe().collect();
    let punkte = |r: &[Kante]| r.iter().map(|k| k.a).collect::<Vec<_>>();
    let (rand, loecher) = ringe.split_first()?;
    let umriss_roh: Umriss = (punkte(rand), loecher.iter().map(|r| punkte(r)).collect());
    let (fertig, mengen, (z_ffb, z_fd)) = fertigmass(model, &roh, &aus);
    let koerper_raum = fertig
        .as_ref()
        .map_or_else(Solid::default, |u| raumkoerper(&u.0, &u.1, (z_ffb, z_fd)));
    Some(RaumDaten {
        raum: id,
        guid: e.guid,
        nummer: e.number.clone(),
        name: s.name.clone(),
        nutzung,
        gruppe,
        geschoss: roh.form.geschoss,
        ausbau: ausbauflaechen(model, platten, &roh, &aus),
        kanten: roh.kanten().cloned().collect(),
        roh: umriss_roh,
        fertig,
        z_roh: (roh.z_boden, roh.z_decke),
        z_fertig: (z_ffb, z_fd),
        mengen,
        koerper: koerper_raum,
    })
}

/// Fertigumriss, Mengen roh und fertig und OK Fertigfußboden bzw. UK
/// Fertigdecke (mm, absolut) eines Raums mit Ausbau.
pub(crate) fn fertigmass(
    model: &Model,
    roh: &Rohbau,
    aus: &RaumAusbau,
) -> (Option<Umriss>, RaumMengen, (f64, f64)) {
    let ringe: Vec<&[Kante]> = roh.ringe().collect();
    let fertig = fertigumriss(&ringe, &ausbau::dicken(model, roh, aus));
    let (z_ffb, z_fd) = ausbau::hoehen(model, roh, aus);
    let flaeche = |u: &Umriss| {
        sk_math::polygon::area(&u.0) - u.1.iter().map(|l| sk_math::polygon::area(l)).sum::<f64>()
    };
    let umfang = |u: &Umriss| {
        sk_math::polygon::perimeter(&u.0)
            + u.1
                .iter()
                .map(|l| sk_math::polygon::perimeter(l))
                .sum::<f64>()
    };
    let hoehe_roh = roh.z_decke - roh.z_boden;
    let hoehe_fertig = (z_fd - z_ffb).max(0.0);
    let (flaeche_fertig, umfang_fertig) = fertig
        .as_ref()
        .map_or((0.0, 0.0), |u| (flaeche(u), umfang(u)));
    let mengen = RaumMengen {
        hoehe_roh,
        hoehe_fertig,
        fussboden: z_ffb - roh.z_boden,
        umfang_roh: roh.form.umfang,
        umfang_fertig,
        flaeche_roh: roh.form.flaeche,
        flaeche_fertig,
        volumen_roh: roh.form.flaeche * hoehe_roh,
        volumen_fertig: flaeche_fertig * hoehe_fertig,
    };
    (fertig, mengen, (z_ffb, z_fd))
}

/// Fertigumriss: jeder Ring je Kante um `dicken` (fortlaufend gezählt)
/// nach innen versetzt; `None`, wenn ein Ring dabei zerfällt.
pub(crate) fn fertigumriss(ringe: &[&[Kante]], dicken: &[f64]) -> Option<Umriss> {
    let mut i = 0;
    let mut out = Vec::new();
    for r in ringe {
        let p: Vec<Vec3> = r.iter().map(|k| k.a).collect();
        let d = dicken.get(i..i + r.len())?;
        i += r.len();
        out.push(sk_math::flaeche::versatz(&p, d)?);
    }
    let rand = out.remove(0);
    Some((rand, out))
}

/// Ausbauflächen nach Fläche und Typ, in der Reihenfolge der Zeilen des
/// Raumbuchs; Körper ohne Zeile kommen dazu.
fn ausbauflaechen(
    model: &Model,
    platten: &Platten,
    roh: &Rohbau,
    aus: &RaumAusbau,
) -> Vec<AusbauDaten> {
    let kanten: Vec<&Kante> = roh.kanten().collect();
    let unten = traeger(model, platten, roh, &kanten, Flaeche::Boden)
        .and_then(|id| model.element(id))
        .map(|e| e.category);
    let kg = |flaeche: Flaeche, typ: LayerSetId, schicht: usize| {
        let l = model
            .layer_set(typ)
            .and_then(|t| t.layers.get(schicht).copied());
        kostengruppe(model, unten, &kanten, flaeche, l)
    };
    let mut out: Vec<AusbauDaten> = Vec::new();
    let eintrag = |out: &mut Vec<AusbauDaten>, flaeche: Flaeche, typ: LayerSetId| -> usize {
        if let Some(i) = out
            .iter()
            .position(|a| a.flaeche == flaeche && a.typ == typ)
        {
            return i;
        }
        out.push(AusbauDaten {
            flaeche,
            typ,
            typ_guid: model.layer_set(typ).map_or(Guid(0), |t| t.guid),
            schichten: Vec::new(),
            traeger: traeger(model, platten, roh, &kanten, flaeche)
                .and_then(|id| model.element(id))
                .map(|e| e.guid),
        });
        out.len() - 1
    };
    for z in rechnen(model, roh, aus, &Regeln::default()) {
        let Some((typ, schicht)) = z.typ else {
            continue;
        };
        let i = eintrag(&mut out, z.flaeche, typ);
        let s = &mut out[i].schichten;
        match s.iter_mut().find(|x| x.schicht == schicht) {
            Some(x) => x.menge += z.menge(),
            None => s.push(SchichtDaten {
                schicht,
                dicke: z.dicke,
                menge: z.menge(),
                kg: kg(z.flaeche, typ, schicht),
                koerper: None,
            }),
        }
    }
    for k in koerper(model, roh, aus) {
        let i = eintrag(&mut out, k.flaeche, k.typ);
        let s = &mut out[i].schichten;
        let dicke = model
            .layer_set(k.typ)
            .and_then(|t| t.layers.get(k.schicht))
            .map_or(0.0, |l| l.thickness);
        match s.iter_mut().find(|x| x.schicht == k.schicht) {
            Some(x) => match &mut x.koerper {
                Some(solid) => solid.append(&k.solid),
                None => x.koerper = Some(k.solid),
            },
            None => s.push(SchichtDaten {
                schicht: k.schicht,
                dicke,
                menge: 0.0,
                kg: kg(k.flaeche, k.typ, k.schicht),
                koerper: Some(k.solid),
            }),
        }
    }
    for a in &mut out {
        a.schichten.sort_by_key(|s| s.schicht);
    }
    out
}

/// Bekleidetes Bauteil einer Fläche.
pub(crate) fn traeger(
    model: &Model,
    platten: &Platten,
    roh: &Rohbau,
    kanten: &[&Kante],
    f: Flaeche,
) -> Option<ElementId> {
    let wand = |k: usize| {
        let Begrenzung::Wand { wand, .. } = kanten.get(k)?.quelle;
        Some(wand)
    };
    match f {
        Flaeche::Wand(k)
        | Flaeche::Laibung(k)
        | Flaeche::Fliesen(k)
        | Flaeche::FliesenLaibung(k) => wand(k),
        Flaeche::Decke => platten.decke(&roh.form, roh.form.geschoss),
        Flaeche::Boden | Flaeche::Estrich => boden(model, platten, &roh.form),
        _ => None,
    }
}

/// Decke oder Sohlplatte mit dem Umriss ihres Zugs (Bezugslinie) und dem
/// Rechteck darum.
#[derive(Clone)]
struct Platte {
    id: ElementId,
    umriss: Vec<Vec3>,
    kasten: [f64; 4],
}

impl Platte {
    /// Liegt der Raum über der Platte?
    fn traegt(&self, form: &RaumForm) -> bool {
        let (p, [x0, y0, x1, y1]) = (form.mitte, self.kasten);
        p.x >= x0
            && p.x <= x1
            && p.y >= y0
            && p.y <= y1
            && sk_math::flaeche::enthaelt(&self.umriss, p)
    }
}

/// Decken je Geschoss und Sohlplatten je Gebäude, einmal je Liste
/// gesammelt (Review #112 H8, #138 B1): Vorher ging jede Bodenzeile alle
/// Bauteile durch und rechnete je Decke die Kette ihres Zugs, quadratisch
/// mit dem Modell. Die Kette eines Zugs rechnet es höchstens einmal. Dazu
/// die Höhen der Geschosse, ebenfalls einmal je Liste (Review #137 B2).
pub struct Platten {
    decken: HashMap<StoreyId, Vec<Platte>>,
    sohlen: HashMap<Option<BuildingId>, Vec<Platte>>,
    pub(crate) geschosse: Geschosse,
}

impl Platten {
    /// Die Platten des Modells, die Ketten ihrer Züge aus `k` (in der App
    /// die berechneten der Szene).
    pub fn neu(model: &Model, k: &(impl Ketten + ?Sized)) -> Platten {
        let mut p = Platten {
            decken: HashMap::new(),
            sohlen: HashMap::new(),
            geschosse: Geschosse::aus(model),
        };
        // Umriss und Rechteck je Zug, die Kette einmal
        let mut zuege: HashMap<RunId, Option<Platte>> = HashMap::new();
        for (id, e) in model.elements().iter() {
            let (run, sohle) = match e.kind {
                ElementKind::Floor(ref d) => (d.run, false),
                ElementKind::GroundSlab(ref s) => (s.run, true),
                _ => continue,
            };
            let zug = zuege.entry(run).or_insert_with(|| {
                let umriss = k.kette(run)?.clean_points();
                let mut kasten = [f64::MAX, f64::MAX, f64::MIN, f64::MIN];
                for q in &umriss {
                    kasten = [
                        kasten[0].min(q.x),
                        kasten[1].min(q.y),
                        kasten[2].max(q.x),
                        kasten[3].max(q.y),
                    ];
                }
                Some(Platte { id, umriss, kasten })
            });
            let Some(platte) = zug.clone().map(|z| Platte { id, ..z }) else {
                continue;
            };
            if sohle {
                let b = model.building_of(e.storey);
                p.sohlen.entry(b).or_default().push(platte);
            } else {
                p.decken.entry(e.storey).or_default().push(platte);
            }
        }
        p
    }

    /// Die Decke über dem Geschoss `g` an der Stelle des Raums.
    fn decke(&self, form: &RaumForm, g: StoreyId) -> Option<ElementId> {
        let d = self.decken.get(&g)?;
        d.iter().find(|p| p.traegt(form)).map(|p| p.id)
    }
}

/// Unter dem Raum: die Decke des Geschosses darunter, im untersten
/// Geschoss die Sohlplatte.
fn boden(model: &Model, platten: &Platten, form: &RaumForm) -> Option<ElementId> {
    // Ebenen je Gebäude aus der Tafel der Liste (Review #138 H-138f)
    let lagen = &platten.geschosse;
    model.storey(form.geschoss)?;
    let geschoss = lagen
        .darunter(form.geschoss)
        .filter(|d| d.1 == LevelKind::Storey)
        .map(|d| d.0);
    match geschoss {
        Some(g) => platten.decke(form, g),
        None => {
            let sohlen = platten.sohlen.get(&model.building_of(form.geschoss))?;
            sohlen.iter().find(|p| p.traegt(form)).map(|p| p.id)
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use sk_math::vec3;

    /// Haus 10 × 8 m mit zwei Geschossen, je ein Raum mit Ausbau.
    pub(crate) fn haus() -> Model {
        let mut m = haus_roh();
        let geschosse: Vec<StoreyId> = m
            .levels_in(m.buildings().iter().next().map(|b| b.0))
            .into_iter()
            .filter(|g| m.storey(*g).unwrap().kind == LevelKind::Storey)
            .collect();
        for g in geschosse {
            let id = m.spaces(g)[0];
            m.begin("Ausbau");
            assert!(ausbau::typen_anlegen(&mut m, id));
            m.commit();
        }
        m
    }

    /// Wie [`haus`], die Räume ohne Ausbautypen im Projekt.
    pub(crate) fn haus_roh() -> Model {
        let mut m = Model::with_seed(9);
        let b = m.add_building(2);
        let pts = [
            vec3(0.0, 0.0, 0.0),
            vec3(0.0, 8000.0, 0.0),
            vec3(10000.0, 8000.0, 0.0),
            vec3(10000.0, 0.0, 0.0),
        ];
        m.build_from_polygon(b, &pts).unwrap();
        let geschosse: Vec<StoreyId> = m
            .levels_in(Some(b))
            .into_iter()
            .filter(|g| m.storey(*g).unwrap().kind == LevelKind::Storey)
            .collect();
        m.begin("Räume");
        assert!(crate::abgleichen(&mut m, &geschosse) > 0);
        m.commit();
        m
    }

    /// Je Raum Mengen roh und fertig, Raumkörper mit dem Nettovolumen,
    /// Ausbau mit Körpern und Trägern: Wand, Decke darüber, Sohlplatte bzw.
    /// Decke darunter.
    #[test]
    fn raeume_fuer_den_austausch() {
        let m = haus();
        let (l, formen) = raumliste_mit_formen(&m);
        assert_eq!(l.len(), 2);
        assert_eq!(formen, l.iter().map(|r| r.raum).collect::<Vec<_>>());
        let (eg, og) = (&l[0], &l[1]);
        assert!(eg.z_roh.0 < og.z_roh.0);
        for r in &l {
            let q = &r.mengen;
            assert_eq!(q.fussboden, 160.0, "Fußbodenaufbau");
            assert!(
                (q.hoehe_fertig - (q.hoehe_roh - 160.0 - 10.0)).abs() < 1e-9,
                "Deckenputz 10"
            );
            assert!(q.flaeche_fertig < q.flaeche_roh && q.umfang_fertig < q.umfang_roh);
            // Fertigmaß: 15 mm Putz ringsum, Rechteck
            let (a, b) = rechteck(&r.roh.0);
            assert!((q.flaeche_fertig - (a - 30.0) * (b - 30.0)).abs() < 1e-3);
            assert!((q.umfang_fertig - 2.0 * (a + b - 60.0)).abs() < 1e-6);
            assert!((q.volumen_fertig - q.flaeche_fertig * q.hoehe_fertig).abs() < 1e-3);
            let v = volumen(&r.koerper);
            assert!(
                (v - q.volumen_fertig).abs() < q.volumen_fertig * 1e-9,
                "{v} {}",
                q.volumen_fertig
            );
            assert!(r.gruppe.is_some());
            // Ausbau: jede Fläche mit Typ, Träger je nach Fläche
            let wand = r
                .ausbau
                .iter()
                .find(|a| matches!(a.flaeche, Flaeche::Wand(_)))
                .unwrap();
            assert!(wand.traeger.is_some());
            // Wandflächen zeigen in die Rohbaukanten, und die Kante nennt
            // die Wand, die die Fläche trägt
            for a in &r.ausbau {
                if let Flaeche::Wand(k) = a.flaeche {
                    let Begrenzung::Wand { wand, .. } = r.kanten[k].quelle;
                    assert_eq!(a.traeger, m.element(wand).map(|e| e.guid));
                }
            }
            assert!(wand
                .schichten
                .iter()
                .any(|s| s.koerper.is_some() && s.menge > 0.0));
            let decke = r
                .ausbau
                .iter()
                .find(|a| a.flaeche == Flaeche::Decke)
                .unwrap();
            let boden = r
                .ausbau
                .iter()
                .find(|a| a.flaeche == Flaeche::Estrich)
                .unwrap();
            assert!(decke.traeger.is_some() && boden.traeger.is_some());
            assert_ne!(decke.traeger, boden.traeger);
            let sockel = r
                .ausbau
                .iter()
                .find(|a| a.flaeche == Flaeche::Sockel)
                .unwrap();
            assert_eq!(sockel.traeger, None);
        }
        // Die Decke über dem EG ist der Boden des OG; unter dem EG die Sohlplatte
        let traeger =
            |r: &RaumDaten, f: Flaeche| r.ausbau.iter().find(|a| a.flaeche == f).unwrap().traeger;
        assert_eq!(traeger(eg, Flaeche::Decke), traeger(og, Flaeche::Estrich));
        let platte = traeger(eg, Flaeche::Estrich).unwrap();
        let e = m
            .elements()
            .iter()
            .find(|(_, e)| e.guid == platte)
            .unwrap()
            .1;
        assert!(matches!(e.kind, ElementKind::GroundSlab(_)));
    }

    /// Rauminhalt eines geschlossenen Körpers (Divergenzsatz).
    fn volumen(s: &Solid) -> f64 {
        s.triangles
            .iter()
            .map(|t| t.p[0].dot(t.p[1].cross(t.p[2])) / 6.0)
            .sum()
    }

    /// Seitenlängen eines achsparallelen Rechtecks.
    fn rechteck(p: &[Vec3]) -> (f64, f64) {
        let x = p.iter().map(|p| p.x);
        let y = p.iter().map(|p| p.y);
        (
            x.clone().fold(f64::MIN, f64::max) - x.fold(f64::MAX, f64::min),
            y.clone().fold(f64::MIN, f64::max) - y.fold(f64::MAX, f64::min),
        )
    }
}
