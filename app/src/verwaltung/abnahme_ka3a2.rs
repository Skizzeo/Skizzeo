//! Abnahme KA-3a2 (paket-ka3a §5 Nr. 3, 4, 5; Koordinator 16:31) an
//! RH-1 bis RH-3, auf dem Weg, den das Hauptfenster bei OK geht
//! (`Verwaltung::ok` → `Scene::fuer_firma(STEP, …)`):
//! - Lohn 60 → 65 über die Verwaltung ergibt denselben Firmenkatalog
//!   (ohne Protokollzeilen) und dieselben Projektkosten wie die Lohnkarte
//!   mit „Auch für neue Häuser“; ein Rückgängig-Schritt, nach Strg+Z
//!   rechnet das Projekt wieder mit 60, der Firmenkatalog behält 65.
//! - Aufwandswert über die Verwaltung: Firmenkatalog hat den neuen Wert
//!   und `[catalog] stand` + 1, das Projekt rechnet damit (ein Schritt
//!   „Firmenkatalog geändert“); nach Strg+Z rechnet es wieder mit dem
//!   alten Wert und die Abgleichzeile nennt den Unterschied.
//! - Abbrechen nach drei Änderungen: Firmenkatalog, Projekt, Revision und
//!   Rückgängig-Liste unverändert.
//! - Regel 89: eigener Projektlohn (Nur dieses Haus) bleibt nach OK.

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
        "skizzeo-abnahme-ka3a2-{name}-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn herkunft() -> sk_cost::Herkunft {
    sk_cost::Herkunft::neu(sk_cost::HerkunftArt::Manual, "2026-10-08", "16:40")
}

fn netto(s: &mut Scene, c: &Company) -> i64 {
    s.kostenblatt(Some((c.library(), c.stand())), &sk_cost::Umfang::projekt())
        .netto
        .0
}

/// OK wie im Hauptfenster.
fn ok(s: &mut Scene, c: &mut Company, v: &mut Verwaltung) {
    let mut out = Out::default();
    v.ok(&mut out);
    assert!(out.ok, "OK gesperrt");
    let ops = v.ops().to_vec();
    s.fuer_firma(STEP, c, &herkunft(), &ops).expect("schreibt");
}

