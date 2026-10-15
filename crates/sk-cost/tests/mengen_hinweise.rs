//! Hinweise der Automatikmengen (`AutoMenge::hinweis`, VOB-Prüfung G3 D1):
//! im Kostenblatt als Befund, im LV an der Position.

use sk_cost::befund::{Ort, Schwere};
use sk_cost::lv::{lv_aus, LvWahl};
use sk_cost::{lesen, Kostenblatt, Umfang};
use sk_model::{qto, szo, GuidGen, Model, TerrainPatch};

/// Wortlaut aus `sk_model::qto_earth` (dort privat).
const OHNE_AUFMASS: &str = "kein Höhenaufmaß";
const FAKTOR_TEXT: &str = "Eigenboden fest zu eingebaut";

const RH1: &str = include_str!("../referenz/rh1-standardhaus.szo");

/// Ebenes Aufmaß 200 × 200 m in 10,00 m ü. NHN um das Haus.
const AUFMASS: &str = "[terrain] src=\"aufmass.txt\" read=\"2026-10-14\" crs=local hsys=DHHN2016 local=1 e0=100 n0=100 rot=0
[tpoint] nr=1 e=0 n=0 h=10
[tpoint] nr=2 e=200 n=0 h=10
[tpoint] nr=3 e=200 n=200 h=10
[tpoint] nr=4 e=0 n=200 h=10
";

/// RH-1, mit oder ohne Aufmaß, dazu eine Grünfläche 15 × 15 m 1,50 m über
/// ±0,00 abseits des Hauses.
fn haus(aufmass: bool) -> Model {
    let mut t = RH1.to_string();
    if aufmass {
        t += AUFMASS;
    }
    let mut m = szo::read_with(&t, GuidGen::with_seed(1), &lesen::ABSCHNITTE_SZO)
        .expect("lädt")
        .model;
    let r = vec![
        [-30e3, -30e3],
        [-15e3, -30e3],
        [-15e3, -15e3],
        [-30e3, -15e3],
    ];
    m.begin("Fläche");
    assert!(m.add_site_patch(TerrainPatch::flat(r, 1500.0)).is_some());
    m.commit();
    m
}

fn blatt(m: &Model) -> Kostenblatt {
    let k = lesen::katalog(m, None);
    lesen::kosten(m, &qto::schedule(m), &k, &Umfang::projekt())
}

#[test]
fn flaeche_ohne_aufmass_meldet_sich() {
    let m = haus(false);
    let b = blatt(&m);
    let h: Vec<_> = b
        .befunde
        .iter()
        .filter(|f| f.satz.contains(OHNE_AUFMASS))
        .collect();
    assert_eq!(h.len(), 1, "{:#?}", b.befunde);
    assert_eq!(h[0].schwere, Schwere::Hinweis);
}

#[test]
fn hinweis_an_der_position_im_lv() {
    let m = haus(true);
    let b = blatt(&m);
    let faktor = FAKTOR_TEXT;
    assert!(
        b.befunde.iter().any(|f| f.satz.starts_with(faktor)),
        "{:#?}",
        b.befunde
    );
    let k = lesen::katalog(&m, None);
    let los = k
        .lose
        .iter()
        .find(|l| l.parent.is_none() && l.name == "Außenanlagen")
        .expect("Los 6");
    let w = LvWahl {
        los: los.guid,
        untertitel: false,
        preise: true,
        heute: None,
    };
    let lv = lv_aus(&m, &b, &k, &w);
    let an_position: Vec<_> = lv
        .befunde
        .iter()
        .filter(|f| f.satz.starts_with(faktor))
        .collect();
    assert!(!an_position.is_empty(), "{:#?}", lv.befunde);
    for f in an_position {
        let Ort::Position(oz) = &f.ort else {
            panic!("{f:?}");
        };
        assert!(
            lv.titel
                .iter()
                .flat_map(|t| &t.positionen)
                .any(|p| &p.oz == oz),
            "{oz} im LV"
        );
    }
}

/// Ohne Sohlplatte trägt keine Zeile den Hinweis: das Kostenblatt meldet ihn
/// für die Datei (VOB-Prüfung G3 D2).
#[test]
fn ohne_traeger_hinweis_fuer_die_datei() {
    let mut m = Model::with_seed(7);
    m.begin("Fläche");
    let r = vec![[30e3, 30e3], [40e3, 30e3], [40e3, 40e3], [30e3, 40e3]];
    assert!(m.add_site_patch(TerrainPatch::flat(r, 100.0)).is_some());
    m.commit();
    let b = blatt(&m);
    let h: Vec<_> = b
        .befunde
        .iter()
        .filter(|f| f.satz == sk_model::qto_earth::OHNE_TRAEGER)
        .collect();
    assert_eq!(h.len(), 1, "{:#?}", b.befunde);
    assert_eq!(h[0].ort, Ort::Datei);
    assert_eq!(h[0].schwere, Schwere::Hinweis);
    assert!(blatt(&haus(true))
        .befunde
        .iter()
        .all(|f| f.satz != sk_model::qto_earth::OHNE_TRAEGER));
}
