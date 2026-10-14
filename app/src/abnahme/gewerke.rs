use super::*;

// Abnahmetests A183–A187: Gewerke und Kostengruppen an der Schicht
// (Commit 1a; bim/paket-1a-gewerke.md §1–6, Regeln 47–49; Grundlage
// projektstruktur/gewerke.md, bim/paket-r4-deckenschichten.md §3–4).
// Spezifikation: test/abnahme-kategorien.md. Setzt R4a voraus
// (`Model::element_layers`, a181-a182-deckenschichten.rs).
//
// Einbau: als `mod gewerke { use super::*; … }` ans Ende von
// app/src/abnahme.rs. Nutzt aus abnahme.rs: gebaeude, cam3d, tool, click,
// key, W, H.
//
// Angenommene Namen stehen nur in den Adaptern (BIM paket-1a §1):
// `Model::trades` mit `Trade { guid, code, name, order }`, `Material::trade`,
// `Model::layer_trade(id, i)` und `Model::layer_kg(id, i)` (aufgelöst für
// Schicht i von `element_layers(id)`), `Model::set_material_trade`,
// `Model::set_layer_trade`, `Model::set_layer_kg`, `Model::trade_by_code`.

use sk_model::{Category, ElementId, LayerSetId, MaterialId};

// ===== Adapter 1a =====

/// Startbestand bzw. Gewerke des Projekts: (ATV, Name, Reihe), nach Reihe.
fn gewerke(m: &Model) -> Vec<(String, String, u16)> {
    let mut v: Vec<_> = m
        .trades()
        .iter()
        .map(|t| (t.code.clone(), t.name.clone(), t.order))
        .collect();
    v.sort_by_key(|g| g.2);
    v
}

fn gewerk_guids(m: &Model) -> Vec<sk_model::Guid> {
    m.trades().iter().map(|t| t.guid).collect()
}

/// ATV-Nummer des Gewerks, das für Schicht `i` des Bauteils gilt
/// (Schicht ∨ Baustoff ∨ Bauteilart); `None`: kein Gewerk (Luft).
fn gewerk(m: &Model, id: ElementId, i: usize) -> Option<String> {
    m.layer_trade(id, i)
        .and_then(|t| m.trade(t))
        .map(|t| t.code.clone())
}

/// KG der Schicht `i` (Schicht ∨ Tabelle nach Bauteilart und Lage).
fn kg(m: &Model, id: ElementId, i: usize) -> Option<u16> {
    m.layer_kg(id, i)
}

/// Gewerk-Vorschlag des Baustoffs.
fn stoff_gewerk(m: &Model, mat: MaterialId) -> Option<String> {
    m.material(mat)
        .unwrap()
        .trade
        .and_then(|t| m.trade(t))
        .map(|t| t.code.clone())
}

/// Gewerk am Baustoff setzen, ein Schritt.
fn stoff_gewerk_setzen(s: &mut Scene, mat: MaterialId, code: &str) -> bool {
    let t = s.model().trade_by_code(code).unwrap();
    s.edit_model("Gewerk geändert", |m| m.set_material_trade(mat, Some(t)))
}

/// Abweichendes Gewerk an Schicht `i` eines Typs (`None`: zurück zum
/// Baustoff), ein Schritt.
fn schicht_gewerk(s: &mut Scene, typ: LayerSetId, i: usize, code: Option<&str>) -> bool {
    let t = code.map(|c| s.model().trade_by_code(c).unwrap());
    s.edit_model("Gewerk geändert", |m| m.set_layer_trade(typ, i, t))
}

/// Abweichende KG an Schicht `i` eines Typs, ein Schritt; `false`:
/// abgelehnt (Regel 49).
fn schicht_kg(s: &mut Scene, typ: LayerSetId, i: usize, kg: Option<u16>) -> bool {
    s.edit_model("Kostengruppe geändert", |m| m.set_layer_kg(typ, i, kg))
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
        .unwrap_or_else(|| panic!("Baustoff {name}"))
}

