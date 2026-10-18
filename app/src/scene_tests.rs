//! Tests der Szene (aus scene.rs, Paket T2).

/// Review H-163d: Lehnt bei Mehrfachwahl ein Zug den dickeren Typ ab
/// (die Gehrung am kurzen Schenkel kippte), wechselt keiner. Es bleibt
/// keine Vorschau offen und kein Schritt im Verlauf.
#[test]
fn typwechsel_abgelehnt_aendert_keinen_zug() {
    use sk_math::vec3;
    use sk_model::{Category, RefSide};
    let mut m = sk_model::Model::with_seed(5);
    let (eg, alt) = (m.defaults().storey, m.defaults().exterior_wall);
    let mut t = m.layer_set(alt).unwrap().clone();
    t.layers[0].thickness = 200.0;
    t.layers[1].thickness = 240.0;
    t.guid = m.new_guid();
    t.code = "AW-44".into();
    let neu = m.add_layer_set(t).unwrap();
    let mut zug = |y: f64, l: f64| {
        let p = [
            vec3(0.0, y, 0.0),
            vec3(6000.0, y, 0.0),
            vec3(6000.0, y + l, 0.0),
        ];
        m.add_wall_run(&p, false, RefSide::Center, eg, alt, Category::ExteriorWall)
            .unwrap()
    };
    let (lang, kurz) = (zug(10000.0, 4000.0), zug(0.0, -250.0));
    let mut s = Scene::with_model(m);
    let vorher = sk_model::szo::write(s.model());
    assert!(!s.preview_run_type(&[lang, kurz], neu));
    assert!(!s.previewing_type());
    s.end_run_type(true);
    assert_eq!(sk_model::szo::write(s.model()), vorher);
    assert!(!s.undo());
    // allein nimmt der lange Zug den Typ an
    assert!(s.preview_run_type(&[lang], neu));
    s.end_run_type(true);
    assert_ne!(sk_model::szo::write(s.model()), vorher);
}

/// Paket PD Abnahme 1–3: Anfangswerte bei „Neu“ ohne Schritt, eine
/// Änderung ist ein Schritt „Projektdaten geändert“, Strg+Z stellt die
/// vorige Nummer her, Unverändertes gibt keinen Schritt.
#[test]
fn projektdaten_neu_und_aendern() {
    let mut s = Scene::with_model(sk_model::Model::new());
    let leer = sk_model::szo::write(s.model());
    assert!(!leer.contains("[projectinfo]"));
    let mut p = s.model.project().clone();
    p.kind = "Neubau Einfamilienhaus".into();
    p.site = "Haus Mustermann".into();
    p.place = "Musterweg 1\n27777 Ganderkesee".into();
    p.number = "01/26".into();
    s.projekt_anfang(p.clone());
    assert_eq!(s.undo_label(), None, "kein Schritt bei „Neu“");
    let text = sk_model::szo::write(s.model());
    assert_eq!(text.matches("[projectinfo]").count(), 1);
    assert!(text.contains("01/26"));
    // Ändern über den Knopf
    let mut q = p.clone();
    q.number = "02/26".into();
    assert!(s.projekt_setzen("Projektdaten geändert", q.clone()));
    assert_eq!(s.undo_label(), Some("Projektdaten geändert"));
    assert!(!s.projekt_setzen("Projektdaten geändert", q));
    assert!(s.undo());
    assert_eq!(s.model.project().number, "01/26");
    assert_eq!(s.undo_label(), None);
}

/// Sonnenstand S1 über die Maske: Breite und Länge bei „Neu“ ohne
/// Schritt; geändert mit den Projektdaten ein Schritt „Projektdaten
/// geändert“; Strg+Z stellt die Datei bytegleich her, Strg+Y die neue.
#[test]
fn projektdaten_mit_lage() {
    use crate::projektdaten::{Antwort, Maske};
    use sk_platform::{Key, Modifiers};
    let mut s = Scene::with_model(sk_model::Model::new());
    let leer = sk_model::szo::write(s.model());
    // „Neu“, Maske leer übernommen: nichts gesetzt, nichts geschrieben
    let mut m = Maske::new(s.model().project(), true, None).mit_ort(s.model().location());
    let Some(Antwort::Uebernehmen(p)) = m.key(Key::Enter, Modifiers::default()) else {
        panic!("Enter übernimmt");
    };
    s.projekt_anfang(*p);
    s.lage_anfang(m.ort());
    assert_eq!(s.undo_label(), None);
    assert_eq!(sk_model::szo::write(s.model()), leer);
    // Nur die Breite geändert: ein Schritt
    let mut m = Maske::new(s.model().project(), false, None).mit_ort(s.model().location());
    m.fokus_auf(8);
    for ch in "52,52".chars() {
        m.text(ch);
    }
    let Some(Antwort::Uebernehmen(p)) = m.key(Key::Enter, Modifiers::default()) else {
        panic!("Enter übernimmt");
    };
    assert!(s.projektdaten_setzen("Projektdaten geändert", *p.clone(), m.ort()));
    assert_eq!(s.undo_label(), Some("Projektdaten geändert"));
    let neu = sk_model::szo::write(s.model());
    assert!(neu.contains("\n[location] lat=52.52\n"), "{neu}");
    assert!(!neu.contains("[projectinfo]"));
    assert!(!s.projektdaten_setzen("Projektdaten geändert", *p, m.ort()));
    assert!(s.undo());
    assert_eq!(s.undo_label(), None, "ein Schritt");
    assert_eq!(sk_model::szo::write(s.model()), leer);
    assert!(s.redo());
    assert_eq!(sk_model::szo::write(s.model()), neu);
    // Bodenkennwerte (Gruppe Baugrund): ein Schritt, in `[soil]`
    // (Regel 113); neue Projekte ohne Kiesschicht
    let mut m = Maske::new(s.model().project(), false, None).mit_ort(s.model().location());
    m.fokus_auf(10);
    for ch in "40".chars() {
        m.text(ch);
    }
    let Some(Antwort::Uebernehmen(p)) = m.key(Key::Enter, Modifiers::default()) else {
        panic!("Enter übernimmt");
    };
    assert!(s.projektdaten_setzen("Projektdaten geändert", *p, m.ort()));
    assert_eq!(s.model().project().soil.oberboden, 400.0);
    let boden = sk_model::szo::write(s.model());
    assert!(
        boden.contains("\n[soil] key=project topsoil=400 capillary_on=0\n"),
        "{boden}"
    );
    assert!(s.undo());
    assert_eq!(sk_model::szo::write(s.model()), neu);
}

