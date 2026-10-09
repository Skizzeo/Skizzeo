//! Erweiterungsbauteile in Kosten, LV und AVA (Schrittplan E8b).

use sk_cost::erweiterung::kennung;
use sk_cost::katalog::{self, Katalog, Umfeld};
use sk_cost::lv::{lv_aus, LvWahl};
use sk_cost::rechnung::{OhneHerkunft, Position, Quelle};
use sk_cost::{lesen, Dez, Kostenblatt, Schwere};
use sk_model::erweiterung::{ExtDef, ExtPart};
use sk_model::qto::{schedule, Umfang};
use sk_model::{ElementId, Guid, Model, StoreyId};

const BEISPIELE: [&str; 5] = [
    include_str!("../../sk-szb/beispiele/werk.bodenplatte.szb"),
    include_str!("../../sk-szb/beispiele/werk.stabgelaender.szb"),
    include_str!("../../sk-szb/beispiele/werk.streifenfundament.szb"),
    include_str!("../../sk-szb/beispiele/werk.stuetze.szb"),
    include_str!("../../sk-szb/beispiele/werk.treppe.szb"),
];

fn geschoss(m: &Model, kurz: &str) -> StoreyId {
    m.storeys()
        .iter()
        .find(|(_, s)| s.short == kurz && s.building.is_some())
        .map(|(id, _)| id)
        .unwrap_or_else(|| panic!("Geschoss {kurz}"))
}

fn projekt() -> Model {
    let mut m = Model::with_seed(1);
    m.add_building(2);
    for t in BEISPIELE {
        m.put_ext_def(ExtDef::lesen(t).unwrap()).unwrap();
    }
    m
}

fn setzen(m: &mut Model, key: &str, kurz: &str, at: [f64; 2]) -> ElementId {
    let s = geschoss(m, kurz);
    let p = ExtPart::new(m.ext_def(key).unwrap(), at);
    m.add_ext(s, p).unwrap()
}

fn werk_g(id: &str) -> Guid {
    Guid::from_ifc(id).unwrap()
}

const BODENPLATTE: &str = "1S7bUW0010080200000001";
const RANDSCHALUNG: &str = "1S7bUW0010080200000005";
const BETONSTAHL: &str = "1S7bUW0010080200000006";

fn blatt(m: &Model, k: &Katalog, u: &Umfang) -> Kostenblatt {
    lesen::kosten(m, &schedule(m), k, u)
}

fn position(b: &Kostenblatt, g: Guid) -> &Position {
    b.positionen
        .iter()
        .find(|p| p.quelle == Quelle::Leistung(g))
        .unwrap_or_else(|| panic!("Position {}", g.to_ifc()))
}

/// Bodenplatte und Stütze im EG, Geländer im OG.
fn haus() -> (Model, [ElementId; 3]) {
    let mut m = projekt();
    let bp = setzen(&mut m, "werk.bodenplatte", "EG", [0.0, 0.0]);
    let st = setzen(&mut m, "werk.stuetze", "EG", [1000.0, 1000.0]);
    let gl = setzen(&mut m, "werk.stabgelaender", "OG", [0.0, 0.0]);
    (m, [bp, st, gl])
}

