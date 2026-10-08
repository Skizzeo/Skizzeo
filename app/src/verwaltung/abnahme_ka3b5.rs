//! Abnahme KA-3b5 Händlerpreis im Reiter Kosten (ea173ba; Koordinator
//! 22:21) an RH-1 bis RH-3, an einem Platz ohne Kennwort:
//! - „Händlerpreis für dieses Haus eintragen“ mit 95 €/m³ an einem Stein in
//!   m² mit Dicke und mit 0,85 €/St an einem Stein mit Steinzahl ergibt im
//!   Haus denselben m²-Preis wie von Hand umgerechnet (wie KA-3a7) und
//!   dieselben Kosten wie „Nur dieses Haus“ mit diesem Preis.
//! - Der Preis steht im Haus mit Marke `proj=1`, die Firmendatei bleibt
//!   bytegleich; ein Rückgängig-Schritt nimmt ihn ganz zurück,
//!   Wiederholen bringt ihn wieder.
//! - Review 3ax: Ein gesperrter `check`-Schritt lässt das Haus bytegleich
//!   und den Rückgängig-Stapel wie vorher.

use super::*;
use std::path::PathBuf;

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
        "skizzeo-abnahme-ka3b5-{name}-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn h() -> sk_cost::Herkunft {
    sk_cost::Herkunft::neu(sk_cost::HerkunftArt::Manual, "2026-10-08", "22:30")
}

fn netto(s: &mut Scene, c: &Company) -> i64 {
    s.kostenblatt(Some((c.library(), c.stand())), &sk_cost::Umfang::projekt())
        .netto
        .0
}

/// Händlerpreis-Ablauf am Platz ohne Kennwort: Artikel, Preis in Einheit
/// `per` (Index in `per=` des Schritts), Quelle; die Ops für das Haus.
fn haendlerpreis(
    s: &Scene,
    c: &Company,
    a: &sk_cost::katalog::Artikel,
    per: usize,
    preis: &str,
) -> Vec<Op> {
    let mut v = Verwaltung::open(s, Some(c), None);
    let haus = v.haus_ablaeufe();
    assert_eq!(haus.len(), 1, "{haus:?}");
    assert!(v.ablauf_im_haus(haus[0].0));
    assert!(v.abfrage.is_none(), "ohne Kennwort");
    v.assistent.as_mut().unwrap().te[0] = TextEdit::new(&a.name);
    let opts = v.a_optionen_jetzt();
    let i = opts.iter().position(|(g, _)| *g == a.guid).expect(&a.name);
    v.assistent.as_mut().unwrap().oben = i;
    v.a_klick(Some(assistent::AZiel::Option(0)), true);
    v.ablauf_weiter();
    assert_eq!(v.assistent.as_ref().unwrap().seite, 1, "{}", a.name);
    v.a_klick(Some(assistent::AZiel::Einheit(per)), true);
    assert!(
        v.assistent.as_ref().unwrap().fehler.is_none(),
        "{}: {:?}",
        a.name,
        v.assistent.as_ref().unwrap().fehler
    );
    v.a_tippen(0, preis);
    v.ablauf_weiter();
    v.a_tippen(0, "Angebot Müller");
    v.ablauf_weiter();
    let fh = v.fuer_haus.take().expect("an die App");
    assert!(v.ops().is_empty(), "nichts für die Firma");
    fh.ops
}