/// Review 3ao: Beim Ziehen (offener Schritt) gibt `lv` das gemerkte LV
/// derselben Wahl zurück, statt jedes Bild neu zu ordnen; nach dem
/// Loslassen gilt der neue Stand.
#[test]
fn lv_beim_ziehen_gemerkt() {
    let m = sk_model::szo::read_with(
        include_str!("../../crates/sk-cost/referenz/rh1-standardhaus.szo"),
        sk_model::GuidGen::with_seed(1),
        &sk_cost::lesen::ABSCHNITTE_SZO,
    )
    .unwrap()
    .model;
    let mut s = Scene::with_model(m);
    let u = sk_model::qto::Umfang::projekt();
    let k = s.katalog(None);
    let los = k.lose.iter().find(|l| l.parent.is_none()).unwrap().guid;
    let w = sk_cost::lv::LvWahl {
        los,
        preise: true,
        untertitel: false,
        heute: None,
    };
    let vorher = s.lv(None, &u, &w);
    s.begin("Ziehen");
    let mut p = s.model.project().clone();
    p.client = "Familie Muster".into();
    assert!(s.model.set_project(p));
    let beim_ziehen = s.lv(None, &u, &w);
    assert!(Rc::ptr_eq(&vorher, &beim_ziehen), "beim Ziehen gemerkt");
    s.commit();
    let danach = s.lv(None, &u, &w);
    assert!(!Rc::ptr_eq(&vorher, &danach));
    assert_eq!(danach.kopf.bauherr.as_deref(), Some("Familie Muster"));
}

/// Review 3ai: Der gemerkte Katalog hängt auch an den Baustoffen des
/// Modells (R73-W). Umbenennen von „Stahlbeton“ in einer Altdatei nimmt
/// die Werkspreise weg; Katalog und Kostenblatt folgen sofort.
#[test]
fn katalog_folgt_umbenanntem_baustoff() {
    let text = include_str!("abnahme_p5.szo");
    let m = sk_model::szo::read_with(
        text,
        sk_model::GuidGen::with_seed(1),
        &sk_cost::lesen::ABSCHNITTE_SZO,
    )
    .unwrap()
    .model;
    let mut s = Scene::with_model(m);
    let u = sk_model::qto::Umfang::projekt();
    let vorher = s.kostenblatt(None, &u).netto;
    let (id, mut x) = s
        .model()
        .materials()
        .iter()
        .find(|(_, x)| x.name == "Stahlbeton")
        .map(|(id, x)| (id, x.clone()))
        .unwrap();
    x.name = "Beton alt".into();
    assert!(s.edit_model("Name", |m| m.set_material(id, x)));
    let frisch = sk_cost::lesen::katalog(s.model(), None);
    assert_eq!(s.katalog(None).befunde, frisch.befunde);
    let sched = sk_model::qto::schedule(s.model());
    let soll = sk_cost::lesen::kosten(s.model(), &sched, &frisch, &u).netto;
    assert_ne!(soll, vorher);
    assert_eq!(s.kostenblatt(None, &u).netto, soll);
}

use super::*;
use sk_model::{Pen, RefSide};

