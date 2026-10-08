//! Referenzhäuser RH-1 bis RH-3 auf den Cent (Abnahme KA-0 „Rechnung“,
//! Sollwerte bim/integration/sollwerte-referenzhaeuser.md, Prüfstand der
//! BIM-Integration). Die Häuser liegen unverändert in `referenz/`.

use sk_cost::{lesen, Cent, Dez, Kostenblatt, Kostenspeicher, Umfang};
use sk_model::{qto, szo, GuidGen, Model};

fn laden(text: &str) -> Model {
    szo::read_with(text, GuidGen::with_seed(1), &lesen::ABSCHNITTE_SZO)
        .expect("lädt")
        .model
}

/// Kurztext, Menge (3 Stellen in Tausendsteln), EP, GP, Stoff-GP in Cent.
type Zeile = (&'static str, i64, i64, i64, i64);

struct Soll {
    datei: &'static str,
    netto: i64,
    material: i64,
    /// Geschoss, Netto in Cent
    geschosse: [(&'static str, i64); 3],
    ausgleich: i64,
    zeilen: &'static [Zeile],
    /// Bauteil, Menge in Tausendsteln
    ohne: &'static [(&'static str, i64)],
}

const RH1: Soll = Soll {
    datei: include_str!("../referenz/rh1-standardhaus.szo"),
    netto: 6008983,
    material: 3114834,
    geschosse: [
        ("Gründung", 948000),
        ("Erdgeschoss", 2709564),
        ("Obergeschoss", 2351416),
    ],
    ausgleich: 3,
    zeilen: &[
        (
            "Bodenplatte Stb C25/30 XC2 d=18-25cm",
            17600,
            22600,
            397760,
            334400,
        ),
        (
            "Frostschürze Stb C25/30 b=30-45cm erdgeschalt",
            7024,
            25000,
            175600,
            133456,
        ),
        (
            "Stb-Decke Ortbeton C25/30 XC1 d=18-25cm",
            29809,
            22600,
            673683,
            566371,
        ),
        (
            "Deckenschalung glatt, kein Sichtbeton, Stützhöhe bis 3,0m",
            124059,
            3800,
            471424,
            99247,
        ),
        (
            "Randschalung Decke/Bodenplatte h bis 25cm",
            102760,
            2900,
            298004,
            51380,
        ),
        (
            "Betonstahl B500 liefern, schneiden, biegen, verlegen",
            4670,
            160000,
            747200,
            467000,
        ),
        (
            "AW Porenbeton-Planstein PP2-0,35 d=17,5cm Dünnbettmörtel",
            172224,
            5400,
            930010,
            465005,
        ),
        (
            "WDVS EPS 035 d=140mm, Kleber, Dübel, Armierung, Oberputz",
            199595,
            11600,
            2315302,
            997975,
        ),
    ],
    ohne: &[("DT-001", 13219), ("DT-001", 13219), ("AB-001", 13000)],
};

const RH2: Soll = Soll {
    datei: include_str!("../referenz/rh2-mehrschalig.szo"),
    netto: 6997263,
    material: 3662105,
    geschosse: [
        ("Gründung", 948000),
        ("Erdgeschoss", 3034024),
        ("Obergeschoss", 3015257),
    ],
    ausgleich: -18,
    zeilen: &[
        (
            "Bodenplatte Stb C25/30 XC2 d=18-25cm",
            17600,
            22600,
            397760,
            334400,
        ),
        (
            "Frostschürze Stb C25/30 b=30-45cm erdgeschalt",
            7024,
            25000,
            175600,
            133456,
        ),
        (
            "Stb-Decke Ortbeton C25/30 XC1 d=18-25cm",
            30385,
            22600,
            686701,
            577315,
        ),
        (
            "Deckenschalung glatt, kein Sichtbeton, Stützhöhe bis 3,0m",
            124605,
            3800,
            473499,
            99684,
        ),
        (
            "Randschalung Decke/Bodenplatte h bis 25cm",
            102960,
            2900,
            298584,
            51480,
        ),
        (
            "Betonstahl B500 liefern, schneiden, biegen, verlegen",
            4727,
            160000,
            756320,
            472700,
        ),
        (
            "AW Porenbeton-Planstein PP2-0,35 d=17,5cm Dünnbettmörtel",
            172751,
            5400,
            932855,
            466428,
        ),
        (
            "IW Porenbeton-Planstein PP2-0,35 d=17,5cm Dünnbettmörtel",
            18498,
            5400,
            99889,
            49945,
        ),
        (
            "IW Porenbeton-Planbauplatte d=11,5cm Dünnbettmörtel",
            18498,
            4299,
            79523,
            35128,
        ),
        (
            "Kerndämmung MW-Platte WLS 035 d=140mm 2-schal. Mauerwerk",
            194368,
            2500,
            485920,
            310989,
        ),
        (
            "Verblendschale Klinker NF d=11,5cm Läuferverband verfugt",
            205560,
            12700,
            2610612,
            1130580,
        ),
    ],
    ohne: &[],
};

const RH3: Soll = Soll {
    datei: include_str!("../referenz/rh3-versatz-dachterrasse.szo"),
    netto: 6694198,
    material: 3512120,
    geschosse: [
        ("Gründung", 948000),
        ("Erdgeschoss", 2937124),
        ("Obergeschoss", 2809049),
    ],
    ausgleich: 25,
    zeilen: &[
        (
            "Bodenplatte Stb C25/30 XC2 d=18-25cm",
            17600,
            22600,
            397760,
            334400,
        ),
        (
            "Frostschürze Stb C25/30 b=30-45cm erdgeschalt",
            7024,
            25000,
            175600,
            133456,
        ),
        (
            "Stb-Decke Ortbeton C25/30 XC1 d=18-25cm",
            33969,
            22600,
            767699,
            645411,
        ),
        (
            "Deckenschalung glatt, kein Sichtbeton, Stützhöhe bis 3,0m",
            136298,
            3800,
            517932,
            109038,
        ),
        (
            "Randschalung Decke/Bodenplatte h bis 25cm",
            106680,
            2900,
            309372,
            53340,
        ),
        (
            "Betonstahl B500 liefern, schneiden, biegen, verlegen",
            5086,
            160000,
            813760,
            508600,
        ),
        (
            "AW Porenbeton-Planstein PP2-0,35 d=24cm Dünnbettmörtel",
            179602,
            6600,
            1185373,
            646567,
        ),
        (
            "IW Porenbeton-Planstein PP2-0,35 d=24cm Dünnbettmörtel",
            19183,
            6600,
            126608,
            69059,
        ),
        (
            "WDVS EPS 035 d=120mm, Kleber, Dübel, Armierung, Oberputz",
            207884,
            11420,
            2374035,
            1002001,
        ),
        (
            "Untersichtdämmung Decke EPS d=120mm geklebt, verputzt",
            2928,
            8900,
            26059,
            10248,
        ),
    ],
    ohne: &[("DT-001", 1757), ("DT-001", 1757), ("AB-001", 10600)],
};

fn blatt(m: &Model, u: &Umfang) -> Kostenblatt {
    let sched = qto::schedule(m);
    let k = lesen::katalog(m, None);
    lesen::kosten(m, &sched, &k, u)
}

fn pruefen(s: &Soll) {
    let m = laden(s.datei);
    // Stammdaten und Abdeckung ohne Befund (Regeln 71–92, 97, 99)
    let k = lesen::katalog(&m, None);
    assert!(
        lesen::befunde(&m, &k).is_empty(),
        "{:#?}",
        lesen::befunde(&m, &k)
    );
    let b = blatt(&m, &Umfang::projekt());
    let ist: Vec<(String, i64, i64, i64, i64)> = b
        .positionen
        .iter()
        .map(|p| {
            (
                p.kurz.clone(),
                p.menge.0 / 1000,
                p.ep.0,
                p.gp.0,
                p.stoff_gp.0,
            )
        })
        .collect();
    for z in s.zeilen {
        assert!(
            ist.iter().any(|i| (i.0.as_str(), i.1, i.2, i.3, i.4) == *z),
            "fehlt {z:?}\nist {ist:#?}"
        );
    }
    assert_eq!(ist.len(), s.zeilen.len(), "{ist:#?}");
    assert_eq!(b.netto, Cent(s.netto));
    assert_eq!(b.nur_material, Cent(s.material));
    assert_eq!(b.mwst, Cent((s.netto * 19 + 50) / 100));
    assert_eq!(b.brutto, b.netto + b.mwst);
    // Anteile je für sich auf den Cent: höchstens ein Cent je Position und
    // Anteil Abstand zum Netto
    let anteile = b.lohn + b.stoff + b.geraet + b.sonst + b.nu;
    assert!((anteile - b.netto).0.abs() <= 4 * b.positionen.len() as i64);
    assert_eq!((b.unvollstaendig, b.geschaetzt), (0, 0));
    // je Geschoss mit einem Rundungsausgleich (Regel 96, ka-0-fach §1.7)
    let geschosse: Vec<(String, i64)> = b
        .nach_geschoss
        .iter()
        .map(|(id, c)| (m.storey(*id).unwrap().name.clone(), c.0))
        .collect();
    let soll: Vec<(String, i64)> = s
        .geschosse
        .iter()
        .map(|(n, c)| (n.to_string(), *c))
        .collect();
    assert_eq!(geschosse, soll);
    assert_eq!(b.ausgleich_geschoss, Cent(s.ausgleich));
    assert!(b.befunde.iter().all(|f| f.regel != 96), "{:#?}", b.befunde);
    // Kostengruppen und Gewerke gehen ohne Rest auf
    let kg: Cent = b.nach_kg.iter().map(|x| x.1).sum();
    assert_eq!(kg + b.ausgleich_kg, b.netto);
    let gw: Cent = b.nach_gewerk.iter().map(|x| x.1).sum();
    assert_eq!(gw, b.netto);
    // ohne Bauleistung, grau mit Menge
    let ohne: Vec<(&str, i64)> = b
        .ohne
        .iter()
        .map(|o| (o.nummer.as_str(), o.menge.0 / 1000))
        .collect();
    assert_eq!(ohne, s.ohne);
    // Umfang Gebäude: ein Haus, keine losen Geschosse, gleiches Blatt
    let (bid, _) = m.buildings().iter().next().expect("ein Gebäude");
    assert_eq!(blatt(&m, &Umfang::gebaeude(bid)), b);
    // mit Zwischenspeicher dasselbe Blatt
    let sched = qto::schedule(&m);
    let (b1, sp) = lesen::kosten_mit(
        Kostenspeicher::default(),
        &m,
        &sched,
        &k,
        &Umfang::projekt(),
    );
    assert_eq!(b1, b);
    let (b2, sp) = lesen::kosten_mit(sp, &m, &sched, &k, &Umfang::projekt());
    assert_eq!(b2, b);
    assert_eq!(sp.neu_zugeordnet(), 0);
}

#[test]
fn rh1_standardhaus() {
    pruefen(&RH1);
}

#[test]
fn rh2_mehrschalig() {
    pruefen(&RH2);
}

#[test]
fn rh3_versatz_dachterrasse() {
    pruefen(&RH3);
}

/// Die Sollbilder zeigen RH-1 ohne Deckenschalung und Randschalung:
/// 52.395,55 € / 29.642,07 €.
#[test]
fn rh1_ohne_schalung() {
    let m = laden(RH1.datei);
    let b = blatt(&m, &Umfang::projekt());
    let ohne = |f: &dyn Fn(&sk_cost::Position) -> Cent| -> Cent {
        b.positionen
            .iter()
            .filter(|p| {
                !p.kurz.starts_with("Deckenschalung") && !p.kurz.starts_with("Randschalung")
            })
            .map(f)
            .sum()
    };
    assert_eq!(ohne(&|p| p.gp), Cent(5_239_555));
    assert_eq!(ohne(&|p| p.stoff_gp), Cent(2_964_207));
    let _ = Dez::NULL;
}

/// Abnahme 28: `lesen::kosten` je Haus unter 1 ms (nur mit Optimierung
/// gemessen), Median aus fünf Läufen.
#[test]
fn zeit_unter_einer_millisekunde() {
    for s in [&RH1, &RH2, &RH3] {
        let m = laden(s.datei);
        let sched = qto::schedule(&m);
        let k = lesen::katalog(&m, None);
        let mut t: Vec<f64> = (0..5)
            .map(|_| {
                let a = std::time::Instant::now();
                std::hint::black_box(lesen::kosten(&m, &sched, &k, &Umfang::projekt()));
                a.elapsed().as_secs_f64() * 1000.0
            })
            .collect();
        t.sort_by(f64::total_cmp);
        eprintln!("kosten {:.3} ms", t[2]);
        if !cfg!(debug_assertions) {
            assert!(t[2] < 1.0, "{:.3} ms", t[2]);
        }
    }
}

/// Abnahme 29 (Teil): Wand verschieben, Typ tauschen, Preis im Projekt.
/// Nach jeder Änderung ist `kosten_mit` gleich `kosten`; nach einer
/// verschobenen Wand ordnet der Speicher nichts neu zu, nach einem Preis im
/// Projekt alles.
#[test]
fn kostenspeicher_gleich_kosten() {
    for s in [&RH1, &RH2] {
        let mut m = laden(s.datei);
        let u = Umfang::projekt();
        let sched = qto::schedule(&m);
        let k = lesen::katalog(&m, None);
        let (_, sp) = lesen::kosten_mit(Kostenspeicher::default(), &m, &sched, &k, &u);
        let typen = sp.neu_zugeordnet();
        assert!(typen > 0);
        // Wand verschieben: die erste Außenwand, die sich um 250 mm
        // versetzen lässt. Der erste Versatz kann ein neues Bauteil ergeben
        // (Untersicht unter dem Überstand); danach ordnet ein weiterer
        // Versatz nichts mehr neu zu
        let waende: Vec<_> = m
            .elements()
            .iter()
            .filter(|(_, e)| e.category == sk_model::element::Category::ExteriorWall)
            .map(|(id, _)| id)
            .collect();
        let mut sp = sp;
        let mut verschoben = 0;
        for w in waende {
            for d in [250.0, -100.0] {
                m.begin("Wand");
                if m.move_segment(w, d).is_none() {
                    m.rollback();
                    continue;
                }
                m.commit();
                let sched = qto::schedule(&m);
                let soll = lesen::kosten(&m, &sched, &k, &u);
                let (ist, neu) = lesen::kosten_mit(sp, &m, &sched, &k, &u);
                assert_eq!(ist, soll);
                if verschoben > 0 {
                    assert_eq!(neu.neu_zugeordnet(), 0, "Wand verschoben: kein Typ neu");
                }
                verschoben += 1;
                sp = neu;
            }
            if verschoben >= 2 {
                break;
            }
        }
        assert!(verschoben >= 2, "keine Wand verschiebbar");
        let sched = qto::schedule(&m);
        // Preis im Projekt: alles neu
        m.begin("Preis");
        sk_cost::ausfuehren(
            &mut m,
            None,
            sk_cost::Rolle::Admin,
            &sk_cost::Herkunft::neu(sk_cost::HerkunftArt::Manual, "2026-10-08", "10:00"),
            sk_cost::Op::FirmenwertSetzen {
                schluessel: "wage".into(),
                wert: Dez::ganz(65),
            },
        )
        .unwrap();
        m.commit();
        let k = lesen::katalog(&m, None);
        let soll = lesen::kosten(&m, &sched, &k, &u);
        let (ist, sp) = lesen::kosten_mit(sp, &m, &sched, &k, &u);
        assert_eq!(ist, soll);
        assert_ne!(ist.netto, Cent(s.netto));
        assert!(sp.neu_zugeordnet() >= typen, "Preis im Projekt: alle neu");
        let (_, leer) = lesen::kosten_mit(Kostenspeicher::default(), &m, &sched, &k, &u);
        assert_eq!(sp.neu_zugeordnet(), leer.neu_zugeordnet());
    }
}
