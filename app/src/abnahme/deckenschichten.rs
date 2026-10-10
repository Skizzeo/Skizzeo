use super::*;

// Abnahmetests A181–A182: Schichten für Decke, Sohlplatte und Frostschürze
// (Commit 0d, R4a; bim/paket-r4-deckenschichten.md §1–2, §5–6, Regeln
// 37–40). Spezifikation: test/abnahme-kategorien.md.
//
// Einbau: als `mod deckenschichten { use super::*; … }` ans Ende von
// app/src/abnahme.rs. Nutzt aus abnahme.rs: gebaeude, decke.
//
// R4a ist unsichtbar: Ohne Typ an Decke, Sohlplatte oder Frostschürze
// bleiben Mengen, Bilder und Dateien bytegleich (A175–A177 sichern das).
//
// Fassung 2 (09:30): Adapter auf die API von 09c2b48 umgestellt
// (`MaterialLayer::new(..).core()`, `Model::build_up`,
// `Model::set_slab_type`).

use sk_model::{
    Category, ElementId, LayerFunction, LayerSet, LayerSetId, MaterialLayer, PropSet, TypeCategory,
};

// ===== Adapter R4a =====

/// Schicht über den Konstruktor (R4 §1.1).
fn schicht(m: &Model, baustoff: &str, t: f64, f: LayerFunction, kern: bool) -> MaterialLayer {
    let l = MaterialLayer::new(stoff(m, baustoff), t, f);
    if kern {
        l.core()
    } else {
        l
    }
}

/// Aufbau eines Bauteils, Wände von außen nach innen, waagerechte
/// Bauteile von oben nach unten: (Baustoff, Dicke mm, Funktion, Kern).
fn aufbau(m: &Model, id: ElementId) -> Vec<(String, f64, LayerFunction, bool)> {
    m.element_layers(id)
        .iter()
        .map(|l| {
            (
                m.material(l.material).unwrap().name.clone(),
                l.thickness,
                l.function,
                l.core,
            )
        })
        .collect()
}

/// Typ an Decke, Sohlplatte oder Frostschürze setzen (`None`: zurück auf
/// den Einschicht-Aufbau), ein Schritt; `false`: abgelehnt.
fn typ_setzen(s: &mut Scene, id: ElementId, t: Option<LayerSetId>) -> bool {
    s.edit_model("Typ geändert", |m| m.set_slab_type(id, t))
}

// ===== Hilfen =====

fn stoff(m: &Model, name: &str) -> sk_model::MaterialId {
    m.materials()
        .iter()
        .find(|(_, x)| x.name == name)
        .map(|(id, _)| id)
        .unwrap_or_else(|| panic!("Baustoff {name}"))
}

fn nr(s: &Scene, nummer: &str) -> ElementId {
    s.model()
        .elements()
        .iter()
        .find(|(_, e)| e.number == nummer)
        .map(|(id, _)| id)
        .unwrap_or_else(|| panic!("{nummer} fehlt"))
}

fn guid_text(g: sk_model::Guid) -> String {
    g.to_string()
}

fn lesen(text: &str) -> sk_model::szo::Loaded {
    sk_model::szo::read(text, sk_model::GuidGen::with_seed(1)).expect("öffnet")
}

/// Projekttyp „Decke mit Estrich“: Putz 50 als Belag oben, darunter der
/// Stahlbetonkern (Dicke am Bauteil).
fn deckentyp(s: &mut Scene) -> LayerSetId {
    let m = s.model();
    let typ = LayerSet {
        guid: sk_model::Guid(0x0a18_1000_0000_0000_0000_0000_0000_0001),
        name: "Decke mit Estrich".into(),
        code: "DE-1".into(),
        category: TypeCategory::Floor,
        layers: vec![
            schicht(m, "Putz", 50.0, LayerFunction::Finish, false),
            schicht(m, "Stahlbeton", 220.0, LayerFunction::Structure, true),
        ],
        props: PropSet::new(),
        note: String::new(),
        changed: 1,
        bearing: Default::default(),
    };
    let mut id = None;
    assert!(s.edit_model("Typ angelegt", |m| {
        id = m.add_layer_set(typ.clone());
        id.is_some()
    }));
    id.unwrap()
}

