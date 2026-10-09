//! E8c: neue Sätze aus Erweiterungen im Firmenkatalog (stammdaten/
//! verwaltung.md §8a, BIM §3.16 „Zweiter Fall“, Regel 105).

use sk_cost::erweiterung::kennung;
use sk_cost::neue_saetze::{neue_saetze, quelle_text, NeuerSatz, Stand};
use sk_cost::op::{Herkunft, HerkunftArt, Rolle, SatzNeu};
use sk_cost::{entwurf_anwenden, firma_anwenden, lesen, verwaltung, Op};
use sk_model::erweiterung::ExtDef;
use sk_model::{Guid, Library, Model};

const STUETZE: &str = include_str!("../../sk-szb/beispiele/werk.stuetze.szb");

fn hand() -> Herkunft {
    Herkunft::neu(HerkunftArt::Manual, "2026-10-09", "20:30")
}

fn lies(t: &str) -> Library {
    sk_model::read_szk_with(t, &sk_cost::lesen::ABSCHNITTE_SZK).unwrap()
}

fn projekt() -> Model {
    let mut m = Model::with_seed(1);
    m.add_building(1);
    m
}

fn op(saetze: &[NeuerSatz], d: &ExtDef, projekt: Option<(Guid, String)>) -> Op {
    Op::SaetzeAusErweiterung {
        projekt,
        quelle: quelle_text(d),
        saetze: saetze
            .iter()
            .filter(|s| s.stand == Stand::Neu)
            .map(|s| SatzNeu {
                rec: s.rec,
                of: s.guid,
                kurz: s.name.clone(),
                zeilen: s.zeilen.clone(),
            })
            .collect(),
    }
}

fn zeilen<'a>(t: &'a str, anfang: &str) -> Vec<&'a str> {
    t.lines().filter(|l| l.starts_with(anfang)).collect()
}

