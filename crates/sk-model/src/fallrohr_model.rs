//! Regenfallrohre am Flachdach (Jörn 11.10., [`crate::fallrohr`]): Methoden
//! von `Model`. Je Ablauf eines Dachaufbaus genau ein Bauteil RF auf der
//! Ebene des Dachaufbaus; es kommt und geht mit den Abläufen.

use super::*;
use crate::fallrohr::{Downpipe, DownpipeSpec, Foot, Overflow, SPACING_MAX, SPACING_MIN};
use crate::roof::FlatRoof;

impl Model {
    /// Regenfallrohre des Dachaufbaus `roof`, nach der Nummer ihres
    /// Ablaufs.
    pub fn downpipes_of(&self, roof: ElementId) -> Vec<ElementId> {
        let mut out: Vec<(u16, ElementId)> = self
            .elements
            .iter()
            .filter_map(|(id, e)| match e.kind {
                ElementKind::Downpipe { roof: r, drain, .. } if r == roof => Some((drain, id)),
                _ => None,
            })
            .collect();
        out.sort_by_key(|x| x.0);
        out.into_iter().map(|x| x.1).collect()
    }

    /// Alle Regenfallrohre.
    pub(super) fn downpipe_ids(&self) -> Vec<ElementId> {
        self.elements
            .iter()
            .filter(|(_, e)| matches!(e.kind, ElementKind::Downpipe { .. }))
            .map(|(id, _)| id)
            .collect()
    }

    /// Regenfallrohr am Ablauf `k` des Dachaufbaus `roof`.
    pub fn downpipe_at(&self, roof: ElementId, k: usize) -> Option<ElementId> {
        self.elements
            .iter()
            .find(|(_, e)| {
                matches!(e.kind, ElementKind::Downpipe { roof: r, drain, .. }
                    if r == roof && drain as usize == k)
            })
            .map(|(id, _)| id)
    }

    /// Decke unter dem Dachaufbau `roof`.
    pub fn floor_of_roof(&self, roof: ElementId) -> Option<ElementId> {
        match self.element(roof)?.kind {
            ElementKind::Roof { floor, .. } => Some(floor),
            _ => None,
        }
    }

    /// Prüfung (Model::check): das Regenfallrohr hängt an einem Ablauf.
    pub(super) fn check_downpipe(
        &self,
        number: &str,
        roof: ElementId,
        drain: u16,
    ) -> Option<String> {
        let n = self.floor_of_roof(roof).map_or(0, |f| self.drain_count(f));
        ((drain as usize) >= n).then(|| format!("{number}: Regenfallrohr ohne Ablauf"))
    }

    /// Abläufe des Flachdachs über der Decke `floor`, an denen die
    /// Regenfallrohre hängen (= `roof.drains`).
    fn drain_count(&self, floor: ElementId) -> usize {
        self.flat_roof_over(floor).map_or(0, |r| r.downpipes.len())
    }

    /// Eigenschaften des Regenfallrohrs `id`.
    pub fn downpipe_spec(&self, id: ElementId) -> Option<DownpipeSpec> {
        match self.element(id)?.kind {
            ElementKind::Downpipe { spec, .. } => Some(spec),
            _ => None,
        }
    }

    /// Ausgelegtes Regenfallrohr `id` (Lage, Höhen, Schellen).
    pub fn downpipe(&self, id: ElementId) -> Option<Downpipe> {
        let ElementKind::Downpipe { roof, drain, .. } = self.element(id)?.kind else {
            return None;
        };
        let ElementKind::Roof { floor, .. } = self.element(roof)?.kind else {
            return None;
        };
        self.flat_roof_over(floor)?
            .downpipes
            .get(drain as usize)
            .cloned()
    }

    /// Setzt die Eigenschaften des Regenfallrohrs `id`; Schellenabstand
    /// [`SPACING_MIN`] bis [`SPACING_MAX`].
    pub fn set_downpipe_spec(&mut self, id: ElementId, spec: DownpipeSpec) -> bool {
        let Some(old) = self.downpipe_spec(id) else {
            return false;
        };
        if !(SPACING_MIN..=SPACING_MAX).contains(&spec.spacing) {
            return false;
        }
        if old == spec {
            return true;
        }
        note!(self, Element, self.elements, id);
        if let Some(ElementKind::Downpipe { spec: s, .. }) =
            self.elements.get_mut(id).map(|e| &mut e.kind)
        {
            *s = spec;
        }
        self.touch();
        true
    }

