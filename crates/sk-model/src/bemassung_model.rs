//! Bemaßung: Methoden von `Model` ([`crate::bemassung`]), aus `model.rs`
//! ausgelagert. Stil und Versätze laufen als ein Eintrag durch Rückgängig;
//! die Wahl am Zahnrad ist Ansichtszustand ohne Schritt.

use super::*;
use crate::bemassung::{Bemassung, Kettenart, MassSicht, Stil, ANSICHTEN};
use crate::hoehen::FLOOR_BUILDUP;

/// Belag über dem Estrich (mm), bis das Raumbuch den Aufbau je Geschoss
/// liefert: Werksvorgabe Parkett 20 auf Estrichaufbau 140
/// ([`FLOOR_BUILDUP`]).
pub const FLOOR_COVER: f64 = 20.0;

impl Model {
    /// Stil und Versätze der Maßketten.
    pub fn bemassung(&self) -> &Bemassung {
        &self.bemassung
    }

    /// Stil der Maßketten.
    pub fn massstil(&self) -> &Stil {
        &self.bemassung.stil
    }

    /// Merkt den Stand vor der ersten Änderung im offenen Schritt.
    fn note_bemassung(&mut self) {
        match self.txn.as_mut() {
            Some(t) => {
                if t.noted.insert(Key::Bemassung) {
                    t.changes.push(Change::Bemassung {
                        old: Box::new(self.bemassung.clone()),
                        new: Box::default(),
                    });
                }
            }
            None => debug_assert!(!self.strict, "Änderung ohne Schritt"),
        }
    }

    /// Setzt den Stil; `false`, wenn er ungültig ist.
    pub fn set_massstil(&mut self, s: Stil) -> bool {
        if !s.gueltig() {
            return false;
        }
        if s == self.bemassung.stil {
            return true;
        }
        self.note_bemassung();
        self.bemassung.stil = s;
        self.touch();
        true
    }

    /// Stil eines neuen Projekts (Firmenstandard), ohne Schritt; ein
    /// ungültiger lässt den Werksstil.
    pub fn init_massstil(&mut self, s: Stil) {
        if s.gueltig() {
            self.bemassung.stil = s;
        }
    }

    /// Versatz der Kette (mm am Bau, + nach außen).
    pub fn massversatz(&self, st: StoreyId, seite: i32, art: Kettenart) -> f64 {
        self.storey(st)
            .and_then(|s| self.bemassung.versatz.get(&(s.guid, seite, art)))
            .copied()
            .unwrap_or(0.0)
    }

    /// Setzt den Versatz einer Kette; 0 entfernt ihn. `false` ohne Geschoss
    /// oder bei einem Wert über 100 m.
    pub fn set_massversatz(&mut self, st: StoreyId, seite: i32, art: Kettenart, v: f64) -> bool {
        let Some(g) = self.storey(st).map(|s| s.guid) else {
            return false;
        };
        if !v.is_finite() || v.abs() > 100_000.0 {
            return false;
        }
        let key = (g, seite.rem_euclid(360), art);
        let v = v.round();
        if self.bemassung.versatz.get(&key).copied().unwrap_or(0.0) == v {
            return true;
        }
        self.note_bemassung();
        if v == 0.0 {
            self.bemassung.versatz.remove(&key);
        } else {
            self.bemassung.versatz.insert(key, v);
        }
        self.touch();
        true
    }

    /// Lädt Stil und Versätze aus der Datei (ohne Schritt).
    pub(crate) fn load_bemassung(&mut self, b: Bemassung, sicht: MassSicht) {
        self.bemassung = b;
        self.mass_sicht = sicht;
    }

    /// Wahl am Zahnrad der Ansicht `i` ([`crate::view::SICHT_VIEWS`]):
    /// (Bemaßung zeigen, Maßbezugspunkte zeigen).
    pub fn mass_sicht(&self, i: usize) -> (bool, bool) {
        if i >= ANSICHTEN {
            return (false, false);
        }
        (self.mass_sicht.zeigen[i], self.mass_sicht.bezug[i])
    }

