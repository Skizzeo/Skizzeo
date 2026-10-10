//! Abnahme Flachdach (Jörn 10.10., planung/flachdach/plan-heute.md D1–D5):
//! Ebene „Flachdach“ über dem OG, Aufkantung AK im Wandtyp darunter,
//! Dachaufbau DA mit 20 cm EPS, Attikablech AB mit 4 cm Tropfkante, Mengen
//! samt Automatikmengen `roof.*`, Rückgängig, Datei und Ausblenden über die
//! Szene. Prüfhaus 10 × 8 m, Achsmaß außen, EG und OG je 2,855 m.

use crate::scene::Scene;
use crate::selection;
use crate::ui::{Field, ViewKind};
use sk_math::vec3;
use sk_model::qto::{ElementQto, Schedule};
use sk_model::{Category, ElementId, Guid, Model, RunId, StoreyId};

/// OK Rohdecke OG = UK Flachdach, OK Aufkantung (mm).
const OK_OG: f64 = 5710.0;
const OK_AK: f64 = 6210.0;

/// Prüfhaus mit dem Außenwandtyp `typ` (Guid), Flachdach eingeschaltet
/// über die Szene (Schalter „Flachdach“ im Paneel „Geschosse“).
fn haus(seed: u64, typ: Guid) -> (Scene, RunId) {
    let mut s = Scene::with_model(Model::with_seed(seed));
    let mut eg = None;
    assert!(s.edit_model("Prüfhaus", |m| {
        let t = m.type_by_guid(typ).unwrap();
        m.set_default_type(sk_model::TypeCategory::ExteriorWall, t);
        let b = m.add_building(2);
        let pts = [
            vec3(0.0, 0.0, 0.0),
            vec3(0.0, 8000.0, 0.0),
            vec3(10000.0, 8000.0, 0.0),
            vec3(10000.0, 0.0, 0.0),
        ];
        eg = m.build_from_polygon(b, &pts);
        eg.is_some()
    }));
    let eg = eg.unwrap();
    let og = s.model().runs_above(eg)[0];
    let st = s.model().run(og).unwrap().storey;
    s.set_active_storey(st);
    assert!(s.set_flat_roof(true), "Schalter Flachdach");
    (s, og)
}

fn fd(s: &Scene, og: RunId) -> StoreyId {
    let st = s.model().run(og).unwrap().storey;
    s.model().roof_level(st).expect("Ebene Flachdach")
}

fn auf_ebene(s: &Scene, st: StoreyId, c: Category) -> Vec<ElementId> {
    let mut v: Vec<(String, ElementId)> = s
        .model()
        .elements()
        .iter()
        .filter(|(_, e)| e.storey == st && e.category == c)
        .map(|(id, e)| (e.number.clone(), id))
        .collect();
    v.sort_by(|a, b| a.0.cmp(&b.0));
    v.into_iter().map(|x| x.1).collect()
}

fn da_ab(s: &Scene, og: RunId) -> (ElementId, ElementId) {
    let floor = s.model().floor_of(og).unwrap();
    (
        s.model().flat_roof_of(floor).expect("DA"),
        s.model().coping_of(floor).expect("AB"),
    )
}

fn wert(s: &Scene, id: ElementId, name: &str) -> String {
    let p = selection::props(s, id).expect("Eigenschaften");
    p.values
        .iter()
        .find(|v| v.0 == name)
        .map(|v| v.1.clone())
        .unwrap_or_else(|| panic!("{name} fehlt: {:?}", p.values))
}

fn schedule(s: &mut Scene) -> Schedule {
    s.schedule().clone()
}