/// Gelände ziehen (Review): nur die Züge mit Sohlplatte neu, und doch
/// dasselbe Bild und dieselben Mengen wie nach einem vollen Neuaufbau.
#[test]
fn gelaende_ziehen_wie_neuaufbau() {
    let mut m = Model::with_seed(3);
    for (ox, n) in [(0.0, 1u8), (20000.0, 2)] {
        let b = m.add_building(n);
        let pts = [
            vec3(ox, 0.0, 0.0),
            vec3(ox, 8000.0, 0.0),
            vec3(ox + 10000.0, 8000.0, 0.0),
            vec3(ox + 10000.0, 0.0, 0.0),
        ];
        m.build_from_polygon(b, &pts).unwrap();
    }
    let mut s = Scene::with_model(m);
    let bild = |s: &mut Scene| {
        let m = s.mesh(ViewKind::Front, None, &[]);
        (m.faces.clone(), m.edges.clone())
    };
    s.begin("Gelände ziehen");
    for z in [-300.0, 250.0, -900.0] {
        s.drag_terrain(z);
    }
    s.commit();
    let mut neu = Scene::with_model(s.model().clone());
    assert_eq!(s.model().terrain_z(), -900.0);
    assert!(bild(&mut s) == bild(&mut neu), "Bild wie nach Neuaufbau");
    assert_eq!(s.bounds(), neu.bounds());
}

/// Gelände Thema 1: Kanten werden an OK Gelände geteilt, nicht bei
/// ±0,00; endet eine Kante auf dem Gelände, bleibt sie ganz.
#[test]
fn kanten_teilen_am_gelaende() {
    let mut s = Solid::default();
    s.edge(vec3(0.0, 0.0, -1000.0), vec3(0.0, 0.0, 500.0));
    s.edge(vec3(1.0, 0.0, -400.0), vec3(1.0, 0.0, 500.0));
    let mut m = MeshData::default();
    mesh_into(&mut m, &s, Boden::eben(-400.0));
    let zs: Vec<_> = m.edges.iter().map(|(e, _)| (e[0][2], e[1][2])).collect();
    assert_eq!(zs, [(-1000.0, -400.0), (-400.0, 500.0), (-400.0, 500.0)]);
}

/// Paket 3 (Vor-Patch): Jede Fläche und Kante des Prüfhauses mit AW-49
/// und Dachterrasse trägt ein Bauteil; Wandflächen eine Schicht des
/// Wandtyps, Decke und Terrasse eine ihres Aufbaus, alles andere
/// [`sk_model::NO_LAYER`]. Gilt in 3D, im Grundriss (jede Art), im
/// Schnitt A und B und an den Hintergrundkanten; jede Schicht der Wand
/// hat Flächen.
#[test]
fn jede_flaeche_und_kante_traegt_bauteil_und_schicht() {
    use sk_model::{Category, NO_LAYER};
    let mut s = Scene::with_model(Model::with_seed(75));
    let pts = [
        vec3(0.0, 0.0, 0.0),
        vec3(0.0, 8000.0, 0.0),
        vec3(10000.0, 8000.0, 0.0),
        vec3(10000.0, 0.0, 0.0),
    ];
    let mut eg = None;
    assert!(s.edit_model("Gebäude erstellt", |m| {
        let b = m.add_building(2);
        eg = m.build_from_polygon(b, &pts);
        eg.is_some()
    }));
    let eg = eg.unwrap();
    let og = s.model().runs_above(eg)[0];
    let aw49 = s.model().type_by_guid(sk_model::CAVITY_TYPE_GUID).unwrap();
    let nord = s.model().wall_at(og, 1).unwrap();
    assert!(s.edit_model("Prüfhaus", |m| {
        m.set_run_type(eg, aw49)
            && m.set_run_type(og, aw49)
            && m.set_linked(nord, false)
            && m.set_offset(nord, -1500.0).is_some()
    }));
    let de = s.model().floor_of(eg).unwrap();
    assert!(s.model().terrace_of(de).is_some(), "Dachterrasse");
    s.rebuild_dirty(false);

    let model = s.model.clone();
    let check = |run: RunId, sol: &Solid, what: &str, per_layer: &mut Vec<usize>| {
        let layers = |id: ElementId| model.element_layers(id).len();
        let ok_layer = |id: ElementId, layer: u8| {
            let e = model.element(id).unwrap();
            match e.category {
                Category::ExteriorWall | Category::InteriorWall => (layer as usize) < layers(id),
                Category::Floor
                | Category::GroundSlab
                | Category::StripFooting
                | Category::PerimeterInsulation => (layer as usize) < layers(id).max(1),
                Category::RoofTerrace => (layer as usize) < layers(id),
                _ => layer == NO_LAYER,
            }
        };
        for t in &sol.triangles {
            let id = model
                .part_of(run, t.elem)
                .unwrap_or_else(|| panic!("{what}: Fläche ohne Bauteil (Teil {})", t.elem));
            assert!(
                ok_layer(id, t.layer),
                "{what}: Fläche {} Schicht {}",
                t.elem,
                t.layer
            );
            let e = model.element(id).unwrap();
            if e.category == Category::ExteriorWall {
                per_layer[t.layer as usize] += 1;
            }
        }
        for e in &sol.edges {
            let id = model
                .part_of(run, e.elem)
                .unwrap_or_else(|| panic!("{what}: Kante ohne Bauteil (Teil {})", e.elem));
            assert!(
                ok_layer(id, e.layer),
                "{what}: Kante {} Schicht {}",
                e.elem,
                e.layer
            );
        }
    };
    let nlayers = model.layer_set(aw49).unwrap().layers.len();
    let planes = [
        (vec3(5000.0, 4000.0, 0.0), vec3(0.0, 1.0, 0.0)),
        (vec3(5000.0, 4000.0, 0.0), vec3(1.0, 0.0, 0.0)),
        (vec3(5000.0, 7300.0, 0.0), vec3(0.0, -1.0, 0.0)),
    ];
    for run in [eg, og] {
        let i = s
            .cache
            .iter()
            .position(|c| c.as_ref().is_some_and(|c| c.id == run));
        let c = s.cache[i.unwrap()].as_mut().unwrap();
        let mut per_layer = vec![0; nlayers];
        let solid = c.solid.clone();
        check(run, &solid, "3D", &mut per_layer);
        for (k, n) in per_layer.iter().enumerate() {
            let air = model.layer_set(aw49).unwrap().layers[k].function
                == sk_model::LayerFunction::AirGap;
            assert!(air || *n > 0, "Schicht {k} ohne Flächen");
        }
        for cut in [1000.0, 2900.0, 3900.0, 6000.0, -300.0] {
            for mode in [PlanMode::Cut, PlanMode::Lower, PlanMode::Foundation] {
                let p = c.plan_solid(cut, mode);
                check(
                    run,
                    &p,
                    &format!("Grundriss {cut} {mode:?}"),
                    &mut vec![0; nlayers],
                );
            }
            let under = Solid {
                edges: c.under_edges(cut).to_vec(),
                ..Solid::default()
            };
            check(run, &under, "Hintergrund", &mut vec![0; nlayers]);
        }
        for pl in planes {
            let sec = c
                .view_solid(ViewKind::Section, Some(pl), 0.0, PlanMode::Cut)
                .unwrap()
                .clone();
            check(run, &sec, &format!("Schnitt {pl:?}"), &mut vec![0; nlayers]);
            // Durch die Terrasse: Belag 0 und Dämmung 1
            if run == eg && pl.1.x == 1.0 {
                let dt = model.terrace_of(de).unwrap();
                let mut got: Vec<u8> = sec
                    .triangles
                    .iter()
                    .filter(|t| model.part_of(run, t.elem) == Some(dt))
                    .map(|t| t.layer)
                    .collect();
                got.sort();
                got.dedup();
                assert_eq!(got, [0, 1], "Schichten der Terrasse im Schnitt");
            }
        }
    }
}

