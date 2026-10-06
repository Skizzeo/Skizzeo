//! Bauteiltypen (K1): Typwechsel, Prüfregeln, Merkmale, Umstellung alter
//! Dateien. Prüfhaus wie A39: 10 × 8 m, zwei Geschosse, Innenwand bei x = 5 m.

use crate::element::{Category, PropValue, RunId};
use crate::library::{LayerSet, LayerSetId, TypeCategory};
use crate::model::{Model, EXTERIOR_TYPE_GUID, INTERIOR_TYPE_GUID};
use crate::txn::Direction;
use crate::wall::RefSide;
use crate::{floor_qto, foundation_qto, run_qto, szo, GuidGen};
use sk_math::vec3;

fn haus() -> (Model, RunId, RunId) {
    let mut m = Model::with_seed(39);
    let b = m.add_building(2);
    let pts = [
        vec3(0.0, 0.0, 0.0),
        vec3(0.0, 8000.0, 0.0),
        vec3(10000.0, 8000.0, 0.0),
        vec3(10000.0, 0.0, 0.0),
    ];
    let aw = m.build_from_polygon(b, &pts).unwrap();
    let eg = m.run(aw).unwrap().storey;
    let set = m.defaults().interior_wall;
    let iw = m
        .add_wall_run(
            &[vec3(5000.0, 0.0, 0.0), vec3(5000.0, 8000.0, 0.0)],
            false,
            RefSide::Center,
            eg,
            set,
            Category::InteriorWall,
        )
        .unwrap();
    (m, aw, iw)
}

fn og(m: &Model, aw: RunId) -> RunId {
    m.runs_above(aw)[0]
}

/// Dämmung, Gasbeton netto (EG-Außenwand), Innenwand, Decke, Sohlplatte,
/// Frostschürze in m³, gerundet auf 4 Stellen wie A39.
fn mengen(m: &Model, aw: RunId, iw: RunId) -> [f64; 6] {
    let r = |v: f64| (v / 1e9 * 1e4).round() / 1e4;
    let q = run_qto(m, aw);
    let schicht = |k: usize| r(q.iter().map(|w| w.layers[k].volume).sum());
    let (sp, fs) = foundation_qto(m, aw).unwrap();
    [
        schicht(0),
        schicht(1),
        r(run_qto(m, iw)[0].volume),
        r(floor_qto(m, aw).unwrap().volume),
        r(sp.volume),
        r(fs.volume),
    ]
}

const STANDARD: [f64; 6] = [14.1654, 15.7613, 3.3985, 16.5084, 17.6, 7.0238];
/// Nach dem Typwechsel auf 12 Dämmung + 24 Gasbeton (BIM, main 1f0763e).
const AW_36: [f64; 6] = [12.1692, 21.5522, 3.357, 16.6623, 17.6, 7.0238];

/// 12 cm Dämmung + 24 cm Gasbeton aus dem Werkstyp.
fn aw_36(m: &Model) -> LayerSet {
    let mut t = m.layer_set(m.defaults().exterior_wall).unwrap().clone();
    t.layers[0].thickness = 120.0;
    t.layers[1].thickness = 240.0;
    t
}

fn outer_x(m: &Model, run: RunId) -> f64 {
    let c = m.chain(run).unwrap();
    c.points[0].x + c.outer_offset() * c.segment_normal(0).unwrap().x
}

#[test]
fn werkstypen_mit_festen_guids_und_kurzzeichen() {
    for m in [Model::new(), Model::with_seed(3)] {
        let aw = m.layer_set(m.defaults().exterior_wall).unwrap();
        let iw = m.layer_set(m.defaults().interior_wall).unwrap();
        assert_eq!((aw.guid, aw.code.as_str()), (EXTERIOR_TYPE_GUID, "AW-31,5"));
        assert_eq!((iw.guid, iw.code.as_str()), (INTERIOR_TYPE_GUID, "IW-17,5"));
        assert_eq!(aw.category, TypeCategory::ExteriorWall);
        assert!(aw.load_bearing() && aw.is_external() && !iw.is_external());
        assert!(m.check().is_empty(), "{:?}", m.check());
    }
    // Neue Projekte: gleiche Bibliothek, eigenes Projekt und eigene Geschosse
    let (a, b) = (Model::new(), Model::new());
    let mats = |m: &Model| {
        m.materials()
            .iter()
            .map(|(_, x)| x.guid)
            .collect::<Vec<_>>()
    };
    assert_eq!(mats(&a), mats(&b));
    assert_ne!(a.project().guid, b.project().guid);
    let st = |m: &Model| m.storeys().iter().map(|(_, s)| s.guid).collect::<Vec<_>>();
    assert!(st(&a).iter().all(|g| !st(&b).contains(g)));
}

