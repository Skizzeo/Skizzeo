//! Verzeichnis für lesende Gänge über das Modell (Mengenliste, Kosten,
//! AVA, Export, Neuberechnung der Wandzüge in der Szene): Decken,
//! Gründung, gestapelte Züge und Anschlüsse je Zug, dazu ein Raster über
//! Öffnungen und Züge. Ohne es sucht jeder Wandkörper das ganze Modell ab,
//! und ein Gang über alle Züge wächst mit dem Quadrat der Modellgröße.
//! Methoden von `Model`, aus `model.rs` ausgelagert.
//!
//! Das Verzeichnis gilt nur in [`Model::lesend`]: Solange es steht, hält
//! der Gang das Modell geliehen, ändern kann es niemand.
//!
//! Zwischen den Gängen hält das Modell es fest und führt es nach (L40,
//! L31): Bauteile, Züge und Geschosse liegen in [`Erfasst`], das jede
//! Änderung mitschreibt, gleich wo sie geschieht (im Schritt, beim Laden,
//! in Rückgängig, im Test); die Anschlüsse in [`JoinListe`]. Der nächste
//! Gang trägt nur die geänderten Einträge neu ein. Neu gebaut wird es nur
//! beim ersten Gang und nach sehr vielen Änderungen auf einmal.

use super::*;
use crate::id::Id;
use crate::{BuildingId, StoreyId};
use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::hash::Hash;
use std::ops::Deref;
use std::sync::Arc;

/// Was die Suchen über das ganze Modell liefern, je Schlüssel in der
/// Reihenfolge der Bauteile bzw. Züge (nach ihrem Platz in der Arena),
/// damit jede Suche mit Verzeichnis dasselbe gibt wie ohne. Jede Liste ist
/// geordnet und keine leer: Ein nachgeführtes Verzeichnis gleicht dem neu
/// gebauten aufs Bit.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Verzeichnis {
    /// [`Model::floors_of`].
    decken: HashMap<RunId, Vec<ElementId>>,
    /// [`Model::slabs_of`].
    platten: HashMap<RunId, Vec<ElementId>>,
    /// [`Model::footings_of`].
    schuerzen: HashMap<ElementId, Vec<ElementId>>,
    /// [`crate::qto::gruendung_von`]: Platte, Schürze und Perimeterdämmung
    /// je Zug.
    gruendung: HashMap<RunId, Vec<ElementId>>,
    /// Schürzen und Perimeterdämmung je Bauteil, an dem sie hängen
    /// (`slab`): Ändert es sich, wechseln sie mit ihm den Zug.
    an_platte: HashMap<ElementId, Vec<ElementId>>,
    /// Bauteile an einer Decke (`floor`): Untersichtdämmung, Dachterrasse,
    /// Blech, Flachdach, Randdämmstreifen. Die Abgleiche beim Schließen
    /// eines Schritts finden sie über die Decken im Bereich.
    an_decke: HashMap<ElementId, Vec<ElementId>>,
    /// Schlüssel aus `an_decke`, die keine Decke (mehr) sind, etwa im
    /// Schritt entfernt: Ihre Bauteile gehören zu jedem Abgleich, der sie
    /// entfernen muss.
    waisen: HashSet<ElementId>,
    /// Randdämmstreifen je Wand, auf der sie liegen.
    an_wand: HashMap<ElementId, Vec<ElementId>>,
    /// Schlüssel aus `an_wand`, deren Wand es nicht mehr gibt.
    waisen_wand: HashSet<ElementId>,
    /// Regenfallrohre je Dachaufbau (`roof`).
    an_dach: HashMap<ElementId, Vec<ElementId>>,
    /// Schlüssel aus `an_dach`, die kein Dachaufbau (mehr) sind.
    waisen_dach: HashSet<ElementId>,
    /// Gekoppelte Wände mit ihrem Zug je Segment darunter
    /// ([`Model::runs_above`]).
    gekoppelt: HashMap<ElementId, Vec<(ElementId, RunId)>>,
    /// Anschlüsse an einen Zug als Wirt oder Partner (`b_run`), nach
    /// ihrem Schlüssel `(a_run, a_end)` geordnet wie die Anschlüsse selbst;
    /// die als `a_run` findet die Suche in der geordneten Liste.
    b_seite: HashMap<RunId, Vec<(RunId, JoinEnd)>>,
    /// Öffnungen je Gebäude über dem Rechteck ihres Pfades.
    oeffnungen: Raster<Option<BuildingId>, Element>,
    /// Züge je Geschoss über dem Rechteck ihrer Punkte.
    zuege: Raster<StoreyId, WallRun>,
    /// Alle Züge über dem Rechteck ihrer Punkte.
    alle_zuege: Raster<(), WallRun>,
    /// Was jedes Bauteil eingetragen hat, zum Austragen.
    beitrag: HashMap<ElementId, Vec<Eintrag>>,
    /// Gebäude je Geschoss, unter dem die Öffnungen stehen.
    haus: HashMap<StoreyId, Option<BuildingId>>,
    /// Öffnungen je Geschoss nach ihrem Platz: Wechselt es das Gebäude,
    /// wandern sie mit. Geordnet statt als Liste, denn eine Siedlung kann
    /// ein Geschoss sein, und Ziehen an einer Öffnung trägt sie aus und
    /// wieder ein (L40).
    auf_geschoss: HashMap<StoreyId, BTreeMap<u32, ElementId>>,
    /// Exemplare aus Erweiterungen, nach ihrem Platz ([`Model::exemplare`]).
    exemplare: Vec<ElementId>,
}

/// Ein Eintrag eines Bauteils im [`Verzeichnis`].
#[derive(Clone, Copy, Debug, PartialEq)]
enum Eintrag {
    Decke(RunId),
    Platte(RunId),
    Schuerze(ElementId),
    Gruendung(RunId),
    AnPlatte(ElementId),
    AnDecke(ElementId),
    AnWand(ElementId),
    AnDach(ElementId),
    Gekoppelt(ElementId),
    Oeffnung(StoreyId),
    Exemplar,
}

/// Trägt `v` unter `k` ein, nach dem Platz geordnet.
fn rein<K: Hash + Eq, V>(map: &mut HashMap<K, Vec<V>>, k: K, v: V, platz: impl Fn(&V) -> u32) {
    let liste = map.entry(k).or_default();
    let p = platz(&v);
    let i = liste.partition_point(|x| platz(x) < p);
    liste.insert(i, v);
}

/// Trägt unter `k` aus, was `weg` trifft; eine leere Liste fällt weg.
fn raus<K: Hash + Eq, V>(map: &mut HashMap<K, Vec<V>>, k: &K, weg: impl Fn(&V) -> bool) {
    if let Some(liste) = map.get_mut(k) {
        liste.retain(|x| !weg(x));
        if liste.is_empty() {
            map.remove(k);
        }
    }
}

fn platz<T>(id: &Id<T>) -> u32 {
    id.index()
}

/// Kantenlänge eines Rasterfelds (mm), etwa ein Haus.
const FELD: f64 = 10_000.0;
/// Rechtecke über mehr Felder stehen in jedem Feld (sehr lange Züge).
const MAX_FELDER: i64 = 4096;

/// Rechtecke (x/y) der Einträge `Id<X>` je Schlüssel `K`, dazu ein Raster:
/// je Feld die Einträge, deren Rechteck es trifft, nach dem Platz geordnet.
struct Raster<K, X> {
    rahmen: HashMap<Id<X>, (K, Vec3, Vec3)>,
    felder: HashMap<(K, i64, i64), Vec<Id<X>>>,
    ueberall: HashMap<K, Vec<Id<X>>>,
}

