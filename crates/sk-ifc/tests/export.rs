//! IFC-Export der Referenzhäuser (Plan ifc/plan.md §5, I1): Form der Datei,
//! Kennungen, Gliederung, Klassen, Schichten und Rauminhalt der Körper.

use sk_ifc::geometrie;
use sk_ifc::step::{self, File, V};
use sk_ifc::{export, Optionen};
use sk_model::{run_qto, szo, Category, ElementKind, GuidGen, Model};
use std::collections::{BTreeMap, HashMap, HashSet};

const HAEUSER: [(&str, &str); 6] = [
    (
        "rh1",
        include_str!("../../sk-cost/referenz/rh1-standardhaus.szo"),
    ),
    (
        "rh2",
        include_str!("../../sk-cost/referenz/rh2-mehrschalig.szo"),
    ),
    (
        "rh3",
        include_str!("../../sk-cost/referenz/rh3-versatz-dachterrasse.szo"),
    ),
    (
        "rh4",
        include_str!("../../sk-cost/referenz/rh4-erweiterungen.szo"),
    ),
    (
        "rh5",
        include_str!("../../sk-cost/referenz/rh5-flachdach.szo"),
    ),
    (
        "r7",
        include_str!("../../sk-cost/referenz/r7-alle-abschnitte.szo"),
    ),
];

fn laden(text: &str) -> Model {
    szo::read(text, GuidGen::with_seed(1)).expect("lädt").model
}

fn opt() -> Optionen {
    Optionen {
        zeit: 1_791_819_528,
        datei: "test.ifc".into(),
        version: "0.2.0".into(),
    }
}

fn verweise(v: &V, out: &mut Vec<u32>) {
    match v {
        V::Ref(i) => out.push(*i),
        V::List(xs) => xs.iter().for_each(|x| verweise(x, out)),
        V::Typed(_, x) => verweise(x, out),
        _ => {}
    }
}

fn text(v: &V) -> &str {
    match v {
        V::Str(t) => t,
        _ => "",
    }
}

fn liste(v: &V) -> Vec<u32> {
    let mut out = Vec::new();
    verweise(v, &mut out);
    out
}

/// Bauteile der Datei (Klassen, die Skizzeo für Bauteile schreibt) nach
/// Kennung: (Nummer, Klasse). Öffnungen und die Türen darin zählen nicht,
/// sie hängen an der Wand (I5).
fn bauteile(f: &File) -> HashMap<String, (u32, String)> {
    let klassen: HashSet<String> = sk_ifc::klassen::KLASSEN
        .iter()
        .map(|k| k.name.to_ascii_uppercase())
        .filter(|k| k != "IFCOPENINGELEMENT")
        .collect();
    let fuellung: HashSet<u32> = f
        .all("IFCRELFILLSELEMENT")
        .map(|(_, e)| liste(&e.args[5])[0])
        .collect();
    f.data
        .iter()
        .filter(|(i, e)| klassen.contains(&e.name) && !fuellung.contains(i))
        .map(|(i, e)| (text(&e.args[0]).to_string(), (*i, e.name.clone())))
        .collect()
}

#[test]
fn datei_ist_vollstaendig_und_eindeutig() {
    for (name, t) in HAEUSER {
        let m = laden(t);
        let (ifc, b) = export(&m, &opt());
        let f = step::read(&ifc).unwrap_or_else(|e| panic!("{name}: {e:?}"));
        assert_eq!(f.schema(), Some("IFC4"), "{name}");
        assert!(f.twice.is_empty(), "{name}: doppelte Nummern {:?}", f.twice);
        // Jede Verweisung zeigt auf einen Datensatz
        for (i, e) in &f.data {
            let mut r = Vec::new();
            e.args.iter().for_each(|a| verweise(a, &mut r));
            for x in r {
                assert!(f.data.contains_key(&x), "{name}: #{i} {} → #{x}", e.name);
            }
        }
        // GlobalId: 22 Zeichen, lesbar, eindeutig
        let mut seen = HashSet::new();
        for e in f.data.values() {
            if let Some(V::Str(g)) = e.args.first() {
                if e.name.starts_with("IFCREL")
                    || e.name.ends_with("TYPE")
                    || matches!(
                        e.name.as_str(),
                        "IFCPROJECT"
                            | "IFCSITE"
                            | "IFCBUILDING"
                            | "IFCBUILDINGSTOREY"
                            | "IFCPROPERTYSET"
                            | "IFCELEMENTQUANTITY"
                            | "IFCELEMENTASSEMBLY"
                            | "IFCOPENINGELEMENT"
                            | "IFCDOOR"
                            | "IFCSPACE"
                            | "IFCCOVERING"
                    )
                {
                    assert!(sk_model::Guid::from_ifc(g).is_some(), "{name}: {g}");
                    assert!(seen.insert(g.clone()), "{name}: {g} doppelt");
                }
            }
        }
        // Jedes Bauteil mit Körper genau einmal, mit seiner Kennung, dazu
        // die Räume und ihr Ausbau (I6)
        let im = bauteile(&f);
        assert_eq!(im.len(), b.bauteile + b.raeume + b.bekleidungen, "{name}");
        assert_eq!(f.all("IFCSPACE").count(), b.raeume, "{name}");
        // Ausgelassen nur Räume ohne Fläche (r7: R-001 ankert in einer Wand)
        for (nr, g) in &b.ausgelassen {
            assert_eq!(*g, sk_ifc::Grund::RaumOhneFlaeche, "{name}: {nr}");
        }
        let solids = m.element_solids();
        let draussen = |nr: &str| b.ausgelassen.iter().any(|(n, _)| n == nr);
        let mit_koerper = solids
            .iter()
            .filter(|(id, _)| !draussen(&m.element(*id).unwrap().number))
            .count();
        assert_eq!(mit_koerper, b.bauteile, "{name}");
        for (id, _) in &solids {
            let el = m.element(*id).unwrap();
            let draussen = b.ausgelassen.iter().any(|(nr, _)| *nr == el.number);
            assert_eq!(
                im.contains_key(&el.guid.to_ifc()),
                !draussen,
                "{name}: {}",
                el.number
            );
        }
        // Jedes Bauteil in genau einem Geschoss
        let mut drin: HashMap<u32, usize> = HashMap::new();
        for (_, e) in f.all("IFCRELCONTAINEDINSPATIALSTRUCTURE") {
            assert_eq!(
                f.get(liste(&e.args[5])[0]).unwrap().name,
                "IFCBUILDINGSTOREY"
            );
            for x in liste(&e.args[4]) {
                *drin.entry(x).or_default() += 1;
            }
        }
        // … direkt oder über genau eine Baugruppe (Wandzug, I4), nie beides
        // (Räume hängen genau einmal am Geschoss, IFC4 WR31)
        let mut teil_von: HashMap<u32, Vec<u32>> = HashMap::new();
        let mut am_geschoss: HashMap<u32, usize> = HashMap::new();
        for (_, e) in f.all("IFCRELAGGREGATES") {
            let ganz = liste(&e.args[4])[0];
            match f.get(ganz).unwrap().name.as_str() {
                "IFCELEMENTASSEMBLY" => {
                    for x in liste(&e.args[5]) {
                        teil_von.entry(x).or_default().push(ganz);
                    }
                }
                "IFCBUILDINGSTOREY" => {
                    for x in liste(&e.args[5]) {
                        *am_geschoss.entry(x).or_default() += 1;
                    }
                }
                _ => {}
            }
        }
        let mut gruppen = HashSet::new();
        for (g, (i, klasse)) in &im {
            if klasse == "IFCSPACE" {
                assert_eq!(drin.get(i), None, "{name}: Raum {g} enthalten");
                assert_eq!(am_geschoss.get(i), Some(&1), "{name}: Raum {g}");
                continue;
            }
            match teil_von.get(i).map(Vec::as_slice) {
                Some(&[a]) => {
                    assert_eq!(drin.get(i), None, "{name}: {g} auch direkt");
                    assert_eq!(drin.get(&a), Some(&1), "{name}: Baugruppe von {g}");
                    gruppen.insert(a);
                }
                None => assert_eq!(drin.get(i), Some(&1), "{name}: {g}"),
                Some(v) => panic!("{name}: {g} in {} Baugruppen", v.len()),
            }
        }
        assert_eq!(gruppen.len(), b.baugruppen, "{name}");
        // Jede Öffnung locht genau eine Wand und steht in keinem Geschoss
        let mut gelocht = HashMap::new();
        for (_, e) in f.all("IFCRELVOIDSELEMENT") {
            let (wand, loch) = (liste(&e.args[4])[0], liste(&e.args[5])[0]);
            assert_eq!(f.get(wand).unwrap().name, "IFCWALL", "{name}");
            assert_eq!(f.get(loch).unwrap().name, "IFCOPENINGELEMENT", "{name}");
            assert_eq!(gelocht.insert(loch, wand), None, "{name}: #{loch} doppelt");
        }
        for (i, _) in f.all("IFCOPENINGELEMENT") {
            assert!(gelocht.contains_key(&i), "{name}: #{i} lochte nichts");
            assert_eq!(drin.get(&i), None, "{name}: #{i} im Geschoss");
        }
        // Jede Tür füllt genau eine Öffnung und steht in genau einem Geschoss
        let mut gefuellt = HashSet::new();
        for (_, e) in f.all("IFCRELFILLSELEMENT") {
            let (loch, tuer) = (liste(&e.args[4])[0], liste(&e.args[5])[0]);
            assert!(gelocht.contains_key(&loch), "{name}");
            assert_eq!(f.get(tuer).unwrap().name, "IFCDOOR", "{name}");
            assert!(gefuellt.insert(tuer), "{name}: #{tuer} doppelt");
            assert_eq!(drin.get(&tuer), Some(&1), "{name}: #{tuer}");
        }
        assert_eq!(gefuellt.len(), b.tueren, "{name}");
        assert_eq!(f.all("IFCELEMENTASSEMBLY").count(), b.baugruppen, "{name}");
        assert_eq!(f.all("IFCBUILDINGSTOREY").count(), b.geschosse, "{name}");
        assert_eq!(f.all("IFCPROJECT").count(), 1);
        assert_eq!(f.all("IFCSITE").count(), 1);
    }
}