/// Vorschlag aus einem Projekt in den Entwurf, Freigabe gesperrt,
/// Übernehmen und Ablehnen, danach gilt der Firmensatz im Projekt.
#[test]
fn vorschlag_uebernehmen_ablehnen_freigeben() {
    let m = projekt();
    let d = ExtDef::lesen(STUETZE).unwrap();
    let t0 = sk_model::write_szk(&Library::standard());
    let ns = neue_saetze(&m, &lies(&t0), &d);
    let beton = kennung("werk.stuetze", "leistung", "stuetze_beton");
    let schal = kennung("werk.stuetze", "leistung", "stuetze_schalung");
    let art = kennung("werk.stuetze", "artikel", "stuetzenschalung");
    let satz = |g: Guid| ns.iter().find(|s| s.guid == g).unwrap();
    assert_eq!(ns.len(), 2, "{ns:#?}");
    assert_eq!(satz(beton).stand, Stand::Neu);
    let abschnitte: Vec<&str> = satz(schal).zeilen.iter().map(|z| z.0).collect();
    assert_eq!(
        abschnitte,
        ["service", "svcpart", "article"],
        "Artikel gehört zur Schalung"
    );
    let abschnitte: Vec<&str> = satz(beton).zeilen.iter().map(|z| z.0).collect();
    assert_eq!(
        abschnitte,
        ["service", "svcpart"],
        "Werks-Beton ist kein neuer Satz"
    );

    // Vorschlag eines Nutzers aus Haus A
    let haus = Some((Guid(77), "Haus A".to_string()));
    let o = op(&ns, &d, haus.clone());
    assert!(firma_anwenden(&t0, &t0, Rolle::Nutzer, &hand(), std::slice::from_ref(&o)).is_err());
    let e1 = entwurf_anwenden(&t0, &t0, Rolle::Nutzer, &hand(), &[o])
        .unwrap()
        .text;
    let vorschlaege = zeilen(&e1, "[proposal]");
    assert_eq!(vorschlaege.len(), 2, "{vorschlaege:#?}");
    assert!(vorschlaege
        .iter()
        .all(|l| l.contains("field=*") && !l.contains("old=") && l.contains("project=")));
    let quelle = "source=\"Erweiterung werk.stuetze v1\"";
    let herkunft: Vec<&str> = zeilen(&e1, "[origin]")
        .into_iter()
        .filter(|l| l.contains(quelle))
        .collect();
    assert_eq!(
        herkunft.len(),
        5,
        "2 Leistungen, 2 Anteile, 1 Artikel: {herkunft:#?}"
    );
    assert!(herkunft
        .iter()
        .all(|l| l.contains("kind=import") && l.contains("status=open")));
    let k1 = verwaltung::katalog_von(&lies(&e1), 0);
    assert!(
        k1.befunde
            .iter()
            .all(|b| b.schwere != sk_cost::Schwere::Fehler),
        "{:#?}",
        k1.befunde
    );
    assert!(
        !k1.oz(k1.leistung(beton).unwrap()).is_empty(),
        "Titel Betonarbeiten"
    );
    assert!(k1.vorschlaege.iter().all(|v| v.ganzer_satz()));

    // Freigabe gesperrt, solange einer offen ist
    let e = verwaltung::freigeben(&t0, &e1, &hand()).unwrap_err();
    assert_eq!(e[0].regel, 105);
    assert_eq!(
        e[0].satz,
        "Neue Sätze aus Erweiterungen sind noch nicht übernommen oder abgelehnt."
    );
    // Verwerfen: neue Sätze gehen mit dem Entwurf weg
    assert_eq!(verwaltung::rest_entwurf(&t0, &e1), None);

    // Übernehmen (nur Verwaltung): Beton bestätigt, Vorschlag weg
    let key = |k: &sk_cost::Katalog, g: Guid| {
        k.vorschlaege
            .iter()
            .find(|v| v.of == g.to_ifc())
            .unwrap()
            .key
    };
    let ueber = Op::VorschlagUebernehmen {
        key: key(&k1, beton),
    };
    assert!(entwurf_anwenden(
        &e1,
        &e1,
        Rolle::Nutzer,
        &hand(),
        std::slice::from_ref(&ueber)
    )
    .is_err());
    let e2 = entwurf_anwenden(&e1, &e1, Rolle::Admin, &hand(), &[ueber])
        .unwrap()
        .text;
    let bestaetigt = |t: &str, g: Guid| {
        zeilen(t, "[origin]")
            .iter()
            .any(|l| l.contains(&format!("key={}", g.to_ifc())) && l.contains("status=confirmed"))
    };
    assert!(bestaetigt(&e2, beton));
    assert!(!bestaetigt(&e2, schal));
    assert_eq!(zeilen(&e2, "[proposal]").len(), 1);

    // Ablehnen: Schalung samt Anteil, Artikel und Herkunft weg
    let k2 = verwaltung::katalog_von(&lies(&e2), 0);
    let ab = Op::VorschlagAblehnen {
        key: key(&k2, schal),
    };
    let e3 = entwurf_anwenden(&e2, &e2, Rolle::Admin, &hand(), &[ab])
        .unwrap()
        .text;
    // nur das Protokoll nennt sie noch (Regel 90: nur anhängen)
    let ohne_log: Vec<&str> = e3.lines().filter(|l| !l.starts_with("[log]")).collect();
    for g in [schal, art] {
        assert!(
            ohne_log.iter().all(|l| !l.contains(&g.to_ifc())),
            "{} bleibt",
            g.to_ifc()
        );
    }
    assert!(e3.contains(&beton.to_ifc()));
    assert!(zeilen(&e3, "[proposal]").is_empty());

    // Freigeben; im Projekt gilt danach der Firmensatz, die Schalung kommt
    // weiter aus der Erweiterung
    let f = verwaltung::freigeben(&t0, &e3, &hand()).unwrap();
    let lib = lies(&f.text);
    let mut m2 = projekt();
    m2.put_ext_def(d.clone()).unwrap();
    let k = lesen::firma_oder_werk(&m2, Some(&lib));
    assert!(k.leistung(beton).is_some());
    assert!(
        k.aus_erweiterung("service", beton).is_none(),
        "Firmensatz geht vor"
    );
    assert!(k.aus_erweiterung("service", schal).is_some());
    let ns2 = neue_saetze(&m2, &lib, &d);
    let stand = |g: Guid| ns2.iter().find(|s| s.guid == g).unwrap().stand.clone();
    assert_eq!(stand(beton), Stand::Vorhanden);
    assert_eq!(stand(schal), Stand::Neu);
}

