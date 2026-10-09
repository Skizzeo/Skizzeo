//! Sonnenstand (Analyse „Nordpfeil und Sonnenstand“, Schritt S0): wo die
//! Sonne an einem Ort zu einem Zeitpunkt steht, ihre Tagesbahn, Auf- und
//! Untergang und die gesetzliche Zeit in Deutschland (MEZ, MESZ).
//!
//! Ohne Bedienung und ohne Modell: Ort, Datum und Uhrzeit hinein, Azimut
//! und Höhe heraus. Darauf setzen das Sonnenstands-System und später die
//! Solar-Erweiterung auf.
//!
//! Gerechnet wird nach den Formeln des NOAA-Sonnenrechners (Meeus,
//! „Astronomical Algorithms“, Kurzform); die Genauigkeit liegt bei etwa
//! 0,01° für die Jahre 1950 bis 2050. Die Zeit ist Weltzeit (UTC); der
//! Unterschied zur dynamischen Zeit (gut eine Minute) wirkt sich unter
//! 0,01° aus.

use crate::{vec3, Vec3};

/// Ort auf der Erde in Grad, Nord und Ost positiv.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Lage {
    pub breite: f64,
    pub laenge: f64,
}

impl Lage {
    /// Vorgabe: Ganderkesee, Denkmalsweg (Analyse §1).
    pub const GANDERKESEE: Lage = Lage {
        breite: 53.0589,
        laenge: 8.5910,
    };
}

/// Ein Kalendertag (gregorianisch).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Datum {
    pub jahr: i32,
    pub monat: u32,
    pub tag: u32,
}