#[test]
fn zwei_exporte_bytegleich_kennungen_bleiben() {
    for (name, t) in HAEUSER {
        let m = laden(t);
        let (a, _) = export(&m, &opt());
        let (b, _) = export(&m, &opt());
        assert!(a == b, "{name}: zwei Exporte unterscheiden sich");
        // Nach Speichern und Laden dieselben Kennungen (Rundlauf .szo)
        let m2 = laden(&szo::write(&m));
        let (c, _) = export(&m2, &opt());
        let ka: Vec<String> = bauteile(&step::read(&a).unwrap()).into_keys().collect();
        let kc: HashSet<String> = bauteile(&step::read(&c).unwrap()).into_keys().collect();
        assert!(ka.iter().all(|k| kc.contains(k)), "{name}");
    }
}

#[test]
fn klassen_wie_in_der_bauteilliste() {
    let m = laden(HAEUSER[5].1);
    let (ifc, _) = export(&m, &opt());
    let f = step::read(&ifc).unwrap();
    let im = bauteile(&f);
    let mut gesehen: BTreeMap<Category, String> = BTreeMap::new();
    for (id, _) in m.element_solids() {
        let el = m.element(id).unwrap();
        let soll = match &el.kind {
            ElementKind::Ext(_) => m.ext_def_of(id).unwrap().ifc().to_string(),
            _ => el.category.ifc_class().to_string(),
        };
        let z = sk_ifc::klassen::zuordnen(&soll);
        let (i, klasse) = &im[&el.guid.to_ifc()];
        assert_eq!(*klasse, z.klasse.name.to_ascii_uppercase(), "{}", el.number);
        let e = f.get(*i).unwrap();
        assert_eq!(text(&e.args[2]), el.number);
        assert_eq!(e.args[8], V::Enum(z.predefined.into()), "{}", el.number);
        gesehen.insert(el.category, klasse.clone());
    }
    // Das Haus mit allen Abschnitten zeigt die wichtigsten Arten
    for c in [
        Category::ExteriorWall,
        Category::GroundSlab,
        Category::StripFooting,
        Category::Floor,
        Category::Roof,
        Category::Parapet,
        Category::Coping,
        Category::Downpipe,
    ] {
        assert!(gesehen.contains_key(&c), "{c:?} fehlt im Prüfhaus");
    }
    // Attikablech: IFC4 kennt COPING noch nicht
    assert!(ifc.contains(",'COPING',"));
    // Umlaute nach ISO 10303-21
    assert!(ifc.contains("'Au\\X2\\00DF\\X0\\enwand'"));
}

#[test]
fn schichten_wie_im_bauteiltyp() {
    for (name, t) in HAEUSER {
        let m = laden(t);
        let (ifc, _) = export(&m, &opt());
        let f = step::read(&ifc).unwrap();
        let typen: HashMap<String, u32> = f
            .all("IFCWALLTYPE")
            .map(|(i, e)| (text(&e.args[2]).to_string(), i))
            .collect();
        // Typ → Aufbau über die Baustoffzuordnung des Typs
        let mut aufbau_von: HashMap<u32, u32> = HashMap::new();
        for (_, e) in f.all("IFCRELASSOCIATESMATERIAL") {
            for x in liste(&e.args[4]) {
                aufbau_von.insert(x, liste(&e.args[5])[0]);
            }
        }
        for (id, set) in m.layer_sets().iter() {
            let Some(&t) = typen.get(&set.name) else {
                continue;
            };
            let _ = id;
            let ls = f.get(aufbau_von[&t]).unwrap();
            assert_eq!(ls.name, "IFCMATERIALLAYERSET", "{name}");
            let schichten = liste(&ls.args[0]);
            assert_eq!(schichten.len(), set.layers.len(), "{name}: {}", set.name);
            for (l, &si) in set.layers.iter().zip(&schichten) {
                let e = f.get(si).unwrap();
                let V::Real(d) = e.args[1] else {
                    panic!("Dicke")
                };
                assert!((d - l.thickness).abs() < 1e-3, "{name}: {}", set.name);
                let mat = m.material(l.material).unwrap();
                let mi = liste(&e.args[0])[0];
                assert_eq!(text(&f.get(mi).unwrap().args[0]), mat.name);
            }
        }
    }
}

