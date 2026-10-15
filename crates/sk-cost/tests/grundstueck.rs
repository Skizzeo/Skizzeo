//! Erdarbeiten der Geländeflächen gehören zum Grundstück, nicht zu einem
//! Haus (VOB-Prüfung G3, E1): bei gewähltem Haus fehlen sie und ein Hinweis
//! sagt es; die Häuser zusammen mit den Positionen des Grundstücks ergeben
//! das Projekt.

use sk_cost::befund::Ort;
use sk_cost::{lesen, Kostenblatt, Umfang};
use sk_model::qto_earth::{GRUNDSTUECK, GRUNDSTUECK_SCHLUESSEL};
use sk_model::{qto, szo, BuildingId, GuidGen, Model, TerrainPatch};

const RH1: &str = include_str!("../referenz/rh1-standardhaus.szo");

/// Ebenes Aufmaß 200 × 200 m in 10,00 m ü. NHN.
const AUFMASS: &str = "[terrain] src=\"aufmass.txt\" read=\"2026-10-14\" crs=local hsys=DHHN2016 local=1 e0=100 n0=100 rot=0
[tpoint] nr=1 e=0 n=0 h=10
[tpoint] nr=2 e=200 n=0 h=10
[tpoint] nr=3 e=200 n=200 h=10
[tpoint] nr=4 e=0 n=200 h=10
";

fn rechteck(x0: f64, y0: f64, x1: f64, y1: f64) -> Vec<[f64; 2]> {
    vec![[x0, y0], [x1, y0], [x1, y1], [x0, y1]]
}

fn neues_haus(m: &mut Model, x0: f64, y0: f64) -> BuildingId {
    let b = m.add_building(1);
    let pts = [
        sk_math::vec3(x0, y0, 0.0),
        sk_math::vec3(x0, y0 + 6000.0, 0.0),
        sk_math::vec3(x0 + 7000.0, y0 + 6000.0, 0.0),
        sk_math::vec3(x0 + 7000.0, y0, 0.0),
    ];
    m.build_from_polygon(b, &pts).unwrap();
    b
}

/// RH-1 und zwei weitere Häuser (das erste neu gesetzt, so tragen die
/// Platten nicht die Reihenfolge der Häuser) mit einer Fläche `f`.
fn drei_haeuser(f: TerrainPatch) -> Model {
    let mut m = szo::read_with(
        &format!("{RH1}{AUFMASS}"),
        GuidGen::with_seed(1),
        &lesen::ABSCHNITTE_SZO,
    )
    .expect("lädt")
    .model;
    m.begin("Häuser");
    let a = neues_haus(&mut m, 25e3, 5e3);
    neues_haus(&mut m, 25e3, -15e3);
    assert!(m.remove_building(a));
    neues_haus(&mut m, 25e3, 5e3);
    m.commit();
    m.begin("Fläche");
    assert!(m.add_site_patch(f).is_some());
    m.commit();
    m
}

fn netto(b: &Kostenblatt) -> i64 {
    b.positionen.iter().map(|p| p.gp.0).sum()
}

fn pruefen(name: &str, m: &Model) {
    let s = qto::schedule(m);
    let k = lesen::katalog(m, None);
    let grund: Vec<_> = s
        .auto
        .iter()
        .filter(|a| GRUNDSTUECK_SCHLUESSEL.contains(&a.key))
        .collect();
    assert!(!grund.is_empty(), "{name}: keine Zeile des Grundstücks");
    assert!(grund.iter().all(|a| a.building.is_none()), "{name}");
    // Verfüllung, Abfuhr und Oberboden der Baugrube bleiben am Haus
    assert!(s
        .auto
        .iter()
        .filter(|a| a.key.starts_with("earth.") && !GRUNDSTUECK_SCHLUESSEL.contains(&a.key))
        .all(|a| a.building.is_some()));

    let projekt = lesen::kosten(m, &s, &k, &Umfang::projekt());
    // ihre Hinweise stehen bei der Datei, nicht am Bauteil von Haus 1
    // (VOB-Prüfung #184 F2)
    for f in projekt.befunde.iter().filter(|f| f.satz.contains("Faktor")) {
        assert_eq!(f.ort, Ort::Datei, "{name}: {f:?}");
    }
    assert!(
        projekt.befunde.iter().all(|f| f.satz != GRUNDSTUECK),
        "{name}"
    );
    // ohne das Geschoss des Trägers bleiben sie im Projekt (H-184a)
    let ohne = Umfang {
        gebaeude: None,
        ohne: vec![grund[0].storey],
    };
    let r = s.restrict(m, &ohne);
    assert_eq!(
        r.auto
            .iter()
            .filter(|a| GRUNDSTUECK_SCHLUESSEL.contains(&a.key))
            .count(),
        grund.len(),
        "{name}"
    );
    let mut haeuser = 0;
    for (b, _) in m.buildings().iter() {
        let bl = lesen::kosten(m, &s, &k, &Umfang::gebaeude(b));
        for p in &bl.positionen {
            assert!(
                p.ansatz.iter().all(|a| a.gebaeude == Some(b)
                    && !a.auto.is_some_and(|k| GRUNDSTUECK_SCHLUESSEL.contains(&k))),
                "{name}: {} im Haus",
                p.oz
            );
        }
        let h: Vec<_> = bl
            .befunde
            .iter()
            .filter(|f| f.satz == GRUNDSTUECK)
            .collect();
        assert_eq!(h.len(), 1, "{name}: {:#?}", bl.befunde);
        haeuser += netto(&bl);
    }
    // eine Position gehört ganz zum Grundstück oder ganz zu Häusern
    let mut grundstueck = 0;
    for p in &projekt.positionen {
        let ohne = p.ansatz.iter().filter(|a| a.gebaeude.is_none()).count();
        assert!(
            ohne == 0 || ohne == p.ansatz.len(),
            "{name}: {} gemischt",
            p.oz
        );
        if ohne > 0 {
            grundstueck += p.gp.0;
        }
    }
    assert!(grundstueck > 0, "{name}");
    // je Position höchstens ein Cent Rundung
    let n = projekt.positionen.len() as i64;
    let rest = netto(&projekt) - haeuser - grundstueck;
    assert!(rest.abs() <= n, "{name}: {rest} ct");
}

#[test]
fn haeuser_und_grundstueck_ergeben_das_projekt() {
    // Auftrag aus den Baugruben, Lieferung, Abtrag mit Überschuss
    for h in [300.0, 900.0, -500.0] {
        let f = TerrainPatch::flat(rechteck(-30e3, -30e3, -25e3, -25e3), h);
        pruefen(&format!("{h}"), &drei_haeuser(f));
    }
    let f = TerrainPatch::flat(rechteck(-30e3, -30e3, -15e3, -15e3), 1500.0);
    let m = drei_haeuser(f);
    pruefen("groß", &m);
    // nach Speichern und Öffnen genauso
    let text = szo::write(&m);
    let wieder = szo::read_with(&text, GuidGen::with_seed(3), &lesen::ABSCHNITTE_SZO)
        .expect("lädt")
        .model;
    pruefen("wieder", &wieder);
}
