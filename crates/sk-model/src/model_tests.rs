
use super::*;
use sk_math::vec3;

fn rechteck(m: &mut Model) -> RunId {
    let pts = [
        vec3(0.0, 0.0, 0.0),
        vec3(0.0, 6000.0, 0.0),
        vec3(8000.0, 6000.0, 0.0),
        vec3(8000.0, 0.0, 0.0),
    ];
    let set = m.defaults().exterior_wall;
    let eg = m.eg_at(3500.0);
    m.add_wall_run(&pts, true, RefSide::Left, eg, set, Category::ExteriorWall)
        .unwrap()
}

/// Gebäude mit `n` Geschossen aus dem Rechteck 10 × 8 m (B12).
fn gebaeude(n: u8) -> (Model, Vec<RunId>) {
    let mut m = Model::with_seed(12);
    let b = m.add_building(n);
    let pts = [
        vec3(0.0, 0.0, 0.0),
        vec3(0.0, 8000.0, 0.0),
        vec3(10000.0, 8000.0, 0.0),
        vec3(10000.0, 0.0, 0.0),
    ];
    let eg = m.build_from_polygon(b, &pts).unwrap();
    let mut runs = vec![eg];
    while let Some(up) = runs.last().and_then(|r| m.runs_above(*r).first().copied()) {
        runs.push(up);
    }
    (m, runs)
}

fn nummern(m: &Model, prefix: &str) -> Vec<String> {
    let mut v: Vec<String> = m
        .elements()
        .iter()
        .map(|(_, e)| e.number.clone())
        .filter(|n| n.starts_with(prefix))
        .collect();
    v.sort();
    v
}

/// U3: Beim Verschieben der Außenwand kommen nur die Innenwände mit, die
/// unter die Decke kommen oder sie verlassen; die anderen behalten ihr
/// Deckenband.
#[test]
fn verschieben_nimmt_nur_innenwaende_mit_neuem_deckenband() {
    let (mut m, runs) = gebaeude(2);
    let eg = runs[0];
    let storey = m.run(eg).unwrap().storey;
    let set = m.defaults().interior_wall;
    let innen = |m: &mut Model, x: f64| {
        let pts = [vec3(x, 3000.0, 0.0), vec3(x, 5000.0, 0.0)];
        m.add_wall_run(
            &pts,
            false,
            RefSide::Center,
            storey,
            set,
            Category::InteriorWall,
        )
        .unwrap()
    };
    let nah = innen(&mut m, 1000.0);
    let fern = innen(&mut m, 6000.0);
    assert_eq!(m.runs_under_floor(eg), [nah, fern]);
    m.begin("Wand verschieben");
    let pts = [
        vec3(2000.0, 0.0, 0.0),
        vec3(2000.0, 8000.0, 0.0),
        vec3(10000.0, 8000.0, 0.0),
        vec3(10000.0, 0.0, 0.0),
    ];
    let out = m.set_run_points(eg, &pts).unwrap();
    assert!(out.contains(&nah), "verlässt die Decke: {out:?}");
    assert!(!out.contains(&fern), "bleibt drunter: {out:?}");
    assert_eq!(m.runs_under_floor(eg), [fern]);
    // Zurück: die nahe kommt wieder unter die Decke
    let back = m.chain(eg).unwrap().points;
    let mut alt = back.clone();
    for p in alt.iter_mut().take(2) {
        p.x = 0.0;
    }
    let out = m.set_run_points(eg, &alt).unwrap();
    assert!(out.contains(&nah) && !out.contains(&fern), "{out:?}");
    m.commit();
    assert!(m.check().is_empty(), "{:?}", m.check());
}

