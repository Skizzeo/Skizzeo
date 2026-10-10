use super::*;
use crate::qto::{run_qto, soffit_qto, WallQto};
use crate::solid::Solid;

fn gebaeude() -> (Model, RunId, RunId) {
    let mut m = Model::with_seed(71);
    let b = m.add_building(2);
    let pts = [
        vec3(0.0, 0.0, 0.0),
        vec3(0.0, 8000.0, 0.0),
        vec3(10000.0, 8000.0, 0.0),
        vec3(10000.0, 0.0, 0.0),
    ];
    let eg = m.build_from_polygon(b, &pts).unwrap();
    let og = m.runs_above(eg)[0];
    (m, eg, og)
}

/// Kopplung setzen; `None` löst die Kette (der Stapelbezug bleibt).
fn set_coupling(m: &mut Model, w: ElementId, c: Option<Coupling>) {
    if let Some(ElementKind::Wall(x)) = m.elements.get_mut(w).map(|e| &mut e.kind) {
        match (c, x.coupling.as_mut()) {
            (Some(c), _) => x.coupling = Some(c),
            (None, Some(now)) => now.linked = false,
            (None, None) => {}
        }
    }
}

/// Zieht Segment `seg` um `out` nach außen (negativ: nach innen).
fn drag(m: &mut Model, run: RunId, seg: usize, out: f64) -> bool {
    let c = m.chain(run).unwrap();
    let c = c.with_segment_moved(seg, c.outward_sign() * out).unwrap();
    m.set_run_points(run, &c.points).is_some()
}

fn ys(m: &Model, run: RunId) -> (f64, f64) {
    let p = &m.run(run).unwrap().points;
    (p[1].y, p[2].y)
}

#[test]
fn geloestes_segment_bleibt_stehen() {
    let (mut m, eg, og) = gebaeude();
    // OG-Wand y = 8 lösen und 0,30 m nach außen ziehen (Normale zeigt nach −y)
    let w = m.wall_at(og, 1).unwrap();
    set_coupling(&mut m, w, None);
    assert!(drag(&mut m, og, 1, 300.0));
    assert_eq!(ys(&m, og), (8300.0, 8300.0));
    // EG-Wand y = 8 um 1 m nach außen: das gelöste OG-Segment bleibt stehen
    assert!(drag(&mut m, eg, 1, 1000.0));
    assert_eq!(ys(&m, eg), (9000.0, 9000.0));
    assert_eq!(ys(&m, og), (8300.0, 8300.0));
    // EG-Westwand 0,5 m nach außen: das gekoppelte OG-Segment folgt
    assert!(drag(&mut m, eg, 0, 500.0));
    let (ex, ox) = (
        m.run(eg).unwrap().points[0].x,
        m.run(og).unwrap().points[0].x,
    );
    assert!(
        (ex + 500.0).abs() < 1e-6 && (ox + 500.0).abs() < 1e-6,
        "{ex} {ox}"
    );
    assert_eq!(ys(&m, og), (8300.0, 8300.0));
    m.sync_terraces();
    assert!(m.check().is_empty(), "{:?}", m.check());
    // Bleibt gelöst, die übrigen bleiben gekoppelt
    let coupling = |k: usize| match m.element(m.wall_at(og, k).unwrap()).map(|e| &e.kind) {
        Some(ElementKind::Wall(x)) => x.coupling,
        _ => None,
    };
    assert!(coupling(1).is_some_and(|c| !c.linked));
    assert!((0..4)
        .filter(|k| *k != 1)
        .all(|k| coupling(k).is_some_and(|c| c.linked)));
}

#[test]
fn ungueltiger_stapel_klemmt_das_gummiband() {
    let (mut m, eg, og) = gebaeude();
    // OG-Wand y = 8 gekoppelt mit −7,0 m (steht bei y = 1 m, OG 1 m tief)
    let w = m.wall_at(og, 1).unwrap();
    let below = m.wall_at(eg, 1).unwrap();
    set_coupling(
        &mut m,
        w,
        Some(Coupling {
            below,
            offset: -7000.0,
            linked: true,
        }),
    );
    m.carry_stack(eg);
    m.sync_terraces();
    assert!(m.check().is_empty(), "{:?}", m.check());
    assert_eq!(ys(&m, og), (1000.0, 1000.0));
    // EG-Südwand 0,6 m nach innen: im OG bliebe zwischen den Wänden
    // (2 × 31,5 cm) kein Raum, die Decke fiele weg → abgelehnt
    let before = m.run(eg).unwrap().points.clone();
    assert!(!drag(&mut m, eg, 3, -600.0));
    assert_eq!(m.run(eg).unwrap().points, before);
    assert_eq!(ys(&m, og), (1000.0, 1000.0));
    // 0,3 m nach innen geht noch (OG 0,7 m tief)
    assert!(drag(&mut m, eg, 3, -300.0));
    assert!(m.check().is_empty(), "{:?}", m.check());
}

#[test]
fn streifen_offen_wo_das_og_versetzt_steht() {
    let (mut m, eg, og) = gebaeude();
    let w = m.wall_at(og, 1).unwrap();
    let below = m.wall_at(eg, 1).unwrap();
    let stelle = |m: &mut Model, offset: f64| {
        set_coupling(
            m,
            w,
            Some(Coupling {
                below,
                offset,
                linked: true,
            }),
        );
        m.carry_stack(eg);
    };
    // Rücksprung: der Fuß steht neben dem Streifen (K3)
    stelle(&mut m, -300.0);
    assert_eq!(m.covered_segments(eg), vec![true, false, true, true]);
    assert_eq!(m.open_segments(og), vec![false, true, false, false]);
    // gelöst bleibt die Lage und damit der offene Streifen
    set_coupling(&mut m, w, None);
    assert_eq!(m.covered_segments(eg), vec![true, false, true, true]);
    assert_eq!(m.open_segments(og), vec![false, true, false, false]);
    // Vorsprung: der Streifen wandert mit der Deckenstirn nach außen und
    // liegt wieder unter der Wand (K4)
    stelle(&mut m, 300.0);
    assert_eq!(m.covered_segments(eg), vec![true; 4]);
    assert_eq!(m.open_segments(og), vec![false; 4]);
}

/// Gebäude mit der Nordwand im OG um `out` nach außen (gekoppelt).
fn mit_versatz(out: f64) -> (Model, RunId, RunId) {
    let (mut m, eg, og) = gebaeude();
    let w = m.wall_at(og, 1).unwrap();
    let below = m.wall_at(eg, 1).unwrap();
    set_coupling(
        &mut m,
        w,
        Some(Coupling {
            below,
            offset: out,
            linked: true,
        }),
    );
    m.carry_stack(eg);
    m.sync_soffits();
    m.sync_terraces();
    assert!(m.check().is_empty(), "{:?}", m.check());
    (m, eg, og)
}

fn near(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-3
}

#[test]
fn eg_faengt_geloestes_og_unter_2_cm() {
    // Review 1t T1: OG-Nordwand gelöst bei +300, EG-Nordfuß 290 nach außen
    let (mut m, eg, og) = gebaeude();
    let w = m.wall_at(og, 1).unwrap();
    set_coupling(&mut m, w, None);
    assert!(drag(&mut m, og, 1, 300.0));
    assert!(drag(&mut m, eg, 1, 290.0));
    // Gemessen wären 10 mm: das Segment rastet bündig ein und bleibt gelöst
    assert_eq!(m.stack_offset(w), Some((0.0, false)));
    assert_eq!(ys(&m, og), ys(&m, eg));
    assert!(m.chain(eg).unwrap().joints.overhang.is_none());
    assert!(m.soffit_floors().is_empty());
    assert!(m.check().is_empty(), "{:?}", m.check());
    // Ab 20 mm bleibt der Versatz stehen
    let (mut m, eg, og) = gebaeude();
    let w = m.wall_at(og, 1).unwrap();
    set_coupling(&mut m, w, None);
    assert!(drag(&mut m, og, 1, 300.0));
    assert!(drag(&mut m, eg, 1, 280.0));
    assert!(m.stack_offset(w).is_some_and(|(d, l)| near(d, 20.0) && !l));
    assert!(m.chain(eg).unwrap().joints.overhang.is_some());
}

#[test]
fn versatz_laesst_keinen_stummel_stehen() {
    // Routine 07.10.: EG-Nordwand mit 400-mm-Sprung; die OG-Wand davor
    // 300 nach außen ließe vom Sprung 100 mm übrig (Regel 30: mindestens
    // min(Wanddicke, Partner) = 315 mm). Der Versatz wird abgelehnt.
    let mut m = Model::with_seed(72);
    let b = m.add_building(2);
    let pts = [
        vec3(0.0, 0.0, 0.0),
        vec3(0.0, 8000.0, 0.0),
        vec3(5000.0, 8000.0, 0.0),
        vec3(5000.0, 8400.0, 0.0),
        vec3(10000.0, 8400.0, 0.0),
        vec3(10000.0, 0.0, 0.0),
    ];
    let eg = m.build_from_polygon(b, &pts).unwrap();
    let og = m.runs_above(eg)[0];
    let w = m.wall_at(og, 1).unwrap();
    assert!(m.set_linked(w, false));
    assert!(m.set_offset(w, 300.0).is_none());
    assert_eq!(m.stack_offset(w), Some((0.0, false)));
    // 80 mm lassen 320 mm stehen und gehen
    assert!(m.set_offset(w, 80.0).is_some());
    m.sync_soffits();
    m.sync_terraces();
    assert!(m.check().is_empty(), "{:?}", m.check());
    // Das Gummiband am EG klemmt ebenso: EG-Sprung auf 100 mm kürzen
    // ließe über dem gelösten OG 20 mm Stummel
    let c = m.chain(eg).unwrap();
    let c = c.with_segment_moved(3, c.outward_sign() * -300.0).unwrap();
    assert!(m.set_run_points(eg, &c.points).is_none());
    assert!(m.check().is_empty(), "{:?}", m.check());
}

