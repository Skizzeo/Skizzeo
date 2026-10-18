//! Automatikmengen (Erdarbeiten, Bauvorbereitung): Bauleistungen mit
//! `auto=` rechnen mit den Mengen der Gründung, ohne Schicht.

use sk_cost::rechnung::Quelle;
use sk_cost::{lesen, Dez, Kostenblatt, Umfang};
use sk_model::{qto, szo, Guid, GuidGen, Model};

const RH1: &str = include_str!("../referenz/rh1-standardhaus.szo");
const TITEL: &str = "1S7bUW001008030000000B";
const ERDBAU: &str = "1S7Wf_00100800000004Ty";

/// Standardhaus mit einer Projektkopie aus Los, Titel und `zeilen`.
fn haus(zeilen: &[String]) -> Model {
    let mut t = RH1.to_string();
    t += "[lot] guid=1S7bUW001008030000000A name=\"Rohbau\" nr=\"1\"\n";
    t += &format!(
        "[lot] guid={TITEL} name=\"Erdarbeiten\" nr=\"02\" parent=1S7bUW001008030000000A\n"
    );
    for z in zeilen {
        t += z;
        t += "\n";
    }
    szo::read_with(&t, GuidGen::with_seed(1), &lesen::ABSCHNITTE_SZO)
        .expect("lädt")
        .model
}

fn leistung(guid: &str, kurz: &str, pos: u32, rest: &str) -> String {
    format!("[service] guid={guid} short=\"{kurz}\" trade={ERDBAU} title={TITEL} pos={pos} {rest}")
}

fn blatt(m: &Model) -> Kostenblatt {
    let k = lesen::katalog(m, None);
    lesen::kosten(m, &qto::schedule(m), &k, &Umfang::projekt())
}

const OBERBODEN: &str = "1S7bUW001008020000000N";
const AUSHUB: &str = "1S7bUW001008020000000O";

#[test]
fn automatikposition_aus_der_gruendung() {
    let m = haus(&[
        leistung(
            OBERBODEN,
            "Oberboden abtragen",
            10,
            "unit=m3 basis=auto auto=earth.topsoil hours=0.1 kg=311",
        ),
        leistung(
            AUSHUB,
            "Aushub",
            20,
            "unit=m3 basis=auto auto=earth.trench_sand equip=12",
        ),
    ]);
    let s = qto::schedule(&m);
    let summe = |k: &str| -> f64 {
        s.auto
            .iter()
            .filter(|a| a.key == k)
            .map(|a| a.value)
            .sum::<f64>()
    };
    assert!(summe("earth.topsoil") > 0.0, "{:?}", s.auto);
    let b = blatt(&m);
    let g = Guid::from_ifc(OBERBODEN).unwrap();
    let p = b
        .positionen
        .iter()
        .find(|p| p.quelle == Quelle::Leistung(g))
        .expect("Oberboden als Position");
    // Menge auf 3 Stellen aus der Automatikmenge, KG der Bauleistung
    let soll = (summe("earth.topsoil") / 1e9 * 1000.0).round() as i64 * 1000;
    assert_eq!(p.menge, Dez(soll));
    assert_eq!(p.oz, "1.02.0010");
    assert!(p
        .ansatz
        .iter()
        .all(|a| a.kg == Some(311) && a.formel.is_some()));
    // Ohne KG an der Bauleistung gilt die der Menge (311)
    let a = Guid::from_ifc(AUSHUB).unwrap();
    let q = b
        .positionen
        .iter()
        .find(|p| p.quelle == Quelle::Leistung(a))
        .expect("Graben als Position");
    assert!(q.ansatz.iter().all(|a| a.kg == Some(311)));
    assert_eq!(q.ep.0, 1200);
}

