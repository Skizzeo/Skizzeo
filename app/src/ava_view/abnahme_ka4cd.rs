//! Abnahme KA-4c/KA-4d (paket-ka4 §4, §6 Nr. 3, 4, 10, 11; Koordinator
//! 15:21) an RH-1 bis RH-3:
//! - CSV jedes Loses: „Summe netto“ = Netto des LV = Summe der Titel; die
//!   CSV aller Lose zusammen = Netto des Kostenblatts auf den Cent (ohne
//!   Untertitel); mit Untertiteln höchstens die Rundung je
//!   Untertitelposition daneben (0,0005 × EP + 0,5 Cent).
//! - OZ in der CSV je Los eindeutig, mit und ohne Untertitel.
//! - Kopf (mit Projektdaten, PD-3): nur anzeigend, jedes Feld öffnet die
//!   Maske; je Änderung über die Maske ein Rückgängig-Schritt; Kopf und CSV
//!   sofort aktuell; Strg+Z bytegleich; Speichern und Laden behalten die
//!   Werte; Klicks im Kopf ändern nichts.
//! - „Geschosse als Untertitel“: ein Schritt, `lvstorey=1` in der Datei,
//!   nach Laden wieder gesetzt, Strg+Z bytegleich.

use super::*;
use sk_cost::Umfang;
use std::collections::BTreeSet;

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

fn lesen(text: &str) -> Model {
    sk_model::szo::read_with(
        text,
        sk_model::GuidGen::with_seed(1),
        &sk_cost::lesen::ABSCHNITTE_SZO,
    )
    .expect("lädt")
    .model
}

fn blatt(s: &mut Scene) -> AvaView {
    let mut v = AvaView::new();
    (v.w, v.h) = (1400, 900);
    v.top = 120.0;
    v.datei = "haus.szo".into();
    v.sync(s, None);
    v
}

fn cent(s: &str) -> i64 {
    let neg = s.starts_with('-') || s.starts_with('−');
    let t = s.trim_start_matches(['-', '−']);
    let (e, c) = t.split_once(',').expect(s);
    let v = e.parse::<i64>().unwrap() * 100 + c.parse::<i64>().unwrap();
    if neg {
        -v
    } else {
        v
    }
}

fn csv_zeilen(v: &AvaView) -> Vec<Vec<String>> {
    let t = String::from_utf8(v.csv()).unwrap();
    let t = t.strip_prefix('\u{feff}').expect("BOM").to_string();
    t.split("\r\n")
        .map(|z| z.split(';').map(str::to_string).collect())
        .collect()
}

fn gliedern(s: &mut Scene, untertitel: bool) {
    let h = sk_cost::Herkunft::neu(sk_cost::HerkunftArt::Manual, "2026-10-08", "15:30");
    let op = sk_cost::Op::LvGliederungSetzen { untertitel };
    s.kosten_folge("LV nach Geschossen gegliedert", None, &h, &[op])
        .unwrap();
}

