//! Standardtyp gleich Vorgaben (Werkbank f044de00, Review 3cm): beim
//! Einlesen einer .szb ein Fehler, beim Öffnen eines älteren Projekts nur ein
//! Hinweis. Die gespeicherte Definition und ihre Exemplare bleiben erhalten,
//! Speichern verliert nichts.

use sk_model::erweiterung::{ExtDef, ExtPart};
use sk_model::{szo, GuidGen, Model};

const STUETZE: &str = include_str!("../../sk-szb/beispiele/werk.stuetze.szb");

fn lesen(text: &str) -> szo::Loaded {
    szo::read(text, GuidGen::with_seed(7)).unwrap_or_else(|e| panic!("{e:?}"))
}

fn zeilen(t: &str) -> Vec<String> {
    let mut v: Vec<String> = t.lines().map(str::to_string).collect();
    v.sort();
    v
}

#[test]
fn abweichender_standardtyp_beim_oeffnen() {
    let abweichend = STUETZE.replace(
        "werte=\"b=240; d=240\" standard=ja",
        "werte=\"b=300; d=240\" standard=ja",
    );
    assert_ne!(abweichend, STUETZE);
    let e = ExtDef::einlesen(&abweichend).unwrap_err();
    assert!(
        e.contains("Standardtyp weicht von den Vorgaben ab (b=300 statt 240)"),
        "{e}"
    );

    let mut m = Model::with_seed(1);
    m.add_building(1);
    let eg = m
        .storeys()
        .iter()
        .find(|(_, s)| s.short == "EG" && s.building.is_some())
        .map(|(id, _)| id)
        .unwrap();
    let d = ExtDef::lesen(STUETZE).unwrap();
    m.put_ext_def(d.clone()).unwrap();
    m.add_ext(eg, ExtPart::new(&d, [0.0, 0.0])).unwrap();
    m.add_ext(eg, ExtPart::new(&d, [1000.0, 0.0])).unwrap();
    let gut = szo::write(&m);
    let alt = gut.replace(
        "werte=\\\"b=240; d=240\\\" standard=ja",
        "werte=\\\"b=300; d=240\\\" standard=ja",
    );
    assert_ne!(alt, gut, "Standardtyp in der .szo geändert");

    let l = lesen(&alt);
    assert!(
        l.hints
            .iter()
            .any(|h| h.contains("Standardtyp weicht von den Vorgaben ab")),
        "Hinweis fehlt: {:?}",
        l.hints
    );
    let def = l
        .model
        .ext_def("werk.stuetze")
        .expect("Definition erhalten");
    assert_eq!(def.text, abweichend, "Text der Definition unverändert");
    assert_eq!(l.model.ext_uses("werk.stuetze").len(), 2);
    let geschrieben = szo::write(&l.model);
    assert_eq!(
        zeilen(&geschrieben),
        zeilen(&alt),
        "Speichern verliert nichts"
    );
    assert_eq!(
        szo::write(&lesen(&geschrieben).model),
        geschrieben,
        "stabil"
    );
}
