use super::*;

// Abnahmetests A175–A180: eine Tabelle je Bauteilart (Review 2a R1,
// projektstruktur/bauteil-integration.md §4–5, Plan „Kategorisierung und
// Dachterrasse“ Paket 0/1). Spezifikation: test/abnahme-kategorien.md.
//
// Einbau: als `mod bauteilarten { use super::*; … }` ans Ende von
// app/src/abnahme.rs. Nutzt aus abnahme.rs: gebaeude, tool, click, key,
// cam3d, W, H.
//
// A175–A178 sind Sicherungstests für den reinen Umbau: Sie sind auf main
// 637f33c/d57e43f grün (Ausnahme A178, siehe dort) und müssen nach R1
// unverändert grün bleiben. Alle Namen, Präfixe, IFC-Klassen,
// Kostengruppen, Datei-Schlüsselwörter, Mengenzeilen und Löschtexte
// bleiben, wie sie sind. Ändert Paket 1 bewusst etwas daran (KG je
// Schicht in der CSV), passt Test die Erwartung an.
//
// A179–A180 prüfen die Registrierung `sk_model::kinds` (main 2f926dd:
// `kinds::spec(Category) -> &KindSpec`). Die Beispiele für den
// allgemeinen Integrationstest stehen im Test (`beispiel`), weil KindSpec
// kein `sample` hat; A179 verlangt für jede neue Kategorie ein Beispiel.
// Für die Höhenprüfung braucht A180 `Scene::element_bounds`
// (test/patches/element-bounds.patch). Heißen Dinge im Bau anders,
// ändert sich nur der Adapter.

use sk_model::{Category, ElementId, StoreyId};

// ===== Adapter R1 =====

use sk_model::kinds::{self, KindSpec};

/// Alle Einträge der Registrierung, je Kategorie einer.
fn arten() -> Vec<&'static KindSpec> {
    Category::ALL.iter().map(|c| kinds::spec(*c)).collect()
}

/// Eintrag der Registrierung für eine Kategorie.
fn art(c: Category) -> &'static KindSpec {
    kinds::spec(c)
}

/// (Kategorie, Präfix, Name, Mehrzahl, IFC wie `Category::ifc_class`,
/// KG, Wort in der .szo).
#[allow(clippy::type_complexity)]
fn angaben(
    k: &KindSpec,
) -> (
    Category,
    &'static str,
    &'static str,
    &'static str,
    &'static str,
    Option<u16>,
    &'static str,
) {
    (k.category, k.prefix, k.name, k.plural, k.ifc, k.kg, k.szo)
}

/// Höhenlage des Körpers eines Bauteils (UK, OK) in mm; `None`: ohne
/// Körper. `Scene::element_bounds` kommt mit test/patches/element-bounds.patch.
fn koerper_z(s: &mut Scene, id: ElementId) -> Option<(f64, f64)> {
    s.element_bounds(id).map(|(lo, hi)| (lo.z, hi.z))
}

// ===== Hilfen =====

/// Musterhaus A: Dialog, Rechteck 10 × 8 m, AW 31,5 (WDVS 14), OG-Nordwand
/// AW-006 gelöst und 0,30 m vor (UD-001), EG-Innenwand IW 17,5 bei x = 5 m.
/// Mit `mono` danach Wandtyp AW-36,5 für den ganzen Stapel (RD-001 … 008).
fn musterhaus(seed: u64, mono: bool) -> Scene {
    let mut s = Scene::with_model(Model::with_seed(seed));
    let (eg, _og) = gebaeude(&mut s);
    let w = nr(&s, "AW-006");
    assert!(s.edit_model("Kopplung gelöst", |m| m.set_linked(w, false)));
    assert!(s.edit_model("Wand verschoben", |m| m.move_segment(w, 300.0).is_some()));
    let c = cam3d();
    let set = s.model().defaults().interior_wall;
    let mut t = tool(&s);
    t.set_category(Category::InteriorWall, s.model().wall_layers(set));
    click(&mut t, &c, vec3(5000.0, 0.0, 0.0));
    click(&mut t, &c, vec3(5000.0, 8000.0, 0.0));
    let iw = t.handle(&key(Key::Enter), &c, W, H, 1.0).commit.unwrap();
    s.add_wall_as(&iw, Category::InteriorWall).unwrap();
    if mono {
        let t = s.model().type_by_guid(sk_model::MONO_TYPE_GUID).unwrap();
        assert!(s.edit_model("Wandtyp", |m| m.set_run_type(eg, t)));
    }
    assert!(s.model().check().is_empty(), "{:?}", s.model().check());
    s
}

fn nr(s: &Scene, nummer: &str) -> ElementId {
    s.model()
        .elements()
        .iter()
        .find(|(_, e)| e.number == nummer)
        .map(|(id, _)| id)
        .unwrap_or_else(|| panic!("{nummer} fehlt"))
}

fn csv_text(s: &mut Scene) -> String {
    let l = s.schedule().clone();
    String::from_utf8(crate::schedule_view::csv(s.model(), &l)).unwrap()
}

/// Die heutige Tabelle (main 637f33c): Name, Präfix, IFC, KG, IsExternal.
#[allow(clippy::type_complexity)]
const TABELLE: [(Category, &str, &str, &str, Option<u16>, bool); 17] = [
    (
        Category::ExteriorWall,
        "Außenwand",
        "AW",
        "IfcWall",
        Some(330),
        true,
    ),
    (
        Category::InteriorWall,
        "Innenwand",
        "IW",
        "IfcWall",
        Some(340),
        false,
    ),
    (
        Category::Floor,
        "Geschossdecke",
        "DE",
        "IfcSlab.FLOOR",
        Some(350),
        false,
    ),
    (
        Category::GroundSlab,
        "Sohlplatte",
        "SP",
        "IfcSlab.BASESLAB",
        Some(322),
        false,
    ),
    // Flachdach (Jörn 10.10.): Dachbelag KG 363, außen
    (
        Category::Roof,
        "Flachdach",
        "DA",
        "IfcRoof",
        Some(363),
        true,
    ),
    (
        Category::Window,
        "Fenster",
        "FE",
        "IfcWindow",
        Some(330),
        false,
    ),
    (Category::Door, "Tür", "TU", "IfcDoor", Some(340), false),
    (
        Category::Opening,
        "Öffnung",
        "OE",
        "IfcOpeningElement",
        None,
        false,
    ),
    (Category::Space, "Raum", "R", "IfcSpace", None, false),
    (
        Category::StripFooting,
        "Frostschürze",
        "FS",
        "IfcFooting.STRIP_FOOTING",
        Some(322),
        false,
    ),
    (
        Category::EdgeInsulation,
        "Randdämmstreifen",
        "RD",
        "IfcBuildingElementPart.INSULATION",
        Some(330),
        false,
    ),
    (
        Category::SoffitInsulation,
        "Untersichtdämmung",
        "UD",
        "IfcCovering.INSULATION",
        Some(354),
        false,
    ),
    // Paket 2a/2c (bim/paket-dachterrasse.md, Steckbriefe DT und AB)
    (
        Category::RoofTerrace,
        "Dachterrasse",
        "DT",
        "IfcCovering.ROOFING",
        Some(363),
        true,
    ),
    (
        Category::Coping,
        "Attikablech",
        "AB",
        "IfcCovering.COPING",
        Some(363),
        true,
    ),
    // Gelände Thema 4: Perimeterdämmung unter der Sohlplatte
    (
        Category::PerimeterInsulation,
        "Perimeterdämmung",
        "PD",
        "IfcCovering.INSULATION",
        Some(325),
        true,
    ),
    // Flachdach (Jörn 10.10.)
    (
        Category::Parapet,
        "Aufkantung",
        "AK",
        "IfcWall.PARAPET",
        Some(330),
        true,
    ),
    // Regenfallrohr (Jörn 11.10.)
    (
        Category::Downpipe,
        "Regenfallrohr",
        "RF",
        "IfcPipeSegment.RIGIDSEGMENT",
        Some(363),
        true,
    ),
];

/// Bauteilarten, die es heute als Bauteil gibt: (Kategorie, Abschnitt und
/// Schlüsselwort in der .szo, Mehrzahl).
const ARTEN: [(Category, &str, &str, &str); 7] = [
    (Category::ExteriorWall, "[wall]", "exterior", "Außenwände"),
    (Category::InteriorWall, "[wall]", "interior", "Innenwände"),
    (Category::Floor, "[floor]", "floor", "Decken"),
    (Category::GroundSlab, "[slab]", "groundslab", "Sohlplatten"),
    (
        Category::StripFooting,
        "[footing]",
        "stripfooting",
        "Frostschürzen",
    ),
    (
        Category::EdgeInsulation,
        "[strip]",
        "edgeinsulation",
        "Randdämmstreifen",
    ),
    (
        Category::SoffitInsulation,
        "[soffit]",
        "soffitinsulation",
        "Untersichtdämmungen",
    ),
];

