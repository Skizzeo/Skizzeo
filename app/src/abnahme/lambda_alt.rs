use super::*;
// Abnahmetests K5-Nachtrag „λ in alten Dateien ergänzen“ (BIM, Koordinator
// 19:06) und K4 „gelöschte Werkstypen kommen nicht wieder“ ([stock] set=…).
// Paket: bim/paket-k4-k5-wandtypen.md, Nachtrag zu K5 („Fertig, wenn“).
// Spezifikation: test/abnahme-wandtypen.md (A122, A123).
//
// Einbau: als `mod lambda_alt { use super::*; … }` ans Ende von
// app/src/abnahme.rs. Nutzt aus abnahme.rs: test_dir und die Prüfhaus-Datei
// abnahme_haus_v3.szo (gespeichert mit 1f0763e, Version 3, alle λ = „-“).
// Keine angenommenen Namen: alles läuft über document::load, Document,
// Model::u_value, catalog::{read_szk, write_szk} und Company::load.

use sk_model::catalog::{read_szk, write_szk};
use sk_model::{LayerSetId, Model};

const HAUS_V3: &str = include_str!("../abnahme_haus_v3.szo");

// ===== Hilfen =====

/// Öffnet `text` als Datei wie „Datei → Öffnen“ (main.rs open_path):
/// laden, Szene, Dokument mit dem Stand nach dem Laden.
fn oeffnen(name: &str, text: &str) -> (Scene, crate::document::Document, Vec<String>) {
    let d = test_dir(name);
    let p = d.join("haus.szo");
    std::fs::write(&p, text).unwrap();
    let l = crate::document::load(&p).expect("öffnet");
    let s = Scene::with_model(l.model);
    let doc = crate::document::Document::opened(p, s.model().revision());
    (s, doc, l.hints)
}

fn lambda(m: &Model, name: &str) -> Option<f64> {
    m.materials()
        .iter()
        .find(|(_, x)| x.name == name)
        .unwrap_or_else(|| panic!("Baustoff {name} fehlt"))
        .1
        .lambda
}

fn typ(m: &Model, code: &str) -> LayerSetId {
    m.layer_sets()
        .iter()
        .find(|(_, t)| t.code == code)
        .map(|(id, _)| id)
        .unwrap_or_else(|| panic!("Typ {code} fehlt"))
}

/// U-Wert auf zwei Stellen, wie im Katalog angezeigt.
fn u2(m: &Model, code: &str) -> Option<f64> {
    m.u_value(typ(m, code)).map(|u| (u * 100.0).round() / 100.0)
}

/// Ersetzt in der Baustoffzeile `name="…"` das λ-Feld durch `neu`
/// (bzw. benennt um, wenn `neuer_name` gesetzt ist).
fn baustoff_zeile(
    text: &str,
    name: &str,
    lambda: Option<&str>,
    neuer_name: Option<&str>,
) -> String {
    let key = format!("name=\"{name}\"");
    let mut n = 0;
    let out: Vec<String> = text
        .lines()
        .map(|l| {
            if !(l.starts_with("[material]") && l.contains(&key)) {
                return l.to_string();
            }
            n += 1;
            let mut l = l.to_string();
            if let Some(v) = lambda {
                l = l
                    .split(' ')
                    .map(|t| {
                        if t.starts_with("lambda=") {
                            format!("lambda={v}")
                        } else {
                            t.to_string()
                        }
                    })
                    .collect::<Vec<_>>()
                    .join(" ");
            }
            if let Some(nn) = neuer_name {
                l = l.replacen(&key, &format!("name=\"{nn}\""), 1);
            }
            l
        })
        .collect();
    assert_eq!(n, 1, "Baustoff {name} genau einmal in der Datei");
    out.join("\n") + "\n"
}

/// Alle λ-Felder der Baustoffzeilen auf „-“.
fn ohne_lambda(text: &str) -> String {
    text.lines()
        .map(|l| {
            if !l.starts_with("[material]") {
                return l.to_string();
            }
            l.split(' ')
                .map(|t| {
                    if t.starts_with("lambda=") {
                        "lambda=-".to_string()
                    } else {
                        t.to_string()
                    }
                })
                .collect::<Vec<_>>()
                .join(" ")
        })
        .collect::<Vec<_>>()
        .join("\n")
        + "\n"
}

// ===== Tests =====

