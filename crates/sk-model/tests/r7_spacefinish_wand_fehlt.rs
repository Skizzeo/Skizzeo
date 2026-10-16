//! R7 Datei-Rundlauf an einer Abweichung je Wandseite (`[spacefinish]
//! wall=`, BIM Regel 24 und 72): Eine Zeile, deren Wand es in der Datei
//! nicht gibt, zählt nicht; sie bleibt roh stehen, mit dem Hinweis „Wand
//! fehlt“, und die Datei kommt bytegleich zurück. Dieselbe Zeile an einer
//! vorhandenen Wand zählt.

use sk_model::{szo, GuidGen};

const R7: &str = include_str!("../../sk-cost/referenz/r7-alle-abschnitte.szo");

/// Raum R-001 in R7.
const R001: &str = "1dIrSPIIL5SulaBsBPBA7K";
/// Eine Wand in R7 (AW-014).
const WAND: &str = "0YDvZAYYLC8h7BbDrom9H3";
/// Keine Wand in R7.
const FREMD: &str = "2zzzzzzzzzzzzzzzzzzzzz";

/// `zeile` hinter dem letzten `[space]` eingefügt.
fn mit(zeile: &str) -> String {
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
            out.push_str(zeile);
            out.push('\n');
        }
    }
    out
}

/// Raum R-001 im geladenen Modell und seine Abweichungen.
fn r001(m: &sk_model::Model) -> usize {
    let r = m
        .elements()
        .iter()
        .find(|(_, e)| e.guid.to_string() == R001)
        .map(|(id, _)| id)
        .expect("R-001");
    m.space(r).unwrap().ausbau.len()
}

/// Abweichungen von R-001 in R7 selbst (seit #230 mit Ausbau je Raum).
fn basis() -> usize {
    r001(&szo::read(R7, GuidGen::with_seed(1)).expect("lädt").model)
}

fn sockel(wand: &str) -> String {
    format!("[spacefinish] space={R001} at=skirting wall={wand} side=l type=none")
}

#[test]
fn r7_spacefinish_wand_fehlt_bleibt_roh() {
    assert!(!R7.contains(FREMD));
    let ein = mit(&sockel(FREMD));
    let l = szo::read(&ein, GuidGen::with_seed(1)).expect("lädt");
    assert!(
        l.hints
            .iter()
            .any(|h| h.contains("[spacefinish] Wand fehlt")),
        "{:?}",
        l.hints
    );
    assert_eq!(r001(&l.model), basis(), "die Zeile zählt nicht");
    let aus = szo::write(&l.model);
    assert_eq!(aus.matches(&sockel(FREMD)).count(), 1, "{aus}");
    let zwei = szo::write(&szo::read(&aus, GuidGen::with_seed(1)).unwrap().model);
    assert_eq!(zwei, aus, "zweiter Rundlauf nicht bytegleich");
}

#[test]
fn r7_spacefinish_wand_da_zaehlt() {
    let ein = mit(&sockel(WAND));
    let l = szo::read(&ein, GuidGen::with_seed(1)).expect("lädt");
    assert!(
        !l.hints.iter().any(|h| h.contains("[spacefinish]")),
        "{:?}",
        l.hints
    );
    assert!(!R7.contains(&sockel(WAND)));
    assert_eq!(r001(&l.model), basis() + 1);
}