/// Ausgemustert: keine Position (so schaltet ein Projekt sie ab).
#[test]
fn ausgemustert_rechnet_nicht() {
    let m = haus(&[leistung(
        OBERBODEN,
        "Oberboden abtragen",
        10,
        "unit=m3 basis=auto auto=earth.topsoil retired=1",
    )]);
    let g = Guid::from_ifc(OBERBODEN).unwrap();
    assert!(blatt(&m)
        .positionen
        .iter()
        .all(|p| p.quelle != Quelle::Leistung(g)));
}

/// Einheit passt nicht zur Menge: Hinweis, keine Position. `basis=auto`
/// ohne `auto=` gilt nicht.
#[test]
fn einheit_und_pflichtfeld() {
    let m = haus(&[
        leistung(
            OBERBODEN,
            "Oberboden abtragen",
            10,
            "unit=m2 basis=auto auto=earth.topsoil",
        ),
        leistung(AUSHUB, "Aushub", 20, "unit=m3 basis=auto"),
    ]);
    let b = blatt(&m);
    assert!(b.positionen.is_empty(), "{:?}", b.positionen);
    assert!(b.befunde.iter().any(|x| x.regel == 80), "{:?}", b.befunde);
    let k = lesen::katalog(&m, None);
    assert!(k.befunde.iter().any(|x| x.regel == 79), "{:?}", k.befunde);
}

/// Umfang: ein Gebäude ohne Gründungsband rechnet keine Erdarbeiten.
#[test]
fn umfang_ohne_gruendung() {
    let m = haus(&[leistung(
        OBERBODEN,
        "Oberboden abtragen",
        10,
        "unit=m3 basis=auto auto=earth.topsoil",
    )]);
    let s = qto::schedule(&m);
    let gr = s.auto[0].storey;
    let u = Umfang {
        gebaeude: None,
        ohne: vec![gr],
    };
    let k = lesen::katalog(&m, None);
    let b = lesen::kosten(&m, &s, &k, &u);
    assert!(b.positionen.is_empty());
}

/// Prüfung 10.10. (Briefing QS §4): Ein unbekannter Schlüssel an `auto=`
/// liefert nie eine Menge. Das darf nicht still bleiben, sondern gibt einen
/// Befund mit der Kurzbezeichnung der Bauleistung.
#[test]
fn unbekannter_auto_schluessel_gibt_befund() {
    let m = haus(&[leistung(
        OBERBODEN,
        "Oberboden abtragen",
        10,
        "unit=m3 basis=auto auto=earth.tippfehler hours=0.1 kg=311",
    )]);
    let k = lesen::katalog(&m, None);
    let bf = lesen::befunde(&m, &k);
    assert!(
        bf.iter()
            .any(|b| b.satz.contains("Oberboden abtragen") && b.satz.contains("earth.tippfehler")),
        "{:?}",
        bf.iter().map(|b| &b.satz).collect::<Vec<_>>()
    );
}