#[test]
fn abnahme_ka3b5_haendlerpreis_im_haus() {
    for (name, text) in haeuser() {
        let d = ordner(name);
        let firma = d.join("firmenkatalog.szk");
        let (mut c, _) = Company::laden(&firma, true);
        let mut s = szene(text);
        let k = Op::KennwortSetzen {
            pw: sk_cost::verwaltung::Pruefwert::neu("Polier7", [5; 16]),
        };
        s.fuer_firma(STEP, &mut c, &h(), &[k]).unwrap();
        let (c, _) = Company::laden(&firma, false);
        let datei = std::fs::read(&firma).unwrap();
        let mut s = szene(text);
        s.rolle = sk_cost::Rolle::Nutzer;
        // Steine, mit denen das Haus rechnet
        let kat = s.katalog(Some((c.library(), c.stand())));
        let blatt = s.kostenblatt(Some((c.library(), c.stand())), &sk_cost::Umfang::projekt());
        let mut genutzt: Vec<sk_cost::katalog::Artikel> = Vec::new();
        for z in &blatt.positionen {
            let sk_cost::rechnung::Quelle::Leistung(g) = z.quelle else {
                continue;
            };
            for a in kat
                .anteile
                .iter()
                .filter(|x| x.leistung == g)
                .filter_map(|x| x.artikel)
                .filter_map(|x| kat.artikel(x))
            {
                if a.preis.is_some()
                    && a.einheit == Einheit::M2
                    && !genutzt.iter().any(|x| x.guid == a.guid)
                {
                    genutzt.push(a.clone());
                }
            }
        }
        // je m³: Stein mit Dicke; je Stück: Stein mit Steinzahl
        let m3 = genutzt
            .iter()
            .find(|a| a.t.is_some_and(|t| t > Dez::NULL))
            .unwrap_or_else(|| panic!("{name}: kein Stein mit Dicke"))
            .clone();
        let st = genutzt
            .iter()
            .find(|a| a.conv.is_some() && a.guid != m3.guid)
            .or_else(|| genutzt.iter().find(|a| a.conv.is_some()))
            .cloned();
        let mut faelle = vec![(
            m3.clone(),
            1usize,
            "95",
            Dez((95i128 * m3.t.unwrap().0 as i128 / 1000) as i64),
        )];
        if let Some(a) = st {
            let x = 85i128 * a.conv.unwrap().0 as i128 / 100;
            faelle.push((a, 2, "0,85", Dez(((x + 50) / 100 * 100) as i64)));
        }
        for (a, per, eingabe, soll) in faelle {
            let vorher = netto(&mut s, &c);
            let preis_vorher = s
                .katalog(Some((c.library(), c.stand())))
                .artikel(a.guid)
                .unwrap()
                .preis;
            let ops = haendlerpreis(&s, &c, &a, per, eingabe);
            s.kosten_folge(
                "Händlerpreis für dieses Haus eintragen",
                Some(c.library()),
                &h(),
                &ops,
            )
            .unwrap_or_else(|b| panic!("{name}: {b:?}"));
            let preis_hier = |s: &mut Scene| {
                s.katalog(Some((c.library(), c.stand())))
                    .artikel(a.guid)
                    .unwrap()
                    .preis
            };
            assert_eq!(
                preis_hier(&mut s),
                Some(soll),
                "{name}: {} {eingabe} je {per}",
                a.name
            );
            let marke = s
                .model()
                .ext("origin")
                .any(|r| r.id.as_deref() == Some(&a.guid.to_ifc()) && r.line.contains("proj=1"));
            assert!(marke, "{name}: Marke proj=1 für {}", a.name);
            assert_eq!(
                std::fs::read(&firma).unwrap(),
                datei,
                "{name}: Firma bytegleich"
            );
            let nachher = netto(&mut s, &c);
            // Wie „Nur dieses Haus“ mit dem umgerechneten Preis
            let mut s0 = szene(text);
            s0.rolle = sk_cost::Rolle::Nutzer;
            s0.kosten_folge(
                "Nur hier",
                Some(c.library()),
                &h(),
                &[Op::PreisSetzen {
                    artikel: a.guid,
                    preis: Some(soll),
                    stand: "10/2026".into(),
                    quelle: "Angebot Müller".into(),
                    eingabe: String::new(),
                }],
            )
            .unwrap();
            if per == 1 {
                // erster Fall: s0 hat sonst nichts geändert
                assert_eq!(nachher, netto(&mut s0, &c), "{name}: wie Nur dieses Haus");
            }
            // Ein Rückgängig-Schritt
            assert_eq!(
                s.undo_label(),
                Some("Händlerpreis für dieses Haus eintragen"),
                "{name}"
            );
            assert!(s.undo());
            assert_eq!(preis_hier(&mut s), preis_vorher, "{name}: rückgängig");
            assert_eq!(netto(&mut s, &c), vorher, "{name}: rückgängig");
            assert!(s.redo());
            assert_eq!(netto(&mut s, &c), nachher, "{name}: wiederholt");
            eprintln!(
                "KA3B5 {name}: {} {eingabe} (Einheit {per}) → {} €/m², netto {:.2} → {:.2} €",
                a.name,
                soll.text(),
                vorher as f64 / 100.0,
                nachher as f64 / 100.0
            );
        }
        let _ = std::fs::remove_dir_all(&d);
    }
}

