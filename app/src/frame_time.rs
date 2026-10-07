//! „Bildzeit messen (10 s)“ (Paket 9, Koordinator 19:39 und 20:04): die
//! Erfassung von `--zeiten` aus dem Menü heraus. Hier nur die reine
//! Auswertung; die Messung selbst steckt in der Hauptschleife.

use std::time::{Duration, Instant};

/// So lange misst der Menüeintrag.
pub const SPAN: Duration = Duration::from_secs(10);

/// Kopfzeile der Tabelle (wie bei `--zeiten`).
pub const HEADER: &str = "ereignisse;netz;zeichnen;tauschen;gesamt\n";

/// (Median, 95 %, Anzahl) in ms; `None` ohne Werte. Median bei gerader
/// Anzahl: Mittel der beiden mittleren; 95 %: Wert mit dem Rang
/// ⌈0,95 · n⌉ (nächster Rang, kein Mitteln).
pub fn stats(ms: &[f64]) -> Option<(f64, f64, usize)> {
    let n = ms.len();
    if n == 0 {
        return None;
    }
    let mut v = ms.to_vec();
    v.sort_by(f64::total_cmp);
    let median = if n % 2 == 1 {
        v[n / 2]
    } else {
        (v[n / 2 - 1] + v[n / 2]) * 0.5
    };
    // ⌈95 · n / 100⌉ ganzzahlig, ohne Rundungsfehler
    let rank = (95 * n).div_ceil(100).max(1);
    Some((median, v[rank - 1], n))
}

/// Zahl mit zwei Nachkommastellen und Dezimalkomma.
fn de(v: f64) -> String {
    format!("{v:.2}").replace('.', ",")
}

/// Zeile für die Statuszeile.
pub fn status_line(ms: &[f64]) -> String {
    match stats(ms) {
        None => "Bildzeit: keine Bilder gemessen".into(),
        Some((m, p, n)) => format!(
            "Bildzeit: Median {} ms, 95 % {} ms ({n} {})",
            de(m),
            de(p),
            if n == 1 { "Bild" } else { "Bilder" }
        ),
    }
}

/// Dateiname der Tabelle zur Ortszeit.
pub fn file_name(j: u32, mo: u32, t: u32, h: u32, mi: u32) -> String {
    format!("bildzeit-{j:04}{mo:02}{t:02}-{h:02}{mi:02}.csv")
}

/// Laufende Messung: Beginn, Gesamtzeiten je Bild (ms) und die Tabelle.
pub struct Measure {
    pub start: Instant,
    pub total: Vec<f64>,
    pub table: String,
}

impl Measure {
    pub fn new() -> Measure {
        Measure {
            start: Instant::now(),
            total: Vec::new(),
            table: HEADER.into(),
        }
    }

    /// Ein Bild: Ereignisse, Netz, Zeichnen, Tauschen (ms).
    pub fn push(&mut self, parts: [f64; 4]) {
        let sum: f64 = parts.iter().sum();
        self.total.push(sum);
        let cols: Vec<String> = parts
            .iter()
            .chain([sum].iter())
            .map(|v| format!("{v:.3}").replace('.', ","))
            .collect();
        self.table.push_str(&cols.join(";"));
        self.table.push('\n');
    }

    pub fn done(&self) -> bool {
        self.start.elapsed() >= SPAN
    }

    /// Wartezeit bis zum Ende der Messung.
    pub fn wait(&self) -> Duration {
        SPAN.saturating_sub(self.start.elapsed())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tabelle_mit_dezimalkomma() {
        let mut m = Measure::new();
        m.push([0.5, 0.25, 3.0, 1.0]);
        assert_eq!(m.total, [4.75]);
        assert_eq!(m.table, format!("{HEADER}0,500;0,250;3,000;1,000;4,750\n"));
        assert!(!m.done());
    }
}
