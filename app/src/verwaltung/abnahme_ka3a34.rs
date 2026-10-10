//! Abnahme KA-3a3 und KA-3a4 (paket-ka3a; Koordinator 17:13) an RH-1 bis
//! RH-3, auf dem Weg des Hauptfensters (`Company::umkehr` →
//! `Verwaltung::zuruecknehmen` → OK → `Scene::fuer_firma(STEP, …)`):
//! - „Diese Änderung zurücknehmen“ mit zwei Arbeitsplätzen auf derselben
//!   Datei: ein fremder späterer Stand an einem anderen Satz bleibt, der
//!   zurückgenommene Satz hat wieder seinen Wert, das Projekt rechnet in
//!   einem Schritt wieder wie vorher. Hat der andere Platz denselben Satz
//!   inzwischen geändert, geht seine Änderung nicht verloren.
//! - €/m² Geschossfläche: RH-1 bis RH-3 als eigene Referenzhäuser, netto
//!   und Fläche unabhängig nachgerechnet.
//! - Kaputte Referenzhäuser (kein Haus, abgeschnitten, kein Text) sperren
//!   OK nicht und bleiben bytegleich.

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
    Scene::with_model(modell(text))
}

fn modell(text: &str) -> sk_model::Model {
    sk_model::szo::read_with(
        text,
        sk_model::GuidGen::with_seed(1),
        &sk_cost::lesen::ABSCHNITTE_SZO,
    )
    .expect("lädt")
    .model
}

