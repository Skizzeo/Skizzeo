//! Paket O2 (Jörn 11.10.): Öffnungen im Modell, im Wandkörper und in der
//! Datei; Höhenlinien je Geschoss.

use crate::element::{Category, RunId};
use crate::hoehen::HeightLines;
use crate::model::{Model, CAVITY_TYPE_GUID, MONO_TYPE_GUID};
use crate::oeffnung::{
    HeightLine, HeightRef, Install, InstallDepth, Opening, OpeningKind, REVEAL_ETICS,
};
use crate::solid::Solid;
use crate::txn::Direction;
use crate::wall::right_of;
use crate::{szo, Guid, GuidGen, LevelRef, OpeningFault, StoreyId};
use sk_math::{vec3, Vec3};

/// Prüfhaus 10 × 8 m, zwei Geschosse, Typ `typ` (`None`: WDVS).
/// Segment 0 läuft von (0, 0) nach (0, 8 m), Segment 1 nach (10 m, 8 m).
fn haus(typ: Option<Guid>) -> (Model, RunId, RunId) {
    let mut m = Model::with_seed(211);
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
    (m, eg, og)
}

/// Punkt auf der Außenfläche von Segment `seg` des Zuges, `u` mm ab dem
/// Segmentanfang.
fn aussen(m: &Model, run: RunId, seg: usize, u: f64) -> Vec3 {
    let c = m.chain(run).unwrap();
    let (pts, _, dirs) = c.layout().unwrap();
    pts[seg] + dirs[seg] * u + right_of(dirs[seg]) * c.outer_offset()
}

/// Äußere Ecke zwischen Segment 0 und 1.
fn ecke(m: &Model, run: RunId) -> Vec3 {
    m.chain(run)
        .unwrap()
        .face_corners(m.chain(run).unwrap().outer_offset())[1]
}

fn storey(m: &Model, run: RunId) -> StoreyId {
    m.run(run).unwrap().storey
}

fn fenster(m: &Model, run: RunId, u: (f64, f64)) -> Opening {
    let st = storey(m, run);
    Opening {
        kind: OpeningKind::Window,
        path: vec![aussen(m, run, 0, u.0), aussen(m, run, 0, u.1)],
        bottom: HeightRef::Line(st, HeightLine::Sill),
        top: HeightRef::Line(st, HeightLine::Lintel),
        reveal: None,
        install: Default::default(),
    }
}

/// Fläche der Dreiecke, deren Normale `n` ist.
fn flaeche(s: &Solid, n: Vec3) -> f64 {
    s.triangles
        .iter()
        .filter(|t| (t.n - n).length() < 1e-6)
        .map(|t| (t.p[1] - t.p[0]).cross(t.p[2] - t.p[0]).length() * 0.5)
        .sum()
}

/// Wandöffnung im WDVS: ein Loch im Segment 0, Brüstung und Sturz aus den
/// Höhenlinien, Anschlag 3 cm seitlich und oben in den Schichten hinter
/// dem WDVS; rückgängig ist die Wand wieder geschlossen.
#[test]
fn wandoeffnung_im_wdvs() {
    let (mut m, eg, _) = haus(None);
    let st = storey(&m, eg);
    let z = m.storey(st).unwrap().elevation;
    let vorher = m.chain(eg).unwrap().solid();
    m.begin("Öffnung");
    let o = m.add_opening(fenster(&m, eg, (3000.0, 4250.0))).unwrap();
    let t = m.commit().unwrap();
    assert!(m.check().is_empty(), "{:?}", m.check());
    let e = m.element(o).unwrap();
    assert!(e.number.starts_with("OE-"), "{}", e.number);
    assert_eq!(e.storey, st);
    let c = m.chain(eg).unwrap();
    assert_eq!(c.joints.holes.len(), 1, "{:?}", c.joints.holes);
    let h = &c.joints.holes[0];
    assert_eq!(h.seg, 0);
    assert!((h.u.0 - 3000.0).abs() < 1e-6 && (h.u.1 - 4250.0).abs() < 1e-6);
    assert!((h.z.0 - z - 875.0).abs() < 1e-6 && (h.z.1 - z - 2385.0).abs() < 1e-6);
    assert_eq!(h.reveal_u, (REVEAL_ETICS[0], REVEAL_ETICS[1]));
    assert_eq!(h.reveal_z, (0.0, REVEAL_ETICS[2]));
    assert_eq!(h.shell, 1, "die Dämmung vor dem Kern ist die Außenschale");
    // Außen fehlt das Loch: 1,25 × 1,51 m weniger Fläche nach außen
    let n = right_of(c.layout().unwrap().2[0]) * (c.outer_offset() - c.inner_offset()).signum();
    let weg = flaeche(&vorher, n) - flaeche(&c.solid(), n);
    assert!(weg > 1250.0 * 1510.0 - 1.0, "{weg}");
    // Nur die Wand im EG
    assert!(m
        .chain(m.runs_above(eg)[0])
        .unwrap()
        .joints
        .holes
        .is_empty());
    m.apply(&t, Direction::Undo);
    assert!(m.chain(eg).unwrap().joints.holes.is_empty());
    assert!(m.element(o).is_none());
}

