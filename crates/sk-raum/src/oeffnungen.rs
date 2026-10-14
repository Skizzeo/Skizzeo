//! Öffnungen und Rohbauhöhen eines Raums (Raumbuch RB3). Die Schnittstelle
//! (`grundriss/schnittstelle-raum.md` §3) sieht sie an der [`RaumForm`] vor;
//! Grundriss G3 rechnet nur die Form, deshalb leitet das Raumbuch sie hier
//! ab: die Öffnungen aus den Löchern der Wandzüge (O2), die Höhen aus dem
//! Geschoss. Kanten zählen fortlaufend über den Umriss und danach über die
//! Löcher in ihrer Reihenfolge ([`Rohbau::kanten`]).

use crate::erkennung::{Begrenzung, Kante, RaumForm, Seite};
use crate::speicher::Ketten;
use sk_model::oeffnung::FaceSide;
use sk_model::{BuildingId, ElementId, ElementKind, LevelKind, Model, RunId, StoreyId, WallChain};
use std::borrow::Cow;
use std::collections::HashMap;

/// Art der Öffnung für das Raumbuch. Das Modell kennt die Innentür als Tür
/// in einer Innenwand (Öffnungen #99), das Raumbuch rechnet sie anders als
/// die Außentür: Nische über die ganze Wanddicke, keine Laibung.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OpeningKind {
    /// Wandöffnung: Fenster, Terrassentür.
    Window,
    /// Türöffnung in der Außenwand: Haustür.
    Door,
    /// Tür in einer Innenwand.
    InnerDoor,
}

impl From<sk_model::oeffnung::OpeningKind> for OpeningKind {
    fn from(k: sk_model::oeffnung::OpeningKind) -> OpeningKind {
        match k {
            sk_model::oeffnung::OpeningKind::Window => OpeningKind::Window,
            sk_model::oeffnung::OpeningKind::Door => OpeningKind::Door,
        }
    }
}

/// Öffnung in einer Kante des Raums.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct OeffnungAmRaum {
    pub oeffnung: ElementId,
    /// Index der Kante, fortlaufend über Umriss und Löcher.
    pub kante: usize,
    /// Längs der Kante ab ihrem Anfang (mm), Rohbaumaß auf der Raumseite.
    pub von: f64,
    pub bis: f64,
    /// Unter- und Oberkante, absolut (mm), Rohbaumaß auf der Raumseite.
    pub uk: f64,
    pub ok: f64,
    /// Laibungstiefe zu diesem Raum im Rohbaumaß: Innentür Wanddicke,
    /// Fenster und Außentür die innere Einbautiefe (außen die äußere).
    pub tiefe: f64,
    pub art: OpeningKind,
    /// Die Tür schlägt in diesen Raum auf.
    pub aufschlag_hier: Option<bool>,
    /// Höhe der Öffnung (mm) in den lichten Höhen der Geschosse darunter und
    /// darüber. Eine geschossübergreifende Öffnung ist mit ihren Stücken dort
    /// eine Aussparung (`vob/oeffnung-vorgabe.md` §2).
    pub weiter: [f64; 2],
}

/// Ein Raum, wie das Raumbuch ihn braucht: die Form aus dem Grundriss, dazu
/// OK Rohdecke, UK Rohdecke darüber und die Öffnungen in seinen Kanten.
#[derive(Clone, Debug, PartialEq)]
pub struct Rohbau {
    pub form: RaumForm,
    /// OK Rohdecke bzw. OK Sohlplatte (mm, absolut).
    pub z_boden: f64,
    /// UK Rohdecke darüber (mm, absolut).
    pub z_decke: f64,
    pub oeffnungen: Vec<OeffnungAmRaum>,
}

/// Ein Geschoss, wie Rohbau und Ausbau eines Raums es brauchen.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Lage {
    gebaeude: Option<BuildingId>,
    art: LevelKind,
    /// Höhenlage, Höhe und lichte Höhe (mm).
    z: f64,
    hoehe: f64,
    licht: f64,
}

