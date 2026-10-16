//! Wohnfläche gegen Handrechnung: Rechteck, L-Form, schräge Ecke, Pfeiler,
//! Nischen, Faktor und das ganze Projekt. Putz der Wohnzimmervorlage 15 mm.

use super::*;
use crate::erkennung::{Begrenzung, Kante, RaumForm, Seite};
use crate::oeffnungen::{OeffnungAmRaum, OpeningKind, Rohbau};
use sk_math::vec3;
use sk_model::model::{FINISH_TYPES, TECHNICAL_SPACE_GUID};
use sk_model::{Category, ElementId, Model, RefSide, Space, StoreyId};

struct Haus {
    m: Model,
    eg: StoreyId,
    /// Wände für die Nummern von Kanten und Öffnungen.
    w: Vec<ElementId>,
}

fn haus() -> Haus {
    let mut m = Model::with_seed(7);
    let b = m.add_building(2);
    let eg = m.ground_of(Some(b)).unwrap();
    let pts = [
        vec3(0.0, 0.0, 0.0),
        vec3(0.0, 8000.0, 0.0),
        vec3(10000.0, 8000.0, 0.0),
        vec3(10000.0, 0.0, 0.0),
    ];
    let set = m.type_by_guid(sk_model::EXTERIOR_TYPE_GUID).unwrap();
    let run = m
        .add_wall_run(&pts, true, RefSide::Left, eg, set, Category::ExteriorWall)
        .unwrap();
    for t in &FINISH_TYPES {
        m.ensure_finish_type(t.guid).unwrap();
    }
    let w = m.run(run).unwrap().segments.clone();
    Haus { m, eg, w }
}

/// Ring von Kanten an der Wand `wand`, Punkte im Rohbaumaß.
fn ring(p: &[(f64, f64)], wand: ElementId) -> Vec<Kante> {
    (0..p.len())
        .map(|i| {
            let (a, b) = (p[i], p[(i + 1) % p.len()]);
            let (a, b) = (vec3(a.0, a.1, 0.0), vec3(b.0, b.1, 0.0));
            Kante {
                a,
                b,
                quelle: Begrenzung::Wand {
                    wand,
                    schicht: 0,
                    seite: Seite::Links,
                    von: 0.0,
                    bis: (b - a).length(),
                },
                ausbau_typ: Vec::new(),
            }
        })
        .collect()
}

fn rohbau(
    h: &Haus,
    umriss: Vec<Kante>,
    loecher: Vec<Vec<Kante>>,
    o: Vec<OeffnungAmRaum>,
) -> Rohbau {
    Rohbau {
        form: RaumForm {
            geschoss: h.eg,
            umriss,
            loecher,
            ecken: Vec::new(),
            flaeche: 0.0,
            umfang: 0.0,
            raum: None,
            mitte: vec3(0.0, 0.0, 0.0),
            stempel: vec3(0.0, 0.0, 0.0),
        },
        z_boden: 0.0,
        z_decke: 2635.0,
        oeffnungen: o,
    }
}

fn rechteck(h: &Haus) -> Vec<Kante> {
    ring(
        &[(0.0, 0.0), (10000.0, 0.0), (10000.0, 8000.0), (0.0, 8000.0)],
        h.w[0],
    )
}

#[allow(clippy::too_many_arguments)]
fn oe(id: ElementId, von: f64, bis: f64, uk: f64, tiefe: f64, art: OpeningKind) -> OeffnungAmRaum {
    OeffnungAmRaum {
        oeffnung: id,
        kante: 0,
        von,
        bis,
        uk,
        ok: uk + 2135.0,
        tiefe,
        art,
        aufschlag_hier: None,
        weiter: [0.0; 2],
    }
}

fn rechnen(h: &Haus, roh: &Rohbau) -> WohnRaum {
    let aus = RaumAusbau::fuer(&h.m, roh).unwrap();
    raum(&h.m, roh, &aus)
}

fn texte(r: &WohnRaum, art: Art) -> Vec<&str> {
    r.teile
        .iter()
        .filter(|t| t.art == art)
        .map(|t| t.text.as_str())
        .collect()
}

#[test]
fn rechteck_an_der_putzoberflaeche() {
    let h = haus();
    let r = rechnen(&h, &rohbau(&h, rechteck(&h), vec![], vec![]));
    assert_eq!(texte(&r, Art::Basis), ["9,970 × 7,970"]);
    assert_eq!(r.teile[0].nr, Some(1));
    assert_eq!(r.teile[0].wert, 9970.0 * 7970.0);
    assert_eq!((r.grund, qm(r.grund)), (7946, "79,46 m²".into()));
    // 2,635 Rohbau, 160 Fußbodenaufbau, 10 Deckenputz
    assert_eq!(r.faktor.lichte_hoehe, 2465.0);
    assert_eq!((r.faktor.prozent, r.wohn), (100.0, 7946));
    assert_eq!(r.faktor.grund, "lichte Höhe 2,465 m");
    assert_eq!(r.umriss.len(), 1);
}