/// BIM Regeln 28 und 30 als eigene Befunde der Prüfung.
#[test]
fn pruefung_stapel_regeln_28_und_30() {
    let (mut m, runs) = gebaeude(2);
    let (eg, og) = (runs[0], runs[1]);
    assert!(m.check().is_empty(), "{:?}", m.check());
    // Regel 28: Segment 1 oben zeigt auf Segment 2 darunter
    let w = m.wall_at(og, 0).unwrap();
    let falsch = m.wall_at(eg, 1).unwrap();
    if let Some(ElementKind::Wall(x)) = m.elements.get_mut(w).map(|e| &mut e.kind) {
        x.coupling = x.coupling.map(|c| Coupling { below: falsch, ..c });
    }
    assert!(
        m.check()
            .iter()
            .any(|t| t.contains("steht nicht über Segment 1")),
        "{:?}",
        m.check()
    );
    // Regel 30: Umriss oben in Achterform
    let (mut m, runs) = gebaeude(2);
    let og = runs[1];
    let acht = [
        vec3(0.0, 0.0, 0.0),
        vec3(10000.0, 8000.0, 0.0),
        vec3(10000.0, 0.0, 0.0),
        vec3(0.0, 8000.0, 0.0),
    ];
    m.runs.get_mut(og).unwrap().points = acht.to_vec();
    assert!(
        m.check()
            .iter()
            .any(|t| t.contains("Umriss schneidet sich selbst")),
        "{:?}",
        m.check()
    );
    // Regel 28: ein Segment ganz ohne Bezug
    let (mut m, runs) = gebaeude(2);
    let w = m.wall_at(runs[1], 2).unwrap();
    if let Some(ElementKind::Wall(x)) = m.elements.get_mut(w).map(|e| &mut e.kind) {
        x.coupling = None;
    }
    assert!(
        m.check().iter().any(|t| t.contains("ohne Stapelbezug")),
        "{:?}",
        m.check()
    );
    // Regel 30: Segment kürzer als min(Wanddicke, Partnerlänge)
    let (mut m, runs) = gebaeude(2);
    let og = runs[1];
    let p = m.run(og).unwrap().points.clone();
    // Ecke 1 so weit vorziehen, dass Segment 1 nur 100 mm lang ist
    let d = (p[1] - p[0]).normalized();
    m.runs.get_mut(og).unwrap().points[1] = p[0] + d * 100.0;
    assert!(
        m.check().iter().any(|t| t.contains("100 mm lang")),
        "{:?}",
        m.check()
    );
    // Regel 26: ein leerer Zug fällt auf
    let (mut m, runs) = gebaeude(2);
    m.runs.get_mut(runs[1]).unwrap().segments.clear();
    assert!(
        m.check().iter().any(|t| t.contains("leer, ohne Segment")),
        "{:?}",
        m.check()
    );
}

#[test]
fn gebaeude_mit_einem_geschoss() {
    let (m, runs) = gebaeude(1);
    assert_eq!(runs.len(), 1);
    let b = m.buildings().ids().next().unwrap();
    let shorts: Vec<String> = m
        .levels_in(Some(b))
        .iter()
        .map(|id| m.storey(*id).unwrap().short.clone())
        .collect();
    assert_eq!(shorts, ["GR", "EG"]);
    assert_eq!(nummern(&m, "AW-").len(), 4);
    assert_eq!(nummern(&m, "DE-"), ["DE-001"]);
    assert!(m.check().is_empty(), "{:?}", m.check());
}

