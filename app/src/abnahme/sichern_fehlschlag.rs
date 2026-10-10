use super::*;
// Abnahmetest A158: Hinweiskarte, wenn das automatische Sichern scheitert
// (einstellungen/paket-f13-sichern.md Abschnitt 8, Review-Patch
// review/1s-sichern-fehlerpfade.patch: ein Fehlschlag gilt als ungesichert
// und wird nach dem Takt wiederholt). Spezifikation: test/abnahme-sichern.md.
// Aussehen und Verhalten im Fenster prüft Handtest H136.
//
// Einbau: als `mod sichern_fehlschlag { use super::*; … }` ans Ende von
// app/src/abnahme.rs. Nutzt aus abnahme.rs: befehl (Adapter E17).
//
// Die Entscheidung zieht der Bauthread als reine Funktion heraus. Angenommene
// Namen stehen nur in den Adaptern (Vorschlag, frei wählbar):
// `crate::autosave::{FailNotice, NoticeStep, fail_notice_lines,
// fail_notice_command}`; `FailNotice::{attempt, saved}` liefern je einen
// `NoticeStep` (Show, Hide, None). Die Zeit geht als Millisekunden hinein.
//
// Annahme: Nach einem gelungenen Sichern oder Speichern beginnt alles neu,
// auch die 30-Minuten-Sperre. Eine neue Fehlerserie zeigt die Karte also
// wieder beim zweiten Fehlschlag.

use crate::autosave::{FailNotice, NoticeStep};
use std::time::Duration;

const MIN: u64 = 60_000;

// ===== Adapter =====

fn hinweis() -> FailNotice {
    FailNotice::default()
}

fn schritt(s: NoticeStep) -> &'static str {
    match s {
        NoticeStep::Show => "zeigen",
        NoticeStep::Hide => "ausblenden",
        NoticeStep::None => "",
    }
}

/// Ergebnis eines Sicherungsversuchs im Takt zur Zeit `t` ms.
fn versuch(h: &mut FailNotice, gelungen: bool, t: u64) -> &'static str {
    schritt(h.attempt(gelungen, Duration::from_millis(t)))
}

/// Strg+S (oder „Jetzt speichern“) hat die Datei geschrieben.
fn gespeichert(h: &mut FailNotice, t: u64) -> &'static str {
    schritt(h.saved(Duration::from_millis(t)))
}

/// Die drei Zeilen der Karte: fett, gedimmt, Verweis.
fn zeilen() -> [String; 3] {
    crate::autosave::fail_notice_lines()
}

/// Befehl hinter dem Verweis „Jetzt speichern“.
fn verweis(unbenannt: bool) -> Option<String> {
    befehl(Some(crate::autosave::fail_notice_command(unbenannt)))
}

// ===== Test =====

/// A158 (F-13 §8): Ein einzelner Fehlschlag bleibt still. Ab dem zweiten
/// Fehlschlag in Folge erscheint die Karte, bei weiterem Scheitern höchstens
/// alle 30 Minuten wieder. Nach gelungenem Sichern oder Speichern ist der
/// Zähler 0, eine offene Karte blendet aus. Ein gelungener Versuch dazwischen
/// unterbricht die Folge. Texte und Verweis wie im Paket.
#[test]
fn a158_hinweis_erst_beim_zweiten_fehlschlag() {
    let mut h = hinweis();
    assert_eq!(versuch(&mut h, false, 5 * MIN), "", "einer bleibt still");
    assert_eq!(
        versuch(&mut h, false, 10 * MIN),
        "zeigen",
        "zweiter in Folge"
    );
    for t in [15, 20, 25, 30, 35] {
        assert_eq!(versuch(&mut h, false, t * MIN), "", "{t} min: gesperrt");
    }
    assert_eq!(
        versuch(&mut h, false, 40 * MIN),
        "zeigen",
        "30 Minuten nach der letzten Karte"
    );
    assert_eq!(versuch(&mut h, false, 45 * MIN), "");
    // Gelungenes Sichern: Karte weg, Zähler 0
    assert_eq!(versuch(&mut h, true, 50 * MIN), "ausblenden");
    assert_eq!(
        versuch(&mut h, false, 55 * MIN),
        "",
        "neue Folge: einer still"
    );
    assert_eq!(
        versuch(&mut h, false, 60 * MIN),
        "zeigen",
        "neue Folge: zweiter zeigt"
    );
    // Speichern: Karte weg, Zähler 0
    assert_eq!(gespeichert(&mut h, 61 * MIN), "ausblenden");
    assert_eq!(versuch(&mut h, false, 66 * MIN), "", "nach Speichern still");
    // Gelungener Versuch dazwischen unterbricht die Folge
    assert_eq!(versuch(&mut h, true, 71 * MIN), "");
    assert_eq!(versuch(&mut h, false, 76 * MIN), "");
    assert_eq!(versuch(&mut h, true, 81 * MIN), "");
    assert_eq!(versuch(&mut h, false, 86 * MIN), "", "nicht in Folge");

    // Texte
    assert_eq!(
        zeilen(),
        [
            "Automatisches Sichern klappt gerade nicht.".to_string(),
            "Der Ordner „Sicherungen“ ist voll oder gesperrt. Bitte die Datei speichern."
                .to_string(),
            "Jetzt speichern".to_string(),
        ]
    );
    assert_eq!(verweis(false).as_deref(), Some("Speichern"));
    assert_eq!(
        verweis(true).as_deref(),
        Some("Speichern unter"),
        "Unbenannt"
    );
}
