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
use sk_model::model::{AusbauWahl, SCREED_FLOOR_TYPE_GUID, SCREED_SLAB_TYPE_GUID};
use sk_model::{ElementId, ElementKind, Guid, LevelKind, Model, RunId, Space, StoreyId, WallChain};
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
        let raeume = crate::abgleich::raeume_im(m, self.geschoss, |_| true);
        self.mit_raeumen(m, &raeume, grund)
    }

    /// Wie [`Lage::mit_grundflaeche`] mit den Räumen des Geschosses, ältere
    /// zuerst ([`crate::abgleich::raeume_im`]).
    fn mit_raeumen(
        &self,
        m: &Model,
        raeume: &[(ElementId, &Space)],
        grund: bool,
    ) -> Vec<(RaumForm, f64)> {
        let fl = self.flaechen();
        let z = zuteilen(&fl, raeume);
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
                let a = if grund {
                    crate::flaechen::grundflaeche_fertig(m, &form)
                } else {
                    0.0
                };
                (form, a)
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
    /// Stand der Räume im Modell ([`Model::raum_revision`]), zu dem die
    /// Liste gilt, bis auf die Räume in `offen`.
    stand: u64,
    /// Räume, die ein offener Schritt geändert hatte, als die Liste zuletzt
    /// nachgeführt wurde: Sie gelten so, wie sie da waren, und werden beim
    /// nächsten Aufruf wieder neu eingereiht (Review H-175h).
    offen: Vec<ElementId>,
    raeume: Vec<(ElementId, Vec3)>,
}

impl Raumliste {
    /// Reiht die Räume `ids` des Geschosses `g` neu ein: heraus, und wenn
    /// es sie auf `g` noch gibt, an ihrer Stelle wieder hinein. Ergebnis:
    /// ihre Anker davor und danach. Bis 16 Räume kommen je einzeln an ihre
    /// Stelle (je ein Verschieben des Rests der Liste), mehr in einem Gang
    /// über die Liste (Review #175 B1, H-175g). `None` ohne Änderung an der
    /// Liste, wenn mehr als 16 und mindestens halb so viele Räume von `ids`
    /// auf `g` liegen, wie Räume in der Liste stehen (doppelte gezählt):
    /// dann ist neu sammeln nicht langsamer (Review H-175f). Räume anderer
    /// Geschosse zählen nicht (Review H-181a); das kostet ein Nachschlagen
    /// je Kennung, die Schlüssel erst danach.
    fn flicken(&mut self, m: &Model, g: StoreyId, ids: &[ElementId]) -> Option<Vec<Vec3>> {
        let auf_g = ids
            .iter()
            .filter(|&&id| m.element(id).is_some_and(|e| e.storey == g))
            .count();
        if auf_g > 16 && 2 * auf_g >= self.raeume.len() {
            return None;
        }
        let ids: HashSet<ElementId> = ids.iter().copied().collect();
        // Wie beim Sammeln: kurze Nummern zuerst, dann nach Nummer, bei
        // gleicher in der Reihenfolge der Bauteile
        let schluessel = |id: ElementId| {
            let n = m.element(id).map_or("", |e| e.number.as_str());
            (n.len(), n, id.index())
        };
        let mut neu: Vec<(_, ElementId, Vec3)> = ids
            .iter()
            .filter(|&&id| m.element(id).is_some_and(|e| e.storey == g))
            .filter_map(|&id| Some((schluessel(id), id, m.space(id)?.anker)))
            .collect();
        let mut punkte = Vec::new();
        self.raeume.retain(|&(id, a)| {
            let raus = ids.contains(&id);
            if raus {
                punkte.push(a);
            }
            !raus
        });
        // Jeder Schlüssel nur einmal, nicht je Vergleich (Review H-175f)
        neu.sort_unstable_by(|a, b| a.0.cmp(&b.0));
        punkte.extend(neu.iter().map(|&(_, _, a)| a));
        if neu.len() <= 16 {
            for (k, id, a) in neu {
                let pos = self.raeume.partition_point(|&(x, _)| schluessel(x) < k);
                self.raeume.insert(pos, (id, a));
            }
        } else {
            let alt = std::mem::take(&mut self.raeume);
            self.raeume.reserve(alt.len() + neu.len());
            let mut neu = neu.into_iter().peekable();
            for e in alt {
                let k = schluessel(e.0);
                while let Some((_, id, a)) = neu.next_if(|n| n.0 < k) {
                    self.raeume.push((id, a));
                }
                self.raeume.push(e);
            }
            self.raeume.extend(neu.map(|(_, id, a)| (id, a)));
        }
        Some(punkte)
    }
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

/// So viele verschobene Anker merkt sich ein Geschoss bis zum nächsten
/// Zuteilen; mehr, und alle Gruppen sehen ihre Räume neu an.
const VERSCHOBEN_MAX: usize = 256;

/// Was die Erkennung je Geschoss behält.
#[derive(Default)]
struct Geschoss {
    letzte: Option<Letzte>,
    raumliste: Option<Raumliste>,
    /// [`ausbau_kennung`] beim letzten Zuteilen.
    ausbau: Option<u64>,
    /// Seit dem letzten Zuteilen neu gesammelt: alle Gruppen sehen ihre
    /// Räume neu an.
    alle: bool,
    /// Anker der seit dem letzten Zuteilen neu eingereihten Räume, davor und
    /// danach: Gruppen, in deren Kasten einer liegt, sehen ihre Räume neu an.
    /// Höchstens [`VERSCHOBEN_MAX`], darüber `alle`.
    verschoben: Vec<Vec3>,
    /// Nach dem Schlüssel der Gruppe.
    zuteilungen: NachSchluessel<Zuteilung>,
}

/// Fingerabdruck der Räume einer Gruppe (in ihrer Reihenfolge): was die
/// Ankerregel, die Stempellage und die Grundfläche lesen (Nutzung, dazu
/// `ausbau` aus [`ausbau_kennung`]).
fn raeume_kennung(schluessel: u64, ausbau: u64, raeume: &[(ElementId, &Space)]) -> u64 {
    let mut h = DefaultHasher::new();
    schluessel.hash(&mut h);
    ausbau.hash(&mut h);
    for (id, s) in raeume {
        id.hash(&mut h);
        [s.anker.x, s.anker.y].map(f64::to_bits).hash(&mut h);
        s.stempel.map(|p| [p.x, p.y].map(f64::to_bits)).hash(&mut h);
        s.nutzung.hash(&mut h);
        // Abweichungen des Raumbuchs (RB6), je Stelle eine und geordnet
        s.ausbau.len().hash(&mut h);
        for a in &s.ausbau {
            a.stelle.hash(&mut h);
            a.wahl
                .map(|w| match w {
                    AusbauWahl::Keiner => None,
                    AusbauWahl::Typ(g) => Some(g),
                })
                .hash(&mut h);
            a.hoehe.map(f64::to_bits).hash(&mut h);
        }
    }
    h.finish()
}

/// Fingerabdruck dessen, was die Grundfläche im Fertigmaß
/// ([`crate::flaechen::grundflaeche_fertig`]) außer Form und Nutzung liest:
/// die Raumvorlagen des Projekts, die Typen mit ihren Schichtdicken
/// ([`typen_kennung`]) und der Estrich der Vorgabe des Geschosses (auf der
/// Sohle oder nicht, [`crate::ausbau::estrich_vorgabe`], Review H-188a).
/// Ändert er sich, sehen alle Gruppen des Geschosses ihre Räume neu an
/// (Bedienbarkeit 43.8). Nur das eigene Geschoss, sonst wüchse jedes Bild
/// mit dem Quadrat der Geschosse (Review #193 B1). Höhen gehen nur über den
/// Estrich ein, Ziehen eines Geschosses ändert ihn nicht.
fn ausbau_kennung(typen: u64, estrich: Guid) -> u64 {
    let mut h = DefaultHasher::new();
    (typen, estrich).hash(&mut h);
    h.finish()
}

/// Fingerabdruck der Raumvorlagen des Projekts und der Typen mit ihren
/// Schichtdicken, für [`ausbau_kennung`]: ein Gang über Vorlagen und Typen,
/// nicht über die Bauteile.
fn typen_kennung(m: &Model) -> u64 {
    let mut h = DefaultHasher::new();
    for v in m.space_types() {
        v.guid.hash(&mut h);
        [v.floor, v.screed, v.wall, v.ceiling, v.skirting].hash(&mut h);
        v.tiles.map(|t| (t.set, t.height.to_bits())).hash(&mut h);
    }
    for (id, t) in m.layer_sets().iter() {
        (id, t.guid).hash(&mut h);
        for l in &t.layers {
            l.thickness.to_bits().hash(&mut h);
        }
    }
    h.finish()
}

/// Züge, Räume und Estrich der Vorgabe je Geschoss, aus einem Gang über das
/// Modell: für Abrufe über viele Geschosse (erstes `begin` nach dem Öffnen,
/// Mengenliste). Sonst ginge jedes Geschoss über alle Züge, Räume und
/// Geschosse des Modells, die Arbeit wüchse mit dem Quadrat der Gebäude
/// (Review H-193e). Die Räume werden erst geordnet, wenn ein Geschoss seine
/// Liste neu sammeln muss.
struct Geordnet<'m> {
    m: &'m Model,
    zuege: HashMap<StoreyId, Vec<(RunId, Option<u64>)>>,
    estrich: HashMap<StoreyId, Guid>,
    typen: u64,
    raeume: OnceCell<HashMap<StoreyId, Vec<Gesammelt<'m>>>>,
}