/// A181 (R4 §1, Regeln 37/38): Decke, Sohlplatte und Frostschürze haben
/// ohne Typ einen gedachten Einschicht-Aufbau aus ihrem Baustoff und ihrer
/// Dicke (Frostschürze: Breite), gespeichert wird nichts Neues. Typarten
/// DE, SP, FS sind offen. Ein Decken-Typ „Putz 50 + Kern“ ergibt den
/// Aufbau 50 + Floor.thickness, der Kernbaustoff ist `mat=`; die Datei
/// schreibt `set=` und `[layerset] cat=floor`, der Rundlauf ist
/// bytegleich, Rückgängig stellt die alte Datei wieder her. Ein Wandtyp
/// an der Decke wird abgelehnt.
#[test]
fn a181_einschicht_aufbau_und_deckentyp() {
    for (c, t) in [
        (Category::Floor, TypeCategory::Floor),
        (Category::GroundSlab, TypeCategory::GroundSlab),
        (Category::StripFooting, TypeCategory::StripFooting),
    ] {
        assert_eq!(TypeCategory::of(c), Some(t));
        assert_eq!(t.prefix(), c.prefix(), "{c:?}");
    }
    for c in [Category::EdgeInsulation, Category::SoffitInsulation] {
        assert_eq!(TypeCategory::of(c), None, "{c:?} ohne Typ");
    }

    let mut s = Scene::with_model(Model::with_seed(181));
    let (eg, _og) = gebaeude(&mut s);
    let l = schicht(
        s.model(),
        "Stahlbeton",
        80.0,
        LayerFunction::Insulation,
        false,
    );
    assert_eq!(
        (l.thickness, l.function, l.core),
        (80.0, LayerFunction::Insulation, false)
    );
    assert!(l.core().core);
    let vorher = sk_model::szo::write(s.model());
    let beton = |t: f64| vec![("Stahlbeton".to_string(), t, LayerFunction::Structure, true)];
    let m = s.model();
    assert_eq!(aufbau(m, nr(&s, "DE-001")), beton(220.0));
    assert_eq!(aufbau(m, nr(&s, "DE-002")), beton(220.0));
    assert_eq!(aufbau(m, nr(&s, "SP-001")), beton(220.0));
    assert_eq!(
        aufbau(m, nr(&s, "FS-001")),
        beton(350.0),
        "Breite der Frostschürze"
    );
    assert_eq!(m.element(nr(&s, "DE-001")).unwrap().layer_set, None);

    // Wandtyp an der Decke: Regel 37
    let de = decke(&s, eg).unwrap();
    let aw = s.model().default_type(TypeCategory::ExteriorWall);
    let schritt = s.undo_label();
    assert!(!typ_setzen(&mut s, de, Some(aw)), "Typart passt nicht");
    assert_eq!(s.undo_label(), schritt);

    let typ = deckentyp(&mut s);
    assert!(typ_setzen(&mut s, de, Some(typ)));
    let m = s.model();
    assert_eq!(
        aufbau(m, de),
        [
            ("Putz".to_string(), 50.0, LayerFunction::Finish, false),
            (
                "Stahlbeton".to_string(),
                220.0,
                LayerFunction::Structure,
                true
            ),
        ]
    );
    assert!(m.check().is_empty(), "{:?}", m.check());
    // Kerndicke steht am Bauteil (variabler Kern)
    assert!(s.edit_model("Deckendicke", |m| m.set_floor_thickness(de, 250.0)));
    let dicken: Vec<f64> = aufbau(s.model(), de).iter().map(|l| l.1).collect();
    assert_eq!(dicken, [50.0, 250.0]);
    assert!(s.undo());

    let text = sk_model::szo::write(s.model());
    let g = guid_text(s.model().layer_set(typ).unwrap().guid);
    assert!(text.lines().any(|l| l.starts_with("[layerset]")
        && l.contains(&format!("guid={g} "))
        && l.contains(" cat=floor ")));
    let zeile = text
        .lines()
        .find(|l| l.starts_with("[floor]") && l.contains("number=\"DE-001\""))
        .unwrap();
    assert!(zeile.contains(&format!(" set={g}")), "{zeile}");
    assert!(zeile.contains(" mat="), "Kernbaustoff bleibt: {zeile}");
    assert_eq!(sk_model::szo::write(&lesen(&text).model), text, "Rundlauf");

    // zurück auf Einschicht: Datei wie vorher (bis auf den Projekttyp)
    assert!(typ_setzen(&mut s, de, None));
    assert_eq!(aufbau(s.model(), de), beton(220.0));
    assert!(s.undo());
    assert!(s.undo());
    assert!(s.undo(), "Typ angelegt");
    assert_eq!(
        sk_model::szo::write(s.model()),
        vorher,
        "bytegleich wie vorher"
    );
}

