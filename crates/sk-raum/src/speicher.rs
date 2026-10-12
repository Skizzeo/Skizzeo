//! Zwischenspeicher der Raumerkennung (Review #87 B1 bis B3): der Rohbau
//! je Wandzug, solange sich seine Kette nicht ändert, und die Flächen je
//! Hausgruppe, solange sich ihre Umrisse nicht ändern. Eine Hausgruppe
//! sind die Wandzüge eines Geschosses, deren Umrisse (um die Fangweite
//! erweitert) einander berühren; nur innerhalb einer Gruppe schließen
//! Wände gemeinsam Räume. Ein Wandschritt rechnet so nur die Gruppe neu,
//! an der er zieht, und der Abgleich sieht nur ihre Räume an.
//!
//! Je Bild und Mausbewegung (Review #87 R2): Die Lage eines Geschosses wird
//! nachgeführt, nicht neu gebildet: Nur die Gruppen der geänderten Züge und
//! die, deren Kasten ein geänderter Zug berührt, entstehen neu aus ihren
//! Zügen. Die geltenden Räume je Gruppe bleiben, solange sich Gruppe und
//! die Räume mit Anker in ihr nicht ändern; solange sich an den Räumen des
//! Geschosses nichts ändert (etwa beim Wandziehen), sieht die Zuteilung nur
//! neue Gruppen an.

use crate::abgleich::{stempel_in, zuteilen, Stand, StandGruppe};
use crate::erkennung::{formen_aus, rohbau, Kante, RaumForm, Teil, TOL};
use sk_math::Vec3;
use sk_model::{ElementId, ElementKind, Model, RunId, Space, StoreyId, WallChain};
use std::borrow::Cow;
use std::cell::OnceCell;
use std::collections::hash_map::DefaultHasher;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::hash::{BuildHasherDefault, Hash, Hasher};
use std::ops::Range;
use std::rc::Rc;

/// Tabelle über Schlüssel, die selbst schon Prüfsummen sind (Hausgruppen,
/// Flächen): sie werden nicht noch einmal gestreut.
pub(crate) type NachSchluessel<V> = HashMap<u64, V, BuildHasherDefault<Durch>>;

/// Reicht einen `u64`-Schlüssel unverändert durch ([`NachSchluessel`]).
#[derive(Default)]
pub(crate) struct Durch(u64);

impl Hasher for Durch {
    fn finish(&self) -> u64 {
        self.0
    }

    fn write(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.0 = self.0.rotate_left(8) ^ u64::from(b);
        }
    }

    fn write_u64(&mut self, n: u64) {
        self.0 = n;
    }
}

/// Achsparalleler Kasten: x und y von–bis (mm).
pub(crate) type Kasten = [f64; 4];

/// So weit (mm) wird der Kasten eines Zugs je Seite erweitert: Umrisse, die
/// sich bis auf die Fangweite nähern, kommen in dieselbe Gruppe.
const RAND: f64 = 2.0 * TOL;

/// Woher die Erkennung die Ketten der Wandzüge nimmt: aus dem Modell (alles
/// neu) oder aus den berechneten Zügen der Szene.
pub trait Ketten {
    /// Fassung der Kette des Zugs: gleich, solange sie sich nicht ändert;
    /// `None`, wenn unbekannt (dann rechnet die Erkennung den Rohbau neu).
    fn fassung(&self, run: RunId) -> Option<u64>;
    /// Die Kette des Zugs ([`Model::chain`]).
    fn kette(&self, run: RunId) -> Option<Cow<'_, WallChain>>;
}

impl Ketten for Model {
    fn fassung(&self, _: RunId) -> Option<u64> {
        None
    }

    fn kette(&self, run: RunId) -> Option<Cow<'_, WallChain>> {
        self.chain(run).map(Cow::Owned)
    }
}

/// Rohbauumrisse eines Zugs, ein Teil je Umriss.
pub(crate) struct Roh {
    umrisse: Vec<Vec<Vec3>>,
    teile: Vec<Teil>,
    kasten: Option<Kasten>,
    /// Fingerabdruck von Umrissen und Teilen.
    kennung: u64,
}

impl Roh {
    fn neu(m: &Model, run: RunId, c: &WallChain) -> Roh {
        let (mut umrisse, mut teile) = (Vec::new(), Vec::new());
        rohbau(m, run, c, &mut umrisse, &mut teile);
        let kasten = kasten_um(umrisse.iter().flatten());
        let mut h = DefaultHasher::new();
        for u in &umrisse {
            u.len().hash(&mut h);
            for p in u {
                [p.x, p.y].map(f64::to_bits).hash(&mut h);
            }
        }
        for t in &teile {
            t.hash_in(&mut h);
        }
        Roh {
            umrisse,
            teile,
            kasten,
            kennung: h.finish(),
        }
    }
}

/// Eine erkannte Fläche (noch ohne Raum) mit allem, was Zuordnung und
/// Stempel oft brauchen.
pub(crate) struct Fl {
    /// Raum `None`, Stempel auf der Mitte.
    pub form: RaumForm,
    pub kasten: Kasten,
    rand: Vec<Vec3>,
    loecher: Vec<Vec<Vec3>>,
    /// Fingerabdruck der Geometrie: dieselbe Fläche hat denselben Wert.
    pub kennung: u64,
    /// Tiefster, dann linkester Punkt des Umrisses (Ordnung der Räume).
    rang: Vec3,
    grund: OnceCell<f64>,
}

impl Fl {
    fn neu(form: RaumForm) -> Fl {
        let ring = |k: &[Kante]| -> Vec<Vec3> { k.iter().map(|k| k.a).collect() };
        let rand = ring(&form.umriss);
        let loecher: Vec<Vec<Vec3>> = form.loecher.iter().map(|l| ring(l)).collect();
        let mut h = DefaultHasher::new();
        for r in std::iter::once(&rand).chain(&loecher) {
            r.len().hash(&mut h);
            for p in r {
                [p.x, p.y].map(f64::to_bits).hash(&mut h);
            }
        }
        let rang = rand
            .iter()
            .copied()
            .min_by(|a, b| a.y.total_cmp(&b.y).then(a.x.total_cmp(&b.x)))
            .unwrap_or_default();
        Fl {
            kasten: kasten_um(rand.iter()).unwrap_or_default(),
            rand,
            loecher,
            kennung: h.finish(),
            rang,
            grund: OnceCell::new(),
            form,
        }
    }

    /// Liegt `p` in der Fläche (nicht in einem Loch)?
    pub fn enthaelt(&self, p: Vec3) -> bool {
        use sk_math::flaeche::enthaelt;
        drin(&self.kasten, p)
            && enthaelt(&self.rand, p)
            && !self.loecher.iter().any(|l| enthaelt(l, p))
    }

    /// Grundfläche (mm², [`crate::flaechen::grundflaeche`]), einmal gerechnet.
    pub fn grundflaeche(&self) -> f64 {
        *self
            .grund
            .get_or_init(|| crate::flaechen::grundflaeche(&self.form))
    }

