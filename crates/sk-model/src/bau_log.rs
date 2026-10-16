//! Bau-Log (Review M2, Architektur 16.10.): welche Gebäude die letzten
//! Schritte geändert haben. Damit behält die Szene die Mengen der übrigen
//! Gebäude ([`crate::qto::Teil`]). Flüchtig: weder in der Datei noch im
//! Verlauf. Rückgängig und Wiederholen melden ihre Gebäude wie ein
//! abgeschlossener Schritt.
//!
//! Lieber zu oft alles als einmal falsch: Ein Schritt nennt nur Gebäude,
//! wenn er allein Wandzüge und Bauteile ändert (über ihr altes und neues
//! Geschoss); alles andere (Aufbauten, Baustoffe, Geschosse, Gebäude,
//! Gewerke, Raumvorlagen, Vorgaben, Gelände, Erweiterungen, Darstellung)
//! heißt alle. Jede Änderung ohne Schritt (Laden, Zurücksetzen, Import)
//! ist unbekannt, also ebenfalls alle.

use super::*;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};

/// So viele Schritte merkt sich das Log.
const BAU_LOG: usize = 64;

/// Kennung je Modell: Ein neues, geladenes oder kopiertes Modell hat eine
/// eigene, sein Stand sagt über ein anderes nichts.
static KENNUNG: AtomicU64 = AtomicU64::new(1);

/// Stand des Bau-Logs eines Modells, für [`Model::gebaeude_seit`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BauStand {
    kennung: u64,
    stand: u64,
}

/// Gebäude eines Schritts; `None` in der Liste: die Geschosse ohne
/// Gebäude; `None` statt der Liste: alle.
type Gebaeude = Option<Box<[Option<BuildingId>]>>;

#[derive(Debug)]
pub(crate) struct BauLog {
    kennung: u64,
    stand: u64,
    /// Ab diesem Stand sind alle Änderungen in `schritte`.
    ab: u64,
    /// Je Schritt der Stand danach und seine Gebäude.
    schritte: VecDeque<(u64, Gebaeude)>,
}

impl Default for BauLog {
    fn default() -> Self {
        BauLog::mit_kennung(KENNUNG.fetch_add(1, Ordering::Relaxed))
    }
}

/// Eine Kopie des Modells fängt mit eigener Kennung an.
impl Clone for BauLog {
    fn clone(&self) -> Self {
        BauLog::default()
    }
}

impl BauLog {
    fn mit_kennung(kennung: u64) -> BauLog {
        BauLog {
            kennung,
            stand: 0,
            ab: 0,
            schritte: VecDeque::new(),
        }
    }

    /// Platzhalter, solange `apply` das Log beiseitelegt: ohne neue
    /// Kennung, nie gefragt.
    pub(crate) fn platzhalter() -> BauLog {
        BauLog::mit_kennung(0)
    }

    /// Änderung ohne Schritt: Was sie ändert, ist nicht bekannt.
    pub(crate) fn unbekannt(&mut self) {
        self.stand += 1;
        self.ab = self.stand;
        self.schritte.clear();
    }

    /// Abgeschlossener oder verworfener Schritt, Rückgängig oder
    /// Wiederholen mit den Gebäuden aus [`gebaeude`].
    pub(crate) fn schritt(&mut self, g: Gebaeude) {
        if g.as_deref().is_some_and(|g| g.is_empty()) {
            return;
        }
        self.stand += 1;
        self.schritte.push_back((self.stand, g));
        if self.schritte.len() > BAU_LOG {
            if let Some((r, _)) = self.schritte.pop_front() {
                self.ab = r;
            }
        }
    }
}

/// Gebäude, deren Mengen die Änderungen `changes` ändern können, im
/// Zustand nach ihnen; `None`: alle.
pub(crate) fn gebaeude(m: &Model, changes: &[Change]) -> Gebaeude {
    let mut out: Vec<Option<BuildingId>> = Vec::new();
    for c in changes {
        let geschosse: Vec<StoreyId> = match c {
            Change::Run { old, new, .. } => old.iter().chain(new).map(|r| r.storey).collect(),
            Change::Element { old, new, .. } => old.iter().chain(new).map(|e| e.storey).collect(),
            _ => return None,
        };
        for s in geschosse {
            // Ein Geschoss, das es nicht mehr gibt, ging mit einem Schritt
            // an den Geschossen; sicher ist sicher
            let st = m.storey(s)?;
            // Geschosse eines Gebäudes, das es nicht gibt, stehen bei denen
            // ohne Gebäude
            out.push(st.building.filter(|b| m.building(*b).is_some()));
        }
    }
    out.sort_by_key(|b| b.map(|b| b.index()));
    out.dedup();
    Some(out.into())
}

