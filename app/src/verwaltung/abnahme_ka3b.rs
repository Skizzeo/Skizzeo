//! Abnahme KA-3b1 bis KA-3b3 (paket-ka3b §5 Nr. 1, 2, 3, 5, 10;
//! Koordinator 20:34) an RH-1 bis RH-3:
//! - Das Kennwort steht nirgends im Klartext: keine Datei im Ordner des
//!   Firmenkatalogs (Firma, Entwurf, Stände, verworfene Entwürfe), kein
//!   `[log]`, keine `Debug`-Ausgabe, kein Fehlerprotokoll, keine Meldung;
//!   der Prüfwert nur in `[catalog] pw`.
//! - „Nur dieses Haus“ geht für einen Platz ohne Kennwort weiter: Preis,
//!   Lohn und die Stunden einer Bauleistung, im Projekt mit Marke.
//! - „Auch für neue Häuser“ mit Kennwort: Firmendatei bytegleich, Wert im
//!   Entwurf, dieses Haus rechnet gleich damit, ein zweiter Platz weiter
//!   mit dem freigegebenen Stand; nach „Freigeben“ Stand + 1, Ablage
//!   bytegleich, Entwurf weg, und dieses Haus folgt späteren
//!   Firmenänderungen über die Abgleichzeile.
//! - Review 3au: „Stunden für dieses und neue Häuser“ mit Kennwort baut
//!   auf dem offenen Entwurf auf; nach „Freigeben“ rechnen dieses und ein
//!   neues Haus gleich (Befund H).
//! - Ein Entwurf auf altem Stand scheitert sichtbar und überschreibt
//!   nichts.

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
        "skizzeo-abnahme-ka3b-{name}-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn h() -> sk_cost::Herkunft {
    sk_cost::Herkunft::neu(sk_cost::HerkunftArt::Manual, "2026-10-08", "20:45")
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

fn stand_von(c: &Company) -> u32 {
    sk_cost::lesen::firma_oder_werk(&sk_model::Model::new(), Some(c.library()))
        .firma_stand
        .unwrap()
}

fn win() -> Win {
    Win {
        w: 1400,
        h: 900,
        top: 40,
        scale: 1.0,
    }
}

fn taste(key: Key) -> Event {
    Event::Key {
        key,
        down: true,
        repeat: false,
        mods: Modifiers::default(),
    }
}

fn tippen(v: &mut Verwaltung, cx: &mut Ctx, text: &str) {
    for ch in text.chars() {
        v.handle(&Event::Text(ch), cx);
    }
}

/// Alle Dateien unter `d` (rekursiv) als Text, verlustbehaftet.
fn alle_dateien(d: &Path) -> Vec<(PathBuf, String)> {
    let mut v = Vec::new();
    let mut offen = vec![d.to_path_buf()];
    while let Some(p) = offen.pop() {
        for e in std::fs::read_dir(&p).unwrap().flatten() {
            let q = e.path();
            if q.is_dir() {
                offen.push(q);
            } else {
                let t = String::from_utf8_lossy(&std::fs::read(&q).unwrap()).into_owned();
                v.push((q, t));
            }
        }
    }
    v
}

/// Firmenkatalog mit Kennwort, gesetzt über das Blatt der Verwaltung wie
/// am Bildschirm.
fn mit_kennwort(d: &Path, s: &mut Scene) -> Company {
    let fonts = super::tests::schriften();
    let mut cx = Ctx {
        fonts: &fonts,
        win: win(),
    };
    let (mut c, _) = Company::laden(&d.join("firmenkatalog.szk"), true);
    c.fuer_firma(&h(), &[lohn(60)]).unwrap();
    let mut v = Verwaltung::open(s, Some(&c), None);
    v.waehlen(Knoten::Kennwort);
    v.aktion(Aktion::Kennwort);
    tippen(&mut v, &mut cx, KENNWORT);
    v.handle(&taste(Key::Tab), &mut cx);
    tippen(&mut v, &mut cx, KENNWORT);
    v.handle(&taste(Key::Enter), &mut cx);
    assert!(v.setz.is_none());
    let ops = v.ops().to_vec();
    assert!(!format!("{ops:?}").contains(KENNWORT), "Debug");
    s.fuer_firma(STEP, &mut c, &h(), &ops)
        .expect("Kennwort gesetzt");
    assert!(sk_cost::verwaltung::hat_kennwort(c.library()));
    c
}