impl<K: Hash + Eq, X> PartialEq for Raster<K, X> {
    fn eq(&self, o: &Self) -> bool {
        self.rahmen == o.rahmen && self.felder == o.felder && self.ueberall == o.ueberall
    }
}

impl<K, X> Default for Raster<K, X> {
    fn default() -> Self {
        Raster {
            rahmen: HashMap::new(),
            felder: HashMap::new(),
            ueberall: HashMap::new(),
        }
    }
}

impl<K: Clone, X> Clone for Raster<K, X> {
    fn clone(&self) -> Self {
        Raster {
            rahmen: self.rahmen.clone(),
            felder: self.felder.clone(),
            ueberall: self.ueberall.clone(),
        }
    }
}

impl<K, X> std::fmt::Debug for Raster<K, X> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Raster({})", self.rahmen.len())
    }
}

/// Felder des Rechtecks `lo`–`hi` als Spannen, `None` ohne Fläche.
fn spannen(lo: Vec3, hi: Vec3) -> Option<(i64, i64, i64, i64)> {
    let k = |x: f64| (x / FELD).floor() as i64;
    (lo.x <= hi.x && lo.y <= hi.y).then(|| (k(lo.x), k(hi.x), k(lo.y), k(hi.y)))
}

/// Trifft das Rechteck mehr Felder, als einzeln eingetragen werden?
fn viele((x0, x1, y0, y1): (i64, i64, i64, i64)) -> bool {
    (x1 - x0 + 1).saturating_mul(y1 - y0 + 1) > MAX_FELDER
}

impl<K: Hash + Eq + Copy, X> Raster<K, X> {
    /// Trägt `t` mit dem Rechteck seiner Punkte ein (ohne Punkte nicht: es
    /// trifft nichts).
    fn rein(&mut self, k: K, t: Id<X>, pts: impl Iterator<Item = Vec3>) {
        let mut lo = vec3(f64::INFINITY, f64::INFINITY, 0.0);
        let mut hi = vec3(f64::NEG_INFINITY, f64::NEG_INFINITY, 0.0);
        for p in pts {
            (lo.x, lo.y) = (lo.x.min(p.x), lo.y.min(p.y));
            (hi.x, hi.y) = (hi.x.max(p.x), hi.y.max(p.y));
        }
        let Some(s) = spannen(lo, hi) else {
            return;
        };
        self.rahmen.insert(t, (k, lo, hi));
        if viele(s) {
            rein(&mut self.ueberall, k, t, platz);
            return;
        }
        for x in s.0..=s.1 {
            for y in s.2..=s.3 {
                rein(&mut self.felder, (k, x, y), t, platz);
            }
        }
    }

    /// Trägt `t` wieder aus.
    fn raus(&mut self, t: Id<X>) {
        let Some((k, lo, hi)) = self.rahmen.remove(&t) else {
            return;
        };
        let Some(s) = spannen(lo, hi) else {
            return;
        };
        if viele(s) {
            raus(&mut self.ueberall, &k, |x| *x == t);
            return;
        }
        for x in s.0..=s.1 {
            for y in s.2..=s.3 {
                raus(&mut self.felder, &(k, x, y), |x| *x == t);
            }
        }
    }

    /// Alles unter `k`, dessen Rechteck das Rechteck `lo`–`hi` trifft (Rand
    /// eingeschlossen), nach dem Platz geordnet.
    fn bei(&self, k: K, lo: Vec3, hi: Vec3) -> Vec<Id<X>> {
        let Some(s) = spannen(lo, hi) else {
            return Vec::new();
        };
        let mut ids: Vec<Id<X>> = Vec::new();
        if viele(s) {
            ids.extend(
                self.rahmen
                    .iter()
                    .filter(|(_, r)| r.0 == k)
                    .map(|(t, _)| *t),
            );
        } else {
            ids.extend(self.ueberall.get(&k).into_iter().flatten());
            for x in s.0..=s.1 {
                for y in s.2..=s.3 {
                    ids.extend(self.felder.get(&(k, x, y)).into_iter().flatten());
                }
            }
        }
        ids.sort_unstable_by_key(platz);
        ids.dedup();
        ids.retain(|t| {
            let (_, a, z) = self.rahmen[t];
            a.x <= hi.x && lo.x <= z.x && a.y <= hi.y && lo.y <= z.y
        });
        ids
    }
}

impl Verzeichnis {
    /// Baut das Verzeichnis über das ganze Modell.
    fn neu(m: &Model) -> Verzeichnis {
        let mut v = Verzeichnis::default();
        for (id, s) in m.storeys.iter() {
            v.haus.insert(id, s.building);
        }
        for (id, r) in m.runs.iter() {
            v.zug_rein(id, r);
        }
        for (id, e) in m.elements.iter() {
            v.bauteil_rein(m, id, e);
        }
        let keys: Vec<ElementId> = v
            .an_decke
            .keys()
            .chain(v.an_wand.keys())
            .chain(v.an_dach.keys())
            .copied()
            .collect();
        for f in keys {
            v.waise_pruefen(m, f);
        }
        v.b_seite_neu(&m.joins);
        v
    }

    /// Steht unter `f` ein Bauteil, obwohl `f` keine Decke bzw. kein
    /// Dachaufbau ist oder die Wand fehlt?
    fn waise_pruefen(&mut self, m: &Model, f: ElementId) {
        let e = m.elements.get(f);
        let decke = matches!(e.map(|e| &e.kind), Some(ElementKind::Floor(_)));
        if self.an_decke.contains_key(&f) && !decke {
            self.waisen.insert(f);
        } else {
            self.waisen.remove(&f);
        }
        if self.an_wand.contains_key(&f) && e.is_none() {
            self.waisen_wand.insert(f);
        } else {
            self.waisen_wand.remove(&f);
        }
        let dach = matches!(e.map(|e| &e.kind), Some(ElementKind::Roof { .. }));
        if self.an_dach.contains_key(&f) && !dach {
            self.waisen_dach.insert(f);
        } else {
            self.waisen_dach.remove(&f);
        }
    }

    /// Decken und Wände, unter denen die Bauteile `ids` eingetragen sind.
    fn wirte_von(&self, ids: &[ElementId]) -> Vec<ElementId> {
        ids.iter()
            .flat_map(|id| self.beitrag.get(id).into_iter().flatten())
            .filter_map(|b| match b {
                Eintrag::AnDecke(f) | Eintrag::AnWand(f) | Eintrag::AnDach(f) => Some(*f),
                _ => None,
            })
            .collect()
    }