/// Ein Raum, wie ihn die Liste eines Geschosses sammelt: Nummer, Bauteil,
/// Anker.
type Gesammelt<'m> = (&'m str, ElementId, Vec3);

impl<'m> Geordnet<'m> {
    fn aus(m: &'m Model, k: &(impl Ketten + ?Sized)) -> Self {
        let mut zuege: HashMap<StoreyId, Vec<(RunId, Option<u64>)>> = HashMap::new();
        for (id, r) in m.runs().iter() {
            zuege.entry(r.storey).or_default().push((id, k.fassung(id)));
        }
        Geordnet {
            m,
            zuege,
            estrich: estrich_je_geschoss(m),
            typen: typen_kennung(m),
            raeume: OnceCell::new(),
        }
    }

    /// Züge des Geschosses wie in [`Erkennung::lage`]: Modellreihenfolge,
    /// je mit Fassung.
    fn zuege(&self, g: StoreyId) -> &[(RunId, Option<u64>)] {
        self.zuege.get(&g).map_or(&[][..], |v| &v[..])
    }

    /// Estrich der Vorgabe wie [`crate::ausbau::estrich_vorgabe`].
    fn estrich(&self, g: StoreyId) -> Guid {
        self.estrich
            .get(&g)
            .copied()
            .unwrap_or(SCREED_FLOOR_TYPE_GUID)
    }

    /// Räume des Geschosses mit Nummer und Anker in Modellreihenfolge, wie
    /// sie [`Erkennung::liste_nachfuehren`] sammelt.
    fn raeume(&self, g: StoreyId) -> &[Gesammelt<'m>] {
        let alle = self.raeume.get_or_init(|| {
            let mut r: HashMap<StoreyId, Vec<_>> = HashMap::new();
            for (id, e) in self.m.elements().iter() {
                if let ElementKind::Space(s) = &e.kind {
                    r.entry(e.storey)
                        .or_default()
                        .push((e.number.as_str(), id, s.anker));
                }
            }
            r
        });
        alle.get(&g).map_or(&[][..], |v| &v[..])
    }
}