    /// Ordnung der Räume: nach dem tiefsten, dann linkesten Punkt.
    pub fn ordnung(a: &Fl, b: &Fl) -> std::cmp::Ordering {
        (a.rang.y.total_cmp(&b.rang.y)).then(a.rang.x.total_cmp(&b.rang.x))
    }
}

/// Wandzüge einer Hausgruppe und ihre Flächen.
#[derive(Clone)]
pub(crate) struct Gruppe {
    /// Fingerabdruck von Geschoss und Rohbau der Züge.
    pub schluessel: u64,
    /// Um alle Züge, je Seite um [`RAND`] erweitert; enthält jede Fläche.
    pub kasten: Kasten,
    pub formen: Rc<[Rc<Fl>]>,
}

/// Erkannte Flächen eines Geschosses, je Hausgruppe.
pub struct Lage {
    pub geschoss: StoreyId,
    pub(crate) gruppen: Vec<Gruppe>,
}

impl Lage {
    /// Alle Flächen, nach ihrem tiefsten, dann linkesten Punkt.
    pub(crate) fn flaechen(&self) -> Vec<&Fl> {
        let mut v: Vec<&Fl> = self
            .gruppen
            .iter()
            .flat_map(|g| g.formen.iter().map(|f| f.as_ref()))
            .collect();
        v.sort_by(|a, b| Fl::ordnung(a, b));
        v
    }

    /// Die Räume des Geschosses wie [`crate::raeume`]: Formen mit dem
    /// geltenden Raum und der Stempellage.
    pub fn raeume(&self, m: &Model) -> Vec<RaumForm> {
        self.mit_grundflaeche_wenn(m, false)
            .into_iter()
            .map(|(f, _)| f)
            .collect()
    }

    /// Wie [`Lage::raeume`], dazu je Raum die Grundfläche (mm²).
    pub fn mit_grundflaeche(&self, m: &Model) -> Vec<(RaumForm, f64)> {
        self.mit_grundflaeche_wenn(m, true)
    }

    fn mit_grundflaeche_wenn(&self, m: &Model, grund: bool) -> Vec<(RaumForm, f64)> {
        let fl = self.flaechen();
        let raeume = crate::abgleich::raeume_im(m, self.geschoss, |_| true);
        let z = zuteilen(&fl, &raeume);
        fl.iter()
            .zip(z)
            .map(|(f, r)| {
                let mut form = f.form.clone();
                if let Some(id) = r {
                    form.raum = Some(id);
                    if let Some(s) = m.space(id) {
                        form.stempel = crate::abgleich::stempel_in(s, f);
                    }
                }
                (form, if grund { f.grundflaeche() } else { 0.0 })
            })
            .collect()
    }

    /// Geltende Räume je Fläche, als Stand vor dem nächsten Schritt
    /// ([`crate::abgleichen_lage`]).
    pub fn stand(&self, m: &Model) -> Stand {
        let fl = self.flaechen();
        let raeume = crate::abgleich::raeume_im(m, self.geschoss, |_| true);
        let z: HashMap<*const Fl, _> = fl
            .iter()
            .map(|f| *f as *const Fl)
            .zip(zuteilen(&fl, &raeume))
            .collect();
        let mut st = Stand::default();
        for g in &self.gruppen {
            let raeume = g
                .formen
                .iter()
                .map(|f| (f.clone(), z.get(&Rc::as_ptr(f)).copied().flatten()))
                .collect();
            st.setzen(g.schluessel, Rc::new(raeume));
        }
        st
    }
}

/// Eine Fläche, wie sie der Grundriss zeigt: Form mit geltendem Raum und
/// Stempellage, Grundfläche (mm²) und Kasten (mm, x und y von–bis).
#[derive(Clone, Debug, PartialEq)]
pub struct Raumflaeche {
    pub form: RaumForm,
    pub grundflaeche: f64,
    pub kasten: [f64; 4],
}

/// Die Räume eines Geschosses je Hausgruppe ([`Erkennung::raumformen`]).
#[derive(Clone)]
pub struct Raumformen {
    pub geschoss: StoreyId,
    pub gruppen: Vec<Rc<[Raumflaeche]>>,
}

impl Raumformen {
    /// Alle Flächen, Gruppe für Gruppe.
    pub fn flaechen(&self) -> impl Iterator<Item = &Raumflaeche> {
        self.gruppen.iter().flat_map(|g| g.iter())
    }
}

struct RohEintrag {
    zug: RunId,
    geschoss: StoreyId,
    fassung: u64,
    roh: Rc<Roh>,
    lauf: u64,
}

struct GruppenEintrag {
    geschoss: StoreyId,
    formen: Rc<[Rc<Fl>]>,
    lauf: u64,
}

/// Letzte Lage eines Geschosses: Daran führt die nächste Lage nur die
/// Gruppen nach, deren Züge sich geändert haben (Review #87 R2). Ein Zug
/// ohne Fassung (im Schritt geändert, noch nicht gebaut) gilt jedes Mal als
/// geändert, so kostet auch der Abschluss eines Schritts nur seine Gruppen
/// (Review #87 R5).
#[derive(Default)]
struct Letzte {
    /// Züge des Geschosses in Modellreihenfolge mit ihrer Fassung.
    zuege: Vec<(RunId, Option<u64>)>,
    /// Gruppen nach Schlüssel, je mit ihren Zügen in Modellreihenfolge.
    gruppen: NachSchluessel<(Gruppe, Vec<RunId>)>,
    /// Schlüssel der Gruppen nach dem Platz ihres ersten Zugs (Reihenfolge
    /// wie bei der vollen Rechnung).
    folge: BTreeMap<u32, u64>,
    /// Gruppe je Zug mit Rohbau.
    gruppe_von: HashMap<RunId, u64>,
}

impl Letzte {
    fn lage(&self, g: StoreyId) -> Lage {
        Lage {
            geschoss: g,
            gruppen: self
                .folge
                .values()
                .filter_map(|s| self.gruppen.get(s).map(|(gr, _)| gr.clone()))
                .collect(),
        }
    }

    fn einsetzen(&mut self, gr: Gruppe, glieder: Vec<RunId>) {
        if let Some(erster) = glieder.first() {
            self.folge.insert(erster.index(), gr.schluessel);
        }
        for &z in &glieder {
            self.gruppe_von.insert(z, gr.schluessel);
        }
        self.gruppen.insert(gr.schluessel, (gr, glieder));
    }
}

/// Räume eines Geschosses, ältere zuerst, mit ihrem Anker.
struct Raumliste {
    /// Solange [`raumstand`] diesen Wert gibt, gilt die Liste.
    stand: Option<u64>,
    raeume: Vec<(ElementId, Vec3)>,
}

/// Geltende Räume einer Hausgruppe.
struct Zuteilung {
    /// Fingerabdruck der Räume mit Anker im Kasten der Gruppe.
    kennung: u64,
    /// Je Fläche der Gruppe ihr geltender Raum, wie ihn der Stand hält.
    stand: Rc<StandGruppe>,
    formen: Rc<[Raumflaeche]>,
    lauf: u64,
}

/// Was die Erkennung je Geschoss behält.
#[derive(Default)]
struct Geschoss {
    letzte: Option<Letzte>,
    raumliste: Option<Raumliste>,
    /// Nach dem Schlüssel der Gruppe.
    zuteilungen: NachSchluessel<Zuteilung>,
}

