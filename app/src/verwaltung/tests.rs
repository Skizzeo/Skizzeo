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
    let teile = wirkung::zeile(&v.wirkung.haeuser);
    assert_eq!(teile[0].0, "Standardhaus");
    assert!(teile[0].1.is_some(), "alt durchgestrichen");
    assert_eq!(s.model().revision(), rev, "Projekt unverändert");
    assert!(v.eingeben(&Feld::Stunden, &komma(alt)));
    assert!(v.ops.is_empty(), "{:?}", v.ops);
    assert_eq!(v.vorher_text(&Feld::Stunden), None);
    let teile = wirkung::zeile(&v.wirkung.haeuser);
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
    let mut v = Verwaltung::open(&s, None, None);
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
    assert!(out.closed && !out.ok);
    assert_eq!(std::fs::read(&pfad).unwrap(), datei);
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
    let mut v = Verwaltung::open(&s, None, None);
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
    let mut v = Verwaltung::open(&s, None, None);
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
}
