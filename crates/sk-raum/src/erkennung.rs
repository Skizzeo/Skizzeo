//! Raumerkennung (Grundriss G3, Jörn 11.10. 16:53, J10): Jede Fläche, die
//! die Wände eines Geschosses ganz umschließen, ist ein Raum. Die Wände gehen
//! mit ihrem Rohbauumriss ein (ohne raumseitige Schichten der Funktion
//! „Bekleidung“, schnittstelle-raum.md §3), Öffnungen unterbrechen nichts.
//! Eine Fläche, die nur Außenseiten von Außenwänden umgrenzen (Innenhof),
//! ist kein Raum; ohne Umschließung gibt es keinen Raum und keinen Hinweis.

use sk_math::flaeche::{anordnung, innenpunkt, Bereich, Herkunft, Ring};
use sk_math::{vec3, Vec3};
use sk_model::{Category, ElementId, LayerFunction, MaterialId, Model, RunId, StoreyId, WallChain};

/// Wände, die so knapp (mm) vor einer anderen enden, schließen den Raum noch.
pub const TOL: f64 = 0.5;
/// Kleinste Raumfläche (mm², 0,1 m²); kleinere Flächen sind Reste zwischen Wänden.
pub const MIN_FLAECHE: f64 = 100_000.0;
/// Kleinste mittlere Breite (2 · Fläche / Umfang, mm): schmale Spalten
/// zwischen zwei Wänden sind kein Raum.
pub const MIN_BREITE: f64 = 200.0;
/// Genauigkeit der Stempelmitte (mm), mindestens.
const GENAU: f64 = 10.0;
/// In größeren Räumen genügt dieser Anteil der kürzeren Seite ihres
/// Umrisskastens: längs der Mitte eines langen Raums sind viele Stellen
/// gleich gut, auf 10 mm genau teilte die Suche dort Hunderte Zellen
/// (Review #87 B3).
const GENAU_ANTEIL: f64 = 0.02;

/// Seite des Wandsegments, an der der Raum liegt: links oder rechts der
/// Zeichenrichtung, oder an der Stirn am Anfang bzw. Ende (freies Wandende).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Seite {
    Links,
    Rechts,
    Anfang,
    Ende,
}

/// Woher eine Kante des Raums kommt.
#[derive(Clone, Debug, PartialEq)]
pub enum Begrenzung {
    /// Rohbaufläche einer Wand: Bauteil (Segment), Schicht (von außen gezählt,
    /// die raumseitige ohne Bekleidung), Seite und Lauf längs der Wand von–bis
    /// (mm ab dem Anfang des Segments auf der Bezugslinie).
    Wand {
        wand: ElementId,
        schicht: usize,
        seite: Seite,
        von: f64,
        bis: f64,
    },
}

/// Kante des Raumumrisses (Grundrissebene, z = 0).
#[derive(Clone, Debug, PartialEq)]
pub struct Kante {
    pub a: Vec3,
    pub b: Vec3,
    pub quelle: Begrenzung,
    /// Bekleidungsschichten des Wandtyps zum Raum hin, von der Rohbaufläche
    /// aus: Dicke (mm) und Baustoff; leer an Stirnseiten und ohne Bekleidung.
    pub ausbau_typ: Vec<(f64, MaterialId)>,
}

impl Kante {
    pub fn laenge(&self) -> f64 {
        (self.b - self.a).length()
    }
}

/// Ecke des Raums: Innenwinkel in Grad (aus Sicht des Raums) und ob es eine
/// Innenecke ist (Winkel unter 180°, z. B. jede Ecke eines Rechteckraums).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Ecke {
    pub p: Vec3,
    pub winkel: f64,
    pub innen: bool,
}