/// Solange dieser Wert gleich bleibt, ändern sich die Räume des Geschosses
/// `g` nicht: außerhalb eines Schritts die Revision des Modells, in einem
/// Schritt, der an ihnen nichts ändert, die Revision bei seinem Beginn;
/// sonst `None`.
fn raumstand(m: &Model, g: StoreyId) -> Option<u64> {
    match m.step_start_revision() {
        None => Some(m.revision()),
        Some(r) => (!m.raum_geschosse_im_schritt().contains(&g)).then_some(r),
    }
}

/// Fingerabdruck der Räume einer Gruppe (in ihrer Reihenfolge): was die
/// Ankerregel und die Stempellage lesen.
fn raeume_kennung(schluessel: u64, raeume: &[(ElementId, &Space)]) -> u64 {
    let mut h = DefaultHasher::new();
    schluessel.hash(&mut h);
    for (id, s) in raeume {
        id.hash(&mut h);
        [s.anker.x, s.anker.y].map(f64::to_bits).hash(&mut h);
        s.stempel.map(|p| [p.x, p.y].map(f64::to_bits)).hash(&mut h);
    }
    h.finish()
}

/// Raumerkennung mit Zwischenspeicher (siehe Modulkopf). Ergebnis wie
/// [`crate::raeume`]; was in einem Aufruf je Geschoss nicht mehr vorkommt,
/// fällt aus dem Speicher.
#[derive(Default)]
pub struct Erkennung {
    /// Je Zug an der Stelle seines Index.
    roh: Vec<Option<RohEintrag>>,
    gruppen: NachSchluessel<GruppenEintrag>,
    geschosse: HashMap<StoreyId, Geschoss>,
    lauf: u64,
    gerechnet: u64,
    zugeteilt: u64,
    voll: u64,
}

impl Erkennung {
    /// Wie oft eine Hausgruppe neu gerechnet wurde (für Tests und Messung).
    pub fn gerechnet(&self) -> u64 {
        self.gerechnet
    }

    /// Wie oft einer Hausgruppe die Räume neu zugeteilt wurden.
    pub fn zugeteilt(&self) -> u64 {
        self.zugeteilt
    }

    /// Flächen des Geschosses `g`, je Hausgruppe. Nach der ersten Rechnung
    /// werden nur die Gruppen der geänderten Züge und der Züge ohne Fassung
    /// neu gebildet.
    pub fn lage(&mut self, m: &Model, k: &(impl Ketten + ?Sized), g: StoreyId) -> Lage {
        self.lauf += 1;
        let jetzt: Vec<(RunId, Option<u64>)> = m
            .runs()
            .iter()
            .filter(|(_, r)| r.storey == g)
            .map(|(id, _)| (id, k.fassung(id)))
            .collect();
        if let Some(l) = self.nachfuehren(m, k, g, &jetzt) {
            return l;
        }
        self.neu(m, k, g, &jetzt)
    }

    /// Rohbau des Zugs, aus dem Speicher, solange seine Fassung gleich ist.
    fn roh(
        &mut self,
        m: &Model,
        k: &(impl Ketten + ?Sized),
        id: RunId,
        fassung: Option<u64>,
        g: StoreyId,
    ) -> Option<Rc<Roh>> {
        let lauf = self.lauf;
        let platz = id.index() as usize;
        if self.roh.len() <= platz {
            self.roh.resize_with(platz + 1, || None);
        }
        if let (Some(f), Some(e)) = (fassung, self.roh[platz].as_mut()) {
            if e.zug == id && e.fassung == f && e.geschoss == g {
                e.lauf = lauf;
                return Some(e.roh.clone());
            }
        }
        let c = k.kette(id)?;
        let roh = Rc::new(Roh::neu(m, id, &c));
        self.roh[platz] = fassung.map(|fassung| RohEintrag {
            zug: id,
            geschoss: g,
            fassung,
            roh: roh.clone(),
            lauf,
        });
        Some(roh)
    }

    /// Hausgruppe aus ihren Zügen (Modellreihenfolge, je Rohbau und
    /// erweiterter Kasten); die Flächen aus dem Speicher, wenn es die Gruppe
    /// schon gab.
    fn gruppe(&mut self, g: StoreyId, zg: &[(Rc<Roh>, Kasten)]) -> Gruppe {
        let lauf = self.lauf;
        let mut h = DefaultHasher::new();
        g.hash(&mut h);
        for (r, _) in zg {
            r.kennung.hash(&mut h);
        }
        let schluessel = h.finish();
        let kasten = zg
            .iter()
            .map(|z| z.1)
            .reduce(|a, b| vereint(&a, &b))
            .unwrap_or_default();
        let formen = match self.gruppen.get_mut(&schluessel) {
            Some(e) if e.geschoss == g => {
                e.lauf = lauf;
                e.formen.clone()
            }
            _ => {
                self.gerechnet += 1;
                let mut umrisse = Vec::new();
                let mut teile = Vec::new();
                for (r, _) in zg {
                    umrisse.extend(r.umrisse.iter().cloned());
                    teile.extend(r.teile.iter().cloned());
                }
                let formen: Rc<[Rc<Fl>]> = formen_aus(&umrisse, &teile, g)
                    .into_iter()
                    .map(|f| Rc::new(Fl::neu(f)))
                    .collect();
                let e = GruppenEintrag {
                    geschoss: g,
                    formen: formen.clone(),
                    lauf,
                };
                self.gruppen.insert(schluessel, e);
                formen
            }
        };
        Gruppe {
            schluessel,
            kasten,
            formen,
        }
    }

    /// Volle Rechnung über alle Züge des Geschosses.
    fn neu(
        &mut self,
        m: &Model,
        k: &(impl Ketten + ?Sized),
        g: StoreyId,
        jetzt: &[(RunId, Option<u64>)],
    ) -> Lage {
        self.voll += 1;
        let lauf = self.lauf;
        let mut zuege: Vec<(RunId, Rc<Roh>)> = Vec::new();
        for &(id, fassung) in jetzt {
            if let Some(roh) = self.roh(m, k, id, fassung, g) {
                if roh.kasten.is_some() {
                    zuege.push((id, roh));
                }
            }
        }
        for e in &mut self.roh {
            if e.as_ref()
                .is_some_and(|e| e.geschoss == g && e.lauf != lauf)
            {
                *e = None;
            }
        }
        let kaesten: Vec<Kasten> = zuege
            .iter()
            .map(|z| weiter(&z.1.kasten.unwrap_or_default(), RAND))
            .collect();
        let (glieder, bereiche) = gruppieren(&kaesten);
        let mut letzte = Letzte::default();
        for b in bereiche {
            let zg: Vec<(Rc<Roh>, Kasten)> = glieder[b.clone()]
                .iter()
                .map(|&i| (zuege[i].1.clone(), kaesten[i]))
                .collect();
            let gr = self.gruppe(g, &zg);
            letzte.einsetzen(gr, glieder[b].iter().map(|&i| zuege[i].0).collect());
        }
        self.gruppen
            .retain(|_, e| e.geschoss != g || e.lauf == lauf);
        let lage = letzte.lage(g);
        let gs = self.geschosse.entry(g).or_default();
        gs.letzte = Some(Letzte {
            zuege: jetzt.to_vec(),
            ..letzte
        });
        lage
    }