/// Anschlag nach Wandtyp: Verblender 6 cm hinter der Vorsatzschale,
/// monolithisch keiner; ein eigener Anschlag gilt nicht monolithisch.
#[test]
fn anschlag_nach_wandtyp() {
    let (mut m, eg, _) = haus(Some(CAVITY_TYPE_GUID));
    m.add_opening(fenster(&m, eg, (3000.0, 4000.0))).unwrap();
    let h = m.chain(eg).unwrap().joints.holes[0].clone();
    assert_eq!(
        (h.shell, h.reveal_u, h.reveal_z),
        (1, (60.0, 60.0), (0.0, 60.0))
    );
    let (mut m, eg, _) = haus(Some(MONO_TYPE_GUID));
    let mut f = fenster(&m, eg, (3000.0, 4000.0));
    f.reveal = Some([50.0, 50.0, 50.0, 0.0]);
    m.add_opening(f).unwrap();
    let c = m.chain(eg).unwrap();
    let h = &c.joints.holes[0];
    assert_eq!((h.reveal_u, h.reveal_z), ((0.0, 0.0), (0.0, 0.0)));
    assert_eq!(h.shell, c.layers.len());
}

/// Einbautiefen nach Wandtyp (Jörn 17:08–17:09): monolithisch 12 cm,
/// sonst 2 cm hinter der Innenseite der Außenschale; das Element 8 cm,
/// innen der Rest. Eigene Werte gelten, in der Wanddicke geklemmt; das Loch
/// und die Lochmaße tragen sie.
#[test]
fn einbautiefe_nach_wandtyp() {
    let dicke = |m: &Model, run: RunId| -> Vec<f64> {
        let e = m.element(m.run(run).unwrap().segments[0]).unwrap();
        let s = m.layer_set(e.layer_set.unwrap()).unwrap();
        s.layers.iter().map(|l| l.thickness).collect()
    };
    let (mut m, eg, _) = haus(Some(MONO_TYPE_GUID));
    let t: f64 = dicke(&m, eg).iter().sum();
    let f = fenster(&m, eg, (3000.0, 4000.0));
    assert_eq!(
        m.install_depth(&f, eg),
        InstallDepth {
            outer: 120.0,
            element: 80.0,
            inner: t - 200.0
        }
    );
    m.add_opening(f.clone()).unwrap();
    assert_eq!(m.chain(eg).unwrap().joints.holes[0].install.outer, 120.0);
    let mut eigen = f.clone();
    eigen.install = Install {
        outer: Some(5000.0),
        element: Some(70.0),
    };
    let d = m.install_depth(&eigen, eg);
    assert_eq!((d.outer, d.element, d.inner), (t - 70.0, 70.0, 0.0));
    // Innenputz zählt nicht: innen bis zur Rohbau-Innenseite
    let id = m
        .element(m.run(eg).unwrap().segments[0])
        .unwrap()
        .layer_set
        .unwrap();
    let mut s = m.layer_set(id).unwrap().clone();
    let mut putz = s.layers[0];
    putz.thickness = 15.0;
    putz.core = false;
    putz.function = crate::LayerFunction::Finish;
    s.layers.push(putz);
    m.begin("Putz");
    assert!(m.set_layer_set(id, s));
    m.commit();
    assert_eq!(m.install_depth(&f, eg).inner, t - 200.0);

    // WDVS: Dämmung (und Putz) vor dem Kern ist die Außenschale
    let (m, eg, _) = haus(None);
    let l = dicke(&m, eg);
    let (shell, _) = m.opening_shell(eg);
    let vor: f64 = l[..shell].iter().sum();
    let d = m.install_depth(&fenster(&m, eg, (3000.0, 4000.0)), eg);
    assert_eq!(d.outer, vor + 20.0);
    assert_eq!(d.inner, l.iter().sum::<f64>() - vor - 20.0 - 80.0);

    // Verblender: bis hinter die Vorsatzschale
    let (m, eg, _) = haus(Some(CAVITY_TYPE_GUID));
    let l = dicke(&m, eg);
    let d = m.install_depth(&fenster(&m, eg, (3000.0, 4000.0)), eg);
    assert_eq!(d.outer, l[0] + 20.0, "{l:?}");
}

/// Von außen gesehen links und rechts: Auf Segment 1 (Nordseite, von
/// außen nach Süden geblickt) läuft u nach links.
#[test]
fn anschlag_links_rechts() {
    let (mut m, eg, _) = haus(None);
    let st = storey(&m, eg);
    let o = Opening {
        path: vec![aussen(&m, eg, 1, 2000.0), aussen(&m, eg, 1, 3000.0)],
        reveal: Some([10.0, 20.0, 30.0, 0.0]),
        install: Default::default(),
        ..fenster(&m, eg, (0.0, 1.0))
    };
    let _ = st;
    m.add_opening(o).unwrap();
    let h = m.chain(eg).unwrap().joints.holes[0].clone();
    assert_eq!(h.seg, 1);
    let nord = m.chain(eg).unwrap().layout().unwrap().2[1];
    // Segment 1 läuft nach Osten; von Norden gesehen ist Osten links
    assert!(nord.x > 0.9);
    assert_eq!(h.reveal_u, (20.0, 10.0));
}

