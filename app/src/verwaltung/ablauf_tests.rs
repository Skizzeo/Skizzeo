//! Tests zu den geführten Abläufen in der Verwaltung (KA-3b5, paket-ka3b
//! §5 Abnahme 9, soll-ka-3d).

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

fn ordner(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("skizzeo-ablauf-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// Firmenkatalog mit Verwaltungskennwort: die Verwaltung arbeitet am
/// Entwurf.
fn mit_kennwort(dir: &std::path::Path) -> (Company, Scene) {
    let (mut c, _) = Company::laden(&dir.join("firmenkatalog.szk"), true);
    let mut s = haus();
    let k = Op::KennwortSetzen {
        pw: sk_cost::verwaltung::Pruefwert::neu("Polier7", [5; 16]),
    };
    s.fuer_firma(STEP, &mut c, &h(), &[k]).unwrap();
    (c, s)
}

fn stein(v: &Verwaltung) -> Guid {
    v.verwaltungs_ablaeufe()
        .find(|a| a.name == "Neuen Stein mit Preis anlegen")
        .unwrap()
        .guid
}

fn seite(v: &Verwaltung) -> usize {
    v.assistent.as_ref().unwrap().seite
}

/// Wählt auf der offenen Seite den Eintrag `name` der Auswahl.
fn waehlen(v: &mut Verwaltung, name: &str) {
    let opts = v.a_optionen_jetzt();
    let i = opts.iter().position(|(_, n)| n == name).expect(name);
    let x = v.assistent.as_mut().unwrap();
    x.oben = i;
    v.a_klick(Some(assistent::AZiel::Option(0)), true);
}

/// Abnahme 9: „Neuen Stein mit Preis anlegen“ auf vier Seiten. 600 mm
/// bleibt bei Frage 2 stehen; Porenbeton, 300 mm und je m³ 110,00 ergeben
/// einen Artikel mit 33,00 €/m², den vorbelegten Namen und eine Quelle,
/// die die Eingabe nennt, mit einer `[log]`-Zeile und einem
/// Schreibvorgang in den Entwurf. Die Firmendatei bleibt bytegleich.
#[test]
fn stein_mit_preis_anlegen() {
    let dir = ordner("stein");
    let (mut c, s) = mit_kennwort(&dir);
    let firma = std::fs::read(c.path()).unwrap();
    let mut v = Verwaltung::open(&s, Some(&c), None);
    let namen: Vec<&str> = v.verwaltungs_ablaeufe().map(|a| a.name.as_str()).collect();
    assert_eq!(
        namen,
        [
            "Neuen Stein mit Preis anlegen",
            "Verrechnungslohn für das neue Jahr setzen"
        ],
        "nur Abläufe der Verwaltung"
    );
    let baum = v.zeilen();
    let z = baum.iter().find(|z| z.knoten == Knoten::Ablaeufe).unwrap();
    assert_eq!(z.anzahl, Some(2));
    v.ablauf_starten(stein(&v));
    assert_eq!(v.assistent.as_ref().unwrap().seiten.len(), 4);
    // Seite 1: Baustoff, ohne Wahl kein Weiter
    v.ablauf_weiter();
    assert_eq!(seite(&v), 0);
    assert!(v.assistent.as_ref().unwrap().fehler.is_some());
    waehlen(&mut v, "Porenbeton");
    v.ablauf_weiter();
    assert_eq!(seite(&v), 1);
    // Seite 2: Dicke, 600 mm geht nicht
    v.a_tippen(0, "600");
    v.ablauf_weiter();
    assert_eq!(seite(&v), 1);
    assert_eq!(
        v.assistent
            .as_ref()
            .unwrap()
            .fehler
            .as_ref()
            .map(|f| f.1.as_str()),
        Some("Bitte zwischen 50 mm und 500 mm.")
    );
    v.a_tippen(0, "300");
    v.ablauf_weiter();
    assert_eq!(seite(&v), 2);
    // Seite 3: Preis je m³
    v.a_klick(Some(assistent::AZiel::Einheit(1)), true);
    v.a_tippen(0, "110,00");
    v.ablauf_weiter();
    assert_eq!(seite(&v), 3);
    // Zurück behält die Antworten
    v.ablauf_zurueck();
    assert_eq!(seite(&v), 2);
    v.ablauf_weiter();
    let x = v.assistent.as_ref().unwrap();
    let name = x.seiten[3][0];
    assert_eq!(x.te[name].text, "Porenbeton-Stein d=300mm", "vorbelegt");
    assert_eq!(
        x.te[x.seiten[3][1]].text, "",
        "Quelle leer, nur der Platzhalter"
    );
    let logs = |t: &str| t.lines().filter(|l| l.starts_with("[log]")).count();
    let vorher = logs(c.entwurf().unwrap_or(c.geladen()));
    // Anlegen: ein Schreibvorgang in den Entwurf
    v.ablauf_weiter();
    assert!(v.assistent.is_none());
    assert_eq!(v.ops().len(), 1);
    assert!(v.entwurf_faellig());
    c.fuer_entwurf(&h(), v.ops()).expect("Entwurf geschrieben");
    v.entwurf_gespeichert(&c);
    assert_eq!(
        v.meldung.as_deref(),
        Some("Stein Porenbeton-Stein d=300mm mit 33,00 €/m² angelegt.")
    );
    let e = c.entwurf().unwrap().to_string();
    assert_eq!(logs(&e), vorher + 1, "eine [log]-Zeile");
    let a = v
        .jetzt
        .artikel
        .iter()
        .find(|a| a.name == "Porenbeton-Stein d=300mm")
        .expect("angelegt");
    assert_eq!(a.preis, Some(Dez::ganz(33)));
    assert_eq!(a.t, Some(Dez::ganz(300)));
    assert_eq!(v.wahl, Knoten::ArtikelSatz(a.guid));
    assert!(
        e.contains("source=\"eingegeben 110,00 €/m³ × 0,3 m\""),
        "Quelle nennt die Eingabe"
    );
    assert_eq!(
        std::fs::read(c.path()).unwrap(),
        firma,
        "Firmendatei bytegleich"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// Abbrechen und Esc ändern nichts; ein Ablauf mit Lücke in `nr` ist grau
/// mit Befund 102, startet nicht und bleibt beim Speichern bytegleich.
#[test]
fn abbruch_und_ungueltig() {
    let dir = ordner("grau");
    let pfad = dir.join("firmenkatalog.szk");
    let (mut c, _) = Company::laden(&pfad, true);
    let mut s = haus();
    let lohn = |w: i64| Op::FirmenwertSetzen {
        schluessel: "wage".into(),
        wert: Dez::ganz(w),
    };
    s.fuer_firma(STEP, &mut c, &h(), &[lohn(61)]).unwrap();
    let kaputt = "[flow] guid=1S7bUW00100807000000F1 name=\"Lohn mit Lücke\" kind=admin\n\
[flowstep] guid=1S7bUW00100808000000F1 flow=1S7bUW00100807000000F1 nr=1 step=ask key=lohn text=\"Lohn?\" type=money\n\
[flowstep] guid=1S7bUW00100808000000F2 flow=1S7bUW00100807000000F1 nr=3 step=op op=firmenwert_setzen args=\"schluessel=wage wert={lohn}\"\n\
[flowstep] guid=1S7bUW00100808000000F3 flow=1S7bUW00100807000000F1 nr=4 step=done text=\"Fertig.\"\n";
    let mut t = std::fs::read_to_string(&pfad).unwrap();
    t.push_str(kaputt);
    std::fs::write(&pfad, &t).unwrap();
    let (mut c, _) = Company::laden(&pfad, true);
    let mut v = Verwaltung::open(&s, Some(&c), None);
    let a = v
        .verwaltungs_ablaeufe()
        .find(|a| a.name == "Lohn mit Lücke")
        .expect("im Baum");
    assert_eq!(a.befund.as_ref().map(|b| b.regel), Some(102));
    let g = a.guid;
    let z = v.zeilen();
    v.offen.insert(Knoten::Ablaeufe);
    let z2 = v.zeilen();
    assert!(z2.len() > z.len());
    assert!(z2.iter().any(|z| z.knoten == Knoten::Ablauf(g) && z.grau));
    v.ablauf_starten(g);
    assert!(v.assistent.is_none(), "ungültig startet nicht");
    // Abbrechen nach einer Antwort: nichts geändert
    let lohn_ablauf = v
        .verwaltungs_ablaeufe()
        .find(|a| a.name.starts_with("Verrechnungslohn"))
        .unwrap()
        .guid;
    v.ablauf_starten(lohn_ablauf);
    v.a_tippen(0, "64");
    v.a_taste(Key::Escape, Modifiers::default());
    assert!(v.assistent.is_none());
    assert!(v.ops().is_empty());
    // Einzelplatz: der Lohn wartet auf OK, die Ablaufzeilen bleiben
    v.ablauf_starten(lohn_ablauf);
    v.a_tippen(0, "64");
    v.ablauf_weiter();
    assert!(v.assistent.is_none());
    assert_eq!(v.wahl, Knoten::Firmenwerte);
    assert_eq!(v.ops(), [lohn(64)]);
    s.fuer_firma(STEP, &mut c, &h(), v.ops()).expect("schreibt");
    let neu = std::fs::read_to_string(&pfad).unwrap();
    assert!(neu.contains(kaputt), "Ablaufzeilen bytegleich");
    assert!(neu.contains("num=64"));
    let _ = std::fs::remove_dir_all(&dir);
}

/// Ist-Bild (soll-ka-3d), nur auf Wunsch:
/// `SKIZZEO_ISTBILDER=<ordner> cargo test -p skizzeo istbilder -- --ignored`
#[test]
#[ignore = "legt Ist-Bilder ab, nur mit SKIZZEO_ISTBILDER"]
fn istbilder_ka3b5() {
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
    let dir = ordner("ist");
    let (c, s) = mit_kennwort(&dir);
    let mut v = Verwaltung::open(&s, Some(&c), None);
    v.offen.insert(Knoten::Ablaeufe);
    let g = stein(&v);
    v.waehlen(Knoten::Ablauf(g));
    v.ablauf_starten(g);
    waehlen(&mut v, "Porenbeton");
    v.ablauf_weiter();
    v.a_tippen(0, "200");
    v.ablauf_weiter();
    v.a_tippen(0, "25,30");
    let (b, _, _) = v.paint(&t, &fonts, &w);
    std::fs::write(ziel.join("ist-ka-3b5-ablauf-preis.png"), b.to_png()).unwrap();
    v.ablauf_weiter();
    let (b, _, _) = v.paint(&t, &fonts, &w);
    std::fs::write(ziel.join("ist-ka-3b5-ablauf-anlegen.png"), b.to_png()).unwrap();
    let _ = std::fs::remove_dir_all(&dir);
}
