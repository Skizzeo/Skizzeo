//! Raum als gespeichertes Bauteil (Grundriss G3, Jörn 11.10. 16:53, J10;
//! schnittstelle-raum.md §2). Gespeichert sind nur Anker, Name, Nutzung,
//! Faktor und eine verschobene Stempellage; die Form rechnet `sk-raum` aus
//! den Wänden, nie gespeichert. Methoden von `Model`, aus `model.rs`
//! ausgelagert.
//!
//! Ein Raum ruht, wenn keine Fläche seinen Anker umschließt oder ein
//! älterer Raum dieselbe Fläche hat (Räume vereint, Wand weg): Er bleibt mit
//! seinen Daten unsichtbar im Modell und lebt wieder auf, wenn seine Fläche
//! zurückkommt. Das rechnet `sk-raum`, gespeichert ist es nicht.

use super::*;
use std::collections::VecDeque;

/// Gespeicherter Raum ([`ElementKind::Space`]).
#[derive(Clone, Debug, PartialEq)]
pub struct Space {
    /// Punkt im Raum (Grundrissebene, z = 0). Ordnet den gespeicherten Raum
    /// seiner Fläche zu.
    pub anker: Vec3,
    /// Lage des Raumstempels, nur wenn der Nutzer ihn verschoben hat.
    pub stempel: Option<Vec3>,
    /// „Raum 1“, bis der Nutzer ihn benennt.
    pub name: String,
    /// Raumvorlage im Raumkatalog des Raumbuchs (`use=`).
    pub nutzung: Option<Guid>,
    /// Abweichender Faktor für die Wohnfläche (0 … 1), selten.
    pub faktor: Option<f64>,
}

impl Space {
    pub fn neu(anker: Vec3, name: String) -> Space {
        Space {
            anker: vec3(anker.x, anker.y, 0.0),
            stempel: None,
            name,
            nutzung: None,
            faktor: None,
        }
    }
}

/// Bauabschnitt der Räume (ohne Ausführungsfolge).
pub const SPACE_SEQ: u16 = 0;

/// So viele Schritte an Räumen merkt sich [`RaumLog`].
const RAUM_LOG: usize = 64;

/// Stand der Räume und welche Räume die letzten Schritte geändert haben
/// (Review H-137b): Damit sammelt `sk-raum` die Räume eines Geschosses nach
/// einem Schritt nicht über alle Bauteile neu, sondern nimmt nur die
/// geänderten heraus und wieder hinein.
#[derive(Clone, Debug, Default)]
pub(crate) struct RaumLog {
    revision: u64,
    /// Ab diesem Stand sind alle Änderungen in `schritte`.
    ab: u64,
    /// Je Schritt an Räumen: Stand danach, die Räume, die er anlegt,
    /// ändert, löscht oder in ein anderes Geschoss hängt.
    schritte: VecDeque<(u64, Box<[ElementId]>)>,
}

impl RaumLog {
    /// Änderung ohne Schritt: Was sie an Räumen ändert, ist nicht bekannt.
    pub(crate) fn unbekannt(&mut self) {
        self.revision += 1;
        self.ab = self.revision;
        self.schritte.clear();
    }

    /// Abgeschlossener oder verworfener Schritt, Rückgängig oder
    /// Wiederholen mit den Änderungen `changes`.
    pub(crate) fn schritt(&mut self, changes: &[Change]) {
        let ids: Box<[ElementId]> = changes
            .iter()
            .filter_map(|c| match c {
                Change::Element { id, old, new }
                    if old
                        .iter()
                        .chain(new)
                        .any(|e| matches!(e.kind, ElementKind::Space(_))) =>
                {
                    Some(*id)
                }
                _ => None,
            })
            .collect();
        if ids.is_empty() {
            return;
        }
        self.revision += 1;
        self.schritte.push_back((self.revision, ids));
        if self.schritte.len() > RAUM_LOG {
            if let Some((r, _)) = self.schritte.pop_front() {
                self.ab = r;
            }
        }
    }
}

impl Model {
    /// Der Raum hinter einem Bauteil.
    pub fn space(&self, id: ElementId) -> Option<&Space> {
        match &self.element(id)?.kind {
            ElementKind::Space(s) => Some(&**s),
            _ => None,
        }
    }

    /// Räume eines Geschosses (auch ruhende), nach Nummer (R-010 vor R-1000).
    pub fn spaces(&self, storey: StoreyId) -> Vec<ElementId> {
        let mut v: Vec<(&str, ElementId)> = self
            .elements
            .iter()
            .filter(|(_, e)| e.storey == storey && matches!(e.kind, ElementKind::Space(_)))
            .map(|(id, e)| (e.number.as_str(), id))
            .collect();
        v.sort_by(|a, b| a.0.len().cmp(&b.0.len()).then(a.0.cmp(b.0)));
        v.into_iter().map(|(_, id)| id).collect()
    }

