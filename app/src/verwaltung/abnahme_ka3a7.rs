//! Abnahme KA-3a7 (Regel 108; Koordinator 18:54) an RH-1 bis RH-3 und
//! die Ablage beim Zurückspielen (Review 3as, 28152c7):
//! - Preis je m³ (Artikel in m² mit Dicke) und je Stück (Artikel mit
//!   `conv`) über die Verwaltung ergeben denselben Firmenkatalog und
//!   dieselben Projektkosten wie der von Hand umgerechnete m²-Preis
//!   (95 €/m³ × t; 0,85 €/St × conv, einmal auf 4 Stellen halb auf).
//! - Zweimal dieselbe Sicherung zurückgespielt: keine Ablage geht
//!   verloren, jede je gesehene Fassung liegt noch im Ordner.

use super::*;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

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
        "skizzeo-abnahme-ka3a7-{name}-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn herkunft() -> sk_cost::Herkunft {
    sk_cost::Herkunft::neu(sk_cost::HerkunftArt::Manual, "2026-10-08", "19:00")
}

fn netto(s: &mut Scene, c: &Company) -> i64 {
    s.kostenblatt(Some((c.library(), c.stand())), &sk_cost::Umfang::projekt())
        .netto
        .0
}

/// Deutsche Zahl ohne Tausenderpunkt, bis 6 Stellen.
fn deutsch(d: Dez) -> String {
    let ganz = d.0 / 1_000_000;
    let rest = format!("{:06}", d.0 % 1_000_000);
    let rest = rest.trim_end_matches('0');
    if rest.is_empty() {
        ganz.to_string()
    } else {
        format!("{ganz},{rest}")
    }
}

/// `[article] … price=…`-Zeilen der Datei, nach Artikel sortiert.
fn preise(p: &Path) -> Vec<String> {
    let t = std::fs::read_to_string(p).unwrap();
    let mut z: Vec<String> = t
        .lines()
        .filter(|l| l.starts_with("[article]"))
        .map(|l| {
            l.split_whitespace()
                .filter(|w| w.starts_with("id=") || w.starts_with("price="))
                .collect::<Vec<_>>()
                .join(" ")
        })
        .collect();
    z.sort();
    z
}