/// Die Geschosse des Modells mit Höhenlage, Höhe und lichter Höhe, aus
/// einem Gang über Geschosse und Bauteile. Wer viele Räume rechnet, hält
/// sie einmal je Stand; sonst ginge jeder Raum und jede Öffnung über alle
/// Bauteile, um die dickste Decke zu finden (Review #137 B2).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Geschosse {
    lagen: HashMap<StoreyId, Lage>,
    /// Je Gebäude seine Geschosse in der Reihenfolge des Modells.
    gebaeude: HashMap<Option<BuildingId>, Vec<StoreyId>>,
}

impl Geschosse {
    pub fn aus(model: &Model) -> Geschosse {
        // Dickste Decke an der OK jedes Geschosses, wie
        // `Model::clear_height`
        let mut decke: HashMap<StoreyId, f64> = HashMap::new();
        for (_, e) in model.elements().iter() {
            if let ElementKind::Floor(f) = &e.kind {
                let d = decke.entry(f.top.storey).or_insert(f.thickness);
                *d = d.max(f.thickness);
            }
        }
        let mut g = Geschosse::default();
        for (id, s) in model.storeys().iter() {
            let d = decke.get(&id).copied().unwrap_or(sk_model::FLOOR_THICKNESS);
            g.lagen.insert(
                id,
                Lage {
                    gebaeude: s.building,
                    art: s.kind,
                    z: s.elevation,
                    hoehe: s.height,
                    licht: s.height - d,
                },
            );
            g.gebaeude.entry(s.building).or_default().push(id);
        }
        g
    }

    /// Das Geschoss unter `g` in seinem Gebäude, nach Höhenlage geordnet wie
    /// `Model::levels_in`, ohne Gang über alle Geschosse des Modells (Review
    /// #138 H-138f), mit seiner Art.
    pub fn darunter(&self, g: StoreyId) -> Option<(StoreyId, LevelKind)> {
        let l = self.lagen.get(&g)?;
        let mut v: Vec<(StoreyId, f64)> = self
            .gebaeude
            .get(&l.gebaeude)?
            .iter()
            .filter_map(|s| Some((*s, self.lagen.get(s)?.z)))
            .collect();
        v.sort_by(|a, b| a.1.total_cmp(&b.1));
        let i = v.iter().position(|x| x.0 == g)?;
        let d = v[i.checked_sub(1)?].0;
        Some((d, self.lagen.get(&d)?.art))
    }

    /// Lichte Höhe des Geschosses (mm) wie `Model::clear_height`, 0 ohne
    /// das Geschoss.
    pub fn licht(&self, g: StoreyId) -> f64 {
        self.lagen.get(&g).map_or(0.0, |l| l.licht)
    }

    /// OK Rohdecke und UK Rohdecke darüber (mm, absolut) wie
    /// [`hoehen_roh`].
    fn roh(&self, g: StoreyId) -> (f64, f64) {
        let z = self.lagen.get(&g).map_or(0.0, |l| l.z);
        (z, z + self.licht(g))
    }

    /// Liegt das Geschoss direkt über der Gründung seines Gebäudes, auf der
    /// Sohlplatte ([`crate::ausbau::estrich_vorgabe`])?
    pub fn auf_sohle(&self, g: StoreyId) -> bool {
        let Some(s) = self.lagen.get(&g) else {
            return false;
        };
        self.im_gebaeude(s.gebaeude).any(|(_, l)| auf_sohle(l, s))
    }

    /// Die Geschosse des Gebäudes `b`.
    fn im_gebaeude(&self, b: Option<BuildingId>) -> impl Iterator<Item = (StoreyId, &Lage)> {
        let ids = self.gebaeude.get(&b).map_or(&[][..], Vec::as_slice);
        ids.iter().filter_map(|id| Some((*id, self.lagen.get(id)?)))
    }
}

/// Ist `g` die Gründung, auf deren Oberkante das Geschoss `s` steht?
fn auf_sohle(g: &Lage, s: &Lage) -> bool {
    g.art == LevelKind::Foundation && g.gebaeude == s.gebaeude && (g.z + g.hoehe - s.z).abs() < 1.0
}

