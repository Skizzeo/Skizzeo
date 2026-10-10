//! Flachdach (Jörn 10.10., planung/flachdach/plan-heute.md D1–D4): die
//! Ebene „Flachdach“ über dem obersten Geschoss, die Aufkantung als
//! gekoppelter Zug im Außenwandtyp darunter, der waagerechte Dachaufbau und
//! das Attikablech auf der Krone der Aufkantung.

use super::*;

impl Model {
    /// Ebene Flachdach des Gebäudes `b` (`None`: der Vorlage), falls es eine
    /// hat.
    pub fn roof_level_of(&self, b: Option<BuildingId>) -> Option<StoreyId> {
        self.levels_in(b)
            .into_iter()
            .find(|id| self.storey(*id).is_some_and(|s| s.kind == LevelKind::Roof))
    }

    /// Ebene Flachdach des Gebäudes, zu dem das Geschoss `id` gehört.
    pub fn roof_level(&self, id: StoreyId) -> Option<StoreyId> {
        self.roof_level_of(self.storey(id)?.building)
    }

    /// Ist `wall` eine Aufkantung des Flachdachs?
    pub fn is_parapet(&self, wall: ElementId) -> bool {
        self.element(wall)
            .is_some_and(|e| e.category == Category::Parapet)
    }

    /// Aufkantungen auf der Ebene Flachdach `fd` (D2): über jedem
    /// geschlossenen Außenwandzug des Geschosses darunter, der noch keinen
    /// Zug darüber hat, ein gekoppelter Zug im selben Wandtyp. Alle Schichten
    /// laufen weiter; der Kern steht auf der Decke wie ein Geschoss höher.
    fn add_parapets(&mut self, fd: StoreyId) {
        let Some(below) = self.level_below(fd) else {
            return;
        };
        let roots: Vec<RunId> = self
            .runs
            .iter()
            .filter(|(_, r)| r.storey == below)
            .map(|(id, _)| id)
            .filter(|id| self.needs_floor(*id) && self.runs_above(*id).is_empty())
            .collect();
        for r in roots {
            self.stack_run(r, fd);
        }
    }