/// Ecköffnung (Jörn 14:22): erster Punkt auf Segment 0, um die Außenecke,
/// zweiter auf Segment 1; beide Löcher sind zur Ecke hin offen.
#[test]
fn ecköffnung_um_die_aussenecke() {
    let (mut m, eg, _) = haus(None);
    let o = Opening {
        path: vec![
            aussen(&m, eg, 0, 7000.0),
            ecke(&m, eg),
            aussen(&m, eg, 1, 1500.0),
        ],
        ..fenster(&m, eg, (0.0, 1.0))
    };
    m.add_opening(o).unwrap();
    let c = m.chain(eg).unwrap();
    let mut hs = c.joints.holes.clone();
    hs.sort_by_key(|h| h.seg);
    assert_eq!(hs.len(), 2, "{hs:?}");
    assert!((hs[0].u.0 - 7000.0).abs() < 1e-6 && hs[0].u.1 == f64::INFINITY);
    assert!(hs[1].u.0 == f64::NEG_INFINITY && (hs[1].u.1 - 1500.0).abs() < 1e-6);
    // Kein Wandstück bleibt im Band an der Ecke stehen
    let s = c.solid();
    let (z0, z1) = hs[0].z;
    let corner = ecke(&m, eg);
    for t in &s.triangles {
        let mid = (t.p[0] + t.p[1] + t.p[2]) * (1.0 / 3.0);
        let nah = (mid.x - corner.x).abs() < 300.0 && (mid.y - corner.y).abs() < 300.0;
        assert!(
            !(nah && mid.z > z0 + 1.0 && mid.z < z1 - 1.0 && t.n.z.abs() < 1e-6),
            "{mid:?} {:?}",
            t.n
        );
    }
}

/// Geschossübergreifend (Jörn 14:04): von der EG-Brüstung bis zum OG-Sturz
/// trifft die Öffnung EG- und OG-Wand; zieht man die Höhenlinie, wandert
/// sie mit.
#[test]
fn geschossuebergreifend_und_hoehenlinie() {
    let (mut m, eg, og) = haus(None);
    let (s_eg, s_og) = (storey(&m, eg), storey(&m, og));
    let o = Opening {
        top: HeightRef::Line(s_og, HeightLine::Lintel),
        ..fenster(&m, eg, (3000.0, 4000.0))
    };
    let id = m.add_opening(o).unwrap();
    assert_eq!(m.element(id).unwrap().storey, s_eg);
    let z_og = m.storey(s_og).unwrap().elevation;
    for r in [eg, og] {
        let c = m.chain(r).unwrap();
        assert_eq!(c.joints.holes.len(), 1, "{r:?}");
        assert!((c.joints.holes[0].z.1 - z_og - 2385.0).abs() < 1e-6);
    }
    m.begin("Höhenlinie");
    assert!(m.set_height_lines(
        s_og,
        HeightLines {
            sill: 900.0,
            lintel: 2200.0
        }
    ));
    let t = m.commit().unwrap();
    assert!((m.chain(og).unwrap().joints.holes[0].z.1 - z_og - 2200.0).abs() < 1e-6);
    m.apply(&t, Direction::Undo);
    assert_eq!(m.storey(s_og).unwrap().lines, HeightLines::default());
    // Ungültig: Sturz unter der Brüstung
    assert!(!m.set_height_lines(
        s_og,
        HeightLines {
            sill: 1000.0,
            lintel: 1050.0
        }
    ));
}

/// Unsinniges wird abgelehnt: zu schmal, gerader Dreipunktpfad, Sturz unter
/// der Brüstung, Anschlag zu groß.
#[test]
fn ungueltige_oeffnungen() {
    let (mut m, eg, _) = haus(None);
    let st = storey(&m, eg);
    assert!(m.add_opening(fenster(&m, eg, (3000.0, 3050.0))).is_none());
    let gerade = Opening {
        path: vec![
            aussen(&m, eg, 0, 1000.0),
            aussen(&m, eg, 0, 2000.0),
            aussen(&m, eg, 0, 3000.0),
        ],
        ..fenster(&m, eg, (0.0, 1.0))
    };
    assert!(m.add_opening(gerade).is_none());
    let verkehrt = Opening {
        bottom: HeightRef::Line(st, HeightLine::Lintel),
        top: HeightRef::Line(st, HeightLine::Sill),
        ..fenster(&m, eg, (3000.0, 4000.0))
    };
    assert!(m.add_opening(verkehrt).is_none());
    let anschlag = Opening {
        reveal: Some([300.0, 0.0, 0.0, 0.0]),
        install: Default::default(),
        ..fenster(&m, eg, (3000.0, 4000.0))
    };
    assert!(m.add_opening(anschlag).is_none());
}