#[test]
fn abnahme_ka4d_csv_gleich_lv_gleich_netto() {
    for (name, text) in haeuser() {
        let mut s = Scene::with_model(lesen(text));
        let netto_blatt = s.kostenblatt(None, &Umfang::projekt()).netto.0;
        let kat = s.katalog(None);
        let lose: Vec<Guid> = kat
            .lose
            .iter()
            .filter(|l| l.parent.is_none() && !l.retired)
            .map(|l| l.guid)
            .collect();
        let mut ohne_ut = 0i64;
        for untertitel in [false, true] {
            if untertitel {
                gliedern(&mut s, true);
            }
            let mut summe = 0i64;
            let mut schranke = 0f64;
            for los in &lose {
                let mut v = blatt(&mut s);
                v.los = Some(*los);
                v.sync(&mut s, None);
                assert_eq!(v.untertitel, untertitel, "{name}");
                let lv = v.lv.as_deref().expect("LV").clone();
                let z = csv_zeilen(&v);
                let kopf = z.iter().position(|r| r[0] == "OZ").expect("Spaltenkopf");
                let mut oz = BTreeSet::new();
                let mut netto_csv = None;
                for r in z[kopf + 1..].iter().filter(|r| r.len() >= 6) {
                    if !r[2].is_empty() {
                        assert!(
                            oz.insert(r[0].clone()),
                            "{name} ut={untertitel} Los {}: OZ {} doppelt",
                            lv.kopf.los,
                            r[0]
                        );
                        if untertitel {
                            assert_eq!(r[0].split('.').count(), 3, "{name}: {r:?}");
                            if !r[4].is_empty() {
                                schranke += cent(&r[4]) as f64 * 0.0005 + 0.5;
                            }
                        }
                    }
                    if r[1] == "Summe netto" {
                        netto_csv = Some(cent(&r[5]));
                    }
                }
                assert_eq!(oz.len(), lv.anzahl(), "{name} Los {}", lv.kopf.los);
                let netto_lv = lv.zusammenstellung.netto.map(|c| c.0);
                if lv.anzahl() == 0 {
                    continue;
                }
                assert_eq!(
                    netto_csv, netto_lv,
                    "{name} ut={untertitel} Los {}: CSV ≠ LV",
                    lv.kopf.los
                );
                summe += netto_csv.unwrap_or(0);
            }
            if untertitel {
                let d = (summe - ohne_ut).abs() as f64;
                assert!(
                    d <= schranke,
                    "{name}: mit Untertiteln {summe} statt {ohne_ut}, Schranke {schranke}"
                );
                eprintln!(
                    "KA4D {name}: Untertitel Σ CSV {:.2} € (Rundung {:.2} €)",
                    summe as f64 / 100.0,
                    d / 100.0
                );
            } else {
                assert_eq!(
                    summe, netto_blatt,
                    "{name}: Σ CSV aller Lose ≠ Netto Kostenblatt"
                );
                ohne_ut = summe;
                eprintln!("KA4D {name}: Σ CSV {:.2} € = Netto", summe as f64 / 100.0);
            }
        }
    }
}

/// Ein Wert aus `LvKopf`, gleich ob Text oder optionaler Text.
trait KopfWert {
    fn wert(&self) -> Option<&str>;
}

impl KopfWert for String {
    fn wert(&self) -> Option<&str> {
        Some(self.as_str()).filter(|s| !s.is_empty())
    }
}

impl KopfWert for Option<String> {
    fn wert(&self) -> Option<&str> {
        self.as_deref().filter(|s| !s.is_empty())
    }
}