/// Review 3ax an RH-1 bis RH-3: Ein `check`-Schritt (Regel 77, zweiter
/// Standardartikel zu Baustoff und Dicke) sperrt den Ablauf für dieses
/// Haus. Danach ist das Haus bytegleich, die Kosten gleich, kein Schritt
/// offen und der Rückgängig-Stapel wie vorher: der letzte Schritt ist der
/// eigene Preis von vorher, Rückgängig nimmt ihn, Wiederholen bringt ihn.
/// Ohne Sperre schreibt derselbe Weg genau einen Rückgängig-Schritt.
#[test]
fn abnahme_ka3b5_gesperrte_pruefung_laesst_haus() {
    for (name, text) in haeuser() {
        let d = ordner(&format!("3ax-{name}"));
        let firma = d.join("firmenkatalog.szk");
        let (mut c, _) = Company::laden(&firma, true);
        let mut s = szene(text);
        let k = Op::KennwortSetzen {
            pw: sk_cost::verwaltung::Pruefwert::neu("Polier7", [5; 16]),
        };
        s.fuer_firma(STEP, &mut c, &h(), &[k]).unwrap();
        let (c, _) = Company::laden(&firma, false);
        let mut s = szene(text);
        s.rolle = sk_cost::Rolle::Nutzer;
        let kat = s.katalog(Some((c.library(), c.stand())));
        let a = kat
            .artikel
            .iter()
            .find(|a| a.std && a.mat.is_some() && a.t.is_some() && !a.retired && a.preis.is_some())
            .unwrap_or_else(|| panic!("{name}: kein Standardartikel"))
            .clone();
        // Vorher: eigener Preis als letzter Rückgängig-Schritt
        let preis = |p: i64| Op::PreisSetzen {
            artikel: a.guid,
            preis: Some(Dez::ganz(p)),
            stand: "10/2026".into(),
            quelle: "Angebot Müller".into(),
            eingabe: String::new(),
        };
        s.kosten_folge("Preis vorher", Some(c.library()), &h(), &[preis(41)])
            .unwrap();
        let bild = sk_model::szo::write(s.model());
        let kosten = netto(&mut s, &c);
        let zweiter = Op::ArtikelAnlegen {
            baustoff: a.mat,
            name: format!("{} Händler", a.name),
            dicke: a.t,
            guete: String::new(),
            format: String::new(),
            einheit: a.einheit,
            preis: Some(Dez::ganz(1)),
            stand: "10/2026".into(),
            quelle: String::new(),
            lieferant: String::new(),
            standard: true,
        };
        // Wie die App: erst prüfen, gesperrt heißt nichts eintragen
        let satz = s
            .ablauf_pruefen(
                Some(c.library()),
                &h(),
                std::slice::from_ref(&zweiter),
                &[77],
            )
            .unwrap_or_else(|| panic!("{name}: Regel 77 sperrt nicht"));
        assert!(satz.contains("mehrere Standardartikel"), "{name}: {satz}");
        assert_eq!(
            sk_model::szo::write(s.model()),
            bild,
            "{name}: Haus bytegleich"
        );
        assert_eq!(netto(&mut s, &c), kosten, "{name}: Kosten gleich");
        assert!(!s.model().in_step(), "{name}: kein Schritt offen");
        assert_eq!(s.undo_label(), Some("Preis vorher"), "{name}: Stapel");
        assert!(s.undo());
        assert_eq!(
            s.katalog(Some((c.library(), c.stand())))
                .artikel(a.guid)
                .unwrap()
                .preis,
            a.preis,
            "{name}: Rückgängig nimmt den Preis von vorher"
        );
        assert!(s.redo());
        assert_eq!(sk_model::szo::write(s.model()), bild, "{name}: Wiederholen");
        // Ohne Sperre: ein Schritt, ganz rückgängig zu machen
        assert_eq!(
            s.ablauf_pruefen(Some(c.library()), &h(), &[preis(43)], &[77]),
            None,
            "{name}"
        );
        assert_eq!(
            sk_model::szo::write(s.model()),
            bild,
            "{name}: Prüfen allein"
        );
        s.kosten_folge(
            "Händlerpreis für dieses Haus eintragen",
            Some(c.library()),
            &h(),
            &[preis(43)],
        )
        .unwrap();
        assert_eq!(
            s.undo_label(),
            Some("Händlerpreis für dieses Haus eintragen"),
            "{name}"
        );
        assert!(s.undo());
        assert_eq!(sk_model::szo::write(s.model()), bild, "{name}: ein Schritt");
        assert_eq!(s.undo_label(), Some("Preis vorher"), "{name}");
        eprintln!("KA3B5-3ax {name}: {} gesperrt: {satz}", a.name);
        let _ = std::fs::remove_dir_all(&d);
    }
}