#[test]
fn buendig_an_zielwand_ein_schritt() {
    let mut s = Scene::with_model(Model::with_seed(74));
    let pts = [
        vec3(0.0, 0.0, 0.0),
        vec3(0.0, 8000.0, 0.0),
        vec3(10000.0, 8000.0, 0.0),
        vec3(10000.0, 0.0, 0.0),
    ];
    let mut eg = None;
    assert!(s.edit_model("Gebäude erstellt", |m| {
        let b = m.add_building(2);
        eg = m.build_from_polygon(b, &pts);
        eg.is_some()
    }));
    let eg = eg.unwrap();
    let og = s.model().runs_above(eg)[0];
    let (w, e) = (
        s.model().wall_at(og, 1).unwrap(),
        s.model().wall_at(eg, 1).unwrap(),
    );
    assert!(s.set_linked(w, false));
    assert!(s.type_offset(w, 300.0));
    let y = |s: &Scene, r| s.model().run(r).unwrap().points[1].y;
    // Ziel OG: die EG-Wand rückt nach außen, ein Schritt
    assert_eq!(s.flush_to(e, w), Ok(true));
    assert_eq!((y(&s, eg), y(&s, og)), (8300.0, 8300.0));
    assert_eq!(s.model().stack_offset(w), Some((0.0, true)));
    assert_eq!(s.flush_to(e, w), Ok(false));
    assert!(s.model().check().is_empty(), "{:?}", s.model().check());
    assert!(s.undo());
    assert_eq!((y(&s, eg), y(&s, og)), (8000.0, 8300.0));
    assert_eq!(s.model().stack_offset(w), Some((300.0, false)));
    // Ziel EG: das OG rückt wie bisher
    assert_eq!(s.flush_to(w, e), Ok(true));
    assert_eq!((y(&s, eg), y(&s, og)), (8000.0, 8000.0));
    // Keine Partner: Grund, kein Schritt
    let label = s.undo_label();
    assert_eq!(
        s.flush_to(e, s.model().wall_at(og, 0).unwrap()),
        Err(sk_model::FlushError::NotPartners)
    );
    assert_eq!(s.undo_label(), label);
}

#[test]
fn blech_vorgabe_gilt_als_verwendet_und_terrasse_hat_kante() {
    let mut s = Scene::with_model(Model::with_seed(75));
    let pts = [
        vec3(0.0, 0.0, 0.0),
        vec3(0.0, 8000.0, 0.0),
        vec3(10000.0, 8000.0, 0.0),
        vec3(10000.0, 0.0, 0.0),
    ];
    let mut eg = None;
    assert!(s.edit_model("Gebäude erstellt", |m| {
        let b = m.add_building(2);
        eg = m.build_from_polygon(b, &pts);
        eg.is_some()
    }));
    let og = s.model().runs_above(eg.unwrap())[0];
    let w = s.model().wall_at(og, 1).unwrap();
    assert!(s.set_linked(w, false));
    assert!(s.type_offset(w, -1500.0));
    let m = s.model();
    let floor = m
        .elements()
        .iter()
        .find_map(|(_, e)| match e.kind {
            sk_model::ElementKind::Coping { floor } => Some(floor),
            _ => None,
        })
        .expect("Attikablech");
    let mat = m.coping_material(floor).expect("Vorgabe");
    assert!(m.material_used(mat), "Vorgabe des Blechs");
    assert!(s.set_active_storey(m.run(og).unwrap().storey));
    let (marks, blocked) = s.terrace_marks();
    assert_eq!(marks.len(), 1);
    let (a, b, n) = marks[0].edge.expect("Blechkante");
    assert!(
        (a.y - b.y).abs() < 1.0 && a.y > 8000.0 && n.y > 0.9,
        "{a:?} {b:?} {n:?}"
    );
    assert!(!blocked.is_empty());
}