/// A175 (R1): Die Angaben je Kategorie bleiben nach dem Umbau gleich:
/// Reihenfolge und Platz in `Category::ALL`, Name, Präfix, IFC-Klasse,
/// Kostengruppe, IsExternal; Typarten nur für AW und IW mit Name und
/// Präfix.
#[test]
fn a175_tabelle_je_bauteilart_unveraendert() {
    assert_eq!(Category::ALL.len(), TABELLE.len());
    for (i, (c, name, prefix, ifc, kg, ext)) in TABELLE.iter().enumerate() {
        assert_eq!(Category::ALL[i], *c, "Reihenfolge");
        assert_eq!(c.index(), i);
        assert_eq!(c.name(), *name, "{c:?}");
        assert_eq!(c.prefix(), *prefix, "{c:?}");
        assert_eq!(c.ifc_class(), *ifc, "{c:?}");
        assert_eq!(c.din276(), *kg, "{c:?}");
        assert_eq!(c.is_external(), *ext, "{c:?}");
    }
    use sk_model::TypeCategory as T;
    for c in Category::ALL {
        let t = T::of(c);
        match c {
            Category::ExteriorWall => assert_eq!(t, Some(T::ExteriorWall)),
            Category::InteriorWall => assert_eq!(t, Some(T::InteriorWall)),
            // R4 öffnet Decke, Sohlplatte und Frostschürze (bim/paket-r4 §1.2)
            Category::Floor | Category::GroundSlab | Category::StripFooting => {}
            // Paket 2a: Werkstyp „Dachterrasse 14“
            Category::RoofTerrace => assert_eq!(t, Some(T::RoofTerrace)),
            // Flachdach: die Aufkantung im Außenwandtyp, der Dachaufbau
            // im Werkstyp „Flachdach 21,5“
            Category::Parapet => assert_eq!(t, Some(T::ExteriorWall)),
            Category::Roof => assert_eq!(t, Some(T::FlatRoof)),
            _ => assert_eq!(t, None, "{c:?} ohne Typ"),
        }
    }
    assert_eq!(
        (T::ExteriorWall.name(), T::ExteriorWall.prefix()),
        ("Außenwand", "AW")
    );
    assert_eq!(
        (T::InteriorWall.name(), T::InteriorWall.prefix()),
        ("Innenwand", "IW")
    );
    assert_eq!(sk_model::type_code(T::ExteriorWall, 315.0), "AW-31,5");
    assert_eq!(sk_model::type_code(T::InteriorWall, 240.0), "IW-24");
}

/// A176 (R1): Jede Bauteilart steht in der .szo im selben Abschnitt mit
/// demselben Schlüsselwort `cat=`, die Typen mit `cat=exterior|interior`.
/// Die Nummern tragen das Präfix der Art. Speichern → Öffnen → Speichern
/// ist bytegleich, für beide Musterhäuser.
#[test]
fn a176_datei_schluesselwoerter_unveraendert() {
    for (seed, mono) in [(176, false), (1760, true)] {
        let s = musterhaus(seed, mono);
        let text = sk_model::szo::write(s.model());
        assert!(text.starts_with("SZO 4\n"), "Version bleibt 4");
        for (c, abschnitt, wort, _) in ARTEN {
            let da = s.model().elements().iter().any(|(_, e)| e.category == c);
            if !da {
                continue;
            }
            let zeilen: Vec<&str> = text
                .lines()
                .filter(|l| l.contains(&format!(" cat={wort} ")))
                .filter(|l| !l.starts_with("[layerset]"))
                .collect();
            assert!(!zeilen.is_empty(), "{c:?}: cat={wort}");
            for z in zeilen {
                assert!(z.starts_with(abschnitt), "{c:?}: {z}");
                assert!(
                    z.contains(&format!("number=\"{}-", c.prefix())),
                    "{c:?}: {z}"
                );
            }
        }
        let typen: Vec<&str> = text
            .lines()
            .filter(|l| l.starts_with("[layerset]"))
            .collect();
        // Die Räume des Hauses entstehen mit Vorlage und Ausbautypen ihrer
        // Nutzung (ohne Wahl Wohnzimmer) und dem Estrich je Geschoss
        // (Raumbuch #112, Einstellungen B1)
        let (waende, ausbau): (Vec<&str>, Vec<&str>) = typen
            .iter()
            .partition(|l| l.contains(" cat=exterior ") || l.contains(" cat=interior "));
        assert_eq!(waende.len(), 7, "Werkstypen");
        let mut arten: Vec<&str> = ausbau
            .iter()
            .filter_map(|l| l.split(" cat=").nth(1)?.split(' ').next())
            .collect();
        arten.sort_unstable();
        assert_eq!(
            arten,
            [
                "ceilingfinish",
                "floorfinish",
                "screed",
                "screed",
                "skirting",
                "wallfinish"
            ],
            "Ausbau der Vorgabe-Nutzung"
        );
        let m2 = sk_model::szo::read(&text, sk_model::GuidGen::with_seed(1))
            .expect("öffnen")
            .model;
        assert_eq!(sk_model::szo::write(&m2), text, "Rundlauf bytegleich");
    }
    // Musterhaus A hat alle Arten außer RD, Musterhaus B alle außer
    // dem WDVS
    let a = musterhaus(176, false);
    let b = musterhaus(1760, true);
    for (c, ..) in ARTEN {
        let in_a = a.model().elements().iter().any(|(_, e)| e.category == c);
        let in_b = b.model().elements().iter().any(|(_, e)| e.category == c);
        assert!(in_a || in_b, "{c:?} kommt in einem Musterhaus vor");
    }
}