#[test]
fn l_form_in_zwei_rechtecken() {
    let h = haus();
    let p = [
        (0.0, 0.0),
        (6000.0, 0.0),
        (6000.0, 3000.0),
        (3000.0, 3000.0),
        (3000.0, 5000.0),
        (0.0, 5000.0),
    ];
    let r = rechnen(&h, &rohbau(&h, ring(&p, h.w[0]), vec![], vec![]));
    assert_eq!(texte(&r, Art::Basis), ["5,970 × 2,970", "2,970 × 2,000"]);
    // 17,7309 + 5,9400
    assert_eq!(r.grund, 2367);
    let summe: f64 = r.teile.iter().map(|t| t.wert).sum();
    let fertig = sk_math::polygon::area(&r.umriss[0]);
    assert!((summe - fertig).abs() < 1.0, "{summe} {fertig}");
    // Jede Teilfläche hat ihren Umriss mit derselben Fläche
    for t in &r.teile {
        assert!((sk_math::polygon::area(&t.umriss) - t.wert).abs() < 1.0);
    }
}

#[test]
fn schraege_ecke_als_dreieck() {
    // Ohne Ausbau gerechnet: nur die Zerlegung
    let p: Vec<Vec3> = [
        (0.0, 0.0),
        (4000.0, 0.0),
        (4000.0, 2000.0),
        (2000.0, 4000.0),
        (0.0, 4000.0),
    ]
    .iter()
    .map(|&(x, y)| vec3(x, y, 0.0))
    .collect();
    let t = zerlegen(&p);
    let mut texte: Vec<&str> = t.iter().map(|x| x.0.as_str()).collect();
    texte.sort();
    assert_eq!(
        texte,
        ["2,000 × 2,000", "4,000 × 2,000", "½ × 2,000 × 2,000"]
    );
    assert_eq!(t.iter().map(|x| x.1).sum::<f64>(), 14e6);
    for (_, wert, u) in &t {
        assert!((sk_math::polygon::area(u) - wert).abs() < 1.0);
    }
    // Ein Parallelogramm, dessen schräge Ränder sich kreuzen: ein Trapez
    let p: Vec<Vec3> = [
        (0.0, 0.0),
        (1000.0, 0.0),
        (4000.0, 1000.0),
        (3000.0, 1000.0),
    ]
    .iter()
    .map(|&(x, y)| vec3(x, y, 0.0))
    .collect();
    let t = zerlegen(&p);
    assert_eq!(t.len(), 1, "{t:?}");
    assert_eq!(t[0].0, "(1,000 + 1,000) / 2 × 1,000");
    assert_eq!(t[0].1, 1e6);
}

#[test]
fn gedrehter_raum_rechnet_gleich() {
    // Rechteck 5 × 3 m um 30° gedreht
    let (s, c) = 30f64.to_radians().sin_cos();
    let dreh = |x: f64, y: f64| vec3(1000.0 + x * c - y * s, 2000.0 + x * s + y * c, 0.0);
    let p = vec![
        dreh(0.0, 0.0),
        dreh(5000.0, 0.0),
        dreh(5000.0, 3000.0),
        dreh(0.0, 3000.0),
    ];
    let t = zerlegen(&p);
    assert_eq!(t.len(), 1);
    assert_eq!(t[0].0, "5,000 × 3,000");
    assert_eq!(t[0].1, 15e6);
}

#[test]
fn pfeiler_ueber_0_1_qm_abgezogen() {
    let h = haus();
    // Im Uhrzeigersinn: Pfeiler 40 × 40 und 24 × 24 cm, mit Putz 43 × 43
    // bzw. 27 × 27 cm
    let gross = ring(
        &[
            (4000.0, 4000.0),
            (4000.0, 4400.0),
            (4400.0, 4400.0),
            (4400.0, 4000.0),
        ],
        h.w[1],
    );
    let klein = ring(
        &[
            (7000.0, 4000.0),
            (7000.0, 4240.0),
            (7240.0, 4240.0),
            (7240.0, 4000.0),
        ],
        h.w[2],
    );
    let r = rechnen(&h, &rohbau(&h, rechteck(&h), vec![gross, klein], vec![]));
    let w = |i: usize| h.m.element(h.w[i]).unwrap().number.clone();
    assert_eq!(texte(&r, Art::Abzug), [format!("{} 0,430 × 0,430", w(1))]);
    let abzug = r.teile.iter().find(|t| t.art == Art::Abzug).unwrap();
    assert_eq!(abzug.wert, -430.0 * 430.0);
    assert_eq!(
        texte(&r, Art::Hinweis),
        [format!(
            "{} 0,073 m² bleibt in der Fläche (bis 0,1 m², § 3 Abs. 3 Nr. 1)",
            w(2)
        )]
    );
    // 79,4609 − 0,1849
    assert_eq!(r.grund, 7928);
    assert_eq!(r.umriss.len(), 3);
}

