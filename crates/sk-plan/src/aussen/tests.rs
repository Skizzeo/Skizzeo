use super::*;
use sk_model::oeffnung::{HeightLine, HeightRef, Opening, OpeningKind};
use sk_model::RefSide;

/// Haus aus dem Umriss `pts` (Außenkante, im Uhrzeigersinn) im EG.
fn haus(pts: &[Vec3]) -> (Model, RunId, StoreyId) {
    let mut m = Model::new();
    m.begin("Haus");
    let b = m.add_building(1);
    let run = m.build_from_polygon(b, pts).expect("Gebäude");
    m.commit();
    let st = m.run(run).unwrap().storey;
    (m, run, st)
}

fn rechteck() -> (Model, RunId, StoreyId) {
    haus(&[
        vec3(0.0, 0.0, 0.0),
        vec3(0.0, 8000.0, 0.0),
        vec3(10_000.0, 8000.0, 0.0),
        vec3(10_000.0, 0.0, 0.0),
    ])
}

fn seite(a: &Aussenmasse, grad: i32) -> &Seite {
    a.seiten.iter().find(|s| s.grad == grad).expect("Seite")
}

fn kette(s: &Seite, art: Kettenart) -> Option<&Kette> {
    s.ketten.iter().find(|k| k.art == art)
}

fn masse(k: &Kette) -> Vec<f64> {
    k.bezuege
        .windows(2)
        .map(|w| (w[1].t - w[0].t).round())
        .collect()
}

/// Putz und Dämmung vor der tragenden Schicht und deren Dicke.
fn schichten(m: &Model, run: RunId) -> (f64, f64) {
    let c = m.chain(run).unwrap();
    let (a, i) = kern(&c);
    ((a - c.outer_offset()).abs(), (i - a).abs())
}

#[test]
fn rechteck_hat_vier_seiten_mit_wand_und_gesamtmass() {
    let (m, run, st) = rechteck();
    let (vor, kern) = schichten(&m, run);
    assert!(kern > 100.0, "tragende Schicht {kern}");
    let a = aussenmasse(&m, st).unwrap();
    assert_eq!(a.seiten.len(), 4);
    let sued = seite(&a, 270);
    assert_eq!(sued.t, vec3(1.0, 0.0, 0.0));
    assert_eq!(
        sued.ketten.iter().map(|k| k.art).collect::<Vec<_>>(),
        [Kettenart::Wand, Kettenart::Gesamt]
    );
    let gesamt = kette(sued, Kettenart::Gesamt).unwrap();
    assert_eq!(masse(gesamt), [(10_000.0 - 2.0 * vor).round()]);
    let wand = kette(sued, Kettenart::Wand).unwrap();
    assert_eq!(
        masse(wand),
        [
            kern.round(),
            (10_000.0 - 2.0 * vor - 2.0 * kern).round(),
            kern.round()
        ]
    );
    // Erste Kette 2 m vor der Rohbauflucht, die nächste 1 m weiter
    assert!((wand.lage - (sued.aussen + 2000.0)).abs() < 1e-6);
    assert!((gesamt.lage - (sued.aussen + 3000.0)).abs() < 1e-6);
    assert!((sued.aussen - (-vor)).abs() < 1e-6);
    // Ecken bewegen die quer stehenden Wände
    assert_eq!(wand.bezuege[0].herkunft, Herkunft::Ecke);
    assert_eq!(wand.bezuege[1].herkunft, Herkunft::Innenseite);
    assert!(matches!(
        wand.bezuege[3].quelle,
        Some(Quelle::Wand { seg: 2, .. })
    ));
    // Ost: von unten nach oben lesbar
    let ost = seite(&a, 0);
    assert_eq!(ost.t, vec3(0.0, 1.0, 0.0));
    assert_eq!(
        masse(kette(ost, Kettenart::Gesamt).unwrap()),
        [(8000.0 - 2.0 * vor).round()]
    );
}

#[test]
fn bezug_aussenkante_wand_misst_ueber_alles() {
    let (mut m, _, st) = rechteck();
    let mut s = m.massstil().clone();
    s.bezug = Aussenbezug::Wand;
    m.begin("Stil");
    assert!(m.set_massstil(s));
    m.commit();
    let a = aussenmasse(&m, st).unwrap();
    let g = kette(seite(&a, 270), Kettenart::Gesamt).unwrap();
    assert_eq!(masse(g), [10_000.0]);
}