    /// Führt die letzte Lage nach: Gruppen der geänderten, neuen und
    /// entfallenen Züge und die, deren Kasten ein geänderter Zug berührt,
    /// werden aus ihren Zügen neu gebildet, die übrigen bleiben. Dasselbe
    /// wie die volle Rechnung, weil sich nur in diesen Gruppen Kästen
    /// berühren können, die es vorher nicht taten (und umgekehrt). `None`:
    /// keine letzte Lage.
    fn nachfuehren(
        &mut self,
        m: &Model,
        k: &(impl Ketten + ?Sized),
        g: StoreyId,
        jetzt: &[(RunId, Option<u64>)],
    ) -> Option<Lage> {
        let mut l = self.geschosse.get_mut(&g)?.letzte.take()?;
        // Geänderte und neue Züge, entfallene Züge (beide Listen nach Platz)
        let (mut neu, mut weg) = (Vec::new(), Vec::new());
        let (mut i, mut j) = (0, 0);
        while i < jetzt.len() || j < l.zuege.len() {
            match (jetzt.get(i), l.zuege.get(j)) {
                (Some(a), Some(b)) if a.0.index() == b.0.index() => {
                    if a.0 != b.0 {
                        weg.push(b.0);
                        neu.push(*a);
                    } else if a.1 != b.1 || a.1.is_none() {
                        neu.push(*a);
                    }
                    i += 1;
                    j += 1;
                }
                (Some(a), Some(b)) if a.0.index() < b.0.index() => {
                    neu.push(*a);
                    i += 1;
                }
                (Some(a), None) => {
                    neu.push(*a);
                    i += 1;
                }
                (_, Some(b)) => {
                    weg.push(b.0);
                    j += 1;
                }
                (None, None) => break,
            }
        }
        if neu.is_empty() && weg.is_empty() {
            let lage = l.lage(g);
            self.geschosse.entry(g).or_default().letzte = Some(l);
            return Some(lage);
        }
        // Jeder geänderte Zug sieht alle Gruppen an: Bei vielen (etwa ohne
        // jede Fassung) ist die volle Rechnung schneller
        if neu.len() + weg.len() > 16 + jetzt.len() / 8 {
            return None;
        }
        // Die übrigen Züge betroffener Gruppen behalten ihren Rohbau; fehlt
        // er im Speicher, rechnet die volle Rechnung
        let geaendert: HashSet<RunId> =
            neu.iter().map(|z| z.0).chain(weg.iter().copied()).collect();
        let mut betroffen: Vec<u64> = geaendert
            .iter()
            .filter_map(|z| l.gruppe_von.get(z).copied())
            .collect();
        let mut zuege: Vec<(RunId, Rc<Roh>, Kasten)> = Vec::new();
        for &(id, f) in &neu {
            let Some(roh) = self.roh(m, k, id, f, g) else {
                continue;
            };
            let Some(kk) = roh.kasten else {
                continue;
            };
            let e = weiter(&kk, RAND);
            betroffen.extend(
                l.gruppen
                    .iter()
                    .filter(|(_, (gr, _))| beruehren(&gr.kasten, &e))
                    .map(|(s, _)| *s),
            );
            zuege.push((id, roh, e));
        }
        betroffen.sort_unstable();
        betroffen.dedup();
        for s in &betroffen {
            let Some((_, glieder)) = l.gruppen.get(s) else {
                continue;
            };
            for &z in glieder.iter().filter(|z| !geaendert.contains(z)) {
                let roh = self.roh[z.index() as usize]
                    .as_ref()
                    .filter(|e| e.zug == z && e.geschoss == g)
                    .map(|e| e.roh.clone());
                let roh = roh?;
                let e = weiter(&roh.kasten.unwrap_or_default(), RAND);
                zuege.push((z, roh, e));
            }
        }
        // Ab hier wird geändert
        for z in &weg {
            if let Some(e) = self.roh.get_mut(z.index() as usize) {
                if e.as_ref().is_some_and(|e| e.zug == *z) {
                    *e = None;
                }
            }
            l.gruppe_von.remove(z);
        }
        for s in &betroffen {
            if let Some((_, glieder)) = l.gruppen.remove(s) {
                if let Some(erster) = glieder.first() {
                    l.folge.remove(&erster.index());
                }
                for z in glieder {
                    l.gruppe_von.remove(&z);
                }
            }
        }
        zuege.sort_by_key(|z| z.0.index());
        let kaesten: Vec<Kasten> = zuege.iter().map(|z| z.2).collect();
        let (glieder, bereiche) = gruppieren(&kaesten);
        for b in bereiche {
            let zg: Vec<(Rc<Roh>, Kasten)> = glieder[b.clone()]
                .iter()
                .map(|&i| (zuege[i].1.clone(), zuege[i].2))
                .collect();
            let gr = self.gruppe(g, &zg);
            l.einsetzen(gr, glieder[b].iter().map(|&i| zuege[i].0).collect());
        }
        for s in &betroffen {
            if !l.gruppen.contains_key(s) {
                self.gruppen.remove(s);
            }
        }
        l.zuege = jetzt.to_vec();
        let lage = l.lage(g);
        self.geschosse.entry(g).or_default().letzte = Some(l);
        Some(lage)
    }

    /// Die Räume des Geschosses `g` je Hausgruppe wie
    /// [`Lage::mit_grundflaeche`], ohne Ordnung über die Gruppen hinweg; eine
    /// Gruppe, deren Flächen und Räume gleich bleiben, kommt aus dem
    /// Speicher.
    pub fn raumformen(&mut self, m: &Model, k: &(impl Ketten + ?Sized), g: StoreyId) -> Raumformen {
        let lage = self.zuteilung(m, k, g);
        let z = &self.geschosse.entry(g).or_default().zuteilungen;
        Raumformen {
            geschoss: g,
            gruppen: lage
                .gruppen
                .iter()
                .filter_map(|gr| z.get(&gr.schluessel).map(|e| e.formen.clone()))
                .collect(),
        }
    }

    /// Geltende Räume je Fläche des Geschosses `g`, als Stand vor dem
    /// nächsten Schritt wie [`Lage::stand`].
    pub fn stand(&mut self, m: &Model, k: &(impl Ketten + ?Sized), g: StoreyId) -> Stand {
        let lage = self.zuteilung(m, k, g);
        let z = &self.geschosse.entry(g).or_default().zuteilungen;
        let mut st = Stand::default();
        for gr in &lage.gruppen {
            if let Some(e) = z.get(&gr.schluessel) {
                st.setzen(gr.schluessel, e.stand.clone());
            }
        }
        st
    }

