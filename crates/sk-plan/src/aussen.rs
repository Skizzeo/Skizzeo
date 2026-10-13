//! Außenmaßketten eines Geschosses (Konzept §3): je Gebäudeseite ein Block
//! aus Öffnungskette, Wandkette, Kette der Vor- und Rücksprünge und
//! Gesamtmaß, jede mit ihren Bezugspunkten und deren Herkunft.
//!
//! Die Hauptrichtung kommt aus der längsten Außenwand. Zu einer Seite gehört
//! jede Außenwandfläche, deren Außenseite in diese Richtung zeigt und von
//! dort zu sehen ist (die vorderste über jeder Stelle der Seite). Schräge
//! Wände bleiben in Stufe 1 unbemaßt.

use sk_math::{vec3, Vec3};
use sk_model::bemassung::{seite_grad, Aussenbezug, BrhBezug, Kettenart, Stil};
use sk_model::{
    Category, ElementId, ElementKind, JoinEnd, JoinKind, LevelKind, Model, RunId, StoreyId,
    WallChain,
};
use std::borrow::Borrow;
use std::collections::HashMap;

/// Gleiche Richtung bis 1°: Kosinus.
const PARALLEL: f64 = 0.999_847_7;
/// Bezugspunkte näher als das zählen als einer (mm).
const GLEICH: f64 = 0.5;
/// So nah muss eine Öffnung an der Außenfläche liegen (mm).
const AUF_FLAECHE: f64 = 2.0;

/// Woher ein Bezugspunkt kommt (für die Sichtprüfung und das Überschreiben).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Herkunft {
    /// Außenecke oder Ecke eines Vor- oder Rücksprungs.
    Ecke,
    /// Innenseite einer quer stehenden Außenwand.
    Innenseite,
    /// Seite einer angeschlossenen Innenwand.
    Innenwand,
    /// Kante einer Öffnung.
    Oeffnung,
}

/// Bauteil, das sich bewegt, wenn das Maß an diesem Bezugspunkt geändert
/// wird.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Quelle {
    /// Segment `seg` eines Außenwandzugs, das quer zur Kette steht.
    Wand { run: RunId, seg: usize },
    /// Eine angeschlossene Innenwand (der ganze Zug).
    Innenwand { run: RunId },
    /// Kante einer Öffnung: 0 die Kante mit kleinerem `t`, 1 die andere.
    Oeffnung { id: ElementId, ende: usize },
}

/// Ein Bezugspunkt einer Kette.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Bezug {
    /// Lage längs der Kette (`p · t`, mm).
    pub t: f64,
    /// Punkt am Bauteil im Grundriss (z = 0).
    pub p: Vec3,
    pub herkunft: Herkunft,
    pub quelle: Option<Quelle>,
}

/// Breite, Höhe und Brüstung einer Öffnung in der Öffnungskette.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Oeffnungsmass {
    pub id: ElementId,
    /// Kanten längs der Kette (`t0 < t1`).
    pub t0: f64,
    pub t1: f64,
    /// Lichte Höhe (mm).
    pub hoehe: f64,
    /// Brüstungshöhe ab dem gewählten Bezug; `None` bei Öffnungen, die am
    /// Boden beginnen (Türen).
    pub brh: Option<f64>,
    /// Außenfläche der Wand vor der Öffnung, quer (`p · n`).
    pub wand: f64,
}

/// Eine Maßkette.
#[derive(Clone, Debug, PartialEq)]
pub struct Kette {
    pub art: Kettenart,
    /// Nach `t` sortiert, mindestens zwei.
    pub bezuege: Vec<Bezug>,
    /// Lage der Maßlinie quer (`p · n`, mm).
    pub lage: f64,
    /// Von Hand verschoben (mm, + nach außen).
    pub versatz: f64,
}

/// Eine Gebäudeseite mit ihrem Kettenblock.
#[derive(Clone, Debug, PartialEq)]
pub struct Seite {
    /// Richtung der Außennormalen in ganzen Grad (Schlüssel der Versätze).
    pub grad: i32,
    /// Außennormale und Richtung längs der Ketten; `t` zeigt so, dass die
    /// Maßzahlen von unten oder von rechts zu lesen sind.
    pub n: Vec3,
    pub t: Vec3,
    /// Äußerste Bezugsfläche der Seite (`p · n`).
    pub aussen: f64,
    /// Von innen nach außen.
    pub ketten: Vec<Kette>,
    pub oeffnungen: Vec<Oeffnungsmass>,
}

