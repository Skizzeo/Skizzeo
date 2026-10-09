//! Abnahme Paket PD-1/PD-2 (Projektdaten, paket-projektdaten.md §2, §4,
//! §6.1–6.3 und 6.5, Regel 110) am Standardhaus RH-1:
//! - „Neu“ mit „Später“ (Esc) und „Neu“ mit leer übernommener Maske: kein
//!   Schritt, keine `[projectinfo]`-Zeile, Datei bytegleich.
//! - Alte Datei mit `site`/`client` an `[project]`: lädt mit den Werten,
//!   speichert bytegleich; die erste Änderung zieht alles nach
//!   `[projectinfo]` um, ein Schritt; Strg+Z bytegleich zur alten Datei,
//!   Strg+Y bytegleich zur neuen.
//! - Mehrere Änderungen mit Zeilenumbrüchen, Anführungszeichen und
//!   Backslash: je ein Schritt, Speichern und Laden bytegleich, jeder
//!   Schritt einzeln zurück und wieder vor, bytegleich.

use super::*;
use crate::projektdaten::{Antwort, Maske};
use sk_platform::{Key, Modifiers};

fn lesen(text: &str) -> sk_model::Model {
    sk_model::szo::read_with(
        text,
        sk_model::GuidGen::with_seed(1),
        &sk_cost::lesen::ABSCHNITTE_SZO,
    )
    .expect("lädt")
    .model
}

const RH1: &str = include_str!("../../crates/sk-cost/referenz/rh1-standardhaus.szo");
const LABEL: &str = "Projektdaten geändert";

fn schreiben(s: &Scene) -> String {
    sk_model::szo::write(s.model())
}

#[test]
fn abnahme_pd_neu_spaeter_ohne_spur() {
    let mut s = Scene::with_model(lesen(RH1));
    let vorher = schreiben(&s);
    assert_eq!(vorher, RH1, "Standardhaus lädt bytegleich");
    // „Später“ bzw. Esc
    let mut m = Maske::new(s.model().project(), true, None);
    assert_eq!(
        m.key(Key::Escape, Modifiers::default()),
        Some(Antwort::Verwerfen)
    );
    assert!(s.undo_label().is_none());
    // Leere Maske übernommen: Anfangswerte ohne Schritt, keine Zeile
    let mut m = Maske::new(s.model().project(), true, None);
    let Some(Antwort::Uebernehmen(p)) = m.key(Key::Enter, Modifiers::default()) else {
        panic!("Enter übernimmt");
    };
    s.projekt_anfang(*p);
    assert!(s.undo_label().is_none(), "kein Schritt bei „Neu“");
    let nachher = schreiben(&s);
    assert!(!nachher.contains("[projectinfo]"), "keine leere Zeile");
    assert_eq!(nachher, vorher, "bytegleich");
}

#[test]
fn abnahme_pd_alte_datei_umzug_ein_schritt() {
    let alt = RH1.replacen(
        "iwset=3ZTJrDtor3KfcH2vccNFkY\n",
        "iwset=3ZTJrDtor3KfcH2vccNFkY site=\"Haus Meier\" client=\"Meier\"\n",
        1,
    );
    assert_ne!(alt, RH1);
    let mut s = Scene::with_model(lesen(&alt));
    assert_eq!(s.model().project().site, "Haus Meier");
    assert_eq!(s.model().project().client, "Meier");
    assert_eq!(schreiben(&s), alt, "ohne Änderung bytegleich");
    let mut p = s.model().project().clone();
    p.number = "01/26".into();
    assert!(s.projekt_setzen(LABEL, p));
    assert_eq!(s.undo_label(), Some(LABEL));
    let neu = schreiben(&s);
    let info: Vec<&str> = neu
        .lines()
        .filter(|l| l.starts_with("[projectinfo]"))
        .collect();
    assert_eq!(info.len(), 1, "{neu}");
    for w in [
        "projno=\"01/26\"",
        "site=\"Haus Meier\"",
        "client=\"Meier\"",
    ] {
        assert!(info[0].contains(w), "{}", info[0]);
    }
    let projekt = neu.lines().find(|l| l.starts_with("[project]")).unwrap();
    assert!(
        !projekt.contains(" site=") && !projekt.contains(" client="),
        "{projekt}"
    );
    assert_eq!(sk_model::szo::write(&lesen(&neu)), neu, "Laden bytegleich");
    assert!(s.undo());
    assert!(s.undo_label().is_none(), "ein Schritt");
    assert_eq!(schreiben(&s), alt, "Strg+Z bytegleich");
    assert!(s.redo());
    assert_eq!(schreiben(&s), neu, "Strg+Y bytegleich");
}

#[test]
fn abnahme_pd_aendern_rueckgaengig_wiederholen() {
    let mut s = Scene::with_model(lesen(RH1));
    let mut stufen = vec![schreiben(&s)];
    type Setzen = fn(&mut sk_model::Project);
    let schritte: [Setzen; 4] = [
        |p| {
            p.kind = "Neubau Einfamilienhaus".into();
            p.site = "Haus \"Mustermann\"".into();
            p.place = "Musterweg 1\n27777 Ganderkesee".into();
        },
        |p| {
            p.number = "01/26".into();
            p.client = "Max Mustermann".into();
            p.client_addr = "Am Phantasieweg 7\nC:\\Briefkasten\n12345 Musterstadt".into();
        },
        |p| {
            p.author = "Dipl.-Ing. (FH) Jörn Horstmann".into();
            p.author_addr = "Denkmalsweg 18b\n27777 Ganderkesee".into();
        },
        |p| p.number = "02/26".into(),
    ];
    for (k, f) in schritte.into_iter().enumerate() {
        let mut p = s.model().project().clone();
        f(&mut p);
        assert!(s.projekt_setzen(LABEL, p.clone()), "{k}");
        assert_eq!(s.undo_label(), Some(LABEL), "{k}");
        assert!(!s.projekt_setzen(LABEL, p), "{k}: unverändert kein Schritt");
        let t = schreiben(&s);
        let m2 = lesen(&t);
        assert_eq!(m2.project(), s.model().project(), "{k}: Werte nach Laden");
        assert_eq!(sk_model::szo::write(&m2), t, "{k}: Laden bytegleich");
        stufen.push(t);
    }
    // Personendaten nie in der Bezeichnung
    assert!(!s.undo_label().unwrap().contains("Mustermann"));
    for i in (0..4).rev() {
        assert!(s.undo(), "{i}");
        assert_eq!(schreiben(&s), stufen[i], "Strg+Z Stufe {i}");
    }
    assert!(s.undo_label().is_none());
    for (i, stufe) in stufen.iter().enumerate().skip(1) {
        assert!(s.redo(), "{i}");
        assert_eq!(&schreiben(&s), stufe, "Strg+Y Stufe {i}");
    }
}