/// Mengenliste von Musterhaus A als CSV (main 637f33c), Zeilenende hier LF.
const CSV_A: &str = concat!(
    "\u{feff}",
    r#"Gebäude;Geschoss;Kostengruppe;Bauteil;Nr.;Länge (m);Stück;Fläche (m²);Volumen (m³);Hinweis
GB-01;Fundament;322;Frostschürze;FS-001;34,6000;1;;7,0238;
GB-01;Fundament;322;Sohlplatte 22 cm;SP-001;;1;80,0000;17,6000;
GB-01;Erdgeschoss;330;Außenwände AW 31,5 Porenbeton + WDVS (Summe);AW-001 … 004;36,0000;4;;29,4573;
GB-01;Erdgeschoss;330;Außenwände AW 31,5 Porenbeton + WDVS AW-001, Höhe 2,855 m;AW-001;8,0000;1;;6,6208;
GB-01;Erdgeschoss;330;Außenwände AW 31,5 Porenbeton + WDVS AW-002, Höhe 2,855 m;AW-002;10,0000;1;;7,8731;
GB-01;Erdgeschoss;330;Außenwände AW 31,5 Porenbeton + WDVS AW-003, Höhe 2,855 m;AW-003;8,0000;1;;6,6208;
GB-01;Erdgeschoss;330;Außenwände AW 31,5 Porenbeton + WDVS AW-004, Höhe 2,855 m;AW-004;10,0000;1;;8,3425;
GB-01;Erdgeschoss;330;Außenwände AW 31,5 Porenbeton + WDVS: Dämmung (WDVS);;;;99,3800;13,6960;
GB-01;Erdgeschoss;330;Außenwände AW 31,5 Porenbeton + WDVS: Porenbeton;;;;;15,7613;
GB-01;Erdgeschoss;330;Außenwände AW 31,5 Porenbeton + WDVS: Abzug Deckenauflager;;;;;-1,3159;in der Decke enthalten
GB-01;Erdgeschoss;340;Innenwände IW 17,5 Porenbeton (Summe);IW-001;7,3700;1;;3,3985;
GB-01;Erdgeschoss;340;Innenwände IW 17,5 Porenbeton IW-001, Höhe 2,855 m;IW-001;7,3700;1;;3,3985;
GB-01;Erdgeschoss;340;Innenwände IW 17,5 Porenbeton: Porenbeton;;;;;3,3985;
GB-01;Erdgeschoss;340;Innenwände IW 17,5 Porenbeton: Abzug Deckenstreifen;;;;;-0,2837;in der Decke enthalten
GB-01;Erdgeschoss;350;Decke über EG 22 cm;DE-001;;1;77,9544;17,1500;
GB-01;Erdgeschoss;350;davon Auflager in den Außenwänden;DE-001;;;;1,3159;in der Decke enthalten
GB-01;Erdgeschoss;354;Untersichtdämmung 12 cm;UD-001;;1;2,9160;0,3499;
GB-01;Erdgeschoss;354;Bekleidung 4 cm;UD-001;;;1,5776;0,0631;
GB-01;Erdgeschoss;354;Grundlattung a = 80 cm;UD-001;22,1172;;;;
GB-01;Erdgeschoss;354;Traglattung a = 40 cm;UD-001;24,0892;;;;
GB-01;Erdgeschoss;354;Lüftungsprofil;UD-001;9,7200;;;;
GB-01;Erdgeschoss;;Raum 1 · lichte Höhe 2,465 m;R-001;;1;33,5255;;
GB-01;Erdgeschoss;324;Boden · Parkett Eiche 2 cm;R-001;;;33,5255;0,6705;bis Putz 7,340 × 4,568 (33,53 m²) = 33,53 m²
GB-01;Erdgeschoss;324;Estrich · Zementestrich CT-C25-F4 6,5 cm;R-001;;;33,5255;2,1792;bis Putz 7,340 × 4,568 (33,53 m²) = 33,53 m²
GB-01;Erdgeschoss;324;Estrich · Trittschalldämmung EPS-T 3 cm;R-001;;;33,5255;1,0058;bis Putz 7,340 × 4,568 (33,53 m²) = 33,53 m²
GB-01;Erdgeschoss;324;Estrich · Wärmedämmung EPS 035 DEO 4 cm;R-001;;;33,5255;1,3410;bis Putz 7,340 × 4,568 (33,53 m²) = 33,53 m²
GB-01;Erdgeschoss;325;Estrich · Abdichtung Bitumenbahn, Sohlplatte 0,5 cm;R-001;;;33,5255;0,1676;bis Putz 7,340 × 4,568 (33,53 m²) = 33,53 m²
GB-01;Erdgeschoss;324;Sockelleiste · Sockelleiste Eiche 60 × 15;R-001;23,8150;;;;IW-001: 7,340 (7,34 m) AW-002: 4,568 (4,57 m) AW-001: 7,340 (7,34 m) AW-004: 4,568 (4,57 m) = 23,82 m
GB-01;Erdgeschoss;324;Randdämmstreifen;R-001;23,8150;;;;Umfang bis Putz (23,82 m) = 23,82 m
GB-01;Erdgeschoss;345;Wand IW-001 Ost · Dispersionsanstrich;R-001;;;19,4200;;Länge × Höhe 7,370 × 2,635 (19,42 m²) = 19,42 m²
GB-01;Erdgeschoss;345;Wand IW-001 Ost · Raufaser;R-001;;;19,4200;;Länge × Höhe 7,370 × 2,635 (19,42 m²) = 19,42 m²
GB-01;Erdgeschoss;345;Wand IW-001 Ost · Gipsputz 1,5 cm;R-001;;;19,4200;0,2913;Länge × Höhe 7,370 × 2,635 (19,42 m²) = 19,42 m²
GB-01;Erdgeschoss;336;Wand AW-002 Nord · Dispersionsanstrich;R-001;;;12,1144;;Länge × Höhe 4,598 × 2,635 (12,11 m²) = 12,11 m²
GB-01;Erdgeschoss;336;Wand AW-002 Nord · Raufaser;R-001;;;12,1144;;Länge × Höhe 4,598 × 2,635 (12,11 m²) = 12,11 m²
GB-01;Erdgeschoss;336;Wand AW-002 Nord · Gipsputz 1,5 cm;R-001;;;12,1144;0,1817;Länge × Höhe 4,598 × 2,635 (12,11 m²) = 12,11 m²
GB-01;Erdgeschoss;336;Wand AW-001 West · Dispersionsanstrich;R-001;;;19,4200;;Länge × Höhe 7,370 × 2,635 (19,42 m²) = 19,42 m²
GB-01;Erdgeschoss;336;Wand AW-001 West · Raufaser;R-001;;;19,4200;;Länge × Höhe 7,370 × 2,635 (19,42 m²) = 19,42 m²
GB-01;Erdgeschoss;336;Wand AW-001 West · Gipsputz 1,5 cm;R-001;;;19,4200;0,2913;Länge × Höhe 7,370 × 2,635 (19,42 m²) = 19,42 m²
GB-01;Erdgeschoss;336;Wand AW-004 Süd · Dispersionsanstrich;R-001;;;12,1144;;Länge × Höhe 4,598 × 2,635 (12,11 m²) = 12,11 m²
GB-01;Erdgeschoss;336;Wand AW-004 Süd · Raufaser;R-001;;;12,1144;;Länge × Höhe 4,598 × 2,635 (12,11 m²) = 12,11 m²
GB-01;Erdgeschoss;336;Wand AW-004 Süd · Gipsputz 1,5 cm;R-001;;;12,1144;0,1817;Länge × Höhe 4,598 × 2,635 (12,11 m²) = 12,11 m²
GB-01;Erdgeschoss;354;Decke · Dispersionsanstrich;R-001;;;33,8836;;Rohbaumaß 7,370 × 4,598 (33,88 m²) = 33,88 m²
GB-01;Erdgeschoss;354;Decke · Gipsputz 1 cm;R-001;;;33,8836;0,3388;Rohbaumaß 7,370 × 4,598 (33,88 m²) = 33,88 m²
GB-01;Erdgeschoss;;Raum 3 · lichte Höhe 2,465 m;R-003;;1;33,5255;;
GB-01;Erdgeschoss;324;Boden · Parkett Eiche 2 cm;R-003;;;33,5255;0,6705;bis Putz 4,568 × 7,340 (33,53 m²) = 33,53 m²
GB-01;Erdgeschoss;324;Estrich · Zementestrich CT-C25-F4 6,5 cm;R-003;;;33,5255;2,1792;bis Putz 4,568 × 7,340 (33,53 m²) = 33,53 m²
GB-01;Erdgeschoss;324;Estrich · Trittschalldämmung EPS-T 3 cm;R-003;;;33,5255;1,0058;bis Putz 4,568 × 7,340 (33,53 m²) = 33,53 m²
GB-01;Erdgeschoss;324;Estrich · Wärmedämmung EPS 035 DEO 4 cm;R-003;;;33,5255;1,3410;bis Putz 4,568 × 7,340 (33,53 m²) = 33,53 m²
GB-01;Erdgeschoss;325;Estrich · Abdichtung Bitumenbahn, Sohlplatte 0,5 cm;R-003;;;33,5255;0,1676;bis Putz 4,568 × 7,340 (33,53 m²) = 33,53 m²
GB-01;Erdgeschoss;324;Sockelleiste · Sockelleiste Eiche 60 × 15;R-003;23,8150;;;;AW-004: 4,568 (4,57 m) AW-003: 7,340 (7,34 m) AW-002: 4,568 (4,57 m) IW-001: 7,340 (7,34 m) = 23,82 m
GB-01;Erdgeschoss;324;Randdämmstreifen;R-003;23,8150;;;;Umfang bis Putz (23,82 m) = 23,82 m
GB-01;Erdgeschoss;336;Wand AW-004 Süd · Dispersionsanstrich;R-003;;;12,1144;;Länge × Höhe 4,598 × 2,635 (12,11 m²) = 12,11 m²
GB-01;Erdgeschoss;336;Wand AW-004 Süd · Raufaser;R-003;;;12,1144;;Länge × Höhe 4,598 × 2,635 (12,11 m²) = 12,11 m²
GB-01;Erdgeschoss;336;Wand AW-004 Süd · Gipsputz 1,5 cm;R-003;;;12,1144;0,1817;Länge × Höhe 4,598 × 2,635 (12,11 m²) = 12,11 m²
GB-01;Erdgeschoss;336;Wand AW-003 Ost · Dispersionsanstrich;R-003;;;19,4200;;Länge × Höhe 7,370 × 2,635 (19,42 m²) = 19,42 m²
GB-01;Erdgeschoss;336;Wand AW-003 Ost · Raufaser;R-003;;;19,4200;;Länge × Höhe 7,370 × 2,635 (19,42 m²) = 19,42 m²
GB-01;Erdgeschoss;336;Wand AW-003 Ost · Gipsputz 1,5 cm;R-003;;;19,4200;0,2913;Länge × Höhe 7,370 × 2,635 (19,42 m²) = 19,42 m²
GB-01;Erdgeschoss;336;Wand AW-002 Nord · Dispersionsanstrich;R-003;;;12,1144;;Länge × Höhe 4,598 × 2,635 (12,11 m²) = 12,11 m²
GB-01;Erdgeschoss;336;Wand AW-002 Nord · Raufaser;R-003;;;12,1144;;Länge × Höhe 4,598 × 2,635 (12,11 m²) = 12,11 m²
GB-01;Erdgeschoss;336;Wand AW-002 Nord · Gipsputz 1,5 cm;R-003;;;12,1144;0,1817;Länge × Höhe 4,598 × 2,635 (12,11 m²) = 12,11 m²
GB-01;Erdgeschoss;345;Wand IW-001 West · Dispersionsanstrich;R-003;;;19,4200;;Länge × Höhe 7,370 × 2,635 (19,42 m²) = 19,42 m²
GB-01;Erdgeschoss;345;Wand IW-001 West · Raufaser;R-003;;;19,4200;;Länge × Höhe 7,370 × 2,635 (19,42 m²) = 19,42 m²
GB-01;Erdgeschoss;345;Wand IW-001 West · Gipsputz 1,5 cm;R-003;;;19,4200;0,2913;Länge × Höhe 7,370 × 2,635 (19,42 m²) = 19,42 m²
GB-01;Erdgeschoss;354;Decke · Dispersionsanstrich;R-003;;;33,8836;;Rohbaumaß 4,598 × 7,370 (33,88 m²) = 33,88 m²
GB-01;Erdgeschoss;354;Decke · Gipsputz 1 cm;R-003;;;33,8836;0,3388;Rohbaumaß 4,598 × 7,370 (33,88 m²) = 33,88 m²
GB-01;Obergeschoss;330;Außenwände AW 31,5 Porenbeton + WDVS (Summe);AW-005 … 008;36,6000;4;;31,0376;
GB-01;Obergeschoss;330;Außenwände AW 31,5 Porenbeton + WDVS AW-005, Höhe 2,855 m;AW-005;8,3000;1;;6,8934;
GB-01;Obergeschoss;330;Außenwände AW 31,5 Porenbeton + WDVS AW-006, Höhe 2,855 m;AW-006;10,0000;1;;8,9084;Versatz +0,30 m
GB-01;Obergeschoss;330;Außenwände AW 31,5 Porenbeton + WDVS AW-007, Höhe 2,855 m;AW-007;8,3000;1;;6,8934;
GB-01;Obergeschoss;330;Außenwände AW 31,5 Porenbeton + WDVS AW-008, Höhe 2,855 m;AW-008;10,0000;1;;8,3425;
GB-01;Obergeschoss;330;Außenwände AW 31,5 Porenbeton + WDVS: Dämmung (WDVS);;;;108,7970;14,9997;
GB-01;Obergeschoss;330;Außenwände AW 31,5 Porenbeton + WDVS: Porenbeton;;;;;16,0379;
GB-01;Obergeschoss;330;Außenwände AW 31,5 Porenbeton + WDVS: Abzug Deckenauflager;;;;;-1,3390;in der Decke enthalten
GB-01;Obergeschoss;350;Decke über OG 22 cm;DE-002;;1;77,9544;17,1500;
GB-01;Obergeschoss;350;davon Auflager in den Außenwänden;DE-002;;;;1,3390;in der Decke enthalten
GB-01;Obergeschoss;;Raum 2 · lichte Höhe 2,465 m;R-002;;1;71,3576;;
GB-01;Obergeschoss;353;Boden · Parkett Eiche 2 cm;R-002;;;71,3576;1,4272;bis Putz 7,640 × 9,340 (71,36 m²) = 71,36 m²
GB-01;Obergeschoss;353;Estrich · Zementestrich CT-C25-F4 6,5 cm;R-002;;;71,3576;4,6382;bis Putz 7,640 × 9,340 (71,36 m²) = 71,36 m²
GB-01;Obergeschoss;353;Estrich · Trittschalldämmung EPS-T 3 cm;R-002;;;71,3576;2,1407;bis Putz 7,640 × 9,340 (71,36 m²) = 71,36 m²
GB-01;Obergeschoss;353;Estrich · Wärmedämmung EPS 035 DEO 4,5 cm;R-002;;;71,3576;3,2111;bis Putz 7,640 × 9,340 (71,36 m²) = 71,36 m²
GB-01;Obergeschoss;353;Sockelleiste · Sockelleiste Eiche 60 × 15;R-002;33,9600;;;;AW-007: 7,640 (7,64 m) AW-006: 9,340 (9,34 m) AW-005: 7,640 (7,64 m) AW-008: 9,340 (9,34 m) = 33,96 m
GB-01;Obergeschoss;353;Randdämmstreifen;R-002;33,9600;;;;Umfang bis Putz (33,96 m) = 33,96 m
GB-01;Obergeschoss;336;Wand AW-007 Ost · Dispersionsanstrich;R-002;;;20,2105;;Länge × Höhe 7,670 × 2,635 (20,21 m²) = 20,21 m²
GB-01;Obergeschoss;336;Wand AW-007 Ost · Raufaser;R-002;;;20,2105;;Länge × Höhe 7,670 × 2,635 (20,21 m²) = 20,21 m²
GB-01;Obergeschoss;336;Wand AW-007 Ost · Gipsputz 1,5 cm;R-002;;;20,2105;0,3032;Länge × Höhe 7,670 × 2,635 (20,21 m²) = 20,21 m²
GB-01;Obergeschoss;336;Wand AW-006 Nord · Dispersionsanstrich;R-002;;;24,6900;;Länge × Höhe 9,370 × 2,635 (24,69 m²) = 24,69 m²
GB-01;Obergeschoss;336;Wand AW-006 Nord · Raufaser;R-002;;;24,6900;;Länge × Höhe 9,370 × 2,635 (24,69 m²) = 24,69 m²
GB-01;Obergeschoss;336;Wand AW-006 Nord · Gipsputz 1,5 cm;R-002;;;24,6900;0,3703;Länge × Höhe 9,370 × 2,635 (24,69 m²) = 24,69 m²
GB-01;Obergeschoss;336;Wand AW-005 West · Dispersionsanstrich;R-002;;;20,2105;;Länge × Höhe 7,670 × 2,635 (20,21 m²) = 20,21 m²
GB-01;Obergeschoss;336;Wand AW-005 West · Raufaser;R-002;;;20,2105;;Länge × Höhe 7,670 × 2,635 (20,21 m²) = 20,21 m²
GB-01;Obergeschoss;336;Wand AW-005 West · Gipsputz 1,5 cm;R-002;;;20,2105;0,3032;Länge × Höhe 7,670 × 2,635 (20,21 m²) = 20,21 m²
GB-01;Obergeschoss;336;Wand AW-008 Süd · Dispersionsanstrich;R-002;;;24,6900;;Länge × Höhe 9,370 × 2,635 (24,69 m²) = 24,69 m²
GB-01;Obergeschoss;336;Wand AW-008 Süd · Raufaser;R-002;;;24,6900;;Länge × Höhe 9,370 × 2,635 (24,69 m²) = 24,69 m²
GB-01;Obergeschoss;336;Wand AW-008 Süd · Gipsputz 1,5 cm;R-002;;;24,6900;0,3703;Länge × Höhe 9,370 × 2,635 (24,69 m²) = 24,69 m²
GB-01;Obergeschoss;354;Decke · Dispersionsanstrich;R-002;;;71,8679;;Rohbaumaß 7,670 × 9,370 (71,87 m²) = 71,87 m²
GB-01;Obergeschoss;354;Decke · Gipsputz 1 cm;R-002;;;71,8679;0,7187;Rohbaumaß 7,670 × 9,370 (71,87 m²) = 71,87 m²
GB-01;Summe nach Baustoff;;Stahlbeton;;;;;58,9237;
GB-01;Summe nach Baustoff;;Bekleidung Faserzement;;;;;0,0631;
GB-01;Summe nach Baustoff;;Porenbeton;;;;;35,1977;
GB-01;Summe nach Baustoff;;Dämmung (WDVS);;;;211,0930;29,0457;
GB-01;Summe nach Baustoff;;Trittschalldämmung EPS-T;;;;138,4085;4,1523;
GB-01;Summe nach Baustoff;;Wärmedämmung EPS 035 DEO;;;;138,4085;5,8931;
GB-01;Summe nach Baustoff;;Gipsputz;;;;;4,6354;
GB-01;Summe nach Baustoff;;Abdichtung Bitumenbahn, Sohlplatte;;;;;0,3353;
GB-01;Summe nach Baustoff;;Zementestrich CT-C25-F4;;;;;8,9966;
GB-01;Summe nach Baustoff;;Parkett Eiche;;;;;2,7682;
"#
);
/// Mengenliste von Musterhaus B als CSV (main 637f33c), Zeilenende hier LF.
const CSV_B: &str = concat!(
    "\u{feff}",
    r#"Gebäude;Geschoss;Kostengruppe;Bauteil;Nr.;Länge (m);Stück;Fläche (m²);Volumen (m³);Hinweis
GB-01;Fundament;322;Frostschürze;FS-001;34,6000;1;;7,0238;
GB-01;Fundament;322;Sohlplatte 22 cm;SP-001;;1;80,0000;17,6000;
GB-01;Erdgeschoss;330;Außenwände AW monolithisch 36,5 (Summe);AW-001 … 004;36,0000;4;;33,2197;
GB-01;Erdgeschoss;330;Außenwände AW monolithisch 36,5 AW-001, Höhe 2,855 m;AW-001;8,0000;1;;7,3432;
GB-01;Erdgeschoss;330;Außenwände AW monolithisch 36,5 AW-002, Höhe 2,855 m;AW-002;10,0000;1;;9,2667;
GB-01;Erdgeschoss;330;Außenwände AW monolithisch 36,5 AW-003, Höhe 2,855 m;AW-003;8,0000;1;;7,3432;
GB-01;Erdgeschoss;330;Außenwände AW monolithisch 36,5 AW-004, Höhe 2,855 m;AW-004;10,0000;1;;9,2667;
GB-01;Erdgeschoss;330;Außenwände AW monolithisch 36,5: Porenbeton;;;;;33,2197;
GB-01;Erdgeschoss;330;Außenwände AW monolithisch 36,5: Abzug Deckenauflager;;;;;-2,7736;in der Decke enthalten
GB-01;Erdgeschoss;330;Randdämmstreifen 12,5 cm × 22 cm;RD-001;8,1750;1;;0,2248;
GB-01;Erdgeschoss;330;Randdämmstreifen 12,5 cm × 22 cm;RD-002;9,8750;1;;0,2716;
GB-01;Erdgeschoss;330;Randdämmstreifen 12,5 cm × 22 cm;RD-003;8,1750;1;;0,2248;
GB-01;Erdgeschoss;330;Randdämmstreifen 12,5 cm × 22 cm;RD-004;9,8750;1;;0,2716;
GB-01;Erdgeschoss;340;Innenwände IW 17,5 Porenbeton (Summe);IW-001;7,2700;1;;3,3524;
GB-01;Erdgeschoss;340;Innenwände IW 17,5 Porenbeton IW-001, Höhe 2,855 m;IW-001;7,2700;1;;3,3524;
GB-01;Erdgeschoss;340;Innenwände IW 17,5 Porenbeton: Porenbeton;;;;;3,3524;
GB-01;Erdgeschoss;340;Innenwände IW 17,5 Porenbeton: Abzug Deckenstreifen;;;;;-0,2799;in der Decke enthalten
GB-01;Erdgeschoss;350;Decke über EG 22 cm;DE-001;;1;78,4875;17,2673;
GB-01;Erdgeschoss;350;davon Auflager in den Außenwänden;DE-001;;;;2,7736;in der Decke enthalten
GB-01;Erdgeschoss;354;Untersichtdämmung 12 cm;UD-001;;1;3,0000;0,3600;
GB-01;Erdgeschoss;354;Bekleidung 4 cm;UD-001;;;3,0000;0,1200;
GB-01;Erdgeschoss;354;Grundlattung a = 80 cm;UD-001;24,3500;;;;
GB-01;Erdgeschoss;354;Traglattung a = 40 cm;UD-001;28,1000;;;;
GB-01;Erdgeschoss;354;Lüftungsprofil;UD-001;10,0000;;;;
GB-01;Erdgeschoss;;Raum 1 · lichte Höhe 2,465 m;R-001;;1;32,7067;;
GB-01;Erdgeschoss;324;Boden · Parkett Eiche 2 cm;R-001;;;32,7067;0,6541;bis Putz 7,240 × 4,518 (32,71 m²) = 32,71 m²
GB-01;Erdgeschoss;324;Estrich · Zementestrich CT-C25-F4 6,5 cm;R-001;;;32,7067;2,1259;bis Putz 7,240 × 4,518 (32,71 m²) = 32,71 m²
GB-01;Erdgeschoss;324;Estrich · Trittschalldämmung EPS-T 3 cm;R-001;;;32,7067;0,9812;bis Putz 7,240 × 4,518 (32,71 m²) = 32,71 m²
GB-01;Erdgeschoss;324;Estrich · Wärmedämmung EPS 035 DEO 4 cm;R-001;;;32,7067;1,3083;bis Putz 7,240 × 4,518 (32,71 m²) = 32,71 m²
GB-01;Erdgeschoss;325;Estrich · Abdichtung Bitumenbahn, Sohlplatte 0,5 cm;R-001;;;32,7067;0,1635;bis Putz 7,240 × 4,518 (32,71 m²) = 32,71 m²
GB-01;Erdgeschoss;324;Sockelleiste · Sockelleiste Eiche 60 × 15;R-001;23,5150;;;;IW-001: 7,240 (7,24 m) AW-002: 4,518 (4,52 m) AW-001: 7,240 (7,24 m) AW-004: 4,518 (4,52 m) = 23,52 m
GB-01;Erdgeschoss;324;Randdämmstreifen;R-001;23,5150;;;;Umfang bis Putz (23,52 m) = 23,52 m
GB-01;Erdgeschoss;345;Wand IW-001 Ost · Dispersionsanstrich;R-001;;;19,1565;;Länge × Höhe 7,270 × 2,635 (19,16 m²) = 19,16 m²
GB-01;Erdgeschoss;345;Wand IW-001 Ost · Raufaser;R-001;;;19,1565;;Länge × Höhe 7,270 × 2,635 (19,16 m²) = 19,16 m²
GB-01;Erdgeschoss;345;Wand IW-001 Ost · Gipsputz 1,5 cm;R-001;;;19,1565;0,2873;Länge × Höhe 7,270 × 2,635 (19,16 m²) = 19,16 m²
GB-01;Erdgeschoss;336;Wand AW-002 Nord · Dispersionsanstrich;R-001;;;11,9827;;Länge × Höhe 4,548 × 2,635 (11,98 m²) = 11,98 m²
GB-01;Erdgeschoss;336;Wand AW-002 Nord · Raufaser;R-001;;;11,9827;;Länge × Höhe 4,548 × 2,635 (11,98 m²) = 11,98 m²
GB-01;Erdgeschoss;336;Wand AW-002 Nord · Gipsputz 1,5 cm;R-001;;;11,9827;0,1797;Länge × Höhe 4,548 × 2,635 (11,98 m²) = 11,98 m²
GB-01;Erdgeschoss;336;Wand AW-001 West · Dispersionsanstrich;R-001;;;19,1565;;Länge × Höhe 7,270 × 2,635 (19,16 m²) = 19,16 m²
GB-01;Erdgeschoss;336;Wand AW-001 West · Raufaser;R-001;;;19,1565;;Länge × Höhe 7,270 × 2,635 (19,16 m²) = 19,16 m²
GB-01;Erdgeschoss;336;Wand AW-001 West · Gipsputz 1,5 cm;R-001;;;19,1565;0,2873;Länge × Höhe 7,270 × 2,635 (19,16 m²) = 19,16 m²
GB-01;Erdgeschoss;336;Wand AW-004 Süd · Dispersionsanstrich;R-001;;;11,9827;;Länge × Höhe 4,548 × 2,635 (11,98 m²) = 11,98 m²
GB-01;Erdgeschoss;336;Wand AW-004 Süd · Raufaser;R-001;;;11,9827;;Länge × Höhe 4,548 × 2,635 (11,98 m²) = 11,98 m²
GB-01;Erdgeschoss;336;Wand AW-004 Süd · Gipsputz 1,5 cm;R-001;;;11,9827;0,1797;Länge × Höhe 4,548 × 2,635 (11,98 m²) = 11,98 m²
GB-01;Erdgeschoss;354;Decke · Dispersionsanstrich;R-001;;;33,0603;;Rohbaumaß 7,270 × 4,548 (33,06 m²) = 33,06 m²
GB-01;Erdgeschoss;354;Decke · Gipsputz 1 cm;R-001;;;33,0603;0,3306;Rohbaumaß 7,270 × 4,548 (33,06 m²) = 33,06 m²
GB-01;Erdgeschoss;;Raum 3 · lichte Höhe 2,465 m;R-003;;1;32,7067;;
GB-01;Erdgeschoss;324;Boden · Parkett Eiche 2 cm;R-003;;;32,7067;0,6541;bis Putz 4,518 × 7,240 (32,71 m²) = 32,71 m²
GB-01;Erdgeschoss;324;Estrich · Zementestrich CT-C25-F4 6,5 cm;R-003;;;32,7067;2,1259;bis Putz 4,518 × 7,240 (32,71 m²) = 32,71 m²
GB-01;Erdgeschoss;324;Estrich · Trittschalldämmung EPS-T 3 cm;R-003;;;32,7067;0,9812;bis Putz 4,518 × 7,240 (32,71 m²) = 32,71 m²
GB-01;Erdgeschoss;324;Estrich · Wärmedämmung EPS 035 DEO 4 cm;R-003;;;32,7067;1,3083;bis Putz 4,518 × 7,240 (32,71 m²) = 32,71 m²
GB-01;Erdgeschoss;325;Estrich · Abdichtung Bitumenbahn, Sohlplatte 0,5 cm;R-003;;;32,7067;0,1635;bis Putz 4,518 × 7,240 (32,71 m²) = 32,71 m²
GB-01;Erdgeschoss;324;Sockelleiste · Sockelleiste Eiche 60 × 15;R-003;23,5150;;;;AW-004: 4,518 (4,52 m) AW-003: 7,240 (7,24 m) AW-002: 4,518 (4,52 m) IW-001: 7,240 (7,24 m) = 23,52 m
GB-01;Erdgeschoss;324;Randdämmstreifen;R-003;23,5150;;;;Umfang bis Putz (23,52 m) = 23,52 m
GB-01;Erdgeschoss;336;Wand AW-004 Süd · Dispersionsanstrich;R-003;;;11,9827;;Länge × Höhe 4,548 × 2,635 (11,98 m²) = 11,98 m²
GB-01;Erdgeschoss;336;Wand AW-004 Süd · Raufaser;R-003;;;11,9827;;Länge × Höhe 4,548 × 2,635 (11,98 m²) = 11,98 m²
GB-01;Erdgeschoss;336;Wand AW-004 Süd · Gipsputz 1,5 cm;R-003;;;11,9827;0,1797;Länge × Höhe 4,548 × 2,635 (11,98 m²) = 11,98 m²
GB-01;Erdgeschoss;336;Wand AW-003 Ost · Dispersionsanstrich;R-003;;;19,1565;;Länge × Höhe 7,270 × 2,635 (19,16 m²) = 19,16 m²
GB-01;Erdgeschoss;336;Wand AW-003 Ost · Raufaser;R-003;;;19,1565;;Länge × Höhe 7,270 × 2,635 (19,16 m²) = 19,16 m²
GB-01;Erdgeschoss;336;Wand AW-003 Ost · Gipsputz 1,5 cm;R-003;;;19,1565;0,2873;Länge × Höhe 7,270 × 2,635 (19,16 m²) = 19,16 m²
GB-01;Erdgeschoss;336;Wand AW-002 Nord · Dispersionsanstrich;R-003;;;11,9827;;Länge × Höhe 4,548 × 2,635 (11,98 m²) = 11,98 m²
GB-01;Erdgeschoss;336;Wand AW-002 Nord · Raufaser;R-003;;;11,9827;;Länge × Höhe 4,548 × 2,635 (11,98 m²) = 11,98 m²
GB-01;Erdgeschoss;336;Wand AW-002 Nord · Gipsputz 1,5 cm;R-003;;;11,9827;0,1797;Länge × Höhe 4,548 × 2,635 (11,98 m²) = 11,98 m²
GB-01;Erdgeschoss;345;Wand IW-001 West · Dispersionsanstrich;R-003;;;19,1565;;Länge × Höhe 7,270 × 2,635 (19,16 m²) = 19,16 m²
GB-01;Erdgeschoss;345;Wand IW-001 West · Raufaser;R-003;;;19,1565;;Länge × Höhe 7,270 × 2,635 (19,16 m²) = 19,16 m²
GB-01;Erdgeschoss;345;Wand IW-001 West · Gipsputz 1,5 cm;R-003;;;19,1565;0,2873;Länge × Höhe 7,270 × 2,635 (19,16 m²) = 19,16 m²
GB-01;Erdgeschoss;354;Decke · Dispersionsanstrich;R-003;;;33,0603;;Rohbaumaß 4,548 × 7,270 (33,06 m²) = 33,06 m²
GB-01;Erdgeschoss;354;Decke · Gipsputz 1 cm;R-003;;;33,0603;0,3306;Rohbaumaß 4,548 × 7,270 (33,06 m²) = 33,06 m²
GB-01;Obergeschoss;330;Außenwände AW monolithisch 36,5 (Summe);AW-005 … 008;36,6000;4;;33,7968;
GB-01;Obergeschoss;330;Außenwände AW monolithisch 36,5 AW-005, Höhe 2,855 m;AW-005;8,3000;1;;7,6317;
GB-01;Obergeschoss;330;Außenwände AW monolithisch 36,5 AW-006, Höhe 2,855 m;AW-006;10,0000;1;;9,2667;Versatz +0,30 m
GB-01;Obergeschoss;330;Außenwände AW monolithisch 36,5 AW-007, Höhe 2,855 m;AW-007;8,3000;1;;7,6317;
GB-01;Obergeschoss;330;Außenwände AW monolithisch 36,5 AW-008, Höhe 2,855 m;AW-008;10,0000;1;;9,2667;
GB-01;Obergeschoss;330;Außenwände AW monolithisch 36,5: Porenbeton;;;;;33,7968;
GB-01;Obergeschoss;330;Außenwände AW monolithisch 36,5: Abzug Deckenauflager;;;;;-2,8217;in der Decke enthalten
GB-01;Obergeschoss;330;Randdämmstreifen 12,5 cm × 22 cm;RD-005;8,1750;1;;0,2248;
GB-01;Obergeschoss;330;Randdämmstreifen 12,5 cm × 22 cm;RD-006;9,8750;1;;0,2716;
GB-01;Obergeschoss;330;Randdämmstreifen 12,5 cm × 22 cm;RD-007;8,1750;1;;0,2248;
GB-01;Obergeschoss;330;Randdämmstreifen 12,5 cm × 22 cm;RD-008;9,8750;1;;0,2716;
GB-01;Obergeschoss;350;Decke über OG 22 cm;DE-002;;1;78,4875;17,2673;
GB-01;Obergeschoss;350;davon Auflager in den Außenwänden;DE-002;;;;2,8217;in der Decke enthalten
GB-01;Obergeschoss;;Raum 2 · lichte Höhe 2,465 m;R-002;;1;69,6696;;
GB-01;Obergeschoss;353;Boden · Parkett Eiche 2 cm;R-002;;;69,6696;1,3934;bis Putz 7,540 × 9,240 (69,67 m²) = 69,67 m²
GB-01;Obergeschoss;353;Estrich · Zementestrich CT-C25-F4 6,5 cm;R-002;;;69,6696;4,5285;bis Putz 7,540 × 9,240 (69,67 m²) = 69,67 m²
GB-01;Obergeschoss;353;Estrich · Trittschalldämmung EPS-T 3 cm;R-002;;;69,6696;2,0901;bis Putz 7,540 × 9,240 (69,67 m²) = 69,67 m²
GB-01;Obergeschoss;353;Estrich · Wärmedämmung EPS 035 DEO 4,5 cm;R-002;;;69,6696;3,1351;bis Putz 7,540 × 9,240 (69,67 m²) = 69,67 m²
GB-01;Obergeschoss;353;Sockelleiste · Sockelleiste Eiche 60 × 15;R-002;33,5600;;;;AW-007: 7,540 (7,54 m) AW-006: 9,240 (9,24 m) AW-005: 7,540 (7,54 m) AW-008: 9,240 (9,24 m) = 33,56 m
GB-01;Obergeschoss;353;Randdämmstreifen;R-002;33,5600;;;;Umfang bis Putz (33,56 m) = 33,56 m
GB-01;Obergeschoss;336;Wand AW-007 Ost · Dispersionsanstrich;R-002;;;19,9470;;Länge × Höhe 7,570 × 2,635 (19,95 m²) = 19,95 m²
GB-01;Obergeschoss;336;Wand AW-007 Ost · Raufaser;R-002;;;19,9470;;Länge × Höhe 7,570 × 2,635 (19,95 m²) = 19,95 m²
GB-01;Obergeschoss;336;Wand AW-007 Ost · Gipsputz 1,5 cm;R-002;;;19,9470;0,2992;Länge × Höhe 7,570 × 2,635 (19,95 m²) = 19,95 m²
GB-01;Obergeschoss;336;Wand AW-006 Nord · Dispersionsanstrich;R-002;;;24,4265;;Länge × Höhe 9,270 × 2,635 (24,43 m²) = 24,43 m²
GB-01;Obergeschoss;336;Wand AW-006 Nord · Raufaser;R-002;;;24,4265;;Länge × Höhe 9,270 × 2,635 (24,43 m²) = 24,43 m²
GB-01;Obergeschoss;336;Wand AW-006 Nord · Gipsputz 1,5 cm;R-002;;;24,4265;0,3664;Länge × Höhe 9,270 × 2,635 (24,43 m²) = 24,43 m²
GB-01;Obergeschoss;336;Wand AW-005 West · Dispersionsanstrich;R-002;;;19,9470;;Länge × Höhe 7,570 × 2,635 (19,95 m²) = 19,95 m²
GB-01;Obergeschoss;336;Wand AW-005 West · Raufaser;R-002;;;19,9470;;Länge × Höhe 7,570 × 2,635 (19,95 m²) = 19,95 m²
GB-01;Obergeschoss;336;Wand AW-005 West · Gipsputz 1,5 cm;R-002;;;19,9470;0,2992;Länge × Höhe 7,570 × 2,635 (19,95 m²) = 19,95 m²
GB-01;Obergeschoss;336;Wand AW-008 Süd · Dispersionsanstrich;R-002;;;24,4265;;Länge × Höhe 9,270 × 2,635 (24,43 m²) = 24,43 m²
GB-01;Obergeschoss;336;Wand AW-008 Süd · Raufaser;R-002;;;24,4265;;Länge × Höhe 9,270 × 2,635 (24,43 m²) = 24,43 m²
GB-01;Obergeschoss;336;Wand AW-008 Süd · Gipsputz 1,5 cm;R-002;;;24,4265;0,3664;Länge × Höhe 9,270 × 2,635 (24,43 m²) = 24,43 m²
GB-01;Obergeschoss;354;Decke · Dispersionsanstrich;R-002;;;70,1739;;Rohbaumaß 7,570 × 9,270 (70,17 m²) = 70,17 m²
GB-01;Obergeschoss;354;Decke · Gipsputz 1 cm;R-002;;;70,1739;0,7017;Rohbaumaß 7,570 × 9,270 (70,17 m²) = 70,17 m²
GB-01;Summe nach Baustoff;;Stahlbeton;;;;;59,1583;
GB-01;Summe nach Baustoff;;Bekleidung Faserzement;;;;;0,1200;
GB-01;Summe nach Baustoff;;Porenbeton;;;;;70,3689;
GB-01;Summe nach Baustoff;;Randdämmung;;;;3,0000;2,3455;
GB-01;Summe nach Baustoff;;Trittschalldämmung EPS-T;;;;135,0830;4,0525;
GB-01;Summe nach Baustoff;;Wärmedämmung EPS 035 DEO;;;;135,0830;5,7517;
GB-01;Summe nach Baustoff;;Gipsputz;;;;;4,5625;
GB-01;Summe nach Baustoff;;Abdichtung Bitumenbahn, Sohlplatte;;;;;0,3271;
GB-01;Summe nach Baustoff;;Zementestrich CT-C25-F4;;;;;8,7804;
GB-01;Summe nach Baustoff;;Parkett Eiche;;;;;2,7017;
"#
);