    /// Trägt die geänderten Bauteile `bauteile`, Züge `zuege` und Geschosse
    /// `geschosse` neu ein, mit allem, was an einem geänderten Bauteil oder
    /// Geschoss hängt.
    fn nachfuehren(
        &mut self,
        m: &Model,
        bauteile: &[ElementId],
        zuege: &[RunId],
        geschosse: &[StoreyId],
    ) {
        let mut arbeit: Vec<ElementId> = Vec::new();
        let mut drin: HashSet<ElementId> = HashSet::new();
        for &s in geschosse {
            let haus = m.storey(s).and_then(|x| x.building);
            if self.haus.get(&s).copied().flatten() != haus {
                arbeit.extend(
                    self.auf_geschoss
                        .get(&s)
                        .into_iter()
                        .flat_map(|o| o.values()),
                );
            }
            match m.storey(s) {
                Some(x) => self.haus.insert(s, x.building),
                None => self.haus.remove(&s),
            };
        }
        arbeit.extend(bauteile);
        arbeit.retain(|&e| drin.insert(e));
        let mut k = 0;
        while k < arbeit.len() {
            let x = arbeit[k];
            k += 1;
            for &d in self.an_platte.get(&x).into_iter().flatten() {
                if drin.insert(d) {
                    arbeit.push(d);
                }
            }
        }
        let mut pruefen = self.wirte_von(&arbeit);
        for &id in &arbeit {
            self.bauteil_raus(id);
        }
        for &id in &arbeit {
            if let Some(e) = m.elements.get(id) {
                self.bauteil_rein(m, id, e);
            }
        }
        pruefen.extend(self.wirte_von(&arbeit));
        pruefen.extend(&arbeit);
        for f in pruefen {
            self.waise_pruefen(m, f);
        }
        let zuege: HashSet<RunId> = zuege.iter().copied().collect();
        for &id in &zuege {
            self.zuege.raus(id);
            self.alle_zuege.raus(id);
        }
        for &id in &zuege {
            if let Some(r) = m.runs.get(id) {
                self.zug_rein(id, r);
            }
        }
    }

    fn zug_rein(&mut self, id: RunId, r: &WallRun) {
        self.zuege.rein(r.storey, id, r.points.iter().copied());
        self.alle_zuege.rein((), id, r.points.iter().copied());
    }

    fn bauteil_rein(&mut self, m: &Model, id: ElementId, e: &Element) {
        let mut b = Vec::new();
        if matches!(
            e.kind,
            ElementKind::GroundSlab(_)
                | ElementKind::StripFooting(_)
                | ElementKind::PerimeterInsulation { .. }
        ) {
            if let Some(r) = m.run_of(id) {
                rein(&mut self.gruendung, r, id, platz);
                b.push(Eintrag::Gruendung(r));
            }
        }
        match &e.kind {
            ElementKind::Floor(f) => {
                rein(&mut self.decken, f.run, id, platz);
                b.push(Eintrag::Decke(f.run));
            }
            ElementKind::Wall(Wall {
                run,
                coupling: Some(c),
                ..
            }) => {
                rein(&mut self.gekoppelt, c.below, (id, *run), |x| x.0.index());
                b.push(Eintrag::Gekoppelt(c.below));
            }
            ElementKind::GroundSlab(p) => {
                rein(&mut self.platten, p.run, id, platz);
                b.push(Eintrag::Platte(p.run));
            }
            ElementKind::StripFooting(f) => {
                rein(&mut self.schuerzen, f.slab, id, platz);
                rein(&mut self.an_platte, f.slab, id, platz);
                b.extend([Eintrag::Schuerze(f.slab), Eintrag::AnPlatte(f.slab)]);
            }
            ElementKind::PerimeterInsulation { slab } => {
                rein(&mut self.an_platte, *slab, id, platz);
                b.push(Eintrag::AnPlatte(*slab));
            }
            ElementKind::SoffitInsulation { floor }
            | ElementKind::RoofTerrace { floor }
            | ElementKind::Coping { floor }
            | ElementKind::Roof { floor, .. } => {
                rein(&mut self.an_decke, *floor, id, platz);
                b.push(Eintrag::AnDecke(*floor));
            }
            ElementKind::EdgeStrip { wall, floor } => {
                rein(&mut self.an_decke, *floor, id, platz);
                rein(&mut self.an_wand, *wall, id, platz);
                b.extend([Eintrag::AnDecke(*floor), Eintrag::AnWand(*wall)]);
            }
            ElementKind::Downpipe { roof, .. } => {
                rein(&mut self.an_dach, *roof, id, platz);
                b.push(Eintrag::AnDach(*roof));
            }
            ElementKind::Opening(op) => {
                let h = m.storey(e.storey).and_then(|s| s.building);
                // Mit der Neigungsreserve wie der Vortest in holes_of
                let (lo, hi) = super::oeffnung_model::pfad_rahmen(&op.path);
                self.oeffnungen.rein(h, id, [lo, hi].into_iter());
                self.auf_geschoss
                    .entry(e.storey)
                    .or_default()
                    .insert(id.index(), id);
                b.push(Eintrag::Oeffnung(e.storey));
            }
            ElementKind::Ext(_) => {
                let i = self.exemplare.partition_point(|x| x.index() < id.index());
                self.exemplare.insert(i, id);
                b.push(Eintrag::Exemplar);
            }
            _ => {}
        }
        if !b.is_empty() {
            self.beitrag.insert(id, b);
        }
    }

    fn bauteil_raus(&mut self, id: ElementId) {
        let gleich = |x: &ElementId| *x == id;
        for b in self.beitrag.remove(&id).into_iter().flatten() {
            match b {
                Eintrag::Decke(r) => raus(&mut self.decken, &r, gleich),
                Eintrag::Platte(r) => raus(&mut self.platten, &r, gleich),
                Eintrag::Schuerze(s) => raus(&mut self.schuerzen, &s, gleich),
                Eintrag::Gruendung(r) => raus(&mut self.gruendung, &r, gleich),
                Eintrag::AnPlatte(s) => raus(&mut self.an_platte, &s, gleich),
                Eintrag::AnDecke(f) => raus(&mut self.an_decke, &f, gleich),
                Eintrag::AnWand(w) => raus(&mut self.an_wand, &w, gleich),
                Eintrag::AnDach(r) => raus(&mut self.an_dach, &r, gleich),
                Eintrag::Gekoppelt(s) => raus(&mut self.gekoppelt, &s, |x| x.0 == id),
                Eintrag::Oeffnung(s) => {
                    self.oeffnungen.raus(id);
                    if let Some(o) = self.auf_geschoss.get_mut(&s) {
                        if o.get(&id.index()) == Some(&id) {
                            o.remove(&id.index());
                        }
                        if o.is_empty() {
                            self.auf_geschoss.remove(&s);
                        }
                    }
                }
                Eintrag::Exemplar => self.exemplare.retain(|x| *x != id),
            }
        }
    }

    fn b_seite_neu(&mut self, joins: &[Join]) {
        self.b_seite.clear();
        for j in joins {
            self.b_rein(j);
        }
    }

    fn b_rein(&mut self, j: &Join) {
        if j.b_run != j.a_run {
            let k = |x: &(RunId, JoinEnd)| (x.0.index(), x.1);
            let liste = self.b_seite.entry(j.b_run).or_default();
            let i = liste.partition_point(|x| k(x) < (j.a_run.index(), j.a_end));
            liste.insert(i, (j.a_run, j.a_end));
        }
    }

    fn b_raus(&mut self, j: &Join) {
        if j.b_run != j.a_run {
            raus(&mut self.b_seite, &j.b_run, |x| *x == (j.a_run, j.a_end));
        }
    }
}

/// Eine [`Arena`], die mitschreibt, welche Einträge sich ändern: Jede
/// Änderung geht durch diese Methoden, gleich wer sie macht. Gelesen wird
/// über `Deref` wie aus der Arena.
#[derive(Clone, Debug)]
pub(crate) struct Erfasst<T> {
    arena: Arena<T>,
    /// Seit dem letzten Gang geänderte Kennungen; `None`: unbekannt oder zu
    /// viele, das Verzeichnis wird neu gebaut.
    log: RefCell<Option<Vec<Id<T>>>>,
    /// Während [`Erfasst::aufzeichnen`]: je geänderter Kennung der Stand
    /// davor (`None`: gab es nicht).
    vorher: Option<Vec<(Id<T>, Option<T>)>>,
}

