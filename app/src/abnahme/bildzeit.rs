// Abnahmetest A294: Auswertung von „Bildzeit messen (10 s)“ (Paket 9,
// Koordinator 19:39 und 20:04). Der Menüeintrag nutzt die Erfassung von
// `--zeiten`, zeigt danach eine Zeile in der Statuszeile und schreibt
// `bildzeit-JJJJMMTT-hhmm.csv`. Geprüft wird nur die reine Auswertung;
// Menü und Messung sind Handtest 0b (planung/handtest-phase6.md).
// Vorbereitet gegen main cb8eda4.
//
// Einbau: als `mod bildzeit { use super::*; … }` ans Ende von
// app/src/abnahme.rs. A60 bekommt zusätzlich die Zeilen
// ("Hilfe", "F1", true) und ("Bildzeit messen (10 s)", "", true) direkt
// vor ("Schließen", …); `befehl` die Arme `Command::Help` und
// `Command::MeasureFrameTime`.
//
// Festlegung (Test, im Sinne von review/bericht.md 1i): Median bei
// gerader Anzahl ist das Mittel der beiden mittleren Werte; das
// 95-%-Quantil ist der Wert mit dem Rang ⌈0,95 · n⌉ (nächster Rang, kein
// Mitteln). Zahlen mit zwei Nachkommastellen und Dezimalkomma, „1 Bild“
// in der Einzahl. Angenommene Namen stehen NUR in den Adaptern.
//
// Nachtrag (Review 3w Befund 3, Koordinator 22:36): Mit VSync liegt die
// Gesamtzeit immer bei etwa 16,7 ms. Die Zeile nennt deshalb zusätzlich
// die Arbeit je Bild (Ereignisse + Netz + Zeichnen, ohne Tauschen):
// „Bildzeit: Median …, 95 % … · Arbeit: Median …, 95 % … (N Bilder)“,
// beide mit zwei Nachkommastellen wie bisher. Angenommen: Feld
// `Measure::work` (ms je Bild) und `status_line(gesamt, arbeit)`.

// ===== Adapter =====

/// (Median, 95 %, Anzahl) in ms; `None` ohne Werte.
fn auswertung(ms: &[f64]) -> Option<(f64, f64, usize)> {
    crate::frame_time::stats(ms)
}

/// Zeile für die Statuszeile aus Gesamt- und Arbeitszeiten je Bild.
fn zeile(gesamt: &[f64], arbeit: &[f64]) -> String {
    crate::frame_time::status_line(gesamt, arbeit)
}

/// Messung mit diesen Bildern (Ereignisse, Netz, Schatten, Zeichnen,
/// Tauschen): (Gesamt je Bild, Arbeit je Bild, Tabelle).
fn messen(bilder: &[[f64; 5]]) -> (Vec<f64>, Vec<f64>, String) {
    let mut m = crate::frame_time::Measure::new();
    for b in bilder {
        m.push(*b);
    }
    (m.total, m.work, m.table)
}

/// Dateiname der Tabelle zur Ortszeit (Jahr, Monat, Tag, Stunde, Minute).
fn dateiname(j: u32, mo: u32, t: u32, h: u32, mi: u32) -> String {
    crate::frame_time::file_name(j, mo, t, h, mi)
}

// ===== Test =====

