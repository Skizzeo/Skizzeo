//! Referenzhaus RH-4 „Erweiterungen“ (bim/integration/sollwerte-rh4.md):
//! RH-1 unverändert plus fünf Werkbank-Definitionen und sieben Exemplare.
//! Die Gleichung gegen RH-1 gilt unabhängig vom Werksbestand: Jede Position
//! von RH-1 ohne Erweiterungsansatz ist in Menge und GP gleich, geteilte
//! Positionen behalten ihren Schichtanteil, neue Positionen kommen nur aus
//! Erweiterungen, und RH-4 kostet genau 7.267,18 € mehr als RH-1.

use sk_cost::rechnung::Position;
use sk_cost::{lesen, Umfang};
use sk_model::{qto, szo, ElementId, ElementKind, GuidGen, Model};
use std::collections::BTreeMap;

const RH1: &str = include_str!("../referenz/rh1-standardhaus.szo");
const RH4: &str = include_str!("../referenz/rh4-erweiterungen.szo");

/// Sollwert §2: Netto RH-4 − Netto RH-1 in Cent.
const ERWEITERUNGEN: i64 = 726_718;

fn laden(t: &str) -> Model {
    let l = szo::read_with(t, GuidGen::with_seed(1), &lesen::ABSCHNITTE_SZO).expect("lädt");
    assert!(l.hints.is_empty(), "{:?}", l.hints);
    l.model
}

fn blatt(m: &Model) -> sk_cost::Kostenblatt {
    let k = lesen::katalog(m, None);
    lesen::kosten(m, &qto::schedule(m), &k, &Umfang::projekt())
}

fn exemplare(m: &Model) -> Vec<ElementId> {
    m.elements()
        .iter()
        .filter(|(_, e)| matches!(e.kind, ElementKind::Ext(_)))
        .map(|(id, _)| id)
        .collect()
}

#[test]
fn rh4_rundlauf_und_aufbau() {
    let m = laden(RH4);
    assert_eq!(szo::write(&m), RH4, "Speichern bytegleich");
    assert!(m.check().is_empty(), "{:?}", m.check());
    let k = lesen::katalog(&m, None);
    assert!(lesen::befunde(&m, &k).is_empty());
    let ohne_ext: String = RH4
        .lines()
        .filter(|l| !l.starts_with("[extdef]") && !l.starts_with("[extpart]"))
        .map(|l| format!("{l}\n"))
        .collect();
    assert_eq!(ohne_ext, RH1, "ohne Erweiterungszeilen gleich RH-1");
    assert_eq!(RH4.matches("[extdef]").count(), 5);
    assert_eq!(exemplare(&m).len(), 7);
}

#[test]
fn rh4_gleich_rh1_plus_erweiterungen() {
    let (m1, m4) = (laden(RH1), laden(RH4));
    let (b1, b4) = (blatt(&m1), blatt(&m4));
    let ext = exemplare(&m4);
    let schluessel = |p: &Position| format!("{:?}|{}", p.quelle, p.stoff);
    let alt: BTreeMap<String, &Position> =
        b1.positionen.iter().map(|p| (schluessel(p), p)).collect();
    let mut delta = 0i64;
    let mut fehler = Vec::new();
    for p in &b4.positionen {
        let aus_ext: i128 = p
            .ansatz
            .iter()
            .filter(|a| ext.contains(&a.element))
            .map(|a| a.menge)
            .sum();
        if p.ansatz
            .iter()
            .any(|a| ext.contains(&a.element) && a.aus.is_some())
        {
            fehler.push(format!("Folgeansatz aus Erweiterung: {}", p.kurz));
        }
        match alt.get(&schluessel(p)) {
            Some(q) if aus_ext == 0 => {
                if q.menge != p.menge || q.gp != p.gp {
                    fehler.push(format!("geändert ohne Erweiterung: {}", p.kurz));
                }
            }
            Some(q) => {
                let schicht: i128 = p
                    .ansatz
                    .iter()
                    .filter(|a| !ext.contains(&a.element))
                    .map(|a| a.menge)
                    .sum();
                let vorher: i128 = q.ansatz.iter().map(|a| a.menge).sum();
                if schicht != vorher {
                    fehler.push(format!("Schichtanteil geändert: {}", p.kurz));
                }
                delta += p.gp.0 - q.gp.0;
            }
            None => {
                if aus_ext == 0 {
                    fehler.push(format!("neu ohne Erweiterung: {}", p.kurz));
                }
                delta += p.gp.0;
            }
        }
    }
    for q in &b1.positionen {
        if !b4.positionen.iter().any(|p| schluessel(p) == schluessel(q)) {
            fehler.push(format!("fehlt in RH-4: {}", q.kurz));
        }
    }
    assert!(fehler.is_empty(), "{fehler:#?}");
    assert_eq!(delta, ERWEITERUNGEN, "Σ der Erweiterungsanteile");
    assert_eq!(b4.netto.0 - b1.netto.0, ERWEITERUNGEN, "Netto-Differenz");
    // Geschosse und Kostengruppen gehen mit Ausgleich im Netto auf
    let g: i64 = b4.nach_geschoss.iter().map(|x| x.1 .0).sum();
    let k: i64 = b4.nach_kg.iter().map(|x| x.1 .0).sum();
    assert_eq!(g + b4.ausgleich_geschoss.0, b4.netto.0);
    assert_eq!(k + b4.ausgleich_kg.0, b4.netto.0);
    println!("RH-4 netto {} (RH-1 {})", b4.netto.0, b1.netto.0);
}
