//! Raumbuchmengen dreier Räume gegen Handrechnung (plan.md §11): Wohnzimmer
//! im EG mit Fenster, Fenstertür und Innentür; Bad im OG mit Fliesen,
//! Fenster und Innentür; Diele im EG, L-förmig, mit Pfeiler, Haustür und
//! drei Innentüren. Dazu die Ecköffnung und der Ausbau aus dem Wandtyp.
//! Alle Räume 2,635 m Rohbauhöhe, Putz 15 mm.

use super::*;
use crate::ausbau::{dicken, estrich_vorgabe, hoehen, laibungen, nutzung};
use crate::erkennung::{Begrenzung, Kante, RaumForm, Seite};
use crate::oeffnungen::{OeffnungAmRaum, OpeningKind, Rohbau};
use sk_math::vec3;
use sk_model::model::{
    BATH_SPACE_GUID, CEILING_TYPE_GUID, DEFAULT_SPACE_GUID, FINISH_TYPES, GYPSUM_PLASTER_GUID,
    HALL_SPACE_GUID, LIME_WALL_TYPE_GUID, LIVING_SPACE_GUID, OAK_SKIRTING_TYPE_GUID,
    PARQUET_TYPE_GUID, PORCELAIN_TYPE_GUID, SCREED_FLOOR_TYPE_GUID, SCREED_SLAB_TYPE_GUID,
    SPACE_TEMPLATES, TILE_SKIRTING_TYPE_GUID, WALL_TILES_TYPE_GUID, WET_FLOOR_TYPE_GUID,
    WOODCHIP_WALL_TYPE_GUID,
};
use sk_model::{Category, ElementId, Guid, Model, RefSide, SpaceType, StoreyId};

struct Haus {
    m: Model,
    eg: StoreyId,
    og: StoreyId,
    /// Bauteile, deren Nummern die Kanten und Öffnungen tragen.
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
    let og = m.run(m.runs_above(run)[0]).unwrap().storey;
    for t in &FINISH_TYPES {
        m.ensure_finish_type(t.guid).unwrap();
    }
    let w = m.run(run).unwrap().segments.clone();
    Haus { m, eg, og, w }
}

fn vorlage(g: Guid) -> SpaceType {
    SPACE_TEMPLATES
        .iter()
        .find(|t| t.guid == g)
        .unwrap()
        .space_type()
}