    /// Legt einen Raum an (im offenen Schritt; Nummer R-…).
    pub fn add_space(&mut self, storey: StoreyId, s: Space) -> ElementId {
        let id = self.new_element(
            Category::Space,
            storey,
            SPACE_SEQ,
            ElementKind::Space(Box::new(s)),
        );
        self.touch();
        id
    }

    /// Ändert einen Raum; gleicher Inhalt ändert nichts. `false`, wenn `id`
    /// kein Raum ist.
    pub fn set_space(&mut self, id: ElementId, f: impl FnOnce(&mut Space)) -> bool {
        let Some(mut s) = self.space(id).cloned() else {
            return false;
        };
        f(&mut s);
        s.anker = vec3(s.anker.x, s.anker.y, 0.0);
        if self.space(id) == Some(&s) {
            return true;
        }
        note!(self, Element, self.elements, id);
        if let Some(e) = self.elements.get_mut(id) {
            e.kind = ElementKind::Space(Box::new(s));
        }
        self.touch();
        true
    }

    /// Stand der Räume (Review H-137b): steigt, wenn ein abgeschlossener
    /// oder verworfener Schritt, Rückgängig oder Wiederholen einen Raum
    /// anlegt, ändert, löscht oder in ein anderes Geschoss hängt, und
    /// vorsichtshalber bei jeder Änderung ohne Schritt. Ein Schritt nur an
    /// Wänden lässt ihn stehen, auch beim Rückgängig. Im offenen Schritt
    /// bleibt er; welche Geschosse der Schritt an Räumen ändert, sagt
    /// [`Model::raum_geschosse_im_schritt`]. Anders als [`Model::revision`]
    /// geht er nie zurück. Er gehört zu diesem Modell und beginnt mit ihm
    /// bei 0, nach dem Laden also neu; wer ihn sich merkt, merkt sich das
    /// Modell mit (die Erkennung der Szene, Review H-175c).
    pub fn raum_revision(&self) -> u64 {
        self.raum_log.revision
    }

    /// Die Räume, die sich seit dem Stand `stand` ([`Model::raum_revision`])
    /// geändert haben, auch gelöschte und solche in einem anderen Geschoss
    /// (eine Kennung kann mehrmals vorkommen); `None`, wenn das nicht
    /// bekannt ist: nach einer Änderung ohne Schritt, wenn `stand` mehr als
    /// 64 Schritte an Räumen zurückliegt oder noch gar nicht erreicht ist.
    pub fn raeume_geaendert_seit(&self, stand: u64) -> Option<Vec<ElementId>> {
        let l = &self.raum_log;
        if stand < l.ab || stand > l.revision {
            return None;
        }
        Some(
            l.schritte
                .iter()
                .filter(|(r, _)| *r > stand)
                .flat_map(|(_, ids)| ids.iter().copied())
                .collect(),
        )
    }

    /// Geschosse, deren Wandzüge oder Wände der offene Schritt ändert
    /// (vorher und nachher): dort gleicht `sk-raum` die Räume ab. Leer ohne
    /// offenen Schritt.
    pub fn wand_geschosse_im_schritt(&self) -> Vec<StoreyId> {
        let mut v: Vec<StoreyId> = Vec::new();
        let Some(open) = &self.txn else {
            return v;
        };
        let mut neu = |g: StoreyId| {
            if !v.contains(&g) {
                v.push(g);
            }
        };
        for c in &open.changes {
            match c {
                Change::Run { id, old, .. } => old
                    .iter()
                    .chain(self.runs.get(*id))
                    .for_each(|r| neu(r.storey)),
                Change::Element { id, old, .. } => old
                    .iter()
                    .chain(self.elements.get(*id))
                    .filter(|e| matches!(e.kind, ElementKind::Wall(_)))
                    .for_each(|e| neu(e.storey)),
                _ => {}
            }
        }
        v
    }

    /// Geschosse, deren Räume der offene Schritt ändert (anlegen, Anker,
    /// Stempel, Name): Die Szene rechnet dort den Stand der Räume neu. Leer
    /// ohne offenen Schritt.
    pub fn raum_geschosse_im_schritt(&self) -> Vec<StoreyId> {
        let mut v: Vec<StoreyId> = Vec::new();
        let Some(open) = &self.txn else {
            return v;
        };
        for c in &open.changes {
            if let Change::Element { id, old, .. } = c {
                for e in old.iter().chain(self.elements.get(*id)) {
                    if matches!(e.kind, ElementKind::Space(_)) && !v.contains(&e.storey) {
                        v.push(e.storey);
                    }
                }
            }
        }
        v
    }