fn typ(m: &Model, code: &str) -> LayerSetId {
    m.layer_sets()
        .iter()
        .find(|(_, t)| t.code == code)
        .map(|(id, _)| id)
        .unwrap_or_else(|| panic!("Typ {code}"))
}

fn lesen(text: &str) -> sk_model::szo::Loaded {
    sk_model::szo::read(text, sk_model::GuidGen::with_seed(1)).expect("öffnet")
}

/// Gewerke aller Schichten eines Bauteils.
fn gewerke_von(m: &Model, id: ElementId) -> Vec<Option<String>> {
    (0..m.element_layers(id).len())
        .map(|i| gewerk(m, id, i))
        .collect()
}

fn kgs_von(m: &Model, id: ElementId) -> Vec<Option<u16>> {
    (0..m.element_layers(id).len())
        .map(|i| kg(m, id, i))
        .collect()
}

fn g(code: &str) -> Option<String> {
    Some(code.to_string())
}

/// Prüfhaus wie paket-og-phase2 §8: Dialog, 10 × 8 m, AW-31,5, dazu im EG
/// IW-17,5 bei x = 5 m und IW-11,5 bei y = 4 m (x 0 … 5); mit `vor`
/// AW-006 gelöst und 0,30 m vor (UD-001). Ohne `innen` keine Innenwände
/// (Prüfhaus der BIM-Sollwerte).
fn pruefhaus(seed: u64, vor: bool, innen: bool) -> Scene {
    let mut s = Scene::with_model(Model::with_seed(seed));
    gebaeude(&mut s);
    let c = cam3d();
    let iw = if innen { 2 } else { 0 };
    for (a, b, code) in [
        ((5000.0, 0.0), (5000.0, 8000.0), "IW-17,5"),
        ((0.0, 4000.0), (5000.0, 4000.0), "IW-11,5"),
    ]
    .into_iter()
    .take(iw)
    {
        let set = typ(s.model(), code);
        let mut t = tool(&s);
        t.set_category(Category::InteriorWall, s.model().wall_layers(set));
        click(&mut t, &c, vec3(a.0, a.1, 0.0));
        click(&mut t, &c, vec3(b.0, b.1, 0.0));
        let w = t.handle(&key(Key::Enter), &c, W, H, 1.0).commit.unwrap();
        s.add_wall_typed(&w, Category::InteriorWall, Some(set))
            .unwrap();
    }
    if vor {
        let w = nr(&s, "AW-006");
        assert!(s.edit_model("Kopplung gelöst", |m| m.set_linked(w, false)));
        assert!(s.edit_model("Wand verschoben", |m| m.move_segment(w, 300.0).is_some()));
    }
    assert!(s.model().check().is_empty(), "{:?}", s.model().check());
    s
}

/// Prüfhaus mit dem Typ `code` für den ganzen Außenwandstapel.
fn pruefhaus_typ(seed: u64, code: &str) -> Scene {
    let mut s = Scene::with_model(Model::with_seed(seed));
    let (eg, _) = gebaeude(&mut s);
    let t = typ(s.model(), code);
    assert!(s.edit_model("Wandtyp", |m| m.set_run_type(eg, t)));
    s
}