/// Gerechnete Form eines Raums (nie gespeichert).
#[derive(Clone, Debug, PartialEq)]
pub struct RaumForm {
    pub geschoss: StoreyId,
    /// Gegen den Uhrzeigersinn, Rohbaumaß.
    pub umriss: Vec<Kante>,
    /// Im Uhrzeigersinn: frei stehende Wände und Schächte im Raum.
    pub loecher: Vec<Vec<Kante>>,
    /// Je Punkt des Umrisses, dann je Punkt der Löcher.
    pub ecken: Vec<Ecke>,
    /// mm², Rohbau.
    pub flaeche: f64,
    /// mm, Rohbau (Umriss und Löcher).
    pub umfang: f64,
    /// Gespeicherter Raum dieser Fläche (Ankerregel, siehe
    /// [`crate::abgleich`]); `None`: eben erkannt, noch nicht angelegt.
    pub raum: Option<ElementId>,
    /// Mitte des größten Kreises in der Fläche (Vorgabe für den Stempel).
    pub mitte: Vec3,
    /// Lage des Raumstempels: die verschobene des Raums, wenn sie in der
    /// Fläche liegt, sonst [`RaumForm::mitte`].
    pub stempel: Vec3,
}

impl RaumForm {
    /// Liegt `p` im Raum (nicht in einem Loch)?
    pub fn enthaelt(&self, p: Vec3) -> bool {
        let ring = |k: &[Kante]| k.iter().map(|k| k.a).collect::<Vec<_>>();
        sk_math::flaeche::enthaelt(&ring(&self.umriss), p)
            && !self
                .loecher
                .iter()
                .any(|l| sk_math::flaeche::enthaelt(&ring(l), p))
    }
}

/// Ein Segment im Rohbauumriss: Bauteil, Bezugslinie, Rohbauflächen links
/// und rechts mit ihrer Schicht, und welche Seite außen liegt (Außenwand).
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Teil {
    wand: ElementId,
    p0: Vec3,
    d: Vec3,
    schicht: [usize; 2],
    aussen: Option<Seite>,
    /// Bekleidung links und rechts, von der Rohbaufläche nach außen.
    ausbau: [Vec<(f64, MaterialId)>; 2],
}

impl Teil {
    pub(crate) fn hash_in(&self, h: &mut impl std::hash::Hasher) {
        use std::hash::Hash;
        self.wand.hash(h);
        for v in [self.p0, self.d] {
            [v.x, v.y, v.z].map(f64::to_bits).hash(h);
        }
        self.schicht.hash(h);
        self.aussen.hash(h);
        for a in self.ausbau.iter() {
            a.len().hash(h);
            for (d, m) in a {
                d.to_bits().hash(h);
                m.hash(h);
            }
        }
    }
}

/// Räume im Geschoss `geschoss`, geordnet nach ihrem tiefsten, dann
/// linkesten Punkt. Rechnet alles neu; die Szene nimmt die Erkennung mit
/// Zwischenspeicher ([`crate::Erkennung`]), mit demselben Ergebnis.
pub fn raeume(model: &Model, geschoss: StoreyId) -> Vec<RaumForm> {
    crate::Erkennung::default()
        .lage(model, model, geschoss)
        .raeume(model)
}

/// Räume aus den Rohbauumrissen einer Hausgruppe (ein Teil je Umriss),
/// ohne Innenhöfe, Reste und Spalten; noch ohne Raum und Ordnung.
pub(crate) fn formen_aus(
    umrisse: &[Vec<Vec3>],
    teile: &[Teil],
    geschoss: StoreyId,
) -> Vec<RaumForm> {
    anordnung(umrisse, TOL)
        .bereiche
        .iter()
        .filter(|b| !innenhof(b, teile))
        .map(|b| form(b, teile, geschoss))
        .filter(|f| f.flaeche >= MIN_FLAECHE && 2.0 * f.flaeche / f.umfang >= MIN_BREITE)
        .collect()
}