#[test]
fn abnahme_ka3b_kennwort_nirgends_im_klartext() {
    let fonts = super::tests::schriften();
    let t = Theme::dark();
    let w = win();
    let mut cx = Ctx {
        fonts: &fonts,
        win: w,
    };
    let d = ordner("klartext");
    let _ = crate::meldung::protokoll_im_test();
    let (name, text) = haeuser()[0];
    let mut s = szene(text);
    let mut c = mit_kennwort(&d, &mut s);
    let mut meldungen: Vec<String> = Vec::new();
    // Zweiter Platz: zweimal falsch (auch fast richtig), dann richtig
    let mut s2 = szene(text);
    s2.rolle = sk_cost::Rolle::Nutzer;
    let mut v = Verwaltung::open(&s2, Some(&c), None);
    v.sperren();
    for falsch in ["maurer-geheim42", "Maurer-Geheim4"] {
        tippen(&mut v, &mut cx, falsch);
        let out = v.handle(&taste(Key::Enter), &mut cx);
        assert!(!out.frei, "{falsch}");
        let _ = v.paint(&t, &fonts, &w);
        meldungen.extend(v.meldung.clone());
    }
    tippen(&mut v, &mut cx, KENNWORT);
    assert!(v.handle(&taste(Key::Enter), &mut cx).frei);
    // Der Nutzer versucht die Firma: Meldung ohne Kennwort
    let e = s2
        .fuer_firma(STEP, &mut c, &h(), &[lohn(70)])
        .expect_err("Nutzer");
    meldungen.push(e.to_string());
    // Admin: Entwurf, verwerfen, neuer Entwurf, freigeben
    c.fuer_entwurf(&h(), &[lohn(61)]).unwrap();
    c.entwurf_verwerfen().unwrap();
    s.fuer_firma_auch_hier("Lohn", &mut c, &h(), &[lohn(62)])
        .unwrap();
    s.freigeben(FREIGEGEBEN, &mut c, &h()).unwrap();
    // Dateien
    let pw = c
        .library()
        .ext("catalog")
        .next()
        .and_then(|r| {
            r.line
                .split_whitespace()
                .find_map(|f| f.strip_prefix("pw="))
                .map(|p| p.trim_matches('"').to_string())
        })
        .expect("pw");
    let hash = pw.rsplit('$').next().unwrap().to_string();
    assert_eq!(hash.len(), 64);
    let dateien = alle_dateien(&d);
    assert!(
        dateien.len() >= 4,
        "{:?}",
        dateien.iter().map(|x| &x.0).collect::<Vec<_>>()
    );
    for (p, inhalt) in &dateien {
        assert!(!inhalt.contains(KENNWORT), "Klartext in {}", p.display());
        for z in inhalt.lines() {
            if z.contains(&hash) {
                assert!(
                    z.starts_with("[catalog]"),
                    "Prüfwert außerhalb des Kopfs: {z}"
                );
            }
        }
    }
    // Projekt, Fehlerprotokoll, Meldungen, Debug
    for m in [
        sk_model::szo::write(s.model()),
        sk_model::szo::write(s2.model()),
    ] {
        assert!(
            !m.contains(KENNWORT) && !m.contains(&hash),
            "{name}: Projekt"
        );
    }
    let prot = crate::meldung::protokoll_im_test();
    for z in prot.iter().chain(meldungen.iter()) {
        assert!(!z.contains(KENNWORT) && !z.contains(&hash), "{z}");
    }
    let pw_op = Op::KennwortSetzen {
        pw: sk_cost::verwaltung::Pruefwert::neu(KENNWORT, [7; 16]),
    };
    let dbg = format!(
        "{pw_op:?} {:?}",
        sk_cost::verwaltung::Pruefwert::neu(KENNWORT, [7; 16])
    );
    assert!(!dbg.contains(KENNWORT) && !dbg.contains("pbkdf2"), "{dbg}");
    eprintln!(
        "KA3B Klartext: {} Dateien geprüft, {} Meldungen, {} Protokollzeilen",
        dateien.len(),
        meldungen.len(),
        prot.len()
    );
    let _ = std::fs::remove_dir_all(&d);
}