#[test]
fn nischen_nach_woflv() {
    let h = haus();
    let o = vec![
        // bodentief, 175 innere Einbautiefe + 15 Putz: zählt, zwischen den
        // geputzten Laibungen 1,010 − 2 × 0,015 breit
        oe(h.w[0], 1000.0, 2010.0, 0.0, 175.0, OpeningKind::Window),
        // bodentief, nur 100 + 15 tief
        oe(h.w[1], 3000.0, 4010.0, 0.0, 100.0, OpeningKind::Window),
        // Brüstung
        oe(h.w[2], 5000.0, 6010.0, 900.0, 175.0, OpeningKind::Window),
        oe(h.w[3], 7000.0, 7885.0, 0.0, 240.0, OpeningKind::InnerDoor),
        oe(h.w[2], 8500.0, 9510.0, 0.0, 175.0, OpeningKind::Door),
    ];
    let r = rechnen(&h, &rohbau(&h, rechteck(&h), vec![], o));
    let w = |i: usize| h.m.element(h.w[i]).unwrap().number.clone();
    assert_eq!(
        texte(&r, Art::Zuschlag),
        [format!("Fensternische {} 0,980 × 0,190", w(0))]
    );
    let n = r.teile.iter().find(|t| t.art == Art::Zuschlag).unwrap();
    assert_eq!((n.nr, n.wert), (Some(2), 980.0 * 190.0));
    // Von der Putzoberfläche bis zum Fenster
    let (lo, hi) = n.umriss.iter().fold((f64::MAX, f64::MIN), |(lo, hi), p| {
        (lo.min(p.y), hi.max(p.y))
    });
    assert_eq!((lo, hi), (-175.0, 15.0));
    assert_eq!(
        texte(&r, Art::Hinweis),
        [
            format!(
                "Fensternische {} 0,115 m tief zählt nicht (bis 0,13 m, § 3 Abs. 3 Nr. 4)",
                w(1)
            ),
            format!("Türnische {} zählt nicht (§ 3 Abs. 3 Nr. 3)", w(3)),
            format!("Türnische {} zählt nicht (§ 3 Abs. 3 Nr. 3)", w(2)),
        ]
    );
    // 79,4609 + 0,1862
    assert_eq!(r.grund, 7965);
}

#[test]
fn faktor_nach_hoehe_vorlage_und_raum() {
    let mut h = haus();
    // Lichte Höhe 1,730 m: zur Hälfte
    let mut roh = rohbau(&h, rechteck(&h), vec![], vec![]);
    roh.z_decke = 1900.0;
    let r = rechnen(&h, &roh);
    assert_eq!((r.faktor.lichte_hoehe, r.faktor.prozent), (1730.0, 50.0));
    assert_eq!(r.faktor.grund, "lichte Höhe 1,730 m: 50 % (§ 4 Nr. 2)");
    assert_eq!((r.grund, r.wohn), (7946, 3973));
    roh.z_decke = 1100.0;
    assert_eq!(rechnen(&h, &roh).wohn, 0, "unter 1 m");
    // Technikraum: Zubehörraum
    let id = h.m.add_space(
        h.eg,
        Space::neu(vec3(5000.0, 4000.0, 0.0), "Technik".into()),
    );
    assert!(h
        .m
        .set_space(id, |s| s.nutzung = Some(TECHNICAL_SPACE_GUID)));
    let mut roh = rohbau(&h, rechteck(&h), vec![], vec![]);
    roh.form.raum = Some(id);
    let r = rechnen(&h, &roh);
    assert_eq!((r.faktor.prozent, r.wohn), (0.0, 0));
    assert_eq!(
        r.faktor.grund,
        "Raumvorlage „Haustechnikraum“: 0 % (Zubehörraum, § 2 Abs. 3)"
    );
    assert_eq!(
        (r.nummer.as_str(), r.name.as_str()),
        (h.m.element(id).unwrap().number.as_str(), "Technik")
    );
    // Ein Faktor am Raum geht vor: 79,46 × 0,75 = 59,595
    assert!(h.m.set_space(id, |s| s.faktor = Some(0.75)));
    let r = rechnen(&h, &roh);
    assert_eq!((r.faktor.prozent, r.wohn), (75.0, 5960));
    assert_eq!(r.faktor.grund, "am Raum festgelegt");
}

#[test]
fn runden_kaufmaennisch() {
    assert_eq!(hundertstel(50_000.0), 5);
    assert_eq!(hundertstel(49_999.0), 5);
    assert_eq!(hundertstel(44_999.0), 4);
    assert_eq!(hundertstel(-50_000.0), -5);
    assert_eq!(anrechnen(1235, 50.0), 618);
    assert_eq!(anrechnen(1234, 25.0), 309);
    assert_eq!(qm(123_456), "1.234,56 m²");
    assert_eq!(qm(5), "0,05 m²");
    assert_eq!(prozent(50.0), "50 %");
}