/// Geschlossene Wandkörper haben den Rauminhalt aus den Mengen (Schichten
/// ohne Luft), im Haus mit zweischaligem Mauerwerk und im Haus mit allen
/// Abschnitten. In den Häusern mit Rücksprung zählen Ansicht und Mengen ein
/// Stück Außenschale über der Decke zum Teil verschiedenen Wänden zu
/// (Befund an Geometrie, ifc/plan.md §8); dort wird nicht verglichen.
#[test]
fn rauminhalt_wie_mengen() {
    let mut geprueft = 0;
    for (name, t) in HAEUSER {
        if !matches!(name, "rh2" | "r7") {
            continue;
        }
        let m = laden(t);
        for (id, s) in m.element_solids() {
            let Some((run, seg)) = m.segment_of(id) else {
                continue;
            };
            let netze = geometrie::netze(&s, Default::default());
            assert!(
                netze.iter().all(geometrie::geschlossen),
                "{name}: {} offen",
                m.element(id).unwrap().number
            );
            let v: f64 = netze.iter().map(geometrie::volumen).sum();
            let q = run_qto(&m, run)[seg].volume;
            assert!(
                (v - q).abs() <= q.abs() * 1e-3,
                "{name}: {} {v} statt {q}",
                m.element(id).unwrap().number
            );
            geprueft += 1;
        }
    }
    assert!(geprueft >= 20, "nur {geprueft} geprüft");
}

/// Review Q10: Ein Raum aus der Werkbank hängt am Geschoss
/// (`IfcRelAggregates`), nicht in `IfcRelContainedInSpatialStructure`
/// (IFC4 WR31); eine Öffnung ohne gelochtes Bauteil bleibt draußen und steht
/// im Bericht.
#[test]
fn raum_am_geschoss_oeffnung_draussen() {
    let t = HAEUSER[3]
        .1
        .replace("ifc=IfcRailing.GUARDRAIL", "ifc=IfcSpace")
        .replace("ifc=IfcColumn.COLUMN", "ifc=IfcOpeningElement");
    let m = laden(&t);
    let (ifc, b) = export(&m, &opt());
    let f = step::read(&ifc).unwrap();
    let raeume: Vec<u32> = f.all("IFCSPACE").map(|(i, _)| i).collect();
    assert!(!raeume.is_empty(), "Geländer als Raum");
    assert!(!b.ausgelassen.is_empty(), "Stütze als Öffnung");
    assert!(b
        .ausgelassen
        .iter()
        .all(|(_, g)| *g == sk_ifc::Grund::OhneWand));
    // als Öffnung eingestuft lochte die Stütze keine Wand
    for (_, e) in f.all("IFCOPENINGELEMENT") {
        assert!(!b.ausgelassen.iter().any(|(nr, _)| nr == text(&e.args[2])));
    }
    for (_, e) in f.all("IFCRELCONTAINEDINSPATIALSTRUCTURE") {
        for x in liste(&e.args[4]) {
            assert!(!raeume.contains(&x), "Raum #{x} enthalten statt angehängt");
        }
    }
    let mut haengt = 0;
    for (_, e) in f.all("IFCRELAGGREGATES") {
        let ganz = f.get(liste(&e.args[4])[0]).unwrap();
        for x in liste(&e.args[5]) {
            if raeume.contains(&x) {
                assert_eq!(ganz.name, "IFCBUILDINGSTOREY");
                haengt += 1;
            }
        }
    }
    assert_eq!(haengt, raeume.len());
}

/// I3 (ifc/plan.md §6): Die Vorschau liest jede geschriebene Datei wieder:
/// jedes Bauteil genau einmal in genau einem Geschoss, mit Nummer, Form mit
/// demselben Rauminhalt wie im Export und dem Schichtaufbau seines Typs.
#[test]
fn vorschau_liest_den_export() {
    for (name, t) in HAEUSER {
        let m = laden(t);
        let (ifc, b) = export(&m, &opt());
        let v = sk_ifc::lesen::vorschau(&step::read(&ifc).unwrap());
        assert_eq!(v.schema, "IFC4");
        assert_eq!(
            v.teile.len(),
            b.bauteile + b.raeume + b.bekleidungen,
            "{name}"
        );
        assert_eq!(v.ohne_form, 0, "{name}");
        let mut im_baum = vec![0; v.teile.len()];
        fn zaehlen(k: &sk_ifc::lesen::Knoten, n: &mut [u32], geschosse: &mut usize) {
            if k.klasse == "IfcBuildingStorey" {
                *geschosse += 1;
            }
            for &i in &k.teile {
                n[i] += 1;
            }
            for c in &k.kinder {
                zaehlen(c, n, geschosse);
            }
        }
        let mut geschosse = 0;
        for k in &v.baum {
            zaehlen(k, &mut im_baum, &mut geschosse);
        }
        assert_eq!(geschosse, b.geschosse, "{name}");
        assert!(im_baum.iter().all(|&n| n == 1), "{name}: {im_baum:?}");
        let teil: HashMap<&str, &sk_ifc::lesen::Teil> =
            v.teile.iter().map(|t| (t.guid.as_str(), t)).collect();
        for (id, s) in m.element_solids() {
            let el = m.element(id).unwrap();
            let t = teil[el.guid.to_ifc().as_str()];
            assert_eq!(t.name, el.number);
            if let Some(ls) = el.layer_set.and_then(|l| m.layer_set(l)) {
                assert_eq!(t.schichten.len(), ls.layers.len(), "{name}: {}", el.number);
            }
            assert!(
                !t.saetze.is_empty(),
                "{name}: {} ohne Eigenschaften",
                el.number
            );
            let netze = geometrie::netze(&s, Default::default());
            assert_eq!(
                t.netze.iter().map(|n| n.dreiecke.len()).sum::<usize>(),
                netze.iter().map(|n| n.dreiecke.len()).sum::<usize>(),
                "{name}: {}",
                el.number
            );
            // Der Rauminhalt hängt nur bei geschlossenen Netzen nicht von der
            // Lage ab (Datei: im Geschoss, Vorschau: in der Welt)
            if !netze.iter().all(geometrie::geschlossen) {
                continue;
            }
            let soll: f64 = netze.iter().map(geometrie::volumen).sum();
            let ist: f64 = t
                .netze
                .iter()
                .map(|n| {
                    geometrie::volumen(&geometrie::Netz {
                        mat: 0,
                        punkte: n.punkte.clone(),
                        dreiecke: n.dreiecke.clone(),
                    })
                })
                .sum();
            assert!(
                // Punkte stehen auf 0,0001 mm gerundet in der Datei
                (ist - soll).abs() <= soll.abs() * 1e-5 + 1.0,
                "{name}: {} {ist} statt {soll}",
                el.number
            );
        }
    }
}