/// paket-ka3b §2 KA-3b1 (Entscheid 19:45) und §5 Nr. 2: Das Kennwort
/// schützt den Firmenkatalog, nicht das Haus. Ein Platz ohne Kennwort
/// setzt „Nur dieses Haus“ Preis, Lohn und die Stunden einer Bauleistung;
/// alle drei stehen im Projekt mit Marke `proj=1`.
#[test]
fn abnahme_ka3b_nutzer_nur_dieses_haus() {
    for (name, text) in haeuser() {
        let d = ordner(&format!("nutzer-{name}"));
        let mut s = szene(text);
        let c = mit_kennwort(&d, &mut s);
        let mut s2 = szene(text);
        s2.rolle = sk_cost::Rolle::Nutzer;
        let vorher = netto(&mut s2, &c);
        let k = s2.katalog(Some((c.library(), c.stand())));
        let art = k
            .artikel
            .iter()
            .find(|a| a.preis.is_some() && !a.retired)
            .unwrap()
            .guid;
        let leistung = k
            .leistungen
            .iter()
            .find(|l| l.stunden != Dez::NULL && !l.retired)
            .unwrap()
            .clone();
        let preis = Op::PreisSetzen {
            artikel: art,
            preis: Some(Dez::ganz(99)),
            stand: "10/2026".into(),
            quelle: "Händler".into(),
            eingabe: String::new(),
        };
        s2.kosten_folge("Preis", Some(c.library()), &h(), &[preis])
            .unwrap_or_else(|b| panic!("{name}: Preis {b:?}"));
        s2.kosten_folge("Lohn", Some(c.library()), &h(), &[lohn(66)])
            .unwrap_or_else(|b| panic!("{name}: Lohn {b:?}"));
        let mut daten = sk_cost::preis::bauleistung(&leistung);
        daten.stunden = Dez(leistung.stunden.0 * 2);
        let stunden = Op::BauleistungAendern {
            bauleistung: leistung.guid,
            daten,
        };
        let r = s2.kosten_folge("Stunden", Some(c.library()), &h(), &[stunden]);
        assert!(
            r.is_ok(),
            "{name}: Stunden „Nur dieses Haus“ abgelehnt: {:?}",
            r.err()
                .map(|b| b.iter().map(|x| x.satz.clone()).collect::<Vec<_>>())
        );
        let marken: Vec<String> = s2
            .model()
            .ext("origin")
            .filter(|r| r.line.contains("proj=1"))
            .filter_map(|r| r.id.clone())
            .collect();
        for id in [art.to_ifc(), "wage".to_string(), leistung.guid.to_ifc()] {
            assert!(marken.contains(&id), "{name}: Marke für {id}: {marken:?}");
        }
        assert_ne!(netto(&mut s2, &c), vorher, "{name}");
        let _ = std::fs::remove_dir_all(&d);
    }
}