/// Alle Außenmaßketten eines Geschosses.
#[derive(Clone, Debug, PartialEq)]
pub struct Aussenmasse {
    pub storey: StoreyId,
    /// Ebene der Ketten: OK Rohdecke des Geschosses (absolut).
    pub z: f64,
    pub seiten: Vec<Seite>,
}

/// Ein geschlossener Außenwandzug: Bezugsfläche außen und innen (nach dem
/// Bezug des Stils, [`flaechen`]) und Außenhaut (mit Dämmung und Putz), je
/// mit Gehrung an den Ecken.
struct Zug {
    run: RunId,
    bezug: Vec<Vec3>,
    innen: Vec<Vec3>,
    haut: Vec<Vec3>,
    /// Gegen den Uhrzeigersinn umlaufend?
    ccw: bool,
}

impl Zug {
    fn n(&self) -> usize {
        self.bezug.len()
    }

    fn kante(&self, i: usize) -> (Vec3, Vec3) {
        (self.bezug[i], self.bezug[(i + 1) % self.n()])
    }

    /// Außennormale von Kante `i`.
    fn normale(&self, i: usize) -> Vec3 {
        let (a, b) = self.kante(i);
        let d = (b - a).normalized();
        let r = vec3(d.y, -d.x, 0.0);
        if self.ccw {
            r
        } else {
            -r
        }
    }

    fn richtung(&self, i: usize) -> Vec3 {
        let (a, b) = self.kante(i);
        (b - a).normalized()
    }
}

/// Lage der tragenden Schicht quer zur Bezugslinie: (außen, innen). Ohne
/// tragende Schicht die ganze Wand.
pub fn kern(c: &WallChain) -> (f64, f64) {
    let (aussen, innen) = (c.outer_offset(), c.inner_offset());
    let lagen = c.layer_offsets();
    let Some(erste) = c.layers.iter().position(|l| l.core) else {
        return (aussen, innen);
    };
    let letzte = erste + c.layers[erste..].iter().take_while(|l| l.core).count() - 1;
    let nah = |(lo, hi, _): (f64, f64, u16)| {
        if (lo - aussen).abs() <= (hi - aussen).abs() {
            (lo, hi)
        } else {
            (hi, lo)
        }
    };
    match (lagen.get(erste), lagen.get(letzte)) {
        (Some(&a), Some(&b)) => (nah(a).0, nah(b).1),
        _ => (aussen, innen),
    }
}

/// Flächen quer zur Bezugslinie, an denen die Ketten eine Wand messen:
/// (außen, innen). Wandpaket: die ganze Wand von der Außen- bis zur
/// Innenkante (Jörn 13.10.), Rohbau: die tragende Schicht ([`kern`]).
pub fn flaechen(c: &WallChain, bezug: Aussenbezug) -> (f64, f64) {
    match bezug {
        Aussenbezug::Rohbau => kern(c),
        Aussenbezug::Wand => (c.outer_offset(), c.inner_offset()),
    }
}

/// Was die Ketten eines Geschosses aus dem Modell brauchen, einmal für
/// alle Geschosse gesammelt (Review W33: sonst durchsucht jedes Geschoss
/// das ganze Modell): geschlossene Außenwandzüge und Öffnungen je Geschoss,
/// T-Anschlüsse (Nummer in [`Model::joins`]) je Wirtszug.
#[derive(Default)]
pub struct Verzeichnis {
    zuege: HashMap<StoreyId, Vec<RunId>>,
    oeffnungen: HashMap<StoreyId, Vec<ElementId>>,
    anschluesse: HashMap<RunId, Vec<usize>>,
}

impl Verzeichnis {
    /// Für alle Geschosse.
    pub fn neu(m: &Model) -> Verzeichnis {
        Verzeichnis::fuer(m, |_| true)
    }

