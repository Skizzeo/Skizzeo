//! Fälle der Rechnung am Standardhaus RH-1 und RH-2 (Abnahme KA-0, Fälle
//! 20–22, 26, 27; ka-0-fach §3). Abgewandelte Stammdaten entstehen aus dem
//! Werksbestand als Text, gelesen mit demselben Leser.

use sk_cost::katalog::{self, Katalog, Quelle, Umfeld};
use sk_cost::{lesen, Cent, Kostenblatt, Umfang};
use sk_model::{qto, szo, GuidGen, Model};

fn laden(text: &str) -> Model {
    szo::read_with(text, GuidGen::with_seed(1), &lesen::ABSCHNITTE_SZO)
        .expect("lädt")
        .model
}

fn rh1() -> Model {
    laden(include_str!("../referenz/rh1-standardhaus.szo"))
}

fn rh2() -> Model {
    laden(include_str!("../referenz/rh2-mehrschalig.szo"))
}

/// Werksbestand mit Änderungen: (alt, neu) als Textersatz, jeder genau
/// einmal; leeres `alt` hängt `neu` als Zeile an.
fn werk_mit(m: &Model, ersatz: &[(&str, &str)]) -> Katalog {
    let mut text = sk_cost::WERK.to_string();
    for (alt, neu) in ersatz {
        if alt.is_empty() {
            text = format!("{text}{neu}\n");
            continue;
        }
        assert_eq!(text.matches(alt).count(), 1, "{alt}");
        text = text.replacen(alt, neu, 1);
    }
    let zeilen: Vec<(String, String)> = text
        .lines()
        .filter_map(|l| {
            let a = l.strip_prefix('[')?.split(']').next()?;
            Some((a.to_string(), l.to_string()))
        })
        .collect();
    katalog::lesen(
        zeilen.iter().map(|(a, l)| (a.as_str(), l.as_str())),
        &Umfeld::aus_modell(m),
        Quelle::Werk {
            stand: "10/2026".into(),
        },
    )
}

fn blatt(m: &Model, k: &Katalog) -> Kostenblatt {
    lesen::kosten(m, &qto::schedule(m), k, &Umfang::projekt())
}

fn position<'a>(b: &'a Kostenblatt, kurz: &str) -> &'a sk_cost::Position {
    b.positionen
        .iter()
        .find(|p| p.kurz.starts_with(kurz))
        .unwrap_or_else(|| panic!("{kurz}"))
}

const W20: &str = "short=\"WDVS EPS 035 d=140mm, Kleber, Dübel, Armierung, Oberputz\" trade=1S7Wf_00100800000004Uf title=1S7bUW0010080300000007 pos=20 unit=m2 basis=area hours=1.1";

/// Fall 7 (Abnahme 20): W20 mit Nachunternehmerpreis 110,00.
#[test]
fn fall_20_nachunternehmer() {
    let m = rh1();
    let neu = format!("{W20} nu=110");
    let k = werk_mit(&m, &[(W20, neu.as_str())]);
    let b = blatt(&m, &k);
    let w = position(&b, "WDVS EPS 035 d=140mm");
    assert_eq!(w.nu, Some(Cent(11_000)));
    // OZ mit Los, Los-Nr. nicht aufgefüllt (ka-4-fach §3.1, 10:35)
    assert!(w.oz.starts_with("2.01."), "{}", w.oz);
    assert_eq!((w.lohn, w.stoff, w.ep), (Cent(0), Cent(0), Cent(11_000)));
    assert_eq!(w.gp, Cent(2_195_545));
    assert_eq!(w.stoff_gp, Cent(0));
    assert_eq!(b.nu, Cent(2_195_545));
    // nur Material ohne W20: 31.474,23 − 9.979,75 (mit Kies der Erdarbeiten)
    assert_eq!(b.nur_material, Cent(3_147_423 - 997_975));
    assert!(b.befunde.iter().any(|f| f.regel == 83), "{:#?}", b.befunde);
}

/// Fall 8 (Abnahme 21): fester Artikel der Kerndämmung ohne Preis.
#[test]
fn fall_21_preis_fehlt() {
    let m = rh2();
    let k = werk_mit(
        &m,
        &[("unit=m2 price=16 date=10/2026", "unit=m2 date=10/2026")],
    );
    let b = blatt(&m, &k);
    assert_eq!(b.unvollstaendig, 1);
    assert!(position(&b, "Kerndämmung").preis_fehlt);
    assert!(
        b.befunde
            .iter()
            .any(|f| f.regel == 82 && f.satz.contains("gibt es keinen Preis")),
        "{:#?}",
        b.befunde
    );
}