impl<T> Default for Erfasst<T> {
    fn default() -> Self {
        Erfasst::new(Arena::new())
    }
}

impl<T> Deref for Erfasst<T> {
    type Target = Arena<T>;

    fn deref(&self) -> &Arena<T> {
        &self.arena
    }
}

impl<T> Erfasst<T> {
    pub(crate) fn new(arena: Arena<T>) -> Self {
        Erfasst {
            arena,
            log: RefCell::new(None),
            vorher: None,
        }
    }

    fn merke(&mut self, id: Id<T>) {
        // Ab einem Viertel der Einträge ist neu bauen nicht teurer
        let grenze = self.arena.len() / 4 + 256;
        let log = self.log.get_mut();
        match log {
            Some(l) if l.len() < grenze => l.push(id),
            _ => *log = None,
        }
    }

    /// Nimmt die geänderten Kennungen ab; ab jetzt wird neu mitgeschrieben.
    fn abnehmen(&self) -> Option<Vec<Id<T>>> {
        self.log.replace(Some(Vec::new()))
    }
}

impl<T: Clone> Erfasst<T> {
    /// Merkt den Stand von `id` vor der ersten Änderung, solange
    /// aufgezeichnet wird.
    fn vorher_merken(&mut self, id: Id<T>) {
        if let Some(v) = &mut self.vorher {
            if !v.iter().any(|x| x.0 == id) {
                v.push((id, self.arena.get(id).cloned()));
            }
        }
    }

    /// Ab jetzt merkt sich jede Änderung den Stand davor, bis
    /// [`Erfasst::vorher_nehmen`]. So braucht, wer alte Stände vergleicht,
    /// nur die geänderten zu kopieren statt aller (L40).
    pub(crate) fn aufzeichnen(&mut self) {
        debug_assert!(self.vorher.is_none(), "Aufzeichnung läuft schon");
        self.vorher = Some(Vec::new());
    }

    /// Beendet die Aufzeichnung: je geänderter Kennung der Stand davor.
    pub(crate) fn vorher_nehmen(&mut self) -> Vec<(Id<T>, Option<T>)> {
        self.vorher.take().unwrap_or_default()
    }

    pub(crate) fn insert(&mut self, value: T) -> Id<T> {
        let id = self.arena.insert(value);
        if let Some(v) = &mut self.vorher {
            v.push((id, None));
        }
        self.merke(id);
        id
    }

    pub(crate) fn remove(&mut self, id: Id<T>) -> Option<T> {
        self.vorher_merken(id);
        self.merke(id);
        self.arena.remove(id)
    }

    pub(crate) fn get_mut(&mut self, id: Id<T>) -> Option<&mut T> {
        self.vorher_merken(id);
        self.merke(id);
        self.arena.get_mut(id)
    }

    pub(crate) fn set(&mut self, id: Id<T>, value: Option<T>) {
        self.vorher_merken(id);
        self.merke(id);
        self.arena.set(id, value)
    }
}

/// Die Anschlüsse des Modells, nach `(a_run, a_end)` geordnet. Geändert
/// werden sie nur über [`JoinListe::ersetzen`] und [`JoinListe::setzen`],
/// die mitschreiben, was wegfällt und dazukommt.
#[derive(Clone, Debug, Default)]
pub(crate) struct JoinListe {
    liste: Vec<Join>,
    /// Seit dem letzten Gang: entfernte (`false`) und neue (`true`)
    /// Anschlüsse; `None`: unbekannt oder zu viele.
    log: RefCell<Option<Vec<(bool, Join)>>>,
}

impl Deref for JoinListe {
    type Target = Vec<Join>;

    fn deref(&self) -> &Vec<Join> {
        &self.liste
    }
}

impl JoinListe {
    /// Ersetzt die Anschlüsse der Enden `ends` durch `fresh` und ordnet.
    /// Jedes Ende hat höchstens einen Anschluss. Bleiben die Enden mit
    /// Anschluss dieselben (beim Ziehen fast immer), tauscht es sie an
    /// ihrem Platz; sonst entfernt, ergänzt und ordnet es die Liste in
    /// einem Gang, ohne Hilfsspeicher (L40).
    pub(crate) fn ersetzen(&mut self, ends: &[(RunId, JoinEnd)], mut fresh: Vec<Join>) {
        let grenze = self.liste.len() / 4 + 256;
        let schluessel = |j: &Join| (j.a_run.index(), j.a_end);
        let mut alt: Vec<usize> = ends
            .iter()
            .filter_map(|&(r, e)| {
                let i = self
                    .liste
                    .partition_point(|j| schluessel(j) < (r.index(), e));
                self.liste
                    .get(i)
                    .is_some_and(|j| j.a_run == r && j.a_end == e)
                    .then_some(i)
            })
            .collect();
        alt.sort_unstable();
        alt.dedup();
        fresh.sort_unstable_by_key(schluessel);
        let log = self.log.get_mut();
        if let Some(l) = log {
            l.extend(alt.iter().map(|&i| (false, self.liste[i].clone())));
            l.extend(fresh.iter().map(|j| (true, j.clone())));
            if l.len() > grenze {
                *log = None;
            }
        }
        let gleich = alt.len() == fresh.len()
            && alt
                .iter()
                .zip(&fresh)
                .all(|(&i, j)| (self.liste[i].a_run, self.liste[i].a_end) == (j.a_run, j.a_end));
        if gleich {
            for (i, j) in alt.into_iter().zip(fresh) {
                self.liste[i] = j;
            }
            return;
        }
        let weg = |j: &Join| ends.contains(&(j.a_run, j.a_end));
        self.liste.retain(|j| !weg(j));
        self.liste.extend(fresh);
        // Schlüssel sind eindeutig: ungeordnet sortieren gibt dasselbe
        self.liste.sort_unstable_by_key(schluessel);
    }

    /// Setzt alle Anschlüsse neu.
    pub(crate) fn setzen(&mut self, liste: Vec<Join>) {
        self.liste = liste;
        *self.log.get_mut() = None;
    }

    /// Nimmt die Änderungen ab; ab jetzt wird neu mitgeschrieben.
    fn abnehmen(&self) -> Option<Vec<(bool, Join)>> {
        self.log.replace(Some(Vec::new()))
    }
}

/// Bis zu so vielen Bauteilen vergleicht der Debug-Build jedes
/// nachgeführte Verzeichnis mit einem neu gebauten; darüber würden Tests,
/// die tausende Bauteile einzeln anlegen, quadratisch.
const PRUEFEN_BIS: usize = 5000;

/// Das Verzeichnis, das das Modell zwischen den Gängen hält. Eine Kopie
/// des Modells trägt es mit; führt sie es nach, bekommt sie ihr eigenes.
#[derive(Clone, Default)]
pub(crate) struct Halter(RefCell<Option<Arc<Verzeichnis>>>);

impl std::fmt::Debug for Halter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Halter")
    }
}

/// Platz für das Verzeichnis im laufenden Gang. Eine Kopie des Modells
/// beginnt ohne: Sie kann geändert werden, während das Original noch liest.
/// `ohne`: Suchen gehen über das ganze Modell (Vergleich im Test).
#[derive(Default)]
pub(crate) struct Merker(RefCell<Option<Arc<Verzeichnis>>>, Cell<bool>);

impl Clone for Merker {
    fn clone(&self) -> Self {
        Merker::default()
    }
}

impl std::fmt::Debug for Merker {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Merker")
    }
}