/// Datei: Öffnungen mit gebundenen und freien Höhen und eigenem Anschlag,
/// abweichende Höhenlinien am Geschoss; ohne beides bleibt die Datei, wie
/// sie war.
#[test]
fn oeffnungen_in_der_datei() {
    let (mut m, eg, og) = haus(None);
    let ohne = szo::write(&m);
    assert!(!ohne.contains("[opening]") && !ohne.contains("sill="));
    let s_og = storey(&m, og);
    m.add_opening(fenster(&m, eg, (3000.0, 4000.0))).unwrap();
    m.add_opening(Opening {
        kind: OpeningKind::Door,
        bottom: HeightRef::Level(LevelRef::bottom(storey(&m, eg))),
        top: HeightRef::Level(LevelRef {
            offset: 2135.5,
            ..LevelRef::bottom(storey(&m, eg))
        }),
        reveal: Some([40.0, 40.0, 40.0, 0.0]),
        install: Install {
            outer: Some(100.0),
            element: Some(70.0),
        },
        ..fenster(&m, eg, (5000.0, 6010.0))
    })
    .unwrap();
    m.add_opening(Opening {
        path: vec![
            aussen(&m, og, 0, 7000.0),
            ecke(&m, og),
            aussen(&m, og, 1, 1500.0),
        ],
        ..fenster(&m, og, (0.0, 1.0))
    })
    .unwrap();
    assert!(m.set_height_lines(
        s_og,
        HeightLines {
            sill: 950.0,
            lintel: 2385.0
        }
    ));
    let text = szo::write(&m);
    assert_eq!(text.matches("[opening]").count(), 3, "{text}");
    assert!(text.contains(" sill=950"), "{text}");
    assert!(!text.contains("lintel=2385"), "{text}");
    assert!(text.contains("art=door") && text.contains("reveal=40,40,40,0"));
    assert!(text.contains("install=100 element=70"), "{text}");
    assert_eq!(text.matches("install=").count(), 1, "nur wo gesetzt");
    let back = szo::read(&text, GuidGen::with_seed(5)).unwrap();
    assert!(back.hints.is_empty(), "{:?}", back.hints);
    assert_eq!(szo::write(&back.model), text);
    assert!(back.model.check().is_empty(), "{:?}", back.model.check());
    let n: usize = back.model.chains().map(|(_, c)| c.joints.holes.len()).sum();
    assert_eq!(n, 4);
    // Unbekannte Höhenlinie: Fehler mit Zeile, wie jeder Verweis
    let kaputt = text.replacen(":sill", ":brüstung", 1);
    assert!(szo::read(&kaputt, GuidGen::with_seed(5)).is_err());
    // Ungültige Werte (Review #79): Hinweis und Vorgabe, die Datei öffnet
    let lies = |alt: &str, neu: &str| {
        assert!(text.contains(alt), "{alt}");
        let l = szo::read(&text.replacen(alt, neu, 1), GuidGen::with_seed(5)).unwrap();
        assert_eq!(l.hints.len(), 1, "{neu}: {:?}", l.hints);
        l.model
    };
    let offen = |m: &Model| -> Vec<Opening> {
        m.elements()
            .iter()
            .filter_map(|(id, _)| m.opening(id).cloned())
            .collect()
    };
    for neu in [" sill=abc", " sill=2300"] {
        let l = lies(" sill=950", neu);
        assert_eq!(
            l.storey(s_og).map(|s| s.lines),
            Some(HeightLines::default())
        );
    }
    let l = lies("reveal=40,40,40,0", "reveal=1,2,3");
    assert!(offen(&l).iter().all(|o| o.reveal.is_none()));
    let l = lies("art=door", "art=fenster");
    assert!(offen(&l).iter().all(|o| o.kind == OpeningKind::Window));
    let l = lies("art=door ", "");
    assert!(offen(&l).iter().all(|o| o.kind == OpeningKind::Window));
    let l = lies(":u:2135.5", ":u:abc");
    assert_eq!(
        offen(&l).len(),
        2,
        "unlesbarer Versatz: die Öffnung fällt weg"
    );
    let l = lies("install=100", "install=-5");
    assert!(offen(&l).iter().all(|o| o.install.outer.is_none()));
    let op = text.find("[opening]").unwrap();
    let pts = op + text[op..].find("pts=\"").unwrap() + 5;
    let ende = pts + text[pts..].find('"').unwrap();
    let l = lies(&text[pts - 5..=ende], "pts=\"abc\"");
    assert_eq!(offen(&l).len(), 2, "die unlesbare Öffnung fällt weg");
    assert!(l.check().is_empty(), "{:?}", l.check());
}