/// Fall 9 (Abnahme 22): Verrechnungslohn 65 €/h, alle Lohnanteile neu,
/// Stoff gleich.
#[test]
fn fall_22_lohn() {
    let m = rh1();
    let alt = blatt(&m, &lesen::katalog(&m, None));
    let k = werk_mit(&m, &[("[rate] key=wage num=60", "[rate] key=wage num=65")]);
    let b = blatt(&m, &k);
    assert_eq!(position(&b, "Bodenplatte").lohn, Cent(3_900));
    assert_eq!(position(&b, "AW Porenbeton").lohn, Cent(2_925));
    for (a, n) in alt.positionen.iter().zip(&b.positionen) {
        assert_eq!(a.stoff, n.stoff, "{}", a.kurz);
        // Vorhaltungen ohne Zeitansatz (Bauvorbereitung) bleiben gleich
        if a.lohn == Cent::NULL {
            assert_eq!(n.lohn, a.lohn, "{}", a.kurz);
        } else {
            assert!(n.lohn > a.lohn, "{}", a.kurz);
        }
    }
    assert_eq!(b.nur_material, alt.nur_material);
}

/// Setzt die Dicke der tragenden Porenbeton-Schicht im Außenwandtyp.
fn aw_dicke(m: &mut Model, mm: f64) {
    let (id, mut t) = m
        .layer_sets()
        .iter()
        .find(|(id, t)| t.code.starts_with("AW") && !m.type_users(*id).is_empty())
        .map(|(id, t)| (id, t.clone()))
        .unwrap();
    let l = t
        .layers
        .iter_mut()
        .find(|l| {
            m.material(l.material)
                .is_some_and(|x| x.name == "Porenbeton")
        })
        .unwrap();
    l.thickness = mm;
    m.begin("Dicke");
    assert!(m.set_layer_set(id, t));
    m.commit();
}

/// Fall 3a (Abnahme 26): nächste Dicke, eigene Zeile, „davon geschätzt“.
#[test]
fn fall_26_geschaetzt_nach_naechster_dicke() {
    let mut m = rh1();
    aw_dicke(&mut m, 200.0);
    let k = lesen::katalog(&m, None);
    let b = blatt(&m, &k);
    let p = position(&b, "AW Porenbeton-Planstein PP2-0,35 d=17,5cm");
    assert!(p.kurz.ends_with(" · geschätzt für d=20cm"), "{}", p.kurz);
    assert_eq!(
        (p.lohn, p.stoff, p.ep),
        (Cent(2_700), Cent(3_017), Cent(5_717))
    );
    assert!(matches!(p.quelle, sk_cost::rechnung::Quelle::Geschaetzt(_)));
    assert_eq!(p.oz, "", "geschätzt: keine OZ (ka-4-fach §3.1, 10:35)");
    assert_eq!(b.geschaetzt, 1);
    assert_eq!(b.geschaetzt_betrag, p.gp);
    // 45 cm nach M30
    aw_dicke(&mut m, 450.0);
    let b = blatt(&m, &k);
    let p = position(&b, "AW Porenbeton-Planstein PP2-0,35 d=36,5cm");
    assert_eq!((p.stoff, p.ep), (Cent(6_130), Cent(9_730)));
    // 50 cm: grau
    aw_dicke(&mut m, 500.0);
    let b = blatt(&m, &k);
    assert!(b
        .positionen
        .iter()
        .all(|p| !p.kurz.starts_with("AW Porenbeton")));
    assert!(!b.ohne.is_empty());
}

/// K12 (Abnahme 27): Artikel der Schicht an zwei Dicken ergibt zwei Zeilen
/// mit „· d=…mm“; die Summe ist die Summe beider. RH-2 hat IW-17,5 im EG und
/// IW-11,5 im OG; M50 gilt hier für 100–180 mm mit Artikel der Schicht, M60
/// ist ausgemustert.
#[test]
fn fall_27_eine_position_ein_preis() {
    let m = rh2();
    let k = werk_mit(
        &m,
        &[
            (
                "pos=50 unit=m2 basis=area hours=0.45 cats=interior mat=2wuC33GkTD9Qack6WJ4EsM tmin=170",
                "pos=50 unit=m2 basis=area hours=0.45 cats=interior mat=2wuC33GkTD9Qack6WJ4EsM tmin=100",
            ),
            (
                "hours=0.4 cats=interior mat=2wuC33GkTD9Qack6WJ4EsM tmin=110 tmax=120",
                "hours=0.4 cats=interior mat=2wuC33GkTD9Qack6WJ4EsM tmin=110 tmax=120 retired=1",
            ),
            (
                "service=1S7bUW001008020000000B nr=1 art=1S7bUW0010080100000002 qty=1",
                "service=1S7bUW001008020000000B nr=1 layer=1 qty=1",
            ),
        ],
    );
    let b = blatt(&m, &k);
    let w: Vec<&sk_cost::Position> = b
        .positionen
        .iter()
        .filter(|p| {
            p.kurz
                .starts_with("IW Porenbeton-Planstein PP2-0,35 d=17,5cm")
        })
        .collect();
    let kurz: Vec<&str> = w.iter().map(|p| p.kurz.as_str()).collect();
    assert_eq!(w.len(), 2, "{kurz:#?}");
    // 22,16 + 4,4 × 1,10 und 15,80 + 4,4 × 1,10
    let d175 = w
        .iter()
        .find(|p| p.kurz.ends_with(" · d=175mm"))
        .expect("175");
    let d115 = w
        .iter()
        .find(|p| p.kurz.ends_with(" · d=115mm"))
        .expect("115");
    assert_eq!((d175.stoff, d115.stoff), (Cent(2_700), Cent(2_064)));
    assert_eq!(d175.menge, d115.menge, "RH-2: beide 18,498 m²");
    // 18,498 × 54,00 = 998,89 und 18,498 × 47,64 = 881,24
    assert_eq!((d175.gp, d115.gp), (Cent(99_889), Cent(88_124)));
    let summe: Cent = b.positionen.iter().map(|p| p.gp).sum();
    assert_eq!(summe, b.netto);
}

