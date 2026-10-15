//! R7 Datei-Rundlauf an den Geländeflächen (#173): Eine `[tpatch]`-Zeile,
//! die nicht gilt, bleibt roh stehen, eine gültige mit fremdem Schlüssel
//! behält ihn. Stehen beide in der Datei, die rohe vorn, geht keine Zeile
//! verloren und keine kommt doppelt; der zweite Rundlauf ist bytegleich.

use sk_model::{szo, GuidGen};

const R7: &str = include_str!("../../sk-cost/referenz/r7-alle-abschnitte.szo");

fn rund(t: &str) -> String {
    szo::write(&szo::read(t, GuidGen::with_seed(1)).expect("lädt").model)
}

/// Die Zeilen ohne Kommentare, sortiert.
fn sortiert(t: &str) -> Vec<&str> {
    let mut v: Vec<&str> = t.lines().filter(|l| !l.starts_with('#')).collect();
    v.sort();
    v
}

/// `zeilen` hinter `[tcode]` eingefügt.
fn mit(zeilen: &[&str]) -> String {
    let mut out = String::new();
    for l in R7.lines() {
        out.push_str(l);
        out.push('\n');
        if l.starts_with("[tcode]") {
            for z in zeilen {
                out.push_str(z);
                out.push('\n');
            }
        }
    }
    out
}

const ROH: &str = "[tpatch] pts=0,0;1000,0;1000,1000 z=x";
const ZWEI_PUNKTE: &str = "[tpatch] pts=0,0;1000,0 z=10";
const FREMD: &str = "[tpatch] pts=0,0;2000,0;2000,2000 z=7 zukunft=1";
const GUELTIG: &str = "[tpatch] pts=0,0;3000,0;3000,3000 z=5";

#[test]
fn r7_tpatch_roh_und_fremd_bleiben() {
    for (name, zeilen) in [
        ("roh vor fremd", vec![ROH, FREMD]),
        ("fremd vor roh", vec![FREMD, ROH]),
        ("gültig, roh, fremd", vec![GUELTIG, ROH, FREMD]),
        ("roh, gültig, fremd", vec![ROH, GUELTIG, FREMD]),
        (
            "zwei rohe vor fremd",
            vec![ZWEI_PUNKTE, ROH, GUELTIG, FREMD],
        ),
    ] {
        let ein = mit(&zeilen);
        let aus = rund(&ein);
        let tp = |t: &str| -> Vec<String> {
            t.lines()
                .filter(|l| l.starts_with("[tpatch]"))
                .map(str::to_string)
                .collect()
        };
        assert_eq!(
            sortiert(&aus),
            sortiert(&ein),
            "{name}: Zeilen verändert; ein {:?}, aus {:?}",
            tp(&ein),
            tp(&aus)
        );
        assert_eq!(rund(&aus), aus, "{name}: zweiter Rundlauf nicht bytegleich");
    }
}
