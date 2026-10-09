//! Lage des Bauorts und Nordrichtung (Sonnenstand S1): Abschnitt
//! `[location]` der `.szo`, höchstens eine Zeile. Ein eigener Abschnitt
//! statt Schlüssel an `[projectinfo]`, denn ein älterer Stand behält einen
//! ganzen fremden Abschnitt bytegleich (F-17b). Ohne Angaben wird nichts
//! geschrieben. Eine Zeile, die nicht als Lage zählt (keine Zahl, außerhalb
//! des Bereichs, keine bekannte Angabe), bleibt bytegleich stehen, bis eine
//! gesetzte Lage sie ersetzt; nie entstehen zwei Zeilen (Befund A der
//! Abnahme S1, Review 3br).

use super::*;
use sk_math::sonne::{Datum, Lage};

/// Größter Betrag einer Koordinate des Fußpunkts (mm, 1000 km).
pub const FOOT_MAX: f64 = 1e9;

/// Fußpunkt des Nordpfeils (x, y in mm, Modellkoordinaten).
pub type Foot = [f64; 2];

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

/// Datum und Uhrzeit des Sonnenstands-Systems (S4, Analyse §8 09:25):
/// Ansichtszustand wie die Schnitte, ohne Rückgängig-Schritt und ohne neue
/// Revision. `[sun] date=2026-06-21 time=12:00 on=1`; geschrieben, sobald
/// das System in der Datei einmal an war, `on=1` nur, solange es an ist.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Sun {
    pub date: Datum,
    /// Gesetzliche Uhrzeit am Bauort (MEZ bzw. MESZ) in Minuten seit
    /// Mitternacht, 0 … 1439.
    pub minutes: u32,
    pub on: bool,
}

impl Sun {
    /// `date=` der Datei: `JJJJ-MM-TT`.
    pub fn date_text(&self) -> String {
        let d = self.date;
        format!("{:04}-{:02}-{:02}", d.jahr, d.monat, d.tag)
    }

    /// `time=` der Datei: `HH:MM`.
    pub fn time_text(&self) -> String {
        format!("{:02}:{:02}", self.minutes / 60, self.minutes % 60)
    }

    /// Liest `JJJJ-MM-TT` (Jahr 1 … 9999, gültiger Kalendertag).
    pub fn parse_date(t: &str) -> Option<Datum> {
        let mut it = t.split('-');
        let mut teil = |n: usize| {
            it.next()
                .filter(|s| s.len() == n && s.bytes().all(|b| b.is_ascii_digit()))
                .and_then(|s| s.parse::<u32>().ok())
        };
        let (j, m, d) = (teil(4)?, teil(2)?, teil(2)?);
        if it.next().is_some() || j == 0 {
            return None;
        }
        Datum::new(j as i32, m, d)
    }

    /// Liest `HH:MM` (00:00 … 23:59) als Minuten.
    pub fn parse_time(t: &str) -> Option<u32> {
        let (h, m) = t.split_once(':')?;
        let zahl = |s: &str| {
            (s.len() == 2 && s.bytes().all(|b| b.is_ascii_digit()))
                .then(|| s.parse::<u32>().ok())
                .flatten()
        };
        let (h, m) = (zahl(h)?, zahl(m)?);
        (h < 24 && m < 60).then_some(h * 60 + m)
    }
}

impl Model {
    /// Datum, Uhrzeit und Schalter des Sonnenstands-Systems; `None`, solange
    /// es in dieser Datei nie an war.
    pub fn sun(&self) -> Option<Sun> {
        self.sun
    }

    /// Sonnenstand ändern: ohne Schritt und ohne neue Revision, wie
    /// [`Model::set_cut`]. Eine unlesbare `[sun]`-Zeile entfällt damit.
    pub fn set_sun(&mut self, s: Sun) {
        self.sun = Some(s);
        self.sun_raw = None;
    }

    /// Die `[sun]`-Zeile der Datei, die nicht zählt, solange keine gesetzt
    /// wurde.
    pub(crate) fn sun_raw(&self) -> Option<&str> {
        self.sun_raw.as_deref()
    }