#[test]
fn ohne_ausbautypen_putz_der_vorgabe() {
    let mut m = Model::with_seed(7);
    let b = m.add_building(1);
    let eg = m.ground_of(Some(b)).unwrap();
    let iw = m.defaults().interior_wall;
    let p = [vec3(0.0, 0.0, 0.0), vec3(1000.0, 0.0, 0.0)];
    let run = m
        .add_wall_run(&p, false, RefSide::Center, eg, iw, Category::InteriorWall)
        .unwrap();
    let w = m.run(run).unwrap().segments.clone();
    let h = Haus { m, eg, w };
    let r = rechnen(&h, &rohbau(&h, rechteck(&h), vec![], vec![]));
    assert_eq!(texte(&r, Art::Basis), ["9,970 × 7,970"]);
    assert_eq!(
        texte(&r, Art::Hinweis),
        ["ohne Ausbautypen des Raumbuchs: Innenputz 15 mm angenommen"]
    );
    assert_eq!(r.faktor.lichte_hoehe, 2475.0, "ohne Deckenputz");
}

#[test]
fn projekt_nach_gebaeude_geschoss_und_raum() {
    let mut m = Model::with_seed(5);
    let b = m.add_building(2);
    let pts = [
        vec3(0.0, 0.0, 0.0),
        vec3(0.0, 8000.0, 0.0),
        vec3(10000.0, 8000.0, 0.0),
        vec3(10000.0, 0.0, 0.0),
    ];
    let st = m.ground_of(Some(b)).unwrap();
    let aw = m.type_by_guid(sk_model::EXTERIOR_TYPE_GUID).unwrap();
    let iw = m.defaults().interior_wall;
    let t = m.wall_layers(aw).iter().map(|l| l.thickness).sum::<f64>();
    m.begin("Haus");
    m.add_wall_run(&pts, true, RefSide::Left, st, aw, Category::ExteriorWall)
        .unwrap();
    m.add_wall_run(
        &[vec3(4000.0, t, 0.0), vec3(4000.0, 8000.0 - t, 0.0)],
        false,
        RefSide::Center,
        st,
        iw,
        Category::InteriorWall,
    )
    .unwrap();
    assert!(crate::abgleichen(&mut m, &[st]) > 0);
    let ids: Vec<ElementId> = crate::raeume(&m, st)
        .iter()
        .filter_map(|f| f.raum)
        .collect();
    for id in &ids {
        assert!(crate::ausbau::typen_anlegen(&mut m, *id));
    }
    assert!(m.commit().is_some());
    let w = projekt(&m);
    assert_eq!(w.gebaeude.len(), 1);
    let g = &w.gebaeude[0];
    assert_eq!(g.name, m.building(b).unwrap().name);
    assert_eq!(g.geschosse.len(), 1, "das OG hat keinen gespeicherten Raum");
    assert_eq!(g.geschosse[0].name, m.storey(st).unwrap().name);
    let rs = &g.geschosse[0].raeume;
    assert_eq!(rs.len(), 2);
    assert!(rs[0].nummer < rs[1].nummer);
    // Je Raum: Rohbaumaß minus Ausbau je Kante
    for r in rs {
        let f = crate::raeume(&m, st)
            .into_iter()
            .find(|f| f.raum == r.raum)
            .unwrap();
        let roh = Rohbau::neu(&m, f);
        let aus = RaumAusbau::fuer(&m, &roh).unwrap();
        let d = crate::ausbau::dicken(&m, &roh, &aus);
        let p: Vec<Vec3> = roh.form.umriss.iter().map(|k| k.a).collect();
        let fertig = sk_math::flaeche::versatz(&p, &d).unwrap();
        assert_eq!(r.grund, hundertstel(sk_math::polygon::area(&fertig)));
        assert_eq!(r.wohn, r.grund);
    }
    assert_eq!(w.wohn(), rs[0].wohn + rs[1].wohn);
    assert_eq!(w.grund(), w.wohn());
    assert_eq!(w.raeume().count(), 2);
}

/// Ansätze runden kaufmännisch wie im Aufmaß: 9,250 × 4,250 = 39,3125
/// zeigt 39,313, nicht die gerade Ziffer (Test an #195).
#[test]
fn ansatz_rundet_kaufmaennisch() {
    assert_eq!(qm3(9250.0 * 4250.0), "39,313 m²");
    assert_eq!(qm3(-9250.0 * 4250.0), "-39,313 m²");
    assert_eq!(qm3(19_731_000.0), "19,731 m²");
    assert_eq!(m(4970.0), "4,970");
    assert_eq!(m(2.5), "0,003");
    assert_eq!(m(0.4), "0,000");
}