    /// Regenfallrohre an den Abläufen des Flachdachs über der Decke
    /// `floor`, dessen Geometrie `r` gerade entsteht; Eigenschaften vom
    /// Bauteil, sonst die Vorgabe. Ein Dach ohne gespeicherte Abläufe
    /// (ohne Gefälle, Review W12 und Kosten B30 an #68) bekommt sie am
    /// Vorschlag, wie ihn „Abläufe vorschlagen“ setzen würde; ein Dach mit
    /// 0 % behält seine gespeicherten Abläufe. `roof.drains` zählt dieselben
    /// Punkte.
    pub(super) fn layout_downpipes(&self, floor: ElementId, r: &FlatRoof) -> Vec<Downpipe> {
        let Some(roof) = self.flat_roof_of(floor) else {
            return Vec::new();
        };
        let Some(ElementKind::Roof { drainage, .. }) = self.element(roof).map(|e| &e.kind) else {
            return Vec::new();
        };
        let b = self.building_of_element(roof);
        let foot = |p: Vec3| self.foot_at(floor, b, p);
        let vorschlag;
        let drains = if drainage.drains.is_empty() {
            vorschlag =
                crate::gefaelle::propose_drains(&r.outline, &crate::gefaelle::Limits::default());
            &vorschlag
        } else {
            &drainage.drains
        };
        drains
            .iter()
            .take(MAX_DOWNPIPES as usize)
            .enumerate()
            .map(|(k, p)| {
                let spec = self
                    .downpipe_at(roof, k)
                    .and_then(|d| self.downpipe_spec(d))
                    .unwrap_or_default();
                Downpipe::layout(r, *p, foot, spec)
            })
            // alle oder keins: der Index bleibt der des Ablaufs (Review #68)
            .collect::<Option<Vec<_>>>()
            .unwrap_or_default()
    }

    /// Fläche unter dem Grundrisspunkt `p` außen vor dem Flachdach über der
    /// Decke `floor` (Prüfung Geometrie #68): das oberste tiefere Geschoss,
    /// dessen Außenfläche `p` umschließt (zurückgesetzte Wand, davor Dach
    /// bzw. Dachterrasse), mit OK Belag bzw. OK Rohdecke; sonst das Gelände.
    fn foot_at(&self, floor: ElementId, b: Option<BuildingId>, p: Vec3) -> Foot {
        let mut cur = self.run_of(floor);
        while let Some(below) = cur.and_then(|r| self.run_below(r)) {
            let c = self.chain(below);
            if c.is_some_and(|c| contains(&c.face_corners(c.outer_offset()), p)) {
                if let Some(Ok(f)) = self.floor(below) {
                    let z = f.terrace_band().map_or(f.params.top, |t| t.1);
                    return Foot { z, lower: true };
                }
            }
            cur = Some(below);
        }
        Foot {
            z: self.terrain_z_at(b, p.x, p.y),
            lower: false,
        }
    }