#[test]
fn gebaeude_mit_drei_geschossen() {
    let (m, runs) = gebaeude(3);
    assert_eq!(runs.len(), 3);
    let b = m.buildings().ids().next().unwrap();
    let bands: Vec<(String, f64, f64)> = m
        .levels_in(Some(b))
        .iter()
        .map(|id| {
            let st = m.storey(*id).unwrap();
            (st.short.clone(), st.elevation, st.top())
        })
        .collect();
    assert_eq!(
        bands,
        [
            ("GR".to_string(), -800.0, 0.0),
            ("EG".to_string(), 0.0, 2855.0),
            ("1. OG".to_string(), 2855.0, 5710.0),
            ("2. OG".to_string(), 5710.0, 8565.0),
        ]
    );
    assert_eq!(nummern(&m, "AW-").len(), 12);
    assert_eq!(nummern(&m, "DE-"), ["DE-001", "DE-002", "DE-003"]);
    let top = m.floor(runs[2]).unwrap().unwrap();
    let q = crate::qto::floor_qto_of(&top);
    assert!((q.volume / 1e9 - 16.508448).abs() < 1e-6, "{}", q.volume);
    let de = m.floors_of(runs[2]);
    assert_eq!(m.element(de[0]).unwrap().number, "DE-003");
    // Wände der 2. OG an die des 1. OG gekoppelt
    let w = m.wall_at(runs[2], 0).unwrap();
    let below = match m.element(w).map(|e| &e.kind) {
        Some(ElementKind::Wall(x)) => x.coupling.unwrap().below,
        _ => unreachable!(),
    };
    assert_eq!(Some(below), m.wall_at(runs[1], 0));
    assert!(m.check().is_empty(), "{:?}", m.check());
}

#[test]
fn je_segment_eine_wand_mit_nummer() {
    let mut m = Model::with_seed(1);
    let r = rechteck(&mut m);
    let run = m.run(r).unwrap();
    assert_eq!(run.segments.len(), 4);
    let nums: Vec<&str> = run
        .segments
        .iter()
        .map(|e| m.element(*e).unwrap().number.as_str())
        .collect();
    assert_eq!(nums, ["AW-001", "AW-002", "AW-003", "AW-004"]);
    for (k, e) in run.segments.iter().enumerate() {
        assert_eq!(m.segment_of(*e), Some((r, k)));
        let el = m.element(*e).unwrap();
        assert_eq!(el.category.din276(), Some(330));
        assert_eq!(el.layer_set, Some(m.defaults().exterior_wall));
    }
    assert!(m.check().is_empty(), "{:?}", m.check());
}

#[test]
fn aufbau_wird_referenziert() {
    let mut m = Model::with_seed(2);
    let r = rechteck(&mut m);
    let c = m.chain(r).unwrap();
    assert!((c.thickness() - 315.0).abs() < 1e-9);
    assert!(c.layers[1].core && !c.layers[0].core);
    // Schichtdicke im Aufbau ändern: die Wand folgt ohne eigene Kopie
    let id = m.defaults().exterior_wall;
    let mut s = m.layer_set(id).unwrap().clone();
    s.layers[0].thickness = 160.0;
    assert!(m.set_layer_set(id, s));
    assert!((m.chain(r).unwrap().thickness() - 335.0).abs() < 1e-9);
    // Darstellungsschlüssel führen zurück zum Baustoff
    let key = c.layers[1].material;
    assert_eq!(m.material_by_key(key).unwrap().name, "Porenbeton");
    let fill = m.material_by_key(key | material::CUT).unwrap().cut_fill;
    assert_eq!(m.attr().fill(fill).unwrap().name, "Mauerwerk");
    assert!(m.material_by_key(material::PLAIN).is_none());
}

#[test]
fn verschieben_behaelt_kennungen() {
    let mut m = Model::with_seed(3);
    let r = rechteck(&mut m);
    let before = m.run(r).unwrap().segments.clone();
    let rev = m.revision();
    let moved = m.chain(r).unwrap().with_segment_moved(1, 500.0).unwrap();
    assert!(m.set_run_points(r, &moved.points).is_some());
    assert_eq!(m.run(r).unwrap().segments, before);
    assert!(m.revision() > rev);
    // Offener Zug mit weniger Punkten: überzählige Wände verschwinden
    let mut m2 = Model::with_seed(4);
    let set = m2.defaults().exterior_wall;
    let pts = [
        vec3(0.0, 0.0, 0.0),
        vec3(0.0, 5000.0, 0.0),
        vec3(5000.0, 5000.0, 0.0),
    ];
    let eg = m2.eg_at(3000.0);
    let r2 = m2
        .add_wall_run(&pts, false, RefSide::Left, eg, set, Category::ExteriorWall)
        .unwrap();
    let segs = m2.run(r2).unwrap().segments.clone();
    assert!(m2.set_run_points(r2, &pts[..2]).is_some());
    assert_eq!(m2.run(r2).unwrap().segments, segs[..1]);
    assert!(m2.element(segs[1]).is_none());
    // Und wieder mehr: neue Wand mit neuer Nummer
    assert!(m2.set_run_points(r2, &pts).is_some());
    let e = m2.run(r2).unwrap().segments[1];
    assert_ne!(e, segs[1]);
    assert_eq!(m2.element(e).unwrap().number, "AW-003");
    assert!(m2.check().is_empty(), "{:?}", m2.check());
}