/// Räumt das Verzeichnis beim Verlassen von [`Model::lesend`] ab, auch
/// wenn der Gang abbricht.
struct Abbau<'a>(&'a Merker);

impl Drop for Abbau<'_> {
    fn drop(&mut self) {
        self.0 .0.borrow_mut().take();
    }
}

impl Model {
    /// Decken der Züge `scope` nach ihrem Platz, aus dem Verzeichnis;
    /// `None`: alle. Die Abgleiche beim Schließen eines Schritts gehen
    /// so nur über den Bereich, nicht über alle Bauteile (L40). Im Debug
    /// gegen den Gang über alle geprüft.
    pub(super) fn decken_in(&self, scope: Option<&[RunId]>) -> Vec<ElementId> {
        let alle = || -> Vec<ElementId> {
            self.elements
                .iter()
                .filter(|(_, e)| match e.kind {
                    ElementKind::Floor(f) => in_scope(scope, f.run),
                    _ => false,
                })
                .map(|(id, _)| id)
                .collect()
        };
        match (scope, self.verz()) {
            (Some(s), Some(v)) => {
                let out = v.decken_der(s);
                if cfg!(debug_assertions) && self.elements.len() <= PRUEFEN_BIS {
                    assert_eq!(out, alle(), "Decken im Bereich");
                }
                out
            }
            _ => alle(),
        }
    }

    /// Die Exemplare aus Erweiterungen, nach ihrem Platz. Über das
    /// Verzeichnis, damit der Abgleich der Szene bei jedem Modellstand
    /// nicht alle Bauteile durchgeht (L40).
    pub fn exemplare(&self) -> Vec<ElementId> {
        match self.verz() {
            Some(v) => v.exemplare.clone(),
            None => self
                .elements
                .iter()
                .filter(|(_, e)| matches!(e.kind, ElementKind::Ext(_)))
                .map(|(id, _)| id)
                .collect(),
        }
    }

    /// Kandidaten für die Bauteile an den Decken der Züge `scope`
    /// (Untersicht, Dachterrasse, Blech, Flachdach, Randdämmstreifen),
    /// dazu die an Bauteilen, die keine Decke mehr sind, und Streifen auf
    /// entfernten Wänden; nach ihrem Platz. `None` oder ohne Verzeichnis:
    /// alle Bauteile. Wer sie nimmt, prüft Art und Bereich wie zuvor. Im
    /// Debug geprüft: Jedes Bauteil, das ein Abgleich im Bereich nimmt, ist
    /// dabei.
    pub(super) fn an_decken_in(&self, scope: Option<&[RunId]>) -> Vec<ElementId> {
        let (Some(s), Some(v)) = (scope, self.verz()) else {
            return self.elements.ids().collect();
        };
        let out = v.an_decken(&v.decken_der(s));
        if cfg!(debug_assertions) && self.elements.len() <= PRUEFEN_BIS {
            for (id, e) in self.elements.iter() {
                let noetig = match e.kind {
                    ElementKind::SoffitInsulation { floor }
                    | ElementKind::RoofTerrace { floor }
                    | ElementKind::Coping { floor }
                    | ElementKind::Roof { floor, .. } => self.floor_in_scope(floor, scope),
                    ElementKind::EdgeStrip { wall, floor } => {
                        self.floor_in_scope(floor, scope) || self.element(wall).is_none()
                    }
                    _ => false,
                };
                let drin = out.binary_search_by_key(&id.index(), |x| x.index()).is_ok();
                assert!(!noetig || drin, "Bauteil an der Decke fehlt im Bereich");
            }
        }
        out
    }

    /// Kandidaten für die Regenfallrohre an den Dachaufbauten `daecher` im
    /// Bereich `scope`, dazu die an Bauteilen, die kein Dachaufbau mehr
    /// sind; nach ihrem Platz. `None` oder ohne Verzeichnis: alle Bauteile.
    /// Im Debug geprüft wie [`Model::an_decken_in`].
    pub(super) fn fallrohre_in(
        &self,
        scope: Option<&[RunId]>,
        daecher: &[ElementId],
    ) -> Vec<ElementId> {
        let (Some(_), Some(v)) = (scope, self.verz()) else {
            return self.elements.ids().collect();
        };
        let out = v.fallrohre(daecher);
        if cfg!(debug_assertions) && self.elements.len() <= PRUEFEN_BIS {
            for (id, e) in self.elements.iter() {
                let ElementKind::Downpipe { roof, .. } = e.kind else {
                    continue;
                };
                let dach = matches!(
                    self.element(roof).map(|e| &e.kind),
                    Some(ElementKind::Roof { .. })
                );
                let noetig = daecher.contains(&roof) || !dach;
                let drin = out.binary_search_by_key(&id.index(), |x| x.index()).is_ok();
                assert!(!noetig || drin, "Regenfallrohr fehlt im Bereich");
            }
        }
        out
    }

    /// Führt `f` mit einem festen Stand des [`Verzeichnis`] aus: Jeder
    /// Wandkörper, jede Decke findet Anschlüsse, Decken, Züge darüber und
    /// Öffnungen darin, ohne dass jede Suche erst nach Änderungen sieht.
    /// Verschachtelt richtet nur der äußerste Aufruf es ein. Seit dem
    /// letzten Gang geänderte Einträge werden nachgeführt, sonst kostet der
    /// Aufruf fast nichts.
    pub fn lesend<R>(&self, f: impl FnOnce() -> R) -> R {
        if self.merker.0.borrow().is_some() {
            return f();
        }
        let v = self.verzeichnis_aktuell();
        *self.merker.0.borrow_mut() = Some(v);
        let _abbau = Abbau(&self.merker);
        f()
    }

    /// Das Verzeichnis: im Gang das von [`Model::lesend`], sonst das
    /// gehaltene, nachgeführt. `None` nur im Test ohne Verzeichnis.
    pub(crate) fn verz(&self) -> Option<Arc<Verzeichnis>> {
        if let Some(v) = self.merker.0.borrow().clone() {
            return Some(v);
        }
        (!self.merker.1.get()).then(|| self.verzeichnis_aktuell())
    }

    /// `f` mit Suchen über das ganze Modell statt im Verzeichnis
    /// (Vergleich im Test).
    #[cfg(test)]
    pub(crate) fn ohne_verzeichnis<R>(&self, f: impl FnOnce() -> R) -> R {
        assert!(self.merker.0.borrow().is_none());
        self.merker.1.set(true);
        let r = f();
        self.merker.1.set(false);
        r
    }

    /// Das gehaltene Verzeichnis, auf den Stand des Modells gebracht. Im
    /// Test (Debug-Build) wird jedes nachgeführte mit einem neu gebauten
    /// verglichen, bis zu [`PRUEFEN_BIS`] Bauteilen.
    fn verzeichnis_aktuell(&self) -> Arc<Verzeichnis> {
        let bauteile = self.elements.abnehmen();
        let zuege = self.runs.abnehmen();
        let geschosse = self.storeys.abnehmen();
        let joins = self.joins.abnehmen();
        let mut halter = self.halter.0.borrow_mut();
        let mut nachgefuehrt = false;
        match (halter.as_mut(), bauteile, zuege, geschosse) {
            (Some(v), Some(b), Some(z), Some(g)) => {
                if !b.is_empty() || !z.is_empty() || !g.is_empty() || joins != Some(Vec::new()) {
                    let v = Arc::make_mut(v);
                    v.nachfuehren(self, &b, &z, &g);
                    match joins {
                        Some(j) => {
                            for (dazu, j) in &j {
                                if *dazu {
                                    v.b_rein(j);
                                } else {
                                    v.b_raus(j);
                                }
                            }
                        }
                        None => v.b_seite_neu(&self.joins),
                    }
                    nachgefuehrt = true;
                }
            }
            _ => *halter = Some(Arc::new(Verzeichnis::neu(self))),
        }
        let v = halter.clone().expect("eben gebaut");
        drop(halter);
        if cfg!(debug_assertions) && nachgefuehrt && self.elements.len() <= PRUEFEN_BIS {
            assert!(
                *v == Verzeichnis::neu(self),
                "nachgeführtes Verzeichnis weicht vom neu gebauten ab"
            );
        }
        v
    }
}