    /// Legt fehlende Regenfallrohre an und entfernt überzählige: je Ablauf
    /// eines Dachaufbaus im Bereich `scope` genau eins, auf dessen Ebene.
    /// Neue übernehmen die Eigenschaften eines Nachbarn am selben Dach.
    pub(super) fn sync_downpipes(&mut self, scope: Option<&[RunId]>) {
        let roofs: Vec<(ElementId, usize, StoreyId, u16)> = self
            .elements
            .iter()
            .filter_map(|(id, e)| match &e.kind {
                ElementKind::Roof { floor, .. } if self.floor_in_scope(*floor, scope) => {
                    Some((id, self.drain_count(*floor), e.storey, e.seq))
                }
                _ => None,
            })
            .collect();
        let have: Vec<(ElementId, ElementId, u16, StoreyId, DownpipeSpec)> = self
            .elements
            .iter()
            .filter_map(|(id, e)| match e.kind {
                ElementKind::Downpipe { roof, drain, spec } => {
                    Some((id, roof, drain, e.storey, spec))
                }
                _ => None,
            })
            .collect();
        let mut kept: Vec<(ElementId, u16, DownpipeSpec)> = Vec::new();
        for (id, roof, k, storey, spec) in have {
            let keep = match roofs.iter().find(|r| r.0 == roof) {
                Some(r) => {
                    (k as usize) < r.1
                        && r.2 == storey
                        && !kept.iter().any(|x| x.0 == roof && x.1 == k)
                }
                // Dach außerhalb des Bereichs: bleibt, wie es ist
                None => matches!(
                    self.element(roof).map(|e| &e.kind),
                    Some(ElementKind::Roof { .. })
                ),
            };
            if keep {
                kept.push((roof, k, spec));
            } else {
                note!(self, Element, self.elements, id);
                self.elements.remove(id);
                self.touch();
            }
        }
        for (roof, n, storey, seq) in roofs {
            let spec = kept
                .iter()
                .find(|x| x.0 == roof)
                .map_or_else(DownpipeSpec::default, |x| x.2);
            for k in 0..n as u16 {
                if kept.iter().any(|x| x.0 == roof && x.1 == k) {
                    continue;
                }
                let kind = ElementKind::Downpipe {
                    roof,
                    drain: k,
                    spec,
                };
                let id = self.new_element(Category::Downpipe, storey, seq, kind);
                // beim Lesen kann die Guid-Folge schon vergebene liefern
                // (gleicher Startwert wie beim Schreiben): weiterziehen
                while self.guid_taken(id) {
                    let g = self.new_guid();
                    if let Some(e) = self.elements.get_mut(id) {
                        e.guid = g;
                    }
                }
                kept.push((roof, k, spec));
                self.touch();
            }
        }
    }

    /// Der Ablauf `k` des Dachaufbaus `roof` fällt weg: sein Regenfallrohr
    /// auch, die dahinter rücken einen Ablauf vor (Nummer und Eigenschaften
    /// bleiben am Bauteil).
    pub(super) fn drop_downpipe(&mut self, roof: ElementId, k: usize) {
        for id in self.downpipes_of(roof) {
            let Some(ElementKind::Downpipe { drain, .. }) = self.element(id).map(|e| &e.kind)
            else {
                continue;
            };
            let drain = *drain as usize;
            if drain < k {
                continue;
            }
            note!(self, Element, self.elements, id);
            if drain == k {
                self.elements.remove(id);
            } else if let Some(ElementKind::Downpipe { drain: d, .. }) =
                self.elements.get_mut(id).map(|e| &mut e.kind)
            {
                *d -= 1;
            }
            self.touch();
        }
    }

    /// Trägt schon etwas anderes im Projekt die Guid von `id` (wie
    /// [`Model::check`] sie sammelt)?
    fn guid_taken(&self, id: ElementId) -> bool {
        let Some(g) = self.element(id).map(|e| e.guid) else {
            return false;
        };
        self.elements.iter().any(|(i, e)| i != id && e.guid == g)
            || self.runs.iter().any(|(_, r)| r.guid == g)
            || self.storeys.iter().any(|(_, s)| s.guid == g)
            || self.buildings.iter().any(|(_, b)| b.guid == g)
            || self.materials.iter().any(|(_, m)| m.guid == g)
            || self.layer_sets.iter().any(|(_, t)| t.guid == g)
            || self.attr.guids().any(|x| x == g)
            || self.project.guid == g
    }