/// A177 (R1): Das Mengenfenster ist Zeile für Zeile gleich:
/// Gruppenreihenfolge nach Bauablauf (FS, SP, AW, RD, IW, DE, UD),
/// Gruppentitel, Kostengruppen, Mengen und Summen nach Baustoff. Seit
/// W3 (10.10.) stehen unter der UD Bekleidung, Lattung und
/// Lüftungsprofil. Seit Raumbuch RB5 stehen die Räume (G3, je mit dem
/// Raumbuch ihrer Vorlage Wohnzimmer) mit ihren Zeilen je Geschoss nach
/// den Bauteilen, ihre Baustoffe in der Summe; die vierte Stelle ist
/// kaufmännisch gerundet (Decke 78,4875 × 0,22 = 17,26725 → 17,2673).
#[test]
fn a177_mengenliste_unveraendert() {
    for (seed, mono, soll) in [(176, false, CSV_A), (1760, true, CSV_B)] {
        let mut s = musterhaus(seed, mono);
        let csv = csv_text(&mut s);
        assert!(!csv.replace("\r\n", "").contains('\n'), "Zeilenende CRLF");
        assert_eq!(csv.replace("\r\n", "\n"), soll, "Musterhaus {seed}");
    }
}

/// A178 (R1): Die Sätze beim Löschen bleiben, mit einer gewollten
/// Änderung: Die Rückfrage „Gebäude löschen?“ nennt jede Bauteilart mit
/// Namen. Auf 637f33c steht dort für die UD „1 weitere“, weil die Liste
/// in delete.rs die UD nicht kennt; aus der Tabelle heraus heißt es
/// „1 Untersichtdämmung“. Die Ablehnungen am Bauteil und der Satz bei
/// gemischter Auswahl bleiben wörtlich.
#[test]
fn a178_loeschtexte_aus_der_tabelle() {
    let s = musterhaus(178, true);
    let b = s.model().buildings().iter().next().unwrap().0;
    assert_eq!(
        crate::delete::parts_text(s.model(), b),
        "Alle Bauteile dieses Gebäudes werden entfernt: 8 Außenwände, \
             1 Innenwand, 2 Decken, Sohlplatte, Frostschürze, \
             8 Randdämmstreifen, 1 Untersichtdämmung."
    );
    let ablehnung = |n: &str| {
        let id = nr(&s, n);
        s.model()
            .can_delete(id)
            .err()
            .map(|r| sk_model::refusal_text(s.model(), id, &r))
    };
    let aw = "Außenwände gehören zum Gebäudeumriss. Zum Entfernen den \
                  Umriss ändern oder das ganze Gebäude löschen.";
    let gruendung = "Sohlplatte und Frostschürze folgen dem Umriss des Erdgeschosses.";
    let de = "Die Decke ergibt sich aus dem Gebäudeumriss und bleibt, \
                  solange das Gebäude steht.";
    let rd = "Der Randdämmstreifen gehört zum Wandtyp; zum Entfernen den Wandtyp ändern.";
    let ud = "Die Untersichtdämmung folgt dem Vorsprung des Geschosses \
                  darüber; ihre Dicke steht bei der Decke.";
    for (n, satz) in [
        ("AW-001", Some(aw)),
        ("AW-006", Some(aw)),
        ("SP-001", Some(gruendung)),
        ("FS-001", Some(gruendung)),
        ("DE-001", Some(de)),
        ("DE-002", Some(de)),
        ("RD-001", Some(rd)),
        ("UD-001", Some(ud)),
        ("IW-001", None),
    ] {
        assert_eq!(ablehnung(n).as_deref(), satz, "{n}");
    }
    // Alles ausgewählt, Entf: nur die Innenwand geht
    let mut s2 = Scene::with_model(s.model().clone());
    let ids: Vec<ElementId> = s2.model().elements().iter().map(|(id, _)| id).collect();
    let d = s2.delete_elements(&ids);
    assert_eq!(
        crate::delete::hint(s2.model(), &d),
        [
            "1 Wand gelöscht.",
            "Außenwände, Decken, Sohlplatten, Frostschürzen und \
                 Randdämmstreifen bleiben, sie gehören zum Gebäudeumriss \
                 oder zum Wandtyp."
        ]
    );
}