/// Mit Höhenaufmaß rechnen die Erdmengen über das Netz (G2): Der
/// Mengenansatz nennt das Verfahren, das LV setzt die Vorbemerkung dazu,
/// einen Hinweis gibt es nicht (VOB-Prüfung G2, A2 und A3). Deckt das Netz
/// die Baugrube nicht, rechnen sie wie in G1 aus der mittleren Geländehöhe,
/// und Kostenblatt und LV geben den Hinweis. Ohne Aufmaß bleibt alles wie
/// bisher.
#[test]
fn erdmengen_aus_dem_aufmass_geben_hinweis() {
    use sk_cost::befund::Ort;
    use sk_cost::lv::{lv_aus, vorbemerkung_netz, LvWahl};
    use sk_cost::rechnung::ERDE_AUS_MITTEL;
    use sk_model::qto_earth::DREIECKSPRISMEN;
    let aushub = leistung(
        AUSHUB,
        "Aushub",
        20,
        "unit=m3 basis=auto auto=earth.trench_sand",
    );
    let mit_aufmass = |punkte: [&str; 4]| {
        let mut z = vec![aushub.clone()];
        z.push(
            "[terrain] src=\"aufmass.txt\" read=\"2026-10-14\" crs=local hsys=DHHN2016 local=1 \
             e0=100 n0=100 rot=0"
                .to_string(),
        );
        z.extend(punkte.map(String::from));
        haus(&z)
    };
    let hinweis = |v: &[sk_cost::Befund]| v.iter().filter(|x| x.satz == ERDE_AUS_MITTEL).count();
    let w = LvWahl {
        los: Guid::from_ifc("1S7bUW001008030000000A").unwrap(),
        untertitel: false,
        preise: false,
        heute: None,
    };
    let a = Guid::from_ifc(AUSHUB).unwrap();
    let graben = |b: &Kostenblatt| {
        b.positionen
            .iter()
            .find(|p| p.quelle == Quelle::Leistung(a))
            .expect("Graben als Position")
            .ansatz
            .iter()
            .map(|x| x.formel.clone().unwrap_or_default())
            .collect::<Vec<_>>()
    };

    // das Netz deckt die Baugrube
    let m = mit_aufmass([
        "[tpoint] nr=1 e=0 n=0 h=10",
        "[tpoint] nr=2 e=200 n=0 h=11",
        "[tpoint] nr=3 e=200 n=200 h=12",
        "[tpoint] nr=4 e=0 n=200 h=11",
    ]);
    assert!(m.terrain().is_some());
    let b = blatt(&m);
    assert_eq!(hinweis(&b.befunde), 0, "{:?}", b.befunde);
    let f = graben(&b);
    assert!(
        f.iter().all(|f| f.contains(DREIECKSPRISMEN)
            && f.ends_with("aus dem Höhenaufmaß aufmass.txt vom 14.10.2026")),
        "{f:?}"
    );
    let lv = lv_aus(&m, &b, &lesen::katalog(&m, None), &w);
    assert!(lv.befunde.iter().all(|x| x.satz != ERDE_AUS_MITTEL));
    // die Dreieckszahl einmal in der Vorbemerkung (VOB-Spruch zu A2)
    let v = vorbemerkung_netz(Some("Höhenaufmaß aufmass.txt vom 14.10.2026, 2 Dreiecke"));
    assert!(v.contains("(Dreiecksprismen, REB-VB 22.013; Höhenaufmaß aufmass.txt vom"));
    assert_eq!(lv.kopf.vorbemerkungen.as_deref(), Some(v.as_str()));

    // das Netz deckt nur einen Teil: mittlere Höhe mit Hinweis
    let m = mit_aufmass([
        "[tpoint] nr=1 e=100 n=100 h=10",
        "[tpoint] nr=2 e=102 n=100 h=10",
        "[tpoint] nr=3 e=102 n=102 h=10",
        "[tpoint] nr=4 e=100 n=102 h=10",
    ]);
    let b = blatt(&m);
    assert_eq!(hinweis(&b.befunde), 1, "{:?}", b.befunde);
    assert!(graben(&b)
        .iter()
        .all(|f| f.contains("; Gelände aus Aufmaß, Mittel ")));
    let lv = lv_aus(&m, &b, &lesen::katalog(&m, None), &w);
    let im_lv: Vec<_> = lv
        .befunde
        .iter()
        .filter(|x| x.satz == ERDE_AUS_MITTEL)
        .collect();
    assert_eq!(im_lv.len(), 1, "{:?}", lv.befunde);
    assert_eq!(lv.kopf.vorbemerkungen, None);
    // Ort: die Position im LV (OZ ohne Los)
    let lv_pos = lv
        .titel
        .iter()
        .flat_map(|t| &t.positionen)
        .find(|p| p.quelle == a)
        .expect("Graben im LV");
    assert_eq!(im_lv[0].ort, Ort::Position(lv_pos.oz.clone()));

    // Aufmaß ohne Lage (UTM, das Projekt hat keine Landeslage): die Mengen
    // bleiben auf der Ebene, also auch kein Hinweis (H123-1)
    let mut z = vec![aushub.clone()];
    z.extend(
        [
            "[terrain] src=\"aufmass.csv\" crs=utm32 hsys=DHHN2016",
            "[tpoint] nr=1 e=467100 n=5878700 h=10",
            "[tpoint] nr=2 e=467300 n=5878700 h=11",
            "[tpoint] nr=3 e=467300 n=5878900 h=12",
            "[tpoint] nr=4 e=467100 n=5878900 h=11",
        ]
        .map(String::from),
    );
    let ohne_lage = haus(&z);
    assert!(ohne_lage.terrain().is_some());
    let b = blatt(&ohne_lage);
    assert_eq!(hinweis(&b.befunde), 0, "{:?}", b.befunde);
    assert!(b.positionen.iter().all(|p| p.ansatz.iter().all(|x| !x
        .formel
        .as_deref()
        .unwrap_or("")
        .contains("Aufmaß"))));
    let lv = lv_aus(&ohne_lage, &b, &lesen::katalog(&ohne_lage, None), &w);
    assert!(
        lv.befunde.iter().all(|x| x.satz != ERDE_AUS_MITTEL),
        "{:?}",
        lv.befunde
    );

    let ohne = haus(&[aushub]);
    let b = blatt(&ohne);
    assert_eq!(hinweis(&b.befunde), 0);
    assert!(b.positionen.iter().all(|p| p.ansatz.iter().all(|x| !x
        .formel
        .as_deref()
        .unwrap_or("")
        .contains("Aufmaß"))));
}

