//! Maßzahlen nach DIN 1356-1 (Bauzeichnungen): unter 1 m in Zentimetern,
//! ab 1 m in Metern mit zwei Stellen; Millimeter stehen hochgestellt
//! (36⁵, 2,36⁵). Rundung auf 1 mm (Standard) oder 5 mm.

/// Maßzahl in zwei Teilen: Hauptzahl und hochgestellte Millimeter (leer,
/// wenn keine).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Masszahl {
    pub haupt: String,
    pub hoch: String,
}

impl Masszahl {
    /// Als eine Zeichenkette mit hochgestellter Ziffer (für Tests und
    /// Hinweise).
    pub fn text(&self) -> String {
        const HOCH: [char; 10] = ['⁰', '¹', '²', '³', '⁴', '⁵', '⁶', '⁷', '⁸', '⁹'];
        let mut s = self.haupt.clone();
        s.extend(
            self.hoch
                .chars()
                .filter_map(|c| c.to_digit(10).map(|d| HOCH[d as usize])),
        );
        s
    }
}

/// Maßzahl für `mm` (Länge in Millimetern, Betrag), auf `rundung` mm
/// gerundet (1 oder 5).
pub fn masszahl(mm: f64, rundung: u8) -> Masszahl {
    let r = f64::from(rundung.max(1));
    let v = if mm.is_finite() {
        ((mm.abs() / r).round() * r) as i64
    } else {
        0
    };
    let (cm, rest) = (v / 10, v % 10);
    let haupt = if v < 1000 {
        cm.to_string()
    } else {
        format!("{},{:02}", cm / 100, cm % 100)
    };
    let hoch = if rest == 0 {
        String::new()
    } else {
        rest.to_string()
    };
    Masszahl { haupt, hoch }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(mm: f64) -> String {
        masszahl(mm, 1).text()
    }

    #[test]
    fn unter_einem_meter_in_zentimetern() {
        assert_eq!(t(240.0), "24");
        assert_eq!(t(365.0), "36⁵");
        assert_eq!(t(875.0), "87⁵");
        assert_eq!(t(115.0), "11⁵");
        assert_eq!(t(5.0), "0⁵");
        assert_eq!(t(999.0), "99⁹");
    }

    #[test]
    fn ab_einem_meter_in_metern() {
        assert_eq!(t(1000.0), "1,00");
        assert_eq!(t(1010.0), "1,01");
        assert_eq!(t(2365.0), "2,36⁵");
        assert_eq!(t(10_490.0), "10,49");
        assert_eq!(t(12_005.0), "12,00⁵");
    }

    #[test]
    fn rundung_auf_fuenf_millimeter_und_ganze_millimeter() {
        assert_eq!(masszahl(2363.0, 5).text(), "2,36⁵");
        assert_eq!(masszahl(2362.0, 5).text(), "2,36");
        assert_eq!(masszahl(999.6, 1).text(), "1,00");
        assert_eq!(masszahl(364.6, 1).text(), "36⁵");
        assert_eq!(masszahl(-240.0, 1).text(), "24");
    }

    #[test]
    fn teile_getrennt_fuer_die_hochzahl() {
        let z = masszahl(2365.0, 1);
        assert_eq!((z.haupt.as_str(), z.hoch.as_str()), ("2,36", "5"));
        let z = masszahl(1250.0, 1);
        assert_eq!((z.haupt.as_str(), z.hoch.as_str()), ("1,25", ""));
    }
}
