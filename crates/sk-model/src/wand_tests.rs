//! Wand-Strang 10.10. (Tagesplan Flachdach W1–W3): Kreuzschraffur und
//! Schaumglas, Fußpunkt der Verblendschale auf der Dachterrasse,
//! Bekleidung unter der Untersichtdämmung.

use crate::attr::{cross_lines, FillKind, CROSS_FILL_NAME};
use crate::element::RunId;
use crate::library::{material_key, MatCategory};
use crate::model::{Model, CAVITY_TYPE_GUID, CROSS_FILL_GUID, FOAMGLASS_MAT_GUID, MONO_TYPE_GUID};
use crate::solid::{material, Solid};
use crate::txn::Direction;
use crate::{run_qto, szo, Guid, GuidGen};
use sk_math::vec3;

fn near(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-3
}

/// Prüfhaus 10 × 8 m, zwei Geschosse, Typ `typ` (Werkstyp-Guid; `None`:
/// AW-31,5 mit WDVS) auf EG und OG; je `(Segment, mm)` die OG-Wand im
/// eigenen Schritt gelöst und versetzt (Nord ist Segment 1).
fn haus(typ: Option<Guid>, versatz: &[(usize, f64)]) -> (Model, RunId, RunId) {
    let mut m = Model::with_seed(102);
    let b = m.add_building(2);
    let pts = [
        vec3(0.0, 0.0, 0.0),
        vec3(0.0, 8000.0, 0.0),
        vec3(10000.0, 8000.0, 0.0),
        vec3(10000.0, 0.0, 0.0),
    ];
    let eg = m.build_from_polygon(b, &pts).unwrap();
    let og = m.runs_above(eg)[0];
    if let Some(g) = typ {
        let t = m.type_by_guid(g).unwrap();
        m.begin("Typ");
        assert!(m.set_run_type(eg, t));
        m.commit();
    }
    for &(k, d) in versatz {
        versetzen(&mut m, og, k, d);
    }
    assert!(m.check().is_empty(), "{:?}", m.check());
    (m, eg, og)
}

fn versetzen(m: &mut Model, og: RunId, k: usize, d: f64) -> Option<crate::txn::Txn> {
    let w = m.wall_at(og, k).unwrap();
    m.begin("Wand verschoben");
    m.set_linked(w, false);
    assert!(m.set_offset(w, d).is_some(), "{k} {d}");
    m.commit()
}

/// Höhenbereich der Dreiecke mit Baustoff `key` (ohne Schnittbit) und die
/// Segmente, zu denen sie gehören.
fn mit_baustoff(s: &Solid, key: u16) -> (Option<(f64, f64)>, Vec<u32>) {
    let mut z: Option<(f64, f64)> = None;
    let mut segs = Vec::new();
    for t in s.triangles.iter().filter(|t| t.mat & !material::CUT == key) {
        for p in &t.p {
            z = Some(z.map_or((p.z, p.z), |(a, b)| (a.min(p.z), b.max(p.z))));
        }
        if !segs.contains(&t.elem) {
            segs.push(t.elem);
        }
    }
    (z, segs)
}

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