#[test]
fn innenwand_und_fenster_kommen_in_ihre_ketten() {
    let (mut m, run, st) = rechteck();
    let iw = m.defaults().interior_wall;
    m.begin("Innenwand");
    let innen = m
        .add_wall_run(
            &[vec3(5000.0, 0.0, 0.0), vec3(5000.0, 8000.0, 0.0)],
            false,
            RefSide::Center,
            st,
            iw,
            Category::InteriorWall,
        )
        .expect("Innenwand");
    let fenster = m
        .add_opening(Opening {
            kind: OpeningKind::Window,
            path: vec![vec3(3010.0, 0.0, 0.0), vec3(2000.0, 0.0, 0.0)],
            bottom: HeightRef::Line(st, HeightLine::Sill),
            top: HeightRef::Line(st, HeightLine::Lintel),
            reveal: None,
            install: Default::default(),
            swing: None,
            lock: Vec::new(),
        })
        .expect("Fenster");
    m.commit();
    let a = aussenmasse(&m, st).unwrap();
    let sued = seite(&a, 270);
    assert_eq!(
        sued.ketten.iter().map(|k| k.art).collect::<Vec<_>>(),
        [Kettenart::Oeffnung, Kettenart::Wand, Kettenart::Gesamt]
    );
    // Öffnungskette: Ecke, Fensterkanten, Ecke
    let (vor, _) = schichten(&m, run);
    let o = kette(sued, Kettenart::Oeffnung).unwrap();
    assert_eq!(
        masse(o),
        [
            (2000.0 - vor).round(),
            1010.0,
            (10_000.0 - vor - 3010.0).round()
        ]
    );
    assert_eq!(
        o.bezuege[1].quelle,
        Some(Quelle::Oeffnung {
            id: fenster,
            ende: 0
        })
    );
    let om = &sued.oeffnungen[0];
    let o_ = m.opening(fenster).unwrap();
    let (z0, z1) = m.opening_band(o_).unwrap();
    assert_eq!((om.t0, om.t1), (2000.0, 3010.0));
    assert!((om.hoehe - (z1 - z0)).abs() < 1e-9);
    let boden = m.storey(st).unwrap().elevation;
    assert_eq!(om.brh, Some(z0 - boden));
    assert!(om.wand.abs() < 1e-6, "Außenhaut bei y = 0: {}", om.wand);
    // Wandkette: beide Seiten der Innenwand (Kern mittig auf x = 5 m)
    let w = kette(sued, Kettenart::Wand).unwrap();
    let iw_c = m.chain(innen).unwrap();
    let (ka, ki) = kern(&iw_c);
    let dicke = (ki - ka).abs();
    let ts: Vec<f64> = w.bezuege.iter().map(|b| b.t.round()).collect();
    assert!(ts.contains(&(5000.0 - dicke / 2.0).round()), "{ts:?}");
    assert!(ts.contains(&(5000.0 + dicke / 2.0).round()), "{ts:?}");
    assert_eq!(ts.len(), 6);
    let b = w
        .bezuege
        .iter()
        .find(|b| b.herkunft == Herkunft::Innenwand)
        .unwrap();
    assert_eq!(b.quelle, Some(Quelle::Innenwand { run: innen }));
    // Nord: dieselbe Innenwand, kein Fenster
    let nord = seite(&a, 90);
    assert!(kette(nord, Kettenart::Oeffnung).is_none());
    assert_eq!(kette(nord, Kettenart::Wand).unwrap().bezuege.len(), 6);
    // BRH ab OK Estrich ist um den Fußbodenaufbau kleiner
    let mut s = m.massstil().clone();
    s.brh_bezug = BrhBezug::Estrich;
    m.begin("Stil");
    m.set_massstil(s);
    m.commit();
    let a = aussenmasse(&m, st).unwrap();
    let e = m.screed_top(st).unwrap();
    assert_eq!(seite(&a, 270).oeffnungen[0].brh, Some(z0 - e));
}

#[test]
fn l_form_bekommt_die_kette_der_spruenge() {
    // L: unten 12 m breit, rechts oben 5 m zurück
    let (m, run, st) = haus(&[
        vec3(0.0, 0.0, 0.0),
        vec3(0.0, 9000.0, 0.0),
        vec3(7000.0, 9000.0, 0.0),
        vec3(7000.0, 4000.0, 0.0),
        vec3(12_000.0, 4000.0, 0.0),
        vec3(12_000.0, 0.0, 0.0),
    ]);
    let (vor, kern) = schichten(&m, run);
    let a = aussenmasse(&m, st).unwrap();
    let nord = seite(&a, 90);
    assert_eq!(
        nord.ketten.iter().map(|k| k.art).collect::<Vec<_>>(),
        [Kettenart::Wand, Kettenart::Sprung, Kettenart::Gesamt]
    );
    let sp = kette(nord, Kettenart::Sprung).unwrap();
    assert_eq!(masse(sp), [(7000.0 - 2.0 * vor).round(), 5000.0]);
    // Die Ketten liegen vor der äußersten Wand der Seite
    assert!((nord.aussen - (9000.0 - vor)).abs() < 1e-6);
    // Wandkette: am Rücksprung die Innenseite der quer stehenden Wand
    let w = kette(nord, Kettenart::Wand).unwrap();
    let ts: Vec<f64> = w.bezuege.iter().map(|b| b.t.round()).collect();
    assert!(ts.contains(&(7000.0 - vor - kern).round()), "{ts:?}");
    // Süd hat keine Sprünge
    assert!(kette(seite(&a, 270), Kettenart::Sprung).is_none());
}