#[test]
fn eg_rueckt_an_die_og_wand() {
    // Jörn 07.10. 07:53: Zielwand frei. OG-Nordwand gelöst bei +300,
    // dann „Bündig setzen“ mit Ziel OG: die EG-Nordwand rückt 300 nach
    // außen, die OG-Wand bleibt, das Paar ist gekoppelt bei 0.
    let (mut m, eg, og) = gebaeude();
    let w = m.wall_at(og, 1).unwrap();
    let e = m.wall_at(eg, 1).unwrap();
    set_coupling(&mut m, w, None);
    assert!(drag(&mut m, og, 1, 300.0));
    assert_eq!(m.can_flush_to(e, w), Ok(()));
    assert_eq!(m.can_flush_to(e, e), Err(FlushError::NotPartners));
    assert_eq!(
        m.can_flush_to(w, m.wall_at(eg, 0).unwrap()),
        Err(FlushError::NotPartners)
    );
    let out = m.flush_to(e, w).unwrap();
    assert!(out.contains(&eg) && out.contains(&og));
    m.sync_soffits();
    m.sync_terraces();
    assert_eq!(ys(&m, eg), (8300.0, 8300.0));
    assert_eq!(ys(&m, og), (8300.0, 8300.0));
    assert_eq!(m.stack_offset(w), Some((0.0, true)));
    // Die gekoppelten OG-Nachbarn folgen den EG-Ecken, alles bündig
    assert_eq!(m.run(og).unwrap().points, m.run(eg).unwrap().points);
    assert!(m.chain(eg).unwrap().joints.overhang.is_none());
    assert!(m.soffit_floors().is_empty());
    assert!(m.check().is_empty(), "{:?}", m.check());
    // Ein zweites Mal ändert nichts
    assert_eq!(m.flush_to(e, w), Ok(Vec::new()));
}

#[test]
fn eg_rueckt_nach_innen_und_gekoppelt_mit_versatz() {
    // Gekoppelt bei −300 (Rücksprung): Ziel OG zieht die EG-Wand 300 nach
    // innen; ein anderes gelöstes OG-Segment bleibt stehen
    let (mut m, eg, og) = mit_versatz(-300.0);
    let w = m.wall_at(og, 1).unwrap();
    let e = m.wall_at(eg, 1).unwrap();
    let ost = m.wall_at(og, 2).unwrap();
    assert!(m.set_linked(ost, false));
    assert!(m.set_offset(ost, 200.0).is_some());
    let ost_x = m.run(og).unwrap().points[2].x;
    assert!(m.flush_to(e, w).is_ok());
    m.sync_soffits();
    m.sync_terraces();
    assert_eq!(ys(&m, eg), (7700.0, 7700.0));
    assert_eq!(ys(&m, og), (7700.0, 7700.0));
    assert_eq!(m.stack_offset(w), Some((0.0, true)));
    assert_eq!(m.run(og).unwrap().points[2].x, ost_x);
    assert!(m
        .stack_offset(ost)
        .is_some_and(|(d, l)| near(d, 200.0) && !l));
    assert!(m.check().is_empty(), "{:?}", m.check());
}

#[test]
fn bündig_ans_eg_wie_bisher_und_abgelehnt_ohne_aenderung() {
    let (mut m, eg, og) = mit_versatz(300.0);
    let w = m.wall_at(og, 1).unwrap();
    let e = m.wall_at(eg, 1).unwrap();
    // Ziel EG: das OG rückt (bisheriges „Bündig setzen“)
    assert!(m.flush_to(w, e).is_ok());
    assert_eq!(ys(&m, og), (8000.0, 8000.0));
    assert_eq!(ys(&m, eg), (8000.0, 8000.0));
    // EG mit 200-mm-Sprung, OG-Wand rechts davon gelöst bei −300: rückte
    // die EG-Wand ans OG, kehrte sich der EG-Sprung um → abgelehnt,
    // nichts ändert sich
    let mut m = Model::with_seed(73);
    let b = m.add_building(2);
    let pts = [
        vec3(0.0, 0.0, 0.0),
        vec3(0.0, 8000.0, 0.0),
        vec3(5000.0, 8000.0, 0.0),
        vec3(5000.0, 8200.0, 0.0),
        vec3(10000.0, 8200.0, 0.0),
        vec3(10000.0, 0.0, 0.0),
    ];
    let eg = m.build_from_polygon(b, &pts).unwrap();
    let og = m.runs_above(eg)[0];
    let w = m.wall_at(og, 3).unwrap();
    let e = m.wall_at(eg, 3).unwrap();
    // die Wand links vom Sprung gelöst bei −300: oben bleibt der Sprung
    // 200 lang (7700 → 7900), unten kehrte er sich um
    let a = m.wall_at(og, 1).unwrap();
    assert!(m.set_linked(a, false));
    assert!(m.set_offset(a, -300.0).is_some());
    assert!(m.set_linked(w, false));
    assert!(m.set_offset(w, -300.0).is_some());
    m.sync_soffits();
    m.sync_terraces();
    let before = (
        m.run(eg).unwrap().points.clone(),
        m.run(og).unwrap().points.clone(),
    );
    assert_eq!(m.can_flush_to(e, w), Err(FlushError::Invalid));
    assert_eq!(m.flush_to(e, w), Err(FlushError::Invalid));
    assert_eq!(
        before,
        (
            m.run(eg).unwrap().points.clone(),
            m.run(og).unwrap().points.clone()
        )
    );
    assert_eq!(m.stack_offset(w), Some((-300.0, false)));
    assert!(m.check().is_empty(), "{:?}", m.check());
}

#[test]
fn dachterrasse_ueber_dem_ruecksprung() {
    // BIM-Sollwert: AW-006 −1,50 → 9,72 × 1,36 = 13,2192 m²
    let (m, eg, og) = mit_versatz(-1500.0);
    let t = m.terrace_outlines(eg);
    assert_eq!(t.len(), 1);
    assert_eq!(t[0].segments, vec![1]);
    assert!(near(t[0].area(), 9720.0 * 1360.0), "{}", t[0].area());
    assert!(m.terrace_outlines(og).is_empty());
    // Vorsprung und bündig: keine Terrasse
    assert_eq!(m_area(&mit_versatz(300.0)), 0.0);
    assert_eq!(m_area(&gebaeude()), 0.0);
    // Lichte Tiefe 20 mm: Rücksprung 160 (WDVS 14) gilt, 150 nicht
    assert!(near(m_area(&mit_versatz(-160.0)), 9720.0 * 20.0));
    assert_eq!(m_area(&mit_versatz(-150.0)), 0.0);
}

#[test]
fn dachterrasse_richtung_spitze_winkel_und_vorsprung_daneben() {
    let haus = |pts: &[Vec3], k: usize, d: f64, nb: Option<(usize, f64)>| {
        let mut m = Model::with_seed(75);
        let b = m.add_building(2);
        let eg = m.build_from_polygon(b, pts).unwrap();
        let og = m.runs_above(eg)[0];
        for (k, d) in std::iter::once((k, d)).chain(nb) {
            let w = m.wall_at(og, k).unwrap();
            assert!(m.set_linked(w, false));
            assert!(m.set_offset(w, d).is_some(), "{k} {d}");
        }
        m.sync_soffits();
        m.sync_terraces();
        assert!(m.check().is_empty(), "{:?}", m.check());
        m.terrace_outlines(eg)
    };
    // Gegenrichtung: dasselbe Haus, Nord ist dort Segment 1 rückwärts
    let ccw = [
        vec3(0.0, 0.0, 0.0),
        vec3(10000.0, 0.0, 0.0),
        vec3(10000.0, 8000.0, 0.0),
        vec3(0.0, 8000.0, 0.0),
    ];
    let t = haus(&ccw, 2, -1500.0, None);
    assert_eq!(t.len(), 1);
    assert!(t[0].area() > 0.0);
    // Spitze Winkel 20° und 35°: Fläche positiv, Umriss einfach
    for deg in [20.0f64, 35.0] {
        let a = deg.to_radians();
        let tri = [
            vec3(0.0, 0.0, 0.0),
            vec3(14000.0 * a.cos(), 14000.0 * a.sin(), 0.0),
            vec3(14000.0, 0.0, 0.0),
        ];
        for k in 0..3 {
            let t = haus(&tri, k, -800.0, None);
            assert_eq!(t.len(), 1, "{deg} {k}");
            assert!(t[0].area() > 0.0 && t[0].parts.iter().all(|p| sk_math::polygon::is_simple(p)));
        }
    }
    // Nord −1,50, West springt 0,30 vor (UD): die Terrasse endet an der
    // Deckenkante in OG-Lage (x = 0,14 − 0,30)
    let (mut m, eg, og) = gebaeude();
    for (k, d) in [(0, 300.0), (1, -1500.0)] {
        let w = m.wall_at(og, k).unwrap();
        assert!(m.set_linked(w, false));
        assert!(m.set_offset(w, d).is_some());
    }
    m.sync_soffits();
    m.sync_terraces();
    let t = m.terrace_outlines(eg);
    assert_eq!(t.len(), 1);
    assert!(
        near(t[0].area(), (9860.0 + 160.0) * 1360.0),
        "{}",
        t[0].area()
    );
}

fn m_area(x: &(Model, RunId, RunId)) -> f64 {
    x.0.terrace_outlines(x.1).iter().map(|t| t.area()).sum()
}

fn r4(x: f64) -> f64 {
    (x * 1e4).round() / 1e4
}

fn r3(x: f64) -> f64 {
    (x * 1e3).round() / 1e3
}