impl Verzeichnis {
    pub(crate) fn decken(&self, run: RunId) -> Vec<ElementId> {
        self.decken.get(&run).cloned().unwrap_or_default()
    }

    /// Decken der Züge `zuege`, nach ihrem Platz.
    fn decken_der(&self, zuege: &[RunId]) -> Vec<ElementId> {
        let mut out: Vec<ElementId> = zuege
            .iter()
            .flat_map(|r| self.decken.get(r).into_iter().flatten().copied())
            .collect();
        out.sort_unstable_by_key(|f| f.index());
        out.dedup();
        out
    }

    /// Bauteile an den Decken `decken`, an verschwundenen Decken und auf
    /// verschwundenen Wänden ([`Verzeichnis::an_decke`]), nach ihrem Platz.
    fn an_decken(&self, decken: &[ElementId]) -> Vec<ElementId> {
        let mut out: Vec<ElementId> = decken
            .iter()
            .chain(&self.waisen)
            .flat_map(|f| self.an_decke.get(f).into_iter().flatten().copied())
            .chain(
                self.waisen_wand
                    .iter()
                    .flat_map(|w| self.an_wand.get(w).into_iter().flatten().copied()),
            )
            .collect();
        out.sort_unstable_by_key(|x| x.index());
        out.dedup();
        out
    }

    /// Regenfallrohre an den Dachaufbauten `daecher` und an Bauteilen, die
    /// kein Dachaufbau mehr sind, nach ihrem Platz.
    fn fallrohre(&self, daecher: &[ElementId]) -> Vec<ElementId> {
        let mut out: Vec<ElementId> = daecher
            .iter()
            .chain(&self.waisen_dach)
            .flat_map(|r| self.an_dach.get(r).into_iter().flatten().copied())
            .collect();
        out.sort_unstable_by_key(|x| x.index());
        out.dedup();
        out
    }

    /// [`Model::runs_above`] zu den Segmenten `segs` des Zuges darunter: die
    /// Züge in der Reihenfolge ihrer ersten gekoppelten Wand.
    pub(crate) fn darueber(&self, segs: &[ElementId]) -> Vec<RunId> {
        let mut waende: Vec<(ElementId, RunId)> = segs
            .iter()
            .flat_map(|s| self.gekoppelt.get(s).into_iter().flatten().copied())
            .collect();
        waende.sort_unstable_by_key(|w| w.0.index());
        let mut out: Vec<RunId> = Vec::new();
        for (_, r) in waende {
            if !out.contains(&r) {
                out.push(r);
            }
        }
        out
    }

    /// Stellen in `joins`, an denen der Zug `run` `a_run` oder `b_run` ist,
    /// aufsteigend.
    pub(crate) fn anschluesse(&self, run: RunId, joins: &[Join]) -> Vec<usize> {
        let stelle = |a: RunId, e: JoinEnd| {
            let i = joins.partition_point(|j| (j.a_run.index(), j.a_end) < (a.index(), e));
            joins[i..]
                .iter()
                .take_while(|j| (j.a_run.index(), j.a_end) == (a.index(), e))
                .position(|j| j.a_run == a)
                .map(|k| i + k)
        };
        let von = joins.partition_point(|j| j.a_run.index() < run.index());
        let mut out: Vec<usize> = (von..joins.len())
            .take_while(|&k| joins[k].a_run.index() == run.index())
            .filter(|&k| joins[k].a_run == run)
            .collect();
        for &(a, e) in self.b_seite.get(&run).into_iter().flatten() {
            out.extend(stelle(a, e));
        }
        out.sort_unstable();
        out.dedup();
        out
    }

    pub(crate) fn platten(&self, run: RunId) -> Vec<ElementId> {
        self.platten.get(&run).cloned().unwrap_or_default()
    }

    pub(crate) fn schuerzen(&self, slab: ElementId) -> Vec<ElementId> {
        self.schuerzen.get(&slab).cloned().unwrap_or_default()
    }

    pub(crate) fn gruendung(&self, run: RunId) -> Vec<ElementId> {
        self.gruendung.get(&run).cloned().unwrap_or_default()
    }

    /// Öffnungen des Gebäudes `b`, deren Rechteck das Rechteck `lo`–`hi`
    /// trifft, in der Reihenfolge der Bauteile.
    pub(crate) fn oeffnungen_bei(
        &self,
        b: Option<BuildingId>,
        lo: Vec3,
        hi: Vec3,
    ) -> Vec<ElementId> {
        self.oeffnungen.bei(b, lo, hi)
    }

    /// Züge im Geschoss `st`, deren Rechteck das Rechteck `lo`–`hi` trifft,
    /// in der Reihenfolge der Züge.
    pub(crate) fn zuege_bei(&self, st: StoreyId, lo: Vec3, hi: Vec3) -> Vec<RunId> {
        self.zuege.bei(st, lo, hi)
    }

    /// Züge aller Geschosse, deren Rechteck das Rechteck `lo`–`hi` trifft,
    /// in der Reihenfolge der Züge.
    pub(crate) fn alle_zuege_bei(&self, lo: Vec3, hi: Vec3) -> Vec<RunId> {
        self.alle_zuege.bei((), lo, hi)
    }
}

#[cfg(test)]
mod tests {
    use crate::{szo, ElementId, ElementKind, GuidGen, Model, RunId};
    use std::sync::Arc;

    const HAEUSER: [&str; 9] = [
        include_str!("../../sk-cost/referenz/rh1-standardhaus.szo"),
        include_str!("../../sk-cost/referenz/rh2-mehrschalig.szo"),
        include_str!("../../sk-cost/referenz/rh3-versatz-dachterrasse.szo"),
        include_str!("../../sk-cost/referenz/rh4-erweiterungen.szo"),
        include_str!("../../sk-cost/referenz/rh5-flachdach.szo"),
        include_str!("../../sk-cost/referenz/r7-alle-abschnitte.szo"),
        include_str!("../../../app/src/abnahme_haus_v3.szo"),
        include_str!("../../../app/src/abnahme_p5.szo"),
        include_str!("../../../app/src/abnahme_p6.szo"),
    ];