/// Startbestand nach BIM paket-1a §2: (Reihe, ATV, Name).
const START: [(u16, &str, &str); 36] = [
    (1, "18459", "Abbruch- und Rückbauarbeiten"),
    (2, "18451", "Gerüstarbeiten"),
    (3, "18300", "Erdarbeiten"),
    (4, "18308", "Drän- und Versickerarbeiten"),
    (5, "18331", "Betonarbeiten"),
    (6, "18330", "Mauerarbeiten"),
    (7, "18336", "Abdichtungsarbeiten"),
    (8, "18334", "Zimmer- und Holzbauarbeiten"),
    (9, "18335", "Stahlbauarbeiten"),
    (10, "18338", "Dachdeckungs- und Dachabdichtungsarbeiten"),
    (11, "18339", "Klempnerarbeiten"),
    (12, "18355", "Tischlerarbeiten"),
    (13, "18360", "Metallbauarbeiten"),
    (14, "18361", "Verglasungsarbeiten"),
    (15, "18358", "Rollladenarbeiten"),
    (16, "18345", "Wärmedämm-Verbundsysteme"),
    (17, "18351", "Vorgehängte hinterlüftete Fassaden"),
    (18, "18350", "Putz- und Stuckarbeiten"),
    (19, "18340", "Trockenbauarbeiten"),
    (20, "18353", "Estricharbeiten"),
    (21, "18352", "Fliesen- und Plattenarbeiten"),
    (22, "18332", "Naturwerksteinarbeiten"),
    (23, "18333", "Betonwerksteinarbeiten"),
    (24, "18356", "Parkett- und Holzpflasterarbeiten"),
    (25, "18365", "Bodenbelagarbeiten"),
    (26, "18357", "Beschlagarbeiten"),
    (27, "18363", "Maler- und Lackierarbeiten"),
    (28, "18366", "Tapezierarbeiten"),
    (29, "18379", "Raumlufttechnische Anlagen"),
    (
        30,
        "18380",
        "Heizanlagen und zentrale Wassererwärmungsanlagen",
    ),
    (
        31,
        "18381",
        "Gas-, Wasser- und Entwässerungsanlagen innerhalb von Gebäuden",
    ),
    (32, "18382", "Nieder- und Mittelspannungsanlagen"),
    (33, "18384", "Blitzschutzanlagen"),
    (
        34,
        "18385",
        "Förderanlagen, Aufzugsanlagen, Fahrtreppen und Fahrsteige",
    ),
    (35, "18386", "Gebäudeautomation"),
    (36, "18320", "Landschaftsbauarbeiten"),
];

/// A183 (paket-1a §2, Regel 48): Jedes Projekt hat den Startbestand von
/// 36 Gewerken in der Reihenfolge des Bauablaufs, mit festen Guids (neue
/// Projekte und Testprojekte gleich), Codes und Reihen eindeutig.
#[test]
fn a183_gewerke_startbestand() {
    let a = Model::with_seed(183);
    let b = Model::new();
    let soll: Vec<(String, String, u16)> = START
        .iter()
        .map(|(o, c, n)| (c.to_string(), n.to_string(), *o))
        .collect();
    assert_eq!(gewerke(&a), soll);
    assert_eq!(gewerke(&b), soll);
    let mut ga = gewerk_guids(&a);
    let mut gb = gewerk_guids(&b);
    ga.sort();
    gb.sort();
    assert_eq!(ga, gb, "feste Guids");
    ga.dedup();
    assert_eq!(ga.len(), 36);
    assert!(a.check().is_empty(), "{:?}", a.check());
}

