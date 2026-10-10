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

#[cfg(test)]
mod probe_3ch {
    use crate::scene::Scene;
    use crate::ui::ViewKind;
    use sk_math::vec3;
    use sk_model::erweiterung::{ExtDef, ExtPart};
    use sk_model::{Model, RefSide, WallChain};
    use std::time::Instant;

    #[test]
    #[ignore]
    /// Review 3ch: Ziehen einer Wand und Picken mit 0 bis 2000 Exemplaren
    /// (`--ignored --nocapture`); vor dem Patch wuchs beides quadratisch.
    fn probe_3ch() {
        let t = include_str!("../../crates/sk-szb/beispiele/werk.stuetze.szb");
        for n in [0usize, 100, 1000, 2000] {
            let mut m = Model::new();
            m.add_building(1);
            let eg = m
                .storeys()
                .iter()
                .find(|(_, s)| s.short == "EG" && s.building.is_some())
                .map(|(id, _)| id)
                .unwrap();
            let d = ExtDef::lesen(t).unwrap();
            m.put_ext_def(d.clone()).unwrap();
            for i in 0..n {
                let p = ExtPart::new(
                    &d,
                    [(i % 50) as f64 * 1000.0, (i / 50) as f64 * 1000.0 + 20000.0],
                );
                m.add_ext(eg, p).unwrap();
            }
            let mut s = Scene::with_model(m);
            let w = WallChain {
                base: 0.0,
                points: vec![vec3(0.0, 0.0, 0.0), vec3(5000.0, 0.0, 0.0)],
                closed: false,
                ref_side: RefSide::Left,
                layers: Vec::new(),
                height: 3000.0,
                joints: Default::default(),
            };
            s.add_wall(&w).unwrap();
            let run = s.model().runs().ids().next().unwrap();
            let b0 = s.ext_builds();
            s.begin("x");
            let k = 20;
            let t0 = Instant::now();
            for j in 0..k {
                let x = 5000.0 + (j % 2) as f64;
                s.set_run_points(run, &[vec3(0.0, 0.0, 0.0), vec3(x, 0.0, 0.0)]);
            }
            let drag = t0.elapsed().as_secs_f64() * 1000.0 / k as f64;
            s.commit();
            let _ = s.mesh(ViewKind::Persp, None, &[]);
            let t1 = Instant::now();
            for _ in 0..k {
                std::hint::black_box(s.pick(
                    ViewKind::Persp,
                    None,
                    vec3(2500.0, -10000.0, 1000.0),
                    vec3(0.0, 1.0, 0.0),
                ));
            }
            let pick = t1.elapsed().as_secs_f64() * 1000.0 / k as f64;
            println!(
                "E={n} ziehen={drag:.3} ms pick={pick:.3} ms neu_gerechnet={}",
                s.ext_builds() - b0
            );
            // Budget (Briefing QS §5, gemessen 46e1ed0: 0,83 und 0,08 ms)
            if n == 2000 && !cfg!(debug_assertions) {
                assert!(drag <= 2.0, "Ziehen bei {n} Exemplaren {drag:.3} ms > 2 ms");
                assert!(
                    pick <= 0.5,
                    "Picken bei {n} Exemplaren {pick:.3} ms > 0,5 ms"
                );
            }
        }
    }

    /// Review 3cl: Mengenliste mit Erweiterungen (Messung).
    #[test]
    #[ignore]
    fn probe_3cl() {
        let t = include_str!("../../crates/sk-szb/beispiele/werk.stuetze.szb");
        for (n, anders) in [(0usize, false), (500, false), (2000, false), (2000, true)] {
            let mut m = Model::new();
            m.add_building(1);
            let eg = m
                .storeys()
                .iter()
                .find(|(_, s)| s.short == "EG" && s.building.is_some())
                .map(|(id, _)| id)
                .unwrap();
            let d = ExtDef::lesen(t).unwrap();
            m.put_ext_def(d.clone()).unwrap();
            for i in 0..n {
                let mut p = ExtPart::new(
                    &d,
                    [(i % 50) as f64 * 1000.0, (i / 50) as f64 * 1000.0 + 20000.0],
                );
                // jede Stütze mit eigener Breite: keine Rechnung doppelt
                if anders {
                    p.set("b", 200.0 + i as f64 * 0.1);
                }
                m.add_ext(eg, p).unwrap();
            }
            let k = 5;
            let t0 = Instant::now();
            for _ in 0..k {
                std::hint::black_box(sk_model::qto::schedule(&m));
            }
            let ms = t0.elapsed().as_secs_f64() * 1000.0 / k as f64;
            let t1 = Instant::now();
            for _ in 0..k {
                for (id, _) in m.elements().iter() {
                    std::hint::black_box(m.ext_ergebnis(id));
                }
            }
            let rech = t1.elapsed().as_secs_f64() * 1000.0 / k as f64;
            println!("E={n} verschieden={anders} schedule={ms:.2} ms davon_rechnen~{rech:.2} ms");
            // Budget (Briefing QS §5, gemessen 46e1ed0: 3,0 und 19,9 ms); der
            // Fall „verschieden“ rechnet jede Stütze einmal, beim Öffnen
            let budget = if anders { 40.0 } else { 6.0 };
            if !cfg!(debug_assertions) {
                assert!(
                    ms <= budget,
                    "schedule bei {n} Exemplaren {ms:.2} ms > {budget} ms"
                );
            }
        }
    }
}