/// I5 (ifc/plan.md §5): r7 hat ein Fenster (OE-001), eine Tür (OE-002),
/// eine Innentür (OE-003, in einer Innenwand) und ein Fenster im OG (OE-004,
/// mit Breitenschloss an OE-001). Jede steht als `IfcOpeningElement` mit der Kennung der Öffnung
/// an ihrer Wand, liegt relativ zu deren Lage, hat ihre Form als
/// `Reference` (das Wandnetz ist gelocht) und Breite, Höhe und Tiefe wie im
/// Modell.
#[test]
fn oeffnungen_an_der_wand() {
    let m = laden(HAEUSER[5].1);
    let (ifc, b) = export(&m, &opt());
    let f = step::read(&ifc).unwrap();
    assert_eq!(b.oeffnungen, 4);
    assert!(b
        .ausgelassen
        .iter()
        .all(|(_, g)| *g == sk_ifc::Grund::RaumOhneFlaeche));
    let wand_von: HashMap<u32, u32> = f
        .all("IFCRELVOIDSELEMENT")
        .map(|(_, e)| (liste(&e.args[5])[0], liste(&e.args[4])[0]))
        .collect();
    let mengen = mengen_der_datei("r7", &f);
    for nr in ["OE-001", "OE-002", "OE-003", "OE-004"] {
        let (id, el) = m.elements().iter().find(|(_, e)| e.number == nr).unwrap();
        let voids = m.opening_voids(id);
        assert_eq!(voids.len(), 1, "{nr}");
        let (oi, oe) = f
            .all("IFCOPENINGELEMENT")
            .find(|(_, e)| text(&e.args[0]) == el.guid.to_ifc())
            .unwrap_or_else(|| panic!("{nr} fehlt"));
        assert_eq!(text(&oe.args[2]), nr);
        // an der Wand, die das Modell lochte, relativ zu ihrer Lage
        let wand = f.get(wand_von[&oi]).unwrap();
        let w = m.element(voids[0].wall).unwrap();
        assert_eq!(text(&wand.args[0]), w.guid.to_ifc(), "{nr}");
        let pl = f.get(liste(&oe.args[5])[0]).unwrap();
        assert_eq!(liste(&pl.args[0]), liste(&wand.args[5]), "{nr}");
        // Form nur als Bezug
        let shape = f.get(liste(&oe.args[6])[0]).unwrap();
        for rep in liste(&shape.args[2]) {
            assert_eq!(text(&f.get(rep).unwrap().args[1]), "Reference", "{nr}");
        }
        let q = &mengen[&el.guid.to_ifc()]["Qto_OpeningElementBaseQuantities"];
        let op = m.opening(id).unwrap();
        assert!((q["Width"] - op.width()).abs() < 1e-6, "{nr}: {q:?}");
        assert!((q["Depth"] - voids[0].depth).abs() < 1e-6, "{nr}");
        let (z0, z1) = m.opening_band(op).unwrap();
        assert!((q["Height"] - (z1 - z0)).abs() < 1e-6, "{nr}");
    }
}

/// Eigenschaften des Satzes `satz` am Objekt `obj`: Name → Wert.
fn eigenschaften(f: &File, obj: u32, satz: &str) -> HashMap<String, V> {
    let mut out = HashMap::new();
    for (_, rel) in f.all("IFCRELDEFINESBYPROPERTIES") {
        let ps = f.get(liste(&rel.args[5])[0]).unwrap();
        if ps.name != "IFCPROPERTYSET" || text(&ps.args[2]) != satz {
            continue;
        }
        if !liste(&rel.args[4]).contains(&obj) {
            continue;
        }
        for p in liste(&ps.args[4]) {
            let p = f.get(p).unwrap();
            let v = match &p.args[2] {
                V::Typed(_, v) => (**v).clone(),
                v => v.clone(),
            };
            out.insert(text(&p.args[0]).to_string(), v);
        }
    }
    out
}

