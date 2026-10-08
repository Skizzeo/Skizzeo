use super::*;
use crate::katalog::{self as kat, Umfeld};
use crate::{lesen, Umfang};
use sk_model::{qto, szo, GuidGen};

const ROHBAU: &str = "1S7bUW0010080300000001";

fn laden(text: &str) -> Model {
    szo::read_with(text, GuidGen::with_seed(1), &lesen::ABSCHNITTE_SZO)
        .expect("lädt")
        .model
}

fn rh1() -> Model {
    laden(include_str!("../../referenz/rh1-standardhaus.szo"))
}

fn wahl(untertitel: bool, preise: bool) -> LvWahl {
    LvWahl {
        los: Guid::from_ifc(ROHBAU).unwrap(),
        untertitel,
        preise,
        heute: Some((2026, 10)),
    }
}

fn gebaeude1(m: &Model) -> Umfang {
    Umfang::gebaeude(m.buildings().iter().next().expect("Gebäude").0)
}

fn ozs(lv: &Lv) -> Vec<(String, Vec<String>)> {
    lv.titel
        .iter()
        .map(|t| {
            (
                t.nr.clone(),
                t.positionen.iter().map(|p| p.oz.clone()).collect(),
            )
        })
        .collect()
}

fn gewerk(m: &Model, code: &str) -> Guid {
    m.trades().iter().find(|t| t.code == code).expect(code).guid
}

/// Fall 1: Los Rohbau, Standardhaus, Gebäude 1, mit Preisen. OZ fest mit
/// Lücken; Zusammenstellung = Kosten der Gewerke 18331 und 18330 ohne
/// geschätzte Zeilen; Menge und EP je Position aus dem Kostenblatt.
#[test]
fn fall_1_rohbau_mit_preisen() {
    let m = rh1();
    let k = lesen::katalog(&m, None);
    let u = gebaeude1(&m);
    let s = qto::schedule(&m);
    let b = lesen::kosten(&m, &s, &k, &u);
    let lv = lesen::lv(&m, &s, &k, &u, &wahl(false, true));
    let o = ozs(&lv);
    assert_eq!(o[0].0, "01");
    assert_eq!(
        o[0].1,
        ["01.0010", "01.0020", "01.0030", "01.0040", "01.0050", "01.0060"]
    );
    assert_eq!(o[1], ("02".to_string(), vec!["02.0010".to_string()]));
    assert!(o[2..].iter().all(|(_, p)| p.is_empty()), "{o:?}");
    assert_eq!(lv.anzahl(), 7);
    let beton_mauer: Cent = b
        .nach_gewerk
        .iter()
        .filter(|(g, _)| *g == Some(gewerk(&m, "18331")) || *g == Some(gewerk(&m, "18330")))
        .map(|x| x.1)
        .sum();
    let z = &lv.zusammenstellung;
    assert_eq!(
        z.netto,
        Some(beton_mauer - z.geschaetzt.unwrap_or(Cent::NULL))
    );
    assert_eq!(
        z.zeilen.len(),
        2,
        "leere Titel nicht in der Zusammenstellung"
    );
    // Abnahme 7: Menge, EP und GP wie im Kostenblatt
    for t in &lv.titel {
        for p in &t.positionen {
            let q = &b.positionen[p.blatt[0]];
            assert_eq!(
                (p.menge, p.ep, p.gp),
                (q.menge, Some(q.ep), Some(q.gp)),
                "{}",
                p.oz
            );
        }
    }
    assert_eq!(lv.kopf.los, "Rohbau");
    assert!(lv
        .kopf
        .vorbemerkungen
        .as_deref()
        .unwrap()
        .starts_with("Abrechnung nach VOB/C"));
    assert_eq!(lv.kopf.art, "mit Preisen");
    // Bauherr fehlt als Hinweis
    assert!(lv
        .befunde
        .iter()
        .any(|f| f.ort == Ort::Kopf && f.satz == "Bauherr fehlt" && f.schwere == Schwere::Hinweis));
}

/// Fall 2: nur EG. B10 und B20 entfallen, B30 bleibt 01.0030.
#[test]
fn fall_2_nur_eg() {
    let m = rh1();
    let k = lesen::katalog(&m, None);
    let mut u = gebaeude1(&m);
    u.ohne = m
        .storeys()
        .iter()
        .filter(|(_, s)| s.short != "EG")
        .map(|(id, _)| id)
        .collect();
    let lv = lesen::lv(&m, &qto::schedule(&m), &k, &u, &wahl(false, true));
    let o = ozs(&lv);
    assert!(!o[0].1.contains(&"01.0010".to_string()), "{o:?}");
    assert!(!o[0].1.contains(&"01.0020".to_string()), "{o:?}");
    assert!(o[0].1.contains(&"01.0030".to_string()), "{o:?}");
}