#[test]
fn kennung_nach_lage() {
    let mut m = Model::with_seed(7);
    let r = rechteck(&mut m);
    let s = m.run(r).unwrap().segments.clone();
    let p = m.run(r).unwrap().points.clone();
    // Punkt in Segment 1 einfügen (oben, 0..8000 bei y = 6000), längerer Teil rechts
    let mut q = p.clone();
    q.insert(2, vec3(3000.0, 6000.0, 0.0));
    assert!(m.set_run_points(r, &q).is_some());
    let t = m.run(r).unwrap().segments.clone();
    assert_eq!(t.len(), 5);
    assert_eq!((t[0], t[3], t[4]), (s[0], s[2], s[3]));
    assert_ne!(t[1], s[1]);
    assert_eq!(t[2], s[1], "der längere Teil behält die Kennung");
    // AW-005…008 sind die gestapelten OG-Wände (B12)
    assert_eq!(m.element(t[1]).unwrap().number, "AW-009");
    // Den Punkt wieder entfernen: das kurze Stück verschwindet mit seiner Kennung
    assert!(m.set_run_points(r, &p).is_some());
    assert_eq!(m.run(r).unwrap().segments, s);
    assert!(m.element(t[1]).is_none());
    for (k, e) in s.iter().enumerate() {
        assert_eq!(m.segment_of(*e), Some((r, k)));
    }
    // Erstes Segment eines offenen Zuges entfernen: genau seine Kennung geht
    let set = m.defaults().exterior_wall;
    let o = [
        vec3(0.0, 0.0, 0.0),
        vec3(0.0, 5000.0, 0.0),
        vec3(5000.0, 5000.0, 0.0),
        vec3(5000.0, 0.0, 0.0),
    ];
    let eg = m.eg_at(3000.0);
    let r2 = m
        .add_wall_run(&o, false, RefSide::Left, eg, set, Category::ExteriorWall)
        .unwrap();
    let s2 = m.run(r2).unwrap().segments.clone();
    assert!(m.set_run_points(r2, &o[1..]).is_some());
    assert_eq!(m.run(r2).unwrap().segments, s2[1..]);
    assert!(m.element(s2[0]).is_none());
    assert!(m.check().is_empty(), "{:?}", m.check());
}

/// Parametrik ohne Revision und Zähler, zum Vergleichen zweier Stände.
fn state(m: &Model) -> String {
    format!(
        "{:?}{:?}{:?}{:?}{:?}{:?}",
        m.runs.iter().collect::<Vec<_>>(),
        m.elements.iter().collect::<Vec<_>>(),
        m.layer_sets.iter().collect::<Vec<_>>(),
        m.materials.iter().collect::<Vec<_>>(),
        m.attr.pens().iter().collect::<Vec<_>>(),
        m.attr.display(),
    )
}

