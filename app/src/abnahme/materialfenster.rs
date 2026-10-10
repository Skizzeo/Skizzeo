use super::*;

// Abnahmetests A230–A240: Materialfenster und Baustoffkennwerte (Paket 5,
// projektstruktur/paket-5-materialfenster.md §4, BIM-Regeln 50–55 in
// bim/paket-5-baustoffkennwerte.md, Koordinator 11:26 und 11:28).
// Spezifikation: test/abnahme-materialfenster.md. A229 (Bildgleichheit
// des Umzugs nach attr_pick.rs) steht in a229-bildgleich-attr-pick.rs.
//
// Einbau: als `mod materialfenster { use super::*; … }` ans Ende von
// app/src/abnahme.rs. Nutzt aus abnahme.rs: gebaeude.
//
// Angenommene Namen stehen nur in den Adaptern (Vorlage §2, §3):
// `Material::props` (PropSet), `Model::set_material_props`,
// `Model::material_uses -> Vec<Use>` (`Use::Type(LayerSetId, usize)`,
// `Use::Element(ElementId)`), `Model::duplicate_material`,
// `Model::can_remove_material`, `crate::material_view::{MaterialView,
// input, price_hint}`, `sk_model::compare_materials`.

use sk_model::{Category, ElementId, Guid, MatCategory, MaterialId, PropValue, RefSide};

// ===== Adapter Paket 5 =====

fn kennwerte(m: &Model, mat: MaterialId) -> sk_model::PropSet {
    m.material(mat).unwrap().props.clone()
}

/// Eingabe im Fenster wie getippt (deutsche Schreibweise, „0,09“). Felder:
/// „λ“, „Rohdichte“, „Richtpreis“, „Preisstand“, „μ“ (auch „µ“), „c“,
/// „Euroklasse“, „Hersteller“, … und eigene Kennwerte unter ihrem Namen.
/// `false`: abgelehnt, nichts geändert.
fn eintippen(m: &mut Model, mat: MaterialId, feld: &str, text: &str) -> bool {
    crate::material_view::input(m, mat, feld, text)
}

/// Eigener Kennwert über „+ Kennwert“. `false`, wenn der Name ein fester
/// Schlüssel ist.
fn eigener(m: &mut Model, mat: MaterialId, name: &str, text: &str) -> bool {
    crate::material_view::add_custom(m, mat, name, text)
}

/// Gespeicherte Preiseinheit des Richtpreises („m3“, „m2“, „m“, „t“).
fn preiseinheit(m: &Model, mat: MaterialId) -> Option<String> {
    wort(&kennwerte(m, mat), "Preiseinheit")
}