    /// Nur für die Geschosse, für die `nur` gilt (beim Ziehen eines).
    pub fn fuer(m: &Model, nur: impl Fn(StoreyId) -> bool) -> Verzeichnis {
        let mut v = Verzeichnis::default();
        for (id, r) in m.runs().iter() {
            if !r.closed || !nur(r.storey) {
                continue;
            }
            let aussenwand = r
                .segments
                .first()
                .and_then(|&e| m.element(e))
                .is_some_and(|e| e.category == Category::ExteriorWall);
            if aussenwand {
                v.zuege.entry(r.storey).or_default().push(id);
            }
        }
        for (id, e) in m.elements().iter() {
            if matches!(e.kind, ElementKind::Opening(_)) && nur(e.storey) {
                v.oeffnungen.entry(e.storey).or_default().push(id);
            }
        }
        for (i, j) in m.joins().iter().enumerate() {
            if j.kind != JoinKind::T {
                continue;
            }
            let Some((host, _)) = m.segment_of(j.b) else {
                continue;
            };
            if m.run(host).is_some_and(|r| nur(r.storey)) {
                v.anschluesse.entry(host).or_default().push(i);
            }
        }
        v
    }
}

fn zuege<C: Borrow<WallChain>>(
    vz: &Verzeichnis,
    st: StoreyId,
    stil: &Stil,
    chain: &impl Fn(RunId) -> Option<C>,
) -> Vec<Zug> {
    let mut out = Vec::new();
    for &id in vz.zuege.get(&st).into_iter().flatten() {
        let Some(c) = chain(id) else { continue };
        let c = c.borrow();
        let (aussen, innen) = flaechen(c, stil.bezug);
        let bezug = c.face_corners(aussen);
        if bezug.len() < 3 || bezug.len() != c.segment_count() {
            continue;
        }
        let flaeche: f64 = (0..bezug.len())
            .map(|i| {
                let (a, b) = (bezug[i], bezug[(i + 1) % bezug.len()]);
                a.x * b.y - a.y * b.x
            })
            .sum();
        out.push(Zug {
            run: id,
            innen: c.face_corners(innen),
            haut: c.face_corners(c.outer_offset()),
            bezug,
            ccw: flaeche > 0.0,
        });
    }
    out
}

/// Hauptrichtung: die längste Außenwand, gedreht in (−45°, 45°].
fn hauptrichtung(zuege: &[Zug]) -> Vec3 {
    let mut best = (0.0, vec3(1.0, 0.0, 0.0));
    for z in zuege {
        for i in 0..z.n() {
            let (a, b) = z.kante(i);
            let l = (b - a).length();
            if l > best.0 + 1e-6 {
                best = (l, (b - a) * (1.0 / l));
            }
        }
    }
    let d = best.1;
    let w = d.y.atan2(d.x);
    let q = std::f64::consts::FRAC_PI_2;
    let mut w = w - (w / q).round() * q;
    if w <= -q / 2.0 + 1e-12 {
        w += q;
    }
    vec3(w.cos(), w.sin(), 0.0)
}

/// Richtung längs einer Seite mit Normale `n`: Maßzahlen von unten oder von
/// rechts lesbar. Auch für die Kette der Innentür (Bemaßung zu #146, H2).
pub fn laengs(n: Vec3) -> Vec3 {
    let t = vec3(-n.y, n.x, 0.0);
    if t.x > 1e-9 || (t.x.abs() <= 1e-9 && t.y > 0.0) {
        t
    } else {
        -t
    }
}

/// Ein sichtbares Stück einer Seite: zusammenhängend, in einer Flucht.
#[derive(Clone, Debug)]
struct Stueck {
    t0: f64,
    t1: f64,
    d: f64,
    /// Beteiligte Kanten (Zug, Kante), von `t0` nach `t1`.
    kanten: Vec<(usize, usize)>,
}

/// Lage einer Kante längs und quer: (t0, d0, t1, d1).
fn projektion(a: Vec3, b: Vec3, n: Vec3, t: Vec3) -> (f64, f64, f64, f64) {
    (a.dot(t), a.dot(n), b.dot(t), b.dot(n))
}

