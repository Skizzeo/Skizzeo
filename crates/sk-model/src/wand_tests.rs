//! Wand-Strang 10.10. (Tagesplan Flachdach W1–W3): Kreuzschraffur und
//! Schaumglas, Fußpunkt der Verblendschale auf der Dachterrasse,
//! Bekleidung unter der Untersichtdämmung.

use crate::attr::{cross_lines, FillKind, CROSS_FILL_NAME};
use crate::library::MatCategory;
use crate::model::{Model, CROSS_FILL_GUID, FOAMGLASS_MAT_GUID};
use crate::txn::Direction;
use crate::{szo, GuidGen};

/// W1: Baustoff und Schraffur entstehen erst beim ersten Gebrauch, mit
/// festen Guids, rückgängig machbar; vorher bleibt die Datei, wie sie war.
#[test]
fn schaumglas_und_kreuzschraffur_beim_ersten_gebrauch() {
    let mut m = Model::with_seed(101);
    let vorher = szo::write(&m);
    assert!(m.foamglass_material().is_none());
    assert!(!vorher.contains(CROSS_FILL_NAME));

    m.begin("Fußpunkt");
    let id = m.ensure_foamglass_material().unwrap();
    // ein zweites Mal: derselbe Baustoff, keine zweite Schraffur
    assert_eq!(m.ensure_foamglass_material(), Some(id));
    let t = m.commit().unwrap();
    let mat = m.material(id).unwrap();
    assert_eq!(mat.guid, FOAMGLASS_MAT_GUID);
    assert_eq!(mat.name, "Schaumglas-Dämmstein");
    assert_eq!(mat.category, MatCategory::Insulation);
    assert_eq!(mat.lambda, Some(0.058));
    assert_eq!(mat.trade, crate::trade::start_id("18330"));
    let f = m.attr().fill(mat.cut_fill).unwrap();
    assert_eq!(f.guid, CROSS_FILL_GUID);
    assert_eq!(f.name, CROSS_FILL_NAME);
    assert_eq!(f.kind, FillKind::Lines(cross_lines()));
    let angles: Vec<f32> = cross_lines().iter().map(|l| l.angle_deg).collect();
    assert_eq!(angles, vec![45.0, 135.0]);
    let kreuz = m
        .attr()
        .fills()
        .iter()
        .filter(|(_, x)| x.guid == CROSS_FILL_GUID)
        .count();
    assert_eq!(kreuz, 1);
    // eingebaut: nie löschbar
    assert!(!m.can_remove_material(id));
    assert!(m.check().is_empty(), "{:?}", m.check());

    // Datei: Rundlauf bytegleich, beide bleiben
    let text = szo::write(&m);
    assert!(text.contains("Schaumglas-Dämmstein") && text.contains(CROSS_FILL_NAME));
    let back = szo::read(&text, GuidGen::with_seed(5)).unwrap();
    assert!(back.hints.is_empty(), "{:?}", back.hints);
    assert_eq!(szo::write(&back.model), text);
    assert!(back.model.foamglass_material().is_some());

    // Rückgängig: beide wieder weg, Datei wie vorher
    m.apply(&t, Direction::Undo);
    assert!(m.foamglass_material().is_none());
    assert_eq!(szo::write(&m), vorher);
}