/// Prüfhaus (paket-dachterrasse §6): OG-Nord −1,50 im Schritt gelöst
/// und versetzt; je Rücksprung `rueck` (Segment, mm).
fn pruefhaus(rueck: &[(usize, f64)]) -> (Model, RunId, RunId) {
    let (mut m, eg, og) = gebaeude();
    for &(k, d) in rueck {
        let w = m.wall_at(og, k).unwrap();
        m.begin("Wand verschoben");
        assert!(m.set_linked(w, false));
        assert!(m.set_offset(w, d).is_some(), "{k} {d}");
        m.commit();
    }
    assert!(m.check().is_empty(), "{:?}", m.check());
    (m, eg, og)
}

/// Das Attikablech überdeckt die Attika außen um 50 mm und innen um
/// 40 mm, mit jedem Dämmstreifen auf der Krone (Schenkel um dessen Dicke
/// länger); ohne Streifen wie vorher.
#[test]
fn blech_ueberdeckt_die_attika_ueber_dem_streifen() {
    let (mut m, eg, _) = pruefhaus(&[(1, -1500.0)]);
    let de = m.floor_of(eg).unwrap();
    let ab = m.coping_of(de).expect("AB");
    for (c, girth) in [(0.0, 270.0), (20.0, 310.0), (40.0, 350.0), (100.0, 470.0)] {
        m.begin("Dämmung auf der Attika");
        assert!(m.set_floor_crown(de, c));
        m.commit();
        let f = m.floor(eg).unwrap().unwrap();
        let (_, top) = f.attika_band().unwrap();
        assert_eq!(f.coping_girth(), girth);
        assert_eq!(crate::qto::coping_qto(&m, ab).unwrap().girth, girth);
        // Blech allein: tiefster Punkt ist der Außenschenkel
        let s = f.coping_solid();
        let lo = s
            .triangles
            .iter()
            .filter(|t| t.p.iter().all(|p| p.z < top - 1.0 || p.z > top + c + 1.0))
            .flat_map(|t| t.p)
            .map(|p| p.z)
            .fold(f64::INFINITY, f64::min);
        assert!((lo - (top - 50.0)).abs() < 1e-6, "{c}: {lo}");
    }
}

/// D1–D3 gegen die endgültigen Sollwerte (BIM 08:54): DT 13,2192 m²,
/// Dämmung 1,0575 m³, Belag 0,7932 m³, AB 13,00 m an der Außenkante,
/// Abwicklung 250 mm, Attika-Mehrmenge WDVS 0,3562 m³, EG-Dämmung
/// 14,5215 m³; Höhen +2,855 / +2,995 / +3,055.
#[test]
fn dachterrasse_sollwerte_pruefhaus() {
    let (m0, eg0, _) = gebaeude();
    let wdvs0: f64 = run_qto(&m0, eg0).iter().map(|w| w.layers[0].volume).sum();
    assert_eq!(r4(wdvs0 / 1e9), 14.1654);

    let (m, eg, og) = pruefhaus(&[(1, -1500.0)]);
    let de = m.floor_of(eg).unwrap();
    let dt = m.terrace_of(de).expect("DT");
    let ab = m.coping_of(de).expect("AB");
    assert_eq!(m.element(dt).unwrap().number, "DT-001");
    assert_eq!(m.element(ab).unwrap().number, "AB-001");
    assert!(m.terrace_of(m.floor_of(og).unwrap()).is_none());
    let t = m
        .layer_set(m.element(dt).unwrap().layer_set.unwrap())
        .unwrap();
    assert_eq!(
        (t.code.as_str(), t.name.as_str()),
        ("DT-14", "Dachterrasse 14")
    );
    assert_eq!(t.category, TypeCategory::RoofTerrace);
    let q = crate::qto::terrace_qto(&m, dt).unwrap();
    assert_eq!(
        (
            r4(q.area / 1e6),
            r4(q.insulation_volume / 1e9),
            r4(q.finish_volume / 1e9)
        ),
        (13.2192, 1.0575, 0.7932)
    );
    let c = crate::qto::coping_qto(&m, ab).unwrap();
    assert!(near(c.length, 13000.0), "{}", c.length);
    // Tropfkante 40 statt 20 (Jörn 10.10., Plan Flachdach P12); über
    // dem Dämmstreifen 40 beide Schenkel um 40 länger: 270 + 80
    assert_eq!(c.girth, 350.0);
    assert_eq!(m.props_of(dt).get("begehbar"), Some(&PropValue::Bool(true)));
    assert_eq!(
        m.props_of(ab).get("Abwicklung"),
        Some(&PropValue::Number(350.0))
    );
    assert_eq!(
        m.props_of(ab).get("Zuschnitt"),
        Some(&PropValue::Number(400.0))
    );
    let f = m.floor(eg).unwrap().unwrap();
    assert_eq!(f.terrace_band(), Some((2855.0, 2995.0)));
    assert_eq!(f.attika_band(), Some((2855.0, 3055.0)));
    let qs = run_qto(&m, eg);
    let wdvs: f64 = qs.iter().map(|w| w.layers[0].volume).sum();
    // 14,165368 + 0,35616 = 14,521528 (BIM rundete die Summanden: 14,5216)
    assert_eq!(r4(wdvs / 1e9), 14.5215);
    assert_eq!(r4((wdvs - wdvs0) / 1e9), 0.3562);
    assert_eq!(qs[3], run_qto(&m0, eg0)[3], "Süd ohne Attika");
    // Körper: Terrasse bis OK Belag, Attika bis OK Attika, Blech darauf
    let z = |s: &Solid| s.bounds().map(|(lo, hi)| (lo.z.round(), hi.z.round()));
    assert_eq!(z(&f.terrace_solid()), Some((2855.0, 2995.0)));
    let wall = m.chain(eg).unwrap().solid();
    assert_eq!(z(&wall).unwrap().1, 3055.0);
    // Dämmstreifen 4 cm auf der Krone (Jörn 10.10. 16:07), Blech darüber
    assert_eq!(f.crown_band(), Some((3055.0, 3095.0)));
    let (lo, hi) = f.coping_solid().bounds().unwrap();
    // Außenschenkel 50 + 40 ab UK Blech: 5 cm über die Attika hinab
    assert!(
        lo.z.round() == 3005.0 && hi.z >= 3095.0 && hi.z < 3140.0,
        "{lo:?} {hi:?}"
    );
    let k = c.crown.as_ref().expect("Dämmstreifen");
    assert_eq!(k.thickness, 40.0);
    assert_eq!(r4(k.volume / 1e9), r4(k.area * 40.0 / 1e9));
    // DE-002 folgt dem OG, DE-001 unverändert, keine UD
    assert_eq!(r4(m.floor(og).unwrap().unwrap().area() / 1e6), 60.4584);
    assert_eq!(r4(f.area() / 1e6), 75.0384);
    assert!(m.soffit_floors().is_empty());

    // Datei: nur ergänzt, Rundlauf bytegleich, fehlende Zeile ergänzt
    let text = crate::szo::write(&m);
    assert!(text.starts_with("SZO 4\n"));
    assert_eq!(
        text.lines().filter(|l| l.starts_with("[terrace]")).count(),
        1
    );
    assert_eq!(
        text.lines().filter(|l| l.starts_with("[coping]")).count(),
        1
    );
    assert!(text.contains(" cat=roofterrace "), "Projekttyp");
    let read = |t: &str| crate::szo::read(t, GuidGen::with_seed(3)).unwrap();
    let back = read(&text);
    assert!(back.hints.is_empty(), "{:?}", back.hints);
    assert_eq!(crate::szo::write(&back.model), text);
    let ohne: String = text
        .lines()
        .filter(|l| !l.starts_with("[terrace]") && !l.starts_with("[coping]"))
        .map(|l| format!("{l}\n"))
        .collect();
    let back = read(&ohne);
    assert!(!back.hints.is_empty());
    assert!(back.model.check().is_empty(), "{:?}", back.model.check());
    let (_, b0, _) = gebaeude();
    let _ = b0;
    let plain = crate::szo::write(&m0);
    assert!(!plain.contains("[terrace]") && !plain.contains("roofterrace"));
}

/// E3: Wechsel auf Vorsprung nimmt DT und AB mit, die UD entsteht;
/// zurück entstehen sie mit neuen Nummern. Über Eck, zwei Stücke und
/// ringsum hat das Blech 21, 26 und 36 m (A197).
#[test]
fn dachterrasse_wechsel_und_laengen() {
    let (mut m, eg, og) = pruefhaus(&[(1, -1500.0)]);
    let de = m.floor_of(eg).unwrap();
    let w = m.wall_at(og, 1).unwrap();
    m.begin("Wand verschoben");
    assert!(m.set_offset(w, 300.0).is_some());
    m.commit();
    assert!(m.terrace_of(de).is_none() && m.coping_of(de).is_none());
    assert!(m.soffit_of(de).is_some());
    m.begin("Wand verschoben");
    assert!(m.set_offset(w, -1500.0).is_some());
    m.commit();
    let dt = m.terrace_of(de).unwrap();
    assert_eq!(m.element(dt).unwrap().number, "DT-002");
    assert_eq!(
        m.element(m.coping_of(de).unwrap()).unwrap().number,
        "AB-002"
    );
    assert!(m.check().is_empty(), "{:?}", m.check());

    for (rueck, len, area) in [
        (vec![(1, -1500.0), (2, -1500.0)], 21000.0, 21.8688),
        (vec![(1, -1500.0), (3, -1500.0)], 26000.0, 26.4384),
        (
            vec![(0, -1500.0), (1, -1500.0), (2, -1500.0), (3, -1500.0)],
            36000.0,
            40.0384,
        ),
    ] {
        let (m, eg, _) = pruefhaus(&rueck);
        let de = m.floor_of(eg).unwrap();
        let c = crate::qto::coping_qto(&m, m.coping_of(de).unwrap()).unwrap();
        assert!(near(c.length, len), "{rueck:?}: {}", c.length);
        let q = crate::qto::terrace_qto(&m, m.terrace_of(de).unwrap()).unwrap();
        assert_eq!(r4(q.area / 1e6), area, "{rueck:?}");
        let f = m.floor(eg).unwrap().unwrap();
        assert!(!f.coping_solid().is_empty());
        assert!(m.chain(eg).unwrap().solid().bounds().unwrap().1.z > 3054.0);
    }
}