/// Abnahme E8b: Stütze und Bodenplatte im LV Rohbau › Betonarbeiten mit den
/// Werks-Leistungen und den eingelesenen Leistungen der Stütze, Betonstahl
/// aus beiden, KG je Ansatz, keine Folgeposition, Geländer ohne Los.
#[test]
fn stuetze_und_bodenplatte_im_lv() {
    let (m, [_, _, gl]) = haus();
    let k = lesen::katalog(&m, None);
    assert!(
        k.befunde.iter().all(|b| b.schwere != Schwere::Fehler),
        "{:#?}",
        k.befunde
    );
    let b = blatt(&m, &k, &Umfang::projekt());
    let bp = position(&b, werk_g(BODENPLATTE));
    assert_eq!((bp.menge, bp.oz.as_str()), (Dez(4_800_000), "1.01.0010"));
    assert_eq!(bp.ansatz[0].kg, Some(322));
    let rand = position(&b, werk_g(RANDSCHALUNG));
    assert_eq!(rand.menge, Dez(20_000_000), "Randschalung 20,000 m");
    let stahl = position(&b, werk_g(BETONSTAHL));
    assert_eq!(stahl.menge, Dez(407_000), "0,384 + 0,023 t");
    let teile = stahl.teile(|a| a.kg);
    assert_eq!(teile, [(Some(322), Dez(384_000)), (Some(343), Dez(23_000))]);
    let grade: Vec<_> = stahl.ansatz.iter().map(|a| a.grad).collect();
    assert_eq!(grade, [Some(80), Some(150)], "B16");
    // eingelesene Leistungen der Stütze im selben Titel, hinter dem Katalog
    let beton = position(&b, kennung("werk.stuetze", "leistung", "stuetze_beton"));
    let schal = position(&b, kennung("werk.stuetze", "leistung", "stuetze_schalung"));
    assert!(beton.oz.starts_with("1.01.") && schal.oz.starts_with("1.01."));
    let hoechste = k
        .leistungen
        .iter()
        .filter(|l| l.titel == k.leistung(werk_g(BODENPLATTE)).unwrap().titel)
        .filter(|l| k.aus_erweiterung("service", l.guid).is_none())
        .map(|l| l.pos)
        .max()
        .unwrap();
    // B14: hinter der höchsten Katalogposition, in der Reihenfolge des
    // Einlesens (Streifenfundament vor Stütze vor Treppe)
    let pos = |d: &str, l: &str| k.leistung(kennung(d, "leistung", l)).unwrap().pos;
    let erste = (hoechste / 10 + 1) * 10;
    assert_eq!(pos("werk.streifenfundament", "fundament_beton"), erste);
    assert_eq!(beton.oz, format!("1.01.{:04}", erste + 10), "B14");
    assert_eq!(pos("werk.treppe", "treppe_schalung"), erste + 40);
    assert!(schal.oz > beton.oz);
    assert_eq!(beton.menge, Dez(152_000));
    assert_eq!(schal.menge, Dez(2_530_000));
    assert_eq!(beton.ansatz[0].kg, Some(343));
    // keine Folgeposition aus 0001
    assert!(b
        .positionen
        .iter()
        .flat_map(|p| &p.ansatz)
        .all(|a| a.aus.is_none()));
    // Geländer: rechnet, steht in keinem Los (A8, E8-2)
    let g = position(&b, kennung("werk.stabgelaender", "leistung", "gelaender"));
    assert_eq!((g.oz.as_str(), g.menge), ("", Dez(3_000_000)));
    let e = m.element(gl).unwrap();
    let los: Vec<_> = b
        .befunde
        .iter()
        .filter(|x| x.satz.starts_with("Steht in keinem Los"))
        .collect();
    assert_eq!(los.len(), 1, "{:#?}", b.befunde);
    assert_eq!(los[0].schwere, Schwere::Fehler);
    assert_eq!(los[0].ort, sk_cost::Ort::Bauteil(e.guid));
    assert!(los[0].satz.contains("GL-001") && los[0].satz.contains("(DIN 18360)"));
    // B15: drei Leistungen aus Erweiterungen
    assert!(b
        .befunde
        .iter()
        .any(|x| x.satz == "Preise aus Erweiterung, nicht im Firmenkatalog: 3 Leistungen"));
    let h = k.aus_erweiterung(
        "service",
        kennung("werk.stuetze", "leistung", "stuetze_beton"),
    );
    assert_eq!(
        h.unwrap().herkunft(),
        "aus Erweiterung werk.stuetze v1 · grob"
    );
    // LV Rohbau: die Positionen der Erweiterungen, B16 im Mengenansatz
    let rohbau = k
        .los(k.leistung(werk_g(BODENPLATTE)).unwrap().titel)
        .unwrap()
        .parent
        .unwrap();
    let w = LvWahl {
        los: rohbau,
        untertitel: false,
        preise: true,
        heute: None,
    };
    let lv = lv_aus(&m, &b, &k, &w);
    let pos: Vec<_> = lv.titel.iter().flat_map(|t| &t.positionen).collect();
    let quellen: Vec<Guid> = pos.iter().map(|p| p.quelle).collect();
    for q in [beton, schal, bp, stahl].map(|p| match p.quelle {
        Quelle::Leistung(g) => g,
        _ => unreachable!(),
    }) {
        assert!(quellen.contains(&q));
    }
    let lv_stahl = pos.iter().find(|p| p.quelle == werk_g(BETONSTAHL)).unwrap();
    let herkunft: Vec<&str> = lv_stahl
        .ansatz
        .iter()
        .map(|a| a.herkunft.as_str())
        .collect();
    assert_eq!(
        herkunft,
        [
            "aus Modell (80 kg/m³ aus der Definition)",
            "aus Modell (150 kg/m³ aus der Definition)"
        ]
    );
    assert_eq!(lv_stahl.kg_teile, [(322, Dez(384_000)), (343, Dez(23_000))]);
    // Σ LV = Netto ohne das Geländer
    let summe: sk_cost::Cent = pos.iter().filter_map(|p| p.gp).sum();
    assert_eq!(summe + g.gp, b.netto);
    // die Zusammenstellung nennt das Geländer als „ohne Los“
    assert_eq!(lv.zusammenstellung.ohne_los, Some(g.gp));
    let ohne_preise = lv_aus(&m, &b, &k, &LvWahl { preise: false, ..w });
    assert_eq!(ohne_preise.zusammenstellung.ohne_los, None);
}