#[test]
fn rueckgaengig_vergibt_nichts_doppelt() {
    let mut m = Model::with_seed(5);
    m.require_steps();
    let empty = state(&m);
    m.begin("Wand");
    let r = rechteck(&mut m);
    let t = m.commit().unwrap();
    let guid = m.run(r).unwrap().guid;
    let rev = m.revision();
    let touched = m.apply(&t, Direction::Undo);
    // EG-Zug und gekoppelter OG-Zug
    assert_eq!(touched.runs.len(), 2);
    assert!(touched.runs.contains(&r));
    assert!(m.runs().is_empty() && m.elements().is_empty());
    assert_eq!(state(&m), empty);
    assert!(m.revision() > rev);
    m.begin("Wand");
    let r2 = rechteck(&mut m);
    m.commit();
    assert_ne!(r2, r);
    assert_ne!(m.run(r2).unwrap().guid, guid);
    let first = m.run(r2).unwrap().segments[0];
    assert_eq!(m.element(first).unwrap().number, "AW-009");
}

#[test]
fn anlegen_rueckgaengig_wiederholen_behaelt_kennungen() {
    let mut m = Model::with_seed(7);
    m.require_steps();
    m.begin("Wand");
    let r = rechteck(&mut m);
    let t = m.commit().unwrap();
    let walls = m.run(r).unwrap().segments.clone();
    let numbers: Vec<String> = walls
        .iter()
        .map(|e| m.element(*e).unwrap().number.clone())
        .collect();
    let after = state(&m);
    m.apply(&t, Direction::Undo);
    assert!(m.run(r).is_none());
    m.apply(&t, Direction::Redo);
    assert_eq!(state(&m), after);
    assert_eq!(m.run(r).unwrap().segments, walls);
    for (e, n) in walls.iter().zip(&numbers) {
        assert_eq!(&m.element(*e).unwrap().number, n);
    }
    assert_eq!(numbers[0], "AW-001");
    assert!(m.check().is_empty(), "{:?}", m.check());
}

#[test]
fn loeschen_rueckgaengig_bringt_alte_kennungen() {
    let mut m = Model::with_seed(8);
    m.require_steps();
    m.begin("Wand");
    let r = rechteck(&mut m);
    m.commit();
    let walls = m.run(r).unwrap().segments.clone();
    let guids: Vec<Guid> = walls.iter().map(|e| m.element(*e).unwrap().guid).collect();
    let before = state(&m);
    m.begin("Löschen");
    assert!(m.remove_run(r));
    let t = m.commit().unwrap();
    // Zug, Wände, Sohlplatte, Frostschürze und Erdgeschossdecke, dazu
    // der gekoppelte OG-Zug mit Wänden und OG-Decke
    assert_eq!(t.changes.len(), 2 * (1 + walls.len()) + 4);
    m.apply(&t, Direction::Undo);
    assert_eq!(state(&m), before);
    for (e, g) in walls.iter().zip(&guids) {
        assert_eq!(m.element(*e).unwrap().guid, *g);
    }
    assert!(m.check().is_empty());
    // Wiederholen löscht, ein neuer Zug bekommt keine früher vergebene Kennung
    m.apply(&t, Direction::Redo);
    m.begin("Wand");
    let r2 = rechteck(&mut m);
    m.commit();
    assert_ne!(r2, r);
    for e in &m.run(r2).unwrap().segments {
        assert!(!walls.contains(e));
    }
}

#[test]
fn ziehen_ergibt_einen_eintrag() {
    let mut m = Model::with_seed(9);
    m.require_steps();
    m.begin("Wand");
    let r = rechteck(&mut m);
    m.commit();
    let start = m.run(r).unwrap().points.clone();
    let before = state(&m);
    m.begin("Verschieben");
    for i in 1..=100 {
        let mut p = start.clone();
        p[2].x += i as f64 * 10.0;
        p[3].x += i as f64 * 10.0;
        assert!(m.set_run_points(r, &p).is_some());
    }
    let t = m.commit().unwrap();
    // EG-Zug und der mitgeführte OG-Zug
    assert_eq!(t.changes.len(), 2, "{:?}", t.changes);
    assert!(matches!(t.changes[0], Change::Run { id, .. } if id == r));
    m.apply(&t, Direction::Undo);
    assert_eq!(m.run(r).unwrap().points, start);
    assert_eq!(state(&m), before);
}