/// KA-0a4, Regel 100: tragend (IFC LoadBearing, Auflager) und KG
/// 341/342 aus derselben Regel. IW-11,5 trägt nicht (KG 342), IW-24 und
/// IW-17,5 tragen (KG 341), alle Außenwandtypen tragen.
#[test]
fn tragend_und_kg_nach_regel_100() {
    let (mut m, eg, _) = gebaeude();
    for (_, t) in m.layer_sets().iter() {
        match t.category {
            TypeCategory::ExteriorWall => assert!(t.load_bearing(), "{}", t.code),
            TypeCategory::InteriorWall => {
                assert_eq!(t.load_bearing(), t.code != "IW-11,5", "{}", t.code)
            }
            _ => {}
        }
    }
    let st = m.run(eg).unwrap().storey;
    m.begin("Innenwände");
    let mut walls = Vec::new();
    for (i, g) in [
        INTERIOR_115_TYPE_GUID,
        INTERIOR_240_TYPE_GUID,
        INTERIOR_TYPE_GUID,
    ]
    .into_iter()
    .enumerate()
    {
        let t = m.type_by_guid(g).unwrap();
        let y = 2000.0 + 1500.0 * i as f64;
        let r = m.add_wall_run(
            &[vec3(1000.0, y, 0.0), vec3(4000.0, y, 0.0)],
            false,
            RefSide::Left,
            st,
            t,
            Category::InteriorWall,
        );
        walls.push((t, r.unwrap()));
    }
    m.commit();
    for (t, r) in walls {
        let w = m.wall_at(r, 0).unwrap();
        let core = m.element_layers(w).iter().position(|l| l.core).unwrap();
        let lb = m.layer_set(t).unwrap().load_bearing();
        assert_eq!(m.layer_kg(w, core), Some(if lb { 341 } else { 342 }));
    }
}

/// KA-0: Schalfläche ohne Auflager auf Außen- und Innenwänden,
/// Randschalung am Umfang, Auflager je Wand. Prüfhaus 10 × 8 m, AW-31,5:
/// Decke 9,72 × 7,72 = 75,0384 m², Kernring 5,9815 m², Schalfläche
/// 69,0569 m² (0,92-fach), Randschalung 34,88 m × 22 cm; Innenwände
/// IW-24 1,44 m² und IW-11,5 0,345 m² mindern die Schalfläche.
#[test]
fn schalung_der_decke_ohne_auflager() {
    let (mut m, eg, _) = gebaeude();
    let de = m.floor_of(eg).unwrap();
    let q = crate::qto::formwork_qto(&m, de).unwrap();
    assert_eq!(r4(q.support_area() / 1e6), 5.9815);
    assert_eq!(r4(q.soffit / 1e6), 69.0569);
    assert_eq!(r4(q.edge / 1e3), 34.88);
    assert_eq!((q.edge_height, q.edge_lost), (220.0, 0.0));
    assert_eq!(q.supports.len(), 4);
    assert!(q.supports.iter().all(|s| s.bearing));
    // Innenwände unter der Decke
    let st = m.run(eg).unwrap().storey;
    let iw24 = m.type_by_guid(INTERIOR_240_TYPE_GUID).unwrap();
    let iw115 = m.type_by_guid(INTERIOR_115_TYPE_GUID).unwrap();
    m.begin("Innenwände");
    let a = m.add_wall_run(
        &[vec3(5000.0, 1000.0, 0.0), vec3(5000.0, 7000.0, 0.0)],
        false,
        RefSide::Left,
        st,
        iw24,
        Category::InteriorWall,
    );
    let b = m.add_wall_run(
        &[vec3(1000.0, 4000.0, 0.0), vec3(4000.0, 4000.0, 0.0)],
        false,
        RefSide::Left,
        st,
        iw115,
        Category::InteriorWall,
    );
    m.commit();
    let (a, b) = (
        m.wall_at(a.unwrap(), 0).unwrap(),
        m.wall_at(b.unwrap(), 0).unwrap(),
    );
    let q = crate::qto::formwork_qto(&m, de).unwrap();
    let s = |w| q.supports.iter().find(|s| s.wall == w).unwrap();
    assert_eq!((r4(s(a).area / 1e6), s(a).bearing), (1.44, true));
    // tragend nach Typ (LayerSet::load_bearing, eine Quelle mit KG und
    // IFC); die Schalfläche zieht beide ab
    let lb = |t| m.layer_set(t).unwrap().load_bearing();
    assert_eq!((r4(s(b).area / 1e6), s(b).bearing), (0.345, lb(iw115)));
    assert_eq!(r4(q.soffit / 1e6), r4(69.0569 - 1.44 - 0.345));
    let extra = if lb(iw115) { 0.345 } else { 0.0 };
    assert_eq!(r4(q.bearing_area() / 1e6), r4(5.9815 + 1.44 + extra));
    // Sohlplatte: keine Schalfläche, Randschalung am Umfang
    let (slab, _) = m.foundation_of(eg).unwrap();
    let p = crate::qto::formwork_qto(&m, slab).unwrap();
    assert_eq!(p.soffit, 0.0);
    assert!(p.edge > 0.0 && p.supports.is_empty());
    // Monolithisch: Randdämmstreifen ersetzen die Randschalung
    let (mut m, eg, og) = gebaeude();
    let t = m.type_by_guid(MONO_TYPE_GUID).unwrap();
    m.begin("Wandtyp");
    assert!(m.set_run_type(eg, t) && m.set_run_type(og, t));
    m.commit();
    let de = m.floor_of(eg).unwrap();
    let q = crate::qto::formwork_qto(&m, de).unwrap();
    assert!(q.edge_lost > 0.0, "{q:?}");
    let f = m.floor(eg).unwrap().unwrap();
    assert!((q.edge + q.edge_lost - f.perimeter()).abs() < 1e-6);
    assert!(crate::qto::formwork_qto(&m, m.wall_at(eg, 0).unwrap()).is_none());
    // Ein Durchlauf: Mengenliste und floor_qto tragen dieselbe Schalung
    let fq = crate::qto::floor_qto(&m, eg).unwrap();
    assert_eq!(fq.formwork.as_ref(), Some(&q));
    let row = crate::qto::schedule(&m)
        .buildings
        .iter()
        .flat_map(|b| &b.storeys)
        .flat_map(|st| &st.groups)
        .flat_map(|g| &g.rows)
        .find_map(|r| match &r.q {
            Some(crate::qto::ElementQto::Floor(f)) if r.element == de => f.formwork.clone(),
            _ => None,
        });
    assert_eq!(row.as_ref(), Some(&q));
    let (slab, _) = m.foundation_of(eg).unwrap();
    let (sq, _) = crate::qto::foundation_qto(&m, eg).unwrap();
    assert_eq!(Some(sq.formwork), crate::qto::formwork_qto(&m, slab));
}

/// BIM Regel 84: Innenfläche je Schicht, innere Kante × Höhe.
#[test]
fn innenflaeche_der_schicht() {
    let (m, eg, _) = gebaeude();
    let qs = crate::qto::run_qto(&m, eg);
    let w = &qs[0];
    let core = w.layers.last().unwrap();
    // Innenkante des Porenbetons × Höhe ohne Deckenband
    assert!(core.inner_area > 0.0);
    let h = w.height - core.pocket / core.area;
    let inner_len = core.inner_area / h;
    assert!(
        inner_len < core.length && core.length < core.side_area / w.height + 1.0,
        "{inner_len} {}",
        core.length
    );
}