/// Hinweis im Fenster, wenn die Einheit nicht mehr zur Art passt.
fn preis_hinweis(m: &Model, mat: MaterialId) -> Option<String> {
    crate::material_view::price_hint(m, mat)
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Nutzung {
    /// Typ (Kürzel) mit Zahl der Bauteile dieses Typs.
    Typ(String, usize),
    /// Bauteil ohne Typ (Nummer).
    Bauteil(String),
}

fn verwendet(m: &Model, mat: MaterialId) -> Vec<Nutzung> {
    let mut v: Vec<Nutzung> = m
        .material_uses(mat)
        .into_iter()
        .map(|u| match u {
            sk_model::Use::Type(t, n) => Nutzung::Typ(m.layer_set(t).unwrap().code.clone(), n),
            sk_model::Use::Element(e) => Nutzung::Bauteil(m.element(e).unwrap().number.clone()),
        })
        .collect();
    v.sort();
    v
}

/// Baustoffart wechseln (Feld „Art“ unter „Mehr“).
fn art_wechseln(m: &mut Model, mat: MaterialId, c: MatCategory) -> bool {
    crate::material_view::input(m, mat, "Art", c.name())
}

fn duplizieren(m: &mut Model, mat: MaterialId) -> MaterialId {
    m.duplicate_material(mat).expect("dupliziert")
}

fn loeschbar(m: &Model, mat: MaterialId) -> bool {
    m.can_remove_material(mat)
}

/// Fenster öffnen (Kopie des Modells), ändern, OK bzw. Abbrechen.
fn fenster_ok(s: &mut Scene, f: impl FnOnce(&mut Model)) {
    let mut w = crate::material_view::MaterialView::open(s);
    f(w.model_mut());
    w.ok(s);
}

fn fenster_abbrechen(s: &mut Scene, f: impl FnOnce(&mut Model)) {
    let mut w = crate::material_view::MaterialView::open(s);
    f(w.model_mut());
    w.cancel(s);
}

/// Abgleich Projekt ↔ Firmenkatalog je Baustoff-Guid.
fn abgleich(m: &Model, lib: &sk_model::Library) -> Vec<(Guid, sk_model::TypeState)> {
    sk_model::compare_materials(m, lib)
}

/// Baustoff aus dem Projekt in den Firmenkatalog bzw. zurück.
fn in_firma(m: &Model, lib: &mut sk_model::Library, mat: MaterialId) {
    sk_model::export_material(m, lib, mat);
}

fn ins_projekt(m: &mut Model, lib: &sk_model::Library, g: Guid) -> bool {
    sk_model::import_material(m, lib, g)
}

// ===== Hilfen =====

fn nr(s: &Scene, nummer: &str) -> ElementId {
    s.model()
        .elements()
        .iter()
        .find(|(_, e)| e.number == nummer)
        .map(|(id, _)| id)
        .unwrap_or_else(|| panic!("{nummer} fehlt"))
}

fn stoff(m: &Model, name: &str) -> MaterialId {
    m.materials()
        .iter()
        .find(|(_, x)| x.name == name)
        .map(|(id, _)| id)
        .unwrap_or_else(|| panic!("{name} fehlt"))
}

fn zahl(p: &sk_model::PropSet, k: &str) -> Option<f64> {
    match p.get(k) {
        Some(PropValue::Number(n)) => Some(*n),
        _ => None,
    }
}

fn wort(p: &sk_model::PropSet, k: &str) -> Option<String> {
    match p.get(k) {
        Some(PropValue::Text(t)) => Some(t.clone()),
        _ => None,
    }
}

fn csv(s: &mut Scene) -> Vec<u8> {
    let l = s.schedule().clone();
    crate::schedule_view::csv(s.model(), &l)
}

fn lesen(t: &str) -> sk_model::szo::Loaded {
    sk_model::szo::read(t, sk_model::GuidGen::with_seed(1)).expect("öffnet")
}

fn matprop_zeilen(t: &str) -> Vec<&str> {
    t.lines().filter(|l| l.starts_with("[matprop]")).collect()
}

/// Prüfhaus wie Paket 4: 10 × 8 m, AW-31,5, IW-001 (IW-17,5) im EG,
/// OG-Nord −1,50 (DT-001, AB-001), OG-Ost +0,60 (UD-001).
fn pruefhaus(seed: u64) -> Scene {
    let mut s = Scene::with_model(Model::with_seed(seed));
    let (eg, _) = gebaeude(&mut s);
    assert!(s.edit_model("Innenwand", |m| {
        let st = m.run(eg).unwrap().storey;
        let t = m.default_type(sk_model::TypeCategory::InteriorWall);
        let iw = [vec3(5000.0, 0.0, 0.0), vec3(5000.0, 8000.0, 0.0)];
        m.add_wall_run(&iw, false, RefSide::Center, st, t, Category::InteriorWall)
            .is_some()
    }));
    for (w, d) in [("AW-006", -1500.0), ("AW-007", 600.0)] {
        let w = nr(&s, w);
        assert!(s.edit_model("Kopplung gelöst", |m| m.set_linked(w, false)));
        assert!(s.edit_model("Wand verschoben", |m| m.move_segment(w, d).is_some()));
    }
    assert!(s.model().check().is_empty(), "{:?}", s.model().check());
    s
}

/// Porenbeton mit Richtpreis 185 €/m³, Stand 10/2026, μ 5, Hersteller und
/// eigenem Kennwert, über das Fenster gesetzt.
fn mit_kennwerten(s: &mut Scene) -> MaterialId {
    let g = stoff(s.model(), "Porenbeton");
    fenster_ok(s, |m| {
        assert!(eintippen(m, g, "Richtpreis", "185"));
        assert!(eintippen(m, g, "Preisstand", "10/2026"));
        assert!(eintippen(m, g, "μ", "5"));
        assert!(eintippen(m, g, "Hersteller", "Muster GmbH"));
        assert!(eigener(m, g, "Druckfestigkeit [N/mm²]", "4"));
    });
    g
}

/// A230 (§2.3, Regel 54): `[matprop]` im Rundlauf. Zahl (`num=`, beim
/// Richtpreis mit `unit=m3`), Text (`value=`) und eigener Kennwert stehen
/// je als eine Zeile; `.szo` bleibt 4; Rundlauf bytegleich; nach dem
/// Öffnen sind die Werte da. Ohne Werte keine Zeile, die Datei bleibt
/// bytegleich wie vor Paket 5. Eine Zeile auf einen unbekannten Baustoff
/// wird mit Hinweis verworfen.
#[test]
fn a230_datei_matprop() {
    let mut s = pruefhaus(230);
    let ohne = sk_model::szo::write(s.model());
    assert!(matprop_zeilen(&ohne).is_empty(), "ohne Werte keine Zeile");
    let g = mit_kennwerten(&mut s);
    let gg = s.model().material(g).unwrap().guid;
    let text = sk_model::szo::write(s.model());
    assert!(text.starts_with("SZO 4\n"));
    let z = matprop_zeilen(&text);
    assert!(z.iter().all(|l| l.contains(&format!("mat={gg}"))), "{z:?}");
    assert!(
        z.iter().any(|l| l.contains("key=\"Richtpreis\"")
            && l.contains("num=185")
            && l.contains("unit=m3")),
        "{z:?}"
    );
    assert!(z
        .iter()
        .any(|l| l.contains("key=\"Preisstand\"") && l.contains("value=\"10/2026\"")));
    assert!(z
        .iter()
        .any(|l| l.contains("key=\"μ\"") && l.contains("num=5")));
    assert!(z
        .iter()
        .any(|l| l.contains("key=\"Druckfestigkeit [N/mm²]\"") && l.contains("num=4")));
    let l = lesen(&text);
    assert!(l.hints.is_empty(), "{:?}", l.hints);
    assert_eq!(sk_model::szo::write(&l.model), text, "Rundlauf");
    let p = kennwerte(&l.model, stoff(&l.model, "Porenbeton"));
    assert_eq!(zahl(&p, "Richtpreis"), Some(185.0));
    assert_eq!(wort(&p, "Hersteller").as_deref(), Some("Muster GmbH"));
    assert_eq!(zahl(&p, "Druckfestigkeit [N/mm²]"), Some(4.0));

    let fremd = format!(
            "{ohne}[matprop] mat=00000000-0000-0000-0000-00000000abcd key=\"Richtpreis\" num=9 unit=m3\n"
        );
    let l = lesen(&fremd);
    assert_eq!(l.hints.len(), 1, "{:?}", l.hints);
    assert_eq!(sk_model::szo::write(&l.model), ohne, "verworfen");
}

/// A231 (§2.3, §5, Regel 54, plan B7): Rücknahme von Paket 5. Eine Datei
/// mit `[matprop]`-Zeilen öffnet auf dem Stand vor Paket 5 (mit F-17b),
/// Speichern behält die Zeilen bytegleich. Läuft ohne Adapter.
#[test]
fn a231_matprop_ruecknahme() {
    let mut s = Scene::with_model(Model::with_seed(231));
    gebaeude(&mut s);
    let text = sk_model::szo::write(s.model());
    let g = s.model().materials().iter().next().unwrap().1.guid;
    let zeilen = format!(
        "[matprop] mat={g} key=\"Richtpreis\" num=185 unit=m3\n\
             [matprop] mat={g} key=\"Preisstand\" value=\"10/2026\"\n\
             [matprop] mat={g} key=\"Druckfestigkeit [N/mm²]\" num=4\n"
    );
    let l = lesen(&format!("{text}{zeilen}"));
    assert!(l.model.check().is_empty());
    let neu = sk_model::szo::write(&l.model);
    let ist: Vec<&str> = matprop_zeilen(&neu);
    assert_eq!(ist, zeilen.lines().collect::<Vec<_>>());
}

/// A232 (§1.3, §2.3, §4.3): `.szk`. Kennwerte gehen mit „In den
/// Firmenkatalog“ hin und mit „Ins Projekt übernehmen“ zurück, auch über
/// Schreiben und Lesen des Katalogs (Rundlauf bytegleich). Der Abgleich
/// meldet „gleich“ und, wenn sich nur der Richtpreis unterscheidet,
/// „abweichend“. Ohne Kennwerte keine `[matprop]`-Zeile im Katalog.
#[test]
fn a232_firmenkatalog_kennwerte() {
    let mut s = pruefhaus(232);
    let g = mit_kennwerten(&mut s);
    let gg = s.model().material(g).unwrap().guid;
    let mut lib = sk_model::Library::default();
    assert!(matprop_zeilen(&sk_model::write_szk(&lib)).is_empty());
    in_firma(s.model(), &mut lib, g);
    let text = sk_model::write_szk(&lib);
    assert!(
        matprop_zeilen(&text).iter().any(|l| l.contains("num=185")),
        "{text}"
    );
    let lib = sk_model::read_szk(&text).expect("liest");
    assert_eq!(sk_model::write_szk(&lib), text, "Rundlauf .szk");
    let stand = |m: &Model, lib: &sk_model::Library| {
        abgleich(m, lib)
            .into_iter()
            .find(|x| x.0 == gg)
            .map(|x| x.1)
    };
    assert_eq!(stand(s.model(), &lib), Some(sk_model::TypeState::Same));
    fenster_ok(&mut s, |m| assert!(eintippen(m, g, "Richtpreis", "190")));
    assert_eq!(
        stand(s.model(), &lib),
        Some(sk_model::TypeState::Differs),
        "nur Preis"
    );

    let mut neu = Scene::with_model(Model::with_seed(2320));
    gebaeude(&mut neu);
    assert!(neu.edit_model("Übernommen", |m| ins_projekt(m, &lib, gg)));
    let id = neu
        .model()
        .materials()
        .iter()
        .find(|(_, x)| x.guid == gg)
        .map(|(id, _)| id)
        .expect("übernommen");
    let p = kennwerte(neu.model(), id);
    assert_eq!(zahl(&p, "Richtpreis"), Some(185.0));
    assert_eq!(zahl(&p, "Druckfestigkeit [N/mm²]"), Some(4.0));
}

/// A233 (§1.2, §4.4, Koordinator 11:26): „Verwendet in“ am Prüfhaus.
/// Porenbeton: AW-31,5 mit 8 Wänden, IW-17,5 mit 1 Wand, dazu jeder
/// weitere Typ, der ihn enthält (Regel 15 zählt Typen, auch unbenutzte).
/// Stahlbeton: DE-001, DE-002, SP-001, FS-001 (Bauteile ohne Typ).
/// Dämmung hart und Terrassenbelag: DT-14 mit 1 Bauteil. Titanzink:
/// AB-001. Putz: nichts.
#[test]
fn a233_verwendet_in() {
    let s = pruefhaus(233);
    let m = s.model();
    let gas = verwendet(m, stoff(m, "Porenbeton"));
    assert!(gas.contains(&Nutzung::Typ("AW-31,5".into(), 8)), "{gas:?}");
    assert!(gas.contains(&Nutzung::Typ("IW-17,5".into(), 1)), "{gas:?}");
    let mut soll_typen: Vec<String> = m
        .layer_sets()
        .iter()
        .filter(|(_, t)| {
            t.layers
                .iter()
                .any(|l| l.material == stoff(m, "Porenbeton"))
        })
        .map(|(_, t)| t.code.clone())
        .collect();
    soll_typen.sort();
    let mut ist_typen: Vec<String> = gas
        .iter()
        .filter_map(|n| match n {
            Nutzung::Typ(c, _) => Some(c.clone()),
            _ => None,
        })
        .collect();
    ist_typen.sort();
    assert_eq!(ist_typen, soll_typen, "jeder Typ mit Porenbeton");
    let beton = verwendet(m, stoff(m, "Stahlbeton"));
    for n in ["DE-001", "DE-002", "SP-001", "FS-001"] {
        assert!(
            beton.contains(&Nutzung::Bauteil(n.into())),
            "{n}: {beton:?}"
        );
    }
    for name in ["Dämmung hart (Terrasse)", "Terrassenbelag"] {
        assert_eq!(
            verwendet(m, stoff(m, name)),
            [Nutzung::Typ("DT-14".into(), 1)],
            "{name}"
        );
    }
    assert_eq!(
        verwendet(m, stoff(m, "Titanzink 0,7")),
        [Nutzung::Bauteil("AB-001".into())],
        "AB nutzt Titanzink (auch als Vorgabe ohne eigene Wahl)"
    );
    assert!(verwendet(m, stoff(m, "Putz")).is_empty());
}

/// A234 (§1.4, Entscheidung 20): Duplizieren ist der einzige Weg zu
/// einem neuen Baustoff. Die Kopie hat eine neue Guid, den Namen
/// „Porenbeton (2)“ (eindeutig, ein zweites Mal „(3)“), dieselben
/// Kennwerte und dieselbe Darstellung, Art, Rohdichte und λ. Sie ist
/// unbenutzt. Mengen ändern sich nicht.
#[test]
fn a234_duplizieren() {
    let mut s = pruefhaus(234);
    let g = mit_kennwerten(&mut s);
    let mengen = csv(&mut s);
    let mut neu = None;
    fenster_ok(&mut s, |m| neu = Some(duplizieren(m, g)));
    let m = s.model();
    let k = stoff(m, "Porenbeton (2)");
    assert!(neu.is_some());
    let (a, b) = (m.material(g).unwrap(), m.material(k).unwrap());
    assert_ne!(a.guid, b.guid);
    assert_eq!(kennwerte(m, g), kennwerte(m, k), "Kennwerte kopiert");
    assert_eq!(a.display(), b.display(), "Darstellung gleich");
    assert_eq!(
        (a.category, a.density, a.lambda),
        (b.category, b.density, b.lambda)
    );
    assert!(verwendet(m, k).is_empty());
    fenster_ok(&mut s, |m| {
        duplizieren(m, g);
    });
    stoff(s.model(), "Porenbeton (3)");
    assert_eq!(csv(&mut s), mengen);
}

/// A235 (§1.4, Regeln 15 und 55): Löschen nur ohne Verwendung. Porenbeton
/// (in Typen) und Stahlbeton (Decken) nein; Titanzink nein, solange
/// AB-001 es nutzt (heute meldet `material_used` es als frei); Luft nie;
/// Putz ja; eine Kopie mit Kennwerten ja (Kennwerte halten nichts fest).
#[test]
fn a235_loeschen() {
    let mut s = pruefhaus(235);
    let g = mit_kennwerten(&mut s);
    fenster_ok(&mut s, |m| {
        duplizieren(m, g);
    });
    let m = s.model();
    assert!(!loeschbar(m, g), "Porenbeton in Typen");
    assert!(!loeschbar(m, stoff(m, "Stahlbeton")), "Decken");
    assert!(!loeschbar(m, stoff(m, "Titanzink 0,7")), "AB-001");
    assert!(!loeschbar(m, stoff(m, "Luft")), "Luft nie");
    assert!(loeschbar(m, stoff(m, "Putz")));
    assert!(
        loeschbar(m, stoff(m, "Porenbeton (2)")),
        "Kennwerte halten nichts"
    );
}

/// A236 (§1.3, §4.7): OK übernimmt mehrere Änderungen als einen Schritt
/// „Baustoffe geändert“; Strg+Z nimmt alles zurück. Abbrechen ändert
/// nichts (Datei bytegleich, kein Schritt). λ eines verwendeten Baustoffs
/// ändert die Mengen nicht (Volumen), die Darstellung folgt nach OK.
#[test]
fn a236_ok_und_abbrechen() {
    let mut s = pruefhaus(236);
    let vorher = sk_model::szo::write(s.model());
    let schritt = s.undo_label();
    let g = stoff(s.model(), "Porenbeton");
    let b = stoff(s.model(), "Stahlbeton");
    fenster_abbrechen(&mut s, |m| {
        assert!(eintippen(m, g, "Richtpreis", "185"));
        assert!(eintippen(m, b, "λ", "2,5"));
    });
    assert_eq!(sk_model::szo::write(s.model()), vorher, "Abbrechen");
    assert_eq!(s.undo_label(), schritt);
    let mengen = csv(&mut s);
    fenster_ok(&mut s, |m| {
        assert!(eintippen(m, g, "Richtpreis", "185"));
        assert!(eintippen(m, b, "λ", "2,5"));
        assert!(eintippen(m, b, "Hersteller", "Beton AG"));
    });
    assert_eq!(s.undo_label(), Some("Baustoffe geändert"));
    assert_eq!(s.model().material(b).unwrap().lambda, Some(2.5));
    assert_eq!(csv(&mut s), mengen, "Volumen bleiben");
    assert!(s.undo());
    assert_eq!(sk_model::szo::write(s.model()), vorher, "ein Schritt");
    assert_eq!(s.undo_label(), schritt);
}

/// A237 (Regel 50): Feste Schlüssel. „µ“ (U+00B5) getippt oder in der
/// Datei wird unter „μ“ (U+03BC) gespeichert, ohne zweiten Schlüssel; die
/// Datei schreibt U+03BC. Ein Wert der falschen Art (Text beim
/// Richtpreis, Zahl beim Hersteller) wird beim Laden mit Hinweis
/// verworfen, die übrigen Zeilen bleiben. Ein eigener Kennwert darf keinen
/// festen Schlüssel tragen.
#[test]
fn a237_feste_schluessel() {
    let mut s = pruefhaus(237);
    let g = stoff(s.model(), "Porenbeton");
    fenster_ok(&mut s, |m| assert!(eintippen(m, g, "\u{b5}", "6")));
    let p = kennwerte(s.model(), g);
    assert_eq!(zahl(&p, "\u{3bc}"), Some(6.0));
    assert!(!p.contains_key("\u{b5}"), "kein zweiter Schlüssel");
    let text = sk_model::szo::write(s.model());
    assert!(text.contains("key=\"\u{3bc}\""));
    assert!(!text.contains('\u{b5}'));

    let ohne = sk_model::szo::write(pruefhaus(2370).model());
    let g0 = {
        let l = lesen(&ohne);
        l.model
            .material(stoff(&l.model, "Porenbeton"))
            .unwrap()
            .guid
    };
    let datei = format!(
        "{ohne}[matprop] mat={g0} key=\"\u{b5}\" num=7\n\
             [matprop] mat={g0} key=\"Richtpreis\" value=\"teuer\"\n\
             [matprop] mat={g0} key=\"Hersteller\" num=5\n\
             [matprop] mat={g0} key=\"Preisstand\" value=\"10/2026\"\n"
    );
    let l = lesen(&datei);
    assert_eq!(l.hints.len(), 2, "zwei falsche Arten: {:?}", l.hints);
    let p = kennwerte(&l.model, stoff(&l.model, "Porenbeton"));
    assert_eq!(zahl(&p, "\u{3bc}"), Some(7.0), "µ aus der Datei → μ");
    assert!(!p.contains_key("\u{b5}"));
    assert!(!p.contains_key("Richtpreis") && !p.contains_key("Hersteller"));
    assert_eq!(
        wort(&p, "Preisstand").as_deref(),
        Some("10/2026"),
        "übrige bleiben"
    );

    let mut m = s.model().clone();
    m.allow_unstepped();
    assert!(!eigener(&mut m, g, "Richtpreis", "5"), "fester Schlüssel");
    assert!(!eigener(&mut m, g, "\u{b5}", "5"), "auch als µ");
}

/// A238 (Regeln 51, 52): Wertebereiche und Euroklasse, bei der Eingabe
/// und beim Laden. Abgelehnt: λ ≤ 0, Rohdichte ≤ 0 (Luft ausgenommen),
/// μ < 1, c ≤ 0, Richtpreis < 0, Preisstand nicht MM/JJJJ, Euroklasse „G“,
/// Zusatz an A1, E oder F. Angenommen: „A1“, „F“, „B-s1,d0“, „Cfl-s1“,
/// „A2-s3,d2“, μ = 1, Richtpreis 0, Preisstand „01/2027“. Beim Laden wird
/// nur die falsche Zeile mit Hinweis verworfen.
#[test]
fn a238_wertebereiche_euroklasse() {
    let s = pruefhaus(238);
    let mut m = s.model().clone();
    m.allow_unstepped();
    let g = stoff(&m, "Porenbeton");
    let luft = stoff(&m, "Luft");
    for (feld, wert) in [
        ("λ", "0"),
        ("λ", "-0,1"),
        ("Rohdichte", "0"),
        ("μ", "0,5"),
        ("c", "0"),
        ("Richtpreis", "-1"),
        ("Preisstand", "13/2026"),
        ("Preisstand", "2026-10"),
        ("Euroklasse", "G"),
        ("Euroklasse", "A1-s1"),
        ("Euroklasse", "E-s1"),
        ("Euroklasse", "F,d0"),
    ] {
        assert!(
            !eintippen(&mut m, g, feld, wert),
            "{feld} = {wert} abgelehnt"
        );
    }
    assert_eq!(m.material(g).unwrap().lambda, Some(0.09), "λ unverändert");
    for (feld, wert) in [
        ("Euroklasse", "A1"),
        ("Euroklasse", "F"),
        ("Euroklasse", "B-s1,d0"),
        ("Euroklasse", "Cfl-s1"),
        ("Euroklasse", "A2-s3,d2"),
        ("μ", "1"),
        ("c", "1000"),
        ("Richtpreis", "0"),
        ("Preisstand", "01/2027"),
    ] {
        assert!(
            eintippen(&mut m, g, feld, wert),
            "{feld} = {wert} angenommen"
        );
    }
    assert!(eintippen(&mut m, luft, "Rohdichte", "1,2"), "Luft");

    let ohne = sk_model::szo::write(s.model());
    let gg = s.model().material(g).unwrap().guid;
    let datei = format!(
        "{ohne}[matprop] mat={gg} key=\"μ\" num=0.5\n\
             [matprop] mat={gg} key=\"c\" num=0\n\
             [matprop] mat={gg} key=\"Euroklasse\" value=\"G\"\n\
             [matprop] mat={gg} key=\"Richtpreis\" num=-3 unit=m3\n\
             [matprop] mat={gg} key=\"Preisstand\" value=\"13/2026\"\n\
             [matprop] mat={gg} key=\"Euroklasse\" value=\"B-s1,d0\"\n\
             [matprop] mat={gg} key=\"Hersteller\" value=\"bleibt\"\n"
    );
    let l = lesen(&datei);
    assert_eq!(l.hints.len(), 5, "{:?}", l.hints);
    let p = kennwerte(&l.model, stoff(&l.model, "Porenbeton"));
    assert_eq!(wort(&p, "Euroklasse").as_deref(), Some("B-s1,d0"));
    assert_eq!(wort(&p, "Hersteller").as_deref(), Some("bleibt"));
    assert!(!p.contains_key("μ") && !p.contains_key("c") && !p.contains_key("Richtpreis"));
    assert!(!p.contains_key("Preisstand"));
}

/// A239 (§2.2, Regel 53): Richtpreis mit Einheit. Mauerwerk, Beton,
/// Dämmung, Holz: m3; Putz und Metall: m2; Luft: kein Preis. Die Einheit
/// wird beim Setzen gespeichert. Wechselt danach die Baustoffart, bleibt
/// Preis und Einheit, das Fenster zeigt den Hinweis mit der alten Einheit
/// („€/m³“, „passt nicht“); neu eingetragen gilt die neue Einheit und der
/// Hinweis verschwindet.
#[test]
fn a239_preiseinheit() {
    let s = pruefhaus(239);
    let mut m = s.model().clone();
    m.allow_unstepped();
    for (name, einheit) in [
        ("Porenbeton", "m3"),
        ("Stahlbeton", "m3"),
        ("Dämmung (WDVS)", "m3"),
        ("Putz", "m2"),
        ("Titanzink 0,7", "m2"),
    ] {
        let id = stoff(&m, name);
        assert!(eintippen(&mut m, id, "Richtpreis", "10"), "{name}");
        assert_eq!(preiseinheit(&m, id).as_deref(), Some(einheit), "{name}");
        assert_eq!(preis_hinweis(&m, id), None, "{name}");
    }
    let luft = stoff(&m, "Luft");
    assert!(
        !eintippen(&mut m, luft, "Richtpreis", "10"),
        "Luft ohne Preis"
    );

    let g = stoff(&m, "Porenbeton");
    assert!(eintippen(&mut m, g, "Richtpreis", "185"));
    assert!(art_wechseln(&mut m, g, MatCategory::Plaster));
    let p = kennwerte(&m, g);
    assert_eq!(zahl(&p, "Richtpreis"), Some(185.0), "nie umgedeutet");
    assert_eq!(preiseinheit(&m, g).as_deref(), Some("m3"));
    let h = preis_hinweis(&m, g).expect("Hinweis");
    assert!(h.contains("€/m³") && h.contains("passt nicht"), "{h}");
    assert!(eintippen(&mut m, g, "Richtpreis", "20"));
    assert_eq!(preiseinheit(&m, g).as_deref(), Some("m2"));
    assert_eq!(preis_hinweis(&m, g), None);
}

/// A240 (§4.9): Paket 5 ändert keine Mengen, Bilder und Nummern. Mit
/// Kennwerten an allen verwendeten Baustoffen sind CSV, Nummern und das
/// 3D-Netz gleich wie ohne.
#[test]
fn a240_mengen_und_bild_unveraendert() {
    let mut s = pruefhaus(240);
    let mengen = csv(&mut s);
    let flaechen = s.mesh(ViewKind::Persp, None, &[]).faces.len();
    let nummern: Vec<String> = s
        .model()
        .elements()
        .iter()
        .map(|(_, e)| e.number.clone())
        .collect();
    let ids: Vec<MaterialId> = s.model().materials().iter().map(|(id, _)| id).collect();
    fenster_ok(&mut s, |m| {
        for id in ids {
            if m.material(id).unwrap().category != MatCategory::Air {
                assert!(eintippen(m, id, "Richtpreis", "100"));
                assert!(eintippen(m, id, "Hersteller", "Werk"));
            }
        }
    });
    assert_eq!(csv(&mut s), mengen);
    assert_eq!(s.mesh(ViewKind::Persp, None, &[]).faces.len(), flaechen);
    let nach: Vec<String> = s
        .model()
        .elements()
        .iter()
        .map(|(_, e)| e.number.clone())
        .collect();
    assert_eq!(nach, nummern);
}