/// Rohbauumriss je Segment eines Zuges: Viereck aus der linken und rechten
/// Rohbaufläche (Kanten 0 links, 1 Stirn am Ende, 2 rechts, 3 Stirn am Anfang).
pub(crate) fn rohbau(
    m: &Model,
    run: RunId,
    c: &WallChain,
    umrisse: &mut Vec<Vec<Vec3>>,
    teile: &mut Vec<Teil>,
) {
    let Some(r) = m.run(run) else {
        return;
    };
    let lagen = c.layer_offsets();
    let Some(&first) = r.segments.first() else {
        return;
    };
    let el = m.element(first);
    // Aufkantungen auf dem Flachdach umschließen keinen Raum
    if !el.is_some_and(|e| matches!(e.category, Category::ExteriorWall | Category::InteriorWall)) {
        return;
    }
    let schichten: Vec<(LayerFunction, MaterialId)> = el
        .and_then(|e| e.layer_set)
        .and_then(|s| m.layer_set(s))
        .map(|s| s.layers.iter().map(|l| (l.function, l.material)).collect())
        .unwrap_or_default();
    let funktionen: Vec<LayerFunction> = schichten.iter().map(|s| s.0).collect();
    let bekleidung = |i: usize| funktionen.get(i) == Some(&LayerFunction::Finish);
    // Rohbauflächen: von jeder Seite die Bekleidungsschichten abziehen
    let mut nach_lo: Vec<usize> = (0..lagen.len()).collect();
    nach_lo.sort_by(|&a, &b| lagen[a].0.total_cmp(&lagen[b].0));
    let roh: Vec<usize> = nach_lo
        .iter()
        .copied()
        .filter(|&i| !bekleidung(i))
        .collect();
    let (Some(&li), Some(&re)) = (roh.first(), roh.last()) else {
        return;
    };
    let (lo, hi) = (lagen[li].0, lagen[re].1);
    // Bekleidung je Seite: was in der Reihenfolge vor bzw. nach dem Rohbau liegt
    let bekl = |idx: &[usize]| -> Vec<(f64, MaterialId)> {
        idx.iter()
            .filter_map(|&i| Some((lagen[i].1 - lagen[i].0, schichten.get(i)?.1)))
            .collect()
    };
    let pos = |i| nach_lo.iter().position(|&x| x == i).unwrap_or(0);
    let links: Vec<usize> = nach_lo[..pos(li)].iter().rev().copied().collect();
    let rechts: Vec<usize> = nach_lo[pos(re) + 1..].to_vec();
    let ausbau = [bekl(&links), bekl(&rechts)];
    let aussen = (el.map(|e| e.category) == Some(Category::ExteriorWall)).then(|| {
        let o = c.outer_offset();
        if (o - lagen[nach_lo[0]].0).abs() < (o - lagen[nach_lo[nach_lo.len() - 1]].1).abs() {
            Seite::Links
        } else {
            Seite::Rechts
        }
    });
    let (fl, fr) = (c.face_corners(lo), c.face_corners(hi));
    let pts = c.clean_points();
    let n = fl.len();
    for k in 0..c.segment_count() {
        let Some(&wand) = r.segments.get(k) else {
            continue;
        };
        let j = (k + 1) % n;
        umrisse.push(vec![fl[k], fl[j], fr[j], fr[k]]);
        let p0 = vec3(pts[k].x, pts[k].y, 0.0);
        let p1 = pts[(k + 1) % pts.len()];
        teile.push(Teil {
            wand,
            p0,
            d: (vec3(p1.x, p1.y, 0.0) - p0).normalized(),
            schicht: [li, re],
            aussen,
            ausbau: ausbau.clone(),
        });
    }
}

/// Seite und Schicht einer Umrisskante (Kantennummer im Viereck).
fn seite(t: &Teil, kante: usize) -> (Seite, usize) {
    match kante {
        0 => (Seite::Links, t.schicht[0]),
        2 => (Seite::Rechts, t.schicht[1]),
        1 => (Seite::Ende, t.schicht[0]),
        _ => (Seite::Anfang, t.schicht[0]),
    }
}