/// Angenommenes Gelände (I1, VOB-Spruch zu I1): Der Ansatz nennt es, das LV
/// setzt seine eigene Vorbemerkung, Kostenblatt und LV geben den Hinweis
/// „Urgelände nicht vermessen“ (ohne zu sperren).
#[test]
fn erdmengen_aus_angenommenem_gelaende() {
    use sk_cost::lv::{lv_aus, vorbemerkung_angenommen, LvWahl};
    use sk_cost::rechnung::NICHT_VERMESSEN;
    use sk_model::qto_earth::ANGENOMMEN;
    let mut z = vec![leistung(
        AUSHUB,
        "Aushub",
        20,
        "unit=m3 basis=auto auto=earth.trench_sand",
    )];
    z.extend(
        [
            "[terrain] src=\"\" crs=local hsys=DHHN2016 local=1 e0=0 n0=0 rot=0",
            "[tpoint] nr=H1 e=-100 n=-100 h=0 src=hand",
            "[tpoint] nr=H2 e=100 n=-100 h=0 src=hand",
            "[tpoint] nr=H3 e=100 n=100 h=0 src=hand",
            "[tpoint] nr=H4 e=-100 n=100 h=0 src=hand",
        ]
        .map(String::from),
    );
    let m = haus(&z);
    assert!(m.terrain().is_some_and(|t| t.assumed()));
    let b = blatt(&m);
    let a = Guid::from_ifc(AUSHUB).unwrap();
    let p = b
        .positionen
        .iter()
        .find(|p| p.quelle == Quelle::Leistung(a))
        .expect("Graben als Position");
    assert!(p
        .ansatz
        .iter()
        .all(|x| x.formel.as_deref().unwrap_or("").contains(ANGENOMMEN)));
    let n = |v: &[sk_cost::Befund]| v.iter().filter(|x| x.satz == NICHT_VERMESSEN).count();
    assert_eq!(n(&b.befunde), 1, "{:?}", b.befunde);
    let w = LvWahl {
        los: Guid::from_ifc("1S7bUW001008030000000A").unwrap(),
        untertitel: false,
        preise: false,
        heute: None,
    };
    let lv = lv_aus(&m, &b, &lesen::katalog(&m, None), &w);
    assert_eq!(n(&lv.befunde), 1, "{:?}", lv.befunde);
    assert_eq!(
        lv.kopf.vorbemerkungen.as_deref(),
        Some(vorbemerkung_angenommen("2").as_str())
    );
    assert!(vorbemerkung_angenommen("2")
        .contains("(Höhen nicht vermessen, Dreiecksprismen nach REB-VB 22.013, 2 Dreiecke)"));
}

