use super::*;
// Abnahmetests A140–A144: F-13 „Automatisch sichern“ (Jörns Ja 07.10.
// 03:46). Gestaltung: einstellungen/paket-f13-sichern.md (Abschnitt 6) und
// Sollbild soll-sichern-1.png. Spezifikation: test/abnahme-sichern.md.
// Aussehen und Bedienung der Startkarte prüfen die Handtests H122–H125.
//
// Ersetzt die erste Fassung (Sicherung neben der .szo, beim Schließen
// gelöscht).
//
// Einbau: als `mod sichern { use super::*; … }` ans Ende von
// app/src/abnahme.rs. Nutzt aus abnahme.rs: haus_b11, test_dir, cam3d,
// tool, click, key, W, H.
//
// Angenommene Namen stehen nur in den Adaptern (Vorschlag, frei wählbar):
// `crate::autosave::{AutoSave, start, restore, card_title, age_text,
// when_text}`, `AutoSave::{new, tick, saved, closed}`. Der Sicherungsordner
// (in der App %APPDATA%\Skizzeo\Sicherungen) geht als Pfad hinein, die Zeit
// als Millisekunden seit Programmstart, damit niemand fünf Minuten warten
// muss.
//
// Annahme zum Takt: Gesichert wird, wenn das Modell seit der letzten
// Sicherung (bzw. seit Öffnen/Speichern) geändert ist und seit der letzten
// Sicherung (bzw. seit Öffnen/Speichern) mindestens 5 Minuten vergangen
// sind. Zählt der Bau anders, ändern sich nur die Zeiten in A140.

use crate::autosave::AutoSave;
use crate::document::Document;
use sk_model::{Category, Model};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

const MIN: u64 = 60_000;
const TAG: u64 = 24 * 60 * 60;

// ===== Adapter =====

/// Sicherer für den Ordner „Sicherungen“.
fn sicherer(ordner: &Path) -> AutoSave {
    AutoSave::new(ordner.to_path_buf())
}

/// Zeitgeber zur Zeit `t` ms: sichert, wenn fällig. `Some(pfad)`, wenn
/// jetzt eine Sicherung geschrieben wurde.
fn takt(a: &mut AutoSave, s: &Scene, doc: &Document, t: u64) -> Option<PathBuf> {
    a.tick(s.model(), doc, Duration::from_millis(t))
}

/// Nach Strg+S (Datei ist geschrieben, `doc` kennt den neuen Stand).
fn gespeichert(a: &mut AutoSave, doc: &Document, t: u64) {
    a.saved(doc, Duration::from_millis(t));
}

/// Programm normal beendet (auch „Nicht speichern“).
fn beendet(a: &mut AutoSave, doc: &Document) {
    a.closed(doc);
}

/// Programmstart zur Zeit `jetzt`: räumt alte Sicherungen auf und meldet
/// die Sicherung für die Startkarte: (Sicherung, gespeicherte Datei oder
/// `None` bei Unbenannt). `None`: keine Karte.
fn beim_start(ordner: &Path, jetzt: SystemTime) -> Option<(PathBuf, Option<PathBuf>)> {
    crate::autosave::start(ordner, jetzt).map(|f| (f.backup, f.original))
}

/// „Wiederherstellen“ auf der Startkarte.
fn wiederherstellen(sicherung: &Path, original: Option<&Path>) -> (Scene, Document) {
    crate::autosave::restore(sicherung, original).expect("Sicherung öffnet")
}

/// Titel der Startkarte.
fn karten_titel(original: Option<&Path>) -> String {
    crate::autosave::card_title(original)
}

/// Akzentzeile der linken Kachel: Sicherung ist `min` Minuten neuer.
fn abstand(min: u64) -> String {
    crate::autosave::age_text(min)
}

/// Zeitangabe einer Kachel; Zeiten als (Jahr, Monat, Tag, Stunde, Minute)
/// wie `sk_platform::local_date_time`.
fn zeit(wann: (u16, u8, u8, u8, u8), jetzt: (u16, u8, u8, u8, u8)) -> String {
    crate::autosave::when_text(wann, jetzt)
}

// ===== Hilfen =====