/// KA-0a2 Regel 84 (architektur/paket-ka0.md §3.3): Abrechnungsfläche
/// je Schicht. Standardhaus 1b (Prüfhaus Nord −1,50): Porenbeton 17,5
/// 30,139 m³, Fläche 172,224 m², WDVS nach Außenfläche 199,595 m²
/// (wie A198); Decke und Sohlplatte mit Fläche, DT mit Abrechnungsfläche,
/// Frostschürze und Attikablech 0.
#[test]
fn abrechnungsflaeche_nach_regel_84() {
    let (m, _, _) = pruefhaus(&[(1, -1500.0)]);
    let s = crate::qto::schedule(&m);
    let rows: Vec<_> = s.buildings[0]
        .by_trade
        .iter()
        .flat_map(|t| &t.rows)
        .collect();
    let named = |n: &str| {
        rows.iter()
            .filter(|r| m.material(r.material).unwrap().name == n)
            .copied()
            .collect::<Vec<_>>()
    };
    let sum = |v: &[&crate::qto::LayerRow], f: fn(&crate::qto::LayerRow) -> f64| {
        v.iter().map(|r| f(r)).sum::<f64>()
    };
    let gb = named("Porenbeton");
    assert_eq!(gb.len(), 8);
    assert_eq!(r3(sum(&gb, |r| r.volume) / 1e9), 30.139);
    // 30,13913 m³ ÷ 0,175 = 172,2236 m²: je Zeile Volumen ÷ Dicke, nicht
    // aus dem gerundeten Volumen (30,139 ÷ 0,175 = 172,2229)
    assert_eq!(r3(sum(&gb, |r| r.face) / 1e6), 172.224);
    let wdvs = named("Dämmung (WDVS)");
    assert!(wdvs.iter().all(|r| r.face == r.bill_area && r.face > 0.0));
    assert_eq!(r3(sum(&wdvs, |r| r.face) / 1e6), 199.595);
    for r in &rows {
        let soll = match r.category {
            Category::Floor | Category::GroundSlab => r.area,
            Category::RoofTerrace => r.bill_area,
            // Blech in m, Dämmstreifen auf der Krone mit seiner Fläche
            Category::Coping if r.layer > 0 => r.bill_area,
            Category::StripFooting | Category::Coping => 0.0,
            _ => continue,
        };
        assert_eq!(r.face, soll, "{} {}", r.number, r.layer);
    }
    // Lage je Schicht der sieben Werkstypen: M Volumen ÷ Dicke, A Außenfläche
    for (g, lage) in [
        (EXTERIOR_TYPE_GUID, "AM"),
        (ETICS_TYPE_GUID, "AM"),
        (CAVITY_TYPE_GUID, "A-MM"),
        (MONO_TYPE_GUID, "M"),
        (INTERIOR_TYPE_GUID, "M"),
        (INTERIOR_115_TYPE_GUID, "M"),
        (INTERIOR_240_TYPE_GUID, "M"),
    ] {
        let (mut m, eg, _) = gebaeude();
        let t = m.type_by_guid(g).unwrap();
        m.begin("Wandtyp");
        let run = if werk_category(g) == Some(TypeCategory::ExteriorWall) {
            assert!(m.set_run_type(eg, t));
            eg
        } else {
            let st = m.run(eg).unwrap().storey;
            m.add_wall_run(
                &[vec3(5000.0, 1000.0, 0.0), vec3(5000.0, 7000.0, 0.0)],
                false,
                RefSide::Left,
                st,
                t,
                Category::InteriorWall,
            )
            .unwrap()
        };
        m.commit();
        assert_eq!(lage_der_schichten(&m, run), lage, "{g:?}");
    }
    // Putz außen nach Außenfläche, innen nach Innenfläche
    let (mut m, eg, og) = gebaeude();
    let putz = m
        .materials
        .iter()
        .find(|(_, x)| x.category == MatCategory::Plaster)
        .map(|(id, _)| id)
        .unwrap();
    let t = m.type_by_guid(ETICS_TYPE_GUID).unwrap();
    let ls = &mut m.layer_sets.get_mut(t).unwrap().layers;
    let p = MaterialLayer::new(putz, 15.0, LayerFunction::Finish);
    ls.insert(0, p);
    ls.push(p);
    m.begin("Wandtyp");
    assert!(m.set_run_type(eg, t) && m.set_run_type(og, t));
    m.commit();
    assert_eq!(lage_der_schichten(&m, eg), "AAMI");
    let iw = m.type_by_guid(INTERIOR_TYPE_GUID).unwrap();
    let ls = &mut m.layer_sets.get_mut(iw).unwrap().layers;
    let p = MaterialLayer::new(putz, 10.0, LayerFunction::Finish);
    ls.insert(0, p);
    ls.push(p);
    let st = m.run(eg).unwrap().storey;
    m.begin("Innenwand");
    // mit Ecke, damit Außen- und Innenfläche verschieden sind
    let r = m.add_wall_run(
        &[
            vec3(5000.0, 1000.0, 0.0),
            vec3(5000.0, 7000.0, 0.0),
            vec3(8000.0, 7000.0, 0.0),
        ],
        false,
        RefSide::Left,
        st,
        iw,
        Category::InteriorWall,
    );
    m.commit();
    assert_eq!(lage_der_schichten(&m, r.unwrap()), "AMI");
}

/// Lage je Schicht des ersten Segments von `run` aus `LayerRow.face`:
/// M Volumen ÷ Dicke, A Außenfläche, I Innenfläche, - ohne Zeile (Luft).
fn lage_der_schichten(m: &Model, run: RunId) -> String {
    let w = m.wall_at(run, 0).unwrap();
    let q = &crate::qto::run_qto(m, run)[0];
    let s = crate::qto::schedule(m);
    let rows: Vec<_> = s.buildings[0]
        .by_kg
        .iter()
        .flat_map(|k| &k.rows)
        .filter(|r| r.element == w)
        .collect();
    (0..q.layers.len())
        .map(|i| {
            let Some(r) = rows.iter().find(|r| r.layer == i) else {
                return '-';
            };
            let l = &q.layers[i];
            assert_eq!(r.pocket, l.pocket);
            let mean = r.volume / r.thickness;
            // eindeutig: die drei Flächen liegen je Schicht auseinander
            assert!((l.side_area - l.inner_area).abs() > 1e3, "{i}");
            if r.face == l.side_area && (mean - l.side_area).abs() > 1e3 {
                'A'
            } else if r.face == l.inner_area && (mean - l.inner_area).abs() > 1e3 {
                'I'
            } else {
                assert!((r.face - mean).abs() < 1e-6, "{i} {} {mean}", r.face);
                'M'
            }
        })
        .collect()
}

/// KA-0a2: Schalung und Auflagertasche in der Mengenliste aus den schon
/// gerechneten Mengen; `restrict` filtert nach Umfang (Regel 95, 96).
#[test]
fn schalung_und_umfang_in_der_mengenliste() {
    let (m, eg, og) = pruefhaus(&[(1, -1500.0)]);
    let s = crate::qto::schedule(&m);
    let (slab, _) = m.foundation_of(eg).unwrap();
    let (de1, de2) = (m.floor_of(eg).unwrap(), m.floor_of(og).unwrap());
    let ids: Vec<_> = s.formwork.iter().map(|(e, _)| *e).collect();
    assert_eq!(ids, [slab, de1, de2]);
    for (e, f) in &s.formwork {
        assert_eq!(crate::qto::formwork_qto(&m, *e).as_ref(), Some(f));
    }
    assert_eq!(r4(s.formwork[1].1.soffit / 1e6), 69.0569);
    // Auflager: Decke trägt die Summe der Wandtaschen darunter
    let rows: Vec<_> = s.buildings[0].by_kg.iter().flat_map(|k| &k.rows).collect();
    let de = rows.iter().find(|r| r.element == de1).unwrap();
    assert_eq!(de.pocket, crate::qto::floor_qto(&m, eg).unwrap().bearing);
    let taschen: f64 = rows
        .iter()
        .filter(|r| r.category == Category::ExteriorWall && r.storey == de.storey)
        .map(|r| r.pocket)
        .sum();
    assert!((taschen - de.pocket).abs() < 1.0, "{taschen} {}", de.pocket);
    assert!(rows
        .iter()
        .filter(|r| r.element == slab)
        .all(|r| r.pocket == 0.0));

    // Umfang Projekt und Gebäude ohne Abwahl: unverändert
    let b = s.buildings[0].id;
    let alle = crate::qto::Umfang::default();
    assert_eq!(s.restrict(&m, &alle), s);
    let gb = crate::qto::Umfang {
        gebaeude: Some(b),
        ohne: vec![],
    };
    assert_eq!(s.restrict(&m, &gb), s);
    // Geschosse einzeln ergeben zusammen dieselben Zeilen und Summen
    let levels: Vec<StoreyId> = s.buildings[0].storeys.iter().map(|x| x.id).collect();
    assert_eq!(levels.len(), 3);
    let mut zeilen = 0;
    let mut schalung = Vec::new();
    let mut gewerk: std::collections::HashMap<crate::trade::TradeId, f64> =
        std::collections::HashMap::new();
    for l in &levels {
        let u = crate::qto::Umfang {
            gebaeude: Some(b),
            ohne: levels.iter().copied().filter(|x| x != l).collect(),
        };
        let t = s.restrict(&m, &u);
        assert_eq!(t.buildings.len(), 1);
        let tb = &t.buildings[0];
        assert!(tb.storeys.iter().all(|x| x.id == *l));
        zeilen += tb.by_kg.iter().map(|k| k.rows.len()).sum::<usize>();
        for ts in &tb.by_trade {
            *gewerk.entry(ts.trade).or_default() += ts.volume;
        }
        schalung.extend(t.formwork);
    }
    assert_eq!(zeilen, rows.len());
    assert_eq!(schalung, s.formwork);
    for ts in &s.buildings[0].by_trade {
        assert!(
            (gewerk[&ts.trade] - ts.volume).abs() < 1.0,
            "{:?}",
            ts.trade
        );
    }
    // Grundfläche (verwaltung §9, Entscheid Architektur 17:20): je
    // Geschoss der Kernumriss seiner eigenen Außenwände. EG 9,72 × 7,72,
    // OG mit Rücksprung Nord 1,50 nur 9,72 × 6,22: Terrasse und die
    // OG-Dämmung auf der Decke zählen nicht (vorher 2 × 75,0384)
    assert_eq!(r4(s.floor_area(&m) / 1e6), r4(75.0384 + 60.4584));
    assert_eq!(r4(m.core_area(eg).unwrap() / 1e6), 75.0384);
    assert_eq!(r4(m.core_area(og).unwrap() / 1e6), 60.4584);
    for n in [1u8, 3] {
        let mut mn = Model::with_seed(12);
        let bn = mn.add_building(n);
        let pts = [
            vec3(0.0, 0.0, 0.0),
            vec3(0.0, 8000.0, 0.0),
            vec3(10000.0, 8000.0, 0.0),
            vec3(10000.0, 0.0, 0.0),
        ];
        mn.build_from_polygon(bn, &pts).unwrap();
        let a = crate::qto::schedule(&mn).floor_area(&mn);
        assert_eq!(r4(a / 1e6), r4(f64::from(n) * 75.0384), "{n}");
    }
    // RH-3: OG 300 mm nach Norden versetzt. Die Decke über EG trägt die
    // Auskragung (9,76 × 0,30 = 2,928 m²); jedes Geschoss zählt nur
    // seinen eigenen Kernumriss 9,76 × 7,76
    let rh3 = crate::szo::read(
        include_str!("../../sk-cost/referenz/rh3-versatz-dachterrasse.szo"),
        GuidGen::with_seed(3),
    )
    .unwrap()
    .model;
    let zuege: Vec<RunId> = rh3
        .runs()
        .ids()
        .filter(|r| rh3.run(*r).is_some_and(|x| x.closed))
        .collect();
    assert_eq!(zuege.len(), 2);
    for r in &zuege {
        assert_eq!(r4(rh3.core_area(*r).unwrap() / 1e6), 75.7376);
    }
    let unten = zuege
        .iter()
        .copied()
        .find(|r| rh3.run_below(*r).is_none())
        .unwrap();
    let decke = rh3.floor(unten).unwrap().unwrap().area();
    assert_eq!(r4(decke / 1e6), r4(75.7376 + 2.928));
    let s3 = crate::qto::schedule(&rh3);
    assert_eq!(r4(s3.floor_area(&rh3) / 1e6), r4(2.0 * 75.7376));
    // Anderes Gebäude: nichts aus diesem
    let mut m2 = m.clone();
    let b2 = m2.add_building(1);
    let u = crate::qto::Umfang {
        gebaeude: Some(b2),
        ohne: vec![],
    };
    let t = crate::qto::schedule(&m2).restrict(&m2, &u);
    assert!(t.formwork.is_empty());
    assert!(t.buildings.iter().all(|x| x.id == b2));
}