/// A330 (D1): Der Schalter legt über dem OG die Ebene „Flachdach“ an (UK =
/// OK Rohdecke OG +5,710, Höhe 0,50 bis OK Aufkantung +6,210). Auf ihr
/// stehen 4 Aufkantungen, der Dachaufbau DA-001 und das Blech AB-001. Im
/// Baum hat die Ebene ihren Ast; blendet man ihre Bauteile aus, ist das
/// Flachdach weg und das Haus darunter bleibt. Ausschalten nimmt alles mit.
#[test]
fn a330_ebene_flachdach_ein_und_ausblenden() {
    let (mut s, og) = haus(320, sk_model::EXTERIOR_TYPE_GUID);
    let st = fd(&s, og);
    let e = s.model().storey(st).unwrap().clone();
    assert_eq!((e.name.as_str(), e.short.as_str()), ("Flachdach", "FD"));
    assert_eq!((e.elevation, e.height), (OK_OG, 500.0));
    assert_eq!(auf_ebene(&s, st, Category::Parapet).len(), 4);
    let (da, ab) = da_ab(&s, og);
    assert_eq!(s.model().element(da).unwrap().number, "DA-001");
    assert_eq!(s.model().element(ab).unwrap().number, "AB-001");
    for id in [da, ab] {
        assert_eq!(s.model().element(id).unwrap().storey, st);
    }
    let t = sk_model::tree::build(s.model());
    assert!(t
        .tab(sk_model::tree::Tab::Tree)
        .iter()
        .any(|n| n.key == sk_model::tree::NodeKey::Storey(e.guid) && n.label == "Flachdach"));
    // Ausblenden der Ebene: alle ihre Bauteile
    let ebene: Vec<ElementId> = s
        .model()
        .elements()
        .iter()
        .filter(|(_, x)| x.storey == st)
        .map(|(id, _)| id)
        .collect();
    assert_eq!(ebene.len(), 6);
    let mut v = s.model().visibility().clone();
    for id in &ebene {
        v.hidden.insert(s.model().element(*id).unwrap().guid);
    }
    s.set_visibility(v);
    for id in &ebene {
        assert!(
            !s.visible(*id),
            "{}",
            s.model().element(*id).unwrap().number
        );
    }
    assert!(
        s.visible(s.model().floor_of(og).unwrap()),
        "OG-Decke bleibt"
    );
    // Ausschalten
    let og_st = s.model().run(og).unwrap().storey;
    s.set_active_storey(og_st);
    assert!(s.set_flat_roof(false));
    assert!(s.model().roof_level(og_st).is_none());
    assert!(s
        .model()
        .flat_roof_of(s.model().floor_of(og).unwrap())
        .is_none());
    assert!(s.model().check().is_empty(), "{:?}", s.model().check());
}

/// A331 (D2): Die Aufkantung ist in allen drei Wandtypen der Außenwandtyp
/// des OG, gekoppelt, ihre Krone liegt auf OK Aufkantung +6,210. Das
/// Gerüst steigt mit (Länge × Höhe wächst um 0,50 m).
#[test]
fn a331_aufkantung_je_wandtyp() {
    for (seed, typ) in [
        (3210, sk_model::EXTERIOR_TYPE_GUID),
        (3211, sk_model::MONO_TYPE_GUID),
        (3212, sk_model::CAVITY_TYPE_GUID),
    ] {
        let (mut s, og) = haus(seed, typ);
        let st = fd(&s, og);
        let t = s.model().type_by_guid(typ);
        let ak = auf_ebene(&s, st, Category::Parapet);
        assert_eq!(ak.len(), 4);
        for id in ak {
            assert_eq!(s.model().element(id).unwrap().layer_set, t);
            assert!(s.model().stack_offset(id).is_some(), "gekoppelt");
            let (_, hi) = s.element_bounds(id).expect("Körper");
            assert!((hi.z - OK_AK).abs() < 1.0, "Krone {}", hi.z);
        }
        assert!(s.model().check().is_empty(), "{:?}", s.model().check());
    }
    // Gerüst: OK Aufkantung + 1 m statt OK Wand OG + 1 m
    let geruest = |s: &mut Scene| {
        schedule(s)
            .auto
            .iter()
            .find(|a| a.key == "site.scaffold")
            .map(|a| a.value)
            .unwrap()
    };
    let (mut s, og) = haus(3213, sk_model::EXTERIOR_TYPE_GUID);
    let mit = geruest(&mut s);
    s.set_active_storey(s.model().run(og).unwrap().storey);
    assert!(s.set_flat_roof(false));
    let ohne = geruest(&mut s);
    assert!(mit > ohne * 1.05, "{mit} > {ohne}");
}

