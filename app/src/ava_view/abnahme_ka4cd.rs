//! Abnahme KA-4c/KA-4d (paket-ka4 §4, §6 Nr. 3, 4, 10, 11; Koordinator
//! 15:21) an RH-1 bis RH-3:
//! - CSV jedes Loses: „Summe netto“ = Netto des LV = Summe der Titel; die
//!   CSV aller Lose zusammen = Netto des Kostenblatts auf den Cent (ohne
//!   Untertitel); mit Untertiteln höchstens die Rundung je
//!   Untertitelposition daneben (0,0005 × EP + 0,5 Cent).
//! - OZ in der CSV je Los eindeutig, mit und ohne Untertitel.
//! - Kopf: Bauvorhaben, Bauherr, Aufsteller je ein Rückgängig-Schritt;
//!   Strg+Z bytegleich; Speichern und Laden behalten die Werte; Kopf
//!   öffnen und Esc ändert nichts.
//! - „Geschosse als Untertitel“: ein Schritt, `lvstorey=1` in der Datei,
//!   nach Laden wieder gesetzt, Strg+Z bytegleich.

use super::*;
use sk_cost::Umfang;
use sk_platform::{Key, Modifiers};
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

fn leer() -> Fonts {
    Fonts {
        regular: None,
        bold: None,
        italic: None,
    }
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

#[test]
fn abnahme_ka4c_kopf_je_ein_schritt() {
    let t = Theme::dark();
    let fonts = leer();
    let mods = Modifiers::default();
    for (name, text) in haeuser() {
        let mut s = Scene::with_model(lesen(text));
        let mut v = blatt(&mut s);
        let anfang = sk_model::szo::write(s.model());
        // Kopf auf, Feld auf, Esc: nichts geändert
        v.kopf_klick(&t, &fonts, Some(Hot::Kopf), 0.0);
        v.kopf_klick(&t, &fonts, Some(Hot::Feld(kopf::Feld::Bauherr)), 0.0);
        v.text('X');
        v.key(Key::Escape, mods);
        assert!(s.undo_label().is_none(), "{name}: Esc ohne Schritt");
        assert_eq!(sk_model::szo::write(s.model()), anfang, "{name}");
        let werte = [
            (
                kopf::Feld::Bauvorhaben,
                "Haus Sonnenweg 3",
                "Bauvorhaben gesetzt",
            ),
            (kopf::Feld::Bauherr, "Familie Muster", "Bauherr gesetzt"),
            (
                kopf::Feld::Aufsteller,
                "Muster Bau GmbH",
                "Aufsteller gesetzt",
            ),
        ];
        let mut stufen = vec![anfang.clone()];
        for (f, wert, label) in werte {
            v.kopf_klick(&t, &fonts, Some(Hot::Feld(f)), 0.0);
            v.key(Key::Char('A'), Modifiers { ctrl: true, ..mods });
            for ch in wert.chars() {
                v.text(ch);
            }
            let Some(ListOut::Kosten(crate::kosten_view::Schreiben::Projekt { projekt, label: l })) =
                v.key(Key::Enter, mods)
            else {
                panic!("{name}: {f:?} kein Schritt");
            };
            assert_eq!(l, label);
            assert!(s.projekt_setzen(l, projekt));
            assert_eq!(s.undo_label(), Some(label), "{name}");
            v.sync(&mut s, None);
            stufen.push(sk_model::szo::write(s.model()));
        }
        let p = s.model().project().clone();
        assert_eq!(
            (p.site.as_str(), p.client.as_str(), p.author.as_str()),
            ("Haus Sonnenweg 3", "Familie Muster", "Muster Bau GmbH")
        );
        let lv = v.lv.as_deref().unwrap();
        assert_eq!(lv.kopf.bauherr.as_deref(), Some("Familie Muster"));
        assert!(!v.bauherr_fehlt());
        // Speichern und Laden
        let text_neu = stufen.last().unwrap().clone();
        let m2 = lesen(&text_neu);
        assert_eq!(m2.project().client, "Familie Muster", "{name}");
        assert_eq!(m2.project().site, "Haus Sonnenweg 3");
        assert_eq!(m2.project().author, "Muster Bau GmbH");
        assert_eq!(
            sk_model::szo::write(&m2),
            text_neu,
            "{name}: Laden bytegleich"
        );
        // Jeder Schritt einzeln zurück, bytegleich
        for i in (0..3).rev() {
            assert!(s.undo(), "{name}");
            assert_eq!(
                sk_model::szo::write(s.model()),
                stufen[i],
                "{name}: Stufe {i}"
            );
        }
        assert!(s.undo_label().is_none());
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