/// Tage im Monat.
fn tage_im_monat(jahr: i32, monat: u32) -> u32 {
    match monat {
        2 if (jahr % 4 == 0 && jahr % 100 != 0) || jahr % 400 == 0 => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

impl Datum {
    /// Ein gültiger Tag, sonst `None`.
    pub fn new(jahr: i32, monat: u32, tag: u32) -> Option<Datum> {
        ((1..=12).contains(&monat) && (1..=tage_im_monat(jahr, monat)).contains(&tag))
            .then_some(Datum { jahr, monat, tag })
    }

    /// Tage seit dem 01.01.1970.
    pub fn tage(self) -> i64 {
        // H. Hinnant, „days_from_civil“
        let j = i64::from(self.jahr) - i64::from(self.monat <= 2);
        let era = j.div_euclid(400);
        let yoe = j - era * 400;
        let m = i64::from(self.monat);
        let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + i64::from(self.tag) - 1;
        let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
        era * 146_097 + doe - 719_468
    }

    /// Der Tag `t` Tage nach dem 01.01.1970.
    pub fn aus_tagen(t: i64) -> Datum {
        let z = t + 719_468;
        let era = z.div_euclid(146_097);
        let doe = z - era * 146_097;
        let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
        let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
        let mp = (5 * doy + 2) / 153;
        let tag = (doy - (153 * mp + 2) / 5 + 1) as u32;
        let monat = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
        let jahr = (yoe + era * 400 + i64::from(monat <= 2)) as i32;
        Datum { jahr, monat, tag }
    }

    /// Der Tag `n` Tage später (negativ: früher).
    pub fn plus(self, n: i64) -> Datum {
        Datum::aus_tagen(self.tage() + n)
    }

    /// Wochentag: 0 Montag bis 6 Sonntag.
    pub fn wochentag(self) -> u32 {
        // Der 01.01.1970 war ein Donnerstag
        (self.tage() + 3).rem_euclid(7) as u32
    }

    /// Letzter Sonntag des Monats.
    fn letzter_sonntag(jahr: i32, monat: u32) -> Datum {
        let ende = Datum {
            jahr,
            monat,
            tag: tage_im_monat(jahr, monat),
        };
        ende.plus(-i64::from((ende.wochentag() + 1) % 7))
    }
}

/// Ein Zeitpunkt in Weltzeit (UTC): Sekunden seit dem 01.01.1970, 00:00.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Zeitpunkt(pub i64);

/// Gesetzliche Zeit in Deutschland zu einem Zeitpunkt.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Ortszeit {
    pub datum: Datum,
    /// Minuten seit Mitternacht (0 bis 1439).
    pub minuten: u32,
    /// MESZ statt MEZ.
    pub sommerzeit: bool,
}

impl Zeitpunkt {
    /// Weltzeit `stunde:minute:sekunde` am Tag `d`.
    pub fn utc(d: Datum, stunde: u32, minute: u32, sekunde: u32) -> Zeitpunkt {
        let s = i64::from(stunde) * 3600 + i64::from(minute) * 60 + i64::from(sekunde);
        Zeitpunkt(d.tage() * 86_400 + s)
    }

    /// Gesetzliche Zeit in Deutschland: MEZ (UTC+1), von 01:00 UTC am
    /// letzten Sonntag im März bis 01:00 UTC am letzten Sonntag im Oktober
    /// MESZ (UTC+2), nach der EU-Regel. Die doppelte Stunde im Oktober gilt
    /// als Sommerzeit; eine Uhrzeit in der übersprungenen Stunde im März
    /// wird als MEZ gelesen.
    pub fn ortszeit(d: Datum, stunde: u32, minute: u32) -> Zeitpunkt {
        let sommer = Zeitpunkt(Zeitpunkt::utc(d, stunde, minute, 0).0 - 7200);
        if sommer.ist_sommerzeit() {
            sommer
        } else {
            Zeitpunkt(sommer.0 + 3600)
        }
    }

    /// Ob zu diesem Zeitpunkt in Deutschland Sommerzeit gilt.
    pub fn ist_sommerzeit(self) -> bool {
        let jahr = Datum::aus_tagen(self.0.div_euclid(86_400)).jahr;
        let (beginn, ende) = sommerzeit(jahr);
        beginn <= self && self < ende
    }

    /// Die gesetzliche Zeit in Deutschland.
    pub fn in_ortszeit(self) -> Ortszeit {
        let sommerzeit = self.ist_sommerzeit();
        let s = self.0 + if sommerzeit { 7200 } else { 3600 };
        Ortszeit {
            datum: Datum::aus_tagen(s.div_euclid(86_400)),
            minuten: (s.rem_euclid(86_400) / 60) as u32,
            sommerzeit,
        }
    }

    /// `n` Sekunden später.
    pub fn plus(self, n: i64) -> Zeitpunkt {
        Zeitpunkt(self.0 + n)
    }

    /// Julianisches Datum.
    fn jd(self) -> f64 {
        self.0 as f64 / 86_400.0 + 2_440_587.5
    }
}

/// Beginn und Ende der Sommerzeit im Jahr `jahr`, je 01:00 UTC am letzten
/// Sonntag im März und im Oktober.
pub fn sommerzeit(jahr: i32) -> (Zeitpunkt, Zeitpunkt) {
    let tag = |monat| Zeitpunkt::utc(Datum::letzter_sonntag(jahr, monat), 1, 0, 0);
    (tag(3), tag(10))
}

/// Stand der Sonne am Himmel.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Sonnenstand {
    /// Grad von Nord im Uhrzeigersinn (90 Ost, 180 Süd, 270 West).
    pub azimut: f64,
    /// Scheinbare Höhe über dem Horizont in Grad, mit der Lichtbrechung der
    /// Luft (so sieht man die Sonne, so fällt ihr Licht).
    pub hoehe: f64,
    /// Höhe ohne Lichtbrechung in Grad.
    pub hoehe_geometrisch: f64,
}

impl Sonnenstand {
    /// Einheitsvektor zur Sonne: x nach Osten, y nach Norden, z nach oben.
    pub fn richtung(&self) -> Vec3 {
        self.richtung_modell(0.0)
    }