/// A332 (D3): Dachaufbau DA-001 im Werkstyp „Flachdach 21,5“ (Abdichtung
/// 10, EPS 035 200, Dampfsperre 5) zwischen den Aufkantungen: Körper von
/// OK Rohdecke +5,710 bis OK Dachhaut +5,925, innerhalb der Innenfläche
/// der Aufkantung (AW-31,5: 315 mm). Fläche 69,06 m², Volumen der Dämmung
/// 13,811 m³, Anschluss 33,48 m, Anschlusshöhe 28,5 cm. Auch im Schnitt
/// umrissen.
#[test]
fn a332_dachaufbau() {
    let (mut s, og) = haus(322, sk_model::EXTERIOR_TYPE_GUID);
    let (da, _) = da_ab(&s, og);
    let m = s.model();
    let t = m.layer_set(m.flat_roof_type(da).unwrap()).unwrap();
    assert_eq!(t.code, "DA-21,5");
    let names: Vec<String> = t
        .layers
        .iter()
        .map(|l| m.material(l.material).unwrap().name.clone())
        .collect();
    assert_eq!(
        names,
        [
            "Abdichtung Bitumen 2-lagig",
            "Dämmung EPS 035 DAA dh",
            "Dampfsperre Bitumen-Alu"
        ]
    );
    for i in 0..3 {
        let g = m.layer_trade(da, i).and_then(|t| m.trade(t)).unwrap();
        assert_eq!(g.code, "18338", "Schicht {i}");
        assert_eq!(m.layer_kg(da, i), Some(363));
    }
    let (lo, hi) = s.element_bounds(da).expect("Körper");
    assert!((lo.z - OK_OG).abs() < 1.0 && (hi.z - (OK_OG + 215.0)).abs() < 1.0);
    assert!((lo.x - 315.0).abs() < 1.0 && (hi.x - 9685.0).abs() < 1.0);
    assert!((lo.y - 315.0).abs() < 1.0 && (hi.y - 7685.0).abs() < 1.0);
    assert_eq!(wert(&s, da, "Fläche"), "69,06 m²");
    assert_eq!(wert(&s, da, "Anschluss"), "33,48 m");
    assert_eq!(wert(&s, da, "Anschlusshöhe"), "28,5 cm");
    let q = s.flat_roof_qto(da).unwrap();
    assert!((q.insulation_volume / 1e9 - 13.8114).abs() < 1e-3);
    // Schnitt A: Umriss hinter der Ebene von +5,710 bis +5,925
    let mut sect = crate::section::SectionLine::default();
    sect.ensure(&s);
    let h = selection::helpers(
        &s,
        da,
        ViewKind::Section,
        sect.plane(),
        1.0,
        &sk_ui::theme::Theme::dark(),
    );
    assert!(!h.is_empty(), "Umriss im Schnitt");
    let (a, b) = h
        .iter()
        .flat_map(|x| [x.a[2], x.b[2]])
        .fold((f32::MAX, f32::MIN), |(a, b), z| (a.min(z), b.max(z)));
    assert!((a as f64 - OK_OG).abs() < 1.0 && (b as f64 - OK_OG - 215.0).abs() < 1.0);
}

/// A333 (D4): Attikablech AB-001 als Ring auf der Krone der Aufkantung,
/// außen 40 mm vor der Fassade abgekantet: Länge an der Außenkante
/// 36,00 m, Abwicklung 315 + 40 + 50 + 50 = 455 mm, Zuschnitt 500 mm.
/// Löschen lehnt es mit dem Satz zum Flachdach ab.
#[test]
fn a333_attikablech() {
    let (mut s, og) = haus(323, sk_model::EXTERIOR_TYPE_GUID);
    let (da, ab) = da_ab(&s, og);
    let (lo, hi) = s.element_bounds(ab).expect("Körper");
    assert!(
        (lo.x + 40.0).abs() < 1.0 && (hi.x - 10040.0).abs() < 1.0,
        "{lo:?} {hi:?}"
    );
    assert!((lo.z - (OK_AK - 50.0)).abs() < 1.0, "Schenkel {}", lo.z);
    assert!(hi.z > OK_AK && hi.z < OK_AK + 40.0, "{}", hi.z);
    assert_eq!(wert(&s, ab, "Länge"), "36,00 m");
    assert_eq!(wert(&s, ab, "Abwicklung"), "455 mm");
    assert_eq!(wert(&s, ab, "Zuschnitt"), "500 mm");
    let m = s.model();
    assert_eq!(m.layer_kg(ab, 0), Some(363));
    for id in [ab, da] {
        let r = m.can_delete(id).unwrap_err();
        let text = sk_model::refusal_lines(m, id, &r).join(" ");
        assert!(text.contains("Flachdach"), "{text}");
    }
}

