//! Geld in ganzen Cent und Dateizahlen als Festkomma (Regel 83, E6): Keine
//! Summe läuft über `f64`, damit Summenprobe und CSV bytegleich bleiben.

use std::fmt;
use std::iter::Sum;
use std::ops::{Add, AddAssign, Neg, Sub};

/// Geldbetrag in ganzen Cent.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Cent(pub i64);

impl Cent {
    pub const NULL: Cent = Cent(0);

    /// Deutsche Schreibweise mit Tausenderpunkt: „3.977,60“.
    pub fn deutsch(self) -> String {
        let neg = self.0 < 0;
        let a = self.0.unsigned_abs();
        let euro = (a / 100).to_string();
        let mut s = String::new();
        for (i, c) in euro.chars().enumerate() {
            if i > 0 && (euro.len() - i).is_multiple_of(3) {
                s.push('.');
            }
            s.push(c);
        }
        format!("{}{s},{:02}", if neg { "−" } else { "" }, a % 100)
    }
}

impl fmt::Display for Cent {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{} €", self.deutsch())
    }
}

impl Add for Cent {
    type Output = Cent;
    fn add(self, o: Cent) -> Cent {
        Cent(self.0 + o.0)
    }
}

impl AddAssign for Cent {
    fn add_assign(&mut self, o: Cent) {
        self.0 += o.0;
    }
}

impl Sub for Cent {
    type Output = Cent;
    fn sub(self, o: Cent) -> Cent {
        Cent(self.0 - o.0)
    }
}

impl Neg for Cent {
    type Output = Cent;
    fn neg(self) -> Cent {
        Cent(-self.0)
    }
}

impl Sum for Cent {
    fn sum<I: Iterator<Item = Cent>>(it: I) -> Cent {
        Cent(it.map(|c| c.0).sum())
    }
}

/// Kaufmännisch gerundeter Quotient `z / n` (ab der Hälfte vom Nullpunkt
/// weg); `n` > 0.
pub fn runden(z: i128, n: i128) -> i128 {
    debug_assert!(n > 0);
    let h = n / 2;
    if z >= 0 {
        (z + h) / n
    } else {
        -((-z + h) / n)
    }
}

/// Zahl eines Dateifelds als Festkomma mit sechs Nachkommastellen.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Dez(pub i64);

impl Dez {
    pub const SKALA: i64 = 1_000_000;
    pub const NULL: Dez = Dez(0);
    pub const EINS: Dez = Dez(Dez::SKALA);

    pub const fn ganz(v: i64) -> Dez {
        Dez(v * Dez::SKALA)
    }

    /// Liest „12“, „0.45“, „-3.5“ mit höchstens `stellen` Nachkommastellen.
    pub fn lesen(s: &str, stellen: u32) -> Option<Dez> {
        let (neg, s) = match s.strip_prefix('-') {
            Some(r) => (true, r),
            None => (false, s),
        };
        let (ganz, bruch) = match s.split_once('.') {
            Some((g, b)) => (g, Some(b)),
            None => (s, None),
        };
        if ganz.is_empty() || !ganz.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        let mut v: i64 = ganz.parse::<i64>().ok()?.checked_mul(Dez::SKALA)?;
        if let Some(b) = bruch {
            if b.is_empty() || b.len() > stellen.min(6) as usize {
                return None;
            }
            if !b.bytes().all(|c| c.is_ascii_digit()) {
                return None;
            }
            let f: i64 = b.parse().ok()?;
            v = v.checked_add(f * 10i64.pow(6 - b.len() as u32))?;
        }
        Some(Dez(if neg { -v } else { v }))
    }

    /// Kürzeste exakte Schreibweise: „60“, „0.45“, „15.8“.
    pub fn text(self) -> String {
        let neg = self.0 < 0;
        let a = self.0.unsigned_abs();
        let g = a / Dez::SKALA as u64;
        let b = a % Dez::SKALA as u64;
        let mut s = if neg { format!("-{g}") } else { g.to_string() };
        if b > 0 {
            let f = format!("{b:06}");
            s.push('.');
            s.push_str(f.trim_end_matches('0'));
        }
        s
    }

    /// Näherung für Anzeige und Vergleich mit Modellmaßen (nie für Geld).
    pub fn f64(self) -> f64 {
        self.0 as f64 / Dez::SKALA as f64
    }

    /// Kaufmännisch auf ganze Cent: der Wert als Eurobetrag.
    pub fn cent(self) -> Cent {
        Cent(runden(self.0 as i128, (Dez::SKALA / 100) as i128) as i64)
    }
}

impl fmt::Display for Dez {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str(&self.text())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dez_lesen_und_schreiben() {
        for s in ["60", "0.45", "15.8", "-3.5", "1000000", "0.000001"] {
            assert_eq!(Dez::lesen(s, 6).unwrap().text(), s);
        }
        assert_eq!(Dez::lesen("1.50", 6).unwrap().text(), "1.5");
        assert_eq!(Dez::lesen("0.45", 6), Some(Dez(450_000)));
        assert!(Dez::lesen("1.23456", 4).is_none());
        for s in ["", ".5", "5.", "1e3", "+1", "1,5", "x", "--1"] {
            assert!(Dez::lesen(s, 6).is_none(), "{s}");
        }
    }

    #[test]
    fn cent_rundet_kaufmaennisch() {
        assert_eq!(runden(5, 10), 1);
        assert_eq!(runden(4, 10), 0);
        assert_eq!(runden(-5, 10), -1);
        assert_eq!(Dez::lesen("22.165", 6).unwrap().cent(), Cent(2217));
        assert_eq!(Cent(397_760).deutsch(), "3.977,60");
        assert_eq!(Cent(6_008_983).deutsch(), "60.089,83");
        assert_eq!(Cent(5).deutsch(), "0,05");
        assert_eq!(Cent(-1).deutsch(), "−0,01");
    }
}
