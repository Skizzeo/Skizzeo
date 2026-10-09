//! Tests der Kostenansicht (ausgelagert, R4: Dateien unter 3.000 Zeilen).

use super::*;
use std::time::Instant;

fn haus() -> Scene {
    let m = sk_model::szo::read_with(
        include_str!("../../../crates/sk-cost/referenz/rh1-standardhaus.szo"),
        sk_model::GuidGen::with_seed(1),
        &sk_cost::lesen::ABSCHNITTE_SZO,
    )
    .expect("lädt")
    .model;
    Scene::with_model(m)
}

/// Betrag aus einem angezeigten Text „1.234,56“.
fn cent(text: &str) -> i64 {
    let t = text.replace(['.', '−'], "").replace(',', "");
    let v: i64 = t.parse().unwrap_or_else(|_| panic!("{text}"));
    if text.starts_with('−') {
        -v
    } else {
        v
    }
}

/// Menge in Tausendsteln aus „172,224 m²“.
fn milli(text: &str) -> i64 {
    let n = text.split(' ').next().unwrap();
    n.replace(['.', ','], "").parse().unwrap()
}

/// Abnahme 6 (Anzeige rechnet nach): In jeder Zeile GP = angezeigte
/// Menge × angezeigter EP auf den Cent; jede Summe = Summe der
/// angezeigten Zeilen; das Netto gleich in jeder Gliederung, mit
/// Rundungsausgleich; beide Modi.
#[test]
fn anzeige_rechnet_nach() {
    let mut s = haus();
    let mut v = KostenView::new();
    v.sync(&mut s, None);
    let netto_voll = v.netto().unwrap();
    assert_eq!(netto_voll, Cent(6_008_983));
    for modus in Modus::ALLE {
        for g in Gliederung::ALLE {
            v.modus = modus;
            v.gliederung = g;
            v.sync(&mut s, None);
            let z = v.zeilen();
            let netto = v.netto().unwrap();
            // GP = Menge × EP
            for x in z.iter().filter(|x| matches!(x.art, Art::Position { .. })) {
                if x.gp.is_empty() {
                    continue;
                }
                let soll = (milli(&x.menge) as i128 * cent(&x.ep) as i128 + 500).div_euclid(1000);
                assert_eq!(cent(&x.gp) as i128, soll, "{modus:?} {g:?} {x:?}");
            }
            // Gruppen = Summe der Zeilen darunter
            for (i, x) in z.iter().enumerate() {
                if x.art != Art::Gruppe || x.betrag.is_none() {
                    continue;
                }
                let kinder: i64 = z[i + 1..]
                    .iter()
                    .take_while(|k| k.ebene > x.ebene || k.art == Art::Ansatz)
                    .filter(|k| k.ebene == x.ebene + 1)
                    .filter_map(|k| k.betrag)
                    .map(|c| c.0)
                    .sum();
                assert_eq!(cent(&x.gp), kinder, "{modus:?} {g:?} {}", x.text);
            }
            // Netto = Gruppen der obersten Ebene + Ausgleich
            let oben: i64 = z
                .iter()
                .filter(|x| x.ebene == 0)
                .filter_map(|x| x.betrag)
                .map(|c| c.0)
                .sum();
            assert_eq!(oben, netto.0, "{modus:?} {g:?}");
        }
    }
    // Gliederung Gewerk ohne Ausgleich; Geschoss mit −0,01 / +0,03 je nach Blatt
    v.modus = Modus::Voll;
    v.gliederung = Gliederung::Geschoss;
    v.sync(&mut s, None);
    let ausgleich = v
        .zeilen()
        .iter()
        .find(|x| x.art == Art::Ausgleich)
        .map(|x| x.betrag);
    assert_eq!(
        ausgleich.flatten(),
        Some(v.blatt().unwrap().ausgleich_geschoss).filter(|c| c.0 != 0)
    );
}

/// Abnahme 1 und 2 (Gliederungen): KG mit 322, 331, 335, 351 und den
/// Zwischensummen der 2. Ebene; Geschoss von unten; Chip-Summe gleich
/// dem Blatt mit nur diesem Geschoss.
#[test]
fn gliederungen_und_chips() {
    let mut s = haus();
    let mut v = KostenView::new();
    v.gliederung = Gliederung::Kostengruppe;
    v.sync(&mut s, None);
    let gruppen: Vec<&str> = v
        .zeilen()
        .iter()
        .filter(|z| z.art == Art::Gruppe)
        .map(|z| z.text.as_str())
        .collect();
    for k in [
        "320 Gründung",
        "322 Flachgründungen und Bodenplatten",
        "331 Tragende Außenwände",
        "335 Außenwandbekleidungen, außen",
        "350 Decken",
        "351 Deckenkonstruktionen",
    ] {
        assert!(gruppen.contains(&k), "{k}: {gruppen:?}");
    }
    v.gliederung = Gliederung::Geschoss;
    v.sync(&mut s, None);
    let oben: Vec<&str> = v
        .zeilen()
        .iter()
        .filter(|z| z.art == Art::Gruppe && z.ebene == 0)
        .map(|z| z.text.as_str())
        .collect();
    assert_eq!(oben, ["Gründung", "Erdgeschoss", "Obergeschoss"]);
    // Chip EG: Summe gleich der Geschosszeile und dem Blatt nur mit EG
    let summen = v.leiste.summen.clone();
    assert_eq!(summen.len(), 3);
    let eg = v
        .zeilen()
        .iter()
        .find(|z| z.text == "Erdgeschoss")
        .unwrap()
        .betrag
        .unwrap();
    assert_eq!(summen[1], euro_ganz(eg));
    let c = v.leiste.chips().to_vec();
    assert!(umfang_view::klick(&mut v.leiste.umfang, &c, 1, true));
    v.sync(&mut s, None);
    assert_eq!(v.netto(), Some(eg));
    assert!(v.subtitle.contains(" · EG · Stand "), "{}", v.subtitle);
    assert!(
        v.subtitle.ends_with(" · Referenzpreise 10/2026"),
        "{}",
        v.subtitle
    );
    // abgewählte Chips behalten ihre Summe
    assert_eq!(v.leiste.summen, summen);
}