/// I6 (ifc/plan.md §5, schnittstelle-raum.md §5, raumbuch/plan.md §9): Ein
/// gespeicherter Raum mit Fläche steht als `IfcSpace` mit seiner Kennung,
/// Nummer und Namen, geschlossener Form von OK Fertigfußboden bis UK
/// Fertigdecke, `Qto_SpaceBaseQuantities` aus Form, Fertigumriss und
/// Raumbuch, Wohnfläche in `Skizzeo_Raum` und Nutzungsgruppe DIN 277. Sein
/// Ausbau steht je Aufbau als `IfcCovering` mit den Körpern des Raumbuchs,
/// am Raum und am bekleideten Bauteil, der Estrich mit wirksamen Dicken.
/// R-001 ankert in einer Wand (ruht): ohne Fläche kein `IfcSpace`.
#[test]
fn raum_mit_form_mengen_und_ausbau() {
    use sk_raum::ausbau::{self, RaumAusbau};
    use sk_raum::oeffnungen::Rohbau;
    use sk_raum::raumbuch::{self, koerper, Flaeche};
    let mut m = laden(HAEUSER[5].1);
    let id = m
        .elements()
        .iter()
        .find(|(_, e)| e.number == "R-002")
        .unwrap()
        .0;
    // Typen wie beim Entstehen im Grundriss: Vorlage, Ausbau, Estrich
    m.begin("Ausbau");
    assert!(ausbau::typen_anlegen(&mut m, id));
    assert!(m.commit().is_some());
    let el = m.element(id).unwrap().clone();
    let form = sk_raum::raeume(&m, el.storey)
        .into_iter()
        .find(|f| f.raum == Some(id))
        .unwrap();
    let roh = Rohbau::neu(&m, form);
    let aus = RaumAusbau::fuer(&m, &roh).unwrap();
    let (ffb, ufd) = ausbau::hoehen(&m, &roh, &aus);
    assert_eq!(ffb - roh.z_boden, 160.0, "Fußbodenaufbau");

    let (ifc, b) = export(&m, &opt());
    let f = step::read(&ifc).unwrap();
    assert_eq!(b.raeume, 1);
    assert_eq!(
        b.ausgelassen,
        vec![("R-001".to_string(), sk_ifc::Grund::RaumOhneFlaeche)]
    );
    assert_eq!(sk_ifc::Grund::RaumOhneFlaeche.text(), "Raum ohne Fläche");
    let (si, sp) = f.all("IFCSPACE").next().unwrap();
    assert_eq!(text(&sp.args[0]), el.guid.to_ifc());
    assert_eq!(text(&sp.args[2]), "R-002");
    assert_eq!(text(&sp.args[3]), "Wohnzimmer");
    assert_eq!(text(&sp.args[7]), m.space(id).unwrap().name);
    assert_eq!(zahl(&sp.args[10]), 160.0, "OK FFB über dem Geschoss");

    // Mengen: roh aus der Form, fertig aus dem Fertigumriss
    let rd = sk_raum::raumliste::raumliste(&m)
        .into_iter()
        .find(|r| r.raum == id)
        .unwrap();
    let (rand, loecher) = rd.fertig.clone().unwrap();
    let netto = sk_math::polygon::area(&rand)
        - loecher
            .iter()
            .map(|l| sk_math::polygon::area(l))
            .sum::<f64>();
    let nah = |a: f64, b: f64| (a - b).abs() <= b.abs() * 1e-9 + 1e-9;
    let mengen = mengen_der_datei("r7", &f);
    let q = &mengen[&el.guid.to_ifc()]["Qto_SpaceBaseQuantities"];
    assert!(nah(q["NetFloorArea"], netto / 1e6), "{q:?}");
    assert!(nah(q["GrossFloorArea"], roh.form.flaeche / 1e6), "{q:?}");
    assert!(netto < roh.form.flaeche);
    assert!(nah(q["GrossPerimeter"], roh.form.umfang), "{q:?}");
    assert!(nah(q["Height"], roh.z_decke - roh.z_boden), "{q:?}");
    assert!(nah(q["FinishCeilingHeight"], ufd - ffb), "{q:?}");
    assert!(nah(q["FinishFloorHeight"], 160.0), "{q:?}");
    assert!(nah(q["NetVolume"], netto * (ufd - ffb) / 1e9), "{q:?}");
    let zeilen = raumbuch::rechnen(&m, &roh, &aus, &raumbuch::Regeln::default());
    let wand: f64 = zeilen
        .iter()
        .filter(|z| matches!(z.flaeche, Flaeche::Wand(_) | Flaeche::Laibung(_)))
        .filter(|z| matches!(z.typ, Some((_, 0))))
        .map(|z| z.menge())
        .sum();
    assert!(wand > 0.0 && nah(q["NetWallArea"], wand / 1e6), "{q:?}");

    // Form: geschlossen, Rauminhalt wie NetVolume
    let v = sk_ifc::lesen::vorschau(&f);
    let t = v.teile.iter().find(|t| t.guid == el.guid.to_ifc()).unwrap();
    assert_eq!(t.klasse, "IfcSpace");
    let netz = |n: &sk_ifc::lesen::Netz| geometrie::Netz {
        mat: 0,
        punkte: n.punkte.clone(),
        dreiecke: n.dreiecke.clone(),
    };
    assert!(t.netze.iter().all(|n| geometrie::geschlossen(&netz(n))));
    let vol: f64 = t.netze.iter().map(|n| geometrie::volumen(&netz(n))).sum();
    assert!(
        (vol / 1e9 - q["NetVolume"]).abs() < q["NetVolume"] * 1e-5,
        "{vol}"
    );

    // Wohnfläche (Faktor der Vorlage Wohnzimmer: 1) und DIN 277
    let r = eigenschaften(&f, si, "Skizzeo_Raum");
    assert!(nah(zahl(&r["Wohnfläche"]), netto / 1e6), "{r:?}");
    assert_eq!(zahl(&r["Wohnflächenfaktor"]), 1.0);
    assert_eq!(text(&r["Nutzungsgruppe"]), "NUF 1 Wohnen und Aufenthalt");
    let din277: Vec<_> = f
        .all("IFCRELASSOCIATESCLASSIFICATION")
        .filter(|(_, e)| text(&e.args[2]).starts_with("DIN 277"))
        .collect();
    assert_eq!(din277.len(), 1);
    assert_eq!(text(&din277[0].1.args[2]), "DIN 277 NUF 1");
    assert_eq!(liste(&din277[0].1.args[4]), vec![si]);

    // Ausbau: je Fläche und Typ ein IfcCovering, alle am Raum
    let koerper = koerper::koerper(&m, &roh, &aus);
    let mut gruppen: Vec<(Flaeche, sk_model::LayerSetId)> = Vec::new();
    for k in &koerper {
        if !gruppen.contains(&(k.flaeche, k.typ)) {
            gruppen.push((k.flaeche, k.typ));
        }
    }
    assert!(gruppen.iter().any(|g| g.0 == Flaeche::Estrich));
    assert_eq!(b.bekleidungen, gruppen.len());
    let am_raum: Vec<(u32, Vec<u32>)> = f
        .all("IFCRELCOVERSSPACES")
        .map(|(_, e)| (liste(&e.args[4])[0], liste(&e.args[5])))
        .collect();
    assert_eq!(am_raum.len(), 1);
    assert_eq!(am_raum[0].0, si);
    let bekleidungen = &am_raum[0].1;
    assert_eq!(bekleidungen.len(), gruppen.len());
    assert_eq!(
        f.all("IFCCOVERING")
            .filter(|(i, _)| bekleidungen.contains(i))
            .count(),
        gruppen.len()
    );
    // Am bekleideten Bauteil: Wandseiten an ihrer Wand, Decke und Boden an
    // der Decke bzw. Sohlplatte
    let mut traeger: Vec<(String, String)> = Vec::new();
    for (_, e) in f.all("IFCRELCOVERSBLDGELEMENTS") {
        let t = f.get(liste(&e.args[4])[0]).unwrap();
        for c in liste(&e.args[5]) {
            let name = text(&f.get(c).unwrap().args[2]).to_string();
            traeger.push((name, format!("{} {}", t.name, text(&t.args[0]))));
        }
    }
    traeger.sort();
    let bauteil = |g: sk_model::Guid| {
        let (_, e) = m.elements().iter().find(|(_, e)| e.guid == g).unwrap();
        let klasse = match e.kind {
            sk_model::ElementKind::Wall(_) => "IFCWALL",
            _ => "IFCSLAB",
        };
        format!("{klasse} {}", g.to_ifc())
    };
    let mut soll: Vec<(String, String)> = rd
        .ausbau
        .iter()
        .filter(|a| a.schichten.iter().any(|s| s.koerper.is_some()))
        .filter_map(|a| {
            let name = match a.flaeche {
                Flaeche::Wand(k) => format!("R-002 Wand {}", k + 1),
                Flaeche::Fliesen(k) => format!("R-002 Fliesen Wand {}", k + 1),
                f => format!("R-002 {f:?}"),
            };
            Some((name, bauteil(a.traeger?)))
        })
        .collect();
    soll.sort();
    assert_eq!(traeger, soll);
    for s in ["Wand 1", "Decke", "Estrich"] {
        assert!(traeger.iter().any(|t| t.0 == format!("R-002 {s}")), "{s}");
    }
    // Körper wie im Raumbuch: derselbe Rauminhalt
    let soll: f64 = koerper
        .iter()
        .flat_map(|k| geometrie::netze(&k.solid, Default::default()))
        .map(|n| geometrie::volumen(&n))
        .sum();
    let ist: f64 = v
        .teile
        .iter()
        .filter(|t| t.klasse == "IfcCovering" && t.name.starts_with("R-002 "))
        .flat_map(|t| t.netze.iter())
        .map(|n| geometrie::volumen(&netz(n)))
        .sum();
    assert!((ist - soll).abs() < soll * 1e-5, "{ist} statt {soll}");
    // Estrich mit wirksamen Dicken: mit dem Belag der Fußbodenaufbau
    let estrich = v.teile.iter().find(|t| t.name == "R-002 Estrich").unwrap();
    let belag = v.teile.iter().find(|t| t.name == "R-002 Boden").unwrap();
    let dicke = |t: &sk_ifc::lesen::Teil| t.schichten.iter().map(|s| s.1).sum::<f64>();
    assert!(
        nah(dicke(estrich) + dicke(belag), 160.0),
        "{:?}",
        estrich.schichten
    );
    // Fläche des Belags aus dem Raumbuch
    let boden: f64 = zeilen
        .iter()
        .filter(|z| z.flaeche == Flaeche::Boden && matches!(z.typ, Some((_, 0))))
        .map(|z| z.menge())
        .sum();
    let g = &belag.guid;
    assert!(nah(
        mengen[g]["Qto_CoveringBaseQuantities"]["NetArea"],
        boden / 1e6
    ));
}

