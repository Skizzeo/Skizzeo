//! „Bildzeit messen (10 s)“ (Paket 9, Koordinator 19:39 und 20:04): die
//! Erfassung von `--zeiten` aus dem Menü heraus. Hier nur die reine
//! Auswertung; die Messung selbst steckt in der Hauptschleife.

use std::time::{Duration, Instant};

/// So lange misst der Menüeintrag.
pub const SPAN: Duration = Duration::from_secs(10);

/// Kopfzeile der Tabelle (wie bei `--zeiten`).
pub const HEADER: &str = "ereignisse;netz;schatten;zeichnen;tauschen;gesamt\n";

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

/// Zeile für die Statuszeile: Gesamtzeit je Bild (mit dem Warten auf
/// VSync) und die Arbeit (Ereignisse, Netz, Schatten, Zeichnen; ohne
/// Tauschen).
#[cfg(test)]
pub fn status_line(total: &[f64], work: &[f64]) -> String {
    status_meldung(total, work).to_string()
}

/// [`status_line`] als Satz für die Statuszeile.
pub fn status_meldung(total: &[f64], work: &[f64]) -> crate::meldung::Meldung {
    use crate::meldung::Meldung;
    match (stats(total), stats(work)) {
        (Some((m, p, n)), Some((wm, wp, _))) => Meldung::mit(
            "Bildzeit: Median {} ms, 95 % {} ms · Arbeit: Median {} ms, 95 % {} ms ({} {})",
            &[
                &de(m),
                &de(p),
                &de(wm),
                &de(wp),
                &n.to_string(),
                if n == 1 { "Bild" } else { "Bilder" },
            ],
        ),
        _ => Meldung::satz("Bildzeit: keine Bilder gemessen"),
    }
}

/// Dateiname der Tabelle zur Ortszeit.
pub fn file_name(j: u32, mo: u32, t: u32, h: u32, mi: u32) -> String {
    format!("bildzeit-{j:04}{mo:02}{t:02}-{h:02}{mi:02}.csv")
}

/// Laufende Messung: Beginn, Gesamtzeit und Arbeit je Bild (ms) und die
/// Tabelle.
pub struct Measure {
    pub start: Instant,
    pub total: Vec<f64>,
    pub work: Vec<f64>,
    pub table: String,
}

impl Measure {
    pub fn new() -> Measure {
        Measure {
            start: Instant::now(),
            total: Vec::new(),
            work: Vec::new(),
            table: HEADER.into(),
        }
    }

    /// Ein Bild: Ereignisse, Netz, Schatten, Zeichnen, Tauschen (ms).
    pub fn push(&mut self, parts: [f64; 5]) {
        let sum: f64 = parts.iter().sum();
        self.total.push(sum);
        self.work.push(parts[..4].iter().sum());
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

/// Die Spalten eines Bildes aus den Uhrzeiten des Rechners (Ereignisse,
/// Netz, Zeichnen, Tauschen) und der Zeit des Tiefen-Durchgangs auf der
/// Grafikkarte (S5/S6, 0 ohne neue Karte): Der Schatten geht vom Zeichnen
/// ab, ein Rest vom Tauschen, so bleibt die Summe die Dauer des Bildes.
/// Die Grafikkarte misst das Bild davor: Ist ihre Zeit länger als
/// Zeichnen und Tauschen zusammen, zählt nur so viel (Test, S6).
pub fn spalten(
    ereignisse: f64,
    netz: f64,
    schatten: f64,
    zeichnen: f64,
    tauschen: f64,
) -> [f64; 5] {
    let schatten = schatten.clamp(0.0, zeichnen + tauschen);
    let z = (zeichnen - schatten).max(0.0);
    let t = (tauschen - (schatten - zeichnen).max(0.0)).max(0.0);
    [ereignisse, netz, schatten, z, t]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tabelle_mit_dezimalkomma() {
        let mut m = Measure::new();
        m.push([0.5, 0.25, 0.0, 3.0, 1.0]);
        assert_eq!(m.total, [4.75]);
        assert_eq!(m.work, [3.75]);
        assert_eq!(
            m.table,
            format!("{HEADER}0,500;0,250;0,000;3,000;1,000;4,750\n")
        );
        assert!(!m.done());
    }

    /// Der Schatten geht vom Zeichnen ab, ein Rest vom Tauschen; die Summe
    /// bleibt.
    #[test]
    fn schatten_aus_zeichnen() {
        assert_eq!(
            spalten(0.5, 0.25, 0.0, 3.0, 1.0),
            [0.5, 0.25, 0.0, 3.0, 1.0]
        );
        assert_eq!(
            spalten(0.5, 0.25, 2.0, 3.0, 1.0),
            [0.5, 0.25, 2.0, 1.0, 1.0]
        );
        assert_eq!(
            spalten(0.5, 0.25, 3.5, 3.0, 1.0),
            [0.5, 0.25, 3.5, 0.0, 0.5]
        );
        // Länger als Zeichnen und Tauschen: die Summe bleibt
        let v = spalten(0.5, 0.25, 9.0, 3.0, 1.0);
        assert_eq!(v, [0.5, 0.25, 4.0, 0.0, 0.0]);
        assert_eq!(v.iter().sum::<f64>(), 0.5 + 0.25 + 3.0 + 1.0);
    }
}
