use super::*;

use sk_model::catalog::{read_szk, write_szk, Library};
use sk_model::{MatCategory, Model};

const SZK_K2: &str = include_str!("../abnahme_firmenkatalog_k2.szk");

fn laden(p: &std::path::Path) -> (crate::catalog::Company, Vec<String>) {
    crate::catalog::Company::load(p, true)
}

fn baustoff<'a>(lib: &'a Library, name: &str) -> &'a sk_model::Material {
    lib.materials
        .iter()
        .find(|(_, x)| x.name == name)
        .unwrap_or_else(|| panic!("Baustoff {name} fehlt"))
        .1
}

fn typ_code<'a>(lib: &'a Library, code: &str) -> Option<&'a sk_model::LayerSet> {
    lib.types
        .iter()
        .find(|(_, t)| t.code == code)
        .map(|(_, t)| t)
}

/// A124: Der Startbestand (mit Luft) und je ein Baustoff jeder Kategorie aus
/// `MatCategory::ALL` (auch Holz und Luft) plus „Randdämmung“ (Dämmung,
/// λ 0,035): schreiben → lesen ergibt dieselbe Bibliothek, schreiben →
/// lesen → schreiben denselben Text. Über die Datei (Company::load): kein
/// Hinweis, nichts verloren, die Datei bleibt bytegleich.
#[test]
fn a124_szk_rundlauf_alle_baustoffkategorien() {
    let mut m = Model::with_seed(124);
    let mut lib = Library::standard();
    assert!(
        lib.materials
            .iter()
            .any(|(_, x)| x.category == MatCategory::Air),
        "Startbestand mit Luft (AW-49)"
    );
    let vorlage = lib.materials.iter().next().unwrap().1.clone();
    for (i, cat) in MatCategory::ALL.iter().enumerate() {
        let mut x = vorlage.clone();
        x.guid = m.new_guid();
        x.name = format!("Prüfbaustoff {i}");
        x.category = *cat;
        x.lambda = Some(0.5 + i as f64 / 100.0);
        lib.materials.insert(x);
    }
    let mut rd = vorlage.clone();
    rd.guid = m.new_guid();
    rd.name = "Randdämmung".into();
    rd.category = MatCategory::Insulation;
    rd.lambda = Some(0.035);
    lib.materials.insert(rd);

    let text = write_szk(&lib);
    assert!(text.contains("cat=air"), "Luft steht in der Datei");
    let zurueck = read_szk(&text).unwrap_or_else(|e| panic!("lesbar: {e}"));
    assert_eq!(zurueck, lib, "schreiben → lesen");
    assert_eq!(write_szk(&zurueck), text, "schreiben → lesen → schreiben");
    for (i, cat) in MatCategory::ALL.iter().enumerate() {
        let x = baustoff(&zurueck, &format!("Prüfbaustoff {i}"));
        assert_eq!((x.category, x.lambda), (*cat, Some(0.5 + i as f64 / 100.0)));
    }
    let r = baustoff(&zurueck, "Randdämmung");
    assert_eq!(
        (r.category, r.lambda),
        (MatCategory::Insulation, Some(0.035))
    );

    // Über die Datei wie beim Start
    let d = test_dir("a124");
    let p = d.join("firmenkatalog.szk");
    std::fs::write(&p, &text).unwrap();
    let (c, h) = laden(&p);
    assert!(h.is_empty(), "kein Hinweis: {h:?}");
    assert_eq!(c.library(), &lib, "nichts verloren");
    assert_eq!(
        std::fs::read_to_string(&p).unwrap(),
        text,
        "Datei unberührt"
    );
    // Ein zweiter Start liest die Datei ebenso
    let (c2, h2) = laden(&p);
    assert!(h2.is_empty(), "{h2:?}");
    assert_eq!(c2.library(), &lib);
}

/// A125: Eine .szk aus K2/K3 (137fca7/c23fb08, ohne [stock], ohne Luft) liest
/// der neue Stand verlustfrei: alle Guids da, Büroname, Schichten, Standards
/// und das von Hand gesetzte λ 0,12 bleiben; jede Zeile der alten Datei steht
/// unverändert im neu geschriebenen Text. Über die Datei: einmal der Hinweis
/// „Werkstypen ergänzt“, Büroname und λ 0,12 bleiben, fehlendes λ (Gasbeton)
/// wird 0,09, Standard AW-36 (K4). Danach ist die Datei für den neuen Stand
/// wieder lesbar, der zweite Start gibt keinen Hinweis.
#[test]
fn a125_alte_szk_verlustfrei() {
    assert!(SZK_K2.starts_with("SZK 1\n") && !SZK_K2.contains("[stock]"));
    let lib = read_szk(SZK_K2).unwrap_or_else(|e| panic!("alte .szk lesbar: {e}"));
    // Jede Zeile der alten Datei bleibt beim Zurückschreiben erhalten
    let neu = write_szk(&lib);
    for l in SZK_K2.lines().filter(|l| l.starts_with('[')) {
        assert!(neu.lines().any(|n| n == l), "verloren: {l}");
    }
    assert_eq!(read_szk(&neu).unwrap(), lib, "Rundlauf");
    let aw = typ_code(&lib, "AW-31,5").expect("AW-31,5");
    assert_eq!(aw.name, "AW 31,5 Büro");
    assert_eq!(
        aw.layers.iter().map(|l| l.thickness).collect::<Vec<_>>(),
        [140.0, 175.0]
    );
    assert_eq!(baustoff(&lib, "Dämmung (WDVS)").lambda, Some(0.12));
    assert_eq!(
        lib.default_exterior
            .and_then(|id| lib.types.get(id))
            .map(|t| t.code.as_str()),
        Some("AW-31,5")
    );
    assert_eq!(
        lib.default_interior
            .and_then(|id| lib.types.get(id))
            .map(|t| t.code.as_str()),
        Some("IW-17,5")
    );

    // Start mit der alten Datei
    let d = test_dir("a125");
    let p = d.join("firmenkatalog.szk");
    std::fs::write(&p, SZK_K2).unwrap();
    let (c, h) = laden(&p);
    assert!(
        h.iter().any(|x| x.contains("Werkstypen")),
        "Hinweis Werkstypen ergänzt: {h:?}"
    );
    assert!(!h.iter().any(|x| x.contains("nicht lesbar")), "{h:?}");
    let b = c.library();
    assert_eq!(b.types.len(), 7, "sieben Werkstypen seit K5");
    assert_eq!(typ_code(b, "AW-31,5").unwrap().name, "AW 31,5 Büro");
    assert_eq!(
        baustoff(b, "Dämmung (WDVS)").lambda,
        Some(0.12),
        "Hand-λ bleibt"
    );
    assert_eq!(
        baustoff(b, "Gasbeton").lambda,
        Some(0.09),
        "fehlendes λ ergänzt"
    );
    assert_eq!(
        b.default_exterior
            .and_then(|id| b.types.get(id))
            .map(|t| t.code.as_str()),
        Some("AW-36")
    );
    let (c2, h2) = laden(&p);
    assert!(h2.is_empty(), "zweiter Start ohne Hinweis: {h2:?}");
    assert_eq!(c2.library(), c.library());
}