/// I5 (ifc/plan.md §2, Merkpunkt Integration): Eine Tür mit Blatt füllt
/// ihre Öffnung als `IfcDoor`. IFC4 zählt links und rechts in Blickrichtung
/// +y der Tür, in die sie aufschlägt, DIN 107 von der Öffnungsfläche aus:
/// DIN links ist `SINGLE_SWING_RIGHT`. Je Aufschlagseite und Türhand: +y
/// zeigt in den Raum des Aufschlags, die Bänder sitzen am Pfosten, den das
/// Modell nennt.
#[test]
fn tuer_in_der_oeffnung_nach_din_107() {
    let r7 = HAEUSER[5].1;
    assert!(r7.contains("swing=r din=links"));
    // OE-003 läuft von (19942,5 | 3000) nach (19942,5 | 3885)
    let (_, lage) = {
        let m = laden(r7);
        let (id, _) = m
            .elements()
            .iter()
            .find(|(_, e)| e.number == "OE-003")
            .unwrap();
        (id, m.door_leaf(id).unwrap())
    };
    // rechts der Laufrichtung (+y) ist +x; DIN links von dort aus gesehen:
    // Bänder am Anfang
    assert!(lage.swing_into.x > 0.99, "{lage:?}");
    assert!((lage.hinge.y - 3000.0).abs() < 1e-6, "{lage:?}");
    for (seite, din, soll) in [
        ("r", "links", "SINGLE_SWING_RIGHT"),
        ("r", "rechts", "SINGLE_SWING_LEFT"),
        ("l", "links", "SINGLE_SWING_RIGHT"),
        ("l", "rechts", "SINGLE_SWING_LEFT"),
    ] {
        let t = r7.replace("swing=r din=links", &format!("swing={seite} din={din}"));
        let m = laden(&t);
        let (id, el) = m
            .elements()
            .iter()
            .find(|(_, e)| e.number == "OE-003")
            .unwrap();
        let l = m.door_leaf(id).unwrap();
        let (ifc, b) = export(&m, &opt());
        let f = step::read(&ifc).unwrap();
        assert_eq!(b.tueren, 1);
        let fill = f.all("IFCRELFILLSELEMENT").next().unwrap().1;
        let loch = f.get(liste(&fill.args[4])[0]).unwrap();
        assert_eq!(text(&loch.args[0]), el.guid.to_ifc());
        let (ti, tuer) = f.all("IFCDOOR").next().unwrap();
        assert_eq!(liste(&fill.args[5]), vec![ti]);
        assert_eq!(text(&tuer.args[2]), "OE-003");
        assert_eq!(tuer.args[10], V::Enum("DOOR".into()));
        assert_eq!(tuer.args[11], V::Enum(soll.into()), "{seite} {din}");
        let op = m.opening(id).unwrap();
        let (z0, z1) = m.opening_band(op).unwrap();
        assert!((zahl_oder_len(&tuer.args[9]) - op.width()).abs() < 1e-6);
        assert!((zahl_oder_len(&tuer.args[8]) - (z1 - z0)).abs() < 1e-6);
        // Lage relativ zur Öffnung, z nach oben, +y in den Aufschlagraum
        let pl = f.get(liste(&tuer.args[5])[0]).unwrap();
        assert_eq!(liste(&pl.args[0]), liste(&loch.args[5]));
        let a = f.get(liste(&pl.args[1])[0]).unwrap();
        let punkt = |i: u32| -> Vec<f64> {
            match &f.get(i).unwrap().args[0] {
                V::List(xs) => xs.iter().map(zahl_oder_len).collect(),
                _ => vec![],
            }
        };
        let o = punkt(liste(&a.args[0])[0]);
        // Unterkante der Öffnung in dieser Wand, im Geschoss
        let elev = m.storey(el.storey).unwrap().elevation;
        assert!((o[2] - (z0 - elev)).abs() < 1e-6, "{o:?}");
        assert_eq!(punkt(liste(&a.args[1])[0]), vec![0.0, 0.0, 1.0]);
        let x = punkt(liste(&a.args[2])[0]);
        let y = (-x[1], x[0]);
        assert!((y.0 - l.swing_into.x).abs() < 1e-9 && (y.1 - l.swing_into.y).abs() < 1e-9);
        // Bänder links (x = 0) oder rechts (x = Breite) in Blickrichtung +y
        let u = (l.hinge.x - o[0]) * x[0] + (l.hinge.y - o[1]) * x[1];
        let links = soll == "SINGLE_SWING_LEFT";
        assert!(
            (u - if links { 0.0 } else { op.width() }).abs() < 1e-6,
            "{seite} {din}: {u}"
        );
        let q = &mengen_der_datei("r7", &f)[text(&tuer.args[0])]["Qto_DoorBaseQuantities"];
        assert!((q["Width"] - op.width()).abs() < 1e-6, "{q:?}");
        assert!(
            (q["Area"] - op.width() * (z1 - z0) * 1e-6).abs() < 1e-6,
            "{q:?}"
        );
        // Merkmal wie in der Ausschreibung
        let ps = f
            .all("IFCPROPERTYSINGLEVALUE")
            .find(|(_, e)| text(&e.args[0]) == "Öffnungsrichtung")
            .unwrap()
            .1;
        assert_eq!(
            ps.args[2],
            V::Typed("IFCLABEL".into(), Box::new(V::Str(format!("DIN {din}"))))
        );
    }
}

fn zahl_oder_len(v: &V) -> f64 {
    match v {
        V::Len(x) => *x,
        _ => zahl(v),
    }
}

/// Mengen der Datei je Bauteil (Kennung) → Satz → Menge → Wert.
type Mengen = HashMap<String, BTreeMap<String, BTreeMap<String, f64>>>;

fn mengen_der_datei(name: &str, f: &File) -> Mengen {
    let mut kennung: HashMap<u32, String> =
        bauteile(f).into_iter().map(|(g, (i, _))| (i, g)).collect();
    for (i, e) in f.all("IFCOPENINGELEMENT").chain(f.all("IFCDOOR")) {
        kennung.insert(i, text(&e.args[0]).to_string());
    }
    let mut out: Mengen = HashMap::new();
    for (_, rel) in f.all("IFCRELDEFINESBYPROPERTIES") {
        let eq = f.get(liste(&rel.args[5])[0]).unwrap();
        if eq.name != "IFCELEMENTQUANTITY" {
            continue;
        }
        let satz = text(&eq.args[2]).to_string();
        for x in liste(&rel.args[4]) {
            let s = out
                .entry(kennung[&x].clone())
                .or_default()
                .entry(satz.clone())
                .or_default();
            for q in liste(&eq.args[5]) {
                let q = f.get(q).unwrap();
                assert!(q.name.starts_with("IFCQUANTITY"), "{name}: {}", q.name);
                let v = match &q.args[3] {
                    V::Real(v) => *v,
                    V::Int(i) => *i as f64,
                    v => panic!("{name}: Wert {v:?}"),
                };
                let n = text(&q.args[0]).to_string();
                assert!(
                    s.insert(n.clone(), v).is_none(),
                    "{name}: {satz} {n} doppelt"
                );
            }
        }
    }
    out
}