/// Prüfhaus als `projekt/haus.szo` im Testordner, geöffnet wie „Datei →
/// Öffnen“, und ein leerer Ordner „Sicherungen“: Szene, Dokument, Pfad,
/// Sicherungsordner.
fn geoeffnet(name: &str) -> (Scene, Document, PathBuf, PathBuf) {
    let mut s = Scene::with_model(Model::with_seed(140));
    haus_b11(&mut s);
    let dir = test_dir(name);
    let projekt = dir.join("projekt");
    let ordner = dir.join("Sicherungen");
    std::fs::create_dir_all(&projekt).unwrap();
    std::fs::create_dir_all(&ordner).unwrap();
    let p = projekt.join("haus.szo");
    crate::document::save(s.model(), &p).unwrap();
    let l = crate::document::load(&p).expect("öffnet");
    let s = Scene::with_model(l.model);
    let doc = Document::opened(p.clone(), s.model().revision());
    (s, doc, p, ordner)
}

/// Eine Änderung wie von Hand: Innenwand bei x = `x`.
fn aendern(s: &mut Scene, x: f64) {
    let c = cam3d();
    let set = s.model().defaults().interior_wall;
    let mut t = tool(s);
    t.set_category(Category::InteriorWall, s.model().wall_layers(set));
    click(&mut t, &c, vec3(x, 0.0, 0.0));
    click(&mut t, &c, vec3(x, 8000.0, 0.0));
    let w = t.handle(&key(Key::Enter), &c, W, H, 1.0).commit.unwrap();
    s.add_wall_as(&w, Category::InteriorWall).unwrap();
}

fn text(s: &Scene) -> String {
    sk_model::szo::write(s.model())
}

fn inhalt(p: &Path) -> String {
    sk_model::szo::write(&crate::document::load(p).expect("öffnet").model)
}

/// .szo-Dateien in `dir` (nur Namen, sortiert).
fn dateien(dir: &Path) -> Vec<String> {
    let mut v: Vec<String> = std::fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().to_string())
        .filter(|n| n.ends_with(".szo"))
        .collect();
    v.sort();
    v
}

/// Alle Einträge in `dir` (nur Namen, sortiert).
fn alles(dir: &Path) -> Vec<String> {
    let mut v: Vec<String> = std::fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().to_string())
        .collect();
    v.sort();
    v
}

/// Name wie „haus 2026-10-07 03-41.szo“ zum Stamm „haus“.
fn name_passt(n: &str, stamm: &str) -> bool {
    let Some(rest) = n.strip_prefix(stamm).and_then(|r| r.strip_prefix(' ')) else {
        return false;
    };
    let Some(z) = rest.strip_suffix(".szo") else {
        return false;
    };
    let b = z.as_bytes();
    b.len() == 16
        && b.iter().enumerate().all(|(i, c)| match i {
            4 | 7 => *c == b'-',
            10 => *c == b' ',
            13 => *c == b'-',
            _ => c.is_ascii_digit(),
        })
}

fn alter_setzen(p: &Path, jetzt: SystemTime, tage: u64) {
    let f = std::fs::File::options().write(true).open(p).unwrap();
    f.set_modified(jetzt - Duration::from_secs(tage * TAG))
        .unwrap();
}

// ===== Tests =====