/// W2 (Jörn 05:53, Skizze Foamglas.png): Verblender auf der Dachterrasse
/// steht auf Schaumglas bis OK Terrassenaufbau, nur auf dem
/// zurückspringenden Segment; Mengen aus dem Verblender herausgerechnet.
#[test]
fn fusspunkt_schaumglas_unter_dem_verblender() {
    let (m, eg, og) = haus(Some(CAVITY_TYPE_GUID), &[(1, -1500.0)]);
    let mat = m.foamglass_material().expect("Schaumglas angelegt");
    let key = material_key(mat);
    let c = m.chain(og).unwrap();
    let f = c.joints.facing_foot.clone().expect("Fußpunkt");
    // Terrassenaufbau DT-14: 6 + 8 cm über OK EG-Rohdecke
    assert_eq!(f.band, (2855.0, 2995.0));
    assert_eq!(f.segs, vec![false, true, false, false]);
    assert_eq!((f.layer, f.mat), (0, key));
    assert_eq!(
        c.layer_parts(0),
        vec![(2855.0, 2995.0, false), (2995.0, c.top(), false)]
    );
    // Kern, Dämmung: ungeteilt
    assert_eq!(c.layer_parts(2).len(), 1);
    // 3D: Schaumglas nur am Fuß der Nordwand
    let (z, segs) = mit_baustoff(&c.solid(), key);
    assert_eq!(z, Some((2855.0, 2995.0)));
    assert_eq!(segs, vec![1]);
    // Schnitt quer durch die Terrasse (x = 5,00): Kreuzschraffur am Fuß
    let caps = c.section_caps(vec3(5000.0, 0.0, 0.0), vec3(1.0, 0.0, 0.0));
    let (z, segs) = mit_baustoff(&caps, key);
    assert_eq!(z, Some((2855.0, 2995.0)));
    assert_eq!(segs, vec![1]);
    assert!(caps.triangles.iter().any(|t| t.mat == key | material::CUT));
    // EG-Wand bleibt ohne Fußpunkt
    assert!(m.chain(eg).unwrap().joints.facing_foot.is_none());

    // Mengen: Länge in der Außenflucht, 2 Lagen, Volumen aus dem Verblender
    let q = run_qto(&m, og);
    let ff = q[1].facing_foot.clone().expect("Mengen Fußpunkt");
    assert!(near(ff.length, 10000.0), "{}", ff.length);
    assert_eq!((ff.height, ff.width, ff.courses), (140.0, 115.0, 2));
    let quad = (10000.0 + 9770.0) * 0.5 * 115.0;
    assert!(near(ff.volume, quad * 140.0), "{}", ff.volume);
    assert!(near(ff.area, 10000.0 * 140.0));
    assert_eq!(ff.material, mat);
    let v = &q[1].layers[0];
    assert!(near(v.foot, ff.volume));
    assert!(near(v.volume + v.foot, quad * c.height), "{}", v.volume);
    assert!(near(v.side_area, 10000.0 * (c.height - 140.0)));
    let summe: f64 = q[1].layers.iter().map(|l| l.volume + l.foot).sum();
    assert!(near(q[1].volume, summe));
    for k in [0, 2, 3] {
        assert!(q[k].facing_foot.is_none(), "{k}");
        assert_eq!(q[k].layers[0].foot, 0.0);
    }
    // Baustoffsumme Schaumglas
    let sched = crate::qto::schedule(&m);
    let sg = sched.buildings[0]
        .by_material
        .iter()
        .find(|x| x.material == mat)
        .unwrap();
    assert!(near(sg.volume, ff.volume));
    assert!(m.check().is_empty(), "{:?}", m.check());
}