/// A294 (Koordinator 19:39, 20:04): Median und 95-%-Quantil aus einer
/// Liste in beliebiger Reihenfolge; leere Liste, ein Wert, gerade und
/// ungerade Anzahl; Zeilenformat mit Arbeit ohne Tauschen und Dateiname.
#[test]
fn a294_bildzeit_auswerten() {
    assert_eq!(auswertung(&[]), None);
    assert_eq!(zeile(&[], &[]), "Bildzeit: keine Bilder gemessen");

    assert_eq!(auswertung(&[4.2]), Some((4.2, 4.2, 1)));
    assert_eq!(
        zeile(&[4.2], &[1.5]),
        "Bildzeit: Median 4,20 ms, 95 % 4,20 ms · Arbeit: Median 1,50 ms, 95 % 1,50 ms (1 Bild)"
    );

    // ungerade: mittlerer Wert, Reihenfolge egal
    assert_eq!(auswertung(&[9.0, 1.0, 5.0]), Some((5.0, 9.0, 3)));
    // gerade: Mittel der beiden mittleren
    assert_eq!(auswertung(&[4.0, 1.0, 3.0, 2.0]), Some((2.5, 4.0, 4)));

    // 95 %: Rang ⌈0,95 · n⌉
    let v: Vec<f64> = (1..=100).rev().map(f64::from).collect();
    assert_eq!(auswertung(&v), Some((50.5, 95.0, 100)));
    let v: Vec<f64> = (1..=20).map(f64::from).collect();
    assert_eq!(auswertung(&v), Some((10.5, 19.0, 20)));
    let v: Vec<f64> = (1..=21).map(f64::from).collect();
    assert_eq!(auswertung(&v), Some((11.0, 20.0, 21)));
    // ein Ausreißer verschiebt den Median nicht
    let mut v = vec![5.0; 99];
    v.push(400.0);
    assert_eq!(auswertung(&v), Some((5.0, 5.0, 100)));

    assert_eq!(
        zeile(&[16.666, 4.0, 8.126], &[2.0, 0.5, 6.004]),
        "Bildzeit: Median 8,13 ms, 95 % 16,67 ms · Arbeit: Median 2,00 ms, 95 % 6,00 ms (3 Bilder)"
    );
    let v: Vec<f64> = (1..=601).map(|i| f64::from(i) / 100.0).collect();
    let a: Vec<f64> = v.iter().map(|x| x / 2.0).collect();
    assert_eq!(
            zeile(&v, &a),
            "Bildzeit: Median 3,01 ms, 95 % 5,71 ms · Arbeit: Median 1,50 ms, 95 % 2,85 ms (601 Bilder)"
        );

    // Mit VSync: Gesamt immer ≈ 16,7 ms, die Arbeit zeigt die Reserve.
    // Arbeit = Ereignisse + Netz + Schatten + Zeichnen, ohne Tauschen
    // (Schatten seit S5, §3.6 der Sonnen-Analyse); die Tabelle
    // bleibt wie bisher.
    let bilder: Vec<[f64; 5]> = (0..20)
        .map(|i| {
            [
                0.5,
                0.25,
                1.0,
                2.0 + f64::from(i % 4),
                16.7 - 3.75 - f64::from(i % 4),
            ]
        })
        .collect();
    let (gesamt, arbeit, tabelle) = messen(&bilder);
    assert_eq!(gesamt.len(), 20);
    assert_eq!(arbeit.len(), 20);
    for (b, (g, a)) in bilder.iter().zip(gesamt.iter().zip(&arbeit)) {
        assert!((g - b.iter().sum::<f64>()).abs() < 1e-9, "Gesamt");
        assert!(
            (a - (b[0] + b[1] + b[2] + b[3])).abs() < 1e-9,
            "Arbeit ohne Tauschen"
        );
    }
    assert_eq!(
            zeile(&gesamt, &arbeit),
            "Bildzeit: Median 16,70 ms, 95 % 16,70 ms · Arbeit: Median 5,25 ms, 95 % 6,75 ms (20 Bilder)"
        );
    assert!(tabelle.starts_with("ereignisse;netz;schatten;zeichnen;tauschen;gesamt\n"));
    assert_eq!(tabelle.lines().count(), 21, "Kopf und 20 Bilder");
    assert_eq!(
        tabelle.lines().nth(1),
        Some("0,500;0,250;1,000;2,000;12,950;16,700"),
        "Spalten in dieser Reihenfolge, Dezimalkomma"
    );

    assert_eq!(dateiname(2026, 10, 7, 9, 5), "bildzeit-20261007-0905.csv");
    assert_eq!(dateiname(2026, 1, 31, 23, 59), "bildzeit-20260131-2359.csv");
}