/// I4: Die Mengen der Datei sind die der Mengenliste. Jede Zeile mit Menge
/// hat ihre Sätze; je Gruppe des Mengenfensters ergeben Länge, Fläche,
/// Rauminhalt und Auflager aus der Datei die Summen der Liste.
#[test]
fn mengen_wie_mengenliste() {
    use sk_model::qto::ElementQto as Q;
    let qto = "Qto_";
    for (name, t) in HAEUSER {
        let m = laden(t);
        let (ifc, _) = export(&m, &opt());
        let f = step::read(&ifc).unwrap();
        let werte = mengen_der_datei(name, &f);
        let liste = sk_model::qto::schedule(&m);
        let mut zeilen = 0;
        for st in liste.buildings.iter().flat_map(|b| &b.storeys) {
            for g in &st.groups {
                let mut summe = [0.0; 4];
                for z in &g.rows {
                    let Some(q) = &z.q else { continue };
                    zeilen += 1;
                    let guid = m.element(z.element).unwrap().guid.to_ifc();
                    let s = werte
                        .get(&guid)
                        .unwrap_or_else(|| panic!("{name}: {} ohne Mengen", z.number));
                    // Menge `n` aus dem Satz der Norm, sonst aus dem eigenen
                    let hol = |n: &str| -> f64 {
                        s.iter()
                            .filter(|(k, _)| k.starts_with(qto))
                            .chain(s.get_key_value("Skizzeo_Mengen"))
                            .find_map(|(_, v)| v.get(n).copied())
                            .unwrap_or(0.0)
                    };
                    let vol = hol("NetVolume").max(hol("Volumen"));
                    let (len, area) = match q {
                        Q::Wall(_) => (hol("Listenlänge"), 0.0),
                        Q::Footing(_) => (hol("Length"), 0.0),
                        Q::Strip(_) | Q::Coping(_) => (hol("Länge"), 0.0),
                        Q::Slab(_) | Q::Floor(_) | Q::Terrace(_) => (0.0, hol("GrossArea")),
                        Q::Soffit(_) | Q::Perimeter(_) => (0.0, hol("NetArea")),
                        Q::Ext(_) => (0.0, 0.0),
                    };
                    let pocket = hol("Deckenauflager") + hol("davon Auflager");
                    summe[0] += len;
                    summe[1] += area * 1e6;
                    summe[2] += vol * 1e9;
                    summe[3] += pocket * 1e9;
                    // auf 1e-9 m³ (1 mm³) geschrieben
                    let ab = (vol * 1e9 - q.volume()).abs();
                    assert!(ab <= 0.5 + 1e-6, "{name}: {} Volumen {vol} m³", z.number);
                }
                let t = g.total;
                for (i, (soll, was)) in [
                    (t.length, "Länge"),
                    (t.area, "Fläche"),
                    (t.volume, "Volumen"),
                    (t.pocket, "Auflager"),
                ]
                .into_iter()
                .enumerate()
                {
                    let ist = summe[i];
                    assert!(
                        (ist - soll).abs() <= (1e-9 * soll.abs()).max(g.rows.len() as f64),
                        "{name}: Gruppe {:?} {was} {ist} statt {soll}",
                        g.category
                    );
                }
            }
        }
        assert!(zeilen > 10, "{name}: {zeilen} Zeilen");
    }
}

/// I4: Kostengruppen mit dem Wortlaut der DIN 276, Gewerke als
/// Leistungsbereich der VOB/C; im Prüfhaus mit allen Abschnitten hat jedes
/// Bauteil beides.
#[test]
fn klassifikation_din276_und_vob() {
    let m = laden(HAEUSER[5].1);
    let (ifc, b) = export(&m, &opt());
    let f = step::read(&ifc).unwrap();
    let mut je: HashMap<u32, Vec<String>> = HashMap::new();
    for (_, rel) in f.all("IFCRELASSOCIATESCLASSIFICATION") {
        let c = f.get(liste(&rel.args[5])[0]).unwrap();
        let quelle = f.get(liste(&c.args[3])[0]).unwrap();
        let quelle = text(&quelle.args[3]).to_string();
        let (id, name) = (text(&c.args[1]), text(&c.args[2]));
        match quelle.as_str() {
            "DIN 276" => assert_eq!(Some(name), sk_model::din276::name(id.parse().unwrap())),
            "VOB/C" => {
                assert!(id.starts_with("DIN 18") && id.len() == 9, "{id}");
                assert!(!name.is_empty(), "{id}");
            }
            // Nutzungsgruppe der Räume (I6)
            "DIN 277" => {
                assert!(id.starts_with("NUF ") || id == "TF" || id == "VF", "{id}");
                assert!(!name.is_empty(), "{id}");
            }
            q => panic!("Klassifikation {q}"),
        }
        for x in liste(&rel.args[4]) {
            je.entry(x).or_default().push(quelle.clone());
        }
    }
    let teile = bauteile(&f);
    assert_eq!(teile.len(), b.bauteile + b.raeume + b.bekleidungen);
    for (g, (i, klasse)) in teile {
        let mut q = je.remove(&i).unwrap_or_default();
        q.sort();
        let soll: &[&str] = match klasse.as_str() {
            "IFCSPACE" => &["DIN 277"],
            _ => &["DIN 276", "VOB/C"],
        };
        assert_eq!(q, soll, "{g} {klasse}");
    }
}

/// I4 in der Vorschau: Mengen mit der Einheit der Datei, Klassifikationen
/// mit ihrem Namen.
#[test]
fn vorschau_zeigt_mengen_und_klassen() {
    let m = laden(HAEUSER[0].1);
    let (ifc, _) = export(&m, &opt());
    let v = sk_ifc::lesen::vorschau(&step::read(&ifc).unwrap());
    let aw = v.teile.iter().find(|t| t.name == "AW-004").unwrap();
    let qto = &aw
        .saetze
        .iter()
        .find(|s| s.0 == "Qto_WallBaseQuantities")
        .unwrap()
        .1;
    let wert = |n: &str| qto.iter().find(|x| x.0 == n).map(|x| x.1.clone());
    assert!(wert("Length").unwrap().ends_with(" mm"), "{qto:?}");
    assert!(wert("GrossSideArea").unwrap().ends_with(" m²"));
    assert!(wert("NetVolume").unwrap().ends_with(" m³"));
    assert!(wert("NetWeight").is_none_or(|w| w.ends_with(" kg")));
    assert!(
        aw.klassen
            .contains(&"DIN 276 · 331 Tragende Außenwände".to_string()),
        "{:?}",
        aw.klassen
    );
    assert!(
        aw.klassen.iter().any(|k| k.starts_with("VOB/C · DIN 183")),
        "{:?}",
        aw.klassen
    );
}