    /// Schaltet die Ebene Flachdach (Jörn 10.10.) über dem obersten
    /// Geschoss des Gebäudes ein oder aus, zu dem das Geschoss `id` gehört:
    /// Band „Flachdach“/„FD“ von OK Rohdecke darunter, [`ROOF_UPSTAND`]
    /// hoch. Ausschalten nimmt die Ebene samt allem, was auf ihr steht.
    /// `false` ohne Geschoss oder ohne Geschoss über der Gründung.
    pub fn set_flat_roof(&mut self, id: StoreyId, on: bool) -> bool {
        let Some(b) = self.storey(id).map(|s| s.building) else {
            return false;
        };
        match (on, self.roof_level_of(b)) {
            (true, Some(_)) | (false, None) => true,
            (true, None) => {
                let Some(top) = self
                    .levels_in(b)
                    .last()
                    .and_then(|t| self.storey(*t))
                    .filter(|s| s.kind == LevelKind::Storey)
                    .map(|s| s.top())
                else {
                    return false;
                };
                let fd = self.insert_storey(Storey {
                    guid: Guid(0),
                    building: b,
                    name: "Flachdach".into(),
                    short: "FD".into(),
                    kind: LevelKind::Roof,
                    elevation: top,
                    height: ROOF_UPSTAND,
                    embed: None,
                });
                self.add_parapets(fd);
                self.touch();
                true
            }
            (false, Some(fd)) => {
                let runs: Vec<RunId> = self
                    .runs
                    .iter()
                    .filter(|(_, r)| r.storey == fd)
                    .map(|(r, _)| r)
                    .collect();
                for r in runs {
                    self.remove_run(r);
                }
                note!(self, Storey, self.storeys, fd);
                self.storeys.remove(fd);
                self.touch();
                true
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::txn::Direction;
    use sk_math::vec3;

    /// Prüfhaus 10 × 8 m mit EG und OG im Wandtyp `typ` (Guid).
    fn haus(typ: Guid) -> (Model, RunId) {
        let mut m = Model::with_seed(12);
        let b = m.add_building(2);
        let pts = [
            vec3(0.0, 0.0, 0.0),
            vec3(0.0, 8000.0, 0.0),
            vec3(10000.0, 8000.0, 0.0),
            vec3(10000.0, 0.0, 0.0),
        ];
        let set = m.type_by_guid(typ).unwrap();
        let eg = m.ground_of(Some(b)).unwrap();
        let eg = m
            .add_wall_run(&pts, true, RefSide::Left, eg, set, Category::ExteriorWall)
            .unwrap();
        let og = m.runs_above(eg)[0];
        (m, og)
    }

    fn schritt(m: &mut Model, f: impl FnOnce(&mut Model) -> bool) -> Txn {
        m.begin("Flachdach");
        assert!(f(m));
        m.commit().expect("Schritt ändert etwas")
    }

    fn ak_runs(m: &Model) -> Vec<RunId> {
        m.runs()
            .iter()
            .filter(|(id, _)| m.category_of(*id) == Some(Category::Parapet))
            .map(|(id, _)| id)
            .collect()
    }

    /// D1/D2: Flachdach an und aus, rückgängig als ein Schritt.
    #[test]
    fn ebene_flachdach_mit_aufkantung() {
        let (mut m, og) = haus(EXTERIOR_TYPE_GUID);
        let og_storey = m.run(og).unwrap().storey;
        let vorher = crate::szo::write(&m);
        assert!(m.roof_level(og_storey).is_none());
        let t = schritt(&mut m, |m| m.set_flat_roof(og_storey, true));
        let fd = m.roof_level(og_storey).expect("FD");
        let st = m.storey(fd).unwrap().clone();
        assert_eq!((st.name.as_str(), st.short.as_str()), ("Flachdach", "FD"));
        assert_eq!(st.kind, LevelKind::Roof);
        assert_eq!(st.elevation, m.storey(og_storey).unwrap().top());
        assert_eq!(st.height, ROOF_UPSTAND);
        assert_eq!(m.level_above(og_storey), Some(fd));
        // Aufkantung: gekoppelt an die OG-Wand, gleicher Typ, keine Decke
        let ak = ak_runs(&m);
        assert_eq!(ak.len(), 1);
        assert_eq!(m.runs_above(og), ak);
        let r = m.run(ak[0]).unwrap().clone();
        assert_eq!(r.storey, fd);
        assert!(r.closed && r.segments.len() == 4);
        let og_set = m.element(m.wall_at(og, 0).unwrap()).unwrap().layer_set;
        for w in &r.segments {
            let e = m.element(*w).unwrap();
            assert!(e.number.starts_with("AK-"), "{}", e.number);
            assert_eq!(e.layer_set, og_set);
            assert_eq!(m.stack_offset(*w), Some((0.0, true)));
        }
        assert!(m.floor_of(ak[0]).is_none());
        assert!(m.check().is_empty(), "{:?}", m.check());
        // Höhe der Ebene = Höhe der Aufkantung
        let c = m.chain(ak[0]).unwrap();
        assert_eq!((c.base, c.height), (st.elevation, ROOF_UPSTAND));
        schritt(&mut m, |m| m.set_storey_height(fd, 800.0));
        assert_eq!(m.chain(ak[0]).unwrap().height, 800.0);
        m.begin("zu hoch");
        assert!(!m.set_storey_height(fd, MAX_ROOF_UPSTAND + 1.0));
        assert!(!m.set_storey_height(fd, MIN_ROOF_UPSTAND - 1.0));
        assert!(m.commit().is_none());
        // Aufkantung bleibt bündig und gekoppelt, löschen abgelehnt
        let w = r.segments[0];
        m.begin("ziehen");
        assert!(m.set_offset(w, 300.0).is_none());
        assert!(!m.set_linked(w, false));
        assert!(m.commit().is_none());
        assert!(matches!(m.can_delete(w), Err(Refusal::Derived { .. })));
        assert!(refusal_text(&m, w, &m.can_delete(w).unwrap_err()).contains("Geschossverwaltung"));
        // Aus: Ebene und Aufkantung weg; rückgängig bringt beides
        let aus = schritt(&mut m, |m| m.set_flat_roof(og_storey, false));
        assert!(m.roof_level(og_storey).is_none() && ak_runs(&m).is_empty());
        m.apply(&aus, Direction::Undo);
        assert_eq!(ak_runs(&m), ak);
        assert_eq!(m.roof_level(og_storey), Some(fd));
        m.apply(&aus, Direction::Redo);
        assert!(ak_runs(&m).is_empty());
        m.apply(&aus, Direction::Undo);
        // Rückgängig bis vor das Flachdach: die Datei ist wie vorher, nur
        // die Nummern der Aufkantung bleiben vergeben
        m.apply(&t, Direction::Undo);
        let nachher = crate::szo::write(&m);
        let soll = vorher.replacen("\n[building]", " next=AK:4\n[building]", 1);
        assert!(nachher == soll, "Datei nach Rückgängig anders");
    }

    /// D1: Datei mit Flachdach (`kind=roof`, `cat=parapet`) liest sich
    /// gleich zurück; ohne Flachdach steht keins der Wörter darin.
    #[test]
    fn datei_mit_flachdach() {
        let (mut m, og) = haus(EXTERIOR_TYPE_GUID);
        let og_storey = m.run(og).unwrap().storey;
        let ohne = crate::szo::write(&m);
        assert!(!ohne.contains("kind=roof") && !ohne.contains("cat=parapet"));
        schritt(&mut m, |m| m.set_flat_roof(og_storey, true));
        let text = crate::szo::write(&m);
        assert!(text.contains("kind=roof"), "{text}");
        assert!(text.contains("cat=parapet"), "{text}");
        let back = crate::szo::read(&text, GuidGen::with_seed(5)).unwrap();
        assert!(back.hints.is_empty(), "{:?}", back.hints);
        assert_eq!(crate::szo::write(&back.model), text);
        assert!(back.model.check().is_empty());
    }

    /// D2: Ein Gebäude, das nach dem Einschalten in der Vorlage entsteht,
    /// bekommt die Aufkantung gleich mit; im OG gezeichnet auch.
    #[test]
    fn vorlage_mit_flachdach() {
        let mut m = Model::with_seed(3);
        let eg = m.defaults().storey;
        assert!(m.set_flat_roof(eg, true));
        let b = m.add_building(2);
        let fd = m.roof_level_of(Some(b)).expect("FD gehört zum Gebäude");
        let levels = m.levels_in(Some(b));
        assert_eq!(levels.last(), Some(&fd));
        assert_eq!(levels.len(), 4);
        let og = levels[2];
        assert_eq!(m.storey(fd).unwrap().elevation, m.storey(og).unwrap().top());
        let pts = [
            vec3(0.0, 0.0, 0.0),
            vec3(0.0, 8000.0, 0.0),
            vec3(10000.0, 8000.0, 0.0),
            vec3(10000.0, 0.0, 0.0),
        ];
        let eg = m.build_from_polygon(b, &pts).unwrap();
        let og_run = m.runs_above(eg)[0];
        assert_eq!(m.runs_above(og_run), ak_runs(&m));
        assert!(m.check().is_empty(), "{:?}", m.check());
        // Auf der Ebene Flachdach lässt sich nichts zeichnen
        let set = m.defaults().exterior_wall;
        let off: Vec<Vec3> = pts.iter().map(|p| *p + vec3(20000.0, 0.0, 0.0)).collect();
        assert!(m
            .add_wall_run(&off, true, RefSide::Left, fd, set, Category::ExteriorWall)
            .is_none());
    }
}