    pub(crate) fn load_sun(&mut self, s: Option<Sun>, raw: Option<String>) {
        self.sun = s;
        self.sun_raw = raw;
    }

    pub fn location(&self) -> &Location {
        &self.location
    }

    /// Die `[location]`-Zeile der Datei, die nicht als Lage zählt, solange
    /// keine Lage gesetzt wurde.
    pub(crate) fn location_raw(&self) -> Option<&str> {
        self.location_raw.as_deref()
    }

    /// Fußpunkt des Nordpfeils (`x=`/`y=` an `[location]`, Sonnenstand S2),
    /// wo der Planer ihn aufgezogen oder hingeschoben hat; gilt nur bei
    /// gesetzter Nordrichtung. Fehlt er, steht der Pfeil neben dem Gebäude.
    pub fn north_foot(&self) -> Option<Foot> {
        self.north_foot.filter(|_| self.location.north.is_some())
    }

    /// Merkt den Stand vor der ersten Änderung im offenen Schritt.
    fn note_location(&mut self) {
        match self.txn.as_mut() {
            Some(t) => {
                if t.noted.insert(Key::Location) {
                    t.changes.push(Change::Location {
                        old: self.location,
                        new: self.location,
                        raw: self.location_raw.clone(),
                        foot: [self.north_foot; 2],
                    });
                }
            }
            None => debug_assert!(!self.strict, "Änderung ohne Schritt"),
        }
    }

    /// Lage und Nordrichtung ändern (im offenen Schritt, z. B. mit den
    /// Projektdaten); der Fußpunkt bleibt. Gleiche Werte ändern nichts.
    /// Eine unlesbare Zeile aus der Datei entfällt damit (Rückgängig holt sie
    /// zurück).
    pub fn set_location(&mut self, l: Location) -> bool {
        let l = l.normalized();
        if l == self.location {
            return false;
        }
        self.note_location();
        self.location = l;
        self.location_raw = None;
        self.touch();
        true
    }

    /// Nordpfeil setzen, drehen oder verschieben (im offenen Schritt,
    /// „Nordrichtung geändert“ bzw. „Nordpfeil verschoben“): Richtung in
    /// Grad im Uhrzeigersinn von +y und Fußpunkt zusammen; Breite und
    /// Länge bleiben. Unbrauchbares ändert nichts.
    pub fn set_north_arrow(&mut self, north: f64, foot: Option<Foot>) -> bool {
        let l = Location {
            north: Some(north),
            ..self.location
        }
        .normalized();
        let foot = foot.filter(|p| p.iter().all(|v| v.is_finite() && v.abs() <= FOOT_MAX));
        if l.north.is_none() || (l == self.location && foot == self.north_foot()) {
            return false;
        }
        self.note_location();
        self.location = l;
        self.north_foot = foot;
        self.location_raw = None;
        self.touch();
        true
    }

    /// Lage aus der Datei, ohne die Revision zu ändern; `raw`: die Zeile,
    /// wenn sie nicht zählt.
    pub(crate) fn load_location(&mut self, l: Location, foot: Option<Foot>, raw: Option<String>) {
        self.location = l;
        self.north_foot = foot;
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

/// Licht der Schatten in einer Ansicht (S7, Analyse §3.4): klassisch
/// parallel zur Raumdiagonale von vorne links bzw. vorne rechts, 45° von
/// vorne oben, oder die Sonne des Sonnenstands-Systems.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShadeLight {
    FrontLeft,
    FrontRight,
    Top,
    Sun,
}

/// Schatten einer der vier Ansichten (S7): an oder aus, als graue Fläche
/// oder Schraffur, mit welchem Licht. `[viewshade] view=front on=1
/// fill=area light=front-left`; geschrieben nur für Ansichten mit eigener
/// Wahl, die übrigen folgen der Vorgabe.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ViewShade {
    pub on: bool,
    pub hatch: bool,
    pub light: ShadeLight,
}

/// Ansichten in der Reihenfolge der Datei- und Speicherplätze: Vorne,
/// Hinten, Links, Rechts (nach Modellachsen, nicht nach Himmelsrichtung).
pub const SHADE_VIEWS: [&str; 4] = ["front", "back", "left", "right"];

