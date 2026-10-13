//! Ausbau innen in der Szene (Raumbuch RB4, `raumbuch/plan.md` §8): die
//! Körper des Ausbaus je Raum ([`sk_raum::raumbuch::koerper`]). Den Estrich
//! zeigt der Schnitt immer (Jörn 11.10., 17:43); Innenputz, Wandbeläge,
//! Deckenputz, Bodenbelag und Sockelleisten nur, wo der Schalter „Ausbau
//! innen“ der Ansicht an ist ([`sk_model::FINISH_VIEWS`]). Grundriss und 3D
//! zeigen ohne Schalter nichts davon und rechnen dafür nichts; mit ihm zeigt
//! der Grundriss die Räume des aktiven Geschosses in seiner Schnitthöhe, 3D
//! den ganzen Ausbau samt Estrich. Der Schnitt zeigt den Ausbau der
//! Hausgruppen, durch die seine Ebene geht, je Raum geschnitten, ganz oder
//! nicht; eine Hausgruppe ganz hinter der Ebene zeigt er wie die Ansichten,
//! ohne Ausbau (Review #137 N2).
//!
//! Zwischenspeicher je Geschoss, Hausgruppe und Raum (Review #112 B1, #137
//! B1, N1, N2): Jeder Schritt meldet, was er berührt ([`RaumCache::merken`]);
//! dazu kommen die Züge, die die Szene neu gebaut hat. Angesehen wird nur
//! ein berührtes Geschoss, darin eine Hausgruppe der Erkennung nur, wenn
//! ihre Flächen, die Fassungen ihrer Züge oder ihre Räume anders sind.
//! Ihre Räume rechnen Rohbau und Ausbau erst, wenn eine Ansicht die
//! Hausgruppe zeigen könnte (ihr grober Quader, [`grob`]), und ihre Körper
//! nur, wenn sich das ändert, was sie bestimmt; ohne den Schalter nur die
//! des Estrichs ([`estrich`]), die übrigen erst, wenn eine Ansicht mit ihm
//! sie braucht. Typen, Raumvorlagen und Baustoffe lassen jeden Raum prüfen,
//! andere Höhen eines Gebäudes (Geschosse, Decken, Gelände) jeden Raum
//! darin. Das Netz hält jede Hausgruppe je Ansicht selbst, das volle Netz
//! hängt es nur an. Grundriss und Schnitt schneiden nur Räume, durch die die
//! Ebene geht; ein Grundriss in anderer Schnitthöhe verschiebt nur die
//! Schnittkanten, solange keine Höhe der Körper dazwischen liegt (N1). Ein
//! Raum ohne gespeicherten Raum (Datei vor G3, vor dem ersten Wandschritt)
//! zeigt keinen Ausbau.

use super::{class_in, mesh_into, union, Aabb, Class, Plane, Scene, Vis};
use crate::scene::MeshData;
use crate::ui::ViewKind;
use sk_math::vec3;
use sk_model::{
    BuildingId, Change, ElementId, ElementKind, LayerSet, LevelKind, Model, RunId, Solid, StoreyId,
};
use sk_raum::ausbau::RaumAusbau;
use sk_raum::oeffnungen::Rohbau;
use sk_raum::raumbuch::koerper::{estrich, koerper, Koerper};
use sk_raum::raumbuch::Flaeche;
use sk_raum::{Begrenzung, Ketten, Raumflaeche};
use std::collections::{HashMap, HashSet};
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

/// Wie eine Ansicht den Ausbau sieht.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Sicht {
    /// 3D: die ganzen Körper.
    Voll,
    /// Grundriss in Schnitthöhe.
    Plan(f64),
    /// Schnitt an der Ebene.
    Schnitt(Plane),
}

impl Sicht {
    /// Ebene (Punkt, Normale), an der geschnitten wird; es bleibt, was
    /// dahinter liegt.
    fn ebene(self) -> Option<Plane> {
        match self {
            Sicht::Voll => None,
            Sicht::Plan(z) => Some((vec3(0.0, 0.0, z), vec3(0.0, 0.0, 1.0))),
            Sicht::Schnitt(p) => Some(p),
        }
    }
}

/// Lage eines Quaders zur Ebene einer Sicht.
#[derive(Clone, Copy, PartialEq)]
enum Lage {
    /// Ganz dahinter oder ohne Ebene: ganz zu sehen.
    Ganz,
    /// Ganz davor (oder leer): weggeschnitten.
    Weg,
    Geschnitten,
}

fn lage(k: Option<Aabb>, sicht: Sicht) -> Lage {
    let Some(e) = sicht.ebene() else {
        return Lage::Ganz;
    };
    let Some((lo, hi)) = k else {
        return Lage::Weg;
    };
    let (mut min, mut max) = (f64::INFINITY, f64::NEG_INFINITY);
    for i in 0..8 {
        let p = vec3(
            if i & 1 == 0 { lo.x } else { hi.x },
            if i & 2 == 0 { lo.y } else { hi.y },
            if i & 4 == 0 { lo.z } else { hi.z },
        );
        let d = (p - e.0).dot(e.1);
        min = min.min(d);
        max = max.max(d);
    }
    if max < 0.0 {
        Lage::Ganz
    } else if min > 0.0 {
        Lage::Weg
    } else {
        Lage::Geschnitten
    }
}

/// Was eine Hausgruppe in der Sicht zeigt, wenn ihr Ausbau in Lage `l`
/// liegt: ganz davor nichts; ganz dahinter im Grundriss alles wie in 3D, im
/// Schnitt nichts (wie die Ansichten); sonst geschnitten.
fn zeigt(l: Lage, sicht: Sicht) -> Option<Sicht> {
    match l {
        Lage::Weg => None,
        Lage::Ganz if matches!(sicht, Sicht::Schnitt(_)) => None,
        Lage::Ganz => Some(Sicht::Voll),
        Lage::Geschnitten => Some(sicht),
    }
}

/// So viele Schnitte behält ein Raum und so viele Netze eine Hausgruppe.
const KEEP: usize = 3;

/// So weit (mm) bleibt ein Grundrissschnitt, der sich verschieben lässt,
/// von jeder Höhe der Körper und vom Gelände.
const ABSTAND: f64 = 1.0;

/// So genau (mm) erkennt [`heben`] die Ecken in Schnitthöhe (f32).
const HEBEN: f32 = 0.05;

/// Ein gerechneter Raum: Was ihn bestimmt, seine Körper, ganz und
/// geschnitten.
struct Gerechnet {
    schluessel: Schluessel,
    /// Mit `alle` alle Körper, sonst nur die des Estrichs, bis eine Ansicht
    /// mit dem Schalter sie braucht.
    koerper: Vec<Koerper>,
    alle: bool,
    voll: Paar,
    /// Quader um den Estrich und um alles.
    kasten: [Option<Aabb>; 2],
    /// Geschnittene Körper (Estrich, Rest, je erst bei Bedarf) je Sicht,
    /// zuletzt gefragte zuerst.
    schnitte: Vec<(Sicht, [Option<Solid>; 2])>,
}