    /// Alles zur Wahl am Zahnrad, auch die rohen Zeilen der Datei.
    pub(crate) fn mass_sicht_alle(&self) -> &MassSicht {
        &self.mass_sicht
    }

    /// Setzt die Wahl am Zahnrad der Ansicht `i`. Ansichtszustand: kein
    /// Schritt, keine neue Revision; eine rohe Zeile dieser Ansicht weicht.
    pub fn set_mass_sicht(&mut self, i: usize, zeigen: bool, bezug: bool) {
        if i >= ANSICHTEN {
            return;
        }
        self.mass_sicht.zeigen[i] = zeigen;
        self.mass_sicht.bezug[i] = bezug;
        let name = format!("view={}", crate::view::SICHT_VIEWS[i]);
        self.mass_sicht
            .raw
            .retain(|l| !l.split_whitespace().any(|w| w == name));
    }

    /// OK Estrich des Geschosses (absolut, mm): OK Rohdecke plus
    /// Fußbodenaufbau ohne Belag, bis das Raumbuch ihn je Geschoss liefert.
    pub fn screed_top(&self, st: StoreyId) -> Option<f64> {
        self.storey(st)
            .map(|s| s.elevation + FLOOR_BUILDUP - FLOOR_COVER)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bemassung::Symbol;

    #[test]
    fn stil_und_versatz_sind_ein_rueckgaengig_schritt() {
        let mut m = Model::new();
        let st = m.storeys().iter().next().unwrap().0;
        m.begin("Bemaßung");
        let mut s = m.massstil().clone();
        s.symbol = Symbol::Schraeg;
        assert!(m.set_massstil(s.clone()));
        assert!(m.set_massversatz(st, 270, Kettenart::Wand, 250.4));
        let t = m.commit().unwrap();
        assert_eq!(m.massstil().symbol, Symbol::Schraeg);
        assert_eq!(m.massversatz(st, 270, Kettenart::Wand), 250.0);
        assert_eq!(m.massversatz(st, -90, Kettenart::Wand), 0.0);
        m.apply(&t, Direction::Undo);
        assert_eq!(m.massstil().symbol, Symbol::Kreis);
        assert_eq!(m.massversatz(st, 270, Kettenart::Wand), 0.0);
        m.apply(&t, Direction::Redo);
        assert_eq!(m.massversatz(st, 270, Kettenart::Wand), 250.0);
        // Ungültig: Rundung 3
        let mut u = s.clone();
        u.rundung = 3;
        assert!(!m.set_massstil(u));
        // Zurück auf null entfernt den Eintrag
        m.set_massversatz(st, 270, Kettenart::Wand, 0.0);
        assert!(m.bemassung().versatz.is_empty());
    }

    #[test]
    fn wahl_am_zahnrad_ohne_revision() {
        let mut m = Model::new();
        assert_eq!(m.mass_sicht(0), (true, false));
        assert_eq!(m.mass_sicht(3), (false, false));
        let rev = m.revision();
        m.set_mass_sicht(1, true, true);
        assert_eq!(m.mass_sicht(1), (true, true));
        assert_eq!(m.revision(), rev);
    }

    /// Ein neues Projekt bekommt den Firmenstandard ohne Schritt; ein
    /// ungültiger lässt das Werk.
    #[test]
    fn neues_projekt_mit_firmenstandard() {
        let mut m = Model::new();
        let rev = m.revision();
        let mut f = Stil::werk();
        f.symbol = Symbol::Schraeg;
        f.erste = 1500.0;
        m.init_massstil(f.clone());
        assert_eq!(*m.massstil(), f);
        assert_eq!(m.revision(), rev);
        f.rundung = 2;
        m.init_massstil(f);
        assert_eq!(m.massstil().erste, 1500.0);
    }
}