/// Fall 3 und 3a: Geschosse als Untertitel. B60 dreimal mit eigener OZ und
/// demselben EP 1.600,00; die Mengen ergeben ungerundet die Menge ohne
/// Untertitel. Jede Zeile rechnet nach: GP = Menge × EP, Titel = Σ Zeilen,
/// MwSt. = Netto × 19 %.
#[test]
fn fall_3_geschosse_als_untertitel() {
    let m = rh1();
    let k = lesen::katalog(&m, None);
    let u = gebaeude1(&m);
    let s = qto::schedule(&m);
    let b = lesen::kosten(&m, &s, &k, &u);
    let ohne = lesen::lv(&m, &s, &k, &u, &wahl(false, true));
    let lv = crate::lv::lv_aus(&m, &b, &k, &wahl(true, true));
    let b60: Vec<&LvPosition> = lv.titel[0]
        .positionen
        .iter()
        .filter(|p| p.oz.ends_with(".0060"))
        .collect();
    let oz: Vec<&str> = b60.iter().map(|p| p.oz.as_str()).collect();
    assert_eq!(oz, ["01.01.0060", "01.02.0060", "01.03.0060"]);
    assert!(b60.iter().all(|p| p.ep == Some(Cent(160_000))), "{b60:?}");
    let u_namen: Vec<&str> = lv.titel[0]
        .untertitel
        .iter()
        .map(|u| u.name.as_str())
        .collect();
    assert_eq!(u_namen, ["Gründung", "Erdgeschoss", "Obergeschoss"]);
    // ungerundet gleich: Ansatz des Kostenblatts
    let ganz = ohne.titel[0]
        .positionen
        .iter()
        .find(|p| p.oz == "01.0060")
        .unwrap();
    let roh: i128 = b.positionen[ganz.blatt[0]]
        .ansatz
        .iter()
        .map(|a| a.menge)
        .sum();
    let teile: i128 = b60
        .iter()
        .map(|p| {
            b.positionen[p.blatt[0]]
                .ansatz
                .iter()
                .filter(|a| untertitel_von(&m, a.geschoss).0 == p.untertitel.unwrap())
                .map(|a| a.menge)
                .sum::<i128>()
        })
        .sum();
    assert_eq!(roh, teile);
    // Geld bis auf Rundung: höchstens 0,0005 × EP je Untertitelposition
    let summe: Cent = b60.iter().map(|p| p.gp.unwrap()).sum();
    let diff = (summe - ganz.gp.unwrap()).0.abs() as i128;
    assert!(
        diff * 10_000 <= b60.len() as i128 * (160_000 * 5 + 5_000),
        "{diff}"
    );
    // 3a: Anzeige rechnet nach
    for t in lv.titel.iter().filter(|t| !t.positionen.is_empty()) {
        for p in &t.positionen {
            assert_eq!(p.gp, Some(gp(p.menge, p.ep.unwrap())), "{}", p.oz);
            let ansatz: Dez = Dez(p.ansatz.iter().map(|a| a.menge.0).sum());
            assert_eq!(ansatz, p.menge, "Mengenansatz {}", p.oz);
        }
        let s: Cent = t.positionen.iter().filter_map(|p| p.gp).sum();
        assert_eq!(t.summe, Some(s));
        let su: Cent = t.untertitel.iter().filter_map(|u| u.summe).sum();
        assert_eq!(su, s, "Untertitel ergeben den Titel");
    }
    let z = &lv.zusammenstellung;
    let netto: Cent = z.zeilen.iter().filter_map(|x| x.2).sum();
    assert_eq!(z.netto, Some(netto));
    assert_eq!(z.mwst, Some(Cent(runden(netto.0 as i128 * 19, 100) as i64)));
    assert_eq!(z.brutto, Some(netto + z.mwst.unwrap()));
}

/// Fall 4: „Für Anfrage (leer)“: keine Zahl in EP, GP und Summen; mit
/// Untertiteln hat jede Zeile ihr eigenes leeres EP-Feld.
#[test]
fn fall_4_ohne_preise() {
    let m = rh1();
    let k = lesen::katalog(&m, None);
    let u = gebaeude1(&m);
    for untertitel in [false, true] {
        let lv = lesen::lv(&m, &qto::schedule(&m), &k, &u, &wahl(untertitel, false));
        assert_eq!(lv.kopf.art, "Anfrage ohne Preise");
        for t in &lv.titel {
            assert_eq!(t.summe, None);
            assert!(t.untertitel.iter().all(|u| u.summe.is_none()));
            for p in &t.positionen {
                assert_eq!((p.ep, p.gp, p.anteile), (None, None, None), "{}", p.oz);
                assert!(p.menge.0 > 0);
            }
        }
        let z = &lv.zusammenstellung;
        assert_eq!(
            (z.netto, z.mwst, z.brutto, z.geschaetzt),
            (None, None, None, None)
        );
        assert!(z.zeilen.iter().all(|x| x.2.is_none()));
    }
}