impl Gerechnet {
    /// Aus den Körpern `koerper` (mit `alle` allen).
    fn neu(schluessel: Schluessel, koerper: Vec<Koerper>, alle: bool) -> Gerechnet {
        let voll = [0, 1].map(|j| teil(&koerper, j, |x| x.solid.clone()));
        let e = voll[0].bounds();
        Gerechnet {
            kasten: [e, union(e, voll[1].bounds())],
            voll,
            koerper,
            alle,
            schluessel,
            schnitte: Vec::new(),
        }
    }

    /// Was der Raum in der Sicht zeigt: Estrich und mit `alles` den übrigen
    /// Ausbau; `n` zählt die Schnitte (Tests).
    fn zeigt(
        &mut self,
        sicht: Sicht,
        alles: bool,
        n: &mut u64,
    ) -> Option<(&Solid, Option<&Solid>)> {
        match lage(self.kasten[usize::from(alles)], sicht) {
            Lage::Weg => return None,
            Lage::Ganz => return Some((&self.voll[0], alles.then_some(&self.voll[1]))),
            Lage::Geschnitten => {}
        }
        let (p0, nv) = sicht.ebene()?;
        let mut s = match self.schnitte.iter().position(|s| s.0 == sicht) {
            Some(i) => self.schnitte.remove(i),
            None => {
                self.schnitte.truncate(KEEP - 1);
                (sicht, [None, None])
            }
        };
        let mut neu = false;
        for (j, t) in s.1.iter_mut().enumerate().take(1 + usize::from(alles)) {
            if t.is_none() {
                *t = Some(teil(&self.koerper, j, |k| k.geschnitten(p0, nv)));
                neu = true;
            }
        }
        *n += u64::from(neu);
        self.schnitte.insert(0, s);
        let [e, rest] = &self.schnitte[0].1;
        Some((e.as_ref()?, rest.as_ref().filter(|_| alles)))
    }
}

/// Ein Raum der Hausgruppe; gerechnet, sobald eine Ansicht ihn braucht.
struct Eintrag {
    id: ElementId,
    storey: StoreyId,
    /// Platz seiner Fläche in der Hausgruppe.
    i: usize,
    /// Nach einer Änderung bis zur Prüfung der alte Stand.
    raum: Option<Gerechnet>,
}

impl Eintrag {
    /// Rechnet Rohbau und Ausbau des Raums, die Körper (mit `alles` alle,
    /// sonst die des Estrichs) nur, wenn sich ändert, was sie bestimmt;
    /// `true`: andere Körper (oder keine mehr).
    fn pruefen(
        &mut self,
        q: &Quelle<impl Ketten>,
        f: &Raumflaeche,
        alles: bool,
        z: &mut Zaehler,
    ) -> bool {
        z.geprueft += 1;
        let roh = Rohbau::mit(q.m, q.k, f.form.clone());
        let Some(aus) = RaumAusbau::fuer(q.m, &roh) else {
            return self.raum.take().is_some();
        };
        let s = Schluessel {
            typen: typen(q.m, &aus),
            roh,
            aus,
        };
        if self.raum.as_ref().is_some_and(|r| r.schluessel == s) {
            return false;
        }
        let k = if alles {
            koerper(q.m, &s.roh, &s.aus)
        } else {
            estrich(q.m, &s.roh, &s.aus)
        };
        z.builds += 1;
        self.raum = Some(Gerechnet::neu(s, k, alles));
        true
    }

    /// Rechnet zum Estrich die übrigen Körper; der Estrich und seine
    /// Schnitte bleiben. `true`: gerechnet.
    fn vervollstaendigen(&mut self, m: &Model, z: &mut Zaehler) -> bool {
        let Some(r) = self.raum.take_if(|r| !r.alle) else {
            return false;
        };
        let k = koerper(m, &r.schluessel.roh, &r.schluessel.aus);
        z.builds += 1;
        let mut neu = Gerechnet::neu(r.schluessel, k, true);
        let s = r.schnitte.into_iter();
        neu.schnitte = s.map(|(s, [e, _])| (s, [e, None])).collect();
        self.raum = Some(neu);
        true
    }
}

/// Netz einer Hausgruppe je Sicht und Schalter; ein Grundriss mit der
/// Spanne der Schnitthöhen, in der er sich nur in z verschiebt.
struct Netz {
    sicht: Sicht,
    alles: bool,
    mesh: MeshData,
    spanne: Option<(f64, f64)>,
}

/// Was eine geänderte Hausgruppe vorher hatte: ihre Räume, Netze und OK
/// Gelände dazu.
type Vorher = (Vec<ElementId>, Vec<Netz>, Option<f64>);

/// Modell und berechnete Wandzüge der Szene, aus denen ein Raum rechnet.
struct Quelle<'a, K> {
    m: &'a Model,
    k: &'a K,
}

/// Wie oft gerechnet wurde (Tests).
#[derive(Default)]
struct Zaehler {
    /// Körper eines Raums.
    builds: u64,
    /// Rohbau und Ausbau eines Raums.
    geprueft: u64,
    /// Netz einer Hausgruppe, Schnitt eines Raums.
    netze: (u64, u64),
}

/// Eine Hausgruppe, wie die Erkennung sie gibt (dieselben Flächen, solange
/// sie aus ihrem Speicher kommen), mit den Wandzügen an ihren Räumen und
/// deren Fassung, als die Räume zuletzt prüften.
struct Gruppe {
    flaechen: Rc<[Raumflaeche]>,
    zuege: Vec<(RunId, Option<u64>)>,
    items: Vec<Eintrag>,
    /// Quader um den Ausbau: grob ([`grob`]) und gerechnet um den Estrich
    /// und um alles.
    grob: Option<Aabb>,
    kasten: [Option<Aabb>; 2],
    /// Die Räume sind seit der Änderung noch nicht geprüft.
    offen: bool,
    /// Alle Räume haben alle Körper.
    alle: bool,
    /// Netze je Sicht und Schalter für OK Gelände `boden`, zuletzt
    /// gefragte zuerst.
    netze: Vec<Netz>,
    boden: Option<f64>,
}

impl Gruppe {
    /// Ist die Gruppe noch, wie ihre Räume zuletzt prüften (dieselben
    /// Flächen vorausgesetzt)? Ungeprüft bleibt sie, wie sie ist, und prüft
    /// gegen das Modell, sobald eine Ansicht sie zeigt.
    fn gleich(&self, k: &impl Ketten, b: &Beruehrt) -> bool {
        self.zuege
            .iter()
            .all(|&(run, f)| f.is_some() && k.fassung(run) == f)
            && !self
                .flaechen
                .iter()
                .filter_map(|f| f.form.raum)
                .any(|r| b.raeume.contains(&r))
    }

