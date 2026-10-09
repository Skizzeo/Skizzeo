//! Lage des Bauorts und Nordrichtung (Sonnenstand S1): Abschnitt
//! `[location]` der `.szo`, höchstens eine Zeile. Ein eigener Abschnitt
//! statt Schlüssel an `[projectinfo]`, denn ein älterer Stand behält einen
//! ganzen fremden Abschnitt bytegleich (F-17b). Ohne Angaben wird nichts
//! geschrieben. Eine Zeile, die nicht als Lage zählt (keine Zahl, außerhalb
//! des Bereichs, keine bekannte Angabe), bleibt bytegleich stehen, bis eine
//! gesetzte Lage sie ersetzt; nie entstehen zwei Zeilen (Befund A der
//! Abnahme S1, Review 3br).

use super::*;
use sk_math::sonne::Lage;

/// Lage und Nordrichtung des Projekts; `None` heißt nicht gesetzt.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Location {
    /// Geografische Breite in Grad, Nord positiv (−90 … 90).
    pub lat: Option<f64>,
    /// Geografische Länge in Grad, Ost positiv (−180 … 180).
    pub lon: Option<f64>,
    /// Richtung nach geografisch Nord in Grad im Uhrzeigersinn von +y
    /// (0 … < 360). Nicht gesetzt: Nord ist +y, die Ansichten heißen
    /// weiter „Vorne“, „Hinten“ …
    pub north: Option<f64>,
}

impl Location {
    /// Nichts gesetzt: kein `[location]` in der Datei.
    pub fn is_unset(&self) -> bool {
        *self == Location::default()
    }

    /// Der Bauort für die Sonnenrechnung: die gesetzten Werte, sonst die
    /// Vorgabe des Arbeitsplatzes (Ganderkesee).
    pub fn lage(&self, vorgabe: Lage) -> Lage {
        Lage {
            breite: self.lat.unwrap_or(vorgabe.breite),
            laenge: self.lon.unwrap_or(vorgabe.laenge),
        }
    }

    /// Nordrichtung in Grad (0: +y), auch wenn sie nicht gesetzt ist.
    pub fn north_deg(&self) -> f64 {
        self.north.unwrap_or(0.0)
    }

    /// Gültige Werte: Breite und Länge im Bereich, Nordrichtung auf
    /// 0 … < 360 gebracht; unbrauchbare Angaben (außerhalb, nicht endlich)
    /// entfallen.
    pub fn normalized(self) -> Location {
        let within = |v: Option<f64>, max: f64| v.filter(|v| v.is_finite() && v.abs() <= max);
        Location {
            lat: within(self.lat, 90.0),
            lon: within(self.lon, 180.0),
            north: self
                .north
                .filter(|v| v.is_finite())
                .map(|v| v.rem_euclid(360.0))
                // −1e-14 ergibt 360,0
                .map(|v| if v >= 360.0 { 0.0 } else { v }),
        }
    }
}

impl Model {
    pub fn location(&self) -> &Location {
        &self.location
    }

    /// Die `[location]`-Zeile der Datei, die nicht als Lage zählt, solange
    /// keine Lage gesetzt wurde.
    pub(crate) fn location_raw(&self) -> Option<&str> {
        self.location_raw.as_deref()
    }

    /// Lage und Nordrichtung ändern (im offenen Schritt, z. B. „Nordrichtung
    /// geändert“ oder mit den Projektdaten). Gleiche Werte ändern nichts.
    /// Eine unlesbare Zeile aus der Datei entfällt damit (Rückgängig holt sie
    /// zurück).
    pub fn set_location(&mut self, l: Location) -> bool {
        let l = l.normalized();
        if l == self.location {
            return false;
        }
        match self.txn.as_mut() {
            Some(t) => {
                if t.noted.insert(Key::Location) {
                    t.changes.push(Change::Location {
                        old: self.location,
                        new: self.location,
                        raw: self.location_raw.clone(),
                    });
                }
            }
            None => debug_assert!(!self.strict, "Änderung ohne Schritt"),
        }
        self.location = l;
        self.location_raw = None;
        self.touch();
        true
    }

    /// Lage aus der Datei, ohne die Revision zu ändern; `raw`: die Zeile,
    /// wenn sie nicht zählt.
    pub(crate) fn load_location(&mut self, l: Location, raw: Option<String>) {
        self.location = l;
        self.location_raw = raw;
    }

    /// Lage eines neuen Projekts (Maske bei „Neu“): ohne Rückgängig-Schritt
    /// wie [`Model::init_project`].
    pub fn init_location(&mut self, l: Location) {
        debug_assert!(self.txn.is_none(), "Anfangswerte im Schritt");
        let l = l.normalized();
        if l != self.location {
            self.location = l;
            self.touch();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalized_haelt_den_bereich() {
        let l = Location {
            lat: Some(91.0),
            lon: Some(-180.0),
            north: Some(-30.0),
        }
        .normalized();
        assert_eq!(l.lat, None);
        assert_eq!(l.lon, Some(-180.0));
        assert_eq!(l.north, Some(330.0));
        assert_eq!(Location::default().normalized(), Location::default());
        let n = Location {
            north: Some(720.0),
            ..Default::default()
        };
        assert_eq!(n.normalized().north, Some(0.0));
        let n = Location {
            north: Some(-1e-15),
            ..Default::default()
        };
        assert_eq!(n.normalized().north, Some(0.0));
        let nan = Location {
            lat: Some(f64::NAN),
            north: Some(f64::INFINITY),
            ..Default::default()
        };
        assert!(nan.normalized().is_unset());
    }

    #[test]
    fn lage_mit_vorgabe() {
        let g = Lage::GANDERKESEE;
        assert_eq!(Location::default().lage(g), g);
        let l = Location {
            lat: Some(48.1),
            lon: Some(11.6),
            north: None,
        };
        assert_eq!(
            l.lage(g),
            Lage {
                breite: 48.1,
                laenge: 11.6
            }
        );
    }
}