/// Z4/T6: Ein Schritt rechnet abgeleitete Bauteile nur an den Zügen, die
/// er berührt; das Ergebnis gleicht dem Abgleich über das ganze Modell.
#[test]
fn abgleich_nur_an_den_zuegen_des_schritts() {
    let (mut m, eg, og) = gebaeude();
    let b2 = m.add_building(2);
    let pts = [
        vec3(20000.0, 0.0, 0.0),
        vec3(20000.0, 8000.0, 0.0),
        vec3(30000.0, 8000.0, 0.0),
        vec3(30000.0, 0.0, 0.0),
    ];
    let eg2 = m.build_from_polygon(b2, &pts).unwrap();
    let og2 = m.runs_above(eg2)[0];
    let ganz = |m: &Model| {
        let mut x = m.clone();
        x.sync_edge_strips(None);
        x.sync_soffits();
        let t = x.sync_terraces();
        (t, x.elements.len() == m.elements.len())
    };
    for (run, k, d) in [
        (og, 1, -1500.0),
        (og2, 2, 300.0),
        (og, 1, 300.0),
        (og, 1, -800.0),
    ] {
        let w = m.wall_at(run, k).unwrap();
        m.begin("Wand verschoben");
        m.set_linked(w, false);
        assert!(m.set_offset(w, d).is_some());
        let scope = m.sync_scope().expect("nur Züge und Bauteile");
        let (this, other) = if run == og {
            ([eg, og], [eg2, og2])
        } else {
            ([eg2, og2], [eg, og])
        };
        assert!(this.iter().all(|r| scope.contains(r)), "{scope:?}");
        assert!(other.iter().all(|r| !scope.contains(r)), "{scope:?}");
        m.commit();
        assert_eq!(ganz(&m), ((0, 0), true), "{k} {d}");
        assert!(m.check().is_empty(), "{:?}", m.check());
    }
    let de = m.floor_of(eg).unwrap();
    assert!(m.terrace_of(de).is_some() && m.coping_of(de).is_some());
    assert!(m.soffit_of(m.floor_of(eg2).unwrap()).is_some());
    // Typänderung: Abgleich über alles
    let t = m.type_by_guid(MONO_TYPE_GUID).unwrap();
    m.begin("Typ");
    let mut x = m.layer_set(t).unwrap().clone();
    x.note = "geändert".into();
    assert!(m.set_layer_set(t, x));
    assert!(m.sync_scope().is_none());
    m.commit();
    // Reine Darstellung: kein Zug, kein Abgleich (T8)
    let (pid, pen) = m
        .attr()
        .pens()
        .iter()
        .next()
        .map(|(id, p)| (id, p.clone()))
        .unwrap();
    m.begin("Stift");
    assert!(m.set_pen(
        pid,
        crate::Pen {
            color: [255, 0, 0],
            ..pen
        }
    ));
    assert_eq!(m.sync_scope(), Some(Vec::new()));
    m.commit();
    assert!(m.check().is_empty(), "{:?}", m.check());
}

/// AW-49: Verblender und Kerndämmung wachsen um die Attika, der Kern
/// bleibt (A195); monolithisch läuft die Zone des Randdämmstreifens hoch.
#[test]
fn attika_mehrschalig_und_monolithisch() {
    for (guid, grows) in [(CAVITY_TYPE_GUID, vec![0, 2]), (MONO_TYPE_GUID, vec![])] {
        let base = |rueck: f64| {
            let (mut m, eg, og) = gebaeude();
            let t = m.type_by_guid(guid).unwrap();
            m.begin("Wandtyp");
            assert!(m.set_run_type(eg, t));
            assert!(m.set_run_type(og, t));
            m.commit();
            if rueck != 0.0 {
                let w = m.wall_at(og, 1).unwrap();
                m.begin("Wand verschoben");
                assert!(m.set_linked(w, false));
                assert!(m.set_offset(w, rueck).is_some());
                m.commit();
            }
            assert!(m.check().is_empty(), "{:?}", m.check());
            (m, eg)
        };
        let (a, ea) = base(0.0);
        let (b, eb) = base(-1500.0);
        let (qa, qb) = (run_qto(&a, ea), run_qto(&b, eb));
        let sum = |q: &[WallQto], i: usize| q.iter().map(|w| w.layers[i].volume).sum::<f64>();
        for i in 0..qa[0].layers.len() {
            if grows.contains(&i) {
                assert!(sum(&qb, i) > sum(&qa, i) + 1.0, "{guid:?} Schicht {i}");
            } else {
                assert!(near(sum(&qb, i), sum(&qa, i)), "{guid:?} Schicht {i}");
            }
        }
        let de = b.floor_of(eb).unwrap();
        assert!(b.terrace_of(de).is_some());
        assert!(b.chain(eb).unwrap().solid().bounds().unwrap().1.z > 3054.0);
    }
}

#[test]
fn dachterrasse_ueber_eck_und_ringsum() {
    // Nord −1,50 und Ost −1,00 gelöst: eine Terrasse über Eck
    let (mut m, eg, og) = gebaeude();
    for (k, d) in [(1, -1500.0), (2, -1000.0)] {
        let w = m.wall_at(og, k).unwrap();
        assert!(m.set_linked(w, false));
        assert!(m.set_offset(w, d).is_some());
    }
    let t = m.terrace_outlines(eg);
    assert_eq!(t.len(), 1);
    assert_eq!(t[0].segments, vec![1, 2]);
    // Deckenumriss 9,72 × 7,72 minus der Teil des OG-Außenumrisses
    // darin (x 0,14 … 9,00, y 0,14 … 6,50): Nord 1,36 tief, Ost 0,86
    let want = 9720.0 * 7720.0 - (9000.0 - 140.0) * (6500.0 - 140.0);
    assert!(near(t[0].area(), want), "{} {want}", t[0].area());
    // Nord und Süd: zwei Terrassen
    let (mut m, eg, og) = gebaeude();
    for k in [1, 3] {
        let w = m.wall_at(og, k).unwrap();
        assert!(m.set_linked(w, false));
        assert!(m.set_offset(w, -1000.0).is_some());
    }
    let t = m.terrace_outlines(eg);
    assert_eq!(t.len(), 2);
    assert!(t.iter().all(|t| near(t.area(), 9720.0 * 860.0)));
    // Ringsum −1,00: ein Ring aus zwei Teilen, Fläche = Decke − OG-Umriss
    let (mut m, eg, og) = gebaeude();
    for k in 0..4 {
        let w = m.wall_at(og, k).unwrap();
        assert!(m.set_linked(w, false));
        assert!(m.set_offset(w, -1000.0).is_some());
    }
    let t = m.terrace_outlines(eg);
    assert_eq!((t.len(), t[0].parts.len()), (1, 2));
    assert_eq!(t[0].segments, vec![0, 1, 2, 3]);
    let want = 9720.0 * 7720.0 - 8000.0 * 6000.0;
    assert!(near(t[0].area(), want), "{} {want}", t[0].area());
}

#[test]
fn rest_unter_2_cm_ist_kein_vorsprung() {
    // Alte Datei: 10 mm Versatz gespeichert. Decke, UD und Außenschichten
    // bleiben wie bündig.
    let (m, eg, _) = mit_versatz(10.0);
    assert!(m.chain(eg).unwrap().joints.overhang.is_none());
    assert!(m.soffit_floors().is_empty());
    let f = m.floor(eg).unwrap().unwrap();
    assert!(near(f.area(), 9720.0 * 7720.0), "{}", f.area());
    // Zurückgesetzt um 10 mm: der Randstreifen gilt als gedeckt
    let (m, eg, og) = mit_versatz(-10.0);
    assert!(m.covered_segments(eg).iter().all(|c| *c));
    assert!(m.open_segments(og).iter().all(|o| !*o));
}

