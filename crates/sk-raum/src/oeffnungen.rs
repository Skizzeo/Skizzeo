//! Öffnungen und Rohbauhöhen eines Raums (Raumbuch RB3). Die Schnittstelle
//! (`grundriss/schnittstelle-raum.md` §3) sieht sie an der [`RaumForm`] vor;
//! Grundriss G3 rechnet nur die Form, deshalb leitet das Raumbuch sie hier
//! ab: die Öffnungen aus den Löchern der Wandzüge (O2), die Höhen aus dem
//! Geschoss. Kanten zählen fortlaufend über den Umriss und danach über die
//! Löcher in ihrer Reihenfolge ([`Rohbau::kanten`]).

use crate::erkennung::{Begrenzung, Kante, RaumForm, Seite};
use sk_model::{ElementId, LevelKind, Model, RunId, WallChain};

/// Art der Öffnung. Bis Grundriss G6 die Innentür in `sk_model::oeffnung::OpeningKind`
/// bringt, hier mit ihr; dann entfällt dieser Typ.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OpeningKind {
    /// Wandöffnung: Fenster, Terrassentür.
    Window,
    /// Türöffnung in der Außenwand: Haustür.
    Door,
    /// Innentüröffnung.
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

impl Rohbau {
    pub fn neu(model: &Model, form: RaumForm) -> Rohbau {
        let (z_boden, z_decke) = hoehen_roh(model, &form);
        let oeffnungen = am_raum(model, &form);
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
    let mut zuege: Vec<(RunId, Option<WallChain>)> = Vec::new();
    let (z_boden, z_decke) = hoehen_roh(model, form);
    let andere = andere_geschosse(model, form);
    let weiter = |z0: f64, z1: f64, unten: bool| -> f64 {
        andere
            .iter()
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
            zuege.push((run, model.chain(run)));
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
                tiefe: if aussen {
                    h.install.outer
                } else {
                    h.install.inner
                },
                art: o.kind.into(),
                aufschlag_hier: None,
                weiter: [weiter(z0, z1, true), weiter(z0, z1, false)],
            });
        }
    }
    out.sort_by(|a, b| a.kante.cmp(&b.kante).then(a.von.total_cmp(&b.von)));
    out
}

/// OK Rohdecke und UK Rohdecke darüber der anderen Geschosse im Gebäude des
/// Raums (mm, absolut).
fn andere_geschosse(model: &Model, form: &RaumForm) -> Vec<(f64, f64)> {
    let Some(b) = model.storey(form.geschoss).map(|s| s.building) else {
        return Vec::new();
    };
    model
        .storeys()
        .iter()
        .filter(|(id, s)| *id != form.geschoss && s.building == b && s.kind == LevelKind::Storey)
        .map(|(id, s)| (s.elevation, s.elevation + model.clear_height(id)))
        .collect()
}

/// Wandzug und Segment des Bauteils `wand` im Geschoss des Raums.
fn segment(model: &Model, form: &RaumForm, wand: ElementId) -> Option<(RunId, usize)> {
    model
        .runs()
        .iter()
        .filter(|(_, r)| r.storey == form.geschoss)
        .find_map(|(id, r)| Some((id, r.segments.iter().position(|&s| s == wand)?)))
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
    use sk_model::oeffnung::{HeightLine, HeightRef, Opening};
    use sk_model::StoreyId;

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

    #[test]
    fn ohne_oeffnung_nichts() {
        let (m, _, st) = haus();
        let r = Rohbau::neu(&m, raeume(&m, st).remove(0));
        assert!(r.oeffnungen.is_empty());
        assert_eq!(r.kanten().count(), 4);
    }
}
