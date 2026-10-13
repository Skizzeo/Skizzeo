//! Verzeichnis für einen lesenden Gang über das ganze Modell (Mengenliste,
//! Kosten, AVA, Export): Decken, Gründung, gestapelte Züge und Anschlüsse
//! je Zug, dazu ein Raster über Öffnungen und Züge, einmal gesammelt. Ohne
//! es sucht jeder Wandkörper das ganze Modell ab, und ein Gang über alle
//! Züge wächst mit dem Quadrat der Modellgröße. Methoden von `Model`, aus
//! `model.rs` ausgelagert.
//!
//! Das Verzeichnis gilt nur in [`Model::lesend`]: Solange es steht, hält
//! der Gang das Modell geliehen, ändern kann es niemand. Danach ist es weg.

use super::*;
use crate::{BuildingId, StoreyId};
use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::Arc;

/// Was die Suchen über das ganze Modell liefern, je Schlüssel in ihrer
/// Reihenfolge (die der Bauteile, Züge bzw. Anschlüsse), damit jede Suche
/// mit Verzeichnis dasselbe gibt wie ohne.
#[derive(Debug, Default)]
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
    /// [`Model::runs_above`].
    darueber: HashMap<RunId, Vec<RunId>>,
    /// Stellen in `joins`, an denen der Zug `a_run` oder `b_run` ist.
    anschluesse: HashMap<RunId, Vec<usize>>,
    /// Öffnungen je Gebäude über dem Rechteck ihres Pfades.
    oeffnungen: Raster<Option<BuildingId>, ElementId>,
    /// Züge je Geschoss über dem Rechteck ihrer Punkte.
    zuege: Raster<StoreyId, RunId>,
    /// Alle Züge über dem Rechteck ihrer Punkte.
    alle_zuege: Raster<(), RunId>,
}

/// Kantenlänge eines Rasterfelds (mm), etwa ein Haus.
const FELD: f64 = 10_000.0;
/// Rechtecke über mehr Felder stehen in jedem Feld (sehr lange Züge).
const MAX_FELDER: i64 = 4096;

/// Rechtecke (x/y) je Schlüssel `K` in der Reihenfolge des Eintragens, dazu
/// ein Raster: je Feld die Stellen, deren Rechteck es trifft.
#[derive(Debug)]
struct Raster<K, T> {
    liste: HashMap<K, Vec<(T, Vec3, Vec3)>>,
    felder: HashMap<(K, i64, i64), Vec<usize>>,
    ueberall: HashMap<K, Vec<usize>>,
}

impl<K, T> Default for Raster<K, T> {
    fn default() -> Self {
        Raster {
            liste: HashMap::new(),
            felder: HashMap::new(),
            ueberall: HashMap::new(),
        }
    }
}

/// Felder des Rechtecks `lo`–`hi` als Spannen, `None` ohne Fläche.
fn spannen(lo: Vec3, hi: Vec3) -> Option<(i64, i64, i64, i64)> {
    let k = |x: f64| (x / FELD).floor() as i64;
    (lo.x <= hi.x && lo.y <= hi.y).then(|| (k(lo.x), k(hi.x), k(lo.y), k(hi.y)))
}

impl<K: std::hash::Hash + Eq + Copy, T: Copy> Raster<K, T> {
    /// Trägt `t` mit dem Rechteck seiner Punkte ein (ohne Punkte nicht: es
    /// trifft nichts).
    fn rein(&mut self, k: K, t: T, pts: impl Iterator<Item = Vec3>) {
        let mut lo = vec3(f64::INFINITY, f64::INFINITY, 0.0);
        let mut hi = vec3(f64::NEG_INFINITY, f64::NEG_INFINITY, 0.0);
        for p in pts {
            (lo.x, lo.y) = (lo.x.min(p.x), lo.y.min(p.y));
            (hi.x, hi.y) = (hi.x.max(p.x), hi.y.max(p.y));
        }
        let Some((x0, x1, y0, y1)) = spannen(lo, hi) else {
            return;
        };
        let liste = self.liste.entry(k).or_default();
        let i = liste.len();
        liste.push((t, lo, hi));
        if (x1 - x0 + 1).saturating_mul(y1 - y0 + 1) > MAX_FELDER {
            self.ueberall.entry(k).or_default().push(i);
            return;
        }
        for x in x0..=x1 {
            for y in y0..=y1 {
                self.felder.entry((k, x, y)).or_default().push(i);
            }
        }
    }

    /// Alles unter `k`, dessen Rechteck das Rechteck `lo`–`hi` trifft (Rand
    /// eingeschlossen), in der Reihenfolge des Eintragens.
    fn bei(&self, k: K, lo: Vec3, hi: Vec3) -> Vec<T> {
        let (Some(liste), Some((x0, x1, y0, y1))) = (self.liste.get(&k), spannen(lo, hi)) else {
            return Vec::new();
        };
        let mut stellen: Vec<usize> = self.ueberall.get(&k).cloned().unwrap_or_default();
        if (x1 - x0 + 1).saturating_mul(y1 - y0 + 1) > MAX_FELDER {
            stellen = (0..liste.len()).collect();
        } else {
            for x in x0..=x1 {
                for y in y0..=y1 {
                    stellen.extend(self.felder.get(&(k, x, y)).into_iter().flatten());
                }
            }
        }
        stellen.sort_unstable();
        stellen.dedup();
        stellen
            .into_iter()
            .map(|i| &liste[i])
            .filter(|(_, a, z)| a.x <= hi.x && lo.x <= z.x && a.y <= hi.y && lo.y <= z.y)
            .map(|(t, _, _)| *t)
            .collect()
    }
}

