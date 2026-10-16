//! Pflicht-Test zum Merken der Mengen je Gebäude (Review M2, Architektur
//! 16.10.): Nach jedem Schritt einer Folge sind Mengenliste, Kostenblatt
//! und LV der Szene dieselben wie die einer frischen Szene, die alles neu
//! rechnet; neu gerechnet wird nur, was der Schritt berührt.

use crate::scene::Scene;
use sk_math::vec3;
use sk_model::oeffnung::{HeightLine, HeightRef, Opening, OpeningKind};
use sk_model::{BuildingId, GuidGen, Model, RunId, StoreyId};
use std::cell::RefCell;
use std::collections::HashMap;

const RH1: &str = include_str!("../../crates/sk-cost/referenz/rh1-standardhaus.szo");

/// Haus mit zwei Geschossen ab `x`, Räumen und Ausbautypen.
fn haus(m: &mut Model, x: f64) -> BuildingId {
    let b = m.add_building(2);
    let pts = [
        vec3(x, 0.0, 0.0),
        vec3(x, 8000.0, 0.0),
        vec3(x + 10000.0, 8000.0, 0.0),
        vec3(x + 10000.0, 0.0, 0.0),
    ];
    m.build_from_polygon(b, &pts).unwrap();
    let geschosse: Vec<sk_model::StoreyId> = m
        .levels_in(Some(b))
        .into_iter()
        .filter(|g| m.storey(*g).unwrap().kind == sk_model::LevelKind::Storey)
        .collect();
    m.begin("Räume");
    assert!(sk_raum::abgleichen(m, &geschosse) > 0);
    m.commit();
    for g in geschosse {
        let id = m.spaces(g)[0];
        m.begin("Ausbau");
        assert!(sk_raum::ausbau::typen_anlegen(m, id));
        m.commit();
    }
    b
}

/// RH-1 (mit Dach, Gründung und Geländeaufmaß) und zwei Häuser mit Räumen.
fn szene() -> (Scene, BuildingId, BuildingId) {
    let mut m =
        sk_model::szo::read_with(RH1, GuidGen::with_seed(1), &sk_cost::lesen::ABSCHNITTE_SZO)
            .expect("lädt")
            .model;
    let b = haus(&mut m, 30e3);
    let c = haus(&mut m, 50e3);
    (Scene::with_model(m), b, c)
}

/// Erster Wandzug eines Gebäudes.
fn zug(s: &Scene, b: BuildingId) -> RunId {
    let m = s.model();
    m.runs()
        .iter()
        .find(|(_, r)| m.building_of(r.storey) == Some(b))
        .map(|(id, _)| id)
        .expect("Zug im Gebäude")
}

/// Liste, Kostenblatt und LV jedes Loses wie in einer frischen Szene;
/// `neu`: so viele Teile hat die Liste neu gerechnet (`None`: alle).
fn gleich(s: &mut Scene, neu: Option<usize>, wann: &str) {
    let mut frisch = Scene::with_model(s.model().clone());
    assert_eq!(*s.schedule(), *frisch.schedule(), "Mengen: {wann}");
    let alle = s.model().buildings().len() + 1;
    assert_eq!(
        s.ausbau.mengen.zuletzt_neu(),
        neu.unwrap_or(alle),
        "neu gerechnete Teile: {wann}"
    );
    let u = sk_cost::Umfang::projekt();
    let blatt = s.kostenblatt(None, &u);
    assert_eq!(*blatt, *frisch.kostenblatt(None, &u), "Kosten: {wann}");
    assert!(!blatt.positionen.is_empty());
    let kat = s.katalog(None);
    for los in kat.lose.iter().filter(|l| !l.retired && l.parent.is_none()) {
        let w = sk_cost::lv::LvWahl {
            los: los.guid,
            untertitel: true,
            preise: true,
            heute: None,
        };
        assert_eq!(*s.lv(None, &u, &w), *frisch.lv(None, &u, &w), "LV: {wann}");
    }
}