    /// Lage des Geschosses; je Hausgruppe stehen danach die geltenden Räume
    /// ihrer Flächen samt Formen im Speicher. Die Räume einer Gruppe sind
    /// die, deren Anker in ihrem Kasten liegt; die Ankerregel
    /// ([`zuteilen`]) gibt je Gruppe dasselbe wie über das ganze Geschoss,
    /// weil jeder Anker in höchstens einer Fläche liegt. Solange sich die
    /// Räume nicht ändern ([`raumstand`]), sieht sie nur neue Gruppen an.
    fn zuteilung(&mut self, m: &Model, k: &(impl Ketten + ?Sized), g: StoreyId) -> Lage {
        let lage = self.lage(m, k, g);
        let lauf = self.lauf;
        let stand = raumstand(m, g);
        let gs = self.geschosse.entry(g).or_default();
        let gilt = stand.is_some() && gs.raumliste.as_ref().is_some_and(|l| l.stand == stand);
        // Räume je Gruppe: alle neu über ein Raster der Gruppenkästen, wenn
        // sich Räume geändert haben können, sonst nur für neue Gruppen
        let mut je: Vec<Option<Vec<(ElementId, &Space)>>> = vec![None; lage.gruppen.len()];
        if !gilt && lage.gruppen.is_empty() {
            // Ohne Gruppe gibt es nichts zuzuteilen
            gs.raumliste = None;
        } else if !gilt {
            let mut raeume: Vec<(&str, ElementId, &Space)> = Vec::new();
            for (id, e) in m.elements().iter() {
                if let ElementKind::Space(s) = &e.kind {
                    if e.storey == g {
                        raeume.push((e.number.as_str(), id, &**s));
                    }
                }
            }
            raeume.sort_by(|a, b| a.0.len().cmp(&b.0.len()).then(a.0.cmp(b.0)));
            let kaesten: Vec<Kasten> = lage.gruppen.iter().map(|gr| gr.kasten).collect();
            let raster = Raster::neu(&kaesten);
            for x in &mut je {
                *x = Some(Vec::new());
            }
            for &(_, id, s) in &raeume {
                // Kästen von Gruppen können sich überdecken (Haus im Winkel
                // eines anderen): dann in jeder, die Ankerregel entscheidet
                for &i in raster.bei(s.anker) {
                    if drin(&kaesten[i as usize], s.anker) {
                        if let Some(v) = je[i as usize].as_mut() {
                            v.push((id, s));
                        }
                    }
                }
            }
            gs.raumliste = Some(Raumliste {
                stand,
                raeume: raeume.iter().map(|&(_, id, s)| (id, s.anker)).collect(),
            });
        }
        let liste = gs.raumliste.as_ref().map_or(&[][..], |l| &l.raeume[..]);
        for (gr, r) in lage.gruppen.iter().zip(&mut je) {
            if r.is_none() && !gs.zuteilungen.contains_key(&gr.schluessel) {
                *r = Some(
                    liste
                        .iter()
                        .filter(|(_, a)| drin(&gr.kasten, *a))
                        .filter_map(|&(id, _)| m.space(id).map(|s| (id, s)))
                        .collect(),
                );
            }
        }
        for (gr, r) in lage.gruppen.iter().zip(je) {
            let Some(r) = r else {
                if let Some(e) = gs.zuteilungen.get_mut(&gr.schluessel) {
                    e.lauf = lauf;
                }
                continue;
            };
            let kennung = raeume_kennung(gr.schluessel, &r);
            if let Some(e) = gs
                .zuteilungen
                .get_mut(&gr.schluessel)
                .filter(|e| e.kennung == kennung)
            {
                e.lauf = lauf;
                continue;
            }
            self.zugeteilt += 1;
            let fl: Vec<&Fl> = gr.formen.iter().map(|f| f.as_ref()).collect();
            let z = zuteilen(&fl, &r);
            let formen = fl
                .iter()
                .zip(&z)
                .map(|(f, id)| {
                    let mut form = f.form.clone();
                    if let Some(id) = *id {
                        form.raum = Some(id);
                        if let Some((_, s)) = r.iter().find(|x| x.0 == id) {
                            form.stempel = stempel_in(s, f);
                        }
                    }
                    Raumflaeche {
                        form,
                        grundflaeche: f.grundflaeche(),
                        kasten: f.kasten,
                    }
                })
                .collect();
            let e = Zuteilung {
                kennung,
                stand: Rc::new(gr.formen.iter().cloned().zip(z).collect()),
                formen,
                lauf,
            };
            gs.zuteilungen.insert(gr.schluessel, e);
        }
        gs.zuteilungen.retain(|_, e| e.lauf == lauf);
        lage
    }
}

/// Hausgruppen: Züge, deren erweiterte Kästen `k` sich berühren (auch
/// über andere), je Gruppe in der Reihenfolge der Züge; als Liste der
/// Glieder und je Gruppe ihr Bereich darin. Zwei Kästen, die sich
/// berühren, liegen in einer gemeinsamen Zelle eines Rasters über alle;
/// verglichen wird nur innerhalb der Zellen (Häuser in Reihen und Spalten
/// bleiben so nahezu linear).
fn gruppieren(k: &[Kasten]) -> (Vec<usize>, Vec<Range<usize>>) {
    let mut eltern: Vec<usize> = (0..k.len()).collect();
    fn wurzel(e: &mut [usize], mut i: usize) -> usize {
        while e[i] != i {
            e[i] = e[e[i]];
            i = e[i];
        }
        i
    }
    for zelle in Raster::neu(k).zellen() {
        for (n, &i) in zelle.iter().enumerate() {
            for &j in &zelle[n + 1..] {
                let (i, j) = (i as usize, j as usize);
                if beruehren(&k[i], &k[j]) {
                    let (a, b) = (wurzel(&mut eltern, i), wurzel(&mut eltern, j));
                    eltern[a.max(b)] = a.min(b);
                }
            }
        }
    }
    // Gruppen in der Reihenfolge ihres ersten Glieds, dann einsortiert
    let mut nummer = vec![usize::MAX; k.len()];
    let mut groesse: Vec<usize> = Vec::new();
    let gruppe: Vec<usize> = (0..k.len())
        .map(|i| {
            let w = wurzel(&mut eltern, i);
            if nummer[w] == usize::MAX {
                nummer[w] = groesse.len();
                groesse.push(0);
            }
            groesse[nummer[w]] += 1;
            nummer[w]
        })
        .collect();
    let mut bereiche = Vec::with_capacity(groesse.len());
    let mut frei = Vec::with_capacity(groesse.len());
    let mut a = 0;
    for n in groesse {
        bereiche.push(a..a + n);
        frei.push(a);
        a += n;
    }
    let mut glieder = vec![0; k.len()];
    for (i, &g) in gruppe.iter().enumerate() {
        glieder[frei[g]] = i;
        frei[g] += 1;
    }
    (glieder, bereiche)
}

pub(crate) fn kasten_um<'a>(pts: impl IntoIterator<Item = &'a Vec3>) -> Option<Kasten> {
    pts.into_iter().fold(None, |k, p| {
        Some(match k {
            None => [p.x, p.y, p.x, p.y],
            Some([x0, y0, x1, y1]) => [x0.min(p.x), y0.min(p.y), x1.max(p.x), y1.max(p.y)],
        })
    })
}