/// Ring von Kanten an Wänden, Punkte im Rohbaumaß.
fn ring(p: &[(f64, f64)], wand: ElementId) -> Vec<Kante> {
    (0..p.len())
        .map(|i| {
            let (a, b) = (p[i], p[(i + 1) % p.len()]);
            let a = vec3(a.0, a.1, 0.0);
            let b = vec3(b.0, b.1, 0.0);
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

fn form(
    geschoss: StoreyId,
    umriss: Vec<Kante>,
    loecher: Vec<Vec<Kante>>,
    o: Vec<OeffnungAmRaum>,
) -> Rohbau {
    Rohbau {
        form: RaumForm {
            geschoss,
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

#[allow(clippy::too_many_arguments)]
fn oe(
    id: ElementId,
    kante: usize,
    von: f64,
    bis: f64,
    uk: f64,
    ok: f64,
    tiefe: f64,
    art: OpeningKind,
    aufschlag_hier: Option<bool>,
) -> OeffnungAmRaum {
    OeffnungAmRaum {
        oeffnung: id,
        kante,
        von,
        bis,
        uk,
        ok,
        tiefe,
        art,
        aufschlag_hier,
    }
}

/// Menge der Zeile (Fläche, Typ, Schicht); prüft dabei, dass es genau eine
/// gibt.
fn menge(m: &Model, z: &[Zeile], f: Flaeche, typ: Option<(Guid, usize)>) -> f64 {
    let treffer: Vec<&Zeile> = z
        .iter()
        .filter(|x| {
            x.flaeche == f && x.typ.map(|(id, j)| (m.layer_set(id).unwrap().guid, j)) == typ
        })
        .collect();
    assert_eq!(treffer.len(), 1, "{f:?} {typ:?}: {treffer:#?}");
    treffer[0].menge()
}

fn gleich(ist: f64, soll: f64) {
    assert!((ist - soll).abs() < 1e-6, "ist {ist}, soll {soll}");
}

/// Wohnzimmer 5,00 × 4,00 m im EG: Fenster OE-1 1,51 × 1,51 (Brüstung
/// 0,875) unten, Innentür 0,885 × 2,135 rechts, schlägt hierher auf;
/// Fenstertür 2,01 × 2,385 oben, Nische 0,165 bis zum Element.
fn wohnzimmer(h: &Haus) -> Rohbau {
    let w = h.w[0];
    let k = ring(
        &[(0.0, 0.0), (5000.0, 0.0), (5000.0, 4000.0), (0.0, 4000.0)],
        w,
    );
    let o = vec![
        oe(
            h.w[1],
            0,
            1000.0,
            2510.0,
            875.0,
            2385.0,
            165.0,
            OpeningKind::Window,
            None,
        ),
        oe(
            h.w[2],
            1,
            500.0,
            1385.0,
            0.0,
            2135.0,
            115.0,
            OpeningKind::InnerDoor,
            Some(true),
        ),
        oe(
            h.w[3],
            2,
            1000.0,
            3010.0,
            0.0,
            2385.0,
            165.0,
            OpeningKind::Window,
            None,
        ),
    ];
    form(h.eg, k, Vec::new(), o)
}

#[test]
fn wohnzimmer_nach_handrechnung() {
    let h = haus();
    let f = wohnzimmer(&h);
    let a = RaumAusbau::nach_vorlage(&h.m, &f, &vorlage(LIVING_SPACE_GUID));
    let z = rechnen(&h.m, &f, &a, &Regeln::default());
    let m = |fl, typ| menge(&h.m, &z, fl, typ);

    // Boden: 4,970 × 3,970 = 19,7309 + vor Tür 0,885 × 0,015 + vor
    // Fenstertür 2,010 × 0,015 + Nische 2,010 × 0,165; die Türnische gehört
    // zum Nachbarraum
    let boden = 19_730_900.0 + 13_275.0 + 30_150.0 + 331_650.0;
    gleich(m(Flaeche::Boden, Some((PARQUET_TYPE_GUID, 0))), boden);
    for j in 0..4 {
        gleich(m(Flaeche::Estrich, Some((SCREED_SLAB_TYPE_GUID, j))), boden);
    }
    let estrich: Vec<f64> = z
        .iter()
        .filter(|x| x.flaeche == Flaeche::Estrich)
        .map(|x| x.dicke)
        .collect();
    assert_eq!(estrich, vec![65.0, 30.0, 40.0, 5.0]);
    for j in 0..2 {
        gleich(
            m(Flaeche::Decke, Some((CEILING_TYPE_GUID, j))),
            20_000_000.0,
        );
    }
    // Wände: Rohbaulänge × 2,635; Fenster 2,28 m² übermessen, Fenstertür
    // 4,79 m² abgezogen, Innentür 1,89 m² übermessen
    let wand = [
        13_175_000.0,
        10_540_000.0,
        13_175_000.0 - 4_793_850.0,
        10_540_000.0,
    ];
    for (k, soll) in wand.iter().enumerate() {
        for j in 0..3 {
            gleich(
                m(Flaeche::Wand(k), Some((WOODCHIP_WALL_TYPE_GUID, j))),
                *soll,
            );
        }
    }
    // Laibungen: Fenster 2 × 1,51 + 1,51; Fenstertür 2 × 2,385 + 2,01;
    // Innentür ohne (Umfassungszarge)
    for j in 0..3 {
        gleich(
            m(Flaeche::Laibung(0), Some((WOODCHIP_WALL_TYPE_GUID, j))),
            4530.0,
        );
        gleich(
            m(Flaeche::Laibung(2), Some((WOODCHIP_WALL_TYPE_GUID, j))),
            6780.0,
        );
    }
    assert!(!z.iter().any(|x| x.flaeche == Flaeche::Laibung(1)));
    // Sockel: Umfang an der Putzoberfläche 17,88 − Fenstertür 2,01
    // (über 1 m); Innentür übermessen
    gleich(
        m(Flaeche::Sockel, Some((OAK_SKIRTING_TYPE_GUID, 0))),
        15_870.0,
    );
    // Eckschienen nur an den Laibungskanten
    gleich(m(Flaeche::Eckschienen, None), 4530.0 + 6780.0);
    gleich(m(Flaeche::Randstreifen, None), 17_880.0);
    assert!(!z.iter().any(|x| matches!(
        x.flaeche,
        Flaeche::Silikon | Flaeche::Dichtband | Flaeche::Fliesen(_)
    )));
    // für den Grundriss
    assert_eq!(dicken(&h.m, &f, &a), vec![15.0; 4]);
    assert_eq!(laibungen(&h.m, &f, &a), vec![15.0, 0.0, 15.0]);
    assert_eq!(hoehen(&h.m, &f, &a), (160.0, 2625.0));
}

/// Bad 2,40 × 2,00 m im OG: Fenster 0,90 × 1,51 unten, Innentür oben,
/// schlägt in die Diele auf (Nische gehört dem Bad).
fn bad(h: &Haus) -> Rohbau {
    let k = ring(
        &[(0.0, 0.0), (2400.0, 0.0), (2400.0, 2000.0), (0.0, 2000.0)],
        h.w[0],
    );
    let o = vec![
        oe(
            h.w[1],
            0,
            700.0,
            1600.0,
            875.0,
            2385.0,
            165.0,
            OpeningKind::Window,
            None,
        ),
        oe(
            h.w[2],
            2,
            300.0,
            1185.0,
            0.0,
            2135.0,
            115.0,
            OpeningKind::InnerDoor,
            Some(false),
        ),
    ];
    form(h.og, k, Vec::new(), o)
}

#[test]
fn bad_nach_handrechnung() {
    let h = haus();
    assert_eq!(estrich_vorgabe(&h.m, h.og), SCREED_FLOOR_TYPE_GUID);
    let f = bad(&h);
    let a = RaumAusbau::nach_vorlage(&h.m, &f, &vorlage(BATH_SPACE_GUID));
    let z = rechnen(&h.m, &f, &a, &Regeln::default());
    let m = |fl, typ| menge(&h.m, &z, fl, typ);

    // Boden: 2,370 × 1,970 + vor Tür 0,885 × 0,015 + Türnische 0,885 × 0,115
    let boden = 4_668_900.0 + 13_275.0 + 101_775.0;
    for j in 0..2 {
        gleich(m(Flaeche::Boden, Some((WET_FLOOR_TYPE_GUID, j))), boden);
    }
    // Estrich auf Decke: Dämmung 45 → 50, damit OK FFB bei 160 bleibt
    let estrich: Vec<f64> = z
        .iter()
        .filter(|x| x.flaeche == Flaeche::Estrich)
        .map(|x| x.dicke)
        .collect();
    assert_eq!(estrich, vec![65.0, 30.0, 50.0]);
    gleich(
        m(Flaeche::Estrich, Some((SCREED_FLOOR_TYPE_GUID, 2))),
        boden,
    );
    gleich(m(Flaeche::Decke, Some((CEILING_TYPE_GUID, 1))), 4_800_000.0);
    // Wand unten (Fenster): Putz ganz, Anstrich über den Fliesen ab 1,36
    let anstrich = (LIME_WALL_TYPE_GUID, 0);
    let kzp = (LIME_WALL_TYPE_GUID, 1);
    gleich(m(Flaeche::Wand(0), Some(kzp)), 6_324_000.0);
    gleich(m(Flaeche::Wand(0), Some(anstrich)), 2400.0 * 1275.0);
    gleich(m(Flaeche::Laibung(0), Some(kzp)), 2.0 * 1510.0 + 900.0);
    gleich(m(Flaeche::Laibung(0), Some(anstrich)), 2.0 * 1025.0 + 900.0);
    // Fliesen: Länge an der Putzoberfläche × 1,20; Fenster im Fliesenfeld
    // übermessen; Laibung 2 × 0,485 und geflieste Fensterbank 0,90
    let wf = Some((WALL_TILES_TYPE_GUID, 0));
    gleich(m(Flaeche::Fliesen(0), wf), 2370.0 * 1200.0);
    gleich(m(Flaeche::FliesenLaibung(0), wf), 2.0 * 485.0 + 900.0);
    gleich(m(Flaeche::Fliesen(1), wf), 1970.0 * 1200.0);
    gleich(m(Flaeche::Fliesen(2), wf), 2370.0 * 1200.0);
    gleich(m(Flaeche::Wand(1), Some(kzp)), 2000.0 * 2635.0);
    gleich(m(Flaeche::Wand(1), Some(anstrich)), 2000.0 * 1275.0);
    gleich(m(Flaeche::Wand(2), Some(anstrich)), 2400.0 * 1275.0);
    assert!(!z.iter().any(|x| x.flaeche == Flaeche::Sockel));
    gleich(m(Flaeche::Eckschienen, None), 3920.0);
    gleich(m(Flaeche::Randstreifen, None), 8680.0);
    // Silikon: Boden/Wand 8,68 − Tür 0,885, vier Innenecken je 1,20
    gleich(m(Flaeche::Silikon, None), 8680.0 - 885.0 + 4.0 * 1200.0);
    gleich(m(Flaeche::Dichtband, None), 8680.0 - 885.0);
    assert_eq!(dicken(&h.m, &f, &a), vec![25.0; 4]);
    assert_eq!(hoehen(&h.m, &f, &a), (160.0, 2625.0));
}

/// Diele im EG, L-förmig mit Pfeiler: Haustür unten, Innentüren rechts
/// (Aufschlag hier), oben (Aufschlag drüben) und links (ohne).
fn diele(h: &Haus) -> Rohbau {
    let k = ring(
        &[
            (0.0, 0.0),
            (4000.0, 0.0),
            (4000.0, 1500.0),
            (1500.0, 1500.0),
            (1500.0, 4000.0),
            (0.0, 4000.0),
        ],
        h.w[0],
    );
    let pfeiler = ring(
        &[
            (600.0, 600.0),
            (600.0, 900.0),
            (900.0, 900.0),
            (900.0, 600.0),
        ],
        h.w[1],
    );
    let o = vec![
        oe(
            h.w[2],
            0,
            1000.0,
            2250.0,
            0.0,
            2385.0,
            165.0,
            OpeningKind::Door,
            None,
        ),
        oe(
            h.w[3],
            1,
            300.0,
            1185.0,
            0.0,
            2135.0,
            115.0,
            OpeningKind::InnerDoor,
            Some(true),
        ),
        oe(
            h.w[3],
            2,
            800.0,
            1685.0,
            0.0,
            2135.0,
            175.0,
            OpeningKind::InnerDoor,
            Some(false),
        ),
        oe(
            h.w[3],
            3,
            1000.0,
            1885.0,
            0.0,
            2135.0,
            115.0,
            OpeningKind::InnerDoor,
            None,
        ),
    ];
    form(h.eg, k, vec![pfeiler], o)
}

#[test]
fn diele_mit_pfeiler_nach_handrechnung() {
    let h = haus();
    assert_eq!(estrich_vorgabe(&h.m, h.eg), SCREED_SLAB_TYPE_GUID);
    let f = diele(&h);
    let a = RaumAusbau::nach_vorlage(&h.m, &f, &vorlage(HALL_SPACE_GUID));
    let z = rechnen(&h.m, &f, &a, &Regeln::default());
    let m = |fl, typ| menge(&h.m, &z, fl, typ);

    // Boden: L bis Putz 3,970 × 1,470 + 1,470 × 2,500; Pfeiler 0,109 m²
    // übermessen; Haustür: Streifen und Nische; Türen: Streifen, Nischen
    // ganz (Aufschlag drüben), keine (Aufschlag hier), hälftig (ohne)
    let boden = 5_835_900.0
        + 3_675_000.0
        + 1250.0 * 15.0
        + 1250.0 * 165.0
        + 3.0 * 885.0 * 15.0
        + 885.0 * 175.0
        + 885.0 * 115.0 * 0.5;
    gleich(m(Flaeche::Boden, Some((PORCELAIN_TYPE_GUID, 0))), boden);
    let estrich: Vec<f64> = z
        .iter()
        .filter(|x| x.flaeche == Flaeche::Estrich)
        .map(|x| x.dicke)
        .collect();
    assert_eq!(estrich, vec![65.0, 30.0, 45.0, 5.0]);
    // Decke im Rohbaumaß, Pfeiler 0,09 m² übermessen
    gleich(m(Flaeche::Decke, Some((CEILING_TYPE_GUID, 0))), 9_750_000.0);
    // Haustür 1,25 × 2,385 = 2,98 m² abgezogen
    let rf = |j| Some((WOODCHIP_WALL_TYPE_GUID, j));
    gleich(
        m(Flaeche::Wand(0), rf(2)),
        4000.0 * 2635.0 - 1250.0 * 2385.0,
    );
    gleich(m(Flaeche::Laibung(0), rf(2)), 2.0 * 2385.0 + 1250.0);
    gleich(m(Flaeche::Wand(2), rf(0)), 2500.0 * 2635.0);
    for k in 6..10 {
        gleich(m(Flaeche::Wand(k), rf(2)), 300.0 * 2635.0);
    }
    // Fliesensockel: 15,88 + Pfeiler 4 × 0,33 − Haustür 1,25
    gleich(
        m(Flaeche::Sockel, Some((TILE_SKIRTING_TYPE_GUID, 0))),
        15_880.0 + 1320.0 - 1250.0,
    );
    // Eckschienen: Ecke des L und vier Pfeilerecken, Laibungskanten Haustür
    gleich(m(Flaeche::Eckschienen, None), 5.0 * 2635.0 + 6020.0);
    gleich(m(Flaeche::Randstreifen, None), 15_880.0 + 1320.0);
}

/// Raum 4,00 × 3,00 m mit Fenstertür über die Ecke unten rechts, je
/// 1,00 m auf beiden Wänden.
fn ecke(h: &Haus) -> Rohbau {
    let k = ring(
        &[(0.0, 0.0), (4000.0, 0.0), (4000.0, 3000.0), (0.0, 3000.0)],
        h.w[0],
    );
    let o = vec![
        oe(
            h.w[1],
            0,
            3000.0,
            4000.0,
            0.0,
            2385.0,
            165.0,
            OpeningKind::Window,
            None,
        ),
        oe(
            h.w[1],
            1,
            0.0,
            1000.0,
            0.0,
            2385.0,
            165.0,
            OpeningKind::Window,
            None,
        ),
    ];
    form(h.eg, k, Vec::new(), o)
}

#[test]
fn ecktuer() {
    let h = haus();
    let f = ecke(&h);
    let a = RaumAusbau::nach_vorlage(&h.m, &f, &vorlage(LIVING_SPACE_GUID));
    let z = rechnen(&h.m, &f, &a, &Regeln::default());
    let m = |fl, typ| menge(&h.m, &z, fl, typ);
    // Nischen beider Wände, das Eckfeld dazwischen, Streifen vor der
    // Öffnung im Eck nur einmal
    let boden =
        3970.0 * 2970.0 + 2.0 * (1000.0 * 15.0 + 1000.0 * 165.0) + 165.0 * 165.0 - 15.0 * 15.0;
    gleich(m(Flaeche::Boden, Some((PARQUET_TYPE_GUID, 0))), boden);
    // je eine seitliche Laibung, die Ecke ist offen
    let rf = Some((WOODCHIP_WALL_TYPE_GUID, 2));
    gleich(m(Flaeche::Laibung(0), rf), 2385.0 + 1000.0);
    gleich(m(Flaeche::Laibung(1), rf), 2385.0 + 1000.0);
    gleich(m(Flaeche::Eckschienen, None), 2.0 * (2385.0 + 1000.0));
    // Sockel: je 1,00 m übermessen
    gleich(
        m(Flaeche::Sockel, Some((OAK_SKIRTING_TYPE_GUID, 0))),
        2.0 * (3970.0 + 2970.0),
    );
}

#[test]
fn ausbau_aus_dem_wandtyp() {
    // Die linke Wand hat Putz im Typ: das Raumbuch setzt dort nur Raufaser
    // und Anstrich dazu (der Putz zählt beim Wandtyp, nicht doppelt) und
    // misst den Boden an dessen Oberfläche
    let mut h = haus();
    let gips = h.m.ensure_finish_material(GYPSUM_PLASTER_GUID).unwrap();
    let mut f = wohnzimmer(&h);
    f.form.umriss[3].ausbau_typ = vec![(20.0, gips)];
    let a = RaumAusbau::nach_vorlage(&h.m, &f, &vorlage(LIVING_SPACE_GUID));
    assert!(a.seiten[3].wand.is_some());
    assert!(a.seiten[3].sockel.is_some());
    let z = rechnen(&h.m, &f, &a, &Regeln::default());
    let w3: Vec<(usize, f64)> = z
        .iter()
        .filter(|x| x.flaeche == Flaeche::Wand(3))
        .map(|x| (x.typ.unwrap().1, x.menge()))
        .collect();
    assert_eq!(w3, vec![(0, 10_540_000.0), (1, 10_540_000.0)]);
    let k = crate::raumbuch::koerper::koerper(&h.m, &f, &a);
    assert!(!k.iter().any(|x| x.flaeche == Flaeche::Wand(3)));
    // bringt der Typ auch die Beschichtung mit, setzt das Raumbuch nichts dazu
    let anstrich =
        h.m.ensure_finish_material(sk_model::model::PAINT_GUID)
            .unwrap();
    f.form.umriss[3].ausbau_typ.push((0.0, anstrich));
    let b = RaumAusbau::nach_vorlage(&h.m, &f, &vorlage(LIVING_SPACE_GUID));
    assert_eq!(b.seiten[3].wand, None);
    assert_eq!(dicken(&h.m, &f, &a), vec![15.0, 15.0, 15.0, 20.0]);
    let boden = 4965.0 * 3970.0 + 13_275.0 + 30_150.0 + 331_650.0;
    gleich(
        menge(&h.m, &z, Flaeche::Boden, Some((PARQUET_TYPE_GUID, 0))),
        boden,
    );
}

#[test]
fn terme_ergeben_die_menge() {
    let h = haus();
    let f = wohnzimmer(&h);
    let a = RaumAusbau::nach_vorlage(&h.m, &f, &vorlage(LIVING_SPACE_GUID));
    for z in rechnen(&h.m, &f, &a, &Regeln::default()) {
        assert!(z.terme.iter().any(|t| t.art == Art::Basis), "{z:?}");
        for t in &z.terme {
            match t.art {
                Art::Hinweis => assert_eq!(t.wert, 0.0),
                Art::Abzug => assert!(t.wert < 0.0),
                Art::Basis | Art::Zuschlag => assert!(t.wert > 0.0),
            }
        }
    }
    // Übermessen steht im Rechenweg
    let z = rechnen(&h.m, &f, &a, &Regeln::default());
    let w0 = z.iter().find(|x| x.flaeche == Flaeche::Wand(0)).unwrap();
    assert!(w0
        .terme
        .iter()
        .any(|t| t.art == Art::Hinweis && t.text.contains("übermessen")));
    // strengere Grenze: das Fenster wird abgezogen
    let streng = Regeln {
        wand: 1.0e6,
        ..Regeln::default()
    };
    let z = rechnen(&h.m, &f, &a, &streng);
    let w0 = z.iter().find(|x| x.flaeche == Flaeche::Wand(0)).unwrap();
    gleich(w0.menge(), 13_175_000.0 - 1510.0 * 1510.0);
}

#[test]
fn leer_und_seite_ohne_ausbau() {
    let h = haus();
    // ohne Umriss: keine Zeile mit Menge, kein Absturz
    let leer = form(h.eg, Vec::new(), Vec::new(), Vec::new());
    let a = RaumAusbau::nach_vorlage(&h.m, &leer, &vorlage(LIVING_SPACE_GUID));
    for z in rechnen(&h.m, &leer, &a, &Regeln::default()) {
        assert_eq!(z.menge(), 0.0, "{z:?}");
    }
    // die obere Wandseite ohne Ausbau (später Abweichung je Wandseite, RB6)
    let mut f = wohnzimmer(&h);
    f.oeffnungen.retain(|o| o.kante != 2);
    let mut a = RaumAusbau::nach_vorlage(&h.m, &f, &vorlage(LIVING_SPACE_GUID));
    a.seiten[2] = crate::ausbau::SeitenAusbau::default();
    let z = rechnen(&h.m, &f, &a, &Regeln::default());
    assert!(!z.iter().any(|x| x.flaeche == Flaeche::Wand(2)));
    // Boden bis zur Seite ohne Putz, Sockel ohne diese Seite
    let boden = 4970.0 * 3985.0 + 13_275.0;
    gleich(
        menge(&h.m, &z, Flaeche::Boden, Some((PARQUET_TYPE_GUID, 0))),
        boden,
    );
    gleich(
        menge(&h.m, &z, Flaeche::Sockel, Some((OAK_SKIRTING_TYPE_GUID, 0))),
        4970.0 + 3985.0 + 3985.0,
    );
    assert_eq!(dicken(&h.m, &f, &a), vec![15.0, 15.0, 0.0, 15.0]);
}

/// Ausbau nach der Nutzung des gespeicherten Raums: ohne Wahl Wohnzimmer,
/// dessen Typen ein frisches Projekt erst mit `typen_anlegen` führt; gewählt das
/// Bad samt Typen; eine geänderte Projektkopie des Wohnzimmers gilt für Räume ohne
/// Wahl.
#[test]
fn ausbau_nach_der_nutzung_des_raums() {
    let mut m = Model::with_seed(5);
    let b = m.add_building(2);
    let pts = [
        vec3(0.0, 0.0, 0.0),
        vec3(0.0, 8000.0, 0.0),
        vec3(10000.0, 8000.0, 0.0),
        vec3(10000.0, 0.0, 0.0),
    ];
    let run = m.build_from_polygon(b, &pts).unwrap();
    let st = m.run(run).unwrap().storey;
    m.begin("Räume");
    assert!(crate::abgleichen(&mut m, &[st]) > 0);
    assert!(m.commit().is_some());
    let roh = |m: &Model| Rohbau::neu(m, crate::raeume(m, st).remove(0));
    let r = roh(&m);
    let id = r.form.raum.unwrap();
    assert_eq!(nutzung(&m, &r.form), DEFAULT_SPACE_GUID);
    let a = RaumAusbau::fuer(&m, &r).unwrap();
    assert_eq!((a.boden, a.estrich, a.decke), (None, None, None));
    assert_eq!(a.seiten.len(), 4);
    // Boden, Wand, Decke, Sockel und der Estrich des Geschosses
    assert_eq!(crate::ausbau::fehlende_typen(&m, &r.form).len(), 5);
    // Anlegen bringt Vorlage, Ausbautypen und den Estrich des Geschosses
    m.begin("Ausbau");
    assert!(crate::ausbau::typen_anlegen(&mut m, id));
    assert!(m.commit().is_some());
    let r = roh(&m);
    assert!(crate::ausbau::fehlende_typen(&m, &r.form).is_empty());
    assert!(m.space_type(DEFAULT_SPACE_GUID).is_some());
    let a = RaumAusbau::fuer(&m, &r).unwrap();
    assert_eq!(hoehen(&m, &r, &a).0, 160.0, "Fußbodenaufbau");
    assert_eq!(m.space(id).unwrap().nutzung, None, "Wahl bleibt offen");
    // Bad gewählt: Vorlage und Typen kommen ins Projekt
    m.begin("Bad");
    assert!(m.ensure_space_type(BATH_SPACE_GUID).is_some());
    assert!(m.set_space(id, |s| s.nutzung = Some(BATH_SPACE_GUID)));
    assert!(m.commit().is_some());
    let r = roh(&m);
    let a = RaumAusbau::fuer(&m, &r).unwrap();
    let bad = vorlage(BATH_SPACE_GUID);
    assert_eq!(a.boden, m.type_by_guid(bad.floor.unwrap()));
    assert_eq!(
        a.estrich,
        m.type_by_guid(SCREED_SLAB_TYPE_GUID),
        "EG auf der Sohlplatte"
    );
    assert!(a
        .seiten
        .iter()
        .all(|s| s.fliesen.is_some() && s.sockel.is_none()));
    // Ohne Wahl gilt die Projektkopie des Wohnzimmers
    m.begin("Wohnzimmer");
    assert!(m.set_space(id, |s| s.nutzung = None));
    assert!(m.ensure_space_type(DEFAULT_SPACE_GUID).is_some());
    let mut w = m.space_type(DEFAULT_SPACE_GUID).unwrap().clone();
    w.floor = m.space_type(BATH_SPACE_GUID).unwrap().floor;
    assert!(m.put_space_type(w));
    assert!(m.commit().is_some());
    let a = RaumAusbau::fuer(&m, &roh(&m)).unwrap();
    assert_eq!(a.boden, m.type_by_guid(bad.floor.unwrap()));
    assert!(a.decke.is_some() && a.seiten.iter().all(|s| s.wand.is_some()));
}

#[path = "koerper_tests.rs"]
mod koerper;