/// S11: Kanten, die das Gelände kreuzen, teilt der Netzbau an z = 0;
/// die feine Kantenart des Shaders ist die des Modells.
#[test]
fn kanten_an_gelaende_geteilt() {
    assert_eq!(sk_model::edge_kind::FINE, sk_render::EDGE_FINE);
    let mut s = Solid::default();
    s.edge(vec3(0.0, 0.0, -800.0), vec3(0.0, 0.0, 2000.0));
    s.edge(vec3(0.0, 0.0, 0.0), vec3(1000.0, 0.0, 0.0));
    s.edge(vec3(0.0, 0.0, -800.0), vec3(1000.0, 0.0, -800.0));
    // Endet eine Kante auf ±0,00, bleibt sie ganz (Review 3cg)
    s.edge(vec3(0.0, 0.0, -800.0), vec3(0.0, 0.0, 0.0));
    s.edge(vec3(0.0, 0.0, 0.0), vec3(0.0, 0.0, -800.0));
    s.edge(vec3(0.0, 0.0, -0.2), vec3(0.0, 0.0, 3000.0));
    let m = mesh_of(&s);
    let z: Vec<[f32; 2]> = m.edges.iter().map(|(p, _)| [p[0][2], p[1][2]]).collect();
    assert_eq!(
        z,
        [
            [-800.0, 0.0],
            [0.0, 2000.0],
            [0.0, 0.0],
            [-800.0, -800.0],
            [-800.0, 0.0],
            [0.0, -800.0],
            [-0.2, 3000.0]
        ]
    );
}

fn rechteck(x: f64) -> WallChain {
    WallChain {
        base: 0.0,
        points: vec![
            vec3(x, 0.0, 0.0),
            vec3(x, 4000.0, 0.0),
            vec3(x + 5000.0, 4000.0, 0.0),
            vec3(x + 5000.0, 0.0, 0.0),
        ],
        closed: true,
        ref_side: RefSide::Left,
        layers: Vec::new(),
        height: 3500.0,
        joints: Default::default(),
    }
}

fn same(a: &MeshData, b: &MeshData) -> bool {
    a.faces == b.faces && a.edges == b.edges
}

/// U4: OK EG eines Gebäudes ziehen rechnet nur dessen Züge neu; das
/// andere Gebäude (eigene Geschosse) bleibt, wie es ist.
#[test]
fn ebene_ziehen_rechnet_nur_das_eigene_gebaeude() {
    let mut s = Scene::with_model(Model::with_seed(1));
    let mut eg = Vec::new();
    for x in [0.0, 20000.0] {
        s.edit_model("Gebäude erstellt", |m| {
            m.add_building(2);
            true
        });
        let b = s.model().buildings().ids().last().unwrap();
        let g = s.model().ground_of(Some(b)).unwrap();
        s.set_active_storey(g);
        s.add_wall(&rechteck(x)).unwrap();
        eg.push(g);
    }
    let of = |s: &Scene, g| -> Vec<RunId> {
        let b = s.model().building_of(g);
        s.model()
            .runs()
            .iter()
            .filter(|(_, r)| s.model().building_of(r.storey) == b)
            .map(|(id, _)| id)
            .collect()
    };
    let (eigen, fremd) = (of(&s, eg[0]), of(&s, eg[1]));
    assert_eq!((eigen.len(), fremd.len()), (2, 2), "EG und OG je Gebäude");
    let count = |s: &Scene, v: &[RunId]| v.iter().map(|r| s.build_count(*r)).collect::<Vec<_>>();
    let (vorher_e, vorher_f) = (count(&s, &eigen), count(&s, &fremd));
    s.begin("Geschoss ziehen");
    s.drag_storey_top(eg[0], 2900.0);
    s.commit();
    let nachher_e = count(&s, &eigen);
    assert!(
        vorher_e.iter().zip(&nachher_e).all(|(a, b)| b > a),
        "{vorher_e:?} → {nachher_e:?}"
    );
    assert_eq!(count(&s, &fremd), vorher_f, "anderes Gebäude unberührt");
    // Ergebnis gleich einer vollen Neuberechnung
    let live = s.mesh(ViewKind::Persp, None, &[]);
    let mut frisch = Scene::with_model(s.model().clone());
    assert!(same(&live, &frisch.mesh(ViewKind::Persp, None, &[])));
}