/// Σ n·A über alle Dreiecke: 0 bei geschlossenem Körper.
fn offen(s: &Solid) -> f64 {
    let mut v = vec3(0.0, 0.0, 0.0);
    for t in &s.triangles {
        v += t.n * ((t.p[1] - t.p[0]).cross(t.p[2] - t.p[0]).length() * 0.5);
    }
    v.length()
}

/// Review #79, Befund 1: Verschiebt man die Wand unter einer Öffnung, meldet
/// die Prüfung die Öffnung ohne Wand (das Mitwandern kommt mit O5); löscht
/// man die Wand, geht die Öffnung mit. Rückgängig ist alles wie vorher.
#[test]
fn wand_verschoben_oder_geloescht() {
    let (mut m, eg, _) = haus(None);
    let vorn = m.add_opening(fenster(&m, eg, (3000.0, 4250.0))).unwrap();
    assert!(m.check().is_empty(), "{:?}", m.check());
    let moved = m.chain(eg).unwrap().with_segment_moved(0, 500.0).unwrap();
    m.begin("Verschieben");
    assert!(m.set_run_points(eg, &moved.points).is_some());
    let t = m.commit().unwrap();
    assert!(m.chain(eg).unwrap().joints.holes.is_empty());
    let befund = m.check();
    assert!(
        befund
            .iter()
            .any(|b| b.contains("nicht auf einer Außenwand")),
        "{befund:?}"
    );
    m.apply(&t, Direction::Undo);
    assert!(m.check().is_empty(), "{:?}", m.check());
    // Löschen geht nur bei einer Wand außerhalb des Umrisses: freie
    // Außenwand neben dem Haus
    let r = m.run(eg).unwrap().clone();
    let set = m.element(r.segments[0]).unwrap().layer_set.unwrap();
    let pts = [vec3(14000.0, 0.0, 0.0), vec3(14000.0, 6000.0, 0.0)];
    m.begin("Wand");
    let frei = m
        .add_wall_run(
            &pts,
            false,
            r.ref_side,
            r.storey,
            set,
            Category::ExteriorWall,
        )
        .unwrap();
    m.commit();
    let id = m.add_opening(fenster(&m, frei, (2000.0, 3000.0))).unwrap();
    assert!(m.check().is_empty(), "{:?}", m.check());
    m.begin("Löschen");
    let d = m.delete_elements(&[m.wall_at(frei, 0).unwrap()]);
    let t = m.commit().unwrap();
    assert!(d.removed.contains(&id), "{d:?}");
    assert!(m.opening(id).is_none());
    assert!(m.check().is_empty(), "{:?}", m.check());
    m.apply(&t, Direction::Undo);
    assert!(m.opening(id).is_some());
    assert_eq!(m.chain(frei).unwrap().joints.holes.len(), 1);
    // Andere Öffnungen bleiben stehen
    m.begin("Löschen");
    let d = m.delete_elements(&[m.wall_at(frei, 0).unwrap()]);
    m.commit();
    assert!(!d.removed.contains(&vorn) && m.opening(vorn).is_some());
}