    /// Kleinster freie Name „Raum n“ im Projekt (auch ruhende zählen).
    pub fn naechster_raumname(&self) -> String {
        self.freie_raumnamen()
            .next()
            .unwrap_or_else(|| "Raum 1".into())
    }

    /// Die freien Namen „Raum n“ im Projekt, kleinste zuerst (einmal über
    /// alle Bauteile, Review #87 B3).
    pub fn freie_raumnamen(&self) -> FreieRaumnamen {
        let mut belegt: Vec<u32> = self
            .elements
            .iter()
            .filter_map(|(_, e)| match &e.kind {
                ElementKind::Space(s) => s.name.strip_prefix("Raum ")?.parse().ok(),
                _ => None,
            })
            .collect();
        belegt.sort_unstable();
        FreieRaumnamen { belegt, i: 0, n: 0 }
    }
}

/// Freie Namen „Raum n“ ([`Model::freie_raumnamen`]).
pub struct FreieRaumnamen {
    /// Belegte Nummern, aufsteigend.
    belegt: Vec<u32>,
    /// Erste belegte Nummer, die nicht unter `n` liegt.
    i: usize,
    n: u32,
}

impl Iterator for FreieRaumnamen {
    type Item = String;

    fn next(&mut self) -> Option<String> {
        loop {
            self.n = self.n.checked_add(1)?;
            while self.belegt.get(self.i).is_some_and(|&b| b < self.n) {
                self.i += 1;
            }
            if self.belegt.get(self.i) != Some(&self.n) {
                return Some(format!("Raum {}", self.n));
            }
        }
    }
}

/// `[space]`-Zeile der Datei, die nicht zählt (Regel 72): roh mit ihren
/// `[prop]`-Zeilen, dem Geschoss, auf das sie zeigt, dem Anker, wenn er
/// lesbar ist, und ihrer Nummer, die belegt bleibt (BIM-Datenmodell zu
/// #87).
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct RawSpace {
    pub line: String,
    pub props: Vec<String>,
    pub guid: Option<Guid>,
    pub storey: Guid,
    pub anker: Option<Vec3>,
    pub number: Option<String>,
}

impl Model {
    /// Die rohen `[space]`-Zeilen, die beim Speichern bleiben: ohne die,
    /// deren Geschoss inzwischen gelöscht ist.
    pub(crate) fn raw_spaces(&self) -> impl Iterator<Item = &RawSpace> {
        self.space_raw
            .iter()
            .filter(|r| self.storeys().iter().any(|(_, s)| s.guid == r.storey))
    }

    /// Lesbare Anker der rohen `[space]`-Zeilen im Geschoss: Eine Fläche,
    /// in der einer liegt, bekommt beim Abgleich keinen neuen Raum.
    pub fn raw_space_anchors(&self, storey: StoreyId) -> Vec<Vec3> {
        let Some(g) = self.storey(storey).map(|s| s.guid) else {
            return Vec::new();
        };
        self.space_raw
            .iter()
            .filter(|r| r.storey == g)
            .filter_map(|r| r.anker)
            .collect()
    }

    pub(crate) fn load_space_raw(&mut self, raw: Vec<RawSpace>) {
        self.space_raw = raw;
    }
}

