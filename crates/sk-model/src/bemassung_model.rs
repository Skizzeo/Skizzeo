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

    /// Entfernt die Versätze des Geschosses `st`, bevor es gelöscht wird
    /// (im selben Schritt, Rückgängig bringt sie zurück; BIM zu #116,
    /// Regel 24).
    pub(crate) fn massversatz_weg(&mut self, st: StoreyId) {
        let Some(g) = self.storey(st).map(|s| s.guid) else {
            return;
        };
        if !self.bemassung.versatz.keys().any(|k| k.0 == g) {
            return;
        }
        self.note_bemassung();
        self.bemassung.versatz.retain(|k, _| k.0 != g);
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

    /// Stil und Versatz ändern nur das Bild der Ketten: Der Schritt
    /// gleicht keine abgeleiteten Bauteile ab (Review W39 zu #126: sonst
    /// lief jeder Versatz über das ganze Modell).
    #[test]
    fn versatz_gleicht_nichts_ab() {
        let mut m = Model::new();
        m.begin("Haus");
        let b = m.add_building(2);
        let pts = [
            sk_math::vec3(0.0, 0.0, 0.0),
            sk_math::vec3(0.0, 8000.0, 0.0),
            sk_math::vec3(10_000.0, 8000.0, 0.0),
            sk_math::vec3(10_000.0, 0.0, 0.0),
        ];
        let eg = m.build_from_polygon(b, &pts).unwrap();
        m.commit();
        let st = m.run(eg).unwrap().storey;
        m.begin("Kette verschieben");
        assert!(m.set_massversatz(st, 270, Kettenart::Gesamt, 500.0));
        let mut s = m.massstil().clone();
        s.symbol = Symbol::Punkt;
        assert!(m.set_massstil(s));
        assert_eq!(m.sync_scope(), Some(Vec::new()));
        let t = m.commit().unwrap();
        assert_eq!(t.changes.len(), 1);
    }

    /// Ein gelöschtes Geschoss nimmt seine Versätze im selben Schritt mit:
    /// Die Datei bekommt keine `[massversatz]`-Zeile ohne Geschoss, und ein
    /// wieder eingeschaltetes Flachdach (gleiche Guid) erbt keinen alten
    /// Versatz; Rückgängig bringt sie zurück (BIM zu #116, Regel 24).
    #[test]
    fn geloeschtes_geschoss_nimmt_versaetze_mit() {
        let mut m = Model::new();
        m.begin("Gebäude");
        m.add_building(2);
        let b2 = m.add_building(2);
        m.commit();
        let st = *m.levels_in(Some(b2)).last().unwrap();
        m.begin("Flachdach");
        assert!(m.set_flat_roof(st, true));
        m.commit();
        let fd = m.roof_level_of(Some(b2)).unwrap();
        m.begin("Versatz");
        assert!(m.set_massversatz(st, 0, Kettenart::Wand, 300.0));
        assert!(m.set_massversatz(fd, 90, Kettenart::Gesamt, -200.0));
        m.commit();
        let zeilen = |m: &Model| crate::szo::write(m).matches("[massversatz]").count();
        assert_eq!(zeilen(&m), 2);

        // Flachdach aus: sein Versatz geht mit, Rückgängig bringt ihn zurück
        let g_fd = m.storey(fd).unwrap().guid;
        m.begin("Flachdach aus");
        assert!(m.set_flat_roof(st, false));
        let aus = m.commit().unwrap();
        assert_eq!(zeilen(&m), 1);
        m.apply(&aus, Direction::Undo);
        assert_eq!(m.massversatz(fd, 90, Kettenart::Gesamt), -200.0);
        m.apply(&aus, Direction::Redo);
        // Wieder ein: gleiche Guid, kein geerbter Versatz
        m.begin("Flachdach ein");
        assert!(m.set_flat_roof(st, true));
        m.commit();
        let fd2 = m.roof_level_of(Some(b2)).unwrap();
        assert_eq!(m.storey(fd2).unwrap().guid, g_fd);
        assert_eq!(m.massversatz(fd2, 90, Kettenart::Gesamt), 0.0);

        // Gebäude weg: seine Versätze auch, Rückgängig bringt sie zurück
        m.begin("Gebäude weg");
        assert!(m.remove_building(b2));
        let t = m.commit().unwrap();
        assert!(m.bemassung().versatz.is_empty());
        assert_eq!(zeilen(&m), 0);
        m.apply(&t, Direction::Undo);
        assert_eq!(m.massversatz(st, 0, Kettenart::Wand), 300.0);
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