/// Review #79, N1 und N2: Eine Öffnung über zwei Geschosse braucht in jedem
/// Geschoss ihre Wand; verliert sie die im OG, meldet es die Prüfung, wird
/// die EG-Wand gelöscht, geht sie mit. Auch eine schon fehlerhafte Öffnung
/// (Überschneidung aus der Datei) geht mit ihrer Wand.
#[test]
fn geschossband_und_loeschen() {
    let (mut m, eg, og) = haus(None);
    let (s_og, s_eg) = (storey(&m, og), storey(&m, eg));
    let hoch = Opening {
        top: HeightRef::Line(s_og, HeightLine::Lintel),
        ..fenster(&m, eg, (3000.0, 4000.0))
    };
    m.add_opening(hoch).unwrap();
    assert!(m.check().is_empty(), "{:?}", m.check());
    let moved = m.chain(og).unwrap().with_segment_moved(0, 500.0).unwrap();
    m.begin("OG verschieben");
    assert!(m.set_run_points(og, &moved.points).is_some());
    let t = m.commit().unwrap();
    let befund = m.check();
    assert!(
        befund
            .iter()
            .any(|b| b.contains("nicht auf einer Außenwand")),
        "{befund:?}"
    );
    m.apply(&t, Direction::Undo);
    // Freie Wände in EG und OG übereinander, die Öffnung über beide
    let r = m.run(eg).unwrap().clone();
    let set = m.element(r.segments[0]).unwrap().layer_set.unwrap();
    let pts = [vec3(14000.0, 0.0, 0.0), vec3(14000.0, 6000.0, 0.0)];
    m.begin("Wände");
    let unten = m
        .add_wall_run(&pts, false, r.ref_side, s_eg, set, Category::ExteriorWall)
        .unwrap();
    m.add_wall_run(&pts, false, r.ref_side, s_og, set, Category::ExteriorWall)
        .unwrap();
    m.commit();
    let id = m
        .add_opening(Opening {
            top: HeightRef::Line(s_og, HeightLine::Lintel),
            ..fenster(&m, unten, (2000.0, 3000.0))
        })
        .unwrap();
    m.begin("Löschen");
    let d = m.delete_elements(&[m.wall_at(unten, 0).unwrap()]);
    m.commit();
    assert!(d.removed.contains(&id), "{d:?}");
    // N2: zwei sich überschneidende Öffnungen aus der Datei
    let (mut m, eg, _) = haus(None);
    let r = m.run(eg).unwrap().clone();
    let set = m.element(r.segments[0]).unwrap().layer_set.unwrap();
    m.begin("Wand");
    let frei = m
        .add_wall_run(
            &pts,
            false,
            r.ref_side,
            r.storey,
            set,
            Category::ExteriorWall,
        )
        .unwrap();
    m.commit();
    m.add_opening(fenster(&m, frei, (1000.0, 2000.0))).unwrap();
    m.add_opening(fenster(&m, frei, (3000.0, 4000.0))).unwrap();
    let text = szo::write(&m);
    let pfad = |o: &Opening| {
        o.path
            .iter()
            .map(|p| format!("{} {}", p.x, p.y))
            .collect::<Vec<_>>()
            .join(";")
    };
    let alt = pfad(&fenster(&m, frei, (3000.0, 4000.0)));
    let neu = pfad(&fenster(&m, frei, (1500.0, 2500.0)));
    assert!(text.contains(&alt), "{text}");
    let mut m = szo::read(&text.replacen(&alt, &neu, 1), GuidGen::with_seed(5))
        .unwrap()
        .model;
    assert!(m.check().iter().any(|b| b.contains("überschneidet")));
    let frei = m
        .runs()
        .iter()
        .find(|(_, r)| !r.closed && r.storey == storey(&m, eg))
        .map(|(id, _)| id)
        .unwrap();
    m.begin("Löschen");
    let d = m.delete_elements(&[m.wall_at(frei, 0).unwrap()]);
    m.commit();
    assert_eq!(d.removed.len(), 3, "Wand und beide Öffnungen: {d:?}");
    assert!(m.check().is_empty(), "{:?}", m.check());
}

/// Review #79, Befund 2: Öffnungen dürfen sich nicht überschneiden, im WDVS
/// samt Anschlag; daneben und in sich verschoben geht.
#[test]
fn ueberschneidende_oeffnungen() {
    let (mut m, eg, _) = haus(Some(MONO_TYPE_GUID));
    let a = m.add_opening(fenster(&m, eg, (3000.0, 4250.0))).unwrap();
    let quer = fenster(&m, eg, (3500.0, 5000.0));
    assert_eq!(m.opening_fault(&quer, None), Some(OpeningFault::Overlap(a)));
    assert!(m.add_opening(quer).is_none());
    let b = m.add_opening(fenster(&m, eg, (4500.0, 5500.0))).unwrap();
    m.begin("Ändern");
    assert!(!m.set_opening(b, fenster(&m, eg, (4000.0, 5500.0))));
    assert!(m.set_opening(b, fenster(&m, eg, (4400.0, 5400.0))));
    m.commit();
    assert!(m.check().is_empty(), "{:?}", m.check());
    // Darüber im OG-Band überschneidet nichts
    let (mut m, eg, _) = haus(None);
    let a = m.add_opening(fenster(&m, eg, (3000.0, 4250.0))).unwrap();
    // 4 cm daneben, aber 3 cm Anschlag je Seite hinter dem WDVS
    assert_eq!(
        m.opening_fault(&fenster(&m, eg, (4290.0, 5000.0)), None),
        Some(OpeningFault::Overlap(a))
    );
    assert!(m.add_opening(fenster(&m, eg, (4320.0, 5000.0))).is_some());
}

/// Review #79, Befund 3: Ein freies Ende muss samt Anschlag in der
/// Wandfläche liegen, also nicht an der Außenecke oder über das
/// Segmentende hinaus; um die Ecke geht es mit drei Punkten.
#[test]
fn ende_an_der_ecke() {
    for typ in [None, Some(MONO_TYPE_GUID), Some(CAVITY_TYPE_GUID)] {
        let (mut m, eg, _) = haus(typ);
        let bis_ecke = Opening {
            path: vec![aussen(&m, eg, 0, 7000.0), ecke(&m, eg)],
            ..fenster(&m, eg, (0.0, 1.0))
        };
        assert_eq!(m.opening_fault(&bis_ecke, None), Some(OpeningFault::AtEnd));
        assert!(m.add_opening(bis_ecke).is_none());
        let drueber = fenster(&m, eg, (7000.0, 9000.0));
        assert_eq!(m.opening_fault(&drueber, None), Some(OpeningFault::AtEnd));
        let daneben = Opening {
            path: vec![aussen(&m, eg, 0, 3000.0), vec3(5000.0, 3000.0, 0.0)],
            ..fenster(&m, eg, (0.0, 1.0))
        };
        assert_eq!(m.opening_fault(&daneben, None), Some(OpeningFault::NoWall));
        let um_die_ecke = Opening {
            path: vec![
                aussen(&m, eg, 0, 7000.0),
                ecke(&m, eg),
                aussen(&m, eg, 1, 1500.0),
            ],
            ..fenster(&m, eg, (0.0, 1.0))
        };
        m.add_opening(um_die_ecke).unwrap();
        let s = m.chain(eg).unwrap().solid();
        assert!(offen(&s) < 1.0, "{typ:?}: {}", offen(&s));
    }
}

