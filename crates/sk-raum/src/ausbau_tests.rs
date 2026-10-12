//! Abweichungen je Raum (RB6): Boden ohne Belag, Wandbekleidung für alle
//! Wandseiten und an einer anders, Wandfliesen mit eigener Höhe an einer
//! Seite; Ausbau, Mengen und Rückgängig.

use super::*;
use crate::erkennung::raeume as formen;
use crate::mengen::raeume;
use crate::oeffnungen::Rohbau;
use crate::raumliste::tests::haus;
use sk_model::model::{LIME_WALL_TYPE_GUID, WALL_TILES_TYPE_GUID, WOODCHIP_WALL_TYPE_GUID};
use sk_model::txn::Direction;

/// Rohbau des Raums `raum` im Geschoss `g`.
fn rohbau(m: &Model, g: StoreyId, raum: ElementId) -> Rohbau {
    let f = formen(m, g)
        .into_iter()
        .find(|f| f.raum == Some(raum))
        .unwrap();
    Rohbau::neu(m, f)
}

/// Unterstes Geschoss und sein Raum.
fn erdgeschoss(m: &Model) -> (StoreyId, ElementId) {
    let eg = m
        .levels_in(m.buildings().iter().next().map(|b| b.0))
        .into_iter()
        .find(|g| m.storey(*g).unwrap().kind == LevelKind::Storey)
        .unwrap();
    (eg, m.spaces(eg)[0])
}

fn ab(stelle: AusbauStelle, wahl: Option<AusbauWahl>, hoehe: Option<f64>) -> AusbauAbweichung {
    AusbauAbweichung {
        stelle,
        wahl,
        hoehe,
    }
}

/// Wohnzimmer im EG: Ohne Abweichung Parkett, Wandbekleidung und
/// Sockelleiste an jeder Wandseite. Mit Abweichungen kein Belag, Kalkputz
/// an allen Wandseiten und Raufaser an einer; die Mengen folgen,
/// Rückgängig stellt den Ausbau der Vorlage wieder her.
#[test]
fn abweichungen_im_raum() {
    let mut m = haus();
    let (eg, r) = erdgeschoss(&m);
    let roh = rohbau(&m, eg, r);
    let vorher = RaumAusbau::fuer(&m, &roh).unwrap();
    assert!(vorher.boden.is_some());
    assert!(vorher
        .seiten
        .iter()
        .all(|s| s.wand.is_some() && s.sockel.is_some() && s.fliesen.is_none()));
    let kanten: Vec<&Kante> = roh.kanten().collect();
    let k = kanten
        .iter()
        .position(|k| wand_seite(&m, k).is_some())
        .unwrap();
    let ws = wand_seite(&m, kanten[k]).unwrap();

    m.begin("Abweichungen");
    m.set_space_finish(r, ab(AusbauStelle::Boden, Some(AusbauWahl::Keiner), None))
        .unwrap();
    let kalk = ab(
        AusbauStelle::Wand(None),
        Some(AusbauWahl::Typ(LIME_WALL_TYPE_GUID)),
        None,
    );
    m.set_space_finish(r, kalk).unwrap();
    let raufaser = ab(
        AusbauStelle::Wand(Some(ws)),
        Some(AusbauWahl::Typ(WOODCHIP_WALL_TYPE_GUID)),
        None,
    );
    m.set_space_finish(r, raufaser).unwrap();
    let tx = m.commit().unwrap();

    let roh = rohbau(&m, eg, r);
    let a = RaumAusbau::fuer(&m, &roh).unwrap();
    let (lime, woodchip) = (
        m.type_by_guid(LIME_WALL_TYPE_GUID),
        m.type_by_guid(WOODCHIP_WALL_TYPE_GUID),
    );
    assert_eq!(a.boden, None);
    assert_eq!(a.estrich, vorher.estrich);
    assert_eq!(a.decke, vorher.decke);
    for (i, s) in a.seiten.iter().enumerate() {
        let wand = if i == k { woodchip } else { lime };
        assert_eq!(s.wand, wand, "Kante {i}");
        assert_eq!(s.sockel, vorher.seiten[i].sockel, "Kante {i}");
    }
    let (_, q) = raeume(&m, eg).into_iter().find(|x| x.0 == r).unwrap();
    let boden = crate::raumbuch::Flaeche::Boden.kennung();
    assert!(!q.lines.iter().any(|l| l.flaeche == boden), "kein Belag");
    assert!(q.lines.iter().any(|l| l.name == "Estrich"));
    let typen = |name: &str| -> Vec<LayerSetId> {
        q.lines
            .iter()
            .filter(|l| l.name.starts_with(name))
            .filter_map(|l| l.finish.map(|f| f.0))
            .collect()
    };
    let waende = typen("Wand ");
    assert!(waende.contains(&lime.unwrap()) && waende.contains(&woodchip.unwrap()));

    m.apply(&tx, Direction::Undo);
    assert_eq!(RaumAusbau::fuer(&m, &rohbau(&m, eg, r)).unwrap(), vorher);
}

/// Wandfliesen für alle Wandseiten in der Höhe der Vorlage, an einer Seite
/// nur eine andere Höhe: Dort gilt der Typ aller Seiten mit der eigenen
/// Höhe. Wo Fliesen liegen, keine Sockelleiste; die Mengen zeigen Fliesen
/// statt Sockelleiste. Die Grundfläche des Stempels folgt den Fliesen wie
/// das Mengenblatt (Bedienbarkeit 43.8).
#[test]
fn fliesenhoehe_je_seite() {
    let mut m = haus();
    let (eg, r) = erdgeschoss(&m);
    let roh = rohbau(&m, eg, r);
    let kanten: Vec<&Kante> = roh.kanten().collect();
    let k = kanten
        .iter()
        .position(|k| wand_seite(&m, k).is_some())
        .unwrap();
    let ws = wand_seite(&m, kanten[k]).unwrap();
    let grund = |m: &Model| {
        let f = formen(m, eg)
            .into_iter()
            .find(|f| f.raum == Some(r))
            .unwrap();
        crate::flaechen::grundflaeche_fertig(m, &f)
    };
    let vorher = grund(&m);
    m.begin("Fliesen");
    let alle = ab(
        AusbauStelle::Fliesen(None),
        Some(AusbauWahl::Typ(WALL_TILES_TYPE_GUID)),
        None,
    );
    m.set_space_finish(r, alle).unwrap();
    m.set_space_finish(r, ab(AusbauStelle::Fliesen(Some(ws)), None, Some(2000.0)))
        .unwrap();
    m.commit();
    let fliesen = m.type_by_guid(WALL_TILES_TYPE_GUID).unwrap();
    let a = RaumAusbau::fuer(&m, &roh).unwrap();
    for (i, s) in a.seiten.iter().enumerate() {
        let h = if i == k { 2000.0 } else { TILES_DEFAULT };
        assert_eq!(s.fliesen, Some((fliesen, h)), "Kante {i}");
        assert_eq!(s.sockel, None, "Kante {i}");
    }
    let (_, q) = raeume(&m, eg).into_iter().find(|x| x.0 == r).unwrap();
    assert!(q.lines.iter().any(|l| l.name.starts_with("Wandfliesen ")));
    assert!(!q.lines.iter().any(|l| l.key == Some("room.skirting")));
    let nachher = grund(&m);
    assert!((nachher - q.area).abs() < 1e-6, "{nachher} {}", q.area);
    assert!(nachher < vorher - 1e5, "{nachher} {vorher}");
}