#[test]
fn typaenderung_aendert_alle_waende_und_rueckgaengig() {
    let (mut m, aw, iw) = haus();
    assert_eq!(mengen(&m, aw, iw), STANDARD);
    assert!(m.check().is_empty(), "{:?}", m.check());
    let x0 = outer_x(&m, aw);
    let set = m.defaults().exterior_wall;
    m.begin("Typ geändert");
    assert!(m.set_layer_set(set, aw_36(&m)));
    let t = m.commit().unwrap();
    assert_eq!(mengen(&m, aw, iw), AW_36);
    assert_eq!(outer_x(&m, aw), x0, "Außenseite bleibt");
    let f = floor_qto(&m, aw).unwrap();
    assert!((f.area / 1e6 - 75.7376).abs() < 1e-4, "{}", f.area / 1e6);
    assert!(m.check().is_empty(), "{:?}", m.check());
    assert_eq!(m.layer_set(set).unwrap().changed, 2);
    m.apply(&t, Direction::Undo);
    assert_eq!(mengen(&m, aw, iw), STANDARD);
    assert_eq!(m.layer_set(set).unwrap().changed, 1);
}

#[test]
fn zugtyp_wechseln_nimmt_das_obergeschoss_mit() {
    let (mut m, aw, iw) = haus();
    let mut t = aw_36(&m);
    t.guid = m.new_guid();
    (t.code, t.name) = ("AW-36".into(), "AW 36".into());
    let neu = m.add_layer_set(t).unwrap();
    let alt = m.defaults().exterior_wall;
    let x0 = outer_x(&m, aw);
    m.begin("Wandtyp geändert");
    assert!(m.set_run_type(og(&m, aw), neu), "auch vom OG aus");
    let t = m.commit().unwrap();
    assert_eq!(mengen(&m, aw, iw), AW_36);
    assert_eq!(outer_x(&m, aw), x0);
    assert_eq!(m.type_users(neu).len(), 8, "EG und OG");
    assert!(m.type_users(alt).is_empty());
    assert!(m.check().is_empty(), "{:?}", m.check());
    m.apply(&t, Direction::Undo);
    assert_eq!(mengen(&m, aw, iw), STANDARD);
    assert_eq!(m.type_users(alt).len(), 8);
    // Innenwandtyp passt nicht zur Außenwand
    let innen = m.defaults().interior_wall;
    assert!(!m.set_run_type(aw, innen));
    assert!(!m.set_run_type(iw, alt));
    assert_eq!(m.type_users(alt).len(), 8);
}

#[test]
fn aussenseite_bleibt_auch_bei_achse_als_bezug() {
    let mut m = Model::with_seed(5);
    let eg = m.defaults().storey;
    let set = m.defaults().exterior_wall;
    let run = m
        .add_wall_run(
            &[vec3(0.0, 0.0, 0.0), vec3(6000.0, 0.0, 0.0)],
            false,
            RefSide::Center,
            eg,
            set,
            Category::ExteriorWall,
        )
        .unwrap();
    let x = |m: &Model| {
        let c = m.chain(run).unwrap();
        c.points[0].y + c.outer_offset() * c.segment_normal(0).unwrap().y
    };
    let y0 = x(&m);
    let mut t = aw_36(&m);
    t.guid = m.new_guid();
    t.code = "AW-36".into();
    let neu = m.add_layer_set(t).unwrap();
    assert!(m.set_run_type(run, neu));
    assert!((x(&m) - y0).abs() < 1e-9, "{} statt {y0}", x(&m));
    assert_eq!(m.chain(run).unwrap().thickness(), 360.0);
}