fn ordner(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!(
        "skizzeo-abnahme-ka3a34-{name}-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn herkunft(uhr: &str) -> sk_cost::Herkunft {
    sk_cost::Herkunft::neu(sk_cost::HerkunftArt::Manual, "2026-10-08", uhr)
}

fn netto(s: &mut Scene, c: &Company) -> i64 {
    s.kostenblatt(Some((c.library(), c.stand())), &sk_cost::Umfang::projekt())
        .netto
        .0
}

/// Aufwandswert der Bauleistung `g` in der Datei `p`, frisch gelesen.
fn stunden(p: &std::path::Path, m: &sk_model::Model, g: sk_model::Guid) -> Dez {
    let (c, _) = Company::laden(p, true);
    sk_cost::lesen::firma_oder_werk(m, Some(c.library()))
        .leistung(g)
        .unwrap()
        .stunden
}

/// Eine Bauleistung mit Aufwandswert, mit der dieses Haus rechnet.
fn leistung(s: &mut Scene, c: &Company) -> sk_model::Guid {
    let k = s.katalog(Some((c.library(), c.stand())));
    let blatt = s.kostenblatt(Some((c.library(), c.stand())), &sk_cost::Umfang::projekt());
    blatt
        .positionen
        .iter()
        .find_map(|z| match z.quelle {
            sk_cost::rechnung::Quelle::Leistung(g)
                if k.leistungen
                    .iter()
                    .any(|l| l.guid == g && l.stunden != Dez::NULL) =>
            {
                Some(g)
            }
            _ => None,
        })
        .expect("Leistung mit Aufwandswert")
}

#[test]
fn abnahme_ka3a3_zuruecknehmen_zwei_plaetze() {
    for (name, text) in haeuser() {
        let d = ordner(&format!("z-{name}"));
        let p = d.join("firmenkatalog.szk");
        // Platz A (dieses Haus) und Platz B (anderes Haus) auf einer Datei
        let (mut a, _) = Company::laden(&p, true);
        let (mut b, _) = Company::laden(&p, true);
        let mut s = szene(text);
        let mut s_b = szene(text);
        let vorher = netto(&mut s, &a);
        let g = leistung(&mut s, &a);
        let h0 = stunden(&p, s.model(), g);

        // Stand 1, Platz A: Aufwandswert 1,5 h über die Verwaltung
        let mut v = Verwaltung::open(&s, Some(&a), None);
        v.waehlen(Knoten::Leistung(g));
        assert!(v.eingeben(&Feld::Stunden, "1,5"));
        s.fuer_firma(STEP, &mut a, &herkunft("17:20"), v.ops())
            .expect("Stand 1");
        let mit_15 = netto(&mut s, &a);
        assert_ne!(mit_15, vorher, "{name}");
        let stand1 = sk_cost::verwaltung::protokoll(&sk_cost::lesen::firma_oder_werk(
            s.model(),
            Some(a.library()),
        ))[0]
            .stand;

        // Stand 2, Platz B (hat Stand 1 nicht gesehen): Lohn 70
        let lohn70 = Op::FirmenwertSetzen {
            schluessel: "wage".into(),
            wert: Dez::ganz(70),
        };
        s_b.fuer_firma(STEP, &mut b, &herkunft("17:21"), &[lohn70])
            .expect("Stand 2");
        let t = std::fs::read_to_string(&p).unwrap();
        assert!(t.contains("key=wage num=70"), "{name}");
        assert!(t.contains("hours=1.5"), "{name}: B behält Stand 1");

        // Platz A nimmt Stand 1 zurück, ohne neu geladen zu haben
        let mut v = Verwaltung::open(&s, Some(&a), None);
        v.waehlen(Knoten::Stand(stand1));
        let r = a.umkehr(s.model(), stand1).map_err(|m| m.to_string());
        v.zuruecknehmen(stand1, r);
        assert!(
            !v.ops().is_empty() && !v.gesperrt(),
            "{name}: {:?}",
            v.meldung
        );
        s.fuer_firma(STEP, &mut a, &herkunft("17:22"), v.ops())
            .expect("Stand 3");
        assert_eq!(s.undo_label(), Some(STEP), "{name}: ein Schritt");
        let t = std::fs::read_to_string(&p).unwrap();
        assert!(
            t.contains("key=wage num=70"),
            "{name}: fremder Stand 2 bleibt"
        );
        assert_eq!(stunden(&p, s.model(), g), h0, "{name}: Aufwandswert zurück");
        let st = sk_cost::verwaltung::protokoll(&sk_cost::lesen::firma_oder_werk(
            s.model(),
            Some(a.library()),
        ));
        assert_eq!(st.len(), 3, "{name}: {st:#?}");
        // Dieses Haus rechnet wieder wie vor Stand 1 (Lohn 70 von Platz B
        // kam nie in dieses Haus), Strg+Z bringt 1,5 h zurück
        assert_eq!(netto(&mut s, &a), vorher, "{name}: Projekt zurück");
        assert!(s.undo());
        assert_eq!(netto(&mut s, &a), mit_15, "{name}: Strg+Z");
        assert!(s.redo());
        assert_eq!(netto(&mut s, &a), vorher, "{name}: Wiederholen");

        // Platz B ändert denselben Satz wieder (Stand 4, 2,0 h); Platz A
        // hat das nicht gesehen und will Stand 3 zurücknehmen
        b.reload(false);
        let mut vb = Verwaltung::open(&s_b, Some(&b), None);
        vb.waehlen(Knoten::Leistung(g));
        assert!(vb.eingeben(&Feld::Stunden, "2"));
        s_b.fuer_firma(STEP, &mut b, &herkunft("17:23"), vb.ops())
            .expect("Stand 4");
        let t4 = std::fs::read_to_string(&p).unwrap();
        let stand3 = st[0].stand;
        let mut v = Verwaltung::open(&s, Some(&a), None);
        let r = a.umkehr(s.model(), stand3).map_err(|m| m.to_string());
        let umkehr_ok = r.is_ok();
        v.zuruecknehmen(stand3, r);
        let geschrieben = if v.ops().is_empty() {
            Err(v.meldung.clone().unwrap_or_default())
        } else {
            s.fuer_firma(STEP, &mut a, &herkunft("17:24"), v.ops())
                .map_err(|m| m.to_string())
        };
        eprintln!(
            "KA3A3 {name}: umkehr ok={umkehr_ok}, Schreiben: {:?}",
            geschrieben.as_ref().map(|_| "geschrieben")
        );
        assert!(geschrieben.is_err(), "{name}: Stand 4 von B überschrieben");
        assert_eq!(
            std::fs::read_to_string(&p).unwrap(),
            t4,
            "{name}: Datei unverändert"
        );
        assert_eq!(
            stunden(&p, s.model(), g),
            Dez::ganz(2),
            "{name}: B behält 2,0 h"
        );
        // Befund E: Nach dem abgelehnten OK setzt das Fenster auf dem neuen
        // Stand auf (Review 3ar, wie main.rs). Ein zweites OK darf Stand 4
        // von Platz B nicht überschreiben: Regel 93, nur der Befund, weil
        // der Satz nach Stand 3 wieder geändert wurde.
        v.neu_grundlage(&a);
        if !v.ops().is_empty() {
            let _ = s.fuer_firma(STEP, &mut a, &herkunft("17:25"), v.ops());
        }
        assert_eq!(
            stunden(&p, s.model(), g),
            Dez::ganz(2),
            "{name}: zweites OK nimmt Stand 4 von B zurück"
        );
        let _ = std::fs::remove_dir_all(&d);
    }
}

#[test]
fn abnahme_ka3a4_je_m2_rh() {
    let d = ordner("m2");
    let ref_ordner = d.join(wirkung::ORDNER);
    std::fs::create_dir_all(&ref_ordner).unwrap();
    for (name, text) in haeuser() {
        std::fs::write(ref_ordner.join(format!("{name}.szo")), text).unwrap();
    }
    let (c, _) = Company::laden(&d.join("firmenkatalog.szk"), true);
    let s = szene(haeuser()[0].1);
    let mut v = Verwaltung::open(&s, Some(&c), None);
    v.waehlen(Knoten::Firmenwerte);
    assert!(v.eingeben(&Feld::Wert("wage".into()), "65"));
    // netto mit Lohn 60 (Werk) und 65, aus Abnahme KA-0 und KA-3a2
    let soll = [
        ("RH-1", 6_195_388, 6_442_692),
        ("RH-2", 7_183_668, 7_467_724),
        ("RH-3", 6_880_603, 7_151_903),
    ];
    for ((name, text), (n, n60, n65)) in haeuser().into_iter().zip(soll) {
        assert_eq!(name, n);
        let h = v
            .wirkung
            .haeuser
            .iter()
            .find(|h| h.name == name)
            .expect("Referenzhaus");
        assert!(h.fehler.is_none() && h.hinweis().is_none(), "{name}");
        let m = modell(text);
        let sched = sk_model::qto::schedule(&m);
        let qm = sched.floor_area(&m) / 1e6;
        assert!((h.flaeche / 1e6 - qm).abs() < 1e-9, "{name}");
        let k = sk_cost::lesen::firma_oder_werk(&m, Some(c.library()));
        let direkt = sk_cost::lesen::kosten(&m, &sched, &k, &sk_cost::Umfang::projekt());
        assert_eq!(h.vorher.0, direkt.netto.0, "{name}: netto");
        assert_eq!(h.vorher.0, n60, "{name}: Referenzwert Lohn 60");
        assert_eq!(h.nachher.0, n65, "{name}: Lohn 65");
        let e = |c: i64| (c as f64 / 100.0 / qm).round() as i64;
        assert_eq!(h.je_m2(h.vorher), Some(e(n60)), "{name}");
        assert_eq!(h.je_m2(h.nachher), Some(e(n65)), "{name}");
        eprintln!(
            "KA3A4 {name}: {qm:.4} m², {:.2} € → {} €/m², Lohn 65: {:.2} € → {} €/m²",
            n60 as f64 / 100.0,
            e(n60),
            n65 as f64 / 100.0,
            e(n65)
        );
    }
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn abnahme_ka3a4_kaputt_sperrt_nicht() {
    let d = ordner("kaputt");
    let ref_ordner = d.join(wirkung::ORDNER);
    std::fs::create_dir_all(&ref_ordner).unwrap();
    let rh2 = haeuser()[1].1;
    let kaputt: [(&str, Vec<u8>); 3] = [
        ("kein-haus.szo", b"kein Haus".to_vec()),
        ("halb.szo", rh2.as_bytes()[..rh2.len() / 2].to_vec()),
        ("binaer.szo", vec![0xff, 0xfe, 0x00, 0x81]),
    ];
    for (n, b) in &kaputt {
        std::fs::write(ref_ordner.join(n), b).unwrap();
    }
    std::fs::write(ref_ordner.join("rh2.szo"), rh2).unwrap();
    let p = d.join("firmenkatalog.szk");
    let (mut c, _) = Company::laden(&p, true);
    let mut s = szene(haeuser()[0].1);
    let mut v = Verwaltung::open(&s, Some(&c), None);
    for h in &v.wirkung.haeuser {
        eprintln!("KA3A4 {}: {:?}", h.name, h.hinweis());
    }
    let rh = v.wirkung.haeuser.iter().find(|h| h.name == "rh2").unwrap();
    assert!(rh.vorher.0 > 0 && rh.fehler.is_none());
    for (n, _) in &kaputt {
        let stamm = n.trim_end_matches(".szo");
        let h = v.wirkung.haeuser.iter().find(|h| h.name == stamm).unwrap();
        assert!(h.hinweis().is_some(), "{n}: grau mit Befund");
        assert_eq!(h.vorher.0, 0, "{n}");
    }
    v.waehlen(Knoten::Firmenwerte);
    assert!(v.eingeben(&Feld::Wert("wage".into()), "65"));
    assert!(!v.gesperrt(), "{:?}", v.meldung);
    let mut out = Out::default();
    v.ok(&mut out);
    assert!(out.ok, "OK gesperrt");
    let ops = v.ops().to_vec();
    s.fuer_firma(STEP, &mut c, &herkunft("17:30"), &ops)
        .expect("schreibt");
    assert!(std::fs::read_to_string(&p)
        .unwrap()
        .contains("key=wage num=65"));
    for (n, b) in &kaputt {
        assert_eq!(&std::fs::read(ref_ordner.join(n)).unwrap(), b, "{n}");
    }
    let _ = std::fs::remove_dir_all(&d);
}