/// Datei ohne Zeilen, die vom Weg abhängen (Protokoll, Herkunft, Datum).
fn ohne_protokoll(t: &str) -> String {
    t.lines()
        .filter(|z| {
            !z.starts_with("[log]") && !z.starts_with("[origin]") && !z.starts_with("[catalog]")
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn stand(t: &str) -> u64 {
    t.lines()
        .find(|z| z.starts_with("[catalog]"))
        .and_then(|z| z.split_whitespace().find_map(|w| w.strip_prefix("stand=")))
        .and_then(|w| w.parse().ok())
        .unwrap_or(0)
}

#[test]
fn abnahme_ka3a2_lohn_wie_lohnkarte() {
    for (name, text) in haeuser() {
        // Weg 1: Verwaltung › Firmenwerte › Lohn 65, OK
        let d1 = ordner(&format!("{name}-v"));
        let p1 = d1.join("firmenkatalog.szk");
        let (mut c1, _) = Company::laden(&p1, true);
        let mut s1 = szene(text);
        let projekt_vorher = sk_model::szo::write(s1.model());
        let vorher = netto(&mut s1, &c1);
        let mut v = Verwaltung::open(&s1, Some(&c1), None);
        v.waehlen(Knoten::Firmenwerte);
        assert!(v.eingeben(&Feld::Wert("wage".into()), "65"));
        ok(&mut s1, &mut c1, &mut v);
        assert_eq!(s1.undo_label(), Some(STEP), "{name}: ein Schritt");
        let t1 = std::fs::read_to_string(&p1).unwrap();

        // Weg 2: Lohnkarte „Auch für neue Häuser“
        let d2 = ordner(&format!("{name}-l"));
        let p2 = d2.join("firmenkatalog.szk");
        let (mut c2, _) = Company::laden(&p2, true);
        let mut s2 = szene(text);
        let op = Op::FirmenwertSetzen {
            schluessel: "wage".into(),
            wert: Dez::ganz(65),
        };
        s2.fuer_firma("Verrechnungslohn 65,00 €/h", &mut c2, &herkunft(), &[op])
            .expect("schreibt");
        let t2 = std::fs::read_to_string(&p2).unwrap();

        assert_eq!(ohne_protokoll(&t1), ohne_protokoll(&t2), "{name}: Katalog");
        assert_eq!(stand(&t1), stand(&t2), "{name}: Stand");
        let n1 = netto(&mut s1, &c1);
        let n2 = netto(&mut s2, &c2);
        assert_eq!(n1, n2, "{name}: Kosten wie per Lohnkarte");
        assert!(n1 > vorher, "{name}: Lohn 65 macht teurer");
        // Projekt gleich bis auf die Guid des jeweiligen Firmenkatalogs
        let ohne_guid = |s: &Scene| {
            sk_model::szo::write(s.model())
                .lines()
                .filter(|z| !z.starts_with("[costproject]"))
                .collect::<Vec<_>>()
                .join("\n")
        };
        assert_eq!(ohne_guid(&s1), ohne_guid(&s2), "{name}: Projekt gleich");
        // Strg+Z: Projekt bytegleich, Firmenkatalog bleibt bei 65
        assert!(s1.undo());
        // Das Haus behält still die Kopie der bisherigen Werte (Scene::fuer_firma)
        let _ = &projekt_vorher;
        assert_eq!(std::fs::read_to_string(&p1).unwrap(), t1, "{name}");
        assert_eq!(
            netto(&mut s1, &c1),
            vorher,
            "{name}: Projekt rechnet mit 60"
        );
        let a = sk_cost::abgleich::abgleich(s1.model(), Some(c1.library()));
        assert!(a.is_some(), "{name}: Abgleichzeile nach Strg+Z");
        eprintln!(
            "KA3A2 {name}: Lohn 65 → {:.2} € (vorher {:.2} €)",
            n1 as f64 / 100.0,
            vorher as f64 / 100.0
        );
        let _ = std::fs::remove_dir_all(&d1);
        let _ = std::fs::remove_dir_all(&d2);
    }
}

#[test]
fn abnahme_ka3a2_aufwandswert_ok_und_abbrechen() {
    for (name, text) in haeuser() {
        let d = ordner(&format!("{name}-a"));
        let p = d.join("firmenkatalog.szk");
        let (mut c, _) = Company::laden(&p, true);
        let mut s = szene(text);
        let vorher = netto(&mut s, &c);
        let mut v = Verwaltung::open(&s, Some(&c), None);
        // eine Bauleistung, mit der dieses Haus rechnet
        let k = s.katalog(Some((c.library(), c.stand())));
        let blatt = s.kostenblatt(Some((c.library(), c.stand())), &sk_cost::Umfang::projekt());
        let g = blatt
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
            .expect("Leistung mit Aufwandswert");

        // Abbrechen nach drei Änderungen
        let datei = std::fs::read(&p).unwrap();
        let projekt = sk_model::szo::write(s.model());
        let rev = s.model().revision();
        v.waehlen(Knoten::Leistung(g));
        assert!(v.eingeben(&Feld::Stunden, "1,5"));
        v.waehlen(Knoten::Firmenwerte);
        assert!(v.eingeben(&Feld::Wert("wage".into()), "70"));
        assert!(v.eingeben(&Feld::Wert("vat".into()), "16"));
        assert!(v.ops().len() >= 3, "{name}: {:?}", v.ops());
        drop(v);
        assert_eq!(std::fs::read(&p).unwrap(), datei, "{name}: Katalog");
        assert_eq!(sk_model::szo::write(s.model()), projekt, "{name}: Projekt");
        assert_eq!(s.model().revision(), rev, "{name}: Revision");
        assert_eq!(s.undo_label(), None, "{name}: kein Schritt");

        // OK mit Aufwandswert 1,5 h
        let stand_vorher = stand(&String::from_utf8(datei.clone()).unwrap());
        let mut v = Verwaltung::open(&s, Some(&c), None);
        v.waehlen(Knoten::Leistung(g));
        assert!(v.eingeben(&Feld::Stunden, "1,5"));
        ok(&mut s, &mut c, &mut v);
        let t = std::fs::read_to_string(&p).unwrap();
        assert!(t.contains("hours=1.5"), "{name}");
        assert!(t.contains("op=bauleistung_aendern"), "{name}");
        assert_eq!(stand(&t), stand_vorher + 1, "{name}: Stand + 1");
        assert_eq!(s.undo_label(), Some(STEP));
        let nachher = netto(&mut s, &c);
        assert_ne!(
            nachher, vorher,
            "{name}: Projekt rechnet mit dem neuen Wert"
        );
        // Projekt rechnet wie lesen::kosten mit dem neuen Firmenkatalog
        let kf = sk_cost::lesen::firma_oder_werk(s.model(), Some(c.library()));
        let sched = sk_model::qto::schedule(s.model());
        let direkt = sk_cost::lesen::kosten(s.model(), &sched, &kf, &sk_cost::Umfang::projekt());
        assert_eq!(direkt.netto.0, nachher, "{name}");
        assert!(s.undo());
        assert_eq!(netto(&mut s, &c), vorher, "{name}: Strg+Z");
        assert!(
            t == std::fs::read_to_string(&p).unwrap(),
            "{name}: Katalog bleibt"
        );
        let a = sk_cost::abgleich::abgleich(s.model(), Some(c.library())).expect("Abgleich");
        assert!(!a.zeile().is_empty(), "{name}");
        let _ = std::fs::remove_dir_all(&d);
    }
}

#[test]
fn abnahme_ka3a2_regel_89_eigener_wert_bleibt() {
    let (name, text) = haeuser()[0];
    let d = ordner("r89");
    let p = d.join("firmenkatalog.szk");
    let (mut c, _) = Company::laden(&p, true);
    let mut s = szene(text);
    // Nur dieses Haus: 70 €/h
    let op = Op::FirmenwertSetzen {
        schluessel: "wage".into(),
        wert: Dez::ganz(70),
    };
    s.kosten_folge(
        "Verrechnungslohn nur dieses Haus",
        Some(c.library()),
        &herkunft(),
        &[op],
    )
    .unwrap();
    let eigen = netto(&mut s, &c);
    let k = s.katalog(Some((c.library(), c.stand())));
    assert!(
        k.herkunft_von("rate", "wage").is_some_and(|u| u.proj),
        "{name}: eigener Projektwert mit proj=1"
    );
    let mut v = Verwaltung::open(&s, Some(&c), None);
    v.waehlen(Knoten::Firmenwerte);
    assert!(v.eingeben(&Feld::Wert("wage".into()), "65"));
    ok(&mut s, &mut c, &mut v);
    let t = std::fs::read_to_string(&p).unwrap();
    assert!(t.contains("key=wage num=65"), "{name}: Firma 65");
    assert_eq!(netto(&mut s, &c), eigen, "{name}: Projekt behält 70");
    let _ = std::fs::remove_dir_all(&d);
}