/// Nur Außenseiten von Außenwänden (Stirnseiten zählen nicht mit).
fn innenhof(b: &Bereich, teile: &[Teil]) -> bool {
    b.rand.herkunft.iter().all(|h| {
        let t = &teile[h.poly];
        let (s, _) = seite(t, h.kante);
        matches!(s, Seite::Anfang | Seite::Ende) || t.aussen == Some(s)
    })
}

fn kanten(r: &Ring, teile: &[Teil]) -> Vec<Kante> {
    let n = r.punkte.len();
    (0..n)
        .map(|i| {
            let (a, b) = (r.punkte[i], r.punkte[(i + 1) % n]);
            let h = r.herkunft[i];
            let t = &teile[h.poly];
            let ausbau_typ = match seite(t, h.kante).0 {
                Seite::Links => t.ausbau[0].clone(),
                Seite::Rechts => t.ausbau[1].clone(),
                Seite::Anfang | Seite::Ende => Vec::new(),
            };
            Kante {
                a,
                b,
                quelle: quelle(h, a, b, teile),
                ausbau_typ,
            }
        })
        .collect()
}

fn quelle(h: Herkunft, a: Vec3, b: Vec3, teile: &[Teil]) -> Begrenzung {
    let t = &teile[h.poly];
    let (seite, schicht) = seite(t, h.kante);
    let (sa, sb) = ((a - t.p0).dot(t.d), (b - t.p0).dot(t.d));
    Begrenzung::Wand {
        wand: t.wand,
        schicht,
        seite,
        von: sa.min(sb),
        bis: sa.max(sb),
    }
}

fn ecken(k: &[Kante]) -> Vec<Ecke> {
    let n = k.len();
    (0..n)
        .map(|i| {
            let e0 = k[(i + n - 1) % n].b - k[(i + n - 1) % n].a;
            let e1 = k[i].b - k[i].a;
            let dreh = (e0.x * e1.y - e0.y * e1.x).atan2(e0.dot(e1));
            Ecke {
                p: k[i].a,
                winkel: 180.0 - dreh.to_degrees(),
                innen: dreh > 1e-9,
            }
        })
        .collect()
}