/// Werkszeilen, `f` ändert jede Zeile (auch zu mehreren).
fn werk_mit(m: &Model, f: impl Fn(&str) -> String) -> Katalog {
    let text = include_str!("../werk.szk");
    let neu: Vec<String> = text.lines().map(&f).collect();
    let zeilen: Vec<(String, String)> = neu
        .iter()
        .flat_map(|l| l.lines())
        .filter_map(|l| {
            let sec = l.strip_prefix('[')?.split(']').next()?;
            Some((sec.to_string(), l.to_string()))
        })
        .collect();
    katalog::lesen(
        zeilen.iter().map(|(a, b)| (a.as_str(), b.as_str())),
        &Umfeld::aus_modell(m),
        katalog::Quelle::Werk {
            stand: "10/2026".into(),
        },
    )
}

fn grund(b: &Kostenblatt) -> Vec<String> {
    b.ohne
        .iter()
        .filter_map(|o| match &o.herkunft {
            OhneHerkunft::Erweiterung { name, grund, .. } => Some(format!("{name}: {grund}")),
            OhneHerkunft::Schicht { .. } => None,
        })
        .collect()
}

/// A7: Dicke außerhalb des Bands der Bauleistung gibt keine Position,
/// sondern eine graue Zeile mit Grund und einen Befund.
#[test]
fn dicke_ausserhalb_des_bands() {
    let (mut m, [bp, ..]) = haus();
    let mut p = match &m.element(bp).unwrap().kind {
        sk_model::ElementKind::Ext(p) => p.clone(),
        _ => unreachable!(),
    };
    p.set("d", 300.0);
    assert!(m.set_ext(bp, p));
    let k = lesen::katalog(&m, None);
    let b = blatt(&m, &k, &Umfang::projekt());
    assert!(b
        .positionen
        .iter()
        .all(|p| p.quelle != Quelle::Leistung(werk_g(BODENPLATTE))));
    let kurz = &k.leistung(werk_g(BODENPLATTE)).unwrap().kurz;
    assert_eq!(
        grund(&b),
        [format!(
            "Beton C25/30: Dicke 300 mm liegt außerhalb von {kurz} (180–250 mm)"
        )]
    );
    let o = &b.ohne[0];
    assert_eq!(
        (o.menge, o.kg, o.nummer.as_str()),
        (Dez(7_200_000), Some(322), "BP-001")
    );
    assert!(b.befunde.iter().any(|x| x.schwere == Schwere::Warnung
        && x.satz
            .starts_with("Ohne Bauleistung: Bodenplatte BP-001 · Beton C25/30 (Dicke 300 mm")));
    // Randschalung und Betonstahl bleiben
    assert_eq!(position(&b, werk_g(RANDSCHALUNG)).menge, Dez(20_000_000));
    assert_eq!(position(&b, werk_g(BETONSTAHL)).ansatz[0].menge, 576_000);
}

