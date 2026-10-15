//! R7 Datei-Rundlauf an den Abweichungen je Raum (`[spacefinish]`, RB6):
//! Eine gültige Zeile mit fremdem Schlüssel behält ihn, eine Zeile, die
//! nicht zählt, bleibt roh stehen. Keine Zeile geht verloren und keine
//! kommt doppelt, auch wenn die Datei die Räume in anderer Reihenfolge
//! nennt, als das Programm sie schreibt; der zweite Rundlauf ist bytegleich.

use sk_model::{szo, GuidGen};

const R7: &str = include_str!("../../sk-cost/referenz/r7-alle-abschnitte.szo");

/// Räume in R7: R-002 und R-001 (die Guid von R-002 sortiert vorn).
const R002: &str = "1JxRjJ2eb3A99QwGHJp0Ji";
const R001: &str = "1dIrSPIIL5SulaBsBPBA7K";
/// Bodenbelag der Werksvorlagen.
const BODEN: &str = "3qs8fx9fCJhdjCg3i9xau8";

fn rund(t: &str) -> String {
    szo::write(&szo::read(t, GuidGen::with_seed(1)).expect("lädt").model)
}

/// Die Zeilen ohne Kommentare, sortiert.
fn sortiert(t: &str) -> Vec<&str> {
    let mut v: Vec<&str> = t.lines().filter(|l| !l.starts_with('#')).collect();
    v.sort();
    v
}

/// `zeilen` hinter dem letzten `[space]` eingefügt.
fn mit(zeilen: &[String]) -> String {
    let letzte = R7
        .lines()
        .enumerate()
        .filter(|(_, l)| l.starts_with("[space] "))
        .map(|(i, _)| i)
        .last()
        .expect("R7 hat Räume");
    let mut out = String::new();
    for (i, l) in R7.lines().enumerate() {
        out.push_str(l);
        out.push('\n');
        if i == letzte {
            for z in zeilen {
                out.push_str(z);
                out.push('\n');
            }
        }
    }
    out
}

#[test]
fn r7_spacefinish_fremd_und_roh_bleiben() {
    let boden =
        |raum: &str, mehr: &str| format!("[spacefinish] space={raum} at=floor type={BODEN}{mehr}");
    let decke = |raum: &str| format!("[spacefinish] space={raum} at=ceiling type=none");
    let roh = |raum: &str| format!("[spacefinish] space={raum} at=xyz");
    for (name, zeilen) in [
        (
            "fremder Schlüssel am später geschriebenen Raum",
            vec![boden(R001, " zukunft=1"), decke(R002)],
        ),
        (
            "rohe Zeile vor fremdem Schlüssel",
            vec![roh(R002), boden(R002, " zukunft=1")],
        ),
        (
            "fremder Schlüssel vor roher Zeile",
            vec![boden(R002, " zukunft=1"), roh(R002)],
        ),
    ] {
        let ein = mit(&zeilen);
        let aus = rund(&ein);
        let sf = |t: &str| -> Vec<String> {
            t.lines()
                .filter(|l| l.starts_with("[spacefinish]"))
                .map(str::to_string)
                .collect()
        };
        assert_eq!(
            sortiert(&aus),
            sortiert(&ein),
            "{name}: Zeilen verändert; ein {:?}, aus {:?}",
            sf(&ein),
            sf(&aus)
        );
        assert_eq!(rund(&aus), aus, "{name}: zweiter Rundlauf nicht bytegleich");
    }
}