/// A122 (K5-Nachtrag „λ in alten Dateien“): Das Prüfhaus v3 (alle λ = „-“,
/// zeitbasierte Baustoff-Guids) öffnet mit Gasbeton 0,09, Dämmung (WDVS)
/// 0,035, Stahlbeton 2,3, Putz 0,87 (Treffer über Name und Kategorie). U von
/// AW-31,5 = 0,16. Die Datei gilt als unverändert: kein •, kein
/// Rückgängig-Schritt, kein Hinweis. Guids bleiben. Zweimal öffnen ergibt
/// dasselbe Modell (Regel 13), beim Speichern steht λ in der Datei.
/// Ein vorhandenes λ 0,12 bleibt 0,12; ein umbenannter „Gasbeton (alt)“
/// bleibt ohne λ. Treffer über die Guid: ein Werksbaustoff mit fremdem Namen
/// bekommt sein λ trotzdem.
#[test]
fn a122_lambda_in_alten_dateien() {
    assert_eq!(HAUS_V3.matches("lambda=-").count(), 4, "Prüfhaus ohne λ");
    let (s, doc, hints) = oeffnen("a122", HAUS_V3);
    let m = s.model();
    for (n, l) in [
        ("Gasbeton", 0.09),
        ("Dämmung (WDVS)", 0.035),
        ("Stahlbeton", 2.3),
        ("Putz", 0.87),
    ] {
        assert_eq!(lambda(m, n), Some(l), "λ {n}");
    }
    assert_eq!(
        u2(m, "AW-31,5"),
        Some(0.16),
        "0,13 + 0,14/0,035 + 0,175/0,09 + 0,04"
    );
    assert!(!doc.is_dirty(m), "Titel ohne •");
    assert_eq!(s.undo_label(), None, "kein Rückgängig-Schritt");
    assert!(hints.is_empty(), "kein Hinweis: {hints:?}");
    assert!(m.check().is_empty(), "{:?}", m.check());
    // Guids der Baustoffe wie in der Datei (Regel 19)
    for l in HAUS_V3.lines().filter(|l| l.starts_with("[material]")) {
        let g = l.split(' ').find_map(|t| t.strip_prefix("guid=")).unwrap();
        assert!(
            m.materials().iter().any(|(_, x)| x.guid.to_string() == g),
            "Guid {g} unverändert"
        );
    }
    // Immer gleich ergänzt, beim Speichern steht λ in der Datei
    let text = sk_model::szo::write(m);
    let (s2, _, _) = oeffnen("a122b", HAUS_V3);
    assert_eq!(
        sk_model::szo::write(s2.model()),
        text,
        "zweimal öffnen = dasselbe"
    );
    assert!(!text.contains("lambda=-"), "λ gespeichert");
    let (s3, doc3, _) = oeffnen("a122c", &text);
    assert_eq!(sk_model::szo::write(s3.model()), text, "Rundlauf");
    assert!(!doc3.is_dirty(s3.model()));

    // Vorhandenes λ wird nie überschrieben
    let eigen = baustoff_zeile(HAUS_V3, "Gasbeton", Some("0.12"), None);
    let (s, doc, _) = oeffnen("a122d", &eigen);
    assert_eq!(lambda(s.model(), "Gasbeton"), Some(0.12));
    assert_eq!(lambda(s.model(), "Dämmung (WDVS)"), Some(0.035));
    assert_eq!(u2(s.model(), "AW-31,5"), Some(0.18), "mit λ 0,12");
    assert!(!doc.is_dirty(s.model()));

    // Umbenannt: kein Treffer, λ bleibt leer, U zeigt „–“
    let alt = baustoff_zeile(HAUS_V3, "Gasbeton", None, Some("Gasbeton (alt)"));
    let (s, _, _) = oeffnen("a122e", &alt);
    assert_eq!(lambda(s.model(), "Gasbeton (alt)"), None);
    assert_eq!(lambda(s.model(), "Dämmung (WDVS)"), Some(0.035));
    assert_eq!(u2(s.model(), "AW-31,5"), None);

    // Treffer über die Guid: aktuelles Projekt ohne λ, Porenbeton umbenannt
    let werk = Model::new();
    let neu = baustoff_zeile(
        &ohne_lambda(&sk_model::szo::write(&werk)),
        "Porenbeton",
        None,
        Some("Porenbeton Büro"),
    );
    let (s, doc, _) = oeffnen("a122f", &neu);
    for (_, w) in werk.materials().iter() {
        let x = s
            .model()
            .materials()
            .iter()
            .find(|(_, x)| x.guid == w.guid)
            .unwrap()
            .1;
        assert_eq!(x.lambda, w.lambda, "λ über die Guid: {}", w.name);
    }
    assert_eq!(lambda(s.model(), "Porenbeton Büro"), Some(0.09));
    assert!(!doc.is_dirty(s.model()));
}

/// A123 (K4, Firmenkatalog): Ein Werkstyp, den das Büro aus dem Firmenkatalog
/// gelöscht hat, kommt beim nächsten Laden nicht wieder (Vermerk
/// [stock] set=…). Ein vom Büro gewählter Standard (AW-31,5) bleibt. Kein
/// Hinweis.
#[test]
fn a123_geloeschter_werkstyp_kommt_nicht_wieder() {
    let d = test_dir("a123");
    let p = d.join("firmenkatalog.szk");
    let (_, h) = crate::catalog::Company::load(&p, true);
    assert!(h.is_empty(), "{h:?}");
    let text = std::fs::read_to_string(&p).expect("am Vorgabeort angelegt");
    assert_eq!(
        text.matches("[stock]").count(),
        7,
        "alle Werkstypen vermerkt (seit K5 mit AW-36,5)"
    );
    let mut lib = read_szk(&text).unwrap();
    let aw49 = lib
        .types
        .iter()
        .find(|(_, t)| t.code == "AW-49")
        .map(|(id, _)| id)
        .unwrap();
    lib.types.remove(aw49);
    lib.default_exterior = lib
        .types
        .iter()
        .find(|(_, t)| t.code == "AW-31,5")
        .map(|(id, _)| id);
    std::fs::write(&p, write_szk(&lib)).unwrap();
    for runde in 0..2 {
        let (c, h) = crate::catalog::Company::load(&p, true);
        assert!(h.is_empty(), "Runde {runde}: kein Hinweis {h:?}");
        let neu = Model::from_library(c.library());
        let codes: Vec<String> = neu
            .layer_sets()
            .iter()
            .map(|(_, t)| t.code.clone())
            .collect();
        assert!(
            !codes.iter().any(|c| c == "AW-49"),
            "bleibt gelöscht: {codes:?}"
        );
        assert_eq!(codes.len(), 6);
        assert_eq!(
            neu.layer_set(neu.defaults().exterior_wall).unwrap().code,
            "AW-31,5",
            "Standard des Büros bleibt"
        );
    }
}
