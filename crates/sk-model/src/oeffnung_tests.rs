//! Paket O2 (Jörn 11.10.): Öffnungen im Modell, im Wandkörper und in der
//! Datei; Höhenlinien je Geschoss.

use crate::element::RunId;
use crate::hoehen::HeightLines;
use crate::model::{Model, CAVITY_TYPE_GUID, MONO_TYPE_GUID};
use crate::oeffnung::{HeightLine, HeightRef, Opening, OpeningKind, REVEAL_ETICS};
use crate::solid::Solid;
use crate::txn::Direction;
use crate::wall::right_of;
use crate::{szo, Guid, GuidGen, LevelRef, StoreyId};
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

/// Von außen gesehen links und rechts: Auf Segment 1 (Nordseite, von
/// außen nach Süden geblickt) läuft u nach links.
#[test]
fn anschlag_links_rechts() {
    let (mut m, eg, _) = haus(None);
    let st = storey(&m, eg);
    let o = Opening {
        path: vec![aussen(&m, eg, 1, 2000.0), aussen(&m, eg, 1, 3000.0)],
        reveal: Some([10.0, 20.0, 30.0, 0.0]),
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
    let back = szo::read(&text, GuidGen::with_seed(5)).unwrap();
    assert!(back.hints.is_empty(), "{:?}", back.hints);
    assert_eq!(szo::write(&back.model), text);
    assert!(back.model.check().is_empty(), "{:?}", back.model.check());
    let n: usize = back.model.chains().map(|(_, c)| c.joints.holes.len()).sum();
    assert_eq!(n, 4);
    // Unbekannte Höhenlinie: Fehler mit Zeile
    let kaputt = text.replacen(":sill", ":brüstung", 1);
    assert!(szo::read(&kaputt, GuidGen::with_seed(5)).is_err());
}