fn zahl(v: &V) -> f64 {
    match v {
        V::Real(x) => *x,
        V::Int(i) => *i as f64,
        _ => f64::NAN,
    }
}

/// I4c (ifc/plan.md §10): ohne Landeskoordinaten keine Georeferenz; mit
/// ihnen zeigt geografisch Nord des Modells nach der `IfcMapConversion`
/// dorthin, wo die UTM-Rechnung den Punkt 100 m weiter nördlich hat.
#[test]
fn georeferenz_aus_landeskoordinaten() {
    let mut m = laden(HAEUSER[0].1);
    let (ifc, _) = export(&m, &opt());
    let f = step::read(&ifc).unwrap();
    assert_eq!(f.all("IFCMAPCONVERSION").count(), 0);
    assert_eq!(f.all("IFCPROJECTEDCRS").count(), 0);

    let (e0, n0) = (467_123.456, 5_878_765.432);
    for (north, nhn) in [(12.0, Some(14.25)), (300.0, None)] {
        assert!(m.set_location(sk_model::Location {
            north: Some(north),
            utm: Some(sk_model::Utm {
                e: e0,
                n: n0,
                zone: 32,
            }),
            nhn,
            ..Default::default()
        }));
        let (ifc, _) = export(&m, &opt());
        let f = step::read(&ifc).unwrap();
        let mc: Vec<_> = f.all("IFCMAPCONVERSION").collect();
        assert_eq!(mc.len(), 1);
        let a = &mc[0].1.args;
        let ctx = f.get(liste(&a[0])[0]).unwrap();
        assert_eq!(ctx.name, "IFCGEOMETRICREPRESENTATIONCONTEXT");
        let crs = f.get(liste(&a[1])[0]).unwrap();
        assert_eq!(crs.name, "IFCPROJECTEDCRS");
        assert_eq!(text(&crs.args[0]), "EPSG:25832");
        assert_eq!(text(&crs.args[5]), "32N");
        assert_eq!(
            text(&crs.args[3]),
            if nhn.is_some() { "DHHN2016" } else { "" }
        );
        let einheit = f.get(liste(&crs.args[6])[0]).unwrap();
        assert_eq!(einheit.args[2], V::Null, "Meter ohne Vorsatz");
        assert_eq!(zahl(&a[2]), e0);
        assert_eq!(zahl(&a[3]), n0);
        assert_eq!(zahl(&a[4]), nhn.unwrap_or(0.0));
        assert_eq!(zahl(&a[7]), 0.001);
        let (xa, xo) = (zahl(&a[5]), zahl(&a[6]));
        assert!((xa.hypot(xo) - 1.0).abs() < 1e-9);

        // geografisch Nord im Modell, 100 m, ins Gitter
        let al = north.to_radians();
        let (x, y) = (1e5 * al.sin(), 1e5 * al.cos());
        let ost = 0.001 * (xa * x - xo * y);
        let nord = 0.001 * (xo * x + xa * y);
        // derselbe Weg auf dem Ellipsoid: 100 m auf dem Meridian
        let (b, l) = sk_math::utm::von_utm(e0, n0, 32);
        let (a_, f_) = (6_378_137.0, 1.0 / 298.257_222_101);
        let e2: f64 = f_ * (2.0 - f_);
        let mr = a_ * (1.0 - e2) / (1.0 - e2 * b.to_radians().sin().powi(2)).powf(1.5);
        let (e1, n1) = sk_math::utm::nach_utm(b + (100.0 / mr).to_degrees(), l, 32);
        let soll = (e1 - e0).atan2(n1 - n0);
        let ist = ost.atan2(nord);
        assert!(
            (soll - ist).abs() < 1e-7,
            "Gitterrichtung {ist} statt {soll}"
        );
        assert!(
            (soll.to_degrees() + sk_math::utm::konvergenz(b, l, 32)).abs() < 1e-4,
            "Nord liegt im Gitter um −γ"
        );

        let site = f.all("IFCSITE").next().unwrap().1;
        match nhn {
            Some(h) => assert_eq!(zahl(&site.args[11]), h * 1000.0),
            None => assert_eq!(site.args[11], V::Null),
        }
    }
}

/// I4: Ein Wandzug mit mehr als einer Wand ist eine Baugruppe mit seiner
/// Kennung, die Wände sind ihre Teile, in der Folge der Segmente.
#[test]
fn wandzug_als_baugruppe() {
    for (name, t) in HAEUSER {
        let m = laden(t);
        let (ifc, b) = export(&m, &opt());
        let f = step::read(&ifc).unwrap();
        let kennung = |i: u32| text(&f.get(i).unwrap().args[0]).to_string();
        let mut teile: HashMap<String, Vec<String>> = HashMap::new();
        for (_, e) in f.all("IFCRELAGGREGATES") {
            let ganz = liste(&e.args[4])[0];
            if f.get(ganz).unwrap().name == "IFCELEMENTASSEMBLY" {
                let t = liste(&e.args[5]).into_iter().map(kennung).collect();
                teile.insert(kennung(ganz), t);
            }
        }
        let solids: HashSet<_> = m.element_solids().into_iter().map(|x| x.0).collect();
        let mut soll = 0;
        for (_, w) in m.runs().iter() {
            let waende: Vec<String> = w
                .segments
                .iter()
                .filter(|e| solids.contains(e))
                .map(|e| m.element(*e).unwrap().guid.to_ifc())
                .collect();
            if waende.len() < 2 {
                assert!(!teile.contains_key(&w.guid.to_ifc()), "{name}");
                continue;
            }
            soll += 1;
            assert_eq!(teile.get(&w.guid.to_ifc()), Some(&waende), "{name}");
        }
        assert!(soll > 0, "{name}");
        assert_eq!(b.baugruppen, soll, "{name}");
    }
}

/// Review #90 H2: Zwei Gewerke mit derselben ATV-Nummer und verschiedenen
/// Namen ergeben eine Beziehung mit einer Kennung, nicht zwei gleiche.
#[test]
fn atv_nummer_doppelt_eine_beziehung() {
    let t = HAEUSER[0].1.replace(
        r#"code="18338" name="Dachdeckungs- und Dachabdichtungsarbeiten""#,
        r#"code="18330" name="Mauerarbeiten (Dach)""#,
    );
    assert_ne!(t, HAEUSER[0].1);
    let (ifc, _) = export(&laden(&t), &opt());
    let f = step::read(&ifc).unwrap();
    let mut je_nummer: HashMap<String, usize> = HashMap::new();
    let mut kennungen = HashSet::new();
    for (_, e) in f.all("IFCRELASSOCIATESCLASSIFICATION") {
        let n = text(&e.args[2]).to_string();
        if n.starts_with("VOB/C") {
            *je_nummer.entry(n).or_default() += 1;
            assert!(kennungen.insert(text(&e.args[0]).to_string()));
        }
    }
    assert_eq!(je_nummer.get("VOB/C DIN 18330"), Some(&1), "{je_nummer:?}");
}