#[test]
fn abnahme_ka3a7_je_m3_und_je_stueck_wie_m2() {
    for (name, text) in haeuser() {
        // Weg 1: je m³ bzw. je Stück eingeben
        let d1 = ordner(&format!("{name}-je"));
        let p1 = d1.join("firmenkatalog.szk");
        let (mut c1, _) = Company::laden(&p1, true);
        let mut s1 = szene(text);
        let vorher = netto(&mut s1, &c1);
        let mut v = Verwaltung::open(&s1, Some(&c1), None);
        let mut soll: Vec<(Guid, Dez)> = Vec::new();
        let (mut n_m3, mut n_st) = (0, 0);
        for a in v.jetzt.artikel.clone() {
            if a.retired || a.preis.is_none() {
                continue;
            }
            let angebot = sk_cost::einheit::angebot(&a);
            v.waehlen(Knoten::ArtikelSatz(a.guid));
            if let (Some(c), true) = (
                a.conv,
                a.einheit != Einheit::St && angebot.contains(&Einheit::St),
            ) {
                // 0,85 €/St × conv, einmal auf 4 Stellen halb auf
                v.aktion(Aktion::Je(Einheit::St));
                assert!(v.eingeben(&Feld::Preis(a.guid), "0,85"), "{}", a.name);
                let x = 85i128 * c.0 as i128 / 100; // 10^-6 €
                let p = (x + 50) / 100 * 100;
                soll.push((a.guid, Dez(p as i64)));
                n_st += 1;
            } else if a.einheit == Einheit::M2 && angebot.contains(&Einheit::M3) {
                // 95 €/m³ × t (t in mm)
                v.aktion(Aktion::Je(Einheit::M3));
                assert!(v.eingeben(&Feld::Preis(a.guid), "95"), "{}", a.name);
                let t = a.t.unwrap();
                let p = 95i128 * t.0 as i128 / 1000;
                soll.push((a.guid, Dez(p as i64)));
                n_m3 += 1;
            }
        }
        assert!(
            n_m3 > 0 && n_st > 0,
            "{name}: {n_m3} je m³, {n_st} je Stück"
        );
        for (g, p) in &soll {
            assert_eq!(v.jetzt.artikel(*g).unwrap().preis, Some(*p), "{name}");
        }
        let mut out = Out::default();
        v.ok(&mut out);
        assert!(out.ok, "{name}: OK gesperrt");
        let ops = v.ops().to_vec();
        s1.fuer_firma(STEP, &mut c1, &herkunft(), &ops)
            .expect("schreibt");

        // Weg 2: der von Hand umgerechnete Preis je m²
        let d2 = ordner(&format!("{name}-m2"));
        let p2 = d2.join("firmenkatalog.szk");
        let (mut c2, _) = Company::laden(&p2, true);
        let mut s2 = szene(text);
        let mut v = Verwaltung::open(&s2, Some(&c2), None);
        for (g, p) in &soll {
            v.waehlen(Knoten::ArtikelSatz(*g));
            assert!(
                v.eingeben(&Feld::Preis(*g), &deutsch(*p)),
                "{}",
                deutsch(*p)
            );
        }
        let ops = v.ops().to_vec();
        s2.fuer_firma(STEP, &mut c2, &herkunft(), &ops)
            .expect("schreibt");

        assert_eq!(preise(&p1), preise(&p2), "{name}: Preise im Katalog");
        let n1 = netto(&mut s1, &c1);
        let n2 = netto(&mut s2, &c2);
        assert_eq!(n1, n2, "{name}: Kosten wie per m²-Preis");
        assert_ne!(n1, vorher, "{name}: die Preise wirken");
        let t1 = std::fs::read_to_string(&p1).unwrap();
        assert!(t1.contains("eingegeben 95,00 €/m³ × "), "{name}");
        assert!(t1.contains("eingegeben 0,85 €/St × "), "{name}");
        eprintln!(
            "KA3A7 {name}: {n_m3} Artikel je m³, {n_st} je Stück, netto {:.2} € (vorher {:.2} €)",
            n1 as f64 / 100.0,
            vorher as f64 / 100.0
        );
        let _ = std::fs::remove_dir_all(&d1);
        let _ = std::fs::remove_dir_all(&d2);
    }
}

/// Alle Fassungen im Ablageordner (Inhalt).
fn ablage(d: &Path) -> BTreeSet<String> {
    std::fs::read_dir(d.join("firmenkatalog-staende"))
        .map(|r| {
            r.filter_map(|e| std::fs::read_to_string(e.ok()?.path()).ok())
                .collect()
        })
        .unwrap_or_default()
}

#[test]
fn abnahme_3as_zurueckspielen_ueberschreibt_keine_sicherung() {
    let d = ordner("zurueck");
    let p = d.join("firmenkatalog.szk");
    let (mut c, _) = Company::laden(&p, true);
    let lohn = |w: i64| Op::FirmenwertSetzen {
        schluessel: "wage".into(),
        wert: Dez::ganz(w),
    };
    let mut gesehen: BTreeSet<String> = BTreeSet::new();
    let schreiben = |c: &mut Company, w: i64, gesehen: &mut BTreeSet<String>| {
        c.fuer_firma(&herkunft(), &[lohn(w)]).unwrap();
        gesehen.extend(ablage(&d));
        let jetzt = ablage(&d);
        let fehlt: Vec<_> = gesehen.difference(&jetzt).collect();
        assert!(fehlt.is_empty(), "Lohn {w}: Ablage verloren: {fehlt:?}");
    };
    schreiben(&mut c, 61, &mut gesehen);
    schreiben(&mut c, 62, &mut gesehen);
    let sicherung = std::fs::read_to_string(&p).unwrap();
    schreiben(&mut c, 63, &mut gesehen);
    schreiben(&mut c, 64, &mut gesehen);
    // Zweimal dieselbe Sicherung zurückgespielt, je zwei neue Stände
    for runde in [[70, 71], [80, 81]] {
        std::fs::write(&p, &sicherung).unwrap();
        c.reload(false);
        for w in runde {
            schreiben(&mut c, w, &mut gesehen);
        }
    }
    let staende = d.join("firmenkatalog-staende");
    for n in ["stand-0003.frueher-1.szk", "stand-0003.frueher-2.szk"] {
        assert!(staende.join(n).exists(), "{n}");
    }
    // Zurücknehmen rechnet gegen die Ablage dieser Fassung
    let m = sk_model::Model::from_library(c.library());
    assert_eq!(c.umkehr(&m, 4).unwrap(), [lohn(80)]);
    let mut namen: Vec<String> = std::fs::read_dir(&staende)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    namen.sort();
    eprintln!("3AS Ablage: {namen:?}");
    let _ = std::fs::remove_dir_all(&d);
}

