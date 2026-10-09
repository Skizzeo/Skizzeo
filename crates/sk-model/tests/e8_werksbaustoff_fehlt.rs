//! E8-10, zweiter Teil (Vorgabe Koordination 09.10. 20:55): Fehlt ein
//! Werksbaustoff im Projekt (gelöscht), bekommt das Exemplar keinen Ersatz,
//! weder einen Baustoff derselben Kategorie noch den Terrassenbelag. Die
//! Mengen bleiben.
use sk_model::erweiterung::{ExtDef, ExtPart};
use sk_model::qto::{schedule, ElementQto};
use sk_model::{szo, GuidGen, Model};

const STUETZE: &str = include_str!("../../sk-szb/beispiele/werk.stuetze.szb");

fn mengen(m: &Model) -> Vec<(String, Option<f64>)> {
    schedule(m)
        .buildings
        .iter()
        .flat_map(|b| &b.storeys)
        .flat_map(|s| &s.groups)
        .flat_map(|g| &g.rows)
        .filter_map(|r| match &r.q {
            Some(ElementQto::Ext(x)) => Some(x),
            _ => None,
        })
        .flat_map(|x| x.mengen.iter().map(|q| (q.name.clone(), q.wert)))
        .collect()
}

#[test]
fn geloeschter_werksbaustoff_ohne_ersatz() {
    // Werksbaustoffe wie in abnahme_p5 (mit Terrassenbelag), ohne Bauteile
    let mut m = Model::with_seed(1);
    m.add_building(1);
    let p5 = szo::read(
        include_str!("../../../app/src/abnahme_p5.szo"),
        GuidGen::with_seed(1),
    )
    .unwrap()
    .model;
    for (_, x) in p5.materials().iter() {
        if m.materials().iter().all(|(_, y)| y.name != x.name) {
            m.add_material(x.clone());
        }
    }
    let eg = m
        .storeys()
        .iter()
        .find(|(_, s)| s.short == "EG" && s.building.is_some())
        .map(|(id, _)| id)
        .unwrap();
    let d = ExtDef::lesen(STUETZE).unwrap();
    m.put_ext_def(d.clone()).unwrap();
    m.add_ext(eg, ExtPart::new(&d, [0.0, 0.0])).unwrap();
    let vorher = mengen(&m);
    assert!(!vorher.is_empty());
    let sb = m
        .ext_material(&d, "stahlbeton")
        .expect("Stahlbeton im Werk");
    assert_eq!(m.material(sb).unwrap().name, "Stahlbeton");

    assert!(m.remove_material(sb), "Stahlbeton löschbar");
    assert!(m.materials().iter().all(|(_, x)| x.name != "Stahlbeton"));
    let ersatz = m
        .ext_material(&d, "stahlbeton")
        .and_then(|id| m.material(id))
        .map(|x| x.name.clone());
    assert_eq!(ersatz, None, "kein Ersatz für den fehlenden Werksbaustoff");
    assert_eq!(mengen(&m), vorher, "Mengen bleiben");
}