/// Haus 10 × 8 m mit einem Raum und seinen Ausbautypen; dazu die
/// Definitionen der Werksstütze und der Werkstreppe.
fn haus_mit_raum() -> (Model, StoreyId, sk_model::ExtDef, sk_model::ExtDef) {
    let mut m = Model::with_seed(5);
    let b = m.add_building(2);
    let pts = [
        vec3(0.0, 0.0, 0.0),
        vec3(0.0, 8000.0, 0.0),
        vec3(10000.0, 8000.0, 0.0),
        vec3(10000.0, 0.0, 0.0),
    ];
    let st = m.ground_of(Some(b)).unwrap();
    let aw = m.type_by_guid(sk_model::EXTERIOR_TYPE_GUID).unwrap();
    m.begin("Haus");
    m.add_wall_run(&pts, true, RefSide::Left, st, aw, Category::ExteriorWall)
        .unwrap();
    assert!(crate::abgleichen(&mut m, &[st]) > 0);
    let r = crate::raeume(&m, st)[0].raum.unwrap();
    assert!(crate::ausbau::typen_anlegen(&mut m, r));
    let lies = |t: &str| sk_model::ExtDef::lesen(t).unwrap();
    let stuetze = lies(include_str!("../../sk-szb/beispiele/werk.stuetze.szb"));
    let treppe = lies(include_str!("../../sk-szb/beispiele/werk.treppe.szb"));
    m.put_ext_def(stuetze.clone()).unwrap();
    m.put_ext_def(treppe.clone()).unwrap();
    assert!(m.commit().is_some());
    (m, st, stuetze, treppe)
}

/// Stütze 40/40 (0,16 m², raumhoch) und Treppe mit 16 Steigungen
/// (4,20 × 1,00 m) werden abgezogen (§ 3 Abs. 3 Nr. 1 und 2), die Stütze
/// 24/24 (0,0576 m²) bleibt mit Hinweis (Review #195 B1). Die Stütze zählt
/// mit dem Putz des Raums ringsum, 0,43 × 0,43 m, wie ein Pfeiler aus einem
/// Wandring (H-195i); die Treppe ohne.
#[test]
fn stuetzen_und_treppen_nach_woflv() {
    let (mut m, st, stuetze, treppe) = haus_mit_raum();
    let vorher = projekt(&m).raeume().next().unwrap().grund;
    m.begin("Einbauten");
    let mut gross = sk_model::ExtPart::new(&stuetze, [7000.0, 5000.0]);
    gross.werte = vec![("b".into(), 400.0), ("d".into(), 400.0)];
    m.add_ext(st, gross).unwrap();
    m.add_ext(st, sk_model::ExtPart::new(&stuetze, [8000.0, 2000.0]))
        .unwrap();
    m.add_ext(st, sk_model::ExtPart::new(&treppe, [1000.0, 3000.0]))
        .unwrap();
    assert!(m.commit().is_some());
    let w = projekt(&m);
    let r = w.raeume().next().unwrap();
    let abzug = texte(r, Art::Abzug);
    assert_eq!(abzug.len(), 2, "{abzug:?}");
    assert!(
        abzug[0].ends_with("0,430 × 0,430 einschl. Putz 15 mm (§ 3 Abs. 3 Nr. 1)"),
        "{abzug:?}"
    );
    assert!(
        abzug[1].ends_with("4,200 × 1,000 (§ 3 Abs. 3 Nr. 2)"),
        "{abzug:?}"
    );
    let hinweise = texte(r, Art::Hinweis);
    assert!(
        hinweise
            .iter()
            .any(|h| h.ends_with("0,073 m² bleibt in der Fläche (bis 0,1 m², § 3 Abs. 3 Nr. 1)")),
        "{hinweise:?}"
    );
    // 0,1849 + 4,20 m², je Raum gerundet: auf 0,01 m² genau
    assert!((vorher - 438 - r.grund).abs() <= 1, "{vorher} {}", r.grund);
}

/// Stütze 50/50 nur 1,40 m hoch und Treppe mit drei Steigungen bleiben in
/// der Fläche, je mit Hinweis (§ 3 Abs. 3 Nr. 1 und 2; Review #195
/// H-195k). Höhe und Steigungen sind an den Einbauten gesetzt, weil die
/// Beispiele der Werkbank sie nicht so klein zulassen.
#[test]
fn niedrige_stuetze_und_kurze_treppe_bleiben() {
    let (mut m, st, stuetze, treppe) = haus_mit_raum();
    m.begin("Einbauten");
    let mut s = sk_model::ExtPart::new(&stuetze, [7000.0, 5000.0]);
    s.werte = vec![("b".into(), 500.0), ("d".into(), 500.0)];
    m.add_ext(st, s).unwrap();
    m.add_ext(st, sk_model::ExtPart::new(&treppe, [1000.0, 3000.0]))
        .unwrap();
    assert!(m.commit().is_some());
    let mut liste = einbau::sammeln(&m).remove(&st).unwrap().liste().to_vec();
    assert_eq!(liste.len(), 2);
    for e in &mut liste {
        if e.treppe {
            e.steigungen = Some(3.0);
        } else {
            e.z.1 = e.z.0 + 1400.0;
        }
    }
    let f = crate::raeume(&m, st).remove(0);
    let roh = Rohbau::neu(&m, f);
    let aus = RaumAusbau::fuer(&m, &roh).unwrap();
    let ohne = raum(&m, &roh, &aus);
    let r = raum_mit(&m, &roh, &aus, &Einbauten::neu(liste));
    assert!(texte(&r, Art::Abzug)
        .iter()
        .all(|t| !t.contains("§ 3 Abs. 3 Nr. 2")));
    let hinweise = texte(&r, Art::Hinweis);
    assert!(
        hinweise
            .iter()
            .any(|h| h.contains("m hoch bleibt in der Fläche (bis 1,50 m, § 3 Abs. 3 Nr. 1)")),
        "{hinweise:?}"
    );
    assert!(
        hinweise
            .iter()
            .any(|h| h
                .ends_with("mit 3 Steigungen bleibt in der Fläche (bis drei, § 3 Abs. 3 Nr. 2)")),
        "{hinweise:?}"
    );
    assert_eq!(r.grund, ohne.grund);
}