/// A334 (D5): Das Mengenfenster führt die Ebene FD mit Aufkantungen,
/// Flachdach und Attikablech; am Flachdach hängen die Automatikmengen
/// `roof.edge` 33,48 m, `roof.corners` 4, `roof.drains` und
/// `roof.overflows` je 1 (69 m² < 150 m²).
#[test]
fn a334_mengen_flachdach() {
    let (mut s, og) = haus(324, sk_model::EXTERIOR_TYPE_GUID);
    let st = fd(&s, og);
    let (da, ab) = da_ab(&s, og);
    let sched = schedule(&mut s);
    let ebene = sched
        .buildings
        .iter()
        .flat_map(|b| &b.storeys)
        .find(|x| x.id == st)
        .expect("Ebene FD im Mengenfenster");
    let arten: Vec<(Category, usize)> = ebene
        .groups
        .iter()
        .map(|g| (g.category, g.rows.len()))
        .collect();
    assert_eq!(
        arten,
        [
            (Category::Parapet, 4),
            (Category::Roof, 1),
            (Category::Coping, 1)
        ]
    );
    let zeile = |id| {
        ebene
            .groups
            .iter()
            .flat_map(|g| &g.rows)
            .find(|r| r.element == id)
            .and_then(|r| r.q.clone())
    };
    match zeile(da) {
        Some(ElementQto::Terrace(t)) => assert!((t.area / 1e6 - 69.0569).abs() < 1e-3),
        q => panic!("DA: {q:?}"),
    }
    match zeile(ab) {
        Some(ElementQto::Coping(c)) => assert!((c.length - 36_000.0).abs() < 1e-6),
        q => panic!("AB: {q:?}"),
    }
    let auto = |k: &str| {
        sched
            .auto
            .iter()
            .find(|a| a.key == k && a.element == da)
            .map(|a| (a.unit, a.value))
    };
    assert_eq!(auto("roof.edge"), Some(("m", 33_480.0)));
    assert_eq!(auto("roof.corners"), Some(("st", 4.0)));
    assert_eq!(auto("roof.drains"), Some(("st", 1.0)));
    assert_eq!(auto("roof.overflows"), Some(("st", 1.0)));
}

/// A335 (D5): Speichern → Öffnen ist bytegleich (`[storey] kind=roof`,
/// `[wall] cat=parapet`, `[roof]`, `[coping]`, `[layerset] cat=roof`);
/// Rückgängig des Schalters nimmt Ebene, Aufkantungen, Dachaufbau, Blech,
/// Werkstyp und Baustoffe zurück, Wiederherstellen bringt dieselben Guids.
#[test]
fn a335_datei_und_rueckgaengig() {
    let (mut s, og) = haus(325, sk_model::EXTERIOR_TYPE_GUID);
    let text = sk_model::szo::write(s.model());
    for w in ["kind=roof", "cat=parapet", "\n[roof] ", "cat=roof"] {
        assert!(text.contains(w), "{w}");
    }
    let l = sk_model::szo::read(&text, sk_model::GuidGen::with_seed(1)).unwrap();
    assert!(l.hints.is_empty(), "{:?}", l.hints);
    assert!(l.model.check().is_empty(), "{:?}", l.model.check());
    assert_eq!(sk_model::szo::write(&l.model), text);
    let (da, ab) = da_ab(&s, og);
    let guids = [da, ab].map(|id| s.model().element(id).unwrap().guid);
    s.undo();
    assert!(s
        .model()
        .roof_level(s.model().run(og).unwrap().storey)
        .is_none());
    assert!(s.model().type_by_guid(sk_model::ROOF_TYPE_GUID).is_none());
    s.redo();
    let (da, ab) = da_ab(&s, og);
    assert_eq!(
        [da, ab].map(|id| s.model().element(id).unwrap().guid),
        guids
    );
    assert!(s.model().check().is_empty(), "{:?}", s.model().check());
}

