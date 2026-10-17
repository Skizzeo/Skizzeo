//! Befund B-245g (Test und Abnahme): Zwei getrennte Häuser behalten beim
//! ebenen Gelände ihre eigene Höhe, unabhängig von der Drehung der Lage.
//! Einbau: als crates/sk-model/tests/b245g_drehung.rs.
//! Stand: auf #245 7f2d7cb rot (gedreht gemeinsam), achsparallel grün.
use sk_math::vec3;
use sk_model::{LevelKind, Model};

/// Zwei Rechtecke 8×10 m mit `fuge` mm Abstand, beide um den Ursprung um
/// `grad` gedreht, Geländeversatz 500 und 1500 mm.
fn modell(grad: f64, fuge: f64) -> Model {
    let mut m = Model::with_seed(5);
    let (s, c) = grad.to_radians().sin_cos();
    let dreh = |x: f64, y: f64| vec3(x * c - y * s, x * s + y * c, 0.0);
    for (x0, versatz) in [(0.0, 500.0), (8000.0 + fuge, 1500.0)] {
        let b = m.add_building(1);
        let pts = [
            dreh(x0, 0.0),
            dreh(x0 + 8000.0, 0.0),
            dreh(x0 + 8000.0, 10000.0),
            dreh(x0, 10000.0),
        ];
        m.build_from_polygon(b, &pts).unwrap();
        m.begin("Gelände");
        assert!(m.set_terrain_offset_of(b, versatz));
        m.commit();
    }
    m
}

fn gruendungen(m: &Model) -> Vec<f64> {
    m.storeys()
        .iter()
        .filter(|(_, s)| s.kind == LevelKind::Foundation)
        .map(|(_, s)| s.elevation)
        .collect()
}

#[test]
fn b245g_getrennte_haeuser_unabhaengig_von_der_drehung() {
    let mut falsch = Vec::new();
    for fuge in [1000.0, 3000.0] {
        for grad in [0.0, 5.0, 10.0, 30.0, 45.0, 60.0] {
            let mut m = modell(grad, fuge);
            let vorher = gruendungen(&m);
            m.begin("Fläche");
            assert!(m.seed_flat_terrain(&[]));
            m.commit();
            let nachher = gruendungen(&m);
            let gleich = vorher
                .iter()
                .zip(&nachher)
                .all(|(a, b)| (a - b).abs() < 1e-6);
            if !gleich {
                falsch.push(format!(
                    "Fuge {fuge} mm, {grad}°: Gründung {vorher:?} → {nachher:?}"
                ));
            }
        }
    }
    assert!(
        falsch.is_empty(),
        "getrennte Häuser angeglichen: {falsch:?}"
    );
}