fn form(b: &Bereich, teile: &[Teil], geschoss: StoreyId) -> RaumForm {
    let umriss = kanten(&b.rand, teile);
    let loecher: Vec<Vec<Kante>> = b.inseln.iter().map(|r| kanten(r, teile)).collect();
    let mut e = ecken(&umriss);
    for l in &loecher {
        e.extend(ecken(l));
    }
    let umfang = umriss
        .iter()
        .chain(loecher.iter().flatten())
        .map(Kante::laenge)
        .sum();
    let genau = crate::speicher::kasten_um(&b.rand.punkte).map_or(GENAU, |k| {
        GENAU.max(GENAU_ANTEIL * (k[2] - k[0]).min(k[3] - k[1]))
    });
    let mitte = innenpunkt(b, genau).0;
    RaumForm {
        geschoss,
        umriss,
        loecher,
        ecken: e,
        flaeche: b.flaeche(),
        umfang,
        raum: None,
        mitte,
        stempel: mitte,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sk_model::RefSide;

    /// Haus 10 × 8 m, Außenwand Bezug außen; Innenwand (Achse) bei x = 5 m
    /// von Innenfläche zu Innenfläche, wenn `teilen`.
    fn haus(teilen: bool) -> (Model, StoreyId, f64, f64) {
        let mut m = Model::new();
        let st = m.defaults().storey;
        let aw = m.defaults().exterior_wall;
        let iw = m.defaults().interior_wall;
        let dicke = |m: &Model, s| m.wall_layers(s).iter().map(|l| l.thickness).sum::<f64>();
        let (t, ti) = (dicke(&m, aw), dicke(&m, iw));
        let pts = [
            vec3(0.0, 0.0, 0.0),
            vec3(0.0, 8000.0, 0.0),
            vec3(10000.0, 8000.0, 0.0),
            vec3(10000.0, 0.0, 0.0),
        ];
        m.add_wall_run(&pts, true, RefSide::Left, st, aw, Category::ExteriorWall)
            .unwrap();
        if teilen {
            m.add_wall_run(
                &[vec3(5000.0, t, 0.0), vec3(5000.0, 8000.0 - t, 0.0)],
                false,
                RefSide::Center,
                st,
                iw,
                Category::InteriorWall,
            )
            .unwrap();
        }
        (m, st, t, ti)
    }

    fn nah(a: f64, b: f64) -> bool {
        (a - b).abs() < 1.0
    }

    #[test]
    fn ring_ergibt_einen_raum() {
        let (m, st, t, _) = haus(false);
        let r = raeume(&m, st);
        assert_eq!(r.len(), 1);
        let f = &r[0];
        assert!(nah(f.flaeche, (10000.0 - 2.0 * t) * (8000.0 - 2.0 * t)));
        assert!(nah(f.umfang, 2.0 * (18000.0 - 4.0 * t)));
        assert_eq!(f.umriss.len(), 4);
        assert!(f.ecken.iter().all(|e| e.innen && nah(e.winkel, 90.0)));
        // Jede Kante liegt an der Innenseite eines Außenwandsegments
        for k in &f.umriss {
            let Begrenzung::Wand {
                wand,
                seite,
                von,
                bis,
                ..
            } = &k.quelle;
            assert_eq!(m.element(*wand).unwrap().category, Category::ExteriorWall);
            assert_eq!(*seite, Seite::Rechts, "Bezug außen links, Raum rechts");
            assert!(nah(bis - von, k.laenge()));
        }
    }

    #[test]
    fn innenwand_teilt_in_zwei_raeume() {
        let (m, st, t, ti) = haus(true);
        let r = raeume(&m, st);
        assert_eq!(r.len(), 2);
        let h = 8000.0 - 2.0 * t;
        let mut f: Vec<f64> = r.iter().map(|f| f.flaeche).collect();
        f.sort_by(f64::total_cmp);
        assert!(nah(f[0], (5000.0 - ti / 2.0 - t) * h), "{f:?}");
        assert!(nah(f[1], (5000.0 - ti / 2.0 - t) * h), "{f:?}");
        // Der linke Raum grenzt mit seiner rechten Kante an die Innenwand
        let links = r
            .iter()
            .find(|f| f.enthaelt(vec3(1000.0, 1000.0, 0.0)))
            .unwrap();
        let an_iw = links
            .umriss
            .iter()
            .filter(|k| {
                let Begrenzung::Wand { wand, .. } = &k.quelle;
                m.element(*wand).unwrap().category == Category::InteriorWall
            })
            .count();
        assert_eq!(an_iw, 1);
    }

    #[test]
    fn offene_innenwand_teilt_nicht() {
        let (mut m, st, t, ti) = haus(false);
        let iw = m.defaults().interior_wall;
        m.add_wall_run(
            &[vec3(5000.0, t, 0.0), vec3(5000.0, 4000.0, 0.0)],
            false,
            RefSide::Center,
            st,
            iw,
            Category::InteriorWall,
        )
        .unwrap();
        let r = raeume(&m, st);
        assert_eq!(r.len(), 1);
        let ganz = (10000.0 - 2.0 * t) * (8000.0 - 2.0 * t);
        assert!(nah(r[0].flaeche, ganz - ti * (4000.0 - t)));
        // Die Stirn des freien Endes gehört zum Umriss
        assert!(r[0].umriss.iter().any(|k| matches!(
            k.quelle,
            Begrenzung::Wand {
                seite: Seite::Ende,
                ..
            }
        )));
        // Zwei Außenecken am freien Ende
        assert_eq!(r[0].ecken.iter().filter(|e| !e.innen).count(), 2);
    }

    #[test]
    fn innenhof_ist_kein_raum() {
        // Hof vor der rechten Außenwand, umschlossen von einem offenen
        // Außenwandzug, dessen Außenseite (Bezug links) zum Hof zeigt
        let hof = |bezug| {
            let (mut m, st, _, _) = haus(false);
            let aw = m.defaults().exterior_wall;
            let u = [
                vec3(10000.0, 2000.0, 0.0),
                vec3(14000.0, 2000.0, 0.0),
                vec3(14000.0, 6000.0, 0.0),
                vec3(10000.0, 6000.0, 0.0),
            ];
            m.add_wall_run(&u, false, bezug, st, aw, Category::ExteriorWall)
                .unwrap();
            raeume(&m, st)
        };
        let r = hof(RefSide::Left);
        assert_eq!(r.len(), 1, "nur das Haus, nicht der Hof");
        assert!(r[0].enthaelt(vec3(5000.0, 4000.0, 0.0)));
        // Zeigt die Innenseite zur Fläche, ist sie ein Raum (Anbau)
        assert_eq!(hof(RefSide::Right).len(), 2);
    }

    /// Referenzhäuser (sk-cost/referenz): RH1 ohne Innenwand, ein Raum je
    /// Geschoss; RH2 mit Verblender (Bekleidung außen) und je Geschoss einer
    /// Innenwand quer durchs Haus, zwei Räume.
    #[test]
    fn referenzhaeuser_je_geschoss() {
        for (datei, n) in [("rh1-standardhaus.szo", 1), ("rh2-mehrschalig.szo", 2)] {
            let pfad = format!("{}/../sk-cost/referenz/{datei}", env!("CARGO_MANIFEST_DIR"));
            let text = std::fs::read_to_string(pfad).unwrap();
            let m = sk_model::szo::read(&text, sk_model::GuidGen::with_seed(1))
                .unwrap()
                .model;
            let mut mit_waenden = 0;
            for (st, _) in m.storeys().iter() {
                if !m.runs().iter().any(|(_, r)| r.storey == st) {
                    continue;
                }
                mit_waenden += 1;
                let r = raeume(&m, st);
                assert_eq!(
                    r.len(),
                    n,
                    "{datei}: {:?}",
                    r.iter().map(|f| f.flaeche).collect::<Vec<_>>()
                );
            }
            assert_eq!(mit_waenden, 2, "{datei}: EG und OG");
        }
    }

    /// Innenputz als Schicht des Wandtyps: der Raum endet an der Rohbaufläche.
    #[test]
    fn bekleidung_zaehlt_nicht_zum_rohbau() {
        let (mut m, st, t, _) = haus(false);
        let aw = m.defaults().exterior_wall;
        let mut set = m.layer_set(aw).unwrap().clone();
        let putz = set.layers[0].material;
        set.layers.push(sk_model::MaterialLayer::new(
            putz,
            15.0,
            LayerFunction::Finish,
        ));
        assert!(m.set_layer_set(aw, set));
        let r = raeume(&m, st);
        assert_eq!(r.len(), 1);
        let t2 = t + 15.0;
        // Wand ist dicker geworden, Rohbau innen bleibt bei der alten Dicke
        assert!(
            nah(r[0].flaeche, (10000.0 - 2.0 * t) * (8000.0 - 2.0 * t)),
            "{} statt Putzfläche {}",
            r[0].flaeche,
            (10000.0 - 2.0 * t2) * (8000.0 - 2.0 * t2)
        );
        let Begrenzung::Wand { schicht, .. } = r[0].umriss[0].quelle;
        assert_eq!(schicht, set_len(&m, aw) - 2, "innerste Schicht ohne Putz");
        // Der Putz steht an jeder Kante als Ausbau des Typs
        assert!(r[0]
            .umriss
            .iter()
            .all(|k| k.ausbau_typ == vec![(15.0, putz)]));
    }

    fn set_len(m: &Model, s: sk_model::LayerSetId) -> usize {
        m.layer_set(s).unwrap().layers.len()
    }
}
