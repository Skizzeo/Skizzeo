//! Tests der Verwaltung (KA-3a2, paket-ka3a §5).

use super::*;
use std::path::{Path as FsPath, PathBuf};

fn haus() -> Scene {
    let m = sk_model::szo::read_with(
        sk_cost::verwaltung::STANDARDHAUS,
        sk_model::GuidGen::with_seed(1),
        &sk_cost::lesen::ABSCHNITTE_SZO,
    )
    .expect("lädt")
    .model;
    Scene::with_model(m)
}

/// Bauleistung nach dem Anfang ihres Kurztexts.
fn leistung(v: &Verwaltung, kurz: &str) -> Guid {
    v.jetzt
        .leistungen
        .iter()
        .find(|l| l.kurz.starts_with(kurz))
        .unwrap_or_else(|| panic!("{kurz}"))
        .guid
}

/// Ein frischer Firmenkatalog in einem eigenen Ordner (danach löschen).
fn firma(name: &str) -> (Company, PathBuf) {
    let dir =
        std::env::temp_dir().join(format!("skizzeo-verwaltung-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let (c, _) = Company::laden(&dir.join("firmenkatalog.szk"), true);
    (c, dir)
}

const AW24: &str = "AW Porenbeton-Planstein PP2-0,35 d=24cm";

fn win() -> Win {
    Win {
        w: 1400,
        h: 900,
        top: 40,
        scale: 1.0,
    }
}

/// Schriften fürs Messen (Liberation Sans, wenn keine Systemschrift).
fn schriften() -> Fonts {
    let f = Fonts::system();
    if f.regular.is_some() {
        return f;
    }
    let lib = FsPath::new("/usr/share/fonts/truetype/liberation");
    let lade = |n: &str| {
        std::fs::read(lib.join(n))
            .ok()
            .and_then(sk_paint::font::Font::parse)
    };
    Fonts {
        regular: lade("LiberationSans-Regular.ttf"),
        bold: lade("LiberationSans-Bold.ttf"),
        italic: lade("LiberationSans-Italic.ttf"),
    }
}

/// Abnahme 1: Baum mit allen Ästen und Anzahl, zuerst die erste
/// Bauleistung gewählt; Titel ohne Standnummer.
#[test]
fn baum_und_wahl() {
    let s = haus();
    let v = Verwaltung::open(&s, None, None);
    let z = v.zeilen();
    let oben: Vec<&str> = z
        .iter()
        .filter(|z| z.tiefe == 0)
        .map(|z| z.text.as_str())
        .collect();
    assert_eq!(
        oben,
        [
            "Firmenwerte",
            "Baustoffe und Preise",
            "Bauleistungen",
            "Lose und Titel",
            "Bauteiltypen",
            "Referenzhäuser",
            "Protokoll",
            "Papierkorb"
        ]
    );
    let n = v.jetzt.leistungen.iter().filter(|l| !l.retired).count();
    let bl = z.iter().find(|z| z.text == "Bauleistungen").unwrap();
    assert_eq!(bl.anzahl, Some(n));
    assert!(bl.offen, "Ast der gewählten Bauleistung offen");
    assert!(matches!(v.wahl, Knoten::Leistung(_)));
    assert!(
        z.iter().any(|z| z.knoten == v.wahl),
        "gewählte Zeile sichtbar"
    );
    assert!(v.titel.starts_with("Firmenkatalog"), "{}", v.titel);
    assert!(!v.titel.contains("Stand "), "{}", v.titel);
    // Suche: nur Passendes mit seinen Ästen
    let mut v = v;
    v.suche = "36,5".into();
    let z = v.zeilen();
    assert!(z.iter().any(|z| z.text.contains("d=36,5cm")));
    assert!(
        z.iter().all(|z| z.tiefe < 3 || z.text.contains("36,5")),
        "{z:?}"
    );
    assert!(!z.iter().any(|z| z.text == "Firmenwerte"));
}

/// Abnahme 2 und 12: Aufwandswert 0,5 → 0,45 wirkt sofort im Fenster
/// (EP, „vorher 0,5“, Wirkzeile), nichts geschrieben; zurück auf 0,5 ist
/// „unverändert“ ohne Operation.
#[test]
fn aufwandswert_live() {
    let s = haus();
    let rev = s.model().revision();
    let mut v = Verwaltung::open(&s, None, None);
    // die Außenwand des Standardhauses
    let l = v
        .jetzt
        .leistungen
        .iter()
        .find(|l| {
            l.kurz.starts_with("AW Porenbeton") && v.wirkung.haeuser[0].genutzt.contains(&l.guid)
        })
        .expect("Standardhaus rechnet mit einer Porenbeton-Außenwand");
    let (g, alt) = (l.guid, l.stunden);
    v.waehlen(Knoten::Leistung(g));
    let vorher = v.wirkung.haeuser[0].vorher;
    assert!(vorher.0 > 0);
    assert!(v.eingeben(&Feld::Stunden, "0,3"));
    assert_eq!(v.ops.len(), 1);
    assert_eq!(
        v.vorher_text(&Feld::Stunden),
        Some(format!("vorher {}", komma(alt)))
    );
    assert_eq!(
        v.jetzt.leistung(g).unwrap().stunden,
        Dez::lesen("0.3", 6).unwrap()
    );
    let h = &v.wirkung.haeuser[0];
    assert!(h.nachher < h.vorher, "{:?} {:?}", h.nachher, h.vorher);
    // Wirkzeile gleich lesen::kosten mit dem geänderten Katalog
    let firma = sk_cost::verwaltung::mit_ops(&v.basis, &v.ops).unwrap();
    let m = &v.wirkung.haeuser[0].m;
    let k = sk_cost::lesen::firma_oder_werk(m, Some(&firma));
    let sched = sk_model::qto::schedule(m);
    let netto = sk_cost::lesen::kosten(m, &sched, &k, &sk_cost::Umfang::projekt()).netto;
    assert_eq!(h.nachher, netto);
    let teile = wirkung::zeile(v.wirkung.haeuser.iter());
    assert_eq!(teile[0].0, "Standardhaus");
    assert!(teile[0].1.is_some(), "alt durchgestrichen");
    assert_eq!(s.model().revision(), rev, "Projekt unverändert");
    assert!(v.eingeben(&Feld::Stunden, &komma(alt)));
    assert!(v.ops.is_empty(), "{:?}", v.ops);
    assert_eq!(v.vorher_text(&Feld::Stunden), None);
    let teile = wirkung::zeile(v.wirkung.haeuser.iter());
    assert_eq!(teile[0].0, "Standardhaus unverändert");
    assert_eq!(teile[0].1, None);
}

/// Abnahme 9: Kurztext über 70 Zeichen und Preis −1 sperren OK, das Feld
/// ist rot; die Eingabe nimmt höchstens 70 Zeichen an.
#[test]
fn ungueltig_sperrt() {
    let s = haus();
    let fonts = schriften();
    let mut v = Verwaltung::open(&s, None, None);
    let g = leistung(&v, AW24);
    v.waehlen(Knoten::Leistung(g));
    assert!(v.eingeben(&Feld::Kurz, &"x".repeat(71)));
    assert!(v.gesperrt());
    assert_eq!(v.fehler_feld, Some(Feld::Kurz));
    assert!(v.befunde[0].satz.contains("71 Zeichen"), "{:?}", v.befunde);
    // Anzeige zeigt die Eingabe
    assert_eq!(v.jetzt.leistung(g).unwrap().kurz.len(), 71);
    let mut out = Out::default();
    v.ok(&mut out);
    assert!(!out.ok && !out.closed, "OK gesperrt");
    // Zurück: frei
    let alt = v.vorher.leistung(g).unwrap().kurz.clone();
    assert!(v.eingeben(&Feld::Kurz, &alt));
    assert!(!v.gesperrt());
    // Tippen: höchstens 70 Zeichen
    let mut cx = Ctx {
        fonts: &fonts,
        win: win(),
    };
    v.edit = Some(Edit {
        feld: Feld::Kurz,
        te: TextEdit::new(""),
    });
    for _ in 0..80 {
        v.handle(&Event::Text('y'), &mut cx);
    }
    assert_eq!(v.edit.as_ref().unwrap().te.text.chars().count(), 70);
    v.edit = None;
    // Preis −1
    let art = v
        .jetzt
        .artikel
        .iter()
        .find(|a| a.preis.is_some())
        .unwrap()
        .guid;
    v.waehlen(Knoten::ArtikelSatz(art));
    v.eingeben(&Feld::Preis(art), "-1");
    assert!(v.gesperrt(), "Preis −1");
    assert_eq!(v.fehler_feld, Some(Feld::Preis(art)));
    // Unlesbares nimmt nichts an
    let n = v.ops.len();
    assert!(!v.eingeben(&Feld::Preis(art), "abc"));
    assert_eq!(v.ops.len(), n);
}

/// Abnahme 7: In den Papierkorb und zurück; Firmenwerte mit „vorher“.
#[test]
fn papierkorb_und_firmenwerte() {
    let s = haus();
    let (c, dir) = firma("korb");
    let mut v = Verwaltung::open(&s, Some(&c), None);
    let _ = std::fs::remove_dir_all(&dir);
    let g = leistung(&v, AW24);
    let satz = SatzId::neu("service", g.to_ifc());
    v.aktion(Aktion::Ausmustern(satz.clone()));
    assert!(v.jetzt.leistung(g).unwrap().retired);
    let z = v.zeilen();
    let korb = z.iter().find(|z| z.text == "Papierkorb").unwrap();
    assert_eq!(korb.anzahl, Some(1));
    v.waehlen(Knoten::Leistung(g));
    assert!(v.offen.contains(&Knoten::Papierkorb));
    v.aktion(Aktion::Wiederherstellen(satz));
    assert!(v.ops.is_empty(), "{:?}", v.ops);
    v.waehlen(Knoten::Firmenwerte);
    assert!(v.eingeben(&Feld::Wert("wage".into()), "65"));
    assert_eq!(v.jetzt.werte.lohn, Dez::ganz(65));
    assert_eq!(
        v.vorher_text(&Feld::Wert("wage".into())).as_deref(),
        Some("vorher 60,00")
    );
    // Seite der Firmenwerte hat ein Feld je Wert
    let fonts = schriften();
    let teile = v.seite(&fonts, 700.0);
    let felder = teile
        .iter()
        .filter(|t| matches!(t.art, felder::Art::Feld { .. }))
        .count();
    assert!(felder >= 6, "{felder}");
}

/// Abnahme 3 (Teil Firma): OK schreibt über `Scene::fuer_firma` einen
/// neuen Stand mit dem neuen Wert und einer `[log]`-Zeile; bis dahin bleibt
/// die Datei bytegleich. Abnahme 4: Abbrechen schreibt nichts.
#[test]
fn ok_schreibt_firma() {
    let dir = std::env::temp_dir().join(format!("skizzeo-verwaltung-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let pfad: PathBuf = dir.join("firmenkatalog.szk");
    let (mut c, _) = Company::laden(&pfad, true);
    let datei = std::fs::read(&pfad).unwrap();
    let mut s = haus();
    let mut v = Verwaltung::open(&s, Some(&c), None);
    let g = leistung(&v, AW24);
    v.waehlen(Knoten::Leistung(g));
    v.eingeben(&Feld::Stunden, "0,45");
    assert_eq!(std::fs::read(&pfad).unwrap(), datei, "bis OK unverändert");
    let mut out = Out::default();
    v.ok(&mut out);
    assert!(out.ok);
    let h = sk_cost::Herkunft::neu(sk_cost::HerkunftArt::Manual, "2026-10-08", "16:00");
    let ops = v.ops().to_vec();
    s.fuer_firma(STEP, &mut c, &h, &ops).expect("schreibt");
    let text = std::fs::read_to_string(&pfad).unwrap();
    assert!(text.contains("hours=0.45"), "{text}");
    assert!(text.contains("op=bauleistung_aendern"), "{text}");
    // Abbrechen: nichts geschrieben
    let datei = std::fs::read(&pfad).unwrap();
    let mut v = Verwaltung::open(&s, Some(&c), None);
    v.waehlen(Knoten::Firmenwerte);
    v.eingeben(&Feld::Wert("wage".into()), "70");
    let fonts = schriften();
    let mut cx = Ctx {
        fonts: &fonts,
        win: win(),
    };
    let out = v.handle(
        &Event::Key {
            key: Key::Escape,
            down: true,
            repeat: false,
            mods: Modifiers::default(),
        },
        &mut cx,
    );
    // Mit Änderungen fragt Esc erst (Bedienbarkeit 13.3)
    assert!(!out.closed && !out.ok && v.frage);
    assert_eq!(std::fs::read(&pfad).unwrap(), datei);
    let _ = std::fs::remove_dir_all(&dir);
}

/// Abnahme 15b (paket-ka3a §5): Stahlbetondecke in den Papierkorb ohne
/// Ersatz sperrt OK mit Regel 97; Wiederherstellen gibt OK wieder frei.
#[test]
fn regel_97_ohne_typ() {
    let s = haus();
    let mut v = Verwaltung::open(&s, None, None);
    assert!(!v.gesperrt());
    let g = leistung(&v, "Stb-Decke Ortbeton");
    let satz = SatzId::neu("service", g.to_ifc());
    v.aktion(Aktion::Ausmustern(satz.clone()));
    assert!(v.gesperrt());
    assert_eq!(
        v.befunde[0].satz,
        "Werksbauteil Decke (Stahlbeton 220 mm) hat keine genaue Bauleistung."
    );
    v.aktion(Aktion::Wiederherstellen(satz));
    assert!(!v.gesperrt() && v.ops().is_empty());
}

/// Abnahme 15 (paket-ka3a §5): nach zwei OK zwei Stände; „Diese Änderung
/// zurücknehmen“ am ersten setzt die alten Werte als dritten Stand. Hat ein
/// späterer Stand denselben Satz wieder geändert, Befund und nichts geschieht.
#[test]
fn protokoll_zuruecknehmen() {
    let dir =
        std::env::temp_dir().join(format!("skizzeo-verwaltung-zurueck-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let pfad: PathBuf = dir.join("firmenkatalog.szk");
    let (mut c, _) = Company::laden(&pfad, true);
    let mut s = haus();
    let fonts = schriften();
    let w = win();
    let schreiben = |s: &mut Scene, c: &mut Company, v: &Verwaltung, uhr: &str| {
        let h = sk_cost::Herkunft::neu(sk_cost::HerkunftArt::Manual, "2026-10-08", uhr);
        s.fuer_firma(STEP, c, &h, v.ops()).expect("schreibt");
    };
    // Erstes OK: Stunden, zweites OK: Verrechnungslohn
    let mut v = Verwaltung::open(&s, Some(&c), None);
    let g = leistung(&v, AW24);
    let stunden = v.jetzt.leistung(g).unwrap().stunden;
    v.waehlen(Knoten::Leistung(g));
    v.eingeben(&Feld::Stunden, "0,45");
    schreiben(&mut s, &mut c, &v, "16:00");
    let mut v = Verwaltung::open(&s, Some(&c), None);
    let lohn = v.jetzt.werte.lohn;
    v.waehlen(Knoten::Firmenwerte);
    v.eingeben(&Feld::Wert("wage".into()), "70");
    schreiben(&mut s, &mut c, &v, "16:05");
    let mut v = Verwaltung::open(&s, Some(&c), None);
    let st = sk_cost::verwaltung::protokoll(&v.vorher);
    assert_eq!(st.len(), 2, "{st:#?}");
    let erster = st[1].stand;
    // Baum: Stände neu zuerst
    let zeilen = {
        v.waehlen(Knoten::Stand(erster));
        v.zeilen()
    };
    let i = zeilen
        .iter()
        .position(|z| z.knoten == Knoten::Stand(st[0].stand))
        .unwrap();
    assert_eq!(zeilen[i + 1].knoten, Knoten::Stand(erster));
    assert!(zeilen[i + 1]
        .text
        .starts_with(&format!("Stand {erster} · 08.10.2026 16:00")));
    // Der Knopf meldet den Stand an die App
    let (r, _) = v
        .teile_px(&w, &fonts)
        .into_iter()
        .find(|(_, t)| t.ziel == Some(Ziel::Aktion(Aktion::Zuruecknehmen(erster))))
        .expect("Knopf");
    let mut cx = Ctx {
        fonts: &fonts,
        win: w,
    };
    let out = v.handle(
        &Event::MouseDown {
            button: MouseButton::Left,
            x: (r.x + 5.0) as f64,
            y: (r.y + 5.0) as f64,
            mods: Modifiers::default(),
        },
        &mut cx,
    );
    assert_eq!(out.zurueck, Some(erster));
    let r = c.umkehr(s.model(), erster).map_err(|m| m.to_string());
    v.zuruecknehmen(erster, r);
    assert!(!v.ops().is_empty() && !v.gesperrt());
    assert_eq!(v.jetzt.leistung(g).unwrap().stunden, stunden);
    assert_eq!(v.jetzt.werte.lohn, Dez::ganz(70), "Stand 2 bleibt");
    // Der Knopf ist jetzt eine Lesezeile
    assert!(!v
        .teile_px(&w, &fonts)
        .iter()
        .any(|(_, t)| t.ziel == Some(Ziel::Aktion(Aktion::Zuruecknehmen(erster)))));
    schreiben(&mut s, &mut c, &v, "16:10");
    let text = std::fs::read_to_string(&pfad).unwrap();
    let st = sk_cost::verwaltung::protokoll(&sk_cost::lesen::firma_oder_werk(
        s.model(),
        Some(c.library()),
    ));
    assert_eq!(st.len(), 3, "{text}");
    assert!(
        st[0]
            .eintraege
            .iter()
            .any(|e| e.op == "bauleistung_aendern"),
        "{:#?}",
        st[0]
    );
    // Lohn zweimal geändert: Stand 2 zurück, dann der neue Wert 75 in
    // Stand 4; Stand 2 zurücknehmen geht nicht mehr
    let mut v = Verwaltung::open(&s, Some(&c), None);
    v.waehlen(Knoten::Firmenwerte);
    v.eingeben(&Feld::Wert("wage".into()), "75");
    schreiben(&mut s, &mut c, &v, "16:15");
    let mut v = Verwaltung::open(&s, Some(&c), None);
    let zweiter = sk_cost::verwaltung::protokoll(&v.vorher)[2].stand;
    v.waehlen(Knoten::Stand(zweiter));
    let r = c.umkehr(s.model(), zweiter).map_err(|m| m.to_string());
    assert!(r.is_err());
    v.zuruecknehmen(zweiter, r);
    assert!(v.ops().is_empty());
    v.paint(&Theme::dark(), &fonts, &w);
    assert!(
        v.meldung
            .as_deref()
            .is_some_and(|m| m.contains("Verrechnungslohn")),
        "{:?}",
        v.meldung
    );
    assert_ne!(lohn, Dez::ganz(75));
    let _ = std::fs::remove_dir_all(&dir);
}

/// Bedienbarkeit 13.3: Esc und × fragen bei gesammelten Änderungen nach,
/// Abbrechen verwirft ohne Frage; ohne Änderungen schließt Esc sofort.
#[test]
fn verwerfen_fragt() {
    let s = haus();
    let fonts = schriften();
    let w = win();
    let mut cx = Ctx {
        fonts: &fonts,
        win: w,
    };
    let taste = |k| Event::Key {
        key: k,
        down: true,
        repeat: false,
        mods: Modifiers::default(),
    };
    let klick = |v: &mut Verwaltung, cx: &mut Ctx, r: Rect| {
        let (x, y) = ((r.x + r.w * 0.5) as f64, (r.y + r.h * 0.5) as f64);
        let mods = Modifiers::default();
        let b = MouseButton::Left;
        v.handle(
            &Event::MouseDown {
                button: b,
                x,
                y,
                mods,
            },
            cx,
        );
        v.handle(
            &Event::MouseUp {
                button: b,
                x,
                y,
                mods,
            },
            cx,
        )
    };
    let mut v = Verwaltung::open(&s, None, None);
    assert!(v.handle(&taste(Key::Escape), &mut cx).closed);
    v.waehlen(Knoten::Firmenwerte);
    v.eingeben(&Feld::Wert("wage".into()), "70");
    // Esc: Rückfrage, Esc noch einmal: zurück ins Fenster
    assert!(!v.handle(&taste(Key::Escape), &mut cx).closed);
    assert!(v.frage);
    assert_eq!(v.knoepfe(&w)[1].2, "Verwerfen");
    assert!(!v.handle(&taste(Key::Escape), &mut cx).closed);
    assert!(!v.frage && !v.ops().is_empty());
    // ×: Rückfrage, „Zurück“ behält, „Verwerfen“ schließt
    let x = v.close_rect(&w);
    assert!(!klick(&mut v, &mut cx, x).closed && v.frage);
    let zurueck = v.knoepfe(&w)[0].1;
    assert!(!klick(&mut v, &mut cx, zurueck).closed && !v.frage);
    klick(&mut v, &mut cx, x);
    let verwerfen = v.knoepfe(&w)[1].1;
    assert!(klick(&mut v, &mut cx, verwerfen).closed);
    // Abbrechen fragt nicht
    let mut v = Verwaltung::open(&s, None, None);
    v.eingeben(&Feld::Wert("wage".into()), "70");
    let abbrechen = v.knoepfe(&w)[0].1;
    assert!(klick(&mut v, &mut cx, abbrechen).closed);
}

/// Bedienbarkeit 13.4: Ohne Firmenkatalog sagt der Kopf es gleich, und
/// die Felder sind nur zum Lesen.
#[test]
fn ohne_firma_nur_ansehen() {
    let s = haus();
    let fonts = schriften();
    let w = win();
    let mut v = Verwaltung::open(&s, None, None);
    assert_eq!(v.titel, "Firmenkatalog nicht erreichbar · nur ansehen");
    let teile = v.teile_px(&w, &fonts);
    assert!(teile
        .iter()
        .all(|(_, t)| !matches!(t.art, felder::Art::Feld { .. })));
    assert!(teile
        .iter()
        .any(|(_, t)| matches!(t.art, felder::Art::Lese(_))));
    let mut cx = Ctx {
        fonts: &fonts,
        win: w,
    };
    let (r, _) = teile
        .iter()
        .find(|(_, t)| matches!(t.art, felder::Art::Lese(_)))
        .unwrap();
    v.handle(
        &Event::MouseDown {
            button: MouseButton::Left,
            x: (r.x + 5.0) as f64,
            y: (r.y + 5.0) as f64,
            mods: Modifiers::default(),
        },
        &mut cx,
    );
    assert!(v.edit.is_none());
    v.waehlen(Knoten::Papierkorb);
    let (c, dir) = firma("nur-ansehen");
    let mit = Verwaltung::open(&s, Some(&c), None);
    let _ = std::fs::remove_dir_all(&dir);
    assert!(!mit.titel.contains("nur ansehen"), "{}", mit.titel);
}

fn artikel_guid(v: &Verwaltung, name: &str) -> Guid {
    v.jetzt
        .artikel
        .iter()
        .find(|a| a.name.starts_with(name))
        .unwrap_or_else(|| panic!("{name}"))
        .guid
}

fn texte(v: &Verwaltung, fonts: &Fonts) -> Vec<String> {
    v.seite(fonts, 700.0)
        .into_iter()
        .filter_map(|t| match t.art {
            felder::Art::Text { text, .. } => Some(text),
            felder::Art::Seg { text, .. } => Some(text),
            _ => None,
        })
        .collect()
}

/// Abnahme 15a (verwaltung.md §15 Fall 15, Regel 108): PB240 mit 0,85 €/St
/// → 5,6695 €/m², die Rechnung leise darunter und die Eingabe in
/// `[origin] source`; PB175 mit 95,00 €/m³ → 16,625.
#[test]
fn preis_je_stueck_und_je_m3() {
    let (mut c, dir) = firma("je-stueck");
    let mut s = haus();
    let fonts = schriften();
    let mut v = Verwaltung::open(&s, Some(&c), None);
    let pb240 = artikel_guid(&v, "Porenbeton-Planstein PP2-0,35 d=24cm");
    let pb175 = artikel_guid(&v, "Porenbeton-Planstein PP2-0,35 d=17,5cm");
    v.waehlen(Knoten::ArtikelSatz(pb240));
    let t = texte(&v, &fonts);
    for e in ["je m²", "je m³", "je Stück"] {
        assert!(t.iter().any(|x| x == e), "{e}: {t:?}");
    }
    v.aktion(Aktion::Je(Einheit::St));
    assert!(v.eingeben(&Feld::Preis(pb240), "0,85"));
    assert_eq!(v.jetzt.artikel(pb240).unwrap().preis, Some(Dez(5_669_500)));
    assert_eq!(v.anzeige(&Feld::Preis(pb240)), "0,85");
    let t = texte(&v, &fonts);
    assert!(
        t.iter()
            .any(|x| x == "0,85 €/St × 6,67 St/m² = 5,6695 €/m²"),
        "{t:?}"
    );
    assert!(t.iter().any(|x| x == "vorher 29,40 €/m²"), "{t:?}");
    // Zurück auf je m² zeigt den gespeicherten Preis
    v.aktion(Aktion::Je(Einheit::M2));
    assert_eq!(v.anzeige(&Feld::Preis(pb240)), "5,6695");
    v.waehlen(Knoten::ArtikelSatz(pb175));
    v.aktion(Aktion::Je(Einheit::M3));
    assert!(v.eingeben(&Feld::Preis(pb175), "95"));
    assert_eq!(v.jetzt.artikel(pb175).unwrap().preis, Some(Dez(16_625_000)));
    let h = sk_cost::Herkunft::neu(sk_cost::HerkunftArt::Manual, "2026-10-08", "17:00");
    let ops = v.ops().to_vec();
    s.fuer_firma(STEP, &mut c, &h, &ops).expect("schreibt");
    let text = std::fs::read_to_string(c.path()).unwrap();
    let _ = std::fs::remove_dir_all(&dir);
    let zeile = |g: Guid, a: &str| {
        text.lines()
            .find(|l| l.starts_with(a) && l.contains(&g.to_ifc()))
            .unwrap_or_else(|| panic!("{a} {}", g.to_ifc()))
            .to_string()
    };
    assert!(zeile(pb240, "[article]").contains(" price=5.6695 "));
    assert!(zeile(pb175, "[article]").contains(" price=16.625 "));
    let o = zeile(pb240, "[origin]");
    assert!(
        o.contains("source=\"eingegeben 0,85 €/St × 6,67 St/m²\""),
        "{o}"
    );
    let o = zeile(pb175, "[origin]");
    assert!(
        o.contains("source=\"eingegeben 95,00 €/m³ × 0,175 m\""),
        "{o}"
    );
}

/// Abnahme 15a, zweiter Teil: ohne `conv` mit „L×H“ erst der Vorschlag,
/// erst nach „Bestätigen“ rechnet je Stück, `conv` geht ins selbe OK.
#[test]
fn stueck_vorschlag_bestaetigen() {
    let (mut c, dir) = firma("vorschlag");
    let mut s = haus();
    let fonts = schriften();
    let mut v = Verwaltung::open(&s, Some(&c), None);
    let kd = artikel_guid(&v, "Kerndämmplatte");
    v.waehlen(Knoten::ArtikelSatz(kd));
    v.aktion(Aktion::Je(Einheit::M3));
    assert!(v.eingeben(&Feld::Preis(kd), "100"));
    v.aktion(Aktion::Je(Einheit::St));
    let t = texte(&v, &fonts);
    assert!(
        t.iter()
            .any(|x| x == "Steine je m² (aus 1000×625, Fuge 10 mm) ·"),
        "{t:?}"
    );
    assert_eq!(v.anzeige(&Feld::Conv(kd)), "1,5592");
    // Vor dem Bestätigen rechnet die Einheit nicht
    assert!(!v.eingeben(&Feld::Preis(kd), "2"));
    let m = v.meldung.clone().unwrap_or_default();
    assert!(m.contains("hat keine Umrechnung"), "{m}");
    // Zahl änderbar, dann bestätigen
    assert!(!v.eingeben(&Feld::Conv(kd), "0"));
    assert!(v.eingeben(&Feld::Conv(kd), "1,6"));
    v.aktion(Aktion::ConvBestaetigen(kd));
    assert_eq!(v.jetzt.artikel(kd).unwrap().conv, Some(Dez(1_600_000)));
    assert!(v.eingeben(&Feld::Preis(kd), "2"));
    assert_eq!(v.jetzt.artikel(kd).unwrap().preis, Some(Dez(3_200_000)));
    // `conv` vor dem Preis, sonst verlöre die Herkunft die Eingabe
    assert!(
        matches!(v.ops()[0], Op::UmrechnungSetzen { .. }),
        "{:?}",
        v.ops()
    );
    let h = sk_cost::Herkunft::neu(sk_cost::HerkunftArt::Manual, "2026-10-08", "17:05");
    let ops = v.ops().to_vec();
    s.fuer_firma(STEP, &mut c, &h, &ops).expect("schreibt");
    let text = std::fs::read_to_string(c.path()).unwrap();
    let _ = std::fs::remove_dir_all(&dir);
    let a = text
        .lines()
        .find(|l| l.starts_with("[article]") && l.contains(&kd.to_ifc()))
        .unwrap();
    assert!(a.contains(" price=3.2 ") && a.contains(" conv=1.6"), "{a}");
    let o = text
        .lines()
        .find(|l| l.starts_with("[origin]") && l.contains(&kd.to_ifc()))
        .unwrap();
    assert!(o.contains(" kind=manual "), "{o}");
    assert!(o.contains("eingegeben 2,00 €/St × 1,6 St/m²"), "{o}");
}

/// Bedienbarkeit 13.1: Die Wirkzeile nennt zuerst dieses Haus. Ohne Kopie
/// rechnet es mit der Firma; mit Kopie übernimmt es beim OK die geänderten
/// Sätze, wo es nicht selbst abweicht (Regel 89).
#[test]
fn dieses_haus_in_der_wirkzeile() {
    let s = haus();
    let mut v = Verwaltung::open(&s, None, None);
    // Eine Außenwand, mit der das Standardhaus (= dieses Haus) rechnet
    let g = v
        .jetzt
        .leistungen
        .iter()
        .find(|l| {
            l.kurz.starts_with("AW Porenbeton") && v.wirkung.haeuser[0].genutzt.contains(&l.guid)
        })
        .unwrap()
        .guid;
    v.waehlen(Knoten::Leistung(g));
    v.eingeben(&Feld::Stunden, "0,3");
    let teile = wirkung::zeile(v.wirkung.alle());
    assert_eq!(teile[0].0, "Dieses Haus");
    assert!(teile[0].1.is_some());
    assert_eq!(teile[1].0, "Standardhaus");
    // Mit Kopie: folgt der Änderung
    let mut m = s.model().clone();
    m.begin("Kopie");
    sk_cost::op::kopie_anlegen(&mut m, Some(&Library::standard()));
    let _ = m.try_commit();
    assert!(sk_cost::op::hat_kopie(&m));
    let s2 = Scene::with_model(m);
    let mut v = Verwaltung::open(&s2, None, None);
    let d = v.wirkung.dieses.as_ref().unwrap();
    let vorher = d.vorher;
    v.waehlen(Knoten::Leistung(g));
    v.eingeben(&Feld::Stunden, "0,3");
    let d = v.wirkung.dieses.as_ref().unwrap();
    assert_eq!(d.vorher, vorher);
    assert!(d.nachher < d.vorher, "{:?} {:?}", d.nachher, d.vorher);
    assert_eq!(
        d.nachher, v.wirkung.haeuser[0].nachher,
        "gleiches Haus, gleiche Summe"
    );
    // Leeres Haus: kein Eintrag
    let leer = Scene::with_model(sk_model::Model::new());
    assert!(Verwaltung::open(&leer, None, None).wirkung.dieses.is_none());
}

/// Abnahme 13 (KA-3a4): €/m² Grundfläche des Standardhauses
/// = Summe netto ÷ 135,50 m², auf ganze €. Je Geschoss der Kernumriss der
/// eigenen Außenwände (Entscheid 17:20): EG 75,0384, OG mit Rücksprung
/// 60,4584 (vorher 2 × 75,0384 über die Decke unter dem OG).
#[test]
fn standardhaus_je_m2() {
    let s = haus();
    let v = Verwaltung::open(&s, None, None);
    let h = &v.wirkung.haeuser[0];
    assert!((h.flaeche / 1e6 - 135.4968).abs() < 1e-3, "{}", h.flaeche);
    let soll = (h.vorher.0 as f64 / 100.0 / 135.4968).round() as i64;
    assert_eq!(h.je_m2(h.vorher), Some(soll));
    let leer = Verwaltung::open(&Scene::with_model(sk_model::Model::new()), None, None);
    assert!(leer.wirkung.dieses.is_none());
    assert_eq!(
        wirkung::aenderung(sk_cost::Cent(100_000), sk_cost::Cent(100_800)),
        "+8 € (+0,8 %)"
    );
    assert_eq!(
        wirkung::aenderung(sk_cost::Cent(6_226_100), sk_cost::Cent(6_009_000)),
        "−2.171 € (−3,5 %)"
    );
}

/// KA-3a4, Regel 106: eigene Referenzhäuser aus `referenzhaeuser/` nach
/// Dateiname; unlesbar grau mit Befund und nie geändert; ohne Kostenzeile
/// ein Hinweis; Unterordner und andere Endungen zählen nicht. „Aktuelles
/// Haus als Referenzhaus“ ist nur eine Dateikopie, das Haus rechnet dann
/// mit dem Firmenkatalog, nicht mit seiner Kopie (S5, Fall 12).
#[test]
fn referenzhaeuser_im_ordner() {
    let (c, dir) = firma("referenz");
    let ordner = dir.join(wirkung::ORDNER);
    std::fs::create_dir_all(ordner.join("unter")).unwrap();
    let rh2 = include_str!("../../../crates/sk-cost/referenz/rh2-mehrschalig.szo");
    let rh3 = include_str!("../../../crates/sk-cost/referenz/rh3-versatz-dachterrasse.szo");
    std::fs::write(ordner.join("b-mehrschalig.szo"), rh2).unwrap();
    std::fs::write(ordner.join("a-versatz.szo"), rh3).unwrap();
    std::fs::write(ordner.join("kaputt.szo"), "kein Haus").unwrap();
    std::fs::write(ordner.join("notiz.txt"), "x").unwrap();
    std::fs::write(ordner.join("unter").join("c.szo"), rh2).unwrap();
    std::fs::write(
        ordner.join("leer.szo"),
        sk_model::szo::write(&sk_model::Model::new()),
    )
    .unwrap();
    // Dieses Haus mit eigener Kopie, Lohn 70 nur hier
    let mut s = haus();
    s.kosten_folge(
        "Lohn nur dieses Haus",
        Some(c.library()),
        &sk_cost::Herkunft::neu(sk_cost::HerkunftArt::Manual, "2026-10-08", "17:00"),
        &[Op::FirmenwertSetzen {
            schluessel: "wage".into(),
            wert: Dez::ganz(70),
        }],
    )
    .unwrap();
    let mut v = Verwaltung::open(&s, Some(&c), None);
    let namen: Vec<&str> = v.wirkung.haeuser.iter().map(|h| h.name.as_str()).collect();
    assert_eq!(
        namen,
        [
            "Standardhaus",
            "a-versatz",
            "b-mehrschalig",
            "kaputt",
            "leer"
        ]
    );
    let kaputt = &v.wirkung.haeuser[3];
    assert!(
        kaputt.fehler.as_deref().is_some_and(|f| f
            .starts_with("Referenzhaus kaputt.szo lässt sich nicht lesen (")
            && f.ends_with("); es wird nicht gerechnet.")),
        "{:?}",
        kaputt.fehler
    );
    assert_eq!(
        v.wirkung.haeuser[4].hinweis().as_deref(),
        Some("Referenzhaus leer.szo hat keine Kostenzeile.")
    );
    assert!(v.wirkung.haeuser[1].vorher.0 > 0 && v.wirkung.haeuser[2].vorher.0 > 0);
    // Im Baum grau, in der Wirkzeile nicht; nichts davon sperrt OK
    v.waehlen(Knoten::Haus(3));
    let z = v.zeilen();
    assert!(z.iter().any(|z| z.knoten == Knoten::Haus(3) && z.grau));
    let zeile: Vec<String> = wirkung::zeile(v.wirkung.alle())
        .into_iter()
        .map(|t| t.0)
        .collect();
    assert_eq!(zeile.len(), 5, "{zeile:?}");
    assert!(zeile.iter().all(|t| !t.starts_with("kaputt")));
    v.waehlen(Knoten::Firmenwerte);
    v.eingeben(&Feld::Wert("wage".into()), "65");
    assert!(!v.gesperrt());
    // Aktuelles Haus als Referenzhaus: rechnet mit der Firma (65), nicht
    // mit seiner Kopie (70)
    v.set_haus_name("Muster Meier.szo");
    v.aktion(Aktion::AlsReferenz);
    assert!(ordner.join("Muster Meier.szo").exists(), "{:?}", v.meldung);
    let i = v
        .wirkung
        .haeuser
        .iter()
        .position(|h| h.name == "Muster Meier")
        .unwrap();
    assert_eq!(v.wahl, Knoten::Haus(i));
    let neu = &v.wirkung.haeuser[i];
    let std = &v.wirkung.haeuser[0];
    assert_eq!(
        (neu.vorher, neu.nachher),
        (std.vorher, std.nachher),
        "wie das Standardhaus"
    );
    let d = v.wirkung.dieses.as_ref().unwrap();
    assert_eq!(d.vorher, d.nachher, "dieses Haus behält 70 (Regel 89)");
    assert!(neu.nachher > neu.vorher);
    v.aktion(Aktion::AlsReferenz);
    assert!(ordner.join("Muster Meier (2).szo").exists());
    // Malen samt Tooltip
    let fonts = schriften();
    let w = win();
    v.waehlen(Knoten::Haeuser);
    v.hover = Some(Ziel::Tipp(wirkung::FLAECHE_TIPP));
    v.paint(&Theme::dark(), &fonts, &w);
    for k in (0..v.wirkung.haeuser.len()).map(Knoten::Haus) {
        v.waehlen(k);
        v.paint(&Theme::dark(), &fonts, &w);
    }
    assert_eq!(
        std::fs::read_to_string(ordner.join("kaputt.szo")).unwrap(),
        "kein Haus",
        "nie geändert"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// Klick in den Baum wählt und klappt; Klick in ein Feld öffnet es, Enter
/// übernimmt. Malen geht in jeder Seite.
#[test]
fn bedienen_und_malen() {
    let s = haus();
    let fonts = schriften();
    let t = Theme::dark();
    let w = win();
    let (c, dir) = firma("bedienen");
    let mut v = Verwaltung::open(&s, Some(&c), None);
    let _ = std::fs::remove_dir_all(&dir);
    let mut cx = Ctx {
        fonts: &fonts,
        win: w,
    };
    // Zeile „Firmenwerte“ anklicken
    let i = v
        .zeilen()
        .iter()
        .position(|z| z.knoten == Knoten::Firmenwerte)
        .unwrap();
    let r = v.zeile_rect(&w, i);
    let (x, y) = ((r.x + r.w * 0.5) as f64, (r.y + r.h * 0.5) as f64);
    let down = |x, y| Event::MouseDown {
        button: MouseButton::Left,
        x,
        y,
        mods: Modifiers::default(),
    };
    v.handle(&down(x, y), &mut cx);
    assert_eq!(v.wahl, Knoten::Firmenwerte);
    // Feld Verrechnungslohn öffnen, 62 tippen, Enter
    let (r, _) = v
        .teile_px(&w, &fonts)
        .into_iter()
        .find(|(_, t)| t.ziel == Some(Ziel::Feld(Feld::Wert("wage".into()))))
        .unwrap();
    v.handle(&down((r.x + 5.0) as f64, (r.y + 5.0) as f64), &mut cx);
    assert!(v.edit.is_some());
    for ch in "62".chars() {
        v.handle(&Event::Text(ch), &mut cx);
    }
    v.handle(
        &Event::Key {
            key: Key::Enter,
            down: true,
            repeat: false,
            mods: Modifiers::default(),
        },
        &mut cx,
    );
    assert_eq!(v.jetzt.werte.lohn, Dez::ganz(62));
    // Jede Seite malt
    let knoten: Vec<Knoten> = v.zeilen().iter().map(|z| z.knoten.clone()).collect();
    v.mehr = true;
    for k in knoten {
        v.waehlen(k);
        let (c, _, _) = v.paint(&t, &fonts, &w);
        assert!(c.width > 0);
    }
    let alle: Vec<Knoten> = v
        .jetzt
        .artikel
        .iter()
        .map(|a| Knoten::ArtikelSatz(a.guid))
        .chain(v.typen().into_iter().map(|(g, _)| Knoten::Typ(g)))
        .chain([Knoten::Haus(0)])
        .collect();
    for k in alle {
        v.waehlen(k);
        let _ = v.paint(&t, &fonts, &w);
    }
}

/// Ist-Bild der Verwaltung (soll-ka-3-verwaltung), nur auf Wunsch:
/// `SKIZZEO_ISTBILDER=<ordner> cargo test -p skizzeo istbilder -- --ignored`
#[test]
#[ignore = "legt Ist-Bilder ab, nur mit SKIZZEO_ISTBILDER"]
fn istbilder_ka3() {
    let Some(dir) = std::env::var_os("SKIZZEO_ISTBILDER").map(PathBuf::from) else {
        return;
    };
    let fonts = schriften();
    if fonts.regular.is_none() {
        return;
    }
    std::fs::create_dir_all(&dir).unwrap();
    let s = haus();
    let t = Theme::dark();
    let w = Win {
        w: 1180,
        h: 820,
        top: 30,
        scale: 1.0,
    };
    let (c, tmp) = firma("ist");
    let mut v = Verwaltung::open(&s, Some(&c), None);
    let _ = std::fs::remove_dir_all(&tmp);
    let g = leistung(&v, AW24);
    v.waehlen(Knoten::Leistung(g));
    v.eingeben(&Feld::Stunden, "0,45");
    let (c, _, _) = v.paint(&t, &fonts, &w);
    std::fs::write(dir.join("ist-ka-3-verwaltung.png"), c.to_png()).unwrap();
    v.mehr = true;
    v.anteil = true;
    let (c, _, _) = v.paint(&t, &fonts, &w);
    std::fs::write(dir.join("ist-ka-3-verwaltung-mehr.png"), c.to_png()).unwrap();
    v.waehlen(Knoten::Firmenwerte);
    let (c, _, _) = v.paint(&t, &fonts, &w);
    std::fs::write(dir.join("ist-ka-3-firmenwerte.png"), c.to_png()).unwrap();
    let art = v
        .jetzt
        .artikel
        .iter()
        .find(|a| a.name.contains("d=24cm"))
        .unwrap()
        .guid;
    v.waehlen(Knoten::ArtikelSatz(art));
    let (c, _, _) = v.paint(&t, &fonts, &w);
    std::fs::write(dir.join("ist-ka-3-artikel.png"), c.to_png()).unwrap();
    // KA-3a7: je Stück mit Rechnung; ohne `conv` der Vorschlag
    v.aktion(Aktion::Je(Einheit::St));
    v.eingeben(&Feld::Preis(art), "0,85");
    let (c, _, _) = v.paint(&t, &fonts, &w);
    std::fs::write(dir.join("ist-ka-3a7-artikel-je-stueck.png"), c.to_png()).unwrap();
    let kd = artikel_guid(&v, "Kerndämmplatte");
    v.waehlen(Knoten::ArtikelSatz(kd));
    v.aktion(Aktion::Je(Einheit::St));
    let (c, _, _) = v.paint(&t, &fonts, &w);
    std::fs::write(dir.join("ist-ka-3a7-vorschlag.png"), c.to_png()).unwrap();
    // Protokoll: ein Stand mit zwei Änderungen, davor und nach dem Klick
    let tmp = std::env::temp_dir().join(format!("skizzeo-ist-ka3-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).unwrap();
    let pfad = tmp.join("firmenkatalog.szk");
    let (mut c, _) = Company::laden(&pfad, true);
    let mut s = haus();
    let mut v = Verwaltung::open(&s, Some(&c), None);
    v.waehlen(Knoten::Leistung(g));
    v.eingeben(&Feld::Stunden, "0,45");
    v.eingeben(
        &Feld::Kurz,
        "AW Porenbeton-Planstein PP2-0,35 d=24cm, Dünnbett",
    );
    v.waehlen(Knoten::Firmenwerte);
    v.eingeben(&Feld::Wert("wage".into()), "64,50");
    let h = sk_cost::Herkunft::neu(sk_cost::HerkunftArt::Manual, "2026-10-08", "16:00");
    s.fuer_firma(STEP, &mut c, &h, v.ops()).expect("schreibt");
    let mut v = Verwaltung::open(&s, Some(&c), None);
    let n = sk_cost::verwaltung::protokoll(&v.vorher)[0].stand;
    v.waehlen(Knoten::Stand(n));
    let (b, _, _) = v.paint(&t, &fonts, &w);
    std::fs::write(dir.join("ist-ka-3-protokoll.png"), b.to_png()).unwrap();
    let r = c.umkehr(s.model(), n).map_err(|m| m.to_string());
    v.zuruecknehmen(n, r);
    let (b, _, _) = v.paint(&t, &fonts, &w);
    std::fs::write(dir.join("ist-ka-3-protokoll-zurueck.png"), b.to_png()).unwrap();
    // Referenzhäuser (KA-3a4): zwei eigene, eins kaputt; Lohn 64,50
    let ordner = tmp.join(wirkung::ORDNER);
    std::fs::create_dir_all(&ordner).unwrap();
    let rh2 = include_str!("../../../crates/sk-cost/referenz/rh2-mehrschalig.szo");
    let rh3 = include_str!("../../../crates/sk-cost/referenz/rh3-versatz-dachterrasse.szo");
    std::fs::write(ordner.join("Doppelhaus Mehrschalig.szo"), rh2).unwrap();
    std::fs::write(ordner.join("Stadtvilla Dachterrasse.szo"), rh3).unwrap();
    std::fs::write(ordner.join("Altbau kaputt.szo"), "kein Haus").unwrap();
    let mut v = Verwaltung::open(&s, Some(&c), None);
    v.waehlen(Knoten::Firmenwerte);
    v.eingeben(&Feld::Wert("wage".into()), "66");
    v.waehlen(Knoten::Haeuser);
    let (b, _, _) = v.paint(&t, &fonts, &w);
    std::fs::write(dir.join("ist-ka-3a4-referenzhaeuser.png"), b.to_png()).unwrap();
    v.waehlen(Knoten::Haus(0));
    v.hover = Some(Ziel::Tipp(wirkung::FLAECHE_TIPP));
    let (b, _, _) = v.paint(&t, &fonts, &w);
    std::fs::write(dir.join("ist-ka-3a4-standardhaus.png"), b.to_png()).unwrap();
    v.hover = None;
    v.waehlen(Knoten::Haus(1));
    let (b, _, _) = v.paint(&t, &fonts, &w);
    std::fs::write(dir.join("ist-ka-3a4-kaputt.png"), b.to_png()).unwrap();
    let _ = std::fs::remove_dir_all(&tmp);
}

/// Review 3ar: Ein anderer Platz ändert dieselbe Bauleistung; das erste OK
/// scheitert und lädt den Katalog neu. Das Fenster steht danach auf dem
/// neuen Stand, sonst schriebe das zweite OK 0,45 über 0,7, ohne dass 0,7
/// je zu sehen war.
#[test]
fn fremde_aenderung_wird_vor_dem_zweiten_ok_gezeigt() {
    let (mut c, dir) = firma("fremd");
    let pfad = c.path().to_path_buf();
    let h = sk_cost::Herkunft::neu(sk_cost::HerkunftArt::Manual, "2026-10-08", "16:00");
    let mut s = haus();
    let mut v = Verwaltung::open(&s, Some(&c), None);
    let g = leistung(&v, AW24);
    v.waehlen(Knoten::Leistung(g));
    v.eingeben(&Feld::Stunden, "0,45");
    let (mut c2, _) = Company::laden(&pfad, true);
    let mut s2 = haus();
    let mut v2 = Verwaltung::open(&s2, Some(&c2), None);
    v2.waehlen(Knoten::Leistung(g));
    v2.eingeben(&Feld::Stunden, "0,7");
    s2.fuer_firma(STEP, &mut c2, &h, v2.ops())
        .expect("anderer Platz schreibt");

    assert!(s.fuer_firma(STEP, &mut c, &h, v.ops()).is_err());
    v.neu_grundlage(&c);
    v.fehler("geändert".into());
    assert_eq!(v.vorher_text(&Feld::Stunden).as_deref(), Some("vorher 0,7"));
    assert_eq!(v.meldung.as_deref(), Some("geändert"));
    // Die Eingabe bleibt; erst jetzt, gesehen, schreibt OK sie
    assert!(s.fuer_firma(STEP, &mut c, &h, v.ops()).is_ok());
    let text = std::fs::read_to_string(&pfad).unwrap();
    let _ = std::fs::remove_dir_all(&dir);
    assert!(text.contains("hours=0.45"), "{text}");
}

/// Testhinweis D: Platz A hat Stand n geschrieben, Platz B danach denselben
/// Satz wieder geändert. Nimmt A (alter Stand im Speicher) Stand n zurück,
/// kommt gleich der Befund mit dem Satz, nicht erst beim OK „anderswo
/// geändert“; die Datei bleibt.
#[test]
fn zuruecknehmen_liest_die_datei_neu() {
    let (mut a, dir) = firma("umkehr-neu");
    let pfad = a.path().to_path_buf();
    let h = sk_cost::Herkunft::neu(sk_cost::HerkunftArt::Manual, "2026-10-08", "17:40");
    let mut s = haus();
    let mut v = Verwaltung::open(&s, Some(&a), None);
    v.waehlen(Knoten::Firmenwerte);
    assert!(v.eingeben(&Feld::Wert("wage".into()), "65"));
    s.fuer_firma(STEP, &mut a, &h, v.ops()).expect("Stand A");
    // Stand aus dem Kopf der Datei
    let stand_a: u32 = std::fs::read_to_string(&pfad)
        .unwrap()
        .lines()
        .find(|l| l.starts_with("[catalog]"))
        .and_then(|l| l.split(' ').find_map(|w| w.strip_prefix("stand=")))
        .and_then(|n| n.parse().ok())
        .expect("Stand im Kopf");
    let (mut b, _) = Company::laden(&pfad, true);
    let mut s2 = haus();
    let mut vb = Verwaltung::open(&s2, Some(&b), None);
    vb.waehlen(Knoten::Firmenwerte);
    assert!(vb.eingeben(&Feld::Wert("wage".into()), "70"));
    s2.fuer_firma(STEP, &mut b, &h, vb.ops()).expect("Stand B");
    let datei = std::fs::read(&pfad).unwrap();
    let r = a.umkehr(s.model(), stand_a).map_err(|m| m.to_string());
    let nachher = std::fs::read(&pfad).unwrap_or_default();
    let _ = std::fs::remove_dir_all(&dir);
    let m = r.expect_err("derselbe Satz wurde später geändert");
    assert!(m.contains("wieder geändert"), "{m}");
    assert_eq!(nachher, datei);
}

/// Befund E: Platz A nimmt Stand n zurück (die Ops stehen im Fenster),
/// dann ändert Platz B denselben Satz wieder. Das OK von A scheitert, das
/// Fenster setzt auf dem neuen Stand auf und rechnet die Rücknahme neu:
/// nur der Befund, keine Ops, ein zweites OK überschreibt B nicht.
#[test]
fn ruecknahme_nach_fremder_aenderung_neu_gerechnet() {
    let (mut a, dir) = firma("umkehr-e");
    let pfad = a.path().to_path_buf();
    let h = sk_cost::Herkunft::neu(sk_cost::HerkunftArt::Manual, "2026-10-08", "18:20");
    let mut s = haus();
    let mut v = Verwaltung::open(&s, Some(&a), None);
    v.waehlen(Knoten::Firmenwerte);
    assert!(v.eingeben(&Feld::Wert("wage".into()), "65"));
    s.fuer_firma(STEP, &mut a, &h, v.ops()).expect("Stand A");
    let stand_a = a
        .library()
        .ext("catalog")
        .next()
        .and_then(|r| r.line.split(' ').find_map(|w| w.strip_prefix("stand=")))
        .and_then(|n| n.parse::<u32>().ok())
        .expect("Stand im Kopf");
    let mut v = Verwaltung::open(&s, Some(&a), None);
    v.zuruecknehmen(
        stand_a,
        a.umkehr(s.model(), stand_a).map_err(|m| m.to_string()),
    );
    assert!(!v.ops().is_empty());
    // Platz B ändert den Lohn wieder, bevor A OK drückt
    let (mut b, _) = Company::laden(&pfad, true);
    let mut s2 = haus();
    let mut vb = Verwaltung::open(&s2, Some(&b), None);
    vb.waehlen(Knoten::Firmenwerte);
    assert!(vb.eingeben(&Feld::Wert("wage".into()), "70"));
    s2.fuer_firma(STEP, &mut b, &h, vb.ops()).expect("Stand B");
    let datei = std::fs::read(&pfad).unwrap();
    assert!(s.fuer_firma(STEP, &mut a, &h, v.ops()).is_err());
    v.neu_grundlage(&a);
    v.fehler("geändert.".into());
    let ops = v.ops().to_vec();
    if !ops.is_empty() {
        let _ = s.fuer_firma(STEP, &mut a, &h, &ops);
    }
    let nachher = std::fs::read(&pfad).unwrap_or_default();
    let _ = std::fs::remove_dir_all(&dir);
    assert!(ops.is_empty(), "{ops:?}");
    let m = v.meldung.clone().unwrap_or_default();
    assert!(m.starts_with("geändert. "), "{m}");
    assert!(m.contains("wieder geändert"), "{m}");
    assert_eq!(nachher, datei, "Stand von B bleibt");
}

/// Review 3at (Befund E): Nach „Zurücknehmen“ setzt der Nutzer denselben
/// Wert selbst anders. Scheitert OK an einer fremden Änderung an einem
/// anderen Satz, rechnet das Fenster die Rücknahme neu; die eigene spätere
/// Eingabe bleibt dabei stehen.
#[test]
fn ruecknahme_neu_gerechnet_laesst_spaetere_eingabe() {
    let (mut a, dir) = firma("umkehr-e2");
    let pfad = a.path().to_path_buf();
    let h = sk_cost::Herkunft::neu(sk_cost::HerkunftArt::Manual, "2026-10-08", "19:10");
    let mut s = haus();
    let mut v = Verwaltung::open(&s, Some(&a), None);
    v.waehlen(Knoten::Firmenwerte);
    assert!(v.eingeben(&Feld::Wert("wage".into()), "65"));
    s.fuer_firma(STEP, &mut a, &h, v.ops()).expect("Stand A");
    let stand_a = a
        .library()
        .ext("catalog")
        .next()
        .and_then(|r| r.line.split(' ').find_map(|w| w.strip_prefix("stand=")))
        .and_then(|n| n.parse::<u32>().ok())
        .expect("Stand im Kopf");
    let mut v = Verwaltung::open(&s, Some(&a), None);
    v.zuruecknehmen(
        stand_a,
        a.umkehr(s.model(), stand_a).map_err(|m| m.to_string()),
    );
    v.waehlen(Knoten::Firmenwerte);
    assert!(v.eingeben(&Feld::Wert("wage".into()), "62"));
    let g = leistung(&v, AW24);
    v.waehlen(Knoten::Leistung(g));
    assert!(v.eingeben(&Feld::Stunden, "0,45"));
    // Platz B ändert dieselbe Bauleistung (nicht den Lohn)
    let (mut b, _) = Company::laden(&pfad, true);
    let mut s2 = haus();
    let mut vb = Verwaltung::open(&s2, Some(&b), None);
    vb.waehlen(Knoten::Leistung(g));
    assert!(vb.eingeben(&Feld::Stunden, "0,7"));
    s2.fuer_firma(STEP, &mut b, &h, vb.ops()).expect("Stand B");
    assert!(s.fuer_firma(STEP, &mut a, &h, v.ops()).is_err());
    v.neu_grundlage(&a);
    let ops = v.ops().to_vec();
    let _ = std::fs::remove_dir_all(&dir);
    let lohn = ops.iter().find_map(|o| match o {
        Op::FirmenwertSetzen { schluessel, wert } if schluessel == "wage" => Some(*wert),
        _ => None,
    });
    assert_eq!(lohn, Some(Dez::ganz(62)), "{ops:?}");
}