pub(crate) fn drin(k: &Kasten, p: Vec3) -> bool {
    k[0] <= p.x && p.x <= k[2] && k[1] <= p.y && p.y <= k[3]
}

pub(crate) fn beruehren(a: &Kasten, b: &Kasten) -> bool {
    a[0] <= b[2] && b[0] <= a[2] && a[1] <= b[3] && b[1] <= a[3]
}

fn weiter(k: &Kasten, d: f64) -> Kasten {
    [k[0] - d, k[1] - d, k[2] + d, k[3] + d]
}

fn vereint(a: &Kasten, b: &Kasten) -> Kasten {
    [
        a[0].min(b[0]),
        a[1].min(b[1]),
        a[2].max(b[2]),
        a[3].max(b[3]),
    ]
}

/// Raster über Kästen: zu einem Punkt die Kästen, die ihn enthalten können.
/// Die Kästen aller Zellen liegen hintereinander in einer Liste.
pub(crate) struct Raster {
    x0: f64,
    y0: f64,
    zx: f64,
    zy: f64,
    nx: usize,
    ny: usize,
    /// Je Zelle der Beginn ihrer Kästen in `kaesten`, am Schluss das Ende.
    anfang: Vec<usize>,
    kaesten: Vec<u32>,
}

impl Raster {
    /// Etwa ein Kasten je Zelle, höchstens 256 × 256 Zellen.
    pub fn neu(kaesten: &[Kasten]) -> Raster {
        let Some(g) = kaesten.iter().copied().reduce(|a, b| vereint(&a, &b)) else {
            return Raster {
                x0: 0.0,
                y0: 0.0,
                zx: 1.0,
                zy: 1.0,
                nx: 0,
                ny: 0,
                anfang: Vec::new(),
                kaesten: Vec::new(),
            };
        };
        let (w, h) = ((g[2] - g[0]).max(1.0), (g[3] - g[1]).max(1.0));
        let z = (w * h / kaesten.len() as f64).sqrt().max(1.0);
        let nx = ((w / z).ceil() as usize).clamp(1, 256);
        let ny = ((h / z).ceil() as usize).clamp(1, 256);
        let mut r = Raster {
            x0: g[0],
            y0: g[1],
            zx: w / nx as f64,
            zy: h / ny as f64,
            nx,
            ny,
            anfang: vec![0; nx * ny + 1],
            kaesten: Vec::new(),
        };
        // Zellen je Kasten; erst zählen, dann einsortieren
        let bereiche: Vec<_> = kaesten
            .iter()
            .map(|k| (r.zelle(k[0], k[1]), r.zelle(k[2], k[3])))
            .collect();
        for &(a, b) in &bereiche {
            for y in a.1..=b.1 {
                for x in a.0..=b.0 {
                    r.anfang[y * nx + x + 1] += 1;
                }
            }
        }
        for c in 0..nx * ny {
            r.anfang[c + 1] += r.anfang[c];
        }
        let mut frei = r.anfang[..nx * ny].to_vec();
        r.kaesten = vec![0; r.anfang[nx * ny]];
        for (i, &(a, b)) in bereiche.iter().enumerate() {
            for y in a.1..=b.1 {
                for x in a.0..=b.0 {
                    let c = y * nx + x;
                    r.kaesten[frei[c]] = i as u32;
                    frei[c] += 1;
                }
            }
        }
        r
    }

    fn zelle(&self, x: f64, y: f64) -> (usize, usize) {
        let i = |v: f64, z: f64, n: usize| ((v / z).floor().max(0.0) as usize).min(n - 1);
        (
            i(x - self.x0, self.zx, self.nx),
            i(y - self.y0, self.zy, self.ny),
        )
    }

    /// Die Kästen je Zelle (Nummern wie beim Anlegen).
    pub fn zellen(&self) -> impl Iterator<Item = &[u32]> {
        self.anfang.windows(2).map(|w| &self.kaesten[w[0]..w[1]])
    }

    /// Kästen, die `p` enthalten können (Nummern wie beim Anlegen).
    pub fn bei(&self, p: Vec3) -> &[u32] {
        if self.nx == 0 || !(p.x.is_finite() && p.y.is_finite()) {
            return &[];
        }
        let (x, y) = self.zelle(p.x, p.y);
        let c = y * self.nx + x;
        &self.kaesten[self.anfang[c]..self.anfang[c + 1]]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sk_math::vec3;
    use sk_model::{Category, RefSide};

    /// `n` Häuser 10 × 8 m nebeneinander, je eine Innenwand bei x = 3 m.
    fn stadt(n: usize) -> (Model, StoreyId) {
        let mut m = Model::new();
        let st = m.defaults().storey;
        let aw = m.defaults().exterior_wall;
        let iw = m.defaults().interior_wall;
        let t: f64 = m.wall_layers(aw).iter().map(|l| l.thickness).sum();
        for i in 0..n {
            let ox = i as f64 * 15000.0;
            let p = |x: f64, y: f64| vec3(ox + x, y, 0.0);
            let pts = [
                p(0.0, 0.0),
                p(0.0, 8000.0),
                p(10000.0, 8000.0),
                p(10000.0, 0.0),
            ];
            m.add_wall_run(&pts, true, RefSide::Left, st, aw, Category::ExteriorWall)
                .unwrap();
            let iwp = [p(3000.0, t), p(3000.0, 8000.0 - t)];
            m.add_wall_run(&iwp, false, RefSide::Center, st, iw, Category::InteriorWall)
                .unwrap();
        }
        (m, st)
    }

    /// Fassung je Zug von außen: ändert sich nur, wenn der Test es sagt.
    struct Feste<'a>(&'a Model, HashMap<RunId, u64>);

    impl Ketten for Feste<'_> {
        fn fassung(&self, run: RunId) -> Option<u64> {
            Some(self.1.get(&run).copied().unwrap_or(0))
        }

