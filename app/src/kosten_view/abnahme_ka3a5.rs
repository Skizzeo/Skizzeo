//! Abnahme KA-3a5 „Unterschiede ansehen“ (Koordinator 19:07) an RH-1 bis
//! RH-3, über die Abgleichzeile des Reiters Kosten:
//! - Das Blatt nennt die Abweichungen (gehen mit „übernehmen“) und den
//!   eigenen Wert dieses Hauses (bleibt, Regel 89).
//! - „n Einträge gleich“ zählt richtig: ein weiterer abweichender Satz
//!   zählt eins weniger, ein Satz, der wieder gleich ist, eins mehr; eine
//!   Firmenänderung an einem eigenen Wert ändert die Zahl nicht.
//! - „so lassen“ und „übernehmen“ wirken bei offenem Blatt und schließen
//!   es; nach „übernehmen“ hat das Haus die Firmenwerte und behält seinen
//!   eigenen Preis.

use super::*;

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

fn preis(artikel: sk_model::Guid, p: i64) -> Op {
    Op::PreisSetzen {
        artikel,
        preis: Some(Dez::ganz(p)),
        stand: "10/2026".into(),
        quelle: "Abnahme".into(),
        eingabe: String::new(),
    }
}

fn lohn(w: i64) -> Op {
    Op::FirmenwertSetzen {
        schluessel: "wage".into(),
        wert: Dez::ganz(w),
    }
}

fn abgleich(s: &Scene, c: &crate::catalog::Company) -> sk_cost::abgleich::Abgleich {
    sk_cost::abgleich::abgleich(s.model(), Some(c.library())).expect("Abgleichzeile")
}

fn mitte(r: (f32, f32, f32, f32)) -> (f64, f64) {
    ((r.0 + r.2 * 0.5) as f64, (r.1 + r.3 * 0.5) as f64)
}