/// A140 (F-13, Takt): Ohne Änderung wird nie gesichert, auch nicht nach
/// 5, 10 oder 60 Minuten. Nach einer Änderung (0:10) nicht vor 5:00, bei
/// 5:00 genau einmal. Neue Änderung bei 5:30: nicht bei 9:59, aber bei
/// 10:00 (5 Minuten nach der letzten Sicherung); ohne weitere Änderung bei
/// 15:00 nicht wieder. Nach Strg+S ohne neue Änderung: nichts mehr.
#[test]
fn a140_sichern_nur_bei_aenderung_alle_5_minuten() {
    let (s, doc, _, ordner) = geoeffnet("a140");
    let mut a = sicherer(&ordner);
    for t in [0, 5 * MIN, 10 * MIN, 60 * MIN] {
        assert_eq!(takt(&mut a, &s, &doc, t), None, "unverändert, {t} ms");
    }
    assert!(dateien(&ordner).is_empty(), "nichts geschrieben");

    let (mut s, mut doc, p, ordner) = geoeffnet("a140b");
    let mut a = sicherer(&ordner);
    assert_eq!(takt(&mut a, &s, &doc, 0), None);
    aendern(&mut s, 2500.0); // 0:10
    assert!(doc.is_dirty(s.model()));
    assert_eq!(takt(&mut a, &s, &doc, MIN), None, "1:00");
    assert_eq!(takt(&mut a, &s, &doc, 5 * MIN - 1), None, "4:59,999");
    assert!(takt(&mut a, &s, &doc, 5 * MIN).is_some(), "5:00");
    assert_eq!(takt(&mut a, &s, &doc, 5 * MIN + 1000), None, "nur einmal");
    aendern(&mut s, 7500.0); // 5:30
    assert_eq!(
        takt(&mut a, &s, &doc, 10 * MIN - 1),
        None,
        "noch keine 5 Minuten seit der letzten Sicherung"
    );
    assert!(takt(&mut a, &s, &doc, 10 * MIN).is_some(), "10:00");
    assert_eq!(takt(&mut a, &s, &doc, 15 * MIN), None, "nichts Neues");

    // Strg+S: danach ohne Änderung nichts
    crate::document::save(s.model(), &p).unwrap();
    doc.mark_saved(p.clone(), s.model().revision());
    gespeichert(&mut a, &doc, 16 * MIN);
    for t in [21 * MIN, 30 * MIN] {
        assert_eq!(takt(&mut a, &s, &doc, t), None, "gespeichert, {t} ms");
    }
}

/// A141 (F-13, Ort): Die Sicherung liegt im Ordner „Sicherungen“ und heißt
/// „haus JJJJ-MM-TT HH-MM.szo“. Je Projekt bleibt nur die jüngste. Sie
/// lässt sich öffnen und enthält den aktuellen Stand. Die .szo bleibt
/// bytegleich, im Projektordner entsteht nichts, der Titel behält das •.
/// Ein unbenanntes Projekt wird als „Unbenannt …“ gesichert.
#[test]
fn a141_sicherung_im_ordner_sicherungen() {
    let (mut s, doc, p, ordner) = geoeffnet("a141");
    let vorher = std::fs::read(&p).unwrap();
    let projekt = p.parent().unwrap().to_path_buf();
    let mut a = sicherer(&ordner);
    takt(&mut a, &s, &doc, 0);
    aendern(&mut s, 2500.0);
    let b = takt(&mut a, &s, &doc, 5 * MIN).expect("gesichert");
    assert_eq!(b.parent(), Some(ordner.as_path()), "im Ordner Sicherungen");
    let n = b.file_name().unwrap().to_string_lossy().to_string();
    assert!(name_passt(&n, "haus"), "Name {n}");
    assert_eq!(inhalt(&b), text(&s), "aktueller Stand");

    aendern(&mut s, 7500.0);
    let b2 = takt(&mut a, &s, &doc, 10 * MIN).expect("wieder gesichert");
    assert_eq!(inhalt(&b2), text(&s), "neuer Stand");
    let liste = dateien(&ordner);
    assert_eq!(liste.len(), 1, "nur die jüngste: {liste:?}");
    assert!(name_passt(&liste[0], "haus"));

    assert_eq!(std::fs::read(&p).unwrap(), vorher, ".szo bytegleich");
    assert_eq!(
        alles(&projekt),
        vec!["haus.szo".to_string()],
        "Projektordner"
    );
    assert_eq!(doc.caption(s.model()), "haus.szo •", "Titel behält das •");

    // Unbenannt
    let mut s = Scene::with_model(Model::with_seed(141));
    let doc = Document::new(s.model().revision());
    let ordner = test_dir("a141u").join("Sicherungen");
    std::fs::create_dir_all(&ordner).unwrap();
    let mut a = sicherer(&ordner);
    takt(&mut a, &s, &doc, 0);
    haus_b11(&mut s);
    let b = takt(&mut a, &s, &doc, 5 * MIN).expect("Unbenannt gesichert");
    let n = b.file_name().unwrap().to_string_lossy().to_string();
    assert!(name_passt(&n, "Unbenannt"), "Name {n}");
    assert_eq!(inhalt(&b), text(&s));
}