/// Am Einzelplatz und in der Verwaltung ohne Projekt: gewöhnlicher Import
/// mit Pille, ohne `[proposal]`; nie ins Projekt.
#[test]
fn ohne_vorschlagsliste() {
    let m = projekt();
    let d = ExtDef::lesen(STUETZE).unwrap();
    let t0 = sk_model::write_szk(&Library::standard());
    let ns = neue_saetze(&m, &lies(&t0), &d);
    let t1 = firma_anwenden(&t0, &t0, Rolle::Admin, &hand(), &[op(&ns, &d, None)])
        .unwrap()
        .text;
    assert!(zeilen(&t1, "[proposal]").is_empty());
    assert_eq!(
        zeilen(&t1, "[origin]")
            .iter()
            .filter(|l| l.contains("kind=import status=open"))
            .count(),
        5
    );
    let e1 = entwurf_anwenden(&t0, &t0, Rolle::Admin, &hand(), &[op(&ns, &d, None)])
        .unwrap()
        .text;
    assert!(zeilen(&e1, "[proposal]").is_empty());
    // Ins Projekt nie
    let e = sk_cost::pruefen(&m, None, Rolle::Admin, &op(&ns, &d, None)).unwrap_err();
    assert!(e[0].satz.contains("nur in den Firmenkatalog"), "{e:#?}");
}

/// Gleichnamiger Katalogartikel (E8-6): kein neuer Satz, Firmenpreis gilt.
#[test]
fn gleicher_name_ist_vorhanden() {
    let m = projekt();
    let t = STUETZE
        .replace(
            "[leistung] key=stuetze_beton",
            "[artikel] key=beton_c25 name=\"Transportbeton C25/30 XC1-XC2 F3\" einheit=m3 preis=190\n[leistung] key=stuetze_beton",
        )
        .replace("stoffe=\"1S7bUW0010080100000006:1\"", "stoffe=\"beton_c25:1\"");
    let d = ExtDef::lesen(&t).unwrap();
    let ns = neue_saetze(&m, &Library::standard(), &d);
    let a = ns.iter().find(|s| s.key == "beton_c25").unwrap();
    assert!(matches!(&a.stand, Stand::Statt { name } if name.contains("C25/30")));
    assert!(a.stand.text().ends_with("Firmenpreis gilt"));
}

/// Regel 105: ein `field=*`-Vorschlag ohne seinen Satz ist ein Befund, ohne
/// Übernehmen; Ablehnen löscht nur ihn.
#[test]
fn vorschlag_ohne_satz() {
    let m = projekt();
    let d = ExtDef::lesen(STUETZE).unwrap();
    let t0 = sk_model::write_szk(&Library::standard());
    let ns = neue_saetze(&m, &lies(&t0), &d);
    let haus = Some((Guid(77), "Haus A".to_string()));
    let e1 = entwurf_anwenden(&t0, &t0, Rolle::Nutzer, &hand(), &[op(&ns, &d, haus)])
        .unwrap()
        .text;
    let fremd = format!(
        "[proposal] key=9 project={} rec=service of={} field=* new=Weg date=2026-10-09",
        Guid(77).to_ifc(),
        Guid(5).to_ifc()
    );
    let e2 = format!("{e1}{fremd}\n");
    let k = verwaltung::katalog_von(&lies(&e2), 0);
    let b: Vec<&str> = k
        .befunde
        .iter()
        .filter(|b| b.regel == 105)
        .map(|b| b.satz.as_str())
        .collect();
    assert_eq!(b, ["Vorschlag Weg passt zu keinem neuen Satz im Entwurf."]);
    let ueber = Op::VorschlagUebernehmen { key: 9 };
    assert!(entwurf_anwenden(&e2, &e2, Rolle::Admin, &hand(), &[ueber]).is_err());
    let e3 = entwurf_anwenden(
        &e2,
        &e2,
        Rolle::Admin,
        &hand(),
        &[Op::VorschlagAblehnen { key: 9 }],
    )
    .unwrap()
    .text;
    assert!(!e3.contains(&fremd));
    assert_eq!(zeilen(&e3, "[proposal]").len(), 2);
    assert_eq!(zeilen(&e3, "[service]"), zeilen(&e1, "[service]"));
}