#[test]
fn abnahme_ka3b_auch_fuer_neue_haeuser_und_freigeben() {
    for (name, text) in haeuser() {
        let d = ordner(&format!("neue-{name}"));
        let firma = d.join("firmenkatalog.szk");
        let mut s = szene(text);
        let mut c = mit_kennwort(&d, &mut s);
        let stand = stand_von(&c);
        let datei = std::fs::read(&firma).unwrap();
        let vorher = netto(&mut s, &c);
        // Vergleich ohne Kennwort: dasselbe als Einzelplatz
        let d0 = ordner(&format!("neue-ohne-{name}"));
        let (mut c0, _) = Company::laden(&d0.join("firmenkatalog.szk"), true);
        c0.fuer_firma(&h(), &[lohn(60)]).unwrap();
        let mut s0 = szene(text);
        s0.fuer_firma_auch_hier("Lohn", &mut c0, &h(), &[lohn(62)])
            .unwrap();
        let soll62 = netto(&mut s0, &c0);

        // „Auch für neue Häuser“ (Lohnkarte), entsperrt
        let hinweis = s
            .fuer_firma_auch_hier("Lohn 62,00 €/h für neue Häuser", &mut c, &h(), &[lohn(62)])
            .expect("in den Entwurf");
        assert_eq!(
            hinweis.map(|m| m.to_string()).as_deref(),
            Some(crate::catalog::IM_ENTWURF),
            "{name}"
        );
        assert_eq!(
            std::fs::read(&firma).unwrap(),
            datei,
            "{name}: Firma bytegleich"
        );
        let entwurf = std::fs::read_to_string(c.entwurf_pfad()).unwrap();
        assert!(
            entwurf.contains("key=wage num=62") && entwurf.contains("status=draft"),
            "{name}"
        );
        assert_eq!(
            netto(&mut s, &c),
            soll62,
            "{name}: dieses Haus rechnet mit 62"
        );
        assert_ne!(soll62, vorher, "{name}");
        // Zweiter Platz: freigegebener Stand
        let (c2, _) = Company::laden(&firma, false);
        let mut s2 = szene(text);
        s2.rolle = sk_cost::Rolle::Nutzer;
        assert_eq!(netto(&mut s2, &c2), vorher, "{name}: zweiter Platz Lohn 60");

        // Freigeben
        s.freigeben(FREIGEGEBEN, &mut c, &h()).expect("freigegeben");
        assert_eq!(stand_von(&c), stand + 1, "{name}");
        let ablage = d
            .join("firmenkatalog-staende")
            .join(format!("stand-{stand:04}.szk"));
        assert_eq!(
            std::fs::read(&ablage).unwrap(),
            datei,
            "{name}: Ablage bytegleich"
        );
        assert!(!c.entwurf_pfad().exists(), "{name}: Entwurf weg");
        assert_eq!(netto(&mut s, &c), soll62, "{name}");
        let (c2, _) = Company::laden(&firma, false);
        assert_eq!(
            netto(&mut s2, &c2),
            vorher,
            "{name}: Nutzerhaus ohne Kopie folgt erst neu"
        );
        let mut s3 = szene(text);
        assert_eq!(netto(&mut s3, &c2), soll62, "{name}: neues Haus mit 62");

        // Später 64, freigegeben an einem anderen Platz: dieses Haus folgt
        // über die Abgleichzeile
        let (mut b, _) = Company::laden(&firma, false);
        b.fuer_entwurf(&h(), &[lohn(64)]).unwrap();
        b.freigeben(&h()).unwrap();
        c.reload(false);
        let a = sk_cost::abgleich::abgleich(s.model(), Some(c.library())).expect("Abgleich");
        assert!(
            a.saetze.iter().any(|x| x.kennung == "wage"),
            "{name}: {a:?}"
        );
        assert!(a.eigene.is_empty(), "{name}: {:?}", a.eigene);
        s.kosten_folge(
            "Übernommen",
            Some(c.library()),
            &h(),
            &[Op::StandUebernehmen { saetze: a.saetze }],
        )
        .unwrap();
        let lohn_hier = |s: &Scene, c: &Company| {
            sk_cost::lesen::katalog(s.model(), Some(c.library()))
                .werte
                .lohn
        };
        assert_eq!(lohn_hier(&s, &c), Dez::ganz(64), "{name}");
        // Später 66, hier freigegeben: dieses Haus zieht gleich nach
        c.fuer_entwurf(&h(), &[lohn(66)]).unwrap();
        s.freigeben(FREIGEGEBEN, &mut c, &h()).unwrap();
        assert_eq!(lohn_hier(&s, &c), Dez::ganz(66), "{name}");
        eprintln!(
            "KA3B {name}: Lohn 60 {:.2} €, 62 {:.2} €, Stand {stand} → {}",
            vorher as f64 / 100.0,
            soll62 as f64 / 100.0,
            stand_von(&c)
        );
        let _ = std::fs::remove_dir_all(&d);
        let _ = std::fs::remove_dir_all(&d0);
    }
}