/// A142 (F-13, Startkarte): Nach normalem Beenden keine Karte, die
/// Sicherung bleibt aber im Ordner. Nach einem Absturz: Karte mit
/// Sicherung und gespeicherter Datei, Titel „Sicherung von haus.szo
/// gefunden“. Strg+S nach der Sicherung, dann Absturz: keine Karte (die
/// Sicherung ist nicht neuer). Unbenannt: Karte ohne gespeicherte Datei,
/// Titel „Sicherung von Unbenannt gefunden“. Sicherungen älter als 7 Tage
/// entfernt der Start still, jüngere bleiben.
#[test]
fn a142_startkarte_nur_nach_absturz() {
    let jetzt = SystemTime::now();

    // Normal beendet
    let (mut s, doc, _, ordner) = geoeffnet("a142");
    assert_eq!(beim_start(&ordner, jetzt), None, "ohne Sicherung");
    let mut a = sicherer(&ordner);
    takt(&mut a, &s, &doc, 0);
    aendern(&mut s, 2500.0);
    takt(&mut a, &s, &doc, 5 * MIN).expect("gesichert");
    beendet(&mut a, &doc);
    drop(a);
    assert_eq!(beim_start(&ordner, jetzt), None, "sauber beendet");
    assert_eq!(dateien(&ordner).len(), 1, "Sicherung bleibt");

    // Absturz
    let (mut s, doc, p, ordner) = geoeffnet("a142b");
    let mut a = sicherer(&ordner);
    takt(&mut a, &s, &doc, 0);
    aendern(&mut s, 2500.0);
    let b = takt(&mut a, &s, &doc, 5 * MIN).expect("gesichert");
    drop(a); // kein Beenden, kein Speichern
    assert_eq!(
        beim_start(&ordner, jetzt),
        Some((b.clone(), Some(p.clone())))
    );
    assert_eq!(karten_titel(Some(&p)), "Sicherung von haus.szo gefunden");
    assert!(b.exists(), "der Start löscht die Sicherung nicht");

    // Strg+S nach der Sicherung, dann Absturz
    let (mut s, mut doc, p, ordner) = geoeffnet("a142c");
    let mut a = sicherer(&ordner);
    takt(&mut a, &s, &doc, 0);
    aendern(&mut s, 2500.0);
    takt(&mut a, &s, &doc, 5 * MIN).expect("gesichert");
    crate::document::save(s.model(), &p).unwrap();
    doc.mark_saved(p.clone(), s.model().revision());
    gespeichert(&mut a, &doc, 6 * MIN);
    drop(a);
    assert_eq!(beim_start(&ordner, jetzt), None, "Datei ist aktuell");

    // Unbenannt, Absturz
    let mut s = Scene::with_model(Model::with_seed(142));
    let doc = Document::new(s.model().revision());
    let ordner = test_dir("a142u").join("Sicherungen");
    std::fs::create_dir_all(&ordner).unwrap();
    let mut a = sicherer(&ordner);
    takt(&mut a, &s, &doc, 0);
    haus_b11(&mut s);
    let b = takt(&mut a, &s, &doc, 5 * MIN).expect("gesichert");
    drop(a);
    assert_eq!(beim_start(&ordner, jetzt), Some((b, None)));
    assert_eq!(karten_titel(None), "Sicherung von Unbenannt gefunden");

    // 7 Tage
    for (tage, bleibt) in [(8, false), (6, true)] {
        let (mut s, doc, _, ordner) = geoeffnet(&format!("a142t{tage}"));
        let mut a = sicherer(&ordner);
        takt(&mut a, &s, &doc, 0);
        aendern(&mut s, 2500.0);
        let b = takt(&mut a, &s, &doc, 5 * MIN).expect("gesichert");
        beendet(&mut a, &doc);
        drop(a);
        alter_setzen(&b, jetzt, tage);
        assert_eq!(beim_start(&ordner, jetzt), None);
        assert_eq!(b.exists(), bleibt, "{tage} Tage alt");
    }
}

