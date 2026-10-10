//! Gelände Thema 1 (Prüfung 10.10.): Wer den Versatz OK Sohle über
//! Gelände hin und zurück stellt, bekommt die Gründung zurück. Die Schürze
//! wird nicht bei jedem Umweg tiefer, ob in einem Zug (Ziehen am Gelände)
//! oder in einzelnen Schritten.
use sk_math::vec3;
use sk_model::{ElementId, Model};

fn haus() -> (Model, ElementId) {
    let mut m = Model::with_seed(12);
    let b = m.add_building(1);
    let pts = [
        vec3(0.0, 0.0, 0.0),
        vec3(0.0, 8000.0, 0.0),
        vec3(10000.0, 8000.0, 0.0),
        vec3(10000.0, 0.0, 0.0),
    ];
    let eg = m.build_from_polygon(b, &pts).unwrap();
    let fuss = m.foundation_of(eg).unwrap().1.unwrap();
    (m, fuss)
}

#[test]
fn versatz_in_einem_zug_hin_und_zurueck() {
    let (mut m, fuss) = haus();
    let d0 = m.footing_depth(fuss).unwrap();
    m.begin("Gelände");
    for v in [-200.0, -400.0, -600.0, -800.0, -1000.0, -600.0, -200.0, 0.0] {
        m.set_terrain_offset(v);
    }
    m.commit();
    assert_eq!(m.terrain_offset(), 0.0);
    assert_eq!(m.footing_depth(fuss), Some(d0));
}

#[test]
fn versatz_in_schritten_hin_und_zurueck() {
    let (mut m, fuss) = haus();
    let d0 = m.footing_depth(fuss).unwrap();
    for v in [-3000.0, 3000.0, 0.0] {
        m.begin("Gelände");
        assert!(m.set_terrain_offset(v));
        m.commit();
    }
    assert_eq!(m.footing_depth(fuss), Some(d0));
}
