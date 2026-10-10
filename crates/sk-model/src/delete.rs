//! Löschen (Paket „Löschen“, Prüfregeln 24–27): Löschbar ist, was man selbst
//! gezeichnet hat (Innenwände, Wände offener Züge). Was Skizzeo ableitet
//! (Decke, Sohlplatte, Frostschürze, Randdämmstreifen, Außenwand des
//! Gebäudeumrisses), verschwindet nur mit seiner Quelle; ganze Gebäude
//! löscht ein eigener Befehl.

use super::*;

/// Warum ein Bauteil nicht gelöscht wird.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Refusal {
    /// Außenwand eines geschlossenen Zuges: gehört zum Umriss des Gebäudes.
    BuildingOutline(Option<BuildingId>),
    /// Decke, Sohlplatte, Frostschürze, Randdämmstreifen: folgen `from`
    /// (Wand des Zuges, Sohlplatte bzw. Wand des Streifens).
    Derived { from: ElementId },
    /// Das Bauteil gibt es nicht (mehr).
    Missing,
    /// Gesperrt (Paket 4 §2.2): erst im Baum entsperren.
    Locked(ElementId),
}

/// Ergebnis von [`Model::delete_elements`].
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Deleted {
    /// Gelöschte Bauteile, in der Reihenfolge der Auswahl.
    pub removed: Vec<ElementId>,
    /// Kategorie je gelöschtem Bauteil (wie `removed`; die Bauteile gibt es
    /// danach nicht mehr).
    pub kinds: Vec<Category>,
    /// Abgelehnte Bauteile mit Grund.
    pub refused: Vec<(ElementId, Refusal)>,
}

/// Grund und Ausweg für ein nicht löschbares Bauteil, je Satz eine Zeile
/// (Gestaltung „Löschen“, Fassung 1). Keine Nummern, keine Guids.
pub fn refusal_lines(m: &Model, id: ElementId, r: &Refusal) -> Vec<&'static str> {
    if matches!(r, Refusal::Locked(_)) {
        return vec!["Entsperren im Baum mit dem Schloss."];
    }
    if matches!(r, Refusal::BuildingOutline(_)) {
        return vec![
            "Außenwände gehören zum Gebäudeumriss.",
            "Zum Entfernen den Umriss ändern oder das ganze Gebäude löschen.",
        ];
    }
    match m.element(id).map(|e| e.category) {
        Some(Category::Floor) => {
            vec!["Die Decke ergibt sich aus dem Gebäudeumriss und bleibt, solange das Gebäude steht."]
        }
        Some(Category::GroundSlab | Category::StripFooting) => {
            vec!["Sohlplatte und Frostschürze folgen dem Umriss des Erdgeschosses."]
        }
        Some(Category::EdgeInsulation) => {
            vec!["Der Randdämmstreifen gehört zum Wandtyp; zum Entfernen den Wandtyp ändern."]
        }
        Some(Category::PerimeterInsulation) => {
            vec!["Die Perimeterdämmung gehört zur Sohlplatte; ihre Dicke stellst du bei der Sohlplatte ein, 0 schaltet sie ab."]
        }
        Some(Category::SoffitInsulation) => {
            vec!["Die Untersichtdämmung folgt dem Vorsprung des Geschosses darüber; ihre Dicke steht bei der Decke."]
        }
        Some(Category::RoofTerrace) => {
            vec!["Die Dachterrasse folgt dem Rücksprung des OG. Ihren Aufbau stellst du im Paneel ein."]
        }
        Some(Category::Coping)
            if m.element(id)
                .and_then(|e| m.storey(e.storey))
                .is_some_and(|s| s.kind == LevelKind::Roof) =>
        {
            vec!["Das Attikablech folgt dem Flachdach. Seinen Baustoff stellst du im Paneel ein."]
        }
        Some(Category::Roof) => {
            vec!["Das Flachdach folgt der Aufkantung. Seinen Aufbau stellst du im Paneel ein."]
        }
        Some(Category::Coping) => {
            vec![
                "Das Attikablech folgt der Dachterrasse. Seinen Baustoff stellst du im Paneel ein.",
            ]
        }
        Some(Category::Parapet) => {
            vec!["Die Aufkantung folgt dem Flachdach. Ihre Höhe stellst du in der Geschossverwaltung ein."]
        }
        _ => vec!["Dieses Bauteil lässt sich nicht löschen."],
    }
}