/// Das Geschoss der Rechnung entsteht in `sammeln` in einem Gang; Höhen
/// wie über `Model::ext_lage` mit `Model::ext_geschoss` (Review #195 B4).
#[test]
fn sammeln_wie_ext_lage() {
    let (mut m, st, stuetze, treppe) = haus_mit_raum();
    m.begin("Einbauten");
    m.add_ext(st, sk_model::ExtPart::new(&stuetze, [7000.0, 5000.0]))
        .unwrap();
    m.add_ext(st, sk_model::ExtPart::new(&treppe, [1000.0, 3000.0]))
        .unwrap();
    assert!(m.commit().is_some());
    let alle = einbau::sammeln(&m);
    let liste = alle[&st].liste();
    assert_eq!(liste.len(), 2);
    for e in liste {
        let (lage, erg) = m.ext_lage(e.id).unwrap();
        let z: Vec<f64> = erg
            .koerper
            .iter()
            .flat_map(|k| {
                k.umriss.iter().flat_map(move |q| {
                    [k.von, k.bis].map(|w| lage.welt(k.punkt(q[0], q[1], w, 0.0)).z)
                })
            })
            .collect();
        let lo = z.iter().copied().fold(f64::INFINITY, f64::min);
        let hi = z.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        assert_eq!(e.z, (lo, hi));
    }
    assert_eq!(liste[1].steigungen, Some(16.0));
}

/// Das Raster findet nur Einbauten unter dem Rechteck des Raums, in der
/// Reihenfolge des Modells, auch über Feldgrenzen (Review #195 B4).
#[test]
fn einbauten_im_raster() {
    let (m, ..) = haus_mit_raum();
    let id = m.elements().iter().next().unwrap().0;
    let quadrat = |x: f64, y: f64, a: f64| Einbau {
        id,
        treppe: false,
        stuecke: vec![vec![
            vec3(x, y, 0.0),
            vec3(x + a, y, 0.0),
            vec3(x + a, y + a, 0.0),
            vec3(x, y + a, 0.0),
        ]],
        z: (0.0, 3000.0),
        steigungen: None,
        loch: false,
    };
    let e = Einbauten::neu(vec![
        quadrat(4900.0, 100.0, 300.0),
        quadrat(-7000.0, -7000.0, 400.0),
        quadrat(1000.0, 1000.0, 12000.0),
        quadrat(30000.0, 30000.0, 400.0),
    ]);
    let x0 = |v: Vec<&Einbau>| v.iter().map(|e| e.stuecke[0][0].x).collect::<Vec<_>>();
    assert_eq!(x0(e.bei([0.0, 0.0, 5000.0, 5000.0])), vec![4900.0, 1000.0]);
    assert_eq!(
        x0(e.bei([-8000.0, -8000.0, -6000.0, -6000.0])),
        vec![-7000.0]
    );
    assert_eq!(
        x0(e.bei([12000.0, 12000.0, 12500.0, 12500.0])),
        vec![1000.0]
    );
    assert_eq!(
        x0(e.bei([20000.0, 0.0, 25000.0, 1000.0])),
        Vec::<f64>::new()
    );
    assert_eq!(
        x0(e.bei([-1e6, -1e6, 1e6, 1e6])),
        vec![4900.0, -7000.0, 1000.0, 30000.0]
    );
    assert!(Einbauten::default().bei([0.0, 0.0, 1.0, 1.0]).is_empty());
}