/// Aufmaß mit Punkten von Hand (I4, VOB-Spruch zu I4): Es bleibt vermessen;
/// die Vorbemerkung nennt ergänzte und nicht verwendete Punkte mit Nummer,
/// Kostenblatt und LV geben den Befund „Höhenaufmaß von Hand geändert“
/// (ohne zu sperren).
#[test]
fn erdmengen_aus_aufmass_mit_punkten_von_hand() {
    use sk_cost::lv::{lv_aus, LvWahl};
    use sk_model::qto_earth::{hand_geaendert, HAND_GEAENDERT};
    let mut z = vec![leistung(
        AUSHUB,
        "Aushub",
        20,
        "unit=m3 basis=auto auto=earth.trench_sand",
    )];
    z.extend(
        [
            "[terrain] src=\"x.csv\" crs=local hsys=DHHN2016 local=1 e0=0 n0=0 rot=0",
            "[tpoint] nr=1 e=-100 n=-100 h=0",
            "[tpoint] nr=2 e=100 n=-100 h=0",
            "[tpoint] nr=3 e=100 n=100 h=0",
            "[tpoint] nr=4 e=-100 n=100 h=0",
            "[tpoint] nr=5 e=0 n=90 h=0 off=1",
            "[tpoint] nr=H1 e=0 n=-90 h=0 src=hand",
        ]
        .map(String::from),
    );
    let m = haus(&z);
    assert!(m.terrain().is_some_and(|t| !t.assumed()));
    assert_eq!(sk_model::qto_earth::netz_geaendert(&m), Some((1, 0, 1)));
    let h = sk_model::qto_earth::netz_herkunft(&m).unwrap();
    assert!(
        h.ends_with(", davon 1 Punkt von Hand ergänzt (H1), davon 1 Punkt nicht verwendet (5)"),
        "{h}"
    );
    let b = blatt(&m);
    let satz = hand_geaendert((1, 0, 1));
    assert!(satz.starts_with(HAND_GEAENDERT));
    let n = |v: &[sk_cost::Befund]| v.iter().filter(|x| x.satz == satz).count();
    assert_eq!(n(&b.befunde), 1, "{:?}", b.befunde);
    let w = LvWahl {
        los: Guid::from_ifc("1S7bUW001008030000000A").unwrap(),
        untertitel: false,
        preise: false,
        heute: None,
    };
    let lv = lv_aus(&m, &b, &lesen::katalog(&m, None), &w);
    assert_eq!(n(&lv.befunde), 1, "{:?}", lv.befunde);
    assert!(lv
        .kopf
        .vorbemerkungen
        .as_deref()
        .is_some_and(|v| v.contains("davon 1 Punkt von Hand ergänzt (H1)")));
}