#[test]
fn teile_je_gebaeude_wie_alles_neu() {
    let (mut s, b, c) = szene();
    gleich(&mut s, None, "Anfang");
    // Wand ziehen in B, mit Liste im offenen Schritt
    let run = zug(&s, b);
    let orig = s.chain(run).unwrap().clone();
    s.begin("Wand verschieben");
    for d in [-100.0, -200.0, -300.0] {
        let moved = orig.with_segment_moved(0, d).unwrap();
        s.set_run_points(run, &moved.points);
        s.schedule();
    }
    s.commit();
    gleich(&mut s, Some(1), "Wand gezogen");
    // Fenster in die Südwand von B
    let (face, p) = s
        .outer_face_at(vec3(35e3, -9000.0, 1500.0), vec3(0.0, 1.0, 0.0))
        .expect("Außenfläche");
    let (a, e) = (
        face.at(face.u_of(p) - 600.0, 0.0),
        face.at(face.u_of(p) + 600.0, 0.0),
    );
    let st = s.model().run(run).unwrap().storey;
    let fenster = Opening {
        kind: OpeningKind::Window,
        path: vec![vec3(a.x, a.y, 0.0), vec3(e.x, e.y, 0.0)],
        bottom: HeightRef::Line(st, HeightLine::Sill),
        top: HeightRef::Line(st, HeightLine::Lintel),
        reveal: None,
        install: Default::default(),
        swing: None,
        lock: Default::default(),
    };
    s.add_opening(fenster).expect("Fenster");
    gleich(&mut s, Some(1), "Fenster");
    // Raum in C ändern
    let g = s.model().ground_of(Some(c)).unwrap();
    let raum = s.model().spaces(g)[0];
    assert!(s.edit_model("Raum", |m| m.set_space(raum, |r| r.faktor = Some(0.75))));
    gleich(&mut s, Some(1), "Raum");
    assert!(s.undo());
    gleich(&mut s, Some(1), "Raum rückgängig");
    assert!(s.undo());
    gleich(&mut s, Some(1), "Fenster rückgängig");
    assert!(s.redo());
    gleich(&mut s, Some(1), "Fenster wiederholt");
    // Wand ziehen im Haus mit Dach und Gründung (RH-1)
    let a = s.model().buildings().ids().next().unwrap();
    let run = zug(&s, a);
    let orig = s.chain(run).unwrap().clone();
    s.begin("Wand verschieben");
    s.set_run_points(run, &orig.with_segment_moved(0, -150.0).unwrap().points);
    s.commit();
    gleich(&mut s, Some(1), "Wand RH-1");
    // Geschosshöhe in B: ein Schritt an den Geschossen, alles
    let og = *s
        .model()
        .levels_in(Some(b))
        .iter()
        .rfind(|g| s.model().storey(**g).unwrap().kind == sk_model::LevelKind::Storey)
        .unwrap();
    let h = s.model().storey(og).unwrap().height + 125.0;
    assert!(s.edit_model("Geschosshöhe", |m| m.set_storey_height(og, h)));
    gleich(&mut s, None, "Geschosshöhe");
    assert!(s.undo());
    gleich(&mut s, None, "Geschosshöhe rückgängig");
    assert!(s.redo());
    gleich(&mut s, None, "Geschosshöhe wiederholt");
    // Gebäude löschen und neu: alles
    assert!(s.edit_model("Gebäude löschen", |m| m.remove_building(c)));
    gleich(&mut s, None, "Gebäude gelöscht");
    assert!(s.edit_model("Gebäude erstellt", |m| {
        let d = m.add_building(1);
        let p = |x: f64, y: f64| vec3(70e3 + x, y, 0.0);
        let pts = [
            p(0.0, 0.0),
            p(0.0, 6000.0),
            p(7000.0, 6000.0),
            p(7000.0, 0.0),
        ];
        m.build_from_polygon(d, &pts).is_some()
    }));
    gleich(&mut s, None, "Gebäude neu");
    assert!(s.undo() && s.undo());
    gleich(&mut s, None, "Gebäude zurück");
    // danach wieder je Gebäude
    let run = zug(&s, b);
    let orig = s.chain(run).unwrap().clone();
    s.begin("Wand verschieben");
    s.set_run_points(run, &orig.with_segment_moved(0, 120.0).unwrap().points);
    s.commit();
    gleich(&mut s, Some(1), "Wand nach Rückgängig");
}

/// Ein Abruf holt die Räume jedes Geschosses höchstens einmal, bei
/// Teil-Neurechnung nur die der berührten Gebäude (Grundriss 16.10.,
/// Review H-193e). In Debug-Tests holt die Prüfung gegen die volle Liste
/// jedes Geschoss ein weiteres Mal.
#[test]
fn raeume_je_abruf_einmal() {
    let (mut s, b, _) = szene();
    let mt = super::MengenTeile::default();
    let mut liste = None;
    let mut abruf = |s: &Scene| {
        let n = RefCell::new(HashMap::<StoreyId, usize>::new());
        let raeume = |_: &Model, g: StoreyId| {
            *n.borrow_mut().entry(g).or_default() += 1;
            Vec::new()
        };
        mt.alt_geben(liste.take());
        let l = std::rc::Rc::new(mt.liste(s.model(), &raeume, &|_| None));
        mt.merken(&l);
        liste = Some(l);
        n.into_inner()
    };
    let alle = abruf(&s);
    assert!(alle.len() > 2 && alle.values().all(|n| *n == 1), "{alle:?}");
    let run = zug(&s, b);
    let orig = s.chain(run).unwrap().clone();
    s.begin("Wand verschieben");
    s.set_run_points(run, &orig.with_segment_moved(0, -100.0).unwrap().points);
    s.commit();
    let neu = abruf(&s);
    assert_eq!(mt.zuletzt_neu(), 1);
    let pruefung = usize::from(cfg!(debug_assertions));
    for g in alle.keys() {
        let in_b = s.model().building_of(*g) == Some(b);
        assert_eq!(
            neu.get(g).copied().unwrap_or(0),
            usize::from(in_b) + pruefung,
            "{neu:?}"
        );
    }
}