#[test]
fn nur_der_geaenderte_zug_wird_neu_berechnet() {
    let mut s = Scene::with_model(Model::with_seed(1));
    let a = s.add_wall(&rechteck(0.0)).unwrap();
    let b = s.add_wall(&rechteck(10000.0)).unwrap();
    assert_eq!((s.build_count(a), s.build_count(b)), (1, 1));
    s.begin("Wand verschieben");
    // Oberes Segment 50 cm nach außen
    let moved = s.chain(a).unwrap().with_segment_moved(1, -500.0).unwrap();
    s.set_run_points(a, &moved.points);
    s.commit();
    assert_eq!((s.build_count(a), s.build_count(b)), (2, 1));
    // Gesamtquader folgt der Änderung
    assert!((s.bounds().unwrap().1.y - 4500.0).abs() < 1e-6);
    // Rückgängig rechnet nur den betroffenen Zug neu, und alles stimmt mit
    // einem vollständigen Neuaufbau überein
    assert!(s.undo());
    assert_eq!((s.build_count(a), s.build_count(b)), (3, 1));
    assert!((s.bounds().unwrap().1.y - 4000.0).abs() < 1e-6);
    let mut full = Scene::with_model(s.model().clone());
    for v in [ViewKind::Persp, ViewKind::Plan] {
        assert!(same(&s.mesh(v, None, &[]), &full.mesh(v, None, &[])));
    }
    let pl = Some((vec3(0.0, 2000.0, 0.0), vec3(0.0, -1.0, 0.0)));
    assert!(same(
        &s.mesh(ViewKind::Section, pl, &[]),
        &full.mesh(ViewKind::Section, pl, &[])
    ));
    assert!(s.model().check().is_empty());
}

#[test]
fn nach_rueckgaengig_bekommt_eine_neue_wand_eine_neue_kennung() {
    let mut s = Scene::with_model(Model::with_seed(4));
    let a = s.add_wall(&rechteck(0.0)).unwrap();
    let wa = s.model().wall_at(a, 0).unwrap();
    assert!(s.undo());
    let b = s.add_wall(&rechteck(0.0)).unwrap();
    assert_ne!(a, b);
    // Die alte Wandkennung zeigt nicht auf die neue Wand
    assert!(s.model().element(wa).is_none());
    assert!(s.model().check().is_empty());
    // Wiederholen geht nach einer neuen Wand nicht mehr, Rückgängig schon
    assert!(!s.redo());
    assert!(s.undo());
    assert!(s.model().run(b).is_none());
}

/// E3, Test 4: Stiftfarben ändern nur die Zeichentabelle; kein Netz wird
/// neu gebaut, und das Netz bleibt Byte für Byte gleich.
#[test]
fn stiftfarbe_aendern_baut_kein_netz_neu() {
    let mut s = Scene::with_model(Model::with_seed(8));
    let run = s.add_wall(&rechteck(0.0)).unwrap();
    let rev = s.table().rev;
    let builds = s.build_count(run);
    let pen = |s: &Scene, n| {
        s.model()
            .attr()
            .pens()
            .iter()
            .find(|(_, p)| p.number == n)
            .map(|(id, p)| (id, p.clone()))
            .unwrap()
    };
    let before = s.mesh(ViewKind::Plan, None, &[]);
    let looks = s.table().looks(1.0);
    // Stift 3 (kräftig, Schnittkante) und Stift 5 (Grund) auf Rot
    for n in [3, 5] {
        let (id, p) = pen(&s, n);
        assert!(s.set_pen(
            id,
            Pen {
                color: [255, 0, 0],
                ..p
            }
        ));
    }
    assert!(s.table().rev > rev);
    assert_eq!(s.build_count(run), builds, "kein Wandzug neu gerechnet");
    let after = s.mesh(ViewKind::Plan, None, &[]);
    assert_eq!(before.faces, after.faces);
    assert_eq!(before.edges, after.edges);
    let red = [1.0, 0.0, 0.0];
    let now = s.table().looks(1.0);
    assert_eq!(now.drawing.color[sk_model::edge_kind::CUT as usize], red);
    assert_eq!(
        now.drawing.color[sk_model::edge_kind::VIEW as usize],
        [0.0; 3]
    );
    let key = (after.faces[0][6] as usize) & 0x7FFF;
    assert_eq!(now.texels[2 * now.keys + key][..3], red, "Grund rot");
    // Rückgängig: alte Tabelle, wieder ohne Neuaufbau
    assert!(s.undo());
    assert!(s.undo());
    assert_eq!(s.table().looks(1.0), looks);
    assert_eq!(s.build_count(run), builds);
}

#[test]
fn ruhendes_und_live_netz_ergeben_das_ganze() {
    let mut s = Scene::with_model(Model::with_seed(2));
    let a = s.add_wall(&rechteck(0.0)).unwrap();
    s.add_wall(&rechteck(10000.0)).unwrap();
    let whole = s.mesh(ViewKind::Persp, None, &[]);
    let rest = s.mesh(ViewKind::Persp, None, &[a]);
    let live = s.mesh_runs(ViewKind::Persp, None, &[a]);
    assert_eq!(whole.faces.len(), rest.faces.len() + live.faces.len());
    assert_eq!(whole.edges.len(), rest.edges.len() + live.edges.len());
}