/// Befunde zu einem Raum für [`Model::check`].
pub(super) fn pruefe(nummer: &str, s: &Space) -> Vec<String> {
    let mut out = Vec::new();
    if !(s.anker.x.is_finite() && s.anker.y.is_finite() && s.anker.z == 0.0) {
        out.push(format!("{nummer}: Anker ungültig"));
    }
    if s.stempel
        .is_some_and(|p| !(p.x.is_finite() && p.y.is_finite()))
    {
        out.push(format!("{nummer}: Stempellage ungültig"));
    }
    if s.name.trim().is_empty() {
        out.push(format!("{nummer}: ohne Namen"));
    }
    if s.faktor.is_some_and(|f| !(0.0..=1.0).contains(&f)) {
        out.push(format!("{nummer}: Faktor außerhalb 0 … 1"));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn raum_anlegen_aendern_benennen() {
        let mut m = Model::new();
        let st = m.defaults().storey;
        m.begin("Raum");
        assert_eq!(m.naechster_raumname(), "Raum 1");
        let a = m.add_space(
            st,
            Space::neu(vec3(1000.0, 1000.0, 5.0), m.naechster_raumname()),
        );
        assert_eq!(m.element(a).unwrap().number, "R-001");
        assert_eq!(m.element(a).unwrap().category, Category::Space);
        assert_eq!(m.space(a).unwrap().anker.z, 0.0);
        assert_eq!(m.naechster_raumname(), "Raum 2");
        let b = m.add_space(st, Space::neu(vec3(3000.0, 1000.0, 0.0), "Raum 2".into()));
        assert!(m.set_space(a, |s| s.name = "Küche".into()));
        assert_eq!(m.naechster_raumname(), "Raum 1", "frei geworden");
        assert_eq!(m.spaces(st), vec![a, b]);
        let iw = m.defaults().interior_wall;
        let pts = [vec3(0.0, 0.0, 0.0), vec3(2000.0, 0.0, 0.0)];
        let r = m
            .add_wall_run(&pts, false, RefSide::Center, st, iw, Category::InteriorWall)
            .unwrap();
        let wand = m.wall_at(r, 0).unwrap();
        assert!(!m.set_space(wand, |_| {}), "keine Wand als Raum");
        assert!(m.commit().is_some());
    }

    #[test]
    fn freie_raumnamen_ueberspringen_belegte() {
        let mut m = Model::new();
        let st = m.defaults().storey;
        m.begin("Räume");
        for n in ["Raum 3", "Raum 1", "Raum 3", "Raum 0", "Küche", "Raum 6"] {
            m.add_space(st, Space::neu(vec3(0.0, 0.0, 0.0), n.into()));
        }
        let frei: Vec<String> = m.freie_raumnamen().take(4).collect();
        assert_eq!(frei, ["Raum 2", "Raum 4", "Raum 5", "Raum 7"]);
        m.commit();
    }

    /// Review H-137b: Der Stand der Räume steigt nur, wenn ein Schritt,
    /// Verwerfen, Rückgängig oder Wiederholen Räume ändert, und bei jeder
    /// Änderung ohne Schritt; ein Wandschritt lässt ihn stehen.
    #[test]
    fn raum_revision_nur_bei_raeumen() {
        let mut m = Model::new();
        let st = m.defaults().storey;
        let iw = m.defaults().interior_wall;
        let pts = [vec3(0.0, 0.0, 0.0), vec3(2000.0, 0.0, 0.0)];
        // Ohne Schritt: vorsichtshalber neu
        let r0 = m.raum_revision();
        let r = m
            .add_wall_run(&pts, false, RefSide::Center, st, iw, Category::InteriorWall)
            .unwrap();
        assert!(m.raum_revision() > r0);
        assert_eq!(m.raeume_geaendert_seit(r0), None, "ohne Schritt unbekannt");
        m.begin("Raum");
        let a = m.add_space(st, Space::neu(vec3(500.0, 500.0, 0.0), "Raum 1".into()));
        let r1 = m.raum_revision();
        let t1 = m.commit().unwrap();
        assert_eq!(m.raum_revision(), r1 + 1, "Raum angelegt");
        assert_eq!(m.raeume_geaendert_seit(r1), Some(vec![a]));
        assert_eq!(m.raeume_geaendert_seit(r1 + 1), Some(vec![]));
        assert_eq!(
            m.raeume_geaendert_seit(r1 + 2),
            None,
            "Stand aus der Zukunft"
        );
        // Wand verschieben: Revision steigt, Stand der Räume nicht
        let rev = m.revision();
        m.begin("Wand");
        let weiter = [vec3(0.0, 100.0, 0.0), vec3(2000.0, 100.0, 0.0)];
        assert!(m.set_run_points(r, &weiter).is_some());
        let t2 = m.commit().unwrap();
        assert!(m.revision() > rev);
        assert_eq!(m.raum_revision(), r1 + 1, "Wandschritt");
        m.apply(&t2, Direction::Undo);
        assert_eq!(m.raum_revision(), r1 + 1, "Wand zurück");
        // Raum ändern, im Schritt bleibt der Stand; Verwerfen, Rückgängig
        // und Wiederholen eines Raums zählen
        m.begin("Name");
        assert!(m.set_space(a, |s| s.name = "Küche".into()));
        assert_eq!(m.raum_revision(), r1 + 1, "im offenen Schritt");
        m.rollback();
        let r2 = m.raum_revision();
        assert!(r2 > r1 + 1, "verworfen");
        m.apply(&t1, Direction::Undo);
        assert!(m.space(a).is_none());
        assert!(m.raum_revision() > r2, "Raum zurück");
        let r3 = m.raum_revision();
        m.apply(&t1, Direction::Redo);
        assert!(m.raum_revision() > r3, "Raum wieder da");
        let seit = m.raeume_geaendert_seit(r1).unwrap();
        assert!(seit.len() >= 4 && seit.iter().all(|&x| x == a));
        // Gemerkt werden die letzten 64 Schritte an Räumen
        let r4 = m.raum_revision();
        for i in 0..64 {
            m.begin("Name");
            assert!(m.set_space(a, |s| s.name = format!("Raum {i}")));
            m.commit();
        }
        assert_eq!(m.raeume_geaendert_seit(r4).map(|v| v.len()), Some(64));
        assert_eq!(m.raeume_geaendert_seit(r4 - 1), None, "zu alt");
    }
}
