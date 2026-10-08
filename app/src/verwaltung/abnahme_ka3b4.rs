//! Abnahme KA-3b4 Vorschläge (paket-ka3b §5 Nr. 8 und 8a; Koordinator
//! 21:39) an RH-1 bis RH-3:
//! - Ein Platz ohne Kennwort schlägt Lohn 65,00, einen Preis und die
//!   Stunden einer Bauleistung vor: dieses Haus rechnet sofort damit wie
//!   mit „Nur dieses Haus“, die Firmendatei bleibt bytegleich, im Entwurf
//!   stehen drei `[proposal]` neben der Änderung des Admins, ein zweiter
//!   Platz rechnet weiter mit dem freigegebenen Stand.
//! - Die Verwaltung zeigt die drei mit alt und neu; „Übernehmen“ schreibt genau
//!   den vorgeschlagenen Wert in den Entwurf (Quelle „Vorschlag aus …“, je
//!   eine `[log]`-Zeile) und behält die Änderung des Admins an derselben
//!   Bauleistung; „Ablehnen“ streicht den Vorschlag, die Firma und das
//!   vorschlagende Haus bleiben, wie sie sind.
//! - Ohne „Freigeben“ ändert sich die Firma nicht; danach rechnet ein neues
//!   Haus mit den übernommenen Werten, ohne den abgelehnten.
//! - Entwurf verwerfen (8a): Archiv, Firma bytegleich, offene Vorschläge
//!   bleiben.

use super::*;
use std::path::{Path, PathBuf};

const KENNWORT: &str = "Maurer-Geheim42";

fn haeuser() -> [(&'static str, &'static str); 3] {
    [
        (
            "RH-1",
            include_str!("../../../crates/sk-cost/referenz/rh1-standardhaus.szo"),
        ),
        (
            "RH-2",
            include_str!("../../../crates/sk-cost/referenz/rh2-mehrschalig.szo"),
        ),
        (
            "RH-3",
            include_str!("../../../crates/sk-cost/referenz/rh3-versatz-dachterrasse.szo"),
        ),
    ]
}

fn szene(text: &str) -> Scene {
    let m = sk_model::szo::read_with(
        text,
        sk_model::GuidGen::with_seed(1),
        &sk_cost::lesen::ABSCHNITTE_SZO,
    )
    .expect("lädt")
    .model;
    Scene::with_model(m)
}