/// Die vordersten Flächen der Seite `n`, nach `t` geordnet.
fn sichtbar(zuege: &[Zug], n: Vec3, t: Vec3) -> Vec<Stueck> {
    let mut ts: Vec<f64> = zuege
        .iter()
        .flat_map(|z| z.bezug.iter().map(move |p| p.dot(t)))
        .collect();
    ts.sort_by(f64::total_cmp);
    ts.dedup_by(|a, b| (*a - *b).abs() < GLEICH);
    let mut stuecke: Vec<Stueck> = Vec::new();
    for w in ts.windows(2) {
        let (ta, tb) = (w[0], w[1]);
        let tm = 0.5 * (ta + tb);
        // Vorderste Kante über tm
        let mut vorn: Option<(f64, Option<(usize, usize)>)> = None;
        for (zi, z) in zuege.iter().enumerate() {
            for i in 0..z.n() {
                let (a, b) = z.kante(i);
                let (t0, d0, t1, d1) = projektion(a, b, n, t);
                if (t1 - t0).abs() < 1e-6 || tm < t0.min(t1) || tm > t0.max(t1) {
                    continue;
                }
                let d = d0 + (d1 - d0) * (tm - t0) / (t1 - t0);
                let zeigt = z.normale(i).dot(n) > PARALLEL;
                if vorn.is_none_or(|(dv, _)| d > dv + 1e-6) {
                    vorn = Some((d, zeigt.then_some((zi, i))));
                }
            }
        }
        let Some((d, Some(k))) = vorn else { continue };
        match stuecke.last_mut() {
            Some(s) if (s.t1 - ta).abs() < GLEICH && (s.d - d).abs() < GLEICH => {
                s.t1 = tb;
                if s.kanten.last() != Some(&k) {
                    s.kanten.push(k);
                }
            }
            _ => stuecke.push(Stueck {
                t0: ta,
                t1: tb,
                d,
                kanten: vec![k],
            }),
        }
    }
    stuecke
}

/// Ecke am Ende eines Stücks: Punkt, und die quer stehende Nachbarkante
/// (Zug, Kante), wenn das Ende an einer echten Ecke liegt.
fn ecke(
    zuege: &[Zug],
    k: (usize, usize),
    at: f64,
    n: Vec3,
    t: Vec3,
) -> (Vec3, Option<(usize, usize)>) {
    let z = &zuege[k.0];
    let (a, b) = z.kante(k.1);
    let m = z.n();
    let quer = |j: usize| z.richtung(j).dot(n).abs() > PARALLEL;
    if (a.dot(t) - at).abs() < GLEICH {
        let j = (k.1 + m - 1) % m;
        return (a, quer(j).then_some((k.0, j)));
    }
    if (b.dot(t) - at).abs() < GLEICH {
        let j = (k.1 + 1) % m;
        return (b, quer(j).then_some((k.0, j)));
    }
    // Teilweise verdeckt: Punkt auf der Kante
    let (t0, t1) = (a.dot(t), b.dot(t));
    let s = ((at - t0) / (t1 - t0)).clamp(0.0, 1.0);
    (a + (b - a) * s, None)
}

/// Fügt einen Bezugspunkt an; ein Punkt am selben `t` bleibt, wenn er
/// zuerst kam (Ecken vor allem anderen).
fn dazu(v: &mut Vec<Bezug>, b: Bezug) {
    if !v.iter().any(|x| (x.t - b.t).abs() < GLEICH) {
        v.push(b);
    }
}

fn sortiert(mut v: Vec<Bezug>) -> Vec<Bezug> {
    v.sort_by(|a, b| a.t.total_cmp(&b.t));
    v
}

/// Ecken aller Stücke der Seite.
fn ecken(
    zuege: &[Zug],
    stuecke: &[Stueck],
    n: Vec3,
    t: Vec3,
) -> Vec<(Bezug, Option<(usize, usize)>)> {
    let mut out: Vec<(Bezug, Option<(usize, usize)>)> = Vec::new();
    for s in stuecke {
        let (k0, k1) = (s.kanten[0], *s.kanten.last().unwrap());
        for (k, at) in [(k0, s.t0), (k1, s.t1)] {
            let (p, quer) = ecke(zuege, k, at, n, t);
            if out.iter().any(|(x, _)| (x.t - at).abs() < GLEICH) {
                continue;
            }
            out.push((
                Bezug {
                    t: at,
                    p,
                    herkunft: Herkunft::Ecke,
                    quelle: quer.map(|(zi, j)| Quelle::Wand {
                        run: zuege[zi].run,
                        seg: j,
                    }),
                },
                quer,
            ));
        }
    }
    out
}

/// Stück, das die Stelle `at` abdeckt.
fn stueck_bei(stuecke: &[Stueck], at: f64) -> Option<&Stueck> {
    stuecke
        .iter()
        .find(|s| at >= s.t0 - GLEICH && at <= s.t1 + GLEICH)
}