/// Die Sätze aus [`refusal_lines`] hintereinander (Hinweis am gedimmten
/// „Löschen“).
pub fn refusal_text(m: &Model, id: ElementId, r: &Refusal) -> String {
    let lines = refusal_lines(m, id, r).join(" ");
    match r {
        Refusal::Locked(x) => {
            let n = m.element(*x).map_or("Das Bauteil", |e| e.number.as_str());
            format!("{n} ist gesperrt. {lines}")
        }
        _ => lines,
    }
}

/// Stücke eines Zuges ohne die Segmente `drop`: zusammenhängende Folgen der
/// übrigen Segmente in Zugrichtung. Ein geschlossener Zug beginnt hinter
/// dem ersten gelöschten Segment (er wird offen, nie zerteilt am Anfang).
fn pieces(n: usize, closed: bool, drop: &[usize]) -> Vec<Vec<usize>> {
    let start = if closed {
        drop.iter().map(|d| (d + 1) % n).min().unwrap_or(0)
    } else {
        0
    };
    let mut out: Vec<Vec<usize>> = Vec::new();
    let mut cur = Vec::new();
    for k in 0..n {
        let s = (start + k) % n;
        if drop.contains(&s) {
            if !cur.is_empty() {
                out.push(std::mem::take(&mut cur));
            }
        } else {
            cur.push(s);
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

impl Model {
    /// Lässt sich das Bauteil löschen? Ohne Nebenwirkung (für ein gedimmtes
    /// „Löschen“ vor dem Drücken).
    pub fn can_delete(&self, id: ElementId) -> Result<(), Refusal> {
        let Some(e) = self.element(id) else {
            return Err(Refusal::Missing);
        };
        let first_wall = |run: RunId| self.wall_at(run, 0).unwrap_or(id);
        match e.kind {
            ElementKind::Wall(w) => {
                let closed = self.run(w.run).is_some_and(|r| r.closed);
                if closed && e.category == Category::ExteriorWall {
                    Err(Refusal::BuildingOutline(self.building_of_element(id)))
                } else if e.category == Category::Parapet {
                    // Folgt dem Flachdach (Jörn 10.10.): nur mit der Ebene weg
                    Err(Refusal::Derived {
                        from: w.coupling.map_or(id, |c| c.below),
                    })
                } else if e.locked {
                    Err(Refusal::Locked(id))
                } else {
                    Ok(())
                }
            }
            ElementKind::Floor(f) => Err(Refusal::Derived {
                from: first_wall(f.run),
            }),
            ElementKind::GroundSlab(s) => Err(Refusal::Derived {
                from: first_wall(s.run),
            }),
            ElementKind::StripFooting(f) => Err(Refusal::Derived { from: f.slab }),
            ElementKind::PerimeterInsulation { slab } => Err(Refusal::Derived { from: slab }),
            ElementKind::EdgeStrip { wall, .. } => Err(Refusal::Derived { from: wall }),
            ElementKind::SoffitInsulation { floor }
            | ElementKind::RoofTerrace { floor }
            | ElementKind::Coping { floor }
            | ElementKind::Roof { floor } => Err(Refusal::Derived { from: floor }),
            ElementKind::Ext(_) if e.locked => Err(Refusal::Locked(id)),
            ElementKind::Ext(_) => Ok(()),
        }
    }

    /// Löscht, was von `ids` löschbar ist, im offenen Schritt; der Rest
    /// bleibt und steht mit Grund in [`Deleted::refused`]. Ein Zug wird um
    /// gelöschte Endsegmente kürzer, an gelöschten Mittelsegmenten geteilt
    /// (der erste Teil behält Kennung und Guid des Zuges) und verschwindet
    /// mit seinem letzten Segment. Wände behalten Kennung, Guid und Nummer.
    pub fn delete_elements(&mut self, ids: &[ElementId]) -> Deleted {
        let mut out = Deleted::default();
        let mut by_run: Vec<(RunId, Vec<usize>)> = Vec::new();
        for &id in ids {
            if out.removed.contains(&id) || out.refused.iter().any(|r| r.0 == id) {
                continue;
            }
            match self.can_delete(id) {
                // Erweiterungsbauteil (E3): steht für sich
                Ok(())
                    if matches!(self.element(id).map(|e| &e.kind), Some(ElementKind::Ext(_))) =>
                {
                    note!(self, Element, self.elements, id);
                    self.elements.remove(id);
                    self.touch();
                    out.removed.push(id);
                    out.kinds.push(Category::Extension);
                }
                Ok(()) => {
                    let (Some((run, seg)), Some(cat)) =
                        (self.segment_of(id), self.element(id).map(|e| e.category))
                    else {
                        continue;
                    };
                    match by_run.iter_mut().find(|(r, _)| *r == run) {
                        Some((_, segs)) => segs.push(seg),
                        None => by_run.push((run, vec![seg])),
                    }
                    out.removed.push(id);
                    out.kinds.push(cat);
                }
                Err(Refusal::Missing) => {}
                Err(r) => out.refused.push((id, r)),
            }
        }
        for (run, segs) in by_run {
            self.remove_segments(run, &segs);
        }
        out
    }

    /// Entfernt die Segmente `drop` eines Zuges (siehe
    /// [`Model::delete_elements`]).
    fn remove_segments(&mut self, id: RunId, drop: &[usize]) {
        let Some(r) = self.run(id).cloned() else {
            return;
        };
        let n = r.segments.len();
        let parts = pieces(n, r.closed, drop);
        if parts.is_empty() {
            self.remove_run(id);
            return;
        }
        let np = r.points.len();
        let points_of = |segs: &[usize]| -> Vec<Vec3> {
            let mut p: Vec<Vec3> = segs.iter().map(|&k| r.points[k]).collect();
            if let Some(&last) = segs.last() {
                p.push(r.points[(last + 1) % np]);
            }
            p
        };
        for &k in drop {
            if let Some(&e) = r.segments.get(k) {
                note!(self, Element, self.elements, e);
                self.elements.remove(e);
            }
        }
        let mut runs = vec![id];
        for (i, segs) in parts.iter().enumerate() {
            let run = if i == 0 {
                note!(self, Run, self.runs, id);
                if let Some(x) = self.runs.get_mut(id) {
                    x.points = points_of(segs);
                    x.closed = false;
                    x.segments = segs.iter().map(|&k| r.segments[k]).collect();
                }
                id
            } else {
                let guid = self.new_guid();
                let new = self.runs.insert(WallRun {
                    guid,
                    points: points_of(segs),
                    closed: false,
                    segments: segs.iter().map(|&k| r.segments[k]).collect(),
                    ..r.clone()
                });
                note!(self, Run, new new);
                runs.push(new);
                new
            };
            for (k, &s) in segs.iter().enumerate() {
                let e = r.segments[s];
                // Nur Zug und Segment ändern sich; die Kopplung bleibt
                let moved = !matches!(
                    self.element(e).map(|x| &x.kind),
                    Some(ElementKind::Wall(w)) if w.run == run && w.seg == k as u32
                );
                if moved {
                    note!(self, Element, self.elements, e);
                    if let Some(ElementKind::Wall(w)) =
                        self.elements.get_mut(e).map(|x| &mut x.kind)
                    {
                        w.run = run;
                        w.seg = k as u32;
                    }
                }
            }
        }
        for &x in &runs {
            self.sync_parts(x);
        }
        self.update_joins(&runs);
        self.touch();
    }

    /// Bauteile eines Gebäudes: alles in seinen Geschossen.
    pub fn building_parts(&self, b: BuildingId) -> Vec<ElementId> {
        let levels = self.levels_in(Some(b));
        self.elements
            .iter()
            .filter(|(_, e)| levels.contains(&e.storey))
            .map(|(id, _)| id)
            .collect()
    }

    /// Zahl der Bauteile, die mit dem Gebäude verschwinden (Rückfrage).
    pub fn building_part_count(&self, b: BuildingId) -> usize {
        self.building_parts(b).len()
    }

    /// Löscht ein Gebäude im offenen Schritt: alle Wandzüge seiner Geschosse
    /// mit Decken, Gründung und Streifen, den Datensatz und seine Geschosse.
    /// Ist es das letzte Gebäude, bleiben die Geschosse als Vorlage mit
    /// ihren Höhen. Typen, Baustoffe und Attribute bleiben. Die Nummer wird
    /// nicht neu vergeben.
    pub fn remove_building(&mut self, b: BuildingId) -> bool {
        if !self.buildings.contains(b) {
            return false;
        }
        let levels = self.levels_in(Some(b));
        let runs: Vec<RunId> = self
            .runs
            .iter()
            .filter(|(_, r)| levels.contains(&r.storey))
            .map(|(id, _)| id)
            .collect();
        for r in runs {
            if self.runs.contains(r) {
                self.remove_run(r);
            }
        }
        for e in self.building_parts(b) {
            note!(self, Element, self.elements, e);
            self.elements.remove(e);
        }
        let last = self.buildings.len() == 1;
        for &st in &levels {
            note!(self, Storey, self.storeys, st);
            if last {
                if let Some(s) = self.storeys.get_mut(st) {
                    s.building = None;
                }
            } else {
                self.storeys.remove(st);
            }
        }
        note!(self, Building, self.buildings, b);
        self.buildings.remove(b);
        if !self.storeys.contains(self.defaults.storey) {
            let eg = self
                .buildings
                .iter()
                .min_by(|a, b| a.1.number.cmp(&b.1.number))
                .and_then(|(id, _)| self.ground_of(Some(id)));
            if let Some(eg) = eg {
                let d = Defaults {
                    storey: eg,
                    ..self.defaults
                };
                if let Some(t) = self.txn.as_mut() {
                    if t.noted.insert(Key::Defaults) {
                        t.changes.push(Change::Defaults {
                            old: self.defaults,
                            new: d,
                        });
                    }
                }
                self.defaults = d;
            }
        }
        self.joins = self.detect_all();
        self.touch();
        true
    }

    /// Nummernzähler, die über der höchsten vorhandenen Nummer stehen (es
    /// wurde gelöscht): Präfix und Zähler für die Datei, z. B. ("IW", 1),
    /// Gebäude als „GB“. Ohne Löschen leer.
    pub(crate) fn number_gaps(&self) -> Vec<(String, u32)> {
        let highest = self.highest_numbers();
        let mut out: Vec<(String, u32)> = Category::ALL
            .iter()
            .filter(|c| self.numbers[c.index()] > highest[c.index()])
            .map(|c| (c.prefix().to_string(), self.numbers[c.index()]))
            .collect();
        // Erweiterungen je Präfix (E3)
        out.extend(self.ext_number_gaps());
        let gb = self
            .buildings
            .iter()
            .filter_map(|(_, b)| building_index(&b.number))
            .max()
            .unwrap_or(0);
        if self.building_number > gb {
            out.push(("GB".to_string(), self.building_number));
        }
        out
    }

    /// Hebt einen Nummernzähler aus der Datei an (nie unter die höchste
    /// vorhandene Nummer).
    pub(crate) fn raise_counter(&mut self, prefix: &str, n: u32) -> bool {
        if prefix == "GB" {
            self.building_number = self.building_number.max(n);
            return true;
        }
        match Category::ALL.iter().find(|c| c.prefix() == prefix) {
            Some(c) => {
                let k = &mut self.numbers[c.index()];
                *k = (*k).max(n);
                true
            }
            None => self.raise_ext_counter(prefix, n),
        }
    }

    /// Höchste vorhandene laufende Nummer je Kategorie.
    fn highest_numbers(&self) -> [u32; Category::ALL.len()] {
        let mut out = [0; Category::ALL.len()];
        for (_, e) in self.elements.iter() {
            // Erweiterungen zählen je Präfix ([`Model::ext_number_gaps`])
            if e.category == Category::Extension {
                continue;
            }
            let n = e
                .number
                .strip_prefix(e.category.prefix())
                .and_then(|r| r.strip_prefix('-'))
                .and_then(|r| r.parse::<u32>().ok());
            if let Some(n) = n {
                let c = &mut out[e.category.index()];
                *c = (*c).max(n);
            }
        }
        out
    }

    /// Prüfregeln 24–27 (Löschen): keine toten Verweise, Zähler nie unter
    /// der höchsten Nummer, keine leeren Züge.
    pub(crate) fn check_delete(&self) -> Vec<String> {
        let mut out = Vec::new();
        let highest = self.highest_numbers();
        for c in Category::ALL {
            if self.numbers[c.index()] < highest[c.index()] {
                out.push(format!(
                    "Nummernzähler {} unter der höchsten Nummer",
                    c.prefix()
                ));
            }
        }
        if self
            .buildings
            .iter()
            .filter_map(|(_, b)| building_index(&b.number))
            .any(|n| n > self.building_number)
        {
            out.push("Nummernzähler GB unter der höchsten Gebäudenummer".into());
        }
        for (id, r) in self.runs.iter() {
            if r.segments.is_empty() {
                out.push(format!("Wandzug {id:?}: ohne Segment"));
            }
        }
        for j in &self.joins {
            if !self.runs.contains(j.a_run) || !self.runs.contains(j.b_run) {
                out.push(format!("Anschluss {:?}: Wandzug fehlt", j.a));
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::pieces;

    #[test]
    fn stuecke_eines_zuges() {
        assert_eq!(pieces(3, false, &[1]), vec![vec![0], vec![2]]);
        assert_eq!(pieces(3, false, &[2]), vec![vec![0, 1]]);
        assert_eq!(pieces(3, false, &[0, 2]), vec![vec![1]]);
        assert!(pieces(2, false, &[0, 1]).is_empty());
        // geschlossen: beginnt hinter dem gelöschten Segment, wird offen
        assert_eq!(pieces(4, true, &[1]), vec![vec![2, 3, 0]]);
        assert_eq!(pieces(4, true, &[0, 2]), vec![vec![1], vec![3]]);
    }
}
