use super::*;

use sk_model::catalog::{read_szk, write_szk, Library};
use sk_model::{MatCategory, Model};

/// Startbestand mit drei Zukunftsstellen: ein Baustoff „Stampflehm“ mit der
/// erfundenen Kategorie `lehm`, ein unbekannter Schlüssel `sd=12` an der
/// Porenbeton-Zeile und ein unbekannter Satz `[zukunft]`.
fn zukunft() -> (String, [String; 3]) {
    let text = write_szk(&Library::standard());
    let gas = text
        .lines()
        .find(|l| l.starts_with("[material]") && l.contains("name=\"Porenbeton\""))
        .unwrap()
        .to_string();
    let alt_guid = gas
        .split(' ')
        .find_map(|t| t.strip_prefix("guid="))
        .unwrap()
        .to_string();
    let g = Model::with_seed(126).new_guid().to_string();
    let lehm = gas
        .replacen(&alt_guid, &g, 1)
        .replacen("name=\"Porenbeton\"", "name=\"Stampflehm\"", 1)
        .replacen("cat=masonry", "cat=lehm", 1);
    assert!(lehm.contains("cat=lehm"));
    let gas_sd = format!("{gas} sd=12");
    let satz = "[zukunft] guid=0000000000000000000126 wert=\"bleibt\"".to_string();
    let mut zeilen: Vec<String> = text
        .lines()
        .map(|l| {
            if l == gas {
                gas_sd.clone()
            } else {
                l.to_string()
            }
        })
        .collect();
    let pos = zeilen.iter().position(|l| l == &gas_sd).unwrap() + 1;
    zeilen.insert(pos, lehm.clone());
    zeilen.push(satz.clone());
    (zeilen.join("\n") + "\n", [lehm, gas_sd, satz])
}

/// Text des Hinweises in der Statuszeile (ohne Fachwörter). Gezählt wird
/// jede unbekannte Angabe einmal: ein unbekannter Wert (cat=lehm), ein
/// unbekannter Schlüssel (sd=12), ein unbekannter Satz ([zukunft]).
fn hinweis(n: usize) -> String {
    format!("Firmenkatalog: {n} unbekannte Angaben übersprungen, alles andere ist geladen.")
}

fn alle_da(text: &str, zeilen: &[String; 3], wo: &str) {
    for z in zeilen {
        assert!(
            text.lines().any(|l| l == z),
            "{wo}: Zeile fehlt oder geändert: {z}"
        );
    }
}

/// A126: Die Zukunfts-.szk lädt (kein „nicht lesbar“, kein Rückfall auf den
/// Startbestand). „Stampflehm“ ist da, mit Ersatzkategorie, Rohdichte und λ
/// wie in der Datei. Ein Hinweis je Dateifassung: erster Start genau einer, zweiter Start mit unveränderter Datei keiner, nach einer Änderung der Datei wieder genau einer. Schreiben behält alle drei
/// Zukunftszeilen bytegleich, auch nach einer Änderung an der Bibliothek;
/// schreiben → lesen → schreiben ist stabil. Company::load lässt die Datei
/// bytegleich (Startbestand vollständig, nichts zu ergänzen).
#[test]
fn a126_szk_mit_unbekannten_schluesselwoertern() {
    let (text, zeilen) = zukunft();
    let lib = read_szk(&text).unwrap_or_else(|e| panic!("lesbar: {e}"));
    assert_eq!(lib.types.len(), 7, "nichts verworfen");
    let lehm = lib
        .materials
        .iter()
        .find(|(_, x)| x.name == "Stampflehm")
        .expect("Baustoff mit unbekannter Kategorie bleibt")
        .1;
    let gas = lib
        .materials
        .iter()
        .find(|(_, x)| x.name == "Porenbeton")
        .unwrap()
        .1;
    assert!(MatCategory::ALL.contains(&lehm.category), "Ersatzkategorie");
    assert_eq!((lehm.density, lehm.lambda), (gas.density, gas.lambda));

    // Zurückschreiben, unverändert und nach einer Änderung
    let neu = write_szk(&lib);
    alle_da(&neu, &zeilen, "schreiben");
    assert_eq!(write_szk(&read_szk(&neu).unwrap()), neu, "stabil");
    let mut geaendert = lib.clone();
    let id = geaendert
        .types
        .iter()
        .find(|(_, t)| t.code == "AW-36")
        .map(|(id, _)| id)
        .unwrap();
    geaendert.types.get_mut(id).unwrap().name = "AW 36 Büro".into();
    let neu2 = write_szk(&geaendert);
    alle_da(&neu2, &zeilen, "nach Änderung");
    assert!(neu2.contains("name=\"AW 36 Büro\""));

    // Start mit der Datei: ein Hinweis je Dateifassung (Koordinator 19:44)
    let d = test_dir("a126");
    let p = d.join("firmenkatalog.szk");
    std::fs::write(&p, &text).unwrap();
    let start = |soll: Option<usize>, wo: &str, inhalt: &str| {
        let (c, h) = crate::catalog::Company::load(&p, true);
        match soll {
            Some(n) => assert_eq!(h, [hinweis(n)], "{wo}"),
            None => assert!(h.is_empty(), "{wo}: kein Hinweis {h:?}"),
        }
        assert_eq!(c.library().types.len(), 7, "{wo}");
        assert!(
            c.library()
                .materials
                .iter()
                .any(|(_, x)| x.name == "Stampflehm"),
            "{wo}"
        );
        assert_eq!(
            std::fs::read_to_string(&p).unwrap(),
            inhalt,
            "{wo}: Datei bytegleich"
        );
    };
    start(Some(3), "erster Start", &text);
    start(None, "zweiter Start, Datei unverändert", &text);
    // Neue Fassung der Datei: eine weitere unbekannte Angabe
    let text2 = format!("{text}[zukunft] guid=0000000000000000000127 wert=\"neu\"\n");
    std::fs::write(&p, &text2).unwrap();
    start(Some(4), "nach Änderung der Datei", &text2);
    start(None, "danach wieder still", &text2);
}