/// Ein Entwurf auf altem Stand: Platz B gibt frei, Platz A hält noch den
/// alten Entwurf; ein liegen gebliebener Entwurf (aus einer Sicherung
/// zurückgespielt) auf Stand n wird über Stand n+1 nicht freigegeben.
#[test]
fn abnahme_ka3b_alter_entwurf_scheitert_sichtbar() {
    let (name, text) = haeuser()[1];
    let d = ordner("alt");
    let firma = d.join("firmenkatalog.szk");
    let mut s = szene(text);
    let mut a = mit_kennwort(&d, &mut s);
    a.fuer_entwurf(&h(), &[lohn(62)]).unwrap();
    let alter_entwurf = std::fs::read_to_string(a.entwurf_pfad()).unwrap();
    // Platz B lädt den Entwurf, ändert und gibt frei
    let (mut b, _) = Company::laden(&firma, false);
    b.entwurf_laden();
    b.fuer_entwurf(&h(), &[lohn(63)]).unwrap();
    b.freigeben(&h()).unwrap();
    let freigegeben = std::fs::read(&firma).unwrap();
    // A schreibt in seinen alten Entwurf oder gibt ihn frei: sichtbar
    // abgelehnt, nichts überschrieben
    let e = a.fuer_entwurf(&h(), &[lohn(65)]).expect_err("A Entwurf");
    assert_eq!(e.to_string(), crate::catalog::ENTWURF_GEAENDERT, "{name}");
    assert!(!a.entwurf_pfad().exists());
    let e = a.freigeben(&h()).expect_err("A freigeben");
    eprintln!("KA3B alt: {e}");
    assert_eq!(std::fs::read(&firma).unwrap(), freigegeben, "{name}");
    // Liegen gebliebener Entwurf auf altem Stand
    std::fs::write(a.entwurf_pfad(), &alter_entwurf).unwrap();
    let (mut c, _) = Company::laden(&firma, false);
    c.entwurf_laden();
    let e = c.freigeben(&h()).expect_err("alter Entwurf");
    let t = e.to_string();
    eprintln!("KA3B alt: {t}");
    assert!(t.contains("beruht auf Stand"), "{t}");
    assert_eq!(
        std::fs::read(&firma).unwrap(),
        freigegeben,
        "{name}: bytegleich"
    );
    let k = sk_cost::lesen::firma_oder_werk(&sk_model::Model::new(), Some(c.library()));
    assert_eq!(k.werte.lohn, Dez::ganz(63), "{name}: Bs Stand bleibt");
    let _ = std::fs::remove_dir_all(&d);
}