/// A336 (D3, Paneel „Aufbau“): Das Feld „Dämmung“ zeigt 20 cm und ändert
/// die Dicke im Typ (4 bis 40 cm) in einem Schritt; bei 40 cm bleibt die
/// Aufkantung 8,5 cm über der Dachhaut, das Paneel nennt den Hinweis zur
/// Anschlusshöhe.
#[test]
fn a336_daemmdicke_im_paneel() {
    let (mut s, og) = haus(326, sk_model::EXTERIOR_TYPE_GUID);
    let (da, _) = da_ab(&s, og);
    let p = selection::props(&s, da).unwrap();
    let f = p
        .sections
        .iter()
        .flat_map(|x| &x.fields)
        .find(|f| f.field == Field::RoofInsulation)
        .expect("Feld Dämmung");
    assert_eq!((f.value, f.min, f.max), (200.0, 40.0, 400.0));
    assert!(s.set_field(da, Field::RoofInsulation, 300.0));
    assert_eq!(wert(&s, da, "Anschlusshöhe"), "18,5 cm");
    assert!(selection::props(&s, da).unwrap().notes.is_empty());
    assert!(s.set_field(da, Field::RoofInsulation, 400.0));
    assert_eq!(selection::props(&s, da).unwrap().notes.len(), 1);
    assert!(!s.set_field(da, Field::RoofInsulation, 450.0), "über 40 cm");
    s.undo();
    assert_eq!(wert(&s, da, "Anschlusshöhe"), "18,5 cm");
}

/// Ebene Flachdach im Modell: (Gebäude, Höhe), falls es eine gibt.
fn flachdach_ebene(s: &Scene) -> Option<(Option<sk_model::BuildingId>, f64)> {
    s.model()
        .storeys()
        .iter()
        .find(|(_, st)| st.kind == sk_model::LevelKind::Roof)
        .map(|(_, st)| (st.building, st.height))
}

/// A337 (P8): Der Dialog „Gebäude erstellen“ hat das Flachdach vorgewählt.
/// Feld „Aufkantung Flachdach“ 50 cm (15–150 cm, 0 = kein Flachdach), die
/// Ebene FD steht sofort im Paneel „Geschosse“. Das Rechteck bekommt
/// Aufkantung, Dachaufbau und Blech im Schritt „Gebäude erstellt“;
/// Abbrechen lässt nichts zurück, Rückgängig nimmt alles.
#[test]
fn a337_dialog_flachdach_vorgewaehlt() {
    let mut s = Scene::with_model(Model::with_seed(327));
    let vorher = sk_model::szo::write(s.model());
    s.open_building_dialog();
    assert_eq!(s.building_draft().roof, 500.0);
    let (b, h) = flachdach_ebene(&s).expect("FD vorgewählt");
    assert!(b.is_some());
    assert_eq!(h, 500.0);
    assert!(
        !s.set_building_dialog_value("flachdach", 100.0),
        "unter 15 cm"
    );
    assert!(
        !s.set_building_dialog_value("flachdach", 1600.0),
        "über 150 cm"
    );
    assert!(s.set_building_dialog_value("flachdach", 800.0));
    assert_eq!(flachdach_ebene(&s).map(|x| x.1), Some(800.0));
    assert!(s.set_building_dialog_value("flachdach", 0.0), "0 = keins");
    assert!(flachdach_ebene(&s).is_none());
    assert!(s.set_building_dialog_value("flachdach", 500.0));
    assert!(flachdach_ebene(&s).is_some());
    s.cancel_building();
    assert!(flachdach_ebene(&s).is_none());
    assert_eq!(sk_model::szo::write(s.model()), vorher, "Abbrechen");

    s.open_building_dialog();
    let (z, height) = s.work_plane();
    let set = s.model().defaults().exterior_wall;
    let chain = sk_model::WallChain {
        points: vec![
            vec3(0.0, 0.0, 0.0),
            vec3(0.0, 8000.0, 0.0),
            vec3(10000.0, 8000.0, 0.0),
            vec3(10000.0, 0.0, 0.0),
        ],
        closed: true,
        ref_side: sk_model::RefSide::Left,
        layers: s.model().wall_layers(set),
        base: z,
        height,
        joints: Default::default(),
    };
    let eg = s.add_wall(&chain).expect("Gebäude gezeichnet");
    let og = s.model().runs_above(eg)[0];
    let fd = fd(&s, og);
    assert_eq!(auf_ebene(&s, fd, Category::Parapet).len(), 4);
    let (da, ab) = da_ab(&s, og);
    assert_eq!(wert(&s, da, "Anschlusshöhe"), "28,5 cm");
    assert!(s.model().element(ab).is_some());
    assert!(s.model().check().is_empty(), "{:?}", s.model().check());
    assert!(s.undo());
    assert!(flachdach_ebene(&s).is_none());
    assert_eq!(s.model().runs().len(), 0);
}