/// A184 (paket-1a §3, R4 §3): Jeder Startbaustoff schlägt sein Gewerk
/// vor, und jede Schicht jedes heutigen Bauteils löst es auf:
/// AW-31,5 und AW-36 WDVS 18345 + Porenbeton 18330; AW-49 Verblender,
/// Kerndämmung und Porenbeton 18330, Luft ohne Gewerk; AW-36,5 18330 mit
/// Randdämmstreifen 18330; IW 18330; Decken, Sohlplatte, Frostschürze
/// 18331; UD über ihre eingebaute Schicht 18345.
#[test]
fn a184_gewerk_je_schicht() {
    let m = Model::with_seed(184);
    for (stoff_name, soll) in [
        ("Porenbeton", g("18330")),
        ("Stahlbeton", g("18331")),
        ("Dämmung (WDVS)", g("18345")),
        ("Kerndämmung (Mineralwolle)", g("18330")),
        ("Verblender (Vormauerziegel)", g("18330")),
        ("Luft", None),
        ("Randdämmung", g("18330")),
        ("Putz", g("18350")),
    ] {
        assert_eq!(
            stoff_gewerk(&m, stoff(&m, stoff_name)),
            soll,
            "{stoff_name}"
        );
    }

    let s = pruefhaus(184, true, true);
    let m = s.model();
    for n in ["AW-001", "AW-006"] {
        assert_eq!(gewerke_von(m, nr(&s, n)), [g("18345"), g("18330")], "{n}");
    }
    for n in ["IW-001", "IW-002"] {
        assert_eq!(gewerke_von(m, nr(&s, n)), [g("18330")], "{n}");
    }
    for n in ["DE-001", "DE-002", "SP-001", "FS-001"] {
        assert_eq!(gewerke_von(m, nr(&s, n)), [g("18331")], "{n}");
    }
    // UD mit Bekleidung darunter (W3, VHF)
    assert_eq!(
        gewerke_von(m, nr(&s, "UD-001")),
        [g("18345"), g("18351")],
        "UD eingebaut"
    );

    let s = pruefhaus_typ(1840, "AW-49");
    assert_eq!(
        gewerke_von(s.model(), nr(&s, "AW-001")),
        [g("18330"), None, g("18330"), g("18330")],
        "Verblender, Luft, Kerndämmung, Porenbeton"
    );
    let s = pruefhaus_typ(1841, "AW-36,5");
    assert_eq!(gewerke_von(s.model(), nr(&s, "AW-001")), [g("18330")]);
    assert_eq!(gewerke_von(s.model(), nr(&s, "RD-001")), [g("18330")]);
    let s = pruefhaus_typ(1842, "AW-36");
    assert_eq!(
        gewerke_von(s.model(), nr(&s, "AW-001")),
        [g("18345"), g("18330")]
    );
    assert!(s.model().check().is_empty());
}

/// A185 (paket-1a §4, Regel 49): KG je Schicht aus Bauteilart und Lage
/// zum Kern: AW außen 335, Kern 331; IW tragend (Kern ab 175 mm) 341,
/// IW-11,5 342; Decke 351; Sohlplatte und Frostschürze 322; UD 354;
/// Randdämmstreifen 331. Eine KG an der Schicht geht vor, außerhalb
/// 311–399 wird sie abgelehnt.
#[test]
fn a185_kg_je_schicht() {
    let mut s = pruefhaus(185, true, true);
    let m = s.model();
    assert_eq!(kgs_von(m, nr(&s, "AW-001")), [Some(335), Some(331)]);
    assert_eq!(kgs_von(m, nr(&s, "IW-001")), [Some(341)], "IW-17,5 tragend");
    assert_eq!(
        kgs_von(m, nr(&s, "IW-002")),
        [Some(342)],
        "IW-11,5 nichttragend"
    );
    for n in ["DE-001", "DE-002"] {
        assert_eq!(kgs_von(m, nr(&s, n)), [Some(351)], "{n}");
    }
    for n in ["SP-001", "FS-001"] {
        assert_eq!(kgs_von(m, nr(&s, n)), [Some(322)], "{n}");
    }
    assert_eq!(kgs_von(m, nr(&s, "UD-001")), [Some(354), Some(354)]);

    let aw = typ(s.model(), "AW-31,5");
    let schritt = s.undo_label();
    for falsch in [299, 400, 35] {
        assert!(
            !schicht_kg(&mut s, aw, 0, Some(falsch)),
            "KG {falsch} abgelehnt"
        );
    }
    assert_eq!(s.undo_label(), schritt, "kein Schritt");
    assert!(schicht_kg(&mut s, aw, 0, Some(336)));
    assert_eq!(kgs_von(s.model(), nr(&s, "AW-001")), [Some(336), Some(331)]);
    assert!(s.model().check().is_empty());
    assert!(s.undo());
    assert_eq!(kgs_von(s.model(), nr(&s, "AW-001")), [Some(335), Some(331)]);

    let s = pruefhaus_typ(1850, "AW-36,5");
    assert_eq!(kgs_von(s.model(), nr(&s, "RD-001")), [Some(331)]);
    assert_eq!(kgs_von(s.model(), nr(&s, "AW-001")), [Some(331)]);
    let s = pruefhaus_typ(1851, "AW-49");
    let k = kgs_von(s.model(), nr(&s, "AW-001"));
    assert_eq!((k[0], k[2], k[3]), (Some(335), Some(335), Some(331)));
}

