//! Werkssätze und die Baustoffe des Projekts (BIM-Integration, R1 09:40).
//!
//! Der Werksbestand verweist über `mat=` auf die Guids der Startbibliothek
//! (`Model::new`). Zwei Fälle, in denen das Projekt diese Guids nicht führt:
//!
//! 1. Altdatei: Dateien von Ständen vor der heutigen Guid-Folge der
//!    Startbibliothek führen „Stahlbeton“, „Putz“, „Dämmung (WDVS)“ … unter
//!    anderen Guids (`app/src/abnahme_p5.szo`, Jörns p5.szo vom 07.10.).
//!    Erwartung: Die Werkssätze greifen trotzdem an diesen Baustoffen, so wie
//!    λ (A310) und Muster (A295) nach dem Namen übernommen werden. Ob das
//!    Laden die Guid umstellt oder der Leser die Verweise übersetzt, lässt
//!    der Test offen; er prüft nur, dass eine Bauleistung auf die Guid zeigt,
//!    die der Baustoff im Projekt hat. Und: kein Fehler nach Regel 73.
//! 2. Gelöschter Werksbaustoff: „Stahlbeton“ und „Putz“ sind in einem neuen
//!    Projekt unbenutzt und dürfen gelöscht werden. Der Nutzer kann den
//!    Werksbestand nicht ändern; darum kein Fehler nach Regel 73, und die
//!    Werkssätze bleiben im Katalog.

use sk_cost::befund::Schwere;
use sk_cost::lesen;
use sk_model::{szo, GuidGen, Model};

/// Werksname heute zu Name in Altdateien (wie `szo::add_lambda`).
const ALT: [(&str, &str); 1] = [("Porenbeton", "Gasbeton")];

fn fehler_73(m: &Model) -> Vec<String> {
    lesen::befunde(m, &lesen::werk(m))
        .into_iter()
        .filter(|b| b.regel == 73 && b.schwere == Schwere::Fehler)
        .map(|b| b.satz)
        .collect()
}

fn altdatei() -> Model {
    let text = include_str!("../../../app/src/abnahme_p5.szo");
    szo::read(text, GuidGen::with_seed(1))
        .expect("p5 lädt")
        .model
}

#[test]
fn neues_projekt_ohne_befund() {
    let m = Model::new();
    assert!(lesen::befunde(&m, &lesen::werk(&m)).is_empty());
}

#[test]
fn altdatei_ohne_fehler_73() {
    let m = altdatei();
    let f = fehler_73(&m);
    assert!(
        f.is_empty(),
        "{} Fehler, etwa {:#?}",
        f.len(),
        &f[..f.len().min(3)]
    );
}

#[test]
fn altdatei_werkssaetze_greifen_an_ihren_baustoffen() {
    let neu = Model::new();
    let werk_neu = lesen::werk(&neu);
    let m = altdatei();
    let k = lesen::werk(&m);
    let mut fehlt = vec![];
    let mut geprueft = vec![];
    for (_, w) in neu.materials().iter() {
        if !werk_neu.leistungen.iter().any(|l| l.mat == Some(w.guid)) {
            continue;
        }
        let namen = [w.name.as_str()]
            .into_iter()
            .chain(ALT.iter().filter(|(n, _)| *n == w.name).map(|(_, a)| *a))
            .collect::<Vec<_>>();
        let Some((_, x)) = m
            .materials()
            .iter()
            .find(|(_, x)| x.category == w.category && namen.contains(&x.name.as_str()))
        else {
            continue;
        };
        geprueft.push(x.name.clone());
        if !k.leistungen.iter().any(|l| l.mat == Some(x.guid)) {
            fehlt.push(x.name.clone());
        }
    }
    // Vorbedingung (Test und Abnahme): p5 führt mindestens die fünf
    // Baustoffe aus dem Befund (Stahlbeton, Putz, WDVS, Verblender,
    // Kerndämmung) mit Werksleistung; sonst prüft die Schleife nichts.
    assert!(geprueft.len() >= 5, "nur {geprueft:?} geprüft");
    assert!(fehlt.is_empty(), "keine Werksleistung für {fehlt:?}");
}

fn ohne_unbenutzte_baustoffe() -> (Model, Vec<String>) {
    let mut m = Model::new();
    let ids: Vec<_> = m
        .materials()
        .iter()
        .map(|(id, x)| (id, x.name.clone()))
        .collect();
    let mut weg = vec![];
    for (id, name) in ids {
        if m.can_remove_material(id) && m.remove_material(id) {
            weg.push(name);
        }
    }
    (m, weg)
}

#[test]
fn geloeschter_werksbaustoff_ist_kein_fehler_73() {
    let (m, weg) = ohne_unbenutzte_baustoffe();
    assert!(
        !weg.is_empty(),
        "Vorbedingung: ein Werksbaustoff ist löschbar"
    );
    let f = fehler_73(&m);
    assert!(f.is_empty(), "gelöscht {weg:?}, Fehler: {f:#?}");
}

#[test]
fn werkssaetze_bleiben_nach_dem_loeschen() {
    let frisch = lesen::werk(&Model::new());
    let (m, _) = ohne_unbenutzte_baustoffe();
    let k = lesen::werk(&m);
    assert_eq!(k.artikel.len(), frisch.artikel.len());
    assert_eq!(k.leistungen.len(), frisch.leistungen.len());
}