#[test]
fn versatz_und_abgeschaltete_kette() {
    let (mut m, _, st) = rechteck();
    m.begin("Kette verschoben");
    assert!(m.set_massversatz(st, 270, Kettenart::Gesamt, 450.0));
    m.commit();
    let a = aussenmasse(&m, st).unwrap();
    let s = seite(&a, 270);
    let g = kette(s, Kettenart::Gesamt).unwrap();
    assert_eq!(g.versatz, 450.0);
    assert!((g.lage - (s.aussen + 3450.0)).abs() < 1e-6);
    let mut stil = m.massstil().clone();
    stil.set_kette(Kettenart::Wand, false);
    m.begin("Stil");
    m.set_massstil(stil);
    m.commit();
    let a = aussenmasse(&m, st).unwrap();
    let s = seite(&a, 270);
    assert_eq!(s.ketten.len(), 1);
    // Rückt auf den ersten Platz nach
    assert!((s.ketten[0].lage - (s.aussen + 2450.0)).abs() < 1e-6);
}

#[test]
fn gedrehtes_haus_folgt_seiner_hauptrichtung() {
    let w = 30f64.to_radians();
    let r = |x: f64, y: f64| vec3(x * w.cos() - y * w.sin(), x * w.sin() + y * w.cos(), 0.0);
    let (m, run, st) = haus(&[
        r(0.0, 0.0),
        r(0.0, 8000.0),
        r(10_000.0, 8000.0),
        r(10_000.0, 0.0),
    ]);
    let (vor, _) = schichten(&m, run);
    let a = aussenmasse(&m, st).unwrap();
    assert_eq!(a.seiten.len(), 4);
    for s in &a.seiten {
        assert!(s.t.x > 0.0 || (s.t.x.abs() < 1e-9 && s.t.y > 0.0));
        let g = kette(s, Kettenart::Gesamt).unwrap();
        let l = masse(g)[0];
        assert!(
            l == (10_000.0 - 2.0 * vor).round() || l == (8000.0 - 2.0 * vor).round(),
            "{l}"
        );
    }
}

#[test]
fn gruendung_und_leeres_geschoss_ohne_ketten() {
    let m = Model::new();
    let st = m.defaults().storey;
    assert!(aussenmasse(&m, st).is_none());
}

/// Geometrie #116: Eine Außenwand, die 0,3° schräg zur Hauptrichtung
/// steht, zählt als parallel und wird bemaßt; ihr Fenster liegt auf ihrer
/// Außenhaut und gehört in die Öffnungskette, auch weit weg von der
/// Wandmitte.
#[test]
fn fenster_in_fast_paralleler_wand() {
    let k = 0.3f64.to_radians().tan();
    // Nordwand am längsten (Hauptrichtung x), Südwand um 0,3° geneigt
    let (mut m, _, st) = haus(&[
        vec3(0.0, 0.0, 0.0),
        vec3(0.0, 8000.0, 0.0),
        vec3(10_300.0, 8000.0, 0.0),
        vec3(10_000.0, 10_000.0 * k, 0.0),
    ]);
    m.begin("Fenster");
    let fenster = m
        .add_opening(Opening {
            kind: OpeningKind::Window,
            path: vec![vec3(3010.0, 3010.0 * k, 0.0), vec3(2000.0, 2000.0 * k, 0.0)],
            bottom: HeightRef::Line(st, HeightLine::Sill),
            top: HeightRef::Line(st, HeightLine::Lintel),
            reveal: None,
            install: Default::default(),
            swing: None,
            lock: Vec::new(),
        })
        .expect("Fenster");
    m.commit();
    let a = aussenmasse(&m, st).unwrap();
    let sued = seite(&a, 270);
    assert!(kette(sued, Kettenart::Wand).is_some(), "Südwand bemaßt");
    assert!(
        sued.oeffnungen.iter().any(|o| o.id == fenster),
        "Fenster fehlt in der Öffnungskette: {:?}",
        sued.ketten.iter().map(|k| k.art).collect::<Vec<_>>()
    );
}
