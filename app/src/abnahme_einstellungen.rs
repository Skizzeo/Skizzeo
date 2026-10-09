//! Abnahme: eine einstellungen.txt aus einer neueren Fassung (unbekannte
//! Abschnitte, Farbrollen, Größen und Schlüssel) übersteht Laden, Ändern
//! und Speichern der heutigen App. Alles Fremde bleibt im Wortlaut und in
//! seiner Reihenfolge, nichts verdoppelt sich.

use crate::settings::Settings;
use sk_paint::Rgba;
use std::path::{Path, PathBuf};

/// Ganze Zeilen, die die heutige App nicht kennt, in Dateireihenfolge.
const FREMDE_ZEILEN: [&str; 6] = [
    "[color] role=ui.zukunftsrolle value=123456",
    "[size] key=zukunft_breite value=42",
    "[zukunft] modus=breit stufe=3",
    "[env] nebel_dichte=0.4",
    "[druck] papier=A3 rand=\"12 mm\"",
    "[ki] vorschlaege=1",
];

/// Unbekannte Schlüssel in bekannten Zeilen: (Abschnitt, Schlüssel=Wert).
const FREMDE_SCHLUESSEL: [(&str, &str); 4] = [
    ("[theme]", "kontrast=hoch"),
    ("[zuletzt]", "angeheftet=1"),
    ("[planung]", "telefon=\"0421 123\""),
    ("[lvblatt]", "logo=1"),
];

const NEUERE_DATEI: &str = "SKIZZEO-EINSTELLUNGEN 1\n\
[theme] base=\"Dunkel\" kontrast=hoch\n\
[color] role=ui.border value=2878dc\n\
[color] role=ui.zukunftsrolle value=123456\n\
[size] key=font value=15\n\
[size] key=zukunft_breite value=42\n\
[zukunft] modus=breit stufe=3\n\
[env] ground_opacity=0.3\n\
[env] nebel_dichte=0.4\n\
[zuletzt] datei=\"C:\\\\Haus A.szo\" angeheftet=1\n\
[zuletzt] datei=\"C:\\\\Haus B.szo\"\n\
[druck] papier=A3 rand=\"12 mm\"\n\
[planung] name=\"Alt\" anschrift=\"Weg 1\" telefon=\"0421 123\"\n\
[lvblatt] titelblatt=1 verzeichnis=0 logo=1\n\
[ki] vorschlaege=1\n";

fn ordner() -> PathBuf {
    let d = std::env::temp_dir().join(format!("skizzeo-abnahme-einst-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    d
}

fn app(d: &Path) -> Settings {
    let args = ["skizzeo.exe".to_string()].into_iter();
    Settings::new(args, Some(d.to_path_buf()))
}

/// Prüft eine gespeicherte Datei: jede fremde Zeile genau einmal und in
/// der Reihenfolge, jeder fremde Schlüssel genau einmal in seinem
/// Abschnitt, keine Zeile doppelt.
fn fremdes_erhalten(text: &str, wann: &str) {
    let zeilen: Vec<&str> = text.lines().collect();
    let mut lage = Vec::new();
    for z in FREMDE_ZEILEN {
        let n = zeilen.iter().filter(|l| **l == z).count();
        assert_eq!(n, 1, "{wann}: „{z}“ {n}-mal\n{text}");
        lage.push(zeilen.iter().position(|l| *l == z).unwrap());
    }
    assert!(
        lage.windows(2).all(|w| w[0] < w[1]),
        "{wann}: Reihenfolge der fremden Zeilen {lage:?}\n{text}"
    );
    for (abschnitt, kv) in FREMDE_SCHLUESSEL {
        let treffer: Vec<&&str> = zeilen.iter().filter(|l| l.contains(kv)).collect();
        assert_eq!(
            treffer.len(),
            1,
            "{wann}: „{kv}“ {}-mal\n{text}",
            treffer.len()
        );
        assert!(
            treffer[0].starts_with(abschnitt),
            "{wann}: „{kv}“ nicht in {abschnitt}: {}",
            treffer[0]
        );
    }
    let a = zeilen.iter().find(|l| l.contains("angeheftet=1")).unwrap();
    assert!(
        a.contains("Haus A.szo"),
        "{wann}: angeheftet an falscher Datei: {a}"
    );
    for (i, l) in zeilen.iter().enumerate() {
        assert!(
            !zeilen[i + 1..].contains(l),
            "{wann}: Zeile doppelt: {l}\n{text}"
        );
    }
}

#[test]
fn abnahme_einstellungen_neuere_fassung_bleibt_erhalten() {
    let d = ordner();
    let path = d.join("Skizzeo").join("einstellungen.txt");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, NEUERE_DATEI).unwrap();

    // Erster Start: laden, mehrere Einstellungen ändern, speichern
    let mut s = app(&d);
    let mut t = s.load();
    assert_eq!(t.size.font, 15.0);
    let akzent = Rgba::rgb(10, 120, 200);
    t.set_accent(akzent);
    s.recent.push(PathBuf::from("C:\\Haus C.szo"));
    s.set_lv_blatt((true, true));
    s.set_planung("Dipl.-Ing. (FH) Jörn Horstmann", "Denkmalsweg 18b");
    s.save_if_changed(&t).unwrap();
    let erst = std::fs::read_to_string(&path).unwrap();
    fremdes_erhalten(&erst, "nach dem ersten Speichern");

    // Zweiter Start: die Änderungen gelten, eine weitere Änderung, speichern
    let mut s = app(&d);
    let t = s.load();
    assert_eq!(t.ui.accent, akzent, "{erst}");
    assert_eq!(t.size.font, 15.0);
    assert_eq!(s.recent.paths()[0], PathBuf::from("C:\\Haus C.szo"));
    assert_eq!(s.recent.paths().len(), 3);
    assert_eq!(s.lv_blatt(), (true, true));
    assert_eq!(
        s.planung().map(|p| p.0),
        Some("Dipl.-Ing. (FH) Jörn Horstmann".to_string())
    );
    s.set_lv_blatt((false, true));
    s.save_if_changed(&t).unwrap();
    let zweit = std::fs::read_to_string(&path).unwrap();
    fremdes_erhalten(&zweit, "nach dem zweiten Speichern");

    // Dritter Start ohne Änderung: die Datei bleibt bytegleich
    let mut s = app(&d);
    let t = s.load();
    assert_eq!(s.lv_blatt(), (false, true));
    s.save_if_changed(&t).unwrap();
    assert_eq!(std::fs::read_to_string(&path).unwrap(), zweit);

    // Noch ein Rundlauf mit Änderung und Rücknahme: kein Wachstum
    let mut s = app(&d);
    let t = s.load();
    s.set_lv_blatt((true, true));
    s.save_if_changed(&t).unwrap();
    let mut s = app(&d);
    let t = s.load();
    s.set_lv_blatt((false, true));
    s.save_if_changed(&t).unwrap();
    let viert = std::fs::read_to_string(&path).unwrap();
    fremdes_erhalten(&viert, "nach dem vierten Speichern");
    assert_eq!(viert.lines().count(), zweit.lines().count(), "{viert}");
    let _ = std::fs::remove_dir_all(&d);
}