impl Model {
    /// Stand des Bau-Logs, für [`Model::gebaeude_seit`].
    pub fn bau_stand(&self) -> BauStand {
        BauStand {
            kennung: self.bau_log.kennung,
            stand: self.bau_log.stand,
        }
    }

    /// Gebäude, deren Mengen sich seit `s` geändert haben können (`None`
    /// in der Liste: die Geschosse ohne Gebäude), aufsteigend; `None`: nicht
    /// bekannt (anderes Modell, Änderung ohne Schritt, zu lange her oder
    /// ein Schritt für alle).
    pub fn gebaeude_seit(&self, s: BauStand) -> Option<Vec<Option<BuildingId>>> {
        let l = &self.bau_log;
        if s.kennung != l.kennung || s.stand < l.ab || s.stand > l.stand {
            return None;
        }
        let mut out: Vec<Option<BuildingId>> = Vec::new();
        for (_, g) in l.schritte.iter().filter(|(r, _)| *r > s.stand) {
            out.extend(g.as_deref()?.iter().copied());
        }
        out.sort_by_key(|b| b.map(|b| b.index()));
        out.dedup();
        Some(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Zwei Gebäude mit je einem Raum im Erdgeschoss.
    fn zwei() -> (Model, [BuildingId; 2], ElementId) {
        let mut m = Model::new();
        let a = m.add_building(1);
        let b = m.add_building(1);
        let mut raeume = Vec::new();
        for (i, g) in [a, b].into_iter().enumerate() {
            let st = m.ground_of(Some(g)).unwrap();
            m.begin("Raum");
            let p = vec3(1000.0 + 20e3 * i as f64, 1000.0, 0.0);
            raeume.push(m.add_space(st, Space::neu(p, format!("Raum {i}"))));
            m.commit();
        }
        (m, [a, b], raeume[0])
    }

    #[test]
    fn schritte_nennen_ihre_gebaeude() {
        let (mut m, [a, b], r) = zwei();
        let s = m.bau_stand();
        assert_eq!(m.gebaeude_seit(s), Some(vec![]));
        m.begin("Raum");
        assert!(m.set_space(r, |x| x.faktor = Some(0.5)));
        let t = m.commit().unwrap();
        assert_eq!(m.gebaeude_seit(s), Some(vec![Some(a)]));
        // Rückgängig und Wiederholen wie ein Schritt
        let u = m.bau_stand();
        m.apply(&t, Direction::Undo);
        assert_eq!(m.gebaeude_seit(u), Some(vec![Some(a)]));
        let u = m.bau_stand();
        m.apply(&t, Direction::Redo);
        assert_eq!(m.gebaeude_seit(u), Some(vec![Some(a)]));
        assert_eq!(m.gebaeude_seit(s), Some(vec![Some(a)]));
        // Ein Schritt an einem Geschoss heißt alle
        let u = m.bau_stand();
        let eg = m.ground_of(Some(b)).unwrap();
        let h = m.storey(eg).unwrap().height + 100.0;
        m.begin("Höhe");
        assert!(m.set_storey_height(eg, h));
        assert!(m.commit().is_some());
        assert_eq!(m.gebaeude_seit(u), None);
        // Änderung ohne Schritt: unbekannt
        let u = m.bau_stand();
        m.touch();
        assert_eq!(m.gebaeude_seit(u), None);
        // Eine Kopie weiß nichts über den Stand des Originals
        assert_eq!(m.clone().gebaeude_seit(m.bau_stand()), None);
    }

    /// Ein Bauteil, das in ein Geschoss eines anderen Gebäudes wechselt,
    /// nennt beide (Grundriss 16.10.); eine Änderung an einem Geschoss alle.
    #[test]
    fn geschosswechsel_nennt_beide_gebaeude() {
        let (m, [a, b], r) = zwei();
        let alt = m.element(r).unwrap().clone();
        let mut neu = alt.clone();
        neu.storey = m.ground_of(Some(b)).unwrap();
        let c = Change::Element {
            id: r,
            old: Some(alt),
            new: Some(neu),
        };
        assert_eq!(gebaeude(&m, &[c]).as_deref(), Some(&[Some(a), Some(b)][..]));
        let eg = m.ground_of(Some(a)).unwrap();
        let st = m.storey(eg).cloned();
        let c = Change::Storey {
            id: eg,
            old: st.clone(),
            new: st,
        };
        assert_eq!(gebaeude(&m, &[c]), None);
    }
}