/// Paket 3 §2: Ausblenden gleitet in `anim_ms` von deckend nach
/// durchsichtig, Isolieren macht den Rest blass; ein Klick springt ans
/// Ende. Während des Übergangs ist nichts Ausgehendes anklickbar.
#[test]
fn sichtbarkeit_gleitet() {
    let mut s = Scene::with_model(Model::with_seed(4));
    s.add_wall(&rechteck(0.0)).unwrap();
    let ms = s.theme.size.anim_ms as u64;
    let w = s.model().elements().iter().next().unwrap().0;
    let g = s.model().element(w).unwrap().guid;
    s.set_now(1000);
    let mut v = s.model().visibility().clone();
    v.hidden.insert(g);
    assert!(s.fade_visibility(v.clone()));
    assert!(!s.fade_visibility(v), "gleich: nichts neu");
    assert!(s.vis_animating() && !s.visible(w));
    assert_eq!(s.ghost_alpha(false), 1.0);
    assert!(!s.ghost_mesh(ViewKind::Persp, None, &[]).faces.is_empty());
    assert!(!s.vis_tick(1000 + ms / 2));
    assert!(s.ghost_alpha(false) > 0.0 && s.ghost_alpha(false) < 1.0);
    assert!(s.vis_tick(1000 + ms));
    assert_eq!(s.ghost_alpha(false), 0.0);
    assert!(s.ghost_mesh(ViewKind::Persp, None, &[]).faces.is_empty());

    let mut v = s.model().visibility().clone();
    v.hidden.clear();
    v.isolate = Some(Isolate::Elements([g].into_iter().collect()));
    assert!(s.fade_visibility(v));
    assert!(s.isolating().is_some());
    assert!(s.skip_vis_animation());
    let a = s.theme.size.ghost_alpha_3d;
    assert_eq!(s.ghost_alpha(false), a);
    assert_eq!(s.ghost_alpha(true), s.theme.size.ghost_alpha_paper);
    assert!(s.pickable(w));
}

/// Review 3h G2 und Befund 2: Das Teilen in deckend und blass hängt an
/// der Generation des Körpers und den Masken, nicht an der Revision.
/// Eine Änderung an einem Haus teilt nur dessen Züge neu; das andere
/// behält seine geteilten Körper, ebenso ein Zug, der gleich neu gebaut
/// wurde. Je Zug und Ansicht höchstens ein Eintrag.
#[test]
fn teilen_nur_neu_gebauter_zuege() {
    let mut s = Scene::with_model(Model::with_seed(5));
    let a = s.add_wall(&rechteck(0.0)).unwrap();
    let b = s.add_wall(&rechteck(10000.0)).unwrap();
    let mut v = s.model().visibility().clone();
    v.hidden_cat.insert(Category::Floor);
    assert!(s.set_visibility(v));
    s.mesh(ViewKind::Persp, None, &[]);
    let geteilt = |s: &Scene, r: RunId| {
        s.cached(r)
            .and_then(|c| c.splits.borrow().first().map(|x| x.2.clone()))
            .expect("geteilt")
    };
    let (va, vb) = (geteilt(&s, a), geteilt(&s, b));
    // Neue Revision ohne neuen Körper: nichts neu geteilt
    let w = s.model().wall_at(a, 0).unwrap();
    let rev = s.model().revision();
    s.begin("Merkmal");
    let text = Some(sk_model::PropValue::Text("neu".into()));
    assert!(s.model.set_prop(w, "Bemerkung", text));
    s.commit();
    assert!(s.model().revision() > rev);
    s.mesh(ViewKind::Persp, None, &[]);
    assert!(Rc::ptr_eq(&va, &geteilt(&s, a)), "Revision allein");
    // Gleich neu gebaut (ein Nachbar hat sich geändert): dieselbe
    // Generation, nichts neu geteilt
    s.mark(a);
    s.rebuild_dirty(false);
    s.mesh(ViewKind::Persp, None, &[]);
    assert!(Rc::ptr_eq(&va, &geteilt(&s, a)), "gleicher Körper");
    // Körper geändert: nur dieser Zug wird neu geteilt
    let moved = s.chain(a).unwrap().with_segment_moved(0, -100.0).unwrap();
    s.begin("Wand verschieben");
    s.set_run_points(a, &moved.points);
    s.commit();
    s.mesh(ViewKind::Persp, None, &[]);
    assert!(
        !Rc::ptr_eq(&va, &geteilt(&s, a)),
        "neuer Körper, neu geteilt"
    );
    assert!(
        Rc::ptr_eq(&vb, &geteilt(&s, b)),
        "anderer Zug aus dem Speicher"
    );
    // Andere Masken: neu geteilt
    let mut v = s.model().visibility().clone();
    v.hidden_cat.clear();
    v.hidden_cat.insert(Category::ExteriorWall);
    assert!(s.set_visibility(v));
    s.mesh(ViewKind::Persp, None, &[]);
    assert!(!Rc::ptr_eq(&vb, &geteilt(&s, b)));
    assert_eq!(s.cached(b).unwrap().splits.borrow().len(), 1, "ein Eintrag");
}