/// Review #79, Befund 4: Eine Tür ab Unterkante Wand und eine Öffnung bis
/// Oberkante Wand lassen keine Boden- oder Deckfläche im Loch stehen; der
/// Grundriss genau auf Oberkante Öffnung hat dort keine Schnittfläche. Der
/// Körper bleibt geschlossen.
#[test]
fn raender_der_oeffnung() {
    for typ in [None, Some(MONO_TYPE_GUID), Some(CAVITY_TYPE_GUID)] {
        let (mut m, eg, _) = haus(typ);
        let st = storey(&m, eg);
        let c = m.chain(eg).unwrap();
        let unten = m.level_z(LevelRef::bottom(st)).unwrap();
        let (zb, zt) = (c.base, c.base + c.height);
        let tuer = Opening {
            kind: OpeningKind::Door,
            bottom: HeightRef::Level(LevelRef {
                offset: zb - unten,
                ..LevelRef::bottom(st)
            }),
            ..fenster(&m, eg, (3000.0, 4010.0))
        };
        m.add_opening(tuer).unwrap();
        let hoch = Opening {
            top: HeightRef::Level(LevelRef {
                offset: zt - unten,
                ..LevelRef::bottom(st)
            }),
            ..fenster(&m, eg, (5000.0, 6000.0))
        };
        m.add_opening(hoch).unwrap();
        let c = m.chain(eg).unwrap();
        let im_loch =
            |p: Vec3, u: (f64, f64)| p.x.abs() < 1000.0 && p.y > u.0 + 1.0 && p.y < u.1 - 1.0;
        let s = c.solid();
        assert!(offen(&s) < 1.0, "{typ:?}: {}", offen(&s));
        for t in &s.triangles {
            let mid = (t.p[0] + t.p[1] + t.p[2]) * (1.0 / 3.0);
            let boden = t.n.z < -0.5 && (mid.z - zb).abs() < 1.0 && im_loch(mid, (3000.0, 4010.0));
            let decke = t.n.z > 0.5 && (mid.z - zt).abs() < 1.0 && im_loch(mid, (5000.0, 6000.0));
            assert!(!boden && !decke, "{typ:?}: {mid:?} {:?}", t.n);
        }
        for e in &s.edges {
            let mid = (e.a + e.b) * 0.5;
            let boden = (mid.z - zb).abs() < 1.0
                && (e.a.z - e.b.z).abs() < 1e-6
                && im_loch(mid, (3000.0, 4010.0));
            assert!(!boden, "{typ:?}: Kante {mid:?}");
        }
        let z1 = m.chain(eg).unwrap().joints.holes[0].z.1;
        let s = c.solid_cut_at(z1);
        for t in &s.triangles {
            let mid = (t.p[0] + t.p[1] + t.p[2]) * (1.0 / 3.0);
            assert!(
                !(t.n.z > 0.5 && (mid.z - z1).abs() < 1.0 && im_loch(mid, (3000.0, 4010.0))),
                "{typ:?}: Schnittfläche im Loch {mid:?}"
            );
        }
    }
}