/// Zeilen ohne Bauleistung stehen grau unter ihrem Gewerk, die
/// Gewerkzeile sagt „ohne Bauleistung“; die Fußzeile nennt sie.
#[test]
fn graue_zeilen_unter_dem_gewerk() {
    let mut s = haus();
    let mut v = KostenView::new();
    v.sync(&mut s, None);
    let z = v.zeilen();
    let i = z
        .iter()
        .position(|x| x.art == Art::Ohne)
        .expect("graue Zeile");
    let g = z[..i].iter().rev().find(|x| x.art == Art::Gruppe).unwrap();
    assert!(g.text.contains("Dach"), "{}", g.text);
    assert_eq!(g.gp, "ohne Bauleistung");
    let f = v.fuss();
    assert!(
        f.iter().any(
            |x| x.text == "Ohne Preis: " && x.verweis == "Dachterrasse, Attikablech (3 Zeilen)"
        ),
        "{:?}",
        f.iter().map(|x| &x.verweis).collect::<Vec<_>>()
    );
}

#[test]
fn zahlen_und_csv() {
    assert_eq!(menge_text(Dez(172_224_000), Einheit::M2), "172,224 m²");
    assert_eq!(menge_text(Dez(1_172_224_000), Einheit::M3), "1.172,224 m³");
    assert_eq!(euro_ganz(Cent(6_008_983)), "60.090 €");
    assert_eq!(prozent(Cent(2_894_149), Cent(6_008_983)), Some(48));
    let mut s = haus();
    let mut v = KostenView::new();
    v.sync(&mut s, None);
    let csv = String::from_utf8(v.csv("Standardhaus", "08.10.2026")).unwrap();
    assert!(csv.starts_with("\u{feff}Projekt;Standardhaus\r\n"), "{csv}");
    assert!(csv.contains("\r\nNetto;60089,83\r\n"), "{csv}");
    assert!(csv.contains("\r\nModus;Material + Lohn\r\n"));
    // OZ mit Los, Frostschürze unter 1.01.0020; Zeilen ohne Bauleistung
    // ohne OZ, Menge und Einheit getrennt
    // OZ als Text für Excel: ="1.01.0020"
    assert!(
        csv.contains(";\"=\"\"1.01.0020\"\"\";Frostschürze"),
        "{csv}"
    );
    let kopf = csv.lines().find(|l| l.starts_with("Gliederung;")).unwrap();
    let spalten = kopf.split(';').count();
    assert_eq!(spalten, 14);
    for l in csv
        .lines()
        .filter(|l| l.contains(';'))
        .skip_while(|l| !l.starts_with("Gliederung;"))
    {
        if l.starts_with("Netto;") {
            break;
        }
        assert_eq!(l.split(';').count(), spalten, "{l}");
    }
}