/// Eine angeschlossene Innenwand am Außenwandzug `zi`, Segment `seg`:
/// Endpunkt, Richtung der Wand, Richtung aus der Wand heraus, gemessene
/// Flächen ([`flaechen`]).
struct Anschluss {
    run: RunId,
    zi: usize,
    seg: usize,
    p: Vec3,
    d: Vec3,
    raus: Vec3,
    flaechen: (f64, f64),
}

/// T-Anschlüsse von Innenwänden an die Außenwandzüge, einmal je Geschoss
/// (Review W30).
fn anschluesse<C: Borrow<WallChain>>(
    m: &Model,
    vz: &Verzeichnis,
    stil: &Stil,
    zuege: &[Zug],
    chain: &impl Fn(RunId) -> Option<C>,
) -> Vec<Anschluss> {
    let mut out = Vec::new();
    let joins = zuege
        .iter()
        .flat_map(|z| vz.anschluesse.get(&z.run).into_iter().flatten())
        .filter_map(|&i| m.joins().get(i));
    for j in joins {
        let Some((host, seg)) = m.segment_of(j.b) else {
            continue;
        };
        let Some(zi) = zuege.iter().position(|z| z.run == host) else {
            continue;
        };
        if j.a_run == host {
            continue;
        }
        let Some(c) = chain(j.a_run) else { continue };
        let c = c.borrow();
        let pts = c.clean_points();
        if pts.len() < 2 {
            continue;
        }
        let (p, d, raus) = match j.a_end {
            JoinEnd::Start => {
                let d = (pts[1] - pts[0]).normalized();
                (pts[0], d, -d)
            }
            JoinEnd::End => {
                let k = pts.len() - 1;
                let d = (pts[k] - pts[k - 1]).normalized();
                (pts[k], d, d)
            }
        };
        out.push(Anschluss {
            run: j.a_run,
            zi,
            seg,
            p,
            d,
            raus,
            flaechen: flaechen(c, stil.bezug),
        });
    }
    out
}

/// Seiten der angeschlossenen Innenwände (Flächen wie bei den Außenwänden),
/// die von innen an eine sichtbare Fläche der Seite stoßen.
fn innenwaende(
    anschluesse: &[Anschluss],
    zuege: &[Zug],
    stuecke: &[Stueck],
    n: Vec3,
    t: Vec3,
) -> Vec<Bezug> {
    let mut out = Vec::new();
    for a in anschluesse {
        // Die Wand läuft von innen auf die Fläche zu
        if a.raus.dot(n) < PARALLEL {
            continue;
        }
        let (zi, seg) = (a.zi, a.seg);
        let z = &zuege[zi];
        let innen_d = 0.5 * (z.innen[seg] + z.innen[(seg + 1) % z.n()]).dot(n);
        let r = vec3(a.d.y, -a.d.x, 0.0);
        let (ka, ki) = a.flaechen;
        for off in [ka, ki] {
            let at = (a.p + r * off).dot(t);
            let Some(s) = stueck_bei(stuecke, at) else {
                continue;
            };
            if !s.kanten.contains(&(zi, seg)) {
                continue;
            }
            dazu(
                &mut out,
                Bezug {
                    t: at,
                    p: t * at + n * innen_d,
                    herkunft: Herkunft::Innenwand,
                    quelle: Some(Quelle::Innenwand { run: a.run }),
                },
            );
        }
    }
    out
}

/// Eine Öffnung des Geschosses: Pfad im Grundriss, Band (UK, OK absolut).
struct Oeffnung {
    id: ElementId,
    pfad: Vec<Vec3>,
    band: (f64, f64),
}

/// Öffnungen des Geschosses `st`, einmal je Geschoss (Review W30).
fn oeffnungen_von(m: &Model, vz: &Verzeichnis, st: StoreyId) -> Vec<Oeffnung> {
    vz.oeffnungen
        .get(&st)
        .into_iter()
        .flatten()
        .filter_map(|&id| {
            let e = m.element(id)?;
            let ElementKind::Opening(o) = &e.kind else {
                return None;
            };
            let band = m.opening_band(o)?;
            let pfad = o.path.iter().map(|p| vec3(p.x, p.y, 0.0)).collect();
            Some(Oeffnung { id, pfad, band })
        })
        .collect()
}

