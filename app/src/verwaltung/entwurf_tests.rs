//! Tests zu Entwurf, Vorschau und Freigabe (KA-3b2/3b3, paket-ka3b §5
//! Abnahme 3, 4, 5, 8a und 10).

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

fn h() -> sk_cost::Herkunft {
    sk_cost::Herkunft::neu(sk_cost::HerkunftArt::Manual, "2026-10-08", "20:00")
}

/// Firmenkatalog mit Verwaltungskennwort in einem eigenen Ordner.
fn mit_kennwort(name: &str) -> (Company, Scene, PathBuf) {
    let dir = std::env::temp_dir().join(format!("skizzeo-entwurf-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let (mut c, _) = Company::laden(&dir.join("firmenkatalog.szk"), true);
    let mut s = haus();
    let k = Op::KennwortSetzen {
        pw: sk_cost::verwaltung::Pruefwert::neu("Polier7", [5; 16]),
    };
    s.fuer_firma(STEP, &mut c, &h(), &[k]).unwrap();
    assert!(sk_cost::verwaltung::hat_kennwort(c.library()));
    (c, s, dir)
}

/// Was die App bei `Out::entwurf` tut.
fn schreiben(v: &mut Verwaltung, c: &mut Company) {
    assert!(v.entwurf_faellig(), "{:?}", v.befunde);
    c.fuer_entwurf(&h(), v.ops()).expect("Entwurf geschrieben");
    v.entwurf_gespeichert(c);
}

fn lohn(lib: &Library) -> Dez {
    sk_cost::lesen::firma_oder_werk(&Model::new(), Some(lib))
        .werte
        .lohn
}

/// Abnahme 3–5: Der Admin ändert zwei Werte; jede Eingabe steht gleich im
/// Entwurf (`status=draft`), die Firmendatei bleibt bytegleich, ein zweiter
/// Platz rechnet weiter mit dem freigegebenen Stand. Pille und Leiste
/// zählen zwei Änderungen, der Baum hat Punkte. Die Vorschau nennt alt →
/// neu mit Herkunft und die Referenzhäuser mit Stand und Entwurf gleich
/// `lesen::kosten` beider Kataloge. „Freigeben“ macht den nächsten Stand,
/// legt den vorigen bytegleich ab und entfernt den Entwurf.
#[test]
fn entwurf_vorschau_freigabe() {
    let (mut c, mut s, dir) = mit_kennwort("freigabe");
    let firma = dir.join("firmenkatalog.szk");
    let vorher = std::fs::read(&firma).unwrap();
    let stand = sk_cost::lesen::firma_oder_werk(s.model(), Some(c.library()))
        .firma_stand
        .unwrap();
    let mut v = Verwaltung::open(&s, Some(&c), None);
    assert_eq!(v.entwurf_anzahl(), 0);
    let w = Win {
        w: 1400,
        h: 900,
        top: 40,
        scale: 1.0,
    };
    assert!(v.knoepfe(&w).is_empty(), "kein OK im Entwurf");
    let alt = lohn(c.library());
    assert!(v.eingeben(&Feld::Wert("wage".into()), "62"));
    schreiben(&mut v, &mut c);
    assert!(v.ops().is_empty());
    let entwurf = std::fs::read_to_string(c.entwurf_pfad()).unwrap();
    let kopf = entwurf
        .lines()
        .find(|l| l.starts_with("[catalog]"))
        .unwrap();
    assert!(
        kopf.contains("status=draft") && kopf.contains(&format!("stand={stand}")),
        "{kopf}"
    );
    assert_eq!(
        std::fs::read(&firma).unwrap(),
        vorher,
        "Firmendatei bytegleich"
    );
    // Ein Artikel mit Preis bekommt einen neuen
    let art = v
        .jetzt
        .artikel
        .iter()
        .find(|a| a.preis.is_some() && !a.retired)
        .unwrap()
        .clone();
    v.waehlen(Knoten::ArtikelSatz(art.guid));
    assert!(v.eingeben(&Feld::Preis(art.guid), "31,50"));
    schreiben(&mut v, &mut c);
    assert_eq!(v.entwurf_anzahl(), 2);
    assert!(v.entwurf_geaendert(&Knoten::Firmenwerte));
    assert!(v.entwurf_geaendert(&Knoten::ArtikelSatz(art.guid)));
    assert_eq!(v.jetzt.werte.lohn, Dez::ganz(62));
    // Der zweite Platz rechnet weiter mit dem freigegebenen Stand
    let (c2, _) = Company::laden(&firma, false);
    assert_eq!(lohn(c2.library()), alt);
    assert_eq!(lohn(c.library()), alt);
    // Vorschau
    v.vorschau_oeffnen();
    let vs = v.vorschau.as_ref().unwrap();
    let l = vs
        .zeilen
        .iter()
        .find(|z| z.satz.abschnitt == "rate")
        .expect("Lohn");
    assert_eq!(
        (l.art.as_str(), l.name.as_str()),
        ("Firmenwert", "Verrechnungslohn")
    );
    assert!(
        l.alt.ends_with(" €/h") && l.neu.starts_with("62") && l.neu.ends_with(" €/h"),
        "{l:?}"
    );
    assert_eq!(l.herkunft, "manuell");
    let p = vs
        .zeilen
        .iter()
        .find(|z| z.satz.abschnitt == "article")
        .expect("Preis");
    assert!(p.neu.starts_with("31,5") && p.neu.contains(" €/"), "{p:?}");
    assert!(vs.befunde.is_empty(), "{:?}", vs.befunde);
    let rh1 = vs.haeuser.first().expect("Standardhaus");
    let m = haus().model().clone();
    let sched = sk_model::qto::schedule(&m);
    let summe = |lib: &Library| {
        let k = sk_cost::lesen::firma_oder_werk(&m, Some(lib));
        sk_cost::lesen::kosten(&m, &sched, &k, &sk_cost::Umfang::projekt()).netto
    };
    assert_eq!(rh1.stand, summe(c.library()));
    let e = sk_model::read_szk_with(&entwurf_text(&c), &sk_cost::lesen::ABSCHNITTE_SZK).unwrap();
    assert_eq!(
        rh1.entwurf,
        summe(&sk_cost::verwaltung::wie_freigegeben(&e))
    );
    assert!(rh1.entwurf > rh1.stand);
    assert!(
        vs.satz
            .as_ref()
            .is_some_and(|x| x.contains("Verrechnungslohn")),
        "{:?}",
        vs.satz
    );
    // Freigeben
    s.freigeben(FREIGEGEBEN, &mut c, &h()).expect("freigegeben");
    v.entwurf_gespeichert(&c);
    assert!(v.vorschau.is_none() && v.entwurf_anzahl() == 0);
    assert!(!c.entwurf_pfad().exists(), "Entwurf weg");
    let neu = std::fs::read_to_string(&firma).unwrap();
    let kopf = neu.lines().find(|l| l.starts_with("[catalog]")).unwrap();
    assert!(kopf.contains(&format!("stand={}", stand + 1)) && kopf.contains("status=released"));
    let abl = dir.join(format!("firmenkatalog-staende/stand-{stand:04}.szk"));
    assert_eq!(
        std::fs::read(abl).unwrap(),
        vorher,
        "voriger Stand bytegleich abgelegt"
    );
    assert_eq!(lohn(c.library()), Dez::ganz(62));
    assert_eq!(
        neu.lines()
            .filter(|l| l.starts_with("[log]") && l.contains(&format!("stand={}", stand + 1)))
            .count(),
        2
    );
    // Das Kennwort bleibt
    assert!(sk_cost::verwaltung::kennwort_stimmt(c.library(), "Polier7"));
    let _ = std::fs::remove_dir_all(&dir);
}

fn entwurf_text(c: &Company) -> String {
    std::fs::read_to_string(c.entwurf_pfad()).unwrap()
}

/// Abnahme 8a: „Änderung verwerfen“ nimmt eine Änderung aus dem Entwurf
/// (samt ihrer Protokollzeile), „Entwurf verwerfen“ legt ihn ins Archiv; die
/// Firmendatei bleibt bytegleich.
#[test]
fn aenderung_und_entwurf_verwerfen() {
    let (mut c, s, dir) = mit_kennwort("verwerfen");
    let firma = dir.join("firmenkatalog.szk");
    let vorher = std::fs::read(&firma).unwrap();
    let alt = lohn(c.library());
    let mut v = Verwaltung::open(&s, Some(&c), None);
    assert!(v.eingeben(&Feld::Wert("wage".into()), "62"));
    schreiben(&mut v, &mut c);
    assert!(v.eingeben(&Feld::Wert("surcharge".into()), "12"));
    schreiben(&mut v, &mut c);
    assert_eq!(v.entwurf_anzahl(), 2);
    let log_lohn = |t: &str| {
        t.lines()
            .filter(|l| l.starts_with("[log]") && l.contains("of=wage"))
            .count()
    };
    let firma_text = String::from_utf8(vorher.clone()).unwrap();
    assert_eq!(log_lohn(&entwurf_text(&c)), log_lohn(&firma_text) + 1);
    c.entwurf_satz_verwerfen(&SatzId::neu("rate", "wage"))
        .unwrap();
    v.entwurf_gespeichert(&c);
    assert_eq!(v.entwurf_anzahl(), 1);
    assert_eq!(v.jetzt.werte.lohn, alt);
    assert_eq!(log_lohn(&entwurf_text(&c)), log_lohn(&firma_text));
    let ablage = c.entwurf_verwerfen().unwrap();
    v.entwurf_gespeichert(&c);
    assert_eq!(v.entwurf_anzahl(), 0);
    assert!(!c.entwurf_pfad().exists());
    assert!(ablage
        .file_name()
        .unwrap()
        .to_string_lossy()
        .starts_with("entwurf-verworfen-"));
    assert!(std::fs::read_to_string(&ablage)
        .unwrap()
        .contains("status=draft"));
    assert_eq!(
        std::fs::read(&firma).unwrap(),
        vorher,
        "Firmendatei bytegleich"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// Abnahme 10: Zwei Administratoren am selben Entwurf; der zweite
/// Speichervorgang wird abgelehnt und lädt neu, danach geht er.
#[test]
fn zwei_admins() {
    let (mut c1, _s, dir) = mit_kennwort("zwei");
    let (mut c2, _) = Company::laden(&dir.join("firmenkatalog.szk"), false);
    let op = |w: i64| Op::FirmenwertSetzen {
        schluessel: "wage".into(),
        wert: Dez::ganz(w),
    };
    c1.fuer_entwurf(&h(), &[op(62)]).unwrap();
    let m = c2.fuer_entwurf(&h(), &[op(63)]).unwrap_err();
    assert_eq!(m.to_string(), crate::catalog::ENTWURF_GEAENDERT);
    assert!(
        c2.entwurf().is_some_and(|t| t.contains("num=62")),
        "neu geladen"
    );
    c2.fuer_entwurf(&h(), &[op(63)]).unwrap();
    assert!(entwurf_text(&c2).contains("num=63"));
    let _ = std::fs::remove_dir_all(&dir);
}

/// Ein Platz ohne eingegebenes Kennwort schreibt weder Firma noch Entwurf:
/// kein Speichern im Entwurf, kein „Änderung verwerfen“, kein „Entwurf
/// verwerfen“, kein „Freigeben“ (Szene und Firmenkatalog lehnen selbst ab;
/// Koordinator 20:47). Firmendatei und Entwurf bleiben bytegleich.
#[test]
fn nutzer_gibt_nicht_frei() {
    let (mut c, mut s, dir) = mit_kennwort("nutzer");
    let op = |w: i64| Op::FirmenwertSetzen {
        schluessel: "wage".into(),
        wert: Dez::ganz(w),
    };
    c.fuer_entwurf(&h(), &[op(62)]).unwrap();
    let firma = std::fs::read(c.path()).unwrap();
    let entwurf = std::fs::read(c.entwurf_pfad()).unwrap();
    c.set_nutzer(true);
    s.rolle = sk_cost::Rolle::Nutzer;
    let nur = crate::catalog::NUR_MIT_KENNWORT;
    assert_eq!(
        c.fuer_entwurf(&h(), &[op(63)]).unwrap_err().to_string(),
        nur
    );
    assert_eq!(c.fuer_firma(&h(), &[op(63)]).unwrap_err().to_string(), nur);
    let satz = SatzId::neu("rate", "wage");
    assert_eq!(
        c.entwurf_satz_verwerfen(&satz).unwrap_err().to_string(),
        nur
    );
    assert_eq!(c.entwurf_verwerfen().unwrap_err().to_string(), nur);
    assert_eq!(c.freigeben(&h()).unwrap_err().to_string(), nur);
    let e = s.freigeben(FREIGEGEBEN, &mut c, &h()).unwrap_err();
    assert!(e.to_string().contains("nur in der Verwaltung"), "{e}");
    assert_eq!(std::fs::read(c.path()).unwrap(), firma);
    assert_eq!(std::fs::read(c.entwurf_pfad()).unwrap(), entwurf);
    // Entsperrt geht es wieder
    c.set_nutzer(false);
    s.rolle = sk_cost::Rolle::Admin;
    s.freigeben(FREIGEGEBEN, &mut c, &h())
        .expect("Admin gibt frei");
    let _ = std::fs::remove_dir_all(&dir);
}

/// Mit Kennwort und entsperrt: „Auch für neue Häuser“ (Lohn) schreibt in
/// den Entwurf, dieses Haus rechnet gleich damit; nach „Freigeben“ ist es der
/// Firmenwert, und das Haus hat keinen eigenen Vermerk mehr: eine spätere
/// Firmenänderung (64) erscheint im Abgleich, nicht als eigener Wert.
#[test]
fn auch_fuer_neue_haeuser_mit_kennwort() {
    let (mut c, mut s, dir) = mit_kennwort("neue");
    let firma = dir.join("firmenkatalog.szk");
    let vorher = std::fs::read(&firma).unwrap();
    let op = |w: i64| Op::FirmenwertSetzen {
        schluessel: "wage".into(),
        wert: Dez::ganz(w),
    };
    let hinweis = s
        .fuer_firma_auch_hier("Lohn 62,00 €/h für neue Häuser", &mut c, &h(), &[op(62)])
        .expect("in den Entwurf");
    assert_eq!(
        hinweis.map(|m| m.to_string()).as_deref(),
        Some(crate::catalog::IM_ENTWURF)
    );
    assert_eq!(
        std::fs::read(&firma).unwrap(),
        vorher,
        "Firmendatei bytegleich"
    );
    assert!(entwurf_text(&c).contains("num=62"));
    let hier = |s: &Scene, c: &Company| {
        sk_cost::lesen::katalog(s.model(), Some(c.library()))
            .werte
            .lohn
    };
    assert_eq!(
        hier(&s, &c),
        Dez::ganz(62),
        "dieses Haus rechnet gleich damit"
    );
    // Freigeben: Firmenwert 62, der eigene Vermerk ist weg
    s.freigeben(FREIGEGEBEN, &mut c, &h()).expect("freigegeben");
    assert_eq!(lohn(c.library()), Dez::ganz(62));
    assert_eq!(hier(&s, &c), Dez::ganz(62));
    let eigen = s
        .model()
        .ext("origin")
        .filter(|r| r.id.as_deref() == Some("wage"))
        .any(|r| r.line.contains("proj=1"));
    assert!(!eigen, "kein eigener Vermerk nach der Freigabe");
    // Später 64 für die Firma: Das Haus folgt im Abgleich
    c.fuer_entwurf(&h(), &[op(64)]).unwrap();
    c.freigeben(&h()).unwrap();
    let a = sk_cost::abgleich::abgleich(s.model(), Some(c.library())).expect("Abgleich");
    assert!(a.saetze.iter().any(|x| x.kennung == "wage"), "{a:?}");
    assert!(a.eigene.is_empty(), "{a:?}");
    let _ = std::fs::remove_dir_all(&dir);
}

/// Ist-Bilder (soll-ka-3b-freigabe), nur auf Wunsch:
/// `SKIZZEO_ISTBILDER=<ordner> cargo test -p skizzeo istbilder -- --ignored`
#[test]
#[ignore = "legt Ist-Bilder ab, nur mit SKIZZEO_ISTBILDER"]
fn istbilder_ka3b3() {
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
    let (mut c, s, dir) = mit_kennwort("ist");
    let mut v = Verwaltung::open(&s, Some(&c), None);
    assert!(v.eingeben(&Feld::Wert("wage".into()), "62"));
    schreiben(&mut v, &mut c);
    let aw = v
        .jetzt
        .leistungen
        .iter()
        .find(|l| l.kurz.starts_with("AW Porenbeton") && l.stunden > Dez::NULL)
        .map(|l| l.guid);
    if let Some(g) = aw {
        v.waehlen(Knoten::Leistung(g));
        assert!(v.eingeben(&Feld::Stunden, "0,50"));
        schreiben(&mut v, &mut c);
    }
    let art = v
        .jetzt
        .artikel
        .iter()
        .filter(|a| a.preis.is_some() && !a.retired)
        .find(|a| a.name.contains("Porenbeton"))
        .map(|a| a.guid);
    if let Some(g) = art {
        v.waehlen(Knoten::ArtikelSatz(g));
        assert!(v.eingeben(&Feld::Preis(g), "30"));
        schreiben(&mut v, &mut c);
    }
    if let Some(g) = aw {
        v.waehlen(Knoten::Leistung(g));
    }
    let (b, _, _) = v.paint(&t, &fonts, &w);
    std::fs::write(ziel.join("ist-ka-3b2-entwurf.png"), b.to_png()).unwrap();
    v.vorschau_oeffnen();
    let (b, _, _) = v.paint(&t, &fonts, &w);
    std::fs::write(ziel.join("ist-ka-3b3-vorschau.png"), b.to_png()).unwrap();
    let _ = std::fs::remove_dir_all(&dir);
}

/// Review 3au: Im Entwurf steht an AW24 Gerät 2,00 (Verwaltung). Danach
/// setzt derselbe Admin im Preisblatt die Stunden für dieses und neue
/// Häuser. Der Entwurf behält das Gerät und bekommt die Stunden.
#[test]
fn neue_haeuser_mit_kennwort_behaelt_entwurf() {
    let (mut c, mut s, dir) = mit_kennwort("entwurf-geraet");
    let mut v = Verwaltung::open(&s, Some(&c), None);
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
    v.waehlen(Knoten::Leistung(g));
    assert!(v.eingeben(&Feld::Geraet, "2"));
    schreiben(&mut v, &mut c);
    let k = s.katalog(Some((c.library(), c.stand())));
    let l = k.leistung(g).unwrap().clone();
    let op = Op::BauleistungAendern {
        bauleistung: g,
        daten: sk_cost::op::Bauleistung {
            stunden: Dez::lesen("0.9", 4).unwrap(),
            ..sk_cost::preis::bauleistung(&l)
        },
    };
    s.fuer_firma_auch_hier("Stunden für neue Häuser", &mut c, &h(), &[op])
        .expect("in den Entwurf");
    let e = sk_model::read_szk_with(&entwurf_text(&c), &sk_cost::lesen::ABSCHNITTE_SZK).unwrap();
    let e = sk_cost::verwaltung::wie_freigegeben(&e);
    let x = sk_cost::lesen::firma_oder_werk(&Model::new(), Some(&e))
        .leistung(g)
        .unwrap()
        .clone();
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(x.stunden, Dez::lesen("0.9", 4).unwrap());
    assert_eq!(x.geraet, Dez::ganz(2), "Gerät aus dem Entwurf bleibt");
}