/// Abnahme 11 (soll-ka-2c): Doppelklick auf den EP öffnet das
/// Preisblatt; „20,5“ getippt zeigt den EP 52,34 live in der Zeile,
/// ohne das Modell zu ändern; Enter liefert genau ein `PreisSetzen`.
/// Danach trägt die Zeile den Punkt, der Tooltip nennt den Firmenwert und
/// die CSV „ja“ in der Spalte Projektabweichung.
#[test]
fn preisblatt_live_und_punkt() {
    let t = Theme::dark();
    let fonts = Fonts {
        regular: None,
        bold: None,
        italic: None,
    };
    let mut s = haus();
    let mut v = KostenView::new();
    (v.w, v.h) = (1200, 900);
    v.sync(&mut s, None);
    let rev = s.model().revision();
    let i = v
        .zeilen()
        .iter()
        .position(|z| z.text.contains("Porenbeton") && z.text.contains("17,5"))
        .expect("Mauerwerk");
    let (_, y, h) = v
        .sichtbar()
        .into_iter()
        .find(|(j, _, _)| *j == i)
        .expect("sichtbar");
    let (x0, cw) = v.content_x(&t);
    let (x, y) = ((col_ep(x0, cw) - 20.0) as f64, (y + h * 0.5) as f64);
    let mut p = Picking::default();
    let mods = sk_platform::Modifiers::default();
    v.mouse_down(&t, &fonts, &mut p, (x, y), mods);
    assert!(!v.preis_offen());
    assert_eq!(
        v.mouse_down(&t, &fonts, &mut p, (x, y), mods),
        Some(ListOut::Repaint)
    );
    v.sync(&mut s, None);
    assert!(v.preis_offen());
    assert_eq!(v.zeilen()[i].ep, "54,00");
    for ch in "20,5".chars() {
        assert_eq!(v.text(ch), Some(ListOut::Repaint));
    }
    v.sync(&mut s, None);
    assert_eq!(v.zeilen()[i].ep, "52,34");
    assert_eq!(v.zeilen()[i].gp, "9.014,20");
    assert_eq!(s.model().revision(), rev, "Vorschau schreibt nichts");
    let out = v.key(&t, sk_platform::Key::Enter, mods);
    let Some(Some(ListOut::Kosten(Schreiben::Preis { ops, gilt, .. }))) = out else {
        panic!("{out:?}");
    };
    assert_eq!(gilt, Gilt::NurHaus);
    assert_eq!(ops.len(), 1);
    assert!(!v.preis_offen());
    let herkunft = sk_cost::Herkunft::neu(sk_cost::HerkunftArt::Manual, "2026-10-08", "12:00");
    s.kosten_folge("Preis im Projekt geändert", None, &herkunft, &ops)
        .unwrap();
    v.sync(&mut s, None);
    assert_eq!(v.zeilen()[i].ep, "52,34");
    let pos = v.zeilen()[i].pos.unwrap().0;
    assert_eq!(v.eigen.get(&pos), Some(&Cent(5_400)));
    assert_eq!(
        v.tip_at(&t, &fonts, x, y).as_deref(),
        Some("im Projekt geändert, Firma 54,00")
    );
    let csv = String::from_utf8(v.csv("Standardhaus", "08.10.2026")).unwrap();
    let zeile = csv
        .lines()
        .find(|l| l.contains("Porenbeton") && l.contains("17,5"))
        .unwrap();
    assert!(zeile.ends_with(";ja"), "{zeile}");
    // Esc verwirft: wieder öffnen, tippen, Esc, nichts geändert
    v.mouse_down(&t, &fonts, &mut p, (x, y), mods);
    v.mouse_down(&t, &fonts, &mut p, (x, y), mods);
    v.sync(&mut s, None);
    v.text('1');
    v.sync(&mut s, None);
    assert_eq!(
        v.key(&t, sk_platform::Key::Escape, mods),
        Some(Some(ListOut::Repaint))
    );
    v.sync(&mut s, None);
    assert_eq!(v.zeilen()[i].ep, "52,34");
}