/// A186 (R4 §3, paket-1a §1): Auflösung Schicht ∨ Baustoff ∨ Bauteilart.
/// Gewerk am Baustoff Porenbeton → 18331: alle Porenbetonschichten folgen
/// (AW-Kern, IW). Abweichung an der WDVS-Schicht von AW-31,5 → 18330:
/// nur diese Schicht, der Baustoff bleibt 18345, die UD (eigene Schicht)
/// bleibt 18345. Zurück auf `None`: wieder der Baustoff. Jede Änderung
/// ist ein Schritt und rückgängig zu machen; `check()` bleibt leer
/// (Regel 47).
#[test]
fn a186_aufloesung_schicht_baustoff_bauteilart() {
    let mut s = pruefhaus(186, true, true);
    let aw = nr(&s, "AW-001");
    let iw = nr(&s, "IW-001");
    let ud = nr(&s, "UD-001");
    let gasbeton = stoff(s.model(), "Porenbeton");
    let wdvs = stoff(s.model(), "Dämmung (WDVS)");

    assert!(stoff_gewerk_setzen(&mut s, gasbeton, "18331"));
    assert_eq!(gewerke_von(s.model(), aw), [g("18345"), g("18331")]);
    assert_eq!(gewerke_von(s.model(), iw), [g("18331")]);
    assert!(s.model().check().is_empty());
    assert!(s.undo());
    assert_eq!(gewerke_von(s.model(), iw), [g("18330")]);

    let t = typ(s.model(), "AW-31,5");
    assert!(schicht_gewerk(&mut s, t, 0, Some("18330")));
    let m = s.model();
    assert_eq!(gewerke_von(m, aw), [g("18330"), g("18330")]);
    assert_eq!(gewerke_von(m, nr(&s, "AW-006")), [g("18330"), g("18330")]);
    assert_eq!(stoff_gewerk(m, wdvs), g("18345"), "Baustoff bleibt");
    assert_eq!(
        gewerke_von(m, ud),
        [g("18345"), g("18351")],
        "UD hat ihre eigene Schicht"
    );
    assert!(m.check().is_empty(), "{:?}", m.check());
    assert!(schicht_gewerk(&mut s, t, 0, None));
    assert_eq!(gewerke_von(s.model(), aw), [g("18345"), g("18330")]);
    assert!(s.undo());
    assert!(s.undo());
    assert_eq!(gewerke_von(s.model(), aw), [g("18345"), g("18330")]);
}