/// Werksbestand mit Ersatz, wie in tests/faelle.rs.
fn werk_mit(m: &Model, ersatz: &[(&str, &str)]) -> Katalog {
    let mut text = crate::WERK.to_string();
    for (alt, neu) in ersatz {
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
    kat::lesen(
        zeilen.iter().map(|(a, l)| (a.as_str(), l.as_str())),
        &Umfeld::aus_modell(m),
        kat::Quelle::Werk {
            stand: "10/2026".into(),
        },
    )
}

/// Fall 5: AW Porenbeton 20 cm ist „nicht ausgeschrieben … geschätzt nach“
/// M10, steht nicht in 02.0010 und erscheint in der Zusammenstellung;
/// Preisstand älter als 12 Monate ist ein Hinweis.
#[test]
fn fall_5_geschaetzt_und_preisstand() {
    let mut m = rh1();
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
    l.thickness = 200.0;
    m.begin("Dicke");
    assert!(m.set_layer_set(id, t));
    m.commit();
    let k = werk_mit(
        &m,
        &[(
            "unit=m2 price=22.16 date=10/2026",
            "unit=m2 price=22.16 date=06/2025",
        )],
    );
    let u = gebaeude1(&m);
    let s = qto::schedule(&m);
    let b = lesen::kosten(&m, &s, &k, &u);
    let lv = lesen::lv(&m, &s, &k, &u, &wahl(false, true));
    let f = lv
        .befunde
        .iter()
        .find(|f| {
            f.satz
                .starts_with("Nicht ausgeschrieben: Porenbeton d=20cm (")
        })
        .unwrap_or_else(|| panic!("{:#?}", lv.befunde));
    assert_eq!(f.schwere, Schwere::Fehler);
    assert!(
        f.satz
            .contains("in den Kosten geschätzt nach 1.02.0010 AW Porenbeton-Planstein"),
        "{}",
        f.satz
    );
    assert!(lv.titel[1].positionen.iter().all(|p| p.oz != "02.0010"));
    assert_eq!(lv.zusammenstellung.geschaetzt, Some(b.geschaetzt_betrag));
}

/// RH-3: Die Dachterrasse hat keine Bauleistung und erscheint im Prüfen
/// des Loses Rohbau (ihr Gewerk hat kein Los).
#[test]
fn dachterrasse_ohne_bauleistung() {
    let m = laden(include_str!("../../referenz/rh3-versatz-dachterrasse.szo"));
    let k = lesen::katalog(&m, None);
    let lv = lesen::lv(
        &m,
        &qto::schedule(&m),
        &k,
        &Umfang::projekt(),
        &wahl(false, true),
    );
    assert!(
        lv.befunde
            .iter()
            .any(|f| f.satz.starts_with("Ohne Bauleistung: Dachterrasse · ")),
        "{:#?}",
        lv.befunde
    );
}

/// K12 (Regel 101): eine Bauleistung an zwei Dicken ist eine Position mit
/// der ganzen Menge; mit Preisen EP leer, Titel unvollständig, Fehler.
#[test]
fn k12_eine_position_mehrere_preise() {
    let m = laden(include_str!("../../referenz/rh2-mehrschalig.szo"));
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
    let s = qto::schedule(&m);
    let u = Umfang::projekt();
    let mit = lesen::lv(&m, &s, &k, &u, &wahl(false, true));
    let p = mit.titel[1]
        .positionen
        .iter()
        .find(|p| p.oz == "02.0050")
        .expect("02.0050");
    assert!(p.mehrere_preise);
    assert_eq!((p.ep, p.gp), (None, None));
    // ganze Menge aus dem Ansatz, nicht die Summe der gerundeten Zeilen
    assert_eq!(p.menge, Dez(36_995_000), "2 × 18,4975…");
    assert!(mit.titel[1].unvollstaendig && mit.zusammenstellung.unvollstaendig);
    let f = mit.befunde.iter().find(|f| f.regel == 101).expect("101");
    assert_eq!(
        f.satz,
        "1.02.0050 IW Porenbeton-Planstein PP2-0,35 d=17,5cm Dünnbettmörtel hat verschiedene Stoffpreise je Dicke (115 mm und 175 mm); bitte die Bauleistung je Dicke anlegen."
    );
    assert_eq!(f.schwere, Schwere::Fehler);
    let leer = lesen::lv(&m, &s, &k, &u, &wahl(false, false));
    let p = leer.titel[1]
        .positionen
        .iter()
        .find(|p| p.oz == "02.0050")
        .unwrap();
    assert_eq!(p.menge, Dez(36_995_000));
}

#[test]
fn bauvorhaben_aus_dem_dateinamen() {
    assert_eq!(bauvorhaben_aus_datei("C:\\Häuser\\haus.szo"), "Haus");
    assert_eq!(
        bauvorhaben_aus_datei("/x/ärger am bach.szo"),
        "Ärger am bach"
    );
    assert_eq!(menge_deutsch(Dez(12_500_000)), "12,500");
    assert_eq!(menge_deutsch(Dez(1_234_567_000)), "1.234,567");
}

/// P1 (Fachprüfung KA-4a): Jede Zeile ist für sich gerundet, der Rest
/// steht als eigener Wert; Zeilen und Rest ergeben die Positionsmenge.
#[test]
fn einzeln_gerundet_mit_ausgleich() {
    // drei Teile je 1/3 m² = 333.333,3… mm²: 3 × 0,333 und Ausgleich 0,001
    let (v, rest) = einzeln(&[333_333, 333_333, 333_334], Einheit::M2);
    assert_eq!(v, [Dez(333_000); 3]);
    assert_eq!(rest, Dez(1_000));
    let (v, rest) = einzeln(&[1_500_000, -500_400], Einheit::M2);
    assert_eq!(v, [Dez(1_500_000), Dez(-500_000)]);
    assert_eq!(rest, Dez::NULL);
}

/// Abnahme 7 (Landkarte P7): Menge und EP jeder LV-Position kommen aus dem
/// Kostenblatt desselben Umfangs; Standardhaus und RH-1 bis RH-3, alle Lose.
#[test]
fn lv_nimmt_menge_und_ep_aus_dem_kostenblatt() {
    for text in [
        include_str!("../../referenz/rh1-standardhaus.szo"),
        include_str!("../../referenz/rh2-mehrschalig.szo"),
        include_str!("../../referenz/rh3-versatz-dachterrasse.szo"),
    ] {
        let m = laden(text);
        let k = lesen::katalog(&m, None);
        let s = qto::schedule(&m);
        let u = Umfang::projekt();
        let b = lesen::kosten(&m, &s, &k, &u);
        let lose: Vec<Guid> = k
            .lose
            .iter()
            .filter(|l| l.parent.is_none() && !l.retired)
            .map(|l| l.guid)
            .collect();
        let mut gesehen = 0;
        for los in lose {
            let w = LvWahl {
                los,
                ..wahl(false, true)
            };
            let lv = lesen::lv(&m, &s, &k, &u, &w);
            for p in lv.titel.iter().flat_map(|t| &t.positionen) {
                gesehen += 1;
                let zeilen: Vec<&crate::Position> =
                    p.blatt.iter().map(|i| &b.positionen[*i]).collect();
                assert!(!zeilen.is_empty(), "{}", p.oz);
                let summe: i64 = zeilen.iter().map(|z| z.menge.0).sum();
                // je Zeile höchstens eine halbe Einheit der dritten Stelle
                let toleranz = 500 * (zeilen.len() as i64 - 1);
                assert!(
                    (p.menge.0 - summe).abs() <= toleranz,
                    "{}: {:?} gegen {summe}",
                    p.oz,
                    p.menge
                );
                if let [z] = zeilen.as_slice() {
                    assert_eq!(p.menge, z.menge, "{}", p.oz);
                    assert_eq!(p.ep, Some(z.ep), "{}", p.oz);
                }
            }
        }
        assert!(gesehen > 5, "{gesehen}");
    }
}

/// Kurztext mit 71 Zeichen ist ein Fehler im LV-Prüfen (Abnahme 6, Regel
/// 79 „Überlänge“, Fachprüfung KA-4a P2): Die Bauleistung bleibt gültig,
/// rechnet und steht im LV; der Befund nennt OZ mit Los und Zeichenzahl.
/// Beim Lesen gibt es dazu keinen Fehler.
#[test]
fn kurztext_mit_71_zeichen() {
    let m = rh1();
    let lang = "Bodenplatte Stb C25/30 XC2 d=18-25cm, Oberfläche flügelgeglättet, ebene";
    assert_eq!(lang.chars().count(), 71);
    let k = werk_mit(
        &m,
        &[(
            "short=\"Bodenplatte Stb C25/30 XC2 d=18-25cm\"",
            &format!("short=\"{lang}\""),
        )],
    );
    assert!(
        !k.befunde.iter().any(|f| f.satz.contains("länger als 70")),
        "{:?}",
        k.befunde
    );
    assert!(k.leistungen.iter().any(|l| l.kurz == lang), "gültig");
    let lv = lesen::lv(
        &m,
        &qto::schedule(&m),
        &k,
        &Umfang::projekt(),
        &wahl(false, true),
    );
    assert!(
        ozs(&lv)[0].1.iter().any(|oz| oz == "01.0010"),
        "{:?}",
        ozs(&lv)
    );
    let f = lv
        .befunde
        .iter()
        .find(|f| f.regel == 79)
        .expect("Befund 79 im LV-Prüfen");
    assert_eq!(f.schwere, Schwere::Fehler);
    assert!(f.satz.contains("71 Zeichen"), "{}", f.satz);
    assert!(f.satz.contains(".01.0010"), "OZ mit Los: {}", f.satz);
}

/// Leere Titel (Bedienbarkeit 2.10) bleiben in der Liste mit 0 Positionen,
/// fehlen aber in der Zusammenstellung.
#[test]
fn leere_titel_bleiben_im_baum() {
    let m = rh1();
    let k = lesen::katalog(&m, None);
    let lv = lesen::lv(
        &m,
        &qto::schedule(&m),
        &k,
        &Umfang::projekt(),
        &wahl(false, true),
    );
    let leer: Vec<&str> = lv
        .titel
        .iter()
        .filter(|t| t.positionen.is_empty())
        .map(|t| t.nr.as_str())
        .collect();
    assert!(
        !leer.is_empty(),
        "Standardhaus ohne Kerndämmung und Verblender"
    );
    for nr in &leer {
        assert!(
            lv.zusammenstellung
                .zeilen
                .iter()
                .all(|(oz, ..)| !oz.ends_with(nr)),
            "{nr} nicht in der Zusammenstellung"
        );
    }
    assert_eq!(
        lv.zusammenstellung.zeilen.len(),
        lv.titel.len() - leer.len()
    );
}

/// Leistung (paket-ka4 §3): das LV liest nur das Kostenblatt und ordnet
/// um; für das Standardhaus deutlich unter einer Millisekunde je Aufruf
/// (hier großzügig gemessen, damit die Prüfmaschine nicht flattert).
#[test]
fn lv_ist_schnell() {
    let m = rh1();
    let k = lesen::katalog(&m, None);
    let s = qto::schedule(&m);
    let u = Umfang::projekt();
    let b = lesen::kosten(&m, &s, &k, &u);
    let w = wahl(false, true);
    let t = std::time::Instant::now();
    for _ in 0..20 {
        std::hint::black_box(lv_aus(&m, &b, &k, &w));
    }
    let je = t.elapsed() / 20;
    assert!(je < std::time::Duration::from_millis(5), "{je:?} je LV");
}

/// Doppelte OZ im Katalog (Regel 86, nur gemeldet): Das LV des Loses
/// meldet sie im Prüfen, je OZ einmal, mit und ohne Untertitel (Review 3an).
#[test]
fn doppelte_oz_im_pruefen() {
    let m = rh1();
    let k = werk_mit(
        &m,
        &[(
            "title=1S7bUW0010080300000002 pos=30 ",
            "title=1S7bUW0010080300000002 pos=60 ",
        )],
    );
    let s = qto::schedule(&m);
    let u = Umfang::projekt();
    let b = lesen::kosten(&m, &s, &k, &u);
    for (ut, oz) in [
        (false, vec!["01.0060"]),
        (true, vec!["01.02.0060", "01.03.0060"]),
    ] {
        let lv = lv_aus(&m, &b, &k, &wahl(ut, true));
        let d: Vec<&Befund> = lv.befunde.iter().filter(|b| b.regel == 86).collect();
        let orte: Vec<Ort> = oz.iter().map(|o| Ort::Position(o.to_string())).collect();
        assert_eq!(
            d.iter().map(|b| b.ort.clone()).collect::<Vec<_>>(),
            orte,
            "{d:?}"
        );
        assert_eq!(d[0].schwere, Schwere::Fehler);
    }
    let lv = lv_aus(&m, &b, &lesen::katalog(&m, None), &wahl(true, true));
    assert!(lv.befunde.iter().all(|b| b.regel != 86));
}