#[test]
fn abnahme_ka3a5_unterschiede_ansehen() {
    let t = Theme::dark();
    let fonts = Fonts {
        regular: None,
        bold: None,
        italic: None,
    };
    let mods = sk_platform::Modifiers::default();
    for (name, text) in haeuser() {
        let d = std::env::temp_dir().join(format!(
            "skizzeo-abnahme-ka3a5-{name}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&d);
        let (mut c, _) = crate::catalog::Company::load(&d.join("firmenkatalog.szk"), true);
        let h = sk_cost::Herkunft::neu(sk_cost::HerkunftArt::Manual, "2026-10-08", "19:30");
        c.fuer_firma(&h, &[lohn(60)]).unwrap();
        let mut s = szene(text);
        // Zwei Artikel, mit denen das Haus rechnet: A bekommt einen eigenen
        // Preis (nur dieses Haus), B ändert später die Firma
        let k = s.katalog(Some((c.library(), c.stand())));
        let blatt = s.kostenblatt(Some((c.library(), c.stand())), &sk_cost::Umfang::projekt());
        let mut genutzt: Vec<sk_model::Guid> = Vec::new();
        for z in &blatt.positionen {
            if let sk_cost::rechnung::Quelle::Leistung(g) = z.quelle {
                for art in k
                    .anteile
                    .iter()
                    .filter(|a| a.leistung == g)
                    .filter_map(|a| a.artikel)
                {
                    if k.artikel(art).is_some_and(|x| x.preis.is_some()) && !genutzt.contains(&art)
                    {
                        genutzt.push(art);
                    }
                }
            }
        }
        assert!(genutzt.len() >= 2, "{name}: {genutzt:?}");
        let (art_a, art_b) = (genutzt[0], genutzt[1]);
        let b_vorher = k.artikel(art_b).unwrap().preis.unwrap();
        let b_satz = k.artikel(art_b).unwrap().satz.clone();
        let b_text = |f: &str| b_satz.text(f).unwrap_or_default().to_string();
        s.kosten_folge("Preis nur hier", Some(c.library()), &h, &[preis(art_a, 99)])
            .unwrap();

        // Firma: Lohn 65
        c.fuer_firma(&h, &[lohn(65)]).unwrap();
        let a1 = abgleich(&s, &c);
        assert_eq!(a1.saetze.len(), 1, "{name}: {:?}", a1.texte);
        assert_eq!(a1.eigene.len(), 1, "{name}: {:?}", a1.eigene);
        assert!(
            a1.eigene[0].ends_with("(hier 99,00)"),
            "{name}: {:?}",
            a1.eigene
        );
        let g1 = a1.gleich;
        assert!(g1 > 50, "{name}: {g1}");
        // Firma: B anders → ein Eintrag weniger gleich
        c.fuer_firma(&h, &[preis(art_b, 77)]).unwrap();
        let a2 = abgleich(&s, &c);
        assert_eq!((a2.saetze.len(), a2.gleich), (2, g1 - 1), "{name}");
        // Firma: A (eigener Wert) anders → Zahl bleibt, A bleibt eigen
        c.fuer_firma(&h, &[preis(art_a, 50)]).unwrap();
        let a3 = abgleich(&s, &c);
        assert_eq!((a3.saetze.len(), a3.gleich), (2, g1 - 1), "{name}");
        assert_eq!(a3.eigene.len(), 1, "{name}");

        // Blatt: abweichend 2, eigener Wert 1, Fuß mit der Zahl
        let mut v = KostenView::new();
        (v.w, v.h) = (1200, 900);
        v.sync(&mut s, Some((c.library(), c.stand())));
        let mut p = Picking::default();
        let ansehen = v.ansehen_mitte(&t, &fonts).expect("Unterschiede ansehen");
        v.mouse_down(&t, &fonts, &mut p, ansehen, mods);
        assert!(v.unterschiede_offen(), "{name}");
        let z = v.unterschiede_zeilen();
        let pos = |kopf: &str| {
            z.iter()
                .position(|x| matches!(x, unterschiede::Zeile::Kopf(k, _) if *k == kopf))
        };
        let (ab, ei) = (
            pos("Abweichend").unwrap(),
            pos("Eigener Wert dieses Hauses").unwrap(),
        );
        assert_eq!(ei - ab - 1, 2, "{name}: {z:?}");
        assert!(
            matches!(&z[ei + 1], unterschiede::Zeile::Eintrag(e) if e.ends_with("(hier 99,00)")),
            "{name}: {z:?}"
        );
        let fuss = match z.last() {
            Some(unterschiede::Zeile::Fuss(f)) => f.replace('.', ""),
            x => panic!("{name}: {x:?}"),
        };
        assert_eq!(fuss, format!("{} Einträge gleich", g1 - 1), "{name}");

        // „so lassen“ bei offenem Blatt: schließt, Zeile weg
        let lage = v.abgleich_lage(&t, &fonts).unwrap();
        let Some(ListOut::Kosten(Schreiben::Lassen(stand))) =
            v.mouse_down(&t, &fonts, &mut p, mitte(lage.lassen), mods)
        else {
            panic!("{name}: so lassen");
        };
        assert!(!v.unterschiede_offen(), "{name}");
        s.kosten_folge(
            "Lassen",
            Some(c.library()),
            &h,
            &[Op::AbgleichLassen { stand }],
        )
        .unwrap();
        v.sync(&mut s, Some((c.library(), c.stand())));
        assert_eq!(v.abgleich_zeile(), None, "{name}: so lassen");

        // Nächster Stand: B wieder wie im Haus → ein Eintrag mehr gleich
        // mit Stand und Quelle wie im Haus (nur der Preis zählt sonst
        // nicht als gleich, siehe Hinweis F im Protokoll)
        let zurueck = Op::PreisSetzen {
            artikel: art_b,
            preis: Some(b_vorher),
            stand: b_text("date"),
            quelle: b_text("source"),
            eingabe: String::new(),
        };
        c.fuer_firma(&h, &[zurueck]).unwrap();
        let a4 = abgleich(&s, &c);
        eprintln!(
            "KA3A5 {name}: B zurück: {:?}, gleich {}",
            a4.texte, a4.gleich
        );
        assert_eq!(
            (a4.saetze.len(), a4.gleich),
            (1, g1),
            "{name}: {:?}",
            a4.texte
        );
        // „übernehmen“ bei offenem Blatt
        v.sync(&mut s, Some((c.library(), c.stand())));
        let ansehen = v.ansehen_mitte(&t, &fonts).unwrap();
        v.mouse_down(&t, &fonts, &mut p, ansehen, mods);
        assert!(v.unterschiede_offen(), "{name}");
        let lage = v.abgleich_lage(&t, &fonts).unwrap();
        let Some(ListOut::Kosten(Schreiben::Uebernehmen(saetze))) =
            v.mouse_down(&t, &fonts, &mut p, mitte(lage.uebernehmen), mods)
        else {
            panic!("{name}: übernehmen");
        };
        assert!(!v.unterschiede_offen(), "{name}");
        s.kosten_folge(
            "Übernommen",
            Some(c.library()),
            &h,
            &[Op::StandUebernehmen { saetze }],
        )
        .unwrap();
        v.sync(&mut s, Some((c.library(), c.stand())));
        assert_eq!(v.abgleich_zeile(), None, "{name}: übernommen");
        // Firmenwerte im Haus, eigener Preis bleibt (Regel 89)
        let k = s.katalog(Some((c.library(), c.stand())));
        assert_eq!(k.werte.lohn, Dez::ganz(65), "{name}: Lohn");
        assert_eq!(k.artikel(art_b).unwrap().preis, Some(b_vorher), "{name}: B");
        assert_eq!(
            k.artikel(art_a).unwrap().preis,
            Some(Dez::ganz(99)),
            "{name}: eigener Preis bleibt"
        );
        eprintln!(
            "KA3A5 {name}: {} Einträge gleich, B {} → 77 → zurück",
            g1,
            b_vorher.text()
        );
        let _ = std::fs::remove_dir_all(&d);
    }
}
