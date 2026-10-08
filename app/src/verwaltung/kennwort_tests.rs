//! Tests zum Verwaltungskennwort (KA-3b1, paket-ka3b §5 Abnahme 1 und 2).

use super::kennwort::{FALSCH, VERSCHIEDEN};
use super::*;
use std::path::PathBuf;

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

fn firma(name: &str) -> (Company, PathBuf) {
    let dir = std::env::temp_dir().join(format!("skizzeo-kennwort-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let (c, _) = Company::laden(&dir.join("firmenkatalog.szk"), true);
    (c, dir)
}

fn win() -> Win {
    Win {
        w: 1400,
        h: 900,
        top: 40,
        scale: 1.0,
    }
}

fn taste(key: Key) -> Event {
    Event::Key {
        key,
        down: true,
        repeat: false,
        mods: Modifiers::default(),
    }
}

fn tippen(v: &mut Verwaltung, cx: &mut Ctx, text: &str) {
    for ch in text.chars() {
        v.handle(&Event::Text(ch), cx);
    }
}

fn h() -> sk_cost::Herkunft {
    sk_cost::Herkunft::neu(sk_cost::HerkunftArt::Manual, "2026-10-08", "20:00")
}

/// Abnahme 1 und 2: Kennwort im Blatt setzen (zwei verschiedene Eingaben
/// sperren), OK schreibt `pw` mit 64 Hex; an einem zweiten Platz fragt die
/// Verwaltung, ein falsches Kennwort lässt sie zu, das richtige gibt frei;
/// der Nutzer schreibt nicht in die Firma und keine `nur_admin`-Operation
/// ins Projekt (Befund 93).
#[test]
fn kennwort_setzen_und_abfragen() {
    let fonts = super::tests::schriften();
    let t = Theme::dark();
    let w = win();
    let mut cx = Ctx {
        fonts: &fonts,
        win: w,
    };
    let (mut c, dir) = firma("setzen");
    let mut s = haus();
    let mut v = Verwaltung::open(&s, Some(&c), None);
    v.waehlen(Knoten::Kennwort);
    v.aktion(Aktion::Kennwort);
    tippen(&mut v, &mut cx, "Polier7");
    v.handle(&taste(Key::Tab), &mut cx);
    tippen(&mut v, &mut cx, "Polier8");
    v.handle(&taste(Key::Enter), &mut cx);
    assert!(
        v.setz.as_ref().is_some_and(|sb| sb.verschieden),
        "{VERSCHIEDEN}"
    );
    assert!(v.ops().is_empty());
    let _ = v.paint(&t, &fonts, &w);
    v.handle(&taste(Key::Backspace), &mut cx);
    tippen(&mut v, &mut cx, "7");
    v.handle(&taste(Key::Enter), &mut cx);
    assert!(v.setz.is_none());
    assert!(
        matches!(v.ops(), [Op::KennwortSetzen { pw }] if !pw.ist_leer()),
        "{:?}",
        v.ops()
    );
    // Das Kennwort steht weder in der Operation noch in ihrer Debug-Ausgabe
    assert!(!format!("{:?}", v.ops()).contains("Polier7"));
    let _ = v.paint(&t, &fonts, &w);
    s.fuer_firma(STEP, &mut c, &h(), v.ops()).expect("schreibt");
    let text = std::fs::read_to_string(c.path()).unwrap();
    let pw = text
        .lines()
        .flat_map(|z| z.split_whitespace())
        .find_map(|f| f.strip_prefix("pw="))
        .expect("pw gesetzt");
    let pw = pw.trim_matches('"');
    let teile: Vec<&str> = pw.split('$').collect();
    assert_eq!(teile[..2], ["pbkdf2-sha256", "200000"], "{pw}");
    assert_eq!((teile[2].len(), teile[3].len()), (32, 64), "{pw}");
    assert!(!text.contains("Polier7"), "Kennwort nie im Klartext");
    assert!(text.contains("op=kennwort_setzen"), "{text}");

    // Zweiter Platz: Nutzer, bis das Kennwort eingegeben ist
    let mut s2 = haus();
    s2.rolle = sk_cost::Rolle::Nutzer;
    let mut v = Verwaltung::open(&s2, Some(&c), None);
    v.sperren();
    let (bild, _, _) = v.paint(&t, &fonts, &w);
    assert!(bild.width < 700, "kleines Blatt, nicht das ganze Fenster");
    tippen(&mut v, &mut cx, "polier7");
    let out = v.handle(&taste(Key::Enter), &mut cx);
    assert!(!out.frei && !out.closed);
    let a = v.abfrage.as_ref().expect("bleibt zu");
    assert!(a.falsch, "{FALSCH}");
    assert!(a.te.text.is_empty());
    let _ = v.paint(&t, &fonts, &w);
    // Nutzer: Firma nicht, nur_admin auch im Projekt nicht
    let lohn = Op::FirmenwertSetzen {
        schluessel: "wage".into(),
        wert: Dez::ganz(70),
    };
    let datei = std::fs::read(c.path()).unwrap();
    let e = s2
        .fuer_firma(STEP, &mut c, &h(), std::slice::from_ref(&lohn))
        .expect_err("Nutzer schreibt die Firma nicht");
    assert!(e.to_string().contains("nur in der Verwaltung"), "{e}");
    assert_eq!(std::fs::read(c.path()).unwrap(), datei);
    let g = v
        .jetzt
        .leistungen
        .iter()
        .find(|l| {
            l.kurz
                .starts_with("AW Porenbeton-Planstein PP2-0,35 d=24cm")
        })
        .unwrap()
        .guid;
    let mut va = Verwaltung::open(&s, Some(&c), None);
    va.waehlen(Knoten::Leistung(g));
    assert!(va.eingeben(&Feld::Stunden, "0,45"));
    assert!(va.ops().iter().any(|o| o.nur_admin()), "{:?}", va.ops());
    let befunde = s2
        .kosten_folge("Stunden", Some(c.library()), &h(), va.ops())
        .expect_err("nur_admin");
    assert!(befunde.iter().any(|b| b.regel == 93), "{befunde:?}");
    // Projektwert ohne nur_admin geht weiter
    s2.kosten_folge("Lohn", Some(c.library()), &h(), &[lohn])
        .expect("Nutzer im Projekt");
    // Richtiges Kennwort: frei, das Fenster wird groß
    tippen(&mut v, &mut cx, "Polier7");
    let out = v.handle(&taste(Key::Enter), &mut cx);
    assert!(out.frei && v.abfrage.is_none());
    let (bild, _, _) = v.paint(&t, &fonts, &w);
    assert!(bild.width > 1000);
    let _ = std::fs::remove_dir_all(&dir);
}

/// Leeres Kennwort heißt zurück zum Einzelplatz; ohne Kennwort ist „leer“
/// keine Änderung. Zweimal dasselbe Kennwort gibt verschiedene Salze.
#[test]
fn kennwort_entfernen() {
    let fonts = super::tests::schriften();
    let mut cx = Ctx {
        fonts: &fonts,
        win: win(),
    };
    let (mut c, dir) = firma("entfernen");
    let mut s = haus();
    let mut v = Verwaltung::open(&s, Some(&c), None);
    v.aktion(Aktion::Kennwort);
    v.handle(&taste(Key::Enter), &mut cx);
    v.handle(&taste(Key::Enter), &mut cx);
    assert!(v.setz.is_none() && v.ops().is_empty());
    let k = Op::KennwortSetzen {
        pw: sk_cost::verwaltung::Pruefwert::neu("a", [7; 16]),
    };
    s.fuer_firma(STEP, &mut c, &h(), &[k]).unwrap();
    assert!(sk_cost::verwaltung::hat_kennwort(c.library()));
    let mut v = Verwaltung::open(&s, Some(&c), None);
    v.aktion(Aktion::Kennwort);
    v.handle(&taste(Key::Enter), &mut cx);
    v.handle(&taste(Key::Enter), &mut cx);
    assert!(matches!(v.ops(), [Op::KennwortSetzen { pw }] if pw.ist_leer()));
    // Mit Kennwort kommt das in den Entwurf; erst die Freigabe entfernt es
    s.fuer_firma(STEP, &mut c, &h(), v.ops()).unwrap();
    assert!(sk_cost::verwaltung::hat_kennwort(c.library()));
    s.freigeben(super::FREIGEGEBEN, &mut c, &h()).unwrap();
    assert!(!sk_cost::verwaltung::hat_kennwort(c.library()));
    let mut salze = Vec::new();
    for _ in 0..2 {
        let mut v = Verwaltung::open(&s, Some(&c), None);
        v.aktion(Aktion::Kennwort);
        tippen(&mut v, &mut cx, "Mauer");
        v.handle(&taste(Key::Enter), &mut cx);
        tippen(&mut v, &mut cx, "Mauer");
        v.handle(&taste(Key::Enter), &mut cx);
        match v.ops() {
            [Op::KennwortSetzen { pw }] => {
                salze.push(pw.text().split('$').nth(2).unwrap().to_string())
            }
            o => panic!("{o:?}"),
        }
    }
    assert_ne!(salze[0], salze[1]);
    let _ = std::fs::remove_dir_all(&dir);
}

/// Ist-Bilder (soll-ka-3c-kennwort), nur auf Wunsch:
/// `SKIZZEO_ISTBILDER=<ordner> cargo test -p skizzeo istbilder -- --ignored`
#[test]
#[ignore = "legt Ist-Bilder ab, nur mit SKIZZEO_ISTBILDER"]
fn istbilder_ka3b1() {
    let Some(ziel) = std::env::var_os("SKIZZEO_ISTBILDER").map(PathBuf::from) else {
        return;
    };
    let fonts = super::tests::schriften();
    if fonts.regular.is_none() {
        return;
    }
    std::fs::create_dir_all(&ziel).unwrap();
    let t = Theme::dark();
    let w = Win {
        w: 1180,
        h: 820,
        top: 30,
        scale: 1.0,
    };
    let mut cx = Ctx {
        fonts: &fonts,
        win: w,
    };
    let (mut c, dir) = firma("ist");
    let mut s = haus();
    let mut v = Verwaltung::open(&s, Some(&c), None);
    v.waehlen(Knoten::Kennwort);
    let (b, _, _) = v.paint(&t, &fonts, &w);
    std::fs::write(ziel.join("ist-ka-3b1-kennwort-seite.png"), b.to_png()).unwrap();
    v.aktion(Aktion::Kennwort);
    tippen(&mut v, &mut cx, "Polier7");
    v.handle(&taste(Key::Tab), &mut cx);
    tippen(&mut v, &mut cx, "Polier8");
    v.handle(&taste(Key::Enter), &mut cx);
    let (b, _, _) = v.paint(&t, &fonts, &w);
    std::fs::write(ziel.join("ist-ka-3b1-kennwort-setzen.png"), b.to_png()).unwrap();
    v.handle(&taste(Key::Backspace), &mut cx);
    tippen(&mut v, &mut cx, "7");
    v.handle(&taste(Key::Enter), &mut cx);
    s.fuer_firma(STEP, &mut c, &h(), v.ops()).expect("schreibt");
    let mut v = Verwaltung::open(&s, Some(&c), None);
    v.sperren();
    tippen(&mut v, &mut cx, "Polier");
    let (b, _, _) = v.paint(&t, &fonts, &w);
    std::fs::write(ziel.join("ist-ka-3b1-abfrage.png"), b.to_png()).unwrap();
    v.handle(&taste(Key::Enter), &mut cx);
    let (b, _, _) = v.paint(&t, &fonts, &w);
    std::fs::write(ziel.join("ist-ka-3b1-abfrage-falsch.png"), b.to_png()).unwrap();
    let _ = std::fs::remove_dir_all(&dir);
}
