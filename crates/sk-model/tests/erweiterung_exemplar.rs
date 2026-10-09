//! Ein unlesbares Exemplar (`[extpart]`) verhindert das Öffnen nicht
//! (BIM-Routine 09.10.): die Zeile bleibt roh, mit Eigenschaften, Sperre
//! und Ausblenden, und ihre Nummer wird nicht neu vergeben.

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
fn unlesbares_exemplar_bleibt_roh() {
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
    let zwei = m.add_ext(eg, ExtPart::new(&d, [1000.0, 0.0])).unwrap();
    m.set_prop(
        zwei,
        "Hinweis",
        Some(sk_model::PropValue::Text("innen".into())),
    );
    let gut = szo::write(&m);
    for (alt, neu) in [
        (" x=1000 ", " x=abc "),
        (" y=0 ", " y=0 rot=quer "),
        (" seq=", " werte=\"b:300\" seq="),
    ] {
        let kaputt: String = gut
            .lines()
            .map(
                |l| match l.starts_with("[extpart]") && l.contains("ST-002") {
                    true => l.replacen(alt, neu, 1),
                    false => l.to_string(),
                },
            )
            .map(|l| format!("{l}\n"))
            .collect();
        assert_ne!(kaputt, gut, "{alt}");
        let l = lesen(&kaputt);
        assert!(
            l.hints.iter().any(|h| h
                .contains("Bauteil ST-002 der Erweiterung „werk.stuetze“ nicht lesbar")
                && h.ends_with("es bleibt unverändert in der Datei")),
            "{neu}: {:?}",
            l.hints
        );
        assert_eq!(l.model.ext_uses("werk.stuetze").len(), 1, "{neu}");
        assert_eq!(
            l.model.ext_raw().len(),
            2,
            "{neu}: Exemplar und Eigenschaft"
        );
        let geschrieben = szo::write(&l.model);
        assert_eq!(zeilen(&geschrieben), zeilen(&kaputt), "{neu}: alles bleibt");
        assert_eq!(
            szo::write(&lesen(&geschrieben).model),
            geschrieben,
            "stabil"
        );
        // Die Nummer des rohen Exemplars wird nicht neu vergeben
        let mut m = l.model;
        let eg = m
            .storeys()
            .iter()
            .find(|(_, s)| s.short == "EG" && s.building.is_some())
            .map(|(id, _)| id)
            .unwrap();
        let d = m.ext_def("werk.stuetze").unwrap().clone();
        m.begin("Setzen");
        let id = m.add_ext(eg, ExtPart::new(&d, [5000.0, 0.0])).unwrap();
        m.commit();
        assert_eq!(m.element(id).unwrap().number, "ST-003", "{neu}");
    }
}