#[test]
fn loeschen_kopieren_standard() {
    let (mut m, _, _) = haus();
    let aw = m.defaults().exterior_wall;
    assert_eq!(m.remove_type(aw), Err(8));
    let kopie = m.duplicate_type(aw).unwrap();
    let k = m.layer_set(kopie).unwrap();
    assert_eq!(k.code, "AW-31,5-2");
    assert_eq!(k.name, "AW 31,5 Gasbeton + WDVS (Kopie)");
    assert_ne!(k.guid, EXTERIOR_TYPE_GUID);
    let k2 = m.duplicate_type(aw).unwrap();
    assert_eq!(m.layer_set(k2).unwrap().code, "AW-31,5-3");
    // Standardtyp nicht löschbar, auch ohne Benutzer
    m.begin("Standard");
    assert!(m.set_default_type(TypeCategory::ExteriorWall, kopie));
    assert!(!m.set_default_type(TypeCategory::InteriorWall, kopie));
    let t = m.commit().unwrap();
    assert_eq!(m.remove_type(kopie), Err(0));
    m.apply(&t, Direction::Undo);
    assert_eq!(m.defaults().exterior_wall, aw);
    m.begin("Löschen");
    assert_eq!(m.remove_type(kopie), Ok(()));
    let t = m.commit().unwrap();
    assert!(m.layer_set(kopie).is_none());
    m.apply(&t, Direction::Undo);
    assert_eq!(m.layer_set(kopie).unwrap().code, "AW-31,5-2");
    // Kurzzeichen eindeutig, Guid bleibt
    let mut doppelt = m.layer_set(k2).unwrap().clone();
    doppelt.code = "AW-31,5".into();
    assert!(!m.set_layer_set(k2, doppelt.clone()));
    doppelt.code = "AW-X".into();
    doppelt.guid = m.new_guid();
    assert!(!m.set_layer_set(k2, doppelt.clone()), "Regel 19");
    assert!(m
        .add_layer_set(LayerSet {
            code: "IW-17,5".into(),
            ..doppelt
        })
        .is_none());
    // Art eines benutzten Typs bleibt
    let mut innen = m.layer_set(aw).unwrap().clone();
    innen.category = TypeCategory::InteriorWall;
    assert!(!m.set_layer_set(aw, innen));
    assert!(m.check().is_empty(), "{:?}", m.check());
}

#[test]
fn merkmale_des_typs_und_ueberschreiben() {
    let (mut m, aw, _) = haus();
    let set = m.defaults().exterior_wall;
    let mut t = m.layer_set(set).unwrap().clone();
    t.props
        .insert("Brandschutz".into(), PropValue::Text("F90".into()));
    t.props
        .insert("Schallschutz".into(), PropValue::Text("R'w 53 dB".into()));
    assert!(m.set_layer_set(set, t));
    let w = m.wall_at(aw, 0).unwrap();
    assert!(m.set_prop(w, "Brandschutz", Some(PropValue::Text("F30".into()))));
    let p = m.props_of(w);
    assert_eq!(p["Brandschutz"], PropValue::Text("F30".into()));
    assert_eq!(p["Schallschutz"], PropValue::Text("R'w 53 dB".into()));
    let anderes = m.wall_at(aw, 1).unwrap();
    assert_eq!(
        m.props_of(anderes)["Brandschutz"],
        PropValue::Text("F90".into())
    );
    // Merkmale reisen in der Datei mit
    let text = szo::write(&m);
    assert!(text.contains("[typeprop]"), "{text}");
    let l = szo::read(&text, GuidGen::with_seed(1)).unwrap();
    assert!(l.hints.is_empty(), "{:?}", l.hints);
    assert_eq!(szo::write(&l.model), text);
    let set2 = l.model.type_by_guid(EXTERIOR_TYPE_GUID).unwrap();
    assert_eq!(
        l.model.layer_set(set2).unwrap().props,
        m.layer_set(set).unwrap().props
    );
}