/// W2: monolithisch und mit WDVS nie (kein Verblender), vorspringend und
/// bündig auch nicht; Rückgängig nimmt Fußpunkt und Baustoff mit.
#[test]
fn fusspunkt_nur_bei_verblender_auf_der_terrasse() {
    for typ in [None, Some(MONO_TYPE_GUID)] {
        let (m, _, og) = haus(typ, &[(1, -1500.0)]);
        assert!(m.chain(og).unwrap().joints.facing_foot.is_none());
        assert!(m.foamglass_material().is_none(), "{typ:?}");
        assert!(run_qto(&m, og).iter().all(|w| w.facing_foot.is_none()));
    }
    // AW-49 bündig und vorspringend: kein Fußpunkt, kein Baustoff
    let (mut m, _, og) = haus(Some(CAVITY_TYPE_GUID), &[]);
    assert!(m.foamglass_material().is_none());
    versetzen(&mut m, og, 1, 300.0);
    assert!(m.chain(og).unwrap().joints.facing_foot.is_none());
    assert!(m.foamglass_material().is_none());
    // Rücksprung: entsteht; Rückgängig: beides weg
    let t = versetzen(&mut m, og, 1, -1500.0).unwrap();
    assert!(m.chain(og).unwrap().joints.facing_foot.is_some());
    m.apply(&t, Direction::Undo);
    assert!(m.chain(og).unwrap().joints.facing_foot.is_none());
    assert!(m.foamglass_material().is_none());
    m.apply(&t, Direction::Redo);
    assert!(m.chain(og).unwrap().joints.facing_foot.is_some());
    // Wieder vor: Fußpunkt weg, der Baustoff bleibt im Projekt
    versetzen(&mut m, og, 1, 300.0);
    assert!(m.chain(og).unwrap().joints.facing_foot.is_none());
    assert!(m.foamglass_material().is_some());
    assert!(m.check().is_empty(), "{:?}", m.check());
    // Typwechsel auf WDVS: kein Fußpunkt mehr
    let (mut m, _, og) = haus(Some(CAVITY_TYPE_GUID), &[(1, -1500.0)]);
    let wdvs = m.defaults().exterior_wall;
    m.begin("Typ");
    assert!(m.set_run_type(og, wdvs));
    m.commit();
    assert!(m.chain(og).unwrap().joints.facing_foot.is_none());
    assert!(run_qto(&m, og).iter().all(|w| w.facing_foot.is_none()));
}

/// W2, Datei: nichts Neues außer dem Baustoff; ältere Datei mit Verblender
/// auf der Terrasse bekommt ihn beim Öffnen samt Hinweis.
#[test]
fn fusspunkt_datei() {
    let (m, _, og) = haus(Some(CAVITY_TYPE_GUID), &[(1, -1500.0)]);
    let text = szo::write(&m);
    let read = |t: &str| szo::read(t, GuidGen::with_seed(7)).unwrap();
    let back = read(&text);
    assert!(back.hints.is_empty(), "{:?}", back.hints);
    assert_eq!(szo::write(&back.model), text);
    // Mengen nach dem Lesen gleich (Baustoff-Ids ordnet das Lesen neu)
    type Fuss = Option<(f64, f64, u32)>;
    let werte = |q: Vec<crate::WallQto>| -> Vec<(f64, Fuss)> {
        q.into_iter()
            .map(|w| {
                let f = w.facing_foot.map(|f| (f.length, f.volume, f.courses));
                (w.volume, f)
            })
            .collect()
    };
    assert_eq!(
        werte(run_qto(&back.model, og_of(&back.model))),
        werte(run_qto(&m, og))
    );
    // Stand vor W2: ohne Baustoff, Oberfläche und Schraffur
    let alt: String = text
        .lines()
        .filter(|l| !l.contains("Schaumglas-Dämmstein") && !l.contains(CROSS_FILL_NAME))
        .map(|l| format!("{l}\n"))
        .collect();
    assert!(alt.len() < text.len());
    let back = read(&alt);
    assert!(
        back.hints.iter().any(|h| h.contains("Fußpunkt")),
        "{:?}",
        back.hints
    );
    assert!(back.model.foamglass_material().is_some());
    let og2 = og_of(&back.model);
    assert!(back.model.chain(og2).unwrap().joints.facing_foot.is_some());
    assert!(back.model.check().is_empty(), "{:?}", back.model.check());
    // Gelesen mit der Folge, die die Datei schrieb: die feste Guid der
    // Oberfläche trifft keine schon vergebene (A295 mit p6.szo)
    let back = szo::read(&alt, GuidGen::with_seed(102)).unwrap();
    assert_eq!(
        back.hints,
        vec!["Fußpunkt aus Schaumglas unter dem Verblender ergänzt".to_string()]
    );
    let neu = szo::write(&back.model);
    assert_eq!(szo::write(&read(&neu).model), neu);
}

fn og_of(m: &Model) -> RunId {
    let eg = m
        .runs()
        .iter()
        .find(|(id, _)| !m.runs_above(*id).is_empty())
        .map(|(id, _)| id)
        .unwrap();
    m.runs_above(eg)[0]
}