/// Kosten A4 (d55ed03): Die Fuge nach Stoffart ändert nur den Vorschlag.
/// Ein schon bestätigtes `conv` (Kerndämmplatte mit altem Vorschlag
/// 1,5592) bleibt im Firmenkatalog und in der Projektkopie, rechnet je
/// Stück weiter damit, und die Kosten der Häuser ändern sich nicht.
#[test]
fn abnahme_ka3a7_gespeichertes_conv_bleibt() {
    let alt = Dez(1_559_200);
    for (name, text) in haeuser() {
        let d = ordner(&format!("conv-{name}"));
        let p = d.join("firmenkatalog.szk");
        let (mut c, _) = Company::laden(&p, true);
        let mut s = szene(text);
        let v = Verwaltung::open(&s, Some(&c), None);
        let kd = v
            .jetzt
            .artikel
            .iter()
            .find(|a| a.name.starts_with("Kerndämmplatte"))
            .expect("Kerndämmplatte")
            .clone();
        // Neuer Vorschlag ohne Fuge
        let neu = sk_cost::einheit::vorschlag(&kd).expect("Vorschlag");
        assert_eq!(neu.conv, Dez(1_600_000), "{name}");
        // Gespeichertes conv von früher, in Firma und Projektkopie
        let op = Op::UmrechnungSetzen {
            artikel: kd.guid,
            conv: Some(alt),
        };
        c.fuer_firma(&herkunft(), std::slice::from_ref(&op))
            .unwrap();
        s.kosten_folge("conv nur hier", Some(c.library()), &herkunft(), &[op])
            .unwrap();
        let datei = std::fs::read_to_string(&p).unwrap();
        let projekt = sk_model::szo::write(s.model());
        let vorher = netto(&mut s, &c);
        // Neu laden wie am nächsten Morgen
        let (c2, _) = Company::laden(&p, true);
        let mut s2 = Scene::with_model(
            sk_model::szo::read_with(
                &projekt,
                sk_model::GuidGen::with_seed(1),
                &sk_cost::lesen::ABSCHNITTE_SZO,
            )
            .unwrap()
            .model,
        );
        assert_eq!(sk_model::szo::write(s2.model()), projekt, "{name}");
        assert_eq!(netto(&mut s2, &c2), vorher, "{name}: Kosten");
        let mut v = Verwaltung::open(&s2, Some(&c2), None);
        let a = v.jetzt.artikel(kd.guid).unwrap().clone();
        assert_eq!(a.conv, Some(alt), "{name}: Firma behält 1,5592");
        assert!(sk_cost::einheit::vorschlag(&a).is_none(), "{name}");
        let k = s2.katalog(Some((c2.library(), c2.stand())));
        assert_eq!(
            k.artikel(kd.guid).unwrap().conv,
            Some(alt),
            "{name}: Projekt"
        );
        // je Stück rechnet mit dem gespeicherten Wert: 10 € × 1,5592
        v.waehlen(Knoten::ArtikelSatz(kd.guid));
        v.aktion(Aktion::Je(Einheit::St));
        assert!(v.eingeben(&Feld::Preis(kd.guid), "10"));
        assert_eq!(
            v.jetzt.artikel(kd.guid).unwrap().preis,
            Some(Dez(15_592_000)),
            "{name}"
        );
        drop(v);
        assert_eq!(std::fs::read_to_string(&p).unwrap(), datei, "{name}");
        let _ = std::fs::remove_dir_all(&d);
    }
}