    /// Einheitsvektor zur Sonne im Modell, dessen Nordrichtung `nord` Grad
    /// im Uhrzeigersinn von +y liegt; z nach oben.
    pub fn richtung_modell(&self, nord: f64) -> Vec3 {
        let (a, h) = ((self.azimut + nord).to_radians(), self.hoehe.to_radians());
        vec3(a.sin() * h.cos(), a.cos() * h.cos(), h.sin())
    }
}

/// Deklination (Grad) und Zeitgleichung (Minuten) zum Zeitpunkt `t`.
fn bahn(t: Zeitpunkt) -> (f64, f64) {
    let jh = (t.jd() - 2_451_545.0) / 36_525.0;
    let l0 = (280.46646 + jh * (36000.76983 + jh * 0.0003032)).rem_euclid(360.0);
    let m = 357.52911 + jh * (35999.05029 - 0.0001537 * jh);
    let e = 0.016708634 - jh * (0.000042037 + 0.0000001267 * jh);
    let mr = m.to_radians();
    let c = mr.sin() * (1.914602 - jh * (0.004817 + 0.000014 * jh))
        + (2.0 * mr).sin() * (0.019993 - 0.000101 * jh)
        + (3.0 * mr).sin() * 0.000289;
    let omega = (125.04 - 1934.136 * jh).to_radians();
    let lambda = (l0 + c - 0.00569 - 0.00478 * omega.sin()).to_radians();
    let eps0 =
        23.0 + (26.0 + (21.448 - jh * (46.815 + jh * (0.00059 - jh * 0.001813))) / 60.0) / 60.0;
    let eps = (eps0 + 0.00256 * omega.cos()).to_radians();
    let dekl = (eps.sin() * lambda.sin()).asin().to_degrees();
    let y = (eps / 2.0).tan().powi(2);
    let l0r = l0.to_radians();
    let zg = y * (2.0 * l0r).sin() - 2.0 * e * mr.sin()
        + 4.0 * e * y * mr.sin() * (2.0 * l0r).cos()
        - 0.5 * y * y * (4.0 * l0r).sin()
        - 1.25 * e * e * (2.0 * mr).sin();
    (dekl, 4.0 * zg.to_degrees())
}

/// Lichtbrechung der Luft (Grad) bei der geometrischen Höhe `h` (Grad),
/// Normalbedingungen (NOAA).
fn brechung(h: f64) -> f64 {
    let t = h.to_radians().tan();
    let bogensekunden = if h > 85.0 {
        0.0
    } else if h > 5.0 {
        58.1 / t - 0.07 / t.powi(3) + 0.000086 / t.powi(5)
    } else if h > -0.575 {
        1735.0 + h * (-518.2 + h * (103.4 + h * (-12.79 + h * 0.711)))
    } else {
        -20.772 / t
    };
    bogensekunden / 3600.0
}

/// Wo die Sonne am Ort `lage` zum Zeitpunkt `t` steht.
pub fn sonnenstand(lage: Lage, t: Zeitpunkt) -> Sonnenstand {
    let (dekl, zg) = bahn(t);
    let minuten = t.0.rem_euclid(86_400) as f64 / 60.0;
    // Wahre Sonnenzeit und Stundenwinkel (Grad, mittags 0, nachmittags positiv)
    let wahre = (minuten + zg + 4.0 * lage.laenge).rem_euclid(1440.0);
    let stunde = (wahre / 4.0 - 180.0).to_radians();
    let (phi, d) = (lage.breite.to_radians(), dekl.to_radians());
    let sin_h = phi.sin() * d.sin() + phi.cos() * d.cos() * stunde.cos();
    let hoehe_geometrisch = sin_h.clamp(-1.0, 1.0).asin().to_degrees();
    // Azimut nach Meeus (13.5), von Süden gezählt, dann von Norden
    let a = stunde
        .sin()
        .atan2(stunde.cos() * phi.sin() - d.tan() * phi.cos());
    Sonnenstand {
        azimut: (a.to_degrees() + 180.0).rem_euclid(360.0),
        hoehe: hoehe_geometrisch + brechung(hoehe_geometrisch),
        hoehe_geometrisch,
    }
}