/// KA-2c2 (Abnahme 12): „Auch für neue Häuser“ schreibt die Firma als
/// neuen Stand. Ohne Projektkopie gibt es keinen Projektschritt; mit
/// Kopie einen, mit Wert im Namen, und Strg+Z nimmt nur ihn zurück.
#[test]
fn auch_fuer_neue_haeuser() {
    let d = std::env::temp_dir().join(format!("skizzeo-kv-firma-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    let (mut c, _) = crate::catalog::Company::load(&d.join("firmenkatalog.szk"), true);
    let mut s = haus();
    let h = sk_cost::Herkunft::neu(sk_cost::HerkunftArt::Manual, "2026-10-08", "12:00");
    let ep = |s: &mut Scene, c: &crate::catalog::Company, firma_allein: bool| {
        let f = Some((c.library(), c.stand()));
        let k = if firma_allein {
            s.firmenkatalog(f)
        } else {
            s.katalog(f)
        };
        let b = s.kostenblatt(f, &Umfang::projekt());
        let p = b
            .positionen
            .iter()
            .find(|p| p.kurz.contains("Porenbeton") && p.kurz.contains("17,5"))
            .unwrap();
        sk_cost::preis::aufbau(s.model(), &k, p).unwrap()
    };
    let a = ep(&mut s, &c, false);
    let stein = a.stoffe.iter().find(|t| t.haupt).unwrap().artikel.unwrap();
    let k = s.katalog(Some((c.library(), c.stand())));
    let setze = |x: &str| {
        sk_cost::preis::preis_ops(
            &k,
            &a,
            &sk_cost::preis::Eingabe {
                stunden: None,
                preise: vec![(stein, Dez::lesen(x, 4).unwrap())],
                ..Default::default()
            },
            "10/2026",
        )
    };
    // Ohne Projektkopie: Firma und dieses Haus, ein Schritt; Strg+Z nimmt
    // ihn nur für dieses Haus zurück, nie den Bauschritt davor
    let vorher = s.undo_label();
    let label = s.bezeichnung("Planstein 20,50 €/m² für dieses und neue Häuser".into());
    assert_eq!(
        s.fuer_firma_auch_hier(label, &mut c, &h, &setze("20.5")),
        Ok(None)
    );
    assert_eq!(s.undo_label(), Some(label));
    assert_eq!(ep(&mut s, &c, false).ep, Cent(5_234));
    assert_eq!(ep(&mut s, &c, true).ep, Cent(5_234));
    assert!(s.undo());
    assert_eq!(s.undo_label(), vorher, "der Bauschritt davor bleibt");
    assert_eq!(ep(&mut s, &c, false).ep, Cent(5_400));
    assert_eq!(ep(&mut s, &c, true).ep, Cent(5_234));
    assert!(s.redo());
    assert_eq!(ep(&mut s, &c, false).ep, Cent(5_234));
    // Nur dieses Haus: 21,00 im Projekt, die Firma bleibt bei 20,50
    let f = Some(c.library());
    s.kosten_folge("Preis im Projekt geändert", f, &h, &setze("21"))
        .unwrap();
    assert_eq!(ep(&mut s, &c, false).ep, Cent(5_284));
    assert_eq!(ep(&mut s, &c, true).ep, Cent(5_234));
    // Auch für neue Häuser mit Kopie: Firma und Projekt 19,00, ein Schritt
    let label = s.bezeichnung("Planstein 19,00 €/m² für dieses und neue Häuser".into());
    assert_eq!(
        s.fuer_firma_auch_hier(label, &mut c, &h, &setze("19")),
        Ok(None)
    );
    assert_eq!(s.undo_label(), Some(label));
    assert_eq!(ep(&mut s, &c, false).ep, Cent(5_084));
    assert_eq!(ep(&mut s, &c, true).ep, Cent(5_084));
    // Strg+Z: nur das Projekt zurück, die Firma behält 19,00
    assert!(s.undo());
    assert_eq!(ep(&mut s, &c, false).ep, Cent(5_284));
    assert_eq!(ep(&mut s, &c, true).ep, Cent(5_084));
    // Gleicher Wortlaut, gleiche Bezeichnung
    assert!(std::ptr::eq(
        s.bezeichnung("Planstein 19,00 €/m² für dieses und neue Häuser".into()),
        label
    ));
    let _ = std::fs::remove_dir_all(&d);
}

/// Abgleichzeile: Firma auf Lohn 65 nach der Projektkopie; „übernehmen“
/// mit Netto im Tooltip, die Liste schiebt sich unter die Zeile.
#[test]
fn abgleichzeile_fuer_neue_haeuser() {
    let t = Theme::dark();
    let fonts = Fonts {
        regular: None,
        bold: None,
        italic: None,
    };
    let d = std::env::temp_dir().join(format!("skizzeo-kv-abgleich-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    let (mut c, _) = crate::catalog::Company::load(&d.join("firmenkatalog.szk"), true);
    let mut s = haus();
    let h = sk_cost::Herkunft::neu(sk_cost::HerkunftArt::Manual, "2026-10-08", "12:00");
    let lohn = |v: i64| sk_cost::Op::FirmenwertSetzen {
        schluessel: "wage".into(),
        wert: Dez::ganz(v),
    };
    c.fuer_firma(&h, &[lohn(60)]).unwrap();
    let mut v = KostenView::new();
    (v.w, v.h) = (1200, 900);
    v.sync(&mut s, Some((c.library(), c.stand())));
    assert_eq!(v.abgleich_zeile(), None, "ohne Kopie");
    let oben = v.list_top();
    // Projektkopie über einen eigenen Preis, dann Lohn 65 in der Firma
    let k = s.katalog(Some((c.library(), c.stand())));
    let art = k.artikel.iter().find(|a| a.preis.is_some()).unwrap().guid;
    let preis = Op::PreisSetzen {
        artikel: art,
        preis: Some(Dez::ganz(99)),
        stand: "10/2026".into(),
        quelle: "Preisblatt".into(),
        eingabe: String::new(),
    };
    s.kosten_folge("Preis", Some(c.library()), &h, &[preis])
        .unwrap();
    c.fuer_firma(&h, &[lohn(65)]).unwrap();
    v.sync(&mut s, Some((c.library(), c.stand())));
    assert_eq!(
        v.abgleich_zeile().as_deref(),
        Some("Für neue Häuser gilt Lohn 65,00 €/h (hier 60,00)")
    );
    assert_eq!(v.list_top(), oben + ABGLEICH_H * v.scale);
    let lage = v.abgleich_lage(&t, &fonts).unwrap();
    let ((ux, uy, uw, uh), (lx, ..)) = (lage.uebernehmen, lage.lassen);
    let (x, y) = ((ux + uw * 0.5) as f64, (uy + uh * 0.5) as f64);
    // Die Vorschau entsteht erst über „übernehmen“ (Review 3ak)
    let mut p = Picking::default();
    v.mouse_move(&t, &fonts, &mut p, x, y);
    assert!(v
        .tip_at(&t, &fonts, x, y)
        .unwrap()
        .starts_with("Rechnet mit"));
    v.sync(&mut s, Some((c.library(), c.stand())));
    let tip = v.tip_at(&t, &fonts, x, y).unwrap();
    assert!(tip.starts_with("Mit den Werten für neue Häuser: "), "{tip}");
    assert!(tip.ends_with("netto · Eigene Werte dieses Hauses bleiben stehen."));
    let mods = sk_platform::Modifiers::default();
    let lassen = v.mouse_down(&t, &fonts, &mut p, ((lx + 2.0) as f64, y), mods);
    assert!(matches!(
        lassen,
        Some(ListOut::Kosten(Schreiben::Lassen(_)))
    ));
    // KA-3a5: „Unterschiede ansehen“ öffnet das Blatt; abweichend, der
    // eigene Preis bleibt, Zahl der gleichen Einträge
    assert!(!v.unterschiede_offen());
    let (ax, ay) = v.ansehen_mitte(&t, &fonts).unwrap();
    assert_eq!(
        v.mouse_down(&t, &fonts, &mut p, (ax, ay), mods),
        Some(ListOut::Repaint)
    );
    assert!(v.unterschiede_offen());
    let z = v.unterschiede_zeilen();
    assert_eq!(
        z[..3],
        [
            // ohne „Stand n“ rechts (Bedienbarkeit 15)
            unterschiede::Zeile::Titel("Unterschiede zum Firmenkatalog".into(), String::new()),
            unterschiede::Zeile::Kopf("Abweichend", "„übernehmen“ holt diese Werte in dieses Haus"),
            unterschiede::Zeile::Eintrag("Lohn 65,00 €/h (hier 60,00)".into()),
        ]
    );
    assert_eq!(
        z[3],
        unterschiede::Zeile::Kopf("Eigener Wert dieses Hauses", "bleibt auch beim Übernehmen")
    );
    assert!(
        matches!(&z[4], unterschiede::Zeile::Eintrag(e) if e.ends_with("(hier 99,00)")),
        "{z:?}"
    );
    assert!(
        matches!(z.last(), Some(unterschiede::Zeile::Fuss(f)) if f.ends_with("Einträge gleich")),
        "{z:?}"
    );
    // Klick auf das Blatt tut nichts und trifft nichts darunter
    let ((bx, by, bw, bh), _) = v.unterschiede_lage(&t, &fonts).unwrap();
    let mitte = ((bx + bw * 0.5) as f64, (by + bh * 0.5) as f64);
    assert_eq!(v.tip_at(&t, &fonts, mitte.0, mitte.1), None);
    assert_eq!(v.mouse_down(&t, &fonts, &mut p, mitte, mods), None);
    assert!(v.unterschiede_offen());
    // Esc schließt, nochmals öffnen, Klick daneben schließt
    assert_eq!(
        v.key(&t, sk_platform::Key::Escape, mods),
        Some(Some(ListOut::Repaint))
    );
    assert!(!v.unterschiede_offen());
    v.mouse_down(&t, &fonts, &mut p, (ax, ay), mods);
    assert!(v.unterschiede_offen());
    let daneben = ((bx + bw + 40.0) as f64, (by + bh + 40.0) as f64);
    assert_eq!(
        v.mouse_down(&t, &fonts, &mut p, daneben, mods),
        Some(ListOut::Repaint)
    );
    assert!(!v.unterschiede_offen());
    // „übernehmen“ wirkt auch bei offenem Blatt und schließt es
    v.mouse_down(&t, &fonts, &mut p, (ax, ay), mods);
    assert!(v.unterschiede_offen());
    let Some(ListOut::Kosten(Schreiben::Uebernehmen(saetze))) =
        v.mouse_down(&t, &fonts, &mut p, (x, y), mods)
    else {
        panic!("übernehmen");
    };
    assert!(!v.unterschiede_offen());
    let op = Op::StandUebernehmen { saetze };
    s.kosten_folge("Übernommen", Some(c.library()), &h, &[op])
        .unwrap();
    v.sync(&mut s, Some((c.library(), c.stand())));
    assert_eq!(v.abgleich_zeile(), None);
    assert_eq!(v.list_top(), oben);
    let _ = std::fs::remove_dir_all(&d);
}

/// Graue Zeile: überfahren zeigt „Bauleistung wählen …“, Klick öffnet das
/// Blatt, Klick auf einen Eintrag schreibt `BauleistungZuordnen`; danach
/// ist die Zeile eine Position.
#[test]
fn bauleistung_waehlen() {
    let t = Theme::dark();
    let fonts = Fonts {
        regular: None,
        bold: None,
        italic: None,
    };
    let mut s = haus();
    let mut v = KostenView::new();
    (v.w, v.h) = (1200, 1400);
    v.sync(&mut s, None);
    let grau = v.blatt().unwrap().ohne.len();
    let i = v
        .zeilen()
        .iter()
        .position(|z| z.ohne.is_some_and(|j| v.waehlbar.contains(&j)))
        .expect("graue Zeile mit Wahl");
    let (_, y, h) = v
        .sichtbar()
        .into_iter()
        .find(|(j, _, _)| *j == i)
        .expect("sichtbar");
    let (lx, ly, lw, lh) = v.waehlen_rect(&t, &fonts, y, h);
    let (x, y) = ((lx + lw * 0.5) as f64, (ly + lh * 0.5) as f64);
    let mut p = Picking::default();
    let mods = sk_platform::Modifiers::default();
    v.mouse_move(&t, &fonts, &mut p, x, y);
    assert_eq!(v.hot, Some(Hot::Waehlen(i)));
    assert_eq!(
        v.mouse_down(&t, &fonts, &mut p, (x, y), mods),
        Some(ListOut::Repaint)
    );
    v.sync(&mut s, None);
    assert!(v.blatt_offen());
    // Enter mit Suchtext, der genau einen Treffer lässt
    let w = v.wahl.as_ref().unwrap();
    let erste = w_erste(w);
    for ch in erste.chars() {
        v.text(ch);
    }
    let out = v.key(&t, sk_platform::Key::Enter, mods);
    let Some(Some(ListOut::Kosten(Schreiben::Bauleistung { op, hinweis }))) = out else {
        panic!("{out:?}");
    };
    assert!(!v.blatt_offen());
    // Bedienbarkeit 8.6: Hinweis nur beim Wechsel des Gewerks
    if let Some(t) = &hinweis {
        assert!(
            t.contains(" gehört jetzt zum Gewerk ") && t.ends_with(")."),
            "{t}"
        );
    }
    let h = sk_cost::Herkunft::neu(sk_cost::HerkunftArt::Manual, "2026-10-08", "12:00");
    s.kosten_folge("Bauleistung gewählt", None, &h, &[*op])
        .unwrap();
    v.sync(&mut s, None);
    assert!(v.blatt().unwrap().ohne.len() < grau);
    // die Liste steht auf der neuen Position und markiert sie
    let now = Instant::now();
    let j = (0..v.zeilen().len())
        .find(|j| v.blitzt(*j, now))
        .expect("neue Position markiert");
    assert!(v.sichtbar().iter().any(|(k, _, _)| *k == j), "in Sicht");
    assert!(v.tick(&t, now));
    assert!(!v.blitzt(j, now + BLITZ));
    assert!(s.undo());
    v.sync(&mut s, None);
    assert_eq!(v.blatt().unwrap().ohne.len(), grau);
}

/// Lohnfeld: die Hinweiskarte schreibt mit Enter für dieses und neue
/// Häuser; der Stundenlohn in der Kachel öffnet das Blatt, Esc schließt.
#[test]
fn lohnfeld_karte_und_kachel() {
    let t = Theme::dark();
    let fonts = Fonts {
        regular: None,
        bold: None,
        italic: None,
    };
    let mut s = haus();
    let mut v = KostenView::new();
    (v.w, v.h) = (1200, 900);
    v.lohn_karte();
    v.sync(&mut s, None);
    assert!(v.blatt_offen());
    let mods = sk_platform::Modifiers::default();
    for ch in "65".chars() {
        v.text(ch);
    }
    let out = v.key(&t, sk_platform::Key::Enter, mods);
    assert_eq!(
        out,
        Some(Some(ListOut::Kosten(Schreiben::Lohn {
            wert: Dez::ganz(65),
            gilt: Gilt::NeueHaeuser
        })))
    );
    assert!(!v.blatt_offen());
    // Kachel Lohnanteil: „60,00 €/h“ ist der Verweis
    let (_, link, _, (x, y, w, h)) = v.lohnsatz_lage(&t, &fonts).unwrap();
    assert_eq!(link, "60,00 €/h");
    let (x, y) = ((x + w * 0.5) as f64, (y + h * 0.5) as f64);
    let mut p = Picking::default();
    v.mouse_move(&t, &fonts, &mut p, x, y);
    assert_eq!(
        v.tip_at(&t, &fonts, x, y).as_deref(),
        Some("Stundenlohn ändern")
    );
    assert_eq!(
        v.mouse_down(&t, &fonts, &mut p, (x, y), mods),
        Some(ListOut::Repaint)
    );
    v.sync(&mut s, None);
    assert!(v.blatt_offen());
    assert_eq!(
        v.key(&t, sk_platform::Key::Escape, mods),
        Some(Some(ListOut::Repaint))
    );
    assert!(!v.blatt_offen());
}

/// Kurzname des ersten wählbaren Eintrags, der eindeutig ist.
fn w_erste(w: &WahlBlatt) -> String {
    w.erster_eindeutig().expect("ein eindeutiger Name")
}

/// Bedienbarkeit 11.1, 11.2, 11.4: Über den Kacheln steht, wofür die
/// Summen gelten; bei niedrigem Fenster werden die Kacheln eine Zeile und
/// die Liste behält mindestens drei Zeilen; bei offenem Preisblatt ist nur
/// die bearbeitete Position markiert, nicht das WDVS derselben Wände.
#[test]
fn summen_ueberschrift_niedrig_und_markierung() {
    let t = Theme::dark();
    let fonts = Fonts {
        regular: None,
        bold: None,
        italic: None,
    };
    let mut s = haus();
    let mut v = KostenView::new();
    (v.w, v.h) = (1200, 900);
    v.sync(&mut s, None);
    assert!(!v.kompakt());
    assert!(
        v.summe_titel().starts_with("Summe · "),
        "{}",
        v.summe_titel()
    );
    assert!(!v.summe_titel().contains("Stand"));
    assert!(v.list_bottom() < v.tiles_top());
    // Niedriges Fenster (angedockt, Laptop): eine Zeile, Liste hat Vorrang;
    // mit Kacheln bleiben immer mindestens drei Zeilen
    let mut kompakt = 0;
    for h in (200..900).rev().step_by(4) {
        v.h = h;
        if v.kompakt() {
            kompakt += 1;
            assert!(v.lohnsatz_lage(&t, &fonts).is_none());
            if kompakt == 1 {
                assert!(v.view_h() >= 3.0 * ROW_POS, "{h}: {}", v.view_h());
            }
        } else {
            assert!(v.view_h() >= 3.0 * ROW_POS, "{h}: {}", v.view_h());
        }
    }
    assert!(kompakt > 0);
    v.h = 900;
    // Preisblatt am Planstein: Wände sind gewählt, markiert ist nur M10
    let i = v
        .zeilen()
        .iter()
        .position(|z| z.text.contains("Porenbeton") && z.text.contains("17,5"))
        .expect("Mauerwerk");
    let el = v.zeilen()[i].elements.clone();
    v.selected = el.clone();
    let andere: Vec<usize> = (0..v.zeilen().len())
        .filter(|j| *j != i && v.markiert(*j))
        .collect();
    assert!(!andere.is_empty(), "ohne Preisblatt alle Zeilen der Wände");
    let (_, y, h) = v
        .sichtbar()
        .into_iter()
        .find(|(j, _, _)| *j == i)
        .expect("sichtbar");
    let (x0, cw) = v.content_x(&t);
    let (x, y) = ((col_ep(x0, cw) - 20.0) as f64, (y + h * 0.5) as f64);
    let mut p = Picking {
        selected: el,
        ..Default::default()
    };
    let mods = sk_platform::Modifiers::default();
    v.mouse_down(&t, &fonts, &mut p, (x, y), mods);
    v.mouse_down(&t, &fonts, &mut p, (x, y), mods);
    v.sync(&mut s, None);
    assert!(v.preis_offen());
    let markiert: Vec<usize> = (0..v.zeilen().len()).filter(|j| v.markiert(*j)).collect();
    assert_eq!(markiert, [i]);
}

/// Review 3as (Regel 89): Stunden „für dieses und neue Häuser“ an einer
/// Bauleistung, die das Haus mit eigenem Gerät hält. Die Firma bekommt nur
/// die Stunden, nicht das Gerät des Hauses; das Haus behält sein Gerät und
/// bekommt die Stunden.
#[test]
fn fuer_neue_haeuser_nur_der_gewaehlte_wert() {
    let d = std::env::temp_dir().join(format!("skizzeo-kv-r89-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    let (mut c, _) = crate::catalog::Company::load(&d.join("firmenkatalog.szk"), true);
    let mut s = haus();
    let h = sk_cost::Herkunft::neu(sk_cost::HerkunftArt::Manual, "2026-10-08", "12:00");
    let f = Some((c.library(), c.stand()));
    let b = s.kostenblatt(f, &Umfang::projekt());
    let p = b
        .positionen
        .iter()
        .find(|p| p.kurz.contains("Porenbeton") && p.kurz.contains("17,5"))
        .unwrap()
        .clone();
    let k = s.katalog(f);
    let a = sk_cost::preis::aufbau(s.model(), &k, &p).unwrap();
    let l = k.leistung(a.leistung).unwrap().clone();
    let drei = Dez::lesen("3", 4).unwrap();
    let op = sk_cost::Op::BauleistungAendern {
        bauleistung: l.guid,
        daten: sk_cost::op::Bauleistung {
            geraet: drei,
            ..sk_cost::preis::bauleistung(&l)
        },
    };
    s.kosten_folge("Gerät im Projekt", Some(c.library()), &h, &[op])
        .unwrap();
    let k = s.katalog(Some((c.library(), c.stand())));
    let ops = sk_cost::preis::preis_ops(
        &k,
        &a,
        &sk_cost::preis::Eingabe {
            stunden: Some(Dez::lesen("0.9", 4).unwrap()),
            preise: vec![],
            eingaben: vec![],
            conv: vec![],
        },
        "10/2026",
    );
    let label = s.bezeichnung("Stunden 0,90 für dieses und neue Häuser".into());
    s.fuer_firma_auch_hier(label, &mut c, &h, &ops).unwrap();
    let f = Some((c.library(), c.stand()));
    let x = s.firmenkatalog(f).leistung(l.guid).unwrap().clone();
    assert_eq!(
        (x.stunden, x.geraet),
        (Dez::lesen("0.9", 4).unwrap(), l.geraet)
    );
    let y = s.katalog(f).leistung(l.guid).unwrap().clone();
    assert_eq!((y.stunden, y.geraet), (Dez::lesen("0.9", 4).unwrap(), drei));
    let _ = std::fs::remove_dir_all(&d);
}

/// paket-ka3b §3: Abläufe `kind=user` stehen leise links vom Knopf; ein
/// Klick startet den Ablauf für dieses Haus. Passt der Verweis nicht neben
/// den Titel, fehlt er.
#[test]
fn ablauf_verweis() {
    let t = Theme::dark();
    let fonts = Fonts {
        regular: None,
        bold: None,
        italic: None,
    };
    let mut s = haus();
    let mut v = KostenView::new();
    (v.w, v.h) = (1200, 900);
    v.sync(&mut s, None);
    assert!(v.ablauf_rects(&t, &fonts).is_empty());
    let g = sk_model::Guid(7);
    let liste = [(g, "Händlerpreis für dieses Haus eintragen".to_string())];
    assert!(v.set_ablaeufe(&liste));
    assert!(!v.set_ablaeufe(&liste), "unverändert");
    let r = v.ablauf_rects(&t, &fonts);
    assert_eq!(r.len(), 1);
    let (x, y, w, h) = r[0].1;
    let knopf = v.button_rect(&t, &fonts);
    assert!(x + w < knopf.0, "links vom Knopf");
    let (x, y) = ((x + w * 0.5) as f64, (y + h * 0.5) as f64);
    let mut p = Picking::default();
    let mods = sk_platform::Modifiers::default();
    assert_eq!(
        v.mouse_down(&t, &fonts, &mut p, (x, y), mods),
        Some(ListOut::Ablauf(g))
    );
    (v.w, v.h) = (420, 900);
    assert!(v.ablauf_rects(&t, &fonts).is_empty(), "zu schmal");
}

/// Review 3bf: Die Höhe der Liste wird mit den Zeilen gebaut, nicht bei
/// jedem Treffertest summiert; sie stimmt in jeder Gliederung und nach
/// dem Aufklappen aller Positionen.
#[test]
fn inhalt_h_mit_den_zeilen() {
    let mut s = haus();
    let mut v = KostenView::new();
    let summe = |v: &KostenView| v.zeilen.iter().map(KostenView::row_h).sum::<f32>() + 8.0;
    for g in Gliederung::ALLE {
        v.gliederung = g;
        v.sync(&mut s, None);
        assert_eq!(v.content_h(), summe(&v), "{g:?}");
    }
    let vorher = v.zeilen.len();
    let keys: Vec<u64> = v.zeilen.iter().map(|z| z.key).collect();
    v.offen.extend(keys);
    v.offen_stand += 1;
    v.sync(&mut s, None);
    assert!(v.zeilen.len() > vorher, "aufgeklappt");
    assert_eq!(v.content_h(), summe(&v));
}

/// Befund Q (Notiz Kopf §3): Bei 480 dip lief „Gliedern“ über den rechten
/// Rand. Von 480 bis 1920 dip (100, 125, 150 %) bleiben Titel, Knopf,
/// Chips, „Preise“ und „Gliedern“ im Inhalt und überdecken sich nicht;
/// rutscht „Gliedern“ unter „Preise“, rückt die Liste mit nach unten. Die
/// Lohnzeile bleibt in ihrer Kachel.
#[test]
fn kopf_ueberdeckt_sich_nie() {
    let lib = std::path::Path::new("/usr/share/fonts/truetype/liberation");
    let lade = |n: &str| {
        std::fs::read(lib.join(n))
            .ok()
            .and_then(sk_paint::font::Font::parse)
    };
    let fonts = Fonts {
        regular: lade("LiberationSans-Regular.ttf"),
        bold: lade("LiberationSans-Bold.ttf"),
        italic: None,
    };
    let Some(bold) = fonts.bold.as_ref() else {
        return;
    };
    let t = Theme::dark();
    let mut s = haus();
    let frei = |a: Rect, b: Rect| {
        a.0 + a.2 <= b.0 + 0.5
            || b.0 + b.2 <= a.0 + 0.5
            || a.1 + a.3 <= b.1 + 0.5
            || b.1 + b.3 <= a.1 + 0.5
    };
    let mut beide = [false, false];
    for scale in [1.0_f32, 1.25, 1.5] {
        for dip in (480..=1920).step_by(40) {
            let mut v = KostenView::new();
            v.scale = scale;
            (v.w, v.h) = ((dip as f32 * scale) as u32, (900.0 * scale) as u32);
            v.top = 120.0;
            v.sync(&mut s, None);
            let wo = format!("{dip} dip bei {scale}");
            let (x0, cw) = v.content_x(&t);
            let ((px, preise), (gx, gl)) = v.schalter(&t, &fonts);
            let pille = |lx: f32, r: Rect| (lx, r.1, r.0 + r.2 - lx, r.3);
            let (p_end, g_end) = (preise[preise.len() - 1].1, gl[gl.len() - 1].1);
            let mut teile = vec![
                (
                    "Titel",
                    (
                        x0,
                        v.top_px() + (TITLE_Y - 19.0) * scale,
                        bold.width("Kosten", 19.0 * scale),
                        22.0 * scale,
                    ),
                ),
                ("Knopf", v.button_rect(&t, &fonts)),
                ("Preise", pille(px, p_end)),
                ("Gliedern", pille(gx, g_end)),
            ];
            let chips = v.leiste.layout(&fonts, v.leiste_lage(&t));
            teile.extend(chips.chips.iter().map(|c| ("Chip", *c)));
            teile.extend(chips.field.map(|c| ("Gebäude", c)));
            teile.extend(chips.all.map(|c| ("Alle", c)));
            for (i, (na, a)) in teile.iter().enumerate() {
                assert!(
                    a.0 >= x0 - 0.5 && a.0 + a.2 <= x0 + cw + 0.5,
                    "{wo}: {na} {a:?} außerhalb {x0}..{}",
                    x0 + cw
                );
                for (nb, b) in &teile[i + 1..] {
                    assert!(frei(*a, *b), "{wo}: {na} {a:?} über {nb} {b:?}");
                }
            }
            let unten = g_end.1 > p_end.1;
            beide[usize::from(unten)] = true;
            assert_eq!(unten, v.zeile2() > 0.0, "{wo}");
            // Die Spaltenköpfe stehen unter den Schaltern
            let kopf = v.top_px() + (v.head() - 12.0 - 10.0) * scale;
            assert!(
                g_end.1 + g_end.3 <= kopf,
                "{wo}: Gliedern über den Spaltenköpfen"
            );
            assert!(v.list_top() > g_end.1 + g_end.3, "{wo}");
            // Kachel Lohnanteil: die Lohnzeile bleibt in ihrer Kachel
            let (vor, _, nach, (lx, _, lw, _)) = v.lohnsatz_lage(&t, &fonts).expect("Lohnzeile");
            let r = fonts.regular.as_ref().unwrap();
            let px = 10.0 * scale;
            let tw = (cw - 2.0 * TILE_GAP * scale) / 3.0;
            let kx = x0 + 2.0 * (tw + TILE_GAP * scale);
            let (a, e) = (lx - r.width(&vor, px), lx + lw + r.width(&nach, px));
            assert!(
                a >= kx + 12.0 * scale - 0.5 && e <= kx + tw - 12.0 * scale + 0.5,
                "{wo}: Lohnzeile {a}..{e} außerhalb der Kachel {kx}..{}",
                kx + tw
            );
        }
    }
    assert_eq!(beide, [true, true], "beide Lagen kommen vor");
}