    /// Mit Verzeichnis entsteht jeder Wandkörper, jede Decke und die ganze
    /// Mengenliste genau wie ohne; danach ist es wieder weg.
    #[test]
    fn mit_verzeichnis_wie_ohne() {
        for text in HAEUSER {
            let m = szo::read(text, GuidGen::with_seed(1)).expect("lädt").model;
            let alle = |m: &Model| {
                let ids: Vec<RunId> = m.runs().ids().collect();
                ids.into_iter()
                    .map(|r| {
                        let unter = (m.foundation_of(r), m.runs_under_floor(r));
                        (
                            m.chain(r),
                            m.floor(r),
                            m.runs_above(r),
                            m.floor_of(r),
                            unter,
                        )
                    })
                    .collect::<Vec<_>>()
            };
            let ohne = m.ohne_verzeichnis(|| alle(&m));
            let mit = m.lesend(|| {
                assert!(m.merker.0.borrow().is_some());
                // verschachtelt: richtet nicht neu ein und räumt nicht ab
                m.lesend(|| ());
                assert!(m.merker.0.borrow().is_some());
                alle(&m)
            });
            assert!(m.merker.0.borrow().is_none());
            assert_eq!(ohne, mit);
            assert_eq!(ohne, alle(&m));
            assert!(ohne.iter().any(|x| x.0.is_some()));
            let ohne = m.ohne_verzeichnis(|| crate::qto::schedule_im_gang(&m));
            assert_eq!(ohne, crate::qto::schedule(&m));
            let ids: Vec<_> = m.elements().ids().collect();
            let gruendung = |m: &Model| {
                ids.iter()
                    .map(|e| crate::qto::gruendung_von(m, *e))
                    .collect::<Vec<_>>()
            };
            let ohne = m.ohne_verzeichnis(|| gruendung(&m));
            assert_eq!(ohne, m.lesend(|| gruendung(&m)));
            assert!(ohne.iter().any(|g| g.len() >= 2));
        }
    }

    /// Das gehaltene Verzeichnis wird nach jeder Art von Änderung
    /// nachgeführt statt neu gebaut und gleicht danach dem neu gebauten:
    /// Wand verschieben, Öffnung anlegen, ändern und löschen, Wand lösen
    /// und versetzen, Flachdach, Wand löschen, jeder Schritt rückgängig und
    /// wieder, ein neues Gebäude (L40).
    #[test]
    fn nachgefuehrt_wie_neu() {
        use crate::oeffnung::{HeightLine, HeightRef, Opening, OpeningKind};
        use crate::txn::Direction;
        use crate::{Category, RefSide};
        use sk_math::vec3;
        let mut m = Model::with_seed(5);
        let b = m.add_building(2);
        for ox in [0.0, 15000.0] {
            let p = |x: f64, y: f64| vec3(ox + x, y, 0.0);
            let pts = [
                p(0.0, 0.0),
                p(0.0, 8000.0),
                p(10000.0, 8000.0),
                p(10000.0, 0.0),
            ];
            m.build_from_polygon(b, &pts).unwrap();
        }
        let eg = m.runs().ids().next().unwrap();
        let og = m.runs_above(eg)[0];
        let st = m.run(eg).unwrap().storey;
        let fenster = |m: &Model, u: f64| {
            let f = m.outer_face(eg, 1).unwrap();
            Opening {
                kind: OpeningKind::Window,
                path: vec![f.at(f.u.0 + u, 0.0), f.at(f.u.0 + u + 900.0, 0.0)],
                bottom: HeightRef::Line(st, HeightLine::Sill),
                top: HeightRef::Line(st, HeightLine::Lintel),
                reveal: None,
                install: Default::default(),
                swing: None,
                lock: Default::default(),
            }
        };
        // Stand des Verzeichnisses: gleich dem neu gebauten, Adresse zeigt,
        // ob es nachgeführt (gleich) oder neu gebaut wurde
        let stand = |m: &Model| {
            let v = m.verzeichnis_aktuell();
            assert_eq!(*v, super::Verzeichnis::neu(m));
            Arc::as_ptr(&v)
        };
        let fest = stand(&m);
        let mut schritte = Vec::new();
        let mut schritt = |m: &mut Model, was: &str, f: &dyn Fn(&mut Model)| {
            m.begin("Probe");
            f(m);
            assert_eq!(stand(m), fest, "{was}: im Schritt");
            schritte.push(m.commit().expect(was));
            assert_eq!(stand(m), fest, "{was}");
        };
        schritt(&mut m, "Wand verschieben", &|m| {
            let c = m.chain(eg).unwrap().with_segment_moved(0, 300.0).unwrap();
            assert!(m.set_run_points(eg, &c.points).is_some());
        });
        schritt(&mut m, "Öffnung anlegen", &|m| {
            assert!(m.add_opening(fenster(m, 800.0)).is_some());
        });
        let (id, _) = m
            .elements()
            .iter()
            .find(|(_, e)| matches!(e.kind, ElementKind::Opening(_)))
            .unwrap();
        schritt(&mut m, "Öffnung verschieben", &|m| {
            assert!(m.set_opening(id, fenster(m, 2500.0)));
        });
        schritt(&mut m, "Wand lösen und versetzen", &|m| {
            let w = m.wall_at(og, 2).unwrap();
            assert!(m.set_linked(w, false));
            assert!(m.set_offset(w, -900.0).is_some());
        });
        schritt(&mut m, "Flachdach", &|m| {
            let oben = m.run(og).unwrap().storey;
            assert!(m.set_flat_roof(oben, true));
        });
        schritt(&mut m, "Öffnung löschen", &|m| {
            assert!(!m.delete_elements(&[id]).removed.is_empty());
        });
        schritt(&mut m, "Innenwand", &|m| {
            let pts = [vec3(4000.0, 0.0, 0.0), vec3(4000.0, 8000.0, 0.0)];
            let set = m.defaults().interior_wall;
            let r = m.add_wall_run(
                &pts,
                false,
                RefSide::Center,
                st,
                set,
                Category::InteriorWall,
            );
            assert!(r.is_some());
        });
        schritt(&mut m, "Innenwand löschen", &|m| {
            let innen = m.runs().ids().last().unwrap();
            let w = m.wall_at(innen, 0).unwrap();
            let d = m.delete_elements(&[w]);
            assert!(!d.removed.is_empty(), "{:?}", d.refused);
        });
        for dir in [Direction::Undo, Direction::Redo] {
            let mut alle: Vec<_> = schritte.iter().collect();
            if dir == Direction::Undo {
                alle.reverse();
            }
            for t in alle {
                m.apply(t, dir);
                assert_eq!(stand(&m), fest, "{} {dir:?}", t.label);
            }
        }
        m.add_building(1);
        assert_eq!(stand(&m), fest, "neues Gebäude");
    }

    /// Öffnungen gelten am Zug mit Verzeichnis wie ohne (r7 hat welche).
    #[test]
    fn loecher_mit_verzeichnis_wie_ohne() {
        let m = szo::read(HAEUSER[5], GuidGen::with_seed(1))
            .expect("lädt")
            .model;
        let ids: Vec<RunId> = m.runs().ids().collect();
        let ohne: Vec<_> = m.ohne_verzeichnis(|| {
            ids.iter()
                .filter_map(|r| m.chain(*r))
                .map(|c| c.joints.holes)
                .collect()
        });
        let mit: Vec<_> = m.lesend(|| {
            ids.iter()
                .filter_map(|r| m.chain(*r))
                .map(|c| c.joints.holes)
                .collect()
        });
        assert!(ohne.iter().map(Vec::len).sum::<usize>() >= 3);
        assert_eq!(ohne, mit);
        // Züge an jeder Öffnung (Export: Öffnungskörper, Türblatt)
        let oe: Vec<ElementId> = m.elements().ids().collect();
        let zuege = |m: &Model| {
            oe.iter()
                .filter_map(|e| match &m.element(*e)?.kind {
                    ElementKind::Opening(o) => {
                        Some((m.opening_runs(o), format!("{:?}", m.opening_voids(*e))))
                    }
                    _ => None,
                })
                .collect::<Vec<_>>()
        };
        let ohne = m.ohne_verzeichnis(|| zuege(&m));
        assert!(ohne.len() >= 3);
        assert!(ohne.iter().all(|z| !z.0.is_empty()));
        assert_eq!(ohne, m.lesend(|| zuege(&m)));
    }