/// Richtung zur Sonne im Modell (Schnittstelle für die Solar-Erweiterung,
/// Analyse §7): [`sonnenstand`], um die Nordrichtung `nord` gedreht.
pub fn sonnenvektor_modell(lage: Lage, nord: f64, t: Zeitpunkt) -> Vec3 {
    sonnenstand(lage, t).richtung_modell(nord)
}

/// Geometrische Höhe des Sonnenmittelpunkts bei Auf- und Untergang:
/// Lichtbrechung am Horizont (34′) und halber Sonnendurchmesser (16′).
const HORIZONT: f64 = -0.833;

/// Höchststand der Sonne (wahrer Mittag) am Tag `datum` (Weltzeit des
/// Tags, für Orte östlich von etwa 170° W derselbe Kalendertag).
pub fn hoechststand(lage: Lage, datum: Datum) -> Zeitpunkt {
    let tag = Zeitpunkt::utc(datum, 0, 0, 0);
    let mut t = tag.plus(43_200);
    for _ in 0..3 {
        let (_, zg) = bahn(t);
        t = tag.plus(((720.0 - 4.0 * lage.laenge - zg) * 60.0).round() as i64);
    }
    t
}

/// Sucht zwischen `a` und `b` den Zeitpunkt, an dem die geometrische Höhe
/// `HORIZONT` kreuzt (auf die Sekunde).
fn horizont_zwischen(lage: Lage, mut a: Zeitpunkt, mut b: Zeitpunkt) -> Zeitpunkt {
    let ueber = |t| sonnenstand(lage, t).hoehe_geometrisch > HORIZONT;
    let a_ueber = ueber(a);
    while b.0 - a.0 > 1 {
        let m = Zeitpunkt(a.0 + (b.0 - a.0) / 2);
        if ueber(m) == a_ueber {
            a = m;
        } else {
            b = m;
        }
    }
    a
}

/// Sonnenauf- und -untergang am Tag `datum`; `None` bei Polarnacht oder
/// Mitternachtssonne.
pub fn auf_untergang(lage: Lage, datum: Datum) -> Option<(Zeitpunkt, Zeitpunkt)> {
    let mittag = hoechststand(lage, datum);
    let hoch = |t| sonnenstand(lage, t).hoehe_geometrisch;
    let (morgen, abend) = (mittag.plus(-43_200), mittag.plus(43_200));
    if hoch(mittag) <= HORIZONT || hoch(morgen) > HORIZONT || hoch(abend) > HORIZONT {
        return None;
    }
    Some((
        horizont_zwischen(lage, morgen, mittag),
        horizont_zwischen(lage, mittag, abend),
    ))
}

/// Tagesbahn der Sonne am Tag `datum`: von Aufgang bis Untergang alle
/// `schritt` Sekunden, Auf- und Untergang selbst eingeschlossen. Bei
/// Mitternachtssonne die 24 Stunden um den Höchststand, bei Polarnacht leer.
pub fn tagesbahn(lage: Lage, datum: Datum, schritt: i64) -> Vec<(Zeitpunkt, Sonnenstand)> {
    let schritt = schritt.max(1);
    let (von, bis) = match auf_untergang(lage, datum) {
        Some(p) => p,
        None => {
            let mittag = hoechststand(lage, datum);
            if sonnenstand(lage, mittag).hoehe_geometrisch <= HORIZONT {
                return Vec::new();
            }
            (mittag.plus(-43_200), mittag.plus(43_200))
        }
    };
    let mut out = Vec::new();
    let mut t = von;
    while t < bis {
        out.push((t, sonnenstand(lage, t)));
        t = t.plus(schritt);
    }
    out.push((bis, sonnenstand(lage, bis)));
    out
}

#[cfg(test)]
mod tests;