/// A187 (paket-1a §5, F-17): Datei bleibt `SZO 4`. `[trade]` steht je
/// verwendetem Gewerk (an einem Baustoff oder einer Schicht) vor dem
/// ersten `[material]`, `[material] trade=` je Baustoff mit Gewerk,
/// `[layer] trade= kg=` nur bei Abweichung. Rundlauf bytegleich. Eine
/// alte Datei ohne Gewerke öffnet ohne Hinweis, die Startbaustoffe haben
/// ihr Gewerk; nach dem ersten Speichern ist der Rundlauf bytegleich.
/// Unbekannte Guid in `trade=`: Hinweis, kein Gewerk. Ein Startgewerk
/// mit anderem Namen in der Datei behält den Namen aus der Datei.
#[test]
fn a187_datei_gewerke() {
    let mut s = pruefhaus(187, false, true);
    let text = sk_model::szo::write(s.model());
    assert!(text.starts_with("SZO 4\n"));
    let trades: Vec<&str> = text.lines().filter(|l| l.starts_with("[trade]")).collect();
    let codes: Vec<&str> = trades
        .iter()
        .map(|l| {
            l.split("code=\"")
                .nth(1)
                .unwrap()
                .split('"')
                .next()
                .unwrap()
        })
        .collect();
    let mut sortiert = codes.clone();
    sortiert.sort();
    // Ausbau der Vorgabe-Nutzung Wohnzimmer der Räume und Estrich
    // (Raumbuch #112): 18336 Dämmung, 18353 Estrich, 18356 Parkett, 18363
    // Maler, 18366 Tapezierer
    assert_eq!(
        sortiert,
        ["18330", "18331", "18336", "18345", "18350", "18353", "18356", "18363", "18366"],
        "verwendete Gewerke"
    );
    let erster_trade = text.find("\n[trade]").unwrap();
    let erstes_material = text.find("\n[material]").unwrap();
    assert!(erster_trade < erstes_material, "[trade] vor [material]");
    for l in text.lines().filter(|l| l.starts_with("[material]")) {
        assert_eq!(l.contains(" trade="), !l.contains("name=\"Luft\""), "{l}");
    }
    assert!(!text
        .lines()
        .any(|l| l.starts_with("[layer]") && (l.contains(" trade=") || l.contains(" kg="))));
    assert_eq!(sk_model::szo::write(&lesen(&text).model), text, "Rundlauf");

    // Abweichung an der Schicht steht an der [layer]-Zeile
    let t = typ(s.model(), "AW-31,5");
    assert!(schicht_gewerk(&mut s, t, 0, Some("18330")));
    assert!(schicht_kg(&mut s, t, 0, Some(336)));
    let text2 = sk_model::szo::write(s.model());
    let mit: Vec<&str> = text2
        .lines()
        .filter(|l| l.starts_with("[layer]") && l.contains(" trade=") && l.contains(" kg=336"))
        .collect();
    assert_eq!(mit.len(), 1, "genau die WDVS-Schicht von AW-31,5");
    assert_eq!(
        sk_model::szo::write(&lesen(&text2).model),
        text2,
        "Rundlauf"
    );

    // alte Datei (Stand vor 1a): ohne [trade] und trade=
    let alt: String = text
        .lines()
        .filter(|l| !l.starts_with("[trade]"))
        .map(|l| {
            let mut z = l.to_string();
            if let Some(i) = z.find(" trade=") {
                let ende = z[i + 1..].find(' ').map_or(z.len(), |j| i + 1 + j);
                z.replace_range(i..ende, "");
            }
            z + "\n"
        })
        .collect();
    assert!(!alt.contains("trade"));
    let geladen = lesen(&alt);
    assert!(geladen.hints.is_empty(), "still: {:?}", geladen.hints);
    let m = &geladen.model;
    assert_eq!(stoff_gewerk(m, stoff(m, "Porenbeton")), g("18330"));
    assert_eq!(stoff_gewerk(m, stoff(m, "Putz")), g("18350"));
    let neu = sk_model::szo::write(m);
    assert_eq!(neu, text, "nach dem ersten Speichern wie eine neue Datei");

    // unbekannte Guid an einem Baustoff
    let fremd = {
        let l = text
            .lines()
            .find(|l| l.starts_with("[material]") && l.contains("name=\"Porenbeton\""))
            .unwrap();
        let i = l.find(" trade=").unwrap() + 7;
        let guid = &l[i..i + 22];
        text.replacen(
            &format!("{l}\n"),
            &format!("{}\n", l.replace(guid, "3zzzzzzzzzzzzzzzzzzzzz")),
            1,
        )
    };
    let geladen = lesen(&fremd);
    assert!(!geladen.hints.is_empty(), "Hinweis bei unbekanntem Gewerk");
    let m = &geladen.model;
    assert_eq!(stoff_gewerk(m, stoff(m, "Porenbeton")), None);

    // Firmenname eines Startgewerks bleibt
    let umbenannt = text.replacen(
        "name=\"Mauerarbeiten\"",
        "name=\"Maurer- und Betonarbeiten\"",
        1,
    );
    let m = lesen(&umbenannt).model;
    let maurer = gewerke(&m).into_iter().find(|g| g.0 == "18330").unwrap();
    assert_eq!(maurer.1, "Maurer- und Betonarbeiten");
    assert_eq!(
        sk_model::szo::write(&m),
        umbenannt,
        "Rundlauf mit Firmenname"
    );
}