impl ViewShade {
    /// Werkvorgabe (§6, Frage 2): an, graue Fläche, klassisch vorne links.
    pub const WERK: ViewShade = ViewShade {
        on: true,
        hatch: false,
        light: ShadeLight::FrontLeft,
    };

    /// `fill=` der Datei.
    pub fn fill_text(&self) -> &'static str {
        if self.hatch {
            "hatch"
        } else {
            "area"
        }
    }

    /// `light=` der Datei.
    pub fn light_text(&self) -> &'static str {
        match self.light {
            ShadeLight::FrontLeft => "front-left",
            ShadeLight::FrontRight => "front-right",
            ShadeLight::Top => "top",
            ShadeLight::Sun => "sun",
        }
    }

    pub fn parse_fill(t: &str) -> Option<bool> {
        match t {
            "area" => Some(false),
            "hatch" => Some(true),
            _ => None,
        }
    }

    pub fn parse_light(t: &str) -> Option<ShadeLight> {
        match t {
            "front-left" => Some(ShadeLight::FrontLeft),
            "front-right" => Some(ShadeLight::FrontRight),
            "top" => Some(ShadeLight::Top),
            "sun" => Some(ShadeLight::Sun),
            _ => None,
        }
    }
}

impl Model {
    /// Schatten der Ansicht `i` (Reihenfolge [`SHADE_VIEWS`]): die eigene
    /// Wahl des Projekts oder die Vorgabe `vorgabe`.
    pub fn view_shade(&self, i: usize, vorgabe: ViewShade) -> ViewShade {
        self.view_shade.get(i).copied().flatten().unwrap_or(vorgabe)
    }

    /// Die eigene Wahl des Projekts für Ansicht `i`, falls es eine gibt.
    pub fn view_shade_own(&self, i: usize) -> Option<ViewShade> {
        self.view_shade.get(i).copied().flatten()
    }

    /// Schatten einer Ansicht wählen: Ansichtszustand ohne Schritt und ohne
    /// neue Revision, wie [`Model::set_sun`]. Gleicht die Wahl der Vorgabe
    /// `vorgabe`, entfällt die eigene Wahl (und ihre Zeile). Eine unlesbare
    /// Zeile dieser Ansicht entfällt damit.
    pub fn set_view_shade(&mut self, i: usize, s: ViewShade, vorgabe: ViewShade) {
        if let Some(v) = self.view_shade.get_mut(i) {
            *v = (s != vorgabe).then_some(s);
            let name = format!("view={}", SHADE_VIEWS[i]);
            self.view_shade_raw
                .retain(|l| !l.split_whitespace().any(|w| w == name));
        }
    }

    /// `[viewshade]`-Zeilen der Datei, die nicht zählen, roh.
    pub(crate) fn view_shade_raw(&self) -> &[String] {
        &self.view_shade_raw
    }

    pub(crate) fn load_view_shade(&mut self, s: [Option<ViewShade>; 4], raw: Vec<String>) {
        self.view_shade = s;
        self.view_shade_raw = raw;
    }

    /// Ansicht `i`: Teile unter dem Gelände (z < 0) gestrichelt (`true`)
    /// oder ausgeblendet (Jörn 09.10. 14:10, S11).
    pub fn view_below(&self, i: usize) -> bool {
        self.view_below.get(i).copied().unwrap_or(false)
    }

    /// Wie [`Model::set_view_shade`]: Ansichtszustand ohne Schritt; eine
    /// unlesbare Zeile dieser Ansicht entfällt.
    pub fn set_view_below(&mut self, i: usize, gestrichelt: bool) {
        if let Some(v) = self.view_below.get_mut(i) {
            *v = gestrichelt;
            let name = format!("view={}", SHADE_VIEWS[i]);
            self.view_below_raw
                .retain(|l| !l.split_whitespace().any(|w| w == name));
        }
    }

    /// `[viewbelow]`-Zeilen der Datei, die nicht zählen, roh.
    pub(crate) fn view_below_raw(&self) -> &[String] {
        &self.view_below_raw
    }

    pub(crate) fn load_view_below(&mut self, b: [bool; 4], raw: Vec<String>) {
        self.view_below = b;
        self.view_below_raw = raw;
    }
}
