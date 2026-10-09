//! E8c-a (Vorgabe Koordination 09.10. 21:38): Ein Baustoff, den ein
//! Exemplar einer Erweiterung nutzt, ist nicht löschbar, ob aus der
//! Erweiterung abgeleitet oder Werksbaustoff; „Verwendet in“ nennt das
//! Exemplar. Ein Projekt von vor E8c ohne den abgeleiteten Baustoff öffnet
//! mit „Baustoff … fehlt“ und legt beim Öffnen nichts still an.
use sk_model::erweiterung::{kennung, ExtDef, ExtPart};
use sk_model::{szo, GuidGen, Model, Use};

const GELAENDER: &str = include_str!("../../sk-szb/beispiele/werk.stabgelaender.szb");
const STUETZE: &str = include_str!("../../sk-szb/beispiele/werk.stuetze.szb");

fn projekt() -> (Model, sk_model::StoreyId) {
    let mut m = Model::with_seed(1);
    m.add_building(1);
    let eg = m
        .storeys()
        .iter()
        .find(|(_, s)| s.short == "EG" && s.building.is_some())
        .map(|(id, _)| id)
        .unwrap();
    (m, eg)
}

#[test]
fn genutzter_baustoff_nicht_loeschbar() {
    let (mut m, eg) = projekt();
    let gl = ExtDef::lesen(GELAENDER).unwrap();
    let st = ExtDef::lesen(STUETZE).unwrap();
    m.put_ext_def(gl.clone()).unwrap();
    m.put_ext_def(st.clone()).unwrap();
    let stahl = m.ext_material(&gl, "stahl_s235").expect("abgeleitet");
    let sb = m.ext_material(&st, "stahlbeton").expect("Werk");
    let g = m.add_ext(eg, ExtPart::new(&gl, [0.0, 0.0])).unwrap();
    let s = m.add_ext(eg, ExtPart::new(&st, [2000.0, 0.0])).unwrap();
    assert_eq!(m.element(g).unwrap().number, "GL-001");
    for (id, e) in [(stahl, g), (sb, s)] {
        assert!(
            m.material_uses(id).contains(&Use::Element(e)),
            "Verwendet in"
        );
        assert!(!m.can_remove_material(id));
        assert!(!m.remove_material(id));
        assert!(m.material(id).is_some());
    }
    // ohne Exemplar wieder löschbar
    m.delete_elements(&[g]);
    assert!(m.can_remove_material(stahl));
}

#[test]
fn projekt_von_vor_e8c() {
    let (mut m, eg) = projekt();
    let gl = ExtDef::lesen(GELAENDER).unwrap();
    m.put_ext_def(gl.clone()).unwrap();
    m.add_ext(eg, ExtPart::new(&gl, [0.0, 0.0])).unwrap();
    let k = kennung("werk.stabgelaender", "baustoff", "stahl_s235").to_ifc();
    let neu = szo::write(&m);
    // Vor E8c stand der abgeleitete Baustoff nicht in der Datei
    let alt: String = neu
        .lines()
        .filter(|l| !(l.contains(&k) && !l.starts_with("[ext")))
        .map(|l| format!("{l}\n"))
        .collect();
    assert_ne!(alt, neu);
    let l = szo::read(&alt, GuidGen::with_seed(1)).unwrap().model;
    assert_eq!(
        l.materials().len(),
        m.materials().len() - 1,
        "nichts still angelegt"
    );
    assert_eq!(l.ext_material(&gl, "stahl_s235"), None);
    assert_eq!(
        l.ext_baustoff_fehlt(&gl, "stahl_s235").as_deref(),
        Some("Baustoff Baustahl S235, verzinkt fehlt")
    );
    assert_eq!(l.ext_uses("werk.stabgelaender").len(), 1);
}
