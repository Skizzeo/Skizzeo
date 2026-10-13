//! Paneel „Eigenschaften“ eines Raums (Grundriss G3): Name, zuerst die
//! Grundfläche an der fertigen Oberfläche wie am Stempel (Bedienbarkeit
//! 39.4), dann Fläche und Umfang im Rohbau aus der Erkennung, die Wände um
//! ihn; ruhend, wenn ihn keine Fläche trägt (vereint oder ohne
//! Umschließung).

use super::de;
use crate::scene::Scene;
use crate::ui::Props;
use sk_model::ElementId;
use sk_raum::Begrenzung;

pub fn props(
    scene: &Scene,
    id: ElementId,
    mut values: Vec<(&'static str, String)>,
) -> Option<Props> {
    let m = scene.model();
    let e = m.element(id)?;
    values.push(("Name", m.space(id)?.name.clone()));
    // Aus dem Speicher je Hausgruppe wie die Stempel (Review #87)
    let formen = scene.raumgruppen(e.storey);
    let flaeche = formen.flaechen().find(|f| f.form.raum == Some(id));
    let Some(r) = flaeche else {
        values.push(("Zustand", "ruht, keine Fläche".into()));
        return Some(Props {
            values,
            set_label: "Begrenzung",
            layer_set: "–".into(),
            ..Default::default()
        });
    };
    let f = &r.form;
    values.extend([
        // Wie am Stempel, mit derselben Rundung
        (
            "Grundfläche (fertig)",
            format!("{} m²", de(r.grundflaeche / 1e6, 2)),
        ),
        ("Fläche (Rohbau)", format!("{} m²", de(f.flaeche / 1e6, 2))),
        ("Umfang (Rohbau)", format!("{} m", de(f.umfang / 1e3, 2))),
    ]);
    let (raum, mengen) = super::raumbuch::werte(m, &scene.ketten(), f);
    let mengen = values.len() + mengen.start..values.len() + mengen.end;
    values.extend(raum);
    let mut waende: Vec<ElementId> = Vec::new();
    for k in f.umriss.iter().chain(f.loecher.iter().flatten()) {
        let Begrenzung::Wand { wand, .. } = k.quelle;
        if !waende.contains(&wand) {
            waende.push(wand);
        }
    }
    let mut nummern: Vec<&str> = waende
        .iter()
        .filter_map(|w| Some(m.element(*w)?.number.as_str()))
        .collect();
    nummern.sort_by(|a, b| a.len().cmp(&b.len()).then(a.cmp(b)));
    Some(Props {
        values,
        set_label: "Begrenzung",
        layer_set: nummern.join(", "),
        set_umbruch: true,
        mengen,
        ..Default::default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use sk_math::vec3;
    use sk_model::{Category, Model, RefSide};

    /// Haus 10 × 8 m mit Innenwand bei x = 3 m, in Schritten der Szene:
    /// die Räume entstehen beim Abschluss, Rückgängig nimmt sie mit.
    #[test]
    fn raum_im_paneel_und_rueckgaengig() {
        let mut s = Scene::with_model(Model::with_seed(3));
        assert!(s.model().spaces(s.model().defaults().storey).is_empty());
        let st = s.model().defaults().storey;
        assert!(s.edit_model("Haus", |m| {
            let aw = m.defaults().exterior_wall;
            let pts = [
                vec3(0.0, 0.0, 0.0),
                vec3(0.0, 8000.0, 0.0),
                vec3(10000.0, 8000.0, 0.0),
                vec3(10000.0, 0.0, 0.0),
            ];
            m.add_wall_run(&pts, true, RefSide::Left, st, aw, Category::ExteriorWall)
                .is_some()
        }));
        assert_eq!(s.model().spaces(st).len(), 1);
        assert!(s.edit_model("Innenwand", |m| {
            let iw = m.defaults().interior_wall;
            let pts = [vec3(3000.0, 0.0, 0.0), vec3(3000.0, 8000.0, 0.0)];
            m.add_wall_run(&pts, false, RefSide::Center, st, iw, Category::InteriorWall)
                .is_some()
        }));
        let r = s.model().spaces(st);
        assert_eq!(r.len(), 2);
        let p = crate::selection::props(&s, r[1]).unwrap();
        let wert = |k: &str| p.values.iter().find(|v| v.0 == k).map(|v| v.1.clone());
        assert_eq!(wert("Name").as_deref(), Some("Raum 3"), "Raum 2 ist das OG");
        assert!(wert("Fläche (Rohbau)").unwrap().ends_with(" m²"));
        // Zuerst die Grundfläche wie am Stempel, dann der Rohbau (39.4)
        let stelle = |k: &str| p.values.iter().position(|v| v.0 == k).unwrap();
        assert!(stelle("Grundfläche (fertig)") < stelle("Fläche (Rohbau)"));
        let zahl = |k: &str| {
            let w = wert(k).unwrap();
            w.trim_end_matches(" m²")
                .replace(',', ".")
                .parse::<f64>()
                .unwrap()
        };
        let fertig = zahl("Grundfläche (fertig)");
        assert!(
            fertig > 0.0 && fertig < zahl("Fläche (Rohbau)"),
            "ohne Putz"
        );
        // Die Mengen der Flächen stehen in `mengen`, Nutzung und Höhen davor
        // (Review #120 B2)
        let flaechen = [
            "Boden",
            "Estrich",
            "Wand",
            "Wandfliesen",
            "Decke",
            "Sockelleiste",
        ];
        assert!(!p.mengen.is_empty() && stelle("Lichte Höhe") < p.mengen.start);
        assert!(flaechen.contains(&p.values[p.mengen.start].0));
        assert!(p.values[p.mengen.clone()]
            .iter()
            .all(|v| v.0.is_empty() || flaechen.contains(&v.0)));
        assert_eq!(p.set_label, "Begrenzung");
        assert!(p.set_umbruch, "bricht im Paneel um");
        assert!(p.layer_set.contains("IW-001"), "{}", p.layer_set);
        assert!(s.undo());
        assert_eq!(s.model().spaces(st).len(), 1, "mit der Wand zurück");
        let p = crate::selection::props(&s, r[1]);
        assert!(p.is_none(), "den Raum gibt es nicht mehr");
    }
}