/// Kopie oder Firmenkatalog von Stand 19 kennt `earth.sand` und
/// `earth.trench_sand` nicht (Review #280 K-280a): Füllsand und Graben im
/// Füllsand stehen als Zeilen ohne Preis mit Warnung 81 und als Fehler im
/// LV, nie still und nie mit dem Werkspreis.
#[test]
fn erdmenge_ohne_bauleistung_im_katalog() {
    use sk_cost::lv::{lv_aus, LvWahl};
    use sk_cost::rechnung::OhneHerkunft;
    const E25: &str = "1S7bUW001008020000001w";
    const E35: &str = "1S7bUW001008020000001x";
    let stand19: String = include_str!("../werk.szk")
        .lines()
        .filter(|l| !l.contains(E25) && !l.contains(E35))
        .map(|l| format!("{l}\n"))
        .collect();
    let lib = sk_model::read_szk_with(&stand19, &lesen::ABSCHNITTE_SZK).unwrap();
    let rh1 = || {
        szo::read_with(RH1, GuidGen::with_seed(1), &lesen::ABSCHNITTE_SZO)
            .unwrap()
            .model
    };
    let firma = rh1();
    let mut kopie = rh1();
    sk_cost::op::kopie_anlegen(&mut kopie, Some(&lib));
    for m in [firma, kopie] {
        let k = lesen::katalog(&m, Some(&lib));
        let b = lesen::kosten(&m, &qto::schedule(&m), &k, &Umfang::projekt());
        // 79.794,22 € mit E25 und E35, ohne sie fehlen genau deren GP
        assert_eq!(b.netto.0, 7_775_521, "{:?}", k.quelle);
        let ohne: Vec<(&str, String)> = b
            .ohne
            .iter()
            .filter_map(|o| match &o.herkunft {
                OhneHerkunft::Automatik { key, .. } => Some((*key, o.menge.text())),
                _ => None,
            })
            .collect();
        assert_eq!(
            ohne,
            [
                ("earth.sand", "47.365".to_string()),
                ("earth.trench_sand", "7.024".to_string())
            ]
        );
        let w81: Vec<&str> = b
            .befunde
            .iter()
            .filter(|f| f.regel == 81)
            .map(|f| f.satz.as_str())
            .collect();
        assert_eq!(w81.len(), 2, "{w81:?}");
        assert!(w81[0].starts_with("Ohne Bauleistung: "), "{w81:?}");
        assert!(
            w81[0].ends_with(
                "47,365 m³, keine Bauleistung mit auto=earth.sand im Katalog, die Menge bleibt ohne Preis"
            ),
            "{w81:?}"
        );
        // im Los der Erdarbeiten, als Fehler: Das LV ist unvollständig
        let fehler: usize = k
            .lose
            .iter()
            .map(|los| {
                let w = LvWahl {
                    los: los.guid,
                    untertitel: false,
                    preise: true,
                    heute: None,
                };
                lv_aus(&m, &b, &k, &w)
                    .befunde
                    .iter()
                    .filter(|f| {
                        f.regel == 81
                            && f.schwere == sk_cost::befund::Schwere::Fehler
                            && f.satz.contains("auto=earth.")
                    })
                    .count()
            })
            .sum();
        assert_eq!(fehler, 2);
    }
}

/// Eine ausgemusterte Bauleistung schaltet ihre Automatikmenge ab und
/// bleibt still (K-280a): keine Zeile ohne Preis, keine Warnung 81.
#[test]
fn ausgemusterte_erdmenge_bleibt_still() {
    use sk_cost::op::{ausfuehren_folge, Herkunft, HerkunftArt, Op, Rolle, SatzId};
    use sk_cost::rechnung::OhneHerkunft;
    let mut m = szo::read_with(RH1, GuidGen::with_seed(1), &lesen::ABSCHNITTE_SZO)
        .unwrap()
        .model;
    let ops = ["1S7bUW001008020000001w", "1S7bUW001008020000001x"].map(|g| Op::Ausmustern {
        satz: SatzId::neu("service", g),
    });
    m.begin("Ausmustern");
    let h = Herkunft::neu(HerkunftArt::Manual, "2026-10-18", "11:00");
    ausfuehren_folge(&mut m, None, Rolle::Nutzer, &h, &ops).expect("ausgemustert");
    m.commit();
    let b = blatt(&m);
    assert!(
        !b.ohne
            .iter()
            .any(|o| matches!(o.herkunft, OhneHerkunft::Automatik { .. })),
        "{:?}",
        b.ohne
    );
    assert!(!b.befunde.iter().any(|f| f.regel == 81), "{:?}", b.befunde);
    assert_eq!(b.netto.0, 7_775_521);
}
