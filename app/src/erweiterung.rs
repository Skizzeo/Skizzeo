//! Erweiterungsbauteile (.szb) in der App: ein Projekt mit Exemplaren
//! öffnet, zählt und speichert ohne Fehler (E3); die Exemplare erscheinen
//! in 3D, Grundriss, Schnitt und Ansichten und sind wählbar (E4,
//! [`crate::ext_cache`]). Einsetzen und Verwaltung folgen in E5 bis E7.

#[cfg(test)]
mod tests {
    use crate::scene::Scene;
    use crate::ui::ViewKind;
    use sk_model::erweiterung::{ExtDef, ExtPart};
    use sk_model::{szo, tree, Category, GuidGen, Model};

    const BEISPIELE: [&str; 5] = [
        include_str!("../../crates/sk-szb/beispiele/werk.bodenplatte.szb"),
        include_str!("../../crates/sk-szb/beispiele/werk.stabgelaender.szb"),
        include_str!("../../crates/sk-szb/beispiele/werk.streifenfundament.szb"),
        include_str!("../../crates/sk-szb/beispiele/werk.stuetze.szb"),
        include_str!("../../crates/sk-szb/beispiele/werk.treppe.szb"),
    ];

    /// Projektdatei mit allen fünf Beispielen, je ein Exemplar im EG, 6 m
    /// auseinander.
    fn datei() -> String {
        let mut m = Model::new();
        m.add_building(1);
        let eg = m
            .storeys()
            .iter()
            .find(|(_, s)| s.short == "EG" && s.building.is_some())
            .map(|(id, _)| id)
            .unwrap();
        for (i, t) in BEISPIELE.iter().enumerate() {
            let d = ExtDef::lesen(t).unwrap();
            let p = ExtPart::new(&d, [1000.0 + 6000.0 * i as f64, 1000.0]);
            m.put_ext_def(d).unwrap();
            m.add_ext(eg, p).unwrap();
        }
        szo::write(&m)
    }

    #[test]
    fn projekt_mit_erweiterungen_oeffnet() {
        let text = datei();
        let l = szo::read(&text, GuidGen::with_seed(5)).unwrap();
        assert!(l.hints.is_empty(), "{:?}", l.hints);
        let mut s = Scene::with_model(l.model);
        let ext: Vec<_> = s
            .model()
            .elements()
            .iter()
            .filter(|(_, e)| e.category == Category::Extension)
            .map(|(id, _)| id)
            .collect();
        assert_eq!(ext.len(), 5);
        for v in ViewKind::ALL {
            let _ = s.mesh(v, None, &[]);
        }
        let _ = s.schedule();
        let t = tree::build(s.model());
        let erw = t
            .tab(tree::Tab::Tree)
            .iter()
            .find(|n| n.label.starts_with("Erweiterungen"))
            .expect("Zweig Erweiterungen im Baum");
        assert_eq!(erw.elements.len(), 5);
        for id in &ext {
            let _ = crate::selection::props(&s, *id);
        }
        let d = s.delete_elements(&ext[..1]);
        assert_eq!(d.removed.len(), 1);
        s.undo();
        assert!(s.model().element(ext[0]).is_some());
        s.redo();
        assert!(s.model().element(ext[0]).is_none());
        assert!(s.model().check().is_empty(), "{:?}", s.model().check());
        let _ = szo::write(s.model());
    }

    /// E4: Erweiterungen erscheinen in 3D, im Grundriss ihres Geschosses
    /// und im Schnitt, sind wählbar, folgen dem Ausblenden; ein geändertes
    /// Exemplar wird allein neu gerechnet.
    #[test]
    fn erweiterungen_zeichnen_und_waehlen() {
        let l = szo::read(&datei(), GuidGen::with_seed(5)).unwrap();
        let mut s = Scene::with_model(l.model);
        let ext: Vec<_> = s
            .model()
            .elements()
            .iter()
            .filter(|(_, e)| e.category == Category::Extension)
            .map(|(id, _)| id)
            .collect();
        let flaechen = |s: &mut Scene, v| s.mesh(v, None, &[]).faces.len();
        let voll = flaechen(&mut s, ViewKind::Persp);
        assert!(voll > 100, "{voll}");
        let gebaut = s.ext_builds();
        assert_eq!(gebaut, 5);
        // Grundriss: die Stütze steht im EG und ist geschnitten
        let eg = s.active_storey();
        assert!(flaechen(&mut s, ViewKind::Plan) > 0);
        let stuetze = ext
            .iter()
            .copied()
            .find(|id| s.model().element(*id).unwrap().number == "ST-001")
            .unwrap();
        let p = match &s.model().element(stuetze).unwrap().kind {
            sk_model::ElementKind::Ext(p) => p.clone(),
            _ => unreachable!(),
        };
        // Von oben auf die Stütze: Treffer
        let von_oben = |s: &mut Scene, v| {
            s.pick(
                v,
                None,
                sk_math::vec3(p.at[0] + 10.0, p.at[1] + 10.0, 10_000.0),
                sk_math::vec3(0.0, 0.0, -1.0),
            )
        };
        assert_eq!(von_oben(&mut s, ViewKind::Persp), Some(stuetze));
        assert_eq!(von_oben(&mut s, ViewKind::Plan), Some(stuetze));
        assert_eq!(s.model().element(stuetze).unwrap().storey, eg);
        // Ausblenden der Art: weg aus Netz und Treffer
        let mut v = s.model().visibility().clone();
        v.hidden_cat.insert(Category::Extension);
        s.set_visibility(v);
        assert!(flaechen(&mut s, ViewKind::Persp) < voll);
        assert_eq!(von_oben(&mut s, ViewKind::Persp), None);
        s.set_visibility(Default::default());
        // Ein Exemplar verschieben: nur dieses neu
        let mut q = p.clone();
        q.at[0] += 500.0;
        assert!(s.edit_model("Verschieben", |m| m.set_ext(stuetze, q)));
        assert_eq!(flaechen(&mut s, ViewKind::Persp), voll);
        assert_eq!(s.ext_builds(), gebaut + 1);
        assert_eq!(von_oben(&mut s, ViewKind::Persp), None, "alte Stelle leer");
        s.undo();
        assert_eq!(von_oben(&mut s, ViewKind::Persp), Some(stuetze));
    }
}