/// A143 (F-13, Wiederherstellen/Verwerfen): Wiederherstellen öffnet den
/// Stand der Sicherung mit dem Originalpfad, Titel „haus.szo •“,
/// Rückgängig-Verlauf leer; das nächste Strg+S schreibt die Originaldatei.
/// Unbenannt bleibt unbenannt. Verwerfen: die Sicherung bleibt im Ordner,
/// die .szo unverändert.
#[test]
fn a143_wiederherstellen_und_verwerfen() {
    let jetzt = SystemTime::now();
    let (mut s, doc, p, ordner) = geoeffnet("a143");
    let vorher = std::fs::read(&p).unwrap();
    let mut a = sicherer(&ordner);
    takt(&mut a, &s, &doc, 0);
    aendern(&mut s, 2500.0);
    takt(&mut a, &s, &doc, 5 * MIN).expect("gesichert");
    drop(a);
    let (b, orig) = beim_start(&ordner, jetzt).expect("Karte");

    // Verwerfen: nichts angefasst
    assert!(b.exists(), "Sicherung bleibt im Ordner");
    assert_eq!(std::fs::read(&p).unwrap(), vorher, ".szo unverändert");

    // Wiederherstellen
    let (neu, ndoc) = wiederherstellen(&b, orig.as_deref());
    assert_eq!(text(&neu), text(&s), "Stand der Sicherung");
    assert_eq!(ndoc.path.as_deref(), Some(p.as_path()), "Originalpfad");
    assert_eq!(ndoc.caption(neu.model()), "haus.szo •");
    assert_eq!(neu.undo_label(), None, "Verlauf beginnt leer");
    let ziel = ndoc.path.clone().unwrap();
    crate::document::save(neu.model(), &ziel).unwrap();
    assert_eq!(inhalt(&p), text(&s), "Strg+S schreibt die Originaldatei");

    // Unbenannt
    let mut s = Scene::with_model(Model::with_seed(143));
    let doc = Document::new(s.model().revision());
    let ordner = test_dir("a143u").join("Sicherungen");
    std::fs::create_dir_all(&ordner).unwrap();
    let mut a = sicherer(&ordner);
    takt(&mut a, &s, &doc, 0);
    haus_b11(&mut s);
    takt(&mut a, &s, &doc, 5 * MIN).expect("gesichert");
    drop(a);
    let (b, orig) = beim_start(&ordner, jetzt).expect("Karte");
    let (neu, ndoc) = wiederherstellen(&b, orig.as_deref());
    assert_eq!(text(&neu), text(&s));
    assert_eq!(ndoc.path, None, "bleibt unbenannt");
    assert_eq!(ndoc.caption(neu.model()), "Unbenannt •");
}

/// A144 (F-13, Zeitangaben auf der Startkarte): „heute, HH:MM“,
/// „gestern, HH:MM“ (auch über den Monatswechsel), sonst „TT.MM., HH:MM“.
/// Abstand: „1 Minute neuer“, „N Minuten neuer“, „1 Stunde neuer“,
/// „N Stunden neuer“.
#[test]
fn a144_zeitangaben() {
    let jetzt = (2026, 10, 7, 3, 45);
    assert_eq!(zeit((2026, 10, 7, 3, 41), jetzt), "heute, 03:41");
    assert_eq!(zeit((2026, 10, 6, 23, 5), jetzt), "gestern, 23:05");
    assert_eq!(zeit((2026, 10, 1, 9, 0), jetzt), "01.10., 09:00");
    assert_eq!(
        zeit((2026, 9, 30, 9, 0), (2026, 10, 1, 8, 0)),
        "gestern, 09:00"
    );
    assert_eq!(
        zeit((2025, 12, 31, 18, 30), (2026, 1, 1, 0, 10)),
        "gestern, 18:30"
    );

    assert_eq!(abstand(1), "1 Minute neuer");
    assert_eq!(abstand(4), "4 Minuten neuer");
    assert_eq!(abstand(59), "59 Minuten neuer");
    assert_eq!(abstand(60), "1 Stunde neuer");
    assert_eq!(abstand(180), "3 Stunden neuer");
}