    /// Prüft die Räume nach einer Änderung; andere Körper verwerfen die
    /// Netze. Mit `alles` rechnen sie alle Körper, wo erst der Estrich ist.
    /// Im offenen Schritt bleibt der alte Stand und folgt beim Abschluss.
    fn pruefen(&mut self, q: &Quelle<impl Ketten>, alles: bool, z: &mut Zaehler) {
        if q.m.in_step() || !self.offen && (self.alle || !alles) {
            return;
        }
        if self.offen {
            self.offen = false;
            let mut anders = false;
            for x in &mut self.items {
                anders |= x.pruefen(q, &self.flaechen[x.i], alles, z);
            }
            if anders {
                self.netze.clear();
            }
            let zuege = zuege_der(q.m, &self.flaechen).into_iter();
            self.zuege = zuege.map(|r| (r, q.k.fassung(r))).collect();
        }
        // Netze mit allem stammen dann aus einem offenen Schritt, nur mit
        // dem Estrich
        let mut neu = false;
        for x in self.items.iter_mut().filter(|_| alles) {
            neu |= x.vervollstaendigen(q.m, z);
        }
        if neu {
            self.netze.retain(|n| !n.alles);
        }
        let r = self.items.iter().filter_map(|x| x.raum.as_ref());
        self.alle = r.clone().all(|r| r.alle);
        self.kasten = [0, 1].map(|j| r.clone().fold(None, |a, r| union(a, r.kasten[j])));
    }

    /// Wie die Gruppe in der Sicht zu sehen ist, mit `alles` samt übrigem
    /// Ausbau ([`zeigt`]); ihre Räume rechnen erst, wenn der grobe Quader
    /// sie zeigen könnte.
    fn sicht(
        &mut self,
        q: &Quelle<impl Ketten>,
        (sicht, alles): (Sicht, bool),
        z: &mut Zaehler,
    ) -> Option<Sicht> {
        zeigt(lage(self.grob, sicht), sicht)?;
        self.pruefen(q, alles, z);
        zeigt(lage(self.kasten[usize::from(alles)], sicht), sicht)
    }

    /// Netz der Gruppe in der Sicht, mit `alles` samt übrigem Ausbau.
    fn netz(
        &mut self,
        q: &Quelle<impl Ketten>,
        (sicht, alles): (Sicht, bool),
        boden: f64,
        z: &mut Zaehler,
    ) -> Option<&MeshData> {
        let sicht = self.sicht(q, (sicht, alles), z)?;
        if self.boden != Some(boden) {
            self.netze.clear();
            self.boden = Some(boden);
        }
        let i = self
            .netze
            .iter()
            .position(|n| n.sicht == sicht && n.alles == alles)
            .or_else(|| self.verschoben(sicht, alles));
        let n = match i {
            Some(i) => self.netze.remove(i),
            None => {
                self.netze.truncate(KEEP - 1);
                self.bauen(sicht, alles, boden, z)
            }
        };
        self.netze.insert(0, n);
        Some(&self.netze[0].mesh)
    }

    /// Grundriss: ein Netz in anderer Schnitthöhe, wenn zwischen beiden
    /// keine Höhe der Körper und nicht das Gelände liegt. Die Körper sind
    /// senkrechte Prismen, es unterscheidet sich nur in den Schnittkanten
    /// (Review #137 N1).
    fn verschoben(&mut self, sicht: Sicht, alles: bool) -> Option<usize> {
        let Sicht::Plan(nach) = sicht else {
            return None;
        };
        let i = self.netze.iter().position(|n| {
            n.alles == alles
                && n.spanne
                    .is_some_and(|(lo, hi)| lo + ABSTAND < nach && nach < hi - ABSTAND)
        })?;
        let n = &mut self.netze[i];
        let Sicht::Plan(von) = n.sicht else {
            return None;
        };
        heben(&mut n.mesh, von, nach);
        n.sicht = sicht;
        Some(i)
    }

    /// Rechnet das Netz der Sicht aus den Räumen.
    fn bauen(&mut self, sicht: Sicht, alles: bool, boden: f64, z: &mut Zaehler) -> Netz {
        let mut m = MeshData::default();
        let mut stufen = vec![boden];
        for r in self.items.iter_mut().filter_map(|x| x.raum.as_mut()) {
            if let Sicht::Plan(_) = sicht {
                let p = r.koerper.iter().flat_map(|k| &k.prismen);
                stufen.extend(p.flat_map(|p| [p.z.0, p.z.1]));
            }
            if let Some((e, rest)) = r.zeigt(sicht, alles, &mut z.netze.1) {
                mesh_into(&mut m, e, boden);
                if let Some(rest) = rest {
                    mesh_into(&mut m, rest, boden);
                }
            }
        }
        z.netze.0 += 1;
        let spanne = match sicht {
            Sicht::Plan(h) => {
                von_oben(&mut m);
                spanne(&stufen, h)
            }
            _ => None,
        };
        Netz {
            sicht,
            alles,
            mesh: m,
            spanne,
        }
    }
}

/// Nächste Höhe aus `stufen` unter und über `h`, wenn beide mindestens
/// [`ABSTAND`] entfernt sind.
fn spanne(stufen: &[f64], h: f64) -> Option<(f64, f64)> {
    let lo = stufen
        .iter()
        .filter(|&&s| s <= h)
        .fold(f64::NEG_INFINITY, |a, &s| a.max(s));
    let hi = stufen
        .iter()
        .filter(|&&s| s > h)
        .fold(f64::INFINITY, |a, &s| a.min(s));
    (h - lo > ABSTAND && hi - h > ABSTAND).then_some((lo, hi))
}

/// Nur die Flächen, die nach oben zeigen: Der Grundriss blickt senkrecht
/// von oben; eine Fläche zur Seite sieht er nicht, eine nach unten liegt
/// unter einer nach oben desselben Körpers.
fn von_oben(m: &mut MeshData) {
    let mut n = 0;
    for i in (0..m.faces.len()).step_by(3) {
        if m.faces[i][5] > 0.5 {
            m.faces.copy_within(i..i + 3, n);
            n += 3;
        }
    }
    m.faces.truncate(n);
}

/// Hebt die Ecken und Kanten in Höhe `von` auf `nach`.
fn heben(m: &mut MeshData, von: f64, nach: f64) {
    let (a, b) = (von as f32, nach as f32);
    let auf = |z: f32| (z - a).abs() < HEBEN;
    for f in &mut m.faces {
        if auf(f[2]) {
            f[2] = b;
        }
    }
    for p in m.edges.iter_mut().flat_map(|e| &mut e.0) {
        if auf(p[2]) {
            p[2] = b;
        }
    }
}

/// Grober Quader eines Raums, bevor er rechnet: der Umriss um `rand`
/// erweitert, in der Höhe das Geschoss `z` ebenso. Sein Ausbau liegt darin:
/// über OK Rohdecke, unter UK Decke darüber, in Öffnungen und Nischen nicht
/// tiefer, als die Wände dick sind.
fn grob(f: &Raumflaeche, z: (f64, f64), rand: f64) -> Option<Aabb> {
    let mut p = f.form.umriss.iter().map(|k| k.a);
    let a = p.next()?;
    let (lo, hi) = p.fold((a, a), |(lo, hi), p| {
        (
            vec3(lo.x.min(p.x), lo.y.min(p.y), 0.0),
            vec3(hi.x.max(p.x), hi.y.max(p.y), 0.0),
        )
    });
    Some((
        vec3(lo.x - rand, lo.y - rand, z.0 - rand),
        vec3(hi.x + rand, hi.y + rand, z.1 + rand),
    ))
}