        fn kette(&self, run: RunId) -> Option<Cow<'_, WallChain>> {
            self.0.chain(run).map(Cow::Owned)
        }
    }

    #[test]
    fn gruppen_je_haus_und_gleich_wie_ohne_speicher() {
        let (m, st) = stadt(3);
        let mut e = Erkennung::default();
        let k = Feste(&m, HashMap::new());
        let l = e.lage(&m, &k, st);
        assert_eq!(l.gruppen.len(), 3, "drei Häuser, drei Gruppen");
        assert!(l.gruppen.iter().all(|g| g.formen.len() == 2));
        assert_eq!(l.raeume(&m), crate::raeume(&m, st));
        // Zweiter Lauf: dieselben Flächen aus dem Speicher
        let l2 = e.lage(&m, &k, st);
        for (a, b) in l.gruppen.iter().zip(&l2.gruppen) {
            assert!(Rc::ptr_eq(&a.formen, &b.formen));
        }
    }

    #[test]
    fn nur_die_geaenderte_gruppe_neu() {
        let (mut m, st) = stadt(3);
        let mut e = Erkennung::default();
        let mut fassung = HashMap::new();
        let vorher = e.lage(&m, &Feste(&m, fassung.clone()), st);
        // Innenwand im mittleren Haus nach x = 4 m
        let iw = m
            .runs()
            .iter()
            .filter(|(_, r)| !r.closed)
            .map(|(id, _)| id)
            .nth(1)
            .unwrap();
        let pts: Vec<Vec3> = m
            .run(iw)
            .unwrap()
            .points
            .iter()
            .map(|p| *p + vec3(1000.0, 0.0, 0.0))
            .collect();
        m.begin("Wand");
        assert!(m.set_run_points(iw, &pts).is_some());
        m.commit();
        fassung.insert(iw, 1);
        let nachher = e.lage(&m, &Feste(&m, fassung), st);
        let gleich: Vec<bool> = vorher
            .gruppen
            .iter()
            .zip(&nachher.gruppen)
            .map(|(a, b)| Rc::ptr_eq(&a.formen, &b.formen))
            .collect();
        assert_eq!(gleich, [true, false, true]);
        assert_eq!(nachher.raeume(&m), crate::raeume(&m, st));
    }

    /// Review #87 R2: Räume je Gruppe wie über das ganze Geschoss, auch wenn
    /// der Kasten einer Gruppe den einer anderen enthält (kleines Haus im
    /// Winkel zweier Häuser); eine Wand ziehen teilt nur ihre Gruppe neu
    /// zu, ein Stempel nur seine.
    #[test]
    fn zuteilung_je_gruppe() {
        let (mut m, st) = stadt(3);
        let aw = m.defaults().exterior_wall;
        let haus = |m: &mut Model, x0: f64, y0: f64, x1: f64, y1: f64| {
            let pts = [
                vec3(x0, y0, 0.0),
                vec3(x0, y1, 0.0),
                vec3(x1, y1, 0.0),
                vec3(x1, y0, 0.0),
            ];
            m.add_wall_run(&pts, true, RefSide::Left, st, aw, Category::ExteriorWall)
                .unwrap();
        };
        haus(&mut m, 0.0, 30000.0, 20000.0, 38000.0);
        haus(&mut m, 0.0, 38000.0, 8000.0, 58000.0);
        haus(&mut m, 12000.0, 45000.0, 18000.0, 52000.0);
        crate::abgleichen(&mut m, &[st]);
        let mut e = Erkennung::default();
        let mut fassung = HashMap::new();
        let gleich_wie_ohne_speicher = |e: &mut Erkennung, m: &Model, f: &HashMap<RunId, u64>| {
            let k = Feste(m, f.clone());
            let r = e.raumformen(m, &k, st);
            let mut v: Vec<RaumForm> = r.flaechen().map(|f| f.form.clone()).collect();
            let unten = |f: &RaumForm| {
                f.umriss
                    .iter()
                    .map(|k| (k.a.y, k.a.x))
                    .min_by(|a, b| a.partial_cmp(b).unwrap())
                    .unwrap()
            };
            v.sort_by(|a, b| unten(a).partial_cmp(&unten(b)).unwrap());
            assert_eq!(v, crate::raeume(m, st));
            assert!(v.iter().all(|f| f.raum.is_some()));
            for f in r.flaechen() {
                assert_eq!(f.grundflaeche, crate::flaechen::grundflaeche(&f.form));
            }
            let l = e.lage(m, &k, st);
            assert_eq!(e.stand(m, &k, st).raeume(), l.stand(m).raeume());
        };
        gleich_wie_ohne_speicher(&mut e, &m, &fassung);
        let l = e.lage(&m, &Feste(&m, fassung.clone()), st);
        assert_eq!(l.gruppen.len(), 5, "drei Häuser, ein Winkel, eins darin");
        let k: Vec<Kasten> = l.gruppen.iter().map(|g| g.kasten).collect();
        let in_einem = |b: &Kasten| {
            k.iter()
                .any(|a| a[0] < b[0] && b[2] < a[2] && a[1] < b[1] && b[3] < a[3])
        };
        assert!(k.iter().any(in_einem), "ein Gruppenkasten im anderen");
        let n0 = e.zugeteilt();
        e.raumformen(&m, &Feste(&m, fassung.clone()), st);
        assert_eq!(e.zugeteilt(), n0, "nichts geändert");
        // Innenwand im mittleren Haus nach x = 4 m
        let iw = m
            .runs()
            .iter()
            .filter(|(_, r)| !r.closed)
            .map(|(id, _)| id)
            .nth(1)
            .unwrap();
        let pts: Vec<Vec3> = m
            .run(iw)
            .unwrap()
            .points
            .iter()
            .map(|p| *p + vec3(1000.0, 0.0, 0.0))
            .collect();
        m.begin("Wand");
        assert!(m.set_run_points(iw, &pts).is_some());
        fassung.insert(iw, 1);
        let n1 = e.zugeteilt();
        gleich_wie_ohne_speicher(&mut e, &m, &fassung);
        assert_eq!(e.zugeteilt(), n1 + 1, "nur das mittlere Haus");
        m.commit();
        // Stempel im ersten Haus verschoben
        let r = e.raumformen(&m, &Feste(&m, fassung.clone()), st);
        let f = &r.gruppen[0][0].form;
        let p = f.mitte + vec3(100.0, 0.0, 0.0);
        m.begin("Stempel");
        assert!(m.set_space(f.raum.unwrap(), |s| s.stempel = Some(p)));
        m.commit();
        let n2 = e.zugeteilt();
        gleich_wie_ohne_speicher(&mut e, &m, &fassung);
        assert_eq!(e.zugeteilt(), n2 + 1, "nur das erste Haus");
        let r = e.raumformen(&m, &Feste(&m, fassung), st);
        assert_eq!(r.gruppen[0][0].form.stempel, p);
    }

    /// Fassung nach dem Inhalt der Kette: ändert sich mit jeder Änderung
    /// am Zug, auch über Anschlüsse.
    struct Inhalt<'a>(&'a Model);

    impl Ketten for Inhalt<'_> {
        fn fassung(&self, run: RunId) -> Option<u64> {
            let mut h = DefaultHasher::new();
            format!("{:?}", self.0.chain(run)).hash(&mut h);
            Some(h.finish())
        }

        fn kette(&self, run: RunId) -> Option<Cow<'_, WallChain>> {
            self.0.chain(run).map(Cow::Owned)
        }
    }

    /// Wie [`Inhalt`], aber etwa ein Drittel der Züge ohne Fassung (im
    /// Schritt geändert, noch nicht gebaut), je nach `1`.
    struct Teils<'a>(&'a Model, u64);

    impl Ketten for Teils<'_> {
        fn fassung(&self, run: RunId) -> Option<u64> {
            let ohne = (run.index() as u64 ^ self.1).is_multiple_of(3);
            Inhalt(self.0).fassung(run).filter(|_| !ohne)
        }

        fn kette(&self, run: RunId) -> Option<Cow<'_, WallChain>> {
            self.0.chain(run).map(Cow::Owned)
        }
    }

    /// Review #87 R2: Die nachgeführte Lage und Zuteilung gleicht in jeder
    /// Folge von Schritten der vollen Rechnung: Wände über die Nachbarwand
    /// und ins Nachbarhaus ziehen, Häuser zusammen- und auseinanderschieben,
    /// Züge anlegen und löschen, Räume anlegen; auch im offenen Schritt und
    /// mit Zügen ohne Fassung (R5).
    #[test]
    fn nachgefuehrt_wie_neu() {
        let (mut m, st) = stadt(4);
        let aw = m.defaults().exterior_wall;
        let iw = m.defaults().interior_wall;
        let mut e = Erkennung::default();
        let mut zufall = 0x2545_f491_4f6c_dd1du64;
        let mut wuerfel = |n: u64| {
            zufall ^= zufall << 13;
            zufall ^= zufall >> 7;
            zufall ^= zufall << 17;
            zufall % n
        };
        let vergleich = |e: &mut Erkennung, m: &Model, schritt: usize, teils: bool| {
            let (inhalt, ohne) = (Inhalt(m), Teils(m, schritt as u64));
            let k: &dyn Ketten = if teils { &ohne } else { &inhalt };
            let l = e.lage(m, k, st);
            let frisch = Erkennung::default().lage(m, m, st);
            let kurz = |l: &Lage| -> Vec<(u64, Kasten)> {
                l.gruppen.iter().map(|g| (g.schluessel, g.kasten)).collect()
            };
            assert_eq!(kurz(&l), kurz(&frisch), "Schritt {schritt}");
            let ordnen = |mut v: Vec<(RaumForm, f64)>| {
                let unten = |f: &RaumForm| (f.mitte.y, f.mitte.x);
                v.sort_by(|a, b| unten(&a.0).partial_cmp(&unten(&b.0)).unwrap());
                v
            };
            let r = e.raumformen(m, k, st);
            let r: Vec<(RaumForm, f64)> = r
                .flaechen()
                .map(|f| (f.form.clone(), f.grundflaeche))
                .collect();
            assert_eq!(
                ordnen(r),
                ordnen(frisch.mit_grundflaeche(m)),
                "Schritt {schritt}"
            );
            assert_eq!(
                e.stand(m, k, st).raeume(),
                frisch.stand(m).raeume(),
                "Schritt {schritt}"
            );
        };
        let voll0 = e.voll;
        for schritt in 0..120 {
            let zuege: Vec<RunId> = m.runs().ids().collect();
            let z = zuege[wuerfel(zuege.len() as u64) as usize];
            let offen = wuerfel(4) == 0;
            let teils = wuerfel(2) == 0;
            m.begin("Schritt");
            match wuerfel(10) {
                0..=5 => {
                    // Zug verschieben, bis ins Nachbarhaus
                    let d = vec3(
                        wuerfel(9000) as f64 - 4500.0,
                        wuerfel(3) as f64 * 700.0 - 700.0,
                        0.0,
                    );
                    let pts: Vec<Vec3> = m.run(z).unwrap().points.iter().map(|p| *p + d).collect();
                    m.set_run_points(z, &pts);
                }
                6 => {
                    m.remove_run(z);
                }
                7 => {
                    let x = wuerfel(60000) as f64;
                    let pts = [vec3(x, 300.0, 0.0), vec3(x, 7700.0, 0.0)];
                    m.add_wall_run(&pts, false, RefSide::Center, st, iw, Category::InteriorWall);
                }
                8 => {
                    let (x, y) = (wuerfel(60000) as f64, wuerfel(2) as f64 * 9000.0);
                    let pts = [
                        vec3(x, y, 0.0),
                        vec3(x, y + 5000.0, 0.0),
                        vec3(x + 4000.0, y + 5000.0, 0.0),
                        vec3(x + 4000.0, y, 0.0),
                    ];
                    m.add_wall_run(&pts, true, RefSide::Left, st, aw, Category::ExteriorWall);
                }
                _ => {
                    crate::abgleichen(&mut m, &[st]);
                }
            }
            if offen {
                // Mitten im Schritt: zweimal nachführen, dann weiter
                vergleich(&mut e, &m, schritt, teils);
                let erster = m.runs().ids().next();
                if let Some(z) = erster {
                    let d = vec3(50.0, 0.0, 0.0);
                    let pts: Vec<Vec3> = m.run(z).unwrap().points.iter().map(|p| *p + d).collect();
                    m.set_run_points(z, &pts);
                }
                vergleich(&mut e, &m, schritt, teils);
            }
            m.commit();
            vergleich(&mut e, &m, schritt, teils);
        }
        assert!(e.voll - voll0 <= 2, "{} volle Rechnungen", e.voll - voll0);
    }

    #[test]
    fn raster_findet_jeden_kasten() {
        let k: Vec<Kasten> = (0..50)
            .map(|i| {
                let x = (i % 10) as f64 * 1000.0;
                let y = (i / 10) as f64 * 700.0;
                [x, y, x + 900.0, y + 600.0]
            })
            .collect();
        let r = Raster::neu(&k);
        for (i, k) in k.iter().enumerate() {
            let p = vec3((k[0] + k[2]) / 2.0, (k[1] + k[3]) / 2.0, 0.0);
            assert!(r.bei(p).contains(&(i as u32)));
            let ecke = vec3(k[2], k[3], 0.0);
            assert!(r.bei(ecke).contains(&(i as u32)));
        }
    }

    /// Gruppen über das Raster wie über alle Paare: Häuser in Reihen und
    /// Spalten, Kästen, die sich nur an einer Kante oder Ecke berühren,
    /// ein langer Kasten über mehrere Häuser.
    #[test]
    fn gruppen_wie_ueber_alle_paare() {
        let mut k: Vec<Kasten> = Vec::new();
        for i in 0..12 {
            for j in 0..9 {
                let (x, y) = (i as f64 * 1500.0, j as f64 * 1500.0);
                k.push([x, y, x + 1000.0, y + 800.0]);
                if (i + j) % 3 == 0 {
                    // Nachbar, der genau an der Kante anliegt
                    k.push([x + 1000.0, y, x + 1500.0, y + 300.0]);
                }
                if (i * j) % 5 == 1 {
                    // nur an der Ecke
                    k.push([x - 200.0, y - 200.0, x, y]);
                }
            }
        }
        k.push([0.0, 4400.0, 9000.0, 4600.0]);
        let mut eltern: Vec<usize> = (0..k.len()).collect();
        fn wurzel(e: &mut [usize], mut i: usize) -> usize {
            while e[i] != i {
                i = e[i];
            }
            i
        }
        for i in 0..k.len() {
            for j in 0..i {
                if beruehren(&k[i], &k[j]) {
                    let (a, b) = (wurzel(&mut eltern, i), wurzel(&mut eltern, j));
                    eltern[a.max(b)] = a.min(b);
                }
            }
        }
        let mut erwartet: Vec<Vec<usize>> = Vec::new();
        for i in 0..k.len() {
            let w = wurzel(&mut eltern, i);
            match erwartet.iter_mut().find(|g| wurzel(&mut eltern, g[0]) == w) {
                Some(g) => g.push(i),
                None => erwartet.push(vec![i]),
            }
        }
        assert!(erwartet.len() > 50 && erwartet.len() < k.len());
        let (glieder, bereiche) = gruppieren(&k);
        let gruppen: Vec<Vec<usize>> = bereiche.into_iter().map(|b| glieder[b].to_vec()).collect();
        assert_eq!(gruppen, erwartet);
    }
}