/// Eine Datei aus Version 3 (vor K1): Kurzzeichen aus Art und Dicke, Art
/// aus der Benutzung; die Mengen bleiben.
#[test]
fn alte_datei_bekommt_kurzzeichen() {
    let (m, aw, iw) = haus();
    let v4 = szo::write(&m);
    let v3: String = v4
        .lines()
        .map(|l| {
            if l.starts_with("SZO 4") {
                "SZO 3".to_string()
            } else if l.starts_with("[layerset]") {
                let cut = l.find(" code=").unwrap();
                l[..cut].to_string()
            } else {
                l.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
        + "\n";
    let l = szo::read(&v3, GuidGen::with_seed(2)).unwrap();
    assert!(l.hints.is_empty(), "{:?}", l.hints);
    let n = &l.model;
    let codes: Vec<(String, TypeCategory)> = n
        .layer_sets()
        .iter()
        .map(|(_, t)| (t.code.clone(), t.category))
        .collect();
    assert!(codes.contains(&("AW-31,5".into(), TypeCategory::ExteriorWall)));
    assert!(codes.contains(&("IW-17,5".into(), TypeCategory::InteriorWall)));
    assert_eq!(szo::write(n), v4, "speichert v4, sonst gleich");
    let aw2 = n
        .runs()
        .ids()
        .find(|r| n.run(*r).unwrap().guid == m.run(aw).unwrap().guid);
    let iw2 = n
        .runs()
        .ids()
        .find(|r| n.run(*r).unwrap().guid == m.run(iw).unwrap().guid);
    assert_eq!(mengen(n, aw2.unwrap(), iw2.unwrap()), STANDARD);
}

/// Unbenutzte Typen gleicher Dicke: „-2“ in Guid-Reihenfolge; unbenutzt im
/// Innenwand-Platz der Standardtypen: Innenwandtyp.
#[test]
fn alte_datei_gleiche_dicke() {
    let mut m = Model::with_seed(4);
    let mut t = m.layer_set(m.defaults().exterior_wall).unwrap().clone();
    t.guid = m.new_guid();
    t.code = "X".into();
    let x: LayerSetId = m.add_layer_set(t).unwrap();
    let v3 = szo::write(&m)
        .replacen("SZO 4", "SZO 3", 1)
        .lines()
        .map(|l| match l.find(" code=") {
            Some(c) if l.starts_with("[layerset]") => l[..c].to_string(),
            _ => l.to_string(),
        })
        .collect::<Vec<_>>()
        .join("\n");
    let n = szo::read(&v3, GuidGen::with_seed(2)).unwrap().model;
    let mut codes: Vec<(u128, String)> = n
        .layer_sets()
        .iter()
        .filter(|(_, t)| t.category == TypeCategory::ExteriorWall)
        .map(|(_, t)| (t.guid.0, t.code.clone()))
        .collect();
    codes.sort();
    assert_eq!(codes[0].1, "AW-31,5");
    assert_eq!(codes[1].1, "AW-31,5-2");
    let iw = n.layer_set(n.defaults().interior_wall).unwrap();
    assert_eq!(
        (iw.code.as_str(), iw.category),
        ("IW-17,5", TypeCategory::InteriorWall)
    );
    let _ = x;
}

/// Prüfregeln 16–18 schlagen bei gezielt kaputten Dateien an.
#[test]
fn pruefregeln_bei_kaputten_dateien() {
    let (m, _, _) = haus();
    let text = szo::write(&m);
    let iw = m
        .layer_set(m.defaults().interior_wall)
        .unwrap()
        .guid
        .to_ifc();
    let aw = EXTERIOR_TYPE_GUID.to_ifc();
    let load = |t: &str| szo::read(t, GuidGen::with_seed(9)).unwrap().hints;
    // Doppeltes Kurzzeichen
    let h = load(&text.replace("code=\"IW-17,5\"", "code=\"AW-31,5\""));
    assert!(
        h.iter().any(|x| x.contains("Kurzzeichen AW-31,5 doppelt")),
        "{h:?}"
    );
    // Ein Zug mit zwei Typen: eine Außenwand bekommt einen zweiten Außenwandtyp
    let zweiter = format!(
        "[layerset] guid={} name=\"Zweiter\" code=\"AW-2\" cat=exterior changed=1 note=\"\"\n[layer] set={} mat=",
        crate::Guid(7).to_ifc(),
        crate::Guid(7).to_ifc()
    );
    let layer = text
        .lines()
        .find(|l| l.starts_with("[layer] ") && l.contains(&aw))
        .unwrap();
    let mat = &layer[layer.find("mat=").unwrap() + 4..];
    let mut t2 = text.replacen("[project]", &format!("{zweiter}{mat}\n[project]"), 1);
    let wand = t2
        .lines()
        .find(|l| l.starts_with("[wall]") && l.contains(&aw))
        .unwrap()
        .to_string();
    t2 = t2.replacen(&wand, &wand.replace(&aw, &crate::Guid(7).to_ifc()), 1);
    let h = load(&t2);
    assert!(h.iter().any(|x| x.contains("verschiedenen Typen")), "{h:?}");
    // Falsche Art: Außenwand mit Innenwandtyp
    let t3 = text.replacen(&wand, &wand.replace(&aw, &iw), 1);
    let h = load(&t3);
    assert!(
        h.iter().any(|x| x.contains("passt nicht zur Außenwand")),
        "{h:?}"
    );
}