/// Fall 2 und 3 (Abnahme 18): Eine zweite Bauleistung 100–400 mm verdrängt
/// M10 nicht; „Porenbeton (2)“ ohne Richtpreis ist grau, mit Richtpreis
/// 400 €/m³ „geschätzt, nur Material“ = Volumen × 400.
#[test]
fn fall_18_genaueste_regel_und_richtpreis() {
    let m = rh1();
    let zweite = "[service] guid=0000000000000000000S18 short=\"AW Porenbeton allgemein\" trade=1S7Wf_00100800000004UQ title=1S7bUW0010080300000003 pos=90 unit=m2 basis=area hours=0.5 cats=exterior mat=2wuC33GkTD9Qack6WJ4EsM tmin=100 tmax=400 fn=loadbearing";
    let k = werk_mit(&m, &[("", zweite)]);
    let b = blatt(&m, &k);
    let m10 = position(&b, "AW Porenbeton-Planstein PP2-0,35 d=17,5cm");
    assert_eq!(m10.gp, Cent(930_010));
    assert!(b
        .positionen
        .iter()
        .all(|p| p.kurz != "AW Porenbeton allgemein"));
    assert!(
        b.befunde
            .iter()
            .any(|f| f.regel == 81 && f.satz.contains("mehrere Bauleistungen passen")),
        "{:#?}",
        b.befunde
    );

    // Porenbeton (2): eigener Baustoff im Außenwandtyp
    let mut m = rh1();
    let (_, pb) = m
        .materials()
        .iter()
        .find(|(_, x)| x.name == "Porenbeton")
        .map(|(id, x)| (id, x.clone()))
        .unwrap();
    let mut pb2 = pb.clone();
    pb2.name = "Porenbeton (2)".into();
    pb2.guid = sk_model::Guid::from_ifc("0000000000000000000P18").unwrap();
    pb2.props.remove(sk_model::matprop::PRICE);
    m.begin("Baustoff");
    let id2 = m.add_material(pb2.clone());
    let (aw, mut t) = m
        .layer_sets()
        .iter()
        .find(|(id, t)| t.code.starts_with("AW") && !m.type_users(*id).is_empty())
        .map(|(id, t)| (id, t.clone()))
        .unwrap();
    for l in &mut t.layers {
        if m.material(l.material)
            .is_some_and(|x| x.name == "Porenbeton")
        {
            l.material = id2;
        }
    }
    assert!(m.set_layer_set(aw, t));
    m.commit();
    let k = lesen::katalog(&m, None);
    let b = blatt(&m, &k);
    assert!(b
        .positionen
        .iter()
        .all(|p| !p.kurz.starts_with("AW Porenbeton")));
    assert!(b
        .ohne
        .iter()
        .any(|o| o.schicht().is_some_and(|s| s.baustoff == pb2.guid)));
    // mit Richtpreis 400 €/m³
    let mut x = m.material(id2).unwrap().clone();
    x.props.insert(
        sk_model::matprop::PRICE.into(),
        sk_model::element::PropValue::Number(400.0),
    );
    m.begin("Richtpreis");
    assert!(m.set_material(id2, x));
    m.commit();
    let b = blatt(&m, &k);
    let p = position(&b, "Porenbeton (2) · geschätzt, nur Material");
    assert_eq!(
        (p.lohn, p.stoff, p.ep),
        (Cent(0), Cent(40_000), Cent(40_000))
    );
    assert_eq!(p.einheit, sk_cost::katalog::Einheit::M3);
    assert_eq!(p.gp, p.stoff_gp);
    // Volumen der Schicht × 400 (M10: 172,224 m² × 0,175 m ≈ 30,139 m³)
    assert_eq!(p.menge.0 / 1000, 30_139);
    assert_eq!(b.geschaetzt, 1);
}
