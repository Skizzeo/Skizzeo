//! Höhenfarben (Jörn 17.10.): das Gelände in 3D in Farbstufen nach der
//! Höhe statt in Rasen, mit kräftigerem Licht, damit man den Verlauf auch
//! von oben sieht. Die Stufen beginnen auf runden Höhen in m ü. NHN; es
//! sind höchstens [`STUFEN`], die Leiste zeigt sie als Legende.

/// Höchstens so viele Stufen, damit die Legende in die Leiste passt.
pub(crate) const STUFEN: u32 = 8;

/// Mögliche Höhe einer Stufe (mm): die feinste, mit der es höchstens
/// [`STUFEN`] werden.
const SCHRITTE: [f64; 14] = [
    50.0, 100.0, 200.0, 250.0, 500.0, 1000.0, 2000.0, 2500.0, 5000.0, 10000.0, 20000.0, 25000.0,
    50000.0, 100000.0,
];

/// `n` Stufen zu je `stufe` ab `z0` (mm zu ±0,00).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Stufen {
    pub(crate) z0: f64,
    pub(crate) stufe: f64,
    pub(crate) n: u32,
}

/// Stufen über `lo` bis `hi` (mm zu ±0,00); ±0,00 liegt auf `h0` m ü.
/// NHN, die Grenzen der Stufen auf Vielfachen der Stufe in NHN.
pub(crate) fn stufen(lo: f64, hi: f64, h0: f64) -> Stufen {
    let basis = h0 * 1000.0;
    let mut s = Stufen {
        z0: lo,
        stufe: SCHRITTE[0],
        n: 1,
    };
    for stufe in SCHRITTE {
        let z0 = ((lo + basis) / stufe).floor() * stufe - basis;
        let n = ((hi - z0) / stufe).ceil().max(1.0);
        s = Stufen {
            z0,
            stufe,
            n: n.min(f64::from(STUFEN)) as u32,
        };
        if n <= f64::from(STUFEN) {
            break;
        }
    }
    s
}

/// Legende von oben nach unten: Farbe und „104,00 bis 104,50“ (m ü.
/// NHN) je Stufe.
pub(crate) fn legende(s: &Stufen, h0: f64) -> Vec<([f32; 3], String)> {
    let m = |z: f64| format!("{:.2}", h0 + z / 1000.0).replace('.', ",");
    (0..s.n)
        .rev()
        .map(|i| {
            let a = s.z0 + f64::from(i) * s.stufe;
            let t = (i as f32 + 0.5) / s.n as f32;
            (
                sk_render::hoehenfarbe(t),
                format!("{} bis {}", m(a), m(a + s.stufe)),
            )
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 2,30 m Unterschied: Stufen zu 0,50 m auf runden Höhen in NHN, die
    /// tiefste und die höchste Höhe liegen darin.
    #[test]
    fn stufen_auf_runden_hoehen() {
        // ±0,00 auf 101,37 m ü. NHN, Gelände von 100,12 bis 102,42
        let (h0, lo, hi) = (101.37, -1250.0, 1050.0);
        let s = stufen(lo, hi, h0);
        assert_eq!(s.stufe, 500.0);
        assert_eq!(s.n, 5);
        assert!((h0 * 1000.0 + s.z0 - 100_000.0).abs() < 1e-6, "{s:?}");
        assert!(s.z0 <= lo && s.z0 + f64::from(s.n) * s.stufe >= hi);
        let l = legende(&s, h0);
        assert_eq!(l.len(), 5);
        assert_eq!(l[0].1, "102,00 bis 102,50");
        assert_eq!(l[4].1, "100,00 bis 100,50");
        assert_ne!(l[0].0, l[4].0, "oben und unten verschieden");
    }

    /// Eben: eine Stufe; sehr steil: höchstens [`STUFEN`].
    #[test]
    fn eben_und_steil() {
        assert_eq!(stufen(0.0, 0.0, 50.0).n, 1);
        let s = stufen(0.0, 30_000.0, 0.0);
        assert!(
            s.n <= STUFEN && s.z0 + f64::from(s.n) * s.stufe >= 30_000.0,
            "{s:?}"
        );
        let s = stufen(0.0, 9_000_000.0, 0.0);
        assert_eq!(s.n, STUFEN);
    }
}