/// Ein Geschoss mit seinen Hausgruppen.
struct Geschoss {
    id: StoreyId,
    gruppen: Vec<Gruppe>,
}

/// Was Schritte am Ausbau berühren können.
#[derive(Default)]
struct Beruehrt {
    /// Typen, Raumvorlagen oder Baustoffe: Jeder Raum prüft.
    alles: bool,
    /// Geschosse mit geänderten Zügen, Bauteilen oder Räumen.
    geschosse: HashSet<StoreyId>,
    /// Gebäude mit geänderten Geschossen, Decken oder Gelände: Ihre Höhen
    /// werden verglichen.
    gebaeude: HashSet<Option<BuildingId>>,
    /// Geänderte Räume.
    raeume: HashSet<ElementId>,
}

impl Beruehrt {
    fn leer(&self) -> bool {
        !self.alles
            && self.geschosse.is_empty()
            && self.gebaeude.is_empty()
            && self.raeume.is_empty()
    }

    /// Nimmt die Änderungen `cs` auf; je Datensatz zählen der Stand vorher,
    /// nachher und der im Modell (im offenen Schritt fehlt „nachher“).
    fn aus(&mut self, m: &Model, cs: &[Change]) {
        for c in cs {
            match c {
                Change::Run { id, old, new } => {
                    for r in old.iter().chain(new).chain(m.run(*id)) {
                        self.geschosse.insert(r.storey);
                    }
                }
                Change::Element { id, old, new } => {
                    for e in old.iter().chain(new).chain(m.element(*id)) {
                        self.geschosse.insert(e.storey);
                        match &e.kind {
                            ElementKind::Space(_) => {
                                self.raeume.insert(*id);
                            }
                            // Dickste Decke: lichte Höhe ([`Model::clear_height`])
                            ElementKind::Floor(f) => {
                                self.gebaeude.insert(m.building_of(e.storey));
                                self.gebaeude.insert(m.building_of(f.top.storey));
                            }
                            _ => {}
                        }
                    }
                }
                Change::Storey { id, old, new } => {
                    self.geschosse.insert(*id);
                    for s in old.iter().chain(new).chain(m.storey(*id)) {
                        self.gebaeude.insert(s.building);
                    }
                }
                Change::Building { id, .. } => {
                    self.gebaeude.insert(Some(*id));
                }
                Change::LayerSet { .. } | Change::Material { .. } | Change::SpaceTypes { .. } => {
                    self.alles = true;
                }
                Change::Pen { .. }
                | Change::LineType { .. }
                | Change::Fill { .. }
                | Change::Surface { .. }
                | Change::Display { .. }
                | Change::Defaults { .. }
                | Change::Trades { .. }
                | Change::ForeignRecords { .. }
                | Change::Ext { .. }
                | Change::ExtDefs { .. }
                | Change::Project { .. }
                | Change::Location { .. }
                | Change::Bemassung { .. } => {}
            }
        }
    }
}

/// Was die Höhen der Räume eines Gebäudes bestimmt: OK Gelände und je
/// Geschoss Art, Höhenlage, Höhe und dickste Decke darauf.
#[derive(Clone, PartialEq)]
struct Hoehen {
    gelaende: f64,
    geschosse: Vec<(StoreyId, LevelKind, f64, f64, Option<f64>)>,
}

/// Höhen der Gebäude `welche`, aus einem Gang über die Bauteile.
fn hoehen(m: &Model, welche: &HashSet<Option<BuildingId>>) -> HashMap<Option<BuildingId>, Hoehen> {
    let mut decke: HashMap<StoreyId, f64> = HashMap::new();
    for (_, e) in m.elements().iter() {
        if let ElementKind::Floor(f) = &e.kind {
            let d = decke.entry(f.top.storey).or_insert(f.thickness);
            *d = d.max(f.thickness);
        }
    }
    let mut out: HashMap<Option<BuildingId>, Hoehen> = HashMap::new();
    for (id, st) in m.storeys().iter() {
        if !welche.contains(&st.building) {
            continue;
        }
        let h = out.entry(st.building).or_insert_with(|| Hoehen {
            gelaende: m.terrain_z_of(st.building),
            geschosse: Vec::new(),
        });
        let d = decke.get(&id).copied();
        h.geschosse.push((id, st.kind, st.elevation, st.height, d));
    }
    out
}

#[derive(Default)]
pub(crate) struct RaumCache {
    /// Was die Schritte seit dem letzten Abgleich berührt haben.
    offen: Beruehrt,
    /// Modellstand beim letzten Abgleich; `None`: noch keiner.
    rev: Option<u64>,
    /// Je Platz der Züge in der Szene Zug, Bauzähler und Geschoss beim
    /// letzten Abgleich.
    zuege: Vec<Option<(RunId, u32, StoreyId)>>,
    hoehen: HashMap<Option<BuildingId>, Hoehen>,
    geschosse: Vec<Geschoss>,
    /// Schnitthöhe des letzten Grundrisses.
    plan: Option<f64>,
    z: Zaehler,
}