    /// Löcher je Zug wie vor dem Vortest `nah` und dem Raster: der alte
    /// Gang über alle Öffnungen des Gebäudes (Review #130 H1).
    #[test]
    fn loecher_wie_vorher() {
        let mut loecher = 0;
        for text in HAEUSER {
            let m = szo::read(text, GuidGen::with_seed(1)).expect("lädt").model;
            let gebaeude = |st| m.storey(st).and_then(|s| s.building);
            let ids: Vec<RunId> = m.runs().ids().collect();
            for r in ids {
                let Some(c) = m.chain(r) else { continue };
                let b = m.run(r).and_then(|z| gebaeude(z.storey));
                loecher += c.joints.holes.len();
                let vorher: Vec<_> = if m
                    .category_of(r)
                    .is_some_and(crate::Category::takes_openings)
                {
                    m.elements()
                        .iter()
                        .filter(|(_, e)| gebaeude(e.storey) == b)
                        .flat_map(|(id, e)| match &e.kind {
                            ElementKind::Opening(op) => m.legs_on(r, &c, Some(id), op),
                            _ => Vec::new(),
                        })
                        .map(|(_, h)| h)
                        .collect()
                } else {
                    Vec::new()
                };
                assert_eq!(m.holes_of(r, &c), vorher);
                assert_eq!(m.lesend(|| m.holes_of(r, &c)), vorher);
            }
        }
        assert!(loecher >= 3);
    }

    /// Ein Pfadstück, das als parallel gilt, weicht bis 1e-3 seiner Länge
    /// ab: so weit reicht das Rechteck über die Punkte hinaus.
    #[test]
    fn pfad_rahmen_traegt_die_neigung() {
        use sk_math::vec3;
        let (lo, hi) = super::super::oeffnung_model::pfad_rahmen(&[
            vec3(0.0, 0.0, 500.0),
            vec3(2_000_000.0, 0.0, 500.0),
        ]);
        assert_eq!(
            (lo.x, lo.y, hi.x, hi.y),
            (-2000.0, -2000.0, 2_002_000.0, 2000.0)
        );
        let (lo, hi) = super::super::oeffnung_model::pfad_rahmen(&[]);
        assert!(lo.x > hi.x);
    }

    /// Bauvorbereitung (`site.*`) aus [`crate::qto_site::huellen`] wie mit
    /// dem alten Gang über alle Züge je Platte (Review #130 H1): je Zug
    /// jeder Datei, erste und weitere Platte.
    #[test]
    fn bauvorbereitung_wie_vorher() {
        use crate::qto::AutoMenge;
        use crate::qto_site::*;
        for text in HAEUSER {
            let m = szo::read(text, GuidGen::with_seed(1)).expect("lädt").model;
            let h = huellen(&m);
            let leer = Huelle::default();
            let ids: Vec<RunId> = m.runs().ids().collect();
            let mut aussen = 0;
            for r in ids {
                let platte = m
                    .chain(r)
                    .map(|c| c.face_corners(c.inner_offset()))
                    .unwrap_or_default();
                let Some((z, &element)) = m.run(r).and_then(|z| Some((z, z.segments.first()?)))
                else {
                    continue;
                };
                let b = m.building_of(z.storey);
                let vorlage = AutoMenge {
                    element,
                    number: String::new(),
                    storey: z.storey,
                    building: b,
                    key: "",
                    unit: "",
                    value: 0.0,
                    kg: None,
                    formula: String::new(),
                };
                aussen += usize::from(h.contains_key(&b));
                for erste in [true, false] {
                    for gelaende in [0.0, -450.0] {
                        let alt = site_alt(&m, r, &platte, &vorlage, gelaende, erste);
                        let neu = site_mengen_in(
                            h.get(&b).unwrap_or(&leer),
                            r,
                            &platte,
                            &vorlage,
                            gelaende,
                            erste,
                        );
                        assert_eq!(neu, alt);
                    }
                }
            }
            assert!(aussen > 0);
        }
    }

    /// `site_mengen` vor #130, wörtlich (Vergleich).
    fn site_alt(
        model: &Model,
        run: RunId,
        platte: &[sk_math::Vec3],
        vorlage: &crate::qto::AutoMenge,
        gelaende: f64,
        erste: bool,
    ) -> Vec<crate::qto::AutoMenge> {
        use crate::qto::AutoMenge;
        use crate::qto_site::*;
        use sk_math::{polygon, Vec3};
        let gebaeude = model.run(run).and_then(|r| model.building_of(r.storey));
        let zuege: Vec<RunId> = model
            .runs()
            .iter()
            .filter(|(_, r)| {
                r.closed
                    && model.building_of(r.storey) == gebaeude
                    && r.segments
                        .first()
                        .and_then(|e| model.element(*e))
                        .is_some_and(|e| e.category.is_outer_wall())
            })
            .map(|(id, _)| id)
            .collect();
        let mut punkte: Vec<Vec3> = platte.to_vec();
        let mut krone = 0.0f64;
        let mut geschosse = Vec::new();
        let mut fassade: Option<Vec<Vec3>> = None;
        for id in &zuege {
            let Some(c) = model.chain(*id) else { continue };
            let aussen = c.face_corners(c.outer_offset());
            punkte.extend(aussen.iter().copied());
            krone = krone.max(c.top());
            if let Some(a) = &c.joints.attika {
                krone = krone.max(a.band.1);
            }
            if let Some(r) = model.run(*id).filter(|r| {
                model
                    .storey(r.storey)
                    .is_some_and(|s| s.kind == crate::LevelKind::Storey)
            }) {
                if !geschosse.contains(&r.storey) {
                    geschosse.push(r.storey);
                }
            }
            if *id == run {
                fassade = Some(aussen);
            }
        }
        let fassade = fassade.unwrap_or_else(|| platte.to_vec());
        let monate = vorhaltemonate(geschosse.len());
        let huelle = convex_hull(&punkte);
        let zaun = offset_perimeter(&huelle, ZAUN_ABSTAND);
        let laenge = offset_perimeter(&fassade, GERUEST_ABSTAND);
        let hoehe = (krone + GERUEST_UEBERSTAND - gelaende).max(0.0);
        let m = |mm: f64| format!("{:.2} m", mm / 1000.0).replace('.', ",");
        let menge = |key, unit, value, kg, formula| AutoMenge {
            key,
            unit,
            value,
            kg: Some(kg),
            formula,
            ..vorlage.clone()
        };
        let geruest = menge(
            SCAFFOLD,
            "m2",
            laenge * hoehe,
            KG_GERUEST,
            format!("{} Länge × {} Höhe", m(laenge), m(hoehe)),
        );
        if !erste {
            return vec![geruest];
        }
        vec![
            menge(LUMP, "psch", 1.0, KG_BE, "1 je Gebäude".into()),
            menge(
                MONTHS,
                "mon",
                monate as f64,
                KG_BE,
                format!(
                    "2 + {} Geschosse ÷ 2, aufgerundet = {monate} Monate",
                    geschosse.len()
                ),
            ),
            menge(
                FENCE,
                "m",
                zaun,
                KG_BE,
                format!(
                    "{} Hülle des Gebäudes, mit {} Abstand",
                    m(polygon::perimeter(&huelle)),
                    m(ZAUN_ABSTAND)
                ),
            ),
            geruest,
        ]
    }
}