impl Rohbau {
    /// Rohbau des Raums, die Wandzüge aus dem Modell.
    pub fn neu(model: &Model, form: RaumForm) -> Rohbau {
        Rohbau::mit(model, model, form)
    }

    /// Wie [`Rohbau::neu`], die Wandzüge aus `k`: in der App die berechneten
    /// der Szene, damit das Paneel nicht je Zug die Kette neu rechnet
    /// (Review #112 B1).
    pub fn mit(model: &Model, k: &(impl Ketten + ?Sized), form: RaumForm) -> Rohbau {
        Rohbau::mit_geschossen(model, k, form, &Geschosse::aus(model))
    }

    /// Wie [`Rohbau::mit`], die Geschosse aus `g` (Review #137 B2).
    pub fn mit_geschossen(
        model: &Model,
        k: &(impl Ketten + ?Sized),
        form: RaumForm,
        g: &Geschosse,
    ) -> Rohbau {
        let (z_boden, z_decke) = g.roh(form.geschoss);
        let oeffnungen = oeffnungen(model, k, &form, (z_boden, z_decke), g);
        Rohbau {
            form,
            z_boden,
            z_decke,
            oeffnungen,
        }
    }

    /// Umriss und Löcher, je als Ring von Kanten.
    pub fn ringe(&self) -> impl Iterator<Item = &[Kante]> {
        ringe(&self.form)
    }

    /// Alle Kanten in der fortlaufenden Zählung.
    pub fn kanten(&self) -> impl Iterator<Item = &Kante> {
        self.ringe().flat_map(|r| r.iter())
    }
}

fn ringe(f: &RaumForm) -> impl Iterator<Item = &[Kante]> {
    std::iter::once(f.umriss.as_slice()).chain(f.loecher.iter().map(|l| l.as_slice()))
}

/// OK Rohdecke und UK Rohdecke darüber (mm, absolut): Unterkante des
/// Geschosses und dieselbe plus lichte Höhe.
pub fn hoehen_roh(model: &Model, form: &RaumForm) -> (f64, f64) {
    let z = model.storey(form.geschoss).map_or(0.0, |s| s.elevation);
    (z, z + model.clear_height(form.geschoss))
}

/// Öffnungen in den Kanten des Raums, nach Kante und Lage: je Loch eines
/// Wandsegments, das eine Kante berührt, das Stück in der Kante, im Maß der
/// raumseitigen Schicht (hinter einer Außenschale um den Anschlag größer).
pub fn am_raum(model: &Model, form: &RaumForm) -> Vec<OeffnungAmRaum> {
    let g = Geschosse::aus(model);
    oeffnungen(model, model, form, g.roh(form.geschoss), &g)
}