/// Seit Paket 2a/2c: Dachterrasse und Attikablech (nicht in den
/// Musterhäusern von A176, die den Stand 637f33c festhalten).
const ARTEN_2A: [(Category, &str, &str, &str); 6] = [
    (
        Category::RoofTerrace,
        "[terrace]",
        "roofterrace",
        "Dachterrassen",
    ),
    (Category::Coping, "[coping]", "coping", "Attikableche"),
    // Gelände Thema 4
    (
        Category::PerimeterInsulation,
        "[perimeter]",
        "perimeterinsulation",
        "Perimeterdämmungen",
    ),
    // Flachdach (Jörn 10.10.)
    (Category::Parapet, "[wall]", "parapet", "Aufkantungen"),
    (Category::Roof, "[roof]", "roof", "Flachdächer"),
    // Regenfallrohr (Jörn 11.10.)
    (
        Category::Downpipe,
        "[downpipe]",
        "downpipe",
        "Regenfallrohre",
    ),
];

/// Kategorien, die es noch nicht als Bauteil gibt (kein Beispiel).
const OHNE_BAUTEIL: [Category; 4] = [
    Category::Window,
    Category::Door,
    Category::Opening,
    Category::Space,
];

/// A179 (R1): Die Registrierung `kinds::spec` hat für jede Kategorie
/// einen Eintrag mit denselben Angaben wie A175/A176: Kategorie, Präfix,
/// Name, Mehrzahl, IFC, KG und Wort in der Datei. Präfixe und Dateiwörter
/// sind eindeutig. Jede Kategorie mit Bauteilen hat ein Beispiel für
/// A180; kommt eine neue Kategorie dazu (DT, AB), schlägt dieser Test
/// fehl, bis A180 ein Beispiel für sie hat.
#[test]
fn a179_registrierung_je_bauteilart() {
    let alle = arten();
    assert_eq!(alle.len(), Category::ALL.len());
    for (k, c) in alle.iter().zip(Category::ALL) {
        let (kc, prefix, name, _, ifc, kg, _) = angaben(k);
        assert_eq!(kc, c, "Reihenfolge wie Category::ALL");
        assert_eq!(
            (prefix, name, ifc, kg),
            (c.prefix(), c.name(), c.ifc_class(), c.din276())
        );
        assert_eq!(
            alle.iter().filter(|a| a.prefix == prefix).count(),
            1,
            "Präfix {prefix} eindeutig"
        );
        assert_eq!(
            alle.iter().filter(|a| a.szo == k.szo).count(),
            1,
            "Dateiwort {} eindeutig",
            k.szo
        );
        assert!(
            OHNE_BAUTEIL.contains(&c) || ARTEN.iter().chain(&ARTEN_2A).any(|a| a.0 == c),
            "{c:?}: Beispiel in A180 fehlt"
        );
    }
    for (c, _, wort, mehrzahl) in ARTEN.into_iter().chain(ARTEN_2A) {
        let (.., plural, _, _, szo) = angaben(art(c));
        assert_eq!(plural, mehrzahl, "{c:?}");
        assert_eq!(szo, wort, "{c:?}");
    }
}

