//! Erweiterungsbauteile (.szb) in der App. Heute (E3): ein Projekt mit
//! Exemplaren öffnet, zeichnet, zählt und speichert ohne Fehler; Darstellung,
//! Einsetzen und Verwaltung folgen in E4 bis E7.

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

    /// Projektdatei mit allen fünf Beispielen, je ein Exemplar im EG.
    fn datei() -> String {
        let mut m = Model::new();
        m.add_building(1);
        let eg = m
            .storeys()
            .iter()
            .find(|(_, s)| s.short == "EG" && s.building.is_some())
            .map(|(id, _)| id)
            .unwrap();
        for t in BEISPIELE {
            let d = ExtDef::lesen(t).unwrap();
            let p = ExtPart::new(&d, [1000.0, 1000.0]);
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
}