    /// Abgeleitete Merkmale des Regenfallrohrs (Bauteileigenschaften):
    /// Nennweite, Längen, Schellen, Notentwässerung, Kontrollablauf.
    pub(super) fn downpipe_props(&self, id: ElementId) -> Vec<(String, PropValue)> {
        let Some(d) = self.downpipe(id) else {
            return Vec::new();
        };
        let mut out = vec![
            ("Nennweite".into(), PropValue::Text("DN 100".into())),
            ("Länge Fallrohr".into(), PropValue::Number(d.pipe_length())),
            (
                "Rohrschellen".into(),
                PropValue::Number(d.clamp_count() as f64),
            ),
            (
                "Notentwässerung".into(),
                PropValue::Text(
                    match d.spec.overflow {
                        Overflow::Pipe => "Notfallrohr",
                        Overflow::Spout => "Wasserspeier",
                    }
                    .into(),
                ),
            ),
            (
                "Kontrollablauf".into(),
                PropValue::Bool(d.control.is_some()),
            ),
        ];
        if d.spec.overflow == Overflow::Pipe {
            out.push((
                "Länge Notfallrohr".into(),
                PropValue::Number(d.overflow_length()),
            ));
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sk_math::vec3;

    /// Prüfhaus 10 × 8 m, EG und OG in AW-31,5, Flachdach auf dem OG.
    fn haus() -> (Model, ElementId, StoreyId) {
        let mut m = Model::with_seed(12);
        let b = m.add_building(2);
        let pts = [
            vec3(0.0, 0.0, 0.0),
            vec3(0.0, 8000.0, 0.0),
            vec3(10000.0, 8000.0, 0.0),
            vec3(10000.0, 0.0, 0.0),
        ];
        let set = m.type_by_guid(crate::EXTERIOR_TYPE_GUID).unwrap();
        let eg = m.ground_of(Some(b)).unwrap();
        let eg = m
            .add_wall_run(&pts, true, RefSide::Left, eg, set, Category::ExteriorWall)
            .unwrap();
        let og = m.runs_above(eg)[0];
        let st = m.run(og).unwrap().storey;
        m.begin("Flachdach");
        assert!(m.set_flat_roof(st, true));
        m.commit().unwrap();
        let da = m.flat_roof_of(m.floor_of(og).unwrap()).unwrap();
        (m, da, st)
    }

    fn schritt(m: &mut Model, f: impl FnOnce(&mut Model) -> bool) -> crate::txn::Txn {
        m.begin("Regenfallrohr");
        assert!(f(m));
        m.commit().expect("Schritt ändert etwas")
    }

    /// Sollwerte am Prüfhaus (Konzept §4): Flachdach 21,5 auf OK Rohdecke
    /// 5,71 m, Dachhaut am Ablauf 5,925 m, Gelände ±0. Vier Abläufe an den
    /// langen Seiten, 60 cm von den Ecken, je ein RF auf der Ebene des
    /// Dachs.
    #[test]
    fn sollwerte_pruefhaus() {
        let (m, da, _) = haus();
        let rf = m.downpipes_of(da);
        assert_eq!(rf.len(), 4);
        let e = m.element(rf[0]).unwrap();
        assert_eq!(e.number, "RF-001");
        assert_eq!(e.category, Category::Downpipe);
        assert_eq!(e.storey, m.element(da).unwrap().storey);
        assert!(m.check().is_empty(), "{:?}", m.check());
        let d = m.downpipe(rf[0]).unwrap();
        // Ablauf auf der Innenfläche der Nordseite, Kasten an der Fassade
        assert_eq!((d.drain.x, d.drain.y), (9085.0, 7685.0));
        assert_eq!((d.face.x, d.face.y), (9085.0, 8000.0));
        assert_eq!((d.out.x, d.out.y), (0.0, 1.0));
        // Auslauf 6 cm unter der Dachhaut, Kasten 10 cm darüber bis 10 cm
        // unter den Kontrollablauf (4 cm über Rohdecke)
        assert_eq!(d.outlet, 5865.0);
        assert_eq!((d.box_bot, d.box_top), (5650.0, 5965.0));
        assert_eq!(d.control, Some(5750.0));
        // Fallrohr bis OK Standrohr 0,70 m: 4,95 m, Schellen alle 1,50 m
        assert_eq!((d.stand_top, d.ground), (700.0, 0.0));
        assert_eq!(d.pipe_length(), 4950.0);
        assert_eq!(d.clamps, [5500.0, 4000.0, 2500.0, 1000.0]);
        // Notüberlauf 35 mm über der Dachhaut, 55 cm neben dem Ablauf zur
        // Mitte der Seite hin;
        // Notfallrohr bis 20 cm über Gelände
        let o = &d.overflow;
        assert_eq!(o.sill, 5960.0);
        assert_eq!((o.face.x, o.face.y), (8535.0, 8000.0));
        assert_eq!(o.end, 200.0);
        assert_eq!(d.overflow_length(), 5580.0);
        assert_eq!(d.clamp_count(), 8);
        // das dritte an der Südseite, nach außen −y
        let d2 = m.downpipe(rf[2]).unwrap();
        assert_eq!((d2.face.x, d2.face.y, d2.out.y), (915.0, 0.0, -1.0));
        assert_eq!(d2.overflow.face.x, 1465.0);
        assert_eq!(d2.pipe_length(), 4950.0);
        // Automatikmengen je RF
        let s = crate::qto::schedule(&m);
        let auto = |k: &str| {
            s.auto
                .iter()
                .find(|a| a.key == k && a.element == rf[0])
                .map(|a| (a.value, a.unit, a.kg))
        };
        use crate::fallrohr::*;
        assert_eq!(auto(BOX), Some((1.0, "st", Some(363))));
        assert_eq!(auto(PIPE), Some((4950.0, "m", Some(363))));
        assert_eq!(auto(CLAMPS), Some((8.0, "st", Some(363))));
        assert_eq!(auto(STAND).map(|a| a.0), Some(1.0));
        assert_eq!(auto(OV_BOX).map(|a| a.0), Some(1.0));
        assert_eq!(auto(OV_PIPE).map(|a| a.0), Some(5580.0));
        assert_eq!(auto(OUTLET).map(|a| a.0), Some(1.0));
        assert_eq!(auto(CONTROL).map(|a| a.0), Some(1.0));
        assert_eq!(auto(SPOUT), None);
        // Merkmale für Bauteileigenschaften
        let p = m.props_of(rf[0]);
        assert_eq!(p.get("Länge Fallrohr"), Some(&PropValue::Number(4950.0)));
        assert_eq!(p.get("Kontrollablauf"), Some(&PropValue::Bool(true)));
        // abgeleitet: Löschen nur über die Abläufe am Flachdach
        assert!(matches!(
            m.can_delete(rf[0]),
            Err(crate::Refusal::Derived { .. })
        ));
    }

    /// Geometrie (Prüfung #68): OG-Nordwand 1,50 m zurückgesetzt, davor
    /// die Dachterrasse auf der EG-Decke. Die Rohre an der Nordseite dürfen
    /// nicht durch Terrasse und EG-Decke bis aufs Gelände laufen: Wo die
    /// Achse über dem EG liegt, enden sie über der Decke darunter.
    #[test]
    fn nicht_durch_die_terrasse() {
        let mut m = Model::with_seed(12);
        let b = m.add_building(2);
        let pts = [
            vec3(0.0, 0.0, 0.0),
            vec3(0.0, 8000.0, 0.0),
            vec3(10000.0, 8000.0, 0.0),
            vec3(10000.0, 0.0, 0.0),
        ];
        let set = m.type_by_guid(crate::EXTERIOR_TYPE_GUID).unwrap();
        let eg = m.ground_of(Some(b)).unwrap();
        let eg = m
            .add_wall_run(&pts, true, RefSide::Left, eg, set, Category::ExteriorWall)
            .unwrap();
        let og = m.runs_above(eg)[0];
        let nord = m.wall_at(og, 1).unwrap();
        schritt(&mut m, |m| m.set_offset(nord, -1500.0).is_some());
        let st = m.run(og).unwrap().storey;
        schritt(&mut m, |m| m.set_flat_roof(st, true));
        let da = m.flat_roof_of(m.floor_of(og).unwrap()).unwrap();
        let ok_og = m.storey(st).unwrap().elevation;
        let c = m.chain(eg).unwrap();
        let unten = sk_math::polygon::to_ccw(&c.face_corners(c.outer_offset()));
        let ueber_eg = |p: Vec3| {
            let n = unten.len();
            (0..n).fold(false, |inn, i| {
                let (a, b) = (unten[i], unten[(i + 1) % n]);
                let quer = (a.y > p.y) != (b.y > p.y)
                    && p.x < a.x + (p.y - a.y) / (b.y - a.y) * (b.x - a.x);
                inn != quer
            })
        };
        let mut nordseite = 0;
        for id in m.downpipes_of(da) {
            let d = m.downpipe(id).unwrap();
            let ov = d.overflow.face + d.out * (crate::fallrohr::WALL_GAP + 50.0);
            if ueber_eg(d.axis()) {
                nordseite += 1;
                assert!(d.stand_top >= ok_og, "Fallrohr bis {}", d.stand_top);
            }
            if ueber_eg(ov) {
                assert!(
                    d.overflow.end >= ok_og,
                    "Notfallrohr bis {}",
                    d.overflow.end
                );
            }
        }
        assert!(nordseite > 0);
        // über der Terrasse kein Standrohr, dafür ein Auslaufbogen mehr
        let s = crate::qto::schedule(&m);
        let zahl = |k: &str| s.auto.iter().filter(|a| a.key == k).count();
        let boegen: f64 = (s.auto.iter())
            .filter(|a| a.key == crate::fallrohr::OUTLET)
            .map(|a| a.value)
            .sum();
        assert_eq!(zahl(crate::fallrohr::STAND), 4 - nordseite);
        assert_eq!(boegen, (4 + nordseite) as f64);
    }

    /// Review W12 und Kosten B30 an #68: Ein Dach ohne gespeicherte Abläufe
    /// bekommt die Regenfallrohre am Vorschlag, und `roof.drains` und
    /// `roof.overflows` zählen dieselben Punkte.
    #[test]
    fn ohne_gefaelle_am_vorschlag() {
        let (mut m, da, _) = haus();
        let d = crate::gefaelle::Drainage::default();
        schritt(&mut m, |m| m.set_roof_drainage(da, d));
        assert!(m.drainage_of_roof(da).unwrap().drains.is_empty());
        let rf = m.downpipes_of(da);
        assert_eq!(rf.len(), 4);
        assert!(m.check().is_empty(), "{:?}", m.check());
        let s = crate::qto::schedule(&m);
        let auto = |k: &str| {
            s.auto
                .iter()
                .find(|a| a.key == k && a.element == da)
                .map(|a| a.value)
        };
        assert_eq!(auto("roof.drains"), Some(4.0));
        assert_eq!(auto("roof.overflows"), Some(4.0));
        let boxes = (s.auto.iter())
            .filter(|a| a.key == crate::fallrohr::BOX)
            .count();
        assert_eq!(boxes, 4);
    }

    /// Wasserspeier statt Notfallrohr, ohne Kontrollablauf, Schellen
    /// höchstens 3 m; außerhalb 1–3 m abgelehnt.
    #[test]
    fn eigenschaften() {
        let (mut m, da, _) = haus();
        let rf = m.downpipes_of(da)[0];
        let spec = DownpipeSpec {
            overflow: Overflow::Spout,
            control: false,
            spacing: 3000.0,
        };
        schritt(&mut m, |m| m.set_downpipe_spec(rf, spec));
        let d = m.downpipe(rf).unwrap();
        assert_eq!(d.control, None);
        // ohne Kontrollablauf reicht der Kasten 30 cm unter den Deckel
        assert_eq!(d.box_bot, 5665.0);
        assert_eq!(d.clamps.len(), 3);
        assert_eq!(d.overflow_length(), 0.0);
        assert_eq!(d.clamp_count(), 3);
        let s = crate::qto::schedule(&m);
        let keys: Vec<&str> = (s.auto.iter())
            .filter(|a| a.element == rf)
            .map(|a| a.key)
            .collect();
        use crate::fallrohr::*;
        assert_eq!(keys, [BOX, PIPE, CLAMPS, STAND, SPOUT]);
        m.begin("falsch");
        for x in [999.0, 3001.0] {
            let falsch = DownpipeSpec { spacing: x, ..spec };
            assert!(!m.set_downpipe_spec(rf, falsch));
        }
        assert!(m.commit().is_none());
    }

    /// Je Ablauf genau ein RF: weniger Abläufe nehmen es mit, mehr legen
    /// eins an (mit den Eigenschaften des Nachbarn); Rückgängig bringt es
    /// mit Guid und Nummer zurück.
    #[test]
    fn folgt_den_ablaeufen() {
        let (mut m, da, og) = haus();
        let rf = m.downpipes_of(da);
        let guid = |m: &Model, id| m.element(id).unwrap().guid;
        let nummer = |m: &Model, id| m.element(id).unwrap().number.clone();
        let g: Vec<_> = rf.iter().map(|&id| guid(&m, id)).collect();
        let speier = DownpipeSpec {
            overflow: Overflow::Spout,
            ..DownpipeSpec::default()
        };
        schritt(&mut m, |m| m.set_downpipe_spec(rf[0], speier));
        let mut d = m.drainage_of_roof(da).unwrap().clone();
        let vier = d.drains.clone();
        d.drains.truncate(3);
        let t = schritt(&mut m, |m| m.set_roof_drainage(da, d.clone()));
        assert_eq!(m.downpipes_of(da), rf[..3]);
        assert!(m.check().is_empty(), "{:?}", m.check());
        m.apply(&t, crate::txn::Direction::Undo);
        let wieder = m.downpipes_of(da);
        assert_eq!(wieder.len(), 4);
        assert_eq!(guid(&m, wieder[3]), g[3]);
        assert_eq!(nummer(&m, wieder[3]), "RF-004");
        // ersten Ablauf löschen: RF-001 geht, die übrigen rücken vor und
        // behalten Nummer und Lage
        let t = schritt(&mut m, |m| m.remove_roof_drain(da, 0));
        let rest = m.downpipes_of(da);
        let n: Vec<String> = rest.iter().map(|&id| nummer(&m, id)).collect();
        assert_eq!(n, ["RF-002", "RF-003", "RF-004"]);
        assert_eq!(m.downpipe(rest[0]).unwrap().face.x, 915.0);
        assert!(m.check().is_empty(), "{:?}", m.check());
        m.apply(&t, crate::txn::Direction::Undo);
        assert_eq!(guid(&m, m.downpipes_of(da)[0]), g[0]);
        // ein fünfter Ablauf an der Westseite: RF-005 mit Speier
        d.drains = vier;
        d.drains.push(vec3(315.0, 4000.0, 0.0));
        schritt(&mut m, |m| m.set_roof_drainage(da, d));
        let fuenf = m.downpipes_of(da);
        assert_eq!(fuenf.len(), 5);
        assert_eq!(guid(&m, fuenf[0]), g[0]);
        assert_eq!(nummer(&m, fuenf[4]), "RF-005");
        assert_eq!(m.downpipe_spec(fuenf[4]), Some(speier));
        let w = m.downpipe(fuenf[4]).unwrap();
        assert_eq!((w.face.x, w.face.y, w.out.x), (0.0, 4000.0, -1.0));
        // Flachdach aus: alle RF weg
        schritt(&mut m, |m| m.set_flat_roof(og, false));
        assert!(m.downpipe_ids().is_empty());
    }

    /// `[downpipe]` liest sich bytegleich zurück; Abweichungen von der
    /// Vorgabe stehen in der Zeile, Werte außerhalb gelten mit Hinweis als
    /// Vorgabe; eine Datei ohne RF bekommt sie beim Lesen.
    #[test]
    fn datei() {
        let (mut m, da, _) = haus();
        let rf = m.downpipes_of(da)[1];
        let spec = DownpipeSpec {
            overflow: Overflow::Spout,
            control: false,
            spacing: 2500.0,
        };
        schritt(&mut m, |m| m.set_downpipe_spec(rf, spec));
        let text = crate::szo::write(&m);
        let zeilen: Vec<&str> = text
            .lines()
            .filter(|l| l.starts_with("[downpipe] "))
            .collect();
        assert_eq!(zeilen.len(), 4);
        assert!(zeilen
            .iter()
            .any(|l| l.contains(" overflow=spout control=0 spacing=2500 ")));
        let back = crate::szo::read(&text, GuidGen::with_seed(3)).unwrap();
        assert!(back.hints.is_empty(), "{:?}", back.hints);
        assert_eq!(crate::szo::write(&back.model), text);
        // außerhalb des Bereichs
        let kaputt = text.replace(" spacing=2500 ", " spacing=9000 ");
        let back = crate::szo::read(&kaputt, GuidGen::with_seed(3)).unwrap();
        assert_eq!(back.hints.len(), 1, "{:?}", back.hints);
        assert!(back.hints[0].contains("Schellenabstand"));
        // ältere Datei ohne Regenfallrohre
        let alt: String = (text.lines())
            .filter(|l| !l.starts_with("[downpipe] "))
            .map(|l| format!("{l}\n"))
            .collect();
        let back = crate::szo::read(&alt, GuidGen::with_seed(3)).unwrap();
        assert_eq!(back.hints, ["Regenfallrohre an den Abläufen ergänzt"]);
        assert_eq!(back.model.downpipe_ids().len(), 4);
    }
}