/// Außenhaut der Kante (Zug `zi`, Kante `i`) quer (`p · n`) an der Stelle
/// `at` längs: auf der Kante gemessen, nicht an ihrer Mitte, damit auch
/// eine bis 1° schräge Wand ihre Öffnungen behält (Geometrie zu #116).
fn haut_bei(z: &Zug, i: usize, at: f64, n: Vec3, t: Vec3) -> f64 {
    let (a, b) = (z.haut[i], z.haut[(i + 1) % z.n()]);
    let (t0, t1) = (a.dot(t), b.dot(t));
    let (d0, d1) = (a.dot(n), b.dot(n));
    if (t1 - t0).abs() < 1e-9 {
        return 0.5 * (d0 + d1);
    }
    d0 + (d1 - d0) * (at - t0) / (t1 - t0)
}

/// Öffnungen in den sichtbaren Flächen der Seite: Kanten und Maße.
#[allow(clippy::too_many_arguments)]
fn oeffnungen(
    m: &Model,
    st: StoreyId,
    stil: &Stil,
    alle: &[Oeffnung],
    zuege: &[Zug],
    stuecke: &[Stueck],
    n: Vec3,
    t: Vec3,
) -> (Vec<Bezug>, Vec<Oeffnungsmass>) {
    let (mut kanten, mut masse) = (Vec::new(), Vec::new());
    let Some(storey) = m.storey(st) else {
        return (kanten, masse);
    };
    let boden = storey.elevation;
    let brh_null = match stil.brh_bezug {
        BrhBezug::Rohdecke => boden,
        BrhBezug::Estrich => m.screed_top(st).unwrap_or(boden),
    };
    for o in alle {
        let (id, (z0, z1)) = (o.id, o.band);
        for w in o.pfad.windows(2) {
            let (a, b) = (w[0], w[1]);
            let l = (b - a).length();
            if l < 1.0 || ((b - a) * (1.0 / l)).dot(t).abs() < PARALLEL {
                continue;
            }
            let (ta, tb) = (a.dot(t), b.dot(t));
            let (t0, t1) = (ta.min(tb), ta.max(tb));
            let tm = 0.5 * (t0 + t1);
            // Liegt das Stück auf der Außenhaut einer sichtbaren Fläche?
            let Some(s) = stueck_bei(stuecke, tm) else {
                continue;
            };
            let haut = s.kanten.iter().find_map(|&(zi, i)| {
                let z = &zuege[zi];
                let auf = (0..2).all(|k| {
                    let p = w[k];
                    (p.dot(n) - haut_bei(z, i, p.dot(t), n, t)).abs() < AUF_FLAECHE
                });
                auf.then(|| haut_bei(z, i, tm, n, t))
            });
            let Some(wand) = haut else { continue };
            for (ende, at) in [(0, t0), (1, t1)] {
                kanten.push(Bezug {
                    t: at,
                    p: t * at + n * s.d,
                    herkunft: Herkunft::Oeffnung,
                    quelle: Some(Quelle::Oeffnung { id, ende }),
                });
            }
            masse.push(Oeffnungsmass {
                id,
                t0,
                t1,
                hoehe: z1 - z0,
                brh: (z0 > boden + 1.0).then_some(z0 - brh_null),
                wand,
            });
        }
    }
    masse.sort_by(|a, b| a.t0.total_cmp(&b.t0));
    (kanten, masse)
}

/// Die Außenmaßketten des Geschosses `st`; `None` ohne geschlossene
/// Außenwand oder in der Gründung.
pub fn aussenmasse(m: &Model, st: StoreyId) -> Option<Aussenmasse> {
    aussenmasse_aus(m, st, |id| m.chain(id))
}

/// Wie [`aussenmasse`], die Geometrie der Züge aus `chain` (etwa dem
/// Zwischenspeicher der App, Review W30), statt sie je Zug neu zu rechnen.
pub fn aussenmasse_aus<C: Borrow<WallChain>>(
    m: &Model,
    st: StoreyId,
    chain: impl Fn(RunId) -> Option<C>,
) -> Option<Aussenmasse> {
    aussenmasse_mit(m, &Verzeichnis::fuer(m, |s| s == st), st, chain)
}