/// A338 (Gefälledämmung G2, Paneel „Gefälle“): Das Feld steht auf 0
/// (waagerecht, 1 bis 10 %). 2 % schalten das Gefälle ein, Skizzeo schlägt
/// zwei Abläufe in der Mitte der Langseiten vor; der Keil liegt in der
/// Dämmschicht. Datei hin und zurück gleich, alte Dächer ohne Gefälle
/// schreiben nichts Neues. Rückgängig nimmt beides; 0 schaltet aus und
/// behält die Abläufe für das nächste Einschalten.
#[test]
fn a338_gefaelle_im_paneel() {
    let (mut s, og) = haus(338, sk_model::EXTERIOR_TYPE_GUID);
    let (da, _) = da_ab(&s, og);
    let floor = s.model().floor_of(og).unwrap();
    let ohne = sk_model::szo::write(s.model());
    assert!(!ohne.contains("slope="), "kein Gefälle in alten Dächern");
    let gefaelle = |s: &Scene| {
        selection::props(s, da)
            .unwrap()
            .sections
            .into_iter()
            .find(|x| x.title == "Gefälle")
            .expect("Abschnitt Gefälle")
    };
    let sec = gefaelle(&s);
    let f = &sec.fields[0];
    assert_eq!(f.field, Field::RoofSlope);
    assert_eq!((f.value, f.min, f.max, f.zero), (0.0, 1.0, 10.0, true));
    assert!(sec.button.is_none());
    assert!(s.flat_roof_over(floor).unwrap().slope.is_none());

    assert!(s.set_field(da, Field::RoofSlope, 2.0));
    let d = s.model().drainage_of(floor).unwrap().clone();
    assert_eq!(d.slope, 2.0);
    assert_eq!(d.drains.len(), 2, "{:?}", d.drains);
    let r = s.flat_roof_over(floor).unwrap();
    let g = r.slope.as_ref().expect("Gefälleplan");
    assert_eq!(r.tapered, r.layers.iter().position(|l| l.2));
    // Langseiten waagerecht: beide Abläufe auf halber Länge
    let mitte = (r.outline.iter().map(|p| p.x).fold(f64::MAX, f64::min)
        + r.outline.iter().map(|p| p.x).fold(f64::MIN, f64::max))
        / 2.0;
    assert!(
        g.drains.iter().all(|p| (p.x - mitte).abs() < 1.0),
        "{:?}",
        g.drains
    );
    assert!(g.wedge_max() > 50.0 && g.wedge_mean() < g.wedge_max());
    assert_eq!(wert(&s, da, "Abläufe"), "2 Stück");
    assert!(gefaelle(&s).button.is_some());

    let text = sk_model::szo::write(s.model());
    assert!(text.contains("slope=2 drains="), "{text}");
    let l = sk_model::szo::read(&text, sk_model::GuidGen::with_seed(1)).unwrap();
    assert!(l.hints.is_empty(), "{:?}", l.hints);
    let geladen = l.model.elements().iter().find_map(|(_, e)| match &e.kind {
        sk_model::ElementKind::Roof { drainage, .. } => Some(drainage),
        _ => None,
    });
    assert_eq!(geladen, Some(&d));
    assert_eq!(sk_model::szo::write(&l.model), text);

    s.undo();
    assert_eq!(s.model().drainage_of(floor), Some(&Default::default()));
    assert!(s.flat_roof_over(floor).unwrap().slope.is_none());
    assert_eq!(sk_model::szo::write(s.model()), ohne);
    s.redo();
    assert_eq!(s.model().drainage_of(floor), Some(&d));

    assert!(s.set_field(da, Field::RoofSlope, 0.0));
    assert!(s.flat_roof_over(floor).unwrap().slope.is_none());
    assert_eq!(s.model().drainage_of(floor).unwrap().drains, d.drains);
    assert!(s.set_field(da, Field::RoofSlope, 3.0));
    assert_eq!(s.model().drainage_of(floor).unwrap().drains, d.drains);
    assert!(!s.set_field(da, Field::RoofSlope, 3.0), "gleicher Wert");
    assert!(s.propose_roof_drains(da));
    assert!(s.model().check().is_empty(), "{:?}", s.model().check());
}