/// Innenwand 17,5 cm bei x = 4,001 m: Die Teilflächen rechnen auf den
/// Millimeter, die Grundfläche wie am Stempel genau; eine Zeile
/// „Rundungsausgleich“ schließt die Lücke, so dass die Ansätze aufgehen
/// (Review #195 B3).
#[test]
fn grundflaeche_wie_am_stempel() {
    let mut m = Model::with_seed(5);
    let b = m.add_building(2);
    let pts = [
        vec3(0.0, 0.0, 0.0),
        vec3(0.0, 8000.0, 0.0),
        vec3(10000.0, 8000.0, 0.0),
        vec3(10000.0, 0.0, 0.0),
    ];
    let st = m.ground_of(Some(b)).unwrap();
    let aw = m.type_by_guid(sk_model::EXTERIOR_TYPE_GUID).unwrap();
    let iw = m.type_by_guid(sk_model::INTERIOR_TYPE_GUID).unwrap();
    let t = m.wall_layers(aw).iter().map(|l| l.thickness).sum::<f64>();
    m.begin("Haus");
    m.add_wall_run(&pts, true, RefSide::Left, st, aw, Category::ExteriorWall)
        .unwrap();
    m.add_wall_run(
        &[vec3(4001.0, t, 0.0), vec3(4001.0, 8000.0 - t, 0.0)],
        false,
        RefSide::Center,
        st,
        iw,
        Category::InteriorWall,
    )
    .unwrap();
    assert!(crate::abgleichen(&mut m, &[st]) > 0);
    let formen = crate::raeume(&m, st);
    for f in &formen {
        assert!(crate::ausbau::typen_anlegen(&mut m, f.raum.unwrap()));
    }
    assert!(m.commit().is_some());
    let w = projekt(&m);
    let mut ausgleich = 0;
    for r in w.raeume() {
        let f = formen.iter().find(|f| f.raum == r.raum).unwrap();
        let stempel = crate::flaechen::grundflaeche_fertig(&m, f);
        assert_eq!(r.grund, hundertstel(stempel), "{}", r.nummer);
        let summe: f64 = r.teile.iter().map(|t| t.wert).sum();
        assert!((summe - stempel).abs() < 1e-6, "{summe} {stempel}");
        ausgleich += r
            .teile
            .iter()
            .filter(|t| t.text.starts_with("Rundungsausgleich"))
            .count();
    }
    assert!(ausgleich > 0, "halbe Millimeter an der Innenwand");
}

/// Eine Stütze 80/80 um den Pfeiler 40/40 aus dem Wandring: Was beide
/// decken, zieht nur der Pfeiler ab, die Stütze den Rest; zusammen
/// 0,83 × 0,83 m mit Putz (Review #195 H-195j).
#[test]
fn stuetze_um_pfeiler_nur_einmal() {
    let h = haus();
    let pfeiler = ring(
        &[
            (4000.0, 4000.0),
            (4000.0, 4400.0),
            (4400.0, 4400.0),
            (4400.0, 4000.0),
        ],
        h.w[1],
    );
    let roh = rohbau(&h, rechteck(&h), vec![pfeiler], vec![]);
    let aus = RaumAusbau::fuer(&h.m, &roh).unwrap();
    let (a, b) = (3800.0, 4600.0);
    let stuetze = Einbau {
        id: h.w[2],
        treppe: false,
        stuecke: vec![vec![
            vec3(a, a, 0.0),
            vec3(b, a, 0.0),
            vec3(b, b, 0.0),
            vec3(a, b, 0.0),
        ]],
        z: (0.0, 3000.0),
        steigungen: None,
        loch: false,
    };
    let r = raum_mit(&h.m, &roh, &aus, &Einbauten::neu(vec![stuetze]));
    let abzug: f64 = r
        .teile
        .iter()
        .filter(|t| t.art == Art::Abzug)
        .map(|t| t.wert)
        .sum();
    assert!((abzug + 830.0 * 830.0).abs() < 1.0, "{abzug}");
    // 79,4609 − 0,6889
    assert_eq!(r.grund, 7877);
}

/// Eine Stütze 30/30 neben dem Pfeiler 40/40 aus dem Wandring, mit Putz
/// 0,33 × 0,33 m = 0,1089 m², überdeckt ihn um 0,13 × 0,33 m: Die Schwelle
/// 0,1 m² gilt für die ganze Stütze, abgezogen wird ihr Rest 0,066 m²
/// (VOB A-241a).
#[test]
fn stuetze_neben_pfeiler_schwelle_ganz() {
    let h = haus();
    let pfeiler = ring(
        &[
            (4000.0, 4000.0),
            (4000.0, 4400.0),
            (4400.0, 4400.0),
            (4400.0, 4000.0),
        ],
        h.w[1],
    );
    let roh = rohbau(&h, rechteck(&h), vec![pfeiler], vec![]);
    let aus = RaumAusbau::fuer(&h.m, &roh).unwrap();
    let (x, y) = ((4300.0, 4600.0), (4050.0, 4350.0));
    let stuetze = Einbau {
        id: h.w[2],
        treppe: false,
        stuecke: vec![vec![
            vec3(x.0, y.0, 0.0),
            vec3(x.1, y.0, 0.0),
            vec3(x.1, y.1, 0.0),
            vec3(x.0, y.1, 0.0),
        ]],
        z: (0.0, 3000.0),
        steigungen: None,
        loch: false,
    };
    let r = raum_mit(&h.m, &roh, &aus, &Einbauten::neu(vec![stuetze]));
    let abzug: Vec<f64> = r
        .teile
        .iter()
        .filter(|t| t.art == Art::Abzug)
        .map(|t| t.wert)
        .collect();
    assert_eq!(abzug.len(), 2, "{abzug:?}");
    let rest = 330.0 * 330.0 - 130.0 * 330.0;
    assert!((abzug[1] + rest).abs() <= 1e3, "{abzug:?}");
    assert!(!r
        .teile
        .iter()
        .any(|t| t.text.contains("bleibt in der Fläche")));
    // 79,4609 − 0,1849 − 0,0660
    assert_eq!(r.grund, 7921);
}