/// Wie [`aussenmasse_aus`], Züge, Öffnungen und Anschlüsse aus `vz` (für
/// viele Geschosse einmal gesammelt).
pub fn aussenmasse_mit<C: Borrow<WallChain>>(
    m: &Model,
    vz: &Verzeichnis,
    st: StoreyId,
    chain: impl Fn(RunId) -> Option<C>,
) -> Option<Aussenmasse> {
    let storey = m.storey(st)?;
    if storey.kind == LevelKind::Foundation {
        return None;
    }
    let stil = m.massstil();
    let zuege = zuege(vz, st, stil, &chain);
    if zuege.is_empty() {
        return None;
    }
    let anschluesse = anschluesse(m, vz, stil, &zuege, &chain);
    let alle_oeffnungen = oeffnungen_von(m, vz, st);
    let e1 = hauptrichtung(&zuege);
    let e2 = vec3(-e1.y, e1.x, 0.0);
    let mut seiten = Vec::new();
    for n in [-e2, e1, e2, -e1] {
        let t = laengs(n);
        let stuecke = sichtbar(&zuege, n, t);
        if stuecke.is_empty() {
            continue;
        }
        let grad = seite_grad(n.x, n.y);
        let aussen = zuege
            .iter()
            .flat_map(|z| z.bezug.iter().map(|p| p.dot(n)))
            .fold(f64::MIN, f64::max);
        let ecken = ecken(&zuege, &stuecke, n, t);
        let nur_ecken: Vec<Bezug> = ecken.iter().map(|(b, _)| *b).collect();
        let (kanten, masse) = oeffnungen(m, st, stil, &alle_oeffnungen, &zuege, &stuecke, n, t);

        let mut ketten: Vec<(Kettenart, Vec<Bezug>)> = Vec::new();
        // 1. Öffnungen
        if !masse.is_empty() {
            let mut v = nur_ecken.clone();
            for k in &kanten {
                dazu(&mut v, *k);
            }
            ketten.push((Kettenart::Oeffnung, sortiert(v)));
        }
        // 2. Wände: Ecken, Innenseiten der quer stehenden Außenwände,
        // angeschlossene Innenwände
        let mut v = nur_ecken.clone();
        for (b, quer) in &ecken {
            let Some((zi, j)) = *quer else { continue };
            let z = &zuege[zi];
            let (p, q) = (z.innen[j], z.innen[(j + 1) % z.n()]);
            // Der Punkt an der Ecke dieser Seite
            let nah = if (p - b.p).length() <= (q - b.p).length() {
                p
            } else {
                q
            };
            dazu(
                &mut v,
                Bezug {
                    t: nah.dot(t),
                    p: nah,
                    herkunft: Herkunft::Innenseite,
                    quelle: b.quelle,
                },
            );
        }
        for b in innenwaende(&anschluesse, &zuege, &stuecke, n, t) {
            dazu(&mut v, b);
        }
        ketten.push((Kettenart::Wand, sortiert(v)));
        // 3. Vor- und Rücksprünge, nur wenn es sie gibt
        let mut tiefen: Vec<f64> = stuecke.iter().map(|s| s.d).collect();
        tiefen.sort_by(f64::total_cmp);
        tiefen.dedup_by(|a, b| (*a - *b).abs() < GLEICH);
        if tiefen.len() > 1 {
            ketten.push((Kettenart::Sprung, sortiert(nur_ecken.clone())));
        }
        // 4. Gesamtmaß
        let ende = sortiert(nur_ecken);
        if let (Some(a), Some(b)) = (ende.first(), ende.last()) {
            ketten.push((Kettenart::Gesamt, vec![*a, *b]));
        }

        let mut out = Vec::new();
        let mut slot = 0.0;
        for (art, bezuege) in ketten {
            if bezuege.len() < 2 || !stil.kette(art) {
                continue;
            }
            let versatz = m.massversatz(st, grad, art);
            out.push(Kette {
                art,
                bezuege,
                lage: aussen + stil.erste + slot * stil.abstand + versatz,
                versatz,
            });
            slot += 1.0;
        }
        seiten.push(Seite {
            grad,
            n,
            t,
            aussen,
            ketten: out,
            oeffnungen: if stil.kette(Kettenart::Oeffnung) {
                masse
            } else {
                Vec::new()
            },
        });
    }
    Some(Aussenmasse {
        storey: st,
        z: storey.elevation,
        seiten,
    })
}

#[cfg(test)]
mod tests;