/// Estrich (`j` = 0) oder alles andere (1), aus den Körpern zusammengesetzt.
fn teil(k: &[Koerper], j: usize, f: impl Fn(&Koerper) -> Solid) -> Solid {
    let mut s = Solid::default();
    for x in k
        .iter()
        .filter(|x| usize::from(x.flaeche != Flaeche::Estrich) == j)
    {
        s.append(&f(x));
    }
    s
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

/// Schlüssel einer Hausgruppe: die Adresse ihrer Flächen.
fn zeiger(f: &Rc<[Raumflaeche]>) -> usize {
    Rc::as_ptr(f).cast::<()>() as usize
}

impl RaumCache {
    /// Nimmt auf, was ein Schritt geändert hat: beim Abschluss der
    /// geschlossene, bei Rückgängig und Wiederholen der angewandte. Ein
    /// verworfener Schritt ändert nichts, denn im offenen gleicht der
    /// Speicher nie ab ([`RaumCache::sync`]) und prüft kein Raum
    /// ([`Gruppe::pruefen`]).
    pub(super) fn merken(&mut self, m: &Model, cs: &[Change]) {
        self.offen.aus(m, cs);
    }

    /// Gleicht den Speicher mit dem Modell ab, die Flächen aus der
    /// Erkennung der Szene, die Züge aus ihren berechneten. Im offenen
    /// Schritt (Ziehen im Geschossmanager, einer Wand) bleibt der Ausbau,
    /// wie er war, und folgt beim Abschluss; vor dem ersten Abgleich zeigt
    /// er bis dahin nichts.
    fn sync(&mut self, s: &Scene) {
        let m = s.model();
        if m.in_step() || self.rev == Some(m.revision()) && self.offen.leer() {
            return;
        }
        let erst = self.rev.is_none();
        self.rev = Some(m.revision());
        let mut b = std::mem::take(&mut self.offen);
        self.neu_gebaut(s, &mut b);
        if !erst && b.leer() {
            return;
        }
        let alle = erst || b.alles;
        let hoch = self.hoehen_abgleichen(m, &b.gebaeude, alle);
        let k = s.ketten();
        // Grober Quader: Öffnungen und Nischen reichen höchstens durch zwei
        // Schalen, dazu der Anschlag
        let dicke = s.cache.iter().flatten().map(|c| c.chain.thickness());
        let rand = 3.0 * dicke.fold(0.0, f64::max) + 100.0;
        let mut alt: HashMap<StoreyId, Geschoss> = std::mem::take(&mut self.geschosse)
            .into_iter()
            .map(|g| (g.id, g))
            .collect();
        for (g, st) in m.storeys().iter() {
            if st.kind != LevelKind::Storey {
                continue;
            }
            let ganz = alle || hoch.contains(&st.building);
            let neu = match alt.remove(&g) {
                Some(x) if !ganz && !b.geschosse.contains(&g) => x,
                x => {
                    let z = (st.elevation, st.elevation + st.height);
                    geschoss(s, &k, (g, z, rand), &b, ganz, x)
                }
            };
            self.geschosse.push(neu);
        }
    }

    /// Die Geschosse der Züge, die die Szene seit dem letzten Abgleich neu
    /// gebaut oder entfernt hat (auch Nachbarn und gekoppelte Züge), und
    /// der noch nicht gebauten.
    fn neu_gebaut(&mut self, s: &Scene, b: &mut Beruehrt) {
        let m = s.model();
        if self.zuege.len() < s.cache.len() {
            self.zuege.resize(s.cache.len(), None);
        }
        for (i, alt) in self.zuege.iter_mut().enumerate() {
            let jetzt = s.cache.get(i).and_then(Option::as_ref);
            let jetzt = jetzt.map(|c| (c.id, c.builds));
            if jetzt == alt.map(|a| (a.0, a.1)) {
                continue;
            }
            b.geschosse.extend(alt.map(|a| a.2));
            *alt = jetzt.and_then(|(id, n)| Some((id, n, m.run(id)?.storey)));
            b.geschosse.extend(alt.map(|a| a.2));
        }
        if s.all_dirty {
            b.geschosse.extend(m.storeys().ids());
        }
        b.geschosse
            .extend(s.dirty.iter().filter_map(|&id| Some(m.run(id)?.storey)));
    }

    /// Vergleicht die Höhen der Gebäude `welche` (aller mit `alle`);
    /// Ergebnis: die Gebäude, deren Höhen anders sind.
    fn hoehen_abgleichen(
        &mut self,
        m: &Model,
        welche: &HashSet<Option<BuildingId>>,
        alle: bool,
    ) -> HashSet<Option<BuildingId>> {
        let welche: HashSet<Option<BuildingId>> = if alle {
            m.storeys().iter().map(|(_, s)| s.building).collect()
        } else if welche.is_empty() {
            return HashSet::new();
        } else {
            welche.clone()
        };
        let mut neu = hoehen(m, &welche);
        let mut anders = HashSet::new();
        for g in welche {
            let h = neu.remove(&g);
            if self.hoehen.get(&g) != h.as_ref() {
                anders.insert(g);
            }
            match h {
                Some(h) => self.hoehen.insert(g, h),
                None => self.hoehen.remove(&g),
            };
        }
        anders
    }

    /// Ausbau der Sicht ins Netz: ohne Filter die Netze der Hausgruppen
    /// angehängt (im Grundriss nur die des Geschosses `plan`), mit Filter
    /// je Raum deckend oder blass wie die Erweiterungen.
    fn anhaengen(
        &mut self,
        s: &Scene,
        m: &mut [MeshData; 2],
        plan: Option<StoreyId>,
        sicht: (Sicht, bool),
    ) {
        let model = s.model();
        let k = s.ketten();
        let q = Quelle { m: model, k: &k };
        let z = &mut self.z;
        let geschosse = self.geschosse.iter_mut().enumerate();
        let geschosse = geschosse.filter(|(_, g)| plan.is_none_or(|p| g.id == p));
        if !s.filtering() {
            // Erst die Netze, dann einmal Platz und anhängen
            let mut teile = Vec::new();
            let (mut nf, mut ne) = (0, 0);
            for (i, g) in geschosse {
                let boden = model.terrain_z_of(model.building_of(g.id));
                for (j, gr) in g.gruppen.iter_mut().enumerate() {
                    if let Some(n) = gr.netz(&q, sicht, boden, z) {
                        nf += n.faces.len();
                        ne += n.edges.len();
                        teile.push((i, j));
                    }
                }
            }
            m[0].faces.reserve_exact(nf);
            m[0].edges.reserve_exact(ne);
            for (i, j) in teile {
                let n = &self.geschosse[i].gruppen[j].netze[0].mesh;
                m[0].faces.extend_from_slice(&n.faces);
                m[0].edges.extend_from_slice(&n.edges);
            }
            return;
        }
        let ground = |id: ElementId| model.terrain_z_of(model.building_of_element(id));
        let vis = Vis {
            model,
            anim: s.vis_anim.as_ref(),
            table: &s.vis_table,
            stamp: s.vis_stamp(),
        };
        let (sicht, alles) = sicht;
        for (_, g) in geschosse {
            for gr in &mut g.gruppen {
                let Some(sicht) = gr.sicht(&q, (sicht, alles), z) else {
                    continue;
                };
                for x in &mut gr.items {
                    let id = x.id;
                    let Some(r) = x.raum.as_mut() else {
                        continue;
                    };
                    let k = match class_in(vis.element_masks(id), sk_model::NO_LAYER) {
                        Class::Solid => 0,
                        Class::Ghost => 1,
                        Class::Hidden => continue,
                    };
                    if let Some((e, rest)) = r.zeigt(sicht, alles, &mut z.netze.1) {
                        for t in std::iter::once(e).chain(rest) {
                            mesh_into(&mut m[k], t, ground(id));
                        }
                    }
                }
            }
        }
    }
}

/// Ein berührtes Geschoss `g` mit Höhenlage und Oberkante, grob um `rand`
/// ([`grob`]): Hausgruppen der Erkennung, die gleich geblieben sind,
/// bleiben samt Netz; in den anderen (mit `ganz` in allen) prüfen die
/// Räume, sobald eine Ansicht sie zeigt.
fn geschoss(
    s: &Scene,
    k: &impl Ketten,
    wo: (StoreyId, (f64, f64), f64),
    b: &Beruehrt,
    ganz: bool,
    vorher: Option<Geschoss>,
) -> Geschoss {
    // Nach dem Zeiger ihrer Flächen; die alte Gruppe hält sie am Leben
    let mut alte: HashMap<usize, Gruppe> = vorher
        .map(|x| x.gruppen)
        .unwrap_or_default()
        .into_iter()
        .map(|gr| (zeiger(&gr.flaechen), gr))
        .collect();
    let neue = s.raumgruppen(wo.0).gruppen;
    let mut bleiben: Vec<Option<Gruppe>> = Vec::with_capacity(neue.len());
    // Netze der Gruppen mit denselben Flächen, die prüfen: Sie bleiben bei
    // denselben Räumen, solange deren Körper gleich bleiben
    // ([`Gruppe::pruefen`])
    let mut netze: HashMap<usize, Vorher> = HashMap::new();
    for f in &neue {
        let z = zeiger(f);
        match alte.remove(&z) {
            Some(gr) if !ganz && gr.gleich(k, b) => bleiben.push(Some(gr)),
            Some(mut gr) => {
                let ids = gr.items.iter().map(|x| x.id).collect();
                netze.insert(z, (ids, std::mem::take(&mut gr.netze), gr.boden));
                alte.insert(z, gr);
                bleiben.push(None);
            }
            None => bleiben.push(None),
        }
    }
    // Die Räume der übrigen mit ihrem alten Stand
    let mut pool: HashMap<ElementId, Eintrag> = alte
        .into_values()
        .flat_map(|gr| gr.items)
        .map(|x| (x.id, x))
        .collect();
    let gruppen = neue
        .into_iter()
        .zip(bleiben)
        .map(|(f, gr)| {
            gr.unwrap_or_else(|| {
                let z = zeiger(&f);
                let mut gr = gruppe(wo, f, &mut pool);
                let ids: Vec<ElementId> = gr.items.iter().map(|x| x.id).collect();
                if let Some((_, n, boden)) = netze.remove(&z).filter(|n| n.0 == ids) {
                    gr.netze = n;
                    gr.boden = boden;
                }
                gr
            })
        })
        .collect();
    Geschoss { id: wo.0, gruppen }
}

/// Eine neue oder geänderte Hausgruppe im Geschoss `g`: Ihre Räume behalten
/// ihren alten Stand, bis sie prüfen; erst dann zählen die Fassungen ihrer
/// Züge ([`Gruppe::gleich`]).
fn gruppe(
    (g, hoehe, rand): (StoreyId, (f64, f64), f64),
    flaechen: Rc<[Raumflaeche]>,
    pool: &mut HashMap<ElementId, Eintrag>,
) -> Gruppe {
    let mut items = Vec::new();
    let mut um = None;
    for (i, f) in flaechen.iter().enumerate() {
        let Some(id) = f.form.raum else {
            continue;
        };
        let alt = pool.remove(&id).filter(|x| x.storey == g);
        items.push(Eintrag {
            id,
            storey: g,
            i,
            raum: alt.and_then(|x| x.raum),
        });
        um = union(um, grob(f, hoehe, rand));
    }
    let r = items.iter().filter_map(|x| x.raum.as_ref());
    Gruppe {
        kasten: [0, 1].map(|j| r.clone().fold(None, |a, r| union(a, r.kasten[j]))),
        alle: r.clone().all(|r| r.alle),
        grob: um,
        flaechen,
        zuege: Vec::new(),
        items,
        offen: true,
        netze: Vec::new(),
        boden: None,
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

    /// Ruhendes Netz (deckend, blass): die Wandzüge `runs` und, was dazu
    /// kommt, Erweiterungen und Ausbau innen. Das Live-Netz beim Ziehen
    /// ([`Scene::mesh_runs`]) trägt nur die gezogenen Züge, sonst stünde
    /// beides doppelt im Bild.
    pub(super) fn mesh_ruhend(
        &mut self,
        view: ViewKind,
        section: Option<Plane>,
        runs: &[RunId],
    ) -> [MeshData; 2] {
        let mut m = self.mesh_split(view, section, runs);
        self.ext_into(&mut m, view, section);
        self.ausbau_into(&mut m, view, section);
        m
    }

    /// Was Erweiterungen und Ausbau in einer Ansicht zeigen: Grundriss
    /// (Geschoss, Schnitthöhe) oder Schnittebene; `None`: Schnitt ohne
    /// Ebene.
    #[allow(clippy::type_complexity)]
    pub(super) fn ext_view(
        &self,
        view: ViewKind,
        section: Option<Plane>,
    ) -> Option<(Option<(StoreyId, f64)>, Option<Plane>)> {
        match view {
            ViewKind::Plan => Some((Some((self.active_storey(), self.plan_cut())), None)),
            ViewKind::Section => section.map(|p| (None, Some(p))),
            _ => Some((None, None)),
        }
    }

    /// Ausbau innen ins Netz ([`RaumCache::anhaengen`]).
    fn ausbau_into(&mut self, m: &mut [MeshData; 2], view: ViewKind, section: Option<Plane>) {
        let Some(i) = platz(view) else {
            return;
        };
        let alles = self.model.view_finish(i);
        // Ohne Schalter zeigt nur der Schnitt etwas: den Estrich
        if !alles && view != ViewKind::Section {
            return;
        }
        let Some((plan, section)) = self.ext_view(view, section) else {
            return;
        };
        let sicht = match (plan, section) {
            // Beim Ziehen der Schnitthöhe bleibt der letzte Schnitt bis zum
            // Loslassen
            (Some((_, cut)), _) => {
                let ziehen = self.model.in_step();
                let z = self.ausbau.plan.filter(|_| ziehen).unwrap_or(cut);
                self.ausbau.plan = Some(z);
                Sicht::Plan(z)
            }
            (None, Some(p)) => Sicht::Schnitt(p),
            (None, None) => Sicht::Voll,
        };
        // Der Speicher liest die Szene (Erkennung, berechnete Züge)
        let mut c = std::mem::take(&mut self.ausbau);
        c.sync(self);
        c.anhaengen(self, m, plan.map(|p| p.0), (sicht, alles));
        self.ausbau = c;
    }

    /// Wie oft die Körper eines Raums gerechnet wurden (Tests).
    #[cfg(test)]
    pub fn ausbau_builds(&self) -> u64 {
        self.ausbau.z.builds
    }

    /// Wie oft ein Raum Rohbau und Ausbau gerechnet hat (Tests).
    #[cfg(test)]
    pub fn ausbau_geprueft(&self) -> u64 {
        self.ausbau.z.geprueft
    }

    /// Wie oft eine Hausgruppe ihr Netz und ein Raum seinen Schnitt
    /// gerechnet hat (Tests).
    #[cfg(test)]
    pub fn ausbau_netze(&self) -> (u64, u64) {
        self.ausbau.z.netze
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
    use std::collections::BTreeMap;

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

    /// Estrich im Schnitt immer, Putz nur mit dem Schalter der Ansicht;
    /// Grundriss und 3D ohne Schalter nichts, 3D mit ihm auch den Estrich.
    /// Umschalten rechnet keinen Raum neu, nur einmal die übrigen Körper.
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
        let nur_estrich = s.ausbau_builds();
        assert!(nur_estrich >= 1);
        s.set_view_finish(1, true);
        let m = s.mesh(ViewKind::Section, quer, &[]);
        assert!(flaechen(&m, &putz, true) > 0, "Putz geschnitten");
        // Ohne Schalter rechnet ein Raum nur den Estrich, mit ihm einmal
        // den Rest dazu
        let gebaut = s.ausbau_builds();
        assert_eq!(gebaut, 2 * nur_estrich);
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
        // 3D: ohne Schalter nichts, mit ihm Estrich und Putz
        let m = s.mesh(ViewKind::Persp, None, &[]);
        assert_eq!(
            flaechen(&m, &estrich, false) + flaechen(&m, &putz, false),
            0
        );
        s.set_view_finish(2, true);
        let m = s.mesh(ViewKind::Persp, None, &[]);
        assert!(flaechen(&m, &estrich, false) > 0);
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

    /// Geprüfte und gebaute Räume und gerechnete Netze der Hausgruppen
    /// seit dem letzten Aufruf, nach dem Netz der 3D-Ansicht.
    fn neu(s: &mut Scene, vorher: &mut (u64, u64, u64)) -> (u64, u64, u64) {
        s.mesh(ViewKind::Persp, None, &[]);
        let jetzt = (s.ausbau_geprueft(), s.ausbau_builds(), s.ausbau_netze().0);
        let d = (jetzt.0 - vorher.0, jetzt.1 - vorher.1, jetzt.2 - vorher.2);
        *vorher = jetzt;
        d
    }

    /// Die Wand `w` um `dx` verschoben, in einem Schritt.
    fn wand_schritt(s: &mut Scene, w: RunId, dx: f64) {
        let pts: Vec<_> = s.model().run(w).unwrap().points.clone();
        let pts: Vec<_> = pts.iter().map(|p| *p + vec3(dx, 0.0, 0.0)).collect();
        s.begin("Wand verschieben");
        s.set_run_points(w, &pts);
        s.commit();
    }

    /// Review #137 B1: Grundriss und 3D ohne Schalter prüfen keinen Raum
    /// und rechnen kein Netz, auch nicht nach einem Schritt oder
    /// Rückgängig.
    #[test]
    fn ohne_schalter_nichts() {
        let mut s = Scene::new();
        let innen = haeuser(&mut s, 0, 2);
        let netze = |s: &mut Scene| {
            for v in [ViewKind::Persp, ViewKind::Plan] {
                s.mesh(v, None, &[]);
            }
            (s.ausbau_geprueft(), s.ausbau_netze())
        };
        assert_eq!(netze(&mut s), (0, (0, 0)));
        wand_schritt(&mut s, innen[0], -150.0);
        assert!(s.undo());
        assert_eq!(netze(&mut s), (0, (0, 0)));
    }

    /// Review #137 H5, N2: Eine Schnittebene schneidet nur die Räume, durch
    /// die sie geht, und zeigt den Ausbau nur in den Hausgruppen, die sie
    /// trifft: Räume ganz dahinter ganz, davor keine. Hausgruppen ganz
    /// dahinter zeigt sie ohne Ausbau wie die Ansichten, ihre Räume rechnen
    /// nicht. Eine verschobene Ebene schneidet nur diese Räume neu, eine
    /// schon gesehene keinen.
    #[test]
    fn schnitt_nur_durch_getroffene_raeume() {
        let mut s = Scene::new();
        haeuser(&mut s, 0, 3);
        let estrich = schluessel(&s, SCREED_SLAB_TYPE_GUID);
        let ebene = |x: f64| Some((vec3(x, 4000.0, 0.0), vec3(1.0, 0.0, 0.0)));
        let mut z = (s.ausbau_geprueft(), s.ausbau_netze());
        let mut neu = |s: &mut Scene, x: f64| {
            let m = s.mesh(ViewKind::Section, ebene(x), &[]);
            let jetzt = (s.ausbau_geprueft(), s.ausbau_netze());
            let d = (jetzt.0 - z.0, jetzt.1 .0 - z.1 .0, jetzt.1 .1 - z.1 .1);
            z = jetzt;
            (
                d,
                flaechen(&m, &estrich, true),
                flaechen(&m, &estrich, false),
            )
        };
        // Haus A: der mittlere Raum und der darüber geschnitten, der linke
        // ganz, der rechte nicht; nur seine vier Räume rechnen, die Häuser B
        // und C liegen davor
        let (d, geschnitten, ganz) = neu(&mut s, 5000.0);
        assert_eq!(d, (4, 2, 2));
        assert!(geschnitten > 0 && ganz > 0);
        assert_eq!(neu(&mut s, 5100.0).0, (0, 2, 2));
        assert_eq!(neu(&mut s, 5000.0).0, (0, 0, 0), "schon gesehen");
        // Zwischen den Häusern A und B und hinter allen: kein Ausbau
        for x in [12500.0, 60000.0] {
            assert_eq!(neu(&mut s, x), ((0, 0, 0), 0, 0));
        }
        // Durch Haus B: nun rechnen auch seine Räume
        let (d, geschnitten, ganz) = neu(&mut s, 20000.0);
        assert_eq!(d, (4, 2, 2));
        assert!(geschnitten > 0 && ganz > 0);
    }

    /// Fläche der Dreiecke je Baustoff und Richtung (auf 1 mm² gerundet)
    /// und die Kanten (auf 0,01 mm): gleich, wie auch immer geteilt.
    #[allow(clippy::type_complexity)]
    fn bild(m: &MeshData) -> (BTreeMap<(i64, [i64; 3]), i64>, Vec<[i64; 7]>) {
        let r = |x: f32, f: f64| (f64::from(x) * f).round() as i64;
        let mut a = BTreeMap::new();
        for t in m.faces.chunks_exact(3) {
            let p = |i: usize| vec3(t[i][0].into(), t[i][1].into(), t[i][2].into());
            let f = (p(1) - p(0)).cross(p(2) - p(0)).length() / 2.0;
            let k = (r(t[0][6], 1.0), [3, 4, 5].map(|i| r(t[0][i], 100.0)));
            *a.entry(k).or_insert(0.0) += f;
        }
        let a = a.into_iter().map(|(k, f)| (k, f.round() as i64)).collect();
        let mut e: Vec<[i64; 7]> = m
            .edges
            .iter()
            .map(|(p, k)| {
                let [a, b] = p.map(|q| q.map(|x| r(x, 100.0)));
                let (a, b) = if a <= b { (a, b) } else { (b, a) };
                [a[0], a[1], a[2], b[0], b[1], b[2], r(*k, 1.0)]
            })
            .collect();
        e.sort_unstable();
        (a, e)
    }

    /// Review #137 N1: Ein Grundriss in anderer Schnitthöhe schneidet keinen
    /// Raum neu und rechnet kein Netz, solange keine Höhe der Körper
    /// dazwischen liegt; er verschiebt nur die Schnittkanten und zeigt
    /// dasselbe wie gleich dort geschnitten. Über eine Höhe hinweg (Belag,
    /// Sockelleiste) schneidet er neu.
    #[test]
    fn andere_schnitthoehe_nur_verschoben() {
        let grundriss = |hoehen: &[f64]| {
            let mut s = Scene::new();
            haeuser(&mut s, 0, 2);
            s.set_view_finish(0, true);
            let mut m = MeshData::default();
            for &h in hoehen {
                s.set_plan_cut_height(h);
                m = s.mesh(ViewKind::Plan, None, &[]);
            }
            (m, s.ausbau_netze())
        };
        let (verschoben, z) = grundriss(&[1000.0, 1400.0, 1250.0]);
        let (direkt, z1) = grundriss(&[1250.0]);
        assert!(z1.1 > 0);
        assert_eq!(z, z1, "nur verschoben");
        assert_eq!(verschoben.faces.len(), direkt.faces.len());
        assert_eq!(bild(&verschoben), bild(&direkt));
        let (_, z2) = grundriss(&[1000.0, 100.0]);
        assert_eq!(z2, (2 * z1.0, 2 * z1.1), "neu geschnitten");
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
        let mut z = (0, 0, 0);
        assert_eq!(neu(&mut s, &mut z), (8, 8, 4));
        assert_eq!(neu(&mut s, &mut z), (0, 0, 0), "Netz ohne Änderung");
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
                (3, 2, 1),
                "Haus A, zwei Räume an der Wand ({i})"
            );
        }
        // Rückgängig: Haus A wieder, aus dem Speicher der Räume
        assert!(s.undo());
        assert_eq!(neu(&mut s, &mut z), (3, 2, 1));
        // Ein drittes Haus: nur seine Räume
        haeuser(&mut s, 2, 3);
        assert_eq!(s.model().spaces(st).len(), 9);
        assert_eq!(neu(&mut s, &mut z), (4, 4, 2));
    }

    /// Im offenen Schritt (Ziehen im Geschossmanager) prüft kein Raum und
    /// schneidet keiner neu, auch nicht in neuer Schnitthöhe oder mit
    /// höherem Geschoss; beim Loslassen folgt der Ausbau.
    #[test]
    fn im_schritt_folgt_der_ausbau_beim_loslassen() {
        let mut s = Scene::new();
        haeuser(&mut s, 0, 2);
        s.set_view_finish(0, true);
        s.mesh(ViewKind::Plan, None, &[]);
        let stand = |s: &Scene| (s.ausbau_geprueft(), s.ausbau_netze());
        let vorher = stand(&s);
        let st = s.active_storey();
        let g = s.model().storey(st).unwrap().clone();
        s.begin("Geschoss ziehen");
        s.drag_plan_cut(g.elevation + 1200.0);
        s.mesh(ViewKind::Plan, None, &[]);
        s.drag_storey_top(st, g.elevation + g.height + 200.0);
        s.mesh(ViewKind::Plan, None, &[]);
        assert_eq!(stand(&s), vorher);
        s.commit();
        s.mesh(ViewKind::Plan, None, &[]);
        let (geprueft, (netze, schnitte)) = stand(&s);
        assert!(geprueft > vorher.0 && netze > vorher.1 .0 && schnitte > vorher.1 .1);
    }

    /// Esc beim Ziehen: Der verworfene Schritt baut keinen Körper neu, das
    /// Netz ist danach wie davor. Zeigt eine Ansicht den Ausbau zum ersten
    /// Mal im offenen Schritt, gleicht der Speicher erst nach ihm ab.
    #[test]
    fn verworfener_schritt() {
        let mut s = Scene::new();
        let innen = haeuser(&mut s, 0, 2);
        s.set_view_finish(2, true);
        let ziehen = |s: &mut Scene| {
            let pts: Vec<_> = s.model().run(innen[0]).unwrap().points.clone();
            let pts: Vec<_> = pts.iter().map(|p| *p + vec3(-150.0, 0.0, 0.0)).collect();
            s.begin("Wand verschieben");
            s.set_run_points(innen[0], &pts);
            s.mesh(ViewKind::Persp, None, &[]);
            s.rollback();
        };
        ziehen(&mut s);
        assert_eq!(s.ausbau_geprueft(), 0);
        let vorher = s.mesh(ViewKind::Persp, None, &[]);
        let gebaut = s.ausbau_builds();
        assert!(s.ausbau_geprueft() > 0 && gebaut > 0);
        ziehen(&mut s);
        let nachher = s.mesh(ViewKind::Persp, None, &[]);
        assert_eq!(s.ausbau_builds(), gebaut);
        assert!(nachher.faces == vorher.faces && nachher.edges == vorher.edges);
    }

    /// Schaltet der Schnitt den Ausbau im offenen Schritt zu, zeigt er bis
    /// zu dessen Ende nur den schon gerechneten Estrich, danach alles wie
    /// ohne den Umweg.
    #[test]
    fn schalter_im_schritt() {
        let ebene = Some((vec3(5000.0, 4000.0, 0.0), vec3(1.0, 0.0, 0.0)));
        let mut s = Scene::new();
        haeuser(&mut s, 0, 1);
        let ohne = s.mesh(ViewKind::Section, ebene, &[]).faces.len();
        s.begin("Schalter");
        s.set_view_finish(1, true);
        assert_eq!(s.mesh(ViewKind::Section, ebene, &[]).faces.len(), ohne);
        s.rollback();
        let mit = s.mesh(ViewKind::Section, ebene, &[]).faces.len();
        assert!(mit > ohne);
        let mut t = Scene::new();
        haeuser(&mut t, 0, 1);
        t.set_view_finish(1, true);
        assert_eq!(t.mesh(ViewKind::Section, ebene, &[]).faces.len(), mit);
    }

    /// Eine andere Nutzung baut nur diesen Raum neu; bringt sie neue Typen
    /// ins Projekt oder ändert sich ein Typ, prüft jeder Raum, gebaut wird
    /// nur, wen es trifft, und nur dessen Hausgruppe rechnet ihr Netz neu.
    #[test]
    fn nutzung_und_typ() {
        let mut s = Scene::new();
        haeuser(&mut s, 0, 2);
        s.set_view_finish(2, true);
        let st = s.active_storey();
        let mut z = (0, 0, 0);
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
        assert_eq!(neu(&mut s, &mut z), (8, 1, 1));
        // Das zweite nur noch sich, geprüft werden die Räume seines Hauses
        let b = *raeume
            .iter()
            .find(|&&r| !same_haus(&s, raeume[0], r))
            .unwrap();
        nutzung(&mut s, b);
        assert_eq!(neu(&mut s, &mut z), (3, 1, 1));
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
        assert_eq!(neu(&mut s, &mut z), (8, 2, 2));
    }

    /// Liegen die Anker der Räume `a` und `b` im selben Haus?
    fn same_haus(s: &Scene, a: ElementId, b: ElementId) -> bool {
        let x = |r| (s.model().space(r).unwrap().anker.x / 15000.0).floor();
        x(a) == x(b)
    }
}