/// Einheit passt nicht, Kennung unbekannt: grau mit Grund, nie still weg.
#[test]
fn einheit_und_kennung() {
    let mut m = Model::with_seed(1);
    m.add_building(2);
    let t = BEISPIELE[3]
        .replace(
            "key=schalung name=\"Schalung Stütze\" einheit=m2",
            "key=schalung name=\"Schalung Stütze\" einheit=m",
        )
        .replace(
            "leistung=1S7bUW0010080200000006",
            "leistung=1S7bUW00100802000000ZZ",
        );
    m.put_ext_def(ExtDef::lesen(&t).unwrap()).unwrap();
    setzen(&mut m, "werk.stuetze", "EG", [0.0, 0.0]);
    let k = lesen::katalog(&m, None);
    let b = blatt(&m, &k, &Umfang::projekt());
    assert_eq!(
        grund(&b),
        [
            "Schalung Stütze: Einheit m passt nicht zur Bauleistung in m²",
            "Betonstahl B500: Bauleistung 1S7bUW00100802000000ZZ gibt es im Katalog nicht"
        ]
    );
    assert!(b.befunde.iter().any(|x| x.satz
        == "Ohne Bauleistung: Stahlbetonstütze ST-001 · Schalung Stütze (Einheit m passt nicht zur Bauleistung in m²)"));
    // Zählmenge „1 Stk Stütze“ ist keine Kostenzeile
    assert_eq!(b.positionen.len(), 1);
}

/// E8-6: Ein Firmenpreis für den Werks-Transportbeton wirkt bei der Stütze;
/// ein Erweiterungsartikel mit genau gleichem Namen nimmt den Katalogartikel.
#[test]
fn firmenpreis_und_gleicher_name() {
    let (m, _) = haus();
    let teuer = werk_mit(&m, |l| l.to_string());
    let firma = werk_mit(&m, |l| {
        if l.starts_with("[article] guid=1S7bUW0010080100000006") {
            l.replace("price=190", "price=175")
        } else {
            l.to_string()
        }
    });
    let g = kennung("werk.stuetze", "leistung", "stuetze_beton");
    let ep = |k: &Katalog| position(&blatt(&m, k, &Umfang::projekt()), g).stoff;
    assert_eq!(ep(&teuer).0 - ep(&firma).0, 1500, "15 € je m³");
    // eigener Artikel, Name wie 0006 (Leerzeichen gleichgültig)
    let mut m2 = Model::with_seed(1);
    m2.add_building(2);
    let t = BEISPIELE[3]
        .replace(
            "[leistung] key=stuetze_beton",
            "[artikel] key=beton_c25 name=\"Transportbeton  C25/30 XC1-XC2 F3\" einheit=m3 preis=190\n[leistung] key=stuetze_beton",
        )
        .replace("stoffe=\"1S7bUW0010080100000006:1\"", "stoffe=\"beton_c25:1\"");
    m2.put_ext_def(ExtDef::lesen(&t).unwrap()).unwrap();
    setzen(&mut m2, "werk.stuetze", "EG", [0.0, 0.0]);
    let firma2 = werk_mit(&m2, |l| l.replace("price=190", "price=175"));
    let a = firma2
        .erweiterung
        .iter()
        .find(|e| e.key == "beton_c25")
        .unwrap();
    assert!(a.statt);
    assert_eq!(a.guid, werk_g("1S7bUW0010080100000006"));
    assert!(firma2
        .artikel(kennung("werk.stuetze", "artikel", "beton_c25"))
        .is_none());
    let b = blatt(&m2, &firma2, &Umfang::projekt());
    assert_eq!(position(&b, g).stoff, ep(&firma));
}

/// Ein Katalogsatz mit der abgeleiteten Kennung (übernommener Vorschlag)
/// geht vor.
#[test]
fn katalogsatz_geht_vor() {
    let (m, _) = haus();
    let g = kennung("werk.stuetze", "leistung", "stuetze_beton");
    let k0 = lesen::katalog(&m, None);
    let titel = k0.leistung(werk_g(BODENPLATTE)).unwrap().titel;
    let zeile = format!(
        "[service] guid={} short=\"Stütze aus der Firma\" trade={} title={} pos=900 unit=m3 basis=volume hours=2",
        g.to_ifc(),
        k0.leistung(werk_g(BODENPLATTE)).unwrap().gewerk.to_ifc(),
        titel.to_ifc()
    );
    let mut z: Vec<(String, String)> = Vec::new();
    for l in include_str!("../werk.szk").lines() {
        if let Some(sec) = l.strip_prefix('[').and_then(|x| x.split(']').next()) {
            z.push((sec.to_string(), l.to_string()));
        }
    }
    z.push(("service".into(), zeile));
    let k = katalog::lesen(
        z.iter().map(|(a, b)| (a.as_str(), b.as_str())),
        &Umfeld::aus_modell(&m),
        katalog::Quelle::Werk {
            stand: "10/2026".into(),
        },
    );
    assert!(k.befunde.iter().all(|b| b.regel != 74), "{:#?}", k.befunde);
    assert!(k.aus_erweiterung("service", g).is_none());
    assert_eq!(k.leistung(g).unwrap().kurz, "Stütze aus der Firma");
    let b = blatt(&m, &k, &Umfang::projekt());
    assert_eq!(position(&b, g).oz, "1.01.0900");
}