/// [`am_raum`] mit den Ketten aus `ketten` und den Höhen des Raums.
fn oeffnungen(
    model: &Model,
    ketten: &(impl Ketten + ?Sized),
    form: &RaumForm,
    (z_boden, z_decke): (f64, f64),
    geschosse: &Geschosse,
) -> Vec<OeffnungAmRaum> {
    let mut zuege: Vec<(RunId, Option<Cow<'_, WallChain>>)> = Vec::new();
    let andere = andere_geschosse(geschosse, form);
    // Die lichte Höhe eines anderen Geschosses nur, wenn die Öffnung in
    // seinen Rohbau reicht; sonst ist ihr Stück dort ohnehin leer
    let weiter = |z0: f64, z1: f64, unten: bool| -> f64 {
        andere
            .iter()
            .filter(|g| z1 > g.1 && z0 < g.2)
            .map(|&(id, z, _)| (z, z + geschosse.licht(id)))
            .filter(|g| {
                if unten {
                    g.1 <= z_boden + 0.5
                } else {
                    g.0 >= z_decke - 0.5
                }
            })
            .map(|g| (z1.min(g.1) - z0.max(g.0)).max(0.0))
            .sum()
    };
    let mut out = Vec::new();
    for (i, k) in ringe(form).flat_map(|r| r.iter()).enumerate() {
        let Begrenzung::Wand {
            wand,
            schicht,
            seite,
            von,
            bis,
        } = k.quelle;
        if matches!(seite, Seite::Anfang | Seite::Ende) {
            continue;
        }
        let Some((run, seg)) = segment(model, form, wand) else {
            continue;
        };
        if !zuege.iter().any(|z| z.0 == run) {
            zuege.push((run, ketten.kette(run)));
        }
        let Some(Some(c)) = zuege.iter().find(|z| z.0 == run).map(|z| &z.1) else {
            continue;
        };
        let aussen = aussen_links(c) == (seite == Seite::Links);
        let laenge = k.laenge();
        // Läuft die Kante in Zeichenrichtung des Segments? Dann liegt ihr
        // Anfang bei `von`, sonst bei `bis`.
        let vorwaerts = laeuft_vorwaerts(c, seg, k);
        for h in c.joints.holes.iter().filter(|h| h.seg == seg) {
            let Some(id) = h.opening else {
                continue;
            };
            let Some(o) = model.opening(id) else {
                continue;
            };
            // Tür in einer Innenwand: Innentür, Laibung über die ganze
            // Rohbaudicke (Öffnungen #99)
            let innen = !h.outer && o.kind == sk_model::oeffnung::OpeningKind::Door;
            let ((u0, u1), (z0, z1)) = h.kasten(schicht < h.shell);
            let (lo, hi) = (u0.max(von), u1.min(bis));
            if hi - lo < 1.0 {
                continue;
            }
            let (a, b) = if vorwaerts {
                (lo - von, hi - von)
            } else {
                (bis - hi, bis - lo)
            };
            out.push(OeffnungAmRaum {
                oeffnung: id,
                kante: i,
                von: a.max(0.0),
                bis: b.min(laenge),
                uk: z0,
                ok: z1,
                tiefe: if innen {
                    h.install.outer + h.install.element + h.install.inner
                } else if aussen {
                    h.install.outer
                } else {
                    h.install.inner
                },
                art: if innen {
                    OpeningKind::InnerDoor
                } else {
                    o.kind.into()
                },
                // Das Blatt sitzt auf der Seite des Raums, in den die Tür
                // aufschlägt (Jörn 12.10. 05:52); Seite wie die der Kante
                // zur Zeichenrichtung des Segments
                aufschlag_hier: h
                    .leaf
                    .map(|l| (l.side == FaceSide::Left) == (seite == Seite::Links)),
                weiter: [weiter(z0, z1, true), weiter(z0, z1, false)],
            });
        }
    }
    out.sort_by(|a, b| a.kante.cmp(&b.kante).then(a.von.total_cmp(&b.von)));
    out
}

/// Die anderen Geschosse im Gebäude des Raums mit OK Rohdecke und Oberkante
/// des Geschosses (mm, absolut).
fn andere_geschosse(g: &Geschosse, form: &RaumForm) -> Vec<(StoreyId, f64, f64)> {
    let Some(b) = g.lagen.get(&form.geschoss).map(|s| s.gebaeude) else {
        return Vec::new();
    };
    g.im_gebaeude(b)
        .filter(|(id, s)| *id != form.geschoss && s.art == LevelKind::Storey)
        .map(|(id, s)| (id, s.z, s.z + s.hoehe))
        .collect()
}

/// Wandzug und Segment des Bauteils `wand` im Geschoss des Raums.
fn segment(model: &Model, form: &RaumForm, wand: ElementId) -> Option<(RunId, usize)> {
    let (run, seg) = model.segment_of(wand)?;
    let r = model.run(run).filter(|r| r.storey == form.geschoss)?;
    (r.segments.get(seg) == Some(&wand)).then_some((run, seg))
}

/// Liegt die Außenseite des Zugs links der Zeichenrichtung (kleinster
/// Versatz der Schichten)? Wie die Erkennung des Grundrisses.
fn aussen_links(c: &WallChain) -> bool {
    let lagen = c.layer_offsets();
    let lo = lagen.iter().map(|l| l.0).fold(f64::INFINITY, f64::min);
    let hi = lagen.iter().map(|l| l.1).fold(f64::NEG_INFINITY, f64::max);
    let o = c.outer_offset();
    (o - lo).abs() < (o - hi).abs()
}

/// Läuft die Kante `k` in Zeichenrichtung des Segments `seg`?
fn laeuft_vorwaerts(c: &WallChain, seg: usize, k: &Kante) -> bool {
    let p = c.clean_points();
    let n = p.len();
    if n < 2 {
        return true;
    }
    let d = p[(seg + 1) % n] - p[seg];
    (k.b - k.a).dot(sk_math::vec3(d.x, d.y, 0.0)) >= 0.0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::raeume;
    use sk_math::{vec3, Vec3};
    use sk_model::oeffnung::{Din, HeightLine, HeightRef, Opening, Swing};
    use sk_model::{Category, LevelRef, RefSide, StoreyId};

    /// Haus 10 × 8 m mit zwei Geschossen im Werkstyp (WDVS), Zug im EG.
    fn haus() -> (Model, RunId, StoreyId) {
        let mut m = Model::with_seed(87);
        let b = m.add_building(2);
        let pts = [
            vec3(0.0, 0.0, 0.0),
            vec3(0.0, 8000.0, 0.0),
            vec3(10000.0, 8000.0, 0.0),
            vec3(10000.0, 0.0, 0.0),
        ];
        let run = m.build_from_polygon(b, &pts).unwrap();
        let st = m.run(run).unwrap().storey;
        (m, run, st)
    }

    /// Ecken der Außenfläche des Zugs.
    fn aussen(m: &Model, run: RunId) -> Vec<Vec3> {
        let c = m.chain(run).unwrap();
        c.face_corners(c.outer_offset())
    }

    fn oeffnung(st: StoreyId, path: Vec<Vec3>) -> Opening {
        Opening {
            kind: sk_model::oeffnung::OpeningKind::Window,
            path,
            bottom: HeightRef::Line(st, HeightLine::Sill),
            top: HeightRef::Line(st, HeightLine::Lintel),
            reveal: None,
            install: Default::default(),
            swing: None,
            lock: Default::default(),
        }
    }

    /// Kante des Raums an Segment `seg` des Zugs.
    fn kante_an(m: &Model, r: &Rohbau, run: RunId, seg: usize) -> usize {
        let wand = m.run(run).unwrap().segments[seg];
        r.kanten()
            .position(|k| matches!(k.quelle, Begrenzung::Wand { wand: w, .. } if w == wand))
            .unwrap()
    }

    #[test]
    fn fenster_in_der_kante_des_raums() {
        let (mut m, run, st) = haus();
        let f = aussen(&m, run);
        let d = (f[1] - f[0]).normalized();
        let o = oeffnung(st, vec![f[0] + d * 2000.0, f[0] + d * 3510.0]);
        let id = m.add_opening(o.clone()).unwrap();
        let formen = raeume(&m, st);
        assert_eq!(formen.len(), 1);
        let r = Rohbau::neu(&m, formen[0].clone());
        assert_eq!((r.z_boden, r.z_decke), (0.0, m.clear_height(st)));
        assert_eq!(r.oeffnungen.len(), 1, "{:?}", r.oeffnungen);
        let x = r.oeffnungen[0];
        let k = kante_an(&m, &r, run, 0);
        assert_eq!((x.oeffnung, x.kante, x.art), (id, k, OpeningKind::Window));
        // Hinter dem WDVS um den Anschlag 30 mm größer, seitlich und oben
        assert!((x.bis - x.von - 1570.0).abs() < 1e-6, "{x:?}");
        let s = m.storey(st).unwrap();
        assert_eq!(x.uk, s.elevation + s.lines.sill);
        assert_eq!(x.ok, s.elevation + s.lines.lintel + 30.0);
        assert_eq!(x.tiefe, m.install_depth(&o, run).inner);
        // Die Mitte liegt in der Flucht der Fenstermitte
        let kante = r.kanten().nth(k).unwrap();
        let mitte = f[0] + d * 2755.0;
        let u = (mitte - kante.a).dot((kante.b - kante.a).normalized());
        assert!((u - (x.von + x.bis) / 2.0).abs() < 1e-6, "{u} {x:?}");
        assert_eq!(x.aufschlag_hier, None);
    }

    #[test]
    fn ecke_auf_zwei_kanten() {
        let (mut m, run, st) = haus();
        let f = aussen(&m, run);
        let d0 = (f[1] - f[0]).normalized();
        let d1 = (f[2] - f[1]).normalized();
        let o = oeffnung(st, vec![f[1] - d0 * 800.0, f[1], f[1] + d1 * 900.0]);
        let id = m.add_opening(o).unwrap();
        let r = Rohbau::neu(&m, raeume(&m, st).remove(0));
        let (k0, k1) = (kante_an(&m, &r, run, 0), kante_an(&m, &r, run, 1));
        let laenge = |k: usize| r.kanten().nth(k).unwrap().laenge();
        let a = r.oeffnungen.iter().find(|x| x.kante == k0).unwrap();
        let b = r.oeffnungen.iter().find(|x| x.kante == k1).unwrap();
        assert_eq!((a.oeffnung, b.oeffnung), (id, id));
        // Der Raum läuft gegen den Uhrzeigersinn: erst die Kante an Segment 1
        // bis in die Ecke, dann die an Segment 0 ab der Ecke; zur Ecke hin
        // offen, sonst um den Anschlag 30 mm größer
        assert_eq!(k0, (k1 + 1) % 4);
        assert!(
            (b.bis - laenge(k1)).abs() < 1e-6 && a.von == 0.0,
            "{a:?} {b:?}"
        );
        let t = m.chain(run).unwrap().thickness();
        assert!((a.bis - a.von - (800.0 - t + 30.0)).abs() < 1e-6, "{a:?}");
        assert!((b.bis - b.von - (900.0 - t + 30.0)).abs() < 1e-6, "{b:?}");
        assert_eq!(r.oeffnungen.len(), 2);
    }

    /// Von der Brüstung im EG bis zum Sturz im OG: Jeder Raum sieht sein
    /// Stück und weiß, wie hoch die Öffnung im anderen Geschoss ist.
    #[test]
    fn ueber_zwei_geschosse() {
        let (mut m, run, st) = haus();
        let og = m.run(m.runs_above(run)[0]).unwrap().storey;
        let f = aussen(&m, run);
        let d = (f[1] - f[0]).normalized();
        let o = Opening {
            top: HeightRef::Line(og, HeightLine::Lintel),
            ..oeffnung(st, vec![f[0] + d * 2000.0, f[0] + d * 3000.0])
        };
        m.add_opening(o).unwrap();
        let unten = Rohbau::neu(&m, raeume(&m, st).remove(0));
        let oben = Rohbau::neu(&m, raeume(&m, og).remove(0));
        let (a, b) = (unten.oeffnungen[0], oben.oeffnungen[0]);
        assert_eq!((a.uk, a.ok), (b.uk, b.ok));
        assert!(a.uk > unten.z_boden && a.ok > oben.z_boden, "{a:?}");
        assert_eq!(a.weiter[0], 0.0);
        assert!((a.weiter[1] - (a.ok - oben.z_boden)).abs() < 1e-6, "{a:?}");
        assert!((b.weiter[0] - (unten.z_decke - b.uk)).abs() < 1e-6, "{b:?}");
        assert_eq!(b.weiter[1], 0.0);
    }

    /// Haus mit Innenwand bei x = 5 m und einer Innentür darin, die nach
    /// Osten (+x) aufschlägt.
    fn mit_innentuer() -> (Model, StoreyId, ElementId, f64) {
        let (mut m, _, st) = haus();
        let set = m.defaults().interior_wall;
        let pts = [vec3(5000.0, 0.0, 0.0), vec3(5000.0, 8000.0, 0.0)];
        let iw = m
            .add_wall_run(
                &pts,
                false,
                RefSide::Center,
                st,
                set,
                Category::InteriorWall,
            )
            .unwrap();
        let c = m.chain(iw).unwrap();
        let (x, t) = (5000.0 + c.outer_offset(), c.thickness());
        let tuer = Opening {
            kind: sk_model::oeffnung::OpeningKind::Door,
            bottom: HeightRef::Level(LevelRef::bottom(st)),
            top: HeightRef::Line(st, HeightLine::Door),
            // Wand und Pfad laufen in +y, rechts ist +x
            swing: Some(Swing {
                side: FaceSide::Right,
                din: Din::Links,
            }),
            ..oeffnung(st, vec![vec3(x, 3000.0, 0.0), vec3(x, 3885.0, 0.0)])
        };
        let id = m.add_opening(tuer).unwrap();
        (m, st, id, t)
    }

    /// Die Öffnung je Raum, der Raum östlich der Innenwand zuerst.
    fn ost_west(m: &Model, st: StoreyId) -> Vec<OeffnungAmRaum> {
        let mut r: Vec<(bool, OeffnungAmRaum)> = raeume(m, st)
            .into_iter()
            .map(|f| {
                let ost = f.mitte.x > 5000.0;
                let r = Rohbau::neu(m, f);
                assert_eq!(r.oeffnungen.len(), 1, "{:?}", r.oeffnungen);
                (ost, r.oeffnungen[0])
            })
            .collect();
        r.sort_by_key(|x| !x.0);
        assert_eq!(r.len(), 2);
        assert!(r[0].0 && !r[1].0);
        r.into_iter().map(|x| x.1).collect()
    }

    /// Innentür in der Innenwand (Öffnungen #99): Art Innentür, Laibung über
    /// die ganze Rohbaudicke, ohne Anschlag; aufgeschlagen in den Raum, auf
    /// dessen Seite das Blatt liegt (Jörn 12.10. 05:52). Kippt der
    /// Aufschlag, wechselt der Raum; ohne Blatt keiner.
    #[test]
    fn innentuer_mit_aufschlag() {
        let (mut m, st, id, t) = mit_innentuer();
        let [ost, west] = ost_west(&m, st)[..] else {
            panic!()
        };
        for x in [ost, west] {
            assert_eq!((x.oeffnung, x.art), (id, OpeningKind::InnerDoor));
            assert!((x.tiefe - t).abs() < 1e-6, "{x:?}");
            assert!((x.bis - x.von - 885.0).abs() < 1e-6, "{x:?}");
            assert_eq!(x.uk, m.storey(st).unwrap().elevation);
        }
        assert_eq!(
            (ost.aufschlag_hier, west.aufschlag_hier),
            (Some(true), Some(false))
        );
        let mut o = m.opening(id).cloned().unwrap();
        o.swing = o.swing.map(Swing::flipped);
        assert!(m.set_opening(id, o.clone()));
        let v = ost_west(&m, st);
        assert_eq!(
            (v[0].aufschlag_hier, v[1].aufschlag_hier),
            (Some(false), Some(true))
        );
        o.swing = None;
        assert!(m.set_opening(id, o));
        assert!(ost_west(&m, st).iter().all(|x| x.aufschlag_hier.is_none()));
    }

    /// Die Türnische gehört dem Raum, aus dem man auf das Blatt sieht: Sein
    /// Belag läuft durch die Laibung bis unter das Blatt, im Aufschlagraum
    /// keine Nische (Jörn 12.10. 05:52, VOB-Prüfung im Raumbuch §5.5).
    #[test]
    fn tuernische_im_raum_ohne_aufschlag() {
        let (mut m, st, _, t) = mit_innentuer();
        m.begin("Räume");
        assert_eq!(crate::abgleichen(&mut m, &[st]), 2);
        m.commit();
        for id in m.spaces(st) {
            m.begin("Ausbau");
            assert!(crate::ausbau::typen_anlegen(&mut m, id));
            m.commit();
        }
        let formen = raeume(&m, st);
        assert_eq!(formen.len(), 2);
        for f in formen {
            let ost = f.mitte.x > 5000.0;
            let roh = Rohbau::neu(&m, f);
            let aus = crate::ausbau::RaumAusbau::fuer(&m, &roh).unwrap();
            let z = crate::raumbuch::rechnen(&m, &roh, &aus, &Default::default());
            let boden = z
                .iter()
                .find(|z| z.flaeche == crate::raumbuch::Flaeche::Boden)
                .unwrap();
            let nische: Vec<_> = boden
                .terme
                .iter()
                .filter(|x| x.text.starts_with("Türnische"))
                .collect();
            assert_eq!(nische.len(), 1, "{nische:?}");
            if ost {
                assert_eq!(nische[0].wert, 0.0, "{nische:?}");
            } else {
                assert!((nische[0].wert - 885.0 * t).abs() < 1e-3, "{nische:?}");
            }
        }
    }

    /// Die Geschosse aus einem Gang geben dieselben Höhen, denselben Rohbau
    /// und denselben Estrich wie die Gänge je Raum (Review #137 B2), auch
    /// mit dickerer Decke und einem zweiten Gebäude daneben.
    #[test]
    fn geschosse_wie_das_modell() {
        use crate::ausbau::{estrich_vorgabe, RaumAusbau};
        let (mut m, run, st) = haus();
        let og = m.run(m.runs_above(run)[0]).unwrap().storey;
        let f = aussen(&m, run);
        let d = (f[1] - f[0]).normalized();
        let o = Opening {
            top: HeightRef::Line(og, HeightLine::Lintel),
            ..oeffnung(st, vec![f[0] + d * 2000.0, f[0] + d * 3000.0])
        };
        m.add_opening(o).unwrap();
        let decke = m.elements().iter().find_map(|(id, e)| match e.kind {
            ElementKind::Floor(f) if f.top.storey == st => Some(id),
            _ => None,
        });
        assert!(m.set_floor_thickness(decke.unwrap(), 300.0));
        let b = m.add_building(1);
        let pts = [
            (20000.0, 0.0),
            (20000.0, 6000.0),
            (26000.0, 6000.0),
            (26000.0, 0.0),
        ];
        let pts = pts.map(|(x, y)| vec3(x, y, 0.0));
        m.build_from_polygon(b, &pts).unwrap();
        let g = Geschosse::aus(&m);
        let (mut sohle, mut raeume_) = (0, 0);
        for (id, _) in m.storeys().iter() {
            assert_eq!(g.licht(id), m.clear_height(id));
            // Geschoss darunter wie aus `levels_in` (H-138f)
            let ebenen = m.levels_in(m.building_of(id));
            let i = ebenen.iter().position(|x| *x == id).unwrap();
            let unten = i
                .checked_sub(1)
                .map(|j| (ebenen[j], m.storey(ebenen[j]).unwrap().kind));
            assert_eq!(g.darunter(id), unten);
            let s = estrich_vorgabe(&m, id) == sk_model::model::SCREED_SLAB_TYPE_GUID;
            assert_eq!(g.auf_sohle(id), s);
            sohle += usize::from(s);
            for form in raeume(&m, id) {
                let r = Rohbau::mit_geschossen(&m, &m, form.clone(), &g);
                assert_eq!((r.z_boden, r.z_decke), hoehen_roh(&m, &form));
                let a = RaumAusbau::fuer_in(&m, &r, &g);
                assert!(a.is_some());
                assert_eq!(a, RaumAusbau::fuer(&m, &r));
                raeume_ += 1;
            }
        }
        assert_eq!((sohle, raeume_), (2, 3));
        // Die Öffnung reicht im OG bis über die dickere Decke des EG
        let oben = Rohbau::mit_geschossen(&m, &m, raeume(&m, og).remove(0), &g);
        let x = oben.oeffnungen[0];
        let s = m.storey(st).unwrap();
        let ok_eg = s.elevation + m.clear_height(st);
        assert_eq!(ok_eg, s.elevation + s.height - 300.0);
        assert!((x.weiter[0] - (ok_eg - x.uk)).abs() < 1e-6, "{x:?}");
    }

    #[test]
    fn ohne_oeffnung_nichts() {
        let (m, _, st) = haus();
        let r = Rohbau::neu(&m, raeume(&m, st).remove(0));
        assert!(r.oeffnungen.is_empty());
        assert_eq!(r.kanten().count(), 4);
    }
}