/// §8a: Ablehnen lässt eine Zeile stehen, auf die ein anderer offener Satz
/// verweist; sie bleibt `status=open`, bis auch dieser Satz übernommen
/// (dann bestätigt) oder abgelehnt (dann gelöscht) ist. Test zu 61761a6.
#[test]
fn geteilter_artikel_bleibt_offen() {
    let text = STUETZE.replace(
        "stoffe=\"1S7bUW0010080100000006:1\"",
        "stoffe=\"1S7bUW0010080100000006:1; stuetzenschalung:0.1\"",
    );
    assert_ne!(text, STUETZE);
    let d = ExtDef::lesen(&text).unwrap();
    let m = projekt();
    let t0 = sk_model::write_szk(&Library::standard());
    let ns = neue_saetze(&m, &lies(&t0), &d);
    let beton = kennung("werk.stuetze", "leistung", "stuetze_beton");
    let schal = kennung("werk.stuetze", "leistung", "stuetze_schalung");
    let art = kennung("werk.stuetze", "artikel", "stuetzenschalung");
    let haus = Some((Guid(77), "Haus A".to_string()));
    let e1 = entwurf_anwenden(&t0, &t0, Rolle::Nutzer, &hand(), &[op(&ns, &d, haus)])
        .unwrap()
        .text;
    let key = |t: &str, g: Guid| {
        verwaltung::katalog_von(&lies(t), 0)
            .vorschlaege
            .iter()
            .find(|v| v.of == g.to_ifc())
            .unwrap()
            .key
    };
    let anwenden = |t: &str, o: Op| {
        entwurf_anwenden(t, t, Rolle::Admin, &hand(), &[o])
            .unwrap()
            .text
    };
    let status = |t: &str, g: Guid| -> Option<String> {
        zeilen(t, "[origin]")
            .iter()
            .find(|l| l.contains(&format!("key={}", g.to_ifc())))
            .map(|l| {
                ["open", "confirmed"]
                    .into_iter()
                    .find(|s| l.contains(&format!("status={s}")))
                    .unwrap_or("?")
                    .to_string()
            })
    };
    let artikel_da = |t: &str| {
        zeilen(t, "[article]")
            .iter()
            .any(|l| l.contains(&art.to_ifc()))
    };
    assert!(artikel_da(&e1));
    assert_eq!(status(&e1, art).as_deref(), Some("open"));

    // Schalung ablehnen: der Artikel bleibt offen, Beton zeigt auf ihn
    let e2 = anwenden(
        &e1,
        Op::VorschlagAblehnen {
            key: key(&e1, schal),
        },
    );
    assert!(!e2
        .lines()
        .filter(|l| !l.starts_with("[log]"))
        .any(|l| l.starts_with("[service]") && l.contains(&schal.to_ifc())));
    assert!(
        artikel_da(&e2),
        "Artikel gelöscht, obwohl Beton auf ihn zeigt"
    );
    assert_eq!(status(&e2, art).as_deref(), Some("open"));
    assert_eq!(
        verwaltung::freigeben(&t0, &e2, &hand()).unwrap_err()[0].regel,
        105
    );

    // a) Beton übernehmen: der Artikel wird mit bestätigt
    let e3 = anwenden(
        &e2,
        Op::VorschlagUebernehmen {
            key: key(&e2, beton),
        },
    );
    assert_eq!(status(&e3, beton).as_deref(), Some("confirmed"));
    assert_eq!(status(&e3, art).as_deref(), Some("confirmed"));
    assert!(verwaltung::freigeben(&t0, &e3, &hand()).is_ok());

    // b) Beton ablehnen: jetzt geht auch der Artikel
    let e4 = anwenden(
        &e2,
        Op::VorschlagAblehnen {
            key: key(&e2, beton),
        },
    );
    assert!(!artikel_da(&e4));
    assert_eq!(status(&e4, art), None);
    assert!(zeilen(&e4, "[proposal]").is_empty());
    assert!(verwaltung::freigeben(&t0, &e4, &hand()).is_ok());

    // c) andere Reihenfolge: Beton zuerst übernommen, Schalung abgelehnt
    let e5 = anwenden(
        &e1,
        Op::VorschlagUebernehmen {
            key: key(&e1, beton),
        },
    );
    let e6 = anwenden(
        &e5,
        Op::VorschlagAblehnen {
            key: key(&e5, schal),
        },
    );
    assert!(artikel_da(&e6));
    assert_eq!(status(&e6, art).as_deref(), Some("confirmed"));
    assert!(verwaltung::freigeben(&t0, &e6, &hand()).is_ok());
}