fn ordner(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!(
        "skizzeo-abnahme-ka3b4-{name}-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn h() -> sk_cost::Herkunft {
    sk_cost::Herkunft::neu(sk_cost::HerkunftArt::Manual, "2026-10-08", "21:50")
}

fn lohn(w: i64) -> Op {
    Op::FirmenwertSetzen {
        schluessel: "wage".into(),
        wert: Dez::ganz(w),
    }
}

fn netto(s: &mut Scene, c: &Company) -> i64 {
    s.kostenblatt(Some((c.library(), c.stand())), &sk_cost::Umfang::projekt())
        .netto
        .0
}

fn firma_katalog(lib: &Library) -> sk_cost::Katalog {
    sk_cost::lesen::firma_oder_werk(&sk_model::Model::new(), Some(lib))
}

fn entwurf_katalog(c: &Company) -> sk_cost::Katalog {
    let e = sk_model::read_szk_with(
        &std::fs::read_to_string(c.entwurf_pfad()).unwrap(),
        &sk_cost::lesen::ABSCHNITTE_SZK,
    )
    .unwrap();
    firma_katalog(&sk_cost::verwaltung::wie_freigegeben(&e))
}

fn taste(key: Key) -> Event {
    Event::Key {
        key,
        down: true,
        repeat: false,
        mods: Modifiers::default(),
    }
}

/// Firmenkatalog mit Kennwort (über das Blatt) und Lohn 60.
fn mit_kennwort(d: &Path, s: &mut Scene) -> Company {
    let fonts = super::tests::schriften();
    let mut cx = Ctx {
        fonts: &fonts,
        win: Win {
            w: 1400,
            h: 900,
            top: 40,
            scale: 1.0,
        },
    };
    let (mut c, _) = Company::laden(&d.join("firmenkatalog.szk"), true);
    c.fuer_firma(&h(), &[lohn(60)]).unwrap();
    let mut v = Verwaltung::open(s, Some(&c), None);
    v.waehlen(Knoten::Kennwort);
    v.aktion(Aktion::Kennwort);
    for ende in [Key::Tab, Key::Enter] {
        for ch in KENNWORT.chars() {
            v.handle(&Event::Text(ch), &mut cx);
        }
        v.handle(&taste(ende), &mut cx);
    }
    let ops = v.ops().to_vec();
    s.fuer_firma(STEP, &mut c, &h(), &ops)
        .expect("Kennwort gesetzt");
    assert!(sk_cost::verwaltung::hat_kennwort(c.library()));
    c
}

/// Was die App bei `Out::entwurf` tut.
fn schreiben(v: &mut Verwaltung, c: &mut Company) {
    assert!(v.entwurf_faellig(), "{:?}", v.befunde);
    c.fuer_entwurf(&h(), v.ops()).expect("Entwurf geschrieben");
    v.entwurf_gespeichert(c);
}

#[test]
fn abnahme_ka3b4_vorschlaege() {
    for (name, text) in haeuser() {
        let d = ordner(name);
        let firma = d.join("firmenkatalog.szk");
        let mut s = szene(text);
        let mut c = mit_kennwort(&d, &mut s);
        let stand_alt = c.stand();
        // Was das Haus nutzt: ein Artikel je m² mit Preis, eine Bauleistung
        // mit Stunden
        let k = s.katalog(Some((c.library(), c.stand())));
        let blatt = s.kostenblatt(Some((c.library(), c.stand())), &sk_cost::Umfang::projekt());
        let mut leistung = None;
        let mut artikel = None;
        for z in &blatt.positionen {
            let sk_cost::rechnung::Quelle::Leistung(g) = z.quelle else {
                continue;
            };
            let l = k.leistung(g).unwrap();
            if leistung.is_none() && l.stunden != Dez::NULL && l.geraet != Dez::ganz(2) {
                leistung = Some(l.clone());
            }
            for a in k
                .anteile
                .iter()
                .filter(|a| a.leistung == g)
                .filter_map(|a| a.artikel)
                .filter_map(|a| k.artikel(a))
            {
                if artikel.is_none() && a.preis.is_some() && a.einheit == Einheit::M2 {
                    artikel = Some(a.clone());
                }
            }
        }
        let (l, a) = (leistung.unwrap(), artikel.unwrap());
        let (g, art) = (l.guid, a.guid);
        let stunden = Dez(l.stunden.0 * 2);
        let preis = Dez::ganz(99);
        let vorher = netto(&mut s, &c);

        // Admin: Gerät 2,00 an derselben Bauleistung im Entwurf
        let mut v = Verwaltung::open(&s, Some(&c), None);
        v.waehlen(Knoten::Leistung(g));
        assert!(v.eingeben(&Feld::Geraet, "2"), "{name}");
        schreiben(&mut v, &mut c);
        let datei = std::fs::read(&firma).unwrap();

        // Platz ohne Kennwort schlägt Lohn, Preis und Stunden vor
        let (mut cn, _) = Company::laden(&firma, false);
        cn.set_nutzer(true);
        let mut sn = szene(text);
        sn.rolle = sk_cost::Rolle::Nutzer;
        let kn = sn.katalog(Some((cn.library(), cn.stand())));
        let mut daten = sk_cost::preis::bauleistung(kn.leistung(g).unwrap());
        daten.stunden = stunden;
        let ops = [
            lohn(65),
            Op::PreisSetzen {
                artikel: art,
                preis: Some(preis),
                stand: "10/2026".into(),
                quelle: "Händler Müller".into(),
                eingabe: String::new(),
            },
            Op::BauleistungAendern {
                bauleistung: g,
                daten,
            },
        ];
        let m = sn
            .der_firma_vorschlagen("Vorschlag", &mut cn, &h(), &ops, "Haus Becker")
            .unwrap_or_else(|m| panic!("{name}: {m}"));
        assert_eq!(m.to_string(), crate::catalog::VORGESCHLAGEN, "{name}");
        // Gilt sofort im Haus, wie „Nur dieses Haus“
        let mut s0 = szene(text);
        s0.rolle = sk_cost::Rolle::Nutzer;
        s0.kosten_folge("Nur hier", Some(cn.library()), &h(), &ops)
            .unwrap();
        let hier = netto(&mut sn, &cn);
        assert_eq!(hier, netto(&mut s0, &cn), "{name}: wie Nur dieses Haus");
        assert_ne!(hier, vorher, "{name}");
        // Firma bytegleich, Entwurf mit drei Vorschlägen und dem Gerät
        assert_eq!(std::fs::read(&firma).unwrap(), datei, "{name}: Firma");
        let e = std::fs::read_to_string(c.entwurf_pfad()).unwrap();
        assert_eq!(e.matches("[proposal]").count(), 3, "{name}: {e}");
        let ek = entwurf_katalog(&c);
        assert_eq!(ek.leistung(g).unwrap().geraet, Dez::ganz(2), "{name}");
        assert_eq!(ek.werte.lohn, Dez::ganz(60), "{name}: Entwurf noch 60");
        // Zweiter Platz: freigegebener Stand
        let (c2, _) = Company::laden(&firma, false);
        let mut s2 = szene(text);
        s2.rolle = sk_cost::Rolle::Nutzer;
        assert_eq!(netto(&mut s2, &c2), vorher, "{name}: zweiter Platz");

        // Admin: drei Vorschläge mit Einheit
        c.entwurf_laden();
        let mut v = Verwaltung::open(&s, Some(&c), None);
        assert_eq!(v.vorschlaege.len(), 3, "{name}: {:?}", v.vorschlaege);
        assert_eq!(v.entwurf_anzahl(), 1, "{name}: nur das Gerät zählt");
        let vor = |v: &Verwaltung, rec: &str| {
            v.vorschlaege
                .iter()
                .find(|x| x.rec == rec)
                .cloned()
                .unwrap_or_else(|| panic!("{name}: {rec}"))
        };
        // Werte genau wie vorgeschlagen, in der Einheit des Satzes
        let wert = |v: &Verwaltung, rec: &str| {
            let x = vor(v, rec);
            (
                x.alt.as_deref().and_then(|t| Dez::lesen(t, 6)),
                Dez::lesen(&x.neu, 6),
                x.name.clone(),
            )
        };
        assert_eq!(
            wert(&v, "rate"),
            (
                Some(Dez::ganz(60)),
                Some(Dez::ganz(65)),
                "Haus Becker".into()
            ),
            "{name}"
        );
        assert_eq!(
            wert(&v, "article"),
            (a.preis, Some(preis), "Haus Becker".into()),
            "{name}"
        );
        assert_eq!(
            wert(&v, "service"),
            (Some(l.stunden), Some(stunden), "Haus Becker".into()),
            "{name}"
        );
        // Lohn und Stunden übernehmen, Preis ablehnen
        for rec in ["rate", "service"] {
            let key = vor(&v, rec).key;
            v.aktion(Aktion::VorschlagUebernehmen(key));
            schreiben(&mut v, &mut c);
        }
        let key = vor(&v, "article").key;
        v.aktion(Aktion::VorschlagAblehnen(key));
        schreiben(&mut v, &mut c);
        assert!(v.vorschlaege.is_empty(), "{name}");
        let e = std::fs::read_to_string(c.entwurf_pfad()).unwrap();
        assert_eq!(e.matches("op=vorschlag_uebernehmen").count(), 2, "{name}");
        assert_eq!(e.matches("op=vorschlag_ablehnen").count(), 1, "{name}");
        assert!(e.contains("Vorschlag aus Haus Becker"), "{name}: {e}");
        let ek = entwurf_katalog(&c);
        assert_eq!(ek.werte.lohn, Dez::ganz(65), "{name}");
        let el = ek.leistung(g).unwrap();
        assert_eq!(
            (el.stunden, el.geraet),
            (stunden, Dez::ganz(2)),
            "{name}: Stunden übernommen, Gerät bleibt"
        );
        assert_eq!(ek.artikel(art).unwrap().preis, a.preis, "{name}: abgelehnt");
        // Ohne Freigeben keine Firmenänderung, das Haus bleibt
        assert_eq!(std::fs::read(&firma).unwrap(), datei, "{name}: Firma");
        assert_eq!(c.stand(), stand_alt, "{name}");
        cn.reload(false);
        assert_eq!(netto(&mut sn, &cn), hier, "{name}: Haus nach Ablehnen");

        // Freigeben: neues Haus mit Lohn 65, Stunden und Gerät, alter Preis
        s.freigeben(FREIGEGEBEN, &mut c, &h()).expect("freigegeben");
        assert!(!c.entwurf_pfad().exists(), "{name}: kein Rest-Entwurf");
        let fk = firma_katalog(c.library());
        assert_eq!(fk.werte.lohn, Dez::ganz(65), "{name}");
        assert_eq!(fk.artikel(art).unwrap().preis, a.preis, "{name}");
        let (c3, _) = Company::laden(&firma, false);
        let mut s3 = szene(text);
        let neu = netto(&mut s3, &c3);
        let mut daten = sk_cost::preis::bauleistung(&l);
        (daten.stunden, daten.geraet) = (stunden, Dez::ganz(2));
        let mut s4 = szene(text);
        s4.kosten_folge(
            "Soll",
            Some(c2.library()),
            &h(),
            &[
                lohn(65),
                Op::BauleistungAendern {
                    bauleistung: g,
                    daten,
                },
            ],
        )
        .unwrap();
        assert_eq!(neu, netto(&mut s4, &c2), "{name}: neues Haus");
        // Das vorschlagende Haus behält seinen Preis
        cn.reload(false);
        let kn = sn.katalog(Some((cn.library(), cn.stand())));
        assert_eq!(kn.artikel(art).unwrap().preis, Some(preis), "{name}");

        // 8a: offener Vorschlag überlebt „Entwurf verwerfen“
        sn.der_firma_vorschlagen("Vorschlag", &mut cn, &h(), &[lohn(70)], "Haus Becker")
            .unwrap();
        c.entwurf_laden();
        c.fuer_entwurf(&h(), &[lohn(66)]).unwrap();
        let datei = std::fs::read(&firma).unwrap();
        c.entwurf_verwerfen().expect("verworfen");
        assert_eq!(std::fs::read(&firma).unwrap(), datei, "{name}: 8a Firma");
        let e = std::fs::read_to_string(c.entwurf_pfad()).unwrap();
        assert!(
            e.contains("new=\"70\"") && !e.contains("num=66"),
            "{name}: {e}"
        );
        let archiv = std::fs::read_dir(d.join("firmenkatalog-staende"))
            .unwrap()
            .flatten()
            .any(|x| {
                x.file_name()
                    .to_string_lossy()
                    .starts_with("entwurf-verworfen")
            });
        assert!(archiv, "{name}: Archiv");
        eprintln!(
            "KA3B4 {name}: vorher {:.2} €, Vorschlag hier {:.2} €, neues Haus {:.2} €",
            vorher as f64 / 100.0,
            hier as f64 / 100.0,
            neu as f64 / 100.0
        );
        let _ = std::fs::remove_dir_all(&d);
    }
}