/// Review 3au (40b4441) an RH-1 bis RH-3: Im Entwurf steht an einer
/// Bauleistung, mit der das Haus rechnet, Gerät 2,00 (Verwaltung). Danach
/// setzt derselbe Admin im Preisblatt die Stunden „für dieses und neue
/// Häuser“. Der Entwurf behält das Gerät, nach „Freigeben“ hat die Firma
/// beides, und ein neues Haus rechnet gleich wie dieses.
#[test]
fn abnahme_ka3b_stunden_neue_haeuser_behaelt_entwurf() {
    for (name, text) in haeuser() {
        let d = ordner(&format!("3au-{name}"));
        let mut s = szene(text);
        let mut c = mit_kennwort(&d, &mut s);
        let vorher = netto(&mut s, &c);
        let blatt = s.kostenblatt(Some((c.library(), c.stand())), &sk_cost::Umfang::projekt());
        let k = s.katalog(Some((c.library(), c.stand())));
        let leistung = blatt
            .positionen
            .iter()
            .filter_map(|z| match z.quelle {
                sk_cost::rechnung::Quelle::Leistung(g) => k.leistung(g),
                _ => None,
            })
            .find(|l| l.stunden != Dez::NULL && l.geraet != Dez::ganz(2))
            .unwrap_or_else(|| panic!("{name}: keine Bauleistung mit Stunden"))
            .clone();
        let g = leistung.guid;
        // Verwaltung: Gerät 2,00 in den Entwurf
        let mut v = Verwaltung::open(&s, Some(&c), None);
        v.waehlen(Knoten::Leistung(g));
        assert!(v.eingeben(&Feld::Geraet, "2"), "{name}");
        assert!(v.entwurf_faellig(), "{name}: {:?}", v.befunde);
        c.fuer_entwurf(&h(), v.ops()).expect("Entwurf geschrieben");
        v.entwurf_gespeichert(&c);
        // Preisblatt: Stunden doppelt, auch für neue Häuser
        let k = s.katalog(Some((c.library(), c.stand())));
        let l = k.leistung(g).unwrap().clone();
        let mut daten = sk_cost::preis::bauleistung(&l);
        let stunden = Dez(leistung.stunden.0 * 2);
        daten.stunden = stunden;
        let hinweis = s
            .fuer_firma_auch_hier(
                "Stunden für neue Häuser",
                &mut c,
                &h(),
                &[Op::BauleistungAendern {
                    bauleistung: g,
                    daten,
                }],
            )
            .unwrap_or_else(|b| panic!("{name}: {b:?}"));
        assert_eq!(
            hinweis.map(|m| m.to_string()).as_deref(),
            Some(crate::catalog::IM_ENTWURF),
            "{name}"
        );
        let im_entwurf = |c: &Company| {
            let e = sk_model::read_szk_with(
                &std::fs::read_to_string(c.entwurf_pfad()).unwrap(),
                &sk_cost::lesen::ABSCHNITTE_SZK,
            )
            .unwrap();
            let e = sk_cost::verwaltung::wie_freigegeben(&e);
            sk_cost::lesen::firma_oder_werk(&sk_model::Model::new(), Some(&e))
                .leistung(g)
                .unwrap()
                .clone()
        };
        let e = im_entwurf(&c);
        assert_eq!(
            (e.stunden, e.geraet),
            (stunden, Dez::ganz(2)),
            "{name}: Entwurf behält Gerät"
        );
        // Freigeben: Firma hat beides, neues Haus rechnet wie dieses
        s.freigeben(FREIGEGEBEN, &mut c, &h()).expect("freigegeben");
        let f = sk_cost::lesen::firma_oder_werk(&sk_model::Model::new(), Some(c.library()))
            .leistung(g)
            .unwrap()
            .clone();
        assert_eq!(
            (f.stunden, f.geraet),
            (stunden, Dez::ganz(2)),
            "{name}: Firma nach Freigeben"
        );
        // Dieses Haus folgt der eigenen Freigabe: keine Marke proj=1 mehr an
        // der Bauleistung, Gerät 2,00 wie in der Firma (Befund H)
        let lh = s
            .katalog(Some((c.library(), c.stand())))
            .leistung(g)
            .unwrap()
            .clone();
        assert_eq!(
            (lh.stunden, lh.geraet),
            (stunden, Dez::ganz(2)),
            "{name}: dieses Haus nach Freigeben"
        );
        let hier = netto(&mut s, &c);
        let (c2, _) = Company::laden(&d.join("firmenkatalog.szk"), false);
        let mut s3 = szene(text);
        assert_eq!(netto(&mut s3, &c2), hier, "{name}: neues Haus");
        assert_ne!(hier, vorher, "{name}");
        eprintln!(
            "KA3B-3au {name}: {} Stunden {} → {}, Gerät 2,00, netto {:.2} → {:.2} €",
            leistung.kurz,
            leistung.stunden.text(),
            stunden.text(),
            vorher as f64 / 100.0,
            hier as f64 / 100.0
        );
        let _ = std::fs::remove_dir_all(&d);
    }
}