/// E8-8: Umfang ohne OG nimmt das Geländer heraus; Geschosse und Ausgleich
/// ergeben das Netto.
#[test]
fn umfang_und_geschosse() {
    let (m, _) = haus();
    let k = lesen::katalog(&m, None);
    let u = Umfang {
        gebaeude: None,
        ohne: vec![geschoss(&m, "OG")],
    };
    let b = blatt(&m, &k, &u);
    let g = kennung("werk.stabgelaender", "leistung", "gelaender");
    assert!(b.positionen.iter().all(|p| p.quelle != Quelle::Leistung(g)));
    assert!(b
        .befunde
        .iter()
        .all(|x| !x.satz.starts_with("Steht in keinem Los")));
    let s: sk_cost::Cent = b.nach_geschoss.iter().map(|x| x.1).sum();
    assert_eq!(s + b.ausgleich_geschoss, b.netto);
    assert_eq!(b.nach_geschoss.len(), 1);
}

/// Frage 3 b: Eine neue Version der Definition ändert Umfeld und Katalog,
/// damit App und Kostenspeicher neu rechnen.
#[test]
fn neue_version_rechnet_neu() {
    let (mut m, _) = haus();
    let vorher = (lesen::umfeld_stempel(&m), lesen::katalog(&m, None).stempel);
    let t = BEISPIELE[3]
        .replace("version=1", "version=2")
        .replace("preis=10 ", "preis=11 ");
    m.put_ext_def(ExtDef::lesen(&t).unwrap()).unwrap();
    let nachher = (lesen::umfeld_stempel(&m), lesen::katalog(&m, None).stempel);
    assert_ne!(vorher.0, nachher.0);
    assert_ne!(vorher.1, nachher.1);
    let k = lesen::katalog(&m, None);
    let g = kennung("werk.stuetze", "leistung", "stuetze_schalung");
    let b = blatt(&m, &k, &Umfang::projekt());
    assert_eq!(position(&b, g).stoff.0, 1100);
}

/// E8-3: Bauleistung mit „Artikel der Schicht“ wählt nach `dicke` der
/// Menge; verschiedene Stoffpreise nennt Regel 101 mit dem Bauteil.
#[test]
fn artikel_der_schicht_nach_dicke() {
    let mut m = projekt();
    setzen(&mut m, "werk.bodenplatte", "EG", [0.0, 0.0]);
    let b = setzen(&mut m, "werk.bodenplatte", "EG", [7000.0, 0.0]);
    let mut p = match &m.element(b).unwrap().kind {
        sk_model::ElementKind::Ext(p) => p.clone(),
        _ => unreachable!(),
    };
    p.set("d", 250.0);
    assert!(m.set_ext(b, p));
    let sb = m
        .materials()
        .iter()
        .find(|(_, x)| x.name == "Stahlbeton")
        .map(|(_, x)| x.guid)
        .unwrap()
        .to_ifc();
    let k = werk_mit(&m, |l| {
        if l.starts_with("[svcpart] guid=1S7bUW0010080400000001") {
            l.replace("art=1S7bUW0010080100000006", "layer=1")
        } else if l.starts_with("[catalog]") {
            format!(
                "{l}\n[article] guid=0000000000000000000A20 name=\"Beton 20\" mat={sb} t=200 unit=m3 price=100"
            )
        } else if l.starts_with("[lot]") && l.contains("guid=1S7bUW0010080300000001") {
            format!(
                "{l}\n[article] guid=0000000000000000000A25 name=\"Beton 25\" mat={sb} t=250 unit=m3 price=120"
            )
        } else {
            l.to_string()
        }
    });
    let blatt = blatt(&m, &k, &Umfang::projekt());
    let bp: Vec<&Position> = blatt
        .positionen
        .iter()
        .filter(|p| p.quelle == Quelle::Leistung(werk_g(BODENPLATTE)))
        .collect();
    assert_eq!(bp.len(), 2, "{:#?}", k.befunde);
    assert_eq!(bp[1].stoff.0 - bp[0].stoff.0, 2000);
    let rohbau = k
        .los(k.leistung(werk_g(BODENPLATTE)).unwrap().titel)
        .unwrap()
        .parent
        .unwrap();
    let w = LvWahl {
        los: rohbau,
        untertitel: false,
        preise: true,
        heute: None,
    };
    let lv = lv_aus(&m, &blatt, &k, &w);
    let r: Vec<&str> = lv
        .befunde
        .iter()
        .filter(|b| b.regel == 101)
        .map(|b| b.satz.as_str())
        .collect();
    assert_eq!(r.len(), 1);
    assert!(
        r[0].contains("je Dicke und Erweiterung (Bodenplatte BP-001 und Bodenplatte BP-002)"),
        "{}",
        r[0]
    );
}