/// Platz für das Verzeichnis im Modell. Eine Kopie des Modells beginnt
/// ohne: Sie kann geändert werden, während das Original noch liest.
#[derive(Default)]
pub(crate) struct Merker(RefCell<Option<Arc<Verzeichnis>>>);

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
    /// Führt `f` mit dem [`Verzeichnis`] aus: Jeder Wandkörper, jede Decke
    /// findet dann Anschlüsse, Decken, Züge darüber und Öffnungen ohne
    /// Suche über das ganze Modell. Verschachtelt baut nur der äußerste
    /// Aufruf.
    pub fn lesend<R>(&self, f: impl FnOnce() -> R) -> R {
        if self.merker.0.borrow().is_some() {
            return f();
        }
        let v = Arc::new(self.verzeichnis());
        *self.merker.0.borrow_mut() = Some(v);
        let _abbau = Abbau(&self.merker);
        f()
    }

    /// Das Verzeichnis, solange [`Model::lesend`] läuft.
    pub(crate) fn verz(&self) -> Option<Arc<Verzeichnis>> {
        self.merker.0.borrow().clone()
    }

    fn verzeichnis(&self) -> Verzeichnis {
        let mut v = Verzeichnis::default();
        // Wer trägt welches Segment (wie `runs_above`: über die Segmente
        // des Zuges darunter, nicht über den Zug in der Wand)
        let mut traeger: HashMap<ElementId, Vec<RunId>> = HashMap::new();
        for (id, r) in self.runs.iter() {
            for e in &r.segments {
                traeger.entry(*e).or_default().push(id);
            }
            v.zuege.rein(r.storey, id, r.points.iter().copied());
            v.alle_zuege.rein((), id, r.points.iter().copied());
        }
        for (id, e) in self.elements.iter() {
            if matches!(
                e.kind,
                ElementKind::GroundSlab(_)
                    | ElementKind::StripFooting(_)
                    | ElementKind::PerimeterInsulation { .. }
            ) {
                if let Some(r) = self.run_of(id) {
                    v.gruendung.entry(r).or_default().push(id);
                }
            }
            match &e.kind {
                ElementKind::Floor(f) => v.decken.entry(f.run).or_default().push(id),
                ElementKind::Wall(Wall {
                    run,
                    coupling: Some(c),
                    ..
                }) => {
                    for below in traeger.get(&c.below).into_iter().flatten() {
                        let ueber = v.darueber.entry(*below).or_default();
                        if !ueber.contains(run) {
                            ueber.push(*run);
                        }
                    }
                }
                ElementKind::GroundSlab(p) => v.platten.entry(p.run).or_default().push(id),
                ElementKind::StripFooting(f) => v.schuerzen.entry(f.slab).or_default().push(id),
                ElementKind::Opening(op) => {
                    let b = self.storey(e.storey).and_then(|s| s.building);
                    // Mit der Neigungsreserve wie der Vortest in holes_of
                    let (lo, hi) = super::oeffnung_model::pfad_rahmen(&op.path);
                    v.oeffnungen.rein(b, id, [lo, hi].into_iter());
                }
                _ => {}
            }
        }
        for (k, j) in self.joins.iter().enumerate() {
            v.anschluesse.entry(j.a_run).or_default().push(k);
            if j.b_run != j.a_run {
                v.anschluesse.entry(j.b_run).or_default().push(k);
            }
        }
        v
    }
}

impl Verzeichnis {
    pub(crate) fn decken(&self, run: RunId) -> Vec<ElementId> {
        self.decken.get(&run).cloned().unwrap_or_default()
    }

    pub(crate) fn darueber(&self, run: RunId) -> Vec<RunId> {
        self.darueber.get(&run).cloned().unwrap_or_default()
    }

    pub(crate) fn anschluesse(&self, run: RunId) -> &[usize] {
        self.anschluesse.get(&run).map_or(&[], |v| &v[..])
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
            let ohne = alle(&m);
            let mit = m.lesend(|| {
                assert!(m.verz().is_some());
                // verschachtelt: baut nicht neu und räumt nicht ab
                m.lesend(|| ());
                assert!(m.verz().is_some());
                alle(&m)
            });
            assert!(m.verz().is_none());
            assert_eq!(ohne, mit);
            assert!(ohne.iter().any(|x| x.0.is_some()));
            assert_eq!(crate::qto::schedule_im_gang(&m), crate::qto::schedule(&m));
            let ids: Vec<_> = m.elements().ids().collect();
            let gruendung = |m: &Model| {
                ids.iter()
                    .map(|e| crate::qto::gruendung_von(m, *e))
                    .collect::<Vec<_>>()
            };
            let ohne = gruendung(&m);
            assert_eq!(ohne, m.lesend(|| gruendung(&m)));
            assert!(ohne.iter().any(|g| g.len() >= 2));
        }
    }

    /// Öffnungen gelten am Zug mit Verzeichnis wie ohne (r7 hat welche).
    #[test]
    fn loecher_mit_verzeichnis_wie_ohne() {
        let m = szo::read(HAEUSER[5], GuidGen::with_seed(1))
            .expect("lädt")
            .model;
        let ids: Vec<RunId> = m.runs().ids().collect();
        let ohne: Vec<_> = ids
            .iter()
            .filter_map(|r| m.chain(*r))
            .map(|c| c.joints.holes)
            .collect();
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
        let ohne = zuege(&m);
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
                        assert_eq!(site_mengen(&m, r, &platte, &vorlage, gelaende, erste), alt);
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