/// Wie oben, dazu eine zweite Stütze über Pfeiler und erster Stütze: Was
/// alle drei decken (0,13 × 0,18 m), geht einmal ab, nicht zweimal
/// (VOB H-241b).
#[test]
fn dreifache_ueberdeckung_einmal() {
    let h = haus();
    let pfeiler = ring(
        &[
            (4000.0, 4000.0),
            (4000.0, 4400.0),
            (4400.0, 4400.0),
            (4400.0, 4000.0),
        ],
        h.w[1],
    );
    let roh = rohbau(&h, rechteck(&h), vec![pfeiler], vec![]);
    let aus = RaumAusbau::fuer(&h.m, &roh).unwrap();
    let stuetze = |id: ElementId, x: (f64, f64), y: (f64, f64)| Einbau {
        id,
        treppe: false,
        stuecke: vec![vec![
            vec3(x.0, y.0, 0.0),
            vec3(x.1, y.0, 0.0),
            vec3(x.1, y.1, 0.0),
            vec3(x.0, y.1, 0.0),
        ]],
        z: (0.0, 3000.0),
        steigungen: None,
        loch: false,
    };
    let a = stuetze(h.w[2], (4300.0, 4600.0), (4050.0, 4350.0));
    let b = stuetze(h.w[3], (4300.0, 4600.0), (4200.0, 4500.0));
    let r = raum_mit(&h.m, &roh, &aus, &Einbauten::neu(vec![a, b]));
    let abzug: Vec<f64> = r
        .teile
        .iter()
        .filter(|t| t.art == Art::Abzug)
        .map(|t| t.wert)
        .collect();
    assert_eq!(abzug.len(), 3, "{abzug:?}");
    // Geputzt 330 × 330; neben Pfeiler und erster Stütze bleiben
    // 330² − (130 · 230 + 330 · 180 − 130 · 180)
    let rest = 330.0 * 330.0 - (130.0 * 230.0 + 330.0 * 180.0 - 130.0 * 180.0);
    assert!((abzug[2] + rest).abs() <= 1e3, "{abzug:?}");
    // 79,4609 − 0,1849 − 0,0660 − 0,0430
    assert_eq!(r.grund, 7917);
}

/// Die Treppe im EG (16 Steigungen, 4,20 × 1,00 m) steht im OG als
/// Treppenloch und geht dort ab; eine Treppe im OG genau darüber zieht
/// denselben Grundriss nicht noch einmal ab (Review #195 H-195j).
#[test]
fn treppenloch_im_geschoss_darueber() {
    let mut m = Model::with_seed(5);
    let b = m.add_building(2);
    let pts = [
        vec3(0.0, 0.0, 0.0),
        vec3(0.0, 8000.0, 0.0),
        vec3(10000.0, 8000.0, 0.0),
        vec3(10000.0, 0.0, 0.0),
    ];
    let eg = m.ground_of(Some(b)).unwrap();
    let og = m.level_above(eg).unwrap();
    let aw = m.type_by_guid(sk_model::EXTERIOR_TYPE_GUID).unwrap();
    m.begin("Haus");
    for st in [eg, og] {
        m.add_wall_run(&pts, true, RefSide::Left, st, aw, Category::ExteriorWall)
            .unwrap();
    }
    assert!(crate::abgleichen(&mut m, &[eg, og]) > 0);
    for st in [eg, og] {
        let r = crate::raeume(&m, st)[0].raum.unwrap();
        assert!(crate::ausbau::typen_anlegen(&mut m, r));
    }
    let treppe =
        sk_model::ExtDef::lesen(include_str!("../../sk-szb/beispiele/werk.treppe.szb")).unwrap();
    m.put_ext_def(treppe.clone()).unwrap();
    assert!(m.commit().is_some());
    let grund = |m: &Model, st: StoreyId| {
        let w = projekt(m);
        let r = w
            .raeume()
            .find(|r| crate::raeume(m, st).iter().any(|f| f.raum == r.raum))
            .unwrap();
        (r.grund, texte(r, Art::Abzug).join(" | "))
    };
    let (eg0, og0) = (grund(&m, eg).0, grund(&m, og).0);

    m.begin("Treppe EG");
    m.add_ext(eg, sk_model::ExtPart::new(&treppe, [1000.0, 3000.0]))
        .unwrap();
    assert!(m.commit().is_some());
    let (eg1, abzug_eg) = grund(&m, eg);
    let (og1, abzug_og) = grund(&m, og);
    assert!(!abzug_eg.contains("Treppenloch"), "{abzug_eg}");
    assert!(
        abzug_og.starts_with("Treppenloch ")
            && abzug_og.ends_with(
                "4,200 × 1,000 angenommen wie Treppe darunter (Deckenöffnung, § 3 Abs. 1)"
            ),
        "{abzug_og}"
    );
    assert_eq!((eg0 - eg1, og0 - og1), (420, 420));

    m.begin("Treppe OG");
    m.add_ext(og, sk_model::ExtPart::new(&treppe, [1000.0, 3000.0]))
        .unwrap();
    assert!(m.commit().is_some());
    let (og2, abzug_og) = grund(&m, og);
    assert_eq!(og2, og1, "{abzug_og}");
}
