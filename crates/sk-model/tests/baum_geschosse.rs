//! Karte „Baum“ je Geschoss (Review #233): die Bauteile werden in einem
//! Gang auf die Geschosse verteilt. Unter jedem Geschoss stehen genau die
//! Bauteile, deren [`schedule_storey`] es ist, Sohlplatte und Frostschürze
//! also unter dem Gründungsband.

use sk_model::qto::schedule_storey;
use sk_model::tree::{self, NodeKey, Tab};
use sk_model::{Category, Model};
use std::collections::HashSet;

#[test]
fn bauteile_je_geschoss_wie_schedule_storey() {
    let mut m = Model::with_seed(7);
    for i in 0..3_i32 {
        let b = m.add_building(2);
        let x = f64::from(i) * 20_000.0;
        let pts = [
            sk_math::vec3(x, 0.0, 0.0),
            sk_math::vec3(x, 8000.0, 0.0),
            sk_math::vec3(x + 10_000.0, 8000.0, 0.0),
            sk_math::vec3(x + 10_000.0, 0.0, 0.0),
        ];
        m.build_from_polygon(b, &pts).unwrap();
    }
    // Die Gründung hängt um: mindestens ein Bauteil steht nicht unter
    // seinem eigenen Geschoss
    let um = m
        .elements()
        .iter()
        .filter(|(_, e)| matches!(e.category, Category::GroundSlab | Category::StripFooting))
        .filter(|(_, e)| schedule_storey(&m, e) != e.storey)
        .count();
    assert!(um > 0, "keine Gründung umgehängt");
    let t = tree::build(&m);
    let baum = &t.tabs[Tab::Tree.index()];
    let mut geschosse = 0;
    for n in baum {
        let NodeKey::Storey(g) = n.key else { continue };
        let (sid, _) = m.storeys().iter().find(|(_, s)| s.guid == g).unwrap();
        let soll: Vec<_> = m
            .elements()
            .iter()
            .filter(|(_, e)| schedule_storey(&m, e) == sid)
            .map(|(id, _)| id)
            .collect();
        let ist = &t.members[n.elements.clone()];
        let menge: HashSet<_> = ist.iter().collect();
        assert_eq!(ist.len(), soll.len(), "{}", n.label);
        assert_eq!(menge, soll.iter().collect(), "{}", n.label);
        geschosse += 1;
    }
    assert!(geschosse >= 9, "{geschosse} Geschosse");
}