/// Mengen (VOB-Vorgabe §5): je getroffener Schicht das eigene Lochmaß mit
/// Anschlag, Laibungslängen und -tiefe; das Volumen sinkt um jedes Loch,
/// die Abrechnungsfläche bleibt ohne Abzug, bis die VOB-Regel entscheidet.
#[test]
fn lochmasse_je_schicht() {
    use crate::qto::run_qto;
    let (mut m, eg, og) = haus(None);
    let vorher = run_qto(&m, eg);
    let faces = |m: &Model| -> Vec<f64> {
        crate::qto::schedule(m)
            .layer_rows(m)
            .into_iter()
            .map(|(_, r)| r.face)
            .collect()
    };
    let face_vorher = faces(&m);
    let id = m.add_opening(fenster(&m, eg, (3000.0, 4250.0))).unwrap();
    let q = run_qto(&m, eg);
    // Abrechnungsfläche ungemindert, auch wo sie aus dem Volumen kommt
    // (Face::Mean): über den Abzug entscheidet die VOB-Regel
    for (a, b) in face_vorher.iter().zip(faces(&m)) {
        assert!((a - b).abs() < 1e-3, "face {a} → {b}");
    }
    let (d, k) = (&q[0].layers[0], &q[0].layers[1]);
    assert_eq!((d.holes.len(), k.holes.len()), (1, 1));
    let (hd, hk) = (&d.holes[0], &k.holes[0]);
    assert_eq!(
        (hd.opening, hd.kind, hd.sill),
        (id, OpeningKind::Window, true)
    );
    assert!((hd.width - 1250.0).abs() < 1e-6 && (hd.height - 1510.0).abs() < 1e-6);
    assert!((hk.width - 1310.0).abs() < 1e-6 && (hk.height - 1540.0).abs() < 1e-6);
    assert_eq!(hd.reveal, [1510.0, 1510.0, 1250.0, 1250.0]);
    assert!((hd.depth - d.thickness).abs() < 1e-9);
    for (a, b) in vorher[0].layers.iter().zip(&q[0].layers) {
        let v: f64 = b.holes.iter().map(|h| h.volume).sum();
        assert!(
            (a.volume - v - b.volume).abs() < 1.0,
            "{} {} {v}",
            a.volume,
            b.volume
        );
    }
    assert_eq!(q[1], vorher[1], "andere Segmente unberührt");
    // Ecköffnung: zur Ecke hin keine Laibung
    let (mut m, eg2, _) = haus(None);
    let _ = (eg, og);
    m.add_opening(Opening {
        path: vec![
            aussen(&m, eg2, 0, 7000.0),
            ecke(&m, eg2),
            aussen(&m, eg2, 1, 1500.0),
        ],
        ..fenster(&m, eg2, (0.0, 1.0))
    })
    .unwrap();
    let q = run_qto(&m, eg2);
    let (a, b) = (&q[0].layers[0].holes[0], &q[1].layers[0].holes[0]);
    // Segment 0 von außen (Westen) gesehen läuft u nach links: die Ecke
    // liegt links, Segment 1 (Norden) läuft ebenfalls nach links
    assert_eq!(a.reveal[..2], [0.0, 1510.0]);
    assert_eq!(b.reveal[..2], [1510.0, 0.0]);
    assert_eq!(a.opening, b.opening);
}

/// Geschossübergreifend: die Höhe teilt sich auf EG- und OG-Wand, oben
/// zeigt nur das OG eine Laibung, unten nur das EG.
#[test]
fn lochmasse_ueber_geschosse() {
    use crate::qto::run_qto;
    let (mut m, eg, og) = haus(None);
    let s_og = storey(&m, og);
    m.add_opening(Opening {
        top: HeightRef::Line(s_og, HeightLine::Lintel),
        ..fenster(&m, eg, (3000.0, 4000.0))
    })
    .unwrap();
    let (a, b) = (&run_qto(&m, eg)[0].layers[0], &run_qto(&m, og)[0].layers[0]);
    let (ha, hb) = (&a.holes[0], &b.holes[0]);
    assert!(ha.reveal[2] == 0.0 && ha.reveal[3] > 0.0, "{:?}", ha.reveal);
    assert!(hb.reveal[2] > 0.0 && hb.reveal[3] == 0.0, "{:?}", hb.reveal);
    assert!(ha.sill && hb.sill, "Brüstung im Geschoss der Öffnung");
    let z_eg = m.storey(storey(&m, eg)).unwrap().elevation;
    let z_og = m.storey(s_og).unwrap().elevation;
    let soll = z_og + 2385.0 - (z_eg + 875.0);
    assert!(ha.height + hb.height <= soll + 1e-6);
    assert!(
        ha.height + hb.height > soll - 400.0,
        "nur das Deckenband fehlt"
    );
}

/// Kosten H79-2: Der Rechenweg des Volumens nennt die Löcher als Term
/// „Öffnung“; ihr Volumen landet nicht im Ausgleich.
#[test]
fn rechenweg_mit_oeffnung() {
    use crate::rechenweg::{weg, Bezug, Modus};
    let (mut m, eg, _) = haus(None);
    let ausgleich = |m: &Model| -> Vec<bool> {
        crate::qto::schedule(m)
            .layer_rows(m)
            .iter()
            .filter_map(|(_, r)| weg(m, r, Bezug::Volumen, Modus::Vob))
            .map(|w| w.terme.iter().any(|t| t.text == "Ecken und Anschlüsse"))
            .collect()
    };
    let vorher = ausgleich(&m);
    m.add_opening(fenster(&m, eg, (3000.0, 4250.0))).unwrap();
    assert_eq!(ausgleich(&m), vorher, "Lochvolumen im Ausgleich");
    let rows = crate::qto::schedule(&m).layer_rows(&m);
    let mut n = 0;
    for (_, r) in &rows {
        let Some(p) = r.parts.as_ref().filter(|p| p.loecher == 1) else {
            continue;
        };
        let w = weg(&m, r, Bezug::Volumen, Modus::Vob).unwrap();
        let t = w.terme.iter().find(|t| t.text == "Öffnung").unwrap();
        assert!((t.wert + p.loch_volumen).abs() < 1e-6 && p.loch_volumen > 0.0);
        assert!((w.menge() - r.volume).abs() < 1e-3);
        n += 1;
    }
    assert!(n >= 2, "{n} Schichten mit Loch");
}