/// A182 (R4 §2, F-17): Der Leser duldet, was eine neuere Fassung
/// schreibt. Ein `[layerset]` mit unbekanntem `cat=` wird mit Hinweis
/// übersprungen; eine Decke mit `set=` auf diesen Typ fällt auf ihren
/// Baustoff zurück (Einschicht, Hinweis). Ein unbekanntes `fn=` wird
/// „Finish“ mit Hinweis. Die Datei öffnet in jedem Fall, und alles
/// andere ist wie vorher.
#[test]
fn a182_leser_duldet_neue_woerter() {
    let mut s = Scene::with_model(Model::with_seed(182));
    gebaeude(&mut s);
    let text = sk_model::szo::write(s.model());
    let g = "0Zukunft00000000000001";
    let gasbeton = guid_text(
        s.model()
            .material(stoff(s.model(), "Porenbeton"))
            .unwrap()
            .guid,
    );
    let neu = format!(
        "[layerset] guid={g} name=\"Zukunft\" code=\"ZK-1\" cat=zukunft changed=1 note=\"\"\n\
             [layer] set={g} mat={gasbeton} t=100 fn=loadbearing core=1\n"
    );
    let mut t = String::new();
    for l in text.lines() {
        if l.starts_with("[project]") {
            t.push_str(&neu);
        }
        if l.starts_with("[floor]") && l.contains("number=\"DE-001\"") {
            t.push_str(&format!("{l} set={g}\n"));
            continue;
        }
        t.push_str(l);
        t.push('\n');
    }
    let geladen = lesen(&t);
    assert!(!geladen.hints.is_empty(), "Hinweis");
    assert!(
        geladen.hints.iter().any(|h| h.contains("zukunft")),
        "{:?}",
        geladen.hints
    );
    let m = &geladen.model;
    assert_eq!(m.layer_sets().len(), 7, "Typ übersprungen");
    let de = m
        .elements()
        .iter()
        .find(|(_, e)| e.number == "DE-001")
        .map(|(id, _)| id)
        .unwrap();
    assert_eq!(m.element(de).unwrap().layer_set, None);
    assert_eq!(
        aufbau(m, de),
        [(
            "Stahlbeton".to_string(),
            220.0,
            LayerFunction::Structure,
            true
        )]
    );
    assert!(m.check().is_empty(), "{:?}", m.check());

    // fn=zukunft an der WDVS-Schicht des Standardtyps
    let t2 = text.replacen(" fn=insulation ", " fn=zukunft ", 1);
    assert_ne!(t2, text);
    let geladen = lesen(&t2);
    assert!(
        geladen.hints.iter().any(|h| h.contains("zukunft")),
        "{:?}",
        geladen.hints
    );
    let fns: Vec<LayerFunction> = geladen
        .model
        .layer_sets()
        .iter()
        .flat_map(|(_, t)| t.layers.iter().map(|l| l.function))
        .collect();
    assert!(fns.contains(&LayerFunction::Finish));
    assert_eq!(geladen.model.elements().len(), s.model().elements().len());
}