/// Prüfung E8, Frage 2: Hinweis beim Einlesen, wenn eine Folge der genutzten
/// Werks-Leistung in der Definition fehlt.
#[test]
fn fehlende_folge_beim_einlesen() {
    let m = projekt();
    let k = lesen::katalog(&m, None);
    let bp = include_str!("../../sk-szb/beispiele/werk.bodenplatte.szb");
    let d = ExtDef::lesen(bp).unwrap();
    assert_eq!(
        sk_cost::erweiterung::fehlende_folgen(&k, &d),
        Vec::<String>::new()
    );
    let ohne_stahl: String = bp
        .lines()
        .filter(|l| !l.starts_with("[menge] key=stahl "))
        .map(|l| format!("{l}\n"))
        .collect();
    let d = ExtDef::lesen(&ohne_stahl).unwrap();
    let h = sk_cost::erweiterung::fehlende_folgen(&k, &d);
    let bpl = k.leistung(werk_g(BODENPLATTE)).unwrap();
    let stahl = k.leistung(werk_g(BETONSTAHL)).unwrap();
    assert_eq!(
        h,
        [format!(
            "{} nutzt „{}“; deren Folge „{}“ kommt in der Definition nicht vor",
            d.name(),
            bpl.kurz,
            stahl.kurz
        )]
    );
}

/// Review 3cm: Kostenblatt und LV mit vielen Erweiterungen (Messung).
#[test]
#[ignore]
fn probe_3cm() {
    use std::time::Instant;
    for (n, gl) in [(0usize, 0usize), (500, 0), (2000, 0), (2000, 500)] {
        let mut m = projekt();
        for i in 0..n {
            setzen(
                &mut m,
                "werk.stuetze",
                "EG",
                [(i % 50) as f64 * 1000.0, (i / 50) as f64 * 1000.0],
            );
        }
        // Geländer: steht in keinem Los, je Bauteil ein Befund
        for i in 0..gl {
            setzen(
                &mut m,
                "werk.stabgelaender",
                "OG",
                [(i % 50) as f64 * 1000.0, (i / 50) as f64 * 1000.0],
            );
        }
        let t0 = Instant::now();
        let k = lesen::katalog(&m, None);
        let kat = t0.elapsed();
        let s = schedule(&m);
        let u = Umfang::projekt();
        let t1 = Instant::now();
        let (b, sp) = sk_cost::rechnung::kosten_mit(Default::default(), &m, &s, &k, &u);
        let kalt = t1.elapsed();
        let t2 = Instant::now();
        let (b2, _) = sk_cost::rechnung::kosten_mit(sp, &m, &s, &k, &u);
        let warm = t2.elapsed();
        assert_eq!(b.netto, b2.netto);
        println!(
            "E={n} gl={gl} katalog={kat:?} kalt={kalt:?} warm={warm:?} befunde={} pos={}",
            b.befunde.len(),
            b.positionen.len()
        );
    }
}