#[test]
fn vorsprung_verlaengert_decke_untersicht_und_daemmung() {
    let (m, eg, _) = mit_versatz(300.0);
    // Decke bis Außenseite Kern OG: y = 8,30 − 0,14
    let f = m.floor(eg).unwrap().unwrap();
    let ymax = f.outline.iter().map(|p| p.y).fold(f64::MIN, f64::max);
    assert!(near(ymax, 8160.0), "{ymax}");
    assert!(near(f.area(), 9720.0 * 8020.0), "{}", f.area());
    // Untersichtdämmung 12 cm unter dem auskragenden Streifen, zwischen
    // den Kernen von EG und OG
    let sp = f.soffit.unwrap();
    assert!(near(sp.thickness, 120.0));
    assert_eq!(f.soffit_band(), Some((2515.0, 2635.0)));
    assert!(near(f.soffit_area(), 9720.0 * 300.0), "{}", f.soffit_area());
    // als eigenes Bauteil UD unter DE-001 (BIM Regel 35)
    let fl = m.floors_of(eg)[0];
    let ud = m.soffit_of(fl).unwrap();
    assert_eq!(m.element(ud).unwrap().number, "UD-001");
    let q = soffit_qto(&m, ud).unwrap();
    assert!(near(q.area, 9720.0 * 300.0));
    assert!(near(q.volume, 9720.0 * 300.0 * 120.0));
    assert!(m.can_delete(ud).is_err());
    // Dämmung der EG-Wände ab UK Untersichtdämmung in der Lage des OG
    let c = m.chain(eg).unwrap();
    let o = c.joints.overhang.clone().unwrap();
    assert_eq!(o.offsets, vec![0.0, 300.0, 0.0, 0.0]);
    assert_eq!((o.from, o.to), (2515.0, 2855.0));
    assert_eq!(
        c.layer_parts(0),
        vec![(0.0, 2515.0, false), (2515.0, 2855.0, true)]
    );
    // Kern: Tasche wie bisher
    assert_eq!(c.layer_parts(1), vec![(0.0, 2635.0, false)]);
    // Mengen (BIM): die herabgezogenen Abschnitte zählen zur OG-Wand.
    // EG-Nordwand: Dämmung nur bis UK Untersicht; EG-Westwand bleibt ganz
    let (w, o) = (run_qto(&m, eg), run_qto(&m, og_of(&m, eg)));
    let nord = (10000.0 + 9720.0) * 0.5 * 140.0;
    let west = (8000.0 + 7720.0) * 0.5 * 140.0;
    assert!(near(w[1].layers[0].volume, nord * 2515.0));
    assert!(near(w[0].layers[0].volume, west * 2855.0));
    assert!(near(w[0].layers[0].side_area, 8000.0 * 2855.0));
    assert!(near(w[1].layers[1].pocket, w[1].layers[1].area * 220.0));
    // OG-Nordwand: dazu der ganze Abschnitt (gleich groß, 0,34 m hoch);
    // OG-Westwand: das Eckstück über dem Vorsprung (0,30 × 0,14 × 0,34)
    let og_west = (8300.0 + 8020.0) * 0.5 * 140.0;
    // … und 70 mm vor die Bekleidung der Untersicht (W3, Jörn 14:03)
    assert!(near(
        o[1].layers[0].volume,
        nord * 2855.0 + nord * 340.0 + nord * 70.0
    ));
    assert!(near(
        o[0].layers[0].volume,
        og_west * 2855.0 + 300.0 * 140.0 * 340.0
    ));
    assert!(near(
        o[0].layers[0].side_area,
        8300.0 * 2855.0 + 300.0 * 340.0
    ));
    let sum = |q: &[WallQto]| q.iter().map(|x| x.layers[0].volume).sum::<f64>();
    assert!(near(
        sum(&w),
        nord * 2515.0 + nord * 2855.0 + 2.0 * west * 2855.0
    ));
    // Ohne Verblender keine Abfangung
    assert!(o.iter().all(|x| x.facing_support == 0.0));
}

fn og_of(m: &Model, eg: RunId) -> RunId {
    m.runs_above(eg)[0]
}

#[test]
fn verblender_wird_abgefangen() {
    let (mut m, eg, og) = gebaeude();
    // AW-49 (Verblender 11,5, Luft 6, Dämmung 14, Kern 17,5) auf alle Wände
    let t = m
        .layer_sets
        .iter()
        .find(|(_, x)| x.guid == CAVITY_TYPE_GUID)
        .map(|(id, _)| id)
        .unwrap();
    for r in [eg, og] {
        for w in m.run(r).unwrap().segments.clone() {
            m.elements.get_mut(w).unwrap().layer_set = Some(t);
        }
    }
    let w = m.wall_at(og, 1).unwrap();
    let below = m.wall_at(eg, 1).unwrap();
    set_coupling(
        &mut m,
        w,
        Some(Coupling {
            below,
            offset: 300.0,
            linked: true,
        }),
    );
    m.carry_stack(eg);
    m.sync_soffits();
    m.sync_terraces();
    assert!(m.check().is_empty(), "{:?}", m.check());
    let o = run_qto(&m, og);
    // Nordwand: Achse des Verblenders über die ganze Länge (10,00 − 0,115);
    // Seitenwände: je das Eckstück 0,30 m
    assert!(
        near(o[1].facing_support, 10000.0 - 115.0),
        "{}",
        o[1].facing_support
    );
    assert!(near(o[0].facing_support, 300.0), "{}", o[0].facing_support);
    assert!(near(o[2].facing_support, 300.0));
    assert!(near(o[3].facing_support, 0.0));
    // Untersicht aus der Dämmschicht der OG-Wand (Kerndämmung)
    let f = m.floor(eg).unwrap().unwrap();
    let ins = m.layer_set(t).unwrap().layers[2].material;
    assert_eq!(f.soffit.unwrap().mat, material_key(ins));
}

#[test]
fn putz_und_duenne_schalen_werden_nicht_abgefangen() {
    // Vorsatzschale aus Putz (z. B. WDVS mit Oberputz) oder unter 7 cm:
    // keine Abfangung; Mauerwerk ab 7 cm schon (BIM, Review 1t T5)
    let plaster = |m: &Model| {
        m.materials
            .iter()
            .find(|(_, x)| x.category == MatCategory::Plaster)
            .map(|(id, _)| id)
            .unwrap()
    };
    for (putz, dicke, soll) in [(true, 115.0, 0.0), (false, 60.0, 0.0), (false, 70.0, 300.0)] {
        let (mut m, eg, og) = gebaeude();
        let t = m
            .layer_sets
            .iter()
            .find(|(_, x)| x.guid == CAVITY_TYPE_GUID)
            .map(|(id, _)| id)
            .unwrap();
        let p = plaster(&m);
        let l = &mut m.layer_sets.get_mut(t).unwrap().layers[0];
        if putz {
            l.material = p;
        }
        l.thickness = dicke;
        for r in [eg, og] {
            for w in m.run(r).unwrap().segments.clone() {
                m.elements.get_mut(w).unwrap().layer_set = Some(t);
            }
        }
        let w = m.wall_at(og, 1).unwrap();
        let below = m.wall_at(eg, 1).unwrap();
        set_coupling(
            &mut m,
            w,
            Some(Coupling {
                below,
                offset: 300.0,
                linked: true,
            }),
        );
        m.carry_stack(eg);
        m.sync_soffits();
        m.sync_terraces();
        let o = run_qto(&m, og);
        assert!(
            near(o[0].facing_support, soll),
            "{putz} {dicke}: {}",
            o[0].facing_support
        );
    }
}

/// Kanten auf der Höhe `z`, die auf der Geraden x = `x` liegen: Länge.
fn kante_bei_x(s: &Solid, z: f64, x: f64) -> f64 {
    s.edges
        .iter()
        .filter(|e| near(e.a.z, z) && near(e.b.z, z) && near(e.a.x, x) && near(e.b.x, x))
        .map(|e| (e.b - e.a).length())
        .sum()
}

#[test]
fn vorsprung_ohne_naht_auf_den_buendigen_seiten() {
    let (m, eg, _) = mit_versatz(300.0);
    let c = m.chain(eg).unwrap();
    let s = c.solid();
    // Westwand außen: unter dem Vorsprung nur das herabgezogene Stück
    // (y 8,00 … 8,30), sonst läuft die Dämmung ohne Naht durch
    assert!(
        near(kante_bei_x(&s, 2515.0, 0.0), 300.0),
        "{}",
        kante_bei_x(&s, 2515.0, 0.0)
    );
    // Nordseite: Unterkante der herabgezogenen Dämmung bei y = 8,30 (sie
    // läuft 70 mm unter die Untersicht, vor die Bekleidung, Jörn 14:03)
    // und EG-Dämmung unter der Untersicht bei y = 8,00
    let north = |y: f64| -> f64 {
        let z = if y > 8000.0 { 2445.0 } else { 2515.0 };
        s.edges
            .iter()
            .filter(|e| near(e.a.z, z) && near(e.b.z, z))
            .filter(|e| near(e.a.y, y) && near(e.b.y, y))
            .map(|e| (e.b - e.a).length())
            .sum()
    };
    assert!(north(8300.0) >= 10000.0 - 1e-3, "{}", north(8300.0));
    assert!(north(8000.0) >= 10000.0 - 1e-3, "{}", north(8000.0));
    // Schnitt durch die Westwand (Ebene y = 4 m): ohne Querlinie bei UK
    // Untersicht; durch die Nordwand (x = 5 m) mit Untersicht und Stufe
    let cw = c.section_caps(vec3(0.0, 4000.0, 0.0), vec3(0.0, 1.0, 0.0));
    assert!(near(kante_bei_x(&cw, 2515.0, 0.0), 0.0));
    assert!(cw
        .edges
        .iter()
        .all(|e| !(near(e.a.z, 2515.0) && near(e.b.z, 2515.0))));
    let cn = c.section_caps(vec3(5000.0, 0.0, 0.0), vec3(1.0, 0.0, 0.0));
    let at = |z: f64| {
        cn.edges
            .iter()
            .filter(|e| near(e.a.z, z) && near(e.b.z, z))
            .count()
    };
    // Oberkante der EG-Dämmung bei UK Untersicht, Unterkante der
    // herabgezogenen Dämmung 70 mm tiefer vor der Bekleidung
    assert!(at(2515.0) >= 1, "{}", at(2515.0));
    assert!(at(2445.0) >= 1, "{}", at(2445.0));
    let f = m.floor(eg).unwrap().unwrap();
    let fc = f.soffit_section_caps(vec3(5000.0, 0.0, 0.0), vec3(1.0, 0.0, 0.0));
    let soffit = fc
        .triangles
        .iter()
        .filter(|t| t.p.iter().all(|p| p.z <= 2635.0 + 1e-6))
        .count();
    assert!(soffit > 0);
    // Dämmschraffur längs der Schicht (Jörn 05:41): v quer über die Dicke
    // (unten 0, oben 1), u läuft waagerecht; darunter die Bekleidung (W3)
    let ud = f.soffit.unwrap().mat | material::CUT;
    for t in fc.triangles.iter().filter(|t| t.mat == ud) {
        for (p, uv) in t.p.iter().zip(t.uv) {
            let v = if near(p.z, 2515.0) { 0.0 } else { 1.0 };
            assert!(near(uv[1], v), "{p:?} {uv:?}");
        }
    }
}