#[test]
fn abbrechen_stellt_den_stand_beim_greifen_her() {
    let mut m = Model::with_seed(10);
    m.require_steps();
    m.begin("Wand");
    let r = rechteck(&mut m);
    m.commit();
    let before = state(&m);
    let start = m.run(r).unwrap().points.clone();
    m.begin("Verschieben");
    // Auch ein Teilen und Zusammenführen von Segmenten
    let mut p = start.clone();
    p.insert(1, vec3(0.0, 3000.0, 0.0));
    p[2].x += 500.0;
    assert!(m.set_run_points(r, &p).is_some());
    assert!(m.set_run_points(r, &start[..3]).is_some());
    let touched = m.rollback();
    assert_eq!(touched.runs.len(), 2, "EG- und OG-Zug");
    assert!(touched.runs.contains(&r));
    assert!(!m.in_step());
    assert_eq!(state(&m), before);
    assert!(m.check().is_empty());
}

#[test]
fn umbenennen_eindeutig() {
    let mut m = Model::with_seed(6);
    let r = rechteck(&mut m);
    let s = m.run(r).unwrap().segments.clone();
    assert_eq!(m.set_number(s[0], "AW-Nord"), Ok(()));
    assert_eq!(m.element_by_number("AW-Nord"), Some(s[0]));
    assert_eq!(m.set_number(s[1], "AW-Nord"), Err(NumberError::Taken(s[0])));
    assert_eq!(m.set_number(s[1], "  "), Err(NumberError::Empty));
    assert!(m.remove_run(r));
    assert!(m.elements().is_empty());
    assert!(m.check().is_empty());
}

/// E5/BIM: Stift löschen nur unbenutzt, als Schritt mit Rückgängig;
/// neue Nummer = höchste + 1; Verwender aus Darstellung und Baustoffen.
#[test]
fn stift_loeschen_nur_unbenutzt() {
    let mut m = Model::with_seed(7);
    let pen = |m: &Model, n: u16| m.attr().pens().iter().find(|p| p.1.number == n).unwrap().0;
    let strong = pen(&m, 3);
    let users = m.attr_users(AttrRef::Pen(strong));
    let labels: Vec<String> = users.iter().map(|u| u.label()).collect();
    assert_eq!(labels, ["Schnittkante in Zeichnungen", "Gelände"]);
    let hatch = pen(&m, 4);
    assert!(m.attr_users(AttrRef::Pen(hatch)).iter().all(|u| matches!(
        u,
        AttrUser::Material {
            role: "Schraffur",
            ..
        }
    )));
    assert_eq!(m.next_pen_number(), 11);
    m.begin("Stift");
    assert!(!m.remove_pen(strong), "verwendet");
    let p = Pen {
        guid: m.new_guid(),
        number: m.next_pen_number(),
        name: "Stift 11".into(),
        color: [0, 0, 0],
        width_mm: 0.25,
    };
    let id = m.add_pen(p);
    let t = m.commit().unwrap();
    assert_eq!(m.attr().pens().len(), 11);
    assert!(m.check().is_empty(), "{:?}", m.check());
    m.begin("Löschen");
    assert!(m.remove_pen(id));
    let del = m.commit().unwrap();
    assert!(m.attr().pen(id).is_none());
    assert_eq!(m.next_pen_number(), 11);
    m.apply(&del, Direction::Undo);
    assert_eq!(m.attr().pen(id).unwrap().number, 11);
    m.apply(&t, Direction::Undo);
    assert!(m.attr().pen(id).is_none());
    assert!(m.check().is_empty());
    // Doppelte Nummer verletzt die Prüfregel
    m.begin("Doppelt");
    let mut dup = m.attr().pen(strong).unwrap().clone();
    dup.guid = m.new_guid();
    m.add_pen(dup);
    m.commit();
    assert!(m.check().iter().any(|e| e.contains("Stiftnummer")));
}