#[test]
fn strahl_liefert_die_wand() {
    let mut s = Scene::with_model(Model::with_seed(3));
    s.add_wall(&rechteck(10000.0)).unwrap();
    let a = s.add_wall(&rechteck(0.0)).unwrap();
    // Segment 2 läuft bei x = 5000 von y = 4000 nach 0; von außen (+x) treffen
    let hit = s.raycast(vec3(9000.0, 2000.0, 1000.0), vec3(-1.0, 0.0, 0.0));
    let (t, wall) = hit.unwrap();
    assert!((t - 4000.0).abs() < 1e-6, "{t}");
    assert_eq!(Some(wall), s.model().wall_at(a, 2));
    // AW-001…008: erstes Haus mit OG, AW-009…012: EG des zweiten
    assert_eq!(s.model().element(wall).unwrap().number, "AW-011");
    // Am Modell vorbei: kein Treffer
    assert!(s
        .raycast(vec3(0.0, -9000.0, 5000.0), vec3(0.0, 0.0, 1.0))
        .is_none());
}

/// Ein neuer Akzent ändert die Zeichnung nicht: Die Tabelle übernimmt den
/// Schema-Stand, Netze müssen nicht neu entstehen. Andere Bildpunkte je mm
/// ändern die Strichbreiten.
#[test]
fn farbschema_aendern() {
    let mut s = Scene::with_model(Model::with_seed(8));
    s.add_wall(&rechteck(0.0)).unwrap();
    let mut theme = Theme::dark();
    assert!(!s.set_theme(&theme), "gleiches Schema");
    theme.set_accent(sk_paint::Rgba::rgb(40, 120, 220));
    assert!(!s.set_theme(&theme), "Akzent ohne neue Netze");
    assert_eq!(s.table().theme_rev, theme.rev);
    let width = s.table().edge_width(true, sk_model::edge_kind::CUT);
    theme.px_per_mm = 6.0;
    theme.rev += 1;
    assert!(s.set_theme(&theme), "Strichbreiten ändern sich");
    assert!(s.table().edge_width(true, sk_model::edge_kind::CUT) > width);
}

/// KA-0d, Abnahme 11–13: `kosten` ist ein Schritt mit Bezeichnung,
/// `kosten_folge` einer oder nichts, `kosten_vorschau` ändert nichts.
#[test]
fn kosten_ein_schritt_vorschau_ohne_aenderung() {
    use sk_cost::{Dez, Herkunft, HerkunftArt, Op};
    let lohn = |w: i64| Op::FirmenwertSetzen {
        schluessel: "wage".into(),
        wert: Dez::ganz(w),
    };
    let h = Herkunft::neu(HerkunftArt::Manual, "2026-10-08", "09:00");
    let mut s = Scene::new();
    let leer = sk_model::szo::write(s.model());
    // Vorschau: alles bleibt
    let (rev, ext) = (s.model().revision(), s.model().ext_revision());
    let p = s
        .kosten_vorschau(None, &[lohn(65)], &sk_model::qto::Umfang::projekt())
        .unwrap();
    assert!(p.aenderungen.iter().any(|a| a.satz.kennung == "wage"));
    assert_eq!(p.netto, Some((sk_cost::Cent(0), sk_cost::Cent(0))));
    assert_eq!(p.katalog.werte.lohn, Dez::ganz(65));
    assert_eq!((s.model().revision(), s.model().ext_revision()), (rev, ext));
    assert_eq!(s.undo_label(), None);
    // ein Schritt mit Bezeichnung; Rückgängig gibt die leere Datei zurück
    s.kosten(None, &h, lohn(65)).unwrap();
    assert_eq!(s.undo_label(), Some("Lohn 65,00 €/h"));
    assert_eq!(
        sk_cost::lesen::katalog(s.model(), None).werte.lohn,
        Dez::ganz(65)
    );
    assert!(s.undo());
    assert_eq!(s.undo_label(), None);
    assert_eq!(sk_model::szo::write(s.model()), leer);
    // Fehler: nichts, kein Schritt
    let rev = s.model().revision();
    let b = s.kosten(None, &h, lohn(1_000_000)).unwrap_err();
    assert!(b.iter().any(|b| b.regel == 93), "{b:?}");
    assert_eq!(s.model().revision(), rev);
    assert_eq!(s.undo_label(), None);
    // Folge: drei sind ein Schritt; scheitert die dritte, bleibt nichts
    let b = s
        .kosten_folge("Löhne", None, &h, &[lohn(61), lohn(62), lohn(1_000_000)])
        .unwrap_err();
    assert!(!b.is_empty());
    assert_eq!(s.undo_label(), None);
    assert_eq!(sk_model::szo::write(s.model()), leer);
    s.kosten_folge("Löhne", None, &h, &[lohn(61), lohn(62), lohn(63)])
        .unwrap();
    assert_eq!(s.undo_label(), Some("Löhne"));
    assert_eq!(
        sk_cost::lesen::katalog(s.model(), None).werte.lohn,
        Dez::ganz(63)
    );
    assert!(s.undo());
    assert_eq!(sk_model::szo::write(s.model()), leer);
}