/// Zahl der Bauteile je Kategorie.
fn zahl(s: &Scene, c: Category) -> usize {
    s.model()
        .elements()
        .iter()
        .filter(|(_, e)| e.category == c)
        .count()
}

fn geschoss(s: &Scene, short: &str) -> StoreyId {
    s.model()
        .storeys()
        .iter()
        .find(|(_, st)| st.short == short)
        .map(|(id, _)| id)
        .unwrap()
}

/// Erstes Bauteil der Kategorie nach Nummer.
fn erstes(s: &Scene, c: Category) -> Option<ElementId> {
    let m = s.model();
    let mut v: Vec<(String, ElementId)> = m
        .elements()
        .iter()
        .filter(|(_, e)| e.category == c)
        .map(|(id, e)| (e.number.clone(), id))
        .collect();
    v.sort_by(|a, b| a.0.cmp(&b.0));
    v.first().map(|x| x.1)
}

/// Beispiel der Kategorie `c` in einem leeren Projekt, in einem Schritt
/// „Beispiel“: das Haus 10 × 8 m direkt im Modell, für IW eine
/// Innenwand dazu, für RD der Typ AW-36,5, für UD AW-006 0,30 m vor,
/// für DT und AB AW-006 1,50 m zurück.
/// Abgeleitete Bauteile entstehen beim Abschluss des Schritts.
fn beispiel(c: Category, s: &mut Scene) -> ElementId {
    let mono = s.model().type_by_guid(sk_model::MONO_TYPE_GUID).unwrap();
    assert!(s.edit_model("Beispiel", |m| {
        if c == Category::EdgeInsulation {
            m.set_default_type(sk_model::TypeCategory::ExteriorWall, mono);
        }
        let b = m.add_building(2);
        let rechteck = [
            vec3(0.0, 0.0, 0.0),
            vec3(0.0, 8000.0, 0.0),
            vec3(10000.0, 8000.0, 0.0),
            vec3(10000.0, 0.0, 0.0),
        ];
        let eg = m.build_from_polygon(b, &rechteck).unwrap();
        match c {
            Category::InteriorWall => {
                let st = m.run(eg).unwrap().storey;
                let t = m.default_type(sk_model::TypeCategory::InteriorWall);
                let iw = [vec3(5000.0, 0.0, 0.0), vec3(5000.0, 8000.0, 0.0)];
                m.add_wall_run(&iw, false, RefSide::Center, st, t, c)
                    .is_some()
            }
            Category::SoffitInsulation => {
                let w = m
                    .elements()
                    .iter()
                    .find(|(_, e)| e.number == "AW-006")
                    .map(|(id, _)| id)
                    .unwrap();
                m.set_linked(w, false) && m.move_segment(w, 300.0).is_some()
            }
            Category::RoofTerrace | Category::Coping => {
                let w = m
                    .elements()
                    .iter()
                    .find(|(_, e)| e.number == "AW-006")
                    .map(|(id, _)| id)
                    .unwrap();
                m.set_linked(w, false) && m.move_segment(w, -1500.0).is_some()
            }
            Category::PerimeterInsulation => {
                let slab = m.foundation_of(eg).unwrap().0;
                m.set_slab_insulation(slab, 120.0)
            }
            Category::Parapet | Category::Roof | Category::Downpipe => {
                let st = m.run(eg).unwrap().storey;
                m.set_flat_roof(st, true)
            }
            _ => true,
        }
    }));
    erstes(s, c).unwrap_or_else(|| panic!("{c:?}: Beispiel fehlt"))
}