#[test]
fn ruecksprung_laesst_decke_und_daemmung_stehen() {
    let (bm, beg, _) = mit_versatz(0.0);
    let (m, eg, og) = mit_versatz(-300.0);
    let (f, bf) = (
        m.floor(eg).unwrap().unwrap(),
        bm.floor(beg).unwrap().unwrap(),
    );
    assert_eq!(f.outline, bf.outline);
    assert!(f.soffit.is_none() && f.soffits.is_empty());
    assert!(m.chain(eg).unwrap().joints.overhang.is_none());
    // Der Kern bleibt; die Außenschicht wächst nur um die Attika (D2)
    let (q, bq) = (run_qto(&m, eg), run_qto(&bm, beg));
    for (a, b) in q.iter().zip(&bq) {
        assert_eq!(a.layers[1], b.layers[1]);
    }
    assert!(q[1].layers[0].volume > bq[1].layers[0].volume);
    assert_eq!(q[3], bq[3], "Süd ohne Attika");
    // Das OG steht auf der Decke; dort gibt es nichts herabzuziehen
    let o = m.chain(og).unwrap();
    assert!(o.joints.overhang.is_none());
}

#[test]
fn untersicht_einstellbar_und_rueckgaengig_als_ganzes() {
    let (mut m, eg, og) = gebaeude();
    let flaeche = m.floor(eg).unwrap().unwrap().area();
    // OG-Nordwand lösen und 0,30 m vorziehen: ein Schritt
    let w = m.wall_at(og, 1).unwrap();
    m.begin("Vorziehen");
    note!(m, Element, m.elements, w);
    set_coupling(&mut m, w, None);
    let out = {
        let c = m.chain(og).unwrap();
        let c = c.with_segment_moved(1, c.outward_sign() * 300.0).unwrap();
        m.set_run_points(og, &c.points).unwrap()
    };
    // Die EG-Wand darunter ändert ihren Körper mit
    assert!(out.contains(&eg), "{out:?}");
    let t = m.commit().unwrap();
    assert!(m.chain(eg).unwrap().joints.overhang.is_some());
    assert!(m.floor(eg).unwrap().unwrap().area() > flaeche + 1.0);
    // UD entsteht im selben Schritt
    let fl = m.floors_of(eg)[0];
    let ud = m.soffit_of(fl).unwrap();
    assert!(m.check().is_empty(), "{:?}", m.check());
    // Dicke 40 … 300 mm, nie 0 (Regel 35); 20 cm: die Schichten reichen tiefer
    m.begin("Untersicht");
    assert!(!m.set_floor_soffit(fl, 0.0));
    assert!(!m.set_floor_soffit(fl, MIN_SOFFIT - 1.0));
    assert!(!m.set_floor_soffit(fl, MAX_SOFFIT + 1.0));
    assert!(m.set_floor_soffit(fl, 200.0));
    let t2 = m.commit().unwrap();
    assert_eq!(m.chain(eg).unwrap().joints.overhang.unwrap().from, 2435.0);
    assert_eq!(m.soffit_of(fl), Some(ud), "bleibt mit Nummer");
    let touched = m.apply(&t2, Direction::Undo);
    assert!(touched.runs.contains(&eg));
    assert_eq!(m.chain(eg).unwrap().joints.overhang.unwrap().from, 2515.0);
    // Rückgängig: alles wieder bündig, Decke wie vorher, keine UD
    let touched = m.apply(&t, Direction::Undo);
    assert!(touched.runs.contains(&eg), "{:?}", touched.runs);
    assert!(m.chain(eg).unwrap().joints.overhang.is_none());
    assert!(near(m.floor(eg).unwrap().unwrap().area(), flaeche));
    assert!(m.soffit_of(fl).is_none());
    assert!(m.check().is_empty(), "{:?}", m.check());
    m.apply(&t, Direction::Redo);
    assert!(m.chain(eg).unwrap().joints.overhang.is_some());
    assert_eq!(m.soffit_of(fl), Some(ud));
}

#[test]
fn untersicht_dicke_in_der_datei() {
    let (mut m, eg, _) = mit_versatz(300.0);
    let fl = m.floors_of(eg)[0];
    m.allow_unstepped();
    assert!(m.set_floor_soffit(fl, 160.0));
    let text = crate::szo::write(&m);
    assert!(text.contains("soffit=160"), "{text}");
    assert!(
        text.contains("[soffit]") || text.contains("soffit guid"),
        "{text}"
    );
    let back = crate::szo::read(&text, crate::GuidGen::with_seed(3))
        .unwrap()
        .model;
    let eg2 = back
        .runs
        .ids()
        .find(|r| back.run_below(*r).is_none() && !back.runs_above(*r).is_empty())
        .unwrap();
    assert_eq!(
        back.floor(eg2).unwrap().unwrap().soffit.unwrap().thickness,
        160.0
    );
    assert!(back.check().is_empty(), "{:?}", back.check());
    let ud = |b: &Model| {
        b.elements()
            .iter()
            .find(|(_, e)| e.category == Category::SoffitInsulation)
            .map(|(_, e)| (e.guid, e.number.clone()))
    };
    assert_eq!(ud(&back), ud(&m), "Guid und Nummer bleiben");
    // Ohne Schlüssel (Datei vor K4): 12 cm; Standarddicke schreibt keinen
    let old = text.replace(" soffit=160", "");
    let back = crate::szo::read(&old, crate::GuidGen::with_seed(3))
        .unwrap()
        .model;
    let eg3 = back
        .runs
        .ids()
        .find(|r| back.run_below(*r).is_none() && !back.runs_above(*r).is_empty())
        .unwrap();
    assert_eq!(
        back.floor(eg3).unwrap().unwrap().soffit.unwrap().thickness,
        120.0
    );
}
/// Lösen, Versatz, Einrasten unter 2 cm, wieder koppeln (Versatz bleibt),
/// bündig setzen; gekoppelt zieht der Fuß der Kette (OG Phase 2).
#[test]
fn koppeln_versatz_buendig() {
    let (mut m, eg, og) = gebaeude();
    let w = m.wall_at(og, 1).unwrap();
    let p = m.wall_at(eg, 1).unwrap();
    assert_eq!(m.stack_offset(p), None, "EG hat keinen Partner");
    assert_eq!(m.chain_foot(w), p);
    assert!(m.set_linked(w, false));
    assert_eq!(m.chain_foot(w), w);
    assert!(m.move_segment(w, -300.0).is_some());
    assert_eq!(m.stack_offset(w), Some((-300.0, false)));
    assert_eq!(ys(&m, og), (7700.0, 7700.0));
    m.sync_terraces();
    assert!(m.check().is_empty(), "{:?}", m.check());
    // EG +1,00: gelöstes OG bleibt, Versatz −1300
    assert!(drag(&mut m, eg, 1, 1000.0));
    assert_eq!(m.stack_offset(w), Some((-1300.0, false)));
    assert_eq!(ys(&m, og), (7700.0, 7700.0));
    // Koppeln: Versatz bleibt, EG −1,00 nimmt das OG mit
    assert!(m.set_linked(w, true));
    assert!(drag(&mut m, eg, 1, -1000.0));
    assert_eq!(m.stack_offset(w), Some((-1300.0, true)));
    assert_eq!(ys(&m, og), (6700.0, 6700.0));
    // 1 cm rastet auf 0
    assert!(m.set_offset(w, 10.0).is_some());
    assert_eq!(m.stack_offset(w), Some((0.0, true)));
    assert!(m.move_segment(w, 300.0).is_some());
    assert!(m.set_flush(w));
    assert_eq!(m.stack_offset(w), Some((0.0, true)));
    assert_eq!(m.run(og).unwrap().points, m.run(eg).unwrap().points);
    m.sync_soffits();
    m.sync_terraces();
    assert!(m.check().is_empty(), "{:?}", m.check());
}

/// Gelöste Segmente schreiben `link=0`, sonst bleibt die Datei gleich;
/// `link=2` wird abgelehnt.
#[test]
fn datei_link() {
    let (mut m, _, og) = gebaeude();
    let plain = crate::szo::write(&m);
    assert!(!plain.contains("link="));
    let w = m.wall_at(og, 1).unwrap();
    m.set_linked(w, false);
    let text = crate::szo::write(&m);
    assert_eq!(text.matches(" link=0").count(), 1);
    let back = crate::szo::read(&text, GuidGen::with_seed(5))
        .unwrap()
        .model;
    assert_eq!(crate::szo::write(&back), text);
    let bad = text.replace(" link=0", " link=2");
    let e = crate::szo::read(&bad, GuidGen::with_seed(5)).err().unwrap();
    assert!(format!("{e:?}").contains("link"), "{e:?}");
}