/// KA-4c mit Projektdaten (PD-3, paket-projektdaten.md §2, §4, §6.3–6.5,
/// Regel 110), Ersatz für `abnahme_ka4c_kopf_je_ein_schritt`: Der Kopf
/// zeigt nur an. Jedes Kopffeld, „Bauherr fehlt“ und „Projektdaten …“
/// öffnen die Maske im passenden Feld und ändern selbst nichts. Geändert wird über die Maske:
/// je Änderung genau ein Rückgängig-Schritt „Projektdaten geändert“ (ohne
/// Werte in der Bezeichnung), unverändert keiner; Kopf und CSV zeigen nach
/// `sync` sofort den neuen Stand; Speichern und Laden bytegleich; die erste
/// Änderung zieht nach `[projectinfo]` um; jeder Schritt einzeln zurück,
/// bytegleich.
#[test]
fn abnahme_ka4c_kopf_je_ein_schritt() {
    for (name, text) in haeuser() {
        let mut s = Scene::with_model(lesen(text));
        let mut v = blatt(&mut s);
        let anfang = sk_model::szo::write(s.model());
        assert!(!anfang.contains("[projectinfo]"), "{name}: alte Datei");

        // Kopf auf; jedes Feld führt in die Maske, im richtigen Feld
        v.kopf_klick(Some(Hot::Kopf));
        for (f, i) in [
            (kopf::Feld::Bauvorhaben, 1),
            (kopf::Feld::Projekt, 0),
            (kopf::Feld::Projektnummer, 3),
            (kopf::Feld::Bauherr, 4),
            (kopf::Feld::Aufsteller, 6),
        ] {
            assert_eq!(f.maske_feld(), i, "{name}: {f:?}");
            let aus = v.kopf_klick(Some(Hot::Feld(f)));
            assert!(
                matches!(aus, Some(ListOut::Projektdaten(k)) if k == i),
                "{name}: {f:?} öffnet die Maske im Feld {i}"
            );
        }
        // „Bauherr fehlt“ öffnet die Maske im Feld Bauherr
        assert!(v.bauherr_fehlt(), "{name}");
        let aus = v.kopf_klick(Some(Hot::BauherrFehlt));
        assert!(
            matches!(aus, Some(ListOut::Projektdaten(4))),
            "{name}: „Bauherr fehlt“ öffnet die Maske im Feld Bauherr"
        );
        // „Projektdaten …“ in der Kopfzeile öffnet die Maske vorn
        let aus = v.kopf_klick(Some(Hot::Projektdaten));
        assert!(
            matches!(aus, Some(ListOut::Projektdaten(0))),
            "{name}: „Projektdaten …“ öffnet die Maske"
        );
        // Der Kopf hat keine Eingabe (kein text/key mehr): nach allen
        // Klicks kein Schritt, Datei bytegleich
        assert!(s.undo_label().is_none(), "{name}: Kopf ohne Schritt");
        assert_eq!(sk_model::szo::write(s.model()), anfang, "{name}");

        // Unverändert übernehmen: kein Schritt
        let p0 = s.model().project().clone();
        assert!(!s.projekt_setzen("Projektdaten geändert", p0), "{name}");
        assert!(s.undo_label().is_none(), "{name}: unverändert ohne Schritt");
        assert_eq!(sk_model::szo::write(s.model()), anfang, "{name}");

        // Drei Änderungen über die Maske, je genau ein Schritt
        type Setzen = fn(&mut sk_model::Project);
        let aenderungen: [Setzen; 3] = [
            |p| {
                p.site = "Haus Mustermann".into();
                p.kind = "Neubau Einfamilienhaus".into();
                p.place = "Musterweg 1\n27777 Ganderkesee".into();
            },
            |p| {
                p.number = "01/26".into();
                p.client = "Max Mustermann".into();
                p.client_addr = "Am Phantasieweg 7\n12345 Musterstadt".into();
            },
            |p| {
                p.author = "Dipl.-Ing. (FH) Jörn Horstmann".into();
                p.author_addr = "Denkmalsweg 18b\n27777 Ganderkesee".into();
                p.number = "02/26".into();
            },
        ];
        let mut stufen = vec![anfang.clone()];
        for (k, aendern) in aenderungen.into_iter().enumerate() {
            let mut p = s.model().project().clone();
            aendern(&mut p);
            assert!(s.projekt_setzen("Projektdaten geändert", p), "{name}: {k}");
            assert_eq!(s.undo_label(), Some("Projektdaten geändert"), "{name}: {k}");
            v.sync(&mut s, None);
            let jetzt = sk_model::szo::write(s.model());
            assert!(jetzt.contains("[projectinfo]"), "{name}: {k}");
            // Regel 110: site, client, author nicht mehr an [project]
            let projekt = jetzt
                .lines()
                .find(|l| l.starts_with("[project]"))
                .expect("[project]");
            for alt in [" site=", " client=", " author="] {
                assert!(!projekt.contains(alt), "{name}: {k}: {alt} umgezogen");
            }
            // Kopf und CSV sofort aktuell
            let lv = v.lv.as_deref().unwrap();
            let kp = &lv.kopf;
            let pr = s.model().project().clone();
            let soll = [
                ("Bauvorhaben", kp.bauvorhaben.wert(), pr.site.as_str()),
                ("Projektart", kp.projektart.wert(), pr.kind.as_str()),
                ("Bauort", kp.bauort.wert(), pr.place.as_str()),
                ("Projekt-Nr.", kp.projektnummer.wert(), pr.number.as_str()),
                ("Bauherr", kp.bauherr.wert(), pr.client.as_str()),
                (
                    "Anschrift Bauherr",
                    kp.bauherr_anschrift.wert(),
                    pr.client_addr.as_str(),
                ),
                ("Aufsteller", kp.aufsteller.wert(), pr.author.as_str()),
                (
                    "Anschrift Aufsteller",
                    kp.aufsteller_anschrift.wert(),
                    pr.author_addr.as_str(),
                ),
            ];
            let csv = String::from_utf8(v.csv()).unwrap();
            for (feld, im_kopf, wert) in soll {
                if wert.is_empty() {
                    continue;
                }
                assert_eq!(im_kopf, Some(wert), "{name}: {k}: Kopf {feld}");
                let erste = wert.lines().next().unwrap();
                let z = csv
                    .split("\r\n")
                    .find(|z| z.starts_with(&format!("{feld};")))
                    .unwrap_or_else(|| panic!("{name}: {k}: CSV-Zeile {feld}"));
                assert!(z.contains(erste), "{name}: {k}: CSV {feld}: {z}");
            }
            stufen.push(jetzt);
        }
        assert!(!v.bauherr_fehlt(), "{name}");
        // Bezeichnung ohne Personendaten (§2, §6.8)
        let l = s.undo_label().unwrap();
        for w in ["Mustermann", "Horstmann", "Musterweg", "01/26", "02/26"] {
            assert!(!l.contains(w), "{name}: {l}");
        }

        // Speichern und Laden bytegleich, mit Zeilenumbrüchen
        let text_neu = stufen.last().unwrap().clone();
        let m2 = lesen(&text_neu);
        let p2 = m2.project();
        assert_eq!(p2.client, "Max Mustermann", "{name}");
        assert_eq!(p2.place, "Musterweg 1\n27777 Ganderkesee", "{name}");
        assert_eq!(p2.number, "02/26", "{name}");
        assert_eq!(
            p2.author_addr, "Denkmalsweg 18b\n27777 Ganderkesee",
            "{name}"
        );
        assert_eq!(
            sk_model::szo::write(&m2),
            text_neu,
            "{name}: Laden bytegleich"
        );

        // Jeder Schritt einzeln zurück, bytegleich; zuletzt die alte Datei
        for i in (0..3).rev() {
            assert!(s.undo(), "{name}");
            assert_eq!(
                sk_model::szo::write(s.model()),
                stufen[i],
                "{name}: Stufe {i}"
            );
        }
        assert!(s.undo_label().is_none(), "{name}");
        v.sync(&mut s, None);
        assert!(v.bauherr_fehlt(), "{name}: nach Strg+Z wieder leer");
    }
}

#[test]
fn abnahme_ka4c_untertitel_ein_schritt_und_gespeichert() {
    for (name, text) in haeuser() {
        let mut s = Scene::with_model(lesen(text));
        let mut v = blatt(&mut s);
        let vorher = sk_model::szo::write(s.model());
        assert!(!vorher.contains("lvstorey=1"), "{name}");
        gliedern(&mut s, true);
        assert_eq!(s.undo_label(), Some("LV nach Geschossen gegliedert"));
        let nachher = sk_model::szo::write(s.model());
        assert!(nachher.contains("lvstorey=1"), "{name}");
        v.sync(&mut s, None);
        assert!(v.untertitel);
        // Laden: wieder gesetzt
        let mut s2 = Scene::with_model(lesen(&nachher));
        let mut v2 = blatt(&mut s2);
        v2.sync(&mut s2, None);
        assert!(v2.untertitel, "{name}: nach Laden");
        assert!(s.undo());
        assert_eq!(
            sk_model::szo::write(s.model()),
            vorher,
            "{name}: Strg+Z bytegleich"
        );
        assert!(s.undo_label().is_none(), "{name}: ein Schritt");
    }
}