/// A180 (R1, bauteil-integration.md §5): Der allgemeine Integrationstest
/// läuft über alle Kategorien der Registrierung, die es als Bauteil gibt:
/// 1. Guid, Nummer mit Präfix, Kategorie, lebendes Geschoss (Regel 1).
/// 2. Höhe nur über LevelRef: EG 30 cm höher → OG-Bauteile wandern genau
///    +300 mit, EG-Bauteile über ±0,00 gehen mit ihrer Oberkante mit
///    (OK EG bzw. UK Decke), die Gründung bleibt.
/// 7. Speichern → Öffnen ist bytegleich, das Dateiwort steht in der Datei.
/// 8. Anlegen → Rückgängig → Wiederherstellen: dieselbe Guid und Nummer.
/// 9. Löschen folgt der Regel: entweder gelöscht ohne tote Verweise oder
///    abgelehnt mit Satz.
/// 10. Die Art hat Mengen (eine Zeile mit ihrer Nummer im Mengenfenster).
/// 11. `Model::check()` ohne Befund.
///
/// Punkt 3 (Gewerk und KG je Schicht) prüft A184/A185 mit Paket 1,
/// Punkte 4–6 (Baum, Ausblenden, Sperren) kommen mit Paket 3/4.
#[test]
fn a180_allgemeiner_integrationstest() {
    for k in arten() {
        let c = k.category;
        if OHNE_BAUTEIL.contains(&c) {
            continue;
        }
        let mut s = Scene::with_model(Model::with_seed(180));
        let id = beispiel(c, &mut s);
        let e = s.model().element(id).expect("Beispiel lebt").clone();
        // 1
        assert_eq!(e.category, c, "{c:?}");
        assert!(
            e.number.starts_with(&format!("{}-", k.prefix)),
            "{c:?}: {}",
            e.number
        );
        assert!(s.model().storey(e.storey).is_some(), "{c:?}: Geschoss");
        let guids: Vec<_> = s.model().elements().iter().map(|(_, e)| e.guid).collect();
        assert_eq!(
            guids.iter().filter(|g| **g == e.guid).count(),
            1,
            "{c:?}: Guid eindeutig"
        );
        // 11
        assert!(
            s.model().check().is_empty(),
            "{c:?}: {:?}",
            s.model().check()
        );
        // 10
        let csv = csv_text(&mut s);
        assert!(
            csv.lines().any(|l| l.contains(&format!(";{};", e.number))
                || l.contains(&format!(";{} …", e.number))),
            "{c:?}: Mengenzeile für {}",
            e.number
        );
        // 7
        let text = sk_model::szo::write(s.model());
        assert!(
            text.contains(&format!(" cat={} ", k.szo)),
            "{c:?}: Dateiwort"
        );
        let m2 = sk_model::szo::read(&text, sk_model::GuidGen::with_seed(1))
            .expect("öffnen")
            .model;
        assert_eq!(sk_model::szo::write(&m2), text, "{c:?}: Rundlauf");
        // 2
        let og = geschoss(&s, "OG");
        let eg = geschoss(&s, "EG");
        let h = s.model().storey(eg).unwrap().height;
        let vorher = koerper_z(&mut s, id).unwrap_or_else(|| panic!("{c:?}: Körper"));
        assert!(s.edit_model("Geschosshöhe", |m| m.set_storey_height(eg, h + 300.0)));
        let nachher = koerper_z(&mut s, id).unwrap();
        if e.storey == og {
            assert_eq!(
                (nachher.0 - vorher.0, nachher.1 - vorher.1),
                (300.0, 300.0),
                "{c:?}: OG wandert mit"
            );
        } else if vorher.1 > 0.0 {
            assert_eq!(nachher.1 - vorher.1, 300.0, "{c:?}: Oberkante folgt dem EG");
            assert!(
                nachher.0 == vorher.0 || nachher.0 - vorher.0 == 300.0,
                "{c:?}"
            );
        } else {
            assert_eq!(nachher, vorher, "{c:?}: Gründung bleibt");
        }
        assert!(s.model().check().is_empty(), "{c:?}");
        assert!(s.undo());
        // 8
        let n = zahl(&s, c);
        assert!(s.undo(), "{c:?}: Anlegen rückgängig");
        assert!(zahl(&s, c) < n, "{c:?}: weg nach Rückgängig");
        assert!(s.redo());
        let wieder = s
            .model()
            .elements()
            .iter()
            .find(|(_, x)| x.guid == e.guid)
            .map(|(i, x)| (i, x.number.clone()));
        let (id, nummer) = wieder.unwrap_or_else(|| panic!("{c:?}: Wiederherstellen"));
        assert_eq!(nummer, e.number, "{c:?}: Nummer bleibt");
        // 9
        match s.model().can_delete(id) {
            Ok(()) => {
                let d = s.delete_elements(&[id]);
                assert!(d.removed.contains(&id), "{c:?}");
                assert!(s.model().element(id).is_none());
                assert!(s.model().check().is_empty(), "{c:?}: keine toten Verweise");
            }
            Err(r) => {
                let satz = sk_model::refusal_text(s.model(), id, &r);
                assert!(satz.ends_with('.'), "{c:?}: Satz {satz}");
            }
        }
    }
}