/// Estrich der Vorgabe je Geschoss wie [`crate::ausbau::estrich_vorgabe`],
/// mit den Oberkanten der Gründungen je Gebäude statt einem Gang über alle
/// Geschosse für jedes.
fn estrich_je_geschoss(m: &Model) -> HashMap<StoreyId, Guid> {
    let mut sohlen: HashMap<_, Vec<f64>> = HashMap::new();
    for (_, s) in m.storeys().iter() {
        if s.kind == LevelKind::Foundation {
            sohlen
                .entry(s.building)
                .or_default()
                .push(s.elevation + s.height);
        }
    }
    m.storeys()
        .iter()
        .map(|(id, s)| {
            let auf_sohle = sohlen
                .get(&s.building)
                .is_some_and(|v| v.iter().any(|ok| (ok - s.elevation).abs() < 1.0));
            let e = if auf_sohle {
                SCREED_SLAB_TYPE_GUID
            } else {
                SCREED_FLOOR_TYPE_GUID
            };
            (id, e)
        })
        .collect()
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
    raumlisten: u64,
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

    /// Wie oft die Räume eines Geschosses neu über alle Bauteile gesammelt
    /// wurden (Review H-137b, für Tests und Messung).
    pub fn raumlisten(&self) -> u64 {
        self.raumlisten
    }

    /// Vergisst die Räume aller Geschosse, der nächste Aufruf sammelt sie
    /// neu (Messung, Review #175 B1).
    #[doc(hidden)]
    pub fn raumlisten_vergessen(&mut self) {
        for gs in self.geschosse.values_mut() {
            gs.raumliste = None;
        }
    }

    /// Flächen des Geschosses `g`, je Hausgruppe. Nach der ersten Rechnung
    /// werden nur die Gruppen der geänderten Züge und der Züge ohne Fassung
    /// neu gebildet.
    pub fn lage(&mut self, m: &Model, k: &(impl Ketten + ?Sized), g: StoreyId) -> Lage {
        let jetzt: Vec<(RunId, Option<u64>)> = m
            .runs()
            .iter()
            .filter(|(_, r)| r.storey == g)
            .map(|(id, _)| (id, k.fassung(id)))
            .collect();
        self.lage_aus(m, k, g, &jetzt)
    }

    /// [`Erkennung::lage`] mit den Zügen `jetzt` des Geschosses
    /// (Modellreihenfolge, je mit Fassung).
    fn lage_aus(
        &mut self,
        m: &Model,
        k: &(impl Ketten + ?Sized),
        g: StoreyId,
        jetzt: &[(RunId, Option<u64>)],
    ) -> Lage {
        self.lauf += 1;
        if let Some(l) = self.nachfuehren(m, k, g, jetzt) {
            return l;
        }
        self.neu(m, k, g, jetzt)
    }

    /// Räume der Geschosse `geschosse` wie [`crate::raeume`] je Geschoss,
    /// in derselben Reihenfolge. Züge und Räume des Modells werden einmal
    /// nach Geschoss geordnet, eine Erkennung rechnet alle Geschosse; so
    /// wächst die Arbeit mit dem Modell, nicht mit Geschossen mal Bauteilen
    /// (Review #138 H-138f, Raumliste für den IFC-Export).
    pub fn raeume_alle(m: &Model, geschosse: &[StoreyId]) -> Vec<Vec<RaumForm>> {
        let mut zuege: HashMap<StoreyId, Vec<(RunId, Option<u64>)>> = HashMap::new();
        for (id, r) in m.runs().iter() {
            zuege
                .entry(r.storey)
                .or_default()
                .push((id, Ketten::fassung(m, id)));
        }
        let mut raeume: HashMap<StoreyId, Vec<(&str, ElementId, &Space)>> = HashMap::new();
        for (id, e) in m.elements().iter() {
            if let ElementKind::Space(s) = &e.kind {
                raeume
                    .entry(e.storey)
                    .or_default()
                    .push((e.number.as_str(), id, &**s));
            }
        }
        let raeume: HashMap<StoreyId, Vec<(ElementId, &Space)>> = raeume
            .into_iter()
            .map(|(g, v)| (g, crate::abgleich::aeltere_zuerst(v)))
            .collect();
        let mut e = Erkennung::default();
        geschosse
            .iter()
            .map(|&g| {
                let jetzt = zuege.get(&g).map_or(&[][..], |v| &v[..]);
                let r = raeume.get(&g).map_or(&[][..], |v| &v[..]);
                e.lage_aus(m, m, g, jetzt)
                    .mit_raeumen(m, r, false)
                    .into_iter()
                    .map(|(f, _)| f)
                    .collect()
            })
            .collect()
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
        // Was ein Geschoss im Speicher hat, kam aus einem früheren Aufruf
        // für dieses Geschoss; ein neues muss nichts aufräumen (so bleibt
        // [`Erkennung::raeume_alle`] linear in den Geschossen)
        let bekannt = self.geschosse.contains_key(&g);
        let mut zuege: Vec<(RunId, Rc<Roh>)> = Vec::new();
        for &(id, fassung) in jetzt {
            if let Some(roh) = self.roh(m, k, id, fassung, g) {
                if roh.kasten.is_some() {
                    zuege.push((id, roh));
                }
            }
        }
        if bekannt {
            for e in &mut self.roh {
                if e.as_ref()
                    .is_some_and(|e| e.geschoss == g && e.lauf != lauf)
                {
                    *e = None;
                }
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
        if bekannt {
            self.gruppen
                .retain(|_, e| e.geschoss != g || e.lauf == lauf);
        }
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
        self.raumformen_aus(g, &lage)
    }

    /// [`Erkennung::raumformen`] für die Geschosse `geschosse`, in derselben
    /// Reihenfolge. Züge und Räume des Modells werden dafür einmal nach
    /// Geschoss geordnet (Review H-193e, Mengenliste).
    pub fn raumformen_je(
        &mut self,
        m: &Model,
        k: &(impl Ketten + ?Sized),
        geschosse: &[StoreyId],
    ) -> Vec<Raumformen> {
        let o = Geordnet::aus(m, k);
        geschosse
            .iter()
            .map(|&g| {
                let lage = self.zuteilung_aus(m, k, g, &o);
                self.raumformen_aus(g, &lage)
            })
            .collect()
    }

    fn raumformen_aus(&mut self, g: StoreyId, lage: &Lage) -> Raumformen {
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
        self.stand_aus(g, &lage)
    }

    /// [`Erkennung::stand`] für die Geschosse `geschosse`, in derselben
    /// Reihenfolge. Züge und Räume des Modells werden dafür einmal nach
    /// Geschoss geordnet (Review H-193e, erstes `begin` nach dem Öffnen).
    pub fn staende(
        &mut self,
        m: &Model,
        k: &(impl Ketten + ?Sized),
        geschosse: &[StoreyId],
    ) -> Vec<Stand> {
        let o = Geordnet::aus(m, k);
        geschosse
            .iter()
            .map(|&g| {
                let lage = self.zuteilung_aus(m, k, g, &o);
                self.stand_aus(g, &lage)
            })
            .collect()
    }

    fn stand_aus(&mut self, g: StoreyId, lage: &Lage) -> Stand {
        let z = &self.geschosse.entry(g).or_default().zuteilungen;
        let mut st = Stand::default();
        for gr in &lage.gruppen {
            if let Some(e) = z.get(&gr.schluessel) {
                st.setzen(gr.schluessel, e.stand.clone());
            }
        }
        st
    }

    /// Räume des Geschosses `g` mit ihrem Anker, ältere zuerst wie
    /// [`crate::abgleich::raeume_im`], aus der nachgeführten Liste: auch im
    /// offenen Schritt nur die geänderten Räume neu eingereiht, nicht über
    /// alle Bauteile (Review H-181c, für [`crate::abgleichen_lage_mit`]).
    pub fn raumliste(&mut self, m: &Model, g: StoreyId) -> &[(ElementId, Vec3)] {
        self.liste_nachfuehren(m, g, true, None);
        self.geschosse
            .get(&g)
            .and_then(|gs| gs.raumliste.as_ref())
            .map_or(&[][..], |l| &l.raeume[..])
    }

    /// Führt die Liste der Räume von `g` auf den Stand des Modells. Solange
    /// sich die Räume nicht ändern ([`Model::raum_revision`], bei Schritten
    /// nur an Wänden gleich, Review H-137b), bleibt sie. Kennt das Modell
    /// die Räume, die Schritte seit der Liste geändert haben, werden nur
    /// diese neu eingereiht, dazu die des offenen Schritts von damals und
    /// von jetzt (Review H-175h); ihre Anker davor und danach sammelt
    /// `verschoben` für das nächste Zuteilen. Sonst wird neu gesammelt
    /// (dann `alle`), aus `o`, wenn gegeben; ohne `sammeln` fällt die Liste
    /// nur weg.
    fn liste_nachfuehren(&mut self, m: &Model, g: StoreyId, sammeln: bool, o: Option<&Geordnet>) {
        let stand = m.raum_revision();
        // Räume, die der offene Schritt auf diesem Geschoss ändert
        let mut schritt = if m.raum_geschosse_im_schritt().contains(&g) {
            m.raeume_im_schritt()
        } else {
            Vec::new()
        };
        let gs = self.geschosse.entry(g).or_default();
        let gilt = schritt.is_empty()
            && gs
                .raumliste
                .as_ref()
                .is_some_and(|l| l.stand == stand && l.offen.is_empty());
        if gilt {
            return;
        }
        if let Some(l) = gs.raumliste.as_mut() {
            if let Some(mut ids) = m.raeume_geaendert_seit(l.stand) {
                ids.extend_from_slice(&l.offen);
                ids.extend_from_slice(&schritt);
                if let Some(p) = l.flicken(m, g, &ids) {
                    gs.verschoben.extend(p);
                    // Ohne Zuteilen dazwischen (Abgleich in einem anderen als
                    // dem gezeigten Geschoss) wächst sie nicht ohne Grenze:
                    // dann sehen alle Gruppen neu an (Review H-193b)
                    if gs.verschoben.len() > VERSCHOBEN_MAX {
                        gs.verschoben.clear();
                        gs.alle = true;
                    }
                    l.stand = stand;
                    l.offen = schritt;
                    return;
                }
            }
        }
        gs.raumliste = None;
        if !sammeln {
            return;
        }
        self.raumlisten += 1;
        let mut raeume: Vec<(&str, ElementId, Vec3)> = Vec::new();
        if let Some(o) = o {
            raeume.extend_from_slice(o.raeume(g));
        } else {
            for (id, e) in m.elements().iter() {
                if let ElementKind::Space(s) = &e.kind {
                    if e.storey == g {
                        raeume.push((e.number.as_str(), id, s.anker));
                    }
                }
            }
        }
        raeume.sort_by(|a, b| a.0.len().cmp(&b.0.len()).then(a.0.cmp(b.0)));
        gs.raumliste = Some(Raumliste {
            stand,
            offen: std::mem::take(&mut schritt),
            raeume: raeume.iter().map(|&(_, id, a)| (id, a)).collect(),
        });
        gs.alle = true;
        gs.verschoben.clear();
    }

    /// Lage des Geschosses; je Hausgruppe stehen danach die geltenden Räume
    /// ihrer Flächen samt Formen im Speicher. Die Räume einer Gruppe sind
    /// die, deren Anker in ihrem Kasten liegt; die Ankerregel
    /// ([`zuteilen`]) gibt je Gruppe dasselbe wie über das ganze Geschoss,
    /// weil jeder Anker in höchstens einer Fläche liegt. Solange sich die
    /// Räume nicht ändern ([`Model::raum_revision`], bei Schritten nur an
    /// Wänden gleich, Review H-137b), sieht sie nur neue Gruppen an; haben
    /// Schritte oder der offene Schritt einige Räume geändert, dazu die
    /// Gruppen, in deren Kasten einer davon vorher oder nachher liegt.
    fn zuteilung(&mut self, m: &Model, k: &(impl Ketten + ?Sized), g: StoreyId) -> Lage {
        let lage = self.lage(m, k, g);
        let ausbau = ausbau_kennung(typen_kennung(m), crate::ausbau::estrich_vorgabe(m, g));
        self.lage_zuteilen(m, g, lage, ausbau, None)
    }

    /// [`Erkennung::zuteilung`] mit Zügen, Räumen und Estrich aus `o`.
    fn zuteilung_aus(
        &mut self,
        m: &Model,
        k: &(impl Ketten + ?Sized),
        g: StoreyId,
        o: &Geordnet,
    ) -> Lage {
        let lage = self.lage_aus(m, k, g, o.zuege(g));
        let ausbau = ausbau_kennung(o.typen, o.estrich(g));
        self.lage_zuteilen(m, g, lage, ausbau, Some(o))
    }

    /// Teilt die Räume den Gruppen von `lage` zu wie [`Erkennung::zuteilung`].
    fn lage_zuteilen(
        &mut self,
        m: &Model,
        g: StoreyId,
        lage: Lage,
        ausbau: u64,
        o: Option<&Geordnet>,
    ) -> Lage {
        let lauf = self.lauf;
        // Ohne Gruppe gibt es nichts zuzuteilen: dann wird nicht gesammelt
        self.liste_nachfuehren(m, g, !lage.gruppen.is_empty(), o);
        let gs = self.geschosse.entry(g).or_default();
        let verschoben = std::mem::take(&mut gs.verschoben);
        // Welche Gruppen ihre Räume neu ansehen: nach dem Neusammeln alle,
        // sonst die neuen und die mit einem geänderten Raum im Kasten
        let kaesten: Vec<Kasten> = lage.gruppen.iter().map(|gr| gr.kasten).collect();
        let mut raster = None;
        let mut je: Vec<Option<Vec<(ElementId, &Space)>>> = vec![None; lage.gruppen.len()];
        if std::mem::take(&mut gs.alle) || gs.ausbau != Some(ausbau) {
            // Neu gesammelt, oder Vorlagen oder Typen geändert: Die Grundfläche
            // jeder Gruppe kann sich ändern (Bedienbarkeit 43.8)
            je.iter_mut().for_each(|x| *x = Some(Vec::new()));
        } else {
            for (gr, x) in lage.gruppen.iter().zip(&mut je) {
                if !gs.zuteilungen.contains_key(&gr.schluessel) {
                    *x = Some(Vec::new());
                }
            }
            if !verschoben.is_empty() {
                let r = Raster::neu(&kaesten);
                for &p in &verschoben {
                    for &i in r.bei(p) {
                        if drin(&kaesten[i as usize], p) {
                            je[i as usize].get_or_insert_with(Vec::new);
                        }
                    }
                }
                raster = Some(r);
            }
        }
        gs.ausbau = Some(ausbau);
        // Räume der Gruppen dazu: bei wenigen je Gruppe aus der Liste, sonst
        // in einem Gang über die Liste und ein Raster der Kästen (Review
        // #175 B1). Kästen können sich überdecken (Haus im Winkel eines
        // anderen): dann in jeder, die Ankerregel entscheidet
        let liste = gs.raumliste.as_ref().map_or(&[][..], |l| &l.raeume[..]);
        let offen = je.iter().filter(|x| x.is_some()).count();
        if offen > 0 && offen <= 8 {
            for (i, r) in je.iter_mut().enumerate() {
                if let Some(v) = r {
                    v.extend(
                        liste
                            .iter()
                            .filter(|(_, a)| drin(&kaesten[i], *a))
                            .filter_map(|&(id, _)| m.space(id).map(|s| (id, s))),
                    );
                }
            }
        } else if offen > 0 {
            let raster = raster.unwrap_or_else(|| Raster::neu(&kaesten));
            for &(id, a) in liste {
                for &i in raster.bei(a) {
                    if let Some(v) = je[i as usize]
                        .as_mut()
                        .filter(|_| drin(&kaesten[i as usize], a))
                    {
                        if let Some(s) = m.space(id) {
                            v.push((id, s));
                        }
                    }
                }
            }
        }
        for (gr, r) in lage.gruppen.iter().zip(je) {
            let Some(r) = r else {
                if let Some(e) = gs.zuteilungen.get_mut(&gr.schluessel) {
                    e.lauf = lauf;
                }
                continue;
            };
            let kennung = raeume_kennung(gr.schluessel, ausbau, &r);
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
                        grundflaeche: crate::flaechen::grundflaeche_fertig(m, &form),
                        form,
                        kasten: f.kasten,
                    }
                })
                .collect::<Rc<[Raumflaeche]>>();
            // Dasselbe Ergebnis (etwa nach neuen Typen, die hier keine
            // Fläche ändern): die alten Formen bleiben, wer sie am Zeiger
            // erkennt, baut nicht neu
            if let Some(e) = gs
                .zuteilungen
                .get_mut(&gr.schluessel)
                .filter(|e| e.formen == formen && e.stand.iter().map(|x| x.1).eq(z.iter().copied()))
            {
                e.kennung = kennung;
                e.lauf = lauf;
                continue;
            }
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

    /// Review H-193b: Schritte, die nur Räume im OG ändern, während das EG
    /// gezeigt wird (Abgleich mit der Raumliste, kein Zuteilen im OG),
    /// merken sich höchstens [`VERSCHOBEN_MAX`] Anker; darüber sehen alle
    /// Gruppen neu an, und das Ergebnis gleicht einer frischen Erkennung.
    #[test]
    fn verschoben_hat_eine_grenze() {
        let mut m = crate::raumliste::tests::haus_roh();
        let og = m
            .storeys()
            .iter()
            .filter(|(_, s)| s.kind == sk_model::LevelKind::Storey)
            .max_by(|x, y| x.1.elevation.total_cmp(&y.1.elevation))
            .unwrap()
            .0;
        let id = m.spaces(og)[0];
        let mut e = Erkennung::default();
        e.raumformen(&m, &m, og);
        for i in 0..300 {
            m.begin("Name");
            m.set_space(id, |s| s.name = format!("Raum {i}"));
            m.commit();
            e.raumliste(&m, og);
            assert!(e.geschosse[&og].verschoben.len() <= VERSCHOBEN_MAX, "{i}");
        }
        assert!(e.geschosse[&og].alle, "über der Grenze sehen alle neu an");
        let namen = |e: &mut Erkennung| -> Vec<_> {
            e.raumformen(&m, &m, og)
                .flaechen()
                .map(|f| (f.form.raum, f.grundflaeche))
                .collect()
        };
        assert_eq!(namen(&mut e), namen(&mut Erkennung::default()));
        assert_eq!(e.raumlisten(), 1, "nur einmal gesammelt");
    }

    /// Review #193 B1 und H-188a: Ob ein Geschoss auf der Sohle steht,
    /// entscheidet über den Estrich der Vorgabe und damit, ob ein Raum
    /// Ausbau hat, wenn das Projekt nur den Estrich auf der Sohle führt und
    /// die Vorlage sonst keinen Typ. Das Geschoss rückt wie bei Wiederholen
    /// oder im Geschossfenster als Schritt von der Gründung weg und wieder
    /// darauf; der Stempel folgt wie eine frische Rechnung, ohne die Räume
    /// neu zu sammeln.
    #[test]
    fn grundflaeche_folgt_dem_estrich_der_vorgabe() {
        use sk_model::model::{SCREED_FLOOR_TYPE_GUID, SCREED_SLAB_TYPE_GUID};
        use sk_model::txn::{Change, Direction, Txn};
        use sk_model::LevelKind;
        let mut m = crate::raumliste::tests::haus_roh();
        m.begin("Typ");
        let mut v = m
            .ensure_space_type(sk_model::model::DEFAULT_SPACE_GUID)
            .and_then(|_| m.space_type(sk_model::model::DEFAULT_SPACE_GUID).cloned())
            .unwrap();
        (v.floor, v.ceiling, v.wall, v.tiles, v.skirting, v.screed) =
            (None, None, None, None, None, None);
        m.put_space_type(v);
        assert!(m.ensure_finish_type(SCREED_SLAB_TYPE_GUID).is_some());
        m.commit();
        let ls: Vec<(StoreyId, sk_model::Storey)> =
            m.storeys().iter().map(|(g, s)| (g, s.clone())).collect();
        let (eg, s) = ls
            .iter()
            .filter(|(_, s)| s.kind == LevelKind::Storey)
            .min_by(|x, y| x.1.elevation.total_cmp(&y.1.elevation))
            .cloned()
            .unwrap();
        let versetzen = |m: &mut Model, dz: f64| {
            let old = m.storey(eg).cloned();
            let mut n = s.clone();
            n.elevation += dz;
            let changes = vec![Change::Storey {
                id: eg,
                old,
                new: Some(n),
            }];
            m.apply(
                &Txn {
                    label: "Geschoss",
                    changes,
                },
                Direction::Redo,
            );
        };
        let mut e = Erkennung::default();
        for (dz, estrich) in [
            (0.0, SCREED_SLAB_TYPE_GUID),
            (500.0, SCREED_FLOOR_TYPE_GUID),
            (0.0, SCREED_SLAB_TYPE_GUID),
        ] {
            versetzen(&mut m, dz);
            assert_eq!(crate::ausbau::estrich_vorgabe(&m, eg), estrich);
            let formen = e.raumformen(&m, &m, eg);
            let f: Vec<&Raumflaeche> = formen.flaechen().collect();
            assert!(!f.is_empty());
            for f in f {
                let frisch = crate::flaechen::grundflaeche_fertig(&m, &f.form);
                assert!(
                    (f.grundflaeche - frisch).abs() < 1e-6,
                    "{dz}: {} {frisch}",
                    f.grundflaeche
                );
            }
        }
        assert_eq!(e.raumlisten(), 1, "nur einmal gesammelt");
    }

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
                assert_eq!(
                    f.grundflaeche,
                    crate::flaechen::grundflaeche_fertig(m, &f.form)
                );
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

    /// Review H-137b (BIM): Schritte an Räumen reihen nur die geänderten
    /// Räume neu ein, ohne die Räume des Geschosses neu zu sammeln, und
    /// geben dasselbe wie die Erkennung ohne Speicher: Anker rücken mit
    /// der Wand, ein Raum wechselt das Geschoss, ein neuer Raum mit
    /// kleinerer Nummer überholt einen älteren in derselben Fläche, auch
    /// über Rückgängig. Nach einer Änderung ohne Schritt wird neu gesammelt.
    #[test]
    fn raum_in_anderes_geschoss() {
        use sk_model::{Change, Direction, LevelKind, Txn};
        let mut m = Model::new();
        let b = m.add_building(2);
        let gs: Vec<StoreyId> = m
            .levels_in(Some(b))
            .into_iter()
            .filter(|&g| m.storey(g).is_some_and(|s| s.kind == LevelKind::Storey))
            .collect();
        let (eg, og) = (gs[0], gs[1]);
        let (aw, iw) = (m.defaults().exterior_wall, m.defaults().interior_wall);
        let mut innen = Vec::new();
        for g in [eg, og] {
            let pts = [
                vec3(0.0, 0.0, 0.0),
                vec3(0.0, 8000.0, 0.0),
                vec3(10000.0, 8000.0, 0.0),
                vec3(10000.0, 0.0, 0.0),
            ];
            m.add_wall_run(&pts, true, RefSide::Left, g, aw, Category::ExteriorWall)
                .unwrap();
            let iwp = [vec3(3000.0, 500.0, 0.0), vec3(3000.0, 7500.0, 0.0)];
            innen.push(
                m.add_wall_run(&iwp, false, RefSide::Center, g, iw, Category::InteriorWall)
                    .unwrap(),
            );
        }
        crate::abgleichen(&mut m, &[eg, og]);
        let mut e = Erkennung::default();
        let wie_neu = |e: &mut Erkennung, m: &Model| {
            for g in [eg, og] {
                let r = e.raumformen(m, m, g);
                let mut v: Vec<RaumForm> = r.flaechen().map(|f| f.form.clone()).collect();
                let mut w = crate::raeume(m, g);
                let x = |f: &RaumForm| (f.mitte.x, f.mitte.y);
                v.sort_by(|a, b| x(a).partial_cmp(&x(b)).unwrap());
                w.sort_by(|a, b| x(a).partial_cmp(&x(b)).unwrap());
                assert_eq!(v, w);
            }
        };
        wie_neu(&mut e, &m);
        let n0 = e.raumlisten();
        // Reiner Wandschritt und sein Rückgängig
        m.begin("Wand");
        let p: Vec<Vec3> = m.run(innen[0]).unwrap().points.clone();
        let q: Vec<Vec3> = p.iter().map(|p| *p + vec3(500.0, 0.0, 0.0)).collect();
        assert!(m.set_run_points(innen[0], &q).is_some());
        let t = m.commit().unwrap();
        m.apply(&t, Direction::Undo);
        wie_neu(&mut e, &m);
        // Wandschritt, dem die Anker folgen, und sein Rückgängig
        let r = m.raum_revision();
        m.begin("Wand");
        assert!(m.set_run_points(innen[0], &q).is_some());
        crate::abgleichen(&mut m, &[eg]);
        let t = m.commit().unwrap();
        assert!(m.raum_revision() > r, "Anker gerückt");
        wie_neu(&mut e, &m);
        m.apply(&t, Direction::Undo);
        wie_neu(&mut e, &m);
        m.apply(&t, Direction::Redo);
        wie_neu(&mut e, &m);
        // Raum des EG ins OG hängen und zurück
        let id = m.spaces(eg)[0];
        let alt = m.element(id).unwrap().clone();
        let anker = m.space(id).unwrap().anker;
        let mut neu = alt.clone();
        neu.storey = og;
        let umhaengen = Txn {
            label: "Umhängen",
            changes: vec![Change::Element {
                id,
                old: Some(alt.clone()),
                new: Some(neu),
            }],
        };
        m.apply(&umhaengen, Direction::Redo);
        assert!(m.spaces(og).contains(&id) && !m.spaces(eg).contains(&id));
        wie_neu(&mut e, &m);
        m.apply(&umhaengen, Direction::Undo);
        wie_neu(&mut e, &m);
        // Neuer Raum am Anker eines älteren: der ältere behält die Fläche,
        // bis der neue die kleinere Nummer bekommt
        m.begin("Raum");
        let x = m.add_space(eg, sk_model::Space::neu(anker, "Neu".into()));
        m.commit();
        wie_neu(&mut e, &m);
        let ex = m.element(x).unwrap().clone();
        let mut vor = ex.clone();
        vor.number = "0".into();
        let nummer = Txn {
            label: "Nummer",
            changes: vec![Change::Element {
                id: x,
                old: Some(ex),
                new: Some(vor),
            }],
        };
        m.apply(&nummer, Direction::Redo);
        let gilt = |e: &mut Erkennung, m: &Model| {
            let r = e.raumformen(m, m, eg);
            let ja = r.flaechen().any(|f| f.form.raum == Some(x));
            ja
        };
        wie_neu(&mut e, &m);
        assert!(gilt(&mut e, &m), "kleinere Nummer gilt");
        m.apply(&nummer, Direction::Undo);
        wie_neu(&mut e, &m);
        assert!(!gilt(&mut e, &m), "ältere Nummer gilt wieder");
        assert_eq!(e.raumlisten(), n0, "nur eingereiht");
        // Ohne Schritt: neu gesammelt
        assert!(m.set_space(x, |s| s.name = "Frei".into()));
        wie_neu(&mut e, &m);
        assert_eq!(e.raumlisten(), n0 + 2, "beide Geschosse neu");
    }

    /// Review H-181a: Ein Schritt an vielen Räumen eines Geschosses sammelt
    /// nur dessen Räume neu, nicht die eines anderen mit wenigen Räumen.
    #[test]
    fn viele_raeume_im_anderen_geschoss() {
        use sk_model::LevelKind;
        let mut m = Model::new();
        let b = m.add_building(2);
        let gs: Vec<StoreyId> = m
            .levels_in(Some(b))
            .into_iter()
            .filter(|&g| m.storey(g).is_some_and(|s| s.kind == LevelKind::Storey))
            .collect();
        let (eg, og) = (gs[0], gs[1]);
        let aw = m.defaults().exterior_wall;
        for g in [eg, og] {
            let pts = [
                vec3(0.0, 0.0, 0.0),
                vec3(0.0, 8000.0, 0.0),
                vec3(10000.0, 8000.0, 0.0),
                vec3(10000.0, 0.0, 0.0),
            ];
            m.add_wall_run(&pts, true, RefSide::Left, g, aw, Category::ExteriorWall)
                .unwrap();
        }
        crate::abgleichen(&mut m, &[eg, og]);
        m.begin("Räume");
        let viele: Vec<ElementId> = (0..20)
            .map(|i| {
                let p = vec3(20000.0 + 1000.0 * i as f64, 0.0, 0.0);
                m.add_space(og, sk_model::Space::neu(p, format!("R{i}")))
            })
            .collect();
        m.commit();
        let mut e = Erkennung::default();
        for g in [eg, og] {
            e.raumformen(&m, &m, g);
        }
        let n0 = e.raumlisten();
        m.begin("Namen");
        for &id in &viele {
            assert!(m.set_space(id, |s| s.name.push('x')));
        }
        m.commit();
        for g in [eg, og] {
            e.raumformen(&m, &m, g);
        }
        assert_eq!(e.raumlisten(), n0 + 1, "nur das OG neu gesammelt");
    }

    /// Review H-175h: Im offenen Schritt Stempel ziehen, einen Raum anlegen
    /// und wieder löschen; Bild für Bild wie ohne Speicher, ohne die Räume
    /// neu zu sammeln, auch nach Verwerfen und Abschließen.
    #[test]
    fn raeume_im_offenen_schritt() {
        use sk_model::LevelKind;
        let mut m = Model::new();
        let b = m.add_building(2);
        let gs: Vec<StoreyId> = m
            .levels_in(Some(b))
            .into_iter()
            .filter(|&g| m.storey(g).is_some_and(|s| s.kind == LevelKind::Storey))
            .collect();
        let (eg, og) = (gs[0], gs[1]);
        let (aw, iw) = (m.defaults().exterior_wall, m.defaults().interior_wall);
        for g in [eg, og] {
            let pts = [
                vec3(0.0, 0.0, 0.0),
                vec3(0.0, 8000.0, 0.0),
                vec3(10000.0, 8000.0, 0.0),
                vec3(10000.0, 0.0, 0.0),
            ];
            m.add_wall_run(&pts, true, RefSide::Left, g, aw, Category::ExteriorWall)
                .unwrap();
            let iwp = [vec3(3000.0, 500.0, 0.0), vec3(3000.0, 7500.0, 0.0)];
            m.add_wall_run(&iwp, false, RefSide::Center, g, iw, Category::InteriorWall)
                .unwrap();
        }
        crate::abgleichen(&mut m, &[eg, og]);
        let mut e = Erkennung::default();
        let wie_neu = |e: &mut Erkennung, m: &Model| {
            for g in [eg, og] {
                let r = e.raumformen(m, m, g);
                let mut v: Vec<RaumForm> = r.flaechen().map(|f| f.form.clone()).collect();
                let mut w = crate::raeume(m, g);
                let x = |f: &RaumForm| (f.mitte.x, f.mitte.y);
                v.sort_by(|a, b| x(a).partial_cmp(&x(b)).unwrap());
                w.sort_by(|a, b| x(a).partial_cmp(&x(b)).unwrap());
                assert_eq!(v, w);
            }
        };
        wie_neu(&mut e, &m);
        let n0 = e.raumlisten();
        let id = m.spaces(eg)[0];
        let anker = m.space(id).unwrap().anker;
        for abschliessen in [false, true] {
            m.begin("Stempel");
            for i in 1..=3 {
                let p = anker + vec3(100.0 * i as f64, 0.0, 0.0);
                assert!(m.set_space(id, |s| s.stempel = Some(p)));
                wie_neu(&mut e, &m);
            }
            // Neuer Raum mit kleinerer Nummer am selben Anker: gilt im
            // Schritt, bis er wieder gelöscht ist
            let x = m.add_space(eg, sk_model::Space::neu(anker, "Neu".into()));
            m.set_number(x, "0").unwrap();
            wie_neu(&mut e, &m);
            assert!(e
                .raumformen(&m, &m, eg)
                .flaechen()
                .any(|f| f.form.raum == Some(x)));
            m.delete_elements(&[x]);
            wie_neu(&mut e, &m);
            if abschliessen {
                m.commit();
            } else {
                m.rollback();
            }
            wie_neu(&mut e, &m);
        }
        assert_eq!(e.raumlisten(), n0, "nur eingereiht");
    }

    /// Review #175 B1: Schritte, die viele Räume in vielen Häusern ändern,
    /// reihen sie in einem Gang ein und teilen über das Raster der Kästen
    /// zu, ohne neu zu sammeln, und geben dasselbe wie die Erkennung ohne
    /// Speicher: Namen in jedem dritten Raum, Stempel in zehn Häusern, neue
    /// Räume am Anker eines älteren und neue Nummern für 40 Räume in einem
    /// Schritt, auch über Rückgängig. Ein Schritt an allen Räumen sammelt
    /// sie neu (Review H-175f).
    #[test]
    fn viele_raeume_geaendert() {
        use sk_model::{Change, Direction, Txn};
        let (mut m, st) = stadt(40);
        crate::abgleichen(&mut m, &[st]);
        let mut e = Erkennung::default();
        let wie_neu = |e: &mut Erkennung, m: &Model| {
            let r = e.raumformen(m, m, st);
            let mut v: Vec<RaumForm> = r.flaechen().map(|f| f.form.clone()).collect();
            let mut w = crate::raeume(m, st);
            let x = |f: &RaumForm| (f.mitte.x, f.mitte.y);
            v.sort_by(|a, b| x(a).partial_cmp(&x(b)).unwrap());
            w.sort_by(|a, b| x(a).partial_cmp(&x(b)).unwrap());
            assert_eq!(v, w);
        };
        wie_neu(&mut e, &m);
        let n0 = e.raumlisten();
        let raeume = m.spaces(st);
        assert_eq!(raeume.len(), 80);
        // Namen in jedem dritten Raum
        m.begin("Namen");
        for (i, &id) in raeume.iter().enumerate().step_by(3) {
            assert!(m.set_space(id, |s| s.name = format!("Zimmer {i}")));
        }
        let namen = m.commit().unwrap();
        wie_neu(&mut e, &m);
        // Stempel in zehn Häusern
        m.begin("Stempel");
        for &id in raeume.iter().step_by(2).take(10) {
            let a = m.space(id).unwrap().anker;
            assert!(m.set_space(id, |s| s.stempel = Some(a + vec3(300.0, 200.0, 0.0))));
        }
        m.commit();
        wie_neu(&mut e, &m);
        // In jedem zweiten Haus ein neuer Raum am Anker eines älteren
        m.begin("Neu");
        let neue: Vec<ElementId> = raeume
            .iter()
            .step_by(4)
            .map(|&id| {
                let a = m.space(id).unwrap().anker;
                m.add_space(st, sk_model::Space::neu(a, "Neu".into()))
            })
            .collect();
        m.commit();
        wie_neu(&mut e, &m);
        // Kleinere Nummern für die neuen, dazu alte umbenannt: 40 Räume in
        // einem Schritt
        let mut changes = Vec::new();
        for (i, &id) in neue.iter().enumerate() {
            let alt = m.element(id).unwrap().clone();
            let mut neu = alt.clone();
            neu.number = format!("{i}");
            changes.push(Change::Element {
                id,
                old: Some(alt),
                new: Some(neu),
            });
        }
        for &id in raeume.iter().skip(1).step_by(4) {
            let alt = m.element(id).unwrap().clone();
            let mut neu = alt.clone();
            neu.number = format!("{}x", neu.number);
            changes.push(Change::Element {
                id,
                old: Some(alt),
                new: Some(neu),
            });
        }
        assert_eq!(changes.len(), 40);
        let nummern = Txn {
            label: "Nummern",
            changes,
        };
        m.apply(&nummern, Direction::Redo);
        wie_neu(&mut e, &m);
        m.apply(&nummern, Direction::Undo);
        wie_neu(&mut e, &m);
        m.apply(&namen, Direction::Undo);
        wie_neu(&mut e, &m);
        assert_eq!(e.raumlisten(), n0, "nur eingereiht");
        // Alle Räume in einem Schritt: neu gesammelt
        m.begin("Alle");
        for &id in &raeume {
            assert!(m.set_space(id, |s| s.name = "Alle".into()));
        }
        m.commit();
        wie_neu(&mut e, &m);
        assert_eq!(e.raumlisten(), n0 + 1, "alle neu gesammelt");
    }

    /// Alle Geschosse in einem Durchgang wie je Geschoss (Review #138
    /// H-138f): Häuser je mit eigenem Gebäude über zwei Geschosse, ein
    /// Raum ins andere Geschoss gehängt, neue am Anker eines älteren mit
    /// kleinerer und mit größerer Nummer, ein Geschoss ohne Wände;
    /// Geschosse in anderer Reihenfolge als im Modell und eines doppelt.
    #[test]
    fn alle_geschosse_wie_je_geschoss() {
        use sk_model::{Change, Direction, LevelKind, Txn};
        let mut m = Model::new();
        let (aw, iw) = (m.defaults().exterior_wall, m.defaults().interior_wall);
        let t: f64 = m.wall_layers(aw).iter().map(|l| l.thickness).sum();
        let geschosse = |m: &Model, b| -> Vec<StoreyId> {
            m.levels_in(Some(b))
                .into_iter()
                .filter(|&g| m.storey(g).is_some_and(|s| s.kind == LevelKind::Storey))
                .collect()
        };
        let mut gs = Vec::new();
        for i in 0..3 {
            let b = m.add_building(2);
            let ox = i as f64 * 15000.0;
            let p = |x: f64, y: f64| vec3(ox + x, y, 0.0);
            for g in geschosse(&m, b) {
                gs.push(g);
                let pts = [
                    p(0.0, 0.0),
                    p(0.0, 8000.0),
                    p(10000.0, 8000.0),
                    p(10000.0, 0.0),
                ];
                m.add_wall_run(&pts, true, RefSide::Left, g, aw, Category::ExteriorWall)
                    .unwrap();
                let x = 3000.0 + 1000.0 * i as f64;
                let iwp = [p(x, t), p(x, 8000.0 - t)];
                m.add_wall_run(&iwp, false, RefSide::Center, g, iw, Category::InteriorWall)
                    .unwrap();
            }
        }
        // Ein Gebäude ohne Wände
        let b = m.add_building(1);
        let leer = geschosse(&m, b)[0];
        gs.push(leer);
        crate::abgleichen(&mut m, &gs);
        assert!(gs[..6].iter().all(|g| m.spaces(*g).len() == 2));
        let umsetzen = |m: &mut Model, id: ElementId, f: &dyn Fn(&mut sk_model::Element)| {
            let alt = m.element(id).unwrap().clone();
            let mut neu = alt.clone();
            f(&mut neu);
            let changes = vec![Change::Element {
                id,
                old: Some(alt),
                new: Some(neu),
            }];
            m.apply(
                &Txn {
                    label: "Test",
                    changes,
                },
                Direction::Redo,
            );
        };
        // Raum des ersten EG ins OG des zweiten Hauses
        let id = m.spaces(gs[0])[0];
        let anker = m.space(id).unwrap().anker;
        umsetzen(&mut m, id, &|e| e.storey = gs[3]);
        // Neuer Raum am Anker eines älteren, mit kleinerer Nummer
        let a2 = m.space(m.spaces(gs[0])[0]).unwrap().anker;
        let x = m.add_space(gs[0], sk_model::Space::neu(a2, "Neu".into()));
        umsetzen(&mut m, x, &|e| e.number = "0".into());
        // Neuer Raum am Anker eines älteren, mit größerer Nummer
        let alt = m.spaces(gs[2])[0];
        let a3 = m.space(alt).unwrap().anker;
        let y = m.add_space(gs[2], sk_model::Space::neu(a3, "Später".into()));
        umsetzen(&mut m, y, &|e| e.number = "R-999".into());
        // Raum im Geschoss ohne Wände
        m.add_space(leer, sk_model::Space::neu(anker, "Ohne".into()));

        let reihe = [leer, gs[5], gs[2], gs[0], gs[3], gs[1], gs[4], gs[0]];
        let alle = Erkennung::raeume_alle(&m, &reihe);
        assert_eq!(alle.len(), reihe.len());
        for (g, v) in reihe.iter().zip(&alle) {
            assert_eq!(*v, crate::raeume(&m, *g), "{g:?}");
        }
        assert!(alle[0].is_empty(), "ohne Wände keine Fläche");
        assert!(alle[3].iter().any(|f| f.raum == Some(x)), "kleinere Nummer");
        assert!(alle[2].iter().any(|f| f.raum == Some(alt)), "ältere Nummer");
        assert!(alle[2].iter().all(|f| f.raum != Some(y)));
        assert!(alle[3].iter().any(|f| f.raum.is_none()), "Anker ins OG");
        assert!(
            alle[4].iter().all(|f| f.raum != Some(id)),
            "Anker im OG fremd"
        );
        assert_eq!(alle.iter().map(Vec::len).sum::<usize>(), 7 * 2);
        // Nach einem Schritt an Räumen: wieder je Geschoss gleich
        m.begin("Raum");
        let z = m.add_space(gs[4], sk_model::Space::neu(a3, "Fremd".into()));
        assert!(m.set_space(alt, |s| s.anker = a2));
        m.commit();
        let alle = Erkennung::raeume_alle(&m, &reihe);
        for (g, v) in reihe.iter().zip(&alle) {
            assert_eq!(*v, crate::raeume(&m, *g), "{g:?}");
        }
        assert!(alle[2].iter().any(|f| f.raum == Some(y)), "Anker gerückt");
        assert!(alle[6].iter().all(|f| f.raum != Some(z)));
    }

    /// Review H-193e: Stand und Räume vieler Geschosse in einem Gang wie je
    /// Geschoss mit einer frischen Erkennung: Häuser je mit eigenem Gebäude
    /// über zwei Geschosse auf der Gründung, ein Geschoss ohne Wände,
    /// Geschosse in anderer Reihenfolge als im Modell und eines doppelt;
    /// auch im offenen Schritt an Räumen und danach. Der Estrich der Vorgabe
    /// je Geschoss ist der für jedes einzeln.
    #[test]
    fn staende_wie_je_geschoss() {
        use sk_model::LevelKind;
        let mut m = Model::new();
        let (aw, iw) = (m.defaults().exterior_wall, m.defaults().interior_wall);
        let t: f64 = m.wall_layers(aw).iter().map(|l| l.thickness).sum();
        let mut gs = Vec::new();
        for i in 0..3 {
            let b = m.add_building(2);
            let ox = i as f64 * 15000.0;
            let p = |x: f64, y: f64| vec3(ox + x, y, 0.0);
            for g in m.levels_in(Some(b)) {
                if !m.storey(g).is_some_and(|s| s.kind == LevelKind::Storey) {
                    continue;
                }
                gs.push(g);
                let pts = [
                    p(0.0, 0.0),
                    p(0.0, 8000.0),
                    p(10000.0, 8000.0),
                    p(10000.0, 0.0),
                ];
                m.add_wall_run(&pts, true, RefSide::Left, g, aw, Category::ExteriorWall)
                    .unwrap();
                let iwp = [p(3000.0, t), p(3000.0, 8000.0 - t)];
                m.add_wall_run(&iwp, false, RefSide::Center, g, iw, Category::InteriorWall)
                    .unwrap();
            }
        }
        let b = m.add_building(1);
        let leer = m.levels_in(Some(b))[0];
        crate::abgleichen(&mut m, &gs);
        assert!(gs.iter().all(|g| m.spaces(*g).len() == 2));

        let estrich = estrich_je_geschoss(&m);
        for (g, _) in m.storeys().iter() {
            assert_eq!(estrich[&g], crate::ausbau::estrich_vorgabe(&m, g), "{g:?}");
        }
        let arten: HashSet<Guid> = gs.iter().map(|g| estrich[g]).collect();
        assert_eq!(arten.len(), 2, "EG auf der Sohle, OG auf der Decke");

        let reihe = [leer, gs[5], gs[2], gs[0], gs[3], gs[1], gs[4], gs[0]];
        let gleich = |m: &Model, e: &mut Erkennung| -> usize {
            let staende = e.staende(m, m, &reihe);
            let formen = e.raumformen_je(m, m, &reihe);
            let flaechen = |f: &Raumformen| {
                let mut v: Vec<String> = f.flaechen().map(|x| format!("{x:?}")).collect();
                v.sort();
                v
            };
            for ((g, st), fo) in reihe.iter().zip(&staende).zip(&formen) {
                let mut frisch = Erkennung::default();
                let z = frisch.stand(m, m, *g).zuordnung();
                assert_eq!(st.zuordnung(), z, "{g:?}");
                assert_eq!(fo.geschoss, *g);
                assert_eq!(
                    flaechen(fo),
                    flaechen(&frisch.raumformen(m, m, *g)),
                    "{g:?}"
                );
            }
            staende.iter().map(Stand::raeume).sum()
        };
        let mut e = Erkennung::default();
        assert_eq!(gleich(&m, &mut e), 7 * 2, "sechs Geschosse, eines doppelt");
        // Im offenen Schritt: ein Anker rückt in die Nachbarfläche, ein
        // neuer Raum kommt dazu
        m.begin("Raum");
        let raeume = m.spaces(gs[2]);
        let a = m.space(raeume[1]).unwrap().anker;
        assert!(m.set_space(raeume[0], |s| s.anker = a));
        m.add_space(gs[4], sk_model::Space::neu(a, "Neu".into()));
        gleich(&m, &mut e);
        gleich(&m, &mut Erkennung::default());
        m.commit();
        gleich(&m, &mut e);
        gleich(&m, &mut Erkennung::default());
    }

    /// Bedienbarkeit 43.8: Die Grundfläche am Stempel ist die des
    /// Mengenblatts, auch nach einem Wechsel der Nutzung und nach einer
    /// geänderten Raumvorlage, ohne die Räume neu zu sammeln.
    #[test]
    fn grundflaeche_wie_im_mengenblatt() {
        use sk_model::model::{BATH_SPACE_GUID, DEFAULT_SPACE_GUID};
        let mut m = crate::raumliste::tests::haus();
        let mut e = Erkennung::default();
        let gleich = |e: &mut Erkennung, m: &Model| {
            for (g, _) in m.storeys().iter() {
                let mengen = crate::mengen::raeume(m, g);
                let r = e.raumformen(m, m, g);
                for f in r.flaechen() {
                    let Some(id) = f.form.raum else { continue };
                    let q = &mengen.iter().find(|x| x.0 == id).unwrap().1;
                    assert!((f.grundflaeche - q.area).abs() < 1e-6, "{id:?}");
                }
            }
        };
        gleich(&mut e, &m);
        let n0 = e.raumlisten();
        let g = m.storeys().iter().next().unwrap().0;
        let id = m.spaces(g)[0];
        let vorher = |e: &mut Erkennung, m: &Model| {
            e.raumformen(m, m, g)
                .flaechen()
                .find(|f| f.form.raum == Some(id))
                .unwrap()
                .grundflaeche
        };
        let a0 = vorher(&mut e, &m);
        // Nutzung Bad mit Fliesen: kleiner
        m.begin("Bad");
        assert!(m.ensure_space_type(BATH_SPACE_GUID).is_some());
        assert!(m.set_space(id, |s| s.nutzung = Some(BATH_SPACE_GUID)));
        assert!(crate::ausbau::typen_anlegen(&mut m, id));
        m.commit();
        gleich(&mut e, &m);
        let a1 = vorher(&mut e, &m);
        assert!(a1 < a0, "Fliese abgezogen: {a1} {a0}");
        // Vorlage ohne Wandbekleidung: Die übrigen Räume werden größer, ohne
        // dass sich an ihnen etwas ändert
        let andere = |e: &mut Erkennung, m: &Model| -> f64 {
            let gs: Vec<StoreyId> = m.storeys().iter().map(|(g, _)| g).collect();
            gs.into_iter()
                .flat_map(|g| {
                    e.raumformen(m, m, g)
                        .flaechen()
                        .filter(|f| f.form.raum.is_some_and(|r| r != id))
                        .map(|f| f.grundflaeche)
                        .collect::<Vec<_>>()
                })
                .sum()
        };
        let b0 = andere(&mut e, &m);
        m.begin("Ohne Wand");
        let mut w = m.space_type(DEFAULT_SPACE_GUID).unwrap().clone();
        w.wall = None;
        assert!(m.put_space_type(w));
        m.commit();
        gleich(&mut e, &m);
        let b1 = andere(&mut e, &m);
        assert!(b1 > b0, "ohne Putz größer: {b1} {b0}");
        assert_eq!(e.raumlisten(), n0, "nur eingereiht");
    }

    /// Die Grundfläche am Stempel folgt den Abweichungen des Raums (RB6)
    /// und der Dicke ihres Typs wie das Mengenblatt, ohne die Räume neu zu
    /// sammeln, auch wenn sich an Vorlagen und Typen nichts ändert;
    /// Rückgängig stellt sie wieder her.
    #[test]
    fn grundflaeche_folgt_den_abweichungen() {
        use sk_model::model::{AusbauAbweichung, AusbauStelle, WALL_TILES_TYPE_GUID};
        use sk_model::txn::Direction;
        let mut m = crate::raumliste::tests::haus();
        let mut e = Erkennung::default();
        let g = m.storeys().iter().next().unwrap().0;
        let id = m.spaces(g)[0];
        let flaeche = |e: &mut Erkennung, m: &Model| {
            let q = crate::mengen::raeume(m, g)
                .into_iter()
                .find(|x| x.0 == id)
                .unwrap()
                .1;
            let f = e
                .raumformen(m, m, g)
                .flaechen()
                .find(|f| f.form.raum == Some(id))
                .unwrap()
                .grundflaeche;
            assert!((f - q.area).abs() < 1e-6, "{f} {}", q.area);
            f
        };
        let a0 = flaeche(&mut e, &m);
        let n0 = e.raumlisten();
        // Wandfliesen an allen Wandseiten: kleiner
        m.begin("Fliesen");
        let fliesen = AusbauAbweichung {
            stelle: AusbauStelle::Fliesen(None),
            wahl: Some(AusbauWahl::Typ(WALL_TILES_TYPE_GUID)),
            hoehe: None,
        };
        m.set_space_finish(id, fliesen).unwrap();
        m.commit();
        let a1 = flaeche(&mut e, &m);
        assert!(a1 < a0, "Fliese abgezogen: {a1} {a0}");
        // Ohne und wieder mit der Abweichung, der Typ bleibt im Projekt:
        // Nur der Raum ändert sich
        for (wahl, soll) in [(None, a0), (fliesen.wahl, a1)] {
            m.begin("Abweichung");
            m.set_space_finish(id, AusbauAbweichung { wahl, ..fliesen })
                .unwrap();
            m.commit();
            assert!((flaeche(&mut e, &m) - soll).abs() < 1e-6);
        }
        // Der Fliesentyp wird dicker: noch kleiner
        let t = m.type_by_guid(WALL_TILES_TYPE_GUID).unwrap();
        let mut set = m.layer_set(t).unwrap().clone();
        set.layers[0].thickness += 10.0;
        m.begin("Dicker");
        assert!(m.set_layer_set(t, set));
        let tx2 = m.commit().unwrap();
        let a2 = flaeche(&mut e, &m);
        assert!(a2 < a1, "dickere Fliese: {a2} {a1}");
        m.apply(&tx2, Direction::Undo);
        assert!((flaeche(&mut e, &m) - a1).abs() < 1e-6);
        assert_eq!(e.raumlisten(), n0, "nur eingereiht");
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
